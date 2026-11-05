//! A rest filling its measure cleared where it stands (X5d fix round 1): its
//! height is decided once its system has set it across, at its final x, from
//! its default height, by whole spaces, reading the box of the glyph it then
//! draws, since a whole or half rest off its staff takes a wider ledgered
//! glyph (*Behind Bars*, p. 37).
//!
//! A rest leaves the middle of its staff only as far as it takes to keep the
//! two parts apart (Gould, p. 36), and every sign stays clear of the next
//! (p. 41). Gould gives no figures. These are MuseScore's, `RestLayout::checkFullMeasureRestCollisions`:
//! [`REST_CLEARANCE`] up and down, against ink within [`REST_REACH`] across.
//! Its exclusions are not copied: it takes accidentals out of what it reads,
//! so a rest stands on a sharp, and it runs only where the measure has voices,
//! so a rest stands on a visitor's head; both are read here.
//!
//! A tie or slur is read as well, only where it passes through the rest:
//! neither Gould nor MuseScore lets a rest make way for a curve near it, but
//! nothing may stand on it either.
//!
//! A voice's rest keeps its side of the other part, as Gould takes a ledger
//! line to the far side of it (p. 37): above all of that ink for an
//! upper voice, below it for a lower. A rest alone on its staff has no part to
//! keep outside: it moves only where visiting ink stands that near it, from
//! the drawn stems it meets, and between stems of both ways, or where it meets
//! none, to the side whose first clear place stands nearer its staff's middle
//! line (D109 Q8), a tie going down as it always has. A stemless visitor turns
//! nothing, and a rest goes no further than the parts need (Gould, p. 36).

use epiphany_layout_ir::{metrics, BoundingBox};

/// How far up and down a rest filling its measure stands clear of ink.
pub(crate) const REST_CLEARANCE: f32 = 0.75;

/// How far across a rest filling its measure reads ink.
pub(crate) const REST_REACH: f32 = 4.0;

/// How far up and down it stands from a curve that passes across it.
pub(crate) const CURVE_CLEARANCE: f32 = 0.25;

/// How far off the ink of a rest moved from its height a slur's end stands on
/// the arc's side: MuseScore's figure (`SlurTieLayout::slurPos`), LilyPond also
/// taking a rest in its column as the head a slur's end reads
/// (`slur-scoring.cc`). Gould is silent on a slur from a rest.
pub(crate) const SLUR_OFF_REST: f32 = 0.9;

/// One piece of ink a rest reads, a stroke's edges running straight across it.
#[derive(Copy, Clone, Debug)]
pub(crate) struct RestInk {
    pub left: f32,
    pub right: f32,
    /// Its bottom and top edges at its left.
    pub at_left: (f32, f32),
    /// Its bottom and top edges at its right.
    pub at_right: (f32, f32),
}

impl RestInk {
    /// A box's ink.
    pub(crate) fn boxed(left: f32, right: f32, bottom: f32, top: f32) -> Self {
        RestInk {
            left,
            right,
            at_left: (bottom, top),
            at_right: (bottom, top),
        }
    }

    /// Its lowest and highest ink between two x, where it stands there.
    fn within(&self, left: f32, right: f32) -> Option<(f32, f32)> {
        let (from, to) = (left.max(self.left), right.min(self.right));
        if from > to {
            return None;
        }
        let at = |x: f32, (a, b): (f32, f32)| {
            if self.right - self.left <= f32::EPSILON {
                a
            } else {
                a + (b - a) * (x - self.left) / (self.right - self.left)
            }
        };
        let bottom = at(from, (self.at_left.0, self.at_right.0))
            .min(at(to, (self.at_left.0, self.at_right.0)));
        let top = at(from, (self.at_left.1, self.at_right.1))
            .max(at(to, (self.at_left.1, self.at_right.1)));
        Some((bottom, top))
    }
}

