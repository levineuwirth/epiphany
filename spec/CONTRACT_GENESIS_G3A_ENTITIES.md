# Contract: Genesis G3a — the four root-level entity mints

**Governed by** `spec/RULING_GENESIS_PERSISTENCE.md` and
`spec/PLAN_GENESIS_OPS.md` §4 (G3 split ratified 2026-07-29) and §6 (rulings 1,
2, 3 ratified 2026-07-29). Predecessor rung: G2b, signed off at `25c4733`.

**Scope.** Four operations that mint the four remaining root-level `Score`
entity vectors:

| Op | Kind | Tag | Carried type | `Score` field | `schema_major()` |
|---|---|---|---|---|---|
| `CreateStaffGroup` | 35 | 35 | `StaffGroup` | `staff_groups` | 0 |
| `CreatePartDefinition` | 36 | 36 | `PartDefinition` | `parts` | 0 |
| `CreateAnalysisLayer` | 37 | 37 | `AnalysisLayer` | `analysis_layers` | 0 |
| `CreateView` | 38 | 38 | `ViewDefinition` | `views` | 0 |

All four ride the `CreateStaff` set-union mint pattern (`reduce.rs:4075`) with
byte-identical re-carry idempotence. Epoch **11** for all four.

**Explicitly out of scope:** `CreateMeasure` (G3b), every delete (§6.1,
deferred), graph invariant 20 (G3b), any new `PreconditionFailureReason` (G3b),
and pruning or compaction of any kind (standing prohibition, see pin 9).

---

## 1. Why this rung exists — a live defect, not a completeness item

All five G3 object kinds become `Live` in the reducer's object map **only**
through base ingest (`reduce.rs:1449`–`:1563`). No operation mints any of them.
Two consequences hold in the tree today:

* `CreateStaff` validates `Staff.group` against a live `StaffGroup`
  (`reduce.rs:4119`). Under **from-empty** reduction — the path G1 created and
  T1b depends on — that precondition is **unsatisfiable**. A document built
  only from operations can never author a grouped staff.
* `TimeAnchor::Measure` (`reduce.rs:1280`) can never resolve from empty. That
  half is G3b's.

G3a closes the staff-group half and completes the four root-level vectors. It
is the last genesis rung that moves no wire bound.

---

## 2. Design pins

### Pin 1 — kinds and tags are 35–38, in **both** spaces, and they are aligned here

Next free is kind 35, tag 35 (`payload.rs:390`, `:714`, re-verified
2026-07-29). The two spaces are **not** aligned in general —
`OperationKind::discriminant()` is a hand-written match (`payload.rs:253`)
while `OperationKindTag` is macro-generated (`payload.rs:440`); `RespellPitch`
is kind 2 and tag 3. They happen to coincide from 24 upward. **Assign each
space explicitly and never derive one from the other**; pin 1's test asserts
both independently.

Assignment order is fixed as the table in §Scope: StaffGroup 35, PartDefinition
36, AnalysisLayer 37, View 38.

### Pin 2 — `schema_major()` gains **no arm**; the catch-all `_ => 0` is correct

This is G2a's shape, not G2b's. All four carried types have exactly one byte
layout: no versioned walk exists for any of them, and both `decode_v0_score`
(`codec.rs:2698`) and the live walk (`:3274`) read all four through plain
`Codec::dec`. **Adding them to the `=> 2` arm would be the bug.** Verify by
reading the walks, not by assuming.

Consequence: **no `epiphany-bundle` change of any kind.** The op-block
accept-set stays at 3 where G2b left it. If the implementation touches a bundle
file, something is wrong.

### Pin 3 — the four types need `canonical_value!` entries, and nothing else in core

