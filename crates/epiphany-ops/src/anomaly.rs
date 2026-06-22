//! Replica-stream anomalies and the integrity-anomaly register (Chapter 6
//! §"Per-Replica HLC Monotonicity"; Chapter 5 §"System-Derived Counter
//! Collisions"; Pass 10).
//!
//! Two kinds of structural failure are modeled here, both kept rigorously
//! *separate* from ordinary [`ConflictRecord`](crate::ConflictRecord)s: a
//! conflict is a canonical-state fact (two valid edits collide and the user
//! resolves it); an [`IntegrityAnomaly`] means the structural assumptions
//! underlying canonical state have failed and the document needs external
//! recovery (Pass 10 moved these out of `ConflictKind`).
//!
//! * **HLC monotonicity** (Chapter 6 §6.6) is a property of the *set* of
//!   accepted envelopes, not of arrival order. For two envelopes of one replica
//!   with counters `c1 < c2`, the monotonicity tuple `(physical, logical,
//!   counter)` of `c1` MUST be ≤ that of `c2`. A violation forms an
//!   [`AnomalousReplicaSegment`]: every envelope from that replica with counter
//!   `≥ first_bad_counter` is *excluded from canonical reduction* (retained, but
//!   not reduced), and the document carries a `ReplicaStreamQuarantined`
//!   anomaly. Excluding the segment — rather than reducing-but-marking-tainted —
//!   keeps canonical state derived only from monotone envelopes.
//! * **System-derived counter collisions** (Chapter 5) and **slot
//!   equivocation** (Chapter 6 §6.5) are the other two anomaly kinds.

use std::collections::BTreeMap;

use epiphany_core::{derive_system_id, IntegrityAnomalyId, OperationId, ReplicaId};
use epiphany_determinism::{CanonicalEncode, SystemDomainTag};

use crate::encode::{push_canon, push_seq, push_tag, push_u64};
use crate::envelope::OperationEnvelope;
use crate::support::{
    IntegrityAnomalyRegistryId, ObjectKind, ReplicaAnomalyRegistryId, SerializedCanonicalInputs,
};

/// The reserved built-in system domain tag (`MUSCSANM`) under which
/// integrity-anomaly identifiers are content-derived, so two replicas reducing
/// the same operation set mint byte-identical anomaly ids. Ratified by Pass 11
/// (item 1.4): `MUSCSANM` is a built-in reserved tag alongside `MUSCSVCE` /
/// `MUSCSPCH` (Chapter 5 §"System-Derived Counter Collisions",
/// Requirement `req:graph:integrity-anomaly-id`), not an extension tag —
/// anomalies are core.
fn anomaly_domain_tag() -> SystemDomainTag {
    SystemDomainTag::ANOMALY
}

/// A range of envelopes from a single replica identified as
/// monotonicity-violating and excluded from ordinary canonical reduction
/// (Chapter 6 §6.6). Retained for diagnostic and recovery purposes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AnomalousReplicaSegment {
    /// The offending replica.
    pub replica: ReplicaId,
    /// The smallest counter at or after which the replica's stream is
    /// anomalous. Every envelope from this replica with counter
    /// `>= first_bad_counter` is excluded from canonical reduction.
    pub first_bad_counter: u64,
    /// The detected anomaly reason.
    pub reason: ReplicaAnomalyReason,
    /// Operation ids in the excluded segment, sorted by counter.
    pub excluded: Vec<OperationId>,
}

impl CanonicalEncode for AnomalousReplicaSegment {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.replica.to_be_bytes());
        push_u64(out, self.first_bad_counter);
        self.reason.encode_canonical(out);
        push_seq(out, &self.excluded);
    }
}

/// Why a replica's stream was flagged anomalous (Chapter 6 §6.6).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ReplicaAnomalyReason {
    /// The replica produced an envelope at counter `c2` with a stamp tuple
    /// strictly less than a previously-known envelope at counter `c1 < c2`.
    HlcMonotonicityViolation {
        violating_pair: (OperationId, OperationId),
    },
    /// A registered anomaly reason defined by an extension or transport.
    Registered(ReplicaAnomalyRegistryId),
}

impl ReplicaAnomalyReason {
    fn discriminant(&self) -> u8 {
        match self {
            ReplicaAnomalyReason::HlcMonotonicityViolation { .. } => 0,
            ReplicaAnomalyReason::Registered(_) => 1,
        }
    }
}

impl CanonicalEncode for ReplicaAnomalyReason {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            ReplicaAnomalyReason::HlcMonotonicityViolation {
                violating_pair: (a, b),
            } => {
                push_canon(out, a);
                push_canon(out, b);
            }
            ReplicaAnomalyReason::Registered(id) => push_canon(out, id),
        }
    }
}

/// An integrity anomaly that takes the document out of ordinary canonical
/// operation (Chapter 5 §"System-Derived Counter Collisions"; Chapter 6 §6.6).
/// Distinct from [`ConflictRecord`](crate::ConflictRecord).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IntegrityAnomaly {
    /// Content-derived identifier (see [`IntegrityAnomaly::new`]).
    pub id: IntegrityAnomalyId,
    /// The anomaly kind.
    pub kind: IntegrityAnomalyKind,
}

