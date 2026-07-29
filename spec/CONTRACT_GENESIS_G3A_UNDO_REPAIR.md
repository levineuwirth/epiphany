# Contract: the G3a undo repair, and the Binary Format chronology restoration

**Status:** **RATIFIED 2026-07-29.** Two packets, two commits. Pins A3
(ungated ledger/map guards), A6 (no restoration lookup while those fields
have no write chain — revisit only if a modifying operation is introduced),
and A7 (retaining the maps is safe given A4's live-state filter and
object-first recreate refusal) were ratified explicitly. Mutation budget
ratified at twenty-two for Packet A and three for Packet B.
**Governs:** the defect that blocks G3a sign-off (Packet A), and P13-S17
(Packet B).
**Predecessor:** `spec/CONTRACT_GENESIS_G3A_ENTITIES.md`, landed at `6c5e69f`.

---

## §0. What was actually verified

Every claim below was read out of the code, not relayed.

**The tombstone branch is reachable.** `mint_container`
(`crates/epiphany-ops/src/reduce.rs:3922`) calls `note_minted`
(`:7848`), which pushes the object into `tx_minted` whenever
`self.current_tx` is set. `undo_transaction` (`:5288`) reads
`tx_minted` and `tombstone_undo_targets` (`:5387`) writes
`ObjectState::Tombstoned` into `objects` for every target. All four G3a
reducers reach `mint_container`: `create_staff_group` `:4283`,
`create_part_definition` `:4340`, `create_analysis_layer` `:4384`,
`create_view` `:4437`. `create_instrument` `:4214` and `create_staff`
`:4165` do the same.

Therefore `crates/epiphany-ops/DECISIONS.md:1986` — "`ObjectState::Tombstoned`
is unreachable for any of them through the public operation API today" — is
**false**, and was false for `CreateStaff` and `CreateInstrument` before this
rung. It is a pre-existing wrong claim that G3a inherited and repeated.

**The graph keeps ghosts.** `materialize_graph_tombstones`
(`:2737`) walks `targets` at `:2768` with arms for `Pitch`, `Voice`,
`Slur`, `Tie`, `Beam`, `Spanner`, `RepeatStructure`, `Staff`,
`TimeSignature`, then `_ => {}` at `:2810`. There is no arm for
`StaffGroup`, `PartDefinition`, `AnalysisLayer`, `View`, or `Instrument`.
The ledger tombstones; `Score.staff_groups` / `.parts` / `.analysis_layers`
/ `.views` / `.instruments` keep the value. Ledger and graph disagree —
the exact failure the `Spanner` arm's own comment at `:2793` records as
having been fixed once already.

**The strand guard is silent on the new families.** `undo_strand_block`
(`:5458`) matches `Staff` and `TimeSignature`, then `_ => None` at
`:5500`. Undoing a `StaffGroup` mint while a live `Staff.group`
(`crates/epiphany-core/src/graph.rs:825`) names it strands the reference;
likewise `AnalysisLayer` ← `ViewDefinition.active_layers` (`graph.rs:1652`)
and `Instrument` ← `Staff.instrument` (`graph.rs:817`).

`PartDefinition` and `View` need no guard: nothing in the graph references
them. Their own fields (`PartDefinition.staves` `graph.rs:1636`,
`ViewDefinition.active_layers`) are **outbound**, and removing the holder
strands nothing.

**Count check.** `spec/PLAN_GMINOR_SCHEMA_MINOR.md:198` says "all ten
kind/tag pairs." The epoch table at `:170`–`:180` lists 24–27 (4), 28–29
(2), 30 (1), 31 (1), 32–33 (2), 34 (1), 35–38 (4) = **fifteen**.

**Binary Format chronology.** `spec/binary_format.tex` history rows: G2a
0.12.0 (`:3599`), G-minor 0.13.0 (`:3628`), G3a 0.14.0 (`:3643`). The
ladder order is G1 → G2a → G-minor → G2b → G3a, so **G2b is missing**
between 0.13.0 and G3a's row, and the accept-set raise
`OperationEnvelopeBlock` 2→3 that G2b performed is recorded nowhere in the
revision history — G3a's row at `:3657` merely observes the block "stays at
3 where genesis tranche G2b left it."

