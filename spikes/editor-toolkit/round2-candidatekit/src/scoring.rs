//! Turns a [`CandidateReport`]'s five check outcomes into the Round 2
//! criterion cell, and reports eligibility separately
//! (`ROUND2_TEXT_RECIPE.md` §1.2).

use crate::outcome::CheckOutcome;
use crate::report::CandidateReport;

/// The round's own platform — `round2_textkit::a11y::ACCEPTED_ROLE_TABLE`'s
/// `"at-spi2"` row — restated here so [`ROUND0_READBACK_EVIDENCE`]'s doc
/// comment and [`require_check5_not_run_is_admissible`]'s panic can name it
/// precisely.
pub const ROUND_PLATFORM: &str = "at-spi2";

/// Round 0's own readback evidence, quoted verbatim in the panic
/// [`criterion_cell`]/[`is_eligible`] raise for a check-5 `NotRun` that
/// carries no [`crate::report::BusUnreachableEvidence`].
///
/// `round0-evidence/c1-egui-readback.txt` and
/// `round0-evidence/c2-vello-readback.txt` both record `READBACK: PASS` — a
/// live, out-of-process AT-SPI2 tree walk that succeeded for **both**
/// candidates on this machine. So on [`ROUND_PLATFORM`], "we did not build
/// an accessibility bridge" is not an environmental cause: the bus is
/// reachable, and check 5 is the round's own platform, not declared
/// out-of-scope adapter coverage. `AdapterStatus::NotBuilt` still covers
/// *other* platforms (Windows UIA, macOS AX, ...) as declared scope; it
/// must not be used to excuse the platform the round actually runs on.
pub const ROUND0_READBACK_EVIDENCE: &str = "round0-evidence/c1-egui-readback.txt and \
    round0-evidence/c2-vello-readback.txt both record READBACK: PASS — a live, out-of-process \
    AT-SPI2 tree walk succeeded for both candidates on this machine, so the platform \
    accessibility bus is reachable here and an unbuilt accessibility bridge is not \
    environmental NOT RUN on this platform. AdapterStatus::NotBuilt covers OTHER platforms \
    (Windows UIA, macOS AX, ...) as declared scope; it does not, by itself, excuse the \
    platform the round actually runs on.";

/// Panics if `report.check5_accessibility` is `NotRun` without
/// `report.check5_bus_unreachable_evidence` present — see
/// [`ROUND0_READBACK_EVIDENCE`]. A no-op for `Pass`/`Fail`, and a no-op for
/// a `NotRun` that *does* carry evidence. Deliberately does **not** inspect
/// `report.cost.adapters` — an `AdapterStatus::NotBuilt` entry for
/// [`ROUND_PLATFORM`] must not, by itself, satisfy this check (that is the
/// exact loophole the review named).
fn require_check5_not_run_is_admissible(report: &CandidateReport) {
    if report.check5_accessibility.is_not_run() && report.check5_bus_unreachable_evidence.is_none()
    {
        panic!(
            "candidate {:?} reported check5_accessibility = NotRun(_) with no \
             check5_bus_unreachable_evidence — {ROUND0_READBACK_EVIDENCE}",
            report.candidate_id
        );
    }
}

/// The Round 2 criterion cell for check 3 (bidi / text-run primitives) is
/// structurally identical to [`CheckOutcome`] — a cell is the worst of the
/// five checks, which is itself just a `CheckOutcome` — kept as a distinct
/// name so a reader is never unsure whether a value in hand is *one check's
/// own* outcome or *the criterion cell* five checks reduce to.
pub type CellOutcome = CheckOutcome;

/// The standing ruling [`criterion_cell`] enforces (`ROUND2_TEXT_RECIPE.md`
/// §1.2, 2026-07-29): no Arabic-capable face is installed on the round's
/// declared machine, and pin 9 makes an absent required face environmental
/// `NOT RUN`. Quoted verbatim in the panic [`criterion_cell`] raises for a
/// report that disagrees with it.
pub const CHECK_3_RULING: &str = "ROUND2_TEXT_RECIPE.md §1.2 (2026-07-29 ruling): check 3 is \
    NOT RUN for every candidate, on both adapters — no Arabic-capable face is installed, and \
    pin 9 makes an absent required face environmental NOT RUN. F-D's supplementary Hebrew/Latin \
    bidi evidence is recorded separately and must never upgrade check 3 to PASS.";

