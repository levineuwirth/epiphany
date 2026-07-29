//! Packet 2A-ii mutation evidence (`ROUND2_TEXT_RECIPE.md` §11).
//!
//! There is no candidate raster yet — Packet 2A is candidate-neutral by
//! design (§10's opening line: "a tolerance chosen after seeing a
//! candidate's output is not a tolerance"). So this binary stands in for a
//! candidate by synthesizing its own reference raster and mutating it,
//! exactly the role Round 1's oracle tranche played when it mutated its own
//! geometry before any candidate consumed it.
//!
//! ## What is synthesized
//!
//! A 1920x1080 opaque-white canvas carrying opaque-black ink that resembles
//! text: five vertical stems (3, 4, 5, 4 and 12 device px wide), one straight
//! diagonal stroke, one curved stroke (a polyline of capsules approximating a
//! quadratic bezier), and one ring, whose enclosed centre is a small bounded
//! counter exactly like a real letterform's.
//!
//! **The stem widths straddle D1's structural floor on purpose.** D1 cannot
//! see an error confined to a stroke narrower than `2 * EDGE_BAND_PX + 1 = 5`
//! device px, because such a stroke lies wholly within the band around its own
//! edges. Four stems sit at or below that floor and one sits well above it, so
//! M3 and M3B measure the boundary from both sides instead of asserting it
//! from one. The 12 px stem is also the width the real fixtures actually have:
//! recipe §3's em size was itself re-derived from this floor, after revision 1
//! pinned 64 px em and produced 5.4 px stems that D1 would have been blind to
//! throughout.
//!
//! ## How antialiasing is computed
//!
//! Each shape is a closed 2D region with an exact `inside(x, y)` test (a
//! half-plane/rect test for stems, a distance-to-segment test for capsules,
//! an annulus test for the ring — no sqrt needed, since every comparison is
//! against a squared radius). Per-pixel coverage is estimated by supersampling:
//! an `sub x sub` regular grid of sample points inside the pixel is tested
//! against every shape, and coverage is the fraction of samples that land
//! inside *any* shape (a boolean union, so overlapping shapes do not
//! double-count). The reference raster uses `sub = 4` (16 samples/pixel).
//! The M10 "legitimate AA variant" is *not* a coarser grid — see
//! [`gamma_variant`]'s doc comment for why the `sub = 3` version turned out
//! to be an artifact of this synthesis's own coordinate choices rather than a
//! fair stand-in for a differently-tuned AA kernel; it is still run, but as a
//! non-blocking observation.
//! Luma is `255 * (1 - coverage)`: full coverage paints (0,0,0) ink, zero
//! coverage leaves (255,255,255) ground, and partial coverage is a
//! continuous grey ramp — a hard-edged reference would make the edge band
//! empty and D1 trivially (and meaninglessly) strong, which is exactly what
//! the recipe warns against.

use round2_diff::{diff, DiffReport, GlyphRegion};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

// ---------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Shape {
    Rect {
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
    },
    Capsule {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        r: f64,
    },
    Ring {
        cx: f64,
        cy: f64,
        inner: f64,
        outer: f64,
    },
}

impl Shape {
    /// Axis-aligned bounding box, `(minx, maxx, miny, maxy)`, used only to
    /// let `coverage_grid` skip the expensive test for pixels nowhere near
    /// this shape.
    fn bbox(&self) -> (f64, f64, f64, f64) {
        match *self {
            Shape::Rect { x0, x1, y0, y1 } => (x0, x1, y0, y1),
            Shape::Capsule { x0, y0, x1, y1, r } => (
                x0.min(x1) - r,
                x0.max(x1) + r,
                y0.min(y1) - r,
                y0.max(y1) + r,
            ),
            Shape::Ring { cx, cy, outer, .. } => (cx - outer, cx + outer, cy - outer, cy + outer),
        }
    }

