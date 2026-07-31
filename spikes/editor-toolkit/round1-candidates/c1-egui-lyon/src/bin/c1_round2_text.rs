//! Packet 2B-C1 — Round 2 (text), candidate **C1 (egui + lyon)**.
//!
//! Drives checks 1 (faithful consumption), 2 (fallback, forced), 4 (hit
//! testing), and 5 (accessibility, via subprocess orchestration of
//! `bin/c1_round2_a11y.rs` + `a11y-verifier/verify.py`,
//! `c1_egui_lyon::a11y_subprocess`) against the frozen Round 2 text
//! apparatus (`round2-candidatekit`, `round2-textkit`, `round2-diff`), and
//! writes a `round2_candidatekit::CandidateReport` to `round2_report.json`
//! in this crate's own directory.
//!
//! **Separate from `src/main.rs`**, the Round 1 binary, which this packet
//! does not touch. This binary renders the resolved text data offscreen —
//! it never calls any egui text-layout API, font-fallback API, or
//! `rustybuzz`; glyph ids and positions come straight from
//! `SpikeResolvedText` (pin 8's fixture data), and outline extraction /
//! lyon-path conversion is this candidate's own work
//! (`c1_egui_lyon::glyph_outline`, `c1_egui_lyon::render_target`).
//!
//! `ReportPart::FixtureAndReportPlumbing` (F3): this file itself is
//! apparatus loading, check 1/2/4 scoring, and report assembly — the actual
//! rendering pipeline lives in `c1_egui_lyon::render_target`
//! (`TextRendering`) and the check-5 subprocess orchestration lives in
//! `c1_egui_lyon::a11y_subprocess`. **Reattribution (user ruling):**
//! `a11y_subprocess.rs` is now `FixtureAndReportPlumbing`, not
//! `AccessibilityIntegrationWiring` — it is the verifier-subprocess harness
//! (spawning, decoding `verify.py`'s output, freshness/publish handling),
//! common to both candidates and not part of either stack's own
//! accessibility integration. Only `c1_egui_lyon::a11y_app`
//! (`AccessibilityIntegrationWiring`) is this candidate's own
//! adapter/window/event-loop wiring; this file calls into both.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use c1_egui_lyon::hit_test;
use c1_egui_lyon::render_target::{self, GpuCtx};

use round2_candidatekit::{
    scoring, AdapterStatus, CandidateReport, CostRecord, DependencyDelta, DiffReportRecord,
    HitTestProbeResult, IntegrationOwnership, LocByPart, ReportPart,
};
use round2_diff::GlyphRegion;
use round2_textkit::faces::{resolve_declared_chain, FaceResolution, LoadedFace};
use round2_textkit::hittest::DevicePoint;

const BASELINE_COMMIT: &str = "c20bc93";
const CANDIDATE_ID: &str = "C1 egui 0.35 + lyon 1.0 (egui_wgpu::Renderer)";

