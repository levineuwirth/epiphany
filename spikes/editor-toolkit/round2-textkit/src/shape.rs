//! Itemization, shaping, and cluster/caret construction (recipe §2, §3, §4,
//! §7): `unicode-bidi` itemizes the run into direction-level spans,
//! `rustybuzz` shapes each face-resolved span, `unicode-segmentation` gives
//! the grapheme boundaries caret stops are built from.
//!
//! ## Determinism: why `language` is always unset
//!
//! `rustybuzz::UnicodeBuffer::guess_segment_properties` fills in `script`
//! (from each character's Unicode `Script` property — a pure function of the
//! codepoints, no host state) and `direction` (derived from that script) when
//! they are unset, but leaves `language` alone — its own source marks this
//! `// TODO: language must be set` (`rustybuzz` 0.20.1,
//! `src/hb/buffer.rs`). Real HarfBuzz falls back to the *host locale* for an
//! unset language; `rustybuzz`'s port never implements that fallback, so a
//! buffer that never calls `set_language` simply ships with `language =
//! None` — deterministically, not by omission on this crate's part. This
//! crate never calls `set_language`, so every fixture's `language` field is
//! `None`, exactly and reproducibly, on every machine.
//!
//! ## Caret-stop geometry: the rule, stated once
//!
//! Every *ordinary* caret stop (recipe §7) reads its position directly off
//! where shaping placed the grapheme's glyph(s), in the order the shaper's
//! output array gives them — no direction-dependent reinterpretation. For a
//! shaping cluster spanning `G` graphemes (a ligature has `G > 1`; recipe
//! §7's interpolation rule), grapheme `k` (0-indexed, in ascending
//! byte-offset order) sits at `cluster_origin + (k / G) * cluster_advance`,
//! where `cluster_origin`/`cluster_advance` are the pen position recorded
//! when that cluster's glyphs were walked and their summed advance. This is
//! the *only* rule; it does not vary by segment direction, so F-A's `ff`
//! interpolation and a plain Hebrew character's stop use the identical
//! formula.
//!
//! At a **direction-run boundary** (recipe §7: F-D bytes 8 and 14), the
//! ordinary rule above already produces one stop — associated with the run
//! *starting* at that offset, affinity `Downstream`. A second stop, affinity
//! `Upstream`, is added for the run *ending* there: its position is that
//! run's own **trailing pen**, i.e. the edge where *that run's own reading
//! direction* terminates — `end_pen` for an `Ltr` run (reading terminates at
//! its right edge, the ordinary continuous-pen value) but `start_pen` for an
//! `Rtl` run (reading terminates at its *left* edge, since RTL reading
//! proceeds toward decreasing x). This is the one place direction enters the
//! geometry, and it is what makes F-D's two byte-8 stops (`Upstream` at the
//! end of "Allegro ", `Downstream` at wherever the Hebrew run's first glyph
//! landed) land at genuinely different x — see `FIXTURES_SUMMARY.md` for the
//! as-measured values.
//!
//! This is a documented, defensible choice, not a claim to have solved
//! general UAX#9-adjacent bidi caret placement — real editors debate this
//! territory extensively. What matters for this recipe is that the two
//! stops are geometrically distinct and each is traceable to an actual
//! shaped position, which `invariants::assert_direction_boundary_stops_differ`
//! checks.

use std::ops::Range;

use rustybuzz::{Direction as RbDirection, Face as RbFace, UnicodeBuffer};
use unicode_bidi::{BidiInfo, Level};
use unicode_segmentation::UnicodeSegmentation;

use crate::faces::LoadedFace;
use crate::types::{
    SpikeCaretAffinity, SpikeCaretStop, SpikeCluster, SpikeClusterMap, SpikeLanguageTag,
    SpikePoint, SpikeScriptTag, SpikeShapedSegment, SpikeStaffSpace, SpikeTextDirection,
};
use crate::EM_SIZE_STAFF_SPACE;

/// A maximal span of text that itemization treats as one shaping unit: a
/// single bidi embedding level, resolved (if at all) to a single face.
struct ItemSpan {
    range: Range<u32>,
    level: Level,
    /// `None` if resolution walked the whole declared chain and no face
    /// covers every codepoint in this span (`crate::findings::W3_F3`).
    face: Option<usize>,
}

fn direction_of(level: Level) -> SpikeTextDirection {
    if level.is_rtl() {
        SpikeTextDirection::Rtl
    } else {
        SpikeTextDirection::Ltr
    }
}

