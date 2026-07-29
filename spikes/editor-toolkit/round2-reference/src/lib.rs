//! Packet 2A-iii, Deliverable 2: turns a `round2_textkit::SpikeResolvedText`
//! into a reference raster via `round2_svgref`'s explicit-glyph emitter, and
//! derives `round2_diff::GlyphRegion` values from the emitter's own returned
//! bounds — never from a second, independently computed geometry.
//!
//! **This crate does not modify `round2-svgref` or `round2-diff`.** Both are
//! reviewed and settled (packet rule). It only calls their public APIs.

use std::collections::{BTreeMap, HashSet};

use round2_diff::GlyphRegion;
use round2_svgref::{DrawGlyph, DrawnBounds};
use round2_textkit::faces::LoadedFace;
use round2_textkit::hittest;
use round2_textkit::types::SpikeResolvedText;

pub const WIDTH: u32 = round2_textkit::TARGET_WIDTH as u32;
pub const HEIGHT: u32 = round2_textkit::TARGET_HEIGHT as u32;

/// **Resolved, not worked around.** An earlier version of this file sliced
/// the `<path>` fragment out of `round2_svgref::emit_svg`'s complete document
/// by searching for that crate's background-rect and `</svg>` markers, because
/// a multi-face run (F-B and F-D each mix face 0 and face 1) needs paths from
/// two faces inside one document and `emit_svg` takes a single face. That
/// worked, and it was a trap: any formatting change in `round2-svgref` would
/// have broken it silently — no compiler error, no failing test, just a
/// reference raster that came out wrong. `round2-svgref` now exposes
/// [`round2_svgref::emit_glyph_paths`] and [`round2_svgref::wrap_document`],
/// so the composition is an API call and a rename would be a build failure.
fn correlate_bounds<'a>(
    glyphs: &[DrawGlyph],
    bounds: &'a [DrawnBounds],
    empty: &[u16],
) -> Vec<Option<&'a DrawnBounds>> {
    let empty_set: HashSet<u16> = empty.iter().copied().collect();
    let mut bi = 0usize;
    let mut out = Vec::with_capacity(glyphs.len());
    for g in glyphs {
        if empty_set.contains(&g.glyph_id) {
            out.push(None);
        } else {
            let b = &bounds[bi];
            assert_eq!(
                b.glyph_id,
                g.glyph_id,
                "bounds/glyph correlation mismatch at input index {} — emit_svg's returned \
                 bounds order must match its input glyphs order",
                out.len()
            );
            out.push(Some(b));
            bi += 1;
        }
    }
    assert_eq!(
        bi,
        bounds.len(),
        "not every returned DrawnBounds was consumed — correlation logic under-counted"
    );
    out
}

/// Whether the emitter enforces that a segment's declared face actually
/// covers that segment's own codepoints.
///
/// **This is recipe §11's M6 refusal, implemented.** Revision 2 of the recipe
/// claimed "emitter refuses; if forced, D4" for a host-substituted face, and
/// nothing implemented the first half — [`build_fixture_raster`] simply used
/// whatever face index the segment carried. A claim that a structural
/// safeguard exists, when it does not, is worse than no claim: it is the
/// safeguard everyone downstream believes is standing.
///
/// The check is cheap and exact: for each segment with `face: Some(i)`, every
/// `char` in the segment's source range must have a `cmap` entry in face `i`.
/// Face resolution (`round2_textkit::shape`) walks the declared chain in order
/// and only ever assigns a face that covers the codepoint, so `Enforce` never
/// fires on an honestly-generated fixture — it fires on a *tampered* one,
/// which is the whole point.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FacePolicy {
    /// Every real caller, including `bin/generate_reference`.
    Enforce,
    /// **Only** the M6 mutation harness (`bin/text_mutations`), which must get
    /// past the refusal in order to measure what D4 says about a substitution
    /// that a real pipeline could never produce. Named this verbosely so that
    /// any other use of it is visible in a grep.
    AllowUncoveredForM6Only,
}

/// Refuses a segment whose declared face cannot represent its own text.
fn enforce_face_coverage(
    fixture_id: &str,
    seg_idx: usize,
    face_idx: u32,
    face: &LoadedFace,
    text: &str,
    range: &std::ops::Range<u32>,
) -> Result<(), String> {
    let sub = text
        .get(range.start as usize..range.end as usize)
        .ok_or_else(|| {
            format!("{fixture_id}: segment {seg_idx} source range is not on a UTF-8 boundary")
        })?;
    let parsed = ttf_parser::Face::parse(&face.bytes, face.identity.face_index)
        .map_err(|e| format!("{fixture_id}: face {face_idx} failed to parse: {e}"))?;
    for ch in sub.chars() {
        if parsed.glyph_index(ch).is_none() {
            return Err(format!(
                "{fixture_id}: segment {seg_idx} declares face {face_idx} ({}), which has no cmap \
                 entry for U+{:04X} — this is a host substitution, exactly what W3 §5 check 2 \
                 forbids, and the emitter refuses it rather than drawing whatever glyph id \
                 happens to land in that face's outline table",
                face.identity.family, ch as u32
            ));
        }
    }
    Ok(())
}

