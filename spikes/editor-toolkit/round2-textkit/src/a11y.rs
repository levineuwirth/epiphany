//! The accessibility oracle for W3 §5 check 5 — **precommitted, before any
//! candidate exists** (recipe §8).
//!
//! Check 5 is in the disqualifying set. Revision 2 of the recipe said "the
//! expected node role is committed per fixture" and then named no role, encoded
//! nothing, and left the whole oracle as prose. An unencoded expectation for a
//! disqualifying check is not an oracle; it is a place where a judgement would
//! have been made *after* seeing a candidate's tree, which is the one thing
//! pin 13 exists to prevent.
//!
//! ## What is pinned, and why it is pinned this way
//!
//! The property W3 §5 check 5 states is that the run reaches assistive
//! technology **as its source string** — not as its shaped glyphs, not as a
//! picture, not as a normalization of it. Three things follow, and all three
//! are encoded here:
//!
//! 1. **The name is the exact source bytes.** Stored twice, as the string and
//!    as its lowercase hex, so a silent normalization is visible even to
//!    someone reading the JSON by eye. F-E is the load-bearing case: its NFD
//!    source must surface as NFD, and `Café` (NFC) is a FAIL, not a nicety.
//!    F-C is the second: its U+0627 is covered by **no** declared face and
//!    draws no ink at all, and the accessible name must contain it anyway —
//!    the accessibility tree carries the text, not the ink.
//! 2. **The role is the platform's static-text role**, from a closed accepted
//!    set per platform, with an explicitly named prohibited set so the failure
//!    mode has a name rather than being "not in the list".
//! 3. **A set of prohibited outcomes** that fail check 5 whatever the role is
//!    — chiefly absence from the tree, which is the outcome a
//!    canvas-rendering toolkit produces by default and the one this check is
//!    most likely to actually catch.
//!
//! ## Candidate neutrality
//!
//! The pin is stated per platform, not in one toolkit's vocabulary. Naming
//! only AccessKit's `Role` enum would have quietly favoured C1 (egui ships
//! AccessKit) over C2 (vello is a rendering crate with no accessibility layer
//! of its own), and a criterion that encodes one candidate's stack is not a
//! criterion. A candidate that must build its own accessibility layer to pass
//! is free to do so on any of the platforms below; what it may not do is
//! expose the run as a picture, or not expose it at all.
//!
//! Recorded so it is not mistaken for an oversight later: **the accepted-role
//! rows below were read from the actual `accesskit` 0.24.1 `Role` enum in this
//! workspace's lockfile**, not from memory — `Label`, `TextRun`, and
//! `Paragraph` all exist there, as do every prohibited name.

use serde::{Deserialize, Serialize};

/// One platform's vocabulary for "this node is static text".
///
/// A candidate satisfies the role half of check 5 by matching **one** row: the
/// platform it actually exposes a tree on, and one of that row's tokens. It
/// does not have to satisfy all of them, and it is not required to expose
/// trees on platforms it does not target.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeRoleMapping {
    pub platform: String,
    pub tokens: Vec<String>,
}

fn mapping(platform: &str, tokens: &[&str]) -> SpikeRoleMapping {
    SpikeRoleMapping {
        platform: platform.to_string(),
        tokens: tokens.iter().map(|s| s.to_string()).collect(),
    }
}

/// The accepted static-text roles, per platform. Restated as literals in
/// [`accepted_roles`] and again in [`crate::output::FixtureFile::validate`],
/// never read back out of the file being checked.
pub const ACCEPTED_ROLE_TABLE: [(&str, &[&str]); 5] = [
    ("accesskit-0.24", &["Label", "TextRun", "Paragraph"]),
    ("at-spi2", &["label", "static", "text", "paragraph"]),
    ("aria", &["(none)", "text", "paragraph"]),
    ("macos-nsaccessibility", &["AXStaticText"]),
    ("windows-uia", &["Text"]),
];

/// Roles that are a FAIL of check 5, named rather than left as "anything not
/// accepted", so a candidate's result reads as *this specific* divergence.
/// These are the outcomes a canvas toolkit actually produces: the whole run
/// lands in the tree as one picture, or as a presentational container that
/// assistive technology is told to skip.
pub const PROHIBITED_ROLE_TABLE: [(&str, &[&str]); 5] = [
    (
        "accesskit-0.24",
        &[
            "Image",
            "GraphicsObject",
            "GraphicsSymbol",
            "GenericContainer",
            "Unknown",
            "Pane",
        ],
    ),
    (
        "at-spi2",
        &["image", "canvas", "filler", "panel", "unknown"],
    ),
    (
        "aria",
        &[
            "img",
            "presentation",
            "none",
            "graphics-object",
            "graphics-symbol",
        ],
    ),
    (
        "macos-nsaccessibility",
        &["AXImage", "AXUnknown", "AXGroup"],
    ),
    ("windows-uia", &["Image", "Pane", "Custom"]),
];

/// Outcomes that fail check 5 **whatever role is reported**.
pub const PROHIBITED_OUTCOMES: [&str; 5] = [
    // The default outcome for a toolkit that draws to a canvas and stops.
    "absent-from-tree",
    // The name is present but empty, which is absence wearing a role.
    "name-empty",
    // F-E's case: the tree exposes NFC for an NFD source.
    "name-normalized",
    // The tree exposes what was drawn rather than what was said — glyph names,
    // glyph ids, or the ligated/substituted text.
    "name-is-shaped-glyphs",
    // F-C's case: the uncovered codepoint is dropped from the name because it
    // drew no ink.
    "name-drops-unresolved-codepoints",
];

