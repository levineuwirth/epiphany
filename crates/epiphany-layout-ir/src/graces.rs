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

/// The SMuFL codepoint of a glyph a grace note draws: a notehead, a flag, an
/// accidental or a dot (Bravura 1.392, as `epiphany-glyphs` bundles them).
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
        "augmentationDot" => '\u{E1E7}',
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
