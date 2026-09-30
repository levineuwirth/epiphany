//! The Chapter 10 performance-budget gate (Phase 2 worklist F1).
//!
//! The benches in `benches/` (home per `DECISIONS.md` F0) *measure* with
//! criterion; criterion never *asserts*, so each bench binary's `main()` ends
//! by running these gates: a calibrated timing check per budget row, with the
//! numeric threshold written at the call site in the bench source. Every row
//! carries an [`Expectation`]:
//!
//! * [`Expectation::Pass`] — the budget must hold **today**; a miss fails the
//!   bench run with a nonzero exit (the CI tripwire).
//! * [`Expectation::Xfail`] — a documented, known-pending miss whose reason
//!   names the defect and its owner. A miss prints an expected-failure line
//!   and does **not** fail the run; a *pass* prints a notice asking for the
//!   marking to be reconsidered. The pass counts only when measured on
//!   hardware comparable to the CI runners the row was marked against; a pass
//!   on a faster machine alone promotes nothing.
//!
//! This is the "F surfaces, K fixes" handshake (`spec/PHASE2_F_WEEK0_WORKLIST.md`
//! F1): a known-pending scale point stays `Xfail` — budget written in the
//! bench — until the named defect is fixed, at which point the gate itself
//! reports that the marking should be reconsidered. The inaugural round
//! completed: the reducer's `O(n²)` `canonical_reduction_order` sank
//! `reduction/50000` until Agent K's subquadratic rewrite, whose XPASS notice
//! promoted the row.
//!
//! ## Methodology note (a deliberate deviation from Chapter 10)
//!
//! Chapter 10's conformance methodology is **p99 over ≥ 1000 iterations per
//! scenario** on the reference hardware profile. That is the reference suite's
//! job, not this gate's: 1000 iterations of a minute-long 50K-envelope cold
//! reduction would be unusable in CI. The gate instead takes the **median of a
//! small, per-row calibrated iteration count** in a release build — enough to
//! reject flukes while keeping `cargo bench` wall-clock sane. Runs are cold
//! (no in-gate warm-up): the marquee budget is an explicitly *cold* reduction
//! rate, and the criterion measurements that precede the gate have already
//! warmed the allocator and caches for the warm-appropriate rows. Conformance
//! *claims* still require the full Chapter 10 methodology.

use std::time::{Duration, Instant};

/// How a budget row is expected to behave on the current implementation.
#[derive(Copy, Clone, Debug)]
pub enum Expectation {
    /// The budget must hold; a miss fails the bench run (nonzero exit).
    Pass,
    /// A documented known-pending miss; the string names the defect and who
    /// fixes it. A miss is reported but tolerated; a pass asks for the marking
    /// to be reconsidered.
    Xfail(&'static str),
}

/// One evaluated budget row.
#[derive(Debug)]
pub struct GateReport {
    /// The row label, e.g. `reduction/10000`.
    pub label: String,
    /// Human-readable `measured vs budget` detail.
    pub detail: String,
    /// Whether the measurement met the budget.
    pub met_budget: bool,
    /// The row's documented expectation.
    pub expectation: Expectation,
}

impl GateReport {
    /// A `Pass`-marked row that missed its budget — the only outcome that
    /// fails the bench run.
    pub fn unexpected_failure(&self) -> bool {
        !self.met_budget && matches!(self.expectation, Expectation::Pass)
    }

    /// An `Xfail`-marked row that met its budget: the marking may be stale,
    /// and the pass counts only when measured on comparable CI hardware.
    pub fn unexpected_pass(&self) -> bool {
        self.met_budget && matches!(self.expectation, Expectation::Xfail(_))
    }

