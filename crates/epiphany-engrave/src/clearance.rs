//! A slur or tie clears the accidentals under it (`ENGRAVER_VERSION` 44) and
//! the stems of its staff inside its span (`ENGRAVER_VERSION` 48, X5c.3): a
//! stem's ink is passed as an accidental's is. A tie whose start runs through
//! its first note's flag starts past the flag first ([`start_past_flag`],
//! `ENGRAVER_VERSION` 49, X5c.4).
//!
//! The constrained pass shapes a slur over the heads and stems of the columns
//! it spans and a tie between its heads, in a frame where columns stand closer
//! than the spacing will set them; an accidental stands at a fixed distance
//! left of its head, so where it falls along the curve moves when the curve
//! stretches. The clearance is therefore taken here, after spacing, in the
//! frame the page is drawn in: each accidental glyph of the curve's staff that
//! the curve passes through is passed over, on the side the curve arcs to.
//!
//! A curve meets an accidental where, at some x across the glyph's box, it
//! stands past the box's near edge (its bottom, for a curve arcing above) and
//! short of its far edge by less than [`ACCIDENTAL_CLEARANCE`]. The worst such
//! meeting is met first, and again until none is left or a bound is reached:
//! - in the middle of the curve, by raising its arc, both inner control points
//!   moving off the chord alike, so the arc keeps its shape;
//! - near a slur's end, where the arc rises too slowly to pass, or where the
//!   arc would grow past [`MAX_ARC`], by lifting that end, the inner control
//!   points moving with the chord.
//!
//! A tie's ends stay at its heads, so a tie only raises its arc, and first only
//! as far as [`MAX_TIE_ARC`]. A tie that would have to arc further to pass
//! every accidental it meets (a long tie through another voice's notes, which
//! MuseScore draws through them) takes a fuller arc instead, its inner control
//! points moved toward its ends ([`FULL_TIE_SHARE`]) so it leaves its heads
//! more steeply and keeps its height over its middle, and may arc as far as
//! [`LONG_TIE_SHARE`] of its span, never past [`MAX_LONG_TIE_ARC`] (X5c.2,
//! `ENGRAVER_VERSION` 47); one that cannot pass so either is left as it was,
//! not half raised.

use epiphany_layout_ir::Point;

/// How far a curve stands clear of an accidental it passes, in staff spaces:
/// as far as an accidental stands from a head.
pub(crate) const ACCIDENTAL_CLEARANCE: f32 = 0.2;

/// The share of a curve's span, at either end, in which a slur lifts its end
/// rather than its arc to pass an accidental.
const NEAR_END: f32 = 0.2;

/// The most a curve's arc stands off its chord once raised for an accidental:
/// a third of its span, and never more than four staff spaces.
const MAX_ARC: f32 = 4.0;

/// The most a tie's arc stands off its chord once raised for an accidental,
/// in staff spaces: under twice the tallest a tie arcs by its length.
const MAX_TIE_ARC: f32 = 1.5;

/// Where a long tie's inner control points stand once it takes a fuller arc,
/// as a share of its span from each end: under a third, so the arc rises
/// sooner off its heads and stays near its height across its middle.
const FULL_TIE_SHARE: f32 = 0.125;

/// The most a long tie arcs once it takes a fuller arc, as a share of its
/// span: half a slur's third, so a tie stays flatter than a slur of its
/// length.
const LONG_TIE_SHARE: f32 = 1.0 / 6.0;

/// The most a long tie arcs, in staff spaces, however long it is.
const MAX_LONG_TIE_ARC: f32 = 3.0;

/// The least a tie runs once it starts past its first note's flag, in staff
/// spaces.
const MIN_TIE_SPAN: f32 = 1.0;

/// How many accidentals one curve passes, at most.
const MAX_PASSES: usize = 24;

/// An accidental's ink on the page, in staff spaces.
#[derive(Copy, Clone, Debug)]
pub(crate) struct InkRect {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
}

fn bezier(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
}

