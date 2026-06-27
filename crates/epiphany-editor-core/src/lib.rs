#![forbid(unsafe_code)]
//! # epiphany-editor-core
//!
//! A **headless editor core** over an Epiphany score: the API a GUI calls to drive
//! the editing loop, with no UI and no rendering backend of its own. It packages
//! the proven vertical slice — hit-test → score object → operation → reduce →
//! re-layout → re-render → re-resolve selection — behind [`EditorSession`].
//!
//! It owns:
//!
//! * **selection state** ([`Selection`]) — the score object and the stable layout
//!   id that anchors it across relayouts;
//! * the current **render and hit-test query** ([`EditorSession::render`],
//!   [`EditorSession::hit_test`], [`EditorSession::click`]);
//! * **operation minting** ([`EditorSession::apply`] and intents like
//!   [`EditorSession::transpose_selection`]) — the caller supplies an
//!   [`OperationKind`] or an intent, and the session assembles the
//!   [`OperationEnvelope`] (id, author, stamp, causal context) so a GUI never
//!   hand-rolls the envelope bookkeeping;
//! * **apply / re-render** — reduce the operation onto the score, re-render, and
//!   refuse a diagnostic-only (non-renderable) layout, leaving the document
//!   unchanged if the edit would not render;
//! * **selection preservation** — re-resolve the selection against the new layout,
//!   keeping it when its layout object survives (the cursor does not jump off the
//!   edited object) and clearing it when the object is gone.
//!
//! The session is **solver-agnostic**: it holds a `Box<dyn ConstraintSolver>`, so a
//! GUI plugs in the real `Engraver`, the stub, or any conformant solver. It
//! produces a [`RenderIR`]; turning that into pixels is the renderer's job.

use std::fmt;

use epiphany_core::{OperationId, ReplicaId, Score, TypedObjectId, WallClockTime};
use epiphany_layout_ir::{
    to_constrained, to_logical, to_render, ConstraintSolver, HitTestMap, LayoutObjectId, Point,
    RenderIR, SolverConfig,
};
use epiphany_ops::{
    AcceptOutcome, AuthorId, CausalContext, DeleteEventOp, DeleteIdentifiedPitchOp,
    HybridLogicalClock, OperationEnvelope, OperationKind, OperationPayload, OperationSet,
    OperationStamp, TransposeOp, TupletCompensation,
};

/// The current selection: the score-graph object to act on, plus the stable layout
/// object id that anchors it across relayouts (so it survives an edit's re-render).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Selection {
    /// The score-graph object selected (what an operation targets).
    pub source: TypedObjectId,
    /// The layout object the selection is anchored on — content-independent, so it
    /// survives a relayout of an unchanged source.
    pub layout_object: LayoutObjectId,
}

/// What an [`EditorSession::apply`] did.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct EditOutcome {
    /// The operation changed the score graph.
    pub graph_changed: bool,
    /// The selection survived the relayout (its layout object still exists).
    pub selection_preserved: bool,
}

/// An editing error. None of these mutate the session.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EditorError {
    /// The solver returned a diagnostic-only (non-renderable) layout — the edit is
    /// rejected and the document is left unchanged.
    NotRenderable,
    /// The minted operation was not accepted by the reducer (it was not
    /// well-formed — e.g. minted under the reserved [`epiphany_core::ReplicaId::SYSTEM_DERIVED`]
    /// identity). The edit is dropped rather than silently no-op'd.
    RejectedOperation,
    /// An intent needed a selection but none is set.
    NoSelection,
    /// The selection is not the kind the intent requires (e.g. a transpose needs a
    /// pitch selection).
    WrongSelection {
        /// The kind of object the intent expected.
        expected: &'static str,
    },
}

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorError::NotRenderable => {
                f.write_str("the resulting layout is diagnostic-only and cannot be rendered")
            }
            EditorError::RejectedOperation => {
                f.write_str("the minted operation was not well-formed and was rejected")
            }
            EditorError::NoSelection => f.write_str("no selection"),
            EditorError::WrongSelection { expected } => {
                write!(f, "the selection is not a {expected}")
            }
        }
    }
}

impl std::error::Error for EditorError {}

/// A headless editor session over a score. A GUI opens one, queries its render and
/// hit-test map to draw and to resolve clicks, and drives edits through it.
pub struct EditorSession {
    score: Score,
    solver: Box<dyn ConstraintSolver>,
    render: RenderIR,
    map: HitTestMap,
    selection: Option<Selection>,
    // Operation-minting context. A real client supplies its own replica/author;
    // the counter and clock advance per minted operation so each gets a fresh,
    // ordered id.
    replica: ReplicaId,
    author: AuthorId,
    op_counter: u64,
}

