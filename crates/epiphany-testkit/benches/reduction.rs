//! The Chapter 10 operation-envelope reduction-rate bench + budget gate
//! (Phase 2 worklist F1).
//!
//! The normative budget (`spec/core_spec.tex`, Chapter 10 §"File Format
//! Performance"):
//!
//! > Operation-envelope reduction rate MUST exceed 10,000 envelopes per second
//! > on the reference hardware profile, measured during cold reduction from a
//! > fresh canonical base.
//!
//! Three documented scale points: **1K** (the acceptance suite's criterion-5
//! scale), **10K**, and **50K** envelopes. The timed section is the cold
//! reduction an opener performs — `OperationSet::new()` + `accept_all` +
//! `reduce()` on a fresh set — with the envelope vector generated once per
//! scale point from a fixed seed and *cloned outside* the timed section.
//!
//! Criterion measures (throughput in envelopes/s); the budget gate in `main`
//! asserts, with the budget table below (`epiphany_testkit::budget` explains
//! the Pass/Xfail semantics and the deliberate deviation from Chapter 10's
//! p99-over-1000-iterations conformance methodology). The defect this bench
//! was written to surface — `canonical_reduction_order`'s literal O(n²)
//! double-loop indegree construction plus its per-emission full ready-scan
//! (`crates/epiphany-ops/src/reduce.rs`) — sank the 50K point decisively
//! (~1.7K env/s ≈ 29 s per cold reduce on the dev profile) and left the 10K
//! point clearing the budget with only ~25% margin. Agent K's subquadratic
//! rewrite (threshold/frontier readiness; see the epiphany-ops DECISIONS
//! entry) closed the loop — **F surfaces, K fixes**
//! (`spec/PHASE2_F_WEEK0_WORKLIST.md` F1) — and the gate's XPASS notice
//! promoted the 50K row to `Pass` (~87K env/s measured, ~8.7x budget).
//!
//! Run: `cargo bench -p epiphany-testkit --bench reduction`. Set
//! `EPIPHANY_BENCH_QUICK=1` for the reduced PR-CI shape (smaller sampling, 50K
//! point skipped). Under `cargo test --benches` criterion runs each measurement
//! once in test mode and the gate is skipped — the gate belongs to `cargo
//! bench`.

use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, SamplingMode, Throughput};
use epiphany_ops::{MaterializedState, OperationEnvelope, OperationSet};
use epiphany_testkit::budget::{self, Expectation};
use epiphany_testkit::{generators, Rng};

/// Chapter 10: the reduction rate MUST exceed 10,000 envelopes per second.
const RATE_BUDGET_ENV_PER_SEC: f64 = 10_000.0;

/// One documented scale point of THE BUDGET TABLE below.
struct ScalePoint {
    /// Envelope count (also the criterion throughput element count).
    n_ops: usize,
    /// Fixed generator seed — bench inputs are reproducible byte-for-byte.
    seed: u64,
    /// The documented expectation against `RATE_BUDGET_ENV_PER_SEC`.
    expectation: Expectation,
    /// Budget-gate timed iterations (full mode, quick mode); `0` skips the
    /// row in that mode (printed as an explicit skip, never silent).
    gate_iters: (usize, usize),
    /// Criterion measurement time (full mode), or `None` to leave the point
    /// gate-only (the 50K point: it predates the reducer fix, when a single
    /// cold reduction was minute-scale; the budget gate's median covers it).
    criterion_time: Option<Duration>,
}

