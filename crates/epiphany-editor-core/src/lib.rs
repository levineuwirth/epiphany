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
//! * **undo / redo** ([`EditorSession::undo`], [`EditorSession::redo`]) — re-reducing
//!   the active prefix of the op log without (or with) its last unit, so even a
//!   delete is undone (the CRDT is delete-wins, so a tombstone is re-reduced *away*
//!   rather than inverted);
//! * two views of the op log: [`EditorSession::applied_operations`] (the
//!   currently-applied prefix, which shrinks on undo) and
//!   [`EditorSession::authored_operations`] (every envelope ever minted, the
//!   **append-only**, monotonic-id record). The authored log is a *local* high-water
//!   source, not streaming-consistent undo — a peer replaying it would re-apply
//!   undone units (undo is a local prefix change, not an operation).
//!
//! The session is **solver-agnostic**: it holds a `Box<dyn ConstraintSolver>`, so a
//! GUI plugs in the real `Engraver`, the stub, or any conformant solver. It
//! produces a [`RenderIR`]; turning that into pixels is the renderer's job.

mod barriers;

pub use barriers::ActiveExtension;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use epiphany_core::prepass::{derive_annotations, DerivedAnnotations, PrePassProfile};
use epiphany_core::{
    AcousticPitch, AcousticRealization, Clef, CmnNominal, Event, EventDuration, EventId,
    EventPosition, IdentifiedPitch, MusicalDuration, MusicalPosition, OperationId, Pitch, PitchId,
    PitchSpaceId, PitchSpacePosition, PitchSpelling, PitchedEvent, RationalTime, RegionId,
    RegionTimeModel, ReplicaId, ScalePosition, Score, SpellingDirective, SpellingNominal,
    SpellingScope, SpellingSourceKind, StaffId, StaffInstance, StaffInstanceId, StemConfiguration,
    TimeSignature, TimeSignatureDisplay, TransactionId, TuningReference, TupletId, TypedObjectId,
    VoiceId, WallClockTime,
};
use epiphany_layout_ir::{
    active_clef, manifestation_layout_id, staff_step_pitch, to_constrained, to_logical, to_render,
    ConstraintSolver, ExtensionRef, HitTestMap, LayoutContent, LayoutObjectId, LogicalLayoutIR,
    Point, Rect, RenderIR, ResolvedLayoutIR, ResolvedSystem, SolverConfig, TimePoint,
};
use epiphany_ops::{
    advisory_violations, AcceptOutcome, AuthorId, CausalContext, DeleteEventOp,
    DeleteIdentifiedPitchOp, HybridLogicalClock, InsertEventOp, InsertIdentifiedPitchOp,
    ModifyEventOp, ModifyIdentifiedPitchOp, OperationEnvelope, OperationKind, OperationKindTag,
    OperationPayload, OperationSet, OperationStamp, RespellPitchOp, TransactionCategory,
    TransactionDescriptor, TransposeOp, TupletCompensation,
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

/// What a click at a world point resolves to **vertically**: the staff under the
/// cursor and the natural diatonic pitch at that height under the staff's clef. The
/// vertical half of click-to-insert (the horizontal half — the musical position — is
/// a separate query). The accidental is left natural; a caller respells if needed.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct StaffPitch {
    /// The staff instance the click is over (nearest staff).
    pub staff_instance: StaffInstanceId,
    /// The diatonic nominal at the clicked height.
    pub nominal: CmnNominal,
    /// The octave (scientific-pitch) at the clicked height.
    pub octave: i8,
}

/// The beat grid a click-to-insert snaps to: the musical-time `step` between insert
/// positions (and the natural default written duration of a note entered there) —
/// a DAW's "grid" setting. The caller chooses the resolution; deriving a default
/// from the region's meter is a later refinement, so this carries no meter logic.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GridResolution {
    /// The grid step from the region origin; insert positions are its multiples.
    pub step: MusicalDuration,
}

impl GridResolution {
    /// A quarter-note grid — a sensible default for a client that has not chosen a
    /// resolution (a meter-derived default is a later refinement).
    pub fn quarter() -> Self {
        // `1/4` is always representable (a non-zero denominator).
        GridResolution {
            step: MusicalDuration(RationalTime::new(1, 4).expect("1/4 is a valid duration")),
        }
    }
}

/// What a click at a world point resolves to **horizontally**: the metric region
/// under the cursor and the grid-snapped musical position to insert at. The vertical
/// half (the pitch) is a [`StaffPitch`]; a click-to-insert combines the two.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GridPosition {
    /// The metric region the position belongs to.
    pub region: RegionId,
    /// The grid-snapped onset to insert at.
    pub position: MusicalPosition,
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
    /// An insert-after would land on a musical position already occupied by another
    /// event in the same voice (the reducer would silently no-op it). The edit is
    /// refused; inserting into a packed voice needs an explicit make-room policy.
    InsertSlotOccupied,
    /// A click-to-insert resolved to no insert target: the point is off any staff, the
    /// region is non-metric or has too few rendered events to place a position, or the
    /// staff has no voice / no diatonic clef. Nothing is inserted.
    NoInsertTarget,
    /// A make-room overwrite could not treat an overlapped tuplet atomically. Tuplets are
    /// atomic: a pencil overwrite touching any member of a *flat* tuplet cascades the whole
    /// tuplet away (every member tombstoned, structure removed), freeing its span for the
    /// new note. This error is raised only when that cascade is not available — the
    /// overlapped member belongs to a **nested** tuplet (parent or child), whose ratio
    /// arithmetic a flat cascade would misstate. Also raised by a *resize*
    /// ([`set_selection_duration`](EditorSession::set_selection_duration)) of a tuplet
    /// member itself: in-place rescaling of a member is a later refinement.
    OverlapsTuplet,
    /// A duration edit was given a non-positive duration, which is not a valid written
    /// note value. Nothing changes.
    InvalidDuration,
    /// A duration change (a resize, or a make-room trim/split) would alter an event that
    /// carries a persistent **decomposition attachment** — its notated components would
    /// no longer sum to the event's duration (invariant 15). Editing decompositions is a
    /// later refinement, so the edit is refused rather than left inconsistent. (A delete
    /// is fine; the tombstoned target's decomposition is no longer checked.)
    DecomposedEvent,
    /// The operation failed an advisory precondition against the current materialized
    /// score (Chapter 6 §"Validation Modes"). The session is **authoring mode**: all
    /// preconditions are enforced, and advisory failures refuse the edit *before an
    /// envelope is minted* — nothing enters the op log, and a peer never sees the
    /// operation. (Replay/remote reduction enforces only invariant preconditions, so
    /// the same envelope, had it been minted elsewhere, would reduce cleanly.)
    AdvisoryViolation {
        /// Every advisory precondition the operation failed.
        violations: Vec<epiphany_ops::AdvisoryViolation>,
    },
    /// The edit matches an active edit barrier (Chapter 8 §"Behavior Under
    /// Unknown Extensions": *edits MUST be checked against every active edit
    /// barrier; a match is prohibited unless the user explicitly performs an
    /// unsafe edit*). Nothing is minted and nothing changes. Crossing the
    /// barrier deliberately is [`EditorSession::apply_unsafe`] /
    /// [`EditorSession::apply_transaction_unsafe`], which acknowledges the loss
    /// of the named extension's data.
    BarrierProhibited {
        /// The extension whose edit barrier prohibits the edit.
        extension: ExtensionRef,
        /// The prohibited operation class.
        operation: OperationKindTag,
    },
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
            EditorError::InsertSlotOccupied => {
                f.write_str("the position after the selection is already occupied in its voice")
            }
            EditorError::EmptyTransaction => {
                f.write_str("a transaction must have at least one member operation")
            }
            EditorError::DeclareTransactionNotAllowed => {
                f.write_str("a transaction cannot be declared through an edit; the session manages transactions")
            }
            EditorError::NoInsertTarget => {
                f.write_str("the click resolved to no insert target (off-staff, non-metric, or no voice)")
            }
            EditorError::OverlapsTuplet => {
                f.write_str("the edit would have to alter a nested tuplet, or resize a tuplet member")
            }
            EditorError::InvalidDuration => f.write_str("a non-positive duration is not a note value"),
            EditorError::DecomposedEvent => {
                f.write_str("the edit would change the duration of an event with a decomposition")
            }
            EditorError::AdvisoryViolation { violations } => {
                write!(
                    f,
                    "the edit failed {} advisory precondition(s) (authoring mode): {violations:?}",
                    violations.len()
                )
            }
            EditorError::BarrierProhibited {
                extension,
                operation,
            } => {
                write!(
                    f,
                    "the edit ({operation:?}) is prohibited by an edit barrier declared by \
                     extension {extension:?}; only an explicit unsafe edit may cross it \
                     (tombstoning that extension's data)"
                )
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
    // The clef in force at the start of each manifested staff, resolved by time from
    // the logical layout's `PlacedClef`s (not vector order). Keyed by the manifesting
    // `(region, staff)`, since one staff can be tiled into several regions. The
    // vertical half of click-to-insert reads this to spell the clicked height.
    start_clefs: BTreeMap<(RegionId, StaffId), Clef>,
    selection: Option<Selection>,
    // Operation-minting identity. A real client supplies its own replica/author.
    // Minted operations form this replica's monotonic local history; the next op's
    // counter is `authored.len()` (never reused across undo), so a failed apply consumes
    // no id. The causal context covers the active prefix, not a fixed counter range, so a
    // fork (undo + new edit) does not strand a later op pending behind a removed unit.
    replica: ReplicaId,
    author: AuthorId,
    // The currently-applied envelopes, in order — the log the session re-reduces onto
    // `base` to materialize `score`. One user action appends one unit (a primitive is
    // one envelope; a transaction is its descriptor plus members). Undo moves the last
    // unit to `redo_stack` and re-reduces the shorter prefix; a new edit clears the redo
    // stack. (A delete tombstones permanently in the CRDT, so undo cannot invert it —
    // it re-reduces *without* the delete instead, which is why the log drives the score
    // rather than a stack of inverse operations.)
    applied: Vec<OperationEnvelope>,
    // The size (envelope count) of each applied unit, in order, so undo can drop a whole
    // unit at once. `undo_units.iter().sum() == applied.len()`.
    undo_units: Vec<usize>,
    // Units undone and available to redo, most-recently-undone last (LIFO). Cleared by a
    // new edit; a redo re-appends the top unit and re-reduces.
    redo_stack: Vec<Vec<OperationEnvelope>>,
    // The append-only record of *every* envelope this session has ever minted — active,
    // undone, and forked-away. It never shrinks, so it is the monotonic high-water source
    // for new operation, event, pitch, and transaction ids: undoing an edit must not let
    // a later edit re-mint its ids (which would equivocate a streamed op or collide an
    // entity id). A new op's counter is `authored.len()`; minting scans this, not the
    // active prefix. (`applied` is always a — possibly non-contiguous, after a fork —
    // subsequence of `authored`.)
    authored: Vec<OperationEnvelope>,
    // The active extension declarations whose edit barriers gate edits (Chapter 8
    // §"Behavior Under Unknown Extensions"). Injected via `set_active_extensions`
    // (the session opens on a bare `Score`, so it never reads a manifest itself);
    // empty means no barriers, i.e. every edit passes the gate.
    active_extensions: Vec<ActiveExtension>,
    // Extensions crossed by an unsafe edit. Per the spec, an unsafe edit MUST
    // tombstone the crossed extension's chunks; the session has no bundle
    // plumbing, so it records the obligation here (see
    // `extensions_requiring_tombstone`) and deactivates the extension's barriers.
    pending_extension_tombstones: BTreeSet<ExtensionRef>,
}

/// One materialization of the op log: the reduced score and everything derived from it
/// that the session installs together (so the render, hit-test, and clef table never
/// disagree with the score). Produced by [`EditorSession::materialize`].
struct Materialization {
    score: Score,
    start_clefs: BTreeMap<(RegionId, StaffId), Clef>,
    resolved: ResolvedLayoutIR,
    render: RenderIR,
    map: HitTestMap,
}

impl EditorSession {
    /// Opens a session on `score` with `solver`, rendering immediately. Errors with
    /// [`EditorError::NotRenderable`] if the initial layout is diagnostic-only.
    pub fn open(score: Score, solver: Box<dyn ConstraintSolver>) -> Result<Self, EditorError> {
        let (start_clefs, resolved, render, map) =
            render_score(&score, solver.as_ref()).ok_or(EditorError::NotRenderable)?;
        Ok(EditorSession {
            base: score.clone(),
            score,
            solver,
            resolved,
            render,
            map,
            start_clefs,
            selection: None,
            replica: ReplicaId(1),
            author: AuthorId(0),
            applied: Vec::new(),
            undo_units: Vec::new(),
            redo_stack: Vec::new(),
            authored: Vec::new(),
            active_extensions: Vec::new(),
            pending_extension_tombstones: BTreeSet::new(),
        })
    }

