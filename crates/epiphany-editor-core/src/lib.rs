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
//!   [`OperationEnvelope`] (id, author, stamp, and a causal context covering the
//!   session's prior edits) so a GUI never hand-rolls the envelope bookkeeping and
//!   so sequential edits to one target read as overwrites, not concurrent conflicts;
//! * **apply / re-render** — reduce the operation onto the score, re-render, and
//!   refuse a diagnostic-only (non-renderable) layout, leaving the document
//!   unchanged if the edit would not render;
//! * **selection preservation** — re-resolve the selection against the new layout,
//!   keeping it when its layout object survives (the cursor does not jump off the
//!   edited object) and clearing it when the object is gone;
//! * an **append-only operation log** ([`EditorSession::applied_operations`],
//!   [`EditorSession::last_applied`]) — every applied envelope, in order, the record
//!   undo, history, and sync build on (each intent feeds it automatically via
//!   `apply`).
//!
//! The session is **solver-agnostic**: it holds a `Box<dyn ConstraintSolver>`, so a
//! GUI plugs in the real `Engraver`, the stub, or any conformant solver. It
//! produces a [`RenderIR`]; turning that into pixels is the renderer's job.

use std::fmt;

use epiphany_core::{
    CmnNominal, IdentifiedPitch, OperationId, Pitch, PitchId, PitchSpacePosition, ReplicaId, Score,
    SpellingDirective, SpellingScope, SpellingSourceKind, TypedObjectId, WallClockTime,
};
use epiphany_layout_ir::{
    to_constrained, to_logical, to_render, ConstraintSolver, HitTestMap, LayoutObjectId, Point,
    RenderIR, SolverConfig,
};
use epiphany_ops::{
    AcceptOutcome, AuthorId, CausalContext, DeleteEventOp, DeleteIdentifiedPitchOp,
    HybridLogicalClock, ModifyIdentifiedPitchOp, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet, OperationStamp, TransposeOp, TupletCompensation,
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
    /// The selected pitch carries an authored spelling override (user-chosen,
    /// imported, or propagated) that outranks the inferred spelling and pins its
    /// rendered staff position. A staff-step move that changed only the pitch value
    /// would move the sound but not the notehead, so it is refused. A respelling-aware
    /// move (atomically rebasing the override) is a follow-up.
    PitchSpellingOverridden,
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
            EditorError::PitchSpellingOverridden => f.write_str(
                "the selected pitch has an authored spelling override that pins its staff position",
            ),
        }
    }
}

impl std::error::Error for EditorError {}

/// A headless editor session over a score. A GUI opens one, queries its render and
/// hit-test map to draw and to resolve clicks, and drives edits through it.
pub struct EditorSession {
    // The pristine open-time score. Every edit reduces the whole applied-op log
    // onto this base, so the session's materialization is exactly the canonical
    // reduction of the operations it emits — which also means a new op's causal
    // predecessors are present in the set being reduced (see `apply`).
    base: Score,
    score: Score,
    solver: Box<dyn ConstraintSolver>,
    render: RenderIR,
    map: HitTestMap,
    selection: Option<Selection>,
    // Operation-minting identity. A real client supplies its own replica/author.
    // Minted operations form this replica's contiguous, zero-based local history;
    // the next op's counter is `applied.len()`, so a failed apply consumes no id.
    replica: ReplicaId,
    author: AuthorId,
    // Append-only log of the envelopes this session has applied, in order — the
    // record undo, sync, and history build on (every intent feeds it via `apply`),
    // and the input the session re-reduces onto `base` on each edit.
    applied: Vec<OperationEnvelope>,
}

impl EditorSession {
    /// Opens a session on `score` with `solver`, rendering immediately. Errors with
    /// [`EditorError::NotRenderable`] if the initial layout is diagnostic-only.
    pub fn open(score: Score, solver: Box<dyn ConstraintSolver>) -> Result<Self, EditorError> {
        let (render, map) =
            render_score(&score, solver.as_ref()).ok_or(EditorError::NotRenderable)?;
        Ok(EditorSession {
            base: score.clone(),
            score,
            solver,
            render,
            map,
            selection: None,
            replica: ReplicaId(1),
            author: AuthorId(0),
            applied: Vec::new(),
        })
    }

