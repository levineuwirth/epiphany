//! **Packet 2B-C2**: candidate C2 (vello) against Round 2's five text checks
//! (`spec/CONTRACT_EDITOR_T4_SPIKE.md` Round 2; `ROUND2_TEXT_RECIPE.md`).
//!
//! This is a **separate entry point** from `src/main.rs` (Round 1's frozen
//! evidence, untouched by this packet) — headless offscreen rendering for
//! checks 1/2/4 plus a real windowed accessibility mode for check 5, which
//! cannot be headless (`ROUND2_TEXT_RECIPE.md` §8 / task instructions: "This
//! needs a real window on the AT-SPI bus"). **This file is
//! `c2_round2_text.rs`**, not `main.rs` — `main.rs` is the untouched Round 1
//! binary named above, and every reference in this file's own cost record
//! below to "this file" means `c2_round2_text.rs` specifically (an earlier
//! revision said `main.rs` in two places by mistake; both are review
//! findings F5, fixed here).
//!
//! Every rendering, hit-test-resolution, and accessibility decision in this
//! binary is candidate-owned (`round2-candidatekit`'s neutrality boundary,
//! its own module doc comment): this file and its `c2_round2_text/`
//! submodules never call into `round2-svgref` (the reference emitter) or
//! `round2_textkit::hittest`'s probe-*generation* functions — see
//! `c2_round2_text/render.rs` and `c2_round2_text/hittest.rs` for exactly
//! why.
//!
//! ## F3/H1 — the cost-table LOC categories, and why they are whole files
//!
//! Every [`ReportPart`] below maps to a **disjoint set of whole files** —
//! `loc_by_part_files` (in `main`) states the mapping once, prints it, and
//! [`assert_loc_by_part_is_exhaustive`] fails loudly if any `.rs` file under
//! this binary's Round 2 sources is unclaimed or claimed twice.
//! [`CostRecord::loc_by_part`] is `wc -l` over exactly the claimed files:
//!
//! | part | file(s) |
//! |---|---|
//! | `TextRendering` | `c2_round2_text/render.rs` |
//! | `HitTestResolution` | `c2_round2_text/hittest.rs` |
//! | `AccessibilityTreeConstruction` | `c2_round2_text/a11y_tree.rs` |
//! | `AccessibilityIntegrationWiring` | `c2_round2_text/a11y_wiring.rs` |
//! | `FixtureAndReportPlumbing` | `c2_round2_text.rs` (this file), `c2_round2_text/a11y_subprocess.rs` |
//!
//! No file contributes to two rows, and `FixtureAndReportPlumbing` is
//! legitimately backed by **two** files — the rule is "disjoint", not "one
//! file per part": a part may be the sum of several files, as long as no
//! file is counted under more than one part.
//!
//! **H1 ruling: the `verify.py` subprocess harness is not
//! `AccessibilityIntegrationWiring`.** `a11y_wiring.rs` used to also own
//! running the verifier, decoding its output, and reducing five fixtures'
//! outcomes to one — but that harness exists identically for both Round 2
//! candidates and is not part of either one's accessibility *stack*, so it
//! is `FixtureAndReportPlumbing` (shared spike/report plumbing), not
//! wiring. It is now `a11y_subprocess.rs`: verifier subprocesses, result
//! decoding/reduction, bus-unreachable evidence, and temporary/canonical
//! evidence-file handling. `a11y_wiring.rs` keeps only the product-side
//! path — adapter lifecycle, event loop, window/bridge setup, tree
//! publication. (Earlier still, before F3, both of those were one file with
//! `a11y_tree.rs`'s tree-construction content mixed in too, split by a
//! hand-picked LOC constant; that estimate is gone, replaced first by the
//! `a11y_tree.rs`/`a11y_wiring.rs` file boundary and now by this second one.)

#[path = "c2_round2_text/a11y_subprocess.rs"]
mod a11y_subprocess;
#[path = "c2_round2_text/a11y_tree.rs"]
mod a11y_tree;
#[path = "c2_round2_text/a11y_wiring.rs"]
mod a11y_wiring;
#[path = "c2_round2_text/hittest.rs"]
mod hittest;
#[path = "c2_round2_text/render.rs"]
mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{anyhow, Result};

use round2_candidatekit::report::{
    AdapterStatus, CostRecord, DependencyDelta, DiffReportRecord, HitTestProbeResult,
    IntegrationOwnership, LocByPart, ReportPart,
};
use round2_candidatekit::{CandidateReport, CheckOutcome};
use round2_diff::DiffReport;
use round2_textkit::faces::{resolve_declared_chain, FaceResolution, LoadedFace};
use round2_textkit::types::{SpikeResolvedText, SpikeTextDirection};

use crate::a11y_subprocess::A11yRoundResult;

/// The Round 1 baseline commit this packet's cost delta is measured against
/// (task instructions).
const BASELINE_COMMIT: &str = "c20bc93";

/// `CARGO_MANIFEST_DIR` is `.../spikes/editor-toolkit/round1-candidates/c2-vello`;
/// the spike workspace root is two levels up.
fn spike_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("spike root must exist")
}

/// Check 1 (faithful consumption): the criterion cell is the worst of all
/// five fixtures' bounded visual differential (`ROUND2_TEXT_RECIPE.md` §10).
fn check1_outcome(diffs: &BTreeMap<String, DiffReport>) -> Result<CheckOutcome> {
    let mut failing = Vec::new();
    for (id, d) in diffs {
        if !d.pass() {
            let mut rules = Vec::new();
            if !d.d1_pass {
                rules.push("D1");
            }
            if !d.d2_pass {
                rules.push("D2");
            }
            if let Some(false) = d.d3_pass {
                rules.push("D3");
            }
            if !d.d4_pass {
                rules.push("D4");
            }
            failing.push(format!("{id} ({})", rules.join(",")));
        }
    }
    if failing.is_empty() {
        Ok(CheckOutcome::Pass)
    } else {
        CheckOutcome::fail(format!(
            "bounded visual differential (recipe §10) FAILed for: {}",
            failing.join("; ")
        ))
        .map_err(|e| anyhow!(e))
    }
}

