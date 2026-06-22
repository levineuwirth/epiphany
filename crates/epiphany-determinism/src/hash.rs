//! Content hashing and the content-address newtypes.
//!
//! Epiphany uses BLAKE3-256 as its single content-hashing algorithm
//! (Chapter 8 §"Content Hashing"). All hashes in canonical state are 32-byte
//! BLAKE3 outputs over a *domain-separated preimage*: the 8-byte
//! [`DomainTag`] is always the first bytes hashed, so two semantically
//! different objects with identical raw bytes never collide.
//!
//! Two derived integer widths recur in the spec:
//!
//! * `trunc64(BLAKE3(...))` — the 64-bit counter of a system-derived
//!   identifier (Chapter 5) and similar. [`trunc64`].
//! * `trunc128(BLAKE3(...))` — content-derived 128-bit identifiers such as
//!   `ConflictId` and `ManifestId` (Chapters 6, 8). [`trunc128`].
//!
//! Both truncations take the **leading** bytes of the digest in **big-endian**
//! order, matching the spec's reference code (`u64::from_be_bytes(hash[0..8])`,
//! `u128::from_be_bytes(hash[0..16])`).

use crate::domain::{DomainTag, SystemDomainTag};
use crate::float::CanonicalF64;

/// Computes the raw BLAKE3-256 digest of `data` (32 bytes). This is the one
/// content-hashing primitive; no other algorithm appears in this format
/// version (Chapter 8 §"Content Hashing").
#[inline]
pub fn blake3_256(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// Truncates a 256-bit digest to 64 bits by taking the leading 8 bytes as a
/// big-endian integer (Chapter 5 `derive_system_counter`).
#[inline]
pub fn trunc64(digest: &[u8; 32]) -> u64 {
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[0..8]);
    u64::from_be_bytes(head)
}

/// Truncates a 256-bit digest to 128 bits by taking the leading 16 bytes as a
/// big-endian integer (Chapter 6 `derive_conflict_id`, Chapter 8 `ManifestId`).
#[inline]
pub fn trunc128(digest: &[u8; 32]) -> u128 {
    let mut head = [0u8; 16];
    head.copy_from_slice(&digest[0..16]);
    u128::from_be_bytes(head)
}

/// Derives the 64-bit counter portion of a system-derived identifier
/// (Chapter 5 §"System-Derived Identifiers"). The preimage is the domain tag
/// followed by the canonical input bytes; the digest is truncated to 64 bits.
///
/// The typed identifiers that wrap this counter live in `epiphany-core`; this
/// crate owns only the deterministic derivation primitive so every replica
/// derives byte-identical system identifiers from identical canonical inputs.
///
/// The `domain` is a [`SystemDomainTag`], so the spec's precondition — only
/// `MUSCSVCE`, `MUSCSPCH`, or an extension `MUSCS` tag may seed a system
/// identifier (Chapter 5) — is enforced by the type, in every build profile.
/// A non-system tag such as [`DomainTag::CHUNK`] simply cannot be passed.
#[inline]
pub fn derive_system_counter(domain: SystemDomainTag, canonical_inputs: &[u8]) -> u64 {
    let mut p = Preimage::new(domain.tag());
    p.push_bytes(canonical_inputs);
    p.finish_trunc64()
}

