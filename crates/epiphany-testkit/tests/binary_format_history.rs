//! A scoped guard on `spec/binary_format.tex`'s Revision History chapter
//! (Packet B of `spec/CONTRACT_GENESIS_G3A_UNDO_REPAIR.md`, pins B5/B6),
//! filed against P13-S17: the chapter once ran genesis tranche G2a straight
//! to G-minor to G3a, omitting G2b entirely — including the accept-set
//! raise `OperationEnvelopeBlock` 2 -> 3 that G2b performed, which reached
//! the normative tables but never the history.
//!
//! Two constraints from pin B6 pull against each other and both must hold:
//!
//! * Bare name-presence is not enough. `"G2b"` already occurs inside the
//!   chapter in prose — the G3a row observes that the accept-set "stays at
//!   3 where genesis tranche G2b left it" — so a guard that only checked
//!   for the substring `"G2b"` would stay green even with the G2b row
//!   deleted. The guard below requires a *principal marker*: the rung name
//!   immediately preceded by the row's `---` separator (e.g.
//!   `--- Genesis tranche G2b`), which a prose mention where the separator
//!   *follows* the name (as in "genesis tranche G1 --- landed at",
//!   `binary_format.tex:3603`) cannot satisfy.
//! * No document version number appears anywhere in this file — not in the
//!   assertions and not in this comment, which is why none is quoted here
//!   even as an example. Encoding one would pin a number a future chronology
//!   correction would have to move, reintroducing the stale hand-maintained
//!   parallel list this project keeps rediscovering. Assert rung identity and
//!   ordering; never the number attached to a rung.
//!
//! **G1 is deliberately unguarded.** `binary_format.tex:3603` states
//! outright that genesis tranche G1 has no standalone Revision History row
//! — it is recorded retroactively *inside* the G2a row ("genesis tranche G1
//! --- landed at `3b09595` with no matching entry here"). Demanding a
//! principal marker for G1 would make this guard born red against a
//! document that pin B2 leaves correct, so only G2a, G-minor, G2b, and G3a
//! get principal-marker assertions here. Do not "fix" this by adding a
//! fifth marker; that rediscovers the contradiction pin B6 already resolved.

use std::fs;
use std::path::Path;

/// The rung name immediately preceded by the row's `---` separator, in the
/// exact spelling each row uses (G-minor is never prefixed "Genesis
/// tranche" in the document; the other three are).
const PRINCIPAL_MARKERS: [(&str, &str); 4] = [
    ("G2a", "--- Genesis tranche G2a"),
    ("G-minor", "--- G-minor"),
    ("G2b", "--- Genesis tranche G2b"),
    ("G3a", "--- Genesis tranche G3a"),
];

fn binary_format_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/binary_format.tex");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    })
}

/// Collapse whitespace runs to a single space so a future rewrap of a row's
/// LaTeX source lines cannot silently break a substring search that this
/// guard depends on. Byte offsets after this pass are relative to the
/// normalized string, which is all the ordering assertion needs — nothing
/// here reports a source line number.
fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = true;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out
}

/// Every non-overlapping byte offset at which `needle` occurs in `haystack`.
fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = haystack[cursor..].find(needle) {
        let offset = cursor + relative;
        offsets.push(offset);
        cursor = offset + needle.len();
    }
    offsets
}

/// Slice the normalized document down to the Revision History chapter: from
/// its own `\chapter{Revision History}` heading to the next `\chapter{`
/// (there is none after it today, so this also tolerates a future chapter
/// being appended afterward) or the end of the document.
fn revision_history_slice(normalized: &str) -> &str {
    const CHAPTER: &str = r"\chapter{Revision History}";
    const NEXT_CHAPTER: &str = r"\chapter{";

    let start = normalized
        .find(CHAPTER)
        .expect("binary_format.tex has no \\chapter{Revision History}");
    let after = start + CHAPTER.len();
    let end = normalized[after..]
        .find(NEXT_CHAPTER)
        .map(|relative| after + relative)
        .unwrap_or(normalized.len());
    &normalized[start..end]
}