    /// The verdict line printed for this row.
    pub fn line(&self) -> String {
        match (self.met_budget, self.expectation) {
            (true, Expectation::Pass) => format!("PASS  {}: {}", self.label, self.detail),
            (false, Expectation::Pass) => format!(
                "FAIL  {}: {} — budget missed on a Pass-marked row",
                self.label, self.detail
            ),
            (false, Expectation::Xfail(reason)) => format!(
                "XFAIL {}: {} — expected failure: {}",
                self.label, self.detail, reason
            ),
            (true, Expectation::Xfail(reason)) => format!(
                "XPASS {}: {} — met the budget despite the xfail marking ({}); \
                 reconsider the marking only if this ran on comparable CI hardware; \
                 a pass on a faster machine alone promotes nothing",
                self.label, self.detail, reason
            ),
        }
    }
}

/// The median over `iters` timed runs of `op`, with per-run input built by
/// `setup` **outside** the timed section (this is how a cold-reduction row
/// clones its envelope vector without the clone being charged to the budget).
/// The output is dropped outside the timed section too. `iters ≥ 1`.
pub fn median_time<S, T>(
    iters: usize,
    mut setup: impl FnMut() -> S,
    mut op: impl FnMut(S) -> T,
) -> Duration {
    assert!(iters >= 1, "a gate row needs at least one timed iteration");
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let input = setup();
        let start = Instant::now();
        let out = op(input);
        let elapsed = start.elapsed();
        std::hint::black_box(&out);
        samples.push(elapsed);
        drop(out);
    }
    samples.sort();
    samples[samples.len() / 2]
}

/// Evaluates a throughput budget: `elements` per timed run must exceed
/// `budget_per_sec` (the Chapter 10 reduction-rate form, "MUST exceed").
pub fn rate_gate(
    label: impl Into<String>,
    elements: u64,
    median: Duration,
    budget_per_sec: f64,
    expectation: Expectation,
) -> GateReport {
    let rate = elements as f64 / median.as_secs_f64();
    GateReport {
        label: label.into(),
        detail: format!(
            "{rate:.0} elements/s ({elements} elements, median {median:.2?}); \
             budget > {budget_per_sec:.0}/s"
        ),
        met_budget: rate > budget_per_sec,
        expectation,
    }
}

/// Evaluates a latency budget: the median must come in at or under `budget`
/// (the Chapter 10 file-format form, "completes within").
pub fn latency_gate(
    label: impl Into<String>,
    median: Duration,
    budget: Duration,
    expectation: Expectation,
) -> GateReport {
    GateReport {
        label: label.into(),
        detail: format!("median {median:.2?}; budget <= {budget:.0?}"),
        met_budget: median <= budget,
        expectation,
    }
}

/// The note printed under the rows when any `Xfail` row met its budget, or
/// `None` when none did. Like the XPASS line it asks for reconsideration only
/// on comparable CI hardware, never for promotion.
pub fn xpass_note(reports: &[GateReport]) -> Option<String> {
    let unexpected_passes = reports.iter().filter(|r| r.unexpected_pass()).count();
    (unexpected_passes > 0).then(|| {
        format!(
            "note: {unexpected_passes} xfail row(s) met their budget — reconsider their \
             marking only if this ran on comparable CI hardware."
        )
    })
}

/// Prints every row's verdict and returns whether the gate holds — i.e. no
/// `Pass`-marked row missed its budget. The bench binary exits nonzero when
/// this returns `false`; `Xfail` misses and `XPASS` rows never fail the run
/// (the latter print a loud notice asking for reconsideration instead).
pub fn verdict(reports: &[GateReport]) -> bool {
    println!("\n== Chapter 10 budget gate (worklist F1) ==");
    for report in reports {
        println!("{}", report.line());
    }
    let unexpected: Vec<&GateReport> = reports.iter().filter(|r| r.unexpected_failure()).collect();
    if let Some(note) = xpass_note(reports) {
        println!("{note}");
    }
    if unexpected.is_empty() {
        println!("budget gate: OK ({} row(s))", reports.len());
        true
    } else {
        println!(
            "budget gate: FAILED — {} Pass-marked row(s) missed their budget",
            unexpected.len()
        );
        false
    }
}

