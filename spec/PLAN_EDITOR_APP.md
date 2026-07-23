# Editor track — from seam probe to product editor: scope and plan

Status: **T1 is split.** **T1a (visual golden harness) dispatches now,
resolver-parallel; T1b (document layer) sequences after the resolver
tranche**, behind two named blockers (§Ruling B). **Ruling C granted**
(casting-off sub-decision, decoded-pixel amendment, CI-artifact clarification
— 2026-07-23). **Ruling D granted conditionally** (condition: the T1b
contract specifies the document/session ownership API — over an unforgeable
document-bound session type or lease token — before code). **Ruling A
granted as amended (explicit, 2026-07-23). Ruling B redrafted twice and
re-scoped — not grantable until the graph-state-persistence and
versioned-decode blockers resolve.** T1a is dispatched under
`CONTRACT_EDITOR_T1A_GOLDENS.md`.
Prepared against `main` @ `2cf2dae`; revised three times on 2026-07-23
against `b0acacb` after three source-level reviews (14 + 11 + 9 findings;
every one dispositioned in §6). Every claim below was checked against the source; where
a probe was run the file and line are given. The companion executable
documents are `spec/CONTRACT_EDITOR_T<N>_<TOPIC>.md`, one per tranche, in the
house style of `CONTRACT_PUSH4B_RESOLVER.md`.

This plan has two jobs. The near job is dispatching T1a safely in parallel
with the Push-4b resolver tranche, and sequencing T1b honestly behind its
blockers. The far job is §3, the **architecture-risk map**: the selected
risks that shape the early rulings, each with its **foreclosure guard** —
what the early tranches must not do, so that no later track finds its door
welded shut by an early convenience. §3 is deliberately *not* a complete
product roadmap (see its closing note).

---

## 1. The facts that shape this track

**The editor's model is already built; only its shell is a demo.** The editing
architecture is layered exactly right for the ambition, and the layers have
sharply different maturity:

| layer | where | state |
|---|---|---|
| headless session (selection, minting, atomic apply, CRDT-correct undo) | `epiphany-editor-core` (6.7k lines, 93 tests, conformance gate `[7c]`) | **solid** |
| render→hit-test provenance contract | `epiphany-layout-ir/src/hittest.rs` | **solid** |
| SVG renderer (Bravura outlines, golden-locked) | `epiphany-render-svg` | **solid** |
| GUI shell | `epiphany-editor-gui` — 598 lines, one `main.rs`, self-described "demo binary" (`main.rs:21`), version 0.0.0 | **disposable by design** |

MuseScore's engraving engine and UI are entangled; Epiphany's are separated by
a tested seam. That separation is the asset this track must not squander:
**every editor capability lands headless in `editor-core` first, proven at the
IR boundary, before any GUI wiring** — the discipline that has already worked
through six intents, undo/redo, and the pencil.

**Fact 1 — the GUI is the only unverified surface in the repo.** The rendering
and interaction have never been visually verified (no display in the dev
environment); only the `ViewMap` click-plane math is unit-tested (3 tests,
`main.rs:530`). Everything below it is golden-locked or conformance-gated. But
verification does **not** need a display: the crate already rasterizes
headlessly via `resvg` (`main.rs:160`), so image snapshots of the rendered
score are ordinary `#[test]`s.

**Fact 2 — the interactive rendering path is a dead end; the data under it is
not.** Every edit re-renders the whole score to an SVG string, parses it with
`usvg`, rasterizes with `resvg`, and uploads one texture (`main.rs:247`). That
is O(score) per keystroke, and a single texture cannot survive real documents
(GPU max-texture dimensions). But the Bravura outlines the renderer draws from
are **staff-space, y-up path data independent of SVG**
(`outlines_generated.rs:1`), and casting-off multi-system layout has landed
(Phase 3 tranche 1, `0316160`). One structural caveat, corrected from this
plan's first draft: `ResolvedLayoutIR` carries pages → systems → staves →
measures as *structure*, but its drawable primitives — `glyphs`, `strokes`,
`curves` — are **flat top-level arrays** (`resolved.rs:86-100`), not owned by
systems. Per-system damage therefore requires explicit primitive ownership
added to the IR before T4 (§3.7), not inferred spatially (cross-system curves
and boundary-straddling primitives make inference ambiguous). And the glyph
assets are not yet a shared seam: the outline table is private to
`render-svg` and stores SVG `d` strings (`outline.rs:5`), so a typed-path /
shared-assets extraction is likewise T4-prerequisite work (§3.7).

**Fact 3 — the editor has no document.** The GUI opens a hard-coded testkit
fixture (`main.rs:197`). Neither editor crate depends on `epiphany-bundle`
(probed: no reference in either `Cargo.toml`). Meanwhile the bundle's own model
says what a saved document *is*: "A block of operation envelopes (**the
canonical document**)" (`chunk.rs:19-20`), and `Bundle::create/open/commit`
plus operation-block read/write are public API
(`bundle.rs:184,265,606,488`). The session side is ready to be wired: ops are
minted under a replica identity (`editor-core/src/lib.rs:305-333`),
`with_identity` guards on the authored history (`lib.rs:392-398`), and the
applied/authored split is explicit (`lib.rs:443`).

**Fact 4 — selection and input are one-object, click-only.** The session's
selection is `Option<Selection>` (`lib.rs:304`) — no range, no list, no caret.
These are seam gaps, not GUI gaps, and they gate copy/paste, batch edits, and
MuseScore-style note entry. They are named tranches below, not part of T1.

**Fact 5 — a parallel tranche is in flight and freezes specific surfaces.**
`CONTRACT_PUSH4B_RESOLVER.md` reserves `crates/epiphany-editor-gui/**` for this
track by name (line 27) and claims `epiphany-core` + its `DECISIONS.md` for
itself; as of `b0acacb` that tranche is actively in progress in the working
tree. Its gates pin, until it lands:

* requirement counts at **212 / 282 / 282** (`requirement_labels.rs:12-14`) —
  so this track adds **no `.tex` edits and no requirement labels**;
* the conformance suite at **8/8** (`conformance_suite.rs:202`) — so **no new
  conformance gate** is promoted mid-flight (ordinary `#[test]`s are fine;
  their gate is "0 failed", not a count);
* **zero golden churn** — their tripwire for wire leakage; this track moves no
  golden while they fly.

Probed decoupling: `ScoreTuningContext` is referenced nowhere outside
`epiphany-core`, so their field addition cannot break the editor crates.
`epiphany-bundle` and `epiphany-ops` were never in the resolver's blast
radius, but T1b's work in them waits regardless (§Ruling B blockers) — only
T1a runs during the flight.

**Fact 6 — the resolver tranche is building the editor's playback input.** Its
deliverable is (pitch-space position, tuning system, reference) → **Hz**. That
is exactly what a playback tranche consumes. Playback is therefore sequenced
*after* Push 4b rather than blocked on a tuning engine of its own; the seam is
named here so it is a plan, not a rediscovery.

