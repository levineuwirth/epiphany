//! Operation payloads: [`OperationKind`], the discriminator-only
//! [`OperationKindTag`], [`OperationPayload`], and the representative operation
//! structs the chapter specifies reduction rules for (Chapter 6 §"The Operation
//! Framework", §"Representative Operations").
//!
//! ## Why these payloads carry identifiers, not whole graph objects
//!
//! The spec's payload structs embed rich graph values — `InsertEventOp` holds an
//! `Event`, `RespellPitchOp` holds a `PitchSpelling`, and so on. Their *canonical
//! wire encoding*, however, is deferred to the Binary Format companion (Agent B's
//! `epiphany-core` canonically encodes only identifiers and the scalar time
//! types; the value-type encoding is its Pass 11 candidate P11-4). To keep an
//! [`OperationEnvelope`](crate::OperationEnvelope) **hashable today** — the
//! [`EnvelopeHash`](crate::EnvelopeHash) and slot equivocation both need
//! canonical bytes — these payloads carry the reduction-relevant *identifiers and
//! canonical scalar coordinates*, plus a content fingerprint where the reduction
//! only needs equality (a respelling's [`ContentHash`]). This is faithful to
//! everything Chapter 6's reduction rules actually consume, and is recorded as a
//! Pass 11 candidate (see `DECISIONS.md`): when the companion lands, the structs
//! grow back their full value fields without changing the reduction.
//!
//! The *set* of kinds here is the representative selection of §6.10, not the full
//! ~60–80-primitive catalog (an explicit open question, §6.11). Together they
//! exercise every reduction discipline the chapter defines.

use epiphany_core::{
    EventId, MusicalDuration, MusicalPosition, PitchId, RegionId, StaffInstanceId, TransactionId,
    TupletId, TypedObjectId, VoiceId,
};
use epiphany_determinism::{sorted_canonical, CanonicalEncode, ContentHash};

use crate::conflict::{ConflictId, ResolutionAction};
use crate::encode::{push_canon, push_seq, push_str, push_tag, push_u8_bool};
use crate::support::OperationKindRegistryId;
use crate::undo::UndoTransactionPayload;

/// The full payload of an operation envelope: a primitive, or one of the two
/// meta-operations (Chapter 6 §"Operation Envelopes").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OperationPayload {
    /// A primitive mutation.
    Primitive(OperationKind),
    /// A meta-operation that resolves a previously-recorded conflict.
    ResolveConflict(ResolveConflictPayload),
    /// A meta-operation that compensates for a previously-committed
    /// transaction; the realization of "undo".
    UndoTransaction(UndoTransactionPayload),
}

impl OperationPayload {
    fn discriminant(&self) -> u8 {
        match self {
            OperationPayload::Primitive(_) => 0,
            OperationPayload::ResolveConflict(_) => 1,
            OperationPayload::UndoTransaction(_) => 2,
        }
    }
}

impl CanonicalEncode for OperationPayload {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            OperationPayload::Primitive(k) => k.encode_canonical(out),
            OperationPayload::ResolveConflict(p) => p.encode_canonical(out),
            OperationPayload::UndoTransaction(p) => p.encode_canonical(out),
        }
    }
}

/// The catalog of primitive operation kinds reduced by this crate (Chapter 6
/// §"Operation Envelopes", representative subset of §6.10).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OperationKind {
    /// Insert an event into a voice (position-keyed; voice promotion on
    /// concurrent overlap).
    InsertEvent(InsertEventOp),
    /// Tombstone an event and its pitches, with tuplet compensation.
    DeleteEvent(DeleteEventOp),
    /// Overwrite a pitch's spelling (later-in-canonical-order wins).
    RespellPitch(RespellPitchOp),
    /// Create a cross-cutting structure (set union).
    CreateCrossCutting(CreateCrossCuttingOp),
    /// Change a region's time model (structural migration).
    ChangeRegionTimeModel(ChangeRegionTimeModelOp),
    /// Set a user system-break preference (LWW advisory).
    SetUserSystemBreak(SetUserSystemBreakOp),
    /// Declare a transaction; member primitives reference it by id.
    DeclareTransaction(TransactionDescriptor),
    /// An extension-defined primitive operation (opaque serialized payload).
    Registered(OperationKindRegistryId, Vec<u8>),
}

