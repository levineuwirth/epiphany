//! The six v0 acceptance criteria (QUICKSTART §"v0 acceptance criteria"),
//! driven only through the testkit's public surface. Each test corresponds to
//! one architecture layer; if any fails, that layer is not done. The heavy soak
//! versions (1M / 10k iterations) live in `examples/conformance_suite.rs`; these
//! run a meaningful slice under the `cargo test` timeout.
//!
//! All six criteria run against the real shipped crates (A/B/C/D/E): criterion
//! 6 drives the real `epiphany-layout-ir` through the `layout_stub` harness
//! module (Agent E has landed). See the crate docs for the harness policy.

use epiphany_testkit::{
    bundle_harness, convergence, equivocation, fixtures, generators, layout_stub, negative,
    roundtrip, Rng,
};

/// Criterion 1 — **Convergence (real Score).** Overlapping edits to a real
/// ~50-bar, two-voice base [`epiphany_core::Score`] by two replicas converge to
/// an *identical materialized Score* — the real graph (arena, voices,
/// tombstones, cross-cutting) together with its bookkeeping state — regardless
/// of envelope delivery order, with every Chapter 5 invariant intact. Driven
/// through Agent C's `OperationSet::reduce_onto`. This is the headline criterion;
/// [`reducer_bookkeeping_convergence`] is its ledger-projection counterpart.
#[test]
fn criterion_1_convergence() {
    for seed in 0..24u64 {
        convergence::run_graph_convergence(6, seed.wrapping_mul(0x9E37_79B9).wrapping_add(11));
    }
}

/// **Reducer-bookkeeping convergence** (the former, weaker half of criterion 1,
/// retained and honestly renamed). The canonical Chapter 6 §6.3 *bookkeeping
/// projection* — `OperationSet::reduce` → `MaterializedState::canonical_bytes` —
/// is byte-identical across delivery orders. That projection is the ledger
/// (effects, conflicts, anomalies, tombstones, spellings, pending), **not** the
/// full musical graph; real-Score convergence is `criterion_1_convergence`.
/// Kept as a fast byte-level determinism gate (and the basis of criterion 5).
#[test]
fn reducer_bookkeeping_convergence() {
    // The named shape: a two-staff, overlapping-edit session (bookkeeping bytes).
    for seed in 0..16u64 {
        convergence::run_two_staff_convergence(8, seed.wrapping_mul(0x9E37_79B9).wrapping_add(11));
    }
    // Plus a broader sweep of smaller randomized sessions.
    for seed in 0..200u64 {
        convergence::run_convergence(24, 6, seed.wrapping_mul(0x9E37_79B9));
    }
}

/// **Audit regression guards.** Every defect the Agent C framework audit
/// surfaced (the M1 fixes) is independently re-asserted here so a regression in
/// `epiphany-ops` trips Agent F's suite directly. See [`negative`].
#[test]
fn audit_defect_regressions() {
    negative::run_all();
}

/// Criterion 2 — **Crash safety.** A crash between any two syscalls in the
/// commit path leaves the bundle openable — possibly at the previous
/// generation, never corrupt (Chapter 8's atomic commit). Runs the testkit's
/// randomized driver, an exhaustive per-syscall sweep, and Agent D's own gate.
#[test]
fn criterion_2_crash_safety() {
    bundle_harness::run_crash_recovery(2_000, 0xF00D_BEEF_1234_5678);

    let mut rng = Rng::new(0x00A1_1CE5);
    for base_commits in 0..3u64 {
        let (image, generation) = bundle_harness::build_base(&mut rng, base_commits);
        bundle_harness::exhaustive_crash_sweep(
            &image,
            generation,
            &[b"alpha".to_vec(), vec![9u8; 300]],
        );
    }

    bundle_harness::bundle_crash_recovery_fuzz(2_000, 0x0123_4567_89AB_CDEF);
}

/// Criterion 3 — **Equivocation.** An injected duplicate `OperationId` with
/// different canonical bytes produces an `OperationSlot::Equivocated` at both
/// replicas, regardless of arrival order (Pass 10's order-independence fix).
#[test]
fn criterion_3_equivocation() {
    for seed in 0..300u64 {
        equivocation::run_equivocation(16, seed.wrapping_mul(0x9E37_79B9).wrapping_add(3));
    }
    // Agent C's authoritative equivocation gate (a slice).
    equivocation::ops_equivocation_fuzz(2_000, 0x1234_5678);
}

/// Criterion 4 — **Canonical serialization stability (typed + container).** The
/// same canonical value serialized → loaded → re-serialized produces
/// byte-identical bytes (Appendix D's canonical-serialization layer): the
/// type-level round-trip corpus over every `CanonicalEncode` type in Agents A
/// and B, Agent A's determinism gate (a slice), and the real bundle
/// manifest/header — including decoder rejection of corruption.
///
/// The **full-Score** byte round-trip is split out below: its bookkeeping
/// projection ([`reducer_bookkeeping_serialization`]) and its reproducibility
/// ([`full_score_materialization_is_reproducible`]) are exercised now; the
/// whole-`Score` byte codec is pending item 5 (Agent B) —
/// [`criterion_4_full_score_byte_roundtrip`].
#[test]
fn criterion_4_canonical_serialization_stability() {
    roundtrip::run_roundtrip_corpus(100_000, 0x00C0_FFEE_1234_5678);
    // A slice of Agent A's 1,000,000-iteration determinism gate (the full run
    // lives in the conformance suite).
    roundtrip::run_determinism_roundtrip_gate(200_000, 0x0A11_CE5E_EDED_2024);

    for seed in 0..48u64 {
        roundtrip::assert_manifest_roundtrip(&roundtrip::committed_manifest(
            seed.wrapping_mul(0x9E37_79B9).wrapping_add(5),
        ));
        let mut rng = Rng::new(seed.wrapping_mul(0x0100_0193).wrapping_add(17));
        let rich = generators::rich_manifest(&mut rng);
        roundtrip::assert_manifest_roundtrip(&rich);
        // The decoder actually validates: corruption is rejected.
        roundtrip::assert_manifest_decode_rejects_corruption(&rich);
    }
}

