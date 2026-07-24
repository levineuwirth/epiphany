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