/// Check 2 (fallback, forced — DISQUALIFYING): F-C's U+0627 is covered by
/// neither declared face and must be **reported**, never substituted; F-B
/// must actually traverse the declared chain (Latin head on face 0, Hebrew
/// tail on face 1), never host-substitute. Both are checked directly against
/// the resolved data this binary drew from (never re-derived from the
/// source text), plus the two fixtures' own bounded-differential result.
fn check2_outcome(
    resolved_by_id: &BTreeMap<&str, &SpikeResolvedText>,
    diffs: &BTreeMap<String, DiffReport>,
) -> Result<CheckOutcome> {
    let f_c = *resolved_by_id
        .get("F-C")
        .ok_or_else(|| anyhow!("F-C missing from loaded fixtures"))?;
    let unresolved: Vec<_> = f_c.segments.iter().filter(|s| s.face.is_none()).collect();
    if unresolved.len() != 1 {
        return CheckOutcome::fail(format!(
            "F-C: expected exactly one unresolved (face=None) segment, found {}",
            unresolved.len()
        ))
        .map_err(|e| anyhow!(e));
    }
    let seg = unresolved[0];
    if !seg.glyphs.is_empty() {
        return CheckOutcome::fail(
            "F-C: the unresolved segment carries glyphs — this consumer must never substitute a \
             fallback/`.notdef` glyph for an uncovered codepoint"
                .to_string(),
        )
        .map_err(|e| anyhow!(e));
    }
    let range = seg.source.start as usize..seg.source.end as usize;
    let uncovered_text = f_c.text.get(range.clone()).ok_or_else(|| {
        anyhow!("F-C: unresolved segment source range is not on a UTF-8 boundary")
    })?;
    // Surfaced explicitly, per task instructions: "a candidate that silently
    // draws nothing looks identical to one that correctly reported, and the
    // check is about which of those you did."
    println!(
        "check 2 (fallback, forced): F-C reports an UNCOVERED span — byte {}..{} = {uncovered_text:?} \
         ({} codepoint(s)), resolved in NEITHER declared face. This binary drew NO ink for it and \
         substituted NOTHING — SpikeShapedSegment::glyphs is empty by construction for a face:None \
         segment (shaping is never attempted against a face that cannot represent the codepoint).",
        seg.source.start,
        seg.source.end,
        uncovered_text.chars().count()
    );

    let f_b = *resolved_by_id
        .get("F-B")
        .ok_or_else(|| anyhow!("F-B missing from loaded fixtures"))?;
    let f_b_faces: Vec<Option<u32>> = f_b.segments.iter().map(|s| s.face).collect();
    if f_b_faces != [Some(0), Some(1)] {
        return CheckOutcome::fail(format!(
            "F-B: expected two segments resolved to faces [Some(0), Some(1)] (Latin head on face \
             0, Hebrew tail on face 1 — the declared fallback traversal), got {f_b_faces:?}"
        ))
        .map_err(|e| anyhow!(e));
    }
    println!(
        "check 2 (fallback, forced): F-B traverses the declared chain — segment 0 -> face 0 \
         (Latin), segment 1 -> face 1 (Hebrew). No host substitution."
    );

    let d_c = diffs.get("F-C").expect("F-C diff already computed");
    if !d_c.pass() {
        return CheckOutcome::fail(format!(
            "F-C rendering diverged from the reference (bounded visual differential FAILed) — \
             worst region {:?}",
            d_c.d4_worst
        ))
        .map_err(|e| anyhow!(e));
    }
    let d_b = diffs.get("F-B").expect("F-B diff already computed");
    if !d_b.pass() {
        return CheckOutcome::fail(format!(
            "F-B rendering diverged from the reference (bounded visual differential FAILed) — \
             worst region {:?}",
            d_b.d4_worst
        ))
        .map_err(|e| anyhow!(e));
    }

    Ok(CheckOutcome::Pass)
}

/// Check 4 (hit testing): resolves every committed probe against this
/// binary's own `hittest::resolve_hit`, and records every result, pass and
/// fail (task instructions: "Record every probe result, pass and fail").
fn check4_outcome(
    probes: &round2_textkit::hittest::HitTestProbeFile,
    resolved_by_id: &BTreeMap<&str, &SpikeResolvedText>,
) -> Result<(CheckOutcome, Vec<HitTestProbeResult>)> {
    let mut results = Vec::new();
    let mut fail_count = 0usize;
    for table in &probes.fixtures {
        let rt = *resolved_by_id
            .get(table.fixture_id.as_str())
            .ok_or_else(|| anyhow!("{}: missing resolved text", table.fixture_id))?;
        for probe in &table.probes {
            let (actual_source_offset, actual_affinity) = hittest::resolve_hit(rt, &probe.point);
            let pass = actual_source_offset == probe.expected_source_offset
                && actual_affinity == probe.expected_affinity;
            if !pass {
                fail_count += 1;
            }
            results.push(HitTestProbeResult {
                fixture_id: table.fixture_id.clone(),
                point: probe.point,
                expected_source_offset: probe.expected_source_offset,
                expected_affinity: probe.expected_affinity,
                actual_source_offset,
                actual_affinity,
                pass,
            });
        }
    }
    let outcome = if fail_count == 0 {
        CheckOutcome::Pass
    } else {
        CheckOutcome::fail(format!(
            "{fail_count}/{} hit-test probes FAILed (recipe §7)",
            results.len()
        ))
        .map_err(|e| anyhow!(e))?
    };
    Ok((outcome, results))
}