**Fact 7 — CI already has the shape this track needs.** The MSRV-pinned
workspace jobs exclude `epiphany-editor-gui`; a separate pinned-stable
`editor-gui` job tests it (`ci.yml:99,109,131`). T1a's goldens run inside
that existing job with **one additive CI change**: an `if: failure()`
artifact-upload step so failing goldens' actual/expected/diff images are
reviewable after the run (Ruling C) — paths printed in assertions refer to
the ephemeral runner and are local-reproduction aids only. No effect on the
pinned counts or resolver safety. A future app crate mirrors the same
exclusion + job pattern.

**Fact 8 — every edit today re-materializes the world, and that is the
deepest technical risk on the whole road.** `EditorSession::apply` reduces the
**entire accumulated log onto the pristine open-time base** on every edit
(`lib.rs:1033`) and re-engraves the whole score. This is correct — it *is* the
canonical reduction, which is why the session renders exactly what a peer
would — and at probe scale it is fine. At orchestral scale it is the
difference between an editor and a slideshow. The hard constraint on any fix:
an incremental result MUST be **byte-identical to the from-scratch canonical
reduction** — incrementality is an optimization of the determinism contract,
never an alternative to it. The intended mechanism — a checkpoint plus the
envelope tail — exists in outline only: today's canonical-base payload
carries reducer bookkeeping, not graph values (Fact 10), so the checkpoint
form is itself blocker-i work (§Ruling B). Incremental *engraving* (re-solving only damaged
systems) has the same shape and the same determinism obligation. This is a
named track (§3.1) with a named ladder position (T4b): performance budgets are
**staged** — reduce, engrave, scene-build, paint measured separately — because
an end-to-end latency number that mixes them cannot attribute blame (§Ruling
A), and a renderer verdict is uninformative while reduction dominates.

**Fact 9 — the specified model already anticipates the product; what is
missing is engines and seam coverage, not vision.** Probed: `PartDefinition`
with per-part layout overrides, visibility overrides, and auto-cue sources is
specified, and **"Parts Are Projections, Not Storage"** is normative —
`req:graph:part-content-projection` (`core_spec.tex:6380-6410`). `LyricLine`
is a modeled graph object with a typed id and codec discriminant
(`core/src/lib.rs:360,515,551`). `SetUserSystemBreak` already exists as an
operation — authored layout decisions are ops, in the canonical document, like
everything else. The gap between Epiphany and a MuseScore-class product is not
a missing data model; it is engines, editor intents over the modeled objects,
and product surface (§3.7 is the verified registry). Model-completeness work
is *coordinated spec+core tranches* in the Push-4b mold — consumed by this
track, never invented inside it.

**Fact 10 — the canonical model constrains persistence more than this plan's
first two drafts assumed.** Verified across both reviews:

* The replicated operation set is a **grow-only CRDT**
  (`req:semops:grow-only-operation-set`, `core_spec.tex:6924`); an envelope
  leaves canonical state only through pruning whose replacement frontier
  **strictly dominates** the old one (`core_spec.tex:11536`).
* **Genesis is outside the operation set, deliberately** (ratified Pass-12
  decision): "there is no `CreateCanvas`/`CreateInstrument`"
  (`binary_format.tex:2420`), and `CreateStaff`'s graph-aware reduction
  **preconditions a live instrument** (`reduce.rs:3792`). Reduction onto an
  empty score therefore cannot reach the staff/instance/voice chain notes
  require — the §Ruling-B genesis blocker.
* The `canonical_base` role MUST stay schema-major 0 (`MaterializedState`);
  a higher-stamped base forces read-only (`bundle.rs:857`); the major-2
  full-`Score` bytes are the **acceleration-snapshot** role
  (`roundtrip.rs:339`), whose `SnapshotId` is an acknowledged test-harness
  stand-in with no normative derivation (`binary_format.tex:698`). And
  `MaterializedState` serializes reducer bookkeeping — effects, existence,
  spellings, breaks, conflicts, pending — **not** the
  `Region`/`Staff`/`Event`/`Instrument`/`Part` graph values (`reduce.rs:499`,
  `binary_format.tex:1695`); pruning may then remove the covered operation
  blocks that carried those values (`core_spec.tex:11553`), and the
  acceleration snapshot cannot repair the loss (it is discardable and MUST
  be ignored or rebuilt on disagreement). **After pruning, today's canonical
  roots cannot reconstruct an editor `Score`.**
* The format requires **migrate-on-read** for older canonical blocks
  (`binary_format.tex:2453,2532`), but `decode_envelope` takes no
  schema-major and decodes embedded values at current layouts
  (`envdecode.rs:622`); writes must stamp blocks via
  `operation_block_versioned` with the block's max payload major
  (`bundle.rs:111`) — the §Ruling-B versioned-decode blocker. **Minimal
  stamping** means a current writer *legitimately* emits major-0, -1, and -2
  blocks according to their contained values (`payload.rs:200`), and the
  manifest carries no writer provenance — "written by the current writer" is
  unprovable from the file.
* Manifest operation roots are **deduplicated, deterministically ordered
  sets** (`manifest.rs:465`) — committed history is a set, not a recoverable
  chronological prefix.
* Extension edit-barrier injection is the *bundle opener's* job
  (`lib.rs:453`); the durable unsafe-edit tombstone encoding is deferred at
  the format level (`core_spec.tex:11962` area); `Bundle::commit`
  auto-restores omitted unknown-extension declarations (`bundle.rs:678`).
* `FileStore::create` **truncates at open** (`store.rs:152`) and tracks file
  length in memory (`store.rs:141`) — the atomic-commit protocol is
  crash-safe for one cooperating writer, not a concurrent-writer protocol,
  and a lock acquired after a truncating open is too late.
* A failure at the commit-point flush is **indeterminate**: the bundle
  poisons itself read-only and requires reopening (`bundle.rs:601,744`) —
  neither a save success nor an ordinary failure. And after reopening, a
  *readable* generation is not thereby *durable*: the failed barrier was
  `sync_all` itself (`store.rs:194`), so only a fresh successful durability
  barrier settles the outcome.

Ruling B is drafted against these constraints, not around them.

---

## 2. The tranche ladder

Named in order. Each tranche gets its own contract, mutation-verified tests,
and the full gate. Nothing here edits `.tex` — if a future tranche wants
normative editor requirements, that is a separate spec-side decision made
after the resolver tranche lands.

* **T1a — the visual golden harness (resolver-parallel; contract next).**
  Ruling C, in `editor-gui` plus one additive CI artifact-upload step.
  Independent of every Ruling-B question; dispatches now.
* **T1b — the document layer (post-resolver; contract after its blockers
  resolve).** Rulings B and D: `EditorDocument` + single-writer enforcement
  in `epiphany-bundle`. Its runway, in order: (1) the **graph-state
  persistence decision** — how canonical graph state is persisted across
  *both* genesis *and* pruning, driven by the field-by-field `Score` table
  (§Ruling B, blocker i); a coordinated spec/core/ops/format decision, or an
  explicit T1b scope limit to empty/metadata/region documents; (2) the
  **versioned-decode disposition** — the migrate-on-read API in
  `epiphany-ops`, preferred and eventually mandatory, or the enforceable
  current-layout restriction (§Ruling B, blocker ii); (3) the **Ruling-D
  ownership API**, specified in the contract before code.
