# Contract — P13-S18: the invariant-20 outcome matrix

**Status:** DRAFT.

**Rung type:** diagnostic and bookkeeping. **No behaviour change.** No graph that
violates invariant 20 today may stop violating it, and no graph that passes today
may start violating it. If the work suggests otherwise, **stop** (§1 pin 6).

---

## §0. What was verified before drafting

Every claim below was read out of the working tree at `cc49533`, not recalled,
and re-confirmed unaffected at `f33673d` (see §4).

1. **`check_measure_meter_consistency` has nine non-success paths**, not the five
   an earlier scoping note claimed. Enumerated with line numbers in pin 1.
2. **Only three are abstentions.** The other six are one inapplicability, two
   delegations, two vacuities, and one separately-filed deferral (pin 2).
3. **Invariant 10 really does own the two delegated paths.**
   `invariants.rs:1220`ff checks that time-signature references resolve *"at
   every level a `MeterChange` can appear"* and its loops cover **per-measure**
   (`si.measures`, emitting `"measure {:?} time signature {:?} is not
   declared"`), **instance-local grids** (`si.local_metric_grid`), the
   **region-default grid**, and the **metric time model's own meter sequence**.
4. **P13-S18's "`Measure` *end* anchor" claim is wrong only in its "any", and
   two successive scoping notes each overcorrected it.** The first called the
   claim simply false; the second called it merely misattributed. The precise
   position has three parts, all read out of `measure20_comparable_order`:
   - **Same id, same position — `End`↔`End` is comparable *when its offsets
     compare*.** c2 requires `ia == ib && pa == pb` and then delegates to
     `measure20_offset_order`, which returns `None` for `Musical`↔`WallClock`
     (`:2427`–`:2428`). So "any `Measure` end anchor" is false as written, but
     the counter-example is conditional and must be stated that way.
   - **Distinct ids — `End` anchors remain incomparable**, because c3 returns
     `None` unless `*pa == MeasurePosition::Start` with both offsets `Zero`. A
     vector index orders measure *reference points*, not arbitrary points near
     them. Distinct-id `End` anchors therefore genuinely do reach **A4, B4 and
     B5**, and are a real residue shape.
   - **The resolver citation describes the missing duration machinery, not
     invariant 20's execution path.** `resolve_anchor`'s `Measure` arm
     (`invariants.rs:503`–`:516`) returns `None` for any `position !=
     MeasurePosition::Start` because a measure's length "needs the deferred
     decomposition/tempo machinery" — a true statement about *why* the duration
     is unavailable. **Invariant 20 never calls it: zero occurrences in
     `:2636`–`:2725`.** The citation explains the underlying gap; it does not
     describe how invariant 20 reaches its abstentions.

   The `invariants.rs:400` line number is separately stale (§0.7).
5. **P11-C5 is not P13-S18's gate.** `PASS11_WORKLIST.md:159` defines it as the
   *"nearest surviving anchor" stand-in* — a re-anchoring proximity metric that
   *"resolves when the graph-mutation phase tracks resolved positions"*. The G3b
   contract cites it correctly but **narrowly**, for the two-distinct-`Event`s
   case, via `PositionOutsideRegion`'s Reserved note (`effect.rs:139`–`:142`).
   The capability P13-S18 waits on — placing on a common timeline **any pair
   c1–c5 cannot order, and any pair it orders without yielding a usable musical
   delta** — is strictly broader and is owned by **no filed candidate**. Pin 10
   is the authoritative scoping; this item must not restate it more narrowly.
6. **Step 0 is per measure reference.** `measure20_governing_by_anchor` is
   called once per measure with that measure's own `start`, and
   `measure20_comparable_order` is a property of the *pair*. An incomparable
   meter change therefore disables agreement for every measure whose start is
   incomparable **to it** — not for every measure in the instance, when the
   instance's measures carry heterogeneous anchor shapes. An earlier scoping
   note said "every measure in that instance"; that was wrong and pin 7 locks
   the correction behaviourally rather than in prose.
