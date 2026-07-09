//! The constraint-solver interface (Chapter 9 "Constraint-Solver Interface")
//! and the v0 stub solver.
//!
//! Chapter 9 specifies the *interface* and its contracts, not an algorithm.
//! This module implements the interface surface in full shape — the
//! [`ConstraintSolver`] trait (`solve`/`solve_incremental`/`tier`/`version`,
//! `Send + Sync`), [`SolverConfig`] (profile, budget, tie-breaking weights),
//! [`SolverState`], the [`InvalidationSet`] (slots/bands/constraints/glyphs), and
//! a [`SolveReport`] with its full diagnostic surface (unsatisfied constraints,
//! warnings, a [`QualityMetricVector`], budget used, state) — and a
//! [`StubSolver`] that, per the QUICKSTART, "returns `SolveStatus::Solved` with
//! the input geometry verbatim" — for a constraint-free problem; with
//! constraints declared it stays a renderable passthrough but claims no
//! satisfaction (see [`StubSolver`]).
//!
//! **The stub computes no quality metrics** (QUICKSTART: "only the interface —
//! don't implement quality metrics"): the
//! [`QualityMetricVector`]/[`NormalizedMetric`] *types* and the
//! [`TieBreakingWeights`] exist (the interface requires them), and the Quality
//! Metric Catalog's normative anchors and threshold tables are transcribed in
//! [`crate::quality`] for solvers that do measure (`epiphany-engrave`). The
//! `StubSolver` is not a conformant solver and passes no reference suite, so it
//! reports the [`SolverTier::Stub`] tier (the honest non-conformance rung, below
//! `Minimal`) and an all-worst metric vector. Those values are deliberately
//! conservative placeholders, not computed quality measurements; the real solver
//! replaces them.

use epiphany_core::TypedObjectId;

use crate::constrained::{ConstrainedLayoutIR, GlyphObjectId};
use crate::glyph::{all_available, BravuraCatalog, GlyphCatalog};
use crate::resolved::{ResolvedGlyph, ResolvedLayoutIR, ResolvedPage, ResolvedSystem};
use crate::spatial::{Margins, Rect, Size2D};
use crate::vertical_band::VerticalBandId;

/// The solver status (Chapter 9 §"The Solver Report"). Variants and their
/// authority rules are quoted from the spec.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SolveStatus {
    /// All hard constraints satisfied, target quality reached.
    Solved,
    /// All hard constraints satisfied, but warnings were generated.
    SolvedWithWarnings,
    /// Deterministic budget exhausted before reaching target quality. The
    /// returned layout still satisfies all hard constraints.
    PartialBudgetExhausted,
    /// Hard constraints cannot be simultaneously satisfied; the layout is
    /// diagnostic-only.
    Unsatisfiable,
    /// Solver bug or unexpected error; the layout is diagnostic-only.
    InternalError,
}

impl SolveStatus {
    /// Whether a layout under this status may be rendered as authoritative
    /// (Chapter 9 §"The Solver Report").
    pub fn is_renderable(self) -> bool {
        matches!(
            self,
            SolveStatus::Solved
                | SolveStatus::SolvedWithWarnings
                | SolveStatus::PartialBudgetExhausted
        )
    }
}

/// The conformance tier a solver claims (Chapter 9 §"Conformance Tiers").
///
/// `Stub` is below the spec's three conformance tiers: it is *not* a conformance
/// claim but its honest absence — an interface-only solver that evaluates no
/// constraints and computes no quality metrics reports `Stub`, never `Minimal`,
/// so a caller cannot mistake the passthrough for the lowest conformant tier.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum SolverTier {
    /// Not a conformance tier: an interface-only / passthrough solver that
    /// evaluates no constraints and computes no quality metrics (e.g.
    /// [`StubSolver`]). Ordered below every conformant tier.
    Stub,
    /// Minimal Layout Solver.
    Minimal,
    /// Standard Engraving Solver.
    Standard,
    /// Advanced / Extension-Aware Solver.
    Advanced,
}

