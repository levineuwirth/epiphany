//! The horizontal spacing pass — the first axis of the planned two-pass spring
//! layout (`epiphany-engrave`'s DECISIONS.md, decision 1).
//!
//! A `ConstrainedLayoutIR` carries horizontal spring slots — one slot per
//! *musical time column* (`to_constrained` groups simultaneous glyphs into a
//! shared column slot, with the clef in a lead column and barlines in their own
//! columns). This pass places each glyph-bearing slot left to right and yields
//! the coordinate-map control points the caller ([`crate::HorizontalRemap`])
//! applies to glyph baselines *and* the strokes that track them.
//!
//! A slot stands at least its predecessor's `preferred_width` after it (the
//! spring's natural width: a note column's comes from the durations sounding
//! through it, the lead's and a signature's from their ink and the gap after
//! it), and far enough right that its ink clears the ink already set at the
//! same height by the gap that ink's slot asks: a skyline of the rightmost ink
//! set so far in each quarter-space band of height, so a column whose notes
//! stand on one staff can sit close after a column whose notes stand on
//! another, as their onsets ask, while a note's accidental never overlaps the
//! previous note it would meet, keeps a signature's gap after it, and a stem
//! reaching across to another staff's beam is cleared there. The casting-off
//! pass (`crate::casting`) then breaks this spaced line into systems and pages
//! and justifies each system by stretching its note columns' springs.

use std::collections::BTreeMap;

use epiphany_layout_ir::{
    is_barline_glyph, is_rigid_width_stroke, ConstrainedLayoutIR, SpringSlotId, VerticalBandKind,
};

use crate::casting::barline_lines;
use crate::owning_glyph;

/// Inter-slot gap (staff spaces) reserved between a note column's right
/// content and a later slot's left content at the same height.
const SLOT_GAP: f32 = 0.3;

/// The least a barline's ink stands clear of the ink after it, an
/// accidental's mostly: MuseScore's default distance from a barline to an
/// accidental (0.65 spaces). The note itself stands further, by the
/// barline's spring.
const BARLINE_CLEARANCE: f32 = 0.65;

/// The height of one band of the skyline, in staff spaces.
const SKYLINE_BAND: f32 = 0.25;

/// How far above and below its ink a slot's content keeps clear of earlier
/// ink: heads a third apart in neighbouring columns still clear each other
/// sideways, while a head an octave from the previous may stand under it.
const SKYLINE_MARGIN: f32 = 0.2;

/// The least advance from one slot to the next, whatever their springs and
/// ink: two onsets a hair apart on different staves still read in order.
const MIN_ADVANCE: f32 = 0.1;

/// The least a tie runs between its ends, which stand clear of their
/// columns' ink, in staff spaces.
const TIE_MIN_SPAN: f32 = 1.0;

/// The least a beam across two staves runs between the stems it joins, whose
/// up stem right of its head can stand close beside the next down stem left
/// of its own, in staff spaces.
const CROSS_BEAM_MIN_SPAN: f32 = 1.5;

/// The spacing pass's output: the interpolation control points for spanning
/// strokes, and each glyph-bearing slot's exact `(source, target)` pair — the
/// rigid delta every member glyph translates by, so intra-slot offsets (a
/// time signature after its barline, key-signature accidentals after the
/// clef, an accidental left of its notehead) survive the re-spacing verbatim.
pub(crate) struct SpacedSlots {
    /// `(source_x, target_x)` control points, sorted by source, sources
    /// distinct — the piecewise-linear map for content that genuinely *spans*
    /// columns (staff lines, brackets).
    pub points: Vec<(f32, f32)>,
    /// Each glyph-bearing slot's own `(source_x, target_x)`.
    pub by_slot: BTreeMap<SpringSlotId, (f32, f32)>,
}

