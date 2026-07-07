//! The open/create/commit driver: the cold-open path and the atomic write
//! protocol (Chapter 8 §"The Atomic Write Protocol", §"Crash Recovery",
//! §"Streaming Reads").
//!
//! A [`Bundle`] wraps a [`BlockStore`] and the currently-active prelude state.
//! Its two load-bearing operations are:
//!
//! * [`Bundle::open`] — the cold-open procedure: header → superblock selection →
//!   manifest read-and-verify, exposing the canonical roots for streaming.
//! * [`Bundle::commit`] — the seven-step atomic write protocol, whose final
//!   durable flush is the commit point. A crash at any step leaves the bundle at
//!   the previous generation (the active superblock and its reachable chunks are
//!   never touched) or, only after the final flush, at the new generation.

use crate::block;
use crate::chunk::{
    chunk_content_hash, chunk_id, content_hash_for, ChunkKind, ChunkRef, CompressionAlgorithm,
};
use crate::codec::DecodeError;
use crate::error::{BundleError, IntegrityAnomaly};
use crate::header::{FixedHeader, SLOT_A_OFFSET, SLOT_B_OFFSET};
use crate::ids::{BlobId, FileUuid, ReductionAlgorithmVersion, SchemaVersion, WallClockTime};
use crate::manifest::{BlobRef, Manifest, ProfileDeclaration};
use crate::opindex::OperationIndex;
use crate::store::{read_vec, BlockStore, MemStore};
use crate::superblock::{
    select_active, CommitState, ProfileId, Slot, SlotParse, SlotReject, Superblock, SUPERBLOCK_LEN,
};
use epiphany_determinism::{ChunkId, ContentHash};
use std::collections::BTreeMap;

/// Offset where variable-length body content begins (after the header and both
/// superblock slots): 576 bytes.
pub const BODY_START: u64 = SLOT_B_OFFSET + SUPERBLOCK_LEN;

/// The schema major version this reader can parse for a **generic canonical
/// chunk** (Chapter 8 §"Schema Versioning"). A chunk at a higher major is not
/// interpretable by this reader as canonical state.
///
/// This is the baseline for a **generic** chunk role. Admission of major 1 is
/// raised **per chunk role** by `max_supported_major` as each role's versioned
/// path lands — never as a blanket accept-set ahead of the decoders (which would
/// let a spec-valid major-1 chunk reach an unversioned decoder and be mis-read).
/// The operation-envelope-block role is raised to major 1 (schema-major track
/// D2: a block bearing a v1 `CreateRegion`); the manifest and layout-cache roles
/// stay at `0`.
pub const SUPPORTED_SCHEMA_MAJOR: u16 = 0;

/// The maximum schema major this reader admits for a chunk of `kind` — the upper
/// bound of its per-role accept-set `[0, max]` (Binary Format companion
/// §"Schema Major 1", "The accept-set gate").
///
/// `OperationEnvelopeBlock` admits major 2 (schema major 2 fills the
/// cross-cutting/staff/metadata bodies its payloads embed; major 1 embedded a
/// v1 `CreateRegion`; the reader treats the block bytes opaquely, so it
/// parses a higher-major block without decoding the payload). Every other
/// role stays at
/// [`SUPPORTED_SCHEMA_MAJOR`] until its own versioned path lands — including the
/// payload-polymorphic `Snapshot` (the acceleration form's migrate-on-read is
/// core-side; the canonical base stays major 0) and the manifest (carried
/// opaquely, never grows a v1 layout). A chunk above its role's max is not
/// admitted; for a **canonical** role that means the bundle opens read-only
/// (a major-0-only reader meeting a v1 op block), not a hard reject.
pub fn max_supported_major(kind: ChunkKind) -> u16 {
    match kind {
        ChunkKind::OperationEnvelopeBlock => 2,
        _ => SUPPORTED_SCHEMA_MAJOR,
    }
}

/// The conformance-profile major version this implementation understands
/// (Chapter 8 §"Format Profiles"). A profile declared at a higher major is a
/// future capability set this reader cannot honor.
pub const SUPPORTED_PROFILE_MAJOR: u32 = 0;

/// Reader resource limit on a manifest chunk. The spec calls manifests "a few
/// kilobytes at most"; this generous bound stops an untrusted superblock length
/// from driving an allocation before the bytes are read (finding: untrusted
/// length → OOM/truncation).
pub const MAX_MANIFEST_BYTES: u64 = 64 << 20;

/// Reader resource limit on a single chunk read, bounding allocation from an
/// untrusted length regardless of (possibly sparse) file size.
pub const MAX_CHUNK_BYTES: u64 = 256 << 20;

/// Default reader resource limit on a blob (Chapter 8 §"Blobs"). A `BlobRef`'s
/// own `declared_max_uncompressed_length`, if smaller, also applies.
pub const MAX_BLOB_BYTES: u64 = 1 << 30;

/// A chunk to be written by a commit: an opaque payload plus its kind and schema
/// version. The bundle assigns its offset, computes its content hash, and
/// returns a [`ChunkRef`] for the manifest builder to wire into roots.
#[derive(Clone, Debug)]
pub struct StagedChunk {
    /// The chunk's kind (dispatches parsing on read).
    pub kind: ChunkKind,
    /// The schema version the payload is encoded against.
    pub schema_version: SchemaVersion,
    /// The opaque uncompressed payload bytes.
    pub payload: Vec<u8>,
}

impl StagedChunk {
    /// A staged operation-envelope block at schema major 0 (the baseline: no
    /// operation in the block carries a schema-major-1 payload). Use
    /// [`StagedChunk::operation_block_versioned`] for a block whose operations
    /// may include a v1 `CreateRegion`.
    pub fn operation_block(payload: Vec<u8>) -> Self {
        StagedChunk::operation_block_versioned(payload, SchemaVersion::V0)
    }

    /// A staged operation-envelope block at the given schema version. The version
    /// is the maximum over the block's operations
    /// (`OperationEnvelope::schema_major`, projected to `SchemaVersion`): a block
    /// bearing a v1 `CreateRegion` is stamped [`SchemaVersion::V1`], so a
    /// major-0-only reader opens the bundle read-only rather than
    /// mis-parsing v1 op bytes as v0 (Binary Format companion §"Schema Major 1").
    pub fn operation_block_versioned(payload: Vec<u8>, schema_version: SchemaVersion) -> Self {
        StagedChunk {
            kind: ChunkKind::OperationEnvelopeBlock,
            schema_version,
            payload,
        }
    }

    /// A staged operation-index chunk at the current schema version (Chapter 8
    /// §"The Operation Index") — a non-canonical accelerator; the payload is an
    /// [`OperationIndex::encode`](crate::OperationIndex::encode). v0 writes it
    /// uncompressed, like every chunk (the spec's *MAY* compress indexes is a
    /// write-path option this version defers).
    pub fn operation_index(payload: Vec<u8>) -> Self {
        StagedChunk {
            kind: ChunkKind::OperationIndex,
            schema_version: SchemaVersion::V0,
            payload,
        }
    }
}

/// Context handed to a commit's manifest builder: the previous manifest, the
/// [`ChunkRef`]s for the chunks this commit just wrote (in staging order), and
/// the generation the new manifest will carry.
pub struct CommitContext<'a> {
    /// The manifest active before this commit.
    pub previous_manifest: &'a Manifest,
    /// References to the chunks written by this commit, in staging order.
    pub new_chunks: &'a [ChunkRef],
    /// The generation the new manifest must declare (active + 1).
    pub generation: u64,
}

/// An open bundle over a block store.
pub struct Bundle<S: BlockStore> {
    store: S,
    header: FixedHeader,
    active_slot: Slot,
    superblock: Superblock,
    manifest: Manifest,
    /// Next append offset. Always at end-of-file, so appends never overwrite a
    /// reachable chunk (Chapter 8 §"The Atomic Write Protocol", step 1).
    write_cursor: u64,
    read_only: bool,
    anomalies: Vec<IntegrityAnomaly>,
}

