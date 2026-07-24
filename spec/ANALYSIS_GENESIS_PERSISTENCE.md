# Analysis: canonical graph-state persistence across genesis and pruning

> **Ruled 2026-07-24 — see `spec/RULING_GENESIS_PERSISTENCE.md`.** Disposition
> **B** was taken: the operation set absorbs genesis. This document stays as
> analysis and is not rewritten to match, so the evidence the ruling rests on
> remains readable; the two amendments it carries (the `Measure` correction and
> the promotion of `identity` to blocking) are marked inline where they belong.

**Status: analysis, not a ruling.** This is the field-by-field `Score` table
that `spec/PLAN_EDITOR_APP.md` §Ruling B blocker (i) requires *before* the
blocker can be resolved, and that the T1b runway names as step (1). It
decides nothing. It establishes what is true, states the three findings that
follow, and lays out the disposition options with their real costs so the
choice can be made on evidence.

Written alongside T4-pre W1 (`CONTRACT_EDITOR_T4PRE_IR.md`). Analysis only:
no code, no `.tex`, no `epiphany-core` edit — no collision with the Push-4b
track, which owns those surfaces.

All citations verified against the working tree at `85d8af6`.

---

## 1. The mechanism, as built

**Reduction has two modes, and only one of them produces a graph.**
`Reducer::new(op_set)` reduces bookkeeping alone; `new_onto(op_set, base:
&Score)` clones a base `Score` and reduces onto the clone
(`ops/src/reduce.rs:1302-1306`). Graph-aware preconditions are *skipped*
in the base-free mode — the code says so outright: "base-free reduction has
no instrument/group universe to check against" (`reduce.rs:3822`), "base-free
reduction has no staff universe to check" (`reduce.rs:3721`). So a reduction
with no base produces a `MaterializedState` and no `Score` at all.

**What the canonical document is.** `req:format:canonical-document-reduction`
(`core_spec.tex:11549-11592`): with `canonical_base = None`, the canonical
document is the deterministic reduction of the union of all
`operation_roots` envelopes; with `Some(base)`, it is the base snapshot's
**materialized state** plus the reduction of the envelopes its
`covers_causal_frontier` does not cover. Acceleration snapshots "**MUST be
ignored** for the purposes of canonical state" and must be rebuilt or
discarded on disagreement.

**What the canonical base actually carries.** `MaterializedState`
(`reduce.rs:503-522`) is: effects, conflicts, anomalies, object
existence (`objects`), spellings, system breaks, page breaks, pending. That
is **reducer bookkeeping**. It contains no `Region`, no `Staff`, no `Event`,
no `Instrument`, no `Part` — no graph values whatsoever. `GraphMaterialization`
pairs it with a `Score`, and says so plainly: the score "is derived state,
never the source of truth" (`reduce.rs:526-534`).

**What pruning does.** `req:format:pruning-state-preservation`
(`core_spec.tex:11596-11621`): materialize a snapshot covering a chosen DVV
frontier, replace `canonical_base`, **remove operation-envelope blocks
entirely covered by the new frontier**, commit atomically — and "pruning
MUSTNOT alter the canonical document state." The manifest carries
`canonical_base: Option<SnapshotRef>` and a separate, explicitly
non-canonical `acceleration_snapshots: Vec<SnapshotRef>`
(`bundle/src/manifest.rs:384-387`).

Put together: the values that make a score a score live **only** in the
envelopes and in whatever base was handed to the reducer. Pruning is licensed
to delete those envelopes, and the thing it puts in their place carries no
values.

---

## 2. The table

Every field of `Score` (`core/src/graph.rs:1710-1735`), with the operations
that write it under graph-aware reduction. "Op-covered" means *some*
operation in `OperationKind` (`ops/src/payload.rs:116-198`) can create or
change it; the writer citations are the actual mutation sites in
`reduce.rs`, distinguished from the base-seed reads at `reduce.rs:1313-1362`,
which are how a base graph's objects enter the reducer's existence index
without a minting operation.

