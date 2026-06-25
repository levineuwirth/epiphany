//! Operation payloads: [`OperationKind`], the discriminator-only
//! [`OperationKindTag`], [`OperationPayload`], and the representative operation
//! structs the chapter specifies reduction rules for (Chapter 6 §"The Operation
//! Framework", §"Representative Operations").
//!
//! ## These payloads are value-typed (Operation Catalog, v1)
//!
//! Track B's Operation Catalog (Agent K) shifted these payloads from the v0
//! *identifier-only projection* (an `InsertEvent` that carried only an `EventId`
//! plus scalars; a `RespellPitch` that carried only a [`ContentHash`] fingerprint
//! of the new spelling) to the **value-typed** form the spec describes:
//! `InsertEventOp` carries the real [`Event`], `RespellPitchOp` carries the real
//! [`PitchSpelling`], `CreateCrossCuttingOp` carries the real cross-cutting
//! structure, and so on. This is what makes an operation *durable* — replayable
//! in a fresh context (a backup restore, a cross-tool round-trip) without the
//! originating graph.
//!
//! The payloads serialize canonically by embedding each value's
//! [`CanonicalValue`] bytes behind a `u32` length prefix (the same ratified byte
//! layout `epiphany-core`'s whole-score codec uses — Pass 11 item 1.8,
//! `req:format:codec-conventions`), so an [`OperationEnvelope`](crate::OperationEnvelope)
//! stays hashable ([`EnvelopeHash`](crate::EnvelopeHash) / slot equivocation). The
//! v0 shapes are retained in [`crate::v0`] solely as the migration regression
//! guard; the [`migrate_v0_envelope`](crate::migrate_v0_envelope) path lifts a v0
//! envelope to this v1 form (deterministically, preserving canonical reduction
//! state). See `DECISIONS.md` (P11-C1 resolved; P12-K1).
//!
//! The *set* of kinds here is the representative selection of §6.10, not the full
//! ~60–80-primitive catalog (an explicit open question, §6.11). Together they
//! exercise every reduction discipline the chapter defines. The Operation Catalog
//! companion (`spec/operation_catalog.tex`) is the normative schema for these
//! payloads.

use epiphany_core::{
    Beam, CanonicalValue, Event, EventDuration, EventId, EventPosition, IdentifiedPitch,
    MusicalDuration, MusicalPosition, Pitch, PitchId, PitchSpelling, RegionId, RegionTimeModel,
    Rest, Slur, Spanner, StaffInstanceId, Tie, TimeAnchor, TransactionId, TupletId, TypedObjectId,
    VoiceId,
};
use epiphany_determinism::{sorted_canonical, CanonicalEncode};

use crate::conflict::{ConflictId, ResolutionAction};
use crate::encode::{push_canon, push_lp_bytes, push_seq, push_str, push_tag, push_u8_bool};
use crate::support::OperationKindRegistryId;
use crate::undo::UndoTransactionPayload;

