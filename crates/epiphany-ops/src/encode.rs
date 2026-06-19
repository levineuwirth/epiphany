//! Canonical-encoding helpers for the composite Chapter 6 types.
//!
//! The determinism crate fixes the canonical byte form of the *primitives*
//! (identifiers, hashes, `CanonicalF64`, `QuantizedCoord`) via
//! [`epiphany_determinism::CanonicalEncode`]; `epiphany-core` does the same for
//! the graph value types. The Chapter 6 types are *composites* of those — with
//! variable-length parts (vectors, the one text label) — so they need a
//! length-discipline that stays unambiguous and deterministic.
//!
//! The rules here are deliberately boring and total:
//!
//! * Integers are little-endian (the convention `epiphany-determinism`'s
//!   [`Preimage`](epiphany_determinism::Preimage) already uses for length and
//!   counter fields).
//! * Variable-length parts carry a `u32` little-endian length prefix, so a
//!   decoder never has to guess where a field ends and two distinct values can
//!   never share an encoding (Appendix D §"Canonical serialization
//!   determinism").
//! * Sequences carry a `u32` count, then each element. Fixed-width elements are
//!   written directly; variable-width elements are each length-prefixed.
//! * The single text field (a transaction label) is NFC-normalized before its
//!   UTF-8 bytes are length-prefixed (Appendix D §"Text and Unicode").
//!
//! These mirror Agent B/D's provisional encodings: a concrete, reversible
//! canonical form that predates the Binary Format companion (a Pass 11
//! candidate — see `DECISIONS.md`).

use epiphany_determinism::CanonicalEncode;
use unicode_normalization::UnicodeNormalization;

/// Appends a `u32` in little-endian order.
#[inline]
pub(crate) fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Appends a `u64` in little-endian order.
#[inline]
pub(crate) fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Appends a single tag/discriminant byte.
#[inline]
pub(crate) fn push_tag(out: &mut Vec<u8>, tag: u8) {
    out.push(tag);
}

/// Appends a boolean as a single canonical byte (`0` or `1`).
#[inline]
pub(crate) fn push_u8_bool(out: &mut Vec<u8>, b: bool) {
    out.push(b as u8);
}

/// Appends a `u32` little-endian length prefix. Panics in debug builds if the
/// length does not fit in `u32`; canonical structures this large are not
/// representable in the prototype and would indicate a logic error, not data.
#[inline]
pub(crate) fn push_len(out: &mut Vec<u8>, len: usize) {
    debug_assert!(len <= u32::MAX as usize, "canonical length exceeds u32");
    push_u32(out, len as u32);
}

/// Appends raw bytes with a `u32` length prefix.
#[inline]
pub(crate) fn push_lp_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    push_len(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// Appends a text field: NFC-normalized, UTF-8, with a `u32` length prefix
/// (Appendix D §"Text and Unicode": canonical text fields MUST be NFC).
#[inline]
pub(crate) fn push_str(out: &mut Vec<u8>, s: &str) {
    let nfc: String = s.nfc().collect();
    push_lp_bytes(out, nfc.as_bytes());
}

/// Appends a canonical-encodable value's bytes directly (no length prefix);
/// use for fixed-width primitives whose width is known to the decoder.
#[inline]
pub(crate) fn push_canon<T: CanonicalEncode>(out: &mut Vec<u8>, value: &T) {
    value.encode_canonical(out);
}

/// Appends a sequence of canonical-encodable values as `count` then each
/// element, every element length-prefixed so variable-width elements stay
/// unambiguous. The caller is responsible for having put `items` into the
/// normative iteration order *before* calling this (Appendix D §"Ordered
/// Iteration"); this helper preserves the given order and does not sort.
pub(crate) fn push_seq<T: CanonicalEncode>(out: &mut Vec<u8>, items: &[T]) {
    push_len(out, items.len());
    let mut scratch = Vec::new();
    for item in items {
        scratch.clear();
        item.encode_canonical(&mut scratch);
        push_lp_bytes(out, &scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefixing_disambiguates_concatenation() {
        // ("ab","") and ("a","b") must not collide once length-prefixed.
        let mut x = Vec::new();
        push_lp_bytes(&mut x, b"ab");
        push_lp_bytes(&mut x, b"");
        let mut y = Vec::new();
        push_lp_bytes(&mut y, b"a");
        push_lp_bytes(&mut y, b"b");
        assert_ne!(x, y);
    }

    #[test]
    fn nfc_normalizes_before_hashing() {
        // U+00E9 (é) vs U+0065 U+0301 (e + combining acute) are the same NFC.
        let mut a = Vec::new();
        push_str(&mut a, "\u{00e9}");
        let mut b = Vec::new();
        push_str(&mut b, "e\u{0301}");
        assert_eq!(a, b, "canonically-equivalent text must encode identically");
    }
}
