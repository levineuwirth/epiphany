#![forbid(unsafe_code)]
//! # epiphany-engrave
//!
//! Agent I's **engraving constraint solver** (spec **Chapter 9**, "Constraint-
//! Solver Interface"): it turns a [`ConstrainedLayoutIR`] into a
//! [`ResolvedLayoutIR`] with real geometry. It is the production-side replacement
//! for `epiphany-layout-ir`'s interface-only [`StubSolver`] — the QUICKSTART puts
//! the *interface* (`layout-ir`) and the *algorithm* (`engrave`) in separate
//! crates so the core/product boundary stays sharp (`spec/PHASE2_QUICKSTART.md`,
//! crate topology).
//!
//! ## Phase status — honest scaffold, not yet `Minimal`
//!
//! Per the QUICKSTART's recommended development pattern, Agent I develops the
//! renderer ([`epiphany-render-svg`]) against the **stub solver** first, then
//! grows this crate into the real two-pass spring solver. This is the first
//! increment: [`Engraver`] runs a genuine deterministic **horizontal spacing
//! pass** (see [`spacing`]) — the first axis of the planned two-pass spring
//! layout — placing each spring slot left-to-right by its preferred width
//! instead of returning the input columns verbatim.
//!
//! It does **not yet** run the vertical spring pass, the soft-constraint
//! stretch/compress solve, or evaluate the IR's declared hard constraints. By the
//! same honesty rule `layout-ir`'s [`StubSolver`] follows, a solver that does not
//! evaluate the declared constraints and computes no quality metrics MUST report
//! [`SolverTier::Stub`], never [`SolverTier::Minimal`] (Chapter 9 §"Conformance
//! Tiers"). So [`Engraver::tier`] reports `Stub` today; it is promoted to
//! `Minimal` in the same change that lands real constraint satisfaction. The
//! quality-metric vector is the conservative all-worst placeholder
//! ([`QualityMetricVector::unmeasured`]) until the Quality Metric Catalog lands
//! (Phase 3).
//!
//! ## Architecture decision (see `DECISIONS.md`)
//!
//! The solver is a **two-pass spring layout** (horizontal then vertical), with
//! the constraint graph derived from the existing [`ConstrainedLayoutIR`]
//! (QUICKSTART decision 2). A global optimization solver is rejected: the spec's
//! deterministic-output requirement makes it expensive to validate.
//!
//! [`epiphany-render-svg`]: ../epiphany_render_svg/index.html

mod spacing;

use std::collections::BTreeMap;

use epiphany_layout_ir::{
    all_available, BravuraCatalog, ConstrainedLayoutIR, ConstraintSolver, GlyphCatalog,
    InvalidationSet, Margins, QualityMetricVector, Rect, ResolvedGlyph, ResolvedLayoutIR,
    ResolvedPage, ResolvedSystem, Size2D, SolveReport, SolveStatus, SolverBudgetUsed, SolverConfig,
    SolverState, SolverTier, SolverVersion, SolverWarning, SolverWarningKind,
};

/// The Epiphany engraving solver (Chapter 9). See the crate docs for the phase
/// status: this is the horizontal-spacing scaffold of the planned two-pass
/// spring layout, reporting [`SolverTier::Stub`] until it earns `Minimal`.
#[derive(Copy, Clone, Debug, Default)]
pub struct Engraver;

/// The implementation version of this solver (Chapter 9: within a fixed version,
/// identical input produces identical output). Distinct from the stub's `0`.
pub const ENGRAVER_VERSION: SolverVersion = SolverVersion(1);

impl Engraver {
    /// Resolves geometry: a deterministic horizontal spacing pass over the spring
    /// slots, copying each glyph to its slot's `x` with its baseline `y`
    /// preserved. Well-formedness is gated exactly as the stub's is — an unknown
    /// glyph, a forged catalog identity, malformed structure, or an explicit
    /// constraint this scaffold cannot yet evaluate yields
    /// [`SolveStatus::InternalError`] (a diagnostic-only layout), never a panic.
    fn resolve(&self, input: &ConstrainedLayoutIR) -> SolveReport {
        let structural_valid = input.validate().is_ok();

        // Short-circuit before catalog construction so an unknown glyph yields a
        // diagnostic, not a panic in the metrics hash (mirrors the stub).
        let names: Vec<&str> = input.glyphs.iter().map(|g| g.glyph.as_str()).collect();
        let metrics_available = all_available(names.iter().copied());
        let catalog_valid = metrics_available && input.catalog == BravuraCatalog.identity(&names);

        // This scaffold does not yet evaluate explicit hard constraints, so — like
        // the stub — it must not claim them satisfied. An input carrying explicit
        // constraints is reported as not-yet-solvable rather than falsely Solved.
        let well_formed = structural_valid && catalog_valid && input.constraints.is_empty();

        let glyphs: Vec<ResolvedGlyph> = if structural_valid {
            let positions = spacing::slot_positions(input);
            place_glyphs(input, &positions)
        } else {
            Vec::new()
        };
        let resolved_glyphs = glyphs.len();

        let mut warnings = Vec::new();
        if structural_valid && catalog_valid && !input.constraints.is_empty() {
            warnings.push(SolverWarning {
                kind: SolverWarningKind::UnusualLayoutDecision(
                    "explicit hard-constraint evaluation is not implemented in the \
                     horizontal-spacing scaffold; it lands with the Minimal tier"
                        .to_owned(),
                ),
                affected_objects: Vec::new(),
                message: "engrave scaffold cannot yet evaluate declared constraints".to_owned(),
            });
        }

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
            status: if well_formed {
                SolveStatus::Solved
            } else {
                SolveStatus::InternalError
            },
            satisfied_hard_constraints: well_formed,
            layout: ResolvedLayoutIR {
                source: input.source,
                pages,
                glyphs,
                engraving_decisions: input.engraving_decisions.clone(),
                catalog: input.catalog.clone(),
            },
            unsatisfied_constraints: Vec::new(),
            warnings,
            // No normalized quality metrics computed yet (Stub tier, Phase 3 work).
            metric_vector: QualityMetricVector::unmeasured(),
            budget_used: SolverBudgetUsed {
                // The horizontal pass touches each slot once; report that honestly.
                iterations: input.horizontal_slots.len() as u64,
                nodes: resolved_glyphs as u64,
                constraint_evaluations: 0,
                wall_time_ms: 0,
            },
            state: SolverState {
                solver_version: Some(self.version()),
                resolved_glyphs,
            },
        }
    }
}

