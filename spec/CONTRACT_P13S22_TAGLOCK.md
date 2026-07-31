# Contract — P13-S22: the tag space gets a second witness

**Status:** DRAFT.

**Rung type:** coverage. **No behaviour change, no wire change, no schema
change, no specification change.** Not one byte on the wire moves, and not one
discriminant is reassigned. The rung adds a guard over assignments that are
already normative, and retires the single lock it wholly subsumes.

**Disposition:** A, ruled 2026-07-31 — a hand-written literal tag→byte table in
`crates/epiphany-ops/src/payload.rs`. Disposition B (giving the numbered corpus
rows the variant-naming property) is **deferred to a separate ledger-only
commit and is no part of this rung**; it is cross-implementation diagnostic
value bought with committed-artifact churn, and it does not replace the
in-crate named failure this rung delivers.

---

## §0. What was verified before drafting

Read and executed against the working tree at `17c1d67`, not recalled.

### 0.1 The defect is topological, and it is narrower than "no lock exists"

The kind side is not safe because someone remembered to extend a table. It is
safe because **`OperationKind::discriminant()` is a hand-written match**
(`payload.rs:374`ff) and there is no `operation_kind_vocabulary!` macro. Two
independent hand-written statements of the same fact exist, and
`operation_kind_wire_discriminants_are_golden` (`:2189`) asserts they agree.

The tag side has **one** statement. `operation_kind_tag_vocabulary!`
(definition `:695`–`:752`, sole invocation `:754`–`:794`) generates
`discriminant()`, `from_discriminant()`, `catalog_name()`, `introduced_minor()`
and `PAYLOAD_FREE` from a single list. `PAYLOAD_FREE` is
`&'static [OperationKindTag]` (`:702`–`:703`) — **variants only**. The `$disc`
literal is consumed into two match bodies (`:709`, `:719`) and is reachable
nowhere else.

**Consequence, and the reason this rung exists:** every existing derived test
obtains the pair as
`PAYLOAD_FREE.iter().map(|t| (t, t.discriminant()))` — both halves expanding
from the same invocation. Such a test asserts `$disc == $disc`. It cannot
disagree with the macro, because it *is* the macro.

The repair is therefore not "add a table." It is **supply the second
independent statement the tag side has never had.**

### 0.2 The signing mutation, executed

The three probes recorded in the P13-S22 ledger row swapped discriminant
*literals* only. That leaves declaration order unchanged, so `PAYLOAD_FREE`
(declaration order, `:703`) begins emitting descending discriminants, the
corpus rows change order, and `the_committed_corpus_matches_the_generator`
fails. All three were caught. **A probe that fails proves a lock exists, not
that one is missing** — the row already says so.

The **coordinated** permutation swaps the discriminant literals *and* the two
declaration lines, so `PAYLOAD_FREE` still emits ascending discriminants and
every derived artifact is byte-identical while the variant→byte association is
reversed.

Executed at `17c1d67` on `payload.rs:786`–`:787`:

```
-    SetCanvasLayoutDefaults = 32 => "set-canvas-layout-defaults" @ Some(9),
-    SetSpellingPrecedence = 33 => "set-spelling-precedence" @ Some(9),
+    SetSpellingPrecedence = 32 => "set-spelling-precedence" @ Some(9),
+    SetCanvasLayoutDefaults = 33 => "set-canvas-layout-defaults" @ Some(9),
```

`cargo test --workspace` → **1558 passed, 0 failed** — byte-identical to the
clean baseline. Two operations exchanged wire discriminants in complete
silence. Restored by hand; `git status` verified clean.

**Why 32↔33 specifically, and why the pair matters:** both carry epoch
`Some(9)`, so the kind/tag epoch-agreement assertion (`:2798`–`:2799`) stays
satisfied. A pair with differing epochs would fail there and the mutation would
be caught for a reason having nothing to do with this rung.

**Checked and confirmed blind to the coordinated swap:**

