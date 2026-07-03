//! The Quality Metric Catalog's normative constants (companion specification
//! *Epiphany — Quality Metric Catalog*, v0.1.0): the per-axis normalization
//! anchors, the per-tier metric threshold table, the profile→threshold-column
//! mapping, and the `QualityFloorApproached` warning fraction.
//!
//! This module is a **transcription**, not an invention: every number here is
//! pinned by the catalog and cited to its chapter. Solvers that compute real
//! metrics (e.g. `epiphany-engrave`) normalize raw measurements through
//! [`normalize`] with the [`anchors`] of catalog Chapter 3 ("The Nine Normative
//! Metrics"), and reference the threshold tables of catalog Chapter 5
//! ("Per-Tier Metric Thresholds") — as does the reference-suite harness in
//! `epiphany-testkit`. The in-crate [`StubSolver`](crate::StubSolver) computes
//! no metrics and touches none of this (it stays on the all-worst
//! [`QualityMetricVector::unmeasured`] placeholder, the honest "no claim"
//! vector the catalog's vacuous-geometry requirement reserves for a solver
//! that computes no metrics at all).

use crate::solver::{
    NormalizedMetric, QualityMetricKind, QualityMetricVector, SolverProfile, SolverTier,
};

/// The nine normative metric axes in their catalog order (catalog §"The
/// Normative Metric Set and `QualityMetricKind`", Table "kind-mapping").
pub const QUALITY_METRIC_KINDS: [QualityMetricKind; 9] = [
    QualityMetricKind::Collision,
    QualityMetricKind::Spacing,
    QualityMetricKind::SlurShape,
    QualityMetricKind::BeamSlope,
    QualityMetricKind::VerticalDensity,
    QualityMetricKind::SystemBreak,
    QualityMetricKind::PageFill,
    QualityMetricKind::CastingOff,
    QualityMetricKind::SymbolDensity,
];

/// The per-axis normalization anchors `R_worst` (catalog Chapter 3): each
/// normative metric defines a dimensionless raw measurement `raw >= 0` and a
/// pinned anchor, and normalizes by the clamped-linear map
/// `n = min(1, raw / R_worst)` (catalog §"Normalization Form",
/// `req:qmc:normalization-form`). Implementations MUST use these anchors;
/// arbitrary normalization is non-conforming.
pub mod anchors {
    /// `collision_penalty` (catalog §`collision_penalty`): colliding
    /// cross-column pairs per glyph; one collision per twenty glyphs is
    /// worst-tolerable.
    pub const COLLISION_R_WORST: f64 = 0.05;
    /// `spacing_distortion` (catalog §`spacing_distortion`): mean per-system
    /// CV of column advances; CV 1.0 is spacing with no discernible
    /// regularity.
    pub const SPACING_R_WORST: f64 = 1.0;
    /// `slur_shape_penalty` (catalog §`slur_shape_penalty`): mean deviation of
    /// the arc ratio from the ideal band `[0.08, 0.25]`; a semicircular slur
    /// (deviation 0.25) is worst-tolerable.
    pub const SLUR_SHAPE_R_WORST: f64 = 0.25;
    /// `beam_slope_penalty` (catalog §`beam_slope_penalty`): mean slope excess
    /// over 0.25; slope 0.5 (deviation 0.25) is worst-tolerable.
    pub const BEAM_SLOPE_R_WORST: f64 = 0.25;
    /// `vertical_density_penalty` (catalog §`vertical_density_penalty`): mean
    /// relative gap deviation `|r - p| / p`; a gap off by its own preferred
    /// size is worst-tolerable.
    pub const VERTICAL_DENSITY_R_WORST: f64 = 1.0;
    /// `system_break_penalty` (catalog §`system_break_penalty`): mean
    /// `|W - w_s| / W` over non-final systems; half-empty (or half-overflowing)
    /// non-final systems are worst-tolerable.
    pub const SYSTEM_BREAK_R_WORST: f64 = 0.5;
    /// `page_fill_efficiency` (catalog §`page_fill_efficiency`): mean unfilled
    /// fraction of non-final pages; three-quarters empty is worst-tolerable.
    pub const PAGE_FILL_R_WORST: f64 = 0.75;
    /// `casting_off_quality` (catalog §`casting_off_quality`): mean per-region
    /// CV of system widths (final system included); CV 0.5 is worst-tolerable.
    pub const CASTING_OFF_R_WORST: f64 = 0.5;
    /// `symbol_density_uniformity` (catalog §`symbol_density_uniformity`):
    /// mean per-region CV of glyphs-per-width densities; CV 0.5 is
    /// worst-tolerable.
    pub const SYMBOL_DENSITY_R_WORST: f64 = 0.5;
}