impl OperationKind {
    fn discriminant(&self) -> u8 {
        match self {
            OperationKind::InsertEvent(_) => 0,
            OperationKind::DeleteEvent(_) => 1,
            OperationKind::RespellPitch(_) => 2,
            OperationKind::CreateCrossCutting(_) => 3,
            OperationKind::ChangeRegionTimeModel(_) => 4,
            OperationKind::SetUserSystemBreak(_) => 5,
            OperationKind::DeclareTransaction(_) => 6,
            OperationKind::Registered(..) => 7,
        }
    }

    /// The discriminator-only [`OperationKindTag`] for this kind. Used by edit
    /// barriers (Chapter 7/8 `prohibited_operation_kinds`) to name a kind
    /// without its payload.
    pub fn tag(&self) -> OperationKindTag {
        match self {
            OperationKind::InsertEvent(_) => OperationKindTag::InsertEvent,
            OperationKind::DeleteEvent(_) => OperationKindTag::DeleteEvent,
            OperationKind::RespellPitch(_) => OperationKindTag::RespellPitch,
            OperationKind::CreateCrossCutting(_) => OperationKindTag::CreateCrossCutting,
            OperationKind::ChangeRegionTimeModel(_) => OperationKindTag::ChangeRegionTimeModel,
            OperationKind::SetUserSystemBreak(_) => OperationKindTag::SetUserSystemBreak,
            OperationKind::DeclareTransaction(_) => OperationKindTag::DeclareTransaction,
            OperationKind::Registered(id, _) => OperationKindTag::Registered(*id),
        }
    }
}

impl CanonicalEncode for OperationKind {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            OperationKind::InsertEvent(op) => op.encode_canonical(out),
            OperationKind::DeleteEvent(op) => op.encode_canonical(out),
            OperationKind::RespellPitch(op) => op.encode_canonical(out),
            OperationKind::CreateCrossCutting(op) => op.encode_canonical(out),
            OperationKind::ChangeRegionTimeModel(op) => op.encode_canonical(out),
            OperationKind::SetUserSystemBreak(op) => op.encode_canonical(out),
            OperationKind::DeclareTransaction(op) => op.encode_canonical(out),
            OperationKind::Registered(id, bytes) => {
                push_canon(out, id);
                crate::encode::push_lp_bytes(out, bytes);
            }
        }
    }
}

/// The discriminator-only projection of [`OperationKind`] (Chapter 6; the type
/// edit barriers store in `prohibited_operation_kinds`). Carries no payload —
/// except the registry id for [`Registered`](OperationKindTag::Registered), so a
/// barrier can prohibit a specific extension kind.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum OperationKindTag {
    InsertEvent,
    DeleteEvent,
    ModifyEvent,
    RespellPitch,
    Transpose,
    CreateCrossCutting,
    DeleteCrossCutting,
    ModifyCrossCutting,
    ChangeRegionTimeModel,
    InsertRegion,
    DeleteRegion,
    InsertStaffInstance,
    DeleteStaffInstance,
    SetUserSystemBreak,
    SetUserPageBreak,
    DeclareTransaction,
    Registered(OperationKindRegistryId),
}

impl OperationKindTag {
    fn discriminant(&self) -> u8 {
        match self {
            OperationKindTag::InsertEvent => 0,
            OperationKindTag::DeleteEvent => 1,
            OperationKindTag::ModifyEvent => 2,
            OperationKindTag::RespellPitch => 3,
            OperationKindTag::Transpose => 4,
            OperationKindTag::CreateCrossCutting => 5,
            OperationKindTag::DeleteCrossCutting => 6,
            OperationKindTag::ModifyCrossCutting => 7,
            OperationKindTag::ChangeRegionTimeModel => 8,
            OperationKindTag::InsertRegion => 9,
            OperationKindTag::DeleteRegion => 10,
            OperationKindTag::InsertStaffInstance => 11,
            OperationKindTag::DeleteStaffInstance => 12,
            OperationKindTag::SetUserSystemBreak => 13,
            OperationKindTag::SetUserPageBreak => 14,
            OperationKindTag::DeclareTransaction => 15,
            OperationKindTag::Registered(_) => 16,
        }
    }
}

impl CanonicalEncode for OperationKindTag {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        if let OperationKindTag::Registered(id) = self {
            push_canon(out, id);
        }
    }
}

// --- Representative operation payloads (Chapter 6 §6.10). --------------------

