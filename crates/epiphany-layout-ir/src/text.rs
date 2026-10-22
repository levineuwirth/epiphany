//! Shaped text runs (Chapter 7 §"ResolvedLayoutIR": `req:layoutir:text-run`,
//! `req:layoutir:text-faces`; disposition E of
//! `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`).
//!
//! A text run carries its source string and the glyphs shaping produced from
//! it, positioned, with every input that determined them named in its
//! [`TextShapingIdentity`]. It is shaped once, here, in the constrained pass,
//! before spacing; no consumer shapes the string again. A renderer draws each
//! [`PositionedGlyph`] from the face its segment names, at the run's
//! `origin` plus its offset.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use epiphany_determinism::CanonicalF64;
use epiphany_text::Face;

use crate::constrained::GlyphStyle;
use crate::glyph::{FontId, SemVer};
use crate::provenance::Provenance;
use crate::solver::SpringSlotId;
use crate::spatial::{BoundingBox, Point, StaffSpace, Transform2D};
use crate::vertical_band::VerticalBandId;

/// A text face, identified by the content of its file.
#[derive(Clone, PartialEq, Debug)]
pub struct TextFaceIdentity {
    /// The face's name: diagnostic only, never an input.
    pub family: FontId,
    pub version: Option<SemVer>,
    /// BLAKE3 of the face file's bytes under the tag `MUSCFNTF`.
    pub file_hash: [u8; 32],
    /// Which face of a collection file; 0 for a single face.
    pub face_index: u32,
    /// A variable face's axis coordinates, by tag; empty for a static face.
    pub variations: Vec<(AxisTag, CanonicalF64)>,
    pub synthesis: FaceSynthesis,
}

/// An OpenType variation axis tag.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct AxisTag(pub [u8; 4]);

/// A style a face lacks, synthesized: it changes the ink, so it is an input.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FaceSynthesis {
    None,
    Emboldened,
    Slanted,
}

/// A shaper, by name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShaperId(pub Cow<'static, str>);

/// An OpenType feature applied beyond the shaper's defaults.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct FeatureSetting {
    pub tag: [u8; 4],
    pub value: u32,
}

/// A Unicode version.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct UnicodeVersion {
    pub major: u8,
    pub minor: u8,
    pub update: u8,
}

/// Every input that determined a run's glyphs.
#[derive(Clone, PartialEq, Debug)]
pub struct TextShapingIdentity {
    /// The ordered face chain; resolution is closed over it.
    pub faces: Vec<TextFaceIdentity>,
    pub shaper: ShaperId,
    pub shaper_version: SemVer,
    /// The OpenType features applied beyond the shaper's defaults, in
    /// canonical order.
    pub features: Vec<FeatureSetting>,
    /// The Unicode version of the segmentation that fixes the caret stops.
    pub unicode_version: UnicodeVersion,
}

/// Which way a segment reads.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

/// An ISO 15924 script tag (`Latn`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ScriptTag(pub [u8; 4]);

/// A BCP 47 language tag; empty where the language is not known.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LanguageTag(pub Cow<'static, str>);

/// One shaped glyph, placed.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PositionedGlyph {
    /// A glyph id in its segment's face's own namespace.
    pub glyph_id: u32,
    /// From the run's `origin`, alignment applied; staff spaces, y up.
    pub offset: Point,
    pub transform: Option<Transform2D>,
}

/// A run of the string set in one face of the chain.
#[derive(Clone, PartialEq, Debug)]
pub struct ShapedSegment {
    /// Index into [`TextShapingIdentity::faces`].
    pub face: u32,
    pub glyphs: Vec<PositionedGlyph>,
    /// The half-open UTF-8 byte range of [`ResolvedText::text`] it covers.
    pub source: Range<u32>,
    pub direction: TextDirection,
    pub script: ScriptTag,
    pub language: LanguageTag,
    /// The em, in staff spaces.
    pub size: StaffSpace,
}

/// Where a caret stands at a grapheme boundary, and to which side it binds.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct CaretStop {
    /// The boundary's UTF-8 byte offset in the string.
    pub offset: u32,
    /// Its x from the run's origin, in staff spaces.
    pub x: StaffSpace,
    pub affinity: CaretAffinity,
}

/// The character a caret stop binds to: the one after it (downstream), or
/// the one before it (upstream), where the two would place it apart.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CaretAffinity {
    Downstream,
    Upstream,
}

