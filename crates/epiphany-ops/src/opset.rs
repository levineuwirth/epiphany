//! The replicated operation set and its acceptance pipeline (Chapter 6
//! §"Envelope Acceptance", §"OperationId Collisions").
//!
//! The operation set is a grow-only CRDT: replicas accumulate envelopes by any
//! delivery mechanism and converge on the same set. It is *set-valued* — an
//! envelope is a member or it is not, with no count — and a replica never
//! discards envelopes except by the (out-of-scope-for-v0) pruning protocol.
//!
//! [`OperationSet::accept`] is the whole pipeline: well-formedness check →
//! slot transition → retention. Every step is a pure function of the *set* of
//! observed envelopes, so two replicas that observe the same envelopes in
//! different orders reach byte-identical slot states (the Pass-10
//! order-independence property; v0 acceptance criterion 3). Causal validation
//! (the missing-predecessor rule) is applied later, during reduction, where the
//! whole set is visible.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::OperationId;

use crate::envelope::{well_formed, EnvelopeHash, OperationEnvelope, WellFormednessError};
use crate::reduce::{reduce_operation_set, MaterializedState};
use crate::slot::OperationSlot;

/// The outcome of accepting one envelope (Chapter 6 §6.5 transition rules).
/// Returned for diagnostics and tests; the canonical state is the resulting
/// slot, not this value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AcceptOutcome {
    /// No slot existed; a `Single` slot was created.
    Accepted,
    /// A byte-identical duplicate of an existing `Single` slot; dropped.
    Duplicate,
    /// A second distinct canonical envelope under an existing id; the slot
    /// transitioned to (or remained) `Equivocated`.
    Equivocated,
    /// A further distinct candidate added to an already-`Equivocated` slot.
    EquivocationExtended,
    /// The envelope was not well-formed; rejected, not added to the set.
    Rejected(WellFormednessError),
}

/// The replicated operation set (Chapter 6 §6.2.4): a map from
/// [`OperationId`] to [`OperationSlot`], plus the diagnostic candidate store
/// that retains every distinct equivocating envelope by hash.
#[derive(Clone, Debug, Default)]
pub struct OperationSet {
    slots: BTreeMap<OperationId, OperationSlot>,
    /// Diagnostic store of equivocating envelopes, keyed by hash. The CRDT
    /// property forbids silent removal, so equivocating envelopes are retained
    /// here even though they contribute nothing to canonical reduction.
    candidates: BTreeMap<EnvelopeHash, OperationEnvelope>,
    /// Count of envelopes rejected at reception (diagnostic only; rejected
    /// envelopes never enter the set).
    rejected: u64,
}

impl OperationSet {
    /// An empty operation set.
    #[inline]
    pub fn new() -> Self {
        OperationSet::default()
    }

    /// Accepts an envelope into the set, applying the Chapter 6 §6.5 transition
    /// rules. The result is independent of the order in which envelopes are
    /// accepted.
    pub fn accept(&mut self, env: OperationEnvelope) -> AcceptOutcome {
        if let Err(e) = well_formed(&env) {
            self.rejected += 1;
            return AcceptOutcome::Rejected(e);
        }
        let id = env.id;
        let hash = env.envelope_hash();
        match self.slots.get_mut(&id) {
            None => {
                self.slots.insert(id, OperationSlot::Single(env));
                AcceptOutcome::Accepted
            }
            Some(OperationSlot::Single(existing)) => {
                if existing.envelope_hash() == hash {
                    // Byte-identical duplicate: drop silently, slot unchanged.
                    AcceptOutcome::Duplicate
                } else {
                    // Distinct second envelope: transition to Equivocated,
                    // retaining both candidates by hash.
                    let existing_hash = existing.envelope_hash();
                    let existing_env = match self.slots.remove(&id) {
                        Some(OperationSlot::Single(e)) => e,
                        _ => unreachable!("just matched a Single slot"),
                    };
                    self.candidates.insert(existing_hash, existing_env);
                    self.candidates.insert(hash, env);
                    let mut candidates = BTreeSet::new();
                    candidates.insert(existing_hash);
                    candidates.insert(hash);
                    self.slots.insert(
                        id,
                        OperationSlot::Equivocated {
                            operation_id: id,
                            candidates,
                        },
                    );
                    AcceptOutcome::Equivocated
                }
            }
            Some(OperationSlot::Equivocated { candidates, .. }) => {
                if candidates.insert(hash) {
                    self.candidates.insert(hash, env);
                    AcceptOutcome::EquivocationExtended
                } else {
                    // Already a known candidate: drop.
                    AcceptOutcome::Duplicate
                }
            }
        }
    }

