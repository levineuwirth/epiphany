//! Bundle-level identifiers and small value types.
//!
//! `epiphany-bundle` depends on `epiphany-determinism` (Agent A) and on nothing
//! else — in particular **not** on `epiphany-core` (Agent B) or `epiphany-ops`
//! (Agent C). The QUICKSTART fixes that boundary deliberately: *"bundles handle
//! bytes, ops handles semantics. A canonical-base snapshot from the bundle's
//! perspective is opaque bytes plus a frontier DVV; only `epiphany-ops`
//! interprets it."*
//!
//! So the identifiers the manifest carries that *belong* to the semantic layer
//! ([`DocumentId`], [`LineageId`], [`SnapshotId`], [`ExtensionId`], the causal
//! [`FrontierBytes`]) are modeled here as opaque fixed-width or length-prefixed
//! values. The bundle stores, orders, and integrity-checks them; it does not
//! interpret them. The identifiers the bundle *owns* ([`FileUuid`],
//! [`ManifestId`]) are defined and derived here in full.

use crate::codec::{DecodeError, Reader, Writer};
use epiphany_determinism::{ContentHash, DomainTag, Preimage};

/// Physical-bundle identity (Chapter 8 §"File, Document, and Lineage
/// Identity"): a 128-bit UUID set at file creation, persisting for the lifetime
/// of the physical file and changing on Save As. Distinct from [`DocumentId`],
/// which identifies the logical work. Opaque to the bundle: 16 raw bytes.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct FileUuid(pub [u8; 16]);

impl FileUuid {
    /// The all-zero UUID. A valid sentinel only for an uninitialized prelude;
    /// a created bundle carries a caller-supplied value. (`as_bytes`/`encode`/
    /// `decode` are provided by the `opaque_id16!` macro below.)
    pub const ZERO: FileUuid = FileUuid([0u8; 16]);
}

/// Logical-work identity (Chapter 8): stable across Save As copies of the same
/// work; a derivative-work fork mints a new one. Opaque to the bundle.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct DocumentId(pub [u8; 16]);

/// Shared-ancestor identity (Chapter 8): records that two documents share a
/// common ancestor, for version-control genealogy. Opaque to the bundle.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct LineageId(pub [u8; 16]);

/// A materialized-snapshot identity (Chapter 8 §"Snapshots and Canonical
/// Bases"). Opaque to the bundle; only the semantic layer materializes state.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SnapshotId(pub [u8; 16]);

/// An extension's identity (Chapter 8 §"Extension Declarations"). Opaque to the
/// bundle, which preserves unknown extensions' chunks without interpreting them.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ExtensionId(pub [u8; 16]);

/// Registry id for a [`crate::ProfileId::Custom`] profile (Chapter 8
/// §"Format Profiles"). Opaque 128-bit value.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ProfileRegistryId(pub [u8; 16]);

macro_rules! opaque_id16 {
    ($name:ident) => {
        impl $name {
            /// The raw 16 bytes.
            #[inline]
            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
            #[inline]
            pub(crate) fn encode(&self, w: &mut Writer) {
                w.put_bytes(&self.0);
            }
            #[inline]
            pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
                Ok($name(r.take_array::<16>()?))
            }
        }
    };
}

opaque_id16!(FileUuid);
opaque_id16!(DocumentId);
opaque_id16!(LineageId);
opaque_id16!(SnapshotId);
opaque_id16!(ExtensionId);
opaque_id16!(ProfileRegistryId);

/// The identity of a manifest (Chapter 8 §"The Manifest"): *"Each commit
/// produces a new `ManifestId`."* The bundle owns this derivation. It is a
/// content-derived 128-bit value, `trunc128(BLAKE3("MUSCMNIF" || preimage))`,
/// using the determinism crate's reserved [`DomainTag::MANIFEST_ID`] tag and
/// the same big-endian truncation as every other content-derived id
/// (`ConflictId`, etc. — Chapter 6/8).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ManifestId(pub u128);