/// Splits `text` into bidi-level runs (paragraph-relative, single paragraph
/// assumed — every fixture is one line with no paragraph separator), then
/// further splits each run into maximal spans sharing one face resolution.
/// Both splits only ever occur on character boundaries.
fn itemize(text: &str, faces: &[LoadedFace]) -> Vec<ItemSpan> {
    let bidi = BidiInfo::new(text, None);
    assert_eq!(
        bidi.paragraphs.len(),
        1,
        "fixture text must be exactly one bidi paragraph (no paragraph separator); got {}",
        bidi.paragraphs.len()
    );
    let levels = &bidi.levels;
    assert_eq!(
        levels.len(),
        text.len(),
        "bidi levels must cover every byte"
    );

    // Pass 1: contiguous same-level runs, at character boundaries.
    let mut level_runs: Vec<(Range<u32>, Level)> = Vec::new();
    for (i, ch) in text.char_indices() {
        let level = levels[i];
        let end = (i + ch.len_utf8()) as u32;
        let start = i as u32;
        match level_runs.last_mut() {
            Some((r, lvl)) if *lvl == level && r.end == start => r.end = end,
            _ => level_runs.push((start..end, level)),
        }
    }

    // Pass 2: within each level run, further split by face resolution.
    let mut spans = Vec::new();
    for (range, level) in level_runs {
        let sub = &text[range.start as usize..range.end as usize];
        for (i, ch) in sub.char_indices() {
            let abs_start = range.start + i as u32;
            let abs_end = abs_start + ch.len_utf8() as u32;
            let face = resolve_face_for_char(ch, faces);
            match spans.last_mut() {
                Some(ItemSpan {
                    range: r,
                    level: lvl,
                    face: f,
                }) if *lvl == level && *f == face && r.end == abs_start => {
                    r.end = abs_end;
                }
                _ => spans.push(ItemSpan {
                    range: abs_start..abs_end,
                    level,
                    face,
                }),
            }
        }
    }
    spans
}

/// Walks the declared chain **in order**, never the host: the first face
/// whose `cmap` covers `ch` wins. `None` if none do (F-C's Arabic letter).
fn resolve_face_for_char(ch: char, faces: &[LoadedFace]) -> Option<usize> {
    faces
        .iter()
        .position(|f| f.face().glyph_index(ch).is_some())
}

