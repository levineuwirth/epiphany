//! The vertical solve's outlines (X5d): how far below the staves above it a
//! staff of a system stands.
//!
//! A staff is set against the merged outline of every staff above it, column
//! by column, as LilyPond places one (`align-interface.cc`, each staff
//! against the skyline of all those above), not against its neighbour's
//! whole extent: two staves need stand only so far apart that no column's
//! ink comes within a clearance of the other's, and never nearer than a
//! floor between their facing lines.
//!
//! An outline keeps each rectangle at its own edges, as MuseScore keeps a
//! skyline's rectangles (`Shape`) and LilyPond its buildings exactly; inks
//! meet across where their ranges come within [`INK_REACH`], as MuseScore's
//! `intersects` tests them (`shape.h`, `skylineMinHorizontalClearance`), and
//! keep [`INK_CLEARANCE`] apart up and down (`minVerticalDistance`). The
//! need between an outline and a staff's ink is found by a sweep over sorted
//! edges, not MuseScore's pairwise product, so a system of many staves stays
//! near linear in its ink. Every value here is a pure function of its input
//! set: ties among equal edges cannot change an envelope's value.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::clearance::InkRect;

/// The least vertical clearance between inks across a gap, in staff spaces:
/// MuseScore's `minVerticalDistance` (`styledef.cpp`). Gould gives none, so
/// MuseScore's stands (D99); LilyPond keeps 1.0 (`padding`), a departure
/// named in engrave's `DECISIONS.md`.
pub(crate) const INK_CLEARANCE: f32 = 0.5;

/// How near across two inks must come to meet, in staff spaces: MuseScore's
/// `skylineMinHorizontalClearance`. LilyPond pads nothing across between
/// staves.
pub(crate) const INK_REACH: f32 = 0.25;

/// How much further than [`INK_CLEARANCE`] a lyric keeps from the next
/// staff's ink, on the side it stands: MuseScore pads a verse's syllables and
/// lines by `lyricsMinBottomDistance` 1.5 less `minVerticalDistance`
/// (`LyricsLayout::addToSkyline`), and LilyPond keeps a lyric line 1.5 from an
/// unrelated staff (`nonstaff-unrelatedstaff-spacing`'s padding).
pub(crate) const LYRIC_PADDING: f32 = 1.0;

/// The floor between two staves of a system, line to line, in staff spaces:
/// a stave's height, as Gould asks staves stand at least (*Behind Bars*,
/// p. 488), LilyPond's least between ungrouped staves too (D109 Q1).
pub(crate) const STAFF_FLOOR: f32 = 4.0;

/// The hardest the floor between staves compresses, and only in a system
/// that would otherwise pass its page's margins: MuseScore's
/// `minStaffSpread`, its one floor with its vertical spread on (D109 Q1).
pub(crate) const STAFF_FLOOR_MIN: f32 = 3.5;

/// How many halvings the search for a compressed floor takes.
const FLOOR_STEPS: usize = 12;

/// A lower envelope across x: on segment `i`, from `xs[i]` to `xs[i + 1]`,
/// the lowest bottom of the rectangles widened to cover it, infinite where
/// none does.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Outline {
    xs: Vec<f32>,
    values: Vec<f32>,
}

/// An `f32` ordered totally, for the sweep's heap.
#[derive(Copy, Clone, PartialEq)]
struct Key(f32);

impl Eq for Key {}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Whether a rectangle holds ink an outline reads: finite, and of positive
/// width and height, as MuseScore reads neither a zero-width range
/// (`intersects`) nor a zero-height rectangle (`minVerticalDistance`).
fn holds_ink(r: &InkRect) -> bool {
    r.left.is_finite()
        && r.right.is_finite()
        && r.bottom.is_finite()
        && r.top.is_finite()
        && r.right > r.left
        && r.top > r.bottom
}

impl Outline {
    /// The lowest bottom of `rects` across x, each rectangle widened by
    /// `reach` on either side.
    pub(crate) fn below(rects: &[InkRect], reach: f32) -> Outline {
        // Each interval's bottom, negated so the heap's greatest is the
        // envelope's lowest.
        let mut spans: Vec<(f32, f32, f32)> = rects
            .iter()
            .filter(|r| holds_ink(r))
            .map(|r| (r.left - reach, r.right + reach, r.bottom))
            .collect();
        if spans.is_empty() {
            return Outline::default();
        }
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut xs: Vec<f32> = spans.iter().flat_map(|s| [s.0, s.1]).collect();
        xs.sort_by(f32::total_cmp);
        xs.dedup();
        let mut values = Vec::with_capacity(xs.len().saturating_sub(1));
        let mut heap: BinaryHeap<(Key, Key)> = BinaryHeap::new();
        let mut next = 0;
        for w in xs.windows(2) {
            let x0 = w[0];
            while next < spans.len() && spans[next].0 <= x0 {
                let (_, end, v) = spans[next];
                heap.push((Key(-v), Key(end)));
                next += 1;
            }
            while heap.peek().is_some_and(|(_, end)| end.0 <= x0) {
                heap.pop();
            }
            values.push(match heap.peek() {
                Some((v, _)) => -v.0,
                None => f32::INFINITY,
            });
        }
        Outline { xs, values }.merged_runs()
    }

    /// The same outline with each run of equal values joined.
    fn merged_runs(self) -> Outline {
        let mut xs = Vec::with_capacity(self.xs.len());
        let mut values: Vec<f32> = Vec::with_capacity(self.values.len());
        for (i, &v) in self.values.iter().enumerate() {
            if values.last() == Some(&v) {
                continue;
            }
            xs.push(self.xs[i]);
            values.push(v);
        }
        if let Some(&end) = self.xs.last() {
            xs.push(end);
        }
        Outline { xs, values }
    }

