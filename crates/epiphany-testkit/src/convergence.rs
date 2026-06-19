//! The CRDT convergence harness (QUICKSTART, Agent F): *"apply same envelope
//! set in N random orders, assert byte-identical materialized state."* This is
//! v0 acceptance criterion 1 (Convergence) and criterion 5 (Reduction
//! determinism) — the determinism heart of Chapter 6.
//!
//! It drives the **real** [`epiphany_ops`] crate (Agent C has shipped): an
//! [`OperationSet`] accepts the envelopes, [`OperationSet::reduce`] materializes
//! them, and the materialized state's `canonical_bytes` is the artifact compared
//! across delivery orders. Because the canonical reduction order
//! ([`epiphany_ops::canonical_reduction_order`]) is a function of the operation
//! *set* and not of delivery order, every delivery permutation must materialize
//! byte-identically. The negative control in the tests proves the harness is not
//! vacuous: a reducer that consumed *arrival* order instead would diverge, and
//! this harness would catch it.

use epiphany_core::OperationId;
use epiphany_ops::{canonical_reduction_order, OperationEnvelope, OperationSet};

use crate::rng::Rng;

/// Agent C's authoritative reduction-determinism gate (10,000 randomized sets;
/// criteria 1 and 5 at scale), re-exported as the suite's single entry point.
pub use epiphany_ops::fuzz::run_reduction_determinism_fuzz as ops_reduction_determinism_fuzz;

/// Accepts `envelopes` in the given order into a fresh [`OperationSet`] and
/// returns the canonical materialized bytes.
fn materialize_in_order(envelopes: &[OperationEnvelope]) -> Vec<u8> {
    let mut set = OperationSet::new();
    set.accept_all(envelopes.iter().cloned());
    set.reduce().canonical_bytes()
}

/// The canonical reduction order (as `OperationId`s) of the accepted singles.
fn reduction_order_of(envelopes: &[OperationEnvelope]) -> Vec<OperationId> {
    let mut set = OperationSet::new();
    set.accept_all(envelopes.iter().cloned());
    let singles = set.single_envelopes();
    canonical_reduction_order(&singles)
        .into_iter()
        .map(|e| e.id)
        .collect()
}

/// Asserts that `envelopes` materialize to **byte-identical** state under
/// `orders` independent random delivery permutations (acceptance criterion 1).
/// Panics, with the diverging permutation index, on the first mismatch.
pub fn assert_convergence(envelopes: &[OperationEnvelope], orders: usize, rng: &mut Rng) {
    let reference = materialize_in_order(envelopes);
    for k in 0..orders {
        let perm = rng.permutation(envelopes.len());
        let shuffled: Vec<OperationEnvelope> = perm.iter().map(|&i| envelopes[i].clone()).collect();
        let got = materialize_in_order(&shuffled);
        assert_eq!(
            reference, got,
            "delivery permutation #{k} changed the materialized state \
             (reduction is not order-independent)"
        );
    }
}

/// Asserts the stronger §6.7 property used by acceptance criterion 5: both the
/// **materialized state** and the **canonical reduction order itself** are
/// identical across `orders` delivery permutations. Returns that canonical order.
pub fn assert_reduction_determinism(
    envelopes: &[OperationEnvelope],
    orders: usize,
    rng: &mut Rng,
) -> Vec<OperationId> {
    let reference_order = reduction_order_of(envelopes);
    let reference_state = materialize_in_order(envelopes);
    for k in 0..orders {
        let perm = rng.permutation(envelopes.len());
        let shuffled: Vec<OperationEnvelope> = perm.iter().map(|&i| envelopes[i].clone()).collect();
        assert_eq!(
            reference_order,
            reduction_order_of(&shuffled),
            "delivery permutation #{k} changed the canonical reduction order"
        );
        assert_eq!(
            reference_state,
            materialize_in_order(&shuffled),
            "delivery permutation #{k} changed the materialized state"
        );
    }
    reference_order
}

/// Asserts the histories actually honor the **causal-first** order, not merely
/// permutation invariance (spec §"Identifiers": every causal predecessor's stamp
/// must be strictly less). For each operation `B` and each operation `A` in
/// `B`'s causal context that is present in the set, asserts `A`'s stamp is
/// strictly less than `B`'s *and* `A` precedes `B` in the canonical reduction
/// order. This proves the order is causal-first, which sorting-by-HLC delivers
/// only when histories are authored conformantly.
pub fn assert_causal_order_respected(envelopes: &[OperationEnvelope]) {
    let mut set = OperationSet::new();
    set.accept_all(envelopes.iter().cloned());
    let singles = set.single_envelopes();
    let ordered = canonical_reduction_order(&singles);
    let pos: std::collections::BTreeMap<OperationId, usize> =
        ordered.iter().enumerate().map(|(i, e)| (e.id, i)).collect();

    for b in &ordered {
        for a in &ordered {
            if a.id != b.id && b.causal_context.covers(a.id) {
                assert!(
                    a.stamp.reduction_tuple() < b.stamp.reduction_tuple(),
                    "authoring rule violated: causal predecessor {:?} stamp is not strictly \
                     less than successor {:?}",
                    a.id,
                    b.id
                );
                assert!(
                    pos[&a.id] < pos[&b.id],
                    "causal predecessor {:?} does not precede successor {:?} in the reduction order",
                    a.id,
                    b.id
                );
            }
        }
    }
}