    fn inside(&self, px: f64, py: f64) -> bool {
        match *self {
            Shape::Rect { x0, x1, y0, y1 } => px >= x0 && px <= x1 && py >= y0 && py <= y1,
            Shape::Capsule { x0, y0, x1, y1, r } => {
                let dx = x1 - x0;
                let dy = y1 - y0;
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 {
                    (((px - x0) * dx + (py - y0) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cx = x0 + t * dx;
                let cy = y0 + t * dy;
                let d2 = (px - cx).powi(2) + (py - cy).powi(2);
                d2 <= r * r
            }
            Shape::Ring {
                cx,
                cy,
                inner,
                outer,
            } => {
                let d2 = (px - cx).powi(2) + (py - cy).powi(2);
                d2 >= inner * inner && d2 <= outer * outer
            }
        }
    }
}

/// Builds the reference geometry. `drop_stem = Some(i)` omits stem `i`,
/// expressing M3 / M3B as a regenerated geometric change rather than a raster
/// patch — a paint-over would leave an antialiasing seam of its own and the
/// differential would be scoring the patch, not the removal.
fn build_geometry(drop_stem: Option<usize>) -> Vec<Shape> {
    let mut shapes = Vec::new();

    // Five vertical stems. The first four are 3/4/5/4 device px — all at or
    // below D1's structural floor of `2 * EDGE_BAND_PX + 1 = 5` px, where a
    // stroke lies entirely within the band around its own edges. The fifth is
    // 12 px, above the floor and close to the 10.8 px stems the real fixtures
    // measure at recipe §3's em size.
    //
    // Both sides of the floor are present **on purpose**: the selftest must
    // demonstrate that D1 is blind below it and sighted above it, rather than
    // assert a boundary it never crossed. M3 drops the last thin stem (D1
    // must stay silent, D4 must fire); M3B drops the thick one (D1 must fire).
    let stems: [(f64, f64); 5] = [
        (300.0, 3.0),
        (340.0, 4.0),
        (380.0, 5.0),
        (420.0, 4.0),
        (480.0, 12.0),
    ];
    for (i, &(xc, w)) in stems.iter().enumerate() {
        if drop_stem == Some(i) {
            continue;
        }
        shapes.push(Shape::Rect {
            x0: xc - w / 2.0,
            x1: xc + w / 2.0,
            y0: 200.0,
            y1: 900.0,
        });
    }

    // One straight diagonal stroke.
    shapes.push(Shape::Capsule {
        x0: 600.0,
        y0: 850.0,
        x1: 750.0,
        y1: 250.0,
        r: 2.0,
    });

    // One curved stroke: a quadratic bezier (900,850) -> control (1060,550)
    // -> (900,250), approximated as a chain of capsule segments.
    let (p0x, p0y) = (900.0f64, 850.0f64);
    let (ctlx, ctly) = (1060.0f64, 550.0f64);
    let (p1x, p1y) = (900.0f64, 250.0f64);
    const CURVE_SEGMENTS: usize = 48;
    let mut prev = (p0x, p0y);
    for i in 1..=CURVE_SEGMENTS {
        let t = i as f64 / CURVE_SEGMENTS as f64;
        let mt = 1.0 - t;
        let x = mt * mt * p0x + 2.0 * mt * t * ctlx + t * t * p1x;
        let y = mt * mt * p0y + 2.0 * mt * t * ctly + t * t * p1y;
        shapes.push(Shape::Capsule {
            x0: prev.0,
            y0: prev.1,
            x1: x,
            y1: y,
            r: 2.0,
        });
        prev = (x, y);
    }

    // One ring: the enclosed inner disc (radius 35) is the small bounded
    // counter — background surrounded on all sides by the ring's ink.
    shapes.push(Shape::Ring {
        cx: 1250.0,
        cy: 550.0,
        inner: 35.0,
        outer: 55.0,
    });

    shapes
}

// ---------------------------------------------------------------------
// Coverage / rendering
// ---------------------------------------------------------------------

/// Supersampled coverage grid: for each pixel, the fraction of an `sub x
/// sub` regular sample grid landing inside the union of `shapes`.
fn coverage_grid(shapes: &[Shape], sub: u32) -> Vec<f32> {
    let bboxes: Vec<(f64, f64, f64, f64)> = shapes.iter().map(Shape::bbox).collect();
    let mut cov = vec![0f32; (WIDTH as usize) * (HEIGHT as usize)];
    let total = (sub * sub) as f32;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut count = 0u32;
            for sy in 0..sub {
                for sx in 0..sub {
                    let px = x as f64 + (sx as f64 + 0.5) / sub as f64;
                    let py = y as f64 + (sy as f64 + 0.5) / sub as f64;
                    let hit = shapes.iter().zip(bboxes.iter()).any(|(s, bb)| {
                        px >= bb.0 && px <= bb.1 && py >= bb.2 && py <= bb.3 && s.inside(px, py)
                    });
                    if hit {
                        count += 1;
                    }
                }
            }
            cov[(y as usize) * (WIDTH as usize) + (x as usize)] = count as f32 / total;
        }
    }
    cov
}

/// Renders a coverage grid to an opaque RGBA buffer: `luma = 255 * (1 -
/// coverage)`, grayscale (ink is achromatic black-on-white per recipe §3).
fn render_rgba(cov: &[f32]) -> Vec<u8> {
    let mut buf = vec![0u8; cov.len() * 4];
    for (i, &c) in cov.iter().enumerate() {
        let c = c.clamp(0.0, 1.0);
        let l = (255.0 * (1.0 - c)).round().clamp(0.0, 255.0) as u8;
        buf[i * 4] = l;
        buf[i * 4 + 1] = l;
        buf[i * 4 + 2] = l;
        buf[i * 4 + 3] = 255;
    }
    buf
}

/// Bends the antialiasing ramp: every pixel with *partial* coverage (`0 <
/// c < 1`) is remapped to `c.powf(gamma)`; fully-covered and fully-empty
/// pixels are left exactly alone.
///
/// This is the legitimate-AA-variant technique recipe §10's own text
/// suggests ("a small gamma difference applied ONLY to partially-covered
/// pixels"), and it is deliberately used here instead of a coarser
/// supersample grid. An earlier version of this selftest tried that
/// (`coverage_grid(&geometry, 3)`, 9 samples/px, vs the reference's 16):
/// it measured a 2.26% mass delta and a **7.6 device px** centroid delta —
/// nowhere near "AA only". The cause, traced by hand: this synthesis
/// deliberately uses round shape coordinates (integer stem centres,
/// integer/half-integer widths), so several edges land at exact
/// half-integer device coordinates. A 3-sample grid's middle sample sits
/// exactly on such a boundary and the inclusive `>=` inside-test counts it
/// as ink, while a 4-sample grid has no sample there at all — a
/// **correlated** bias across every such edge, not the independent,
/// near-zero-mean noise a real renderer's coarser sampling would produce.
/// That made the sub=3 variant an artifact of this synthetic geometry's
/// coordinate choices, not a fair stand-in for "a differently-tuned real
/// AA kernel" — so it is reported as a finding below instead of kept as
/// the demonstration variant. Gamma-bending the ramp changes only the
/// pixels that are already antialiased, by construction (see the call
/// site), without that alignment artifact.
fn gamma_variant(cov: &[f32], gamma: f32) -> Vec<f32> {
    cov.iter()
        .map(|&c| if c > 0.0 && c < 1.0 { c.powf(gamma) } else { c })
        .collect()
}

/// Bilinear sample of a coverage grid at a continuous coordinate. Samples
/// falling outside `[0, WIDTH) x [0, HEIGHT)` read as coverage `0`
/// (background) — a translated or scaled raster's newly-exposed edge is
/// whatever the renderer would have drawn there, and since no shape in this
/// synthesis reaches within ~150px of any border, that value is always
/// background here, not a border artefact this selftest depends on.
fn sample_bilinear(grid: &[f32], x: f64, y: f64) -> f32 {
    let x0f = x.floor();
    let y0f = y.floor();
    let x0 = x0f as i64;
    let y0 = y0f as i64;
    let fx = (x - x0f) as f32;
    let fy = (y - y0f) as f32;
    let get = |xi: i64, yi: i64| -> f32 {
        if xi < 0 || yi < 0 || xi >= WIDTH as i64 || yi >= HEIGHT as i64 {
            0.0
        } else {
            grid[(yi as usize) * (WIDTH as usize) + (xi as usize)]
        }
    };
    let v00 = get(x0, y0);
    let v10 = get(x0 + 1, y0);
    let v01 = get(x0, y0 + 1);
    let v11 = get(x0 + 1, y0 + 1);
    let top = v00 * (1.0 - fx) + v10 * fx;
    let bot = v01 * (1.0 - fx) + v11 * fx;
    top * (1.0 - fy) + bot * fy
}

/// Resamples `grid` by translating content by `(dx, dy)` device px: output
/// pixel `(x, y)` reads input at `(x - dx, y - dy)`.
fn translate(grid: &[f32], dx: f64, dy: f64) -> Vec<f32> {
    let mut out = vec![0f32; grid.len()];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            out[(y as usize) * (WIDTH as usize) + (x as usize)] =
                sample_bilinear(grid, x as f64 - dx, y as f64 - dy);
        }
    }
    out
}

