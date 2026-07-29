# Plan — G-minor: the chunk schema minor

**Filed as** P13-S14. **Ruled** 2026-07-28: its own rung, sequenced **after
G2a and before G2b** (`spec/PLAN_GENESIS_OPS.md` §4).

**Status:** policy ratified 2026-07-28 (§4); vocabulary audit landed (§5.1,
`spec/AUDIT_GMINOR_VOCABULARIES.md`, c63258d); **epoch ladder ratified
2026-07-28** (§4, minors 2–9). **Every gate in §5 is now discharged — the
implementation contract may be drafted.** Two things it must carry that are
not yet designed: the manifest derivation seam across bundle-opaque barrier
bytes (§4), and the `binary_format.tex:2373` correction (§5.1).

> **Revision 2026-07-28.** The first draft of this plan got three things wrong
> and recommended a policy that cannot work. Corrections are marked inline
> rather than deleted, because two of them are the kind of mistake that
> regenerates: an under-scoped vocabulary, and a corpus claim asserted from a
> `grep -c` over *lines* rather than occurrences.

---

## 1. The requirement, and the evidence it has never been met

`binary_format.tex:2330` (§"Schema Versioning", the Minor bullet):

> **Minor** = additive. v0 readers verify the major only; the minor is a
> *record*, not a gate — but it is a mandatory record: a writer **MUST** raise
> the chunk schema minor when it emits any discriminant appended after the
> minor it otherwise declares, so that a decode failure on an unknown appended
> discriminant is attributable to a version skew rather than corruption.

Three facts, each read from the tree:

1. `SchemaVersion::for_major` (`bundle/src/ids.rs:204`) maps a major to a
   **fixed constant** and **accepts only a major**. There is no parameter a
   minor could travel through. (`V0` is `{0, 1}`; `V1`/`V2`/`V3` are `{n, 0}`.
   The baselines are already inconsistent — §4 preserves that rather than
   normalising it.)
2. Both writer-side staging paths derive only the major
   (`testkit/src/bundle_harness.rs:32`, `textproj/src/serialize.rs:189`).
3. So every post-baseline append ships with no additive record. The
   requirement's own failure mode is what the gap produces: a reader meeting an
   unknown discriminant cannot distinguish a stale vocabulary from damaged
   bytes.

## 2. The minor is content-addressed

`chunk_content_hash` (`bundle/src/chunk.rs:177`) pushes
`schema.canonical_bytes()` into the preimage, and that is "major then minor,
little-endian" (`ids.rs:222`). `chunk_id` dispatches to the same function
(`chunk.rs:202`).

**So raising a chunk's minor changes its `ChunkId`**, which changes the
manifest body naming it, and therefore the `ManifestId`. This is real **address
churn** — a migration and deduplication cost — and it is the main reason the
rung is not a patch.

> **Correction 1 (was P2).** The first draft framed this as tension with
> `req:format:manifest-id`. It is not. That requirement
> (`core_spec.tex:11213`) promises that *"two conforming writers committing
> **the same manifest body** at the same generation of the same document derive
> identical `ManifestId`s"*. Once a `ChunkRef` changes, the bodies are not the
> same, so the promise does not apply. Two conforming writers both following
> one normative minor derivation still agree — and the *historical* writer was
> already violating the minor MUST. Churn, not a broken guarantee.

> **Correction 2 (was P1).** The first draft claimed op-block minor changes
> reach the text projection and therefore force another `COMPANION_VERSION`
> bump. **They do not.** Operation blocks are decoded into envelopes and the
> block's physical schema is discarded during projection
> (`textproj/src/project.rs:424`); schemas are carried only for preserved
> extension chunks and the canonical base (`:454`). The committed corpus holds
> **seven** decoded `(schema 0 1)` forms in its *accepted* vectors — not the
> six the draft claimed, an error from counting matching lines instead of
> occurrences — and every one belongs to an extension chunk or a canonical
> base. **No companion bump is required for op-block stamping.** A separate
> decision to move canonical-base or extension schemas would change this;
> the op-block sweep alone does not.
>
> **Still true, but no longer the whole answer (2026-07-28).** The manifest
> seam ruling (§4) makes the manifest's aggregate `SchemaVersion` a *carried*
> `TextDocument` attribute, so **G-minor does bump the companion** — 0.9.0 →
> 0.10.0, with corpus regeneration. Op-block stamping remains
> projection-invisible exactly as this correction says; the bump comes from the
> manifest, not from it.

