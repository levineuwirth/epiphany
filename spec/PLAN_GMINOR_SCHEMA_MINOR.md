# Plan — G-minor: the chunk schema minor, and why it is not a small rung

**Filed as** P13-S14. **Ruled** 2026-07-28: its own rung, sequenced **after
G2a and before G2b** (`spec/PLAN_GENESIS_OPS.md` §4 — the sweep is scoped to
kinds 24–33, and G2b appends 34).

**Status:** scoped, not contracted. §5 lists what needs ratification first.

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
   per-kind minor could travel through. (Note `V0` is `{0, 1}` — `ids.rs:173`
   — while `V1`/`V2`/`V3` are `{n, 0}`. The baselines are already inconsistent,
   which matters for §4.)
2. Both writer-side staging paths derive only the major:
   `testkit/src/bundle_harness.rs:32` and `textproj/src/serialize.rs:189`, each
   computing `max(schema_major)` and handing it to `for_major`.
3. Therefore kinds **24–27** (Phase 3), **28–29** (major-2 repeats), **30**
   (Push 4a), **31** (G1), and **32–33** (G2a) all ship with no additive
   record. The requirement's own failure mode is exactly what the gap
   produces: a reader meeting kind 33 from a newer writer cannot distinguish
   "my vocabulary is stale" from "these bytes are damaged".

## 2. The decisive scoping finding: the minor is content-addressed

**This is what makes G-minor a real tranche rather than a one-line fix, and it
must be settled before any contract is written.**

`chunk_content_hash` (`bundle/src/chunk.rs:177`) builds the preimage as

```rust
p.push_bytes(&kind.canonical_bytes());
p.push_bytes(&schema.canonical_bytes());   // <-- major AND minor
p.push_u64_le(payload.len() as u64);
p.push_bytes(payload);
```

and `SchemaVersion::canonical_bytes` (`ids.rs:222`) is "major then minor,
little-endian" — the minor is *in* the preimage. `chunk_id` dispatches through
`content_hash_for` to the same function (`chunk.rs:202`).

**So raising a chunk's minor changes its `ChunkId`.** Every affected chunk gets
a new content address, which propagates to the manifest that names it. This is
not a semantic break — readers gate on the major only, exactly as the spec says
— but it is a **content-address-moving change**, and it lands on the one
structure `req:format:manifest-id` promises two conforming writers derive
identically.

**Two corpora move with it:**

* `spec/vectors/textproj_document_vectors.txt` — the text projection projects
  the minor as a document surface: `project_schema` (`textproj/src/project.rs:270`)
  emits `(schema <major> <minor>)`, and the committed corpus contains six
  literal `(schema 0 1)` occurrences. If op-block minors rise, those texts
  change — which makes this **another `COMPANION_VERSION` bump** on the G1/G2a
  precedent.
* `spec/vectors/decode_vectors.txt` — value-level, so it moves only if the
  packet touches value codecs. It should not, and that is a check, not an
  assumption.

**Sixty-two sites across sixteen files construct a `SchemaVersion`**
(`SchemaVersion::{V0,new,for_major}`), concentrated in `epiphany-bundle`
(`chunk.rs`, `manifest.rs`, `superblock.rs`, `opindex.rs`, `bundle.rs`,
`vectors.rs`, `fuzz.rs`), `epiphany-testkit` (`bundle_harness.rs`,
`generators.rs`, `roundtrip.rs`, `benches/bundle.rs`), and `epiphany-textproj`
(`serialize.rs`, `parse.rs`, `project.rs`, `vectors.rs`). Most are fixtures
stamping `V0` and are unaffected; the ones that matter are the two staging
paths and anything asserting a literal id.

## 3. What the rung owes

1. A **minor-assignment policy** — see §4. This is a judgement, not a
   derivation, and it is the reason this is a rung and not a patch.