/// Insert an event into a voice (Chapter 6 §6.10 InsertEvent).
///
/// The `staff_instance` makes the system-promoted-voice derivation total
/// without a containment walk (a full reducer recovers it from the voice's
/// container; see `DECISIONS.md`). `position`/`duration` are exact musical
/// rationals, the collision key for voice promotion. `pitches` are tombstoned
/// with the event on delete.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InsertEventOp {
    pub voice: VoiceId,
    pub staff_instance: StaffInstanceId,
    pub event: EventId,
    pub position: MusicalPosition,
    pub duration: MusicalDuration,
    pub pitches: Vec<PitchId>,
}

impl CanonicalEncode for InsertEventOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.voice);
        push_canon(out, &self.staff_instance);
        push_canon(out, &self.event);
        push_canon(out, &self.position);
        push_canon(out, &self.duration);
        push_seq(out, &sorted_canonical(self.pitches.clone()));
    }
}

/// Tombstone an event and its pitches (Chapter 6 §6.10 DeleteEvent).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeleteEventOp {
    pub event: EventId,
    pub tuplet_compensation: TupletCompensation,
}

impl CanonicalEncode for DeleteEventOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.event);
        self.tuplet_compensation.encode_canonical(out);
    }
}

/// Compensation for a deleted event's tuplet membership (Chapter 6 §6.10).
/// The replacement rest is represented by its freshly-minted [`EventId`] and
/// duration (the reducer adds it live and validates the duration); the full
/// `Rest` value is deferred with the rest of the payload encoding.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TupletCompensation {
    /// Target is not in any tuplet.
    NotInTuplet,
    /// Replace the deleted event with a rest of the same duration.
    ReplaceWithRest {
        new_rest: EventId,
        duration: MusicalDuration,
    },
    /// Rewrite the enclosing tuplet(s) to remain consistent.
    RewriteTuplets { tuplets: Vec<TupletId> },
    /// Cascade-delete the tuplet group(s) containing the target.
    CascadeDeleteTuplets { tuplets: Vec<TupletId> },
}

impl TupletCompensation {
    fn discriminant(&self) -> u8 {
        match self {
            TupletCompensation::NotInTuplet => 0,
            TupletCompensation::ReplaceWithRest { .. } => 1,
            TupletCompensation::RewriteTuplets { .. } => 2,
            TupletCompensation::CascadeDeleteTuplets { .. } => 3,
        }
    }

    /// Whether this declares any tuplet compensation (i.e. is not
    /// [`NotInTuplet`](TupletCompensation::NotInTuplet)).
    pub fn is_compensating(&self) -> bool {
        !matches!(self, TupletCompensation::NotInTuplet)
    }
}

impl CanonicalEncode for TupletCompensation {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            TupletCompensation::NotInTuplet => {}
            TupletCompensation::ReplaceWithRest { new_rest, duration } => {
                push_canon(out, new_rest);
                push_canon(out, duration);
            }
            TupletCompensation::RewriteTuplets { tuplets }
            | TupletCompensation::CascadeDeleteTuplets { tuplets } => {
                push_seq(out, &sorted_canonical(tuplets.clone()));
            }
        }
    }
}

/// Overwrite a pitch's spelling (Chapter 6 §6.10 RespellPitch). The intended
/// [`PitchSpelling`](epiphany_core::PitchSpelling) is represented by a
/// [`ContentHash`] fingerprint: the reduction only needs spelling *equality*
/// (identical concurrent respellings reduce idempotently; differing ones
/// conflict).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RespellPitchOp {
    pub pitch: PitchId,
    pub spelling: ContentHash,
}

impl CanonicalEncode for RespellPitchOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.pitch);
        push_canon(out, &self.spelling);
    }
}

/// Create a cross-cutting structure (Chapter 6 §6.10 CreateCrossCutting). The
/// structure is identified by its [`TypedObjectId`] (whose variant carries the
/// kind — Slur, Tie, Beam, …) and its referenced endpoints; that is everything
/// the set-union reduction and the re-anchoring rule table consume.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateCrossCuttingOp {
    pub structure: CrossCuttingRef,
}

impl CanonicalEncode for CreateCrossCuttingOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        self.structure.encode_canonical(out);
    }
}

