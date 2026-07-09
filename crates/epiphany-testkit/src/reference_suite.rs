//! The **Reference Suite harness** — the executable binding of the *Reference
//! Suite* companion (v0.1.0) the companion's non-normative Harness Binding
//! chapter says is "delivered with the reference implementation".
//!
//! The six v0.1 entries ([`entries`]) are transcribed from the companion's
//! Chapter 3 ("The v0.1 Entry Set"), each named by reference-implementation
//! builder and seed exactly as the companion's score-referencing rule
//! (`req:refsuite:referencing`) prescribes: a seeded test-kit builder
//! (`fixtures::ten_measure_single_staff`), a core generator the corpus pins
//! (`generators::valid_score_rich` seed `0xF302` = corpus entry
//! `gen_valid_score_rich`), or a zero-argument corpus entry resolved by its
//! `name` string. [`evaluate_minimal`] runs one entry through the standard
//! pipeline (`to_logical` → `to_constrained` → `solve`) under the companion's
//! declared solve configuration — the solver's documented default A4 geometry
//! and the default [`SolverConfig`] (`Standard` profile, unbounded
//! deterministic budget, the Quality Metric Catalog's default tie-breaking
//! weights) — and asserts the companion's four-condition per-entry pass rule
//! (`req:refsuite:pass`) at the **Minimal** tier:
//!
//! 1. every hard constraint satisfied (renderable, non-partial status);
//! 2. internal determinism (byte-identical `ResolvedLayoutIR` canonical bytes
//!    *and* bitwise-identical metric vectors across repeated solves);
//! 3. a well-formed, diagnostically accurate report (`Minimal` tier claim,
//!    a metric vector that is valid — finite, in `[0,1]` — and computed,
//!    never the all-worst placeholder);
//! 4. every normative metric at or below the Quality Metric Catalog's
//!    Minimal-column threshold for its axis (no v0.1 entry overrides them).
//!
//! Condition 4 carries the budget harness's **Pass/Xfail discipline**
//! ([`crate::budget`], DECISIONS F1): an axis listed in
//! [`SuiteEntry::minimal_xfail`] is a *documented, measured* threshold miss —
//! asserted to still miss, so the marking cannot rot (an `XPASS` fails the
//! harness demanding promotion), and reported for spec-side resolution rather
//! than silently waived. As of engrave v3 every v0.1 entry passes every Minimal
//! threshold, so no entry carries an xfail row: RS-1's original
//! `casting_off_quality` miss (P12-I11 — greedy first-fit's stub last system)
//! was cleared by the engraver's casting-off widow-rebalance phase, the machinery
//! that would have asserted the miss now stands ready for the next one.
//!
//! Following the testkit's library-module-per-harness policy (DECISIONS F0),
//! this module holds the machinery and `tests/reference_suite.rs` asserts it —
//! one test per entry, so a failure names its entry. The module is
//! solver-parametric (the crate's dependency on `epiphany-engrave` is
//! dev-only); the integration test supplies the real `Engraver`.

use epiphany_core::Score;
use epiphany_layout_ir::{
    to_constrained, to_logical, ConstraintSolver, QualityMetricKind, QualityMetricVector,
    SolveStatus, SolverConfig, SolverTier, MINIMAL_THRESHOLDS, QUALITY_METRIC_KINDS,
};

use crate::corpus::corpus;
use crate::fixtures;

/// A documented Minimal-threshold miss (the budget harness's `Xfail` shape):
/// the axis and the reason it is expected to exceed its threshold today.
#[derive(Copy, Clone, Debug)]
pub struct MinimalXfail {
    pub axis: QualityMetricKind,
    pub reason: &'static str,
}

/// One v0.1 suite entry (Reference Suite companion, Table "entries").
pub struct SuiteEntry {
    /// The companion's entry id (`RS-1` … `RS-6`).
    pub id: &'static str,
    /// The companion's entry title.
    pub title: &'static str,
    /// The companion's construction reference (builder + seed / corpus name).
    pub construction: &'static str,
    /// Deterministic score construction per that reference.
    pub build: fn() -> Score,
    /// Documented, measured Minimal-threshold misses (see the module docs).
    pub minimal_xfail: &'static [MinimalXfail],
}

fn corpus_score(name: &str) -> Score {
    let fixture = corpus()
        .into_iter()
        .find(|fixture| fixture.name == name)
        .unwrap_or_else(|| panic!("corpus entry {name} named by the Reference Suite is missing"));
    (fixture.build)()
}