/// Resamples `grid`, scaling content by `factor` about the pixel-index
/// centre of the canvas — the inverse-mapping resample standard for a
/// "scale about centre" transform: output pixel `p` reads input at `centre
/// + (p - centre) / factor`.
fn scale_about_centre(grid: &[f32], factor: f64) -> Vec<f32> {
    let cx = (WIDTH as f64 - 1.0) / 2.0;
    let cy = (HEIGHT as f64 - 1.0) / 2.0;
    let mut out = vec![0f32; grid.len()];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let sx = cx + (x as f64 - cx) / factor;
            let sy = cy + (y as f64 - cy) / factor;
            out[(y as usize) * (WIDTH as usize) + (x as usize)] = sample_bilinear(grid, sx, sy);
        }
    }
    out
}

// ---------------------------------------------------------------------
// Report printing
// ---------------------------------------------------------------------

fn print_case(label: &str, expectation: &str, r: &DiffReport) -> bool {
    println!("--- {label} ---");
    println!("  expected to fail: {expectation}");
    println!(
        "  D1  outside-band differing pixels = {}  (pass = {})",
        r.d1_pixels_outside_band_differing, r.d1_pass
    );
    println!(
        "  D2  ref_mass = {:.3}  cand_mass = {:.3}  relative_delta = {:.4}%  (pass = {}, tolerance 2%)",
        r.reference_ink_mass,
        r.candidate_ink_mass,
        r.d2_relative_delta * 100.0,
        r.d2_pass
    );
    match (
        r.reference_centroid,
        r.candidate_centroid,
        r.d3_delta,
        r.d3_pass,
    ) {
        (Some(rc), Some(cc), Some(d), Some(pass)) => {
            println!(
                "  D3  ref_centroid = ({:.3}, {:.3})  cand_centroid = ({:.3}, {:.3})  \
                 delta = ({:.3}, {:.3}) px  (pass = {pass}, tolerance 0.5 px/axis)",
                rc.0, rc.1, cc.0, cc.1, d.0, d.1
            );
        }
        _ => {
            println!("  D3  undefined (one side has zero ink mass) — not blocking, see d3_pass doc")
        }
    }
    println!(
        "  in-band: pixel_count = {}, max|delta luma| = {}, count(|delta| > 16) = {}  (reported only)",
        r.band_pixel_count, r.in_band_max_abs_delta_luma, r.in_band_count_delta_gt_report_threshold
    );
    match &r.d4_worst {
        Some(w) => println!(
            "  D4  {} region(s); worst = {:?}  ref_mass = {:.3}  cand_mass = {:.3}  \
             relative_delta = {:.4}%  (pass = {}, tolerance {}%)",
            r.d4_regions.len(),
            w.label,
            w.reference_mass,
            w.candidate_mass,
            w.relative_delta * 100.0,
            r.d4_pass,
            round2_diff::D4_RELATIVE_TOLERANCE * 100.0
        ),
        None => println!("  D4  no regions supplied"),
    }
    for reg in r.d4_regions.iter().filter(|x| !x.pass) {
        println!(
            "        FAIL region {:?}: ref {:.3} -> cand {:.3} ({:.4}%)",
            reg.label,
            reg.reference_mass,
            reg.candidate_mass,
            reg.relative_delta * 100.0
        );
    }
    println!("  overall pass = {}", r.pass());
    r.pass()
}