/// A reference-level view of a cross-cutting structure: its identity and the
/// objects it references (Chapter 6 §6.5 re-anchoring operands).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CrossCuttingRef {
    /// The structure's identity; its `TypedObjectId` variant names the kind.
    pub id: TypedObjectId,
    /// The objects this structure references (its endpoints/anchors).
    pub endpoints: Vec<TypedObjectId>,
}

impl CanonicalEncode for CrossCuttingRef {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.id);
        // Endpoint order is meaningful (start before end), so it is NOT sorted.
        push_seq(out, &self.endpoints);
    }
}

/// Change a region's time model (Chapter 6 §6.10 ChangeRegionTimeModel). The
/// target model is represented by its [`RegionTimeModelTag`]; the prototype's
/// reducer cannot recompute coordinate-kind compatibility from the not-yet-
/// canonical region contents, so the authoring layer declares any events it
/// knows to be un-migratable (`declared_incompatible`), which drive the
/// `TimeModelMigrationFailure` conflict (see `DECISIONS.md`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChangeRegionTimeModelOp {
    pub region: RegionId,
    pub new_time_model: RegionTimeModelTag,
    pub declared_incompatible: Vec<EventId>,
    pub remapping: PositionRemapping,
}

impl CanonicalEncode for ChangeRegionTimeModelOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        self.new_time_model.encode_canonical(out);
        push_seq(out, &sorted_canonical(self.declared_incompatible.clone()));
        self.remapping.encode_canonical(out);
    }
}

/// Which region time model a migration targets (Chapter 3 §"Region Time
/// Models"). The discriminator the reducer needs; the full
/// [`RegionTimeModel`](epiphany_core::RegionTimeModel) value is deferred.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RegionTimeModelTag {
    Metric,
    Proportional,
    Aleatoric,
}

impl RegionTimeModelTag {
    fn discriminant(&self) -> u8 {
        match self {
            RegionTimeModelTag::Metric => 0,
            RegionTimeModelTag::Proportional => 1,
            RegionTimeModelTag::Aleatoric => 2,
        }
    }
}

impl CanonicalEncode for RegionTimeModelTag {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
    }
}

/// How event positions are remapped under a time-model change (Chapter 6
/// §6.10). `PreserveTime`'s converter and `Reassign`'s full event positions
/// follow the deferred payload encoding; the reducer consumes the event set.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PositionRemapping {
    /// Preserve absolute time positions where possible.
    PreserveTime,
    /// Reassign positions explicitly.
    Reassign(Vec<(EventId, MusicalPosition)>),
}

impl PositionRemapping {
    fn discriminant(&self) -> u8 {
        match self {
            PositionRemapping::PreserveTime => 0,
            PositionRemapping::Reassign(_) => 1,
        }
    }
}

impl CanonicalEncode for PositionRemapping {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        if let PositionRemapping::Reassign(map) = self {
            // Canonicalize: ascending by EventId so equal mappings built in
            // different orders encode identically (Appendix D §Ordered Iteration).
            let mut entries = map.clone();
            entries.sort_by_key(|(e, _)| e.canonical_bytes());
            push_len_pairs(out, &entries);
        }
    }
}

fn push_len_pairs(out: &mut Vec<u8>, entries: &[(EventId, MusicalPosition)]) {
    crate::encode::push_len(out, entries.len());
    for (e, p) in entries {
        push_canon(out, e);
        let mut scratch = Vec::new();
        p.encode_canonical(&mut scratch);
        crate::encode::push_lp_bytes(out, &scratch);
    }
}

/// Set a user system-break preference (Chapter 6 §6.10 SetUserSystemBreak). The
/// anchor is represented by its resolved [`MusicalPosition`] — the canonical
/// LWW bucketing key; the full [`TimeAnchor`](epiphany_core::TimeAnchor) value
/// is deferred.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetUserSystemBreakOp {
    pub region: RegionId,
    pub anchor: MusicalPosition,
    pub present: bool,
}

impl CanonicalEncode for SetUserSystemBreakOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        push_canon(out, &self.anchor);
        push_u8_bool(out, self.present);
    }
}

/// A transaction descriptor (Chapter 6 §6.6). The descriptor is a separate
/// `DeclareTransaction` envelope; member primitives reference its id and MUST
/// causally depend on it (Chapter 6 §6.6.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TransactionDescriptor {
    pub id: TransactionId,
    /// Human-readable label, for undo history and diagnostics (NFC canonical).
    pub label: String,
    /// Optional categorization, used by UIs and analytics.
    pub category: Option<TransactionCategory>,
}

