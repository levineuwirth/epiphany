//! The shared report shape both candidates emit ([`CandidateReport`]), plus
//! serializable mirrors of `round2-diff`'s pass/fail types.
//!
//! `round2-diff` is a reviewed, frozen packet — its own `Cargo.toml` doc
//! comment states it is "deliberately zero dependencies", and this crate
//! does not modify it to add a `serde` derive it does not otherwise need.
//! [`DiffReportRecord`] and [`RegionMassRecord`] are lossless mirrors, with
//! an infallible `From` conversion, of `round2_diff::DiffReport` and
//! `round2_diff::RegionMass` — the same pattern `round2-reference`'s
//! `RegionRecord` uses for `round2_diff::GlyphRegion`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use round2_diff::{DiffReport, RegionMass};
use round2_textkit::hittest::DevicePoint;
use round2_textkit::types::SpikeCaretAffinity;

use crate::outcome::CheckOutcome;

/// Serializable mirror of `round2_diff::RegionMass`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionMassRecord {
    pub label: String,
    pub reference_mass: f64,
    pub candidate_mass: f64,
    pub relative_delta: f64,
    pub pass: bool,
}

impl From<&RegionMass> for RegionMassRecord {
    fn from(r: &RegionMass) -> Self {
        RegionMassRecord {
            label: r.label.clone(),
            reference_mass: r.reference_mass,
            candidate_mass: r.candidate_mass,
            relative_delta: r.relative_delta,
            pass: r.pass,
        }
    }
}

/// Serializable mirror of `round2_diff::DiffReport` — see this module's doc
/// comment for why this crate mirrors rather than modifies `round2-diff`.
/// `pass` is [`DiffReport::pass`]'s own computed verdict, stored rather than
/// re-derived, so a report read back from JSON does not need the four
/// D-rule fields recomputed by hand to know its own outcome.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffReportRecord {
    pub width: u32,
    pub height: u32,
    pub band_pixel_count: u64,
    pub d1_pixels_outside_band_differing: u64,
    pub d1_pass: bool,
    pub reference_ink_mass: f64,
    pub candidate_ink_mass: f64,
    pub d2_relative_delta: f64,
    pub d2_pass: bool,
    pub reference_centroid: Option<(f64, f64)>,
    pub candidate_centroid: Option<(f64, f64)>,
    pub d3_delta: Option<(f64, f64)>,
    pub d3_pass: Option<bool>,
    pub in_band_max_abs_delta_luma: u8,
    pub in_band_count_delta_gt_report_threshold: u64,
    pub d4_regions: Vec<RegionMassRecord>,
    pub d4_pass: bool,
    pub d4_worst: Option<RegionMassRecord>,
    /// [`DiffReport::pass`]'s overall verdict: D1, D2, D4 must all hold,
    /// and D3 must either hold or be inapplicable.
    pub pass: bool,
}

impl From<&DiffReport> for DiffReportRecord {
    fn from(r: &DiffReport) -> Self {
        DiffReportRecord {
            width: r.width,
            height: r.height,
            band_pixel_count: r.band_pixel_count,
            d1_pixels_outside_band_differing: r.d1_pixels_outside_band_differing,
            d1_pass: r.d1_pass,
            reference_ink_mass: r.reference_ink_mass,
            candidate_ink_mass: r.candidate_ink_mass,
            d2_relative_delta: r.d2_relative_delta,
            d2_pass: r.d2_pass,
            reference_centroid: r.reference_centroid,
            candidate_centroid: r.candidate_centroid,
            d3_delta: r.d3_delta,
            d3_pass: r.d3_pass,
            in_band_max_abs_delta_luma: r.in_band_max_abs_delta_luma,
            in_band_count_delta_gt_report_threshold: r.in_band_count_delta_gt_report_threshold,
            d4_regions: r.d4_regions.iter().map(RegionMassRecord::from).collect(),
            d4_pass: r.d4_pass,
            d4_worst: r.d4_worst.as_ref().map(RegionMassRecord::from),
            pass: r.pass(),
        }
    }
}