/// One cluster: a range of the string and the glyphs shaping made of it.
#[derive(Clone, PartialEq, Debug)]
pub struct TextCluster {
    /// The half-open UTF-8 byte range of the string.
    pub source: Range<u32>,
    /// Index into [`ResolvedText::segments`].
    pub segment: u32,
    /// The half-open range of that segment's glyphs.
    pub glyphs: Range<u32>,
    /// The caret stops at the grapheme boundaries the cluster holds.
    pub carets: Vec<CaretStop>,
    /// False where no face of the chain covers the cluster's characters: it
    /// holds no glyph, and its characters are reported.
    pub resolved: bool,
}

/// The source string's clusters, in logical order.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ClusterMap {
    pub clusters: Vec<TextCluster>,
}

/// How a run stands at its origin: a record of the decision, since the
/// glyphs' offsets already carry it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// A shaped run of text with its definitive position (disposition E).
#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedText {
    pub provenance: Provenance,
    /// The source string, verbatim: for accessibility, search and editing.
    pub text: String,
    pub shaping: Arc<TextShapingIdentity>,
    /// The shaped result every consumer draws; a logical partition of `text`.
    pub segments: Vec<ShapedSegment>,
    /// Source ranges to glyph ranges and caret stops.
    pub clusters: ClusterMap,
    /// The ink shaping produced, from the origin.
    pub bounds: BoundingBox,
    /// What the engraver allocates around it, from the origin: its advance
    /// across, and the first face's ascender to its descender.
    pub reserved_box: BoundingBox,
    /// The run's anchor on its baseline.
    pub origin: Point,
    pub align: TextAlign,
    pub style: GlyphStyle,
    pub layer: i32,
}

impl ResolvedText {
    /// This run's stable id, in the id space glyphs and strokes share.
    pub fn id(&self) -> crate::constrained::GlyphObjectId {
        crate::constrained::GlyphObjectId(self.provenance.stable_id.0)
    }
}

/// A text run presented to the solver: it rides one spring slot and is owned
/// by one band, as a glyph is, and moves rigidly with its slot.
#[derive(Clone, PartialEq, Debug)]
pub struct TextObject {
    /// The run, at its place in the constrained frame.
    pub text: ResolvedText,
    pub horizontal_slot: SpringSlotId,
    pub vertical_band: VerticalBandId,
}

impl TextObject {
    pub fn id(&self) -> crate::constrained::GlyphObjectId {
        self.text.id()
    }
}

/// Which of the text faces a run is set in; each has Bravura Text behind it
/// for the music symbols a word may hold.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TextFace {
    Roman,
    Italic,
    Bold,
    BoldItalic,
}

impl TextFace {
    fn chain(self) -> [Face; 2] {
        let first = match self {
            TextFace::Roman => Face::EdwinRoman,
            TextFace::Italic => Face::EdwinItalic,
            TextFace::Bold => Face::EdwinBold,
            TextFace::BoldItalic => Face::EdwinBoldItalic,
        };
        [first, Face::BravuraText]
    }
}

/// What a run needs reported: the characters no face of its chain covers,
/// and whether any of it reads right to left, which the engraver does not
/// shape.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ShapingGaps {
    pub uncovered: Vec<char>,
    pub right_to_left: bool,
}

fn identity(face: Face) -> TextFaceIdentity {
    let (major, minor, patch) = face.version();
    TextFaceIdentity {
        family: FontId(Cow::Borrowed(face.family())),
        version: Some(SemVer::new(major, minor, patch)),
        file_hash: face.file_hash(),
        face_index: 0,
        variations: Vec::new(),
        synthesis: FaceSynthesis::None,
    }
}

/// The shaping identity of a run set in `face`, shared by every run so set.
pub fn shaping_identity(face: TextFace) -> Arc<TextShapingIdentity> {
    use std::sync::OnceLock;
    static IDS: [OnceLock<Arc<TextShapingIdentity>>; 4] = [
        OnceLock::new(),
        OnceLock::new(),
        OnceLock::new(),
        OnceLock::new(),
    ];
    IDS[face as usize]
        .get_or_init(|| {
            let (major, minor, patch) = epiphany_text::SHAPER_VERSION;
            let (u_major, u_minor, u_update) = epiphany_text::UNICODE_VERSION;
            Arc::new(TextShapingIdentity {
                faces: face.chain().into_iter().map(identity).collect(),
                shaper: ShaperId(Cow::Borrowed(epiphany_text::SHAPER)),
                shaper_version: SemVer::new(major, minor, patch),
                features: Vec::new(),
                unicode_version: UnicodeVersion {
                    major: u_major as u8,
                    minor: u_minor as u8,
                    update: u_update as u8,
                },
            })
        })
        .clone()
}

