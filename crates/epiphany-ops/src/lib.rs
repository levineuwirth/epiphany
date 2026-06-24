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
mod envelope;
mod migrate;
mod opset;
mod payload;
mod reduce;
mod slot;
mod stamp;
mod support;
mod v0;
pub mod valuegen;

pub mod fuzz;

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
pub use envelope::{well_formed, EnvelopeHash, OperationEnvelope, WellFormednessError};
pub use migrate::{migrate_v0_envelope, project_v1_to_v0, MigrationError};
pub use opset::OperationSet;
pub use payload::{
    ChangeRegionTimeModelOp, CreateCrossCuttingOp, CrossCuttingValue, DeleteEventOp,
    DeleteIdentifiedPitchOp, InsertEventOp, InsertIdentifiedPitchOp, ModifyEventOp,
    ModifyIdentifiedPitchOp, OperationKind, OperationKindTag, OperationPayload, PositionRemapping,
    ResolveConflictPayload, RespellPitchOp, SetUserSystemBreakOp, TransactionCategory,
    TransactionDescriptor, TransposeOp, TupletCompensation,
};
pub use reduce::{
    canonical_reduction_order, GraphMaterialization, MaterializedState, ObjectState, PendingReason,
};
pub use slot::OperationSlot;
pub use stamp::{HybridLogicalClock, OperationStamp, StampTuple};
pub use support::{
    AuthorId, ConflictKindRegistryId, ExtensionPreconditionId, IntegrityAnomalyRegistryId,
    ObjectKind, OperationKindRegistryId, PreconditionFailureRegistryId, ReanchorReasonRegistryId,
    RepairKindRegistryId, ReplicaAnomalyRegistryId, ResolutionRegistryId,
    SerializedCanonicalInputs,
};

pub use undo::{UndoPolicy, UndoTransactionPayload};
pub use v0::V0OperationEnvelope;

mod undo;
