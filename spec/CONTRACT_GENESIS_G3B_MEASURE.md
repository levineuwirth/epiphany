# Contract: genesis tranche G3b — `CreateMeasure`, invariant 20, and the close of the genesis ladder

**Status:** RATIFIED.
**Governs:** the final rung of `spec/PLAN_GENESIS_OPS.md`. One kind, one
invariant, three precondition reasons, one epoch.
**Predecessors:** G3a `6c5e69f`; the G3a undo repair `4b0abaf`; P13-S17
`6170015`.

---

## §0. What was verified before drafting

Read out of the tree, not assumed. Every number below was checked.

**`Measure` is a nested container child, not a root-level entity.** It lives
in `StaffInstance.measures` (`crates/epiphany-core/src/graph.rs:611`), not in
a `Score` root vector. **Every one of G3a's four families was root-level**, so
G3a's shape does not transfer: G3b's precedent is `CreateStaffInstance`
(`payload.rs:1510`, reducer `reduce.rs:4026`), which carries a parent id
beside the value:

```rust
pub struct CreateStaffInstanceOp {
    pub region: RegionId,
    pub instance: StaffInstance,
}
```

**`Measure` is schema major 0, so G3b moves no wire bound.** `struct_codec!(Measure { id, start, time_signature, explicit_number, number_visibility })`
at `codec.rs:1825` is a plain unversioned walk, and its only non-scalar field
`start: TimeAnchor` has a hand-written `Codec` (`codec.rs:750`–`:800`) with
**no version branching of any kind** — I read both. `MeasureNumberVisibility`
is a `cstyle_enum_codec!` (`codec.rs:1526`). Therefore
`OperationKind::schema_major()` (`payload.rs:311`) gains **no arm**;
`CreateMeasure` falls through `_ => 0`, and `OperationEnvelopeBlock` stays at
**3** where G2b left it. G3b is *not* a G2b-shaped rung.

**No id-vocabulary append.** `TypedObjectId::Measure(MeasureId)` already
exists (`ids.rs:500`) with kind byte **9** (`ids.rs:536`), round-trip at
`:572`/`:666`.

**`Measure` is absent from `canonical_value!`.** It has a `Codec` and — via
`struct_codec!` — a `TextValue`, but the strict-decode list
(`codec.rs:3597`ff, where G3a added its four) does not contain it. Verified by
scanning the macro invocation body: zero occurrences.

**Measures currently reach the graph only through base ingest.**
`create_staff_instance` explicitly rejects a carried instance bearing
measures (`reduce.rs:4047`: `!op.instance.measures.is_empty()` →
`container_not_empty()`). This is the same from-empty condition that made
G3a's four families base-ingest-only, and it is what `CreateMeasure` retires.

**Discriminants 16, 17 and 18 are free on `PreconditionFailureReason`.** The
`discriminant()` match (`effect.rs:190`ff) ends at
`TranspositionOutOfRange => 15`. `introduced_minor()` is exhaustive with no
wildcard arm, so a new variant **cannot compile** without an epoch — the
G-minor control, working as designed.

**Invariant 20 is free.** `GraphInvariant::number()`
(`invariants.rs:96`–`:119`) runs 1–19, ending `BarlineGroupSameRegion => 19`.
`all()` returns **`[GraphInvariant; 19]`** (`invariants.rs:121`) — a
hand-maintained count literal — under a doc comment reading "All 19
invariants."

**Invariant 10 already checks measure-signature *resolution*.** Its body
covers a measure's signature reference at `invariants.rs:1180`–`:1212`, with
a direct test `inv10_flags_unresolved_time_signature_reference` at `:3596`.
Invariant 20 therefore MUST NOT re-check resolution (pin 9b).

**The effective grid, from the actual field types.**
`StaffInstance.local_metric_grid: Option<MetricGrid>` (`graph.rs:608`),
falling back to the enclosing region's
`default_metric_grid: Option<MetricGrid>` (`graph.rs:655`).
`MetricGrid { meter_sequence: Vec<MeterChange> }` (`graph.rs:358`), and
`MeterChange { anchor: TimeAnchor, time_signature: TimeSignatureId }`
(`graph.rs:237`) — the change is positioned by a **`TimeAnchor`**, not a
scalar offset, which pins 6 and 6b must account for.

**`Measure` has a live inbound-reference surface.** Cross-cutting structures
anchor to measures through `TimeAnchor::Measure { id, .. }` — see
`payload.rs:1051` (spanner endpoints → `TypedObjectId::Measure`),
`indexes.rs:56`, and `invariants.rs:452`. **This is the Packet A defect class,
live for G3b before a line is written** (pin 10).

**The six boundary crossings, at their post-G3a values.** One compiles
loudly; five fail silently:

| # | Site | Now | Becomes |
|---|---|---|---|
| 1 | `epiphany-editor-core/src/barriers.rs` `subjects_of` | exhaustive match | one new arm — **compiles loudly** |
| 2 | `epiphany-layout-ir/src/barrier.rs:1179` | `tag: 39` | `tag: 40` |
| 3 | `epiphany-testkit/tests/text_projection_grammar.rs:316`–`:317` | `39`, `"38 payload-free kinds plus `Registered`"` | `40`, `"39 payload-free kinds…"` |
| 4 | `epiphany-testkit/src/generators.rs:1932` + test body | comment `30..=38`; `saw_*` flags per kind | `30..=39` + a `saw_create_measure` flag and its assertion |
| 5 | `epiphany-testkit/src/layout_stub.rs:1375` | comment `30..=38` | `30..=39` |
| 6 | `epiphany-textproj/src/vectors.rs:533`–`:534`, `:941` | `(0 12 0)` / `(0 11 0)` | `(0 13 0)` / `(0 12 0)` |

Sites 4 and 5 are **not** comment-only: both test bodies enumerate appended
kinds explicitly, so site 4 needs a new boolean and assertion, not a literal
bump. `COMPANION_VERSION` is `(0, 12, 0)` at
`epiphany-textproj/src/lib.rs:59`.

**The golden lock still stops at 30.** `payload.rs:2165` reads
`let table: [(OperationKind, u8); 30] = [`. Kinds 30–39 stay outside it —
**P13-S15 remains open by design**; G3b does not extend it (pin 12).

**Packet B's own guard now constrains this rung.** The history guard
(`epiphany-testkit/tests/binary_format_history.rs`) asserts a principal
marker per standalone-row rung. G3b must add its Revision History row *and*
its marker to `PRINCIPAL_MARKERS`, or the guard fails — by construction
(pin 14).

---

## §1. Pins

**Pin 1 — kind and tag 39, epoch 12, in both discriminant spaces.** The two
spaces are unaligned and both hand-touched: `OperationKind::discriminant()`
(hand-written match, `payload.rs:253`ff) and `OperationKindTag` (macro
`operation_kind_tag_vocabulary!`, `payload.rs:440`ff). Add
`CreateMeasure => 39` to the first and
`CreateMeasure = 39 => "create-measure" @ Some(12)` to the second.

**Pin 2 — `schema_major()` gains no arm.** Per §0, `Measure` is major 0.
`OperationEnvelopeBlock` stays at 3. If an arm for `CreateMeasure` appears in
`schema_major()`, the packet is wrong.

**Pin 3 — `Measure` joins `canonical_value!`** (`codec.rs:3597`ff), one
entry. This introduces **no new wire layout** — the macro delegates to the
existing `Codec` and generates strict `decode_canonical` (decode →
`finish()` → re-encode → reject on byte mismatch). No `textvalue_graph.rs`
work: `struct_codec!` already generated `Measure`'s `TextValue`.

**Pin 4 — the operation shape follows `CreateStaffInstance`, not G3a.**

```rust
pub struct CreateMeasureOp {
    pub instance: StaffInstanceId,
    pub measure: Measure,
}
```