7. **The G3b contract's `invariants.rs:400`ff citation (`:206`) has drifted.**
   The resolver it describes now begins at `invariants.rs:466`, with the
   `Measure` arm at `:503`–`:516`. The paragraph's own narrower claim — that
   cross-boundary comparison stays unverifiable without the boundary's
   duration — remains **correct** and is not touched (pin 9).

---

## §1. Pins

**Pin 1 — the nine paths, and their sites.** These are exhaustive over
`check_measure_meter_consistency` (`invariants.rs:2636`). Any tenth found during
execution is a finding, reported before proceeding.

| Id | Clause | Path | Site |
|---|---|---|---|
| A1 | agreement | `m.time_signature` is `None` | `:2661` |
| A2 | agreement | declared signature does not resolve | `:2662` |
| A3 | agreement | `Governing20::None` | `:2675` |
| A4 | agreement | `Governing20::Indeterminate` | `:2677` |
| B1 | boundary | first measure (`i == 0`) | `:2683` |
| B2 | boundary | governing signature does not resolve | `:2689`–`:2693` |
| B3 | boundary | `Governing20::None` | `:2717` |
| B4 | boundary | `Governing20::Indeterminate` | `:2719` |
| B5 | boundary | musical delta not computable | `:2713` |

**Pin 2 — the classification, ratified.** Six labels. Every path takes exactly
one.

| Id | Class | Why |
|---|---|---|
| A1 | **inapplicable** | no declared signature can disagree with anything; pin 9b makes `None` avoid *only* this clause, never the boundary clause |
| A2 | **delegated** | invariant 10, per-measure arm (§0.3) |
| A3 | **vacuous** | no governing signature exists to disagree with (pin 6c case 1) |
| A4 | **genuine abstention** | the relation cannot place a candidate |
| B1 | **P13-S19** | the pickup/anacrusis deferral, already filed |
| B2 | **delegated** | invariant 10, grid-level arms (§0.3) |
| B3 | **vacuous** | pin 6c case 1 |
| B4 | **genuine abstention** | as A4 |
| B5 | **genuine abstention** | order without distance |

**Exactly three genuine abstentions: A4, B4, B5.** This is the whole point of
the rung — P13-S18 currently presents all nine as one undifferentiated residue,
which both overstates the gap and hides which part of it is real.

**Pin 3 — delegation must be verified, not asserted.** A2 and B2 are classed
`delegated` only if some *other* invariant actually reports the condition. The
matrix must show, for each, that invariant 10 emits a violation on the same
graph — and M7/M8 must show that removing invariant 10's arm makes the condition
go **unreported by the whole suite**, not merely by invariant 20. A delegation
nobody discharges is an abstention wearing a better name.

**Pin 4 — the outcome matrix.** Rows are representative anchor shapes, columns
are the two clauses, each cell names the path id that fired and its class.
Minimum shapes:

| Shape | Description |
|---|---|
| S1 | `WallClock` start, `WallClock`-anchored meter changes |
| S2 | `WallClock` start, `Region`-anchored meter changes |
| S3 | `Region` same id, same edge, `Musical` offsets — fully decidable |
| S4 | `Measure` **same id, `pos: End` on both sides**, `Musical` offsets (c2) |
| S5 | `Measure` distinct ids, `Start`, `Zero` (c3) |
| S6 | `Event` same id, **live** event, `Musical` offsets (c1) |
| S7 | `Event` distinct ids, otherwise identical to S6 |
| S8 | matching referent, **differing** `pos`/`edge` selector |
| S9 | one instance, **heterogeneous** measure anchors (pin 7) |

**S6 and S7 are a positive/negative pair and neither may be dropped.**
`Measure.start` and `MeterChange.anchor` are unrestricted `TimeAnchor`s, so
`Event`-anchored measures are valid rather than hypothetical. S6 proves such a
measure genuinely reaches c1. S7 changes **only the referent identity** and
isolates the distinct-`Event` fall-through — which is precisely the case the
retained P11-C5 citation at `CONTRACT_GENESIS_G3B_MEASURE.md:223` exists for.
Both use live events and `Musical` offsets.