/// Detects `ch`'s script the same deterministic way `guess_segment_properties`
/// does internally (a pure function of the Unicode `Script` property table,
/// no locale), exposed here only for the diagnostic `script` field.
fn detect_script_tag(sub: &str) -> String {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(sub);
    buffer.guess_segment_properties();
    let bytes = buffer.script().tag().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn quantized_point(x: f64, y: f64) -> SpikePoint {
    SpikePoint::new(
        crate::quantize::quantize_component(x),
        crate::quantize::quantize_component(y),
    )
}

/// One resolved segment's overall pen span, tracked alongside `segments` so
/// the direction-boundary post-pass (see the module doc comment) can read
/// each run's own leading/trailing edge without re-walking its glyphs.
#[derive(Copy, Clone)]
struct PenSpan {
    start_pen: f64,
    end_pen: f64,
    direction: SpikeTextDirection,
}

impl PenSpan {
    /// The edge where *this run's own reading direction* terminates — see
    /// the module doc comment's caret-stop geometry section.
    fn trailing_pen(&self) -> f64 {
        match self.direction {
            SpikeTextDirection::Ltr => self.end_pen,
            SpikeTextDirection::Rtl => self.start_pen,
        }
    }
}

/// The result of shaping one fixture string: its segments and cluster map,
/// ready to become a [`crate::types::SpikeResolvedText`].
pub struct ShapeResult {
    pub segments: Vec<SpikeShapedSegment>,
    pub clusters: SpikeClusterMap,
}

/// Itemizes, shapes, and clusters `text` against the resolved `faces`
/// (recipe §2/§3/§4/§7). This is the single entry point `crate::fixtures`
/// calls per fixture.
pub fn shape_text(text: &str, faces: &[LoadedFace]) -> ShapeResult {
    let spans = itemize(text, faces);
    let graphemes: Vec<(u32, u32)> = text
        .grapheme_indices(true)
        .map(|(i, g)| (i as u32, (i + g.len()) as u32))
        .collect();

    let mut segments: Vec<SpikeShapedSegment> = Vec::new();
    let mut pen_spans: Vec<PenSpan> = Vec::new();
    let mut clusters: Vec<SpikeCluster> = Vec::new();
    let mut pen_staff_x: f64 = 0.0;

    for span in spans {
        let direction = direction_of(span.level);
        let start_pen = pen_staff_x;
        match span.face {
            None => {
                let seg_index = segments.len();
                segments.push(SpikeShapedSegment {
                    face: None,
                    glyphs: Vec::new(),
                    source: span.range.clone(),
                    direction,
                    script: SpikeScriptTag(detect_script_tag(
                        &text[span.range.start as usize..span.range.end as usize],
                    )),
                    language: SpikeLanguageTag(None),
                    size: SpikeStaffSpace(EM_SIZE_STAFF_SPACE),
                });
                // No shaping is attempted (see the type's doc comment: doing
                // so would silently draw `.notdef`). Each grapheme in the
                // span becomes its own diagnostic, unresolved cluster;
                // contributes no advance.
                for &(g_start, g_end) in graphemes
                    .iter()
                    .filter(|&&(s, _)| s >= span.range.start && s < span.range.end)
                {
                    clusters.push(SpikeCluster {
                        source: g_start..g_end,
                        segment: seg_index,
                        glyph_indices: Vec::new(),
                        resolved: false,
                        grapheme_count: 1,
                        caret_stops: vec![SpikeCaretStop {
                            source_offset: g_start,
                            position: quantized_point(pen_staff_x, 0.0),
                            affinity: SpikeCaretAffinity::Downstream,
                        }],
                    });
                }
                pen_spans.push(PenSpan {
                    start_pen,
                    end_pen: pen_staff_x,
                    direction,
                });
            }
            Some(face_idx) => {
                let loaded = &faces[face_idx];
                let rb_face: RbFace = loaded.face();
                let upem = rb_face.units_per_em() as f64;
                let scale = EM_SIZE_STAFF_SPACE / upem;

                let sub_text = &text[span.range.start as usize..span.range.end as usize];
                let mut buffer = UnicodeBuffer::new();
                buffer.push_str(sub_text);
                buffer.guess_segment_properties();
                let script_tag = {
                    let bytes = buffer.script().tag().to_bytes();
                    String::from_utf8_lossy(&bytes).into_owned()
                };
                buffer.set_direction(match direction {
                    SpikeTextDirection::Ltr => RbDirection::LeftToRight,
                    SpikeTextDirection::Rtl => RbDirection::RightToLeft,
                });
                // `language` is deliberately never set — see the module doc
                // comment.
                let glyph_buffer = rustybuzz::shape(&rb_face, &[], buffer);
                let infos = glyph_buffer.glyph_infos();
                let positions = glyph_buffer.glyph_positions();
                assert_eq!(infos.len(), positions.len());

                let seg_index = segments.len();
                let mut seg_glyphs = Vec::with_capacity(infos.len());

                let mut distinct: Vec<u32> = infos.iter().map(|i| i.cluster).collect();
                distinct.sort_unstable();
                distinct.dedup();

                let mut cluster_glyph_indices: Vec<(u32, Vec<u32>)> =
                    distinct.iter().map(|&c| (c, Vec::new())).collect();
                let mut cluster_origin: Vec<(u32, f64)> =
                    distinct.iter().map(|&c| (c, 0.0)).collect();
                let mut cluster_advance: Vec<(u32, f64)> =
                    distinct.iter().map(|&c| (c, 0.0)).collect();
                let mut cluster_first_seen: Vec<bool> = vec![false; distinct.len()];

                let index_of = |c: u32| {
                    distinct
                        .binary_search(&c)
                        .expect("cluster id must be in the distinct set")
                };

                for (info, pos) in infos.iter().zip(positions.iter()) {
                    let ci = index_of(info.cluster);
                    if !cluster_first_seen[ci] {
                        cluster_origin[ci].1 = pen_staff_x;
                        cluster_first_seen[ci] = true;
                    }
                    let gx = pen_staff_x + pos.x_offset as f64 * scale;
                    let gy = pos.y_offset as f64 * scale;
                    let glyph_index_in_seg = seg_glyphs.len() as u32;
                    seg_glyphs.push(crate::types::SpikePositionedGlyph {
                        glyph_id: info.glyph_id,
                        offset: quantized_point(gx, gy),
                        transform: None,
                    });
                    cluster_glyph_indices[ci].1.push(glyph_index_in_seg);
                    let adv = pos.x_advance as f64 * scale;
                    pen_staff_x += adv;
                    cluster_advance[ci].1 += adv;
                }

                segments.push(SpikeShapedSegment {
                    face: Some(face_idx as u32),
                    glyphs: seg_glyphs,
                    source: span.range.clone(),
                    direction,
                    script: SpikeScriptTag(script_tag),
                    language: SpikeLanguageTag(None),
                    size: SpikeStaffSpace(EM_SIZE_STAFF_SPACE),
                });
                pen_spans.push(PenSpan {
                    start_pen,
                    end_pen: pen_staff_x,
                    direction,
                });

                let span_len = span.range.end - span.range.start;
                for (k, &cid) in distinct.iter().enumerate() {
                    let rel_start = cid;
                    let rel_end = if k + 1 < distinct.len() {
                        distinct[k + 1]
                    } else {
                        span_len
                    };
                    let abs_start = span.range.start + rel_start;
                    let abs_end = span.range.start + rel_end;
                    let origin = cluster_origin[k].1;
                    let advance = cluster_advance[k].1;
                    let cluster_graphemes: Vec<(u32, u32)> = graphemes
                        .iter()
                        .copied()
                        .filter(|&(s, _)| s >= abs_start && s < abs_end)
                        .collect();
                    let gcount = cluster_graphemes.len().max(1) as u32;
                    let mut stops = Vec::with_capacity(cluster_graphemes.len());
                    for (gk, &(g_start, _g_end)) in cluster_graphemes.iter().enumerate() {
                        let frac = gk as f64 / gcount as f64;
                        let sx = origin + frac * advance;
                        stops.push(SpikeCaretStop {
                            source_offset: g_start,
                            position: quantized_point(sx, 0.0),
                            affinity: SpikeCaretAffinity::Downstream,
                        });
                    }
                    clusters.push(SpikeCluster {
                        source: abs_start..abs_end,
                        segment: seg_index,
                        glyph_indices: cluster_glyph_indices[k].1.clone(),
                        resolved: true,
                        grapheme_count: gcount,
                        caret_stops: stops,
                    });
                }
            }
        }
    }

    inject_direction_boundary_stops(&segments, &pen_spans, &mut clusters);

    ShapeResult {
        segments,
        clusters: SpikeClusterMap { clusters },
    }
}

/// The Upstream half of the direction-boundary rule (see the module doc
/// comment): for every pair of adjacent segments whose `direction` differs,
/// finds the cluster that starts at the boundary offset (already carrying
/// its ordinary `Downstream` stop) and appends the `Upstream` stop computed
/// from the *preceding* segment's own trailing pen.
fn inject_direction_boundary_stops(
    segments: &[SpikeShapedSegment],
    pen_spans: &[PenSpan],
    clusters: &mut [SpikeCluster],
) {
    for i in 0..segments.len().saturating_sub(1) {
        let a = &segments[i];
        let b = &segments[i + 1];
        if a.source.end != b.source.start {
            continue; // segments must be contiguous; a gap is a bug elsewhere.
        }
        if a.direction == b.direction {
            continue;
        }
        let boundary = a.source.end;
        let upstream_pos = pen_spans[i].trailing_pen();
        let cluster = clusters
            .iter_mut()
            .find(|c| c.source.start == boundary)
            .unwrap_or_else(|| {
                panic!("no cluster starts exactly at direction boundary byte {boundary}")
            });
        cluster.caret_stops.push(SpikeCaretStop {
            source_offset: boundary,
            position: quantized_point(upstream_pos, 0.0),
            affinity: SpikeCaretAffinity::Upstream,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny synthetic pen span, used to test `trailing_pen` directly
    /// without needing real shaping.
    #[test]
    fn trailing_pen_uses_the_far_edge_for_rtl() {
        let ltr = PenSpan {
            start_pen: 1.0,
            end_pen: 5.0,
            direction: SpikeTextDirection::Ltr,
        };
        assert_eq!(ltr.trailing_pen(), 5.0);
        let rtl = PenSpan {
            start_pen: 1.0,
            end_pen: 5.0,
            direction: SpikeTextDirection::Rtl,
        };
        assert_eq!(rtl.trailing_pen(), 1.0);
    }

    /// Mutation-first: if `trailing_pen` used `end_pen` unconditionally
    /// (the bug this method exists to avoid), Upstream and Downstream would
    /// coincide at an Ltr-preceding-Rtl boundary in exactly the way the
    /// module doc comment says they must not.
    #[test]
    fn trailing_pen_would_collide_with_downstream_if_direction_were_ignored() {
        let rtl = PenSpan {
            start_pen: 2.0,
            end_pen: 9.0,
            direction: SpikeTextDirection::Rtl,
        };
        let naive_wrong = rtl.end_pen; // what a direction-blind implementation would use
        assert_ne!(
            rtl.trailing_pen(),
            naive_wrong,
            "the direction-aware trailing edge must differ from the naive end_pen for Rtl"
        );
    }
}
