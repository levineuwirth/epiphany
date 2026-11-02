//! Grace notes on the page (X4d.2): each drawn small before its principal,
//! the note of its voice that sounds where it stands.
//!
//! The rules are Gould's (*Behind Bars*, pp. 125 to 130) and MuseScore 4.7.5's
//! where she gives no figure: heads, accidentals, flags and beams at 0.7 of
//! their size (`graceNoteMag`; Gould: smaller than a cue note's three
//! quarters), stems about 2¼ spaces long (Gould, p. 125, as the owner ruled
//! over MuseScore's and LilyPond's 3.5 scaled to 2.45), up but in a lower
//! part (p. 126), a single grace slashed through its flag and a group's slash
//! at its first stem across the beam (p. 126; `ChordLayout`, the slash only on
//! an unbeamed chord or a beam's first). A group stands right to left before
//! its principal, its graces 0.3 apart (`graceToGraceNoteDist`) and the group
//! 0.45 clear of the principal's ink (`graceToMainNoteDist`), wholly left of
//! it, as Gould sets graces before the beat (p. 127). The principal's column
//! then stands further right by the group's reach, the other parts aligning
//! with the measured note (p. 127).

use crate::glyph::metrics;
use crate::provenance::Provenance;
use crate::spatial::Point;
use crate::text::{symbol_run, ResolvedText, Symbol, SymbolPlace, Turn};

/// The size of a grace note's parts against a note's (`graceNoteMag`).
pub(crate) const GRACE_MAG: f32 = 0.7;
/// A grace note's stem, from its head's centre (Gould, p. 125).
pub(crate) const GRACE_STEM: f32 = 2.25;
/// Between graces, and from a group to its principal's ink.
pub(crate) const GRACE_TO_GRACE: f32 = 0.3;
pub(crate) const GRACE_TO_MAIN: f32 = 0.45;
/// An acciaccatura's slash (`stemSlashAngle`, `stemSlashPosition`,
/// `stemSlashThickness`): 40° to the horizontal, starting the position times
/// the grace's size from the stem's tip (two thirds as far on a beamed stem),
/// at half a head's width left of the stem's right edge, to the flag's right
/// edge or two spaces along a beam.
pub(crate) const SLASH_ANGLE: f32 = 40.0;
pub(crate) const SLASH_POSITION: f32 = 2.0;
pub(crate) const SLASH_BEAMED_REDUCTION: f32 = 0.66;
pub(crate) const SLASH_BEAMED_LENGTH: f32 = 2.0;
pub(crate) const SLASH_THICKNESS: f32 = 0.125;

/// An ink box `[left, bottom, right, top]`.
pub(crate) type Ink = [f32; 4];