/// Copies each glyph to the `x` of its horizontal slot (its baseline `y`
/// preserved), preserving provenance, glyph identity, bounds, style, and layer.
/// A glyph whose slot has no computed position keeps its baseline `x`.
fn place_glyphs(
    input: &ConstrainedLayoutIR,
    positions: &BTreeMap<epiphany_layout_ir::SpringSlotId, f32>,
) -> Vec<ResolvedGlyph> {
    input
        .glyphs
        .iter()
        .map(|g| {
            let x = positions
                .get(&g.horizontal_slot)
                .copied()
                .unwrap_or(g.baseline.x.0);
            ResolvedGlyph {
                provenance: g.provenance.clone(),
                glyph: g.glyph.clone(),
                position: epiphany_layout_ir::Point::new(x, g.baseline.y.0),
                transform: None,
                bounding_box: g.bounding_box,
                style: g.style,
                layer: g.layer,
            }
        })
        .collect()
}

impl ConstraintSolver for Engraver {
    fn tier(&self) -> SolverTier {
        // Honest: a solver that does not evaluate the declared hard constraints
        // and computes no quality metrics is below Minimal (Chapter 9). Promoted
        // to `Minimal` in the change that lands real constraint satisfaction.
        SolverTier::Stub
    }

    fn version(&self) -> SolverVersion {
        ENGRAVER_VERSION
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
        // The scaffold recomputes spacing from scratch, which is trivially
        // observationally equivalent to a scoped incremental solve (Chapter 9
        // §"Observational Equivalence"). Real incremental scoping is Minimal-tier
        // work.
        self.resolve(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{to_constrained, to_logical, StubSolver};

    fn fixture() -> ConstrainedLayoutIR {
        to_constrained(&to_logical(&valid_score_rich(11)))
    }

    #[test]
    fn reports_the_honest_stub_tier_until_it_earns_minimal() {
        // The crate exists to *become* the Minimal solver, but until it evaluates
        // the declared constraints it reports Stub — never a tier it has not
        // earned. This guards the honesty invariant.
        assert_eq!(Engraver.tier(), SolverTier::Stub);
        assert!(Engraver.tier() < SolverTier::Minimal);
        assert_eq!(Engraver.version(), ENGRAVER_VERSION);
        assert_ne!(Engraver.version(), StubSolver.version());
    }

    #[test]
    fn solves_the_stub_pipeline_and_preserves_provenance() {
        let input = fixture();
        let report = Engraver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved);
        assert!(report.satisfied_hard_constraints);
        assert_eq!(report.layout.glyphs.len(), input.glyphs.len());
        // Every input glyph's provenance survives, one-for-one.
        for (resolved, original) in report.layout.glyphs.iter().zip(&input.glyphs) {
            assert_eq!(resolved.provenance, original.provenance);
            assert_eq!(resolved.glyph, original.glyph);
        }
        // The metric vector is the honest all-worst placeholder (no metrics yet).
        assert_eq!(report.metric_vector, QualityMetricVector::unmeasured());
    }

    #[test]
    fn horizontal_spacing_differs_from_the_verbatim_stub() {
        // The whole point of the scaffold: it re-spaces horizontally rather than
        // echoing the input columns. With multiple glyphs the positions differ
        // from the stub's verbatim baselines.
        let input = fixture();
        assert!(input.glyphs.len() >= 2);
        let engraved = Engraver.solve(&input, &SolverConfig::default()).layout;
        let stub = StubSolver.solve(&input, &SolverConfig::default()).layout;
        assert_ne!(
            engraved
                .glyphs
                .iter()
                .map(|g| g.position.x.0)
                .collect::<Vec<_>>(),
            stub.glyphs
                .iter()
                .map(|g| g.position.x.0)
                .collect::<Vec<_>>(),
            "the engrave pass must re-space, not echo the stub's columns"
        );
        // ...but it preserves the same glyph set and order.
        assert_eq!(engraved.glyphs.len(), stub.glyphs.len());
    }

    #[test]
    fn solve_is_deterministic_and_quantizable() {
        let input = fixture();
        let a = Engraver.solve(&input, &SolverConfig::default()).layout;
        let b = Engraver.solve(&input, &SolverConfig::default()).layout;
        // Byte-identical canonical output across solves (Chapter 9 determinism).
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    }

    #[test]
    fn incremental_is_observationally_equivalent_to_full() {
        let input = fixture();
        let full = Engraver.solve(&input, &SolverConfig::default());
        let inc = Engraver.solve_incremental(
            &input,
            &full.state,
            &InvalidationSet {
                scope: epiphany_layout_ir::InvalidationScope::WholeScore,
                slots: vec![],
                bands: vec![],
                constraints: vec![],
                glyphs: vec![],
            },
            &SolverConfig::default(),
        );
        assert_eq!(full.layout, inc.layout);
    }
}