with `measure_id()` returning `self.measure.id`, and `encode_canonical`
pushing the parent id then the value — mirroring `CreateStaffInstanceOp`
(`payload.rs:1522`ff). The parent is the `StaffInstance`, because
`measures` is per-instance and admits polymeter (`graph.rs:609`–`:611`).

**Pin 5 — the carried-value map must carry the OWNING INSTANCE.** A
`BTreeMap<MeasureId, Measure>` is **insufficient**: `Measure` carries no
back-pointer to its parent (`graph.rs:588`–`:594`), so a map keyed by
`MeasureId` alone cannot tell the graph-removal arm (pin 10) which
`StaffInstance.measures` to remove from, and cannot support any per-instance
ordering check. Every G3a family was root-level and had no parent to lose;
`Measure` does. The map is therefore

```rust
measure_values: BTreeMap<MeasureId, (StaffInstanceId, Measure)>,
```

threaded through the **same seven sites** — reducer state declaration,
`WorkingSnapshot` declaration, initialization, **base seeding in
`seed_from_graph`** (recording the enclosing instance id as it walks), mint
insertion, snapshot, restore. Site 4 fails silently when omitted (the G1
`instrument_values` hazard).

Set-union discipline: byte-identical re-carry → `AlreadyApplied`; differing
value under a live id → `RecreateContentMismatch`; tombstoned →
`TargetTombstoned`. **A re-carry naming a *different* parent instance with an
otherwise identical measure is `RecreateContentMismatch`, not
`AlreadyApplied`** — the parent is part of identity here, and the comparison
MUST include it.

**Pin 6 — the comparable relation: exact shapes, exhaustively.**

The first two drafts were both wrong about this. Draft 1 said only
cross-variant comparison failed. Draft 2 said "same referent, compare
offsets", which ignores that offsets themselves are not always comparable and
that three anchor shapes carry no referent id at all.

**Offsets first.** `AnchorOffset` (`time.rs:557`) is
`Musical(MusicalDuration) | WallClock(WallClockDuration) | Zero` — three
clocks with **no cross-clock ordering**. Normalization: `Zero` is read as the
additive identity **of whichever clock it is compared against**, so
`Zero` ≟ `Musical(d)` and `Zero` ≟ `WallClock(d)` are both defined. Two
offsets are **comparable** iff they are the same variant, or at least one is
`Zero`. `Musical` against `WallClock` is **never** comparable — that is the
deferred wall-clock/musical reconciliation, not an implementation detail.

**Anchor shapes.** `MeasurePosition` and `RegionEdge` are each
`Start | End` (`time.rs:540`, `:547`). **The boundary selector must be
IDENTICAL — it is never ordered.** Two anchors are **comparable** iff they
match one of these five shapes and their offsets are comparable:

| # | Shape | Order by |
|---|---|---|
| c1 | `Event{id: a, off}` vs `Event{id: a, off}` — **same id** | offset |
| c2 | `Measure{id: a, pos: p, off}` vs `Measure{id: a, pos: p, off}` — same id **and same `pos`** | offset |
| c3 | `Measure{id: a, pos: Start, off: Zero}` vs `Measure{id: b, pos: Start, off: Zero}`, `a`/`b` **in the same `StaffInstance.measures`** | vector index |
| c4 | `Region{id: a, edge: e, off}` vs `Region{id: a, edge: e, off}` — same id **and same `edge`** | offset |
| c5 | `WallClock{time}` vs `WallClock{time}` — **no referent id** | `time` |

**Draft 3 ordered `Start < End` and then compared offsets. That is unsound,
and core says so.** With a nonzero offset the selector does not bound the
point: `Region{edge: Start, off: Musical(100)}` is **not** provably before
`Region{edge: End, off: Zero}` without knowing the region's length. The same
holds for `Measure` Start vs End — and
`crates/epiphany-core/src/invariants.rs:466`ff records exactly this: the
prototype anchor resolver places `Measure` **start** anchors and `Region`
edges but returns `None` for a `Measure` **end**, because a coordinate
"cannot be placed without the deferred tempo/measure-length machinery."
Cross-boundary comparison therefore stays **unverifiable** unless the
boundary's own duration is resolved — which is the deferred machinery, not
this rung's business.

**c3's restriction to `pos: Start` and `off: Zero` is load-bearing.** A
vector index orders measure *reference points*, not arbitrary points near
them: a nonzero offset or an `End` position can carry a point past its
neighbour, so the index no longer bounds it.