/// A rest filling its measure as it reads its ink.
pub(crate) struct RestToClear<'a> {
    /// Its name on its staff, before any ledger line.
    pub name: &'static str,
    /// Its default height.
    pub y: f32,
    /// Its staff's bottom and top lines.
    pub lines: (f32, f32),
    /// Its voice's side, up or down; `None` for a rest alone on its staff.
    pub up: Option<bool>,
    /// Where its centre stands across; `None` where it keeps its column, then
    /// the x its glyph stands at.
    pub centre: Option<f32>,
    pub column: f32,
    /// The ink it reads: the ink of its staff's note columns in its measure.
    pub ink: &'a [RestInk],
    /// The curves of its staff, each a run of chords: a tie or slur moves it
    /// only where it passes through it.
    pub curves: &'a [RestInk],
    /// Each visitor's drawn stem and heads across its staff, and whether its stem
    /// turns up.
    pub stems: &'a [(f32, f32, bool)],
}

/// Where such a rest stands, and what it draws.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct RestPlace {
    pub y: f32,
    pub name: &'static str,
    pub bounds: BoundingBox,
    /// Its glyph's x.
    pub x: f32,
}

/// The ledgered form a rest takes at a height; a rest's name otherwise.
fn glyph_at(rest: &RestToClear, y: f32) -> (&'static str, BoundingBox) {
    let name = epiphany_layout_ir::ledgered_rest(rest.name, y, rest.lines);
    let bounds = metrics(name)
        .or_else(|| metrics(rest.name))
        .expect("a rest's metrics are bundled")
        .bounding_box();
    (name, bounds)
}

fn glyph_x(rest: &RestToClear, b: &BoundingBox) -> f32 {
    rest.centre
        .map_or(rest.column, |centre| centre - (b.left.0 + b.right.0) * 0.5)
}

/// The rest's box at a height.
fn rest_box(rest: &RestToClear, y: f32) -> (f32, f32, f32, f32) {
    let (_, b) = glyph_at(rest, y);
    let x = glyph_x(rest, &b);
    (x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0)
}

