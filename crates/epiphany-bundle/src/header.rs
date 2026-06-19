//! The fixed 64-byte header (Chapter 8 §"The Fixed Header").
//!
//! The header sits at file offset zero, identifies the format, and locates the
//! superblock slots. It is written once at bundle creation and **never changes
//! thereafter** (except a major-version rewrite of the whole file), so commits
//! never touch it — which is exactly why a crash during a commit can never
//! corrupt the header.
//!
//! Layout (little-endian; see `DECISIONS.md` for the prototype byte-layout
//! choice that anticipates the deferred Binary Format companion):
//!
//! | range   | field                  |
//! |---------|------------------------|
//! | `0..8`  | magic `"MUSCBND\0"`    |
//! | `8..10` | `format_major` (u16)   |
//! | `10..12`| `format_minor` (u16)   |
//! | `12..16`| `header_length` (u32)  |
//! | `16..24`| `superblock_a_offset`  |
//! | `24..32`| `superblock_b_offset`  |
//! | `32..48`| `file_uuid` (16 bytes) |
//! | `48..60`| reserved (zero)        |
//! | `60..64`| `header_crc` (CRC-32C of `0..60`) |

use crate::codec::{DecodeError, Reader, Writer};
use crate::crc::crc32c;
use crate::error::BundleError;
use crate::ids::FileUuid;

/// The fixed header length, in bytes. Always 64 in this format version.
pub const HEADER_LEN: u64 = 64;

/// Offset of superblock slot A (immediately after the header).
pub const SLOT_A_OFFSET: u64 = 64;

/// Offset of superblock slot B (after slot A).
pub const SLOT_B_OFFSET: u64 = 320;

/// The format major version this crate writes and understands.
pub const FORMAT_MAJOR: u16 = 0;

/// The format minor version this crate writes.
pub const FORMAT_MINOR: u16 = 1;

/// Byte range covered by the header CRC: everything before the CRC field.
const HEADER_CRC_RANGE: usize = 60;

/// The fixed bundle header.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FixedHeader {
    /// Major format version (non-backward-compatible changes).
    pub format_major: u16,
    /// Minor format version (backward-compatible changes).
    pub format_minor: u16,
    /// Header length in bytes (currently always 64).
    pub header_length: u32,
    /// Offset of superblock slot A (currently always 64).
    pub superblock_a_offset: u64,
    /// Offset of superblock slot B (currently always 320).
    pub superblock_b_offset: u64,
    /// Physical-bundle UUID, set at creation, changed on Save As.
    pub file_uuid: FileUuid,
}

impl FixedHeader {
    /// Builds the canonical header for a freshly created bundle.
    pub fn new(file_uuid: FileUuid) -> Self {
        FixedHeader {
            format_major: FORMAT_MAJOR,
            format_minor: FORMAT_MINOR,
            header_length: HEADER_LEN as u32,
            superblock_a_offset: SLOT_A_OFFSET,
            superblock_b_offset: SLOT_B_OFFSET,
            file_uuid,
        }
    }

    /// Serializes to the fixed 64-byte form, computing and appending the CRC.
    pub fn encode(&self) -> [u8; HEADER_LEN as usize] {
        let mut w = Writer::with_capacity(HEADER_CRC_RANGE);
        w.put_bytes(&epiphany_determinism::BUNDLE_MAGIC);
        w.put_u16(self.format_major);
        w.put_u16(self.format_minor);
        w.put_u32(self.header_length);
        w.put_u64(self.superblock_a_offset);
        w.put_u64(self.superblock_b_offset);
        self.file_uuid.encode(&mut w);
        w.put_bytes(&[0u8; 12]); // reserved bytes 48..60
        debug_assert_eq!(w.len(), HEADER_CRC_RANGE);

        let mut buf = [0u8; HEADER_LEN as usize];
        buf[..HEADER_CRC_RANGE].copy_from_slice(w.as_bytes());
        let crc = crc32c(&buf[0..HEADER_CRC_RANGE]);
        buf[60..64].copy_from_slice(&crc.to_le_bytes());
        buf
    }

