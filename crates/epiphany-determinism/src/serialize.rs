//! Canonical encode/decode for the determinism-layer value types.
//!
//! Appendix D §"Canonical serialization determinism": the same canonical
//! state must produce identical bytes, and identical bytes must decode to the
//! same canonical value. This module gives the determinism-owned types a
//! minimal, fixed-width canonical byte form and the inverse, plus the
//! round-trip property the fuzz harness exercises (the Agent A hand-off gate).
//!
//! The full bundle wire format lives in `epiphany-bundle` (Agent D) and its
//! Binary Format companion; this trait covers only the primitives this crate
//! defines.

use crate::coord::QuantizedCoord;
use crate::domain::DomainTag;
use crate::float::CanonicalF64;
use crate::hash::{ChunkId, ContentHash};

/// Serializes a value to its canonical byte form. Encoding is total and
/// deterministic: a given value always produces the same bytes.
pub trait CanonicalEncode {
    /// Appends the canonical bytes of `self` to `out`.
    fn encode_canonical(&self, out: &mut Vec<u8>);

    /// The canonical bytes of `self` as a fresh `Vec`.
    fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_canonical(&mut out);
        out
    }
}

/// Reconstructs a value from its canonical byte form. Decoding is *partial*:
/// not every byte string is a valid canonical value (e.g. NaN bytes for a
/// [`CanonicalF64`]), and invalid input is reported, never silently accepted.
pub trait CanonicalDecode: Sized {
    /// Decodes exactly the canonical form produced by [`CanonicalEncode`].
    /// Trailing bytes are an error: the encoding is fixed-width, so the input
    /// length must match exactly.
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError>;
}

/// Why a canonical decode failed.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DecodeError {
    /// The input length did not match the fixed width of the target type.
    UnexpectedLength { expected: usize, actual: usize },

    /// A floating-point field decoded to NaN or infinity, which canonical
    /// state forbids (Appendix D). Readers must treat this as data corruption.
    NonFiniteFloat,

    /// An 8-byte field did not form a valid [`crate::DomainTag`] from the
    /// closed `MUSC*` vocabulary. Treated as corruption / foreign data.
    MalformedDomainTag,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::UnexpectedLength { expected, actual } => {
                write!(f, "expected {expected} canonical bytes, got {actual}")
            }
            DecodeError::NonFiniteFloat => {
                f.write_str("non-finite float in canonical bytes (corruption)")
            }
            DecodeError::MalformedDomainTag => {
                f.write_str("8 bytes did not form a valid Epiphany domain tag")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

/// Reads exactly `N` bytes, erroring if the length is wrong.
fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], DecodeError> {
    if bytes.len() != N {
        return Err(DecodeError::UnexpectedLength {
            expected: N,
            actual: bytes.len(),
        });
    }
    let mut buf = [0u8; N];
    buf.copy_from_slice(bytes);
    Ok(buf)
}

impl CanonicalEncode for QuantizedCoord {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
}
impl CanonicalDecode for QuantizedCoord {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        Ok(QuantizedCoord::from_le_bytes(fixed::<8>(bytes)?))
    }
}

impl CanonicalEncode for CanonicalF64 {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
}
impl CanonicalDecode for CanonicalF64 {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        CanonicalF64::from_le_bytes(fixed::<8>(bytes)?).ok_or(DecodeError::NonFiniteFloat)
    }
}

impl CanonicalEncode for ContentHash {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }
}
impl CanonicalDecode for ContentHash {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        Ok(ContentHash(fixed::<32>(bytes)?))
    }
}

impl CanonicalEncode for ChunkId {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }
}
impl CanonicalDecode for ChunkId {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        Ok(ChunkId(ContentHash(fixed::<32>(bytes)?)))
    }
}

impl CanonicalEncode for DomainTag {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }
}
impl CanonicalDecode for DomainTag {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        DomainTag::from_bytes(fixed::<8>(bytes)?).ok_or(DecodeError::MalformedDomainTag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantized_coord_round_trips() {
        let q = QuantizedCoord::from_units(-987_654);
        assert_eq!(
            QuantizedCoord::decode_canonical(&q.to_canonical_bytes()).unwrap(),
            q
        );
    }

    #[test]
    fn canonical_f64_rejects_non_finite_bytes() {
        let nan = f64::NAN.to_le_bytes().to_vec();
        assert_eq!(
            CanonicalF64::decode_canonical(&nan),
            Err(DecodeError::NonFiniteFloat)
        );
    }

    #[test]
    fn domain_tag_decode_rejects_foreign_bytes() {
        assert_eq!(
            DomainTag::decode_canonical(b"BAD_TAG!"),
            Err(DecodeError::MalformedDomainTag)
        );
        assert_eq!(
            DomainTag::decode_canonical(b"MUSCCHNK").unwrap(),
            DomainTag::CHUNK
        );
    }

    #[test]
    fn wrong_length_is_an_error() {
        assert_eq!(
            ContentHash::decode_canonical(&[0u8; 31]),
            Err(DecodeError::UnexpectedLength {
                expected: 32,
                actual: 31
            })
        );
    }

    #[test]
    fn re_encode_is_byte_identical() {
        let id = ChunkId(ContentHash([7u8; 32]));
        let bytes = id.to_canonical_bytes();
        let decoded = ChunkId::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded.to_canonical_bytes(), bytes);
    }
}
