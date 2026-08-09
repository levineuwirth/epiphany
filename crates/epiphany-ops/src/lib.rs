#![forbid(unsafe_code)]
//! # epiphany-ops
//!
//! The Epiphany **concurrent semantics**: the operations through which the
//! score graph becomes a *live* model, and the deterministic reduction by
//! which a set of operations becomes a materialized score state. This crate
//! implements the normative requirements of **Chapter 6 (Semantic Operations
//! and Concurrent Reduction)** of the core specification. It is Agent C's crate
//! per `spec/QUICKSTART.md`; it depends on [`epiphany_determinism`] (Agent A)
//! and [`epiphany_core`] (Agent B), and on nothing else.
//!
//! ## The thesis in one paragraph
//!
//! A score's canonical state is the set of operations committed to it; the
//! materialized graph is a *deterministic reduction* of that set (Chapter 6
//! §"Design Principles"). The replicated operation set is a grow-only CRDT;
//! the materialized graph is not. Replicas accumulate [envelopes](OperationEnvelope)
//! and converge on the same set, then reduce it — *in a single canonical order*
//! — to byte-identical materialized state. The canonical reduction order
//! ([`canonical_reduction_order`]) is the determinism heart of the
//! architecture: any permutation of the same input envelopes reduces to the
//! same bytes (Appendix D §"Canonical score determinism"). If that does not
//! hold, nothing else matters.
//!
//! ## What lives here
//!
//! * `stamp` — [`OperationStamp`] and the [`HybridLogicalClock`], with the
//!   per-replica monotonicity tuple `(physical, logical, counter)` that the
//!   canonical order and anomaly detection both consume (Chapter 6 §6.6).
//! * `causal` — [`CausalContext`] as a dotted version vector, and the
//!   happens-before closure used for transaction ordering and the
//!   missing-predecessor rule (Chapter 6 §6.2).
//! * `payload` — [`OperationKind`], the discriminator-only [`OperationKindTag`],
//!   [`OperationPayload`], and the representative operation payloads the chapter
//!   specifies reduction rules for (Chapter 6 §6.10).
//! * `envelope` — [`OperationEnvelope`], its canonical serialization, the
//!   [`EnvelopeHash`] (`MUSCENVH`), and the well-formedness contract including
//!   the `stamp.id == id` invariant (Chapter 6 §6.4).
//! * `slot` — the order-independent [`OperationSlot`] model: `Single` or
//!   `Equivocated`, with the Pass-10 transition rules (Chapter 6 §6.5).
//! * `anomaly` — [`AnomalousReplicaSegment`] and the [`IntegrityAnomaly`]
//!   register, kept separate from ordinary conflicts (Chapter 6 §6.6,
//!   Chapter 5 §"System-Derived Counter Collisions").
//! * `effect` — [`OperationEffect`], [`NoOpReason`], the typed
//!   [`PreconditionFailureReason`], and the [`RepairRecord`] / [`RepairKind`]
//!   re-anchoring vocabulary (Chapter 6 §6.2.3, §6.7).
//! * `conflict` — [`ConflictRecord`], [`ConflictKind`], the content-derived
//!   [`ConflictId`] ([`derive_conflict_id`]), and the conflict registry
//!   (Chapter 6 §6.4).
//! * `transaction` / `undo` — [`TransactionDescriptor`] with the
//!   causal-prior-descriptor rule, and [`UndoTransactionPayload`] with its
//!   [`UndoPolicy`] (Chapter 6 §6.6, §6.8).
//! * `opset` — [`OperationSet`]: the slot map plus the acceptance pipeline
//!   (well-formedness → slot transition → causal validation).
//! * `reduce` — [`canonical_reduction_order`], [`MaterializedState`], and the
//!   reduction driver (Chapter 6 §6.3). [`OperationSet::reduce_onto`] also
//!   materializes the representative mutations into an Agent B
//!   [`epiphany_core::Score`].
//!
//! ## Scope (per QUICKSTART and Chapter 6 §6.11)
//!
//! Chapter 6 specifies the *framework* and a *representative selection* of
//! operations; the full catalog of ~60–80 primitives is an explicit open
//! question (§6.11) deferred to the Operation Catalog companion. This crate
//! mirrors that: it implements the framework in full and the representative
//! operations the chapter gives reduction rules for, which is sufficient to
//! exercise every reduction *discipline* (position-keyed insert with voice
//! promotion, delete-wins with tombstones and re-anchoring, field-overwrite
//! with conflict records, set-union, LWW-advisory, structural-migration
//! conflict, and atomic transactions). See `DECISIONS.md` for the boundary and
//! the batched Pass 11 candidates.
//!
//! ## Implementation decisions (per QUICKSTART "Decisions you'll need to make")
//!
//! Fully sync, no async (decision 4); current stable Rust, MSRV 1.77
//! (decision 5); `unsafe` forbidden crate-wide. Canonical iteration is enforced
//! structurally with `BTreeMap`/`BTreeSet` and sorted projections
//! (Appendix D §"Ordered Iteration").

mod anomaly;
mod causal;
mod conflict;
mod decode;
mod effect;
mod encode;
mod envdecode;
mod envelope;
mod migrate;
mod opset;
mod payload;
mod reduce;
mod slot;
mod stamp;
mod support;
#[cfg(test)]
mod textproj_conformance;
mod textproj_envelope;
mod textproj_kind;
mod textproj_leaf;
mod v0;
mod validate;
pub mod valuegen;

pub mod fuzz;
pub mod vectors;