**Everything not in that table is NOT comparable**, and no other relation may
be invented — defining one is a specification question this rung has no
authority over. In particular: never `Event`↔`Measure`, never
`Measure`↔`Region`, never two `Event`s with different ids (that needs each
event's resolved position, the deferred **P11-C5** machinery — see
`PositionOutsideRegion`'s own "Reserved" note, `effect.rs:139`–`:142`), never
`Musical` against `WallClock`, and **never across differing `pos`/`edge`
selectors**.

**Pin 6b — boundary consistency needs a musical DELTA, not an ordering.**
Pin 6 answers "before or after"; invariant 20's second clause needs "exactly
how far", compared against `TimeSignature::measure_duration()`
(`graph.rs:346`). Ordering does not supply that, and this was conflated in
both prior drafts.

The delta between two measure starts is computable **only** in shape c1, c2,
or c4 with **both offsets in the `Musical` clock** (or `Zero`, normalized to
`Musical(0)`), **and only when the boundary selector is identical** — same
referent id *and* same `pos`/`edge`. Then the shared reference point cancels
and the difference of the offsets is exact.

**Draft 3 subtracted offsets across differing `pos`/`edge`, which is the same
unsoundness as pin 6's ordering error.** Across a boundary the reference
points differ by the boundary's own duration, which is precisely the
unresolved quantity (`invariants.rs:400`ff). Subtracting the offsets then
omits that duration and yields a wrong delta, not merely an unordered one.

`WallClock` deltas are not musical durations and MUST NOT be compared with
`measure_duration`. **c3 supplies no delta at all**: a vector index gives
order, never distance.

Where the delta is not computable, boundary consistency **abstains** (pin 7).

**Pin 6c — the effective-grid oracle, and the instance-grid ledger the
reducer does not have.**

Every prior draft said "the active signature at the measure's start" without
defining *selection*. Over a **partially comparable** `meter_sequence` that is
not a function. Define it:

**Step 0 comes before any candidate set.** If **any** `MeterChange` in the
effective grid has an anchor **incomparable** to the measure's start (pin 6),
selection is **indeterminate** whenever a governing signature is needed — the
operation refuses (`MeasureOrderUnverifiable`), the invariant abstains (pin 7).
This holds **even when the candidate set is empty**, and draft 4 got it wrong:
it let "no candidates" win, so a sequence in which *every* change is
incomparable read as "no active signature, agreement vacuous" instead of
"cannot tell." An incomparable change is not absent; it is unplaced, and it
might have governed.

Only once step 0 passes — every change comparable — let `C` = those changes
**not after** the measure's start. Then:

1. `C` empty → **no active signature.** Agreement is vacuous; the boundary
   clause abstains. This is *not* a violation. Reachable only from a
   genuinely empty or wholly-later sequence, never from incomparability.
2. `C` has a unique maximum under pin 6's relation → that is the governing
   change.
3. `C` has **multiple maxima mutually incomparable to each other** →
   indeterminate, as step 0. Two changes can each be comparable to the
   measure's start yet not to one another, so this rule is not subsumed by
   step 0. Never pick one by document order, id, or vector position — that
   fabricates a total order pin 6 refuses to define.

**The ledger gap, which blocks base-free correctness.**
`StaffInstance.local_metric_grid` **overrides** the region default
(`graph.rs:608`, `:655`), but **the reducer retains no instance-local grid
state.** `create_staff_instance` accepts the field and seeds only the layout
advisory chain (`reduce.rs:4026`ff). So base-free, a `CreateMeasure`,
`SetMetricGrid`, or `SetTimeSignature` **cannot tell a local override from an
inherited default** — it would silently read the region default and could
refuse or permit wrongly.

This rung MUST resolve that, and there are exactly two admissible answers.
**Ratify one:**

**Disposition (A) is RATIFIED** (2026-07-29): it preserves graph-aware /
base-free parity and avoids knowingly authoring states that become invalid the
moment they are materialized.

- **(A) Add an instance-grid ledger** — `instance_grid: BTreeMap<StaffInstanceId, Option<MetricGrid>>`
  with the full seven-site lifecycle (declaration, snapshot decl, init, base
  seed, mint/write, snapshot, restore), seeded by `create_staff_instance` and
  by `seed_from_graph`.

  **The ledger alone is NOT the oracle.** It distinguishes *override* from
  *inheritance*; it does not reconstruct what is inherited. When an instance
  inherits, the effective grid MUST be reconstructed from the **existing**
  ledgers, in canonical order:

  1. `metric_grid_chain` (`reduce.rs:974`,
     `BTreeMap<RegionId, WriteChain<Option<MetricGrid>>>`) — whole-grid writes
     for the enclosing region;
  2. `meter_change_chain` (`:993`,
     `WriteChain<Option<MeterChange>>` per `(RegionId, MusicalPosition)`) —
     per-key changes layered over that grid, `None` meaning explicit removal.

  Both are `WriteChain`s, so the reconstruction MUST also fold in **prospective
  undo restorations** (pin 9c.3) when evaluating an undo: the grid the
  restoration would install, not the one currently in place.

  **The same oracle runs in both reduction modes.** Graph-aware reduction must
  not shortcut to `score` while base-free reduction uses the chains — that would
  reintroduce the parity gap (A) exists to close, and the two paths would
  disagree on exactly the cases this rung refuses.
- **(B) Ratify an explicit graph-only divergence** — the agreement and
  boundary preconditions are graph-aware only, and base-free reduction
  **skips them entirely** rather than consulting the region default. Cheaper,
  but it means a base-free stream can author a measure that violates
  invariant 20 the moment a base is supplied.

Silently reading the region default base-free is **not** an option: it is
neither (A) nor (B), and it produces wrong answers rather than absent ones.

**Pin 7 — fail-closed at the operation, abstain at the invariant.** These are
different things, and draft 1 mislabelled abstention as fail-closed. The split
is deliberate:

- **At the operation — fail closed.** `CreateMeasure` and the two grid
  setters (pin 9c) *refuse* when the comparison or delta they need is not
  computable. Nothing incomputable enters through this rung.
- **At the invariant — abstain, and name it.** Invariant 20 emits no
  violation where agreement or delta is not computable, because base-ingested
  data may predate the rule and flagging it would make the invariant useless
  on real scores. This is **abstention**, stated as such in the doc comment,
  **not** a safety property.

File the residue as **P13-S18**: invariant 20's agreement and boundary checks
are partial, and **P13-S23** — the general common-timeline/musical-distance
capability, filed against this residue — is what would close them.
**Status: open, deliberately.**

**Pin 8 — referential preconditions.**
1. The parent `StaffInstance` must be live — **ungated**, matching
   `create_staff_instance`'s region check (`reduce.rs:4031`): a mint into a
   non-existent parent has nowhere to go even base-free.
2. `measure.time_signature`, when `Some(id)`, must resolve to a live
   `TimeSignature` — graph-aware.
3. **`measure.start`'s referents must resolve** — `start` is a `TimeAnchor`
   (`graph.rs:590`), so each non-`WallClock` variant preconditions a live
   referent of the right kind. Omitted from draft 1 entirely; an unresolvable
   `start` would have minted a measure anchored to nothing. Graph-aware.

**Pin 8b — three new `PreconditionFailureReason` variants, all epoch 12.**
Draft 2 had two, and used `MeasureOrderUnverifiable` for a
comparable-but-reversed start, which is untruthful — that case is perfectly
verifiable and simply wrong.

| Discriminant | Variant | Meaning |
|---|---|---|
| 16 | `MeasureMeterMismatch` | a resolving `time_signature` **disagrees** with the effective grid's active signature (distinct from the resolution failure, which is `TargetMissing`) |
| 17 | `MeasureOutOfOrder` | the carried `start` is **comparable** to the current last measure's start and is not strictly after it |
| 18 | `MeasureOrderUnverifiable` | the two starts are **not comparable** (pin 6), or the delta is not computable (pin 6b) |

All three gain `introduced_minor()` arms; none compiles without one.

**Pin 9 — `CreateMeasure` is append-only, and must check BOTH clauses
prospectively.** `StaffInstance.measures` is documented "in order"
(`graph.rs:609`–`:611`), so the measure is pushed at the end and the operation
refuses unless:

1. the carried `start` is **comparable** to the current last measure's start
   (else `MeasureOrderUnverifiable`) and **strictly after** it (else
   `MeasureOutOfOrder`);
2. agreement holds against the effective grid (else `MeasureMeterMismatch`);
3. **the delta from the previous measure's start equals that measure's
   governing `measure_duration()`** (else `MeasureMeterMismatch`), or the
   delta is not computable (else `MeasureOrderUnverifiable`).

**Clause 3 is the gap draft 2 left open:** a strictly-later start at the
*wrong distance* is comparable, correctly ordered, agreeing — and violates
invariant 20 the instant it lands. An operation that can create an immediate
invariant violation is not preserving the invariant.

The first measure of an instance has no predecessor: clauses 1 and 3 are
vacuous for it, and **pickup/anacrusis is deferred** — a partial first measure
MUST NOT be refused or flagged. File as **P13-S19**, status open.

**Pin 9b — invariant 20 covers agreement and boundary ONLY, and `None` is
narrower than draft 2 said.** Invariant 10 already checks that a measure's
signature reference *resolves* (`invariants.rs:1180`–`:1212`, test `:3596`);
invariant 20 MUST NOT duplicate it. Invariant 20 checks:

1. **agreement** — a resolving `Some(id)` equals the effective grid's active
   signature at that measure's start, within pin 6's relation;
2. **boundary consistency** — consecutive starts differ by the governing
   `measure_duration()`, within pin 6b's delta.

**`time_signature: None` avoids ONLY the agreement clause.** Draft 2 said
"never a violation", which is wrong: `None` means *inherit*, and the inherited
meter still governs **boundary consistency**. A `None` measure at the wrong
distance from its predecessor IS an invariant-20 violation.

**Pin 9c — the invariant must be PRESERVED by everything that can break it.**
An invariant nothing maintains is decoration. Two existing operations write
the effective grid: `SetMetricGrid` (kind 22, `payload.rs:388`) and
`SetTimeSignature` (kind 25, `:392`). This rung MUST add **prospective checks
for both invariant-20 clauses** — agreement *and* boundary consistency; draft
2 specified only agreement — to:

1. both setters' forward paths, refusing with `MeasureMeterMismatch` (or
   `MeasureOrderUnverifiable` when incomputable) if the *resulting* grid would
   violate invariant 20 for any **live** measure;
2. **All invariant checks MUST run BEFORE any mint.** `set_time_signature`
   mints its carried signature at `reduce.rs:4518`–`:4522`, *before* it writes
   the meter change. Appending a prospective refusal after that point **leaks a
   freshly minted `TimeSignature`** from an operation that reported a
   precondition no-op — and from a *non-transactional* operation there is no
   undo to reclaim it. Restructure so every check precedes `mint_time_signature`,
   and test that a refusal leaves **no residue in `objects`, no residue in any
   carried-value map, and no residue in the graph**.

