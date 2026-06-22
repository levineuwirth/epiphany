//! Domain-separation tags.
//!
//! Epiphany hashes domain-separated preimages so that two semantically
//! different chunks with identical raw bytes never share a content address
//! (Chapter 8 §"Domain-Separated Preimages"). Every tag is a fixed 8-byte
//! ASCII string beginning with `MUSC`. Centralizing them here keeps the set
//! drift-free: there is exactly one definition of each tag in the workspace.

/// A fixed 8-byte domain-separation tag. Always the first bytes of a hash
/// preimage (see [`crate::Preimage`]).
///
/// The spec's domain-tag vocabulary is closed: the reserved built-ins
/// ([`DomainTag::BUILTINS`]) plus extension-introduced *system-derived* tags,
/// which "MUST begin with `MUSCS` and have length exactly 8 bytes" (Chapter 5).
/// Every tag is an 8-byte ASCII string. The field is private and the
/// constructors enforce that vocabulary, so a nonconforming tag — wrong prefix
/// (`b"BAD_TAG!"`), non-ASCII bytes, or an unregistered `MUSC....` domain —
/// cannot be minted and therefore cannot reach [`crate::derive_system_counter`]
/// or a hash preimage. To mint an extension's own tag use [`SystemDomainTag`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DomainTag([u8; 8]);

impl DomainTag {
    /// Length of every domain tag, in bytes.
    pub const LEN: usize = 8;

    /// The prefix marking a *system-derived* tag (Chapter 5:
    /// "Additional domain tags introduced by registered extensions MUST begin
    /// with `MUSCS` and have length exactly 8 bytes"). The three built-in
    /// system tags ([`Self::SYSTEM_VOICE`], [`Self::SYSTEM_PITCH`],
    /// [`Self::SYSTEM_ANOMALY`]) also carry it.
    const SYSTEM_PREFIX: &'static [u8] = b"MUSCS";

    /// The raw 8 ASCII bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    // --- Built-in tags (Chapter 8 §"Domain-Separated Preimages", Ch. 5/6). ---

    /// `.musc` chunk payloads.
    pub const CHUNK: DomainTag = DomainTag(*b"MUSCCHNK");
    /// Manifest chunk payloads.
    pub const MANIFEST: DomainTag = DomainTag(*b"MUSCMANI");
    /// Blob payloads; a `BlobId` is the [`crate::ContentHash`] under this tag.
    pub const BLOB: DomainTag = DomainTag(*b"MUSCBLOB");
    /// `ConflictId` derivation (Chapter 6 §"Conflict Identity").
    pub const CONFLICT: DomainTag = DomainTag(*b"MUSCCONF");
    /// Canonical operation-envelope hash, `EnvelopeHash` (Chapter 6 §6.5).
    pub const ENVELOPE: DomainTag = DomainTag(*b"MUSCENVH");
    /// Glyph-catalog metrics hash (Chapter 7 §"Glyph Catalog Identity").
    pub const FONT_METRICS: DomainTag = DomainTag(*b"MUSCFNTM");
    /// `ManifestId` derivation (Chapter 8 / deferred-types table).
    pub const MANIFEST_ID: DomainTag = DomainTag(*b"MUSCMNIF");
    /// System-promoted voice counter derivation (Chapter 5 §"System-Derived").
    pub const SYSTEM_VOICE: DomainTag = DomainTag(*b"MUSCSVCE");
    /// System-derived pitch counter derivation (Chapter 5 §"System-Derived").
    pub const SYSTEM_PITCH: DomainTag = DomainTag(*b"MUSCSPCH");
    /// `IntegrityAnomalyId` derivation (Chapter 5 §"System-Derived Counter
    /// Collisions"). Reserved built-in: anomalies are core, not an extension
    /// concern (Pass 11, item 1.4).
    pub const SYSTEM_ANOMALY: DomainTag = DomainTag(*b"MUSCSANM");

    /// Every built-in tag, in declaration order. The closed core vocabulary.
    pub const BUILTINS: [DomainTag; 10] = [
        Self::CHUNK,
        Self::MANIFEST,
        Self::BLOB,
        Self::CONFLICT,
        Self::ENVELOPE,
        Self::FONT_METRICS,
        Self::MANIFEST_ID,
        Self::SYSTEM_VOICE,
        Self::SYSTEM_PITCH,
        Self::SYSTEM_ANOMALY,
    ];