/// A solver's implementation version (Chapter 9: within a fixed version,
/// identical input produces identical output).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct SolverVersion(pub u32);

/// The conformance profile under which to solve (Chapter 9 §"The Solver
/// Interface": `SolverConfig.profile` — selects metric thresholds and the active
/// constraint/extension set). The registered profile catalog and each profile's
/// threshold column are the Quality Metric Catalog's Chapter 6, transcribed as
/// [`crate::quality::profile_thresholds`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum SolverProfile {
    /// Fast, low-quality (draft) profile.
    Draft,
    /// The reference engraving-quality profile.
    #[default]
    Standard,
    /// The highest-quality (publication) profile.
    Publication,
}

/// Tie-breaking weights among layouts of equivalent quality (Chapter 9
/// §"Quality Metrics": `TieBreakingWeights`). The normative defaults are the
/// Quality Metric Catalog's Chapter 4: every weight `1.0` — exactly this
/// type's [`Default`].
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct TieBreakingWeights {
    pub collision: f64,
    pub spacing: f64,
    pub slur_shape: f64,
    pub beam_slope: f64,
    pub vertical_density: f64,
    pub system_break: f64,
    pub page_fill: f64,
    pub casting_off: f64,
    pub symbol_density: f64,
}

impl Default for TieBreakingWeights {
    fn default() -> Self {
        TieBreakingWeights {
            collision: 1.0,
            spacing: 1.0,
            slur_shape: 1.0,
            beam_slope: 1.0,
            vertical_density: 1.0,
            system_break: 1.0,
            page_fill: 1.0,
            casting_off: 1.0,
            symbol_density: 1.0,
        }
    }
}

/// The deterministic budget (Chapter 9 §"The Solver Interface": `SolverBudget`).
/// Wall-clock time is advisory only; the canonical layout depends on the
/// deterministic counters.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct SolverBudget {
    pub max_iterations: u64,
    pub max_nodes: u64,
    pub max_constraint_evaluations: u64,
    pub advisory_wall_time_ms: Option<u64>,
}

impl Default for SolverBudget {
    fn default() -> Self {
        SolverBudget {
            max_iterations: u64::MAX,
            max_nodes: u64::MAX,
            max_constraint_evaluations: u64::MAX,
            advisory_wall_time_ms: None,
        }
    }
}

/// The deterministic budget consumed by a solve (Chapter 9: `SolverBudgetUsed`).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct SolverBudgetUsed {
    pub iterations: u64,
    pub nodes: u64,
    pub constraint_evaluations: u64,
    pub wall_time_ms: u64,
}

/// Solver configuration (Chapter 9 §"The Solver Interface": `SolverConfig`):
/// the conformance profile, the deterministic budget, and the tie-breaking
/// weights.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct SolverConfig {
    pub profile: SolverProfile,
    pub budget: SolverBudget,
    pub tie_breaking: TieBreakingWeights,
}

/// A quality metric normalized to `[0.0, 1.0]`, lower is better (Chapter 9
/// §"Quality Metrics": `NormalizedMetric`).
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug, Default)]
pub struct NormalizedMetric(pub f64);

impl NormalizedMetric {
    /// Constructs, panicking if the value is not finite or out of `[0.0, 1.0]`
    /// — conforming implementations construct only valid values (Chapter 9).
    pub fn new(value: f64) -> Self {
        assert!(value.is_finite(), "NormalizedMetric must be finite");
        assert!(
            (0.0..=1.0).contains(&value),
            "NormalizedMetric must lie in [0.0, 1.0]"
        );
        NormalizedMetric(value)
    }
}

/// An extension-contributed quality metric id (Chapter 9: `ExtensionMetricId`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ExtensionMetricId(pub u128);

/// An extension-contributed quality metric (Chapter 9: `ExtensionMetric`).
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ExtensionMetric {
    pub metric_id: ExtensionMetricId,
    pub value: NormalizedMetric,
}