/// Each rung's principal marker exactly once in the slice. A prose-only
/// mention (bare name, no preceding `---`) does not count, and neither
/// does a duplicated row.
#[test]
fn revision_history_has_exactly_one_principal_marker_per_rung() {
    let source = binary_format_source();
    let normalized = normalize_whitespace(&source);
    let slice = revision_history_slice(&normalized);

    for (rung, marker) in PRINCIPAL_MARKERS {
        let offsets = find_all(slice, marker);
        assert_eq!(
            offsets.len(),
            1,
            "expected exactly one principal marker {marker:?} for rung {rung} in the \
             Revision History chapter, found {} (offsets {offsets:?})",
            offsets.len()
        );
    }
}

/// The four marked rungs appear in ladder order: G1 -> G2a -> G-minor ->
/// G2b -> G3a. G1 has no marker of its own (see the module comment), so
/// this checks the remaining four.
#[test]
fn revision_history_rungs_are_strictly_ordered() {
    let source = binary_format_source();
    let normalized = normalize_whitespace(&source);
    let slice = revision_history_slice(&normalized);

    let offsets: Vec<(&str, usize)> = PRINCIPAL_MARKERS
        .iter()
        .map(|(rung, marker)| {
            let found = find_all(slice, marker);
            assert_eq!(
                found.len(),
                1,
                "expected exactly one principal marker {marker:?} for rung {rung}, found \
                 {found:?}"
            );
            (*rung, found[0])
        })
        .collect();

    for window in offsets.windows(2) {
        let (earlier_rung, earlier_offset) = window[0];
        let (later_rung, later_offset) = window[1];
        assert!(
            earlier_offset < later_offset,
            "expected {earlier_rung} (offset {earlier_offset}) to precede {later_rung} \
             (offset {later_offset}) in the Revision History chapter; ladder order is \
             G2a < G-minor < G2b < G3a"
        );
    }
}

/// The G2b row states what G2b actually did (pin B3), and the search is
/// bounded to the G2b row's own segment — from its principal marker to the
/// next principal marker (or the slice end) — so that G3a's row, which also
/// names `OperationEnvelopeBlock` 3 and mentions G2b in prose, cannot
/// satisfy this after the G2b row itself is deleted. Unbounded searching is
/// exactly the hole P13-S17 was filed over.
#[test]
fn revision_history_g2b_row_states_what_g2b_did() {
    let source = binary_format_source();
    let normalized = normalize_whitespace(&source);
    let slice = revision_history_slice(&normalized);

    const G2B_MARKER: &str = "--- Genesis tranche G2b";
    let g2b_offsets = find_all(slice, G2B_MARKER);
    assert_eq!(
        g2b_offsets.len(),
        1,
        "expected exactly one G2b principal marker, found {g2b_offsets:?}"
    );
    let g2b_start = g2b_offsets[0];
    let after_g2b = g2b_start + G2B_MARKER.len();

    let next_marker_offset = PRINCIPAL_MARKERS
        .iter()
        .filter_map(|(_, marker)| {
            find_all(&slice[after_g2b..], marker)
                .first()
                .map(|relative| after_g2b + relative)
        })
        .min()
        .unwrap_or(slice.len());

    let row_segment = &slice[g2b_start..next_marker_offset];

    assert!(
        row_segment.contains("SetTuningContext"),
        "G2b row segment does not name SetTuningContext: {row_segment:?}"
    );
    assert!(
        row_segment.contains(r"\tablenums{34}"),
        "G2b row segment does not carry discriminant 34: {row_segment:?}"
    );
    assert!(
        row_segment.contains("TuningContextSettings"),
        "G2b row segment does not name the TuningContextSettings payload subset: \
         {row_segment:?}"
    );
    assert!(
        row_segment.contains("OperationEnvelopeBlock")
            && row_segment.contains(r"\tablenums{2}~$\rightarrow$~\tablenums{3}"),
        "G2b row segment does not record the OperationEnvelopeBlock accept-set raise \
         2 -> 3: {row_segment:?}"
    );
}