2. A per-kind minor (or an equivalent watermark) reachable from a payload.
3. Block minor = **max over payloads**, mirroring how the major is derived.
4. A `for_major` replacement that accepts a minor — `SchemaVersion::new`
   already exists (`ids.rs:200`), so this is a call-site change, not a new API.
5. Both staging paths, and a check that no third path has appeared.
6. Regenerated corpora, a companion bump if §2's projection finding holds, and
   the `binary_format.tex` accounting for whichever policy §4 ratifies.

## 4. The policy question — needs ratification before contracting

The spec says a writer must raise the minor "when it emits any discriminant
appended **after the minor it otherwise declares**". That phrasing presumes a
correspondence between each minor and a vocabulary watermark, but no such
correspondence has ever been written down. Three candidate policies:

**(a) One minor per tranche, monotonic.** Phase 3 → 2, repeats → 3, Push 4a →
4, G1 → 5, G2a → 6. Matches the intuitive reading of "minor" as a format
revision counter. **Cost:** the assignment is a retroactive judgement with no
derivation behind it, so it must be written into the spec as a table and
maintained by hand forever — a seventh hand-maintained site, on a track whose
defining lesson is that those go stale.

**(b) Minor = the highest kind discriminant the chunk emits.** A block whose
largest kind is 33 stamps minor 33. **Derivable, self-describing, and it
satisfies the rationale exactly** — a reader seeing minor 33 knows precisely
which vocabulary it needs. Nothing to remember and nothing to maintain.
**Cost:** it redefines "minor" from *format revision* to *vocabulary
watermark*, it collides with `V0`'s existing minor of `1`, and it is
per-vocabulary — the layout cache and the operation index have their own
discriminant spaces, so "the minor" would mean different things per role.

**(c) Per-major append counter.** A hybrid: minor counts vocabulary appends
within a major. Inherits (a)'s bookkeeping without (b)'s redefinition.

My recommendation is **(b)**, on the strength of the track's own history: every
hand-maintained parallel list on this project has gone stale (four at Push 4a,
six found during G2a), and (b) is the only option with nothing to maintain. But
it is a genuine redefinition of a spec term and the per-role ambiguity is real,
so it is a ruling, not a default.

**A fourth option worth pricing before choosing:** amend the MUST. It was
declined at P13-S14 filing on the ground that its rationale — distinguishing
skew from corruption — is sound and unchallenged. If §2's content-address cost
is judged too high for the benefit, that trade deserves an explicit re-look
rather than a silent deferral.

## 5. Open questions

1. **The policy** (§4). Blocks everything.
2. **Does the canonical base move?** Its major is pinned to 0 by role
   (`mis_stamped_canonical_base`, `bundle.rs:866`), but its *minor* is
   unconstrained, and `the_canonical_base_is_byte_identical_across_data_model_majors`
   pins the `MaterializedState` **payload** bytes, not the chunk header. A base
   whose minor rises keeps its payload and changes its `ChunkId`. Decide
   deliberately whether the base is exempt.
3. **Per-role or global?** Op blocks, layout cache, and operation index have
   independent discriminant spaces. Policy (b) forces this question; (a) and
   (c) can dodge it.
4. **Does the manifest's own `manifest_schema_version`
   (`superblock.rs:174`) participate?** The manifest is carried opaquely and
   never grows a versioned layout, so probably not — but it is a
   `SchemaVersion` and should be ruled in or out explicitly.
5. **Migration.** Existing bundles carry minor 0/1 with appended kinds inside.
   After this rung they are, by the new rule, mis-stamped. No production corpus
   exists (local repo and test bundles only — the standing
   `epiphany-ops/DECISIONS.md` position), so the answer is probably "nothing to
   do", but it should be *stated* rather than assumed.

*Related: `spec/PASS13_CANDIDATES.md` (P13-S14), `spec/PLAN_GENESIS_OPS.md` §4
(the ladder), `spec/binary_format.tex` §"Schema Versioning".*
