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

G3a completes the four root-level vectors. **It closes the satisfiability half
only** — §1.1 (ratified) rules that a bidirectionally consistent staff group
remains unauthorable in this packet, by design and on the record.

---

## 1.1 The pin, RESOLVED — `StaffGroup`/`Staff` authorship authority

**Ratified 2026-07-29: disposition B.** Drafted in the contract, decided by the
user — the shape G2b's `accidental_extensions` pin took.

### The cycle

`CreateStaff` requires its `group` to be a live `StaffGroup`
(`reduce.rs:4117`). `CreateStaffGroup` (pin 4) requires each of its `members`
to be a live `Staff`. Neither can name the other first, so **with mints only,
no authoring order produces a bidirectionally consistent group**:

* `CreateStaffGroup(g, members: [])` → `CreateStaff(s, group: Some(g))` leaves
  `s.group == Some(g)` while `g.members == []`.
* `CreateStaff(s, group: None)` → `CreateStaffGroup(g, members: [s])` leaves
  the mirror image, and re-carrying `CreateStaff` with `group: Some(g)` is
  `RecreateContentMismatch`, not an amendment.

There is no `ModifyStaffGroup` and no `ModifyStaff`, and deletes are deferred
(§6.1). So the inconsistency is not merely unchecked — **it is unrepairable
within this packet.**

### Why nothing in the tree decides it