/// The parameter at which the curve stands at `x`, by bisection: a slur's and a
/// tie's control points stand in x order, so x rises with the parameter.
fn parameter_at(cp: &[Point; 4], x: f32) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if bezier(cp[0].x.0, cp[1].x.0, cp[2].x.0, cp[3].x.0, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// `cp` with its arc raised and, for a slur, its ends lifted until it passes
/// every accidental in `accidentals` it would otherwise meet. A curve that
/// does not arc (its inner control points on its chord) is left as it is.
pub(crate) fn clear_accidentals(cp: [Point; 4], accidentals: &[InkRect], tie: bool) -> [Point; 4] {
    let (x0, x3) = (cp[0].x.0, cp[3].x.0);
    let span = x3 - x0;
    if span <= 1e-3 {
        return cp;
    }
    let initial = offset(&cp);
    if initial.abs() < 1e-4 {
        return cp;
    }
    let near: Vec<&InkRect> = accidentals
        .iter()
        .filter(|b| b.right > x0 && b.left < x3)
        .collect();
    if !tie {
        return pass(cp, &near, (span / 3.0).min(MAX_ARC), false).0;
    }
    match pass(cp, &near, MAX_TIE_ARC, true) {
        (cleared, true) => cleared,
        // A long tie takes a fuller arc and may arc further, in proportion to
        // its span; one that cannot pass so either is left as it was.
        _ => {
            let mut full = cp;
            full[1].x.0 = x0 + span * FULL_TIE_SHARE;
            full[2].x.0 = x3 - span * FULL_TIE_SHARE;
            let reach = (span * LONG_TIE_SHARE).clamp(MAX_TIE_ARC, MAX_LONG_TIE_ARC);
            match pass(full, &near, reach, true) {
                (cleared, true) if reach > MAX_TIE_ARC => cleared,
                _ => cp,
            }
        }
    }
}

/// `cp`, a tie, starting past any flag at its start that it runs through: its
/// first note's flag, standing on the tie's side of its head, which the tie
/// would otherwise leave its head into. The start moves right to the flag's
/// right edge and [`ACCIDENTAL_CLEARANCE`] beyond, at the same height, and the
/// inner control points keep their shares of the shorter span; a tie the move
/// would leave shorter than [`MIN_TIE_SPAN`] is kept as it was.
pub(crate) fn start_past_flag(cp: [Point; 4], flags: &[InkRect]) -> [Point; 4] {
    let (x0, x3) = (cp[0].x.0, cp[3].x.0);
    let span = x3 - x0;
    if span <= 1e-3 {
        return cp;
    }
    let meets = |b: &InkRect| {
        (1..64).any(|k| {
            let t = k as f32 / 64.0;
            let x = bezier(cp[0].x.0, cp[1].x.0, cp[2].x.0, cp[3].x.0, t);
            let y = bezier(cp[0].y.0, cp[1].y.0, cp[2].y.0, cp[3].y.0, t);
            x > b.left && x < b.right && y > b.bottom && y < b.top
        })
    };
    let start = flags
        .iter()
        .filter(|b| b.left < x0 + 0.5 && b.right > x0 && meets(b))
        .map(|b| b.right + ACCIDENTAL_CLEARANCE)
        .fold(x0, f32::max);
    if start <= x0 || x3 - start < MIN_TIE_SPAN {
        return cp;
    }
    let along = |x: f32| start + (x - x0) * (x3 - start) / span;
    [
        Point::new(start, cp[0].y.0),
        Point::new(along(cp[1].x.0), cp[1].y.0),
        Point::new(along(cp[2].x.0), cp[2].y.0),
        cp[3],
    ]
}

/// The inner control points' offsets from the chord, at its thirds.
fn offset(cp: &[Point; 4]) -> f32 {
    let chord = |f: f32| cp[0].y.0 + f * (cp[3].y.0 - cp[0].y.0);
    (cp[1].y.0 - chord(1.0 / 3.0) + cp[2].y.0 - chord(2.0 / 3.0)) / 2.0
}

/// `cp` raised, and for a slur its ends lifted, past each accidental of
/// `near` it meets, its arc kept within `max_arc` (or its own, if higher);
/// and whether it now passes every one. A tie only raises its arc.
fn pass(mut cp: [Point; 4], near: &[&InkRect], max_arc: f32, tie: bool) -> ([Point; 4], bool) {
    let (x0, x3) = (cp[0].x.0, cp[3].x.0);
    let initial = offset(&cp);
    let side = initial.signum();
    // The arc's height off its chord at its middle is three quarters of the
    // control points' offset.
    let max_offset = (initial.abs() * 0.75).max(max_arc) / 0.75;
    for _ in 0..MAX_PASSES {
        // The worst meeting: how far the curve must move off its chord, and
        // where along it.
        let mut worst: Option<(f32, f32)> = None;
        for b in near {
            let (near_edge, far_edge) = if side > 0.0 {
                (b.bottom, b.top)
            } else {
                (b.top, b.bottom)
            };
            let xs = [b.left.max(x0), 0.5 * (b.left + b.right), b.right.min(x3)];
            for x in xs {
                if x <= x0 || x >= x3 {
                    continue;
                }
                let t = parameter_at(&cp, x);
                if !(1e-3..=1.0 - 1e-3).contains(&t) {
                    continue;
                }
                let y = bezier(cp[0].y.0, cp[1].y.0, cp[2].y.0, cp[3].y.0, t);
                let past_near = side * y > side * near_edge - ACCIDENTAL_CLEARANCE;
                let deficit = side * far_edge + ACCIDENTAL_CLEARANCE - side * y;
                if past_near && deficit > 1e-4 && worst.is_none_or(|(d, _)| deficit > d) {
                    worst = Some((deficit, t));
                }
            }
        }
        let Some((deficit, t)) = worst else {
            return (cp, true);
        };
        // Raising both inner control points by `d` raises the arc at `t` by
        // `3t(1-t)d`.
        let raise = deficit / (3.0 * t * (1.0 - t));
        let within = (offset(&cp).abs() + raise) <= max_offset;
        let at_end = !(NEAR_END..=1.0 - NEAR_END).contains(&t);
        if within && (tie || !at_end) {
            cp[1].y.0 += side * raise;
            cp[2].y.0 += side * raise;
        } else if tie {
            // A tie keeps its ends at its heads and its arc within bounds.
            return (cp, false);
        } else if t > 0.5 {
            // Lifting the end by `r` lifts the curve at `t` by `t·r`.
            let lift = side * deficit / t;
            cp[1].y.0 += lift / 3.0;
            cp[2].y.0 += lift * 2.0 / 3.0;
            cp[3].y.0 += lift;
        } else {
            let lift = side * deficit / (1.0 - t);
            cp[0].y.0 += lift;
            cp[1].y.0 += lift * 2.0 / 3.0;
            cp[2].y.0 += lift / 3.0;
        }
    }
    // Still meeting one after every pass: a slur keeps what it has cleared.
    (cp, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slur(x0: f32, x3: f32, y: f32, lift: f32) -> [Point; 4] {
        let span = x3 - x0;
        [
            Point::new(x0, y),
            Point::new(x0 + span / 3.0, y + lift),
            Point::new(x3 - span / 3.0, y + lift),
            Point::new(x3, y),
        ]
    }

    /// The curve's least clearance over `b`'s width on its arc side (negative
    /// where it runs through the box).
    fn clearance(cp: &[Point; 4], b: &InkRect, side: f32) -> f32 {
        (0..=40)
            .map(|k| b.left + (b.right - b.left) * k as f32 / 40.0)
            .filter(|x| *x > cp[0].x.0 && *x < cp[3].x.0)
            .map(|x| {
                let t = parameter_at(cp, x);
                let y = bezier(cp[0].y.0, cp[1].y.0, cp[2].y.0, cp[3].y.0, t);
                let far = if side > 0.0 { b.top } else { b.bottom };
                side * (y - far)
            })
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn a_slur_passes_over_an_accidental_in_its_middle_by_its_arc() {
        let cp = slur(0.0, 10.0, 1.0, 1.0);
        let sharp = InkRect {
            left: 4.0,
            right: 5.0,
            bottom: 0.0,
            top: 2.5,
        };
        assert!(clearance(&cp, &sharp, 1.0) < 0.0, "the slur meets it");
        let cleared = clear_accidentals(cp, &[sharp], false);
        assert!(clearance(&cleared, &sharp, 1.0) >= ACCIDENTAL_CLEARANCE - 1e-3);
        assert_eq!(cleared[0].y.0, cp[0].y.0, "its ends stay");
        assert_eq!(cleared[3].y.0, cp[3].y.0, "its ends stay");
    }

    #[test]
    fn a_slur_lifts_its_end_over_its_last_notes_accidental() {
        let cp = slur(0.0, 10.0, 1.0, 1.0);
        let flat = InkRect {
            left: 8.6,
            right: 9.2,
            bottom: -0.2,
            top: 2.3,
        };
        assert!(clearance(&cp, &flat, 1.0) < 0.0, "the slur meets it");
        let cleared = clear_accidentals(cp, &[flat], false);
        assert!(clearance(&cleared, &flat, 1.0) >= ACCIDENTAL_CLEARANCE - 1e-3);
        assert!(cleared[3].y.0 > cp[3].y.0, "the end lifts");
        assert_eq!(cleared[0].y.0, cp[0].y.0, "the start stays");
    }

    #[test]
    fn a_slur_below_passes_under_an_accidental() {
        let cp = slur(0.0, 10.0, -1.0, -1.0);
        let natural = InkRect {
            left: 3.0,
            right: 3.6,
            bottom: -2.6,
            top: 0.0,
        };
        let cleared = clear_accidentals(cp, &[natural], false);
        assert!(clearance(&cleared, &natural, -1.0) >= ACCIDENTAL_CLEARANCE - 1e-3);
    }

    #[test]
    fn a_tie_raises_its_arc_and_keeps_its_ends() {
        let span = 12.0;
        let tie = [
            Point::new(0.0, 0.0),
            Point::new(span / 4.0, -1.0),
            Point::new(span * 3.0 / 4.0, -1.0),
            Point::new(span, 0.0),
        ];
        let sharp = InkRect {
            left: 5.0,
            right: 5.8,
            bottom: -1.2,
            top: 1.2,
        };
        assert!(clearance(&tie, &sharp, -1.0) < 0.0, "the tie meets it");
        let cleared = clear_accidentals(tie, &[sharp], true);
        assert!(clearance(&cleared, &sharp, -1.0) >= ACCIDENTAL_CLEARANCE - 1e-3);
        assert_eq!((cleared[0], cleared[3]), (tie[0], tie[3]), "its ends stay");
    }

    #[test]
    fn a_tie_that_cannot_pass_within_bounds_is_left_as_it_was() {
        let span = 12.0;
        let tie = [
            Point::new(0.0, 0.0),
            Point::new(span / 4.0, -1.0),
            Point::new(span * 3.0 / 4.0, -1.0),
            Point::new(span, 0.0),
        ];
        let sharps = [
            // Passable within bounds, alone.
            InkRect {
                left: 5.0,
                right: 5.8,
                bottom: -1.6,
                top: 1.2,
            },
            // Just past the start, reaching deep: not within bounds.
            InkRect {
                left: 0.6,
                right: 1.4,
                bottom: -2.5,
                top: 0.5,
            },
        ];
        assert_eq!(clear_accidentals(tie, &sharps, true), tie);
    }

    /// A long tie that must arc further than a short tie may takes a fuller
    /// arc, its inner control points moved toward its ends, and passes within
    /// its share of its span; a short tie needing as much is left as it was.
    #[test]
    fn a_long_tie_takes_a_fuller_arc_to_pass_and_a_short_one_does_not() {
        let tie = |span: f32| {
            [
                Point::new(0.0, 0.0),
                Point::new(span / 4.0, 1.0),
                Point::new(span * 3.0 / 4.0, 1.0),
                Point::new(span, 0.0),
            ]
        };
        // An accidental a step above the tie's heads, at its middle: passing
        // it takes 2.4 spaces of arc.
        let flat = |middle: f32| InkRect {
            left: middle - 0.4,
            right: middle + 0.4,
            bottom: 0.3,
            top: 2.2,
        };
        let long = tie(15.0);
        let over = flat(7.5);
        assert!(clearance(&long, &over, 1.0) < 0.0, "the tie meets it");
        let cleared = clear_accidentals(long, &[over], true);
        assert!(clearance(&cleared, &over, 1.0) >= ACCIDENTAL_CLEARANCE - 1e-3);
        assert_eq!(
            (cleared[0], cleared[3]),
            (long[0], long[3]),
            "its ends stay"
        );
        assert!(
            cleared[1].x.0 < long[1].x.0 && cleared[2].x.0 > long[2].x.0,
            "its inner points move toward its ends"
        );
        assert!(offset(&cleared) * 0.75 <= 15.0 * LONG_TIE_SHARE + 1e-3);
        let short = tie(8.0);
        assert_eq!(clear_accidentals(short, &[flat(4.0)], true), short);
    }

    #[test]
    fn an_accidental_beside_the_curve_moves_nothing() {
        let cp = slur(0.0, 10.0, 1.0, 1.0);
        let clear = [
            // Below the arc, under its chord.
            InkRect {
                left: 4.0,
                right: 5.0,
                bottom: -2.0,
                top: 0.5,
            },
            // Past either end.
            InkRect {
                left: -2.0,
                right: -1.0,
                bottom: 0.0,
                top: 4.0,
            },
            // Far above the arc.
            InkRect {
                left: 4.0,
                right: 5.0,
                bottom: 6.0,
                top: 8.0,
            },
        ];
        assert_eq!(clear_accidentals(cp, &clear, false), cp);
    }
}