/// The quality metric vector for a layout (Chapter 9 §"Quality Metrics":
/// `QualityMetricVector`). An interface-only solver that computes no metrics
/// reports the conservative all-worst placeholder
/// ([`QualityMetricVector::unmeasured`], every metric `1.0`), never a measured
/// value, so a caller cannot mistake an unmeasured layout for a good one. (The
/// derived [`Default`] is all-`0.0`/nominal-best and is *not* what the stub
/// reports.) A measuring solver computes each axis per the Quality Metric
/// Catalog's formulas, normalized through [`crate::quality::normalize`] with
/// the catalog's pinned anchors ([`crate::quality::anchors`]).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct QualityMetricVector {
    pub collision_penalty: NormalizedMetric,
    pub spacing_distortion: NormalizedMetric,
    pub slur_shape_penalty: NormalizedMetric,
    pub beam_slope_penalty: NormalizedMetric,
    pub vertical_density_penalty: NormalizedMetric,
    pub system_break_penalty: NormalizedMetric,
    pub page_fill_efficiency: NormalizedMetric,
    pub casting_off_quality: NormalizedMetric,
    pub symbol_density_uniformity: NormalizedMetric,
    pub extension_metrics: Vec<ExtensionMetric>,
}

impl QualityMetricVector {
    /// A conservative placeholder for an interface-only solver that does not
    /// compute quality metrics. Every built-in metric is worst-valued.
    pub fn unmeasured() -> Self {
        let worst = NormalizedMetric::new(1.0);
        QualityMetricVector {
            collision_penalty: worst,
            spacing_distortion: worst,
            slur_shape_penalty: worst,
            beam_slope_penalty: worst,
            vertical_density_penalty: worst,
            system_break_penalty: worst,
            page_fill_efficiency: worst,
            casting_off_quality: worst,
            symbol_density_uniformity: worst,
            extension_metrics: Vec::new(),
        }
    }
}

/// Opaque solver state threaded into [`ConstraintSolver::solve_incremental`]
/// (Chapter 9 §"The Solver Report": `SolverState`). v0 records the solver
/// version and the resolved-glyph count, enough to drive the observational-
/// equivalence contract for the trivial stub.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct SolverState {
    pub solver_version: Option<SolverVersion>,
    pub resolved_glyphs: usize,
}

/// The scope of an incremental invalidation (Chapter 9 §"Incremental Solving":
/// `InvalidationScope`). The solver MAY widen this conservatively; it MUST NOT
/// narrow it.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum InvalidationScope {
    ObjectLocal,
    MeasureLocal,
    SystemLocal,
    PageLocal,
    RegionLocal,
    WholeScore,
}

/// A horizontal spring-slot id (Chapter 7 §"Spring Slots": `SpringSlotId`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SpringSlotId(pub u128);

/// A constraint identifier referenced by [`SolveReport::unsatisfied_constraints`]
/// (Chapter 9: `ConstraintId`). The stub never reports any: it evaluates no
/// constraints, so it neither claims one satisfied nor names one unsatisfied.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConstraintId(pub u128);

/// The strength a constraint binds the solver with (Chapter 9 §"Strength
/// Levels": `ConstraintStrength`). Constraints do not carry this in the IR —
/// the spec's [`crate::LayoutConstraint`] enum has no strength field — so it is
/// attached by rule via [`crate::LayoutConstraint::strength`].
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ConstraintStrength {
    /// Hard constraint. The solver MUST satisfy it or return
    /// [`SolveStatus::Unsatisfiable`], and MUST NOT treat it as if it were
    /// `Preferred` for any reason, including quality optimization (Chapter 9
    /// §"Strength Levels").
    Required,
    /// Soft constraint with an associated weight. The solver minimizes the
    /// weighted violation when optimizing; an unhonoured preference is a
    /// warning ([`SolverWarningKind::LargeSoftConstraintViolation`]), never an
    /// `Unsatisfiable`.
    Preferred { weight: f64 },
}