/// The pinned anchor `R_worst` for a normative axis (catalog Chapter 3; see
/// [`anchors`]).
pub fn r_worst(kind: QualityMetricKind) -> f64 {
    match kind {
        QualityMetricKind::Collision => anchors::COLLISION_R_WORST,
        QualityMetricKind::Spacing => anchors::SPACING_R_WORST,
        QualityMetricKind::SlurShape => anchors::SLUR_SHAPE_R_WORST,
        QualityMetricKind::BeamSlope => anchors::BEAM_SLOPE_R_WORST,
        QualityMetricKind::VerticalDensity => anchors::VERTICAL_DENSITY_R_WORST,
        QualityMetricKind::SystemBreak => anchors::SYSTEM_BREAK_R_WORST,
        QualityMetricKind::PageFill => anchors::PAGE_FILL_R_WORST,
        QualityMetricKind::CastingOff => anchors::CASTING_OFF_R_WORST,
        QualityMetricKind::SymbolDensity => anchors::SYMBOL_DENSITY_R_WORST,
    }
}

/// The catalog's clamped-linear normalization (catalog §"Normalization Form",
/// `req:qmc:normalization-form`): `n = min(1, raw / R_worst)`, so `raw = 0`
/// (the ideal) normalizes to `0.0` and `raw >= R_worst` (the worst-tolerable
/// anchor and beyond) normalizes to `1.0`.
///
/// `raw` must be a finite, non-negative measurement and `r_worst` a positive
/// anchor, per the catalog; the result is a valid [`NormalizedMetric`] by
/// construction.
pub fn normalize(raw: f64, r_worst: f64) -> NormalizedMetric {
    assert!(
        raw.is_finite() && raw >= 0.0,
        "a raw quality measurement must be finite and non-negative (got {raw})"
    );
    assert!(
        r_worst > 0.0,
        "a normalization anchor must be positive (got {r_worst})"
    );
    NormalizedMetric::new((raw / r_worst).min(1.0))
}

/// One column of the catalog's per-tier threshold table (catalog Chapter 5,
/// Table "tier-thresholds"): the maximum permitted [`NormalizedMetric`] value
/// per axis for a reference-suite entry evaluated at that tier.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct MetricThresholds {
    pub collision_penalty: f64,
    pub spacing_distortion: f64,
    pub slur_shape_penalty: f64,
    pub beam_slope_penalty: f64,
    pub vertical_density_penalty: f64,
    pub system_break_penalty: f64,
    pub page_fill_efficiency: f64,
    pub casting_off_quality: f64,
    pub symbol_density_uniformity: f64,
}

impl MetricThresholds {
    /// The column's threshold for one axis.
    pub fn axis(&self, kind: QualityMetricKind) -> f64 {
        match kind {
            QualityMetricKind::Collision => self.collision_penalty,
            QualityMetricKind::Spacing => self.spacing_distortion,
            QualityMetricKind::SlurShape => self.slur_shape_penalty,
            QualityMetricKind::BeamSlope => self.beam_slope_penalty,
            QualityMetricKind::VerticalDensity => self.vertical_density_penalty,
            QualityMetricKind::SystemBreak => self.system_break_penalty,
            QualityMetricKind::PageFill => self.page_fill_efficiency,
            QualityMetricKind::CastingOff => self.casting_off_quality,
            QualityMetricKind::SymbolDensity => self.symbol_density_uniformity,
        }
    }
}