impl CanonicalEncode for TransactionDescriptor {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.id);
        push_str(out, &self.label);
        match &self.category {
            None => push_tag(out, 0),
            Some(c) => {
                push_tag(out, 1);
                c.encode_canonical(out);
            }
        }
    }
}

/// A transaction category (Chapter 6 §6.6). The spec leaves the set open ("used
/// by UIs and analytics"); these are a minimal core set plus a registered
/// escape (see `DECISIONS.md`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TransactionCategory {
    NoteEntry,
    Structural,
    Layout,
    Import,
    Registered(OperationKindRegistryId),
}

impl TransactionCategory {
    fn discriminant(&self) -> u8 {
        match self {
            TransactionCategory::NoteEntry => 0,
            TransactionCategory::Structural => 1,
            TransactionCategory::Layout => 2,
            TransactionCategory::Import => 3,
            TransactionCategory::Registered(_) => 4,
        }
    }
}

impl CanonicalEncode for TransactionCategory {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        if let TransactionCategory::Registered(id) = self {
            push_canon(out, id);
        }
    }
}

/// The payload of a conflict-resolution meta-operation (Chapter 6 §6.4.4).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ResolveConflictPayload {
    pub target: ConflictId,
    pub action: ResolutionAction,
}

impl CanonicalEncode for ResolveConflictPayload {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.target);
        self.action.encode_canonical(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{ReplicaId, SlurId};

    #[test]
    fn kind_tag_projects_and_round_trips_registered_id() {
        let reg = OperationKindRegistryId(7);
        let k = OperationKind::Registered(reg, vec![1, 2, 3]);
        assert_eq!(k.tag(), OperationKindTag::Registered(reg));
        let prim = OperationKind::RespellPitch(RespellPitchOp {
            pitch: PitchId::new(ReplicaId(1), 1),
            spelling: ContentHash([4u8; 32]),
        });
        assert_eq!(prim.tag(), OperationKindTag::RespellPitch);
    }

    #[test]
    fn every_normative_operation_tag_has_a_distinct_canonical_discriminant() {
        let tags = [
            OperationKindTag::InsertEvent,
            OperationKindTag::DeleteEvent,
            OperationKindTag::ModifyEvent,
            OperationKindTag::RespellPitch,
            OperationKindTag::Transpose,
            OperationKindTag::CreateCrossCutting,
            OperationKindTag::DeleteCrossCutting,
            OperationKindTag::ModifyCrossCutting,
            OperationKindTag::ChangeRegionTimeModel,
            OperationKindTag::InsertRegion,
            OperationKindTag::DeleteRegion,
            OperationKindTag::InsertStaffInstance,
            OperationKindTag::DeleteStaffInstance,
            OperationKindTag::SetUserSystemBreak,
            OperationKindTag::SetUserPageBreak,
            OperationKindTag::DeclareTransaction,
        ];
        let encoded: std::collections::BTreeSet<_> = tags
            .iter()
            .map(CanonicalEncode::to_canonical_bytes)
            .collect();
        assert_eq!(encoded.len(), tags.len());
    }

    #[test]
    fn reassign_remapping_is_order_independent() {
        let e1 = EventId::new(ReplicaId(1), 1);
        let e2 = EventId::new(ReplicaId(1), 2);
        let p = MusicalPosition::default();
        let a = PositionRemapping::Reassign(vec![(e1, p.clone()), (e2, p.clone())]);
        let b = PositionRemapping::Reassign(vec![(e2, p.clone()), (e1, p.clone())]);
        assert_eq!(a.to_canonical_bytes(), b.to_canonical_bytes());
    }

    #[test]
    fn cross_cutting_endpoint_order_is_significant() {
        let s = TypedObjectId::Slur(SlurId::new(ReplicaId(1), 1));
        let a = TypedObjectId::Event(EventId::new(ReplicaId(1), 2));
        let b = TypedObjectId::Event(EventId::new(ReplicaId(1), 3));
        let fwd = CrossCuttingRef {
            id: s,
            endpoints: vec![a, b],
        };
        let rev = CrossCuttingRef {
            id: s,
            endpoints: vec![b, a],
        };
        assert_ne!(fwd.to_canonical_bytes(), rev.to_canonical_bytes());
    }
}