fn rs1() -> Score {
    fixtures::ten_measure_single_staff(0x000A_11CE)
}
fn rs2() -> Score {
    // The companion cites `generators::valid_score_rich(0xF302)`, "identically
    // reachable as the test-kit corpus entry `gen_valid_score_rich`" — resolve
    // through the corpus and pin the identity in `rs2_construction_reproduces`.
    corpus_score("gen_valid_score_rich")
}
fn rs3() -> Score {
    corpus_score("b_flat_major_scale")
}
fn rs4() -> Score {
    corpus_score("two_voice_counterpoint")
}
fn rs5() -> Score {
    corpus_score("notes_and_rests")
}
fn rs6() -> Score {
    corpus_score("meter_three_four")
}

/// The companion's cited construction for RS-2, for the identity pin: the
/// corpus entry must reproduce this score graph bit-for-bit.
pub fn rs2_cited_builder() -> Score {
    epiphany_core::generators::valid_score_rich(0xF302)
}

/// The v0.1 entry set, exactly the six entries of the companion's
/// Table "entries". Every entry is required at the Minimal tier; the same six
/// constitute the Standard subset (not asserted here: no implementation claims
/// Standard as of this suite version).
pub fn entries() -> Vec<SuiteEntry> {
    vec![
        SuiteEntry {
            id: "RS-1",
            title: "Ten-measure single staff",
            construction: "fixtures::ten_measure_single_staff(0x000A_11CE)",
            build: rs1,
            // Passes every Minimal threshold under the reference engraver
            // (the QMC anchors it is scored against are unchanged through
            // v0.3.0). Greedy first-fit alone left a
            // two-measure stub last system (width CV 0.61 -> clamped 1.0 > the
            // 0.90 threshold, the original P12-I11 miss); casting-off's
            // widow-rebalance phase now evens the split to a six/four
            // distribution (width CV ~0.22 -> casting_off ~0.45), clearing the
            // miss the honest way — an engrave-side balance pass, goldens
            // regenerated, no QMC anchor/threshold relaxation. See DECISIONS.md
            // and the engrave casting module.
            minimal_xfail: &[],
        },
        SuiteEntry {
            id: "RS-2",
            title: "Rich multi-region score",
            construction: "generators::valid_score_rich(0xF302) = corpus gen_valid_score_rich",
            build: rs2,
            minimal_xfail: &[],
        },
        SuiteEntry {
            id: "RS-3",
            title: "B-flat major scale",
            construction: "corpus b_flat_major_scale",
            build: rs3,
            minimal_xfail: &[],
        },
        SuiteEntry {
            id: "RS-4",
            title: "Two-voice counterpoint",
            construction: "corpus two_voice_counterpoint",
            build: rs4,
            minimal_xfail: &[],
        },
        SuiteEntry {
            id: "RS-5",
            title: "Notes and rests",
            construction: "corpus notes_and_rests",
            build: rs5,
            minimal_xfail: &[],
        },
        SuiteEntry {
            id: "RS-6",
            title: "Three-four meter line",
            construction: "corpus meter_three_four",
            build: rs6,
            minimal_xfail: &[],
        },
    ]
}

/// What evaluating one entry measured, for the report table.
pub struct EntryOutcome {
    pub id: &'static str,
    pub title: &'static str,
    pub status: SolveStatus,
    pub metrics: QualityMetricVector,
    /// Axes that exceeded their Minimal threshold under a documented xfail row.
    pub xfailed: Vec<QualityMetricKind>,
}

