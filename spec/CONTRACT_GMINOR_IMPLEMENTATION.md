# Contract: G-minor — the chunk schema minor becomes a derived record

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/PLAN_GMINOR_SCHEMA_MINOR.md` (policy §4, seam §4, epoch ladder §4,
audit fold-in §5.1) and `spec/AUDIT_GMINOR_VOCABULARIES.md` (the governing
reachability inventory). Filed as **P13-S14**. Sequenced **after G2a, before
G2b** (`spec/PLAN_GENESIS_OPS.md` §4).

This rung implements a **MUST that no writer has ever honoured**
(`binary_format.tex:2330`): a writer must raise the chunk schema minor when it
emits a discriminant appended after the minor it otherwise declares, so an
unknown-discriminant decode failure is attributable to version skew rather than
corruption.

**Parallel safety.** The editor track owns `crates/epiphany-editor-gui/**`,
`crates/epiphany-render-svg/**`, `crates/epiphany-glyphs/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, `spec/PLAN_EDITOR_APP.md`,
every `spec/CONTRACT_EDITOR_*.md`, `spec/DRAFT_T4_FIXTURE_RECIPE.md`,
`spec/ANALYSIS_*.md`, `crates/epiphany-editor-gui/goldens/*.png`, the entire
`spikes/` tree, and the current unstaged root `Cargo.toml` change. **All of it
is out of scope.** **Stage only the files this contract names — never
`git add -A`.**

**One-time authorization**, not generalising beyond this packet: this contract
may edit `crates/epiphany-editor-core/src/barriers.rs` **only if** the
compiler requires it. It should not — this rung appends no `OperationKind`
variant — and if an edit there turns out to be needed, that is a finding to
report, because it would mean the change is wider than scoped.

---

## 0. What this rung is not

* **Not an operation-vocabulary append.** No new `OperationKind`,
  `OperationKindTag`, or `OperationPayload` variant. The four-document append
  ritual (`binary_format.tex` + `operation_catalog.tex` + `core_spec.tex` +
  `text_projection.tex` payload/tag tables) does **not** apply.
* **Not a schema-major change.** No field is added to any encoded struct. See
  pin 6 for the one place that constraint bites hardest.
* **Not a migration.** Existing bundles are accepted exactly as they are.

---

## 1. Design pins

### Pin 1 — the epoch ladder is normative input, not a derivation

`PLAN_GMINOR_SCHEMA_MINOR.md` §4's table is **ratified**. Transcribe it; do not
re-derive it, do not renumber it, and do not "improve" it.

| Minor | Additive event | Variants introduced |
|---|---|---|
| 2 | M2c | `PreconditionFailureReason::ContainerNotEmpty` = 10 |
| 3 | Push 3 | `OperationPayload::ResolveEquivocation` = 3 |
| 4 | Phase-3 first tranche | `OperationKind`/`OperationKindTag` 24–27; `PreconditionFailureReason::TempoMapMalformed` = 11 |
| 5 | Pass-12 G-pass | `ReanchorReason::SameCanvasNearer` = 6; `PreconditionFailureReason` 12–13 |
| 6 | Schema-major-2 repeat revision | `OperationKind`/`OperationKindTag` 28–29 |
| 7 | Push 4a | `OperationKind`/`OperationKindTag` 30; `PreconditionFailureReason` 14–15 |
| 8 | Genesis G1 | `OperationKind`/`OperationKindTag` 31 |
| 9 | Genesis G2a | `OperationKind`/`OperationKindTag` 32–33 |

**Everything else is baseline and contributes no epoch.** The audit's fourteen
clean escape-carriers and seven clean closed unions are baseline; recording an
epoch for any of them is a defect.

### Pin 2 — `introduced_minor` is co-located, exhaustive, and wildcard-free

For each of the **five** vocabularies with post-baseline variants
(`OperationKind`, `OperationKindTag`, `OperationPayload`, `ReanchorReason`,
`PreconditionFailureReason`), add an `introduced_minor()` that returns the
epoch, with the **baseline arm returning the sentinel for "no additive
requirement"** (see pin 3 for what that sentinel is).

**It must be exhaustive with no wildcard arm**, so a future variant cannot
compile without being assigned an epoch. This is the control the whole policy
rests on — `PLAN_GMINOR_SCHEMA_MINOR.md` §4 adopts it precisely because this
track has now watched hand-maintained parallel lists go stale twice (four sites
at Push 4a, six found during G2a).

**`OperationKindTag`'s must live inside `operation_kind_tag_vocabulary!`**
(`ops/src/payload.rs:487`), not beside it. The macro is the compile-enforced
source of truth for that space; a sibling match would reintroduce exactly the
parallel list this pin exists to prevent.

**`OperationKind::introduced_minor()` must NOT be bolted onto
`discriminant()`'s match** (`payload.rs:275`). That hand-written match is *the
site Push 4a got wrong*. Adding a second responsibility to it is acceptable only
if the arms stay one-per-variant and exhaustive; a `_ =>` anywhere in the new
code is a contract violation.

### Pin 3 — the derivation

* An **envelope's** required minor = the **max** over every discriminant it
  **actually emits**: its outer `OperationPayload` variant, its primitive
  `OperationKind` (when `Primitive`), and every nested additive variant reached
  by its encoder.
* A **block's** = the max over its envelopes.
* The **major** is unchanged: still the independent max of `schema_major()`.
  Major and minor are derived separately and never influence each other.
* **The emitted version is `{major, max(baseline_minor(major), epoch_max)}`.**
  Baselines are **not** normalised: `V0` keeps minor 1, `V1`–`V3` keep minor 0.
  A major-0 block emitting only baseline vocabulary still stamps `{0, 1}`.

**Represent "no additive requirement" as a distinct value, not as `0`.** `0` is
a real baseline minor for `V1`–`V3`, and conflating them makes the `max` above
read correctly by accident rather than by construction. `Option<u16>` with
`None` for baseline is the shape to prefer.

### Pin 4 — `for_major` is replaced, not extended

`SchemaVersion::for_major` (`bundle/src/ids.rs:204`) accepts only a major and
maps it to a fixed constant; there is no parameter a minor could travel through.
Add the minor-aware constructor and route both staging paths through it.

**Keep `for_major` itself** as the baseline lookup — pin 3 needs
`baseline_minor(major)` and that is exactly what `for_major` already computes.
Do not delete it; give it a doc comment saying it yields the *baseline*, and
that a writer emitting post-baseline vocabulary must use the new constructor.

### Pin 5 — content-minimal stamping for every other role

Per the audit's dispositions:

* **Canonical base (`MaterializedState`)** — rises only when its **own bytes**
  emit a later-added discriminant (`ReanchorReason` 6 or
  `PreconditionFailureReason` 10–15). **It never emits `OperationKind`,
  `OperationKindTag`, or `OperationPayload`**, so newer source operations alone
  never move it.
* **Acceleration snapshot (full `Score`)** — audited clean; baseline.
* **`OperationIndex`** — emits no additive discriminant; baseline.
* **`Blob`, `ExtensionData`** — producer-owned opaque bytes. Core derives
  nothing; the schema is carried from the producer.
* **`TextProjection`, `LayoutCache`, `IntegrityIndex`** — **no producer exists
  in this codebase.** Write no derivation for them. If one is later wired, it
  inherits this policy then.
* **`CompressionAlgorithm` is excluded by ruling** (audit §1.8): `ChunkRef`
  transport metadata, deliberately outside the chunk-identity preimage.

### Pin 6 — the manifest seam

**`epiphany-bundle` stays opaque. Add no `epiphany-ops` or
`epiphany-layout-ir` dependency to it.** Its entire dependency list is
`epiphany-determinism` + `zstd` and it must stay that way.

1. **Producers supply the aggregate manifest `SchemaVersion` explicitly.** The
   write paths take it as an input.
2. **`CommitContext` gains a previous-manifest-version field.** This is new
   plumbing, not a read-through: `CommitContext::previous_manifest`
   (`bundle.rs:156`) is a `&Manifest`, and **`Manifest` carries no
   schema-version field at all** — it lives in the superblock
   (`superblock.rs:174`, bytes 64..68).
3. **It must NOT become a `Manifest` body field.** A field addition is schema-
   **major** regardless of type (`binary_format.tex:2360`), which would defeat
   this entire rung. The wire slot already exists in the superblock.
4. **A manifest naming a barrier tag in 24–33 takes that tag's epoch**;
   otherwise it retains its baseline. **Changed child `ChunkRef`s never raise
   the manifest minor.** The manifest **major stays 0**.
5. **Changed barrier bytes require an aware producer**, which must compute the
   exact new epoch or **refuse**. Blindly retaining the previous aggregate is
   specifically wrong: **removing the sole barrier that contributed the maximum
   leaves the manifest over-stamped.**
6. **Ordinary repacks preserving the complete barrier content preserve the
   carried version exactly.** This is the common path; keep it cheap.

**Where the epoch is computed.** `epiphany-layout-ir` is the only crate that
can both decode `EditBarrier` and reach `OperationKindTag`'s epoch table (it
depends on `epiphany-ops`; `barrier.rs:392` is the encoder). The aggregate
helper belongs there. `epiphany-bundle` never calls it.

### Pin 7 — `Manifest::SCHEMA` becomes a baseline constant

`manifest.rs:591` (`pub const SCHEMA: SchemaVersion = SchemaVersion::V0`) stops
being the universally emitted version and becomes the documented **baseline**.

**Three** sites silently select it for hashing and must stop:

| Site | What it is |
|---|---|
| `bundle.rs:220` | the create path |
| `bundle.rs:724` | the commit path |
| `bundle.rs:1298` | `manifest_chunk_hash`, a **re-exported public helper** |

The third is **public API**, so this is a signature change with callers outside
the crate, not an internal edit. Find and update every caller.

**`bundle.rs:301` stays exactly as it is.** It compares
`superblock.manifest_schema_version.major != Manifest::SCHEMA.major` — major
only — which is precisely the v0 rule that the minor is a *record*, not a gate.
**Tightening it to compare the full version would be a conformance
regression**, and it is the most tempting wrong edit in this packet.

### Pin 8 — the text-projection consequence

`TextDocument` gains the carried manifest `SchemaVersion`. **The projection
carries it; it never derives it** — `epiphany-textproj` depends on
`epiphany-core`/`bundle`/`ops` and **not on `epiphany-layout-ir`**, so it
structurally cannot decode barrier bytes, which is the point.

* **`COMPANION_VERSION` 0.9.0 → 0.10.0** (`textproj/src/lib.rs:34`).
* **Regenerate `spec/vectors/textproj_document_vectors.txt`.** Holding the
  version while changing the surface would leave two incompatible grammars both
  claiming `(0 9 0)` — the same reasoning as G2a's kind-production bump.
* **Op-block stamping stays projection-invisible** (block schemas are discarded
  at `project.rs:424`). The bump is caused by the manifest attribute **only**.
  State this in the changelog entry so a later reader does not infer that
  op-block minors reach the text surface.

### Pin 9 — `decode_vectors.txt` must be checked, not assumed

The value-level decode corpus should **not** move: chunk schema versions are a
chunk-header concern and the decode vectors are value-level. **Verify it and
report the check**; a silent "it didn't change" is indistinguishable from
"I didn't look."

### Pin 10 — the `binary_format.tex:2373` repair is owed by this packet

The bullet list at `:2373-2385` says `OperationKind` and `OperationKindTag`
"append at ${\geq}30$" and its parenthetical history names only 24–27 and
28/29. **Kinds 30–33 are absent from both the number and the narrative, and 30
is no longer free.** The normative tables (`:1443-1457`, `:1526-1527`) are
current and authoritative.

Repair the paragraph: correct the next-free-slot numbers and complete the
history. **This paragraph defines the minor-additive mechanism this rung
implements**, so it must also state the derivation the rung ships.

---

## 2. Open pin requiring ratification BEFORE implementation

**How does a text round-trip detect changed barrier bytes?**

Pin 6.5 requires an aware producer to recompute or refuse. But
`epiphany-textproj`'s serialize path receives a whole `TextDocument` and has no
prior state to diff against, and cannot decode barriers even if it did. So a
hand-edited text document whose barrier bytes changed while its carried
`SchemaVersion` did not is **undetectable at that layer**.

Three dispositions:

* **(a) Preserving producer, documented.** `textproj` carries the version
  verbatim and is explicitly *not* an aware producer. A hand-edited barrier with
  a stale carried version is a producer error, exactly as hand-editing any other
  opaque preserved payload is. **Recommended** — it is the only option
  consistent with keeping the layer free of `layout-ir`.
* **(b) Validate in conformance.** As (a), plus a `testkit` check (testkit
  *does* depend on `layout-ir`) that recomputes the aggregate from decodable
  barriers and fails on mismatch. Costs a conformance row; catches the in-tree
  case only.
* **(c) Make `textproj` aware.** Add `layout-ir` to `textproj`. **Rejected on
  its face** — it inverts pin 8 and makes the projection depend on the barrier
  vocabulary it exists to carry opaquely.

**Do not implement until this is ruled.** (a) and (b) differ by a test, not a
design, so this is cheap to settle — but it determines whether the packet ships
a conformance row.

---

## 3. Touch table

| # | File | Change |
|---|---|---|
| 1 | `ops/src/payload.rs` | `introduced_minor()` for `OperationKind` (:116) and `OperationPayload` (:59); tag epochs **inside** `operation_kind_tag_vocabulary!` (:487) |
| 2 | `ops/src/effect.rs` | `introduced_minor()` for `ReanchorReason` (:317) and `PreconditionFailureReason` (:124) |
| 3 | `ops/src/payload.rs` | `OperationEnvelope`'s required-minor derivation (max over emitted) |
| 4 | `ops/src/reduce.rs` | `MaterializedState`'s required-minor derivation (pin 5) |
| 5 | `bundle/src/ids.rs` | minor-aware constructor; `for_major` documented as baseline (:204) |
| 6 | `bundle/src/manifest.rs` | `SCHEMA` documented as baseline (:591) |
| 7 | `bundle/src/bundle.rs` | `CommitContext` field (:154); three hash sites (:220, :724, :1298); **:301 unchanged** |
| 8 | `layout-ir/src/barrier.rs` | aggregate-epoch helper over `prohibited_operation_kinds` |
| 9 | `testkit/src/bundle_harness.rs` | `stage_operation_block` derives the minor (:25) |
| 10 | `textproj/src/serialize.rs` | `stage_operation_envelope_block` derives the minor (:183); manifest version supplied on commit |
| 11 | `textproj/src/project.rs` | project the manifest `SchemaVersion` into `TextDocument` |
| 12 | `textproj/src/parse.rs` | parse it back |
| 13 | `textproj/src/lib.rs` | `COMPANION_VERSION` → `(0, 10, 0)` (:34) |
| 14 | `spec/vectors/textproj_document_vectors.txt` | regenerate |
| 15 | `spec/binary_format.tex` | pin 10 repair; version + Revision History row |
| 16 | `spec/text_projection.tex` | companion 0.10.0 (:237, :521, :1330) + changelog stating the manifest cause |

**Anything outside this table is a finding to report, not a fix to apply.**

---

## 4. Tests — each with the mutation that must kill it

Mutation discipline (`testing-discipline-mutation-first`): anchor-assert →
introduce the bug → **observe the actual failure** → restore by reversing the
edit, never `git checkout`. **A test whose mutation you did not run is not
signed off.**

| # | Test | Required mutation |
|---|---|---|
| s1 | Every epoch in pin 1's table is returned by `introduced_minor` for its variant | Change one variant's epoch by one; the table assertion must fail |
| s2 | Every baseline variant returns "no additive requirement" | Give `InsertEvent` an epoch; must fail |
| s3 | A block of only baseline envelopes stamps `{0, 1}` — **not** `{0, 0}` | Normalise the baseline to 0; must fail |
| s4 | A block containing kind 31 stamps minor 8 | Return the *count* of additive variants instead of the max; must fail |
| s5 | A block mixing an old kind 23 with `ResolveEquivocation` stamps **3**, not 1 | Derive the minor from the highest *discriminant* rather than the epoch — the rejected policy — must fail |
| s6 | Major and minor derive independently: a major-2 block with kind 28 stamps `{2, 6}` | Make the minor depend on the major; must fail |
| s7 | A canonical base whose effects include `SameCanvasNearer` stamps 5; one without stamps baseline **even when its source operations are kind 33** | Drag the base's minor from the op kinds; must fail |
| s8 | A manifest naming barrier tag 31 stamps 8; naming only baseline tags stamps baseline | Ignore `prohibited_operation_kinds`; must fail |
| s9 | Removing the sole max-contributing barrier **lowers** the manifest minor | Retain the previous aggregate unconditionally; must fail (this is pin 6.5's over-stamp) |
| s10 | A repack with unchanged barrier content preserves the carried version byte-for-byte | Recompute from scratch and drop an undecodable barrier's contribution; must fail |
| s11 | `bundle.rs:301` still accepts a bundle whose manifest minor differs but major matches | Tighten it to full-version equality; must fail — **this is the conformance regression pin 7 warns about** |
| s12 | A raised minor changes the `ChunkId` and therefore the `ManifestId` | Drop the schema from the hash preimage; must fail |
| s13 | `TextDocument` round-trips the carried manifest `SchemaVersion` | Emit the baseline on serialize instead of the carried value; must fail |
| s14 | The committed corpus parses at `(0 10 0)` and a `(0 9 0)` document is rejected | Leave `COMPANION_VERSION` at 0.9.0; must fail |

---

## 5. Gate

* `cargo fmt --check` clean; `cargo clippy --workspace --all-targets` **0
  warnings**; `cargo test --workspace` green with the count reported.
* Conformance suites reported with counts (they were 8/8 + 9/9 at G2a).
* Decode vectors: report the count **and** pin 9's verification that the
  value-level corpus did not move.
* Text-projection vectors regenerated; report the count.
* All four PDFs build at **0 undefined references**.
* `git status --short` **before and after**, in full. The only differences may
  be the files in §3's touch table. The tree is already dirty with the editor
  track's `M Cargo.toml` and untracked `spikes/` — **that dirt must appear
  unchanged in both captures.**

## 6. Report

The gate outputs; every mutation with the failure it actually produced; any
touch outside §3; and **any place where this contract's own assumptions turned
out to be wrong**. That last is not politeness: the plan behind this contract
has already been revised twice for scoping errors, and its §3 vocabulary table
was found incomplete by the audit that followed it.