    /// The outline moved up by `dy` (down where negative).
    pub(crate) fn raised(&self, dy: f32) -> Outline {
        Outline {
            xs: self.xs.clone(),
            values: self.values.iter().map(|v| v + dy).collect(),
        }
    }

    /// The lower envelope of this outline and `other`: at each x, the lower.
    pub(crate) fn lower_with(&self, other: &Outline) -> Outline {
        if self.values.is_empty() {
            return other.clone();
        }
        if other.values.is_empty() {
            return self.clone();
        }
        let mut xs: Vec<f32> = self.xs.iter().chain(&other.xs).copied().collect();
        xs.sort_by(f32::total_cmp);
        xs.dedup();
        let values = xs
            .windows(2)
            .map(|w| self.at(w[0]).min(other.at(w[0])))
            .collect();
        Outline { xs, values }.merged_runs()
    }

    /// The value on the segment starting at or before `x`, infinite outside.
    fn at(&self, x: f32) -> f32 {
        let i = self.xs.partition_point(|&e| e <= x);
        if i == 0 || i >= self.xs.len() {
            return f32::INFINITY;
        }
        self.values[i - 1]
    }

    /// The lowest value on any segment the open range `(left, right)` meets,
    /// infinite where it meets none.
    pub(crate) fn lowest_over(&self, left: f32, right: f32) -> f32 {
        if self.values.is_empty() {
            return f32::INFINITY;
        }
        // The first segment ending past `left`.
        let mut i = self.xs[1..].partition_point(|&end| end <= left);
        let mut lowest = f32::INFINITY;
        while i < self.values.len() && self.xs[i] < right {
            lowest = lowest.min(self.values[i]);
            i += 1;
        }
        lowest
    }
}

/// How far down a staff's ink must move, at least, to keep [`INK_CLEARANCE`]
/// below `above`, the outline of the ink over it: the most any of `rects`
/// reaches past it, or minus infinity where none meets it.
pub(crate) fn need_below(above: &Outline, rects: &[InkRect]) -> f32 {
    rects
        .iter()
        .filter(|r| holds_ink(r))
        .filter_map(|r| {
            let lowest = above.lowest_over(r.left, r.right);
            lowest.is_finite().then_some(r.top + INK_CLEARANCE - lowest)
        })
        .fold(f32::NEG_INFINITY, f32::max)
}

/// One staff of a system as the stacking reads it, in the frame the
/// constrained stage stacks it in (y up), across where its system sets it.
pub(crate) struct StaffInk {
    /// Its outer lines: the top line's centre and the bottom line's.
    pub top: f32,
    pub bottom: f32,
    /// Its ink as it faces a staff below, a lyric under it reaching further
    /// down by [`LYRIC_PADDING`].
    pub down: Vec<InkRect>,
    /// Its ink as it faces a staff above, a lyric over it reaching further up.
    pub up: Vec<InkRect>,
}

/// A beam joining two staves of a system: the lower staff (by index) sinks,
/// relative to the upper, at least `least` (negative where it may rise) so
/// that every stem reaching the beam keeps its least length.
pub(crate) struct BeamBound {
    pub upper: usize,
    pub lower: usize,
    pub least: f32,
}

/// Each staff's downward shift from where the constrained stage stood it,
/// top first, the first staff anchoring the system at 0. A staff stands as
/// high as it can while its top line keeps `floor` below the bottom line of
/// the staff above it, its ink [`INK_CLEARANCE`] below the merged outline of
/// every staff above it where they meet across, and every beam joining it to
/// a staff above keeps its bound. The shift is two-sided: a slack pair closes
/// as a crowded one opens.
pub(crate) fn stack(staves: &[StaffInk], floor: f32, beams: &[BeamBound]) -> Vec<f32> {
    let mut shifts: Vec<f32> = Vec::with_capacity(staves.len());
    let mut merged = Outline::default();
    for (k, staff) in staves.iter().enumerate() {
        let shift = if k == 0 {
            0.0
        } else {
            let upper = &staves[k - 1];
            // The floor, between this staff's top line and the bottom line
            // of the staff above where it now stands.
            let by_floor = floor + staff.top - upper.bottom + shifts[k - 1];
            let by_ink = need_below(&merged, &staff.up);
            let by_beam = beams
                .iter()
                .filter(|b| b.lower == k && b.upper < k)
                .map(|b| shifts[b.upper] + b.least)
                .fold(f32::NEG_INFINITY, f32::max);
            by_floor.max(by_ink).max(by_beam)
        };
        shifts.push(shift);
        merged = merged.lower_with(&Outline::below(&staff.down, INK_REACH).raised(-shift));
    }
    shifts
}

/// The floor a system takes, and its staves' shifts there: [`STAFF_FLOOR`]
/// where the system's height there `fits`, else the largest floor toward
/// [`STAFF_FLOOR_MIN`] at which it does, found by halving, or that least
/// floor where none does (D109 Q1).
pub(crate) fn stack_fitting(
    staves: &[StaffInk],
    beams: &[BeamBound],
    fits: impl Fn(&[f32]) -> bool,
) -> (f32, Vec<f32>) {
    let full = stack(staves, STAFF_FLOOR, beams);
    if fits(&full) {
        return (STAFF_FLOOR, full);
    }
    let least = stack(staves, STAFF_FLOOR_MIN, beams);
    if !fits(&least) {
        return (STAFF_FLOOR_MIN, least);
    }
    let (mut lo, mut hi) = (STAFF_FLOOR_MIN, STAFF_FLOOR);
    let mut best = (lo, least);
    for _ in 0..FLOOR_STEPS {
        let mid = 0.5 * (lo + hi);
        let shifts = stack(staves, mid, beams);
        if fits(&shifts) {
            lo = mid;
            best = (mid, shifts);
        } else {
            hi = mid;
        }
    }
    best
}

