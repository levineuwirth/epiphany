//! Operation stamps and the hybrid logical clock (Chapter 6 §"Operation
//! Identity and Stamps").
//!
//! An operation carries two related but distinct things: its *identity*
//! ([`epiphany_core::OperationId`] — who authored it, with what counter) and
//! its *stamp* (when it was committed, for ordering). Identity is fixed at
//! authoring and never moves; the stamp is consumed only as ordering metadata
//! and "never for identity."
//!
//! The [`HybridLogicalClock`] combines a physical wall-clock component with a
//! logical counter that advances when physical time does not. Two derived
//! orderings come off the stamp:
//!
//! * The **per-replica monotonicity tuple** `(physical, logical, id.counter)`
//!   (Chapter 6 §6.6): for two envelopes from one replica with counters
//!   `c1 < c2`, the `c1` tuple MUST be ≤ the `c2` tuple, or the replica stream
//!   is anomalous.
//! * The **canonical reduction tuple** `(physical, logical, replica, counter)`
//!   (Chapter 6 §6.3.3): the order in which *concurrent* operations reduce.
//!
//! Acceptance never trusts stamp *content*: a peer may emit implausible future
//! times or extreme logical counters, and the reduction consumes the stamp as
//! ordering metadata without validating plausibility (Chapter 6 §6.4). The one
//! thing a well-formed stamp MUST satisfy is a finite, non-negative physical
//! time (checked in [`crate::well_formed`]).

use epiphany_core::{OperationId, ReplicaId, WallClockTime};
use epiphany_determinism::CanonicalEncode;

use crate::encode::{push_canon, push_u32};

/// A hybrid logical clock: a physical wall-clock component plus a logical
/// counter advanced when physical time does not move (Chapter 6
/// §"Operation Identity and Stamps").
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct HybridLogicalClock {
    /// Physical time component, in canonical nanosecond units
    /// ([`WallClockTime`]). Well-formed envelopes carry a finite, non-negative
    /// value (Chapter 6 §6.4).
    pub physical_time: WallClockTime,
    /// Logical counter, advanced when physical time does not.
    pub logical_counter: u32,
}

impl HybridLogicalClock {
    /// Builds a clock reading.
    #[inline]
    pub const fn new(physical_time: WallClockTime, logical_counter: u32) -> Self {
        HybridLogicalClock {
            physical_time,
            logical_counter,
        }
    }
}

impl CanonicalEncode for HybridLogicalClock {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.physical_time);
        push_u32(out, self.logical_counter);
    }
}

/// The ordering stamp of an operation (Chapter 6): a clock reading plus the
/// operation's identity. The `id` here MUST equal the envelope's top-level
/// `id` (the `stamp.id == id` well-formedness invariant, Chapter 6 §6.4).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct OperationStamp {
    /// The hybrid logical clock reading at commit.
    pub hlc: HybridLogicalClock,
    /// The operation this stamp addresses. Ordering metadata, never identity.
    pub id: OperationId,
}

impl OperationStamp {
    /// Builds a stamp.
    #[inline]
    pub const fn new(hlc: HybridLogicalClock, id: OperationId) -> Self {
        OperationStamp { hlc, id }
    }

    /// The canonical reduction tuple `(physical, logical, replica, counter)`
    /// used to order *concurrent* operations (Chapter 6 §6.3.3). Causal order
    /// dominates this, but the authoring HLC rule guarantees a causal
    /// predecessor's tuple is strictly less, so a plain lexicographic sort by
    /// this tuple is already causal-respecting (see [`crate::canonical_reduction_order`]).
    #[inline]
    pub fn reduction_tuple(&self) -> StampTuple {
        StampTuple {
            physical_time: self.hlc.physical_time,
            logical_counter: self.hlc.logical_counter,
            replica: self.id.replica,
            counter: self.id.counter,
        }
    }

    /// The per-replica monotonicity tuple `(physical, logical, counter)`
    /// (Chapter 6 §6.6). Compared only between two envelopes of the *same*
    /// replica, so the replica field is dropped.
    #[inline]
    pub fn monotonicity_tuple(&self) -> (WallClockTime, u32, u64) {
        (
            self.hlc.physical_time,
            self.hlc.logical_counter,
            self.id.counter,
        )
    }
}

impl CanonicalEncode for OperationStamp {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.hlc);
        push_canon(out, &self.id);
    }
}

/// The canonical reduction tuple of a stamp: `(physical_time, logical_counter,
/// replica, counter)`, ordered lexicographically ascending (Chapter 6 §6.3.3).
///
/// The field declaration order *is* the comparison order, so the derived `Ord`
/// is exactly the spec's lexicographic order. [`ReplicaId`]'s numeric `Ord`
/// equals the big-endian byte order the spec names ("lexicographic on the
/// replica identifier's canonical byte form"), because the replica is the high
/// bits of an unsigned integer.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StampTuple {
    /// Physical time, ascending.
    pub physical_time: WallClockTime,
    /// Logical counter, ascending.
    pub logical_counter: u32,
    /// Authoring replica, ascending (= big-endian byte order).
    pub replica: ReplicaId,
    /// Authoring counter, ascending.
    pub counter: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(p: i64, l: u32, r: u64, c: u64) -> OperationStamp {
        OperationStamp::new(
            HybridLogicalClock::new(WallClockTime(p), l),
            OperationId::new(ReplicaId(r), c),
        )
    }

    #[test]
    fn reduction_tuple_is_lexicographic_physical_logical_replica_counter() {
        // physical dominates everything
        assert!(stamp(1, 9, 9, 9).reduction_tuple() < stamp(2, 0, 0, 0).reduction_tuple());
        // then logical
        assert!(stamp(5, 1, 9, 9).reduction_tuple() < stamp(5, 2, 0, 0).reduction_tuple());
        // then replica
        assert!(stamp(5, 5, 1, 9).reduction_tuple() < stamp(5, 5, 2, 0).reduction_tuple());
        // then counter
        assert!(stamp(5, 5, 5, 1).reduction_tuple() < stamp(5, 5, 5, 2).reduction_tuple());
    }

    #[test]
    fn monotonicity_tuple_drops_the_replica() {
        let s = stamp(7, 3, 42, 11);
        assert_eq!(s.monotonicity_tuple(), (WallClockTime(7), 3, 11));
    }

    #[test]
    fn stamp_encode_is_stable() {
        let s = stamp(123, 4, 5, 6);
        let mut a = Vec::new();
        s.encode_canonical(&mut a);
        let mut b = Vec::new();
        s.encode_canonical(&mut b);
        assert_eq!(a, b);
        // physical(8) + logical(4) + id(16) = 28 bytes.
        assert_eq!(a.len(), 28);
    }
}