impl<S: BlockStore> Bundle<S> {
    /// Creates a brand-new bundle in `store` with the given file UUID and
    /// initial manifest (its generation is forced to 0). Writes the manifest
    /// chunk, then the header, the generation-0 superblock in slot A, and a
    /// zeroed (invalid) slot B; flushes at the manifest and at the superblock.
    ///
    /// A crash *during creation* may leave a half-formed file that [`Bundle::open`]
    /// rejects as corrupt — acceptable, since the file is not yet a bundle. The
    /// crash-safety guarantee is about *commits to an existing bundle*.
    pub fn create(
        mut store: S,
        file_uuid: FileUuid,
        mut manifest: Manifest,
    ) -> Result<Self, BundleError> {
        manifest.generation = 0;
        manifest.manifest_id = manifest.derive_id();

        // The manifest must be emittable (it would otherwise be rejected by
        // `open`): at least one declared profile.
        let active_profile = validate_emittable_manifest(&manifest)?;

        // A freshly created bundle writes only the manifest chunk, so it cannot
        // have any canonical roots or blobs yet: reject an initial manifest that
        // declares (necessarily dangling) operation roots, a canonical base, or
        // blob roots.
        if !manifest.operation_roots.is_empty()
            || manifest.canonical_base.is_some()
            || !manifest.blob_roots.is_empty()
        {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "create() requires a manifest with no canonical roots or blobs",
            )));
        }

        // Encode the manifest and enforce the reader's manifest-size limit *here*
        // (a writer must not emit a manifest its own `open` would reject as
        // oversize). Then normalize the in-memory copy to the canonical
        // (sorted/deduplicated) form by decoding the bytes we just produced, so
        // `bundle.manifest()` matches what a reopen would yield.
        let manifest_payload = manifest.encode();
        enforce_limit(manifest_payload.len() as u64, MAX_MANIFEST_BYTES)?;
        let manifest = Manifest::decode(&manifest_payload)?;
        let manifest_hash =
            chunk_content_hash(ChunkKind::Manifest, Manifest::SCHEMA, &manifest_payload);
        store.write_at(BODY_START, &manifest_payload)?;
        store.flush()?;

        let superblock = Superblock {
            generation: 0,
            manifest_offset: BODY_START,
            manifest_length: manifest_payload.len() as u64,
            manifest_hash,
            manifest_schema_version: Manifest::SCHEMA,
            reduction_algorithm_version: reduction_version_for(&manifest),
            profile_id: active_profile.profile_id,
            commit_state: CommitState::Committed,
            commit_timestamp: WallClockTime(0),
        };

        let header = FixedHeader::new(file_uuid);
        store.write_at(0, &header.encode())?;
        store.write_at(SLOT_A_OFFSET, &superblock.encode())?;
        // Slot B is explicitly zeroed so it is invalid (bad magic) until the
        // first commit writes a real superblock there.
        store.write_at(SLOT_B_OFFSET, &[0u8; SUPERBLOCK_LEN as usize])?;
        store.flush()?;

        let write_cursor = store.len();
        // A required unknown extension, or a non-editable (`ReadOnly`) active
        // profile, makes even a freshly created bundle read-only.
        let read_only =
            manifest_forces_read_only(&manifest) || !profile_is_editable(&active_profile);
        Ok(Bundle {
            store,
            header,
            active_slot: Slot::A,
            superblock,
            manifest,
            write_cursor,
            read_only,
            anomalies: Vec::new(),
        })
    }

    /// Opens an existing bundle: the cold-open procedure (Chapter 8
    /// §"Cold Open Procedure"). Verifies the header (magic, CRC), selects the
    /// active superblock (validating each slot's magic, CRC, commit state, and
    /// manifest hash), and reads + verifies the manifest. A structural anomaly
    /// (generation gap, divergent same-generation slots, a non-committed slot)
    /// opens the bundle **read-only** and is recorded in [`Bundle::anomalies`].
    pub fn open(store: S) -> Result<Self, BundleError> {
        // 1. Header.
        let header_bytes = read_vec(&store, 0, crate::header::HEADER_LEN)?;
        let header = FixedHeader::decode(&header_bytes)?;

        // 2. Both slots.
        let slot_a_bytes = read_vec(&store, SLOT_A_OFFSET, SUPERBLOCK_LEN)?;
        let slot_b_bytes = read_vec(&store, SLOT_B_OFFSET, SUPERBLOCK_LEN)?;
        let mut anomalies = Vec::new();

        // 3. Parse + manifest-hash-verify each slot. A slot survives only if it
        //    is a valid, committed superblock whose manifest chunk verifies.
        let a = verified_slot(&store, &slot_a_bytes, &mut anomalies);
        let b = verified_slot(&store, &slot_b_bytes, &mut anomalies);

        // 4–6. Apply the selection rule. A *selection-level* anomaly (a
        // generation gap > 1, or divergent slots at the same generation) forces
        // read-only recovery — these are the cases Chapter 8 says MUSTNOT be
        // treated as normal. A non-committed slot (recorded by `verified_slot`)
        // is ordinary fallback per the atomic-write protocol: it is surfaced but
        // does not, on its own, block editing.
        let selection = select_active(a, b)?;
        let mut read_only = selection.anomaly.is_some();
        if let Some(anomaly) = selection.anomaly.clone() {
            anomalies.push(anomaly);
        }
        let superblock = selection.superblock;

        // A manifest at an unsupported schema major cannot be interpreted as
        // canonical state (Chapter 8 §"Schema Versioning"). The manifest stays
        // at its own schema major across the schema-major-1 bump (its body is
        // carried opaquely and never grows a v1 layout here), so this gate is
        // exact to `Manifest::SCHEMA.major` — it must not admit a manifest major
        // that has no defined wire form.
        if superblock.manifest_schema_version.major != Manifest::SCHEMA.major {
            return Err(BundleError::UnsupportedSchemaVersion {
                version: superblock.manifest_schema_version,
            });
        }

        // Read and decode the manifest the selected superblock points at. The
        // placement (in-body, within the size limit) and hash were already
        // checked by `verified_slot`; `Manifest::decode` additionally rejects
        // non-canonical bytes.
        let manifest_payload = read_manifest_payload(&store, &superblock)?;
        let manifest = Manifest::decode(&manifest_payload)?;

        // The manifest's self-declared generation must match the superblock that
        // referenced it (a mismatch is structural corruption).
        if manifest.generation != superblock.generation {
            return Err(BundleError::GenerationMismatch {
                superblock: superblock.generation,
                manifest: manifest.generation,
            });
        }
        // Every bundle MUST declare at least one profile, with distinct ids
        // (Chapter 8 §"Format Profiles").
        if manifest.profile_declarations.is_empty() {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "manifest declares no conformance profile",
            )));
        }
        if !profile_ids_distinct(&manifest) {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "manifest declares the same profile id more than once",
            )));
        }
        // The active superblock's profile must be one the manifest declares.
        let active_profile = manifest
            .profile_declarations
            .iter()
            .find(|p| p.profile_id == superblock.profile_id)
            .copied();
        let active_profile = match active_profile {
            Some(p) => p,
            None => {
                return Err(BundleError::Decode(DecodeError::Malformed(
                    "superblock profile is not declared by the manifest",
                )))
            }
        };
        // If the active profile is not understood (a `Custom` registry profile,
        // a future major, or a block bound beyond the reader's limit), open
        // read-only and surface it; if it is understood but not editable
        // (`ReadOnly`), open read-only silently. (Chapter 8 §"Format Profiles":
        // a reader edits only under a profile it supports.)
        if !profile_is_understood(&active_profile) {
            read_only = true;
            anomalies.push(IntegrityAnomaly::UnsupportedProfile);
        } else if !profile_is_editable(&active_profile) {
            read_only = true;
        }
        // A canonical base is usable only if its reduction-algorithm version
        // matches the active superblock's *and* its profile is one the manifest
        // declares (Chapter 8 §"Canonical Document Identity").
        if let Some(base) = &manifest.canonical_base {
            if base.reduction_algorithm_version != superblock.reduction_algorithm_version {
                return Err(BundleError::Decode(DecodeError::Malformed(
                    "canonical base reduction-algorithm version disagrees with the superblock",
                )));
            }
            if !manifest
                .profile_declarations
                .iter()
                .any(|p| p.profile_id == base.profile_id)
            {
                return Err(BundleError::Decode(DecodeError::Malformed(
                    "canonical base profile is not declared by the manifest",
                )));
            }
        }
        // An unknown *required* extension forces read-only (Chapter 8 §"Behavior
        // Under Unknown Extensions").
        if manifest_forces_read_only(&manifest) {
            read_only = true;
            anomalies.push(IntegrityAnomaly::UnknownRequiredExtension);
        }

        // A canonical operation root at a schema major above this reader's
        // accept-set forces read-only preservation (Binary Format companion
        // §"Schema Major 1"): the reader still reads the canonical base and
        // manifest (both major 0) but refuses to author against op history it
        // cannot interpret. A cheap manifest-metadata scan; the beyond-accept
        // blocks are never read (a lazy read would hit the accept-set gate).
        if let Some(major) = unsupported_operation_root_major(&manifest) {
            read_only = true;
            anomalies.push(IntegrityAnomaly::UnsupportedCanonicalChunkMajor {
                schema_major: major,
            });
        }

        let write_cursor = store.len();
        Ok(Bundle {
            store,
            header,
            active_slot: selection.slot,
            superblock,
            manifest,
            write_cursor,
            read_only,
            anomalies,
        })
    }

    /// The active manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The active generation.
    pub fn generation(&self) -> u64 {
        self.superblock.generation
    }

    /// The fixed header.
    pub fn header(&self) -> &FixedHeader {
        &self.header
    }

    /// The active superblock.
    pub fn superblock(&self) -> &Superblock {
        &self.superblock
    }

    /// Which slot is currently active.
    pub fn active_slot(&self) -> Slot {
        self.active_slot
    }

    /// The physical-bundle UUID.
    pub fn file_uuid(&self) -> FileUuid {
        self.header.file_uuid
    }

    /// Whether the bundle is open read-only (an integrity anomaly was detected,
    /// or an unknown required extension). Commits are refused.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Structural anomalies detected at open (empty for a normal bundle).
    pub fn anomalies(&self) -> &[IntegrityAnomaly] {
        &self.anomalies
    }

    /// The underlying store.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Consumes the bundle, returning its store (e.g. to inspect the bytes).
    pub fn into_store(self) -> S {
        self.store
    }

    /// Reads and fully verifies a chunk against its reference: that it lies in
    /// the body (not the prelude), that its schema major is supported, its
    /// declared length, its BLAKE3 content hash, and the `id == hash` redundancy
    /// the spec keeps (Chapter 8 §"Chunks"). A canonical chunk whose hash fails
    /// is hard corruption ([`BundleError::ChunkHashMismatch`]).
    pub fn read_chunk(&self, r: &ChunkRef) -> Result<Vec<u8>, BundleError> {
        read_and_verify_chunk(&self.store, r)
    }

    /// Reads and verifies a blob (Chapter 8 §"Blobs"): the same checks as
    /// [`Bundle::read_chunk`], but addressed via the bare `MUSCBLOB` hash. A
    /// resource-limit check honors the blob's `declared_max_uncompressed_length`.
    pub fn read_blob(&self, b: &BlobRef) -> Result<Vec<u8>, BundleError> {
        read_and_verify_blob(&self.store, b, MAX_BLOB_BYTES)
    }

    /// Reads a chunk and splits it into its opaque operation-envelope byte
    /// strings (Chapter 8 §"Operation Envelope Blocks"). Rejects a reference of
    /// the wrong kind, and a block whose uncompressed size exceeds the active
    /// profile's maximum block size. The bundle does not interpret the envelopes.
    pub fn read_operation_block(&self, r: &ChunkRef) -> Result<Vec<Vec<u8>>, BundleError> {
        if r.kind != ChunkKind::OperationEnvelopeBlock {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "chunk reference is not an operation-envelope block",
            )));
        }
        if r.uncompressed_length > self.max_block_size() {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "operation-envelope block exceeds the active profile's maximum size",
            )));
        }
        let payload = self.read_chunk(r)?;
        block::decode_block(&payload).map_err(BundleError::Decode)
    }

    /// Reads and decodes an operation-index chunk (Chapter 8 §"The Operation
    /// Index"). Rejects a reference of the wrong kind; the read itself is
    /// bounded by the reader's [`MAX_CHUNK_BYTES`] policy and hash-verified
    /// like any chunk read.
    ///
    /// **The index is not canonical**: a failure here — corrupt bytes, a
    /// malformed payload, even an I/O error on the index region — must NOT be
    /// treated as bundle corruption. The spec's discipline is *reject and
    /// rebuild from blocks*; [`Bundle::usable_operation_index`] packages it
    /// (including the staleness check). Call this directly only when the
    /// underlying failure itself is wanted, e.g. for diagnostics.
    pub fn read_operation_index(&self, r: &ChunkRef) -> Result<OperationIndex, BundleError> {
        if r.kind != ChunkKind::OperationIndex {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "chunk reference is not an operation index",
            )));
        }
        let payload = self.read_chunk(r)?;
        OperationIndex::decode(&payload).map_err(BundleError::Decode)
    }

    /// The manifest's operation index, if — and only if — it is *usable*:
    /// declared, readable, hash-intact, well-formed, and covering exactly the
    /// manifest's current `operation_roots` ([`OperationIndex::covers`]).
    /// `None` on **any** defect: absent, stale (its block set differs from the
    /// operation roots), corrupt, malformed, or unreadable.
    ///
    /// `None` always means "rebuild by scanning all blocks", never "the bundle
    /// is corrupt": the operation index is an acceleration structure, not
    /// canonical (Chapter 8 §"The Operation Index"), and failed verification
    /// of a non-canonical chunk MUST NOT be surfaced as bundle corruption
    /// (Chapter 8 §"Canonical and Non-Canonical Manifest Roots") — the reader
    /// discards the index and rebuilds it from the blocks.
    pub fn usable_operation_index(&self) -> Option<OperationIndex> {
        let root = self.manifest.operation_index_root.as_ref()?;
        let index = self.read_operation_index(root).ok()?;
        index
            .covers(&self.manifest.operation_roots)
            .then_some(index)
    }

    /// The active profile's maximum uncompressed operation-block size
    /// (Chapter 8 §"Operation Envelope Blocks"). The *active* profile is the one
    /// the selected superblock names — a bundle opened under `Lite` must read
    /// under `Lite`'s limits, not the canonical-first profile's. Falls back to
    /// the canonical-first declaration, then the 64 MiB default.
    fn max_block_size(&self) -> u64 {
        self.active_profile()
            .or_else(|| self.manifest.canonical_first_profile())
            .map(|p| p.constraints.max_uncompressed_block_size)
            .unwrap_or(block::MAX_BLOCK_DEFAULT)
    }

    /// The profile declaration the active superblock names, if the manifest
    /// declares it.
    fn active_profile(&self) -> Option<ProfileDeclaration> {
        self.manifest
            .profile_declarations
            .iter()
            .find(|p| p.profile_id == self.superblock.profile_id)
            .copied()
    }

    /// Verifies every canonical chunk reachable from the manifest — the operation
    /// blocks, the canonical base (its root chunk, with the snapshot's restated
    /// hash cross-checked against the root), and every declared blob — is present
    /// and intact. The crash-recovery and cold-open paths call this to honor the
    /// spec rule that failed verification of a *canonical* chunk is hard
    /// corruption. (The bundle cannot tell *which* blobs are canonical without
    /// interpreting operations, so it conservatively verifies all of them.)
    pub fn verify_canonical_chunks(&self) -> Result<(), BundleError> {
        for r in &self.manifest.operation_roots {
            read_and_verify_chunk(&self.store, r)?;
        }
        if let Some(base) = &self.manifest.canonical_base {
            read_and_verify_chunk(&self.store, &base.root)?;
            if base.hash != base.root.hash {
                return Err(BundleError::ChunkHashMismatch {
                    expected: base.root.hash,
                    actual: base.hash,
                });
            }
        }
        for b in &self.manifest.blob_roots {
            self.read_blob(b)?;
        }
        Ok(())
    }

    /// Commits a new generation via the seven-step atomic write protocol.
    ///
    /// `new_chunks` are written (and flushed) first; the `build` closure then
    /// assembles the new manifest from the resulting [`ChunkRef`]s and the
    /// previous manifest; the manifest is written (and flushed); finally a new
    /// superblock at `generation + 1` is written to the inactive slot and
    /// flushed — the commit point. On success the in-memory state advances to
    /// the new generation.
    ///
    /// On a store error *before* the commit-point flush, the active superblock is
    /// untouched, so the bundle is unchanged on disk except for unreachable
    /// appended bytes. On an error *at* the commit-point flush the durable result
    /// is indeterminate (the new superblock may or may not have landed); the
    /// bundle is poisoned read-only and the caller must reopen from storage.
    pub fn commit(
        &mut self,
        new_chunks: &[StagedChunk],
        build: impl FnOnce(&CommitContext) -> Manifest,
    ) -> Result<(), BundleError> {
        if self.read_only {
            return Err(BundleError::ReadOnly);
        }

        let next_generation = self
            .superblock
            .generation
            .checked_add(1)
            .ok_or(BundleError::GenerationExhausted)?;
        let inactive = self.active_slot.other();
        let mut cursor = self.write_cursor;

        // Step 1: write all new chunks outside the prelude and reachable chunks
        // (we always append at EOF, so the "MUSTNOT overwrite a reachable chunk"
        // rule holds by construction). Content-addressed dedup: a staged chunk
        // whose content hash already exists — referenced by the current manifest
        // or written earlier in this same commit — reuses that storage instead
        // of re-appending (Chapter 8: duplicate content shares storage).
        let mut known: BTreeMap<ChunkId, ChunkRef> = self
            .manifest
            .referenced_chunk_refs()
            .into_iter()
            .map(|r| (r.id, r))
            .collect();
        // Index blobs too (keyed by their bare content hash), so re-staging a
        // payload already retained only as a `BlobRef` reuses its storage.
        for b in &self.manifest.blob_roots {
            known.entry(ChunkId(b.hash)).or_insert(ChunkRef {
                id: ChunkId(b.hash),
                kind: ChunkKind::Blob,
                schema_version: SchemaVersion::V0,
                offset: b.offset,
                compressed_length: b.compressed_length,
                uncompressed_length: b.uncompressed_length,
                compression: b.compression,
                hash: b.hash,
            });
        }
        let mut new_refs = Vec::with_capacity(new_chunks.len());
        for staged in new_chunks {
            let id = chunk_id(staged.kind, staged.schema_version, &staged.payload);
            let r = if let Some(existing) = known.get(&id) {
                *existing
            } else {
                let r = append_chunk(
                    &mut self.store,
                    &mut cursor,
                    staged.kind,
                    staged.schema_version,
                    &staged.payload,
                )?;
                known.insert(id, r);
                r
            };
            new_refs.push(r);
        }
        // Step 2: durable flush.
        self.store.flush()?;

        // Build the new manifest from the previous one and the new chunk refs.
        let previous = self.manifest.clone();
        let mut manifest = build(&CommitContext {
            previous_manifest: &previous,
            new_chunks: &new_refs,
            generation: next_generation,
        });

        // Extension-root preservation (Chapter 8 §"Behavior Under Unknown
        // Extensions"). The bundle's job is preservation: an extension-unaware
        // commit closure must not silently drop unknown extensions and their
        // `preserved_chunk_roots` (which would orphan those chunks). Carry
        // forward every prior extension declaration the closure did not itself
        // re-declare; an extension-*aware* writer that re-declares its own
        // `extension_id` keeps full control of that declaration. (The manifest
        // encoder sorts/dedups `extension_declarations`, so append order does not
        // affect the canonical form.)
        let redeclared: std::collections::BTreeSet<crate::ids::ExtensionId> = manifest
            .extension_declarations
            .iter()
            .map(|e| e.extension_id)
            .collect();
        for prior in &previous.extension_declarations {
            if !redeclared.contains(&prior.extension_id) {
                manifest.extension_declarations.push(prior.clone());
            }
        }

        manifest.generation = next_generation;
        manifest.manifest_id = manifest.derive_id();

        // The new manifest must be emittable (else `open` would reject it): at
        // least one declared profile.
        let active_profile = validate_emittable_manifest(&manifest)?;

        // Before publishing, validate that every canonical root the new manifest
        // declares actually resolves to a present, hash-intact chunk of the right
        // kind and shape (the new chunks are written and flushed; retained roots
        // are already in the body). This refuses a builder closure that produced
        // dangling, wrong-kind, or mismatched roots — an abort here leaves the
        // active superblock untouched, so the bundle stays at the old generation.
        validate_canonical_roots(&self.store, &manifest, &active_profile)?;

        // Step 3: write the new (uncompressed) manifest chunk. Enforce the
        // reader's manifest-size limit before publishing (else the new slot would
        // reopen as oversize), and normalize the in-memory copy to the canonical
        // form by decoding the bytes we produce — so `bundle.manifest()` matches
        // a reopen (e.g. duplicate roots are already collapsed).
        let manifest_payload = manifest.encode();
        enforce_limit(manifest_payload.len() as u64, MAX_MANIFEST_BYTES)?;
        let manifest = Manifest::decode(&manifest_payload)?;
        let manifest_hash =
            chunk_content_hash(ChunkKind::Manifest, Manifest::SCHEMA, &manifest_payload);
        let manifest_offset = cursor;
        self.store.write_at(manifest_offset, &manifest_payload)?;
        cursor += manifest_payload.len() as u64;
        // Step 4: durable flush.
        self.store.flush()?;

        // Step 5: compute the new superblock.
        let superblock = Superblock {
            generation: next_generation,
            manifest_offset,
            manifest_length: manifest_payload.len() as u64,
            manifest_hash,
            manifest_schema_version: Manifest::SCHEMA,
            reduction_algorithm_version: reduction_version_for(&manifest),
            profile_id: active_profile.profile_id,
            commit_state: CommitState::Committed,
            commit_timestamp: WallClockTime(0),
        };
        // Step 6: write it to the currently-inactive slot.
        self.store
            .write_at(inactive.offset(), &superblock.encode())?;
        // Step 7: durable flush — the commit point. An error here is
        // *indeterminate*: the new superblock may or may not have reached durable
        // storage, so the on-disk active generation is now unknown and the
        // in-memory state cannot be trusted. Poison the bundle (read-only) so no
        // further commit runs against stale state and risks overwriting the slot
        // that may have just become active; the caller must reopen from storage.
        if let Err(e) = self.store.flush() {
            self.read_only = true;
            return Err(e.into());
        }

        // The commit is durable; advance the in-memory state. If the new manifest
        // introduces a required unknown extension or a non-editable active
        // profile, the bundle becomes read-only immediately (not only on the next
        // open).
        self.active_slot = inactive;
        self.superblock = superblock;
        self.read_only =
            manifest_forces_read_only(&manifest) || !profile_is_editable(&active_profile);
        // A commit that publishes a canonical operation root beyond this reader's
        // accept-set (a forward-compat write) makes the *live* bundle read-only
        // at once — mirroring the open-time scan — so no further commit runs
        // against canonical history this reader can no longer parse.
        if let Some(major) = unsupported_operation_root_major(&manifest) {
            self.read_only = true;
            self.anomalies
                .push(IntegrityAnomaly::UnsupportedCanonicalChunkMajor {
                    schema_major: major,
                });
        }
        self.manifest = manifest;
        self.write_cursor = cursor;
        Ok(())
    }
}