| Surface | Why it cannot see it |
|---|---|
| `the_tag_vocabulary_is_complete` (`:2652`) | derived; proves completeness, density, round-trip — never which tag holds which byte |
| the numbered corpus rows (`ops/src/vectors.rs:206`–`:209`) | name `tag_{:02}` and payload both from `discriminant()`; identity discarded. Order preserved by the coordinated form |
| `the_kind_productions_are_the_operation_vocabulary` (`testkit/tests/text_projection_grammar.rs:290`) | collects a `BTreeSet<String>` of `catalog_name()` — order-insensitive, discriminant-free |
| `catalog_name()` / `introduced_minor()` | per-variant; both follow the variant, not the byte |
| the kind-side golden (`:2189`) | asserts `kind.discriminant()` and `to_canonical_bytes()[0]`, both **kind**-space; `kind.tag()` is used only to format the message |
| `t1_set_tuning_context_kind_and_tag_are_both_34` (`reduce.rs:12738`), `t1_g3a_kinds_and_tags_are_35_to_38_in_both_spaces` (`reduce.rs:15931`) | the only two `tag().discriminant()` assertions in the workspace; they pin 34 and 35–38, not 32/33 |
| `edit_barriers_blob_bytes_are_golden` (`layout-ir/src/barrier.rs:1038`) | its frozen blob carries tag 1 only |

### 0.3 Current coverage, recounted from the tree

Semantic locks — a **named variant** bound to a **literal byte** — exist for
**1, 16, 24–29, 34, 35–38, 39 = 14 of 40**. Unlocked: **0, 2–15, 17–23,
30–33 = 26**, which matches the ledger row's list exactly (1 + 14 + 7 + 4).

*(The reconnaissance summary that produced this inventory stated "13 of 40";
its enumeration listed fourteen values. The enumeration is correct.)*

### 0.4 None of the fourteen names a tag on failure

| Fragment | Failure text |
|---|---|
| `phase3_tag_discriminants_are_golden` `:2740`, `:2741` | bare `left/right`, no message |
| `reduce.rs:12744` | bare `left/right` |
| `reduce.rs:15941` | bare `left/right` |
| `payload.rs:2936` | bare `left/right` |
| the corpus (`testkit/src/vectors.rs:224`) | *"spec/vectors/decode_vectors.txt is stale. Regenerate: …"* |
| `barrier.rs:1061` | a 53-element array diff |

This is the ledger row's claim — *"the gap is intent and diagnosis, not
exposure"* — verified at each of the six sites.

### 0.5 The in-tree statement this rung overturns

`payload.rs:2201`–`:2205`, inside the kind-side golden test's own header
comment:

> The sibling tag half needs no such extension: `the_tag_vocabulary_is_complete`
> is derived from `OperationKindTag::PAYLOAD_FREE` with a computed bound rather
> than a spelled one, so it already covers every tag including 30..=39.

Every clause is individually true. The conclusion is false: a derived,
computed-bound test covers every tag's *existence* and no tag's *value*. This
is the same shape the pass has found six times — locally true, globally
misleading — and it is the reason P13-S15 closed believing the tag half needed
nothing.

---

## §1. Pins

**Pin 1 — the table is hand-written literals, and the length is spelled.**
A new `#[test] fn tag_wire_discriminants_are_golden()` in
`crates/epiphany-ops/src/payload.rs`, declaring

```rust
let table: [(OperationKindTag, u8); 40] = [ … ];
```

with **all 40 rows typed out**, each pairing a variant named in source against
a `u8` literal typed in source. The length `40` is a spelled literal.

**Forbidden absolutely:** deriving any row, the length, or the ordering from
`PAYLOAD_FREE`, `discriminant()`, `from_discriminant()`, or any other macro
output. A table so derived asserts `$disc == $disc` and is born green. The
project states this reasoning in three existing comments
(`testkit/tests/text_projection_grammar.rs:312`–`:314`,
`layout-ir/src/barrier.rs:1171`–`:1172`, `payload.rs:2638`–`:2639`); the new
test carries the same warning in its own header.