/// The D4 regions, standing in for the per-glyph bounding boxes the real
/// fixtures supply. One per drawn feature, sized to the feature's own extent —
/// the crate dilates each by `D4_REGION_DILATION_PX` internally.
///
/// These are stated as literals rather than derived from `build_geometry`, on
/// purpose: a region list computed from the same function that draws the
/// shapes would shrink automatically when a shape is dropped, and the dropped
/// glyph would be scored against a region that no longer covers it — which is
/// precisely the failure D4 exists to catch, silently repaired.
fn regions() -> Vec<GlyphRegion> {
    let mut v = vec![];
    for (i, (xc, w)) in [
        (300.0, 3.0),
        (340.0, 4.0),
        (380.0, 5.0),
        (420.0, 4.0),
        (480.0, 12.0),
    ]
    .into_iter()
    .enumerate()
    {
        let half = (w as f64) / 2.0;
        v.push(GlyphRegion {
            label: format!("stem{i}(w={w})"),
            x0: (xc - half).floor() as u32,
            x1: (xc + half).ceil() as u32,
            y0: 200,
            y1: 900,
        });
    }
    v.push(GlyphRegion {
        label: "diagonal".into(),
        x0: 595,
        y0: 245,
        x1: 755,
        y1: 855,
    });
    v.push(GlyphRegion {
        label: "curve".into(),
        x0: 895,
        y0: 245,
        x1: 1065,
        y1: 855,
    });
    v.push(GlyphRegion {
        label: "ring".into(),
        x0: 1193,
        y0: 493,
        x1: 1307,
        y1: 607,
    });
    v
}