Each already has a `Codec` — `struct_codec!(PartDefinition …)`
(`codec.rs:1790`), `AnalysisLayer` (`:1791`), `ViewDefinition` (`:1792`),
`StaffGroup` (`:2329`) — and each is already exported from `lib.rs`. **None is
in `canonical_value!`** (`codec.rs:3518`); G3a adds exactly four entries, in
the §Scope order, under one comment naming this contract.

`canonical_value!` introduces **no new byte layout** — it makes the existing
whole-score layout reachable per-value, and its generated `decode_canonical`
gives strict canonical-form enforcement (decode → `finish()` → re-encode →
reject on mismatch) for free.

**No `textvalue_graph.rs` work.** `struct_codec!` generates the `TextValue`
impl as well as the `Codec` (`codec.rs:510`, `:522`), so all four types already
project and parse. This is the one place G3a is *cheaper* than G2b, which had
to hand-write `TextValue for TuningContextSettings`. Confirm it by compiling,
not by assuming.

### Pin 4 — referential preconditions are **graph-aware**, mirroring `CreateStaff`

Per ruling §2, and copying `create_staff`'s structure (`reduce.rs:4102`–`:4125`)
including its `if self.graph.is_some()` guard — base-free reduction has no
universe to check against and MUST NOT enforce these:

| Op | Precondition | Failure reason |
|---|---|---|
| `CreateStaffGroup` | every `members[i]` is a live `Staff` | `TargetMissing` |
| `CreatePartDefinition` | every `staves[i]` is a live `Staff` | `TargetMissing` |
| `CreateAnalysisLayer` | *(none — no outbound references)* | — |
| `CreateView` | every `active_layers[i]` is a live `AnalysisLayer` | `TargetMissing` |

**Reuse `TargetMissing` (discriminant 0). Add no new
`PreconditionFailureReason`** — that space stays at 0–15 until G3b.

Mint preconditions are `CreateStaff`'s exactly: a live id re-carried with a
byte-identical value is `NoOp { AlreadyApplied }`; a live id with a differing
value is `NoOp { PreconditionFailedUnderReduction { RecreateContentMismatch } }`;
a tombstoned id is `NoOp { TargetTombstoned }`.

**The packet is self-contained**: `CreateView`'s precondition target is minted
by `CreateAnalysisLayer` in this same packet, so the ordering is testable
end-to-end without a base score.

### Pin 5 — every precondition must correspond to an existing invariant-10 check

Invariant 10's **body** already resolves a staff's group, a group's members, a
part's staves, and a view's active layers (`core/src/invariants.rs:1122`–`:1156`).
The reducer's new preconditions and that checker must agree: **a score reduced
from empty through these operations MUST pass `check_invariants`.** This is the
oracle, and it is stronger than any assertion the reducer can make about itself.

### Pin 6 — the invariant-10 **prose** reconciliation (§6.3), and it is doc-only

Invariant 10's doc comment (`invariants.rs:59`–`:62`) names only cross-cutting
structures and event-internal references. Its body checks materially more: the
four reference classes above, plus measure and grid time-signature references
(`:1180`–`:1212`). **G3a repairs the doc comment to describe what the check
actually enforces.**

**No enum entry, no discriminant, no behaviour change, no `all()` count
change.** `GraphInvariant` stays at 19. It does not reach the wire — no
reference from `epiphany-ops` or `epiphany-bundle` — so this is not a schema
event. Invariant 20 is G3b's.

### Pin 7 — G-minor interaction: all four kinds carry epoch 11

