//! The bounded visual differential (`ROUND2_TEXT_RECIPE.md` §10): pure image
//! math over two RGBA raster buffers, with no dependency on how either one
//! was produced. Ruling A permitted "geometry/scene equivalence plus a
//! bounded visual differential under a controlled backend, NOT pixel
//! equality"; this crate is where that phrase finally gets a number, fixed
//! **before** either candidate's raster exists (see the `selftest` binary,
//! which stands in for a candidate the way Round 1's oracle tranche mutated
//! its own geometry before any candidate did).
//!
//! Four rules decide, all hard (§10):
//!
//! - **D1** — outside a band around reference edges, zero pixels may differ
//!   in ink class.
//! - **D2** — whole-image ink mass (Σ(255 − luma)/255) agrees within 2%.
//! - **D3** — the whole-image ink centroid agrees within 0.5 device px per
//!   axis, for *gross* misplacement only; its detection floor is declared on
//!   [`D3_TOLERANCE_DEVICE_PX`].
//! - **D4** — **per-glyph** ink mass agrees within 2%, for every glyph.
//!
//! **D4 is the rule that catches a wrong, dropped, or re-shaped glyph, and it
//! exists because recipe revision 1 shipped without it and was wrong.** That
//! revision assigned the job to D1, and the mutation set proved D1 cannot do
//! it: D1 is structurally blind to any error confined to a stroke narrower
//! than `2 * EDGE_BAND_PX + 1 = 5` device px, because such a stroke lies
//! entirely inside the band around its own edges. Measured here — deleting a
//! 4 px stem gives `d1 = 0` differing pixels while D4 reports 100%; deleting a
//! 12 px stem, above the floor, gives `d1 = 4164`. The floor is measured from
//! both sides rather than asserted from one.
//!
//! Two more numbers are *reported inside the band but never decide anything*
//! — the max |Δluma| and the count of pixels differing by more than 16 —
//! because inside the band a difference is expected to be antialiasing, and
//! that is precisely what this differential is bounded *against* measuring.

/// Rec. 601 integer luma weights, identical to
/// `round1-candidates/harness`'s `classify_pixel` — the same convention
/// Round 1 used, so "ink" means the same thing in both rounds.
fn luma(rgba: [u8; 4]) -> u8 {
    let [r, g, b, _a] = rgba;
    ((299 * r as u32 + 587 * g as u32 + 114 * b as u32) / 1000) as u8
}

/// A pixel is ink if its luma is strictly below this. Recipe §10: "Ink =
/// luma < 128." Not a tunable — the recipe fixes it, so it is a `const`,
/// not a parameter a caller could quietly relax.
pub const INK_LUMA_THRESHOLD: u8 = 128;

/// The band radius, in Chebyshev distance, around a reference edge pixel.
/// Recipe §10 / §11: `EDGE_BAND_PX = 2`, "the same device Round 1 used for
/// its 8 px clearance floor" — confine D1 to where the answer is geometric,
/// not a coin flip about antialiasing.
pub const EDGE_BAND_PX: i32 = 2;

/// D2's relative tolerance on ink mass: 2% (recipe §10).
pub const D2_RELATIVE_TOLERANCE: f64 = 0.02;

/// D3's per-axis tolerance on the ink centroid, in device px (recipe §10).
///
/// **D3's declared detection floor.** A whole-image centroid is one number
/// over two million pixels. Measured on this crate's own synthetic reference,
/// a legitimate antialiasing-only variant moved it 0.346 px while a true
/// 0.5 px translation moved it 0.486 px — not separable. So D3 does **not**
/// detect uniform drift below roughly 1 device px, and the recipe does not
/// claim it does; it is retained for gross misplacement, where it is decisive
/// (deleting one stem moved it 40.7 px). Sub-pixel registration is out of
/// scope for this round, declared in advance rather than inferred later from
/// a candidate's numbers.
pub const D3_TOLERANCE_DEVICE_PX: f64 = 0.5;

/// D4's relative tolerance on **per-glyph** ink mass: 2% (recipe §10).
pub const D4_RELATIVE_TOLERANCE: f64 = 0.02;

