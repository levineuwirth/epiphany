# Ruling: the operation set absorbs genesis

**Ratified 2026-07-24.** Resolves `spec/PLAN_EDITOR_APP.md` §Ruling B blocker
(i) — canonical graph-state persistence across genesis and pruning. The
evidence is `spec/ANALYSIS_GENESIS_PERSISTENCE.md`, which stays as analysis;
this document is the decision and its constraints.

Execution belongs to the **Push-4b-class coordinated spec+core+ops track**,
not to the editor track. The editor track consumes it and does not block on
it (§3.7's standing posture).

---

## 1. The decision

**Every mutable field of `Score` becomes operation-authored.** The Pass-12 K8
ratification — "genesis is outside the operation set", `binary_format.tex:2420`
— is **reversed**. A document is `Score::empty(identity)` plus its envelope
log; the instrument → staff → staff instance → voice → event chain is
authorable end to end.

**There is no genesis block.** No new chunk role, no manifest field, no
write-once payload, no immutability rule, and no merge or fail-closed rule for
a non-CRDT canonical blob. That machinery was the price of leaving genesis
outside the operation set, and this ruling declines to pay it. Genesis becomes
the first few operations of an ordinary log, converging under the rules
already in force.

**Rationale, in one line:** genesis state is edited — instruments are added,
page geometry is changed, temperaments are chosen — and every alternative made
those edits either single-writer, unmergeable, or impossible. Concurrency is a
first-order product commitment (§3.4), so the exception was not worth
institutionalising in the format.

### What was considered and rejected

* **A — a canonical genesis block.** Rejected: it fixes genesis but leaves the
  *music* unreconstructable after a prune, and a non-op canonical payload has
  no merge rule, forcing either single-writer genesis or a fail-closed sync
  rule. Considered in a staged form (immutable block + settings ops) and
  rejected once it became clear the block buys nothing that B does not.
* **C — the canonical base carries graph values.** Not rejected — **deferred**,
  and still required. See §4.
* **D — scope-limit T1b.** Rejected as a destination; unnecessary as a
  sequencing device once B is committed to.

---

## 2. Scope

Two existing reduction patterns cover the whole tranche. Neither is new
semantics; both are templates already proven in `reduce.rs`.

**Settings — LWW field-overwrite, the `SetMetadata` pattern** (`reduce.rs:2713`,
seeded for value-restoring undo at `:1357`):

| Field | Operation |
|---|---|
| `canvas.layout_defaults` | `SetCanvasLayoutDefaults` |
| `tuning_context` | `SetTuningContext` |
| `spelling_precedence` | `SetSpellingPrecedence` |

**Entities — set-union mint with byte-identical re-carry idempotence, the
`CreateStaff` pattern** (`reduce.rs:3850`, graph-aware preconditions at
`:3824`):

| Field | Family |
|---|---|
| `instruments` | `CreateInstrument` … |
| `staff_groups` | `CreateStaffGroup` … |
| `parts` | `CreatePartDefinition` … |
| `analysis_layers` | `CreateAnalysisLayer` … |
| `views` | `CreateView` … |
| `StaffInstance.measures` | `CreateMeasure` … |

Delete and modify coverage per family is the tranche contract's design work,
guided by the existing precedent (Group 3's "mint + empty-only delete" for
containers; `CreateStaff` lands today with no `DeleteStaff`, so full CRUD is
not automatically owed). What **is** owed is that every family's referential
preconditions are **graph-aware**, in the shape `CreateStaff` already uses:

* `CreateStaffGroup.members`, `CreatePartDefinition.staves` → live `Staff`s;
* `CreateView.active_layers` → live `AnalysisLayer`s;
* `CreateMeasure` → a live `StaffInstance`;
* deleting an entity with live dependents → refuse (container-not-empty).

### Measures: authored, not derived

Ruled 2026-07-24. `TimeAnchor::Measure { id, .. }` means cross-cutting
structures anchor to measure ids; deriving measures from the metric grid would
make their identity a function of the meter, so every time-signature change
would re-derive a different measure set and orphan the anchors pointing into
it. Authored ids survive a meter change.

**Accepted cost:** measure/meter consistency becomes an authoring obligation
backed by a graph invariant, not a model guarantee. The invariant belongs in
the tranche.

### Out of scope, with reasons

* **`identity`** — not an authored field; see §3, which must be settled first.
* **`decomposition_attachments`** — **derived, not authored.** The prepass
  creates it (`core/src/prepass.rs:382`); reduction only ever *retains*
  (`reduce.rs:2342`). It leaves the table rather than gaining operations.
  Flagged for ratification with the tranche.

---

## 3. The one open sub-decision, and it blocks specification

**`identity` must be dispositioned before the tranche can be specified.**

`IdentityContext { replica_id, next_counter }` is replica-scoped by
construction (`ids.rs:711-717`) yet lives on `Score`, which the codec
**encodes** (`codec.rs:2766`, `:3223`). Today the value is inherited from a
shared base, which masks the tension. Under this ruling reduction runs onto
`Score::empty(identity)`, so whoever opens the document chooses it — and two
replicas reducing an identical log then produce Scores differing in an encoded
field while the music is identical.

Options: move it to the manifest (document-scoped, recommended); keep it on
`Score` but exclude it from the canonical encoding; or accept
replica-dependent `Score` bytes and confine byte-equality claims to
`MaterializedState`. **Not ruled here.**

---

## 4. Standing constraints

**Pruning MUST NOT be implemented until disposition C lands.** This ruling
makes documents openable, editable, and collaborative; it does **not** make
them prunable. A prune deletes the covered envelopes and installs a
`MaterializedState` base carrying no graph values, so the score is
unreconstructable afterward — and `MaterializedState.effects` are outcomes,
not payloads, so nothing rebuilds it. Pruning is unimplemented today (no
`fn prune`; `opset.rs:7` calls it out-of-scope-for-v0), which is why this costs
nothing now and would cost everything later. C — the canonical base carrying
graph values, the same checkpoint T4b needs for incremental materialization —
remains **required before any pruning implementation**.

**The from-empty path must reduce with a graph, not base-free.** Reduction has
two modes, and graph-aware preconditions are skipped in the base-free one
*because it has no universe to check against* (`reduce.rs:3822`, `:3721`).
`Score::empty(identity)` **is** a graph, so `new_onto` with an empty score
enforces every precondition from the first operation; the base-free mode does
not. A from-empty document reduced through the wrong entry point silently
loses referential enforcement. Name this in the tranche and test it.

**One accept-set raise, spent once.** `OperationEnvelopeBlock` is capped at
major 2 (`bundle.rs:69`) because no operation payload embeds these types. The
first new kind that does raises it to 3; every later kind in that major is
free. The new kinds therefore land as **one batch**, not dribbled out per
need. Note this is a *different* major from Push-4b's schema major 3, which is
the Score/Snapshot-role wire that tranche 3b-i opened and permanently froze —
there is no free ride between them.

**Additive discriminants.** New kinds extend past `TransposeInterval` under
the existing convention (`req:binfmt:kind-discriminants`); canonical bytes for
existing types do not move.

---

## 5. Acceptance

* A document created empty and given **only operations** materializes a
  note-bearing `Score` — the full chain, no fixture, no base.
* Two replicas applying concurrent genesis-era operations in any delivery
  order converge to byte-identical `MaterializedState`.
* Opening such a document from a bundle reaches a note, which is what
  unblocks T1b.
* Existing canonical bytes, goldens, and conformance gates unmoved except
  where the accept-set raise is the deliberate change.

---

## 6. What this unblocks

`spec/PLAN_EDITOR_APP.md` §Ruling B blocker (i) is **resolved**. Blocker (ii)
— the version-aware envelope decoder — is unaffected and still open; its
residual is one bounded ops packet. T1b's remaining runway is therefore
blocker (ii) plus the Ruling-D ownership API, and the lease/save/single-writer
machinery can be contracted in parallel with this tranche, since it does not
depend on genesis being authorable.

*Related: `spec/ANALYSIS_GENESIS_PERSISTENCE.md`, `spec/PLAN_EDITOR_APP.md`
§Ruling B / §3.7, `spec/PLAN_PUSH4B_TUNING.md` (the tranche mold).*
