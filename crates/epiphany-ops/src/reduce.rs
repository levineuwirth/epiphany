//! The canonical reduction (Chapter 6 §"The Canonical Reduction").
//!
//! The materialized score state is the deterministic reduction of the operation
//! set. This module is the determinism heart of the architecture:
//!
//! * [`canonical_reduction_order`] is the **single function** that orders
//!   operations (Chapter 6 §6.3.3). The order is causal-first, then by the HLC
//!   tuple `(physical, logical, replica, counter)`. The authoring HLC rule
//!   (Chapter 6 §6.1) guarantees a causal predecessor's stamp is strictly less,
//!   so a plain lexicographic sort by that tuple already respects causal order
//!   — no topological pass is needed, and the sort key is intrinsic to each
//!   envelope, so the order is trivially independent of arrival order.
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
//! against an object-existence + spelling + LWW working state — the canonical
//! bookkeeping Chapter 6 itself owns (effect log, conflict registry, anomaly
//! register, tombstones). The full musical-graph mutation against
//! `epiphany_core::Score` is the integration point with Agent B's crate and the
//! deferred Operation Catalog (§6.11); see `DECISIONS.md` for what is modeled
//! versus deferred, and the prototype conventions (voice promotion via a
//! pre-pass, respell-winner effect tag, undo via minted-object compensation).

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    derive_promoted_voice_id, EventId, MusicalPosition, OperationId, PitchId, RegionId,
    TransactionId, TypedObjectId, VoiceId,
};
use epiphany_determinism::{CanonicalEncode, ContentHash};

use crate::anomaly::{detect_replica_anomalies, IntegrityAnomaly, IntegrityAnomalyKind};
use crate::conflict::{ConflictKind, ConflictRecord, ConflictRegistry, FieldPath};
use crate::effect::{
    NoOpReason, OperationEffect, PreconditionFailureReason, ReanchorReason, RepairKind,
    RepairRecord, TupletCompensationKind,
};
use crate::encode::{push_canon, push_len, push_u8_bool};
use crate::envelope::OperationEnvelope;
use crate::opset::OperationSet;
use crate::payload::{
    CreateCrossCuttingOp, DeleteEventOp, InsertEventOp, OperationKind, OperationPayload,
    RespellPitchOp, TupletCompensation,
};
use crate::undo::{UndoPolicy, UndoTransactionPayload};

/// Orders operation envelopes into the canonical reduction order (Chapter 6
/// §6.3.3): causal-first, then by the HLC tuple `(physical, logical, replica,
/// counter)`. Returns the envelopes in that order.
///
/// This is the single ordering function the determinism property tests against:
/// the key is intrinsic to each envelope (no dependence on input order), and the
/// authoring HLC invariant guarantees the sort respects causal order without an
/// explicit topological pass.
pub fn canonical_reduction_order<'a>(
    envelopes: &[&'a OperationEnvelope],
) -> Vec<&'a OperationEnvelope> {
    let mut ordered: Vec<&'a OperationEnvelope> = envelopes.to_vec();
    ordered.sort_by_key(|e| e.stamp.reduction_tuple());
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
    pub spellings: BTreeMap<PitchId, ContentHash>,
    /// User system-break preferences (LWW advisory), keyed by region+anchor.
    pub breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    /// Operations held pending, ordered by `OperationId`.
    pub pending: Vec<(OperationId, PendingReason)>,
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
        // Spellings (PitchId order).
        push_len(&mut out, self.spellings.len());
        for (pitch, hash) in &self.spellings {
            push_canon(&mut out, pitch);
            push_canon(&mut out, hash);
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
    Reducer::new(op_set).run()
}

/// The working state of one reduction pass.
struct Reducer<'a> {
    op_set: &'a OperationSet,
    // Canonical results.
    objects: BTreeMap<TypedObjectId, ObjectState>,
    spellings: BTreeMap<PitchId, ContentHash>,
    breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    conflicts: ConflictRegistry,
    effects: Vec<(OperationId, OperationEffect)>,
    anomalies: BTreeMap<epiphany_core::IntegrityAnomalyId, IntegrityAnomaly>,
    // Transient indices.
    minted_by: BTreeMap<TypedObjectId, OperationId>,
    event_pitches: BTreeMap<EventId, Vec<PitchId>>,
    voice_occupancy: BTreeMap<(VoiceId, MusicalPosition), EventId>,
    last_respell: BTreeMap<PitchId, OperationId>,
    structures: BTreeMap<TypedObjectId, Vec<TypedObjectId>>,
    migrated_regions: BTreeSet<RegionId>,
    region_migrator: BTreeMap<RegionId, OperationId>,
    descriptors: BTreeMap<TransactionId, OperationId>,
    promotion: BTreeMap<OperationId, VoiceId>,
    tx_minted: BTreeMap<TransactionId, Vec<TypedObjectId>>,
    current_tx: Option<TransactionId>,
}