* **T2 — selection model v2 (post-resolver).** Range and list selection in
  `editor-core` (`Selection` becomes plural with an anchor), `within(rect)`
  wired to the GUI as rubber-band select; the batch intents selection makes
  meaningful (delete-range, transpose-range); **copy/paste, whose clipboard
  format is a T2 ruling — a versioned fragment projection, not the Text
  Projection itself**: the TP grammar defines only complete documents
  (`text_projection.tex:957`; `textproj/src/lib.rs:24`), so the fragment
  form must define closure over referenced objects, external references vs
  included dependencies, fresh-id remapping on paste, destination placement
  (region/voice/time), causal-context treatment, and partial-structure
  policy (ties, tuplets, beams, spanners). It may reuse TP leaf/value
  productions; paste mints fresh destination operations rather than
  importing source envelopes. Fragments are **untrusted input**: the ruling
  also specifies byte/count/depth limits and unknown-version rejection.
  Also: promotion of T1a's golden harness to a numbered conformance gate in
  testkit (the count moves here, deliberately).
* **T3 — note-entry caret.** The input cursor as headless session state
  (position, advance-by-duration), the MuseScore-style entry loop — with the
  entry seam **input-method-agnostic**: a caret intent takes (pitch, duration)
  and does not care whether they came from typed letters, an on-screen
  keyboard, or **step-time MIDI**. Live MIDI entry is the primary input method
  for most working musicians, so the seam is designed and tested for it from
  the first commit; the device wiring itself (a `midir`-style listener minting
  caret intents) lands app-side, T3 if cheap, T4 otherwise. Real-time
  (played-against-a-click) entry, and the metronome it requires, are
  playback-era work (T5), not T3 — audio dependencies do not enter the
  workspace before T5's architecture exists. GUI wiring after the seam is
  proven.
* **T4 — the rendering tranche.** The Ruling-A architecture built as the new
  app crate (Ruling D): direct vector canvas over `ResolvedLayoutIR`,
  viewport-culled, per-system damage against the IR's (by-then) explicit
  primitive ownership (§3.7); a **command registry** as the app's action
  architecture from day one (§3.5); the toolkit spike (§Ruling A) opens this
  tranche. Prerequisites from §3.7 land first: IR primitive ownership, the
  shared typed glyph-asset seam, and the text-run primitive decision.
* **T4b — incremental materialization + engraving (the app's exit
  criterion).** **Checkpointed-reducer-state-plus-tail** reduction (the
  "snapshot-plus-tail" name is retired: today's canonical-base payload
  cannot rehydrate graph content — §Ruling B blocker i) and per-system
  re-solve, each gated by **byte-equality against the from-scratch result**
  (§3.1). Sequenced
  *before* the app claims parity and retires the demo, and *before* T5 —
  T4's staged budgets (Fact 8) tell us how much is needed; the app does not
  ship "at parity" while paying O(score) per keystroke.
* **T5 — playback seam.** Consumes the Push-4b resolver's Hz (Fact 6);
  transport + audio sit app-side per the Chapter 1 core/product boundary.
  Scoped honestly as its own plan when reached; two scope decisions are
  already visible and named now: what `SoundConfiguration`'s opaque bytes
  *mean* (patch references? synth state? — the answer shapes §3.5's plugin
  runtime), and MTS export (§3.3) so microtonal scores sound right outside
  Epiphany.

---

## 3. The architecture-risk map

The selected risks that shape the rulings — each with what gates it and its
**foreclosure guard**. This is *not* a complete MuseScore-class roadmap: major
product areas remain deliberately unmapped here — full notation-command
coverage, score/instrument setup and templates, lyrics/text engraving,
style/inspector systems, autosave/recovery, printing, packaging/distribution,
plugin security & versioning. They are named so their absence is a decision;
each gets mapped when its gating work exists.

### 3.1 Incremental materialization + incremental engraving

The Fact-8 risk. Two coupled halves: reduce from a checkpoint + envelope
tail instead of from genesis — which needs a **reducer-from-state-plus-
frontier API in `epiphany-ops`** *and* a checkpoint payload that can
rehydrate both CRDT bookkeeping and graph content (today's
`MaterializedState` cannot, Fact 10 — the checkpoint form is §Ruling-B
blocker-i work), each its own tranche — and re-engrave only damaged systems. Both carry the byte-identity obligation — the conformance
suite's role here is an equivalence gate (incremental vs from-scratch,
byte-compared) before any incremental path is trusted. **Fast open belongs to
this track too**: acceleration snapshots with a trusted validation story, a
selection/retention policy (frontier coverage, reduction version, profile),
and a normative `SnapshotId` — T1b deliberately writes none (Ruling B), so
"fast open" is never claimed while cold-open still replays the log. Ladder
position: T4b.
**Foreclosure guard:** Ruling B keeps every saved document's full envelope log
canonical and intact (grow-only), which is precisely what makes any future
incremental result *checkable* against from-scratch truth. T4's damage unit is
the *system* — the same unit incremental engraving will re-solve — so paint
damage and engrave damage share a boundary instead of fighting over one.

### 3.2 Parts and multi-view (document ≠ session ≠ view)

Specified and normative (Fact 9): parts are projections with layout/visibility
overrides. Architecturally this forces the split Ruling D now makes concrete:
one **document** (bundle + canonical operation state), one **session** per
editing replica (undo, selection, caret), many **views** (score + each part,
page vs continuous — each its own engraving of the same graph, so solver +
resolved layout ultimately belong at the view level). Gated on: a
parts-materialization engine (spec+core tranche), and T4's view architecture.
**Foreclosure guard:** T1b introduces `EditorDocument` and keeps save/bundle
ownership OFF `EditorSession`'s public shape (Ruling D condition); T4's canvas
takes a `ResolvedLayoutIR` it is *given* rather than owning the pipeline, so
N views are N canvases over one document.

### 3.3 Interchange: MusicXML, MIDI, MEI, and adoption

Nobody migrates to a notation editor that cannot read their existing scores.
MusicXML import/export (and MIDI in/out) is adoption-critical for any
"next MuseScore" claim, and it is absent from every current plan. It is its
own future track — importers/exporters as separate crates behind seams,
exactly the `render-svg` pattern, with import producing *operations* (a
MusicXML file becomes a minted op log over a genesis, so an imported score is
a first-class document with provenance, not a foreign object). Alongside
MusicXML and MIDI: **MEI**, the academic/archival standard — a spec-driven,
deterministic, microtonal platform's natural early adopters are exactly MEI's
community — and **MTS export** (MIDI Tuning Standard), which the spec already
names on the import side (`core_spec.tex:3333`).

A sequencing fact the tranche ladder hides: **interchange is headless and
parallelizable.** An importer consumes `epiphany-ops`/`-core`/`-textproj` and
never touches the editor ladder — a MusicXML→op-log converter can be built as
its own parallel track well before T4/T5, gated on model coverage (§3.7) and
the same genesis-persistence decision as T1b, not on the editor. And a
*partial* importer (notes, rhythms, meters, keys) is already the evaluation
gateway prospective users need; import completeness then tracks model
completeness, honestly reported per element.
**Foreclosure guard:** Ruling B's op-log-is-the-document semantics is
precisely what makes import-as-minting possible. Named here so the product
roadmap never treats it as an afterthought.