**Pin 2 — coverage is derived; association is not. Both are asserted, and the
distinction is stated.**
The table's *totality over the vocabulary* may legitimately be computed, and
must be, because a 40-long array does not prove forty distinct tags — duplicate
rows satisfy the length. Assert, as a separate block from the row loop:

- the table's tag set equals `PAYLOAD_FREE` ∪ `{Registered}` — every variant
  present **exactly once**;
- the table's byte set is exactly `0..=39`, no gaps, no repeats.

The test's comment must state why this is not circular: **the coverage
assertion proves the table is total over the vocabulary; it says nothing about
which tag holds which byte, which only the literal rows state.**

**Pin 3 — the canonical-bytes assertion is shape-aware, and retiring `phase3`
must not weaken it.**
`phase3_tag_discriminants_are_golden` asserts
`tag.to_canonical_bytes() == vec![expected]` — the **whole vector**, which also
proves the encoding is exactly one byte long. The kind-side idiom asserts only
`[0]`. Copying the kind-side idiom while retiring `phase3` would silently drop
the length-1 property for 24–29.

Therefore: for the 39 payload-free tags assert the **whole canonical byte
vector** equals `vec![expected]`; for `Registered` — which carries a 16-byte
registry id — assert **`to_canonical_bytes()[0] == expected`** only. The test
must make the asymmetry explicit in a comment, naming `Registered` as the sole
non-payload-free tag.

**Pin 4 — the failure must name the variant.**
Each row's discriminant assertion carries a message naming the tag, in the
kind-side idiom:

```rust
"wire discriminant for {:?} moved — canonical encodings are append-only"
```

formatted from the tag itself. This is the deliverable: today the coordinated
swap is silent, and after this rung it must fail *by variant name*. A bare
`assert_eq!` anywhere in the row loop does not satisfy this pin.

**Pin 5 — retire exactly one lock. Retain the other three tag→byte
assertions, and be exact about why.**

Retire **`phase3_tag_discriminants_are_golden`** (`:2727`–`:2743`) — the whole
test. Its subject *is* the tag space and nothing else: every line of its body
is tag→byte for 24–29, and the table states all six pairs under pin 3. Nothing
survives it that the table does not say better.

Retain **`payload.rs:2936`**, **`reduce.rs:12744`**, and **`reduce.rs:15941`**.

The distinction, stated exactly, because an imprecise version of it was the
first defect found in this contract:

> The table **duplicates the tag→byte subclaim** each of these three lines
> makes. It does **not** subsume the claim each enclosing test exists to make.
> Those tests assert **kind-and-tag agreement** — that `OperationKind::X`'s
> `tag()` is `OperationKindTag::X` *and* that this tag carries a specific byte,
> as a paired two-step statement in one place. The table states only the second
> step, for every tag. Removing the byte line would leave the pairing broken in
> tests whose names promise both spaces
> (`…kind_and_tag_discriminant_are_39`, `…kind_and_tag_are_both_34`,
> `…in_both_spaces`).
>
> It is **not** correct to say these tests assert a fact the table lacks. The
> table lacks no fact they state. What the table lacks is their **locality** —
> the adjacency of the two steps — and, for `payload.rs:2936`, its standing as
> the documented home of **genesis G3b's ratified mutation M2** (doc comment
> `:2919`–`:2925`, *"move the tag discriminant to 40; must fail"*). That
> evidence stays where the ratified contract already points, and
> `spec/CONTRACT_GENESIS_G3B_MEASURE.md` needs no edit. Its mutation count of
> 75 is untouched.

Also retain, neither being a unit lock:

- `barrier.rs:1058` — a comment; it asserts nothing and costs nothing.
- the `Registered` corpus row (`ops/src/vectors.rs:210`–`:217`,
  `spec/vectors/decode_vectors.txt:80`) — a decode vector serving the
  cross-implementation corpus.

