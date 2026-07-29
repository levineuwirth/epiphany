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
    Beam, CanonicalValue, CanvasLayoutDefaults, Event, EventDuration, EventId, EventPosition,
    IdentifiedPitch, Instrument, InstrumentId, MetricGrid, MusicalDuration, MusicalPosition,
    OperationId, Pitch, PitchId, PitchSpelling, Region, RegionId, RegionTimeModel, RepeatStructure,
    RepeatStructureId, Rest, ScoreMetadata, Slur, Spanner, SpellingPrecedence, Staff, StaffId,
    StaffInstance, StaffInstanceId, StaffLineConfiguration, TempoSegment, Tie, TimeAnchor,
    TimeSignature, TransactionId, TranspositionInterval, TuningContextSettings, TupletId,
    TypedObjectId, Voice, VoiceId,
};
use epiphany_determinism::{
    sorted_canonical, CanonicalDecode, CanonicalEncode, CanonicalSet, DecodeError,
};

use crate::conflict::{ConflictId, ResolutionAction};
use crate::encode::{push_canon, push_lp_bytes, push_seq, push_str, push_tag, push_u8_bool};
use crate::envelope::{EnvelopeHash, OperationEnvelope};
use crate::support::OperationKindRegistryId;
use crate::undo::UndoTransactionPayload;

/// The full payload of an operation envelope: a primitive, or one of the
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
    /// A meta-operation that resolves an equivocated operation slot by naming
    /// the chosen candidate envelope (operation_catalog
    /// §"ResolveEquivocation").
    ResolveEquivocation(ResolveEquivocationPayload),
}

impl OperationPayload {
    fn discriminant(&self) -> u8 {
        match self {
            OperationPayload::Primitive(_) => 0,
            OperationPayload::ResolveConflict(_) => 1,
            OperationPayload::UndoTransaction(_) => 2,
            // Appended (ResolveEquivocation); the ratified 0..=2 stay stable.
            OperationPayload::ResolveEquivocation(_) => 3,
        }
    }

    /// The binary-format schema major this payload's canonical encoding requires
    /// ([`OperationKind::schema_major`]). The meta-operations embed no
    /// data-model value, so they are major 0.
    pub fn schema_major(&self) -> u16 {
        match self {
            OperationPayload::Primitive(kind) => kind.schema_major(),
            OperationPayload::ResolveConflict(_)
            | OperationPayload::UndoTransaction(_)
            | OperationPayload::ResolveEquivocation(_) => 0,
        }
    }

    /// The G-minor schema-minor epoch this payload's own **outer**
    /// discriminant requires (`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4, pin 1),
    /// or `None` for "no additive requirement" — the distinct sentinel pin 3
    /// requires, never `0`. This is the outer `OperationPayload` variant
    /// only; a `Primitive`'s nested `OperationKind` epoch is a separate
    /// contribution an envelope's derivation combines with this one (see
    /// `OperationEnvelope::introduced_minor`, below).
    pub fn introduced_minor(&self) -> Option<u16> {
        match self {
            OperationPayload::Primitive(_)
            | OperationPayload::ResolveConflict(_)
            | OperationPayload::UndoTransaction(_) => None,
            // Minor 3 (Push 3).
            OperationPayload::ResolveEquivocation(_) => Some(3),
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
            OperationPayload::ResolveEquivocation(p) => p.encode_canonical(out),
        }
    }
}

impl OperationEnvelope {
    /// The G-minor schema-minor epoch this envelope's canonical bytes require
    /// (`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4, pin 3): the maximum over its
    /// outer [`OperationPayload::introduced_minor`] and — when the payload is
    /// [`OperationPayload::Primitive`] — the nested
    /// [`OperationKind::introduced_minor`]. `None` when neither imposes a
    /// requirement (both are baseline). No other nested vocabulary an
    /// envelope's encoder reaches (`OperationEnvelope::encode_canonical`)
    /// carries a post-baseline append: the embedded graph values ride
    /// `epiphany-core`'s codec, which the G-minor audit found clean
    /// (`spec/AUDIT_GMINOR_VOCABULARIES.md` Part 2).
    pub fn introduced_minor(&self) -> Option<u16> {
        let payload_epoch = self.payload.introduced_minor();
        let kind_epoch = match &self.payload {
            OperationPayload::Primitive(kind) => kind.introduced_minor(),
            OperationPayload::ResolveConflict(_)
            | OperationPayload::UndoTransaction(_)
            | OperationPayload::ResolveEquivocation(_) => None,
        };
        payload_epoch.into_iter().chain(kind_epoch).max()
    }
}

/// An operation-envelope block's required G-minor schema-minor epoch (pin 3):
/// the maximum [`OperationEnvelope::introduced_minor`] over every envelope it
/// carries, or `None` if the block carries no envelope or every envelope is
/// baseline. Callers combine this with the block's schema **major** (an
/// independent derivation, `OperationEnvelope::schema_major`) via
/// `epiphany_bundle::SchemaVersion::for_major_at_epoch` — major and minor
/// never influence each other (pin 3).
pub fn operation_block_introduced_minor(envelopes: &[OperationEnvelope]) -> Option<u16> {
    envelopes.iter().filter_map(|e| e.introduced_minor()).max()
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
    // --- Group 3 (M2c): structural container CRUD. Mint + empty-only delete. ---
    /// Mint an empty region into the canvas.
    CreateRegion(CreateRegionOp),
    /// Tombstone an empty region (delete-wins; precondition: no live instances).
    DeleteRegion(DeleteRegionOp),
    /// Mint an empty staff instance into a live region.
    CreateStaffInstance(CreateStaffInstanceOp),
    /// Tombstone an empty staff instance (precondition: no live voices).
    DeleteStaffInstance(DeleteStaffInstanceOp),
    /// Mint an empty voice into a live staff instance.
    CreateVoice(CreateVoiceOp),
    /// Tombstone an empty voice (precondition: no live events).
    DeleteVoice(DeleteVoiceOp),
    // --- Group 4 (M2d): score settings. LWW field-overwrite. ---
    /// Overwrite the score metadata (later-in-canonical-order wins).
    SetMetadata(SetMetadataOp),
    /// Overwrite a region's default metric grid (later-in-canonical-order wins).
    SetMetricGrid(SetMetricGridOp),
    /// Set a user page-break preference (LWW advisory; the page-break sibling of
    /// `SetUserSystemBreak`).
    SetUserPageBreak(SetUserPageBreakOp),
    // --- Phase-3 first tranche: staff mint, meter/tempo overwrites, layout
    // advisory (operation_catalog §CreateStaff, §"Meter and Tempo Overwrites",
    // §SetStaffLayout). Discriminants extend additively past 23. ---
    /// Mint a global staff on the score root (set-union creation).
    CreateStaff(CreateStaffOp),
    /// Set, replace, or remove the single meter change at a resolved position
    /// in a region's default metric grid (LWW structural overwrite).
    SetTimeSignature(SetTimeSignatureOp),
    /// Set, replace, or remove the single tempo segment starting at a resolved
    /// position in the score-level or region-local tempo map (LWW structural
    /// overwrite).
    SetTempoSegment(SetTempoSegmentOp),
    /// Overwrite a staff instance's inline layout advisories as a unit (LWW
    /// advisory).
    SetStaffLayout(SetStaffLayoutOp),
    // --- Schema-major-2 revision: repeat authoring (operation_catalog
    // §"Repeat Structures"). Discriminants extend additively past 27. ---
    /// Mint a repeat structure into the cross-cutting registry (set-union
    /// creation; every event-referencing anchor site must resolve live).
    CreateRepeatStructure(CreateRepeatStructureOp),
    /// Tombstone a repeat structure (delete-wins, idempotent on re-delete).
    DeleteRepeatStructure(DeleteRepeatStructureOp),
    /// Push 4a: the faithful transpose. Appended past 29 — a schema-minor
    /// vocabulary append (`req:binfmt:kind-discriminants`).
    TransposeInterval(TransposeIntervalOp),
    // --- Genesis tranche G1 (`spec/CONTRACT_GENESIS_G1_INSTRUMENT.md`): the
    // from-empty spine's missing link. Discriminant extends additively past 30.
    // ---
    /// Mint an abstract instrument on the score root (set-union creation) —
    /// the single missing link between `Score::empty` and a note:
    /// `CreateStaff` already demands a live `Instrument` and nothing else can
    /// create one.
    CreateInstrument(CreateInstrumentOp),
    // --- Genesis tranche G2a (`spec/CONTRACT_GENESIS_G2A_SETTINGS.md`): the
    // two major-0 settings setters. Discriminants extend additively past 31.
    // ---
    /// Overwrite the canvas's default layout advisories (later-in-canonical-
    /// order wins).
    SetCanvasLayoutDefaults(SetCanvasLayoutDefaultsOp),
    /// Overwrite the score's spelling precedence (later-in-canonical-order
    /// wins).
    SetSpellingPrecedence(SetSpellingPrecedenceOp),
    // --- Genesis tranche G2b (`spec/CONTRACT_GENESIS_G2B_TUNING.md`): the
    // sole genesis payload born at schema major 3. Discriminant extends
    // additively past 33.
    // ---
    /// Overwrite the score's tuning context settings (later-in-canonical-
    /// order wins). Carries [`TuningContextSettings`], the authored subset of
    /// `ScoreTuningContext` — `accidental_extensions` is not on the wire and
    /// is left untouched by reduction (contract §1).
    SetTuningContext(SetTuningContextOp),
}

