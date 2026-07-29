//! Round 1 shared harness: oracle loading + pixel-classification / report
//! plumbing common to both candidate binaries (`c1-egui-lyon`,
//! `c2-vello`). See the crate doc comment in `Cargo.toml` for why this crate
//! carries no GPU/wgpu dependency of its own.

use std::path::PathBuf;

use epiphany_glyphs::BravuraGlyphCatalog;
use epiphany_layout_ir::{GlyphCatalog, PathCommand};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------
// Oracle model (mirrors round1-oracle's `GlyphOracle` JSON shape exactly;
// this crate does NOT depend on the round1-oracle crate itself — it is a
// frozen, committed artifact, and re-deriving its Rust types independently
// here means a shape drift is a deserialization error, not a silent
// coupling).
//
// `deny_unknown_fields` is what actually makes that true: serde IGNORES
// unknown fields by default, so without it a field added to the oracle would
// deserialize silently and the "drift is an error" claim above would be
// false. Structural agreement is checked here; SEMANTIC agreement — that the
// oracle is internally coherent and complete — is checked by
// `OracleFile::validate`, which every candidate must call before rendering.
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleFile {
    pub contract: String,
    pub round: String,
    pub render_transform_rule: String,
    pub flatten_tolerance_staff_space: f64,
    pub clearance_floor_device_px: f64,
    pub glyphs: Vec<GlyphOracle>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum Requirement {
    BoundedHole,
    DisjointComponents,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum SampleClass {
    Ink,
    Background,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub scale: f64,
    pub tx: f64,
    pub ty: f64,
    pub target_width: f64,
    pub target_height: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleEvidence {
    pub inside_outer_contour: bool,
    pub even_odd_filled: bool,
    pub nonzero_filled: bool,
    pub outer_contour_ring_index: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamplePoint {
    pub staff: (f64, f64),
    pub device: (f64, f64),
    pub class: SampleClass,
    pub clearance_device_px: f64,
    pub hole_evidence: Option<HoleEvidence>,
    pub subpath_index: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleDiagnostic {
    pub raw_hole_grid_hits: u64,
    pub best_hole_clearance_device_px: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlyphOracle {
    pub name: String,
    pub requirement: Requirement,
    pub subpath_count: usize,
    pub expected_subpath_count: Option<usize>,
    pub transform: Transform,
    pub bbox_staff: [f64; 4],
    pub outer_contour_ring_index: usize,
    pub ring_signed_areas: Vec<f64>,
    pub points: Vec<SamplePoint>,
    pub ink_candidates_found: usize,
    pub background_candidates_found: usize,
    pub ink_spacing_relaxed: bool,
    pub background_spacing_relaxed: bool,
    pub ink_satisfied: bool,
    pub background_required: bool,
    pub background_satisfied: bool,
    pub subpath_coverage_required: bool,
    pub subpath_coverage_satisfied: bool,
    pub satisfied: bool,
    pub hole_diagnostic: HoleDiagnostic,
}

// ---------------------------------------------------------------------
// The Round 1 roster, restated here as literals.
//
// These are NOT read from the oracle — restating them is the entire point.
// A validator that only checks the oracle against itself accepts any
// self-consistent oracle, including one with a glyph deleted, a glyph renamed,
// or every target resized together. The contract (Revision 6) names this exact
// roster, so the harness names it too, and a candidate refuses to render
// against anything else.
// ---------------------------------------------------------------------

/// `(name, requirement, subpath_count, point_count)` — CONTRACT_EDITOR_T4_SPIKE
/// Revision 6, Round 1. Four bounded-hole glyphs at 3 ink + 3 hole background
/// each, plus `fClef` at one tagged ink point per disjoint component.
const ROUND1_ROSTER: [(&str, Requirement, usize, usize); 5] = [
    ("gClef", Requirement::BoundedHole, 4, 6),
    ("timeSig8", Requirement::BoundedHole, 3, 6),
    ("accidentalFlat", Requirement::BoundedHole, 2, 6),
    ("noteheadHalf", Requirement::BoundedHole, 2, 6),
    ("fClef", Requirement::DisjointComponents, 3, 3),
];

/// Total sample points across the roster: 4 x 6 + 3. Checked as a census so a
/// point silently dropped from one glyph cannot be absorbed by the per-glyph
/// minimums.
const ROUND1_POINT_CENSUS: usize = 27;

/// Pin 4's offscreen target. Exact, not "whatever the oracle agrees on with
/// itself": scaling every glyph's target together stays self-consistent while
/// changing the rasterization the round is meant to compare.
const TARGET_WIDTH: f64 = 1920.0;
const TARGET_HEIGHT: f64 = 1080.0;

/// Minimum distance from any sample point to the nearest outline edge. This is
/// what makes the >= 8 px claim load-bearing: below it, a sample can land in
/// the antialiased band, where a correct render legitimately produces a
/// mid-grey and the luminance threshold decides the verdict instead of the
/// geometry.
const CLEARANCE_FLOOR_DEVICE_PX: f64 = 8.0;

/// Loads the frozen `round1-oracle/oracle.json` relative to this crate's
/// manifest directory (`round1-candidates/harness/../../round1-oracle`).
/// Never writes to that path; read-only.
pub fn load_oracle() -> OracleFile {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../round1-oracle/oracle.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read oracle at {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse oracle at {}: {e}", path.display()))
}

impl OracleFile {
    /// Checks the oracle is internally coherent and complete **before** any
    /// candidate renders against it.
    ///
    /// `deny_unknown_fields` catches *structural* drift; this catches semantic
    /// drift, which is the dangerous kind: an oracle that deserializes cleanly
    /// but whose expectations no longer mean what Round 1 requires would be
    /// tested against faithfully and pass, and the run would look green.
    ///
    /// Every expectation below is checked against a **literal restated here**
    /// (`ROUND1_ROSTER`, `TARGET_WIDTH`/`_HEIGHT`, `CLEARANCE_FLOOR_DEVICE_PX`,
    /// `ROUND1_POINT_CENSUS`) rather than against the oracle's own other
    /// fields. An earlier version compared each glyph's target to the *first
    /// glyph's* target and each subpath count to the oracle's own
    /// `expected_subpath_count`, which accepts any self-consistent file:
    /// deleting `noteheadHalf`, renaming a glyph, or resizing every target
    /// together all validated cleanly.
    ///
    /// Enforced:
    /// - the glyph set is exactly the five roster names, no duplicates, each
    ///   with its roster requirement, subpath count, and point count, and 27
    ///   points in total;
    /// - every target is exactly 1920x1080;
    /// - the declared clearance floor is 8 px and every sample meets it;
    /// - no glyph's point search fell back to relaxed spacing;
    /// - every glyph is `satisfied`, with `ink_satisfied` and the
    ///   requirement-specific status flags coherent for its class;
    /// - hole glyphs carry >= 3 ink and >= 3 bounded-hole background points,
    ///   with hole evidence on each;
    /// - `fClef`-class glyphs carry no background requirement and >= 1 tagged
    ///   ink point in **every** subpath.
    pub fn validate(&self) -> Result<(), String> {
        if self.round != "round1" {
            return Err(format!(
                "oracle declares round {:?}, not \"round1\"",
                self.round
            ));
        }
        if self.clearance_floor_device_px != CLEARANCE_FLOOR_DEVICE_PX {
            return Err(format!(
                "oracle declares a clearance floor of {} device px, not {CLEARANCE_FLOOR_DEVICE_PX} \
                 — lowering it lets samples sit in the antialiased band, where the threshold, not \
                 the geometry, decides the verdict",
                self.clearance_floor_device_px
            ));
        }

        // Exact set equality, in both directions: same length, every roster
        // name present, and no name appearing twice (so a duplicate cannot
        // stand in for a deleted glyph and keep the count right).
        if self.glyphs.len() != ROUND1_ROSTER.len() {
            let names: Vec<&str> = self.glyphs.iter().map(|g| g.name.as_str()).collect();
            return Err(format!(
                "oracle carries {} glyphs {names:?}, not the {} Round 1 requires",
                self.glyphs.len(),
                ROUND1_ROSTER.len()
            ));
        }
        for (i, g) in self.glyphs.iter().enumerate() {
            if self.glyphs[..i].iter().any(|o| o.name == g.name) {
                return Err(format!("glyph {:?} appears more than once", g.name));
            }
        }
        let total_points: usize = self.glyphs.iter().map(|g| g.points.len()).sum();
        if total_points != ROUND1_POINT_CENSUS {
            return Err(format!(
                "oracle carries {total_points} sample points in total, not the \
                 {ROUND1_POINT_CENSUS} Round 1 requires"
            ));
        }

        for (name, requirement, subpaths, point_count) in ROUND1_ROSTER {
            let g = self
                .glyphs
                .iter()
                .find(|g| g.name == name)
                .ok_or_else(|| format!("oracle is missing required Round 1 glyph {name:?}"))?;
            let at = |m: String| format!("{}: {m}", g.name);

            if g.requirement != requirement {
                return Err(at(format!(
                    "carries requirement {:?}, but Round 1 tests it for {requirement:?}",
                    g.requirement
                )));
            }
            if g.subpath_count != subpaths || g.expected_subpath_count != Some(subpaths) {
                return Err(at(format!(
                    "subpath_count {} / expected {:?} disagrees with the {subpaths} subpaths \
                     Round 1 names for this glyph",
                    g.subpath_count, g.expected_subpath_count
                )));
            }
            if g.points.len() != point_count {
                return Err(at(format!(
                    "carries {} sample points, not the {point_count} Round 1 names",
                    g.points.len()
                )));
            }
            if !g.satisfied || !g.ink_satisfied {
                return Err(at(format!(
                    "oracle records satisfied = {} / ink_satisfied = {}",
                    g.satisfied, g.ink_satisfied
                )));
            }
            if g.ink_spacing_relaxed || g.background_spacing_relaxed {
                return Err(at(
                    "point search fell back to relaxed spacing — the recorded points are closer \
                     together than the round's design, so they no longer probe independent regions"
                        .to_string(),
                ));
            }
            if g.transform.target_width != TARGET_WIDTH
                || g.transform.target_height != TARGET_HEIGHT
            {
                return Err(at(format!(
                    "target is {}x{}, not pin 4's {TARGET_WIDTH}x{TARGET_HEIGHT}",
                    g.transform.target_width, g.transform.target_height
                )));
            }
            for (i, p) in g.points.iter().enumerate() {
                if !(p.clearance_device_px >= CLEARANCE_FLOOR_DEVICE_PX) {
                    return Err(at(format!(
                        "point {i} at device ({}, {}) has clearance {} device px, below the \
                         {CLEARANCE_FLOOR_DEVICE_PX} px floor",
                        p.device.0, p.device.1, p.clearance_device_px
                    )));
                }
            }

            let ink: Vec<&SamplePoint> = g
                .points
                .iter()
                .filter(|p| p.class == SampleClass::Ink)
                .collect();
            let bg: Vec<&SamplePoint> = g
                .points
                .iter()
                .filter(|p| p.class == SampleClass::Background)
                .collect();

            match g.requirement {
                Requirement::BoundedHole => {
                    if !g.background_required
                        || !g.background_satisfied
                        || g.subpath_coverage_required
                    {
                        return Err(at(format!(
                            "BoundedHole must require and satisfy background points and must not \
                             claim subpath coverage; has background_required = {}, \
                             background_satisfied = {}, subpath_coverage_required = {}",
                            g.background_required,
                            g.background_satisfied,
                            g.subpath_coverage_required
                        )));
                    }
                    if ink.len() < 3 || bg.len() < 3 {
                        return Err(at(format!(
                            "BoundedHole needs >= 3 ink and >= 3 background points, has {} and {}",
                            ink.len(),
                            bg.len()
                        )));
                    }
                    for p in &bg {
                        let e = p.hole_evidence.as_ref().ok_or_else(|| {
                            at("background point carries no hole evidence".to_string())
                        })?;
                        // Unfilled under BOTH rules, not just even-odd: that
                        // agreement is the measured fact criterion 1 rests on
                        // (Bravura's contours are correctly oppositely wound),
                        // so a point the two rules disagree about is not a
                        // bounded hole this round can test with.
                        if !e.inside_outer_contour || e.even_odd_filled || e.nonzero_filled {
                            return Err(at(format!(
                                "background point at device ({}, {}) is not an unambiguously \
                                 bounded hole: inside_outer_contour = {}, even_odd_filled = {}, \
                                 nonzero_filled = {}",
                                p.device.0,
                                p.device.1,
                                e.inside_outer_contour,
                                e.even_odd_filled,
                                e.nonzero_filled
                            )));
                        }
                        if e.outer_contour_ring_index != g.outer_contour_ring_index {
                            return Err(at(format!(
                                "background point names outer ring {} but the glyph names {}",
                                e.outer_contour_ring_index, g.outer_contour_ring_index
                            )));
                        }
                    }
                }
                Requirement::DisjointComponents => {
                    if g.background_required
                        || !g.subpath_coverage_required
                        || !g.subpath_coverage_satisfied
                    {
                        return Err(at(
                            "DisjointComponents must require subpath coverage and no background"
                                .to_string(),
                        ));
                    }
                    if !bg.is_empty() {
                        return Err(at(format!(
                            "DisjointComponents must carry no background points, has {}",
                            bg.len()
                        )));
                    }
                    let mut covered: Vec<usize> = ink
                        .iter()
                        .filter_map(|p| p.subpath_index)
                        .collect::<Vec<_>>();
                    covered.sort_unstable();
                    covered.dedup();
                    if covered.len() != g.subpath_count
                        || covered.first() != Some(&0)
                        || covered.last() != Some(&(g.subpath_count - 1))
                    {
                        return Err(at(format!(
                            "ink points cover subpaths {covered:?}, not every index in \
                             0..{} — the check that catches a largest-contour-only tessellator \
                             would not fire",
                            g.subpath_count
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Fetches one glyph's typed outline from the real `BravuraGlyphCatalog` —
/// the same catalog `round1-oracle` derived its geometry from.
pub fn outline_for(name: &str) -> Vec<PathCommand> {
    let catalog = BravuraGlyphCatalog;
    catalog
        .render_data(name)
        .unwrap_or_else(|| panic!("{name}: BravuraGlyphCatalog has no render data"))
        .outline
}

// ---------------------------------------------------------------------
// Pixel classification + report model
// ---------------------------------------------------------------------

/// Ink-vs-background classification of one sampled RGBA pixel.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PixelClass {
    Ink,
    Background,
}

/// Classifies `rgba` by luminance against `threshold` (0..=255): a pixel
/// whose luminance is strictly below `threshold` is `Ink` (opaque black
/// fill), otherwise `Background` (opaque white). Since every oracle sample
/// point sits >= 8 device px from any outline edge, every real sample should
/// land solidly at (0,0,0,255) or (255,255,255,255) — the exact threshold
/// value barely matters for a passing render, but a stated one is required
/// so a FAIL's root cause is never "which threshold did you mean".
pub fn classify_pixel(rgba: [u8; 4], threshold: u8) -> PixelClass {
    let [r, g, b, _a] = rgba;
    // Standard luma weights (Rec. 601), integer-approximated.
    let luma = (299 * r as u32 + 587 * g as u32 + 114 * b as u32) / 1000;
    if luma < threshold as u32 {
        PixelClass::Ink
    } else {
        PixelClass::Background
    }
}

pub fn expected_class(oracle_class: SampleClass) -> PixelClass {
    match oracle_class {
        SampleClass::Ink => PixelClass::Ink,
        SampleClass::Background => PixelClass::Background,
    }
}

#[derive(Clone, Debug)]
pub struct PointResult {
    pub staff: (f64, f64),
    pub device: (f64, f64),
    pub oracle_class: SampleClass,
    pub subpath_index: Option<usize>,
    pub sampled_rgba: [u8; 4],
    pub actual_class: PixelClass,
    pub expected_class: PixelClass,
    pub pass: bool,
}

#[derive(Clone, Debug)]
pub struct GlyphResult {
    pub name: String,
    pub points: Vec<PointResult>,
}

impl GlyphResult {
    pub fn all_pass(&self) -> bool {
        self.points.iter().all(|p| p.pass)
    }
}

#[derive(Clone, Debug)]
pub struct AdapterInfo {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub vendor_id: u32,
    pub device_id: u32,
}

#[derive(Clone, Debug)]
pub struct RunReport {
    pub candidate: String,
    pub adapter: AdapterInfo,
    /// The **nominal** sample count. Both candidates report 8, as pin 4
    /// requires, but the number alone overstates the agreement — see
    /// `aa_mechanism`, which is printed beside it for exactly that reason.
    pub msaa_samples: u32,
    /// How that sample count is actually achieved. C1 uses a hardware
    /// multisample render-target attachment; C2 uses vello's compute-shader
    /// antialiasing into a single-sample storage texture. Recording only the
    /// integer would let a later round read "8 == 8" as "same work", which it
    /// is not — and at Round 4 that difference is in the deciding numbers.
    pub aa_mechanism: String,
    pub target_format: String,
    pub luminance_threshold: u8,
    pub fill_rule: String,
    pub glyphs: Vec<GlyphResult>,
    pub notes: Vec<String>,
}

impl RunReport {
    pub fn all_pass(&self) -> bool {
        self.glyphs.iter().all(|g| g.all_pass())
    }

    /// Renders the full per-glyph x per-point table as plain text.
    pub fn table(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "=== {} on {} ({}) [backend={}, vendor=0x{:04x}, device=0x{:04x}] ===\n",
            self.candidate,
            self.adapter.name,
            self.adapter.device_type,
            self.adapter.backend,
            self.adapter.vendor_id,
            self.adapter.device_id,
        ));
        out.push_str(&format!(
            "msaa={} ({}) format={} luminance_threshold<{} fill_rule={}\n",
            self.msaa_samples,
            self.aa_mechanism,
            self.target_format,
            self.luminance_threshold,
            self.fill_rule
        ));
        for g in &self.glyphs {
            out.push_str(&format!(
                "--- {} ({}/{} points PASS) ---\n",
                g.name,
                g.points.iter().filter(|p| p.pass).count(),
                g.points.len()
            ));
            out.push_str(
                "  idx  class       subpath  device(x,y)              rgba              verdict\n",
            );
            for (i, p) in g.points.iter().enumerate() {
                out.push_str(&format!(
                    "  {:>3}  {:<10}  {:<7}  ({:>8.2},{:>8.2})  ({:>3},{:>3},{:>3},{:>3})  {}\n",
                    i,
                    format!("{:?}", p.oracle_class),
                    p.subpath_index
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    p.device.0,
                    p.device.1,
                    p.sampled_rgba[0],
                    p.sampled_rgba[1],
                    p.sampled_rgba[2],
                    p.sampled_rgba[3],
                    if p.pass { "PASS" } else { "FAIL" },
                ));
            }
        }
        if !self.notes.is_empty() {
            out.push_str("notes:\n");
            for n in &self.notes {
                out.push_str(&format!("  - {n}\n"));
            }
        }
        out
    }
}

/// Rounds a device-space coordinate to the nearest pixel index, **erroring
/// rather than clamping** when it falls outside `[0, dim)`.
///
/// Clamping was the earlier behaviour and it is unsafe here: an out-of-range
/// coordinate silently becomes an edge pixel, which for a glyph centred in a
/// 1920x1080 target is always background — so a mis-transformed ink point
/// would read as a candidate FAIL, and a mis-transformed background point as a
/// PASS. Either way the harness would be reporting on a pixel the oracle never
/// named.
pub fn device_index(v: f64, dim: u32) -> Result<u32, String> {
    let r = v.round();
    if r < 0.0 || r >= dim as f64 {
        return Err(format!(
            "device coordinate {v} rounds to {r}, outside [0, {dim}) — the render target does not \
             cover the oracle's sample point"
        ));
    }
    Ok(r as u32)
}

/// Builds a `GlyphResult` from an already-rendered RGBA readback buffer
/// (tightly packed, `width * height * 4` bytes, row-major top-to-bottom —
/// standard wgpu texture-copy layout after unpadding row strides) by
/// sampling each oracle point's `device` pixel.
///
/// **Every failure mode here is an error, never a substituted sample.** A
/// short buffer previously yielded `(0,0,0,0)`, whose luma is 0 — it
/// classifies as *ink*, so a truncated readback would have made every ink
/// point pass. Buffer length, coordinate range, and sample opacity are all
/// checked, because each of them can turn a broken run into a green one.
pub fn evaluate_glyph(
    oracle: &GlyphOracle,
    width: u32,
    height: u32,
    rgba: &[u8],
    threshold: u8,
) -> Result<GlyphResult, String> {
    let expect_len = (width as usize) * (height as usize) * 4;
    if rgba.len() != expect_len {
        return Err(format!(
            "{}: readback buffer is {} bytes, expected exactly {expect_len} ({width}x{height} \
             RGBA) — a short or padded buffer cannot be sampled safely",
            oracle.name,
            rgba.len()
        ));
    }
    let mut points = Vec::with_capacity(oracle.points.len());
    for p in &oracle.points {
        let px = device_index(p.device.0, width).map_err(|e| format!("{}: x: {e}", oracle.name))?;
        let py =
            device_index(p.device.1, height).map_err(|e| format!("{}: y: {e}", oracle.name))?;
        let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
        let sampled = [rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]];
        if sampled[3] != 255 {
            return Err(format!(
                "{}: sample at device ({}, {}) has alpha {} — the target must be fully opaque; a \
                 transparent sample means the clear or the blend is wrong, not that the candidate \
                 drew the wrong colour",
                oracle.name, p.device.0, p.device.1, sampled[3]
            ));
        }
        let actual = classify_pixel(sampled, threshold);
        let expected = expected_class(p.class);
        points.push(PointResult {
            staff: p.staff,
            device: p.device,
            oracle_class: p.class,
            subpath_index: p.subpath_index,
            sampled_rgba: sampled,
            actual_class: actual,
            expected_class: expected,
            pass: actual == expected,
        });
    }
    Ok(GlyphResult {
        name: oracle.name.clone(),
        points,
    })
}