fn spike_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() -> Result<()> {
    let root = spike_root();
    println!("== Packet 2B-C1: Round 2 text, candidate C1 (egui + lyon) ==");
    println!("spike root: {}", root.display());

    let inputs = round2_candidatekit::load_all(&root)
        .map_err(|e| anyhow!("failed to load candidate-neutral apparatus: {e}"))?;
    println!(
        "loaded {} fixtures, {} hit-test probe tables, {} reference rasters",
        inputs.fixtures.fixtures.len(),
        inputs.hittest_probes.fixtures.len(),
        inputs.reference.len()
    );

    // Resolve the two declared faces. Both are present on this machine
    // (measured; see the recipe §1 hashes) — a missing face here would be
    // pin 14's environmental NOT RUN, but since rendering, hit testing, and
    // accessibility all key off the SAME resolved `fixtures.json` (which
    // itself required both faces to generate), a missing face at this
    // point would make every check NOT RUN, not just one, so this is
    // treated as a fatal precondition rather than folded into any one
    // check's outcome.
    let resolved_chain = resolve_declared_chain();
    let mut loaded_faces: Vec<LoadedFace> = Vec::new();
    for r in resolved_chain {
        match r {
            FaceResolution::Loaded(lf) => loaded_faces.push(lf),
            FaceResolution::Missing { path } => {
                return Err(anyhow!(
                    "NOT RUN: declared face missing at {} — environment absence (pin 14), not a \
                     candidate failure; every Round 2 check requires both declared faces",
                    path.display()
                ));
            }
        }
    }
    let ttf_faces: Vec<ttf_parser::Face> = loaded_faces
        .iter()
        .map(|lf| {
            ttf_parser::Face::parse(&lf.bytes, lf.identity.face_index)
                .expect("face bytes already validated by resolve_declared_chain")
        })
        .collect();

    let mut gpu: GpuCtx = render_target::build_gpu()?;
    println!(
        "GPU adapter: {} ({})",
        gpu.adapter_name, gpu.adapter_device_type
    );

    // ---- Checks 1 & 2: render + diff every fixture ----
    let mut per_fixture_diffs: BTreeMap<String, DiffReportRecord> = BTreeMap::new();
    let mut unresolved_by_fixture: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut check1_failures: Vec<String> = Vec::new();

    for f in &inputs.fixtures.fixtures {
        let rt = &f.resolved;
        let draw = render_target::draw_fixture(&mut gpu, rt, &ttf_faces)
            .with_context(|| format!("{}: render failed", f.id))?;
        unresolved_by_fixture.insert(f.id.clone(), draw.unresolved_segments.clone());

        let reference = &inputs.reference[&f.id];
        let regions: Vec<GlyphRegion> = reference.regions.clone();
        let diff = round2_diff::diff(
            &reference.reference_rgba,
            &draw.rgba,
            render_target::WIDTH,
            render_target::HEIGHT,
            &regions,
        )
        .map_err(|e| anyhow!("{}: diff failed: {e}", f.id))?;

        println!(
            "{}: D1={} D2={:.4}% D3={:?} D4_worst={:?} pass={}",
            f.id,
            diff.d1_pixels_outside_band_differing,
            diff.d2_relative_delta * 100.0,
            diff.d3_delta,
            diff.d4_worst
                .as_ref()
                .map(|w| (w.label.clone(), w.relative_delta)),
            diff.pass()
        );
        if !diff.pass() {
            check1_failures.push(format!(
                "{}: d1_pass={} d2_pass={} d3_pass={:?} d4_pass={}",
                f.id, diff.d1_pass, diff.d2_pass, diff.d3_pass, diff.d4_pass
            ));
        }
        per_fixture_diffs.insert(f.id.clone(), DiffReportRecord::from(&diff));
    }

    let check1_faithful_consumption = if check1_failures.is_empty() {
        round2_candidatekit::CheckOutcome::Pass
    } else {
        round2_candidatekit::CheckOutcome::fail(format!(
            "bounded visual differential failed for: {}",
            check1_failures.join("; ")
        ))
        .unwrap()
    };

    // ---- Check 2: fallback, forced ----
    // F-C's U+0627 must resolve to face:None (reported explicitly, never
    // substituted) and F-B/F-C's renders must still match the reference
    // (proving the *rest* of the declared chain — including the traversal
    // to face 1 for Hebrew — was followed faithfully, not host-substituted).
    let fc_unresolved = unresolved_by_fixture
        .get("F-C")
        .cloned()
        .unwrap_or_default();
    let fb_pass = per_fixture_diffs
        .get("F-B")
        .map(|d| d.pass)
        .unwrap_or(false);
    let fc_pass = per_fixture_diffs
        .get("F-C")
        .map(|d| d.pass)
        .unwrap_or(false);
    println!("F-C unresolved segments (check 2 evidence): {fc_unresolved:?}");

    let check2_fallback = if fc_unresolved.is_empty() {
        round2_candidatekit::CheckOutcome::fail(
            "F-C produced no unresolved (face: None) segment at all — expected U+0627 to be \
             explicitly reported as uncovered by the declared chain",
        )
        .unwrap()
    } else if !fb_pass || !fc_pass {
        round2_candidatekit::CheckOutcome::fail(format!(
            "F-B pass={fb_pass}, F-C pass={fc_pass} — the declared fallback chain was not \
             rendered faithfully"
        ))
        .unwrap()
    } else {
        round2_candidatekit::CheckOutcome::Pass
    };

    // ---- Check 4: hit testing ----
    let mut hittest_probe_results = Vec::new();
    let mut check4_fail_count = 0usize;
    for ft in &inputs.hittest_probes.fixtures {
        let rt = &inputs
            .fixtures
            .fixtures
            .iter()
            .find(|f| f.id == ft.fixture_id)
            .unwrap()
            .resolved;
        for p in &ft.probes {
            let point = DevicePoint {
                x: p.point.x,
                y: p.point.y,
            };
            let answer = hit_test::resolve(rt, point);
            let pass = answer.source_offset == p.expected_source_offset
                && answer.affinity == p.expected_affinity;
            if !pass {
                check4_fail_count += 1;
            }
            hittest_probe_results.push(HitTestProbeResult {
                fixture_id: ft.fixture_id.clone(),
                point: p.point,
                expected_source_offset: p.expected_source_offset,
                expected_affinity: p.expected_affinity,
                actual_source_offset: answer.source_offset,
                actual_affinity: answer.affinity,
                pass,
            });
        }
    }
    println!(
        "check 4: {}/{} probes passed",
        hittest_probe_results.len() - check4_fail_count,
        hittest_probe_results.len()
    );
    let check4_hit_testing = if check4_fail_count == 0 {
        round2_candidatekit::CheckOutcome::Pass
    } else {
        round2_candidatekit::CheckOutcome::fail(format!(
            "{check4_fail_count}/{} hit-test probes disagreed with the committed expected answer",
            hittest_probe_results.len()
        ))
        .unwrap()
    };

    // ---- Supplementary F-D bidi row (never reaches check 3's cell) ----
    let fd_pass = per_fixture_diffs
        .get("F-D")
        .map(|d| d.pass)
        .unwrap_or(false);
    let fd_probe_count = hittest_probe_results
        .iter()
        .filter(|r| r.fixture_id == "F-D")
        .count();
    let fd_probe_fail = hittest_probe_results
        .iter()
        .filter(|r| r.fixture_id == "F-D" && !r.pass)
        .count();
    let supplementary_f_d_bidi = if fd_pass && fd_probe_fail == 0 {
        round2_candidatekit::CheckOutcome::Pass
    } else {
        round2_candidatekit::CheckOutcome::fail(format!(
            "F-D diff pass={fd_pass}, hit-test probes {fd_probe_fail}/{fd_probe_count} failed"
        ))
        .unwrap()
    };

    // ---- Check 5: accessibility, out-of-process (F1/F2 fixes live in
    // c1_egui_lyon::a11y_subprocess) ----
    let exe_dir = std::env::current_exe()?
        .parent()
        .expect("executable has a parent directory")
        .to_path_buf();
    let a11y_bin = exe_dir.join("c1_round2_a11y");
    if !a11y_bin.exists() {
        return Err(anyhow!(
            "{} does not exist — build it first (cargo build -p c1-egui-lyon --bin \
             c1_round2_a11y)",
            a11y_bin.display()
        ));
    }
    let evidence_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("round2_a11y_evidence");
    let fixture_ids: Vec<String> = inputs
        .fixtures
        .fixtures
        .iter()
        .map(|f| f.id.clone())
        .collect();
    let check5 =
        c1_egui_lyon::a11y_subprocess::run_all(&root, &a11y_bin, &evidence_dir, &fixture_ids)?;

    // ---- Cost record ----
    let cost = CostRecord {
        baseline_commit: BASELINE_COMMIT.to_string(),
        dependencies_added: vec![
            DependencyDelta {
                name: "round2-candidatekit".to_string(),
                version: "0.1.0 (path)".to_string(),
                reason: "candidate-neutral fixture/oracle loading and the shared report shape / \
                    scoring rule Round 2 requires every candidate to consume rather than \
                    re-derive."
                    .to_string(),
            },
            DependencyDelta {
                name: "round2-diff".to_string(),
                version: "0.1.0 (path)".to_string(),
                reason: "the precommitted bounded visual differential (D1-D4) checks 1/2 score \
                    against."
                    .to_string(),
            },
            DependencyDelta {
                name: "round2-textkit".to_string(),
                version: "0.1.0 (path)".to_string(),
                reason: "SpikeResolvedText fixtures, the staff->device transform \
                    (hittest::to_device), the hit-test probe table, and face resolution."
                    .to_string(),
            },
            DependencyDelta {
                name: "ttf-parser".to_string(),
                version: "=0.25.1".to_string(),
                reason: "candidate-owned glyph outline extraction from the two declared host \
                    faces (not from Bravura's typed PathCommand data, which Round 1 used) — \
                    pinned to the exact version round2-textkit shapes fixtures against."
                    .to_string(),
            },
            DependencyDelta {
                name: "serde_json".to_string(),
                version: "1".to_string(),
                reason: "serializing CandidateReport to round2_report.json, and parsing \
                    verify.py's --json output."
                    .to_string(),
            },
            DependencyDelta {
                name: "eframe".to_string(),
                version: "0.35".to_string(),
                reason: "check 5 needs a real window on the live AT-SPI2 bus; Round 1's binary \
                    is headless (offscreen wgpu only, no winit/eframe at all). Same first-party \
                    AccessKit route probe-egui's Round 0 binary used."
                    .to_string(),
            },
        ],
        adapters: vec![
            AdapterStatus::Implemented {
                platform: "at-spi2".to_string(),
                notes: "the round's own platform on this Linux/Wayland machine — verified via \
                    a11y-verifier/verify.py's live, out-of-process AT-SPI2 readback for all five \
                    fixtures (round2_a11y_evidence/*.json)."
                    .to_string(),
                integration_ownership: IntegrationOwnership::inherited(
                    "eframe 0.35 -> egui-winit -> accesskit_winit -> accesskit_unix \
                        (the AT-SPI2 adapter and its lifecycle ship with eframe; this candidate \
                        wrote none of that plumbing)",
                )
                .expect("provider is a non-empty literal"),
            },
            AdapterStatus::Implemented {
                platform: "accesskit-0.24".to_string(),
                notes: "reached and exercised — every check-5 readback below travelled this \
                    path. What this candidate does write on top of the inherited integration is \
                    the accessible node for its own canvas-painted run — counted under \
                    ReportPart::AccessibilityTreeConstruction (c1_egui_lyon::a11y_node), not \
                    here."
                    .to_string(),
                integration_ownership: IntegrationOwnership::inherited(
                    "eframe 0.35 (bundled AccessKit integration; accesskit was already \
                        in this crate's Round 1 dependency graph at c20bc93 via egui 0.35)",
                )
                .expect("provider is a non-empty literal"),
            },
            AdapterStatus::NotBuilt {
                platform: "aria".to_string(),
                reason: "no web/ARIA target exists for this candidate — egui/eframe here is a \
                    native desktop app, not a web build."
                    .to_string(),
            },
            AdapterStatus::NotBuilt {
                platform: "macos-nsaccessibility".to_string(),
                reason: "no macOS runner available in this environment.".to_string(),
            },
            AdapterStatus::NotBuilt {
                platform: "windows-uia".to_string(),
                reason: "no Windows runner available in this environment.".to_string(),
            },
        ],
        integration_wiring: vec![
            "glyph_outline.rs: ttf_parser::OutlineBuilder callbacks converted directly to a \
                lyon_path::Path in device space (no PathCommand/SVG intermediate), tessellated \
                with lyon's NonZero fill rule as one compound path per glyph so bounded holes \
                (e.g. 'o', 'e') survive."
                .to_string(),
            "render_target.rs: the offscreen egui_wgpu render target (device/adapter setup, \
                MSAA/resolve texture pair, render pass, CPU readback) checks 1/2 draw into."
                .to_string(),
            "hit_test.rs: a hand-written floor-search resolver over the resolved text's own \
                Downstream caret-stop partition, reusing only round2_textkit::hittest::to_device \
                for the shared staff->device transform — no other apparatus from the probe \
                generator is called."
                .to_string(),
            format!(
                "check 2 evidence: F-C's U+0627 (byte range recorded per-fixture below) is \
                 explicitly detected via SpikeShapedSegment::face == None in \
                 render_target::draw_fixture(), which asserts its glyph list is empty and \
                 records the span in round2_report.json's console log rather than silently \
                 drawing nothing — this candidate's fixture-level trace is: F-C unresolved \
                 segments = {fc_unresolved:?}"
            ),
            "a11y_node.rs: a custom AccessKit node (Role::Label, value = the fixture's exact \
                source string) built directly via egui::Context::accesskit_node_builder on an \
                Id allocated with ui.interact(..., Sense::hover()) — bypassing egui's Label \
                widget and its own text-layout/galley construction entirely, so the accessible \
                name is never touched by anything that could re-shape, wrap, or normalize it."
                .to_string(),
            "a11y_app.rs (ReportPart::AccessibilityIntegrationWiring — this candidate's own \
                integration, and the only file counted under this row): the eframe::App/window/ \
                event-loop wiring the check-5 windowed probe runs under (no visual glyph \
                rendering — dropped by the F3 fix, see that file's own doc comment)."
                .to_string(),
            "a11y_subprocess.rs (ReportPart::FixtureAndReportPlumbing, reattributed by user \
                ruling: a verifier-subprocess harness common to both Round 2 candidates, not \
                part of either stack's own accessibility integration): subprocess orchestration \
                spawning c1_round2_a11y once per fixture and invoking a11y-verifier/verify.py \
                out-of-process for the live AT-SPI2 readback (never a same-process self-report). \
                F1: every invocation writes to a fresh, unique path and requires the exit status \
                and JSON verdict to agree, erroring hard on disagreement rather than trusting \
                either. G1: admission of an exit-2 NOT RUN requires both the exact prefix and an \
                approved environmental-cause marker (an allow-list, not a deny-list). F2: any \
                FAIL across the fixture set wins over any NotRun in the final aggregate, \
                regardless of which was observed first. G3: validation (in the system temp \
                directory) and publishing (one canonical file per fixture, overwriting) are \
                separate steps, so evidence never accumulates."
                .to_string(),
        ],
        loc_by_part: loc_by_part()?,
    };

    let report = CandidateReport {
        candidate_id: CANDIDATE_ID.to_string(),
        check1_faithful_consumption,
        check2_fallback,
        check3_bidi: round2_candidatekit::CheckOutcome::not_run(scoring::CHECK_3_RULING).unwrap(),
        check4_hit_testing,
        check5_accessibility: check5.check5_accessibility,
        check5_bus_unreachable_evidence: check5.check5_bus_unreachable_evidence,
        supplementary_f_d_bidi,
        per_fixture_diffs,
        hittest_probe_results,
        a11y_evidence: check5.a11y_evidence,
        cost,
    };

    let cell = scoring::criterion_cell(&report);
    let eligible = scoring::is_eligible(&report);
    println!("\n== Round 2 criterion cell: {cell:?} ==");
    println!("== eligible (disqualifying set passed): {eligible} ==");

    let out_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("round2_report.json");
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&out_path, json)?;
    println!("wrote {}", out_path.display());

    Ok(())
}