/// A declared invalidation (Chapter 9: `InvalidationSet`) over the invalidated
/// slots, bands, constraints, and glyphs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InvalidationSet {
    pub scope: InvalidationScope,
    pub slots: Vec<SpringSlotId>,
    pub bands: Vec<VerticalBandId>,
    pub constraints: Vec<ConstraintId>,
    pub glyphs: Vec<GlyphObjectId>,
}

/// A normative quality-metric axis (Chapter 9 §"Quality Metrics"), referenced by
/// [`SolverWarningKind::QualityFloorApproached`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum QualityMetricKind {
    Collision,
    Spacing,
    SlurShape,
    BeamSlope,
    VerticalDensity,
    SystemBreak,
    PageFill,
    CastingOff,
    SymbolDensity,
}

/// An extension-defined solver-warning id (Chapter 9: `ExtensionWarningId`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ExtensionWarningId(pub u128);

/// The kind of a non-fatal solver warning (Chapter 9 §"The Solver Report":
/// `SolverWarningKind`) — every normative variant.
#[derive(Clone, PartialEq, Debug)]
pub enum SolverWarningKind {
    LargeSoftConstraintViolation {
        constraint: ConstraintId,
        magnitude: f64,
    },
    UnusualLayoutDecision(String),
    QualityFloorApproached {
        metric: QualityMetricKind,
    },
    ExtensionWarning(ExtensionWarningId),
}

/// A non-fatal solver warning (Chapter 9 §"The Solver Report": `SolverWarning`).
#[derive(Clone, PartialEq, Debug)]
pub struct SolverWarning {
    pub kind: SolverWarningKind,
    pub affected_objects: Vec<TypedObjectId>,
    pub message: String,
}

/// The solver report (Chapter 9 §"The Solver Report"). The `layout` is always
/// present; under a failure `status` it is diagnostic-only and MUST NOT be used
/// as if valid.
#[derive(Clone, PartialEq, Debug)]
pub struct SolveReport {
    pub status: SolveStatus,
    /// Whether every hard constraint is satisfied.
    pub satisfied_hard_constraints: bool,
    pub layout: ResolvedLayoutIR,
    /// Unsatisfied hard constraints, if any (empty under `Solved`).
    pub unsatisfied_constraints: Vec<ConstraintId>,
    /// Non-fatal warnings about the solution.
    pub warnings: Vec<SolverWarning>,
    /// Quality metric vector for the returned layout.
    pub metric_vector: QualityMetricVector,
    /// Budget consumed during this solve.
    pub budget_used: SolverBudgetUsed,
    /// Updated solver state for subsequent incremental calls.
    pub state: SolverState,
}

/// The constraint-solver interface (Chapter 9 §"The Solver Interface"). `solve`
/// and `solve_incremental` MUST be pure functions of their inputs within the
/// determinism contract; the trait is `Send + Sync` per the spec.
pub trait ConstraintSolver: Send + Sync {
    /// The solver's identifying conformance tier.
    fn tier(&self) -> SolverTier;
    /// The solver's implementation version.
    fn version(&self) -> SolverVersion;
    /// Solve from scratch.
    fn solve(&self, input: &ConstrainedLayoutIR, config: &SolverConfig) -> SolveReport;
    /// Solve incrementally over the declared invalidation scope. Must be
    /// observationally equivalent to [`ConstraintSolver::solve`] restricted to
    /// that scope (Chapter 9 §"Observational Equivalence").
    fn solve_incremental(
        &self,
        input: &ConstrainedLayoutIR,
        prior: &SolverState,
        invalidations: &InvalidationSet,
        config: &SolverConfig,
    ) -> SolveReport;
}

