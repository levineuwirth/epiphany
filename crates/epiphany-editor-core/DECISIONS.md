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