/// The least distance between two systems, from the bottom line of the one
/// above to the top line of the one below: MuseScore's `minSystemSpread`,
/// its floor with its vertical spread on (D109 Q3, X5d.3).
pub(crate) const SYSTEM_FLOOR: f32 = 8.5;

/// How far below its page's top margin a page's first staff's top line
/// stands at least, and how far above its bottom margin the last staff's
/// bottom line aims: MuseScore's `staffUpperBorder` and `staffLowerBorder`
/// (D109 Q4).
pub(crate) const PAGE_BORDER: f32 = 7.0;

/// The spread's weights: a gap between systems, a gap at a bracket's edge
/// and at a brace's (MuseScore's `spreadSystem`, `spreadSquareBracket`,
/// `spreadCurlyBracket`).
pub(crate) const SPREAD_SYSTEM: f32 = 2.5;
pub(crate) const SPREAD_BRACKET: f32 = 1.2;
pub(crate) const SPREAD_BRACE: f32 = 1.1;

/// The spread's caps, each a gap's whole line distance: between staves,
/// inside a brace and between systems (MuseScore's `maxStaffSpread`,
/// `maxAkkoladeDistance` and `maxSystemSpread`), and the most the spread's
/// second pass adds to a gap (`maxPageFillSpread`).
pub(crate) const MAX_STAFF_SPREAD: f32 = 20.0;
pub(crate) const MAX_BRACE_SPREAD: f32 = 6.5;
pub(crate) const MAX_SYSTEM_SPREAD: f32 = 32.0;
pub(crate) const MAX_PAGE_FILL: f32 = 6.0;

/// One gap of a page as the spread reads it.
#[derive(Copy, Clone, Debug)]
pub(crate) struct SpreadGap {
    /// Its line distance before the spread.
    pub distance: f32,
    /// Its weight, 1.0 for a plain gap between staves.
    pub factor: f32,
    /// The most its line distance may come to, in the spread's units, as
    /// MuseScore keeps it (`m_maxActualSpacing`): between systems and inside a
    /// brace the cap divided by the weight, so its stated figure on the page;
    /// any other gap between staves the staff cap undivided, so a bracket's
    /// edge stops at 24 on the page, 20 in units of its 1.2.
    pub cap: f32,
    /// A gap the spread leaves: the page's top border.
    pub fixed: bool,
}

/// Raises each value from `base` toward its cap, never past `limit`, to one
/// common level, at a cost of its weight per unit, until `space` is spent or
/// every value stands at the level `limit` or its cap: the level found
/// exactly over the piecewise linear cost. Returns the new values and the
/// space spent.
fn level(base: &[f32], caps: &[f32], weights: &[f32], limit: f32, space: f32) -> (Vec<f32>, f32) {
    let cost = |l: f32| -> f32 {
        base.iter()
            .zip(caps)
            .zip(weights)
            .map(|((&b, &c), &w)| w * (l.min(c).max(b) - b))
            .sum()
    };
    let raise = |l: f32| -> Vec<f32> {
        base.iter()
            .zip(caps)
            .map(|(&b, &c)| l.min(c).max(b))
            .collect()
    };
    if space <= 0.0 || base.is_empty() {
        return (base.to_vec(), 0.0);
    }
    let full = cost(limit);
    if full <= space {
        return (raise(limit), full);
    }
    // The cost's breakpoints below `limit`, ascending; between two, it is
    // linear in the level.
    let mut points: Vec<f32> = base
        .iter()
        .chain(caps)
        .copied()
        .filter(|&p| p < limit)
        .collect();
    points.push(limit);
    points.sort_by(f32::total_cmp);
    points.dedup();
    let mut lo = points[0];
    for &hi in &points[1..] {
        let (c_lo, c_hi) = (cost(lo), cost(hi));
        if c_hi >= space {
            let l = if c_hi > c_lo {
                lo + (space - c_lo) / (c_hi - c_lo) * (hi - lo)
            } else {
                hi
            };
            return (raise(l), space);
        }
        lo = hi;
    }
    (raise(limit), full)
}

