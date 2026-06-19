//! The canonical spatial coordinate grid.
//!
//! Appendix D §"Quantized Layout Coordinates": canonical layout coordinates
//! are quantized to a fixed grid of `1/1024` staff space per unit. Internal
//! solvers may use floating point during computation; the act of quantization
//! at serialization time absorbs all floating-point variation from canonical
//! output. Two implementations whose internal computations agree to better
//! than `1/2048` staff space at every coordinate produce identical canonical
//! output after quantization.
//!
//! Chapter 7 §7.2 fixes the surrounding context: IR coordinates are expressed
//! in staff spaces (as `f32`); the conversion to this canonical integer grid
//! happens only when emitting canonical `ResolvedLayoutIR`.

/// Canonical-grid resolution: units of `1/1024` staff space per unit. Fixed
/// for this format version; changing it is a non-backward-compatible major
/// change (Appendix D).
pub const STAFF_SPACE_GRID: i64 = 1024;

/// First `f64` strictly above `i64::MAX` (`2^63`). A finite coordinate is
/// representable as `QuantizedCoord` iff its scaled unit value lands in
/// `[-2^63, 2^63)`; outside that, `as i64` would silently saturate.
const I64_SPAN: f64 = 9_223_372_036_854_775_808.0;

/// A canonical spatial coordinate, quantized to `1/1024` of a staff space.
///
/// This is the exact integer type that appears in canonical serialized
/// `ResolvedLayoutIR`. Solvers compute in floating point and round to this
/// grid via [`QuantizedCoord::from_staff_spaces`] at serialization time.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct QuantizedCoord {
    /// Coordinate value in units of `1/1024` staff space.
    pub units: i64,
}

impl QuantizedCoord {
    /// The origin (`0` staff spaces).
    pub const ORIGIN: QuantizedCoord = QuantizedCoord { units: 0 };

    /// Constructs directly from a grid-unit count.
    #[inline]
    pub const fn from_units(units: i64) -> Self {
        QuantizedCoord { units }
    }

    /// Quantizes a finite, representable staff-space measurement onto the
    /// canonical grid using round-to-nearest, ties-to-even on the integer unit
    /// value (Appendix D).
    ///
    /// Returns `None` for input that cannot be faithfully placed on the grid:
    /// NaN, infinity, or a value whose scaled unit count falls outside the
    /// `i64` range. The spec's contract is to *quantize finite internal
    /// coordinates*, not to normalize invalid geometry into canonical state —
    /// so an `as i64` saturation that would turn `+inf` into `i64::MAX` or NaN
    /// into `0` is reported as a failure instead of silently accepted. Callers
    /// with a value they know is on-grid can use [`QuantizedCoord::from_units`].
    #[inline]
    pub fn from_staff_spaces(staff_spaces: f64) -> Option<Self> {
        if !staff_spaces.is_finite() {
            return None;
        }
        let scaled = (staff_spaces * STAFF_SPACE_GRID as f64).round_ties_even();
        if !(-I64_SPAN..I64_SPAN).contains(&scaled) {
            return None;
        }
        // `scaled` is an integer-valued f64 within `[-2^63, 2^63)`, so the cast
        // is exact (no saturation).
        Some(QuantizedCoord {
            units: scaled as i64,
        })
    }

    /// Quantizes an `f32` staff-space measurement (the IR's native precision,
    /// Chapter 7 §7.2) by widening to `f64` first so the multiply-by-1024 and
    /// the tie-break happen in the wider format. Same rejection rules as
    /// [`QuantizedCoord::from_staff_spaces`].
    #[inline]
    pub fn from_staff_space_f32(staff_spaces: f32) -> Option<Self> {
        Self::from_staff_spaces(staff_spaces as f64)
    }

    /// The coordinate as a staff-space measurement. Exact for
    /// `|units| < 2^53`; `1024` is a power of two, so the division introduces
    /// no rounding in that range.
    #[inline]
    pub fn to_staff_spaces(self) -> f64 {
        self.units as f64 / STAFF_SPACE_GRID as f64
    }

    /// Canonical little-endian serialization (8 bytes, `i64`).
    #[inline]
    pub fn to_le_bytes(self) -> [u8; 8] {
        self.units.to_le_bytes()
    }