impl OperationKind {
    /// The binary-format schema major this kind's canonical payload encodes at
    /// (Binary Format companion §"Schema Major 1" / §"Schema Major 2").
    ///
    /// An op-envelope block's schema major is the maximum over the operations
    /// it carries. **Minimal stamping** (Binary Format §Schema Major 2): the
    /// stamp is the *lowest* major whose layouts decode the payload's bytes —
    /// a pure function of the value, so identical content stamps (and hashes)
    /// identically. The kinds whose v2 fills are mandatory appended fields
    /// are always major 2; the kinds whose only v2 embedding hides behind an
    /// `Option` (encoding byte-identically to the prior major when `None`)
    /// are value-dependent.
    pub fn schema_major(&self) -> u16 {
        match self {
            // Mandatory v2 appends: CrossCuttingValue (Slur/Tie/Beam/Spanner
            // bodies), Staff (default_clef + filled line config),
            // ScoreMetadata (six appended fields), and Instrument
            // (sound_config/default_clef/default_staff_lines et al. — G1;
            // unconditional, not `Option`-hidden, so no lower-major layout for
            // this payload exists — do not copy the value-dependent arm shape
            // below).
            OperationKind::CreateCrossCutting(_)
            | OperationKind::ModifyCrossCutting(_)
            | OperationKind::CreateStaff(_)
            | OperationKind::SetMetadata(_)
            | OperationKind::CreateInstrument(_) => 2,
            // Genesis tranche G2b (contract pin 2): unconditionally 3.
            // `ScoreTuningContext`'s `smufl`/`overrides` appends are
            // mandatory, not `Option`-hidden, so there is no lower-major
            // layout for this payload — a `CreateInstrument`-shaped arm, not
            // a `CreateRegion`-shaped one. The sole surface among the nine
            // genesis settings/creates that raises the op-block accept-set.
            OperationKind::SetTuningContext(_) => 3,
            // Value-dependent: the embedded StaffLineConfiguration rides an
            // Option; None encodes byte-identically to the prior major.
            OperationKind::CreateRegion(op) => {
                if op
                    .region
                    .content
                    .staff_instances()
                    .iter()
                    .any(|si| si.staff_lines_override.is_some())
                {
                    2
                } else {
                    1
                }
            }
            OperationKind::CreateStaffInstance(op)
                if op.instance.staff_lines_override.is_some() =>
            {
                2
            }
            OperationKind::SetStaffLayout(op) if op.staff_lines_override.is_some() => 2,
            // Born at v2: the carried RepeatStructure's v2 fields are
            // unconditional (`kind`/`voltas` are not `Option`s), so no
            // lower-major layout for this payload exists. The DELETE sibling
            // carries a bare identifier — a major-0 layout under a minor
            // kind append (the Phase-3 precedent) — and stays in the
            // catch-all 0 arm below.
            OperationKind::CreateRepeatStructure(_) => 2,
            _ => 0,
        }
    }

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
            OperationKind::CreateRegion(_) => 15,
            OperationKind::DeleteRegion(_) => 16,
            OperationKind::CreateStaffInstance(_) => 17,
            OperationKind::DeleteStaffInstance(_) => 18,
            OperationKind::CreateVoice(_) => 19,
            OperationKind::DeleteVoice(_) => 20,
            OperationKind::SetMetadata(_) => 21,
            OperationKind::SetMetricGrid(_) => 22,
            OperationKind::SetUserPageBreak(_) => 23,
            // Phase-3 first tranche; appended past the golden-locked 0..=23.
            OperationKind::CreateStaff(_) => 24,
            OperationKind::SetTimeSignature(_) => 25,
            OperationKind::SetTempoSegment(_) => 26,
            OperationKind::SetStaffLayout(_) => 27,
            // Schema-major-2 revision; appended past the Phase-3 24..=27.
            OperationKind::CreateRepeatStructure(_) => 28,
            OperationKind::DeleteRepeatStructure(_) => 29,
            // Push 4a; appended past 29. Every constituent is a major-0
            // layout, so `schema_major` leaves it in the catch-all 0 arm.
            OperationKind::TransposeInterval(_) => 30,
            // Genesis tranche G1; appended past 30. Coincides with tag 31 by
            // accident, not by rule — the two spaces are independent and
            // misaligned elsewhere (`RespellPitch` is kind 2 / tag 3).
            OperationKind::CreateInstrument(_) => 31,
            // Genesis tranche G2a; appended past 31. Coincide with tags 32/33
            // by the same accident as above, not by rule.
            OperationKind::SetCanvasLayoutDefaults(_) => 32,
            OperationKind::SetSpellingPrecedence(_) => 33,
            // Genesis tranche G2b; appended past 33. Coincides with tag 34 by
            // the same accident as G1/G2a, not by rule.
            OperationKind::SetTuningContext(_) => 34,
        }
    }

    /// The G-minor schema-minor epoch this kind's own discriminant requires
    /// (`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4, pin 1's ratified table), or
    /// `None` for a baseline (golden-locked 0..=23) variant — the distinct
    /// "no additive requirement" sentinel pin 3 requires, never `0`.
    ///
    /// **Deliberately a separate match from [`Self::discriminant`]** (pin 2):
    /// that hand-written match is the site Push 4a's `TransposeInterval`
    /// append went wrong by being consulted for only one of the vocabulary's
    /// several responsibilities. Exhaustive here too, with no wildcard arm,
    /// so a future variant cannot compile without an epoch assignment.
    pub fn introduced_minor(&self) -> Option<u16> {
        match self {
            OperationKind::InsertEvent(_)
            | OperationKind::DeleteEvent(_)
            | OperationKind::RespellPitch(_)
            | OperationKind::CreateCrossCutting(_)
            | OperationKind::ChangeRegionTimeModel(_)
            | OperationKind::SetUserSystemBreak(_)
            | OperationKind::DeclareTransaction(_)
            | OperationKind::Registered(..)
            | OperationKind::ModifyEvent(_)
            | OperationKind::Transpose(_)
            | OperationKind::InsertIdentifiedPitch(_)
            | OperationKind::DeleteIdentifiedPitch(_)
            | OperationKind::ModifyIdentifiedPitch(_)
            | OperationKind::DeleteCrossCutting(_)
            | OperationKind::ModifyCrossCutting(_)
            | OperationKind::CreateRegion(_)
            | OperationKind::DeleteRegion(_)
            | OperationKind::CreateStaffInstance(_)
            | OperationKind::DeleteStaffInstance(_)
            | OperationKind::CreateVoice(_)
            | OperationKind::DeleteVoice(_)
            | OperationKind::SetMetadata(_)
            | OperationKind::SetMetricGrid(_)
            | OperationKind::SetUserPageBreak(_) => None,
            // Minor 4 (Phase-3 first tranche).
            OperationKind::CreateStaff(_)
            | OperationKind::SetTimeSignature(_)
            | OperationKind::SetTempoSegment(_)
            | OperationKind::SetStaffLayout(_) => Some(4),
            // Minor 6 (schema-major-2 repeat-authoring revision).
            OperationKind::CreateRepeatStructure(_) | OperationKind::DeleteRepeatStructure(_) => {
                Some(6)
            }
            // Minor 7 (Push 4a).
            OperationKind::TransposeInterval(_) => Some(7),
            // Minor 8 (Genesis tranche G1).
            OperationKind::CreateInstrument(_) => Some(8),
            // Minor 9 (Genesis tranche G2a).
            OperationKind::SetCanvasLayoutDefaults(_) | OperationKind::SetSpellingPrecedence(_) => {
                Some(9)
            }
            // Minor 10 (Genesis tranche G2b), ratified
            // `spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4.
            OperationKind::SetTuningContext(_) => Some(10),
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
            OperationKind::CreateRegion(_) => OperationKindTag::InsertRegion,
            OperationKind::DeleteRegion(_) => OperationKindTag::DeleteRegion,
            OperationKind::CreateStaffInstance(_) => OperationKindTag::InsertStaffInstance,
            OperationKind::DeleteStaffInstance(_) => OperationKindTag::DeleteStaffInstance,
            OperationKind::CreateVoice(_) => OperationKindTag::CreateVoice,
            OperationKind::DeleteVoice(_) => OperationKindTag::DeleteVoice,
            OperationKind::SetMetadata(_) => OperationKindTag::SetMetadata,
            OperationKind::SetMetricGrid(_) => OperationKindTag::SetMetricGrid,
            OperationKind::SetUserPageBreak(_) => OperationKindTag::SetUserPageBreak,
            // Create→Insert tag-naming convention, cf. `InsertRegion`.
            OperationKind::CreateStaff(_) => OperationKindTag::InsertStaff,
            OperationKind::SetTimeSignature(_) => OperationKindTag::SetTimeSignature,
            OperationKind::SetTempoSegment(_) => OperationKindTag::SetTempoSegment,
            OperationKind::SetStaffLayout(_) => OperationKindTag::SetStaffLayout,
            // Name-verbatim projection, as the cross-cutting tags do.
            OperationKind::CreateRepeatStructure(_) => OperationKindTag::CreateRepeatStructure,
            OperationKind::DeleteRepeatStructure(_) => OperationKindTag::DeleteRepeatStructure,
            OperationKind::TransposeInterval(_) => OperationKindTag::TransposeInterval,
            // Name-verbatim, as the two most recent additions (`CreateVoice`,
            // `CreateRepeatStructure`) are — the tag layer's older
            // Create→Insert convention is not followed here (contract pin 3).
            OperationKind::CreateInstrument(_) => OperationKindTag::CreateInstrument,
            OperationKind::SetCanvasLayoutDefaults(_) => OperationKindTag::SetCanvasLayoutDefaults,
            OperationKind::SetSpellingPrecedence(_) => OperationKindTag::SetSpellingPrecedence,
            OperationKind::SetTuningContext(_) => OperationKindTag::SetTuningContext,
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
            OperationKind::TransposeInterval(op) => op.encode_canonical(out),
            OperationKind::InsertIdentifiedPitch(op) => op.encode_canonical(out),
            OperationKind::DeleteIdentifiedPitch(op) => op.encode_canonical(out),
            OperationKind::ModifyIdentifiedPitch(op) => op.encode_canonical(out),
            OperationKind::DeleteCrossCutting(op) => op.encode_canonical(out),
            OperationKind::ModifyCrossCutting(op) => op.encode_canonical(out),
            OperationKind::CreateRegion(op) => op.encode_canonical(out),
            OperationKind::DeleteRegion(op) => op.encode_canonical(out),
            OperationKind::CreateStaffInstance(op) => op.encode_canonical(out),
            OperationKind::DeleteStaffInstance(op) => op.encode_canonical(out),
            OperationKind::CreateVoice(op) => op.encode_canonical(out),
            OperationKind::DeleteVoice(op) => op.encode_canonical(out),
            OperationKind::SetMetadata(op) => op.encode_canonical(out),
            OperationKind::SetMetricGrid(op) => op.encode_canonical(out),
            OperationKind::SetUserPageBreak(op) => op.encode_canonical(out),
            OperationKind::CreateStaff(op) => op.encode_canonical(out),
            OperationKind::SetTimeSignature(op) => op.encode_canonical(out),
            OperationKind::SetTempoSegment(op) => op.encode_canonical(out),
            OperationKind::SetStaffLayout(op) => op.encode_canonical(out),
            OperationKind::CreateRepeatStructure(op) => op.encode_canonical(out),
            OperationKind::DeleteRepeatStructure(op) => op.encode_canonical(out),
            OperationKind::CreateInstrument(op) => op.encode_canonical(out),
            OperationKind::SetCanvasLayoutDefaults(op) => op.encode_canonical(out),
            OperationKind::SetSpellingPrecedence(op) => op.encode_canonical(out),
            OperationKind::SetTuningContext(op) => op.encode_canonical(out),
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
    CreateVoice,
    DeleteVoice,
    SetMetadata,
    SetMetricGrid,
    // Phase-3 first tranche. `InsertStaff` follows the tag layer's
    // Create→Insert naming convention (cf. `InsertRegion` for `CreateRegion`).
    InsertStaff,
    SetTimeSignature,
    SetTempoSegment,
    SetStaffLayout,
    // Schema-major-2 revision (repeat authoring). Name-verbatim, as the
    // cross-cutting tags are.
    CreateRepeatStructure,
    DeleteRepeatStructure,
    /// Push 4a.
    TransposeInterval,
    /// Genesis tranche G1. Name-verbatim (contract pin 3): the tag layer's
    /// older Create→Insert convention (`InsertStaff` for `CreateStaff`) is not
    /// followed here, matching the two most recent additions.
    CreateInstrument,
    /// Genesis tranche G2a.
    SetCanvasLayoutDefaults,
    /// Genesis tranche G2a.
    SetSpellingPrecedence,
    /// Genesis tranche G2b.
    SetTuningContext,
}

/// The discriminant of [`OperationKindTag::Registered`], the one tag that
/// carries a payload. It sits inside the payload-free range, so it is named
/// rather than derived.
pub const REGISTERED_TAG_DISCRIMINANT: u8 = 16;

/// The single source of truth for the payload-free tag vocabulary.
///
/// Generates the discriminant mapping, its inverse, and
/// [`OperationKindTag::PAYLOAD_FREE`] from one list. The generated
/// `discriminant` match is **exhaustive over the enum**, so a variant added to
/// `OperationKindTag` without being added here fails to compile — and because
/// the decoder, the fuzz corpus, the conformance vectors, and the edit-barrier
/// round-trip all read `PAYLOAD_FREE`, adding it here reaches every one of them.
///
/// This exists because it did not. Push 4a added `TransposeInterval` to a
/// hand-written `discriminant` match and to nothing else: the decoder rejected
/// its own encoding, an edit barrier naming it could not be reopened, and four
/// separate hand-maintained lists — two of them asserting the tag was *unknown*
/// — stayed green (Push 5 / P4).
macro_rules! operation_kind_tag_vocabulary {
    ($($variant:ident = $disc:literal => $catalog:literal @ $epoch:expr),+ $(,)?) => {
        impl OperationKindTag {
            /// Every payload-free tag, in discriminant order. [`Registered`]
            /// is excluded: it carries an id and has no bare encoding.
            ///
            /// [`Registered`]: OperationKindTag::Registered
            pub const PAYLOAD_FREE: &'static [OperationKindTag] =
                &[$(OperationKindTag::$variant),+];

            /// The tag's one-byte wire discriminant
            /// (`req:binfmt:kind-tag`; append-only).
            pub fn discriminant(&self) -> u8 {
                match self {
                    $(OperationKindTag::$variant => $disc,)+
                    OperationKindTag::Registered(_) => REGISTERED_TAG_DISCRIMINANT,
                }
            }

            /// The payload-free tag for `discriminant`, or `None` if it names
            /// no tag. `Registered` is never returned: it is decoded separately,
            /// with its id.
            fn from_discriminant(discriminant: u8) -> Option<OperationKindTag> {
                Some(match discriminant {
                    $($disc => OperationKindTag::$variant,)+
                    _ => return None,
                })
            }

            /// The name this kind carries in the Text Projection, which is the
            /// **Operation Catalog's** section name, not this enum's variant
            /// name. The tag space renamed three pairs (`InsertRegion`,
            /// `InsertStaff`, `InsertStaffInstance`); the projection keeps the
            /// catalog's `create-*`, because that is what the specification a
            /// reader holds calls them.
            pub fn catalog_name(&self) -> &'static str {
                match self {
                    $(OperationKindTag::$variant => $catalog,)+
                    OperationKindTag::Registered(_) => "registered",
                }
            }

            /// The G-minor schema-minor epoch this tag requires
            /// (`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4, pin 1), or `None` for
            /// "no additive requirement" — never `0`, a real baseline minor
            /// for `V1`–`V3` (pin 3). **Co-located inside this macro** (pin
            /// 2), not a sibling match: the macro is the compile-enforced
            /// source of truth for this vocabulary, exhaustive over every
            /// generated arm with no wildcard, including `Registered`.
            pub fn introduced_minor(&self) -> Option<u16> {
                match self {
                    $(OperationKindTag::$variant => $epoch,)+
                    OperationKindTag::Registered(_) => None,
                }
            }
        }
    };
}