/// One hit-test probe's recorded comparison. The device point and expected
/// answer come straight from `round2-textkit`'s committed
/// `hittest_probes.json` (`round2_textkit::hittest::HitTestProbe`);
/// resolving *which* byte offset and affinity a candidate's renderer
/// actually returns for that point is the candidate's own job — check 4's
/// entire subject — so this type only carries the recorded outcome of that
/// resolution, never performs it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTestProbeResult {
    pub fixture_id: String,
    pub point: DevicePoint,
    pub expected_source_offset: u32,
    pub expected_affinity: SpikeCaretAffinity,
    pub actual_source_offset: u32,
    pub actual_affinity: SpikeCaretAffinity,
    pub pass: bool,
}

/// One fixture's observed accessibility evidence — what the candidate's own
/// tree (or its absence) actually looked like, compared against
/// `round2-textkit`'s precommitted
/// `round2_textkit::a11y::SpikeAccessibilityExpectation`. Building the tree
/// is the candidate's job (recipe §8.4: "nothing here says *how* a
/// candidate builds the tree, on which thread, or through which crate");
/// this type only carries what was observed.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct A11yEvidence {
    pub fixture_id: String,
    /// The platform row (recipe §8.2 table key, e.g. `"accesskit-0.24"`)
    /// this evidence was collected against — a candidate satisfies check 5
    /// by matching one row, the platform it actually exposes a tree on.
    pub platform: String,
    /// `None` when the run is absent from the tree entirely (the
    /// `absent-from-tree` prohibited outcome) — a distinct state from an
    /// empty-but-present name (`name-empty`), which is `Some("")`.
    pub observed_name: Option<String>,
    pub observed_name_bytes_hex: Option<String>,
    pub observed_role: Option<String>,
    /// One of `round2_textkit::a11y::PROHIBITED_OUTCOMES`, or `None` if no
    /// prohibited outcome applies.
    pub prohibited_outcome: Option<String>,
    pub pass: bool,
    pub notes: String,
}

/// Positive evidence that the platform accessibility bus itself was
/// unreachable — the *only* thing that can make
/// [`CandidateReport::check5_accessibility`] `NotRun` admissible on the
/// round's own platform (AT-SPI2, on this machine); see
/// `crate::scoring::ROUND0_READBACK_EVIDENCE` for why "we did not build a
/// bridge" is not, by itself, an environmental cause here.
///
/// A **typed** field rather than folding this into `CheckOutcome::NotRun`'s
/// free-text reason on purpose: a free-text reason is something a candidate
/// can write anything into ("bus unreachable" typed by hand proves
/// nothing), while this type asks for the specific thing that would make
/// the claim checkable — what was attempted, and what was actually
/// observed.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusUnreachableEvidence {
    /// How the candidate attempted to reach the platform accessibility bus
    /// before concluding it was unreachable (e.g. "connected to the AT-SPI2
    /// session bus via `atspi::Bus::connect`").
    pub probe_description: String,
    /// What was actually observed — the failure itself, not a restatement
    /// of "unreachable" (e.g. the connection error message).
    pub probe_output: String,
}

/// One dependency added to the candidate's own crate(s) over the Round 1
/// baseline. `reason` is a one-line justification a reader can check
/// against what the candidate actually needed to build.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyDelta {
    pub name: String,
    pub version: String,
    pub reason: String,
}