impl ManifestId {
    /// Derives the manifest id from the manifest's identity-bearing preimage:
    /// the document id, generation, and the canonical bytes of the manifest
    /// body (with the `manifest_id` field itself excluded to avoid a circular
    /// dependency). Deterministic — two writers encoding the same manifest
    /// content derive the same id. RATIFIED by Pass 11 (item 1.6, P11-D5):
    /// core_spec §"Manifest Encoding", Requirement `req:format:manifest-id`
    /// (`trunc128(BLAKE3("MUSCMNIF" || document_id || generation || body))`,
    /// body excluding `manifest_id`).
    ///
    /// Note: `document_id` and `generation` are committed twice — explicitly
    /// here and again inside `body_preimage` (the canonical manifest body opens
    /// with them). This duplication is intentional and golden-locked, not an
    /// oversight: the preimage shape above is the ratified format.
    pub(crate) fn derive(document_id: DocumentId, generation: u64, body_preimage: &[u8]) -> Self {
        let mut p = Preimage::new(DomainTag::MANIFEST_ID);
        p.push_bytes(document_id.as_bytes());
        p.push_u64_le(generation);
        p.push_bytes(body_preimage);
        ManifestId(p.finish_trunc128())
    }

    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_u128(self.0);
    }

    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(ManifestId(r.get_u128()?))
    }
}

impl core::fmt::Debug for ManifestId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ManifestId({:032x})", self.0)
    }
}

/// A blob identity (Chapter 8 §"Blobs"): the [`ContentHash`] of the blob's
/// uncompressed payload under the `MUSCBLOB` domain tag. A newtype over
/// `ContentHash`, mirroring the determinism crate's `ChunkId`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct BlobId(pub ContentHash);

impl BlobId {
    /// The blob id of `payload`: `BLAKE3("MUSCBLOB" || payload)`
    /// ([`ContentHash::of_blob`]). Blobs are the one content hash that is a
    /// bare `domain || payload` (Chapter 8 §"Blobs"; determinism crate
    /// `ContentHash::of_blob`), not the structured chunk preimage.
    #[inline]
    pub fn of_payload(payload: &[u8]) -> Self {
        BlobId(ContentHash::of_blob(payload))
    }

    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_bytes(self.0.as_bytes());
    }

    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(BlobId(ContentHash(r.take_array::<32>()?)))
    }
}

/// The schema version a chunk payload is encoded against (Chapter 8
/// §"Schema Versioning"). Major changes are non-backward-compatible; minor
/// changes only add optional fields/variants.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
}

impl SchemaVersion {
    /// The baseline prototype schema version (major 0). Every chunk whose
    /// layout is unchanged by the schema-major-1 bump keeps this version.
    pub const V0: SchemaVersion = SchemaVersion { major: 0, minor: 1 };

    /// Schema major 1 — the first data-model expansion major (Binary Format
    /// companion §"Schema Major 1"). Stamped on chunks whose payload carries a
    /// v1 layout: the acceleration full-`Score` snapshot, the resolved-layout
    /// `LayoutCache`, and any operation-envelope block bearing a v1
    /// `CreateRegion`. The canonical-base `MaterializedState`, the manifest,
    /// and operation blocks without a changed payload stay at [`Self::V0`].
    pub const V1: SchemaVersion = SchemaVersion { major: 1, minor: 0 };

    /// Schema major 2 — the second data-model expansion major (Binary Format
    /// companion §"Schema Major 2"): the cross-cutting bodies, repeats/voltas,
    /// staff/instrument/metadata fills. Stamped (minimally — the lowest major
    /// whose layouts decode the bytes) on chunks whose payload carries a v2
    /// layout: the acceleration full-`Score` snapshot and any
    /// operation-envelope block bearing a v2 value.
    pub const V2: SchemaVersion = SchemaVersion { major: 2, minor: 0 };

    /// Constructs a schema version.
    #[inline]
    pub const fn new(major: u16, minor: u16) -> Self {
        SchemaVersion { major, minor }
    }

    /// The current schema version at a given major: [`Self::V0`] for major 0,
    /// [`Self::V1`] for major 1, [`Self::V2`] for major 2, and `{major, 0}`
    /// for any higher (future) major. A writer maps a chunk's derived schema
    /// major to a version this way — e.g. an operation-envelope block stamps
    /// the max over its operations' `schema_major()`.
    #[inline]
    pub const fn for_major(major: u16) -> Self {
        match major {
            0 => SchemaVersion::V0,
            1 => SchemaVersion::V1,
            2 => SchemaVersion::V2,
            m => SchemaVersion { major: m, minor: 0 },
        }
    }

    /// Canonical 4 bytes for the hash preimage (Chapter 8
    /// §"Domain-Separated Preimages"): major then minor, little-endian.
    #[inline]
    pub fn canonical_bytes(self) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[0..2].copy_from_slice(&self.major.to_le_bytes());
        out[2..4].copy_from_slice(&self.minor.to_le_bytes());
        out
    }

    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_u16(self.major).put_u16(self.minor);
    }

    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(SchemaVersion {
            major: r.get_u16()?,
            minor: r.get_u16()?,
        })
    }
}