3. both setters' **undo restoration** paths, because a restoration reinstating
   a prior grid breaks agreement exactly as a forward write does — and
   **both undo policies**, evaluated in **aggregate**:

   A single undo can collect a whole-grid restoration *and* one or more
   meter-change restorations together, and **their safety is not
   independent**: individually-unsafe restorations can be jointly safe (a grid
   restore plus the meter-change restore that re-agrees with it), and
   individually-safe ones can be jointly unsafe. Per-restoration evaluation is
   therefore wrong in both directions.

   - `StrictInverse` / `Cascade`: evaluate invariant 20 against the
     **prospective post-undo state with the whole restoration set applied**,
     and `Conflicted` if that state violates it. Never accept or reject
     restorations one at a time.
   - `BestEffort`: apply the **maximal safe subset**, chosen by a
     **deterministic, documented rule** — the canonical-order greedy: consider
     restorations in the reduction's existing canonical order, admit each one
     whose addition keeps the accumulated prospective state invariant-20-clean,
     skip the rest. "Maximal" here means maximal under that rule, not
     set-theoretically maximum; state that plainly, because the true maximum
     subset is not uniquely defined and a search for it would not be
     deterministic. It must not refuse wholesale, and it must not apply a
     violating set.
3. `undo_strand_block`'s **existing `TimeSignature` arm** (`reduce.rs:5476`ff),
   which today consults only `meter_change_chain`, extended so that a minted
   `TimeSignature` still named by a live `Measure.time_signature` also blocks.
   **That hole exists today**, before G3b, and is the class Packet A closed
   three instances of.

**Pin 10 — undo coverage, and TWO ledgers the rung must build.**

1. `materialize_graph_tombstones` (`reduce.rs:2768`ff) gains a
   `TypedObjectId::Measure(id)` arm. Structurally harder than G3a's five
   root-vector `retain`s: it must reach the owning instance across
   `score.canvas.regions[*].content.staff_instances`. Pin 5's parent-carrying
   map is what makes that possible without a search.

2. The `Measure` strand guard **cannot use `structures`.** That index is
   **event-only by design**: the endpoint walk filters `_ => None`
   (`reduce.rs:1755`–`:1757`) and the comment at `:1763`–`:1765` says so
   outright — "Non-event anchorings (region, measure, wall-clock, free)
   contribute no entry." A guard written against it is **born green**.

3. **For spanners:** read `cross_cutting_modify_chain` (`reduce.rs:973`),
   which holds the full `CrossCuttingValue` and therefore real anchors, and be
   **restoration-aware** — the inverse of Packet A's pin A6, because
   `ModifyCrossCutting` rewrites those values and
   `ValueRestoration::CrossCutting` (`reduce.rs:842`) restores them, so a write
   chain genuinely exists. **Both directions:** a modify that *removed* a
   measure anchor, undone, reinstates the reference and MUST block; one that
   *added* an anchor, undone, removes it and MUST NOT block.

4. **For repeat structures, that chain is structurally unusable.**
   `CrossCuttingValue` is `Tie | Slur | Beam | Spanner`
   (`payload.rs:980`–`:985`) — **`RepeatStructure` is not a variant**, so
   `cross_cutting_modify_chain` can never contain one. And
   `create_repeat_structure` retains no value: it inserts liveness only
   (`reduce.rs:3727`ff), pushing the value into the graph when there is one.
   Draft 2's instruction to "cover RepeatStructure via that chain" was
   impossible.

   The rung MUST therefore add **`repeat_values: BTreeMap<RepeatStructureId, RepeatStructure>`**
   — or a generalized full-anchor ledger — with pin 5's **seven** sites:
   declaration, `WorkingSnapshot` declaration, initialization, base seed,
   create insertion, snapshot, restore. **No delete site.**

   **Draft 6 claimed an eighth, delete site on a false premise, and it is
   withdrawn.** The stated justification — that retaining a deleted repeat's
   value would make the strand guard "block on a corpse" — contradicts the
   guard itself: pin 10.5 requires the *owning* object to be `Live`, and
   `delete_repeat_structure` sets `objects` to `Tombstoned`
   (`reduce.rs:3766`–`:3768`) rather than removing the entry. A retained
   tombstoned value therefore **cannot** block. The premise was wrong, so the
   site is unnecessary.

   **Retention is also the ratified project discipline, and this rung must not
   diverge from it.** Packet A's pin A7 ruled that value maps are *not* pruned
   — safe precisely because every reader consults `self.objects` first — and
   the tree bears that out: there is **no `.remove` on any value map anywhere
   in the reducer** (verified: zero occurrences across `staff_values`,
   `instrument_values`, `time_signature_values`, `staff_group_values`). An
   eighth site would make `repeat_values` the sole exception to a uniform rule,
   for no benefit.

   **And retention is what makes M62's repeat observation reachable at all.**
   The missing-`Live`-conjunct mutant needs a state where a tombstoned repeat's
   value is still present and still names the measure. Pruning on delete would
   destroy exactly that state, and M62's repeat row would be **born green** —
   the failure mode this contract keeps rediscovering. `measure_values` reaches
   seven sites by a different route (measures are mint-only, §6 ruling 1); both
   maps land on seven, and the totals are unchanged at **75**. Use the helpers that already walk every anchor kind:
   `RepeatStructure::anchor_sites()` (`graph.rs:1249`) and
   `anchor_object_refs` (`reduce.rs:1290`), which maps `Event`, `Measure`
   **and** `Region` — `create_repeat_structure` already relies on exactly this
   pair (`reduce.rs:3717`).

5. **The inbound surface is SEVEN surface classes.** Drafts 1–3 covered two
   (spanner, repeat) and stopped; draft 4 said "five ledgers" and conflated
   the additions with the total. The seven are: **spanner, repeat, measure,
   meter change, system break, page break, tempo segment.** The five beyond
   spanner and repeat sit together in the reducer's declarations at
   `reduce.rs:988`ff — so the omission was visible at the site the contract
   was already citing:

   | Surface | Ledger | Holds |
   |---|---|---|
   | another measure's `start` | `measure_values` (pin 5) | `Measure.start: TimeAnchor` |
   | meter changes | `meter_change_chain` (`:993`) | `Option<MeterChange>` → `anchor: TimeAnchor` |
   | system breaks | `break_chain` (`:988`) | `(TimeAnchor, bool)` |
   | page breaks | `page_break_chain` (`:989`) | `(TimeAnchor, bool)` |
   | tempo segments | `tempo_segment_chain` (`:994`) | `Option<TempoSegment>` → **two** anchor sites: `start: TimeAnchor` and `end: Option<TimeAnchor>` (`tempo.rs:109`) |

   Each chain needs **prospective and restoration-aware** treatment — every one
   is a `WriteChain`, so a restoration can reinstate a measure reference the
   undo was about to strand, exactly as with `cross_cutting_modify_chain`.

   **`TempoSegment` does carry anchors, at two sites** — `start` and the
   optional `end` (`tempo.rs:109`). Both MUST be checked; a guard reading only
   `start` misses a measure named as a segment's end.

   **Owner-liveness and `targets` handling is per surface, and the tempo
   surface splits by KEY — not wholesale.** Six classes are always owned
   (spanner, repeat, measure, meter change, system break, page break): the
   guard requires the *owning* object `Live` and not in `targets`, as Packet
   A's guards do.

   `tempo_segment_chain` is keyed `(Option<RegionId>, MusicalPosition)`
   (`reduce.rs:995`), and the two key shapes behave differently:
   - **`Some(region)` — owned.** There is a live `Region` to test, so this
     entry carries the full `Live` + `!targets.contains` treatment exactly like
     the other six.
   - **`None` — score-level, genuinely ownerless.** No object exists to test
     for liveness. The guard blocks on the reference itself, exempting only a
     segment whose *write* belongs to the undone transaction.

   Draft 5 excluded the **entire** tempo surface from owner treatment on the
   strength of the `None` case. That was an over-correction: it would leave a
   region-scoped tempo segment's `Live` conjunct unguarded.

   A `Measure.start` naming another measure is the subtlest: undoing measure
   *B* while live measure *C* is anchored to it strands authored state inside
   the very map pin 5 introduces.