/// Whether the CI-friendly quick mode is on (`EPIPHANY_BENCH_QUICK=1`):
/// reduced criterion sampling, reduced gate iteration counts, and the heaviest
/// scale points (the 50K-envelope reduction row) skipped entirely. PR CI sets
/// it; the nightly soak and local full runs leave it unset.
pub fn quick_mode() -> bool {
    std::env::var("EPIPHANY_BENCH_QUICK").is_ok_and(|v| !v.is_empty() && v != "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Neither XPASS message may ask for promotion, in any case: an XPASS
    /// triggers reconsideration on comparable CI hardware only (roadmap D15,
    /// D16).
    fn assert_asks_no_promotion(message: &str) {
        let lower = message.to_lowercase();
        assert!(!lower.contains("to pass"), "asks for promotion: {message}");
        assert!(
            !lower.contains("promote this"),
            "asks for promotion: {message}"
        );
        assert!(
            !lower.contains("promote them"),
            "asks for promotion: {message}"
        );
    }

    fn row(met_budget: bool, expectation: Expectation) -> GateReport {
        GateReport {
            label: "test/row".to_owned(),
            detail: "detail".to_owned(),
            met_budget,
            expectation,
        }
    }

    #[test]
    fn pass_row_meeting_budget_holds() {
        let r = row(true, Expectation::Pass);
        assert!(!r.unexpected_failure());
        assert!(!r.unexpected_pass());
        assert!(verdict(&[r]));
    }

    #[test]
    fn pass_row_missing_budget_fails_the_gate() {
        let r = row(false, Expectation::Pass);
        assert!(r.unexpected_failure());
        assert!(r.line().starts_with("FAIL"));
        assert!(!verdict(&[r]));
    }

    #[test]
    fn xfail_row_missing_budget_is_tolerated() {
        let r = row(false, Expectation::Xfail("documented defect"));
        assert!(!r.unexpected_failure());
        assert!(r.line().starts_with("XFAIL"));
        assert!(r.line().contains("documented defect"));
        assert!(verdict(&[r]));
    }

    #[test]
    fn xfail_row_meeting_budget_asks_for_reconsideration_but_holds() {
        let r = row(true, Expectation::Xfail("documented defect"));
        assert!(r.unexpected_pass());
        assert!(r.line().starts_with("XPASS"));
        assert!(r.line().contains("reconsider the marking"));
        assert!(r.line().contains("comparable CI hardware"));
        assert_asks_no_promotion(&r.line());
        assert!(verdict(&[r]));
    }

    #[test]
    fn xpass_note_asks_for_reconsideration_but_not_promotion() {
        let reports = [
            row(true, Expectation::Xfail("documented defect")),
            row(true, Expectation::Pass),
            row(false, Expectation::Xfail("documented defect")),
        ];
        let note = xpass_note(&reports).expect("one xfail row met its budget");
        assert!(note.starts_with("note: 1 xfail row(s) met their budget"));
        assert!(note.contains("reconsider their marking"));
        assert!(note.contains("comparable CI hardware"));
        assert_asks_no_promotion(&note);
        assert_eq!(xpass_note(&reports[1..]), None);
    }

    #[test]
    fn gates_evaluate_their_thresholds() {
        // 100 elements in 1 ms = 100,000/s.
        let fast = rate_gate(
            "rate/fast",
            100,
            Duration::from_millis(1),
            10_000.0,
            Expectation::Pass,
        );
        assert!(fast.met_budget);
        let slow = rate_gate(
            "rate/slow",
            100,
            Duration::from_millis(100),
            10_000.0,
            Expectation::Pass,
        );
        assert!(!slow.met_budget);

        let ok = latency_gate(
            "lat/ok",
            Duration::from_millis(10),
            Duration::from_millis(50),
            Expectation::Pass,
        );
        assert!(ok.met_budget);
        let over = latency_gate(
            "lat/over",
            Duration::from_millis(60),
            Duration::from_millis(50),
            Expectation::Pass,
        );
        assert!(!over.met_budget);
    }

    #[test]
    fn median_time_takes_the_middle_sample() {
        // Deterministic ordering check via a controlled op: the median of an
        // odd sample count must be a real observed sample, not an average.
        let mut calls = 0u32;
        let d = median_time(
            5,
            || (),
            |()| {
                calls += 1;
            },
        );
        assert_eq!(calls, 5);
        // No timing assertion (flaky); the structural property is that a
        // duration was produced at all and the closure ran `iters` times.
        let _ = d;
    }
}