    /// Overrides the replica/author the session mints operations under (a GUI sets
    /// these to the local editing identity). Defaults to `ReplicaId(1)` / author 0.
    ///
    /// **Pre-edit only** — panics if called after an [`Self::apply`]. A session's op
    /// log is one replica's contiguous, zero-based history (each counter is
    /// `applied.len()`); switching identity mid-stream would continue the counter
    /// under a new replica, leaving a `(new_replica, 0)` hole that the
    /// missing-predecessor rule would hold pending. The identity is therefore fixed
    /// before the first edit.
    pub fn with_identity(mut self, replica: ReplicaId, author: AuthorId) -> Self {
        assert!(
            self.applied.is_empty(),
            "with_identity must be set before any edit: a session's op log is a \
             single replica's contiguous history"
        );
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

    /// The operations this session has applied, oldest first — an append-only log
    /// for undo, history, and sync (a client streams these to peers; an undo layer
    /// rebuilds or inverts from them). Only successfully-applied edits appear; a
    /// rejected or non-renderable edit leaves the log untouched.
    pub fn applied_operations(&self) -> &[OperationEnvelope] {
        &self.applied
    }

    /// The most recently applied operation, if any (the tail of
    /// [`Self::applied_operations`]).
    pub fn last_applied(&self) -> Option<&OperationEnvelope> {
        self.applied.last()
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
    /// context) a GUI would otherwise assemble by hand. Pure: it reads no mutable
    /// state, so a failed [`Self::apply`] consumes no id.
    ///
    /// The causal context makes the session's edits a single replica's contiguous,
    /// zero-based history: the op at `counter` covers every prior local op (the
    /// range `[0, counter - 1]`). Two sequential edits to the same target therefore
    /// read as intentional overwrites — the later covering the earlier — rather than
    /// concurrent conflicting edits, both when this session re-reduces its own log
    /// and when a peer replays it. The first op (counter 0) is a root: an empty
    /// context.
    fn envelope_for(&self, counter: u64, kind: OperationKind) -> OperationEnvelope {
        let id = OperationId::new(self.replica, counter);
        let causal_context = if counter > 0 {
            CausalContext::new().with_seen(self.replica, counter - 1)
        } else {
            CausalContext::new()
        };
        OperationEnvelope {
            id,
            author: self.author,
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(counter as i64), 0),
                id,
            ),
            causal_context,
            transaction: None,
            payload: OperationPayload::Primitive(kind),
        }
    }