`introduced_minor()` returns `Option<u16>` and has **no wildcard arm** by
design, so a new variant cannot compile without an epoch. All four take
`Some(11)` — one epoch for one additive event, per the ratified policy
(`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4); G2a's precedent put two kinds at the
single epoch 9.

**The sentinel must not be 0.** `0` is a real baseline minor for V1–V3.

Two sites, both required: the `@ Some(11)` annotations in the tag vocabulary
(`payload.rs:714` region) and the ratified-table transcription in test `s1`
(`payload.rs:2475` region). **An epoch omitted from the s1 table is an epoch
that test cannot see go wrong** — that comment is already in the file; honour it.

Append the epoch-11 row to `spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4's ladder,
naming the introducing commit once it exists.

### Pin 8 — the four-document append ritual applies in full

An operation-vocabulary append is a documented event in **four** specification
documents. G1 shipped five normative falsehoods by declaring them out of scope;
that is not repeatable.

* `operation_catalog.tex` — a `\section` per kind (four), version bump,
  changelog paragraph.
* `binary_format.tex` — payload-layout and tag rows per kind, version bump,
  Revision History row.
* `core_spec.tex` — the normative `OperationKind`/`OperationKindTag` listings
  and the spelled-out payload counts.
* `text_projection.tex` — four new kind productions are a document-surface
  change, so `COMPANION_VERSION` bumps (0.10.0 → 0.11.0), re-sweeping five live
  version sites plus a changelog row and re-flipping the negative
  `superseded_companion_version` vector.

Use `\sectionsc{...}` for cross-document references. **`\ref` cannot cross
documents** — `operation_catalog.tex` shipped an undefined reference that way.

Regenerate all four PDFs; they are tracked.

### Pin 9 — explicit non-goal: G3a authorizes **no** pruning or compaction

The standing prohibition holds and has had real teeth since G2b: pruning would
discard **authored** genesis state, not merely re-derivable state. G3a adds
four more authored families to that surface. Blocked on disposition C.

### Pin 10 — P13-S15 stays open, and these kinds stay outside the golden lock

The `[(OperationKind, u8); 30]` golden lock (`payload.rs:2011`) ends at
discriminant 29. Kinds 35–38 are **outside** it, exactly as 30–34 already are.
**Do not extend the lock in this packet.** P13-S15 lands as its own rung with
its own mutation evidence; widening it here would ship the extension without
that evidence.

---

## 3. Touch table

Derived from `git show 3b09595 --name-only` (G1, the closest mint precedent)
and `git show 13c3d2f --name-only` (G2b), minus what pins 2 and 3 exclude.
Every line number below re-verified against the working tree 2026-07-29.

### Core

| File | What |
|---|---|
| `crates/epiphany-core/src/codec.rs` | four `canonical_value!` entries (`:3518` list) |
| `crates/epiphany-core/src/invariants.rs` | pin 6: invariant-10 doc comment (`:59`–`:62`) |
| `crates/epiphany-core/DECISIONS.md` | the rung's record |

**Not touched:** `graph.rs` (all four types exist), `textvalue_graph.rs` (pin
3), `lib.rs` (already exported).

### Ops

| File | What |
|---|---|
| `crates/epiphany-ops/src/payload.rs` | four op structs + `CanonicalEncode`; `OperationKind` variants; `discriminant()` (`:390` region); `schema_major()` — **no arm**, pin 2; `introduced_minor()` (`:449` region); `tag()` (`:497` region); `encode_canonical` (`:543` region); tag vocabulary `@ Some(11)` (`:714` region); s1 epoch table (`:2475` region) |
| `crates/epiphany-ops/src/envdecode.rs` | decode arms (`:599` region) and the tag-dispatch arms (`:901` region), plus validation |
| `crates/epiphany-ops/src/reduce.rs` | four dispatch arms + four mint reducers, on `create_staff`'s shape (`:4075`, `:4148`) |
| `crates/epiphany-ops/src/textproj_kind.rs` | production arms (`:232` region) **and** parse arms (`:572` region) |
| `crates/epiphany-ops/src/migrate.rs` | both directions (`:192`, `:356` regions) |
| `crates/epiphany-ops/src/v0.rs` | `V0OperationKind` variants (`:118` region) |
| `crates/epiphany-ops/src/fuzz.rs` | generator arms (`:304` region) |
| `crates/epiphany-ops/src/valuegen.rs` | value generators |
| `crates/epiphany-ops/src/vectors.rs` | four envelope decode vectors, pinned to **literal bytes** (trap 4) |
| `crates/epiphany-ops/src/lib.rs` | re-exports (`:135` region) |
| `crates/epiphany-ops/DECISIONS.md` | the rung's record |

### Boundary crossings — budgeted up front (trap 6)

An `OperationKind` append is **not** containable to core + ops. All five
re-verified 2026-07-29; earlier revisions of the plan carried three drifted
citations.

| File | What | Why it bites |
|---|---|---|
| `crates/epiphany-editor-core/src/barriers.rs` | four arms in `subjects_of` (`:313`, pattern at `:444`) | Rust exhaustiveness; testkit depends on editor-core, so a missing arm blocks conformance **and** `requirement_labels` — the gate cannot run at all |
| `crates/epiphany-layout-ir/src/barrier.rs` | the "one past the vocabulary" tag `35` → `39` (`:1156`, assertion at `:1170`/`:1176`) and its comment | Deliberately a literal; unbumped, it pins a bug — a barrier prohibiting a new op encodes fine and cannot read back |
| `crates/epiphany-testkit/tests/text_projection_grammar.rs` | count `35` → `39` and the message string (`:315`) | Hand-maintained literal parallel to a derived list |
| `crates/epiphany-testkit/src/generators.rs` | drawn range `30..=34` → `30..=38` (`:1908`) and the never-drawn guard (`:1947`) | A kind never drawn is a kind never fuzzed |
| `crates/epiphany-textproj/src/vectors.rs` | the negative vector whose "wrong version" moves with each bump | Silently passes for the wrong reason otherwise |

**Both `barriers.rs` and `barrier.rs` are editor-track files.** The one-time
authorization to edit them is per-packet and **does not generalise**; it is
granted for this packet for these two files only, for the exhaustiveness arms
and the literal bump. Touch nothing else in either crate.

### Text projection

| File | What |
|---|---|
| `crates/epiphany-textproj/src/lib.rs` | `COMPANION_VERSION` 0.10.0 → 0.11.0 and the live version sites |
| `crates/epiphany-textproj/src/parse.rs` | kind productions |
| `crates/epiphany-textproj/src/vectors.rs` | four positive document vectors + the negative-vector flip |

**Bind vector sources by name, never by positional index.** Inserting a
document repointed positionally-bound negative vectors in G2b and broke
generation. `by_name(...)` exists for this.

**Do not conflate version domains.** A corpus fixture's
`manifest_schema_version` is the *manifest's* version, not the epoch a block
requires. G2b shipped `SchemaVersion::new(0, 10)` here with a comment making
exactly the inference `text_projection.tex:1367` forbids. Use
`SchemaVersion::V0`.

### Normative documents (pin 8)

`spec/operation_catalog.tex` + `.pdf`, `spec/binary_format.tex` + `.pdf`,
`spec/core_spec.tex` + `.pdf`, `spec/text_projection.tex` + `.pdf`.

### Vectors and tracking

`spec/vectors/decode_vectors.txt`, `spec/vectors/textproj_document_vectors.txt`
(regenerated; any hardcoded corpus **count** moves with them),
`spec/PLAN_GENESIS_OPS.md`, `spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4.

---

## 4. Tests — each with the mutation that must kill it

Each row names a mutation that MUST be **observed failing** and then reversed
**by editing back** — never `git checkout`, never `git stash`. A mutation that
does not compile produces no test output and signs nothing. A mutation absorbed
by the compiler (e.g. deleting a match arm) proves nothing about the test:
prefer a mutation that keeps the workspace compiling and isolates the behaviour
under test. **A shared mutation that leaves a test green signs nothing** — the
t9 lesson from G2b.

| # | Test | Mutation that must kill it |
|---|---|---|
| t1 | All four kinds and tags are 35–38 in **both** spaces, and the discriminant byte leads each canonical encoding | Move any one kind to 39; then, separately, move its tag. Both must fail — the spaces are asserted independently (pin 1) |
| t2 | `schema_major()` returns **0** for all four | Add them to the `=> 2` arm; must fail (pin 2's stated bug) |
| t3 | A block containing all four stamps major **0**, and the op-block accept-set is untouched at 3 | Make one kind report major 2; must fail |
| t4 | Each op round-trips through `encode` → `envdecode` → reduce, byte-identical, with its decode vector pinned to **literal bytes** | Swap two fields in one op's `encode_canonical`; must fail. *(A self-consistent reorder applied to both codec halves passes round-trip tests — trap 4, the 3b-i lesson. Literal-byte vectors are what catch it.)* |
| t5 | Re-carrying a live id with a **byte-identical** value is `AlreadyApplied`; with a **differing** value is `RecreateContentMismatch`; a tombstoned id is `TargetTombstoned` | Return `Applied` for the differing-value case; must fail |
| t6 | Referential preconditions refuse under a graph: a `CreateStaffGroup` naming a non-live `Staff`, and a `CreateView` naming a non-live `AnalysisLayer`, are both `TargetMissing` | Drop the members loop from `create_staff_group`; must fail |
| t7 | Those same preconditions are **not** enforced base-free | Remove the `if self.graph.is_some()` guard from one reducer; must fail — base-free has no universe to check against |
| t8 | **The defect closes**: from empty, `CreateInstrument` → `CreateStaffGroup` → `CreateStaff` **with `group: Some(...)`** succeeds and reaches a note | Replace the `CreateStaffGroup` dispatch arm with `OperationKind::CreateStaffGroup(_) => OperationEffect::Applied`, keeping the match exhaustive; must fail at the grouped-staff assertion while the spine stays applied |
| t9 | A score reduced from empty through all four ops **passes `check_invariants`** (pin 5) | Make `create_view` skip the `active_layers` check *and* author a dangling layer reference; invariant 10 must fire |
| t10 | All four kinds carry **epoch 11**, and a block containing them stamps minor **11** | Assign epoch 10 to one kind; must fail. Run against **both** epoch sites separately (vocabulary annotation, s1 table) — each must be independently able to fail |
| t11 | Text projection round-trips all four kinds, and the companion version is 0.11.0 | Drop one parse arm; must fail |
| t12 | Invariant 10's doc comment names the four reference classes its body checks (pin 6) | Grep-assert the repaired prose is present; revert the comment to see it fail |

**On t12's grep shape:** a self-matching needle is a real hazard — G2b hit it
twice, once when a multi-line needle matched the test's own source and once
when the assertion *message* contained the searched phrase. Keep the needle
short, keep it out of the message, and if the guard reads more than one file,
iterate `include_str!` over each.

---

## 5. Gate

* `cargo test --workspace` — full pass, zero failures, with the count reported.
* `cargo clippy --workspace --all-targets` — zero warnings.
* `cargo fmt --check` — clean.
* `git diff --check` — clean.
* All four PDFs regenerated; no undefined LaTeX references.
* Every t-row mutation **observed failing** and reversed by editing back, with
  the observed failure quoted. **Not** "would fail".
* **Stage only the files in §3's touch table, explicitly named.** Never
  `git add -A`. The editor track has parallel work in `spikes/` and elsewhere
  that MUST NOT be staged.

## 6. Report

State, with evidence: kinds/tags assigned and the two spaces asserted
separately; that `schema_major()` gained **no** arm and no bundle file was
touched; the four `canonical_value!` entries; that `textvalue_graph.rs` needed
no change; each precondition and its invariant-10 correspondence; the from-empty
grouped-staff defect closing; epoch 11 at both sites; the four-document sweep;
and the five boundary-crossing literals with their new values. Report each
mutation's **observed** output. Anything not done, say so plainly.
