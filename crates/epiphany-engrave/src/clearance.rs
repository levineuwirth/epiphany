//! A slur or tie clears the accidentals under it (`ENGRAVER_VERSION` 44).
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
//! A tie's ends stay at its heads, so a tie only raises its arc, and only as
//! far as [`MAX_TIE_ARC`]: a tie that would have to arc further to pass every
//! accidental it meets (a long tie under another voice's notes, which
//! MuseScore draws through them too) is left as it was, not half raised.

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
pub(crate) fn clear_accidentals(
    mut cp: [Point; 4],
    accidentals: &[InkRect],
    tie: bool,
) -> [Point; 4] {
    let (x0, x3) = (cp[0].x.0, cp[3].x.0);
    let span = x3 - x0;
    if span <= 1e-3 {
        return cp;
    }
    // The inner control points' offsets from the chord, at their thirds.
    let offset = |cp: &[Point; 4]| {
        let chord = |f: f32| cp[0].y.0 + f * (cp[3].y.0 - cp[0].y.0);
        (cp[1].y.0 - chord(1.0 / 3.0) + cp[2].y.0 - chord(2.0 / 3.0)) / 2.0
    };
    let initial = offset(&cp);
    if initial.abs() < 1e-4 {
        return cp;
    }
    let side = initial.signum();
    // The arc's height off its chord at its middle is three quarters of the
    // control points' offset.
    let max_arc = if tie {
        MAX_TIE_ARC
    } else {
        (span / 3.0).min(MAX_ARC)
    };
    let max_offset = (initial.abs() * 0.75).max(max_arc) / 0.75;
    let original = cp;
    let near: Vec<&InkRect> = accidentals
        .iter()
        .filter(|b| b.right > x0 && b.left < x3)
        .collect();
    for _ in 0..MAX_PASSES {
        // The worst meeting: how far the curve must move off its chord, and
        // where along it.
        let mut worst: Option<(f32, f32)> = None;
        for b in &near {
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
            return cp;
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
            // A tie keeps its ends at its heads and its arc within bounds: one
            // that cannot pass every accidental so is left as it was.
            return original;
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
    // Still meeting one after every pass: a slur keeps what it has cleared, a
    // tie its own arc.
    if tie {
        original
    } else {
        cp
    }
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
