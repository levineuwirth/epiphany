# epiphany-editor-core — Decisions

Decision log for the headless editing seam (`EditorSession`). Older decisions
for this crate were recorded in session notes and the Pass-12 batch rows it
filed (P12-E4, P12-E5); this file starts with their ratification.


## Pass 12 G-pass (2026-07-07): E4/E5 are ratified

Dispositions in `spec/PASS12_RATIFICATION_LOG.md` ("G-pass tranche").
**E4** adopt-as-implemented (`req:format:barrier-matching`): target-free
operations (`SetMetadata`, `DeclareTransaction`) are matched by score-wide
barriers only; opaque `Registered` operations match fully conservatively.
**E5** semantics ratified (`req:format:unsafe-tombstone`): crossing a barrier
immediately deactivates the extension's remaining barriers; the crossing MUST
be durably recorded at the next commit; a tombstoned `required = true`
extension leaves the bundle read-only for dependents. The manifest-side byte
encoding is deferred to the Binary Format companion (new open question there:
the manifest is frozen at major 0, so the record rides the blob layer or a
new chunk kind — next bundle-format tranche);
`extensions_requiring_tombstone()` remains the producer awaiting that
consumer.

## Selection v2 — the selection set with an anchor (2026-07-23, T2-W1)

Dispatched under `spec/CONTRACT_EDITOR_T2_SELECTION.md` §W1. Replaces the
session's single `Option<Selection>` (`EditorSession::selection`, formerly
`lib.rs:304`) with a private `SelectionSet { members: Vec<Selection>, anchor:
Option<LayoutObjectId> }`. Public surface: `selections() -> &[Selection]`,
`anchor() -> Option<&Selection>`, `click`/`select` (replace-with-single,
unchanged), `toggle_at(point)` (add/remove one member), `select_within(rect:
BoundingBox)` (paint-ordered rubber-band set), `clear_selection()`; `selection()`
is kept as the anchor, copied — it fell out of `anchor()` directly, so it was
kept rather than removed (the GUI crate's call sites are out of this packet's
blast radius per the contract, and the compatibility read costs nothing).

**The set model.** Anchor identity is tracked by `LayoutObjectId`, not
duplicated as a `Selection` copy, so `anchor()`'s lookup in `members` can never
desync from a member's relayout-refreshed `source`. Two constructors populate
`members`: `replace_single` (click/select) and the paint-ordered path built by
`select_within` itself (see below) via `SelectionSet::replace_members`, which
trusts an already-ordered, already-deduped `Vec<Selection>`. `toggle_at` keeps
members in add order (not re-sorted into paint order) — the contract's "a
selection set in paint order" is select_within's contract specifically; nothing
in the spec requires toggling to re-derive true paint order for an arbitrary
mixed-source set, and doing so would need a public paint-order query
`HitTestMap` does not expose (its `paint_order()` ranking is private, by
design — only `hit`/`within` are public).

**Anchor-fallback rule (one rule, two call sites).** Both `toggle`-removing-the-
anchor and `SelectionSet::reresolve` (relayout) apply the same rule: the anchor
falls to the member now occupying its old position (i.e. the member that was
right after it), wrapping to the first member if it was last (or only), or to
`None` if the set emptied. `reresolve` computes this over the *surviving*
members' original positions, so a member that also dropped is skipped rather
than treated as "the next one". Tested directly (m3): dropping the anchor's own
member via a plain single delete (bypassing `delete_selection`, to isolate the
mechanism from the batch path) leaves the other selected member as both the
sole survivor and the new anchor.

**`select_within` and the ledger-line finding.** A hit-test region's `source`
(the score object) and `layout_object` (its layout-id anchor) are **not** the
same identity when the object manifests as more than one primitive: a note
needing ledger lines gets the ledger strokes *synthesized from* its `Pitch`
source (`Provenance::synthesized`, distinct `stable_id` per synthesis kind/key)
**and** its own notehead glyph (`Provenance::manifested`/`projected`, a
different `stable_id`) — same `source`, different `layout_object`s. Deduping a
`within(rect)` result by `layout_object` (the first design here) does **not**
merge them into one selected object; it must dedupe by `source`, and the
group's representative `layout_object` must prefer the **non-synthesized**
occurrence when one exists — tracking a ledger stroke's id would make the
member wrongly drop on a relayout that keeps the note but moves it back
on-staff (no ledger needed), even though the note itself is still live. This
grouping needs `HitRegion::synthesis`, so it lives in
`EditorSession::select_within` (which sees full `HitRegion`s), not inside
`SelectionSet` (which only ever sees bare `Selection` values). Verified with a
dedicated fixture (a note pushed to octave 8, forcing real ledger-line
strokes): one member, its `layout_object` equal to the notehead's own region,
not a ledger's.