/// The v0 stub solver (QUICKSTART, Agent E: "the stub returns
/// `SolveStatus::Solved` with the input geometry verbatim").
///
/// It copies each glyph's baseline anchor into its resolved position unchanged,
/// preserves provenance, carries the engraving decisions and catalog forward,
/// and reports all hard constraints satisfied — provided every glyph's metrics
/// are bundled and the catalog hash actually covers the consulted metrics
/// (Chapter 7 §7.3.2). A glyph whose metrics are not bundled, or a catalog hash
/// that does not match its glyphs, is a well-formedness failure reported as
/// [`SolveStatus::InternalError`] (never a panic).
///
/// **Declared constraints are not evaluated** ([`SolverTier::Stub`]), and the
/// report is honest about it in both directions: the solve stays renderable
/// (geometry passes through; unevaluated constraints are not a defect in the
/// *input*), but `satisfied_hard_constraints` is `false` and a warning names
/// the gap — the stub never claims satisfaction it did not check. Chapter 9
/// has no status for "renderable, constraints unevaluated", so the closest
/// non-claiming renderable status, [`SolveStatus::SolvedWithWarnings`], is
/// used (see DECISIONS.md).
pub struct StubSolver;

impl StubSolver {
    fn resolve(&self, input: &ConstrainedLayoutIR) -> SolveReport {
        let structural_valid = input.validate().is_ok();
        // Short-circuit before catalog identity construction so an unknown glyph
        // yields InternalError rather than panicking in the metrics hash.
        let names: Vec<&str> = input
            .glyphs
            .iter()
            .map(|glyph| glyph.glyph.as_str())
            .collect();
        let metrics_available = all_available(names.iter().copied());
        let catalog_valid = metrics_available && input.catalog == BravuraCatalog.identity(&names);
        let well_formed = structural_valid && catalog_valid;
        // This interface-only solver can preserve already-resolved geometry but
        // does not evaluate explicit constraints. It must not claim those are
        // satisfied merely because the input is structurally well formed.
        let unevaluated = input.constraints.len();

        let status = if !well_formed {
            SolveStatus::InternalError
        } else if unevaluated > 0 {
            SolveStatus::SolvedWithWarnings
        } else {
            SolveStatus::Solved
        };
        let warnings = if well_formed && unevaluated > 0 {
            vec![SolverWarning {
                kind: SolverWarningKind::UnusualLayoutDecision(format!(
                    "the interface-only stub solver evaluated none of the {unevaluated} \
                     declared constraint(s); satisfaction is not claimed"
                )),
                affected_objects: Vec::new(),
                message: "declared constraints were not evaluated".to_owned(),
            }]
        } else {
            Vec::new()
        };

        let glyphs: Vec<ResolvedGlyph> = if structural_valid {
            input
                .glyphs
                .iter()
                .map(|g| ResolvedGlyph {
                    provenance: g.provenance.clone(),
                    glyph: g.glyph.clone(),
                    position: g.baseline,
                    transform: None,
                    bounding_box: g.bounding_box,
                    style: g.style,
                    layer: g.layer,
                })
                .collect()
        } else {
            Vec::new()
        };
        // Strokes and curves pass through verbatim (the stub resolves no
        // geometry), gated on the same structural validity as the glyphs.
        let strokes = if structural_valid {
            input.strokes.clone()
        } else {
            Vec::new()
        };
        let curves = if structural_valid {
            input.curves.clone()
        } else {
            Vec::new()
        };
        let resolved_glyphs = glyphs.len();
        let pages = input
            .regions
            .first()
            .map(|first| ResolvedPage {
                provenance: first.provenance.clone(),
                number: 1,
                size: Size2D::default(),
                margins: Margins::default(),
                systems: input
                    .regions
                    .iter()
                    .map(|region| ResolvedSystem {
                        provenance: region.provenance.clone(),
                        bounding_box: Rect::default(),
                        staves: Vec::new(),
                        measures: Vec::new(),
                    })
                    .collect(),
                free_objects: Vec::new(),
            })
            .into_iter()
            .collect();

        SolveReport {
            status,
            // Honest in both directions: false when the input is malformed *and*
            // when constraints were declared but not evaluated.
            satisfied_hard_constraints: well_formed && unevaluated == 0,
            layout: ResolvedLayoutIR {
                source: input.source,
                pages,
                glyphs,
                strokes,
                curves,
                engraving_decisions: input.engraving_decisions.clone(),
                catalog: input.catalog.clone(),
            },
            unsatisfied_constraints: Vec::new(),
            warnings,
            metric_vector: QualityMetricVector::unmeasured(),
            // The stub does no iterative work; its deterministic budget use is zero.
            budget_used: SolverBudgetUsed::default(),
            state: SolverState {
                solver_version: Some(self.version()),
                resolved_glyphs,
            },
        }
    }
}

