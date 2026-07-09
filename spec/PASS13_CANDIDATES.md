# Pass 13 — candidate ledger

Ambiguities and cross-cut inconsistencies found since Pass 12 closed, filed
per the house rule (a batch pass opens at ≥3 candidates; this file opened
when P13-D1/D2 joined P13-K1). Each entry names the owning DECISIONS record;
this file is the index, not the analysis.

## Batch 1 — CLOSED (2026-07-08)

All four candidates are resolved (worked down in order): P13-D3 and P13-K1 by
the user's ratified calls ("fix the mint only" / "reject the introduction"),
P13-D1 and P13-D2 as correctness fixes with convergence-locked /
execute-then-fix regressions.

| Id | One-line statement | Filed in | Status |
|---|---|---|---|
| P13-K1 | The K3 verdict for a system pitch introduced by a ModifyEvent replacement value differs across a snapshot cut (in-session `TargetMissing` vs post-snapshot `SystemDerivedContentImmutable`); really "may ModifyEvent introduce never-minted pitch ids" | `crates/epiphany-ops/DECISIONS.md` (Pass-12 G-pass code tranche) | **resolved** (Pass 13: reject the introduction — modify_event refuses a never-minted system-derived pitch, verdict now snapshot-cut-invariant; user "reject the introduction") |
| P13-D1 | Undo-driven event tombstones run graph-side re-anchor/cascade but never ledger-side `reanchor_for_tombstone`: structures leave the graph while staying `Live`, no `RepairRecord` — Ch6's same-step recording MUST is unmet for undo-driven tombstones (pre-existing class: slurs/spanners; repeats now too) | `crates/epiphany-ops/DECISIONS.md` (Schema major 2, Phase D) | **resolved** (Pass 13: `tombstone_undo_targets` runs the ledger re-anchor per event target; liveness guard; convergence-locked) |
| P13-D2 | Cue-cascade recursion re-anchors against the triggering event before its tombstone lands in `objects`: a structure anchored on {X, cue-of-X} can record `Reanchored{to: X}` then `CascadeDeleted` in one effect (contradictory repair trail; plausible by code trace, unexecuted) | `crates/epiphany-ops/DECISIONS.md` (Schema major 2, Phase D) | **resolved** (Pass 13: `delete_event` tombstones before the graph delete, matching `cascade_cue`/undo; repro executed then fixed) |
| P13-D3 | `CreateCrossCutting` validates only event endpoints (`CrossCuttingValue::endpoints()`), so a SPANNER anchored to a missing region/measure mints dangling past `anchor_target_exists`; and non-event referent tombstones (`DeleteRegion` under a region-anchored spanner/repeat) re-anchor nothing — "every referenced endpoint is live" is events-only as implemented | `crates/epiphany-ops/DECISIONS.md` (Phase D follow-up) | **resolved** (Pass 13: mint fixed via `anchor_object_refs`; non-event referent re-anchoring ratified events-only, user "fix the mint only") |

## Batch 2 — CLOSED (2026-07-09)

Three candidates accumulated while the Standard-tier solver track closed and the
notation-quality pass (stems, slurs) landed. All three were **parked** as they
were found, each in `crates/epiphany-layout-ir/DECISIONS.md`, and reaching three
reopened the pass per the house rule. None was a live incorrectness in shipped
output; each was a place where the code, the spec, and the data disagreed about
what is true.

**All three resolved**, worked down in order. Two grew when examined: P13-I1 was
filed as two elided fields and was three, the third (`diagnostics`) named nowhere
in core_spec; P13-I2's fix had to reach `editor-core` as well as the projection,
or a click on a bass staff would have resolved its pitch as treble. Zero golden
churn across all three. No open Pass-13 candidates remain; a future ≥3-candidate
batch reopens the pass.

| Id | One-line statement | Filed in | Status |
|---|---|---|---|
| P13-I1 | Chapter 7's `ConstrainedLayoutIR` listing elides **three** fields the code carries: `break_origins: Vec<BreakOrigin>` (named by `req:layoutir:break-origin-attribution`, its own shape unlisted), `catalog: GlyphCatalogIdentity` (its type specified, the field unlisted), and `diagnostics: Vec<LayoutDiagnostic>` — which appears **nowhere** in core_spec, though it is how the projection's honesty rule manifests: an unspellable pitch or an unbundled glyph is placed as a fallback *and recorded*, never silently guessed | `crates/epiphany-layout-ir/DECISIONS.md` ("the ConstrainedLayoutIR listing is still abridged") | **resolved** (Pass 13: listing gains all three fields; `BreakOrigin` and `LayoutDiagnostic` shapes added; new `req:layoutir:coverage-diagnostics` ratifies as-implemented that an unengravable object is recorded AND still placed — never guessed, never dropped) |
| P13-I2 | `Staff::default_clef` is never consulted: `to_constrained` takes the active clef from the staff instance's `clef_sequence` and falls back to `Clef::default()` (treble), so a bass-clef staff that declares its clef only on the `Staff` engraves as treble. The field is decorative in the projection — is it the fallback, or should it not exist? | `crates/epiphany-layout-ir/DECISIONS.md` ("`Staff::default_clef` is never consulted") | **resolved** (Pass 13: it IS the fallback. `StaffContent` carries it, `active_clef_or` resolves against it, and `editor-core`'s hit-test reads the same function — else a click on a bass staff would resolve its pitch as treble. Removal was rejected: the field is named for its purpose, is encoded on the wire, and dropping it is schema-major) |
| P13-I3 | `BRAVURA_METRICS`' `NOTEHEAD_ANCHORS` are hand-written, unconsumed, and doubly suspect: they name `stemUpNW`/`stemDownSE` — the corners a normal notehead's stems do *not* attach to, and a pair Bravura's `noteheadBlack` does not define — and their x of `1180` reads like 1.18 staff spaces written in thousandths rather than the table's `1/1024` units (1.18 sp = 1208). They enter only `metrics_hash`, so any correction moves the `GlyphCatalogIdentity` every conformance claim declares. The font is not vendored, so the values cannot be verified in-tree | `crates/epiphany-layout-ir/DECISIONS.md` ("the notehead stem anchors are unusable as written") | **resolved** (Pass 13: deleted. Unverifiable in-tree, unconsumed, and hand-derived where every neighbouring number is machine-extracted. `extract_bravura_outlines.py --anchors` now emits them from the pinned `bravura_metadata.json`, so the table regains them generated rather than remembered; the metadata's SHA-256 is deliberately unpinned and the script refuses until an operator with the font pins it. `GlyphCatalogIdentity` moves once, now, while no conformance claim declares the old one; user "delete them + teach the extractor") |
