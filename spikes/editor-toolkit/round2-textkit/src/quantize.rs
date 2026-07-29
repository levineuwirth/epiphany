//! The one place this crate quantizes an `f64` staff-space value onto the
//! `1/1024` grid (W3 §5 invariant 5). Every position this crate constructs
//! — glyph offsets, caret stops, the run's `origin`, `bounds`, and
//! `reserved_box` — goes through this function, so there is exactly one
//! rounding rule to audit rather than one per call site.
//!
//! ## It is the *canonical* rounding rule, not a second one
//!
//! W3 §5 is explicit that this must not be a separate convention:
//!
//! > Positions are staff-space y-up, quantized on the same 1/1024 grid as
//! > glyph positions (`resolved.rs:11-13`), **so text quantization is not a
//! > second convention.**
//!
//! Revision 2 of this module implemented the grid arithmetic locally as
//! `(v * 1024.0).round() / 1024.0` and *named* the divergence in a doc comment
//! — `f64::round` is round-half-**away-from-zero**, while
//! `epiphany_determinism::QuantizedCoord::from_staff_spaces` is round-half-to-
//! **even** (Appendix D) — with the reasoning that this spike's values never
//! land on a tie in practice. That reasoning was wrong twice over. It is not
//! checkable (nothing asserted that no value ever ties, and a font metric or a
//! padding constant could land on one at any time), and more importantly W3's
//! requirement is about the *convention*, not about whether the two conventions
//! happen to agree on today's inputs. Naming a divergence is not the same as
//! being allowed to take it.
//!
//! So [`quantize_component`] now routes through `QuantizedCoord` itself. There
//! is one quantizer in this project and this module calls it; the tie-break is
//! whatever Appendix D says it is, today and after any future change to it.

use epiphany_determinism::QuantizedCoord;

/// Rounds `v` onto the canonical `1/1024` staff-space grid, using the
/// project's own quantizer — **round-to-nearest, ties-to-even**, per
/// Appendix D.
///
/// # Panics
///
/// If `v` is NaN, infinite, or so large that its scaled unit count leaves
/// `i64` range. `QuantizedCoord::from_staff_spaces` returns `None` for those
/// rather than saturating, and this crate deliberately does not paper over it
/// with a default: a position that cannot be placed on the canonical grid is a
/// bug in whatever computed it, and the fixture generator is the right place
/// for it to stop. No such value occurs in this recipe — every input is a font
/// metric or a stated constant — so this panic is a guard, not a code path.
pub fn quantize_component(v: f64) -> f64 {
    QuantizedCoord::from_staff_spaces(v)
        .unwrap_or_else(|| {
            panic!(
                "{v} cannot be placed on the canonical 1/1024 staff-space grid (NaN, infinite, or \
                 outside i64 range) — see epiphany_determinism::QuantizedCoord::from_staff_spaces"
            )
        })
        .to_staff_spaces()
}

