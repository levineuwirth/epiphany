# epiphany-bundle — decisions and Pass 11 candidates

This file records (a) the implementation decisions the QUICKSTART asked each
agent to make once and document, and (b) the ambiguities discovered while
building `epiphany-bundle`, batched as **Pass 11 candidates** for the spec rather
than improvised in code (QUICKSTART, Process notes: *"Ambiguities go into a
batch, not into code … Don't open Pass 11 until you have at least three such
items batched."*).

> **RATIFIED (Pass 11, 2026-06-21).** The bundle-layer Pass 11 candidates have
> been ratified into `core_spec.tex` — see `spec/PASS11_RATIFICATION_LOG.md`.
> Highlights: D4 adopted (ChunkKind/ProfileId/CompressionAlgorithm discriminants);
> D5 adopted (`ManifestId` preimage, with `manifest_id` excluded); D1 fixed
> (equal-generation superblock rule → `DivergentSameGeneration`); D3 fixed (blob
> hashing is bare `MUSCBLOB‖payload`, spec contradiction removed); D6 fixed
> (`ProfileConstraints` defined with the required `RetentionPolicy`,
> first-declared multi-profile precedence). D2 (Binary Format companion) stays
> Track B, with the convention baseline ratified by core item 1.8.

## Implementation decisions (QUICKSTART "Decisions you'll need to make")

1. **Replica ID entropy source** — N/A to this crate (Agent B/`epiphany-core`).

2. **Event-arena storage** — N/A to this crate (Agent B).

3. **Chunk store backend for v0 — a positioned single-file `BlockStore`,
   append-at-EOF.** The bundle *is* the file format, so chunks are addressed by
   file offset within one file (the spec's `ChunkRef::offset`), not a side
   `BTreeMap<ChunkId, Bytes>`. The `BlockStore` trait abstracts positioned
   reads/writes plus an explicit durable `flush`; three implementations back it:
   - `MemStore` — an in-memory byte image (`flush` is a no-op); the default v0
     backing and the recovered-image reader.
   - `FileStore` — a real file whose `flush` is `fsync` (the production
     durability path; unix-only, via positioned `pread`/`pwrite`).
   - `FaultStore` — the crash simulator behind the acceptance gate.
   The QUICKSTART suggested an in-memory `BTreeMap` for v0 and deferring the
   mmap'd file backend "until Agent D's crash fuzzer is green." The crash fuzzer
   *is* this crate's gate, and it drives `FaultStore`; memory-mapping is left as
   the deferred optimization (the format's chunk immutability makes it safe
   later, per Chapter 8 §"Memory Mapping"). The body is allocated append-at-EOF,
   which trivially satisfies "MUSTNOT overwrite any currently-reachable chunk."

4. **Async or sync — sync only.** No async traits anywhere; `BlockStore` is sync
   (decision 4). A thin async wrapper crate can come later, as the QUICKSTART
   suggests, without touching this type system.

5. **MSRV — workspace 1.77.** No exotic features. `std::io::Error::other`
   (stable since 1.74) is used; `overflow-checks` stay on in release so offset and
   length arithmetic faults loudly rather than wrapping.

### Additional local decisions

- **A prototype canonical byte layout precedes the Binary Format companion.**
  Chapter 8 §"Binary Format Companion" defers the byte-level encoding to a
  separate spec that does not yet exist (an explicit `openquestion`). To make the
  atomic-commit, crash-recovery, and re-serialization guarantees testable now,
  this crate defines a concrete, fixed-convention encoding: little-endian
  integers (matching the spec's preimage convention), `u32`-length-prefixed
  variable fields, `0`/`1` option-presence bytes, and the fixed prelude offsets
  documented in the `header`/`superblock` module tables. This is the bundle's
  analogue of `epiphany-core`'s P11-4 and is provisional — see P11-D2.

- **CRC-32C is hand-rolled (Castagnoli, table-driven `const fn`).** Chapter 8
  specifies CRC-32C for the header and each superblock; a ~30-line table-driven
  implementation avoids a second hashing dependency (the workspace keeps `blake3`
  as the sole content-hash dependency) and pins the `"123456789" → 0xE3069283`
  check vector.

- **Blobs hash bare; chunks/manifests hash structured.** Following Agent A's
  `ContentHash::of_blob`, a blob id is `BLAKE3("MUSCBLOB" || payload)`. Non-blob
  chunks use the structured Chapter 8 preimage
  `domain || kind || schema || uncompressed_length || payload`, with the manifest
  under `MUSCMANI` and all other kinds under `MUSCCHNK`. See P11-D3.

- **`commit_timestamp` is written as `0`.** It is advisory (selection is by
  generation, never timestamp), and a fixed value keeps commits byte-reproducible
  for the fuzzer. A real editor would stamp wall-clock time here.

- **Crash model.** `FaultStore` separates *live* (page-cache) bytes from *durable*
  bytes; only a successful `flush` promotes live → durable, and a crash on a
  `flush` may tear the most-recent (single in-flight) write to a prefix. This is
  faithful because the protocol's flush points isolate dependencies: the
  superblock-slot write is the *only* pending write at the commit-point flush, so
  a torn superblock is the only torn write that can affect selection, and the CRC
  catches it. Earlier-step torn writes only ever produce unreachable garbage.

- **Indeterminate commit-point flush poisons the bundle.** If the *final* flush
  (the commit point) returns an error, the new superblock may or may not have
  reached durable storage, so the on-disk active generation is unknown. `commit`
  marks the in-memory bundle read-only and returns the error; the caller must
  reopen from storage to resync. (Earlier flush errors are safe — the active slot
  is untouched, so the bundle remains validly at the old generation.)

- **Reader resource limits.** Untrusted lengths (a superblock's
  `manifest_length`, a manifest's chunk/blob lengths) are checked against
  policy caps (`MAX_MANIFEST_BYTES`, `MAX_CHUNK_BYTES`, `MAX_BLOB_BYTES`, plus a
  `BlobRef`'s own `declared_max`) *before* any allocation, so a large/sparse
  hostile file cannot drive an OOM or a 32-bit truncation. The values are
  generous v0 defaults; a production reader would make them configurable.

- **Writer/reader symmetry.** `create`/`commit` refuse to emit anything their
  own `open` would reject: at least one declared profile; the canonical base's
  profile declared and its reduction-version consistent; every canonical root
  present and of the right kind/shape (operation roots decode and fit the
  profile's max block size; the canonical base is a snapshot; every blob root
  resolves and hash-verifies); the encoded manifest within `MAX_MANIFEST_BYTES`;
  no initial canonical roots/blobs at `create`. After emitting, the in-memory
  manifest is **normalized** (decoded back from its own canonical bytes), so
  `bundle.manifest()` matches a reopen (duplicate roots already collapsed), and a
  required extension makes even a freshly *created* bundle read-only. `open`
  additionally checks the manifest lies in the body, its generation matches the
  superblock, the superblock's profile is declared, and profile ids are distinct.
  The *active* profile (and thus the max block size enforced on reads) is the one
  the **selected superblock** names — a bundle opened under `Lite` reads under
  `Lite`'s limits, not the canonical-first profile's.

- **Profile support model (emittability vs editability).** A profile is
  *editable* (`Full`/`Lite`, exact major, block bound ≤ the reader's
  `MAX_CHUNK_BYTES`), *understood but read-only* (`ReadOnly`), or *unsupported*
  (`Custom` registry profiles, a mismatched major, or a block bound the reader
  cannot allocate). **Emittability and editability are separate.** A bundle is
  emittable as long as it declares at least one *understood* profile — so a sole
  `ReadOnly` profile produces a valid read-only bundle (the spec describes
  ReadOnly-produced bundles). The active profile a writer names prefers the
  canonical-first *editable* profile (so `[ReadOnly, Lite]` is emitted under
  `Lite`, editable), falling back to the first merely understood one (a sole
  `ReadOnly` → read-only bundle). That selected declaration is computed once and
  drives canonical-root limits, the superblock profile, and the live read-only
  state, so commit-time validation cannot disagree with the reopened bundle.
  `open` mirrors this: an understood profile
  opens read-only-if-`ReadOnly`, an unsupported one opens read-only with an
  `UnsupportedProfile` anomaly. A profile major must match exactly. The spec's
  *SHOULD* upgrade-the-profile-on-first-edit is **deferred** — v0 opens a
  `ReadOnly` bundle read-only rather than rewriting the profile.

- **Numeric `SemVer` ordering.** Declarations carrying a `SemVer` (profiles,
  extensions) sort by an explicit numeric `(id, version)` key, not by encoded
  bytes — the little-endian version integers would otherwise sort byte-wise
  (`256.0.0` before `1.0.0`), violating Appendix D's "ascending by … semantic
  version."

- **Blob media types** are validated as the narrow RFC 6838 §4.2 *restricted
  name* `type/subtype` (ASCII, ≤127 chars per component, alphanumeric-first, the
  restricted-name alphabet — not the broader HTTP token set) on both decode and
  emit, keeping arbitrary or non-NFC bytes out of canonical manifests (ASCII is
  already NFC).

- **Generation exhaustion** returns `BundleError::GenerationExhausted` rather
  than overflow-panicking at `u64::MAX`.

- **Zstd chunk *reading* is supported; the write path stays uncompressed
  (2026-07-01 spec-audit fix).** A spec audit flagged the read paths as the
  file-format chapter's only exercised-path MUST violation: Chapter 8
  §"Compression" requires conforming implementations to support *reading*
  chunks "compressed with Zstandard at any level zstd defines", but
  `read_and_verify_chunk`/`read_and_verify_blob` returned
  `UnsupportedCompression` for anything but `None`. They now decompress
  `Zstd` payloads (`Reserved` remains `UnsupportedCompression`; the writer
  still emits only `None` — the QUICKSTART's compression deferral is about
  the *write* path, which the spec leaves as MAY). Decisions taken:
  - **Dependency: the `zstd` crate (libzstd bindings), not pure-Rust
    `ruzstd`.** (1) `zstd::bulk::decompress_to_buffer` writes into a
    caller-allocated buffer sized *exactly* from the declared
    `uncompressed_length` — which is validated against the reader's
    resource limits *before* allocation — so a hostile stream has a hard
    output bound, and libzstd's decoding window is capped internally;
    (2) libzstd is the reference implementation, battle-tested against
    malformed frames, matching this crate's hostile-input posture;
    (3) the workspace already requires a C toolchain (`blake3`'s `cc`
    build), so pure Rust bought nothing here; (4) tests need an *encoder*
    to produce fixtures and `ruzstd` is decode-only, so picking it would
    have pulled `zstd` in anyway as a dev-dependency — two zstd
    implementations in one build graph. The read-only mandate is enforced
    at the call sites instead: production code never calls the encoder.
  - **Length rule (spec: "reject chunks whose decompressed size
    disagrees").** The output buffer is sized exactly by the declared
    length: a stream that ends short yields a precise
    `ChunkLengthMismatch`; one that would exceed the declaration hits
    libzstd's destination-full error; malformed, truncated, and
    trailing-garbage streams all fail — the latter three as the new typed
    `BundleError::Decompression`. No path panics or allocates past the
    declaration. Hashing (including the `id == hash` redundancy) is
    unchanged and runs strictly *after* decompression, over the
    uncompressed bytes — compression stays outside content identity.
  - **The manifest stays mandatorily uncompressed** (§"Manifest
    Encoding"). The superblock deliberately has no compression field, so
    stored manifest bytes are the payload; an image whose manifest bytes
    are compressed anyway fails to open (hash mismatch →
    `NoValidSuperblock`, or, with a colluding hash over the compressed
    bytes, a manifest decode failure). A `ChunkKind::Manifest` *chunk
    reference* declaring compression is additionally refused outright with
    the new typed `BundleError::CompressedManifest`, before any bytes are
    read.
  - `CompressionAlgorithm`'s golden-locked two-byte encoding
    (`req:format:chunkkind-discriminants`) is untouched; the ratified
    discriminants already modeled `Zstd { level } = 1`.

- **The operation index is implemented with a provisional, golden-locked
  payload (Push-3).** Chapter 8 §"The Operation Index" defines the semantics —
  an *optional, non-canonical* accelerator mapping each `OperationId` to the
  `ChunkRef` of its enclosing block plus an offset within the block, O(log n)
  lookup, absent → rebuild by scanning, present-but-corrupt-or-stale → MUST
  reject and rebuild — but defers the byte format to the Binary Format
  companion (P11-D2). Until that lands, `OperationIndex` encodes under this
  crate's fixed codec conventions and the exact bytes are **golden-locked**
  (`opindex::tests::payload_encoding_is_golden`), so a layout change breaks
  deliberately:

  ```text
  u32 block_count
    block_count × ChunkRef            — strictly ascending canonical order
                                        (kind discriminant, hash, offset)
  u32 entry_count
    entry_count × { id: [u8;16], block: u32 LE, offset: u32 LE }
                                      — strictly ascending by id bytes
  ```

  `block` is an ordinal into the block vector; `offset` is the byte offset of
  the envelope's **first content byte** within the block's *decoded*
  (uncompressed) payload — exactly the coordinate `envelope_offsets` reports
  (its `u32` length prefix sits at `offset - 4`). Decisions taken:
  - **Layering: raw id bytes in the bundle, the peek in ops.** The bundle
    stays semantics-free — entries key on the opaque 16 canonical id bytes.
    That a canonical envelope *leads* with those bytes is an `epiphany-ops`
    invariant, vouched for by ops' `peek_operation_id` (tested against
    `encode_canonical`); builders pair it with the bundle's
    `envelope_offsets` (which shares `decode_block`'s exact validation) to
    produce index entries. The same "ops computes, bundle carries" split as
    the block-summary metadata.
  - **Reject, never normalize.** `OperationIndex::decode` rejects unsorted or
    duplicated blocks or ids, a non-`OperationEnvelopeBlock` reference, an
    out-of-range ordinal, and trailing bytes — the manifest decoder's
    discipline, so accepted bytes are byte-stable. `build` rejects duplicate
    ids (an `OperationId` occupies exactly one slot in one block) and
    duplicate blocks at construction.
  - **Staleness is coverage equality over full `ChunkRef`s.**
    `OperationIndex::covers` is true iff the index's block set equals the
    manifest's `operation_roots` set as *full references*, not just chunk
    ids: `locate` hands out the index's stored refs for reading, so a ref
    agreeing in hash but differing in any locator field (offset, lengths,
    compression) is not the manifest's block and must count as stale rather
    than steering reads elsewhere. `false` = stale → reject and rebuild.
  - **A defective index is never bundle corruption** (Chapter 8 §"Canonical
    and Non-Canonical Manifest Roots"). `Bundle::usable_operation_index`
    packages the whole discipline: `Some` only for a declared, readable,
    hash-intact, well-formed index covering the current operation roots;
    `None` on *any* defect, meaning "rebuild by scanning all blocks".
    `Bundle::read_operation_index` exposes the underlying failure for
    diagnostics only. The testkit proves the boundary: a garbage or
    byte-flipped index chunk leaves the bundle opening cleanly with all
    canonical reads intact
    (`bundle_harness::assert_corrupt_operation_index_is_not_bundle_corruption`).
  - **The commit-time SHOULD is a builder, not a policy.** The spec says
    writers SHOULD rebuild/update the index at commit when the operation set
    has grown significantly. v0 deliberately ships the *mechanism* —
    `OperationIndex::build` from per-block `(id bytes, offset)` lists,
    `StagedChunk::operation_index`, and the testkit's commit-time
    rebuild-and-wire demonstration
    (`bundle_harness::assert_operation_index_end_to_end` /
    `scan_rebuild_operation_index`) — and no automatic "grown significantly"
    heuristic; when/how often to refresh is editor policy layered above this
    crate.
  - **The write path stays uncompressed.** The spec's *MAY* compress
    operation indexes is honored on the read side (an index chunk reads
    through the same zstd-capable `read_chunk` path as any chunk); writing
    compressed indexes is deferred with the rest of write-path compression.

## Known v0 limitations (deliberately deferred, not defects)

These are bounded by v0 scope (QUICKSTART "Don't do these" / "decisions you'll
need to make") rather than spec ambiguities. Each is honest about what is *not*
yet enforced so a later integration knows where to extend.

- **Retention/GC is a type, not an engine.** `RetentionPolicy` is modeled as the
  QUICKSTART asks, and rollback over the two fixed slots is structurally
  supported, but there is no retained-manifest catalog, deterministic retention
  *selection*, GC reachability pass, or rollback *operation*. The spec itself
  frames GC as *"a conservative, optional, deferred operation"* that *"MUSTNOT
  run as part of a commit's critical path"* (Chapter 8 §"Garbage Collection and
  Retention"), and the body is append-only in v0, so nothing is reclaimed yet —
  manifests older than one generation are simply retained. A policy requesting
  more than one retained manifest cannot be *honored for reclamation* until the
  GC engine lands, but no manifest is *lost* either.

- **Content-address dedup is current-manifest scoped, not whole-history.** A
  commit reuses a chunk (or blob) already referenced by the *active* manifest
  rather than re-appending it. It does not dedup against older retained manifests
  or unreferenced garbage, and it trusts the existing reference's location
  without re-verifying the chunk (the chunk was verified when first committed and
  is re-verified on read). Whole-history dedup needs the same body-wide content
  index as the deferred GC engine.

- **Operation-envelope block summary metadata is carried (M4 follow-up).**
  Chapter 8's `OperationEnvelopeBlock` carries `dvv_summary`, `min_stamp`, and
  `max_stamp`. These are *semantic* — a DVV and `OperationStamp`s computed by
  reading the envelopes, which belong to `epiphany-ops` (Agent C). The bundle
  still treats a block as opaque envelope bytes and cannot compute them, but the
  manifest now carries an `OperationBlockSummary { dvv_summary, min_stamp,
  max_stamp }` per block, keyed by the block's `ChunkId`
  (`Manifest::operation_block_summaries` / `operation_block_summary`), as
  **opaque ops-supplied bytes** in canonical (ChunkId-ascending) order. This lets
  a reader select or skip a block by causal frontier / stamp range without
  decoding it. The C/D integration point — ops computes the summary, the bundle
  carries it — is exercised end to end by Agent F
  (`roundtrip::operation_block_summary` +
  `assert_operation_block_summary_survives_storage`). `read_operation_block`
  still enforces the chunk kind and the active profile's maximum block size.

- **Schema negotiation is major-gate only.** A canonical chunk or manifest at an
  unsupported schema *major* is refused (`BundleError::UnsupportedSchemaVersion`);
  v0 defines only schema `0.x`, so there is no minor-version back-compat matrix
  to exercise yet. Non-canonical opaque chunks at unknown majors are carried
  verbatim (they are never parsed).

- **Extensions: required → read-only; opaque preservation is now enforced.** An
  unknown *required* extension forces read-only (v0 understands no extensions,
  so all are unknown). Optional-extension `preserved_chunk_roots` are carried in
  the manifest, and (M4 follow-up) `commit` now **enforces** preservation:
  after the builder closure runs, every prior extension declaration the closure
  did not itself re-declare (by `extension_id`) is carried forward verbatim, so
  an extension-*unaware* writer cannot silently orphan an unknown extension's
  roots; an extension-*aware* writer that re-declares its own id keeps control.
  (Edit barriers / the unsafe-edit path are still not evaluated — barrier
  operands `OperationKindTag`/`ObjectKind`/`EditBarrier` are owned by Agents
  C/E.) The commit closure is also validated to never publish dangling or
  mismatched *canonical* roots.

## Pass 11 candidates (ambiguities for the spec, not resolved in code)

### P11-D1 — Superblock selection has no tie-break for equal generations

Chapter 8 §"Superblock Selection" says *"the slot with the higher generation is
active"* but specifies no rule for two valid slots at the **same** generation
(which the QUICKSTART nonetheless lists as a scenario the harness must handle).
This crate resolves it deterministically: equal generation **and** equivalent
load-bearing fields (`manifest_hash`, `manifest_schema_version`,
`reduction_algorithm_version`, `profile_id` — the advisory `commit_timestamp`
and the physical manifest offset/length are excluded) → the slots are
equivalent, pick A; equal generation that differs in any of those → an
`IntegrityAnomaly::DivergentSameGeneration`, opened read-only (two different
committed states cannot share a generation under a conforming writer). The spec
should adopt or override this.

### P11-D2 — The Binary Format companion is not yet written

The concrete byte layout in this crate (prelude field offsets, integer
endianness, length-prefix widths, option/enum-discriminant encodings) is
provisional, standing in for the deferred Binary Format companion specification.
When that companion lands, reconcile this crate's `header`, `superblock`,
`chunk`, and `manifest` encodings with it (a failing cross-implementation
round-trip would be the trigger, per the QUICKSTART process notes). This is the
file-format analogue of `epiphany-core`'s P11-4.

> **Ratified (2026-07-02):** `spec/binary_format.tex` v0.1.0 Chapter 7 pins
> this crate's header (64-byte table), superblock (256-byte table), chunk
> framing and hash preimages, `ChunkRef`, block framing, the manifest body
> order with its sort/dedup rules, and the operation-index payload (P12-D1,
> `req:binfmt:opindex`) exactly as implemented and golden-locked here. The
> reconciliation trigger never fired: the companion was transcribed from this
> crate.

### P11-D3 — Blob hashing shape is ambiguous

Chapter 8 §"Blobs" says blobs are *"content-addressed identically to chunks
(BLAKE3 of uncompressed payload, with the `MUSCBLOB` domain tag)."* "Identically
to chunks" implies the structured preimage (which commits to kind, schema
version, and length); "BLAKE3 of uncompressed payload with the domain tag"
implies a bare `MUSCBLOB || payload`. The two disagree. This crate follows Agent
A's `ContentHash::of_blob` (bare `MUSCBLOB || payload`), the only spec content
hash documented as a bare `domain || payload`. The spec should state explicitly
whether a `BlobId` commits to a kind/schema/length or is bare.

### P11-D4 — Enum discriminant values entering canonical state are unspecified

`ChunkKind::canonical_bytes()` appears in the chunk hash preimage (Chapter 8
§"Domain-Separated Preimages"), so each `ChunkKind`'s numeric discriminant is
**normative** — yet Chapter 8 fixes only the *shape* of the preimage, not the
discriminant table (exactly the situation `epiphany-core` P11-1 flags for
`TypedObjectId`). This crate assigns `ChunkKind` discriminants by declaration
order (`OperationEnvelopeBlock = 0` … `Manifest = 8`), as a single byte, and
likewise fixes `ProfileId` (0–3) and `CompressionAlgorithm` (0–2) discriminants.
The spec should pin these, since the `ChunkKind` value in particular changes
content hashes.

### P11-D5 — `ManifestId` derivation inputs are undefined

Chapter 8 §"The Manifest" says *"Each commit produces a new `ManifestId`"* and the
deferred-types table assigns it the `MUSCMNIF` domain tag, but no derivation
preimage is given. This crate derives
`trunc128(BLAKE3("MUSCMNIF" || document_id || generation || manifest_body))`,
where `manifest_body` is the canonical manifest encoding with the `manifest_id`
field excluded (to avoid self-reference). The spec should fix the canonical
input list so two conforming writers derive identical manifest ids.

### P11-D6 — Where the `RetentionPolicy` lives is not shown

Chapter 8 §"Garbage Collection and Retention" requires that *"the active
conformance profile MUST declare a `RetentionPolicy`,"* but the
`ProfileDeclaration` / `ProfileConstraints` structs shown in §"Format Profiles"
do not include the field. This crate places `retention_policy` inside
`ProfileConstraints`. The spec should show the field explicitly (and confirm
whether a bundle declaring multiple profiles resolves retention from the first
declared profile, as this crate does).