impl Bundle<MemStore> {
    /// The current in-memory bundle image (only available for the in-memory
    /// store). Useful for snapshotting a bundle's bytes between commits.
    pub fn image(&self) -> &[u8] {
        self.store.as_bytes()
    }
}

/// Whether a manifest is well-formed enough to *emit* — a writer must not emit a
/// manifest its own `open` would reject. Chapter 8 §"Format Profiles" requires at
/// least one declared profile, and §"Canonical Document Identity" requires a
/// canonical base's profile to be one the manifest declares.
fn validate_emittable_manifest(manifest: &Manifest) -> Result<ProfileDeclaration, BundleError> {
    if manifest.profile_declarations.is_empty() {
        return Err(BundleError::Decode(DecodeError::Malformed(
            "manifest declares no conformance profile",
        )));
    }
    if !profile_ids_distinct(manifest) {
        return Err(BundleError::Decode(DecodeError::Malformed(
            "manifest declares the same profile id more than once",
        )));
    }
    // The active profile this manifest would be emitted under must be one this
    // implementation *understands* (can interpret and honor). It need not be
    // editable: a bundle may legitimately be produced under `ReadOnly` (it just
    // opens read-only). This refuses emitting only under an unknown/`Custom`,
    // wrong-major, or oversize-block profile — there is no understood profile to
    // operate under.
    let active_profile = active_profile_for_emit(manifest).ok_or(BundleError::Decode(
        DecodeError::Malformed("no declared profile is supported by this implementation"),
    ))?;
    if let Some(base) = &manifest.canonical_base {
        if !manifest
            .profile_declarations
            .iter()
            .any(|p| p.profile_id == base.profile_id)
        {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "canonical base profile is not declared by the manifest",
            )));
        }
    }
    Ok(active_profile)
}

/// Whether a manifest forces read-only mode: it declares a `required` extension
/// this implementation does not understand (v0 understands none, so any
/// `required` extension qualifies — Chapter 8 §"Behavior Under Unknown
/// Extensions").
fn manifest_forces_read_only(manifest: &Manifest) -> bool {
    manifest.extension_declarations.iter().any(|e| e.required)
}

/// The schema major of a declared **canonical operation root** that is beyond
/// this reader's accept-set for the op-block role, if any (Binary Format
/// companion §"Schema Major 1", "Canonical chunks — parse or open read-only").
/// Such a root forces read-only preservation: this reader cannot parse the newer
/// op bytes, so it must not author against op history it cannot interpret. Both
/// `open` (at load) and `commit` (a builder that publishes such a root, e.g. a
/// forward-compat write) consult this so the in-memory bundle goes read-only
/// immediately, not only on the next reopen.
fn unsupported_operation_root_major(manifest: &Manifest) -> Option<u16> {
    manifest
        .operation_roots
        .iter()
        .map(|r| r.schema_version.major)
        .find(|&m| m > max_supported_major(ChunkKind::OperationEnvelopeBlock))
}

/// Whether this implementation *understands* a profile (can interpret and honor
/// its constraints): a built-in `ProfileId` (not a `Custom` registry profile),
/// a supported major version, and a block bound within the reader's hard chunk
/// limit (a profile demanding larger blocks than the reader can allocate is not
/// honorable — Chapter 8 §"Operation Envelope Blocks" / §"Format Profiles").
fn profile_is_understood(decl: &ProfileDeclaration) -> bool {
    // A profile major change is non-backward-compatible (like a schema major), so
    // the major must match exactly; minor/patch are accepted.
    matches!(
        decl.profile_id,
        ProfileId::Full | ProfileId::ReadOnly | ProfileId::Lite
    ) && decl.version.major == SUPPORTED_PROFILE_MAJOR
        && decl.constraints.max_uncompressed_block_size <= MAX_CHUNK_BYTES
}

/// Whether a profile is *editable*: understood, and not the `ReadOnly` profile.
/// A `ReadOnly`-profile bundle opens read-only; v0 does not auto-upgrade it to a
/// writable profile (the spec's SHOULD-upgrade-on-edit is deferred).
fn profile_is_editable(decl: &ProfileDeclaration) -> bool {
    profile_is_understood(decl) && matches!(decl.profile_id, ProfileId::Full | ProfileId::Lite)
}

/// Whether a manifest's profile declarations have distinct `ProfileId`s. A
/// duplicate id (even at a different version/constraints) is ambiguous about
/// which declaration governs and is rejected.
fn profile_ids_distinct(manifest: &Manifest) -> bool {
    let mut ids: Vec<ProfileId> = manifest
        .profile_declarations
        .iter()
        .map(|p| p.profile_id)
        .collect();
    ids.sort();
    let len = ids.len();
    ids.dedup();
    ids.len() == len
}

/// The profile a freshly emitted superblock should name as active: prefer the
/// canonical-first *editable* profile, so the bundle is editable whenever it
/// declares an editable profile (e.g. `[ReadOnly, Lite]` is emitted under
/// `Lite`); otherwise the canonical-first merely *understood* profile (e.g. a
/// sole `ReadOnly` yields a read-only bundle). `None` if no declared profile is
/// understood — `validate_emittable_manifest` rejects that before this is used.
fn active_profile_for_emit(manifest: &Manifest) -> Option<ProfileDeclaration> {
    let profiles = manifest.canonical_profiles();
    profiles
        .iter()
        .find(|p| profile_is_editable(p))
        .or_else(|| profiles.iter().find(|p| profile_is_understood(p)))
        .copied()
}