/// Each glyph region is dilated by this many device px before its mass is
/// measured, so a glyph whose ink legitimately spills a fraction of a pixel
/// past its reported bounding box (antialiasing does exactly this) is not
/// scored on a clipped footprint.
pub const D4_REGION_DILATION_PX: u32 = 3;

/// One glyph's device-space footprint, for D4. Half-open in the same sense as
/// the raster: `x0..x1`, `y0..y1`.
#[derive(Clone, Debug)]
pub struct GlyphRegion {
    /// Human-readable identity for the report — a `FAIL` names the glyph, not
    /// an index into a list the reader does not have.
    pub label: String,
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// One region's D4 outcome, kept per region so a `FAIL` says *which glyph*.
#[derive(Clone, Debug)]
pub struct RegionMass {
    pub label: String,
    pub reference_mass: f64,
    pub candidate_mass: f64,
    pub relative_delta: f64,
    pub pass: bool,
}

/// The in-band "reported, not deciding" delta-luma threshold (recipe §10):
/// pixels inside the band whose |Δluma| exceeds this are counted, but the
/// count never feeds a pass/fail verdict.
pub const IN_BAND_REPORT_DELTA_LUMA: u8 = 16;

fn is_ink(rgba: [u8; 4]) -> bool {
    (luma(rgba) as u32) < INK_LUMA_THRESHOLD as u32
}

fn pixel_at(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y as usize) * (width as usize) + (x as usize)) * 4;
    [rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]]
}

/// Checks `rgba` is exactly `width * height * 4` bytes and every pixel is
/// fully opaque, **before** anything reads a single sample out of it.
///
/// Both checks exist for the same reason Round 1's `evaluate_glyph` checks
/// them: a short buffer indexed with a substituted default reads back as
/// `(0,0,0,0)`, whose luma is 0 — that classifies as *ink*, so a truncated
/// or otherwise malformed readback would silently turn a broken run green.
/// This function is the single gate every other function in this crate
/// calls before it touches a pixel, so "malformed buffer" is always an
/// `Err`, never a value.
pub fn validate_rgba(rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    let expected_len = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected_len {
        return Err(format!(
            "buffer is {} bytes, expected exactly {expected_len} ({width}x{height} RGBA) — a \
             short or padded buffer cannot be sampled safely",
            rgba.len()
        ));
    }
    for y in 0..height {
        for x in 0..width {
            let a = pixel_at(rgba, width, x, y)[3];
            if a != 255 {
                return Err(format!(
                    "pixel at ({x}, {y}) has alpha {a}, not 255 — the target must be fully \
                     opaque; a transparent pixel means the clear or the blend is wrong, not \
                     that the differential should guess a colour for it"
                ));
            }
        }
    }
    Ok(())
}

/// Total ink mass: Σ over every pixel of `(255 - luma) / 255`. Recipe §10,
/// D2's quantity. A fully black image has mass `width * height`; a fully
/// white image has mass `0`.
pub fn ink_mass(rgba: &[u8], width: u32, height: u32) -> Result<f64, String> {
    validate_rgba(rgba, width, height)?;
    let mut mass = 0.0f64;
    for y in 0..height {
        for x in 0..width {
            let l = luma(pixel_at(rgba, width, x, y));
            mass += (255 - l) as f64 / 255.0;
        }
    }
    Ok(mass)
}

/// The mass-weighted ink centroid, using the same per-pixel weight as
/// [`ink_mass`]. Returns `Ok(None)` rather than dividing by zero when the
/// image carries no ink at all (mass `0`) — a blank image has no centroid
/// to report, and a `NaN` snuck into a report would be worse than an
/// explicit "undefined".
pub fn centroid(rgba: &[u8], width: u32, height: u32) -> Result<Option<(f64, f64)>, String> {
    validate_rgba(rgba, width, height)?;
    let mut mass = 0.0f64;
    let mut sx = 0.0f64;
    let mut sy = 0.0f64;
    for y in 0..height {
        for x in 0..width {
            let l = luma(pixel_at(rgba, width, x, y));
            let w = (255 - l) as f64 / 255.0;
            mass += w;
            sx += w * x as f64;
            sy += w * y as f64;
        }
    }
    if mass <= 0.0 {
        return Ok(None);
    }
    Ok(Some((sx / mass, sy / mass)))
}

