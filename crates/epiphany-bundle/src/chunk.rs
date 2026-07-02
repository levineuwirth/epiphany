//! Content-addressed chunks (Chapter 8 §"Chunks" and §"Content Hashing").
//!
//! Every on-disk object other than the fixed prelude is an immutable,
//! content-addressed chunk. A chunk's identity is the BLAKE3-256 of a
//! *domain-separated preimage* over its uncompressed payload, kind, schema
//! version, and length — never its compressed bytes, so identical content
//! deduplicates regardless of compression choice (Chapter 8
//! §"Domain-Separated Preimages"; Appendix D §"Compression and File Bytes").

use crate::codec::{DecodeError, Reader, Writer};
use crate::ids::SchemaVersion;
use epiphany_determinism::{ChunkId, ContentHash, DomainTag, Preimage};

/// The kind of a chunk; the reader uses it to dispatch parsing (Chapter 8).
/// A closed vocabulary — there is no `Registered` variant, so the discriminant
/// is a single stable byte.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ChunkKind {
    /// A block of operation envelopes (the canonical document).
    OperationEnvelopeBlock,
    /// An operation-id-to-block index accelerator.
    OperationIndex,
    /// A materialized snapshot of canonical state. Canonical iff referenced as
    /// the manifest's `canonical_base`; an acceleration cache otherwise.
    Snapshot,
    /// A large opaque blob (audio, image, font, ML model).
    Blob,
    /// Extension-defined data, preserved opaquely by core readers.
    ExtensionData,
    /// The text projection's root document.
    TextProjection,
    /// Layout cache: derivative, always discardable.
    LayoutCache,
    /// Integrity index: cross-references chunk hashes.
    IntegrityIndex,
    /// Manifest chunk. Special: referenced from the superblock, not a chunk.
    Manifest,
}

impl ChunkKind {
    /// The stable 1-byte discriminant, assigned by the spec's declaration order.
    /// Part of the hash preimage and of the chunk-reference canonical order, so
    /// the assignment is normative for this format version. RATIFIED by Pass 11
    /// (item 1.5, P11-D4): core_spec §"Chunks",
    /// Requirement `req:format:chunkkind-discriminants` (0..=8).
    #[inline]
    pub const fn discriminant(self) -> u8 {
        match self {
            ChunkKind::OperationEnvelopeBlock => 0,
            ChunkKind::OperationIndex => 1,
            ChunkKind::Snapshot => 2,
            ChunkKind::Blob => 3,
            ChunkKind::ExtensionData => 4,
            ChunkKind::TextProjection => 5,
            ChunkKind::LayoutCache => 6,
            ChunkKind::IntegrityIndex => 7,
            ChunkKind::Manifest => 8,
        }
    }

    /// Reconstructs a kind from its discriminant.
    #[inline]
    pub const fn from_discriminant(value: u8) -> Option<Self> {
        Some(match value {
            0 => ChunkKind::OperationEnvelopeBlock,
            1 => ChunkKind::OperationIndex,
            2 => ChunkKind::Snapshot,
            3 => ChunkKind::Blob,
            4 => ChunkKind::ExtensionData,
            5 => ChunkKind::TextProjection,
            6 => ChunkKind::LayoutCache,
            7 => ChunkKind::IntegrityIndex,
            8 => ChunkKind::Manifest,
            _ => return None,
        })
    }

    /// Canonical bytes for the hash preimage: the 1-byte discriminant.
    #[inline]
    pub fn canonical_bytes(self) -> [u8; 1] {
        [self.discriminant()]
    }

    /// The domain tag a chunk of this kind hashes under (Chapter 8
    /// §"Content Hashing" rationale): manifests use `MUSCMANI`; every other
    /// chunk kind uses `MUSCCHNK`. (Blobs are addressed separately, as a bare
    /// `MUSCBLOB || payload`; see [`crate::BlobId`].)
    #[inline]
    pub(crate) fn domain_tag(self) -> DomainTag {
        match self {
            ChunkKind::Manifest => DomainTag::MANIFEST,
            _ => DomainTag::CHUNK,
        }
    }

    #[inline]
    pub(crate) fn encode(self, w: &mut Writer) {
        w.put_u8(self.discriminant());
    }

    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let v = r.get_u8()?;
        ChunkKind::from_discriminant(v).ok_or(DecodeError::InvalidDiscriminant {
            what: "ChunkKind",
            value: v as u64,
        })
    }
}