impl ConstraintSolver for StubSolver {
    fn tier(&self) -> SolverTier {
        // Honest: a passthrough that evaluates no constraints is below Minimal.
        SolverTier::Stub
    }

    fn version(&self) -> SolverVersion {
        SolverVersion(0)
    }

    fn solve(&self, input: &ConstrainedLayoutIR, _config: &SolverConfig) -> SolveReport {
        self.resolve(input)
    }

    fn solve_incremental(
        &self,
        input: &ConstrainedLayoutIR,
        _prior: &SolverState,
        _invalidations: &InvalidationSet,
        _config: &SolverConfig,
    ) -> SolveReport {
        // The stub resolves geometry verbatim, so a full re-solve is trivially
        // observationally equivalent to any scoped incremental solve
        // (Chapter 9 §"Observational Equivalence").
        self.resolve(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constrained::GlyphObject;
    use crate::glyph::GlyphCatalogIdentity;
    use crate::provenance::{LayoutObjectId, Provenance};
    use crate::spatial::Point;
    use crate::vertical_band::{VerticalBand, VerticalBandId};
    use epiphany_core::{EventId, TypedObjectId, WallClockTime};

    fn glyph(name: &'static str) -> GlyphObject {
        let source = TypedObjectId::Event(EventId::from_raw(1));
        GlyphObject {
            provenance: Provenance::projected(source, vec![]),
            glyph: crate::GlyphReference::borrowed(name),
            horizontal_slot: SpringSlotId(0),
            baseline: Point::new(1.0, 0.0),
            vertical_band: VerticalBandId(0),
            bounding_box: crate::BoundingBox::default(),
            anchor: Point::ORIGIN,
            layer: 0,
            style: crate::GlyphStyle { rgba: 0x0000_00ff },
        }
    }

    fn constrained(mut glyphs: Vec<GlyphObject>) -> ConstrainedLayoutIR {
        let band = VerticalBand::margin(
            LayoutObjectId(0),
            glyphs.iter().map(GlyphObject::id).collect(),
        );
        for glyph in &mut glyphs {
            glyph.vertical_band = band.id;
        }
        let names: Vec<&str> = glyphs.iter().map(|glyph| glyph.glyph.as_str()).collect();
        let catalog = BravuraCatalog.identity(&names);
        ConstrainedLayoutIR {
            source: crate::ScoreVersion::default(),
            regions: vec![],
            horizontal_slots: vec![crate::SpringSlot {
                id: SpringSlotId(0),
                time: crate::TimePoint::WallClock(WallClockTime(0)),
                min_width: crate::StaffSpace(1.0),
                preferred_width: crate::StaffSpace(1.0),
                max_width: None,
                stretch_factor: 1.0,
                compress_factor: 1.0,
                members: glyphs.iter().map(GlyphObject::id).collect(),
            }],
            glyphs,
            strokes: vec![],
            curves: vec![],
            vertical_bands: vec![band],
            constraints: vec![],
            break_origins: vec![],
            engraving_decisions: vec![],
            diagnostics: vec![],
            catalog,
        }
    }

    #[test]
    fn stub_reports_the_non_conformant_stub_tier_and_worst_metrics() {
        // Honest non-conformance: a passthrough reports Stub, never Minimal, and
        // Stub orders below every real conformance tier.
        assert_eq!(StubSolver.tier(), SolverTier::Stub);
        assert!(SolverTier::Stub < SolverTier::Minimal);
        assert_eq!(StubSolver.version(), SolverVersion(0));
        let input = constrained(vec![glyph("noteheadBlack")]);
        assert_eq!(
            StubSolver
                .solve(&input, &SolverConfig::default())
                .metric_vector,
            QualityMetricVector::unmeasured()
        );
    }

    #[test]
    fn validate_rejects_dangling_constraint_references() {
        use crate::constrained::{
            BreakKind, ConstrainedValidationError, GlyphObjectId, LayoutConstraint,
        };
        let mut input = constrained(vec![glyph("noteheadBlack")]);
        assert!(input.validate().is_ok());
        let real = input.glyphs[0].id();

        // A constraint naming a glyph that is not in the set is rejected, not
        // silently accepted.
        let ghost = GlyphObjectId(real.0 ^ 0xABCD);
        input
            .constraints
            .push(LayoutConstraint::NoCollision { a: real, b: ghost });
        assert_eq!(
            input.validate(),
            Err(ConstrainedValidationError::UnknownConstraintGlyph(ghost))
        );

        // A break constraint on a non-existent slot is rejected.
        input.constraints = vec![LayoutConstraint::SystemBreakAt {
            slot: SpringSlotId(999),
            kind: BreakKind::Hard,
        }];
        assert_eq!(
            input.validate(),
            Err(ConstrainedValidationError::UnknownConstraintSlot(
                SpringSlotId(999)
            ))
        );

        // A well-formed constraint reference validates — and the stub solver
        // still does not *evaluate* it: the solve stays renderable, but it does
        // not claim the constraint satisfied.
        input.constraints = vec![LayoutConstraint::NoCollision { a: real, b: real }];
        assert!(input.validate().is_ok());
        let report = StubSolver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::SolvedWithWarnings);
        assert!(!report.satisfied_hard_constraints);
    }

    #[test]
    fn unknown_glyph_yields_internal_error_not_panic() {
        let mut unknown = glyph("noSuchGlyph");
        let band = VerticalBand::margin(LayoutObjectId(0), vec![unknown.id()]);
        unknown.vertical_band = band.id;
        let input = ConstrainedLayoutIR {
            source: crate::ScoreVersion::default(),
            regions: vec![],
            horizontal_slots: vec![crate::SpringSlot {
                id: SpringSlotId(0),
                time: crate::TimePoint::WallClock(WallClockTime(0)),
                min_width: crate::StaffSpace(1.0),
                preferred_width: crate::StaffSpace(1.0),
                max_width: None,
                stretch_factor: 1.0,
                compress_factor: 1.0,
                members: vec![unknown.id()],
            }],
            glyphs: vec![unknown],
            strokes: vec![],
            curves: vec![],
            vertical_bands: vec![band],
            constraints: vec![],
            break_origins: vec![],
            engraving_decisions: vec![],
            diagnostics: vec![],
            catalog: GlyphCatalogIdentity::default(),
        };
        let report = StubSolver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::InternalError);
        assert!(!report.satisfied_hard_constraints);
    }

