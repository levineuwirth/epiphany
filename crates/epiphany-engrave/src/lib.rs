#![forbid(unsafe_code)]
//! # epiphany-engrave
//!
//! Agent I's **engraving constraint solver** (spec **Chapter 9**, "Constraint-
//! Solver Interface"): it turns a [`ConstrainedLayoutIR`] into a
//! [`ResolvedLayoutIR`] with real geometry. It is the production-side replacement
//! for `epiphany-layout-ir`'s interface-only `StubSolver` — the QUICKSTART puts
//! the *interface* (`layout-ir`) and the *algorithm* (`engrave`) in separate
//! crates so the core/product boundary stays sharp (`spec/PHASE2_QUICKSTART.md`,
//! crate topology).
//!
//! ## Phase status — `Minimal` tier, with casting-off
//!
//! [`Engraver`] runs a genuine deterministic **horizontal spacing pass** (the
//! private `spacing` module) — placing each glyph-bearing slot left-to-right by
//! a collision-aware advance (its preferred width floored by the real glyph
//! bearings) — then a **casting-off pass** (the [`casting`] module; Chapter 9
//! §"The Constraint-Solving Stage": the solver "resolve\[s\] page and system
//! breaks"): greedy first-fit system breaking at measure boundaries against a
//! [`PageGeometry`], a widow-rebalance phase that evens a region's system widths
//! so the final system is not left a stub, vertical system stacking at the
//! vertical-band model's inter-system gap, page assignment by content height,
//! and a real populated page/system tree (Chapter 7 §"ResolvedLayoutIR"). Every
//! chosen break is
//! recorded as an [`epiphany_layout_ir::EngravingDecision`] whose target is a
//! `MUSCLOID` id synthesized under
//! [`epiphany_layout_ir::SynthesisKind::EngravedBreak`], attributed to the user
//! override that asked for it when one did
//! ([`epiphany_layout_ir::DecisionSource::UserOverride`]).
//!
//! The declared constraints are **evaluated**, routed by
//! [`LayoutConstraint::strength`] (Chapter 9 §"Strength Levels"). Geometric
//! constraints (no-collision, alignment, position-within) are evaluated in the
//! **pre-casting spaced frame** — they are region-frame obligations, and
//! casting-off relocates whole systems by rigid motions that cannot un-satisfy
//! them within a system (see `DECISIONS.md`). Break constraints are evaluated
//! against the **final break structure**: a `SystemBreakAt`/`PageBreakAt` is
//! satisfied iff the cast layout breaks (starts a system/page) at that slot. A
//! violated `Required` constraint makes the solve
//! [`SolveStatus::Unsatisfiable`]; a violated `Preferred` one (a soft break
//! skipped on the documented pathological path) surfaces as a
//! [`SolverWarningKind::LargeSoftConstraintViolation`] warning under
//! [`SolveStatus::SolvedWithWarnings`] plus an `IrOverride`-sourced decision,
//! never a failure. A solve is [`SolveStatus::Solved`] only when every declared
//! constraint holds.
//!
//! Having earned it, [`Engraver::tier`] reports [`SolverTier::Minimal`] — which
//! (Chapter 9 §"Conformance Tiers" / QUICKSTART) means *hard constraints
//! satisfied, no claim about optimality* — greedy first-fit casting-off is
//! legitimate at this tier. The solve reports a **real quality-metric vector**:
//! the private `quality` module computes all nine normative axes per the
//! ratified *Quality Metric Catalog* companion (collision census, spacing
//! regularity, break/page/casting-off distribution, vertical gap deviation;
//! `slur_shape` is now **measured** — each drawn slur's arc ratio against the
//! shallow-arc band — while `beam_slope` stays vacuous-`0.0` because no drawn
//! beam geometry exists yet), normalized through the catalog's pinned anchors
//! ([`epiphany_layout_ir::quality`]), with
//! [`SolverWarningKind::QualityFloorApproached`] diagnostics against the
//! threshold column the config's profile selects. The all-worst
//! [`QualityMetricVector::unmeasured`] placeholder remains only for malformed
//! inputs the solver cannot measure. Still deferred to a later tier: the
//! **vertical spring pass** (glyph `y` within a system is the constrained
//! natural staff layout, preserved verbatim; systems stack by real content
//! extents), per-system justification/stretch, and optimal break search.
//!
//! ## Architecture decision (see `DECISIONS.md`)
//!
//! The solver is a **two-pass spring layout** (horizontal then vertical), with
//! the constraint graph derived from the existing [`ConstrainedLayoutIR`]
//! (QUICKSTART decision 2). A global optimization solver is rejected: the spec's
//! deterministic-output requirement makes it expensive to validate.
//!
//! [`epiphany-render-svg`]: ../epiphany_render_svg/index.html

pub mod casting;
mod quality;
mod spacing;

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::TypedObjectId;
use epiphany_layout_ir::{
    all_available, profile_thresholds, Axis, BravuraCatalog, ConstrainedLayoutIR, ConstraintId,
    ConstraintSolver, ConstraintStrength, Curve, GlyphCatalog, GlyphObject, GlyphObjectId,
    InvalidationSet, LayoutConstraint, Point, QualityMetricVector, Rect, ResolvedGlyph,
    ResolvedLayoutIR, SolveReport, SolveStatus, SolverBudgetUsed, SolverConfig, SolverState,
    SolverTier, SolverVersion, SolverWarning, SolverWarningKind, SpringSlotId, Stroke,
};

pub use casting::{PageGeometry, INTER_PAGE_GAP, SYSTEM_CONTINUATION_SYNTHESIS};

/// The glyph a fixed-width stroke (a ledger line) belongs to: the same-source glyph
/// whose baseline falls within the stroke's horizontal span (its accidentals sit
/// outside the span, to the left). The stroke is anchored to this glyph's column so
/// it translates with it — found by source, never inferred from the stroke's own
/// midpoint, which for a wide head can fall nearer a neighbouring column.
pub(crate) fn owning_glyph<'a>(
    stroke: &Stroke,
    glyphs: &'a [GlyphObject],
) -> Option<&'a GlyphObject> {
    let lo = stroke.from.x.0.min(stroke.to.x.0);
    let hi = stroke.from.x.0.max(stroke.to.x.0);
    glyphs.iter().find(|g| {
        g.provenance.source == stroke.provenance.source
            && g.baseline.x.0 >= lo
            && g.baseline.x.0 <= hi
    })
}

/// The glyph a per-event COMPONENT stroke belongs to — the notehead that shares
/// its `Event` source — found by source ALONE, so it also catches a **stem**,
/// which sits offset from its column (at `notehead_x + stem_offset`) and whose
/// x therefore contains no glyph baseline (`owning_glyph`'s x-span test misses
/// it). Such a stroke tracks its notehead's slot rigidly, so re-spacing and
/// justification move it *with* its head instead of stretching its offset. A
/// stroke whose source is not an `Event` — a staff line (`Staff`), a volta
/// bracket (a `RepeatStructure`, a source its ending-number glyphs also carry) —
/// has no same-slot owner here and stretches with its system instead. (Every
/// event-sourced stroke today is a single-slot component: stem, ledger, or a
/// zero-extent anchor. A future event-spanning stroke — a beam — would need a
/// span-aware guard added here.)
pub(crate) fn component_glyph<'a>(
    stroke: &Stroke,
    glyphs: &'a [GlyphObject],
) -> Option<&'a GlyphObject> {
    // Spanning strokes stretch with their system, so they own no single slot: a
    // staff line (`Staff` source), a volta bracket (`RepeatStructure` source —
    // which its ending-number glyphs also carry, so a plain source match would
    // wrongly anchor the whole bracket to one number's slot).
    if matches!(
        stroke.provenance.source,
        TypedObjectId::Staff(_) | TypedObjectId::RepeatStructure(_)
    ) {
        return None;
    }
    // A ledger shares its notehead's `Pitch` source and overlaps it in x, so
    // `owning_glyph` finds it directly.
    if let Some(g) = owning_glyph(stroke, glyphs) {
        return Some(g);
    }
    // A stem is `Event`-sourced with NO same-source glyph (noteheads are
    // `Pitch`-sourced) and sits offset from its column (`stem_x = notehead_x +
    // 1.15`, inside the 1.6 column step), so its x contains no glyph baseline.
    // It belongs to the slot just to its LEFT — the glyph with the greatest
    // baseline ≤ its x, which is a notehead or dot in the stem's OWN column (all
    // of that column's glyphs share the one slot, so the pick's slot is exact).
    let x = stroke.from.x.0.max(stroke.to.x.0);
    glyphs
        .iter()
        .filter(|g| g.baseline.x.0 <= x + f32::EPSILON)
        .max_by(|a, b| a.baseline.x.0.total_cmp(&b.baseline.x.0))
}

/// The Epiphany engraving solver (Chapter 9). A `Minimal`-tier solver: it spaces
/// glyphs horizontally, casts the result off into systems and pages against its
/// [`PageGeometry`], and satisfies the IR's declared hard constraints — break
/// constraints included. See the crate docs for what each tier claims and what
/// remains deferred.
#[derive(Copy, Clone, Debug, Default)]
pub struct Engraver {
    geometry: PageGeometry,
}

/// The implementation version of this solver (Chapter 9: within a fixed version,
/// identical input produces identical output). Distinct from the stub's `0`;
/// bumped to `2` when the casting-off pass landed (the resolved geometry of a
/// wrapping score differs from version `1`'s single endless system), to `3`
/// when casting-off gained its widow-rebalance phase (a wrapping score's system
/// breaks — and so its baked geometry — differ again from version `2`'s pure
/// greedy first-fit), to `4` when repeat barlines and volta brackets landed
/// and the horizontal remap became **slot-relative** (a repeat-bearing score's
/// baked geometry differs from version `3`'s invisible traced anchors, and a
/// same-slot companion glyph — a time-signature digit, key-signature
/// accidental, or spelling accidental — now rides its slot's rigid delta
/// instead of drifting by interpolation; scores without such companions are
/// unchanged), and to `5` when slur curves landed (a slur-bearing score gains
/// a drawn cubic-bézier curve where version `4` had only a traced anchor; the
/// resolved output now carries a third primitive kind, so its canonical bytes
/// differ; slur-free scores draw the same ink as before), and to `6` when slur
/// curves gained a line pattern (a dashed or dotted slur renders its authored
/// `SpanStyle.line` faithfully; a curve's canonical bytes now include its line
/// style; solid slurs and slur-free scores are unchanged), and to `7` when a
/// slur spanning a system break began splitting into per-system sub-curves (de
/// Casteljau) instead of drawing whole in its start system; a slur that fits in
/// one system is unchanged, and to `8` when **per-system justification** landed
/// (every non-final system of a multi-system region stretches its horizontal
/// slack so its ink fills the content width, instead of sitting at its natural
/// left-aligned width) **together with correct component-stroke slot-anchoring**
/// (a stem — an `Event`-sourced stroke offset from its column, which the old
/// ledger-only rigid-width test missed — now rides its notehead's slot in both
/// the spacing pass and casting, instead of being stretched by the interpolation
/// / justification map and drifting off its head; any wrapping score's baked
/// geometry differs, and any score with drawn stems shifts them onto their
/// heads), and to `9` when **vertical justification** landed (the systems of a
/// non-final page spread so the last one's bottom reaches the content bottom,
/// filling the page height; a multi-page score's baked geometry differs, while
/// a single-page score — whose only page is ragged-bottom by convention — is
/// unchanged), and to `10` when casting-off replaced greedy first-fit + widow
/// rebalance with an **optimal (badness-minimizing) break search** (a wrapping
/// score's measures partition into more balanced systems — e.g. a 5/4 split
/// where greedy left a fuller-then-stub one — so its system breaks, and every
/// geometry that flows from them, differ; a score that does not wrap is
/// unchanged).
pub const ENGRAVER_VERSION: SolverVersion = SolverVersion(10);