/// The line distance each gap of a page takes once the page is spread by
/// `space`, MuseScore's spread (`PageLayout::distributeStaves`) done exactly:
/// in its units (a gap's distance over its weight), the smallest gaps raised
/// level first, as far as the widest gap of the page that the spread may move,
/// a gap its ink holds past its cap included (`VerticalGapData`'s `smallest`
/// passes over only a gap at its cap exactly), each no further than its cap;
/// then what space is left, no more than [`MAX_PAGE_FILL`] for each gap of
/// the page, shared by weight, each gap taking at most [`MAX_PAGE_FILL`] in
/// those units, and none past its cap.
///
/// The levelling is a departure from MuseScore's, which raises by steps, undoes
/// the step that would overspend and hands what is left to the fill, stops
/// after twenty passes and treats spacings of one integer ceiling as level.
/// Those leave gaps uneven, sometimes by whole spaces on a page, where this
/// leaves them level; neither Gould nor LilyPond asks for it (D123 Q3).
pub(crate) fn spread(gaps: &[SpreadGap], space: f32) -> Vec<f32> {
    let open: Vec<usize> = (0..gaps.len())
        .filter(|&i| !gaps[i].fixed && gaps[i].distance / gaps[i].factor < gaps[i].cap)
        .collect();
    let mut units: Vec<f32> = gaps.iter().map(|g| g.distance / g.factor).collect();
    if space <= 0.0 || open.is_empty() {
        return gaps.iter().map(|g| g.distance).collect();
    }
    let base: Vec<f32> = open.iter().map(|&i| units[i]).collect();
    let caps: Vec<f32> = open.iter().map(|&i| gaps[i].cap).collect();
    let weights: Vec<f32> = open.iter().map(|&i| gaps[i].factor).collect();
    let widest = (0..gaps.len())
        .filter(|&i| !gaps[i].fixed)
        .map(|i| units[i])
        .fold(f32::NEG_INFINITY, f32::max);
    let (levelled, spent) = level(&base, &caps, &weights, widest, space);
    // The fill: at most MAX_PAGE_FILL a gap, for every gap of the page.
    let left = (space - spent).min(MAX_PAGE_FILL * gaps.len() as f32);
    let room: Vec<f32> = levelled
        .iter()
        .zip(&caps)
        .map(|(&v, &c)| (c - v).max(0.0))
        .collect();
    let zero = vec![0.0; room.len()];
    let (fill, _) = level(&zero, &room, &weights, MAX_PAGE_FILL, left);
    for (k, &i) in open.iter().enumerate() {
        units[i] = levelled[k] + fill[k];
    }
    units
        .iter()
        .zip(gaps)
        .map(|(&u, g)| if g.fixed { g.distance } else { u * g.factor })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f32, right: f32, bottom: f32, top: f32) -> InkRect {
        InkRect {
            left,
            right,
            bottom,
            top,
        }
    }

    /// A five-line staff's lines from `top` down, across `0..width`.
    fn lines(top: f32, width: f32) -> Vec<InkRect> {
        (0..5)
            .map(|i| {
                let y = top - i as f32;
                rect(0.0, width, y - 0.065, y + 0.065)
            })
            .collect()
    }

    fn staff(top: f32, extra: Vec<InkRect>) -> StaffInk {
        let mut ink = lines(top, 40.0);
        ink.extend(extra);
        StaffInk {
            top,
            bottom: top - 4.0,
            down: ink.clone(),
            up: ink,
        }
    }

    fn gap(distance: f32, factor: f32, cap: f32) -> SpreadGap {
        SpreadGap {
            distance,
            factor,
            cap: cap / factor,
            fixed: false,
        }
    }

    /// A gap its ink holds past its cap stays in the levelling's reach: the
    /// others rise level with it, as `smallest` lets them.
    #[test]
    fn gaps_rise_level_with_a_gap_past_its_cap() {
        let gaps = [gap(4.0, 1.0, 20.0), gap(12.0, 1.0, 6.5)];
        // The first takes all 8, to 12, level with the second, which its cap
        // leaves where it stands; read by the open gaps alone the first would
        // stop at once, and take only the 6 the fill allows.
        let d = spread(&gaps, 8.0);
        assert!((d[0] - 12.0).abs() < 1e-5, "{d:?}");
        assert_eq!(d[1], 12.0);
    }

    /// The page's top border moves nothing: it is not a gap to rise level with.
    #[test]
    fn a_fixed_gap_sets_no_level() {
        let fixed = SpreadGap {
            distance: 30.0,
            factor: 1.0,
            cap: 32.0,
            fixed: true,
        };
        let gaps = [fixed, gap(4.0, 1.0, 20.0), gap(6.0, 1.0, 20.0)];
        // 2 levels the two free gaps at 6; a further 6 is the fill.
        let d = spread(&gaps, 2.0);
        assert!((d[1] - 6.0).abs() < 1e-5 && d[2] == 6.0, "{d:?}");
        assert_eq!(d[0], 30.0);
    }

    #[test]
    fn the_spread_raises_the_smallest_gaps_level_first() {
        let gaps = [
            gap(4.0, 1.0, 20.0),
            gap(6.0, 1.0, 20.0),
            gap(5.0, 1.0, 20.0),
        ];
        // 1.5 brings 4 and 5 level at 5.25, short of 6.
        let d = spread(&gaps, 1.5);
        assert!(
            (d[0] - 5.25).abs() < 1e-5 && (d[2] - 5.25).abs() < 1e-5,
            "{d:?}"
        );
        assert_eq!(d[1], 6.0);
        // 3.0 levels all three at 6, then shares the last 0.0.
        let d = spread(&gaps, 3.0);
        assert!(d.iter().all(|&v| (v - 6.0).abs() < 1e-5), "{d:?}");
        // 6.0 levels them, then fills each by a third of 3.0.
        let d = spread(&gaps, 6.0);
        assert!(d.iter().all(|&v| (v - 7.0).abs() < 1e-5), "{d:?}");
    }

    #[test]
    fn the_spread_weighs_a_gap_by_its_kind() {
        // A gap between systems weighs 2.5: in the spread's units it stands
        // at its distance over its weight, and takes 2.5 for each unit.
        let gaps = [
            gap(10.0, SPREAD_SYSTEM, MAX_SYSTEM_SPREAD),
            gap(4.0, 1.0, 20.0),
        ];
        // Units 4 and 4: level, so 3.5 shared by weight, 1 a unit.
        let d = spread(&gaps, 3.5);
        assert!(
            (d[0] - 12.5).abs() < 1e-4 && (d[1] - 5.0).abs() < 1e-4,
            "{d:?}"
        );
    }

    #[test]
    fn the_spread_keeps_each_gap_under_its_cap_and_its_fill() {
        // Inside a brace no gap passes 6.5; the fill adds at most 6 a gap.
        let gaps = [
            gap(4.0, 1.0, MAX_BRACE_SPREAD),
            gap(4.0, 1.0, MAX_STAFF_SPREAD),
        ];
        let d = spread(&gaps, 100.0);
        assert!((d[0] - MAX_BRACE_SPREAD).abs() < 1e-5, "{d:?}");
        assert!((d[1] - (4.0 + MAX_PAGE_FILL)).abs() < 1e-5, "{d:?}");
        // A fixed gap, the page's top border, takes nothing.
        let mut top = gap(7.0, 1.0, 20.0);
        top.fixed = true;
        let d = spread(&[top, gap(4.0, 1.0, 20.0)], 3.0);
        assert_eq!(d[0], 7.0);
        assert!((d[1] - 7.0).abs() < 1e-5, "{d:?}");
    }

    #[test]
    fn the_spread_fills_a_page_no_more_than_six_a_gap_in_all() {
        // Three gaps between systems and the page's fixed top border: the
        // fill shares at most 6 for each of the four, 24, though each gap
        // could take 6 in its units, 15.
        let mut top = gap(7.0, 1.0, 20.0);
        top.fixed = true;
        let gaps = [
            top,
            gap(8.5, SPREAD_SYSTEM, MAX_SYSTEM_SPREAD),
            gap(8.5, SPREAD_SYSTEM, MAX_SYSTEM_SPREAD),
            gap(8.5, SPREAD_SYSTEM, MAX_SYSTEM_SPREAD),
        ];
        let d = spread(&gaps, 100.0);
        assert!(d[1..].iter().all(|&v| (v - 16.5).abs() < 1e-4), "{d:?}");
    }

    #[test]
    fn the_spread_spends_no_more_than_it_is_given() {
        let gaps = [
            gap(4.0, 1.2, 20.0),
            gap(9.0, 2.5, 32.0),
            gap(5.5, 1.0, 20.0),
            gap(4.0, 1.1, 6.5),
        ];
        for space in [0.0, 0.3, 2.0, 7.0, 15.0] {
            let d = spread(&gaps, space);
            let spent: f32 = d.iter().zip(&gaps).map(|(v, g)| v - g.distance).sum();
            assert!(spent <= space + 1e-4, "{space}: {spent}");
            assert!(d.iter().zip(&gaps).all(|(v, g)| *v >= g.distance - 1e-6));
        }
    }

    /// The gap bands the quality measure reads carry the solve's numbers:
    /// the floors, the caps and the weight between systems cannot drift.
    #[test]
    fn the_gap_bands_carry_the_solves_numbers() {
        use epiphany_layout_ir::{VerticalBand, VerticalBandId};
        let staff = VerticalBand::inter_staff_gap(VerticalBandId(0));
        assert_eq!(staff.min_height.0, STAFF_FLOOR);
        assert_eq!(staff.preferred_height.0, STAFF_FLOOR);
        assert_eq!(staff.max_height.map(|h| h.0), Some(MAX_STAFF_SPREAD));
        assert_eq!(staff.stretch_factor, 1.0);
        let system = VerticalBand::inter_system_gap(VerticalBandId(0));
        assert_eq!(system.min_height.0, SYSTEM_FLOOR);
        assert_eq!(system.preferred_height.0, SYSTEM_FLOOR);
        assert_eq!(system.max_height.map(|h| h.0), Some(MAX_SYSTEM_SPREAD));
        assert_eq!(system.stretch_factor, SPREAD_SYSTEM);
    }

    #[test]
    fn an_outline_keeps_each_rectangle_at_its_own_edges() {
        let o = Outline::below(
            &[rect(0.0, 2.0, -1.0, 0.0), rect(1.0, 3.0, -3.0, -2.0)],
            0.0,
        );
        assert_eq!(o.lowest_over(-1.0, 0.5), -1.0);
        assert_eq!(o.lowest_over(1.5, 1.6), -3.0);
        assert_eq!(o.lowest_over(2.5, 2.6), -3.0);
        // Open ranges: one ending where another starts does not meet it.
        assert_eq!(o.lowest_over(3.0, 4.0), f32::INFINITY);
        assert_eq!(o.lowest_over(-1.0, 1.0), -1.0);
    }

    #[test]
    fn inks_meet_across_within_the_reach() {
        let upper = Outline::below(&[rect(0.0, 1.0, -2.0, 0.0)], INK_REACH);
        // 0.2 across from the upper ink: they meet.
        assert!(need_below(&upper, &[rect(1.2, 2.0, -3.0, -1.0)]).is_finite());
        // 0.3 across: they do not.
        assert_eq!(
            need_below(&upper, &[rect(1.3, 2.0, -3.0, -1.0)]),
            f32::NEG_INFINITY
        );
        // A rectangle of no width or height holds no ink.
        assert_eq!(
            need_below(&upper, &[rect(0.5, 0.5, -3.0, -1.0)]),
            f32::NEG_INFINITY
        );
    }

    #[test]
    fn the_need_keeps_the_clearance_between_meeting_inks() {
        let upper = Outline::below(&[rect(0.0, 1.0, -2.0, 0.0)], 0.0);
        // The lower ink's top at -1.0 must sink to -2.5: 1.5.
        let need = need_below(&upper, &[rect(0.5, 2.0, -3.0, -1.0)]);
        assert!((need - 1.5).abs() < 1e-6, "{need}");
    }

    #[test]
    fn envelopes_combine_pointwise() {
        let a = Outline::below(&[rect(0.0, 2.0, -1.0, 0.0)], 0.0);
        let b = Outline::below(&[rect(1.0, 3.0, -2.0, 0.0)], 0.0);
        let low = a.lower_with(&b);
        assert_eq!(low.lowest_over(0.0, 0.5), -1.0);
        assert_eq!(low.lowest_over(1.5, 1.6), -2.0);
        assert_eq!(low.lowest_over(2.5, 2.6), -2.0);
        assert_eq!(a.raised(-1.0).lowest_over(0.0, 1.0), -2.0);
    }

    #[test]
    fn empty_staves_stand_the_floor_apart() {
        // Constrained frame: 12 apart, so 8 line to line; the floor closes it.
        let staves = [staff(4.0, vec![]), staff(-8.0, vec![])];
        let shifts = stack(&staves, STAFF_FLOOR, &[]);
        let distance = (staves[0].bottom) - (staves[1].top - shifts[1]);
        assert!((distance - STAFF_FLOOR).abs() < 1e-5, "{distance}");
    }

    #[test]
    fn ink_in_different_columns_does_not_add_up() {
        // A note three spaces under the upper staff at x 2, one three spaces
        // over the lower at x 20: a whole-extent gap would add both; the
        // outlines do not, and the floor holds.
        let low = rect(2.0, 3.0, -3.0, -1.0);
        let high = rect(20.0, 21.0, -7.0, -5.0);
        let staves = [staff(4.0, vec![low]), staff(-8.0, vec![high])];
        let shifts = stack(&staves, STAFF_FLOOR, &[]);
        let distance = staves[0].bottom - (staves[1].top - shifts[1]);
        assert!((distance - STAFF_FLOOR).abs() < 1e-5, "{distance}");
        // In one column they do.
        let high = rect(2.0, 3.0, -7.0, -5.0);
        let staves = [staff(4.0, vec![low]), staff(-8.0, vec![high])];
        let shifts = stack(&staves, STAFF_FLOOR, &[]);
        let gap = low.bottom - (high.top - shifts[1]);
        assert!((gap - INK_CLEARANCE).abs() < 1e-5, "{gap}");
    }

    #[test]
    fn a_staff_clears_every_staff_above_it() {
        // The top staff's ink past the end of the lines (x 45) reaches below
        // the middle staff, which has no ink there; the bottom staff's ink
        // there must clear it, though the staff next above it is clear.
        let reach = rect(45.0, 46.0, -20.0, 0.0);
        let staves = [
            staff(4.0, vec![reach]),
            staff(-8.0, vec![]),
            staff(-20.0, vec![rect(45.0, 46.0, -22.0, -14.0)]),
        ];
        let shifts = stack(&staves, STAFF_FLOOR, &[]);
        let gap = reach.bottom - (-14.0 - shifts[2]);
        assert!(
            (gap - INK_CLEARANCE).abs() < 1e-5,
            "the bottom staff clears the top's ink, no lower: {gap}"
        );
    }

    #[test]
    fn a_beam_across_two_staves_bounds_their_distance() {
        let staves = [staff(4.0, vec![]), staff(-8.0, vec![])];
        // The lower staff may rise at most 1.0 from the constrained frame.
        let beams = [BeamBound {
            upper: 0,
            lower: 1,
            least: -1.0,
        }];
        let shifts = stack(&staves, STAFF_FLOOR, &beams);
        assert!((shifts[1] + 1.0).abs() < 1e-5, "{shifts:?}");
    }

    #[test]
    fn a_floor_compresses_only_where_the_system_would_not_fit() {
        let staves = [
            staff(4.0, vec![]),
            staff(-8.0, vec![]),
            staff(-20.0, vec![]),
        ];
        let height = |shifts: &[f32]| 4.0 - (staves[2].bottom - shifts[2]);
        // Room for it: the full floor.
        let (floor, _) = stack_fitting(&staves, &[], |s| height(s) <= 100.0);
        assert_eq!(floor, STAFF_FLOOR);
        // Three staves at 4.0 stand 20 tall; at 3.5, 19. Room for 19.4.
        let (floor, shifts) = stack_fitting(&staves, &[], |s| height(s) <= 19.4);
        assert!(floor > STAFF_FLOOR_MIN && floor < STAFF_FLOOR, "{floor}");
        assert!(height(&shifts) <= 19.4 + 1e-4, "{}", height(&shifts));
        assert!(
            height(&shifts) > 19.39,
            "as little compressed as fits: {}",
            height(&shifts)
        );
        // No room even at the least floor: the least floor.
        let (floor, _) = stack_fitting(&staves, &[], |s| height(s) <= 10.0);
        assert_eq!(floor, STAFF_FLOOR_MIN);
    }
}

