//! Bundle error and integrity-anomaly types.
//!
//! The spec draws a sharp line (Chapter 8 §"Superblock Selection",
//! §"Crash Recovery", §"Canonical and Non-Canonical Manifest Roots") between
//! two failure categories, and so does this module:
//!
//! * **Hard errors** ([`BundleError`]) — the bundle cannot be opened or a
//!   *canonical* chunk failed verification. *"if neither superblock validates,
//!   the file is corrupt and MUST be reported as such"*; failed verification of
//!   a canonical chunk *"is corruption and MUST be surfaced as a hard error."*
//! * **Integrity anomalies** ([`IntegrityAnomaly`]) — the file is openable but
//!   structurally suspicious (e.g. a generation gap > 1). The reader *"MAY
//!   continue in read-only recovery mode … it MUSTNOT silently treat the file
//!   as normal."* These are returned alongside a successfully opened (read-only)
//!   bundle, not as errors.

use crate::codec::DecodeError;
use crate::ids::SchemaVersion;
use epiphany_determinism::ContentHash;

/// A hard bundle failure: the file is unopenable, or a canonical chunk is
/// corrupt. Recoverable conditions (a torn inactive slot, unreachable garbage)
/// are *not* errors — they are handled silently by superblock selection.
#[derive(Debug)]
pub enum BundleError {
    /// An underlying storage operation failed.
    Io(std::io::Error),

    /// The fixed header's magic bytes were not `"MUSCBND\0"`. Not an Epiphany
    /// bundle (Chapter 8 §"The Fixed Header": readers verify magic first).
    BadHeaderMagic,

    /// The fixed header's CRC did not match its contents (the header is torn or
    /// corrupt). Readers verify the header CRC before consulting anything else.
    HeaderCrcMismatch,

    /// The header declared a length this format version cannot interpret.
    UnsupportedHeaderLength { declared: u32 },

    /// The header declared a major format version this reader does not support.
    UnsupportedFormatVersion { major: u16, minor: u16 },

    /// Neither superblock slot was valid for ordinary selection: the file is
    /// corrupt (Chapter 8 §"Superblock Selection", step 6). Readers MUST surface
    /// this and MUSTNOT synthesize state.
    NoValidSuperblock,

    /// The active superblock's manifest chunk failed BLAKE3 verification: a
    /// canonical chunk is corrupt, which is a hard error.
    ManifestHashMismatch {
        expected: ContentHash,
        actual: ContentHash,
    },

    /// A canonical chunk's recomputed hash did not match its declared hash
    /// (Chapter 8 §"Chunks": canonical-chunk hash failure is hard corruption).
    ChunkHashMismatch {
        expected: ContentHash,
        actual: ContentHash,
    },

    /// A chunk's payload length did not match its declared `uncompressed_length`
    /// (Chapter 8: length mismatch is corruption).
    ChunkLengthMismatch { expected: u64, actual: u64 },

    /// A chunk reference pointed outside the bundle's bytes (or into the fixed
    /// prelude, which holds no chunks).
    ChunkOutOfBounds {
        offset: u64,
        length: u64,
        file_len: u64,
    },

    /// The selected manifest's self-declared generation did not match the
    /// superblock that referenced it: structural corruption / a non-conforming
    /// writer (Chapter 8 §"The Manifest": the manifest's `generation` matches the
    /// referencing superblock).
    GenerationMismatch { superblock: u64, manifest: u64 },

    /// A declared length exceeded the reader's resource-limit policy (Chapter 8
    /// §"Blobs": *"uncompressed length MUST be checked against the reader's
    /// policy before decompression begins"*). Checked before any allocation, so
    /// an untrusted length in a (possibly sparse) file cannot drive an OOM.
    ResourceLimitExceeded { length: u64, limit: u64 },

    /// A compressed chunk was encountered. v0 writes only uncompressed chunks
    /// and does not implement decompression (QUICKSTART: zstd is deferred).
    UnsupportedCompression,

    /// A chunk declared a schema major version this reader cannot parse
    /// (Chapter 8 §"Schema Versioning").
    UnsupportedSchemaVersion { version: SchemaVersion },

    /// The generation counter is exhausted (`u64::MAX`): no further commit can
    /// allocate a higher generation. Surfaced instead of overflow-panicking.
    GenerationExhausted,

    /// A structured payload (manifest, superblock, etc.) failed to decode.
    Decode(DecodeError),

    /// An edit was attempted on a bundle opened read-only (an unknown required
    /// extension, or a recovery/anomaly open). Chapter 8 §"Behavior Under
    /// Unknown Extensions" / §"Superblock Selection".
    ReadOnly,
}

