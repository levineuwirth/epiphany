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

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    derive_promoted_voice_id, AnchorOffset, CanonicalValue, Event, EventDuration, EventId,
    EventPosition, MusicalDuration, MusicalPosition, OperationId, PitchId, PitchSpelling,
    RegionEdge, RegionId, RegionTimeModel, Score, TimeAnchor, TransactionId, TypedObjectId, Voice,
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
    CreateCrossCuttingOp, CrossCuttingValue, DeleteEventOp, InsertEventOp, OperationKind,
    OperationPayload, RespellPitchOp, TupletCompensation,
};
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
pub fn canonical_reduction_order<'a>(
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
}

impl PendingReason {
    fn discriminant(&self) -> u8 {
        match self {
            PendingReason::MissingCausalPredecessor { .. } => 0,
            PendingReason::DependsOnEquivocated { .. } => 1,
            PendingReason::DependsOnExcluded { .. } => 2,
            PendingReason::DependsOnPending { .. } => 3,
        }
    }
    fn blocker(&self) -> OperationId {
        match self {
            PendingReason::MissingCausalPredecessor { missing } => *missing,
            PendingReason::DependsOnEquivocated { on }
            | PendingReason::DependsOnExcluded { on }
            | PendingReason::DependsOnPending { on } => *on,
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

/// The working state of one reduction pass.
struct Reducer<'a> {
    op_set: &'a OperationSet,
    // Canonical results.
    objects: BTreeMap<TypedObjectId, ObjectState>,
    spellings: BTreeMap<PitchId, PitchSpelling>,
    breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    conflicts: ConflictRegistry,
    effects: Vec<(OperationId, OperationEffect)>,
    anomalies: BTreeMap<epiphany_core::IntegrityAnomalyId, IntegrityAnomaly>,
    // Transient indices.
    minted_by: BTreeMap<TypedObjectId, OperationId>,
    event_pitches: BTreeMap<EventId, Vec<PitchId>>,
    voice_occupancy: BTreeMap<VoiceId, Vec<(MusicalPosition, MusicalDuration, EventId)>>,
    last_respell: BTreeMap<PitchId, OperationId>,
    structures: BTreeMap<TypedObjectId, Vec<TypedObjectId>>,
    migrated_regions: BTreeSet<RegionId>,
    region_migrator: BTreeMap<RegionId, OperationId>,
    descriptors: BTreeMap<TransactionId, OperationId>,
    // Losing insert -> (promoted voice, winning insert).
    promotion: BTreeMap<OperationId, (VoiceId, OperationId)>,
    tx_minted: BTreeMap<TransactionId, Vec<TypedObjectId>>,
    current_tx: Option<TransactionId>,
    graph: Option<Score>,
}

/// A snapshot of the working state, for atomic transaction rollback.
struct WorkingSnapshot {
    objects: BTreeMap<TypedObjectId, ObjectState>,
    spellings: BTreeMap<PitchId, PitchSpelling>,
    breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    conflicts: ConflictRegistry,
    minted_by: BTreeMap<TypedObjectId, OperationId>,
    event_pitches: BTreeMap<EventId, Vec<PitchId>>,
    voice_occupancy: BTreeMap<VoiceId, Vec<(MusicalPosition, MusicalDuration, EventId)>>,
    last_respell: BTreeMap<PitchId, OperationId>,
    structures: BTreeMap<TypedObjectId, Vec<TypedObjectId>>,
    migrated_regions: BTreeSet<RegionId>,
    region_migrator: BTreeMap<RegionId, OperationId>,
    descriptors: BTreeMap<TransactionId, OperationId>,
    tx_minted: BTreeMap<TransactionId, Vec<TypedObjectId>>,
    graph: Option<Score>,
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

fn graph_voice_location(score: &Score, voice: VoiceId) -> Option<(usize, usize, usize)> {
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

/// The graph value inserted by a value-typed InsertEvent: the carried [`Event`]
/// itself, with its voice rebound to the (possibly system-promoted) target
/// voice. The Operation Catalog (v1) carries the real event, so this is no
/// longer a placeholder reconstruction.
fn graph_event_from_insert(op: &InsertEventOp, target_voice: VoiceId) -> Event {
    let mut event = op.event.clone();
    event.set_voice(target_voice);
    event
}

impl<'a> Reducer<'a> {
    fn new(op_set: &'a OperationSet) -> Self {
        Reducer {
            op_set,
            objects: BTreeMap::new(),
            spellings: BTreeMap::new(),
            breaks: BTreeMap::new(),
            conflicts: ConflictRegistry::new(),
            effects: Vec::new(),
            anomalies: BTreeMap::new(),
            minted_by: BTreeMap::new(),
            event_pitches: BTreeMap::new(),
            voice_occupancy: BTreeMap::new(),
            last_respell: BTreeMap::new(),
            structures: BTreeMap::new(),
            migrated_regions: BTreeSet::new(),
            region_migrator: BTreeMap::new(),
            descriptors: BTreeMap::new(),
            promotion: BTreeMap::new(),
            tx_minted: BTreeMap::new(),
            current_tx: None,
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
        }
        for layer in &score.analysis_layers {
            self.objects
                .insert(TypedObjectId::AnalysisLayer(layer.id), ObjectState::Live);
        }
        for view in &score.views {
            self.objects
                .insert(TypedObjectId::View(view.id), ObjectState::Live);
        }

        for region in &score.canvas.regions {
            self.objects
                .insert(TypedObjectId::Region(region.id), ObjectState::Live);
            for instance in region.staff_instances() {
                self.objects
                    .insert(TypedObjectId::StaffInstance(instance.id), ObjectState::Live);
                for measure in &instance.measures {
                    self.objects
                        .insert(TypedObjectId::Measure(measure.id), ObjectState::Live);
                }
                for voice in &instance.voices {
                    self.objects
                        .insert(TypedObjectId::Voice(voice.id), ObjectState::Live);
                }
            }
        }

        for event in score.events.iter_canonical() {
            let event_id = event.id();
            self.objects
                .insert(TypedObjectId::Event(event_id), ObjectState::Live);
            let mut pitch_ids = Vec::new();
            let mut pitches = Vec::new();
            event.collect_identified_pitches(&mut pitches);
            for pitch in pitches {
                pitch_ids.push(pitch.id);
                self.objects
                    .insert(TypedObjectId::Pitch(pitch.id), ObjectState::Live);
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
        }

        for slur in &score.cross_cutting.slurs {
            let id = TypedObjectId::Slur(slur.id);
            self.objects.insert(id, ObjectState::Live);
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
            self.objects
                .insert(TypedObjectId::Spanner(spanner.id), ObjectState::Live);
        }
        for marker in &score.cross_cutting.markers {
            self.objects
                .insert(TypedObjectId::Marker(marker.id), ObjectState::Live);
        }
        for annotation in &score.cross_cutting.analytical {
            self.objects.insert(
                TypedObjectId::AnalyticalAnnotation(annotation.id),
                ObjectState::Live,
            );
        }
        for comment in &score.cross_cutting.comments {
            self.objects
                .insert(TypedObjectId::Comment(comment.id), ObjectState::Live);
        }
        for gesture in &score.cross_cutting.graphic_gestures {
            self.objects
                .insert(TypedObjectId::GraphicGesture(gesture.id), ObjectState::Live);
        }
        for repeat in &score.cross_cutting.repeats {
            self.objects
                .insert(TypedObjectId::RepeatStructure(repeat.id), ObjectState::Live);
        }
        for lyric in &score.cross_cutting.lyrics {
            self.objects
                .insert(TypedObjectId::LyricLine(lyric.id), ObjectState::Live);
        }
        for chord in &score.cross_cutting.chord_symbols {
            self.objects
                .insert(TypedObjectId::ChordSymbol(chord.id), ObjectState::Live);
        }
    }

    fn run(mut self) -> (MaterializedState, Option<Score>) {
        let singles = self.op_set.single_envelopes();
        let equivocated: BTreeSet<OperationId> =
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
        for id in &equivocated {
            self.record_anomaly(IntegrityAnomalyKind::OperationSlotEquivocated {
                operation_id: *id,
            });
        }

        // 2. Reducible candidates = Single slots minus excluded.
        let reducible: Vec<&OperationEnvelope> = singles
            .iter()
            .copied()
            .filter(|e| !excluded.contains(&e.id))
            .collect();
        let reducible_ids: BTreeSet<OperationId> = reducible.iter().map(|e| e.id).collect();
        let declared_transactions: BTreeSet<TransactionId> = singles
            .iter()
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
        let mut processed: BTreeSet<OperationId> = BTreeSet::new();
        for env in &order {
            if processed.contains(&env.id) {
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

        let mut pending_vec: Vec<(OperationId, PendingReason)> = pending.into_iter().collect();
        pending_vec.sort_by_key(|(id, _)| *id);

        let graph = self.graph.take();
        let state = MaterializedState {
            effects: self.effects,
            conflicts: self.conflicts,
            anomalies: self.anomalies.into_values().collect(),
            objects: self.objects,
            spellings: self.spellings,
            breaks: self.breaks,
            pending: pending_vec,
        };
        (state, graph)
    }

    fn record_anomaly(&mut self, kind: IntegrityAnomalyKind) {
        let a = IntegrityAnomaly::new(kind);
        self.anomalies.entry(a.id).or_insert(a);
    }

    fn env_of(&self, id: OperationId) -> Option<&'a OperationEnvelope> {
        self.op_set.slot(id).and_then(|s| s.single())
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

    fn materialize_graph_delete(&mut self, op: &DeleteEventOp) {
        let Some(score) = self.graph.as_mut() else {
            return;
        };
        let Some(event) = score.events.remove(op.event) else {
            return;
        };
        let voice_id = event.voice();
        let location = graph_voice_location(score, voice_id);
        let region_id = location.map(|(region, _, _)| score.canvas.regions[region].id);
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
        score
            .cross_cutting
            .slurs
            .retain(|slur| slur.start_event != op.event && slur.end_event != op.event);
        score.cross_cutting.lyrics.retain_mut(|line| {
            line.events.retain(|event| *event != op.event);
            !line.events.is_empty()
        });
        if let Some(region) = region_id {
            let fallback = TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Zero,
            };
            for marker in &mut score.cross_cutting.markers {
                if matches!(marker.anchor, TimeAnchor::Event { id, .. } if id == op.event) {
                    marker.anchor = fallback.clone();
                }
            }
        }
    }

    fn materialize_graph_tombstones(&mut self, targets: &[TypedObjectId]) {
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
            self.materialize_graph_delete(&DeleteEventOp {
                event,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            });
        }

        let Some(score) = self.graph.as_mut() else {
            return;
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
                _ => {}
            }
        }
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
                OperationKind::SetUserSystemBreak(op) => self.set_user_system_break(op),
                OperationKind::DeclareTransaction(desc) => {
                    self.descriptors.insert(desc.id, env.id);
                    OperationEffect::Applied
                }
                // Extension-defined primitive: opaque to the core; recorded as
                // applied (the extension realizes its own effect).
                OperationKind::Registered(_, _) => OperationEffect::Applied,
            },
            OperationPayload::ResolveConflict(op) => self.resolve_conflict(env, op),
            OperationPayload::UndoTransaction(op) => self.undo_transaction(env, op),
        }
    }

    // --- Per-kind reduction. ------------------------------------------------

    fn set_user_system_break(
        &mut self,
        op: &crate::payload::SetUserSystemBreakOp,
    ) -> OperationEffect {
        if let Some(score) = self.graph.as_mut() {
            let Some(region) = score
                .canvas
                .regions
                .iter_mut()
                .find(|region| region.id == op.region)
            else {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            };
            let breaks = match &mut region.content {
                epiphany_core::RegionContent::StaffBased(content) => {
                    &mut content.user_system_breaks
                }
                epiphany_core::RegionContent::Hybrid { staves, .. } => {
                    &mut staves.user_system_breaks
                }
                epiphany_core::RegionContent::FreeGraphic(_) => {
                    return OperationEffect::NoOp {
                        reason: NoOpReason::PreconditionFailedUnderReduction {
                            reason: PreconditionFailureReason::TargetMissing,
                        },
                    }
                }
            };
            // The value-typed payload (v1) carries the full TimeAnchor, so the
            // graph break is the anchor itself rather than a reconstructed one.
            let anchor = op.anchor.clone();
            if op.present {
                if !breaks.contains(&anchor) {
                    breaks.push(anchor);
                }
            } else {
                breaks.retain(|candidate| candidate != &anchor);
            }
        }

        // The LWW bucketing key is the anchor's resolved musical position.
        self.breaks
            .insert((op.region, op.resolved_position()), op.present);
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
        self.materialize_graph_delete(op);

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
        let mut repairs = Vec::new();

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
        self.reanchor_for_tombstone(env, ev_obj, &mut repairs);

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

        match self.last_respell.get(&op.pitch).copied() {
            None => {
                self.spellings.insert(op.pitch, op.spelling.clone());
                self.last_respell.insert(op.pitch, env.id);
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
                        self.spellings.insert(op.pitch, op.spelling.clone());
                        self.last_respell.insert(op.pitch, env.id);
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
                    self.spellings.insert(op.pitch, op.spelling.clone());
                    self.last_respell.insert(op.pitch, env.id);
                    OperationEffect::Applied
                }
            }
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
        self.structures.insert(sid, endpoints);
        OperationEffect::Applied
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
            Some(RS::Resolved { action, .. }) => {
                if action == op.action {
                    OperationEffect::NoOp {
                        reason: NoOpReason::AlreadyApplied,
                    }
                } else {
                    // Differing concurrent resolution → meta-conflict.
                    let conflict = ConflictRecord::new(
                        ConflictKind::StructuralFieldCollision {
                            winner: env.id,
                            loser: env.id,
                            field: FieldPath("conflict_resolution".to_string()),
                        },
                        vec![env.id],
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

    fn undo_transaction(
        &mut self,
        env: &OperationEnvelope,
        op: &UndoTransactionPayload,
    ) -> OperationEffect {
        let targets = self.tx_minted.get(&op.target).cloned().unwrap_or_default();
        if targets.is_empty() {
            return OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TargetMissing,
                },
            };
        }
        let all_live = targets
            .iter()
            .all(|t| matches!(self.objects.get(t), Some(ObjectState::Live)));
        match op.policy {
            UndoPolicy::StrictInverse | UndoPolicy::Cascade => {
                if all_live {
                    let mut repairs = Vec::new();
                    for t in &targets {
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
                    self.materialize_graph_tombstones(&targets);
                    OperationEffect::AppliedWithRepair { repairs }
                } else {
                    // A target was already tombstoned/modified: strict undo conflicts.
                    let stuck = targets
                        .iter()
                        .find(|t| !matches!(self.objects.get(t), Some(ObjectState::Live)))
                        .copied()
                        .unwrap_or(targets[0]);
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
                    OperationEffect::Conflicted { conflict: cid }
                }
            }
            UndoPolicy::BestEffort => {
                let mut repairs = Vec::new();
                let mut tombstoned = Vec::new();
                for t in &targets {
                    if matches!(self.objects.get(t), Some(ObjectState::Live)) {
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
                        tombstoned.push(*t);
                    }
                }
                self.materialize_graph_tombstones(&tombstoned);
                OperationEffect::AppliedWithRepair { repairs }
            }
        }
    }

    // --- Re-anchoring (Chapter 6 §6.5 rule table, representative subset). ----

    fn reanchor_for_tombstone(
        &mut self,
        env: &OperationEnvelope,
        tombstoned: TypedObjectId,
        repairs: &mut Vec<RepairRecord>,
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
                TypedObjectId::Comment(_) | TypedObjectId::AnalyticalAnnotation(_) => {
                    // User content is never silently deleted: orphan.
                    repairs.push(RepairRecord {
                        kind: RepairKind::Orphaned,
                        target: sid,
                    });
                }
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
                TypedObjectId::Slur(_) | TypedObjectId::Spanner(_) => {
                    let survivors = self.surviving_endpoints(sid, tombstoned);
                    if survivors < 1 {
                        self.cascade_structure(env, sid, repairs);
                    } else if let Some(to) = self.nearest_survivor(sid, tombstoned) {
                        repairs.push(RepairRecord {
                            kind: RepairKind::Reanchored {
                                from: tombstoned,
                                to,
                                reason: ReanchorReason::SameVoiceNearer,
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
        // Deterministic "nearest" stand-in: the lexicographically-smallest
        // surviving endpoint (the spec's full proximity ordering needs resolved
        // positions; see DECISIONS.md).
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
            conflicts: self.conflicts.clone(),
            minted_by: self.minted_by.clone(),
            event_pitches: self.event_pitches.clone(),
            voice_occupancy: self.voice_occupancy.clone(),
            last_respell: self.last_respell.clone(),
            structures: self.structures.clone(),
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
        self.conflicts = s.conflicts;
        self.minted_by = s.minted_by;
        self.event_pitches = s.event_pitches;
        self.voice_occupancy = s.voice_occupancy;
        self.last_respell = s.last_respell;
        self.structures = s.structures;
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
}