/// The **Minimal** threshold column (catalog Chapter 5, Table
/// "tier-thresholds"): uniformly `0.90` — relaxed but non-vacuous, excluding
/// layouts at an axis's worst-tolerable anchor and the all-worst unmeasured
/// placeholder ("measuring is part of the Minimal claim").
pub const MINIMAL_THRESHOLDS: MetricThresholds = MetricThresholds {
    collision_penalty: 0.90,
    spacing_distortion: 0.90,
    slur_shape_penalty: 0.90,
    beam_slope_penalty: 0.90,
    vertical_density_penalty: 0.90,
    system_break_penalty: 0.90,
    page_fill_efficiency: 0.90,
    casting_off_quality: 0.90,
    symbol_density_uniformity: 0.90,
};

/// The **Standard** threshold column (catalog Chapter 5, Table
/// "tier-thresholds"): professional engraving quality — collisions bounded
/// tightest (`0.25`), the break family at `0.35`, distribution/vertical proxies
/// at `0.40`, slur/beam shape at `0.30`.
pub const STANDARD_THRESHOLDS: MetricThresholds = MetricThresholds {
    collision_penalty: 0.25,
    spacing_distortion: 0.40,
    slur_shape_penalty: 0.30,
    beam_slope_penalty: 0.30,
    vertical_density_penalty: 0.40,
    system_break_penalty: 0.35,
    page_fill_efficiency: 0.40,
    casting_off_quality: 0.35,
    symbol_density_uniformity: 0.40,
};

/// The `QualityFloorApproached` warning fraction (catalog §"The
/// `QualityFloorApproached` Warning", `req:qmc:floor-warning`): a solver SHOULD
/// warn for axis `k` when `k`'s computed value exceeds **0.8×** the applicable
/// threshold — the one selected by the solve's [`SolverProfile`]
/// ([`profile_thresholds`]). The warning is diagnostic: emitting it does not
/// change the solve's status.
pub const QUALITY_FLOOR_FRACTION: f64 = 0.8;

/// The threshold column a **conformance tier** is evaluated against on the
/// reference suite (catalog Chapter 5): `Minimal` has its own relaxed column;
/// `Standard` the professional column; `Advanced` imposes the Standard column
/// on the nine normative axes (plus per-extension thresholds,
/// `req:qmc:advanced`, which this table does not model). `Stub` is below every
/// conformance tier and is evaluated against nothing — it computes no metrics
/// and passes no suite.
pub fn tier_thresholds(tier: SolverTier) -> Option<&'static MetricThresholds> {
    match tier {
        SolverTier::Stub => None,
        SolverTier::Minimal => Some(&MINIMAL_THRESHOLDS),
        SolverTier::Standard | SolverTier::Advanced => Some(&STANDARD_THRESHOLDS),
    }
}

/// The threshold column a **registered profile** selects (catalog Chapter 6,
/// `req:qmc:profiles`): `Draft` → the Minimal column (few warnings, fast
/// iteration); `Standard` and `Publication` → the Standard column (no column
/// tighter than Standard is ratified in v0.1). This is the column the solver's
/// own `QualityFloorApproached` diagnostics reference during ordinary solves;
/// suite evaluation at a claimed tier always uses that *tier's* column
/// ([`tier_thresholds`]).
pub fn profile_thresholds(profile: SolverProfile) -> &'static MetricThresholds {
    match profile {
        SolverProfile::Draft => &MINIMAL_THRESHOLDS,
        SolverProfile::Standard | SolverProfile::Publication => &STANDARD_THRESHOLDS,
    }
}

