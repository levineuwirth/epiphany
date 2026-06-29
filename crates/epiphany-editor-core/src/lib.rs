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
//! * **atomic transactions** ([`EditorSession::apply_transaction`]) — several
//!   primitives committed together under one `DeclareTransaction`, applied
//!   all-or-nothing by the reducer and reconstructed as one unit by a peer; the
//!   substrate for intents that must land a value change and its matching respelling
//!   together (and a cleaner unit of work for undo);
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

use std::cmp::Reverse;
use std::fmt;

use epiphany_core::{
    AcousticRealization, CmnNominal, Event, EventDuration, EventId, EventPosition, IdentifiedPitch,
    MusicalDuration, MusicalPosition, OperationId, Pitch, PitchId, PitchSpacePosition,
    PitchSpelling, PitchedEvent, RegionTimeModel, ReplicaId, Score, SpellingDirective,
    SpellingNominal, SpellingScope, SpellingSourceKind, StaffInstanceId, StemConfiguration,
    TransactionId, TypedObjectId, VoiceId, WallClockTime,
};
use epiphany_layout_ir::{
    to_constrained, to_logical, to_render, ConstraintSolver, HitTestMap, LayoutObjectId, Point,
    RenderIR, ResolvedLayoutIR, SolverConfig,
};
use epiphany_ops::{
    AcceptOutcome, AuthorId, CausalContext, DeleteEventOp, DeleteIdentifiedPitchOp,
    HybridLogicalClock, InsertEventOp, InsertIdentifiedPitchOp, ModifyIdentifiedPitchOp,
    OperationEnvelope, OperationKind, OperationPayload, OperationSet, OperationStamp,
    RespellPitchOp, TransactionCategory, TransactionDescriptor, TransposeOp, TupletCompensation,
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
    /// The edit was not applied and nothing changed: either a minted operation was
    /// not well-formed (e.g. minted under the reserved
    /// [`epiphany_core::ReplicaId::SYSTEM_DERIVED`] identity), or the reduction was
    /// not clean — an atomic transaction whose member preconditions failed rolls back
    /// as a conflict, and the editor refuses to log a transaction that did not take
    /// effect. The edit is dropped rather than silently no-op'd.
    RejectedOperation,
    /// [`EditorSession::apply_transaction`] was called with no member operations,
    /// which would log only a no-op transaction descriptor.
    EmptyTransaction,
    /// An [`OperationKind::DeclareTransaction`] was submitted as an edit — passed to
    /// [`EditorSession::apply`], or as a member of [`EditorSession::apply_transaction`].
    /// The session declares transactions itself; only primitive mutations may be
    /// submitted, so a directly-applied or nested declaration (which would log a dead
    /// descriptor-only unit) is refused.
    DeclareTransactionNotAllowed,
    /// An intent needed a selection but none is set.
    NoSelection,
    /// The selection is not the kind the intent requires (e.g. a transpose needs a
    /// pitch selection).
    WrongSelection {
        /// The kind of object the intent expected.
        expected: &'static str,
    },
    /// A pitch involved in the edit carries an authored spelling override (user-chosen,
    /// imported, or propagated) that outranks the inferred spelling, and the intent
    /// cannot carry it. Raised by a chord add whose target event has an override — the
    /// rendered staff order can't be read off the raw pitch positions, so the "above
    /// the top" pick could be wrong; resolved-spelling-aware stacking is a follow-up.
    /// The staff-step move and the insert do *not* raise this — they rebase / carry the
    /// override atomically instead.
    PitchSpellingOverridden,
    /// An insert-after would land on a musical position already occupied by another
    /// event in the same voice (the reducer would silently no-op it). The edit is
    /// refused; inserting into a packed voice needs an explicit make-room policy.
    InsertSlotOccupied,
}

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorError::NotRenderable => {
                f.write_str("the resulting layout is diagnostic-only and cannot be rendered")
            }
            EditorError::RejectedOperation => f.write_str(
                "the edit was rejected and nothing changed (an ill-formed operation, or a \
                 transaction that rolled back)",
            ),
            EditorError::NoSelection => f.write_str("no selection"),
            EditorError::WrongSelection { expected } => {
                write!(f, "the selection is not a {expected}")
            }
            EditorError::PitchSpellingOverridden => f.write_str(
                "the selected pitch has an authored spelling override that pins its staff position",
            ),
            EditorError::InsertSlotOccupied => {
                f.write_str("the position after the selection is already occupied in its voice")
            }
            EditorError::EmptyTransaction => {
                f.write_str("a transaction must have at least one member operation")
            }
            EditorError::DeclareTransactionNotAllowed => {
                f.write_str("a transaction cannot be declared through an edit; the session manages transactions")
            }
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
    // The solved layout the current render derives from — the input a renderer (e.g.
    // epiphany-render-svg) consumes. Kept alongside the RenderIR, which is the
    // hit-test projection of it.
    resolved: ResolvedLayoutIR,
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
        let (resolved, render, map) =
            render_score(&score, solver.as_ref()).ok_or(EditorError::NotRenderable)?;
        Ok(EditorSession {
            base: score.clone(),
            score,
            solver,
            resolved,
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

    /// The current resolved layout — the input a renderer (e.g. `epiphany-render-svg`)
    /// consumes to draw the score. The [`Self::render`] / [`Self::hit_test`] views are
    /// its hit-test projection.
    pub fn resolved(&self) -> &ResolvedLayoutIR {
        &self.resolved
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

    /// Builds an [`OperationEnvelope`] at operation `counter` under the session's
    /// identity, carrying `payload` and an optional `transaction` membership — the
    /// bookkeeping (id, author, stamp, causal context) a GUI would otherwise assemble
    /// by hand. Pure: it reads no mutable state, so a failed apply consumes no id.
    ///
    /// The causal context makes the session's edits a single replica's contiguous,
    /// zero-based history: the op at `counter` covers every prior local op (the
    /// range `[0, counter - 1]`). Two sequential edits to the same target therefore
    /// read as intentional overwrites — the later covering the earlier — rather than
    /// concurrent conflicting edits, both when this session re-reduces its own log
    /// and when a peer replays it. The first op (counter 0) is a root: an empty
    /// context. This also gives transaction members descriptor-precedence for free:
    /// a member at a later counter covers the descriptor minted before it.
    fn envelope_at(
        &self,
        counter: u64,
        payload: OperationPayload,
        transaction: Option<TransactionId>,
    ) -> OperationEnvelope {
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
            transaction,
            payload,
        }
    }

    /// Builds a standalone primitive envelope for `kind` at `counter`.
    fn envelope_for(&self, counter: u64, kind: OperationKind) -> OperationEnvelope {
        self.envelope_at(counter, OperationPayload::Primitive(kind), None)
    }

    /// Accepts `new` envelopes on top of the prior log, reduces the whole set onto
    /// the pristine open-time `base`, re-renders, and re-resolves the selection,
    /// committing only if every step succeeds. **Atomic**: on a rejected (not
    /// well-formed) envelope or a diagnostic-only layout, nothing mutates — the op
    /// log included. The shared engine of [`Self::apply`] and
    /// [`Self::apply_transaction`].
    ///
    /// Reducing the *accumulated* set onto `base` (rather than the new ops alone onto
    /// the running materialization) is what lets each envelope carry a causal context
    /// covering its predecessors: those predecessors are present in the set, so the
    /// missing-predecessor rule does not hold a new op pending, and the session's
    /// render is exactly the canonical reduction of the op log it emits. (Re-reducing
    /// the whole log each edit is fine at editor scale; incremental reduction is a
    /// later optimization.)
    fn commit(&mut self, new: Vec<OperationEnvelope>) -> Result<EditOutcome, EditorError> {
        // Re-accept the prior log (idempotent — they were accepted before), then the
        // new envelopes. One the reducer will not accept (e.g. a reserved replica
        // identity) must not partially apply.
        let mut set = OperationSet::new();
        for prior in &self.applied {
            set.accept(prior.clone());
        }
        for env in &new {
            if !matches!(set.accept(env.clone()), AcceptOutcome::Accepted) {
                return Err(EditorError::RejectedOperation);
            }
        }
        let materialized = set.reduce_onto(&self.base);
        // A non-clean reduction means the edit did not apply cleanly: an atomic
        // transaction whose member preconditions fail rolls back as a conflict, yet
        // the reducer still returns a score. Refuse rather than log an edit that did
        // not take effect. (Committed edits are kept clean, so any conflict/pending is
        // introduced by `new`.)
        if !materialized.state.is_clean() {
            return Err(EditorError::RejectedOperation);
        }
        let edited = materialized.score;
        let graph_changed = edited != self.score;

        // Refuse a diagnostic-only layout, still before committing anything.
        let (resolved, render, map) =
            render_score(&edited, self.solver.as_ref()).ok_or(EditorError::NotRenderable)?;

        // Commit (the only mutation point — so an error above leaves all state,
        // op log included, untouched).
        self.score = edited;
        self.resolved = resolved;
        self.render = render;
        self.map = map;
        self.applied.extend(new);

        let selection_preserved = self.reresolve_selection();
        Ok(EditOutcome {
            graph_changed,
            selection_preserved,
        })
    }

    /// Applies a single primitive operation: mints an envelope and commits it. A
    /// [`OperationKind::DeclareTransaction`] is not a primitive mutation — the session
    /// declares transactions via [`Self::apply_transaction`] — so it is refused here.
    pub fn apply(&mut self, kind: OperationKind) -> Result<EditOutcome, EditorError> {
        if matches!(kind, OperationKind::DeclareTransaction(_)) {
            return Err(EditorError::DeclareTransactionNotAllowed);
        }
        let counter = self.applied.len() as u64;
        let envelope = self.envelope_for(counter, kind);
        self.commit(vec![envelope])
    }

    /// Applies a sequence of primitive operations as one **atomic transaction**: a
    /// `DeclareTransaction` descriptor (carrying `label` and `category`, for undo
    /// history) plus one member envelope per `kind`, all committed together. The
    /// reducer applies the members all-or-nothing — if any member's precondition
    /// fails, the whole block rolls back as a transaction conflict — and a peer
    /// replaying the log reconstructs the same atomic unit. This is the substrate for
    /// intents that must land several primitives together (e.g. a value change with a
    /// matching respelling); editor-level atomicity (commit only on full success) is
    /// inherited from [`Self::commit`].
    pub fn apply_transaction(
        &mut self,
        label: &str,
        category: Option<TransactionCategory>,
        kinds: Vec<OperationKind>,
    ) -> Result<EditOutcome, EditorError> {
        // A member-less transaction would log a descriptor-only no-op (a dead
        // undo/sync unit). Refuse before minting anything.
        if kinds.is_empty() {
            return Err(EditorError::EmptyTransaction);
        }
        // The session declares the transaction; a member that is itself a
        // DeclareTransaction would log a nested no-op declaration. Refuse it.
        if kinds
            .iter()
            .any(|k| matches!(k, OperationKind::DeclareTransaction(_)))
        {
            return Err(EditorError::DeclareTransactionNotAllowed);
        }
        let envelopes = self.transaction_envelopes(label, category, kinds);
        self.commit(envelopes)
    }

    /// Mints the envelopes for an atomic transaction: a `DeclareTransaction`
    /// descriptor at the next counter, then one member envelope per kind at the
    /// following counters, each referencing the transaction id. The contiguous causal
    /// context gives every member descriptor-precedence over the descriptor for free.
    /// Pure.
    fn transaction_envelopes(
        &self,
        label: &str,
        category: Option<TransactionCategory>,
        kinds: Vec<OperationKind>,
    ) -> Vec<OperationEnvelope> {
        let base = self.applied.len() as u64;
        let tx_id = self.mint_transaction_id();
        let descriptor = TransactionDescriptor {
            id: tx_id,
            label: label.to_string(),
            category,
        };
        let mut envelopes = Vec::with_capacity(kinds.len() + 1);
        envelopes.push(self.envelope_at(
            base,
            OperationPayload::Primitive(OperationKind::DeclareTransaction(descriptor)),
            None,
        ));
        for (i, kind) in kinds.into_iter().enumerate() {
            let counter = base + 1 + i as u64;
            envelopes.push(self.envelope_at(
                counter,
                OperationPayload::Primitive(kind),
                Some(tx_id),
            ));
        }
        envelopes
    }

    /// Mints a fresh [`TransactionId`] in the session's replica namespace, one past
    /// the highest transaction counter declared in this session's op log. (Transaction
    /// ids live only in the op stream — the materialized score retains no trace — so
    /// the log is the sole source.)
    fn mint_transaction_id(&self) -> TransactionId {
        let next = self
            .applied
            .iter()
            .filter_map(declared_transaction_id)
            .filter(|t| t.replica() == self.replica)
            .map(|t| t.counter())
            .max()
            .map_or(0, |c| {
                c.checked_add(1)
                    .expect("transaction id counter overflowed u64")
            });
        TransactionId::new(self.replica, next)
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
    /// If the pitch carries an **authored spelling override** (which pins the rendered
    /// staff position), the move also rebases that spelling by the same step, applied
    /// **atomically** as one transaction — so the notehead and the sound move together
    /// rather than the move being refused.
    ///
    /// Errors if nothing — or a non-pitch — is selected, or the selected pitch (or its
    /// override) has no CMN staff position to step (an N-tone or grammar-defined
    /// position).
    pub fn move_selection_staff_step(&mut self, steps: i32) -> Result<EditOutcome, EditorError> {
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let TypedObjectId::Pitch(pitch) = selection.source else {
            return Err(EditorError::WrongSelection { expected: "pitch" });
        };
        // The move is relative to the pitch's current value, so read it from the
        // graph and step its staff position.
        let current = self
            .current_pitch(pitch)
            .ok_or(EditorError::WrongSelection { expected: "pitch" })?;
        let moved = staff_step(&current, steps).ok_or(EditorError::WrongSelection {
            expected: "CMN pitch",
        })?;
        let modify = OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
            pitch,
            value: moved,
        });

        // With no override, the inferred spelling follows the new value — a plain
        // modify suffices. With one, step it too and land both atomically, so the
        // pinned notehead moves with the sound.
        match authored_spelling(&self.score, pitch) {
            None => self.apply(modify),
            Some(spelling) => {
                let moved_spelling =
                    staff_step_spelling(&spelling, steps).ok_or(EditorError::WrongSelection {
                        expected: "CMN pitch",
                    })?;
                self.apply_transaction(
                    "move note",
                    Some(TransactionCategory::NoteEntry),
                    vec![
                        modify,
                        OperationKind::RespellPitch(RespellPitchOp {
                            pitch,
                            spelling: moved_spelling,
                        }),
                    ],
                )
            }
        }
    }

    /// Adds a note to the selected pitch's event, forming (or extending) a chord: a
    /// new identified pitch one diatonic staff step above the event's highest note,
    /// with a fresh id ([`OperationKind::InsertIdentifiedPitch`]). The selection is
    /// unchanged — the anchored notehead is still there. Errors if nothing — or a
    /// non-pitch — is selected, the event has no CMN note to step above, or any note
    /// in the event carries an authored spelling override (which would make the
    /// rendered staff order differ from the raw pitch order — see
    /// [`EditorError::PitchSpellingOverridden`]).
    ///
    /// The default is deliberately minimal (a real client picks the inserted pitch);
    /// stepping above the top note means repeated calls build a rising chord rather
    /// than stacking duplicates on one staff position.
    pub fn add_note_to_selection(&mut self) -> Result<EditOutcome, EditorError> {
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let TypedObjectId::Pitch(anchor) = selection.source else {
            return Err(EditorError::WrongSelection { expected: "pitch" });
        };
        let (event, _) = self
            .event_and_pitch_of(anchor)
            .ok_or(EditorError::WrongSelection { expected: "pitch" })?;
        // "Above the top" ranks raw pitch positions; an authored override makes the
        // rendered staff order diverge from that, so the pick could be wrong. Refuse
        // (as the move intent does) rather than guess — resolved-spelling-aware
        // stacking is a follow-up.
        if self.event_has_authored_override(event) {
            return Err(EditorError::PitchSpellingOverridden);
        }
        let top = self
            .highest_pitch_in_event(event)
            .ok_or(EditorError::WrongSelection {
                expected: "CMN pitch",
            })?;
        let value = note_above(&top).ok_or(EditorError::WrongSelection {
            expected: "CMN pitch",
        })?;
        let pitch = IdentifiedPitch {
            id: self.mint_pitch_id(),
            pitch: value,
        };
        self.apply(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp { event, pitch },
        ))
    }

    /// The highest CMN note (by diatonic staff index) embedded in `event`, if any —
    /// the note a chord-add stacks above.
    fn highest_pitch_in_event(&self, event: EventId) -> Option<Pitch> {
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        self.score
            .events
            .get(event)?
            .collect_identified_pitches(&mut buf);
        buf.iter()
            .filter_map(|ip| diatonic_index(&ip.pitch).map(|d| (d, &ip.pitch)))
            .max_by_key(|(d, _)| *d)
            .map(|(_, p)| p.clone())
    }

    /// Whether any note in `event` carries an authored spelling override (so the
    /// rendered staff order cannot be read off the raw pitch positions).
    fn event_has_authored_override(&self, event: EventId) -> bool {
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        let Some(ev) = self.score.events.get(event) else {
            return false;
        };
        ev.collect_identified_pitches(&mut buf);
        buf.iter()
            .any(|ip| has_authored_spelling_override(&self.score, ip.id))
    }

    /// Inserts a new note in the selected pitch's voice immediately after its event:
    /// a fresh single-note event ([`OperationKind::InsertEvent`]) at the next musical
    /// position, copying the selected pitch and its rhythmic value. The selection is
    /// unchanged — the anchored notehead is still there.
    ///
    /// If the selected pitch carries an **authored spelling override**, the copy carries
    /// it too: the insert lands **atomically** with a `RespellPitch` that gives the new
    /// note the same spelling, so the copy renders like the original.
    ///
    /// Errors if nothing — or a non-pitch — is selected, the anchor event is not a
    /// metric (musical) event in a metric region, or the position right after it is
    /// already occupied in the voice ([`EditorError::InsertSlotOccupied`]) — inserting
    /// into a packed voice needs an explicit make-room policy, a follow-up.
    pub fn insert_note_after_selection(&mut self) -> Result<EditOutcome, EditorError> {
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let TypedObjectId::Pitch(anchor) = selection.source else {
            return Err(EditorError::WrongSelection { expected: "pitch" });
        };
        let (event_id, anchor_value) = self
            .event_and_pitch_of(anchor)
            .ok_or(EditorError::WrongSelection { expected: "pitch" })?;

        // The anchor's voice, position, and duration — the position and duration must
        // be metric, with a positive duration (InsertEvent rejects a zero/negative
        // span and a non-metric region).
        let (voice, position, duration) = {
            let ev = self
                .score
                .events
                .get(event_id)
                .ok_or(EditorError::WrongSelection { expected: "pitch" })?;
            let EventPosition::Musical(position) = ev.position().clone() else {
                return Err(EditorError::WrongSelection {
                    expected: "metric event",
                });
            };
            let EventDuration::Musical(duration) = ev.duration().clone() else {
                return Err(EditorError::WrongSelection {
                    expected: "metric event",
                });
            };
            (ev.voice(), position, duration)
        };
        if !duration.is_positive() {
            return Err(EditorError::WrongSelection {
                expected: "metric event",
            });
        }
        let next_position = position + duration.clone();

        // Resolve the voice's region + staff instance together and require a metric
        // region — the reducer's InsertEvent precondition rejects any other time model.
        let staff_instance =
            self.metric_staff_instance_of_voice(voice)
                .ok_or(EditorError::WrongSelection {
                    expected: "metric event",
                })?;
        // Pre-check the slot so the edit refuses cleanly instead of being silently
        // no-op'd (and logged) by the reducer's voice-overlap rule.
        if self.voice_slot_occupied(voice, &next_position, &duration) {
            return Err(EditorError::InsertSlotOccupied);
        }

        let new_pitch = self.mint_pitch_id();
        let event = Event::Pitched(PitchedEvent {
            id: self.mint_event_id(),
            voice,
            position: EventPosition::Musical(next_position),
            duration: EventDuration::Musical(duration),
            pitches: vec![IdentifiedPitch {
                id: new_pitch,
                pitch: anchor_value,
            }],
            articulations: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: StemConfiguration,
            grace: None,
        });
        let insert = OperationKind::InsertEvent(InsertEventOp {
            staff_instance,
            event,
        });

        // The new note copies the selected pitch's value; if that pitch has an authored
        // spelling, copy it onto the new note too — atomically, so the copy renders the
        // same. The respell targets the pitch the insert mints, so the insert must run
        // first; the transaction's canonical order (by counter) guarantees that.
        match authored_spelling(&self.score, anchor) {
            None => self.apply(insert),
            Some(spelling) => self.apply_transaction(
                "insert note",
                Some(TransactionCategory::NoteEntry),
                vec![
                    insert,
                    OperationKind::RespellPitch(RespellPitchOp {
                        pitch: new_pitch,
                        spelling,
                    }),
                ],
            ),
        }
    }

    /// The staff instance hosting `voice`, but only when its region is metric — the
    /// time model `InsertEvent` requires (the reducer rejects any other). `None` if
    /// the voice is absent from the voice tree or its region is non-metric.
    fn metric_staff_instance_of_voice(&self, voice: VoiceId) -> Option<StaffInstanceId> {
        let (region_id, staff_instance, _) = self.score.voices().find(|(_, _, v)| v.id == voice)?;
        let region = self
            .score
            .canvas
            .regions
            .iter()
            .find(|r| r.id == region_id)?;
        matches!(region.time_model, RegionTimeModel::Metric(_)).then_some(staff_instance)
    }

    /// Whether any metric event already in `voice` overlaps `[position, position +
    /// duration)` — the same voice-overlap test the reducer's insert applies, so a
    /// clean pre-check matches its accept/no-op decision exactly.
    fn voice_slot_occupied(
        &self,
        voice: VoiceId,
        position: &MusicalPosition,
        duration: &MusicalDuration,
    ) -> bool {
        self.score.events.iter().any(|ev| {
            ev.voice() == voice
                && matches!(
                    (ev.position(), ev.duration()),
                    (EventPosition::Musical(p), EventDuration::Musical(d))
                        if musical_overlap(position, duration, p, d)
                )
        })
    }

    /// Mints a fresh [`EventId`] in the session's replica namespace, on the same
    /// three-source high-water-mark basis as [`Self::mint_pitch_id`]: the pristine
    /// `base`, the current score (each live or tombstoned), and this session's op log.
    fn mint_event_id(&self) -> EventId {
        let ids = self
            .base
            .events
            .iter()
            .map(Event::id)
            .chain(self.base.tombstoned_events.iter().copied())
            .chain(self.score.events.iter().map(Event::id))
            .chain(self.score.tombstoned_events.iter().copied())
            .chain(self.applied.iter().flat_map(inserted_event_ids));
        let next = ids
            .filter(|e| e.replica() == self.replica)
            .map(|e| e.counter())
            .max()
            .map_or(0, |c| {
                c.checked_add(1).expect("event id counter overflowed u64")
            });
        EventId::new(self.replica, next)
    }

    /// The current value of the identified `pitch` in the score graph, if it is
    /// present (a live embedded pitch).
    fn current_pitch(&self, pitch: PitchId) -> Option<Pitch> {
        self.event_and_pitch_of(pitch).map(|(_, value)| value)
    }

    /// The event holding the identified `pitch` and the pitch's current value, if it
    /// is present (a live embedded pitch).
    fn event_and_pitch_of(&self, pitch: PitchId) -> Option<(EventId, Pitch)> {
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        for event in self.score.events.iter() {
            buf.clear();
            event.collect_identified_pitches(&mut buf);
            if let Some(ip) = buf.iter().find(|ip| ip.id == pitch) {
                return Some((event.id(), ip.pitch.clone()));
            }
        }
        None
    }

    /// Mints a fresh [`PitchId`] in the session's replica namespace: one past the
    /// highest pitch counter this replica has ever named. A pitch can leave the
    /// *current* score without being recorded anywhere in it — `DeleteIdentifiedPitch`
    /// tombstones only reducer state, never `Score.tombstoned_pitches` — so the
    /// high-water mark is taken over three sources: the pristine open-time `base`
    /// (catches an opened pitch since deleted), the current score (catches anything a
    /// future reducer change records), and this session's op log (catches a
    /// session-inserted pitch since deleted). Reusing an id would make a later insert
    /// no-op against a tombstone under whole-log reduction. Pitches authored by other
    /// replicas occupy disjoint namespaces and do not constrain it.
    fn mint_pitch_id(&self) -> PitchId {
        let ids = self
            .base
            .live_pitch_ids()
            .into_iter()
            .chain(self.base.tombstoned_pitches.iter().copied())
            .chain(self.score.live_pitch_ids())
            .chain(self.score.tombstoned_pitches.iter().copied())
            .chain(self.applied.iter().flat_map(inserted_pitch_ids));
        let next = ids
            .filter(|p| p.replica() == self.replica)
            .map(|p| p.counter())
            .max()
            .map_or(0, |c| {
                c.checked_add(1).expect("pitch id counter overflowed u64")
            });
        PitchId::new(self.replica, next)
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
    let PitchSpacePosition::Cmn { alteration, .. } = pitch.scale_position.position else {
        return None;
    };
    // Seven nominals to an octave: index diatonically, move, then decompose so a
    // move past B (or below C) carries the octave. Computed in i64 so an extreme
    // `steps` cannot overflow the intermediate before the octave range-check below.
    let diatonic = diatonic_index(pitch)? + steps as i64;
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

/// The diatonic staff index of a CMN pitch — `octave * 7 + nominal`, so two pitches
/// compare by staff position and a step is `± 1`. `None` for a non-CMN position.
fn diatonic_index(pitch: &Pitch) -> Option<i64> {
    match pitch.scale_position.position {
        PitchSpacePosition::Cmn {
            nominal, octave, ..
        } => Some(octave as i64 * 7 + nominal as i64),
        _ => None,
    }
}

/// The pitch one staff step above `top`, for a *newly inserted* note: like
/// [`staff_step`] but with the acoustic realization reset to
/// [`AcousticRealization::Implicit`]. A fresh note must sound at its written
/// position; cloning an explicit absolute-Hz or cents-offset realization from the
/// note it stacks above would make it look higher but sound the same frequency.
fn note_above(top: &Pitch) -> Option<Pitch> {
    let mut above = staff_step(top, 1)?;
    above.acoustic.realization = AcousticRealization::Implicit;
    Some(above)
}

/// The pitch ids a session envelope brought into being — so the minter never reuses
/// one, including ids since deleted (a `DeleteIdentifiedPitch` leaves no trace in the
/// materialized score, so the log is the authoritative record). Both insert ops mint
/// pitches: `InsertIdentifiedPitch` (one) and `InsertEvent` (the event's embedded
/// pitches).
fn inserted_pitch_ids(env: &OperationEnvelope) -> Vec<PitchId> {
    match &env.payload {
        OperationPayload::Primitive(OperationKind::InsertIdentifiedPitch(op)) => vec![op.pitch.id],
        OperationPayload::Primitive(OperationKind::InsertEvent(op)) => op.pitch_ids(),
        _ => Vec::new(),
    }
}

/// The event ids a session envelope brought into being — the event-id analogue of
/// [`inserted_pitch_ids`], so the event minter never reuses a since-deleted id.
fn inserted_event_ids(env: &OperationEnvelope) -> Vec<EventId> {
    match &env.payload {
        OperationPayload::Primitive(OperationKind::InsertEvent(op)) => vec![op.event_id()],
        _ => Vec::new(),
    }
}

/// The transaction id a session envelope declares, if it is a `DeclareTransaction`.
fn declared_transaction_id(env: &OperationEnvelope) -> Option<TransactionId> {
    match &env.payload {
        OperationPayload::Primitive(OperationKind::DeclareTransaction(desc)) => Some(desc.id),
        _ => None,
    }
}

/// Whether two metric spans overlap — `[a_pos, a_pos + a_dur)` against `[b_pos,
/// b_pos + b_dur)`. Mirrors the reducer's `intervals_overlap` (non-positive spans
/// never overlap), so the editor's pre-check matches the reducer's decision.
fn musical_overlap(
    a_pos: &MusicalPosition,
    a_dur: &MusicalDuration,
    b_pos: &MusicalPosition,
    b_dur: &MusicalDuration,
) -> bool {
    if !a_dur.is_positive() || !b_dur.is_positive() {
        return false;
    }
    let a_end = a_pos.clone() + a_dur.clone();
    let b_end = b_pos.clone() + b_dur.clone();
    *a_pos < b_end && *b_pos < a_end
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

/// The authored explicit spelling the resolver would use for `pitch` — the one that
/// pins its rendered staff position — or `None` if there is none. Mirrors
/// `epiphany_core`'s `resolve_spelling`: among engraved-layer, pitch-scoped, explicit
/// attachments whose source outranks `Inferred` under the score's precedence (the
/// default ranks `UserChosen`, `Imported`, and `Propagated` all ahead of `Inferred`),
/// the winner is the lowest precedence rank, then highest priority, then first in
/// canonical order.
fn authored_spelling(score: &Score, pitch: PitchId) -> Option<PitchSpelling> {
    let inferred_rank = score.spelling_precedence.rank(SpellingSourceKind::Inferred);
    score
        .spelling_attachments
        .iter()
        .filter(|att| {
            att.layer.is_none()
                && matches!(&att.scope, SpellingScope::Pitch(p) if *p == pitch)
                && matches!(att.directive, SpellingDirective::Explicit(_))
                && score.spelling_precedence.rank(att.source.kind()) < inferred_rank
        })
        .min_by_key(|att| {
            (
                score.spelling_precedence.rank(att.source.kind()),
                Reverse(att.priority),
            )
        })
        .and_then(|att| match &att.directive {
            SpellingDirective::Explicit(spelling) => Some(spelling.clone()),
            _ => None,
        })
}

/// Whether `pitch` has an authored spelling override that pins its rendered position.
fn has_authored_spelling_override(score: &Score, pitch: PitchId) -> bool {
    authored_spelling(score, pitch).is_some()
}

/// The spelling one or more diatonic staff steps from `spelling`: its CMN nominal
/// moves by `steps` (carrying the octave at the B↔C boundary), with the accidental
/// stack and render hints preserved. `None` for a non-CMN spelling nominal. The
/// spelling analogue of [`staff_step`], used to rebase an override as a pitch moves.
fn staff_step_spelling(spelling: &PitchSpelling, steps: i32) -> Option<PitchSpelling> {
    let SpellingNominal::Cmn(nominal) = spelling.nominal else {
        return None;
    };
    let diatonic = spelling.octave as i64 * 7 + nominal as i64 + steps as i64;
    let new_octave = i8::try_from(diatonic.div_euclid(7)).ok()?;
    Some(PitchSpelling {
        nominal: SpellingNominal::Cmn(nominal_from_index(diatonic.rem_euclid(7))),
        accidentals: spelling.accidentals.clone(),
        octave: new_octave,
        render_hints: spelling.render_hints,
    })
}

/// Renders a score with `solver` to its `RenderIR` + hit-test map, or `None` if the
/// solver's report is diagnostic-only (not renderable).
fn render_score(
    score: &Score,
    solver: &dyn ConstraintSolver,
) -> Option<(ResolvedLayoutIR, RenderIR, HitTestMap)> {
    let report = solver.solve(
        &to_constrained(&to_logical(score)),
        &SolverConfig::default(),
    );
    if !report.status.is_renderable() {
        return None;
    }
    let render = to_render(&report.layout);
    let map = render.hit_test_map();
    Some((report.layout, render, map))
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

    /// Selects the pitch `pid` by finding its notehead in the hit map.
    fn select_pitch(session: &mut EditorSession, pid: PitchId) -> Selection {
        let lo = session
            .hit_test()
            .regions
            .iter()
            .find(|r| r.source == TypedObjectId::Pitch(pid))
            .map(|r| r.layout_object)
            .expect("the notehead is in the hit map");
        session.select(lo).expect("selects the pitch")
    }

    /// A pitch in the last event of the first voice that has events — its slot has
    /// room after it (nothing follows in that voice), so an insert-after applies.
    fn last_event_pitch(session: &EditorSession) -> PitchId {
        let last_eid = session
            .score()
            .voices()
            .filter_map(|(_, _, v)| v.events.last().copied())
            .next()
            .expect("a voice with events");
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        session
            .score()
            .events
            .get(last_eid)
            .unwrap()
            .collect_identified_pitches(&mut buf);
        buf.first().expect("the last event has a pitch").id
    }

    #[test]
    fn open_renders_and_starts_unselected() {
        let session = open_rich(0x5EED);
        assert!(!session.render().primitives.is_empty(), "the score renders");
        assert!(!session.hit_test().regions.is_empty(), "with hit regions");
        assert!(
            !session.resolved().glyphs.is_empty(),
            "the resolved layout a renderer consumes is exposed"
        );
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
    fn move_with_an_authored_spelling_rebases_it_atomically() {
        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pid) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        // Pin the pitch with an explicit user spelling (D4) — what a manual respell
        // leaves. Before this step, a move would have refused.
        session
            .apply(OperationKind::RespellPitch(RespellPitchOp {
                pitch: pid,
                spelling: PitchSpelling::cmn(CmnNominal::D, 4),
            }))
            .expect("the respell applies");
        let before_value = session.current_pitch(pid).unwrap();
        let logged = session.applied_operations().len();

        let outcome = session
            .move_selection_staff_step(1)
            .expect("the override-aware move applies");
        assert!(outcome.graph_changed);
        assert!(outcome.selection_preserved, "the moved pitch survives");

        // Both the value and the pinned spelling moved one staff step: the pitch value
        // steps up, and the override D4 → E4.
        assert_eq!(
            session.current_pitch(pid).unwrap(),
            staff_step(&before_value, 1).unwrap(),
            "the pitch value moved one staff step"
        );
        let spelling = authored_spelling(session.score(), pid).expect("still overridden");
        assert!(matches!(
            spelling.nominal,
            SpellingNominal::Cmn(CmnNominal::E)
        ));
        assert_eq!(spelling.octave, 4, "the override moved D4 → E4");

        // It landed as one atomic transaction: a descriptor plus two members.
        assert_eq!(session.applied_operations().len(), logged + 3);
    }

    #[test]
    fn staff_step_spelling_carries_the_octave_and_keeps_accidentals() {
        // B4 (with an accidental stack) up one step → C5, accidentals + octave carry.
        let b4 = PitchSpelling {
            nominal: SpellingNominal::Cmn(CmnNominal::B),
            accidentals: PitchSpelling::cmn(CmnNominal::B, 4).accidentals,
            octave: 4,
            render_hints: Default::default(),
        };
        let up = staff_step_spelling(&b4, 1).expect("a CMN spelling steps");
        assert!(matches!(up.nominal, SpellingNominal::Cmn(CmnNominal::C)));
        assert_eq!(up.octave, 5);
        assert_eq!(
            up.accidentals, b4.accidentals,
            "the accidental stack is kept"
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
    fn add_note_to_selection_adds_a_chord_note_above_the_top() {
        let mut session = open_rich(0x5EED);
        let before_render = session.render().clone();
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(anchor) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let (event, _) = session.event_and_pitch_of(anchor).expect("anchor is live");
        let top = session.highest_pitch_in_event(event).expect("a CMN note");
        let before = session.score().live_pitch_ids();

        let outcome = session.add_note_to_selection().expect("the add applies");
        assert!(outcome.graph_changed, "a pitch was inserted");
        assert!(outcome.selection_preserved, "the anchor notehead survives");

        // Exactly one new pitch, minted under the session replica, a staff step above
        // the prior top note.
        let after = session.score().live_pitch_ids();
        let new_ids: Vec<_> = after.difference(&before).copied().collect();
        assert_eq!(new_ids.len(), 1, "exactly one new pitch");
        let new_id = new_ids[0];
        assert_eq!(
            new_id.replica(),
            ReplicaId(1),
            "minted under the session replica"
        );
        assert_eq!(
            session.current_pitch(new_id).unwrap(),
            note_above(&top).unwrap(),
            "the new note sits a staff step above the event's top note"
        );
        assert_ne!(&before_render, session.render(), "the chord note shows");
    }

    #[test]
    fn add_note_to_selection_requires_a_pitch_selection() {
        let mut session = open_rich(0x5EED);
        assert_eq!(
            session.add_note_to_selection(),
            Err(EditorError::NoSelection)
        );
    }

    #[test]
    fn re_minting_skips_an_id_used_earlier_in_the_session_log() {
        // Add a chord pitch, delete it, add again. The deleted pitch leaves no trace
        // in the materialized score (graph_delete_pitch does not record it in
        // tombstoned_pitches), so a score-only minter would re-mint it — and the
        // re-add would then no-op against the tombstone under whole-log reduction.
        // The minter consults the op log, so the second add gets a fresh id.
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        session.add_note_to_selection().expect("first add");
        let first = *session
            .score()
            .live_pitch_ids()
            .iter()
            .find(|p| p.replica() == ReplicaId(1))
            .expect("the inserted pitch is under the session replica");

        session
            .apply(OperationKind::DeleteIdentifiedPitch(
                DeleteIdentifiedPitchOp { pitch: first },
            ))
            .expect("the delete applies");

        let outcome = session.add_note_to_selection().expect("the re-add applies");
        assert!(
            outcome.graph_changed,
            "the re-add materialized — a fresh id, not the tombstoned one"
        );
        let live_self: Vec<_> = session
            .score()
            .live_pitch_ids()
            .into_iter()
            .filter(|p| p.replica() == ReplicaId(1))
            .collect();
        assert_eq!(
            live_self,
            vec![PitchId::new(ReplicaId(1), first.counter() + 1)],
            "the re-add minted the next counter, not the deleted id"
        );
    }

    #[test]
    fn re_minting_skips_a_deleted_base_pitch_under_the_session_replica() {
        // The reopen case: the session edits under the same replica that authored some
        // base pitches. Deleting the highest-counter base pitch removes it from the
        // current score without recording it anywhere there, so a score-only minter
        // would reuse its counter. Scanning the pristine `base` keeps the high-water
        // mark.
        let base = valid_score_rich(0x5EED);
        let target = *base
            .live_pitch_ids()
            .iter()
            .max_by_key(|p| p.counter())
            .expect("the fixture has live pitches");
        let replica = target.replica();
        let base_max = base
            .live_pitch_ids()
            .into_iter()
            .chain(base.tombstoned_pitches.iter().copied())
            .filter(|p| p.replica() == replica)
            .map(|p| p.counter())
            .max()
            .unwrap();

        let mut session = EditorSession::open(base, Box::new(StubSolver))
            .unwrap()
            .with_identity(replica, AuthorId(0));
        session
            .apply(OperationKind::DeleteIdentifiedPitch(
                DeleteIdentifiedPitchOp { pitch: target },
            ))
            .expect("the base pitch deletes");

        let minted = session.mint_pitch_id();
        assert_eq!(
            minted,
            PitchId::new(replica, base_max + 1),
            "minted past the base high-water mark, not reusing the deleted pitch"
        );
        assert_ne!(minted, target, "the deleted base pitch was not reused");
    }

    #[test]
    fn add_refuses_when_a_note_in_the_event_has_an_authored_spelling_override() {
        use epiphany_core::PitchSpelling;
        use epiphany_ops::RespellPitchOp;

        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(anchor) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        // Pin the anchor's spelling; "above the top" can no longer be read off raw
        // pitch positions, so the add is refused (as the move intent is).
        session
            .apply(OperationKind::RespellPitch(RespellPitchOp {
                pitch: anchor,
                spelling: PitchSpelling::cmn(CmnNominal::C, 4),
            }))
            .expect("the respell applies");
        assert_eq!(
            session.add_note_to_selection(),
            Err(EditorError::PitchSpellingOverridden)
        );
    }

    #[test]
    fn an_inserted_note_sounds_at_its_written_position_not_a_cloned_frequency() {
        // staff_step preserves acoustic realization (intended for notation-only
        // moves); a freshly inserted note must not inherit an explicit absolute
        // frequency from the note it stacks above — it would look higher but sound
        // identical. note_above resets the realization to Implicit.
        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(anchor) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let mut top = session.current_pitch(anchor).expect("anchor is live");
        top.acoustic.realization = AcousticRealization::absolute_hz(440.0).unwrap();

        let inserted = note_above(&top).expect("a CMN note steps");
        assert_eq!(
            inserted.acoustic.realization,
            AcousticRealization::Implicit,
            "the new note drops the source's explicit frequency"
        );
        assert!(
            diatonic_index(&inserted) > diatonic_index(&top),
            "and it is genuinely a staff step higher"
        );
    }

    #[test]
    fn repeated_adds_mint_distinct_fresh_ids_and_build_a_rising_chord() {
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        let before = session.score().live_pitch_ids();

        session.add_note_to_selection().expect("first add");
        session.add_note_to_selection().expect("second add");

        let after = session.score().live_pitch_ids();
        let mut new_ids: Vec<_> = after.difference(&before).copied().collect();
        assert_eq!(new_ids.len(), 2, "two distinct pitches added");
        // Both minted under the session replica, with distinct (advancing) counters —
        // ids are never reused.
        assert!(new_ids.iter().all(|p| p.replica() == ReplicaId(1)));
        new_ids.sort_by_key(|p| p.counter());
        assert_ne!(new_ids[0].counter(), new_ids[1].counter());
        // The second add stacked above the first: strictly higher staff position.
        let lo = session.current_pitch(new_ids[0]).unwrap();
        let hi = session.current_pitch(new_ids[1]).unwrap();
        assert!(
            diatonic_index(&hi) > diatonic_index(&lo),
            "the chord rises rather than duplicating a position"
        );
    }

    #[test]
    fn insert_note_after_selection_adds_a_following_event() {
        let mut session = open_rich(0x5EED);
        let before_render = session.render().clone();
        let anchor = last_event_pitch(&session);
        select_pitch(&mut session, anchor);

        // The anchor event's voice / end position / value, before the insert.
        let (anchor_event, anchor_value) = session.event_and_pitch_of(anchor).unwrap();
        let ev = session.score().events.get(anchor_event).unwrap();
        let anchor_voice = ev.voice();
        let (EventPosition::Musical(pos), EventDuration::Musical(dur)) =
            (ev.position().clone(), ev.duration().clone())
        else {
            panic!("the fixture's events are metric");
        };
        let expected_position = pos + dur;
        let before_ids: std::collections::BTreeSet<_> =
            session.score().events.iter().map(Event::id).collect();

        let outcome = session
            .insert_note_after_selection()
            .expect("the insert applies after the last note");
        assert!(outcome.graph_changed, "a new event was inserted");
        assert!(outcome.selection_preserved, "the anchor note survives");

        // Exactly one new event, in the anchor's voice, at the next position, with a
        // fresh id under the session replica and a single note copying the anchor.
        let new_eid = session
            .score()
            .events
            .iter()
            .map(Event::id)
            .find(|e| !before_ids.contains(e))
            .expect("a new event");
        assert_eq!(new_eid.replica(), ReplicaId(1));
        let new_event = session.score().events.get(new_eid).unwrap();
        assert_eq!(new_event.voice(), anchor_voice, "same voice as the anchor");
        assert!(
            matches!(new_event.position(), EventPosition::Musical(p) if *p == expected_position),
            "placed immediately after the anchor"
        );
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        new_event.collect_identified_pitches(&mut buf);
        assert_eq!(buf.len(), 1, "a single inserted note");
        assert_eq!(buf[0].pitch, anchor_value, "copies the selected pitch");
        assert_ne!(&before_render, session.render(), "the new note shows");
    }

    #[test]
    fn insert_after_an_already_filled_slot_is_refused() {
        let mut session = open_rich(0x5EED);
        let anchor = last_event_pitch(&session);
        select_pitch(&mut session, anchor);
        // The first insert fills the slot right after the anchor.
        session
            .insert_note_after_selection()
            .expect("the first insert applies");
        let logged = session.applied_operations().len();
        // The selection still anchors the same note, so a second insert-after targets
        // the now-occupied slot and is refused (not silently no-op'd).
        assert_eq!(
            session.insert_note_after_selection(),
            Err(EditorError::InsertSlotOccupied)
        );
        // A pre-apply refusal mints/logs nothing — no dead op is appended.
        assert_eq!(session.applied_operations().len(), logged);
    }

    #[test]
    fn insert_after_an_overridden_pitch_carries_the_spelling() {
        let mut session = open_rich(0x5EED);
        let anchor = last_event_pitch(&session);
        select_pitch(&mut session, anchor);
        // Pin the anchor's spelling (C4). The copy must carry it, not drop it.
        session
            .apply(OperationKind::RespellPitch(RespellPitchOp {
                pitch: anchor,
                spelling: PitchSpelling::cmn(CmnNominal::C, 4),
            }))
            .expect("the respell applies");
        let before_ids: std::collections::BTreeSet<_> =
            session.score().events.iter().map(Event::id).collect();

        let outcome = session
            .insert_note_after_selection()
            .expect("the override-carrying insert applies");
        assert!(outcome.graph_changed);

        // The new event's note carries the same authored spelling (C4).
        let new_eid = session
            .score()
            .events
            .iter()
            .map(Event::id)
            .find(|e| !before_ids.contains(e))
            .expect("a new event");
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        session
            .score()
            .events
            .get(new_eid)
            .unwrap()
            .collect_identified_pitches(&mut buf);
        let new_pitch = buf[0].id;
        let spelling =
            authored_spelling(session.score(), new_pitch).expect("the copy carries an override");
        assert!(matches!(
            spelling.nominal,
            SpellingNominal::Cmn(CmnNominal::C)
        ));
        assert_eq!(
            spelling.octave, 4,
            "the copy's spelling matches the original"
        );
    }

    #[test]
    fn insert_note_after_selection_requires_a_pitch_selection() {
        let mut session = open_rich(0x5EED);
        assert_eq!(
            session.insert_note_after_selection(),
            Err(EditorError::NoSelection)
        );
    }

    #[test]
    fn successive_inserts_mint_distinct_event_ids() {
        let mut session = open_rich(0x5EED);
        let anchor = last_event_pitch(&session);
        select_pitch(&mut session, anchor);
        let before_ids: std::collections::BTreeSet<_> =
            session.score().events.iter().map(Event::id).collect();

        // Insert after the anchor, then after the new (now last) event.
        session.insert_note_after_selection().expect("first insert");
        let first_new = session
            .score()
            .events
            .iter()
            .map(Event::id)
            .find(|e| !before_ids.contains(e))
            .expect("the first new event");
        let next_anchor = last_event_pitch(&session);
        select_pitch(&mut session, next_anchor);
        session
            .insert_note_after_selection()
            .expect("second insert");

        let self_ids: Vec<_> = session
            .score()
            .events
            .iter()
            .map(Event::id)
            .filter(|e| e.replica() == ReplicaId(1))
            .collect();
        assert_eq!(self_ids.len(), 2, "two session-minted events");
        // The minter consulted the log: the first event id survived and the second
        // is a distinct, never-reused id.
        assert!(self_ids.contains(&first_new), "the first event id survived");
        assert_ne!(self_ids[0], self_ids[1], "distinct, never reused");
    }

    #[test]
    fn apply_transaction_lands_all_members_atomically() {
        use epiphany_core::PitchSpelling;
        use epiphany_ops::RespellPitchOp;

        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pitch) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let current = session.current_pitch(pitch).expect("the pitch is live");
        let moved = staff_step(&current, 1).expect("a CMN pitch");

        // A value change and a matching respelling, together.
        let outcome = session
            .apply_transaction(
                "move note",
                Some(TransactionCategory::NoteEntry),
                vec![
                    OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                        pitch,
                        value: moved.clone(),
                    }),
                    OperationKind::RespellPitch(RespellPitchOp {
                        pitch,
                        spelling: PitchSpelling::cmn(CmnNominal::D, 4),
                    }),
                ],
            )
            .expect("the transaction applies");

        assert!(outcome.graph_changed);
        assert!(outcome.selection_preserved, "the edited pitch survives");
        // Both members materialized: the value moved and an authored override landed.
        assert_eq!(session.current_pitch(pitch).unwrap(), moved);
        assert!(has_authored_spelling_override(session.score(), pitch));

        // The log holds the descriptor plus two members under one transaction id.
        let log = session.applied_operations();
        assert_eq!(log.len(), 3);
        let tx_id = log
            .iter()
            .find_map(declared_transaction_id)
            .expect("a declared transaction");
        let members = log.iter().filter(|e| e.transaction == Some(tx_id)).count();
        assert_eq!(members, 2, "two members reference the transaction id");
    }

    #[test]
    fn a_transaction_replays_as_a_clean_atomic_unit() {
        use epiphany_core::PitchSpelling;
        use epiphany_ops::RespellPitchOp;

        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pitch) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let moved = staff_step(&session.current_pitch(pitch).unwrap(), 1).unwrap();
        session
            .apply_transaction(
                "move note",
                None,
                vec![
                    OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                        pitch,
                        value: moved,
                    }),
                    OperationKind::RespellPitch(RespellPitchOp {
                        pitch,
                        spelling: PitchSpelling::cmn(CmnNominal::D, 4),
                    }),
                ],
            )
            .unwrap();

        // Replaying the emitted log reduces with no conflict and nothing pending: the
        // descriptor-precedence rule holds (members cover the descriptor) and the
        // block applies atomically.
        let base = valid_score_rich(0x5EED);
        let mut set = OperationSet::new();
        for env in session.applied_operations() {
            set.accept(env.clone());
        }
        let materialized = set.reduce_onto(&base);
        assert!(
            materialized.state.is_clean(),
            "the transaction replays without a conflict or pending op"
        );
    }

    #[test]
    fn a_transaction_with_a_failing_member_rolls_back_and_is_not_logged() {
        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pitch) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let moved = staff_step(&session.current_pitch(pitch).unwrap(), 1).unwrap();
        // A fresh id is absent from the score, so a modify targeting it fails its
        // precondition — and the reducer rolls back the whole transaction.
        let missing = session.mint_pitch_id();
        let before_score = session.score().clone();
        let before_log = session.applied_operations().len();

        let result = session.apply_transaction(
            "bad move",
            None,
            vec![
                OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                    pitch,
                    value: moved.clone(),
                }),
                OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                    pitch: missing,
                    value: moved,
                }),
            ],
        );
        assert_eq!(result, Err(EditorError::RejectedOperation));
        assert_eq!(&before_score, session.score(), "the score is unchanged");
        assert_eq!(
            session.applied_operations().len(),
            before_log,
            "a rolled-back transaction logs nothing"
        );
    }

    #[test]
    fn an_empty_transaction_is_refused() {
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        assert_eq!(
            session.apply_transaction("empty", None, vec![]),
            Err(EditorError::EmptyTransaction)
        );
        assert!(
            session.applied_operations().is_empty(),
            "no descriptor-only op is logged"
        );
    }

    #[test]
    fn declare_transaction_is_refused_as_a_direct_edit_and_as_a_member() {
        let declare = || {
            OperationKind::DeclareTransaction(TransactionDescriptor {
                id: TransactionId::new(ReplicaId(1), 0),
                label: "decl".to_string(),
                category: None,
            })
        };

        // Directly through apply: the session manages transaction declaration.
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        assert_eq!(
            session.apply(declare()),
            Err(EditorError::DeclareTransactionNotAllowed)
        );
        assert!(
            session.applied_operations().is_empty(),
            "a direct declaration logs nothing"
        );

        // And as a transaction member (a nested no-op declaration).
        assert_eq!(
            session.apply_transaction("outer", None, vec![declare()]),
            Err(EditorError::DeclareTransactionNotAllowed)
        );
        assert!(
            session.applied_operations().is_empty(),
            "a nested declaration logs nothing"
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