/// A BLAKE3-256 content hash: the canonical hash for every content-addressed
/// object in the bundle (Chapter 8). Ordering is lexicographic on the 32
/// bytes, which is exactly the order Appendix D mandates for chunk references.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// The all-zero hash. Useful as a sentinel; never a real content address.
    pub const ZERO: ContentHash = ContentHash([0u8; 32]);

    /// Hashes a blob payload: `BLAKE3(MUSCBLOB || payload)`. This is the
    /// `BlobId` construction — the only spec content hash that is a bare
    /// `domain || payload`. RATIFIED by Pass 11 (item 3.1, P11-D3, a spec-bug
    /// fix): core_spec §"Blobs", Requirement `req:format:blob-hash-shape` now
    /// states the bare form explicitly and deletes the contradictory
    /// "identically to chunks" phrasing.
    ///
    /// Other content hashes are *not* this shape: a chunk hash also commits to
    /// kind, schema version, and uncompressed length (Chapter 8
    /// §"Domain-Separated Preimages"), and the manifest/snapshot hashes have
    /// their own structured preimages. Build those with [`Preimage`] in
    /// `epiphany-bundle`; there is deliberately no arbitrary-domain
    /// single-payload constructor here, so a chunk hash can't accidentally be
    /// computed as `BLAKE3(MUSCCHNK || payload)`.
    #[inline]
    pub fn of_blob(payload: &[u8]) -> Self {
        Preimage::new(DomainTag::BLOB).push_bytes(payload).finish()
    }

    /// The raw 32 bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex rendering (64 characters). Deterministic and
    /// locale-independent (Appendix D §"Text and Unicode").
    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(64);
        for &b in &self.0 {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0x0f) as usize] as char);
        }
        s
    }
}

impl core::fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Hashes are long; show the first 8 hex chars, like git short hashes.
        write!(f, "ContentHash({}…)", &self.to_hex()[..8])
    }
}

/// A chunk identifier: a newtype around [`ContentHash`]. The chunk's
/// identifier *is* its content hash; the distinct type makes the role visible
/// at use sites (a `ChunkId` is what you store in a `ChunkRef` and look up; a
/// `ContentHash` is what you compute by hashing). Both occupy the same 32
/// bytes (Chapter 8 §"Chunks").
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ChunkId(pub ContentHash);

impl ChunkId {
    /// The underlying content hash.
    #[inline]
    pub const fn content_hash(&self) -> ContentHash {
        self.0
    }

    /// The raw 32 bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl From<ContentHash> for ChunkId {
    #[inline]
    fn from(h: ContentHash) -> Self {
        ChunkId(h)
    }
}

/// A builder for domain-separated hash preimages.
///
/// Every canonical hash starts with an 8-byte [`DomainTag`] and then appends
/// canonical field bytes in a fixed order. `Preimage` enforces the
/// tag-goes-first discipline and offers little-endian integer and canonical
/// `f64` pushes so callers never hand-roll byte orders. Floating-point fields
/// are pushed as [`CanonicalF64`], so a NaN/inf can never enter a preimage and
/// `-0.0` is already normalized to `+0.0` (Appendix D §Serialization).
#[derive(Clone, Debug)]
pub struct Preimage {
    buf: Vec<u8>,
}

impl Preimage {
    /// Starts a preimage with its domain tag.
    #[inline]
    pub fn new(domain: DomainTag) -> Self {
        let mut buf = Vec::with_capacity(DomainTag::LEN + 32);
        buf.extend_from_slice(domain.as_bytes());
        Preimage { buf }
    }