/// One grace note set out about its own origin, its first head's left edge,
/// before the group places it: its boxes and the primitives they come from.
#[derive(Clone, Debug, Default)]
pub(crate) struct GraceShape {
    /// Its heads: each glyph's origin and name, with its pitch's index among
    /// the grace's pitches (`None` for an unpitched head).
    pub heads: Vec<(Point, &'static str, Option<usize>)>,
    /// Its accidentals: each glyph's origin and name, by pitch index.
    pub accidentals: Vec<(Point, &'static str, usize)>,
    /// Its stem's x and its two ends' heights, the head's end first.
    pub stem: Option<(f32, f32, f32)>,
    /// Its flag's origin (the stem's outer edge at its tip) and name.
    pub flag: Option<(Point, &'static str)>,
    /// Its ledger lines' ends.
    pub ledgers: Vec<(Point, Point)>,
    /// Its slash's ends.
    pub slash: Option<(Point, Point)>,
    /// Whether its stem turns up.
    pub up: bool,
}

impl GraceShape {
    /// The grace moved `dx` across.
    pub(crate) fn moved(mut self, dx: f32) -> Self {
        let m = |p: Point| Point::new(p.x.0 + dx, p.y.0);
        for h in &mut self.heads {
            h.0 = m(h.0);
        }
        for a in &mut self.accidentals {
            a.0 = m(a.0);
        }
        if let Some((x, a, b)) = self.stem {
            self.stem = Some((x + dx, a, b));
        }
        if let Some((p, name)) = self.flag {
            self.flag = Some((m(p), name));
        }
        for l in &mut self.ledgers {
            *l = (m(l.0), m(l.1));
        }
        if let Some((a, b)) = self.slash {
            self.slash = Some((m(a), m(b)));
        }
        self
    }
}

/// The left edge of each grace of a group, right to left from the
/// principal's: the grace nearest the principal stands `GRACE_TO_MAIN` clear
/// of `principal`'s leftmost ink, and each before it `GRACE_TO_GRACE` clear
/// of the next one's leftmost ink, each wholly left of what follows it.
/// `inks` are each grace's ink boxes about its own origin, in the group's
/// order; returns each grace's shift.
pub(crate) fn place_group(inks: &[Vec<Ink>], principal: &[Ink]) -> Vec<f32> {
    let left = |boxes: &[Ink]| boxes.iter().map(|b| b[0]).fold(f32::INFINITY, f32::min);
    let right = |boxes: &[Ink]| boxes.iter().map(|b| b[2]).fold(f32::NEG_INFINITY, f32::max);
    let mut limit = left(principal);
    if !limit.is_finite() {
        limit = 0.0;
    }
    let mut gap = GRACE_TO_MAIN;
    let mut shifts = vec![0.0; inks.len()];
    for (i, boxes) in inks.iter().enumerate().rev() {
        let r = right(boxes);
        let shift = if r.is_finite() {
            limit - gap - r
        } else {
            limit - gap
        };
        shifts[i] = shift;
        let l = left(boxes);
        limit = if l.is_finite() {
            l + shift
        } else {
            limit - gap
        };
        gap = GRACE_TO_GRACE;
    }
    shifts
}

/// An acciaccatura's slash on an up or down stem at `stem_x` (its centre),
/// `width` wide, whose tip stands at `tip`: across a flag whose right edge is
/// `flag_right`, or, beamed, two spaces along the beam's slope `beam_slope`
/// (`TLayout::layoutStemSlash`), in this frame, y up.
pub(crate) fn slash(
    stem_x: f32,
    width: f32,
    tip: f32,
    up: bool,
    head_width: f32,
    flag_right: Option<f32>,
    beam_slope: f32,
) -> (Point, Point) {
    let toward = if up { 1.0 } else { -1.0 };
    let start_x = stem_x + width * 0.5 - head_width * 0.5;
    let angle = SLASH_ANGLE.to_radians();
    match flag_right {
        Some(end_x) => {
            let start_y = tip - toward * GRACE_MAG * SLASH_POSITION;
            let end_y = start_y + toward * (end_x - start_x) * angle.tan();
            (Point::new(start_x, start_y), Point::new(end_x, end_y))
        }
        None => {
            let start_y = tip - toward * GRACE_MAG * SLASH_POSITION * SLASH_BEAMED_REDUCTION;
            let angle = angle + toward * beam_slope.atan() * 0.5;
            let (dx, dy) = (
                SLASH_BEAMED_LENGTH * angle.cos(),
                toward * SLASH_BEAMED_LENGTH * angle.sin(),
            );
            (
                Point::new(start_x, start_y),
                Point::new(start_x + dx, start_y + dy),
            )
        }
    }
}

/// How far a grace note's mark stands from its staff and its grace's ink, and
/// from the ink of its column it meets and the mark before it on its side:
/// MuseScore's `propertyDistance` and `articulationMinDistance`, at a note's
/// size, as MuseScore keeps a grace's (Gould gives no figure).
pub(crate) const GRACE_MARK_DISTANCE: f32 = 0.4;

/// A grace note's mark placed (X4d fix round 1): its tag among its event's
/// marks, its glyph, and its origin, in the frame the grace is set out in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraceMark {
    pub tag: u8,
    pub name: &'static str,
    pub origin: Point,
}

/// Which side of the staff a run of graces (a lone grace, or graces one beam
/// joins) takes its marks on (X4d fix round 1): outside the staff where
/// Gould puts them, the most conspicuous place, since a small mark is hard
/// to see on the staff (*Behind Bars*, p. 130). Read from her examples, that
/// is the side on which a mark set outside the staff and the run's ink
/// stands nearer the run's heads: under heads standing under the staff,
/// over a beam over it, over the staff beyond the stem of a grace high on
/// it. Where both stand as near, the stem's side. Beside another voice, a
/// caller takes the stem's side, as MuseScore does and as Gould sets each
/// part's marks at its own stem (pp. 117 and 118).
pub(crate) fn marks_above(run: &[&GraceShape], yo: f32) -> bool {
    let k = GRACE_MAG;
    let (top_line, bottom_line) = (yo + 4.0, yo);
    let mut centres: Vec<f32> = Vec::new();
    let (mut top, mut bottom) = (top_line, bottom_line);
    for shape in run {
        for (at, name, _) in &shape.heads {
            if let Some(hb) = metrics(name).map(|m| m.bounding_box()) {
                centres.push(at.y.0 + k * (hb.bottom.0 + hb.top.0) * 0.5);
            }
        }
        for b in grace_boxes(shape) {
            top = top.max(b[3]);
            bottom = bottom.min(b[1]);
        }
    }
    let up = run.first().is_none_or(|shape| shape.up);
    if centres.is_empty() {
        return up;
    }
    let centre = centres.iter().sum::<f32>() / centres.len() as f32;
    let over = top + GRACE_MARK_DISTANCE - centre;
    let under = centre - (bottom - GRACE_MARK_DISTANCE);
    if (over - under).abs() < 1e-3 {
        up
    } else {
        over < under
    }
}

/// A grace note's marks, at its size, on the side `above` gives
/// ([`marks_above`]; a marcato or a string, brass or snap-pizzicato sign
/// above always): each `GRACE_MARK_DISTANCE` off the staff and the grace's
/// own ink, centred on its head, or on the stem's side midway between the
/// head and the stem (MuseScore's `articulationStemHAlign`), the next on its
/// side as far past it; and, as X4d.3's column rule sets a note's outside
/// marks, as far clear of the ink of its column it meets across its width
/// (`column`: the principal's, and every grace's of the column, their beams
/// with them). `marks` are each mark's tag, its glyph above and below, and
/// whether it stands above always; `yo` is the staff's bottom line.
pub(crate) fn place_marks(
    shape: &GraceShape,
    marks: &[(u8, &'static str, &'static str, bool)],
    yo: f32,
    above: bool,
    column: &[Ink],
) -> Vec<GraceMark> {
    let k = GRACE_MAG;
    let Some(&(first, head_name, _)) = shape.heads.first() else {
        return Vec::new();
    };
    let Some(hb) = metrics(head_name).map(|m| m.bounding_box()) else {
        return Vec::new();
    };
    let (top_line, bottom_line) = (yo + 4.0, yo);
    let own = grace_boxes(shape);
    let own_top = own.iter().map(|b| b[3]).fold(top_line, f32::max);
    let own_bottom = own.iter().map(|b| b[1]).fold(bottom_line, f32::min);
    let head_centre = first.x.0 + k * (hb.left.0 + hb.right.0) * 0.5;
    let mut next_above = own_top + GRACE_MARK_DISTANCE;
    let mut next_below = own_bottom - GRACE_MARK_DISTANCE;
    let mut placed = Vec::new();
    for &(tag, above_name, below_name, always_above) in marks {
        let up = always_above || above;
        let name = if up { above_name } else { below_name };
        let Some(b) = metrics(name).map(|m| m.bounding_box()) else {
            continue;
        };
        let stem_side = up == shape.up && shape.stem.is_some();
        let centre = match shape.stem {
            Some((x, _, _)) if stem_side => (head_centre + x) * 0.5,
            _ => head_centre,
        };
        let left = centre - k * (b.right.0 - b.left.0) * 0.5;
        let right = centre + k * (b.right.0 - b.left.0) * 0.5;
        let across = column.iter().filter(|c| c[0] < right && c[2] > left);
        let x = left - k * b.left.0;
        if up {
            let bottom = across
                .map(|c| c[3] + GRACE_MARK_DISTANCE)
                .fold(next_above, f32::max);
            placed.push(GraceMark {
                tag,
                name,
                origin: Point::new(x, bottom - k * b.bottom.0),
            });
            next_above = bottom + k * (b.top.0 - b.bottom.0) + GRACE_MARK_DISTANCE;
        } else {
            let top = across
                .map(|c| c[1] - GRACE_MARK_DISTANCE)
                .fold(next_below, f32::min);
            placed.push(GraceMark {
                tag,
                name,
                origin: Point::new(x, top - k * b.top.0),
            });
            next_below = top - k * (b.top.0 - b.bottom.0) - GRACE_MARK_DISTANCE;
        }
    }
    placed
}

/// A grace's own ink boxes for its marks: its heads, accidentals, stem, flag,
/// ledger lines and slash.
fn grace_boxes(shape: &GraceShape) -> Vec<Ink> {
    let k = GRACE_MAG;
    let glyph = |at: Point, name: &str| {
        metrics(name).map(|m| {
            let b = m.bounding_box();
            [
                at.x.0 + k * b.left.0,
                at.y.0 + k * b.bottom.0,
                at.x.0 + k * b.right.0,
                at.y.0 + k * b.top.0,
            ]
        })
    };
    let mut ink: Vec<Ink> = Vec::new();
    ink.extend(shape.heads.iter().filter_map(|(at, n, _)| glyph(*at, n)));
    ink.extend(
        shape
            .accidentals
            .iter()
            .filter_map(|(at, n, _)| glyph(*at, n)),
    );
    if let Some((at, name)) = shape.flag {
        ink.extend(glyph(at, name));
    }
    if let Some((x, a, b)) = shape.stem {
        ink.push([x, a.min(b), x, a.max(b)]);
    }
    for (a, b) in shape.ledgers.iter().chain(shape.slash.iter()) {
        ink.push([
            a.x.0.min(b.x.0),
            a.y.0.min(b.y.0),
            a.x.0.max(b.x.0),
            a.y.0.max(b.y.0),
        ]);
    }
    ink
}

/// The SMuFL codepoint of a glyph a grace note draws: a notehead, a flag, an
/// accidental, a dot or a mark (Bravura 1.392, as `epiphany-glyphs` bundles
/// them).
pub(crate) fn codepoint(name: &str) -> Option<char> {
    Some(match name {
        "accidentalDoubleFlat" => '\u{E264}',
        "accidentalDoubleSharp" => '\u{E263}',
        "accidentalFiveQuarterTonesFlatArrowDown" => '\u{E279}',
        "accidentalFiveQuarterTonesSharpArrowUp" => '\u{E276}',
        "accidentalFlat" => '\u{E260}',
        "accidentalNatural" => '\u{E261}',
        "accidentalQuarterToneFlatArrowUp" => '\u{E270}',
        "accidentalQuarterToneFlatNaturalArrowDown" => '\u{E273}',
        "accidentalQuarterToneFlatStein" => '\u{E280}',
        "accidentalQuarterToneSharpArrowDown" => '\u{E275}',
        "accidentalQuarterToneSharpNaturalArrowUp" => '\u{E272}',
        "accidentalQuarterToneSharpStein" => '\u{E282}',
        "accidentalSharp" => '\u{E262}',
        "accidentalThreeQuarterTonesFlatArrowDown" => '\u{E271}',
        "accidentalThreeQuarterTonesFlatArrowUp" => '\u{E278}',
        "accidentalThreeQuarterTonesFlatZimmermann" => '\u{E281}',
        "accidentalThreeQuarterTonesSharpArrowDown" => '\u{E277}',
        "accidentalThreeQuarterTonesSharpArrowUp" => '\u{E274}',
        "accidentalThreeQuarterTonesSharpStein" => '\u{E283}',
        "articAccentAbove" => '\u{E4A0}',
        "articAccentBelow" => '\u{E4A1}',
        "articMarcatoAbove" => '\u{E4AC}',
        "articMarcatoBelow" => '\u{E4AD}',
        "articStaccatissimoAbove" => '\u{E4A6}',
        "articStaccatissimoBelow" => '\u{E4A7}',
        "articStaccatissimoStrokeAbove" => '\u{E4AA}',
        "articStaccatissimoStrokeBelow" => '\u{E4AB}',
        "articStaccatoAbove" => '\u{E4A2}',
        "articStaccatoBelow" => '\u{E4A3}',
        "articStressAbove" => '\u{E4B6}',
        "articStressBelow" => '\u{E4B7}',
        "articTenutoAbove" => '\u{E4A4}',
        "articTenutoBelow" => '\u{E4A5}',
        "articTenutoStaccatoAbove" => '\u{E4B2}',
        "articTenutoStaccatoBelow" => '\u{E4B3}',
        "articUnstressAbove" => '\u{E4B8}',
        "articUnstressBelow" => '\u{E4B9}',
        "augmentationDot" => '\u{E1E7}',
        "brassMuteClosed" => '\u{E5E5}',
        "brassMuteOpen" => '\u{E5E7}',
        "flag16thDown" => '\u{E243}',
        "flag16thUp" => '\u{E242}',
        "flag32ndDown" => '\u{E245}',
        "flag32ndUp" => '\u{E244}',
        "flag64thDown" => '\u{E247}',
        "flag64thUp" => '\u{E246}',
        "flag8thDown" => '\u{E241}',
        "flag8thUp" => '\u{E240}',
        "noteheadBlack" => '\u{E0A4}',
        "noteheadDoubleWhole" => '\u{E0A0}',
        "noteheadHalf" => '\u{E0A3}',
        "noteheadWhole" => '\u{E0A2}',
        "pluckedSnapPizzicatoAbove" => '\u{E631}',
        "pluckedSnapPizzicatoBelow" => '\u{E630}',
        "stringsDownBow" => '\u{E610}',
        "stringsHarmonic" => '\u{E614}',
        "stringsUpBow" => '\u{E612}',
        _ => return None,
    })
}

/// A glyph a grace note draws, set at its size from Bravura Text with its
/// origin at `origin`: its ink is Bravura's box for the glyph scaled about
/// that origin.
pub(crate) fn small_glyph(
    provenance: Provenance,
    name: &str,
    origin: Point,
) -> Option<ResolvedText> {
    let b = metrics(name)?.bounding_box();
    let symbol = Symbol {
        codepoint: codepoint(name)?,
        at: SymbolPlace::Corner(Point::new(GRACE_MAG * b.left.0, GRACE_MAG * b.bottom.0)),
        scale: GRACE_MAG,
        turn: Turn::None,
    };
    Some(symbol_run(provenance, &[symbol], origin))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each glyph a grace draws stands at 0.7 of Bravura's box for it.
    #[test]
    fn a_grace_glyph_stands_at_its_size() {
        use epiphany_core::{MarkerId, TypedObjectId};
        let provenance =
            Provenance::projected(TypedObjectId::Marker(MarkerId::from_raw(1)), vec![]);
        for name in [
            "noteheadBlack",
            "noteheadHalf",
            "flag8thUp",
            "flag16thDown",
            "accidentalSharp",
            "accidentalFlat",
            "accidentalQuarterToneFlatStein",
        ] {
            let run = small_glyph(provenance.clone(), name, Point::new(10.0, 3.0)).expect(name);
            let b = metrics(name).expect(name).bounding_box();
            let want = [
                10.0 + 0.7 * b.left.0,
                3.0 + 0.7 * b.bottom.0,
                10.0 + 0.7 * b.right.0,
                3.0 + 0.7 * b.top.0,
            ];
            let got = [
                run.origin.x.0 + run.bounds.left.0,
                run.origin.y.0 + run.bounds.bottom.0,
                run.origin.x.0 + run.bounds.right.0,
                run.origin.y.0 + run.bounds.top.0,
            ];
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 0.02, "{name}: {got:?} against {want:?}");
            }
        }
    }