impl core::fmt::Display for BundleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BundleError::Io(e) => write!(f, "bundle I/O error: {e}"),
            BundleError::BadHeaderMagic => f.write_str("not an Epiphany bundle (bad header magic)"),
            BundleError::HeaderCrcMismatch => {
                f.write_str("fixed header CRC mismatch (corrupt header)")
            }
            BundleError::UnsupportedHeaderLength { declared } => {
                write!(f, "unsupported header length {declared}")
            }
            BundleError::UnsupportedFormatVersion { major, minor } => {
                write!(f, "unsupported format version {major}.{minor}")
            }
            BundleError::NoValidSuperblock => f.write_str("no valid superblock: bundle is corrupt"),
            BundleError::ManifestHashMismatch { expected, actual } => write!(
                f,
                "manifest hash mismatch: superblock declared {expected:?}, computed {actual:?}"
            ),
            BundleError::ChunkHashMismatch { expected, actual } => write!(
                f,
                "canonical chunk hash mismatch: declared {expected:?}, computed {actual:?}"
            ),
            BundleError::ChunkLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "chunk length mismatch: declared {expected}, got {actual}"
                )
            }
            BundleError::GenerationMismatch {
                superblock,
                manifest,
            } => write!(
                f,
                "manifest generation {manifest} does not match superblock generation {superblock}"
            ),
            BundleError::ChunkOutOfBounds {
                offset,
                length,
                file_len,
            } => write!(
                f,
                "chunk ref [{offset}, {offset}+{length}) lies outside the {file_len}-byte file"
            ),
            BundleError::ResourceLimitExceeded { length, limit } => {
                write!(
                    f,
                    "declared length {length} exceeds the reader limit {limit}"
                )
            }
            BundleError::UnsupportedCompression => {
                f.write_str("compressed chunk encountered; v0 supports only uncompressed chunks")
            }
            BundleError::UnsupportedSchemaVersion { version } => {
                write!(
                    f,
                    "unsupported schema version {}.{}",
                    version.major, version.minor
                )
            }
            BundleError::GenerationExhausted => {
                f.write_str("generation counter exhausted (u64::MAX); cannot commit")
            }
            BundleError::Decode(e) => write!(f, "decode error: {e}"),
            BundleError::ReadOnly => f.write_str("bundle is open read-only; edits are refused"),
        }
    }
}

impl std::error::Error for BundleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BundleError::Io(e) => Some(e),
            BundleError::Decode(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BundleError {
    fn from(e: std::io::Error) -> Self {
        BundleError::Io(e)
    }
}

impl From<DecodeError> for BundleError {
    fn from(e: DecodeError) -> Self {
        BundleError::Decode(e)
    }
}

/// A structural anomaly that does *not* prevent opening but does forbid treating
/// the bundle as normal (Chapter 8 §"Superblock Selection"). Surfaced alongside
/// a read-only open, never thrown as an error. Kept distinct from semantic
/// `ConflictKind`/`IntegrityAnomaly` types in `epiphany-ops`: those are facts
/// about canonical *state*; these are facts about the physical *bundle*.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IntegrityAnomaly {
    /// Both slots are valid but their generations differ by more than one
    /// (Chapter 8 §"Superblock Selection", step 4): possible tampering, an
    /// accidental merge, or a non-conforming writer. Recovery continues
    /// read-only at the highest-generation valid slot.
    GenerationGap { active: u64, other: u64 },

    /// Both slots are valid at the *same* generation but reference *different*
    /// manifests. The spec's selection rule has no tie-break for equal
    /// generations with divergent content; this crate treats it as an anomaly
    /// and opens read-only at slot A (see `DECISIONS.md`, Pass 11 candidate).
    DivergentSameGeneration { generation: u64 },

    /// A slot carried a non-`Committed` commit state (Chapter 8
    /// §"Superblock Selection", step 3): a crashed or non-conforming writer.
    /// The slot is excluded from ordinary selection; its presence is reported.
    NonCommittedSlot,

    /// The manifest declares a `required` extension this implementation does not
    /// understand (Chapter 8 §"Behavior Under Unknown Extensions"). v0 supports
    /// no extensions, so any required extension is unknown; the bundle opens
    /// strictly read-only.
    UnknownRequiredExtension,

    /// The active superblock names a profile this implementation does not
    /// understand — a `Custom` registry profile, an unsupported profile major
    /// version, or one demanding a block bound beyond the reader's limit
    /// (Chapter 8 §"Format Profiles"). The bundle opens read-only.
    UnsupportedProfile,
}

impl core::fmt::Display for IntegrityAnomaly {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IntegrityAnomaly::GenerationGap { active, other } => write!(
                f,
                "superblock generation gap > 1 (active {active}, other {other}); read-only recovery"
            ),
            IntegrityAnomaly::DivergentSameGeneration { generation } => write!(
                f,
                "both slots valid at generation {generation} with divergent manifests; read-only recovery"
            ),
            IntegrityAnomaly::NonCommittedSlot => {
                f.write_str("a superblock slot was not in the Committed state")
            }
            IntegrityAnomaly::UnknownRequiredExtension => {
                f.write_str("an unknown required extension forces read-only mode")
            }
            IntegrityAnomaly::UnsupportedProfile => {
                f.write_str("the active profile is unsupported; opened read-only")
            }
        }
    }
}