/// F-D's supplementary bidi row (`ROUND2_TEXT_RECIPE.md` §1.2) — never
/// reaches the check 3 criterion cell (`round2_candidatekit::scoring` reads
/// only `check3_bidi`, which this binary always reports as `NotRun` per the
/// standing ruling). Verified structurally against the resolved segments
/// themselves (three segments, faces [0,1,0], directions [Ltr,Rtl,Ltr]) plus
/// F-D's own bounded-differential result.
fn supplementary_f_d_outcome(
    f_d: &SpikeResolvedText,
    diff_f_d: &DiffReport,
) -> Result<CheckOutcome> {
    let faces: Vec<Option<u32>> = f_d.segments.iter().map(|s| s.face).collect();
    if faces != [Some(0), Some(1), Some(0)] {
        return CheckOutcome::fail(format!(
            "F-D: expected segment faces [Some(0), Some(1), Some(0)] (outer Latin, inner Hebrew), \
             got {faces:?}"
        ))
        .map_err(|e| anyhow!(e));
    }
    let dirs: Vec<SpikeTextDirection> = f_d.segments.iter().map(|s| s.direction).collect();
    if dirs
        != [
            SpikeTextDirection::Ltr,
            SpikeTextDirection::Rtl,
            SpikeTextDirection::Ltr,
        ]
    {
        return CheckOutcome::fail(format!(
            "F-D: expected segment directions [Ltr, Rtl, Ltr] (three visual runs at levels \
             0/1/0), got {dirs:?}"
        ))
        .map_err(|e| anyhow!(e));
    }
    if !diff_f_d.pass() {
        return CheckOutcome::fail(format!(
            "F-D rendering diverged from the reference (bounded visual differential FAILed) — \
             worst region {:?}",
            diff_f_d.d4_worst
        ))
        .map_err(|e| anyhow!(e));
    }
    Ok(CheckOutcome::Pass)
}

fn count_file_lines(path: &std::path::Path) -> u64 {
    std::fs::read_to_string(path)
        .map(|s| s.lines().count() as u64)
        .unwrap_or(0)
}

/// J1: aggregates one `(part, lines)` pair per *file* into exactly one row
/// per distinct `ReportPart`, order-preserving (first-seen order) — the fix
/// for the serialized report carrying two rows for the same part once a
/// part gained a second contributing file (H1's split gave
/// `FixtureAndReportPlumbing` two: `a11y_subprocess.rs` and this file).
///
/// `ReportPart` derives neither `Ord` nor `Hash`, so this aggregates by
/// linear scan (`==`, from its `PartialEq` derive) rather than a
/// `BTreeMap`/`HashMap` key — fine at this scale (five parts, at most a
/// handful of files each).
fn aggregate_loc_by_part(per_file: &[(ReportPart, u64)]) -> Vec<LocByPart> {
    let mut aggregated: Vec<(ReportPart, u64)> = Vec::new();
    for (part, lines) in per_file {
        match aggregated.iter_mut().find(|(p, _)| p == part) {
            Some((_, total)) => *total += lines,
            None => aggregated.push((part.clone(), *lines)),
        }
    }
    aggregated
        .into_iter()
        .map(|(part, lines)| LocByPart { part, lines })
        .collect()
}

/// The F3/H1 file-to-part mapping, stated once. `src_dir` is
/// `src/bin/c2_round2_text/`; `this_file` is `src/bin/c2_round2_text.rs`
/// itself.
fn loc_by_part_files(
    src_dir: &std::path::Path,
    this_file: &std::path::Path,
) -> Vec<(ReportPart, PathBuf)> {
    vec![
        (ReportPart::TextRendering, src_dir.join("render.rs")),
        (ReportPart::HitTestResolution, src_dir.join("hittest.rs")),
        (
            ReportPart::AccessibilityTreeConstruction,
            src_dir.join("a11y_tree.rs"),
        ),
        (
            ReportPart::AccessibilityIntegrationWiring,
            src_dir.join("a11y_wiring.rs"),
        ),
        (
            ReportPart::FixtureAndReportPlumbing,
            src_dir.join("a11y_subprocess.rs"),
        ),
        (
            ReportPart::FixtureAndReportPlumbing,
            this_file.to_path_buf(),
        ),
    ]
}

/// H1's exhaustiveness guard: every `.rs` file actually present under this
/// binary's Round 2 sources (`src_dir`'s contents, plus `this_file`) must be
/// claimed by [`loc_by_part_files`] **exactly once** — never zero times
/// (silently uncounted cost), never twice (double-counted cost). Panics,
/// naming every file that violates either half, rather than silently
/// under- or over-reporting the table C1's identical split is compared
/// against.
fn assert_loc_by_part_is_exhaustive(
    claimed: &[(ReportPart, PathBuf)],
    src_dir: &std::path::Path,
    this_file: &std::path::Path,
) {
    let mut claim_count: BTreeMap<PathBuf, u32> = BTreeMap::new();
    for (_part, path) in claimed {
        *claim_count.entry(path.clone()).or_insert(0) += 1;
    }
    let claimed_set: BTreeSet<PathBuf> = claim_count.keys().cloned().collect();

    let mut actual: BTreeSet<PathBuf> = BTreeSet::new();
    actual.insert(this_file.to_path_buf());
    for entry in std::fs::read_dir(src_dir)
        .unwrap_or_else(|e| panic!("{}: could not list Round 2 sources: {e}", src_dir.display()))
    {
        let entry = entry.expect("readable directory entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            actual.insert(path);
        }
    }

    let unclaimed: Vec<&PathBuf> = actual.difference(&claimed_set).collect();
    let nonexistent_claims: Vec<&PathBuf> = claimed_set.difference(&actual).collect();
    let double_claimed: Vec<(&PathBuf, u32)> = claim_count
        .iter()
        .filter(|(_, &n)| n > 1)
        .map(|(p, &n)| (p, n))
        .collect();

    if !unclaimed.is_empty() || !nonexistent_claims.is_empty() || !double_claimed.is_empty() {
        panic!(
            "loc_by_part is not exhaustive/disjoint over this binary's Round 2 sources:\n  \
             unclaimed files (exist on disk, no ReportPart claims them): {unclaimed:?}\n  \
             claimed files that do not exist on disk: {nonexistent_claims:?}\n  \
             files claimed by more than one ReportPart entry: {double_claimed:?}"
        );
    }
}