**G1 has no standalone history row, by design.** It is recorded
retroactively *inside* the G2a row: the principal marker is at `:3599`
(`0.12.0 --- Genesis tranche G2a`), and G1 appears nested at `:3603` as
"(`CreateInstrument`, genesis tranche G1 --- landed at `3b09595` with no
matching entry here)". The row is explicit that G1 never got its own entry.
Pin B6 is scoped accordingly.

---

# PACKET A — the undo repair

## §A1. Pins

**Pin A1 — five graph-removal arms.** In `materialize_graph_tombstones`
(`reduce.rs:2768`), before the `_ => {}` catch-all, add:

| Target | Removal |
|---|---|
| `TypedObjectId::StaffGroup(id)` | `score.staff_groups.retain(\|v\| v.id != *id)` |
| `TypedObjectId::PartDefinition(id)` | `score.parts.retain(\|v\| v.id != *id)` |
| `TypedObjectId::AnalysisLayer(id)` | `score.analysis_layers.retain(\|v\| v.id != *id)` |
| `TypedObjectId::View(id)` | `score.views.retain(\|v\| v.id != *id)` |
| `TypedObjectId::Instrument(id)` | `score.instruments.retain(\|v\| v.id != *id)` |

Each mirrors the existing `Staff` (`:2804`) and `TimeSignature` (`:2807`)
arms exactly. **No new `RepairRecord`**: `tombstone_undo_targets` already
pushes one `CascadeDeleted` per target at `:5427` before calling this
function, and none of the existing arms push more.

**Pin A2 — three inbound-reference guards.** In `undo_strand_block`
(`:5458`), before `_ => None`:

| Target | Blocked by | Read from |
|---|---|---|
| `StaffGroup(g)` | a live `Staff` whose `group == Some(g)` | `self.staff_values` |
| `AnalysisLayer(l)` | a live `ViewDefinition` whose `active_layers` contains `l` | `self.view_values` |
| `Instrument(i)` | a live `Staff` whose `instrument == i` | `self.staff_values` |

Return `Some((*target, referencer_obj))` on the first match in map order.

**Pin A3 — the guards are ledger-based and ungated.** They read the
carried-value maps (`staff_values` `:1001`, `view_values` `:1018`), **not**
`self.graph`, and are **not** wrapped in `if self.graph.is_some()`.

Rationale, and it is a real decision: the *create*-side referential
preconditions **are** graph-gated (`create_staff:4137`,
`create_staff_group:4266`, `create_view:4420`) because base-free reduction
has no universe to resolve against. The *undo* side is different — the
carried-value maps are populated by mints regardless of graph presence and
are seeded from base when there is one, so under base-free reduction the
ledger still knows that a live staff names this group. Gating the guard
would let base-free undo strand a reference the ledger can plainly see.
The two existing guards (`instance_staff`, `meter_change_chain`) are
likewise ungated. **Stronger than the create side, and strictly safer.**

**Pin A4 — liveness is read from `objects`, not from map presence.** The
value maps are insert-only; `tombstone_undo_targets` never removes from
them. A guard that treated map presence as liveness would let an
already-tombstoned staff block an unrelated undo forever. Each guard MUST
require `matches!(self.objects.get(&referencer_obj), Some(ObjectState::Live))`.

**Pin A5 — same-transaction referencers do not block.** Each guard MUST
also require `!targets.contains(&referencer_obj)`, matching the `Staff`
guard's `!targets.contains(&iobj)` at `:5471` and the doc comment at
`:5456`. A transaction that mints a group and a staff in it must be
undoable whole.