6. Test rows: removal from the owning instance, ledger tombstoning,
   `TargetTombstoned` on byte-identical re-carry after undo, strand refusal
   from **each of the seven surfaces**, both restoration directions,
   same-transaction teardown, and a tombstoned referencer not blocking — the
   last two **per surface**, since each guard has its own conjuncts to get
   wrong, and the score-level tempo case has no owner conjunct at all.

**Pin 11 — the `all()` count, and a mutation that compiles.**
`[GraphInvariant; 19]` → `; 20`, the array gains its entry, the "All 19
invariants" doc becomes 20, and `check_invariants` (`invariants.rs:229`)
dispatches it. A hand-maintained count — the class this project keeps
rediscovering (four stale sites at Push 4a, six at G2a, the golden lock at
P13-S15).

**Reverting the count literal alone is a type error** over a 20-element array,
and a mutation that does not compile signs nothing. The signing mutation is
**deleting invariant 20's arm from `check_invariants`' dispatch**, leaving the
enum and `all()` intact — so the row must assert **behaviourally** (a score
violating only invariant 20 is flagged by `check_invariants`), since
`all().len() == 20` passes with the dispatch gone.

**Pin 12 — the golden lock is not extended.** It stays
`[(OperationKind, u8); 30]`. P13-S15 remains open by design; extending it is
its own rung, so that a golden-lock extension lands with its own mutation
evidence and nothing else in the diff.

**Pin 13 — companion 0.12.0 → 0.13.0**, `epiphany-textproj/src/lib.rs:59`,
with the negative-vector anchors at `vectors.rs:533`–`:534` and `:941` moved
to reject `(0 12 0)`.

**Pin 14 — the four-document ritual, plus the new guard.** An
operation-vocabulary append is a documented event in **four** spec documents:
`operation_catalog.tex` (a new `CreateMeasure` section with undo semantics
and the stale-form/deferral notes), `binary_format.tex` (kind table, tag
table, payload layout, **version 0.15.0 → 0.16.0, and a Revision History
row**), `core_spec.tex` (invariant 20 in the invariant table and its
numbering prose), and `text_projection.tex` (the companion grammar).

**And `binary_format_history.rs` must gain `("G3b", "--- Genesis tranche G3b")`
in `PRINCIPAL_MARKERS`.** The guard I landed at `6170015` will fail otherwise
— deliberately. Do not weaken the guard to accommodate the rung; add the row
and the marker.

**Pin 15 — the monotonicity evidence repair.**
`spec/PLAN_GMINOR_SCHEMA_MINOR.md:201` claims real-time monotonicity with an
evidence chain ending at G2a `7df5ca1`. Extend it with **vocabulary-introducing
events only**:

| Epoch | Event | Commit | Date |
|---|---|---|---|
| 10 | G2b | `13c3d2f` | 2026-07-29 |
| 11 | G3a | `6c5e69f` | 2026-07-29 |
| 12 | G3b | `e64a4b7` | 2026-07-30 |

**Closed by the pre-push repair.** G3b landed across six commits, so its row
needed the same distinction the 2026-07-28 correction draws for G2a: the
introducing commit is **`e64a4b7`** (packet 1), where kind/tag 39 enters
`ops/src/payload.rs` and reasons 16–18 enter `ops/src/effect.rs`. The four
commits after it introduce no discriminant, and the rung *completes* at
`d58eee8` (packet 3b, documentation). Recorded by the pre-push repair rather
than by G3b itself, because a commit cannot cite its own hash and G3b closed
the ladder — there was no later rung to fill it in.

**G-minor and P13-S17 (`6170015`) do NOT belong in this chain** — G-minor
built the epoch machinery and P13-S17 restored a document's history; neither
introduced an additive variant, and the chain records introducing commits, not
rungs. All four dates verified with `git show -s --format=%cs`.

**The 2026-07-29 tie is resolved by ancestry, not timestamp.** `13c3d2f` is an
ancestor of `6c5e69f` (verified with `git merge-base --is-ancestor`), so G2b
precedes G3a. Record the tie-break method, exactly as the existing prose does
for the two events sharing 2026-07-07.

---

## §2. Touch table

**Draft 1 omitted mandatory integration files; drafts 2–3 miscounted them.**
The authority is G3a's own footprint — `git show --name-only 6c5e69f`, 35
files — not memory. Rows 6–14 are **nine** files, not ten; the tenth omitted
G3a-footprint file is **`core/DECISIONS.md`, carried in row 35**. Row 14a
(`QUICKSTART.md`) and row 12a (`decode.rs`) are further surfaces found in later
review rounds. Every one of them fails **silently**, none at compile time.

**Row 12a is a pre-existing live bug, not just this rung's plumbing.**
`decode.rs`'s `precondition_reason` (`:205`) ends at **13** and rejects
everything higher, so `AcousticRealizationPinned` (14) and
`TranspositionOutOfRange` (15) **encode but cannot decode today** — a
materialized effect carrying either fails canonical round-trip. Nothing caught
it because the testkit's generator draws `rng.below(14)` (`generators.rs:417`)
under a doc comment claiming "every core and registered variant." This rung
repairs the decoder through **18** and the generator through **18**, and files
the pre-existing half as **P13-S20**.

