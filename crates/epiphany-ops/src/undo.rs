//! Forward, compensating undo (Chapter 6 §"Undo").
//!
//! Undo is **not** literal time travel. An [`UndoTransactionPayload`] is a new
//! operation, committed to the operation set like any other, whose reduction
//! computes a compensating edit against the materialized state at its canonical
//! position. History stays append-only; the obvious inverse-op approach is
//! explicitly rejected because it breaks under concurrent edits (Chapter 6
//! §"Design Principles"; QUICKSTART "Don't implement undo as inverse-based").

use epiphany_core::TransactionId;
use epiphany_determinism::CanonicalEncode;

use crate::encode::{push_canon, push_tag};

/// The payload of an undo meta-operation (Chapter 6 §6.8.1).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct UndoTransactionPayload {
    /// The transaction being undone.
    pub target: TransactionId,
    /// How to undo when the target's effects have been partially superseded.
    pub policy: UndoPolicy,
}

impl CanonicalEncode for UndoTransactionPayload {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.target);
        self.policy.encode_canonical(out);
    }
}

/// Policy governing undo when the target's effects are partially superseded
/// (Chapter 6 §6.8.1).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum UndoPolicy {
    /// Compute the inverse of each primitive against the current materialized
    /// state. If any cannot apply cleanly, the entire undo conflicts.
    StrictInverse,
    /// Undo what remains valid; record conflicts or no-ops for the rest. The
    /// undo succeeds, but partial.
    BestEffort,
    /// Also undo operations causally or semantically dependent on the target.
    /// Dependency is computed from causal contexts and explicit dependency
    /// links; broad musical dependence MUST NOT be inferred heuristically.
    Cascade,
}

impl UndoPolicy {
    fn discriminant(&self) -> u8 {
        match self {
            UndoPolicy::StrictInverse => 0,
            UndoPolicy::BestEffort => 1,
            UndoPolicy::Cascade => 2,
        }
    }
}

impl CanonicalEncode for UndoPolicy {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
    }
}
