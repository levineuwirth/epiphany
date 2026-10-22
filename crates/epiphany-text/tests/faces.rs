//! The faces are the files their sources note names, and the shaper sets
//! them as HarfBuzz does: every face locked by its hash, its coverage of the
//! characters scores set in words, and shaping that keeps its glyphs and
//! positions from run to run.

use epiphany_text::{grapheme_boundaries, shape, Face, Segment};

/// Each face's identity, BLAKE3 over `MUSCFNTF` and the file's bytes,
/// computed apart from this crate (Python's `blake3` over the same files)
/// when the faces were fetched. A face whose bytes change fails here.
const LOCKED: [(Face, &str); 5] = [
    (
        Face::EdwinRoman,
        "c68368cd457fff0999f04e27c420997d061ca30e3e1be3dfc28206b27dcbda56",
    ),
    (
        Face::EdwinItalic,
        "c82d706bfa7e7d96372b0b3563a0f555362e1da0e255a09e63784bc282dfd0e7",
    ),
    (
        Face::EdwinBold,
        "64c00cefdaec8069d287b76056219ecf4952e4d4a3b5a4b23b2cd62ebb777fce",
    ),
    (
        Face::EdwinBoldItalic,
        "091f547f3498ab7fcdf060214942d8a18f7f27b54026a9633653b32794c7a6c1",
    ),
    (
        Face::BravuraText,
        "90e8d1c47264d280307944647d5ce4661e90895ea1dfee0a76d37b37070bc533",
    ),
];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn every_face_is_locked_by_its_hash() {
    assert_eq!(LOCKED.len(), Face::ALL.len(), "every face is locked");
    for (face, locked) in LOCKED {
        assert_eq!(hex(&face.file_hash()), locked, "{}", face.family());
    }
}

#[test]
fn faces_hash_differently() {
    let mut hashes: Vec<[u8; 32]> = Face::ALL.iter().map(|f| f.file_hash()).collect();
    hashes.sort();
    hashes.dedup();
    assert_eq!(hashes.len(), Face::ALL.len());
}

#[test]
fn every_face_parses_with_its_em() {
    for face in Face::ALL {
        assert_eq!(face.units_per_em(), 1000, "{}", face.family());
        let (ascent, descent) = face.vertical_metrics();
        assert!(ascent > 0 && descent < 0, "{}", face.family());
    }
}

#[test]
fn edwin_covers_latin_words_and_bravura_text_the_metronome_notes() {
    let latin: Vec<char> = (' '..='~').chain("áéñóöúü♭".chars()).collect();
    for face in [
        Face::EdwinRoman,
        Face::EdwinItalic,
        Face::EdwinBold,
        Face::EdwinBoldItalic,
    ] {
        for &c in &latin {
            assert!(face.covers(c), "{} lacks {c:?}", face.family());
        }
        // A metronome mark's note is a SMuFL symbol, set in Bravura Text.
        assert!(!face.covers('\u{ECA5}'), "{}", face.family());
    }
    for c in '\u{ECA0}'..='\u{ECB7}' {
        assert!(Face::BravuraText.covers(c), "Bravura Text lacks {c:?}");
    }
    // Bravura Text holds almost no Latin.
    assert!(!Face::BravuraText.covers('a'));
}

fn chain() -> [Face; 2] {
    [Face::EdwinRoman, Face::BravuraText]
}

fn covered(segments: &[Segment], text: &str) {
    let mut at = 0;
    for segment in segments {
        assert_eq!(segment.source.start, at, "the segments are in order");
        assert!(segment.source.end > segment.source.start);
        at = segment.source.end;
    }
    assert_eq!(at, text.len(), "the segments cover the string");
}

#[test]
fn each_character_takes_the_first_face_that_covers_it() {
    let text = "Andante \u{ECA5} = 72";
    let segments = shape(text, &chain());
    covered(&segments, text);
    let faces: Vec<(usize, &str)> = segments
        .iter()
        .map(|s| (s.face, &text[s.source.clone()]))
        .collect();
    assert_eq!(
        faces,
        vec![(0, "Andante "), (1, "\u{ECA5}"), (0, " = 72")],
        "Edwin sets the words and Bravura Text the note"
    );
    assert!(segments.iter().all(|s| s.resolved && s.left_to_right));
}

