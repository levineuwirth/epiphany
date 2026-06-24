//! The horizontal spacing pass — the first axis of the planned two-pass spring
//! layout (`epiphany-engrave`'s DECISIONS.md, decision 1).
//!
//! A `ConstrainedLayoutIR` carries one horizontal spring slot per musical time
//! column, each with a `preferred_width` in staff spaces. This pass walks the
//! slots in their emitted left-to-right order and assigns each an absolute `x`
//! by accumulating preferred widths, producing even, non-overlapping horizontal
//! spacing (the stub solver, by contrast, returns the raw input columns
//! verbatim). The vertical pass, the soft-spring stretch/compress solve, and
//! constraint evaluation are deferred to the `Minimal`-tier work (next phase).

use std::collections::BTreeMap;

use epiphany_layout_ir::{ConstrainedLayoutIR, SpringSlotId, StaffSpace};

/// The absolute `x` (in staff spaces) assigned to each spring slot, accumulated
/// left-to-right over the slots' emitted order. Deterministic: a pure function
/// of the slot sequence and their preferred widths.
pub(crate) fn slot_positions(input: &ConstrainedLayoutIR) -> BTreeMap<SpringSlotId, f32> {
    let mut positions = BTreeMap::new();
    let mut cursor = 0.0_f32;
    for slot in &input.horizontal_slots {
        positions.insert(slot.id, cursor);
        // Advance by the slot's preferred width; a non-finite or negative width
        // would have been rejected by `ConstrainedLayoutIR::validate`, but clamp
        // defensively so the cursor stays finite and monotonic regardless.
        let StaffSpace(width) = slot.preferred_width;
        cursor += if width.is_finite() && width >= 0.0 {
            width
        } else {
            0.0
        };
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_core::WallClockTime;
    use epiphany_layout_ir::{
        to_constrained, to_logical, GlyphCatalogIdentity, SpringSlot, TimePoint,
    };

    fn slot(id: u128, preferred: f32) -> SpringSlot {
        SpringSlot {
            id: SpringSlotId(id),
            time: TimePoint::WallClock(WallClockTime(id as i64)),
            min_width: StaffSpace(preferred),
            preferred_width: StaffSpace(preferred),
            max_width: None,
            stretch_factor: 1.0,
            compress_factor: 1.0,
            members: vec![],
        }
    }

    fn ir_with_slots(slots: Vec<SpringSlot>) -> ConstrainedLayoutIR {
        ConstrainedLayoutIR {
            source: Default::default(),
            regions: vec![],
            horizontal_slots: slots,
            glyphs: vec![],
            vertical_bands: vec![],
            constraints: vec![],
            engraving_decisions: vec![],
            catalog: GlyphCatalogIdentity::default(),
        }
    }

    #[test]
    fn slots_are_placed_left_to_right_by_accumulated_width() {
        let c = to_constrained(&to_logical(&valid_score_rich(7)));
        let positions = slot_positions(&c);
        // Every slot got a position equal to the running cursor, non-decreasing
        // in emitted order (preferred widths are non-negative).
        assert_eq!(positions.len(), c.horizontal_slots.len());
        let mut cursor = 0.0_f32;
        for slot in &c.horizontal_slots {
            let x = positions[&slot.id];
            assert!(x.is_finite());
            assert!(
                (x - cursor).abs() < 1e-3,
                "slot x must equal the running cursor"
            );
            cursor += slot.preferred_width.0;
        }
    }

    #[test]
    fn spacing_uses_preferred_widths_not_input_columns() {
        // Slots that each prefer 2.0 staff spaces lay out at 0, 2, 4 — a pure
        // function of widths, independent of any input baseline column.
        let input = ir_with_slots(vec![slot(1, 2.0), slot(2, 2.0), slot(3, 2.0)]);
        let p = slot_positions(&input);
        assert_eq!(p[&SpringSlotId(1)], 0.0);
        assert_eq!(p[&SpringSlotId(2)], 2.0);
        assert_eq!(p[&SpringSlotId(3)], 4.0);
    }

    #[test]
    fn spacing_is_deterministic() {
        let input = ir_with_slots(vec![slot(10, 1.5), slot(20, 1.5)]);
        assert_eq!(slot_positions(&input), slot_positions(&input));
    }
}
