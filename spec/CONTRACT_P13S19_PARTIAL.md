# Contract — P13-S19: what a partial measure actually costs

**Status:** DRAFT.

**Rung type:** correction and observation. **No behaviour change.** Not one
graph's invariant-20 verdict may move, and not one operation's effect may
change. The rung makes the tree say what it already does.

---

## §0. What was verified before drafting

Read out of the working tree at `339269b`, not recalled.

1. **An existing test is already the pickup demonstration.**
   `m35_boundary_flags_wrong_distance` (`invariants.rs`) places `m0` at region
   start offset `0` and `m1` at offset `Musical(1/2)`, under a signature whose
   `measure_duration()` is a **whole** (`sig()`, `:4597`), and asserts invariant
   20 **fires**. That is a first measure occupying half a bar followed by its
   successor — a pickup — and the test passes today inside the 1556. It has
   been labelled "wrong distance" since packet 2.
2. **`create_measure` applies the same rule as a refusal.** Clause 3
   (`reduce.rs:5548`ff) resolves the governing signature at `prev_start`, then
   `(Some(_), Some(_)) => MeasureMeterMismatch` when the delta differs from
   `measure_duration()`. **Read, not executed** — pin 4 makes observing it a
   deliverable, because this rung's headline claim rests on it.
3. **The exemption is index-based, not partial-aware, and it is narrower than
   an earlier draft of this contract claimed.** Invariant 20 skips only the
   *boundary* clause at `i == 0`; `create_measure` skips only clauses 1 and 3
   when `predecessor` is `None`. **The agreement clause is not
   predecessor-dependent**: `invariants.rs:2684`ff evaluates it *before* the
   `i == 0` bypass, and `reduce.rs:5526`ff is ungated. Neither asks whether any
   measure is partial.
4. **Seven surfaces carry the understated form**, three of them normative
   documents and one of them outright **false**:

   | # | Site | Form |
   |---|---|---|
   | a | `spec/PASS13_CANDIDATES.md`, the P13-S19 row | "MUST NOT be refused or flagged by either" |
   | b | `crates/epiphany-core/src/graph.rs:598`–`:599` | "never refused or flagged by this rung" |
   | c | `spec/operation_catalog.tex:1678` | "never refused or flagged on this account" |
   | d | `spec/core_spec.tex:6632`–`:6633` | "has no predecessor and is never flagged" |
   | e | `crates/epiphany-core/src/invariants.rs:111`–`:112` | the **public doc comment** on `GraphInvariant::MeasureMeterConsistency`, same form |
   | f | `m38_pickup_first_measure_not_flagged` | its **name**, its comment, and its assertion message ("must never be flagged by invariant 20") |
   | g | `crates/epiphany-ops/DECISIONS.md:2161` | **"All three clauses are vacuous for an instance's first measure — no predecessor to compare against."** This is not understatement, it is **false**: clause 2 has no predecessor dependency and runs |

   Rows **d, e, f** were **absent from the ratified scope**, which named the
   catalog and then rows a/b/g. They are included because the scope's own
   reasoning applies to each verbatim: narrow literal truth leaving the
   misleading conclusion intact.

   **`crates/epiphany-core/DECISIONS.md:1559`–`:1560` is NOT a surface and MUST
   NOT be edited.** An earlier draft of this contract listed it as one, having
   matched the phrase "never flagged" without reading its qualifier. It reads
   "has no predecessor and is therefore never flagged **by the boundary
   clause**" — accurate, correctly scoped, and the only site in the tree that
   already states the distinction this whole rung exists to draw. It is recorded
   here so that a later reader comparing it against the corrected row **g** does
   not "fix" it into agreement and destroy the one correct statement.
5. **A fifth pickup surface is a different deferral.** `core_spec.tex:2484` says
   the integer-grid metric splitter "assumes the region origin falls on a
   barline (anacrusis/pickup handling deferred)". Chapter 3, derived notation,
   predating G3b, filed nowhere. Pin 6.
6. `grep -rn "pickup\|anacrusis" spec/*.tex` returns **exactly** those three
   `.tex` sites. There is no fourth to miss.
7. `operation_catalog.tex` is at **0.13.0** (title `:234`, changelog `:481`).
   `core_spec.tex` carries no title-page version and never has.
8. **§4's absence gate was proven non-vacuous before ratification.** All seven
   phrases were confirmed **present** in their files at `339269b` under the
   gate's own normalization, and the positive check confirmed present — so the
   gate **fails today** and can pass only once the corrections land. A gate that
   already passes before the work is done proves nothing, which is the defect
   the bare `grep -rn "pickup\|anacrusis"` would have shipped.

---

## §1. Pins