impl Engraver {
    /// An engraver casting off against the given page geometry.
    /// [`Engraver::default`] uses [`PageGeometry::default`] (A4 portrait at an
    /// 8 mm staff — see its docs for the arithmetic).
    pub fn with_geometry(geometry: PageGeometry) -> Self {
        Engraver { geometry }
    }

    /// The page geometry this engraver casts off against.
    pub fn geometry(&self) -> PageGeometry {
        self.geometry
    }

    /// Resolves geometry: a deterministic horizontal spacing pass over the spring
    /// slots (each glyph to its slot's `x`, baseline `y` preserved), then the
    /// casting-off pass (system breaking, vertical stacking, page assignment —
    /// see [`casting`]), then evaluation of the declared constraints by
    /// strength, then the **quality-metric census** (the private `quality`
    /// module): all nine normative axes of the Quality Metric Catalog computed
    /// over the cast geometry, with `QualityFloorApproached` warnings against
    /// the threshold column the config's profile selects (diagnostic — per the
    /// catalog they never change the status). A malformed input — an unknown
    /// glyph, a forged catalog identity, or invalid structure — yields
    /// [`SolveStatus::InternalError`] with the all-worst unmeasured vector
    /// (nothing trustworthy to measure); a valid problem whose `Required`
    /// constraints cannot all be satisfied yields
    /// [`SolveStatus::Unsatisfiable`] (naming the unsatisfied constraints), its
    /// real geometry measured honestly. Neither panics. Violated `Preferred`
    /// constraints yield soft-violation warnings under
    /// [`SolveStatus::SolvedWithWarnings`] — a valid, renderable layout.
    fn resolve(&self, input: &ConstrainedLayoutIR, config: &SolverConfig) -> SolveReport {
        let structural_valid = input.validate().is_ok();

        // Short-circuit before catalog construction so an unknown glyph yields a
        // diagnostic, not a panic in the metrics hash (mirrors the stub).
        let names: Vec<&str> = input.glyphs.iter().map(|g| g.glyph.as_str()).collect();
        let metrics_available = all_available(names.iter().copied());
        let catalog_valid = metrics_available && input.catalog == BravuraCatalog.identity(&names);

        // The horizontal spacing pass re-places each glyph by its spring slot.
        // The strokes that track those glyphs (stems, staff lines, barlines) must
        // ride the *same* horizontal map, or a re-spaced notehead would leave its
        // stem behind. Both gate on structural validity: a malformed input must
        // not leak geometry into the diagnostic layout (which reaches
        // canonical_bytes / the renderer).
        let (spaced_glyphs, spaced_strokes, spaced_curves): (
            Vec<ResolvedGlyph>,
            Vec<Stroke>,
            Vec<Curve>,
        ) = if structural_valid {
            let remap = HorizontalRemap::build(input);
            (
                remap.glyphs(input),
                remap.strokes(input),
                remap.curves(input),
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        // Casting-off: break the spaced line into systems, stack them, assign
        // pages, and bake every position into the single world frame. Pure
        // geometry, so it runs whenever the structure is trustworthy (the
        // catalog gate below only guards constraint *evaluation*).
        let cast = if structural_valid {
            Some(casting::cast_off(
                input,
                &spaced_glyphs,
                &spaced_strokes,
                &spaced_curves,
                &self.geometry,
            ))
        } else {
            None
        };
        let resolved_glyphs = spaced_glyphs.len();

        // Evaluate every declared constraint, routed by its strength (Chapter 9
        // §"Strength Levels"): geometric constraints against the *pre-casting*
        // spaced geometry (their frame of expression — casting-off relocates
        // whole systems rigidly), break constraints against the final break
        // structure. A violated `Required` constraint is unsatisfied (the solve
        // is `Unsatisfiable`); a violated `Preferred` one is a soft-violation
        // *warning*, never a failure. A structurally invalid or bad-catalog
        // input is not evaluated — there is no trustworthy geometry — so it
        // reports no evaluation work.
        let (evaluation, constraints_evaluated) = if structural_valid && catalog_valid {
            let cast = cast
                .as_ref()
                .expect("casting ran on structurally valid input");
            (
                evaluate_constraints(
                    &input.constraints,
                    &spaced_glyphs,
                    &BreakOutcome {
                        system_starts: &cast.system_start_slots,
                        page_starts: &cast.page_start_slots,
                    },
                ),
                input.constraints.len() as u64,
            )
        } else {
            (ConstraintEvaluation::not_evaluated(), 0)
        };
        let ConstraintEvaluation {
            required_satisfied,
            unsatisfied: unsatisfied_constraints,
            soft_violations,
        } = evaluation;
        let well_formed = structural_valid && catalog_valid && required_satisfied;
        // Distinguish a *malformed/unusable* input (InternalError — a structural or
        // catalog defect the solver cannot proceed past) from a *valid problem
        // whose declared hard constraints cannot all be satisfied* (Unsatisfiable),
        // per the solver-report contract (Chapter 9 §"The Solver Report"). Hard
        // constraints all satisfied but soft ones violated is a valid layout
        // worth flagging: SolvedWithWarnings.
        let status = if !structural_valid || !catalog_valid {
            SolveStatus::InternalError
        } else if !required_satisfied {
            SolveStatus::Unsatisfiable
        } else if !soft_violations.is_empty() {
            SolveStatus::SolvedWithWarnings
        } else {
            SolveStatus::Solved
        };

        let mut warnings = soft_violations;
        if structural_valid && catalog_valid && !unsatisfied_constraints.is_empty() {
            warnings.push(SolverWarning {
                kind: SolverWarningKind::UnusualLayoutDecision(
                    "one or more declared hard constraints are not satisfied by this \
                     Minimal solve (e.g. an unverifiable extension constraint, or \
                     colliding geometry the spacing pass cannot separate); see \
                     unsatisfied_constraints"
                        .to_owned(),
                ),
                affected_objects: Vec::new(),
                message: "declared hard constraints unsatisfied".to_owned(),
            });
        }

        // The quality-metric census (Quality Metric Catalog): measured whenever
        // the geometry is trustworthy — structure valid (the cast ran) and the
        // catalog identity genuine (the glyph boxes the census sweeps are the
        // real bundled metrics). A malformed input keeps the all-worst
        // unmeasured placeholder: there is nothing honest to measure. The
        // floor warnings reference the threshold column the config's profile
        // selects (Draft -> Minimal, Standard/Publication -> Standard); per the
        // catalog they are diagnostic and never change `status`, which was
        // fixed above.
        let metric_vector = match (&cast, catalog_valid) {
            (Some(cast), true) => {
                let vector = quality::measure(input, cast, &spaced_curves, &self.geometry);
                warnings.extend(quality::floor_warnings(
                    &vector,
                    profile_thresholds(config.profile),
                ));
                vector
            }
            _ => QualityMetricVector::unmeasured(),
        };

        // The final layout is the cast world frame: real pages and systems,
        // glyph/stroke positions baked, the engraver's break decisions appended
        // to the pipeline's (Chapter 7 §"ResolvedLayoutIR": decisions "including
        // any the solver itself made").
        let (glyphs, strokes, curves, pages, engraving_decisions) = match cast {
            Some(cast) => {
                let mut decisions = input.engraving_decisions.clone();
                decisions.extend(cast.decisions);
                (
                    cast.glyphs,
                    cast.strokes,
                    cast.curves,
                    cast.pages,
                    decisions,
                )
            }
            None => (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                input.engraving_decisions.clone(),
            ),
        };

        SolveReport {
            status,
            satisfied_hard_constraints: well_formed,
            layout: ResolvedLayoutIR {
                source: input.source,
                pages,
                glyphs,
                strokes,
                curves,
                engraving_decisions,
                catalog: input.catalog.clone(),
            },
            unsatisfied_constraints,
            warnings,
            // The real nine-axis census computed above (or the honest all-worst
            // placeholder for a malformed input the solver could not measure).
            metric_vector,
            budget_used: SolverBudgetUsed {
                // The horizontal pass and the casting-off walk each touch every
                // slot once; report the spacing pass's touch honestly.
                iterations: input.horizontal_slots.len() as u64,
                nodes: resolved_glyphs as u64,
                constraint_evaluations: constraints_evaluated,
                wall_time_ms: 0,
            },
            state: SolverState {
                solver_version: Some(self.version()),
                resolved_glyphs,
            },
        }
    }
}

/// A monotonic piecewise-linear map from a constrained x to its spaced x. Each
/// column's *source* x (the baseline its member glyphs share) maps to the
/// *target* x the spacing pass assigns its slot; intermediate and outlying
/// coordinates interpolate/extrapolate linearly. Applied to glyph baselines
/// **and** stroke endpoints alike, so a stroke at (or near) a glyph's column
/// moves with it instead of detaching — the fix for strokes being left at their
/// constrained coordinates while glyphs re-space.
struct HorizontalRemap {
    /// `(source_x, target_x)` control points, sorted by source, sources distinct.
    points: Vec<(f32, f32)>,
    /// Each glyph-bearing slot's rigid translation (`target − source`). A
    /// glyph moves by **its own slot's** delta — never by interpolation, which
    /// would drag a same-slot companion (a time-signature digit after its
    /// barline, a key-signature accidental after the clef) by a neighbouring
    /// interval whenever its absolute x crosses the next slot's source. The
    /// spacing pass already reserved the companion's extent in the slot's
    /// advance; the rigid delta is what honors that reservation.
    slot_delta: BTreeMap<SpringSlotId, f32>,
}

impl HorizontalRemap {
    fn build(input: &ConstrainedLayoutIR) -> Self {
        // The control points are computed collision-aware (per-slot bearings) by
        // the spacing pass; sources are globally monotonic because regions tile
        // left-to-right.
        let spaced = spacing::space_slots(input);
        HorizontalRemap {
            points: spaced.points,
            slot_delta: spaced
                .by_slot
                .into_iter()
                .map(|(id, (source, target))| (id, target - source))
                .collect(),
        }
    }

    /// Maps a constrained x to its spaced x.
    fn map(&self, x: f32) -> f32 {
        let p = &self.points;
        match p.len() {
            0 => x,
            // One column: a pure translation keeps relative offsets.
            1 => x + (p[0].1 - p[0].0),
            n => {
                if x <= p[0].0 {
                    interp(p[0], p[1], x)
                } else if x >= p[n - 1].0 {
                    interp(p[n - 2], p[n - 1], x)
                } else {
                    p.windows(2)
                        .find(|w| x >= w[0].0 && x <= w[1].0)
                        .map(|w| interp(w[0], w[1], x))
                        .unwrap_or(x)
                }
            }
        }
    }

    /// Re-places each glyph by **its slot's rigid delta** (intra-slot offsets
    /// preserved verbatim — see [`HorizontalRemap::slot_delta`]), baseline `y`
    /// preserved; provenance, glyph identity, bounds, style, and layer carried
    /// through. A glyph whose slot the spacing pass never placed
    /// (out-of-pipeline input) falls back to the interpolated map.
    fn glyphs(&self, input: &ConstrainedLayoutIR) -> Vec<ResolvedGlyph> {
        input
            .glyphs
            .iter()
            .map(|g| {
                let x = match self.slot_delta.get(&g.horizontal_slot) {
                    Some(delta) => g.baseline.x.0 + delta,
                    None => self.map(g.baseline.x.0),
                };
                ResolvedGlyph {
                    provenance: g.provenance.clone(),
                    glyph: g.glyph.clone(),
                    position: Point::new(x, g.baseline.y.0),
                    transform: None,
                    bounding_box: g.bounding_box,
                    style: g.style,
                    layer: g.layer,
                }
            })
            .collect()
    }

    /// Re-maps each stroke's endpoints so it tracks the glyphs it spans. A
    /// system-spanning stroke (staff line, barline, …) maps both endpoints, so it
    /// stretches with the spacing; a fixed-width stroke (a ledger line on one
    /// notehead) is translated rigidly by its owning column's delta
    /// ([`epiphany_layout_ir::is_rigid_width_stroke`]) — preserving both its length
    /// and its offset from its glyph, which maps by that same delta at its column.
    fn strokes(&self, input: &ConstrainedLayoutIR) -> Vec<Stroke> {
        input
            .strokes
            .iter()
            .map(|s| {
                let (from_x, to_x) = if let Some(g) = component_glyph(s, &input.glyphs) {
                    // A per-event component stroke (a stem, a ledger) translates
                    // rigidly by its *owning glyph's* slot delta — found by
                    // source, not the stroke's own x (a stem sits offset from its
                    // column, so its midpoint could pick a neighbouring slot). The
                    // slot delta is the exact column translation the glyph itself
                    // moves by, so the stroke keeps its offset from its head and
                    // its length.
                    let delta = self
                        .slot_delta
                        .get(&g.horizontal_slot)
                        .copied()
                        .unwrap_or_else(|| self.map(g.baseline.x.0) - g.baseline.x.0);
                    (s.from.x.0 + delta, s.to.x.0 + delta)
                } else {
                    (self.map(s.from.x.0), self.map(s.to.x.0))
                };
                Stroke {
                    provenance: s.provenance.clone(),
                    from: Point::new(from_x, s.from.y.0),
                    to: Point::new(to_x, s.to.y.0),
                    thickness: s.thickness,
                    layer: s.layer,
                    style: s.style,
                }
            })
            .collect()
    }

    /// Re-maps each curve's four control-point x's through the same coordinate
    /// map as a spanning stroke's endpoints (a slur is never rigid-width), so
    /// the arc stretches with the spacing between its endpoint columns. Each
    /// control point's y is preserved verbatim.
    fn curves(&self, input: &ConstrainedLayoutIR) -> Vec<Curve> {
        input
            .curves
            .iter()
            .map(|c| {
                let map_x = |point: Point| Point::new(self.map(point.x.0), point.y.0);
                Curve {
                    provenance: c.provenance.clone(),
                    p0: map_x(c.p0),
                    p1: map_x(c.p1),
                    p2: map_x(c.p2),
                    p3: map_x(c.p3),
                    thickness: c.thickness,
                    layer: c.layer,
                    style: c.style,
                    line: c.line,
                }
            })
            .collect()
    }
}

/// Linear interpolation/extrapolation through two control points.
fn interp((s0, t0): (f32, f32), (s1, t1): (f32, f32), x: f32) -> f32 {
    if (s1 - s0).abs() < f32::EPSILON {
        t0
    } else {
        t0 + (x - s0) * (t1 - t0) / (s1 - s0)
    }
}

/// What evaluating the declared constraints found, routed by strength: whether
/// every `Required` constraint held, the ids of those that did not, and a
/// soft-violation warning per unhonoured `Preferred` constraint.
struct ConstraintEvaluation {
    required_satisfied: bool,
    unsatisfied: Vec<ConstraintId>,
    soft_violations: Vec<SolverWarning>,
}

impl ConstraintEvaluation {
    /// The result for an input that was never evaluated (malformed structure or
    /// catalog): nothing is claimed satisfied, nothing is named unsatisfied.
    fn not_evaluated() -> Self {
        ConstraintEvaluation {
            required_satisfied: false,
            unsatisfied: Vec::new(),
            soft_violations: Vec::new(),
        }
    }
}

/// The break structure the casting-off pass produced, for constraint
/// evaluation: the slots at which the final layout starts a system, and the
/// subset at which it starts a page.
struct BreakOutcome<'a> {
    system_starts: &'a BTreeSet<SpringSlotId>,
    page_starts: &'a BTreeSet<SpringSlotId>,
}

/// Evaluates the IR's declared constraints — a constraint's id is its index in
/// the IR's constraint list — routing each violation by
/// [`LayoutConstraint::strength`] (Chapter 9 §"Strength Levels"): a violated
/// `Required` constraint is reported unsatisfied, a violated `Preferred` one
/// becomes a [`SolverWarningKind::LargeSoftConstraintViolation`] warning and
/// never fails the solve.
///
/// Geometric constraints (no-collision, alignment, position-within) are checked
/// against the **pre-casting spaced** glyph boxes — the frame the constraints
/// are expressed in; casting-off then relocates whole systems by rigid motions
/// (see `DECISIONS.md`, frame of evaluation). Break constraints are checked
/// against the **cast break structure**: `SystemBreakAt`/`PageBreakAt` is
/// satisfied iff the final layout starts a system/page at that slot — so a hard
/// break is `Unsatisfiable` only if casting-off failed to honour it (which
/// cannot happen for a feasible, structurally valid input), and a soft break is
/// a warning exactly when it was skipped on the documented pathological path.
/// An extension `Registered` constraint this solver cannot interpret is not
/// claimed satisfied (Chapter 7 §"Behavior Under Unknown Extensions":
/// conservative).
fn evaluate_constraints(
    constraints: &[LayoutConstraint],
    glyphs: &[ResolvedGlyph],
    breaks: &BreakOutcome,
) -> ConstraintEvaluation {
    let by_id: BTreeMap<GlyphObjectId, &ResolvedGlyph> = glyphs
        .iter()
        .map(|g| (GlyphObjectId(g.provenance.stable_id.0), g))
        .collect();
    let mut unsatisfied = Vec::new();
    let mut soft_violations = Vec::new();
    for (index, constraint) in constraints.iter().enumerate() {
        let holds = match constraint {
            LayoutConstraint::NoCollision { a, b } => match (by_id.get(a), by_id.get(b)) {
                (Some(a), Some(b)) => !overlaps(a, b),
                // A referenced glyph was dropped (a diagnostic layout): not claimed.
                _ => false,
            },
            LayoutConstraint::Align { a, b, axis } => match (by_id.get(a), by_id.get(b)) {
                (Some(a), Some(b)) => aligned(a, b, *axis),
                _ => false,
            },
            LayoutConstraint::PositionWithin { glyph, region } => match by_id.get(glyph) {
                Some(g) => within(g, region),
                None => false,
            },
            LayoutConstraint::SystemBreakAt { slot, .. } => breaks.system_starts.contains(slot),
            LayoutConstraint::PageBreakAt { slot, .. } => breaks.page_starts.contains(slot),
            LayoutConstraint::Registered(_, _) => false,
        };
        if holds {
            continue;
        }
        let id = ConstraintId(index as u128);
        match constraint.strength() {
            ConstraintStrength::Required => unsatisfied.push(id),
            ConstraintStrength::Preferred { .. } => soft_violations.push(SolverWarning {
                kind: SolverWarningKind::LargeSoftConstraintViolation {
                    constraint: id,
                    // A break preference is binary — honoured or not — so an
                    // unhonoured one is a full (1.0) violation.
                    magnitude: 1.0,
                },
                affected_objects: Vec::new(),
                message: "a preferred (soft) break is not honoured by this solve \
                          (skipped on the pathological-system path; an IrOverride \
                          decision records it)"
                    .to_owned(),
            }),
        }
    }
    ConstraintEvaluation {
        required_satisfied: unsatisfied.is_empty(),
        unsatisfied,
        soft_violations,
    }
}

/// A resolved glyph's absolute bounding box `[left, bottom, right, top]`.
fn abs_box(g: &ResolvedGlyph) -> [f32; 4] {
    [
        g.position.x.0 + g.bounding_box.left.0,
        g.position.y.0 + g.bounding_box.bottom.0,
        g.position.x.0 + g.bounding_box.right.0,
        g.position.y.0 + g.bounding_box.top.0,
    ]
}

/// Whether two glyphs' boxes overlap (touching edges do not count).
fn overlaps(a: &ResolvedGlyph, b: &ResolvedGlyph) -> bool {
    let [al, ab, ar, at] = abs_box(a);
    let [bl, bb, br, bt] = abs_box(b);
    ar > bl && br > al && at > bb && bt > ab
}

/// Whether two glyphs are aligned along `axis`: a common horizontal line (equal
/// baseline y) for `Horizontal`, a common vertical line (equal x) for `Vertical`.
/// (The spec leaves the axis sense to the solver; this is the chosen convention.)
fn aligned(a: &ResolvedGlyph, b: &ResolvedGlyph, axis: Axis) -> bool {
    const EPS: f32 = 1e-3;
    match axis {
        Axis::Horizontal => (a.position.y.0 - b.position.y.0).abs() < EPS,
        Axis::Vertical => (a.position.x.0 - b.position.x.0).abs() < EPS,
    }
}

/// Whether a glyph's box lies within a region rectangle (inclusive, with a small
/// tolerance for quantization).
fn within(g: &ResolvedGlyph, region: &Rect) -> bool {
    const EPS: f32 = 1e-3;
    let [l, b, r, t] = abs_box(g);
    let rl = region.origin.x.0;
    let rb = region.origin.y.0;
    let rr = rl + region.size.width.0;
    let rt = rb + region.size.height.0;
    l >= rl - EPS && b >= rb - EPS && r <= rr + EPS && t <= rt + EPS
}

impl ConstraintSolver for Engraver {
    fn tier(&self) -> SolverTier {
        // Minimal (Chapter 9): it evaluates and satisfies the IR's declared hard
        // constraints, reporting honestly which (if any) it cannot, and computes
        // real quality-metric vectors per the Quality Metric Catalog — accurate
        // reports being part of the Minimal claim. `Minimal` still makes no
        // optimality claim (greedy first-fit casting-off is legitimate here);
        // the Standard tier's tighter thresholds are not claimed.
        SolverTier::Minimal
    }

