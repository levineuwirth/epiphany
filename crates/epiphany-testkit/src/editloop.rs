//! The editing-loop vertical slice — the seam between ops, layout, and render.
//!
//! One iteration of an interactive edit, end to end: render a score to its
//! `RenderIR`, **click** a notehead and resolve it to its graph pitch through the
//! hit-test map, **apply** a real operation (sharpen — a `+1`-chromatic
//! `Transpose`), **reduce** it onto the score graph, **re-render**, and confirm the
//! **selection survives** the relayout. This walks the exact path an editor walks —
//! hit-test → score object → operation → reduction → re-layout → re-render →
//! re-resolve selection — across the crate seams, so a missing connection surfaces
//! here rather than in a GUI.
//!
//! Selection survival rests on the `MUSCLOID` layout-object id, which is a function
//! of the score object's *identity*, not its content: the sharpen changes the
//! pitch's value but not its `PitchId`, so its layout object keeps the same stable
//! id and the editor re-finds the selection after the relayout.

use epiphany_core::{
    OperationId, PitchId, ReplicaId, Score, TranspositionInterval, TypedObjectId, WallClockTime,
};
use epiphany_layout_ir::{
    to_constrained, to_logical, to_render, ConstraintSolver, HitShape, LayoutObjectId, Point,
    PrimitiveRef, RenderIR, SolverConfig, StubSolver,
};
use epiphany_ops::{
    AuthorId, CausalContext, HybridLogicalClock, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet, OperationStamp, TransposeIntervalOp,
};

/// What one editing-loop iteration did, for inspection by tests.
#[derive(Clone, Debug)]
pub struct EditLoopReport {
    /// The pitch the click resolved to — the score object that gets selected.
    pub selected_pitch: PitchId,
    /// The layout object the selection is anchored on (its stable id), what an
    /// editor holds across edits.
    pub selection: LayoutObjectId,
    /// The operation changed the score graph (the edit reduced onto it).
    pub graph_changed: bool,
    /// After the relayout, the same layout-object id still resolves — to the same
    /// pitch — so the selection survived (a cursor would not jump off the note).
    pub selection_preserved: bool,
    /// The re-rendered output differs from the original (the edit is visible).
    pub render_changed: bool,
}

/// Renders a score through the v0 pipeline with `solver` to its `RenderIR`, or
/// `None` if the solver's report is **diagnostic-only** (not renderable). A generic
/// [`ConstraintSolver`] may return glyphs in an `Unsatisfiable`/`InternalError`
/// report; that layout must never be rendered, hit-tested, or edited against, so
/// the loop refuses it.
fn render_with<S: ConstraintSolver>(score: &Score, solver: &S) -> Option<RenderIR> {
    let report = solver.solve(
        &to_constrained(&to_logical(score)),
        &SolverConfig::default(),
    );
    report
        .status
        .is_renderable()
        .then(|| to_render(&report.layout))
}

/// Builds a "sharpen" edit — a `+1`-chromatic [`TransposeIntervalOp`] on
/// `pitch` — and reduces it onto `base`, returning the edited score (the same
/// `PitchId`s, the target's CMN alteration shifted by one, its staff line
/// unchanged).
///
/// This models an *editing action*, so it authors the faithful operation. The
/// frozen `Transpose` MUST NOT be emitted by new authoring
/// (`req:opcat:transpose-frozen`); it survives only in the random-kind corpora
/// of `generators.rs`, which exist to prove it still replays.
fn sharpen(base: &Score, pitch: PitchId) -> Score {
    let id = OperationId::new(ReplicaId(1), 1);
    let envelope = OperationEnvelope {
        id,
        author: AuthorId(0),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(1), 0), id),
        causal_context: CausalContext::new(),
        transaction: None,
        payload: OperationPayload::Primitive(OperationKind::TransposeInterval(
            TransposeIntervalOp {
                targets: [pitch].into_iter().collect(),
                interval: TranspositionInterval {
                    diatonic_steps: 0,
                    chromatic_steps: 1,
                },
            },
        )),
    };
    let mut set = OperationSet::new();
    set.accept(envelope);
    set.reduce_onto(base).score
}