impl QualityMetricVector {
    /// The vector's value for one normative axis, by its
    /// [`QualityMetricKind`] (catalog Table "kind-mapping": each kind names
    /// exactly one vector field).
    pub fn axis(&self, kind: QualityMetricKind) -> NormalizedMetric {
        match kind {
            QualityMetricKind::Collision => self.collision_penalty,
            QualityMetricKind::Spacing => self.spacing_distortion,
            QualityMetricKind::SlurShape => self.slur_shape_penalty,
            QualityMetricKind::BeamSlope => self.beam_slope_penalty,
            QualityMetricKind::VerticalDensity => self.vertical_density_penalty,
            QualityMetricKind::SystemBreak => self.system_break_penalty,
            QualityMetricKind::PageFill => self.page_fill_efficiency,
            QualityMetricKind::CastingOff => self.casting_off_quality,
            QualityMetricKind::SymbolDensity => self.symbol_density_uniformity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TieBreakingWeights;

    #[test]
    fn normalization_is_the_catalogs_clamped_linear_map() {
        assert_eq!(normalize(0.0, 0.5).0, 0.0);
        assert_eq!(normalize(0.25, 0.5).0, 0.5);
        assert_eq!(normalize(0.5, 0.5).0, 1.0);
        // At and beyond the anchor clamps to the worst-tolerable 1.0.
        assert_eq!(normalize(3.0, 0.5).0, 1.0);
    }

    #[test]
    #[should_panic(expected = "finite and non-negative")]
    fn normalization_rejects_a_negative_raw() {
        let _ = normalize(-0.1, 0.5);
    }

    #[test]
    fn minimal_is_uniformly_more_permissive_than_standard() {
        // Catalog Table "tier-thresholds": "Minimal is uniformly more
        // permissive than Standard on every axis."
        for kind in QUALITY_METRIC_KINDS {
            assert!(
                MINIMAL_THRESHOLDS.axis(kind) > STANDARD_THRESHOLDS.axis(kind),
                "{kind:?}"
            );
            // Both columns are valid NormalizedMetric bounds.
            assert!((0.0..=1.0).contains(&MINIMAL_THRESHOLDS.axis(kind)));
            assert!((0.0..=1.0).contains(&STANDARD_THRESHOLDS.axis(kind)));
        }
    }

    #[test]
    fn the_minimal_column_excludes_the_unmeasured_placeholder() {
        // Catalog Chapter 5 rationale: "a solver reporting the unmeasured 1.0
        // placeholder cannot pass the Minimal suite" — measuring is part of
        // the Minimal claim.
        let unmeasured = QualityMetricVector::unmeasured();
        assert!(QUALITY_METRIC_KINDS
            .iter()
            .any(|&k| unmeasured.axis(k).0 > MINIMAL_THRESHOLDS.axis(k)));
    }

    #[test]
    fn tier_and_profile_columns_map_per_the_catalog() {
        // Tiers (catalog ch5): Minimal has its own column; Standard and
        // Advanced share the Standard column; Stub is evaluated against nothing.
        assert_eq!(tier_thresholds(SolverTier::Stub), None);
        assert_eq!(
            tier_thresholds(SolverTier::Minimal),
            Some(&MINIMAL_THRESHOLDS)
        );
        assert_eq!(
            tier_thresholds(SolverTier::Standard),
            Some(&STANDARD_THRESHOLDS)
        );
        assert_eq!(
            tier_thresholds(SolverTier::Advanced),
            Some(&STANDARD_THRESHOLDS)
        );
        // Profiles (catalog ch6): Draft → Minimal column; Standard and
        // Publication → Standard column; Standard is the default profile.
        assert_eq!(
            profile_thresholds(SolverProfile::Draft),
            &MINIMAL_THRESHOLDS
        );
        assert_eq!(
            profile_thresholds(SolverProfile::Standard),
            &STANDARD_THRESHOLDS
        );
        assert_eq!(
            profile_thresholds(SolverProfile::Publication),
            &STANDARD_THRESHOLDS
        );
        assert_eq!(SolverProfile::default(), SolverProfile::Standard);
    }

    #[test]
    fn default_tie_breaking_weights_are_the_catalogs_normative_defaults() {
        // Catalog Chapter 4 (`req:qmc:weights`): every one of the nine weights
        // defaults to 1.0 — blessing the implementation's existing `Default`.
        let w = TieBreakingWeights::default();
        for value in [
            w.collision,
            w.spacing,
            w.slur_shape,
            w.beam_slope,
            w.vertical_density,
            w.system_break,
            w.page_fill,
            w.casting_off,
            w.symbol_density,
        ] {
            assert_eq!(value, 1.0);
        }
    }

    #[test]
    fn every_axis_has_a_positive_anchor() {
        for kind in QUALITY_METRIC_KINDS {
            assert!(r_worst(kind) > 0.0, "{kind:?}");
        }
    }
}