/// A snapshot of the working state, for atomic transaction rollback.
struct WorkingSnapshot {
    objects: BTreeMap<TypedObjectId, ObjectState>,
    spellings: BTreeMap<PitchId, ContentHash>,
    breaks: BTreeMap<(RegionId, MusicalPosition), bool>,
    minted_by: BTreeMap<TypedObjectId, OperationId>,
    event_pitches: BTreeMap<EventId, Vec<PitchId>>,
    voice_occupancy: BTreeMap<(VoiceId, MusicalPosition), EventId>,
    last_respell: BTreeMap<PitchId, OperationId>,
    structures: BTreeMap<TypedObjectId, Vec<TypedObjectId>>,
    migrated_regions: BTreeSet<RegionId>,
    region_migrator: BTreeMap<RegionId, OperationId>,
    tx_minted: BTreeMap<TransactionId, Vec<TypedObjectId>>,
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
        }
    }

    fn run(mut self) -> MaterializedState {
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

        // 3. Missing-causal-predecessor rule → pending set (with reasons).
        let pending = compute_pending(&reducible, &reducible_ids, &equivocated, &excluded);
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

        MaterializedState {
            effects: self.effects,
            conflicts: self.conflicts,
            anomalies: self.anomalies.into_values().collect(),
            objects: self.objects,
            spellings: self.spellings,
            breaks: self.breaks,
            pending: pending_vec,
        }
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
        // Bucket InsertEvent ops by (voice, position).
        let mut buckets: BTreeMap<(VoiceId, MusicalPosition), Vec<&OperationEnvelope>> =
            BTreeMap::new();
        for env in active {
            if let OperationPayload::Primitive(OperationKind::InsertEvent(op)) = &env.payload {
                buckets
                    .entry((op.voice, op.position.clone()))
                    .or_default()
                    .push(env);
            }
        }
        for (_, mut bucket) in buckets {
            if bucket.len() < 2 {
                continue;
            }
            // Smallest OperationId keeps the original voice; the rest promote.
            bucket.sort_by_key(|e| e.id);
            let winner = bucket[0].id;
            for env in &bucket[1..] {
                if let OperationPayload::Primitive(OperationKind::InsertEvent(op)) = &env.payload {
                    let promoted =
                        derive_promoted_voice_id(op.staff_instance, op.voice, winner, env.id);
                    self.promotion.insert(env.id, promoted);
                }
            }
        }
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
                OperationKind::SetUserSystemBreak(op) => {
                    self.breaks
                        .insert((op.region, op.anchor.clone()), op.present);
                    OperationEffect::Applied
                }
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

    fn insert_event(&mut self, env: &OperationEnvelope, op: &InsertEventOp) -> OperationEffect {
        let ev_obj = TypedObjectId::Event(op.event);
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
        let voice_obj = TypedObjectId::Voice(op.voice);
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

        let mut repairs = Vec::new();
        let target_voice = if let Some(promoted) = self.promotion.get(&env.id).copied() {
            let pv = TypedObjectId::Voice(promoted);
            self.objects.entry(pv).or_insert(ObjectState::Live);
            self.minted_by.entry(pv).or_insert(env.id);
            repairs.push(RepairRecord {
                kind: RepairKind::VoicePromoted {
                    from: op.voice,
                    to: promoted,
                },
                target: pv,
            });
            promoted
        } else {
            op.voice
        };

        self.objects.insert(ev_obj, ObjectState::Live);
        self.minted_by.insert(ev_obj, env.id);
        self.note_minted(env, ev_obj);
        let mut pitches = Vec::new();
        for &p in &op.pitches {
            let p_obj = TypedObjectId::Pitch(p);
            self.objects.insert(p_obj, ObjectState::Live);
            self.minted_by.insert(p_obj, env.id);
            self.note_minted(env, p_obj);
            pitches.push(p);
        }
        self.event_pitches.insert(op.event, pitches);
        self.voice_occupancy
            .insert((target_voice, op.position.clone()), op.event);

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

        let minter = self.minted_by.get(&ev_obj).copied().unwrap_or(env.id);
        self.objects.insert(
            ev_obj,
            ObjectState::Tombstoned {
                deleted_by: env.id,
                minted_by: minter,
            },
        );
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
            TupletCompensation::ReplaceWithRest { new_rest, .. } => {
                let rest_obj = TypedObjectId::Event(*new_rest);
                self.objects.insert(rest_obj, ObjectState::Live);
                self.minted_by.insert(rest_obj, env.id);
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
                self.spellings.insert(op.pitch, op.spelling);
                self.last_respell.insert(op.pitch, env.id);
                OperationEffect::Applied
            }
            Some(prev_op) => {
                let prev_spelling = self.spellings.get(&op.pitch).copied();
                let concurrent = self.concurrent(env.id, prev_op);
                if concurrent {
                    if prev_spelling == Some(op.spelling) {
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
                        self.spellings.insert(op.pitch, op.spelling);
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
                    self.spellings.insert(op.pitch, op.spelling);
                    self.last_respell.insert(op.pitch, env.id);
                    OperationEffect::Applied
                }
            }
        }
    }

    fn create_cross_cutting(
        &mut self,
        _env: &OperationEnvelope,
        op: &CreateCrossCuttingOp,
    ) -> OperationEffect {
        let sid = op.structure.id;
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
        for e in &op.structure.endpoints {
            if !matches!(self.objects.get(e), Some(ObjectState::Live)) {
                return OperationEffect::NoOp {
                    reason: NoOpReason::PreconditionFailedUnderReduction {
                        reason: PreconditionFailureReason::TargetMissing,
                    },
                };
            }
        }
        self.objects.insert(sid, ObjectState::Live);
        self.minted_by.insert(sid, _env.id);
        self.structures.insert(sid, op.structure.endpoints.clone());
        OperationEffect::Applied
    }

    fn change_region_time_model(
        &mut self,
        env: &OperationEnvelope,
        op: &crate::payload::ChangeRegionTimeModelOp,
    ) -> OperationEffect {
        if self.migrated_regions.contains(&op.region) {
            // Concurrent same-target migration: earlier applies, later conflicts.
            let winner = self
                .region_migrator
                .get(&op.region)
                .copied()
                .unwrap_or(env.id);
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
        if !op.declared_incompatible.is_empty() {
            let incompatible: Vec<TypedObjectId> = op
                .declared_incompatible
                .iter()
                .map(|e| TypedObjectId::Event(*e))
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
                    rec.resolution_state = RS::Resolved {
                        by: env.id,
                        action: op.action,
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
                    }
                }
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
            minted_by: self.minted_by.clone(),
            event_pitches: self.event_pitches.clone(),
            voice_occupancy: self.voice_occupancy.clone(),
            last_respell: self.last_respell.clone(),
            structures: self.structures.clone(),
            migrated_regions: self.migrated_regions.clone(),
            region_migrator: self.region_migrator.clone(),
            tx_minted: self.tx_minted.clone(),
        }
    }

    fn restore(&mut self, s: WorkingSnapshot) {
        self.objects = s.objects;
        self.spellings = s.spellings;
        self.breaks = s.breaks;
        self.minted_by = s.minted_by;
        self.event_pitches = s.event_pitches;
        self.voice_occupancy = s.voice_occupancy;
        self.last_respell = s.last_respell;
        self.structures = s.structures;
        self.migrated_regions = s.migrated_regions;
        self.region_migrator = s.region_migrator;
        self.tx_minted = s.tx_minted;
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
) -> BTreeMap<OperationId, PendingReason> {
    let mut blocked: BTreeMap<OperationId, PendingReason> = BTreeMap::new();

    // Direct causes: dots referencing non-reducible ids, and vector coverage of
    // known equivocated/excluded ids.
    for env in reducible {
        let mut causes: Vec<(OperationId, PendingReason)> = Vec::new();
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
                voice: VoiceId::new(ReplicaId(9), voice),
                staff_instance: StaffInstanceId::new(ReplicaId(9), 0),
                event: EventId::new(ReplicaId(replica), event),
                position: pos(pos_units),
                duration: epiphany_core::MusicalDuration::whole(),
                pitches: vec![],
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
}