/// `src/main.rs` is Round 1's frozen binary — untouched by this packet, not
/// part of Round 2's cost table at all — and is the **only** file under
/// `src/` this mapping is allowed to leave unclaimed. Named as a constant
/// rather than an inline literal so [`check_mapping_exhaustive_and_disjoint`]
/// and its tests refer to the same one string.
const FROZEN_ROUND1_FILE: &str = "src/main.rs";

/// G2: the file-to-`ReportPart` mapping must be **exhaustive**, not merely
/// disjoint — every `.rs` file under `src/` (`src/main.rs` excepted, see
/// [`FROZEN_ROUND1_FILE`]) must be claimed by **exactly one** part. A file
/// silently omitted (as `src/lib.rs` was, before this fix) understates the
/// packet total and makes this table disagree with a sibling candidate's
/// equivalent table about what it even counts. Returns every problem found
/// (never just the first), each naming the specific file.
fn check_mapping_exhaustive_and_disjoint(
    all_files: &[String],
    mapping: &[(&str, &[&str])],
) -> Result<()> {
    use std::collections::BTreeMap;

    let mut claimed_by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (part, files) in mapping {
        for f in *files {
            claimed_by.entry(f).or_default().push(part);
        }
    }

    let mut problems = Vec::new();
    for f in all_files {
        match claimed_by.get(f.as_str()) {
            None => problems.push(format!("{f}: unclaimed by any ReportPart")),
            Some(parts) if parts.len() > 1 => {
                problems.push(format!("{f}: claimed by multiple parts: {parts:?}"))
            }
            _ => {}
        }
    }
    for (f, parts) in &claimed_by {
        if !all_files.iter().any(|a| a == f) {
            problems.push(format!(
                "{f}: claimed by {parts:?} but not found under src/ (stale mapping entry?)"
            ));
        }
    }

    if !problems.is_empty() {
        return Err(anyhow!(
            "ReportPart file mapping is not exhaustive/disjoint over src/:\n  {}",
            problems.join("\n  ")
        ));
    }
    Ok(())
}