/// The edge band, as a `width * height` row-major mask of `true` = "inside
/// the band", computed from **`rgba` alone** — recipe §10 defines the band
/// from the *reference* pixel's own neighbourhood, never the candidate's, so
/// `diff` always calls this on the reference buffer only.
///
/// **Border handling, decided and documented (recipe §10 requires this be
/// explicit):** a pixel's 3x3 neighbourhood, and later its 5x5 dilation
/// window, is clipped to the buffer's actual extent — out-of-bounds
/// neighbours are simply absent from the neighbourhood, not synthesized as
/// either class. The alternative (treating off-image neighbours as
/// background, since the canvas is nominally an infinite white page) would
/// make every border pixel automatically non-edge whenever the interior
/// pixel at the border is uniform, which is *true* for this recipe's
/// rasters (ink never reaches the image border) but is not a property this
/// function should assume for a caller's buffer in general. Treating
/// off-image neighbours as ink would be worse — it would manufacture edges
/// along the whole border of any image whose border pixels are ink. Only
/// "absent from the vote" makes no assumption about what lies outside the
/// buffer, at the cost that a border pixel needs strictly fewer differing
/// neighbours to qualify as an edge than an interior pixel does. For this
/// recipe's rasters (ink confined well within the frame) the choice is
/// inert in practice; it is recorded here because a general-purpose
/// function must still pick something.
pub fn edge_band_mask(rgba: &[u8], width: u32, height: u32) -> Result<Vec<bool>, String> {
    validate_rgba(rgba, width, height)?;
    let w = width as i32;
    let h = height as i32;
    let len = (width as usize) * (height as usize);

    let mut ink = vec![false; len];
    for y in 0..height {
        for x in 0..width {
            ink[(y as usize) * (width as usize) + (x as usize)] =
                is_ink(pixel_at(rgba, width, x, y));
        }
    }

    let mut edge = vec![false; len];
    for y in 0..h {
        for x in 0..w {
            let mut seen_ink = false;
            let mut seen_bg = false;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        // Out-of-bounds neighbour: excluded from the vote.
                        // See the doc comment above for why.
                        continue;
                    }
                    if ink[(ny as usize) * (width as usize) + (nx as usize)] {
                        seen_ink = true;
                    } else {
                        seen_bg = true;
                    }
                }
            }
            if seen_ink && seen_bg {
                edge[(y as usize) * (width as usize) + (x as usize)] = true;
            }
        }
    }

    // Dilate every edge pixel out to Chebyshev distance EDGE_BAND_PX,
    // clipped to the buffer (same border rule: no wraparound, no synthesized
    // pixels beyond the edge).
    let mut band = vec![false; len];
    for y in 0..h {
        for x in 0..w {
            if !edge[(y as usize) * (width as usize) + (x as usize)] {
                continue;
            }
            let y0 = (y - EDGE_BAND_PX).max(0);
            let y1 = (y + EDGE_BAND_PX).min(h - 1);
            let x0 = (x - EDGE_BAND_PX).max(0);
            let x1 = (x + EDGE_BAND_PX).min(w - 1);
            for by in y0..=y1 {
                for bx in x0..=x1 {
                    band[(by as usize) * (width as usize) + (bx as usize)] = true;
                }
            }
        }
    }

    Ok(band)
}

/// Every number D1/D2/D3 rest on, plus the in-band figures that are
/// reported but never decide — a `FAIL` must be diagnosable from this
/// struct alone, without re-running anything (recipe §10).
#[derive(Clone, Debug)]
pub struct DiffReport {
    pub width: u32,
    pub height: u32,

    /// Pixel count in the edge band (computed from the reference only).
    pub band_pixel_count: u64,

