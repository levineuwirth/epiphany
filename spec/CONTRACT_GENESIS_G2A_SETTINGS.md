# Contract: genesis tranche G2a — the two major-0 settings setters

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/RULING_GENESIS_PERSISTENCE.md` (011c68a) and `spec/PLAN_GENESIS_OPS.md` §4
(ladder ratified 2026-07-24, split into G2a/G2b 2026-07-28). Predecessor:
`spec/CONTRACT_GENESIS_G1_INSTRUMENT.md`, landed at 3b09595.

Execution model as every tranche on this track: Sonnet subagent, coordinator
line-level review with **independent mutation re-runs**, user deep-dive at
contract sign-off and final report. Mutation discipline throughout: anchor-assert
before substituting, restore by reversing, never `git checkout`.

**Parallel safety.** The editor track owns `epiphany-editor-gui`,
`epiphany-render-svg`, `epiphany-glyphs`, every
`crates/epiphany-editor-gui/goldens/*.png`, `spec/PLAN_EDITOR_APP.md`,
`spec/CONTRACT_EDITOR_*.md`, and `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`
(currently untracked — do not stage it). **This packet does cross into
`epiphany-editor-core` and `epiphany-layout-ir`**, by necessity, at the two
sites named in §"The boundary crossing". That crossing is bounded to those two
files and is authorized for this packet only. Stage files explicitly; never
`git add -A`.

---

## What this packet does, in one sentence

Adds two LWW settings operations — `SetCanvasLayoutDefaults` and
`SetSpellingPrecedence` — on the `SetMetadata` pattern, so that two more of the
ruling's nine `Score` surfaces become operation-authored, **without touching any
wire bound**.

## Why these two, and why not the third

G2's three setters do not sit at the same schema major. Verified by reading the
frozen codec walks, not by inference:

| Op | Carried type | Major | Evidence |
|---|---|---|---|
| `SetSpellingPrecedence` | `SpellingPrecedence` | **0** | plain `Codec::dec` at v0/v1/v2 and live (`core/src/codec.rs:2673`, `:3249`, `:3372`) — never versioned |
| `SetCanvasLayoutDefaults` | `CanvasLayoutDefaults` | **0** | versioning lives in the containing `Canvas` walk, not the leaf: `dec_canvas_v0` default-fills the field (`codec.rs:2775`), `enc_canvas_v1` writes it through the live `Codec` (`:3158`) |
| `SetTuningContext` | `ScoreTuningContext` | **3** | `enc_tuning_context_v2` writes 3 fields (`codec.rs:3293`), the live `Codec` writes 5; a default `smufl` + empty `overrides` still append bytes |

`SetTuningContext` alone drags `max_supported_major(OperationEnvelopeBlock)`
from 2 to 3, and that raise is a one-way door — an older reader meeting a v3
block preserves the bundle read-only. It is **G2b**, a separate packet, and this
one must not anticipate it in any way.

## Design pins

1. **No wire layout is designed.** Both carried types already have `Codec` impls
   and already ship inside `Score`. Add `CanvasLayoutDefaults` and
   `SpellingPrecedence` to `canonical_value!` (`core/src/codec.rs:3455`) and
   encode each payload as `push_lp_bytes(out, &self.<field>.canonical_bytes())`
   — the `SetMetadataOp` template verbatim (`ops/src/payload.rs:1360`). The
   generated `decode_canonical` already does decode → `finish()` → re-encode →
   reject-on-mismatch, so strict-form enforcement is inherited, not written.
2. **Discriminants: kind 32/33, tag 32/33.** Both spaces currently top out at 31
   (`CreateInstrument`). Assign `SetCanvasLayoutDefaults` = 32,
   `SetSpellingPrecedence` = 33 in **both** spaces. They coincide here by
   accident, not by rule — the spaces are independent and misaligned elsewhere
   (`RespellPitch` is kind 2 / tag 3). Append-only under
   `req:binfmt:kind-discriminants`.
3. **`schema_major()` gains NO arm.** Both fall into the existing catch-all
   `_ => 0` (`payload.rs:262`). **Adding them to the `=> 2` arm alongside
   `SetMetadata` would be the bug** — `SetMetadata` is there because
   `ScoreMetadata` has six mandatory major-2 appends, which is a property of
   *that* type and nothing else. Test i5 exists to catch exactly this.
4. **No `epiphany-bundle` change of any kind — and this now has two distinct
   reasons, one of which is a deliberate deferral rather than a non-event.**
   (a) `max_supported_major(OperationEnvelopeBlock)` is 2 (`bundle.rs:69`) and
   **stays 2**. Do not edit `bundle.rs`, and do not touch its cap comment at
   `:58` — that prose belongs to G2b and moving it early leaves a different
   falsehood behind. (b) **The chunk schema *minor* is not stamped, and this
   packet does not fix that.** `binary_format.tex:2330` is a MUST: a writer must
   raise the chunk schema minor when it emits a discriminant appended after the
   minor it declares, so an unknown-discriminant decode failure is attributable
   to skew rather than corruption. `SchemaVersion::for_major`
   (`bundle/src/ids.rs:204`) maps a major to a **fixed constant** — note `V0` is
   `{0, 1}`, not `{0, 0}` (`ids.rs:173`), while `V1`/`V2`/`V3` carry minor 0 —
   and, decisively, **accepts only a major**, so no per-kind additive minor can
   reach it. Both staging paths likewise derive only the major
   (`testkit/src/bundle_harness.rs:25`, `textproj/src/serialize.rs:183`). So
   kinds 24–31 already have no additive-version record, and **G2a knowingly
   takes that from eight kinds to ten.** Filed as **P13-S14** and ruled
   2026-07-28 to be its own rung, sequenced after G2a, sweeping 24–33 in one
   retroactive pass. **Do not implement minor stamping in this packet, and do
   not work around its absence** — if something here appears to need it, that is
   a finding about the ruling, so report it.
5. **Reduction copies `set_metadata` structurally** (`reduce.rs:2814`): advisory
   LWW, **no conflict, no idempotence short-circuit**, no `AlreadyApplied`. The
   write chain records unconditionally; the graph field is overwritten when a
   graph is present. Do **not** import the `create_staff` mint discipline
   (`AlreadyApplied` / `RecreateContentMismatch`) — these are field overwrites,
   not mints, and a re-write of an identical value is a legitimate new write.
6. **Both chains must be seeded from the base**, beside
   `metadata_chain.seed(...)` (`reduce.rs:1385`). This is what makes
   value-restoring undo of the *first* operational write restore the
   pre-operational value rather than nothing. Under from-empty the base is
   `Score::empty`, so the seed is the type's `Default` — and restoring the
   default is correct for both the never-authored and the authored-to-default
   case. There is no "never authored" state to distinguish: these are
   always-valued `Score` fields, not map keys, so the `Predecessor::Base` vs
   `::Write` distinction that matters for `spellings`/`breaks`
   (`reduce.rs:707`) does **not** apply. Do not invent an `Option` wrapper.
7. **Undo plumbing is three sites per setter, and one is easy to miss.** Mirror
   `metadata_chain` at: the `undo_verdict` walk (`reduce.rs:5184`), the
   `ValueRestoration` enum variant (`:785`), and the restoration *application*
   (`:5404`). Missing the application site fails silently — the verdict is
   computed and discarded.
8. **The snapshot/restore pair.** Both chains must appear in the
   snapshot/restore pair alongside `metadata_chain` (`reduce.rs:7386`, `:7423`).
   **This is the G1 silent-failure site**: omitting it surfaces only under
   undo/replay, never in a straight-line test.
9. **No deletes.** These are field overwrites; there is nothing to tombstone.

## The boundary crossing

Per `PLAN_GENESIS_OPS.md` §5 trap 6 — the G1 lesson, budgeted up front rather
than discovered mid-dispatch. Adding an `OperationKind` variant is **not**
containable to core + ops:

* **`crates/epiphany-editor-core/src/barriers.rs:468`** — `subjects_of` is
  exhaustive; without arms the workspace does not compile, and because
  `epiphany-testkit` depends on editor-core, **the entire gate is blocked** —
  conformance and `requirement_labels` included. Both new kinds are score-level
  field overwrites with no resolvable region or object, exactly like
  `SetMetadata`: **join the existing `OperationKind::SetMetadata(_) |
  OperationKind::DeclareTransaction(_)` arm** rather than writing new ones.
  That file's module doc (`:22`) already names `SetMetadata` as the
  score-level exemplar; extend that sentence.
* **`crates/epiphany-layout-ir/src/barrier.rs:1105`** — the "one past the
  vocabulary" literal, currently `32`, becomes `34`. It is deliberately a
  literal rather than `PAYLOAD_FREE.len()` so the bump is a conscious act; keep
  it that way and update the comment's kind list.
* **`crates/epiphany-testkit/tests/text_projection_grammar.rs:307`** — a
  hardcoded kind *count*, currently `32`, becomes `34`. Its own comment explains
  why it stays a literal; do not "fix" it into a derivation.

## Normative documentation — and the G1 debt this packet repairs

**The G1 contract said "no `binary_format.tex`". That was wrong, and G1 landed
leaving four falsehoods in normative documents.** Verified against 3b09595,
which touched `operation_catalog.tex` and `text_projection.tex` and no other
`.tex`:

* `binary_format.tex:1443` — the payload-layout table stops at kind **30**.
  `req:binfmt:kind-discriminants` (`:1310`) says the table is golden-locked and
  that *"each row also pins the payload's byte layout"* (`:1315`). Kind 31 is
  assigned in code and pinned nowhere.
* `binary_format.tex:1516` — the `OperationKindTag` table likewise stops at 30.
* `binary_format.tex:2432` — still asserts *"there is no
  `CreateCanvas`/`CreateInstrument` … instruments live in score genesis"*, and
  that `Canvas.layout_defaults` and `Instrument.range` are *"confined to that
  one **non-canonical** chunk"*. G1 falsified the `Instrument` half; **G2a
  falsifies the `Canvas.layout_defaults` half.**
* `core_spec.tex:12186` — the same claim in the schema-major-1 narrative:
  *"`Canvas.layout_defaults` and `Instrument.range` reach only the
  non-canonical acceleration snapshot."* Already false; G2a makes it doubly so.
* `operation_catalog.tex` — G1 added `\section{CreateInstrument}` with **no
  version bump and no changelog paragraph**, against that document's own
  convention (`:301`, `:315`, `:335`).

So the contract's earlier "no `binary_format.tex`" line is **struck**. Scope:

**`spec/binary_format.tex` → version 0.11.0 (`:243`) becomes 0.12.0**, with a
Revision History row (`:3323`) in the established shape — the 0.2.0 row is the
exact precedent, since it appended kinds 24–27 with their payload layouts and
their matching tag discriminants. The edit adds **three** payload-layout rows
(31 `CreateInstrument` — the G1 repair — plus 32 `SetCanvasLayoutDefaults` and
33 `SetSpellingPrecedence`) and the same three tag rows. Each new payload is
`lp(T)` over the carried type; use the `SetMetadata` row as the shape.
Rewrite the `:2432` bullet: `Canvas.layout_defaults` and `Instrument` now reach
the canonical operation layer, and the "no `CreateInstrument`" clause goes. The
canvas *itself* is still not minted by any operation — say that instead, since
it remains true and is the reason the bullet existed.

**`spec/operation_catalog.tex` → 0.9.0 (`:234`) becomes 0.10.0**, with **two**
changelog paragraphs: one retroactively recording G1's `CreateInstrument`
section as the 0.10.0 entry's first half (flag it explicitly as a G1 omission
being repaired, not as new work), and the two new `\section`s for this packet.

**`spec/core_spec.tex`** — **five live-text edits plus one historical
annotation**. The first is a doctrine amendment and must be done exactly as
pinned; the rest are corrections of fact:

* `:5114` — the Pass-12 K8 doctrine paragraph, which still reads *"genesis is
  the creation of an empty score together with its bundle, **outside the
  operation set**"*. `spec/RULING_GENESIS_PERSISTENCE.md` reverses precisely
  that clause. **Amend it narrowly and do not improvise:** the score root and
  the canvas remain structural givens that no operation mints, addresses, or
  deletes, and there is still no `TypedObjectId` kind for either — *that half
  survives the ruling intact and is the load-bearing half*. What is superseded
  is only the claim that the score's **contents** arrive outside the operation
  set. Cite the ruling.
* `:6899` — the `pub enum OperationKind` listing, introduced by its own prose
  as **normative for the core**. It needs **four** additions, not two:
  `TransposeInterval` (kind 30, **Push-4a debt** — the listing carries the older
  `Transpose` and never gained its successor), `CreateInstrument` (kind 31, G1
  debt), and both new setters. Add them in the listing's existing
  grouped-by-comment style.
* `:11862` — the `OperationKindTag` listing, same treatment, **the same four**.
  It likewise has `Transpose` and no `TransposeInterval`.
* `:12186` — correct the sentence naming which values reach only the
  non-canonical snapshot. Do not restructure the surrounding schema-major-1
  narrative.
* `:12207` — *"through the **eight** operation payloads that embed them"*,
  followed by an explicit enumeration. Both the count and the list move.
  **Count them from the enumeration you write, not from arithmetic on the old
  number.**
* `:16475`'s Pass-12 history entry is a record of what was ratified then, not a
  live claim: **leave the entry**, append a parenthetical noting the reversal.

**`spec/operation_catalog.tex`** — beyond the two new sections and the version
work above, two live-text repairs:

* `:1495` — the *Value restoration* passage enumerates the LWW families
  exhaustively ("… metadata, metric grid, meter change, tempo segment, staff
  layout, and the user break advisories"). Both new setters join that list.
  This list is normative for undo behaviour, so an omission here is a silent
  semantic gap, not a typo.
* `:1633` — the *Retired slots* section, which asserts *"genesis is normatively
  the empty-document constructor plus bundle creation, outside the operation
  set"*. Same narrow amendment as `core_spec.tex:5114` and the **same trap**:
  the *Create score / canvas* slot stays retired and the canvas is still never
  op-minted. Only the outside-the-operation-set clause for score **contents**
  is superseded.

**`spec/binary_format.tex`** — beyond the tables and `:2432` above:

* `:2595` — *"Snapshot-only: `Instrument` (no operation embeds one)"*. False
  since G1.
* `:2604` — *"Canonical operation layer: **eight** operation payloads"* and the
  minimal-stamping list that follows. Same rule as `core_spec.tex:12207`:
  recount from the list you write.

**That is eleven normative sites across four documents, and most of the work is
not this packet's own:** five sites are G1 debt, and the two core listings are
additionally **Push-4a debt** — they carry `Transpose` and never gained
`TransposeInterval`, appended at kind 30 in 2026-07. So the vocabulary has been
drifting from its normative listings for two tranches, not one.

Three independent reviews each found sites the previous ones missed, so treat
the list as a floor, not a ceiling: **grep for the load-bearing phrases** —
`CreateInstrument`, `TransposeInterval`, `layout_defaults`,
`SpellingPrecedence`, "outside the operation set", "snapshot-only", and every
spelled-out payload count — and report anything the list does not already name
rather than silently fixing or silently skipping it. **A count that disagrees
with the enumeration beside it is the signature to look for**; two such counts
are already known (`core_spec.tex:12207`, `binary_format.tex:2604`).

## The companion version bump

Two new **kind** productions are a document-surface grammar change, so
`COMPANION_VERSION` moves **0.8.0 → 0.9.0** (`textproj/src/lib.rs:29`). This is
the G1 precedent and it is not optional: holding the version while extending the
grammar leaves two incompatible grammars claiming `(0 8 0)`, and
`req:textproj:header-version` requires a parser to accept exactly the version it
implements and reject all others. Cached projections do not migrate —
`TextProjection` is a non-canonical accelerator, so stale ones regenerate.

Sites: `textproj/src/lib.rs:29` (the constant and its doc block), and in
`spec/text_projection.tex` the five live sites at `:237`, `:470`, `:521`,
`:1121`, `:1309` plus a **new** changelog row.

**Do not bulk-replace `0.8.0` / `(0 8 0)` across the `.tex`.** The 0.8.0
changelog row is history and must keep its version; a blind sweep falsified
exactly that row during G1 and had to be reverted. Edit the five live sites
individually and append the new row.

Also flip `textproj/src/vectors.rs`'s negative
`superseded_companion_version` vector: its "wrong version" must become the newly
superseded `(0 8 0)`, since `(0 9 0)` is now the accepted one.

## Touch points

**The earlier claim that this table was "derived by enumerating every
`SetMetadata` site" was false.** That grep was run, and rows 28 and 29 below
appeared in its output and were not carried into the table — the evidence was on
screen and went unused, which is worse than not having looked. Treat the table
as reviewed rather than derived, and check it against a fresh grep.

Each row is **per setter** unless noted.

| # | File | Site (SetMetadata analogue) |
|---|---|---|
| 1 | `core/src/codec.rs` | `canonical_value!` — add both carried types (`:3455`) |
| 2 | `ops/src/payload.rs` | op struct + `CanonicalEncode` (`:1356`, `:1360`) |
| 3 | `ops/src/payload.rs` | `OperationKind` variant (`:167`) |
| 4 | `ops/src/payload.rs` | `discriminant()` → 32 / 33 (`:289`) |
| 5 | `ops/src/payload.rs` | `tag()` (`:336`) + encode dispatch (`:385`) |
| 6 | `ops/src/payload.rs` | `OperationKindTag` variant (`:427`) + `operation_kind_tag_vocabulary!` entry (`:533`) — **compile-enforced** |
| 7 | `ops/src/payload.rs` | `schema_major()` — **no edit** (pin 3) |
| 8 | `ops/src/envdecode.rs` | discriminant decode (`:530`) + tag→kind (`:819`) |
| 9 | `ops/src/v0.rs` | `V0OperationKind` variant (`:92`) |
| 10 | `ops/src/migrate.rs` | both directions (`:167`, `:323`) |
| 11 | `ops/src/reduce.rs` | dispatch (`:2736`) + the setter fn beside `set_metadata` (`:2814`) |
| 12 | `ops/src/reduce.rs` | chain decls (`:901`, `:1012`), init (`:1292`), base seed (`:1385`) |
| 13 | `ops/src/reduce.rs` | undo verdict (`:5184`), `ValueRestoration` variant (`:785`), restoration apply (`:5404`) |
| 14 | `ops/src/reduce.rs` | snapshot / restore (`:7386`, `:7423`) |
| 15 | `ops/src/textproj_kind.rs` | production (`:174`) + parse (`:443`) |
| 16 | `ops/src/fuzz.rs` | generator arm (`:197`) |
| 17 | `ops/src/valuegen.rs` | LWW generator (`:325`) |
| 18 | `ops/src/vectors.rs` | a decode vector per new payload |
| 18a | `ops/src/lib.rs` | **public re-export** of both op types beside `SetMetadataOp` (`:126`) — downstream crates cannot name them otherwise |
| 19 | `editor-core/src/barriers.rs` | join the score-level arm (`:468`) + module doc (`:22`) |
| 20 | `layout-ir/src/barrier.rs` | literal 32 → 34 (`:1105`) |
| 21 | `testkit/tests/text_projection_grammar.rs` | count 32 → 34 (`:307`) |
| 22 | `textproj/src/lib.rs` | `COMPANION_VERSION` → `(0, 9, 0)` (`:29`) |
| 23 | `textproj/src/vectors.rs` | flip the superseded-version negative vector |
| 23a | `textproj/src/parse.rs` | the literal `HEADER` fixture `"(text-projection (0 8 0))"` → `(0 9 0)` (`:645`). **`the_test_header_tracks_the_implemented_version` fails deliberately until this moves** — it is a tripwire, not a breakage |
| 24 | `spec/text_projection.tex` | `kind` production + five version sites + changelog row |
| 25 | `spec/operation_catalog.tex` | two new `\section`s + the `:1495` and `:1633` repairs + version 0.9.0 → 0.10.0 + changelog, **including the retroactive G1 entry** |
| 26 | `spec/binary_format.tex` | three payload-layout rows + three tag rows + the `:2432`, `:2595`, `:2604` rewrites + version 0.11.0 → 0.12.0 + Revision History row |
| 27 | `spec/core_spec.tex` | five edits (`:5114`, `:6899`, `:11862`, `:12186`, `:12207`) + the `:16475` parenthetical |
| 28 | `testkit/src/generators.rs` | the operation generator (`:647`), `rng.below(30)` over a hand-maintained arm list. **Already stale**: it stops after the repeat pair, so `TransposeInterval` (Push-4a debt) and `CreateInstrument` (G1 debt) are absent from every corpus it feeds. Add all four and widen the bound |
| 29 | `testkit/src/layout_stub.rs` | `gen_operation_kind_tag` (`:951`), whose doc claims **"every variant Agent C's type provides"** while `rng.below(30)` omits tags 30 and 31. **Do not just extend it** — derive the built-ins from `OperationKindTag::PAYLOAD_FREE` and append `Registered` (excluded from `PAYLOAD_FREE` by design, `payload.rs:469`). `payload.rs:1991` already does exactly this and is the in-repo precedent |
| 30 | `ops/src/textproj_kind.rs` | a **fourth** deliberate literal count (`:597`), currently 32 → 34 |

The tag vocabulary macro (#6) is a **single source of truth**: the decoder, fuzz
corpus, conformance vectors, and edit-barrier round-trip all read
`PAYLOAD_FREE`, and a variant missing an entry **fails to compile**. It exists
because Push 4a added `TransposeInterval` to a hand-written match and nothing
else, and four hand-maintained lists stayed green while the decoder rejected its
own encoding. `OperationKind::discriminant()` (#4) has **no such guard**.

## Tests + minimum mutations

Every test must be mutation-verified: re-introduce the bug it exists to catch and
show it dies. A test that cannot see its own bug is not a test.

* **(s1) from-empty authoring.** From `Score::empty(identity)` through
  `reduce_operation_set_onto`, each setter produces the authored value in the
  materialized `Score`. **Mutation:** drop the graph write in the setter fn →
  the field stays at its `Default` while the effect still reads `Applied`.
* **(s2) LWW, last write wins, no conflict.** Two concurrent differing writes
  resolve to the later in canonical order and record **no** conflict — matching
  `SetMetadata` (`ops/tests/graph_reduction.rs:1408`). **Mutation:** *not* a
  reversed comparison — `set_metadata` contains **no comparison at all**
  (`reduce.rs:2814`); it records and overwrites unconditionally, and the LWW
  outcome falls out of canonical reduction order upstream. Mutate
  setter-locally to first-write-wins instead: skip the record-and-overwrite
  when the chain already holds a write → the earlier value survives and the
  test dies.
* **(s3) re-write of an identical value is a new write, not a no-op.** Assert
  the effect is `Applied` and the chain grew. **Mutation:** add an
  `AlreadyApplied` short-circuit → dies. *This test exists because pin 5 is the
  most likely thing for a subagent to get wrong by pattern-matching on
  `create_staff`.*
* **(s4) value-restoring undo reaches the seeded base.** Author once inside a
  transaction, undo it, and assert the field returns to the **base** value —
  run this both from-empty (base = `Default`) and onto a loaded base with a
  non-default value, so the test distinguishes "restored the base" from
  "restored the type default". **Mutation:** remove the `.seed(...)` call →
  the undo produces `Restore(None)` and the field does not move.
* **(s5) minimal stamping stays 0.** Both kinds report `schema_major() == 0`
  for every value, including a non-default one. Extend
  `schema_majors_follow_the_minimal_stamping_rule` (`reduce.rs:10546`).
  **Mutation:** move either kind into the `=> 2` arm → dies.
* **(s6) the accept-set did not move.** Assert
  `max_supported_major(ChunkKind::OperationEnvelopeBlock) == 2`, and assert a
  block carrying either new kind stamps at major 0. **Mutation:** stamp a
  payload at 3 → the assertion fires. *This is the packet's boundary against
  G2b.*
* **(s7) transaction rollback discards the write — and the assertion must not
  be on the field.** `WorkingSnapshot` (`reduce.rs:7371`) is the **transaction
  rollback** mechanism: snapshot before, restore on failure. But `restore`
  reassigns the **whole graph** (`self.graph = s.graph`, `reduce.rs:7441`)
  independently of every write chain, so a setter's *field* rolls back whether
  or not its chain was snapshotted. **An assertion on the field cannot see this
  bug** — it passes under the mutation. Two framings of this test have now been
  wrong; this is the third and it must be verified by actually running the
  mutation, not by inspection.

  Shape it so the stale chain is observable: author inside a **failed**
  transaction, then perform a **successful** write in a second transaction, then
  **undo that second transaction** and assert it restores the genuine
  predecessor — the pre-failure value, not the rolled-back one. Alternatively
  assert against chain contents directly. **Mutation:** omit either chain from
  the snapshot/restore pair (`reduce.rs:7386`, `:7423`) → the failed
  transaction's write survives in the chain and the undo restores it instead of
  the true predecessor. **If the mutation does not kill the test, the test is
  wrong — report that rather than weakening the mutation.**
* **(s8) decode vectors pinned to literal bytes**, not round-trip. Round-trip
  locking cannot see a self-consistent encoder/decoder reorder — the 3b-i
  lesson, where a swap applied to both halves passed 1283 tests and 8/8
  conformance. **Mutation:** *not* a field swap — each new payload carries
  exactly **one** field, so there are no adjacent fields to reorder. Swap the
  two new **discriminants** (32 ↔ 33) in both the encoder and the decoder
  instead: self-consistent, so every round-trip test stays green, while
  correctly-named literal vectors die. If the vectors survive too, they are
  pinned to a round-trip and are not the test they claim to be.
* **(s9) text-projection round-trip** for both kinds, matching the existing
  per-kind coverage, plus a negative test that a `(0 8 0)` header is now
  **rejected**. **Mutation (round-trip half):** emit one kind's production
  under the other's tag → parse must reject or mis-round-trip. **Mutation
  (version half):** widen the parser to accept any `(0 x 0)` → the negative
  test dies. That second one also guards `req:textproj:header-version`'s
  reject-all-others clause, which is the actual normative claim here.
* **(s10) the generators actually emit the new kinds.** Rows 28–29 feed corpora
  other suites treat as exhaustive, so a kind absent from them is untested
  everywhere downstream while every suite stays green — which is how
  `TransposeInterval` and `CreateInstrument` came to be missing from both.
  Assert that a bounded draw from each generator yields all four appended kinds
  (30–33). **Mutation:** drop one arm / leave a bound unwidened → the assertion
  fires. Row 29's derived form should make its half of this unfailable by
  construction; if it does not, the derivation is wrong.

## Blast radius

`crates/epiphany-core/src/codec.rs` + its `DECISIONS.md`;
`crates/epiphany-ops/src/{lib,payload,envdecode,v0,migrate,reduce,textproj_kind,fuzz,valuegen,vectors}.rs`
+ its `DECISIONS.md`; `crates/epiphany-textproj/src/{lib,parse,vectors}.rs`;
`crates/epiphany-editor-core/src/barriers.rs` and
`crates/epiphany-layout-ir/src/barrier.rs` (the two authorized crossings only);
`crates/epiphany-testkit/tests/text_projection_grammar.rs`;
`spec/{binary_format,core_spec,operation_catalog,text_projection}.tex` **and all
four rebuilt PDFs**; `spec/vectors/*.txt` (regenerated, not hand-edited).

Plus `crates/epiphany-testkit/src/{generators,layout_stub}.rs` (rows 28–29) —
**generator source, not test enrichment**: both are already stale by two
tranches, and both feed corpora that other suites treat as exhaustive.

**Nothing else.** No `epiphany-bundle` — the accept-set is G2b's and this packet
must not anticipate it. No `epiphany-editor-gui`, no golden re-blessing.

**Note what rows 28–30 are evidence of.** `operation_kind_tag_vocabulary!`
exists precisely because Push 4a added `TransposeInterval` to a hand-written
match and nothing else, and its own doc says four hand-maintained lists stayed
green (`payload.rs:461`). The macro made the *compile-enforced* half safe —
and these three lists went stale at that same append anyway, because nothing
forces a `rng.below(N)` bound or a literal count to move. So the vocabulary now
has **four** hand-maintained literal sites (`layout-ir/src/barrier.rs:1105`,
`testkit/tests/text_projection_grammar.rs:307`, `ops/src/textproj_kind.rs:597`,
plus the two generator bounds), and each new kind must visit all of them. Row
29's fix is the only structural one available here — derive from `PAYLOAD_FREE`
— so **prefer deriving over extending wherever a list can be derived**, and say
in the report which sites could not be.

Expect `the_canonical_base_is_byte_identical_across_data_model_majors`
(`reduce.rs:10715`) to require a **conscious re-pin**: the seeded corpus's
`gen_payload` gains discriminants 32/33, shifting the RNG stream, exactly as at
Phase D and at G1. That is a corpus shift, not a value leak. **State in the
report which it is, and how you distinguished them** — a genuine leak of a
settings value into the canonical base would be a ruling violation, and the
easy mistake is to re-pin without checking.

## Gate (report actual output, never stale numbers)

`cargo fmt --check`; `cargo clippy --workspace --all-targets -D warnings` at
**0** warnings; `cargo test --workspace` at **0** failed; `cargo test --doc` at
0 failed; conformance **8/8**, and **9/9** with `--features golden-gate`;
`requirement_labels` **6/6** with its three observed counts reported as seen
(212/282/282 at 439e1e2; they move with the catalog sections). Plus:

* `max_supported_major(OperationEnvelopeBlock)` is **still 2** — assert it in
  code and state it in the report.
* Vector corpora **regenerated** via
  `cargo run -q -p epiphany-testkit --example generate_vectors`, never
  hand-edited, with the decode-vector count reported before and after.
* **All four** `.tex` PDFs rebuilt (`latexmk -xelatex`) with **0 undefined
  references** reported for each. A `.tex` commit without its PDF has been a
  repeat lapse, and this packet touches four documents.
* The normative repairs actually landed: grep `binary_format.tex` for
  `CreateInstrument` and confirm the `:2432` bullet no longer denies it, and
  confirm both tables reach 33.
* No `crates/epiphany-editor-gui/goldens/*.png` byte changes.

## What I will verify independently before committing

Build to survive this. I re-run every mutation myself, and I check specifically:
that #4's hand-written discriminant match and #6's macro entry agree; that pin 3
really added no `schema_major` arm; that pin 8's snapshot/restore pair was not
missed; that s3 fails for the stated reason rather than incidentally; that the
canonical-base re-pin is a corpus shift and not a value leak; that the `.tex`
0.8.0 changelog row was **not** rewritten; and that no claim in the report is
copied forward from this contract rather than observed — the recurring failure
mode on this project is a plausible claim propagating because nobody re-derived
it.

## Report

Files + summary, exact asserted values, every mutation with kill evidence, gate
output verbatim, deviations flagged explicitly. If any pin here turns out to be
wrong, say so rather than working around it silently. In particular: if the
boundary crossing turns out to be wider than the three sites named above, that
is a finding about the contract, not a nuisance — report it.
