# Contract: Genesis G2b — `SetTuningContext`, and the accept-set raise it pays for

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/RULING_GENESIS_PERSISTENCE.md`, `spec/PLAN_GENESIS_OPS.md` §4, and
`spec/PASS13_CANDIDATES.md` (P13-S13, which **closes** here).

**One operation, one kind, one tag, one accept-set raise.** G2 was split
precisely so this surface could be isolated: minimal stamping is a pure
function of each payload's value, and `ScoreTuningContext` is the **only**
genesis payload born at schema major 3. The raise is charged here, not
amortised across nine surfaces as the ruling's framing first implied.

**Prerequisite, already met:** G-minor landed at `ff9bd0f`. This contract
appends kinds/tags **34**, which is why it sequences after that sweep — running
it first would have shipped 34 with the very defect G-minor retired.

**Parallel safety.** The editor track owns `crates/epiphany-editor-gui/**`,
`crates/epiphany-render-svg/**`, `crates/epiphany-glyphs/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, `spec/PLAN_EDITOR_APP.md`,
every `spec/CONTRACT_EDITOR_*.md`, `spec/DRAFT_T4_FIXTURE_RECIPE.md`,
`spec/ANALYSIS_*.md`, `crates/epiphany-editor-gui/goldens/*.png`, and the entire
`spikes/` tree. **All out of scope. Stage only the files this contract names.**

**One-time authorization, not generalising:** this packet **will** need to edit
`crates/epiphany-editor-core/src/barriers.rs` (`subjects_of`) and
`crates/epiphany-layout-ir/src/barrier.rs`. Rust exhaustiveness forces the
first, and the second holds a "one past the vocabulary" literal. **Budget both
up front** — this is the G1 lesson, where an unbudgeted crossing blocked the
entire gate because `epiphany-testkit` depends on `epiphany-editor-core`.

---

## 1. The pin, RESOLVED — subset, not normalization

**This was the open pin. It is resolved here, in the contract, not left to the
implementation.**

### The hazard

`ScoreTuningContext` has six fields, but its `Codec` **deliberately encodes
five** (`core/src/codec.rs:1939`): `default_pitch_space`,
`default_tuning_system`, `reference`, `smufl`, `overrides`.
`accidental_extensions` is staged out of schema major 3 and `dec` reconstructs
it as `Vec::new()` (`graph.rs:1676` documents it "**In memory only**").

A naïve full-value `SetTuningContextOp(ScoreTuningContext)` therefore diverges:

* **Live session** — `OperationSet::accept` stores the envelope as a **value**
  (`opset.rs`, `OperationSlot::Single(env)`), so reduction applies
  `accidental_extensions` intact.
* **Same document reloaded** — the envelope round-trips through bytes, the
  field decodes as empty, and reduction applies an empty one.

Same document, two graph states, depending only on whether you just authored it.

**`canonical_value!` structurally cannot catch this.** Its generated
`decode_canonical` does decode → `finish()` → re-encode → reject-on-mismatch,
comparing **bytes** and never the originating value. A field that never reached
the bytes is invisible to it. This is not a gap to plug; it is what that macro
is.

### The ruling: **subset**

`SetTuningContextOp` carries a **new payload type holding exactly the five
wire-bearing fields**, in the codec's existing order. Reduction writes those
five onto `score.tuning_context` and **leaves `accidental_extensions`
untouched**, preserving whatever the graph already held.

Four reasons, in order of weight:

1. **The wire bytes are identical either way, so this costs no layout design.**
   `ScoreTuningContext::enc` already encodes exactly those five fields in
   exactly that order. The subset type's canonical encoding is byte-for-byte
   the current one. This is a **type-level narrowing, not a new wire form** —
   `canonical_value!` still applies and the G1/G2a payload template is unchanged.
2. **It makes the divergence unrepresentable.** Normalization would make
   correctness depend on remembering to clear a field at every construction
   site, enforced by nothing the compiler or the codec can see — and this track
   has now been burned twice by exactly that shape (four stale literal sites at
   Push 4a, six found during G2a). A field that does not exist cannot be set
   wrongly.
3. **Preserving is semantically right; clearing is not.** The operation carries
   no information about `accidental_extensions`. Normalization would make
   "set the tuning context" silently *erase* a live in-memory registry
   extension — an effect the author never asked for and the wire cannot even
   record.
4. **The forward path is identical.** When a later schema major lands
   `accidental_extensions` on the wire, the subset type gains the field: a
   normal major payload change, the same cost normalization would have paid.

**Rejected: normalization** (clear the field at construction) — fails 2 and 3.
**Rejected: reject-on-non-empty** (`well_formed` refuses a populated field) —
turns an in-memory-only field into an authoring error for callers who never
opted into persistence, and still needs the check maintained by hand.

**Evidence this is low-risk today:** nothing in production populates
`accidental_extensions` — every non-`Vec::new()` reference in the tree is a
test (`codec.rs:3725`, `textvalue_graph.rs:1092`). The **text projection drops
it too** (`textvalue_graph.rs:1123`), so it already survives no persistence
surface at all. The subset type ratifies the status quo rather than changing it.

**RATIFIED 2026-07-28:** the subset design, and the type's name and path —
**`epiphany_core::TuningContextSettings`**. It lives in `epiphany-core`
alongside the type it narrows, and is publicly exported at that exact path.

---

## 2. Design pins

### Pin 1 — kinds and tags are 34, in both spaces

`OperationKind::SetTuningContext` = **34** (`payload.rs`, the hand-written
`discriminant()` match at :275 — **the exact site Push 4a got wrong**) and
`OperationKindTag::SetTuningContext` = **34** inside
`operation_kind_tag_vocabulary!`. They coincide numerically here; that is a
coincidence, not an alignment. **Two rows, never one.**

### Pin 2 — `schema_major()` gains a real arm, unlike G2a

G2a's pin 3 said `schema_major()` gains **no** arm, because both setters were
major 0 and fell through the catch-all. **G2b is the opposite.**
`SetTuningContext` must return **3**, unconditionally — `ScoreTuningContext`'s
wire form is born at major 3 and its appends are mandatory, not `Option`-hidden.
This is the single fact the whole rung is charged for.

### Pin 3 — the accept-set raise, and the prose that must move with it

`max_supported_major(ChunkKind::OperationEnvelopeBlock)` goes **2 → 3**
(`bundle/src/bundle.rs:69`).

**The doc comment above it does not merely document the cap — it asserts a
claim this rung falsifies.** `bundle.rs:56-58` currently reads: *"Schema major 3
(Push 4b tranche 3b-i) does **not** raise this role: no operation payload embeds
the tuning context, so no op block is ever born at v3."* After G2b that sentence
is **false**. Rewrite it; do not leave it beside a changed number. A stale
rationale next to a correct constant is how `binary_format.tex:2373` happened.

### Pin 4 — G-minor interaction: kind 34 needs an epoch

G-minor's `introduced_minor()` is **exhaustive with no wildcard arm**, so
kind/tag 34 **cannot compile** without an epoch. That is the control working as
designed — do not defeat it.

**Epoch 10, for the genesis G2b event — RATIFIED 2026-07-28** and already
recorded in the authoritative ladder (`PLAN_GMINOR_SCHEMA_MINOR.md` §4).
**Transcribe it; do not edit that plan.** This is the first exercise of the
ladder's own growth path, and it stayed monotonic and prefix-closed.

### Pin 5 — undo restores the **seeded base**, and the never-authored /
authored-to-default distinction must stay **unobservable**

**An earlier draft of this contract had this backwards.** It required undo to
distinguish "never authored" from "authored to the default value".
`spec/PLAN_GENESIS_OPS.md` §5 trap 5 **withdrew** that on 2026-07-28, and
`SetMetadata` is the disproof: `Score::empty` seeds `metadata` exactly as it
seeds `tuning_context` (`graph.rs:1749`, `:1757`), and base ingest then runs
`metadata_chain.seed(score.metadata.clone())` (`reduce.rs:1385`) precisely so a
value-restoring undo of the *first* operational write restores the
pre-operational state.

**The rule:**

* From-empty reduces **onto** `Score::empty` (via `reduce_operation_set_onto`),
  so **the seed runs**. Undoing the first write yields
  `Restore(Some(Predecessor::Base(seeded)))`.
* **Undo restores the seeded base settings — default or non-default alike.**
  Restoring the seeded value is correct in *both* cases, so the distinction is
  unobservable, **and this contract must keep it so.** Writing code that can
  tell them apart is a defect, not a safeguard.
* **`Predecessor::Base` vs `::Write` earns its keep only for the canonical
  bookkeeping families** (`spellings`, `breaks`, `page_breaks`,
  `reduce.rs:707`), where a base predecessor returns a **map key to absence**.
  `ScoreTuningContext` is an **always-valued** `Score` field, like `metadata`:
  **there is no absent state to return to.**
* **Copy `set_metadata` structurally**, as all three G2 setters do. Do not
  invent a chain shape for this field.

The chain's value type is the **subset type**, not `ScoreTuningContext`:
`accidental_extensions` never participates in undo, because no operation ever
writes it and undo must leave it exactly as it found it.

### Pin 6 — P13-S13 closes here

Mark it resolved in `spec/PASS13_CANDIDATES.md`, citing this rung. Its closure
argument is the `metadata` precedent: the op log is canonical, and the canonical
base embeds no graph values for **any** field.

### Pin 7 — the four-document append ritual applies in full

**This is the G2a lesson and it is not optional.** An operation-vocabulary
append is a documented event in **four** specification documents:

* `binary_format.tex` — payload-layout table, tag table, **and** the §2373
  minor-additive bullet (now G-minor-repaired; kind 34 extends its history),
  version, Revision History row;
* `operation_catalog.tex` — a `\section`, version, changelog paragraph;
* `core_spec.tex` — the **normative** `OperationKind` *and* `OperationKindTag`
  listings, plus any spelled-out payload count;
* `text_projection.tex` — a kind production and **`COMPANION_VERSION`
  0.10.0 → 0.11.0**.

**The signature to grep for: a spelled-out count sitting next to an
enumeration that disagrees with it.**

### Pin 8 — the hand-maintained sites, budgeted up front

Five non-compiler-checked sites (`layout_stub`'s is now derived):

1. `ops/src/payload.rs` — `OperationKind::discriminant()`;
2. `layout-ir/src/barrier.rs` — the "one past the vocabulary" literal;
3. `testkit/tests/text_projection_grammar.rs` — a kind **count**;
4. `ops/src/textproj_kind.rs` — a kind **count**;
5. `testkit/src/generators.rs` — an `rng.below(N)` bound.

Plus `editor-core/src/barriers.rs`'s `subjects_of`, which the compiler forces.
**Prefer deriving over extending** wherever the site allows it.

### Pin 9 — explicit non-goal: G2b authorizes **no** pruning or compaction

**G2b does not authorize pruning or compaction of the canonical operation
log**, and nothing in this packet may implement, enable, or prepare either.

`spec/PLAN_GENESIS_OPS.md` §3 requires this stated outright, and the reason is
that G2b **sharpens** the standing prohibition rather than relaxing it. Before
this rung, pruning was a performance concern: it would have discarded
re-derivable state. **The moment G2b lands, pruning would discard *authored
genesis state*** — the tuning context exists canonically only in the op log,
because the canonical base embeds no graph values for any field.

The prohibition remains blocked on **disposition C** (the canonical base
carrying graph values). There is no `fn prune` in the tree today, which is
exactly why the prohibition is free now and would be catastrophic later. **If
the implementation finds itself wanting a prune or compaction step for any
reason, that is a finding to report, not a feature to add.**

### Pin 10 — P13-S15 stays open

The golden lock ends at 29 and **this rung must not extend it**. Adding 34 while
30–33 remain unlocked would make the table look maintained while the gap
persists. S15 lands as its own rung with its own mutation evidence.

---

## 3. Touch table

**Derived from G2a's actual footprint** (`git show 7df5ca1 --name-only`, thirty
files) plus G1's one extra, then extended for what G2b does that neither did.
It is a **floor, not a ceiling** — a touch outside it is a finding to report,
but a file here that turns out not to need changing is fine, said plainly.

### Core — the new type

| # | File | Change |
|---|---|---|
| 1 | `core/src/graph.rs` | **new `TuningContextSettings`** (the five wire-bearing fields, in codec order); doc it as the authored subset of `ScoreTuningContext` and say why `accidental_extensions` is absent |
| 2 | `core/src/lib.rs` | **public export** `epiphany_core::TuningContextSettings` (the ratified path) |
| 3 | `core/src/codec.rs` | its `Codec` — **byte-identical to `ScoreTuningContext`'s existing five-field walk**; add the assertion that the two encodings agree |
| 4 | `core/DECISIONS.md` | the subset ruling and its rationale |

### Ops — the operation

| # | File | Change |
|---|---|---|
| 5 | `ops/src/payload.rs` | `SetTuningContext` **kind 34** in the enum, the hand-written `discriminant()` match, `schema_major()` → **3** (pin 2), `tag()`, tag **34** inside `operation_kind_tag_vocabulary!`, and **`introduced_minor()` → epoch 10 in both spaces** (pin 4) |
| 6 | `ops/src/reduce.rs` | the `WriteChain<TuningContextSettings>` — decl, snapshot, init, **seed**, setter, undo verdict, restoration apply, working-snapshot save/restore. **Copy `set_metadata` structurally** (pin 5) |
| 7 | `ops/src/envdecode.rs` | envelope decode arm |
| 8 | `ops/src/migrate.rs` | migration arm |
| 9 | `ops/src/v0.rs` | v0-catalog arm |
| 10 | `ops/src/valuegen.rs` | a generator for the payload |
| 11 | `ops/src/vectors.rs` | **decode vectors pinned to literal bytes, not round-trip** (plan trap 4: a self-consistent reorder passed 1283 tests and 8/8) |
| 12 | `ops/src/fuzz.rs` | fuzz arm |
| 13 | `ops/src/textproj_kind.rs` | the kind **count** — hand-maintained (pin 8) |
| 14 | `ops/src/lib.rs` | public exports |
| 15 | `ops/DECISIONS.md` | the rung's decisions |

### Bundle — the accept-set raise (**new to G2b; G2a touched no bundle file**)

| # | File | Change |
|---|---|---|
| 16 | `bundle/src/bundle.rs` | `max_supported_major(OperationEnvelopeBlock)` **2 → 3** (:69) **and the rewritten rationale at :53-58** whose current text this rung falsifies (pin 3) |
| 17 | `bundle/DECISIONS.md` | why the raise is charged to this surface alone |

### Boundary crossings — budgeted up front (pin 8)

| # | File | Change |
|---|---|---|
| 18 | `editor-core/src/barriers.rs` | `subjects_of` arm — **compiler-forced**; without it the workspace does not build and the gate cannot run at all |
| 19 | `layout-ir/src/barrier.rs` | the "one past the vocabulary" literal → 35 |
| 20 | `testkit/src/generators.rs` | the `rng.below(N)` bound |
| 21 | `testkit/src/layout_stub.rs` | confirm the derived `PAYLOAD_FREE` path still holds — **prefer deriving over extending** |
| 22 | `testkit/tests/text_projection_grammar.rs` | the kind **count** |

### Text projection

| # | File | Change |
|---|---|---|
| 23 | `textproj/src/parse.rs` | the kind production |
| 24 | `textproj/src/project.rs` | the projection arm (G1 touched this; G2a did not — **check, don't assume**) |
| 25 | `textproj/src/vectors.rs` | vector coverage for the new kind |
| 26 | `textproj/src/lib.rs` | **`COMPANION_VERSION` 0.10.0 → 0.11.0** |

### Normative documents — the four-document ritual (pin 7)

| # | File | Change |
|---|---|---|
| 27 | `spec/binary_format.tex` | payload-layout row, tag row, the **accept-set/major-3 text**, the §2373 minor-additive history (kind 34 / epoch 10), version, Revision History row |
| 28 | `spec/operation_catalog.tex` | a `\section{SetTuningContext}`, version, changelog |
| 29 | `spec/core_spec.tex` | the **normative `OperationKind` *and* `OperationKindTag` listings**, plus any spelled-out payload count |
| 30 | `spec/text_projection.tex` | kind production, companion 0.11.0, changelog |
| 31–34 | `spec/binary_format.pdf`, `operation_catalog.pdf`, `core_spec.pdf`, `text_projection.pdf` | **all four regenerate and are committed** — every `.tex` source changes here |

### Vectors and tracking

| # | File | Change |
|---|---|---|
| 35 | `spec/vectors/decode_vectors.txt` | regenerate |
| 36 | `spec/vectors/textproj_document_vectors.txt` | regenerate |
| 37 | `spec/PASS13_CANDIDATES.md` | **close P13-S13**, citing this rung and the `metadata` precedent (pin 6) |
| 38 | `spec/PLAN_GENESIS_OPS.md` | mark G2b complete; ladder advances to G3 |

**Already done by the ratification, do NOT redo:** epoch 10 is recorded in
`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4. Transcribe it into code; do not edit that
plan.

## 4. Tests — each with the mutation that must kill it

| # | Test | Required mutation |
|---|---|---|
| t1 | Kind and tag are both 34, and the discriminant byte leads the canonical encoding | Move either to 35; must fail |
| t2 | `schema_major()` returns **3** for `SetTuningContext` | Return 0 (the pre-G2b catch-all behaviour); must fail |
| t3 | A block containing one `SetTuningContext` stamps major **3** | Make `schema_major` ignore the new arm; must fail |
| t4 | That block is **admitted** by the accept-set | Revert the cap to 2; must fail |
| t5 | **The subset pin holds**: an op authored live and the same op reloaded from bytes produce **identical** graph states, with a pre-existing non-empty `accidental_extensions` **preserved across both** | Make the payload carry the full `ScoreTuningContext`; must fail — this is the pin's whole reason for existing |
| t6 | Undo restores the previous five-field value | Restore the default instead of the predecessor; must fail |
| t7 | Undo of the **first** authoring restores the **seeded base** settings, and does so identically whether the seed was the default or a non-default value — with `accidental_extensions` untouched throughout | Skip `seed()` on the tuning chain during base ingest, so the first undo yields `NotWritten` instead of `Restore(Base)`; must fail. **Assert the two cases are indistinguishable**, not that they differ |
| t8 | Kind 34 carries **epoch 10** and a block containing it stamps minor 10 | Assign epoch 9; must fail |
| t9 | The from-empty spine still reaches a note, now with a tuning context authored | Replace the dispatch arm with `OperationKind::SetTuningContext(_) => OperationEffect::Applied` (`reduce.rs`), bypassing tuning reduction while keeping the match exhaustive; must fail on the **authored tuning-context** assertions while the spine ops stay applied and the note stays reachable. **Corrected 2026-07-29:** this row previously said "mutation is t2's", which the sweep disproved — t9 stayed green under t2's `schema_major` change, because the spine reaches a note regardless of stamping. A shared mutation that does not kill the test signs nothing |
| t10 | The accept-set prose no longer claims no payload embeds the tuning context | Grep-assert the stale sentence is absent; restore it to see the test fail |

---

## 5. Gate

`cargo fmt --check`; `cargo clippy --workspace --all-targets` **0 warnings**;
`cargo test --workspace` green with the count; conformance suites with counts
including `[7f]`; decode vectors regenerated with the count; text-projection
vectors regenerated; **all four PDFs at 0 undefined references**, committing
`binary_format.pdf`, `operation_catalog.pdf`, `core_spec.pdf` and
`text_projection.pdf` (**all four `.tex` sources change here**, unlike G-minor);
and `git status --short` **before and after**, whose only differences may be
the §3 touch table — the editor track's dirt must appear unchanged
in both.

## 6. Report

The gate outputs; every mutation with the failure actually observed; any touch
outside this contract; and **any place where this contract's own assumptions
turned out to be wrong**. Required, not politeness: G1 shipped five normative
falsehoods because its contract said "no `binary_format.tex`", and G2a's
"derived by enumerating every `SetMetadata` site" was false with the evidence
already in a grep that had been read.