operation_kind_tag_vocabulary! {
    InsertEvent = 0 => "insert-event" @ None,
    DeleteEvent = 1 => "delete-event" @ None,
    ModifyEvent = 2 => "modify-event" @ None,
    RespellPitch = 3 => "respell-pitch" @ None,
    Transpose = 4 => "transpose" @ None,
    CreateCrossCutting = 5 => "create-cross-cutting" @ None,
    DeleteCrossCutting = 6 => "delete-cross-cutting" @ None,
    ModifyCrossCutting = 7 => "modify-cross-cutting" @ None,
    ChangeRegionTimeModel = 8 => "change-region-time-model" @ None,
    InsertRegion = 9 => "create-region" @ None,
    DeleteRegion = 10 => "delete-region" @ None,
    InsertStaffInstance = 11 => "create-staff-instance" @ None,
    DeleteStaffInstance = 12 => "delete-staff-instance" @ None,
    SetUserSystemBreak = 13 => "set-user-system-break" @ None,
    SetUserPageBreak = 14 => "set-user-page-break" @ None,
    DeclareTransaction = 15 => "declare-transaction" @ None,
    InsertIdentifiedPitch = 17 => "insert-identified-pitch" @ None,
    DeleteIdentifiedPitch = 18 => "delete-identified-pitch" @ None,
    ModifyIdentifiedPitch = 19 => "modify-identified-pitch" @ None,
    CreateVoice = 20 => "create-voice" @ None,
    DeleteVoice = 21 => "delete-voice" @ None,
    SetMetadata = 22 => "set-metadata" @ None,
    SetMetricGrid = 23 => "set-metric-grid" @ None,
    InsertStaff = 24 => "create-staff" @ Some(4),
    SetTimeSignature = 25 => "set-time-signature" @ Some(4),
    SetTempoSegment = 26 => "set-tempo-segment" @ Some(4),
    SetStaffLayout = 27 => "set-staff-layout" @ Some(4),
    CreateRepeatStructure = 28 => "create-repeat-structure" @ Some(6),
    DeleteRepeatStructure = 29 => "delete-repeat-structure" @ Some(6),
    TransposeInterval = 30 => "transpose-interval" @ Some(7),
    CreateInstrument = 31 => "create-instrument" @ Some(8),
    SetCanvasLayoutDefaults = 32 => "set-canvas-layout-defaults" @ Some(9),
    SetSpellingPrecedence = 33 => "set-spelling-precedence" @ Some(9),
    SetTuningContext = 34 => "set-tuning-context" @ Some(10),
}