**S4 is the row that falsifies "any `End`", and it must do so by observation.**
It is deliberately an `End`↔`End` pair on the **same** measure id with `Musical`
offsets — c2's exact shape — and its meter changes carry the same anchor form,
so both clauses reach a decision. It is therefore `D`/`D` and, per the rule
below, deliberately wrong. S5 supplies the contrast on distinct ids. A contract
that only *stated* "same-id `End` is fine" would be repeating the mistake this
pin exists to correct.

**Every cell holds either a path id (A1–B5) or `D`.** A fully comparable shape
takes *none* of the nine paths — it decides — and a cell left blank because
"nothing fired" is indistinguishable from a cell nobody looked at. **Every `D`
fixture must be deliberately wrong**, so that the emitted violation is what
proves the clause decided. S3 and S6 are `D` in both columns; an S6 that emits
nothing has demonstrated nothing.

Every cell must be **observed**, never derived. A cell filled in by reading the
match arms and reasoning about which one wins is unsigned.

**Pin 5 — the `WallClock` finding, and the split an earlier draft got wrong.**
`valuegen::measure` (`ops/src/valuegen.rs:447`) anchors starts to
`TimeAnchor::WallClock`. An earlier draft of this contract said that shape makes
**both** clauses abstain. It does not, and the difference is the meter changes'
anchors, not the measures':

- **S1 — `WallClock` measures, `WallClock` meter changes.** The anchors are
  mutually comparable under c5, so step 0 passes and a unique maximum is found:
  **agreement decides.** But `measure20_musical_delta` never returns a
  `WallClock` delta (`:2527`, `AnchorOffset::WallClock(_) => None`), so the
  boundary clause takes **B5**.
- **S2 — `WallClock` measures, `Region` meter changes.** `comparable_order`
  falls through to `_ => None`, so step 0 returns `Indeterminate`: agreement
  takes **A4** and the boundary clause takes **B4** — *not* B5, because the
  `Governing20` match short-circuits before the delta is ever computed.

**A `WallClock` start does not by itself disable both clauses.** The report must
state the split, must **not** claim `WallClock` is the dominant shape in real
scores (measure starts are author-supplied), and may claim only that it is the
shape this repository's fixture generator emits.

**Pin 6 — the hard stop.** No widening of `measure20_comparable_order` or
`measure20_musical_delta`. No new violation. No change to which graphs violate
invariant 20. The G3b contract holds that defining a new comparability relation
*"is a specification question this rung has no authority over"*, and that is
unchanged. **If the classification work suggests a behaviour change — including
"A3 should really be a violation" or "B2 should be reported here rather than by
10" — stop, report, and let it be split into a separately ratified semantic
rung.** Do not implement it here, and do not soften a class label to avoid
raising it.

**Pin 7 — step-0 scope, locked behaviourally.** S9 must contain one instance
whose measures carry heterogeneous anchor shapes, such that **one measure's
agreement clause abstains via A4 while another measure's agreement clause in the
same instance reaches a decision**. Prose asserting this is not enough; the row
is what forbids the "one incomparable change disables the whole instance"
overstatement from reappearing.

**Pin 8 — the P13-S18 correction.** Five defects, all of them mine:
1. three abstention shapes → **nine paths**, classified;
2. **correct**, not delete, the "`Measure` *end* anchor" claim, to all three
   parts of §0.4: same-id `End`↔`End` is comparable under c2 **when its offsets
   compare** — `measure20_offset_order` rejects `Musical`↔`WallClock` — so
   "any" is false but conditionally so; distinct-id `End` anchors **are**
   incomparable and do reach A4/B4/B5; and the resolver citation names the
   missing duration machinery rather than invariant 20's execution path;
3. re-gate: not P11-C5, but the broader capability filed as P13-S23 (pin 10);
4. name A2/B2 as delegated to invariant 10 with its line, so the entry stops
   counting them as gap;
5. state the S1 finding.
The entry stays **open** — A4, B4 and B5 remain real — but open at its true size.

**Pin 9 — the G3b contract corrections, two of them and both narrow.**

1. **`:347`** says the residue's closure needs "P11-C5 resolved positions". That
   is the overbroad claim. Correct it to name P13-S23 as the owner of the
   general case.