    #[test]
    fn well_formed_input_solves_verbatim() {
        let input = constrained(vec![glyph("noteheadBlack")]);
        let report = StubSolver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved);
        assert!(report.satisfied_hard_constraints);
        assert_eq!(report.layout.glyphs[0].position, input.glyphs[0].baseline);
        assert_eq!(report.state.resolved_glyphs, 1);
        assert!(report.unsatisfied_constraints.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn strokes_survive_the_solve_and_enter_the_canonical_bytes() {
        let mut input = constrained(vec![glyph("noteheadBlack")]);
        let baseline = StubSolver
            .solve(&input, &SolverConfig::default())
            .layout
            .canonical_bytes();
        input.strokes.push(crate::Stroke {
            provenance: input.glyphs[0].provenance.clone(),
            vertical_band: input.glyphs[0].vertical_band,
            from: crate::Point::new(0.0, 0.0),
            to: crate::Point::new(1.5, 0.0),
            thickness: crate::StaffSpace(0.13),
            layer: 0,
            style: crate::GlyphStyle::default(),
        });
        let solved = StubSolver.solve(&input, &SolverConfig::default());
        assert_eq!(
            solved.layout.strokes.len(),
            1,
            "the stroke survives the solve"
        );
        assert_ne!(
            solved.layout.canonical_bytes(),
            baseline,
            "a stroke changes the resolved canonical bytes"
        );
    }

    #[test]
    fn forged_catalog_metadata_is_rejected() {
        let mut input = constrained(vec![glyph("noteheadBlack")]);
        input.catalog.font_id = crate::glyph::FontId::owned("Not Bravura");
        let report = StubSolver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::InternalError);
        assert!(!report.satisfied_hard_constraints);

        let mut input = constrained(vec![glyph("noteheadBlack")]);
        input.catalog.smufl_version.minor += 1;
        assert_eq!(
            StubSolver.solve(&input, &SolverConfig::default()).status,
            SolveStatus::InternalError
        );

        let mut input = constrained(vec![glyph("noteheadBlack")]);
        input.catalog.font_version = None;
        assert_eq!(
            StubSolver.solve(&input, &SolverConfig::default()).status,
            SolveStatus::InternalError
        );
    }