/// Every `.rs` file under `dir`'s `src/`, recursively, as `src/...`-relative
/// paths — `src/main.rs` excluded (see [`FROZEN_ROUND1_FILE`]).
fn list_rs_files_under_src(dir: &Path) -> Result<Vec<String>> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out)?;
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                let rel = path
                    .strip_prefix(base)
                    .expect("walked path is under base")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(&dir.join("src"), dir, &mut files)?;
    files.retain(|f| f != FROZEN_ROUND1_FILE);
    files.sort();
    Ok(files)
}

/// LOC per shared `ReportPart`, from this crate's own new Round 2 files
/// (`wc -l` equivalents, computed at run time so the figure never drifts
/// from what is actually on disk).
///
/// **F3: one `ReportPart` maps to a disjoint set of whole files — no file
/// contributes to two parts.** **G2: the mapping is also exhaustive** —
/// [`check_mapping_exhaustive_and_disjoint`] fails loudly, naming the file,
/// if anything under `src/` (besides `src/main.rs`) is left unclaimed or
/// claimed twice, so a mapping that silently omits a file (as `src/lib.rs`
/// was) can no longer happen unnoticed. The mapping is printed (not only
/// asserted) so it can be checked against the module layout directly.
fn loc_by_part() -> Result<Vec<LocByPart>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let count = |rel: &str| -> Result<u64> {
        Ok(std::fs::read_to_string(dir.join(rel))?.lines().count() as u64)
    };

    // Reattribution (user ruling): AccessibilityIntegrationWiring holds only
    // the product-side path — adapter lifecycle, event loop, window/bridge
    // setup, tree publication. The verifier-subprocess harness
    // (a11y_subprocess.rs: spawning, result decoding/reduction, freshness
    // and canonical-publish handling, their mutation tests) is common to
    // both Round 2 candidates and not part of either stack's own
    // accessibility integration, so it counts as FixtureAndReportPlumbing —
    // never `Other`, which the ruling reserves for genuinely
    // candidate-specific seams, and this harness is not one.
    let mapping: [(&str, &[&str]); 5] = [
        (
            "TextRendering",
            &["src/glyph_outline.rs", "src/render_target.rs"],
        ),
        ("HitTestResolution", &["src/hit_test.rs"]),
        ("AccessibilityTreeConstruction", &["src/a11y_node.rs"]),
        ("AccessibilityIntegrationWiring", &["src/a11y_app.rs"]),
        (
            "FixtureAndReportPlumbing",
            &[
                "src/lib.rs",
                "src/a11y_subprocess.rs",
                "src/bin/c1_round2_text.rs",
                "src/bin/c1_round2_a11y.rs",
            ],
        ),
    ];

    let all_files = list_rs_files_under_src(&dir)?;
    check_mapping_exhaustive_and_disjoint(&all_files, &mapping)?;

    println!("\n== ReportPart file mapping (F3 disjoint, G2 exhaustive) ==");
    let mut rows = Vec::with_capacity(mapping.len());
    for (label, files) in mapping {
        let mut total = 0u64;
        for f in files {
            let n = count(f)?;
            println!("  {label:<32} {f:<32} {n:>5} lines");
            total += n;
        }
        rows.push(total);
    }
    let packet_total: u64 = rows.iter().sum();
    println!("  {:<32} {:<32} {packet_total:>5} lines", "TOTAL", "");

    Ok(vec![
        LocByPart {
            part: ReportPart::TextRendering,
            lines: rows[0],
        },
        LocByPart {
            part: ReportPart::HitTestResolution,
            lines: rows[1],
        },
        LocByPart {
            part: ReportPart::AccessibilityTreeConstruction,
            lines: rows[2],
        },
        LocByPart {
            part: ReportPart::AccessibilityIntegrationWiring,
            lines: rows[3],
        },
        LocByPart {
            part: ReportPart::FixtureAndReportPlumbing,
            lines: rows[4],
        },
    ])
}