    /// A group stands right to left: its last grace 0.45 clear of the
    /// principal's leftmost ink, each before it 0.3 clear of the next.
    #[test]
    fn a_group_stands_right_to_left_clear_of_its_principal() {
        let grace = vec![[0.0, -0.5, 1.0, 1.5]];
        let principal = vec![[-0.8, -0.5, 0.0, 0.5], [0.0, -0.5, 1.18, 0.5]];
        let shifts = place_group(&[grace.clone(), grace.clone()], &principal);
        assert!((shifts[1] - (-0.8 - 0.45 - 1.0)).abs() < 1e-5, "{shifts:?}");
        assert!(
            (shifts[0] - (shifts[1] - 0.3 - 1.0)).abs() < 1e-5,
            "{shifts:?}"
        );
    }

    /// A grace of `steps` (half spaces up from the staff's bottom line, at
    /// 0) set out about x 0, its stem up 2¼ from its highest head.
    fn shape_at(steps: &[i32]) -> GraceShape {
        let k = GRACE_MAG;
        let hb = metrics("noteheadBlack").expect("bundled").bounding_box();
        let ys: Vec<f32> = steps.iter().map(|s| *s as f32 * 0.5).collect();
        let x = k * hb.right.0;
        GraceShape {
            heads: ys
                .iter()
                .map(|y| (Point::new(0.0, *y), "noteheadBlack", Some(0)))
                .collect(),
            stem: Some((x, ys[0], ys[ys.len() - 1] + GRACE_STEM)),
            up: true,
            ..Default::default()
        }
    }