/// Spaces the glyph-bearing slots left to right. Each slot's source is its
/// column reference (its first member glyph's baseline); its target is the
/// least that keeps its predecessor's spring, clears the ink set so far at
/// each height its own ink reaches (its accidentals and other left overhang
/// included) by the clearance that ink's slot asks, and gives each tie and
/// cross-staff beam ending there its least span.
/// Deterministic: a pure function of the glyphs, their bounding boxes and the
/// slots' springs.
pub(crate) fn space_slots(input: &ConstrainedLayoutIR) -> SpacedSlots {
    let anchors = crate::span_anchors(input);
    /// One slot's horizontal extent, from its member glyphs and the strokes
    /// that ride it, by height.
    struct Extent {
        /// Column reference x (the first member's baseline).
        source: f32,
        /// Each skyline band's leftmost and rightmost content edge.
        bands: BTreeMap<i32, (f32, f32)>,
        /// The spring's natural width.
        preferred: f32,
        /// Whether the spring keeps its width when a system is justified:
        /// a lead's, a signature's, a change's or a barline's.
        fixed: bool,
        /// Whether every member is a barline or a repeat sign.
        barline: bool,
    }

    let preferred_of: BTreeMap<SpringSlotId, (f32, bool)> = input
        .horizontal_slots
        .iter()
        .map(|s| (s.id, (s.preferred_width.0, s.stretch_factor == 0.0)))
        .collect();
    let mut by_slot: BTreeMap<SpringSlotId, Extent> = BTreeMap::new();
    // Widens a slot's extent by a box of ink, in every band its height (and
    // the margin about it) reaches.
    let widen = |extent: &mut Extent, (left, right): (f32, f32), (bottom, top): (f32, f32)| {
        let lo = ((bottom - SKYLINE_MARGIN) / SKYLINE_BAND).floor() as i32;
        let hi = ((top + SKYLINE_MARGIN) / SKYLINE_BAND).floor() as i32;
        for band in lo..=hi.max(lo) {
            extent
                .bands
                .entry(band)
                .and_modify(|e| {
                    e.0 = e.0.min(left);
                    e.1 = e.1.max(right);
                })
                .or_insert((left, right));
        }
    };
    for glyph in &input.glyphs {
        let x = glyph.baseline.x.0;
        let y = glyph.baseline.y.0;
        let b = glyph.bounding_box;
        let (preferred, fixed) = preferred_of
            .get(&glyph.horizontal_slot)
            .copied()
            .unwrap_or((0.0, false));
        let extent = by_slot.entry(glyph.horizontal_slot).or_insert(Extent {
            source: x,
            bands: BTreeMap::new(),
            preferred,
            fixed,
            barline: true,
        });
        let name = glyph.glyph.as_str();
        extent.barline &= name.starts_with("barline") || name.starts_with("repeat");
        widen(
            extent,
            (x + b.left.0, x + b.right.0),
            (y + b.bottom.0, y + b.top.0),
        );
    }

    // A group whose barlines join from staff to staff has each joined across
    // the gap between two of its staves by the casting stage, after spacing,
    // so the joining line is reserved here: a barline slot's extent takes,
    // in each such gap, each line of the upper staff's barline at its own
    // thickness, from the lower staff's barline to the upper's, as casting
    // draws it, and an accidental on a note above or below its staff keeps a
    // barline's clearance in the gap as on the staff.
    let staff_of_band: BTreeMap<_, _> = input
        .vertical_bands
        .iter()
        .filter_map(|band| match band.kind {
            VerticalBandKind::Staff(staff) => Some((band.id, staff)),
            _ => None,
        })
        .collect();
    for group in input
        .staff_groups
        .iter()
        .filter(|group| group.joined && group.staves.len() > 1)
    {
        // Each barline slot's barlines on the group's staves, top first.
        let mut barlines: BTreeMap<SpringSlotId, Vec<&epiphany_layout_ir::GlyphObject>> =
            BTreeMap::new();
        for glyph in &input.glyphs {
            if is_barline_glyph(glyph.glyph.as_str())
                && staff_of_band
                    .get(&glyph.vertical_band)
                    .is_some_and(|staff| group.staves.contains(staff))
            {
                barlines
                    .entry(glyph.horizontal_slot)
                    .or_default()
                    .push(glyph);
            }
        }
        for (slot, mut glyphs) in barlines {
            let Some(extent) = by_slot.get_mut(&slot) else {
                continue;
            };
            glyphs.sort_by(|a, b| b.baseline.y.0.total_cmp(&a.baseline.y.0));
            for pair in glyphs.windows(2) {
                let (upper, lower) = (pair[0], pair[1]);
                let (x, b) = (upper.baseline.x.0, &upper.bounding_box);
                let from = lower.baseline.y.0 + lower.bounding_box.top.0;
                let to = upper.baseline.y.0 + b.bottom.0;
                for (dx, thickness) in barline_lines(upper.glyph.as_str(), b) {
                    let half = thickness / 2.0;
                    widen(extent, (x + dx - half, x + dx + half), (from, to));
                }
            }
        }
    }

    // Fold into its slot's extent each ledger line (a fixed-width stroke, by its
    // notehead: the same-source glyph whose baseline lies within the stroke's
    // span, its accidentals standing outside it to the left), so a ledger that
    // overhangs the notehead reserves room, and each stroke that rides one
    // slot at both ends (a stem), so a stem reaching another staff's beam is
    // cleared where it reaches.
    for stroke in &input.strokes {
        let slot = if is_rigid_width_stroke(stroke) {
            owning_glyph(stroke, &input.glyphs).map(|glyph| glyph.horizontal_slot)
        } else {
            anchors
                .get(&stroke.id())
                .filter(|(start, end)| start == end)
                .map(|(start, _)| *start)
        };
        let Some(extent) = slot.and_then(|slot| by_slot.get_mut(&slot)) else {
            continue;
        };
        let half = stroke.thickness.0 * 0.5;
        widen(
            extent,
            (
                stroke.from.x.0.min(stroke.to.x.0) - half,
                stroke.from.x.0.max(stroke.to.x.0) + half,
            ),
            (
                stroke.from.y.0.min(stroke.to.y.0) - half,
                stroke.from.y.0.max(stroke.to.y.0) + half,
            ),
        );
    }

    let mut slots: Vec<(SpringSlotId, Extent)> = by_slot.into_iter().collect();
    slots.sort_by(|a, b| {
        a.1.source
            .partial_cmp(&b.1.source)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // A tie anchored to two slots runs at least `TIE_MIN_SPAN` between its
    // ends: the later slot stands at least that far, and its end's and the
    // earlier end's offsets from their slots, after the earlier. Each
    // requirement is `(earlier slot, distance)`, by the later slot (a beam
    // across two staves adds its own below).
    let source_of: BTreeMap<SpringSlotId, f32> =
        slots.iter().map(|(id, e)| (*id, e.source)).collect();
    let mut ties: BTreeMap<SpringSlotId, Vec<(SpringSlotId, f32)>> = BTreeMap::new();
    for curve in &input.curves {
        let Some((start, end)) = anchors.get(&curve.id()) else {
            continue;
        };
        let (Some(&s0), Some(&s1)) = (source_of.get(start), source_of.get(end)) else {
            continue;
        };
        if start == end || s1 <= s0 {
            continue;
        }
        let need = (curve.p0.x.0 - s0) + TIE_MIN_SPAN + (s1 - curve.p3.x.0);
        ties.entry(*end).or_default().push((*start, need));
    }
    // Likewise a beam across two staves, between the stems at its ends.
    let crossing: std::collections::BTreeSet<_> = input
        .cross_staff_beams
        .iter()
        .flat_map(|beam| beam.ink.iter().copied())
        .collect();
    for stroke in input.strokes.iter().filter(|s| crossing.contains(&s.id())) {
        let Some((start, end)) = anchors.get(&stroke.id()) else {
            continue;
        };
        let (Some(&s0), Some(&s1)) = (source_of.get(start), source_of.get(end)) else {
            continue;
        };
        if start == end || s1 <= s0 {
            continue;
        }
        let need = (stroke.from.x.0 - s0) + CROSS_BEAM_MIN_SPAN + (s1 - stroke.to.x.0);
        ties.entry(*end).or_default().push((*start, need));
    }

    let mut points = Vec::with_capacity(slots.len());
    let mut placed: BTreeMap<SpringSlotId, (f32, f32)> = BTreeMap::new();
    // Each band's rightmost ink set so far, in the target frame, with the
    // clearance its slot asks after it: a note column `SLOT_GAP`, a barline
    // `BARLINE_CLEARANCE`, and a lead, signature or change the gap its spring
    // reserves after its ink, so a following accidental keeps that gap too.
    let mut frontier: BTreeMap<i32, f32> = BTreeMap::new();
    // The previous slot's target and spring.
    let mut previous: Option<(f32, f32)> = None;
    for (id, extent) in &slots {
        let mut target = previous.map_or(0.0, |(at, preferred)| at + preferred.max(MIN_ADVANCE));
        for (band, (left, _)) in &extent.bands {
            if let Some(&edge) = frontier.get(band) {
                target = target.max(edge + (extent.source - left));
            }
        }
        for (start, need) in ties.get(id).into_iter().flatten() {
            if let Some(&(_, at)) = placed.get(start) {
                target = target.max(at + need);
            }
        }
        let bearing = extent
            .bands
            .values()
            .map(|(_, right)| right - extent.source)
            .fold(0.0_f32, f32::max);
        let clearance = match (extent.fixed, extent.barline) {
            (false, _) => SLOT_GAP,
            (true, true) => BARLINE_CLEARANCE,
            (true, false) => (extent.preferred - bearing).max(SLOT_GAP),
        };
        for (band, (_, right)) in &extent.bands {
            let edge = target + (right - extent.source) + clearance;
            frontier
                .entry(*band)
                .and_modify(|e| *e = e.max(edge))
                .or_insert(edge);
        }
        points.push((extent.source, target));
        placed.insert(*id, (extent.source, target));
        previous = Some((target, extent.preferred));
    }
    points.dedup_by(|a, b| a.0 == b.0);
    SpacedSlots {
        points,
        by_slot: placed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{to_constrained, to_logical};

    #[test]
    fn control_points_are_monotonic_in_source_and_target() {
        let c = to_constrained(&to_logical(&valid_score_rich(7)));
        let spaced = space_slots(&c);
        assert!(!spaced.points.is_empty());
        for w in spaced.points.windows(2) {
            assert!(w[1].0 > w[0].0, "sources strictly increase");
            assert!(w[1].1 > w[0].1, "targets strictly increase");
        }
        // The two views describe one spacing: every placed slot's pair is one
        // of the control points (this fixture's slot sources are all distinct,
        // so the equal-source dedup removes nothing).
        for (source, target) in spaced.by_slot.values() {
            assert!(
                spaced
                    .points
                    .iter()
                    .any(|(s, t)| s == source && t == target),
                "slot pair ({source}, {target}) must be a control point"
            );
        }
    }

    #[test]
    fn spacing_re_spaces_rather_than_echoing_sources() {
        // A wide lead (clef) advances by more than a uniform note slot, so the
        // engraved targets are not a copy of the source columns.
        let c = to_constrained(&to_logical(&valid_score_rich(7)));
        let spaced = space_slots(&c);
        assert!(
            spaced.points.iter().any(|(s, t)| (s - t).abs() > 1e-3),
            "targets must differ from sources (re-spacing happened)"
        );
    }

    #[test]
    fn spacing_is_deterministic() {
        let c = to_constrained(&to_logical(&valid_score_rich(3)));
        let (a, b) = (space_slots(&c), space_slots(&c));
        assert_eq!(a.points, b.points);
        assert_eq!(a.by_slot, b.by_slot);
    }
}