| # | File | Change |
|---|---|---|
| 1 | `ops/src/payload.rs` | `CreateMeasureOp`; `OperationKind::CreateMeasure`; `discriminant()` 39; `introduced_minor()` `Some(12)`; tag `39 => "create-measure" @ Some(12)`; `CanonicalEncode` (parent then value); **no `schema_major` arm** |
| 2 | `ops/src/effect.rs` | `MeasureMeterMismatch = 16`, `MeasureOutOfOrder = 17`, `MeasureOrderUnverifiable = 18`; three `introduced_minor()` arms → `Some(12)` |
| 3 | `core/src/codec.rs` | `Measure` in `canonical_value!` |
| 4 | `core/src/invariants.rs` | invariant 20; `number()` arm; `all()` `19`→`20` + entry + doc; `check_invariants` dispatch; the check body (pins 6, 6b, 9b); doc-comment abstention + pickup deferrals |
| 5 | `ops/src/reduce.rs` | `create_measure` (pins 8, 9); parent-carrying **`measure_values` ×7 sites**; **`repeat_values` ×7 sites** (no delete site — pin 10.4); **`instance_grid` ×7 sites** and the **shared effective-grid oracle** reconstructing inheritance from `metric_grid_chain` + `meter_change_chain` in **canonical write order**, folding in prospective restorations, run in **both** reduction modes (pin 6c, M30a–M30c, M31a); `Measure` graph-removal arm; the `Measure` strand guard across **all seven surfaces** — `cross_cutting_modify_chain` (spanner), `repeat_values`, `measure_values`, `meter_change_chain`, `break_chain`, `page_break_chain`, `tempo_segment_chain` (both anchor sites) — restoration-aware on the five `WriteChain` surfaces; **`SetMetricGrid`/`SetTimeSignature` preservation, both clauses, both undo policies, all checks before any mint; `TimeSignature` strand extension (pin 9c)** |
| **6** | `ops/src/envdecode.rs` | envelope decode arm for kind 39 |
| **7** | `ops/src/migrate.rs` | migrate-on-read handling |
| **8** | `ops/src/v0.rs` | v0 wire path |
| **9** | `ops/src/vectors.rs` | operation wire vector for kind 39 |
| **10** | `ops/src/valuegen.rs` | a `measure(...)` fixture — see §3's length-distinguishability requirement |
| **11** | `ops/src/fuzz.rs` | fuzz arm |
| **12** | `ops/src/textproj_kind.rs` | kind ↔ projection mapping — **this is where the operation-kind production lives**, not `textproj/project.rs` |
| **12a** | `ops/src/decode.rs` | `precondition_reason` (`:205`) must decode **16, 17, 18** — and **14, 15**, which it already omits. See below |
| **13** | `spec/vectors/decode_vectors.txt` | frozen cross-impl decode vectors — **append only** |
| **14** | `spec/vectors/textproj_document_vectors.txt` | frozen projection vectors — **append only** |
| **14a** | `spec/QUICKSTART.md:56` | "the 19 graph invariants" → 20. A stale surface absent from every prior draft; it is prose, so it fails silently forever |
| 15 | `ops/src/lib.rs` | re-exports |
| 16 | `core/src/graph.rs` | `Measure` / `measures` docs: append-only ordering, the abstention, `None`-inherits-boundaries |
| 17 | `textproj/src/lib.rs:59` | `COMPANION_VERSION` → `(0, 13, 0)` |
| 18 | `textproj/src/parse.rs` | **companion-version fixture only.** `project.rs` has **zero** operation-kind productions (verified: 0 matches for the G3a kinds, vs 4 in `ops/src/textproj_kind.rs`), so drafts 1–3 named the wrong file |
| 19 | `textproj/src/vectors.rs` | positive vector; negative anchors → `(0 12 0)` |
| 20 | `editor-core/src/barriers.rs` | one `subjects_of` arm — **the only authorized editor-crate change** |
| 21 | `layout-ir/src/barrier.rs:1179` | `tag: 39` → `40` |
| 22 | `testkit/tests/text_projection_grammar.rs:316`–`:317` | `40`, `"39 payload-free kinds…"` |
| 23 | `testkit/src/generators.rs` | draw arm; comment `30..=39`; `saw_create_measure` + assertion; **`precondition_failure_reason`'s `rng.below(14)` (`:417`) → `below(19)` with arms 14–18** — its doc comment claims "every core and registered variant" and is false today |
| 24 | `testkit/src/layout_stub.rs:1375` | comment `30..=39` |
| 25 | `testkit/src/roundtrip.rs` | **unconditional**: verify at ratification whether the kind list is enumerated there, and record the finding either way. Draft 2's "if the kind list is enumerated there" made a touch row depend on the implementer's reading |
| 26 | `testkit/tests/binary_format_history.rs` | `("G3b", "--- Genesis tranche G3b")` in `PRINCIPAL_MARKERS` |
| 27 | `spec/operation_catalog.tex` | `CreateMeasure` section incl. undo semantics; **the three new precondition reasons 16–18**; **document version bump + changelog entry** |
| 28 | `spec/binary_format.tex` | kind table, tag table, payload layout, **the three new `PreconditionFailureReason` discriminants 16–18**, **0.15.0 → 0.16.0**, Revision History row |
| 29 | `spec/core_spec.tex` | invariant 20 + the numbering prose, **and the normative `OperationKind` / `OperationKindTag` listings, which must gain `CreateMeasure`** — drafts 1–3 named only the invariant |
| 30 | `spec/text_projection.tex` | companion grammar, **plus the document title/version bump and a changelog entry** — not merely the grammar |
| 31 | four `.pdf` | regenerated |
| 32 | `spec/PLAN_GMINOR_SCHEMA_MINOR.md` | epoch 12 row; **pin 15's evidence repair** |
| 33 | `spec/PLAN_GENESIS_OPS.md` | ladder CLOSED |
| 34 | `spec/PASS13_CANDIDATES.md` | **P13-S18** (pin 7 abstention residue, **open**), **P13-S19** (pin 9 pickup deferral, **open**), **P13-S20** (the reasons-14/15 decoder hole, **RESOLVED in this rung**) |
| 35 | `core/DECISIONS.md`, `ops/DECISIONS.md` | the rung's record |
| 36 | this contract | DRAFT → RATIFIED |

**No `epiphany-bundle` change** (pin 2: no accept-set move). No editor-crate
change beyond row 20.

---

## §3. Mutation plan — seventy-five mutations, ratified

Draft 2 named outcomes without edits for most rows. Every mutation below is a
**specific edit to a specific site**, and each must be **observed** red.
Reasoning that a mutation "would fail" signs nothing.

### Numbering rule

**One M = one edit.** Draft 3 claimed 52 while M48 and M50 each specified two
distinct edits, so the stated total was not a count of anything. Where one edit
is observed across several rows, that is stated and it remains **one** M.

### Vocabulary — both spaces, all five epoch sites (M1–M10)

Draft 2's single "t1" collapsed all of this into one tag-epoch mutation. There
are **two** discriminant spaces and **two** epoch functions for the kind
(`OperationKind::introduced_minor()` at `payload.rs:431`, the tag vocabulary's
at `:723`), plus three reasons with a discriminant and an epoch each.

| M | Edit |
|---|---|
| M1 | `OperationKind::discriminant()` `CreateMeasure` 39 → 40 |
| M2 | tag vocabulary `CreateMeasure` 39 → 40 |
| M3 | `OperationKind::introduced_minor()` → `Some(11)` |
| M4 | tag vocabulary `@ Some(12)` → `Some(11)` |
| M5, M6, M7 | each new reason's discriminant → 19 (unused, so it compiles) |
| M8, M9, M10 | each new reason's `introduced_minor()` → `Some(11)` |

### Wire (M11–M14)

| M | Edit | Row |
|---|---|---|
| M11 | add `CreateMeasure(_) => 3` to `schema_major()` | a block containing **only** a `CreateMeasure` envelope stamps major **0**; `OperationEnvelopeBlock` stays 3. The block composition is load-bearing: a mixed block would stamp from another kind and pass with M11 applied |
| M12 | reorder `struct_codec!(Measure)`'s field list | frozen literal bytes |
| M13 | swap parent/value order in `CreateMeasureOp::encode_canonical` | frozen literal bytes. Signs pin 4 |
| M14 | **weaken the strict re-encode rejection** — make `decode_canonical` return the decoded value without comparing the re-encoding | strict-canonicality row |

**M14 is not "remove `Measure` from `canonical_value!`".** That does not
compile: the new envelope decode arm calls `value::<Measure>`, whose bound is
`T: CanonicalValue` (`envdecode.rs:224`), so dropping the macro entry breaks the
build and signs nothing.

**And the row must use a structurally decodable but NONCANONICAL encoding.** A
field-swapped byte string is likely rejected by ordinary structural decoding,
which proves nothing about the macro's re-encode check.

**The fixture is concrete and verified — the row is signable, with no
conditional.** Encode `Measure.start` as a `Musical` offset whose
`RationalTime` is spelled **unreduced, `2/4`**. That decodes cleanly:
`RationalTime`'s `decode_canonical` ends in
`Ok(RationalTime::from_big(BigRational::new(numer, denom)))`
(`time.rs:352`ff), and `BigRational::new` **reduces on construction**, so the
value becomes `1/2`. Re-encoding then emits `1/2`'s magnitudes, which differ
from the `2/4` bytes fed in — so the strict per-value re-encode comparison is
the *only* thing standing between the input and acceptance, which is exactly
what M14 must prove.

Draft 5's "if none exists, mark the row unsignable" branch is **removed**: it
contradicted §3's no-deviation rule, and the encoding demonstrably exists.

### Mint (M15–M22)

| M | Edit | Row |
|---|---|---|
| M15 | skip the `measure_values` base seed | **base**-recarry only — it cannot reach the from-empty row |
| M16 | make the value comparison always report `identical` | differing re-carry observes `AlreadyApplied` |
| M17 | drop `StaffInstanceId` from the comparison | parent-mismatch re-carry. Signs pin 5 |
| M18 | remove the parent-liveness check | dead parent, graph-aware **and** base-free legs (ungated) |
| M19 | remove the `time_signature` resolution check | unresolvable signature |
| M20 | remove the `start`-referent resolution check | three observations — `Event`, `Measure`, `Region`. Signs pin 8.3 |
| M21 | append unconditionally | ordering precondition |
| M22 | report `MeasureOrderUnverifiable` for a comparable-but-reversed start | must observe the wrong reason. Signs pin 8b |

### Comparability (M23–M26)