/// Per-chunk compression metadata (Chapter 8 §"Compression"). **Not** part of
/// content identity. v0 *writes* only [`CompressionAlgorithm::None`] (the
/// QUICKSTART defers compression on the write path: *"Don't implement
/// compression in the bundle … add zstd later as a non-breaking minor
/// version"*), but *reading* zstd-compressed chunks is a conformance MUST and
/// is supported. The manifest chunk is mandatorily uncompressed either way
/// (Chapter 8 §"Manifest Encoding").
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CompressionAlgorithm {
    /// No compression. Stored bytes equal the uncompressed payload.
    None,
    /// Zstandard at the given level. Readers MUST support any level zstd
    /// defines; the level byte is writer metadata the decoder does not need.
    Zstd { level: u8 },
    /// Reserved for future format major versions.
    Reserved(u8),
}

impl CompressionAlgorithm {
    #[inline]
    pub(crate) fn encode(self, w: &mut Writer) {
        match self {
            CompressionAlgorithm::None => w.put_u8(0).put_u8(0),
            CompressionAlgorithm::Zstd { level } => w.put_u8(1).put_u8(level),
            CompressionAlgorithm::Reserved(v) => w.put_u8(2).put_u8(v),
        };
    }

    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let tag = r.get_u8()?;
        let param = r.get_u8()?;
        Ok(match tag {
            0 => CompressionAlgorithm::None,
            1 => CompressionAlgorithm::Zstd { level: param },
            2 => CompressionAlgorithm::Reserved(param),
            other => {
                return Err(DecodeError::InvalidDiscriminant {
                    what: "CompressionAlgorithm",
                    value: other as u64,
                })
            }
        })
    }
}

/// Computes the content hash of a non-blob chunk: the BLAKE3-256 of the
/// canonical domain-separated preimage
/// `domain || kind || schema || uncompressed_length(le) || payload`
/// (Chapter 8 §"Domain-Separated Preimages"). The compression algorithm is
/// deliberately *not* in the preimage. Blobs use [`crate::BlobId::of_payload`].
pub fn chunk_content_hash(kind: ChunkKind, schema: SchemaVersion, payload: &[u8]) -> ContentHash {
    let mut p = Preimage::new(kind.domain_tag());
    p.push_bytes(&kind.canonical_bytes());
    p.push_bytes(&schema.canonical_bytes());
    p.push_u64_le(payload.len() as u64);
    p.push_bytes(payload);
    p.finish()
}

/// The content hash a chunk of `kind` is *addressed by* — the single source of
/// truth shared by writing and verification, so a staged chunk always reads
/// back. Blobs use the bare `BLAKE3("MUSCBLOB" || payload)` form (Chapter 8
/// §"Blobs"; [`ContentHash::of_blob`]); every other kind uses the structured
/// chunk preimage ([`chunk_content_hash`]).
#[inline]
pub fn content_hash_for(kind: ChunkKind, schema: SchemaVersion, payload: &[u8]) -> ContentHash {
    match kind {
        ChunkKind::Blob => ContentHash::of_blob(payload),
        _ => chunk_content_hash(kind, schema, payload),
    }
}

/// Computes the [`ChunkId`] of a chunk (its content hash). Dispatches on `kind`
/// via [`content_hash_for`], so a `Blob` chunk's id is its `BlobId`.
#[inline]
pub fn chunk_id(kind: ChunkKind, schema: SchemaVersion, payload: &[u8]) -> ChunkId {
    ChunkId(content_hash_for(kind, schema, payload))
}

/// A reference to a stored chunk (Chapter 8 §"Chunks"). Locates the chunk in
/// the bundle body and carries enough metadata to verify it without re-reading
/// the manifest. The `id` and `hash` fields are both the content hash (the spec
/// keeps both: `id` is the lookup key, `hash` enables fast verification).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ChunkRef {
    /// Content identifier: BLAKE3 of the canonical preimage.
    pub id: ChunkId,
    /// Kind of chunk; the reader dispatches parsing on it.
    pub kind: ChunkKind,
    /// Schema version of this chunk's payload encoding.
    pub schema_version: SchemaVersion,
    /// Offset of the on-disk (compressed) payload in the bundle file.
    pub offset: u64,
    /// Length of the on-disk (compressed) payload.
    pub compressed_length: u64,
    /// Length of the uncompressed payload.
    pub uncompressed_length: u64,
    /// Compression algorithm (metadata, not identity).
    pub compression: CompressionAlgorithm,
    /// Restated content hash, redundant with `id` (Chapter 8).
    pub hash: ContentHash,
}

