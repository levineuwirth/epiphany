#![forbid(unsafe_code)]
//! # epiphany-bundle
//!
//! The Epiphany `.musc` file format: the on-disk container for a score,
//! implementing the normative requirements of **Chapter 8 (File Format)** of
//! the core specification. This is Agent D's crate per `spec/QUICKSTART.md`. It
//! depends on [`epiphany_determinism`] (Agent A) and on **nothing else** — in
//! particular not on `epiphany-core` (Agent B) or `epiphany-ops` (Agent C):
//! *"bundles handle bytes, ops handles semantics."* Operation envelopes,
//! snapshots, and causal frontiers are opaque to the bundle; it stores,
//! addresses, and integrity-checks them without interpreting them.
//!
//! ## The architecture in one paragraph
//!
//! A bundle is a single file: a fixed 64-byte [`FixedHeader`] at offset 0, two
//! 256-byte [`Superblock`] slots, then a body of immutable, content-addressed
//! [chunks](ChunkRef). The superblocks are the only mutable on-disk objects. A
//! commit appends new chunks, writes a new [`Manifest`] chunk, then flips the
//! active superblock by writing the inactive slot and durably flushing it — that
//! flush is the commit point ([`Bundle::commit`]). Recovery is just: read both
//! slots, validate (magic, CRC, commit state, manifest hash), select the highest
//! valid generation ([`Bundle::open`]). Because commits only ever *append* and
//! touch the *inactive* slot, a crash can never corrupt the active state; the
//! bundle opens at the previous generation, or — only after the final flush — at
//! the new one. That is the property the crash-recovery [`fuzz`]er proves, and
//! it is the most important single test in the prototype (QUICKSTART, Agent D).
//!
//! ## Determinism
//!
//! Content identity is the BLAKE3-256 of a *domain-separated preimage* over the
//! uncompressed payload, kind, schema version, and length
//! ([`chunk_content_hash`]) — never the compressed bytes, so identical content
//! deduplicates across compression choices (Appendix D §"Compression and File
//! Bytes"). The manifest's reference vectors are put into the Appendix D
//! canonical order at encode time, so serialize → load → re-serialize is
//! byte-identical (v0 acceptance criterion 4).
//!
//! ## Scope (per QUICKSTART "Don't do these")
//!
//! v0 writes only uncompressed chunks (compression on write is deferred), but
//! *reads* zstd-compressed chunks and blobs, as the spec's §Compression MUST
//! requires. It does not implement the text-projection *content* (only carries
//! its root), and does not *evaluate* edit barriers (their operand types live
//! in other crates). The manifest is mandatory-uncompressed in this format
//! version regardless, and a compressed manifest is rejected. See `DECISIONS.md`
//! for the prototype byte-layout choices that anticipate the deferred Binary
//! Format companion, and the batched Pass 11 candidates.

mod block;
mod bundle;
mod chunk;
mod codec;
mod crc;
mod error;
mod header;
mod ids;
mod manifest;
mod opindex;
mod store;
mod superblock;

pub mod fuzz;
pub mod vectors;

pub use block::{
    decode_block, encode_block, envelope_offsets, pack_operation_blocks, BLOCK_SOFT_LIMIT,
    MAX_BLOCK_DEFAULT,
};
pub use bundle::{
    manifest_chunk_hash, Bundle, CommitContext, StagedChunk, BODY_START, MAX_BLOB_BYTES,
    MAX_CHUNK_BYTES, MAX_MANIFEST_BYTES, SUPPORTED_SCHEMA_MAJOR,
};
pub use chunk::{
    chunk_content_hash, chunk_id, content_hash_for, ChunkKind, ChunkRef, CompressionAlgorithm,
};
pub use codec::{DecodeError, Reader, Writer};
pub use crc::crc32c;
// Re-exported from the determinism crate: these content-address newtypes appear
// in this crate's public types (`ChunkRef`, `BlobId`, `SnapshotRef`), so callers
// need them without depending on `epiphany-determinism` directly.
pub use epiphany_determinism::{ChunkId, ContentHash};
pub use error::{BundleError, IntegrityAnomaly};
pub use header::{
    FixedHeader, FORMAT_MAJOR, FORMAT_MINOR, HEADER_LEN, SLOT_A_OFFSET, SLOT_B_OFFSET,
};
pub use ids::{
    BlobId, DocumentId, ExtensionId, FileUuid, FrontierBytes, LineageId, ManifestId,
    ProfileRegistryId, ReductionAlgorithmVersion, SchemaVersion, SemVer, SnapshotId,
    WallClockDuration, WallClockTime,
};
pub use manifest::{
    BlobRef, ExtensionDeclaration, Manifest, OperationBlockSummary, ProfileConstraints,
    ProfileDeclaration, RetentionPolicy, SnapshotRef,
};
pub use opindex::{
    IndexedBlock, OperationIdBytes, OperationIndex, OperationIndexBuildError, OperationIndexEntry,
};
pub use store::{BlockStore, CrashPoint, FaultStore, MemStore, Tear};
pub use superblock::{
    select_active, CommitState, ProfileId, Selection, Slot, SlotParse, SlotReject, Superblock,
    SUPERBLOCK_LEN,
};

#[cfg(unix)]
pub use store::FileStore;
