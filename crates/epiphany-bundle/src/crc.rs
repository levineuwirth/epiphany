//! CRC-32C (Castagnoli) checksums for the fixed prelude.
//!
//! Chapter 8 specifies CRC-32C for both the fixed header (bytes 60..64, over
//! bytes 0..60) and each superblock slot (the trailing 4 bytes, over all
//! preceding bytes). The checksum's job is narrow but critical: it lets a
//! reader *reject a torn superblock write* (Chapter 8 §"Crash Recovery": "The
//! CRC check rejects torn writes. Recovery falls back to the other slot"). It
//! is an integrity check against partial/garbled writes, not a security MAC.
//!
//! This is the standard reflected CRC-32C: polynomial `0x1EDC6F41` (reflected
//! `0x82F63B78`), initial value `0xFFFFFFFF`, input and output reflected, final
//! XOR `0xFFFFFFFF`. The check value for the ASCII string `"123456789"` is
//! `0xE3069283`, which the unit tests pin.
//!
//! The 256-entry lookup table is built at compile time by a `const fn`, so no
//! second hashing dependency is pulled in (the workspace keeps `blake3` as the
//! sole content-hash dependency; CRC-32C is a separate, non-content-hash
//! integrity primitive owned here).

/// Reflected CRC-32C polynomial.
const POLY: u32 = 0x82F6_3B78;

/// The lookup table, generated once at compile time.
const TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// Computes the CRC-32C of `data`.
#[inline]
pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ TABLE[idx];
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_vector_matches_castagnoli() {
        // The canonical CRC-32C check value for "123456789".
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn empty_input_is_zero() {
        // CRC-32C of the empty string is 0 with these parameters.
        assert_eq!(crc32c(b""), 0);
    }

    #[test]
    fn a_single_flipped_bit_changes_the_crc() {
        let mut bytes = [0x55u8; 64];
        let base = crc32c(&bytes);
        bytes[37] ^= 0x01;
        assert_ne!(crc32c(&bytes), base, "CRC must detect a one-bit change");
    }

    #[test]
    fn detects_truncation_of_a_superblock_sized_buffer() {
        // The torn-write case the format relies on: a 256-byte slot whose tail
        // was not fully persisted must not checksum the same as the full slot.
        let full = [0xABu8; 256];
        let torn = &full[..200];
        assert_ne!(crc32c(&full), crc32c(torn));
    }
}
