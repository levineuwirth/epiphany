//! Canonical byte encoding helpers.
//!
//! Chapter 8 §"Binary Format Companion" defers the exact byte-level layout
//! (varint conventions, field bit widths, struct alignment) to a separate
//! specification that does not yet exist. To make the bundle's atomic-commit,
//! crash-recovery, and re-serialization guarantees testable now, this crate
//! defines a concrete, fixed-convention canonical encoding (see `DECISIONS.md`,
//! and the parallel choice in `epiphany-core` DECISIONS P11-4). The conventions:
//!
//! * Integers are little-endian (matching the spec's hash-preimage convention,
//!   which serializes `uncompressed_length` as `to_le_bytes`, Chapter 8
//!   §"Domain-Separated Preimages").
//! * Variable-length fields are `u32` length-prefixed.
//! * `Option`s are a single presence byte (`0`/`1`) followed by the payload.
//! * Vectors are a `u32` count followed by each element, in canonical order
//!   (Appendix D §"Ordered Iteration"); ordering is the encoder's job, fixed at
//!   serialization time so re-encoding is byte-stable.
//!
//! [`Writer`] is infallible (it grows a `Vec`). [`Reader`] is *total* and
//! bounds-checked: every read either consumes exactly the bytes it needs or
//! returns a [`DecodeError`]. It never panics and never trusts a length prefix
//! before checking it against the bytes actually remaining, so a corrupt or
//! adversarial buffer cannot drive an over-allocation.