impl CanonicalEncode for OperationKindTag {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        if let OperationKindTag::Registered(id) = self {
            push_canon(out, id);
        }
    }
}

impl CanonicalDecode for OperationKindTag {
    /// Decodes exactly the canonical form [`CanonicalEncode`] produces: the
    /// discriminant byte, plus — for [`OperationKindTag::Registered`] only —
    /// the registry id's 16 big-endian bytes. Variable-width, so the input
    /// length must match the decoded variant exactly (trailing bytes are an
    /// error). An unknown discriminant is rejected, never normalized —
    /// [`DecodeError::MalformedDomainTag`], the same unknown-discriminant
    /// rejection [`TypedObjectId::decode_canonical`] uses.
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (&tag, rest) = bytes.split_first().ok_or(DecodeError::UnexpectedLength {
            expected: 1,
            actual: 0,
        })?;
        if tag == REGISTERED_TAG_DISCRIMINANT {
            let arr: [u8; 16] = rest.try_into().map_err(|_| DecodeError::UnexpectedLength {
                expected: 17,
                actual: bytes.len(),
            })?;
            return Ok(OperationKindTag::Registered(OperationKindRegistryId(
                u128::from_be_bytes(arr),
            )));
        }
        if !rest.is_empty() {
            return Err(DecodeError::UnexpectedLength {
                expected: 1,
                actual: bytes.len(),
            });
        }
        // The mapping is generated from the same list as `discriminant`
        // (`operation_kind_tag_vocabulary!`), so encode and decode cannot drift.
        OperationKindTag::from_discriminant(tag).ok_or(DecodeError::MalformedDomainTag)
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

    /// Every object this structure's anchors reference — its endpoint events,
    /// and (for a spanner) the measures and regions its [`TimeAnchor`]s name.
    /// The mint precondition checks all of these are live, so a spanner
    /// anchored to a missing region or measure is rejected rather than minted
    /// dangling (P13-D3). Distinct from [`Self::endpoints`], which stays
    /// event-only: it feeds the re-anchoring referent index, which repairs
    /// event tombstones only (the spanner discipline — non-event referent
    /// re-anchoring stays deferred, ratified events-only).
    pub fn anchor_object_refs(&self) -> Vec<TypedObjectId> {
        match self {
            // Event-anchored kinds reference exactly their endpoint events.
            CrossCuttingValue::Tie(_) | CrossCuttingValue::Slur(_) | CrossCuttingValue::Beam(_) => {
                self.endpoints()
            }
            CrossCuttingValue::Spanner(s) => [&s.start, &s.end]
                .into_iter()
                .filter_map(|anchor| match anchor {
                    TimeAnchor::Event { id, .. } => Some(TypedObjectId::Event(*id)),
                    TimeAnchor::Measure { id, .. } => Some(TypedObjectId::Measure(*id)),
                    TimeAnchor::Region { id, .. } => Some(TypedObjectId::Region(*id)),
                    TimeAnchor::WallClock { .. } => None,
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

/// The musical position a [`TimeAnchor`] resolves to for user-break LWW
/// bucketing. A region-relative musical offset resolves to that offset; any
/// other anchor shape resolves to the region origin (the break still applies to
/// the region, the prototype LWW key is coarse — see `DECISIONS.md`). The graph
/// reducer keys break anchors by this same position, so two anchors resolving
/// here to one position occupy a single LWW slot.
pub(crate) fn resolved_anchor_position(anchor: &TimeAnchor) -> MusicalPosition {
    match anchor {
        TimeAnchor::Region {
            offset: epiphany_core::AnchorOffset::Musical(d),
            ..
        } => MusicalPosition(d.0.clone()),
        _ => MusicalPosition::origin(),
    }
}

impl SetUserSystemBreakOp {
    /// The anchor's resolved musical position — the canonical LWW bucketing key
    /// (see the private `resolved_anchor_position` helper).
    pub fn resolved_position(&self) -> MusicalPosition {
        resolved_anchor_position(&self.anchor)
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

/// The payload of an equivocation-resolution meta-operation (operation_catalog
/// §"ResolveEquivocation"): the equivocated slot and the candidate envelope (by
/// canonical-bytes hash) that shall stand. Value-complete.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ResolveEquivocationPayload {
    /// The `OperationId` of the equivocated slot to resolve.
    pub target: OperationId,
    /// The [`EnvelopeHash`] of the candidate envelope that shall stand.
    pub chosen: EnvelopeHash,
}

impl CanonicalEncode for ResolveEquivocationPayload {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        // Catalog: `target` (16 canonical bytes), then `chosen` (32 bytes).
        push_canon(out, &self.target);
        push_canon(out, &self.chosen);
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

/// **Frozen; replay only** (operation_catalog §Transpose,
/// `req:opcat:transpose-frozen`). New authoring emits [`TransposeIntervalOp`].
///
/// Shifts each live target's CMN `alteration` by `chromatic_steps`. Its
/// semantics are pinned exactly as they shipped, because an operation is
/// history: a replica replaying a stored `Transpose` must reconstruct the state
/// its author saw, and a "corrected" reduction rule would silently rewrite
/// every score that used one. The pinned defects, all normative for replay:
///
/// * `targets` is a **multiset** — a target named twice is shifted twice.
/// * The nominal and octave never move: `C4` by `+12` becomes `C4` with
///   `alteration: 12` (six double-sharps), not `C5`.
/// * A shift past the `i8` bound **saturates silently** and still reports
///   `Applied`, so the operation is not invertible.
/// * A non-`Cmn` position is left unchanged and still reports `Applied`.
///
/// [`TransposeIntervalOp`] repairs every one of these under a new discriminant
/// that no historical operation carries (Push 4a, closing P12-K2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TransposeOp {
    pub targets: Vec<PitchId>,
    pub chromatic_steps: i32,
}

impl CanonicalEncode for TransposeOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        // `sorted_canonical`, NOT a set: the multiset is frozen wire form.
        push_seq(out, &sorted_canonical(self.targets.clone()));
        out.extend_from_slice(&self.chromatic_steps.to_le_bytes());
    }
}

/// Transpose live pitches by a [`TranspositionInterval`] (operation_catalog
/// §TransposeInterval; Push 4a). The faithful replacement for the frozen
/// [`TransposeOp`].
///
/// `targets` is a genuine **set** at the type level — [`CanonicalSet`] is a
/// `BTreeSet`, and `PitchId`'s `Ord` *is* its canonical byte order, so it
/// iterates in canonical order and cannot hold a duplicate. A transposition is
/// a function of *which* pitches it names, never of how many times.
///
/// Reduction refuses atomically rather than saturating: see
/// [`Pitch::transposed`](epiphany_core::Pitch::transposed) for the algebra and
/// the three refusal cases.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TransposeIntervalOp {
    pub targets: CanonicalSet<PitchId>,
    pub interval: TranspositionInterval,
}

impl CanonicalEncode for TransposeIntervalOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        // A BTreeSet already iterates in canonical order (PitchId: Ord is its
        // canonical byte order) and cannot repeat — the `seq^⇑` of the wire
        // table, by construction rather than by a call someone can forget.
        let targets: Vec<PitchId> = self.targets.iter().copied().collect();
        push_seq(out, &targets);
        out.extend_from_slice(&self.interval.diatonic_steps.to_le_bytes());
        out.extend_from_slice(&self.interval.chromatic_steps.to_le_bytes());
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

// --- Group 3 (M2c): structural container CRUD (Chapter 6 §6.10). -------------
//
// Creates are value-typed mints of an *empty* container (set-union creation);
// deletes are empty-only delete-wins tombstones (the container must have no live
// children — the caller deletes contents first). See `DECISIONS.md`.

/// Mint an empty region into the canvas (Chapter 6 §6.10 InsertRegion). Carries
/// the full [`Region`] value (`permits_spanning_slurs` since schema major 1);
/// the reduction preconditions it carries no staff instances (an empty
/// container). Minimal stamping ([`OperationKind::schema_major`]): the payload
/// is major 1, or major 2 iff a carried staff instance bears
/// `Some(staff_lines_override)` (wire-representable though reduction refuses
/// non-empty regions).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateRegionOp {
    pub region: Region,
}

impl CreateRegionOp {
    /// The minted region's identifier.
    pub fn region_id(&self) -> RegionId {
        self.region.id
    }
}

impl CanonicalEncode for CreateRegionOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        // Schema major 1: the payload embeds the region's full canonical form,
        // carrying `permits_spanning_slurs`. Because this encoding changed at
        // major 1, a `CreateRegion` operation encodes at schema major 1
        // ([`OperationKind::schema_major`]), and any op-envelope block carrying
        // one is stamped major 1 — a major-0-only reader opens such a bundle
        // read-only (Binary Format companion §"Schema Major 1").
        push_lp_bytes(out, &self.region.canonical_bytes());
    }
}

