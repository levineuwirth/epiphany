//! An operation-envelope block held to the supported-layout boundary (Binary
//! Format companion §"The Chunk-Level Gate", `req:binfmt:op-block-boundary`).
//!
//! Before the product's 1.0 release nothing is migrated: what a reader
//! supports is what the current writer writes, and anything else is refused by
//! name. For an operation block that is every envelope decoding under the live
//! layouts, and the block stamped exactly as minimal stamping derives from
//! them, major and minor. [`decode_operation_block`] is the one check; both
//! production readers, the editor's document layer and the text projection,
//! call it on every block they read.
//!
//! The declared version arrives as numbers ([`BlockStamp`]), not as the bundle's
//! `SchemaVersion`, because this crate does not depend on `epiphany-bundle`.
//! `epiphany-testkit` holds [`BlockStamp::derive`] and the bundle's
//! `SchemaVersion::for_major_at_epoch` to one answer.

use crate::envdecode::{decode_envelope, EnvelopeDecodeError};
use crate::envelope::OperationEnvelope;
use crate::payload::operation_block_introduced_minor;

/// The highest schema-minor epoch this build assigns to any appended
/// discriminant. A block declaring a later minor was written by a later build:
/// it is refused before any of its envelopes is decoded
/// ([`OperationBlockError::UnsupportedSchemaMinor`]), so a discriminant this
/// build does not know reads as a version boundary rather than as corruption.
/// An append at a new epoch raises it in the same change.
pub const HIGHEST_SCHEMA_MINOR: u16 = 16;

/// An operation block's schema version: the major, then the minor.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BlockStamp {
    /// The schema major.
    pub major: u16,
    /// The schema minor.
    pub minor: u16,
}

impl BlockStamp {
    /// The stamp minimal stamping derives for a block carrying `envelopes`:
    /// the highest major among them, and the highest epoch they emit or the
    /// major's baseline minor, whichever is greater (1 for major 0, 0 for every
    /// later major).
    pub fn derive(envelopes: &[OperationEnvelope]) -> Self {
        let major = envelopes
            .iter()
            .map(OperationEnvelope::schema_major)
            .max()
            .unwrap_or(0);
        let baseline = if major == 0 { 1 } else { 0 };
        let minor = match operation_block_introduced_minor(envelopes) {
            Some(epoch) if epoch > baseline => epoch,
            _ => baseline,
        };
        BlockStamp { major, minor }
    }
}

impl core::fmt::Display for BlockStamp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Why an operation block is outside the supported-layout boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OperationBlockError {
    /// The block's minor is above [`HIGHEST_SCHEMA_MINOR`]: a later build wrote
    /// it. Read before any envelope.
    UnsupportedSchemaMinor {
        /// The block's declared major.
        major: u16,
        /// The block's declared minor.
        minor: u16,
    },
    /// Every envelope decodes, but the block is not stamped as minimal stamping
    /// derives from them.
    MisStamped {
        /// The version the block declares.
        declared: BlockStamp,
        /// The version its envelopes derive.
        derived: BlockStamp,
    },
    /// An envelope does not decode under the live layouts. A block whose hash
    /// verified is not corrupt, so it is named by its declared version, and by
    /// the value where this build recognizes the historical layout (a tuplet
    /// written before schema major 4).
    UnsupportedLayout {
        /// The version the block declares.
        declared: BlockStamp,
        /// The value whose historical layout was recognized, if one was.
        value: Option<&'static str>,
    },
}

impl core::fmt::Display for OperationBlockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedSchemaMinor { major, minor } => write!(
                f,
                "an operation block is at schema {major}.{minor}, a minor later than this build's \
                 {HIGHEST_SCHEMA_MINOR}"
            ),
            Self::MisStamped { declared, derived } => write!(
                f,
                "an operation block is stamped {declared}, where its operations derive {derived}"
            ),
            Self::UnsupportedLayout {
                declared,
                value: Some(value),
            } => write!(
                f,
                "an operation block stamped {declared} carries a {value} in a layout this build \
                 does not read"
            ),
            Self::UnsupportedLayout {
                declared,
                value: None,
            } => write!(
                f,
                "an operation block stamped {declared} carries an operation in a layout this \
                 build does not read"
            ),
        }
    }
}

impl std::error::Error for OperationBlockError {}