/// The full payload of an operation envelope: a primitive, or one of the two
/// meta-operations (Chapter 6 §"Operation Envelopes").
// v1 payloads carry whole graph values, so the `Primitive` variant is
// intentionally larger than the meta-operations — the durability the catalog
// requires. Operation payloads are not packed in a hot path, so the size
// disparity is acceptable rather than worth a `Box` indirection on every match.
#[allow(clippy::large_enum_variant)]
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
// `InsertEvent` carries a whole `Event`, so this variant is intentionally larger
// than the others (see `OperationPayload`); inline values are the v1 design.
#[allow(clippy::large_enum_variant)]
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
    // --- Group 1 (M2): event & pitch leaf-field ops. Discriminants extend the
    // stable v1 wire form (0..=7 above) additively. ---
    /// Overwrite a live event's value (later-in-canonical-order wins).
    ModifyEvent(ModifyEventOp),
    /// Transpose live pitches by an interval (order-dependent; ids preserved).
    Transpose(TransposeOp),
    /// Add a pitch to a live event (mint).
    InsertIdentifiedPitch(InsertIdentifiedPitchOp),
    /// Tombstone a live pitch (delete-wins).
    DeleteIdentifiedPitch(DeleteIdentifiedPitchOp),
    /// Overwrite a live pitch's value (later-in-canonical-order wins).
    ModifyIdentifiedPitch(ModifyIdentifiedPitchOp),
    // --- Group 2 (M2): cross-cutting CRUD. Discriminants extend additively. ---
    /// Tombstone a cross-cutting structure (delete-wins).
    DeleteCrossCutting(DeleteCrossCuttingOp),
    /// Overwrite a cross-cutting structure's value (later-in-canonical-order
    /// wins).
    ModifyCrossCutting(ModifyCrossCuttingOp),
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
            OperationKind::ModifyEvent(_) => 8,
            OperationKind::Transpose(_) => 9,
            OperationKind::InsertIdentifiedPitch(_) => 10,
            OperationKind::DeleteIdentifiedPitch(_) => 11,
            OperationKind::ModifyIdentifiedPitch(_) => 12,
            OperationKind::DeleteCrossCutting(_) => 13,
            OperationKind::ModifyCrossCutting(_) => 14,
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
            OperationKind::ModifyEvent(_) => OperationKindTag::ModifyEvent,
            OperationKind::Transpose(_) => OperationKindTag::Transpose,
            OperationKind::InsertIdentifiedPitch(_) => OperationKindTag::InsertIdentifiedPitch,
            OperationKind::DeleteIdentifiedPitch(_) => OperationKindTag::DeleteIdentifiedPitch,
            OperationKind::ModifyIdentifiedPitch(_) => OperationKindTag::ModifyIdentifiedPitch,
            OperationKind::DeleteCrossCutting(_) => OperationKindTag::DeleteCrossCutting,
            OperationKind::ModifyCrossCutting(_) => OperationKindTag::ModifyCrossCutting,
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
            OperationKind::ModifyEvent(op) => op.encode_canonical(out),
            OperationKind::Transpose(op) => op.encode_canonical(out),
            OperationKind::InsertIdentifiedPitch(op) => op.encode_canonical(out),
            OperationKind::DeleteIdentifiedPitch(op) => op.encode_canonical(out),
            OperationKind::ModifyIdentifiedPitch(op) => op.encode_canonical(out),
            OperationKind::DeleteCrossCutting(op) => op.encode_canonical(out),
            OperationKind::ModifyCrossCutting(op) => op.encode_canonical(out),
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
    InsertIdentifiedPitch,
    DeleteIdentifiedPitch,
    ModifyIdentifiedPitch,
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
            OperationKindTag::InsertIdentifiedPitch => 17,
            OperationKindTag::DeleteIdentifiedPitch => 18,
            OperationKindTag::ModifyIdentifiedPitch => 19,
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

/// Insert an event into a voice (Chapter 6 §6.10 InsertEvent). Carries the full
/// [`Event`] value (v1, value-typed); the voice, position, duration, and pitch
/// identities the reduction keys on are read from it via the accessors below.
///
/// `staff_instance` is retained alongside the event so the system-promoted-voice
/// derivation is total without a containment walk (a full reducer recovers it
/// from the voice's container; see `DECISIONS.md`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InsertEventOp {
    pub staff_instance: StaffInstanceId,
    pub event: Event,
}

impl InsertEventOp {
    /// The voice the event is inserted into (the bucketing / promotion key).
    pub fn voice(&self) -> VoiceId {
        self.event.voice()
    }

    /// The inserted event's identifier.
    pub fn event_id(&self) -> EventId {
        self.event.id()
    }

    /// The pitch identities the event embeds (minted live with the event;
    /// tombstoned with it on delete).
    pub fn pitch_ids(&self) -> Vec<PitchId> {
        let mut ips: Vec<&epiphany_core::IdentifiedPitch> = Vec::new();
        self.event.collect_identified_pitches(&mut ips);
        ips.iter().map(|ip| ip.id).collect()
    }

    /// The event's musical position — the voice-promotion collision key. A
    /// non-musical position (reachable only in a non-metric region, which the
    /// graph precondition rejects for InsertEvent) reads as the origin.
    pub fn musical_position(&self) -> MusicalPosition {
        match self.event.position() {
            EventPosition::Musical(p) => p.clone(),
            EventPosition::WallClock(_) => MusicalPosition::origin(),
        }
    }

    /// The event's musical duration — the other half of the collision interval.
    pub fn musical_duration(&self) -> MusicalDuration {
        match self.event.duration() {
            EventDuration::Musical(d) => d.clone(),
            EventDuration::WallClock(_) | EventDuration::Indeterminate(_) => {
                MusicalDuration::zero()
            }
        }
    }
}