**Pin A6 — `restorations` is not consulted by the three new guards.** The
`TimeSignature` guard consults it (`:5483`) because `MeterChange` has a
write chain that undo may restore to a prior value. `Staff.group`,
`Staff.instrument` and `ViewDefinition.active_layers` have **no modify
operation at all** — that is the standing §1.1 condition — so no
restoration can change the prospective post-undo value. The implementer
MUST NOT cargo-cult the restoration lookup; its presence would be dead
code asserting a write chain that does not exist.

**Pin A7 — the value maps are not pruned on undo.** `tombstone_undo_targets`
leaves `staff_group_values` and siblings untouched, matching the existing
`staff_values` / `time_signature_values` / `instrument_values` precedent.
This is safe **because every create reducer consults `self.objects` before
its value map**. The `match self.objects.get(..)` lines, not the function
declarations: `create_staff_group` `:4241`, `create_part_definition`
`:4298`, `create_analysis_layer` `:4356`, `create_view` `:4395`,
`create_staff` `:4109`, `create_instrument` `:4186`. A re-create after undo
therefore hits `Some(ObjectState::Tombstoned)` → `TargetTombstoned` and
never reaches the stale value. Row family **u4** signs that ordering, and it
is the only family that does — a re-create assertion bolted onto u1 would be
unsigned regression coverage, since u1's arm-deletion mutation cannot reach
it (§A3).

**Pin A8 — Instrument is in scope as collateral.** It is the same root
cause, it already has normative undo semantics, and leaving it would mean
knowingly shipping a fifth instance of a defect this packet exists to fix.

**Pin A9 — no new `OperationKind`, tag, epoch, discriminant, wire layout,
accept-set, or companion version.** This packet changes reducer behaviour
only. `spec/binary_format.tex` is **not** touched by Packet A. If any of
those surfaces appears in the diff, the packet is wrong.

**Pin A10 — DECISIONS.md correction of record.** Replace the false
paragraph at `crates/epiphany-ops/DECISIONS.md:1986` with a correction that
states plainly: the branch **is** reachable via `UndoTransaction` over a
transaction containing the mint; it was reachable for `CreateStaff` and
`CreateInstrument` before G3a; the claim as written was wrong; and the
guards and removal arms this packet adds are what make it correct. Do not
silently delete it — the wrong claim was signed off and the record must say
so.