    /// Parses and validates a 64-byte header: magic, then CRC, then the
    /// version/length it can interpret (Chapter 8 §"The Fixed Header":
    /// *"Readers MUST verify the magic bytes and the header CRC before
    /// consulting any other part of the file"*; reserved bytes are ignored).
    pub fn decode(bytes: &[u8]) -> Result<Self, BundleError> {
        if bytes.len() < HEADER_LEN as usize {
            return Err(BundleError::HeaderCrcMismatch);
        }
        let buf = &bytes[..HEADER_LEN as usize];
        if buf[0..8] != epiphany_determinism::BUNDLE_MAGIC {
            return Err(BundleError::BadHeaderMagic);
        }
        let stored_crc = u32::from_le_bytes([buf[60], buf[61], buf[62], buf[63]]);
        if stored_crc != crc32c(&buf[0..HEADER_CRC_RANGE]) {
            return Err(BundleError::HeaderCrcMismatch);
        }

        // Magic and CRC verified; parse the fields from the CRC-covered region.
        let mut r = Reader::new(&buf[0..HEADER_CRC_RANGE]);
        let _magic = r.take_array::<8>()?;
        let format_major = r.get_u16()?;
        let format_minor = r.get_u16()?;
        if format_major != FORMAT_MAJOR {
            return Err(BundleError::UnsupportedFormatVersion {
                major: format_major,
                minor: format_minor,
            });
        }
        let header_length = r.get_u32()?;
        if header_length != HEADER_LEN as u32 {
            return Err(BundleError::UnsupportedHeaderLength {
                declared: header_length,
            });
        }
        let superblock_a_offset = r.get_u64()?;
        let superblock_b_offset = r.get_u64()?;
        // This format version fixes the slot offsets; `Bundle::open` reads them
        // at the constants, so a CRC-valid header that disagrees is foreign and
        // must be rejected rather than silently honored-then-ignored.
        if superblock_a_offset != SLOT_A_OFFSET || superblock_b_offset != SLOT_B_OFFSET {
            return Err(BundleError::Decode(DecodeError::Malformed(
                "header superblock offsets are not the fixed 64 and 320",
            )));
        }
        let file_uuid = FileUuid::decode(&mut r)?;
        // Remaining bytes are reserved and ignored.
        Ok(FixedHeader {
            format_major,
            format_minor,
            header_length,
            superblock_a_offset,
            superblock_b_offset,
            file_uuid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let h = FixedHeader::new(FileUuid([0xAB; 16]));
        let bytes = h.encode();
        assert_eq!(bytes.len(), 64);
        assert_eq!(FixedHeader::decode(&bytes).unwrap(), h);
    }

    #[test]
    fn reserved_bytes_are_zero_and_offsets_are_fixed() {
        let bytes = FixedHeader::new(FileUuid::ZERO).encode();
        assert_eq!(&bytes[48..60], &[0u8; 12]);
        assert_eq!(&bytes[0..8], &epiphany_determinism::BUNDLE_MAGIC);
        assert_eq!(u64::from_le_bytes(bytes[16..24].try_into().unwrap()), 64);
        assert_eq!(u64::from_le_bytes(bytes[24..32].try_into().unwrap()), 320);
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = FixedHeader::new(FileUuid::ZERO).encode();
        bytes[0] = b'X';
        assert!(matches!(
            FixedHeader::decode(&bytes),
            Err(BundleError::BadHeaderMagic)
        ));
    }

    #[test]
    fn a_corrupt_header_byte_fails_crc() {
        let mut bytes = FixedHeader::new(FileUuid([1; 16])).encode();
        bytes[40] ^= 0xFF; // flip a file_uuid byte
        assert!(matches!(
            FixedHeader::decode(&bytes),
            Err(BundleError::HeaderCrcMismatch)
        ));
    }

    #[test]
    fn reserved_bytes_are_ignored_on_read() {
        // A future minor version may use reserved bytes; current readers must
        // ignore them. They are *not* in the CRC-excluded region, though, so to
        // keep the header valid we recompute the CRC after setting them.
        let mut bytes = FixedHeader::new(FileUuid([2; 16])).encode();
        bytes[50] = 0x99;
        let crc = crc32c(&bytes[0..60]);
        bytes[60..64].copy_from_slice(&crc.to_le_bytes());
        // Still decodes; the reserved byte does not affect the parsed fields.
        assert!(FixedHeader::decode(&bytes).is_ok());
    }
}