impl ChunkRef {
    /// The canonical sort key for chunk references (Appendix D §"Ordered
    /// Iteration": *"ascending by `ChunkKind` discriminant, then by content hash
    /// lexicographic, then by file offset"*). Manifest chunk-reference vectors
    /// are ordered by this before serialization, so re-encoding is byte-stable.
    #[inline]
    pub fn canonical_sort_key(&self) -> (u8, [u8; 32], u64) {
        (self.kind.discriminant(), *self.hash.as_bytes(), self.offset)
    }

    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_bytes(self.id.as_bytes());
        self.kind.encode(w);
        self.schema_version.encode(w);
        w.put_u64(self.offset);
        w.put_u64(self.compressed_length);
        w.put_u64(self.uncompressed_length);
        self.compression.encode(w);
        w.put_bytes(self.hash.as_bytes());
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let id = ChunkId(ContentHash(r.take_array::<32>()?));
        let kind = ChunkKind::decode(r)?;
        let schema_version = SchemaVersion::decode(r)?;
        let offset = r.get_u64()?;
        let compressed_length = r.get_u64()?;
        let uncompressed_length = r.get_u64()?;
        let compression = CompressionAlgorithm::decode(r)?;
        let hash = ContentHash(r.take_array::<32>()?);
        Ok(ChunkRef {
            id,
            kind,
            schema_version,
            offset,
            compressed_length,
            uncompressed_length,
            compression,
            hash,
        })
    }
}