Redundancy between the table and the three retained lines is **intended**, and
the new test's header comment must say so, so that a later reader does not
"consolidate" them and destroy the pairing.

**Pin 6 — the stale comment is removed, and its removal is gated by meaning,
not by name.**
`payload.rs:2201`–`:2205` must go, replaced by text stating what is now true:
the tag half's derived tests prove completeness, density and round-trip, and
`tag_wire_discriminants_are_golden` is what pins the mapping.

**The gate may not be a search for the word "tag."** Neighbouring prose in the
same comment mentions tags legitimately (`:2201`, and the retained
`the_tag_vocabulary_is_complete` reference). §4 specifies a phrase-absence gate
with whitespace normalization, because the offending sentence straddles four
source lines and any line-oriented search for it is born green.

**Pin 7 — nothing else moves.**
No change to `operation_kind_tag_vocabulary!`, to any discriminant, to
`PAYLOAD_FREE`, to `spec/vectors/decode_vectors.txt`, to any `.tex` document,
to any version or companion version, to any `DECISIONS.md`. The mapping is
already normative at `spec/binary_format.tex:1533`–`:1552` (40 tags, two per
line); this rung adds a guard over it. **No `DECISIONS.md` entry is warranted**
— following P13-S15's precedent, this is test-coverage bookkeeping, not a
semantic ruling.

**Pin 8 — the ledger row closes, and closes only itself.**
`spec/PASS13_CANDIDATES.md`'s P13-S22 row moves to RESOLVED, recording: the
disposition-A ruling; that the corrected inventory is 14 locked / 26 unlocked;
the coordinated-swap mutation with its observed 1558/0 silence; and that
disposition B was considered and **deferred to a separate ledger-only commit**.

Two wording repairs inside that same row, both now false as written:

- *"superseding the six scattered fragments rather than adding a seventh"* →
  the table supersedes **one** fragment (`phase3_tag_discriminants_are_golden`)
  and deliberately duplicates three more, for the locality reason in pin 5.
- the row's probe-design note says the signing mutation is "the inverse —
  delete the proposed table and show that some permutation then passes." That
  is M2, and it is necessary but not sufficient. Record the **coordinated**
  permutation (literals *and* declaration lines) as the mutation, since the
  literal-only form is caught by the corpus and proves nothing.

**No other candidate is filed, opened, closed, or reworded by this rung.**
Disposition B gets no id here.

---

## §2. Touch table