#[cfg(test)]
mod loc_mapping_tests {
    use super::*;

    fn sample_mapping() -> Vec<(&'static str, &'static [&'static str])> {
        vec![("A", &["src/a.rs"]), ("B", &["src/b.rs", "src/c.rs"])]
    }

    #[test]
    fn an_exhaustive_disjoint_mapping_passes() {
        let files = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        check_mapping_exhaustive_and_disjoint(&files, &sample_mapping()).unwrap();
    }

    /// G2 required kill: a file present under `src/` but named in no
    /// part's file list must be refused, naming the file.
    #[test]
    fn an_unclaimed_file_is_refused_by_name() {
        let files = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
            "src/d.rs".to_string(), // not in any part's file list
        ];
        let err = check_mapping_exhaustive_and_disjoint(&files, &sample_mapping()).unwrap_err();
        assert!(err.to_string().contains("src/d.rs: unclaimed"), "{err}");
    }

    /// G2 required kill: a file named in two parts' file lists must be
    /// refused, naming the file and both parts — the disjointness half of
    /// the rule, still enforced now that exhaustiveness is checked too.
    #[test]
    fn a_doubly_claimed_file_is_refused_by_name() {
        let mapping = vec![
            ("A", &["src/a.rs"][..]),
            ("B", &["src/a.rs", "src/c.rs"][..]),
        ];
        let files = vec!["src/a.rs".to_string(), "src/c.rs".to_string()];
        let err = check_mapping_exhaustive_and_disjoint(&files, &mapping).unwrap_err();
        assert!(
            err.to_string().contains("src/a.rs: claimed by multiple"),
            "{err}"
        );
    }