    /// D1: pixels *outside* the band whose ink class differs between the
    /// two images. Must be exactly 0 to pass.
    pub d1_pixels_outside_band_differing: u64,
    pub d1_pass: bool,

    pub reference_ink_mass: f64,
    pub candidate_ink_mass: f64,
    /// D2: relative delta of ink mass, `|candidate - reference| /
    /// reference`. If the reference has no ink at all (mass 0) the
    /// denominator is undefined; see the field doc on how that case is
    /// handled.
    pub d2_relative_delta: f64,
    pub d2_pass: bool,

    pub reference_centroid: Option<(f64, f64)>,
    pub candidate_centroid: Option<(f64, f64)>,
    /// D3: `(|Δx|, |Δy|)` between the two centroids. `None` when either
    /// image has zero ink mass, so no centroid exists to compare — see
    /// `d3_pass`.
    pub d3_delta: Option<(f64, f64)>,
    /// `None` when [`DiffReport::d3_delta`] is `None` (no verdict is
    /// possible, not a failed one). `Some(true)`/`Some(false)` otherwise.
    /// [`DiffReport::pass`] treats `None` as non-blocking: a comparison
    /// that cannot be made cannot itself fail D3, and in every case this
    /// crate's mutation set produces, an undefined D3 is accompanied by a
    /// D2 failure that already dooms the overall verdict (blanking the
    /// candidate to zero ink is the case in point — see the `selftest`
    /// binary's M8).
    pub d3_pass: Option<bool>,

    /// Inside the band, the largest |Δluma| observed. Reported only — this
    /// number is antialiasing, which is what the band exists to stop from
    /// deciding anything.
    pub in_band_max_abs_delta_luma: u8,
    /// Inside the band, the count of pixels whose |Δluma| exceeds
    /// [`IN_BAND_REPORT_DELTA_LUMA`]. Reported only, same reason.
    pub in_band_count_delta_gt_report_threshold: u64,

    /// D4: per-glyph ink mass, one entry per supplied region, in the order
    /// they were given. Every entry must pass.
    pub d4_regions: Vec<RegionMass>,
    pub d4_pass: bool,
    /// The single worst region, for the one-line summary a report leads with.
    pub d4_worst: Option<RegionMass>,
}

impl DiffReport {
    /// The overall verdict: D1, D2 and D4 must all hold, and D3 must either
    /// hold or be inapplicable (see `d3_pass`'s doc comment).
    pub fn pass(&self) -> bool {
        self.d1_pass && self.d2_pass && self.d4_pass && self.d3_pass.unwrap_or(true)
    }
}

/// Per-glyph ink mass over one region, dilated by [`D4_REGION_DILATION_PX`]
/// and clipped to the image.
///
/// This is the rule that actually catches a wrong, dropped, or re-shaped
/// glyph, and it exists because the other three demonstrably do not:
///
/// - **D1 cannot**, because it is structurally blind to any error confined to
///   a stroke narrower than `2 * EDGE_BAND_PX + 1 = 5 device px` — such a
///   stroke lies entirely within 2 px of its own edges, so deleting it changes
///   no unbanded pixel. Measured: deleting a 4 px stem from the synthetic
///   reference gave `d1 = 0`.
/// - **Whole-image D2 cannot**, because one glyph in a 28-glyph run is a few
///   percent of the total mass, and the tolerance is 2%.
///
/// D4 has neither weakness: the denominator is one glyph's own mass, so a
/// dropped glyph is a 100% error and a missing diacritic is a large one.
fn region_mass(rgba: &[u8], width: u32, height: u32, region: &GlyphRegion) -> Result<f64, String> {
    let d = D4_REGION_DILATION_PX;
    let x0 = region.x0.saturating_sub(d);
    let y0 = region.y0.saturating_sub(d);
    let x1 = (region.x1 + d).min(width);
    let y1 = (region.y1 + d).min(height);
    if x0 >= x1 || y0 >= y1 {
        return Err(format!(
            "region {:?} is empty after clipping to {width}x{height}: x {x0}..{x1}, y {y0}..{y1} \
             — an empty region would score 0 mass against 0 mass and pass vacuously",
            region.label
        ));
    }
    let mut mass = 0.0f64;
    for y in y0..y1 {
        for x in x0..x1 {
            let l = luma(pixel_at(rgba, width, x, y));
            mass += (255 - l) as f64 / 255.0;
        }
    }
    Ok(mass)
}

