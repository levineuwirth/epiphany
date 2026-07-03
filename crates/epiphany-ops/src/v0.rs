//! Frozen **v0** (identifier-only) operation payload shapes, retained solely as
//! the migration regression guard (Chapter 6; QUICKSTART Agent K, "v0 → v1
//! payload migration", migration *option 2*).
//!
//! Track B's Operation Catalog shifted the live [`OperationPayload`] from
//! carrying *identifiers + scalars + a content fingerprint* to carrying the real
//! **value-typed** graph payloads. Production code now speaks only v1; these v0
//! shapes never appear in a live envelope. They exist so the test corpus can
//! exercise the [`migrate_v0_envelope`](crate::migrate_v0_envelope) path and
//! prove it is *deterministic* and *equivalence-preserving*: a v0 envelope and
//! its v1 migration reduce to byte-identical canonical [`MaterializedState`].
//!
//! These are deliberately a **byte-for-byte snapshot** of the pre-catalog
//! payload structs. They carry no `CanonicalEncode`: the regression guard works
//! on v0 *values* (projected from v1, or built directly), never on a historical
//! v0 *wire* form — there is no production corpus of v0 bundle bytes to read
//! (the prototype never shipped), so the wire form is not part of the guard.

use epiphany_core::{
    EventId, MusicalDuration, MusicalPosition, PitchId, RegionId, StaffInstanceId, TransactionId,
    TupletId, TypedObjectId, VoiceId,
};
use epiphany_determinism::ContentHash;

use crate::payload::{ResolveConflictPayload, ResolveEquivocationPayload, TransactionDescriptor};
use crate::support::OperationKindRegistryId;
use crate::undo::UndoTransactionPayload;
use crate::OperationEnvelope;

/// Frozen v0 envelope: identical envelope wrapper, identifier-only payload.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct V0OperationEnvelope {
    pub id: epiphany_core::OperationId,
    pub author: crate::support::AuthorId,
    pub stamp: crate::OperationStamp,
    pub causal_context: crate::CausalContext,
    pub transaction: Option<TransactionId>,
    pub payload: V0OperationPayload,
}

/// Frozen v0 payload union (pre-catalog).
// Mirrors the live payload's size profile (the v1-native Group-1 kinds carry
// whole values); inline values are the design (see `payload::OperationKind`).
#[allow(clippy::large_enum_variant)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum V0OperationPayload {
    Primitive(V0OperationKind),
    /// Value-complete in v0 already; unchanged in v1.
    ResolveConflict(ResolveConflictPayload),
    /// Value-complete in v0 already; unchanged in v1.
    UndoTransaction(UndoTransactionPayload),
    /// v1-native (no identifier-only v0 predecessor existed — v0 predates the
    /// catalog's equivocation-resolution entry); carried verbatim so the
    /// migration round-trips it by identity, like the Group 1–4 kinds below.
    ResolveEquivocation(ResolveEquivocationPayload),
}

/// Frozen v0 primitive kinds (the representative §6.10 set).
#[allow(clippy::large_enum_variant)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum V0OperationKind {
    InsertEvent(V0InsertEventOp),
    DeleteEvent(V0DeleteEventOp),
    RespellPitch(V0RespellPitchOp),
    CreateCrossCutting(V0CreateCrossCuttingOp),
    ChangeRegionTimeModel(V0ChangeRegionTimeModelOp),
    SetUserSystemBreak(V0SetUserSystemBreakOp),
    /// Value-complete in v0 already; unchanged in v1.
    DeclareTransaction(TransactionDescriptor),
    /// Opaque extension payload; unchanged in v1.
    Registered(OperationKindRegistryId, Vec<u8>),
    // --- Group 1 (M2) kinds. These are v1-native: they had no identifier-only
    // v0 predecessor, so their "v0 form" carries the v1 payload verbatim and the
    // migration round-trips them by identity (only the original kinds above have
    // a lossy v0 projection to reconstruct). ---
    ModifyEvent(crate::payload::ModifyEventOp),
    Transpose(crate::payload::TransposeOp),
    InsertIdentifiedPitch(crate::payload::InsertIdentifiedPitchOp),
    DeleteIdentifiedPitch(crate::payload::DeleteIdentifiedPitchOp),
    ModifyIdentifiedPitch(crate::payload::ModifyIdentifiedPitchOp),
    // Group 2 (M2) — also v1-native; round-trip by identity.
    DeleteCrossCutting(crate::payload::DeleteCrossCuttingOp),
    ModifyCrossCutting(crate::payload::ModifyCrossCuttingOp),
    // Group 3 (M2c) — also v1-native; round-trip by identity.
    CreateRegion(crate::payload::CreateRegionOp),
    DeleteRegion(crate::payload::DeleteRegionOp),
    CreateStaffInstance(crate::payload::CreateStaffInstanceOp),
    DeleteStaffInstance(crate::payload::DeleteStaffInstanceOp),
    CreateVoice(crate::payload::CreateVoiceOp),
    DeleteVoice(crate::payload::DeleteVoiceOp),
    // Group 4 (M2d) — also v1-native; round-trip by identity.
    SetMetadata(crate::payload::SetMetadataOp),
    SetMetricGrid(crate::payload::SetMetricGridOp),
    SetUserPageBreak(crate::payload::SetUserPageBreakOp),
    // Phase-3 first tranche — also v1-native; round-trip by identity.
    CreateStaff(crate::payload::CreateStaffOp),
    SetTimeSignature(crate::payload::SetTimeSignatureOp),
    SetTempoSegment(crate::payload::SetTempoSegmentOp),
    SetStaffLayout(crate::payload::SetStaffLayoutOp),
}