/// The text face whose file hashes to `hash`, for a consumer drawing a run's
/// glyphs from the face its segment names; `None` for a face this build does
/// not carry.
pub fn face_by_hash(hash: &[u8; 32]) -> Option<Face> {
    use std::sync::OnceLock;
    static HASHES: OnceLock<Vec<(Face, [u8; 32])>> = OnceLock::new();
    HASHES
        .get_or_init(|| Face::ALL.iter().map(|f| (*f, f.file_hash())).collect())
        .iter()
        .find(|(_, h)| h == hash)
        .map(|(f, _)| *f)
}

/// Shapes `text` in `face` with Bravura Text behind it, at an em of `size`
/// staff spaces, aligned at `origin` by `align`. Returns the run and what it
/// needs reported: each character no face covers stays in the string, in an
/// unresolved cluster with no glyph, and is never drawn from another face.
pub fn shape_text(
    provenance: Provenance,
    text: &str,
    face: TextFace,
    size: f32,
    align: TextAlign,
    origin: Point,
) -> (ResolvedText, ShapingGaps) {
    let chain = face.chain();
    let shaped = epiphany_text::shape(text, &chain);
    let boundaries = epiphany_text::grapheme_boundaries(text);
    let mut gaps = ShapingGaps::default();

    // Each glyph's pen position and offset, in staff spaces from the run's
    // start, the pen advancing in its face's units scaled by the em.
    let mut pen = 0.0f64;
    let mut segments: Vec<ShapedSegment> = Vec::with_capacity(shaped.len());
    // Each segment's glyphs' (x at pen, advance), for its clusters below.
    let mut pens: Vec<Vec<(f64, f64)>> = Vec::with_capacity(shaped.len());
    for segment in &shaped {
        let face = chain[segment.face];
        let scale = f64::from(size) / f64::from(face.units_per_em());
        if !segment.resolved {
            gaps.uncovered.extend(text[segment.source.clone()].chars());
        }
        if !segment.left_to_right {
            gaps.right_to_left = true;
        }
        let mut glyphs = Vec::with_capacity(segment.glyphs.len());
        let mut at = Vec::with_capacity(segment.glyphs.len());
        for g in &segment.glyphs {
            let x = pen + f64::from(g.x_offset) * scale;
            let y = f64::from(g.y_offset) * scale;
            glyphs.push(PositionedGlyph {
                glyph_id: g.glyph_id,
                offset: Point::new(x as f32, y as f32),
                transform: None,
            });
            let advance = f64::from(g.x_advance) * scale;
            at.push((pen, advance));
            pen += advance;
        }
        segments.push(ShapedSegment {
            face: segment.face as u32,
            glyphs,
            source: segment.source.start as u32..segment.source.end as u32,
            direction: TextDirection::LeftToRight,
            script: ScriptTag(segment.script),
            language: LanguageTag(Cow::Borrowed("")),
            size: StaffSpace(size),
        });
        pens.push(at);
    }
    let width = pen;
    let shift = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => -width / 2.0,
        TextAlign::Right => -width,
    };
    for segment in &mut segments {
        for g in &mut segment.glyphs {
            g.offset = Point::new((f64::from(g.offset.x.0) + shift) as f32, g.offset.y.0);
        }
    }

    // Clusters: each segment's glyphs grouped by the cluster shaping gave
    // them, in logical order, each with the caret stops at the grapheme
    // boundaries it holds, a ligature's stops spread evenly over its advance.
    let mut clusters = Vec::new();
    let mut cursor = 0.0f64;
    for (s, (segment, at)) in shaped.iter().zip(&pens).enumerate() {
        let mut starts: Vec<(usize, usize)> = Vec::new();
        for (i, g) in segment.glyphs.iter().enumerate() {
            let at = g.cluster as usize;
            if starts.last().is_none_or(|&(c, _)| c != at) {
                starts.push((at, i));
            }
        }
        if starts.is_empty() {
            // An unresolved run: a cluster per grapheme, holding no glyph.
            let inside: Vec<usize> = boundaries
                .iter()
                .copied()
                .filter(|b| segment.source.contains(b))
                .collect();
            for (k, &start) in inside.iter().enumerate() {
                let end = inside.get(k + 1).copied().unwrap_or(segment.source.end);
                clusters.push(TextCluster {
                    source: start as u32..end as u32,
                    segment: s as u32,
                    glyphs: 0..0,
                    carets: vec![CaretStop {
                        offset: start as u32,
                        x: StaffSpace((cursor + shift) as f32),
                        affinity: CaretAffinity::Downstream,
                    }],
                    resolved: segment.resolved,
                });
            }
            continue;
        }
        for (k, &(start, first)) in starts.iter().enumerate() {
            let (end, last) = match starts.get(k + 1) {
                Some(&(next, glyph)) => (next, glyph),
                None => (segment.source.end, segment.glyphs.len()),
            };
            let x0 = at[first].0;
            let advance: f64 = at[first..last].iter().map(|(_, a)| a).sum();
            let inside: Vec<usize> = boundaries
                .iter()
                .copied()
                .filter(|b| (start..end).contains(b))
                .collect();
            let n = inside.len().max(1) as f64;
            let carets = inside
                .iter()
                .enumerate()
                .map(|(j, &b)| CaretStop {
                    offset: b as u32,
                    x: StaffSpace((x0 + advance * j as f64 / n + shift) as f32),
                    affinity: CaretAffinity::Downstream,
                })
                .collect();
            clusters.push(TextCluster {
                source: start as u32..end as u32,
                segment: s as u32,
                glyphs: first as u32..last as u32,
                carets,
                resolved: true,
            });
            cursor = x0 + advance;
        }
    }
    // The end of the string is a stop too, binding to what precedes it.
    if let Some(last) = clusters.last_mut() {
        last.carets.push(CaretStop {
            offset: text.len() as u32,
            x: StaffSpace((width + shift) as f32),
            affinity: CaretAffinity::Upstream,
        });
    }

    // The ink: each glyph's outline bounds, scaled and placed.
    let mut ink: Option<[f32; 4]> = None;
    for segment in &segments {
        let face = chain[segment.face as usize];
        let scale = segment.size.0 / f32::from(face.units_per_em());
        for g in &segment.glyphs {
            let Some([l, b, r, t]) = face.glyph_bounds(g.glyph_id) else {
                continue;
            };
            let (x, y) = (g.offset.x.0, g.offset.y.0);
            let placed = [x + l * scale, y + b * scale, x + r * scale, y + t * scale];
            ink = Some(match ink {
                None => placed,
                Some(i) => [
                    i[0].min(placed[0]),
                    i[1].min(placed[1]),
                    i[2].max(placed[2]),
                    i[3].max(placed[3]),
                ],
            });
        }
    }
    let left = shift as f32;
    let right = (width + shift) as f32;
    let bounds = match ink {
        Some([l, b, r, t]) => BoundingBox::new(l, b, r, t),
        None => BoundingBox::new(left, 0.0, left, 0.0),
    };
    let first = chain[0];
    let (ascent, descent) = first.vertical_metrics();
    let em = size / f32::from(first.units_per_em());
    let reserved_box = BoundingBox::new(left, descent as f32 * em, right, ascent as f32 * em);

    let run = ResolvedText {
        provenance,
        text: text.to_owned(),
        shaping: shaping_identity(face),
        segments,
        clusters: ClusterMap { clusters },
        bounds,
        reserved_box,
        origin,
        align,
        style: GlyphStyle { rgba: 0x0000_00ff },
        layer: 0,
    };
    (run, gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{MarkerId, TypedObjectId};

    fn provenance() -> Provenance {
        Provenance::projected(TypedObjectId::Marker(MarkerId::from_raw(1)), vec![])
    }

    fn run(text: &str, align: TextAlign) -> (ResolvedText, ShapingGaps) {
        shape_text(
            provenance(),
            text,
            TextFace::Roman,
            2.0,
            align,
            Point::new(0.0, 0.0),
        )
    }

    #[test]
    fn segments_and_clusters_partition_the_string_on_char_boundaries() {
        let text = "Moderato \u{ECA5} = 96 \u{4E2D}!";
        let (run, gaps) = run(text, TextAlign::Left);
        assert_eq!(gaps.uncovered, vec!['\u{4E2D}']);
        let mut at = 0;
        for segment in &run.segments {
            assert_eq!(segment.source.start, at);
            at = segment.source.end;
        }
        assert_eq!(at as usize, text.len());
        let mut at = 0;
        for cluster in &run.clusters.clusters {
            assert_eq!(cluster.source.start, at);
            at = cluster.source.end;
            for caret in &cluster.carets {
                assert!(text.is_char_boundary(caret.offset as usize));
            }
            let segment = &run.segments[cluster.segment as usize];
            assert!(cluster.glyphs.end as usize <= segment.glyphs.len());
        }
        assert_eq!(at as usize, text.len());
        let unresolved: Vec<&TextCluster> = run
            .clusters
            .clusters
            .iter()
            .filter(|c| !c.resolved)
            .collect();
        assert_eq!(unresolved.len(), 1, "the uncovered character is kept");
        assert!(unresolved[0].glyphs.is_empty(), "and drawn from no face");
    }

    #[test]
    fn caret_stops_follow_the_pen_and_end_at_the_advance() {
        let (run, _) = run("ab", TextAlign::Left);
        let stops: Vec<(u32, f32)> = run
            .clusters
            .clusters
            .iter()
            .flat_map(|c| c.carets.iter().map(|s| (s.offset, s.x.0)))
            .collect();
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[0], (0, 0.0));
        assert!(stops[1].1 > 0.0 && stops[2].1 > stops[1].1);
        assert_eq!(stops[2].0, 2);
        assert!((run.reserved_box.right.0 - stops[2].1).abs() < 1e-6);
    }

    #[test]
    fn alignment_moves_the_glyphs_about_the_origin() {
        let (left, _) = run("dolce", TextAlign::Left);
        let (centre, _) = run("dolce", TextAlign::Center);
        let (right, _) = run("dolce", TextAlign::Right);
        let width = left.reserved_box.right.0 - left.reserved_box.left.0;
        assert!(width > 0.0);
        let x = |r: &ResolvedText| r.segments[0].glyphs[0].offset.x.0;
        assert!((x(&centre) - (x(&left) - width / 2.0)).abs() < 1e-5);
        assert!((x(&right) - (x(&left) - width)).abs() < 1e-5);
        assert!((centre.reserved_box.left.0 + width / 2.0).abs() < 1e-5);
        assert!((right.reserved_box.right.0).abs() < 1e-5);
    }

    #[test]
    fn the_ink_lies_within_the_line_and_the_advance() {
        let (run, _) = run("Andante", TextAlign::Left);
        let (ink, line) = (run.bounds, run.reserved_box);
        assert!(ink.left.0 >= line.left.0 - 0.2 && ink.right.0 <= line.right.0 + 0.2);
        assert!(ink.bottom.0 >= line.bottom.0 && ink.top.0 <= line.top.0);
        assert!(ink.top.0 > 1.0, "a capital stands most of the em high");
    }

    #[test]
    fn every_run_names_its_faces_by_hash() {
        let (run, _) = run("a", TextAlign::Left);
        assert_eq!(run.shaping.faces.len(), 2);
        for (identity, face) in run.shaping.faces.iter().zip(TextFace::Roman.chain()) {
            assert_eq!(identity.file_hash, face.file_hash());
            assert_eq!(face_by_hash(&identity.file_hash), Some(face));
        }
        assert_eq!(face_by_hash(&[0; 32]), None);
    }

    #[test]
    fn a_ligatures_caret_stops_spread_over_its_advance() {
        // Edwin sets "fi" as one glyph: one cluster of two graphemes.
        let (run, _) = run("fi", TextAlign::Left);
        let glyphs: usize = run.segments.iter().map(|s| s.glyphs.len()).sum();
        assert_eq!(glyphs, 1, "a ligature");
        let cluster = &run.clusters.clusters[0];
        assert_eq!(cluster.source, 0..2);
        let xs: Vec<f32> = cluster.carets.iter().map(|c| c.x.0).collect();
        assert_eq!(xs.len(), 3, "a stop before each letter and at the end");
        let advance = run.reserved_box.right.0;
        assert!(
            (xs[1] - advance / 2.0).abs() < 1e-5,
            "{xs:?} over {advance}"
        );
        assert!(xs[0] < xs[1] && xs[1] < xs[2]);
    }
}