// Canonical ordering leads with the Appendix D chunk-reference order
// (`ChunkKind` discriminant, content hash, offset), then breaks ties on the
// remaining fields so the order is *total and consistent with `Eq`*: two
// references that compare `Equal` are equal in every field. (For valid
// content-addressed references the leading key already disambiguates, since the
// hash determines schema/length/kind; the tie-break only orders malformed
// same-key references deterministically instead of preserving insertion order.)
impl PartialOrd for ChunkRef {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ChunkRef {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.canonical_sort_key()
            .cmp(&other.canonical_sort_key())
            .then_with(|| {
                (
                    self.schema_version,
                    self.compressed_length,
                    self.uncompressed_length,
                    self.compression,
                    *self.id.as_bytes(),
                )
                    .cmp(&(
                        other.schema_version,
                        other.compressed_length,
                        other.uncompressed_length,
                        other.compression,
                        *other.id.as_bytes(),
                    ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_kind_discriminants_round_trip() {
        for kind in [
            ChunkKind::OperationEnvelopeBlock,
            ChunkKind::OperationIndex,
            ChunkKind::Snapshot,
            ChunkKind::Blob,
            ChunkKind::ExtensionData,
            ChunkKind::TextProjection,
            ChunkKind::LayoutCache,
            ChunkKind::IntegrityIndex,
            ChunkKind::Manifest,
        ] {
            assert_eq!(
                ChunkKind::from_discriminant(kind.discriminant()),
                Some(kind)
            );
        }
        assert_eq!(ChunkKind::from_discriminant(9), None);
    }

    #[test]
    fn chunk_kind_discriminants_are_golden() {
        // RATIFIED by Pass 11 (item 1.5, req:format:chunkkind-discriminants):
        // the declaration-order discriminant byte is in the chunk hash preimage,
        // so the *literal* values are normative. The round-trip test above is
        // invariant under a coordinated renumbering; this locks the values
        // themselves so a reorder breaks deliberately (and cannot silently
        // change every chunk's content address).
        assert_eq!(ChunkKind::OperationEnvelopeBlock.discriminant(), 0);
        assert_eq!(ChunkKind::OperationIndex.discriminant(), 1);
        assert_eq!(ChunkKind::Snapshot.discriminant(), 2);
        assert_eq!(ChunkKind::Blob.discriminant(), 3);
        assert_eq!(ChunkKind::ExtensionData.discriminant(), 4);
        assert_eq!(ChunkKind::TextProjection.discriminant(), 5);
        assert_eq!(ChunkKind::LayoutCache.discriminant(), 6);
        assert_eq!(ChunkKind::IntegrityIndex.discriminant(), 7);
        assert_eq!(ChunkKind::Manifest.discriminant(), 8);
    }

    #[test]
    fn compression_algorithm_encoding_is_golden() {
        // RATIFIED by Pass 11 (item 1.5, req:format:chunkkind-discriminants):
        // CompressionAlgorithm encodes as a fixed two bytes — a discriminant
        // byte plus an always-present parameter byte. `None` carries a zero
        // parameter byte (it is NOT a bare tag), so lock the exact bytes.
        let enc = |c: CompressionAlgorithm| {
            let mut w = Writer::new();
            c.encode(&mut w);
            w.into_bytes()
        };
        assert_eq!(enc(CompressionAlgorithm::None), vec![0, 0]);
        assert_eq!(enc(CompressionAlgorithm::Zstd { level: 9 }), vec![1, 9]);
        assert_eq!(enc(CompressionAlgorithm::Reserved(7)), vec![2, 7]);
    }

    #[test]
    fn manifest_and_ordinary_chunks_use_distinct_domains() {
        // A manifest payload and an operation block with identical bytes must
        // get different content hashes (different domain tag).
        let m = chunk_content_hash(ChunkKind::Manifest, SchemaVersion::V0, b"same");
        let o = chunk_content_hash(
            ChunkKind::OperationEnvelopeBlock,
            SchemaVersion::V0,
            b"same",
        );
        assert_ne!(m, o);
    }

    #[test]
    fn chunk_hash_commits_to_kind_schema_and_length() {
        let base = chunk_content_hash(ChunkKind::OperationIndex, SchemaVersion::V0, b"abc");
        // Different schema -> different hash.
        assert_ne!(
            base,
            chunk_content_hash(ChunkKind::OperationIndex, SchemaVersion::new(1, 0), b"abc")
        );
        // Different kind -> different hash.
        assert_ne!(
            base,
            chunk_content_hash(ChunkKind::LayoutCache, SchemaVersion::V0, b"abc")
        );
    }

    #[test]
    fn hash_preimage_matches_the_spec_layout() {
        // Reproduce hash_preimage() from Chapter 8 by hand and confirm equality.
        let kind = ChunkKind::OperationEnvelopeBlock;
        let schema = SchemaVersion::new(0, 1);
        let payload = b"envelope-bytes";
        let mut manual = Vec::new();
        manual.extend_from_slice(DomainTag::CHUNK.as_bytes());
        manual.extend_from_slice(&kind.canonical_bytes());
        manual.extend_from_slice(&schema.canonical_bytes());
        manual.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        manual.extend_from_slice(payload);
        assert_eq!(
            chunk_content_hash(kind, schema, payload),
            ContentHash(epiphany_determinism::blake3_256(&manual))
        );
    }

    #[test]
    fn chunk_refs_order_by_kind_then_hash_then_offset() {
        let mk = |kind: ChunkKind, h: u8, off: u64| ChunkRef {
            id: ChunkId(ContentHash([h; 32])),
            kind,
            schema_version: SchemaVersion::V0,
            offset: off,
            compressed_length: 1,
            uncompressed_length: 1,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([h; 32]),
        };
        let mut refs = [
            mk(ChunkKind::Snapshot, 1, 10),
            mk(ChunkKind::OperationEnvelopeBlock, 9, 5),
            mk(ChunkKind::OperationEnvelopeBlock, 9, 1),
            mk(ChunkKind::OperationEnvelopeBlock, 2, 99),
        ];
        refs.sort();
        // OperationEnvelopeBlock (disc 0) sorts before Snapshot (disc 2); within
        // the block kind, hash 2 before hash 9; within equal hash, offset 1 < 5.
        assert_eq!(refs[0].hash.as_bytes()[0], 2);
        assert_eq!(refs[1].offset, 1);
        assert_eq!(refs[2].offset, 5);
        assert_eq!(refs[3].kind, ChunkKind::Snapshot);
    }

    #[test]
    fn chunk_ref_ord_is_total_and_consistent_with_eq() {
        // Two references sharing (kind, hash, offset) but differing in another
        // field must not compare Equal — Ord is total and agrees with Eq.
        let base = ChunkRef {
            id: ChunkId(ContentHash([5; 32])),
            kind: ChunkKind::OperationEnvelopeBlock,
            schema_version: SchemaVersion::V0,
            offset: 600,
            compressed_length: 10,
            uncompressed_length: 10,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([5; 32]),
        };
        let mut other = base;
        other.uncompressed_length = 11;
        assert_ne!(base, other);
        assert_ne!(base.cmp(&other), core::cmp::Ordering::Equal);
        // Equal refs still compare Equal.
        assert_eq!(base.cmp(&base), core::cmp::Ordering::Equal);
    }

    #[test]
    fn chunk_ref_round_trips() {
        let r0 = ChunkRef {
            id: ChunkId(ContentHash([3; 32])),
            kind: ChunkKind::Blob,
            schema_version: SchemaVersion::new(2, 5),
            offset: 0x0102_0304,
            compressed_length: 42,
            uncompressed_length: 42,
            compression: CompressionAlgorithm::Zstd { level: 9 },
            hash: ContentHash([3; 32]),
        };
        let mut w = Writer::new();
        r0.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(ChunkRef::decode(&mut r).unwrap(), r0);
        assert!(r.finish().is_ok());
    }
}