### 3.4 Collaboration — the differentiator MuseScore cannot retrofit

Stated precisely (this section's first draft overclaimed): **the operation-set
convergence substrate exists** — canonical concurrent reduction converges
byte-identically in any delivery order, and that substrate is what no
incumbent can bolt on. What does *not* yet exist is the editing layer above
it: the session's commit gate **rejects any non-clean reduction**
(`lib.rs:1033`, `reduce.rs:604`) — a collaboration-hostile simplification,
since real collaboration requires conflict-bearing documents to remain open
and editable where safe; remote envelopes must apply **without entering local
undo history**; canonical-base/frontier reconciliation, equivocation and
unresolved-operation surfacing, and durable sync checkpoints with idempotent
retry all need design. Undo across a sync boundary is the known hard edge: a
committed op is never retracted by prefix-dropping (Fact 10, grow-only) —
that is the **undo-as-operation** design (compensating operations), gated on
this track.
**Foreclosure guard:** Ruling B writes **only applied units** into documents
— an envelope never reaches canonical state while its author considers it
undone — and every session mints under a fresh replica with counters that can
never collide (Ruling B identity rule). T1b's save semantics are the
async-collab substrate; that is why they get a full ruling rather than an
implementation detail. Ruling B's open-policy matrix keeps conflict-bearing
documents *openable* (read-only, status surfaced) rather than rejected.

### 3.5 Command architecture, scripting, and Chapter-11 extensions

MuseScore has hundreds of commands, keybindable and scriptable; Chapter 11
already declares extension points and keeps the plugin runtime product-side.
The seam's intent methods (`delete_selection`, `insert_note_at`, …) are the
right primitives, but the app must not hardcode toolbar→method calls the way
the demo does: T4 builds a **command registry** — every action a named,
introspectable, keybindable command over seam intents — which is also the
future scripting/plugin surface and the macro/repeat substrate. When the
plugin runtime arrives, the anticipated substrate is **WASM** —
language-agnostic, sandboxable, content-addressable like everything else in
the platform — and the permission/isolation model ships **with the first
plugin, not after it**: edit barriers already exist in the editor
(`editor-core/src/barriers.rs`), and the runtime must honor them from day one.
**Foreclosure guard:** T4 starts with the registry; nothing in T1–T3 needs to
change, because headless intents are exactly what commands wrap.

### 3.6 Accessibility, internationalization, and performance budgets

MuseScore 4 made screen-reader accessibility a headline feature; a successor
cannot regress it, and it cannot be retrofitted onto a toolkit that lacks an
accessibility tree — nor onto a canvas that has a tree but exposes nothing
meaningful through it (§Ruling A criterion 4). Likewise the spec's Chapter 10
performance budgets exist as types with the reference suite deferred —
editing latency should eventually be gated, not vibes-checked, and gated
**per stage** (Fact 8: reduce / engrave / scene-build / paint), because an
end-to-end number cannot attribute blame. Text-heavy surfaces assume
translation from the start. And one differentiator worth naming: a
**Music Braille translator** — no major notation tool has first-class
Braille score output. Its input is the **materialized `Score`** (or a
dedicated semantic projection of it), NOT the Text Projection: the TP
serializes document identity and envelope history
(`text_projection.tex:957`), and two different histories can materialize the
same score — Braille output must not vary with edit history. The TP can be a
transport; it is not the semantic source. Named as an accessibility-track
deliverable, unscheduled.
**Foreclosure guard:** accessibility semantics, text shaping, and staged
latency are **hard T4 spike criteria** in Ruling A, not tiebreakers.

### 3.7 Model, engraving, and IR completeness — the verified registry

Fact 9's posture made concrete. Verified against source this pass; these are
**coordinated spec+core/IR tranches in the Push-4b mold** — the editor ladder
consumes them and never blocks on them — and the sequencing currency is the
**schema-major budget** ("a major is a budget to spend deliberately",
`PLAN_PUSH4B_TUNING.md`), not editor tranche numbers:

* **Genesis persistence (T1b blocker i, §Ruling B):** how genesis-only graph
  data — instruments, canvas — is canonically persisted and travels with a
  document. The one registry item that *does* gate an editor tranche.
* **Articulations, dynamics, ornaments are empty wire types.**
  `ArticulationMark` / `DynamicMark` / `OrnamentMark` are unit structs
  (`event.rs:49-57`) that already encode — giving them real payloads is a
  **schema major**, to be batched deliberately together with their engraving
  semantics (placement, collision, spacing impact), not dribbled out.
* **`NoteheadShape` is cited but defined nowhere** — the spec's
  `NoteDecisions` carries the field (`core_spec.tex:9603`) but no enum
  definition exists in spec or Rust. Gates percussion (X, diamond), early
  music (void, rhombus), and contemporary heads.
* **Percussion is modeled but unmapped.** `UnpitchedEvent` exists with
  `UnpitchedMemberId(pub u32)` opaque (`event.rs:45,127`); the member →
  staff-position → notehead-glyph → sound mapping that makes drums editable
  does not. Pairs naturally with the `NoteheadShape` tranche.
* **Grace notes: modeled, never engraved.** `GraceKind` is carried by both
  pitched and unpitched events (`event.rs:91,114,137`); `epiphany-engrave`
  contains no reference to grace at all. Spacing/slur/stem consequences make
  this an engraving-track item.
* **Figured bass and tablature are absent from the spec entirely** (verified:
  zero mentions). Chapter-scale model decisions, sequenced by product
  priority — named so their absence is a decision, not an oversight.
* **The version-aware envelope decoder (T1b blocker ii, §Ruling B):** the
  migrate-on-read API in `epiphany-ops` the format already requires
  (Fact 10) — also a prerequisite for any importer/exporter that reads real
  documents (§3.3).
* **The layout-IR readiness tranche (T4 prerequisite):** explicit per-system
  **primitive ownership** (or deterministic per-system primitive ranges) for
  the flat `glyphs`/`strokes`/`curves` arrays (Fact 2); a **shared typed
  glyph-asset seam** (the Bravura outline table is private to `render-svg`
  and stores SVG `d` strings — a canvas needs typed vector paths from a
  shared crate); and **text-run primitives** in the resolved IR — titles,
  lyrics, chord symbols, rehearsal marks, instrument names — with shaping,
  fallback, and metrics consistent across canvas, SVG/PDF export, hit
  testing, and accessibility (Ruling A criterion).
* **The engraving-quality track:** the Standard-tier solver and real
  quality-metric computation (Chapter 9's nine axes) — repeatedly deferred,
  and the actual gate on "professional engraving" claims.
* **Graphic/aleatoric notation is NOT on this list** — the model is already
  real (`GraphicEvent`, `IndeterminateEvent` implemented and encoded,
  `codec.rs:44`; `EventOrderingDAG` + time-brackets specified,
  `core_spec.tex:2672,2719`). What it lacks is product surface: drawing tools
  (a Ruling-A spike criterion) and editor intents. It is a differentiator
  waiting on the canvas, not on the spec.

---

## 4. Rulings

### Ruling A — interactive rendering architecture — **GRANTED 2026-07-23 (as amended)**

**The product canvas paints vectors directly from `ResolvedLayoutIR`; the SVG
string path is demoted to export and goldens.** Concretely: tessellate typed
glyph paths and the IR's strokes/curves straight to the GPU surface; cull by
viewport; re-tessellate only damaged **systems** — the same unit incremental
engraving will later re-solve (§3.1), so paint damage and engrave damage share
a boundary. The interactive path never serializes to an SVG string. The canvas
consumes a `ResolvedLayoutIR` it is *given* (§3.2's guard), so part views and
split views are more canvases, not more pipelines.

**Prerequisites (before or at T4 open, from §3.7):** IR per-system primitive
ownership; the shared typed glyph-asset seam; the text-run primitive decision.
These are IR/render tranches this ruling *depends on*, not work it smuggles in.

What this ruling does **not** pin: the toolkit/tessellation stack. The T4
spike decides it, bounded by these recorded criteria:

1. **Fill correctness:** `epaint`'s filled-path tessellator does not implement
   even-odd/nonzero fill for paths with holes — and glyphs have holes — so
   "just egui shapes" is likely out; the candidates are lyon-tessellated
   meshes inside egui, or a vector renderer (e.g. Vello) behind a window
   shell.
2. **Staged latency, not end-to-end latency:** reduce / engrave / scene-build
   / paint measured **separately** on large multi-system fixtures against
   Chapter-10-style budgets (Fact 8). A toolkit verdict from an end-to-end
   number is uninformative while reduction or solving dominates; the spike
   measures the stages the toolkit actually owns.
3. **Text pipeline (hard criterion):** shaping, font fallback, bidi/complex
   scripts, and metrics consistent between interactive canvas, SVG/PDF
   export, hit testing, and the accessibility tree. A stack with no credible
   text story is disqualified regardless of vector performance.
4. **Accessibility semantics, not toolkit support (hard criterion):** an
   accessibility tree (e.g., AccessKit) is necessary but not sufficient — the
   spike must demonstrate **one score fragment exposing meaningful semantics
   through it**: labeled notes/rests/measures, focus movement, navigation,
   selection state, and command activation. A toolkit can carry a tree while
   the custom canvas remains unusable to a screen reader; that outcome fails
   this criterion.
5. **Vision-critical interactions prototyped, not assumed:** an overlay layer
   suitable for collaborative presence cursors (§3.4); freehand/shape input
   for graphic-region editing (§3.7); touch. These are what a wrong toolkit
   forecloses.
6. The demo's egui pin is 0.29 (0.35 redesigned the `App` trait); whether the
   app crate starts on modern egui, iced, or a Vello surface is the spike's
   call under criteria 1–5.

SVG (`render-svg`) remains the export format, the golden format, and a
correctness cross-check for the T4 canvas — as **defined geometry/scene
equivalence plus a bounded visual differential under a controlled backend**,
NOT pixel equality: a GPU tessellator legitimately differs from `resvg` in
antialiasing and curve flattening while being geometrically correct.
Pixel-exact comparison is reserved for Ruling C, where both sides are the
same SVG/resvg pipeline.

### Ruling E — the clipboard fragment projection — **GRANTED 2026-07-23**

**A versioned s-expression fragment format, values-only, paste-as-minting.**
The T2 ruling the ladder reserved, now drafted:

* **Header and versioning:** `(epiphany-fragment (major minor patch))`,
  starting `(0 1 0)`. An unrecognized major is **rejected**, never partially
  parsed. The fragment format is application-level and versioned — it is NOT
  canonical wire format, opens no schema major, and may evolve.
* **Values, never identities.** A fragment reuses the Text Projection's
  *value/leaf productions* (pitches, durations, spellings — the ratified
  textual forms) but NOT its document grammar: no document id, no envelopes,
  no causal contexts, no object ids. Content is per-event **values** plus a
  **rational onset relative to the fragment origin**, in per-voice lanes
  keyed by ordinal (not `VoiceId`). Paste **mints fresh operations**
  (`InsertEvent` + `RespellPitch` transactions, fresh ids from the session's
  minters) — the §3.3 import-as-minting principle at clipboard scale.
* **Closure, v1 (fail closed, report dropped):** notes/rests with their
  per-event attachments copy; a slur copies iff **both** endpoints are inside
  the range, else it is dropped and reported; a **partially-selected tuplet
  refuses the copy** (the reducer's own refusal discipline); a tie cut by the
  range boundary is dropped and reported; derived state (decomposition,
  spellings that are merely inferred) is never copied — it re-derives.
* **Placement:** `paste_at(point, &grid)` (pencil-style, via `position_at`)
  and `paste_over_selection()` (at the anchor member's onset, in its voice) —
  both with **make-room overwrite** semantics reusing `make_room`, and both
  atomic transactions (a refused member rolls back the whole paste).
* **Untrusted input:** hard caps on bytes, event count, and nesting depth,
  each a named constant with a value-asserting rejection test; unknown
  version → clean error. Fragments arrive from the OS clipboard; they are
  input, not trusted state.

Granting this ruling unblocks T2's W4 packet (copy/paste); W1–W3 do not
depend on it.

### Ruling B — the document layer and persistence semantics — **REDRAFTED ×2 — NOT grantable yet**

*(First draft withdrawn for violating the grow-only operation set. Second
draft amended by the 2026-07-23 second review. Two blockers stand between
this ruling and grant; both are outside the resolver-parallel blast radius,
which is why T1b sequences after the resolver.)*

**Blocker (i) — canonical graph-state persistence, across genesis AND
pruning.** Two halves of one question. *Genesis:* "every piece of content
enters as operations" is false for more than instruments — genesis is
outside the operation set by ratified design (`binary_format.tex:2420`),
`CreateStaff` preconditions a live instrument (`reduce.rs:3792`), and the
`Score` root also carries staff groups, parts, tuning context, tempo map,
analysis layers, and views (`graph.rs:1693`), for which the operation
vocabulary (`payload.rs:120`) has incomplete or no construction coverage —
resolving instruments alone could still leave parts, staff groups, custom
tuning, analysis layers, and views unsavable. *Pruning:* the canonical-base
payload is `MaterializedState` — reducer bookkeeping without graph values —
while pruning may remove the covered blocks that carried those values
(Fact 10): after pruning, canonical roots alone cannot reconstruct an editor
`Score`, which also undermines any "base-plus-tail" reading of §3.1/T4b
until the checkpoint payload can rehydrate graph content.

Before choosing "persist genesis" vs "add operations", the resolution
requires a **field-by-field table over the `Score` root**: field; canonical
default; existing create/modify/delete coverage; whether arbitrary imported
values are representable; and whether the field belongs in genesis, the
operation log, or derived state. Resolution, one of: **(a)** a coordinated
spec/core/ops/format decision on canonical graph-state persistence (the
recommended path — it also unblocks import, §3.3, and T4b's checkpoint); or
**(b)** an explicitly scope-limited T1b (empty / metadata / region
persistence only), with the full table named as the prerequisite for
*useful* documents. The T1b contract opens with this disposition.

**Blocker (ii) — versioned decode.** Reopening any document whose blocks
carry older schema majors requires migrate-on-read
(`binary_format.tex:2453,2532`); `decode_envelope` is current-layout-only
(`envdecode.rs:622`). A "written-by-the-current-writer" restriction is
**unenforceable**: minimal stamping means a current writer legitimately
emits major-0, -1, and -2 blocks by content (`payload.rs:200`), and the
manifest carries no writer provenance — rejecting lower majors would reject
T1b's own files. Resolution, preferred: the **version-aware
decoder/migration API in `epiphany-ops`** (reads driven by each block's
`schema_version`; its own tranche, §3.7), with mixed-major reopen tests,
**mandatory before general `EditorDocument::open`**. The only honest interim
restriction, if T1b proceeds first: *T1b supports only envelope bytes
decodable under current layouts; recognized historical layouts receive a
distinct `UnsupportedHistoricalEncoding` error* — enforceable, limited, and
stated in the contract. Writes in every case stamp blocks via
`operation_block_versioned` with the maximum payload major in the block
(`bundle.rs:111`).

**The ruling (as it stands, pending the blockers):**

* **Ownership and the save protocol (the Ruling D condition):** T1b
  introduces `EditorDocument`, owning the bundle handle, the committed
  operation set, the **committed generation**, read-only status, identity
  allocation, and extension + anomaly state. T1b grants **one writable
  session lease per document, and the lease is unforgeable in the type
  system** — a bare `save(&mut EditorSession)` is rejected as a design,
  because a probe-mode session, a session leased from another document, or a
  session based on a different committed generation could otherwise be
  persisted into the wrong bundle as a validly encoded but semantically
  unrelated document. The contract picks one concrete shape —
  `WritableEditorSession<'doc>`, a `DocumentSession` guard owning
  `&mut EditorDocument`, or a private lease token carrying document identity
  + committed generation that `save` validates — and specifies **how the
  session materializes committed + local operations after promotion** (an
  immutable shared reference to the committed set, or a coordinator; the
  document exclusively owns the mutable committed state either way). Save
  commits the leased session's staged units and, **only after a successful
  commit and durability barrier, atomically promotes exactly those units**
  from the session's undoable state into the immutable committed partition
  (`lib.rs:312` ties every active envelope to an undo unit — promotion is a
  partition move, never a clear-while-active). **Dirty state lives with the
  session/lease**, not the document. The redo stack's fate at the save
  boundary is specified and tested in the contract (redo units reference
  unsaved envelopes by construction; the contract proves whether they remain
  valid or are cleared). A multi-session coordinator is §3.2's future, not
  T1b's.
* **Documents arise from genesis** (as constrained by blocker i). The
  canonical document is the operation log reduced over the document's
  genesis; `EditorDocument::create` mints all post-genesis content as
  operations. Opening an arbitrary in-memory `Score` (the demo fixture path)
  remains supported as a **probe mode and is not savable**: a score whose
  construction is in neither genesis nor the log cannot be a canonical
  document. Any further construction gap discovered en route fails closed
  and is filed, not papered over.
* **Save appends; a successful save is an undo boundary.** Save commits
  exactly the units staged at save time as new operation blocks. The
  post-save invariant (committed history is a *set*, `manifest.rs:465` — a
  positional-prefix claim was withdrawn): **committed_after =
  committed_before ∪ local_applied_at_save**, and every promoted local unit
  leaves local undo without ever leaving canonical reachability
  (authored−applied is a non-contiguous *subsequence* after forks,
  `lib.rs:326`). Undo never crosses a save: undoing a committed envelope
  requires a *compensating operation* (undo-as-operation, §3.4), never
  removal (Fact 10). No manifest re-referencing, no block repacking.
* **Commit outcomes are three-way, and reconciliation must prove
  durability, not visibility.** Failure *before* the commit point: units
  remain unsaved and undoable. Success: units are promoted.
  **Indeterminate commit-point flush** (the bundle poisons itself read-only,
  `bundle.rs:601,744`): the failed barrier was `sync_all` itself
  (`store.rs:194`), so a generation readable after reopen may be kernel
  cache, not durable state. Reconciliation therefore: (1) reopen and
  identify the selected generation; (2) confirm whether the exact staged
  envelope set is reachable; (3) execute a **fresh durability barrier** and
  require it to succeed *before* reporting saved and promoting units; (4) if
  that barrier fails, remain in an indeterminate / recovery-required state —
  never declare saved *or* unsaved. The injected-fault tests distinguish
  "visible after reopen" from "confirmed durable".
* **No snapshots in T1b — of either role.** The `canonical_base` role is
  written only by pruning (none in T1b). Acceleration snapshots are
  **omitted entirely**: as reviewed, validating one against full re-reduction
  on load delivers no fast open (replay stays on the cold path), using one
  before validation displays untrusted state, selection/retention among
  multiple snapshots is unspecified, and the `SnapshotId` stand-in must not
  be productized as a derivation. Reopen is full replay; fast open joins
  §3.1 when a real validation strategy exists.
* **Reopen preserves causal history.** Opening a document loads the stored
  envelopes as an immutable **committed partition** of the session's
  operation set — a *logical* partition, not a chronological prefix
  (manifest roots are deduplicated sets, `manifest.rs:465`). Materialization
  reduces committed + session ops together, and the first new edit's causal
  context **extends the stored frontier** (an empty context would author an
  edit causally concurrent with the very state on screen; a claimed frontier
  without predecessors present would strand the edit pending —
  `lib.rs:909,1021`). **No frontier is stored** (T1b writes no base and no
  snapshots), so the frontier is **derived exactly from committed
  membership**, honoring the DVV model (`causal.rs:1`): the vector floor
  covers only the *contiguous* counter prefix per replica; operations beyond
  a gap become individual dots. Required frontier-builder test: committed
  {(r,0), (r,2)} → floor `r:0` + dot `(r,2)`, never floor `r:2` —
  undo/forking makes counter gaps routine. Undo/redo operate strictly on
  session units. Full-log
  re-reduction cost at scale is Fact 8's known debt; the
  reducer-from-base-plus-frontier API is §3.1's tranche, not T1b's.
* **The open-policy matrix (every `Bundle::is_read_only()` cause
  propagated), with two distinct read-only outcomes:** a
  **`PreservedDocument`** (metadata/bundle access only, NO materialized
  editor session — the UI must never imply the score was faithfully
  displayed) versus a **read-only `EditorDocument`** (canonical state
  understood and rendered; editing prohibited). Hard byte corruption → open
  error; unsupported canonical operation major → **`PreservedDocument`**
  (its operations cannot be decoded into a score); bundle / profile anomaly
  → preserved read-only at the strongest level honestly claimable; semantic
  conflict, pending dependency, or equivocation in reduction → **read-only
  `EditorDocument` with surfaced status** until collaboration support lands
  (§3.4) — never a rejection of a valid document; identical duplicate
  envelopes across blocks → accepted as set duplicates, not corruption.
* **Identity: fresh by default.** Every session mints under a **fresh random
  replica** (the v0 `getrandom` convention). Resuming a prior replica is
  **not offered in T1b**: a max-counter scan cannot see orphaned blocks,
  older manifests, backups, or a concurrently open session on the same
  replica — durable resumption needs high-water storage with atomic
  counter-range reservation, covering operation *and* entity ids, deferred.
  Entity-id minting scans the committed partition + the session's authored
  log, preserving the never-re-mint guarantee (`lib.rs:326`) across reopen.
* **Single writer: lock before anything destructive.** `FileStore::create`
  truncates at open (`store.rs:152`) — so creation uses `create_new`, and
  overwrite is open-without-truncate → **acquire the exclusive advisory
  lock** → validate overwrite authorization → truncate. Lock denied is a
  defined result (read-only open or error, per caller intent). Generation +
  file-UUID are revalidated immediately before commit, with a defined
  **external-modification error** (surfaced, never silently overwritten or
  retried) — scoped honestly to **cooperating writers of the same file**: an
  atomic path replacement is invisible to a held inode and is out of scope
  unless the contract adds path-identity revalidation. Tests drive two
  independently opened handles and prove the second cannot truncate or
  mutate the first's document. This is `epiphany-bundle` work.
* **Extensions fail closed.** Barrier injection is the opener's job
  (`lib.rs:453`) and the durable tombstone encoding is deferred at the format
  level (Fact 10). T1b opens any bundle carrying extension declarations
  **read-only**; unsafe edits are refused on bundle-backed documents.
  Writable extension-bearing documents wait for the format tranche that
  supplies the tombstone channel — silently preserving invalidated extension
  data would violate a MUST.

Deferred, named so they are plans: undo-as-operation (§3.4); durable replica
resumption; pruning (and with it any `canonical_base` writing); acceleration
snapshots + fast open (§3.1); the reducer-from-base API (§3.1); the
multi-session coordinator (§3.2).

### Ruling C — the visual golden harness — **GRANTED 2026-07-23 (as amended and clarified)**

**Golden images of the resvg-rasterized score, as ordinary `#[test]`s in
`epiphany-editor-gui`, running in the existing `editor-gui` CI job — compared
as decoded pixels, not encoded files.** Design points:

* **What is locked:** the rasterized pixmap of `render()` output in
  `GlyphMode::PathOutline` at a fixed `px_per_staff_space` — the exact surface
  the GUI displays. `render-svg` already golden-locks SVG *bytes*; this locks
  the raster step and the edit loop's visible result (fixture as opened; after
  a scripted pencil insert; after undo — each state one golden). This is
  verification of the **score raster layer**; egui widgets and overlays remain
  outside it (below).
* **Comparison contract (amended 2026-07-23):** decode the committed PNG and
  compare **dimensions plus raw RGBA bytes** exactly — never the encoded PNG
  file, which would also lock encoder/compression behavior and can churn
  while every pixel is identical. On failure the test writes **actual,
  expected, and diff images** to the test-output directory and names their
  paths in the assertion message.
* **CI artifacts (clarified 2026-07-23):** assertion-message paths refer to
  the ephemeral runner and are **local-reproduction aids**; the reviewable
  record is an `if: failure()` `upload-artifact` step added to the
  `editor-gui` job — the one additive CI change (Fact 7). No effect on
  pinned counts or resolver safety.
* **The goldens are also the render-determinism diff gate:** any solver or
  renderer change that alters output pixels fails CI until the diff is
  reviewed and deliberately re-blessed — the editor-level analogue of the
  reduction-determinism criteria, and the review moment engraving changes
  deserve.
* **[granted 2026-07-23]** the multi-system casting-off fixture **is**
  goldened in T1a — it is the layout path real documents take, and the larger
  re-bless surface is accepted deliberately: whenever engraving improves, the
  diff there is exactly the review moment the goldens exist to create. One
  casting-off golden alongside the single-staff edit-loop states.
* **Determinism basis:** `PathOutline` mode uses no fonts, and
  `resvg`/`tiny-skia` are pure Rust with deterministic rasterization; CI and
  dev are both Linux. The project's re-bless discipline applies (never
  re-bless to make a test pass; a diff is a finding). If cross-platform drift
  ever appears, the fallback is a bounded per-pixel tolerance — recorded
  then, not pre-engineered now.
* **What is not covered, stated honestly:** egui widget interaction (toolbar,
  overlay painting, event routing) — the selection overlay and click plane
  remain unit-tested via `ViewMap`. Widget-level harnessing (`egui_kittest`
  exists for newer egui; the pin is 0.29) is evaluated in T4 when the toolkit
  is chosen, not before.
* **Promotion:** wiring these into the numbered conformance suite would move
  the 8/8 count the resolver contract pins — so promotion to a `[9/9]` gate is
  T2, explicitly.

### Ruling D — crate strategy, CI, and ownership — **GRANTED CONDITIONALLY 2026-07-23**

**The product GUI is a new crate (working name `epiphany-editor-app`), created
at T4, not before; the demo stays as the seam probe until the app reaches
parity, then is retired by explicit decision.** T1–T3 need no new crate: their
work lands in `editor-core` (seam + document layer), `epiphany-bundle`
(single-writer enforcement), and `editor-gui` (probe + goldens). When T4
creates the app crate it joins the MSRV-exclusion set and gets its own
pinned-stable CI job, mirroring `ci.yml:109`'s existing pattern; it is built
around the §3.5 command registry from its first commit. The demo is never
grown into the product.

**Grant condition (held through all three reviews):** the T1b contract
specifies the **document/session ownership API** — `EditorDocument` owning
bundle, committed state and generation, read-only status, identity,
extension + anomaly state; the leased session owning undo, dirty state,
selection/caret, solver/layout; save as a document method over an
**unforgeable document-bound session type or lease token** (§Ruling B — a
bare `&mut EditorSession` does not satisfy the condition), with the atomic
post-commit-and-durability promotion protocol — *before* any implementation.
The ruling is granted contingent on that API appearing in the contract and
surviving its review.

---

## 5. Parallel-safety contract with the resolver tranche

While `CONTRACT_PUSH4B_RESOLVER.md` is in flight (it is, as of `b0acacb` —
uncommitted resolver work sits in `epiphany-core` in this working tree),
**only T1a runs**, and it:

* touches only `crates/epiphany-editor-gui/**`, `.github/workflows/ci.yml`
  (the single additive `if: failure()` artifact-upload step in the
  `editor-gui` job), and `spec/*.md` additions — no `epiphany-core`, no
  `.tex`, no `DECISIONS.md` but this track's own future files;
* adds no requirement labels (counts stay 212 / 282 / 282), promotes no
  conformance gate (suite stays 8/8); **adds new `editor-gui` PNG baselines
  but modifies/re-blesses no existing golden and changes no fuzz digest** —
  keeping the resolver's zero-churn tripwire unambiguous;
* lands only green (`cargo test --workspace` clean at every landing) — the
  resolver's gate runs workspace-wide and must never be reddened from our
  side;
* whoever lands second rebases; both contracts state the same frozen numbers,
  so a spurious count movement is detected by either side.

T1b's surfaces (`editor-core`, `epiphany-bundle`, `epiphany-ops` for the
versioned decoder) wait for the resolver to land regardless of file overlap —
its blockers, not just courtesy, set that order.

---

## 6. Review ledger

House-ratification style: one line per finding, disposition and location.

### First review — 2026-07-23, 14 findings, all accepted

| # | finding (short) | disposition |
|---|---|---|
| 1 | Saved envelopes cannot be re-referenced away (grow-only, `:6924`; pruning dominance, `:11536`) | **Ruling B redrafted**: save = undo boundary; undo-as-op stays §3.4 |
| 2 | Snapshot payload/role wrong; genesis gap (major-0 base, `bundle.rs:857`; accel role, `roundtrip.rs:339`) | **Ruling B redrafted**: roles corrected; genesis question later became blocker i (second review) |
| 3 | Empty logs on reopen lose causal history (`lib.rs:909,1021`) | **Ruling B redrafted**: committed partition; contexts extend stored frontier |
| 4 | Replica-resume can reuse ids | **Ruling B redrafted**: fresh random replica default; resumption deferred behind durable reservation |
| 5 | Extension barriers/tombstones unhonorable in T1 (`lib.rs:453`, `bundle.rs:678`) | **Ruling B**: extension-bearing bundles open read-only; fail closed |
| 6 | Active-prefix rewrite ignored forks + block granularity (`lib.rs:326`, `block.rs:98`) | Dissolved by finding-1 redraft; saved-set-is-a-prefix invariant recorded |
| 7 | FileStore is not a concurrent-writer protocol (`store.rs:141`) | **Ruling B**: lock + revalidation + two-handle tests; tightened again by second review #7 |
| 8 | IR primitives are flat, not system-owned (`resolved.rs:86-100`); glyph seam private | **Fact 2 corrected**; IR-readiness tranche in §3.7; Ruling A prerequisite |
| 9 | Rendering ruling omitted general text | **Ruling A criterion 3** (hard); text-run primitives in §3.7 |
| 10 | Performance sequencing circular | **Fact 8 amended** (staged budgets); **T4b added** as app exit criterion |
| 11 | Document/session/view asserted, not designed | **Ruling D grant condition**; protocol specified by second review #4 |
| 12 | Collaboration section overclaimed (`lib.rs:1033`) | **§3.4 narrowed** + itemized remaining work |
| 13 | PNG goldens must compare decoded pixels | **Ruling C amended** and granted |
| 14 | Status/roadmap wording | Header restated per-ruling; §3 reframed as risk map |

### Second review — 2026-07-23, 11 findings, all accepted

| # | finding (short) | disposition |
|---|---|---|
| 1 | **Blocker**: no usable document from empty genesis (no `CreateInstrument`, `binary_format.tex:2420`; live-instrument precondition, `reduce.rs:3792`) | **Ruling B blocker i**; T1 split — T1a now, T1b post-resolver, contract opens with the genesis disposition |
| 2 | **High**: reopen needs version-aware envelope decode (`envdecode.rs:622` vs `binary_format.tex:2453,2532`; `bundle.rs:111`) | **Ruling B blocker ii**; migrate-on-read tranche named in §3.7, or honest current-major restriction |
| 3 | **High**: clipboard ruling named a format that does not exist (`text_projection.tex:957`) | **T2 entry redrafted**: versioned *fragment projection* is a T2 ruling; paste mints fresh ops |
| 4 | **High**: save/dirty ownership protocol unanswered (`lib.rs:312`; roots are sets, `manifest.rs:465`) | **Ruling B + D condition**: single writable session lease; `save(&mut EditorSession)`; atomic post-commit promotion; dirty on session; "committed partition" language |
| 5 | Conflict-bearing documents need an open policy (`reduce.rs:604`) | **Ruling B open-policy matrix**; all `is_read_only()` causes propagated |
| 6 | Acceleration snapshot delivers no fast open; `SnapshotId` stand-in must not be productized (`binary_format.tex:698`) | **Ruling B**: T1b writes no snapshots; fast open joins §3.1 |
| 7 | Lock must precede destructive open (`store.rs:152`); path replacement undetectable | **Ruling B single-writer bullet tightened**: `create_new`, lock-then-truncate, scoped external-modification claim |
| 8 | Indeterminate commit-point flush (`bundle.rs:601,744`) | **Ruling B commit-outcome bullet**: three-way protocol + reopen-and-reconcile + injected-fault tests |
| 9 | T4 must not claim pixel equality with resvg goldens | **Ruling A closing amended**: geometry/scene equivalence + bounded visual differential |
| 10 | Golden diff files vanish in CI (`ci.yml:109` uploads nothing) | **Ruling C + Fact 7**: `if: failure()` artifact upload — the one additive CI change |
| 11 | Accessibility criterion must test semantics, not toolkit support | **Ruling A criterion 4 rewritten**: score fragment with labels, focus, navigation, selection, activation |

### Third review — 2026-07-23, 9 findings, all accepted

| # | finding (short) | disposition |
|---|---|---|
| 1 | **High**: blocker (i) scoped too narrowly — `Score` root carries staff groups, parts, tuning, analysis layers, views (`graph.rs:1693`) | **Blocker (i) widened** to canonical graph-state persistence; field-by-field table required before choosing a resolution |
| 2 | **High**: `MaterializedState` cannot restore the graph after pruning (`reduce.rs:499`, `core_spec.tex:11553`) — undermines base-plus-tail | **Blocker (i) widened** to cover pruning; **T4b renamed** to checkpointed-reducer-state-plus-tail; Fact 8/§3.1 caveated |
| 3 | **High**: "current-major-writer-only" fallback unenforceable under minimal stamping (`payload.rs:200`) | **Blocker (ii) rewritten**: version-aware decoder mandatory before general open; interim = current-layout-decodable only, `UnsupportedHistoricalEncoding` for recognized historical layouts |
| 4 | **High**: `save(&mut EditorSession)` enforces neither lease nor document binding; prefix invariant wrong | **Ruling B ownership bullet rewritten**: unforgeable document-bound session type/token; post-promotion materialization specified; invariant → committed_after = committed_before ∪ local_applied_at_save; **Ruling D condition strengthened** |
| 5 | **Medium-high**: reopen visibility ≠ durability after failed `sync_all` (`store.rs:194`) | **Commit-outcome bullet rewritten**: fresh durability barrier required before promotion; tests distinguish visible from durable |
| 6 | Unsupported-major preservation ≠ an editor session | **Open matrix split**: `PreservedDocument` vs read-only `EditorDocument` |
| 7 | No stored frontier; max-counter derivation invalid under gaps (`causal.rs:1`) | **Reopen bullet**: frontier derived exactly from committed membership — contiguous floor + dots; {(r,0),(r,2)} test required |
| 8 | §5 "moves no golden" misleading — T1a adds baselines | **§5 reworded**: adds new PNG baselines; re-blesses nothing existing |
| 9 | Braille must consume materialized semantics, not TP history; clipboard fragments are untrusted input | **§3.6 rewritten** (materialized `Score` as input); **T2 fragment ruling** gains limits + unknown-version rejection |

---

## 7. Next

**Dispatch `CONTRACT_EDITOR_T1A_GOLDENS.md` now** — Ruling C is granted,
T1a's surfaces are resolver-safe (§5), and nothing in it waits on Ruling B.
The T1b contract is drafted **after the resolver lands**, opening with, in
order: the graph-state-persistence disposition with its field-by-field
`Score` table (blocker i), the versioned-decode disposition (blocker ii),
and the Ruling-D ownership API over an unforgeable document-bound session
type — then Ruling B's grant is re-sought against that concrete contract. T2+ contracts are written
only when their tranche opens; the §3 tracks get plans of their own when
their gating work exists.
