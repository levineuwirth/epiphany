//! The canonical reduction (Chapter 6 §"The Canonical Reduction").
//!
//! The materialized score state is the deterministic reduction of the operation
//! set. This module is the determinism heart of the architecture:
//!
//! * [`canonical_reduction_order`] is the **single function** that orders
//!   operations (Chapter 6 §6.3.3). The order is causal-first, then by the HLC
//!   tuple `(physical, logical, replica, counter)`. A deterministic topological
//!   pass enforces causal precedence even for an accepted remote envelope whose
//!   HLC contradicts its causal context; HLC orders only ready operations.
//! * [`reduce_operation_set`] walks that order and produces a
//!   [`MaterializedState`] whose [`canonical_bytes`](MaterializedState::canonical_bytes)
//!   are **byte-identical across any permutation of the input** (Appendix D
//!   §"Canonical score determinism"; v0 acceptance criteria 1 and 5).
//!
//! The reduction handles, deterministically: equivocated-slot and
//! anomalous-segment exclusion, the missing-causal-predecessor rule (dependents
//! held pending), atomic transactions with the descriptor-precedence rule,
//! tombstones with re-anchoring, conflict generation with content-derived ids,
//! LWW-advisory fields, and forward (compensating) undo.
//!
//! ## Prototype scope
//!
//! The per-kind reduction implements the representative operations of §6.10
//! against the canonical bookkeeping Chapter 6 owns. [`OperationSet::reduce_onto`]
//! additionally seeds that state from and materializes it into Agent B's
//! [`Score`]. Rich values absent from the provisional operation payloads remain
//! deferred to the Operation Catalog (§6.11); see `DECISIONS.md` for the exact
//! boundary.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use epiphany_core::{
    canonical_pitch_bytes, derive_promoted_voice_id, AnchorOffset, AnnotationAnchor,
    CanonicalValue, Event, EventDuration, EventId, EventPosition, GestureAnchoring, InstrumentId,
    MeterChange, MetricGrid, MusicalDuration, MusicalPosition, OperationId, Pitch, PitchId,
    PitchSpelling, RationalTime, RegionEdge, RegionId, RegionTimeModel, ReplicaId, Score,
    ScoreMetadata, SpellingAttachment, SpellingDirective, SpellingScope, SpellingSource, Staff,
    StaffId, StaffInstance, StaffInstanceId, StaffLineConfiguration, TempoMap, TempoSegment,
    TempoShape, TimeAnchor, TimeSignature, TimeSignatureId, TransactionId, TypedObjectId, Voice,
    VoiceId, VoiceOrigin,
};
use epiphany_determinism::CanonicalEncode;

use crate::anomaly::{detect_replica_anomalies, IntegrityAnomaly, IntegrityAnomalyKind};
use crate::conflict::{
    ConflictKind, ConflictRecord, ConflictRegistry, FieldPath, ResolutionAction,
};
use crate::effect::{
    NoOpReason, OperationEffect, PreconditionFailureReason, ReanchorReason, RepairKind,
    RepairRecord, TupletCompensationKind,
};
use crate::encode::{push_canon, push_len, push_lp_bytes, push_u8_bool};
use crate::envelope::OperationEnvelope;
use crate::opset::OperationSet;
use crate::payload::{
    resolved_anchor_position, CreateCrossCuttingOp, CreateRegionOp, CreateRepeatStructureOp,
    CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp, CrossCuttingValue, DeleteCrossCuttingOp,
    DeleteEventOp, DeleteIdentifiedPitchOp, DeleteRegionOp, DeleteRepeatStructureOp,
    DeleteStaffInstanceOp, DeleteVoiceOp, InsertEventOp, InsertIdentifiedPitchOp,
    ModifyCrossCuttingOp, ModifyEventOp, ModifyIdentifiedPitchOp, OperationKind, OperationPayload,
    RespellPitchOp, SetMetadataOp, SetMetricGridOp, SetStaffLayoutOp, SetTempoSegmentOp,
    SetTimeSignatureOp, SetUserPageBreakOp, TransposeOp, TupletCompensation,
};
use crate::stamp::StampTuple;
use crate::support::{ObjectKind, SerializedCanonicalInputs};
use crate::undo::{UndoPolicy, UndoTransactionPayload};

/// Orders operation envelopes into the canonical reduction order (Chapter 6
/// §6.3.3): causal-first, then by the HLC tuple `(physical, logical, replica,
/// counter)`. Returns the envelopes in that order.
///
/// This is the single ordering function the determinism property tests against.
/// It performs deterministic Kahn topological ordering over causal-context
/// coverage, choosing the smallest HLC reduction tuple among ready operations.
/// A malformed causal cycle has no valid topological order; the smallest HLC
/// tuple deterministically breaks the cycle so every replica still converges.
///
/// ## Subquadratic construction (worklist F1 → K fix)
///
/// The order is *defined* pairwise: an edge `p → s` exists iff `p != s` and
/// `s.causal_context.covers(p.id)`; an envelope is *ready* when every present
/// covered predecessor has been emitted; the smallest reduction tuple among
/// ready envelopes emits next (slice position breaks the — duplicate-id-only —
/// tuple ties, matching `min_by_key`'s first-minimum rule, which the retained
/// test-only `canonical_reduction_order_reference` oracle implements literally
/// in O(n²)). Materializing the edges is inherently quadratic for the common
/// chain-context shape (every DVV floor covers the full replica prefix), so
/// this implementation never enumerates pairs. It decomposes each context into
/// *requirement terms* whose conjunction is exactly pairwise readiness:
///
/// * A **vector floor** `(r, n)` covers precisely the present envelopes of
///   replica `r` with counter `<= n` (the zero-based DVV floor, P11-C7) —
///   a *prefix* of the replica's lane sorted by `(counter, slice index)`.
///   The term is satisfied when the lane's emission frontier (the first
///   unemitted lane slot) passes the prefix; a floor that covers the
///   envelope's *own* id exempts only the self-pair, so that term is instead
///   "frontier at own slot and *second* frontier past the prefix". Both
///   frontiers are monotone, so each term is woken exactly once from a
///   `BTreeMap` keyed by the threshold slot.
/// * An **explicit dot** covers precisely the present envelopes bearing that
///   id; the term is satisfied when the id's unemitted multiplicity reaches
///   zero (or one, for a dot naming the envelope's own id). A dot also lying
///   under one of the context's own floors yields a (redundant) second term —
///   harmless, because readiness is the conjunction of terms, not a
///   predecessor count, so no per-pair dedup is needed.
///
/// Context entries covering no present envelope (absent replicas, floors below
/// every present counter, absent dots) yield no term, exactly as they yield no
/// edge pairwise. Total work is `O((n + Σ context entries) · log n)`.
pub fn canonical_reduction_order<'a>(
    envelopes: &[&'a OperationEnvelope],
) -> Vec<&'a OperationEnvelope> {
    let len = envelopes.len();
    let keys: Vec<StampTuple> = envelopes
        .iter()
        .map(|env| env.stamp.reduction_tuple())
        .collect();

    // Static indexes over the present set: per-replica lanes sorted by
    // (counter, slice index), each envelope's (lane, slot) position, and the
    // per-id unemitted multiplicity (> 1 only for duplicate ids, e.g.
    // equivocation twins fed to this function directly).
    let mut lane_of: BTreeMap<ReplicaId, usize> = BTreeMap::new();
    let mut lanes: Vec<OrderLane> = Vec::new();
    let mut position = vec![(0usize, 0usize); len];
    {
        let mut sorted: Vec<(ReplicaId, u64, usize)> = envelopes
            .iter()
            .enumerate()
            .map(|(index, env)| (env.id.replica, env.id.counter, index))
            .collect();
        sorted.sort_unstable();
        for (replica, counter, index) in sorted {
            let lane_index = *lane_of.entry(replica).or_insert_with(|| {
                lanes.push(OrderLane::default());
                lanes.len() - 1
            });
            let lane = &mut lanes[lane_index];
            position[index] = (lane_index, lane.slots.len());
            lane.slots.push(index);
            lane.counters.push(counter);
        }
        for lane in &mut lanes {
            // All slots start unemitted: frontier 0, second frontier 1
            // (clamped to the lane length, the "exhausted" sentinel).
            lane.second = 1.min(lane.slots.len());
        }
    }
    let mut id_slots: BTreeMap<OperationId, IdSlot> = BTreeMap::new();
    for env in envelopes {
        id_slots.entry(env.id).or_default().unemitted += 1;
    }

    // One requirement term per covering context entry; `remaining` counts the
    // currently-unsatisfied terms. Terms already satisfied here (they cover
    // nothing, or nothing beyond the envelope itself) register no watcher.
    let mut remaining = vec![0usize; len];
    for (index, env) in envelopes.iter().enumerate() {
        for (&replica, &floor) in &env.causal_context.vector {
            let Some(&lane_index) = lane_of.get(&replica) else {
                continue;
            };
            let lane = &mut lanes[lane_index];
            let prefix = lane.counters.partition_point(|&counter| counter <= floor);
            if prefix == 0 {
                continue;
            }
            if env.id.replica == replica && env.id.counter <= floor {
                // The floor covers this envelope's own id; only the self-pair
                // is exempt. Required: every other prefix slot emitted, i.e.
                // frontier at the own slot *and* second frontier past the
                // prefix (the own slot stays unemitted until emission).
                let own_slot = position[index].1;
                if lane.frontier < own_slot {
                    remaining[index] += 1;
                    lane.frontier_watchers.entry(own_slot).or_default().push(
                        FloorWatcher::ExceptSelf {
                            node: index,
                            prefix,
                        },
                    );
                } else if lane.second < prefix {
                    remaining[index] += 1;
                    lane.second_watchers.entry(prefix).or_default().push(index);
                }
            } else if lane.frontier < prefix {
                remaining[index] += 1;
                lane.frontier_watchers
                    .entry(prefix)
                    .or_default()
                    .push(FloorWatcher::Whole { node: index });
            }
        }
        for dot in env.causal_context.dots() {
            let Some(id_slot) = id_slots.get_mut(&dot) else {
                continue; // absent id: covers nothing present, no edge
            };
            if dot == env.id {
                // A dot naming the envelope's own id covers only duplicates.
                if id_slot.unemitted > 1 {
                    remaining[index] += 1;
                    id_slot.watch_one.push(index);
                }
            } else {
                remaining[index] += 1;
                id_slot.watch_zero.push(index);
            }
        }
    }

    // Deterministic Kahn walk. The heap holds every envelope whose terms are
    // all satisfied (pushed exactly at the transition; entries for envelopes
    // already emitted through cycle-breaking are skipped lazily), keyed by
    // (reduction tuple, slice index) — the reference's `min_by_key` order.
    let mut heap: BinaryHeap<Reverse<(StampTuple, usize)>> = (0..len)
        .filter(|&index| remaining[index] == 0)
        .map(|index| Reverse((keys[index], index)))
        .collect();
    let mut by_key: Vec<usize> = (0..len).collect();
    by_key.sort_unstable_by_key(|&index| (keys[index], index));
    let mut cycle_cursor = 0usize;

    let mut emitted = vec![false; len];
    let mut ordered = Vec::with_capacity(len);
    let mut woken: Vec<usize> = Vec::new();
    while ordered.len() < len {
        let mut ready = None;
        while let Some(Reverse((_, index))) = heap.pop() {
            if !emitted[index] {
                ready = Some(index);
                break;
            }
        }
        let next = match ready {
            Some(index) => index,
            None => {
                // A cycle is malformed, but selecting by the canonical
                // tie-breaker keeps reduction deterministic and unlocks the
                // forced envelope's dependents.
                while emitted[by_key[cycle_cursor]] {
                    cycle_cursor += 1;
                }
                by_key[cycle_cursor]
            }
        };
        emitted[next] = true;
        ordered.push(envelopes[next]);

        // Dot wake-ups: the id's unemitted multiplicity dropped by one.
        let id_slot = id_slots
            .get_mut(&envelopes[next].id)
            .expect("every present id has a slot");
        id_slot.unemitted -= 1;
        if id_slot.unemitted <= 1 {
            woken.append(&mut id_slot.watch_one);
        }
        if id_slot.unemitted == 0 {
            woken.append(&mut id_slot.watch_zero);
        }

        // Floor wake-ups: advance the lane frontiers (both point at unemitted
        // slots — or the lane length — by invariant, so an emission below the
        // second frontier is at one of them) and drain the passed watchers.
        let (lane_index, slot) = position[next];
        let lane = &mut lanes[lane_index];
        if slot == lane.frontier {
            lane.frontier = lane.second;
            lane.second = next_unemitted(&lane.slots, &emitted, lane.frontier + 1);
        } else if slot == lane.second {
            lane.second = next_unemitted(&lane.slots, &emitted, slot + 1);
        }
        while let Some(entry) = lane.frontier_watchers.first_entry() {
            if *entry.key() > lane.frontier {
                break;
            }
            for watcher in entry.remove() {
                match watcher {
                    FloorWatcher::Whole { node } => woken.push(node),
                    FloorWatcher::ExceptSelf { node, prefix } => {
                        if lane.second >= prefix {
                            woken.push(node);
                        } else {
                            lane.second_watchers.entry(prefix).or_default().push(node);
                        }
                    }
                }
            }
        }
        while let Some(entry) = lane.second_watchers.first_entry() {
            if *entry.key() > lane.second {
                break;
            }
            for node in entry.remove() {
                woken.push(node);
            }
        }

        for node in woken.drain(..) {
            remaining[node] -= 1;
            if remaining[node] == 0 && !emitted[node] {
                heap.push(Reverse((keys[node], node)));
            }
        }
    }
    ordered
}

/// One replica's present envelopes in [`canonical_reduction_order`], sorted by
/// `(counter, slice index)`, with the two monotone emission frontiers and the
/// floor-term watchers keyed by the frontier slot they wait for.
#[derive(Default)]
struct OrderLane {
    /// Envelope slice indexes, sorted by `(counter, slice index)`.
    slots: Vec<usize>,
    /// The slots' counters (parallel to `slots`, ascending).
    counters: Vec<u64>,
    /// First unemitted slot (== `slots.len()` once exhausted).
    frontier: usize,
    /// Second unemitted slot (>= `slots.len()` once fewer than two remain).
    second: usize,
    /// Floor terms waiting for `frontier >= key`.
    frontier_watchers: BTreeMap<usize, Vec<FloorWatcher>>,
    /// Self-exempt floor terms waiting for `second >= key`.
    second_watchers: BTreeMap<usize, Vec<usize>>,
}

/// One present `OperationId`'s bookkeeping in [`canonical_reduction_order`]:
/// its unemitted multiplicity and the dot terms watching it.
#[derive(Default)]
struct IdSlot {
    /// Present envelopes bearing this id that are not yet emitted.
    unemitted: usize,
    /// Dot terms satisfied when `unemitted` reaches zero.
    watch_zero: Vec<usize>,
    /// Self-dot terms (duplicate ids) satisfied when `unemitted` reaches one.
    watch_one: Vec<usize>,
}

/// A floor term parked in [`OrderLane::frontier_watchers`].
enum FloorWatcher {
    /// Satisfied outright when the frontier reaches its key (the prefix end).
    Whole { node: usize },
    /// A floor covering the node's own id: when the frontier reaches the
    /// node's own slot (its key), the term is satisfied if the second
    /// frontier already passed `prefix`, else it re-parks on the second
    /// frontier.
    ExceptSelf { node: usize, prefix: usize },
}

/// The first unemitted slot position at or after `from` (== `slots.len()` when
/// exhausted). Frontier scans only ever move forward, so the total scan work
/// per lane is linear.
fn next_unemitted(slots: &[usize], emitted: &[bool], from: usize) -> usize {
    let mut at = from.min(slots.len());
    while at < slots.len() && emitted[slots[at]] {
        at += 1;
    }
    at
}

/// The pre-F1 O(n²) implementation, retained verbatim as the property-test
/// oracle for [`canonical_reduction_order`]: it materializes every covered
/// `(predecessor, successor)` pair and re-scans the whole set per emission,
/// which *is* the order's pairwise definition, executed literally.
#[cfg(test)]
pub(crate) fn canonical_reduction_order_reference<'a>(
    envelopes: &[&'a OperationEnvelope],
) -> Vec<&'a OperationEnvelope> {
    let len = envelopes.len();
    let mut indegree = vec![0usize; len];
    let mut successors = vec![Vec::<usize>::new(); len];
    for predecessor in 0..len {
        for successor in 0..len {
            if predecessor != successor
                && envelopes[successor]
                    .causal_context
                    .covers(envelopes[predecessor].id)
            {
                successors[predecessor].push(successor);
                indegree[successor] += 1;
            }
        }
    }

    let mut emitted = vec![false; len];
    let mut ordered = Vec::with_capacity(len);
    for _ in 0..len {
        let ready = (0..len)
            .filter(|&index| !emitted[index] && indegree[index] == 0)
            .min_by_key(|&index| envelopes[index].stamp.reduction_tuple());
        let next = ready.unwrap_or_else(|| {
            // A cycle is malformed, but selecting by the canonical tie-breaker
            // keeps reduction deterministic and unlocks its outgoing edges.
            (0..len)
                .filter(|&index| !emitted[index])
                .min_by_key(|&index| envelopes[index].stamp.reduction_tuple())
                .expect("an un-emitted operation remains")
        });
        emitted[next] = true;
        ordered.push(envelopes[next]);
        for &successor in &successors[next] {
            indegree[successor] = indegree[successor].saturating_sub(1);
        }
    }
    ordered
}

/// The state of an object's identifier in the materialized graph (Chapter 6
/// §"Object Existence and Tombstones"). `Unknown` is not represented: an
/// identifier the reduction has never seen simply has no entry.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ObjectState {
    /// Resolves to a current object.
    Live,
    /// Resolves to a deletion record carrying the deleting and minting
    /// operations. Tombstones are never resurrected except by an explicit
    /// inverse-of-delete (Chapter 6 §6.3.4).
    Tombstoned {
        deleted_by: OperationId,
        minted_by: OperationId,
    },
}

impl ObjectState {
    fn discriminant(&self) -> u8 {
        match self {
            ObjectState::Live => 0,
            ObjectState::Tombstoned { .. } => 1,
        }
    }
}

impl CanonicalEncode for ObjectState {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.push(self.discriminant());
        if let ObjectState::Tombstoned {
            deleted_by,
            minted_by,
        } = self
        {
            push_canon(out, deleted_by);
            push_canon(out, minted_by);
        }
    }
}

/// Why an operation is held pending rather than reduced (Chapter 6 §6.5, §6.6).
/// A pending operation is retained in the operation set but produces no
/// canonical effect until its blocking cause is resolved.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PendingReason {
    /// A causal predecessor is absent from the operation set entirely.
    MissingCausalPredecessor { missing: OperationId },
    /// A causal predecessor's slot is equivocated.
    DependsOnEquivocated { on: OperationId },
    /// A causal predecessor was excluded by an anomalous-replica segment.
    DependsOnExcluded { on: OperationId },
    /// A causal predecessor is itself held pending.
    DependsOnPending { on: OperationId },
    /// Reduction halted at a system-derived identifier collision (Chapter 5
    /// §"System-Derived Counter Collisions"): this operation is at or past the
    /// collision point in canonical order — or is the earlier occupant of the
    /// collided counter — and is held pending external recovery. `at` is the
    /// operation whose mint collided.
    HaltedBySystemCollision { at: OperationId },
}

impl PendingReason {
    fn discriminant(&self) -> u8 {
        match self {
            PendingReason::MissingCausalPredecessor { .. } => 0,
            PendingReason::DependsOnEquivocated { .. } => 1,
            PendingReason::DependsOnExcluded { .. } => 2,
            PendingReason::DependsOnPending { .. } => 3,
            PendingReason::HaltedBySystemCollision { .. } => 4,
        }
    }
    fn blocker(&self) -> OperationId {
        match self {
            PendingReason::MissingCausalPredecessor { missing } => *missing,
            PendingReason::DependsOnEquivocated { on }
            | PendingReason::DependsOnExcluded { on }
            | PendingReason::DependsOnPending { on } => *on,
            PendingReason::HaltedBySystemCollision { at } => *at,
        }
    }
}

impl CanonicalEncode for PendingReason {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.push(self.discriminant());
        push_canon(out, &self.blocker());
    }
}

/// The canonical materialized state produced by the reduction (Chapter 6 §6.3).
/// Every field is in a normative total order, so [`MaterializedState::canonical_bytes`]
/// is byte-identical across any permutation of the input operation set.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MaterializedState {
    /// The effect log: one [`OperationEffect`] per reduced operation, in
    /// canonical reduction order (Chapter 6 §6.3.2). Part of canonical state.
    pub effects: Vec<(OperationId, OperationEffect)>,
    /// The conflict registry, ordered by `ConflictId`.
    pub conflicts: ConflictRegistry,
    /// The integrity-anomaly register, ordered by `IntegrityAnomalyId`.
    pub anomalies: Vec<IntegrityAnomaly>,
    /// Object existence (live/tombstoned), keyed by `TypedObjectId`.
    pub objects: BTreeMap<TypedObjectId, ObjectState>,
    /// Current resolved spelling per pitch (the `RespellPitch` field).
    pub spellings: BTreeMap<PitchId, PitchSpelling>,
    /// User system-break preferences (LWW advisory), keyed by region+anchor.
    pub breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    /// User page-break preferences (LWW advisory), keyed by region+anchor (the
    /// page-break sibling of [`MaterializedState::breaks`], M2d).
    pub page_breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    /// Operations held pending, ordered by `OperationId`.
    pub pending: Vec<(OperationId, PendingReason)>,
}

/// The result of reducing an operation set onto a canonical base score.
///
/// `state` remains the byte-canonical Chapter 6 reduction product. `score` is
/// the corresponding Agent B graph materialization used by editing, invariant
/// checking, indexing, and layout; it is derived state, never the source of
/// truth.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GraphMaterialization {
    pub state: MaterializedState,
    pub score: Score,
}

impl MaterializedState {
    /// The canonical byte serialization of the materialized state. Two
    /// reductions of the same operation set — in any order — produce identical
    /// bytes (the determinism property; v0 acceptance criteria 1 and 5).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        // Effects, in reduction order.
        push_len(&mut out, self.effects.len());
        for (id, eff) in &self.effects {
            push_canon(&mut out, id);
            let mut scratch = Vec::new();
            eff.encode_canonical(&mut scratch);
            crate::encode::push_lp_bytes(&mut out, &scratch);
        }
        // Conflict registry (ConflictId order).
        self.conflicts.encode_canonical(&mut out);
        // Anomaly register (IntegrityAnomalyId order).
        push_len(&mut out, self.anomalies.len());
        for a in &self.anomalies {
            let mut scratch = Vec::new();
            a.encode_canonical(&mut scratch);
            crate::encode::push_lp_bytes(&mut out, &scratch);
        }
        // Objects (TypedObjectId order).
        push_len(&mut out, self.objects.len());
        for (id, state) in &self.objects {
            push_canon(&mut out, id);
            state.encode_canonical(&mut out);
        }
        // Spellings (PitchId order). The value is the full PitchSpelling (v1),
        // encoded behind a u32 length prefix via its canonical value bytes.
        push_len(&mut out, self.spellings.len());
        for (pitch, spelling) in &self.spellings {
            push_canon(&mut out, pitch);
            push_lp_bytes(&mut out, &spelling.canonical_bytes());
        }
        // Breaks (region+anchor order).
        push_len(&mut out, self.breaks.len());
        for ((region, anchor), present) in &self.breaks {
            push_canon(&mut out, region);
            push_canon(&mut out, anchor);
            push_u8_bool(&mut out, *present);
        }
        // Page breaks (region+anchor order) — sibling of breaks (M2d).
        push_len(&mut out, self.page_breaks.len());
        for ((region, anchor), present) in &self.page_breaks {
            push_canon(&mut out, region);
            push_canon(&mut out, anchor);
            push_u8_bool(&mut out, *present);
        }
        // Pending (OperationId order).
        push_len(&mut out, self.pending.len());
        for (id, reason) in &self.pending {
            push_canon(&mut out, id);
            reason.encode_canonical(&mut out);
        }
        out
    }

    /// Decodes the exact inverse of [`MaterializedState::canonical_bytes`].
    ///
    /// The decoder validates all nested tags, lengths, primitive values,
    /// canonical ordering, and trailing-byte discipline. Structurally valid but
    /// non-canonical bytes are rejected rather than silently normalized.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, crate::MaterializedDecodeError> {
        crate::decode::decode_materialized_state(bytes)
    }

    /// Whether the materialized state is free of conflicts and anomalies and
    /// has no pending operations — a clean reduction.
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.anomalies.is_empty() && self.pending.is_empty()
    }
}

/// Reduces an [`OperationSet`] to its canonical [`MaterializedState`].
pub fn reduce_operation_set(op_set: &OperationSet) -> MaterializedState {
    Reducer::new(op_set).run().0
}

/// Reduces an [`OperationSet`] onto a canonical base [`Score`].
pub fn reduce_operation_set_onto(op_set: &OperationSet, base: &Score) -> GraphMaterialization {
    let (state, score) = Reducer::new_onto(op_set, base).run();
    GraphMaterialization {
        state,
        score: score.expect("graph-aware reduction always retains its base score"),
    }
}

/// One write in a per-key canonical-order write chain (operation_catalog
/// §UndoTransaction, "Value restoration"): the writer, the transaction it was a
/// member of (if any), and the value it wrote.
#[derive(Clone, PartialEq, Eq, Debug)]
struct ChainWrite<V> {
    op: OperationId,
    tx: Option<TransactionId>,
    value: V,
}

/// The canonical-order write chain of one LWW-overwritten key (operation_catalog
/// §UndoTransaction). `base` is the key's pre-operational value — seeded from
/// the base graph (`seed_from_graph`) or from the value a mint carried — so a
/// chain-predecessor is defined for keys that exist before the first overwrite;
/// a key with no base entry restores to *absence*. `writes` append in canonical
/// processing order (the reducer applies operations in canonical order, so
/// appends are inherently canonical and the chain is permutation-invariant).
#[derive(Clone, PartialEq, Eq, Debug)]
struct WriteChain<V> {
    base: Option<V>,
    writes: Vec<ChainWrite<V>>,
}

impl<V: Clone> WriteChain<V> {
    fn new() -> Self {
        WriteChain {
            base: None,
            writes: Vec::new(),
        }
    }

    /// Seeds the base value only if the chain has neither a base nor writes
    /// (idempotent seeding; a mint never clobbers recorded history).
    fn seed(&mut self, base: V) {
        if self.base.is_none() && self.writes.is_empty() {
            self.base = Some(base);
        }
    }

    /// Appends a write in canonical processing order.
    fn record(&mut self, op: OperationId, tx: Option<TransactionId>, value: V) {
        self.writes.push(ChainWrite { op, tx, value });
    }

    /// The most recent write, if any (the LWW concurrency comparison point;
    /// the base entry never participates in conflict detection).
    fn last_write(&self) -> Option<&ChainWrite<V>> {
        self.writes.last()
    }

    /// The key's current resolved value: the last write, falling back to the
    /// base entry.
    fn current(&self) -> Option<&V> {
        self.last_write()
            .map(|write| &write.value)
            .or(self.base.as_ref())
    }

    /// The undo verdict for transaction `tx` on this key (operation_catalog
    /// §UndoTransaction, "Value restoration").
    fn undo_verdict(&self, tx: TransactionId) -> ChainUndoVerdict<V> {
        if !self.writes.iter().any(|w| w.tx == Some(tx)) {
            return ChainUndoVerdict::NotWritten;
        }
        let last = self.writes.last().expect("chain has a write by `tx`");
        if last.tx != Some(tx) {
            return ChainUndoVerdict::Superseded { by: last.op };
        }
        // Chain-predecessor: the latest entry not written by the transaction,
        // falling back to the base value, else absence.
        let predecessor = self
            .writes
            .iter()
            .rev()
            .find(|w| w.tx != Some(tx))
            .map(|w| Predecessor::Write(w.value.clone()))
            .or_else(|| self.base.clone().map(Predecessor::Base));
        ChainUndoVerdict::Restore(predecessor)
    }
}

/// The chain-predecessor an undo restores: a prior operational write, or the
/// key's base (pre-operational) value. The distinction matters for the
/// canonical bookkeeping maps (`spellings`, `breaks`, `page_breaks`), which
/// return to key-*absence* when the predecessor is the base value — the base
/// state lives in the graph, not in the operational ledger.
#[derive(Clone, Debug)]
enum Predecessor<V> {
    Write(V),
    Base(V),
}

impl<V> Predecessor<V> {
    fn into_value(self) -> V {
        match self {
            Predecessor::Write(v) | Predecessor::Base(v) => v,
        }
    }
}

/// The per-key outcome of undoing a transaction's write chain.
enum ChainUndoVerdict<V> {
    /// The transaction never wrote this key.
    NotWritten,
    /// The transaction wrote the key but a later write superseded it.
    Superseded { by: OperationId },
    /// The transaction's write is still last: restore the chain-predecessor
    /// (`None` = the transaction introduced the first value; restore absence).
    Restore(Option<Predecessor<V>>),
}

/// The staff-instance layout advisories `SetStaffLayout` overwrites as a unit
/// (operation_catalog §SetStaffLayout).
type StaffLayoutValue = (Option<InstrumentId>, Option<StaffLineConfiguration>, bool);

/// One key restoration a value-restoring undo applies (operation_catalog
/// §UndoTransaction "Value restoration"). For the always-valued families
/// (event, pitch, cross-cutting, metadata, staff layout) `None` means the
/// chain knows no predecessor (an object minted before this reduction's
/// horizon, reachable only base-free): the undo verdict stands but there is no
/// value to write back — a bookkeeping-only restoration. For the optional
/// families (grid, meter change, tempo segment) the flattened `None` *is* the
/// restoration: clear/remove at the key. The canonical bookkeeping families
/// (spelling, breaks) keep the [`Predecessor`] distinction: a base predecessor
/// returns the ledger map to key-absence while the graph restores the base
/// value.
enum ValueRestoration {
    Event {
        event: EventId,
        value: Option<Event>,
    },
    Pitch {
        pitch: PitchId,
        value: Option<Pitch>,
    },
    Spelling {
        pitch: PitchId,
        predecessor: Option<Predecessor<PitchSpelling>>,
    },
    CrossCutting {
        id: TypedObjectId,
        value: Option<CrossCuttingValue>,
    },
    Metadata {
        value: Option<ScoreMetadata>,
    },
    MetricGrid {
        region: RegionId,
        value: Option<MetricGrid>,
    },
    MeterChange {
        region: RegionId,
        position: MusicalPosition,
        value: Option<MeterChange>,
    },
    TempoSegment {
        region: Option<RegionId>,
        position: MusicalPosition,
        value: Option<TempoSegment>,
    },
    StaffLayout {
        instance: StaffInstanceId,
        value: Option<StaffLayoutValue>,
    },
    SystemBreak {
        region: RegionId,
        position: MusicalPosition,
        predecessor: Option<Predecessor<(TimeAnchor, bool)>>,
    },
    PageBreak {
        region: RegionId,
        position: MusicalPosition,
        predecessor: Option<Predecessor<(TimeAnchor, bool)>>,
    },
}

/// Whether a tempo segment's shape carries the end data it requires
/// (operation_catalog §"Meter and Tempo Overwrites"): a non-constant shape
/// needs both an explicit `end` and an `end_tempo`; a constant segment's
/// `end_tempo`, when present, must equal its `start_tempo` (the same structural
/// rules the graph-invariant checker applies to tempo maps).
fn tempo_segment_shape_well_formed(segment: &TempoSegment) -> bool {
    match segment.shape {
        TempoShape::Constant => segment
            .end_tempo
            .as_ref()
            .map_or(true, |end| end == &segment.start_tempo),
        TempoShape::Linear | TempoShape::Exponential | TempoShape::Curve => {
            segment.end_tempo.is_some() && segment.end.is_some()
        }
    }
}

/// Replaces (or removes, for `None`) the segment at `position` in `map`,
/// keeping the segment list ordered by resolved start position — the same
/// coarse anchor resolution the LWW key uses, so one resolved position holds
/// exactly one segment.
fn edit_tempo_map_segments(
    map: &mut TempoMap,
    position: &MusicalPosition,
    segment: &Option<TempoSegment>,
) {
    map.segments
        .retain(|existing| resolved_anchor_position(&existing.start) != *position);
    if let Some(segment) = segment {
        let index = map
            .segments
            .iter()
            .position(|existing| resolved_anchor_position(&existing.start) > *position)
            .unwrap_or(map.segments.len());
        map.segments.insert(index, segment.clone());
    }
}

/// The working state of one reduction pass.
struct Reducer<'a> {
    op_set: &'a OperationSet,
    // Canonical results.
    objects: BTreeMap<TypedObjectId, ObjectState>,
    spellings: BTreeMap<PitchId, PitchSpelling>,
    breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    page_breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    conflicts: ConflictRegistry,
    effects: Vec<(OperationId, OperationEffect)>,
    anomalies: BTreeMap<epiphany_core::IntegrityAnomalyId, IntegrityAnomaly>,
    // Transient indices.
    minted_by: BTreeMap<TypedObjectId, OperationId>,
    event_pitches: BTreeMap<EventId, Vec<PitchId>>,
    voice_occupancy: BTreeMap<VoiceId, Vec<(MusicalPosition, MusicalDuration, EventId)>>,
    // Per-key canonical-order write chains for every LWW overwrite family
    // (operation_catalog §UndoTransaction "Value restoration"). Each chain's
    // last write doubles as the LWW working state the concurrent-differing
    // conflict detection reads (formerly the `last_*` maps); the full chain is
    // what value-restoring undo walks. Advisory families (metadata, breaks,
    // staff layout) keep chains for undo but record no conflicts.
    respell_chain: BTreeMap<PitchId, WriteChain<PitchSpelling>>,
    event_modify_chain: BTreeMap<EventId, WriteChain<Event>>,
    pitch_modify_chain: BTreeMap<PitchId, WriteChain<Pitch>>,
    cross_cutting_modify_chain: BTreeMap<TypedObjectId, WriteChain<CrossCuttingValue>>,
    metric_grid_chain: BTreeMap<RegionId, WriteChain<Option<MetricGrid>>>,
    metadata_chain: WriteChain<ScoreMetadata>,
    break_chain: BTreeMap<(RegionId, MusicalPosition), WriteChain<(TimeAnchor, bool)>>,
    page_break_chain: BTreeMap<(RegionId, MusicalPosition), WriteChain<(TimeAnchor, bool)>>,
    // Meter/tempo overwrite chains (Phase-3 tranche): `Some` = a set/replace at
    // the key, `None` = an explicit removal. The meter value is the graph-level
    // `MeterChange` (anchor + signature id) so a restoration can re-install it.
    meter_change_chain: BTreeMap<(RegionId, MusicalPosition), WriteChain<Option<MeterChange>>>,
    tempo_segment_chain:
        BTreeMap<(Option<RegionId>, MusicalPosition), WriteChain<Option<TempoSegment>>>,
    staff_layout_chain: BTreeMap<StaffInstanceId, WriteChain<StaffLayoutValue>>,
    // Carried values of set-union-minted staves and time signatures, for the
    // byte-identical-re-carry idempotence check (operation_catalog §CreateStaff:
    // identical re-create is idempotent; a differing value under a live id is a
    // precondition no-op). Seeded from the base graph.
    staff_values: BTreeMap<StaffId, Staff>,
    time_signature_values: BTreeMap<TimeSignatureId, TimeSignature>,
    structures: BTreeMap<TypedObjectId, Vec<TypedObjectId>>,
    // Live child sets for the structural-container empty-only delete (Group 3):
    // a region's live staff instances, and a staff instance's live voices. (A
    // voice's live events are read from `voice_occupancy`.)
    region_instances: BTreeMap<RegionId, BTreeSet<StaffInstanceId>>,
    instance_voices: BTreeMap<StaffInstanceId, BTreeSet<VoiceId>>,
    // The staff each staff instance manifests, for the containment-proximity
    // key of the re-anchoring "nearest" ordering (same staff = rank 2). Kept
    // base-free (seeded + maintained by CreateStaffInstance) so reduce() and
    // reduce_onto() rank identically wherever both represent the scenario.
    instance_staff: BTreeMap<StaffInstanceId, StaffId>,
    // Regions whose content carries a staff-based slot (staff-based or hybrid),
    // and so can hold a metric grid or user break. FreeGraphic regions cannot.
    // Tracking this lets SetMetricGrid / SetUserPageBreak / SetUserSystemBreak
    // reach the same precondition verdict for any region *represented in reducer
    // state* (those an op stream creates/deletes) whether or not a graph is
    // present. A base-only region exists in reducer state solely after
    // `seed_from_graph` (reduce_onto), so a base-free reduce() that never sees it
    // can still diverge on a base-region target — the corpus targets only
    // op-created regions, where the two agree.
    staff_based_regions: BTreeSet<RegionId>,
    migrated_regions: BTreeSet<RegionId>,
    region_migrator: BTreeMap<RegionId, OperationId>,
    descriptors: BTreeMap<TransactionId, OperationId>,
    // Losing insert -> (promoted voice, winning insert).
    promotion: BTreeMap<OperationId, (VoiceId, OperationId)>,
    // System-derived mint registry for the counter-collision check (Chapter 5
    // §"System-Derived Counter Collisions"): (kind, derived counter) → the
    // canonical inputs that derived it, plus the minting operation (None for
    // an occupant seeded from the base graph). Consulted only by the pre-walk
    // collision detection, never mutated during apply, so it needs no
    // transaction snapshot.
    system_mints: BTreeMap<(ObjectKind, u64), (SerializedCanonicalInputs, Option<OperationId>)>,
    tx_minted: BTreeMap<TransactionId, Vec<TypedObjectId>>,
    current_tx: Option<TransactionId>,
    // ResolveEquivocation promotion results (operation_catalog
    // §"ResolveEquivocation"), computed by the set-level pre-pass in `run`:
    // target slot id → (governing resolve id, chosen candidate hash), and
    // target slot id → the promoted candidate envelope (from the opset's
    // diagnostic candidate store). Pure functions of the slot map — never
    // mutated during apply — so, like `system_mints`, they need no transaction
    // snapshot.
    equivocation_resolutions: BTreeMap<OperationId, (OperationId, crate::EnvelopeHash)>,
    promoted_singles: BTreeMap<OperationId, &'a OperationEnvelope>,
    graph: Option<Score>,
}

/// A snapshot of the working state, for atomic transaction rollback.
struct WorkingSnapshot {
    objects: BTreeMap<TypedObjectId, ObjectState>,
    spellings: BTreeMap<PitchId, PitchSpelling>,
    breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    page_breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    conflicts: ConflictRegistry,
    minted_by: BTreeMap<TypedObjectId, OperationId>,
    event_pitches: BTreeMap<EventId, Vec<PitchId>>,
    voice_occupancy: BTreeMap<VoiceId, Vec<(MusicalPosition, MusicalDuration, EventId)>>,
    respell_chain: BTreeMap<PitchId, WriteChain<PitchSpelling>>,
    event_modify_chain: BTreeMap<EventId, WriteChain<Event>>,
    pitch_modify_chain: BTreeMap<PitchId, WriteChain<Pitch>>,
    cross_cutting_modify_chain: BTreeMap<TypedObjectId, WriteChain<CrossCuttingValue>>,
    metric_grid_chain: BTreeMap<RegionId, WriteChain<Option<MetricGrid>>>,
    metadata_chain: WriteChain<ScoreMetadata>,
    break_chain: BTreeMap<(RegionId, MusicalPosition), WriteChain<(TimeAnchor, bool)>>,
    page_break_chain: BTreeMap<(RegionId, MusicalPosition), WriteChain<(TimeAnchor, bool)>>,
    meter_change_chain: BTreeMap<(RegionId, MusicalPosition), WriteChain<Option<MeterChange>>>,
    tempo_segment_chain:
        BTreeMap<(Option<RegionId>, MusicalPosition), WriteChain<Option<TempoSegment>>>,
    staff_layout_chain: BTreeMap<StaffInstanceId, WriteChain<StaffLayoutValue>>,
    staff_values: BTreeMap<StaffId, Staff>,
    time_signature_values: BTreeMap<TimeSignatureId, TimeSignature>,
    structures: BTreeMap<TypedObjectId, Vec<TypedObjectId>>,
    region_instances: BTreeMap<RegionId, BTreeSet<StaffInstanceId>>,
    instance_voices: BTreeMap<StaffInstanceId, BTreeSet<VoiceId>>,
    instance_staff: BTreeMap<StaffInstanceId, StaffId>,
    staff_based_regions: BTreeSet<RegionId>,
    migrated_regions: BTreeSet<RegionId>,
    region_migrator: BTreeMap<RegionId, OperationId>,
    descriptors: BTreeMap<TransactionId, OperationId>,
    tx_minted: BTreeMap<TransactionId, Vec<TypedObjectId>>,
    graph: Option<Score>,
}

/// The precondition no-op a structural create or delete returns when a container
/// is non-empty where the operation requires it empty (a create carrying children,
/// or a delete of a container with live children).
fn container_not_empty() -> OperationEffect {
    OperationEffect::NoOp {
        reason: NoOpReason::PreconditionFailedUnderReduction {
            reason: PreconditionFailureReason::ContainerNotEmpty,
        },
    }
}

/// Apply a user break to a region's break list under the canonical LWW key — the
/// anchor's resolved musical position. Any existing anchor resolving to that same
/// position is dropped first, so two anchors at one position never both persist,
/// then the new anchor is pushed iff the break is present. The graph break list
/// then matches the resolved-position-keyed ledger map.
fn apply_break_lww(breaks: &mut Vec<TimeAnchor>, anchor: &TimeAnchor, present: bool) {
    let resolved = crate::payload::resolved_anchor_position(anchor);
    breaks.retain(|existing| crate::payload::resolved_anchor_position(existing) != resolved);
    if present {
        breaks.push(anchor.clone());
    }
}

/// The 64-byte canonical input preimage of a promoted-voice derivation
/// (`MUSCSVCE`, Chapter 5 §"System-Promoted Voices"): staff_instance ‖
/// original_voice ‖ winning_op ‖ losing_op, 16 big-endian bytes each — exactly
/// the bytes [`derive_promoted_voice_id`] hashes. The collision check compares
/// these inputs to distinguish two derivations contending for one counter.
fn promoted_voice_inputs(
    staff_instance: StaffInstanceId,
    original_voice: VoiceId,
    winning_op: OperationId,
    losing_op: OperationId,
) -> Vec<u8> {
    let mut inputs = Vec::with_capacity(64);
    inputs.extend_from_slice(&staff_instance.canonical_bytes());
    inputs.extend_from_slice(&original_voice.canonical_bytes());
    inputs.extend_from_slice(&winning_op.canonical_bytes());
    inputs.extend_from_slice(&losing_op.canonical_bytes());
    inputs
}

fn intervals_overlap(
    a_position: &MusicalPosition,
    a_duration: &MusicalDuration,
    b_position: &MusicalPosition,
    b_duration: &MusicalDuration,
) -> bool {
    if !a_duration.is_positive() || !b_duration.is_positive() {
        return false;
    }
    let a_end = a_position.clone() + a_duration.clone();
    let b_end = b_position.clone() + b_duration.clone();
    a_position < &b_end && b_position < &a_end
}

fn insert_intervals_overlap(a: &InsertEventOp, b: &InsertEventOp) -> bool {
    intervals_overlap(
        &a.musical_position(),
        &a.musical_duration(),
        &b.musical_position(),
        &b.musical_duration(),
    )
}

pub(crate) fn graph_voice_location(score: &Score, voice: VoiceId) -> Option<(usize, usize, usize)> {
    for (region_index, region) in score.canvas.regions.iter().enumerate() {
        for (instance_index, instance) in region.staff_instances().iter().enumerate() {
            if let Some(voice_index) = instance
                .voices
                .iter()
                .position(|candidate| candidate.id == voice)
            {
                return Some((region_index, instance_index, voice_index));
            }
        }
    }
    None
}

/// The verdict on a [`ModifyEvent`](OperationKind::ModifyEvent)'s placement: whether
/// it moves the target event's metric span, and if so whether the move keeps
/// invariant 3 (`VoiceEventsSortedNonOverlap`). A non-metric or same-placement modify
/// is [`Unchanged`](PlacementVerdict::Unchanged) — handled by the existing field-edit
/// path.
enum PlacementVerdict {
    /// Same placement, or a non-metric event: nothing to materialize or refuse.
    Unchanged,
    /// A metric move that would overlap a sibling, or carries a non-positive span.
    Refused,
    /// A valid metric move to `position`/`duration` within `voice`.
    Moved {
        voice: VoiceId,
        position: MusicalPosition,
        duration: MusicalDuration,
    },
}

/// The graph value inserted by a value-typed InsertEvent: the carried [`Event`]
/// itself, with its voice rebound to the (possibly system-promoted) target
/// voice. The Operation Catalog (v1) carries the real event, so this is no
/// longer a placeholder reconstruction.
fn graph_event_from_insert(op: &InsertEventOp, target_voice: VoiceId) -> Event {
    let mut event = op.event.clone();
    event.set_voice(target_voice);
    event
}

// --- Re-anchoring referent support (Chapter 6 §"Total Ordering for Nearest",
// §"The Re-Anchoring Rule Table"). --------------------------------------------

/// The tombstoned referent's resolved placement and containment, captured from
/// the graph at the moment [`Reducer::materialize_graph_delete`] removes it —
/// the referent side of the four-key "nearest" ordering and of the range
/// reconstructions in the re-anchoring rule table.
struct ReferentContext {
    voice: VoiceId,
    region: Option<RegionId>,
    position: EventPosition,
    duration: EventDuration,
}

/// Proximity bound "same staff instance" (the rule table's declared maximum for
/// markers and graphic gestures): candidates ranked farther than the referent's
/// staff instance are excluded from "nearest".
const PROXIMITY_SAME_STAFF_INSTANCE: u8 = 1;

/// Maps an *established* containment-proximity rank (k1 of the "nearest"
/// ordering) to the ratified [`ReanchorReason`] vocabulary. Rank 4 (same
/// canvas) records the appended `SameCanvasNearer` (Pass 12, P12-C4; wire
/// discriminant 6 — `DeclaredByExtension` already owned 5). Callers with a
/// possibly-*unestablished* rank 4 — `containment_rank` also returns 4 when a
/// voice's placement is unresolvable, a selection-order tie the recording
/// must not launder into a positive proximity claim — route through
/// [`Reduction::rank_reason`], which downgrades those to `ExplicitFallback`.
fn reason_for_rank(rank: u8) -> ReanchorReason {
    match rank {
        0 => ReanchorReason::SameVoiceNearer,
        1 => ReanchorReason::SameStaffInstanceNearer,
        2 => ReanchorReason::SameStaffNearer,
        3 => ReanchorReason::SameRegionNearer,
        4 => ReanchorReason::SameCanvasNearer,
        _ => ReanchorReason::ExplicitFallback,
    }
}

/// The event references among a set of [`TimeAnchor`]s (the referent-index
/// entries a tombstone must repair). Non-event anchors contribute nothing.
fn anchor_event_refs<'a>(anchors: impl IntoIterator<Item = &'a TimeAnchor>) -> Vec<TypedObjectId> {
    anchors
        .into_iter()
        .filter_map(|anchor| match anchor {
            TimeAnchor::Event { id, .. } => Some(TypedObjectId::Event(*id)),
            _ => None,
        })
        .collect()
}

/// The event references an annotation anchor carries: its point event, or any
/// event-anchored range endpoints. Region anchors reference no event.
fn annotation_anchor_event_refs(anchor: &AnnotationAnchor) -> Vec<TypedObjectId> {
    match anchor {
        AnnotationAnchor::Event(event) => vec![TypedObjectId::Event(*event)],
        AnnotationAnchor::Range { start, end } => anchor_event_refs([start, end]),
        AnnotationAnchor::Region(_) => Vec::new(),
    }
}

/// The event references a gesture anchoring carries. `Free` anchoring follows
/// no score content and so never enters the referent index (table row: "for
/// Free anchoring, no action").
fn gesture_event_refs(anchoring: &GestureAnchoring) -> Vec<TypedObjectId> {
    match anchoring {
        GestureAnchoring::Events(events) => {
            events.iter().copied().map(TypedObjectId::Event).collect()
        }
        GestureAnchoring::Range { start, end, .. } => anchor_event_refs([start, end]),
        GestureAnchoring::Free => Vec::new(),
    }
}

/// Replaces a range endpoint anchored to the tombstoned event with the
/// containing region's edge — the deterministic "truncate" reading for
/// range-anchored referents (start endpoints move to the region start, end
/// endpoints to the region end; see DECISIONS.md and the proposed Pass-12 row
/// on the underdetermined "truncate" semantics).
fn retarget_dead_endpoint(
    endpoint: &mut TimeAnchor,
    deleted: EventId,
    region: RegionId,
    edge: RegionEdge,
) {
    if matches!(endpoint, TimeAnchor::Event { id, .. } if *id == deleted) {
        *endpoint = TimeAnchor::Region {
            id: region,
            edge,
            offset: AnchorOffset::Zero,
        };
    }
}

/// Degrades an orphaned annotation anchor's dead event references to the
/// containing-region forms, so the orphaned (kept) referent stays
/// reference-clean under invariant 10. The ledger records `Orphaned`; this is
/// anchor hygiene, not a re-anchoring choice.
fn orphan_annotation_anchor(anchor: &mut AnnotationAnchor, deleted: EventId, region: RegionId) {
    match anchor {
        AnnotationAnchor::Event(event) if *event == deleted => {
            *anchor = AnnotationAnchor::Region(region);
        }
        AnnotationAnchor::Range { start, end } => {
            retarget_dead_endpoint(start, deleted, region, RegionEdge::Start);
            retarget_dead_endpoint(end, deleted, region, RegionEdge::End);
        }
        _ => {}
    }
}

impl<'a> Reducer<'a> {
    fn new(op_set: &'a OperationSet) -> Self {
        Reducer {
            op_set,
            objects: BTreeMap::new(),
            spellings: BTreeMap::new(),
            breaks: BTreeMap::new(),
            page_breaks: BTreeMap::new(),
            conflicts: ConflictRegistry::new(),
            effects: Vec::new(),
            anomalies: BTreeMap::new(),
            minted_by: BTreeMap::new(),
            event_pitches: BTreeMap::new(),
            voice_occupancy: BTreeMap::new(),
            respell_chain: BTreeMap::new(),
            event_modify_chain: BTreeMap::new(),
            pitch_modify_chain: BTreeMap::new(),
            cross_cutting_modify_chain: BTreeMap::new(),
            metric_grid_chain: BTreeMap::new(),
            metadata_chain: WriteChain::new(),
            break_chain: BTreeMap::new(),
            page_break_chain: BTreeMap::new(),
            meter_change_chain: BTreeMap::new(),
            tempo_segment_chain: BTreeMap::new(),
            staff_layout_chain: BTreeMap::new(),
            staff_values: BTreeMap::new(),
            time_signature_values: BTreeMap::new(),
            structures: BTreeMap::new(),
            region_instances: BTreeMap::new(),
            instance_voices: BTreeMap::new(),
            instance_staff: BTreeMap::new(),
            staff_based_regions: BTreeSet::new(),
            migrated_regions: BTreeSet::new(),
            region_migrator: BTreeMap::new(),
            descriptors: BTreeMap::new(),
            promotion: BTreeMap::new(),
            system_mints: BTreeMap::new(),
            tx_minted: BTreeMap::new(),
            current_tx: None,
            equivocation_resolutions: BTreeMap::new(),
            promoted_singles: BTreeMap::new(),
            graph: None,
        }
    }

    fn new_onto(op_set: &'a OperationSet, base: &Score) -> Self {
        let mut reducer = Self::new(op_set);
        reducer.graph = Some(base.clone());
        reducer.seed_from_graph();
        reducer
    }

    /// Seeds reduction indices from the canonical base graph. Base objects are
    /// live but have no operation minter; if a later operation tombstones one,
    /// that deleting operation is used as the provenance fallback already
    /// defined by the bookkeeping reducer.
    fn seed_from_graph(&mut self) {
        let Some(score) = self.graph.as_ref() else {
            return;
        };

        for instrument in &score.instruments {
            self.objects
                .insert(TypedObjectId::Instrument(instrument.id), ObjectState::Live);
        }
        for staff in &score.staves {
            self.objects
                .insert(TypedObjectId::Staff(staff.id), ObjectState::Live);
            // The carried value backs CreateStaff's byte-identical-re-carry
            // idempotence check against base staves.
            self.staff_values.insert(staff.id, staff.clone());
        }
        for group in &score.staff_groups {
            self.objects
                .insert(TypedObjectId::StaffGroup(group.id), ObjectState::Live);
        }
        for part in &score.parts {
            self.objects
                .insert(TypedObjectId::PartDefinition(part.id), ObjectState::Live);
        }
        for signature in &score.time_signatures {
            self.objects.insert(
                TypedObjectId::TimeSignature(signature.id),
                ObjectState::Live,
            );
            self.time_signature_values
                .insert(signature.id, signature.clone());
        }
        for layer in &score.analysis_layers {
            self.objects
                .insert(TypedObjectId::AnalysisLayer(layer.id), ObjectState::Live);
        }
        for view in &score.views {
            self.objects
                .insert(TypedObjectId::View(view.id), ObjectState::Live);
        }

        // The score-level LWW chains seed with the base values so a
        // value-restoring undo of the first operational write can restore the
        // pre-operational state (operation_catalog §UndoTransaction).
        self.metadata_chain.seed(score.metadata.clone());
        for segment in &score.tempo_map.segments {
            self.tempo_segment_chain
                .entry((None, resolved_anchor_position(&segment.start)))
                .or_insert_with(WriteChain::new)
                .seed(Some(segment.clone()));
        }

        for region in &score.canvas.regions {
            self.objects
                .insert(TypedObjectId::Region(region.id), ObjectState::Live);
            if let Some(content) = region.content.staff_based() {
                self.staff_based_regions.insert(region.id);
                self.metric_grid_chain
                    .entry(region.id)
                    .or_insert_with(WriteChain::new)
                    .seed(content.default_metric_grid.clone());
                if let Some(grid) = &content.default_metric_grid {
                    for change in &grid.meter_sequence {
                        self.meter_change_chain
                            .entry((region.id, resolved_anchor_position(&change.anchor)))
                            .or_insert_with(WriteChain::new)
                            .seed(Some(change.clone()));
                    }
                }
                for anchor in &content.user_system_breaks {
                    self.break_chain
                        .entry((region.id, resolved_anchor_position(anchor)))
                        .or_insert_with(WriteChain::new)
                        .seed((anchor.clone(), true));
                }
                for anchor in &content.user_page_breaks {
                    self.page_break_chain
                        .entry((region.id, resolved_anchor_position(anchor)))
                        .or_insert_with(WriteChain::new)
                        .seed((anchor.clone(), true));
                }
            }
            if let Some(local) = &region.local_tempo_map {
                for segment in &local.segments {
                    self.tempo_segment_chain
                        .entry((Some(region.id), resolved_anchor_position(&segment.start)))
                        .or_insert_with(WriteChain::new)
                        .seed(Some(segment.clone()));
                }
            }
            let instance_set = self.region_instances.entry(region.id).or_default();
            for instance in region.staff_instances() {
                instance_set.insert(instance.id);
            }
            for instance in region.staff_instances() {
                self.objects
                    .insert(TypedObjectId::StaffInstance(instance.id), ObjectState::Live);
                self.instance_staff.insert(instance.id, instance.staff);
                self.staff_layout_chain
                    .entry(instance.id)
                    .or_insert_with(WriteChain::new)
                    .seed((
                        instance.instrument_override,
                        instance.staff_lines_override.clone(),
                        instance.visible,
                    ));
                let voice_set = self.instance_voices.entry(instance.id).or_default();
                for voice in &instance.voices {
                    voice_set.insert(voice.id);
                }
                for measure in &instance.measures {
                    self.objects
                        .insert(TypedObjectId::Measure(measure.id), ObjectState::Live);
                }
                for voice in &instance.voices {
                    self.objects
                        .insert(TypedObjectId::Voice(voice.id), ObjectState::Live);
                    // Register base system-promoted voices in the mint registry
                    // so a promotion minted by this reduction is collision-checked
                    // against them (Chapter 5 §"System-Derived Counter Collisions").
                    // Base-internal duplicates keep the first registration: a base
                    // that already collided is invariant-11/18 territory, not a
                    // reduction-time mint.
                    if voice.id.replica() == ReplicaId::SYSTEM_DERIVED {
                        if let VoiceOrigin::SystemPromoted {
                            winning_operation,
                            losing_operation,
                            original_voice,
                        } = &voice.origin
                        {
                            let inputs = promoted_voice_inputs(
                                instance.id,
                                *original_voice,
                                *winning_operation,
                                *losing_operation,
                            );
                            self.system_mints
                                .entry((ObjectKind::Voice, voice.id.counter()))
                                .or_insert((SerializedCanonicalInputs(inputs), None));
                        }
                    }
                }
            }
        }

        for event in score.events.iter_canonical() {
            let event_id = event.id();
            self.objects
                .insert(TypedObjectId::Event(event_id), ObjectState::Live);
            self.event_modify_chain
                .entry(event_id)
                .or_insert_with(WriteChain::new)
                .seed(event.clone());
            let mut pitch_ids = Vec::new();
            let mut pitches = Vec::new();
            event.collect_identified_pitches(&mut pitches);
            for pitch in pitches {
                pitch_ids.push(pitch.id);
                self.objects
                    .insert(TypedObjectId::Pitch(pitch.id), ObjectState::Live);
                self.pitch_modify_chain
                    .entry(pitch.id)
                    .or_insert_with(WriteChain::new)
                    .seed(pitch.pitch.clone());
                // Register base synthetic pitches in the mint registry (same
                // rule as promoted voices above).
                if pitch.id.replica() == ReplicaId::SYSTEM_DERIVED {
                    self.system_mints
                        .entry((ObjectKind::Pitch, pitch.id.counter()))
                        .or_insert((
                            SerializedCanonicalInputs(canonical_pitch_bytes(&pitch.pitch)),
                            None,
                        ));
                }
            }
            self.event_pitches.insert(event_id, pitch_ids);

            if let (EventPosition::Musical(position), EventDuration::Musical(duration)) =
                (event.position(), event.duration())
            {
                self.voice_occupancy
                    .entry(event.voice())
                    .or_default()
                    .push((position.clone(), duration.clone(), event_id));
            }

            // A cue event *references* its source events (Chapter 5 §"Cue
            // Events"), so it enters the referent index: a source tombstone
            // cascade-deletes the cue through the re-anchoring rule table.
            if let Event::Cue(cue) = event {
                if !cue.source.is_empty() {
                    self.structures.insert(
                        TypedObjectId::Event(event_id),
                        cue.source
                            .iter()
                            .copied()
                            .map(TypedObjectId::Event)
                            .collect(),
                    );
                }
            }
        }

        for slur in &score.cross_cutting.slurs {
            let id = TypedObjectId::Slur(slur.id);
            self.objects.insert(id, ObjectState::Live);
            self.cross_cutting_modify_chain
                .entry(id)
                .or_insert_with(WriteChain::new)
                .seed(CrossCuttingValue::Slur(slur.clone()));
            self.structures.insert(
                id,
                vec![
                    TypedObjectId::Event(slur.start_event),
                    TypedObjectId::Event(slur.end_event),
                ],
            );
        }
        for tie in &score.cross_cutting.ties {
            let id = TypedObjectId::Tie(tie.id);
            self.objects.insert(id, ObjectState::Live);
            self.cross_cutting_modify_chain
                .entry(id)
                .or_insert_with(WriteChain::new)
                .seed(CrossCuttingValue::Tie(tie.clone()));
            self.structures.insert(
                id,
                vec![
                    TypedObjectId::Event(tie.start_event),
                    TypedObjectId::Event(tie.end_event),
                ],
            );
        }
        for beam in &score.cross_cutting.beams {
            let id = TypedObjectId::Beam(beam.id);
            self.objects.insert(id, ObjectState::Live);
            self.cross_cutting_modify_chain
                .entry(id)
                .or_insert_with(WriteChain::new)
                .seed(CrossCuttingValue::Beam(beam.clone()));
            self.structures.insert(
                id,
                beam.events
                    .iter()
                    .copied()
                    .map(TypedObjectId::Event)
                    .collect(),
            );
        }
        for tuplet in &score.cross_cutting.tuplets {
            let id = TypedObjectId::Tuplet(tuplet.id);
            self.objects.insert(id, ObjectState::Live);
            self.structures.insert(
                id,
                tuplet
                    .members
                    .iter()
                    .copied()
                    .map(TypedObjectId::Event)
                    .collect(),
            );
        }
        for spanner in &score.cross_cutting.spanners {
            let id = TypedObjectId::Spanner(spanner.id);
            self.objects.insert(id, ObjectState::Live);
            self.cross_cutting_modify_chain
                .entry(id)
                .or_insert_with(WriteChain::new)
                .seed(CrossCuttingValue::Spanner(spanner.clone()));
            // Record the spanner's event-anchored endpoints so a later event
            // tombstone re-anchors it through the same rule table as a created
            // spanner (keeping the graph and ledger consistent on delete).
            self.structures.insert(
                id,
                [&spanner.start, &spanner.end]
                    .into_iter()
                    .filter_map(|anchor| match anchor {
                        TimeAnchor::Event { id, .. } => Some(TypedObjectId::Event(*id)),
                        _ => None,
                    })
                    .collect(),
            );
        }
        // The remaining referent kinds of the re-anchoring rule table enter the
        // same index as slurs/ties/beams/spanners, keyed by their typed id and
        // listing the event references a tombstone must repair. Non-event
        // anchorings (region, measure, wall-clock, free) contribute no entry.
        for marker in &score.cross_cutting.markers {
            self.objects
                .insert(TypedObjectId::Marker(marker.id), ObjectState::Live);
            let refs = anchor_event_refs([&marker.anchor]);
            if !refs.is_empty() {
                self.structures
                    .insert(TypedObjectId::Marker(marker.id), refs);
            }
        }
        for annotation in &score.cross_cutting.analytical {
            self.objects.insert(
                TypedObjectId::AnalyticalAnnotation(annotation.id),
                ObjectState::Live,
            );
            let refs = annotation_anchor_event_refs(&annotation.anchor);
            if !refs.is_empty() {
                self.structures
                    .insert(TypedObjectId::AnalyticalAnnotation(annotation.id), refs);
            }
        }
        for comment in &score.cross_cutting.comments {
            self.objects
                .insert(TypedObjectId::Comment(comment.id), ObjectState::Live);
            let refs = annotation_anchor_event_refs(&comment.anchor);
            if !refs.is_empty() {
                self.structures
                    .insert(TypedObjectId::Comment(comment.id), refs);
            }
        }
        for gesture in &score.cross_cutting.graphic_gestures {
            self.objects
                .insert(TypedObjectId::GraphicGesture(gesture.id), ObjectState::Live);
            let refs = gesture_event_refs(&gesture.anchoring);
            if !refs.is_empty() {
                self.structures
                    .insert(TypedObjectId::GraphicGesture(gesture.id), refs);
            }
        }
        for repeat in &score.cross_cutting.repeats {
            self.objects
                .insert(TypedObjectId::RepeatStructure(repeat.id), ObjectState::Live);
            // Repeats participate in event re-anchoring across every anchor
            // site (rule table "Repeat structure / Anchor").
            let refs = anchor_event_refs(repeat.anchor_sites());
            if !refs.is_empty() {
                self.structures
                    .insert(TypedObjectId::RepeatStructure(repeat.id), refs);
            }
        }
        for lyric in &score.cross_cutting.lyrics {
            self.objects
                .insert(TypedObjectId::LyricLine(lyric.id), ObjectState::Live);
        }
        for chord in &score.cross_cutting.chord_symbols {
            self.objects
                .insert(TypedObjectId::ChordSymbol(chord.id), ObjectState::Live);
        }
        // The base score's explicit user-chosen per-pitch spellings seed the
        // respell chains, so undoing the first operational respell restores
        // the base attachment value rather than dropping it (the bookkeeping
        // `spellings` map still returns to key-absence — base state lives in
        // the graph, not the operational ledger).
        for attachment in &score.spelling_attachments {
            if attachment.layer.is_none() && matches!(attachment.source, SpellingSource::UserChosen)
            {
                if let (SpellingScope::Pitch(pitch), SpellingDirective::Explicit(spelling)) =
                    (&attachment.scope, &attachment.directive)
                {
                    self.respell_chain
                        .entry(*pitch)
                        .or_insert_with(WriteChain::new)
                        .seed(spelling.clone());
                }
            }
        }
    }

    fn run(mut self) -> (MaterializedState, Option<Score>) {
        let singles = self.op_set.single_envelopes();
        let equivocated_all: BTreeSet<OperationId> =
            self.op_set.equivocated_ids().into_iter().collect();

        // 1. HLC monotonicity: exclude anomalous segments.
        let segments = detect_replica_anomalies(&singles);
        let mut excluded: BTreeSet<OperationId> = BTreeSet::new();
        for seg in &segments {
            excluded.extend(seg.excluded.iter().copied());
            self.record_anomaly(IntegrityAnomalyKind::ReplicaStreamQuarantined {
                replica: seg.replica,
                first_bad_counter: seg.first_bad_counter,
            });
        }

        // 1b. ResolveEquivocation promotion (operation_catalog
        // §"ResolveEquivocation"): a set-level, order-independent pre-pass.
        // Among the Single-slot, non-excluded resolves whose `target` is an
        // Equivocated slot and whose `chosen` names one of its candidates, the
        // resolve earliest in canonical order (smallest reduction tuple — the
        // same total HLC order `canonical_reduction_order` selects ready
        // operations by) governs. The chosen candidate envelope joins the
        // reducible set at its own canonical position — the slot reduces as if
        // it had always been Single — and no `OperationSlotEquivocated`
        // anomaly is recorded for it. The verdict is a pure function of the
        // slot map (never of arrival order), so every replica agrees. A
        // resolve that is itself equivocated occupies no Single slot and thus
        // never governs; a resolve in a quarantined segment is excluded from
        // reduction and likewise never governs.
        let mut governing: BTreeMap<OperationId, &OperationEnvelope> = BTreeMap::new();
        for &env in &singles {
            if excluded.contains(&env.id) {
                continue;
            }
            let OperationPayload::ResolveEquivocation(op) = &env.payload else {
                continue;
            };
            let Some(slot) = self.op_set.slot(op.target) else {
                continue;
            };
            if !slot.is_equivocated() || !slot.candidates().any(|c| c == op.chosen) {
                continue;
            }
            governing
                .entry(op.target)
                .and_modify(|current| {
                    if env.stamp.reduction_tuple() < current.stamp.reduction_tuple() {
                        *current = env;
                    }
                })
                .or_insert(env);
        }
        for (target, resolve) in &governing {
            let OperationPayload::ResolveEquivocation(op) = &resolve.payload else {
                unreachable!("only ResolveEquivocation envelopes govern a promotion");
            };
            let candidate = self
                .op_set
                .candidate(op.chosen)
                .expect("every candidate hash of an equivocated slot is retained in the store");
            self.promoted_singles.insert(*target, candidate);
            self.equivocation_resolutions
                .insert(*target, (resolve.id, op.chosen));
        }
        // The losing candidates remain only in the opset's diagnostic
        // candidate store; a resolved slot records no equivocation anomaly.
        let equivocated: BTreeSet<OperationId> = equivocated_all
            .into_iter()
            .filter(|id| !governing.contains_key(id))
            .collect();
        for id in &equivocated {
            self.record_anomaly(IntegrityAnomalyKind::OperationSlotEquivocated {
                operation_id: *id,
            });
        }

        // 2. Reducible candidates = Single slots minus excluded, plus the
        // promoted candidates (each at its own canonical position).
        let reducible: Vec<&OperationEnvelope> = singles
            .iter()
            .copied()
            .filter(|e| !excluded.contains(&e.id))
            .chain(self.promoted_singles.values().copied())
            .collect();
        let reducible_ids: BTreeSet<OperationId> = reducible.iter().map(|e| e.id).collect();
        let declared_transactions: BTreeSet<TransactionId> = singles
            .iter()
            .copied()
            .chain(self.promoted_singles.values().copied())
            .filter_map(|env| match &env.payload {
                OperationPayload::Primitive(OperationKind::DeclareTransaction(descriptor)) => {
                    Some(descriptor.id)
                }
                _ => None,
            })
            .collect();

        // 3. Missing-causal-predecessor rule → pending set (with reasons).
        let pending = compute_pending(
            &reducible,
            &reducible_ids,
            &equivocated,
            &excluded,
            &declared_transactions,
        );
        let active: Vec<&OperationEnvelope> = reducible
            .iter()
            .copied()
            .filter(|e| !pending.contains_key(&e.id))
            .collect();

        // 4. Voice-promotion pre-pass (order-independent assignment).
        self.compute_promotions(&active);

        // 5. Walk active ops in canonical reduction order; group transactions.
        let order = canonical_reduction_order(&active);
        let tx_members = transaction_members(&active);

        // 5a. System-derived counter collision check (Chapter 5 §"System-Derived
        // Counter Collisions"): on a collision, reduction does not continue past
        // the collision point, and neither colliding input set occupies the
        // collided counter. Held operations stay in the (grow-only) operation
        // set and surface in `pending` for external recovery.
        let mut held: BTreeMap<OperationId, PendingReason> = BTreeMap::new();
        if let Some((halt_index, at, earlier_owner)) = self.detect_system_collision(&order) {
            for env in &order[halt_index..] {
                held.insert(env.id, PendingReason::HaltedBySystemCollision { at });
            }
            if let Some(owner) = earlier_owner {
                held.insert(owner, PendingReason::HaltedBySystemCollision { at });
            }
            // Transitive closure: a transaction with a held member is wholly
            // held (atomicity), and an operation causally covering a held
            // operation is held behind it.
            loop {
                let mut changed = false;
                for env in &order {
                    if held.contains_key(&env.id) {
                        continue;
                    }
                    let tx_blocked = member_transaction(env)
                        .and_then(|tx| tx_members.get(&tx))
                        .and_then(|members| {
                            members
                                .iter()
                                .map(|m| m.id)
                                .filter(|id| held.contains_key(id))
                                .min()
                        });
                    let causal_blocked =
                        held.keys().copied().find(|h| env.causal_context.covers(*h));
                    if let Some(on) = tx_blocked.into_iter().chain(causal_blocked).min() {
                        held.insert(env.id, PendingReason::DependsOnPending { on });
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
        }

        let mut processed: BTreeSet<OperationId> = BTreeSet::new();
        for env in &order {
            if processed.contains(&env.id) || held.contains_key(&env.id) {
                continue;
            }
            if let Some(tx) = member_transaction(env) {
                let members = tx_members.get(&tx).cloned().unwrap_or_default();
                self.reduce_transaction_block(tx, &members);
                processed.extend(members.iter().map(|m| m.id));
            } else {
                let effect = self.apply(env);
                self.effects.push((env.id, effect));
                processed.insert(env.id);
            }
        }

        let mut pending_vec: Vec<(OperationId, PendingReason)> =
            pending.into_iter().chain(held).collect();
        pending_vec.sort_by_key(|(id, _)| *id);

        let graph = self.graph.take();
        let state = MaterializedState {
            effects: self.effects,
            conflicts: self.conflicts,
            anomalies: self.anomalies.into_values().collect(),
            objects: self.objects,
            spellings: self.spellings,
            breaks: self.breaks,
            page_breaks: self.page_breaks,
            pending: pending_vec,
        };
        (state, graph)
    }

    fn record_anomaly(&mut self, kind: IntegrityAnomalyKind) {
        let a = IntegrityAnomaly::new(kind);
        self.anomalies.entry(a.id).or_insert(a);
    }

    fn env_of(&self, id: OperationId) -> Option<&'a OperationEnvelope> {
        // A slot promoted by a governing ResolveEquivocation reduces as if it
        // had always been Single with the chosen candidate.
        self.op_set
            .slot(id)
            .and_then(|s| s.single())
            .or_else(|| self.promoted_singles.get(&id).copied())
    }

    // --- Voice promotion pre-pass (Chapter 6 §6.10 InsertEvent). ------------

    fn compute_promotions(&mut self, active: &[&OperationEnvelope]) {
        // Bucket inserts by target voice. Bucketing by `op.voice` alone realizes
        // the spec's `(staff_instance, original_voice)` key: a VoiceId is
        // globally unique and (Invariant 5) belongs to exactly one staff
        // instance, so the voice id alone determines the pair. Promotion applies
        // only to concurrent operations whose half-open duration intervals overlap.
        let mut buckets: BTreeMap<VoiceId, Vec<&OperationEnvelope>> = BTreeMap::new();
        for env in active {
            if let OperationPayload::Primitive(OperationKind::InsertEvent(op)) = &env.payload {
                if self.graph_insert_precondition(op).is_err() {
                    continue;
                }
                buckets.entry(op.voice()).or_default().push(env);
            }
        }
        for (_, mut bucket) in buckets {
            if bucket.len() < 2 {
                continue;
            }
            // Retain non-overlapping operations in the original voice in
            // OperationId order. A concurrent collision with an already-kept
            // insert receives its own deterministic promoted voice.
            bucket.sort_by_key(|e| e.id);
            let mut original_voice = Vec::<&OperationEnvelope>::new();
            for env in bucket {
                let OperationPayload::Primitive(OperationKind::InsertEvent(op)) = &env.payload
                else {
                    continue;
                };
                let collision = original_voice.iter().copied().find(|kept| {
                    let OperationPayload::Primitive(OperationKind::InsertEvent(kept_op)) =
                        &kept.payload
                    else {
                        return false;
                    };
                    self.concurrent(env.id, kept.id) && insert_intervals_overlap(op, kept_op)
                });
                if let Some(winner) = collision {
                    let promoted =
                        derive_promoted_voice_id(op.staff_instance, op.voice(), winner.id, env.id);
                    self.promotion.insert(env.id, (promoted, winner.id));
                } else {
                    original_voice.push(env);
                }
            }
        }
    }

    // --- System-derived counter collision check (Chapter 5 §"System-Derived
    // Counter Collisions"). ---------------------------------------------------

    /// The system-derived identifiers an operation would admit into canonical
    /// state, each with the canonical inputs of its derivation: the promoted
    /// voice assigned by the promotion pre-pass, and any `SYSTEM_DERIVED`-
    /// namespace pitch carried by a minting payload (InsertEvent,
    /// InsertIdentifiedPitch). Non-minting references to system-derived ids —
    /// e.g. a ModifyEvent rewriting a live pitch's content in place — are
    /// deliberately not treated as mints (that is Invariant-11 territory, not a
    /// derivation collision).
    fn prospective_system_mints(
        &self,
        env: &OperationEnvelope,
    ) -> Vec<((ObjectKind, u64), SerializedCanonicalInputs)> {
        let mut mints = Vec::new();
        let OperationPayload::Primitive(kind) = &env.payload else {
            return mints;
        };
        match kind {
            OperationKind::InsertEvent(op) => {
                if let Some((promoted, winner)) = self.promotion.get(&env.id) {
                    mints.push((
                        (ObjectKind::Voice, promoted.counter()),
                        SerializedCanonicalInputs(promoted_voice_inputs(
                            op.staff_instance,
                            op.voice(),
                            *winner,
                            env.id,
                        )),
                    ));
                }
                let mut pitches = Vec::new();
                op.event.collect_identified_pitches(&mut pitches);
                for pitch in pitches {
                    if pitch.id.replica() == ReplicaId::SYSTEM_DERIVED {
                        mints.push((
                            (ObjectKind::Pitch, pitch.id.counter()),
                            SerializedCanonicalInputs(canonical_pitch_bytes(&pitch.pitch)),
                        ));
                    }
                }
            }
            OperationKind::InsertIdentifiedPitch(op)
                if op.pitch.id.replica() == ReplicaId::SYSTEM_DERIVED =>
            {
                mints.push((
                    (ObjectKind::Pitch, op.pitch.id.counter()),
                    SerializedCanonicalInputs(canonical_pitch_bytes(&op.pitch.pitch)),
                ));
            }
            _ => {}
        }
        mints
    }

    /// Walks the canonical order checking every prospective system-derived
    /// mint against the registry (base-seeded occupants plus earlier mints in
    /// the walk). On the first collision — the same `(kind, counter)` claimed
    /// by *different* canonical inputs — records the
    /// `SystemIdentifierCollision` anomaly and returns the halt point
    /// `(index, colliding op, earlier occupant op)`: reduction must not
    /// continue past the collision, and neither input set may occupy the
    /// collided counter (§"System-Derived Counter Collisions"). The earlier
    /// occupant is `None` when it was seeded from the base graph, which cannot
    /// be evicted by this reduction and is left to diagnostic recovery.
    ///
    /// The check is conservative: a claimed mint participates even if the
    /// operation would later fail an unrelated apply-time precondition —
    /// two input sets contending for one counter is a structural identity
    /// failure regardless of which contender ultimately materializes.
    fn detect_system_collision(
        &mut self,
        order: &[&OperationEnvelope],
    ) -> Option<(usize, OperationId, Option<OperationId>)> {
        for (index, env) in order.iter().enumerate() {
            for (key, inputs) in self.prospective_system_mints(env) {
                match self.system_mints.get(&key) {
                    None => {
                        self.system_mints.insert(key, (inputs, Some(env.id)));
                    }
                    Some((existing, _)) if existing.0 == inputs.0 => {
                        // The same derivation re-observed: not a collision.
                    }
                    Some((existing, owner)) => {
                        let owner = *owner;
                        let existing = existing.clone();
                        self.record_anomaly(IntegrityAnomalyKind::SystemIdentifierCollision {
                            kind: key.0,
                            colliding_counter: key.1,
                            input_set_a: existing,
                            input_set_b: inputs,
                        });
                        return Some((index, env.id, owner));
                    }
                }
            }
        }
        None
    }

    fn graph_insert_precondition(
        &self,
        op: &InsertEventOp,
    ) -> Result<(usize, usize, usize), PreconditionFailureReason> {
        let Some(score) = self.graph.as_ref() else {
            return Ok((0, 0, 0));
        };
        let location = graph_voice_location(score, op.voice())
            .ok_or(PreconditionFailureReason::VoiceMissing)?;
        let (region_index, instance_index, _) = location;
        let region = &score.canvas.regions[region_index];
        let instance = &region.staff_instances()[instance_index];
        if instance.id != op.staff_instance {
            return Err(PreconditionFailureReason::VoiceMissing);
        }
        if !matches!(region.time_model, epiphany_core::RegionTimeModel::Metric(_)) {
            return Err(PreconditionFailureReason::WrongRegionTimeModel);
        }
        let event_id = op.event_id();
        if score.events.contains(event_id)
            || score.tombstoned_events.contains(&event_id)
            || op.pitch_ids().iter().any(|pitch| {
                self.objects.contains_key(&TypedObjectId::Pitch(*pitch))
                    || score.tombstoned_pitches.contains(pitch)
            })
        {
            return Err(PreconditionFailureReason::TargetTombstoned);
        }
        Ok(location)
    }

    fn materialize_graph_insert(
        &mut self,
        env: &OperationEnvelope,
        op: &InsertEventOp,
        location: (usize, usize, usize),
        target_voice: VoiceId,
        promotion: Option<(VoiceId, OperationId)>,
    ) -> Result<(), PreconditionFailureReason> {
        let Some(score) = self.graph.as_mut() else {
            return Ok(());
        };
        let (region_index, instance_index, voice_index) = location;
        let event = graph_event_from_insert(op, target_voice);
        score
            .events
            .insert(event)
            .map_err(|_| PreconditionFailureReason::EventDurationInvalid)?;

        if let Some((promoted, winner)) = promotion {
            let instance = score.canvas.regions[region_index]
                .content
                .staff_instances_mut()
                .expect("the precondition found a staff-based instance")
                .get_mut(instance_index)
                .expect("the precondition found this instance");
            instance.voices.push(Voice {
                id: promoted,
                events: vec![op.event_id()],
                default_stem_direction: None,
                is_primary: false,
                origin: VoiceOrigin::SystemPromoted {
                    winning_operation: winner,
                    losing_operation: env.id,
                    original_voice: op.voice(),
                },
            });
        } else {
            let mut ordered = score.canvas.regions[region_index].staff_instances()[instance_index]
                .voices[voice_index]
                .events
                .clone();
            ordered.push(op.event_id());
            ordered.sort_by(|a, b| {
                let a_position = score.events.get(*a).map(Event::position);
                let b_position = score.events.get(*b).map(Event::position);
                match (a_position, b_position) {
                    (
                        Some(EventPosition::Musical(a_position)),
                        Some(EventPosition::Musical(b_position)),
                    ) => a_position.cmp(b_position).then_with(|| a.cmp(b)),
                    _ => a.cmp(b),
                }
            });
            score.canvas.regions[region_index]
                .content
                .staff_instances_mut()
                .expect("the precondition found a staff-based instance")[instance_index]
                .voices[voice_index]
                .events = ordered;
        }
        Ok(())
    }

    fn graph_delete_precondition(
        &self,
        op: &DeleteEventOp,
    ) -> Result<(), PreconditionFailureReason> {
        let Some(score) = self.graph.as_ref() else {
            return Ok(());
        };
        let event = score
            .events
            .get(op.event)
            .ok_or(PreconditionFailureReason::TargetMissing)?;
        let containing_tuplets: Vec<_> = score
            .cross_cutting
            .tuplets
            .iter()
            .filter(|tuplet| tuplet.members.contains(&op.event))
            .map(|tuplet| tuplet.id)
            .collect();
        match &op.tuplet_compensation {
            TupletCompensation::NotInTuplet if !containing_tuplets.is_empty() => {
                Err(PreconditionFailureReason::TupletCompensationInvalid)
            }
            TupletCompensation::NotInTuplet => Ok(()),
            TupletCompensation::ReplaceWithRest { rest } => {
                if score.events.contains(rest.id)
                    || score.tombstoned_events.contains(&rest.id)
                    || event.duration() != &rest.duration
                {
                    Err(PreconditionFailureReason::TupletCompensationInvalid)
                } else {
                    Ok(())
                }
            }
            // The prototype payload carries only ids, not the rewritten tuplet
            // values required to preserve invariant 16. Graph-aware reduction
            // refuses to fabricate those values.
            TupletCompensation::RewriteTuplets { .. } => {
                Err(PreconditionFailureReason::TupletCompensationInvalid)
            }
            TupletCompensation::CascadeDeleteTuplets { tuplets } => {
                let listed: BTreeSet<_> = tuplets.iter().copied().collect();
                let containing: BTreeSet<_> = containing_tuplets.into_iter().collect();
                if listed == containing && !listed.is_empty() {
                    Ok(())
                } else {
                    Err(PreconditionFailureReason::TupletCompensationInvalid)
                }
            }
        }
    }

    /// Removes the deleted event from the materialized graph and keeps it
    /// reference-clean. Returns the repair records for the re-anchoring this
    /// performs beyond the bookkeeping rules — the rule-table rows for the
    /// graph-only referent kinds (markers, cue events, comments, analytical
    /// annotations, graphic gestures) via [`Self::reanchor_event_referents`] —
    /// so the triggering operation's effect can record them (Chapter 6
    /// §Re-Anchoring: "Re-anchoring actions MUST be recorded as RepairRecord
    /// entries in the triggering operation's effect").
    fn materialize_graph_delete(
        &mut self,
        env: &OperationEnvelope,
        op: &DeleteEventOp,
    ) -> Vec<RepairRecord> {
        let Some(score) = self.graph.as_mut() else {
            return Vec::new();
        };
        let Some(event) = score.events.remove(op.event) else {
            return Vec::new();
        };
        let voice_id = event.voice();
        let location = graph_voice_location(score, voice_id);
        let region_id = location.map(|(region, _, _)| score.canvas.regions[region].id);
        // The referent side of the four-key "nearest" ordering, captured before
        // any mutation: the tombstoned event's containment and resolved
        // placement (positions are region-relative; exact rational time in
        // metric regions).
        let referent = ReferentContext {
            voice: voice_id,
            region: region_id,
            position: event.position().clone(),
            duration: event.duration().clone(),
        };
        let removed_event_index = location.and_then(|(region, instance, voice)| {
            score.canvas.regions[region].staff_instances()[instance].voices[voice]
                .events
                .iter()
                .position(|event| *event == op.event)
        });
        if let Some((region_index, instance_index, voice_index)) = location {
            score.canvas.regions[region_index]
                .content
                .staff_instances_mut()
                .expect("an event voice belongs to staff-based content")[instance_index]
                .voices[voice_index]
                .events
                .retain(|id| *id != op.event);
        }

        let mut identified = Vec::new();
        event.collect_identified_pitches(&mut identified);
        let deleted_pitches: Vec<PitchId> = identified.iter().map(|pitch| pitch.id).collect();
        score.tombstoned_events.insert(op.event);
        score
            .tombstoned_pitches
            .extend(deleted_pitches.iter().copied());

        match &op.tuplet_compensation {
            TupletCompensation::ReplaceWithRest { rest } => {
                let new_rest = rest.id;
                for tuplet in &mut score.cross_cutting.tuplets {
                    for member in &mut tuplet.members {
                        if *member == op.event {
                            *member = new_rest;
                        }
                    }
                }
                // The value-typed payload (v1) carries the replacement Rest; it
                // is placed at the deleted event's voice and position.
                let mut replacement = rest.clone();
                replacement.voice = voice_id;
                replacement.position = event.position().clone();
                score
                    .events
                    .insert(Event::Rest(replacement))
                    .expect("replacement-rest preconditions were checked");
                if let Some((region_index, instance_index, voice_index)) =
                    graph_voice_location(score, voice_id)
                {
                    let voice = &mut score.canvas.regions[region_index]
                        .content
                        .staff_instances_mut()
                        .expect("an event voice belongs to staff-based content")[instance_index]
                        .voices[voice_index];
                    if !voice.events.contains(&new_rest) {
                        let index = removed_event_index
                            .unwrap_or(voice.events.len())
                            .min(voice.events.len());
                        voice.events.insert(index, new_rest);
                    }
                }
            }
            TupletCompensation::CascadeDeleteTuplets { tuplets } => {
                let removed: BTreeSet<_> = tuplets.iter().copied().collect();
                score
                    .cross_cutting
                    .tuplets
                    .retain(|tuplet| !removed.contains(&tuplet.id));
                // A decomposition component records its tuplet by id; once that tuplet is
                // gone the reference would dangle (invariant 6, cross-cutting refs
                // resolve), so drop any attachment that names a removed tuplet. The
                // member it described is being tombstoned in the same cascade, so the
                // decomposition has nothing left to describe.
                score.decomposition_attachments.retain(|attachment| {
                    !attachment
                        .components
                        .iter()
                        .any(|component| component.tuplet.is_some_and(|t| removed.contains(&t)))
                });
            }
            TupletCompensation::NotInTuplet | TupletCompensation::RewriteTuplets { .. } => {}
        }

        // Keep the materialized graph reference-clean. The detailed repair
        // records remain in Chapter 6 state; these graph updates realize the
        // representative event-anchored structures Agent B models.
        score
            .cross_cutting
            .ties
            .retain(|tie| tie.start_event != op.event && tie.end_event != op.event);
        score.cross_cutting.beams.retain_mut(|beam| {
            beam.events.retain(|event| *event != op.event);
            beam.events.len() >= 2
        });
        // Slurs and spanners follow the bookkeeping re-anchoring rule: an
        // endpoint-deleted structure re-anchors to its surviving endpoint while
        // one survives, and cascade-deletes only when none does (Chapter 6 §6.5;
        // matching `reanchor_for_tombstone`, so the graph and the ledger agree on
        // the structure's existence). A re-anchored two-endpoint structure
        // collapses onto the survivor — degenerate but reference-clean (both
        // endpoints stay live); proximity-aware re-anchoring is deferred (P11-C5).
        let slurs = std::mem::take(&mut score.cross_cutting.slurs);
        let kept_slurs: Vec<_> = slurs
            .into_iter()
            .filter_map(|mut slur| {
                let start_hit = slur.start_event == op.event;
                let end_hit = slur.end_event == op.event;
                if !start_hit && !end_hit {
                    return Some(slur);
                }
                let survivor = if start_hit {
                    slur.end_event
                } else {
                    slur.start_event
                };
                if survivor != op.event && score.events.contains(survivor) {
                    slur.start_event = if start_hit {
                        survivor
                    } else {
                        slur.start_event
                    };
                    slur.end_event = if end_hit { survivor } else { slur.end_event };
                    Some(slur)
                } else {
                    None
                }
            })
            .collect();
        score.cross_cutting.slurs = kept_slurs;

        let spanners = std::mem::take(&mut score.cross_cutting.spanners);
        let kept_spanners: Vec<_> = spanners
            .into_iter()
            .filter_map(|mut spanner| {
                let start_hit =
                    matches!(spanner.start, TimeAnchor::Event { id, .. } if id == op.event);
                let end_hit = matches!(spanner.end, TimeAnchor::Event { id, .. } if id == op.event);
                if !start_hit && !end_hit {
                    return Some(spanner);
                }
                // The survivor is the *other* anchor's event, if it is an event
                // anchor on a live event; otherwise the spanner cascade-deletes.
                let other = if start_hit {
                    &spanner.end
                } else {
                    &spanner.start
                };
                let survivor = match other {
                    TimeAnchor::Event { id, .. }
                        if *id != op.event && score.events.contains(*id) =>
                    {
                        Some(*id)
                    }
                    _ => None,
                };
                let to = survivor?;
                if start_hit {
                    if let TimeAnchor::Event { id, .. } = &mut spanner.start {
                        *id = to;
                    }
                }
                if end_hit {
                    if let TimeAnchor::Event { id, .. } = &mut spanner.end {
                        *id = to;
                    }
                }
                Some(spanner)
            })
            .collect();
        score.cross_cutting.spanners = kept_spanners;

        // Repeat structures follow the same rule across EVERY anchor site
        // (start/end, jump targets, volta spans): each dead event anchor
        // re-anchors to the nearest surviving event anchor — the
        // lexicographically smallest, matching `nearest_survivor`, so the
        // graph and the ledger agree on both existence and target — and the
        // structure cascade-deletes only when no event anchor survives.
        let repeats = std::mem::take(&mut score.cross_cutting.repeats);
        let kept_repeats: Vec<_> = repeats
            .into_iter()
            .filter_map(|mut repeat| {
                let hit = repeat
                    .anchor_sites()
                    .into_iter()
                    .any(|a| matches!(a, TimeAnchor::Event { id, .. } if *id == op.event));
                if !hit {
                    return Some(repeat);
                }
                let survivor = repeat
                    .anchor_sites()
                    .into_iter()
                    .filter_map(|a| match a {
                        TimeAnchor::Event { id, .. }
                            if *id != op.event && score.events.contains(*id) =>
                        {
                            Some(*id)
                        }
                        _ => None,
                    })
                    .min()?;
                for site in repeat.anchor_sites_mut() {
                    if let TimeAnchor::Event { id, .. } = site {
                        if *id == op.event {
                            *id = survivor;
                        }
                    }
                }
                Some(repeat)
            })
            .collect();
        score.cross_cutting.repeats = kept_repeats;

        score.cross_cutting.lyrics.retain_mut(|line| {
            line.events.retain(|event| *event != op.event);
            !line.events.is_empty()
        });
        // The remaining rule-table rows — markers, cue events, comments,
        // analytical annotations, graphic gestures — are decided and applied
        // together (ledger record + graph mutation) once the event is out of
        // the graph, so the two can never disagree.
        self.reanchor_event_referents(env, op.event, &referent)
    }

    fn materialize_graph_tombstones(
        &mut self,
        env: &OperationEnvelope,
        targets: &[TypedObjectId],
    ) -> Vec<RepairRecord> {
        let mut repairs = Vec::new();
        let events: Vec<EventId> = targets
            .iter()
            .filter_map(|target| match target {
                TypedObjectId::Event(event) => Some(*event),
                _ => None,
            })
            .collect();
        for event in events {
            for placements in self.voice_occupancy.values_mut() {
                placements.retain(|(_, _, stored_event)| *stored_event != event);
            }
            self.voice_occupancy
                .retain(|_, placements| !placements.is_empty());
            repairs.extend(self.materialize_graph_delete(
                env,
                &DeleteEventOp {
                    event,
                    tuplet_compensation: TupletCompensation::NotInTuplet,
                },
            ));
        }

        let Some(score) = self.graph.as_mut() else {
            return repairs;
        };
        for target in targets {
            match target {
                TypedObjectId::Pitch(pitch) => {
                    score.tombstoned_pitches.insert(*pitch);
                }
                TypedObjectId::Voice(voice) => {
                    for region in &mut score.canvas.regions {
                        if let Some(instances) = region.content.staff_instances_mut() {
                            for instance in instances {
                                instance.voices.retain(|candidate| {
                                    candidate.id != *voice || !candidate.events.is_empty()
                                });
                            }
                        }
                    }
                }
                TypedObjectId::Slur(id) => {
                    score.cross_cutting.slurs.retain(|value| value.id != *id);
                }
                TypedObjectId::Tie(id) => {
                    score.cross_cutting.ties.retain(|value| value.id != *id);
                }
                TypedObjectId::Beam(id) => {
                    score.cross_cutting.beams.retain(|value| value.id != *id);
                }
                // Spanner was missing from this walk (an undone spanner mint
                // left a ghost value in the graph) — fixed alongside the
                // repeat arm; both mirror the slur/tie/beam removals.
                TypedObjectId::Spanner(id) => {
                    score.cross_cutting.spanners.retain(|value| value.id != *id);
                }
                TypedObjectId::RepeatStructure(id) => {
                    score.cross_cutting.repeats.retain(|value| value.id != *id);
                }
                // Phase-3 mints: a tombstoned staff / time signature leaves the
                // graph (the undo path preconditions no live reference remains).
                TypedObjectId::Staff(id) => {
                    score.staves.retain(|value| value.id != *id);
                }
                TypedObjectId::TimeSignature(id) => {
                    score.time_signatures.retain(|value| value.id != *id);
                }
                _ => {}
            }
        }
        repairs
    }

    fn materialize_graph_cross_cutting(
        &mut self,
        op: &CreateCrossCuttingOp,
    ) -> Result<(), PreconditionFailureReason> {
        let Some(score) = self.graph.as_mut() else {
            return Ok(());
        };
        // The value-typed payload (v1) carries the real structure with its rich
        // fields, so materialization clones it directly rather than rebuilding a
        // default from the reference-level projection.
        match &op.structure {
            CrossCuttingValue::Slur(slur) => score.cross_cutting.slurs.push(slur.clone()),
            CrossCuttingValue::Beam(beam) => {
                if beam.events.len() < 2 {
                    return Err(PreconditionFailureReason::TargetMissing);
                }
                score.cross_cutting.beams.push(beam.clone());
            }
            CrossCuttingValue::Tie(tie) => score.cross_cutting.ties.push(tie.clone()),
            CrossCuttingValue::Spanner(spanner) => {
                score.cross_cutting.spanners.push(spanner.clone())
            }
        }
        Ok(())
    }

    // --- Dispatch. ----------------------------------------------------------

    fn apply(&mut self, env: &OperationEnvelope) -> OperationEffect {
        match &env.payload {
            OperationPayload::Primitive(kind) => match kind {
                OperationKind::InsertEvent(op) => self.insert_event(env, op),
                OperationKind::DeleteEvent(op) => self.delete_event(env, op),
                OperationKind::RespellPitch(op) => self.respell_pitch(env, op),
                OperationKind::CreateCrossCutting(op) => self.create_cross_cutting(env, op),
                OperationKind::ChangeRegionTimeModel(op) => self.change_region_time_model(env, op),
                OperationKind::SetUserSystemBreak(op) => self.set_user_system_break(env, op),
                OperationKind::DeclareTransaction(desc) => {
                    self.descriptors.insert(desc.id, env.id);
                    OperationEffect::Applied
                }
                // Extension-defined primitive: opaque to the core; recorded as
                // applied (the extension realizes its own effect).
                OperationKind::Registered(_, _) => OperationEffect::Applied,
                OperationKind::ModifyEvent(op) => self.modify_event(env, op),
                OperationKind::Transpose(op) => self.transpose(env, op),
                OperationKind::InsertIdentifiedPitch(op) => self.insert_identified_pitch(env, op),
                OperationKind::DeleteIdentifiedPitch(op) => self.delete_identified_pitch(env, op),
                OperationKind::ModifyIdentifiedPitch(op) => self.modify_identified_pitch(env, op),
                OperationKind::DeleteCrossCutting(op) => self.delete_cross_cutting(env, op),
                OperationKind::ModifyCrossCutting(op) => self.modify_cross_cutting(env, op),
                OperationKind::CreateRegion(op) => self.create_region(env, op),
                OperationKind::DeleteRegion(op) => self.delete_region(env, op),
                OperationKind::CreateStaffInstance(op) => self.create_staff_instance(env, op),
                OperationKind::DeleteStaffInstance(op) => self.delete_staff_instance(env, op),
                OperationKind::CreateVoice(op) => self.create_voice(env, op),
                OperationKind::DeleteVoice(op) => self.delete_voice(env, op),
                OperationKind::SetMetadata(op) => self.set_metadata(env, op),
                OperationKind::SetMetricGrid(op) => self.set_metric_grid(env, op),
                OperationKind::SetUserPageBreak(op) => self.set_user_page_break(env, op),
                OperationKind::CreateStaff(op) => self.create_staff(env, op),
                OperationKind::SetTimeSignature(op) => self.set_time_signature(env, op),
                OperationKind::SetTempoSegment(op) => self.set_tempo_segment(env, op),
                OperationKind::SetStaffLayout(op) => self.set_staff_layout(env, op),
                OperationKind::CreateRepeatStructure(op) => self.create_repeat_structure(env, op),
                OperationKind::DeleteRepeatStructure(op) => self.delete_repeat_structure(env, op),
            },
            OperationPayload::ResolveConflict(op) => self.resolve_conflict(env, op),
            OperationPayload::UndoTransaction(op) => self.undo_transaction(env, op),
            OperationPayload::ResolveEquivocation(op) => self.resolve_equivocation(env, op),
        }
    }

    // --- Per-kind reduction. ------------------------------------------------

    fn set_user_system_break(
        &mut self,
        env: &OperationEnvelope,
        op: &crate::payload::SetUserSystemBreakOp,
    ) -> OperationEffect {
        if let Some(effect) = self.layout_region_slot(op.region) {
            return effect;
        }
        if let Some(score) = self.graph.as_mut() {
            if let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == op.region) {
                if let Some(content) = region.content.staff_based_mut() {
                    apply_break_lww(&mut content.user_system_breaks, &op.anchor, op.present);
                }
            }
        }

        // The LWW bucketing key is the anchor's resolved musical position.
        let key = (op.region, op.resolved_position());
        self.breaks.insert(key.clone(), op.present);
        self.break_chain
            .entry(key)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, (op.anchor.clone(), op.present));
        OperationEffect::Applied
    }

    // --- Group 4 (M2d): score settings (LWW field-overwrite). --------------
    //
    // SetMetadata is an *advisory* last-writer-wins field (operation_catalog
    // §set-user-system-break "LWW advisory"): the latest write in canonical order
    // silently wins, with no conflict — a clean concurrent metadata edit keeps the
    // state clean. SetMetricGrid is a structural field-overwrite: the resolved grid
    // lives in the graph and a concurrent differing grid records a
    // StructuralFieldCollision. SetUserPageBreak mirrors SetUserSystemBreak: a
    // canonical LWW advisory (page_breaks).
    //
    // SetMetricGrid / SetUserPageBreak target a region's staff-based slot, so they
    // share `layout_region_slot`: the region must be live *and* staff-based (a
    // FreeGraphic region has neither a metric grid nor a break list). That verdict
    // reads only the base-free indices, so reduce() and reduce_onto() agree on it.

    /// `Some(NoOp)` when `region` cannot carry a metric grid or user break — it is
    /// missing, tombstoned, or FreeGraphic; `None` when it has a staff-based slot.
    fn layout_region_slot(&self, region: RegionId) -> Option<OperationEffect> {
        let live = matches!(
            self.objects.get(&TypedObjectId::Region(region)),
            Some(ObjectState::Live)
        );
        if live && self.staff_based_regions.contains(&region) {
            None
        } else {
            Some(OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            })
        }
    }

    fn set_metadata(&mut self, env: &OperationEnvelope, op: &SetMetadataOp) -> OperationEffect {
        // Advisory LWW: no conflict, no idempotence short-circuit. The resolved
        // value is the last write in canonical order, held in the graph. The
        // write chain backs value-restoring undo only.
        self.metadata_chain
            .record(env.id, env.transaction, op.metadata.clone());
        if let Some(score) = self.graph.as_mut() {
            score.metadata = op.metadata.clone();
        }
        OperationEffect::Applied
    }

    fn set_metric_grid(
        &mut self,
        env: &OperationEnvelope,
        op: &SetMetricGridOp,
    ) -> OperationEffect {
        if let Some(effect) = self.layout_region_slot(op.region) {
            return effect;
        }
        // A non-empty grid names a time signature per meter change; the graph
        // invariant (epiphany-core invariants.rs) rejects a grid that references an
        // undeclared signature, so reject it here rather than install an
        // invariant-violating grid. Time signatures are seeded from the base, so
        // this verdict is identical with or without a graph.
        if let Some(grid) = &op.grid {
            for change in &grid.meter_sequence {
                if !matches!(
                    self.objects
                        .get(&TypedObjectId::TimeSignature(change.time_signature)),
                    Some(ObjectState::Live)
                ) {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::PreconditionFailedUnderReduction {
                            reason: PreconditionFailureReason::TargetMissing,
                        },
                    };
                }
            }
        }
        let prev = self
            .metric_grid_chain
            .get(&op.region)
            .and_then(|chain| chain.last_write())
            .map(|write| (write.op, write.value.clone()));
        let effect = match prev {
            Some((prev_op, prev_grid)) if self.concurrent(env.id, prev_op) => {
                if prev_grid == op.grid {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    };
                }
                let conflict = ConflictRecord::new(
                    ConflictKind::StructuralFieldCollision {
                        winner: env.id,
                        loser: prev_op,
                        field: FieldPath("metric_grid".to_string()),
                    },
                    vec![env.id, prev_op],
                    vec![TypedObjectId::Region(op.region)],
                );
                let cid = conflict.id;
                self.conflicts.insert(conflict);
                OperationEffect::Conflicted { conflict: cid }
            }
            _ => OperationEffect::Applied,
        };
        self.metric_grid_chain
            .entry(op.region)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, op.grid.clone());
        self.graph_set_metric_grid(op.region, &op.grid);
        effect
    }

    fn graph_set_metric_grid(&mut self, region: RegionId, grid: &Option<MetricGrid>) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        if let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == region) {
            if let Some(content) = region.content.staff_based_mut() {
                content.default_metric_grid = grid.clone();
            }
        }
    }

    fn set_user_page_break(
        &mut self,
        env: &OperationEnvelope,
        op: &SetUserPageBreakOp,
    ) -> OperationEffect {
        if let Some(effect) = self.layout_region_slot(op.region) {
            return effect;
        }
        if let Some(score) = self.graph.as_mut() {
            if let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == op.region) {
                if let Some(content) = region.content.staff_based_mut() {
                    apply_break_lww(&mut content.user_page_breaks, &op.anchor, op.present);
                }
            }
        }
        let key = (op.region, op.resolved_position());
        self.page_breaks.insert(key.clone(), op.present);
        self.page_break_chain
            .entry(key)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, (op.anchor.clone(), op.present));
        OperationEffect::Applied
    }

    fn insert_event(&mut self, env: &OperationEnvelope, op: &InsertEventOp) -> OperationEffect {
        // The reduction keys are read from the carried event value (v1).
        let event_id = op.event_id();
        let orig_voice = op.voice();
        let op_position = op.musical_position();
        let op_duration = op.musical_duration();
        if !op_duration.is_positive() {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::EventDurationInvalid,
                },
            };
        }
        let ev_obj = TypedObjectId::Event(event_id);
        match self.objects.get(&ev_obj) {
            Some(ObjectState::Live) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            None => {}
        }
        let graph_location = match self.graph_insert_precondition(op) {
            Ok(location) => location,
            Err(reason) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction { reason },
                }
            }
        };
        // Pitch-id freshness in base-free reduction: a carried pitch id that
        // already exists in canonical state (live or tombstoned) is not fresh.
        // The graph-aware precondition above already enforces this (with the
        // same reason), so this only fires when no graph is present — it keeps
        // the two reduction APIs in agreement on the same operation set.
        if op
            .pitch_ids()
            .iter()
            .any(|pitch| self.objects.contains_key(&TypedObjectId::Pitch(*pitch)))
        {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetTombstoned,
                },
            };
        }
        let voice_obj = TypedObjectId::Voice(orig_voice);
        match self.objects.get(&voice_obj) {
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::VoiceMissing,
                    },
                }
            }
            Some(ObjectState::Live) => {}
            None => {
                // Implicit voice creation on first use (prototype convention).
                self.objects.insert(voice_obj, ObjectState::Live);
                self.minted_by.insert(voice_obj, env.id);
                // Track it under its staff instance for the container empty-check.
                self.instance_voices
                    .entry(op.staff_instance)
                    .or_default()
                    .insert(orig_voice);
            }
        }

        let promotion = self.promotion.get(&env.id).copied();
        let target_voice = promotion.map(|(voice, _)| voice).unwrap_or(orig_voice);
        if self
            .voice_occupancy
            .get(&target_voice)
            .is_some_and(|events| {
                events.iter().any(|(position, duration, _)| {
                    intervals_overlap(position, duration, &op_position, &op_duration)
                })
            })
        {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::EventDurationInvalid,
                },
            };
        }

        if let Err(reason) =
            self.materialize_graph_insert(env, op, graph_location, target_voice, promotion)
        {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction { reason },
            };
        }

        let mut repairs = Vec::new();
        if let Some((promoted, _)) = promotion {
            let pv = TypedObjectId::Voice(promoted);
            self.objects.entry(pv).or_insert(ObjectState::Live);
            self.minted_by.entry(pv).or_insert(env.id);
            self.note_minted(env, pv);
            repairs.push(RepairRecord {
                kind: RepairKind::VoicePromoted {
                    from: orig_voice,
                    to: promoted,
                },
                target: pv,
            });
        }

        self.objects.insert(ev_obj, ObjectState::Live);
        self.minted_by.insert(ev_obj, env.id);
        self.note_minted(env, ev_obj);
        // Seed the write chains with the minted values (the same value the
        // graph materializes), so a later modify's chain-predecessor is the
        // inserted state in graph-free and graph-aware reduction alike.
        self.event_modify_chain
            .entry(event_id)
            .or_insert_with(WriteChain::new)
            .seed(graph_event_from_insert(op, target_voice));
        let mut carried: Vec<&epiphany_core::IdentifiedPitch> = Vec::new();
        op.event.collect_identified_pitches(&mut carried);
        for ip in &carried {
            self.pitch_modify_chain
                .entry(ip.id)
                .or_insert_with(WriteChain::new)
                .seed(ip.pitch.clone());
        }
        let mut pitches = Vec::new();
        for p in op.pitch_ids() {
            let p_obj = TypedObjectId::Pitch(p);
            self.objects.insert(p_obj, ObjectState::Live);
            self.minted_by.insert(p_obj, env.id);
            self.note_minted(env, p_obj);
            pitches.push(p);
        }
        self.event_pitches.insert(event_id, pitches);
        self.voice_occupancy.entry(target_voice).or_default().push((
            op_position,
            op_duration,
            event_id,
        ));

        if repairs.is_empty() {
            OperationEffect::Applied
        } else {
            OperationEffect::AppliedWithRepair { repairs }
        }
    }

    fn delete_event(&mut self, env: &OperationEnvelope, op: &DeleteEventOp) -> OperationEffect {
        let ev_obj = TypedObjectId::Event(op.event);
        match self.objects.get(&ev_obj) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                // Concurrent same-target deletes are idempotent.
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                };
            }
            Some(ObjectState::Live) => {}
        }
        if let Err(reason) = self.graph_delete_precondition(op) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction { reason },
            };
        }

        let deleted_placement = self.voice_occupancy.iter().find_map(|(voice, events)| {
            events
                .iter()
                .find(|(_, _, event)| *event == op.event)
                .map(|(position, duration, _)| (*voice, position.clone(), duration.clone()))
        });
        // The tombstoned referent's voice, for the containment-proximity key of
        // the re-anchoring reasons below: the occupancy index (base-free) with
        // the graph as fallback for non-metric events. Captured before the
        // graph delete removes the event.
        let referent_voice = deleted_placement
            .as_ref()
            .map(|(voice, _, _)| *voice)
            .or_else(|| {
                self.graph
                    .as_ref()
                    .and_then(|score| score.events.get(op.event))
                    .map(Event::voice)
            });
        let graph_repairs = self.materialize_graph_delete(env, op);

        let minter = self.minted_by.get(&ev_obj).copied().unwrap_or(env.id);
        self.objects.insert(
            ev_obj,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by: minter,
            },
        );
        for events in self.voice_occupancy.values_mut() {
            events.retain(|(_, _, event)| *event != op.event);
        }
        self.voice_occupancy.retain(|_, events| !events.is_empty());
        let mut repairs = graph_repairs;

        // Tombstone contained pitches.
        if let Some(pitches) = self.event_pitches.get(&op.event).cloned() {
            for p in pitches {
                let p_obj = TypedObjectId::Pitch(p);
                let pm = self.minted_by.get(&p_obj).copied().unwrap_or(env.id);
                self.objects.insert(
                    p_obj,
                    ObjectState::Tombstoned {
                        deleted_by: env.id,
                        minted_by: pm,
                    },
                );
            }
        }

        // Tuplet compensation.
        match &op.tuplet_compensation {
            TupletCompensation::NotInTuplet => {}
            TupletCompensation::ReplaceWithRest { rest } => {
                let new_rest = rest.id;
                let rest_duration = match &rest.duration {
                    EventDuration::Musical(d) => d.clone(),
                    EventDuration::WallClock(_) | EventDuration::Indeterminate(_) => {
                        MusicalDuration::zero()
                    }
                };
                let rest_obj = TypedObjectId::Event(new_rest);
                self.objects.insert(rest_obj, ObjectState::Live);
                self.minted_by.insert(rest_obj, env.id);
                self.note_minted(env, rest_obj);
                self.event_pitches.insert(new_rest, Vec::new());
                if let Some((voice, position, _)) = &deleted_placement {
                    self.voice_occupancy.entry(*voice).or_default().push((
                        position.clone(),
                        rest_duration,
                        new_rest,
                    ));
                }
                repairs.push(RepairRecord {
                    kind: RepairKind::TupletCompensated {
                        compensation_kind: TupletCompensationKind::ReplaceWithRest,
                    },
                    target: rest_obj,
                });
            }
            TupletCompensation::RewriteTuplets { tuplets } => {
                if let Some(first) = tuplets.first() {
                    repairs.push(RepairRecord {
                        kind: RepairKind::TupletCompensated {
                            compensation_kind: TupletCompensationKind::RewriteTuplets,
                        },
                        target: TypedObjectId::Tuplet(*first),
                    });
                }
            }
            TupletCompensation::CascadeDeleteTuplets { tuplets } => {
                for t in tuplets {
                    let t_obj = TypedObjectId::Tuplet(*t);
                    let tm = self.minted_by.get(&t_obj).copied().unwrap_or(env.id);
                    self.objects.insert(
                        t_obj,
                        ObjectState::Tombstoned {
                            deleted_by: env.id,
                            minted_by: tm,
                        },
                    );
                    repairs.push(RepairRecord {
                        kind: RepairKind::TupletCompensated {
                            compensation_kind: TupletCompensationKind::CascadeDeleteTuplets,
                        },
                        target: t_obj,
                    });
                }
            }
        }

        // Re-anchor cross-cutting structures referencing the tombstoned event.
        self.reanchor_for_tombstone(env, ev_obj, &mut repairs, referent_voice);

        if repairs.is_empty() {
            OperationEffect::Applied
        } else {
            OperationEffect::AppliedWithRepair { repairs }
        }
    }

    fn respell_pitch(&mut self, env: &OperationEnvelope, op: &RespellPitchOp) -> OperationEffect {
        let p_obj = TypedObjectId::Pitch(op.pitch);
        match self.objects.get(&p_obj) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            Some(ObjectState::Live) => {}
        }

        let prev_op = self
            .respell_chain
            .get(&op.pitch)
            .and_then(|chain| chain.last_write())
            .map(|write| write.op);
        match prev_op {
            None => {
                self.materialize_respell(env, op);
                OperationEffect::Applied
            }
            Some(prev_op) => {
                let prev_spelling = self.spellings.get(&op.pitch).cloned();
                let concurrent = self.concurrent(env.id, prev_op);
                if concurrent {
                    if prev_spelling.as_ref() == Some(&op.spelling) {
                        // Identical concurrent respelling: idempotent.
                        OperationEffect::NoOp {
                            reason: NoOpReason::AlreadyApplied,
                        }
                    } else {
                        // Differing concurrent respelling: this op is later in
                        // canonical order, so it wins and materializes; the
                        // earlier op is recorded as the loser. (Prototype
                        // convention: the winner carries the Conflicted effect;
                        // see DECISIONS.md.)
                        self.materialize_respell(env, op);
                        let conflict = ConflictRecord::new(
                            ConflictKind::StructuralFieldCollision {
                                winner: env.id,
                                loser: prev_op,
                                field: FieldPath("spelling".to_string()),
                            },
                            vec![env.id, prev_op],
                            vec![p_obj],
                        );
                        let cid = conflict.id;
                        self.conflicts.insert(conflict);
                        OperationEffect::Conflicted { conflict: cid }
                    }
                } else {
                    // Causally-ordered re-respell: intentional overwrite.
                    self.materialize_respell(env, op);
                    OperationEffect::Applied
                }
            }
        }
    }

    /// Records a winning respell: the canonical bookkeeping spelling, the LWW
    /// marker, and — for graph-aware reduction — an explicit user-chosen
    /// [`SpellingAttachment`] so the spelling/decomposition pre-passes (Agent H's
    /// `derive_annotations`) resolve the override with manual-override precedence.
    /// Without the graph attachment a reduced `RespellPitch` would be visible only
    /// in `MaterializedState.spellings` and lost before annotation derivation.
    fn materialize_respell(&mut self, env: &OperationEnvelope, op: &RespellPitchOp) {
        self.spellings.insert(op.pitch, op.spelling.clone());
        self.respell_chain
            .entry(op.pitch)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, op.spelling.clone());
        self.graph_respell_pitch(op.pitch, &op.spelling);
    }

    fn graph_respell_pitch(&mut self, pitch: PitchId, spelling: &PitchSpelling) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        // Upsert the user-chosen explicit override (one per pitch), keeping the
        // attachment list's canonical order stable for the resolver's tie-break.
        if let Some(existing) = score.spelling_attachments.iter_mut().find(|a| {
            a.layer.is_none()
                && matches!(a.source, SpellingSource::UserChosen)
                && matches!(&a.scope, SpellingScope::Pitch(p) if *p == pitch)
                && matches!(a.directive, SpellingDirective::Explicit(_))
        }) {
            existing.directive = SpellingDirective::Explicit(spelling.clone());
        } else {
            score.spelling_attachments.push(SpellingAttachment {
                scope: SpellingScope::Pitch(pitch),
                directive: SpellingDirective::Explicit(spelling.clone()),
                source: SpellingSource::UserChosen,
                priority: 0,
                layer: None,
            });
        }
    }

    fn create_cross_cutting(
        &mut self,
        env: &OperationEnvelope,
        op: &CreateCrossCuttingOp,
    ) -> OperationEffect {
        let sid = op.structure.id();
        let endpoints = op.structure.endpoints();
        match self.objects.get(&sid) {
            Some(ObjectState::Live) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            None => {}
        }
        // Endpoints must exist (live).
        for e in &endpoints {
            if !matches!(self.objects.get(e), Some(ObjectState::Live)) {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            }
        }
        if let Err(reason) = self.materialize_graph_cross_cutting(op) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction { reason },
            };
        }
        self.objects.insert(sid, ObjectState::Live);
        self.minted_by.insert(sid, env.id);
        self.note_minted(env, sid);
        // Seed the write chain with the minted value, so a later modify's
        // chain-predecessor is the created state.
        self.cross_cutting_modify_chain
            .entry(sid)
            .or_insert_with(WriteChain::new)
            .seed(op.structure.clone());
        self.structures.insert(sid, endpoints);
        OperationEffect::Applied
    }

    fn delete_cross_cutting(
        &mut self,
        env: &OperationEnvelope,
        op: &DeleteCrossCuttingOp,
    ) -> OperationEffect {
        let sid = op.structure;
        // DeleteCrossCutting names a cross-cutting structure only; refuse to
        // tombstone any other object kind through this path.
        if !matches!(
            sid,
            TypedObjectId::Tie(_)
                | TypedObjectId::Slur(_)
                | TypedObjectId::Beam(_)
                | TypedObjectId::Spanner(_)
        ) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        let minted_by = match self.objects.get(&sid) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            // Concurrent same-target deletes are idempotent (delete-wins).
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                }
            }
            Some(ObjectState::Live) => self.minted_by.get(&sid).copied().unwrap_or(env.id),
        };
        self.objects.insert(
            sid,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by,
            },
        );
        // Drop the transient endpoint/LWW indices so a later event tombstone's
        // re-anchoring pass never re-processes the deleted structure. The write
        // chain goes with it: a delete is not inverted (P11-C8), so a
        // tombstoned structure's chain can never be restored.
        self.structures.remove(&sid);
        self.cross_cutting_modify_chain.remove(&sid);
        self.graph_delete_cross_cutting(sid);
        OperationEffect::Applied
    }

    fn create_repeat_structure(
        &mut self,
        env: &OperationEnvelope,
        op: &CreateRepeatStructureOp,
    ) -> OperationEffect {
        let sid = TypedObjectId::RepeatStructure(op.repeat.id);
        match self.objects.get(&sid) {
            // Set-union: a repeat create of a live id reads AlreadyApplied
            // without value comparison (the cross-cutting discipline; the
            // RecreateContentMismatch scope stays CreateStaff + carried
            // TimeSignature).
            Some(ObjectState::Live) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            None => {}
        }
        // Every event-referencing anchor site must resolve live — start/end,
        // the kind's jump targets, each volta's span (operation_catalog
        // §"Repeat Structures": the mint must leave the graph satisfying the
        // reference-resolution invariants).
        let endpoints = anchor_event_refs(op.repeat.anchor_sites());
        for e in &endpoints {
            if !matches!(self.objects.get(e), Some(ObjectState::Live)) {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            }
        }
        if let Some(score) = self.graph.as_mut() {
            score.cross_cutting.repeats.push(op.repeat.clone());
        }
        self.objects.insert(sid, ObjectState::Live);
        self.minted_by.insert(sid, env.id);
        self.note_minted(env, sid);
        // Register into the referent index so event tombstones repair the
        // structure per the rule table (no write chain: there is no
        // ModifyRepeatStructure at this revision).
        if !endpoints.is_empty() {
            self.structures.insert(sid, endpoints);
        }
        OperationEffect::Applied
    }

    fn delete_repeat_structure(
        &mut self,
        env: &OperationEnvelope,
        op: &DeleteRepeatStructureOp,
    ) -> OperationEffect {
        let sid = TypedObjectId::RepeatStructure(op.repeat);
        let minted_by = match self.objects.get(&sid) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            // Concurrent same-target deletes are idempotent (delete-wins).
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                }
            }
            Some(ObjectState::Live) => self.minted_by.get(&sid).copied().unwrap_or(env.id),
        };
        self.objects.insert(
            sid,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by,
            },
        );
        // Drop the referent-index entry so a later event tombstone's
        // re-anchoring pass never re-processes the deleted structure.
        self.structures.remove(&sid);
        if let Some(score) = self.graph.as_mut() {
            score.cross_cutting.repeats.retain(|r| r.id != op.repeat);
        }
        OperationEffect::Applied
    }

    fn modify_cross_cutting(
        &mut self,
        env: &OperationEnvelope,
        op: &ModifyCrossCuttingOp,
    ) -> OperationEffect {
        let sid = op.id();
        match self.objects.get(&sid) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            Some(ObjectState::Live) => {}
        }
        // A beam must keep at least two events (mirrors CreateCrossCutting); the
        // new endpoints must all be live.
        if let CrossCuttingValue::Beam(beam) = &op.structure {
            if beam.events.len() < 2 {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            }
        }
        let endpoints = op.structure.endpoints();
        for e in &endpoints {
            if !matches!(self.objects.get(e), Some(ObjectState::Live)) {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            }
        }
        // LWW field-overwrite, mirroring modify_event: the resolved value lives in
        // the graph; MaterializedState records only the effect and, on a
        // concurrent differing write, a StructuralFieldCollision.
        let prev = self
            .cross_cutting_modify_chain
            .get(&sid)
            .and_then(|chain| chain.last_write())
            .map(|write| (write.op, write.value.clone()));
        let effect = match prev {
            Some((prev_op, prev_value)) if self.concurrent(env.id, prev_op) => {
                if prev_value == op.structure {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    };
                }
                let conflict = ConflictRecord::new(
                    ConflictKind::StructuralFieldCollision {
                        winner: env.id,
                        loser: prev_op,
                        field: FieldPath("cross_cutting".to_string()),
                    },
                    vec![env.id, prev_op],
                    vec![sid],
                );
                let cid = conflict.id;
                self.conflicts.insert(conflict);
                OperationEffect::Conflicted { conflict: cid }
            }
            _ => OperationEffect::Applied,
        };
        self.cross_cutting_modify_chain
            .entry(sid)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, op.structure.clone());
        self.structures.insert(sid, endpoints);
        self.graph_modify_cross_cutting(&op.structure);
        effect
    }

    fn graph_delete_cross_cutting(&mut self, sid: TypedObjectId) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        match sid {
            TypedObjectId::Slur(id) => score.cross_cutting.slurs.retain(|v| v.id != id),
            TypedObjectId::Tie(id) => score.cross_cutting.ties.retain(|v| v.id != id),
            TypedObjectId::Beam(id) => score.cross_cutting.beams.retain(|v| v.id != id),
            TypedObjectId::Spanner(id) => score.cross_cutting.spanners.retain(|v| v.id != id),
            _ => {}
        }
    }

    fn graph_modify_cross_cutting(&mut self, value: &CrossCuttingValue) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        // Replace the structure in place by id (identity is preserved across a
        // modify). A structure that is Live in bookkeeping but absent from the
        // graph (e.g. one a DeleteEvent re-anchor removed from the graph while
        // bookkeeping kept it Live) is left as-is; the modify is still recorded.
        match value {
            CrossCuttingValue::Slur(slur) => {
                if let Some(existing) = score
                    .cross_cutting
                    .slurs
                    .iter_mut()
                    .find(|v| v.id == slur.id)
                {
                    *existing = slur.clone();
                }
            }
            CrossCuttingValue::Tie(tie) => {
                if let Some(existing) = score.cross_cutting.ties.iter_mut().find(|v| v.id == tie.id)
                {
                    *existing = tie.clone();
                }
            }
            CrossCuttingValue::Beam(beam) => {
                if let Some(existing) = score
                    .cross_cutting
                    .beams
                    .iter_mut()
                    .find(|v| v.id == beam.id)
                {
                    *existing = beam.clone();
                }
            }
            CrossCuttingValue::Spanner(spanner) => {
                if let Some(existing) = score
                    .cross_cutting
                    .spanners
                    .iter_mut()
                    .find(|v| v.id == spanner.id)
                {
                    *existing = spanner.clone();
                }
            }
        }
    }

    // --- Group 3 (M2c): structural container CRUD. -------------------------
    //
    // Creates mint an empty container (set-union creation; the authoring contract
    // is that the carried value is empty, so only its own object is minted).
    // Deletes are empty-only delete-wins: a precondition NoOp unless the
    // container has no live children. Child liveness is read from
    // `region_instances` / `instance_voices` (and `voice_occupancy` for a voice's
    // events), so the ledger projection and the graph agree on the result.

    fn mint_container(&mut self, env: &OperationEnvelope, obj: TypedObjectId) {
        self.objects.insert(obj, ObjectState::Live);
        self.minted_by.insert(obj, env.id);
        self.note_minted(env, obj);
    }

    /// `Some(effect)` when `obj` cannot be freshly minted (already live or
    /// tombstoned); `None` when it is fresh and the create may proceed.
    fn mint_precondition(&self, obj: TypedObjectId) -> Option<OperationEffect> {
        match self.objects.get(&obj) {
            Some(ObjectState::Live) => Some(OperationEffect::NoOp {
                reason: NoOpReason::AlreadyApplied,
            }),
            Some(ObjectState::Tombstoned { .. }) => Some(OperationEffect::NoOp {
                reason: NoOpReason::TargetTombstoned,
            }),
            None => None,
        }
    }

    fn create_region(&mut self, env: &OperationEnvelope, op: &CreateRegionOp) -> OperationEffect {
        let robj = TypedObjectId::Region(op.region_id());
        if let Some(effect) = self.mint_precondition(robj) {
            return effect;
        }
        // A create mints an *empty* container: its child objects are minted (or
        // base-seeded) separately. A carried value bearing any typed child object
        // — a staff instance, a barline-alignment group, or a graphic object, each
        // a distinct TypedObjectId the reducer does not mint here — would import an
        // unminted object into the graph, so it is rejected (Catalog §Structural
        // Containers).
        if !op.region.content.staff_instances().is_empty()
            || !op.region.content.barline_alignment_groups().is_empty()
            || !op.region.content.graphic_objects().is_empty()
        {
            return container_not_empty();
        }
        self.graph_create_region(&op.region);
        self.mint_container(env, robj);
        self.region_instances.entry(op.region_id()).or_default();
        if let Some(content) = op.region.content.staff_based() {
            self.staff_based_regions.insert(op.region_id());
            // Seed the region's layout/metric write chains from the carried
            // content (empty of typed children, but it may carry a grid or
            // break advisories), so a later overwrite's chain-predecessor is
            // the created state.
            self.metric_grid_chain
                .entry(op.region_id())
                .or_insert_with(WriteChain::new)
                .seed(content.default_metric_grid.clone());
            if let Some(grid) = &content.default_metric_grid {
                for change in &grid.meter_sequence {
                    self.meter_change_chain
                        .entry((op.region_id(), resolved_anchor_position(&change.anchor)))
                        .or_insert_with(WriteChain::new)
                        .seed(Some(change.clone()));
                }
            }
            for anchor in &content.user_system_breaks {
                self.break_chain
                    .entry((op.region_id(), resolved_anchor_position(anchor)))
                    .or_insert_with(WriteChain::new)
                    .seed((anchor.clone(), true));
            }
            for anchor in &content.user_page_breaks {
                self.page_break_chain
                    .entry((op.region_id(), resolved_anchor_position(anchor)))
                    .or_insert_with(WriteChain::new)
                    .seed((anchor.clone(), true));
            }
        }
        if let Some(local) = &op.region.local_tempo_map {
            for segment in &local.segments {
                self.tempo_segment_chain
                    .entry((
                        Some(op.region_id()),
                        resolved_anchor_position(&segment.start),
                    ))
                    .or_insert_with(WriteChain::new)
                    .seed(Some(segment.clone()));
            }
        }
        OperationEffect::Applied
    }

    fn create_staff_instance(
        &mut self,
        env: &OperationEnvelope,
        op: &CreateStaffInstanceOp,
    ) -> OperationEffect {
        if !matches!(
            self.objects.get(&TypedObjectId::Region(op.region)),
            Some(ObjectState::Live)
        ) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        let iobj = TypedObjectId::StaffInstance(op.instance_id());
        if let Some(effect) = self.mint_precondition(iobj) {
            return effect;
        }
        // Reject a carried staff instance bearing any typed child object — a voice
        // or a measure (the two object collections it can hold).
        if !op.instance.voices.is_empty() || !op.instance.measures.is_empty() {
            return container_not_empty();
        }
        // With staves mintable (operation_catalog §CreateStaff), the instance's
        // referenced global Staff must be live — the mint must leave the graph
        // satisfying reference resolution. Graph-aware only (like the insert
        // preconditions): base-free reduction has no staff universe to check
        // against, and the base-seeded scenarios satisfy this vacuously.
        if self.graph.is_some()
            && !matches!(
                self.objects.get(&TypedObjectId::Staff(op.instance.staff)),
                Some(ObjectState::Live)
            )
        {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        self.graph_create_staff_instance(op.region, &op.instance);
        self.mint_container(env, iobj);
        self.region_instances
            .entry(op.region)
            .or_default()
            .insert(op.instance_id());
        self.instance_voices.entry(op.instance_id()).or_default();
        self.instance_staff
            .insert(op.instance_id(), op.instance.staff);
        // Seed the layout-advisory chain with the minted instance's fields, so
        // a later SetStaffLayout's chain-predecessor is the created state.
        self.staff_layout_chain
            .entry(op.instance_id())
            .or_insert_with(WriteChain::new)
            .seed((
                op.instance.instrument_override,
                op.instance.staff_lines_override.clone(),
                op.instance.visible,
            ));
        OperationEffect::Applied
    }

    fn create_voice(&mut self, env: &OperationEnvelope, op: &CreateVoiceOp) -> OperationEffect {
        if !matches!(
            self.objects
                .get(&TypedObjectId::StaffInstance(op.staff_instance)),
            Some(ObjectState::Live)
        ) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        let vobj = TypedObjectId::Voice(op.voice_id());
        if let Some(effect) = self.mint_precondition(vobj) {
            return effect;
        }
        if !op.voice.events.is_empty() {
            return container_not_empty();
        }
        self.graph_create_voice(op.staff_instance, &op.voice);
        self.mint_container(env, vobj);
        self.instance_voices
            .entry(op.staff_instance)
            .or_default()
            .insert(op.voice_id());
        OperationEffect::Applied
    }

    // --- Phase-3 first tranche (operation_catalog §CreateStaff, §"Meter and
    // Tempo Overwrites", §SetStaffLayout). ------------------------------------

    /// Set-union creation of a global `Staff` on the score root
    /// (operation_catalog §CreateStaff): fresh id mints; a byte-identical
    /// re-carry is idempotent; a differing value under a live id is a
    /// precondition no-op. Graph-aware reduction additionally preconditions
    /// that the referenced instrument is live and, when `group` is present,
    /// that the staff group resolves.
    fn create_staff(&mut self, env: &OperationEnvelope, op: &CreateStaffOp) -> OperationEffect {
        let sobj = TypedObjectId::Staff(op.staff_id());
        match self.objects.get(&sobj) {
            Some(ObjectState::Live) => {
                let identical = self
                    .staff_values
                    .get(&op.staff_id())
                    .is_some_and(|known| known == &op.staff);
                return if identical {
                    OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    }
                } else {
                    OperationEffect::NoOp {
                        reason: NoOpReason::PreconditionFailedUnderReduction {
                            reason: PreconditionFailureReason::RecreateContentMismatch,
                        },
                    }
                };
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            None => {}
        }
        // Reference-resolution preconditions are graph-aware (like the insert
        // preconditions): base-free reduction has no instrument/group universe
        // to check against.
        if self.graph.is_some() {
            if !matches!(
                self.objects
                    .get(&TypedObjectId::Instrument(op.staff.instrument)),
                Some(ObjectState::Live)
            ) {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            }
            if let Some(group) = op.staff.group {
                if !matches!(
                    self.objects.get(&TypedObjectId::StaffGroup(group)),
                    Some(ObjectState::Live)
                ) {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::PreconditionFailedUnderReduction {
                            reason: PreconditionFailureReason::TargetMissing,
                        },
                    };
                }
            }
        }
        if let Some(score) = self.graph.as_mut() {
            score.staves.push(op.staff.clone());
        }
        self.mint_container(env, sobj);
        self.staff_values.insert(op.staff_id(), op.staff.clone());
        OperationEffect::Applied
    }

    /// Set-union mint of a `TimeSignature` carried by a `SetTimeSignature`
    /// (operation_catalog §"Meter and Tempo Overwrites"): fresh id mints;
    /// byte-identical re-carry is idempotent; a differing value under a live
    /// id — or a tombstoned id — refuses the whole operation.
    fn mint_time_signature(
        &mut self,
        env: &OperationEnvelope,
        signature: &TimeSignature,
    ) -> Result<(), OperationEffect> {
        let obj = TypedObjectId::TimeSignature(signature.id);
        match self.objects.get(&obj) {
            Some(ObjectState::Live) => {
                let identical = self
                    .time_signature_values
                    .get(&signature.id)
                    .is_some_and(|known| known == signature);
                if identical {
                    Ok(())
                } else {
                    Err(OperationEffect::NoOp {
                        reason: NoOpReason::PreconditionFailedUnderReduction {
                            reason: PreconditionFailureReason::RecreateContentMismatch,
                        },
                    })
                }
            }
            Some(ObjectState::Tombstoned { .. }) => Err(OperationEffect::NoOp {
                reason: NoOpReason::TargetTombstoned,
            }),
            None => {
                self.mint_container(env, obj);
                self.time_signature_values
                    .insert(signature.id, signature.clone());
                if let Some(score) = self.graph.as_mut() {
                    if !score.time_signatures.iter().any(|t| t.id == signature.id) {
                        score.time_signatures.push(signature.clone());
                    }
                }
                Ok(())
            }
        }
    }

    /// Sets, replaces, or removes the single meter change at the anchor's
    /// resolved position in the region's default metric grid — an LWW
    /// structural overwrite keyed by `(region, resolved position)`
    /// (operation_catalog §"Meter and Tempo Overwrites"). The carried
    /// signature's beat-group sum is validated at construction and at decode,
    /// so a malformed value never reaches this reduction.
    fn set_time_signature(
        &mut self,
        env: &OperationEnvelope,
        op: &SetTimeSignatureOp,
    ) -> OperationEffect {
        if let Some(effect) = self.layout_region_slot(op.region) {
            return effect;
        }
        if let Some(signature) = &op.time_signature {
            if let Err(effect) = self.mint_time_signature(env, signature) {
                return effect;
            }
        }
        let key = (op.region, op.resolved_position());
        let written: Option<MeterChange> =
            op.time_signature.as_ref().map(|signature| MeterChange {
                anchor: op.anchor.clone(),
                time_signature: signature.id,
            });
        let prev = self
            .meter_change_chain
            .get(&key)
            .and_then(|chain| chain.last_write())
            .map(|write| (write.op, write.value.clone()));
        let effect = match prev {
            Some((prev_op, prev_value)) if self.concurrent(env.id, prev_op) => {
                if prev_value == written {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    };
                }
                let conflict = ConflictRecord::new(
                    ConflictKind::StructuralFieldCollision {
                        winner: env.id,
                        loser: prev_op,
                        field: FieldPath("meter_sequence".to_string()),
                    },
                    vec![env.id, prev_op],
                    vec![TypedObjectId::Region(op.region)],
                );
                let cid = conflict.id;
                self.conflicts.insert(conflict);
                OperationEffect::Conflicted { conflict: cid }
            }
            _ => OperationEffect::Applied,
        };
        self.meter_change_chain
            .entry(key.clone())
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, written.clone());
        self.graph_apply_meter_change(op.region, &key.1, &written);
        effect
    }

    /// Applies a meter-change overwrite (or removal) to the region's default
    /// metric grid, keeping the sequence ordered by resolved position. A set
    /// on a region whose grid is `None` creates the grid; a removal that
    /// empties the sequence normalizes the slot back to `None` unless a
    /// whole-grid write (or the base) holds a grid value independently.
    fn graph_apply_meter_change(
        &mut self,
        region: RegionId,
        position: &MusicalPosition,
        change: &Option<MeterChange>,
    ) {
        let baseline_grid = self
            .metric_grid_chain
            .get(&region)
            .and_then(|chain| chain.current())
            .is_some_and(|grid| grid.is_some());
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == region) else {
            return;
        };
        let Some(content) = region.content.staff_based_mut() else {
            return;
        };
        match change {
            Some(meter) => {
                let grid = content
                    .default_metric_grid
                    .get_or_insert_with(MetricGrid::default);
                grid.meter_sequence
                    .retain(|existing| resolved_anchor_position(&existing.anchor) != *position);
                let index = grid
                    .meter_sequence
                    .iter()
                    .position(|existing| resolved_anchor_position(&existing.anchor) > *position)
                    .unwrap_or(grid.meter_sequence.len());
                grid.meter_sequence.insert(index, meter.clone());
            }
            None => {
                if let Some(grid) = content.default_metric_grid.as_mut() {
                    grid.meter_sequence
                        .retain(|existing| resolved_anchor_position(&existing.anchor) != *position);
                    if grid.meter_sequence.is_empty() && !baseline_grid {
                        content.default_metric_grid = None;
                    }
                }
            }
        }
    }

    /// Whether the scope's *resulting* tempo map is well-formed with `written`
    /// installed at `key` (operation_catalog §"Meter and Tempo Overwrites"):
    /// the carried segment's own start equals the operation's key, every
    /// segment's shape carries its end data, and resolved ends neither precede
    /// their own start nor overlap the next segment. Read purely from the
    /// tempo chains' current values (which seed from the base map), so
    /// graph-free and graph-aware reduction agree wherever both represent the
    /// scope.
    fn prospective_tempo_write_well_formed(
        &self,
        key: &(Option<RegionId>, MusicalPosition),
        written: &TempoSegment,
    ) -> bool {
        if resolved_anchor_position(&written.start) != key.1 {
            return false;
        }
        let mut segments: Vec<(MusicalPosition, &TempoSegment)> = Vec::new();
        for ((scope, position), chain) in &self.tempo_segment_chain {
            if scope != &key.0 || *position == key.1 {
                continue;
            }
            if let Some(Some(segment)) = chain.current() {
                segments.push((position.clone(), segment));
            }
        }
        segments.push((key.1.clone(), written));
        segments.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (index, (position, segment)) in segments.iter().enumerate() {
            if !tempo_segment_shape_well_formed(segment) {
                return false;
            }
            if let Some(end) = &segment.end {
                let resolved_end = resolved_anchor_position(end);
                if resolved_end < *position {
                    return false;
                }
                if let Some((next_position, _)) = segments.get(index + 1) {
                    if resolved_end > *next_position {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Sets, replaces, or removes the single tempo segment starting at the
    /// resolved position in the scoped tempo map — an LWW structural overwrite
    /// keyed by `(scope, resolved start)` (operation_catalog §"Meter and Tempo
    /// Overwrites"). A write that would malform the resulting map is refused
    /// (`TempoMapMalformed`).
    fn set_tempo_segment(
        &mut self,
        env: &OperationEnvelope,
        op: &SetTempoSegmentOp,
    ) -> OperationEffect {
        if let Some(region) = op.region {
            if !matches!(
                self.objects.get(&TypedObjectId::Region(region)),
                Some(ObjectState::Live)
            ) {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            }
        }
        let key = (op.region, op.resolved_start());
        if let Some(segment) = &op.segment {
            if !self.prospective_tempo_write_well_formed(&key, segment) {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TempoMapMalformed,
                    },
                };
            }
        }
        let prev = self
            .tempo_segment_chain
            .get(&key)
            .and_then(|chain| chain.last_write())
            .map(|write| (write.op, write.value.clone()));
        let effect = match prev {
            Some((prev_op, prev_value)) if self.concurrent(env.id, prev_op) => {
                if prev_value == op.segment {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    };
                }
                let affected = match op.region {
                    Some(region) => vec![TypedObjectId::Region(region)],
                    None => vec![],
                };
                let conflict = ConflictRecord::new(
                    ConflictKind::StructuralFieldCollision {
                        winner: env.id,
                        loser: prev_op,
                        field: FieldPath("tempo_segments".to_string()),
                    },
                    vec![env.id, prev_op],
                    affected,
                );
                let cid = conflict.id;
                self.conflicts.insert(conflict);
                OperationEffect::Conflicted { conflict: cid }
            }
            _ => OperationEffect::Applied,
        };
        self.tempo_segment_chain
            .entry(key.clone())
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, op.segment.clone());
        self.graph_apply_tempo_segment(op.region, &key.1, &op.segment);
        effect
    }

    /// Applies a tempo-segment overwrite (or removal) to the scoped map. A set
    /// on a region with no local map creates one; an empty, `initial`-less
    /// local map left behind by a removal normalizes back to `None` (an empty
    /// local map would shadow the score map instead of falling back to it).
    fn graph_apply_tempo_segment(
        &mut self,
        scope: Option<RegionId>,
        position: &MusicalPosition,
        segment: &Option<TempoSegment>,
    ) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        match scope {
            None => edit_tempo_map_segments(&mut score.tempo_map, position, segment),
            Some(region_id) => {
                let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == region_id)
                else {
                    return;
                };
                if segment.is_some() && region.local_tempo_map.is_none() {
                    region.local_tempo_map = Some(TempoMap::default());
                }
                if let Some(map) = region.local_tempo_map.as_mut() {
                    edit_tempo_map_segments(map, position, segment);
                }
                if region
                    .local_tempo_map
                    .as_ref()
                    .is_some_and(|map| map.initial.is_none() && map.segments.is_empty())
                {
                    region.local_tempo_map = None;
                }
            }
        }
    }

    /// Overwrites a staff instance's three inline layout advisories as a unit
    /// — an LWW *advisory* keyed by `staff_instance` (operation_catalog
    /// §SetStaffLayout): no conflicts; the latest write in canonical order
    /// wins. A present `instrument_override` must resolve to a live instrument
    /// under graph-aware reduction.
    fn set_staff_layout(
        &mut self,
        env: &OperationEnvelope,
        op: &SetStaffLayoutOp,
    ) -> OperationEffect {
        match self
            .objects
            .get(&TypedObjectId::StaffInstance(op.staff_instance))
        {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            Some(ObjectState::Live) => {}
        }
        if self.graph.is_some() {
            if let Some(instrument) = op.instrument_override {
                if !matches!(
                    self.objects.get(&TypedObjectId::Instrument(instrument)),
                    Some(ObjectState::Live)
                ) {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::PreconditionFailedUnderReduction {
                            reason: PreconditionFailureReason::TargetMissing,
                        },
                    };
                }
            }
        }
        let value: StaffLayoutValue = (
            op.instrument_override,
            op.staff_lines_override.clone(),
            op.visible,
        );
        self.staff_layout_chain
            .entry(op.staff_instance)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, value.clone());
        self.graph_set_staff_layout(op.staff_instance, &value);
        OperationEffect::Applied
    }

    fn graph_set_staff_layout(&mut self, instance_id: StaffInstanceId, value: &StaffLayoutValue) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        for region in &mut score.canvas.regions {
            if let Some(instances) = region.content.staff_instances_mut() {
                if let Some(instance) = instances.iter_mut().find(|i| i.id == instance_id) {
                    instance.instrument_override = value.0;
                    instance.staff_lines_override = value.1.clone();
                    instance.visible = value.2;
                    return;
                }
            }
        }
    }

    /// `Some(effect)` when `obj` cannot be deleted (missing, idempotent
    /// re-delete, or non-empty); `None` with the resolved minter when the
    /// empty-only delete may proceed.
    fn delete_precondition(
        &self,
        obj: TypedObjectId,
        env: &OperationEnvelope,
        has_live_children: bool,
    ) -> Result<OperationId, OperationEffect> {
        match self.objects.get(&obj) {
            None => Err(OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            }),
            Some(ObjectState::Tombstoned { .. }) => Err(OperationEffect::NoOp {
                reason: NoOpReason::AlreadyApplied,
            }),
            Some(ObjectState::Live) if has_live_children => Err(OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::ContainerNotEmpty,
                },
            }),
            Some(ObjectState::Live) => Ok(self.minted_by.get(&obj).copied().unwrap_or(env.id)),
        }
    }

    fn delete_region(&mut self, env: &OperationEnvelope, op: &DeleteRegionOp) -> OperationEffect {
        let robj = TypedObjectId::Region(op.region);
        let has_instances = self
            .region_instances
            .get(&op.region)
            .is_some_and(|s| !s.is_empty());
        let minted_by = match self.delete_precondition(robj, env, has_instances) {
            Ok(minter) => minter,
            Err(effect) => return effect,
        };
        self.objects.insert(
            robj,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by,
            },
        );
        self.region_instances.remove(&op.region);
        self.staff_based_regions.remove(&op.region);
        self.graph_delete_region(op.region);
        OperationEffect::Applied
    }

    fn delete_staff_instance(
        &mut self,
        env: &OperationEnvelope,
        op: &DeleteStaffInstanceOp,
    ) -> OperationEffect {
        let iobj = TypedObjectId::StaffInstance(op.staff_instance);
        let has_voices = self
            .instance_voices
            .get(&op.staff_instance)
            .is_some_and(|s| !s.is_empty());
        let minted_by = match self.delete_precondition(iobj, env, has_voices) {
            Ok(minter) => minter,
            Err(effect) => return effect,
        };
        self.objects.insert(
            iobj,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by,
            },
        );
        self.instance_voices.remove(&op.staff_instance);
        for set in self.region_instances.values_mut() {
            set.remove(&op.staff_instance);
        }
        self.graph_delete_staff_instance(op.staff_instance);
        OperationEffect::Applied
    }

    fn delete_voice(&mut self, env: &OperationEnvelope, op: &DeleteVoiceOp) -> OperationEffect {
        let vobj = TypedObjectId::Voice(op.voice);
        let has_events = self
            .voice_occupancy
            .get(&op.voice)
            .is_some_and(|e| !e.is_empty());
        let minted_by = match self.delete_precondition(vobj, env, has_events) {
            Ok(minter) => minter,
            Err(effect) => return effect,
        };
        self.objects.insert(
            vobj,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by,
            },
        );
        self.voice_occupancy.remove(&op.voice);
        for set in self.instance_voices.values_mut() {
            set.remove(&op.voice);
        }
        self.graph_delete_voice(op.voice);
        OperationEffect::Applied
    }

    // --- Group 3 graph mutations (reduce_onto only; no-op when graph is None). --

    fn graph_create_region(&mut self, region: &epiphany_core::Region) {
        if let Some(score) = self.graph.as_mut() {
            score.canvas.regions.push(region.clone());
        }
    }

    fn graph_create_staff_instance(&mut self, region: RegionId, instance: &StaffInstance) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        if let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == region) {
            let staff = instance.staff;
            if let Some(instances) = region.content.staff_instances_mut() {
                instances.push(instance.clone());
            }
            // Keep the region's staff extent listing exactly its manifested
            // staves (Chapter 5 RegionExtents).
            if !region.staff_extent.staves.contains(&staff) {
                region.staff_extent.staves.push(staff);
            }
        }
    }

    fn graph_create_voice(&mut self, staff_instance: StaffInstanceId, voice: &Voice) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        for region in &mut score.canvas.regions {
            if let Some(instances) = region.content.staff_instances_mut() {
                if let Some(instance) = instances.iter_mut().find(|i| i.id == staff_instance) {
                    instance.voices.push(voice.clone());
                    return;
                }
            }
        }
    }

    fn graph_delete_region(&mut self, region: RegionId) {
        if let Some(score) = self.graph.as_mut() {
            score.canvas.regions.retain(|r| r.id != region);
        }
    }

    fn graph_delete_staff_instance(&mut self, staff_instance: StaffInstanceId) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        for region in &mut score.canvas.regions {
            let Some(instances) = region.content.staff_instances_mut() else {
                continue;
            };
            let Some(removed_staff) = instances
                .iter()
                .find(|i| i.id == staff_instance)
                .map(|i| i.staff)
            else {
                continue;
            };
            instances.retain(|i| i.id != staff_instance);
            // Drop the staff from the extent if no remaining instance manifests
            // it (Chapter 5 RegionExtents).
            let still_used = instances.iter().any(|i| i.staff == removed_staff);
            if !still_used {
                region.staff_extent.staves.retain(|s| *s != removed_staff);
            }
        }
    }

    fn graph_delete_voice(&mut self, voice: VoiceId) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        for region in &mut score.canvas.regions {
            if let Some(instances) = region.content.staff_instances_mut() {
                for instance in instances {
                    instance.voices.retain(|v| v.id != voice);
                }
            }
        }
    }

    fn change_region_time_model(
        &mut self,
        env: &OperationEnvelope,
        op: &crate::payload::ChangeRegionTimeModelOp,
    ) -> OperationEffect {
        if let Some(winner) = self.region_migrator.get(&op.region).copied() {
            // Concurrent same-target migrations conflict. A causally-later
            // migration is an intentional second structural change and is
            // evaluated against the graph produced by the first.
            if !self.concurrent(env.id, winner) {
                self.migrated_regions.insert(op.region);
            } else {
                let conflict = ConflictRecord::new(
                    ConflictKind::StructuralFieldCollision {
                        winner,
                        loser: env.id,
                        field: FieldPath("time_model".to_string()),
                    },
                    vec![env.id, winner],
                    vec![TypedObjectId::Region(op.region)],
                );
                let cid = conflict.id;
                self.conflicts.insert(conflict);
                return OperationEffect::Conflicted { conflict: cid };
            }
        }
        let mut incompatible_events: BTreeSet<EventId> =
            op.declared_incompatible.iter().copied().collect();
        let mut graph_region_index = None;
        if let Some(score) = self.graph.as_ref() {
            let Some(region_index) = score
                .canvas
                .regions
                .iter()
                .position(|region| region.id == op.region)
            else {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            };
            graph_region_index = Some(region_index);
            let region = &score.canvas.regions[region_index];
            let event_ids: Vec<EventId> = region
                .staff_instances()
                .iter()
                .flat_map(|instance| &instance.voices)
                .flat_map(|voice| voice.events.iter().copied())
                .collect();

            for event_id in &event_ids {
                let Some(event) = score.events.get(*event_id) else {
                    incompatible_events.insert(*event_id);
                    continue;
                };
                let compatible = match &op.new_time_model {
                    RegionTimeModel::Metric(_) => matches!(
                        (event.position(), event.duration()),
                        (EventPosition::Musical(_), EventDuration::Musical(_))
                    ),
                    RegionTimeModel::Proportional(_) => matches!(
                        (event.position(), event.duration()),
                        (EventPosition::WallClock(_), EventDuration::WallClock(_))
                    ),
                    RegionTimeModel::Aleatoric(_) => true,
                };
                if !compatible {
                    incompatible_events.insert(*event_id);
                }
            }

            if let crate::payload::PositionRemapping::Reassign(remapping) = &op.remapping {
                let mapped: BTreeSet<EventId> = remapping.iter().map(|(event, _)| *event).collect();
                incompatible_events.extend(
                    event_ids
                        .iter()
                        .filter(|event| !mapped.contains(event))
                        .copied(),
                );
                if matches!(op.new_time_model, RegionTimeModel::Proportional(_)) {
                    // Reassign carries musical positions in the current
                    // prototype schema, so it cannot satisfy a proportional
                    // region's wall-clock coordinate discipline.
                    incompatible_events.extend(event_ids);
                }
            }
        }

        if !incompatible_events.is_empty() {
            let incompatible: Vec<TypedObjectId> = incompatible_events
                .into_iter()
                .map(TypedObjectId::Event)
                .collect();
            let mut affected = vec![TypedObjectId::Region(op.region)];
            affected.extend(incompatible.iter().copied());
            let conflict = ConflictRecord::new(
                ConflictKind::TimeModelMigrationFailure {
                    region: op.region,
                    incompatible_events: incompatible,
                },
                vec![env.id],
                affected,
            );
            let cid = conflict.id;
            self.conflicts.insert(conflict);
            return OperationEffect::Conflicted { conflict: cid };
        }

        if let Some(region_index) = graph_region_index {
            let score = self
                .graph
                .as_mut()
                .expect("a graph region index implies graph-aware reduction");
            if let crate::payload::PositionRemapping::Reassign(remapping) = &op.remapping {
                for (event, position) in remapping {
                    if let Some(value) = score.events.get_mut(*event) {
                        value.set_position(EventPosition::Musical(position.clone()));
                    }
                    for placements in self.voice_occupancy.values_mut() {
                        if let Some((stored_position, _, _)) = placements
                            .iter_mut()
                            .find(|(_, _, stored_event)| stored_event == event)
                        {
                            *stored_position = position.clone();
                        }
                    }
                }
            }
            // The value-typed payload (v1) carries the real target model, so the
            // region adopts it directly rather than rebuilding a default from a
            // discriminator tag.
            let region = &mut score.canvas.regions[region_index];
            region.time_model = op.new_time_model.clone();
        }
        self.migrated_regions.insert(op.region);
        self.region_migrator.insert(op.region, env.id);
        OperationEffect::Applied
    }

    fn resolve_conflict(
        &mut self,
        env: &OperationEnvelope,
        op: &crate::payload::ResolveConflictPayload,
    ) -> OperationEffect {
        use crate::conflict::ConflictResolutionState as RS;
        let existing_state = self
            .conflicts
            .get_mut(op.target)
            .map(|r| r.resolution_state);
        match existing_state {
            None => OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            },
            Some(RS::Unresolved) => {
                if let Some(rec) = self.conflicts.get_mut(op.target) {
                    // ResolutionAction::Dismiss selects the Dismissed state;
                    // every other action selects Resolved with that action
                    // applied (Pass 11, item 2.5).
                    rec.resolution_state = match op.action {
                        ResolutionAction::Dismiss => RS::Dismissed { by: env.id },
                        action => RS::Resolved { by: env.id, action },
                    };
                }
                OperationEffect::Applied
            }
            Some(RS::Resolved { by, action }) => {
                if action == op.action {
                    OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    }
                } else {
                    // Differing concurrent resolution → meta-conflict. The
                    // earlier resolve stands (its action materialized), so it
                    // is the record's winner; this op is the loser, and both
                    // resolvers are named as causes ("at least two for a true
                    // conflict", Chapter 6 §Conflict Records). A conflict
                    // record has no TypedObjectId, so `affected_objects`
                    // cannot name the contested conflict and stays empty.
                    let conflict = ConflictRecord::new(
                        ConflictKind::StructuralFieldCollision {
                            winner: by,
                            loser: env.id,
                            field: FieldPath("conflict_resolution".to_string()),
                        },
                        vec![by, env.id],
                        vec![],
                    );
                    let cid = conflict.id;
                    self.conflicts.insert(conflict);
                    OperationEffect::Conflicted { conflict: cid }
                }
            }
            Some(RS::Dismissed { .. }) => OperationEffect::NoOp {
                reason: NoOpReason::AlreadyApplied,
            },
        }
    }

    /// The recorded effect of a `ResolveEquivocation` (operation_catalog
    /// §"ResolveEquivocation"). The slot promotion itself happened in the
    /// set-level pre-pass ([`Reducer::run`] step 1b); this only records the
    /// per-resolve verdict, mirroring [`Reducer::resolve_conflict`]'s
    /// discipline: the governing (earliest-in-canonical-order) resolve is
    /// `Applied`; a later resolve naming the same candidate reduces
    /// idempotently; a later valid resolve naming a differing candidate is a
    /// `StructuralFieldCollision` meta-conflict on `equivocation_resolution`.
    /// A resolve whose target is not an equivocated slot, or whose chosen is
    /// not among the slot's candidates, is a precondition no-op
    /// (`TargetMissing` — the named target/candidate pair does not exist).
    fn resolve_equivocation(
        &mut self,
        env: &OperationEnvelope,
        op: &crate::payload::ResolveEquivocationPayload,
    ) -> OperationEffect {
        let precondition_noop = OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::TargetMissing,
            },
        };
        match self.equivocation_resolutions.get(&op.target) {
            // No governing resolve exists for the target: it is absent, holds
            // a Single slot, or is equivocated with no valid resolve — in
            // every case this resolve's precondition ("an Equivocated slot for
            // `target` with `chosen` among its candidates") failed, or it
            // would have governed.
            None => precondition_noop,
            Some((winner, chosen)) => {
                if env.id == *winner {
                    OperationEffect::Applied
                } else if op.chosen == *chosen {
                    // A later resolve naming the same candidate: idempotent.
                    OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    }
                } else if !self
                    .op_set
                    .slot(op.target)
                    .is_some_and(|slot| slot.candidates().any(|c| c == op.chosen))
                {
                    // A differing `chosen` that never named a real candidate
                    // is a failed precondition, not a contested resolution.
                    precondition_noop
                } else {
                    // Two valid resolves naming differing candidates: the
                    // governing (earlier) resolve stands as the record's
                    // winner; this op is the loser, both named as causes. A
                    // slot is not a TypedObjectId, so `affected_objects`
                    // stays empty (the ResolveConflict discipline).
                    let conflict = ConflictRecord::new(
                        ConflictKind::StructuralFieldCollision {
                            winner: *winner,
                            loser: env.id,
                            field: FieldPath("equivocation_resolution".to_string()),
                        },
                        vec![*winner, env.id],
                        vec![],
                    );
                    let cid = conflict.id;
                    self.conflicts.insert(conflict);
                    OperationEffect::Conflicted { conflict: cid }
                }
            }
        }
    }

    /// Forward compensating undo (operation_catalog §UndoTransaction): the
    /// minted-object tombstoning pass plus, per this revision, the
    /// value-restoration pass over every LWW write chain the target
    /// transaction wrote. `StrictInverse` refuses the whole undo if *either*
    /// part fails; `BestEffort` compensates what it cleanly can. A fully
    /// clean compensation is `Applied`; `AppliedWithRepair` carries only the
    /// tombstone repairs from minted objects.
    fn undo_transaction(
        &mut self,
        env: &OperationEnvelope,
        op: &UndoTransactionPayload,
    ) -> OperationEffect {
        let targets = self.tx_minted.get(&op.target).cloned().unwrap_or_default();
        let (restorations, superseded) = self.collect_restorations(op.target, &targets);
        if targets.is_empty() && restorations.is_empty() && superseded.is_empty() {
            // The transaction minted nothing and overwrote nothing this
            // reduction knows of (unknown, rolled back, or all its written
            // keys are gone): nothing to compensate.
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        match op.policy {
            UndoPolicy::StrictInverse | UndoPolicy::Cascade => {
                // A minted target already tombstoned: strict undo conflicts
                // (the pre-existing minted-object discipline).
                if let Some(stuck) = targets
                    .iter()
                    .find(|t| !matches!(self.objects.get(t), Some(ObjectState::Live)))
                    .copied()
                {
                    let conflict = ConflictRecord::new(
                        ConflictKind::TombstonedTarget {
                            target: stuck,
                            operation: env.id,
                        },
                        vec![env.id],
                        vec![stuck],
                    );
                    let cid = conflict.id;
                    self.conflicts.insert(conflict);
                    return OperationEffect::Conflicted { conflict: cid };
                }
                // A minted staff still manifested by a live instance, or a
                // minted time signature still referenced by a surviving meter
                // change: tombstoning it would strand the reference
                // (operation_catalog §CreateStaff undo semantics).
                if let Some((blocked, referencer)) = targets
                    .iter()
                    .find_map(|t| self.undo_strand_block(t, &targets, &restorations))
                {
                    let conflict = ConflictRecord::new(
                        ConflictKind::TransactionConflict {
                            transaction: op.target,
                            failed_members: vec![env.id],
                        },
                        vec![env.id],
                        vec![blocked, referencer],
                    );
                    let cid = conflict.id;
                    self.conflicts.insert(conflict);
                    return OperationEffect::Conflicted { conflict: cid };
                }
                // A written key superseded by a later writer: strict undo
                // refuses the whole compensation, naming the undo and the
                // (canonically first) superseding writer.
                if let Some(by) = superseded.first().copied() {
                    let conflict = ConflictRecord::new(
                        ConflictKind::TransactionConflict {
                            transaction: op.target,
                            failed_members: vec![env.id],
                        },
                        vec![env.id, by],
                        vec![],
                    );
                    let cid = conflict.id;
                    self.conflicts.insert(conflict);
                    return OperationEffect::Conflicted { conflict: cid };
                }
                let repairs = self.tombstone_undo_targets(env, &targets);
                self.apply_restorations(env, restorations);
                if repairs.is_empty() {
                    OperationEffect::Applied
                } else {
                    OperationEffect::AppliedWithRepair { repairs }
                }
            }
            UndoPolicy::BestEffort => {
                // Tombstone the still-live, non-stranding mints; restore the
                // still-last-written keys; skip the rest.
                let tombstonable: Vec<TypedObjectId> = targets
                    .iter()
                    .filter(|t| matches!(self.objects.get(t), Some(ObjectState::Live)))
                    .filter(|t| self.undo_strand_block(t, &targets, &restorations).is_none())
                    .copied()
                    .collect();
                let repairs = self.tombstone_undo_targets(env, &tombstonable);
                self.apply_restorations(env, restorations);
                if repairs.is_empty() {
                    OperationEffect::Applied
                } else {
                    OperationEffect::AppliedWithRepair { repairs }
                }
            }
        }
    }

    /// Tombstones the minted objects of an undone transaction and materializes
    /// the graph-side removals, returning the `CascadeDeleted` repair records.
    fn tombstone_undo_targets(
        &mut self,
        env: &OperationEnvelope,
        targets: &[TypedObjectId],
    ) -> Vec<RepairRecord> {
        let mut repairs = Vec::new();
        for t in targets {
            let minter = self.minted_by.get(t).copied().unwrap_or(env.id);
            self.objects.insert(
                *t,
                ObjectState::Tombstoned {
                    deleted_by: env.id,
                    minted_by: minter,
                },
            );
            repairs.push(RepairRecord {
                kind: RepairKind::CascadeDeleted,
                target: *t,
            });
        }
        repairs.extend(self.materialize_graph_tombstones(env, targets));
        repairs
    }

    /// `Some((blocked, referencer))` when tombstoning `target` under undo
    /// would strand a live reference: a minted `Staff` still manifested by a
    /// live staff instance (operation_catalog §CreateStaff), or a minted
    /// `TimeSignature` still referenced by a meter change that survives the
    /// restoration pass. References held by objects the same undo tombstones
    /// do not block.
    fn undo_strand_block(
        &self,
        target: &TypedObjectId,
        targets: &[TypedObjectId],
        restorations: &[ValueRestoration],
    ) -> Option<(TypedObjectId, TypedObjectId)> {
        match target {
            TypedObjectId::Staff(staff) => {
                self.instance_staff
                    .iter()
                    .find_map(|(instance, manifested)| {
                        let iobj = TypedObjectId::StaffInstance(*instance);
                        (manifested == staff
                            && !targets.contains(&iobj)
                            && matches!(self.objects.get(&iobj), Some(ObjectState::Live)))
                        .then_some((*target, iobj))
                    })
            }
            TypedObjectId::TimeSignature(id) => {
                self.meter_change_chain
                    .iter()
                    .find_map(|((region, position), chain)| {
                        // The prospective post-undo value at this key: the
                        // restoration's value where one applies, else the
                        // chain's current value.
                        let prospective: Option<MeterChange> = restorations
                            .iter()
                            .find_map(|restoration| match restoration {
                                ValueRestoration::MeterChange {
                                    region: r,
                                    position: p,
                                    value,
                                } if r == region && p == position => Some(value.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| chain.current().cloned().flatten());
                        prospective.and_then(|meter| {
                            (meter.time_signature == *id)
                                .then_some((*target, TypedObjectId::Region(*region)))
                        })
                    })
            }
            _ => None,
        }
    }

    /// Walks every write chain and collects, for the target transaction: the
    /// keys still last-written by it (with their chain-predecessor
    /// restorations) and the operations that superseded its other writes
    /// (sorted, deduplicated). Keys whose owning object is tombstoned — or is
    /// itself one of the transaction's mints, about to be tombstoned by this
    /// undo — are skipped entirely: there is no live slot to restore.
    fn collect_restorations(
        &self,
        tx: TransactionId,
        targets: &[TypedObjectId],
    ) -> (Vec<ValueRestoration>, Vec<OperationId>) {
        let mut restorations = Vec::new();
        let mut superseded: Vec<OperationId> = Vec::new();
        let slot_live = |obj: TypedObjectId| {
            matches!(self.objects.get(&obj), Some(ObjectState::Live)) && !targets.contains(&obj)
        };

        for (event, chain) in &self.event_modify_chain {
            if !slot_live(TypedObjectId::Event(*event)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::Event {
                        event: *event,
                        value: predecessor.map(Predecessor::into_value),
                    })
                }
            }
        }
        for (pitch, chain) in &self.pitch_modify_chain {
            if !slot_live(TypedObjectId::Pitch(*pitch)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::Pitch {
                        pitch: *pitch,
                        value: predecessor.map(Predecessor::into_value),
                    })
                }
            }
        }
        for (pitch, chain) in &self.respell_chain {
            if !slot_live(TypedObjectId::Pitch(*pitch)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::Spelling {
                        pitch: *pitch,
                        predecessor,
                    })
                }
            }
        }
        for (id, chain) in &self.cross_cutting_modify_chain {
            if !slot_live(*id) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::CrossCutting {
                        id: *id,
                        value: predecessor.map(Predecessor::into_value),
                    })
                }
            }
        }
        match self.metadata_chain.undo_verdict(tx) {
            ChainUndoVerdict::NotWritten => {}
            ChainUndoVerdict::Superseded { by } => superseded.push(by),
            ChainUndoVerdict::Restore(predecessor) => {
                restorations.push(ValueRestoration::Metadata {
                    value: predecessor.map(Predecessor::into_value),
                })
            }
        }
        for (region, chain) in &self.metric_grid_chain {
            if !slot_live(TypedObjectId::Region(*region)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    // Flattened: no predecessor and a cleared-grid predecessor
                    // both restore "no grid".
                    restorations.push(ValueRestoration::MetricGrid {
                        region: *region,
                        value: match predecessor {
                            Some(p) => p.into_value(),
                            None => None,
                        },
                    })
                }
            }
        }
        for ((region, position), chain) in &self.meter_change_chain {
            if !slot_live(TypedObjectId::Region(*region)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::MeterChange {
                        region: *region,
                        position: position.clone(),
                        value: match predecessor {
                            Some(p) => p.into_value(),
                            None => None,
                        },
                    })
                }
            }
        }
        for ((scope, position), chain) in &self.tempo_segment_chain {
            if let Some(region) = scope {
                if !slot_live(TypedObjectId::Region(*region)) {
                    continue;
                }
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::TempoSegment {
                        region: *scope,
                        position: position.clone(),
                        value: match predecessor {
                            Some(p) => p.into_value(),
                            None => None,
                        },
                    })
                }
            }
        }
        for (instance, chain) in &self.staff_layout_chain {
            if !slot_live(TypedObjectId::StaffInstance(*instance)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::StaffLayout {
                        instance: *instance,
                        value: predecessor.map(Predecessor::into_value),
                    })
                }
            }
        }
        for ((region, position), chain) in &self.break_chain {
            if !slot_live(TypedObjectId::Region(*region)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::SystemBreak {
                        region: *region,
                        position: position.clone(),
                        predecessor,
                    })
                }
            }
        }
        for ((region, position), chain) in &self.page_break_chain {
            if !slot_live(TypedObjectId::Region(*region)) {
                continue;
            }
            match chain.undo_verdict(tx) {
                ChainUndoVerdict::NotWritten => {}
                ChainUndoVerdict::Superseded { by } => superseded.push(by),
                ChainUndoVerdict::Restore(predecessor) => {
                    restorations.push(ValueRestoration::PageBreak {
                        region: *region,
                        position: position.clone(),
                        predecessor,
                    })
                }
            }
        }
        superseded.sort();
        superseded.dedup();
        (restorations, superseded)
    }

    /// Applies the collected restorations to the bookkeeping and the graph,
    /// recording each restored *value* into its chain as a new write by the
    /// undo operation (so a later undo sees the restoration as the key's last
    /// writer — the pinned undo-of-undo discipline; see DECISIONS.md). An
    /// absence restoration (no predecessor at all) leaves the chain
    /// unchanged: a repeated undo of the same transaction re-restores absence
    /// idempotently.
    fn apply_restorations(&mut self, env: &OperationEnvelope, restorations: Vec<ValueRestoration>) {
        for restoration in restorations {
            match restoration {
                ValueRestoration::Event { event, value } => {
                    if let Some(value) = value {
                        self.apply_event_value(&value);
                        self.event_modify_chain
                            .entry(event)
                            .or_insert_with(WriteChain::new)
                            .record(env.id, env.transaction, value);
                    }
                }
                ValueRestoration::Pitch { pitch, value } => {
                    if let Some(value) = value {
                        self.graph_modify_pitch(pitch, &value);
                        self.pitch_modify_chain
                            .entry(pitch)
                            .or_insert_with(WriteChain::new)
                            .record(env.id, env.transaction, value);
                    }
                }
                ValueRestoration::Spelling { pitch, predecessor } => match predecessor {
                    Some(Predecessor::Write(spelling)) => {
                        self.spellings.insert(pitch, spelling.clone());
                        self.respell_chain
                            .entry(pitch)
                            .or_insert_with(WriteChain::new)
                            .record(env.id, env.transaction, spelling.clone());
                        self.graph_respell_pitch(pitch, &spelling);
                    }
                    Some(Predecessor::Base(spelling)) => {
                        // The ledger returns to key-absence (the base state
                        // lives in the graph); the graph attachment restores
                        // the base value.
                        self.spellings.remove(&pitch);
                        self.respell_chain
                            .entry(pitch)
                            .or_insert_with(WriteChain::new)
                            .record(env.id, env.transaction, spelling.clone());
                        self.graph_respell_pitch(pitch, &spelling);
                    }
                    None => {
                        self.spellings.remove(&pitch);
                        self.graph_remove_respell(pitch);
                    }
                },
                ValueRestoration::CrossCutting { id, value } => {
                    if let Some(value) = value {
                        self.structures.insert(id, value.endpoints());
                        self.graph_modify_cross_cutting(&value);
                        self.cross_cutting_modify_chain
                            .entry(id)
                            .or_insert_with(WriteChain::new)
                            .record(env.id, env.transaction, value);
                    }
                }
                ValueRestoration::Metadata { value } => {
                    if let Some(value) = value {
                        if let Some(score) = self.graph.as_mut() {
                            score.metadata = value.clone();
                        }
                        self.metadata_chain.record(env.id, env.transaction, value);
                    }
                }
                ValueRestoration::MetricGrid { region, value } => {
                    self.metric_grid_chain
                        .entry(region)
                        .or_insert_with(WriteChain::new)
                        .record(env.id, env.transaction, value.clone());
                    self.graph_set_metric_grid(region, &value);
                }
                ValueRestoration::MeterChange {
                    region,
                    position,
                    value,
                } => {
                    self.meter_change_chain
                        .entry((region, position.clone()))
                        .or_insert_with(WriteChain::new)
                        .record(env.id, env.transaction, value.clone());
                    self.graph_apply_meter_change(region, &position, &value);
                }
                ValueRestoration::TempoSegment {
                    region,
                    position,
                    value,
                } => {
                    self.tempo_segment_chain
                        .entry((region, position.clone()))
                        .or_insert_with(WriteChain::new)
                        .record(env.id, env.transaction, value.clone());
                    self.graph_apply_tempo_segment(region, &position, &value);
                }
                ValueRestoration::StaffLayout { instance, value } => {
                    if let Some(value) = value {
                        self.graph_set_staff_layout(instance, &value);
                        self.staff_layout_chain
                            .entry(instance)
                            .or_insert_with(WriteChain::new)
                            .record(env.id, env.transaction, value);
                    }
                }
                ValueRestoration::SystemBreak {
                    region,
                    position,
                    predecessor,
                } => self.restore_break(env, region, position, predecessor, false),
                ValueRestoration::PageBreak {
                    region,
                    position,
                    predecessor,
                } => self.restore_break(env, region, position, predecessor, true),
            }
        }
    }

    /// Restores one user-break key: a write predecessor re-enters the ledger
    /// map; a base predecessor returns the map to key-absence while the graph
    /// restores the base anchor; no predecessor removes the key from map and
    /// graph alike.
    fn restore_break(
        &mut self,
        env: &OperationEnvelope,
        region: RegionId,
        position: MusicalPosition,
        predecessor: Option<Predecessor<(TimeAnchor, bool)>>,
        page: bool,
    ) {
        let key = (region, position.clone());
        match predecessor {
            Some(Predecessor::Write((anchor, present))) => {
                if page {
                    self.page_breaks.insert(key.clone(), present);
                    self.page_break_chain
                        .entry(key)
                        .or_insert_with(WriteChain::new)
                        .record(env.id, env.transaction, (anchor.clone(), present));
                } else {
                    self.breaks.insert(key.clone(), present);
                    self.break_chain
                        .entry(key)
                        .or_insert_with(WriteChain::new)
                        .record(env.id, env.transaction, (anchor.clone(), present));
                }
                self.graph_apply_break(region, &anchor, present, page);
            }
            Some(Predecessor::Base((anchor, present))) => {
                if page {
                    self.page_breaks.remove(&key);
                    self.page_break_chain
                        .entry(key)
                        .or_insert_with(WriteChain::new)
                        .record(env.id, env.transaction, (anchor.clone(), present));
                } else {
                    self.breaks.remove(&key);
                    self.break_chain
                        .entry(key)
                        .or_insert_with(WriteChain::new)
                        .record(env.id, env.transaction, (anchor.clone(), present));
                }
                self.graph_apply_break(region, &anchor, present, page);
            }
            None => {
                if page {
                    self.page_breaks.remove(&key);
                } else {
                    self.breaks.remove(&key);
                }
                self.graph_clear_break(region, &position, page);
            }
        }
    }

    fn graph_apply_break(
        &mut self,
        region: RegionId,
        anchor: &TimeAnchor,
        present: bool,
        page: bool,
    ) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        if let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == region) {
            if let Some(content) = region.content.staff_based_mut() {
                let list = if page {
                    &mut content.user_page_breaks
                } else {
                    &mut content.user_system_breaks
                };
                apply_break_lww(list, anchor, present);
            }
        }
    }

    fn graph_clear_break(&mut self, region: RegionId, position: &MusicalPosition, page: bool) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        if let Some(region) = score.canvas.regions.iter_mut().find(|r| r.id == region) {
            if let Some(content) = region.content.staff_based_mut() {
                let list = if page {
                    &mut content.user_page_breaks
                } else {
                    &mut content.user_system_breaks
                };
                list.retain(|existing| resolved_anchor_position(existing) != *position);
            }
        }
    }

    /// Removes the user-chosen explicit spelling attachment for `pitch` — the
    /// inverse of the attachment `graph_respell_pitch` installs, used when a
    /// respell restoration has no predecessor (the operation introduced the
    /// first spelling).
    fn graph_remove_respell(&mut self, pitch: PitchId) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        score.spelling_attachments.retain(|a| {
            !(a.layer.is_none()
                && matches!(a.source, SpellingSource::UserChosen)
                && matches!(&a.scope, SpellingScope::Pitch(p) if *p == pitch)
                && matches!(a.directive, SpellingDirective::Explicit(_)))
        });
    }

    // --- Group 1 (M2): event & pitch leaf-field ops. ------------------------
    //
    // The modify/transpose ops follow respell's field-overwrite discipline but
    // do NOT store the resolved value in MaterializedState (an Event/Pitch is a
    // graph object, not a bookkeeping-owned annotation like a spelling): their
    // canonical footprint is the effect-log entry plus, on a concurrent differing
    // write, a StructuralFieldCollision. The resolved value is materialized in the
    // graph by reduce_onto. The LWW key/diff uses `last_*_modify` working state.

    /// A `SYSTEM_DERIVED` pitch's intrinsic content is immutable under
    /// reduction (Pass 12, P12-K3; core spec Ch5 §System-Derived
    /// Identifiers): its identifier is content-derived, so an in-place
    /// rewrite would invalidate the derivation. The check compares the
    /// replacement value's canonical pitch bytes against the id's
    /// *registered derivation inputs* — the same `system_mints` registry the
    /// collision pre-walk maintains (base-seeded occupants plus op mints),
    /// so `reduce()` and `reduce_onto()` agree wherever the registry holds
    /// the entry. An unregistered id (base-free reduction over a pitch the
    /// base seeded) is unverifiable and passes, like the other
    /// graph-aware-only preconditions.
    ///
    /// Known residue (filed as a Pass-13 candidate; see DECISIONS.md): a
    /// system pitch *introduced into the graph by a ModifyEvent replacement*
    /// (never minted — the pre-walk deliberately excludes ModifyEvent) gets
    /// this verdict only after a snapshot re-seeds the registry from the
    /// base graph; in-session it reads `TargetMissing` instead. That
    /// checkpoint-cut asymmetry for ModifyEvent-introduced content predates
    /// this precondition (pre-K3 the same split read `TargetMissing` vs a
    /// silent `Applied` rewrite) and is a ModifyEvent-introduction question,
    /// not a K3 one.
    fn system_derived_rewrite(&self, id: PitchId, value: &Pitch) -> bool {
        if id.replica() != ReplicaId::SYSTEM_DERIVED {
            return false;
        }
        match self.system_mints.get(&(ObjectKind::Pitch, id.counter())) {
            Some((inputs, _)) => inputs.0 != canonical_pitch_bytes(value),
            None => false,
        }
    }

    fn modify_event(&mut self, env: &OperationEnvelope, op: &ModifyEventOp) -> OperationEffect {
        let event_id = op.event_id();
        let ev_obj = TypedObjectId::Event(event_id);
        match self.objects.get(&ev_obj) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            Some(ObjectState::Live) => {}
        }
        // Identity precondition (P12-K3) before the placement precondition:
        // a replacement value that rewrites a system-derived pitch's
        // intrinsic content in place is refused outright.
        let mut carried = Vec::new();
        op.event.collect_identified_pitches(&mut carried);
        if carried
            .iter()
            .any(|ip| self.system_derived_rewrite(ip.id, &ip.pitch))
        {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::SystemDerivedContentImmutable,
                },
            };
        }
        // A `ModifyEvent` that moves a metric event's span (a trim or move) is now
        // materialized, but it must keep invariant 3 (`VoiceEventsSortedNonOverlap`):
        // refuse a move onto another live event in the voice, or one with a
        // non-positive span, rather than skip it silently (which would log a clean op
        // that never took effect). The verdict reads `voice_occupancy`, the
        // graph-independent index, so `reduce()` and `reduce_onto()` agree on it.
        let placement = self.metric_placement_verdict(&op.event);
        if matches!(placement, PlacementVerdict::Refused) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::EventDurationInvalid,
                },
            };
        }
        let prev = self
            .event_modify_chain
            .get(&event_id)
            .and_then(|chain| chain.last_write())
            .map(|write| (write.op, write.value.clone()));
        let effect = match prev {
            Some((prev_op, prev_event)) if self.concurrent(env.id, prev_op) => {
                if prev_event == op.event {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    };
                }
                // Later in canonical order wins and materializes; the earlier op
                // is the loser. (Winner carries the Conflicted tag; see DECISIONS.)
                let conflict = ConflictRecord::new(
                    ConflictKind::StructuralFieldCollision {
                        winner: env.id,
                        loser: prev_op,
                        field: FieldPath("event".to_string()),
                    },
                    vec![env.id, prev_op],
                    vec![ev_obj],
                );
                let cid = conflict.id;
                self.conflicts.insert(conflict);
                OperationEffect::Conflicted { conflict: cid }
            }
            // First modify, or a causally-ordered intentional overwrite.
            _ => OperationEffect::Applied,
        };
        self.event_modify_chain
            .entry(event_id)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, op.event.clone());
        self.apply_event_value(&op.event);
        effect
    }

    /// Applies an event *value* (a modify's replacement, or an undo's restored
    /// predecessor) to the graph and the occupancy index. A move materializes
    /// only when it is a sanctioned metric move (`Moved`) *and* the value is
    /// well-formed — so the graph and the occupancy index move together. A
    /// malformed (empty) pitched value is not materialized in the graph
    /// (`graph_replace_event` skips it), so it must not move occupancy either;
    /// a refused or non-metric placement leaves placement untouched (only
    /// same-placement field edits apply).
    fn apply_event_value(&mut self, value: &Event) {
        let placement = self.metric_placement_verdict(value);
        let materialize_move = matches!(placement, PlacementVerdict::Moved { .. })
            && !matches!(value, Event::Pitched(pe) if !pe.is_well_formed());
        self.graph_replace_event(value, materialize_move);
        // Keep the voice-occupancy index in step with a materialized move, so a later
        // insert sees the freed/changed span (the same index its overlap check reads).
        if materialize_move {
            if let PlacementVerdict::Moved {
                voice,
                position,
                duration,
            } = placement
            {
                if let Some(events) = self.voice_occupancy.get_mut(&voice) {
                    for slot in events.iter_mut().filter(|slot| slot.2 == value.id()) {
                        slot.0 = position.clone();
                        slot.1 = duration.clone();
                    }
                }
            }
        }
    }

    /// The verdict on a [`ModifyEvent`](OperationKind::ModifyEvent)'s placement: does
    /// it move the event's metric span, and if so does the move keep invariant 3
    /// (`VoiceEventsSortedNonOverlap`)? Read from `voice_occupancy` — the canonical,
    /// graph-independent placement index — so the verdict is identical with or without
    /// a base graph.
    fn metric_placement_verdict(&self, new_event: &Event) -> PlacementVerdict {
        let event_id = new_event.id();
        let Some((voice, current_position, current_duration)) =
            self.voice_occupancy.iter().find_map(|(voice, events)| {
                events
                    .iter()
                    .find(|(_, _, event)| *event == event_id)
                    .map(|(position, duration, _)| (*voice, position.clone(), duration.clone()))
            })
        else {
            // The event has no metric occupancy entry (untracked or non-metric):
            // nothing to materialize or refuse here.
            return PlacementVerdict::Unchanged;
        };
        let (EventPosition::Musical(new_position), EventDuration::Musical(new_duration)) =
            (new_event.position(), new_event.duration())
        else {
            // A non-metric placement is left deferred, neither moved nor refused.
            return PlacementVerdict::Unchanged;
        };
        if *new_position == current_position && *new_duration == current_duration {
            return PlacementVerdict::Unchanged;
        }
        if !new_duration.is_positive() {
            return PlacementVerdict::Refused;
        }
        let overlaps = self.voice_occupancy.get(&voice).is_some_and(|events| {
            events.iter().any(|(position, duration, event)| {
                *event != event_id
                    && intervals_overlap(new_position, new_duration, position, duration)
            })
        });
        if overlaps {
            PlacementVerdict::Refused
        } else {
            PlacementVerdict::Moved {
                voice,
                position: new_position.clone(),
                duration: new_duration.clone(),
            }
        }
    }

    /// Re-sorts `voice`'s graph event list by ascending position (id-tiebroken), the
    /// same order an insert maintains — run after a materialized placement change so
    /// the voice stays sorted (invariant 3). A no-op when the graph is absent.
    fn resort_voice(&mut self, voice: VoiceId) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        let Some((region_index, instance_index, voice_index)) = graph_voice_location(score, voice)
        else {
            return;
        };
        let mut ordered = score.canvas.regions[region_index].staff_instances()[instance_index]
            .voices[voice_index]
            .events
            .clone();
        ordered.sort_by(|a, b| {
            let a_position = score.events.get(*a).map(Event::position);
            let b_position = score.events.get(*b).map(Event::position);
            match (a_position, b_position) {
                (
                    Some(EventPosition::Musical(a_position)),
                    Some(EventPosition::Musical(b_position)),
                ) => a_position.cmp(b_position).then_with(|| a.cmp(b)),
                _ => a.cmp(b),
            }
        });
        score.canvas.regions[region_index]
            .content
            .staff_instances_mut()
            .expect("the voice was located in a staff-based instance")[instance_index]
            .voices[voice_index]
            .events = ordered;
    }

    fn transpose(&mut self, _env: &OperationEnvelope, op: &TransposeOp) -> OperationEffect {
        // Precondition: a target that never entered canonical state is a
        // dangling reference — the whole operation refuses. Tombstoned targets
        // are *skipped* per the catalog's re-anchoring rule ("the transpose
        // applies only to live pitches", Operation Catalog §Transpose); the
        // shift still applies to the remaining live targets. Transpose is
        // order-dependent (transpositions do not commute); its canonical
        // footprint is the effect-log entry. The transposed values are
        // materialized in the graph.
        if op
            .targets
            .iter()
            .any(|pitch| !self.objects.contains_key(&TypedObjectId::Pitch(*pitch)))
        {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        let live: Vec<PitchId> = op
            .targets
            .iter()
            .copied()
            .filter(|pitch| {
                matches!(
                    self.objects.get(&TypedObjectId::Pitch(*pitch)),
                    Some(ObjectState::Live)
                )
            })
            .collect();
        if live.is_empty() {
            // Every target was tombstoned by a causally-prior delete: the
            // skip-all case degenerates to no effect.
            return OperationEffect::NoOp {
                reason: NoOpReason::TargetTombstoned,
            };
        }
        // P12-K3: a SYSTEM_DERIVED pitch's intrinsic content is immutable —
        // an in-place alteration shift would desynchronize the content from
        // the id's derivation inputs (and from the `system_mints` registry).
        // System-derived targets are *skipped* like tombstoned ones (the
        // shift still applies to the remaining targets); a transpose whose
        // live targets are all system-derived reduces as a precondition
        // no-op. The filter reads only the id's namespace, so base-free and
        // graph-aware reduction agree.
        let mutable: Vec<PitchId> = live
            .into_iter()
            .filter(|pitch| pitch.replica() != ReplicaId::SYSTEM_DERIVED)
            .collect();
        if mutable.is_empty() {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::SystemDerivedContentImmutable,
                },
            };
        }
        for pitch in mutable {
            self.graph_transpose_pitch(pitch, op.chromatic_steps);
        }
        OperationEffect::Applied
    }

    fn insert_identified_pitch(
        &mut self,
        env: &OperationEnvelope,
        op: &InsertIdentifiedPitchOp,
    ) -> OperationEffect {
        let pitch_id = op.pitch_id();
        let p_obj = TypedObjectId::Pitch(pitch_id);
        // The target event must be live; the pitch id must be fresh.
        if !matches!(
            self.objects.get(&TypedObjectId::Event(op.event)),
            Some(ObjectState::Live)
        ) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        match self.objects.get(&p_obj) {
            Some(ObjectState::Live) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            None => {}
        }
        self.objects.insert(p_obj, ObjectState::Live);
        self.minted_by.insert(p_obj, env.id);
        self.note_minted(env, p_obj);
        // Seed the pitch's write chain with the minted value, so a later
        // modify's chain-predecessor is the inserted state.
        self.pitch_modify_chain
            .entry(pitch_id)
            .or_insert_with(WriteChain::new)
            .seed(op.pitch.pitch.clone());
        self.event_pitches
            .entry(op.event)
            .or_default()
            .push(pitch_id);
        self.graph_insert_pitch(op.event, &op.pitch);
        OperationEffect::Applied
    }

    fn delete_identified_pitch(
        &mut self,
        env: &OperationEnvelope,
        op: &DeleteIdentifiedPitchOp,
    ) -> OperationEffect {
        let p_obj = TypedObjectId::Pitch(op.pitch);
        let minted_by = match self.objects.get(&p_obj) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied,
                }
            }
            Some(ObjectState::Live) => self.minted_by.get(&p_obj).copied().unwrap_or(env.id),
        };
        self.objects.insert(
            p_obj,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by,
            },
        );
        for pitches in self.event_pitches.values_mut() {
            pitches.retain(|p| *p != op.pitch);
        }
        self.graph_delete_pitch(op.pitch);
        OperationEffect::Applied
    }

    fn modify_identified_pitch(
        &mut self,
        env: &OperationEnvelope,
        op: &ModifyIdentifiedPitchOp,
    ) -> OperationEffect {
        let p_obj = TypedObjectId::Pitch(op.pitch);
        match self.objects.get(&p_obj) {
            None => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                }
            }
            Some(ObjectState::Tombstoned { .. }) => {
                return OperationEffect::NoOp {
                    reason: NoOpReason::TargetTombstoned,
                }
            }
            Some(ObjectState::Live) => {}
        }
        // Identity precondition (P12-K3): a system-derived pitch's intrinsic
        // content is immutable in place.
        if self.system_derived_rewrite(op.pitch, &op.value) {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::SystemDerivedContentImmutable,
                },
            };
        }
        let prev = self
            .pitch_modify_chain
            .get(&op.pitch)
            .and_then(|chain| chain.last_write())
            .map(|write| (write.op, write.value.clone()));
        let effect = match prev {
            Some((prev_op, prev_value)) if self.concurrent(env.id, prev_op) => {
                if prev_value == op.value {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    };
                }
                let conflict = ConflictRecord::new(
                    ConflictKind::StructuralFieldCollision {
                        winner: env.id,
                        loser: prev_op,
                        field: FieldPath("pitch".to_string()),
                    },
                    vec![env.id, prev_op],
                    vec![p_obj],
                );
                let cid = conflict.id;
                self.conflicts.insert(conflict);
                OperationEffect::Conflicted { conflict: cid }
            }
            _ => OperationEffect::Applied,
        };
        self.pitch_modify_chain
            .entry(op.pitch)
            .or_insert_with(WriteChain::new)
            .record(env.id, env.transaction, op.value.clone());
        self.graph_modify_pitch(op.pitch, &op.value);
        effect
    }

    // --- Group 1 graph mutations (reduce_onto only; no-op when graph is None). --

    /// Applies a `ModifyEvent`'s value to the graph. `materialize_move` is the caller's
    /// sanction (from [`Self::metric_placement_verdict`]) that a *placement* change is a
    /// valid metric move and should be applied + the voice re-sorted; when it is false a
    /// placement change is deferred (a non-metric move, a no-occupancy event, or a
    /// refused move), leaving only same-placement field edits to apply. Keeping
    /// materialization gated on this single sanction is what holds the graph and the
    /// `voice_occupancy` index in agreement.
    fn graph_replace_event(&mut self, new_event: &Event, materialize_move: bool) {
        let placement_changed;
        let voice;
        {
            let Some(score) = self.graph.as_mut() else {
                return;
            };
            // A ModifyEvent carrying a malformed (empty) pitched event must not
            // corrupt the arena: `get_mut` bypasses `insert`'s well-formedness guard,
            // so an empty chord would only be caught later by `check_invariants`.
            // Skip the graph replace in that case (bookkeeping still records it).
            if let Event::Pitched(pe) = new_event {
                if !pe.is_well_formed() {
                    return;
                }
            }
            let Some(existing) = score.events.get_mut(new_event.id()) else {
                return;
            };
            placement_changed = new_event.position() != existing.position()
                || new_event.duration() != existing.duration();
            // A placement change is materialized (and the voice re-sorted below) only
            // when the caller sanctioned it as a valid metric move; otherwise it is
            // deferred, leaving the LWW bookkeeping to record the modify. Same-placement
            // field edits always apply, preserving the original voice membership.
            if placement_changed && !materialize_move {
                return;
            }
            voice = existing.voice();
            let mut replacement = new_event.clone();
            replacement.set_voice(voice);
            *existing = replacement;
        }
        if placement_changed {
            self.resort_voice(voice);
        }
    }

    fn graph_event_of_pitch(score: &Score, pitch: PitchId) -> Option<EventId> {
        score.events.iter().find_map(|event| {
            let mut ips = Vec::new();
            event.collect_identified_pitches(&mut ips);
            ips.iter().any(|ip| ip.id == pitch).then(|| event.id())
        })
    }

    fn graph_insert_pitch(&mut self, event: EventId, pitch: &epiphany_core::IdentifiedPitch) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        let Some(slot) = score.events.get_mut(event) else {
            return;
        };
        if let Event::Pitched(pe) = slot {
            if !pe.pitches.iter().any(|ip| ip.id == pitch.id) {
                pe.pitches.push(pitch.clone());
            }
            return;
        }
        // Adding a pitch to a rest turns the rest into a note — the dual of a
        // last-pitch delete (below). Without this, the bookkeeping mints the
        // pitch live while the graph silently drops it (a non-pitched slot has
        // no pitch list), so the two would diverge.
        if let Event::Rest(rest) = slot {
            let replacement = epiphany_core::PitchedEvent {
                id: rest.id,
                voice: rest.voice,
                position: rest.position.clone(),
                duration: rest.duration.clone(),
                pitches: vec![pitch.clone()],
                articulations: Vec::new(),
                dynamic: None,
                ornaments: Vec::new(),
                stem: epiphany_core::StemConfiguration,
                grace: None,
            };
            *slot = Event::Pitched(replacement);
        }
    }

    fn graph_delete_pitch(&mut self, pitch: PitchId) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        // Drop any user-chosen respell override for the deleted pitch (the dual
        // of `graph_respell_pitch`), so no spelling attachment is left targeting a
        // pitch that is no longer present (Chapter 5 SpellingScopeResolves). The
        // pitch is not added to `tombstoned_pitches` here: unlike a whole-event
        // delete the event survives, and a later ModifyEvent may legitimately
        // reintroduce the id — tombstoning it would make it both live and
        // tombstoned (invariant 11).
        score.spelling_attachments.retain(|a| {
            !(a.layer.is_none()
                && matches!(a.source, SpellingSource::UserChosen)
                && matches!(&a.scope, SpellingScope::Pitch(p) if *p == pitch)
                && matches!(a.directive, SpellingDirective::Explicit(_)))
        });
        let Some(event) = Self::graph_event_of_pitch(score, pitch) else {
            return;
        };
        let Some(slot) = score.events.get_mut(event) else {
            return;
        };
        if let Event::Pitched(pe) = slot {
            if pe.pitches.iter().filter(|ip| ip.id != pitch).count() == 0 {
                // Removing the last pitch would leave an empty (invalid) pitched
                // event; Chapter 5 forbids that ("use Rest for the no-pitch
                // case"), so the note degrades to a rest of the same placement
                // and duration. Keeps `get_mut` from materializing a malformed
                // chord that `check_invariants` would later reject.
                let rest = epiphany_core::Rest {
                    id: pe.id,
                    voice: pe.voice,
                    position: pe.position.clone(),
                    duration: pe.duration.clone(),
                    vertical_position: None,
                    visible: true,
                };
                *slot = Event::Rest(rest);
            } else {
                pe.pitches.retain(|ip| ip.id != pitch);
            }
        }
    }

    fn graph_modify_pitch(&mut self, pitch: PitchId, value: &Pitch) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        let Some(event) = Self::graph_event_of_pitch(score, pitch) else {
            return;
        };
        if let Some(Event::Pitched(pe)) = score.events.get_mut(event) {
            if let Some(ip) = pe.pitches.iter_mut().find(|ip| ip.id == pitch) {
                ip.pitch = value.clone();
            }
        }
    }

    fn graph_transpose_pitch(&mut self, pitch: PitchId, chromatic_steps: i32) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        let Some(event) = Self::graph_event_of_pitch(score, pitch) else {
            return;
        };
        if let Some(Event::Pitched(pe)) = score.events.get_mut(event) {
            if let Some(ip) = pe.pitches.iter_mut().find(|ip| ip.id == pitch) {
                // Minimal interval: shift the CMN alteration, saturating at the
                // `i8` bound (a lossy stand-in — an extreme transpose clamps
                // rather than renormalizing nominal/octave). Full interval
                // algebra (Chapter 4 tuning) is deferred — P12-K2.
                if let epiphany_core::PitchSpacePosition::Cmn { alteration, .. } =
                    &mut ip.pitch.scale_position.position
                {
                    let shifted = (*alteration as i32).saturating_add(chromatic_steps);
                    *alteration = shifted.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
                }
            }
        }
    }

    // --- Re-anchoring (Chapter 6 §6.5 rule table, representative subset). ----

    fn reanchor_for_tombstone(
        &mut self,
        env: &OperationEnvelope,
        tombstoned: TypedObjectId,
        repairs: &mut Vec<RepairRecord>,
        referent_voice: Option<VoiceId>,
    ) {
        // Find structures referencing the tombstoned object.
        let referencing: Vec<TypedObjectId> = self
            .structures
            .iter()
            .filter(|(_, endpoints)| endpoints.contains(&tombstoned))
            .map(|(sid, _)| *sid)
            .collect();
        for sid in referencing {
            match sid {
                TypedObjectId::Tie(_) => {
                    // A tie's existence requires both endpoints: cascade-delete.
                    self.cascade_structure(env, sid, repairs);
                }
                // The graph-only referent kinds — markers, cue events,
                // comments, analytical annotations, graphic gestures — are
                // repaired where their graph mutation happens
                // (`reanchor_event_referents`, run from
                // `materialize_graph_delete`), so the ledger record and the
                // graph always agree. They only exist under graph-aware
                // reduction (none is creatable by an operation).
                TypedObjectId::Marker(_)
                | TypedObjectId::Comment(_)
                | TypedObjectId::AnalyticalAnnotation(_)
                | TypedObjectId::GraphicGesture(_)
                | TypedObjectId::Event(_) => {}
                TypedObjectId::Beam(_) => {
                    let survivors = self.surviving_endpoints(sid, tombstoned);
                    if survivors < 2 {
                        self.cascade_structure(env, sid, repairs);
                    } else {
                        repairs.push(RepairRecord {
                            kind: RepairKind::SpannerTruncated {
                                removed_members: vec![tombstoned],
                            },
                            target: sid,
                        });
                    }
                }
                // Repeat structures follow the spanner row across every
                // anchor site (rule table "Repeat structure / Anchor").
                TypedObjectId::Slur(_)
                | TypedObjectId::Spanner(_)
                | TypedObjectId::RepeatStructure(_) => {
                    let survivors = self.surviving_endpoints(sid, tombstoned);
                    if survivors < 1 {
                        self.cascade_structure(env, sid, repairs);
                    } else if let Some(to) = self.nearest_survivor(sid, tombstoned) {
                        // The survivor is fixed (the structure's other
                        // endpoint — surviving-endpoint collapse per the rule
                        // table), so only the containment key applies: the
                        // reason names the survivor's actual proximity rank to
                        // the tombstoned endpoint rather than a hardcoded
                        // same-voice claim.
                        let survivor_voice = match to {
                            TypedObjectId::Event(event) => self.event_voice(event),
                            _ => None,
                        };
                        let reason = match (referent_voice, survivor_voice) {
                            (Some(referent), Some(survivor)) => self.rank_reason(
                                self.containment_rank(referent, survivor),
                                referent,
                                Some(survivor),
                            ),
                            // No indexed placement for either side (a
                            // non-metric endpoint): the pre-four-key default.
                            _ => ReanchorReason::SameVoiceNearer,
                        };
                        repairs.push(RepairRecord {
                            kind: RepairKind::Reanchored {
                                from: tombstoned,
                                to,
                                reason,
                            },
                            target: sid,
                        });
                    } else {
                        self.cascade_structure(env, sid, repairs);
                    }
                }
                _ => {
                    repairs.push(RepairRecord {
                        kind: RepairKind::AttachmentTombstoned,
                        target: sid,
                    });
                }
            }
        }
    }

    fn cascade_structure(
        &mut self,
        env: &OperationEnvelope,
        sid: TypedObjectId,
        repairs: &mut Vec<RepairRecord>,
    ) {
        let minter = self.minted_by.get(&sid).copied().unwrap_or(env.id);
        self.objects.insert(
            sid,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by: minter,
            },
        );
        repairs.push(RepairRecord {
            kind: RepairKind::CascadeDeleted,
            target: sid,
        });
    }

    fn surviving_endpoints(&self, sid: TypedObjectId, just_tombstoned: TypedObjectId) -> usize {
        self.structures
            .get(&sid)
            .map(|eps| {
                eps.iter()
                    .filter(|e| {
                        **e != just_tombstoned
                            && matches!(self.objects.get(*e), Some(ObjectState::Live))
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    fn nearest_survivor(
        &self,
        sid: TypedObjectId,
        just_tombstoned: TypedObjectId,
    ) -> Option<TypedObjectId> {
        // For slurs/spanners the rule table prescribes surviving-endpoint
        // collapse, so the candidate set is the structure's own endpoints; the
        // lexicographically-smallest survivor realizes the id tie-break (a
        // two-endpoint structure has exactly one). Proximity-aware re-targeting
        // beyond the endpoints stays a deferred refinement (the table says so
        // explicitly); the open-candidate four-key ordering lives in
        // `nearest_live_event`.
        self.structures.get(&sid).and_then(|eps| {
            eps.iter()
                .filter(|e| {
                    **e != just_tombstoned
                        && matches!(self.objects.get(*e), Some(ObjectState::Live))
                })
                .min()
                .copied()
        })
    }

    // --- The four-key "nearest" ordering (Chapter 6 §"Total Ordering for
    // Nearest") and the graph-only rule-table rows. ---------------------------

    /// The staff instance a voice lives in, from the base-free ledger index.
    fn voice_instance(&self, voice: VoiceId) -> Option<StaffInstanceId> {
        self.instance_voices
            .iter()
            .find_map(|(instance, voices)| voices.contains(&voice).then_some(*instance))
    }

    /// The region a staff instance lives in, from the base-free ledger index.
    fn instance_region_of(&self, instance: StaffInstanceId) -> Option<RegionId> {
        self.region_instances
            .iter()
            .find_map(|(region, instances)| instances.contains(&instance).then_some(*region))
    }

    /// The voice of a live event with an indexed metric placement.
    fn event_voice(&self, event: EventId) -> Option<VoiceId> {
        self.voice_occupancy.iter().find_map(|(voice, placements)| {
            placements
                .iter()
                .any(|(_, _, placed)| *placed == event)
                .then_some(*voice)
        })
    }

    /// Containment proximity (key 1 of the "nearest" ordering): same voice 0,
    /// same staff instance 1, same staff 2, same region 3, same canvas 4.
    /// Computed from the base-free ledger indices, so `reduce()` and
    /// `reduce_onto()` rank identically wherever both represent the scenario.
    /// Whether a rank-4 verdict from [`Self::containment_rank`] was
    /// *established* (both voices' placements resolve through instance and
    /// region, so "same canvas" is a proven fact of the singleton canvas)
    /// rather than the unresolvable-placement fallthrough. Recording reads
    /// this so an unestablished 4 stays the honest `ExplicitFallback`
    /// (Pass 12, P12-C4) — selection order is unaffected either way.
    fn canvas_rank_established(&self, referent_voice: VoiceId, candidate_voice: VoiceId) -> bool {
        let (Some(referent), Some(candidate)) = (
            self.voice_instance(referent_voice),
            self.voice_instance(candidate_voice),
        ) else {
            return false;
        };
        self.instance_region_of(referent).is_some() && self.instance_region_of(candidate).is_some()
    }

    /// The [`ReanchorReason`] to *record* for an achieved rank: ranks 0–3 map
    /// directly; rank 4 records `SameCanvasNearer` only when the proximity
    /// was established ([`Self::canvas_rank_established`]), else the honest
    /// `ExplicitFallback`. `survivor_voice` is `None` when the winning
    /// candidate's voice is itself unresolvable — never established.
    fn rank_reason(
        &self,
        rank: u8,
        referent_voice: VoiceId,
        survivor_voice: Option<VoiceId>,
    ) -> ReanchorReason {
        if rank == 4 {
            let established = survivor_voice
                .is_some_and(|survivor| self.canvas_rank_established(referent_voice, survivor));
            return if established {
                ReanchorReason::SameCanvasNearer
            } else {
                ReanchorReason::ExplicitFallback
            };
        }
        reason_for_rank(rank)
    }

    fn containment_rank(&self, referent_voice: VoiceId, candidate_voice: VoiceId) -> u8 {
        if referent_voice == candidate_voice {
            return 0;
        }
        let (Some(referent), Some(candidate)) = (
            self.voice_instance(referent_voice),
            self.voice_instance(candidate_voice),
        ) else {
            return 4;
        };
        if referent == candidate {
            return 1;
        }
        if let (Some(a), Some(b)) = (
            self.instance_staff.get(&referent),
            self.instance_staff.get(&candidate),
        ) {
            if a == b {
                return 2;
            }
        }
        if let (Some(a), Some(b)) = (
            self.instance_region_of(referent),
            self.instance_region_of(candidate),
        ) {
            if a == b {
                return 3;
            }
        }
        4
    }

    /// The nearest surviving live event to the tombstoned referent under the
    /// four-key total order (Chapter 6 §"Total Ordering for Nearest"): the
    /// strict lexicographic minimum of (containment proximity, absolute time
    /// distance from the referent's resolved position, forward before
    /// backward, typed id bytes ascending — an `EventId`'s numeric order *is*
    /// its canonical 16-byte order). Candidates ranked farther than `max_rank`
    /// are excluded. Read entirely from the canonical ledger indices, so the
    /// choice is a function of canonical state (permutation-invariant). Only
    /// *metric* placements are indexed; wall-clock distance (proportional
    /// regions) is a deferred refinement, so a wall-clock referent finds no
    /// candidate and falls to the kind's declared failure action.
    fn nearest_live_event(
        &self,
        referent: &ReferentContext,
        exclude: EventId,
        max_rank: u8,
    ) -> Option<(EventId, u8)> {
        let EventPosition::Musical(referent_position) = &referent.position else {
            return None;
        };
        let mut best: Option<(u8, RationalTime, u8, EventId)> = None;
        for (voice, placements) in &self.voice_occupancy {
            let rank = self.containment_rank(referent.voice, *voice);
            if rank > max_rank {
                continue;
            }
            for (position, _, event) in placements {
                if *event == exclude
                    || !matches!(
                        self.objects.get(&TypedObjectId::Event(*event)),
                        Some(ObjectState::Live)
                    )
                {
                    continue;
                }
                let signed = position.0.sub(&referent_position.0);
                let (direction, distance) = if signed.is_negative() {
                    (1u8, RationalTime::zero().sub(&signed))
                } else {
                    (0u8, signed)
                };
                let key = (rank, distance, direction, *event);
                if best.as_ref().map_or(true, |current| key < *current) {
                    best = Some(key);
                }
            }
        }
        best.map(|(rank, _, _, event)| (event, rank))
    }

    /// Drops `dead` from `sid`'s referent-index entry, removing the entry when
    /// no event reference remains.
    fn drop_structure_ref(&mut self, sid: TypedObjectId, dead: TypedObjectId) {
        if let Some(refs) = self.structures.get_mut(&sid) {
            refs.retain(|existing| *existing != dead);
            if refs.is_empty() {
                self.structures.remove(&sid);
            }
        }
    }

    /// The rule-table rows for the graph-only referent kinds — markers, cue
    /// events, comments, analytical annotations, graphic gestures (Chapter 6
    /// §"The Re-Anchoring Rule Table"). Runs from
    /// [`Self::materialize_graph_delete`], so both the DeleteEvent path and the
    /// undo path record the same repairs in the triggering operation's effect.
    /// Each row's ledger record and graph mutation are decided together, in
    /// canonical id order ("the graph follows the ledger").
    fn reanchor_event_referents(
        &mut self,
        env: &OperationEnvelope,
        deleted: EventId,
        referent: &ReferentContext,
    ) -> Vec<RepairRecord> {
        let mut repairs = Vec::new();
        let deleted_obj = TypedObjectId::Event(deleted);
        // The deleted event's own referent entry (a cue's source list) dies
        // with it.
        self.structures.remove(&deleted_obj);
        let referencing: Vec<TypedObjectId> = self
            .structures
            .iter()
            .filter(|(sid, refs)| {
                matches!(
                    sid,
                    TypedObjectId::Marker(_)
                        | TypedObjectId::Comment(_)
                        | TypedObjectId::AnalyticalAnnotation(_)
                        | TypedObjectId::GraphicGesture(_)
                        | TypedObjectId::Event(_)
                ) && refs.contains(&deleted_obj)
                    && matches!(self.objects.get(sid), Some(ObjectState::Live))
            })
            .map(|(sid, _)| *sid)
            .collect();
        for sid in referencing {
            match sid {
                TypedObjectId::Marker(_) => {
                    self.reanchor_marker(deleted, referent, sid, &mut repairs)
                }
                TypedObjectId::Event(cue) => self.cascade_cue(env, cue, &mut repairs),
                TypedObjectId::Comment(_) => {
                    self.orphan_comment(deleted, referent, sid, &mut repairs)
                }
                TypedObjectId::AnalyticalAnnotation(_) => {
                    self.reanchor_annotation(deleted, referent, sid, &mut repairs)
                }
                TypedObjectId::GraphicGesture(_) => {
                    self.reanchor_gesture(deleted, referent, sid, &mut repairs)
                }
                _ => {}
            }
        }
        repairs
    }

    /// Row "Marker / Anchor": re-anchor to the nearest event in the same staff
    /// instance (proximity max: same staff instance); orphan on failure.
    fn reanchor_marker(
        &mut self,
        deleted: EventId,
        referent: &ReferentContext,
        sid: TypedObjectId,
        repairs: &mut Vec<RepairRecord>,
    ) {
        let TypedObjectId::Marker(marker) = sid else {
            return;
        };
        match self.nearest_live_event(referent, deleted, PROXIMITY_SAME_STAFF_INSTANCE) {
            Some((to, rank)) => {
                if let Some(score) = self.graph.as_mut() {
                    if let Some(value) = score
                        .cross_cutting
                        .markers
                        .iter_mut()
                        .find(|value| value.id == marker)
                    {
                        if let TimeAnchor::Event { id, .. } = &mut value.anchor {
                            if *id == deleted {
                                // The anchor offset is preserved: the survivor
                                // shares the staff instance, hence the region
                                // and its offset discipline (invariant 9).
                                *id = to;
                            }
                        }
                    }
                }
                self.structures.insert(sid, vec![TypedObjectId::Event(to)]);
                repairs.push(RepairRecord {
                    kind: RepairKind::Reanchored {
                        from: TypedObjectId::Event(deleted),
                        to: TypedObjectId::Event(to),
                        reason: reason_for_rank(rank),
                    },
                    target: sid,
                });
            }
            None => {
                // Orphan: the marker (user content) is kept. Invariant 10
                // rejects a dangling event anchor, so the graph anchor degrades
                // to the containing region's start — anchor hygiene, not a
                // re-anchoring choice; the ledger records the orphaning.
                if let Some(region) = referent.region {
                    if let Some(score) = self.graph.as_mut() {
                        if let Some(value) = score
                            .cross_cutting
                            .markers
                            .iter_mut()
                            .find(|value| value.id == marker)
                        {
                            if matches!(value.anchor, TimeAnchor::Event { id, .. } if id == deleted)
                            {
                                value.anchor = TimeAnchor::Region {
                                    id: region,
                                    edge: RegionEdge::Start,
                                    offset: AnchorOffset::Zero,
                                };
                            }
                        }
                    }
                }
                self.structures.remove(&sid);
                repairs.push(RepairRecord {
                    kind: RepairKind::Orphaned,
                    target: sid,
                });
            }
        }
    }

    /// Row "Cue event / Source event": cascade-delete — the plain normative
    /// action, on *any* source deletion (the multi-source rationale tension is
    /// a proposed Pass-12 row). The cascaded cue is itself a tombstoned event,
    /// so the full re-anchoring pass — the graph-only rows via the recursive
    /// `materialize_graph_delete`, and the tie/beam/slur/spanner ledger arm via
    /// `reanchor_for_tombstone` — runs over its own referents transitively, in
    /// the same reduction step.
    fn cascade_cue(
        &mut self,
        env: &OperationEnvelope,
        cue: EventId,
        repairs: &mut Vec<RepairRecord>,
    ) {
        let sid = TypedObjectId::Event(cue);
        // A cue this pass already cascaded transitively (a cue-of-a-cue chain
        // reaching back into the referencing list) must not double-record.
        if !matches!(self.objects.get(&sid), Some(ObjectState::Live)) {
            return;
        }
        let cue_voice = self.event_voice(cue);
        self.cascade_structure(env, sid, repairs);
        self.structures.remove(&sid);
        for events in self.voice_occupancy.values_mut() {
            events.retain(|(_, _, event)| *event != cue);
        }
        self.voice_occupancy.retain(|_, events| !events.is_empty());
        self.event_pitches.remove(&cue);
        let cue_delete = DeleteEventOp {
            event: cue,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        };
        repairs.extend(self.materialize_graph_delete(env, &cue_delete));
        self.reanchor_for_tombstone(env, sid, repairs, cue_voice);
    }

    /// Row "Comment / Anchor": orphan — user content is never silently
    /// deleted. The comment stays live in ledger and graph; its dangling
    /// anchor references degrade to the containing-region forms so invariant
    /// 10 keeps holding.
    fn orphan_comment(
        &mut self,
        deleted: EventId,
        referent: &ReferentContext,
        sid: TypedObjectId,
        repairs: &mut Vec<RepairRecord>,
    ) {
        let TypedObjectId::Comment(comment) = sid else {
            return;
        };
        if let Some(region) = referent.region {
            if let Some(score) = self.graph.as_mut() {
                if let Some(value) = score
                    .cross_cutting
                    .comments
                    .iter_mut()
                    .find(|value| value.id == comment)
                {
                    orphan_annotation_anchor(&mut value.anchor, deleted, region);
                }
            }
        }
        self.drop_structure_ref(sid, TypedObjectId::Event(deleted));
        repairs.push(RepairRecord {
            kind: RepairKind::Orphaned,
            target: sid,
        });
    }

    /// Row "Analytical annotation / Anchor": re-anchor to a time range
    /// preserving the original extent; orphan when the range cannot be
    /// reconstructed. Reconstruction needs the containing region plus an exact
    /// musical placement — the range endpoints become region-start offsets, so
    /// they resolve to the deleted event's exact span. A wall-clock or
    /// indeterminate span is not expressible as a stored region-relative range
    /// (the expressibility gap is a proposed Pass-12 row), so it orphans.
    fn reanchor_annotation(
        &mut self,
        deleted: EventId,
        referent: &ReferentContext,
        sid: TypedObjectId,
        repairs: &mut Vec<RepairRecord>,
    ) {
        let TypedObjectId::AnalyticalAnnotation(annotation) = sid else {
            return;
        };
        let deleted_obj = TypedObjectId::Event(deleted);
        let current = self.graph.as_ref().and_then(|score| {
            score
                .cross_cutting
                .analytical
                .iter()
                .find(|value| value.id == annotation)
                .map(|value| value.anchor.clone())
        });
        let Some(current) = current else {
            self.drop_structure_ref(sid, deleted_obj);
            return;
        };
        // A stale index entry (the anchor no longer references the deleted
        // event) drops the reference with no repair.
        if !annotation_anchor_event_refs(&current).contains(&deleted_obj) {
            self.drop_structure_ref(sid, deleted_obj);
            return;
        }
        let musical_span = match (&referent.position, &referent.duration) {
            (EventPosition::Musical(position), EventDuration::Musical(duration)) => {
                Some((position.0.clone(), duration.0.clone()))
            }
            _ => None,
        };
        let range_point = |resolved: RationalTime, region: RegionId| TimeAnchor::Region {
            id: region,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration(resolved)),
        };
        let reconstructed: Option<AnnotationAnchor> = match &current {
            AnnotationAnchor::Event(event) if *event == deleted => {
                match (musical_span.as_ref(), referent.region) {
                    (Some((position, duration)), Some(region)) => Some(AnnotationAnchor::Range {
                        start: range_point(position.clone(), region),
                        end: range_point(position.add(duration), region),
                    }),
                    _ => None,
                }
            }
            AnnotationAnchor::Range { start, end } => {
                // Per endpoint: an event-anchored endpoint of the deleted event
                // is rebuilt at its resolved position (the event's position
                // plus any musical anchor offset); live endpoints are kept.
                let rebuild = |endpoint: &TimeAnchor| -> Option<TimeAnchor> {
                    match endpoint {
                        TimeAnchor::Event { id, offset } if *id == deleted => {
                            let (position, _) = musical_span.as_ref()?;
                            let region = referent.region?;
                            let resolved = match offset {
                                AnchorOffset::Zero => position.clone(),
                                AnchorOffset::Musical(delta) => position.add(&delta.0),
                                AnchorOffset::WallClock(_) => return None,
                            };
                            Some(range_point(resolved, region))
                        }
                        other => Some(other.clone()),
                    }
                };
                match (rebuild(start), rebuild(end)) {
                    (Some(start), Some(end)) => Some(AnnotationAnchor::Range { start, end }),
                    _ => None,
                }
            }
            // Stale index entry (the anchor no longer references the deleted
            // event): drop the reference, no repair.
            _ => {
                self.drop_structure_ref(sid, deleted_obj);
                return;
            }
        };
        match reconstructed {
            Some(anchor) => {
                let region = referent
                    .region
                    .expect("range reconstruction required the containing region");
                if let Some(score) = self.graph.as_mut() {
                    if let Some(value) = score
                        .cross_cutting
                        .analytical
                        .iter_mut()
                        .find(|value| value.id == annotation)
                    {
                        value.anchor = anchor;
                    }
                }
                self.drop_structure_ref(sid, deleted_obj);
                repairs.push(RepairRecord {
                    kind: RepairKind::Reanchored {
                        from: deleted_obj,
                        to: TypedObjectId::Region(region),
                        reason: ReanchorReason::ExplicitFallback,
                    },
                    target: sid,
                });
            }
            None => {
                if let Some(region) = referent.region {
                    if let Some(score) = self.graph.as_mut() {
                        if let Some(value) = score
                            .cross_cutting
                            .analytical
                            .iter_mut()
                            .find(|value| value.id == annotation)
                        {
                            orphan_annotation_anchor(&mut value.anchor, deleted, region);
                        }
                    }
                }
                self.drop_structure_ref(sid, deleted_obj);
                repairs.push(RepairRecord {
                    kind: RepairKind::Orphaned,
                    target: sid,
                });
            }
        }
    }

    /// Row "Graphic gesture / Anchor event": re-anchor each deleted event
    /// reference to the nearest surviving event of the same staff instance
    /// (proximity max: same staff instance); with no candidate the reference is
    /// dropped — truncation while references remain, orphaning when the list
    /// empties. Range anchoring truncates (dead endpoints move to the region
    /// edges); Free anchoring is never indexed.
    fn reanchor_gesture(
        &mut self,
        deleted: EventId,
        referent: &ReferentContext,
        sid: TypedObjectId,
        repairs: &mut Vec<RepairRecord>,
    ) {
        let TypedObjectId::GraphicGesture(gesture) = sid else {
            return;
        };
        let deleted_obj = TypedObjectId::Event(deleted);
        let current = self.graph.as_ref().and_then(|score| {
            score
                .cross_cutting
                .graphic_gestures
                .iter()
                .find(|value| value.id == gesture)
                .map(|value| value.anchoring.clone())
        });
        let Some(anchoring) = current else {
            self.drop_structure_ref(sid, deleted_obj);
            return;
        };
        let set_anchoring = |reducer: &mut Self, anchoring: GestureAnchoring| {
            if let Some(score) = reducer.graph.as_mut() {
                if let Some(value) = score
                    .cross_cutting
                    .graphic_gestures
                    .iter_mut()
                    .find(|value| value.id == gesture)
                {
                    value.anchoring = anchoring;
                }
            }
        };
        match anchoring {
            GestureAnchoring::Events(events) => {
                if !events.contains(&deleted) {
                    self.drop_structure_ref(sid, deleted_obj);
                    return;
                }
                match self.nearest_live_event(referent, deleted, PROXIMITY_SAME_STAFF_INSTANCE) {
                    Some((to, rank)) => {
                        let retargeted: Vec<EventId> = events
                            .iter()
                            .map(|event| if *event == deleted { to } else { *event })
                            .collect();
                        self.structures.insert(
                            sid,
                            retargeted
                                .iter()
                                .copied()
                                .map(TypedObjectId::Event)
                                .collect(),
                        );
                        set_anchoring(self, GestureAnchoring::Events(retargeted));
                        repairs.push(RepairRecord {
                            kind: RepairKind::Reanchored {
                                from: deleted_obj,
                                to: TypedObjectId::Event(to),
                                reason: reason_for_rank(rank),
                            },
                            target: sid,
                        });
                    }
                    None => {
                        let remaining: Vec<EventId> = events
                            .iter()
                            .copied()
                            .filter(|event| *event != deleted)
                            .collect();
                        let emptied = remaining.is_empty();
                        if emptied {
                            self.structures.remove(&sid);
                        } else {
                            self.structures.insert(
                                sid,
                                remaining
                                    .iter()
                                    .copied()
                                    .map(TypedObjectId::Event)
                                    .collect(),
                            );
                        }
                        set_anchoring(self, GestureAnchoring::Events(remaining));
                        repairs.push(RepairRecord {
                            kind: if emptied {
                                // The reference list emptied: the gesture (user
                                // content) is kept, reference-free.
                                RepairKind::Orphaned
                            } else {
                                RepairKind::SpannerTruncated {
                                    removed_members: vec![deleted_obj],
                                }
                            },
                            target: sid,
                        });
                    }
                }
            }
            GestureAnchoring::Range { start, end, staves } => {
                let Some(region) = referent.region else {
                    self.drop_structure_ref(sid, deleted_obj);
                    return;
                };
                let mut start = start;
                let mut end = end;
                retarget_dead_endpoint(&mut start, deleted, region, RegionEdge::Start);
                retarget_dead_endpoint(&mut end, deleted, region, RegionEdge::End);
                set_anchoring(self, GestureAnchoring::Range { start, end, staves });
                self.drop_structure_ref(sid, deleted_obj);
                repairs.push(RepairRecord {
                    kind: RepairKind::Reanchored {
                        from: deleted_obj,
                        to: TypedObjectId::Region(region),
                        reason: ReanchorReason::ExplicitFallback,
                    },
                    target: sid,
                });
            }
            GestureAnchoring::Free => {
                self.drop_structure_ref(sid, deleted_obj);
            }
        }
    }

    // --- Transactions (Chapter 6 §6.6). -------------------------------------

    fn reduce_transaction_block(&mut self, tx: TransactionId, members: &[&'a OperationEnvelope]) {
        let ordered = canonical_reduction_order(members);
        // Descriptor-precedence rule: the DeclareTransaction must be present and
        // causally precede every member.
        let desc = self.descriptors.get(&tx).copied();
        let well_formed = desc.is_some()
            && ordered
                .iter()
                .all(|m| m.causal_context.covers(desc.expect("checked is_some")));
        if !well_formed {
            let member_ids: Vec<OperationId> = ordered.iter().map(|m| m.id).collect();
            let conflict = ConflictRecord::new(
                ConflictKind::TransactionConflict {
                    transaction: tx,
                    failed_members: member_ids.clone(),
                },
                member_ids,
                vec![],
            );
            self.conflicts.insert(conflict);
            for m in &ordered {
                self.effects.push((
                    m.id,
                    OperationEffect::NoOp {
                        reason: NoOpReason::TransactionConflict,
                    },
                ));
            }
            return;
        }

        // Atomic: apply members against a snapshot; if any fails, roll back.
        let snapshot = self.snapshot();
        self.current_tx = Some(tx);
        let mut member_effects: Vec<(OperationId, OperationEffect)> = Vec::new();
        let mut failed_members: Vec<OperationId> = Vec::new();
        for m in &ordered {
            let eff = self.apply(m);
            if is_member_failure(&eff) {
                failed_members.push(m.id);
            }
            member_effects.push((m.id, eff));
        }
        self.current_tx = None;

        if failed_members.is_empty() {
            for (id, eff) in member_effects {
                self.effects.push((id, eff));
            }
        } else {
            self.restore(snapshot);
            let member_ids: Vec<OperationId> = ordered.iter().map(|m| m.id).collect();
            let conflict = ConflictRecord::new(
                ConflictKind::TransactionConflict {
                    transaction: tx,
                    failed_members,
                },
                member_ids,
                vec![],
            );
            self.conflicts.insert(conflict);
            for m in &ordered {
                self.effects.push((
                    m.id,
                    OperationEffect::NoOp {
                        reason: NoOpReason::TransactionConflict,
                    },
                ));
            }
        }
    }

    /// Records an object as minted by the operation's transaction (if any), so
    /// undo can compensate it later.
    fn note_minted(&mut self, _env: &OperationEnvelope, obj: TypedObjectId) {
        if let Some(tx) = self.current_tx {
            self.tx_minted.entry(tx).or_default().push(obj);
        }
    }

    fn concurrent(&self, a: OperationId, b: OperationId) -> bool {
        let a_ctx = self
            .env_of(a)
            .map(|e| e.causal_context.covers(b))
            .unwrap_or(false);
        let b_ctx = self
            .env_of(b)
            .map(|e| e.causal_context.covers(a))
            .unwrap_or(false);
        !a_ctx && !b_ctx
    }

    fn snapshot(&self) -> WorkingSnapshot {
        WorkingSnapshot {
            objects: self.objects.clone(),
            spellings: self.spellings.clone(),
            breaks: self.breaks.clone(),
            page_breaks: self.page_breaks.clone(),
            conflicts: self.conflicts.clone(),
            minted_by: self.minted_by.clone(),
            event_pitches: self.event_pitches.clone(),
            voice_occupancy: self.voice_occupancy.clone(),
            respell_chain: self.respell_chain.clone(),
            event_modify_chain: self.event_modify_chain.clone(),
            pitch_modify_chain: self.pitch_modify_chain.clone(),
            cross_cutting_modify_chain: self.cross_cutting_modify_chain.clone(),
            metric_grid_chain: self.metric_grid_chain.clone(),
            metadata_chain: self.metadata_chain.clone(),
            break_chain: self.break_chain.clone(),
            page_break_chain: self.page_break_chain.clone(),
            meter_change_chain: self.meter_change_chain.clone(),
            tempo_segment_chain: self.tempo_segment_chain.clone(),
            staff_layout_chain: self.staff_layout_chain.clone(),
            staff_values: self.staff_values.clone(),
            time_signature_values: self.time_signature_values.clone(),
            structures: self.structures.clone(),
            region_instances: self.region_instances.clone(),
            instance_voices: self.instance_voices.clone(),
            instance_staff: self.instance_staff.clone(),
            staff_based_regions: self.staff_based_regions.clone(),
            migrated_regions: self.migrated_regions.clone(),
            region_migrator: self.region_migrator.clone(),
            descriptors: self.descriptors.clone(),
            tx_minted: self.tx_minted.clone(),
            graph: self.graph.clone(),
        }
    }

    fn restore(&mut self, s: WorkingSnapshot) {
        self.objects = s.objects;
        self.spellings = s.spellings;
        self.breaks = s.breaks;
        self.page_breaks = s.page_breaks;
        self.conflicts = s.conflicts;
        self.minted_by = s.minted_by;
        self.event_pitches = s.event_pitches;
        self.voice_occupancy = s.voice_occupancy;
        self.respell_chain = s.respell_chain;
        self.event_modify_chain = s.event_modify_chain;
        self.pitch_modify_chain = s.pitch_modify_chain;
        self.cross_cutting_modify_chain = s.cross_cutting_modify_chain;
        self.metric_grid_chain = s.metric_grid_chain;
        self.metadata_chain = s.metadata_chain;
        self.break_chain = s.break_chain;
        self.page_break_chain = s.page_break_chain;
        self.meter_change_chain = s.meter_change_chain;
        self.tempo_segment_chain = s.tempo_segment_chain;
        self.staff_layout_chain = s.staff_layout_chain;
        self.staff_values = s.staff_values;
        self.time_signature_values = s.time_signature_values;
        self.structures = s.structures;
        self.region_instances = s.region_instances;
        self.instance_voices = s.instance_voices;
        self.instance_staff = s.instance_staff;
        self.staff_based_regions = s.staff_based_regions;
        self.migrated_regions = s.migrated_regions;
        self.region_migrator = s.region_migrator;
        self.descriptors = s.descriptors;
        self.tx_minted = s.tx_minted;
        self.graph = s.graph;
    }
}

/// Whether a transaction member's effect counts as an invariant-precondition
/// failure that conflicts the whole transaction (Chapter 6 §6.6).
fn is_member_failure(effect: &OperationEffect) -> bool {
    matches!(
        effect,
        OperationEffect::Conflicted { .. }
            | OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction { .. }
            }
            | OperationEffect::NoOp {
                reason: NoOpReason::TargetTombstoned
            }
    )
}

/// The transaction an envelope is a *member* of: its `transaction` field, unless
/// it is the `DeclareTransaction` descriptor itself (which is not a member of
/// the transaction it declares).
fn member_transaction(env: &OperationEnvelope) -> Option<TransactionId> {
    let tx = env.transaction?;
    if let OperationPayload::Primitive(OperationKind::DeclareTransaction(desc)) = &env.payload {
        if desc.id == tx {
            return None;
        }
    }
    Some(tx)
}

/// Groups active operations into transaction blocks by membership.
fn transaction_members<'a>(
    active: &[&'a OperationEnvelope],
) -> BTreeMap<TransactionId, Vec<&'a OperationEnvelope>> {
    let mut map: BTreeMap<TransactionId, Vec<&'a OperationEnvelope>> = BTreeMap::new();
    for env in active {
        if let Some(tx) = member_transaction(env) {
            map.entry(tx).or_default().push(env);
        }
    }
    map
}

/// Computes which reducible operations are held pending under the
/// missing-causal-predecessor rule (Chapter 6 §6.5, §6.6), and why.
///
/// An operation is directly blocked if a causal predecessor is absent,
/// equivocated, or excluded; the block then propagates transitively to anything
/// that causally depends on a blocked operation. Ties between multiple blocking
/// causes are broken by smallest blocker `OperationId`, so the reason is
/// deterministic.
fn compute_pending(
    reducible: &[&OperationEnvelope],
    reducible_ids: &BTreeSet<OperationId>,
    equivocated: &BTreeSet<OperationId>,
    excluded: &BTreeSet<OperationId>,
    declared_transactions: &BTreeSet<TransactionId>,
) -> BTreeMap<OperationId, PendingReason> {
    let mut blocked: BTreeMap<OperationId, PendingReason> = BTreeMap::new();
    let known_ids: BTreeSet<OperationId> = reducible_ids
        .iter()
        .chain(equivocated)
        .chain(excluded)
        .copied()
        .collect();
    // Direct causes: a hole in an asserted contiguous vector range, a dot
    // referencing a non-reducible id, or coverage of a known bad id.
    for env in reducible {
        // An absent transaction descriptor has a more specific normative
        // outcome: TransactionConflict. Let transaction reduction report it
        // instead of masking it as an ordinary missing predecessor.
        if member_transaction(env).is_some_and(|tx| !declared_transactions.contains(&tx)) {
            continue;
        }
        let mut causes: Vec<(OperationId, PendingReason)> = Vec::new();
        if let Some(missing) = first_missing_vector_predecessor(env, &known_ids) {
            causes.push((missing, PendingReason::MissingCausalPredecessor { missing }));
        }
        for d in env.causal_context.dots() {
            if !reducible_ids.contains(&d) {
                let reason = if equivocated.contains(&d) {
                    PendingReason::DependsOnEquivocated { on: d }
                } else if excluded.contains(&d) {
                    PendingReason::DependsOnExcluded { on: d }
                } else {
                    PendingReason::MissingCausalPredecessor { missing: d }
                };
                causes.push((d, reason));
            }
        }
        for e in equivocated {
            if env.causal_context.covers(*e) {
                causes.push((*e, PendingReason::DependsOnEquivocated { on: *e }));
            }
        }
        for x in excluded {
            if env.causal_context.covers(*x) {
                causes.push((*x, PendingReason::DependsOnExcluded { on: *x }));
            }
        }
        if let Some((_, reason)) = causes.into_iter().min_by_key(|(id, _)| *id) {
            blocked.insert(env.id, reason);
        }
    }

    // Transitive propagation: an op covering a blocked op is itself blocked.
    loop {
        let mut changed = false;
        for env in reducible {
            if blocked.contains_key(&env.id) {
                continue;
            }
            // Smallest blocked id this op covers.
            let cover = blocked
                .keys()
                .copied()
                .find(|b| env.causal_context.covers(*b));
            if let Some(on) = cover {
                blocked.insert(env.id, PendingReason::DependsOnPending { on });
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    blocked
}

/// Finds the smallest absent id asserted by a causal context's contiguous
/// vector ranges without expanding those ranges counter by counter.
fn first_missing_vector_predecessor(
    env: &OperationEnvelope,
    known_ids: &BTreeSet<OperationId>,
) -> Option<OperationId> {
    let mut first_missing = None;

    for (&replica, &high) in &env.causal_context.vector {
        let mut expected = 0_u64;
        let mut complete = false;

        for id in known_ids.range(OperationId::new(replica, 0)..=OperationId::new(replica, high)) {
            if id.counter > expected {
                break;
            }
            if id.counter == expected {
                if expected == high {
                    complete = true;
                    break;
                }
                expected += 1;
            }
        }

        if !complete {
            let candidate = OperationId::new(replica, expected);
            if first_missing.map_or(true, |current| candidate < current) {
                first_missing = Some(candidate);
            }
        }
    }

    first_missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::causal::CausalContext;
    use crate::stamp::{HybridLogicalClock, OperationStamp};
    use crate::support::AuthorId;
    use epiphany_core::{RationalTime, ReplicaId, StaffInstanceId, WallClockTime};

    fn pos(n: i64) -> MusicalPosition {
        MusicalPosition(RationalTime::from_int(n as i32))
    }

    fn insert(
        replica: u64,
        counter: u64,
        physical: i64,
        voice: u64,
        event: u64,
        pos_units: i64,
    ) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(replica), counter);
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
                // Voice and staff instance live in a shared (author-independent)
                // namespace so two authors can target the same voice.
                staff_instance: StaffInstanceId::new(ReplicaId(9), 0),
                event: crate::valuegen::insert_event_value(
                    EventId::new(ReplicaId(replica), event),
                    VoiceId::new(ReplicaId(9), voice),
                    pos(pos_units),
                    epiphany_core::MusicalDuration::whole(),
                    &[],
                ),
            })),
        }
    }

    // --- Subquadratic canonical order vs. the retained O(n²) oracle. --------

    /// A minimal envelope for pure ordering tests: payload content never
    /// affects the canonical reduction order.
    fn order_env(
        replica: ReplicaId,
        counter: u64,
        physical: i64,
        logical: u32,
        ctx: CausalContext,
    ) -> OperationEnvelope {
        let id = OperationId::new(replica, counter);
        OperationEnvelope {
            id,
            author: AuthorId(1),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(physical), logical),
                id,
            ),
            causal_context: ctx,
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
                event: EventId::new(ReplicaId(7), counter % 5),
                tuplet_compensation: TupletCompensation::NotInTuplet,
            })),
        }
    }

    /// Asserts the subquadratic order equals the reference oracle's
    /// element-for-element — by slice element identity (pointer equality), the
    /// strictest possible check: it distinguishes even byte-identical
    /// duplicate envelopes, whose tuple ties both implementations must break
    /// by slice position.
    fn assert_order_matches_reference(envelopes: &[OperationEnvelope]) {
        let refs: Vec<&OperationEnvelope> = envelopes.iter().collect();
        let fast = canonical_reduction_order(&refs);
        let oracle = canonical_reduction_order_reference(&refs);
        assert_eq!(fast.len(), oracle.len());
        for (at, (a, b)) in fast.iter().zip(&oracle).enumerate() {
            assert!(
                std::ptr::eq(*a, *b),
                "canonical order diverges from the reference at position {at}: \
                 {:?} vs {:?} (n = {})",
                a.id,
                b.id,
                envelopes.len()
            );
        }
    }

    /// In-place Fisher–Yates driven by the seeded generator (slice order is an
    /// input to the tuple tie-break, so permutations must be exercised too).
    fn shuffle_envelopes(
        envelopes: &mut [OperationEnvelope],
        rng: &mut epiphany_determinism::fuzz::SplitMix64,
    ) {
        for i in (1..envelopes.len()).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            envelopes.swap(i, j);
        }
    }

    /// A hostile ordering input the crate's well-formed generators avoid:
    /// floors that cover the envelope's own id or absent counters, dots to
    /// present / absent / own ids, duplicate ids (byte-identical twins),
    /// `SYSTEM_DERIVED` replicas, colliding stamps that contradict the causal
    /// edges (so the topological pass and the cycle-breaker both engage), and
    /// empty contexts.
    fn adversarial_set(
        rng: &mut epiphany_determinism::fuzz::SplitMix64,
        n: usize,
    ) -> Vec<OperationEnvelope> {
        let replicas = 1 + rng.next_u64() % 4;
        let mut next_counter: BTreeMap<ReplicaId, u64> = BTreeMap::new();
        let mut envs: Vec<OperationEnvelope> = Vec::with_capacity(n);
        for _ in 0..n {
            if !envs.is_empty() && rng.next_u64() % 8 == 0 {
                // A duplicate id — and a byte-identical stamp, so the
                // reduction tuple genuinely ties.
                let victim = envs[(rng.next_u64() % envs.len() as u64) as usize].clone();
                envs.push(victim);
                continue;
            }
            let replica = if rng.next_u64() % 16 == 0 {
                ReplicaId::SYSTEM_DERIVED
            } else {
                ReplicaId(1 + rng.next_u64() % replicas)
            };
            let slot = next_counter.entry(replica).or_insert(0);
            // Occasionally skip counters so floors assert absent predecessors.
            let counter = *slot + rng.next_u64() % 2;
            *slot = counter + 1;
            let id = OperationId::new(replica, counter);

            let mut ctx = CausalContext::new();
            for _ in 0..rng.next_u64() % 3 {
                let target = match rng.next_u64() % 5 {
                    0 => replica,      // may cover the envelope's own id
                    1 => ReplicaId(9), // absent replica
                    2 => ReplicaId::SYSTEM_DERIVED,
                    _ => ReplicaId(1 + rng.next_u64() % replicas),
                };
                // May exceed every present counter (covering future authoring
                // of that replica — a causal cycle) or fall below all of them.
                ctx = ctx.with_seen(target, rng.next_u64() % 8);
            }
            for _ in 0..rng.next_u64() % 3 {
                let dot = if !envs.is_empty() && rng.next_u64() % 2 == 0 {
                    envs[(rng.next_u64() % envs.len() as u64) as usize].id
                } else if rng.next_u64() % 4 == 0 {
                    id // the envelope's own id
                } else {
                    OperationId::new(ReplicaId(1 + rng.next_u64() % 5), rng.next_u64() % 10)
                };
                ctx = ctx.with_dot(dot);
            }

            // Tiny stamp ranges force heavy tuple collisions and stamps that
            // contradict the causal edges.
            envs.push(order_env(
                replica,
                counter,
                (rng.next_u64() % 6) as i64,
                (rng.next_u64() % 3) as u32,
                ctx,
            ));
        }
        envs
    }

    #[test]
    fn canonical_order_matches_reference_on_fuzz_sets() {
        // The crate's own well-formed generator: multi-replica meshes of
        // vector floors, occasional equivocation twins (duplicate ids) and
        // HLC-monotonicity anomalies, empty contexts on counter-0 roots.
        let mut rng = epiphany_determinism::fuzz::SplitMix64::new(0xF1_0DE2_0001);
        for _ in 0..250 {
            let n = 1 + (rng.next_u64() % 40) as usize;
            let mut envs = crate::fuzz::gen_envelope_set(&mut rng, n);
            assert_order_matches_reference(&envs);
            shuffle_envelopes(&mut envs, &mut rng);
            assert_order_matches_reference(&envs);
        }
    }

    #[test]
    fn canonical_order_matches_reference_on_adversarial_sets() {
        let mut rng = epiphany_determinism::fuzz::SplitMix64::new(0xADE5_A71A_0002);
        assert_order_matches_reference(&[]);
        for iteration in 0..400 {
            let n = 1 + (rng.next_u64() % 60) as usize;
            let mut envs = adversarial_set(&mut rng, n);
            assert_order_matches_reference(&envs);
            if iteration % 4 == 0 {
                shuffle_envelopes(&mut envs, &mut rng);
                assert_order_matches_reference(&envs);
            }
        }
    }

    #[test]
    fn canonical_order_matches_reference_on_directed_shapes() {
        let r = ReplicaId(1);

        // A 2,000-envelope single-replica chain whose every DVV floor covers
        // the full replica prefix — the inherently-quadratic-pairs shape the
        // subquadratic construction exists for — with *descending* stamps, so
        // the causal edges (not the HLC) decide every single emission.
        let full_chain: Vec<OperationEnvelope> = (0..2_000)
            .map(|c| {
                let ctx = if c == 0 {
                    CausalContext::new()
                } else {
                    CausalContext::new().with_seen(r, c - 1)
                };
                order_env(r, c, 2_000 - c as i64, 0, ctx)
            })
            .collect();
        assert_order_matches_reference(&full_chain);

        // A self-covering chain: every floor also covers the envelope's own
        // id (the exempted self-pair).
        let self_chain: Vec<OperationEnvelope> = (0..600)
            .map(|c| {
                order_env(
                    r,
                    c,
                    600 - c as i64,
                    0,
                    CausalContext::new().with_seen(r, c),
                )
            })
            .collect();
        assert_order_matches_reference(&self_chain);

        // A dot-only chain (no vector floors at all).
        let dot_chain: Vec<OperationEnvelope> = (0..600)
            .map(|c| {
                let ctx = if c == 0 {
                    CausalContext::new()
                } else {
                    CausalContext::new().with_dot(OperationId::new(r, c - 1))
                };
                order_env(r, c, 600 - c as i64, 0, ctx)
            })
            .collect();
        assert_order_matches_reference(&dot_chain);

        // Malformed dot cycles (2-cycle and 3-cycle) among bystanders with
        // empty contexts and identical stamps.
        let id = |rep: u64, c: u64| OperationId::new(ReplicaId(rep), c);
        let cycles = vec![
            order_env(
                ReplicaId(2),
                0,
                5,
                0,
                CausalContext::new().with_dot(id(2, 1)),
            ),
            order_env(
                ReplicaId(2),
                1,
                5,
                0,
                CausalContext::new().with_dot(id(2, 0)),
            ),
            order_env(
                ReplicaId(3),
                0,
                5,
                0,
                CausalContext::new().with_dot(id(3, 2)),
            ),
            order_env(
                ReplicaId(3),
                1,
                5,
                0,
                CausalContext::new().with_dot(id(3, 0)),
            ),
            order_env(
                ReplicaId(3),
                2,
                5,
                0,
                CausalContext::new().with_dot(id(3, 1)),
            ),
            order_env(ReplicaId(4), 0, 5, 0, CausalContext::new()),
            order_env(ReplicaId(5), 0, 5, 0, CausalContext::new()),
        ];
        assert_order_matches_reference(&cycles);

        // Mutual full-coverage floors (a floor cycle where each envelope also
        // covers itself), plus coverage of absent ids on an absent replica.
        let floor_cycle = vec![
            order_env(
                ReplicaId(6),
                0,
                9,
                0,
                CausalContext::new().with_seen(ReplicaId(6), 1),
            ),
            order_env(
                ReplicaId(6),
                1,
                8,
                0,
                CausalContext::new().with_seen(ReplicaId(6), 1),
            ),
            order_env(
                ReplicaId(6),
                2,
                7,
                0,
                CausalContext::new()
                    .with_seen(ReplicaId(40), 12)
                    .with_dot(id(41, 3)),
            ),
        ];
        assert_order_matches_reference(&floor_cycle);

        // Byte-identical duplicate ids (tuple ties broken by slice position)
        // in several slice orders, including a dot and a floor onto the
        // duplicated id.
        let twin = order_env(ReplicaId(2), 3, 1, 0, CausalContext::new());
        let mut twins = vec![
            twin.clone(),
            twin.clone(),
            order_env(
                ReplicaId(2),
                4,
                0,
                0,
                CausalContext::new().with_dot(id(2, 3)),
            ),
            order_env(
                ReplicaId(3),
                0,
                0,
                0,
                CausalContext::new().with_seen(ReplicaId(2), 3),
            ),
            // A twin that dots its own id: covers only its duplicate.
            order_env(
                ReplicaId(2),
                3,
                1,
                0,
                CausalContext::new().with_dot(id(2, 3)),
            ),
        ];
        assert_order_matches_reference(&twins);
        twins.reverse();
        assert_order_matches_reference(&twins);
        twins.swap(0, 2);
        assert_order_matches_reference(&twins);

        // SYSTEM_DERIVED authoring under floors and dots from user replicas.
        let sys = ReplicaId::SYSTEM_DERIVED;
        let system = vec![
            order_env(sys, 0, 3, 0, CausalContext::new()),
            order_env(sys, 1, 2, 0, CausalContext::new().with_seen(sys, 0)),
            order_env(
                ReplicaId(2),
                0,
                1,
                0,
                CausalContext::new().with_seen(sys, 1),
            ),
            order_env(
                ReplicaId(2),
                1,
                0,
                0,
                CausalContext::new().with_dot(id(u64::MAX, 0)),
            ),
        ];
        assert_order_matches_reference(&system);
    }

    #[test]
    fn reduction_is_permutation_invariant() {
        let envs = vec![
            insert(1, 0, 10, 1, 100, 0),
            insert(1, 1, 20, 1, 101, 1),
            insert(2, 0, 15, 2, 200, 0),
        ];
        let mut forward = OperationSet::new();
        forward.accept_all(envs.clone());
        let mut backward = OperationSet::new();
        let mut rev = envs.clone();
        rev.reverse();
        backward.accept_all(rev);
        assert_eq!(
            forward.reduce().canonical_bytes(),
            backward.reduce().canonical_bytes()
        );
    }

    #[test]
    fn concurrent_same_position_insert_promotes_the_greater_id() {
        // Two concurrent inserts, same voice and position, different events.
        let a = insert(1, 0, 10, 7, 100, 5);
        let b = insert(2, 0, 10, 7, 200, 5);
        let mut set = OperationSet::new();
        set.accept_all(vec![a.clone(), b.clone()]);
        let state = set.reduce();
        // The greater OperationId (replica 2) is promoted to a system voice.
        let promoted = state
            .effects
            .iter()
            .find(|(id, _)| *id == b.id)
            .map(|(_, e)| e);
        assert!(matches!(
            promoted,
            Some(OperationEffect::AppliedWithRepair { .. })
        ));
        let kept = state
            .effects
            .iter()
            .find(|(id, _)| *id == a.id)
            .map(|(_, e)| e);
        assert_eq!(kept, Some(&OperationEffect::Applied));
    }

    #[test]
    fn resolve_conflict_with_dismiss_reaches_dismissed_state() {
        // Pass 11, item 2.5: an authored ResolveConflict whose action is
        // `Dismiss` must reach the `Dismissed` resolution state (previously
        // `Dismissed` was representable but unreachable by any authored op).
        use crate::conflict::{ConflictResolutionState, ResolutionAction};
        use crate::payload::{ResolveConflictPayload, RespellPitchOp};
        use epiphany_core::PitchId;

        let pitch = PitchId::new(ReplicaId(9), 500);

        // An InsertEvent carrying `pitch` makes the pitch Live.
        let mut insert_env = insert(1, 0, 10, 1, 100, 0);
        if let OperationPayload::Primitive(OperationKind::InsertEvent(ref mut op)) =
            insert_env.payload
        {
            op.event = crate::valuegen::insert_event_value(
                op.event_id(),
                op.voice(),
                op.musical_position(),
                op.musical_duration(),
                &[pitch],
            );
        }

        // Two concurrent, differing respellings of `pitch`, both causally after
        // the insert (so the pitch is Live) but concurrent with each other (so
        // they collide into a StructuralFieldCollision conflict).
        let respell = |replica: u64, counter: u64, physical: i64, byte: u8| {
            let id = OperationId::new(ReplicaId(replica), counter);
            OperationEnvelope {
                id,
                author: AuthorId(0),
                stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
                causal_context: CausalContext::new().with_seen(ReplicaId(1), 0),
                transaction: None,
                payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                    pitch,
                    spelling: crate::valuegen::spelling(byte),
                })),
            }
        };
        let respell_a = respell(2, 0, 20, 0xAA);
        let respell_b = respell(3, 0, 21, 0xBB);

        // Phase 1: reduce to discover the content-derived conflict id.
        let mut set = OperationSet::new();
        set.accept_all(vec![
            insert_env.clone(),
            respell_a.clone(),
            respell_b.clone(),
        ]);
        let state = set.reduce();
        assert_eq!(
            state.conflicts.records().len(),
            1,
            "expected exactly one field-collision conflict"
        );
        let cid = state.conflicts.records()[0].id;
        assert_eq!(
            state.conflicts.records()[0].resolution_state,
            ConflictResolutionState::Unresolved
        );

        // Phase 2: author ResolveConflict { Dismiss } against that conflict,
        // causally after the colliding respells so it reduces against the
        // already-created conflict record.
        let resolve_id = OperationId::new(ReplicaId(4), 0);
        let resolve_env = OperationEnvelope {
            id: resolve_id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(30), 0), resolve_id),
            causal_context: CausalContext::new()
                .with_dot(respell_a.id)
                .with_dot(respell_b.id),
            transaction: None,
            payload: OperationPayload::ResolveConflict(ResolveConflictPayload {
                target: cid,
                action: ResolutionAction::Dismiss,
            }),
        };

        let mut set2 = OperationSet::new();
        set2.accept_all(vec![insert_env, respell_a, respell_b, resolve_env]);
        let state2 = set2.reduce();
        let rec = state2
            .conflicts
            .records()
            .iter()
            .find(|r| r.id == cid)
            .expect("conflict still present after resolution");
        assert_eq!(
            rec.resolution_state,
            ConflictResolutionState::Dismissed { by: resolve_id },
            "Dismiss action must select the Dismissed state"
        );
    }

    // --- Group 1 (M2) behavior. --------------------------------------------

    fn prim_env(
        replica: u64,
        counter: u64,
        physical: i64,
        ctx: CausalContext,
        kind: OperationKind,
    ) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(replica), counter);
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
            causal_context: ctx,
            transaction: None,
            payload: OperationPayload::Primitive(kind),
        }
    }

    fn modify_event_of(event: EventId, pitch: PitchId) -> OperationKind {
        OperationKind::ModifyEvent(crate::payload::ModifyEventOp {
            event: crate::valuegen::insert_event_value(
                event,
                VoiceId::new(ReplicaId(9), 1),
                pos(0),
                epiphany_core::MusicalDuration::whole(),
                &[pitch],
            ),
        })
    }

    #[test]
    fn concurrent_differing_modify_event_conflicts() {
        let event = EventId::new(ReplicaId(1), 100);
        let seen = CausalContext::new().with_seen(ReplicaId(1), 0);
        // Insert the event (replica 1), then two concurrent differing modifies.
        let insert = insert(1, 0, 10, 1, 100, 0);
        let mod_a = prim_env(
            2,
            0,
            20,
            seen.clone(),
            modify_event_of(event, PitchId::new(ReplicaId(1), 1)),
        );
        let mod_b = prim_env(
            3,
            0,
            20,
            seen,
            modify_event_of(event, PitchId::new(ReplicaId(1), 2)),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![insert, mod_a, mod_b]);
        let state = set.reduce();
        assert_eq!(
            state.conflicts.records().len(),
            1,
            "concurrent differing ModifyEvent must record exactly one conflict"
        );
        assert!(matches!(
            state.conflicts.records()[0].kind,
            ConflictKind::StructuralFieldCollision { .. }
        ));
    }

    // --- ModifyEvent placement changes (trim/move): the make-room enabler. ------

    fn musical(numerator: i64, denominator: i64) -> MusicalDuration {
        MusicalDuration(RationalTime::new(numerator, denominator).unwrap())
    }

    fn position_of(numerator: i64, denominator: i64) -> MusicalPosition {
        MusicalPosition(RationalTime::new(numerator, denominator).unwrap())
    }

    /// An InsertEvent of a rest at an explicit metric span.
    fn insert_at(
        replica: u64,
        counter: u64,
        event: u64,
        voice: u64,
        position: MusicalPosition,
        duration: MusicalDuration,
        ctx: CausalContext,
    ) -> OperationEnvelope {
        prim_env(
            replica,
            counter,
            (counter as i64 + 1) * 10,
            ctx,
            OperationKind::InsertEvent(InsertEventOp {
                staff_instance: StaffInstanceId::new(ReplicaId(9), 0),
                event: crate::valuegen::insert_event_value(
                    EventId::new(ReplicaId(replica), event),
                    VoiceId::new(ReplicaId(9), voice),
                    position,
                    duration,
                    &[],
                ),
            }),
        )
    }

    /// A ModifyEvent that re-places `event` (a rest) at a new metric span.
    fn modify_to(
        replica: u64,
        counter: u64,
        event: u64,
        voice: u64,
        position: MusicalPosition,
        duration: MusicalDuration,
        ctx: CausalContext,
    ) -> OperationEnvelope {
        prim_env(
            replica,
            counter,
            (counter as i64 + 1) * 10,
            ctx,
            OperationKind::ModifyEvent(crate::payload::ModifyEventOp {
                event: crate::valuegen::insert_event_value(
                    EventId::new(ReplicaId(replica), event),
                    VoiceId::new(ReplicaId(9), voice),
                    position,
                    duration,
                    &[],
                ),
            }),
        )
    }

    fn effect_at(state: &MaterializedState, counter: u64) -> Option<&OperationEffect> {
        state
            .effects
            .iter()
            .find(|(id, _)| *id == OperationId::new(ReplicaId(1), counter))
            .map(|(_, effect)| effect)
    }

    #[test]
    fn modify_event_trim_frees_the_voice_slot() {
        // e1 fills [0, 1). Trim it to [0, 1/2), then insert e2 into the freed [1/2, 1):
        // the insert only fits if the trim updated the voice-occupancy index.
        let e1 = insert(1, 0, 10, 1, 100, 0);
        let trim = modify_to(
            1,
            1,
            100,
            1,
            position_of(0, 1),
            musical(1, 2),
            CausalContext::new().with_seen(ReplicaId(1), 0),
        );
        let e2 = insert_at(
            1,
            2,
            101,
            1,
            position_of(1, 2),
            musical(1, 2),
            CausalContext::new().with_seen(ReplicaId(1), 1),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![e1, trim, e2]);
        let state = set.reduce();
        assert!(
            matches!(effect_at(&state, 1), Some(OperationEffect::Applied)),
            "the trim applies"
        );
        assert!(
            matches!(effect_at(&state, 2), Some(OperationEffect::Applied)),
            "the insert fits the span the trim freed (occupancy was updated)"
        );
    }

    #[test]
    fn modify_event_move_onto_a_sibling_is_refused() {
        // e1 [0, 1), e2 [1, 2). Moving e1 onto e2's span would break invariant 3, so
        // it is refused (a clean NoOp), not silently skipped.
        let e1 = insert(1, 0, 10, 1, 100, 0);
        let e2 = insert(1, 1, 20, 1, 101, 1);
        let onto_sibling = modify_to(
            1,
            2,
            100,
            1,
            position_of(1, 1),
            musical(1, 1),
            CausalContext::new().with_seen(ReplicaId(1), 1),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![e1, e2, onto_sibling]);
        let state = set.reduce();
        assert!(
            matches!(
                effect_at(&state, 2),
                Some(OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::EventDurationInvalid,
                    },
                })
            ),
            "a move onto a live sibling is refused"
        );
        assert!(
            state.is_clean(),
            "a refused move records no conflict/anomaly (like an insert overlap no-op)"
        );
    }

    #[test]
    fn modify_event_trim_materializes_in_the_graph() {
        use epiphany_core::check_invariants;
        use epiphany_core::generators::valid_score;

        // Shrink the first metric event's duration; the change must now reach the
        // graph (it was previously deferred) and leave the voice invariant-3 valid.
        let base = valid_score(0x5EED);
        let (event_id, duration) = base
            .voices()
            .flat_map(|(_, _, v)| v.events.clone())
            .find_map(|eid| {
                let ev = base.events.get(eid)?;
                match (ev.position(), ev.duration()) {
                    (EventPosition::Musical(_), EventDuration::Musical(d)) => {
                        Some((eid, d.clone()))
                    }
                    _ => None,
                }
            })
            .expect("the fixture has a metric event");
        let half = MusicalDuration(duration.0.mul(&RationalTime::new(1, 2).unwrap()));
        let mut shrunk = base.events.get(event_id).unwrap().clone();
        match &mut shrunk {
            Event::Pitched(pe) => pe.duration = EventDuration::Musical(half.clone()),
            Event::Rest(rest) => rest.duration = EventDuration::Musical(half.clone()),
            _ => panic!("a metric event is pitched or a rest"),
        }
        let modify = prim_env(
            2,
            0,
            10,
            CausalContext::new(),
            OperationKind::ModifyEvent(crate::payload::ModifyEventOp { event: shrunk }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![modify]);
        let result = set.reduce_onto(&base);
        assert_eq!(
            result.score.events.get(event_id).map(Event::duration),
            Some(&EventDuration::Musical(half)),
            "the trimmed duration is materialized in the graph"
        );
        assert!(
            check_invariants(&result.score).is_empty(),
            "the trimmed voice stays sorted and non-overlapping"
        );
    }

    #[test]
    fn modify_event_does_not_rewrite_a_non_metric_event_as_metric() {
        use epiphany_core::check_invariants;
        use epiphany_core::generators::valid_score_rich;

        // A ModifyEvent that rewrites a wall-clock event (a proportional region) into a
        // metric one must stay deferred — materializing it would re-place a non-metric
        // event onto a musical grid, breaking the region's time-model invariant.
        let base = valid_score_rich(0x5EED);
        let event_id = base
            .voices()
            .flat_map(|(_, _, v)| v.events.clone())
            .find(|eid| {
                matches!(
                    base.events.get(*eid).map(Event::position),
                    Some(EventPosition::WallClock(_))
                )
            })
            .expect("the fixture has a proportional region with a wall-clock event");
        let voice = base.events.get(event_id).unwrap().voice();
        let as_metric = crate::valuegen::insert_event_value(
            event_id,
            voice,
            position_of(0, 1),
            musical(1, 2),
            &[],
        );
        let modify = prim_env(
            2,
            0,
            10,
            CausalContext::new(),
            OperationKind::ModifyEvent(crate::payload::ModifyEventOp { event: as_metric }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![modify]);
        let result = set.reduce_onto(&base);
        assert!(
            matches!(
                result.score.events.get(event_id).map(Event::position),
                Some(EventPosition::WallClock(_))
            ),
            "the non-metric event is left as-is, not rewritten onto the musical grid"
        );
        assert!(
            check_invariants(&result.score).is_empty(),
            "the graph stays invariant-valid"
        );
    }

    #[test]
    fn modify_event_malformed_move_does_not_free_the_slot() {
        // A ModifyEvent rewriting e1 (filling [0, 1)) to an *empty* (malformed) pitched
        // event at [0, 1/2) is not materialized in the graph — so it must not move the
        // occupancy index either. A later insert into [1/2, 1) is therefore refused,
        // since e1 still occupies [0, 1).
        let e1 = insert(1, 0, 10, 1, 100, 0);
        let empty = Event::Pitched(epiphany_core::PitchedEvent {
            id: EventId::new(ReplicaId(1), 100),
            voice: VoiceId::new(ReplicaId(9), 1),
            position: EventPosition::Musical(position_of(0, 1)),
            duration: EventDuration::Musical(musical(1, 2)),
            pitches: vec![],
            articulations: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: epiphany_core::StemConfiguration,
            grace: None,
        });
        let malformed = prim_env(
            1,
            1,
            20,
            CausalContext::new().with_seen(ReplicaId(1), 0),
            OperationKind::ModifyEvent(crate::payload::ModifyEventOp { event: empty }),
        );
        let e2 = insert_at(
            1,
            2,
            101,
            1,
            position_of(1, 2),
            musical(1, 2),
            CausalContext::new().with_seen(ReplicaId(1), 1),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![e1, malformed, e2]);
        let state = set.reduce();
        assert!(
            matches!(
                effect_at(&state, 2),
                Some(OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction { .. },
                })
            ),
            "the malformed trim did not free the slot, so the later insert is refused"
        );
    }

    #[test]
    fn insert_then_delete_identified_pitch_tombstones() {
        let event = EventId::new(ReplicaId(1), 100);
        let pitch = PitchId::new(ReplicaId(1), 7);
        let insert = insert(1, 0, 10, 1, 100, 0);
        let add = prim_env(
            1,
            1,
            20,
            CausalContext::new().with_seen(ReplicaId(1), 0),
            OperationKind::InsertIdentifiedPitch(crate::payload::InsertIdentifiedPitchOp {
                event,
                pitch: crate::valuegen::identified_pitch(pitch),
            }),
        );
        let del = prim_env(
            1,
            2,
            30,
            CausalContext::new().with_seen(ReplicaId(1), 1),
            OperationKind::DeleteIdentifiedPitch(crate::payload::DeleteIdentifiedPitchOp { pitch }),
        );

        let mut after_add = OperationSet::new();
        after_add.accept_all(vec![insert.clone(), add.clone()]);
        assert_eq!(
            after_add.reduce().objects.get(&TypedObjectId::Pitch(pitch)),
            Some(&ObjectState::Live),
            "InsertIdentifiedPitch mints the pitch live"
        );

        let mut after_del = OperationSet::new();
        after_del.accept_all(vec![insert, add, del]);
        assert!(
            matches!(
                after_del.reduce().objects.get(&TypedObjectId::Pitch(pitch)),
                Some(ObjectState::Tombstoned { .. })
            ),
            "DeleteIdentifiedPitch tombstones the pitch"
        );
    }

    // --- Group 2 (M2) behavior. --------------------------------------------

    fn create_slur(slur: epiphany_core::SlurId, a: EventId, b: EventId) -> OperationKind {
        OperationKind::CreateCrossCutting(crate::payload::CreateCrossCuttingOp {
            structure: CrossCuttingValue::Slur(crate::valuegen::slur(slur, a, b)),
        })
    }

    #[test]
    fn delete_cross_cutting_tombstones_the_structure() {
        let e1 = EventId::new(ReplicaId(1), 100);
        let e2 = EventId::new(ReplicaId(1), 101);
        let slur = epiphany_core::SlurId::new(ReplicaId(1), 1);
        let sid = TypedObjectId::Slur(slur);
        let create = prim_env(
            1,
            2,
            12,
            CausalContext::new().with_seen(ReplicaId(1), 1),
            create_slur(slur, e1, e2),
        );
        let delete = prim_env(
            1,
            3,
            13,
            CausalContext::new().with_seen(ReplicaId(1), 2),
            OperationKind::DeleteCrossCutting(crate::payload::DeleteCrossCuttingOp {
                structure: sid,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![
            insert(1, 0, 10, 1, 100, 0),
            insert(1, 1, 11, 1, 101, 1),
            create,
            delete,
        ]);
        let state = set.reduce();
        assert!(
            matches!(
                state.objects.get(&sid),
                Some(ObjectState::Tombstoned { .. })
            ),
            "DeleteCrossCutting tombstones the structure (delete-wins)"
        );
    }

    #[test]
    fn concurrent_differing_modify_cross_cutting_conflicts() {
        let e1 = EventId::new(ReplicaId(1), 100);
        let e2 = EventId::new(ReplicaId(1), 101);
        let e3 = EventId::new(ReplicaId(1), 102);
        let slur = epiphany_core::SlurId::new(ReplicaId(1), 1);
        let create = prim_env(
            1,
            3,
            13,
            CausalContext::new().with_seen(ReplicaId(1), 2),
            create_slur(slur, e1, e2),
        );
        // Two concurrent modifies (seen the create, not each other) with differing
        // endpoints: e1->e2 vs e1->e3.
        let seen_create = CausalContext::new().with_seen(ReplicaId(1), 3);
        let modify = |replica: u64, end: EventId| {
            prim_env(
                replica,
                0,
                20,
                seen_create.clone(),
                OperationKind::ModifyCrossCutting(crate::payload::ModifyCrossCuttingOp {
                    structure: CrossCuttingValue::Slur(crate::valuegen::slur(slur, e1, end)),
                }),
            )
        };
        let mut set = OperationSet::new();
        set.accept_all(vec![
            insert(1, 0, 10, 1, 100, 0),
            insert(1, 1, 11, 1, 101, 1),
            insert(1, 2, 12, 1, 102, 2),
            create,
            modify(2, e2),
            modify(3, e3),
        ]);
        let state = set.reduce();
        assert_eq!(
            state.conflicts.records().len(),
            1,
            "concurrent differing ModifyCrossCutting must record exactly one conflict"
        );
        assert!(matches!(
            state.conflicts.records()[0].kind,
            ConflictKind::StructuralFieldCollision { .. }
        ));
    }

    // --- Phase D: repeat authoring (schema-major-2 revision). ----------------

    fn create_repeat(
        rid: epiphany_core::RepeatStructureId,
        a: EventId,
        b: EventId,
    ) -> OperationKind {
        OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
            repeat: crate::valuegen::repeat_structure(rid, a, b),
        })
    }

    #[test]
    fn create_repeat_structure_mints_and_delete_wins_tombstones() {
        let e1 = EventId::new(ReplicaId(1), 100);
        let e2 = EventId::new(ReplicaId(1), 101);
        let rid = epiphany_core::RepeatStructureId::new(ReplicaId(1), 1);
        let sid = TypedObjectId::RepeatStructure(rid);

        let mut set = OperationSet::new();
        set.accept_all(vec![
            insert(1, 0, 10, 1, 100, 0),
            insert(1, 1, 11, 1, 101, 1),
            prim_env(1, 2, 12, seen_r1(1), create_repeat(rid, e1, e2)),
        ]);
        assert!(
            matches!(set.reduce().objects.get(&sid), Some(ObjectState::Live)),
            "set-union mint with live anchors"
        );

        // Delete-wins: the tombstone survives a concurrent re-delete
        // (idempotent) and a post-delete re-create (TargetTombstoned no-op).
        let mut set = OperationSet::new();
        set.accept_all(vec![
            insert(1, 0, 10, 1, 100, 0),
            insert(1, 1, 11, 1, 101, 1),
            prim_env(1, 2, 12, seen_r1(1), create_repeat(rid, e1, e2)),
            prim_env(
                1,
                3,
                13,
                seen_r1(2),
                OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp { repeat: rid }),
            ),
            prim_env(
                2,
                0,
                14,
                seen_r1(3),
                OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp { repeat: rid }),
            ),
            prim_env(3, 0, 15, seen_r1(3), create_repeat(rid, e1, e2)),
        ]);
        assert!(
            matches!(
                set.reduce().objects.get(&sid),
                Some(ObjectState::Tombstoned { .. })
            ),
            "the tombstone survives re-delete and re-create (delete-wins)"
        );
    }

    #[test]
    fn create_repeat_structure_preconditions_every_anchor_site_live() {
        // The all-anchors-live precondition covers the kind's jump targets and
        // each volta's span, not just start/end (operation_catalog §"Repeat
        // Structures").
        let e1 = EventId::new(ReplicaId(1), 100);
        let e2 = EventId::new(ReplicaId(1), 101);
        let ghost = EventId::new(ReplicaId(1), 6_666);
        let rid = epiphany_core::RepeatStructureId::new(ReplicaId(1), 1);

        let mut dal_segno = crate::valuegen::repeat_structure(rid, e1, e2);
        dal_segno.kind = epiphany_core::RepeatKind::DalSegno {
            segno: crate::valuegen::event_anchor(ghost),
            end_target: crate::valuegen::event_anchor(e2),
        };
        let mut volta = crate::valuegen::volta_repeat(rid, e1, e2);
        volta.voltas[1].end = crate::valuegen::event_anchor(ghost);

        for repeat in [dal_segno, volta] {
            let mut set = OperationSet::new();
            set.accept_all(vec![
                insert(1, 0, 10, 1, 100, 0),
                insert(1, 1, 11, 1, 101, 1),
                prim_env(
                    1,
                    2,
                    12,
                    seen_r1(1),
                    OperationKind::CreateRepeatStructure(CreateRepeatStructureOp { repeat }),
                ),
            ]);
            assert!(
                !set.reduce()
                    .objects
                    .contains_key(&TypedObjectId::RepeatStructure(rid)),
                "a dead jump-target/volta anchor is a TargetMissing no-op — nothing minted"
            );
        }
    }

    #[test]
    fn deleting_an_event_reanchors_a_repeat_across_every_anchor_site() {
        // The rule table's "Repeat structure / Anchor" row: re-anchor to the
        // nearest surviving anchor, across start/end AND volta spans, recorded
        // as a RepairRecord — and the graph agrees with the ledger.
        use epiphany_core::generators::valid_score;
        let mut base = valid_score(0x5EED);
        let voice_events = base
            .voices()
            .map(|(_, _, v)| v.events.clone())
            .next()
            .expect("the fixture has a voice");
        let (e0, e1) = (voice_events[0], voice_events[1]);
        let rid = epiphany_core::RepeatStructureId::new(ReplicaId(9), 800);
        base.cross_cutting
            .repeats
            .push(epiphany_core::RepeatStructure {
                id: rid,
                start: crate::valuegen::event_anchor(e0),
                end: crate::valuegen::event_anchor(e1),
                kind: epiphany_core::RepeatKind::Volta,
                voltas: vec![epiphany_core::Volta {
                    endings: vec![1],
                    start: crate::valuegen::event_anchor(e0),
                    end: crate::valuegen::event_anchor(e1),
                }],
            });

        let del = prim_env(
            2,
            0,
            10,
            CausalContext::new(),
            OperationKind::DeleteEvent(DeleteEventOp {
                event: e0,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![del.clone()]);
        let result = set.reduce_onto(&base);

        let effect = result
            .state
            .effects
            .iter()
            .find(|(id, _)| *id == del.id)
            .map(|(_, e)| e)
            .expect("delete effect recorded");
        let OperationEffect::AppliedWithRepair { repairs } = effect else {
            panic!("expected AppliedWithRepair, got {effect:?}");
        };
        assert!(
            repairs.iter().any(|r| {
                r.target == TypedObjectId::RepeatStructure(rid)
                    && r.kind
                        == RepairKind::Reanchored {
                            from: TypedObjectId::Event(e0),
                            to: TypedObjectId::Event(e1),
                            reason: ReanchorReason::SameVoiceNearer,
                        }
            }),
            "the repeat re-anchor is a recorded repair: {repairs:?}"
        );
        let repeat = result
            .score
            .cross_cutting
            .repeats
            .iter()
            .find(|r| r.id == rid)
            .expect("the repeat survives with one live anchor");
        let expect_e1 =
            |anchor: &TimeAnchor| matches!(anchor, TimeAnchor::Event { id, .. } if *id == e1);
        assert!(
            expect_e1(&repeat.start)
                && expect_e1(&repeat.end)
                && expect_e1(&repeat.voltas[0].start)
                && expect_e1(&repeat.voltas[0].end),
            "EVERY dead anchor site — start and the volta span — moved to the survivor"
        );
        assert!(epiphany_core::check_invariants(&result.score).is_empty());
    }

    #[test]
    fn deleting_an_event_rewires_a_dal_segno_jump_target() {
        // The kind's jump targets are anchor SITES like any other: a dead
        // segno re-anchors to the surviving event anchor (graph + ledger),
        // and a repeat whose ONLY event anchor is its segno cascades when
        // that event dies. (No other test exercised event-anchored jump
        // targets through the delete path.)
        use epiphany_core::generators::valid_score;
        let mut base = valid_score(0x5EED);
        let voice_events = base
            .voices()
            .map(|(_, _, v)| v.events.clone())
            .next()
            .expect("the fixture has a voice");
        let (e0, e1) = (voice_events[0], voice_events[1]);
        let region = base.canvas.regions[0].id;
        let region_edge = |edge| TimeAnchor::Region {
            id: region,
            edge,
            offset: AnchorOffset::Zero,
        };

        // Repeat A: start/end/segno all event-anchored; e0 dies -> every dead
        // site (including the segno) moves to e1.
        let rid_a = epiphany_core::RepeatStructureId::new(ReplicaId(9), 810);
        base.cross_cutting
            .repeats
            .push(epiphany_core::RepeatStructure {
                id: rid_a,
                start: crate::valuegen::event_anchor(e0),
                end: crate::valuegen::event_anchor(e1),
                kind: epiphany_core::RepeatKind::DalSegno {
                    segno: crate::valuegen::event_anchor(e0),
                    end_target: crate::valuegen::event_anchor(e1),
                },
                voltas: Vec::new(),
            });
        // Repeat B: the ONLY event anchor is the segno; e0 dies -> cascade.
        let rid_b = epiphany_core::RepeatStructureId::new(ReplicaId(9), 811);
        base.cross_cutting
            .repeats
            .push(epiphany_core::RepeatStructure {
                id: rid_b,
                start: region_edge(epiphany_core::RegionEdge::Start),
                end: region_edge(epiphany_core::RegionEdge::End),
                kind: epiphany_core::RepeatKind::DalSegno {
                    segno: crate::valuegen::event_anchor(e0),
                    end_target: region_edge(epiphany_core::RegionEdge::End),
                },
                voltas: Vec::new(),
            });

        let del = prim_env(
            2,
            0,
            10,
            CausalContext::new(),
            OperationKind::DeleteEvent(DeleteEventOp {
                event: e0,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![del]);
        let result = set.reduce_onto(&base);

        let repeat_a = result
            .score
            .cross_cutting
            .repeats
            .iter()
            .find(|r| r.id == rid_a)
            .expect("repeat A survives on its e1 anchors");
        let segno_moved = matches!(
            &repeat_a.kind,
            epiphany_core::RepeatKind::DalSegno { segno: TimeAnchor::Event { id, .. }, .. }
                if *id == e1
        );
        assert!(
            segno_moved,
            "the dead segno jump target re-anchored to the survivor: {:?}",
            repeat_a.kind
        );
        assert!(
            matches!(repeat_a.start, TimeAnchor::Event { id, .. } if id == e1),
            "start moved with it"
        );

        assert!(
            !result
                .score
                .cross_cutting
                .repeats
                .iter()
                .any(|r| r.id == rid_b),
            "a repeat whose only event anchor was its segno cascades"
        );
        assert!(
            matches!(
                result
                    .state
                    .objects
                    .get(&TypedObjectId::RepeatStructure(rid_b)),
                Some(ObjectState::Tombstoned { .. })
            ),
            "the ledger agrees on the cascade"
        );
        assert!(epiphany_core::check_invariants(&result.score).is_empty());
    }

    #[test]
    fn deleting_the_only_anchor_event_cascades_the_repeat() {
        // No surviving event anchor: cascade-delete, in the ledger (tombstone
        // + CascadeDeleted repair) and the graph together.
        use epiphany_core::generators::valid_score;
        let mut base = valid_score(0x5EED);
        let voice_events = base
            .voices()
            .map(|(_, _, v)| v.events.clone())
            .next()
            .expect("the fixture has a voice");
        let e0 = voice_events[0];
        let rid = epiphany_core::RepeatStructureId::new(ReplicaId(9), 801);
        base.cross_cutting
            .repeats
            .push(epiphany_core::RepeatStructure {
                id: rid,
                start: crate::valuegen::event_anchor(e0),
                end: crate::valuegen::event_anchor(e0),
                kind: epiphany_core::RepeatKind::SimpleRepeat { count: 2 },
                voltas: Vec::new(),
            });

        let del = prim_env(
            2,
            0,
            10,
            CausalContext::new(),
            OperationKind::DeleteEvent(DeleteEventOp {
                event: e0,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![del.clone()]);
        let result = set.reduce_onto(&base);

        assert!(
            matches!(
                result
                    .state
                    .objects
                    .get(&TypedObjectId::RepeatStructure(rid)),
                Some(ObjectState::Tombstoned { .. })
            ),
            "no surviving anchor: the ledger cascade-tombstones the repeat"
        );
        let effect = result
            .state
            .effects
            .iter()
            .find(|(id, _)| *id == del.id)
            .map(|(_, e)| e)
            .expect("delete effect recorded");
        let OperationEffect::AppliedWithRepair { repairs } = effect else {
            panic!("expected AppliedWithRepair, got {effect:?}");
        };
        assert!(
            repairs.iter().any(|r| {
                r.target == TypedObjectId::RepeatStructure(rid)
                    && r.kind == RepairKind::CascadeDeleted
            }),
            "the cascade is a recorded repair: {repairs:?}"
        );
        assert!(
            !result
                .score
                .cross_cutting
                .repeats
                .iter()
                .any(|r| r.id == rid),
            "the graph agrees: the repeat is gone"
        );
        assert!(epiphany_core::check_invariants(&result.score).is_empty());
    }

    #[test]
    fn undoing_a_repeat_or_spanner_create_removes_it_from_the_graph() {
        // Undo tombstones the minted structure AND materializes the graph-side
        // removal (materialize_graph_tombstones). The spanner leg regression-
        // locks the pre-existing gap fixed alongside Phase D: an undone
        // spanner mint used to leave a ghost value in the graph.
        use epiphany_core::generators::valid_score;
        let base = valid_score(0x5EED);
        let voice_events = base
            .voices()
            .map(|(_, _, v)| v.events.clone())
            .next()
            .expect("the fixture has a voice");
        let (e0, e1) = (voice_events[0], voice_events[1]);
        let rid = epiphany_core::RepeatStructureId::new(ReplicaId(9), 802);
        let spanner_id = epiphany_core::SpannerId::new(ReplicaId(9), 803);
        let tx = TransactionId::new(ReplicaId(1), 900);

        let mut set = OperationSet::new();
        set.accept_all(vec![
            declare_transaction(1, 0, 10, CausalContext::new(), tx),
            tx_member(1, 1, 11, seen_r1(0), tx, create_repeat(rid, e0, e1)),
            tx_member(
                1,
                2,
                12,
                seen_r1(1),
                tx,
                OperationKind::CreateCrossCutting(crate::payload::CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Spanner(epiphany_core::Spanner {
                        id: spanner_id,
                        start: crate::valuegen::event_anchor(e0),
                        end: crate::valuegen::event_anchor(e1),
                        staves: Vec::new(),
                        kind: Default::default(),
                        style: Default::default(),
                    }),
                }),
            ),
            undo_env(1, 3, 13, seen_r1(2), tx, UndoPolicy::StrictInverse),
        ]);
        let result = set.reduce_onto(&base);

        assert!(
            matches!(
                result
                    .state
                    .objects
                    .get(&TypedObjectId::RepeatStructure(rid)),
                Some(ObjectState::Tombstoned { .. })
            ) && matches!(
                result
                    .state
                    .objects
                    .get(&TypedObjectId::Spanner(spanner_id)),
                Some(ObjectState::Tombstoned { .. })
            ),
            "undo tombstones both mints"
        );
        assert!(
            !result
                .score
                .cross_cutting
                .repeats
                .iter()
                .any(|r| r.id == rid),
            "the undone repeat mint leaves the graph"
        );
        assert!(
            !result
                .score
                .cross_cutting
                .spanners
                .iter()
                .any(|sp| sp.id == spanner_id),
            "the undone spanner mint leaves the graph (the ghost-value fix)"
        );
        assert!(epiphany_core::check_invariants(&result.score).is_empty());
    }

    // --- Push-1 spec-compliance fixes (Transpose skip, meta-conflict record,
    // marker re-anchor repair, system-derived counter collisions). -----------

    /// An InsertEvent envelope whose event carries exactly one identified
    /// pitch with the given id and intrinsic content.
    #[allow(clippy::too_many_arguments)]
    fn insert_with_pitch_content(
        replica: u64,
        counter: u64,
        physical: i64,
        voice: u64,
        event: u64,
        pos_units: i64,
        pitch_id: PitchId,
        content: &Pitch,
    ) -> OperationEnvelope {
        let mut env = insert(replica, counter, physical, voice, event, pos_units);
        if let OperationPayload::Primitive(OperationKind::InsertEvent(ref mut op)) = env.payload {
            op.event = crate::valuegen::insert_event_value(
                op.event_id(),
                op.voice(),
                pos(pos_units),
                epiphany_core::MusicalDuration::whole(),
                &[pitch_id],
            );
            if let Event::Pitched(pe) = &mut op.event {
                pe.pitches[0].pitch = content.clone();
            }
        }
        env
    }

    #[test]
    fn transpose_skips_tombstoned_targets_and_shifts_the_live_ones() {
        // Operation Catalog §Transpose (re-anchoring): "Tombstoned targets are
        // skipped (the transpose applies only to live pitches)."
        let p1 = PitchId::new(ReplicaId(9), 501);
        let p2 = PitchId::new(ReplicaId(9), 502);
        let neutral = crate::valuegen::pitch_value();
        let a = insert_with_pitch_content(1, 0, 10, 1, 100, 0, p1, &neutral);
        let b = insert_with_pitch_content(1, 1, 11, 2, 101, 0, p2, &neutral);
        let del = prim_env(
            1,
            2,
            20,
            CausalContext::new().with_seen(ReplicaId(1), 1),
            OperationKind::DeleteEvent(DeleteEventOp {
                event: EventId::new(ReplicaId(1), 100),
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
        );
        let after_delete = CausalContext::new().with_seen(ReplicaId(1), 2);
        // Mixed live/tombstoned targets: skips p1, shifts p2, applies.
        let t_mixed = prim_env(
            3,
            0,
            30,
            after_delete.clone(),
            OperationKind::Transpose(TransposeOp {
                targets: vec![p1, p2],
                chromatic_steps: 2,
            }),
        );
        // All targets tombstoned: degenerates to no effect.
        let t_dead = prim_env(
            4,
            0,
            31,
            after_delete.clone(),
            OperationKind::Transpose(TransposeOp {
                targets: vec![p1],
                chromatic_steps: 2,
            }),
        );
        // A target that never existed: dangling reference, whole op refuses.
        let t_missing = prim_env(
            5,
            0,
            32,
            after_delete,
            OperationKind::Transpose(TransposeOp {
                targets: vec![p2, PitchId::new(ReplicaId(9), 999)],
                chromatic_steps: 2,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![
            a,
            b,
            del,
            t_mixed.clone(),
            t_dead.clone(),
            t_missing.clone(),
        ]);
        let state = set.reduce();
        let effect = |id: OperationId| {
            state
                .effects
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, eff)| eff)
                .expect("effect recorded")
        };
        assert_eq!(effect(t_mixed.id), &OperationEffect::Applied);
        assert_eq!(
            effect(t_dead.id),
            &OperationEffect::NoOp {
                reason: NoOpReason::TargetTombstoned,
            }
        );
        assert_eq!(
            effect(t_missing.id),
            &OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            }
        );
    }

    #[test]
    fn schema_majors_follow_the_minimal_stamping_rule() {
        // Binary Format §Schema Major 2, "Minimal stamping": the stamp is the
        // lowest major whose layouts decode the payload's bytes — a pure
        // function of the value. Mandatory-append kinds are always 2; the
        // Option-hidden embeddings are value-dependent; everything else keeps
        // its prior major.
        use crate::payload::{
            CreateCrossCuttingOp, CreateRegionOp, CreateStaffInstanceOp, CrossCuttingValue,
            SetStaffLayoutOp,
        };
        use epiphany_core::{RegionId, SlurId, StaffInstanceId};

        let slur = crate::valuegen::slur(
            SlurId::new(ReplicaId(9), 1),
            EventId::new(ReplicaId(9), 1),
            EventId::new(ReplicaId(9), 2),
        );
        assert_eq!(
            OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                structure: CrossCuttingValue::Slur(slur),
            })
            .schema_major(),
            2,
            "mandatory v2 appends stamp 2"
        );

        let region = crate::valuegen::region(RegionId::new(ReplicaId(9), 3));
        assert_eq!(
            OperationKind::CreateRegion(CreateRegionOp {
                region: region.clone()
            })
            .schema_major(),
            1,
            "an empty-instance CreateRegion keeps its v1 stamp"
        );

        let iid = StaffInstanceId::new(ReplicaId(9), 4);
        let mut instance = crate::valuegen::staff_instance(iid, StaffId::new(ReplicaId(9), 5));
        assert_eq!(
            OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region: region.id,
                instance: instance.clone(),
            })
            .schema_major(),
            0,
            "a None-override instance encodes byte-identically at v0"
        );
        instance.staff_lines_override = Some(epiphany_core::StaffLineConfiguration::default());
        assert_eq!(
            OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region: region.id,
                instance,
            })
            .schema_major(),
            2,
            "a Some-override instance bears the v2 StaffLineConfiguration"
        );

        assert_eq!(
            OperationKind::SetStaffLayout(SetStaffLayoutOp {
                staff_instance: iid,
                instrument_override: None,
                staff_lines_override: None,
                visible: true,
            })
            .schema_major(),
            0
        );
        assert_eq!(
            OperationKind::SetStaffLayout(SetStaffLayoutOp {
                staff_instance: iid,
                instrument_override: None,
                staff_lines_override: Some(epiphany_core::StaffLineConfiguration::default()),
                visible: true,
            })
            .schema_major(),
            2,
            "a Some-override layout bears the v2 StaffLineConfiguration"
        );

        // The repeat pair (Phase D): the create is born at v2 — its carried
        // RepeatStructure's kind/voltas are unconditional fields, so even the
        // migration-default value has no lower-major layout. The delete's
        // bare-id payload is a major-0 layout under a minor kind append.
        let rid = epiphany_core::RepeatStructureId::new(ReplicaId(9), 6);
        assert_eq!(
            OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
                repeat: crate::valuegen::repeat_structure(
                    rid,
                    EventId::new(ReplicaId(9), 1),
                    EventId::new(ReplicaId(9), 2),
                ),
            })
            .schema_major(),
            2,
            "CreateRepeatStructure is born at v2 — even for default kind/voltas"
        );
        assert_eq!(
            OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp { repeat: rid })
                .schema_major(),
            0,
            "DeleteRepeatStructure carries a bare id — a major-0 layout"
        );
    }

    #[test]
    fn the_canonical_base_embeds_no_repeat_values() {
        // The surgical form of the cross-major byte-identity promise for the
        // repeat vocabulary: two reductions identical except for the created
        // repeat's v2 content (kind payload) must produce byte-identical
        // canonical bases — the base records the mint as a TypedObjectId
        // (discriminant 23) plus an Applied effect, never the filled value.
        let e1 = EventId::new(ReplicaId(1), 100);
        let e2 = EventId::new(ReplicaId(1), 101);
        let rid = epiphany_core::RepeatStructureId::new(ReplicaId(1), 1);
        let base_bytes = |count: u32| {
            let mut repeat = crate::valuegen::repeat_structure(rid, e1, e2);
            repeat.kind = epiphany_core::RepeatKind::SimpleRepeat { count };
            let mut set = OperationSet::new();
            set.accept_all(vec![
                insert(1, 0, 10, 1, 100, 0),
                insert(1, 1, 11, 1, 101, 1),
                prim_env(
                    1,
                    2,
                    12,
                    seen_r1(1),
                    OperationKind::CreateRepeatStructure(CreateRepeatStructureOp { repeat }),
                ),
            ]);
            let state = set.reduce();
            assert!(
                matches!(
                    state.objects.get(&TypedObjectId::RepeatStructure(rid)),
                    Some(ObjectState::Live)
                ),
                "the create must APPLY for this test to mean anything"
            );
            state.canonical_bytes()
        };
        assert_eq!(
            base_bytes(2),
            base_bytes(9),
            "differing repeat v2 content must not reach the canonical base"
        );
    }

    #[test]
    fn the_canonical_base_is_byte_identical_across_data_model_majors() {
        // Binary Format §Schema Major 1 / §Schema Major 2: the canonical-base
        // MaterializedState embeds none of the data-model-major values, so its
        // bytes MUST NOT move across those bumps. This golden-locks a seeded
        // reduction's canonical bytes; if it fails after a data-model change,
        // a filled type has leaked into the canonical base — which the majors
        // promise not to do. (A deliberate change to the base's own vocabulary
        // — an appended discriminant the seeded corpus emits — re-pins this
        // consciously. Re-pinned at Phase D: `gen_payload` gained the repeat
        // pair, discriminants 28/29, which shifted the seeded RNG stream and
        // with it the whole corpus. NOTE the seeded corpus's repeat creates
        // all no-op (their random anchors miss the live events), so THIS pin
        // cannot detect a repeat-value leak into the base — the dedicated
        // `the_canonical_base_embeds_no_repeat_values` test below covers
        // that with an APPLIED create.)
        let mut rng = epiphany_determinism::fuzz::SplitMix64::new(0xBA5E);
        let envelopes = crate::fuzz::gen_envelope_set(&mut rng, 200);
        let mut set = OperationSet::new();
        set.accept_all(envelopes);
        let bytes = set.reduce().canonical_bytes();
        let digest = epiphany_determinism::blake3_256(&bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "6e47a3113cfc54116af2e4a1b66fae16b9d1b63436fd26d9e6fc332bf501f5ed"
        );
    }

    #[test]
    fn transpose_skips_system_derived_targets_p12_k3() {
        // Review finding on P12-K3: Transpose must not rewrite a
        // SYSTEM_DERIVED pitch's intrinsic content in place — that would
        // desynchronize the content from the id's derivation inputs (and from
        // the `system_mints` registry the modify preconditions consult),
        // making the K3 verdict depend on where a snapshot was cut. System
        // targets are skipped like tombstoned ones; all-system degenerates to
        // the K3 precondition no-op.
        use epiphany_core::derive_system_pitch_id;
        let content = crate::valuegen::pitch_value_nth(1);
        let system_id = derive_system_pitch_id(&content);
        let normal = PitchId::new(ReplicaId(9), 501);

        let a = insert_with_pitch_content(1, 0, 10, 1, 100, 0, system_id, &content);
        let b = insert_with_pitch_content(1, 1, 11, 2, 101, 0, normal, &content);
        let after_inserts = CausalContext::new().with_seen(ReplicaId(1), 1);
        // Mixed normal/system targets: the system pitch is skipped, the
        // normal one shifts, the op applies.
        let t_mixed = prim_env(
            3,
            0,
            30,
            after_inserts.clone(),
            OperationKind::Transpose(TransposeOp {
                targets: vec![system_id, normal],
                chromatic_steps: 2,
            }),
        );
        // All targets system-derived: the K3 precondition no-op.
        let t_system = prim_env(
            4,
            0,
            31,
            after_inserts,
            OperationKind::Transpose(TransposeOp {
                targets: vec![system_id],
                chromatic_steps: 2,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![a, b, t_mixed.clone(), t_system.clone()]);
        let state = set.reduce();
        let effect = |id: OperationId| {
            state
                .effects
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, eff)| eff)
                .expect("effect recorded")
        };
        assert_eq!(effect(t_mixed.id), &OperationEffect::Applied);
        assert_eq!(
            effect(t_system.id),
            &OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::SystemDerivedContentImmutable,
                },
            },
            "an all-system-derived transpose refuses under P12-K3"
        );
    }

    #[test]
    fn differing_concurrent_resolves_name_both_resolvers_in_the_meta_conflict() {
        // Chapter 6 §Conflict Resolution Operations: a later differing resolve
        // reduces to Conflicted with a meta-conflict record; the record names
        // the earlier resolver (whose action stands) as winner and both
        // resolvers as causes.
        use crate::conflict::ResolutionAction;
        use crate::payload::{ResolveConflictPayload, RespellPitchOp};

        let pitch = PitchId::new(ReplicaId(9), 500);
        let mut insert_env = insert(1, 0, 10, 1, 100, 0);
        if let OperationPayload::Primitive(OperationKind::InsertEvent(ref mut op)) =
            insert_env.payload
        {
            op.event = crate::valuegen::insert_event_value(
                op.event_id(),
                op.voice(),
                op.musical_position(),
                op.musical_duration(),
                &[pitch],
            );
        }
        let respell = |replica: u64, physical: i64, byte: u8| {
            prim_env(
                replica,
                0,
                physical,
                CausalContext::new().with_seen(ReplicaId(1), 0),
                OperationKind::RespellPitch(RespellPitchOp {
                    pitch,
                    spelling: crate::valuegen::spelling(byte),
                }),
            )
        };
        let respell_a = respell(2, 20, 0xAA);
        let respell_b = respell(3, 21, 0xBB);
        let mut set = OperationSet::new();
        set.accept_all(vec![
            insert_env.clone(),
            respell_a.clone(),
            respell_b.clone(),
        ]);
        let cid = set.reduce().conflicts.records()[0].id;

        let resolve = |replica: u64, physical: i64, action: ResolutionAction| {
            let id = OperationId::new(ReplicaId(replica), 0);
            OperationEnvelope {
                id,
                author: AuthorId(0),
                stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
                causal_context: CausalContext::new()
                    .with_dot(respell_a.id)
                    .with_dot(respell_b.id),
                transaction: None,
                payload: OperationPayload::ResolveConflict(ResolveConflictPayload {
                    target: cid,
                    action,
                }),
            }
        };
        let first = resolve(4, 30, ResolutionAction::KeepWinner);
        let second = resolve(5, 31, ResolutionAction::AcceptLoser);
        let mut set2 = OperationSet::new();
        set2.accept_all(vec![
            insert_env,
            respell_a,
            respell_b,
            first.clone(),
            second.clone(),
        ]);
        let state = set2.reduce();
        let meta = state
            .conflicts
            .records()
            .iter()
            .find(|r| r.id != cid)
            .expect("a meta-conflict record exists");
        assert_eq!(
            meta.kind,
            ConflictKind::StructuralFieldCollision {
                winner: first.id,
                loser: second.id,
                field: FieldPath("conflict_resolution".to_string()),
            },
            "the earlier resolver's action stands, so it is the winner"
        );
        assert_eq!(
            meta.caused_by,
            vec![first.id, second.id],
            "both resolvers are named as causes"
        );
    }

    #[test]
    fn deleting_an_event_records_the_marker_reanchor_repair() {
        // Chapter 6 §Re-Anchoring: "Re-anchoring actions MUST be recorded as
        // RepairRecord entries in the triggering operation's effect", and the
        // rule table's marker row: re-anchor to the *nearest event in the same
        // staff instance* (four-key ordering). The fixture voice's events sit
        // at ascending quarter positions, so deleting the first re-anchors the
        // marker to the second (same voice: proximity rank 0 dominates any
        // closer event in a sibling voice).
        use epiphany_core::generators::valid_score;
        let mut base = valid_score(0x5EED);
        let voice_events = base
            .voices()
            .map(|(_, _, v)| v.events.clone())
            .next()
            .expect("the fixture has a voice");
        let event_id = voice_events[0];
        let expected = voice_events[1];
        let marker_id = epiphany_core::MarkerId::new(ReplicaId(9), 700);
        base.cross_cutting.markers.push(epiphany_core::Marker {
            id: marker_id,
            anchor: TimeAnchor::Event {
                id: event_id,
                offset: AnchorOffset::Zero,
            },
        });

        let del = prim_env(
            2,
            0,
            10,
            CausalContext::new(),
            OperationKind::DeleteEvent(DeleteEventOp {
                event: event_id,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![del.clone()]);
        let result = set.reduce_onto(&base);

        let effect = result
            .state
            .effects
            .iter()
            .find(|(id, _)| *id == del.id)
            .map(|(_, e)| e)
            .expect("delete effect recorded");
        let OperationEffect::AppliedWithRepair { repairs } = effect else {
            panic!("expected AppliedWithRepair, got {effect:?}");
        };
        assert!(
            repairs.iter().any(|r| {
                r.target == TypedObjectId::Marker(marker_id)
                    && r.kind
                        == RepairKind::Reanchored {
                            from: TypedObjectId::Event(event_id),
                            to: TypedObjectId::Event(expected),
                            reason: ReanchorReason::SameVoiceNearer,
                        }
            }),
            "the marker re-anchor to the nearest same-voice event is a recorded \
             repair, not a silent mutation: {repairs:?}"
        );
        let marker = result
            .score
            .cross_cutting
            .markers
            .iter()
            .find(|m| m.id == marker_id)
            .expect("marker survives");
        assert!(
            matches!(marker.anchor, TimeAnchor::Event { id, .. } if id == expected),
            "the graph agrees with the recorded repair"
        );
        assert!(epiphany_core::check_invariants(&result.score).is_empty());
    }

    #[test]
    fn marker_reanchor_prefers_the_forward_survivor_on_distance_ties() {
        // Four-key ordering, key 3: forward (0) before backward (1). Three
        // whole-note events at positions 10/12/14 in the base's first voice;
        // deleting the middle one leaves survivors at equal distance 2 on both
        // sides, so the *forward* neighbor (position 14) wins.
        use epiphany_core::generators::valid_score;
        let mut base = valid_score(0x5EED);
        let (staff_instance, voice) = {
            let instance = &base.canvas.regions[0].staff_instances()[0];
            (instance.id, instance.voices[0].id)
        };
        let backward = EventId::new(ReplicaId(3), 0);
        let referent = EventId::new(ReplicaId(3), 1);
        let forward = EventId::new(ReplicaId(3), 2);
        let insert_at = |counter: u64, event: EventId, position: i64| {
            let ctx = if counter == 0 {
                CausalContext::new()
            } else {
                CausalContext::new().with_seen(ReplicaId(3), counter - 1)
            };
            prim_env(
                3,
                counter,
                10 + counter as i64,
                ctx,
                OperationKind::InsertEvent(InsertEventOp {
                    staff_instance,
                    event: crate::valuegen::insert_event_value(
                        event,
                        voice,
                        pos(position),
                        epiphany_core::MusicalDuration::whole(),
                        &[],
                    ),
                }),
            )
        };
        let marker_id = epiphany_core::MarkerId::new(ReplicaId(9), 701);
        base.cross_cutting.markers.push(epiphany_core::Marker {
            id: marker_id,
            anchor: TimeAnchor::Event {
                id: referent,
                offset: AnchorOffset::Zero,
            },
        });
        let del = prim_env(
            3,
            3,
            20,
            CausalContext::new().with_seen(ReplicaId(3), 2),
            OperationKind::DeleteEvent(DeleteEventOp {
                event: referent,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![
            insert_at(0, backward, 10),
            insert_at(1, referent, 12),
            insert_at(2, forward, 14),
            del.clone(),
        ]);
        let result = set.reduce_onto(&base);
        let effect = result
            .state
            .effects
            .iter()
            .find(|(id, _)| *id == del.id)
            .map(|(_, e)| e)
            .expect("delete effect recorded");
        let OperationEffect::AppliedWithRepair { repairs } = effect else {
            panic!("expected AppliedWithRepair, got {effect:?}");
        };
        assert!(
            repairs.iter().any(|r| {
                r.target == TypedObjectId::Marker(marker_id)
                    && r.kind
                        == RepairKind::Reanchored {
                            from: TypedObjectId::Event(referent),
                            to: TypedObjectId::Event(forward),
                            reason: ReanchorReason::SameVoiceNearer,
                        }
            }),
            "equal distances tie-break forward before backward: {repairs:?}"
        );
        assert!(
            matches!(
                result
                    .score
                    .cross_cutting
                    .markers
                    .iter()
                    .find(|m| m.id == marker_id)
                    .expect("marker survives")
                    .anchor,
                TimeAnchor::Event { id, .. } if id == forward
            ),
            "the graph agrees with the recorded repair"
        );
        assert!(epiphany_core::check_invariants(&result.score).is_empty());
    }

    #[test]
    fn system_pitch_counter_collision_halts_reduction() {
        // Chapter 5 §"System-Derived Counter Collisions": two different
        // canonical input sets claiming one system-derived counter record a
        // SystemIdentifierCollision, reduction does not continue past the
        // collision, and neither input set occupies the collided counter.
        use epiphany_core::derive_system_pitch_id;
        let content_x = crate::valuegen::pitch_value_nth(1);
        let content_y = crate::valuegen::pitch_value_nth(2);
        let system_id = derive_system_pitch_id(&content_x);
        assert_eq!(system_id.replica(), ReplicaId::SYSTEM_DERIVED);

        let before = insert(3, 0, 5, 4, 400, 0);
        let legit = insert_with_pitch_content(1, 0, 10, 1, 100, 0, system_id, &content_x);
        let claim = insert_with_pitch_content(2, 0, 20, 2, 200, 5, system_id, &content_y);
        let after = insert(1, 1, 30, 3, 300, 9);

        let mut set = OperationSet::new();
        set.accept_all(vec![
            before.clone(),
            legit.clone(),
            claim.clone(),
            after.clone(),
        ]);
        let state = set.reduce();

        assert_eq!(state.anomalies.len(), 1, "exactly one collision recorded");
        match &state.anomalies[0].kind {
            IntegrityAnomalyKind::SystemIdentifierCollision {
                kind,
                colliding_counter,
                input_set_a,
                input_set_b,
            } => {
                assert_eq!(*kind, ObjectKind::Pitch);
                assert_eq!(*colliding_counter, system_id.counter());
                let mut sets = [input_set_a.0.clone(), input_set_b.0.clone()];
                sets.sort();
                let mut expected = [
                    canonical_pitch_bytes(&content_x),
                    canonical_pitch_bytes(&content_y),
                ];
                expected.sort();
                assert_eq!(sets, expected, "the anomaly retains both input sets");
            }
            other => panic!("expected SystemIdentifierCollision, got {other:?}"),
        }
        // Only the operation before the collision point reduced.
        assert_eq!(state.effects.len(), 1);
        assert_eq!(state.effects[0].0, before.id);
        // Neither input set occupies the collided counter.
        assert!(!state.objects.contains_key(&TypedObjectId::Pitch(system_id)));
        assert!(!state
            .objects
            .contains_key(&TypedObjectId::Event(EventId::new(ReplicaId(1), 100))));
        // The colliding pair and everything past the halt are held pending.
        let pending: BTreeMap<_, _> = state.pending.iter().copied().collect();
        let halted = PendingReason::HaltedBySystemCollision { at: claim.id };
        assert_eq!(pending.get(&legit.id), Some(&halted));
        assert_eq!(pending.get(&claim.id), Some(&halted));
        assert_eq!(pending.get(&after.id), Some(&halted));
        // Determinism: any permutation reduces to identical bytes.
        let mut reversed = OperationSet::new();
        reversed.accept_all(vec![after, claim, legit, before]);
        assert_eq!(state.canonical_bytes(), reversed.reduce().canonical_bytes());
    }

    #[test]
    fn reobserving_the_same_system_derivation_is_not_a_collision() {
        // The same (counter, inputs) pair re-observed is idempotent, not a
        // collision; the duplicate insert is refused by pitch-id freshness
        // (base-free parity with the graph-aware precondition).
        use epiphany_core::derive_system_pitch_id;
        let content_x = crate::valuegen::pitch_value_nth(1);
        let system_id = derive_system_pitch_id(&content_x);
        let a = insert_with_pitch_content(1, 0, 10, 1, 100, 0, system_id, &content_x);
        let b = insert_with_pitch_content(2, 0, 20, 2, 200, 5, system_id, &content_x);
        let mut set = OperationSet::new();
        set.accept_all(vec![a.clone(), b.clone()]);
        let state = set.reduce();
        assert!(state.anomalies.is_empty(), "same derivation: no collision");
        assert!(state.pending.is_empty(), "nothing is held");
        let effect = |id: OperationId| {
            state
                .effects
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, eff)| eff)
                .expect("effect recorded")
        };
        assert_eq!(effect(a.id), &OperationEffect::Applied);
        assert_eq!(
            effect(b.id),
            &OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetTombstoned,
                },
            },
            "a reused pitch id is not fresh, in base-free reduction too"
        );
    }

    #[test]
    fn base_seeded_system_pitch_collides_with_an_op_claim() {
        // The registry seeds from the base graph: an operation claiming a
        // base-occupied system counter with different content collides. The
        // base occupant is graph state (not an operation of this reduction),
        // so it is left in place for diagnostic recovery.
        use epiphany_core::generators::valid_score;
        use epiphany_core::{derive_system_pitch_id, IdentifiedPitch};

        let content_x = crate::valuegen::pitch_value_nth(1);
        let content_y = crate::valuegen::pitch_value_nth(2);
        let system_id = derive_system_pitch_id(&content_x);

        let mut base = valid_score(0x5EED);
        let pitched_id = base
            .voices()
            .flat_map(|(_, _, v)| v.events.clone())
            .find(|e| matches!(base.events.get(*e), Some(Event::Pitched(_))))
            .expect("the fixture has a pitched event");
        if let Some(Event::Pitched(pe)) = base.events.get_mut(pitched_id) {
            pe.pitches[0] = IdentifiedPitch {
                id: system_id,
                pitch: content_x.clone(),
            };
        }

        let claim = insert_with_pitch_content(2, 0, 10, 2, 200, 5, system_id, &content_y);
        let mut set = OperationSet::new();
        set.accept_all(vec![claim.clone()]);
        let result = set.reduce_onto(&base);

        assert_eq!(result.state.anomalies.len(), 1);
        assert!(matches!(
            &result.state.anomalies[0].kind,
            IntegrityAnomalyKind::SystemIdentifierCollision {
                kind: ObjectKind::Pitch,
                ..
            }
        ));
        assert!(result.state.effects.is_empty(), "reduction halted");
        let pending: BTreeMap<_, _> = result.state.pending.iter().copied().collect();
        assert_eq!(
            pending.get(&claim.id),
            Some(&PendingReason::HaltedBySystemCollision { at: claim.id })
        );
        assert!(
            result.score.events.get(pitched_id).is_some(),
            "the base occupant stays; recovery is external"
        );
    }

    #[test]
    fn a_system_derived_pitch_content_rewrite_is_refused_p12_k3() {
        // Pass 12 (P12-K3): a SYSTEM_DERIVED pitch's intrinsic content is
        // immutable — an in-place rewrite would invalidate the id's content
        // derivation. A modify carrying the *registered* derivation content
        // still applies (nothing is rewritten).
        use epiphany_core::derive_system_pitch_id;
        let content = crate::valuegen::pitch_value_nth(1);
        let rewritten = crate::valuegen::pitch_value_nth(2);
        let system_id = derive_system_pitch_id(&content);

        let mint = insert_with_pitch_content(1, 0, 10, 1, 100, 0, system_id, &content);
        let rewrite = prim_env(
            1,
            1,
            20,
            seen_r1(0),
            OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                pitch: system_id,
                value: rewritten.clone(),
            }),
        );
        let same = prim_env(
            1,
            2,
            30,
            seen_r1(1),
            OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                pitch: system_id,
                value: content.clone(),
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![mint.clone(), rewrite.clone(), same.clone()]);
        let state = set.reduce();

        let effect_of = |id: OperationId| {
            state
                .effects
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, eff)| eff)
        };
        assert_eq!(
            effect_of(rewrite.id),
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::SystemDerivedContentImmutable,
                },
            }),
            "rewriting a system-derived pitch's intrinsic content is refused"
        );
        assert_eq!(
            effect_of(same.id),
            Some(&OperationEffect::Applied),
            "a modify carrying the registered derivation content passes"
        );
        // Determinism: any permutation reduces to identical bytes.
        let mut reversed = OperationSet::new();
        reversed.accept_all(vec![same, rewrite, mint]);
        assert_eq!(state.canonical_bytes(), reversed.reduce().canonical_bytes());
    }

    #[test]
    fn a_modify_event_rewriting_a_system_pitch_is_refused_p12_k3() {
        // The same identity precondition through the ModifyEvent path: the
        // replacement event carries the system pitch with rewritten content.
        use epiphany_core::derive_system_pitch_id;
        let content = crate::valuegen::pitch_value_nth(1);
        let rewritten = crate::valuegen::pitch_value_nth(2);
        let system_id = derive_system_pitch_id(&content);

        let mint = insert_with_pitch_content(1, 0, 10, 1, 100, 0, system_id, &content);
        let mut replacement = crate::valuegen::insert_event_value(
            EventId::new(ReplicaId(1), 100),
            VoiceId::new(ReplicaId(9), 1),
            pos(0),
            epiphany_core::MusicalDuration::whole(),
            &[system_id],
        );
        if let Event::Pitched(pe) = &mut replacement {
            pe.pitches[0].pitch = rewritten;
        }
        let modify = prim_env(
            1,
            1,
            20,
            seen_r1(0),
            OperationKind::ModifyEvent(ModifyEventOp { event: replacement }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![mint, modify.clone()]);
        let state = set.reduce();

        assert_eq!(
            state
                .effects
                .iter()
                .find(|(e, _)| *e == modify.id)
                .map(|(_, eff)| eff),
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::SystemDerivedContentImmutable,
                },
            }),
            "a ModifyEvent rewriting a carried system pitch is refused"
        );
    }

    #[test]
    fn a_rank_four_reanchor_records_same_canvas_nearer_p12_c4() {
        // Pass 12 (P12-C4): the rank-4 (same-canvas) proximity survivor has
        // its own appended reason; ExplicitFallback stays the beyond-ladder
        // recording only.
        assert_eq!(reason_for_rank(4), ReanchorReason::SameCanvasNearer);
        assert_eq!(reason_for_rank(3), ReanchorReason::SameRegionNearer);
        assert_eq!(reason_for_rank(5), ReanchorReason::ExplicitFallback);
    }

    #[test]
    fn an_unestablished_rank_four_reanchor_keeps_the_explicit_fallback() {
        // Review finding on P12-C4: `containment_rank` also returns 4 when a
        // voice's staff-instance placement is *unresolvable* (here: base-free
        // reduction, where inserted voices have no op-created instance), and
        // that conflated 4 must NOT be recorded as a positive
        // `SameCanvasNearer` claim — the honest recording stays
        // `ExplicitFallback`. Selection order is unchanged either way.
        let e1 = EventId::new(ReplicaId(1), 100);
        let e2 = EventId::new(ReplicaId(1), 101);
        let slur = epiphany_core::SlurId::new(ReplicaId(1), 1);
        let create = prim_env(
            1,
            2,
            12,
            CausalContext::new().with_seen(ReplicaId(1), 1),
            create_slur(slur, e1, e2),
        );
        let del = prim_env(
            1,
            3,
            13,
            CausalContext::new().with_seen(ReplicaId(1), 2),
            OperationKind::DeleteEvent(DeleteEventOp {
                event: e1,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
        );
        // Different voices on different staff instances, neither instance
        // op-created in a region: instance→region never resolves, so
        // containment_rank falls through to its unestablished 4.
        let a = insert(1, 0, 10, 1, 100, 0);
        let mut b = insert(1, 1, 11, 2, 101, 1);
        if let OperationPayload::Primitive(OperationKind::InsertEvent(ref mut op)) = b.payload {
            op.staff_instance = StaffInstanceId::new(ReplicaId(9), 1);
        }
        let mut set = OperationSet::new();
        set.accept_all(vec![a, b, create, del.clone()]);
        let state = set.reduce();
        let repair = state
            .effects
            .iter()
            .find(|(id, _)| *id == del.id)
            .and_then(|(_, eff)| match eff {
                OperationEffect::AppliedWithRepair { repairs } => repairs
                    .iter()
                    .find(|r| r.target == TypedObjectId::Slur(slur))
                    .cloned(),
                _ => None,
            })
            .expect("the slur endpoint deletion records a re-anchor repair");
        assert_eq!(
            repair.kind,
            RepairKind::Reanchored {
                from: TypedObjectId::Event(e1),
                to: TypedObjectId::Event(e2),
                reason: ReanchorReason::ExplicitFallback,
            },
            "an unresolvable-placement rank 4 records the honest fallback"
        );
    }

    // --- ResolveEquivocation (operation_catalog §"ResolveEquivocation"). -----

    /// A `RespellPitch` envelope at `id` with an explicit causal context.
    fn respell_at(
        id: OperationId,
        physical: i64,
        spelling: u8,
        ctx: CausalContext,
    ) -> OperationEnvelope {
        use crate::payload::RespellPitchOp;
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
            causal_context: ctx,
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                pitch: epiphany_core::PitchId::new(ReplicaId(9), 500),
                spelling: crate::valuegen::spelling(spelling),
            })),
        }
    }

    /// A `ResolveEquivocation` envelope at `id` naming `(target, chosen)`.
    fn resolve_equivocation_env(
        id: OperationId,
        physical: i64,
        target: OperationId,
        chosen: crate::EnvelopeHash,
    ) -> OperationEnvelope {
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::ResolveEquivocation(
                crate::payload::ResolveEquivocationPayload { target, chosen },
            ),
        }
    }

    fn effect_of(state: &MaterializedState, id: OperationId) -> Option<&OperationEffect> {
        state
            .effects
            .iter()
            .find(|(e, _)| *e == id)
            .map(|(_, eff)| eff)
    }

    #[test]
    fn resolve_equivocation_promotes_the_chosen_candidate_and_unblocks_dependents() {
        let pitch = epiphany_core::PitchId::new(ReplicaId(9), 500);

        // An InsertEvent carrying `pitch` makes the pitch Live.
        let mut insert_env = insert(1, 0, 10, 1, 100, 0);
        if let OperationPayload::Primitive(OperationKind::InsertEvent(ref mut op)) =
            insert_env.payload
        {
            op.event = crate::valuegen::insert_event_value(
                op.event_id(),
                op.voice(),
                op.musical_position(),
                op.musical_duration(),
                &[pitch],
            );
        }

        // An equivocated pair of respellings under one id, both after the insert.
        let eq_id = OperationId::new(ReplicaId(9), 0);
        let after_insert = CausalContext::new().with_seen(ReplicaId(1), 0);
        let cand_a = respell_at(eq_id, 20, 0xAA, after_insert.clone());
        let cand_b = respell_at(eq_id, 20, 0xBB, after_insert.clone());
        assert_ne!(cand_a.envelope_hash(), cand_b.envelope_hash());

        // A dependent causally covering the equivocated id: previously held
        // pending (DependsOnEquivocated); must unblock and reduce.
        let dependent = respell_at(
            OperationId::new(ReplicaId(2), 0),
            30,
            0xCC,
            CausalContext::new()
                .with_seen(ReplicaId(1), 0)
                .with_seen(ReplicaId(9), 0),
        );

        // Baseline (no resolve): the slot is anomalous, the dependent pending.
        let mut without = OperationSet::new();
        without.accept_all(vec![
            insert_env.clone(),
            cand_a.clone(),
            cand_b.clone(),
            dependent.clone(),
        ]);
        let state = without.reduce();
        assert!(state.anomalies.iter().any(|an| matches!(
            an.kind,
            IntegrityAnomalyKind::OperationSlotEquivocated { operation_id } if operation_id == eq_id
        )));
        let pending: BTreeMap<_, _> = state.pending.iter().copied().collect();
        assert_eq!(
            pending.get(&dependent.id),
            Some(&PendingReason::DependsOnEquivocated { on: eq_id })
        );

        // With a resolve choosing candidate A: the slot reduces as if it had
        // always been Single with A — A contributes at its own canonical
        // position, the dependent unblocks, and no anomaly is recorded.
        let resolve = resolve_equivocation_env(
            OperationId::new(ReplicaId(3), 0),
            40,
            eq_id,
            cand_a.envelope_hash(),
        );
        let all = vec![
            insert_env.clone(),
            cand_a.clone(),
            cand_b.clone(),
            dependent.clone(),
            resolve.clone(),
        ];
        let mut set = OperationSet::new();
        set.accept_all(all.clone());
        let state = set.reduce();
        assert!(
            state.is_clean(),
            "no conflict, anomaly, or pending: {state:?}"
        );
        assert_eq!(effect_of(&state, eq_id), Some(&OperationEffect::Applied));
        assert_eq!(
            effect_of(&state, dependent.id),
            Some(&OperationEffect::Applied)
        );
        assert_eq!(
            effect_of(&state, resolve.id),
            Some(&OperationEffect::Applied)
        );
        // The dependent respell is causally after the promoted candidate, so
        // its spelling (0xCC) is the resolved value — an intentional overwrite.
        assert_eq!(
            state.spellings.get(&pitch),
            Some(&crate::valuegen::spelling(0xCC))
        );
        // Losing candidate B remains only in the diagnostic candidate store.
        assert!(set.candidate(cand_b.envelope_hash()).is_some());

        // Order-independent: a reversed acceptance order reduces to the bytes.
        let mut reversed = OperationSet::new();
        let mut rev = all;
        rev.reverse();
        reversed.accept_all(rev);
        assert_eq!(reversed.reduce().canonical_bytes(), state.canonical_bytes());
    }

    #[test]
    fn later_resolves_reduce_idempotently_or_collide_on_equivocation_resolution() {
        let eq_id = OperationId::new(ReplicaId(9), 0);
        let cand_a = respell_at(eq_id, 10, 0xAA, CausalContext::new());
        let cand_b = respell_at(eq_id, 10, 0xBB, CausalContext::new());

        // Three resolves: the earliest (in canonical order) governs; a later
        // one naming the same candidate is idempotent; a later one naming a
        // differing candidate collides.
        let first = resolve_equivocation_env(
            OperationId::new(ReplicaId(2), 0),
            20,
            eq_id,
            cand_a.envelope_hash(),
        );
        let same = resolve_equivocation_env(
            OperationId::new(ReplicaId(3), 0),
            30,
            eq_id,
            cand_a.envelope_hash(),
        );
        let differing = resolve_equivocation_env(
            OperationId::new(ReplicaId(4), 0),
            40,
            eq_id,
            cand_b.envelope_hash(),
        );

        let mut set = OperationSet::new();
        set.accept_all(vec![
            cand_a.clone(),
            cand_b.clone(),
            first.clone(),
            same.clone(),
            differing.clone(),
        ]);
        let state = set.reduce();

        assert_eq!(effect_of(&state, first.id), Some(&OperationEffect::Applied));
        assert_eq!(
            effect_of(&state, same.id),
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::AlreadyApplied,
            })
        );
        assert!(matches!(
            effect_of(&state, differing.id),
            Some(OperationEffect::Conflicted { .. })
        ));
        assert_eq!(state.conflicts.records().len(), 1);
        let record = &state.conflicts.records()[0];
        assert_eq!(
            record.kind,
            ConflictKind::StructuralFieldCollision {
                winner: first.id,
                loser: differing.id,
                field: FieldPath("equivocation_resolution".to_string()),
            }
        );
        assert_eq!(record.caused_by, vec![first.id, differing.id]);
        assert!(record.affected_objects.is_empty());
        // The slot still promoted A; no anomaly for it.
        assert!(state.anomalies.is_empty());
        assert!(effect_of(&state, eq_id).is_some());
    }

    #[test]
    fn resolve_without_a_matching_equivocation_is_a_precondition_noop() {
        let precondition_noop = OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::TargetMissing,
            },
        };

        // (a) Target id entirely absent from the operation set.
        let absent = resolve_equivocation_env(
            OperationId::new(ReplicaId(2), 0),
            20,
            OperationId::new(ReplicaId(9), 7),
            crate::EnvelopeHash([1; 32]),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![absent.clone()]);
        let state = set.reduce();
        assert_eq!(effect_of(&state, absent.id), Some(&precondition_noop));

        // (b) Target occupies an ordinary Single slot (no equivocation).
        let single = respell_at(
            OperationId::new(ReplicaId(9), 0),
            10,
            0xAA,
            CausalContext::new(),
        );
        let not_equivocated = resolve_equivocation_env(
            OperationId::new(ReplicaId(2), 0),
            20,
            single.id,
            single.envelope_hash(),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![single.clone(), not_equivocated.clone()]);
        let state = set.reduce();
        assert_eq!(
            effect_of(&state, not_equivocated.id),
            Some(&precondition_noop)
        );

        // (c) Target is equivocated but `chosen` names no candidate: the slot
        // stays equivocated (anomaly recorded, dependents still pending) and
        // the resolve is a precondition no-op — behavior is otherwise exactly
        // the unresolved baseline.
        let eq_id = OperationId::new(ReplicaId(9), 0);
        let cand_a = respell_at(eq_id, 10, 0xAA, CausalContext::new());
        let cand_b = respell_at(eq_id, 10, 0xBB, CausalContext::new());
        let dependent = respell_at(
            OperationId::new(ReplicaId(4), 0),
            30,
            0xCC,
            CausalContext::new().with_seen(ReplicaId(9), 0),
        );
        let bogus = resolve_equivocation_env(
            OperationId::new(ReplicaId(2), 0),
            20,
            eq_id,
            crate::EnvelopeHash([0xEE; 32]),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![
            cand_a.clone(),
            cand_b.clone(),
            dependent.clone(),
            bogus.clone(),
        ]);
        let state = set.reduce();
        assert_eq!(effect_of(&state, bogus.id), Some(&precondition_noop));
        assert!(effect_of(&state, eq_id).is_none());
        assert!(state.anomalies.iter().any(|an| matches!(
            an.kind,
            IntegrityAnomalyKind::OperationSlotEquivocated { operation_id } if operation_id == eq_id
        )));
        let pending: BTreeMap<_, _> = state.pending.iter().copied().collect();
        assert_eq!(
            pending.get(&dependent.id),
            Some(&PendingReason::DependsOnEquivocated { on: eq_id })
        );
    }

    #[test]
    fn an_equivocated_resolve_is_excluded_like_any_equivocated_slot() {
        let eq_id = OperationId::new(ReplicaId(9), 0);
        let cand_a = respell_at(eq_id, 10, 0xAA, CausalContext::new());
        let cand_b = respell_at(eq_id, 10, 0xBB, CausalContext::new());

        // Two distinct resolve envelopes under one id (differing `chosen`):
        // the resolve slot itself equivocates, so it never governs.
        let resolve_id = OperationId::new(ReplicaId(5), 0);
        let resolve_a = resolve_equivocation_env(resolve_id, 20, eq_id, cand_a.envelope_hash());
        let resolve_b = resolve_equivocation_env(resolve_id, 20, eq_id, cand_b.envelope_hash());
        assert_ne!(resolve_a.envelope_hash(), resolve_b.envelope_hash());

        let mut set = OperationSet::new();
        set.accept_all(vec![cand_a, cand_b, resolve_a, resolve_b]);
        let state = set.reduce();

        // Neither slot contributes; both record equivocation anomalies.
        assert!(state.effects.is_empty());
        for id in [eq_id, resolve_id] {
            assert!(
                state.anomalies.iter().any(|an| matches!(
                    an.kind,
                    IntegrityAnomalyKind::OperationSlotEquivocated { operation_id } if operation_id == id
                )),
                "expected an equivocation anomaly for {id:?}"
            );
        }
    }

    // =========================================================================
    // Phase-3 first tranche: CreateStaff, SetTimeSignature, SetTempoSegment,
    // SetStaffLayout, and value-restoring undo (operation_catalog
    // §CreateStaff, §"Meter and Tempo Overwrites", §SetStaffLayout, §undo).
    // =========================================================================

    fn tx_member(
        replica: u64,
        counter: u64,
        physical: i64,
        ctx: CausalContext,
        tx: TransactionId,
        kind: OperationKind,
    ) -> OperationEnvelope {
        let mut env = prim_env(replica, counter, physical, ctx, kind);
        env.transaction = Some(tx);
        env
    }

    fn declare_transaction(
        replica: u64,
        counter: u64,
        physical: i64,
        ctx: CausalContext,
        tx: TransactionId,
    ) -> OperationEnvelope {
        tx_member(
            replica,
            counter,
            physical,
            ctx,
            tx,
            OperationKind::DeclareTransaction(crate::payload::TransactionDescriptor {
                id: tx,
                label: String::from("phase-3 undo scenario"),
                category: None,
            }),
        )
    }

    fn undo_env(
        replica: u64,
        counter: u64,
        physical: i64,
        ctx: CausalContext,
        target: TransactionId,
        policy: UndoPolicy,
    ) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(replica), counter);
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
            causal_context: ctx,
            transaction: None,
            payload: OperationPayload::UndoTransaction(UndoTransactionPayload { target, policy }),
        }
    }

    fn seen_r1(counter: u64) -> CausalContext {
        CausalContext::new().with_seen(ReplicaId(1), counter)
    }

    fn respell_kind(pitch: PitchId, nth: u8) -> OperationKind {
        OperationKind::RespellPitch(RespellPitchOp {
            pitch,
            spelling: crate::valuegen::spelling(nth),
        })
    }

    #[test]
    fn create_staff_set_union_mint_discipline() {
        let staff_id = StaffId::new(ReplicaId(9), 7);
        let instrument = InstrumentId::new(ReplicaId(9), 1);
        let value = crate::valuegen::staff(staff_id, instrument);
        let create = prim_env(
            1,
            0,
            10,
            CausalContext::new(),
            OperationKind::CreateStaff(CreateStaffOp {
                staff: value.clone(),
            }),
        );
        let identical = prim_env(
            2,
            0,
            20,
            CausalContext::new(),
            OperationKind::CreateStaff(CreateStaffOp {
                staff: value.clone(),
            }),
        );
        let mut differing_value = value.clone();
        differing_value.name = String::from("something else");
        let differing = prim_env(
            3,
            0,
            30,
            CausalContext::new(),
            OperationKind::CreateStaff(CreateStaffOp {
                staff: differing_value,
            }),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![differing.clone(), identical.clone(), create.clone()]);
        let state = set.reduce();

        let effect_of = |id: OperationId| {
            state
                .effects
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, eff)| eff)
        };
        assert_eq!(effect_of(create.id), Some(&OperationEffect::Applied));
        assert_eq!(
            effect_of(identical.id),
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::AlreadyApplied,
            }),
            "a byte-identical re-create reduces idempotently"
        );
        assert_eq!(
            effect_of(differing.id),
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::RecreateContentMismatch,
                },
            }),
            "a differing value under a live id is a precondition no-op (P12-K9)"
        );
        assert!(matches!(
            state.objects.get(&TypedObjectId::Staff(staff_id)),
            Some(ObjectState::Live)
        ));
    }

    fn create_region_env(
        replica: u64,
        counter: u64,
        physical: i64,
        region: RegionId,
    ) -> OperationEnvelope {
        prim_env(
            replica,
            counter,
            physical,
            CausalContext::new(),
            OperationKind::CreateRegion(CreateRegionOp {
                region: crate::valuegen::region(region),
            }),
        )
    }

    fn set_meter_kind(
        region: RegionId,
        at: MusicalPosition,
        signature: Option<TimeSignature>,
    ) -> OperationKind {
        OperationKind::SetTimeSignature(SetTimeSignatureOp {
            region,
            anchor: crate::valuegen::region_start_anchor(region, at),
            time_signature: signature,
        })
    }

    #[test]
    fn concurrent_differing_meter_writes_collide_on_meter_sequence() {
        let region = RegionId::new(ReplicaId(9), 3);
        let create = create_region_env(1, 0, 10, region);
        let seen = seen_r1(0);
        let ts_a = crate::valuegen::time_signature(TimeSignatureId::new(ReplicaId(9), 1), 4);
        let ts_b = crate::valuegen::time_signature(TimeSignatureId::new(ReplicaId(9), 2), 3);
        let set_a = prim_env(
            2,
            0,
            20,
            seen.clone(),
            set_meter_kind(region, pos(0), Some(ts_a)),
        );
        let set_b = prim_env(3, 0, 20, seen, set_meter_kind(region, pos(0), Some(ts_b)));
        let mut set = OperationSet::new();
        set.accept_all(vec![set_b.clone(), set_a.clone(), create]);
        let state = set.reduce();

        assert_eq!(state.conflicts.records().len(), 1);
        assert!(matches!(
            &state.conflicts.records()[0].kind,
            ConflictKind::StructuralFieldCollision { field, winner, loser }
                if field.0 == "meter_sequence" && *winner == set_b.id && *loser == set_a.id
        ));
    }

    #[test]
    fn identical_concurrent_meter_writes_reduce_idempotently() {
        let region = RegionId::new(ReplicaId(9), 3);
        let create = create_region_env(1, 0, 10, region);
        let seen = seen_r1(0);
        let ts = crate::valuegen::time_signature(TimeSignatureId::new(ReplicaId(9), 1), 4);
        let set_a = prim_env(
            2,
            0,
            20,
            seen.clone(),
            set_meter_kind(region, pos(0), Some(ts.clone())),
        );
        let set_b = prim_env(3, 0, 20, seen, set_meter_kind(region, pos(0), Some(ts)));
        let mut set = OperationSet::new();
        set.accept_all(vec![set_b.clone(), set_a.clone(), create]);
        let state = set.reduce();

        assert!(state.conflicts.is_empty());
        let effect_of = |id: OperationId| {
            state
                .effects
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, eff)| eff)
        };
        assert_eq!(effect_of(set_a.id), Some(&OperationEffect::Applied));
        assert_eq!(
            effect_of(set_b.id),
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::AlreadyApplied,
            })
        );
    }

    #[test]
    fn a_differing_recarry_of_a_live_time_signature_is_refused() {
        let region = RegionId::new(ReplicaId(9), 3);
        let signature_id = TimeSignatureId::new(ReplicaId(9), 1);
        let create = create_region_env(1, 0, 10, region);
        let set_a = prim_env(
            1,
            1,
            20,
            seen_r1(0),
            set_meter_kind(
                region,
                pos(0),
                Some(crate::valuegen::time_signature(signature_id, 4)),
            ),
        );
        // Causally later, same signature id, different value, different key.
        let set_b = prim_env(
            1,
            2,
            30,
            seen_r1(1),
            set_meter_kind(
                region,
                pos(4),
                Some(crate::valuegen::time_signature(signature_id, 3)),
            ),
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![create, set_a, set_b.clone()]);
        let state = set.reduce();

        assert_eq!(
            state
                .effects
                .iter()
                .find(|(e, _)| *e == set_b.id)
                .map(|(_, eff)| eff),
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::RecreateContentMismatch,
                },
            }),
            "a differing value under a live signature id is a precondition no-op (P12-K9)"
        );
    }

    #[test]
    fn set_tempo_segment_refuses_writes_that_would_malform_the_map() {
        let region = RegionId::new(ReplicaId(9), 0);
        let anchor_at = |at: i64| crate::valuegen::region_start_anchor(region, pos(at));
        let malformed = |counter: u64, op: SetTempoSegmentOp| {
            prim_env(
                1,
                counter,
                (counter as i64 + 1) * 10,
                if counter == 0 {
                    CausalContext::new()
                } else {
                    seen_r1(counter - 1)
                },
                OperationKind::SetTempoSegment(op),
            )
        };
        // (0) A clean open constant segment at 0 applies (score scope).
        let clean = malformed(
            0,
            SetTempoSegmentOp {
                region: None,
                start: anchor_at(0),
                segment: Some(crate::valuegen::tempo_segment(region, pos(0), 120.0)),
            },
        );
        // (1) Carried segment start disagrees with the operation's key.
        let key_mismatch = malformed(
            1,
            SetTempoSegmentOp {
                region: None,
                start: anchor_at(2),
                segment: Some(crate::valuegen::tempo_segment(region, pos(3), 90.0)),
            },
        );
        // (2) A non-constant shape missing its end data.
        let mut ramp = crate::valuegen::tempo_segment(region, pos(4), 60.0);
        ramp.shape = epiphany_core::TempoShape::Linear;
        let missing_end = malformed(
            2,
            SetTempoSegmentOp {
                region: None,
                start: anchor_at(4),
                segment: Some(ramp),
            },
        );
        // (3) An explicit end overlapping the next segment: a segment at -4
        // whose end (at 2) runs past the existing segment's start at 0.
        let mut overlapping = crate::valuegen::tempo_segment(region, pos(-4), 100.0);
        overlapping.end = Some(anchor_at(2));
        let overlap = malformed(
            3,
            SetTempoSegmentOp {
                region: None,
                start: anchor_at(-4),
                segment: Some(overlapping),
            },
        );
        let mut set = OperationSet::new();
        set.accept_all(vec![
            clean.clone(),
            key_mismatch.clone(),
            missing_end.clone(),
            overlap.clone(),
        ]);
        let state = set.reduce();

        let effect_of = |id: OperationId| {
            state
                .effects
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, eff)| eff)
        };
        assert_eq!(effect_of(clean.id), Some(&OperationEffect::Applied));
        let refused = OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::TempoMapMalformed,
            },
        };
        assert_eq!(effect_of(key_mismatch.id), Some(&refused));
        assert_eq!(effect_of(missing_end.id), Some(&refused));
        assert_eq!(effect_of(overlap.id), Some(&refused));
    }

    /// The base scenario of the value-restoring undo unit tests: an event with
    /// one pitch (r1c0), a pre-transaction respell to `spelling(1)` (r1c1), a
    /// transaction T (declared r1c2) whose member respells to `spelling(2)`
    /// (r1c3).
    fn respell_undo_fixture(tx: TransactionId) -> (PitchId, Vec<OperationEnvelope>) {
        let pitch = PitchId::new(ReplicaId(1), 50);
        let e0 =
            insert_with_pitch_content(1, 0, 10, 1, 100, 0, pitch, &crate::valuegen::pitch_value());
        let pre = prim_env(1, 1, 20, seen_r1(0), respell_kind(pitch, 1));
        let declare = declare_transaction(1, 2, 30, seen_r1(1), tx);
        let member = tx_member(1, 3, 40, seen_r1(2), tx, respell_kind(pitch, 2));
        (pitch, vec![e0, pre, declare, member])
    }

    #[test]
    fn undo_restores_the_chain_predecessor_spelling() {
        let tx = TransactionId::from_raw(21);
        let (pitch, mut envelopes) = respell_undo_fixture(tx);
        envelopes.push(undo_env(
            1,
            4,
            50,
            seen_r1(3),
            tx,
            UndoPolicy::StrictInverse,
        ));
        let mut set = OperationSet::new();
        set.accept_all(envelopes);
        let state = set.reduce();

        assert!(state.conflicts.is_empty());
        assert_eq!(
            state.spellings.get(&pitch),
            Some(&crate::valuegen::spelling(1)),
            "undo must restore the pre-transaction spelling"
        );
        // A pure value restoration (no mints) is a clean `Applied`.
        assert_eq!(effect_at(&state, 4), Some(&OperationEffect::Applied));
    }

    #[test]
    fn undo_removes_a_first_spelling_write() {
        // No pre-transaction respell: the transaction introduced the first
        // spelling, so undo restores absence.
        let tx = TransactionId::from_raw(22);
        let pitch = PitchId::new(ReplicaId(1), 50);
        let e0 =
            insert_with_pitch_content(1, 0, 10, 1, 100, 0, pitch, &crate::valuegen::pitch_value());
        let declare = declare_transaction(1, 1, 20, seen_r1(0), tx);
        let member = tx_member(1, 2, 30, seen_r1(1), tx, respell_kind(pitch, 2));
        let undo = undo_env(1, 3, 40, seen_r1(2), tx, UndoPolicy::StrictInverse);
        let mut set = OperationSet::new();
        set.accept_all(vec![undo, member, declare, e0]);
        let state = set.reduce();

        assert!(state.conflicts.is_empty());
        assert_eq!(state.spellings.get(&pitch), None);
        assert_eq!(effect_at(&state, 3), Some(&OperationEffect::Applied));
    }

    #[test]
    fn superseded_undo_conflicts_strict_and_skips_best_effort() {
        let tx = TransactionId::from_raw(23);
        let (pitch, mut envelopes) = respell_undo_fixture(tx);
        // A causally-later respell supersedes the transaction's write.
        let superseder = prim_env(1, 4, 50, seen_r1(3), respell_kind(pitch, 3));
        let strict = undo_env(1, 5, 60, seen_r1(4), tx, UndoPolicy::StrictInverse);
        let best_effort = undo_env(1, 6, 70, seen_r1(5), tx, UndoPolicy::BestEffort);
        envelopes.extend([superseder.clone(), strict.clone(), best_effort.clone()]);
        let mut set = OperationSet::new();
        set.accept_all(envelopes);
        let state = set.reduce();

        // StrictInverse refuses the whole undo, naming the undo and the
        // superseding writer.
        assert!(matches!(
            effect_at(&state, 5),
            Some(OperationEffect::Conflicted { .. })
        ));
        let record = state
            .conflicts
            .records()
            .iter()
            .find(|record| record.caused_by.contains(&strict.id))
            .expect("the strict undo records a conflict");
        assert!(matches!(
            &record.kind,
            ConflictKind::TransactionConflict { transaction, .. } if *transaction == tx
        ));
        assert!(record.caused_by.contains(&superseder.id));
        // BestEffort skips the superseded key: applied, nothing restored.
        assert_eq!(effect_at(&state, 6), Some(&OperationEffect::Applied));
        assert_eq!(
            state.spellings.get(&pitch),
            Some(&crate::valuegen::spelling(3)),
            "the superseding write stands"
        );
    }

    #[test]
    fn undo_of_undo_restores_the_pre_undo_value() {
        // PINNED (see DECISIONS.md): an undo's restoration enters the write
        // chain as a new write by the undo operation. Undoing the undo's own
        // enclosing transaction therefore restores the value the first undo
        // removed.
        let tx = TransactionId::from_raw(24);
        let (pitch, mut envelopes) = respell_undo_fixture(tx);
        let undo_tx = TransactionId::from_raw(25);
        let declare_undo_tx = declare_transaction(1, 4, 50, seen_r1(3), undo_tx);
        let mut first_undo = undo_env(1, 5, 60, seen_r1(4), tx, UndoPolicy::StrictInverse);
        first_undo.transaction = Some(undo_tx);
        let second_undo = undo_env(1, 6, 70, seen_r1(5), undo_tx, UndoPolicy::StrictInverse);
        envelopes.extend([declare_undo_tx, first_undo, second_undo]);
        let mut set = OperationSet::new();
        set.accept_all(envelopes);
        let state = set.reduce();

        assert!(state.conflicts.is_empty());
        assert_eq!(
            state.spellings.get(&pitch),
            Some(&crate::valuegen::spelling(2)),
            "undoing the undo restores the originally-undone spelling"
        );
    }

    #[test]
    fn a_second_undo_of_the_same_transaction_sees_the_first_as_superseding() {
        // PINNED (see DECISIONS.md): the first undo's restoration is a write,
        // so a repeated strict undo of the same transaction conflicts rather
        // than double-restoring.
        let tx = TransactionId::from_raw(26);
        let (pitch, mut envelopes) = respell_undo_fixture(tx);
        let first = undo_env(1, 4, 50, seen_r1(3), tx, UndoPolicy::StrictInverse);
        let second = undo_env(1, 5, 60, seen_r1(4), tx, UndoPolicy::StrictInverse);
        envelopes.extend([first, second]);
        let mut set = OperationSet::new();
        set.accept_all(envelopes);
        let state = set.reduce();

        assert_eq!(
            state.spellings.get(&pitch),
            Some(&crate::valuegen::spelling(1)),
            "the first undo's restoration stands"
        );
        assert!(matches!(
            effect_at(&state, 5),
            Some(OperationEffect::Conflicted { .. })
        ));
    }

    #[test]
    fn undo_restoration_is_permutation_invariant() {
        // The write chains append in canonical processing order, so the undo's
        // restoration verdict and restored values are pure functions of the
        // operation set: any delivery order reduces to byte-identical state.
        let tx = TransactionId::from_raw(27);
        let (_, mut envelopes) = respell_undo_fixture(tx);
        // Add a meter overwrite + undo flavor alongside the respell flavor.
        let region = RegionId::new(ReplicaId(9), 3);
        envelopes.push(prim_env(
            2,
            0,
            15,
            CausalContext::new(),
            OperationKind::CreateRegion(CreateRegionOp {
                region: crate::valuegen::region(region),
            }),
        ));
        envelopes.push(undo_env(
            1,
            4,
            50,
            seen_r1(3),
            tx,
            UndoPolicy::StrictInverse,
        ));

        let baseline = {
            let mut set = OperationSet::new();
            set.accept_all(envelopes.clone());
            set.reduce().canonical_bytes()
        };
        let mut rng = epiphany_determinism::fuzz::SplitMix64::new(0x9E37_79B9_7F4A_7C15);
        for _ in 0..4 {
            let mut shuffled = envelopes.clone();
            shuffle_envelopes(&mut shuffled, &mut rng);
            let mut set = OperationSet::new();
            set.accept_all(shuffled);
            assert_eq!(
                set.reduce().canonical_bytes(),
                baseline,
                "value-restoring undo must be permutation-invariant"
            );
        }
    }
}