    fn version(&self) -> SolverVersion {
        ENGRAVER_VERSION
    }

    fn solve(&self, input: &ConstrainedLayoutIR, config: &SolverConfig) -> SolveReport {
        self.resolve(input, config)
    }

    fn solve_incremental(
        &self,
        input: &ConstrainedLayoutIR,
        _prior: &SolverState,
        _invalidations: &InvalidationSet,
        config: &SolverConfig,
    ) -> SolveReport {
        // The scaffold recomputes spacing from scratch, which is trivially
        // observationally equivalent to a scoped incremental solve (Chapter 9
        // §"Observational Equivalence"). Real incremental scoping is Minimal-tier
        // work.
        self.resolve(input, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{is_rigid_width_stroke, to_constrained, to_logical, StubSolver};

    fn fixture() -> ConstrainedLayoutIR {
        to_constrained(&to_logical(&valid_score_rich(11)))
    }

    #[test]
    fn reports_the_minimal_tier_it_has_earned() {
        use epiphany_layout_ir::{MINIMAL_THRESHOLDS, QUALITY_METRIC_KINDS};
        // It evaluates the declared hard constraints, so it reports Minimal — above
        // the interface-only stub, below the tighter-threshold Standard tier.
        assert_eq!(Engraver::default().tier(), SolverTier::Minimal);
        assert!(Engraver::default().tier() > StubSolver.tier());
        assert!(Engraver::default().tier() < SolverTier::Standard);
        // Accurate metric vectors are part of the Minimal claim (Chapter 9;
        // Quality Metric Catalog): the vector is *real* — never the all-worst
        // unmeasured placeholder — collision-free on this clean fixture, and
        // every axis is a valid NormalizedMetric within the catalog's Minimal
        // threshold column (the fixture's three regions each cast onto a
        // single system, so the break-family axes degenerate to exactly 0.0
        // under the vacuous-geometry rule).
        let report = Engraver::default().solve(&fixture(), &SolverConfig::default());
        assert_ne!(report.metric_vector, QualityMetricVector::unmeasured());
        assert_eq!(report.metric_vector.collision_penalty.0, 0.0);
        for kind in QUALITY_METRIC_KINDS {
            let value = report.metric_vector.axis(kind).0;
            assert!(
                value.is_finite() && (0.0..=1.0).contains(&value),
                "{kind:?}"
            );
            assert!(
                value <= MINIMAL_THRESHOLDS.axis(kind),
                "{kind:?} = {value} exceeds its Minimal threshold"
            );
        }
        assert_eq!(Engraver::default().version(), ENGRAVER_VERSION);
        assert_ne!(Engraver::default().version(), StubSolver.version());
    }

    /// Builds a tiny valid constrained IR — a clef and a single note — and lets
    /// the caller add constraints over its glyphs.
    fn with_constraints(
        constraints: impl FnOnce(&ConstrainedLayoutIR) -> Vec<epiphany_layout_ir::LayoutConstraint>,
    ) -> ConstrainedLayoutIR {
        use epiphany_core::{
            CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling, RegionId, StaffId,
            StaffInstanceId, TypedObjectId,
        };
        use epiphany_layout_ir::{
            LayoutContent, LayoutObject, LayoutRegion, LocalCoordinateSystem, LogicalLayoutIR,
            MetricTimeAxis, NoteContent, NotePitch, Provenance, ScoreVersion, StaffContent,
            TimeAxisModel, TimePoint, VerticalExtent,
        };
        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch = PitchId::from_raw(100);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    manifested(
                        TypedObjectId::StaffInstance(StaffInstanceId::from_raw(1)),
                        LayoutContent::Staff(StaffContent {
                            clefs: vec![],
                            keys: vec![],
                        }),
                    ),
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(1)),
                        LayoutContent::Note(NoteContent {
                            position: TimePoint::Musical(MusicalPosition::origin()),
                            components: vec![],
                            pitches: vec![NotePitch {
                                pitch,
                                spelling: Some(PitchSpelling::cmn(CmnNominal::C, 5)),
                            }],
                        }),
                    ),
                    manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let mut c = to_constrained(&logical);
        c.constraints = constraints(&c);
        c
    }

