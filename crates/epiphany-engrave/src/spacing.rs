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
//! The advance from one slot to the next is the larger of the slot's
//! `preferred_width` (the spring's natural width — a uniform placeholder in v0)
//! and a **collision minimum** derived from real glyph bounding boxes: the slot's
//! right content extent, plus a gap, plus the *next* slot's left overhang (its
//! accidental zone). Reserving the next slot's left overhang against *this* slot's
//! advance is what protects a note's accidental from overlapping the previous
//! note — a single per-slot `preferred_width` could only reserve space to the
//! right of a slot's source. The casting-off pass (`crate::casting`) then breaks
//! this spaced line into systems and pages; the vertical soft-spring
//! stretch/compress solve and per-system justification remain deferred (see
//! `DECISIONS.md`).

use std::collections::BTreeMap;

use epiphany_layout_ir::{is_rigid_width_stroke, ConstrainedLayoutIR, SpringSlotId};

use crate::owning_glyph;

/// Inter-slot gap (staff spaces) reserved between one slot's right content and
/// the next slot's left content.
const SLOT_GAP: f32 = 0.3;

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
/// column reference (its first member glyph's baseline); the target
/// accumulates collision-aware advances so neighbouring slots' content —
/// including left-overhanging accidentals — never overlaps, and a wide lead
/// (clef + key signature) reserves real space. Deterministic: a pure function
/// of the glyphs and their bounding boxes.
pub(crate) fn space_slots(input: &ConstrainedLayoutIR) -> SpacedSlots {
    /// One slot's horizontal extent, from its member glyphs.
    struct Extent {
        /// Column reference x (the first member's baseline).
        source: f32,
        /// Leftmost / rightmost content edge across the slot's glyphs.
        min_left: f32,
        max_right: f32,
        /// The spring's natural width.
        preferred: f32,
    }

    let preferred_of: BTreeMap<SpringSlotId, f32> = input
        .horizontal_slots
        .iter()
        .map(|s| (s.id, s.preferred_width.0))
        .collect();
    let mut by_slot: BTreeMap<SpringSlotId, Extent> = BTreeMap::new();
    for glyph in &input.glyphs {
        let left = glyph.baseline.x.0 + glyph.bounding_box.left.0;
        let right = glyph.baseline.x.0 + glyph.bounding_box.right.0;
        by_slot
            .entry(glyph.horizontal_slot)
            .and_modify(|e| {
                e.min_left = e.min_left.min(left);
                e.max_right = e.max_right.max(right);
            })
            .or_insert(Extent {
                source: glyph.baseline.x.0,
                min_left: left,
                max_right: right,
                preferred: preferred_of
                    .get(&glyph.horizontal_slot)
                    .copied()
                    .unwrap_or(0.0),
            });
    }

    // Fold each ledger line (a fixed-width stroke) into its notehead's slot extent,
    // so a ledger that overhangs the notehead reserves room — otherwise adjacent
    // off-staff notes' ledgers can overlap even though glyph spacing is collision-
    // aware. The owning notehead is the same-source glyph whose baseline lies within
    // the stroke's span (its accidentals sit outside it, to the left).
    for stroke in &input.strokes {
        if !is_rigid_width_stroke(stroke) {
            continue;
        }
        if let Some(glyph) = owning_glyph(stroke, &input.glyphs) {
            let lo = stroke.from.x.0.min(stroke.to.x.0);
            let hi = stroke.from.x.0.max(stroke.to.x.0);
            if let Some(extent) = by_slot.get_mut(&glyph.horizontal_slot) {
                extent.min_left = extent.min_left.min(lo);
                extent.max_right = extent.max_right.max(hi);
            }
        }
    }

    let mut slots: Vec<(SpringSlotId, Extent)> = by_slot.into_iter().collect();
    slots.sort_by(|a, b| {
        a.1.source
            .partial_cmp(&b.1.source)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut points = Vec::with_capacity(slots.len());
    let mut placed: BTreeMap<SpringSlotId, (f32, f32)> = BTreeMap::new();
    let mut target = 0.0_f32;
    for i in 0..slots.len() {
        let (id, extent) = &slots[i];
        points.push((extent.source, target));
        placed.insert(*id, (extent.source, target));
        let right_bearing = extent.max_right - extent.source;
        // The next slot's left overhang must be cleared by *this* slot's advance.
        let next_left = slots
            .get(i + 1)
            .map(|(_, next)| next.source - next.min_left)
            .unwrap_or(0.0);
        let advance = extent.preferred.max(right_bearing + SLOT_GAP + next_left);
        target += advance;
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