/// THE BUDGET TABLE (worklist F1). Budget: > 10,000 envelopes/second, cold.
///
/// | envelopes | expectation | measured (dev profile, 2026-07, post-K-fix) | why |
/// |-----------|-------------|----------------------------------------------|-----|
/// | 1,000     | Pass        | ~674,000 env/s (~1.5 ms)                     | criterion-5 scale; ~67x margin |
/// | 10,000    | Pass        | ~257,000 env/s (~39 ms)                      | ~26x margin |
/// | 50,000    | Pass        | ~87,000 env/s (~0.58 s)                      | promoted from Xfail by Agent K's subquadratic reducer; full/nightly runs only |
///
/// Pre-fix (the numbers the F1 xfail table documented): ~155K / ~12.5K /
/// ~1.7K env/s — the O(n²) `canonical_reduction_order` indegree construction,
/// which sank 50K (~29 s per cold reduce) and left 10K only ~25% over budget.
/// Agent K's threshold/frontier rewrite (see the epiphany-ops DECISIONS
/// entry) is byte-identical in order and subquadratic; the gate's XPASS
/// notice triggered the 50K promotion recorded here.
///
/// If any row starts missing the budget again, that is a fresh regression:
/// fix the reducer, do not re-mark rows Xfail without a written decision.
const SCALE_POINTS: &[ScalePoint] = &[
    ScalePoint {
        n_ops: 1_000,
        seed: 0x00F1_5EED_0001,
        expectation: Expectation::Pass,
        gate_iters: (9, 5),
        criterion_time: Some(Duration::from_secs(6)),
    },
    ScalePoint {
        n_ops: 10_000,
        seed: 0x00F1_5EED_0002,
        expectation: Expectation::Pass,
        gate_iters: (3, 2),
        criterion_time: Some(Duration::from_secs(20)),
    },
    ScalePoint {
        n_ops: 50_000,
        seed: 0x00F1_5EED_0003,
        expectation: Expectation::Pass,
        gate_iters: (1, 0),
        criterion_time: None,
    },
];

/// The scale point's reproducible envelope set — the criterion-5 session shape
/// (3 replicas, 40 events, 40 pitches) at `n_ops` envelopes.
fn envelopes_at(point: &ScalePoint) -> Vec<OperationEnvelope> {
    let mut rng = Rng::new(point.seed);
    generators::operation_envelopes(&mut rng, point.n_ops, 3, 40, 40)
}

/// The timed section: cold reduction from a fresh canonical base — build a
/// fresh set (acceptance) and reduce it. The envelope clone happens in the
/// caller's un-timed setup.
fn cold_reduce(envelopes: Vec<OperationEnvelope>) -> MaterializedState {
    let mut set = OperationSet::new();
    set.accept_all(envelopes);
    set.reduce()
}

/// The criterion measurement side (envelopes/s via `Throughput::Elements`).
fn criterion_measurements(criterion: &mut Criterion, quick: bool) {
    let mut group = criterion.benchmark_group("reduction_cold");
    // Cold multi-second iterations at 10K: flat sampling, the minimum sample
    // count, and per-point measurement times keep `cargo bench` wall-clock sane.
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    for point in SCALE_POINTS {
        let Some(time) = point.criterion_time else {
            continue; // 50K is gate-only; see THE BUDGET TABLE.
        };
        if quick && point.n_ops > 1_000 {
            continue; // quick mode: the gate still measures 10K, cheaply.
        }
        let envelopes = envelopes_at(point);
        group.throughput(Throughput::Elements(point.n_ops as u64));
        group.measurement_time(if quick { Duration::from_secs(2) } else { time });
        group.warm_up_time(Duration::from_millis(if quick { 500 } else { 1500 }));
        group.bench_with_input(
            BenchmarkId::from_parameter(point.n_ops),
            &envelopes,
            |b, envs| b.iter_batched(|| envs.clone(), cold_reduce, BatchSize::PerIteration),
        );
    }
    group.finish();
}

/// The budget-gate side: evaluates THE BUDGET TABLE and returns the rows.
fn budget_gate(quick: bool) -> Vec<budget::GateReport> {
    let mut reports = Vec::new();
    for point in SCALE_POINTS {
        let iters = if quick {
            point.gate_iters.1
        } else {
            point.gate_iters.0
        };
        if iters == 0 {
            println!(
                "skip  reduction/{}: heaviest scale point; full/nightly runs only \
                 (unset EPIPHANY_BENCH_QUICK)",
                point.n_ops
            );
            continue;
        }
        let envelopes = envelopes_at(point);
        let median = budget::median_time(iters, || envelopes.clone(), cold_reduce);
        reports.push(budget::rate_gate(
            format!("reduction/{}", point.n_ops),
            point.n_ops as u64,
            median,
            RATE_BUDGET_ENV_PER_SEC,
            point.expectation,
        ));
    }
    reports
}

fn main() {
    // `cargo bench` passes `--bench`; its absence means criterion's test mode
    // (`cargo test --benches` / `--all-targets`): run each measurement once,
    // skip the gate.
    let bench_mode = std::env::args().any(|arg| arg == "--bench");
    let quick = budget::quick_mode();

    let mut criterion = Criterion::default().configure_from_args();
    criterion_measurements(&mut criterion, quick);
    criterion.final_summary();

    if !bench_mode {
        return;
    }
    if !budget::verdict(&budget_gate(quick)) {
        std::process::exit(1);
    }
}