Exactly these files. Nothing else may be read for modification, and nothing
else may be staged.

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-ops/src/payload.rs` | add `tag_wire_discriminants_are_golden` (pins 1–4); delete `phase3_tag_discriminants_are_golden` `:2727`–`:2743` (pin 5); replace `:2201`–`:2205` (pin 6) |
| 2 | `spec/PASS13_CANDIDATES.md` | P13-S22 row → RESOLVED, plus its two wording repairs (pin 8). **No other row is touched.** |

**Two files. No others.**

---

## §3. Mutation plan

Four mutations. Every one is applied to the working tree, **run**, its actual
output recorded verbatim, and restored **by hand-editing back** — never by
`git checkout`, `git restore`, or `git stash`. Reasoning that a mutation would
fail signs nothing.

**M1 — the signing mutation.** Apply the coordinated 32↔33 swap of §0.2
(literals *and* declaration lines). Run
`cargo test -p epiphany-ops tag_wire_discriminants_are_golden`.
**Must fail, and the recorded output must contain the variant name
`SetCanvasLayoutDefaults` or `SetSpellingPrecedence`.** A failure that names no
variant does not satisfy pin 4 and does not sign this rung.

**M2 — the paired control, which is what makes M1 mean anything.** With M1's
mutation **still applied**, delete the new table's row loop (or the whole new
test) and run `cargo test --workspace`. **Must pass 1558/0** — reproducing
P13-S22 itself in the tree rather than arguing for it, exactly as P13-S15's
`&table[..30]` control did. Then restore the test, leaving M1 applied, confirm
the failure returns, and restore M1.

> Without M2, M1 shows only that some test fails under some mutation. The whole
> claim of this rung is that **nothing else** sees it.

**M3 — coverage is real, not a length check.** Duplicate one row (two rows
naming the same tag, so the array is still 40 long) and confirm the pin-2
coverage block fails. This proves the length literal is not doing the work
alone.

**M4 — the payload-free/`Registered` asymmetry is load-bearing.** Change
`Registered`'s row to assert the whole canonical byte vector rather than `[0]`
and confirm it fails on the 16-byte registry-id payload. This signs that the
asymmetry in pin 3 is a real property of the encoding, not defensive
formatting.

**Reporting:** for each mutation, the exact edit, the exact command, and the
**verbatim first failure line**. "Fails as expected" is not a record.

---

## §4. Gate

All of the following, run by the executing agent, and independently re-run by
the reviewer before commit.

1. `cargo test --workspace` → **1558 passed, 0 failed.** The count is
   unchanged: this rung adds one test and deletes one, so the net is zero.
   **A count of 1559 or 1557 is a finding, not a rounding error** — report it
   rather than adjusting the expectation.
2. `cargo clippy --workspace --all-targets -- -D warnings` → clean.
3. `cargo fmt -p epiphany-ops --check` → clean.
   **`cargo fmt --all` is forbidden** — it reaches the `spikes/` workspace
   through path dependencies.
4. **Phrase-absence gate for pin 6.** Read `crates/epiphany-ops/src/payload.rs`,
   strip `///`, `//!` and `//` markers, collapse all runs of whitespace
   (including newlines) to single spaces, then assert **absence** of:
   - `The sibling tag half needs no such extension`
   - `it already covers every tag including`

   Both straddle source lines in the original, so a line-oriented `grep` finds
   neither and is born green. Then assert **presence** of a replacement
   sentence naming `tag_wire_discriminants_are_golden`.
5. `git diff --cached --check` → clean. **Note it is vacuous if nothing is
   staged**, and blind to untracked files; run it after staging, and confirm
   the staged file list is exactly the two files of §2.
6. Confirm `spec/vectors/decode_vectors.txt` is **unmodified** — this rung
   changes no committed byte artifact.

---

## §5. Staging and boundary

**Stage only the two files of §2, by explicit path.** Never `git add -A`.

**A concurrent session commits to this repository.** Re-check `HEAD` before
staging and again before commit. **Never** run `git reset`, `git restore
--staged`, `git checkout`, or `git stash` against the shared index.

**Out of bounds — MUST NOT be read, written, or staged:** the entire `spikes/`
tree, `spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_*.md`,
`spec/ANALYSIS_GENESIS_PERSISTENCE.md`, `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`,
`spec/DRAFT_T4_FIXTURE_RECIPE.md`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`, `crates/epiphany-editor-gui/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the root `Cargo.toml`,
`.claude/worktrees/`.

**`spec/CONTRACT_GENESIS_G3B_MEASURE.md` is RATIFIED and MUST NOT be edited.**
Nothing in this rung disturbs its mutation M2 or its count of 75 — see pin 5.

**The executing agent MUST NOT commit.** Leave the work staged for independent
verification.

---

## §6. Report requirements

1. The four mutations of §3, each with its verbatim failure line — and for M2,
   the verbatim passing count.
2. The six gate results of §4, each with the command run.
3. The exact staged file list.
4. The final table as written, so the reviewer can check all 40 rows against
   `payload.rs:755`–`:793` plus `REGISTERED_TAG_DISCRIMINANT` (`:679`) without
   re-deriving them.
5. Anything found that contradicts this contract. A contract defect reported is
   worth more than a contract satisfied.