/// Tombstone an empty region (Chapter 6 §6.10 DeleteRegion). Delete-wins, but a
/// precondition NoOp if the region still has live staff instances.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DeleteRegionOp {
    pub region: RegionId,
}

impl CanonicalEncode for DeleteRegionOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
    }
}

/// Mint an empty staff instance into a live region (Chapter 6 §6.10
/// InsertStaffInstance). Carries the full [`StaffInstance`] value (v1); the
/// reduction preconditions it carries no voices.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateStaffInstanceOp {
    pub region: RegionId,
    pub instance: StaffInstance,
}

impl CreateStaffInstanceOp {
    /// The minted staff instance's identifier.
    pub fn instance_id(&self) -> StaffInstanceId {
        self.instance.id
    }
}

impl CanonicalEncode for CreateStaffInstanceOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        push_lp_bytes(out, &self.instance.canonical_bytes());
    }
}

/// Tombstone an empty staff instance (Chapter 6 §6.10 DeleteStaffInstance).
/// Delete-wins, but a precondition NoOp if it still has live voices.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DeleteStaffInstanceOp {
    pub staff_instance: StaffInstanceId,
}

impl CanonicalEncode for DeleteStaffInstanceOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.staff_instance);
    }
}

/// Mint an empty voice into a live staff instance (Chapter 6 §6.10 CreateVoice).
/// Carries the full [`Voice`] value (v1); the reduction preconditions it carries
/// no events.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateVoiceOp {
    pub staff_instance: StaffInstanceId,
    pub voice: Voice,
}

impl CreateVoiceOp {
    /// The minted voice's identifier.
    pub fn voice_id(&self) -> VoiceId {
        self.voice.id
    }
}

impl CanonicalEncode for CreateVoiceOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.staff_instance);
        push_lp_bytes(out, &self.voice.canonical_bytes());
    }
}

/// Tombstone an empty voice (Chapter 6 §6.10 DeleteVoice). Delete-wins, but a
/// precondition NoOp if it still has live events.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DeleteVoiceOp {
    pub voice: VoiceId,
}

impl CanonicalEncode for DeleteVoiceOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.voice);
    }
}

// --- Group 4 (M2d): score settings (Chapter 6 §6.10). LWW field-overwrite. ---

/// Overwrite the score metadata (Chapter 6 §6.10 SetMetadata). Carries the full
/// [`ScoreMetadata`] (v1); the score-singleton field-overwrite is *advisory*
/// last-writer-wins — the latest write in canonical order silently wins and no
/// conflict is recorded (operation_catalog §set-user-system-break "LWW advisory").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetMetadataOp {
    pub metadata: ScoreMetadata,
}

impl CanonicalEncode for SetMetadataOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.metadata.canonical_bytes());
    }
}

/// Overwrite a region's default metric grid (Chapter 6 §6.10 SetMetricGrid).
/// Carries the full target [`MetricGrid`] (or `None` to clear it); LWW
/// field-overwrite keyed by region (concurrent differing ⇒
/// structural-field-collision).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetMetricGridOp {
    pub region: RegionId,
    pub grid: Option<MetricGrid>,
}

impl CanonicalEncode for SetMetricGridOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        match &self.grid {
            None => push_tag(out, 0),
            Some(grid) => {
                push_tag(out, 1);
                push_lp_bytes(out, &grid.canonical_bytes());
            }
        }
    }
}

/// Set a user page-break preference (Chapter 6 §6.10 SetUserPageBreak) — the
/// page-break sibling of [`SetUserSystemBreakOp`]. Carries the full
/// [`TimeAnchor`] (v1); the LWW bucketing key is the anchor's resolved
/// [`MusicalPosition`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetUserPageBreakOp {
    pub region: RegionId,
    pub anchor: TimeAnchor,
    pub present: bool,
}

impl SetUserPageBreakOp {
    /// The anchor's resolved musical position — the canonical LWW bucketing key
    /// (see the private `resolved_anchor_position` helper).
    pub fn resolved_position(&self) -> MusicalPosition {
        resolved_anchor_position(&self.anchor)
    }
}

impl CanonicalEncode for SetUserPageBreakOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        push_lp_bytes(out, &self.anchor.canonical_bytes());
        push_u8_bool(out, self.present);
    }
}

// --- Phase-3 first tranche (operation_catalog §CreateStaff, §"Meter and Tempo
// Overwrites", §SetStaffLayout). ----------------------------------------------

/// Mint a global [`Staff`] on the score root (operation_catalog §CreateStaff).
/// Carries the full global-staff value (v1): identity, name, abbreviation,
/// instrument reference, default staff-line configuration, and optional group
/// membership. Set-union creation, completing the structural-container family
/// upward: staff *instances* reference global staves. A repeat create carrying
/// a byte-identical value is idempotent; a differing value under a live id is a
/// precondition no-op.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateStaffOp {
    pub staff: Staff,
}

impl CreateStaffOp {
    /// The minted staff's identifier.
    pub fn staff_id(&self) -> StaffId {
        self.staff.id
    }
}

impl CanonicalEncode for CreateStaffOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.staff.canonical_bytes());
    }
}

// --- Genesis tranche G1 (`spec/CONTRACT_GENESIS_G1_INSTRUMENT.md`). ----------

/// Mint an abstract [`Instrument`] on the score root (operation_catalog
/// §CreateInstrument). Carries the full instrument value (schema major 2):
/// identity, name, range, abbreviation, sound configuration, transposition,
/// default clef, default staff-line configuration, and unpitched members.
/// `Instrument` holds no outbound entity references, so this operation needs
/// no referential preconditions — only mint and idempotence, exactly the
/// `CreateStaff` discipline. Set-union creation: a repeat create carrying a
/// byte-identical value is idempotent; a differing value under a live id is a
/// precondition no-op.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateInstrumentOp {
    pub instrument: Instrument,
}

impl CreateInstrumentOp {
    /// The minted instrument's identifier.
    pub fn instrument_id(&self) -> InstrumentId {
        self.instrument.id
    }
}

impl CanonicalEncode for CreateInstrumentOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.instrument.canonical_bytes());
    }
}

// --- Genesis tranche G2a (`spec/CONTRACT_GENESIS_G2A_SETTINGS.md`): the two
// major-0 settings setters. LWW field-overwrite, exactly the `SetMetadata`
// discipline (advisory last-writer-wins, no conflict, no idempotence
// short-circuit). ---

/// Overwrite the canvas's default layout advisories (operation_catalog
/// §SetCanvasLayoutDefaults). Carries the full [`CanvasLayoutDefaults`]; the
/// score-singleton field-overwrite is *advisory* last-writer-wins — the latest
/// write in canonical order silently wins and no conflict is recorded, exactly
/// `SetMetadata`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetCanvasLayoutDefaultsOp {
    pub layout_defaults: CanvasLayoutDefaults,
}

impl CanonicalEncode for SetCanvasLayoutDefaultsOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.layout_defaults.canonical_bytes());
    }
}

/// Overwrite the score's spelling precedence (operation_catalog
/// §SetSpellingPrecedence). Carries the full [`SpellingPrecedence`]; the
/// score-singleton field-overwrite is *advisory* last-writer-wins, exactly
/// `SetMetadata`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetSpellingPrecedenceOp {
    pub precedence: SpellingPrecedence,
}

impl CanonicalEncode for SetSpellingPrecedenceOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.precedence.canonical_bytes());
    }
}

// --- Genesis tranche G2b (`spec/CONTRACT_GENESIS_G2B_TUNING.md`): the sole
// genesis payload born at schema major 3. LWW field-overwrite, the same
// `SetMetadata` discipline. ---

/// Overwrite the score's tuning context settings (operation_catalog
/// §SetTuningContext). Carries [`TuningContextSettings`] — the **authored
/// subset** of `ScoreTuningContext` (contract §1): exactly the five
/// wire-bearing fields, in the codec's existing order.
/// `accidental_extensions` is not a field of the carried type and is never
/// touched by reduction, which writes only these five fields onto
/// `score.tuning_context`. The score-singleton field-overwrite is *advisory*
/// last-writer-wins — the latest write in canonical order silently wins and
/// no conflict is recorded, exactly `SetMetadata`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetTuningContextOp {
    pub settings: TuningContextSettings,
}

impl CanonicalEncode for SetTuningContextOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.settings.canonical_bytes());
    }
}

/// Set, replace, or (`None`) remove the single meter change at the anchor's
/// resolved musical position in a region's default metric grid
/// (operation_catalog §"Meter and Tempo Overwrites"). Carries the full
/// [`TimeSignature`] value (v1), minted set-union under the same discipline as
/// `CreateStaff`. LWW structural overwrite keyed by `(region, resolved
/// position)`; concurrent differing writes collide on the field
/// `meter_sequence`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetTimeSignatureOp {
    pub region: RegionId,
    pub anchor: TimeAnchor,
    pub time_signature: Option<TimeSignature>,
}

impl SetTimeSignatureOp {
    /// The anchor's resolved musical position — the canonical LWW key (the
    /// same coarse resolution the user-break advisories use).
    pub fn resolved_position(&self) -> MusicalPosition {
        resolved_anchor_position(&self.anchor)
    }
}