impl EditorSession {
    /// Opens a session on `score` with `solver`, rendering immediately. Errors with
    /// [`EditorError::NotRenderable`] if the initial layout is diagnostic-only.
    pub fn open(score: Score, solver: Box<dyn ConstraintSolver>) -> Result<Self, EditorError> {
        let (render, map) =
            render_score(&score, solver.as_ref()).ok_or(EditorError::NotRenderable)?;
        Ok(EditorSession {
            score,
            solver,
            render,
            map,
            selection: None,
            replica: ReplicaId(1),
            author: AuthorId(0),
            op_counter: 0,
        })
    }

    /// Overrides the replica/author the session mints operations under (a GUI sets
    /// these to the local editing identity). Defaults to `ReplicaId(1)` / author 0.
    pub fn with_identity(mut self, replica: ReplicaId, author: AuthorId) -> Self {
        self.replica = replica;
        self.author = author;
        self
    }

    /// The current document.
    pub fn score(&self) -> &Score {
        &self.score
    }

    /// The current render, for the GUI to draw.
    pub fn render(&self) -> &RenderIR {
        &self.render
    }

    /// The current hit-test map, for the GUI to resolve clicks and drags.
    pub fn hit_test(&self) -> &HitTestMap {
        &self.map
    }

    /// The current selection, if any.
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Resolves a click at a world `point`: selects the **topmost** hit there (what
    /// a GUI selects), or clears the selection if the point hits nothing. Returns
    /// the new selection.
    pub fn click(&mut self, point: Point) -> Option<Selection> {
        self.selection = self.map.hit(point).into_iter().next().map(|r| Selection {
            source: r.source,
            layout_object: r.layout_object,
        });
        self.selection
    }

    /// Selects a layout object by id (a programmatic / restored selection), if it is
    /// present in the current layout. Returns the new selection.
    pub fn select(&mut self, layout_object: LayoutObjectId) -> Option<Selection> {
        self.selection = self
            .map
            .regions
            .iter()
            .find(|r| r.layout_object == layout_object)
            .map(|r| Selection {
                source: r.source,
                layout_object,
            });
        self.selection
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Builds an [`OperationEnvelope`] for a primitive `kind` under the session's
    /// identity at operation `counter` — the bookkeeping (id, author, stamp, causal
    /// context) a GUI would otherwise assemble by hand. Pure: it does not advance
    /// the session's counter, so a failed [`Self::apply`] consumes no id.
    fn envelope_for(&self, counter: u64, kind: OperationKind) -> OperationEnvelope {
        let id = OperationId::new(self.replica, counter);
        OperationEnvelope {
            id,
            author: self.author,
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(counter as i64), 0),
                id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(kind),
        }
    }

    /// Applies a primitive operation: mints an envelope, reduces it onto the score,
    /// re-renders, and re-resolves the selection. **Atomic**: on any error — a
    /// rejected (not well-formed) operation, or a diagnostic-only layout — the
    /// session is left entirely unchanged, including its operation counter.
    pub fn apply(&mut self, kind: OperationKind) -> Result<EditOutcome, EditorError> {
        let counter = self.op_counter + 1;
        let envelope = self.envelope_for(counter, kind);

        // A minted operation that the reducer will not accept (e.g. a reserved
        // replica identity) must not silently no-op the edit.
        let mut set = OperationSet::new();
        if !matches!(set.accept(envelope), AcceptOutcome::Accepted) {
            return Err(EditorError::RejectedOperation);
        }
        let edited = set.reduce_onto(&self.score).score;
        let graph_changed = edited != self.score;

        // Refuse a diagnostic-only layout, still before committing anything.
        let (render, map) =
            render_score(&edited, self.solver.as_ref()).ok_or(EditorError::NotRenderable)?;

        // Commit (the only mutation point — so an error above leaves all state,
        // counter included, untouched).
        self.op_counter = counter;
        self.score = edited;
        self.render = render;
        self.map = map;

        let selection_preserved = self.reresolve_selection();
        Ok(EditOutcome {
            graph_changed,
            selection_preserved,
        })
    }

    /// Transposes the selected pitch by `chromatic_steps` (a `+1` is a sharpen).
    /// Errors if nothing — or a non-pitch — is selected.
    pub fn transpose_selection(
        &mut self,
        chromatic_steps: i32,
    ) -> Result<EditOutcome, EditorError> {
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let TypedObjectId::Pitch(pitch) = selection.source else {
            return Err(EditorError::WrongSelection { expected: "pitch" });
        };
        self.apply(OperationKind::Transpose(TransposeOp {
            targets: vec![pitch],
            chromatic_steps,
        }))
    }

    /// Deletes the selected object. A selected **pitch** (a notehead) is tombstoned
    /// — the note's last pitch degrades its event to a rest of the same duration, so
    /// the rhythm survives; a selected **event** (a rest, a stem) is deleted whole.
    /// Errors if the selection is neither. The selection does not survive (its layout
    /// object is gone), so it is cleared.
    pub fn delete_selection(&mut self) -> Result<EditOutcome, EditorError> {
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let kind = match selection.source {
            TypedObjectId::Pitch(pitch) => {
                OperationKind::DeleteIdentifiedPitch(DeleteIdentifiedPitchOp { pitch })
            }
            TypedObjectId::Event(event) => OperationKind::DeleteEvent(DeleteEventOp {
                event,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }),
            _ => {
                return Err(EditorError::WrongSelection {
                    expected: "pitch or event",
                })
            }
        };
        self.apply(kind)
    }

    /// Re-resolves the current selection against the current layout: keeps it
    /// (refreshing its source) when its layout object survives, clears it otherwise.
    /// Returns whether it survived.
    fn reresolve_selection(&mut self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        match self
            .map
            .regions
            .iter()
            .find(|r| r.layout_object == selection.layout_object)
        {
            Some(region) => {
                self.selection = Some(Selection {
                    source: region.source,
                    layout_object: selection.layout_object,
                });
                true
            }
            None => {
                self.selection = None;
                false
            }
        }
    }
}

/// Renders a score with `solver` to its `RenderIR` + hit-test map, or `None` if the
/// solver's report is diagnostic-only (not renderable).
fn render_score(score: &Score, solver: &dyn ConstraintSolver) -> Option<(RenderIR, HitTestMap)> {
    let report = solver.solve(
        &to_constrained(&to_logical(score)),
        &SolverConfig::default(),
    );
    if !report.status.is_renderable() {
        return None;
    }
    let render = to_render(&report.layout);
    let map = render.hit_test_map();
    Some((render, map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{
        ConstrainedLayoutIR, HitShape, InvalidationSet, SolveReport, SolveStatus, SolverState,
        SolverTier, SolverVersion, StubSolver,
    };

    fn open_rich(seed: u64) -> EditorSession {
        EditorSession::open(valid_score_rich(seed), Box::new(StubSolver)).expect("rich renders")
    }

    /// Clicks the centre of the first notehead (a pitch-backed glyph) and returns the
    /// resulting selection.
    fn click_a_notehead(session: &mut EditorSession) -> Selection {
        let click = session
            .hit_test()
            .regions
            .iter()
            .filter(|r| r.primitive.is_glyph() && matches!(r.source, TypedObjectId::Pitch(_)))
            .find_map(|r| match r.shape {
                HitShape::Box(b) => Some(Point::new(
                    (b.left.0 + b.right.0) / 2.0,
                    (b.bottom.0 + b.top.0) / 2.0,
                )),
                HitShape::Segment { .. } => None,
            })
            .expect("the rich fixture renders a notehead");
        session.click(click).expect("the click selects a glyph")
    }

    #[test]
    fn open_renders_and_starts_unselected() {
        let session = open_rich(0x5EED);
        assert!(!session.render().primitives.is_empty(), "the score renders");
        assert!(!session.hit_test().regions.is_empty(), "with hit regions");
        assert_eq!(session.selection(), None, "nothing is selected at open");
    }

    #[test]
    fn a_click_selects_the_topmost_hit() {
        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        assert!(matches!(selection.source, TypedObjectId::Pitch(_)));
        assert_eq!(session.selection(), Some(selection));
        // Re-selecting that layout object by id resolves to the same thing.
        assert_eq!(session.select(selection.layout_object), Some(selection));
        // A click on empty space clears the selection.
        session.click(Point::new(-1.0e6, -1.0e6));
        assert_eq!(session.selection(), None);
    }

    #[test]
    fn the_full_editing_loop_runs_through_the_session() {
        let mut session = open_rich(0x5EED);
        let before = session.render().clone();

        // Click a notehead, then sharpen the selected pitch — minting the operation
        // is the session's job, not the caller's.
        let selection = click_a_notehead(&mut session);
        let outcome = session.transpose_selection(1).expect("the sharpen applies");

        assert!(outcome.graph_changed, "the edit reduced onto the graph");
        assert!(
            outcome.selection_preserved,
            "the selection survived the relayout"
        );
        assert_eq!(
            session.selection().map(|s| s.layout_object),
            Some(selection.layout_object),
            "the selection still anchors the same layout object"
        );
        assert_ne!(&before, session.render(), "the re-render shows the edit");
    }

    #[test]
    fn transpose_requires_a_pitch_selection() {
        let mut session = open_rich(0x5EED);
        // Nothing selected.
        assert_eq!(
            session.transpose_selection(1),
            Err(EditorError::NoSelection)
        );
        // Select a non-pitch object (a region, present in any score) and try again.
        let non_pitch = session
            .hit_test()
            .regions
            .iter()
            .find(|r| !matches!(r.source, TypedObjectId::Pitch(_)))
            .map(|r| r.layout_object);
        if let Some(id) = non_pitch {
            session.select(id);
            assert_eq!(
                session.transpose_selection(1),
                Err(EditorError::WrongSelection { expected: "pitch" })
            );
        }
    }

    #[test]
    fn two_edits_in_a_row_both_apply() {
        // Distinct minted op ids, so the second edit is not deduplicated.
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        assert!(session.transpose_selection(1).unwrap().graph_changed);
        let mid = session.score().clone();
        assert!(session.transpose_selection(1).unwrap().graph_changed);
        assert_ne!(
            &mid,
            session.score(),
            "the second edit also changed the graph"
        );
    }

    /// A solver whose report is diagnostic-only (`Unsatisfiable`) yet carries a
    /// non-empty layout — which the editor must refuse.
    struct UnsatisfiableSolver;

    impl ConstraintSolver for UnsatisfiableSolver {
        fn tier(&self) -> SolverTier {
            SolverTier::Minimal
        }
        fn version(&self) -> SolverVersion {
            SolverVersion(99)
        }
        fn solve(&self, input: &ConstrainedLayoutIR, config: &SolverConfig) -> SolveReport {
            let mut report = StubSolver.solve(input, config);
            report.status = SolveStatus::Unsatisfiable;
            report.satisfied_hard_constraints = false;
            report
        }
        fn solve_incremental(
            &self,
            input: &ConstrainedLayoutIR,
            _prior: &SolverState,
            _invalidations: &InvalidationSet,
            config: &SolverConfig,
        ) -> SolveReport {
            self.solve(input, config)
        }
    }

    #[test]
    fn a_diagnostic_only_layout_is_refused_at_open() {
        let opened = EditorSession::open(valid_score_rich(0x5EED), Box::new(UnsatisfiableSolver));
        assert_eq!(opened.err(), Some(EditorError::NotRenderable));
    }

    #[test]
    fn delete_selection_removes_the_note_and_drops_the_selection() {
        let mut session = open_rich(0x5EED);
        let before = session.render().clone();
        let selection = click_a_notehead(&mut session);

        let outcome = session.delete_selection().expect("the delete applies");
        assert!(outcome.graph_changed, "the note was tombstoned");
        // The selected pitch is gone (its event degraded to a rest or lost a chord
        // note), so its layout object no longer exists and the selection is cleared.
        assert!(!outcome.selection_preserved);
        assert_eq!(session.selection(), None);
        assert!(!session
            .hit_test()
            .regions
            .iter()
            .any(|r| r.layout_object == selection.layout_object));
        assert_ne!(&before, session.render(), "the delete changed the render");
    }

    #[test]
    fn delete_selection_requires_a_selection() {
        let mut session = open_rich(0x5EED);
        assert_eq!(session.delete_selection(), Err(EditorError::NoSelection));
    }

    #[test]
    fn a_rejected_operation_is_an_error_not_a_silent_no_op() {
        use epiphany_core::ReplicaId;
        // Minting under the reserved replica makes every operation ill-formed.
        let mut session = EditorSession::open(valid_score_rich(0x5EED), Box::new(StubSolver))
            .unwrap()
            .with_identity(ReplicaId::SYSTEM_DERIVED, AuthorId(0));
        click_a_notehead(&mut session);
        let before = session.score().clone();

        // The edit is rejected (not a silent Ok/graph_changed=false), and nothing —
        // not even the operation counter — mutates, so a later valid edit still
        // works.
        assert_eq!(
            session.transpose_selection(1),
            Err(EditorError::RejectedOperation)
        );
        assert_eq!(
            &before,
            session.score(),
            "a rejected edit leaves the document untouched"
        );
    }
}