| # | Field | Op-covered | Writing operations | Verified at |
|---|---|---|---|---|
| 1 | `metadata` | ✅ | `SetMetadata` (LWW) | `reduce.rs:2713`, `:5249` |
| 2 | `canvas.regions` | ✅ | `CreateRegion` / `DeleteRegion`, `ChangeRegionTimeModel`, `SetMetricGrid`, `SetTimeSignature`, `SetUserSystemBreak` / `SetUserPageBreak` | 29 sites incl. `:2656` |
| 2a | ↳ `StaffInstance.measures` | ❌ **none** | — see the correction below | `Measure {` built only in `testkit/src/fixtures.rs` |
| 3 | `canvas.layout_defaults` | ❌ **none** | — page size and margins | zero hits in `epiphany-ops` |
| 4 | `instruments` | ❌ **none** | — no `CreateInstrument` exists | read-only at `:1318` |
| 5 | `staves` | ✅ | `CreateStaff` (mint), tombstone removal | `:3850`, `:2560` |
| 6 | `staff_groups` | ❌ **none** | — | read-only at `:1329` |
| 7 | `parts` | ❌ **none** | — | read-only at `:1333` |
| 8 | `cross_cutting` | ✅ | `CreateCrossCutting` / `Delete` / `Modify`, `CreateRepeatStructure` / `DeleteRepeatStructure` | 38 sites |
| 9 | `time_signatures` | ✅ | `SetTimeSignature` (set-union mint) | `:3891`, `:2563` |
| 10 | `tuning_context` | ❌ **none** | — pitch space, tuning system, reference | zero hits in `epiphany-ops` |
| 11 | `tempo_map` | ✅ | `SetTempoSegment` | `:4142` |
| 12 | `events` | ✅ | `InsertEvent`, `DeleteEvent`, `ModifyEvent`, `Insert`/`Delete`/`ModifyIdentifiedPitch`, `Transpose`, `TransposeInterval` | 28 sites |
| 13 | `spelling_attachments` | ✅ | `RespellPitch` | 15 sites |
| 14 | `decomposition_attachments` | ⚠️ **removal only** | reduction only *retains*; the creator is the prepass | `:2342`; `core/src/prepass.rs:382` |
| 15 | `spelling_precedence` | ❌ **none** | — | zero hits in `epiphany-ops` |
| 16 | `analysis_layers` | ❌ **none** | — | read-only at `:1345` |
| 17 | `views` | ❌ **none** | — | read-only at `:1349` |
| 18 | `identity` | ❌ **none** | — the `IdentityContext` itself | zero hits |
| 19 | `tombstoned_pitches` | ✅ derived | delete/tombstone paths | `:2526` |
| 20 | `tombstoned_events` | ✅ derived | delete/tombstone paths | 3 sites |

**Eight top-level fields have no operation that can produce them** (3, 4, 6, 7,
10, 15, 16, 17, plus `identity` at 18), and one more (14) can only be pruned
back, never authored. Of the covered ones, several are covered *only in the
graph-aware mode* — they write through `if let Some(score) = self.graph`.

> **Correction (2026-07-24, made while ruling on this analysis).** Row 2 scored
> `canvas.regions` op-covered at *container* granularity — regions, staff
> instances, voices — and that hid a ninth gap one level deeper. **`Measure` is
> authored by nothing**: no operation mints it, no reducer path writes it, and
> `Measure {` is constructed only in `testkit/src/fixtures.rs` (`:137`, `:235`,
> `:389`). `CreateStaffInstance` moreover *refuses* an instance carrying
> measures (`reduce.rs:3715`, container-not-empty), so they cannot enter at
> instance-mint either. A from-empty document therefore cannot have a measure,
> which puts this on the critical path rather than in the margins. Ruled
> **authored, not derived** (2026-07-24): `TimeAnchor::Measure { id, .. }` means
> cross-cutting structures anchor to measure ids, so deriving measures from the
> metric grid would make their identity a function of the meter and every
> time-signature change would orphan the anchors pointing into them. The cost
> accepted with that ruling is that measure/meter consistency becomes an
> authoring obligation backed by a graph invariant, not a model guarantee.

---

## 3. The three findings

### Finding 1 — the genesis root is unreachable from an empty base

The creation chain is instrument → staff → staff instance → voice → event,
and each link is enforced under graph-aware reduction:

* `CreateStaff` refuses unless `TypedObjectId::Instrument(op.staff.instrument)`
  is live (`reduce.rs:3824-3833`) — and **no operation creates an
  `Instrument`**. Genesis is outside the operation set by ratified decision
  ("there is no `CreateCanvas`/`CreateInstrument`",
  `binary_format.tex:2420`; Pass-12 K8).
* `CreateStaffInstance` refuses unless the referenced global `Staff` is live
  (`reduce.rs:3723-3734`).

So reduction onto `Score::empty(identity)` can never reach a note. The escape
hatch that exists today is not a design — it is the base-free mode, which
skips the preconditions precisely *because* it has no graph to check, and
produces no graph either.

### Finding 2 — the canonical base cannot carry what the base-only fields hold

The eight uncovered fields can enter a `Score` only by being in the base
handed to the reducer. Pruning replaces that base with a `MaterializedState`
snapshot, which carries none of them, and deletes the covered envelopes —
which never carried them either. **After a prune, the uncovered fields are
gone with no canonical way to recover them**, and the acceleration snapshot
that does hold full-`Score` bytes is normatively forbidden as a source of
canonical state.

This is not only an editor concern. Field 3 is the printed page geometry;
field 10 is the tuning context the entire Push-4b resolver consumes; field 7
is the part definitions that `req:graph:part-content-projection` calls
normative.

### Finding 3 — there is no canonical wire path from bundle bytes to a `Score`

`canonical_base` holds `MaterializedState`; a base stamped above schema major
0 forces read-only (`bundle.rs:857`); full-`Score` bytes exist on the wire
only in the acceleration-snapshot role, which MUST be ignored. Consequently
the editor cannot open a document today — and Fact 3 of the plan records the
symptom: the GUI opens a hard-coded testkit fixture, and neither editor crate
even depends on `epiphany-bundle`.

The three findings are one problem seen from three sides: **the canonical
document format has no representation for graph state that no operation
authors.**