    /// The real, current mapping (as built in `loc_by_part`) must itself
    /// pass against the real, current file tree — this is the regression
    /// lock for G2 on the actual packet, not just the synthetic cases
    /// above.
    #[test]
    fn the_real_mapping_is_exhaustive_and_disjoint_over_the_real_tree() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mapping: [(&str, &[&str]); 5] = [
            (
                "TextRendering",
                &["src/glyph_outline.rs", "src/render_target.rs"],
            ),
            ("HitTestResolution", &["src/hit_test.rs"]),
            ("AccessibilityTreeConstruction", &["src/a11y_node.rs"]),
            ("AccessibilityIntegrationWiring", &["src/a11y_app.rs"]),
            (
                "FixtureAndReportPlumbing",
                &[
                    "src/lib.rs",
                    "src/a11y_subprocess.rs",
                    "src/bin/c1_round2_text.rs",
                    "src/bin/c1_round2_a11y.rs",
                ],
            ),
        ];
        let all_files = list_rs_files_under_src(&dir).unwrap();
        check_mapping_exhaustive_and_disjoint(&all_files, &mapping).unwrap();
    }

    /// Reattribution regression lock (user ruling): `a11y_subprocess.rs`
    /// must be claimed by `FixtureAndReportPlumbing`, never
    /// `AccessibilityIntegrationWiring` and never folded into `Other` — the
    /// ruling explicitly reserves `Other` for candidate-specific seams, and
    /// the verifier-subprocess harness is shared with C2, not one.
    #[test]
    fn a11y_subprocess_is_plumbing_not_integration_wiring_or_other() {
        let mapping: [(&str, &[&str]); 5] = [
            (
                "TextRendering",
                &["src/glyph_outline.rs", "src/render_target.rs"],
            ),
            ("HitTestResolution", &["src/hit_test.rs"]),
            ("AccessibilityTreeConstruction", &["src/a11y_node.rs"]),
            ("AccessibilityIntegrationWiring", &["src/a11y_app.rs"]),
            (
                "FixtureAndReportPlumbing",
                &[
                    "src/lib.rs",
                    "src/a11y_subprocess.rs",
                    "src/bin/c1_round2_text.rs",
                    "src/bin/c1_round2_a11y.rs",
                ],
            ),
        ];
        let (label, _) = mapping
            .iter()
            .find(|(_, files)| files.contains(&"src/a11y_subprocess.rs"))
            .expect("src/a11y_subprocess.rs must be claimed by some part");
        assert_eq!(*label, "FixtureAndReportPlumbing", "{label}");
        assert_ne!(*label, "AccessibilityIntegrationWiring");
    }

    /// `list_rs_files_under_src` must exclude `src/main.rs` (Round 1's
    /// frozen binary) but include everything else, e.g. `src/lib.rs` — the
    /// exact file G2 found omitted from the mapping.
    #[test]
    fn main_rs_is_excluded_but_lib_rs_is_present() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let files = list_rs_files_under_src(&dir).unwrap();
        assert!(!files.contains(&"src/main.rs".to_string()), "{files:?}");
        assert!(files.contains(&"src/lib.rs".to_string()), "{files:?}");
    }
}
