//! Delegates the bundled Bravura outline lookup to `epiphany-glyphs`, the
//! shared typed glyph-asset seam (Editor T4-pre W2): the outline table used
//! to live here privately (`outline()` was `pub(crate)`, with no callers
//! outside this crate); it now lives in `epiphany-glyphs` so a future canvas
//! renderer can depend on the same typed geometry without depending on this
//! SVG renderer. This crate's own outline-data tests (sortedness, pipeline
//! coverage, metric/outline bbox containment) moved with the table; what
//! remains here is font-subset-specific (that generated file stays in this
//! crate — W2 pin 2: an embeddable font subset is a renderer concern, not a
//! shared asset).
//!
//! Byte-neutrality (W2 pin 5): `svg.rs` still reads `outline(name).path` —
//! the *stored* `d` string — directly into the emitted SVG. It never routes
//! through `epiphany-glyphs`'s typed-path parser/re-emitter, so every SVG
//! golden and layout-conformance byte is unaffected by this delegation.

pub(crate) use epiphany_glyphs::outline;

/// How many glyph outlines are bundled. Re-exported from `epiphany-glyphs`
/// even though nothing outside `render-svg` calls it today — an API that
/// costs one `pub use` is not worth breaking (W2 pin 2).
pub fn bundled_glyph_count() -> usize {
    epiphany_glyphs::bundled_glyph_count()
}