impl IntegrityAnomaly {
    /// Builds an anomaly, content-deriving its [`IntegrityAnomalyId`] from the
    /// kind's canonical bytes in the `SYSTEM_DERIVED` namespace, so the
    /// register is itself a deterministic materialized fact.
    pub fn new(kind: IntegrityAnomalyKind) -> Self {
        let id: IntegrityAnomalyId =
            derive_system_id(anomaly_domain_tag(), &kind.to_canonical_bytes());
        IntegrityAnomaly { id, kind }
    }
}

impl CanonicalEncode for IntegrityAnomaly {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.id);
        self.kind.encode_canonical(out);
    }
}

/// The kind of integrity anomaly (Chapter 5 §"System-Derived Counter
/// Collisions"; Pass 10).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IntegrityAnomalyKind {
    /// A system-derived identifier counter collision: two different canonical
    /// input sets derived the same 64-bit counter within one object kind.
    SystemIdentifierCollision {
        kind: ObjectKind,
        colliding_counter: u64,
        input_set_a: SerializedCanonicalInputs,
        input_set_b: SerializedCanonicalInputs,
    },
    /// An `OperationSlot` is equivocated; reduction has excluded the operation.
    OperationSlotEquivocated { operation_id: OperationId },
    /// A replica's stream has been quarantined per the HLC monotonicity rule.
    ReplicaStreamQuarantined {
        replica: ReplicaId,
        first_bad_counter: u64,
    },
    /// A registered anomaly defined by an extension or transport.
    Registered(IntegrityAnomalyRegistryId),
}

impl IntegrityAnomalyKind {
    fn discriminant(&self) -> u8 {
        match self {
            IntegrityAnomalyKind::SystemIdentifierCollision { .. } => 0,
            IntegrityAnomalyKind::OperationSlotEquivocated { .. } => 1,
            IntegrityAnomalyKind::ReplicaStreamQuarantined { .. } => 2,
            IntegrityAnomalyKind::Registered(_) => 3,
        }
    }
}

impl CanonicalEncode for IntegrityAnomalyKind {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            IntegrityAnomalyKind::SystemIdentifierCollision {
                kind,
                colliding_counter,
                input_set_a,
                input_set_b,
            } => {
                kind.encode_canonical(out);
                push_u64(out, *colliding_counter);
                // Order the two input sets canonically so the anomaly's identity
                // does not depend on which collided input was discovered first.
                let (lo, hi) = if input_set_a.0 <= input_set_b.0 {
                    (input_set_a, input_set_b)
                } else {
                    (input_set_b, input_set_a)
                };
                lo.encode_canonical(out);
                hi.encode_canonical(out);
            }
            IntegrityAnomalyKind::OperationSlotEquivocated { operation_id } => {
                push_canon(out, operation_id)
            }
            IntegrityAnomalyKind::ReplicaStreamQuarantined {
                replica,
                first_bad_counter,
            } => {
                out.extend_from_slice(&replica.to_be_bytes());
                push_u64(out, *first_bad_counter);
            }
            IntegrityAnomalyKind::Registered(id) => push_canon(out, id),
        }
    }
}