    /// Constructs a domain tag from raw bytes, accepting only the spec's closed
    /// vocabulary: a reserved built-in, or a well-formed extension system tag
    /// (begins `MUSCS`). Every byte must be printable ASCII. Returns `None`
    /// otherwise — wrong prefix, non-ASCII bytes, or an unregistered
    /// `MUSC....` domain that is neither built-in nor a `MUSCS` system tag.
    /// This is the checked entry point for decoding a tag from storage/interop.
    #[inline]
    pub fn from_bytes(raw: [u8; 8]) -> Option<Self> {
        if Self::is_valid_bytes(&raw) {
            Some(DomainTag(raw))
        } else {
            None
        }
    }

    /// Validity predicate for the closed vocabulary: printable-ASCII, not a
    /// file-format magic byte string, and either a registered built-in or a
    /// `MUSCS`-prefixed system tag.
    #[inline]
    fn is_valid_bytes(raw: &[u8; 8]) -> bool {
        if !raw.iter().all(u8::is_ascii_graphic) {
            return false;
        }
        if Self::is_file_magic(raw) {
            return false;
        }
        DomainTag::BUILTINS.iter().any(|b| b.as_bytes() == raw)
            || raw.starts_with(Self::SYSTEM_PREFIX)
    }

    /// File-format magic byte strings are in the same 8-byte `MUSC*`
    /// namespace, but they are not hash-domain tags and must not be reused by
    /// extension system identifiers.
    #[inline]
    fn is_file_magic(raw: &[u8; 8]) -> bool {
        raw == &BUNDLE_MAGIC || raw == &SUPERBLOCK_MAGIC
    }

    /// Whether this is one of the reserved built-in tags ([`Self::BUILTINS`]).
    #[inline]
    pub fn is_builtin(&self) -> bool {
        Self::BUILTINS.contains(self)
    }

    /// Whether this is a *system-derived* tag (begins `MUSCS`): a built-in
    /// [`Self::SYSTEM_VOICE`] / [`Self::SYSTEM_PITCH`] / [`Self::SYSTEM_ANOMALY`]
    /// or an extension tag minted via [`SystemDomainTag::new_extension`].
    #[inline]
    pub fn is_system_derived(&self) -> bool {
        self.0.starts_with(Self::SYSTEM_PREFIX)
    }

    /// Whether this is an *extension-introduced* system tag: system-derived and
    /// not a reserved built-in. The three built-in system tags return `false`
    /// here — they are reserved, not extension-introduced.
    #[inline]
    pub fn is_extension_system_tag(&self) -> bool {
        self.is_system_derived() && !self.is_builtin()
    }
}

/// A [`DomainTag`] proven to be *system-derived* (begins `MUSCS`): a built-in
/// [`DomainTag::SYSTEM_VOICE`] / [`DomainTag::SYSTEM_PITCH`] /
/// [`DomainTag::SYSTEM_ANOMALY`], or an
/// extension-introduced tag. Only these are admissible seeds for
/// [`crate::derive_system_counter`] (Chapter 5 §"System-Derived Identifiers").
///
/// Carrying the precondition in the type — rather than checking it at the call
/// site — makes `derive_system_counter` total: it is impossible to seed a
/// system identifier from, say, [`DomainTag::CHUNK`], because that value cannot
/// be turned into a `SystemDomainTag`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SystemDomainTag(DomainTag);

impl SystemDomainTag {
    /// Built-in: system-promoted voice counters (`MUSCSVCE`).
    pub const VOICE: SystemDomainTag = SystemDomainTag(DomainTag::SYSTEM_VOICE);
    /// Built-in: system-derived pitch counters (`MUSCSPCH`).
    pub const PITCH: SystemDomainTag = SystemDomainTag(DomainTag::SYSTEM_PITCH);
    /// Built-in: integrity-anomaly identifiers (`MUSCSANM`).
    pub const ANOMALY: SystemDomainTag = SystemDomainTag(DomainTag::SYSTEM_ANOMALY);