impl CanonicalEncode for InsertEventOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.staff_instance);
        push_lp_bytes(out, &self.event.canonical_bytes());
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
/// The replacement rest carries its full [`Rest`] value (v1, value-typed); the
/// reducer adds it live and validates the duration.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TupletCompensation {
    /// Target is not in any tuplet.
    NotInTuplet,
    /// Replace the deleted event with a rest of the same duration.
    ReplaceWithRest { rest: Rest },
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
            TupletCompensation::ReplaceWithRest { rest } => {
                push_lp_bytes(out, &rest.canonical_bytes());
            }
            TupletCompensation::RewriteTuplets { tuplets }
            | TupletCompensation::CascadeDeleteTuplets { tuplets } => {
                push_seq(out, &sorted_canonical(tuplets.clone()));
            }
        }
    }
}

/// Overwrite a pitch's spelling (Chapter 6 §6.10 RespellPitch). Carries the full
/// [`PitchSpelling`] value (v1, value-typed); the reduction needs spelling
/// *equality* (identical concurrent respellings reduce idempotently; differing
/// ones conflict), which is now structural value equality rather than a
/// fingerprint comparison.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RespellPitchOp {
    pub pitch: PitchId,
    pub spelling: PitchSpelling,
}

impl CanonicalEncode for RespellPitchOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.pitch);
        push_lp_bytes(out, &self.spelling.canonical_bytes());
    }
}

/// Create a cross-cutting structure (Chapter 6 §6.10 CreateCrossCutting). Carries
/// the full typed structure value (v1, value-typed); the set-union reduction and
/// the re-anchoring rule table key on its [`CrossCuttingValue::id`] and
/// [`CrossCuttingValue::endpoints`], which it derives from the value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateCrossCuttingOp {
    pub structure: CrossCuttingValue,
}

impl CanonicalEncode for CreateCrossCuttingOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        self.structure.encode_canonical(out);
    }
}

/// A cross-cutting structure value (Chapter 5 §"Cross-Cutting Structures"): the
/// representative event-anchored family the reduction materializes. The reduction
/// keys only on the [`id`](CrossCuttingValue::id) and
/// [`endpoints`](CrossCuttingValue::endpoints); the rich per-kind fields (a tie's
/// class, a beam's level) carry through to the graph materialization.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CrossCuttingValue {
    Tie(Tie),
    Slur(Slur),
    Beam(Beam),
    Spanner(Spanner),
}

impl CrossCuttingValue {
    fn discriminant(&self) -> u8 {
        match self {
            CrossCuttingValue::Tie(_) => 0,
            CrossCuttingValue::Slur(_) => 1,
            CrossCuttingValue::Beam(_) => 2,
            CrossCuttingValue::Spanner(_) => 3,
        }
    }

    /// The structure's identity; its [`TypedObjectId`] variant names the kind.
    pub fn id(&self) -> TypedObjectId {
        match self {
            CrossCuttingValue::Tie(t) => TypedObjectId::Tie(t.id),
            CrossCuttingValue::Slur(s) => TypedObjectId::Slur(s.id),
            CrossCuttingValue::Beam(b) => TypedObjectId::Beam(b.id),
            CrossCuttingValue::Spanner(s) => TypedObjectId::Spanner(s.id),
        }
    }

    /// The objects this structure references (its endpoints/anchors), in
    /// significant order (start before end). A spanner contributes the event
    /// ids of any [`TimeAnchor::Event`] endpoints it carries.
    pub fn endpoints(&self) -> Vec<TypedObjectId> {
        match self {
            CrossCuttingValue::Tie(t) => vec![
                TypedObjectId::Event(t.start_event),
                TypedObjectId::Event(t.end_event),
            ],
            CrossCuttingValue::Slur(s) => vec![
                TypedObjectId::Event(s.start_event),
                TypedObjectId::Event(s.end_event),
            ],
            CrossCuttingValue::Beam(b) => {
                b.events.iter().copied().map(TypedObjectId::Event).collect()
            }
            CrossCuttingValue::Spanner(s) => [&s.start, &s.end]
                .into_iter()
                .filter_map(|anchor| match anchor {
                    TimeAnchor::Event { id, .. } => Some(TypedObjectId::Event(*id)),
                    _ => None,
                })
                .collect(),
        }
    }
}

impl CanonicalEncode for CrossCuttingValue {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        let bytes = match self {
            CrossCuttingValue::Tie(t) => t.canonical_bytes(),
            CrossCuttingValue::Slur(s) => s.canonical_bytes(),
            CrossCuttingValue::Beam(b) => b.canonical_bytes(),
            CrossCuttingValue::Spanner(s) => s.canonical_bytes(),
        };
        push_lp_bytes(out, &bytes);
    }
}