fn main() {
    let t_start = std::time::Instant::now();
    let regions = regions();

    let full_geometry = build_geometry(None);
    let cov_ref = coverage_grid(&full_geometry, 4);
    let rgba_ref = render_rgba(&cov_ref);
    round2_diff::validate_rgba(&rgba_ref, WIDTH, HEIGHT)
        .unwrap_or_else(|e| panic!("synthesized reference is malformed: {e}"));

    println!(
        "synthesized {WIDTH}x{HEIGHT} reference in {:.2}s",
        t_start.elapsed().as_secs_f64()
    );

    let selfdiff = diff(&rgba_ref, &rgba_ref, WIDTH, HEIGHT, &regions).unwrap();
    let selfcheck_ok = print_case(
        "selfcheck (reference vs itself)",
        "nothing — must pass everything",
        &selfdiff,
    );

    // Two lists, and the difference between them is the exit code.
    //
    // `failures` is the blocking set: a mutation the recipe requires to kill
    // that did not kill, or the M10 AA-only variant failing. Any entry here
    // means this binary exits non-zero, because a self-test that prints its
    // own bad news and then reports success is indistinguishable from one
    // that found nothing — which is exactly how a mutation set rots.
    //
    // `observations` is the non-blocking set: measured facts worth printing
    // that the recipe does not require anything of (today, only the
    // coarse-grid supersampling artifact, which traces to this synthesis's
    // own coordinate choices rather than to D1/D2/D3 — see
    // `gamma_variant`'s doc comment).
    let mut failures: Vec<String> = Vec::new();
    let mut observations: Vec<String> = Vec::new();
    if !selfcheck_ok {
        failures.push("selfcheck: the reference does not pass the differential against itself — the \
                        differential or the synthesis has a bug, and nothing below is trustworthy until \
                        that is fixed"
            .to_string());
    }

    // M1: translate by 1 device px in x. Must fail D1 and/or D3.
    let rgba_m1 = render_rgba(&translate(&cov_ref, 1.0, 0.0));
    round2_diff::validate_rgba(&rgba_m1, WIDTH, HEIGHT).unwrap();
    let r1 = diff(&rgba_ref, &rgba_m1, WIDTH, HEIGHT, &regions).unwrap();
    let m1_killed = !r1.d1_pass || !r1.d3_pass.unwrap_or(true);
    print_case("M1: translate 1 device px in x", "D1 and/or D3", &r1);
    if !m1_killed {
        failures.push(
            "M1 (translate 1px) did not fail D1 or D3 — the differential cannot see a whole-run \
                        1px shift"
                .to_string(),
        );
    }

    // M2: translate by 0.5 device px (resample). BOUNDARY PROBE — nothing is
    // required of it; see the explanation below and recipe §11.
    let rgba_m2 = render_rgba(&translate(&cov_ref, 0.5, 0.0));
    round2_diff::validate_rgba(&rgba_m2, WIDTH, HEIGHT).unwrap();
    let r2 = diff(&rgba_ref, &rgba_m2, WIDTH, HEIGHT, &regions).unwrap();
    // Recipe revision 2 demotes M2 to a BOUNDARY PROBE. Revision 1 required
    // it to kill D3 while setting the mutation magnitude (0.5 px) exactly
    // equal to D3's own tolerance (0.5 px) — a test of arithmetic, not of the
    // rule. Its outcome is recorded either way; nothing is required of it.
    print_case(
        "M2: translate 0.5 device px (resample) [BOUNDARY PROBE — no kill required]",
        "nothing required; recipe §10 declares D3's floor at ~1 px",
        &r2,
    );

    // M3: drop the last THIN stem (4 px, below D1's 5 px structural floor).
    // Recipe revision 2: D4 must fire. D1 is EXPECTED to stay silent — that
    // is the measured blind spot, and this case is what measures it.
    let rgba_m3 = render_rgba(&coverage_grid(&build_geometry(Some(3)), 4));
    round2_diff::validate_rgba(&rgba_m3, WIDTH, HEIGHT).unwrap();
    let r3 = diff(&rgba_ref, &rgba_m3, WIDTH, HEIGHT, &regions).unwrap();
    print_case(
        "M3: drop a 4px stem (below D1's 5px floor)",
        "D4 (and D2); D1 expected SILENT — this measures the blind spot",
        &r3,
    );
    if r3.d4_pass {
        failures.push(
            "M3 (drop a thin stem) did not fail D4 — the one rule that is supposed to catch a \
             dropped glyph did not"
                .to_string(),
        );
    }
    if !r3.d1_pass {
        failures.push(
            "M3: D1 FIRED on a 4px stem, contradicting recipe §10's stated blind spot — the \
             stated floor is wrong and the recipe must be corrected, not the expectation"
                .to_string(),
        );
    }

    // M3B: drop the 12px stem, ABOVE D1's floor. D1 must fire. Without this
    // case the blind-spot claim would be asserted from one side only.
    let rgba_m3b = render_rgba(&coverage_grid(&build_geometry(Some(4)), 4));
    round2_diff::validate_rgba(&rgba_m3b, WIDTH, HEIGHT).unwrap();
    let r3b = diff(&rgba_ref, &rgba_m3b, WIDTH, HEIGHT, &regions).unwrap();
    print_case(
        "M3B: drop the 12px stem (above D1's 5px floor)",
        "D1 AND D4 — proves the floor is a floor, not blanket blindness",
        &r3b,
    );
    if r3b.d1_pass || r3b.d4_pass {
        failures.push(format!(
            "M3B (drop a 12px stem) did not fail both D1 and D4 — d1_pass={}, d4_pass={}. If D1 \
             is blind even above its stated floor, the rule is not doing the job the recipe \
             assigns it",
            r3b.d1_pass, r3b.d4_pass
        ));
    }

    // M7: scale by 1% about the image centre. Must fail D1 and D3.
    let rgba_m7 = render_rgba(&scale_about_centre(&cov_ref, 1.01));
    round2_diff::validate_rgba(&rgba_m7, WIDTH, HEIGHT).unwrap();
    let r7 = diff(&rgba_ref, &rgba_m7, WIDTH, HEIGHT, &regions).unwrap();
    print_case("M7: scale by 1% about image centre", "D1 and D3", &r7);
    if r7.d1_pass || r7.d3_pass.unwrap_or(true) {
        failures.push(format!(
            "M7 (scale 1%) did not fail both D1 and D3 — d1_pass={}, d3_pass={:?}",
            r7.d1_pass, r7.d3_pass
        ));
    }

    // M8: blank the target entirely. Must fail D2.
    let rgba_m8 = vec![255u8; (WIDTH as usize) * (HEIGHT as usize) * 4];
    round2_diff::validate_rgba(&rgba_m8, WIDTH, HEIGHT).unwrap();
    let r8 = diff(&rgba_ref, &rgba_m8, WIDTH, HEIGHT, &regions).unwrap();
    print_case("M8: blank target to all white", "D2", &r8);
    if r8.d2_pass {
        failures.push(
            "M8 (blank target) did not fail D2 — the differential cannot see a completely \
                        missing render"
                .to_string(),
        );
    }

    // Supplementary evidence, not the demonstration variant: a coarser
    // supersample grid (9 vs the reference's 16 samples/px) over the SAME
    // geometry, printed for the record because it is exactly what surfaced
    // the coordinate-alignment artifact `gamma_variant`'s doc comment
    // explains. It is reported, not treated as a differential defect: the
    // bias traces to this synthesis's own round shape coordinates landing
    // on the coarse grid's sample offsets, not to D1/D2/D3 themselves.
    let cov_coarse = coverage_grid(&full_geometry, 3);
    let rgba_coarse = render_rgba(&cov_coarse);
    round2_diff::validate_rgba(&rgba_coarse, WIDTH, HEIGHT).unwrap();
    let r_coarse = diff(&rgba_ref, &rgba_coarse, WIDTH, HEIGHT, &regions).unwrap();
    let coarse_passed = print_case(
        "(supplementary) coarser-grid AA variant: 9-sample vs 16-sample supersampling",
        "nothing, if this were a fair AA-only variant",
        &r_coarse,
    );
    if !coarse_passed {
        observations.push(format!(
            "supplementary: a 9-sample-vs-16-sample supersample grid on this synthesis's \
             round shape coordinates does NOT pass the differential (d1_pass={}, d2_pass={}, \
             d3_pass={:?}) — traced to sample points landing exactly on shape boundaries at \
             several edges, a correlated bias specific to these coordinates rather than \
             independent AA noise; not used as the pass-demonstrating variant for that reason, \
             see gamma_variant's doc comment",
            r_coarse.d1_pass, r_coarse.d2_pass, r_coarse.d3_pass
        ));
    }

    // Legitimate AA-only variant: identical geometry, identical sampling
    // grid, but a small gamma bend applied ONLY to partially-covered pixels
    // (see gamma_variant's doc comment for why this — not a coarser
    // supersample grid — is the honest way to model "a different AA
    // kernel" here). Must PASS all three.
    let cov_variant = gamma_variant(&cov_ref, 0.97);
    let rgba_variant = render_rgba(&cov_variant);
    round2_diff::validate_rgba(&rgba_variant, WIDTH, HEIGHT).unwrap();
    let rv = diff(&rgba_ref, &rgba_variant, WIDTH, HEIGHT, &regions).unwrap();
    let variant_passed = print_case(
        "AA-only variant: same geometry/grid, gamma 0.97 on partial-coverage pixels only",
        "nothing — must PASS D1, D2, and D3",
        &rv,
    );
    if !variant_passed {
        failures.push(format!(
            "the legitimate AA-only variant did NOT pass the differential — d1_pass={}, d2_pass={}, \
             d3_pass={:?}; the tolerance is too tight, rejecting a change that is antialiasing only",
            rv.d1_pass, rv.d2_pass, rv.d3_pass
        ));
    }

    println!("\n=== observations (non-blocking) ===");
    if observations.is_empty() {
        println!("  none");
    } else {
        for o in &observations {
            println!("  OBSERVATION: {o}");
        }
    }

    println!("\n=== failures (blocking) ===");
    if failures.is_empty() {
        println!("  none: every mutation killed as required, and the legitimate AA variant passed");
    } else {
        for f in &failures {
            println!("  FAILURE: {f}");
        }
    }

    println!("\ntotal wall time: {:.2}s", t_start.elapsed().as_secs_f64());

    // The exit code is the point. An earlier version of this binary printed
    // the list above and then returned normally, so a run that reported a
    // surviving mutation still exited 0 — a self-test whose bad news is
    // invisible to every caller that checks a status is a self-test that will
    // rot without anyone noticing.
    if !failures.is_empty() {
        eprintln!(
            "\nselftest FAILED: {} blocking failure(s) — see the list above",
            failures.len()
        );
        std::process::exit(1);
    }
}