    /// Overrides the replica/author the session mints operations under (a GUI sets
    /// these to the local editing identity). Defaults to `ReplicaId(1)` / author 0.
    ///
    /// **Pre-edit only** — panics if called after any edit (including ones since undone).
    /// A session's op ids are one replica's monotonic history; switching identity
    /// mid-stream would continue the counter under a new replica, leaving a
    /// `(new_replica, 0)` hole that the missing-predecessor rule would hold pending. The
    /// guard is on the **authored** history (every id ever minted), not the active
    /// prefix, so undoing back to an empty prefix does not reopen it.
    pub fn with_identity(mut self, replica: ReplicaId, author: AuthorId) -> Self {
        assert!(
            self.authored.is_empty(),
            "with_identity must be set before any edit: a session's op log is a \
             single replica's monotonic history"
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

    /// The operations **currently applied**, oldest first — the active prefix the
    /// materialized score reduces from. It grows by one unit per successful edit and
    /// **shrinks on [`undo`](Self::undo)** (a redo re-extends it), so it is not an
    /// append-only history; the session keeps that separately as the monotonic id
    /// source. A rejected or non-renderable edit leaves it untouched.
    pub fn applied_operations(&self) -> &[OperationEnvelope] {
        &self.applied
    }

    /// Every envelope this session has ever minted, in mint order — the **append-only**
    /// authored history, including units since undone or forked away by a later edit.
    /// This is the monotonic high-water source that keeps ids unique; the currently
    /// applied subset is [`applied_operations`](Self::applied_operations).
    pub fn authored_operations(&self) -> &[OperationEnvelope] {
        &self.authored
    }

    /// The most recently applied operation, if any (the tail of
    /// [`Self::applied_operations`]).
    pub fn last_applied(&self) -> Option<&OperationEnvelope> {
        self.applied.last()
    }

    /// Installs the active extension set whose edit barriers gate subsequent
    /// edits (Chapter 8 §"Behavior Under Unknown Extensions": edits MUST be
    /// checked against every active edit barrier). Replaces any previous set.
    ///
    /// The session opens on a bare [`Score`], not a bundle, so it never reads a
    /// manifest itself: whoever opened the bundle decodes each declaration's
    /// `edit_barriers` blob ([`epiphany_layout_ir::decode_edit_barriers`]) and
    /// injects the result here. A session with no active extensions (the
    /// default) has no barriers, and every edit passes the gate.
    pub fn set_active_extensions(&mut self, extensions: Vec<ActiveExtension>) {
        self.active_extensions = extensions;
    }

    /// The currently active extension declarations (barriers included). An
    /// extension crossed by an unsafe edit is removed from this set — its data
    /// is bound for tombstoning, so its barriers no longer bind.
    pub fn active_extensions(&self) -> &[ActiveExtension] {
        &self.active_extensions
    }

    /// Extensions crossed by an unsafe edit in this session, whose chunks MUST
    /// be tombstoned per the spec (§"Behavior Under Unknown Extensions": *the
    /// unsafe-edit operation MUST tombstone the relevant extension chunks (so
    /// they are no longer preserved) rather than silently breaking extension
    /// invariants*). The session has no bundle plumbing of its own, so it
    /// records the obligation: the next bundle write reads this set and drops
    /// the named extensions' declarations (and with them their
    /// `preserved_chunk_roots`) from the manifest it commits.
    pub fn extensions_requiring_tombstone(&self) -> &BTreeSet<ExtensionRef> {
        &self.pending_extension_tombstones
    }

    /// The first active-barrier match for `kind`, as the refusal `apply` /
    /// `apply_transaction` surface (spec: a matching edit *is prohibited*).
    fn barrier_refusal(&self, kind: &OperationKind) -> Option<EditorError> {
        self.crossed_extensions(kind)
            .into_iter()
            .next()
            .map(|extension| EditorError::BarrierProhibited {
                extension,
                operation: kind.tag(),
            })
    }

    /// Every active extension with a barrier prohibiting `kind` against the
    /// current materialized score — the extensions an unsafe edit crosses.
    fn crossed_extensions(&self, kind: &OperationKind) -> Vec<ExtensionRef> {
        if self.active_extensions.is_empty() {
            return Vec::new();
        }
        let subjects = barriers::subjects_of(kind, &self.score);
        barriers::prohibiting_extensions(
            &self.active_extensions,
            kind.tag(),
            &subjects,
            &barriers::ScoreOracle(&self.score),
        )
    }

    /// Records that an unsafe edit crossed `extension`: its chunks are now
    /// bound for tombstoning ([`Self::extensions_requiring_tombstone`]), and —
    /// since its data will no longer be preserved — its barriers no longer
    /// bind, so the declaration leaves the active set.
    fn record_unsafe_crossing(&mut self, extension: ExtensionRef) {
        self.pending_extension_tombstones.insert(extension);
        self.active_extensions.retain(|e| e.extension != extension);
    }

    /// The staff and diatonic pitch at a world `point` — the **vertical half** of a
    /// click-to-insert. Finds the staff the click is over (the nearest staff by its
    /// rendered line band) and the natural pitch at that height under the staff's
    /// clef. `None` if there is no staff, the staff has no diatonic clef (percussion),
    /// or the height is out of representable range. The horizontal half (the musical
    /// position to insert at) is a separate query.
    pub fn staff_pitch_at(&self, point: Point) -> Option<StaffPitch> {
        let (region, si, origin) = self.nearest_manifestation(point)?;
        // The clef in force at the staff start, resolved by time when the layout was
        // built (a mid-staff clef change is a later refinement); default treble when
        // none is declared.
        let clef = self
            .start_clefs
            .get(&(region, si.staff))
            .copied()
            .unwrap_or_default();
        // Each step is half a staff space, +y up; round to the nearest line/space.
        let step = ((point.y.0 - origin) * 2.0).round() as i32;
        let (nominal, octave) = staff_step_pitch(step, &clef)?;
        Some(StaffPitch {
            staff_instance: si.id,
            nominal,
            octave,
        })
    }

    /// The manifested staff a world `point` is nearest — its `(region, staff
    /// instance)` and the staff's step-origin `y`. Both halves of click-to-insert —
    /// [`Self::staff_pitch_at`] (pitch) and [`Self::position_at`] (position) —
    /// select the staff/region through this. `None` on a non-finite point or no
    /// staff line.
    ///
    /// When the layout carries real cast-off page geometry, the staff is resolved
    /// *within the system under the click* ([`Self::system_manifestation`]):
    /// casting-off splits a staff's lines per system, and only the first segment
    /// keeps the manifestation `stable_id`, so the flat scan below would always
    /// answer with system 1's origin/span. Without cast geometry (a solver that
    /// does not cast off, e.g. the stub), the flat scan is the whole story:
    /// horizontal span first (which region — one staff tiles across regions that
    /// can share a y band), then the vertical band (which staff within it). The
    /// bottom staff line carries the staff's manifestation id as its stroke
    /// `stable_id`, which is how a rendered line maps back to
    /// `(region, staff_instance)`.
    fn nearest_manifestation(&self, point: Point) -> Option<(RegionId, &StaffInstance, f32)> {
        // Reject a non-finite click up front: `dist_to_band`'s `<`/`>` would let a
        // NaN fall through as distance 0 (matching every staff), and downstream a
        // `round() as i32` would saturate a non-finite height into a bogus step. A
        // malformed view transform yields no manifestation rather than an arbitrary one.
        if !point.x.0.is_finite() || !point.y.0.is_finite() {
            return None;
        }
        if let Some(found) = self.system_manifestation(point) {
            return Some(found);
        }
        let mut best: Option<(RegionId, &StaffInstance, f32)> = None;
        let mut best_dist = (f32::INFINITY, f32::INFINITY);
        for (region, si) in self.score.staff_instances() {
            let manifest = manifestation_layout_id(&TypedObjectId::Staff(si.staff), region);
            let Some(bottom) = self
                .resolved
                .strokes
                .iter()
                .find(|s| s.provenance.stable_id == manifest)
            else {
                continue;
            };
            // The bottom line is horizontal; its `y` is the step origin and its
            // endpoints bound the staff's horizontal extent.
            let origin = bottom.from.y.0;
            let span = (
                bottom.from.x.0.min(bottom.to.x.0),
                bottom.from.x.0.max(bottom.to.x.0),
            );
            let dist = (
                dist_to_band(point.x.0, span),
                dist_to_band(point.y.0, (origin, origin + STAFF_SPAN)),
            );
            if dist < best_dist {
                best_dist = dist;
                best = Some((region, si, origin));
            }
        }
        best
    }

    /// The manifested staff under `point`, resolved through the cast-off page tree
    /// — the multi-system path of [`Self::nearest_manifestation`]. Finds the system
    /// under the click ([`Self::containing_system`]), reads the region it manifests
    /// from its provenance, picks the vertically nearest staff band among the
    /// system's staff records, and recovers that staff's step origin **in this
    /// system**. `None` when no system carries real geometry (the caller then falls
    /// back to the flat stroke scan), or when the containing system carries no
    /// usable staff record.
    fn system_manifestation(&self, point: Point) -> Option<(RegionId, &StaffInstance, f32)> {
        let system = self.containing_system(point)?;
        // A system manifests one region, and carries it as its provenance source
        // whether it is the region's first system (the region's own provenance) or
        // a later one (synthesized under `EngravedBreak` *from the region*) — read
        // the identity from the data rather than assuming which system this is.
        let TypedObjectId::Region(region) = system.provenance.source else {
            return None;
        };
        // The nearest staff band vertically: within one system the region is fixed,
        // and its staves are stacked in disjoint y bands, so — unlike the flat
        // scan, where x picks the region first — the vertical distance alone is
        // the discriminator.
        let staff = system
            .staves
            .iter()
            .filter(|s| rect_is_real(&s.bounding_box))
            .min_by(|a, b| {
                let da = dist_to_band(point.y.0, rect_y_band(&a.bounding_box));
                let db = dist_to_band(point.y.0, rect_y_band(&b.bounding_box));
                da.total_cmp(&db)
            })?;
        // The staff record's provenance is its bottom-most rendered line *in this
        // system* (`build_system` in the engraver's casting pass); that stroke's
        // height is the exact step origin the pitch math expects. Fall back to
        // deriving it from the staff's box, whose vertical extent is the 5-line
        // span padded by the line half-thickness on both sides — the bottom line
        // sits half the (span + padding) height above the box bottom, minus half
        // the span.
        let origin = self
            .resolved
            .strokes
            .iter()
            .find(|s| s.provenance.stable_id == staff.provenance.stable_id)
            .map(|s| s.from.y.0)
            .unwrap_or_else(|| {
                let b = &staff.bounding_box;
                b.origin.y.0 + b.size.height.0 / 2.0 - STAFF_SPAN / 2.0
            });
        let si = self
            .score
            .staff_instances()
            .find(|(r, si)| *r == region && si.staff == staff.staff)
            .map(|(_, si)| si)?;
        Some((region, si, origin))
    }

    /// The cast-off system whose bounding box contains `point`, or — when the
    /// point is in the gutter between systems — the **nearest system by vertical
    /// distance**: systems on a page all start at the left margin, so they overlap
    /// in x and are disjoint in y, making the y band the discriminator (and a
    /// click slightly above/below a system still resolves, mirroring the flat
    /// path's nearest-staff tolerance). Only a system with real (non-degenerate)
    /// geometry is a candidate: a solver that does not cast off (the stub) emits
    /// zero-size boxes, and those must not capture clicks — `None` sends the
    /// caller down the flat single-system path unchanged.
    fn containing_system(&self, point: Point) -> Option<&ResolvedSystem> {
        if !point.x.0.is_finite() || !point.y.0.is_finite() {
            return None;
        }
        let mut nearest: Option<&ResolvedSystem> = None;
        let mut nearest_dy = f32::INFINITY;
        for system in self.resolved.pages.iter().flat_map(|p| p.systems.iter()) {
            let bounds = &system.bounding_box;
            if !rect_is_real(bounds) {
                continue;
            }
            if rect_contains(bounds, point) {
                return Some(system);
            }
            let dy = dist_to_band(point.y.0, rect_y_band(bounds));
            // Strict `<`: on a tie, the earlier system in page/reading order wins
            // (deterministic, and the gutter midpoint resolves upward).
            if dy < nearest_dy {
                nearest_dy = dy;
                nearest = Some(system);
            }
        }
        nearest
    }

    /// The musical position a world `point` snaps to on the beat grid — the
    /// **horizontal half** of a click-to-insert. Finds the metric region under the
    /// cursor, inverts the click's `x` to a raw musical position (piecewise-linear
    /// through the region's rendered event anchors), then snaps it to `grid`. `None`
    /// if the click is off any staff, the region is non-metric (a proportional or
    /// aleatoric region has no musical position to land on), `grid` is non-positive,
    /// or there are fewer than two rendered metric events to fix a scale from.
    /// The vertical half (the pitch) is [`Self::staff_pitch_at`].
    ///
    /// In a cast-off multi-system layout the inverse works **within the system
    /// under the click**: each system restarts at the page's left margin, so one x
    /// names a different time on each system. A click right of a system's last
    /// anchor extrapolates that system's end segment (the empty staff after its
    /// last note — the same end-extrapolation as the flat layout, and it may name
    /// a time that *renders* on the next system: the result is a musical position,
    /// not a system-local one); a click left of its first anchor extrapolates
    /// backward and clamps at the region origin; and a system rendering fewer than
    /// two of the region's anchors yields `None`, the per-system reading of the
    /// two-anchor rule above.
    pub fn position_at(&self, point: Point, grid: &GridResolution) -> Option<GridPosition> {
        if !grid.step.is_positive() {
            return None;
        }
        let (region, _, _) = self.nearest_manifestation(point)?;
        // Only a metric region has a musical position to land on (a proportional or
        // aleatoric region is measured in wall-clock / DAG order, not metric onsets).
        if !self.region_is_metric(region) {
            return None;
        }
        // Constrain the anchors to the system under the click: casting-off bakes
        // every system back to the left margin, so the region-wide anchor list is
        // x-non-monotonic in time, and inverting through it would map a later
        // system's click onto the first system's times. Without cast geometry
        // (`containing_system` is `None` — the stub) the whole region is one flat
        // monotonic run, unchanged.
        let system_box = self.containing_system(point).map(|s| s.bounding_box);
        // Two anchors fix the x→time scale; with fewer, the spacing density is
        // unknown, so there is nothing to extrapolate an empty-space position from.
        let anchors = self.position_anchors(region, system_box.as_ref());
        if anchors.len() < 2 {
            return None;
        }
        let raw = invert_x(&anchors, point.x.0);
        let position = snap_to_grid(raw, &grid.step);
        Some(GridPosition { region, position })
    }

    /// A sensible [`GridResolution`] for a click at `point`, derived from the meter of
    /// the region under it (its governing time signature's beat — `1/denominator`),
    /// defaulting to a quarter when there is no determinable meter. A GUI uses this so
    /// the pencil snaps to the score's beat instead of a fixed value. `None` only when
    /// the point resolves to no staff (a non-finite point, or nothing rendered).
    pub fn default_grid_at(&self, point: Point) -> Option<GridResolution> {
        let (region, _, _) = self.nearest_manifestation(point)?;
        Some(self.region_default_grid(region))
    }

    /// The meter-derived default [`GridResolution`] for `region`: the beat (`1/
    /// denominator`) of its governing time signature — the first one a region measure
    /// references, else the score's first — and a quarter note when none is determinable
    /// (or the meter has no single denominator). Mirrors the prepass's single-governing-
    /// meter-per-region resolution (`resolve_measure_units`).
    fn region_default_grid(&self, region: RegionId) -> GridResolution {
        let governing = self
            .score
            .canvas
            .regions
            .iter()
            .find(|r| r.id == region)
            .and_then(|graph_region| {
                graph_region
                    .staff_instances()
                    .iter()
                    .flat_map(|si| si.measures.iter())
                    .filter_map(|measure| measure.time_signature.as_ref())
                    .find_map(|tsid| self.score.time_signatures.iter().find(|ts| &ts.id == tsid))
                    .or_else(|| self.score.time_signatures.first())
            });
        match governing.and_then(time_signature_beat) {
            Some(step) => GridResolution { step },
            None => GridResolution::quarter(),
        }
    }

    /// Whether `region` uses metric time (positions are musical onsets).
    fn region_is_metric(&self, region: RegionId) -> bool {
        self.score
            .canvas
            .regions
            .iter()
            .find(|r| r.id == region)
            .is_some_and(|r| matches!(r.time_model, RegionTimeModel::Metric(_)))
    }

    /// The `(musical onset, leftmost rendered x)` anchors of `region`'s metric events,
    /// in ascending time order — the samples the horizontal inverse interpolates. A
    /// glyph maps to its onset through its `Pitch`/`Event` provenance source; the
    /// leftmost glyph at an onset (the notehead/stem column) fixes that onset's x.
    ///
    /// With `within` (a cast-off system's bounding box), only glyphs positioned
    /// inside that box are sampled: casting-off restarts every system at the left
    /// margin, so the region-wide list is x-non-monotonic in time, and the inverse
    /// must see a single system's monotonic run. `None` samples the whole region —
    /// the flat single-system behavior.
    fn position_anchors(
        &self,
        region: RegionId,
        within: Option<&Rect>,
    ) -> Vec<(MusicalPosition, f32)> {
        // Source id (event or one of its pitches) → the event's metric onset.
        let mut onset: HashMap<TypedObjectId, MusicalPosition> = HashMap::new();
        let mut pitches: Vec<&IdentifiedPitch> = Vec::new();
        for (rid, _, voice) in self.score.voices() {
            if rid != region {
                continue;
            }
            for event_id in &voice.events {
                let Some(event) = self.score.events.get(*event_id) else {
                    continue;
                };
                let EventPosition::Musical(at) = event.position() else {
                    continue;
                };
                onset.insert(TypedObjectId::Event(*event_id), at.clone());
                pitches.clear();
                event.collect_identified_pitches(&mut pitches);
                for ip in &pitches {
                    onset.insert(TypedObjectId::Pitch(ip.id), at.clone());
                }
            }
        }
        // Leftmost glyph x per onset, gathered in time order (BTreeMap key = onset).
        let mut by_onset: BTreeMap<MusicalPosition, f32> = BTreeMap::new();
        for glyph in &self.resolved.glyphs {
            // Only the directly-manifested onset glyph (the notehead/rest) fixes the
            // onset's x. Skip synthesized glyphs: an accidental shares its note's
            // `Pitch` source but is placed *left* of the notehead, so taking it as the
            // anchor would pull the onset's x off the time column.
            if glyph.provenance.synthesis.is_some() {
                continue;
            }
            // Constrain to the requested system's box: a glyph on another system
            // must not contribute an anchor to this system's monotonic run.
            if within.is_some_and(|bounds| !rect_contains(bounds, glyph.position)) {
                continue;
            }
            if let Some(at) = onset.get(&glyph.provenance.source) {
                let x = glyph.position.x.0;
                by_onset
                    .entry(at.clone())
                    .and_modify(|cur| *cur = cur.min(x))
                    .or_insert(x);
            }
        }
        by_onset.into_iter().collect()
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
    /// identity, carrying `payload`, an optional `transaction` membership, and the
    /// `causal_context` the caller derived from the active head — the bookkeeping a GUI
    /// would otherwise assemble by hand. Pure: it reads no mutable state, so a failed
    /// apply consumes no id.
    fn envelope_at(
        &self,
        counter: u64,
        payload: OperationPayload,
        transaction: Option<TransactionId>,
        causal_context: CausalContext,
    ) -> OperationEnvelope {
        let id = OperationId::new(self.replica, counter);
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

    /// The causal context a newly-minted op carries: it covers every **currently
    /// applied** op (so two sequential same-target edits read as intentional overwrites,
    /// the later covering the earlier, rather than concurrent conflicts). Derived from
    /// the active head's own context plus the head, so it covers the whole active prefix
    /// without asserting coverage of any unit since undone or forked away — which would
    /// otherwise leave the new op held pending behind a missing predecessor. Empty when
    /// nothing is applied (a root op). The compact contiguous form is preserved while
    /// the active prefix has no holes (the common case); a fork records the active tail
    /// as dots (see [`extend_context`]).
    fn active_prior_context(&self) -> CausalContext {
        match self.applied.last() {
            None => CausalContext::new(),
            Some(head) => extend_context(head.causal_context.clone(), head.id),
        }
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
        let unit = new.len();
        let kept = self.applied.len();
        // Append the unit tentatively and materialize the whole prefix; on any failure
        // (a rejected op, a rolled-back transaction, a diagnostic-only layout) truncate
        // back so the failed edit leaves all state — the op log included — untouched.
        self.applied.extend(new.iter().cloned());
        match self.materialize(&self.applied) {
            Ok(materialized) => {
                let graph_changed = materialized.score != self.score;
                // Record the unit permanently (the monotonic id source) and as the new
                // active tail; a new edit forks history past any undos.
                self.authored.extend(new);
                self.undo_units.push(unit);
                self.redo_stack.clear();
                self.install(materialized);
                let selection_preserved = self.reresolve_selection();
                Ok(EditOutcome {
                    graph_changed,
                    selection_preserved,
                })
            }
            Err(err) => {
                self.applied.truncate(kept);
                Err(err)
            }
        }
    }

    /// Undoes the most recent edit (one user action — a primitive, or a whole
    /// transaction). Re-reduces the log without that unit, so even a delete is undone
    /// (its tombstone is simply never produced). The unit moves to the redo stack.
    /// `None` if there is nothing to undo.
    pub fn undo(&mut self) -> Option<EditOutcome> {
        let unit = *self.undo_units.last()?;
        let cut = self.applied.len() - unit;
        // A prefix of a valid log is itself valid (units are dropped from the end, so
        // every remaining op's causal predecessors are still present), so this succeeds.
        let materialized = self.materialize(&self.applied[..cut]).ok()?;
        let graph_changed = materialized.score != self.score;
        let undone = self.applied.split_off(cut);
        self.redo_stack.push(undone);
        self.undo_units.pop();
        self.install(materialized);
        let selection_preserved = self.reresolve_selection();
        Some(EditOutcome {
            graph_changed,
            selection_preserved,
        })
    }

    /// Redoes the most recently undone edit, re-appending its unit and re-reducing.
    /// `None` if there is nothing to redo (a new edit clears the redo stack).
    pub fn redo(&mut self) -> Option<EditOutcome> {
        let unit = self.redo_stack.last()?.clone();
        let kept = self.applied.len();
        self.applied.extend(unit.iter().cloned());
        let Ok(materialized) = self.materialize(&self.applied) else {
            self.applied.truncate(kept); // leave the redo stack intact on the (unexpected) failure
            return None;
        };
        let graph_changed = materialized.score != self.score;
        self.redo_stack.pop();
        self.undo_units.push(unit.len());
        self.install(materialized);
        let selection_preserved = self.reresolve_selection();
        Some(EditOutcome {
            graph_changed,
            selection_preserved,
        })
    }

    /// Whether there is an applied edit to [`undo`](Self::undo).
    pub fn can_undo(&self) -> bool {
        !self.undo_units.is_empty()
    }

    /// Whether there is an undone edit to [`redo`](Self::redo).
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Reduces `log` onto `base` and renders it — the materialization every edit, undo,
    /// and redo installs. Errors with [`EditorError::RejectedOperation`] if an op does
    /// not accept or the reduction is not clean (a transaction whose members fail rolls
    /// back as a conflict yet still returns a score), or [`EditorError::NotRenderable`]
    /// for a diagnostic-only layout.
    fn materialize(&self, log: &[OperationEnvelope]) -> Result<Materialization, EditorError> {
        let mut set = OperationSet::new();
        for env in log {
            if !matches!(set.accept(env.clone()), AcceptOutcome::Accepted) {
                return Err(EditorError::RejectedOperation);
            }
        }
        let materialized = set.reduce_onto(&self.base);
        if !materialized.state.is_clean() {
            return Err(EditorError::RejectedOperation);
        }
        let score = materialized.score;
        let (start_clefs, resolved, render, map) =
            render_score(&score, self.solver.as_ref()).ok_or(EditorError::NotRenderable)?;
        Ok(Materialization {
            score,
            start_clefs,
            resolved,
            render,
            map,
        })
    }

    /// Installs a materialization as the session's current score, layout, and render.
    fn install(&mut self, materialized: Materialization) {
        self.score = materialized.score;
        self.start_clefs = materialized.start_clefs;
        self.resolved = materialized.resolved;
        self.render = materialized.render;
        self.map = materialized.map;
    }

    /// Applies a single primitive operation: mints an envelope and commits it. A
    /// [`OperationKind::DeclareTransaction`] is not a primitive mutation — the session
    /// declares transactions via [`Self::apply_transaction`] — so it is refused here.
    ///
    /// Two pre-mint gates run, in order (both refuse with the op log untouched):
    ///
    /// 1. **Edit barriers** (Chapter 8 §"Behavior Under Unknown Extensions"):
    ///    the edit is checked against every active barrier; a match is refused
    ///    ([`EditorError::BarrierProhibited`]). The barrier gate runs *first*
    ///    because its prohibition is the spec's normative MUST and its refusal
    ///    names the unsafe-edit escape ([`Self::apply_unsafe`]); the advisory
    ///    check is the author's local policy.
    /// 2. **Advisory preconditions** (Chapter 6 §"Validation Modes"): the
    ///    session is authoring mode, so advisory preconditions are checked
    ///    against the current materialized score, and any violation refuses the
    ///    edit ([`EditorError::AdvisoryViolation`]). Reduction itself stays pure
    ///    replay mode — it never consults advisory checks.
    pub fn apply(&mut self, kind: OperationKind) -> Result<EditOutcome, EditorError> {
        if matches!(kind, OperationKind::DeclareTransaction(_)) {
            return Err(EditorError::DeclareTransactionNotAllowed);
        }
        if let Some(refusal) = self.barrier_refusal(&kind) {
            return Err(refusal);
        }
        self.apply_past_barriers(kind)
    }

    /// Applies a single primitive operation as an **unsafe edit** (Chapter 8
    /// §"Behavior Under Unknown Extensions"): the explicit user action that
    /// crosses matching edit barriers, acknowledging the loss of the crossed
    /// extensions' data. Everything else about [`Self::apply`] holds — the
    /// advisory gate still runs, and a failed edit changes nothing (including
    /// the tombstone record: an edit that did not land loses no data).
    ///
    /// On success, every crossed extension is recorded in
    /// [`Self::extensions_requiring_tombstone`] (the spec's MUST: its chunks
    /// are tombstoned rather than silently invalidated) and removed from the
    /// active set. An unsafe apply that crosses no barrier is an ordinary
    /// apply.
    pub fn apply_unsafe(&mut self, kind: OperationKind) -> Result<EditOutcome, EditorError> {
        if matches!(kind, OperationKind::DeclareTransaction(_)) {
            return Err(EditorError::DeclareTransactionNotAllowed);
        }
        let crossed = self.crossed_extensions(&kind);
        let outcome = self.apply_past_barriers(kind)?;
        for extension in crossed {
            self.record_unsafe_crossing(extension);
        }
        Ok(outcome)
    }

    /// The shared tail of [`Self::apply`] / [`Self::apply_unsafe`]: everything
    /// past the barrier gate (advisory gate, mint, commit).
    fn apply_past_barriers(&mut self, kind: OperationKind) -> Result<EditOutcome, EditorError> {
        let violations = advisory_violations(&kind, &self.score);
        if !violations.is_empty() {
            return Err(EditorError::AdvisoryViolation { violations });
        }
        // The next id is one past every id ever minted (monotonic across undo), and the
        // context covers the currently-applied ops.
        let counter = self.authored.len() as u64;
        let envelope = self.envelope_at(
            counter,
            OperationPayload::Primitive(kind),
            None,
            self.active_prior_context(),
        );
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
    /// inherited from the private `commit` path.
    pub fn apply_transaction(
        &mut self,
        label: &str,
        category: Option<TransactionCategory>,
        kinds: Vec<OperationKind>,
    ) -> Result<EditOutcome, EditorError> {
        Self::check_transaction_shape(&kinds)?;
        // The barrier gate (see `apply` for the gate ordering): the descriptor
        // the session will mint and every member are each checked against every
        // active barrier; any match refuses the whole transaction before
        // anything is minted.
        if let Some(refusal) = self.transaction_barrier_refusal(&kinds) {
            return Err(refusal);
        }
        self.apply_transaction_past_barriers(label, category, kinds)
    }

    /// [`Self::apply_transaction`] as an **unsafe edit** — the transaction
    /// sibling of [`Self::apply_unsafe`]: matching edit barriers are crossed
    /// rather than refused, and on success every crossed extension is recorded
    /// for tombstoning and deactivated. The structural checks and the advisory
    /// gate still apply, and a transaction that fails to commit records
    /// nothing.
    pub fn apply_transaction_unsafe(
        &mut self,
        label: &str,
        category: Option<TransactionCategory>,
        kinds: Vec<OperationKind>,
    ) -> Result<EditOutcome, EditorError> {
        Self::check_transaction_shape(&kinds)?;
        let mut crossed = self.crossed_extensions(&Self::descriptor_probe());
        for kind in &kinds {
            for extension in self.crossed_extensions(kind) {
                if !crossed.contains(&extension) {
                    crossed.push(extension);
                }
            }
        }
        let outcome = self.apply_transaction_past_barriers(label, category, kinds)?;
        for extension in crossed {
            self.record_unsafe_crossing(extension);
        }
        Ok(outcome)
    }

    /// The structural refusals shared by both transaction entry points.
    fn check_transaction_shape(kinds: &[OperationKind]) -> Result<(), EditorError> {
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
        Ok(())
    }

    /// The first active-barrier match across the transaction the session is
    /// about to mint: the `DeclareTransaction` descriptor (a score-level
    /// operation a score-wide barrier can prohibit), then each member in order.
    fn transaction_barrier_refusal(&self, kinds: &[OperationKind]) -> Option<EditorError> {
        self.barrier_refusal(&Self::descriptor_probe())
            .or_else(|| kinds.iter().find_map(|kind| self.barrier_refusal(kind)))
    }

    /// A stand-in `DeclareTransaction` for gating: barrier matching reads only
    /// the operation *class* (and, for a descriptor, no payload-named object),
    /// so the placeholder id and label never influence the verdict.
    fn descriptor_probe() -> OperationKind {
        OperationKind::DeclareTransaction(TransactionDescriptor {
            id: TransactionId::new(ReplicaId(0), 0),
            label: String::new(),
            category: None,
        })
    }

    /// The shared tail of [`Self::apply_transaction`] /
    /// [`Self::apply_transaction_unsafe`]: everything past the barrier gate.
    fn apply_transaction_past_barriers(
        &mut self,
        label: &str,
        category: Option<TransactionCategory>,
        kinds: Vec<OperationKind>,
    ) -> Result<EditOutcome, EditorError> {
        // Authoring mode (see `apply`): every member must pass its advisory
        // preconditions before anything is minted. Members are checked against
        // the current materialized score — the pre-transaction state — which is
        // conservative for the rare member that only violates against another
        // member's intermediate effect (advisory checks are the author's local
        // policy, not canonical state, so this cannot diverge replicas).
        let violations: Vec<_> = kinds
            .iter()
            .flat_map(|kind| advisory_violations(kind, &self.score))
            .collect();
        if !violations.is_empty() {
            return Err(EditorError::AdvisoryViolation { violations });
        }
        let envelopes = self.transaction_envelopes(label, category, kinds);
        self.commit(envelopes)
    }

    /// Mints the envelopes for an atomic transaction: a `DeclareTransaction`
    /// descriptor at the next counter, then one member envelope per kind at the
    /// following counters, each referencing the transaction id. Each envelope's context
    /// covers the ones before it (the active prefix, then the descriptor, then earlier
    /// members), which gives every member descriptor-precedence over the descriptor for
    /// free. Pure.
    fn transaction_envelopes(
        &self,
        label: &str,
        category: Option<TransactionCategory>,
        kinds: Vec<OperationKind>,
    ) -> Vec<OperationEnvelope> {
        let base = self.authored.len() as u64;
        let tx_id = self.mint_transaction_id();
        let descriptor = TransactionDescriptor {
            id: tx_id,
            label: label.to_string(),
            category,
        };
        let mut envelopes = Vec::with_capacity(kinds.len() + 1);
        // Thread the context: each envelope sees the active prefix plus every earlier
        // envelope in this unit.
        let mut context = self.active_prior_context();
        let descriptor_env = self.envelope_at(
            base,
            OperationPayload::Primitive(OperationKind::DeclareTransaction(descriptor)),
            None,
            context.clone(),
        );
        context = extend_context(context, descriptor_env.id);
        envelopes.push(descriptor_env);
        for (i, kind) in kinds.into_iter().enumerate() {
            let counter = base + 1 + i as u64;
            let member = self.envelope_at(
                counter,
                OperationPayload::Primitive(kind),
                Some(tx_id),
                context.clone(),
            );
            context = extend_context(context, member.id);
            envelopes.push(member);
        }
        envelopes
    }

    /// Mints a fresh [`TransactionId`] in the session's replica namespace, one past the
    /// highest transaction counter declared in this session's **authored** history
    /// (so an undone transaction's id is never reused). Transaction ids live only in the
    /// op stream — the materialized score retains no trace — so the log is the sole
    /// source.
    fn mint_transaction_id(&self) -> TransactionId {
        let next = self
            .authored
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
    /// new identified pitch one diatonic staff step above the event's **rendered**-
    /// highest note, with a fresh id ([`OperationKind::InsertIdentifiedPitch`]). The
    /// selection is unchanged — the anchored notehead is still there. Errors if
    /// nothing — or a non-pitch — is selected, or the event has no CMN note to step
    /// above.
    ///
    /// The "highest note" is ranked by each note's **resolved** spelling (the staff
    /// position the layout draws it at), so an authored respelling — or an inferred
    /// one — is honored rather than refused. The default is deliberately minimal (a
    /// real client picks the inserted pitch); stepping above the top note means
    /// repeated calls build a rising chord rather than stacking on one position.
    pub fn add_note_to_selection(&mut self) -> Result<EditOutcome, EditorError> {
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let TypedObjectId::Pitch(anchor) = selection.source else {
            return Err(EditorError::WrongSelection { expected: "pitch" });
        };
        let (event, _) = self
            .event_and_pitch_of(anchor)
            .ok_or(EditorError::WrongSelection { expected: "pitch" })?;
        // Rank the chord's notes by their *resolved* staff position — the spelling the
        // layout renders, which an authored override or an inferred respelling can move
        // off the raw pitch position — and step one staff step above the rendered-
        // highest note. (Resolved-spelling-aware, so an authored override no longer
        // refuses.) The new note's raw position is the rendered top's `+ 1`: stepping
        // the top's pitch by `rendered_top + 1 - raw_top` carries its alteration and
        // lands its raw position there; the acoustic realization resets so the new note
        // sounds at its written height, not the top's frequency.
        let (top, rendered_top) =
            self.rendered_top_of_event(event)
                .ok_or(EditorError::WrongSelection {
                    expected: "CMN pitch",
                })?;
        let raw_top = diatonic_index(&top).ok_or(EditorError::WrongSelection {
            expected: "CMN pitch",
        })?;
        let value = note_stepped(&top, (rendered_top + 1 - raw_top) as i32).ok_or(
            EditorError::WrongSelection {
                expected: "CMN pitch",
            },
        )?;
        let pitch = IdentifiedPitch {
            id: self.mint_pitch_id(),
            pitch: value,
        };
        self.apply(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp { event, pitch },
        ))
    }

    /// The note in `event` that renders highest, and that rendered staff index — ranked
    /// by each note's **resolved** spelling (from [`derive_annotations`], the same
    /// spellings the layout places noteheads from), so an authored override or an
    /// inferred respelling is accounted for. Falls back to a note's raw pitch position
    /// when it has no resolved CMN spelling. `None` if the event has no CMN note.
    fn rendered_top_of_event(&self, event: EventId) -> Option<(Pitch, i64)> {
        let annotations = derive_annotations(&self.score, &PrePassProfile::default())
            .expect("the default pre-pass algorithms are supported");
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        self.score
            .events
            .get(event)?
            .collect_identified_pitches(&mut buf);
        buf.iter()
            .filter_map(|ip| {
                resolved_staff_index(&annotations, ip.id, &ip.pitch).map(|d| (d, &ip.pitch))
            })
            .max_by_key(|(d, _)| *d)
            .map(|(d, p)| (p.clone(), d))
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

    /// Inserts a note at a world `point` on the beat grid — the **click-to-insert**
    /// ("pencil"): the pitch is the natural pitch at the cursor's height, the position
    /// is the grid-snapped onset, and the written duration is the grid `step`. The note
    /// goes into the staff's primary voice and **makes room** under an overwrite policy
    /// — an existing note/rest the new note fully covers is deleted, one it partially
    /// overlaps is trimmed, and one it lands inside is split (head trimmed, tail
    /// re-inserted), and a **tuplet** any part of the new note overlaps is cascaded away
    /// whole (every member tombstoned, structure removed — tuplets are atomic) — all as
    /// one transaction, so it applies atomically or not at all.
    ///
    /// Errors with [`EditorError::NoInsertTarget`] when the click resolves to no metric
    /// staff/position (or the overlap includes a non-note/rest event there is no
    /// make-room rule for), or [`EditorError::OverlapsTuplet`] when the overlapped member
    /// belongs to a *nested* tuplet, which the flat cascade cannot treat atomically.
    pub fn insert_note_at(
        &mut self,
        point: Point,
        grid: &GridResolution,
    ) -> Result<EditOutcome, EditorError> {
        let pitch = self
            .staff_pitch_at(point)
            .ok_or(EditorError::NoInsertTarget)?;
        let placed = self
            .position_at(point, grid)
            .ok_or(EditorError::NoInsertTarget)?;
        // The staff instance's region (must match the resolved position's) and its
        // primary voice — the new note's home.
        let (region, voice) = self
            .score
            .staff_instances()
            .find(|(_, si)| si.id == pitch.staff_instance)
            .and_then(|(region, si)| {
                let voice = si
                    .voices
                    .iter()
                    .find(|v| v.is_primary)
                    .or_else(|| si.voices.first())?;
                Some((region, voice.id))
            })
            .ok_or(EditorError::NoInsertTarget)?;
        if region != placed.region {
            return Err(EditorError::NoInsertTarget);
        }

        // The new note's half-open span; make room over whatever it overlaps.
        let start = placed.position;
        let duration = grid.step.clone();
        let end = start.clone() + duration.clone();
        let room = self.make_room(voice, &start, &end, None)?;

        let mut minter = self.minter();
        let mut ops = self.make_room_ops(room, pitch.staff_instance, &mut minter);
        let new_note = note_event(
            minter.event(),
            voice,
            start,
            duration,
            vec![IdentifiedPitch {
                id: minter.pitch(),
                pitch: cmn_pitch(pitch.nominal, pitch.octave),
            }],
        );
        ops.push(OperationKind::InsertEvent(InsertEventOp {
            staff_instance: pitch.staff_instance,
            event: new_note,
        }));

        // A bare insert needs no transaction; a make-room insert is atomic.
        if ops.len() == 1 {
            self.apply(ops.into_iter().next().expect("one op"))
        } else {
            self.apply_transaction("insert note", Some(TransactionCategory::NoteEntry), ops)
        }
    }

    /// Sets the selected event's written **duration** (a notation duration-palette
    /// gesture; the selection may be a notehead or a rest/stem). Shrinking just frees
    /// the space after the event; **lengthening makes room** under the overwrite policy
    /// — the events it grows over are trimmed, deleted, or split, and a tuplet it grows
    /// over is cascaded away whole (tuplets are atomic), atomically with the resize.
    /// Errors: [`InvalidDuration`](EditorError::InvalidDuration) for a non-positive
    /// duration, [`OverlapsTuplet`](EditorError::OverlapsTuplet) when the *selected* event
    /// is itself a tuplet member (in-place rescaling of a member is a later refinement) or
    /// it grows over a *nested* tuplet the flat cascade cannot treat atomically, and
    /// [`WrongSelection`](EditorError::WrongSelection) when nothing apt is selected or
    /// the event is not metric.
    pub fn set_selection_duration(
        &mut self,
        duration: MusicalDuration,
    ) -> Result<EditOutcome, EditorError> {
        if !duration.is_positive() {
            return Err(EditorError::InvalidDuration);
        }
        let selection = self.selection.ok_or(EditorError::NoSelection)?;
        let event_id = match selection.source {
            TypedObjectId::Pitch(pitch) => self
                .event_and_pitch_of(pitch)
                .map(|(event, _)| event)
                .ok_or(EditorError::WrongSelection {
                expected: "pitch or event",
            })?,
            TypedObjectId::Event(event) => event,
            _ => {
                return Err(EditorError::WrongSelection {
                    expected: "pitch or event",
                })
            }
        };
        // The event's voice and metric onset. Only a note or rest is resizable here (a
        // non-metric event has no note value; only `Pitched`/`Rest` carry a make-room
        // rule). A tuplet member's duration is ratio-governed, and a decomposed event's
        // notated components would no longer sum to a changed duration, so refuse both.
        let (voice, position) = {
            let event = self
                .score
                .events
                .get(event_id)
                .ok_or(EditorError::WrongSelection {
                    expected: "pitch or event",
                })?;
            if !matches!(event, Event::Pitched(_) | Event::Rest(_)) {
                return Err(EditorError::WrongSelection {
                    expected: "note or rest",
                });
            }
            let (EventPosition::Musical(position), EventDuration::Musical(_)) =
                (event.position().clone(), event.duration())
            else {
                return Err(EditorError::WrongSelection {
                    expected: "metric event",
                });
            };
            (event.voice(), position)
        };
        if self.event_in_tuplet(event_id) {
            return Err(EditorError::OverlapsTuplet);
        }
        if self.event_has_decomposition(event_id) {
            return Err(EditorError::DecomposedEvent);
        }
        // Require a metric region (its time model) — and resolve the staff instance for
        // any re-inserted split tails.
        let staff_instance =
            self.metric_staff_instance_of_voice(voice)
                .ok_or(EditorError::WrongSelection {
                    expected: "metric event",
                })?;

        // Lengthening claims [position, position + duration); make room over the other
        // events there (the resized event itself is excluded). Shrinking frees space, so
        // make-room finds nothing. The resize applies last, after the span is cleared.
        let end = position.clone() + duration.clone();
        let resized = replace_span(
            self.score.events.get(event_id).unwrap(),
            position.clone(),
            duration,
        );
        let room = self.make_room(voice, &position, &end, Some(event_id))?;

        let mut minter = self.minter();
        let mut ops = self.make_room_ops(room, staff_instance, &mut minter);
        ops.push(OperationKind::ModifyEvent(ModifyEventOp { event: resized }));
        if ops.len() == 1 {
            self.apply(ops.into_iter().next().expect("one op"))
        } else {
            self.apply_transaction("set duration", Some(TransactionCategory::NoteEntry), ops)
        }
    }

    /// The events make-room must change to clear `[start, end)` in `voice` (other than
    /// `exclude`, the event being inserted/resized): whole-event deletes, in-place
    /// trims, splits, and whole-tuplet cascade deletes. A tuplet is **atomic** — an
    /// overlap with any of its members removes the whole tuplet (every member, plus the
    /// structure), since a member's duration is ratio-bound and cannot be trimmed in
    /// place. Errors with [`OverlapsTuplet`](EditorError::OverlapsTuplet) for a *nested*
    /// tuplet (cascading one level only is not yet safe), [`NoInsertTarget`] for a
    /// non-note/rest overlap, and [`DecomposedEvent`](EditorError::DecomposedEvent) for a
    /// trim/split of a (non-tuplet) decomposed event.
    fn make_room(
        &self,
        voice: VoiceId,
        start: &MusicalPosition,
        end: &MusicalPosition,
        exclude: Option<EventId>,
    ) -> Result<MakeRoom, EditorError> {
        let mut room = MakeRoom::default();
        let mut cascade_tuplets: std::collections::BTreeSet<TupletId> =
            std::collections::BTreeSet::new();
        for event in self.score.events.iter() {
            if event.voice() != voice || Some(event.id()) == exclude {
                continue;
            }
            let (EventPosition::Musical(ep), EventDuration::Musical(ed)) =
                (event.position(), event.duration())
            else {
                continue;
            };
            // A non-positive existing span never occupies the range (matching the
            // reducer's overlap rule), so it is not in the way.
            if !ed.is_positive() {
                continue;
            }
            let event_end = ep.clone() + ed.clone();
            if !(ep < end && start < &event_end) {
                continue; // disjoint from [start, end)
            }
            // A tuplet member: mark its (flat) tuplet for whole-tuplet cascade deletion,
            // and stop treating it as an ordinary overlap. A nested tuplet is refused.
            let containing = self.tuplets_containing(event.id());
            if !containing.is_empty() {
                for tuplet in containing {
                    if !self.is_flat_tuplet(tuplet) {
                        return Err(EditorError::OverlapsTuplet);
                    }
                    cascade_tuplets.insert(tuplet);
                }
                continue;
            }
            if !matches!(event, Event::Pitched(_) | Event::Rest(_)) {
                return Err(EditorError::NoInsertTarget); // no make-room rule for this kind
            }
            // A trim or split changes the event's duration; refuse if a persistent
            // decomposition would no longer sum to it (a full-cover delete is fine — the
            // tombstoned target's decomposition is no longer checked).
            let fully_covered = ep >= start && &event_end <= end;
            if !fully_covered && self.event_has_decomposition(event.id()) {
                return Err(EditorError::DecomposedEvent);
            }
            match (ep < start, &event_end > end) {
                // Fully covered → delete.
                (false, false) => room.deletes.push(event.id()),
                // The span lands inside it → split: trim to the head, re-insert the tail.
                (true, true) => {
                    room.trims
                        .push(replace_span(event, ep.clone(), span_between(ep, start)));
                    room.tails
                        .push((event.clone(), end.clone(), span_between(end, &event_end)));
                }
                // Overlaps the head (event ends inside the span) → trim the tail off.
                (true, false) => {
                    room.trims
                        .push(replace_span(event, ep.clone(), span_between(ep, start)))
                }
                // Overlaps the tail (event starts inside the span) → trim the head off.
                (false, true) => room.trims.push(replace_span(
                    event,
                    end.clone(),
                    span_between(end, &event_end),
                )),
            }
        }
        // Expand each cascade tuplet into deletes of all its live members. The first
        // member removes the tuplet structure (so the rest no longer belong to a tuplet
        // and delete as ordinary events); the order is preserved by `make_room_ops`.
        for tuplet in cascade_tuplets {
            for (i, member) in self.tuplet_members(tuplet).into_iter().enumerate() {
                let compensation = if i == 0 {
                    TupletCompensation::CascadeDeleteTuplets {
                        tuplets: vec![tuplet],
                    }
                } else {
                    TupletCompensation::NotInTuplet
                };
                room.cascade_deletes.push((member, compensation));
            }
        }
        Ok(room)
    }

    /// The ids of the tuplets `event` is a member of.
    fn tuplets_containing(&self, event: EventId) -> Vec<TupletId> {
        self.score
            .cross_cutting
            .tuplets
            .iter()
            .filter(|tuplet| tuplet.members.contains(&event))
            .map(|tuplet| tuplet.id)
            .collect()
    }

    /// Whether `tuplet` is flat — not nested inside another and not the parent of one.
    /// Cascade-deleting a nested tuplet would leave the parent referencing tombstoned
    /// members, so make-room refuses it for now.
    fn is_flat_tuplet(&self, tuplet: TupletId) -> bool {
        let tuplets = &self.score.cross_cutting.tuplets;
        tuplets
            .iter()
            .find(|t| t.id == tuplet)
            .is_some_and(|t| t.parent.is_none())
            && !tuplets.iter().any(|t| t.parent == Some(tuplet))
    }

    /// `tuplet`'s live member events, in member order.
    fn tuplet_members(&self, tuplet: TupletId) -> Vec<EventId> {
        self.score
            .cross_cutting
            .tuplets
            .iter()
            .find(|t| t.id == tuplet)
            .map(|t| {
                t.members
                    .iter()
                    .copied()
                    .filter(|m| self.score.events.get(*m).is_some())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Turns a [`MakeRoom`] into operations: trims (`ModifyEvent`), deletes
    /// (`DeleteEvent`), and split tails (`InsertEvent`, cloning the original event's
    /// shape with fresh ids, carrying any authored spelling via `RespellPitch`). Order
    /// frees the span before the caller appends its own op (the new note or the resize).
    fn make_room_ops(
        &self,
        room: MakeRoom,
        staff_instance: StaffInstanceId,
        minter: &mut Minter,
    ) -> Vec<OperationKind> {
        let mut ops: Vec<OperationKind> = Vec::new();
        // Cascade-delete whole tuplets first, in order — the structure-removing delete
        // must precede the members that then delete as ordinary (no-longer-tuplet) events.
        for (event, tuplet_compensation) in room.cascade_deletes {
            ops.push(OperationKind::DeleteEvent(DeleteEventOp {
                event,
                tuplet_compensation,
            }));
        }
        for event in room.trims {
            ops.push(OperationKind::ModifyEvent(ModifyEventOp { event }));
        }
        for event in room.deletes {
            ops.push(OperationKind::DeleteEvent(DeleteEventOp {
                event,
                tuplet_compensation: TupletCompensation::NotInTuplet,
            }));
        }
        for (original, position, duration) in room.tails {
            // The tail is the original event's later portion: clone its whole shape (a
            // rest's visibility, a note's articulations/dynamics/stem/grace) with fresh
            // ids, then carry any *authored* spelling onto the fresh pitches (an inferred
            // one re-derives) — the same atomic copy `insert_note_after_selection` does.
            let mut pitches: Vec<&IdentifiedPitch> = Vec::new();
            original.collect_identified_pitches(&mut pitches);
            let originals: Vec<PitchId> = pitches.iter().map(|ip| ip.id).collect();
            let fresh: Vec<PitchId> = originals.iter().map(|_| minter.pitch()).collect();
            let tail = respan_with_fresh_ids(&original, minter.event(), position, duration, &fresh);
            ops.push(OperationKind::InsertEvent(InsertEventOp {
                staff_instance,
                event: tail,
            }));
            for (old, new) in originals.iter().zip(&fresh) {
                if let Some(spelling) = authored_spelling(&self.score, *old) {
                    ops.push(OperationKind::RespellPitch(RespellPitchOp {
                        pitch: *new,
                        spelling,
                    }));
                }
            }
        }
        ops
    }

    /// A fresh-id [`Minter`] seeded from the session high-water marks (which do not move
    /// until commit), so several mints within one intent never collide.
    fn minter(&self) -> Minter {
        Minter {
            replica: self.replica,
            next_event: self.mint_event_id().counter(),
            next_pitch: self.mint_pitch_id().counter(),
        }
    }

    /// Whether `event` is a member of any tuplet (its duration is ratio-governed, so
    /// make-room cannot trim or delete it without tuplet compensation).
    fn event_in_tuplet(&self, event: EventId) -> bool {
        self.score
            .cross_cutting
            .tuplets
            .iter()
            .any(|tuplet| tuplet.members.contains(&event))
    }

    /// Whether `event` carries a persistent decomposition attachment — its notated
    /// components, which a duration change would no longer sum to (invariant 15), so the
    /// editor cannot resize/trim it until decomposition-edit ops exist.
    fn event_has_decomposition(&self, event: EventId) -> bool {
        self.score
            .decomposition_attachments
            .iter()
            .any(|attachment| attachment.target == event)
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
    /// `base`, the current score (each live or tombstoned), and this session's
    /// **authored** history (so an id from an undone insert is never reused).
    fn mint_event_id(&self) -> EventId {
        let ids = self
            .base
            .events
            .iter()
            .map(Event::id)
            .chain(self.base.tombstoned_events.iter().copied())
            .chain(self.score.events.iter().map(Event::id))
            .chain(self.score.tombstoned_events.iter().copied())
            .chain(self.authored.iter().flat_map(inserted_event_ids));
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
    /// future reducer change records), and this session's **authored** history (catches
    /// a session-inserted pitch since deleted *or undone*). Reusing an id would make a
    /// later insert no-op against a tombstone under whole-log reduction. Pitches authored
    /// by other replicas occupy disjoint namespaces and do not constrain it.
    fn mint_pitch_id(&self) -> PitchId {
        let ids = self
            .base
            .live_pitch_ids()
            .into_iter()
            .chain(self.base.tombstoned_pitches.iter().copied())
            .chain(self.score.live_pitch_ids())
            .chain(self.score.tombstoned_pitches.iter().copied())
            .chain(self.authored.iter().flat_map(inserted_pitch_ids));
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

/// A 5-line staff spans four staff spaces above its bottom line.
const STAFF_SPAN: f32 = 4.0;

/// The distance from height `y` to a staff's line band `(bottom, top)`: zero inside
/// the band, else the gap to the nearer edge. Used to pick the staff a click is over.
fn dist_to_band(y: f32, (bottom, top): (f32, f32)) -> f32 {
    if y < bottom {
        bottom - y
    } else if y > top {
        y - top
    } else {
        0.0
    }
}

/// Whether a resolved bounding box carries **real** cast-off geometry: finite
/// origin and strictly positive extent on both axes. A solver that does not cast
/// off (the stub) emits `Rect::default()` — zero-size — boxes, which must not
/// capture clicks; the callers fall back to the flat single-system paths instead.
fn rect_is_real(rect: &Rect) -> bool {
    let width = rect.size.width.0;
    let height = rect.size.height.0;
    rect.origin.x.0.is_finite()
        && rect.origin.y.0.is_finite()
        && width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0
}

/// A rect's vertical band as `(bottom, top)`, the shape [`dist_to_band`] takes.
fn rect_y_band(rect: &Rect) -> (f32, f32) {
    (rect.origin.y.0, rect.origin.y.0 + rect.size.height.0)
}

/// Whether `point` lies within `rect`, edges included (a glyph exactly on a
/// system's edge belongs to that system).
fn rect_contains(rect: &Rect, point: Point) -> bool {
    point.x.0 >= rect.origin.x.0
        && point.x.0 <= rect.origin.x.0 + rect.size.width.0
        && point.y.0 >= rect.origin.y.0
        && point.y.0 <= rect.origin.y.0 + rect.size.height.0
}

/// Inverts an `x` coordinate to a raw musical position through `(onset, x)` anchors
/// in ascending order (`>= 2`, leftmost first) — the horizontal inverse before grid
/// snapping. Within the anchored span it interpolates the bracketing segment; outside
/// it, it extrapolates the nearest end segment's slope (the common case — a click in
/// the empty staff after the last note). `f64` because this is geometry, not exact
/// musical time; the result is snapped to an exact grid position by [`snap_to_grid`].
fn invert_x(anchors: &[(MusicalPosition, f32)], x: f32) -> f64 {
    let n = anchors.len();
    debug_assert!(n >= 2, "invert_x needs at least two anchors for a scale");
    let pos = |i: usize| anchors[i].0 .0.to_f64();
    let ax = |i: usize| anchors[i].1 as f64;
    // The segment to (inter/extra)polate on: the one bracketing `x`, clamped to the
    // first/last segment when `x` is left of / right of every anchor.
    let seg = if x <= anchors[0].1 {
        0
    } else if x >= anchors[n - 1].1 {
        n - 2
    } else {
        (0..n - 1)
            .find(|&i| x >= anchors[i].1 && x <= anchors[i + 1].1)
            .unwrap_or(n - 2)
    };
    let (x0, x1) = (ax(seg), ax(seg + 1));
    let (p0, p1) = (pos(seg), pos(seg + 1));
    let span = x1 - x0;
    if span.abs() < f64::EPSILON {
        return p0;
    }
    p0 + (x as f64 - x0) / span * (p1 - p0)
}

/// Snaps a raw musical position to the nearest multiple of `step` from the origin,
/// clamped to be non-negative (no position precedes the region start). The multiple
/// is taken in `f64`, then the position is rebuilt by exact rational arithmetic
/// (`step * k`), so the result lands exactly on the grid, not on a rounded float.
fn snap_to_grid(raw: f64, step: &MusicalDuration) -> MusicalPosition {
    let step_f = step.0.to_f64();
    let k = if step_f > 0.0 {
        (raw / step_f).round().clamp(0.0, i32::MAX as f64) as i32
    } else {
        0
    };
    MusicalPosition(step.0.mul(&RationalTime::from_int(k)))
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

/// The diatonic staff index of a resolved CMN spelling — `octave * 7 + nominal`, on
/// the same scale as [`diatonic_index`], so a note's resolved (rendered) staff position
/// and a raw pitch position are directly comparable. `None` for a non-CMN nominal.
fn spelling_diatonic(spelling: &PitchSpelling) -> Option<i64> {
    match spelling.nominal {
        SpellingNominal::Cmn(nominal) => Some(spelling.octave as i64 * 7 + nominal as i64),
        _ => None,
    }
}

/// The **rendered** staff index of `pitch` (id `id`): from its resolved spelling when
/// that is CMN — so an authored override or an inferred respelling is reflected —
/// otherwise its raw pitch position.
fn resolved_staff_index(
    annotations: &DerivedAnnotations,
    id: PitchId,
    pitch: &Pitch,
) -> Option<i64> {
    annotations
        .spellings
        .get(&id)
        .and_then(|resolved| spelling_diatonic(&resolved.spelling))
        .or_else(|| diatonic_index(pitch))
}

/// `top` moved `steps` diatonic staff steps (preserving its alteration), with the
/// acoustic realization reset to [`AcousticRealization::Implicit`]. A fresh note must
/// sound at its written position; cloning an explicit absolute-Hz or cents-offset
/// realization from the note it stacks above would make it look higher but sound the
/// same frequency. `None` for a non-CMN position.
fn note_stepped(top: &Pitch, steps: i32) -> Option<Pitch> {
    let mut moved = staff_step(top, steps)?;
    moved.acoustic.realization = AcousticRealization::Implicit;
    Some(moved)
}

/// A natural CMN pitch at `nominal`/`octave`, sounding at its written position
/// ([`AcousticRealization::Implicit`]) — the value a click-to-insert mints for the
/// height under the cursor (a caller respells if an accidental is wanted).
fn cmn_pitch(nominal: CmnNominal, octave: i8) -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Cmn {
                nominal,
                alteration: 0,
                octave,
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

/// The musical duration spanning `from..to` (`to - from`), exact over rational time.
fn span_between(from: &MusicalPosition, to: &MusicalPosition) -> MusicalDuration {
    MusicalDuration(to.0.sub(&from.0))
}

/// A time signature's beat unit — the note value it counts in, `1/denominator` (4/4 → a
/// quarter, 6/8 → an eighth). `None` for a meter with no single denominator
/// (mixed-denominator, none, or a symbolic display), which the caller defaults.
fn time_signature_beat(ts: &TimeSignature) -> Option<MusicalDuration> {
    let denominator: i64 = match &ts.display {
        TimeSignatureDisplay::Standard { denominator, .. }
        | TimeSignatureDisplay::Compound { denominator, .. } => denominator.get() as i64,
        TimeSignatureDisplay::Irrational { denominator, .. } => denominator.get() as i64,
        TimeSignatureDisplay::MixedDenominators { .. }
        | TimeSignatureDisplay::None
        | TimeSignatureDisplay::Symbolic(_) => return None,
    };
    RationalTime::new(1, denominator).map(MusicalDuration)
}

/// The events make-room must change to clear a span: whole-event deletes, in-place
/// trims (a `ModifyEvent` value), and splits (the original event plus the tail's onset
/// and duration, for re-inserting the tail with fresh ids). Built by
/// [`EditorSession::make_room`], turned into ops by [`EditorSession::make_room_ops`].
#[derive(Default)]
struct MakeRoom {
    trims: Vec<Event>,
    deletes: Vec<EventId>,
    tails: Vec<(Event, MusicalPosition, MusicalDuration)>,
    /// Whole-tuplet cascade deletes: every member of each overlapped tuplet, paired with
    /// its delete compensation. The first member of a tuplet carries
    /// [`TupletCompensation::CascadeDeleteTuplets`] (which removes the tuplet structure);
    /// the rest are then ordinary [`NotInTuplet`](TupletCompensation::NotInTuplet)
    /// deletes, so they must apply in this order.
    cascade_deletes: Vec<(EventId, TupletCompensation)>,
}

/// Mints fresh event/pitch ids within one intent, advancing local counters (checked,
/// like the session minters) so the several mints in one transaction never collide —
/// the session high-water mark does not move until commit. Seeded by
/// [`EditorSession::minter`].
struct Minter {
    replica: ReplicaId,
    next_event: u64,
    next_pitch: u64,
}

impl Minter {
    fn event(&mut self) -> EventId {
        let id = EventId::new(self.replica, self.next_event);
        self.next_event = self
            .next_event
            .checked_add(1)
            .expect("event id counter overflowed u64");
        id
    }

    fn pitch(&mut self) -> PitchId {
        let id = PitchId::new(self.replica, self.next_pitch);
        self.next_pitch = self
            .next_pitch
            .checked_add(1)
            .expect("pitch id counter overflowed u64");
        id
    }
}

/// `event` re-placed at a new metric `position`/`duration` (a make-room trim), keeping
/// everything else. Only notes and rests carry a make-room rule; other kinds return
/// unchanged (the caller refuses such an overlap before reaching here).
fn replace_span(event: &Event, position: MusicalPosition, duration: MusicalDuration) -> Event {
    let mut moved = event.clone();
    match &mut moved {
        Event::Pitched(pe) => {
            pe.position = EventPosition::Musical(position);
            pe.duration = EventDuration::Musical(duration);
        }
        Event::Rest(rest) => {
            rest.position = EventPosition::Musical(position);
            rest.duration = EventDuration::Musical(duration);
        }
        _ => {}
    }
    moved
}

/// `event`'s later portion as a fresh event for a split tail: a clone with a fresh
/// event `id`, its pitches re-identified by `fresh_pitch_ids` (same values, in order),
/// re-placed at `position`/`duration`. Cloning keeps everything a continuation should
/// keep — a rest's `visible`/`vertical_position`, a note's articulations, dynamics,
/// ornaments, stem, and grace. Authored spellings are carried separately (they key on
/// pitch id, so the caller adds `RespellPitch`es for the fresh ids).
fn respan_with_fresh_ids(
    event: &Event,
    id: EventId,
    position: MusicalPosition,
    duration: MusicalDuration,
    fresh_pitch_ids: &[PitchId],
) -> Event {
    let mut tail = event.clone();
    let position = EventPosition::Musical(position);
    let duration = EventDuration::Musical(duration);
    match &mut tail {
        Event::Pitched(pe) => {
            pe.id = id;
            pe.position = position;
            pe.duration = duration;
            for (ip, fresh) in pe.pitches.iter_mut().zip(fresh_pitch_ids) {
                ip.id = *fresh;
            }
        }
        Event::Rest(rest) => {
            rest.id = id;
            rest.position = position;
            rest.duration = duration;
        }
        _ => {}
    }
    tail
}

/// A fresh metric event in `voice`: a [`PitchedEvent`] when `pitches` is non-empty,
/// else a [`Rest`](epiphany_core::Rest) — used for the new note and any split tail.
fn note_event(
    id: EventId,
    voice: VoiceId,
    position: MusicalPosition,
    duration: MusicalDuration,
    pitches: Vec<IdentifiedPitch>,
) -> Event {
    let position = EventPosition::Musical(position);
    let duration = EventDuration::Musical(duration);
    if pitches.is_empty() {
        Event::Rest(epiphany_core::Rest {
            id,
            voice,
            position,
            duration,
            vertical_position: None,
            visible: true,
        })
    } else {
        Event::Pitched(PitchedEvent {
            id,
            voice,
            position,
            duration,
            pitches,
            articulations: Vec::new(),
            dynamic: None,
            ornaments: Vec::new(),
            stem: StemConfiguration,
            grace: None,
        })
    }
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

/// Extends a causal context to also cover `op`. When `op` continues `op.replica`'s
/// contiguous range (the common case — a session with no undo mints `0, 1, 2, …`), it
/// folds into the compact version vector, reproducing the simple `with_seen(replica,
/// counter)` history; once a fork (undo + new edit) leaves a gap below `op.counter`, it
/// is recorded as an individual dot so the context covers the active op without
/// asserting — and stranding the new op pending behind — the forked-away counters in
/// between.
fn extend_context(context: CausalContext, op: OperationId) -> CausalContext {
    let continues = context
        .vector
        .get(&op.replica)
        .map_or(op.counter == 0, |&high| op.counter == high + 1);
    if continues {
        context.with_seen(op.replica, op.counter)
    } else {
        context.with_dot(op)
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
type StartClefs = BTreeMap<(RegionId, StaffId), Clef>;

fn render_score(
    score: &Score,
    solver: &dyn ConstraintSolver,
) -> Option<(StartClefs, ResolvedLayoutIR, RenderIR, HitTestMap)> {
    // Build the start-clef table from the logical layout, where anchor resolution has
    // already placed each `PlacedClef` at a concrete time — the editor's vertical
    // inverse spells the clicked height against this, not against vector order.
    let logical = to_logical(score);
    let start_clefs = staff_start_clefs(&logical);
    let report = solver.solve(&to_constrained(&logical), &SolverConfig::default());
    if !report.status.is_renderable() {
        return None;
    }
    let render = to_render(&report.layout);
    let map = render.hit_test_map();
    Some((start_clefs, report.layout, render, map))
}

/// The clef in force at each manifested staff's start, resolved by time from the
/// logical layout's placed clefs (see [`active_clef`]). Keyed by the manifesting
/// `(region, staff)` so tiled copies of one staff each get their own entry — the
/// vertical half of click-to-insert looks a staff up by the region its rendered
/// lines were traced to.
fn staff_start_clefs(logical: &LogicalLayoutIR) -> StartClefs {
    let start = TimePoint::Musical(MusicalPosition::origin());
    let mut clefs = StartClefs::new();
    for region in &logical.regions {
        let TypedObjectId::Region(region_id) = region.provenance.source else {
            continue;
        };
        for object in &region.objects {
            if let (Some(staff), LayoutContent::Staff(content)) = (object.staff(), object.content())
            {
                clefs.insert((region_id, staff), active_clef(&content.clefs, &start));
            }
        }
    }
    clefs
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::{valid_score, valid_score_rich};
    use epiphany_layout_ir::{
        ConstrainedLayoutIR, HitShape, InvalidationSet, SolveReport, SolveStatus, SolverState,
        SolverTier, SolverVersion, StubSolver,
    };

    fn open_rich(seed: u64) -> EditorSession {
        EditorSession::open(valid_score_rich(seed), Box::new(StubSolver)).expect("rich renders")
    }

    /// A session on the plain fixture, whose single metric region is a tuplet-free run
    /// of quarter notes — the clean target for the make-room tests (the rich fixture's
    /// only metric region is a triplet).
    fn open_plain(seed: u64) -> EditorSession {
        EditorSession::open(valid_score(seed), Box::new(StubSolver)).expect("plain renders")
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
                HitShape::Segment { .. } | HitShape::Curve { .. } => None,
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

    /// Clicking a slur curve selects the slur (schema-major-2 E2). A slur draws
    /// a cubic-bézier `Curve` primitive whose hit region flows through
    /// `click()` generically — no slur-specific arm — so the selection resolves
    /// to `TypedObjectId::Slur`. A click on the *arc* (its `t = 0.5` apex, not
    /// its chord) hits it, proving the flattened-capsule hit shape.
    #[test]
    fn clicking_a_slur_curve_selects_the_slur() {
        use epiphany_core::{Slur, SlurId, SlurKind, SpanStyle};

        let mut score = valid_score_rich(0x5EED);
        // Two events of region A's first voice, on its one staff.
        let events: Vec<_> = score.canvas.regions[0].staff_instances()[0].voices[0]
            .events
            .clone();
        let slur_id: SlurId = score.identity.mint();
        score.cross_cutting.slurs.push(Slur {
            id: slur_id,
            start_event: events[0],
            end_event: events[2],
            kind: SlurKind::Legato,
            curvature_override: None,
            style: SpanStyle::default(),
        });
        let mut session = EditorSession::open(score, Box::new(StubSolver)).expect("renders");

        // The slur's curve region, and the arc's apex (cubic at t = 0.5).
        let apex = session
            .hit_test()
            .regions
            .iter()
            .find_map(|r| match r.shape {
                HitShape::Curve { p0, p1, p2, p3, .. }
                    if r.source == TypedObjectId::Slur(slur_id) =>
                {
                    let mid = |a: f32, b: f32, c: f32, d: f32| (a + 3.0 * b + 3.0 * c + d) / 8.0;
                    Some(Point::new(
                        mid(p0.x.0, p1.x.0, p2.x.0, p3.x.0),
                        mid(p0.y.0, p1.y.0, p2.y.0, p3.y.0),
                    ))
                }
                _ => None,
            })
            .expect("the slur draws a curve region");

        let selection = session.click(apex).expect("the click selects the slur");
        assert_eq!(selection.source, TypedObjectId::Slur(slur_id));
        // A slur is not an editable target: an edit op cleanly refuses it
        // rather than mishandling the non-pitch selection.
        assert!(session.transpose_selection(1).is_err());
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
    fn staff_pitch_at_reads_the_clicked_height() {
        let session = open_rich(0x5EED);
        // The lowest staff-line stroke is the bottom staff's step origin.
        let origin = session
            .resolved()
            .strokes
            .iter()
            .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
            .map(|s| s.from.y.0)
            .fold(f32::INFINITY, f32::min);
        assert!(origin.is_finite(), "the score renders staff lines");

        let diatonic = |p: StaffPitch| p.octave as i32 * 7 + p.nominal as i32;
        let at = |dy: f32| session.staff_pitch_at(Point::new(5.0, origin + dy));

        let bottom = at(0.0).expect("a staff under the click");
        // Two staff spaces up is four diatonic steps (a fifth) higher, on the same
        // staff — so y maps to staff steps at two steps per space.
        let up_a_fifth = at(2.0).expect("still over the staff");
        assert_eq!(up_a_fifth.staff_instance, bottom.staff_instance);
        assert_eq!(diatonic(up_a_fifth) - diatonic(bottom), 4);
        // A half-space below the bottom line is one diatonic step down.
        let below = at(-0.5).expect("just below the staff still resolves");
        assert_eq!(diatonic(bottom) - diatonic(below), 1);
    }

    #[test]
    fn staff_pitch_at_picks_the_staff_under_the_x() {
        let session = open_rich(0x5EED);
        // Each staff manifestation's bottom line: its step-origin y and its x span.
        let mut bands: Vec<(StaffInstanceId, f32, f32, f32)> = session
            .score()
            .staff_instances()
            .filter_map(|(region, si)| {
                let m = manifestation_layout_id(&TypedObjectId::Staff(si.staff), region);
                session
                    .resolved()
                    .strokes
                    .iter()
                    .find(|s| s.provenance.stable_id == m)
                    .map(|s| {
                        (
                            si.id,
                            s.from.x.0.min(s.to.x.0),
                            s.from.x.0.max(s.to.x.0),
                            s.from.y.0,
                        )
                    })
            })
            .collect();
        bands.sort_by(|a, b| a.1.total_cmp(&b.1));
        // The fixture tiles its staves left-to-right sharing one y band — so height
        // alone cannot tell them apart; only x identifies the clicked staff.
        assert!(bands.len() >= 2, "the fixture manifests multiple staves");
        let origin = bands[0].3;
        assert!(
            bands.iter().all(|b| (b.3 - origin).abs() < 1e-3),
            "the staves share a y band, so the click is ambiguous by height alone"
        );
        let mut resolved = std::collections::BTreeSet::new();
        for &(expected, lo, hi, _) in &bands {
            // Click the centre of this staff's x span, at the shared band height.
            let pitch = session
                .staff_pitch_at(Point::new((lo + hi) / 2.0, origin + 2.0))
                .expect("a staff under the click");
            assert_eq!(
                pitch.staff_instance, expected,
                "the click resolves to the staff occupying that x, not a height-only pick"
            );
            resolved.insert(pitch.staff_instance);
        }
        assert_eq!(
            resolved.len(),
            bands.len(),
            "distinct x positions resolved to distinct staves (height alone could not)"
        );
    }

    #[test]
    fn staff_pitch_at_rejects_non_finite_clicks() {
        let session = open_rich(0x5EED);
        // A finite click resolves, so the fixture has a staff to hit.
        assert!(session.staff_pitch_at(Point::new(5.0, 0.0)).is_some());
        // A malformed view transform can hand a NaN/inf coordinate; it must yield no
        // pitch, not a bogus one (NaN would slip through `dist_to_band` as distance 0
        // and saturate the `round() as i32` step).
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(session.staff_pitch_at(Point::new(bad, 0.0)), None);
            assert_eq!(session.staff_pitch_at(Point::new(5.0, bad)), None);
        }
    }

    #[test]
    fn staff_start_clefs_resolve_by_time_not_vector_order() {
        use epiphany_core::{MusicalPosition, RationalTime};
        use epiphany_layout_ir::{LayoutObject, PlacedClef};

        let at = |n, d| TimePoint::Musical(MusicalPosition(RationalTime::new(n, d).unwrap()));
        let mut logical = to_logical(&valid_score_rich(0x5EED));
        // Author one staff's clefs out of order: bass at time 1, treble at the origin.
        // The clef in force at the staff start is treble (the latest change at or
        // before time 0), even though bass is first in the vector.
        let mut patched = None;
        'outer: for region in &mut logical.regions {
            let TypedObjectId::Region(rid) = region.provenance.source else {
                continue;
            };
            for object in &mut region.objects {
                // Match on content, not variant: the staff-content object projects from
                // a `StaffInstance` source, so it is not the `Staff` layout variant.
                let LayoutContent::Staff(content) = object.content() else {
                    continue;
                };
                let Some(staff) = object.staff() else {
                    continue;
                };
                let mut content = content.clone();
                content.clefs = vec![
                    PlacedClef {
                        time: at(1, 1),
                        clef: Clef::bass(),
                    },
                    PlacedClef {
                        time: at(0, 1),
                        clef: Clef::treble(),
                    },
                ];
                *object = LayoutObject::from_projection_with_content(
                    object.provenance().clone(),
                    Some(staff),
                    LayoutContent::Staff(content),
                );
                patched = Some((rid, staff));
                break 'outer;
            }
        }
        let key = patched.expect("the fixture has a staff layout object");
        let clefs = staff_start_clefs(&logical);
        assert_eq!(
            clefs.get(&key).copied(),
            Some(Clef::treble()),
            "the start clef resolves by time (treble@0), not vector order (bass first)"
        );
    }

    /// The first region whose events have `time_model`-matching positions, picked by
    /// the `metric` flag (a `Musical` vs `WallClock` onset).
    fn a_region_with(session: &EditorSession, metric: bool) -> RegionId {
        session
            .score()
            .voices()
            .find_map(|(rid, _, v)| {
                v.events.iter().find_map(|eid| {
                    let ev = session.score().events.get(*eid)?;
                    (matches!(ev.position(), EventPosition::Musical(_)) == metric).then_some(rid)
                })
            })
            .expect("a region with the requested time model")
    }

    /// `region`'s pitched metric events as `(onset, first pitch id)`, in onset order.
    fn region_pitched_events(
        session: &EditorSession,
        region: RegionId,
    ) -> Vec<(MusicalPosition, PitchId)> {
        let mut out = Vec::new();
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        for (rid, _, v) in session.score().voices() {
            if rid != region {
                continue;
            }
            for eid in &v.events {
                let Some(ev) = session.score().events.get(*eid) else {
                    continue;
                };
                let EventPosition::Musical(at) = ev.position() else {
                    continue;
                };
                buf.clear();
                ev.collect_identified_pitches(&mut buf);
                if let Some(ip) = buf.first() {
                    out.push((at.clone(), ip.id));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// `region`'s rendered bottom staff line as `(left_x, right_x, origin_y)`.
    ///
    /// **Flat-layout (stub) helper**: it finds the stroke carrying the staff's
    /// manifestation id, which in a cast-off layout is only the *first* system's
    /// segment. Every test here runs on the [`StubSolver`], which never splits a
    /// line, so the first segment is the whole line; multi-system geometry is
    /// exercised via [`install_two_system_geometry`] and, over the real engraver,
    /// by the testkit's `multisystem_click` integration test.
    fn region_staff_line(session: &EditorSession, region: RegionId) -> (f32, f32, f32) {
        let (_, si) = session
            .score()
            .staff_instances()
            .find(|(r, _)| *r == region)
            .expect("the region has a staff instance");
        let m = manifestation_layout_id(&TypedObjectId::Staff(si.staff), region);
        let line = session
            .resolved()
            .strokes
            .iter()
            .find(|s| s.provenance.stable_id == m)
            .expect("the staff renders its bottom line");
        (
            line.from.x.0.min(line.to.x.0),
            line.from.x.0.max(line.to.x.0),
            line.from.y.0,
        )
    }

    /// A world point on `region`'s staff: the midpoint of its bottom line, one staff
    /// space up (inside the staff, so the click resolves to this region).
    fn point_on_region_staff(session: &EditorSession, region: RegionId) -> Point {
        let (left, right, origin_y) = region_staff_line(session, region);
        Point::new((left + right) / 2.0, origin_y + 1.0)
    }

    #[test]
    fn position_at_snaps_a_click_to_the_beat_grid() {
        let session = open_rich(0x5EED);
        let region = a_region_with(&session, true);
        let anchors = session.position_anchors(region, None);
        assert!(
            anchors.len() >= 2,
            "the metric region renders multiple notes"
        );
        let (_, right, origin_y) = region_staff_line(&session, region);
        let y = origin_y + 1.0;

        // Grid = the first inter-onset gap, so every rendered onset is a grid multiple.
        let step = MusicalDuration(anchors[1].0 .0.sub(&anchors[0].0 .0));
        assert!(step.is_positive());
        let grid = GridResolution { step };

        // Clicking each notehead snaps to that note's onset.
        for (onset, x) in &anchors {
            let gp = session
                .position_at(Point::new(*x, y), &grid)
                .expect("a metric region under the click");
            assert_eq!(gp.region, region);
            assert_eq!(&gp.position, onset, "the click snaps to the note's onset");
        }

        // Past the last note but still over this staff (between the last notehead and
        // the staff's right edge — the empty space a make-room insert targets), the
        // inverse extrapolates to a later grid slot, strictly after the last onset.
        let (last_onset, last_x) = anchors.last().cloned().unwrap();
        let reach = (last_x + right) / 2.0;
        assert!(reach > last_x, "the staff extends past the last note");
        let far = session
            .position_at(Point::new(reach, y), &grid)
            .expect("empty space past the notes still resolves");
        assert!(
            far.position > last_onset,
            "a click past the last note lands later on the grid"
        );

        // Left of the first note clamps to the region origin (no negative musical time).
        let before = session
            .position_at(Point::new(anchors[0].1 - 1000.0, y), &grid)
            .expect("left of the first note still resolves");
        assert_eq!(before.position, MusicalPosition::origin());
    }

    #[test]
    fn position_at_anchors_on_the_notehead_not_the_accidental() {
        // An accidental shares its note's `Pitch` source but is drawn left of the
        // notehead. Sharpen a note so it grows an accidental, then verify a click on
        // its notehead still snaps to the note's onset — the accidental's leftward x
        // must not become the time anchor.
        let base = open_rich(0x5EED);
        let region = a_region_with(&base, true);
        let events = region_pitched_events(&base, region);
        assert!(
            events.len() >= 2,
            "the metric region renders multiple notes"
        );
        // Grid = the first inter-onset gap, so every onset is a grid multiple.
        let grid = GridResolution {
            step: MusicalDuration(events[1].0 .0.sub(&events[0].0 .0)),
        };
        let (_, _, origin_y) = region_staff_line(&base, region);

        // Use the first note past the origin whose sharpen renders an accidental
        // (a semitone up is a natural for some spellings, e.g. E→F — skip those).
        let mut tested = false;
        for (onset, pid) in events.iter().skip(1) {
            let mut session = open_rich(0x5EED);
            select_pitch(&mut session, *pid);
            if session.transpose_selection(1).is_err() {
                continue;
            }
            let glyph_x = |synthesized: bool| {
                session
                    .resolved()
                    .glyphs
                    .iter()
                    .filter(|g| {
                        g.provenance.source == TypedObjectId::Pitch(*pid)
                            && g.provenance.synthesis.is_some() == synthesized
                    })
                    .map(|g| g.position.x.0)
                    .next()
            };
            let Some(notehead_x) = glyph_x(false) else {
                continue;
            };
            let Some(accidental_x) = glyph_x(true) else {
                continue; // the sharpen produced a natural; not the case we want
            };
            assert!(
                accidental_x < notehead_x,
                "the accidental sits left of the notehead"
            );
            // The onset's anchor must be the notehead x, not the (leftmost) accidental
            // — the exact check, independent of how coarse the grid is.
            let anchor_x = session
                .position_anchors(region, None)
                .into_iter()
                .find(|(o, _)| o == onset)
                .map(|(_, x)| x)
                .expect("the sharped note has a rendered anchor");
            assert_eq!(
                anchor_x, notehead_x,
                "the onset anchors on the notehead, not the accidental left of it"
            );
            // And end-to-end: clicking the notehead snaps to the note's onset.
            let gp = session
                .position_at(Point::new(notehead_x, origin_y + 1.0), &grid)
                .expect("a metric region under the click");
            assert_eq!(gp.position, *onset, "the click snaps to the note's onset");
            tested = true;
            break;
        }
        assert!(
            tested,
            "a sharpened note rendered an accidental to test the anchor against"
        );
    }

    #[test]
    fn position_at_rejects_a_non_metric_region() {
        let session = open_rich(0x5EED);
        // The fixture's middle region is proportional (wall-clock events) — it has no
        // musical onset to land on, so a click over it yields no grid position.
        let region = a_region_with(&session, false);
        assert!(!session.region_is_metric(region));
        let grid = GridResolution {
            step: MusicalDuration(RationalTime::new(1, 4).unwrap()),
        };
        assert_eq!(
            session.position_at(point_on_region_staff(&session, region), &grid),
            None
        );
    }

    #[test]
    fn position_at_rejects_a_non_positive_grid() {
        let session = open_rich(0x5EED);
        let region = a_region_with(&session, true);
        let at = point_on_region_staff(&session, region);
        // A positive grid resolves; a zero/negative one cannot snap, so it is refused.
        let ok = GridResolution {
            step: MusicalDuration(RationalTime::new(1, 12).unwrap()),
        };
        assert!(session.position_at(at, &ok).is_some());
        let zero = GridResolution {
            step: MusicalDuration(RationalTime::zero()),
        };
        assert_eq!(session.position_at(at, &zero), None);
    }

    /// Where [`install_two_system_geometry`] puts each system's staff bottom line
    /// (the step origin), in world y: system 1 on top, system 2 below it.
    const SYS1_ORIGIN_Y: f32 = 0.0;
    const SYS2_ORIGIN_Y: f32 = -20.0;

    /// Overwrites `session`'s resolved geometry with a hand-built **two-system
    /// cast-off layout** over its single metric region — the shape the real
    /// engraver produces and the stub never does. The first half of the region's
    /// onsets renders on system 1, the rest on system 2; both systems start at the
    /// same left margin (x restarts, so the region-wide anchor list is
    /// x-non-monotonic in time) and sit in disjoint y bands. Each system carries a
    /// staff record whose provenance is its own bottom-line stroke — system 1 the
    /// staff's manifestation provenance, system 2 a synthesized continuation —
    /// exactly as the engraver's casting pass writes them. Only the resolved
    /// geometry is replaced (render/hit-test stay the stub's): these tests
    /// exercise the resolved-geometry queries alone.
    ///
    /// Returns the region, each event as `(onset, anchor x, system index)` in
    /// onset order, and the two system bounding boxes.
    fn install_two_system_geometry(
        session: &mut EditorSession,
    ) -> (RegionId, Vec<(MusicalPosition, f32, usize)>, Rect, Rect) {
        use epiphany_layout_ir::{
            BoundingBox, GlyphReference, GlyphStyle, Margins, Provenance, ResolvedGlyph,
            ResolvedPage, ResolvedStaff, Size2D, StaffSpace, Stroke, SynthesisInstanceKey,
            SynthesisKind,
        };

        let region = a_region_with(session, true);
        let staff = session
            .score()
            .staff_instances()
            .find(|(r, _)| *r == region)
            .map(|(_, si)| si.staff)
            .expect("the metric region has a staff instance");
        let events = region_pitched_events(session, region);
        assert!(
            events.len() >= 4,
            "four onsets give each system two anchors to fix a scale"
        );
        assert!(
            events.windows(2).all(|w| w[0].0 < w[1].0),
            "onsets are strictly ascending (distinct)"
        );
        let half = events.len() / 2;

        let staff_source = TypedObjectId::Staff(staff);
        // System 1 keeps the staff's manifestation provenance; system 2's line is
        // an engraver-synthesized continuation with its own stable id — the split
        // casting-off performs on a system-spanning stroke.
        let line_provenance = [
            Provenance::manifested(staff_source, region, vec![]),
            Provenance::synthesized(
                staff_source,
                SynthesisKind::EngravedBreak,
                SynthesisInstanceKey(1),
                vec![],
            ),
        ];
        let origins = [SYS1_ORIGIN_Y, SYS2_ORIGIN_Y];

        let systems: Vec<ResolvedSystem> = origins
            .iter()
            .zip(&line_provenance)
            .enumerate()
            .map(|(s, (&origin, provenance))| ResolvedSystem {
                provenance: if s == 0 {
                    Provenance::projected(TypedObjectId::Region(region), vec![])
                } else {
                    Provenance::synthesized(
                        TypedObjectId::Region(region),
                        SynthesisKind::EngravedBreak,
                        SynthesisInstanceKey(2),
                        vec![],
                    )
                },
                bounding_box: Rect {
                    origin: Point::new(0.0, origin - 2.0),
                    size: Size2D {
                        width: StaffSpace(90.0),
                        height: StaffSpace(STAFF_SPAN + 4.0),
                    },
                },
                staves: vec![ResolvedStaff {
                    provenance: provenance.clone(),
                    staff,
                    bounding_box: Rect {
                        origin: Point::new(0.0, origin - 0.05),
                        size: Size2D {
                            width: StaffSpace(88.0),
                            height: StaffSpace(STAFF_SPAN + 0.1),
                        },
                    },
                }],
                measures: Vec::new(),
            })
            .collect();
        let strokes: Vec<Stroke> = origins
            .iter()
            .zip(&line_provenance)
            .map(|(&y, provenance)| Stroke {
                provenance: provenance.clone(),
                from: Point::new(0.0, y),
                to: Point::new(88.0, y),
                thickness: StaffSpace(0.1),
                layer: 0,
                style: GlyphStyle::default(),
            })
            .collect();

        let mut placed: Vec<(MusicalPosition, f32, usize)> = Vec::new();
        let glyphs: Vec<ResolvedGlyph> = events
            .iter()
            .enumerate()
            .map(|(i, (onset, pid))| {
                let system = usize::from(i >= half);
                let local = if system == 0 { i } else { i - half };
                // 20 staff spaces per quarter, both systems restarting at x = 10.
                let x = 10.0 + 20.0 * local as f32;
                placed.push((onset.clone(), x, system));
                ResolvedGlyph {
                    provenance: Provenance::manifested(TypedObjectId::Pitch(*pid), region, vec![]),
                    glyph: GlyphReference::borrowed("noteheadBlack"),
                    position: Point::new(x, origins[system] + 1.0),
                    transform: None,
                    bounding_box: BoundingBox::new(0.0, -0.5, 1.2, 0.5),
                    style: GlyphStyle::default(),
                    layer: 0,
                }
            })
            .collect();

        let (sys1_box, sys2_box) = (systems[0].bounding_box, systems[1].bounding_box);
        session.resolved.pages = vec![ResolvedPage {
            provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
            number: 1,
            size: Size2D::default(),
            margins: Margins::default(),
            systems,
            free_objects: Vec::new(),
        }];
        session.resolved.glyphs = glyphs;
        session.resolved.strokes = strokes;
        (region, placed, sys1_box, sys2_box)
    }

    #[test]
    fn containing_system_requires_real_cast_geometry() {
        // The stub's page tree carries only degenerate (zero-size) system boxes:
        // no system may capture a click, and the flat single-system path stays in
        // charge — which is what keeps every pre-casting behavior unchanged.
        let session = open_plain(1);
        assert!(
            !session.resolved().pages.is_empty(),
            "the stub emits a page tree"
        );
        assert!(session.containing_system(Point::new(1.0, 0.0)).is_none());
        let region = a_region_with(&session, true);
        let at = point_on_region_staff(&session, region);
        assert!(
            session.staff_pitch_at(at).is_some(),
            "the flat path still resolves the click"
        );
    }

    #[test]
    fn staff_pitch_at_reads_the_clicked_system_origin() {
        let mut session = open_plain(1);
        let (_region, _placed, sys1, sys2) = install_two_system_geometry(&mut session);

        // Same staff-relative height, one click per system: the pitch must match —
        // system 2's step origin is its own bottom line, not system 1's. (The
        // regression: only the first line segment keeps the manifestation stable
        // id, so the flat path read every system-2 click against system 1's
        // origin, ~20 staff spaces off.)
        let p1 = session
            .staff_pitch_at(Point::new(30.0, SYS1_ORIGIN_Y + 1.0))
            .expect("a staff under the system-1 click");
        let p2 = session
            .staff_pitch_at(Point::new(30.0, SYS2_ORIGIN_Y + 1.0))
            .expect("a staff under the system-2 click");
        assert_eq!(
            p1.staff_instance, p2.staff_instance,
            "one staff, two systems"
        );
        assert_eq!(
            (p2.nominal, p2.octave),
            (p1.nominal, p1.octave),
            "the same staff-relative height names the same pitch in either system"
        );

        // The containing system is keyed on the click's y — full containment first…
        let in_sys2 = session
            .containing_system(Point::new(30.0, SYS2_ORIGIN_Y + 1.0))
            .expect("system 2 contains the point");
        assert_eq!(in_sys2.bounding_box, sys2);
        // …and a click in the inter-system gutter resolves to the nearest system
        // by vertical distance (mirroring the nearest-staff tolerance), never to
        // nothing.
        let just_under_sys1 = Point::new(30.0, rect_y_band(&sys1).0 - 1.0);
        assert_eq!(
            session
                .containing_system(just_under_sys1)
                .expect("the gutter still resolves")
                .bounding_box,
            sys1
        );
        let just_over_sys2 = Point::new(30.0, rect_y_band(&sys2).1 + 1.0);
        assert_eq!(
            session
                .containing_system(just_over_sys2)
                .expect("the gutter still resolves")
                .bounding_box,
            sys2
        );
    }

    #[test]
    fn position_at_inverts_within_the_clicked_system() {
        let mut session = open_plain(1);
        let (region, placed, _sys1, sys2) = install_two_system_geometry(&mut session);
        let half = placed.iter().filter(|(_, _, s)| *s == 0).count();
        // The fixture's onsets are consecutive quarters, so a quarter grid puts
        // every rendered onset on the grid.
        let quarter = grid(1, 4);
        let step = MusicalDuration(RationalTime::new(1, 4).unwrap());

        // Every anchor click snaps to its own onset — in both systems.
        for (onset, x, system) in &placed {
            let y = if *system == 0 {
                SYS1_ORIGIN_Y
            } else {
                SYS2_ORIGIN_Y
            } + 1.0;
            let gp = session
                .position_at(Point::new(*x, y), &quarter)
                .expect("a metric position under the click");
            assert_eq!(
                &gp.position, onset,
                "the click snaps to the clicked system's onset"
            );
        }
        // The regression pinned directly: system 2's first anchor shares its x
        // with system 1's first anchor but is a *later* time.
        let (first_sys2_onset, x0, _) = placed[half].clone();
        let gp = session
            .position_at(Point::new(x0, SYS2_ORIGIN_Y + 1.0), &quarter)
            .expect("a metric position under the click");
        assert_eq!(gp.position, first_sys2_onset);
        assert!(
            gp.position > placed[0].0,
            "a system-2 click is not a system-1 time"
        );

        // Anchor filtering: within system 2's box the run is monotonic in x and
        // carries exactly the second half of the onsets; the unfiltered
        // region-wide list is x-non-monotonic (the hazard the filter removes).
        let filtered = session.position_anchors(region, Some(&sys2));
        assert_eq!(filtered.len(), placed.len() - half);
        assert!(filtered.windows(2).all(|w| w[0].1 < w[1].1));
        assert_eq!(filtered[0].0, first_sys2_onset);
        let flat = session.position_anchors(region, None);
        assert_eq!(flat.len(), placed.len());
        assert!(
            !flat.windows(2).all(|w| w[0].1 < w[1].1),
            "the region-wide anchor list is x-non-monotonic across systems"
        );

        // End extrapolation stays within the clicked system: one anchor gap right
        // of a system's last note is that system's next grid slot. For system 1
        // that names the time system 2 renders first — the result is a musical
        // position, not a system-local one.
        let (last_onset, last_x, _) = placed.last().cloned().unwrap();
        let past = session
            .position_at(Point::new(last_x + 20.0, SYS2_ORIGIN_Y + 1.0), &quarter)
            .expect("empty space past the last note still resolves");
        assert_eq!(past.position, last_onset + step.clone());
        let (sys1_last_onset, sys1_last_x, _) = placed[half - 1].clone();
        let hang = session
            .position_at(
                Point::new(sys1_last_x + 20.0, SYS1_ORIGIN_Y + 1.0),
                &quarter,
            )
            .expect("system 1's trailing space still resolves");
        assert_eq!(hang.position, sys1_last_onset + step);
    }

    #[test]
    fn position_at_rejects_non_finite_clicks() {
        let session = open_rich(0x5EED);
        let grid = GridResolution {
            step: MusicalDuration(RationalTime::new(1, 12).unwrap()),
        };
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(session.position_at(Point::new(bad, 1.0), &grid), None);
            assert_eq!(session.position_at(Point::new(5.0, bad), &grid), None);
        }
    }

    /// A time signature with `display`, a measure of `measure`, and one beat group
    /// spanning it (so the beat-sum invariant holds).
    fn time_sig(
        display: epiphany_core::TimeSignatureDisplay,
        measure: MusicalDuration,
    ) -> TimeSignature {
        use epiphany_core::{BeatGroup, TimeSignatureId};
        TimeSignature::new(
            TimeSignatureId::new(ReplicaId(9), 0),
            display,
            measure.clone(),
            vec![BeatGroup {
                duration: measure,
                subdivision: None,
                accent: 0,
            }],
        )
        .expect("a single-beat-group time signature is well-formed")
    }

    #[test]
    fn time_signature_beat_is_one_over_the_denominator() {
        use epiphany_core::{PowerOfTwo, TimeSignatureDisplay};
        let whole = MusicalDuration(RationalTime::new(1, 1).unwrap());
        // 4/4 → quarter; 2/2 → half.
        let four_four = time_sig(
            TimeSignatureDisplay::Standard {
                numerator: 4,
                denominator: PowerOfTwo::new(4).unwrap(),
            },
            whole.clone(),
        );
        assert_eq!(time_signature_beat(&four_four), Some(dur(1, 4)));
        let cut = time_sig(
            TimeSignatureDisplay::Standard {
                numerator: 2,
                denominator: PowerOfTwo::new(2).unwrap(),
            },
            whole.clone(),
        );
        assert_eq!(time_signature_beat(&cut), Some(dur(1, 2)));
        // A meter with no single denominator has no derivable beat.
        let mixed = time_sig(
            TimeSignatureDisplay::MixedDenominators { components: vec![] },
            whole,
        );
        assert_eq!(time_signature_beat(&mixed), None);
    }

    #[test]
    fn default_grid_at_uses_the_governing_meter() {
        use epiphany_core::{PowerOfTwo, TimeSignatureDisplay};
        // Declare a 6/8 (its beat is an eighth). With no measure referencing one, the
        // score's first time signature governs — the same fallback `resolve_measure_units`
        // uses.
        let mut score = valid_score(0x5EED);
        score.time_signatures.push(time_sig(
            TimeSignatureDisplay::Standard {
                numerator: 6,
                denominator: PowerOfTwo::new(8).unwrap(),
            },
            dur(6, 8),
        ));
        let session = EditorSession::open(score, Box::new(StubSolver)).expect("renders");
        let region = a_clean_metric_region(&session);
        let at = point_on_region_staff(&session, region);
        assert_eq!(
            session.default_grid_at(at),
            Some(GridResolution { step: dur(1, 8) }),
            "a 6/8 meter gives an eighth-note grid"
        );
    }

    #[test]
    fn default_grid_at_defaults_to_a_quarter_without_a_meter() {
        // The plain fixture declares no time signatures, so the default is a quarter.
        let session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let at = point_on_region_staff(&session, region);
        assert_eq!(session.default_grid_at(at), Some(GridResolution::quarter()));
        // A non-finite click resolves to no staff, so there is no grid.
        assert_eq!(session.default_grid_at(Point::new(f32::NAN, 0.0)), None);
    }

    fn grid(numerator: i64, denominator: i64) -> GridResolution {
        GridResolution {
            step: MusicalDuration(RationalTime::new(numerator, denominator).unwrap()),
        }
    }

    /// A metric region (its time model is metric) none of whose events are tuplet
    /// members — the clean target for the make-room tests.
    fn a_clean_metric_region(session: &EditorSession) -> RegionId {
        session
            .score()
            .canvas
            .regions
            .iter()
            .filter(|r| matches!(r.time_model, RegionTimeModel::Metric(_)))
            .map(|r| r.id)
            .find(|rid| {
                session
                    .score()
                    .voices()
                    .filter(|(r, _, _)| r == rid)
                    .flat_map(|(_, _, v)| v.events.clone())
                    .all(|eid| {
                        !session
                            .score()
                            .cross_cutting
                            .tuplets
                            .iter()
                            .any(|t| t.members.contains(&eid))
                    })
            })
            .expect("a tuplet-free metric region")
    }

    fn primary_voice(session: &EditorSession, region: RegionId) -> VoiceId {
        let (_, si) = session
            .score()
            .staff_instances()
            .find(|(r, _)| *r == region)
            .expect("the region has a staff instance");
        si.voices
            .iter()
            .find(|v| v.is_primary)
            .or_else(|| si.voices.first())
            .expect("the staff has a voice")
            .id
    }

    fn voice_events(
        session: &EditorSession,
        voice: VoiceId,
    ) -> Vec<(EventId, MusicalPosition, MusicalDuration)> {
        let mut out = Vec::new();
        for ev in session.score().events.iter() {
            if ev.voice() != voice {
                continue;
            }
            if let (EventPosition::Musical(p), EventDuration::Musical(d)) =
                (ev.position(), ev.duration())
            {
                out.push((ev.id(), p.clone(), d.clone()));
            }
        }
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out
    }

    /// A click whose horizontal inverse snaps to `position` in `region` — derived from
    /// the region's rendered anchors, so it is the inverse of `position_at` (the stub
    /// solver spaces a metric region's onsets linearly, so a global fit suffices).
    fn click_for_position(
        session: &EditorSession,
        region: RegionId,
        position: &MusicalPosition,
        y: f32,
    ) -> Point {
        let anchors = session.position_anchors(region, None);
        let (p0, x0) = (anchors[0].0 .0.to_f64(), anchors[0].1 as f64);
        let last = anchors.last().unwrap();
        let (p1, x1) = (last.0 .0.to_f64(), last.1 as f64);
        let t = (position.0.to_f64() - p0) / (p1 - p0);
        Point::new((x0 + t * (x1 - x0)) as f32, y)
    }

    #[test]
    fn insert_note_at_fills_empty_space() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let before = voice_events(&session, voice);
        let (_, _, origin_y) = region_staff_line(&session, region);

        // The onset just after the last note — empty space, so a bare insert.
        let (_, last_pos, last_dur) = before.last().unwrap().clone();
        let target = MusicalPosition(last_pos.0.add(&last_dur.0));
        let at = click_for_position(&session, region, &target, origin_y + 1.0);
        let outcome = session
            .insert_note_at(at, &grid(1, 4))
            .expect("insert into empty space");
        assert!(outcome.graph_changed);

        let after = voice_events(&session, voice);
        assert_eq!(
            after.len(),
            before.len() + 1,
            "one note added, none removed"
        );
        assert!(
            after
                .iter()
                .any(|(_, p, d)| *p == target && d.0 == RationalTime::new(1, 4).unwrap()),
            "the new note sits at the snapped onset with the grid duration"
        );
    }

    #[test]
    fn insert_note_at_overwrites_a_covered_note() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let before = voice_events(&session, voice);
        let (_, _, origin_y) = region_staff_line(&session, region);

        // Click the last note with grid = its duration: the new note fully covers it.
        let (old_id, pos, dur) = before.last().unwrap().clone();
        let at = click_for_position(&session, region, &pos, origin_y + 1.0);
        session
            .insert_note_at(at, &GridResolution { step: dur })
            .expect("overwrite the covered note");

        let after = voice_events(&session, voice);
        assert_eq!(after.len(), before.len(), "delete + insert keeps the count");
        let covering = after
            .iter()
            .find(|(_, p, _)| *p == pos)
            .expect("a note at the covered onset");
        assert_ne!(covering.0, old_id, "the covered note was replaced");
        assert!(
            !after.iter().any(|(id, _, _)| *id == old_id),
            "the covered note's event is gone"
        );
    }

    #[test]
    fn insert_note_at_splits_an_enclosing_note() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let before = voice_events(&session, voice);
        let (_, _, origin_y) = region_staff_line(&session, region);

        // A grid a quarter of the first note's length, clicked one cell in, lands the
        // new note strictly inside the note (its second of four cells) — a split.
        let (first_id, first_pos, first_dur) = before.first().unwrap().clone();
        let cell = first_dur.0.mul(&RationalTime::new(1, 4).unwrap());
        let step = GridResolution {
            step: MusicalDuration(cell.clone()),
        };
        let target = MusicalPosition(first_pos.0.add(&cell));
        let at = click_for_position(&session, region, &target, origin_y + 1.0);
        session
            .insert_note_at(at, &step)
            .expect("split the enclosing note");

        let after = voice_events(&session, voice);
        assert_eq!(
            after.len(),
            before.len() + 2,
            "the split adds the new note and a tail"
        );
        let head = after
            .iter()
            .find(|(id, _, _)| *id == first_id)
            .expect("the original note survives as the head");
        assert_eq!(head.1, first_pos, "the head keeps the original onset");
        assert_eq!(head.2 .0, cell, "the head is trimmed to the click");
        assert!(
            after
                .iter()
                .any(|(id, p, d)| *id != first_id && *p == target && d.0 == cell),
            "the new note sits at the click"
        );
        let tail_start = MusicalPosition(target.0.add(&cell));
        assert!(
            after.iter().any(|(_, p, _)| *p == tail_start),
            "a tail picks up where the new note ends"
        );
    }

    #[test]
    fn insert_note_at_split_tail_keeps_the_event_shape() {
        // Give the first metric note an articulation, then split it: the tail must keep
        // the note's shape (it is a clone with fresh ids), not be rebuilt as a default
        // note that drops articulations/dynamics/stem/grace.
        let mut score = valid_score(0x5EED);
        let region = score
            .canvas
            .regions
            .iter()
            .find(|r| matches!(r.time_model, RegionTimeModel::Metric(_)))
            .unwrap()
            .id;
        let voice = score
            .voices()
            .find(|(r, _, _)| *r == region)
            .map(|(_, _, v)| v.id)
            .unwrap();
        let first_id = {
            let mut evs: Vec<(EventId, MusicalPosition)> = score
                .events
                .iter()
                .filter(|e| e.voice() == voice)
                .filter_map(|e| match e.position() {
                    EventPosition::Musical(p) => Some((e.id(), p.clone())),
                    _ => None,
                })
                .collect();
            evs.sort_by(|a, b| a.1.cmp(&b.1));
            evs[0].0
        };
        if let Some(Event::Pitched(pe)) = score.events.get_mut(first_id) {
            pe.articulations.push(epiphany_core::ArticulationMark);
        }

        let mut session = EditorSession::open(score, Box::new(StubSolver)).expect("renders");
        let (_, _, origin_y) = region_staff_line(&session, region);
        let (_, first_pos, first_dur) = voice_events(&session, voice)
            .into_iter()
            .find(|(id, _, _)| *id == first_id)
            .unwrap();
        let cell = first_dur.0.mul(&RationalTime::new(1, 4).unwrap());
        let target = MusicalPosition(first_pos.0.add(&cell));
        let at = click_for_position(&session, region, &target, origin_y + 1.0);
        session
            .insert_note_at(
                at,
                &GridResolution {
                    step: MusicalDuration(cell.clone()),
                },
            )
            .expect("split the articulated note");

        let tail_pos = MusicalPosition(target.0.add(&cell));
        let tail = session
            .score()
            .events
            .iter()
            .find(|e| {
                e.voice() == voice
                    && matches!(e.position(), EventPosition::Musical(p) if *p == tail_pos)
            })
            .expect("a tail event at the original note's remainder");
        match tail {
            Event::Pitched(pe) => assert!(
                !pe.articulations.is_empty(),
                "the split tail kept the note's articulation"
            ),
            _ => panic!("the tail of a pitched note is pitched"),
        }
        assert_ne!(
            tail.id(),
            first_id,
            "the tail is a fresh event, not the original"
        );
    }

    #[test]
    fn insert_note_at_split_tail_carries_an_authored_spelling() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let before = voice_events(&session, voice);
        let (_, _, origin_y) = region_staff_line(&session, region);
        let (first_id, first_pos, first_dur) = before.first().unwrap().clone();

        // Pin the first note's pitch with an explicit user spelling — an authored
        // override the split tail must carry onto its fresh pitch id.
        let source_pitch = {
            let ev = session.score().events.get(first_id).unwrap();
            let mut buf: Vec<&IdentifiedPitch> = Vec::new();
            ev.collect_identified_pitches(&mut buf);
            buf.first().expect("the first note has a pitch").id
        };
        let pinned = PitchSpelling::cmn(CmnNominal::D, 4);
        session
            .apply(OperationKind::RespellPitch(RespellPitchOp {
                pitch: source_pitch,
                spelling: pinned.clone(),
            }))
            .expect("the respell applies");
        assert_eq!(
            authored_spelling(session.score(), source_pitch),
            Some(pinned.clone())
        );

        // Split the pinned note (grid a quarter of its length, clicked one cell in).
        let cell = first_dur.0.mul(&RationalTime::new(1, 4).unwrap());
        let target = MusicalPosition(first_pos.0.add(&cell));
        let at = click_for_position(&session, region, &target, origin_y + 1.0);
        session
            .insert_note_at(
                at,
                &GridResolution {
                    step: MusicalDuration(cell.clone()),
                },
            )
            .expect("split the pinned note");

        // The tail is a fresh event whose pitch carries the authored spelling.
        let tail_pos = MusicalPosition(target.0.add(&cell));
        let tail = session
            .score()
            .events
            .iter()
            .find(|e| {
                e.voice() == voice
                    && matches!(e.position(), EventPosition::Musical(p) if *p == tail_pos)
            })
            .expect("a tail event");
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        tail.collect_identified_pitches(&mut buf);
        let tail_pitch = buf.first().expect("the tail has a pitch").id;
        assert_ne!(tail_pitch, source_pitch, "the tail's pitch id is fresh");
        assert_eq!(
            authored_spelling(session.score(), tail_pitch),
            Some(pinned),
            "the authored spelling carried onto the tail"
        );
    }

    #[test]
    fn insert_note_at_cascades_a_whole_tuplet() {
        let mut session = open_rich(0x5EED);
        // The rich fixture's first metric region is a 3:2 eighth-note triplet over
        // [0, 1/4), and its first member carries an in-tuplet decomposition. Tuplets are
        // atomic: a pencil overwrite touching any member removes the *whole* tuplet —
        // every member, the structure, and the now-orphaned decomposition — freeing the
        // span for the new note and leaving an invariant-valid graph.
        let region = a_region_with(&session, true);
        let voice = primary_voice(&session, region);
        let members: Vec<EventId> = voice_events(&session, voice)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(members.len(), 3, "the region is a three-member triplet");
        assert!(
            !session.score().cross_cutting.tuplets.is_empty(),
            "the region starts as a tuplet"
        );

        // Click the first member's onset with a grid of its own (eighth-note) value, so
        // the new note overlaps just that one member — yet the whole triplet cascades.
        let (_, pos, dur) = voice_events(&session, voice).into_iter().next().unwrap();
        let (_, _, origin_y) = region_staff_line(&session, region);
        let at = click_for_position(&session, region, &pos, origin_y + 1.0);
        session
            .insert_note_at(at, &GridResolution { step: dur.clone() })
            .expect("the overwrite cascades the tuplet and inserts the note");

        let after = voice_events(&session, voice);
        assert!(
            members
                .iter()
                .all(|m| !after.iter().any(|(id, _, _)| id == m)),
            "every original triplet member is gone"
        );
        assert!(
            session.score().cross_cutting.tuplets.is_empty(),
            "the tuplet structure is removed"
        );
        let inserted = after
            .iter()
            .find(|(id, _, _)| !members.contains(id))
            .expect("the new note was inserted");
        assert_eq!(inserted.1, pos, "the new note sits at the clicked onset");
        assert_eq!(inserted.2, dur, "the new note has the grid duration");
        assert!(
            epiphany_core::check_invariants(session.score()).is_empty(),
            "the cascaded graph is invariant-valid"
        );
    }

    #[test]
    fn insert_note_at_over_a_nested_tuplet_is_refused() {
        use epiphany_core::{Tuplet, TupletRatio};
        // A nested tuplet's ratio arithmetic a flat cascade cannot restate, so make-room
        // refuses rather than corrupt it. Give the fixture's flat triplet a child tuplet
        // so it is no longer flat, then try to overwrite one of its members.
        let mut score = valid_score_rich(0x5EED);
        let triplet_id = score.cross_cutting.tuplets[0].id;
        let replica = score.identity.replica_id;
        score.cross_cutting.tuplets.push(Tuplet {
            id: TupletId::new(replica, 7_000_002),
            ratio: TupletRatio::new(3, 2).unwrap(),
            members: vec![],
            parent: Some(triplet_id),
            required_total: MusicalDuration(RationalTime::new(1, 8).unwrap()),
        });
        let mut session = EditorSession::open(score, Box::new(StubSolver)).expect("renders");

        let region = a_region_with(&session, true);
        let voice = primary_voice(&session, region);
        let (_, pos, dur) = voice_events(&session, voice).into_iter().next().unwrap();
        let (_, _, origin_y) = region_staff_line(&session, region);
        let at = click_for_position(&session, region, &pos, origin_y + 1.0);
        assert_eq!(
            session.insert_note_at(at, &GridResolution { step: dur }),
            Err(EditorError::OverlapsTuplet)
        );
    }

    #[test]
    fn insert_note_at_on_a_non_metric_region_is_no_target() {
        let mut session = open_rich(0x5EED);
        // The proportional region has no musical onset to insert at.
        let region = a_region_with(&session, false);
        let at = point_on_region_staff(&session, region);
        assert_eq!(
            session.insert_note_at(at, &grid(1, 4)),
            Err(EditorError::NoInsertTarget)
        );
    }

    fn dur(numerator: i64, denominator: i64) -> MusicalDuration {
        MusicalDuration(RationalTime::new(numerator, denominator).unwrap())
    }

    /// Selects the first pitch (notehead) of `event` and returns its id.
    fn select_first_pitch_of(session: &mut EditorSession, event: EventId) -> PitchId {
        let mut buf: Vec<&IdentifiedPitch> = Vec::new();
        session
            .score()
            .events
            .get(event)
            .expect("event present")
            .collect_identified_pitches(&mut buf);
        let pid = buf.first().expect("a pitch on the event").id;
        select_pitch(session, pid);
        pid
    }

    #[test]
    fn set_selection_duration_shrinks_a_note() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let events = voice_events(&session, voice);
        let (target, pos, _) = events[0].clone();
        select_first_pitch_of(&mut session, target);

        session
            .set_selection_duration(dur(1, 8))
            .expect("shrink applies");
        let after = voice_events(&session, voice);
        let resized = after
            .iter()
            .find(|(id, _, _)| *id == target)
            .expect("the note survives");
        assert_eq!(resized.2, dur(1, 8), "the note is shorter");
        assert_eq!(resized.1, pos, "its onset is unchanged");
        assert_eq!(
            after.len(),
            events.len(),
            "no other events change (the gap is ok)"
        );
    }

    #[test]
    fn set_selection_duration_lengthens_into_empty_space() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let events = voice_events(&session, voice);
        let (last, _, _) = events.last().unwrap().clone();
        select_first_pitch_of(&mut session, last);

        session
            .set_selection_duration(dur(1, 2))
            .expect("lengthen into empty space applies");
        let after = voice_events(&session, voice);
        assert_eq!(
            after.iter().find(|(id, _, _)| *id == last).unwrap().2,
            dur(1, 2)
        );
        assert_eq!(after.len(), events.len(), "no other events change");
    }

    #[test]
    fn set_selection_duration_lengthens_and_deletes_a_covered_note() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let events = voice_events(&session, voice);
        // First note [0, 1/4); next [1/4, 1/2). Lengthen the first to 1/2 → fully covers
        // the next, which is deleted.
        let (first, _, _) = events[0].clone();
        let (second, _, _) = events[1].clone();
        select_first_pitch_of(&mut session, first);

        session
            .set_selection_duration(dur(1, 2))
            .expect("lengthen with overwrite applies");
        let after = voice_events(&session, voice);
        assert_eq!(
            after.iter().find(|(id, _, _)| *id == first).unwrap().2,
            dur(1, 2),
            "lengthened to 1/2"
        );
        assert!(
            !after.iter().any(|(id, _, _)| *id == second),
            "the covered next note was deleted"
        );
        assert_eq!(after.len(), events.len() - 1, "one event removed");
    }

    #[test]
    fn set_selection_duration_lengthen_trims_a_partial_overlap() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let events = voice_events(&session, voice);
        let (first, _, _) = events[0].clone();
        let (second, _, _) = events[1].clone();
        select_first_pitch_of(&mut session, first);

        // Lengthen [0, 1/4) to 3/8 → [0, 3/8) overlaps [1/4, 1/2)'s head → trim it.
        session
            .set_selection_duration(dur(3, 8))
            .expect("partial-overlap lengthen applies");
        let after = voice_events(&session, voice);
        assert_eq!(
            after.iter().find(|(id, _, _)| *id == first).unwrap().2,
            dur(3, 8)
        );
        let trimmed = after
            .iter()
            .find(|(id, _, _)| *id == second)
            .expect("the next note survives, trimmed");
        assert_eq!(
            trimmed.1,
            MusicalPosition(RationalTime::new(3, 8).unwrap()),
            "trimmed to start where the first now ends"
        );
        assert_eq!(trimmed.2, dur(1, 8), "with the remaining 1/8");
        assert_eq!(after.len(), events.len(), "trimmed, not deleted");
    }

    #[test]
    fn set_selection_duration_refuses_non_positive_and_tuplet_members() {
        // Non-positive duration: refused before anything else.
        let mut plain = open_plain(0x5EED);
        let region = a_clean_metric_region(&plain);
        let voice = primary_voice(&plain, region);
        let (note, _, _) = voice_events(&plain, voice)[0].clone();
        select_first_pitch_of(&mut plain, note);
        assert_eq!(
            plain.set_selection_duration(MusicalDuration(RationalTime::zero())),
            Err(EditorError::InvalidDuration)
        );

        // A tuplet member's duration is ratio-governed → refused.
        let mut rich = open_rich(0x5EED);
        let triplet = a_region_with(&rich, true);
        let tvoice = primary_voice(&rich, triplet);
        let (member, _, member_dur) = voice_events(&rich, tvoice)[0].clone();
        select_first_pitch_of(&mut rich, member);
        assert_eq!(
            rich.set_selection_duration(member_dur),
            Err(EditorError::OverlapsTuplet)
        );
    }

    #[test]
    fn set_selection_duration_refuses_a_decomposed_event() {
        use epiphany_core::{
            DecompositionAttachment, DecompositionSource, NotatedComponent, NoteValue,
        };

        let mut score = valid_score(0x5EED);
        let region = score
            .canvas
            .regions
            .iter()
            .find(|r| matches!(r.time_model, RegionTimeModel::Metric(_)))
            .unwrap()
            .id;
        let voice = score
            .voices()
            .find(|(r, _, _)| *r == region)
            .map(|(_, _, v)| v.id)
            .unwrap();
        let first = {
            let mut evs: Vec<(EventId, MusicalPosition)> = score
                .events
                .iter()
                .filter(|e| e.voice() == voice)
                .filter_map(|e| match e.position() {
                    EventPosition::Musical(p) => Some((e.id(), p.clone())),
                    _ => None,
                })
                .collect();
            evs.sort_by(|a, b| a.1.cmp(&b.1));
            evs[0].0
        };
        // A valid quarter-note decomposition of the first (quarter) note: changing the
        // note's duration would leave it no longer summing, so the resize is refused.
        score
            .decomposition_attachments
            .push(DecompositionAttachment {
                target: first,
                components: vec![NotatedComponent {
                    base_value: NoteValue::Quarter,
                    dots: 0,
                    tuplet: None,
                    tied_to_next: false,
                }],
                source: DecompositionSource::Inferred,
            });

        let mut session = EditorSession::open(score, Box::new(StubSolver)).expect("renders");
        select_first_pitch_of(&mut session, first);
        assert_eq!(
            session.set_selection_duration(dur(1, 8)),
            Err(EditorError::DecomposedEvent)
        );
    }

    #[test]
    fn set_selection_duration_refuses_a_non_note_event() {
        use epiphany_core::{StaffPosition, UnpitchedEvent, UnpitchedMemberId};

        // Turn the first metric note into an unpitched event (same id/voice/span): it
        // renders as a structural anchor and can be selected, but has no note value to
        // resize, so the duration edit is refused before anything is minted.
        let mut score = valid_score(0x5EED);
        let region = score
            .canvas
            .regions
            .iter()
            .find(|r| matches!(r.time_model, RegionTimeModel::Metric(_)))
            .unwrap()
            .id;
        let voice = score
            .voices()
            .find(|(r, _, _)| *r == region)
            .map(|(_, _, v)| v.id)
            .unwrap();
        let first = {
            let mut evs: Vec<(EventId, MusicalPosition)> = score
                .events
                .iter()
                .filter(|e| e.voice() == voice)
                .filter_map(|e| match e.position() {
                    EventPosition::Musical(p) => Some((e.id(), p.clone())),
                    _ => None,
                })
                .collect();
            evs.sort_by(|a, b| a.1.cmp(&b.1));
            evs[0].0
        };
        {
            let ev = score.events.get(first).unwrap();
            let (position, duration) = (ev.position().clone(), ev.duration().clone());
            *score.events.get_mut(first).unwrap() = Event::Unpitched(UnpitchedEvent {
                id: first,
                voice,
                position,
                duration,
                staff_position: StaffPosition(0),
                instrument_member: UnpitchedMemberId(0),
                articulations: Vec::new(),
                dynamic: None,
                stem: StemConfiguration,
                grace: None,
            });
        }

        let mut session = EditorSession::open(score, Box::new(StubSolver)).expect("renders");
        let lo = session
            .hit_test()
            .regions
            .iter()
            .find(|r| r.source == TypedObjectId::Event(first))
            .map(|r| r.layout_object)
            .expect("the unpitched event has a hit region");
        session.select(lo).expect("selects the unpitched event");
        assert_eq!(
            session.set_selection_duration(dur(1, 4)),
            Err(EditorError::WrongSelection {
                expected: "note or rest"
            })
        );
    }

    #[test]
    fn set_selection_duration_requires_a_selection() {
        let mut session = open_plain(0x5EED);
        assert_eq!(
            session.set_selection_duration(dur(1, 4)),
            Err(EditorError::NoSelection)
        );
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
        assert!(authored_spelling(&score, pid).is_none());

        score.spelling_attachments.push(SpellingAttachment {
            scope: SpellingScope::Pitch(pid),
            directive: SpellingDirective::Explicit(PitchSpelling::cmn(CmnNominal::C, 4)),
            source: SpellingSource::Propagated { from: pid },
            priority: 0,
            layer: None,
        });
        assert!(
            authored_spelling(&score, pid).is_some(),
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
    fn advisory_violating_edit_is_refused_in_authoring_but_reduces_in_replay() {
        // Chapter 6 §"Validation Modes": the session is authoring mode — an
        // advisory-violating operation is refused before an envelope is minted
        // — while the same operation, minted elsewhere, reduces cleanly through
        // raw OperationSet reduction (replay mode enforces only invariant
        // preconditions; advisory ones fail silently).
        use epiphany_core::{
            AnchorOffset, MusicalDuration, MusicalPosition, RationalTime, RegionEdge, TimeAnchor,
            WallClockTime,
        };

        // The plain fixture, with its single region given a *musical* end
        // bound of 12 whole units (the fixture's own extent is wall-clock,
        // which the advisory boundary check cannot resolve).
        let mut score = valid_score(0x5EED);
        let region_id = score.canvas.regions[0].id;
        score.canvas.regions[0].time_extent.end = TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(12))),
        };
        let instance = score.canvas.regions[0].staff_instances()[0].id;
        let voice = score.canvas.regions[0].staff_instances()[0].voices[0].id;

        // An insert whose span straddles the bound: starts at 10, ends at 14 —
        // applying it would require splitting the event across the boundary.
        let event_id = EventId::new(ReplicaId(50), 999);
        let kind = OperationKind::InsertEvent(InsertEventOp {
            staff_instance: instance,
            event: epiphany_ops::valuegen::insert_event_value(
                event_id,
                voice,
                MusicalPosition(RationalTime::from_int(10)),
                MusicalDuration(RationalTime::from_int(4)),
                &[PitchId::new(ReplicaId(50), 998)],
            ),
        });

        // Authoring: refused pre-mint, on both the single-op and the
        // transaction seam; the op log stays untouched.
        let mut session =
            EditorSession::open(score.clone(), Box::new(StubSolver)).expect("the fixture renders");
        let err = session
            .apply(kind.clone())
            .expect_err("an advisory violation must refuse the edit");
        assert!(matches!(err, EditorError::AdvisoryViolation { .. }));
        assert!(session.applied_operations().is_empty());
        let err = session
            .apply_transaction("cross-boundary insert", None, vec![kind.clone()])
            .expect_err("an advisory-violating member must refuse the transaction");
        assert!(matches!(err, EditorError::AdvisoryViolation { .. }));
        assert!(session.applied_operations().is_empty());

        // Replay: the same operation in an envelope reduces cleanly and the
        // event materializes — the advisory check has no channel into
        // reduction.
        let id = epiphany_core::OperationId::new(ReplicaId(50), 0);
        let env = OperationEnvelope {
            id,
            author: AuthorId(7),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(1), 0), id),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(kind),
        };
        let mut set = OperationSet::new();
        assert_eq!(set.accept(env), AcceptOutcome::Accepted);
        let materialized = set.reduce_onto(&score);
        assert!(
            materialized.state.is_clean(),
            "replay applies the advisory-violating insert cleanly: {:?}",
            materialized.state
        );
        assert!(
            materialized.score.events.get(event_id).is_some(),
            "the event materializes under replay"
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
        let (_, rendered_top) = session.rendered_top_of_event(event).expect("a CMN note");
        let before = session.score().live_pitch_ids();

        let outcome = session.add_note_to_selection().expect("the add applies");
        assert!(outcome.graph_changed, "a pitch was inserted");
        assert!(outcome.selection_preserved, "the anchor notehead survives");

        // Exactly one new pitch, minted under the session replica, one staff step above
        // the rendered-top note's resolved position.
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
            diatonic_index(&session.current_pitch(new_id).unwrap()),
            Some(rendered_top + 1),
            "the new note's staff position is one step above the rendered top"
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
    fn add_to_a_respelled_note_stacks_above_the_rendered_position() {
        use epiphany_core::PitchSpelling;
        use epiphany_ops::RespellPitchOp;

        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(anchor) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let raw = diatonic_index(&session.current_pitch(anchor).unwrap()).expect("a CMN pitch");
        // Pin the note three staff steps above its raw pitch position, so the rendered
        // staff order diverges from the raw pitch order.
        let rendered = raw + 3;
        session
            .apply(OperationKind::RespellPitch(RespellPitchOp {
                pitch: anchor,
                spelling: PitchSpelling::cmn(
                    nominal_from_index(rendered.rem_euclid(7)),
                    rendered.div_euclid(7) as i8,
                ),
            }))
            .expect("the respell applies");

        // The add no longer refuses; it stacks one step above the *rendered* position
        // (raw + 4), not above the raw pitch position (raw ranking would give raw + 1).
        let before = session.score().live_pitch_ids();
        session
            .add_note_to_selection()
            .expect("the override-aware add applies");
        let new_id = *session
            .score()
            .live_pitch_ids()
            .difference(&before)
            .next()
            .expect("a new pitch");
        assert_eq!(
            diatonic_index(&session.current_pitch(new_id).unwrap()),
            Some(rendered + 1),
            "stacked above the rendered (respelled) position, not the raw one"
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

        let inserted = note_stepped(&top, 1).expect("a CMN note steps");
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
        assert!(authored_spelling(session.score(), pitch).is_some());

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
    fn undo_and_redo_a_transpose() {
        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pid) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        let before = session.current_pitch(pid).unwrap();
        session.transpose_selection(1).expect("sharpen");
        let after = session.current_pitch(pid).unwrap();
        assert_ne!(before, after);
        assert!(session.can_undo() && !session.can_redo());

        let outcome = session.undo().expect("undo");
        assert!(outcome.graph_changed);
        assert_eq!(
            session.current_pitch(pid).unwrap(),
            before,
            "undo restores the value"
        );
        assert!(!session.can_undo() && session.can_redo());

        session.redo().expect("redo");
        assert_eq!(
            session.current_pitch(pid).unwrap(),
            after,
            "redo re-applies the edit"
        );
        assert!(session.can_undo() && !session.can_redo());
    }

    #[test]
    fn undo_restores_a_deleted_note() {
        // The CRDT is delete-wins (a tombstone is permanent), so undo cannot *invert* a
        // delete — it re-reduces the log without it, and the note reappears.
        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pid) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        assert!(session.score().live_pitch_ids().contains(&pid));
        session.delete_selection().expect("delete");
        assert!(!session.score().live_pitch_ids().contains(&pid), "deleted");

        session.undo().expect("undo");
        assert!(
            session.score().live_pitch_ids().contains(&pid),
            "undo brings the deleted note back"
        );
        session.redo().expect("redo");
        assert!(
            !session.score().live_pitch_ids().contains(&pid),
            "redo deletes it again"
        );
    }

    #[test]
    fn undo_reverts_a_split_insert_as_one_unit() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let before = voice_events(&session, voice);
        let (_, _, origin_y) = region_staff_line(&session, region);

        let (_, first_pos, first_dur) = before[0].clone();
        let cell = first_dur.0.mul(&RationalTime::new(1, 4).unwrap());
        let target = MusicalPosition(first_pos.0.add(&cell));
        let at = click_for_position(&session, region, &target, origin_y + 1.0);
        session
            .insert_note_at(
                at,
                &GridResolution {
                    step: MusicalDuration(cell),
                },
            )
            .expect("split");
        assert_eq!(voice_events(&session, voice).len(), before.len() + 2);
        assert!(
            session.applied_operations().len() > 1,
            "the split is a multi-op transaction"
        );

        session.undo().expect("undo");
        // Trim-head + tail-insert + new-note all undo together as one unit.
        assert_eq!(
            voice_events(&session, voice),
            before,
            "undo restores the original voice exactly"
        );
        assert_eq!(
            session.applied_operations().len(),
            0,
            "the whole unit is gone"
        );

        session.redo().expect("redo");
        assert_eq!(
            voice_events(&session, voice).len(),
            before.len() + 2,
            "redo re-splits"
        );
    }

    #[test]
    fn undo_restores_a_make_room_overwrite() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let before = voice_events(&session, voice);
        let (first, _, _) = before[0].clone();
        let (second, _, _) = before[1].clone();
        select_first_pitch_of(&mut session, first);

        session
            .set_selection_duration(dur(1, 2))
            .expect("lengthen with overwrite");
        assert!(
            !voice_events(&session, voice)
                .iter()
                .any(|(id, _, _)| *id == second),
            "the covered note was deleted"
        );
        session.undo().expect("undo");
        assert_eq!(
            voice_events(&session, voice),
            before,
            "undo restores both the resize and the deleted note"
        );
    }

    #[test]
    fn undo_an_override_aware_move_restores_value_and_spelling() {
        use epiphany_core::PitchSpelling;
        use epiphany_ops::RespellPitchOp;

        let mut session = open_rich(0x5EED);
        let selection = click_a_notehead(&mut session);
        let TypedObjectId::Pitch(pid) = selection.source else {
            panic!("a notehead selects a pitch");
        };
        // Pin a spelling so the move lands as a value + respell transaction.
        session
            .apply(OperationKind::RespellPitch(RespellPitchOp {
                pitch: pid,
                spelling: PitchSpelling::cmn(CmnNominal::D, 4),
            }))
            .expect("respell");
        let value = session.current_pitch(pid).unwrap();
        let spelling = authored_spelling(session.score(), pid).expect("pinned");

        session
            .move_selection_staff_step(1)
            .expect("override-aware move");
        assert_ne!(session.current_pitch(pid).unwrap(), value, "value moved");
        assert_ne!(
            authored_spelling(session.score(), pid),
            Some(spelling.clone()),
            "spelling moved"
        );

        session.undo().expect("undo the move");
        // Both transaction members (the modify and the respell) undo together.
        assert_eq!(session.current_pitch(pid).unwrap(), value, "value restored");
        assert_eq!(
            authored_spelling(session.score(), pid),
            Some(spelling),
            "spelling restored"
        );
    }

    #[test]
    fn a_new_edit_after_undo_forks_history() {
        let mut session = open_rich(0x5EED);
        assert!(!session.can_undo() && !session.can_redo());
        assert!(session.undo().is_none(), "nothing to undo");
        assert!(session.redo().is_none(), "nothing to redo");

        click_a_notehead(&mut session);
        session.transpose_selection(1).expect("e1");
        session.transpose_selection(1).expect("e2");
        assert_eq!(session.applied_operations().len(), 2);

        session.undo().expect("undo e2");
        assert!(session.can_redo());
        // A new edit past an undo clears the redo stack.
        session.transpose_selection(-1).expect("e3");
        assert!(!session.can_redo(), "the new edit forked away the redo");
        assert_eq!(
            session.applied_operations().len(),
            2,
            "e1 + e3 (e2 was undone and forked away)"
        );
    }

    #[test]
    fn a_fork_mints_a_fresh_operation_id_not_the_undone_one() {
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        session.transpose_selection(1).expect("A");
        let a_id = session.last_applied().unwrap().id;
        session.undo().expect("undo A");
        session.transpose_selection(-1).expect("B (forks A)");
        let b_id = session.last_applied().unwrap().id;

        assert_ne!(a_id, b_id, "B gets a fresh op id, not A's reused counter");
        // B is in the active prefix; A is forked out of it but kept in authored history.
        assert!(session.applied_operations().iter().any(|e| e.id == b_id));
        assert!(!session.applied_operations().iter().any(|e| e.id == a_id));
        assert!(session.authored_operations().iter().any(|e| e.id == a_id));
        assert_eq!(
            session.authored_operations().len(),
            2,
            "A and B both authored"
        );
    }

    #[test]
    fn a_fork_does_not_reuse_minted_event_ids() {
        let mut session = open_plain(0x5EED);
        let region = a_clean_metric_region(&session);
        let voice = primary_voice(&session, region);
        let (_, _, origin_y) = region_staff_line(&session, region);
        let opened: std::collections::BTreeSet<EventId> =
            session.score().events.iter().map(|e| e.id()).collect();

        // Insert into empty space (mints a fresh event), then undo it.
        let last = voice_events(&session, voice).last().unwrap().clone();
        let target = MusicalPosition(last.1 .0.add(&last.2 .0));
        let at = click_for_position(&session, region, &target, origin_y + 1.0);
        let grid = GridResolution {
            step: last.2.clone(),
        };
        session.insert_note_at(at, &grid).expect("insert 1");
        let inserted = |s: &EditorSession| -> EventId {
            *s.score()
                .events
                .iter()
                .map(|e| e.id())
                .collect::<std::collections::BTreeSet<_>>()
                .difference(&opened)
                .next()
                .expect("one inserted event")
        };
        let first = inserted(&session);
        session.undo().expect("undo insert 1");

        // Insert again at the same spot: the undone event's id left the score and the
        // active prefix, but it is still in authored history, so a fresh id is minted.
        session.insert_note_at(at, &grid).expect("insert 2");
        let second = inserted(&session);
        assert_ne!(
            second, first,
            "the second insert mints a fresh event id, not the undone one's"
        );
    }

    #[test]
    fn edits_after_a_fork_re_reduce_cleanly() {
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        session.transpose_selection(1).expect("A");
        session.transpose_selection(1).expect("B");
        session.undo().expect("undo B");
        // Each edit re-reduces the whole active prefix; if a forked op were stranded
        // pending (a missing-predecessor hole left by a non-contiguous context), the
        // reduction would be unclean and the edit would be refused. So these succeeding
        // is the proof the fork's causal contexts are correct.
        session.transpose_selection(-1).expect("C");
        session.transpose_selection(-1).expect("D");

        let active = session.applied_operations();
        assert_eq!(active.len(), 3, "A, C, D active; B forked away");
        let ids: std::collections::BTreeSet<_> = active.iter().map(|e| e.id).collect();
        assert_eq!(
            ids.len(),
            3,
            "distinct, monotonic op ids (B's was not reused)"
        );
        assert_eq!(
            session.authored_operations().len(),
            4,
            "A, B, C, D all authored"
        );
    }

    #[test]
    #[should_panic(expected = "with_identity must be set before any edit")]
    fn with_identity_is_refused_after_an_undone_edit() {
        let mut session = open_rich(0x5EED);
        click_a_notehead(&mut session);
        session.transpose_selection(1).expect("edit");
        session.undo().expect("undo");
        assert!(
            session.applied_operations().is_empty(),
            "the active prefix is empty"
        );
        // ...but the session has authored history, so the identity is still fixed.
        let _ = session.with_identity(ReplicaId(2), AuthorId(0));
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

    // --- The edit-barrier gate (Chapter 8 §"Behavior Under Unknown Extensions") ---

    use epiphany_layout_ir::{BarrierCondition, BarrierScope, EditBarrier, ObjectKind};

    /// A whole-score barrier prohibiting one operation class (no object-kind or
    /// condition narrowing), as extension `ext` would declare it.
    fn extension_prohibiting(ext: u128, op: OperationKindTag) -> ActiveExtension {
        ActiveExtension {
            extension: ExtensionRef(ext),
            barriers: vec![EditBarrier {
                scope: BarrierScope::WholeScore,
                affected_object_kinds: vec![],
                prohibited_operation_kinds: vec![op],
                condition: BarrierCondition::Always,
            }],
        }
    }

    /// The first event in the plain fixture's (single) voice, and a delete of it.
    fn first_event_delete(session: &EditorSession) -> (EventId, OperationKind) {
        let event = session
            .score()
            .voices()
            .find_map(|(_, _, v)| v.events.first().copied())
            .expect("the fixture has an event");
        let delete = OperationKind::DeleteEvent(DeleteEventOp {
            event,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        });
        (event, delete)
    }

    #[test]
    fn a_barrier_matching_edit_is_refused_and_a_non_matching_edit_proceeds() {
        let mut session = open_plain(7);
        let (_, delete) = first_event_delete(&session);
        session.set_active_extensions(vec![extension_prohibiting(
            0xE1,
            OperationKindTag::DeleteEvent,
        )]);

        // The matching edit is refused, naming the declaring extension, with
        // nothing minted (the op log untouched).
        assert_eq!(
            session.apply(delete.clone()),
            Err(EditorError::BarrierProhibited {
                extension: ExtensionRef(0xE1),
                operation: OperationKindTag::DeleteEvent,
            })
        );
        assert!(session.applied_operations().is_empty());

        // An edit of a different operation class proceeds.
        let pitch = last_event_pitch(&session);
        let transpose = OperationKind::Transpose(TransposeOp {
            targets: vec![pitch],
            chromatic_steps: 1,
        });
        session
            .apply(transpose)
            .expect("a non-prohibited class passes the gate");
        assert_eq!(session.applied_operations().len(), 1);
    }

    #[test]
    fn a_region_scoped_barrier_gates_by_the_targets_real_containment() {
        // A barrier scoped to a *different* region does not prohibit deleting
        // an event here — known scopes are evaluated precisely against the
        // target's containment, not conservatively.
        let mut session = open_plain(7);
        let (_, delete) = first_event_delete(&session);
        let elsewhere = RegionId::from_raw(u128::MAX);
        let scoped = |region| ActiveExtension {
            extension: ExtensionRef(0xE2),
            barriers: vec![EditBarrier {
                scope: BarrierScope::Region(region),
                affected_object_kinds: vec![],
                prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
                condition: BarrierCondition::Always,
            }],
        };
        session.set_active_extensions(vec![scoped(elsewhere)]);
        session
            .apply(delete.clone())
            .expect("a barrier over another region does not bind here");

        // The same barrier scoped to the event's own region refuses the edit.
        let mut session = open_plain(7);
        let (_, delete) = first_event_delete(&session);
        let here = session.score().canvas.regions[0].id;
        session.set_active_extensions(vec![scoped(here)]);
        assert_eq!(
            session.apply(delete),
            Err(EditorError::BarrierProhibited {
                extension: ExtensionRef(0xE2),
                operation: OperationKindTag::DeleteEvent,
            })
        );
    }

    #[test]
    fn an_object_kind_narrowed_barrier_matches_only_that_kind() {
        // A barrier protecting only Pitch objects prohibits a transpose but not
        // an event delete, even under the same prohibited class list.
        let mut session = open_plain(7);
        let pitch = last_event_pitch(&session);
        let pitch_kind = ObjectKind::of(&TypedObjectId::Pitch(pitch));
        session.set_active_extensions(vec![ActiveExtension {
            extension: ExtensionRef(0xE3),
            barriers: vec![EditBarrier {
                scope: BarrierScope::WholeScore,
                affected_object_kinds: vec![pitch_kind],
                prohibited_operation_kinds: vec![
                    OperationKindTag::Transpose,
                    OperationKindTag::DeleteEvent,
                ],
                condition: BarrierCondition::Always,
            }],
        }]);
        let transpose = OperationKind::Transpose(TransposeOp {
            targets: vec![pitch],
            chromatic_steps: 1,
        });
        assert!(matches!(
            session.apply(transpose),
            Err(EditorError::BarrierProhibited { .. })
        ));
        let (_, delete) = first_event_delete(&session);
        session
            .apply(delete)
            .expect("an Event target does not match a Pitch-kind barrier");
    }

    #[test]
    fn phase3_operation_kinds_derive_subjects_and_gate_on_barriers() {
        // The Phase-3 tranche (CreateStaff / SetTimeSignature / SetTempoSegment
        // / SetStaffLayout) participates in the barrier gate: a whole-score
        // barrier naming each tag refuses the matching edit before minting.
        let mut session = open_plain(7);
        let region = session.score().canvas.regions[0].id;
        let instance = session.score().canvas.regions[0].staff_instances()[0].id;
        let staff = session.score().staves[0].id;
        let instrument = session.score().staves[0].instrument;
        let anchor = epiphany_core::TimeAnchor::Region {
            id: region,
            edge: epiphany_core::RegionEdge::Start,
            offset: epiphany_core::AnchorOffset::Zero,
        };
        let edits: Vec<(OperationKindTag, OperationKind)> = vec![
            (
                OperationKindTag::InsertStaff,
                OperationKind::CreateStaff(epiphany_ops::CreateStaffOp {
                    staff: epiphany_ops::valuegen::staff(staff, instrument),
                }),
            ),
            (
                OperationKindTag::SetTimeSignature,
                OperationKind::SetTimeSignature(epiphany_ops::SetTimeSignatureOp {
                    region,
                    anchor: anchor.clone(),
                    time_signature: None,
                }),
            ),
            (
                OperationKindTag::SetTempoSegment,
                OperationKind::SetTempoSegment(epiphany_ops::SetTempoSegmentOp {
                    region: Some(region),
                    start: anchor,
                    segment: None,
                }),
            ),
            (
                OperationKindTag::SetStaffLayout,
                OperationKind::SetStaffLayout(epiphany_ops::SetStaffLayoutOp {
                    staff_instance: instance,
                    instrument_override: None,
                    staff_lines_override: None,
                    visible: false,
                }),
            ),
        ];
        for (tag, kind) in &edits {
            session.set_active_extensions(vec![extension_prohibiting(0xE7, *tag)]);
            assert_eq!(
                session.apply(kind.clone()),
                Err(EditorError::BarrierProhibited {
                    extension: ExtensionRef(0xE7),
                    operation: *tag,
                }),
                "a whole-score barrier naming {tag:?} must refuse the edit"
            );
        }
        assert!(session.applied_operations().is_empty());

        // With no barriers active, the layout advisory applies end to end (the
        // spec declares no advisory preconditions for the tranche).
        session.set_active_extensions(vec![]);
        session
            .apply(OperationKind::SetStaffLayout(
                epiphany_ops::SetStaffLayoutOp {
                    staff_instance: instance,
                    instrument_override: None,
                    staff_lines_override: None,
                    visible: false,
                },
            ))
            .expect("an unbarred layout write applies");
        let materialized = session
            .score()
            .staff_instances()
            .find(|(_, si)| si.id == instance)
            .expect("instance survives")
            .1
            .clone();
        assert!(!materialized.visible);
    }

    #[test]
    fn repeat_authoring_kinds_derive_subjects_and_gate_on_barriers() {
        // The repeat pair (schema-major-2 revision) participates in the
        // barrier gate: a whole-score barrier naming each tag refuses the
        // matching edit before minting, and with no barriers active the
        // create/delete pair applies end to end (subjects derive from the
        // repeat's anchor sites through `repeat_event_refs`).
        let mut session = open_plain(11);
        let events: Vec<epiphany_core::EventId> = session
            .score()
            .voices()
            .flat_map(|(_, _, v)| v.events.clone())
            .collect();
        let (e0, e1) = (events[0], events[1]);
        let rid = epiphany_core::RepeatStructureId::new(epiphany_core::ReplicaId(0xED), 1);
        let create = OperationKind::CreateRepeatStructure(epiphany_ops::CreateRepeatStructureOp {
            repeat: epiphany_ops::valuegen::repeat_structure(rid, e0, e1),
        });
        let delete = OperationKind::DeleteRepeatStructure(epiphany_ops::DeleteRepeatStructureOp {
            repeat: rid,
        });

        for (tag, kind) in [
            (OperationKindTag::CreateRepeatStructure, create.clone()),
            (OperationKindTag::DeleteRepeatStructure, delete.clone()),
        ] {
            session.set_active_extensions(vec![extension_prohibiting(0xE8, tag)]);
            assert_eq!(
                session.apply(kind),
                Err(EditorError::BarrierProhibited {
                    extension: ExtensionRef(0xE8),
                    operation: tag,
                }),
                "a whole-score barrier naming {tag:?} must refuse the edit"
            );
        }
        assert!(session.applied_operations().is_empty());

        session.set_active_extensions(vec![]);
        session
            .apply(create)
            .expect("an unbarred repeat create applies");
        assert!(
            session
                .score()
                .cross_cutting
                .repeats
                .iter()
                .any(|r| r.id == rid),
            "the mint materializes"
        );
        session
            .apply(delete)
            .expect("an unbarred repeat delete applies");
        assert!(
            !session
                .score()
                .cross_cutting
                .repeats
                .iter()
                .any(|r| r.id == rid),
            "the tombstone removes it"
        );
    }

    #[test]
    fn a_region_scoped_barrier_gates_a_region_anchored_repeat() {
        // A repeat anchored ONLY to a region derives that region as its
        // containment (repeat_context walks all anchor objects, not just
        // events) — a region-scoped barrier over it must fire, and one over
        // another region must not. (An event-only walk left region-anchored
        // repeats with a default context, silently bypassing the scope.)
        let scoped = |region| ActiveExtension {
            extension: ExtensionRef(0xE9),
            barriers: vec![EditBarrier {
                scope: BarrierScope::Region(region),
                affected_object_kinds: vec![],
                prohibited_operation_kinds: vec![OperationKindTag::CreateRepeatStructure],
                condition: BarrierCondition::Always,
            }],
        };
        let create_in = |session: &EditorSession| {
            let here = session.score().canvas.regions[0].id;
            let anchor = |edge| epiphany_core::TimeAnchor::Region {
                id: here,
                edge,
                offset: epiphany_core::AnchorOffset::Zero,
            };
            OperationKind::CreateRepeatStructure(epiphany_ops::CreateRepeatStructureOp {
                repeat: epiphany_core::RepeatStructure {
                    id: epiphany_core::RepeatStructureId::new(epiphany_core::ReplicaId(0xEE), 1),
                    start: anchor(epiphany_core::RegionEdge::Start),
                    end: anchor(epiphany_core::RegionEdge::End),
                    kind: epiphany_core::RepeatKind::SimpleRepeat { count: 2 },
                    voltas: Vec::new(),
                },
            })
        };

        let mut session = open_plain(7);
        let elsewhere = RegionId::from_raw(u128::MAX);
        session.set_active_extensions(vec![scoped(elsewhere)]);
        let create = create_in(&session);
        session
            .apply(create)
            .expect("a barrier over another region does not bind here");

        let mut session = open_plain(7);
        let here = session.score().canvas.regions[0].id;
        session.set_active_extensions(vec![scoped(here)]);
        let create = create_in(&session);
        assert_eq!(
            session.apply(create),
            Err(EditorError::BarrierProhibited {
                extension: ExtensionRef(0xE9),
                operation: OperationKindTag::CreateRepeatStructure,
            }),
            "the protected region's own barrier fires for a region-anchored repeat"
        );
    }

    #[test]
    fn an_unsafe_edit_crosses_the_barrier_and_records_the_tombstone_obligation() {
        let mut session = open_plain(7);
        let (_, delete) = first_event_delete(&session);
        session.set_active_extensions(vec![extension_prohibiting(
            0xE4,
            OperationKindTag::DeleteEvent,
        )]);
        assert!(matches!(
            session.apply(delete.clone()),
            Err(EditorError::BarrierProhibited { .. })
        ));

        // The explicit unsafe edit proceeds ...
        let outcome = session.apply_unsafe(delete).expect("the unsafe edit lands");
        assert!(outcome.graph_changed);
        // ... records that the crossed extension's chunks MUST be tombstoned
        // (spec §"Behavior Under Unknown Extensions") ...
        assert!(session
            .extensions_requiring_tombstone()
            .contains(&ExtensionRef(0xE4)));
        // ... and deactivates the extension (its data is gone, so its barriers
        // no longer bind): the next matching edit passes the ordinary gate.
        assert!(session.active_extensions().is_empty());
        let (_, next_delete) = first_event_delete(&session);
        session
            .apply(next_delete)
            .expect("the crossed extension's barriers no longer bind");
    }

    #[test]
    fn an_unsafe_edit_crossing_no_barrier_records_nothing() {
        let mut session = open_plain(7);
        session.set_active_extensions(vec![extension_prohibiting(
            0xE5,
            OperationKindTag::DeleteEvent,
        )]);
        let pitch = last_event_pitch(&session);
        session
            .apply_unsafe(OperationKind::Transpose(TransposeOp {
                targets: vec![pitch],
                chromatic_steps: 1,
            }))
            .expect("an unsafe apply of a non-matching edit is an ordinary apply");
        assert!(session.extensions_requiring_tombstone().is_empty());
        assert_eq!(session.active_extensions().len(), 1);
    }

    #[test]
    fn the_barrier_gate_and_the_advisory_gate_coexist() {
        use epiphany_core::{AnchorOffset, RegionEdge, ReplicaId};
        // The plain fixture with its region's end bound declared at 12 whole
        // units — the shape under which an event spanning 10..14 fails the
        // InsertEvent advisory precondition (Chapter 6 §6.10).
        let mut score = valid_score(7);
        let region_id = score.canvas.regions[0].id;
        score.canvas.regions[0].time_extent.end = epiphany_core::TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(12))),
        };
        let instance = score.canvas.regions[0].staff_instances()[0].id;
        let voice = score.canvas.regions[0].staff_instances()[0].voices[0].id;
        let mut session =
            EditorSession::open(score, Box::new(StubSolver)).expect("the bounded fixture renders");
        let crossing_insert = |duration: i32| {
            OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event: epiphany_ops::valuegen::insert_event_value(
                    EventId::new(ReplicaId(50), 999),
                    voice,
                    MusicalPosition(RationalTime::from_int(10)),
                    MusicalDuration(RationalTime::from_int(duration)),
                    &[PitchId::new(ReplicaId(50), 998)],
                ),
            })
        };

        // With no barriers, the advisory gate alone refuses the crossing span.
        assert!(matches!(
            session.apply(crossing_insert(4)),
            Err(EditorError::AdvisoryViolation { .. })
        ));

        // With a matching barrier, the barrier gate fires first (its refusal is
        // the spec's MUST and names the unsafe-edit escape).
        session.set_active_extensions(vec![extension_prohibiting(
            0xE6,
            OperationKindTag::InsertEvent,
        )]);
        assert!(matches!(
            session.apply(crossing_insert(4)),
            Err(EditorError::BarrierProhibited { .. })
        ));

        // The unsafe path crosses the barrier but still enforces the advisory
        // gate — and a refused edit loses no data, so nothing is recorded.
        assert!(matches!(
            session.apply_unsafe(crossing_insert(4)),
            Err(EditorError::AdvisoryViolation { .. })
        ));
        assert!(session.extensions_requiring_tombstone().is_empty());
        assert_eq!(session.active_extensions().len(), 1, "nothing was crossed");

        // A span inside the bound passes the advisory gate; unsafely applying
        // it crosses the barrier and records the obligation.
        let outcome = session
            .apply_unsafe(crossing_insert(2))
            .expect("within the bound, only the barrier stood in the way");
        assert!(outcome.graph_changed);
        assert!(session
            .extensions_requiring_tombstone()
            .contains(&ExtensionRef(0xE6)));
    }

    #[test]
    fn a_transaction_is_gated_per_member_and_has_an_unsafe_sibling() {
        let mut session = open_plain(7);
        let pitch = last_event_pitch(&session);
        let transpose = OperationKind::Transpose(TransposeOp {
            targets: vec![pitch],
            chromatic_steps: 1,
        });
        session.set_active_extensions(vec![extension_prohibiting(
            0xE7,
            OperationKindTag::Transpose,
        )]);

        // A member matching a barrier refuses the whole transaction, unminted.
        assert_eq!(
            session.apply_transaction("sharpen", None, vec![transpose.clone()]),
            Err(EditorError::BarrierProhibited {
                extension: ExtensionRef(0xE7),
                operation: OperationKindTag::Transpose,
            })
        );
        assert!(session.applied_operations().is_empty());

        // The unsafe sibling crosses and records.
        session
            .apply_transaction_unsafe("sharpen", None, vec![transpose])
            .expect("the unsafe transaction lands");
        assert!(session
            .extensions_requiring_tombstone()
            .contains(&ExtensionRef(0xE7)));

        // A score-wide barrier on the descriptor class gates transactions as a
        // whole: DeclareTransaction is a score-level operation.
        let mut session = open_plain(7);
        let pitch = last_event_pitch(&session);
        session.set_active_extensions(vec![extension_prohibiting(
            0xE8,
            OperationKindTag::DeclareTransaction,
        )]);
        assert_eq!(
            session.apply_transaction(
                "sharpen",
                None,
                vec![OperationKind::Transpose(TransposeOp {
                    targets: vec![pitch],
                    chromatic_steps: 1,
                })],
            ),
            Err(EditorError::BarrierProhibited {
                extension: ExtensionRef(0xE8),
                operation: OperationKindTag::DeclareTransaction,
            })
        );
    }
}
