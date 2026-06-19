//! Spatial primitives for the layout IR (Chapter 7 §"Spatial Primitives").
//!
//! IR coordinates are single-precision **staff spaces** ([`StaffSpace`],
//! Chapter 7 §7.2: "Single-precision floating point (`f32`) MUST be used for IR
//! coordinates"). Quantization to the canonical `1/1024`-staff-space grid
//! ([`epiphany_determinism::QuantizedCoord`]) happens only when *serializing*
//! canonical `ResolvedLayoutIR` output — exactly as Appendix D §"Quantized
//! Layout Coordinates" prescribes: "Internal solvers MAY use floating point
//! during computation; canonical serialization rounds to `QuantizedCoord`." The
//! quantization rule is round-to-nearest, ties-to-even
//! ([`QuantizedCoord::from_staff_space_f32`]).
//!
//! Conversion to absolute units (points, millimeters) via [`ScaleContext`]
//! occurs only at the render boundary, which is out of v0 scope.

use epiphany_determinism::QuantizedCoord;

/// Staff space: the fundamental unit of music engraving — the distance between
/// adjacent staff lines (Chapter 7 §7.2). `f32`, per the IR-coordinate rule.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug, Default)]
pub struct StaffSpace(pub f32);

impl StaffSpace {
    /// Quantizes to the canonical `1/1024` grid (round-to-nearest, ties-to-even).
    /// Returns `None` for a non-finite or out-of-range value, which canonical
    /// state forbids (Appendix D).
    pub fn quantize(self) -> Option<QuantizedCoord> {
        QuantizedCoord::from_staff_space_f32(self.0)
    }
}

/// Points-per-staff-space scaling, applied only at the render boundary
/// (Chapter 7 §7.2: `ScaleContext`). Out of v0 scope beyond the type.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct ScaleContext {
    /// Points per staff space (typically 4–10).
    pub points_per_staff_space: f32,
}

/// A 2-D point in staff spaces (Chapter 7 §"Geometric Types": `Point2D`). This
/// is the working IR coordinate; canonical output is its [`Point::quantize`].
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Point {
    pub x: StaffSpace,
    pub y: StaffSpace,
}

impl Point {
    /// The origin (`0, 0`).
    pub const ORIGIN: Point = Point {
        x: StaffSpace(0.0),
        y: StaffSpace(0.0),
    };

    /// Constructs a point from staff-space coordinates.
    pub const fn new(x: f32, y: f32) -> Self {
        Point {
            x: StaffSpace(x),
            y: StaffSpace(y),
        }
    }

    /// Quantizes both coordinates to the canonical grid (Appendix D). Returns
    /// `None` if either coordinate is non-finite or out of range.
    pub fn quantize(self) -> Option<(QuantizedCoord, QuantizedCoord)> {
        Some((self.x.quantize()?, self.y.quantize()?))
    }
}

/// A 2-D size in staff spaces (Chapter 7 §"Geometric Types": `Size2D`).
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Size2D {
    pub width: StaffSpace,
    pub height: StaffSpace,
}

/// A glyph's bounding box in staff spaces, relative to its anchor (Chapter 7
/// §"Geometric Types"). Carried in the in-tree glyph catalog ([`crate::glyph`]);
/// metrics are queried from the catalog, never embedded in pipeline objects
/// (Chapter 7 §"Glyph metrics live elsewhere").
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct BoundingBox {
    pub left: StaffSpace,
    pub bottom: StaffSpace,
    pub right: StaffSpace,
    pub top: StaffSpace,
}

/// An axis-aligned rectangle in staff-space coordinates.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Rect {
    pub origin: Point,
    pub size: Size2D,
}

/// A 2-D affine/projective transform in homogeneous coordinates.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Transform2D {
    pub matrix: [[f32; 3]; 3],
}

impl Default for Transform2D {
    fn default() -> Self {
        Transform2D {
            matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

/// Page margins in staff-space units.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Margins {
    pub top: StaffSpace,
    pub right: StaffSpace,
    pub bottom: StaffSpace,
    pub left: StaffSpace,
}

impl BoundingBox {
    /// Constructs from staff-space extents `[left, bottom, right, top]`.
    pub const fn new(left: f32, bottom: f32, right: f32, top: f32) -> Self {
        BoundingBox {
            left: StaffSpace(left),
            bottom: StaffSpace(bottom),
            right: StaffSpace(right),
            top: StaffSpace(top),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantization_rounds_to_the_grid() {
        // 1 staff space == 1024 grid units; exact on the grid.
        assert_eq!(
            Point::new(1.0, -2.0).quantize(),
            Some((
                QuantizedCoord::from_units(1024),
                QuantizedCoord::from_units(-2048)
            ))
        );
        // Round-to-nearest, ties-to-even: 1/2048 staff space rounds toward even.
        let half_unit = 0.5 / 1024.0; // half a grid unit
        assert_eq!(
            StaffSpace(half_unit).quantize(),
            Some(QuantizedCoord::from_units(0))
        );
    }

    #[test]
    fn non_finite_coordinates_do_not_quantize() {
        assert_eq!(StaffSpace(f32::NAN).quantize(), None);
        assert_eq!(StaffSpace(f32::INFINITY).quantize(), None);
        assert_eq!(Point::new(f32::NAN, 0.0).quantize(), None);
    }
}
