# Contract: G-minor vocabulary audit — a reachability matrix, and nothing else

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/PLAN_GMINOR_SCHEMA_MINOR.md` (§3 and §5.1; policy ratified 2026-07-28)
and `spec/PASS13_CANDIDATES.md` (P13-S14).

**This is a READ-ONLY packet.** It produces one document. It changes no Rust,
no `.tex`, no vectors, and no goldens.

**Parallel safety.** The editor track owns `crates/epiphany-editor-gui/**`,
`crates/epiphany-render-svg/**`, `crates/epiphany-glyphs/**`,
`spec/PLAN_EDITOR_APP.md`, every `spec/CONTRACT_EDITOR_*.md`,
`spec/DRAFT_T4_FIXTURE_RECIPE.md`, the entire `spikes/` tree, and the current
unstaged root `Cargo.toml` change. **All of it is out of scope**, and since
this packet writes only one new file, none of it should ever be staged.

---

## Why this packet exists

The G-minor rung cannot be contracted until every append-only discriminant
reachable from an affected chunk payload is known. A first attempt scoped the
problem as per-`OperationKind` and recommended a policy that could not work —
an envelope also emits the outer `OperationPayload` discriminant, and
`ResolveEquivocation` carries no `OperationKind` at all. **That scoping error
is the reason this audit is a separate packet rather than a paragraph in a
contract.**

## The deliverable

One new file, `spec/AUDIT_GMINOR_VOCABULARIES.md`, whose core is a
**reachability matrix** with exactly these columns:

| chunk role | encoded payload type | discriminant vocabulary | post-baseline variants | introduction event | derivation site |

* **chunk role** — a `ChunkKind` variant (`bundle/src/chunk.rs:18`; nine of
  them). Every role gets at least one row or an explicit "emits no additive
  discriminant" row with the reason.
* **encoded payload type** — the Rust type whose canonical encoding *is* that
  role's payload. Some roles are payload-polymorphic (`Snapshot` is the
  canonical-base `MaterializedState` *or* the acceleration full-`Score`); those
  get one row per form, distinguished.
* **discriminant vocabulary** — the enum whose tag is written.
* **post-baseline variants** — the specific variants appended after the
  format's initial ratification, listed individually with their discriminant
  values. Not a count.
* **introduction event** — the tranche or revision that appended it (e.g.
  "Phase-3 first tranche", "schema-major-2 repeat revision", "Push 4a",
  "genesis G1", "genesis G2a"). **Name the event, not an epoch number.**
* **derivation site** — `file.rs:line` of the code that writes the tag, so the
  G-minor implementation knows where `introduced_minor` has to live.

## Method — walk the encoder, never the type

**The ground truth is the encoder, not the struct definition.** A field can
exist on a type and never reach the wire: `ScoreTuningContext::accidental_extensions`
is staged out of schema major 3 and its `Codec` deliberately drops it
(`core/src/codec.rs:1939`). A type-driven walk would record a vocabulary that
is not actually emitted, and the matrix's whole purpose is to say what is
*emitted*.

So: start from each role's payload type, walk its `encode_canonical` /
`Codec::enc` implementation transitively, and record every site that writes a
tag or discriminant. `grep -rn "push_tag\|fn discriminant"` over
`crates/epiphany-core/src` and `crates/epiphany-ops/src` returns ~65 hits
across 11 files and is a reasonable starting net — but it is a *net*, not the
method. Confirm reachability from a role by following the encoder.

## Inclusions and exclusions

**IN scope:**

* Native variants appended to any open vocabulary after baseline.
* **Nested** additive variants — a vocabulary reached only through another
  value's encoder counts, at whatever depth.
* Later native additions to enums that *also* carry a `Registered` escape.
  **This is the audit's sharpest edge**: the escape variant is out of scope,
  but a new native variant on the same enum is in scope, and the two are easy
  to conflate. `binary_format.tex` names fifteen escape carriers
  (`RepairKind`, `ReanchorReason`, `PreconditionFailureReason`,
  `IntegrityAnomalyKind`, `ReplicaAnomalyReason`, `TransactionCategory`,
  `ResolutionAction`, `ConflictKind` via `ExtensionConflict`,
  `BarrierScope`/`BarrierCondition`, `TieClass`, `StaffGroupKind`,
  `PitchSpacePosition`, `SpellingNominal`, `TypedObjectId`, and the barrier
  `ObjectKind`); every one needs checking for native appends.

**OUT of scope:**

* **`Registered` payload values.** Extension through an escape variant is not a
  schema change at all — the wire form is already defined.
* **`ChunkKind` itself.** It is closed: no `Registered` variant, and its
  discriminant enters every chunk's hash preimage, so a new chunk kind is a
  format-**major** event.
* Anything requiring a schema-major bump. Field additions are major
  *regardless of type* — an `Option` still occupies a positional slot
  (`binary_format.tex:2360`). Only discriminant appends are minor-additive.

## Traps

1. **The spec's "append at ≥N" phrasing names the next free slot, not the
   baseline boundary.** `binary_format.tex:2369` says `OperationPayload`
   appends at ≥4 — yet discriminant **3** (`ResolveEquivocation`) is itself an
   append, as both the spec (`:1291`) and the code comment
   (`payload.rs:80`, "the ratified 0..=2 stay stable") state. Likewise
   `OperationKind` "appends at ≥30" while 24–29 were also appends. **Do not
   derive post-baseline membership from the ≥N numbers.** Establish each
   variant's introduction event from the spec's revision history, the
   `DECISIONS.md` files, and `git log` — and say which source settled it when
   they disagree.
2. **`OperationKind` and `OperationKindTag` are independent, misaligned
   spaces.** `RespellPitch` is kind 2 / tag 3. They are two rows, never one.
3. **Payload-polymorphic roles.** `Snapshot` carries both the canonical-base
   `MaterializedState` and the acceleration full-`Score`; they emit different
   vocabularies and must not be merged. The canonical base notably does **not**
   emit `OperationKind` at all.
4. **A hand-written `discriminant()` match is not evidence of completeness.**
   `OperationKind::discriminant()` is exactly the site Push 4a got wrong. Where
   a macro-generated vocabulary exists (`operation_kind_tag_vocabulary!`),
   prefer it as the source of truth and note the divergence if the hand-written
   sibling disagrees.

## What this packet must NOT do

* **Do not ratify epoch numbers.** Column 5 records the introduction *event*.
  Assigning epochs is a later ruling that depends on this matrix being
  complete.
* **Do not write or modify any Rust**, including tests, and do not add
  `introduced_minor` anywhere.
* **Do not modify any `.tex`**, vectors, or goldens.
* **Do not draft the G-minor implementation contract.**
* Do not touch `spikes/`, the root `Cargo.toml`, or any editor-track file.

## Completeness — how to know the matrix is done

The audit's failure mode is a missed vocabulary, and it is silent. So the
report must state, for each of the nine roles, either the rows it produced or
why it produces none — an unlisted role is indistinguishable from an
overlooked one.

State explicitly:

* which roles emit no additive discriminant, and why;
* every escape-carrying enum checked for native appends, **including those that
  turned out to have none** — a negative result recorded is evidence, a
  negative result omitted is a gap;
* any variant whose introduction event could not be established from the
  sources in trap 1, flagged as **unresolved** rather than guessed;
* any place where the spec and the code disagree about what was appended when.

**A guess presented as a finding is the worst possible output here**, because
the epoch mapping will be ratified on top of it and a wrong epoch is a wrong
wire record that then freezes.

## Gate

`cargo fmt --check` and `cargo test --workspace` are expected to be **untouched
and green** — that is the check that this packet really was read-only. Report
`git status --short` verbatim so the file list can be seen to contain exactly
one new file.

## Report

The matrix itself, plus: the completeness statement above; the sources used to
establish introduction events and which settled disputes; anything found that
the plan's §3 four-vocabulary table did not name; and any place where this
contract's own assumptions turned out to be wrong. That last one is not
politeness — the plan this packet serves already had to be revised once for a
scoping error of exactly this kind, and the contract's four-vocabulary starting
table is a floor, not a ceiling.