fn main() -> Result<()> {
    let spike_root = spike_root();
    println!("== Packet 2B-C2: C2 (vello) against Round 2's five text checks ==");
    println!("spike root: {}", spike_root.display());

    // ---- Load the candidate-neutral apparatus (fixtures, hit-test probes,
    // reference rasters + D4 regions) — Packet 2A/2B-0, frozen, read-only. ----
    let inputs = round2_candidatekit::load_all(&spike_root).map_err(|e| anyhow!(e))?;
    println!(
        "loaded {} fixtures, {} reference rasters, {} hit-test probes",
        inputs.fixtures.fixtures.len(),
        inputs.reference.len(),
        inputs
            .hittest_probes
            .fixtures
            .iter()
            .map(|f| f.probes.len())
            .sum::<usize>()
    );

    // ---- Resolve the declared face chain (pin 9): neutral geometry/data —
    // "faces come from round2_textkit::faces" (task instructions). ----
    let resolved_faces = resolve_declared_chain();
    let mut faces: Vec<LoadedFace> = Vec::with_capacity(resolved_faces.len());
    for r in resolved_faces {
        match r {
            FaceResolution::Loaded(lf) => faces.push(lf),
            FaceResolution::Missing { path } => {
                // pin 14: an absent required face is an environment absence,
                // never a candidate failure — this run cannot proceed at all.
                anyhow::bail!(
                    "NOT RUN: declared face missing at {} — every check in this round requires \
                     the pin-9 declared chain (environment absence, not a candidate defect)",
                    path.display()
                );
            }
        }
    }
    println!("resolved {} declared faces", faces.len());

    let digest = round2_textkit::output::expected_artifact_digest().to_string();

    // ---- Checks 1, 2 (rendering half), 4's data: render every fixture
    // offscreen and diff against the frozen reference raster. ----
    let mut gpu = render::init_gpu()?;
    println!(
        "GPU: {} ({}), Vulkan",
        gpu.adapter_name, gpu.adapter_device_type
    );

    let mut diffs: BTreeMap<String, DiffReport> = BTreeMap::new();
    let mut per_fixture_diffs: BTreeMap<String, DiffReportRecord> = BTreeMap::new();
    let mut resolved_by_id: BTreeMap<&str, &SpikeResolvedText> = BTreeMap::new();
    for f in &inputs.fixtures.fixtures {
        resolved_by_id.insert(f.id.as_str(), &f.resolved);
    }

    for f in &inputs.fixtures.fixtures {
        let candidate_rgba = render::render_fixture(&mut gpu, &f.resolved, &faces)?;
        let reference = inputs
            .reference
            .get(&f.id)
            .ok_or_else(|| anyhow!("{}: no reference fixture loaded", f.id))?;
        let report = round2_diff::diff(
            &reference.reference_rgba,
            &candidate_rgba,
            round2_candidatekit::inputs::WIDTH,
            round2_candidatekit::inputs::HEIGHT,
            &reference.regions,
        )
        .map_err(|e| anyhow!("{}: diff failed: {e}", f.id))?;
        println!(
            "{}: d1={} d2={:.4}% d3={:?} d4_worst={:?} pass={}",
            f.id,
            report.d1_pixels_outside_band_differing,
            report.d2_relative_delta * 100.0,
            report.d3_delta,
            report
                .d4_worst
                .as_ref()
                .map(|w| (w.label.clone(), w.relative_delta)),
            report.pass()
        );
        per_fixture_diffs.insert(f.id.clone(), DiffReportRecord::from(&report));
        diffs.insert(f.id.clone(), report);
    }

    let check1 = check1_outcome(&diffs)?;
    let check2 = check2_outcome(&resolved_by_id, &diffs)?;
    let check3 = CheckOutcome::not_run(round2_candidatekit::scoring::CHECK_3_RULING)
        .map_err(|e| anyhow!(e))?;
    let (check4, hittest_probe_results) = check4_outcome(&inputs.hittest_probes, &resolved_by_id)?;
    let supplementary_f_d_bidi = supplementary_f_d_outcome(
        resolved_by_id["F-D"],
        diffs.get("F-D").expect("F-D diff computed"),
    )?;

    println!("check1 (faithful consumption): {check1:?}");
    println!("check2 (fallback, forced): {check2:?}");
    println!("check3 (bidi): {check3:?}");
    println!(
        "check4 (hit testing): {check4:?} ({} probes total)",
        hittest_probe_results.len()
    );
    println!("supplementary F-D bidi: {supplementary_f_d_bidi:?}");

    // ---- Check 5 (accessibility, disqualifying): a real window on the
    // AT-SPI bus, scored by the committed out-of-process verifier. ----
    let mut fixture_texts: [String; 5] = Default::default();
    for (slot, id) in a11y_subprocess::FIXTURE_ORDER.iter().enumerate() {
        fixture_texts[slot] = resolved_by_id[*id].text.clone();
    }
    let a11y_result = a11y_subprocess::run_a11y_round(&spike_root, &digest, fixture_texts)?;
    let (check5, check5_bus_unreachable_evidence, a11y_evidence) = match a11y_result {
        A11yRoundResult::Scored(evidence) => {
            for e in &evidence {
                println!(
                    "check5 {}: {} role={:?} name={:?} prohibited={:?} — {}",
                    e.fixture_id,
                    if e.pass { "PASS" } else { "FAIL" },
                    e.observed_role,
                    e.observed_name,
                    e.prohibited_outcome,
                    e.notes
                );
            }
            let failing: Vec<String> = evidence
                .iter()
                .filter(|e| !e.pass)
                .map(|e| {
                    format!(
                        "{} ({})",
                        e.fixture_id,
                        e.prohibited_outcome
                            .clone()
                            .unwrap_or_else(|| "unnamed divergence".to_string())
                    )
                })
                .collect();
            let outcome = if failing.is_empty() {
                CheckOutcome::Pass
            } else {
                CheckOutcome::fail(format!("check 5 FAILed for: {}", failing.join("; ")))
                    .map_err(|e| anyhow!(e))?
            };
            (outcome, None, evidence)
        }
        // F2: reaching this arm already means `a11y_subprocess::reduce_outcomes`
        // found no FAIL anywhere among the fixtures it *did* score — a FAIL
        // observed on any fixture, in either order relative to this
        // BusUnreachable, is reported through the `Scored` arm above
        // instead, never here. `partial_scored` still carries whatever was
        // observed before/around the bus issue, so it is not discarded.
        A11yRoundResult::BusUnreachable {
            evidence: ev,
            partial_scored,
        } => {
            println!(
                "check5: NOT RUN — AT-SPI bus unreachable mid-run (probe: {}); {} fixture(s) \
                 scored before the bus issue, none of them FAILed",
                ev.probe_description,
                partial_scored.len()
            );
            let outcome = CheckOutcome::not_run(format!(
                "AT-SPI bus unreachable mid-run: {}",
                ev.probe_output
            ))
            .map_err(|e| anyhow!(e))?;
            (outcome, Some(ev), partial_scored)
        }
    };
    println!("check5 (accessibility): {check5:?}");

    // ---- Cost record. ----
    // F3/H1: every ReportPart maps to a disjoint set of whole files (see
    // this file's own module doc comment for the table) -- printed here so
    // the mapping itself, not just the resulting counts, can be checked, and
    // checked exhaustively against what is actually on disk.
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bin/c2_round2_text");
    let this_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bin/c2_round2_text.rs");
    let loc_by_part_files = loc_by_part_files(&src_dir, &this_file);
    assert_loc_by_part_is_exhaustive(&loc_by_part_files, &src_dir, &this_file);
    println!("loc_by_part file mapping (F3/H1 -- disjoint, exhaustive, per file):");
    let mut per_file_lines: Vec<(ReportPart, u64)> = Vec::with_capacity(loc_by_part_files.len());
    for (part, path) in &loc_by_part_files {
        let lines = count_file_lines(path);
        println!("  {part:?} <- {} ({lines} lines)", path.display());
        per_file_lines.push((part.clone(), lines));
    }
    // J1: the *serialized* loc_by_part is one aggregated row per ReportPart
    // -- the printed per-file mapping above stays as the auditable detail,
    // but the report itself must not carry two rows for the same part
    // (this file used to, for FixtureAndReportPlumbing, once it gained a
    // second contributing file in H1's split) forcing every reader to know
    // to sum them.
    let loc_by_part = aggregate_loc_by_part(&per_file_lines);
    println!("loc_by_part serialized rows (J1 -- one row per ReportPart):");
    for row in &loc_by_part {
        println!("  {:?}: {} lines", row.part, row.lines);
    }

    let cost = CostRecord {
        baseline_commit: BASELINE_COMMIT.to_string(),
        dependencies_added: vec![
            DependencyDelta {
                name: "round2-candidatekit".to_string(),
                version: "0.1.0 (path)".to_string(),
                reason: "the shared fixture/oracle loader and report shape both Round 2 \
                    candidates report through (neutrality boundary)."
                    .to_string(),
            },
            DependencyDelta {
                name: "round2-diff".to_string(),
                version: "0.1.0 (path)".to_string(),
                reason: "the bounded visual differential (D1-D4) check 1 is scored against."
                    .to_string(),
            },
            DependencyDelta {
                name: "round2-textkit".to_string(),
                version: "0.1.0 (path)".to_string(),
                reason: "SpikeResolvedText and the declared-face loader/hasher (pin 9); the \
                    neutral staff->device transform (hittest::to_device)."
                    .to_string(),
            },
            DependencyDelta {
                name: "ttf-parser".to_string(),
                version: "0.25.1".to_string(),
                reason: "candidate-owned outline extraction from the resolved face into a kurbo \
                    BezPath (task instructions) — not reused from round2-svgref, whose output is \
                    an SVG path string for the reference emitter, not kurbo geometry."
                    .to_string(),
            },
            DependencyDelta {
                name: "serde_json".to_string(),
                version: "1.0.151".to_string(),
                reason: "serializes CandidateReport to JSON and parses a11y-verifier/verify.py's \
                    --json output."
                    .to_string(),
            },
            DependencyDelta {
                name: "winit".to_string(),
                version: "0.30.13".to_string(),
                reason: "check 5 needs a real window on the AT-SPI bus; matches probe-vello's \
                    Round 0 pin exactly."
                    .to_string(),
            },
            DependencyDelta {
                name: "accesskit".to_string(),
                version: "0.24.1".to_string(),
                reason: "the accessibility node/tree types this candidate builds by hand (vello \
                    ships no accessibility layer)."
                    .to_string(),
            },
            DependencyDelta {
                name: "accesskit_winit".to_string(),
                version: "0.33.2".to_string(),
                reason: "the manual winit<->accesskit bridge; matches probe-vello's Round 0 pin \
                    exactly."
                    .to_string(),
            },
        ],
        adapters: vec![
            AdapterStatus::Implemented {
                platform: "accesskit-0.24".to_string(),
                notes: "the in-process accessibility tree this binary constructs by hand \
                    (Role::Window root, Role::Paragraph children) — the same tree every platform \
                    bridge below is built from. vello ships no accessibility layer, so accesskit \
                    was absent from this crate's Round 1 dependency graph at c20bc93 and is \
                    declared here directly."
                    .to_string(),
                integration_ownership: IntegrationOwnership::CandidateOwned,
            },
            AdapterStatus::Implemented {
                platform: "at-spi2".to_string(),
                notes: format!(
                    "the round's own platform: a live window pushed through accesskit_winit \
                    0.33 -> accesskit_unix 0.22.1, read back out-of-process by \
                    a11y-verifier/verify.py (an AT-SPI2 client via gi.repository.Atspi) for all \
                    five fixtures. See {} in this run's report.",
                    "a11y_evidence"
                ),
                integration_ownership: IntegrationOwnership::CandidateOwned,
            },
            AdapterStatus::NotBuilt {
                platform: "aria".to_string(),
                reason: "no wasm/web accesskit embedding target in this spike.".to_string(),
            },
            AdapterStatus::NotBuilt {
                platform: "macos-nsaccessibility".to_string(),
                reason: "no macOS runner available to this spike.".to_string(),
            },
            AdapterStatus::NotBuilt {
                platform: "windows-uia".to_string(),
                reason: "no Windows runner available to this spike.".to_string(),
            },
        ],
        integration_wiring: vec![
            "render.rs: a ttf_parser::OutlineBuilder implementation collecting one glyph's \
                outline directly into a device-space kurbo::BezPath (scale + y-flip), reused per \
                glyph across every segment/face in a fixture; one vello::Scene::fill(NonZero) \
                call per glyph, matching the reference emitter's fill rule."
                .to_string(),
            "hittest.rs: an independent point -> (byte offset, affinity) resolver — every \
                Downstream caret stop across every cluster, converted to device space via the \
                shared to_device transform, sorted by device x, floor-looked-up against the \
                query point. Does not call any of round2_textkit::hittest's probe-generation \
                functions."
                .to_string(),
            "a11y_tree.rs: builds the accessible node content by hand — one Role::Window root \
                plus five Role::Paragraph siblings (one per fixture), each carrying that \
                fixture's exact source string as its accessible name. No platform/adapter code."
                .to_string(),
            "a11y_wiring.rs (AccessibilityIntegrationWiring, product-side only, H1/J2): manual \
                accesskit_winit wiring behind a generic, verifier-agnostic surface -- \
                run_window<T> owns the winit event loop, the accesskit_winit::Adapter's \
                lifecycle, window/bridge setup, and publishing a11y_tree.rs's tree, then blocks \
                until any caller delivers a T via FinishHandle::finish and returns it. Nothing \
                in this file names a verifier, a subprocess, or A11yRoundResult -- every line \
                here is what a real editor shipping this stack would keep."
                .to_string(),
            "a11y_subprocess.rs (FixtureAndReportPlumbing, shared spike/report harness, H1/J2): \
                drives a11y_wiring::run_window, spawning the worker thread whose only jobs are \
                calling run_all_fixtures and delivering its result -- the coordination that \
                exists solely because this spike scores itself out-of-process, moved out of the \
                wiring file entirely. run_all_fixtures itself runs a11y-verifier/verify.py once \
                per fixture as a subprocess, against a fresh run-unique --json output path \
                deleted immediately before each invocation (F1), cross-checking the exit status \
                against the json's own verdict and fixture_id fields before trusting either; \
                requires the exact 'CHECK5: NOT RUN' prefix AND one of verify.py's approved \
                environmental markers before treating exit 2 as bus-unreachable, checked as two \
                independent conditions (H2); reduces all five fixtures' outcomes to the round's \
                verdict only after every one has been attempted, so a FAIL found on any fixture \
                always wins over a BusUnreachable found on another regardless of ordering (F2). \
                This harness is common to both Round 2 candidates and is not part of either \
                one's accessibility stack."
                .to_string(),
            "c2_round2_text.rs (FixtureAndReportPlumbing): orchestration, plus the \
                check2/supplementary structural verifications (segment face-index and direction \
                checks against the resolved data actually drawn from) that \
                round2-candidatekit's neutrality boundary leaves to the candidate; the F3/H1 \
                loc_by_part file mapping and its exhaustiveness guard."
                .to_string(),
        ],
        loc_by_part,
    };

    let report = CandidateReport {
        candidate_id: "C2 vello 0.9 + kurbo (Round 2 text)".to_string(),
        check1_faithful_consumption: check1,
        check2_fallback: check2,
        check3_bidi: check3,
        check4_hit_testing: check4,
        check5_accessibility: check5,
        check5_bus_unreachable_evidence,
        supplementary_f_d_bidi,
        per_fixture_diffs,
        hittest_probe_results,
        a11y_evidence,
        cost,
    };

    let out_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("round2_report.json");
    std::fs::write(&out_path, serde_json::to_string_pretty(&report)?)?;
    println!("wrote report to {}", out_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use round2_textkit::identity::{
        SemVerRecord, SpikeShaperId, SpikeTextShapingIdentity, SpikeUnicodeComponent,
    };
    use round2_textkit::types::{
        SpikeBoundingBox, SpikeClusterMap, SpikeGlyphStyle, SpikeLanguageTag, SpikePoint,
        SpikePositionedGlyph, SpikeProvenance, SpikeScriptTag, SpikeShapedSegment, SpikeStaffSpace,
        SpikeTextAlign, SpikeTypedObjectId,
    };

    fn dummy_identity() -> SpikeTextShapingIdentity {
        SpikeTextShapingIdentity {
            faces: Vec::new(),
            shaper: SpikeShaperId("rustybuzz".to_string()),
            shaper_version: SemVerRecord {
                major: 0,
                minor: 20,
                patch: 1,
            },
            features: Vec::new(),
            unicode_bidi: SpikeUnicodeComponent {
                impl_name: "unicode-bidi".to_string(),
                crate_version: "0.3.18".to_string(),
                unicode_version: Some("16.0.0".to_string()),
            },
            unicode_segmentation: SpikeUnicodeComponent {
                impl_name: "unicode-segmentation".to_string(),
                crate_version: "1.13.3".to_string(),
                unicode_version: Some("17.0.0".to_string()),
            },
        }
    }

    fn dummy_provenance() -> SpikeProvenance {
        SpikeProvenance {
            source: SpikeTypedObjectId {
                discriminant: 0,
                canonical_bytes_hex: "00".repeat(18),
            },
            synthesis: None,
            dependencies: Vec::new(),
            stable_id: 0,
        }
    }

    fn seg(
        face: Option<u32>,
        glyphs: Vec<SpikePositionedGlyph>,
        source: std::ops::Range<u32>,
        direction: SpikeTextDirection,
    ) -> SpikeShapedSegment {
        SpikeShapedSegment {
            face,
            glyphs,
            source,
            direction,
            script: SpikeScriptTag("Latn".to_string()),
            language: SpikeLanguageTag(None),
            size: SpikeStaffSpace(1.28),
        }
    }

    fn one_glyph(id: u32) -> SpikePositionedGlyph {
        SpikePositionedGlyph {
            glyph_id: id,
            offset: SpikePoint::new(0.0, 0.0),
            transform: None,
        }
    }

    fn rt_with_segments(text: &str, segments: Vec<SpikeShapedSegment>) -> SpikeResolvedText {
        SpikeResolvedText {
            provenance: dummy_provenance(),
            text: text.to_string(),
            shaping: dummy_identity(),
            segments,
            clusters: SpikeClusterMap::default(),
            bounds: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 1.0,
                top: 1.0,
            },
            reserved_box: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 1.0,
                top: 1.0,
            },
            origin: SpikePoint::new(0.0, 0.0),
            align: SpikeTextAlign::Start,
            style: SpikeGlyphStyle { rgba: 0 },
            layer: 0,
        }
    }

    /// A minimal, honest stand-in for F-B/F-C's real resolved shape (recipe
    /// §4): F-C has an unresolved (face:None, glyphs:empty) second segment;
    /// F-B's second segment resolved to face 1 — exactly the two structural
    /// facts `check2_outcome` verifies.
    fn base_fixtures() -> BTreeMap<&'static str, SpikeResolvedText> {
        let mut m = BTreeMap::new();
        m.insert(
            "F-C",
            rt_with_segments(
                "Coro \u{0627}",
                vec![
                    seg(Some(0), vec![one_glyph(1)], 0..5, SpikeTextDirection::Ltr),
                    seg(None, vec![], 5..7, SpikeTextDirection::Rtl),
                ],
            ),
        );
        m.insert(
            "F-B",
            rt_with_segments(
                "Coro \u{05D0}\u{05D1}\u{05D2}",
                vec![
                    seg(Some(0), vec![one_glyph(1)], 0..5, SpikeTextDirection::Ltr),
                    seg(Some(1), vec![one_glyph(2)], 5..11, SpikeTextDirection::Rtl),
                ],
            ),
        );
        m
    }

    fn as_refs<'a>(
        m: &'a BTreeMap<&'static str, SpikeResolvedText>,
    ) -> BTreeMap<&'a str, &'a SpikeResolvedText> {
        m.iter().map(|(k, v)| (*k, v)).collect()
    }

    fn passing_diff() -> DiffReport {
        DiffReport {
            width: 1,
            height: 1,
            band_pixel_count: 0,
            d1_pixels_outside_band_differing: 0,
            d1_pass: true,
            reference_ink_mass: 0.0,
            candidate_ink_mass: 0.0,
            d2_relative_delta: 0.0,
            d2_pass: true,
            reference_centroid: None,
            candidate_centroid: None,
            d3_delta: None,
            d3_pass: None,
            in_band_max_abs_delta_luma: 0,
            in_band_count_delta_gt_report_threshold: 0,
            d4_regions: Vec::new(),
            d4_pass: true,
            d4_worst: None,
        }
    }

    fn passing_diffs() -> BTreeMap<String, DiffReport> {
        let mut d = BTreeMap::new();
        d.insert("F-B".to_string(), passing_diff());
        d.insert("F-C".to_string(), passing_diff());
        d
    }

    #[test]
    fn check2_passes_on_honest_data() {
        let fixtures = base_fixtures();
        let refs = as_refs(&fixtures);
        let outcome = check2_outcome(&refs, &passing_diffs()).unwrap();
        assert!(outcome.is_pass(), "{outcome:?}");
    }

    /// Mutation-first: if F-C's unresolved segment carries a glyph (a
    /// substituted fallback/`.notdef`, exactly what check 2 forbids), the
    /// check must FAIL naming that specifically — this is the guard that
    /// would catch a regression where `render.rs` started drawing something
    /// for an uncovered span.
    #[test]
    fn check2_fails_if_the_unresolved_segment_carries_a_glyph() {
        let mut fixtures = base_fixtures();
        fixtures.get_mut("F-C").unwrap().segments[1]
            .glyphs
            .push(one_glyph(999));
        let refs = as_refs(&fixtures);
        let outcome = check2_outcome(&refs, &passing_diffs()).unwrap();
        assert!(
            matches!(&outcome, CheckOutcome::Fail(r) if r.contains("carries glyphs")),
            "{outcome:?}"
        );
    }

    /// Mutation-first: if F-B's Hebrew segment never actually traversed to
    /// the second declared face (host substitution instead of fallback),
    /// the check must FAIL.
    #[test]
    fn check2_fails_if_f_b_never_traversed_to_the_second_face() {
        let mut fixtures = base_fixtures();
        fixtures.get_mut("F-B").unwrap().segments[1].face = Some(0);
        let refs = as_refs(&fixtures);
        let outcome = check2_outcome(&refs, &passing_diffs()).unwrap();
        assert!(outcome.is_fail(), "{outcome:?}");
    }

    #[test]
    fn check1_fails_and_names_the_fixture_when_a_diff_fails() {
        let mut diffs = passing_diffs();
        let mut broken = passing_diff();
        broken.d1_pass = false;
        broken.d1_pixels_outside_band_differing = 42;
        diffs.insert("F-C".to_string(), broken);
        let outcome = check1_outcome(&diffs).unwrap();
        assert!(
            matches!(&outcome, CheckOutcome::Fail(r) if r.contains("F-C") && r.contains("D1")),
            "{outcome:?}"
        );
    }

    #[test]
    fn check1_passes_when_every_diff_passes() {
        let outcome = check1_outcome(&passing_diffs()).unwrap();
        assert!(outcome.is_pass(), "{outcome:?}");
    }

    // ---- H1/F3: the loc_by_part mapping is exhaustive and disjoint over
    // the real, on-disk Round 2 sources ----

    fn real_src_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bin/c2_round2_text")
    }

    fn real_this_file() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bin/c2_round2_text.rs")
    }

    #[test]
    fn the_real_mapping_is_exhaustive_and_disjoint() {
        let src_dir = real_src_dir();
        let this_file = real_this_file();
        let claimed = loc_by_part_files(&src_dir, &this_file);
        // Must not panic.
        assert_loc_by_part_is_exhaustive(&claimed, &src_dir, &this_file);
    }

    /// Required kill: a mapping missing one real file (here, dropping
    /// `hittest.rs`'s claim) must be rejected, naming it as unclaimed —
    /// this is the exact failure mode C1 hit ("its mapping silently omitted
    /// a file") that this guard exists to catch.
    #[test]
    #[should_panic(expected = "unclaimed files")]
    fn a_mapping_missing_a_real_file_is_rejected() {
        let src_dir = real_src_dir();
        let this_file = real_this_file();
        let mut claimed = loc_by_part_files(&src_dir, &this_file);
        claimed.retain(|(_, p)| p != &src_dir.join("hittest.rs"));
        assert_loc_by_part_is_exhaustive(&claimed, &src_dir, &this_file);
    }

    /// Required kill: a file claimed by two parts must be rejected, naming
    /// it as double-claimed.
    #[test]
    #[should_panic(expected = "more than one ReportPart")]
    fn a_file_claimed_twice_is_rejected() {
        let src_dir = real_src_dir();
        let this_file = real_this_file();
        let mut claimed = loc_by_part_files(&src_dir, &this_file);
        claimed.push((ReportPart::TextRendering, src_dir.join("hittest.rs")));
        assert_loc_by_part_is_exhaustive(&claimed, &src_dir, &this_file);
    }

    /// Required kill: a claim naming a file that does not exist on disk
    /// must be rejected too — the guard is checking the *mapping*, not just
    /// counting whatever the mapping happens to list.
    #[test]
    #[should_panic(expected = "do not exist on disk")]
    fn a_claim_naming_a_nonexistent_file_is_rejected() {
        let src_dir = real_src_dir();
        let this_file = real_this_file();
        let mut claimed = loc_by_part_files(&src_dir, &this_file);
        claimed.push((
            ReportPart::Other("bogus".to_string()),
            src_dir.join("nope.rs"),
        ));
        assert_loc_by_part_is_exhaustive(&claimed, &src_dir, &this_file);
    }

    // ---- J1: the serialized loc_by_part is one aggregated row per
    // ReportPart, and each row's count is the true sum of its files ----

    /// Required kill, half 1: no `ReportPart` appears twice in the
    /// aggregated output, even though the input names
    /// `FixtureAndReportPlumbing` twice (two files).
    #[test]
    fn aggregate_loc_by_part_has_no_duplicate_parts() {
        let per_file = vec![
            (ReportPart::TextRendering, 342),
            (ReportPart::HitTestResolution, 254),
            (ReportPart::FixtureAndReportPlumbing, 736),
            (ReportPart::FixtureAndReportPlumbing, 1024),
        ];
        let aggregated = aggregate_loc_by_part(&per_file);
        let mut seen: Vec<ReportPart> = Vec::new();
        for row in &aggregated {
            assert!(
                !seen.contains(&row.part),
                "{:?} appears twice in {aggregated:?}",
                row.part
            );
            seen.push(row.part.clone());
        }
    }

    /// Required kill, half 2 — **this is the one that matters most**: each
    /// row's line count is the true sum of every file mapped to it, not
    /// (for example) the last file's count with earlier ones silently
    /// dropped. A uniqueness check alone would pass a buggy aggregator that
    /// kept only the last-seen file per part; this catches that directly.
    #[test]
    fn aggregate_loc_by_part_sums_are_correct_not_just_unique() {
        let per_file = vec![
            (ReportPart::TextRendering, 342),
            (ReportPart::HitTestResolution, 254),
            (ReportPart::FixtureAndReportPlumbing, 736),
            (ReportPart::FixtureAndReportPlumbing, 1024),
        ];
        let aggregated = aggregate_loc_by_part(&per_file);
        let find = |part: &ReportPart| aggregated.iter().find(|r| &r.part == part).unwrap();
        assert_eq!(find(&ReportPart::TextRendering).lines, 342);
        assert_eq!(find(&ReportPart::HitTestResolution).lines, 254);
        assert_eq!(
            find(&ReportPart::FixtureAndReportPlumbing).lines,
            736 + 1024,
            "must be the SUM of both contributing files, not just one of them"
        );
    }

    /// The same two properties, grounded against the real on-disk mapping
    /// rather than synthetic data — end-to-end proof that what `main` will
    /// actually serialize is exhaustive, disjoint, and correctly summed.
    #[test]
    fn the_real_aggregation_has_no_duplicate_parts_and_correct_sums() {
        let src_dir = real_src_dir();
        let this_file = real_this_file();
        let files = loc_by_part_files(&src_dir, &this_file);
        let per_file: Vec<(ReportPart, u64)> = files
            .iter()
            .map(|(p, path)| (p.clone(), count_file_lines(path)))
            .collect();
        let aggregated = aggregate_loc_by_part(&per_file);

        let mut seen: Vec<ReportPart> = Vec::new();
        for row in &aggregated {
            assert!(!seen.contains(&row.part), "{:?} appears twice", row.part);
            seen.push(row.part.clone());
        }

        let mut expected_by_part: Vec<(ReportPart, u64)> = Vec::new();
        for (part, path) in &files {
            let lines = count_file_lines(path);
            match expected_by_part.iter_mut().find(|(p, _)| p == part) {
                Some((_, total)) => *total += lines,
                None => expected_by_part.push((part.clone(), lines)),
            }
        }
        for (part, expected) in &expected_by_part {
            let row = aggregated.iter().find(|r| &r.part == part).unwrap();
            assert_eq!(row.lines, *expected, "{part:?}");
        }
    }
}