Invariant 10 checks resolution in **both** directions independently — a staff's
group is declared (`invariants.rs:1126`), a group's members are declared
(`:1135`) — and **agreement in neither**. No agreement check exists anywhere in
the crate. The specification declares `StaffGroup.members`
(`core_spec.tex:4231`) and `Staff.group` (`:5578`, "Visual grouping: which
staff group (if any) this staff belongs to") without stating which is
authoritative or requiring them to agree. **This is genuinely unspecified, not
merely unimplemented.**

### Disposition A — `Staff.group` authoritative, `members` reduction-maintained

`CreateStaffGroup` mints identity, name, and kind, and MUST carry
`members: []`; a non-empty `members` is refused or normalized away at
construction (the "subset over normalization" choice G2b faced). Thereafter
`StaffGroup.members` is maintained **by reduction**: `create_staff` with
`group: Some(g)` appends to `g.members`. Fully authorable from empty; joining
is commutative and set-union, which fits the CRDT discipline.

Costs, stated honestly: it makes `members` a *derived* field of a stored type,
so byte-identical re-carry of `CreateStaffGroup` MUST compare against the
**carried** value (empty), never the current derived state — an explicit
sub-pin, and exactly the class of confusion `canonical_value!` cannot catch. It
also expands G3a beyond a pure mint packet, which is the property that made
this rung cheap.

### Disposition B — declare the authority, defer the enforcement — **RATIFIED**

Ratified 2026-07-29 with this precise meaning, which is **normative**:

* **`Staff.group` is the sole authority for membership.** Every consumer reads
  membership from `Staff.group`. Nothing may read `StaffGroup.members` to
  decide whether a staff is in a group.
* **`StaffGroup.members` is a non-authoritative denormalized projection.**
* **G3a stores it but neither maintains nor trusts it.** `CreateStaffGroup`
  carries `members` as given and validates resolution only (pin 4).
* **G3a permits both stale forms**, and they are equally permitted:
  * a **missing** member — `s.group == Some(g)` while `g.members` omits `s`;
  * a **spurious** member — `g.members` contains `s` while `s.group` is `None`
    or names a *different* group.

  An earlier draft named only the first. Naming only the missing-member form
  would have understated the ruling and left the spurious form looking like a
  bug rather than a permitted state.

**This is a normative semantic ruling, not a deferral of one.** The earlier
draft's argument for B — that it "adds no semantics" while A does — was wrong
and is withdrawn. B *assigns authority to a field the specification left
unranked*, which is a semantic change; what it defers is **enforcement**, not
meaning. The honest statement of B's advantage over A is narrower: it adds
semantics without adding *machinery*, leaving the mint a mint.

**Filed as P13-S16** (`spec/PASS13_CANDIDATES.md`) — the concrete gap, both
stale forms, the disposition-A fix, the candidate invariant 21, and the
instruction that consumers read `Staff.group` only. The earlier draft claimed
a "filed gap" while no such entry existed; it does now.

**Disposition A remains the later maintenance/enforcement fix**, sequenced
after G3b so an invariant append is not competing with invariant 20.

### What t8 must therefore claim

**t8's claim is narrowed** — see §4. The unqualified "the defect closes"
framing was wrong: what closes is that `CreateStaff.group`'s precondition
becomes *satisfiable*, not that a consistent group becomes *authorable*.

**t8 must pin both asymmetric authoring orders**, not only the first:

* `CreateStaffGroup(g, members: [])` → `CreateStaff(s, group: Some(g))`
  ⟹ `s.group == Some(g)`, `g.members == []` — the **missing** form;
* `CreateStaff(s, group: None)` → `CreateStaffGroup(g, members: [s])`
  ⟹ `g.members == [s]`, `s.group == None` — the **spurious** form.

Both must be asserted to *hold*, as permitted states under the ruling. A test
that pins only one leaves the other free to change silently.

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

### Pin 3 — the four types need `canonical_value!` entries, and no other *code* change in core

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

### Pin 4a — byte-identical re-carry needs a **carried-value map**, at seven sites each

The first draft of pin 4 promised re-carry idempotence without the machinery
that makes it work, which would have shipped the promise and not the property.
Comparing "the same value" requires *retaining* the value; the object map holds
only `Live`/`Tombstoned`.

Exactly three such maps exist today — `staff_values`,
`time_signature_values`, `instrument_values` (`reduce.rs:999`–`:1007`) — and
each is threaded through **seven** sites. G3a adds four more
(`staff_group_values`, `part_definition_values`, `analysis_layer_values`,
`view_values`), so this is **28 touch points**, none of which appeared in the
first draft's touch table:

| # | Site | Existing lines |
|---|---|---|
| 1 | reducer state declaration | `reduce.rs:999`–`:1007` |
| 2 | `WorkingSnapshot` declaration | `:1106`–`:1108` |
| 3 | initialization | `:1388`–`:1390` |
| 4 | **base seeding** in `seed_from_graph` | `:1437`, `:1445`, `:1460` |
| 5 | mint insertion in the reducer | `:4134`, `:4183`, `:4219` |
| 6 | snapshot | `:7637`–`:7639` |
| 7 | restore | `:7677`–`:7679` |

**Site 4 is the one that fails silently.** Without a base seed, a re-carry
against an entity that came from the *base* score — not from the log — finds no
retained value and misclassifies. G1 already documents this exact hazard and
its mutation for `instrument_values` (`reduce.rs:13264`–`:13268`); copy that
shape. **Each of the four omitted seeds must be killed separately** (t13), and
a base-recarry test must exist (t5b) — a re-carry test that only ever reduces
from empty cannot see a missing seed at all.

**The packet is self-contained**: `CreateView`'s precondition target is minted
by `CreateAnalysisLayer` in this same packet, so the ordering is testable
end-to-end without a base score.

### Pin 4b — §1.1's ruling MUST land on the normative surfaces, not only here

**A normative ruling that lives only in a contract and a candidate ledger is
not normative.** §1.1 assigns authority; until the specification and the field
declarations say so, the normative record stays ambiguous and every reader
outside this packet is entitled to the opposite reading. Pin 3's "nothing else
in core" is therefore **narrowed to the codec surface only** — §1.1 adds a
documentation obligation to `graph.rs`, and pin 8's four-document ritual gains
two specific normative additions.

Required, all four:

| Site | What it must say |
|---|---|
| `core/src/graph.rs:819` — `Staff.group` | **Currently has no doc comment at all.** Write one: this field is the **sole authority** for group membership. |
| `core/src/graph.rs:1614` — `StaffGroup.members` | **Currently has no doc comment at all.** Write one: a **non-authoritative denormalized projection**; MUST NOT be read to decide membership; may be stale in **both** directions. |
| `core_spec.tex` — the `Staff`/`StaffGroup` declarations (`:5578`, `:4231`) | The same authority rule, normatively. This is the document that declared both fields without ranking them, so this is where the ambiguity actually lives. |
| `operation_catalog.tex` — the **new** `CreateStaffGroup` section **and** the **existing** `CreateStaff` section (`:1104`) | Explicit stale-form semantics: both the missing and the spurious form are **permitted outcomes**, named as such, with the authoring orders that produce them. `CreateStaff` needs it too — it is the operation that *creates* the missing form, and its section currently promises nothing about the projection. |

Cross-reference **P13-S16** from each, so a reader who meets the disagreement
finds the ruling rather than filing it again.

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

**Two of the four carry pin 4b's normative additions on top of the routine
sweep**: `core_spec.tex` gains the authority rule at the `Staff`/`StaffGroup`
declarations, and `operation_catalog.tex` gains explicit stale-form semantics
in **both** the new `CreateStaffGroup` section and the existing `CreateStaff`
section (`:1104`). Neither is optional, and neither is satisfied by the routine
append alone.

* `operation_catalog.tex` — a `\section` per kind (four), version bump,
  changelog paragraph.
* `binary_format.tex` — payload-layout and tag rows per kind, version bump,
  Revision History row.
* `core_spec.tex` — the normative `OperationKind`/`OperationKindTag` listings
  and the spelled-out payload counts.
* `text_projection.tex` — four new kind productions are a document-surface
  change, so `COMPANION_VERSION` bumps **0.11.0 → 0.12.0**, re-sweeping five
  live version sites plus a changelog row and re-flipping the negative
  `superseded_companion_version` vector so it rejects **0.11.0**.
  **The tree is already at 0.11.0** — G2b bumped it (`textproj/src/lib.rs:47`
  and its doc comment). An earlier draft of this contract repeated G2b's
  0.10.0 → 0.11.0, which would have re-declared the current version as new and
  left the negative vector rejecting a version no longer superseded.

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
| `crates/epiphany-core/src/graph.rs` | **pin 4b**: write the authority doc comment on `Staff.group` (`:819`) and the projection doc comment on `StaffGroup.members` (`:1614`) — **neither field is documented today** |
| `crates/epiphany-core/DECISIONS.md` | the rung's record |

**Not touched:** `textvalue_graph.rs` (pin 3), `lib.rs` (already exported). No
*type* changes anywhere in core — `graph.rs` gains documentation only, per pin
4b; the four carried types already exist unchanged.

### Ops

| File | What |
|---|---|
| `crates/epiphany-ops/src/payload.rs` | four op structs + `CanonicalEncode`; `OperationKind` variants; `discriminant()` (`:390` region); `schema_major()` — **no arm**, pin 2; `introduced_minor()` (`:449` region); `tag()` (`:497` region); `encode_canonical` (`:543` region); tag vocabulary `@ Some(11)` (`:714` region); s1 epoch table (`:2475` region) |
| `crates/epiphany-ops/src/envdecode.rs` | decode arms (`:599` region) and the tag-dispatch arms (`:901` region), plus validation |
| `crates/epiphany-ops/src/reduce.rs` | four dispatch arms + four mint reducers, on `create_staff`'s shape (`:4075`, `:4148`); **plus four carried-value maps at seven sites each — 28 touch points, see pin 4a** |
| `crates/epiphany-ops/src/textproj_kind.rs` | production arms (`:232` region) **and** parse arms (`:572` region) |
| `crates/epiphany-ops/src/migrate.rs` | both directions (`:192`, `:356` regions) |
| `crates/epiphany-ops/src/v0.rs` | `V0OperationKind` variants (`:118` region) |
| `crates/epiphany-ops/src/fuzz.rs` | generator arms (`:304` region) |
| `crates/epiphany-ops/src/valuegen.rs` | value generators |
| `crates/epiphany-ops/src/vectors.rs` | four envelope decode vectors, pinned to **literal bytes** (trap 4) |
| `crates/epiphany-ops/src/lib.rs` | re-exports (`:135` region) |
| `crates/epiphany-ops/DECISIONS.md` | the rung's record |

### Boundary crossings — budgeted up front (trap 6)

An `OperationKind` append is **not** containable to core + ops. **Six
crossings: one exhaustive-match site plus five literal/prose sentinels.** All
six re-verified 2026-07-29; earlier revisions of the plan carried three drifted
citations, and earlier revisions of this contract alternated between five and
six by counting the exhaustive-match site inconsistently.

The two classes fail differently, which is why they are counted together but
named apart. The **exhaustive-match** site fails loudly — the workspace will
not compile. The five **sentinels** fail silently: each stays green while
meaning something narrower than it says.

| File | What | Why it bites |
|---|---|---|
| `crates/epiphany-editor-core/src/barriers.rs` | four arms in `subjects_of` (`:313`, pattern at `:444`) | Rust exhaustiveness; testkit depends on editor-core, so a missing arm blocks conformance **and** `requirement_labels` — the gate cannot run at all |
| `crates/epiphany-layout-ir/src/barrier.rs` | the "one past the vocabulary" tag `35` → `39` (`:1156`, assertion at `:1170`/`:1176`) and its comment | Deliberately a literal; unbumped, it pins a bug — a barrier prohibiting a new op encodes fine and cannot read back |
| `crates/epiphany-testkit/tests/text_projection_grammar.rs` | count `35` → `39` and the message string (`:315`) | Hand-maintained literal parallel to a derived list |
| `crates/epiphany-testkit/src/generators.rs` | drawn range `30..=34` → `30..=38` (`:1908`) and the never-drawn guard (`:1947`) | A kind never drawn is a kind never fuzzed |
| `crates/epiphany-testkit/src/layout_stub.rs` | the `30..=34` range in the s10/row-29 comment (`:1373`) | Prose, but it states the coverage claim the test rests on; stale text here is how a narrowed guard reads as a broad one |
| `crates/epiphany-textproj/src/vectors.rs` | the negative vector whose "wrong version" moves with each bump | Silently passes for the wrong reason otherwise |

**Both `barriers.rs` and `barrier.rs` are editor-track files.** Authorization
is per-packet and **does not generalise**. Granted 2026-07-29, narrowly:

* `barriers.rs` — **the four required exhaustive `subjects_of` arms, and
  nothing else.**
* `barrier.rs` — **the invalid-tag literal, its comment, and its assertions,
  35 → 39, and nothing else.**

**No other change in either crate is authorized by this packet.** If the
workspace appears to need one, stop and report rather than widening.

### Text projection

| File | What |
|---|---|
| `crates/epiphany-textproj/src/lib.rs` | `COMPANION_VERSION` **0.11.0 → 0.12.0** (`:47`) and the live version sites |
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
| t4 | Each op round-trips through `encode` → `envdecode` → reduce, byte-identical, with its decode vector pinned to **literal bytes** | **Four independent mutations, one per carried type** — swap two fields in each `struct_codec!` declaration: `PartDefinition { id, name, staves }` → `{ id, staves, name }` (`core/src/codec.rs:1790`), and likewise `AnalysisLayer` (`:1791`), `ViewDefinition` (`:1792`), `StaffGroup` (`:2329`). Each type's literal-byte vector must fail on its own. **These are four separate layouts; one mutation signs one of them.** Collapse to a single mutation only if the implementation consolidates all four behind one shared mechanism, and say so explicitly if it does. *(A create op's `encode_canonical` is a single `push_lp_bytes` line over one carried value, so there is nothing in the op to reorder — the first draft's mutation was impossible. The reorder must go where the layout actually lives, and because `struct_codec!` moves both halves at once it is self-consistent and passes round-trip tests: trap 4, the 3b-i lesson. Literal-byte vectors are the only thing that catches it.)* |
| t5 | Re-carrying a live id with a **byte-identical** value is `AlreadyApplied`; with a **differing** value is `RecreateContentMismatch`; a tombstoned id is `TargetTombstoned` | Return `Applied` for the differing-value case; must fail |
| t5b | **Re-carry against a base-derived entity**: reduce `new_onto` a score whose four vectors are already populated, re-carry each byte-identically, and get `AlreadyApplied` | Covered by t13 per-seed. A re-carry test that only reduces from empty **cannot see a missing base seed at all** — that is why this row is separate from t5 |
| t6 | All three referential loops refuse under a graph, **each asserted separately**: `CreateStaffGroup.members`, `CreatePartDefinition.staves`, and `CreateView.active_layers` naming a non-live target are each `TargetMissing` | **Three independent mutations**, one per loop: drop the members loop from `create_staff_group`; drop the staves loop from `create_part_definition`; drop the active-layers loop from `create_view`. Each must fail on its own row. *(The first draft omitted `PartDefinition.staves` entirely — one uncovered loop is one loop that can be deleted green.)* |
| t7 | Those same preconditions are **not** enforced base-free | Remove the `if self.graph.is_some()` guard from one reducer; must fail — base-free has no universe to check against |
| t8 | From empty, `CreateInstrument` → `CreateStaffGroup` → `CreateStaff` **with `group: Some(...)`** succeeds and reaches a note — i.e. `CreateStaff`'s group precondition becomes **satisfiable**. **Claim narrowed per §1.1: this asserts satisfiability, NOT that a bidirectionally consistent group is authorable** | Replace the `CreateStaffGroup` dispatch arm with `OperationKind::CreateStaffGroup(_) => OperationEffect::Applied`, keeping the match exhaustive; must fail at the grouped-staff assertion while the spine stays applied |
| t8b | **Both permitted stale forms are pinned** (§1.1): the missing form (`CreateStaffGroup(g, [])` → `CreateStaff(s, Some(g))` ⟹ `s.group == Some(g)`, `g.members == []`) and the spurious form (`CreateStaff(s, None)` → `CreateStaffGroup(g, [s])` ⟹ `g.members == [s]`, `s.group == None`). Both asserted to **hold**, as states the ruling permits | **Two mutations, each in the reducer that actually runs second in its order.** Missing form → mutate **`create_staff`**: when `group` is `Some(g)`, append the newly minted staff to `g.members` (disposition A's maintenance rule); the missing-form assertion must fail. Spurious form → mutate **`create_staff_group`**: reject or normalize away a non-empty carried `members`; the spurious-form assertion must fail. *(An earlier draft assigned these the other way round, which is impossible in both directions: `create_staff_group` runs first in the missing order and cannot append a staff that does not exist yet, and `create_staff` runs first in the spurious order and has no later group to repair. A mutation in the operation that runs first cannot reach the state the assertion names.)* **A test pinning only one order leaves the other free to change silently** |
| t9 | A score reduced from empty through all four ops **passes `check_invariants`**, and each skipped reducer check **independently** makes invariant 10 fire (pin 5) | **The fixture is constant across all mutations**: the baseline input already attempts a dangling reference in each of the three loops, and passes because the reducer refuses them. Then mutate **only production**, one skipped check at a time — three separate mutations, each of which must let its dangling reference through and make invariant 10 fire. *(The first draft moved the fixture and the production code together, which proves only that a hand-built bad score fails a checker — not that the reducer's refusal is what was holding the line.)* |
| t10 | All four kinds carry **epoch 11**, and a block containing them stamps minor **11** | Assign epoch 10 to one kind; must fail. Run against **both** epoch sites separately (vocabulary annotation, s1 table) — each must be independently able to fail |
| t11 | Text projection round-trips all four kinds, and the companion version is **0.12.0**, with the negative vector rejecting **0.11.0** | Drop one parse arm; must fail. Separately, leave `COMPANION_VERSION` at 0.11.0; the negative vector must fail |
| t12 | Invariant 10's doc comment names the four reference classes its body checks (pin 6) | Grep-assert the repaired prose is present **within the invariant-10 doc block only** — slice the source from the `/// 10.` line to the `CrossCuttingRefsResolve,` line and search *that*. Revert the comment to see it fail. **Searching the whole file passes on the implementation body**, which contains the same identifiers the doc comment is supposed to gain — a guard that cannot fail |
| t13 | **Each of the four base seeds is load-bearing** (pin 4a, site 4) | **Four separate mutations**: skip seeding each of `staff_group_values`, `part_definition_values`, `analysis_layer_values`, `view_values` in `seed_from_graph`, one at a time. Each must make t5b fail. Model on G1's documented precedent for `instrument_values` (`reduce.rs:13264`–`:13268`) |
| t14 | **The authority rule reached the field declarations** (pin 4b): `Staff.group`'s doc comment states it is authoritative, and `StaffGroup.members`' states it is a non-authoritative projection that may be stale in both directions | Grep-assert each, **slicing to that field's doc block only** — same discipline as t12, since `graph.rs` mentions both identifiers throughout and a file-wide search cannot fail. Delete either doc comment to see the corresponding assertion fail. *(The `.tex` halves of pin 4b are not machine-checkable here; they are gate items, reported explicitly.)* |

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
no change; **the four carried-value maps at all seven sites each, named site by
site**; each of the three referential loops and its invariant-10
correspondence; **that §1.1's ruling reached all four pin-4b sites — both field
doc comments, `core_spec.tex`, and both `operation_catalog.tex` sections —
quoting the text written at each**, and what t8 therefore does and does not
claim; epoch 11 at both sites; the companion bump
0.11.0 → 0.12.0 with the negative vector rejecting 0.11.0; the four-document
sweep; and the six boundary crossings — the one exhaustive-match site and the
five sentinels, each with its new value. Report
each mutation's **observed** output — t6, t9, and t13 are multi-mutation rows
and each sub-mutation must be reported separately. Anything not done, say so
plainly.
