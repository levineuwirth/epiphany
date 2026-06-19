//! Operation effects and the re-anchoring vocabulary (Chapter 6 §"Operation
//! Effects", §"Re-Anchoring").
//!
//! Every operation in the canonical set produces exactly one
//! [`OperationEffect`] under reduction, and those effects are *visible graph
//! facts* recorded in the materialized state's effect log — "part of canonical
//! state, not implementation detail" (Chapter 6 §6.3.2). Two replicas reducing
//! the same operation set in the canonical order produce the same effect for
//! each operation, including identical [`NoOpReason`] values.
//!
//! The failure reason on a no-op is a *typed* [`PreconditionFailureReason`],
//! never a free-form string: canonical effects must not contain free-form text,
//! since two implementations could otherwise produce divergent canonical state
//! while agreeing semantically (Chapter 6 §6.3.1).

use epiphany_core::{OperationId, TypedObjectId, VoiceId};
use epiphany_determinism::CanonicalEncode;

use crate::conflict::ConflictId;
use crate::encode::{push_canon, push_seq, push_tag};
use crate::support::{
    ExtensionPreconditionId, PreconditionFailureRegistryId, ReanchorReasonRegistryId,
    RepairKindRegistryId,
};

/// The deterministic effect of an operation under canonical reduction
/// (Chapter 6 §6.3.2). Recorded as part of materialized state.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OperationEffect {
    /// Applied cleanly with no compensating changes.
    Applied,
    /// Applied with deterministic compensating changes (re-anchoring,
    /// attachment migration, voice promotion, …).
    AppliedWithRepair { repairs: Vec<RepairRecord> },
    /// Could not apply cleanly; a conflict record was created.
    Conflicted { conflict: ConflictId },
    /// The target was tombstoned; preserved in the operation set, no graph
    /// effect beyond the recorded effect.
    TombstonedTarget { target: TypedObjectId },
    /// Reduces to no effect; the reason is recorded and is canonical.
    NoOp { reason: NoOpReason },
}

impl OperationEffect {
    fn discriminant(&self) -> u8 {
        match self {
            OperationEffect::Applied => 0,
            OperationEffect::AppliedWithRepair { .. } => 1,
            OperationEffect::Conflicted { .. } => 2,
            OperationEffect::TombstonedTarget { .. } => 3,
            OperationEffect::NoOp { .. } => 4,
        }
    }
}

impl CanonicalEncode for OperationEffect {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            OperationEffect::Applied => {}
            OperationEffect::AppliedWithRepair { repairs } => push_seq(out, repairs),
            OperationEffect::Conflicted { conflict } => push_canon(out, conflict),
            OperationEffect::TombstonedTarget { target } => push_canon(out, target),
            OperationEffect::NoOp { reason } => reason.encode_canonical(out),
        }
    }
}

/// Why an operation reduced to no effect (Chapter 6 §6.3.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NoOpReason {
    /// The target was tombstoned by a causally-prior operation and the kind has
    /// no compensating repair rule.
    TargetTombstoned,
    /// Duplicates a causally-prior operation's effect.
    AlreadyApplied,
    /// A later operation in canonical order subsumed this one's effect.
    SupersededByLaterOperation { superseder: OperationId },
    /// An invariant precondition satisfied at authoring time fails under
    /// concurrent reduction; the intent is not preserved.
    PreconditionFailedUnderReduction { reason: PreconditionFailureReason },
    /// Belonged to a transaction whose other members failed.
    TransactionConflict,
}

impl NoOpReason {
    fn discriminant(&self) -> u8 {
        match self {
            NoOpReason::TargetTombstoned => 0,
            NoOpReason::AlreadyApplied => 1,
            NoOpReason::SupersededByLaterOperation { .. } => 2,
            NoOpReason::PreconditionFailedUnderReduction { .. } => 3,
            NoOpReason::TransactionConflict => 4,
        }
    }
}

impl CanonicalEncode for NoOpReason {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            NoOpReason::TargetTombstoned
            | NoOpReason::AlreadyApplied
            | NoOpReason::TransactionConflict => {}
            NoOpReason::SupersededByLaterOperation { superseder } => push_canon(out, superseder),
            NoOpReason::PreconditionFailedUnderReduction { reason } => reason.encode_canonical(out),
        }
    }
}