/// The rest's place, cleared of its ink.
pub(crate) fn clear(rest: &RestToClear) -> RestPlace {
    // Every piece of ink past the rest's reach is read no further; it stands
    // clear of the ink within it after at most the ink's height and its own.
    let extent = rest.ink.iter().chain(rest.curves).fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(lo, hi), i| {
            (
                lo.min(i.at_left.0).min(i.at_right.0),
                hi.max(i.at_left.1).max(i.at_right.1),
            )
        },
    );
    let bound = (extent.1 - extent.0).abs() + (rest.y - extent.0).abs() + 16.0;
    let reach = |y: f32| {
        let (left, right, ..) = rest_box(rest, y);
        rest.ink
            .iter()
            .filter_map(move |i| i.within(left - REST_REACH, right + REST_REACH))
    };
    let off_curves = |y: f32| {
        let (left, right, bottom, top) = rest_box(rest, y);
        rest.curves
            .iter()
            .filter_map(|i| i.within(left, right))
            .all(|(b, t)| !(bottom < t + CURVE_CLEARANCE && b < top + CURVE_CLEARANCE))
    };
    let walk = |clear_at: &dyn Fn(f32) -> bool, step: f32| {
        let (mut y, mut moved) = (rest.y, 0.0);
        while moved < bound && !clear_at(y) {
            y += step;
            moved += 1.0;
        }
        y
    };
    let y = match rest.up {
        // The whole of the other part, whichever way it stands.
        Some(true) => walk(
            &|y| {
                let (_, _, bottom, _) = rest_box(rest, y);
                reach(y).all(|(_, top)| bottom - top >= REST_CLEARANCE) && off_curves(y)
            },
            1.0,
        ),
        Some(false) => walk(
            &|y| {
                let (_, _, _, top) = rest_box(rest, y);
                reach(y).all(|(bottom, _)| bottom - top >= REST_CLEARANCE) && off_curves(y)
            },
            -1.0,
        ),
        None => {
            let clear_at = |y: f32| {
                let (_, _, bottom, top) = rest_box(rest, y);
                reach(y).all(|(b, t)| !(bottom < t + REST_CLEARANCE && b < top + REST_CLEARANCE))
                    && off_curves(y)
            };
            if clear_at(rest.y) {
                rest.y
            } else {
                let (left, right, ..) = rest_box(rest, rest.y);
                let near = rest
                    .stems
                    .iter()
                    .filter(|(l, r, _)| *l < right + REST_REACH && left - REST_REACH < *r);
                let (ups, downs) =
                    near.fold((false, false), |(u, d), (_, _, up)| (u || *up, d || !*up));
                let up = if ups == downs {
                    let middle = (rest.lines.0 + rest.lines.1) * 0.5;
                    let off = |y: f32| {
                        let (_, _, bottom, top) = rest_box(rest, y);
                        ((bottom + top) * 0.5 - middle).abs()
                    };
                    off(walk(&clear_at, 1.0)) + 1e-3 < off(walk(&clear_at, -1.0))
                } else {
                    !ups
                };
                walk(&clear_at, if up { 1.0 } else { -1.0 })
            }
        }
    };
    let (name, bounds) = glyph_at(rest, y);
    RestPlace {
        y,
        name,
        bounds,
        x: glyph_x(rest, &bounds),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whole(up: Option<bool>, ink: &[RestInk], stems: &[(f32, f32, bool)]) -> RestPlace {
        // A whole rest hanging from the top line of a staff 0 to 4, or from
        // the fourth line alone, a lower voice's from the middle line.
        let y = match up {
            Some(true) => 4.0,
            Some(false) => 2.0,
            None => 3.0,
        };
        clear(&RestToClear {
            name: "restWhole",
            y,
            lines: (0.0, 4.0),
            up,
            centre: Some(10.0),
            column: 0.0,
            ink,
            curves: &[],
            stems,
        })
    }

    #[test]
    fn a_voice_with_no_ink_near_it_keeps_its_height() {
        let far = [RestInk::boxed(30.0, 31.0, 3.0, 4.0)];
        assert_eq!(whole(Some(true), &far, &[]).y, 4.0);
        assert_eq!(whole(Some(false), &far, &[]).y, 2.0);
        assert_eq!(whole(None, &far, &[]).y, 3.0);
    }

    /// An upper voice's rest stands above the whole of the other part within
    /// its reach, 0.75 clear, by whole spaces, a ledger line taken above the
    /// staff.
    #[test]
    fn an_upper_voice_rises_above_ink_near_it_by_whole_spaces() {
        let ink = [RestInk::boxed(9.0, 10.0, 4.5, 5.5)];
        let place = whole(Some(true), &ink, &[]);
        // Its bottom, 0.54 under its line, 0.75 over 5.5.
        assert_eq!(place.y, 7.0);
        assert_eq!(place.name, "restWholeLegerLine");
    }

    /// One further than the reach is not read, whichever edge.
    #[test]
    fn the_reach_is_four_spaces_from_the_glyph_it_draws() {
        let (left, right) = (10.0 - 1.156 * 0.5, 10.0 + 1.156 * 0.5);
        let over = [RestInk::boxed(right + 3.9, right + 4.9, 4.5, 5.5)];
        assert_eq!(
            whole(Some(true), &over, &[]).y,
            7.0,
            "within it, on the right"
        );
        let past = [RestInk::boxed(right + 4.1, right + 5.1, 4.5, 5.5)];
        assert_eq!(
            whole(Some(true), &past, &[]).y,
            4.0,
            "past it, on the right"
        );
        let over = [RestInk::boxed(left - 4.9, left - 3.9, 4.5, 5.5)];
        assert_eq!(
            whole(Some(true), &over, &[]).y,
            7.0,
            "within it, on the left"
        );
        let past = [RestInk::boxed(left - 5.1, left - 4.1, 4.5, 5.5)];
        assert_eq!(whole(Some(true), &past, &[]).y, 4.0, "past it, on the left");
    }

    /// A lower voice's stands below the other part, its top 0.75 clear.
    #[test]
    fn a_lower_voice_falls_below_ink_near_it() {
        let ink = [RestInk::boxed(9.0, 10.0, 0.5, 1.5)];
        let place = whole(Some(false), &ink, &[]);
        // Its top, 0.04 over its line, 0.75 under 0.5.
        assert_eq!(place.y, -1.0);
        assert_eq!(place.name, "restWholeLegerLine");
    }

    /// The ledger line's width is read at the height that draws it: a rest
    /// clear of the ink as plain is not clear of it as ledgered.
    #[test]
    fn the_ledgered_glyph_is_what_is_read() {
        // The plain glyph reaches 10.578 + 4 = 14.578; the ledgered 11.0775 + 4.
        let ink = [RestInk::boxed(14.8, 15.5, 4.5, 5.5)];
        let plain = whole(Some(true), &ink, &[]);
        assert_eq!(plain.y, 4.0, "its default stands inside the staff, plain");
        // Starting above the top line the same ink is within the reach.
        let raised = clear(&RestToClear {
            name: "restWhole",
            y: 5.0,
            lines: (0.0, 4.0),
            up: Some(true),
            centre: Some(10.0),
            column: 0.0,
            ink: &ink,
            curves: &[],
            stems: &[],
        });
        assert_eq!(raised.y, 7.0);
    }

    /// A slanted stroke reads as it stands within the reach, not its whole
    /// rise.
    #[test]
    fn a_slanted_stroke_is_read_where_it_stands_within_the_reach() {
        let beam = RestInk {
            left: 0.0,
            right: 40.0,
            at_left: (-30.0, -29.5),
            at_right: (10.0, 10.5),
        };
        // At the rest, x about 10, its edges stand near -20; at the reach's
        // right, about 14.6, near -15.
        let place = whole(Some(true), &[beam], &[]);
        assert_eq!(place.y, 4.0);
    }

    /// And a lower voice's reads its bottom where it stands within the reach,
    /// not the stroke's whole fall.
    #[test]
    fn a_lower_voice_reads_a_slanted_stroke_where_it_stands_within_the_reach() {
        let beam = RestInk {
            left: 0.0,
            right: 40.0,
            at_left: (30.0, 30.5),
            at_right: (-10.0, -9.5),
        };
        // At the rest's reach, x about 5.4 to 14.6, its bottom stands at 24.6
        // down to 15.4, all over the rest.
        assert_eq!(whole(Some(false), &[beam], &[]).y, 2.0);
    }

    /// And an upper voice's its top: falling toward the rest within its reach.
    #[test]
    fn an_upper_voice_reads_a_stroke_rising_past_its_reach_where_it_stands() {
        let beam = RestInk {
            left: 0.0,
            right: 40.0,
            at_left: (-30.0, -29.5),
            at_right: (30.0, 30.5),
        };
        // Within the reach its top stands at about -21 to -7.5, under the rest.
        assert_eq!(whole(Some(true), &[beam], &[]).y, 4.0);
    }

    /// A voice's rest keeps past the other part, however far from it that
    /// part stands: it does not stay under a note high over it.
    #[test]
    fn a_voice_stays_outside_the_other_part_however_far() {
        let high = [RestInk::boxed(9.5, 10.5, 9.0, 10.0)];
        // Its bottom 0.75 over 10, its glyph 0.54 under its line, at 12.
        assert_eq!(whole(Some(true), &high, &[]).y, 12.0);
        let low = [RestInk::boxed(9.5, 10.5, -7.0, -6.0)];
        // Its top 0.75 under -7, its ledgered top 0.08 over its line, at -8.
        assert_eq!(whole(Some(false), &low, &[]).y, -8.0);
    }

    /// Only the stems within its reach turn it: a stem up beside it and a stem
    /// down far off leave it turning down, where both ways would take it up.
    #[test]
    fn a_rest_alone_reads_only_the_stems_within_its_reach() {
        let ink = [RestInk::boxed(9.5, 10.5, -0.5, 3.5)];
        let stems = [(9.5, 10.5, true), (30.0, 31.0, false)];
        assert!(whole(None, &ink, &stems).y < -1.0);
    }

    /// A rest alone moves only where visiting ink stands that near it, from
    /// the stems it meets.
    #[test]
    fn a_rest_alone_turns_from_the_stems_it_meets() {
        let ink = [RestInk::boxed(9.5, 10.5, 2.5, 3.5)];
        let down = whole(None, &ink, &[(9.5, 10.5, true)]);
        assert!(down.y < 3.0, "down from a stem up: {}", down.y);
        let up = whole(None, &ink, &[(9.5, 10.5, false)]);
        assert!(up.y > 3.0, "up from a stem down: {}", up.y);
    }

    /// With no drawn stem near it, it takes the nearer side, a tie going down.
    #[test]
    fn a_rest_alone_with_no_stem_near_takes_the_nearer_side() {
        // Ink reaching down onto it from far above: a space down clears it.
        let high = [RestInk::boxed(9.5, 10.5, 3.6, 7.0)];
        assert_eq!(whole(None, &high, &[]).y, 2.0, "down is the nearer");
        // Ink reaching up onto it from far below: a space up clears it.
        let low = [RestInk::boxed(9.5, 10.5, -3.0, 2.0)];
        assert_eq!(whole(None, &low, &[]).y, 4.0, "up is the nearer");
        // Level both ways, by a breve's box: it goes down.
        let level = clear(&RestToClear {
            name: "restDoubleWhole",
            y: 2.0,
            lines: (-5.0, 9.0),
            up: None,
            centre: Some(10.0),
            column: 0.0,
            ink: &[RestInk::boxed(9.5, 10.5, 0.75, 3.25)],
            curves: &[],
            stems: &[],
        });
        assert_eq!(level.y, -1.0);
    }

    /// A rest alone with ink far above it stays: it has no part to keep
    /// outside.
    #[test]
    fn a_rest_alone_goes_no_further_than_collision() {
        let ink = [RestInk::boxed(9.5, 10.5, 8.0, 9.0)];
        assert_eq!(whole(None, &ink, &[(9.5, 10.5, false)]).y, 3.0);
    }

    /// Between visitors of both ways it takes the side whose first clear
    /// place stands nearer its staff's middle line (D109 Q8), at its centre.
    #[test]
    fn a_rest_between_stems_of_both_ways_takes_the_nearer_side() {
        let stems = [(9.5, 10.5, true), (9.5, 10.5, false)];
        // Ink under it reaching up: the way up is short.
        let low = [RestInk::boxed(9.5, 10.5, 0.0, 2.8)];
        assert!(whole(None, &low, &stems).y > 3.0);
        // Ink over it reaching down: the way down is short.
        let high = [RestInk::boxed(9.5, 10.5, 2.4, 6.0)];
        assert!(whole(None, &high, &stems).y < 3.0);
    }

    /// A half rest, whose centre stands off its line: its box decides.
    #[test]
    fn a_half_rest_between_stems_reads_its_boxs_centre() {
        let stems = [(9.5, 10.5, true), (9.5, 10.5, false)];
        let ink = [RestInk::boxed(9.5, 10.5, 2.0, 2.3)];
        let place = clear(&RestToClear {
            name: "restHalf",
            y: 2.0,
            lines: (0.0, 4.0),
            up: None,
            centre: Some(10.0),
            column: 0.0,
            ink: &ink,
            curves: &[],
            stems: &stems,
        });
        // Up to 4, its bottom 0.75 over 2.3 at 3.99 and its centre 2.28 off
        // the middle; down to 0, its top 0.75 under 2.0 and its centre 1.72
        // off, the nearer.
        assert_eq!(place.y, 0.0);
    }

    /// A breve rest's centre stands half a space up, so a whole number of
    /// spaces can stand level both ways, and a tie goes down.
    #[test]
    fn a_breve_rest_level_both_ways_goes_down() {
        let stems = [(9.5, 10.5, true), (9.5, 10.5, false)];
        // Lines far enough apart that it stays plain either way. Up to 4 and
        // down to -1, each 2.5 off the middle by its centre: its bottom 0.75
        // over 3.25, and its top 0.75 under 0.75.
        let ink = [RestInk::boxed(9.5, 10.5, 0.75, 3.25)];
        let place = clear(&RestToClear {
            name: "restDoubleWhole",
            y: 2.0,
            lines: (-5.0, 9.0),
            up: None,
            centre: Some(10.0),
            column: 0.0,
            ink: &ink,
            curves: &[],
            stems: &stems,
        });
        assert_eq!(place.y, -1.0);
    }

    /// A curve moves it only where it passes through it, its voice's way: not
    /// one within its reach beside it.
    #[test]
    fn a_curve_moves_a_rest_only_where_it_passes_through_it() {
        let move_by = |curves: &[RestInk]| {
            clear(&RestToClear {
                name: "restWhole",
                y: 4.0,
                lines: (0.0, 4.0),
                up: Some(true),
                centre: Some(10.0),
                column: 0.0,
                ink: &[],
                curves,
                stems: &[],
            })
            .y
        };
        let through = [RestInk::boxed(8.0, 12.0, 3.6, 3.8)];
        assert_eq!(move_by(&through), 5.0, "up, clear of it by a quarter");
        let beside = [RestInk::boxed(11.0, 14.0, 3.6, 3.8)];
        assert_eq!(move_by(&beside), 4.0, "beside it, within its reach");
        let under = [RestInk::boxed(8.0, 12.0, 2.5, 2.7)];
        assert_eq!(move_by(&under), 4.0, "under it, further than a quarter");
        let near = [RestInk::boxed(8.0, 12.0, 3.1, 3.3)];
        assert_eq!(move_by(&near), 5.0, "nearer than a quarter");
    }

    /// A lower voice's goes down from one, and a rest alone from one with no
    /// stems to turn it, the nearer way.
    #[test]
    fn a_curve_moves_a_lower_voice_down_and_a_rest_alone_the_nearer_way() {
        let at = |up: Option<bool>, y: f32, curves: &[RestInk]| {
            clear(&RestToClear {
                name: "restWhole",
                y,
                lines: (0.0, 4.0),
                up,
                centre: Some(10.0),
                column: 0.0,
                ink: &[],
                curves,
                stems: &[],
            })
            .y
        };
        let through = [RestInk::boxed(8.0, 12.0, 1.6, 1.8)];
        assert_eq!(at(Some(false), 2.0, &through), 1.0);
        let through = [RestInk::boxed(8.0, 12.0, 0.7, 0.9)];
        assert_eq!(at(None, 1.0, &through), 2.0, "under the middle, it rises");
        let through = [RestInk::boxed(8.0, 12.0, 2.6, 2.8)];
        assert_eq!(at(None, 3.0, &through), 2.0, "over the middle, it falls");
    }

    /// Where it keeps its column it reads from there.
    #[test]
    fn a_rest_kept_at_its_column_reads_from_there() {
        let ink = [RestInk::boxed(6.0, 7.0, 4.5, 5.5)];
        let place = clear(&RestToClear {
            name: "restWhole",
            y: 4.0,
            lines: (0.0, 4.0),
            up: Some(true),
            centre: None,
            column: 2.0,
            ink: &ink,
            curves: &[],
            stems: &[],
        });
        assert_eq!(place.y, 7.0);
        assert_eq!(place.x, 2.0);
    }
}