impl CanonicalEncode for SetTimeSignatureOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.region);
        push_lp_bytes(out, &self.anchor.canonical_bytes());
        match &self.time_signature {
            None => push_tag(out, 0),
            Some(signature) => {
                push_tag(out, 1);
                push_lp_bytes(out, &signature.canonical_bytes());
            }
        }
    }
}

/// Set, replace, or (`None`) remove the single tempo segment starting at the
/// resolved position, in the score-level tempo map (`region: None`) or the
/// region's local map (`Some`; a set on a region with no local map creates
/// one) (operation_catalog §"Meter and Tempo Overwrites"). LWW structural
/// overwrite keyed by `(scope, resolved start)`; a write that would malform
/// the resulting map is refused
/// ([`PreconditionFailureReason::TempoMapMalformed`](crate::PreconditionFailureReason)).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetTempoSegmentOp {
    pub region: Option<RegionId>,
    pub start: TimeAnchor,
    pub segment: Option<TempoSegment>,
}

impl SetTempoSegmentOp {
    /// The start anchor's resolved musical position — the canonical LWW key's
    /// position half (the scope is the other half).
    pub fn resolved_start(&self) -> MusicalPosition {
        resolved_anchor_position(&self.start)
    }
}

impl CanonicalEncode for SetTempoSegmentOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        // Catalog: an Option discriminant and (when present) `region`, then the
        // length-framed `start`, then an Option discriminant and (when present)
        // the length-framed `segment`.
        match &self.region {
            None => push_tag(out, 0),
            Some(region) => {
                push_tag(out, 1);
                push_canon(out, region);
            }
        }
        push_lp_bytes(out, &self.start.canonical_bytes());
        match &self.segment {
            None => push_tag(out, 0),
            Some(segment) => {
                push_tag(out, 1);
                push_lp_bytes(out, &segment.canonical_bytes());
            }
        }
    }
}

/// Overwrite a staff instance's inline layout advisories as a unit
/// (operation_catalog §SetStaffLayout): the three non-break layout advisories
/// with a graph home (`instrument_override`, `staff_lines_override`,
/// `visible`). LWW *advisory* keyed by `staff_instance` — no conflicts. The
/// richer engraving-override vocabulary has no durable graph home yet and
/// remains projected layout state.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetStaffLayoutOp {
    pub staff_instance: StaffInstanceId,
    pub instrument_override: Option<InstrumentId>,
    pub staff_lines_override: Option<StaffLineConfiguration>,
    pub visible: bool,
}

impl CanonicalEncode for SetStaffLayoutOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.staff_instance);
        match &self.instrument_override {
            None => push_tag(out, 0),
            Some(instrument) => {
                push_tag(out, 1);
                push_canon(out, instrument);
            }
        }
        match &self.staff_lines_override {
            None => push_tag(out, 0),
            Some(lines) => {
                push_tag(out, 1);
                push_lp_bytes(out, &lines.canonical_bytes());
            }
        }
        push_u8_bool(out, self.visible);
    }
}

/// Mint a repeat structure (operation_catalog §"Repeat Structures"; Chapter 6
/// re-anchoring rule table "Repeat structure / Anchor"). Carries the full
/// [`RepeatStructure`] value — identity, `start`/`end` anchors, kind, and
/// voltas. The v2 layout is unconditional (`kind`/`voltas` are not `Option`s),
/// so the create is *born at v2*: see [`OperationKind::schema_major`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CreateRepeatStructureOp {
    pub repeat: RepeatStructure,
}

impl CreateRepeatStructureOp {
    /// The minted structure's identity (read from the carried value).
    pub fn repeat_structure_id(&self) -> RepeatStructureId {
        self.repeat.id
    }
}

impl CanonicalEncode for CreateRepeatStructureOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.repeat.canonical_bytes());
    }
}

/// Tombstone a repeat structure (operation_catalog §"Repeat Structures";
/// delete-wins, idempotent on re-delete). The payload is the bare identifier
/// — a major-0 layout under a minor kind append, so the op stamps major 0
/// (see [`OperationKind::schema_major`]).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DeleteRepeatStructureOp {
    pub repeat: RepeatStructureId,
}

