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

**Open for ratification:** the type's **name**. `TuningContextSettings` is the
default. It must not be named so as to imply it is the whole context.

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

**Epoch 10, for the genesis G2b event.** This extends the ratified ladder
(`PLAN_GMINOR_SCHEMA_MINOR.md` §4, minors 2–9) by one, monotonically. **This
addition requires ratification** alongside this contract; it is the first
exercise of the ladder's own growth path.

### Pin 5 — undo must distinguish never-authored from authored-to-default

`Score::empty` seeds `tuning_context` with a **default value**, so the
`WriteChain` cannot treat "no base" and "base equals default" alike. Follow
G2a's chain shape (`reduce.rs`), and note the chain's value type is the
**subset type**, not `ScoreTuningContext` — `accidental_extensions` never
participates in undo because no operation ever writes it.

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

### Pin 9 — P13-S15 stays open

The golden lock ends at 29 and **this rung must not extend it**. Adding 34 while
30–33 remain unlocked would make the table look maintained while the gap
persists. S15 lands as its own rung with its own mutation evidence.

---

## 3. Tests — each with the mutation that must kill it

| # | Test | Required mutation |
|---|---|---|
| t1 | Kind and tag are both 34, and the discriminant byte leads the canonical encoding | Move either to 35; must fail |
| t2 | `schema_major()` returns **3** for `SetTuningContext` | Return 0 (the pre-G2b catch-all behaviour); must fail |
| t3 | A block containing one `SetTuningContext` stamps major **3** | Make `schema_major` ignore the new arm; must fail |
| t4 | That block is **admitted** by the accept-set | Revert the cap to 2; must fail |
| t5 | **The subset pin holds**: an op authored live and the same op reloaded from bytes produce **identical** graph states, with a pre-existing non-empty `accidental_extensions` **preserved across both** | Make the payload carry the full `ScoreTuningContext`; must fail — this is the pin's whole reason for existing |
| t6 | Undo restores the previous five-field value | Restore the default instead of the predecessor; must fail |
| t7 | Undo of the **first** authoring returns to never-written, distinguished from authored-to-default | Treat the `Score::empty` seed as a base write; must fail |
| t8 | Kind 34 carries **epoch 10** and a block containing it stamps minor 10 | Assign epoch 9; must fail |
| t9 | The from-empty spine still reaches a note, now with a tuning context authored | — regression guard; mutation is t2's |
| t10 | The accept-set prose no longer claims no payload embeds the tuning context | Grep-assert the stale sentence is absent; restore it to see the test fail |

---

## 4. Gate

`cargo fmt --check`; `cargo clippy --workspace --all-targets` **0 warnings**;
`cargo test --workspace` green with the count; conformance suites with counts
including `[7f]`; decode vectors regenerated with the count; text-projection
vectors regenerated; **all four PDFs at 0 undefined references**, committing
`binary_format.pdf`, `operation_catalog.pdf`, `core_spec.pdf` and
`text_projection.pdf` (**all four `.tex` sources change here**, unlike G-minor);
and `git status --short` **before and after**, whose only differences may be
this contract's own touch set — the editor track's dirt must appear unchanged
in both.

## 5. Report

The gate outputs; every mutation with the failure actually observed; any touch
outside this contract; and **any place where this contract's own assumptions
turned out to be wrong**. Required, not politeness: G1 shipped five normative
falsehoods because its contract said "no `binary_format.tex`", and G2a's
"derived by enumerating every `SetMetadata` site" was false with the evidence
already in a grep that had been read.