/// Runs the full bounded visual differential (recipe §10) between a
/// reference raster and a candidate raster of identical dimensions.
///
/// The edge band is computed from `reference_rgba` alone (see
/// [`edge_band_mask`]'s doc comment) — the recipe defines "edge pixel" in
/// terms of the reference image, never the candidate, because the band is
/// meant to bound where the reference's own antialiasing lives, not where
/// the candidate happened to draw one.
/// `regions` must be **non-empty**. An empty list is refused rather than
/// accepted, because D4 over zero regions is a rule that always passes, and a
/// caller that forgot to supply the glyph boxes would otherwise get a green
/// verdict from a differential missing its strongest rule. Refusing here means
/// "D4 was not evaluated" can never be mistaken for "D4 held".
pub fn diff(
    reference_rgba: &[u8],
    candidate_rgba: &[u8],
    width: u32,
    height: u32,
    regions: &[GlyphRegion],
) -> Result<DiffReport, String> {
    validate_rgba(reference_rgba, width, height)?;
    validate_rgba(candidate_rgba, width, height)?;
    if regions.is_empty() {
        return Err(
            "diff called with no glyph regions — D4 (per-glyph ink mass) would be vacuous, and a \
             vacuous rule that reports `pass` is worse than an absent one. Supply one region per \
             shaped glyph."
                .to_string(),
        );
    }

    let band = edge_band_mask(reference_rgba, width, height)?;
    let band_pixel_count = band.iter().filter(|&&b| b).count() as u64;

    let mut d1_diff = 0u64;
    let mut in_band_max_delta = 0u8;
    let mut in_band_gt_threshold = 0u64;
    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize) * (width as usize) + (x as usize);
            let rp = pixel_at(reference_rgba, width, x, y);
            let cp = pixel_at(candidate_rgba, width, x, y);
            let rl = luma(rp);
            let cl = luma(cp);
            let delta = (rl as i32 - cl as i32).unsigned_abs() as u8;
            if band[idx] {
                if delta > in_band_max_delta {
                    in_band_max_delta = delta;
                }
                if delta > IN_BAND_REPORT_DELTA_LUMA {
                    in_band_gt_threshold += 1;
                }
            } else {
                let r_ink = (rl as u32) < INK_LUMA_THRESHOLD as u32;
                let c_ink = (cl as u32) < INK_LUMA_THRESHOLD as u32;
                if r_ink != c_ink {
                    d1_diff += 1;
                }
            }
        }
    }
    let d1_pass = d1_diff == 0;

    let reference_ink_mass = ink_mass(reference_rgba, width, height)?;
    let candidate_ink_mass = ink_mass(candidate_rgba, width, height)?;
    // Relative to the reference mass, which is the recipe's own framing
    // ("agrees within 2% relative" — relative *to the reference*, the fixed
    // point of the comparison). If the reference itself carries no ink,
    // "relative" has no denominator: any candidate ink at all is reported
    // as an unbounded (infinite) relative delta rather than a fabricated
    // number, and two blank images agree exactly (delta 0).
    let d2_relative_delta = if reference_ink_mass > 0.0 {
        (candidate_ink_mass - reference_ink_mass).abs() / reference_ink_mass
    } else if candidate_ink_mass > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    let d2_pass = d2_relative_delta <= D2_RELATIVE_TOLERANCE;

    let reference_centroid = centroid(reference_rgba, width, height)?;
    let candidate_centroid = centroid(candidate_rgba, width, height)?;
    let (d3_delta, d3_pass) = match (reference_centroid, candidate_centroid) {
        (Some((rx, ry)), Some((cx, cy))) => {
            let delta = ((cx - rx).abs(), (cy - ry).abs());
            let pass = delta.0 <= D3_TOLERANCE_DEVICE_PX && delta.1 <= D3_TOLERANCE_DEVICE_PX;
            (Some(delta), Some(pass))
        }
        _ => (None, None),
    };

    let mut d4_regions = Vec::with_capacity(regions.len());
    for r in regions {
        let reference_mass = region_mass(reference_rgba, width, height, r)?;
        let candidate_mass = region_mass(candidate_rgba, width, height, r)?;
        // Same convention as D2, for the same reason: a region the reference
        // left blank has no denominator, so any candidate ink there is an
        // unbounded error rather than a fabricated percentage.
        let relative_delta = if reference_mass > 0.0 {
            (candidate_mass - reference_mass).abs() / reference_mass
        } else if candidate_mass > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };
        d4_regions.push(RegionMass {
            label: r.label.clone(),
            reference_mass,
            candidate_mass,
            relative_delta,
            pass: relative_delta <= D4_RELATIVE_TOLERANCE,
        });
    }
    let d4_pass = d4_regions.iter().all(|r| r.pass);
    let d4_worst = d4_regions
        .iter()
        .max_by(|a, b| {
            a.relative_delta
                .partial_cmp(&b.relative_delta)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();

    Ok(DiffReport {
        width,
        height,
        band_pixel_count,
        d1_pixels_outside_band_differing: d1_diff,
        d1_pass,
        reference_ink_mass,
        candidate_ink_mass,
        d2_relative_delta,
        d2_pass,
        reference_centroid,
        candidate_centroid,
        d3_delta,
        d3_pass,
        in_band_max_abs_delta_luma: in_band_max_delta,
        in_band_count_delta_gt_report_threshold: in_band_gt_threshold,
        d4_regions,
        d4_pass,
        d4_worst,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn validate_rejects_wrong_length() {
        let buf = vec![0u8; 3];
        let err = validate_rgba(&buf, 2, 2).unwrap_err();
        assert!(err.contains("expected exactly 16"));
    }

    #[test]
    fn validate_rejects_non_opaque_pixel_and_names_coordinate() {
        let mut buf = solid(2, 2, [255, 255, 255]);
        // Pixel (1, 0): index 1*4 = 4.
        buf[4 * 1 + 3] = 254;
        let err = validate_rgba(&buf, 2, 2).unwrap_err();
        assert!(
            err.contains("(1, 0)"),
            "error should name the coordinate: {err}"
        );
    }

    #[test]
    fn short_buffer_never_silently_classifies() {
        // A short buffer must error, not decode as (0,0,0,0) "ink".
        let buf = vec![0u8; 4]; // one pixel's worth, for a claimed 2x2 image
        assert!(ink_mass(&buf, 2, 2).is_err());
        assert!(centroid(&buf, 2, 2).is_err());
        assert!(edge_band_mask(&buf, 2, 2).is_err());
        assert!(diff(&buf, &buf, 2, 2, &one_region("x", 0, 0, 2, 2)).is_err());
    }

    #[test]
    fn all_white_has_zero_mass_and_no_centroid() {
        let buf = solid(4, 4, [255, 255, 255]);
        assert_eq!(ink_mass(&buf, 4, 4).unwrap(), 0.0);
        assert_eq!(centroid(&buf, 4, 4).unwrap(), None);
    }

    #[test]
    fn all_black_has_full_mass_and_centre_centroid() {
        let buf = solid(4, 4, [0, 0, 0]);
        assert_eq!(ink_mass(&buf, 4, 4).unwrap(), 16.0);
        // Uniform mass over a 4x4 grid centres at (1.5, 1.5).
        let (cx, cy) = centroid(&buf, 4, 4).unwrap().unwrap();
        assert!((cx - 1.5).abs() < 1e-9);
        assert!((cy - 1.5).abs() < 1e-9);
    }

    fn one_region(label: &str, x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<GlyphRegion> {
        vec![GlyphRegion {
            label: label.to_string(),
            x0,
            y0,
            x1,
            y1,
        }]
    }

    #[test]
    fn empty_region_list_is_refused_not_passed_vacuously() {
        let buf = solid(16, 16, [255, 255, 255]);
        let err = diff(&buf, &buf, 16, 16, &[]).unwrap_err();
        assert!(
            err.contains("no glyph regions"),
            "an empty region list must be an error, not a green D4: {err}"
        );
    }

    #[test]
    fn d4_catches_a_dropped_shape_that_d1_is_blind_to() {
        // A 4px-wide stem: narrower than D1's floor of 2*EDGE_BAND_PX+1 = 5,
        // so every one of its pixels lies inside the band around its own
        // edges and D1 cannot see it vanish. D4 must.
        let mut reference = solid(64, 64, [255, 255, 255]);
        for y in 10..54u32 {
            for x in 30..34u32 {
                let idx = ((y as usize) * 64 + x as usize) * 4;
                reference[idx] = 0;
                reference[idx + 1] = 0;
                reference[idx + 2] = 0;
            }
        }
        let candidate = solid(64, 64, [255, 255, 255]);
        let regions = one_region("stem", 30, 10, 34, 54);
        let r = diff(&reference, &candidate, 64, 64, &regions).unwrap();
        assert!(
            r.d1_pass,
            "D1 is expected to be blind here — if it fired, the declared 5px floor is wrong and \
             the recipe must change, not this assertion"
        );
        assert!(!r.d4_pass, "D4 must catch the dropped stem");
        assert_eq!(r.d4_worst.as_ref().unwrap().label, "stem");
        assert!(!r.pass());
    }

    #[test]
    fn identical_images_pass_trivially() {
        let mut buf = solid(16, 16, [255, 255, 255]);
        // Paint a small black square so the band is non-degenerate.
        for y in 6..10u32 {
            for x in 6..10u32 {
                let idx = ((y as usize) * 16 + x as usize) * 4;
                buf[idx] = 0;
                buf[idx + 1] = 0;
                buf[idx + 2] = 0;
            }
        }
        let report = diff(&buf, &buf, 16, 16, &one_region("square", 6, 6, 10, 10)).unwrap();
        assert!(report.pass());
        assert_eq!(report.d1_pixels_outside_band_differing, 0);
        assert_eq!(report.d2_relative_delta, 0.0);
        assert_eq!(report.d3_delta, Some((0.0, 0.0)));
        assert!(
            report.band_pixel_count > 0,
            "the square's edge should produce a band"
        );
    }

    #[test]
    fn blanking_the_candidate_fails_d2() {
        let mut reference = solid(16, 16, [255, 255, 255]);
        for y in 6..10u32 {
            for x in 6..10u32 {
                let idx = ((y as usize) * 16 + x as usize) * 4;
                reference[idx] = 0;
                reference[idx + 1] = 0;
                reference[idx + 2] = 0;
            }
        }
        let candidate = solid(16, 16, [255, 255, 255]);
        let report = diff(
            &reference,
            &candidate,
            16,
            16,
            &one_region("square", 6, 6, 10, 10),
        )
        .unwrap();
        assert!(!report.d2_pass);
        assert!(!report.pass());
    }

    #[test]
    fn edge_band_excludes_out_of_bounds_neighbours_rather_than_assuming_a_class() {
        // A 3x3 image, all white except the centre pixel, which is black.
        // The border pixels' 3x3 neighbourhoods are clipped to the 3x3
        // image itself; every one of them sees the black centre, so every
        // pixel in this tiny image is an edge pixel, and thus the whole
        // image is banded. This confirms the border rule does not silently
        // extend the image with a synthesized background that would make
        // border pixels edge-blind.
        let mut buf = solid(3, 3, [255, 255, 255]);
        let idx = (1 * 3 + 1) * 4;
        buf[idx] = 0;
        buf[idx + 1] = 0;
        buf[idx + 2] = 0;
        let band = edge_band_mask(&buf, 3, 3).unwrap();
        assert!(
            band.iter().all(|&b| b),
            "every pixel should fall in the band: {band:?}"
        );
    }
}