## §A2. Touch table (Packet A)

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-ops/src/reduce.rs` | five arms in `materialize_graph_tombstones` (pin A1) |
| 2 | `crates/epiphany-ops/src/reduce.rs` | three arms in `undo_strand_block` (pins A2–A6) |
| 3 | `crates/epiphany-ops/src/reduce.rs` | doc comment on `undo_strand_block` `:5452` naming the three new blocks |
| 4 | `crates/epiphany-ops/src/reduce.rs` | test rows u1a–u1e, u2a–u2c, u2bf-a–c, u2tomb-a–c, u3a–u3c, u4a–u4e |
| 5 | `crates/epiphany-ops/DECISIONS.md` | pin A10 correction; record the five arms, three guards, and pin A3's ruling |
| 6 | `spec/PLAN_GENESIS_OPS.md:25` | "commit pending" → `6c5e69f`; note the undo repair rides after it |
| 7 | `spec/PLAN_GMINOR_SCHEMA_MINOR.md:190` | "introducing commit pending" → `6c5e69f` |
| 8 | `spec/PLAN_GMINOR_SCHEMA_MINOR.md:198` | "all ten kind/tag pairs" → "all fifteen kind/tag pairs" |
| 9 | `spec/CONTRACT_GENESIS_G3A_UNDO_REPAIR.md` | **this file** — status line DRAFT → RATIFIED, dated, naming the ratifying decision on pins A3/A6/A7 |

**Row 9 exists because the contract governing a commit must land with it.**
The file is currently untracked; without this row it would sit outside both
packets while §A2 says "nothing else" and §E permits staging only
touch-table files — a rule that excluded the rule-book. It joins Packet A.

**Nothing else.** No `epiphany-bundle`, no `epiphany-core`, no spec `.tex`,
no editor-track file, no `spikes/` entry.

## §A3. Test rows

Every row names the mutation that MUST be **observed** to fail. Reasoning
that a mutation "would fail" does not sign a row — the G3a rung already
produced two such unsigned claims, both of which I had to run myself.

**Twenty-two mutations in Packet A, three in Packet B: twenty-five total.**
The six row families below are u1 (5), u2 (3), u2bf (3), u2tomb (3),
u3 (3), u4 (5).

### u1a–u1e — removal and tombstoning (5 mutations)

One row per family: `StaffGroup`, `PartDefinition`, `AnalysisLayer`,
`View`, `Instrument`. Each mints the object inside a declared transaction
against a non-empty base, undoes the transaction with `StrictInverse`, and
asserts:

- (i) the value is **gone** from the corresponding `Score` vector;
- (ii) `objects` reports `ObjectState::Tombstoned` for its `TypedObjectId`.

> **Killing mutation, five independent runs:** delete that family's arm from
> pin A1's table. Assertion (i) must fail. Deleting one arm must not be
> observed to kill another family's row — run them one at a time and record
> five separate observations.

**These rows do not sign pin A7.** A re-create assertion may be included as
regression coverage, but the arm-deletion mutation says nothing about it;
the ordering pin is signed by u4 below, and nowhere else.

### u2a–u2c — a live outside referencer blocks the undo (3 mutations)

Graph-aware (`reduce_operation_set_onto`). In each, transaction T mints the
referent; a **separate, non-transactional** op mints the referencer; undo T
under `StrictInverse` MUST be `Conflicted` and the referent MUST remain in
its `Score` vector.

| Row | T mints | Outside op mints |
|---|---|---|
| u2a | `StaffGroup g` | `Staff s` with `group: Some(g)` |
| u2b | `AnalysisLayer l` | `ViewDefinition v` with `active_layers` ⊇ `[l]` |
| u2c | `Instrument i` | `Staff s` with `instrument: i` |

> **Killing mutation, three independent runs:** delete that family's arm
> from pin A2. The effect becomes `Applied` and the reference strands.

### u2bf-a–u2bf-c — the guards hold base-free (3 mutations) — signs pin A3

The same three shapes reduced through **`reduce_operation_set`**
(`reduce.rs:674`), with no base `Score`. Assert the effect is `Conflicted`.
There is no graph to assert against — `MaterializedState` carries no score —
so these rows assert the effect and the `objects` state only.

Base-free reduction skips every create-side referential precondition
(`create_staff:4137`, `create_view:4420`), so the referencer mints
unconditionally and its value lands in `staff_values` / `view_values`
regardless. That is exactly the situation pin A3 exists to cover: the ledger
plainly knows the reference, and the undo must refuse.

> **Killing mutation, three independent runs:** wrap that family's guard arm
> in `if self.graph.is_some() { … } else { None }`. The u2bf row must go red
> **while its u2 counterpart stays green** — that divergence is the whole
> signature. Record both observations per run.

### u2tomb-a–u2tomb-c — a tombstoned referencer does not block (3 mutations) — signs pin A4

Graph-aware. Two transactions, undone in reverse order:

1. T1 mints the referent (`g` / `l` / `i`).
2. T2 mints the referencer naming it (`s` / `v` / `s`).
3. Undo **T2** — the referencer is tombstoned in `objects`, leaves its
   `Score` vector, and by pin A7 **its value is retained** in
   `staff_values` / `view_values`.
4. Undo **T1** — MUST be `Applied` (or `AppliedWithRepair`), and the
   referent MUST leave its `Score` vector.

Step 3 is what creates the state pin A4 guards against: a value-map entry
present while its `objects` state is `Tombstoned`.

> **Killing mutation, three independent runs:** delete the
> `matches!(self.objects.get(&…), Some(ObjectState::Live))` conjunct from
> that family's guard. The dead referencer blocks, step 4 becomes
> `Conflicted`, and the row goes red.

### u3a–u3c — same-transaction teardown is allowed (3 mutations) — signs pin A5

The mirror of u2a–u2c: put **both** the referent and the referencer inside
T. Undo T MUST be `Applied` (or `AppliedWithRepair`), and **both** values
MUST leave their `Score` vectors.

> **Killing mutation, three independent runs:** drop the
> `!targets.contains(…)` conjunct from that family's guard. The undo becomes
> `Conflicted`.

### u4a–u4e — objects outranks the retained value map (5 mutations) — signs pin A7

One row per family. Take u1's state (minted in T, T undone, so the object is
`Tombstoned` while its value map still holds the value by pin A7), then
apply a **byte-identical re-carry** of the original `Create…`. It MUST yield
`NoOpReason::TargetTombstoned` — **not** `AlreadyApplied`.

Byte-identical is the load-bearing choice: it is the one carried value for
which the retained map would return `identical == true` and produce
`AlreadyApplied`, so it is the only re-carry that can distinguish the two
orderings. A differing re-carry would yield a precondition no-op either way
and sign nothing.

> **Killing mutation, five independent runs:** in that family's reducer,
> make the `Some(ObjectState::Tombstoned { .. })` arm fall through to the
> value-map identity check instead of returning `TargetTombstoned` — i.e.
> reorder so the retained value overrides the tombstone. The row must
> observe `AlreadyApplied` and go red.

### Row-construction note

`create_staff` preconditions instrument liveness at `:4137` and group
liveness at `:4149`; `create_view` preconditions layer liveness at `:4420`.
All are graph-gated, so they bind in the u2 / u2tomb / u3 / u4 rows and are
skipped in u2bf.

- **u2a, u3a, u2tomb-a** need a base carrying a live `Instrument`: the
  referencing `Staff` requires one, and the transaction under test mints a
  `StaffGroup`, not an instrument.
- **u2c, u3c, u2tomb-c** need **no** base instrument — the instrument the
  staff names is the one their own transaction mints.
- **u2b, u3b, u2tomb-b** need no base entity at all; `AnalysisLayer` and
  `ViewDefinition` reference nothing outside the pair.

In every graph-aware row the referent must be minted **before** the
referencing op in accepted order. A base-ingested instrument is `Live` in
`objects` through `seed_from_graph` without any mint.

### Anti-traps

A mutation that does not compile signs nothing. A mutation in an op that
runs *before* the state under assertion cannot reach it — the t8b defect
from G3a review. A mutation that leaves the row green signs nothing, and
must be reported as such rather than reasoned around. Confirm each mutation
produces a **red test**, then restore by editing the source back, never by
`git checkout` or `git stash`.

## §A4. Gate (Packet A)

- `cargo test --workspace` — full pass, count reported and compared to the
  1429 baseline at `6c5e69f`, with the delta explained by the new rows.
- `cargo clippy --workspace --all-targets` — zero warnings.
- `cargo fmt --check` — clean.
- Whitespace, per §C — **`git diff --cached --check` after staging**, then
  the scoped committed-range check after the commit.
- All **twenty-two** Packet A mutations **observed** red and restored, each
  reported with its actual failing test name. u2bf additionally reports its
  paired u2 row staying green.

---

# PACKET B — P13-S17, the Binary Format chronology

Separate commit. The stack is unpublished, so the true chronology can be
restored rather than patched over.

## §B1. Pins

**Pin B1 — file P13-S17** in `spec/PASS13_CANDIDATES.md`: *Binary Format
revision history omitted genesis tranche G2b; the accept-set raise
`OperationEnvelopeBlock` 2→3 reached the normative tables but never the
history.* Record that G2b's own contract touch row 27 required "version,
Revision History row" and that the rung was signed off without it — the
gate did not catch a documentation MUST because nothing tests the history.

**P13-S17 lands RESOLVED, in the same commit that files it.** Its ledger
disposition is not left open: pins B2–B4 restore the chronology and pins
B5–B6 add the guard that makes the omission recurrence-detectable, so the
entry is filed and closed by Packet B itself. It is filed rather than merely
fixed because the candidate ledger is the record of *how the gate failed*,
and a silent repair would erase that. Contrast **P13-S15** and **P13-S16**,
which remain open by design because their fixes are sequenced to later
rungs.

**Pin B2 — restore the chronology.** Ladder order is G1 → G2a → G-minor →
G2b → G3a, so:

| Document version | Event | Where |
|---|---|---|
| 0.12.0 | G2a | `binary_format.tex:3599`, unchanged |
| 0.13.0 | G-minor | `:3628`, unchanged |
| **0.14.0** | **G2b — new row** | inserted after the G-minor row |
| **0.15.0** | G3a | `:3643`, renumbered from 0.14.0 |

The title line at `:243` moves to **0.15.0** with a description matching
G3a (it already names G3a; only the number changes).

**Pin B3 — the G2b row must state what G2b actually did.** Read
`spec/CONTRACT_GENESIS_G2B_TUNING.md` and commit `13c3d2f` and write the
row from them, not from memory. It must name at minimum: `OperationKind` /
`OperationKindTag` **34** (`SetTuningContext`), epoch **10**, the payload as
the five-field subset `epiphany_core::TuningContextSettings` rather than the
full graph type, and — the omission that motivates this packet — the
accept-set raise **`OperationEnvelopeBlock` 2→3**, the first accept-set move
since G2a explicitly recorded staying at 2.

**Pin B4 — regenerate `spec/binary_format.pdf`** from the amended source,
using the repository's existing build path.

**Pin B5 — a scoped history guard, in `epiphany-testkit`.** A new test that
reads `spec/binary_format.tex` and **slices to the
`\chapter{Revision History}` section only**. Follow the loading precedent in
`crates/epiphany-testkit/tests/requirement_labels.rs` (`std::fs` from a path
relative to the manifest dir) or `text_projection_grammar.rs`'s
`include_str!` — either, but state which.

**Pin B6 — presence of a rung *name* is not enough, and version numbers are
forbidden.** Two constraints that pull against each other, and the guard
must satisfy both.

*Why bare name-presence fails:* "G2b" already occurs **inside the Revision
History chapter** at `binary_format.tex:3657`, in the G3a row's sentence
"`OperationEnvelopeBlock` stays at 3 where genesis tranche G2b left it." A
guard asserting only that "G2b" appears in the slice stays **green after the
new row is deleted**. It would be born dead.

*Why version literals are also wrong:* encoding "0.14.0" would pin a number
this packet is itself moving, and the next chronology correction would have
to edit the guard — the stale hand-maintained parallel list failure
(`PLAN_GMINOR_SCHEMA_MINOR.md:156`).

*Which rungs have principal markers:* **G2a, G-minor, G2b, G3a — not G1.**
G1 has no standalone row and this packet does not authorize inventing one
(pin B2 lists exactly four rows, one of them new). G1 is recorded
retroactively inside the G2a row at `:3603`, and that row states outright
that G1 landed "with no matching entry here." A guard demanding a G1
principal marker would be **born red** against a document B2 leaves
correct — the guard would be wrong, not the spec.

The guard MUST therefore assert:

1. a **distinct principal marker** for each of **G2a, G-minor, G2b, G3a**,
   matching the row form — the rung name immediately preceded by the row's
   `---` separator, e.g. `--- Genesis tranche G2b`. Prose mentions cannot
   satisfy this: `:3657` says "where genesis tranche G2b left it" and
   `:3603` says "genesis tranche G1 --- landed at", where the separator
   *follows* the name rather than preceding it.
2. **ordering** — marker offsets strictly increasing within the slice:
   `G2a < G-minor < G2b < G3a`.
3. at least one **B3-specific content anchor** for the G2b row —
   `SetTuningContext`, discriminant `34`, and the accept-set raise to
   `OperationEnvelopeBlock` `3` — searched **only within the G2b row
   segment**, i.e. the span from the G2b marker to the next marker (or the
   slice end). Unbounded searching would let G3a's row, which names both
   `OperationEnvelopeBlock` `3` and G2b, satisfy the anchor after the G2b
   row is deleted — reintroducing the exact hole this pin exists to close.

**G1 is deliberately unguarded.** Record that in the test's own comment,
citing `:3603`, so a later reader does not "fix" the omission by adding a
fifth marker assertion and rediscovering this contradiction.

> **Killing mutations, three independent runs:**
> (a) delete the newly added G2b row entirely — the marker assertion goes
> red, *and* it must be confirmed that a name-only guard would have stayed
> green here, which is the finding this pin encodes;
> (b) strip the accept-set-raise clause from the G2b row, leaving its
> marker — the content anchor goes red;
> (c) move the G2b row after the G3a row — the ordering assertion goes red.
>
> All three MUST be **observed**. Packet B therefore carries **three**
> mutations, not one.

## §B2. Touch table (Packet B)

| # | File | Change |
|---|---|---|
| 1 | `spec/PASS13_CANDIDATES.md` | P13-S17 (pin B1) |
| 2 | `spec/binary_format.tex` | new G2b row; G3a 0.14.0 → 0.15.0; title `:243` → 0.15.0 |
| 3 | `spec/binary_format.pdf` | regenerated |
| 4 | `crates/epiphany-testkit/tests/` | the scoped history guard (pins B5–B6) |

**Nothing else.** Packet B touches no reducer and no crate but the testkit.

## §B3. Gate (Packet B)

- Full workspace test, clippy, fmt, and the §C whitespace checks as in §A4.
- All **three** pin-B5/B6 mutations **observed** red and restored,
  including the confirmation under (a) that a name-only guard would have
  stayed green.
- The regenerated PDF's title page reads 0.15.0.

---

## §C. Whitespace checking — the hole this packet closes

`git diff --check -- crates/ spec/` is what the G3a rung used, and it is
**blind to untracked files**: it reported clean while this very contract sat
untracked. It is also blind to what is already staged.

Per packet, in order:

1. Stage the touch-table files explicitly. Never `git add -A`.
2. **`git diff --cached --check`** — catches whitespace in exactly what is
   about to be committed, untracked-and-now-staged files included.
3. Commit.
4. **`git diff --check <parent>..HEAD -- crates/ spec/`** — a scoped
   committed-range check confirming the landed commit is clean.

Step 4 is deliberately path-scoped. The unpushed range fails
`git diff --check` at `spikes/editor-toolkit/round1-oracle/ORACLE_SUMMARY.md:147`,
an editor-track file outside this packet's authorization. Scoping keeps that
pre-existing failure from masking a real one in `crates/` or `spec/`, and
keeps this packet from being tempted to "fix" a file it must not touch.

---

## §D. Report requirements (both packets)

State, per packet: the test count before and after; every mutation with the
**observed** failing test name; anything found that the contract did not
anticipate. If a pin turns out to be wrong or unsatisfiable, **stop and say
so** rather than working around it — pin A3 and pin A6 are rulings, not
suggestions, and pin A6 in particular forbids an addition that would look
like diligence.

## §E. Boundary — unchanged and absolute

These MUST NOT be read, written, or staged: `spec/PLAN_EDITOR_APP.md`,
`spec/CONTRACT_EDITOR_*.md`, `spec/ANALYSIS_GENESIS_PERSISTENCE.md`,
`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`, `spec/DRAFT_T4_FIXTURE_RECIPE.md`,
`crates/epiphany-editor-gui/goldens/*.png`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the entire `spikes/`
tree, the unstaged root `Cargo.toml` change, and `.claude/worktrees/`.

The narrow editor authorization granted for the G3a packet
(`epiphany-editor-core/src/barriers.rs`, `epiphany-layout-ir/src/barrier.rs`)
was spent by that packet and **does not carry forward**. This packet
authorizes no editor-crate change of any kind.

Stage only the files in the two touch tables, explicitly. Never `git add -A`.