/// The reduction semantics **this build implements**, as a bare number.
///
/// `core_spec.tex` §"Canonical Document Identity" is normative: *snapshots
/// produced under an earlier algorithm version cannot be used as canonical
/// bases under a later one without rebuilding*. Enforcing that needs a value
/// naming what the running implementation actually does — and before P13-S27
/// no such value existed anywhere. `ReductionAlgorithmVersion`
/// (`epiphany-bundle`) was a wire field whose reader compared it only against
/// the superblock that same value had seeded, so the check was a tautology for
/// every conformingly-written document.
///
/// # The bump discipline — this is the whole guarantee
///
/// **Any change to a canonical reduction verdict, or to canonical reduced
/// state, MUST bump this constant and record the change in the list below.**
///
/// **No mechanism can detect a semantics change.** A golden test over reduction
/// outputs can *prompt* the question — outputs moved, did semantics? — but it
/// can never answer it: a deliberate semantics change and an accidental
/// regression look identical from outside. **The discipline is the guarantee;
/// there is no backstop.**
///
/// # Why `0`, and why that is a decision
///
/// Bundles written to date carry `0` when they have no canonical base, and
/// bases self-report whatever they were stamped with. Starting anywhere but `0`
/// would make every existing base-bearing document fail to open **without any
/// semantics having changed** — the check would manufacture the breakage it
/// exists to detect. `0` is therefore a decision, **not "unset"**.
///
/// # Bumps
///
/// * `0` — the baseline. The semantics `canonical_reduction_order` and
///   `reduce_onto` implement as of P13-S27 (2026-08-08). No earlier version
///   exists; nothing predates this constant.
///
/// The first real bump belongs to **P13-S16**, which changes
/// `CreateStaffGroup`'s reduction verdict and must move this to `1`.
///
/// # Layering
///
/// This is a plain `u32`, and `epiphany-ops` **MUST NOT** gain a dependency on
/// `epiphany-bundle` in order to use that crate's `ReductionAlgorithmVersion`
/// wrapper. The wrapper is constructed at the composition boundary by whoever
/// depends on both (P13-S27 pin 1, §0.3).
pub const CURRENT_REDUCTION_ALGORITHM_VERSION: u32 = 0;

pub use anomaly::{
    AnomalousReplicaSegment, IntegrityAnomaly, IntegrityAnomalyKind, ReplicaAnomalyReason,
};
pub use causal::CausalContext;
pub use conflict::{
    derive_conflict_id, ConflictId, ConflictKind, ConflictRecord, ConflictRegistry,
    ConflictResolutionState, FieldPath, ResolutionAction,
};
pub use decode::MaterializedDecodeError;
pub use effect::{
    NoOpReason, OperationEffect, PreconditionFailureReason, ReanchorReason, ReanchorResult,
    RepairKind, RepairRecord, TupletCompensationKind,
};
pub use envdecode::{decode_envelope, EnvelopeDecodeError};
pub use envelope::{
    peek_operation_id, well_formed, EnvelopeHash, OperationEnvelope, WellFormednessError,
};
pub use migrate::{migrate_v0_envelope, project_v1_to_v0, MigrationError};
pub use opset::{AcceptOutcome, OperationSet};
pub use payload::{
    operation_block_introduced_minor, ChangeRegionTimeModelOp, CreateAnalysisLayerOp,
    CreateCrossCuttingOp, CreateInstrumentOp, CreateMeasureOp, CreatePartDefinitionOp,
    CreateRegionOp, CreateRepeatStructureOp, CreateStaffGroupOp, CreateStaffInstanceOp,
    CreateStaffOp, CreateViewOp, CreateVoiceOp, CrossCuttingValue, DeleteCrossCuttingOp,
    DeleteEventOp, DeleteIdentifiedPitchOp, DeleteRegionOp, DeleteRepeatStructureOp,
    DeleteStaffInstanceOp, DeleteVoiceOp, InsertEventOp, InsertIdentifiedPitchOp,
    ModifyCrossCuttingOp, ModifyEventOp, ModifyIdentifiedPitchOp, OperationKind, OperationKindTag,
    OperationPayload, PositionRemapping, ResolveConflictPayload, ResolveEquivocationPayload,
    RespellPitchOp, SetCanvasLayoutDefaultsOp, SetMetadataOp, SetMetricGridOp,
    SetSpellingPrecedenceOp, SetStaffLayoutOp, SetTempoSegmentOp, SetTimeSignatureOp,
    SetTuningContextOp, SetUserPageBreakOp, SetUserSystemBreakOp, TransactionCategory,
    TransactionDescriptor, TransposeIntervalOp, TransposeOp, TupletCompensation,
};
pub use reduce::{
    canonical_reduction_order, measure_anchor_relation_for_agreement_test, GraphMaterialization,
    MaterializedState, ObjectState, PendingReason,
};
pub use slot::OperationSlot;
pub use stamp::{HybridLogicalClock, OperationStamp, StampTuple};
pub use support::{
    AuthorId, ConflictKindRegistryId, ExtensionPreconditionId, IntegrityAnomalyRegistryId,
    ObjectKind, OperationKindRegistryId, PreconditionFailureRegistryId, ReanchorReasonRegistryId,
    RepairKindRegistryId, ReplicaAnomalyRegistryId, ResolutionRegistryId,
    SerializedCanonicalInputs,
};
pub use textproj_envelope::{parse_envelope, project_envelope};

pub use undo::{UndoPolicy, UndoTransactionPayload};
pub use v0::V0OperationEnvelope;
pub use validate::{advisory_violations, AdvisoryViolation, ValidationMode};

mod undo;