/// The solve read back from the baked output, through the whole pipeline: no
/// code of the solve's own measures it, the gaps found pairwise.
#[cfg(test)]
mod solve_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use epiphany_core::{StaffId, TypedObjectId};
    use epiphany_layout_ir::{
        to_constrained, to_logical, ConstrainedLayoutIR, GlyphObjectId, PlaceOrder, VerticalBandId,
        VerticalBandKind,
    };

    use super::{INK_CLEARANCE, INK_REACH, LYRIC_PADDING, STAFF_FLOOR};
    use crate::casting::{cast_off, CastLayout, PageGeometry, Respacing};
    use crate::clearance::InkRect;
    use crate::{Engraver, HorizontalRemap};

    pub(crate) fn cast(input: &ConstrainedLayoutIR, geometry: &PageGeometry) -> CastLayout {
        let remap = HorizontalRemap::build(input);
        cast_off(
            input,
            &remap.glyphs(input),
            &remap.strokes(input),
            &remap.curves(input),
            &remap.texts(input),
            &Respacing {
                skyline: &remap.skyline,
                curves: &remap.unpassed_curves(input),
            },
            geometry,
        )
    }

    /// One pair of adjacent staves of a system as drawn: their line distance,
    /// and the least clearance between any two inks of the staves above and
    /// of the lower staff that meet across, a lyric's padding taken off.
    #[derive(Debug)]
    pub(crate) struct Gap {
        pub distance: f32,
        pub clearance: f32,
    }

    /// Each system's staves' ink as baked, each rectangle with its lyric
    /// padding below and above; each staff's lines; each system's page.
    type Ink = Vec<(InkRect, f32, f32)>;
    struct Baked {
        ink: BTreeMap<(usize, StaffId), Ink>,
        lines: BTreeMap<(usize, StaffId), (f32, f32)>,
        page: Vec<usize>,
    }

    fn baked(input: &ConstrainedLayoutIR, cast: &CastLayout) -> Baked {
        let staff_of: BTreeMap<VerticalBandId, StaffId> = input
            .vertical_bands
            .iter()
            .filter_map(|b| match b.kind {
                VerticalBandKind::Staff(s) => Some((b.id, s)),
                _ => None,
            })
            .collect();
        let between: BTreeSet<GlyphObjectId> = input
            .cross_staff_beams
            .iter()
            .flat_map(|b| b.ink.iter().copied())
            .collect();
        // A lyric's ink by id, and whether it stands above its staff.
        let mut lyric: BTreeMap<GlyphObjectId, bool> = BTreeMap::new();
        for mark in &input.family {
            if let Some(p) = mark.placement.filter(|p| p.order == PlaceOrder::Lyric) {
                for id in mark.glyphs.iter().chain(&mark.strokes).chain(&mark.texts) {
                    lyric.insert(*id, p.above);
                }
            }
        }
        let mut ink: BTreeMap<(usize, StaffId), Ink> = BTreeMap::new();
        let mut put = |s: usize, band: VerticalBandId, id: GlyphObjectId, r: InkRect| {
            if between.contains(&id) {
                return;
            }
            if let Some(st) = staff_of.get(&band) {
                let (below, above) = match lyric.get(&id) {
                    Some(true) => (0.0, LYRIC_PADDING),
                    Some(false) => (LYRIC_PADDING, 0.0),
                    None => (0.0, 0.0),
                };
                ink.entry((s, *st)).or_default().push((r, below, above));
            }
        };
        let rect = |l: f32, r: f32, b: f32, t: f32| InkRect {
            left: l.min(r),
            right: l.max(r),
            bottom: b.min(t),
            top: b.max(t),
        };
        for (i, g) in cast.glyphs.iter().enumerate() {
            let Some(s) = cast.glyph_system[i] else {
                continue;
            };
            let band = match input.glyphs.get(i) {
                Some(source) => source.vertical_band,
                None => cast.appended_bands[i - input.glyphs.len()],
            };
            let (x, y, b) = (g.position.x.0, g.position.y.0, g.bounding_box);
            let id = input
                .glyphs
                .get(i)
                .map_or(GlyphObjectId(u128::MAX), |s| s.id());
            put(
                s,
                band,
                id,
                rect(x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0),
            );
        }
        let mut lines: BTreeMap<(usize, StaffId), (f32, f32)> = BTreeMap::new();
        for (i, st) in cast.strokes.iter().enumerate() {
            let Some(s) = cast.stroke_system[i] else {
                continue;
            };
            let h = st.thickness.0 * 0.5;
            put(
                s,
                st.vertical_band,
                st.id(),
                rect(
                    st.from.x.0.min(st.to.x.0) - h,
                    st.from.x.0.max(st.to.x.0) + h,
                    st.from.y.0.min(st.to.y.0) - h,
                    st.from.y.0.max(st.to.y.0) + h,
                ),
            );
            if let TypedObjectId::Staff(staff) = st.provenance.source {
                if st.from.y == st.to.y {
                    let y = st.from.y.0;
                    let e = lines.entry((s, staff)).or_insert((y, y));
                    e.0 = e.0.min(y);
                    e.1 = e.1.max(y);
                }
            }
        }
        for (i, c) in cast.curves.iter().enumerate() {
            let Some(s) = cast.curve_system[i] else {
                continue;
            };
            let cp = c.control_points();
            let h = c.thickness.0 * 0.5;
            let at = |t: f32| {
                let u = 1.0 - t;
                let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                (
                    (0..4).map(|k| w[k] * cp[k].x.0).sum::<f32>(),
                    (0..4).map(|k| w[k] * cp[k].y.0).sum::<f32>(),
                )
            };
            let mut last = at(0.0);
            for k in 1..=12 {
                let next = at(k as f32 / 12.0);
                put(
                    s,
                    c.vertical_band,
                    c.id(),
                    rect(
                        last.0 - h,
                        next.0 + h,
                        last.1.min(next.1) - h,
                        last.1.max(next.1) + h,
                    ),
                );
                last = next;
            }
        }
        let systems: Vec<_> = cast.pages.iter().flat_map(|p| &p.systems).collect();
        for (s, system) in systems.iter().enumerate() {
            for &i in &system.primitives.texts {
                let t = &cast.texts[i as usize];
                let band = match input.texts.get(i as usize) {
                    Some(source) => source.vertical_band,
                    None => cast.appended_text_bands[i as usize - input.texts.len()],
                };
                let (x, y, b) = (t.origin.x.0, t.origin.y.0, t.bounds);
                put(
                    s,
                    band,
                    t.id(),
                    rect(x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0),
                );
            }
        }
        let page = cast
            .pages
            .iter()
            .enumerate()
            .flat_map(|(p, page)| page.systems.iter().map(move |_| p))
            .collect();
        Baked { ink, lines, page }
    }

    fn meets(a: &InkRect, b: &InkRect) -> bool {
        a.right > a.left
            && b.right > b.left
            && a.top > a.bottom
            && b.top > b.bottom
            && a.right + INK_REACH > b.left
            && a.left < b.right + INK_REACH
    }

    /// Each system's staves top first, with their lines.
    fn staves_of(baked: &Baked, s: usize) -> Vec<(StaffId, (f32, f32))> {
        let mut staves: Vec<(StaffId, (f32, f32))> = baked
            .lines
            .iter()
            .filter(|((sys, _), _)| *sys == s)
            .map(|((_, st), l)| (*st, *l))
            .collect();
        staves.sort_by(|a, b| b.1 .1.total_cmp(&a.1 .1));
        staves
    }

    /// The least clearance between any ink of `upper` and any of `lower`
    /// that meet across, each lyric's padding taken off.
    fn clearance(upper: &[&Ink], lower: &[&Ink]) -> f32 {
        let mut least = f32::INFINITY;
        for (a, below, _) in upper.iter().copied().flatten() {
            for (b, _, above) in lower.iter().copied().flatten() {
                if meets(a, b) {
                    least = least.min((a.bottom - below) - (b.top + above));
                }
            }
        }
        least
    }

    pub(crate) fn gaps(input: &ConstrainedLayoutIR, cast: &CastLayout) -> Vec<Gap> {
        let baked = baked(input, cast);
        let none = Ink::new();
        let ink = |s: usize, st: StaffId| baked.ink.get(&(s, st)).unwrap_or(&none);
        let mut out = Vec::new();
        for s in 0..baked.page.len() {
            let staves = staves_of(&baked, s);
            for k in 1..staves.len() {
                let upper: Vec<&Ink> = staves[..k].iter().map(|u| ink(s, u.0)).collect();
                out.push(Gap {
                    distance: staves[k - 1].1 .0 - staves[k].1 .1,
                    clearance: clearance(&upper, &[ink(s, staves[k].0)]),
                });
            }
        }
        out
    }

    /// Each pair of consecutive systems on one page as drawn: the bottom
    /// line of the one above to the top line of the one below, and the least
    /// clearance between their inks.
    pub(crate) fn system_gaps(input: &ConstrainedLayoutIR, cast: &CastLayout) -> Vec<Gap> {
        let baked = baked(input, cast);
        let none = Ink::new();
        let all = |s: usize| -> Vec<&Ink> {
            staves_of(&baked, s)
                .iter()
                .map(|(st, _)| baked.ink.get(&(s, *st)).unwrap_or(&none))
                .collect()
        };
        (1..baked.page.len())
            .filter(|&s| baked.page[s] == baked.page[s - 1])
            .map(|s| {
                let (above, below) = (staves_of(&baked, s - 1), staves_of(&baked, s));
                Gap {
                    distance: above.last().expect("a staff").1 .0 - below[0].1 .1,
                    clearance: clearance(&all(s - 1), &all(s)),
                }
            })
            .collect()
    }

    /// Every gap as the rule sets it: no meeting inks nearer than the
    /// clearance, and the staves no further apart than the floor or that
    /// clearance asks.
    fn assert_set_by_the_rule(gaps: &[Gap], floor: f32) {
        assert!(!gaps.is_empty());
        for g in gaps {
            assert!(
                g.clearance >= INK_CLEARANCE - 2e-3,
                "inks nearer than the clearance: {g:?}"
            );
            assert!(
                g.distance >= floor - 2e-3,
                "staves nearer than the floor: {g:?}"
            );
            assert!(
                (g.distance - floor).abs() < 2e-3 || (g.clearance - INK_CLEARANCE).abs() < 2e-3,
                "staves further apart than the rule asks: {g:?}"
            );
        }
    }

    /// A page of no height, which nothing breaks or spreads: what the staff
    /// solve alone sets.
    fn unbounded() -> PageGeometry {
        let mut geometry = Engraver::default().geometry();
        geometry.size.height = epiphany_layout_ir::StaffSpace(0.0);
        geometry
    }

    fn solved(score: &epiphany_core::Score) -> (ConstrainedLayoutIR, CastLayout) {
        let input = to_constrained(&to_logical(score));
        let cast = cast(&input, &unbounded());
        (input, cast)
    }

    #[test]
    fn colliding_staves_stand_their_inks_clearance_apart() {
        // The top staff's C2 stands over the bottom staff's C6 in one column:
        // that column sets the distance, the inks 0.5 apart.
        let (input, cast) = solved(&epiphany_testkit::fixtures::two_staff_close_content(1));
        let gaps = gaps(&input, &cast);
        assert_set_by_the_rule(&gaps, STAFF_FLOOR);
        assert!(
            (gaps[0].clearance - INK_CLEARANCE).abs() < 2e-3 && gaps[0].distance > STAFF_FLOOR,
            "the ink sets the distance: {gaps:?}"
        );
    }

    #[test]
    fn three_staves_each_clear_what_stands_above_them() {
        let (input, cast) = solved(&epiphany_testkit::fixtures::three_staff_close_content(1));
        let gaps = gaps(&input, &cast);
        assert_eq!(gaps.len(), 2, "{gaps:?}");
        assert_set_by_the_rule(&gaps, STAFF_FLOOR);
    }

    #[test]
    fn each_system_is_set_by_its_own_ink() {
        // The wrapping fixture's first system is crowded, the next slack: the
        // first set by its ink, the next by the floor.
        let (input, cast) = solved(&epiphany_testkit::fixtures::two_staff_wrapping_pressure(1));
        let gaps = gaps(&input, &cast);
        assert!(gaps.len() >= 2, "{gaps:?}");
        assert_set_by_the_rule(&gaps, STAFF_FLOOR);
        assert!(gaps[0].distance > STAFF_FLOOR + 1.0, "{gaps:?}");
        assert!(
            gaps[1..]
                .iter()
                .any(|g| (g.distance - STAFF_FLOOR).abs() < 2e-3),
            "a slack system closes to the floor: {gaps:?}"
        );
    }

    #[test]
    fn systems_stand_their_floor_or_their_inks_clearance_apart() {
        for score in [
            epiphany_testkit::fixtures::ten_measure_single_staff(1),
            epiphany_testkit::fixtures::two_staff_wrapping_pressure(1),
            epiphany_testkit::fixtures::ten_measure_with_slurs(1),
        ] {
            let (input, cast) = solved(&score);
            let gaps = system_gaps(&input, &cast);
            assert!(!gaps.is_empty(), "the fixture wraps");
            assert_set_by_the_rule(&gaps, super::SYSTEM_FLOOR);
        }
        // MuseScore's least between systems, with its spread on.
        assert_eq!(super::SYSTEM_FLOOR, 8.5);
        let (input, cast) = solved(&epiphany_testkit::fixtures::ten_measure_single_staff(1));
        assert!(
            system_gaps(&input, &cast)
                .iter()
                .any(|g| (g.distance - 8.5).abs() < 1e-3),
            "a plain single staff's systems stand the floor apart"
        );
    }

    #[test]
    fn a_staff_owning_no_glyphs_is_set_by_the_rule_too() {
        let input = epiphany_testkit::fixtures::percussion_placeholder_constrained(1);
        let cast = cast(&input, &unbounded());
        assert_set_by_the_rule(&gaps(&input, &cast), STAFF_FLOOR);
    }
}