    /// Accepts many envelopes, returning the per-envelope outcomes in input
    /// order. A convenience over repeated [`OperationSet::accept`].
    pub fn accept_all<I: IntoIterator<Item = OperationEnvelope>>(
        &mut self,
        envs: I,
    ) -> Vec<AcceptOutcome> {
        envs.into_iter().map(|e| self.accept(e)).collect()
    }

    /// The number of slots (distinct `OperationId`s observed).
    #[inline]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the set has no slots.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The slot for `id`, if any.
    #[inline]
    pub fn slot(&self, id: OperationId) -> Option<&OperationSlot> {
        self.slots.get(&id)
    }

    /// All slots, in ascending `OperationId` order.
    #[inline]
    pub fn slots(&self) -> impl Iterator<Item = (&OperationId, &OperationSlot)> {
        self.slots.iter()
    }

    /// The `Single`-slot envelopes, in ascending `OperationId` order. These are
    /// the envelopes eligible for canonical reduction (before anomaly exclusion
    /// and the missing-predecessor rule).
    pub fn single_envelopes(&self) -> Vec<&OperationEnvelope> {
        self.slots
            .values()
            .filter_map(OperationSlot::single)
            .collect()
    }

    /// The equivocated `OperationId`s, in ascending order.
    pub fn equivocated_ids(&self) -> Vec<OperationId> {
        self.slots
            .values()
            .filter(|s| s.is_equivocated())
            .map(OperationSlot::operation_id)
            .collect()
    }

    /// Whether `id` resolves to a reducible `Single` slot.
    #[inline]
    pub fn has_single(&self, id: OperationId) -> bool {
        matches!(self.slots.get(&id), Some(OperationSlot::Single(_)))
    }

    /// A retained equivocating envelope by hash (diagnostic recovery).
    #[inline]
    pub fn candidate(&self, hash: EnvelopeHash) -> Option<&OperationEnvelope> {
        self.candidates.get(&hash)
    }

    /// The count of envelopes rejected at reception (diagnostic).
    #[inline]
    pub fn rejected_count(&self) -> u64 {
        self.rejected
    }

    /// Reduces the operation set to its canonical [`MaterializedState`]
    /// (Chapter 6 §6.3). Deterministic: any operation set with the same slots
    /// reduces to byte-identical state regardless of how it was assembled.
    pub fn reduce(&self) -> MaterializedState {
        reduce_operation_set(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::causal::CausalContext;
    use crate::payload::{OperationKind, RespellPitchOp};
    use crate::stamp::{HybridLogicalClock, OperationStamp};
    use crate::support::AuthorId;
    use crate::OperationPayload;
    use epiphany_core::{PitchId, ReplicaId, WallClockTime};
    use epiphany_determinism::ContentHash;

    fn env_with(id: OperationId, spelling: u8) -> OperationEnvelope {
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(1), 0), id),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                pitch: PitchId::new(ReplicaId(1), 1),
                spelling: ContentHash([spelling; 32]),
            })),
        }
    }

    #[test]
    fn duplicate_is_idempotent() {
        let mut set = OperationSet::new();
        let id = OperationId::new(ReplicaId(1), 1);
        assert_eq!(set.accept(env_with(id, 7)), AcceptOutcome::Accepted);
        assert_eq!(set.accept(env_with(id, 7)), AcceptOutcome::Duplicate);
        assert_eq!(set.len(), 1);
        assert!(set.has_single(id));
    }

    #[test]
    fn distinct_envelope_under_same_id_equivocates_regardless_of_order() {
        let id = OperationId::new(ReplicaId(1), 1);
        // Order A then B.
        let mut s1 = OperationSet::new();
        s1.accept(env_with(id, 7));
        assert_eq!(s1.accept(env_with(id, 9)), AcceptOutcome::Equivocated);
        // Order B then A.
        let mut s2 = OperationSet::new();
        s2.accept(env_with(id, 9));
        assert_eq!(s2.accept(env_with(id, 7)), AcceptOutcome::Equivocated);
        // Both equivocated, same candidate set.
        assert!(!s1.has_single(id));
        assert!(!s2.has_single(id));
        let c1: Vec<_> = s1.slot(id).unwrap().candidates().collect();
        let c2: Vec<_> = s2.slot(id).unwrap().candidates().collect();
        assert_eq!(c1, c2, "equivocation candidates are order-independent");
    }

    #[test]
    fn malformed_envelope_is_rejected_not_added() {
        let mut set = OperationSet::new();
        let id = OperationId::new(ReplicaId(1), 1);
        let mut bad = env_with(id, 7);
        bad.stamp.id = OperationId::new(ReplicaId(1), 2); // stamp.id != id
        assert_eq!(
            set.accept(bad),
            AcceptOutcome::Rejected(WellFormednessError::StampIdMismatch)
        );
        assert!(set.is_empty());
        assert_eq!(set.rejected_count(), 1);
    }
}
