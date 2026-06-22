//! Causal context as a dotted version vector (Chapter 6 §"Causal Context via
//! Dotted Version Vectors").
//!
//! Operations carry a *compact* causal context, not an exhaustive predecessor
//! list — exhaustive lists scale linearly with history and become untenable.
//! A [`CausalContext`] is a dotted version vector (DVV):
//!
//! * `vector`: for each replica the authoring replica knows, the highest
//!   *contiguous* counter it has observed. `vector[r] = n` asserts that every
//!   operation `(r, 0..=n)` is a causal predecessor. The counter floor is
//!   **zero-based** and normative — RATIFIED by Pass 11 (item 3.4, P11-C7):
//!   core_spec §"Causal Context via Dotted Version Vectors" now pins the
//!   zero-based floor so a second implementation cannot pick a one-based floor
//!   and diverge on pending detection.
//! * `dots`: individual [`OperationId`]s observed but not yet contiguous in the
//!   vector — "known but not yet contiguous" predecessors.
//!
//! The canonical reduction order is causal-first (Chapter 6 §6.3.3). Although
//! correctly authored operations give causal predecessors strictly-lesser HLC
//! stamps, accepted remote envelopes may violate that authoring rule. Reduction
//! therefore topologically orders the DVV edges and uses HLC only among ready
//! operations — see [`crate::canonical_reduction_order`]. The DVV also drives
//! the *missing-causal-predecessor* rule (an operation whose predecessor is
//! absent, equivocated, or excluded is held pending) and the transaction
//! descriptor-precedence rule (Chapter 6 §6.7).

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{OperationId, ReplicaId};
use epiphany_determinism::CanonicalEncode;

use crate::encode::{push_canon, push_len, push_u64};

/// A compact causal context: a dotted version vector (Chapter 6 §6.2).
///
/// Both members are canonically-ordered collections (`BTreeMap` keyed by
/// [`ReplicaId`], `BTreeSet` of [`OperationId`]), so iteration is already in the
/// Appendix-D normative order and the canonical encoding is order-independent
/// of how the context was built.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CausalContext {
    /// Highest contiguous counter observed per replica.
    pub vector: BTreeMap<ReplicaId, u64>,
    /// Individual operations known but not yet contiguous in the vector.
    pub dots: BTreeSet<OperationId>,
}

impl CausalContext {
    /// The empty context: no observed predecessors (a root operation).
    #[inline]
    pub fn new() -> Self {
        CausalContext::default()
    }

    /// Records that every operation of `replica` up to and including `counter`
    /// has been observed (the contiguous-history assertion). A later, higher
    /// value for the same replica replaces an earlier one.
    pub fn with_seen(mut self, replica: ReplicaId, counter: u64) -> Self {
        let slot = self.vector.entry(replica).or_insert(counter);
        if counter > *slot {
            *slot = counter;
        }
        self
    }

    /// Records a single non-contiguous predecessor (a "dot").
    pub fn with_dot(mut self, op: OperationId) -> Self {
        self.dots.insert(op);
        self
    }

    /// Whether `op` is a (direct) causal predecessor under this context: either
    /// its counter is within the contiguous range recorded for its replica, or
    /// it appears among the dots.
    #[inline]
    pub fn covers(&self, op: OperationId) -> bool {
        if let Some(&high) = self.vector.get(&op.replica) {
            if op.counter <= high {
                return true;
            }
        }
        self.dots.contains(&op)
    }

    /// Whether this context references any predecessor at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.vector.is_empty() && self.dots.is_empty()
    }

    /// The dots as a slice-free iterator, in canonical (ascending) order.
    #[inline]
    pub fn dots(&self) -> impl Iterator<Item = OperationId> + '_ {
        self.dots.iter().copied()
    }
}

impl CanonicalEncode for CausalContext {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        // Vector: count, then (replica big-endian, counter little-endian) in
        // ascending replica order (BTreeMap iteration is already canonical).
        push_len(out, self.vector.len());
        for (replica, counter) in &self.vector {
            out.extend_from_slice(&replica.to_be_bytes());
            push_u64(out, *counter);
        }
        // Dots: count, then each OperationId's 16 canonical bytes in ascending
        // order (BTreeSet iteration is already canonical).
        push_len(out, self.dots.len());
        for dot in &self.dots {
            push_canon(out, dot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(r: u64, c: u64) -> OperationId {
        OperationId::new(ReplicaId(r), c)
    }

    #[test]
    fn covers_uses_contiguous_range_and_dots() {
        let ctx = CausalContext::new()
            .with_seen(ReplicaId(1), 5)
            .with_dot(op(2, 9));
        assert!(ctx.covers(op(1, 0)));
        assert!(ctx.covers(op(1, 5)));
        assert!(!ctx.covers(op(1, 6)));
        assert!(ctx.covers(op(2, 9)));
        assert!(!ctx.covers(op(2, 8)));
        assert!(!ctx.covers(op(3, 0)));
    }

    #[test]
    fn with_seen_keeps_the_highest_counter() {
        let ctx = CausalContext::new()
            .with_seen(ReplicaId(1), 5)
            .with_seen(ReplicaId(1), 3);
        assert_eq!(ctx.vector.get(&ReplicaId(1)), Some(&5));
    }

    #[test]
    fn canonical_encoding_is_build_order_independent() {
        let a = CausalContext::new()
            .with_seen(ReplicaId(2), 1)
            .with_seen(ReplicaId(1), 7)
            .with_dot(op(9, 9))
            .with_dot(op(3, 3));
        let b = CausalContext::new()
            .with_dot(op(3, 3))
            .with_seen(ReplicaId(1), 7)
            .with_dot(op(9, 9))
            .with_seen(ReplicaId(2), 1);
        assert_eq!(a.to_canonical_bytes(), b.to_canonical_bytes());
    }
}