**Pin 1 — the real consequence, stated exactly, and the exemption's true
width.** A first measure is exempt from the **predecessor-dependent** checks
only: invariant 20's boundary clause (`i == 0` → `continue`) and
`create_measure`'s clauses 1 and 3 (`predecessor: None`). **The agreement clause
is not predecessor-dependent and applies to a first measure like any other** —
so a pickup carrying a **resolving signature that disagrees with the governing
grid is flagged by invariant 20 and refused by `create_measure` with
`MeasureMeterMismatch`.** An earlier draft of this contract said a pickup "is
itself neither refused nor flagged"; that was wrong, and it is the same
over-reading the rung exists to correct, committed inside the correction.

**What a first measure avoids is exactly this and no more:** the
predecessor-dependent checks, plus the agreement check when it declares `None`
or a matching signature — **and only when its other preconditions are
satisfied.** It can still be refused for a dead parent `StaffInstance`, an
unresolving `measure.time_signature` referent, or an unresolving referent of
`measure.start`; and invariant 10 can still flag an unresolved signature
reference on it. "A pickup is not refused or flagged" is false as a general
claim in **both** directions, and the corrected text must not trade one
over-reading for another.

**Its successor is measured against the governing signature's full
`measure_duration()`, and is refused (`MeasureMeterMismatch`) by
`create_measure` and flagged by invariant 20.** The old ledger form — "a partial
first measure MUST NOT be refused or flagged by either" — is true only of the
pickup itself, only under the conditions above, and reads as "pickups work".

**Pin 2 — the scope, corrected.** P13-S19 covers **boundaries following any
partial measure**, not "every partial measure". The check compares
`prev.start → current.start`, so a **mid-score partial may itself enter
successfully** — nothing examines its own duration — while **its successor is
what exposes the partial duration and fails**. The failure is always attributed
to the measure *after* the partial one, which is also why the witness text names
the wrong measure. Mid-score partials have **no exemption at all**, unlike the
first measure.

**Pin 3 — the root cause is a missing quantity, not a missing exemption.** Both
rules compare `delta(prev.start, m.start)` against the *governing signature's*
`measure_duration()`, when the distance actually equals **`prev`'s own content
duration**. Those coincide only for full measures. Closing this needs a
per-measure duration — the "partial measure" notion the entry correctly names —
and **this rung does not introduce it.** Widening the exemption instead would
suppress the symptom and lose real violations.

**Pin 4 — the reducer refusal must be observed, not read.** The conclusion is
incomplete until `create_measure` is driven end-to-end through the envelope
harness (`g3b_region_and_instance_envs` / `prim_env` /
`g3b_region_anchor`, modelled on `g3b_create_measure_ordering_agreement_boundary`,
`reduce.rs:19573`) and observed returning
`NoOp { PreconditionFailedUnderReduction { MeasureMeterMismatch } }` for the
successor of a pickup. **The same test must observe the pickup's own mint
`Applied`** — that is pin 1's true half, and asserting only the refusal would
leave it unsigned.

**The pickup must carry `time_signature: None` or a signature that matches the
governing grid.** Per pin 1 the agreement clause runs on a first measure, so a
pickup declaring a mismatching signature is refused by **clause 2** — and the
test would then observe a `MeasureMeterMismatch` that has nothing to do with
partiality, while appearing to confirm exactly this rung's claim. Both refusals
carry the *same reason code*, so the fixture is the only thing separating them.
The report must state which form the pickup used.

**Pin 5 — `m35` records what it demonstrates.** Rename and extend it so the tree
says it is the pickup case. **Its existing assertion may not be weakened**: it
must still fire, for the same reason, on the same shape. Extension only.

**Pin 6 — P13-S24, filed separately and cross-linked.** The Chapter 3 splitter
deferral (`core_spec.tex:2484`, mirrored at
`crates/epiphany-core/DECISIONS.md:340`, "barline (anacrusis/pickup deferred)")
gets its own candidate. It **shares the missing partial-duration concept** with
P13-S19 and is otherwise **independent**: it affects **derived notation**, not
invariant 20 and not `CreateMeasure`. Each entry links the other, and P13-S24
names both of its sites so the next reader does not have to rediscover the
mirror the way this contract rediscovered surface h. Filing it inside S19 would merge two subsystems' work
into one id; leaving it unfiled is what let it sit invisible since Chapter 3.

**Pin 7 — the catalog pays the full ritual.** `operation_catalog.tex:1678` is
normative. Correct it to state the real behaviour: the first measure skips its
predecessor check, **and its successor is still checked against a full
`measure_duration()` and refused**. Then **0.13.0 → 0.14.0** on the title page
(`:234`), a changelog paragraph modelled on `:481`'s, and a regenerated PDF.
`core_spec.tex:6632` gets the same correction; it carries no version, so it
needs only the text and its PDF.