/// Decodes the envelopes of one operation block, holding the block to the
/// supported-layout boundary: its minor no later than [`HIGHEST_SCHEMA_MINOR`],
/// every envelope decoding under the live layouts, and `declared` equal to
/// what minimal stamping derives from them ([`BlockStamp::derive`]).
pub fn decode_operation_block(
    envelopes: &[Vec<u8>],
    declared: BlockStamp,
) -> Result<Vec<OperationEnvelope>, OperationBlockError> {
    if declared.minor > HIGHEST_SCHEMA_MINOR {
        return Err(OperationBlockError::UnsupportedSchemaMinor {
            major: declared.major,
            minor: declared.minor,
        });
    }
    let decoded = envelopes
        .iter()
        .map(|bytes| decode_envelope(bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| OperationBlockError::UnsupportedLayout {
            declared,
            value: match e {
                EnvelopeDecodeError::UnsupportedLayout(value) => Some(value),
                _ => None,
            },
        })?;
    let derived = BlockStamp::derive(&decoded);
    if derived != declared {
        return Err(OperationBlockError::MisStamped { declared, derived });
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::gen_envelope_set;
    use epiphany_determinism::fuzz::SplitMix64;
    use epiphany_determinism::CanonicalEncode;

    fn bytes(envelopes: &[OperationEnvelope]) -> Vec<Vec<u8>> {
        envelopes.iter().map(|e| e.to_canonical_bytes()).collect()
    }

    /// A block stamped as its envelopes derive decodes to them, at every
    /// version a generated set reaches.
    #[test]
    fn a_block_stamped_as_derived_decodes() {
        let mut rng = SplitMix64::new(0x5B02);
        let mut stamps = std::collections::BTreeSet::new();
        for _ in 0..64 {
            let envelopes = gen_envelope_set(&mut rng, 24);
            for chunk in envelopes.chunks(5) {
                let stamp = BlockStamp::derive(chunk);
                stamps.insert(stamp);
                assert_eq!(
                    decode_operation_block(&bytes(chunk), stamp).as_deref(),
                    Ok(chunk)
                );
            }
        }
        assert!(
            stamps.iter().any(|s| s.major == 0) && stamps.iter().any(|s| s.major == 2),
            "fixture reach: {stamps:?}"
        );
    }

    /// The minor is a gate: a block declaring a later minor than this build
    /// assigns is refused before any envelope is read, even one that would not
    /// decode at all.
    #[test]
    fn a_later_minor_is_refused_before_any_envelope() {
        let declared = BlockStamp {
            major: 0,
            minor: HIGHEST_SCHEMA_MINOR + 1,
        };
        assert_eq!(
            decode_operation_block(&[vec![0xFF; 3]], declared),
            Err(OperationBlockError::UnsupportedSchemaMinor {
                major: 0,
                minor: HIGHEST_SCHEMA_MINOR + 1,
            })
        );
        // At this build's last epoch the same bytes reach the decoder.
        assert!(matches!(
            decode_operation_block(
                &[vec![0xFF; 3]],
                BlockStamp {
                    major: 0,
                    minor: HIGHEST_SCHEMA_MINOR,
                }
            ),
            Err(OperationBlockError::UnsupportedLayout { value: None, .. })
        ));
    }

    /// A block stamped above or below its derivation is refused, though every
    /// envelope in it decodes.
    #[test]
    fn a_block_stamped_otherwise_is_refused() {
        let mut rng = SplitMix64::new(0x5B03);
        let envelopes = gen_envelope_set(&mut rng, 12);
        let derived = BlockStamp::derive(&envelopes);
        assert!(derived.minor > 0, "fixture reach: {derived}");
        for declared in [
            BlockStamp {
                major: derived.major + 1,
                minor: derived.minor,
            },
            BlockStamp {
                major: derived.major,
                minor: derived.minor + 1,
            },
            BlockStamp {
                major: derived.major,
                minor: derived.minor - 1,
            },
        ] {
            assert_eq!(
                decode_operation_block(&bytes(&envelopes), declared),
                Err(OperationBlockError::MisStamped { declared, derived })
            );
        }
    }

    /// The highest epoch is the last one any vocabulary assigns: no tag's is
    /// later, and the newest appended variants reach it.
    #[test]
    fn the_highest_epoch_is_the_last_one_assigned() {
        use crate::effect::PreconditionFailureReason as R;
        use crate::payload::OperationKindTag;
        let newest = OperationKindTag::PAYLOAD_FREE
            .iter()
            .filter_map(OperationKindTag::introduced_minor)
            .chain(
                [R::SlotOccupied, R::HomeIsOwnStaff, R::RegionsOverlap]
                    .iter()
                    .filter_map(R::introduced_minor),
            )
            .max();
        assert_eq!(newest, Some(HIGHEST_SCHEMA_MINOR));
    }

    /// Minimal stamping keeps a major's baseline minor: 1 at major 0, 0 above.
    #[test]
    fn an_empty_or_baseline_block_keeps_its_major_baseline() {
        assert_eq!(BlockStamp::derive(&[]), BlockStamp { major: 0, minor: 1 });
    }
}