    #[test]
    fn the_pipelines_emitted_constraints_are_satisfied() {
        // The spacing stage now emits real constraints (no-collision chains,
        // per-glyph containment) — this is *not* a vacuous empty-set solve. The
        // collision-aware spacing satisfies every one of them, and the solve
        // honestly reports the evaluation work it did.
        let input = fixture();
        assert!(
            !input.constraints.is_empty(),
            "the pipeline declares real constraints"
        );
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved);
        assert!(report.satisfied_hard_constraints);
        assert!(report.unsatisfied_constraints.is_empty());
        assert_eq!(
            report.budget_used.constraint_evaluations,
            input.constraints.len() as u64
        );
    }

    #[test]
    fn a_satisfied_no_collision_constraint_solves() {
        use epiphany_layout_ir::LayoutConstraint;
        // The clef and the notehead are in different columns, so they do not
        // collide; a NoCollision over them is satisfied.
        let input = with_constraints(|c| {
            let clef = c
                .glyphs
                .iter()
                .find(|g| g.glyph.as_str() == "gClef")
                .unwrap()
                .id();
            let head = c
                .glyphs
                .iter()
                .find(|g| g.glyph.as_str().starts_with("notehead"))
                .unwrap()
                .id();
            vec![LayoutConstraint::NoCollision { a: clef, b: head }]
        });
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
        assert!(report.satisfied_hard_constraints);
        assert!(report.unsatisfied_constraints.is_empty());
        assert_eq!(report.budget_used.constraint_evaluations, 1);
    }

    #[test]
    fn a_violated_no_collision_is_reported_not_falsely_solved() {
        use epiphany_layout_ir::LayoutConstraint;
        // A glyph trivially collides with itself; NoCollision(g, g) is unsatisfiable
        // and must be reported, not silently accepted.
        let input = with_constraints(|c| {
            let g = c
                .glyphs
                .iter()
                .find(|g| g.glyph.as_str().starts_with("notehead"))
                .unwrap()
                .id();
            vec![LayoutConstraint::NoCollision { a: g, b: g }]
        });
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        // A valid problem whose hard constraint cannot be met is Unsatisfiable,
        // not an InternalError (which is reserved for solver/structure failures).
        assert_eq!(report.status, SolveStatus::Unsatisfiable);
        assert!(!report.satisfied_hard_constraints);
        assert_eq!(report.unsatisfied_constraints.len(), 1);
        assert_eq!(report.budget_used.constraint_evaluations, 1);
    }

    /// Total systems across all pages of a resolved layout.
    fn system_count(layout: &ResolvedLayoutIR) -> usize {
        layout.pages.iter().map(|p| p.systems.len()).sum()
    }

    #[test]
    fn a_hard_break_is_honoured_by_casting_off() {
        use epiphany_layout_ir::{BreakKind, DecisionSource, EngravingDecisionKind};
        // Inverse of the pre-casting-off pin (`a_hard_break_cannot_be_honoured_
        // by_single_system_minimal`): a hard break maps to
        // ConstraintStrength::Required, and the casting-off pass ALWAYS breaks
        // at it — even though that leaves a clef-only first system — so the
        // solve is Solved and the system count increases.
        let baseline =
            Engraver::default().solve(&with_constraints(|_| vec![]), &SolverConfig::default());
        assert_eq!(baseline.status, SolveStatus::Solved);
        assert_eq!(system_count(&baseline.layout), 1);

        let input = with_constraints(|c| {
            // Slot 0 is the clef lead (trivially at a boundary); the note
            // column is the non-trivial break target.
            let slot = c.horizontal_slots[1].id;
            vec![LayoutConstraint::SystemBreakAt {
                slot,
                kind: BreakKind::Hard,
            }]
        });
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
        assert!(report.satisfied_hard_constraints);
        assert!(report.unsatisfied_constraints.is_empty());
        assert_eq!(
            system_count(&report.layout),
            2,
            "the hard break splits the line into two systems"
        );
        // The chosen break is recorded as an engraved decision; no user
        // override projected this constraint, so it is attributed Automatic.
        assert!(report
            .layout
            .engraving_decisions
            .iter()
            .any(|d| d.kind == EngravingDecisionKind::SystemBreak
                && d.source == DecisionSource::Automatic));
    }

    #[test]
    fn a_soft_break_with_content_before_it_is_honoured() {
        use epiphany_layout_ir::{BreakKind, DecisionSource, EngravingDecisionKind};
        // A soft break whose closing system carries musical content is simply
        // honoured: a clean Solved two-system layout, no soft-violation
        // warning, and an Automatic engraved decision.
        let mut input = two_off_staff_whole_notes();
        let slot = input.horizontal_slots[2].id; // the second note column
        input.constraints.push(LayoutConstraint::SystemBreakAt {
            slot,
            kind: BreakKind::Soft,
        });
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
        // The honoured break is never reported as a soft violation. (The
        // report legitimately carries QualityFloorApproached diagnostics: this
        // two-note micro-score casts off into wildly uneven system widths,
        // which the casting-off axis honestly measures — quality warnings are
        // diagnostic and, per the catalog, never change the status.)
        assert!(
            !report.warnings.iter().any(|w| matches!(
                w.kind,
                SolverWarningKind::LargeSoftConstraintViolation { .. }
            )),
            "an honoured break must not surface as a soft violation: {:?}",
            report.warnings
        );
        assert!(report.satisfied_hard_constraints);
        assert_eq!(system_count(&report.layout), 2);
        assert!(report
            .layout
            .engraving_decisions
            .iter()
            .any(|d| d.kind == EngravingDecisionKind::SystemBreak
                && d.source == DecisionSource::Automatic));
    }