| M | Edit | Row |
|---|---|---|
| M23 | allow `Musical` vs `WallClock` offsets | cross-clock must be `MeasureOrderUnverifiable` |
| M24 | widen c3 to admit `pos: End` or nonzero offsets | a nonzero-offset measure anchor must be **not comparable** |
| M25 | **order across differing `pos`/`edge`** (restore draft 3's `Start < End`) | `Region{Start, Musical(100)}` vs `Region{End, Zero}` must be **unverifiable**. Signs pin 6's identical-selector rule |
| M26 | **subtract offsets across differing `pos`/`edge`** in the delta | the delta must be **not computable**, not a wrong number. Signs pin 6b |

### Grid oracle and the instance-grid ledger (M27–M31)

| M | Edit | Row |
|---|---|---|
| M27 | on multiple mutually-incomparable maxima, pick by document order | must refuse (`MeasureOrderUnverifiable`), never fabricate a total order |
| M28 | drop **step 0**, letting an empty candidate set win | a sequence whose changes are **all incomparable** must be indeterminate, **not** vacuous. Signs pin 6c step 0 |
| M29 | treat an empty candidate set as a violation | a **genuinely empty** `meter_sequence` — no active signature is **vacuous**, not a violation |
| M30 | skip the `instance_grid` base seed | base-free must still see a local override |
| M30a | reconstruct the inherited grid from `metric_grid_chain` **only**, ignoring `meter_change_chain` | layered per-key changes must reach the oracle |
| M30b | in graph-aware mode, read the effective grid from `score` instead of the shared oracle | both modes must run the **same** oracle |
| M30c | overlay **current** per-key `meter_change_chain` values onto the **current** whole grid, ignoring write order | the two chains must interleave in **canonical order**. Tests must cover **both** interleavings: a `SetTimeSignature` **before** a later `SetMetricGrid` (the whole-grid write supersedes the earlier per-key change) and **after** it (the per-key change overlays). M30a/M30b/M31a are all satisfiable by an order-ignoring oracle, so this is a distinct obligation |
| M31 | read the region default while a local override exists | the override must win |
| M31a | omit prospective undo restorations from the reconstruction | the grid a restoration **would** install must govern the check |

**Disposition (A) is ratified**, so M30, M30a, M30b, M31 and M31a all apply.

**The total is 75.** The arithmetic has moved twice, each time because a
review round added a real obligation rather than because the count drifted:

- **71 → 74.** (A) keeps M30/M31 as two rather than collapsing them, and the
  confirmed `TempoSegment` double-site keeps M58 — but disposition (A)'s oracle
  carries three obligations draft 4 had no mutations for: reconstruct
  inheritance from **both** chains (M30a), fold in **prospective restorations**
  (M31a), and run in **both** reduction modes (M30b).
- **74 → 75.** M30a/M30b/M31a are each satisfiable by an oracle that ignores
  write order, so **canonical interleaving was required but unsigned**;
  **M30c** signs it.

Counted mechanically: **71 rows, two of which carry three ids each → 75
distinct single-edit mutations.**

### Create-side clauses (M32–M33)

| M | Edit | Row |
|---|---|---|
| M32 | remove `create_measure`'s agreement check | disagreeing signature |
| M33 | remove `create_measure`'s **delta** check | a strictly-later start at the **wrong distance** must be refused — the immediate-violation gap |

### Invariant 20 (M34–M40)

| M | Edit | Row |
|---|---|---|
| M34 | remove the agreement clause | flags disagreement |
| M35 | remove the boundary clause | flags wrong distance |
| M36 | skip the boundary clause when `time_signature` is `None` | `None` inherits and is still bound |
| M37 | flag the incomparable case | abstention |
| M38 | flag a pickup first measure | deferral |
| M39 | add a resolution check to invariant 20 | an unresolvable reference is an invariant-**10** violation and **not** a 20 |
| M40 | delete invariant 20's arm from `check_invariants`' dispatch | asserted **behaviourally**; reverting `all()`'s count is a type error and signs nothing |

### Preservation (M41–M47)

| M | Edit | Row |
|---|---|---|
| M41 | remove `SetMetricGrid`'s agreement precondition | |
| M42 | remove `SetMetricGrid`'s boundary precondition | draft 2 checked agreement only |
| M43 | remove `SetTimeSignature`'s agreement precondition | |
| M44 | remove `SetTimeSignature`'s boundary precondition | |
| M45 | **move the checks back after `mint_time_signature`** (`reduce.rs:4518`) | a refused `SetTimeSignature` must leave **no residue** in `objects`, in any carried-value map, or in the graph. Signs pin 9c.2 |
| M46 | evaluate restorations **individually** under `StrictInverse` | a jointly-safe grid + meter-change set must be **applied**, and a jointly-unsafe individually-safe set must **conflict** — both observed |
| M47 | replace `BestEffort`'s canonical-order greedy with a naive per-restoration filter | the safe subset must be the documented deterministic one |

### Undo — seven surface classes (M48–M62)

| M | Edit | Row |
|---|---|---|
| M48 | remove the `TimeSignature`↔`Measure` strand extension | closes a hole live **today** |
| M49 | remove the `Measure` graph-removal arm | removal from the owning instance |
| M50 | rewrite the `Measure` strand guard against `structures` | **must go red**, proving the event-only index cannot see measure anchors (`reduce.rs:1755`–`:1765`) |
| M51 | remove the spanner surface | |
| M52 | remove the repeat surface | signs pin 10.4 — `CrossCuttingValue` has no `RepeatStructure` variant |
| M53 | skip the `repeat_values` base seed | |
| M54 | remove the **`measure_values`** surface | another `Measure.start` anchored to the undone measure |
| M55 | remove the **`meter_change_chain`** surface | |
| M56 | remove the **`break_chain`** surface | |
| M57 | remove the **`page_break_chain`** surface | |
| M58 | remove the **`tempo_segment_chain`** surface | **confirmed applicable**: `TempoSegment` carries `start: TimeAnchor` **and** `end: Option<TimeAnchor>` (`tempo.rs:109`). Must be observed at **both** sites — a `start`-only guard misses a measure named as a segment's end |
| M59 | drop restoration-awareness on the shared prospective-value path, **reinstated-reference** direction (one edit) | must block. **Observed on all five restoration-capable surfaces**: spanner (`cross_cutting_modify_chain`), meter change, system break, page break, and tempo segment — the last at **both** anchor sites. Six observations, one mutant |
| M60 | drop the **added-anchor** direction on the same shared path (one edit) | must **not** block. Same five surfaces, same six observations |
| M61 | drop `!targets.contains(…)` from every surface guard (one edit) | observed on **all seven** surface rows — same-transaction teardown |
| M62 | drop the `Live` conjunct from every owned-surface guard (one edit) | observed on **seven** rows — the six always-owned classes **plus region-scoped (`Some(region)`) tempo segments**, which have a live `Region` owner. Only **score-level (`None`) tempo** is N/A. M58 cannot substitute here: it removes tempo handling wholesale and so cannot detect a *missing `Live` conjunct* in tempo handling that is present |

**Restoration is N/A for exactly two surfaces.** `repeat_values` and
`measure_values` are **immutable value maps**, not `WriteChain`s — nothing
rewrites a repeat's or a measure's anchors in place, so there is no prospective
post-undo value that could differ from the stored one. Drafts 4–5 left M59/M60
as single generic observations, which signed the *mechanism* but not its
*coverage*: a prospective-value path wired up for spanners only would have
passed both. One mutant each is still correct — the path is shared — but **every
applicable row must observe it**, and the two N/A surfaces must be named as N/A
rather than silently absent.

### Post-undo re-create ordering (M63)

| M | Edit | Row |
|---|---|---|
| M63 | make `create_measure`'s `Tombstoned` arm fall through to the value-map identity check | a **byte-identical** re-carry after undo must observe `TargetTombstoned`, not `AlreadyApplied`. Draft 3 had regression coverage here and **no killing mutation** |

### Precondition-reason decoding (M64–M66) — and a live bug

`decode.rs`'s `precondition_reason` (`:205`) ends at **13**. Reasons **14 and
15 already encode but cannot decode**, so a materialized effect carrying either
fails canonical round-trip **today**. Nothing caught it because
`generators.rs:417` draws `rng.below(14)` under a doc comment claiming "every
core and registered variant."

| M | Edit | Row |
|---|---|---|
| M64 | revert the decoder to end at 13 | reasons 16–18 round-trip |
| M65 | remove decoder arms **14/15** | the pre-existing hole, now regression-locked (**P13-S20**) |
| M66 | revert `generators.rs` `below(19)` → `below(14)` | the generator must actually reach every variant its doc claims |

### Sentinels and bookkeeping (M67–M71)

| M | Edit | Row |
|---|---|---|
| M67 | `barrier.rs` `tag: 40` → `39` | invalid-tag sentinel |
| M68 | revert the grammar count | grammar sentinel |
| M69 | revert the `saw_create_measure` assertion | generator sentinel |
| M70 | revert `COMPANION_VERSION` | companion accepts 0.13.0, rejects `(0 12 0)` |
| M71 | delete the G3b Revision History row | the `binary_format_history.rs` guard |

**Ratified total: 75 mutations, one edit each.** Every former conditional is
resolved — disposition (A) is ratified, `TempoSegment` carries two anchor sites,
and M14's fixture is concrete — so **no deviation clause remains anywhere in
this contract**. Report the observed count; any deviation from 75 is a finding.

Prose-only sentinel sites (`layout_stub.rs:1375`, `QUICKSTART.md:56`) have no
mutation; they are covered by the §4 grep gate, and the report must say so
rather than implying mutation coverage.

### Not mutations — gates

Draft 2 listed these among the mutations, which overstated its own coverage:

- **Frozen-corpus byte stability**: `spec/vectors/*.txt` gain entries and **no
  existing vector's bytes move** — a corpus **diff gate**, verified by
  inspecting the diff, not by mutating anything.
- **PDF title pages** read the bumped versions.

### The `valuegen` fixture trap, from G3a

G3a's `analysis_layer` fixture happened to encode a name at exactly 16 bytes —
the same width as the id — making a field-swap mutation **byte-invisible**.
`Measure` has five fields including two `Option`s and a `TimeAnchor`. Choose
fixture values whose encodings are **mutually distinguishable in length**, and
state in the report that this was checked. M12/M13's frozen literals are what
make it matter.

### Anti-traps

A mutation that does not compile signs nothing. A mutation in an operation that
runs *before* the asserted state cannot reach it. A symmetric encode/decode
change is invisible to round-trip assertions. A grep guard must be sliced so it
cannot match its own citation, and *name-presence* is not enough when the name
already appears in neighbouring prose. **A guard written against an index that
structurally cannot hold the referent is born green.** A row asserted but not
observed red is unsigned.

---

## §4. Gate

- `cargo test --workspace` — count compared to the **1454** baseline at
  `6170015`, delta explained.
- `cargo clippy --workspace --all-targets` — zero warnings.
- `cargo fmt --check` — clean.
- All **75** mutations observed red and restored by hand-editing back.
- The two corpus diff gates and the four PDF title pages.
- A grep gate over the prose-only sentinels (`layout_stub.rs:1375`,
  `QUICKSTART.md:56`) confirming no stale count survives.
- Whitespace per §5.

## §5. Whitespace and staging

1. Stage the touch-table files explicitly. **Never `git add -A`.**
2. **`git diff --cached --check`** — catches staged and formerly-untracked
   files. `git diff --check` alone is blind to both, which is how this
   session's own contract sat untracked past a "clean" gate.
3. Commit.
4. `git diff --check <parent>..HEAD -- crates/ spec/` — path-scoped, so the
   pre-existing `spikes/` failure cannot mask a real one.

**A concurrent session commits to this repository.** Re-check `HEAD` before
committing, commit with an explicit pathspec, and never run `git reset`,
`git restore --staged`, `git checkout`, or `git stash` against the shared
index.

## §6. Boundary — unchanged and absolute

MUST NOT be read, written, or staged: `spec/PLAN_EDITOR_APP.md`,
`spec/CONTRACT_EDITOR_*.md`, `spec/ANALYSIS_GENESIS_PERSISTENCE.md`,
`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`, `spec/DRAFT_T4_FIXTURE_RECIPE.md`,
`crates/epiphany-editor-gui/goldens/*.png`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the entire `spikes/`
tree, the root `Cargo.toml` change, `.claude/worktrees/`.

**Touch-table row 20** (`barriers.rs`, one `subjects_of` arm) is the **only**
authorized editor-crate change, and it does not generalize beyond this packet.

## §7. Report requirements

Baseline and final counts; **all 75 mutations** with their **observed** failing
test names; the four PDF title pages; anything the contract did not anticipate.

Confirm explicitly:
- `schema_major()` gained **no** arm, and the wire row used a
  **`CreateMeasure`-only** block;
- the golden lock still reads `; 30]`;
- the evidence chain lists **only** the three vocabulary-introducing events;
- **all nine** integration files in rows 6–14 were touched, plus row 12a
  (`decode.rs`), row 14a (`QUICKSTART.md`), and `core/DECISIONS.md` in row 35 —
  which is the tenth omitted G3a-footprint file;
- that disposition **(A)** was implemented — the instance ledger **plus** the
  shared oracle reconstructing inheritance from `metric_grid_chain` and
  `meter_change_chain` in canonical order, folding in prospective restorations,
  and running in **both** reduction modes (M30a, M30b, M31a);
- that M58 was observed at **both** `TempoSegment` anchor sites;
- that M14 used the ratified unreduced-`RationalTime` (`2/4`) fixture and
  observed the strict re-encode rejection;
- that **no mint precedes any invariant check** in either setter, and that a
  refusal leaves no residue in `objects`, the value maps, or the graph;
- that restoration safety was evaluated in **aggregate**, with `BestEffort`'s
  subset chosen by the documented canonical-order greedy;
- that **all seven** surface classes have guards, with the per-surface owner
  rule stated — M62 observed on **seven** rows (six always-owned plus
  region-scoped tempo), with only score-level (`None`) tempo N/A;
- that M30c observed **both** grid-chain interleavings;
- that M59 and M60 were each observed on **all five** restoration-capable
  surfaces — six observations apiece, tempo at both anchor sites — and that
  `repeat_values` and `measure_values` were recorded as **N/A** for restoration
  rather than silently omitted;
- the physical site counts actually implemented: `measure_values` **7**,
  `instance_grid` **7**, `repeat_values` **7** — **no delete site on any of
  them**, and tombstoned values deliberately retained (pin 10.4);
- that M62's repeat observation used a state where a **tombstoned repeat's
  value is retained** and still names the measure;
- that `decode.rs` decodes reasons **14 through 18** and `generators.rs` draws
  all of them — **P13-S20** filed as RESOLVED;
- whether `roundtrip.rs` enumerates kinds (row 25) — **report the finding
  either way**;
- **no existing vector's bytes moved** in either frozen corpus;
- the `valuegen` fixture's field encodings are length-distinguishable;
- M50 was observed red — the strand guard genuinely cannot be written against
  the event-only `structures` index;
- **P13-S18** and **P13-S19** were filed with status **open**.

If a pin is wrong or unsatisfiable, **stop and say so.** Pins 2, 9b, and 12
are prohibitions. Pin 6 forbids inventing a comparable relation beyond its five
shapes, and pin 6b forbids treating an ordering as a delta — the correct
response to an incomputable case is to refuse at the operation and abstain at
the invariant. Pin 9c widens this rung into two pre-existing operations, both
undo policies, and one pre-existing undo hole; **if that scope is wrong, say so
before writing code** rather than delivering an invariant nothing maintains.
