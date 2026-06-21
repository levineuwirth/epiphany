//! Drives the whole conformance suite at scale from the command line, so the
//! heavy gates can run outside the unit-test timeout — the analogue of
//! `epiphany-determinism`'s `fuzz_roundtrip` and `epiphany-bundle`'s
//! `fuzz_crash`. Every gate is deterministic, so any failure reproduces from
//! its seed.
//!
//! Usage:
//!   cargo run --release --example conformance_suite [SCALE]
//!
//! `SCALE` (default 1) multiplies the iteration counts. `SCALE=10` is a soak
//! run; `SCALE=0` runs a fast smoke pass. Exits non-zero (via panic) on the
//! first violation.

use epiphany_testkit::{
    bundle_harness, convergence, equivocation, fixtures, generators, layout_stub, negative,
    roundtrip, Rng,
};

fn main() {
    let scale: u64 = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("SCALE must be an integer"))
        .unwrap_or(1);
    let n = |base: u64| (base * scale).max(if base == 0 { 0 } else { 1 });

    // 0. Agent A's headline hand-off gate: 1,000,000 round-trip iterations.
    //    (Scaled; at SCALE=0 a smaller smoke count still runs.)
    let det_iters = if scale == 0 {
        50_000
    } else {
        1_000_000 * scale
    };
    eprintln!("[0/8] determinism round-trip gate: {det_iters} iters");
    roundtrip::run_determinism_roundtrip_gate(det_iters, 0x0A11_CE5E_EDED_2024);

    // 1. Canonical round-trip (criterion 4): type-level corpus.
    let iters = (50_000 * scale).max(2_000);
    eprintln!("[1/8] canonical round-trip corpus: {iters} iters");
    roundtrip::run_roundtrip_corpus(iters, 0x00C0_FFEE_1234_5678);

    // 1b. Bundle manifest + reducer-bookkeeping serialization + full-Score byte
    //     round-trip (item 5's whole-score codec, via reduce_onto).
    eprintln!("[1b ] manifest + bookkeeping + full-Score serialization stability");
    for seed in 0..n(64) {
        roundtrip::assert_manifest_roundtrip(&roundtrip::committed_manifest(seed));
        let mut rng = Rng::new(seed.wrapping_mul(0x0100_0193).wrapping_add(17));
        roundtrip::assert_manifest_roundtrip(&generators::rich_manifest(&mut rng));
        let session = generators::operation_envelopes(&mut rng, 40, 3, 6, 6);
        roundtrip::assert_reduction_serialization_stable(&session, seed);
        let (score, frontier) = convergence::materialized_score(seed.wrapping_add(101));
        roundtrip::assert_score_serialization_stable(&score, &frontier, seed);
        let envs = generators::operation_envelopes(&mut rng, 24, 3, 8, 8);
        roundtrip::assert_operation_block_summary_survives_storage(&envs, seed.wrapping_add(202));
    }
    roundtrip::assert_content_mutation_changes_serialization();

    // 2. Crash safety (criterion 2): the testkit driver + Agent D's gate.
    let crash_iters = (10_000 * scale).max(1_000);
    eprintln!("[2/8] crash recovery: {crash_iters} iters (testkit) + bundle gate");
    bundle_harness::run_crash_recovery(crash_iters, 0xF00D_BEEF_1234_5678);
    bundle_harness::bundle_crash_recovery_fuzz(crash_iters, 0x0123_4567_89AB_CDEF);

    // 3. Equivocation (criterion 3): testkit driver + Agent C's gate.
    eprintln!("[3/8] equivocation order-independence");
    for seed in 0..n(500) {
        equivocation::run_equivocation(16, seed.wrapping_mul(0x9E37_79B9).wrapping_add(1));
    }
    equivocation::ops_equivocation_fuzz((10_000 * scale).max(1_000), 0x1234_5678);

    // 4. Manifest selection.
    eprintln!("[4/8] manifest selection");
    for seed in 0..n(32) {
        bundle_harness::run_manifest_selection(seed);
    }

    // 5. Convergence (criterion 1): real-Score convergence through reduce_onto,
    //    plus the reducer-bookkeeping projection convergence.
    eprintln!("[5/8] convergence across delivery orders (real Score + bookkeeping)");
    for seed in 0..n(64) {
        convergence::run_graph_convergence(6, seed.wrapping_mul(0x9E37_79B9).wrapping_add(11));
    }
    for seed in 0..n(500) {
        convergence::run_convergence(24, 8, seed.wrapping_mul(0x9E37_79B9));
    }
    for seed in 0..n(32) {
        convergence::run_two_staff_convergence(8, seed.wrapping_mul(0x9E37_79B9).wrapping_add(7));
    }

    // 5b. Audit regression guards (every defect the Agent C audit surfaced).
    eprintln!("[5b ] audit defect regression guards");
    negative::run_all();

    // 6. Reduction determinism (criterion 5): a large set reduced many ways, the
    //    testkit's authoritative causal-order gate, + Agent C's own gate.
    let big = (1_000 * scale).max(1_000) as usize;
    eprintln!("[6/8] reduction determinism: {big}-envelope set, 10 orders");
    {
        let mut rng = Rng::new(0x5EED_0006_0F0F_0F0F);
        let envelopes = generators::operation_envelopes(&mut rng, big, 3, 40, 40);
        convergence::assert_causal_order_respected(&envelopes);
        convergence::assert_reduction_determinism(&envelopes, 10, &mut rng);
    }
    // Authoritative: causal-order correctness over many conformant sets.
    convergence::run_authoritative_reduction_gate(
        (10_000 * scale).max(1_000) as usize,
        3,
        0x0CA0_05A1,
    );
    // Supplementary: Agent C's own hand-off gate (permutation invariance).
    convergence::ops_reduction_determinism_fuzz((10_000 * scale).max(1_000), 0x00C0_FFEE);

    // 7. Layout round-trip (criterion 6).
    eprintln!("[7/8] layout round-trip");
    for seed in 0..n(128) {
        layout_stub::round_trip(&fixtures::ten_measure_single_staff(seed));
        layout_stub::round_trip(&generators::graph::valid_score_rich(seed));
    }

    eprintln!("[8/8] ok: full conformance suite passed (scale {scale})");
}