---

## 4. Dispositions

Four coherent options. They are not mutually exclusive — A and D compose, and
C subsumes much of A.

**A. A canonical genesis block.** A new chunk role, canonical and never
pruned, carrying exactly the uncovered fields: canvas layout defaults,
instruments, staff groups, parts, tuning context, spelling precedence,
analysis layers, views, identity. Reduction takes it as the base. This keeps
the ratified "genesis outside the operation set" decision intact — genesis
becomes *persisted canonical input* rather than operations — and it is the
smallest change that makes documents openable. Cost: one format addition and
a manifest field; the hard question it must answer is what happens when two
replicas' genesis blocks disagree, since a non-op payload has no CRDT merge
rule. Concurrent editing of genesis fields would remain unsupported, which
is honest for v1 but is a real product ceiling (adding an instrument to a
shared score is a normal edit).

**B. Close the op-coverage gap.** Add `CreateInstrument`, `CreateStaffGroup`,
`CreatePart`, `SetCanvasLayoutDefaults`, `SetTuningContext`,
`SetSpellingPrecedence`, and analysis/view CRUD, making the op log
self-sufficient. This is the only option under which genesis state converges
concurrently like everything else, and it makes the grow-only log the whole
truth. Cost: it reverses a ratified Pass-12 decision, and each operation
needs reduction semantics, conflict behaviour, undo behaviour, and catalog
text — a schema major spent deliberately, in the Push-4b mold, not a
tranche this track can absorb. Note it does **not** by itself fix pruning:
the envelopes carrying these ops become prunable like any other.

**C. Promote the canonical base to carry graph values.** Make the
canonical-base snapshot *checkpointed reducer state plus graph state* rather
than bookkeeping alone. This is the same shape T4b needs for incremental
materialization ("checkpointed-reducer-state-plus-tail"), so the two tracks
would pay for one mechanism. It also repairs pruning for **every** field at
once, covered or not. Cost: the largest format change of the four; the base
role's schema-major-0 pin and the read-only-above-0 rule
(`bundle.rs:857`) must be revisited; and the `SnapshotId` derivation is
today an acknowledged test-harness stand-in with no normative derivation
(`binary_format.tex:698`), which would have to become real before snapshots
are load-bearing.

**D. Scope-limit T1b.** Ship the document layer for documents whose genesis
is empty, metadata-only, or region-only, and refuse the rest cleanly —
the plan's existing escape hatch. Cost: an editor that cannot open a score
with an instrument in it is not the editor; this is a way to land the
bundle/session/lease machinery (Ruling D's unforgeable lease, single-writer
enforcement, the save protocol) against a real format while the genesis
decision is made properly. It buys sequencing, not a solution.

**The recommendation I would defend:** **D now, C as the destination, A only
if C proves too large to sequence.** D unblocks the Ruling-D ownership API
and the save/dirty protocol — the parts of T1b that the editor track actually
needs next and that are independent of this question. C is where the format
wants to end up, because it is the only option that makes pruning safe for
every field rather than for an enumerated list, and because T4b needs the
same checkpoint mechanism regardless. B is the right long-run answer for
*concurrent* genesis editing, but it is a Push-4b-class spec tranche and
should be sequenced by product priority, not by this blocker.

Whatever is chosen inherits one hard constraint: pruning MUSTNOT alter the
canonical document state (`core_spec.tex:11612-11616`). Any option that
enumerates fields must be re-audited against this table every time a field is
added to `Score` — which argues, again, for C.

---

## 5. Open questions for the ruling

1. **Genesis divergence.** Under A, what is the merge rule when two replicas
   present different genesis blocks for the same document id? (Under B this
   question dissolves; under C it becomes snapshot-disagreement handling,
   which the spec already answers for acceleration snapshots.)
2. **Is `identity` document state or session state?** It is on `Score` today
   with no op coverage. If a document has one `IdentityContext` and each
   session mints under its own replica id, the field's role needs stating.
   **Promoted to blocking (2026-07-24):** under the ruled disposition,
   reduction runs onto `Score::empty(identity)`, so whoever opens the document
   chooses the value — and the codec *encodes* it (`codec.rs:2766`, `:3223`),
   so two replicas reducing an identical log produce Scores differing in an
   encoded field while the music is identical. `IdentityContext` is
   `{replica_id, next_counter}`, replica-scoped by construction
   (`ids.rs:711-717`). This must be dispositioned before the tranche can be
   specified; the likely answer is that it belongs in the manifest rather than
   the graph.
3. **`decomposition_attachments` and `spelling_precedence`** are consumed by
   the prepass. Are they derived state that should be rebuilt rather than
   persisted — in which case they leave this table — or authored state?
   Field 14's removal-only reduction path suggests the former.
4. **Does the canvas's page geometry belong to the document or to a view?**
   §3.2's document ≠ session ≠ view split may relocate field 3 entirely,
   and part-specific page geometry is a real product requirement.

---

*Related: `spec/PLAN_EDITOR_APP.md` §Ruling B, §3.1, §3.2, §3.7;
`spec/CONTRACT_EDITOR_T4PRE_IR.md`.*