    /// The side Gould's examples give (*Behind Bars*, p. 130): under the
    /// staff for a grace under it and for one low on it, its stem up; over
    /// the staff for one high on it and for a pair over it.
    #[test]
    fn a_graces_marks_take_the_side_nearer_its_heads_outside_the_staff() {
        let side = |steps: &[&[i32]]| {
            let shapes: Vec<GraceShape> = steps.iter().map(|s| shape_at(s)).collect();
            marks_above(&shapes.iter().collect::<Vec<_>>(), 0.0)
        };
        assert!(!side(&[&[-4]]), "under the staff: under");
        assert!(!side(&[&[2]]), "low on the staff: under");
        assert!(side(&[&[7]]), "high on the staff: over");
        assert!(side(&[&[10], &[12]]), "a pair over the staff: over");
        assert!(
            !side(&[&[-4], &[-3], &[-2]]),
            "a run under the staff: under"
        );
    }

    /// A grace's mark stands clear of its staff and its own ink by 0.4, and
    /// of the ink of its column it meets across its width by 0.4 more, as
    /// X4d.3's column rule sets a note's outside marks.
    #[test]
    fn a_graces_mark_stands_clear_of_its_columns_ink() {
        let shape = shape_at(&[10]);
        let mark = [(1, "articStaccatoAbove", "articStaccatoBelow", false)];
        let alone = place_marks(&shape, &mark, 0.0, true, &[]);
        let b = metrics("articStaccatoAbove")
            .expect("bundled")
            .bounding_box();
        let bottom = |m: &GraceMark| m.origin.y.0 + GRACE_MAG * b.bottom.0;
        let tip = 5.0 + GRACE_STEM;
        assert!((bottom(&alone[0]) - (tip + 0.4)).abs() < 1e-4, "{alone:?}");
        // Another voice's ink across it, a space over its stem's tip.
        let other = [[-1.0, tip + 0.5, 2.0, tip + 1.0]];
        let lifted = place_marks(&shape, &mark, 0.0, true, &other);
        assert!(
            (bottom(&lifted[0]) - (tip + 1.4)).abs() < 1e-4,
            "{lifted:?}"
        );
        // Ink beside it, not across, leaves it.
        let beside = [[3.0, tip + 0.5, 4.0, tip + 1.0]];
        assert_eq!(place_marks(&shape, &mark, 0.0, true, &beside), alone);
    }

    /// A single grace's slash rises at 40° across its stem from 1.4 under an
    /// up stem's tip to its flag's right edge.
    #[test]
    fn a_single_graces_slash_crosses_its_stem_to_its_flag() {
        let (a, b) = slash(1.0, 0.084, 3.0, true, 0.826, Some(1.8), 0.0);
        assert!((a.y.0 - (3.0 - 1.4)).abs() < 1e-5, "{a:?}");
        assert!((a.x.0 - (1.0 + 0.042 - 0.413)).abs() < 1e-5, "{a:?}");
        let rise = (b.y.0 - a.y.0) / (b.x.0 - a.x.0);
        assert!((rise - 40f32.to_radians().tan()).abs() < 1e-5, "{rise}");
        assert_eq!(b.x.0, 1.8);
    }
}