/// The reduction-algorithm version a superblock should carry: the canonical
/// base's, if a base is present (only a base records a reduction); otherwise the
/// default (no base means no reduced base state at this generation).
fn reduction_version_for(manifest: &Manifest) -> ReductionAlgorithmVersion {
    manifest
        .canonical_base
        .as_ref()
        .map(|b| b.reduction_algorithm_version)
        .unwrap_or_default()
}

/// Reads and fully verifies a chunk against its reference — the shared core of
/// [`Bundle::read_chunk`] and the commit-time canonical-root validation. Checks
/// compression support (decompressing zstd payloads), body-placement,
/// schema-major support, declared length, the content hash, and the
/// `id == hash` redundancy (Chapter 8 §"Chunks").
fn read_and_verify_chunk(store: &dyn BlockStore, r: &ChunkRef) -> Result<Vec<u8>, BundleError> {
    read_and_verify_chunk_impl(store, r, true)
}

/// [`read_and_verify_chunk`] with the schema-major **accept-set** check made
/// optional. The accept-set is a *reader-capability* gate, not a structural
/// property: a bundle may legitimately carry a canonical root written by a
/// newer writer at a major beyond this reader's accept-set. Commit-time
/// canonical-root validation therefore checks a root's *structure* (resolves,
/// hash-intact, right kind, decodes) with `enforce_accept_set = false` — it must
/// not refuse to publish a root it merely cannot itself parse — while every read
/// path enforces the accept-set (a beyond-accept-set canonical root instead
/// opens the bundle read-only at `open`).
fn read_and_verify_chunk_impl(
    store: &dyn BlockStore,
    r: &ChunkRef,
    enforce_accept_set: bool,
) -> Result<Vec<u8>, BundleError> {
    // The manifest chunk is mandatorily uncompressed in this format version
    // (Chapter 8 §"Manifest Encoding"): a compressed manifest reference is
    // rejected outright, before any bytes are read.
    if r.kind == ChunkKind::Manifest && r.compression != CompressionAlgorithm::None {
        return Err(BundleError::CompressedManifest);
    }
    if let CompressionAlgorithm::Reserved(_) = r.compression {
        return Err(BundleError::UnsupportedCompression);
    }
    // A chunk reference must point into the body, never the fixed prelude.
    if r.offset < BODY_START {
        return Err(BundleError::ChunkOutOfBounds {
            offset: r.offset,
            length: r.compressed_length,
            file_len: store.len(),
        });
    }
    // A chunk at a schema major this reader cannot parse for its role. The
    // accept-set is `[0, max_supported_major(kind)]`: the op-block role admits
    // major 1 (D2), every other role stays exact-0 until its versioned path
    // lands (Binary Format companion §"Schema Major 1"). A chunk above its
    // role's max reaches this only on a direct read — a canonical root beyond
    // the accept-set opens the bundle read-only at `open` instead, before any
    // such read (see the operation-root scan there). Commit-time structural
    // validation skips this gate (a newer writer's higher-major root is still
    // structurally valid).
    if enforce_accept_set && r.schema_version.major > max_supported_major(r.kind) {
        return Err(BundleError::UnsupportedSchemaVersion {
            version: r.schema_version,
        });
    }
    // Bound both allocations by the reader's policy before touching a length.
    enforce_limit(r.compressed_length, MAX_CHUNK_BYTES)?;
    enforce_limit(r.uncompressed_length, MAX_CHUNK_BYTES)?;
    let stored = read_chunk_bytes(store, r.offset, r.compressed_length)?;
    let payload = decode_stored_payload(stored, r.compression, r.uncompressed_length)?;
    let actual = content_hash_for(r.kind, r.schema_version, &payload);
    if actual != r.hash {
        return Err(BundleError::ChunkHashMismatch {
            expected: r.hash,
            actual,
        });
    }
    if r.id.content_hash() != r.hash {
        return Err(BundleError::ChunkHashMismatch {
            expected: r.hash,
            actual: r.id.content_hash(),
        });
    }
    Ok(payload)
}

/// Reads and fully verifies a blob against its reference (the shared core of
/// [`Bundle::read_blob`] and the commit-time blob validation): compression
/// support, the reader resource limit (`min(max_bytes, declared_max)`),
/// body-placement, declared length, and the bare-`MUSCBLOB` content hash with
/// the `blob_id == hash` redundancy.
fn read_and_verify_blob(
    store: &dyn BlockStore,
    b: &BlobRef,
    max_bytes: u64,
) -> Result<Vec<u8>, BundleError> {
    if let CompressionAlgorithm::Reserved(_) = b.compression {
        return Err(BundleError::UnsupportedCompression);
    }
    let limit = b
        .declared_max_uncompressed_length
        .unwrap_or(u64::MAX)
        .min(max_bytes);
    // Chapter 8 §"Blobs": the declared uncompressed length is checked against
    // the reader's policy *before decompression begins* (and before any
    // allocation keyed on it).
    enforce_limit(b.uncompressed_length, limit)?;
    enforce_limit(b.compressed_length, max_bytes)?;
    if b.offset < BODY_START {
        return Err(BundleError::ChunkOutOfBounds {
            offset: b.offset,
            length: b.compressed_length,
            file_len: store.len(),
        });
    }
    let stored = read_chunk_bytes(store, b.offset, b.compressed_length)?;
    let payload = decode_stored_payload(stored, b.compression, b.uncompressed_length)?;
    let actual = BlobId::of_payload(&payload).0;
    if actual != b.hash || b.blob_id.0 != b.hash {
        return Err(BundleError::ChunkHashMismatch {
            expected: b.hash,
            actual,
        });
    }
    Ok(payload)
}

/// Errors if `length` exceeds `limit`, before any allocation keyed on it.
fn enforce_limit(length: u64, limit: u64) -> Result<(), BundleError> {
    if length > limit {
        Err(BundleError::ResourceLimitExceeded { length, limit })
    } else {
        Ok(())
    }
}

/// Recovers a chunk's uncompressed payload from its stored (possibly
/// compressed) bytes, verifying it is *exactly* `declared_len` bytes long
/// (Chapter 8 §"Compression" / §"Chunks": a decompressed size that disagrees
/// with the declared `uncompressed_length` is corruption). The caller has
/// already validated `declared_len` against its resource-limit policy, so
/// every allocation here is bounded. Content hashing happens strictly *after*
/// this step, over the uncompressed bytes — compression is `ChunkRef`
/// metadata, never part of content identity.
fn decode_stored_payload(
    stored: Vec<u8>,
    compression: CompressionAlgorithm,
    declared_len: u64,
) -> Result<Vec<u8>, BundleError> {
    match compression {
        CompressionAlgorithm::None => {
            if stored.len() as u64 != declared_len {
                return Err(BundleError::ChunkLengthMismatch {
                    expected: declared_len,
                    actual: stored.len() as u64,
                });
            }
            Ok(stored)
        }
        // Reading zstd at any level is a conformance MUST (Chapter 8
        // §"Compression"); the declared level byte is advisory metadata the
        // decoder does not need.
        CompressionAlgorithm::Zstd { .. } => decompress_zstd(&stored, declared_len),
        CompressionAlgorithm::Reserved(_) => Err(BundleError::UnsupportedCompression),
    }
}

/// Decompresses a zstd frame sequence into a buffer sized *exactly* by the
/// declared uncompressed length, so a hostile stream can never allocate past
/// the (already limit-checked) declaration:
///
/// * a stream that would exceed `declared_len` hits libzstd's
///   destination-full error → [`BundleError::Decompression`];
/// * a stream that ends short of `declared_len` →
///   [`BundleError::ChunkLengthMismatch`];
/// * a truncated or otherwise malformed stream (including trailing garbage
///   after the final frame) → [`BundleError::Decompression`].
///
/// No path panics or allocates beyond `declared_len` plus libzstd's own
/// bounded decoding context (whose window is capped internally).
fn decompress_zstd(stored: &[u8], declared_len: u64) -> Result<Vec<u8>, BundleError> {
    let declared = usize::try_from(declared_len).map_err(|_| {
        // Unreachable on 64-bit targets; on narrower ones an unaddressable
        // declaration is a resource-limit refusal, not a wrap.
        BundleError::ResourceLimitExceeded {
            length: declared_len,
            limit: usize::MAX as u64,
        }
    })?;
    let mut payload = vec![0u8; declared];
    match zstd::bulk::decompress_to_buffer(stored, payload.as_mut_slice()) {
        Ok(n) if n as u64 == declared_len => Ok(payload),
        Ok(n) => Err(BundleError::ChunkLengthMismatch {
            expected: declared_len,
            actual: n as u64,
        }),
        Err(e) => Err(BundleError::Decompression(e)),
    }
}

/// Validates that every canonical root a manifest declares resolves to a
/// present, hash-intact chunk of the *right kind and shape* before a commit
/// publishes it — so a bundle that opens normally can never carry a dangling,
/// mis-roled, or malformed canonical root:
///
/// * each operation root is an `OperationEnvelopeBlock`, within the active
///   profile's maximum block size, whose payload actually decodes;
/// * the canonical base's root is a `Snapshot`, with its restated hash matching;
/// * every declared blob root resolves and verifies.
fn validate_canonical_roots(
    store: &dyn BlockStore,
    manifest: &Manifest,
    active_profile: &ProfileDeclaration,
) -> Result<(), BundleError> {
    let max_block = active_profile.constraints.max_uncompressed_block_size;

    for r in &manifest.operation_roots {
        if r.kind != ChunkKind::OperationEnvelopeBlock {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "operation root is not an operation-envelope block",
            )));
        }
        if r.uncompressed_length > max_block {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "operation block exceeds the active profile's maximum size",
            )));
        }
        // Structural verification only (`enforce_accept_set = false`): a commit
        // may publish an op block at a major beyond this reader's accept-set (a
        // newer writer's v1+ block). The accept-set is enforced on read, and a
        // beyond-accept-set canonical root opens the bundle read-only at `open`.
        let payload = read_and_verify_chunk_impl(store, r, false)?;
        // The block payload must be a well-formed envelope sequence.
        block::decode_block(&payload).map_err(BundleError::Decode)?;
    }
    if let Some(base) = &manifest.canonical_base {
        if base.root.kind != ChunkKind::Snapshot {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "canonical base root is not a snapshot chunk",
            )));
        }
        read_and_verify_chunk(store, &base.root)?;
        if base.hash != base.root.hash {
            return Err(BundleError::ChunkHashMismatch {
                expected: base.root.hash,
                actual: base.hash,
            });
        }
    }
    for b in &manifest.blob_roots {
        if !crate::manifest::valid_media_type(&b.media_type) {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "blob media type is not a valid RFC 6838 type/subtype",
            )));
        }
        read_and_verify_blob(store, b, MAX_BLOB_BYTES)?;
    }
    Ok(())
}

/// Reads the manifest payload a superblock points at, enforcing that it lies in
/// the body — Chapter 8 fixes the prelude layout, so a "manifest" overlapping a
/// header or superblock slot is foreign — and within the reader's manifest size
/// limit, before allocating.
///
/// The manifest chunk is mandatorily *uncompressed* in this format version
/// (Chapter 8 §"Manifest Encoding"): the superblock deliberately carries no
/// compression field, so the stored bytes ARE the payload. A bundle whose
/// manifest bytes are compressed anyway therefore fails downstream as
/// malformed: hash verification (over the uncompressed preimage) rejects the
/// slot, and even a colluding hash-over-compressed-bytes cannot survive
/// `Manifest::decode`.
fn read_manifest_payload(store: &dyn BlockStore, sb: &Superblock) -> Result<Vec<u8>, BundleError> {
    if sb.manifest_offset < BODY_START {
        return Err(BundleError::ChunkOutOfBounds {
            offset: sb.manifest_offset,
            length: sb.manifest_length,
            file_len: store.len(),
        });
    }
    enforce_limit(sb.manifest_length, MAX_MANIFEST_BYTES)?;
    read_chunk_bytes(store, sb.manifest_offset, sb.manifest_length)
}