    /// Appends raw bytes.
    #[inline]
    pub fn push_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    /// Appends a `u64` in little-endian order (the spec's convention for
    /// length and counter fields in preimages, e.g. `uncompressed_length`).
    #[inline]
    pub fn push_u64_le(&mut self, value: u64) -> &mut Self {
        self.buf.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Appends a canonical `f64` (8 little-endian bytes). The argument is a
    /// [`CanonicalF64`], so a NaN/inf can never reach a hash preimage: it was
    /// rejected at [`CanonicalF64::new`]. `-0.0` is already normalized to
    /// `+0.0` by that type.
    #[inline]
    pub fn push_f64(&mut self, value: CanonicalF64) -> &mut Self {
        self.buf.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// The accumulated preimage bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Finishes as a [`ContentHash`] (full 256-bit digest).
    #[inline]
    pub fn finish(&self) -> ContentHash {
        ContentHash(blake3_256(&self.buf))
    }

    /// Finishes as a [`ChunkId`].
    #[inline]
    pub fn finish_chunk_id(&self) -> ChunkId {
        ChunkId(self.finish())
    }

    /// Finishes as a 64-bit big-endian truncation (system-derived counters).
    #[inline]
    pub fn finish_trunc64(&self) -> u64 {
        trunc64(&blake3_256(&self.buf))
    }

    /// Finishes as a 128-bit big-endian truncation (content-derived ids).
    #[inline]
    pub fn finish_trunc128(&self) -> u128 {
        trunc128(&blake3_256(&self.buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // BLAKE3 of the empty input — the canonical published test vector. Locking
    // it down proves we are hashing the bytes we think we are.
    const EMPTY_BLAKE3_HEX: &str =
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    #[test]
    fn empty_digest_matches_published_vector() {
        let h = ContentHash(blake3_256(b""));
        assert_eq!(h.to_hex(), EMPTY_BLAKE3_HEX);
    }

    #[test]
    fn truncations_are_big_endian_leading_bytes() {
        let d = blake3_256(b"");
        assert_eq!(trunc64(&d), 0xaf13_49b9_f5f9_a1a6);
        assert_eq!(trunc128(&d), 0xaf13_49b9_f5f9_a1a6_a040_4dea_36dc_c949);
    }

    #[test]
    fn domain_separation_changes_the_hash() {
        let a = Preimage::new(DomainTag::BLOB)
            .push_bytes(b"payload")
            .finish();
        let b = Preimage::new(DomainTag::CHUNK)
            .push_bytes(b"payload")
            .finish();
        assert_ne!(a, b, "same payload under different tags must differ");

        // `of_blob` really is BLAKE3(MUSCBLOB || payload).
        let mut manual = Vec::new();
        manual.extend_from_slice(DomainTag::BLOB.as_bytes());
        manual.extend_from_slice(b"payload");
        assert_eq!(
            ContentHash::of_blob(b"payload"),
            ContentHash(blake3_256(&manual))
        );
        assert_eq!(a, ContentHash::of_blob(b"payload"));
    }

    #[test]
    fn preimage_field_order_is_significant() {
        let mut p1 = Preimage::new(DomainTag::CONFLICT);
        p1.push_u64_le(1).push_u64_le(2);
        let mut p2 = Preimage::new(DomainTag::CONFLICT);
        p2.push_u64_le(2).push_u64_le(1);
        assert_ne!(p1.finish(), p2.finish());
    }

    #[test]
    fn preimage_f64_uses_canonical_zero() {
        let mut neg = Preimage::new(DomainTag::CONFLICT);
        neg.push_f64(CanonicalF64::new(-0.0).unwrap());
        let mut pos = Preimage::new(DomainTag::CONFLICT);
        pos.push_f64(CanonicalF64::new(0.0).unwrap());
        assert_eq!(neg.finish(), pos.finish());
    }

    #[test]
    fn derive_system_counter_is_tag_then_inputs_trunc64() {
        let got = derive_system_counter(SystemDomainTag::VOICE, b"abc");
        let mut manual = Vec::new();
        manual.extend_from_slice(DomainTag::SYSTEM_VOICE.as_bytes());
        manual.extend_from_slice(b"abc");
        assert_eq!(got, trunc64(&blake3_256(&manual)));
    }

    #[test]
    fn chunk_id_shares_bytes_with_content_hash() {
        let h = ContentHash::of_blob(b"x");
        let id = ChunkId::from(h);
        assert_eq!(id.as_bytes(), h.as_bytes());
        assert_eq!(id.content_hash(), h);
    }

    #[test]
    fn content_hash_orders_lexicographically() {
        let lo = ContentHash([0u8; 32]);
        let mut hi_bytes = [0u8; 32];
        hi_bytes[0] = 1;
        let hi = ContentHash(hi_bytes);
        assert!(lo < hi);
    }
}