2. **`:206`** cites `invariants.rs:400`ff for the prototype anchor resolver. The
   resolver now begins at `:466` with its `Measure` arm at `:503`–`:516`.
   **Correct the line number only.** The paragraph's claim — that cross-boundary
   comparison stays unverifiable without the boundary's own duration — is
   correct, is narrower than P13-S18's use of it, and must survive verbatim.

**`:223`'s citation is correct and stays.** It cites P11-C5 for the
two-distinct-`Event`s case specifically, which is genuinely what
`PositionOutsideRegion`'s Reserved note (`effect.rs:139`–`:142`) covers.
**The contract is RATIFIED and its pins are not reopened**; both edits are
citation repairs, and neither changes a ratified claim.

**Pin 10 — P13-S23.** File the unowned capability: *placing anchor pairs on a
common timeline and measuring musical distance along it, wherever **c1–c5 do not
already yield both**.* Two disjoint deficiencies, and the candidate owns both:

1. **No ordering.** The pair is not comparable at all. The failure may be in the
   **referent** (distinct `Event` ids, distinct `Measure` ids outside c3's
   `Start`+`Zero` restriction), the **variant or selector** (`Event`↔`Measure`,
   `Measure`↔`Region`, differing `pos`/`edge`), or the **clock**
   (`Musical`↔`WallClock`, including inside `measure20_offset_order`). This is
   what A4 and B4 are made of.
2. **Ordering without a usable delta.** The pair *is* comparable and still
   yields no musical distance: **c3** supplies a vector index, which is an order
   and never a distance; **c5** compares two `WallClock`s, and
   `measure20_musical_delta` never returns a `WallClock` delta (`:2527`). This
   is what **B5** is made of.

**Scoping this as "not directly comparable under c1–c5" would have excluded B5
entirely** — S5 is c3-comparable and S1 is c5-comparable, and both reach B5 — so
the candidate would have disowned a third of the residue it is filed to own. An
earlier draft also said "anchors of differing shapes", which additionally
excluded the same-variant cases (distinct `Event`s, distinct `Measure`s).

Explicitly **broader than P11-C5**, with the distinction stated: P11-C5 is a
re-anchoring proximity metric that waits on resolved positions; P13-S23 is the
timeline itself. Name its dependents: invariant 20's A4/B4/B5, and
`PositionOutsideRegion`'s Reserved status. Filed **open**; no code owed.

---