impl CanonicalEncode for DeleteRepeatStructureOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.repeat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{RegionId, ReplicaId, SlurId};

    fn envelope(id: u64, payload: OperationPayload) -> OperationEnvelope {
        use crate::causal::CausalContext;
        use crate::stamp::{HybridLogicalClock, OperationStamp};
        use crate::support::AuthorId;
        use epiphany_core::WallClockTime;
        let oid = OperationId::new(ReplicaId(1), id);
        OperationEnvelope {
            id: oid,
            author: AuthorId(1),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(id as i64), 0), oid),
            causal_context: CausalContext::new(),
            transaction: None,
            payload,
        }
    }

    fn create_instrument_envelope(id: u64) -> OperationEnvelope {
        envelope(
            id,
            OperationPayload::Primitive(OperationKind::CreateInstrument(CreateInstrumentOp {
                instrument: crate::valuegen::instrument(epiphany_core::InstrumentId::new(
                    ReplicaId(1),
                    id,
                )),
            })),
        )
    }

    fn set_user_page_break_envelope(id: u64) -> OperationEnvelope {
        // Baseline kind 23 (golden-locked 0..=23): no additive requirement.
        envelope(
            id,
            OperationPayload::Primitive(OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                region: RegionId::new(ReplicaId(1), id),
                anchor: crate::valuegen::region_start_anchor(
                    RegionId::new(ReplicaId(1), id),
                    MusicalPosition::origin(),
                ),
                present: true,
            })),
        )
    }

    fn resolve_equivocation_envelope(id: u64) -> OperationEnvelope {
        envelope(
            id,
            OperationPayload::ResolveEquivocation(ResolveEquivocationPayload {
                target: OperationId::new(ReplicaId(1), id),
                chosen: EnvelopeHash([0; 32]),
            }),
        )
    }

    fn create_repeat_structure_envelope(id: u64) -> OperationEnvelope {
        let repeat_id = RepeatStructureId::new(ReplicaId(1), id);
        let a = EventId::new(ReplicaId(1), id * 10 + 1);
        let b = EventId::new(ReplicaId(1), id * 10 + 2);
        envelope(
            id,
            OperationPayload::Primitive(OperationKind::CreateRepeatStructure(
                CreateRepeatStructureOp {
                    repeat: crate::valuegen::repeat_structure(repeat_id, a, b),
                },
            )),
        )
    }

    // -----------------------------------------------------------------
    // G-minor block/envelope derivation: s4, s5, s6.
    // -----------------------------------------------------------------

    #[test]
    fn s4_a_block_containing_kind_31_stamps_minor_8() {
        let envelopes = vec![create_instrument_envelope(1)];
        assert_eq!(operation_block_introduced_minor(&envelopes), Some(8));
    }

    #[test]
    fn s5_mixing_an_old_kind_23_with_resolve_equivocation_stamps_3_not_1() {
        // `SetUserPageBreak` is kind 23 (baseline, no additive requirement);
        // `ResolveEquivocation` is the outer payload appended at minor 3. The
        // block's minor must be the *epoch* max (3), never the highest
        // *discriminant* (23) — the rejected policy pin 3 names explicitly.
        let envelopes = vec![
            set_user_page_break_envelope(1),
            resolve_equivocation_envelope(2),
        ];
        assert_eq!(operation_block_introduced_minor(&envelopes), Some(3));
    }

    #[test]
    fn s6_major_and_minor_derive_independently() {
        // `CreateRepeatStructure` (kind 28) is schema-major 2 (born at v2) and
        // epoch 6 (schema-major-2 repeat-authoring revision). A major-2 block
        // containing it must stamp minor 6 regardless of major — the two
        // derivations must never influence each other.
        let envelopes = vec![create_repeat_structure_envelope(1)];
        let major = envelopes.iter().map(|e| e.schema_major()).max().unwrap();
        assert_eq!(
            major, 2,
            "major derives from schema_major, independent of epoch"
        );
        let epoch_max = operation_block_introduced_minor(&envelopes);
        assert_eq!(
            epoch_max,
            Some(6),
            "minor epoch derives from introduced_minor, independent of major"
        );
        // `SchemaVersion::for_major_at_epoch(2, Some(6))` (tested directly in
        // `epiphany-bundle`'s `ids.rs`, which this crate does not depend on)
        // combines these two independent values into `{2, 6}` — this test's
        // job is only to show the two inputs to that combination are each
        // correct and mutually uninfluenced.
    }

    #[test]
    fn operation_kind_wire_discriminants_are_golden() {
        // GOLDEN LOCK: the discriminant byte leads every canonically-encoded
        // primitive payload (operation_catalog §"Value-Typed Payloads"), so the
        // literal values are normative wire facts. Encodings are append-only:
        // new kinds append past 29; the values below never change.
        use crate::valuegen;
        use epiphany_core::{MusicalDuration, MusicalPosition, TimeSignatureId};

        let r = ReplicaId(1);
        let event_a = EventId::new(r, 1);
        let event_b = EventId::new(r, 2);
        let pitch = PitchId::new(r, 3);
        let region = RegionId::new(r, 4);
        let staff = StaffId::new(r, 5);
        let instance = StaffInstanceId::new(r, 6);
        let voice = VoiceId::new(r, 7);
        let slur_id = SlurId::new(r, 8);
        let instrument = InstrumentId::new(r, 10);
        let event_value = || {
            valuegen::insert_event_value(
                event_a,
                voice,
                MusicalPosition::origin(),
                MusicalDuration::whole(),
                &[],
            )
        };
        let slur_value = || CrossCuttingValue::Slur(valuegen::slur(slur_id, event_a, event_b));
        let anchor = || valuegen::region_start_anchor(region, MusicalPosition::origin());

        let repeat_id = RepeatStructureId::new(r, 12);
        let table: [(OperationKind, u8); 30] = [
            (
                OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: instance,
                    event: event_value(),
                }),
                0,
            ),
            (
                OperationKind::DeleteEvent(DeleteEventOp {
                    event: event_a,
                    tuplet_compensation: TupletCompensation::NotInTuplet,
                }),
                1,
            ),
            (
                OperationKind::RespellPitch(RespellPitchOp {
                    pitch,
                    spelling: valuegen::spelling(1),
                }),
                2,
            ),
            (
                OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: slur_value(),
                }),
                3,
            ),
            (
                OperationKind::ChangeRegionTimeModel(ChangeRegionTimeModelOp {
                    region,
                    new_time_model: valuegen::metric_model(),
                    declared_incompatible: vec![],
                    remapping: PositionRemapping::PreserveTime,
                }),
                4,
            ),
            (
                OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
                    region,
                    anchor: anchor(),
                    present: true,
                }),
                5,
            ),
            (
                OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: TransactionId::new(r, 9),
                    label: String::new(),
                    category: None,
                }),
                6,
            ),
            (
                OperationKind::Registered(OperationKindRegistryId(0), vec![]),
                7,
            ),
            (
                OperationKind::ModifyEvent(ModifyEventOp {
                    event: event_value(),
                }),
                8,
            ),
            (
                OperationKind::Transpose(TransposeOp {
                    targets: vec![pitch],
                    chromatic_steps: 0,
                }),
                9,
            ),
            (
                OperationKind::InsertIdentifiedPitch(InsertIdentifiedPitchOp {
                    event: event_a,
                    pitch: valuegen::identified_pitch(pitch),
                }),
                10,
            ),
            (
                OperationKind::DeleteIdentifiedPitch(DeleteIdentifiedPitchOp { pitch }),
                11,
            ),
            (
                OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                    pitch,
                    value: valuegen::pitch_value(),
                }),
                12,
            ),
            (
                OperationKind::DeleteCrossCutting(DeleteCrossCuttingOp {
                    structure: TypedObjectId::Slur(slur_id),
                }),
                13,
            ),
            (
                OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                    structure: slur_value(),
                }),
                14,
            ),
            (
                OperationKind::CreateRegion(CreateRegionOp {
                    region: valuegen::region(region),
                }),
                15,
            ),
            (OperationKind::DeleteRegion(DeleteRegionOp { region }), 16),
            (
                OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                    region,
                    instance: valuegen::staff_instance(instance, staff),
                }),
                17,
            ),
            (
                OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
                    staff_instance: instance,
                }),
                18,
            ),
            (
                OperationKind::CreateVoice(CreateVoiceOp {
                    staff_instance: instance,
                    voice: valuegen::voice(voice),
                }),
                19,
            ),
            (OperationKind::DeleteVoice(DeleteVoiceOp { voice }), 20),
            (
                OperationKind::SetMetadata(SetMetadataOp {
                    metadata: valuegen::score_metadata(0),
                }),
                21,
            ),
            (
                OperationKind::SetMetricGrid(SetMetricGridOp { region, grid: None }),
                22,
            ),
            (
                OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                    region,
                    anchor: anchor(),
                    present: true,
                }),
                23,
            ),
            (
                OperationKind::CreateStaff(CreateStaffOp {
                    staff: valuegen::staff(staff, instrument),
                }),
                24,
            ),
            (
                OperationKind::SetTimeSignature(SetTimeSignatureOp {
                    region,
                    anchor: anchor(),
                    time_signature: Some(valuegen::time_signature(TimeSignatureId::new(r, 11), 4)),
                }),
                25,
            ),
            (
                OperationKind::SetTempoSegment(SetTempoSegmentOp {
                    region: Some(region),
                    start: anchor(),
                    segment: Some(valuegen::tempo_segment(
                        region,
                        MusicalPosition::origin(),
                        120.0,
                    )),
                }),
                26,
            ),
            (
                OperationKind::SetStaffLayout(SetStaffLayoutOp {
                    staff_instance: instance,
                    instrument_override: Some(instrument),
                    staff_lines_override: None,
                    visible: true,
                }),
                27,
            ),
            (
                OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
                    repeat: valuegen::repeat_structure(repeat_id, event_a, event_b),
                }),
                28,
            ),
            (
                OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp { repeat: repeat_id }),
                29,
            ),
        ];
        for (kind, expected) in &table {
            assert_eq!(
                kind.discriminant(),
                *expected,
                "wire discriminant for {:?} moved — canonical encodings are append-only",
                kind.tag(),
            );
            // The discriminant byte truly leads the canonical encoding.
            assert_eq!(kind.to_canonical_bytes()[0], *expected);
        }
    }

    #[test]
    fn operation_payload_discriminants_are_golden() {
        // GOLDEN LOCK: the payload-union discriminant byte leads every
        // canonically-encoded envelope payload. 0..=2 are the ratified v1
        // values; 3 (ResolveEquivocation) is appended by the catalog entry
        // §"ResolveEquivocation". Append-only; never renumber.
        use crate::undo::UndoPolicy;
        use epiphany_core::OperationId;

        let r = ReplicaId(1);
        let primitive = OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: EventId::new(r, 1),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        }));
        let resolve_conflict = OperationPayload::ResolveConflict(ResolveConflictPayload {
            target: ConflictId(0),
            action: ResolutionAction::Dismiss,
        });
        let undo = OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: TransactionId::new(r, 2),
            policy: UndoPolicy::BestEffort,
        });
        let resolve_equivocation =
            OperationPayload::ResolveEquivocation(ResolveEquivocationPayload {
                target: OperationId::new(r, 3),
                chosen: EnvelopeHash([0; 32]),
            });
        for (payload, expected) in [
            (&primitive, 0u8),
            (&resolve_conflict, 1),
            (&undo, 2),
            (&resolve_equivocation, 3),
        ] {
            assert_eq!(payload.discriminant(), expected);
            assert_eq!(payload.to_canonical_bytes()[0], expected);
        }
    }

    #[test]
    fn resolve_equivocation_payload_encodes_target_then_hash() {
        // Catalog §"ResolveEquivocation": the canonical encoding is exactly the
        // target's 16 canonical bytes followed by the 32 hash bytes.
        use epiphany_core::OperationId;
        let target = OperationId::new(ReplicaId(0x0102_0304_0506_0708), 0x1122_3344_5566_7788);
        let chosen = EnvelopeHash([0xAB; 32]);
        let p = ResolveEquivocationPayload { target, chosen };
        let bytes = p.to_canonical_bytes();
        assert_eq!(bytes.len(), 48);
        assert_eq!(&bytes[..16], &target.canonical_bytes());
        assert_eq!(&bytes[16..], &[0xAB; 32]);
    }

    #[test]
    fn create_region_payload_is_v1_and_carries_the_spanning_flag() {
        // Schema major 1: the CreateRegion payload embeds the region's full
        // canonical form, so permits_spanning_slurs IS carried — a region with
        // the flag set encodes differently from one without, and the payload is
        // exactly the region's (v1) canonical_bytes, length-prefixed.
        let rid = RegionId::new(ReplicaId(9), 3);
        let mut permit = crate::valuegen::region(rid);
        permit.permits_spanning_slurs = true;
        let forbid = crate::valuegen::region(rid); // valuegen defaults the flag false

        let enc = |region: Region| {
            let mut out = Vec::new();
            CreateRegionOp { region }.encode_canonical(&mut out);
            out
        };
        // The flag is carried: the two encode differently.
        assert_ne!(enc(permit.clone()), enc(forbid.clone()));
        // And the payload is exactly the region's v1 canonical form, LP-framed.
        let mut expected = Vec::new();
        push_lp_bytes(&mut expected, &forbid.canonical_bytes());
        assert_eq!(enc(forbid), expected);
    }

    #[test]
    fn create_region_op_encodes_at_schema_major_1() {
        // CreateRegion's canonical encoding changed at major 1, so the op reports
        // schema major 1; a representative major-0 op reports 0.
        let rid = RegionId::new(ReplicaId(9), 3);
        let create = OperationKind::CreateRegion(CreateRegionOp {
            region: crate::valuegen::region(rid),
        });
        assert_eq!(create.schema_major(), 1);
        let delete = OperationKind::DeleteRegion(DeleteRegionOp { region: rid });
        assert_eq!(delete.schema_major(), 0);
    }

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

    /// **Every** `OperationKindTag`, derived from the production vocabulary.
    ///
    /// Not a hand-written list. `OperationKindTag::PAYLOAD_FREE` is generated by
    /// `operation_kind_tag_vocabulary!`, whose `discriminant` match is
    /// exhaustive over the enum — so a variant that exists cannot be missing
    /// here, and the fuzz corpus, the conformance vectors, and the edit-barrier
    /// round-trip all read the same constant.
    fn all_tags() -> Vec<OperationKindTag> {
        let mut tags = OperationKindTag::PAYLOAD_FREE.to_vec();
        tags.push(OperationKindTag::Registered(OperationKindRegistryId(
            0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10,
        )));
        tags
    }

    /// One past the payload-free vocabulary — computed, never spelled. Spelling
    /// it is how `30` came to be asserted *unknown* while it was
    /// `TransposeInterval`.
    fn first_unknown_discriminant() -> u8 {
        OperationKindTag::PAYLOAD_FREE
            .iter()
            .map(OperationKindTag::discriminant)
            .max()
            .expect("a non-empty vocabulary")
            + 1
    }

    /// A compile-time-ish completeness check: `all_tags` must match the
    /// vocabulary the decoder accepts, in both directions.
    #[test]
    fn the_tag_vocabulary_is_complete() {
        let tags = all_tags();
        let unknown = first_unknown_discriminant();

        // The payload-free discriminants are exactly `0..unknown`, minus the one
        // `Registered` occupies. No gaps: the vocabulary is dense and
        // append-only.
        for d in (0..unknown).filter(|d| *d != REGISTERED_TAG_DISCRIMINANT) {
            let decoded = OperationKindTag::decode_canonical(&[d])
                .unwrap_or_else(|e| panic!("discriminant {d} must decode: {e:?}"));
            assert!(
                tags.contains(&decoded),
                "{decoded:?} missing from PAYLOAD_FREE"
            );
        }
        assert_eq!(
            tags.len(),
            usize::from(unknown),
            "{unknown} payload-free discriminants (one of them `Registered`'s), \
             plus the `Registered` value itself"
        );
        assert!(OperationKindTag::decode_canonical(&[unknown]).is_err());
    }

    #[test]
    fn every_normative_operation_tag_has_a_distinct_canonical_discriminant() {
        let tags = all_tags();
        let encoded: std::collections::BTreeSet<_> = tags
            .iter()
            .map(CanonicalEncode::to_canonical_bytes)
            .collect();
        assert_eq!(encoded.len(), tags.len());
    }

    #[test]
    fn operation_kind_tag_decode_mirrors_encode_exactly() {
        // Every variant round-trips: the payload-less ones through their 1-byte
        // form, `Registered` through its 17-byte (tag + registry id) form.
        //
        // Driven from `all_tags()`, i.e. from the VARIANTS. Driving it from the
        // discriminants the decoder already knows is what let a variant the
        // decoder was missing pass unnoticed.
        for tag in all_tags() {
            let bytes = tag.to_canonical_bytes();
            let decoded = OperationKindTag::decode_canonical(&bytes).expect("round-trips");
            assert_eq!(decoded, tag);
            assert_eq!(
                decoded.to_canonical_bytes(),
                bytes,
                "re-encode is byte-identical"
            );
        }
    }

    #[test]
    fn operation_kind_tag_decode_rejects_malformed_bytes() {
        use epiphany_determinism::DecodeError;
        // One past the vocabulary: rejected, never normalized. COMPUTED, not
        // spelled. This assertion literally said `30` until Push 5 / P4, by
        // which time 30 was `TransposeInterval` — so the test was locking a bug
        // in place rather than guarding against one. A spelled constant here is
        // a trap that springs on whoever appends the next tag.
        assert_eq!(
            OperationKindTag::decode_canonical(&[first_unknown_discriminant()]),
            Err(DecodeError::MalformedDomainTag)
        );
        // Empty input.
        assert!(OperationKindTag::decode_canonical(&[]).is_err());
        // Trailing byte after a payload-less tag.
        assert!(OperationKindTag::decode_canonical(&[0, 0]).is_err());
        // A truncated (and an oversized) Registered payload.
        assert!(OperationKindTag::decode_canonical(&[16; 16]).is_err());
        assert!(OperationKindTag::decode_canonical(&[16; 19]).is_err());
    }

    #[test]
    fn phase3_tag_discriminants_are_golden() {
        // GOLDEN LOCK (Phase-3 first tranche): appended past the ratified
        // 0..=23; the values below never change.
        for (tag, expected) in [
            (OperationKindTag::InsertStaff, 24u8),
            (OperationKindTag::SetTimeSignature, 25),
            (OperationKindTag::SetTempoSegment, 26),
            (OperationKindTag::SetStaffLayout, 27),
            // Schema-major-2 revision (repeat authoring).
            (OperationKindTag::CreateRepeatStructure, 28),
            (OperationKindTag::DeleteRepeatStructure, 29),
        ] {
            assert_eq!(tag.discriminant(), expected);
            assert_eq!(tag.to_canonical_bytes(), vec![expected]);
        }
    }

    // -----------------------------------------------------------------
    // G-minor (`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4, pin 1's ratified
    // epoch table): s1, s2.
    // -----------------------------------------------------------------

    #[test]
    fn s1_operation_kind_tag_epochs_match_pin1s_table() {
        // Every post-baseline `OperationKindTag` epoch, transcribed from pin
        // 1's ratified table.
        for (tag, expected) in [
            (OperationKindTag::InsertStaff, 4u16),
            (OperationKindTag::SetTimeSignature, 4),
            (OperationKindTag::SetTempoSegment, 4),
            (OperationKindTag::SetStaffLayout, 4),
            (OperationKindTag::CreateRepeatStructure, 6),
            (OperationKindTag::DeleteRepeatStructure, 6),
            (OperationKindTag::TransposeInterval, 7),
            (OperationKindTag::CreateInstrument, 8),
            (OperationKindTag::SetCanvasLayoutDefaults, 9),
            (OperationKindTag::SetSpellingPrecedence, 9),
        ] {
            assert_eq!(
                tag.introduced_minor(),
                Some(expected),
                "{tag:?} must require epoch {expected}"
            );
        }
    }

    #[test]
    fn s1_operation_kind_epochs_match_tag_epochs_across_every_generated_kind() {
        // `OperationKind`'s own epoch table mirrors `OperationKindTag`'s (same
        // ten events); rather than hand-build all thirty-four payloads, this
        // checks every kind the corpus generator reaches agrees with its own
        // tag's already pin-1-verified epoch (s1 above) — so a divergence in
        // either table is caught.
        use crate::fuzz::gen_envelope_set;
        use epiphany_determinism::fuzz::SplitMix64;
        let mut rng = SplitMix64::new(0xC0FFEE);
        let envelopes = gen_envelope_set(&mut rng, 400);
        let mut seen_post_baseline = false;
        for envelope in &envelopes {
            if let OperationPayload::Primitive(kind) = &envelope.payload {
                assert_eq!(
                    kind.introduced_minor(),
                    kind.tag().introduced_minor(),
                    "{:?}: OperationKind and OperationKindTag epochs disagree",
                    kind.tag()
                );
                seen_post_baseline |= kind.introduced_minor().is_some();
            }
        }
        assert!(
            seen_post_baseline,
            "fixture reach: the generated set never reached a post-baseline kind"
        );
    }

    #[test]
    fn s1_operation_payload_and_reanchor_and_precondition_epochs_match_pin1() {
        assert_eq!(
            OperationPayload::ResolveEquivocation(ResolveEquivocationPayload {
                target: OperationId::new(epiphany_core::ReplicaId(1), 1),
                chosen: EnvelopeHash([0; 32]),
            })
            .introduced_minor(),
            Some(3)
        );
        assert_eq!(
            crate::effect::ReanchorReason::SameCanvasNearer.introduced_minor(),
            Some(5)
        );
        for (reason, expected) in [
            (
                crate::effect::PreconditionFailureReason::ContainerNotEmpty,
                2u16,
            ),
            (
                crate::effect::PreconditionFailureReason::TempoMapMalformed,
                4,
            ),
            (
                crate::effect::PreconditionFailureReason::SystemDerivedContentImmutable,
                5,
            ),
            (
                crate::effect::PreconditionFailureReason::RecreateContentMismatch,
                5,
            ),
            (
                crate::effect::PreconditionFailureReason::AcousticRealizationPinned,
                7,
            ),
            (
                crate::effect::PreconditionFailureReason::TranspositionOutOfRange,
                7,
            ),
        ] {
            assert_eq!(reason.introduced_minor(), Some(expected), "{reason:?}");
        }
    }

    #[test]
    fn s2_every_baseline_variant_returns_no_additive_requirement() {
        // A representative baseline member of each of the five vocabularies.
        assert_eq!(OperationKindTag::InsertEvent.introduced_minor(), None);
        assert_eq!(OperationKindTag::SetMetricGrid.introduced_minor(), None);
        assert_eq!(
            OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
                staff_instance: StaffInstanceId::new(epiphany_core::ReplicaId(1), 1),
                event: crate::valuegen::insert_event_value(
                    EventId::new(epiphany_core::ReplicaId(1), 2),
                    VoiceId::new(epiphany_core::ReplicaId(1), 3),
                    MusicalPosition::origin(),
                    MusicalDuration::whole(),
                    &[],
                ),
            }))
            .introduced_minor(),
            None
        );
        assert_eq!(
            crate::effect::ReanchorReason::SameVoiceNearer.introduced_minor(),
            None
        );
        assert_eq!(
            crate::effect::PreconditionFailureReason::TargetMissing.introduced_minor(),
            None
        );
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