/// The reduction-algorithm version that produced a canonical-base snapshot
/// (Chapter 8): a snapshot may serve as a canonical base only if this matches
/// the active superblock's value. Modeled as an opaque monotonically-versioned
/// `u32` (the algorithm catalog itself lives in `epiphany-ops`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ReductionAlgorithmVersion(pub u32);

impl ReductionAlgorithmVersion {
    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_u32(self.0);
    }
    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(ReductionAlgorithmVersion(r.get_u32()?))
    }
}

/// A semantic version (Chapter 8 §"Format Profiles" / §"Extension
/// Declarations"). Ordered major, then minor, then patch — the
/// "semantic version lexicographic" order Appendix D names for extension
/// declarations.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    /// Constructs a semantic version.
    #[inline]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        SemVer {
            major,
            minor,
            patch,
        }
    }

    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_u32(self.major)
            .put_u32(self.minor)
            .put_u32(self.patch);
    }
    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(SemVer {
            major: r.get_u32()?,
            minor: r.get_u32()?,
            patch: r.get_u32()?,
        })
    }
}

/// A point in wall-clock time, nanoseconds from a region origin (mirrors
/// `epiphany-core`'s Chapter 3 `WallClockTime`, redefined locally to keep the
/// A-only dependency boundary). In the bundle it is the superblock's advisory
/// `commit_timestamp` only; **superblock selection is by generation, never by
/// timestamp** (Chapter 8 §"The Superblock Slots").
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct WallClockTime(pub i64);

impl WallClockTime {
    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_i64(self.0);
    }
    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(WallClockTime(r.get_i64()?))
    }
}

/// A wall-clock duration, nanoseconds (Chapter 3). Used by [`crate::RetentionPolicy`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct WallClockDuration(pub i64);

impl WallClockDuration {
    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_i64(self.0);
    }
    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(WallClockDuration(r.get_i64()?))
    }
}

/// A causal frontier (a dotted version vector) as seen by the bundle: an opaque,
/// length-prefixed byte string. The bundle stores and round-trips it but does
/// not interpret it — coverage and the DVV partial order are `epiphany-ops`'s
/// job (QUICKSTART: *"a frontier DVV; only `epiphany-ops` interprets it"*).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct FrontierBytes(pub Vec<u8>);

impl FrontierBytes {
    /// An empty frontier (the natural value for a bundle with no canonical base).
    pub const fn empty() -> Self {
        FrontierBytes(Vec::new())
    }

    /// Wraps opaque DVV bytes produced by the semantic layer.
    #[inline]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        FrontierBytes(bytes)
    }

    /// The opaque bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[inline]
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_var_bytes(&self.0);
    }
    #[inline]
    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(FrontierBytes(r.get_var_bytes()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_id_is_content_derived_and_deterministic() {
        let doc = DocumentId([7u8; 16]);
        let a = ManifestId::derive(doc, 3, b"body-bytes");
        let b = ManifestId::derive(doc, 3, b"body-bytes");
        let c = ManifestId::derive(doc, 4, b"body-bytes");
        assert_eq!(a, b, "same inputs derive the same manifest id");
        assert_ne!(a, c, "a different generation derives a different id");
    }

    #[test]
    fn schema_version_canonical_bytes_are_major_then_minor_le() {
        let v = SchemaVersion::new(0x0102, 0x0304);
        assert_eq!(v.canonical_bytes(), [0x02, 0x01, 0x04, 0x03]);
    }

    #[test]
    fn blob_id_matches_bare_domain_payload() {
        assert_eq!(BlobId::of_payload(b"x").0, ContentHash::of_blob(b"x"));
    }

    #[test]
    fn value_types_round_trip() {
        let mut w = Writer::new();
        SchemaVersion::new(2, 9).encode(&mut w);
        SemVer::new(1, 4, 7).encode(&mut w);
        WallClockTime(-123).encode(&mut w);
        FrontierBytes::from_bytes(vec![1, 2, 3]).encode(&mut w);
        let bytes = w.into_bytes();

        let mut r = Reader::new(&bytes);
        assert_eq!(
            SchemaVersion::decode(&mut r).unwrap(),
            SchemaVersion::new(2, 9)
        );
        assert_eq!(SemVer::decode(&mut r).unwrap(), SemVer::new(1, 4, 7));
        assert_eq!(WallClockTime::decode(&mut r).unwrap(), WallClockTime(-123));
        assert_eq!(
            FrontierBytes::decode(&mut r).unwrap(),
            FrontierBytes::from_bytes(vec![1, 2, 3])
        );
        assert!(r.finish().is_ok());
    }
}
