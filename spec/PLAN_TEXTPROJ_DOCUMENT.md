# Text Projection — the document layer: scope and plan

Status: **scoping complete, two rulings needed before dispatch.**
Prepared against `master` @ `fdb4a57`. Every claim was checked against the code;
where I ran a probe I say so.

This is the last phase. Chapter-5 values (`cf81074`) and Chapter-6 operations
(`fdb4a57`) already project and parse strictly. What remains is the document
around them: the header, identity, profile, extension, canonical-base and blob
lines — and then the two round-trip equations over a whole bundle.

---

## 1. Decisions needed

### Ruling A — no blob can be canonical today. What should the projector do?

`req:textproj:canonical-blobs` and `core_spec` §"Canonical and Non-Canonical
Manifest Roots" agree exactly: a blob is canonical **iff** it is *referenced by a
canonical operation or by canonical reduced state*. A blob referenced only by
acceleration structures is non-canonical and `MUSTNOT` be projected. Canonicality
is by **reachability**, not by membership in `blob_roots`.

I grepped `epiphany-core` and `epiphany-ops` for `BlobId`: **no hit.** No
operation payload and no Chapter-5 value carries a blob reference. There is no
mechanism by which a blob can be reached from canonical state, so **today the
correct projection of every real bundle emits zero `(blob ...)` lines**, and the
production is future-proofing.

The dangerous wrong answer is projecting `manifest.blob_roots` wholesale — it
looks obviously right, is one line of code, and violates the requirement by
emitting non-canonical blobs.

**Recommendation.** Implement the reachability predicate as a real function that
today provably returns the empty set, and *prove* it rather than asserting it: a
test that fails the moment any core or ops type gains a `BlobId` field, so the
hook is implemented when it is first needed rather than silently skipped. Same
discipline as computing a bound instead of spelling it. The `(blob ...)`
projection and parse must still be written and tested against a synthetic
document, because the parser must accept texts that a future writer emits.

### Ruling B — the header version is the companion version, and that couples every byte

The companion says the header names *"the version of **this companion** the text
conforms to."* So a projection written today starts `(text-projection (0 6 0))`.

Two consequences worth accepting deliberately:

* The companion's own worked example currently reads `(text-projection (0 3 0))`
  — **stale**. My conformance lock covers the envelope line of that example, not
  the header above it.
* Every companion version bump changes the first line of every projection, so
  `req:textproj:canonical-text` is *version-relative* and the text vector corpus
  must be regenerated on each bump.

**Recommendation.** Keep the coupling — a text that does not say which rules it
follows cannot be strictly parsed against them — fix the stale example, and make
the corpus regeneration a documented, one-command step so the cost is mechanical
rather than a surprise. If instead you want the header to track a slower-moving
*format* version independent of editorial revisions, that is a spec change and
should be decided now, not after a corpus exists.

---

## 2. What I verified

**A new crate is unavoidable, and the orphan rule does not bite.**
`epiphany-bundle` depends only on `epiphany-determinism` — **not** on
`epiphany-core`. So `TextValue` (a core trait) cannot be implemented for
`Manifest`, `BlobRef`, `ProfileDeclaration` and friends from any third crate.

That would have been a real problem, except: **no document-line production
contains a `value` position.** Every one bottoms out in leaves — `bytes`,
`integer`, `bool`, `string`, `option` — plus the closed vocabularies
`profile-id`, `chunk-kind`, `schema`. So the document layer needs **free
functions**, not trait impls, exactly as the grammar-directed operation layer
did. No orphan problem, no new dependency edge, and `epiphany-bundle` stays
independent of the music model as designed.

`epiphany-textproj` depends on core (for `Sexp`, `render`, `read_sexp`), ops (for
`project_envelope`/`parse_envelope` and `canonical_reduction_order`), and bundle.

**Parse cannot produce a `Manifest`.** The text carries **payloads inline** — the
canonical base's root chunk payload, each extension's preserved chunk payloads,
each blob's payload — whereas a `Manifest` holds `ChunkRef`s with offsets and
lengths that the text deliberately erases. The parse output needs its own type
(§4), and the bundle writer synthesizes the physical layer.

**The write path already does the synthesis.** `Bundle::create` + `commit`
assigns offsets, content-addresses and de-duplicates chunks, and derives every
`ChunkId`/`ContentHash`. That is precisely what `req:textproj:derive-or-carry`
assumes, so serialization stages payloads and lets `commit` do the rest.

**Envelope ordering is available.** `epiphany_ops::canonical_reduction_order` is
public and exported; `Bundle::read_operation_block` yields raw envelope bytes for
`decode_envelope`.

**Accelerators are dropped, by design.** `operation_index_root`,
`acceleration_snapshots`, `text_projection_root`, `integrity_root` and
`operation_block_summaries` are non-canonical and are not projected. A bundle
that goes text → binary comes back without them. That is correct and should be
stated in a doc comment, because it looks like data loss and is not.

