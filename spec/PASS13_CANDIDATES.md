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

## Batch 3 — OPEN (2026-07-09)

Three candidates, so the pass reopens per the house rule. P13-S3 arrived as a
**live incorrectness** — a Push-4a follow-up audit found that the spelling-set
chain introduced to make `TransposeInterval` undoable had only one writer, so
undo erased an ordinary `RespellPitch` on either side of it. It is resolved.
S1 and S2 remain open.

| Id | One-line statement | Filed in | Status |
|---|---|---|---|
| P13-S1 | **169 of core_spec's 207 `requirement` blocks carry no `\label`**, so no conformance claim can cite them. Chapter 4 (`Tuning Systems and Pitch Spaces`) is 9/9 unlabeled and Chapter 11 (`Determinism Contract`) is 15/15, but the gap is universal, not local: `Semantic Operations` 24/27, `The Score Graph` 22/28, `Pitch` 10/13. The requirements *are* normative and *are* implemented; they simply cannot be named. Every `req:*` label the repo cites was added ad hoc by the pass that needed it | this file | **resolved** (all 207 core_spec requirements labelled, suite 277/277; and the pass found that labelling alone was insufficient — no document *numbered* its requirements, so a `\label` bound to the enclosing section and 61 of 207 shared a rendered number, one shared by six. All six documents now carry a real counter. Locked by `requirement_labels.rs`) |
| P13-S2 | `cmn-24` is declared in the built-in pitch-space table (`core_spec.tex` §"Built-in Catalog") as "CMN extended with 24-EDO quarter-tone accidentals", but **cannot be represented**: `PitchSpacePosition::Cmn.alteration` is an `i8` documented as *whole semitones*, and a quarter-tone is half of one. Either the space is not `Cmn`-representable (and needs `Integer`/`Registered`), or `alteration` needs a finer unit — a data-model major | `crates/epiphany-core/DECISIONS.md` (Push 4b blockers) | **open** (blocks Push 4b) |
| P13-S3 | The `engraved_spelling_chain` introduced with `TransposeInterval`'s undo had a **single writer**. `RespellPitch` mutates the same graph attachments but recorded only on `respell_chain`, so (a) undoing a transposed transaction after a prior respell restored the pitch and **erased the respell**, and (b) a respell landing canonically *after* the transaction was invisible to the chain, so a `StrictInverse` undo reported `Applied` and **wiped the newer authoring** instead of refusing as superseded. A `BestEffort` undo could also restore the pre-transpose pitch while leaving a spelling authored against the transposed one | `crates/epiphany-ops/DECISIONS.md` (P13-S3) | **resolved** (both operations now record on the shared key; pitch value + spelling set undo as one unit; new `req:opcat:spelling-set-chain`. The chain stays *physically* separate from `respell_chain`, which is `RespellPitch`'s LWW conflict state — folding transposes in would make a concurrent respell conflict with a transpose and move the canonical bytes of every existing history) |
| P13-S4 | **No labelled requirement governs vertical-band *heights*.** Pass 12 twice recorded a behavioural fix to the inter-staff solve — realizing an `InterStaffGap` band's declared `preferred_height` rather than a constructor default — and both times cited `req:layoutir:vertical-bands`, which never existed. The two real band requirements (`req:layoutir:primitive-band-ownership`, `req:layoutir:resolved-band-ownership`) govern *ownership*: which band a primitive belongs to, and that a resolved primitive retains it. Nothing states what a band's height means or that the solver must realize it, so the shipped behaviour is unspecified and the log invented a name for the gap | this file | **open** (found in P13-S1 review: an agent 'corrected' the dangling citations to the two ownership requirements, which replaced a visibly broken pointer with a silently wrong one) |