/// A self-contained driver: generate `n_ops` envelopes from `seed`, assert the
/// histories honor causal order, and assert convergence across `orders`
/// permutations. Deterministic.
pub fn run_convergence(n_ops: usize, orders: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    let envelopes = crate::generators::operation_envelopes(&mut rng, n_ops, 3, 6, 6);
    assert_causal_order_respected(&envelopes);
    assert_convergence(&envelopes, orders, &mut rng);
}

/// The v0 criterion-1 scenario: overlapping edits to a two-staff score by two
/// replicas. Asserts both staves are actually populated in the materialized
/// result, that the histories honor causal order, and that they converge to
/// byte-identical materialized state across `orders` delivery permutations.
pub fn run_two_staff_convergence(orders: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    let envelopes = crate::generators::two_staff_edit_session(&mut rng);
    assert_causal_order_respected(&envelopes);
    crate::generators::assert_two_staff_populated(&envelopes);
    assert_convergence(&envelopes, orders, &mut rng);
}

/// **The authoritative criterion-1/5 gate.** Generates `sets` randomized
/// *conformant* operation sets (the testkit's generator honors the HLC authoring
/// rule) and asserts, for each: (a) causal-first order is genuinely respected
/// ([`assert_causal_order_respected`]), and (b) reduction is byte-identical and
/// order-identical across `orders` delivery permutations
/// ([`assert_reduction_determinism`]).
///
/// This is the testkit's own gate and the one the suite treats as authoritative
/// for causal-order correctness. Agent C's re-exported
/// [`ops_reduction_determinism_fuzz`] is run in addition; its baseline histories
/// now use the same causal HLC authoring rule, while its explicit anomaly
/// injections continue to exercise quarantine behavior.
pub fn run_authoritative_reduction_gate(sets: usize, orders: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    for _ in 0..sets {
        let n = 1 + rng.below(30) as usize;
        let envelopes = crate::generators::operation_envelopes(&mut rng, n, 3, 8, 8);
        assert_causal_order_respected(&envelopes);
        assert_reduction_determinism(&envelopes, orders, &mut rng);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{PitchId, ReplicaId, WallClockTime};
    use epiphany_determinism::ContentHash;
    use epiphany_ops::{
        AuthorId, CausalContext, HybridLogicalClock, OperationKind, OperationPayload,
        OperationStamp, RespellPitchOp,
    };
    use std::collections::BTreeMap;

    #[test]
    fn small_sets_converge() {
        for seed in 0..200u64 {
            run_convergence(16, 8, seed.wrapping_mul(0x9E37_79B9));
        }
    }

    #[test]
    fn two_staff_session_converges() {
        for seed in 0..16u64 {
            run_two_staff_convergence(8, seed.wrapping_mul(0x9E37_79B9).wrapping_add(1));
        }
    }

    #[test]
    fn empty_and_singleton_sets_are_trivially_stable() {
        let mut rng = Rng::new(1);
        assert_convergence(&[], 4, &mut rng);
        let one = crate::generators::operation_envelopes(&mut rng, 1, 2, 6, 6);
        assert_convergence(&one, 4, &mut rng);
    }

    // --- Negative control: prove the harness is not vacuous. ---------------
    //
    // Two concurrent RespellPitch operations on the *same* pitch with different
    // spellings. A reducer that consumed *arrival* order would land on whichever
    // arrived last — so two delivery orders diverge. The real (canonical) reducer
    // lands on the same spelling regardless. The convergence harness compares the
    // canonical result, so it would FAIL for the arrival-order reducer; this test
    // demonstrates the discriminating power directly.

    fn respell(replica: u64, counter: u64, phys: i64, spelling: u8) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(replica), counter);
        OperationEnvelope {
            id,
            author: AuthorId(replica as u128),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(phys), 0), id),
            // No causal link → the two operations are concurrent.
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                pitch: PitchId::new(ReplicaId(0x0B7E_C700), 0),
                spelling: ContentHash([spelling; 32]),
            })),
        }
    }

    /// A deliberately broken, order-*dependent* reducer: last spelling wins by
    /// arrival order. Stands in for the bug the harness must catch.
    fn naive_arrival_order_spelling(envs: &[OperationEnvelope]) -> Option<[u8; 32]> {
        let mut last: BTreeMap<PitchId, [u8; 32]> = BTreeMap::new();
        for e in envs {
            if let OperationPayload::Primitive(OperationKind::RespellPitch(op)) = &e.payload {
                last.insert(op.pitch, op.spelling.0);
            }
        }
        last.values().next().copied()
    }

    #[test]
    fn negative_control_arrival_order_reducer_would_diverge() {
        let a = respell(1, 0, 10, 0xAA);
        let b = respell(2, 0, 20, 0xBB);

        // Arrival order A,B vs B,A: the broken arrival-order reducer diverges...
        let naive_ab = naive_arrival_order_spelling(&[a.clone(), b.clone()]);
        let naive_ba = naive_arrival_order_spelling(&[b.clone(), a.clone()]);
        assert_ne!(
            naive_ab, naive_ba,
            "negative control is mis-constructed: an arrival-order reducer must diverge here"
        );

        // ...while the real canonical reducer converges (the harness passes).
        let canon_ab = materialize_in_order(&[a.clone(), b.clone()]);
        let canon_ba = materialize_in_order(&[b, a]);
        assert_eq!(
            canon_ab, canon_ba,
            "the canonical reducer must converge regardless of arrival order"
        );
    }
}