#[test]
fn a_character_no_face_covers_is_left_unresolved_and_undrawn() {
    let text = "a\u{4E2D}b";
    let segments = shape(text, &chain());
    covered(&segments, text);
    assert_eq!(segments.len(), 3);
    let middle = &segments[1];
    assert!(!middle.resolved);
    assert_eq!(&text[middle.source.clone()], "\u{4E2D}");
    assert!(middle.glyphs.is_empty(), "nothing is drawn for it");
    assert!(segments[0].resolved && segments[2].resolved);
}

#[test]
fn a_right_to_left_script_is_read_as_one() {
    // Neither face covers Hebrew; a chain that did would still shape it left
    // to right, so the run says which way it reads.
    let segments = shape("\u{05D0}", &[Face::EdwinRoman]);
    assert!(!segments[0].resolved);
    let latin = shape("abc", &[Face::EdwinRoman]);
    assert!(latin[0].left_to_right);
    assert_eq!(&latin[0].script, b"Latn");
}

#[test]
fn shaping_kerns_where_mapping_alone_would_not() {
    // "AV" sets its letters closer than their own advances: the shaper reads
    // the face's kerning, as HarfBuzz does.
    let pair = shape("AV", &[Face::EdwinRoman]);
    let alone: i32 = ["A", "V"]
        .iter()
        .map(|s| shape(s, &[Face::EdwinRoman])[0].glyphs[0].x_advance)
        .sum();
    let together: i32 = pair[0].glyphs.iter().map(|g| g.x_advance).sum();
    assert!(together < alone, "kerned {together} against {alone}");
}

#[test]
fn clusters_are_byte_offsets_into_the_whole_string() {
    let text = "dé \u{ECA5}x";
    let segments = shape(text, &chain());
    for segment in &segments {
        for glyph in &segment.glyphs {
            let at = glyph.cluster as usize;
            assert!(segment.source.contains(&at));
            assert!(text.is_char_boundary(at));
        }
    }
}

#[test]
fn caret_stops_fall_on_grapheme_boundaries() {
    // "e" with a combining acute is one grapheme of three bytes.
    let text = "ae\u{0301}b";
    assert_eq!(grapheme_boundaries(text), vec![0, 1, 4, 5]);
    assert_eq!(grapheme_boundaries(""), vec![0]);
}

/// Every field shaping produces, for a fixed set of words, folded into one
/// digest: the glyph ids, clusters, advances and offsets of each segment.
fn shaping_digest() -> String {
    let words = [
        "Allegro moderato",
        "Andante con moto",
        "a tempo",
        "cresc. poco a poco",
        "dolce espressivo",
        "pizz.",
        "Kyrie eleison",
        "Gloria in excelsis Deo",
        "affine office waffle",
        "AVATAR To Wa Ty",
        "Moderato \u{ECA5} = 96",
        "Più mosso, señor",
        "B\u{266D} clarinet",
    ];
    let mut bytes: Vec<u8> = Vec::new();
    for faces in [
        [Face::EdwinRoman, Face::BravuraText],
        [Face::EdwinItalic, Face::BravuraText],
        [Face::EdwinBold, Face::BravuraText],
        [Face::EdwinBoldItalic, Face::BravuraText],
    ] {
        for word in words {
            for segment in shape(word, &faces) {
                bytes.extend_from_slice(&(segment.face as u32).to_le_bytes());
                bytes.extend_from_slice(&(segment.source.start as u32).to_le_bytes());
                bytes.extend_from_slice(&(segment.source.end as u32).to_le_bytes());
                bytes.push(segment.resolved as u8);
                for g in &segment.glyphs {
                    for v in [
                        g.glyph_id as i64,
                        g.cluster as i64,
                        g.x_advance as i64,
                        g.y_advance as i64,
                        g.x_offset as i64,
                        g.y_offset as i64,
                    ] {
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
        }
    }
    let mut preimage =
        epiphany_determinism::Preimage::new(epiphany_determinism::DomainTag::FONT_FILE);
    preimage.push_bytes(&bytes);
    hex(preimage.finish().as_bytes())
}

/// Shaping is a function of the string, the faces and the shaper: the same
/// words set in the same faces give the same glyphs on every run, and on
/// every toolchain the suite runs under (CI's pinned stable and its MSRV
/// floor run this test alike). A change to the faces, the shaper or how it
/// is driven moves the digest, and is then a deliberate change.
#[test]
fn shaping_gives_the_same_glyphs_on_every_run() {
    let first = shaping_digest();
    assert_eq!(first, shaping_digest());
    assert_eq!(
        first,
        "4b14c16a988aa9fd7ea28f57dc9a916a8e97b0696118c48c1554b5e58378feb3"
    );
}