/// Runs one editing-loop iteration through the stub solver. See
/// [`run_edit_loop_with`]; this is the stub wrapper.
pub fn run_edit_loop(base: &Score) -> Option<EditLoopReport> {
    run_edit_loop_with(base, &StubSolver)
}

/// Runs one editing-loop iteration on `base` with `solver`: render, click a real
/// notehead, sharpen its pitch, re-render, and check the selection survived.
/// Parameterized over the solver so the same slice exercises both the stub and the
/// real `Engraver` (the latter from a crate that depends on `epiphany-engrave`).
/// Returns `None` if either solver pass reports a diagnostic-only layout, or if
/// the renderable layout has no pitch-backed notehead to click.
pub fn run_edit_loop_with<S: ConstraintSolver>(base: &Score, solver: &S) -> Option<EditLoopReport> {
    let render0 = render_with(base, solver)?;
    let map0 = render0.hit_test_map();

    // 1. Resolve a "click": aim at each real notehead's centre in turn (an actual
    //    notehead glyph, not a synthesized/cautionary mark, backed by a pitch), and
    //    select whatever a GUI would there — the *topmost* hit. Take the first aim
    //    whose topmost hit is a pitch-backed glyph, and derive the selection from
    //    *that* hit: faithful to a real click and robust to an occluding unison or
    //    chord notehead (which a "the aimed glyph must be topmost" check would
    //    false-fail on).
    let (pitch, selection) = render0
        .primitives
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.glyph.as_str().starts_with("notehead")
                && p.provenance.synthesis.is_none()
                && matches!(p.provenance.source, TypedObjectId::Pitch(_))
        })
        .find_map(|(i, _)| {
            let region = map0
                .regions
                .iter()
                .find(|r| r.primitive == PrimitiveRef::Glyph(i))?;
            let HitShape::Box(b) = region.shape else {
                return None; // a glyph region is always a box
            };
            let click = Point::new((b.left.0 + b.right.0) / 2.0, (b.bottom.0 + b.top.0) / 2.0);
            let top = map0.hit(click).into_iter().next()?;
            match (top.primitive.is_glyph(), top.source) {
                (true, TypedObjectId::Pitch(pid)) => Some((pid, top.layout_object)),
                _ => None,
            }
        })?;

    // 2. Apply the edit by reducing the operation onto the score graph.
    let edited = sharpen(base, pitch);
    let graph_changed = &edited != base;

    // 3. Re-render and re-resolve (refusing a diagnostic-only edited layout).
    let render1 = render_with(&edited, solver)?;
    let map1 = render1.hit_test_map();

    // 4. The selection survives: the same stable id still resolves, to the same
    //    pitch. (Its id is content-independent, so the relayout does not move the
    //    cursor off the edited note even though the note's glyphs changed.)
    let selection_preserved = map1
        .regions
        .iter()
        .any(|r| r.layout_object == selection && r.source == TypedObjectId::Pitch(pitch));

    let render_changed = render0 != render1;

    Some(EditLoopReport {
        selected_pitch: pitch,
        selection,
        graph_changed,
        selection_preserved,
        render_changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;

    #[test]
    fn the_sharpen_records_the_spelling_it_propagated() {
        // The frozen `Transpose` recorded none, so an authored spelling stayed
        // pinned to the pre-edit notehead. `TransposeInterval` must attach the
        // spelling its interval determined (`req:opcat:transpose-interval-spelling`).
        use epiphany_core::{SpellingScope, SpellingSource};
        let base = valid_score_rich(0x5EED);
        let pitch = base
            .events
            .iter()
            .find_map(|e| {
                let mut ips = Vec::new();
                e.collect_identified_pitches(&mut ips);
                ips.first().map(|ip| ip.id)
            })
            .expect("the rich fixture has a pitch");
        assert!(
            !base
                .spelling_attachments
                .iter()
                .any(|a| matches!(&a.source, SpellingSource::Propagated { .. })),
            "the fixture starts with no propagated attachment"
        );

        let edited = sharpen(&base, pitch);
        assert!(
            edited.spelling_attachments.iter().any(|a| {
                matches!(a.source, SpellingSource::Propagated { from } if from == pitch)
                    && matches!(&a.scope, SpellingScope::Pitch(p) if *p == pitch)
            }),
            "the sharpen propagated its spelling"
        );
    }

    #[test]
    fn one_editing_loop_iteration_holds_every_seam() {
        // A returned report means the click resolved to a pitch-backed notehead (the
        // hit-test → score-object seam); the fields cover the rest.
        let report = run_edit_loop(&valid_score_rich(0x5EED))
            .expect("the rich fixture renders a clickable notehead");
        // operation → reduction.
        assert!(report.graph_changed, "the sharpen changed the score graph");
        // re-layout → re-render → re-resolve selection.
        assert!(
            report.selection_preserved,
            "the selection survived the relayout (stable layout id)"
        );
        // the edit is visible.
        assert!(
            report.render_changed,
            "the sharpen changed the re-rendered output"
        );
    }

    #[test]
    fn the_seams_hold_across_many_scores() {
        // The rich fixture always renders a clickable notehead, so *every* seed must
        // drive the loop — a regression that left only some seeds clickable must not
        // pass by being silently skipped.
        for seed in 0..48u64 {
            let report = run_edit_loop(&valid_score_rich(seed))
                .unwrap_or_else(|| panic!("seed {seed}: no clickable notehead to drive the loop"));
            assert!(report.graph_changed, "seed {seed}: graph unchanged");
            assert!(
                report.selection_preserved,
                "seed {seed}: selection lost across relayout"
            );
            assert!(report.render_changed, "seed {seed}: edit not visible");
        }
    }

    /// A solver whose report is diagnostic-only (`Unsatisfiable`) but still carries a
    /// non-empty layout — exactly the case a hit-test/edit must not run against.
    struct UnsatisfiableSolver;

    impl ConstraintSolver for UnsatisfiableSolver {
        fn tier(&self) -> epiphany_layout_ir::SolverTier {
            epiphany_layout_ir::SolverTier::Minimal
        }
        fn version(&self) -> epiphany_layout_ir::SolverVersion {
            epiphany_layout_ir::SolverVersion(99)
        }
        fn solve(
            &self,
            input: &epiphany_layout_ir::ConstrainedLayoutIR,
            config: &SolverConfig,
        ) -> epiphany_layout_ir::SolveReport {
            // Borrow the stub's (non-empty) geometry, then mark the report
            // diagnostic-only — a conformant Unsatisfiable layout the caller must
            // not render.
            let mut report = StubSolver.solve(input, config);
            report.status = epiphany_layout_ir::SolveStatus::Unsatisfiable;
            report.satisfied_hard_constraints = false;
            report
        }
        fn solve_incremental(
            &self,
            input: &epiphany_layout_ir::ConstrainedLayoutIR,
            _prior: &epiphany_layout_ir::SolverState,
            _invalidations: &epiphany_layout_ir::InvalidationSet,
            config: &SolverConfig,
        ) -> epiphany_layout_ir::SolveReport {
            self.solve(input, config)
        }
    }

    #[test]
    fn a_diagnostic_only_solver_layout_is_refused() {
        // The unsatisfiable solver returns a non-empty layout, but it is not
        // renderable, so the loop refuses to hit-test or edit against it.
        let report = UnsatisfiableSolver.solve(
            &to_constrained(&to_logical(&valid_score_rich(0x5EED))),
            &SolverConfig::default(),
        );
        assert!(
            !report.layout.glyphs.is_empty(),
            "the fake layout is non-empty"
        );
        assert!(!report.status.is_renderable());
        assert!(
            run_edit_loop_with(&valid_score_rich(0x5EED), &UnsatisfiableSolver).is_none(),
            "a diagnostic-only layout must not drive the editing loop"
        );
    }
}