/// Detects per-replica HLC monotonicity anomalies over a set of reducible
/// envelopes (Chapter 6 §6.6).
///
/// The check is order-independent: it groups the envelopes by authoring
/// replica, walks each replica's envelopes in ascending counter order, and
/// finds every pair `c1 < c2` whose monotonicity tuples decrease. The resulting
/// segment starts at the smallest `c1` participating in any violating pair, and
/// every envelope of that replica with counter `>= c1` is excluded.
///
/// Returns one [`AnomalousReplicaSegment`] per offending replica, in ascending
/// replica order.
pub fn detect_replica_anomalies(envelopes: &[&OperationEnvelope]) -> Vec<AnomalousReplicaSegment> {
    // (counter, op id, monotonicity tuple) for one of a replica's envelopes.
    type ReplicaEntry = (u64, OperationId, (i64, u32, u64));
    // Group by replica, collecting each envelope's entry.
    let mut by_replica: BTreeMap<ReplicaId, Vec<ReplicaEntry>> = BTreeMap::new();
    for env in envelopes {
        let t = env.stamp.monotonicity_tuple();
        by_replica.entry(env.id.replica).or_default().push((
            env.id.counter,
            env.id,
            (t.0 .0, t.1, t.2),
        ));
    }

    let mut segments = Vec::new();
    for (replica, mut ops) in by_replica {
        ops.sort_by_key(|(counter, _, _)| *counter);
        // Find the earliest counter participating as the left side of any
        // violating pair. Comparing only against the preceding maximum is not
        // enough: [100, 200, 50] also makes (counter 0, counter 2) a violating
        // pair, so quarantine must begin at counter 0 rather than counter 1.
        // Suffix minima make this linear after the counter sort.
        let mut suffix_min = vec![(i64::MAX, u32::MAX, u64::MAX); ops.len()];
        if let Some(last) = ops.last() {
            suffix_min[ops.len() - 1] = last.2;
            for index in (0..ops.len() - 1).rev() {
                suffix_min[index] = ops[index].2.min(suffix_min[index + 1]);
            }
        }
        let first_bad = (0..ops.len().saturating_sub(1)).find_map(|earlier| {
            if ops[earlier].2 <= suffix_min[earlier + 1] {
                return None;
            }
            let later = (earlier + 1..ops.len())
                .find(|&later| ops[earlier].2 > ops[later].2)
                .expect("suffix minimum proves that a violating successor exists");
            Some((ops[earlier].0, ops[earlier].1, ops[later].1))
        });

        if let Some((first_bad_counter, c1_op, c2_op)) = first_bad {
            let excluded: Vec<OperationId> = ops
                .iter()
                .filter(|(counter, _, _)| *counter >= first_bad_counter)
                .map(|(_, op, _)| *op)
                .collect();
            segments.push(AnomalousReplicaSegment {
                replica,
                first_bad_counter,
                reason: ReplicaAnomalyReason::HlcMonotonicityViolation {
                    violating_pair: (c1_op, c2_op),
                },
                excluded,
            });
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::causal::CausalContext;
    use crate::payload::{OperationKind, RespellPitchOp};
    use crate::stamp::{HybridLogicalClock, OperationStamp};
    use crate::support::AuthorId;
    use crate::OperationPayload;
    use epiphany_core::{PitchId, WallClockTime};
    use epiphany_determinism::ContentHash;

    fn env(replica: u64, counter: u64, physical: i64, logical: u32) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(replica), counter);
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(physical), logical),
                id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                pitch: PitchId::new(ReplicaId(replica), counter),
                spelling: ContentHash([1u8; 32]),
            })),
        }
    }

    #[test]
    fn integrity_anomaly_id_byte_form_is_locked() {
        // Golden: locks the MUSCSANM-derived anomaly id for a fixed kind.
        // RATIFIED by Pass 11 (item 1.4, req:graph:integrity-anomaly-id): the
        // identity is content-derived so two replicas observing the same failure
        // agree on it, which is a conformance property. A change to the domain
        // tag, the kind's canonical encoding, or the derivation breaks this
        // deliberately.
        let kind = IntegrityAnomalyKind::OperationSlotEquivocated {
            operation_id: OperationId::new(ReplicaId(7), 42),
        };
        let id = IntegrityAnomaly::new(kind).id;
        // First 8 bytes are the SYSTEM_DERIVED replica; the id is in that namespace.
        assert_eq!(
            &id.canonical_bytes()[0..8],
            &ReplicaId::SYSTEM_DERIVED.to_be_bytes()
        );
        const GOLDEN: [u8; 16] = [
            255, 255, 255, 255, 255, 255, 255, 255, 81, 178, 18, 20, 252, 222, 201, 215,
        ];
        assert_eq!(id.canonical_bytes(), GOLDEN);
    }

    #[test]
    fn monotone_stream_has_no_anomaly() {
        let a = env(1, 0, 10, 0);
        let b = env(1, 1, 10, 1);
        let c = env(1, 2, 20, 0);
        let refs = [&a, &b, &c];
        assert!(detect_replica_anomalies(&refs).is_empty());
    }

    #[test]
    fn non_monotone_stream_quarantines_from_first_bad_counter() {
        // counter 0: physical 50; counter 1: physical 10 (< 50) -> violation.
        let a = env(1, 0, 50, 0);
        let b = env(1, 1, 10, 0);
        let refs = [&a, &b];
        let seg = detect_replica_anomalies(&refs);
        assert_eq!(seg.len(), 1);
        assert_eq!(seg[0].replica, ReplicaId(1));
        // first_bad is the earlier counter of the pair (0).
        assert_eq!(seg[0].first_bad_counter, 0);
        assert_eq!(seg[0].excluded.len(), 2);
    }

    #[test]
    fn detection_is_arrival_order_independent() {
        let a = env(1, 0, 50, 0);
        let b = env(1, 1, 10, 0);
        let forward = detect_replica_anomalies(&[&a, &b]);
        let backward = detect_replica_anomalies(&[&b, &a]);
        assert_eq!(forward, backward);
    }

    #[test]
    fn quarantine_starts_at_earliest_counter_in_any_violating_pair() {
        // Counter 2 is below both earlier stamps. The first violating pair by
        // counter is (0, 2), even though counter 1 carries the maximum stamp.
        let a = env(1, 0, 100, 0);
        let b = env(1, 1, 200, 0);
        let c = env(1, 2, 50, 0);
        let seg = detect_replica_anomalies(&[&a, &b, &c]);
        assert_eq!(seg.len(), 1);
        assert_eq!(seg[0].first_bad_counter, 0);
        assert_eq!(seg[0].excluded, vec![a.id, b.id, c.id]);
        assert_eq!(
            seg[0].reason,
            ReplicaAnomalyReason::HlcMonotonicityViolation {
                violating_pair: (a.id, c.id),
            }
        );
    }
}