**Sixty-six** occurrences of `SchemaVersion::{V0, new, for_major}` across
sixteen files (corrected from 62), concentrated in `epiphany-bundle`,
`epiphany-testkit`, and `epiphany-textproj`. Most are fixtures stamping `V0`
and are unaffected; the load-bearing ones are the two staging paths.

## 3. The vocabulary inventory — the part the first draft missed

> **Correction 3 (was P1), and the reason the recommended policy died.** The
> draft reduced the problem to a per-`OperationKind` minor. An envelope also
> emits the **independent outer `OperationPayload` discriminant**, and
> `ResolveEquivocation` — appended at 3 — contains no `OperationKind` at all
> (`ops/src/payload.rs:60`). So the ambiguity lives **inside one role and one
> block**, not merely between roles, and no per-kind method can discharge the
> MUST.

`binary_format.tex:2369` states that the *only* minor-additive mechanism in
schema major 0 is appending discriminants to open vocabularies, and enumerates
them:

| Vocabulary | Appends at | Note |
|---|---|---|
| `OperationKind` | ≥ 30 | 24–27, 28–29 also took this mechanism |
| `OperationKindTag` | ≥ 30 | independent space from `OperationKind` |
| `OperationPayload` | ≥ 4 | **outer**; `ResolveEquivocation` carries no kind |
| value-layer unions closed under `req:binfmt:frozen-layout` | — | appendable only via a ratified revision of that document; also minor-additive |

Two boundaries that keep the audit finite:

* **`ChunkKind` is closed.** It has no `Registered` variant and its
  discriminant enters every chunk's hash preimage, so a new chunk kind is a
  format-**major** event, not a minor one.
* **`Registered` escape variants are not schema changes at all** — the wire
  form is already defined. `binary_format.tex` lists fifteen carriers
  (`RepairKind`, `ReanchorReason`, `PreconditionFailureReason`,
  `IntegrityAnomalyKind`, `TypedObjectId`, …). Extension through an escape is
  out of scope; **appending a native variant to one of those same enums is
  not**, and that distinction is the audit's sharpest edge.

**The rung must inventory every append-only discriminant reachable from each
affected chunk payload**, not just `OperationKind`. For the canonical base that
means the vocabularies `MaterializedState` actually emits — `OperationEffect`,
`NoOpReason`, `PreconditionFailureReason`, `ConflictKind`,
`IntegrityAnomalyKind`, `PendingReason`, `ObjectState`, `TypedObjectId` —
which notably do **not** include `OperationKind`.

## 4. Policy — RATIFIED 2026-07-28

**A global additive epoch with content-minimal stamping.** The first draft
recommended "minor = highest discriminant emitted"; that is rejected. It cannot
represent multiple independent vocabularies in one block, and generalising it
to "highest from any vocabulary" is worse than useless — an old `OperationKind`
23 would numerically mask a newly appended `OperationPayload` 3.

The ratified rule:

1. Each additive format revision receives **one globally meaningful minor
   epoch**.
2. Every appended variant is annotated with the epoch that introduced it.
3. An **envelope's** required minor is the maximum across its outer payload
   variant, its primitive kind, and every nested additive variant **actually
   emitted**.
4. A **block** takes the maximum required minor over its envelopes. The major
   remains the independent maximum of `schema_major()`.
5. Old content keeps its major's baseline — `V0`'s existing `1`, the others'
   `0`. Baselines are not normalised.

**Per-major counters are also rejected:** a mixed block can carry a new major-0
kind beside a major-2 payload, so independently numbered minor namespaces do
not compose after the block takes `max_major`.