**Single-target intents retarget to the anchor** (`transpose_selection`,
`alter_selection` on a one-member set, `move_selection_staff_step`,
`add_note_to_selection`, `insert_note_after_selection`,
`set_selection_duration`): each now reads `self.selection.anchor().copied()`
where it used to read the bare field. Behavior with one member is unchanged —
every pre-existing test in this file passed **without modification** once the
five call sites were mechanically updated (no test needed a behavioral change,
only the internal accessor changed).

**Batch `delete_selection`: `apply_transaction`, N=1 unwrapped.** Every member
maps to its own delete op (matching today's single-object mapping exactly:
`Pitch` → `DeleteIdentifiedPitch`, `Event` → `DeleteEvent` with the plain
`NotInTuplet` compensation — so a tuplet member is a genuine refusal-worthy
target, not a construction error). One member: `self.apply(kind)`, exactly
today's op stream. More than one: `self.apply_transaction(...)`, whose
reducer-level all-or-nothing rollback (`reduce_transaction_block`,
`is_member_failure`) is what atomicity actually rests on — verified (m2) that
*without* the transaction wrapper, a refused tuplet-member delete is merely a
silent, accepted **no-op** (not a rejection: `graph_delete_precondition`'s
`TupletCompensationInvalid` returns a clean `OperationEffect::NoOp`, which
registers as a conflict only *inside* `reduce_transaction_block`, never for a
standalone `apply`), so the *other* selected member's delete would have gone
through — the exact partial-batch bug the transaction wrapper exists to
prevent. This mirrors the crate's pre-existing "no transaction for one op"
idiom (`insert_note_at`, `set_selection_duration`: `if ops.len() == 1 { apply }
else { apply_transaction }`), now applied to selection-set size.

**Batch `alter_selection`: *not* wrapped in `apply_transaction`, by design —
the N=1 finding generalized.** `TransposeIntervalOp.targets` is already a
`CanonicalSet<PitchId>` (a target *set*, not a scalar target), and the reducer
is already atomic over it end to end
(`req:opcat:transpose-interval-atomic` — every target's new value and spelling
is resolved before any of them is written, and the whole operation refuses if
any target cannot transpose). So a selection's pitches ride **one** primitive
`TransposeInterval` naming every one of them, applied via plain `self.apply`,
at *every* set size — never `apply_transaction`. This is not a smaller version
of the same "avoid a gratuitous wrapper at N=1" reasoning `delete_selection`
needed; it is the discovery the contract's N=1 clause was fishing for, taken to
its conclusion: wrapping in `apply_transaction` would (a) add a
`DeclareTransaction` envelope this method has never emitted, breaking N=1
op-stream compatibility outright, and (b) at N>1, replicate — forever, in the
canonical log — a strictly more verbose encoding (one descriptor + N
single-target ops) than the wire format already provides for exactly this
case. Non-pitch members are silently ignored (not refused): "alter" has
nothing to say about a selected slur or rest, unlike delete. Verified: a
single-member set mints the identical op (`applied_operations().len() == 1`,
same `targets`/`interval`) as before this packet; a multi-pitch batch mints
exactly one `TransposeInterval` naming every selected pitch; a batch containing
one untransposable pitch (`AcousticRealization::AbsoluteHz`, pinned) leaves
*every* selected pitch — including the otherwise-transposable one — unmoved
(`graph_changed == false`), proving the reducer's own atomicity, not this
method's plumbing, is what's carrying the guarantee.

## The clipboard fragment projection (2026-07-23, T2-W4a)

Dispatched under `spec/PLAN_EDITOR_APP.md` §Ruling E (granted 2026-07-23) and
`spec/CONTRACT_EDITOR_T2_SELECTION.md` §W4. New module `src/fragment.rs` (the
versioned s-expression format, private/`pub(crate)` types) plus three new
`EditorSession` intents in `lib.rs`: `copy_selection`, `paste_at`,
`paste_over_selection`. Blast radius: `epiphany-editor-core` only — the GUI
clipboard wiring is packet W4b.