    /// Decodes canonical little-endian bytes. Total: every 8-byte string is a
    /// valid `i64` grid coordinate.
    #[inline]
    pub fn from_le_bytes(bytes: [u8; 8]) -> Self {
        QuantizedCoord {
            units: i64::from_le_bytes(bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_staff_space_is_1024_units() {
        assert_eq!(QuantizedCoord::from_staff_spaces(1.0).unwrap().units, 1024);
        assert_eq!(
            QuantizedCoord::from_staff_spaces(-2.0).unwrap().units,
            -2048
        );
        assert_eq!(
            QuantizedCoord::from_staff_spaces(1.0)
                .unwrap()
                .to_staff_spaces(),
            1.0
        );
    }

    #[test]
    fn quantization_is_round_half_to_even() {
        // Build inputs that land exactly on `*.5` units after *1024.
        let q = |ss: f64| QuantizedCoord::from_staff_spaces(ss).unwrap().units;
        let half = 0.5 / STAFF_SPACE_GRID as f64; // 0.5 units -> ties to 0
        let one_half = 1.5 / STAFF_SPACE_GRID as f64; // 1.5 units -> ties to 2
        let two_half = 2.5 / STAFF_SPACE_GRID as f64; // 2.5 units -> ties to 2
        let three_half = 3.5 / STAFF_SPACE_GRID as f64; // 3.5 units -> ties to 4
        assert_eq!(q(half), 0);
        assert_eq!(q(one_half), 2);
        assert_eq!(q(two_half), 2);
        assert_eq!(q(three_half), 4);
        // Negative ties also go to even.
        assert_eq!(q(-one_half), -2);
        assert_eq!(q(-two_half), -2);
    }

    #[test]
    fn sub_half_unit_variation_quantizes_away() {
        // Appendix D rationale: implementations agreeing to better than
        // 1/2048 staff space produce identical canonical output.
        let base = 12.0 / STAFF_SPACE_GRID as f64; // exactly 12 units
        let jitter = 0.49 / STAFF_SPACE_GRID as f64; // < half a unit
        assert_eq!(
            QuantizedCoord::from_staff_spaces(base + jitter),
            QuantizedCoord::from_staff_spaces(base - jitter)
        );
    }

    #[test]
    fn coordinate_round_trips_through_staff_spaces_in_musical_range() {
        for units in [-1_000_000, -1024, -1, 0, 1, 1024, 999_999] {
            let q = QuantizedCoord::from_units(units);
            assert_eq!(
                QuantizedCoord::from_staff_spaces(q.to_staff_spaces()),
                Some(q)
            );
        }
    }

    #[test]
    fn bytes_round_trip_full_i64_range() {
        for units in [i64::MIN, -1, 0, 1, 42, i64::MAX] {
            let q = QuantizedCoord::from_units(units);
            assert_eq!(QuantizedCoord::from_le_bytes(q.to_le_bytes()), q);
        }
    }

    #[test]
    fn invalid_geometry_is_rejected_not_normalized() {
        // NaN, infinities, and out-of-range magnitudes must NOT become valid
        // canonical coordinates via `as i64` saturation.
        assert_eq!(QuantizedCoord::from_staff_spaces(f64::NAN), None);
        assert_eq!(QuantizedCoord::from_staff_spaces(f64::INFINITY), None);
        assert_eq!(QuantizedCoord::from_staff_spaces(f64::NEG_INFINITY), None);
        assert_eq!(QuantizedCoord::from_staff_spaces(1e300), None);
        assert_eq!(QuantizedCoord::from_staff_spaces(-1e300), None);
        assert_eq!(QuantizedCoord::from_staff_space_f32(f32::INFINITY), None);
    }

    #[test]
    fn representable_range_boundary() {
        // Largest representable: just under 2^63 units, i.e. (2^63 / 1024)
        // staff spaces minus a hair. The exact i64::MIN unit count is
        // reachable; one staff space beyond the top is not.
        let min_ss = i64::MIN as f64 / STAFF_SPACE_GRID as f64;
        assert_eq!(
            QuantizedCoord::from_staff_spaces(min_ss),
            Some(QuantizedCoord::from_units(i64::MIN))
        );
        // 2^63 staff-space-units worth is out of range (would saturate).
        let over = I64_SPAN / STAFF_SPACE_GRID as f64;
        assert_eq!(QuantizedCoord::from_staff_spaces(over), None);
    }
}