    #[test]
    fn dangling_band_and_non_finite_geometry_are_rejected() {
        let mut dangling = constrained(vec![glyph("noteheadBlack")]);
        dangling.vertical_bands.clear();
        assert_eq!(
            StubSolver.solve(&dangling, &SolverConfig::default()).status,
            SolveStatus::InternalError
        );

        let mut non_finite = constrained(vec![glyph("noteheadBlack")]);
        non_finite.glyphs[0].baseline = Point::new(f32::NAN, 0.0);
        let report = StubSolver.solve(&non_finite, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::InternalError);
        assert!(report.layout.glyphs.is_empty());
    }

    #[test]
    fn explicit_constraints_are_not_falsely_reported_satisfied() {
        let mut input = constrained(vec![glyph("noteheadBlack")]);
        let glyph = input.glyphs[0].id();
        input
            .constraints
            .push(crate::LayoutConstraint::NoCollision { a: glyph, b: glyph });
        let report = StubSolver.solve(&input, &SolverConfig::default());
        // Unevaluated constraints are not a defect in the input, so the solve is
        // renderable — but satisfaction is not claimed, and a warning names the gap.
        assert_eq!(report.status, SolveStatus::SolvedWithWarnings);
        assert!(report.status.is_renderable());
        assert!(!report.satisfied_hard_constraints);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.unsatisfied_constraints.is_empty());
        // The geometry still passes through verbatim.
        assert_eq!(report.layout.glyphs[0].position, input.glyphs[0].baseline);
    }

    #[test]
    fn incremental_is_observationally_equivalent_to_full() {
        let input = constrained(vec![glyph("noteheadBlack"), glyph("gClef")]);
        let full = StubSolver.solve(&input, &SolverConfig::default());
        let inc = StubSolver.solve_incremental(
            &input,
            &full.state,
            &InvalidationSet {
                scope: InvalidationScope::WholeScore,
                slots: vec![],
                bands: vec![],
                constraints: vec![],
                glyphs: vec![],
            },
            &SolverConfig::default(),
        );
        assert_eq!(full.layout, inc.layout);
    }

    #[test]
    fn normalized_metric_accepts_its_range() {
        assert_eq!(NormalizedMetric::new(0.0), NormalizedMetric(0.0));
        assert_eq!(NormalizedMetric::new(1.0), NormalizedMetric(1.0));
    }

    #[test]
    #[should_panic(expected = "[0.0, 1.0]")]
    fn normalized_metric_rejects_out_of_range() {
        let _ = NormalizedMetric::new(1.5);
    }
}