**Maintenance risk, and how it is controlled.** The obvious objection to an
epoch table is the one this track keeps proving — hand-maintained parallel
lists go stale (four at Push 4a, six found during G2a). The control is to
**co-locate `introduced_minor` with each discriminant in an exhaustive macro or
match with no wildcard arm**, so a newly appended variant *cannot compile*
without being assigned an epoch. That assignment is an unavoidable schema
decision, not a fallible parallel list — the same reasoning that made
`operation_kind_tag_vocabulary!` safe.

### The epoch mapping — RATIFIED 2026-07-28

**These are schema-minor epochs, not companion semver numbers.** The two
numbering spaces are unrelated and must not be cross-read.

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
| 10 | Genesis G2b | `OperationKind`/`OperationKindTag` 34 (`SetTuningContext`) |

> **Epoch 10 ratified 2026-07-28**, with G2b as the event. This is the **first
> exercise of the ladder's own growth path**: G-minor's `introduced_minor()` is
> exhaustive with no wildcard arm, so kind/tag 34 *cannot compile* without an
> epoch — the control working exactly as designed. The ladder stays monotonic
> (G2b follows G2a) and prefix-closed. Epoch assignment remains a ratified
> schema decision, never an implementer's choice.

**The ladder is complete against the audit** — every post-baseline variant in
`AUDIT_GMINOR_VOCABULARIES.md` appears exactly once: all ten kind/tag pairs,
`OperationPayload` 3, `ReanchorReason` 6, and all six
`PreconditionFailureReason` appends.

**And it is monotonic in real time**, which is what makes a minor prefix-closed
(a reader declaring minor *n* supports every epoch ≤ *n*). Verified against the
introducing commits rather than assumed: M2c `a207077` (2026-06-25) → Push 3
`92aaccf` (07-02) → Phase-3 `0316160` (07-02) → G-pass `e4edea6` (07-07) →
repeat pair `9b5339f` (07-07) → Push 4a `2740a6c` (07-09) → G1 `3b09595`
(07-24) → **G2a `7df5ca1`** (07-28). The two events sharing 2026-07-07 are
ordered correctly: the G-pass precedes the repeat revision.

> **Correction 2026-07-28.** G2a's introducing commit is **`7df5ca1`**, where
> kinds/tags 32–33 enter `ops/src/payload.rs`. `55eff00` is the *subsequent
> review-fix* commit and introduces no discriminant. The distinction matters
> because these hashes are the evidence for monotonicity: citing a follow-up
> commit would make the ladder unverifiable at the one rung a future reader is
> most likely to re-derive.

**Existing major baselines are unchanged**: `V0` uses minor 1; `V1`–`V3` use
minor 0. **Baseline variants impose no additive override** — M2c's *operation
kinds* stay baseline (inside the golden-locked 0..=23) while M2c's
`PreconditionFailureReason` append requires epoch 2. Same tranche, two
vocabularies, two answers.

**Baseline boundaries are per vocabulary, even though the epoch space is
global.** The epoch answers *which revision introduced this variant*; the
boundary answers *was this variant present at that vocabulary's own lock*.
Only the second varies. M2c is the proof: it is baseline for `OperationKind`
(absorbed into the golden-locked 0..=23) and a genuine append for
`PreconditionFailureReason` (`ContainerNotEmpty` = 10, whose 0..=9 lock
predates it). So `introduced_minor` is assigned **per variant, per
vocabulary**.

### Consequent calls, all ruled 2026-07-28

* **Keep the MUST.** Its cost is lower than §2 first claimed.
* **No text-companion bump** for op-block stamping.
* **The canonical base does not move merely because its source operations are
  newer.** It never emits the op-kind discriminant. It retains its current
  minor while its payload uses only baseline vocabulary, and rises only when
  the `MaterializedState` bytes themselves emit a later-added discriminant —
  a newly introduced effect or reason variant, say. *(This replaces the first
  draft's "is the base exempt?", which was the wrong binary question: the base
  is neither exempt nor automatically dragged, it is content-minimally
  stamped like everything else.)*