/// How the accessible name may be assembled — the one place this oracle
/// deliberately admits two shapes.
///
/// F-B and F-D are multi-segment runs, and an implementation that exposes one
/// text node per direction run is not wrong; requiring exactly one node would
/// have manufactured a failure for a legitimate tree. So the requirement is on
/// the *concatenation*: the run's own accessible name, or the names of its text
/// descendants concatenated in **logical** (not visual) order, must equal the
/// source string byte-for-byte.
pub const NAME_COMPOSITION: &str = "single-text-node-name, or logical-order concatenation of the \
     run subtree's text-descendant names — either must equal `name` byte for byte";

/// The precommitted check-5 expectation for one fixture.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeAccessibilityExpectation {
    /// The exact accessible name: the fixture's source string, verbatim.
    pub name: String,
    /// The same bytes in lowercase hex. Redundant on purpose — a
    /// normalization, a trimmed space, or a re-encoded dash changes this field
    /// visibly in a diff of the JSON, where the string form can look
    /// identical.
    pub name_bytes_hex: String,
    pub name_byte_len: usize,
    pub name_composition: String,
    pub accepted_roles: Vec<SpikeRoleMapping>,
    pub prohibited_roles: Vec<SpikeRoleMapping>,
    pub prohibited_outcomes: Vec<String>,
    /// What this fixture is load-bearing for in check 5, in one line.
    pub note: String,
}

pub fn accepted_roles() -> Vec<SpikeRoleMapping> {
    ACCEPTED_ROLE_TABLE
        .iter()
        .map(|(p, t)| mapping(p, t))
        .collect()
}

pub fn prohibited_roles() -> Vec<SpikeRoleMapping> {
    PROHIBITED_ROLE_TABLE
        .iter()
        .map(|(p, t)| mapping(p, t))
        .collect()
}

/// The per-fixture note, keyed by fixture id. Every fixture in the recipe has
/// one; an id with no note is a hard error rather than a blank field, so
/// adding a sixth fixture cannot silently arrive with an unstated
/// accessibility expectation.
pub fn note_for(fixture_id: &str) -> Result<&'static str, String> {
    Ok(match fixture_id {
        "F-A" => {
            "Ligatures: the name must be the source `ff`/`fi`, never the ligature glyphs that \
             drew them."
        }
        "F-B" => {
            "Two segments, two faces: the name must be the whole logical string, assembled in \
             logical order, not the visual order the Hebrew tail is drawn in."
        }
        "F-C" => {
            "Uncovered codepoint: U+0627 draws no ink in either declared face, and must appear \
             in the name regardless — the tree carries the text, not the ink."
        }
        "F-D" => {
            "Three visual runs at levels 0/1/0: the concatenation is logical-order, so a tree \
             built by walking the visual runs left to right fails here and only here."
        }
        "F-E" => {
            "NFD: `Cafe\\u{301}` must surface as NFD. A tree exposing `Café` (NFC) has silently \
             normalized, and that is a FAIL, not a formatting difference."
        }
        other => {
            return Err(format!(
                "no accessibility note precommitted for fixture {other}"
            ))
        }
    })
}

pub fn build_expectation(
    fixture_id: &str,
    text: &str,
) -> Result<SpikeAccessibilityExpectation, String> {
    Ok(SpikeAccessibilityExpectation {
        name: text.to_string(),
        name_bytes_hex: hex_lower(text.as_bytes()),
        name_byte_len: text.len(),
        name_composition: NAME_COMPOSITION.to_string(),
        accepted_roles: accepted_roles(),
        prohibited_roles: prohibited_roles(),
        prohibited_outcomes: PROHIBITED_OUTCOMES.iter().map(|s| s.to_string()).collect(),
        note: note_for(fixture_id)?.to_string(),
    })
}

pub fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_recipe_fixture_has_a_precommitted_note() {
        for def in crate::fixtures::FIXTURES {
            note_for(def.id).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn an_unknown_fixture_id_is_an_error_not_a_blank_note() {
        assert!(note_for("F-Z").is_err());
    }

    #[test]
    fn the_hex_form_tracks_the_exact_bytes() {
        let e = build_expectation("F-E", "Cafe\u{301}").unwrap();
        assert_eq!(e.name_bytes_hex, "43616665cc81");
        assert_eq!(e.name_byte_len, 6);
        // The NFC form is a different string AND a different hex — which is
        // the entire reason the hex is stored.
        let nfc = build_expectation("F-E", "Caf\u{e9}").unwrap();
        assert_ne!(nfc.name_bytes_hex, e.name_bytes_hex);
        assert_eq!(nfc.name_byte_len, 5);
    }

    /// The accepted and prohibited sets must not overlap. A token in both
    /// would make the oracle unfalsifiable for that platform, and the tables
    /// are hand-maintained.
    #[test]
    fn no_token_is_both_accepted_and_prohibited() {
        for (plat, accepted) in ACCEPTED_ROLE_TABLE {
            let prohibited = PROHIBITED_ROLE_TABLE
                .iter()
                .find(|(p, _)| *p == plat)
                .unwrap_or_else(|| panic!("platform {plat} has accepted roles but no prohibited"))
                .1;
            for a in accepted {
                assert!(
                    !prohibited.contains(a),
                    "{plat}: {a:?} is both accepted and prohibited"
                );
            }
        }
    }

    /// Every platform named in one table is named in the other, in the same
    /// order — so a platform cannot arrive with an accepted set and no
    /// prohibitions (or the reverse), which would silently accept anything.
    #[test]
    fn the_two_role_tables_cover_the_same_platforms() {
        let a: Vec<&str> = ACCEPTED_ROLE_TABLE.iter().map(|(p, _)| *p).collect();
        let p: Vec<&str> = PROHIBITED_ROLE_TABLE.iter().map(|(p, _)| *p).collect();
        assert_eq!(a, p);
    }
}
