# Plan — G-minor: the chunk schema minor

**Filed as** P13-S14. **Ruled** 2026-07-28: its own rung, sequenced **after
G2a and before G2b** (`spec/PLAN_GENESIS_OPS.md` §4).

**Status:** scoped; policy ratified 2026-07-28 (§4). Not contracted — §5 lists
the audit that must complete first.

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

**Tentative epoch mapping** — Phase 3 → 2, repeats → 3, Push 4a → 4, G1 → 5,
G2a → 6. Reasonable, but **not ratified**: it must wait until every
post-baseline additive vocabulary is audited (§3), not just `OperationKind`.

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
* **`Manifest::SCHEMA` stays unchanged.** A changed child `ChunkRef` is changed
  manifest *data*, not a new manifest-layout discriminant; its body, id, and
  chunk hash move naturally without raising the manifest's own schema.
* **No migration.** Existing bundles are accepted as-is; newly emitted or
  repacked blocks are stamped correctly.
* **Scope is "all additive discriminants reachable in affected chunk
  payloads"**, not "kinds 24–33".

## 5. What must complete before a contract

1. **The vocabulary audit** (§3). This is now the gating work, and it is the
   only thing standing between here and a contract. Its output is the epoch
   annotation for every post-baseline additive variant in every vocabulary
   reachable from an affected payload.
2. Ratify the epoch mapping once that audit lands.
3. Confirm no third staging path has appeared beside the two in §1.
4. Decide whether `decode_vectors.txt` moves — it is value-level, so it should
   not, but that is a check rather than an assumption.

*Related: `spec/PASS13_CANDIDATES.md` (P13-S14), `spec/PLAN_GENESIS_OPS.md` §4,
`spec/binary_format.tex` §"Schema Versioning" / §"What ``Additive'' Means
Here".*