/// The SMuFL codepoint of a bundled glyph name, if bundled. Used by the
/// embedded-font render mode (which references glyphs by codepoint) and for
/// debugging glyph identity.
pub fn smufl_codepoint(name: &str) -> Option<u32> {
    epiphany_glyphs::smufl_codepoint(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal RFC-4648 base64 decoder for the integrity test (the crate has no
    /// base64 dependency); skips non-alphabet bytes, stops at padding.
    fn decode_base64(s: &str) -> Vec<u8> {
        fn val(c: u8) -> Option<u8> {
            match c {
                b'A'..=b'Z' => Some(c - b'A'),
                b'a'..=b'z' => Some(c - b'a' + 26),
                b'0'..=b'9' => Some(c - b'0' + 52),
                b'+' => Some(62),
                b'/' => Some(63),
                _ => None,
            }
        }
        let mut out = Vec::new();
        let (mut buf, mut bits) = (0u32, 0u32);
        for &c in s.as_bytes() {
            if c == b'=' {
                break;
            }
            let Some(v) = val(c) else { continue };
            buf = (buf << 6) | u32::from(v);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
            }
        }
        out
    }

    #[test]
    fn embedded_font_payload_is_a_locked_valid_otf() {
        use crate::font_subset_generated::{
            BRAVURA_SUBSET_BLAKE3, BRAVURA_SUBSET_LEN, BRAVURA_SUBSET_OTF_BASE64,
        };
        let bytes = decode_base64(BRAVURA_SUBSET_OTF_BASE64);
        // Length + signature: a truncated or non-OTF payload fails here, not later
        // in a consumer's font engine.
        assert_eq!(
            bytes.len(),
            BRAVURA_SUBSET_LEN,
            "embedded font length changed; regenerate font_subset_generated.rs"
        );
        assert_eq!(
            &bytes[..4],
            b"OTTO",
            "embedded font is not a CFF OpenType (OTTO) font"
        );
        // Content lock: any byte-level corruption flips the BLAKE3 (the workspace's
        // sole hash), even one that preserves the length.
        let digest = epiphany_determinism::blake3_256(&bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex, BRAVURA_SUBSET_BLAKE3,
            "embedded font content hash changed; regenerate font_subset_generated.rs"
        );
    }

    /// Reads an sfnt table slice by 4-byte tag from a decoded OTF.
    fn sfnt_table<'a>(font: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
        let num_tables = u16::from_be_bytes(font.get(4..6)?.try_into().ok()?) as usize;
        for i in 0..num_tables {
            let rec = 12 + i * 16; // after the 12-byte sfnt header
            if font.get(rec..rec + 4)? == tag {
                let off =
                    u32::from_be_bytes(font.get(rec + 8..rec + 12)?.try_into().ok()?) as usize;
                let len =
                    u32::from_be_bytes(font.get(rec + 12..rec + 16)?.try_into().ok()?) as usize;
                return font.get(off..off + len);
            }
        }
        None
    }

    /// The SFNT `name` table's family record (nameID 1), decoded from the first
    /// record carrying it (UTF-16BE for Windows/Unicode platforms, Latin-1 for Mac).
    fn sfnt_family_name(name_table: &[u8]) -> Option<String> {
        let count = u16::from_be_bytes(name_table.get(2..4)?.try_into().ok()?) as usize;
        let storage = u16::from_be_bytes(name_table.get(4..6)?.try_into().ok()?) as usize;
        for i in 0..count {
            let r = 6 + i * 12;
            let platform = u16::from_be_bytes(name_table.get(r..r + 2)?.try_into().ok()?);
            let name_id = u16::from_be_bytes(name_table.get(r + 6..r + 8)?.try_into().ok()?);
            if name_id != 1 {
                continue;
            }
            let len = u16::from_be_bytes(name_table.get(r + 8..r + 10)?.try_into().ok()?) as usize;
            let off = u16::from_be_bytes(name_table.get(r + 10..r + 12)?.try_into().ok()?) as usize;
            let raw = name_table.get(storage + off..storage + off + len)?;
            return Some(if platform == 1 {
                raw.iter().map(|&b| b as char).collect()
            } else {
                raw.chunks_exact(2)
                    .filter_map(|p| char::from_u32(u32::from(u16::from_be_bytes([p[0], p[1]]))))
                    .collect()
            });
        }
        None
    }

    /// The CFF table's font name — the first entry of its Name INDEX (whose offsets
    /// are 1-based from the byte preceding the object data).
    fn cff_font_name(cff: &[u8]) -> Option<String> {
        let hdr_size = *cff.get(2)? as usize; // CFF header: major, minor, hdrSize, offSize
        let count = u16::from_be_bytes(cff.get(hdr_size..hdr_size + 2)?.try_into().ok()?) as usize;
        if count == 0 {
            return None;
        }
        let off_size = usize::from(*cff.get(hdr_size + 2)?);
        let off_base = hdr_size + 3;
        let read = |i: usize| -> Option<usize> {
            let s = off_base + i * off_size;
            let mut v = 0usize;
            for k in 0..off_size {
                v = (v << 8) | usize::from(*cff.get(s + k)?);
            }
            Some(v)
        };
        let data_base = off_base + (count + 1) * off_size - 1;
        let s = cff.get(data_base + read(0)?..data_base + read(1)?)?;
        Some(String::from_utf8_lossy(s).into_owned())
    }

    #[test]
    fn embedded_font_presents_no_reserved_primary_name() {
        use crate::font_subset_generated::BRAVURA_SUBSET_OTF_BASE64;
        let bytes = decode_base64(BRAVURA_SUBSET_OTF_BASE64);
        // An OTF carries two naming structures; the OFL restricts the *primary name*
        // of a Modified Version, so both must be the non-reserved subset family, never
        // the bare Reserved Font Name "Bravura". (Attribution records may, and do,
        // still name Bravura — those are not the primary name.)
        let name_tbl = sfnt_table(&bytes, b"name").expect("name table present");
        assert_eq!(
            sfnt_family_name(name_tbl).as_deref(),
            Some("EpiphanyBravuraSubset"),
            "SFNT family name (nameID 1) must be the non-reserved subset family"
        );
        let cff = sfnt_table(&bytes, b"CFF ").expect("CFF table present");
        assert_eq!(
            cff_font_name(cff).as_deref(),
            Some("EpiphanyBravuraSubset"),
            "CFF Name INDEX must be the non-reserved subset family"
        );
    }

    #[test]
    fn outline_delegates_to_epiphany_glyphs() {
        // A thin smoke test that the delegation is wired, not a re-test of
        // epiphany-glyphs's own (more thorough) outline-data tests.
        assert!(outline("noteheadBlack").is_some());
        assert!(outline("noSuchGlyph").is_none());
        assert_eq!(
            bundled_glyph_count(),
            epiphany_glyphs::bundled_glyph_count()
        );
        assert_eq!(
            smufl_codepoint("gClef"),
            epiphany_glyphs::smufl_codepoint("gClef")
        );
    }
}