/// v0 `InsertEvent`: the event was a bare [`EventId`] plus the reduction-relevant
/// scalars (position/duration/pitch ids), not the full `Event` value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct V0InsertEventOp {
    pub voice: VoiceId,
    pub staff_instance: StaffInstanceId,
    pub event: EventId,
    pub position: MusicalPosition,
    pub duration: MusicalDuration,
    pub pitches: Vec<PitchId>,
}

/// v0 `DeleteEvent`: identifier-keyed (delete needs only the id); the
/// replacement-rest value was deferred.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct V0DeleteEventOp {
    pub event: EventId,
    pub tuplet_compensation: V0TupletCompensation,
}

/// v0 tuplet compensation (the replacement rest was an id + duration, not a
/// `Rest` value).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum V0TupletCompensation {
    NotInTuplet,
    ReplaceWithRest {
        new_rest: EventId,
        duration: MusicalDuration,
    },
    RewriteTuplets {
        tuplets: Vec<TupletId>,
    },
    CascadeDeleteTuplets {
        tuplets: Vec<TupletId>,
    },
}

/// v0 `RespellPitch`: the new spelling was a [`ContentHash`] *fingerprint*, never
/// the `PitchSpelling` value (the reduction only needed spelling equality). This
/// is the irreversible case: a fingerprint cannot be inverted to a spelling
/// without a side table (see `migrate`, `MigrationError::Irreversible`, P12-K1).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct V0RespellPitchOp {
    pub pitch: PitchId,
    pub spelling: ContentHash,
}

/// v0 `CreateCrossCutting`: a reference-only view (id + endpoints), not the
/// typed structure value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct V0CreateCrossCuttingOp {
    pub id: TypedObjectId,
    pub endpoints: Vec<TypedObjectId>,
}

/// v0 `ChangeRegionTimeModel`: a discriminator tag, not the `RegionTimeModel`
/// value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct V0ChangeRegionTimeModelOp {
    pub region: RegionId,
    pub new_time_model: V0RegionTimeModelTag,
    pub declared_incompatible: Vec<EventId>,
    pub remapping: V0PositionRemapping,
}

/// v0 region-time-model discriminator.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum V0RegionTimeModelTag {
    Metric,
    Proportional,
    Aleatoric,
}

/// v0 position remapping (unchanged shape; copied so v0 is self-contained).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum V0PositionRemapping {
    PreserveTime,
    Reassign(Vec<(EventId, MusicalPosition)>),
}

/// v0 `SetUserSystemBreak`: the anchor was a resolved [`MusicalPosition`], not a
/// `TimeAnchor` value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct V0SetUserSystemBreakOp {
    pub region: RegionId,
    pub anchor: MusicalPosition,
    pub present: bool,
}

impl V0OperationEnvelope {
    /// The envelope wrapper carried over verbatim (identity, stamp, causal
    /// context, transaction); only the [`payload`](Self::payload) is migrated.
    pub(crate) fn rewrap(&self, payload: crate::OperationPayload) -> OperationEnvelope {
        OperationEnvelope {
            id: self.id,
            author: self.author,
            stamp: self.stamp,
            causal_context: self.causal_context.clone(),
            transaction: self.transaction,
            payload,
        }
    }
}