**Grammar: reuse the Text Projection's leaf productions directly, not a
re-derived lookalike.** `Pitch`, `PitchSpelling`, `MusicalDuration`,
`SlurKind`, `TieClass`, `SpanStyle`, `CurvatureOverride`,
`ArticulationMark`/`DynamicMark`/`OrnamentMark`/`StemConfiguration`/`GraceKind`/`StaffPosition`
already implement `epiphany_core::textvalue::TextValue` — the same
`Sexp`/`TextValue`/`read_sexp` machinery `epiphany-textproj`/`epiphany-ops`'s
`textproj_leaf` build on, generated by `epiphany-core`'s `struct_codec!` /
`cstyle_enum_codec!` macros alongside their binary codec. The fragment format
calls `.project()`/`::parse()` on these directly rather than reinventing
their textual shape, and needs **no new dependency on `epiphany-textproj`**:
`epiphany_core::textvalue` is already public, and `epiphany-editor-core`
already depends on `epiphany-core`. What is bespoke is only the
document-shaped container Ruling E says the fragment must NOT borrow from
TP's document grammar: `(epiphany-fragment (0 1 0) VOICES SLURS TIES)`, with
`EventRef { voice: u32, event: u32 }` (a position in the fragment's own
per-voice lanes) as the sole way anything inside a fragment refers to
anything else — never a source `EventId`/`PitchId`/`SlurId`/`TieId`. Worked
example, captured live from `copy_paste_round_trip_preserves_values_with_fresh_ids`
(a two-note voice; the first note carries an authored spelling override, the
second does not — no cross-cutting structures span this range):
```
(epiphany-fragment (0 1 0) ((voice ((event (ratio 0 1) (ratio 1 4) (pitched ((pitch-entry (pitch (scale-position "cmn-12" (cmn g 0 4)) (acoustic-pitch inherit implicit)) (some (pitch-spelling (cmn d) () 4 (spelling-render-hints false false false false))))) () () () stem-configuration ())) (event (ratio 1 4) (ratio 1 4) (pitched ((pitch-entry (pitch (scale-position "cmn-12" (cmn g 0 4)) (acoustic-pitch inherit implicit)) ())) () () () stem-configuration ()))))) () ())
```

**API, as landed (diverges from the packet's indicative shape in two
places, both flagged there and re-flagged here):**
* `copy_selection(&self) -> Result<CopyOutcome, EditorError>` — `&self`, not
  `&mut self` (it never mutates); `CopyOutcome { fragment: String, dropped:
  Vec<DroppedItem> }`. The indicative shape had `copy_selection() ->
  Result<String, EditorError>` with `dropped` living on `PasteOutcome`; the
  required boundary-slur/tie test needs the dropped report at the point the
  closure *decision* is made, which is copy time, against the source
  selection — a fragment carries no memory of what it isn't, so paste could
  never reconstruct this list. `PasteOutcome { outcome: EditOutcome,
  events_inserted: usize }` therefore carries no `dropped`.
* `paste_at(&mut self, point: Point, grid: &GridResolution, fragment: &str)
  -> Result<PasteOutcome, EditorError>`, `paste_over_selection(&mut self,
  fragment: &str) -> Result<PasteOutcome, EditorError>` — match the
  indicative shape.
* `fragment::{FragmentError, MAX_FRAGMENT_BYTES, MAX_FRAGMENT_EVENTS,
  MAX_FRAGMENT_NESTING_DEPTH}` are re-exported at the crate root;
  `EditorError::InvalidFragment(fragment::FragmentError)` folds decode
  failures in via `From`. New `EditorError` variants:
  `PartialTupletSelection { tuplet }` (copy-side closure refusal),
  `InsufficientVoicesForFragment { needed, available }` (paste-side lane
  policy, below), `EmptyFragment` (a decoded fragment naming no events).

**Closure v1, as implemented.** Tuplet refusal and the slur/tie
copy-or-drop-and-report decision are both evaluated in `copy_selection`
against the live score (`fragment.rs` never sees a tuplet, a `Score`, or a
selection — it only knows its own already-built `FragmentDocument`). A slur
whose `start_event`/`end_event` both resolve to fragment-local `EventRef`s
is carried into the fragment (`kind`/`curvature_override`/`style` copied
verbatim, since none are identities); exactly one resolving is a boundary
cut, reported via `DroppedItem::Slur`/`Tie`; neither resolving means the
slur/tie has nothing to do with this copy and is silently absent (not a
"drop" — it was never in scope). A partially-covered tuplet's members
short-circuit the whole copy before any fragment content is built.

**Points Ruling E left underdetermined, decided here (flagged per the
brief):**
1. **Event-kind scope.** Ruling E names no event-kind boundary for
   "copies". `copy_selection` scopes to what `make_room` already treats as
   copyable — a live, metric `Pitched`/`Rest` event — mirroring the crate's
   one other place this exact line gets drawn, rather than inventing a
   second boundary. Non-metric events and every other `Event` variant
   (`Unpitched`/`Indeterminate`/`Trajectory`/`Graphic`/`Cue`) are out of v1.