* ~~**`Manifest::SCHEMA` stays unchanged.**~~ **NARROWLY SUPERSEDED
  2026-07-28 by the audit.** The original call was right about its own case and
  wrong as an unconditional rule:

  * **Still true** — a changed child `ChunkRef` is changed manifest *data*, not
    a new manifest-layout discriminant. Its body, id, and chunk hash move
    naturally, and **that alone never raises the manifest minor.**
  * **Newly true** — **emitted barrier tags do.** The manifest reaches
    `OperationKindTag` through `ExtensionDeclaration::edit_barriers` →
    `EditBarrier::prohibited_operation_kinds` (audit §5.1, Part 2). **A
    manifest containing an edit barrier naming a tag in 24–33 takes that tag's
    epoch**; a manifest naming only baseline tags retains its baseline. This is
    content-minimal stamping applied to the manifest like any other role — the
    original call had simply not seen that the manifest emits an additive
    vocabulary at all.
  * **The manifest major remains 0.**

  **The implementation contract must design the derivation seam across the
  bundle-opaque barrier bytes**, and this is the hard part, not a detail.
  `epiphany-bundle`'s entire dependency list is `epiphany-determinism` and
  `zstd` — **not `epiphany-ops`, not `epiphany-layout-ir`, not
  `epiphany-core`**. It holds `edit_barriers` as `Vec<u8>` by design
  (`manifest.rs:283`: the barrier family "is owned by Agents C and E"), so the
  layer that stamps the manifest is structurally incapable of reading the tags
  that determine its minor. Three shapes are available — a new dependency edge,
  a producer-supplied minor travelling beside the blob, or a
  decode-and-inspect step at a higher layer that already depends on both —
  and choosing among them is contract work.

  **The genuinely hard case:** barrier bytes are *preserved verbatim* across
  writes, including for extensions the writer does not understand. A repack can
  carry a barrier blob it cannot decode, whose tags it cannot enumerate, and
  whose epoch it cannot derive. **"Decode and inspect" silently does not cover
  this** — it looks complete until a foreign extension is present.

### The seam — RULED 2026-07-28

**`epiphany-bundle` stays opaque. No `epiphany-ops` or `epiphany-layout-ir`
dependency is added.** The producer supplies the aggregate manifest
`SchemaVersion` explicitly; the bundle records it and never derives it.

1. **Producers supply the aggregate.** The write paths take the manifest's
   `SchemaVersion` as an input instead of selecting a constant.
2. **`CommitContext` must expose the previous manifest version**, so unchanged
   barrier content can preserve it. **This is new plumbing, not a read-through:**
   `CommitContext::previous_manifest` is a `&Manifest`, and **`Manifest` has no
   schema-version field at all** — the version lives in the *superblock*
   (`superblock.rs:174`, bytes 64..68). It must therefore travel as its own
   `CommitContext` field.
3. **It must NOT become a `Manifest` body field.** Adding one is a schema
   **major** change regardless of type (`binary_format.tex:2360`), which would
   defeat the entire rung. The wire slot already exists in the superblock; no
   new one is needed.
4. **Changed barrier bytes require an aware producer.** One that cannot
   establish the exact new epoch **must refuse**. Blindly retaining the previous
   aggregate is specifically wrong: **removing the sole barrier that contributed
   the maximum would leave the manifest over-stamped**, claiming a vocabulary
   level its content no longer needs.
5. **Ordinary repacks preserving the complete barrier content preserve the
   carried version exactly.** This is the common path and it stays cheap.

**Consequently `Manifest::SCHEMA` becomes a baseline constant, not the
universally emitted version** (`manifest.rs:591`, currently
`SchemaVersion::V0` = `{0, 1}`). **Three** sites silently select it for
hashing and must stop: `bundle.rs:220` (create), `bundle.rs:724` (commit), and
the re-exported harness helper `manifest_chunk_hash` (`bundle.rs:1298`) —
which is **public API**, so it is a signature change, not an internal edit. The
major check at `bundle.rs:301` stays as-is and stays correct: it compares
`.major` only, which is exactly the v0 rule that the minor is a record, not a
gate.