/// Why decoding a canonical byte buffer failed. Decoding is partial: not every
/// byte string is a valid structure, and invalid input is always reported,
/// never silently accepted (Appendix D: readers treat malformed canonical bytes
/// as corruption).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DecodeError {
    /// A read ran past the end of the buffer.
    UnexpectedEof { needed: usize, remaining: usize },
    /// Bytes remained after a fixed-shape structure was fully decoded.
    TrailingBytes { remaining: usize },
    /// An enum discriminant did not name a known variant.
    InvalidDiscriminant { what: &'static str, value: u64 },
    /// A length prefix exceeded the bytes actually present (corruption, or a
    /// truncated buffer). Caught *before* any allocation.
    LengthOverflow { declared: u64, remaining: usize },
    /// A presence byte for an `Option` was neither `0` nor `1`.
    InvalidPresenceByte(u8),
    /// A text field was not valid UTF-8.
    InvalidUtf8,
    /// A structural rule was violated (described by a stable, locale-independent
    /// reason string).
    Malformed(&'static str),
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::UnexpectedEof { needed, remaining } => {
                write!(
                    f,
                    "unexpected end of input: needed {needed}, had {remaining}"
                )
            }
            DecodeError::TrailingBytes { remaining } => {
                write!(f, "{remaining} trailing bytes after a fixed-shape value")
            }
            DecodeError::InvalidDiscriminant { what, value } => {
                write!(f, "invalid {what} discriminant {value}")
            }
            DecodeError::LengthOverflow {
                declared,
                remaining,
            } => {
                write!(
                    f,
                    "declared length {declared} exceeds {remaining} remaining bytes"
                )
            }
            DecodeError::InvalidPresenceByte(b) => write!(f, "invalid option presence byte {b}"),
            DecodeError::InvalidUtf8 => f.write_str("invalid UTF-8 in canonical text field"),
            DecodeError::Malformed(why) => write!(f, "malformed canonical bytes: {why}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// A growable canonical byte sink. Encoding is infallible and deterministic.
#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// A fresh writer.
    #[inline]
    pub fn new() -> Self {
        Writer { buf: Vec::new() }
    }

    /// A writer pre-sized for `cap` bytes.
    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        Writer {
            buf: Vec::with_capacity(cap),
        }
    }

    /// Consumes the writer, returning the encoded bytes.
    #[inline]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// The bytes written so far.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Bytes written so far.
    #[inline]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether nothing has been written.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    #[inline]
    pub fn put_u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    #[inline]
    pub fn put_u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    #[inline]
    pub fn put_u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    #[inline]
    pub fn put_u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    #[inline]
    pub fn put_u128(&mut self, v: u128) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    #[inline]
    pub fn put_i64(&mut self, v: i64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// Raw bytes with no length prefix (the caller knows the width).
    #[inline]
    pub fn put_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    /// A `u32`-length-prefixed byte string. A length that would overflow the
    /// `u32` prefix is rejected in **all** build profiles (a hard `assert!`, not
    /// a `debug_assert!`): aborting beats silently truncating the prefix and
    /// emitting corrupt bytes. v0 inputs are bounded far below 4 GiB by the
    /// reader resource limits, so this is unreachable in practice.
    #[inline]
    pub fn put_var_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        assert!(
            bytes.len() <= u32::MAX as usize,
            "var-bytes length {} overflows the u32 prefix",
            bytes.len()
        );
        self.put_u32(bytes.len() as u32);
        self.buf.extend_from_slice(bytes);
        self
    }

    /// A `bool` as a single `0`/`1` byte.
    #[inline]
    pub fn put_bool(&mut self, v: bool) -> &mut Self {
        self.put_u8(v as u8)
    }

    /// An optional value: a presence byte, then `encode` if present.
    #[inline]
    pub fn put_opt<T>(
        &mut self,
        value: &Option<T>,
        encode: impl FnOnce(&mut Self, &T),
    ) -> &mut Self {
        match value {
            Some(v) => {
                self.put_u8(1);
                encode(self, v);
            }
            None => {
                self.put_u8(0);
            }
        }
        self
    }

    /// A length-prefixed, canonically ordered sequence. The caller is
    /// responsible for having ordered `items` canonically before encoding.
    #[inline]
    pub fn put_seq<T>(&mut self, items: &[T], mut encode: impl FnMut(&mut Self, &T)) -> &mut Self {
        assert!(
            items.len() <= u32::MAX as usize,
            "sequence length {} overflows the u32 count",
            items.len()
        );
        self.put_u32(items.len() as u32);
        for item in items {
            encode(self, item);
        }
        self
    }
}

/// A bounds-checked cursor over canonical bytes. Every read is total: it either
/// consumes exactly the bytes it needs or returns [`DecodeError::UnexpectedEof`].
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Wraps a byte slice.
    #[inline]
    pub fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    /// Bytes not yet consumed.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    /// Errors unless the buffer is fully consumed. Call after decoding a
    /// fixed-shape structure to reject trailing bytes (the encoding is exact).
    #[inline]
    pub fn finish(&self) -> Result<(), DecodeError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes {
                remaining: self.remaining(),
            })
        }
    }

    #[inline]
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if n > self.remaining() {
            return Err(DecodeError::UnexpectedEof {
                needed: n,
                remaining: self.remaining(),
            });
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    #[inline]
    pub fn take_array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut arr = [0u8; N];
        arr.copy_from_slice(self.take(N)?);
        Ok(arr)
    }

    #[inline]
    pub fn get_u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    #[inline]
    pub fn get_u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take_array()?))
    }

    #[inline]
    pub fn get_u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take_array()?))
    }

    #[inline]
    pub fn get_u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take_array()?))
    }

    #[inline]
    pub fn get_u128(&mut self) -> Result<u128, DecodeError> {
        Ok(u128::from_le_bytes(self.take_array()?))
    }

    #[inline]
    pub fn get_i64(&mut self) -> Result<i64, DecodeError> {
        Ok(i64::from_le_bytes(self.take_array()?))
    }

    /// A `u32`-length-prefixed byte string, *borrowed* from the input rather
    /// than copied. The length prefix is checked against the bytes remaining
    /// before the slice is taken. The zero-copy counterpart of
    /// [`Reader::get_var_bytes`], for callers that need positions or slices
    /// into the original buffer (e.g. the per-envelope offsets the operation
    /// index records).
    pub fn get_var_slice(&mut self) -> Result<&'a [u8], DecodeError> {
        let len = self.get_u32()? as usize;
        if len > self.remaining() {
            return Err(DecodeError::LengthOverflow {
                declared: len as u64,
                remaining: self.remaining(),
            });
        }
        self.take(len)
    }

    /// A `u32`-length-prefixed byte string, copied out. The length prefix is
    /// checked against the bytes remaining *before* any allocation.
    pub fn get_var_bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        Ok(self.get_var_slice()?.to_vec())
    }

    /// A `u32`-length-prefixed UTF-8 string.
    pub fn get_string(&mut self) -> Result<String, DecodeError> {
        let bytes = self.get_var_bytes()?;
        String::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)
    }

    /// A `bool` from a single `0`/`1` byte.
    pub fn get_bool(&mut self) -> Result<bool, DecodeError> {
        match self.get_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(DecodeError::InvalidPresenceByte(other)),
        }
    }

    /// An optional value: a presence byte, then `decode` if present.
    pub fn get_opt<T>(
        &mut self,
        decode: impl FnOnce(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<Option<T>, DecodeError> {
        match self.get_u8()? {
            0 => Ok(None),
            1 => Ok(Some(decode(self)?)),
            other => Err(DecodeError::InvalidPresenceByte(other)),
        }
    }

    /// A length-prefixed sequence. The count is checked against the bytes
    /// remaining before allocating, so a corrupt count cannot drive the loop
    /// past the buffer (each element is at least one byte, so `count <=
    /// remaining`). The *reserved* capacity is additionally capped: a multi-byte
    /// element type would otherwise let `count` (bounded only by byte count)
    /// reserve several times the input size, so we grow from a small reservation
    /// instead.
    pub fn get_seq<T>(
        &mut self,
        mut decode: impl FnMut(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<Vec<T>, DecodeError> {
        const MAX_RESERVE: usize = 1024;
        let count = self.get_u32()? as usize;
        if count > self.remaining() {
            return Err(DecodeError::LengthOverflow {
                declared: count as u64,
                remaining: self.remaining(),
            });
        }
        let mut out = Vec::with_capacity(count.min(MAX_RESERVE));
        for _ in 0..count {
            out.push(decode(self)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_round_trip() {
        let mut w = Writer::new();
        w.put_u8(0x12)
            .put_u16(0x3456)
            .put_u32(0x789a_bcde)
            .put_u64(0x0102_0304_0506_0708)
            .put_u128(0x1)
            .put_i64(-5)
            .put_bool(true)
            .put_var_bytes(b"hello");
        let bytes = w.into_bytes();

        let mut r = Reader::new(&bytes);
        assert_eq!(r.get_u8().unwrap(), 0x12);
        assert_eq!(r.get_u16().unwrap(), 0x3456);
        assert_eq!(r.get_u32().unwrap(), 0x789a_bcde);
        assert_eq!(r.get_u64().unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(r.get_u128().unwrap(), 1);
        assert_eq!(r.get_i64().unwrap(), -5);
        assert!(r.get_bool().unwrap());
        assert_eq!(r.get_var_bytes().unwrap(), b"hello");
        assert!(r.finish().is_ok());
    }

    #[test]
    fn short_read_is_an_error_not_a_panic() {
        let mut r = Reader::new(&[0u8; 3]);
        assert_eq!(
            r.get_u64(),
            Err(DecodeError::UnexpectedEof {
                needed: 8,
                remaining: 3
            })
        );
    }

    #[test]
    fn corrupt_length_prefix_cannot_over_allocate() {
        // A 4-byte buffer claiming a 4-billion-byte payload must error, not OOM.
        let mut bytes = u32::MAX.to_le_bytes().to_vec();
        bytes.push(0); // one stray payload byte
        let mut r = Reader::new(&bytes);
        assert!(matches!(
            r.get_var_bytes(),
            Err(DecodeError::LengthOverflow { .. })
        ));
    }

    #[test]
    fn seq_and_opt_round_trip() {
        let mut w = Writer::new();
        w.put_seq(&[1u32, 2, 3], |w, v| {
            w.put_u32(*v);
        });
        w.put_opt(&Some(7u8), |w, v| {
            w.put_u8(*v);
        });
        w.put_opt(&None::<u8>, |w, v| {
            w.put_u8(*v);
        });
        let bytes = w.into_bytes();

        let mut r = Reader::new(&bytes);
        assert_eq!(r.get_seq(|r| r.get_u32()).unwrap(), vec![1, 2, 3]);
        assert_eq!(r.get_opt(|r| r.get_u8()).unwrap(), Some(7));
        assert_eq!(r.get_opt(|r| r.get_u8()).unwrap(), None);
        assert!(r.finish().is_ok());
    }

    #[test]
    fn trailing_bytes_rejected() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert_eq!(r.get_u8().unwrap(), 1);
        assert_eq!(r.finish(), Err(DecodeError::TrailingBytes { remaining: 2 }));
    }
}