2. **A selection member that is not a copyable pitch/event is *skipped*, not
   refused — a deliberate departure from `delete_selection`'s hard-refusal
   precedent, discovered while writing the required `select_within`-driven
   tests.** A rubber-band `select_within` over a real staff routinely also
   selects incidental geometry it visually crosses — staff lines, a note's
   own stem — verified directly (a tight rect around two on-staff notes
   still selected the staff line their noteheads sit on, plus both notes'
   stems, five members total for two intended notes). A hard refusal
   (`delete_selection`'s own rule: anything that is not a pitch/event errors
   the whole call) would make geometric copy nearly unusable in practice.
   `copy_selection` instead mirrors `alter_selection`'s established
   "silently ignored" precedent (this file, above): a member that is not a
   pitch/event, or a pitch/event that does not resolve to a live copyable
   note/rest, is skipped; the whole copy still refuses
   (`WrongSelection`) only if *nothing* usable remains. A note's own stem
   (an `Event`-sourced region distinct from its `Pitch`-sourced notehead)
   being incidentally selected alongside its notehead is harmless either
   way — both resolve to the same `EventId` and collapse in the
   `BTreeSet`.
3. **Multi-lane paste placement is resolved positionally, refusing rather
   than guessing when short.** Ruling E specifies the destination staff/voice
   for both placement forms but not a policy for a fragment naming more than
   one voice lane. This session maps fragment lane `i` onto the destination
   staff instance's `i`-th voice (`StaffInstance::voices[i]`) for both
   `paste_at` (rooted at the clicked staff instance) and
   `paste_over_selection` (rooted at the anchor's own staff instance);
   `InsufficientVoicesForFragment` refuses cleanly when the destination has
   fewer voices than the fragment has lanes, rather than collapsing lanes
   onto one voice or dropping the extras silently.
4. **Slur/tie closure captures into the fragment *format* fully; paste does
   not yet replay a captured one.** A fully-contained slur/tie is carried in
   the fragment text (satisfying "copies" as a wire-format guarantee — the
   data is not lost, and a future paste enhancement needs no format bump to
   consume it), but Ruling E's own placement bullet names exactly
   `InsertEvent` + `RespellPitch` as what paste mints, and no required test
   exercises re-minting a pasted slur/tie (`CreateCrossCutting`, which would
   need its own `SlurId`/`TieId` high-water-mark minter alongside
   `mint_event_id`/`mint_pitch_id`). Filed here as a named follow-up, not a
   silent gap: a fragment carrying a captured slur/tie today decodes and
   pastes its notes correctly; the slur/tie itself is inert cargo until a
   later packet wires the mint.
5. **A tie's explicit `pitch_pairing` is not carried; captured ties fall
   back to `None`** (default enharmonic pairing on any future replay).
   `pitch_pairing: Option<Vec<(PitchId, PitchId)>>` keys on source
   `PitchId`s, which the fragment deliberately never carries (Ruling E: no
   object ids); remapping specific pairings through per-pitch ordinals was
   judged not worth the added grammar for a rare feature with no test
   coverage requirement. `TieClass`/`SpanStyle` are still carried in full.

**Untrusted-input caps**, each a named public constant with its own
value-asserting rejection test in `fragment.rs`: `MAX_FRAGMENT_BYTES = 1 <<
20` (byte length, checked first — cheapest rejection); `MAX_FRAGMENT_EVENTS
= 4096` (checked after structural parse, since counting needs typed
voices); `MAX_FRAGMENT_NESTING_DEPTH = 64` (`(`-nesting, checked by one
linear, non-recursive scan over the raw text *before* `read_sexp`'s
recursive-descent reader ever sees adversarial input — the scan is
string-literal-aware, so a legitimate catalog-id string's own characters
can never spuriously trip it). An unrecognized major is rejected
immediately after the version triple is read and *before* `Vec<FragmentVoice>::parse`
or its siblings ever attempt to interpret the body — "never partially
parsed" verified directly (a major-1 fragment whose body would not even
parse under today's grammar still reports `UnsupportedVersion`, not a parse
error, proving the gate runs first).