/// Everything measured while turning one fixture into a reference raster.
pub struct FixtureRasterResult {
    pub svg: String,
    pub rgba: Vec<u8>,
    pub regions: Vec<GlyphRegion>,
    pub drawn_glyph_count: usize,
    pub empty_glyph_count: usize,
    /// Segments with `face: None` (F-C's uncovered Arabic letter) — each
    /// contributes zero glyphs to `drawn_glyph_count + empty_glyph_count` by
    /// construction (`SpikeShapedSegment::glyphs` is always empty for these,
    /// W3-F3 / `invariants::assert_unresolved_clusters_are_diagnostic`), so
    /// this count is how that fact stays *visible* rather than silently
    /// absent from the report.
    pub unresolved_segment_count: usize,
    pub stored_glyph_count: usize,
}

/// Builds one fixture's reference SVG + raster, deriving every
/// `round2_diff::GlyphRegion` from the bounds `emit_svg` itself returned.
///
/// Each segment's own `size` (staff-space em, recipe §3 — `1.28` for every
/// segment in this recipe, but read from the data, never hard-coded) and
/// each glyph's own `offset` (relative to `rt.origin`, recipe §5) are
/// converted to device space via `round2_textkit::hittest::to_device` —
/// reused rather than re-implemented, so there is exactly one transform
/// implementation in this whole packet, not two that could quietly diverge.
pub fn build_fixture_raster(
    fixture_id: &str,
    rt: &SpikeResolvedText,
    faces: &[LoadedFace],
    width: u32,
    height: u32,
    policy: FacePolicy,
) -> Result<FixtureRasterResult, String> {
    struct Entry {
        seg_idx: usize,
        glyph_idx: usize,
        draw: DrawGlyph,
    }

    let mut by_face: BTreeMap<u32, Vec<Entry>> = BTreeMap::new();
    let mut unresolved_segment_count = 0usize;
    let mut stored_glyph_count = 0usize;

    for (seg_idx, seg) in rt.segments.iter().enumerate() {
        stored_glyph_count += seg.glyphs.len();
        let Some(face_idx) = seg.face else {
            // F-C's uncovered codepoint: no face resolved, and per W3-F3 /
            // invariant 4, `seg.glyphs` is guaranteed empty here — nothing to
            // draw, nothing added to `by_face`. Shaping was never attempted
            // against a face that cannot represent the codepoint (see
            // `SpikeShapedSegment::face`'s own doc comment), so there is no
            // "draw the .notdef glyph" fallback to suppress here either.
            unresolved_segment_count += 1;
            continue;
        };
        if policy == FacePolicy::Enforce {
            let face = faces.get(face_idx as usize).ok_or_else(|| {
                format!(
                    "{fixture_id}: segment {seg_idx} resolved to face {face_idx}, but only {} \
                     faces were loaded",
                    faces.len()
                )
            })?;
            enforce_face_coverage(fixture_id, seg_idx, face_idx, face, &rt.text, &seg.source)?;
        }
        let em_px = seg.size.0 * hittest::DEVICE_SCALE;
        for (glyph_idx, g) in seg.glyphs.iter().enumerate() {
            let device = hittest::to_device(rt, &g.offset);
            by_face.entry(face_idx).or_default().push(Entry {
                seg_idx,
                glyph_idx,
                draw: DrawGlyph {
                    glyph_id: g.glyph_id as u16,
                    origin_x: device.x,
                    origin_y: device.y,
                    em_px,
                },
            });
        }
    }

    let mut path_fragments = Vec::new();
    let mut regions = Vec::new();
    let mut drawn_glyph_count = 0usize;
    let mut empty_glyph_count = 0usize;

    for (face_idx, entries) in &by_face {
        let face = faces.get(*face_idx as usize).ok_or_else(|| {
            format!(
                "{fixture_id}: a segment resolved to face {face_idx}, but only {} faces were \
                 loaded",
                faces.len()
            )
        })?;
        let draw_glyphs: Vec<DrawGlyph> = entries.iter().map(|e| e.draw).collect();
        let (fragments, bounds, empty) =
            round2_svgref::emit_glyph_paths(&face.bytes, face.identity.face_index, &draw_glyphs)?;

        let correlated = correlate_bounds(&draw_glyphs, &bounds, &empty);
        for (entry, maybe_bounds) in entries.iter().zip(correlated.iter()) {
            match maybe_bounds {
                Some(b) => {
                    let label = format!(
                        "{fixture_id} seg{}.glyph{} (face {face_idx}, gid {})",
                        entry.seg_idx, entry.glyph_idx, entry.draw.glyph_id
                    );
                    regions.push(GlyphRegion {
                        label,
                        x0: b.x0.floor().max(0.0) as u32,
                        y0: b.y0.floor().max(0.0) as u32,
                        x1: b.x1.ceil().max(0.0) as u32,
                        y1: b.y1.ceil().max(0.0) as u32,
                    });
                    drawn_glyph_count += 1;
                }
                None => empty_glyph_count += 1,
            }
        }

        path_fragments.extend(fragments);
    }

    let final_svg = round2_svgref::wrap_document(width, height, &path_fragments);
    round2_svgref::assert_no_text_elements(&final_svg)?;
    let rgba = round2_svgref::rasterize(&final_svg, width, height)?;

    Ok(FixtureRasterResult {
        svg: final_svg,
        rgba,
        regions,
        drawn_glyph_count,
        empty_glyph_count,
        unresolved_segment_count,
        stored_glyph_count,
    })
}