/// Reads `length` bytes at `offset`, bounds-checking against the store size and
/// reporting an out-of-range reference rather than an opaque I/O error.
fn read_chunk_bytes(
    store: &dyn BlockStore,
    offset: u64,
    length: u64,
) -> Result<Vec<u8>, BundleError> {
    let end = offset.checked_add(length);
    match end {
        Some(end) if end <= store.len() => Ok(read_vec(store, offset, length)?),
        _ => Err(BundleError::ChunkOutOfBounds {
            offset,
            length,
            file_len: store.len(),
        }),
    }
}

/// Writes a chunk payload at `*cursor`, advances the cursor, and returns the
/// resulting reference. v0 writes uncompressed, so compressed and uncompressed
/// lengths are equal.
fn append_chunk(
    store: &mut dyn BlockStore,
    cursor: &mut u64,
    kind: ChunkKind,
    schema: SchemaVersion,
    payload: &[u8],
) -> Result<ChunkRef, BundleError> {
    let id = chunk_id(kind, schema, payload);
    let offset = *cursor;
    store.write_at(offset, payload)?;
    *cursor += payload.len() as u64;
    Ok(ChunkRef {
        id,
        kind,
        schema_version: schema,
        offset,
        compressed_length: payload.len() as u64,
        uncompressed_length: payload.len() as u64,
        compression: CompressionAlgorithm::None,
        hash: id.content_hash(),
    })
}