/// Reduces a [`CandidateReport`]'s five check outcomes to the Round 2
/// criterion cell: the **worst of the five**, ordered `Pass` < `NotRun` <
/// `Fail` ([`CheckOutcome::severity_rank`]).
///
/// The supplementary F-D bidi result
/// ([`CandidateReport::supplementary_f_d_bidi`]) is a separate field on
/// `CandidateReport` and this function never reads it — that is what makes
/// it **structurally** incapable of reaching the cell (recipe §1.2: "it
/// must not upgrade check 3 to PASS"), rather than merely conventionally
/// excluded by a check this function could someday grow to include by
/// accident.
///
/// # Panics
///
/// Panics if `report.check3_bidi` is anything other than `NotRun` — a
/// candidate reporting `Pass` or `Fail` for check 3 has violated the
/// standing ruling ([`CHECK_3_RULING`]), which this function treats as a
/// programming error in how the candidate assembled its report, not a value
/// a scoring rule is allowed to interpret. (Not every environmental
/// deviation deserves a panic; this one does, because pin 9's face-absence
/// fact does not vary between the two candidates or between runs — a
/// non-`NotRun` value here can only mean the report was built wrong.)
pub fn criterion_cell(report: &CandidateReport) -> CellOutcome {
    if !report.check3_bidi.is_not_run() {
        panic!(
            "candidate {:?} reported check 3 as {:?}, not NotRun(_) — {CHECK_3_RULING}",
            report.candidate_id, report.check3_bidi
        );
    }
    require_check5_not_run_is_admissible(report);

    let checks = [
        &report.check1_faithful_consumption,
        &report.check2_fallback,
        &report.check3_bidi,
        &report.check4_hit_testing,
        &report.check5_accessibility,
    ];
    checks
        .into_iter()
        .max_by_key(|c| c.severity_rank())
        .cloned()
        .expect("`checks` is a fixed non-empty array of five elements")
}

/// The two disqualifying checks (recipe §1.2: "checks 2 and 5 are the
/// disqualifying set"). Named so the disqualifying set is a fact a reader
/// (and a grep) can find, not a claim buried in a comment beside
/// [`is_eligible`].
pub const DISQUALIFYING_CHECKS: &str = "check2_fallback, check5_accessibility";