/// Evaluates one entry's four-condition **Minimal** pass
/// (`req:refsuite:pass`), panicking with the entry's id on the first violated
/// condition. Returns the measured outcome for the report table.
pub fn evaluate_minimal(solver: &dyn ConstraintSolver, entry: &SuiteEntry) -> EntryOutcome {
    let id = entry.id;
    // The solve the entry declares: the standard pipeline under the default
    // solver configuration (`req:refsuite:solve-config`). The page geometry is
    // the solver's construction-time parameter; the integration test pins the
    // reference solver's default to the companion's declared A4 numbers.
    let constrained = to_constrained(&to_logical(&(entry.build)()));
    let config = SolverConfig::default();
    let report = solver.solve(&constrained, &config);
    let again = solver.solve(&constrained, &config);

    // Condition 3 (tier claim): the suite evaluates a Minimal-tier claim.
    assert_eq!(
        solver.tier(),
        SolverTier::Minimal,
        "{id}: the solver under test must claim the Minimal tier"
    );

    // Condition 1: every hard constraint satisfied. Unsatisfiable and
    // budget-exhausted partial solves are failures, not exemptions.
    assert!(
        matches!(
            report.status,
            SolveStatus::Solved | SolveStatus::SolvedWithWarnings
        ),
        "{id}: not a fully solved layout: {:?}",
        report.status
    );
    assert!(
        report.satisfied_hard_constraints,
        "{id}: hard constraints unsatisfied"
    );
    assert!(
        report.unsatisfied_constraints.is_empty(),
        "{id}: unsatisfied constraints reported: {:?}",
        report.unsatisfied_constraints
    );

    // Condition 2: internal determinism — byte-identical canonical layout and
    // bitwise-identical metric vectors across repeated identical solves.
    assert_eq!(
        report.layout.canonical_bytes(),
        again.layout.canonical_bytes(),
        "{id}: repeated solves differ in canonical ResolvedLayoutIR bytes"
    );
    for kind in QUALITY_METRIC_KINDS {
        assert_eq!(
            report.metric_vector.axis(kind).0.to_bits(),
            again.metric_vector.axis(kind).0.to_bits(),
            "{id}: repeated solves differ on {kind:?}"
        );
    }

    // Condition 3 (report accuracy): the metric vector is valid and computed
    // per the Quality Metric Catalog — never a placeholder.
    for kind in QUALITY_METRIC_KINDS {
        let value = report.metric_vector.axis(kind).0;
        assert!(
            value.is_finite() && (0.0..=1.0).contains(&value),
            "{id}: {kind:?} = {value} is not a valid NormalizedMetric"
        );
    }
    assert_ne!(
        report.metric_vector,
        QualityMetricVector::unmeasured(),
        "{id}: the metric vector is the unmeasured placeholder"
    );

    // Condition 4: every normative metric within the Minimal threshold column
    // (no v0.1 entry declares an override), under the Pass/Xfail discipline.
    let mut xfailed = Vec::new();
    for kind in QUALITY_METRIC_KINDS {
        let value = report.metric_vector.axis(kind).0;
        let threshold = MINIMAL_THRESHOLDS.axis(kind);
        match entry.minimal_xfail.iter().find(|row| row.axis == kind) {
            Some(row) => {
                assert!(
                    value > threshold,
                    "{id}: XPASS on {kind:?} ({value} <= {threshold}) — the measured miss \
                     was resolved; promote the entry by removing its xfail row ({})",
                    row.reason
                );
                xfailed.push(kind);
            }
            None => assert!(
                value <= threshold,
                "{id}: {kind:?} = {value} exceeds its Minimal threshold {threshold}"
            ),
        }
    }

    EntryOutcome {
        id: entry.id,
        title: entry.title,
        status: report.status,
        metrics: report.metric_vector,
        xfailed,
    }
}

/// One aligned report row per outcome, for the printed metric table (run the
/// integration test with `--nocapture` to see it).
pub fn table(outcomes: &[EntryOutcome]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<5} {:<24} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}\n",
        "entry",
        "title",
        "collision",
        "spacing",
        "slur",
        "beam",
        "vertical",
        "sysbreak",
        "pagefill",
        "castoff",
        "density"
    ));
    for outcome in outcomes {
        let value = |kind: QualityMetricKind| {
            let v = outcome.metrics.axis(kind).0;
            if outcome.xfailed.contains(&kind) {
                format!("{v:.4}*")
            } else {
                format!("{v:.4}")
            }
        };
        out.push_str(&format!(
            "{:<5} {:<24} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}\n",
            outcome.id,
            outcome.title,
            value(QualityMetricKind::Collision),
            value(QualityMetricKind::Spacing),
            value(QualityMetricKind::SlurShape),
            value(QualityMetricKind::BeamSlope),
            value(QualityMetricKind::VerticalDensity),
            value(QualityMetricKind::SystemBreak),
            value(QualityMetricKind::PageFill),
            value(QualityMetricKind::CastingOff),
            value(QualityMetricKind::SymbolDensity),
        ));
    }
    out.push_str("(* = documented Minimal xfail row, asserted to still miss)\n");
    out
}
