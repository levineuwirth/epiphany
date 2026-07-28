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

One new file, `spec/AUDIT_GMINOR_VOCABULARIES.md`, containing **two linked
parts**.

### Part 1 — the vocabulary ledger

Every reachable vocabulary, with **every** variant and its discriminant,
classified as one of:

* **baseline** — present at the format's initial ratification;
* **post-baseline native** — appended later (the rows the matrix cares about);
* **escape / reserved** — `Registered` and any reserved slot.

Baseline variants are **mandatory**, not optional. Enumerating them is what
makes each post-baseline classification *checkable* against a complete list
rather than asserted — and this plan has already been revised once for a
vocabulary that was scoped by assertion.

The ledger is the single place large baseline sets are written down, so the
matrix can reference a vocabulary by name instead of repeating it per role.

### Part 2 — the reachability matrix

Keyed by role, **referencing** Part 1 rather than restating it:

| chunk role | encoded payload type | discriminant vocabulary | post-baseline variants | introduction event | derivation site |

* **chunk role** — a `ChunkKind` variant (`bundle/src/chunk.rs:18`; nine of
  them). Every role gets at least one row or an explicit "emits no additive
  discriminant" row with the reason.
* **encoded payload type** — the Rust type whose canonical encoding *is* that
  role's payload. Some roles are payload-polymorphic (`Snapshot` is the
  canonical-base `MaterializedState` *or* the acceleration full-`Score`); those
  get one row per form, distinguished.
* **discriminant vocabulary** — the enum whose tag is written. This is the
  reference into Part 1; the ledger holds its full variant set.
* **post-baseline variants** — the specific variants appended after the
  format's initial ratification, listed individually with their discriminant
  values. Not a count. **Must agree exactly** with Part 1's `post-baseline
  native` classification for that vocabulary; a disagreement between the two
  parts is a defect in the audit, not a nuance to explain away.
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

**IN scope: every reachable discriminant vocabulary — whether open,
append-only, or expanded by a ratified revision.** That last clause is not
decorative: `binary_format.tex:2386` states that the value-layer unions
*enumerated as closed* in `req:binfmt:frozen-layout` may still gain appended
variants through a ratified revision of that document, **and that such appends
are also minor-additive**. A vocabulary described as closed is therefore not
automatically out of scope; only `ChunkKind` is (see below).

* Native variants appended to any such vocabulary after baseline.
* **Nested** additive variants — a vocabulary reached only through another
  value's encoder counts, at whatever depth.
* Later native additions to enums that *also* carry a `Registered` escape.
  **This is the audit's sharpest edge**: the escape variant is out of scope,
  but a new native variant on the same enum is in scope, and the two are easy
  to conflate.

  **This checklist is authoritative; no count of it is.** `BarrierScope` and
  `BarrierCondition` are *separate* enums (`layout-ir/src/barrier.rs:60`,
  `:75`), and a prose summary that pairs them undercounts — which is exactly
  why the number is omitted here rather than corrected:

  1. `RepairKind`
  2. `ReanchorReason`
  3. `PreconditionFailureReason`
  4. `IntegrityAnomalyKind`
  5. `ReplicaAnomalyReason`
  6. `TransactionCategory`
  7. `ResolutionAction`
  8. `ConflictKind` (via `ExtensionConflict`)
  9. `BarrierScope`
  10. `BarrierCondition`
  11. `TieClass`
  12. `StaffGroupKind`
  13. `PitchSpacePosition`
  14. `SpellingNominal`
  15. `TypedObjectId`
  16. the barrier `ObjectKind`'s open value space

  Every entry needs checking for native appends, and every entry needs a
  recorded result — including the clean ones.

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

The audit's failure mode is a missed vocabulary, and it is silent. So every one
of the nine roles must be accounted for — an unlisted role is
indistinguishable from an overlooked one. **Each role resolves to exactly one
of four dispositions**, and "opaque" is not one of them, because opacity is a
property of a *layer*, not of the bytes:

**(a) Rows.** The role's payload encoder is walked and yields matrix rows.

**(b) Emits no additive discriminant.** Stated with the reason.

**(c) Normatively typed bytes, opaque only to the immediate layer.** The
carrying layer treats the payload as bytes, but a specification pins what
produced them — operation envelopes inside an `OperationEnvelopeBlock`, and
barrier blobs inside a manifest, are both of this shape. **The audit must cross
that boundary and walk the specified producer's encoder.** Treating these as
opaque would silently drop the single most important vocabulary in the whole
matrix, since the op-block role is the entire reason G-minor exists.

**(d) Truly producer-owned opaque bytes.** `ExtensionData` is the case: no core
encoder produces it and no core derivation is possible. Record that explicitly
— *the schema is carried from the producer, and core cannot derive a minor for
it* — and do **not** write it up as emitting nothing. "We cannot see inside" and
"there is nothing inside" are different findings, and only the first is true
here.

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
and green** — that is the check that this packet really was read-only.

**The status check is a delta, not an absolute.** The working tree is already
dirty with the editor track's work — a modified root `Cargo.toml` and an
untracked `spikes/` subtree — so "exactly one file" is unsatisfiable and any
gate demanding it would have to be either faked or argued around. Instead:

1. Capture `git status --short` **before** starting. Include it in the report.
2. Capture `git status --short` **after**. Include it in full.
3. The **only** difference between them must be the addition of
   `?? spec/AUDIT_GMINOR_VOCABULARIES.md`.

Any other delta — including a `Cargo.lock` touched by running `cargo` — is a
finding to report, not something to clean up silently.

## Report

The matrix itself, plus: the completeness statement above; the sources used to
establish introduction events and which settled disputes; anything found that
the plan's §3 four-vocabulary table did not name; and any place where this
contract's own assumptions turned out to be wrong. That last one is not
politeness — the plan this packet serves already had to be revised once for a
scoping error of exactly this kind, and the contract's four-vocabulary starting
table is a floor, not a ceiling.