/// Whether `v` already sits **exactly** on the grid.
///
/// Revision 2 tested `(scaled - scaled.round()).abs() < 1e-6`, which claimed
/// exactness in its own doc comment while accepting anything within a
/// tolerance — the precise defect it was written to prevent, since "within
/// some tolerance of the grid" is what a fixed grid exists to rule out.
///
/// The test is now a round-trip through the canonical quantizer: `v` is on the
/// grid iff quantizing it changes nothing. That is exact by construction (the
/// multiply and divide by `1024` are exact for every value this crate handles,
/// `1024` being a power of two), and it rejects NaN, infinity and
/// out-of-range values as off-grid rather than passing them to a subtraction
/// that would produce `NaN < 1e-6 == false` by accident.
pub fn is_on_grid(v: f64) -> bool {
    QuantizedCoord::from_staff_spaces(v).map(|q| q.to_staff_spaces()) == Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recipe's original nominal origin. Kept as a permanent record of the
    /// case even though §3 now states the quantized value: the property that
    /// matters is that an unrepresentable literal is *moved*, not that this
    /// particular one has since been fixed.
    #[test]
    fn quantizing_the_recipes_nominal_origin_lands_on_grid() {
        let q = quantize_component(1.6);
        assert!(is_on_grid(q));
        assert_eq!(q, 1638.0 / 1024.0);
    }

    /// The constant the recipe now states must already be on the grid, so
    /// that quantizing it changes nothing. If someone re-edits
    /// `RUN_ORIGIN_STAFF` back to an unrepresentable literal, this fails
    /// rather than being repaired in silence.
    #[test]
    fn the_declared_run_origin_is_already_on_grid() {
        let (ox, oy) = crate::RUN_ORIGIN_STAFF;
        assert!(
            is_on_grid(ox as f64),
            "origin x {ox} is off the 1/1024 grid"
        );
        assert!(
            is_on_grid(oy as f64),
            "origin y {oy} is off the 1/1024 grid"
        );
        assert_eq!(quantize_component(ox as f64), ox as f64);
        assert_eq!(quantize_component(oy as f64), oy as f64);
    }

    #[test]
    fn the_unquantized_literal_is_not_on_grid() {
        // This is exactly the discrepancy the module doc comment names —
        // proven here so it cannot silently stop being true.
        assert!(!is_on_grid(1.6));
    }

    #[test]
    fn is_on_grid_kills_a_value_one_unit_off() {
        let q = quantize_component(1.6);
        assert!(!is_on_grid(q + 1.0 / 2048.0));
    }

    // ---- ties-to-even, the convention W3 requires ----
    //
    // Each case below is a half-grid value where ties-to-even and
    // ties-away-from-zero DISAGREE, so reverting `quantize_component` to
    // `(v * 1024.0).round() / 1024.0` fails every one of them. A tie that both
    // rules resolve the same way (scaled = 1.5, where both give 2) would prove
    // nothing, and is checked separately below so this distinction is on the
    // record rather than implied.

    /// `0.5` grid units, positive: ties-to-even rounds **down to 0**;
    /// ties-away-from-zero would round up to 1 unit.
    #[test]
    fn positive_half_grid_ties_to_even_down() {
        let v = 1.0 / 2048.0; // scaled = 0.5
        assert_eq!(quantize_component(v), 0.0);
        assert_ne!(quantize_component(v), 1.0 / 1024.0);
    }

    /// `2.5` grid units, positive: ties-to-even rounds **down to 2**;
    /// ties-away-from-zero would round up to 3.
    #[test]
    fn positive_half_grid_ties_to_even_at_two_and_a_half() {
        let v = 5.0 / 2048.0; // scaled = 2.5
        assert_eq!(quantize_component(v), 2.0 / 1024.0);
        assert_ne!(quantize_component(v), 3.0 / 1024.0);
    }

    /// `-0.5` grid units: ties-to-even rounds **towards zero**;
    /// ties-away-from-zero would round to -1 unit. The sign matters because
    /// `f64::round`'s bias is away from zero in *both* directions, so a
    /// positive-only test would miss half of the divergence.
    #[test]
    fn negative_half_grid_ties_to_even_towards_zero() {
        let v = -1.0 / 2048.0; // scaled = -0.5
        assert_eq!(quantize_component(v), 0.0);
        assert_ne!(quantize_component(v), -1.0 / 1024.0);
    }

    /// `-2.5` grid units: ties-to-even rounds **to -2**;
    /// ties-away-from-zero would round to -3.
    #[test]
    fn negative_half_grid_ties_to_even_at_minus_two_and_a_half() {
        let v = -5.0 / 2048.0; // scaled = -2.5
        assert_eq!(quantize_component(v), -2.0 / 1024.0);
        assert_ne!(quantize_component(v), -3.0 / 1024.0);
    }

    /// A tie the two rules agree on, stated so the four tests above are
    /// understood as testing the *disagreement* and not merely "ties round
    /// somewhere".
    #[test]
    fn a_tie_the_two_conventions_agree_on_is_not_evidence() {
        let v = 3.0 / 2048.0; // scaled = 1.5; ties-even -> 2, ties-away -> 2
        assert_eq!(quantize_component(v), 2.0 / 1024.0);
        assert_eq!((v * 1024.0).round() / 1024.0, 2.0 / 1024.0);
    }

    /// This module must not have its own arithmetic at all: quantizing agrees
    /// with the canonical type exactly, for ties and non-ties alike.
    #[test]
    fn quantizing_is_the_canonical_quantizer() {
        for v in [
            0.0,
            1.6,
            -1.6,
            1638.0 / 1024.0,
            1.0 / 2048.0,
            -1.0 / 2048.0,
            5.0 / 2048.0,
            -5.0 / 2048.0,
            17.151_367_187_5,
            -0.362_304_687_5,
        ] {
            let canonical = QuantizedCoord::from_staff_spaces(v)
                .unwrap()
                .to_staff_spaces();
            assert_eq!(quantize_component(v), canonical, "disagreement at {v}");
        }
    }

    /// `is_on_grid` is exact, not tolerant. A value one part in `2^40` off the
    /// grid is off the grid; revision 2's `1e-6` tolerance accepted it.
    #[test]
    fn is_on_grid_is_exact_not_tolerant() {
        let on = 1638.0 / 1024.0;
        assert!(is_on_grid(on));
        let barely_off = f64::from_bits(on.to_bits() + 1);
        assert_ne!(
            barely_off, on,
            "anchor: the perturbation must change the value"
        );
        assert!(
            (barely_off * 1024.0 - (barely_off * 1024.0).round()).abs() < 1e-6,
            "anchor: revision 2's tolerant test would have ACCEPTED this value"
        );
        assert!(
            !is_on_grid(barely_off),
            "one ULP off the grid is off the grid"
        );
    }

    /// Non-finite and out-of-range values are off-grid, not accidentally
    /// on-grid. Revision 2's subtraction produced `NaN`, and `NaN < 1e-6` is
    /// `false`, so NaN came out off-grid by luck rather than by rule.
    #[test]
    fn non_finite_values_are_off_grid_by_rule() {
        assert!(!is_on_grid(f64::NAN));
        assert!(!is_on_grid(f64::INFINITY));
        assert!(!is_on_grid(f64::NEG_INFINITY));
        assert!(!is_on_grid(1e30)); // scaled well past i64 range
    }
}