**Paste atomicity.** `paste_document` builds the *entire* op list — every
lane's make-room clears plus every fragment event's `InsertEvent` (+
`RespellPitch` for a carried spelling override, insert-before-its-own-respell,
the same discipline `make_room_ops`'s split-tail loop already uses) — before
calling `apply`/`apply_transaction` even once; a refusal discovered while
building it (`make_room` overlapping a nested tuplet) therefore can never
leave a partial mutation, regardless of how the final application is shaped.
Because of that, the packet's own suggested atomicity scenario ("make-room
hits a refusal") cannot distinguish a transaction-committing implementation
from one that applies ops individually — both return `Err` before anything
would be applied either way (verified by construction, not asserted away:
`paste_refuses_cleanly_on_a_nested_tuplet_and_changes_nothing` keeps that
scenario as a real, useful test of a different property). The mutation that
*does* separate the two designs needs a refusal that a **second lane's**
make-room hits *after* a first lane's ops have already been built —
`paste_atomicity_rolls_back_mid_transaction_on_a_second_lanes_refusal` pastes
a two-lane fragment onto a destination whose voice 0 is clear and voice 1
carries a nested tuplet; mutating the commit to run per-lane inside the
loop (instead of once, over every lane's accumulated ops) lands voice 0's
insert before voice 1's refusal is discovered, changing `canonical_bytes`
even though the call still returns `Err` — killed, confirmed live.

## Slur/tie replay on paste (2026-07-24, T2-W4b) — closing the W4a follow-up

Dispatched as the W4b packet's Half 1, ratified by the user 2026-07-23. W4a
(above, point 4) captured a fully-contained slur/tie into the fragment
*format* but never re-minted it on paste, filed as a named follow-up rather
than papered over. This closes it: `paste_document` now re-mints every
fragment slur/tie, once both endpoints resolve to freshly-minted destination
events, as a fresh `CreateCrossCutting` op — inside the same single atomic
paste transaction as the inserts/respells (Ruling E), never a second unit.

**Id mapping.** A fragment's `EventRef` names an event by its
fragment-local position, never a source id (Ruling E: no object ids) — so
paste needs its own map from that position to the *freshly minted*
destination `EventId`. Implemented as `event_map: Vec<Vec<EventId>>`
(`event_map[voice_ordinal][event_ordinal]`), built alongside the existing
per-voice insert loop and pushed once per lane (an empty `Vec` for an
empty/skipped lane) so indices stay aligned with the fragment's own voice
ordinals — a plain array index, not a fallible lookup, because
`fragment::decode` already rejected any dangling `EventRef` before this
method ever sees the document (the module's own documented invariant: "an
`EventRef` decode validated resolves — nothing downstream needs to
re-check"). A `BTreeMap<fragment::EventRef, EventId>` was the first design
considered; `EventRef` derives no `Ord`, and adding one purely for a
paste-local, non-format concern was judged not worth extending a
purpose-built, already-reviewed type's derive set. Fresh `SlurId`/`TieId`
values come from two new session minters (`mint_slur_id`/`mint_tie_id`,
mirroring `mint_event_id`/`mint_pitch_id`'s three-source high-water-mark
discipline: `base`, current `score`, and this session's **authored**
history) plus matching `Minter::slur`/`Minter::tie` fields, so several
cross-cutting mints within one paste never collide and a since-deleted or
since-undone slur/tie's id is never reused. Unlike a pitch, a deleted
cross-cutting structure has no separate score-level tombstone list to chain
(`DeleteCrossCutting` → `graph_delete_cross_cutting` removes it from
`score.cross_cutting.{slurs,ties}` outright, `reduce.rs:3539`), so the
`authored` source is what actually carries a deleted/undone slur/tie's id
forward — the same reasoning `mint_pitch_id`'s doc already gives for
pitches, just with no `base`/`score` tombstone list to add.

**Ordering: inserts before cross-cuttings, same transaction.** The
reducer's own `create_cross_cutting` precondition (`reduce.rs:3249`)
requires every anchor event already `Live` in its per-envelope object
registry — checked against reduction's own evolving state as the
transaction's envelopes apply in order, not against `self.score` (which
does not yet contain the paste at advisory-check time). `paste_document`
therefore builds every lane's `InsertEvent`s first, then every
`CreateCrossCutting` after, in one `ops` list — the existing
`apply`/`apply_transaction` dispatch (N=1 unwrapped, else one
`DeclareTransaction` + members) needed no change at all; a freshly-minted
event and the slur/tie naming it simply ride the same transaction as any
other multi-op paste. The pre-mint advisory gate
(`epiphany_ops::validate::advisory_violations`) is unaffected: its
`CreateCrossCutting(Slur)` check resolves both endpoints' regions against
`self.score` (the pre-transaction state), so a freshly-minted event
resolves to `None` on both sides and the boundary check passes vacuously —
documented as intentional in that module ("conservative for the rare member
that only violates against another member's intermediate effect").

**Tie pairing: `None`, and this is NOT a structural block.** A replayed
tie always carries `pitch_pairing: None` — the fragment format never
carries the source's explicit pairing at all (W4a's `FragmentTie` has no
such field; Ruling E: no object ids, and pairing keys on source `PitchId`s).
The packet's brief asked to fail closed (skip tie replay, report it) *if*
the model requires a pairing the fragment cannot express. Investigated and
ruled out: `Tie::pitch_pairing: Option<...>` with `None` means "pair all
pitches by enharmonic matching in ascending `PitchId` order" — a fully
representable, spec-legitimate value (Chapter 5 §"Ties";
`epiphany_core::invariants::check_tie_pairing`'s `None` arm implements
exactly this rule as a *checkable* invariant, not a requirement the reducer
enforces at mint time). `create_cross_cutting`'s only preconditions are (a)
the structure id is not already live/tombstoned and (b) its anchor events
are live — nothing inspects `pitch_pairing` at all
(`materialize_graph_cross_cutting` pushes the `Tie` value verbatim). The
pairing-consistency invariant check lives in `epiphany_core::invariants`, a
graph-wide checker this session's `apply`/`apply_transaction` never runs
(the crate's own tests call `epiphany_core::check_invariants` explicitly,
separately, when they want it — `lib.rs`'s `commit` does not). So: full tie
replay is implemented, unconditionally, with `pitch_pairing: None`; no
`dropped`-style outcome field was needed because there is no fail-closed
case to report on this path. `PasteOutcome` gained `slurs_inserted` /
`ties_inserted` counts instead (both always equal to the fragment's own
`slurs.len()`/`ties.len()` given a well-formed paste — a straightforward
"how many landed" report, not a fallibility channel).

**`CopyOutcome` gained `events_copied: usize` too** (same packet, needed by
W4b's Half 2 GUI status line: "N event(s) copied"). The fragment
grammar/decoder (`fragment.rs`) are crate-private by design (Ruling E's
format is application-internal, not a public surface), so a caller outside
this crate has no way to learn how many events a copy actually captured
without this field — `copy_selection` now sums `document.voices[*].events.len()`
once, right before encoding, and returns it alongside the fragment text.

**Tests.** `paste_replays_a_contained_slur_as_a_fresh_cross_cutting_structure`
(r1) and `paste_replays_a_contained_tie_with_the_default_pairing` (r2) each
build a source fixture with the cross-cutting structure captured directly on
the raw `Score` (mirroring `copy_selection_drops_a_boundary_cut_slur_and_reports_it`'s
technique), copy, paste far away in the *same* session, and assert the
destination carries a **second**, fresh-id structure spanning exactly the
two pasted events — r2's source tie carries an explicit `pitch_pairing` on
purpose, so the assertion that the replayed tie's pairing is `None` proves
the drop is real, not merely untested. `paste_atomicity_rolls_back_mid_transaction_with_a_slur_aboard`
(r3) reuses the existing two-lane refusal fixture shape
(`two_voice_second_voice_has_a_nested_tuplet` as the destination) with a new
source fixture whose lane 0 carries two notes and a slur; the whole paste —
cross-cutting op included — still rolls back byte-identically when lane 1's
make-room refuses. `paste_emits_one_transaction_descriptor_plus_members` (the
coordinator-added transaction-shape test) now pins the **exact** member
count with a slur aboard (descriptor + 2 inserts + 1 respell + 1
`CreateCrossCutting` = 5), re-proven live against the same "per-op loop
instead of the transaction dispatch" mutation the test's own doc comment
already described for the pre-slur version — the mutation still kills it
(0 descriptors instead of 1), confirmed and reverted.

## The note-entry caret (2026-07-24, T3-W1)

Dispatched under `spec/CONTRACT_EDITOR_T3_CARET.md` §W1. Adds
`EditorSession`'s `caret: Option<Caret>` (`Caret { voice, position,
entry_duration }`, all `pub` fields) plus `caret()`, `clear_caret()`,
`set_caret_at(point)`, `set_entry_duration(duration)`, `advance()`,
`retreat()`, `enter_nominal(nominal)`, `enter_pitch(pitch)`, `enter_rest()`,
the pure `midi_note_to_pitch(u8) -> Pitch`, and `x_at_position(region,
within, position)`. Session-local, never in the op log; undo does not move
it. `EditorError::NoCaret` is a new variant (see below).

**`NoCaret`, not an overload of `NoSelection`.** The contract asked to
check whether `NoSelection`'s shape generalizes. It doesn't cleanly: a
caret and a selection are independent session-local cursors (one can be
set while the other is empty, e.g. mid-caret-entry with nothing selected),
and `NoSelection`'s doc ("An intent needed a selection but none is set")
would read as a lie for a caret-only intent. `NoCaret` is its own variant,
same shape, one line in `Display`.

**One insertion core, reusing the pencil's machinery verbatim.**
`enter_nominal`/`enter_pitch`/`enter_rest` all funnel to a private
`enter_at_caret(pitch: Option<Pitch>)`: builds `[start, start+entry_duration)`
from the caret, calls the *same* `make_room`/`make_room_ops`/`Minter` the
pencil (`insert_note_at`) uses, appends one `InsertEvent`, dispatches
through the existing `apply`/`apply_transaction` N=1 idiom, then advances
the caret by the entry duration **only after** the apply/transaction
succeeds (the `?` on the apply result runs before the advance, so a
refused edit changes nothing including the caret). `enter_rest` passes
`pitch: None` (an empty pitch list mints a `Rest` — `note_event`'s existing
behavior, unchanged).

**Caret re-resolution rides the same seam as selection re-resolution.**
`reresolve_caret` (clears the caret iff `caret.voice` is absent from
`self.score.voices()`) is called from the same three sites
`reresolve_selection` already is — `commit`, `undo`, `redo` — right after
`self.install(materialized)`. Unlike the selection set's survivor/fallback
rule, a point cursor has no fallback: vanish just clears it.

**The vanish test uses a real op, not a synthetic swap.** The contract's
own fallback ("if no current op can remove a voice, ... test the clear via
... a synthetic score swap") turned out not to be needed:
`epiphany-ops::DeleteVoiceOp` exists (Chapter 6 §6.10, "tombstone an empty
voice … precondition no-op if it still has live events") and cleanly
deletes an empty voice. `caret_clears_when_its_voice_is_deleted` hand-adds
a fresh empty `Voice` to a fixture's staff instance (`score.identity.mint()`
+ `Voice::user(id)`, pushed via `RegionContent::staff_based_mut()`), points
the caret at it directly (`session.caret = Some(...)`, legal from the same-
crate `tests` submodule), applies `DeleteVoice`, and asserts the caret is
`None` afterward.

**Octave inference: implemented, table-tested, and its tie-break is
*provably unreachable* through the table.** `infer_octave(reference:
Option<(CmnNominal, i8)>, nominal) -> i8` computes the nearest candidate
octave in diatonic staff steps (`diff = ref_index - nominal_val = 7q + r`,
`0 <= r < 7`; down candidate at distance `r`, up candidate at distance
`7 - r`; `nearer_is_down(r, 7 - r)` picks). **Finding, verified by direct
computation, not assumed:** because the candidate octaves for a fixed
`nominal` are exactly 7 diatonic steps apart and 7 is odd, `r` and `7 - r`
can never be equal for an integer `r` (equal would need `r = 3.5`) — so
**no (reference, nominal) pair drawn from the seven CMN letters can ever
produce a genuine tie**. The contract's own suggested case, "ref G4 enter
D" (a perfect fifth either direction), was flagged by the contract itself
as needing verification ("verify this IS the equidistant case in staff
steps and if not, construct the true equidistant pair") — checked directly:
down is 3 staff steps (G4→F4→E4→D4), up is 4 (G4→A4→B4→C5→D5); **not** a
tie in staff steps (it's only a tie in semitones — a P5 both ways — which
is not the metric the contract pins: "nearest ... in diatonic staff
steps"). No true equidistant *letter* pair exists to substitute for it.
Consequence for testing: the downward tie-break (`nearer_is_down`,
`down_distance <= up_distance`) is implemented as specified and documented,
but `octave_inference_table`'s four contract rows cannot kill a flipped
comparison (verified directly — see the mutation report). `nearer_is_down`
is therefore unit-tested on its own, directly, with a synthetic tied input
(`nearer_is_down(3, 3)`) that no real call site can ever produce; this
proves the *written comparison* is correct without pretending the table
exercises it.

**Reference-pitch policy, two silent decisions the contract left open,**
both in `reference_pitch_before(voice, position)`: (1) **chord reference**
— when the nearest preceding note is a chord, its **first** pitch (as
authored) is the reference; the contract names no chord tie-break. (2)
**non-CMN reference** — a preceding note whose pitch is not a `Cmn` scale
position (a JI/serial score) is treated the same as "no reference"
(falls back to octave 4), since there is no staff-step notion to measure
from. Both are flagged here rather than silently baked in.

**Naturals-only / sharp-spelled MIDI, as pinned.** `enter_nominal` mints
via `cmn_pitch(nominal, octave)` (alteration always 0 — accidentals are a
follow-up transpose gesture, unchanged). `midi_note_to_pitch` is new and
needed an alteration-bearing pitch constructor `cmn_pitch` doesn't have;
rather than duplicate `cmn_pitch`'s body, it now delegates to a new
`chromatic_pitch(nominal, alteration, octave)`, and `midi_note_to_pitch`
is `chromatic_pitch`'s only non-zero-alteration caller. The 12-entry table
is the ordinary MIDI-to-SPN convention (`60 = C4`, `69 = A4`, every black
key sharp) with octave `note/12 - 1` (integer division; correct for the
full `u8` range including the low notes below MIDI 12, where it goes
negative into `i8`).

**`x_at_position`'s signature mirrors `position_anchors`, not
`position_at`'s point-based one.** The contract left the shape open
("region-or-point-context"). `position_at` starts from a `Point` because a
*click* is a point; the caret has no click, only a `(region-bearing voice,
position)`, and — under cast-off geometry — knowing *which system* a bare
musical position falls on requires either a point to test against
(`containing_system`) or scanning every system's anchor coverage, which is
GUI-side work this packet doesn't own. So `x_at_position(region: RegionId,
within: Option<&Rect>, position: &MusicalPosition) -> Option<f32>` takes
exactly `position_anchors`'s own two scoping parameters — `within = None`
reads the whole region as one flat run (the stub solver, and every test
here); a GUI drawing into one system supplies that system's box the same
way `position_at` derives one via `containing_system`. The engine,
`forward_x`, is the literal mirror of `invert_x` with the roles of `x` and
musical time swapped (interpolate the bracketing segment; extrapolate the
nearest end segment's slope outside the anchored span).

**`retreat`'s clamped subtraction.** There is no `MusicalPosition -
MusicalDuration` in Chapter 3's type algebra (only `Duration - Duration`
and `Position - Position -> Duration`) because an unclamped result could
go negative, which is not a valid position. `retreat_position` reimplements
it directly over the raw `RationalTime`, clamping at `MusicalPosition::origin()`.

**`set_entry_duration`'s check order** mirrors `set_selection_duration`'s
existing precedent: duration positivity (`InvalidDuration`) is checked
*before* caret existence (`NoCaret`) — a malformed argument is rejected
independent of session state.

**Tests and mutations** (all substituted, observed failing, then reversed
— never `git checkout`): t1 `caret_advances_across_a_would_be_barline`
(mutation: skip the advance in `enter_at_caret` → `left: MusicalPosition
(3/4)` vs `right: MusicalPosition(1/1)`, dies). t2
`octave_inference_table` + `octave_tie_break_prefers_downward` +
`enter_nominal_infers_octave_from_the_nearest_preceding_note` (mutation:
flip `nearer_is_down`'s `<=` to `<` → kills `octave_tie_break_prefers_downward`
only, confirming by direct observation that `octave_inference_table` is
insensitive to it — the tie-break's real unreachability, not a testing
gap). t3 `enter_pitch_reproduces_insert_note_at_s_overwrite` — twin
sessions from the same seed, one via `insert_note_at`, one via the caret at
the same voice/position/duration/pitch, asserted **byte-identical**
(`assert_eq!(session_a.score(), session_b.score())`) rather than just
field-by-field, since both sessions mint fresh ids deterministically from
identical starting state (mutation: skip `make_room_ops`, insert directly
→ **not** a reducer refusal — `apply` returns `Ok`, but the reducer's own
overlap precondition silently no-ops the bare `InsertEvent` against the
already-occupied slot, so `session_b`'s score is simply unchanged from
before the edit while `session_a`'s carries the overwrite; the assertion
catches the value mismatch, confirmed by inspecting both failing `Score`
dumps). t4 `midi_note_to_pitch_table` (mutation: `note/12` instead of
`note/12 - 1` → `A0` becomes `A1`, dies). t5
`x_at_position_round_trips_with_position_at` +
`forward_x_extrapolates_from_the_last_segment_not_the_first` — the
"wrong segment" mutation (`n - 2` → `0` in `forward_x`'s past-the-last-
anchor branch) does **not** kill the round-trip test: `valid_score`'s
onsets are uniformly time-spaced and the stub renders them uniformly in
`x`, so every segment shares one slope and segment 0 vs. the true last
segment are indistinguishable there — an honest finding, not swept under
the round-trip test's apparent coverage. A second, direct test calls
`forward_x` with hand-built, deliberately non-uniform anchors (slopes 40
and 8 x-per-whole-note); the mutation there gives 30 instead of the
correct 14, confirmed killed, then reverted. t6
`undo_restores_the_score_but_leaves_the_caret_advanced` (mutation: undo
also retreats the caret by one entry duration → `1/1` vs `5/4`, dies) and
`caret_clears_when_its_voice_is_deleted` (mutation: empty out
`reresolve_caret`'s body → caret stays `Some` after `DeleteVoice`
succeeds, dies).