/// Change a region's time model (Chapter 6 §6.10 ChangeRegionTimeModel). Carries
/// the full target [`RegionTimeModel`] value (v1, value-typed); the reduction
/// keys coordinate-kind compatibility on the value's *kind*. The authoring layer
/// additionally declares any events it knows to be un-migratable
/// (`declared_incompatible`), which drive the `TimeModelMigrationFailure`
/// conflict (see `DECISIONS.md`, P11-C6 — graph-aware reduction derives the rest
/// from the region contents).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChangeRegionTimeModelOp {
    pub region: RegionId,
    pub new_time_model: RegionTimeModel,
    pub declared_incompatible: Vec<EventId>,
    pub remapping: PositionRemapping,
}

impl CanonicalEncode for ChangeRegionTimeModelOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        push_lp_bytes(out, &self.new_time_model.canonical_bytes());
        push_seq(out, &sorted_canonical(self.declared_incompatible.clone()));
        self.remapping.encode_canonical(out);
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

/// Set a user system-break preference (Chapter 6 §6.10 SetUserSystemBreak).
/// Carries the full [`TimeAnchor`] value (v1, value-typed); the reduction's LWW
/// bucketing key is the anchor's resolved [`MusicalPosition`]
/// ([`SetUserSystemBreakOp::resolved_position`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetUserSystemBreakOp {
    pub region: RegionId,
    pub anchor: TimeAnchor,
    pub present: bool,
}

impl SetUserSystemBreakOp {
    /// The anchor's resolved musical position — the canonical LWW bucketing key.
    /// A region-relative musical offset resolves to that offset; any other anchor
    /// shape resolves to the region origin (the break still applies to the
    /// region, the prototype LWW key is coarse — see `DECISIONS.md`).
    pub fn resolved_position(&self) -> MusicalPosition {
        match &self.anchor {
            TimeAnchor::Region {
                offset: epiphany_core::AnchorOffset::Musical(d),
                ..
            } => MusicalPosition(d.0.clone()),
            _ => MusicalPosition::origin(),
        }
    }
}

impl CanonicalEncode for SetUserSystemBreakOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        push_lp_bytes(out, &self.anchor.canonical_bytes());
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

// --- Group 1 (M2): event & pitch leaf-field ops (Chapter 6 §6.10). -----------

/// Overwrite a live event's value (Chapter 6 §6.10 ModifyEvent). Carries the
/// full replacement [`Event`] (v1, value-typed). Field-overwrite discipline:
/// later-in-canonical-order wins; concurrent differing modifications conflict.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ModifyEventOp {
    pub event: Event,
}

impl ModifyEventOp {
    /// The modified event's identifier (the LWW key).
    pub fn event_id(&self) -> EventId {
        self.event.id()
    }
}

impl CanonicalEncode for ModifyEventOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.event.canonical_bytes());
    }
}

/// Transpose live pitches by a chromatic interval (Chapter 6 §6.10 Transpose).
/// Pitch identifiers are preserved; reduction is order-dependent in the general
/// case (interval composition need not commute). `chromatic_steps` is a minimal
/// interval (a CMN alteration shift) that, in this prototype, commutes except at
/// the alteration's `i8` saturation bound; rich interval algebra is deferred
/// (Chapter 4 tuning catalog; P12-K2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TransposeOp {
    pub targets: Vec<PitchId>,
    pub chromatic_steps: i32,
}

impl CanonicalEncode for TransposeOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_seq(out, &sorted_canonical(self.targets.clone()));
        out.extend_from_slice(&self.chromatic_steps.to_le_bytes());
    }
}

/// Add a pitch to a live event (Chapter 6 §6.10 InsertIdentifiedPitch). Carries
/// the full [`IdentifiedPitch`] (v1). Mint discipline.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InsertIdentifiedPitchOp {
    pub event: EventId,
    pub pitch: IdentifiedPitch,
}

impl InsertIdentifiedPitchOp {
    /// The minted pitch's identifier.
    pub fn pitch_id(&self) -> PitchId {
        self.pitch.id
    }
}

impl CanonicalEncode for InsertIdentifiedPitchOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.event);
        push_lp_bytes(out, &self.pitch.canonical_bytes());
    }
}

