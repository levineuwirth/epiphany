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
4. **No `epiphany-bundle` change of any kind.**
   `max_supported_major(OperationEnvelopeBlock)` is 2 (`bundle.rs:69`) and
   **stays 2**. Do not edit `bundle.rs`, and do not touch its cap comment at
   `:58` — that prose belongs to G2b and moving it early leaves a different
   falsehood behind.
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

Derived by enumerating every `SetMetadata` site. Each row is **per setter**
unless noted.

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
| 19 | `editor-core/src/barriers.rs` | join the score-level arm (`:468`) + module doc (`:22`) |
| 20 | `layout-ir/src/barrier.rs` | literal 32 → 34 (`:1105`) |
| 21 | `testkit/tests/text_projection_grammar.rs` | count 32 → 34 (`:307`) |
| 22 | `textproj/src/lib.rs` | `COMPANION_VERSION` → `(0, 9, 0)` (`:29`) |
| 23 | `textproj/src/vectors.rs` | flip the superseded-version negative vector |
| 24 | `spec/text_projection.tex` | `kind` production + five version sites + changelog row |
| 25 | `spec/operation_catalog.tex` | §`SetCanvasLayoutDefaults`, §`SetSpellingPrecedence` |

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
  `SetMetadata` (`ops/tests/graph_reduction.rs:1408`). **Mutation:** reverse the
  canonical comparison → the earlier value wins.
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
* **(s7) undo survives snapshot/restore** — the pin-8 site. Snapshot the
  reducer mid-run, restore, then undo. **Mutation:** omit either chain from the
  snapshot/restore pair → the restored reducer loses the write history and the
  undo silently no-ops.
* **(s8) decode vectors pinned to literal bytes**, not round-trip. Round-trip
  locking cannot see a self-consistent encoder/decoder reorder — the 3b-i
  lesson, where a swap applied to both halves passed 1283 tests and 8/8
  conformance.
* **(s9) text-projection round-trip** for both kinds, matching the existing
  per-kind coverage, plus a negative test that a `(0 8 0)` header is now
  **rejected**.

## Blast radius

`crates/epiphany-core/src/codec.rs` + its `DECISIONS.md`;
`crates/epiphany-ops/src/{payload,envdecode,v0,migrate,reduce,textproj_kind,fuzz,valuegen,vectors}.rs`
+ its `DECISIONS.md`; `crates/epiphany-textproj/src/{lib,vectors}.rs`;
`crates/epiphany-editor-core/src/barriers.rs` and
`crates/epiphany-layout-ir/src/barrier.rs` (the two authorized crossings only);
`crates/epiphany-testkit/tests/text_projection_grammar.rs`;
`spec/{operation_catalog,text_projection}.tex` **and their rebuilt PDFs**;
`spec/vectors/*.txt` (regenerated, not hand-edited).

**Nothing else.** No `epiphany-bundle`, no `binary_format.tex`, no
`epiphany-editor-gui`, no golden re-blessing.

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
* Both `.tex` PDFs rebuilt (`latexmk -xelatex`) with **0 undefined references**
  reported. A `.tex` commit without its PDF has been a repeat lapse.
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