    #[test]
    fn a_pathological_soft_break_is_skipped_and_recorded_as_ir_override() {
        use epiphany_layout_ir::{BreakKind, DecisionSource, EngravingDecisionKind};
        // A soft break at the first note column would close a system containing
        // only the clef — no musical content. The documented exceptional path
        // skips it: still renderable (a Preferred violation is a warning, never
        // a failure), the constraint is reported as a soft violation, and the
        // unhonoured preference is recorded as an IrOverride-sourced decision
        // (the spec's override-resolution rule: record, don't drop).
        let input = with_constraints(|c| {
            let slot = c.horizontal_slots[1].id;
            vec![LayoutConstraint::SystemBreakAt {
                slot,
                kind: BreakKind::Soft,
            }]
        });
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::SolvedWithWarnings);
        assert!(report.status.is_renderable());
        assert!(
            report.satisfied_hard_constraints,
            "a soft-break violation must not flip hard-constraint satisfaction"
        );
        assert!(report.unsatisfied_constraints.is_empty());
        assert!(report.warnings.iter().any(|w| matches!(
            w.kind,
            SolverWarningKind::LargeSoftConstraintViolation {
                constraint: ConstraintId(0),
                magnitude,
            } if magnitude == 1.0
        )));
        assert_eq!(
            system_count(&report.layout),
            1,
            "the pathological break was skipped, not honoured"
        );
        assert!(report
            .layout
            .engraving_decisions
            .iter()
            .any(|d| d.kind == EngravingDecisionKind::SystemBreak
                && d.source == DecisionSource::IrOverride));
    }

    #[test]
    fn a_users_break_is_honoured_and_recorded_with_its_override() {
        // Inverse of the pre-casting-off pin (`a_users_break_flows_to_a_soft_
        // violation_not_a_failure`). End to end: a user system break on the
        // score graph projects through the logical stage's break override into
        // a Soft break constraint, which casting-off HONOURS — the anchored
        // column starts a new system at the left margin, the solve is clean
        // (Solved, no warnings), and the engraved decision cites the user's
        // override id (DecisionSource::UserOverride).
        use epiphany_core::generators::valid_score;
        use epiphany_core::{AnchorOffset, Event, EventPosition, TimeAnchor};
        use epiphany_layout_ir::{DecisionSource, EngravingDecisionKind};
        let mut score = valid_score(3);
        // The latest pitched onset: a mid-region break target, so the closing
        // system carries musical content (the honoured, non-pathological path).
        let event = score.canvas.regions[0]
            .staff_instances()
            .iter()
            .flat_map(|si| si.voices.iter())
            .flat_map(|voice| voice.events.iter().copied())
            .filter(|eid| {
                matches!(score.events.get(*eid), Some(Event::Pitched(p)) if !p.pitches.is_empty())
            })
            .max_by_key(|eid| match score.events.get(*eid).map(|e| e.position()) {
                Some(EventPosition::Musical(p)) => Some(p.clone()),
                _ => None,
            })
            .expect("valid_score has a pitched event");
        score.canvas.regions[0]
            .content
            .staff_based_mut()
            .expect("valid_score is staff based")
            .user_system_breaks
            .push(TimeAnchor::Event {
                id: event,
                offset: AnchorOffset::Zero,
            });

        let constrained = to_constrained(&to_logical(&score));
        let break_slot = constrained
            .constraints
            .iter()
            .find_map(|c| match c {
                LayoutConstraint::SystemBreakAt { slot, .. } => Some(*slot),
                _ => None,
            })
            .expect("the user break projects into a break constraint");
        let origin = constrained
            .break_origins
            .iter()
            .find(|o| o.slot == break_slot)
            .expect("the projection records the override attribution");

        let engraver = Engraver::default();
        let report = engraver.solve(&constrained, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
        // An honoured break never warns *about the break* (no soft violation).
        // The report may carry QualityFloorApproached diagnostics — this
        // few-note score's user break honestly leaves a stub last system,
        // which the casting-off axis measures; quality warnings never change
        // the status per the catalog.
        assert!(
            !report.warnings.iter().any(|w| matches!(
                w.kind,
                SolverWarningKind::LargeSoftConstraintViolation { .. }
            )),
            "an honoured break never surfaces as a soft violation: {:?}",
            report.warnings
        );
        assert!(report.satisfied_hard_constraints);
        assert!(report.unsatisfied_constraints.is_empty());
        assert!(
            system_count(&report.layout) >= 2,
            "the honoured break increases the system count"
        );
        // The break lands at the anchor's column: the anchored slot's glyphs
        // now start their system at the page's left content edge (up to the
        // ledger-line extension, 0.3 staff spaces, which also participates in
        // the system's extent and may sit left of the notehead box).
        let left_edge = report
            .layout
            .glyphs
            .iter()
            .zip(&constrained.glyphs)
            .filter(|(_, c)| c.horizontal_slot == break_slot)
            .map(|(r, c)| r.position.x.0 + c.bounding_box.left.0)
            .fold(f32::INFINITY, f32::min);
        let margin = engraver.geometry().margins.left.0;
        assert!(
            left_edge >= margin - 1e-3 && left_edge <= margin + 0.5,
            "the anchored column starts its system at the left margin \
             (edge {left_edge}, margin {margin})"
        );
        // The decision record cites the user's override.
        assert!(report
            .layout
            .engraving_decisions
            .iter()
            .any(|d| d.kind == EngravingDecisionKind::SystemBreak
                && d.source == DecisionSource::UserOverride(origin.override_id)));
    }

    #[test]
    fn ledger_lines_keep_their_width_through_the_spacing_pass() {
        // A corpus score with off-staff notes yields ledger strokes; the horizontal
        // spacing pass must translate them (preserving length), not re-map both
        // endpoints — which would scale a fixed-width mark with the local spacing.
        let mut checked = 0;
        for seed in 0..16 {
            let input = to_constrained(&to_logical(&valid_score_rich(seed)));
            let widths: std::collections::HashMap<u128, f32> = input
                .strokes
                .iter()
                .filter(|s| is_rigid_width_stroke(s))
                .map(|s| (s.provenance.stable_id.0, s.to.x.0 - s.from.x.0))
                .collect();
            if widths.is_empty() {
                continue;
            }
            let report = Engraver::default().solve(&input, &SolverConfig::default());
            for s in report
                .layout
                .strokes
                .iter()
                .filter(|s| is_rigid_width_stroke(s))
            {
                if let Some(&w_in) = widths.get(&s.provenance.stable_id.0) {
                    let w_out = s.to.x.0 - s.from.x.0;
                    assert!(
                        (w_out - w_in).abs() < 1e-4,
                        "ledger width changed through spacing: {w_in} -> {w_out}"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "no ledger strokes exercised across 16 seeds");
    }

    /// Two whole notes (wide heads) a step above the staff, in adjacent time
    /// columns — the case where a ledger's own midpoint can fall nearer the next
    /// column than its notehead's.
    fn two_off_staff_whole_notes() -> ConstrainedLayoutIR {
        use epiphany_core::{
            CmnNominal, EventId, MusicalDuration, MusicalPosition, NotatedComponent, NoteValue,
            PitchId, PitchSpelling, RationalTime, RegionId, StaffId, StaffInstanceId,
            TypedObjectId,
        };
        use epiphany_layout_ir::{
            LayoutContent, LayoutObject, LayoutRegion, LocalCoordinateSystem, LogicalLayoutIR,
            MetricTimeAxis, NoteContent, NotePitch, PlacedComponent, Provenance, ScoreVersion,
            StaffContent, TimeAxisModel, TimePoint, VerticalExtent,
        };
        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let whole = || {
            vec![PlacedComponent {
                offset: MusicalDuration::zero(),
                component: NotatedComponent {
                    base_value: NoteValue::Whole,
                    dots: 0,
                    tuplet: None,
                    tied_to_next: false,
                },
                tuplet: None,
            }]
        };
        let note = |eid: u128, pid: u128, pos: MusicalPosition| {
            let pitch = PitchId::from_raw(pid);
            [
                manifested(
                    TypedObjectId::Event(EventId::from_raw(eid)),
                    LayoutContent::Note(NoteContent {
                        position: TimePoint::Musical(pos),
                        components: whole(),
                        // C6 is a step above the treble staff, so each head earns
                        // ledger lines.
                        pitches: vec![NotePitch {
                            pitch,
                            spelling: Some(PitchSpelling::cmn(CmnNominal::C, 6)),
                        }],
                    }),
                ),
                manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
            ]
        };
        let mut objects = vec![manifested(
            TypedObjectId::StaffInstance(StaffInstanceId::from_raw(1)),
            LayoutContent::Staff(StaffContent {
                clefs: vec![],
                keys: vec![],
            }),
        )];
        objects.extend(note(1, 101, MusicalPosition::origin()));
        objects.extend(note(
            2,
            102,
            MusicalPosition(RationalTime::new(1, 1).unwrap()),
        ));
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: VerticalExtent {
                    staves: vec![staff],
                },
                objects,
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        to_constrained(&logical)
    }

    #[test]
    fn an_off_staff_whole_note_ledger_does_not_drift() {
        let input = two_off_staff_whole_notes();
        assert!(
            input
                .glyphs
                .iter()
                .any(|g| g.glyph.as_str() == "noteheadWhole"),
            "the fixture engraves whole notes"
        );
        let ledger_count = input
            .strokes
            .iter()
            .filter(|s| is_rigid_width_stroke(s))
            .count();
        assert!(
            ledger_count >= 2,
            "off-staff whole notes earn ledger strokes"
        );

        let report = Engraver::default().solve(&input, &SolverConfig::default());
        // The wide whole-note columns re-space (their deltas differ), so a midpoint-
        // anchored ledger would translate by a neighbouring column's delta and drift.
        // The owning-glyph anchor keeps every ledger at its notehead's offset.
        for s_in in input.strokes.iter().filter(|s| is_rigid_width_stroke(s)) {
            let g_in = owning_glyph(s_in, &input.glyphs).expect("owning notehead");
            let s_out = report
                .layout
                .strokes
                .iter()
                .find(|s| s.provenance.stable_id == s_in.provenance.stable_id)
                .expect("stroke survives");
            let g_out = report
                .layout
                .glyphs
                .iter()
                .find(|g| g.provenance.stable_id == g_in.provenance.stable_id)
                .expect("glyph survives");
            let offset_in = s_in.from.x.0 - g_in.baseline.x.0;
            let offset_out = s_out.from.x.0 - g_out.position.x.0;
            assert!(
                (offset_out - offset_in).abs() < 1e-4,
                "whole-note ledger drifted {offset_in} -> {offset_out}"
            );
        }
    }

    #[test]
    fn ledger_offsets_from_the_notehead_survive_the_engraver() {
        // The column-delta translation keeps a ledger at exactly the same offset from
        // its notehead through the spacing pass; interpolating its midpoint (the bug
        // this replaced) would shift it under a non-unit local slope.
        let mut checked = 0;
        for seed in 0..16 {
            let input = to_constrained(&to_logical(&valid_score_rich(seed)));
            let report = Engraver::default().solve(&input, &SolverConfig::default());
            for s_in in input.strokes.iter().filter(|s| is_rigid_width_stroke(s)) {
                let lo = s_in.from.x.0.min(s_in.to.x.0);
                let hi = s_in.from.x.0.max(s_in.to.x.0);
                let Some(g_in) = input.glyphs.iter().find(|g| {
                    g.provenance.source == s_in.provenance.source
                        && g.baseline.x.0 >= lo
                        && g.baseline.x.0 <= hi
                }) else {
                    continue;
                };
                let Some(s_out) = report
                    .layout
                    .strokes
                    .iter()
                    .find(|s| s.provenance.stable_id == s_in.provenance.stable_id)
                else {
                    continue;
                };
                let Some(g_out) = report
                    .layout
                    .glyphs
                    .iter()
                    .find(|g| g.provenance.stable_id == g_in.provenance.stable_id)
                else {
                    continue;
                };
                let offset_in = s_in.from.x.0 - g_in.baseline.x.0;
                let offset_out = s_out.from.x.0 - g_out.position.x.0;
                assert!(
                    (offset_out - offset_in).abs() < 1e-4,
                    "seed {seed}: ledger offset drifted {offset_in} -> {offset_out}"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no ledger/notehead pairs exercised");
    }

    #[test]
    fn stem_offsets_from_the_notehead_survive_justification() {
        // A stem is `Event`-sourced with no same-source glyph, so it tracks its
        // column via `component_glyph`'s nearest-left notehead. Its offset from
        // that column must survive the FULL solve — spacing AND per-system
        // justification — so a stem in a stretched non-final system stays
        // attached to its head rather than being dragged into the gap (the
        // review's severe finding). Checked across seeds that wrap (justify).
        let mut checked = 0;
        for seed in 0..16 {
            let input = to_constrained(&to_logical(&valid_score_rich(seed)));
            let report = Engraver::default().solve(&input, &SolverConfig::default());
            for s_in in &input.strokes {
                // Vertical, non-zero-length strokes are drawn stems.
                if (s_in.from.x.0 - s_in.to.x.0).abs() > 1e-4
                    || (s_in.from.y.0 - s_in.to.y.0).abs() < 1e-3
                {
                    continue;
                }
                let Some(owner_in) = component_glyph(s_in, &input.glyphs) else {
                    continue;
                };
                // A stem's owner is found by nearest-left, NOT source match
                // (that path is the ledger case); skip anything same-source.
                if owner_in.provenance.source == s_in.provenance.source {
                    continue;
                }
                let (Some(s_out), Some(g_out)) = (
                    report
                        .layout
                        .strokes
                        .iter()
                        .find(|s| s.provenance.stable_id == s_in.provenance.stable_id),
                    report
                        .layout
                        .glyphs
                        .iter()
                        .find(|g| g.provenance.stable_id == owner_in.provenance.stable_id),
                ) else {
                    continue;
                };
                let offset_in = s_in.from.x.0 - owner_in.baseline.x.0;
                let offset_out = s_out.from.x.0 - g_out.position.x.0;
                assert!(
                    (offset_out - offset_in).abs() < 1e-3,
                    "seed {seed}: stem offset drifted {offset_in} -> {offset_out} \
                     (justification scaled it off its notehead)"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no stem/notehead pairs exercised");
    }

    #[test]
    fn adjacent_ledger_lines_are_spaced_not_overlapping() {
        use std::collections::HashMap;
        // The spacing pass reserves room for ledger overhang, so two off-staff notes
        // that share a ledger height (same step) and sit in neighbouring columns get
        // ledger strokes that do not overlap.
        let mut ledgers = 0;
        let mut pairs = 0;
        for seed in 0..32 {
            let report = Engraver::default().solve(
                &to_constrained(&to_logical(&valid_score_rich(seed))),
                &SolverConfig::default(),
            );
            let mut by_height: HashMap<i64, Vec<(f32, f32, _)>> = HashMap::new();
            for s in report
                .layout
                .strokes
                .iter()
                .filter(|s| is_rigid_width_stroke(s))
            {
                ledgers += 1;
                let y = (s.from.y.0 * 1024.0).round() as i64;
                let (lo, hi) = (s.from.x.0.min(s.to.x.0), s.from.x.0.max(s.to.x.0));
                by_height
                    .entry(y)
                    .or_default()
                    .push((lo, hi, s.provenance.source));
            }
            for group in by_height.values_mut() {
                group.sort_by(|a, b| a.0.total_cmp(&b.0));
                for w in group.windows(2) {
                    // Distinct notes' ledgers at the same height must not overlap.
                    if w[0].2 != w[1].2 {
                        pairs += 1;
                        assert!(
                            w[0].1 <= w[1].0 + 1e-3,
                            "seed {seed}: ledger lines overlap ({:?} vs {:?})",
                            w[0],
                            w[1]
                        );
                    }
                }
            }
        }
        assert!(ledgers > 0, "the corpus exercised no ledger lines");
        assert!(pairs > 0, "no adjacent same-height ledger pairs to check");
    }

    #[test]
    fn solves_the_stub_pipeline_and_preserves_provenance() {
        let input = fixture();
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved);
        assert!(report.satisfied_hard_constraints);
        assert_eq!(report.layout.glyphs.len(), input.glyphs.len());
        // Every input glyph's provenance survives, one-for-one.
        for (resolved, original) in report.layout.glyphs.iter().zip(&input.glyphs) {
            assert_eq!(resolved.provenance, original.provenance);
            assert_eq!(resolved.glyph, original.glyph);
        }
        // The metric vector is real — computed per the Quality Metric Catalog,
        // never the all-worst placeholder — and this clean pipeline fixture is
        // collision-free under the full same-system census.
        assert_ne!(report.metric_vector, QualityMetricVector::unmeasured());
        assert_eq!(report.metric_vector.collision_penalty.0, 0.0);
    }

    #[test]
    fn a_structurally_invalid_input_emits_no_strokes() {
        // Strokes are gated on the same structural validity as glyphs: an input
        // whose validation fails (here, an out-of-range stroke thickness) yields a
        // diagnostic layout with no glyphs *and* no strokes — the malformed stroke
        // must not leak into canonical_bytes or the renderer.
        let mut input = fixture();
        let provenance = input.glyphs[0].provenance.clone();
        input.strokes.push(epiphany_layout_ir::Stroke {
            provenance,
            from: epiphany_layout_ir::Point::new(0.0, 0.0),
            to: epiphany_layout_ir::Point::new(1.0, 0.0),
            thickness: epiphany_layout_ir::StaffSpace(f32::MAX),
            layer: 0,
            style: epiphany_layout_ir::GlyphStyle::default(),
        });
        // Declare a constraint too: a malformed input is *not* evaluated, so the
        // report must claim zero constraint evaluations (not work it never did).
        let g = input.glyphs[0].id();
        input
            .constraints
            .push(epiphany_layout_ir::LayoutConstraint::NoCollision { a: g, b: g });
        assert!(
            input.validate().is_err(),
            "the out-of-range stroke is invalid"
        );
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::InternalError);
        assert!(report.layout.glyphs.is_empty());
        assert!(
            report.layout.strokes.is_empty(),
            "a structurally invalid input emits no strokes (gated like glyphs)"
        );
        assert_eq!(
            report.budget_used.constraint_evaluations, 0,
            "no constraints were evaluated on a malformed input"
        );
    }

    #[test]
    fn horizontal_spacing_differs_from_the_verbatim_stub() {
        // The whole point of the scaffold: it re-spaces horizontally rather than
        // echoing the input columns. With multiple glyphs the positions differ
        // from the stub's verbatim baselines.
        let input = fixture();
        assert!(input.glyphs.len() >= 2);
        let engraved = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .layout;
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
    fn strokes_ride_the_same_coordinate_map_as_glyphs() {
        // The spacing pass re-places glyphs; the strokes that track them must move
        // by the same horizontal map, not stay at their constrained coordinates.
        let input = fixture();
        let engraved = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .layout;

        // Constrained x -> engraved x, per glyph.
        let glyph_map: Vec<(f32, f32)> = input
            .glyphs
            .iter()
            .zip(&engraved.glyphs)
            .map(|(c, r)| (c.baseline.x.0, r.position.x.0))
            .collect();

        // Every stroke endpoint coincident with a glyph's column lands at that
        // glyph's engraved x — they ride one map, so they stay attached.
        let mut checked = 0;
        for (c, r) in input.strokes.iter().zip(&engraved.strokes) {
            for (gx, ex) in &glyph_map {
                if (c.from.x.0 - gx).abs() < 1e-6 {
                    assert!(
                        (r.from.x.0 - ex).abs() < 1e-3,
                        "a stroke at a glyph's column detached from it after spacing"
                    );
                    checked += 1;
                }
            }
        }
        assert!(
            checked > 0,
            "expected strokes coincident with glyph columns"
        );

        // …and the strokes actually moved (the pass re-spaces, it does not echo).
        let moved = input.strokes.iter().zip(&engraved.strokes).any(|(c, r)| {
            (c.from.x.0 - r.from.x.0).abs() > 1e-4 || (c.to.x.0 - r.to.x.0).abs() > 1e-4
        });
        assert!(
            moved,
            "strokes must be re-spaced with the glyphs, not left behind"
        );
    }

    #[test]
    fn solve_is_deterministic_and_quantizable() {
        let input = fixture();
        let a = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .layout;
        let b = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .layout;
        // Byte-identical canonical output across solves (Chapter 9 determinism).
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    }

    #[test]
    fn engraver_reserves_an_accidental_against_the_previous_note() {
        // A note's accidental overhangs *left* of its notehead, into the previous
        // note's column. The spacing pass must reserve that overhang (against the
        // previous slot's advance), or the accidental overlaps the prior notehead.
        use epiphany_core::{
            AccidentalId, CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling,
            RationalTime, RegionId, StaffId, TypedObjectId,
        };
        use epiphany_layout_ir::{
            LayoutContent, LayoutObject, LayoutRegion, LocalCoordinateSystem, LogicalLayoutIR,
            MetricTimeAxis, NoteContent, NotePitch, Provenance, ScoreVersion, TimeAxisModel,
            TimePoint, VerticalExtent,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let plain = PitchId::from_raw(100);
        let sharped = PitchId::from_raw(101);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let at = |n, d| TimePoint::Musical(MusicalPosition(RationalTime::new(n, d).unwrap()));
        let note = |pid: PitchId, time: TimePoint, accidental: bool| {
            let mut spelling = PitchSpelling::cmn(CmnNominal::C, 5);
            if accidental {
                spelling.accidentals.push(AccidentalId::new("sharp"));
            }
            LayoutContent::Note(NoteContent {
                position: time,
                components: vec![],
                pitches: vec![NotePitch {
                    pitch: pid,
                    spelling: Some(spelling),
                }],
            })
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    // A plain note, then a note with a sharp a quarter later.
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(1)),
                        note(plain, at(0, 1), false),
                    ),
                    manifested(TypedObjectId::Pitch(plain), LayoutContent::Structural),
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(2)),
                        note(sharped, at(1, 4), true),
                    ),
                    manifested(TypedObjectId::Pitch(sharped), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };

        let constrained = to_constrained(&logical);
        let engraved = Engraver::default()
            .solve(&constrained, &SolverConfig::default())
            .layout;
        let mut noteheads: Vec<_> = engraved
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "noteheadBlack")
            .collect();
        noteheads.sort_by(|a, b| a.position.x.0.partial_cmp(&b.position.x.0).unwrap());
        assert_eq!(noteheads.len(), 2, "two noteheads");
        let first_right = noteheads[0].position.x.0 + noteheads[0].bounding_box.right.0;
        let sharp = engraved
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str() == "accidentalSharp")
            .expect("a sharp is drawn");
        let sharp_left = sharp.position.x.0 + sharp.bounding_box.left.0;
        assert!(
            sharp_left >= first_right,
            "the accidental ({sharp_left}) overlaps the previous notehead's right edge ({first_right})"
        );
    }

    #[test]
    fn engraver_preserves_key_signature_lead_spacing() {
        // The lead area (clef + key signature) is fixed-width content. The spacing
        // pass must reserve it via the lead slot's preferred width, or it compresses
        // the key signature back onto the clef. This drives the *real* engraver, not
        // just the verbatim stub.
        use epiphany_core::{
            CmnNominal, EventId, KeySignature, MusicalPosition, PitchId, PitchSpelling, RegionId,
            StaffId, StaffInstanceId, TypedObjectId,
        };
        use epiphany_layout_ir::{
            LayoutContent, LayoutObject, LayoutRegion, LocalCoordinateSystem, LogicalLayoutIR,
            MetricTimeAxis, NoteContent, NotePitch, PlacedKeySignature, Provenance, ScoreVersion,
            StaffContent, TimeAxisModel, TimePoint, VerticalExtent,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch = PitchId::from_raw(100);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    // A 3-sharp (A major) key signature, then a note.
                    manifested(
                        TypedObjectId::StaffInstance(StaffInstanceId::from_raw(1)),
                        LayoutContent::Staff(StaffContent {
                            clefs: vec![],
                            keys: vec![PlacedKeySignature {
                                time: TimePoint::Musical(MusicalPosition::origin()),
                                key: KeySignature::new(3).expect("three sharps"),
                            }],
                        }),
                    ),
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(1)),
                        LayoutContent::Note(NoteContent {
                            position: TimePoint::Musical(MusicalPosition::origin()),
                            components: vec![],
                            pitches: vec![NotePitch {
                                pitch,
                                spelling: Some(PitchSpelling::cmn(CmnNominal::C, 5)),
                            }],
                        }),
                    ),
                    manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };

        let constrained = to_constrained(&logical);
        let engraved = Engraver::default()
            .solve(&constrained, &SolverConfig::default())
            .layout;
        let x_of = |name: &str| {
            engraved
                .glyphs
                .iter()
                .find(|g| g.glyph.as_str() == name)
                .map(|g| g.position.x.0)
        };
        let clef_x = x_of("gClef").expect("clef engraved");
        let note_x = x_of("noteheadBlack").expect("notehead engraved");
        let sharps: Vec<f32> = engraved
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "accidentalSharp")
            .map(|g| g.position.x.0)
            .collect();
        assert_eq!(sharps.len(), 3, "a three-sharp signature");

        // Not compressed into the clef: the lead clearly exceeds one note slot.
        assert!(
            note_x - clef_x > 3.0,
            "key signature compressed into the clef (lead width {})",
            note_x - clef_x
        );
        // The accidentals sit in the lead (clef..note), spread to distinct x.
        assert!(
            sharps.iter().all(|&x| x > clef_x && x < note_x),
            "accidentals must lie between the clef and the first note"
        );
        let mut sorted = sharps;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            sorted[0] < sorted[1] && sorted[1] < sorted[2],
            "accidentals are spread out, not stacked at one x"
        );
    }

    #[test]
    fn incremental_is_observationally_equivalent_to_full() {
        let input = fixture();
        let full = Engraver::default().solve(&input, &SolverConfig::default());
        let inc = Engraver::default().solve_incremental(
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

    /// Acceptance criterion 6 (the Chapter 7 layout round-trip) against the **real**
    /// Engraver, not the verbatim stub. `round_trip_with` drives graph -> logical ->
    /// constrained -> *engraved* -> render and asserts the whole provenance contract
    /// internally: every laid-out object is covered, the complete `Provenance`
    /// (source, synthesis, dependencies, stable id) survives the engrave pass and the
    /// render unchanged, the recovered source set is exactly the set laid out, and no
    /// two objects share a stable id. The point the stub can never make: provenance is
    /// preserved *through a real geometry change* — the Engraver re-spaces every glyph
    /// and the strokes that track them, yet not one back-reference is lost.
    #[test]
    fn criterion_six_round_trips_through_the_engravers_respacing() {
        use epiphany_core::generators::valid_score;
        use epiphany_layout_ir::{round_trip_with, SolveStatus};
        use epiphany_testkit::fixtures::{ten_measure_single_staff, ten_measure_with_repeats};

        for seed in 0..32u64 {
            // Mirror the criterion-6 hand-off gate's own fixtures — the 10-measure
            // single staff (measures + barlines), its repeat-bearing sibling
            // (morphed/standalone repeat signs, dot pair, volta brackets — all
            // re-spaced and cast off), and the rich score (cross-cutting
            // tuplet/tie/spanner/marker) — and keep `valid_score` for breadth.
            let scores = [
                ten_measure_single_staff(seed),
                ten_measure_with_repeats(seed),
                valid_score(seed),
                valid_score_rich(seed),
            ];
            for score in scores {
                // round_trip_with asserts the full provenance contract; a Solved
                // status also confirms the Engraver satisfied the pipeline's hard
                // constraints (the stub pipeline declares none, so vacuously).
                let report = round_trip_with(&score, &Engraver::default());
                assert_eq!(report.status, SolveStatus::Solved);
            }
        }

        // Non-vacuity: the contract above held *through* a genuine re-spacing — the
        // Engraver's geometry differs from the stub's verbatim columns, so provenance
        // survived a real geometry change rather than a pass-through.
        let constrained = to_constrained(&to_logical(&valid_score_rich(11)));
        assert!(constrained.glyphs.len() >= 2);
        let engraved = Engraver::default()
            .solve(&constrained, &SolverConfig::default())
            .layout;
        let stub = StubSolver
            .solve(&constrained, &SolverConfig::default())
            .layout;
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
            "the Engraver must re-space, not echo the stub's verbatim columns"
        );
    }

    /// A repeat-morphed barline whose measure introduces a time signature: the
    /// digits sit in the barline's **slot** but right of the *following note
    /// column's source* (`TIME_SIG_X` plus the sign's right extension exceeds
    /// the constrained column step), so a per-glyph interpolated remap would
    /// drag them by the wrong interval and collapse them into the following
    /// note. The slot-relative remap keeps them with their barline: resolved,
    /// every digit clears both the repeat sign and every notehead, and every
    /// same-slot pair keeps its exact constrained offset.
    #[test]
    fn time_signature_digits_ride_their_barline_slot_past_a_repeat_sign() {
        use epiphany_core::{
            BeatGroup, MusicalDuration, PowerOfTwo, RationalTime, TimeSignature,
            TimeSignatureDisplay, TimeSignatureId, TypedObjectId,
        };
        use epiphany_layout_ir::{Margins, Size2D};

        let mut score = epiphany_testkit::fixtures::ten_measure_with_repeats(0x000A_11CE);
        let ts_id: TimeSignatureId = score.identity.mint();
        let beat = || BeatGroup {
            duration: MusicalDuration(RationalTime::new(1, 4).expect("nonzero")),
            subdivision: None,
            accent: 1,
        };
        score.time_signatures.push(
            TimeSignature::new(
                ts_id,
                TimeSignatureDisplay::Standard {
                    numerator: 4,
                    denominator: PowerOfTwo::new(4).expect("4 is a power of two"),
                },
                MusicalDuration(RationalTime::new(1, 1).expect("nonzero")),
                vec![beat(), beat(), beat(), beat()],
            )
            .expect("4/4 beat groups sum to a whole note"),
        );
        // Measure 2 is the fixture's repeatLeft morph (the first repeat's start).
        score.canvas.regions[0]
            .content
            .staff_instances_mut()
            .expect("the fixture is staff-based")[0]
            .measures[1]
            .time_signature = Some(ts_id);

        let constrained = to_constrained(&to_logical(&score));
        // An unbounded page keeps everything on one endless system, so
        // x-disjointness below compares glyphs of the same line (casting
        // restarts x per system, making cross-system x-overlap legitimate).
        let layout = Engraver::with_geometry(PageGeometry {
            size: Size2D::default(),
            margins: Margins::default(),
        })
        .solve(&constrained, &SolverConfig::default())
        .layout;
        let interval = |position: f32, bounding_box: &epiphany_layout_ir::BoundingBox| {
            (
                position + bounding_box.left.0,
                position + bounding_box.right.0,
            )
        };

        let digits: Vec<_> = layout
            .glyphs
            .iter()
            .filter(|g| {
                g.glyph.as_str().starts_with("timeSig")
                    && matches!(g.provenance.source, TypedObjectId::Measure(_))
            })
            .collect();
        assert!(!digits.is_empty(), "the 4/4 draws digit glyphs");
        let sign = layout
            .glyphs
            .iter()
            .find(|g| {
                g.glyph.as_str() == "repeatLeft"
                    && matches!(g.provenance.source, TypedObjectId::Measure(_))
            })
            .expect("the morphed start sign is engraved");
        let (_, sign_right) = interval(sign.position.x.0, &sign.bounding_box);
        for digit in &digits {
            let (digit_left, digit_right) = interval(digit.position.x.0, &digit.bounding_box);
            assert!(
                digit_left >= sign_right,
                "a digit must clear the repeat sign's ink"
            );
            for head in layout
                .glyphs
                .iter()
                .filter(|g| g.glyph.as_str().starts_with("notehead"))
            {
                let (head_left, head_right) = interval(head.position.x.0, &head.bounding_box);
                assert!(
                    digit_right <= head_left || head_right <= digit_left,
                    "a digit must not cross a notehead's ink"
                );
            }
        }

        // The invariant behind the fix: same-slot companions keep their exact
        // constrained offsets through the re-spacing.
        let resolved: Vec<_> = constrained.glyphs.iter().zip(&layout.glyphs).collect();
        for (a, ra) in &resolved {
            for (b, rb) in &resolved {
                if a.horizontal_slot == b.horizontal_slot {
                    let before = b.baseline.x.0 - a.baseline.x.0;
                    let after = rb.position.x.0 - ra.position.x.0;
                    assert!(
                        (before - after).abs() < 1e-4,
                        "intra-slot offsets must survive the re-spacing"
                    );
                }
            }
        }
    }

    /// The editing-loop vertical slice (testkit's `run_edit_loop_with`) driven
    /// through the **real Engraver**: click a notehead, sharpen its pitch, re-space
    /// with the Engraver, and confirm the selection survives. The selection is the
    /// `MUSCLOID` layout id (content-independent), so it holds even though the
    /// Engraver re-spaces every glyph — proving the slice's "real Engraver too"
    /// claim, not just the stub's.
    #[test]
    fn the_editing_loop_holds_through_the_real_engraver() {
        use epiphany_core::generators::valid_score_rich;
        for seed in 0..16u64 {
            let report = epiphany_testkit::editloop::run_edit_loop_with(
                &valid_score_rich(seed),
                &Engraver::default(),
            )
            .unwrap_or_else(|| panic!("seed {seed}: no clickable notehead to drive the loop"));
            assert!(report.graph_changed, "seed {seed}: graph unchanged");
            assert!(
                report.selection_preserved,
                "seed {seed}: selection lost across the Engraver's relayout"
            );
            assert!(report.render_changed, "seed {seed}: edit not visible");
        }
    }

    // ---- Casting-off (system breaking, stacking, page assignment) ----------

    /// The QUICKSTART ten-measure hand-off fixture through the default page
    /// geometry — the honest multi-system case the goldens lock.
    fn ten_measure_constrained() -> ConstrainedLayoutIR {
        to_constrained(&to_logical(
            &epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
        ))
    }

    #[test]
    fn greedy_wrap_breaks_at_measure_boundaries() {
        let input = ten_measure_constrained();
        let engraver = Engraver::default();
        let report = engraver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);

        let layout = &report.layout;
        assert_eq!(layout.pages.len(), 1, "two short systems fit one page");
        let systems = &layout.pages[0].systems;
        assert!(
            systems.len() >= 2,
            "the ten-measure fixture (≈99 staff spaces) wraps under the \
             default 90-staff-space content width; got {} system(s)",
            systems.len()
        );
        // Every wrapped system fits the content width (no measure in this
        // fixture is wider than a page), and every system starts at the left
        // content margin.
        let geometry = engraver.geometry();
        for system in systems {
            assert!(
                system.bounding_box.size.width.0 <= geometry.content_width() + 1e-3,
                "an automatically wrapped system must fit the content width"
            );
            assert!(
                (system.bounding_box.origin.x.0 - geometry.margins.left.0).abs() < 1e-3,
                "every system starts at the left content margin"
            );
        }
        // The greedy pass breaks at measure boundaries only: each wrapped
        // system after the first begins with a barline column.
        for system in &systems[1..] {
            let top = system.bounding_box.origin.y.0 + system.bounding_box.size.height.0;
            let bottom = system.bounding_box.origin.y.0;
            let first_glyph = layout
                .glyphs
                .iter()
                .filter(|g| g.position.y.0 >= bottom - 1e-3 && g.position.y.0 <= top + 1e-3)
                .min_by(|a, b| a.position.x.0.total_cmp(&b.position.x.0))
                .expect("a wrapped system has glyphs");
            assert!(
                first_glyph.glyph.as_str().starts_with("barline"),
                "a greedy system boundary sits at a measure boundary, got {}",
                first_glyph.glyph.as_str()
            );
        }
        // One Automatic engraved decision per chosen boundary, *appended* to
        // the pipeline's own decisions (which are carried through unchanged).
        let appended = layout
            .engraving_decisions
            .iter()
            .filter(|d| !input.engraving_decisions.contains(d))
            .collect::<Vec<_>>();
        assert_eq!(appended.len(), systems.len() - 1);
        assert!(appended.iter().all(|d| {
            d.kind == epiphany_layout_ir::EngravingDecisionKind::SystemBreak
                && d.source == epiphany_layout_ir::DecisionSource::Automatic
        }));
    }

    #[test]
    fn a_hard_page_break_starts_a_new_page() {
        use epiphany_layout_ir::{BreakKind, DecisionSource, EngravingDecisionKind};
        let mut input = two_off_staff_whole_notes();
        let slot = input.horizontal_slots[2].id; // the second note column
        input.constraints.push(LayoutConstraint::PageBreakAt {
            slot,
            kind: BreakKind::Hard,
        });
        let engraver = Engraver::default();
        let report = engraver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
        assert!(report.satisfied_hard_constraints);
        assert_eq!(
            report.layout.pages.len(),
            2,
            "the hard page break paginates"
        );
        assert_eq!(report.layout.pages[0].number, 1);
        assert_eq!(report.layout.pages[1].number, 2);
        assert_eq!(report.layout.pages[0].systems.len(), 1);
        assert_eq!(report.layout.pages[1].systems.len(), 1);
        // Page 2's system sits inside page 2's world frame (a full page height
        // plus the inter-page gap below page 1's frame).
        let geometry = engraver.geometry();
        let page2_content_top =
            -(geometry.size.height.0 + crate::INTER_PAGE_GAP) - geometry.margins.top.0;
        let system2 = &report.layout.pages[1].systems[0];
        let system2_top = system2.bounding_box.origin.y.0 + system2.bounding_box.size.height.0;
        assert!(
            (system2_top - page2_content_top).abs() < 1e-3,
            "page 2's first system starts at page 2's content top \
             ({system2_top} vs {page2_content_top})"
        );
        assert!(report
            .layout
            .engraving_decisions
            .iter()
            .any(|d| d.kind == EngravingDecisionKind::PageBreak
                && d.source == DecisionSource::Automatic));
    }

    #[test]
    fn vertical_stacking_respects_the_inter_system_gap() {
        use epiphany_layout_ir::{VerticalBand, VerticalBandId};
        let report =
            Engraver::default().solve(&ten_measure_constrained(), &SolverConfig::default());
        let systems = &report.layout.pages[0].systems;
        assert!(systems.len() >= 2);
        // The gap between consecutive systems' real extents is exactly the
        // vertical-band model's preferred inter-system gap.
        let preferred = VerticalBand::inter_system_gap(VerticalBandId(0))
            .preferred_height
            .0;
        for pair in systems.windows(2) {
            let upper_bottom = pair[0].bounding_box.origin.y.0;
            let lower_top = pair[1].bounding_box.origin.y.0 + pair[1].bounding_box.size.height.0;
            let gap = upper_bottom - lower_top;
            assert!(
                (gap - preferred).abs() < 1e-3,
                "inter-system gap {gap} != preferred {preferred}"
            );
        }
    }

    #[test]
    fn page_overflow_starts_a_second_page() {
        use epiphany_layout_ir::{Margins, Size2D, StaffSpace};
        // A deliberately small page: 50×10 staff spaces of content, so the
        // ten-measure fixture wraps into systems (≈7 staff spaces tall) of
        // which only one fits a page — the multi-page path.
        let geometry = PageGeometry {
            size: Size2D {
                width: StaffSpace(60.0),
                height: StaffSpace(20.0),
            },
            margins: Margins {
                top: StaffSpace(5.0),
                right: StaffSpace(5.0),
                bottom: StaffSpace(5.0),
                left: StaffSpace(5.0),
            },
        };
        let engraver = Engraver::with_geometry(geometry);
        let report = engraver.solve(&ten_measure_constrained(), &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
        let pages = &report.layout.pages;
        assert!(pages.len() >= 2, "a 10-staff-space content page overflows");
        for (index, page) in pages.iter().enumerate() {
            assert_eq!(page.number, index as u32 + 1, "page numbers are 1-based");
            assert!(!page.systems.is_empty(), "no page is emitted empty");
            assert_eq!(page.size, geometry.size);
            assert_eq!(page.margins, geometry.margins);
            // Every system lies within its page's content frame.
            let page_top = -(index as f32) * (geometry.size.height.0 + crate::INTER_PAGE_GAP);
            let content_top = page_top - geometry.margins.top.0;
            let content_bottom = content_top - geometry.content_height();
            for system in &page.systems {
                let top = system.bounding_box.origin.y.0 + system.bounding_box.size.height.0;
                let bottom = system.bounding_box.origin.y.0;
                assert!(
                    top <= content_top + 1e-3 && bottom >= content_bottom - 1e-3,
                    "system [{bottom}, {top}] escapes page {} content \
                     [{content_bottom}, {content_top}]",
                    page.number
                );
            }
        }
    }

    #[test]
    fn vertical_justification_fills_non_final_pages() {
        use epiphany_layout_ir::{Margins, Size2D, StaffSpace};
        // Narrow and short, so the ten-measure fixture wraps into several
        // systems with two-plus fitting a page — a non-final page to justify.
        let geometry = PageGeometry {
            size: Size2D {
                width: StaffSpace(40.0),
                height: StaffSpace(30.0),
            },
            margins: Margins {
                top: StaffSpace(5.0),
                right: StaffSpace(5.0),
                bottom: StaffSpace(5.0),
                left: StaffSpace(5.0),
            },
        };
        let engraver = Engraver::with_geometry(geometry);
        let report = engraver.solve(&ten_measure_constrained(), &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
        let pages = &report.layout.pages;
        assert!(
            pages.len() >= 2,
            "the fixture spans pages; got {}",
            pages.len()
        );

        let content_bottom_of = |index: usize| {
            let page_top = -(index as f32) * (geometry.size.height.0 + crate::INTER_PAGE_GAP)
                - geometry.margins.top.0;
            page_top - geometry.content_height()
        };
        // Every non-final page with ≥2 systems fills: its last system's bottom
        // lands on the content bottom.
        let mut justified = 0;
        for (index, page) in pages.iter().enumerate() {
            if index + 1 == pages.len() || page.systems.len() < 2 {
                continue;
            }
            let last_bottom = page.systems.last().unwrap().bounding_box.origin.y.0;
            assert!(
                (last_bottom - content_bottom_of(index)).abs() < 1e-2,
                "page {} last-system bottom {last_bottom} should reach content \
                 bottom {}",
                page.number,
                content_bottom_of(index)
            );
            justified += 1;
        }
        assert!(
            justified > 0,
            "no non-final page carried ≥2 systems to justify"
        );
        // The LAST page stays ragged-bottom — its last system's bottom sits
        // above the content bottom, not force-filled.
        let last_index = pages.len() - 1;
        let last_bottom = pages[last_index]
            .systems
            .last()
            .unwrap()
            .bounding_box
            .origin
            .y
            .0;
        assert!(
            last_bottom > content_bottom_of(last_index) + 1.0,
            "the last page is ragged-bottom: {last_bottom} vs {}",
            content_bottom_of(last_index)
        );
    }

    #[test]
    fn the_resolved_page_tree_is_populated() {
        use std::collections::BTreeSet;
        let report =
            Engraver::default().solve(&ten_measure_constrained(), &SolverConfig::default());
        let page = &report.layout.pages[0];
        assert_eq!(page.number, 1);
        assert!(page.size.width.0 > 0.0 && page.size.height.0 > 0.0);
        assert!(page.free_objects.is_empty());

        let mut system_ids = BTreeSet::new();
        let mut measure_ids = BTreeSet::new();
        let mut measure_records = 0usize;
        let mut previous_top = f32::INFINITY;
        for system in &page.systems {
            // Real, ordered bounding boxes: non-default, stacked top to bottom.
            assert!(system.bounding_box.size.width.0 > 0.0);
            assert!(system.bounding_box.size.height.0 > 0.0);
            let top = system.bounding_box.origin.y.0 + system.bounding_box.size.height.0;
            assert!(top < previous_top, "systems are ordered top to bottom");
            previous_top = top;
            assert!(
                system_ids.insert(system.provenance.stable_id),
                "each system has a distinct stable id"
            );
            // One staff record (the fixture is single-staff), spanning the
            // system and standing four staff spaces tall (plus line thickness).
            assert_eq!(system.staves.len(), 1);
            let staff = &system.staves[0];
            let staff_height = staff.bounding_box.size.height.0;
            assert!(
                (4.0..4.5).contains(&staff_height),
                "a five-line staff spans four staff spaces, got {staff_height}"
            );
            assert!(staff.bounding_box.size.width.0 > 0.0);
            // Measure records: within the system box, ordered by x, distinct.
            let mut previous_x = f32::NEG_INFINITY;
            for measure in &system.measures {
                measure_records += 1;
                assert!(measure_ids.insert(measure.measure), "measures are distinct");
                let x = measure.bounding_box.origin.x.0;
                assert!(x > previous_x, "measures are ordered by x");
                previous_x = x;
                assert!(measure.bounding_box.size.width.0 > 0.0);
                assert!(
                    x >= system.bounding_box.origin.x.0 - 1e-3
                        && x + measure.bounding_box.size.width.0
                            <= system.bounding_box.origin.x.0
                                + system.bounding_box.size.width.0
                                + 1e-3,
                    "a measure record lies within its system"
                );
            }
        }
        // Nine of the fixture's ten measures are marked by a start barline
        // column (the final-barline measure's start is not marked by any
        // column in this projection, so its record is honestly omitted).
        assert_eq!(measure_records, 9);
    }

    #[test]
    fn casting_off_is_deterministic_byte_for_byte() {
        // Chapter 9 determinism over the full multi-system output: two solves
        // of the wrapping fixture produce byte-identical canonical layouts.
        let input = ten_measure_constrained();
        let a = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .layout;
        let b = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .layout;
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    }

    #[test]
    fn staff_lines_are_split_per_system_with_synthesized_continuations() {
        use epiphany_layout_ir::SynthesisKind;
        let input = ten_measure_constrained();
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        let systems = &report.layout.pages[0].systems;
        assert!(systems.len() >= 2);
        // Five lines of one staff, one segment per system: the first segment of
        // each keeps the original stroke's provenance; each later one is
        // synthesized under the continuation registry kind.
        let continuations = report
            .layout
            .strokes
            .iter()
            .filter(|s| {
                s.provenance.synthesis
                    == Some(SynthesisKind::Registered(SYSTEM_CONTINUATION_SYNTHESIS))
            })
            .count();
        assert_eq!(
            continuations,
            5 * (systems.len() - 1),
            "one synthesized continuation per staff line per later system"
        );
        // Every input stroke's provenance survives (the first segments).
        for stroke in &input.strokes {
            assert!(
                report
                    .layout
                    .strokes
                    .iter()
                    .any(|s| s.provenance == stroke.provenance),
                "an input stroke's provenance was lost in the split"
            );
        }
        // Each system's staff-line segments stay within their system's box.
        for system in systems {
            let staff = &system.staves[0];
            let box_left = system.bounding_box.origin.x.0;
            let box_right = box_left + system.bounding_box.size.width.0;
            assert!(staff.bounding_box.origin.x.0 >= box_left - 1e-3);
            assert!(
                staff.bounding_box.origin.x.0 + staff.bounding_box.size.width.0 <= box_right + 1e-3
            );
        }
    }

    #[test]
    fn hit_testing_resolves_a_glyph_in_the_second_system() {
        use epiphany_layout_ir::{to_render, HitShape, Point, PrimitiveRef};
        let input = ten_measure_constrained();
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        let first_system_bottom = report.layout.pages[0].systems[0].bounding_box.origin.y.0;
        // A real notehead that wrapped into a later system (below the first).
        let (index, glyph) = report
            .layout
            .glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                g.glyph.as_str().starts_with("notehead") && g.provenance.synthesis.is_none()
            })
            .min_by(|a, b| a.1.position.y.0.total_cmp(&b.1.position.y.0))
            .expect("the fixture has noteheads");
        assert!(
            glyph.position.y.0 < first_system_bottom,
            "the lowest notehead sits below the first system (it wrapped)"
        );
        // The baked world frame is the hit-test frame: clicking its box centre
        // resolves to the same glyph and its score-graph source.
        let render = to_render(&report.layout);
        let map = render.hit_test_map();
        let region = map
            .regions
            .iter()
            .find(|r| r.primitive == PrimitiveRef::Glyph(index))
            .expect("every glyph has a hit region");
        let HitShape::Box(bounds) = region.shape else {
            panic!("a glyph hit region is a box");
        };
        let click = Point::new(
            (bounds.left.0 + bounds.right.0) / 2.0,
            (bounds.bottom.0 + bounds.top.0) / 2.0,
        );
        let top = map.hit(click).into_iter().next().expect("the click hits");
        assert_eq!(top.layout_object, glyph.provenance.stable_id);
        assert_eq!(top.source, glyph.provenance.source);
    }
}