/// Tombstone a live pitch (Chapter 6 §6.10 DeleteIdentifiedPitch). Delete-wins
/// discipline; the identifier is retained as a tombstone.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DeleteIdentifiedPitchOp {
    pub pitch: PitchId,
}

impl CanonicalEncode for DeleteIdentifiedPitchOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.pitch);
    }
}

/// Overwrite a live pitch's value (Chapter 6 §6.10 ModifyIdentifiedPitch).
/// Carries the full replacement [`Pitch`] (v1) — its acoustic / scale-position,
/// distinct from [`RespellPitchOp`] which overwrites only the *spelling*.
/// Field-overwrite discipline (later-in-canonical-order wins; concurrent
/// differing conflict).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ModifyIdentifiedPitchOp {
    pub pitch: PitchId,
    pub value: Pitch,
}

impl CanonicalEncode for ModifyIdentifiedPitchOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.pitch);
        push_lp_bytes(out, &self.value.canonical_bytes());
    }
}

// --- Group 2 (M2): cross-cutting CRUD (Chapter 6 §6.10). ---------------------

/// Tombstone a cross-cutting structure (Chapter 6 §6.10 DeleteCrossCutting).
/// Delete-wins discipline; the structure id is retained as a tombstone. The
/// structure is named by its [`TypedObjectId`] — the same key the set-union
/// creation and the re-anchoring table use — which must be a cross-cutting kind
/// (`Tie`/`Slur`/`Beam`/`Spanner`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DeleteCrossCuttingOp {
    pub structure: TypedObjectId,
}

impl CanonicalEncode for DeleteCrossCuttingOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.structure);
    }
}

/// Overwrite a cross-cutting structure's value (Chapter 6 §6.10
/// ModifyCrossCutting). Carries the full replacement [`CrossCuttingValue`] (v1,
/// value-typed); the field-overwrite discipline keys on the structure's
/// [`CrossCuttingValue::id`] (later-in-canonical-order wins; concurrent differing
/// conflict). The replacement keeps the structure's identity but may change its
/// endpoints and per-kind fields — the reduction re-derives its endpoints from
/// the new value (so a later re-anchoring sees them).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ModifyCrossCuttingOp {
    pub structure: CrossCuttingValue,
}

impl ModifyCrossCuttingOp {
    /// The structure's identity (the LWW key).
    pub fn id(&self) -> TypedObjectId {
        self.structure.id()
    }
}

impl CanonicalEncode for ModifyCrossCuttingOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        self.structure.encode_canonical(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{ReplicaId, SlurId};

    #[test]
    fn transaction_category_discriminants_are_golden() {
        // RATIFIED by Pass 11 (item 2.4, req:semops:transaction-category): the
        // declaration-order discriminants are normative and canonically encoded.
        assert_eq!(TransactionCategory::NoteEntry.discriminant(), 0);
        assert_eq!(TransactionCategory::Structural.discriminant(), 1);
        assert_eq!(TransactionCategory::Layout.discriminant(), 2);
        assert_eq!(TransactionCategory::Import.discriminant(), 3);
        assert_eq!(
            TransactionCategory::Registered(OperationKindRegistryId(0)).discriminant(),
            4
        );
    }

    #[test]
    fn kind_tag_projects_and_round_trips_registered_id() {
        let reg = OperationKindRegistryId(7);
        let k = OperationKind::Registered(reg, vec![1, 2, 3]);
        assert_eq!(k.tag(), OperationKindTag::Registered(reg));
        let prim = OperationKind::RespellPitch(RespellPitchOp {
            pitch: PitchId::new(ReplicaId(1), 1),
            spelling: crate::valuegen::spelling(4),
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
            OperationKindTag::InsertIdentifiedPitch,
            OperationKindTag::DeleteIdentifiedPitch,
            OperationKindTag::ModifyIdentifiedPitch,
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
        // A slur's (start, end) order is meaningful: swapping the endpoints
        // changes the canonical bytes.
        let s = SlurId::new(ReplicaId(1), 1);
        let a = EventId::new(ReplicaId(1), 2);
        let b = EventId::new(ReplicaId(1), 3);
        let fwd = CrossCuttingValue::Slur(crate::valuegen::slur(s, a, b));
        let rev = CrossCuttingValue::Slur(crate::valuegen::slur(s, b, a));
        assert_ne!(fwd.to_canonical_bytes(), rev.to_canonical_bytes());
    }
}
