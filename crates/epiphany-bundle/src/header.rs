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
///
/// **Format epoch** (`spec/CONTRACT_FORMAT_EPOCH_MAJOR1.md`): a major-1
/// container is one whose every base-bearing commit was validated against a
/// supplied reduction authority. Major 0 is the pre-epoch **legacy** format —
/// still decoded, deliberately, but never trusted to carry a canonical base
/// (see [`FormatEpoch`]). A new major restarts minor numbering, which is why
/// [`FORMAT_MINOR`] resets to `0` here rather than continuing from legacy's
/// `1`.
pub const FORMAT_MAJOR: u16 = 1;

/// The format minor version this crate writes.
///
/// Reset to `0` by the major-1 epoch bump (`spec/CONTRACT_FORMAT_EPOCH_MAJOR1.md`
/// pin 1): a new major restarts minor numbering.
pub const FORMAT_MINOR: u16 = 0;

/// Which format-epoch a decoded header belongs to
/// (`spec/CONTRACT_FORMAT_EPOCH_MAJOR1.md` pin 2). A named enum rather than a
/// boolean so the legacy case is a value the type system carries — every
/// consumer of the epoch matches this, rather than re-deriving
/// `format_major == 0` at each use site.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FormatEpoch {
    /// Format major 0: decoded deliberately, for backward compatibility, but
    /// never trusted to carry a canonical base (the epoch is not retroactive
    /// — `CONTRACT_FORMAT_EPOCH_MAJOR1.md` pin 3, row 2/3).
    Legacy,
    /// Format major 1: the current epoch. Every base-bearing commit is meant
    /// to be validated against a supplied reduction authority — the
    /// capability itself lands with P13-S27; until then this rung refuses
    /// base introduction outright (pin 3a).
    Current,
}

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
    /// The format epoch this header's major classifies to (pin 2). Carried
    /// on the header rather than re-derived at each call site.
    pub epoch: FormatEpoch,
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
            epoch: FormatEpoch::Current,
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
        // Pin 2's explicit three-way classification: 0 is legacy (decoded
        // deliberately, marked as such), FORMAT_MAJOR is current, anything
        // else is unsupported — unchanged from the pre-epoch exact-major
        // rejection for that third arm.
        let epoch = match format_major {
            0 => FormatEpoch::Legacy,
            FORMAT_MAJOR => FormatEpoch::Current,
            _ => {
                return Err(BundleError::UnsupportedFormatVersion {
                    major: format_major,
                    minor: format_minor,
                })
            }
        };
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
            epoch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_epoch_constants_are_major_1_minor_0() {
        // Gate 7 (`CONTRACT_FORMAT_EPOCH_MAJOR1.md` pin 1): the epoch bump,
        // asserted in a test rather than only by reading the constants.
        assert_eq!(FORMAT_MAJOR, 1);
        assert_eq!(FORMAT_MINOR, 0);
    }

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
    fn an_unknown_major_is_still_unsupported_format_version() {
        // Pin 2's third arm: neither 0 (legacy) nor FORMAT_MAJOR (current) —
        // still hard-rejected, exactly as the pre-epoch exact-major check was
        // (`CONTRACT_FORMAT_EPOCH_MAJOR1.md`).
        let mut bytes = FixedHeader::new(FileUuid::ZERO).encode();
        bytes[8..10].copy_from_slice(&2u16.to_le_bytes()); // format_major = 2
        let crc = crc32c(&bytes[0..60]);
        bytes[60..64].copy_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            FixedHeader::decode(&bytes),
            Err(BundleError::UnsupportedFormatVersion { major: 2, minor: 0 })
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