    /// Wraps a domain tag if it is system-derived; returns `None` otherwise.
    #[inline]
    pub fn new(tag: DomainTag) -> Option<Self> {
        if tag.is_system_derived() {
            Some(SystemDomainTag(tag))
        } else {
            None
        }
    }

    /// Mints an *extension-introduced* system-derived tag from raw bytes,
    /// enforcing the Chapter 5 rule: printable ASCII, begins `MUSCS`, and does
    /// not collide with a reserved built-in. The only sanctioned way for an
    /// extension to introduce its own system-derived domain tag.
    #[inline]
    pub fn new_extension(raw: [u8; 8]) -> Option<Self> {
        let tag = DomainTag::from_bytes(raw)?;
        if tag.is_extension_system_tag() {
            Some(SystemDomainTag(tag))
        } else {
            None
        }
    }

    /// The underlying domain tag.
    #[inline]
    pub const fn tag(self) -> DomainTag {
        self.0
    }

    /// The raw 8 ASCII bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 8] {
        self.0.as_bytes()
    }
}

// --- File-format magic byte strings (Chapter 8 §"The Bundle Layout"). ---
//
// These are not hashing domain tags; they are the literal magic bytes that
// open the fixed header and the superblock slots. They are centralized here
// alongside the domain tags so the full set of 8-byte `MUSC*` constants has a
// single home. `epiphany-bundle` (Agent D) consumes them.

/// Bundle fixed-header magic: ASCII `"MUSCBND\0"` (8 bytes, trailing NUL).
pub const BUNDLE_MAGIC: [u8; 8] = *b"MUSCBND\0";