/// One platform accessibility adapter's status.
///
/// `NotBuilt` is a distinct variant from a failing status **on purpose** —
/// the user's ruling is that an adapter the candidate chose not to build is
/// **scope, not a hidden failure**. A string convention (e.g. a `notes`
/// field reading `"not built"`) could be typo'd, omitted, or silently
/// absorbed into a `PASS`; making it a variant the compiler enforces means
/// a report can never accidentally claim a platform is covered by leaving
/// its status ambiguous.
///
/// **This variant covers *other* platforms only** (Windows UIA, macOS AX,
/// ...) — it must never be used to excuse an unbuilt bridge on the round's
/// own platform (AT-SPI2, on this machine); see
/// [`crate::scoring::ROUND0_READBACK_EVIDENCE`] and
/// [`CandidateReport::check5_bus_unreachable_evidence`] for the field that
/// actually governs whether check 5 is allowed to be `NotRun`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AdapterStatus {
    /// The candidate built and exercised an adapter for this platform.
    Implemented { platform: String, notes: String },
    /// The candidate did not build an adapter for this platform.
    /// **Scope not covered — not a failure.**
    NotBuilt { platform: String, reason: String },
}

/// A shared part of the candidate's own integration work, common to both C1
/// and C2 so their per-part LOC tables can be read **side by side** — the
/// one thing the user's ruling on cost tables asks of this record.
///
/// Replaces an earlier free-text `part: String` design: free text let each
/// candidate invent its own vocabulary, which produced two tables that
/// could not be compared directly. `Other(String)` is the escape hatch for
/// a genuinely candidate-specific seam that none of the five shared rows
/// describes (e.g. egui's immediate-mode re-layout-per-frame glue, or
/// vello's scene-graph diffing) — the divergence between the two
/// candidates is still expressible, just visibly, instead of silently
/// fragmenting every row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ReportPart {
    TextRendering,
    HitTestResolution,
    AccessibilityTreeConstruction,
    AccessibilityIntegrationWiring,
    FixtureAndReportPlumbing,
    /// A seam that is genuinely candidate-specific — not one of the five
    /// shared rows above.
    Other(String),
}

/// LOC for one part of the candidate's own integration work.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocByPart {
    pub part: ReportPart,
    pub lines: u64,
}

/// Observed cost facts, reported at the same granularity by both
/// candidates — never a subjective score, only what was actually added,
/// built, or left as scope.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostRecord {
    /// The Round 1 baseline commit this delta is measured against.
    pub baseline_commit: String,
    pub dependencies_added: Vec<DependencyDelta>,
    /// One entry per platform row in `round2_textkit::a11y::ACCEPTED_ROLE_TABLE`
    /// — every platform gets a status, `Implemented` or `NotBuilt`, never an
    /// absent entry (an absent entry is indistinguishable from "forgot to
    /// report", which is exactly what `NotBuilt` exists to make explicit).
    pub adapters: Vec<AdapterStatus>,
    /// Free-text bullets describing integration/wiring the candidate wrote
    /// itself — glue code, not vendored or generated.
    pub integration_wiring: Vec<String>,
    pub loc_by_part: Vec<LocByPart>,
}

/// The shape both Round 2 text candidates emit.
///
/// `check1`..`check5` are the five checks [`crate::scoring::criterion_cell`]
/// reduces to the criterion cell. `supplementary_f_d_bidi` is deliberately
/// **not** one of them — see that function's doc comment for why it is
/// structurally incapable of reaching the cell.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateReport {
    pub candidate_id: String,

    pub check1_faithful_consumption: CheckOutcome,
    pub check2_fallback: CheckOutcome,
    /// Must be `CheckOutcome::NotRun(_)` by the standing ruling
    /// (`ROUND2_TEXT_RECIPE.md` §1.2) — enforced in
    /// `crate::scoring::criterion_cell` (by panic, not silent acceptance),
    /// not at construction time here, so a report can still be built and
    /// inspected before that function ever runs.
    pub check3_bidi: CheckOutcome,
    pub check4_hit_testing: CheckOutcome,
    pub check5_accessibility: CheckOutcome,
    /// Present only when `check5_accessibility` is `NotRun` **and** that
    /// `NotRun` is claimed to be caused by the platform accessibility bus
    /// itself being unreachable — the only cause
    /// `crate::scoring::criterion_cell`/`crate::scoring::is_eligible`
    /// accept for a check-5 `NotRun` on this round's own platform. `None`
    /// whenever `check5_accessibility` is `Pass` or `Fail`.
    pub check5_bus_unreachable_evidence: Option<BusUnreachableEvidence>,

    /// F-D's supplementary Hebrew/Latin bidi evidence (recipe §1.2) — a
    /// separate field, never merged into the five above and never read by
    /// `crate::scoring::criterion_cell`.
    pub supplementary_f_d_bidi: CheckOutcome,

    /// Keyed by fixture id (`F-A`..`F-E`).
    pub per_fixture_diffs: BTreeMap<String, DiffReportRecord>,
    pub hittest_probe_results: Vec<HitTestProbeResult>,
    pub a11y_evidence: Vec<A11yEvidence>,
    pub cost: CostRecord,
}