/// A typed precondition-failure reason (Chapter 6 §6.3.2). Never free-form
/// text — canonical effects must be byte-reproducible across implementations.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PreconditionFailureReason {
    /// The target object did not exist in the working state at reduction.
    TargetMissing,
    /// The target was tombstoned by a causally-prior operation, and a
    /// repair-on-tombstone attempt itself failed precondition.
    TargetTombstoned,
    /// The operation requires a region time model different from the one in
    /// effect at the target position.
    WrongRegionTimeModel,
    /// A declared tuplet compensation is invalid against the current structure.
    TupletCompensationInvalid,
    /// An event duration the operation specifies is invalid in the target
    /// voice or region.
    EventDurationInvalid,
    /// The target position falls outside the region declared by the envelope,
    /// or the region does not exist.
    PositionOutsideRegion,
    /// A pitch-space or tuning-context precondition failed.
    PitchSpaceMismatch,
    /// The operation targeted a voice that does not exist or is tombstoned.
    VoiceMissing,
    /// An extension-declared precondition failed.
    ExtensionPrecondition(ExtensionPreconditionId),
    /// A registered precondition code from a versioned registry.
    Registered(PreconditionFailureRegistryId),
}

impl PreconditionFailureReason {
    fn discriminant(&self) -> u8 {
        match self {
            PreconditionFailureReason::TargetMissing => 0,
            PreconditionFailureReason::TargetTombstoned => 1,
            PreconditionFailureReason::WrongRegionTimeModel => 2,
            PreconditionFailureReason::TupletCompensationInvalid => 3,
            PreconditionFailureReason::EventDurationInvalid => 4,
            PreconditionFailureReason::PositionOutsideRegion => 5,
            PreconditionFailureReason::PitchSpaceMismatch => 6,
            PreconditionFailureReason::VoiceMissing => 7,
            PreconditionFailureReason::ExtensionPrecondition(_) => 8,
            PreconditionFailureReason::Registered(_) => 9,
        }
    }
}

impl CanonicalEncode for PreconditionFailureReason {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            PreconditionFailureReason::ExtensionPrecondition(id) => push_canon(out, id),
            PreconditionFailureReason::Registered(id) => push_canon(out, id),
            _ => {}
        }
    }
}

/// A deterministic compensating change made during reduction, recorded as part
/// of canonical state (Chapter 6 §6.3.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RepairRecord {
    /// The kind of repair performed.
    pub kind: RepairKind,
    /// The object affected by this repair.
    pub target: TypedObjectId,
}

impl CanonicalEncode for RepairRecord {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        self.kind.encode_canonical(out);
        push_canon(out, &self.target);
    }
}

/// The kind of repair a reduction performed to preserve graph invariants in the
/// presence of a tombstoning or migrating operation (Chapter 6 §6.3.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RepairKind {
    /// A reference was re-anchored to a surviving target per the re-anchoring
    /// rule table (Chapter 6 §6.5).
    Reanchored {
        from: TypedObjectId,
        to: TypedObjectId,
        reason: ReanchorReason,
    },
    /// A spanner or beam lost members but enough remained to survive.
    SpannerTruncated { removed_members: Vec<TypedObjectId> },
    /// A user-content reference was orphaned (target lost, reference kept).
    Orphaned,
    /// A reference whose existence required its target was cascade-deleted.
    CascadeDeleted,
    /// An attachment transitioned to tombstoned-target state.
    AttachmentTombstoned,
    /// A voice was promoted to a system-derived identity on concurrent
    /// insertion collision (Chapter 6 §6.10 InsertEvent).
    VoicePromoted { from: VoiceId, to: VoiceId },
    /// A tuplet was compensated per the operation's declared compensation.
    TupletCompensated {
        compensation_kind: TupletCompensationKind,
    },
    /// A registered extension-defined repair kind.
    Registered(RepairKindRegistryId),
}

impl RepairKind {
    fn discriminant(&self) -> u8 {
        match self {
            RepairKind::Reanchored { .. } => 0,
            RepairKind::SpannerTruncated { .. } => 1,
            RepairKind::Orphaned => 2,
            RepairKind::CascadeDeleted => 3,
            RepairKind::AttachmentTombstoned => 4,
            RepairKind::VoicePromoted { .. } => 5,
            RepairKind::TupletCompensated { .. } => 6,
            RepairKind::Registered(_) => 7,
        }
    }
}