/// Superblock-slot magic: ASCII `"MUSCSUPR"` (8 bytes).
pub const SUPERBLOCK_MAGIC: [u8; 8] = *b"MUSCSUPR";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tag_is_eight_ascii_bytes_starting_with_musc() {
        let tags = [
            DomainTag::CHUNK,
            DomainTag::MANIFEST,
            DomainTag::BLOB,
            DomainTag::CONFLICT,
            DomainTag::ENVELOPE,
            DomainTag::FONT_METRICS,
            DomainTag::MANIFEST_ID,
            DomainTag::SYSTEM_VOICE,
            DomainTag::SYSTEM_PITCH,
            DomainTag::SYSTEM_ANOMALY,
        ];
        for t in tags {
            assert_eq!(t.as_bytes().len(), DomainTag::LEN);
            assert!(t.as_bytes().starts_with(b"MUSC"), "{t:?}");
            assert!(t.as_bytes().iter().all(|b| b.is_ascii()), "{t:?}");
        }
    }

    #[test]
    fn tags_are_pairwise_distinct() {
        let tags = [
            DomainTag::CHUNK,
            DomainTag::MANIFEST,
            DomainTag::BLOB,
            DomainTag::CONFLICT,
            DomainTag::ENVELOPE,
            DomainTag::FONT_METRICS,
            DomainTag::MANIFEST_ID,
            DomainTag::SYSTEM_VOICE,
            DomainTag::SYSTEM_PITCH,
            DomainTag::SYSTEM_ANOMALY,
        ];
        for (i, a) in tags.iter().enumerate() {
            for b in &tags[i + 1..] {
                assert_ne!(a, b, "duplicate domain tag {a:?}");
            }
        }
    }

    #[test]
    fn exact_tag_spellings_match_spec() {
        // Locked literally against Chapter 8 / Chapter 5 / Chapter 6.
        assert_eq!(DomainTag::CHUNK.as_bytes(), b"MUSCCHNK");
        assert_eq!(DomainTag::MANIFEST.as_bytes(), b"MUSCMANI");
        assert_eq!(DomainTag::BLOB.as_bytes(), b"MUSCBLOB");
        assert_eq!(DomainTag::CONFLICT.as_bytes(), b"MUSCCONF");
        assert_eq!(DomainTag::ENVELOPE.as_bytes(), b"MUSCENVH");
        assert_eq!(DomainTag::FONT_METRICS.as_bytes(), b"MUSCFNTM");
        assert_eq!(DomainTag::MANIFEST_ID.as_bytes(), b"MUSCMNIF");
        assert_eq!(DomainTag::SYSTEM_VOICE.as_bytes(), b"MUSCSVCE");
        assert_eq!(DomainTag::SYSTEM_PITCH.as_bytes(), b"MUSCSPCH");
        assert_eq!(DomainTag::SYSTEM_ANOMALY.as_bytes(), b"MUSCSANM");
        assert_eq!(&BUNDLE_MAGIC, b"MUSCBND\0");
        assert_eq!(&SUPERBLOCK_MAGIC, b"MUSCSUPR");
    }

    #[test]
    fn builtin_system_tags_are_not_extension_tags() {
        // They are system-derived (begin MUSCS)...
        assert!(DomainTag::SYSTEM_VOICE.is_system_derived());
        assert!(DomainTag::SYSTEM_PITCH.is_system_derived());
        assert!(DomainTag::SYSTEM_ANOMALY.is_system_derived());
        // ...but reserved built-ins, NOT extension-introduced.
        assert!(DomainTag::SYSTEM_VOICE.is_builtin());
        assert!(DomainTag::SYSTEM_ANOMALY.is_builtin());
        assert!(!DomainTag::SYSTEM_VOICE.is_extension_system_tag());
        assert!(!DomainTag::SYSTEM_PITCH.is_extension_system_tag());
        assert!(!DomainTag::SYSTEM_ANOMALY.is_extension_system_tag());
        // A non-system tag is neither.
        assert!(!DomainTag::CHUNK.is_system_derived());
        assert!(!DomainTag::CHUNK.is_extension_system_tag());
    }

    #[test]
    fn from_bytes_accepts_only_the_closed_vocabulary() {
        // Wrong prefix.
        assert!(DomainTag::from_bytes(*b"BAD_TAG!").is_none());
        assert!(DomainTag::from_bytes(*b"SHA2CHNK").is_none());
        // Right format prefix but unregistered, non-system domain.
        assert!(DomainTag::from_bytes(*b"MUSCWXYZ").is_none());
        // Non-ASCII payload byte (0xFF) is rejected even with a MUSC prefix.
        assert!(DomainTag::from_bytes([b'M', b'U', b'S', b'C', 0xFF, b'A', b'B', b'C']).is_none());
        // Control byte (NUL) is not printable ASCII.
        assert!(DomainTag::from_bytes(*b"MUSCS\0\0\0").is_none());
        // Built-in: accepted.
        assert_eq!(
            DomainTag::from_bytes(*b"MUSCCHNK").unwrap(),
            DomainTag::CHUNK
        );
        // Extension system tag: accepted.
        assert!(DomainTag::from_bytes(*b"MUSCSEXT")
            .unwrap()
            .is_extension_system_tag());
    }

    #[test]
    fn system_domain_tag_enforces_the_chapter5_rule() {
        // Must begin MUSCS.
        assert!(SystemDomainTag::new_extension(*b"MUSCXXXX").is_none());
        // Must not collide with a reserved built-in.
        assert!(SystemDomainTag::new_extension(*b"MUSCSVCE").is_none());
        // MUSCSANM is now a reserved built-in too (Pass 11): not extension-mintable.
        assert!(SystemDomainTag::new_extension(*b"MUSCSANM").is_none());
        // Non-ASCII rejected.
        assert!(
            SystemDomainTag::new_extension([b'M', b'U', b'S', b'C', b'S', 0xFF, b'A', b'B'])
                .is_none()
        );
        // File-format magic strings are reserved in the shared MUSC* namespace.
        assert!(SystemDomainTag::new_extension(SUPERBLOCK_MAGIC).is_none());
        // A genuine extension tag is accepted and classified correctly.
        let ext = SystemDomainTag::new_extension(*b"MUSCSEXT").unwrap();
        assert!(ext.tag().is_extension_system_tag());
        // Built-in system tags wrap; non-system tags do not.
        assert_eq!(SystemDomainTag::VOICE.tag(), DomainTag::SYSTEM_VOICE);
        assert_eq!(SystemDomainTag::ANOMALY.tag(), DomainTag::SYSTEM_ANOMALY);
        assert!(SystemDomainTag::new(DomainTag::SYSTEM_PITCH).is_some());
        assert!(SystemDomainTag::new(DomainTag::SYSTEM_ANOMALY).is_some());
        assert!(SystemDomainTag::new(DomainTag::CHUNK).is_none());
    }
}