#[cfg(test)]
mod tests {
    use super::*;
    use round2_diff::GlyphRegion;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
        for px in buf.chunks_mut(4) {
            px[0] = rgb[0];
            px[1] = rgb[1];
            px[2] = rgb[2];
            px[3] = 255;
        }
        buf
    }

    #[test]
    fn diff_report_record_mirrors_every_field_and_the_computed_verdict() {
        let reference = solid(8, 8, [255, 255, 255]);
        let candidate = reference.clone();
        let region = GlyphRegion {
            label: "x".to_string(),
            x0: 2,
            y0: 2,
            x1: 6,
            y1: 6,
        };
        let report = round2_diff::diff(&reference, &candidate, 8, 8, &[region]).unwrap();
        let record = DiffReportRecord::from(&report);
        assert_eq!(record.width, report.width);
        assert_eq!(record.height, report.height);
        assert_eq!(record.d1_pass, report.d1_pass);
        assert_eq!(record.d2_pass, report.d2_pass);
        assert_eq!(record.d3_pass, report.d3_pass);
        assert_eq!(record.d4_pass, report.d4_pass);
        assert_eq!(record.pass, report.pass());
        assert_eq!(record.d4_regions.len(), report.d4_regions.len());
    }

    fn empty_cost() -> CostRecord {
        CostRecord {
            baseline_commit: "abc1234".to_string(),
            dependencies_added: Vec::new(),
            adapters: vec![AdapterStatus::NotBuilt {
                platform: "windows-uia".to_string(),
                reason: "no Windows CI runner for this spike".to_string(),
            }],
            integration_wiring: Vec::new(),
            loc_by_part: Vec::new(),
        }
    }

    fn base_candidate_report() -> CandidateReport {
        CandidateReport {
            candidate_id: "C-TEST".to_string(),
            check1_faithful_consumption: CheckOutcome::Pass,
            check2_fallback: CheckOutcome::Pass,
            check3_bidi: CheckOutcome::NotRun("x".to_string()),
            check4_hit_testing: CheckOutcome::Pass,
            check5_accessibility: CheckOutcome::Pass,
            check5_bus_unreachable_evidence: None,
            supplementary_f_d_bidi: CheckOutcome::Pass,
            per_fixture_diffs: BTreeMap::new(),
            hittest_probe_results: Vec::new(),
            a11y_evidence: Vec::new(),
            cost: empty_cost(),
        }
    }

    #[test]
    fn candidate_report_round_trips_through_json() {
        let report = base_candidate_report();
        let json = serde_json::to_string_pretty(&report).unwrap();
        let reloaded: CandidateReport = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.candidate_id, "C-TEST");
        assert!(matches!(
            reloaded.cost.adapters[0],
            AdapterStatus::NotBuilt { .. }
        ));
        assert!(reloaded.check5_bus_unreachable_evidence.is_none());
    }

    /// `check5_bus_unreachable_evidence` must round-trip when present, not
    /// just when `None` — the field the review named is exactly the one a
    /// lossy round trip would silently drop.
    #[test]
    fn bus_unreachable_evidence_round_trips_through_json() {
        let mut report = base_candidate_report();
        report.check5_accessibility = CheckOutcome::not_run("bus unreachable").unwrap();
        report.check5_bus_unreachable_evidence = Some(BusUnreachableEvidence {
            probe_description: "connected to the AT-SPI2 session bus".to_string(),
            probe_output: "org.freedesktop.DBus.Error.ServiceUnknown".to_string(),
        });
        let json = serde_json::to_string_pretty(&report).unwrap();
        let reloaded: CandidateReport = serde_json::from_str(&json).unwrap();
        let evidence = reloaded
            .check5_bus_unreachable_evidence
            .expect("evidence must survive the round trip");
        assert_eq!(
            evidence.probe_output,
            "org.freedesktop.DBus.Error.ServiceUnknown"
        );
    }

    /// `NotBuilt` must not be interchangeable with `Implemented` — the
    /// compiler-enforced distinction the doc comment claims.
    #[test]
    fn not_built_adapter_is_a_distinct_variant_from_implemented() {
        let a = AdapterStatus::NotBuilt {
            platform: "macos-nsaccessibility".to_string(),
            reason: "no macOS runner".to_string(),
        };
        assert!(matches!(a, AdapterStatus::NotBuilt { .. }));
        assert!(!matches!(a, AdapterStatus::Implemented { .. }));
    }

    /// An unknown field on the wire must be refused, not ignored — the same
    /// discipline every deserializable type in this workspace uses.
    #[test]
    fn an_unknown_field_on_cost_record_is_refused() {
        let mut v = serde_json::to_value(empty_cost()).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("smuggled_field".into(), serde_json::json!(1));
        let err = serde_json::from_value::<CostRecord>(v)
            .unwrap_err()
            .to_string();
        assert!(err.contains("smuggled_field"), "{err}");
    }

    // ---- F4: the five shared ReportPart rows compare directly ----

    /// The whole point of replacing free-text `part: String` with a fixed
    /// enum: two candidates' `LocByPart` rows for the same shared part are
    /// now directly comparable (`==`), which a free-text label (e.g. "text
    /// rendering" vs. "rendering text") could never guarantee.
    #[test]
    fn the_same_shared_part_from_two_candidates_compares_equal() {
        let c1_row = LocByPart {
            part: ReportPart::HitTestResolution,
            lines: 340,
        };
        let c2_row = LocByPart {
            part: ReportPart::HitTestResolution,
            lines: 210,
        };
        assert_eq!(c1_row.part, c2_row.part);
        assert_ne!(
            c1_row.lines, c2_row.lines,
            "the LOC counts may legitimately differ"
        );
    }

    /// `Other` stays the escape hatch: two candidate-specific seams with
    /// different labels remain distinguishable, unlike the five fixed rows.
    #[test]
    fn other_parts_with_different_labels_are_not_conflated() {
        let egui_seam = ReportPart::Other("immediate-mode re-layout per frame".to_string());
        let vello_seam = ReportPart::Other("scene-graph diffing".to_string());
        assert_ne!(egui_seam, vello_seam);
    }

    /// All five shared rows round-trip, and `Other` carries its label
    /// through — a lossy `Serialize`/`Deserialize` impl on the enum would
    /// silently collapse rows that must stay comparable.
    #[test]
    fn every_report_part_round_trips_through_json() {
        let parts = [
            ReportPart::TextRendering,
            ReportPart::HitTestResolution,
            ReportPart::AccessibilityTreeConstruction,
            ReportPart::AccessibilityIntegrationWiring,
            ReportPart::FixtureAndReportPlumbing,
            ReportPart::Other("candidate-specific seam".to_string()),
        ];
        for part in parts {
            let json = serde_json::to_string(&part).unwrap();
            let back: ReportPart = serde_json::from_str(&json).unwrap();
            assert_eq!(part, back);
        }
    }
}