---

## 3. The round trip, stated precisely

`req:textproj:roundtrip` gives two equations. **Both hold; neither says bytes
survive.**

```
  semantics(parse(project(B))) = semantics(B)     -- weaker
  project(serialize(parse(T))) = T                -- byte-checkable, the corpus test
```

Going bundle → text → bundle is **deliberately lossy in the physical layer** and
in three specific ways an implementer will otherwise read as bugs:

1. **Duplicate blobs collapse.** `req:textproj:derived-ordering` de-duplicates
   blob lines and extension chunk roots by *projected form*. Two byte-identical
   blobs stored twice are one blob; that is the erasure working.
2. **Layout is regenerated.** Offsets, compression, block splitting, chunk count.
3. **Accelerators vanish**, as above.

All three preserve `semantics(...)`, and none of them affects the second
equation, which quantifies over *texts*, not bundles. Write the corpus against
the second equation.

---

## 4. Work breakdown

New crate `crates/epiphany-textproj`. Three waves; file boundaries are
one-per-agent.

### Wave 1 — scaffold and types (blocks everything; small)

`Cargo.toml`, `lib.rs`, and the parse-output type. Roughly:

```rust
pub struct TextDocument {
    pub document_id: DocumentId,
    pub lineage_id: Option<LineageId>,
    pub profiles: Vec<ProfileDeclaration>,
    pub extensions: Vec<TextExtension>,      // chunk payloads INLINE
    pub canonical_base: Option<TextCanonicalBase>,  // root payload INLINE
    pub blobs: Vec<TextBlob>,                // payloads INLINE
    pub envelopes: Vec<OperationEnvelope>,   // canonical operation order
}
```

The `Text*` types exist because the corresponding bundle types carry `ChunkRef`s
the text erases. Do not try to reuse `SnapshotRef`/`ExtensionDeclaration`
verbatim.

### Wave 2 — three parallel agents

* **`project.rs`** — `Bundle<S>` → `TextDocument` → text. Reads chunk and blob
  payloads through the bundle, decodes envelopes, sorts by
  `canonical_reduction_order`, and applies `derived-ordering`'s sort-and-dedup by
  projected form to blob lines and extension chunk roots.
* **`parse.rs`** — text → `TextDocument`, strictly. Line-oriented: each line is
  one complete s-expression (`req:textproj:envelope-per-line`), and the line
  *order* is itself constrained by the `projection` production — header, document,
  lineage?, profile*, extension*, canonical-base?, blob*, envelope*. A
  out-of-order or repeated section is a rejection, not a reordering.
* **`serialize.rs`** — `TextDocument` → `Bundle<S>`. Stages the base root chunk,
  extension chunks, blobs and one operation-envelope block; builds the manifest;
  commits. Block splitting is a free physical choice — one block is fine and
  simplest.

### Wave 3 — conformance

Text vectors in the shape of `spec/vectors/decode_vectors.txt`, drift-locked and
gated as conformance step **`[7e]`**, plus the two round-trip equations over the
generated-score corpus. This is also where the stale `(0 3 0)` header in the
companion's example gets fixed and locked.

---

## 5. Traps

* **`(blob ...)` is unreachable from real data.** Its tests need a synthetic
  document. See Ruling A.
* **Section order is normative.** The `projection` production is a sequence, not
  a set. A parser that accepts lines in any order and sorts them is normalizing.
* **`derived-ordering` sorts by *projected form*, not by any binary key** — order
  and de-duplicate on the rendered UTF-8 bytes of each line, which is the whole
  point (a chunk's offset must not decide the text's order).
* **The extension line is easy to get subtly wrong.** Six fields in ratified
  declaration order — id, version, required, chunks, affected-kinds, barriers —
  and `affected_object_kinds`/`edit_barriers` are opaque **byte strings**, never
  structured. The `Vec<u8>`-is-not-a-sequence trap that bit the ops layer lives
  here too.
* **`ProfileId::Custom` carries a 16-byte registry id** and is the one profile
  that is not a bare symbol.
* **Do not project `manifest.blob_roots`.** See Ruling A; it is the plausible
  wrong answer.
* **Two `WallClockDuration` types exist** — one in `epiphany-core`, one in
  `epiphany-bundle`. The retention policy uses the bundle's.

---

## 6. Verification contract

The standing gate, plus:

* **Mutation-verify every check**, anchor asserted before substitution, and report
  per-check with output.
* **Exercise every outbound normalization** — the contract rule added after five
  such normalizations shipped untested in the ops layer. `derived-ordering`'s
  sort-and-dedup is exactly this shape: build a document with duplicate blobs and
  out-of-order chunk roots *in memory* and prove the projection collapses and
  orders them.
* **Assert the corpus's own reach.** A round-trip suite that never exercises an
  extension, a canonical base, or a multi-envelope document proves less than its
  green tick suggests. State the counts.