### The text-projection consequence — G-minor DOES need a companion bump

**This reverses Correction 2 in §2 for the manifest, though not for op
blocks.** The reasoning there still holds where it was aimed: op-block minors
are discarded during projection, so **op-block stamping remains
projection-invisible**. But the seam above makes the manifest's aggregate
`SchemaVersion` a **carried document attribute**, and the projection cannot
derive it — that is the whole point of keeping the bundle opaque, and the bytes
may come from a future or unknown extension.

So `TextDocument` gains the carried manifest `SchemaVersion`
(it currently has no such field), which is a document-surface change:

* **`COMPANION_VERSION` 0.9.0 → 0.10.0** (`textproj/src/lib.rs:34`).
* **Corpus regeneration**, on the same reasoning as G2a's kind-production bump:
  holding the version while changing the surface would leave two incompatible
  grammars both claiming `(0 9 0)`.

**Not** because of op-block stamping — because the manifest schema becomes a
carried attribute. Recording the distinction because §2's "no companion bump"
is otherwise still true and will read as contradictory.
* **No migration.** Existing bundles are accepted as-is; newly emitted or
  repacked blocks are stamped correctly.
* **Scope is "all additive discriminants reachable in affected chunk
  payloads"**, not "kinds 24–33".

## 5. What must complete before a contract

1. ~~**The vocabulary audit** (§3).~~ **DONE** — see §5.1.
2. ~~Ratify the epoch mapping.~~ **DONE** — §4, minors 2–9, complete against
   the audit and monotonic in real time.
3. Confirm no third staging path has appeared beside the two in §1. *(Contract
   work — cheap, but a check rather than an assumption.)*
4. Decide whether `decode_vectors.txt` moves — it is value-level, so it should
   not, but that is a check rather than an assumption. *(Contract work.)*

Items 3 and 4 are checks the contract performs, not gates on drafting it.

### 5.1 The audit landed — and it is the governing inventory

**`spec/AUDIT_GMINOR_VOCABULARIES.md` (c63258d) supersedes §3's table as the
authoritative reachability inventory.** §3 is retained as the reasoning that
led here; where the two differ, the audit governs. It walks each of the nine
`ChunkKind` roles' payload encoders and classifies every variant of every
vocabulary actually emitted.

What it changes about §3:

* **The canonical base reaches more than §3 lists** — add `RepairKind`,
  `ReanchorReason`, and `SpellingNominal`. Not cosmetic: `ReanchorReason` is
  one of only two vocabularies in the tree with a real native append.
* **The manifest reaches `OperationKindTag` with no operation envelope in
  it**, through `ExtensionDeclaration::edit_barriers` →
  `EditBarrier::prohibited_operation_kinds` (`layout-ir/src/barrier.rs:392`).
  §3's table is framed entirely around the operation layer and names neither
  the role nor the path. A manifest declaring a barrier on `CreateInstrument`
  (tag 31) needs the same stamping discipline as a block containing one, and
  the manifest chunk's minor is a separate decision from any op block's.
* **`CompressionAlgorithm` is excluded** (audit §1.8): `ChunkRef` transport
  metadata, not a discriminant emitted by the payload `SchemaVersion` governs,
  and deliberately outside the chunk-identity preimage. Recorded so it is not
  reopened.
* **`binary_format.tex:2373` is stale narrative** — "append at ≥30" with a
  history stopping at 29, while 30–33 are taken. The normative tables
  (`:1443`, `:1526`) are current and authoritative. **The `.tex` correction is
  owed by the G-minor implementation**, which must edit that paragraph
  regardless, since it defines the mechanism the rung implements.

*Related: `spec/AUDIT_GMINOR_VOCABULARIES.md`,
`spec/PASS13_CANDIDATES.md` (P13-S14), `spec/PLAN_GENESIS_OPS.md` §4,
`spec/binary_format.tex` §"Schema Versioning" / §"What ``Additive'' Means
Here".*