/// **Reducer-bookkeeping serialization** (the former, narrower half of criterion
/// 4, retained and honestly renamed). The canonical *bookkeeping projection*
/// (`MaterializedState::canonical_bytes`) survives content-addressed storage in
/// a real bundle and re-serializes byte-identically, and is musically sensitive
/// (same identities + changed content → different bytes). This is the Chapter 6
/// ledger, **not** the whole musical `Score`; the full-Score byte round-trip is
/// [`criterion_4_full_score_byte_roundtrip`] (pending item 5).
#[test]
fn reducer_bookkeeping_serialization() {
    for seed in 0..48u64 {
        let mut rng = Rng::new(seed.wrapping_mul(0x0100_0193).wrapping_add(17));
        // The reduced canonical state survives content-addressed storage, and is
        // musically sensitive: same identities + changed content → different bytes.
        let session = generators::operation_envelopes(&mut rng, 40, 3, 6, 6);
        roundtrip::assert_reduction_serialization_stable(&session, seed);
        let other = generators::operation_envelopes(&mut rng, 41, 3, 6, 6);
        roundtrip::assert_distinct_scores_serialize_differently(&session, &other);
    }
    // Strong sensitivity: same identities, changed content -> different bytes.
    roundtrip::assert_content_mutation_changes_serialization();
}

/// **Full-Score materialization reproducibility** (achievable without the byte
/// codec). Reducing the same edit session onto the same base `Score` twice —
/// once in authored order, once shuffled — yields the *identical* materialized
/// `epiphany_core::Score` and bookkeeping state. This is the determinism
/// precondition any future whole-Score byte codec depends on, asserted today via
/// structural equality of the real graph (`reduce_onto`).
#[test]
fn full_score_materialization_is_reproducible() {
    for seed in 0..16u64 {
        convergence::run_graph_convergence(4, seed.wrapping_mul(0x0100_0193).wrapping_add(23));
    }
}

/// Criterion 4 (full-Score byte round-trip) — **pending item 5 (Agent B).** A
/// whole-`epiphany_core::Score` / `GraphMaterialization` `encode → decode →
/// re-encode` byte round-trip requires the whole-score canonical codec
/// (`CanonicalEncode`/`CanonicalDecode for Score`), which does not exist yet:
/// today only the bookkeeping `MaterializedState` and the A/B typed values have
/// codecs. This gate is intentionally `#[ignore]`'d (visible as *ignored*, never
/// falsely green) until item 5 lands the codec; then drop the attribute and
/// assert the real byte cycle through a bundle snapshot.
#[test]
#[ignore = "pending item 5 (Agent B): whole-score codec (CanonicalEncode/Decode for Score) does not exist yet"]
fn criterion_4_full_score_byte_roundtrip() {
    unimplemented!(
        "blocked on item 5: epiphany_core::Score has no canonical byte codec. \
         When it lands, reduce_onto a base, encode the Score, decode, and assert \
         a byte-identical re-encode through a real bundle snapshot."
    );
}

/// Criterion 5 — **Reduction determinism.** A randomized 1,000-envelope set,
/// reduced 10 times in 10 different orders, produces byte-identical materialized
/// states *and* an identical canonical reduction order (Appendix D's
/// canonical-reduction layer). Drives the real reducer.
#[test]
fn criterion_5_reduction_determinism() {
    let mut rng = Rng::new(0x5EED_0005_0F0F_0F0F);
    let envelopes = generators::operation_envelopes(&mut rng, 1_000, 3, 40, 40);
    // The big set honors causal order and reduces deterministically.
    convergence::assert_causal_order_respected(&envelopes);
    let order = convergence::assert_reduction_determinism(&envelopes, 10, &mut rng);
    assert!(!order.is_empty());
    // The testkit's authoritative gate over many conformant sets (proves
    // causal-order correctness, not just permutation invariance).
    convergence::run_authoritative_reduction_gate(1_500, 3, 0x00C0_FFEE_0042);
    // Agent C's own hand-off gate, including conformant causal histories and
    // explicit anomaly injections.
    convergence::ops_reduction_determinism_fuzz(2_000, 0x00C0_FFEE);
}

/// Criterion 6 — **Layout round-trip.** A score graph → LogicalLayoutIR →
/// stub-solved ResolvedLayoutIR → RenderIR interface call completes without
/// panic and without losing provenance back-references (Chapter 7's IR
/// contract). Driven on the 10-measure single-staff hand-off fixture and the
/// rich multi-region generator.
#[test]
fn criterion_6_layout_round_trip() {
    for seed in 0..128u64 {
        let report = layout_stub::round_trip(&fixtures::ten_measure_single_staff(seed));
        assert!(report.glyphs > 0);
        assert_eq!(report.glyphs, report.render_primitives);

        layout_stub::round_trip(&generators::graph::valid_score_rich(seed));
    }
}

/// The manifest-selection harness (QUICKSTART, Agent F): every corruption
/// scenario plus the commit-protocol selection check.
#[test]
fn manifest_selection_harness() {
    for seed in 0..16u64 {
        bundle_harness::run_manifest_selection(seed);
    }
}