    /// Applies a primitive operation: mints an envelope, reduces the whole local
    /// op log (prior edits plus this one) onto the pristine open-time score,
    /// re-renders, and re-resolves the selection. **Atomic**: on any error — a
    /// rejected (not well-formed) operation, or a diagnostic-only layout — the
    /// session is left entirely unchanged, op log included.
    ///
    /// Reducing the *accumulated* set onto `base` (rather than the new op alone
    /// onto the running materialization) is what lets each envelope carry a causal
    /// context that covers its predecessors: those predecessors are present in the
    /// set, so the missing-predecessor rule does not hold the new op pending, and
    /// the session's render is exactly the canonical reduction of the op log it
    /// emits. (Re-reducing the whole log each edit is fine at editor scale;
    /// incremental reduction is a later optimization.)
    pub fn apply(&mut self, kind: OperationKind) -> Result<EditOutcome, EditorError> {
        let counter = self.applied.len() as u64;
        let envelope = self.envelope_for(counter, kind);

        // Re-accept the prior log (idempotent — they were accepted before), then
        // the new op. A minted operation the reducer will not accept (e.g. a
        // reserved replica identity) must not silently no-op the edit.
        let mut set = OperationSet::new();
        for prior in &self.applied {
            set.accept(prior.clone());
        }
        if !matches!(set.accept(envelope.clone()), AcceptOutcome::Accepted) {
            return Err(EditorError::RejectedOperation);
        }
        let edited = set.reduce_onto(&self.base).score;
        let graph_changed = edited != self.score;

        // Refuse a diagnostic-only layout, still before committing anything.
        let (render, map) =
            render_score(&edited, self.solver.as_ref()).ok_or(EditorError::NotRenderable)?;

        // Commit (the only mutation point — so an error above leaves all state,
        // op log included, untouched).
        self.score = edited;
        self.render = render;
        self.map = map;
        self.applied.push(envelope);

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

    /// Moves the selected pitch by `steps` diatonic **staff steps** (`+1` is up one
    /// staff position, `-1` down): a nominal move that wraps the octave at the B↔C
    /// boundary and preserves the accidental — the "diatonic move" a vertical drag
    /// performs. It modifies the pitch in place ([`OperationKind::ModifyIdentifiedPitch`]),
    /// so the note keeps its identity and the selection survives the relayout.
    ///
    /// Errors if nothing — or a non-pitch — is selected, if the selected pitch has no
    /// CMN staff position to step (an N-tone or grammar-defined position), or if the
    /// pitch carries an authored spelling override that would pin its rendered position
    /// ([`EditorError::PitchSpellingOverridden`]).
    pub fn move_selection_staff_step(&mut self, steps: i32) -> Result<EditOutcome, EditorError> {
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let TypedObjectId::Pitch(pitch) = selection.source else {
            return Err(EditorError::WrongSelection { expected: "pitch" });
        };
        // An authored spelling override resolves ahead of the inferred spelling, so
        // it pins the rendered staff position: modifying the pitch value alone would
        // change the sound but leave the notehead where it was. Refuse rather than
        // mislead — moving the override too needs an atomic respell (a follow-up).
        if has_authored_spelling_override(&self.score, pitch) {
            return Err(EditorError::PitchSpellingOverridden);
        }
        // The move is relative to the pitch's current value, so read it from the
        // graph and step its staff position.
        let moved = self
            .current_pitch(pitch)
            .as_ref()
            .and_then(|current| staff_step(current, steps))
            .ok_or(EditorError::WrongSelection {
                expected: "CMN pitch",
            })?;
        self.apply(OperationKind::ModifyIdentifiedPitch(
            ModifyIdentifiedPitchOp {
                pitch,
                value: moved,
            },
        ))
    }

    /// The current value of the identified `pitch` in the score graph, if it is
    /// present (a live embedded pitch).
    fn current_pitch(&self, pitch: PitchId) -> Option<Pitch> {
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        for event in self.score.events.iter() {
            buf.clear();
            event.collect_identified_pitches(&mut buf);
            if let Some(ip) = buf.iter().find(|ip| ip.id == pitch) {
                return Some(ip.pitch.clone());
            }
        }
        None
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

/// The pitch one or more diatonic **staff steps** from `pitch`: the CMN nominal is
/// moved by `steps`, carrying the octave at the B↔C boundary, with the alteration and
/// acoustic realization preserved (a pure staff-position move). Returns `None` for a
/// non-CMN position — which has no staff-step notion — or an octave that overflows
/// `i8`.
fn staff_step(pitch: &Pitch, steps: i32) -> Option<Pitch> {
    let PitchSpacePosition::Cmn {
        nominal,
        alteration,
        octave,
    } = pitch.scale_position.position
    else {
        return None;
    };
    // Seven nominals to an octave: index diatonically, move, then decompose so a
    // move past B (or below C) carries the octave. Computed in i64 so an extreme
    // `steps` cannot overflow the intermediate before the octave range-check below.
    let diatonic = octave as i64 * 7 + nominal as i64 + steps as i64;
    let new_octave = i8::try_from(diatonic.div_euclid(7)).ok()?;
    let new_nominal = nominal_from_index(diatonic.rem_euclid(7));
    let mut moved = pitch.clone();
    moved.scale_position.position = PitchSpacePosition::Cmn {
        nominal: new_nominal,
        alteration,
        octave: new_octave,
    };
    Some(moved)
}

/// The CMN nominal at diatonic index `i` (`0..=6` → `C..=B`); callers pass
/// `rem_euclid(7)`, so other values are unreachable and fold to `B`.
fn nominal_from_index(i: i64) -> CmnNominal {
    match i {
        0 => CmnNominal::C,
        1 => CmnNominal::D,
        2 => CmnNominal::E,
        3 => CmnNominal::F,
        4 => CmnNominal::G,
        5 => CmnNominal::A,
        _ => CmnNominal::B,
    }
}

/// Whether the score carries an authored explicit spelling override for `pitch`
/// that the resolver would rank ahead of the inferred spelling — pinning the
/// rendered staff position regardless of the pitch value. Mirrors `epiphany_core`'s
/// `resolve_spelling`: engraved layer, pitch-scoped, explicit, and a source that
/// outranks `Inferred` under the score's precedence (the default ranks `UserChosen`,
/// `Imported`, and `Propagated` all ahead of `Inferred`, so any of them pins).
fn has_authored_spelling_override(score: &Score, pitch: PitchId) -> bool {
    let inferred_rank = score.spelling_precedence.rank(SpellingSourceKind::Inferred);
    score.spelling_attachments.iter().any(|att| {
        att.layer.is_none()
            && matches!(&att.scope, SpellingScope::Pitch(p) if *p == pitch)
            && matches!(att.directive, SpellingDirective::Explicit(_))
            && score.spelling_precedence.rank(att.source.kind()) < inferred_rank
    })
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
    fn move_selection_staff_step_moves_the_note_and_keeps_the_selection() {
        let mut session = open_rich(0x5EED);
        let before_render = session.render().clone();
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pid) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let before = session
            .current_pitch(pid)
            .expect("the selected pitch is live");

        let outcome = session
            .move_selection_staff_step(1)
            .expect("the move applies");
        assert!(outcome.graph_changed, "the move changed the pitch");
        assert!(outcome.selection_preserved, "the note keeps its identity");
        assert_eq!(
            session.selection().map(|s| s.layout_object),
            Some(selection.layout_object),
            "the selection still anchors the same notehead"
        );
        let after = session.current_pitch(pid).expect("the pitch is still live");
        assert_eq!(
            after,
            staff_step(&before, 1).unwrap(),
            "the pitch moved exactly one staff step up"
        );
        assert_ne!(
            &before_render,
            session.render(),
            "the note moved on the staff"
        );
    }

    #[test]
    fn move_selection_staff_step_requires_a_pitch_selection() {
        let mut session = open_rich(0x5EED);
        assert_eq!(
            session.move_selection_staff_step(1),
            Err(EditorError::NoSelection)
        );
    }

    #[test]
    fn staff_step_carries_the_octave_and_keeps_the_accidental() {
        // A real fixture pitch is a convenient template (it spares hand-building the
        // acoustic identity); only its staff position is what the step touches.
        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pid) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let mut p = session
            .current_pitch(pid)
            .expect("the selected pitch is live");

        // C#4 up one staff step → D#4: the letter advances, the sharp is preserved.
        p.scale_position.position = PitchSpacePosition::Cmn {
            nominal: CmnNominal::C,
            alteration: 1,
            octave: 4,
        };
        assert!(matches!(
            staff_step(&p, 1).unwrap().scale_position.position,
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::D,
                alteration: 1,
                octave: 4
            }
        ));
        // B4 up → C5: the octave carries up at the boundary.
        p.scale_position.position = PitchSpacePosition::Cmn {
            nominal: CmnNominal::B,
            alteration: 0,
            octave: 4,
        };
        assert!(matches!(
            staff_step(&p, 1).unwrap().scale_position.position,
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                octave: 5,
                ..
            }
        ));
        // C4 down → B3: and down.
        p.scale_position.position = PitchSpacePosition::Cmn {
            nominal: CmnNominal::C,
            alteration: 0,
            octave: 4,
        };
        assert!(matches!(
            staff_step(&p, -1).unwrap().scale_position.position,
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::B,
                octave: 3,
                ..
            }
        ));
    }

    #[test]
    fn move_refuses_a_pitch_with_an_authored_spelling_override() {
        use epiphany_core::PitchSpelling;
        use epiphany_ops::RespellPitchOp;

        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pid) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        // Pin the pitch with an explicit user spelling (what an authored respell leaves).
        session
            .apply(OperationKind::RespellPitch(RespellPitchOp {
                pitch: pid,
                spelling: PitchSpelling::cmn(CmnNominal::C, 4),
            }))
            .expect("the respell applies");

        // The override pins the rendered staff position, so a value-only move is
        // refused rather than silently changing the sound without moving the notehead.
        assert_eq!(
            session.move_selection_staff_step(1),
            Err(EditorError::PitchSpellingOverridden)
        );
    }

    #[test]
    fn an_imported_or_propagated_override_pins_a_move_too() {
        // The resolver ranks any explicit override that outranks Inferred — not just
        // user-chosen ones — so the refusal predicate must mirror the precedence, not
        // a single source. A propagated override (default rank above Inferred) pins.
        use epiphany_core::{
            PitchSpelling, SpellingAttachment, SpellingDirective, SpellingScope, SpellingSource,
        };

        let mut score = valid_score_rich(0x5EED);
        let pid = *score.live_pitch_ids().iter().next().expect("a live pitch");
        // Baseline: an unspelled live pitch is not pinned.
        assert!(!has_authored_spelling_override(&score, pid));

        score.spelling_attachments.push(SpellingAttachment {
            scope: SpellingScope::Pitch(pid),
            directive: SpellingDirective::Explicit(PitchSpelling::cmn(CmnNominal::C, 4)),
            source: SpellingSource::Propagated { from: pid },
            priority: 0,
            layer: None,
        });
        assert!(
            has_authored_spelling_override(&score, pid),
            "a propagated explicit override outranks Inferred and pins the notehead"
        );
    }

    #[test]
    fn sequential_staff_step_moves_replay_without_structural_conflict() {
        // Two moves on one notehead are sequential local edits to the same target,
        // each a distinct-value ModifyIdentifiedPitch (the note climbs, e.g. C→D→E).
        // The second envelope covers the first, so replaying the emitted op log
        // reduces them as intentional overwrites — no StructuralFieldCollision and
        // nothing held pending. This is the modify path the per-edit causal context
        // exists for (a transpose would not exercise the concurrent() check).
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        session.move_selection_staff_step(1).expect("first move");
        session.move_selection_staff_step(1).expect("second move");

        let log = session.applied_operations();
        assert_eq!(log.len(), 2);

        let base = valid_score_rich(0x5EED);
        let mut set = OperationSet::new();
        for env in log {
            set.accept(env.clone());
        }
        let materialized = set.reduce_onto(&base);
        assert!(
            materialized.state.is_clean(),
            "sequential same-target moves must replay without a conflict or pending op"
        );
    }

    #[test]
    fn applied_operations_log_records_each_successful_edit_in_order() {
        let mut session = open_rich(0x5EED);
        assert!(session.applied_operations().is_empty());
        assert!(session.last_applied().is_none());

        click_a_notehead(&mut session);
        session.transpose_selection(1).unwrap();
        session.transpose_selection(-1).unwrap();

        // Two edits, two log entries, distinct ids, in application order, and
        // last_applied is the tail.
        let log = session.applied_operations();
        assert_eq!(log.len(), 2);
        assert_ne!(log[0].id, log[1].id);
        assert_eq!(session.last_applied(), Some(&log[1]));
        // The log carries real envelopes a peer/undo layer can consume.
        assert!(matches!(
            log[1].payload,
            OperationPayload::Primitive(OperationKind::Transpose(_))
        ));
    }

    #[test]
    fn each_local_edit_causally_covers_the_prior_one() {
        // The session's edits form one replica's contiguous, zero-based history, so
        // a later edit's envelope causally covers the earlier one. That is what makes
        // two sequential same-target edits (e.g. a diatonic move, which modifies a
        // pitch in place) reduce as intentional overwrites rather than concurrent
        // conflicts — both here and on a peer replaying the log.
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        session.transpose_selection(1).unwrap();
        session.transpose_selection(1).unwrap();

        let log = session.applied_operations();
        assert_eq!(log.len(), 2);
        // The first edit is a root: no causal predecessors.
        assert!(
            log[0].causal_context.is_empty(),
            "the first edit has no predecessors"
        );
        // The second covers the first (and shares neither id).
        assert_ne!(log[0].id, log[1].id);
        assert!(
            log[1].causal_context.covers(log[0].id),
            "the second edit causally covers the first"
        );
        // And it still materialized — covering its predecessor must not hold it
        // pending under the missing-predecessor rule (the predecessor is in the set
        // the session reduces).
        assert!(
            session.last_applied().is_some(),
            "the covering edit applied"
        );
    }

    #[test]
    #[should_panic(expected = "before any edit")]
    fn changing_identity_after_an_edit_is_refused() {
        // The op log is one replica's contiguous history; switching identity after
        // an edit would continue the counter under a new replica and open a
        // `(new_replica, 0)` hole. The session refuses it.
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        session.transpose_selection(1).unwrap();
        let _ = session.with_identity(ReplicaId(2), AuthorId(0));
    }

    #[test]
    fn a_rejected_edit_is_not_logged() {
        use epiphany_core::ReplicaId;
        let mut session = open_rich(0x5EED).with_identity(ReplicaId::SYSTEM_DERIVED, AuthorId(0));
        click_a_notehead(&mut session);
        assert_eq!(
            session.transpose_selection(1),
            Err(EditorError::RejectedOperation)
        );
        assert!(
            session.applied_operations().is_empty(),
            "a rejected edit leaves the op log untouched"
        );
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