/// Parses one slot for ordinary selection and, if it is a valid committed
/// superblock, verifies its manifest chunk's hash against the store. Returns the
/// superblock only if every check passes; records a non-committed slot as an
/// anomaly.
fn verified_slot(
    store: &dyn BlockStore,
    slot_bytes: &[u8],
    anomalies: &mut Vec<IntegrityAnomaly>,
) -> Option<Superblock> {
    match Superblock::parse_slot(slot_bytes) {
        SlotParse::Valid(sb) => {
            // Verify the manifest chunk's hash (Chapter 8 §"Superblock
            // Selection", step 2): a slot whose manifest is out of the body,
            // oversize, or does not hash-verify is not valid for ordinary
            // selection.
            match read_manifest_payload(store, &sb) {
                Ok(payload) => {
                    let actual = chunk_content_hash(
                        ChunkKind::Manifest,
                        sb.manifest_schema_version,
                        &payload,
                    );
                    if actual == sb.manifest_hash {
                        Some(sb)
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        }
        SlotParse::Rejected(SlotReject::NotCommitted) => {
            anomalies.push(IntegrityAnomaly::NonCommittedSlot);
            None
        }
        SlotParse::Rejected(_) => None,
    }
}

// Re-exported helper for harnesses that build raw images: the body start offset
// and the content hash of a manifest payload.
/// The content hash of a manifest chunk payload (Chapter 8 §"Content Hashing").
pub fn manifest_chunk_hash(payload: &[u8]) -> ContentHash {
    chunk_content_hash(ChunkKind::Manifest, Manifest::SCHEMA, payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::DocumentId;

    #[test]
    fn schema_major_1_admission_is_raised_per_role_op_blocks_only() {
        // Admission of major 1 is raised **per chunk role**, never as a blanket
        // accept-set (Binary Format companion §"Schema Major 1"). D2 raised the
        // operation-envelope-block role to major 1 (a block bearing a v1
        // CreateRegion), and schema major 2 to major 2 (a block bearing a v2
        // cross-cutting/staff/metadata value); every other role stays exact-0
        // until its own versioned path lands, and the manifest stays major 0
        // forever.
        assert_eq!(SchemaVersion::V1.major, 1);
        assert_eq!(SchemaVersion::V2.major, 2);
        // The op-block role admits [0, 2].
        assert_eq!(max_supported_major(ChunkKind::OperationEnvelopeBlock), 2);
        // Every other role stays at the generic baseline (major 0): the
        // payload-polymorphic Snapshot (its migrate-on-read is core-side), the
        // layout cache, and the operation index.
        assert_eq!(SUPPORTED_SCHEMA_MAJOR, 0);
        assert_eq!(max_supported_major(ChunkKind::Snapshot), 0);
        assert_eq!(max_supported_major(ChunkKind::LayoutCache), 0);
        assert_eq!(max_supported_major(ChunkKind::OperationIndex), 0);
        // The manifest gate is exact to the manifest's own major (0), independent
        // of the per-role chunk accept-set.
        assert_eq!(Manifest::SCHEMA.major, 0);
    }

    #[test]
    fn committing_an_unsupported_major_op_root_makes_the_live_bundle_read_only() {
        // A commit publishes an op block beyond this reader's accept-set (a
        // forward-compat write): structural validation lets it through, but the
        // LIVE bundle must go read-only at once — not only on the next reopen —
        // so no further commit runs against canonical history it cannot parse.
        let mut bundle = fresh_bundle();
        let block = StagedChunk::operation_block_versioned(
            crate::block::encode_block(&[vec![1u8, 2, 3]]),
            SchemaVersion::new(3, 0),
        );
        bundle
            .commit(&[block], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots.push(ctx.new_chunks[0]);
                m
            })
            .expect("a structurally-valid future-major root is publishable");

        assert!(
            bundle.is_read_only(),
            "the live bundle is read-only immediately after the commit"
        );
        assert!(bundle.anomalies().iter().any(|a| matches!(
            a,
            IntegrityAnomaly::UnsupportedCanonicalChunkMajor { schema_major: 3 }
        )));
        // A further commit against the now-read-only bundle is refused.
        let more = StagedChunk::operation_block_versioned(
            crate::block::encode_block(&[vec![9u8]]),
            SchemaVersion::V0,
        );
        assert!(matches!(
            bundle.commit(&[more], |ctx| ctx.previous_manifest.clone()),
            Err(BundleError::ReadOnly)
        ));
    }

    fn fresh_bundle() -> Bundle<MemStore> {
        Bundle::create(
            MemStore::new(),
            FileUuid([1; 16]),
            Manifest::empty(DocumentId([2; 16])),
        )
        .unwrap()
    }

    #[test]
    fn create_then_open_round_trips() {
        let bundle = fresh_bundle();
        let image = bundle.into_store().into_bytes();
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert_eq!(reopened.generation(), 0);
        assert_eq!(reopened.active_slot(), Slot::A);
        assert_eq!(reopened.file_uuid(), FileUuid([1; 16]));
        assert!(!reopened.is_read_only());
        assert!(reopened.anomalies().is_empty());
    }

    #[test]
    fn commit_advances_generation_and_flips_slot() {
        let mut bundle = fresh_bundle();
        let block = block::encode_block(&[b"env-1".to_vec(), b"env-2".to_vec()]);
        bundle
            .commit(&[StagedChunk::operation_block(block)], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots = ctx.new_chunks.to_vec();
                m
            })
            .unwrap();
        assert_eq!(bundle.generation(), 1);
        assert_eq!(bundle.active_slot(), Slot::B);

        // Reopen from the image; the committed state is visible and verifies.
        let image = bundle.into_store().into_bytes();
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert_eq!(reopened.generation(), 1);
        assert_eq!(reopened.manifest().operation_roots.len(), 1);
        reopened.verify_canonical_chunks().unwrap();
        let envs = reopened
            .read_operation_block(&reopened.manifest().operation_roots[0])
            .unwrap();
        assert_eq!(envs, vec![b"env-1".to_vec(), b"env-2".to_vec()]);
    }

    #[test]
    fn successive_commits_alternate_slots() {
        let mut bundle = fresh_bundle();
        for i in 1..=5u64 {
            let payload = block::encode_block(&[vec![i as u8; 16]]);
            bundle
                .commit(&[StagedChunk::operation_block(payload)], |ctx| {
                    let mut m = ctx.previous_manifest.clone();
                    m.operation_roots.extend(ctx.new_chunks.iter().copied());
                    m
                })
                .unwrap();
            assert_eq!(bundle.generation(), i);
            assert_eq!(
                bundle.active_slot(),
                if i % 2 == 1 { Slot::B } else { Slot::A }
            );
        }
        assert_eq!(bundle.manifest().operation_roots.len(), 5);
    }

    #[test]
    fn a_corrupt_canonical_chunk_is_hard_corruption() {
        let mut bundle = fresh_bundle();
        let payload = block::encode_block(&[b"env".to_vec()]);
        bundle
            .commit(&[StagedChunk::operation_block(payload)], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots = ctx.new_chunks.to_vec();
                m
            })
            .unwrap();
        let op_offset = bundle.manifest().operation_roots[0].offset;
        let mut image = bundle.into_store().into_bytes();
        image[op_offset as usize] ^= 0xFF; // corrupt the operation block payload

        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        // Opening still works (the manifest/superblock are intact)...
        assert_eq!(reopened.generation(), 1);
        // ...but verifying the canonical chunk surfaces hard corruption.
        assert!(matches!(
            reopened.verify_canonical_chunks(),
            Err(BundleError::ChunkHashMismatch { .. })
        ));
    }

    #[test]
    fn commit_is_refused_on_a_read_only_bundle() {
        let mut bundle = fresh_bundle();
        bundle.read_only = true;
        let err = bundle.commit(&[], |ctx| ctx.previous_manifest.clone());
        assert!(matches!(err, Err(BundleError::ReadOnly)));
    }

    fn op_block(bundle: &mut Bundle<MemStore>, envelopes: &[&[u8]]) {
        let payload =
            block::encode_block(&envelopes.iter().map(|e| e.to_vec()).collect::<Vec<_>>());
        bundle
            .commit(&[StagedChunk::operation_block(payload)], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots.extend(ctx.new_chunks.iter().copied());
                m
            })
            .unwrap();
    }

    #[test]
    fn commit_rejects_a_dangling_canonical_root() {
        // Finding 2: a builder closure that publishes a root pointing at a chunk
        // that does not exist must be refused; the bundle stays at generation 0.
        let mut bundle = fresh_bundle();
        let bogus = ChunkRef {
            id: ChunkId(ContentHash([9; 32])),
            kind: ChunkKind::OperationEnvelopeBlock,
            schema_version: SchemaVersion::V0,
            offset: 1_000_000,
            compressed_length: 10,
            uncompressed_length: 10,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([9; 32]),
        };
        let err = bundle.commit(&[], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            m.operation_roots = vec![bogus];
            m
        });
        assert!(matches!(err, Err(BundleError::ChunkOutOfBounds { .. })));
        assert_eq!(bundle.generation(), 0);
    }

    #[test]
    fn commit_rejects_a_root_with_a_mismatched_hash() {
        // Finding 2: a root referencing a real chunk but with the wrong declared
        // hash is rejected before commit.
        let mut bundle = fresh_bundle();
        op_block(&mut bundle, &[b"real"]);
        let mut tampered = bundle.manifest().operation_roots[0];
        tampered.hash = ContentHash([0; 32]);
        let err = bundle.commit(&[], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            m.operation_roots = vec![tampered];
            m
        });
        assert!(matches!(err, Err(BundleError::ChunkHashMismatch { .. })));
    }

    #[test]
    fn create_rejects_initial_canonical_roots() {
        // Finding 2: create() writes no chunks, so any canonical root would be
        // dangling; reject it.
        let mut m = Manifest::empty(DocumentId([5; 16]));
        m.operation_roots = vec![ChunkRef {
            id: ChunkId(ContentHash([1; 32])),
            kind: ChunkKind::OperationEnvelopeBlock,
            schema_version: SchemaVersion::V0,
            offset: BODY_START,
            compressed_length: 1,
            uncompressed_length: 1,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([1; 32]),
        }];
        assert!(Bundle::create(MemStore::new(), FileUuid([1; 16]), m).is_err());
    }

    #[test]
    fn forged_chunk_id_is_rejected_on_read() {
        // Finding 12: a reference whose id disagrees with its (correct) hash
        // fails verification.
        let mut bundle = fresh_bundle();
        op_block(&mut bundle, &[b"env"]);
        let mut forged = bundle.manifest().operation_roots[0];
        forged.id = ChunkId(ContentHash([0xAB; 32])); // hash still correct
        assert!(matches!(
            bundle.read_chunk(&forged),
            Err(BundleError::ChunkHashMismatch { .. })
        ));
    }

    #[test]
    fn manifest_id_is_synced_after_create_and_commit() {
        // Finding 13: the in-memory manifest id matches the derived id, with no
        // reopen required.
        let mut bundle = fresh_bundle();
        assert_eq!(bundle.manifest().manifest_id, bundle.manifest().derive_id());
        op_block(&mut bundle, &[b"env"]);
        assert_eq!(bundle.manifest().manifest_id, bundle.manifest().derive_id());
    }

    fn commit_blob(bundle: &mut Bundle<MemStore>, payload: &[u8]) {
        let staged = StagedChunk {
            kind: ChunkKind::Blob,
            schema_version: SchemaVersion::V0,
            payload: payload.to_vec(),
        };
        bundle
            .commit(&[staged], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                let r = ctx.new_chunks[0];
                m.blob_roots.push(BlobRef {
                    blob_id: BlobId(r.hash),
                    media_type: "application/octet-stream".to_string(),
                    offset: r.offset,
                    compressed_length: r.compressed_length,
                    uncompressed_length: r.uncompressed_length,
                    compression: CompressionAlgorithm::None,
                    hash: r.hash,
                    declared_max_uncompressed_length: None,
                });
                m
            })
            .unwrap();
    }

    #[test]
    fn staged_blob_reads_back() {
        // Finding 5: a staged Blob chunk hashes the way it is verified, so it
        // round-trips through read_blob (wired into blob_roots, not the
        // operation roots — which would now be rejected for the wrong kind).
        let mut bundle = fresh_bundle();
        let payload = b"blob-bytes".to_vec();
        commit_blob(&mut bundle, &payload);
        let b = bundle.manifest().blob_roots[0].clone();
        assert_eq!(bundle.read_blob(&b).unwrap(), payload);
    }

    #[test]
    fn duplicate_content_is_deduplicated() {
        // Committing the same payload twice reuses the existing chunk's storage
        // (storage dedup) and collapses to a single canonical root (manifest
        // dedup), with the in-memory manifest already normalized.
        let mut bundle = fresh_bundle();
        op_block(&mut bundle, &[b"same-content"]);
        let cursor_before = bundle.write_cursor;
        let manifest_len_before = bundle.superblock().manifest_length;
        op_block(&mut bundle, &[b"same-content"]); // build extends roots to [r, r]

        // Manifest dedup: the duplicate operation root collapsed to one, in
        // memory (not only after a reopen).
        assert_eq!(bundle.manifest().operation_roots.len(), 1);

        // Storage dedup: the only body growth was the new manifest chunk, not a
        // second copy of the block.
        let grew = bundle.write_cursor - cursor_before;
        assert!(
            grew <= manifest_len_before + 256,
            "duplicate content appears to have been re-stored (body grew by {grew})"
        );

        // And a reopen agrees with the in-memory (already normalized) manifest.
        let image = bundle.into_store().into_bytes();
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert_eq!(reopened.manifest().operation_roots.len(), 1);
    }

    #[test]
    fn required_extension_forces_read_only() {
        // Finding 3: a declared required extension (unknown in v0) opens the
        // bundle read-only.
        let mut bundle = fresh_bundle();
        bundle
            .commit(&[], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.extension_declarations
                    .push(crate::manifest::ExtensionDeclaration {
                        extension_id: crate::ids::ExtensionId([3; 16]),
                        version: crate::ids::SemVer::new(1, 0, 0),
                        required: true,
                        preserved_chunk_roots: Vec::new(),
                        affected_object_kinds: Vec::new(),
                        edit_barriers: Vec::new(),
                    });
                m
            })
            .unwrap();
        let image = bundle.into_store().into_bytes();
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert!(reopened.is_read_only());
        assert!(reopened
            .anomalies()
            .contains(&IntegrityAnomaly::UnknownRequiredExtension));
    }

    #[test]
    fn commit_preserves_unknown_extension_roots_when_the_closure_drops_them() {
        // The bundle's job is preservation: an extension-*unaware* writer that
        // rebuilds the manifest from scratch must not orphan an unknown
        // (optional) extension's preserved roots.
        let mut bundle = fresh_bundle();
        let ext_id = crate::ids::ExtensionId([7; 16]);
        let ext_root = ChunkRef {
            id: ChunkId(ContentHash([42; 32])),
            kind: ChunkKind::ExtensionData,
            schema_version: SchemaVersion::V0,
            offset: 4096,
            compressed_length: 8,
            uncompressed_length: 8,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([42; 32]),
        };

        // 1) An extension-aware commit declares the optional extension + a root.
        bundle
            .commit(&[], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.extension_declarations
                    .push(crate::manifest::ExtensionDeclaration {
                        extension_id: ext_id,
                        version: crate::ids::SemVer::new(1, 0, 0),
                        required: false,
                        preserved_chunk_roots: vec![ext_root],
                        affected_object_kinds: Vec::new(),
                        edit_barriers: Vec::new(),
                    });
                m
            })
            .unwrap();
        assert!(bundle
            .manifest()
            .extension_declarations
            .iter()
            .any(|e| e.extension_id == ext_id));

        // 2) An extension-unaware commit rebuilds the manifest from empty,
        //    carrying only what it understands (operation roots). The bundle must
        //    still carry the extension and its root forward.
        let doc = bundle.manifest().document_id;
        bundle
            .commit(&[], |ctx| {
                let mut m = Manifest::empty(doc);
                m.operation_roots = ctx.previous_manifest.operation_roots.clone();
                m
            })
            .unwrap();

        let survives = |m: &Manifest| {
            m.extension_declarations.iter().any(|e| {
                e.extension_id == ext_id
                    && e.preserved_chunk_roots.iter().any(|r| r.id == ext_root.id)
            })
        };
        assert!(
            survives(bundle.manifest()),
            "unknown extension + its root must survive an extension-unaware commit"
        );

        // 3) And it survives a reopen (durably preserved).
        let image = bundle.into_store().into_bytes();
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert!(survives(reopened.manifest()));
    }

    #[test]
    fn profile_selection_is_stable_across_reload() {
        // Finding 9: the superblock's profile_id is the canonical-first profile,
        // so a [Lite, Full] manifest does not flip its active profile on reload.
        use crate::manifest::{ProfileConstraints, ProfileDeclaration};
        let mut m = Manifest::empty(DocumentId([6; 16]));
        m.profile_declarations = vec![
            ProfileDeclaration {
                profile_id: ProfileId::Lite,
                version: crate::ids::SemVer::new(0, 1, 0),
                constraints: ProfileConstraints::DEFAULT_FULL,
            },
            ProfileDeclaration {
                profile_id: ProfileId::Full,
                version: crate::ids::SemVer::new(0, 1, 0),
                constraints: ProfileConstraints::DEFAULT_FULL,
            },
        ];
        let bundle = Bundle::create(MemStore::new(), FileUuid([1; 16]), m).unwrap();
        let in_memory_profile = bundle.superblock().profile_id;
        let image = bundle.into_store().into_bytes();
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert_eq!(in_memory_profile, reopened.superblock().profile_id);
    }

    #[test]
    fn corrupt_canonical_blob_is_surfaced() {
        // Finding 4: a corrupt blob referenced by the manifest is caught by
        // verify_canonical_chunks.
        let mut bundle = fresh_bundle();
        commit_blob(&mut bundle, b"audio-bytes");
        let blob_offset = bundle.manifest().blob_roots[0].offset;
        bundle.verify_canonical_chunks().unwrap(); // intact
        let mut image = bundle.into_store().into_bytes();
        image[blob_offset as usize] ^= 0xFF;
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert!(matches!(
            reopened.verify_canonical_chunks(),
            Err(BundleError::ChunkHashMismatch { .. })
        ));
    }

    #[test]
    fn empty_profile_list_is_rejected_at_create_and_commit() {
        // Finding 1: a writer must not emit a manifest its own open would reject.
        let mut m = Manifest::empty(DocumentId([8; 16]));
        m.profile_declarations.clear();
        assert!(Bundle::create(MemStore::new(), FileUuid([1; 16]), m).is_err());

        let mut bundle = fresh_bundle();
        let err = bundle.commit(&[], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            m.profile_declarations.clear();
            m
        });
        assert!(err.is_err());
        assert_eq!(bundle.generation(), 0);
    }

    #[test]
    fn commit_rejects_a_dangling_blob_root() {
        // Finding 3: a blob root pointing nowhere is refused before commit.
        let mut bundle = fresh_bundle();
        let err = bundle.commit(&[], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            m.blob_roots.push(BlobRef {
                blob_id: BlobId(ContentHash([4; 32])),
                media_type: "x/y".to_string(),
                offset: 9_000_000,
                compressed_length: 4,
                uncompressed_length: 4,
                compression: CompressionAlgorithm::None,
                hash: ContentHash([4; 32]),
                declared_max_uncompressed_length: None,
            });
            m
        });
        assert!(err.is_err());
        assert_eq!(bundle.generation(), 0);
    }

    #[test]
    fn commit_rejects_wrong_kind_operation_root() {
        // Finding 2: an operation root must be an operation-envelope block.
        let mut bundle = fresh_bundle();
        let staged = StagedChunk {
            kind: ChunkKind::LayoutCache,
            schema_version: SchemaVersion::V0,
            payload: b"not a block".to_vec(),
        };
        let err = bundle.commit(&[staged], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            m.operation_roots = ctx.new_chunks.to_vec();
            m
        });
        assert!(matches!(err, Err(BundleError::Decode(_))));
    }

    #[test]
    fn live_bundle_becomes_read_only_after_committing_a_required_extension() {
        // Finding 4: read-only takes effect on the live object, not only on
        // reopen.
        let mut bundle = fresh_bundle();
        bundle
            .commit(&[], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.extension_declarations
                    .push(crate::manifest::ExtensionDeclaration {
                        extension_id: crate::ids::ExtensionId([3; 16]),
                        version: crate::ids::SemVer::new(1, 0, 0),
                        required: true,
                        preserved_chunk_roots: Vec::new(),
                        affected_object_kinds: Vec::new(),
                        edit_barriers: Vec::new(),
                    });
                m
            })
            .unwrap();
        assert!(bundle.is_read_only());
        assert!(matches!(
            bundle.commit(&[], |ctx| ctx.previous_manifest.clone()),
            Err(BundleError::ReadOnly)
        ));
    }

    #[test]
    fn indeterminate_final_flush_poisons_the_bundle() {
        // Finding 5: if the commit-point flush persists G+1 but returns an error,
        // the live bundle must not keep accepting commits against stale state.
        use crate::store::{CrashPoint, FaultStore, Tear};

        // Discover the commit's final-flush syscall index via a no-fault run.
        let base = fresh_bundle().into_store().into_bytes();
        let total = {
            let mut b = Bundle::open(FaultStore::no_fault(base.clone())).unwrap();
            b.commit(
                &[StagedChunk::operation_block(block::encode_block(&[
                    b"e".to_vec()
                ]))],
                |ctx| {
                    let mut m = ctx.previous_manifest.clone();
                    m.operation_roots.extend(ctx.new_chunks.iter().copied());
                    m
                },
            )
            .unwrap();
            b.into_store().syscalls_issued()
        };

        // Crash on the final flush but persist the whole superblock (full tear):
        // durable is G+1, yet commit returns Err.
        let crash = CrashPoint {
            after_syscalls: total - 1,
            tear: Tear::TornLastWrite { prefix: 256 },
        };
        let mut bundle = Bundle::open(FaultStore::new(base, crash)).unwrap();
        let result = bundle.commit(
            &[StagedChunk::operation_block(block::encode_block(&[
                b"e".to_vec()
            ]))],
            |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots.extend(ctx.new_chunks.iter().copied());
                m
            },
        );
        assert!(result.is_err(), "the final flush errored");
        // The bundle is poisoned: further commits are refused (the durable state
        // is indeterminate and must be reloaded).
        assert!(bundle.is_read_only());
    }

    #[test]
    fn create_with_required_extension_is_read_only_immediately() {
        // Finding 3: a freshly created bundle with a required extension is
        // read-only without a reopen.
        let mut m = Manifest::empty(DocumentId([9; 16]));
        m.extension_declarations
            .push(crate::manifest::ExtensionDeclaration {
                extension_id: crate::ids::ExtensionId([1; 16]),
                version: crate::ids::SemVer::new(1, 0, 0),
                required: true,
                preserved_chunk_roots: Vec::new(),
                affected_object_kinds: Vec::new(),
                edit_barriers: Vec::new(),
            });
        let bundle = Bundle::create(MemStore::new(), FileUuid([1; 16]), m).unwrap();
        assert!(bundle.is_read_only());
    }

    #[test]
    fn commit_rejects_a_canonical_base_with_an_undeclared_profile() {
        // Finding 1: a writer must not emit a canonical base whose profile its
        // own open would reject.
        let mut bundle = fresh_bundle(); // declares only Full
        let snap_payload = b"snapshot".to_vec();
        let staged = StagedChunk {
            kind: ChunkKind::Snapshot,
            schema_version: SchemaVersion::V0,
            payload: snap_payload,
        };
        let err = bundle.commit(&[staged], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            let root = ctx.new_chunks[0];
            m.canonical_base = Some(crate::manifest::SnapshotRef {
                snapshot_id: crate::ids::SnapshotId([1; 16]),
                covers_causal_frontier: crate::ids::FrontierBytes::empty(),
                reduction_algorithm_version: ReductionAlgorithmVersion(0),
                profile_id: ProfileId::Lite, // NOT declared by the manifest
                root,
                hash: root.hash,
            });
            m
        });
        assert!(matches!(err, Err(BundleError::Decode(_))));
        assert_eq!(bundle.generation(), 0);
    }

    #[test]
    fn oversize_manifest_is_refused_by_the_writer() {
        // Finding 2: a manifest exceeding the reader limit is rejected at
        // create/commit, not only on reopen.
        let mut m = Manifest::empty(DocumentId([2; 16]));
        m.extension_declarations
            .push(crate::manifest::ExtensionDeclaration {
                extension_id: crate::ids::ExtensionId([1; 16]),
                version: crate::ids::SemVer::new(1, 0, 0),
                required: false,
                preserved_chunk_roots: Vec::new(),
                affected_object_kinds: vec![0u8; (MAX_MANIFEST_BYTES + 1) as usize],
                edit_barriers: Vec::new(),
            });
        assert!(matches!(
            Bundle::create(MemStore::new(), FileUuid([1; 16]), m),
            Err(BundleError::ResourceLimitExceeded { .. })
        ));
    }

    #[test]
    fn block_size_limit_follows_the_active_superblock_profile() {
        // Finding 4: a bundle selected under a smaller profile reads blocks under
        // that profile's limit, not the canonical-first profile's.
        use crate::manifest::{ProfileConstraints, ProfileDeclaration, RetentionPolicy};
        let big = ProfileConstraints {
            max_uncompressed_block_size: 64 << 20,
            retention_policy: RetentionPolicy::DEFAULT_FULL,
        };
        let small = ProfileConstraints {
            max_uncompressed_block_size: 2048,
            retention_policy: RetentionPolicy::DEFAULT_FULL,
        };
        let mut m = Manifest::empty(DocumentId([3; 16]));
        m.profile_declarations = vec![
            ProfileDeclaration {
                profile_id: ProfileId::Full,
                version: crate::ids::SemVer::new(0, 1, 0),
                constraints: big,
            },
            ProfileDeclaration {
                profile_id: ProfileId::Lite,
                version: crate::ids::SemVer::new(0, 1, 0),
                constraints: small,
            },
        ];
        let mut bundle = Bundle::create(MemStore::new(), FileUuid([1; 16]), m).unwrap();
        // Canonical-first profile is Full (discriminant 0): active = Full limit.
        assert_eq!(bundle.superblock().profile_id, ProfileId::Full);
        assert_eq!(bundle.max_block_size(), 64 << 20);
        // Simulate a bundle selected under Lite (as a foreign writer might emit):
        // the active limit must follow the superblock, not canonical-first.
        bundle.superblock.profile_id = ProfileId::Lite;
        assert_eq!(bundle.max_block_size(), 2048);
    }

    fn profile(
        profile_id: ProfileId,
        version: crate::ids::SemVer,
        max_block: u64,
    ) -> ProfileDeclaration {
        ProfileDeclaration {
            profile_id,
            version,
            constraints: crate::manifest::ProfileConstraints {
                max_uncompressed_block_size: max_block,
                retention_policy: crate::manifest::RetentionPolicy::DEFAULT_FULL,
            },
        }
    }

    #[test]
    fn profile_support_is_classified_correctly() {
        // Findings 2/3/6: built-in editable, ReadOnly understood-but-not-editable,
        // Custom/future-major/oversize-block not understood.
        let v0 = crate::ids::SemVer::new(0, 1, 0);
        let full = profile(ProfileId::Full, v0, 1 << 20);
        assert!(profile_is_editable(&full));

        let ro = profile(ProfileId::ReadOnly, v0, 1 << 20);
        assert!(profile_is_understood(&ro) && !profile_is_editable(&ro));

        let custom = profile(
            ProfileId::Custom(crate::ids::ProfileRegistryId([1; 16])),
            v0,
            1 << 20,
        );
        assert!(!profile_is_understood(&custom));

        let future = profile(ProfileId::Full, crate::ids::SemVer::new(1, 0, 0), 1 << 20);
        assert!(!profile_is_understood(&future));

        let huge = profile(ProfileId::Full, v0, MAX_CHUNK_BYTES + 1);
        assert!(!profile_is_understood(&huge));
    }

    #[test]
    fn create_rejects_unsupported_or_duplicate_active_profiles() {
        let v0 = crate::ids::SemVer::new(0, 1, 0);
        let make = |decls: Vec<ProfileDeclaration>| {
            let mut m = Manifest::empty(DocumentId([1; 16]));
            m.profile_declarations = decls;
            Bundle::create(MemStore::new(), FileUuid([1; 16]), m)
        };
        // Custom-only (no understood profile to operate under).
        assert!(make(vec![profile(
            ProfileId::Custom(crate::ids::ProfileRegistryId([2; 16])),
            v0,
            1 << 20
        )])
        .is_err());
        // Block bound beyond the reader's hard limit (unsupported).
        assert!(make(vec![profile(ProfileId::Full, v0, MAX_CHUNK_BYTES + 1)]).is_err());
        // Duplicate profile id.
        assert!(make(vec![
            profile(ProfileId::Full, v0, 1 << 20),
            profile(ProfileId::Full, crate::ids::SemVer::new(0, 2, 0), 1 << 20),
        ])
        .is_err());
        // A plain Full profile is fine.
        assert!(make(vec![profile(ProfileId::Full, v0, 1 << 20)]).is_ok());
    }

    #[test]
    fn read_only_profile_is_emittable_as_a_read_only_bundle() {
        // Finding: a sole ReadOnly profile is a *valid* bundle to produce — it
        // just opens read-only (the spec describes ReadOnly-produced bundles).
        let v0 = crate::ids::SemVer::new(0, 1, 0);
        let mut m = Manifest::empty(DocumentId([1; 16]));
        m.profile_declarations = vec![profile(ProfileId::ReadOnly, v0, 1 << 20)];
        let bundle = Bundle::create(MemStore::new(), FileUuid([1; 16]), m).unwrap();
        assert_eq!(bundle.superblock().profile_id, ProfileId::ReadOnly);
        assert!(bundle.is_read_only());
        // Round-trips: a reopen agrees it is read-only.
        let image = bundle.into_store().into_bytes();
        assert!(Bundle::open(MemStore::from_bytes(image))
            .unwrap()
            .is_read_only());
    }

    #[test]
    fn editable_profile_is_preferred_for_the_active_superblock() {
        // [ReadOnly, Lite]: ReadOnly sorts first, but the bundle is emitted under
        // the editable Lite profile, so it is editable.
        let v0 = crate::ids::SemVer::new(0, 1, 0);
        let mut m = Manifest::empty(DocumentId([1; 16]));
        m.profile_declarations = vec![
            profile(ProfileId::ReadOnly, v0, 1 << 20),
            profile(ProfileId::Lite, v0, 1 << 20),
        ];
        let bundle = Bundle::create(MemStore::new(), FileUuid([1; 16]), m).unwrap();
        assert_eq!(bundle.superblock().profile_id, ProfileId::Lite);
        assert!(!bundle.is_read_only());
    }

    #[test]
    fn commit_validates_roots_under_the_profile_it_emits() {
        let make_bundle = |read_only_max, lite_max| {
            let v0 = crate::ids::SemVer::new(0, 1, 0);
            let mut m = Manifest::empty(DocumentId([1; 16]));
            m.profile_declarations = vec![
                profile(ProfileId::ReadOnly, v0, read_only_max),
                profile(ProfileId::Lite, v0, lite_max),
            ];
            Bundle::create(MemStore::new(), FileUuid([1; 16]), m).unwrap()
        };
        let staged = || StagedChunk::operation_block(block::encode_block(&[vec![7; 16]]));
        let append_root = |ctx: &CommitContext| {
            let mut m = ctx.previous_manifest.clone();
            m.operation_roots.push(ctx.new_chunks[0]);
            m
        };

        // ReadOnly sorts first, but Lite is the emitted active profile. Its
        // smaller bound must reject this 24-byte operation block.
        let mut strict_lite = make_bundle(1024, 8);
        assert_eq!(strict_lite.superblock().profile_id, ProfileId::Lite);
        assert!(strict_lite.commit(&[staged()], append_root).is_err());
        assert_eq!(strict_lite.generation(), 0);

        // Conversely, the selected Lite profile's larger bound must admit the
        // block even though canonical-first ReadOnly has a smaller bound.
        let mut permissive_lite = make_bundle(8, 1024);
        permissive_lite.commit(&[staged()], append_root).unwrap();
        let root = permissive_lite.manifest().operation_roots[0];
        assert!(permissive_lite.read_operation_block(&root).is_ok());
        let reopened = Bundle::open(MemStore::from_bytes(
            permissive_lite.into_store().into_bytes(),
        ))
        .unwrap();
        assert!(reopened.read_operation_block(&root).is_ok());
    }

    /// Builds a minimal valid bundle image at `generation`, with the given
    /// profile declared and named by the superblock.
    fn craft_image(generation: u64, profile_id: ProfileId) -> Vec<u8> {
        let mut m = Manifest::empty(DocumentId([1; 16]));
        m.generation = generation;
        // Always declare Full (a distinct editable profile); add the named active
        // profile only when it is something other than Full, to avoid a duplicate.
        let mut decls = vec![ProfileDeclaration::full()];
        if profile_id != ProfileId::Full {
            decls.push(profile(
                profile_id,
                crate::ids::SemVer::new(0, 1, 0),
                1 << 20,
            ));
        }
        m.profile_declarations = decls;
        let payload = m.encode();
        let mut image = vec![0u8; BODY_START as usize];
        image.extend_from_slice(&payload);
        let sb = Superblock {
            generation,
            manifest_offset: BODY_START,
            manifest_length: payload.len() as u64,
            manifest_hash: manifest_chunk_hash(&payload),
            manifest_schema_version: SchemaVersion::V0,
            reduction_algorithm_version: ReductionAlgorithmVersion(0),
            profile_id,
            commit_state: CommitState::Committed,
            commit_timestamp: WallClockTime(0),
        };
        image[0..crate::header::HEADER_LEN as usize]
            .copy_from_slice(&FixedHeader::new(FileUuid([1; 16])).encode());
        image[SLOT_A_OFFSET as usize..SLOT_A_OFFSET as usize + SUPERBLOCK_LEN as usize]
            .copy_from_slice(&sb.encode());
        image
    }

    #[test]
    fn read_only_profile_opens_read_only() {
        // Finding 6: a bundle whose active profile is ReadOnly opens read-only
        // (v0 does not auto-upgrade it).
        let image = craft_image(0, ProfileId::ReadOnly);
        let bundle = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert!(bundle.is_read_only());
        assert!(bundle.anomalies().is_empty()); // a normal read-only bundle
    }

    #[test]
    fn unsupported_custom_profile_opens_read_only_with_anomaly() {
        // Finding 2: a Custom (registry-defined) active profile is unsupported in
        // v0 — open read-only and surface it.
        let image = craft_image(0, ProfileId::Custom(crate::ids::ProfileRegistryId([9; 16])));
        let bundle = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert!(bundle.is_read_only());
        assert!(bundle
            .anomalies()
            .contains(&IntegrityAnomaly::UnsupportedProfile));
    }

    #[test]
    fn generation_exhaustion_is_an_error_not_a_panic() {
        // Finding 5: committing a generation-u64::MAX bundle returns an error.
        let image = craft_image(u64::MAX, ProfileId::Full);
        let mut bundle = Bundle::open(MemStore::from_bytes(image)).unwrap();
        assert_eq!(bundle.generation(), u64::MAX);
        assert!(matches!(
            bundle.commit(&[], |ctx| ctx.previous_manifest.clone()),
            Err(BundleError::GenerationExhausted)
        ));
    }

    // ------------------------------------------------------------------
    // Zstd read support (Chapter 8 §"Compression": reading zstd-compressed
    // chunks is a conformance MUST; this crate's writer still emits only
    // uncompressed chunks, so tests plant externally-compressed bytes).
    // ------------------------------------------------------------------

    /// Appends externally-zstd-compressed bytes to the bundle body (as a
    /// foreign compressing writer would have) and returns the reference
    /// describing them. `declared_len` lets a test lie about the uncompressed
    /// length; honest callers pass `payload.len()`.
    fn plant_zstd_chunk(
        bundle: &mut Bundle<MemStore>,
        kind: ChunkKind,
        payload: &[u8],
        declared_len: u64,
    ) -> ChunkRef {
        let compressed = zstd::bulk::compress(payload, 3).unwrap();
        let offset = bundle.write_cursor;
        bundle.store.write_at(offset, &compressed).unwrap();
        bundle.write_cursor += compressed.len() as u64;
        let hash = content_hash_for(kind, SchemaVersion::V0, payload);
        ChunkRef {
            id: ChunkId(hash),
            kind,
            schema_version: SchemaVersion::V0,
            offset,
            compressed_length: compressed.len() as u64,
            uncompressed_length: declared_len,
            compression: CompressionAlgorithm::Zstd { level: 3 },
            hash,
        }
    }

    /// Like [`plant_zstd_chunk`], but for a blob (bare `MUSCBLOB` addressing).
    fn plant_zstd_blob(
        bundle: &mut Bundle<MemStore>,
        payload: &[u8],
        declared_len: u64,
    ) -> BlobRef {
        let r = plant_zstd_chunk(bundle, ChunkKind::Blob, payload, declared_len);
        BlobRef {
            blob_id: BlobId(r.hash),
            media_type: "application/octet-stream".to_string(),
            offset: r.offset,
            compressed_length: r.compressed_length,
            uncompressed_length: declared_len,
            compression: r.compression,
            hash: r.hash,
            declared_max_uncompressed_length: None,
        }
    }

    #[test]
    fn zstd_compressed_chunk_round_trips_with_hash_verified() {
        // §Compression round-trip: an externally-compressed chunk reads back
        // byte-identical, and the content hash is verified over the
        // *uncompressed* bytes (the preimage rule: compression is metadata).
        let mut bundle = fresh_bundle();
        let payload: Vec<u8> = b"layout-cache-bytes ".repeat(64); // compressible
        let r = plant_zstd_chunk(
            &mut bundle,
            ChunkKind::LayoutCache,
            &payload,
            payload.len() as u64,
        );
        assert!(
            r.compressed_length < r.uncompressed_length,
            "fixture actually compressed"
        );
        assert_eq!(bundle.read_chunk(&r).unwrap(), payload);

        // The hash check runs on the decompressed payload: a tampered declared
        // hash is caught even though the stored (compressed) bytes are intact.
        let mut tampered = r;
        tampered.hash = ContentHash([0; 32]);
        tampered.id = ChunkId(ContentHash([0; 32])); // keep id == hash
        assert!(matches!(
            bundle.read_chunk(&tampered),
            Err(BundleError::ChunkHashMismatch { .. })
        ));
    }

    #[test]
    fn compressed_operation_root_commits_and_reopens() {
        // End to end: a compressed operation block can be published as a
        // canonical root (commit-time validation decompresses + verifies it),
        // survives a reopen, and streams back through read_operation_block.
        let mut bundle = fresh_bundle();
        let envelopes = vec![b"env-1".to_vec(), b"env-2".to_vec()];
        let payload = block::encode_block(&envelopes);
        let root = plant_zstd_chunk(
            &mut bundle,
            ChunkKind::OperationEnvelopeBlock,
            &payload,
            payload.len() as u64,
        );
        bundle
            .commit(&[], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots.push(root);
                m
            })
            .unwrap();

        let image = bundle.into_store().into_bytes();
        let reopened = Bundle::open(MemStore::from_bytes(image)).unwrap();
        reopened.verify_canonical_chunks().unwrap();
        let stored_root = reopened.manifest().operation_roots[0];
        assert_eq!(
            stored_root.compression,
            CompressionAlgorithm::Zstd { level: 3 }
        );
        assert_eq!(
            reopened.read_operation_block(&stored_root).unwrap(),
            envelopes
        );
    }

    #[test]
    fn zstd_stream_ending_short_of_declared_length_is_rejected() {
        // §Compression: "Decompression MUST verify the output length against
        // the declared uncompressed_length" — a stream that ends short of the
        // declaration is corruption, reported with both lengths.
        let mut bundle = fresh_bundle();
        let payload = b"short-stream".to_vec();
        let r = plant_zstd_chunk(
            &mut bundle,
            ChunkKind::LayoutCache,
            &payload,
            payload.len() as u64 + 5, // declares more than the stream yields
        );
        assert!(matches!(
            bundle.read_chunk(&r),
            Err(BundleError::ChunkLengthMismatch {
                expected: 17,
                actual: 12,
            })
        ));
    }

    #[test]
    fn zstd_stream_exceeding_declared_length_is_rejected() {
        // The dual failure: a stream that would decompress *past* the declared
        // length must be refused without allocating beyond the declaration
        // (the output buffer is sized by the declared length, so libzstd hits
        // destination-full and errors).
        let mut bundle = fresh_bundle();
        let payload: Vec<u8> = b"overlong ".repeat(32);
        let r = plant_zstd_chunk(
            &mut bundle,
            ChunkKind::LayoutCache,
            &payload,
            payload.len() as u64 - 1, // declares less than the stream yields
        );
        assert!(matches!(
            bundle.read_chunk(&r),
            Err(BundleError::Decompression(_))
        ));
    }

    #[test]
    fn corrupt_or_truncated_zstd_stream_is_a_typed_error() {
        let mut bundle = fresh_bundle();
        let payload: Vec<u8> = b"to-be-corrupted ".repeat(16);
        let r = plant_zstd_chunk(
            &mut bundle,
            ChunkKind::LayoutCache,
            &payload,
            payload.len() as u64,
        );

        // Corrupt the frame header magic in the stored bytes: malformed stream.
        bundle.store.write_at(r.offset, &[0xFF]).unwrap();
        assert!(matches!(
            bundle.read_chunk(&r),
            Err(BundleError::Decompression(_))
        ));

        // Truncated stream: same compressed bytes, but the reference claims
        // fewer of them than the frame needs.
        let mut bundle = fresh_bundle();
        let mut truncated = plant_zstd_chunk(
            &mut bundle,
            ChunkKind::LayoutCache,
            &payload,
            payload.len() as u64,
        );
        truncated.compressed_length /= 2;
        assert!(matches!(
            bundle.read_chunk(&truncated),
            Err(BundleError::Decompression(_))
        ));
    }

    #[test]
    fn zstd_compressed_blob_round_trips_and_fails_typed() {
        // The blob path shares the decode: round-trip, short-stream, corrupt.
        let mut bundle = fresh_bundle();
        let payload: Vec<u8> = b"blob-audio-bytes ".repeat(64);
        let b = plant_zstd_blob(&mut bundle, &payload, payload.len() as u64);
        assert_eq!(bundle.read_blob(&b).unwrap(), payload);

        // Declared length beyond the stream's yield → typed error.
        let mut short = b.clone();
        short.uncompressed_length = payload.len() as u64 + 3;
        assert!(matches!(
            bundle.read_blob(&short),
            Err(BundleError::ChunkLengthMismatch { .. })
        ));

        // The declared_max cap still applies *before* decompression begins.
        let mut capped = b.clone();
        capped.declared_max_uncompressed_length = Some(4);
        assert!(matches!(
            bundle.read_blob(&capped),
            Err(BundleError::ResourceLimitExceeded { .. })
        ));

        // Corrupt stored stream → typed error, no panic.
        bundle.store.write_at(b.offset, &[0xFF]).unwrap();
        assert!(matches!(
            bundle.read_blob(&b),
            Err(BundleError::Decompression(_))
        ));
    }

    #[test]
    fn reserved_compression_is_still_unsupported() {
        // §Compression: Reserved algorithms belong to future format majors.
        let mut bundle = fresh_bundle();
        op_block(&mut bundle, &[b"env"]);
        let mut r = bundle.manifest().operation_roots[0];
        r.compression = CompressionAlgorithm::Reserved(7);
        assert!(matches!(
            bundle.read_chunk(&r),
            Err(BundleError::UnsupportedCompression)
        ));

        commit_blob(&mut bundle, b"blob");
        let mut b = bundle.manifest().blob_roots[0].clone();
        b.compression = CompressionAlgorithm::Reserved(7);
        assert!(matches!(
            bundle.read_blob(&b),
            Err(BundleError::UnsupportedCompression)
        ));
    }

    #[test]
    fn compressed_manifest_chunk_ref_is_rejected() {
        // §Manifest Encoding: the manifest chunk MUST be stored uncompressed in
        // this format version. A manifest reference declaring compression is
        // refused outright — even when the compressed bytes are a perfectly
        // valid zstd stream of a perfectly valid manifest.
        let mut bundle = fresh_bundle();
        let manifest_payload = Manifest::empty(DocumentId([7; 16])).encode();
        let r = plant_zstd_chunk(
            &mut bundle,
            ChunkKind::Manifest,
            &manifest_payload,
            manifest_payload.len() as u64,
        );
        assert!(matches!(
            bundle.read_chunk(&r),
            Err(BundleError::CompressedManifest)
        ));
    }

    /// A minimal image whose superblock points at `stored` as the manifest
    /// payload, declaring `manifest_hash` for it.
    fn craft_image_with_manifest_bytes(stored: &[u8], manifest_hash: ContentHash) -> Vec<u8> {
        let mut image = vec![0u8; BODY_START as usize];
        image.extend_from_slice(stored);
        let sb = Superblock {
            generation: 0,
            manifest_offset: BODY_START,
            manifest_length: stored.len() as u64,
            manifest_hash,
            manifest_schema_version: SchemaVersion::V0,
            reduction_algorithm_version: ReductionAlgorithmVersion(0),
            profile_id: ProfileId::Full,
            commit_state: CommitState::Committed,
            commit_timestamp: WallClockTime(0),
        };
        image[0..crate::header::HEADER_LEN as usize]
            .copy_from_slice(&FixedHeader::new(FileUuid([1; 16])).encode());
        image[SLOT_A_OFFSET as usize..SLOT_A_OFFSET as usize + SUPERBLOCK_LEN as usize]
            .copy_from_slice(&sb.encode());
        image
    }

    #[test]
    fn open_rejects_a_bundle_whose_manifest_bytes_are_compressed() {
        // §Manifest Encoding: "Implementations MUST reject as malformed any
        // bundle whose manifest payload is not directly parseable as a
        // canonical manifest chunk's uncompressed bytes." The superblock
        // carries no compression field, so the stored bytes are treated as the
        // payload — a compressed manifest fails with a typed error either way
        // a hostile writer declares its hash.
        let payload = Manifest::empty(DocumentId([1; 16])).encode();
        let compressed = zstd::bulk::compress(&payload, 3).unwrap();

        // (a) Hash declared over the true (uncompressed) manifest content:
        //     the stored bytes fail hash verification → no valid superblock.
        let image = craft_image_with_manifest_bytes(&compressed, manifest_chunk_hash(&payload));
        assert!(matches!(
            Bundle::open(MemStore::from_bytes(image)),
            Err(BundleError::NoValidSuperblock)
        ));

        // (b) Colluding hash over the compressed bytes: the slot verifies, but
        //     the payload is not parseable as a manifest → rejected as
        //     malformed.
        let image = craft_image_with_manifest_bytes(&compressed, manifest_chunk_hash(&compressed));
        assert!(matches!(
            Bundle::open(MemStore::from_bytes(image)),
            Err(BundleError::Decode(_))
        ));
    }
}
