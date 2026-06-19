//! The order-independent operation slot (Chapter 6 §"OperationId Collisions and
//! Replica Equivocation"; Pass 10).
//!
//! Two received envelopes may carry the same [`OperationId`]. Acceptance must
//! **not** depend on arrival order — if it did, two replicas observing the same
//! envelopes in different orders could disagree on which is canonical. So the
//! operation set maps each `OperationId` to an [`OperationSlot`] rather than
//! directly to an envelope:
//!
//! * `Single` — exactly one canonical envelope is known; it reduces normally.
//! * `Equivocated` — two or more *distinct* canonical envelopes have been
//!   observed; the slot produces no canonical effect until external recovery
//!   resolves it. The candidate envelope *hashes* live in the slot (sorted, so
//!   enumeration is deterministic); the full envelopes are retained in the
//!   operation set's diagnostic candidate store.
//!
//! The transition rules ([`OperationSet::accept`](crate::OperationSet::accept))
//! are a pure function of the *set* of observed envelopes, never their order:
//! observing a second distinct canonical envelope transitions the slot to
//! `Equivocated` regardless of which arrived first.

use std::collections::BTreeSet;

use epiphany_core::OperationId;

use crate::envelope::{EnvelopeHash, OperationEnvelope};

/// A slot in the operation set, keyed by [`OperationId`] (Chapter 6 §6.5).
///
/// The `Single` variant is intentionally the larger one: it holds the whole
/// canonical [`OperationEnvelope`], the common case on the reduction hot path,
/// while `Equivocated` is the rare structural-failure case. The spec models the
/// slot as `Single(OperationEnvelope)` (not boxed), and boxing the common
/// variant would add an allocation per accepted envelope, so the size
/// difference is accepted deliberately.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OperationSlot {
    /// Exactly one canonical envelope is known for this `OperationId`.
    Single(OperationEnvelope),
    /// Two or more distinct canonical envelopes have been observed; the slot is
    /// equivocated and contributes nothing to canonical reduction.
    Equivocated {
        operation_id: OperationId,
        /// Every distinct canonical envelope hash observed, sorted
        /// lexicographically for deterministic enumeration.
        candidates: BTreeSet<EnvelopeHash>,
    },
}

impl OperationSlot {
    /// The `OperationId` this slot is keyed by.
    #[inline]
    pub fn operation_id(&self) -> OperationId {
        match self {
            OperationSlot::Single(env) => env.id,
            OperationSlot::Equivocated { operation_id, .. } => *operation_id,
        }
    }

    /// Whether this slot is equivocated (and thus excluded from reduction).
    #[inline]
    pub fn is_equivocated(&self) -> bool {
        matches!(self, OperationSlot::Equivocated { .. })
    }

    /// The single canonical envelope, if this slot is `Single`. An equivocated
    /// slot returns `None` — it contributes nothing to canonical reduction.
    #[inline]
    pub fn single(&self) -> Option<&OperationEnvelope> {
        match self {
            OperationSlot::Single(env) => Some(env),
            OperationSlot::Equivocated { .. } => None,
        }
    }

    /// The candidate hashes of an equivocated slot, in canonical order; empty
    /// for a `Single` slot.
    pub fn candidates(&self) -> impl Iterator<Item = EnvelopeHash> + '_ {
        // `Box`-free: yield from the set for Equivocated, nothing for Single.
        let set: Option<&BTreeSet<EnvelopeHash>> = match self {
            OperationSlot::Equivocated { candidates, .. } => Some(candidates),
            OperationSlot::Single(_) => None,
        };
        set.into_iter().flat_map(|s| s.iter().copied())
    }
}