/// Whether `report` remains a candidate at all — reported **separately**
/// from [`criterion_cell`], because the two questions are different: the
/// cell is what the criterion 3 table shows, eligibility is whether the
/// candidate survives at all.
///
/// Failing check 2 or check 5 disqualifies. Check 3's `NotRun` (the only
/// state it is ever allowed to carry — see [`criterion_cell`]) does **not**
/// disqualify, because check 3 is not in the disqualifying set
/// ([`DISQUALIFYING_CHECKS`]).
///
/// # Panics
///
/// Panics under the same condition [`criterion_cell`] does for check 5 —
/// see [`require_check5_not_run_is_admissible`] / [`ROUND0_READBACK_EVIDENCE`].
/// Without this, a candidate that never wired an accessibility bridge could
/// report `check5_accessibility = NotRun("we did not build it")`, and
/// `is_eligible` would return `true` because `NotRun` is not `Fail` — the
/// exact loophole the ruling this function enforces exists to close. This
/// function does not merely return `false` for that case, because the
/// report itself is malformed (an inadmissible claim), not merely
/// disqualifying: a malformed report should not be silently readable as "at
/// least eligible."
pub fn is_eligible(report: &CandidateReport) -> bool {
    require_check5_not_run_is_admissible(report);
    !report.check2_fallback.is_fail() && !report.check5_accessibility.is_fail()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{AdapterStatus, BusUnreachableEvidence, CostRecord};
    use std::collections::BTreeMap;

    fn base_report() -> CandidateReport {
        CandidateReport {
            candidate_id: "C-TEST".to_string(),
            check1_faithful_consumption: CheckOutcome::Pass,
            check2_fallback: CheckOutcome::Pass,
            check3_bidi: CheckOutcome::NotRun(CHECK_3_RULING.to_string()),
            check4_hit_testing: CheckOutcome::Pass,
            check5_accessibility: CheckOutcome::Pass,
            check5_bus_unreachable_evidence: None,
            supplementary_f_d_bidi: CheckOutcome::Pass,
            per_fixture_diffs: BTreeMap::new(),
            hittest_probe_results: Vec::new(),
            a11y_evidence: Vec::new(),
            cost: CostRecord {
                baseline_commit: "0000000".to_string(),
                dependencies_added: Vec::new(),
                adapters: Vec::new(),
                integration_wiring: Vec::new(),
                loc_by_part: Vec::new(),
            },
        }
    }

    fn some_evidence() -> BusUnreachableEvidence {
        BusUnreachableEvidence {
            probe_description: "connected to the AT-SPI2 session bus".to_string(),
            probe_output: "org.freedesktop.DBus.Error.ServiceUnknown".to_string(),
        }
    }

    // ---- criterion_cell: the worst-of-five rule ----

    #[test]
    fn all_pass_except_the_pinned_check_3_yields_a_not_run_cell() {
        let cell = criterion_cell(&base_report());
        assert!(matches!(cell, CheckOutcome::NotRun(_)), "{cell:?}");
    }

    /// Required kill: a `CandidateReport` claiming check 3 `Pass` is
    /// rejected, naming the §1.2 ruling.
    #[test]
    #[should_panic(expected = "§1.2")]
    fn a_check_3_pass_is_rejected_naming_the_ruling() {
        let mut report = base_report();
        report.check3_bidi = CheckOutcome::Pass;
        let _ = criterion_cell(&report);
    }

    /// Same requirement, the other disallowed value: a `Fail` for check 3
    /// is rejected exactly as a `Pass` is — the ruling pins check 3 to
    /// `NotRun` specifically, not merely "not Pass".
    #[test]
    #[should_panic(expected = "§1.2")]
    fn a_check_3_fail_is_also_rejected_naming_the_ruling() {
        let mut report = base_report();
        report.check3_bidi = CheckOutcome::Fail("pretend Arabic shaping worked".to_string());
        let _ = criterion_cell(&report);
    }

    /// Required kill: a supplementary F-D `Pass` does not move the cell off
    /// `NotRun`.
    #[test]
    fn a_supplementary_f_d_pass_does_not_move_the_cell_off_not_run() {
        let mut report = base_report();
        report.supplementary_f_d_bidi = CheckOutcome::Pass;
        assert!(matches!(criterion_cell(&report), CheckOutcome::NotRun(_)));
    }

    /// Required kill: a supplementary F-D `Fail` does not move the cell
    /// either — in particular it must not turn `NotRun` into `Fail`, which
    /// is the direction a naive "worst of six" implementation would break.
    #[test]
    fn a_supplementary_f_d_fail_does_not_move_the_cell_either() {
        let mut report = base_report();
        report.supplementary_f_d_bidi =
            CheckOutcome::Fail("Hebrew segment drawn in the wrong face".to_string());
        let cell = criterion_cell(&report);
        assert!(
            matches!(cell, CheckOutcome::NotRun(_)),
            "a FAIL on the supplementary row must not reach the cell at all: got {cell:?}"
        );
    }

    /// A genuine check-2 FAIL must still win the worst-of-five over the
    /// pinned check-3 NotRun — confirms the ordering is real, not just
    /// "always NotRun".
    #[test]
    fn a_check_2_failure_outranks_the_pinned_not_run_in_the_cell() {
        let mut report = base_report();
        report.check2_fallback =
            CheckOutcome::Fail("host-substituted the Hebrew segment".to_string());
        let cell = criterion_cell(&report);
        assert!(matches!(cell, CheckOutcome::Fail(_)), "{cell:?}");
    }

    // ---- is_eligible: the disqualifying set is {check2, check5} only ----

    /// Required kill: a candidate failing check 2 is ineligible.
    #[test]
    fn failing_check_2_makes_a_candidate_ineligible() {
        let mut report = base_report();
        report.check2_fallback = CheckOutcome::Fail("...".to_string());
        assert!(!is_eligible(&report));
    }

    /// Required kill: a candidate failing check 5 is ineligible.
    #[test]
    fn failing_check_5_makes_a_candidate_ineligible() {
        let mut report = base_report();
        report.check5_accessibility = CheckOutcome::Fail("...".to_string());
        assert!(!is_eligible(&report));
    }

    /// Required kill: a candidate whose only non-Pass is check 3 `NotRun`
    /// is eligible.
    #[test]
    fn a_candidate_whose_only_non_pass_is_check_3_not_run_is_eligible() {
        let report = base_report(); // check3 is NotRun; everything else Pass.
        assert!(
            is_eligible(&report),
            "check 3 is not in the disqualifying set"
        );
    }

    /// Checks 1 and 4 are not disqualifying either — only 2 and 5 are. This
    /// distinguishes "affects the cell" from "affects eligibility": a
    /// check-1 FAIL sinks the cell to FAIL but must not, by itself, remove
    /// the candidate from the round.
    #[test]
    fn failing_check_1_or_4_sinks_the_cell_but_not_eligibility() {
        let mut report = base_report();
        report.check1_faithful_consumption = CheckOutcome::Fail("...".to_string());
        assert!(
            is_eligible(&report),
            "checks 1 and 4 are not in the disqualifying set"
        );
        assert!(matches!(criterion_cell(&report), CheckOutcome::Fail(_)));
    }

    // ---- F1: a check-5 NotRun is admissible only with bus-unreachable evidence ----

    /// Required kill: a check-5 `NotRun` with no unreachable-bus evidence is
    /// rejected, naming Round 0's readback evidence.
    #[test]
    #[should_panic(expected = "READBACK: PASS")]
    fn a_check_5_not_run_with_no_evidence_is_rejected_by_is_eligible() {
        let mut report = base_report();
        report.check5_accessibility = CheckOutcome::not_run("we did not build it").unwrap();
        report.check5_bus_unreachable_evidence = None;
        let _ = is_eligible(&report);
    }

    /// Same rejection, reached through `criterion_cell` instead of
    /// `is_eligible` — both are "the scoring path" the review named.
    #[test]
    #[should_panic(expected = "READBACK: PASS")]
    fn a_check_5_not_run_with_no_evidence_is_rejected_by_criterion_cell() {
        let mut report = base_report();
        report.check5_accessibility = CheckOutcome::not_run("we did not build it").unwrap();
        report.check5_bus_unreachable_evidence = None;
        let _ = criterion_cell(&report);
    }

    /// Required kill: a check-5 `NotRun` **with** unreachable-bus evidence
    /// is accepted, and does not disqualify the candidate (NotRun is not in
    /// the disqualifying set — see [`DISQUALIFYING_CHECKS`]).
    #[test]
    fn a_check_5_not_run_with_evidence_is_accepted_and_does_not_disqualify() {
        let mut report = base_report();
        report.check5_accessibility = CheckOutcome::not_run("bus unreachable").unwrap();
        report.check5_bus_unreachable_evidence = Some(some_evidence());
        assert!(
            is_eligible(&report),
            "a legitimately NotRun check 5 must not disqualify"
        );
        let cell = criterion_cell(&report);
        assert!(matches!(cell, CheckOutcome::NotRun(_)), "{cell:?}");
    }

    /// Required kill: an `AdapterStatus::NotBuilt` entry for the round's own
    /// platform does not, by itself, make a check-5 `NotRun` admissible —
    /// `NotBuilt` covers *other* platforms as declared scope, and must not
    /// be usable to excuse AT-SPI2, the platform this round actually runs
    /// on. The admissibility check must ignore `cost.adapters` entirely.
    #[test]
    #[should_panic(expected = "READBACK: PASS")]
    fn a_not_built_adapter_for_the_rounds_own_platform_does_not_grant_admissibility() {
        let mut report = base_report();
        report.check5_accessibility = CheckOutcome::not_run("we did not build it").unwrap();
        report.check5_bus_unreachable_evidence = None;
        report.cost.adapters.push(AdapterStatus::NotBuilt {
            platform: ROUND_PLATFORM.to_string(),
            reason: "ran out of time".to_string(),
        });
        let _ = is_eligible(&report);
    }

    /// A check-5 `Pass` or `Fail` never triggers the admissibility check at
    /// all — it exists only to gate `NotRun`, and must not fire on a report
    /// that never claimed environmental absence.
    #[test]
    fn a_check_5_pass_or_fail_never_needs_bus_unreachable_evidence() {
        let mut report = base_report();
        report.check5_accessibility = CheckOutcome::Pass;
        report.check5_bus_unreachable_evidence = None;
        assert!(is_eligible(&report));
        let _ = criterion_cell(&report);

        let mut report = base_report();
        report.check5_accessibility = CheckOutcome::fail("absent-from-tree").unwrap();
        report.check5_bus_unreachable_evidence = None;
        assert!(!is_eligible(&report));
        let _ = criterion_cell(&report);
    }
}
