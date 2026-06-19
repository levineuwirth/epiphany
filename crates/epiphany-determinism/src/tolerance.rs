//! The named tolerance classes.
//!
//! Appendix D §"Tolerance Classes": specifications and conformance documents
//! **must not** introduce ad-hoc epsilon constants. Every numerical tolerance
//! that affects normative behavior must be declared as a named [`Tolerance`]
//! belonging to one of the five [`ToleranceClass`]es, with an explicit unit
//! (implied by the class), absolute and optional relative bounds, and a
//! [`ToleranceGovernance`] category.
//!
//! Tolerances never apply to identity: ids, graph membership, hash identity,
//! and operation ordering are exact, never "within tolerance".
//!
//! The concrete tolerance *values* for each profile and tier are normative in
//! the companion specifications (Quality Metric Catalog, Reference Suite,
//! Performance Reference Suite); this crate provides only the vocabulary.

use crate::float::CanonicalF64;

/// The five tolerance classes. The measurement unit is implied by the class.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ToleranceClass {
    /// Acoustic pitch comparison, in cents (Chapter 2 §"Cents as the Offset
    /// Unit"; Chapter 4 reference-pitch frequency resolution).
    AcousticCents,

    /// Layout coordinate comparison, in staff spaces (Chapter 7 §7.2).
    LayoutCoordinate,

    /// Quality-metric comparison; absolute on the `[0.0, 1.0]`
    /// `NormalizedMetric` scale (Chapter 9).
    QualityMetric,

    /// Tempo-integration residual: maximum permitted error in
    /// `musical_to_wallclock` / `wallclock_to_musical` conversion (Chapter 3).
    TempoIntegration,

    /// Solver residual: maximum permitted constraint violation for a soft
    /// constraint to count as satisfied (Chapter 9).
    SolverResidual,
}

impl ToleranceClass {
    /// A stable, locale-independent unit label for diagnostics. Non-normative
    /// text; the class identity is what is normative.
    pub const fn unit(self) -> &'static str {
        match self {
            ToleranceClass::AcousticCents => "cents",
            ToleranceClass::LayoutCoordinate => "staff spaces",
            ToleranceClass::QualityMetric => "normalized [0,1]",
            ToleranceClass::TempoIntegration => "wallclock seconds",
            ToleranceClass::SolverResidual => "constraint units",
        }
    }
}

/// How a tolerance participates in canonical behavior.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ToleranceGovernance {
    /// Affects canonical equality (rare; usually only diagnostic comparison).
    Equality,

    /// A validation threshold: a constraint counts as satisfied when the
    /// violation is below the tolerance.
    Validation,

    /// Affects only diagnostic output; canonical state is unaffected.
    Diagnostic,
}

/// A fully specified numerical tolerance.
///
/// Bounds are held as [`CanonicalF64`] so a tolerance can never carry NaN,
/// infinity, or `-0.0` — the same float hygiene canonical state requires
/// everywhere else.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Tolerance {
    /// Which class this tolerance belongs to; fixes the unit.
    pub class: ToleranceClass,

    /// Absolute tolerance, in the class's unit.
    pub absolute: CanonicalF64,

    /// Optional relative tolerance, applied to nonzero values.
    pub relative: Option<CanonicalF64>,

    /// What the tolerance governs.
    pub governance: ToleranceGovernance,
}

impl Tolerance {
    /// Constructs an absolute-only tolerance, rejecting non-finite bounds and
    /// negative absolute bounds (a tolerance is a non-negative magnitude).
    pub fn absolute(
        class: ToleranceClass,
        absolute: f64,
        governance: ToleranceGovernance,
    ) -> Option<Self> {
        let absolute = CanonicalF64::new(absolute)?;
        if absolute.get() < 0.0 {
            return None;
        }
        Some(Tolerance {
            class,
            absolute,
            relative: None,
            governance,
        })
    }

    /// Adds a relative bound, rejecting a non-finite or negative value.
    pub fn with_relative(mut self, relative: f64) -> Option<Self> {
        let relative = CanonicalF64::new(relative)?;
        if relative.get() < 0.0 {
            return None;
        }
        self.relative = Some(relative);
        Some(self)
    }

    /// Whether `value` is within tolerance of `reference`: the combined
    /// absolute-or-relative test, `|value - reference| <= absolute +
    /// relative * |reference|`.
    ///
    /// Returns `false` unless both operands are finite. A tolerance bounds a
    /// real measurement against a real reference; a NaN or infinite operand is
    /// never "within tolerance" (without this guard, `within(0.0, inf)` with a
    /// relative bound would spuriously return `true`, since both the difference
    /// and the bound become infinite).
    ///
    /// This is a deterministic *validation/diagnostic* comparison. It must
    /// never be used to decide identity (Appendix D: tolerances never apply to
    /// ids, membership, ordering, or hashes).
    pub fn within(&self, value: f64, reference: f64) -> bool {
        if !value.is_finite() || !reference.is_finite() {
            return false;
        }
        let diff = (value - reference).abs();
        let bound = self.absolute.get() + self.relative.map_or(0.0, |r| r.get() * reference.abs());
        diff <= bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_constructor_rejects_bad_bounds() {
        assert!(Tolerance::absolute(
            ToleranceClass::AcousticCents,
            f64::NAN,
            ToleranceGovernance::Validation
        )
        .is_none());
        assert!(Tolerance::absolute(
            ToleranceClass::AcousticCents,
            -1.0,
            ToleranceGovernance::Validation
        )
        .is_none());
        assert!(Tolerance::absolute(
            ToleranceClass::AcousticCents,
            0.0,
            ToleranceGovernance::Validation
        )
        .is_some());
    }

    #[test]
    fn within_uses_absolute_and_relative() {
        let t = Tolerance::absolute(
            ToleranceClass::LayoutCoordinate,
            0.01,
            ToleranceGovernance::Validation,
        )
        .unwrap();
        assert!(t.within(1.005, 1.0));
        assert!(!t.within(1.02, 1.0));

        let tr = t.with_relative(0.1).unwrap();
        // bound = 0.01 + 0.1 * |10| = 1.01
        assert!(tr.within(11.0, 10.0));
        assert!(!tr.within(11.1, 10.0));
    }

    #[test]
    fn within_rejects_non_finite_operands() {
        let tr = Tolerance::absolute(
            ToleranceClass::TempoIntegration,
            0.01,
            ToleranceGovernance::Validation,
        )
        .unwrap()
        .with_relative(0.1)
        .unwrap();
        // Without the finite guard, diff and bound both become inf -> true.
        assert!(!tr.within(0.0, f64::INFINITY));
        assert!(!tr.within(f64::INFINITY, 0.0));
        assert!(!tr.within(f64::NAN, 0.0));
        assert!(!tr.within(0.0, f64::NAN));
    }

    #[test]
    fn classes_have_distinct_units() {
        let classes = [
            ToleranceClass::AcousticCents,
            ToleranceClass::LayoutCoordinate,
            ToleranceClass::QualityMetric,
            ToleranceClass::TempoIntegration,
            ToleranceClass::SolverResidual,
        ];
        for (i, a) in classes.iter().enumerate() {
            for b in &classes[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