**Pin 8 — a workaround may not be named unless it is executed.** Reading
suggests a pickup carrying its own shorter meter change satisfies clause 3,
because the governing signature at `prev_start` becomes the short one.
**Unverified.** If any corrected text names a workaround, a test must observe
it end-to-end; otherwise no text may name one. A normative document acquiring an
untested recommendation is how the last four rungs' defects started.

**Pin 9 — the hard stop.** No production logic edit. No change to any
invariant-20 verdict or any operation effect. `check_measure_meter_consistency`
and `create_measure` both stay byte-identical. **If the work suggests the
current behaviour is wrong rather than merely undocumented — including "the
successor should be exempt" — stop and report.** That is a semantic rung and it
needs the partial-duration notion first.

---

## §2. Touch table

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-core/src/invariants.rs` | **tests plus doc only** — pin 5's rename/extension of `m35`; surface **f** (`m38`'s name, comment, assertion message); surface **e** (the public doc comment on `GraphInvariant::MeasureMeterConsistency`, `:111`–`:112`) |
| 2 | `crates/epiphany-core/src/graph.rs` | surface **b**, the `Measure` doc comment (`:598`–`:599`) |
| 3 | `crates/epiphany-ops/src/reduce.rs` | **tests only** — pin 4's observed refusal, plus the mid-score case of pin 2 |
| 4 | `spec/operation_catalog.tex` | surface **c** (`:1678`); title `:234` 0.13.0 → **0.14.0**; changelog paragraph |
| 5 | `spec/core_spec.tex` | surface **d** (`:6632`–`:6633`). **No version bump** — this document has none |
| 6 | `spec/operation_catalog.pdf`, `spec/core_spec.pdf` | regenerated, **after** their sources are final |
| 7 | `spec/PASS13_CANDIDATES.md` | surface **a** — P13-S19 corrected per pins 1–3; **P13-S24** filed per pin 6 |
| 8 | `crates/epiphany-ops/DECISIONS.md` | surface **g**, `:2161` — repair the **false** "all three clauses are vacuous" |
| 9 | this contract | DRAFT → RATIFIED |

**Nine rows. `crates/epiphany-core/DECISIONS.md` is NOT among them** — see
§0.4's closing note. It is correct as written and is out of bounds for this
rung.

**No `binary_format.tex`** — no wire, discriminant, or accept-set change.

**Rows 8 and 9 do not overturn the standing "no `DECISIONS.md` entry" ruling.**
That ruling says this class of rung adds no new record, and it holds: nothing is
appended. What rows 8 and 9 do is **repair statements already there that are
false or misleading**, which no ruling protects. Row 8 in particular is the only
place in the tree where the understatement has hardened into an outright false
claim.

---

## §3. Mutation plan

| M | Edit | Row it signs |
|---|---|---|
| M1 | in the extended `m35`, change `m1`'s offset from `Musical(1/2)` to a full `measure_duration` | invariant 20 fires **because** the predecessor is partial, not for some other reason |
| M2 | in pin 4's reducer test, move the successor to a full `measure_duration` after the pickup | the refusal is the partial distance: the effect becomes `Applied` |
| M3 | in pin 4's reducer test, repoint the `Applied` assertion at the **successor's** already-executed effect — the `NoOp` in the same harness | the pickup's `Applied` assertion is *precise*, not merely satisfied: it fails when aimed one operation over. No new fixture; a separate refusal fixture would test a different graph, not this assertion |
| M4 | in the mid-score test, move the **successor** to one full `measure_duration` after the partial measure | pin 2: the failure follows the partial predecessor, and a mid-score partial has no exemption. **There is no "make the measure full" edit** — `Measure` carries no duration field, and pin 3 is precisely that its length is inferred only from its successor's start |

**Four mutations, one edit each.** Each observed red and restored by
hand-editing back — never `git checkout`, never `git stash`. Report the observed
count; any deviation is a finding.

**Prose-only surfaces have no mutation** — surfaces a, b, c, d, e, g. They
are covered by the §4 grep gate, and the report must say so rather than implying
mutation coverage. Surface **f** is the exception: `m38` is a test, so its
corrected assertion message must still describe an assertion that actually
holds, and pin 5's no-weakening rule applies to it as it does to `m35`.

**Anti-traps.** A mutation that does not compile signs nothing. Envelope counter
gaps make operations permanently pending — a reducer fixture whose ops never
execute asserts nothing, and the refusal this rung exists to observe would be
indistinguishable from an op that never ran. **Assert the pickup's mint
`Applied` before asserting the successor's `NoOp`**, so a vacuous fixture cannot
masquerade as a refusal. Transaction-block members reduce atomically at the
first member's canonical position.

---

## §4. Gate

- `cargo test --workspace` — baseline **1556 / 0** at `339269b`; delta explained.
- `cargo clippy --workspace --all-targets` — zero warnings.
- `cargo fmt -p epiphany-core -p epiphany-ops -- --check` — clean. **Never
  `cargo fmt --all`**: it crosses into the `spikes/` workspace.
- All **4** mutations observed red and hand-restored.
- **Byte-identical bodies** for `check_measure_meter_consistency` **and**
  `create_measure`, demonstrated mechanically against `339269b`. State the
  command and paste its output.
- **The absence gate.** A bare `grep -rn "pickup\|anacrusis"` is
  **non-discriminating** — it lists occurrences and succeeds whether every
  obsolete claim survives or not. It is replaced by an **absence check on seven
  specific phrases**, each in its own file, plus one **positive** check.

  **Whitespace must be normalized before matching.** Phrases **d** and **g**
  straddle a newline in their sources today (`core_spec.tex:6632`–`:6633`,
  `ops/DECISIONS.md:2160`–`:2161`), so a line-oriented search for either would
  return nothing **before** the correction as well as after — born green, the
  same defect in the opposite direction. The gate must read each file whole,
  collapse whitespace runs to a single space, and strip `///`, `//!`, and `//`
  markers, before searching.

  | # | File | Phrase that MUST be absent |
  |---|---|---|
  | a | `spec/PASS13_CANDIDATES.md` | `MUST NOT be refused or flagged by either` |
  | b | `crates/epiphany-core/src/graph.rs` | `never refused or flagged by this rung` |
  | c | `spec/operation_catalog.tex` | `never refused or flagged on this account` |
  | d | `spec/core_spec.tex` | `first measure has no predecessor and is never flagged` |
  | e | `crates/epiphany-core/src/invariants.rs` | `first measure has no predecessor and is never flagged (P13-S19, deferred)` |
  | f | `crates/epiphany-core/src/invariants.rs` | `must never be flagged by invariant 20` |
  | g | `crates/epiphany-ops/DECISIONS.md` | `All three clauses are vacuous for an instance's first measure` |

  **Positive check:** `crates/epiphany-core/DECISIONS.md` MUST still contain
  `never flagged by the boundary clause`. That file is correct as written
  (§0.4's closing note) and this check is what stops a later reader from
  "fixing" it into agreement with the corrected row **g**.

  The report must paste the gate's output, not merely assert it ran.
- **Renames strand citations.** `m35_boundary_flags_wrong_distance` and
  `m38_pickup_first_measure_not_flagged` are cited **in this contract**
  (`:16`, `:44`) and nowhere else in `spec/` or `crates/` — verified. Pin 5's
  rename must update those two citations in the same commit, and the report must
  confirm the post-rename names appear nowhere stale.
- Both regenerated PDFs, with `operation_catalog.pdf`'s title page read back.
- `git diff --cached --check` clean; nothing from `spikes/` or `.claude/`
  staged.

## §5. Whitespace and staging

1. Stage the touch-table files explicitly. **Never `git add -A`.**
2. `git diff --cached --check`.
3. Commit.
4. `git diff --check <parent>..HEAD -- crates/ spec/`.

**A concurrent session commits to `spikes/`.** Re-check `HEAD` before
committing; never `git reset`, `git restore --staged`, `git checkout`, or
`git stash` against the shared index.

## §6. Boundary — unchanged and absolute

MUST NOT be read, written, or staged: `spec/PLAN_EDITOR_APP.md`,
`spec/CONTRACT_EDITOR_*.md`, `spec/ANALYSIS_GENESIS_PERSISTENCE.md`,
`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`, `spec/DRAFT_T4_FIXTURE_RECIPE.md`,
`crates/epiphany-editor-gui/goldens/*.png`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the entire `spikes/`
tree, the root `Cargo.toml`, `.claude/worktrees/`.

## §7. Report requirements

- The observed `MeasureMeterMismatch` refusal, with the effect value and the
  test name — and the pickup's own `Applied` alongside it.
- The mid-score observation for pin 2.
- All **4** mutations with their observed failing test names.
- Both byte-identical-body commands and their output.
- The corrected text of **all seven** surfaces (a–g), quoted, with row **g**'s
  false claim shown before and after.
- Confirmation that `crates/epiphany-core/DECISIONS.md` was **not** touched.
- Which form pin 4's pickup used — `time_signature: None` or a matching
  signature — and confirmation that the observed refusal is clause 3's, not
  clause 2's.
- `operation_catalog.pdf`'s title-page version as rendered.
- Whether pin 8's workaround was executed, and if not, confirmation that no
  corrected text names one.
- **Anything the contract did not anticipate** — a fifth understated surface, a
  behaviour that contradicts pin 1, or anything suggesting the current
  behaviour is wrong rather than undocumented. Report it and **stop**.