/// A discrete ink-pixel count — distinct from `round2_diff::ink_mass`'s
/// continuous sum. Reimplements the same Rec. 601 luma weights
/// `round2_diff` documents (its own `luma` helper is private), so "ink"
/// means the same thing here as it does inside the differential: `luma <
/// round2_diff::INK_LUMA_THRESHOLD`.
pub fn count_ink_pixels(rgba: &[u8]) -> usize {
    rgba.chunks(4)
        .filter(|p| {
            let luma = (299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32) / 1000;
            luma < round2_diff::INK_LUMA_THRESHOLD as u32
        })
        .count()
}

/// Serializable mirror of `round2_diff::GlyphRegion` (which carries no
/// `serde` derive — it is a working type in a dependency-free crate, not a
/// wire type). Same boundary-mirror pattern `round2_textkit::types` uses for
/// `epiphany_layout_ir` types, for the same reason.
#[derive(serde::Serialize)]
pub struct RegionRecord {
    pub label: String,
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl From<&GlyphRegion> for RegionRecord {
    fn from(r: &GlyphRegion) -> Self {
        RegionRecord {
            label: r.label.clone(),
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_face_composition_goes_through_the_api_not_a_substring_search() {
        // Regression guard for the finding at the top of this file: composing
        // a two-face document must use round2-svgref's own API. If that crate
        // ever renames or reshapes these functions, this fails to COMPILE,
        // which is the entire point — the substring version failed silently.
        let paths = vec![
            "<path fill=\"#000000\" d=\"M0 0 L1 1 Z\"/>".to_string(),
            "<path fill=\"#000000\" d=\"M2 2 L3 3 Z\"/>".to_string(),
        ];
        let doc = round2_svgref::wrap_document(64, 32, &paths);
        assert!(doc.starts_with("<svg"));
        assert!(doc.ends_with("</svg>"));
        assert_eq!(doc.matches("<path").count(), 2);
        assert!(round2_svgref::assert_no_text_elements(&doc).is_ok());
    }

    #[test]
    fn correlate_bounds_matches_empty_and_nonempty_glyphs_by_id() {
        let glyphs = [
            DrawGlyph {
                glyph_id: 5,
                origin_x: 0.0,
                origin_y: 0.0,
                em_px: 10.0,
            },
            DrawGlyph {
                glyph_id: 1,
                origin_x: 1.0,
                origin_y: 0.0,
                em_px: 10.0,
            }, // empty (e.g. space)
            DrawGlyph {
                glyph_id: 5,
                origin_x: 2.0,
                origin_y: 0.0,
                em_px: 10.0,
            },
        ];
        let bounds = vec![
            DrawnBounds {
                glyph_id: 5,
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            DrawnBounds {
                glyph_id: 5,
                x0: 2.0,
                y0: 0.0,
                x1: 3.0,
                y1: 1.0,
            },
        ];
        let empty = vec![1u16];
        let correlated = correlate_bounds(&glyphs, &bounds, &empty);
        assert_eq!(correlated.len(), 3);
        assert!(correlated[0].is_some());
        assert!(correlated[1].is_none());
        assert!(correlated[2].is_some());
        assert!((correlated[2].unwrap().x0 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn count_ink_pixels_matches_a_hand_built_buffer() {
        // 2x2: one black (ink), three white (background).
        let mut rgba = vec![255u8; 2 * 2 * 4];
        rgba[0] = 0;
        rgba[1] = 0;
        rgba[2] = 0;
        assert_eq!(count_ink_pixels(&rgba), 1);
    }
}