impl CanonicalEncode for RepairKind {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            RepairKind::Reanchored { from, to, reason } => {
                push_canon(out, from);
                push_canon(out, to);
                reason.encode_canonical(out);
            }
            RepairKind::SpannerTruncated { removed_members } => push_seq(out, removed_members),
            RepairKind::Orphaned
            | RepairKind::CascadeDeleted
            | RepairKind::AttachmentTombstoned => {}
            RepairKind::VoicePromoted { from, to } => {
                push_canon(out, from);
                push_canon(out, to);
            }
            RepairKind::TupletCompensated { compensation_kind } => {
                compensation_kind.encode_canonical(out)
            }
            RepairKind::Registered(id) => push_canon(out, id),
        }
    }
}

/// The reason a reference was re-anchored to a new target (Chapter 6 §6.5).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ReanchorReason {
    SameVoiceNearer,
    SameStaffInstanceNearer,
    SameStaffNearer,
    SameRegionNearer,
    ExplicitFallback,
    DeclaredByExtension(ReanchorReasonRegistryId),
}

impl ReanchorReason {
    fn discriminant(&self) -> u8 {
        match self {
            ReanchorReason::SameVoiceNearer => 0,
            ReanchorReason::SameStaffInstanceNearer => 1,
            ReanchorReason::SameStaffNearer => 2,
            ReanchorReason::SameRegionNearer => 3,
            ReanchorReason::ExplicitFallback => 4,
            ReanchorReason::DeclaredByExtension(_) => 5,
        }
    }
}

impl CanonicalEncode for ReanchorReason {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        if let ReanchorReason::DeclaredByExtension(id) = self {
            push_canon(out, id);
        }
    }
}

/// Which tuplet compensation a [`RepairKind::TupletCompensated`] applied
/// (Chapter 6 §6.10 DeleteEvent). Mirrors the operation's declared
/// [`crate::TupletCompensation`] choice.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TupletCompensationKind {
    ReplaceWithRest,
    RewriteTuplets,
    CascadeDeleteTuplets,
}

impl TupletCompensationKind {
    fn discriminant(&self) -> u8 {
        match self {
            TupletCompensationKind::ReplaceWithRest => 0,
            TupletCompensationKind::RewriteTuplets => 1,
            TupletCompensationKind::CascadeDeleteTuplets => 2,
        }
    }
}

impl CanonicalEncode for TupletCompensationKind {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
    }
}

/// The result of the re-anchor function for one referencing object
/// (Chapter 6 §6.5). The reduction maps each result to a [`RepairKind`] (or a
/// conflict) on the triggering operation's effect.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReanchorResult {
    /// The reference is replaced with a reference to a new target.
    Reanchored {
        new_target: TypedObjectId,
        reason: ReanchorReason,
    },
    /// The reference is preserved but its target is tombstoned.
    TombstonedTarget,
    /// The referencing object is marked orphaned but retained.
    Orphaned,
    /// Re-anchoring cannot be performed deterministically; a conflict is
    /// recorded.
    Conflicted { conflict: ConflictId },
    /// The referencing object is cascade-deleted (tombstoned).
    CascadeDeleted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{EventId, ReplicaId};

    #[test]
    fn distinct_effects_encode_distinctly() {
        let a = OperationEffect::Applied;
        let b = OperationEffect::NoOp {
            reason: NoOpReason::AlreadyApplied,
        };
        let c = OperationEffect::TombstonedTarget {
            target: TypedObjectId::Event(EventId::from_raw(1)),
        };
        assert_ne!(a.to_canonical_bytes(), b.to_canonical_bytes());
        assert_ne!(a.to_canonical_bytes(), c.to_canonical_bytes());
        assert_ne!(b.to_canonical_bytes(), c.to_canonical_bytes());
    }

    #[test]
    fn typed_precondition_reasons_are_distinguishable() {
        let a = NoOpReason::PreconditionFailedUnderReduction {
            reason: PreconditionFailureReason::VoiceMissing,
        };
        let b = NoOpReason::PreconditionFailedUnderReduction {
            reason: PreconditionFailureReason::TargetMissing,
        };
        assert_ne!(a.to_canonical_bytes(), b.to_canonical_bytes());
    }

    #[test]
    fn voice_promotion_repair_round_trips_shape() {
        let r = RepairRecord {
            kind: RepairKind::VoicePromoted {
                from: VoiceId::new(ReplicaId(1), 2),
                to: VoiceId::new(ReplicaId::SYSTEM_DERIVED, 99),
            },
            target: TypedObjectId::Voice(VoiceId::new(ReplicaId(1), 2)),
        };
        // Encoding is stable.
        assert_eq!(r.to_canonical_bytes(), r.to_canonical_bytes());
    }
}