## §2. Touch table

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-core/src/invariants.rs` | **tests only**, plus a doc-comment classification on `check_measure_meter_consistency` naming each of the nine paths and its class. The doc comment is not normative and changes no behaviour. **No production logic edit of any kind.** |
| 2 | `spec/PASS13_CANDIDATES.md` | P13-S18 corrected per pin 8; **P13-S23** filed per pin 10 |
| 3 | `spec/CONTRACT_GENESIS_G3B_MEASURE.md` | `:347` (overbroad P11-C5 claim) and `:206` (stale resolver line number), per pin 9. `:223` untouched |
| 4 | this contract | DRAFT → RATIFIED |

**No `.tex` change and no PDF.** The classification describes existing
behaviour; it makes no normative claim. **If the classification turns out to
contradict `core_spec.tex`'s invariant-20 text, that is a finding — stop and
report it** (pin 6), because a spec correction is a semantic rung.

**No `DECISIONS.md` entry.** Consistent with the P13-S15 ruling: test coverage
and ledger bookkeeping, not a semantic ruling.

---

## §3. Mutation plan

Every mutation is a specific edit to a specific site, each **observed** red and
restored by hand-editing back. Never `git checkout`, never `git stash`.

| M | Edit | Row it signs |
|---|---|---|
| M1 | make A4's graph comparable (swap the incomparable meter change's anchor to the measure's own shape) | A4 is an abstention, not a pass: the agreement violation **appears** |
| M2 | as M1 for B4 | B4 is an abstention |
| M3 | swap S5's two measure starts to `Region` same-id `Musical` offsets | B5 is an abstention: the delta becomes computable and the boundary violation appears |
| M4 | give A1's measure a resolving `time_signature` that disagrees | A1 is inapplicable, not concealing: the violation appears |
| M5 | make **S2**'s meter-change anchors `WallClock`, matching its measures | pin 5's split: S2's cell pair moves **A4/B4 → D/B5** — agreement stops abstaining and decides, and the boundary clause moves from indeterminate selection to incomputable delta. Writing this as "A4/B4 → B5" is wrong: A4 is an agreement path and cannot appear in the boundary column. **S2 is the target, not S1** — S1's agreement clause already decides, so no mutation of S1 could show it begin to |
| M6 | in S9, give the second measure the first's incomparable anchor shape | pin 7: its agreement clause stops deciding — and **only** then |
| M7 | delete invariant 10's per-measure time-signature arm (`:1250`ff) | A2 is delegated: the condition becomes **unreported by the whole suite** |
| M8 | delete invariant 10's instance-local-grid arm | B2 is delegated, same standard |
| M9 | make B1's measure a non-first measure | B1 is the P13-S19 deferral, not a silent pass |
| M10 | remove B3's emptying filter so a governing signature exists and disagrees | B3 is vacuity, not concealment |

**Ten mutations.** Report the observed count; any deviation is a finding.

**Anti-traps, all earned in this session.** A mutation that does not compile
signs nothing. A mutation in an operation that runs *before* the asserted state
cannot reach it. A guard written against an index that structurally cannot hold
the referent is born green. Transaction-block members reduce atomically at the
*first* member's canonical position. Envelope counter gaps make operations
permanently pending, which makes fixtures silently vacuous. **And the one this
rung is most exposed to: a matrix cell that agrees with the prediction is the
easiest place in the world to stop looking.** Each cell must fail loudly if the
path it names is not the path taken — assert the *outcome*, and construct the
graph so no other path could have produced it.

---

## §4. Gate

- `cargo test --workspace` — baseline **1541 / 0**, confirmed at `f33673d`
  (the commits between `cc49533` and it are `spikes/`-only, a separate
  workspace, so the root count is unmoved); delta explained.
- `cargo clippy --workspace --all-targets` — zero warnings.
- `cargo fmt -p epiphany-core -- --check` — clean. **Never `cargo fmt --all`**;
  it crosses into the `spikes/` workspace through path dependencies.
- All **10** mutations observed red and hand-restored.
- **A diff check that `check_measure_meter_consistency`'s executable body is
  byte-identical to `cc49533`** — the doc comment may grow; not one line of
  logic may move. This is the rung's central claim and must be mechanically
  demonstrated, not asserted.
- `git diff --cached --check` clean; nothing from `spikes/` or `.claude/`
  staged.

## §5. Whitespace and staging

1. Stage the touch-table files explicitly. **Never `git add -A`.**
2. `git diff --cached --check` — catches staged and formerly-untracked files.
3. Commit.
4. `git diff --check <parent>..HEAD -- crates/ spec/` — path-scoped.

**A concurrent session commits to this repository.** Re-check `HEAD` before
committing, and never run `git reset`, `git restore --staged`, `git checkout`,
or `git stash` against the shared index.

## §6. Boundary — unchanged and absolute

MUST NOT be read, written, or staged: `spec/PLAN_EDITOR_APP.md`,
`spec/CONTRACT_EDITOR_*.md`, `spec/ANALYSIS_GENESIS_PERSISTENCE.md`,
`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`, `spec/DRAFT_T4_FIXTURE_RECIPE.md`,
`crates/epiphany-editor-gui/goldens/*.png`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the entire `spikes/`
tree, the root `Cargo.toml` change, `.claude/worktrees/`.

## §7. Report requirements

- The **full matrix — nine shapes × two clauses, 18 cells** — every cell
  observed, each holding a path id or `D`, with the test name that observed it.
- All **10** mutations with their observed failing test names.
- Explicit confirmation that `check_measure_meter_consistency`'s logic is
  byte-identical, with the command that showed it.
- Explicit confirmation that **no new violation** is emitted anywhere, and that
  the workspace count moved only by the tests added.
- For A2 and B2: the invariant-10 violation text observed, and the M7/M8 result
  showing the condition otherwise goes unreported.
- **Anything the contract did not anticipate** — a tenth path, a cell that
  contradicts pin 2, or anything suggesting a behaviour change. Report it and
  **stop**; do not implement it.
