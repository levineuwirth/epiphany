//! `fixtures.json`'s root document and its validator.
//!
//! [`FixtureFile::validate`] follows `round1-candidates/harness`'s
//! `OracleFile::validate` discipline exactly: every check compares the
//! loaded data against a **literal restated in this function**, never
//! against the file's own other fields. A validator that only checks a file
//! against itself accepts any self-consistent file — including one with a
//! fixture deleted, a text silently normalized, or a face hash quietly
//! changed to match a swapped-in font.

use serde::{Deserialize, Serialize};

use crate::a11y::{self, SpikeAccessibilityExpectation};
use crate::faces::LoadedFace;
use crate::identity::SpikeTextFaceIdentity;
use crate::types::SpikeResolvedText;
use crate::{invariants, EM_SIZE_STAFF_SPACE, QUANTIZE_GRID, TARGET_HEIGHT, TARGET_WIDTH};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderTarget {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceRecord {
    pub chain_index: usize,
    pub path: String,
    pub identity: SpikeTextFaceIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureRecord {
    pub id: String,
    pub purpose: String,
    pub resolved: SpikeResolvedText,
    /// The precommitted W3 §5 check-5 oracle for this fixture (recipe §8,
    /// [`crate::a11y`]). Check 5 is disqualifying, so its expectation is
    /// encoded in the artifact candidates consume rather than left as prose
    /// that would be interpreted after a tree had been seen.
    pub accessibility: SpikeAccessibilityExpectation,
}

/// `fixtures.json`'s root. Field order here is JSON key order (serde_json's
/// struct serialization is declaration order, not sorted), which is what
/// makes the output deterministic across regenerations without needing a
/// separate key-sort pass.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureFile {
    pub contract: String,
    pub recipe: String,
    pub em_size_staff_space: f64,
    pub quantize_grid: f64,
    pub target: RenderTarget,
    pub faces: Vec<FaceRecord>,
    pub fixtures: Vec<FixtureRecord>,
}

/// Fails rather than defaults if a fixture has no precommitted accessibility
/// note ([`crate::a11y::note_for`]) — a sixth fixture must not be able to
/// arrive with a blank check-5 expectation.
pub fn build_fixture_file(
    faces: &[LoadedFace],
    fixtures: Vec<(String, String, SpikeResolvedText)>,
) -> Result<FixtureFile, String> {
    let mut records = Vec::with_capacity(fixtures.len());
    for (id, purpose, resolved) in fixtures {
        let accessibility = a11y::build_expectation(&id, &resolved.text)?;
        records.push(FixtureRecord {
            id,
            purpose,
            resolved,
            accessibility,
        });
    }
    Ok(FixtureFile {
        contract: "spec/CONTRACT_EDITOR_T4_SPIKE.md pins 8, 9, 10, 13, 14".to_string(),
        recipe: "spikes/editor-toolkit/ROUND2_TEXT_RECIPE.md".to_string(),
        em_size_staff_space: EM_SIZE_STAFF_SPACE,
        quantize_grid: QUANTIZE_GRID,
        target: RenderTarget {
            width: TARGET_WIDTH,
            height: TARGET_HEIGHT,
        },
        faces: faces
            .iter()
            .enumerate()
            .map(|(i, f)| FaceRecord {
                chain_index: i,
                path: f.path.display().to_string(),
                identity: f.identity.clone(),
            })
            .collect(),
        fixtures: records,
    })
}

/// The two declared faces' SHA-256 hashes, restated as literals (recipe §1
/// table) — the same pair `crate::faces::DECLARED_CHAIN` hard-codes, kept
/// independently here so a validator run against a *different* build of
/// this crate (or a hand-edited `fixtures.json`) still catches a hash that
/// silently drifted from the recipe, not just from this crate's own
/// current source.
const EXPECTED_FACE_HASHES_HEX: [&str; 2] = [
    "44e64260716d8f2bbe412baa1ee99b7c995190ac4573177c24def0b9200438c7",
    "058ea80864aef09a23f45cbec2bb5400bc3dfbdea01c3f10538a21fcb497fb74",
];

/// The exact five fixture ids, in order, and their verbatim literals
/// (recipe §2 table) — restated, not read from `crate::fixtures::FIXTURES`,
/// so a validator built from a *different* copy of this crate still catches
/// drift (the same reasoning `round1-candidates/harness`'s `ROUND1_ROSTER`
/// documents for not reading the roster back out of the oracle it checks).
const EXPECTED_FIXTURES: [(&str, &str); 5] = [
    ("F-A", "Allegro affettuoso \u{2014} al fine"),
    ("F-B", "Coro \u{05D0}\u{05D1}\u{05D2}"),
    ("F-C", "Coro \u{0627}"),
    ("F-D", "Allegro \u{05D0}\u{05D1}\u{05D2} con brio"),
    ("F-E", "Cafe\u{301} \u{2014} resume\u{301}"),
];

/// Each fixture's **purpose string**, restated verbatim — because this string
/// is not decoration. It is what `fixtures.json` carries, what
/// `FIXTURES_SUMMARY.md` prints, and what every generator writes to the
/// console next to that fixture's numbers, so it is the label a reader will
/// use when transcribing results into the Round 2 criterion table.
///
/// **F-D is the reason this check exists.** It used to read `"check 3
/// (bidi)"`, and after the 2026-07-29 ruling (recipe §1.2) that is a
/// contradiction the artifacts assert on every run: check 3 is `NOT RUN` for
/// every candidate (no Arabic-capable face; pin 9), and F-D is scored on a
/// separate **Supplementary** row that must never upgrade check 3 to PASS. A
/// ruling recorded only in the recipe, while the machine-readable artifact and
/// the console output still say "check 3", is a ruling that will be
/// contradicted by whichever record someone happens to read.
const EXPECTED_PURPOSES: [(&str, &str); 5] = [
    (
        "F-A",
        "check 1 (faithful consumption), check 5 (accessibility)",
    ),
    ("F-B", "check 2 (fallback, forced)"),
    ("F-C", "check 2 (uncovered codepoint)"),
    (
        "F-D",
        "SUPPLEMENTARY bidi evidence (Hebrew/Latin) — check 3 remains NOT RUN (no Arabic-capable \
         face; recipe §1.2)",
    ),
    ("F-E", "check 4 (hit testing / caret)"),
];

/// Recipe §4's measured glyph/segment counts, restated (not derived from
/// `crate::fixtures::check_against_recipe`, which runs at generation time
/// against the *live* shaped result — this validates the *serialized*
/// file, which may have been produced by a different run or hand-inspected
/// copy).
///
/// Revision 1 listed only F-A, F-B and F-E, on the correct reasoning that a
/// validator must not invent a number the recipe never committed to. Recipe
/// revision 2 states the other two measured counts instead — every measured
/// count belongs on the record, and leaving two of five fixtures uncovered by
/// the one check that catches shaped-output drift was the worse of the two
/// options. Revision 2 adds F-C's 5 and F-D's 20, which revision 1 left
/// unstated in prose. The right fix was to state them in the recipe — every
/// measured count belongs on the record — rather than to leave two of five
/// fixtures uncovered by the one check that catches a shaped-output drift.
const EXPECTED_GLYPH_COUNTS: [(&str, usize); 5] = [
    ("F-A", 26),
    ("F-B", 8),
    ("F-C", 5),
    ("F-D", 20),
    ("F-E", 13),
];

/// Only F-B ("Two segments, two faces") and F-D ("Three segments") are
/// stated in prose; F-A and F-E's single-segment shape is a natural
/// consequence of unidirectional, single-face text but is not a number §4
/// writes down, so it is not asserted here as a "recipe §4" literal (it is
/// still checked at generation time in `crate::fixtures::check_f_a`/`_e`,
/// documented there as a derived — not quoted — expectation).
const EXPECTED_SEGMENT_COUNTS: [(&str, usize); 2] = [("F-B", 2), ("F-D", 3)];

/// SHA-256 over the whole artifact's canonical JSON — **the check that makes
/// this validator complete**, and the reason the checks above are no longer
/// the only thing standing between a tampered file and a candidate run.
///
/// Every check before this one is a *named* property: the em size, the target,
/// the face hashes, the five texts, the glyph and segment counts, the five W3
/// invariants, the check-5 oracle. Together they still accept an enormous
/// space of wrong files, because they say nothing about the individual glyph
/// ids, the individual quantized offsets, or the caret-stop positions —
/// tens of thousands of numbers that a candidate in packet 2B will be scored
/// against. Change F-A's ligature glyph id while keeping counts and indices
/// valid and every check above passes; the candidate then renders faithfully,
/// disagrees with the unchanged reference raster, and fails a test it should
/// have passed. That is not a hypothetical: it is the review finding this
/// constant exists to close.
///
/// **What it binds, and what that costs.** The digest covers the complete
/// serialized `FixtureFile`, `faces[].path` included — so it is bound to the
/// absolute font paths on the machine that generated it. That is deliberate
/// (pin 9 requires faces to be resolved from an *explicit path list*, and a
/// file whose paths changed is a file that may have been generated against
/// different bytes), and it means regenerating on a machine with fonts
/// installed elsewhere will legitimately produce a different digest. When that
/// happens the fix is to re-record the digest **after** confirming the two
/// face hashes are unchanged — never to relax the check.
///
/// Regenerate with `cargo run -p round2-textkit --bin generate`, which prints
/// the digest it produced next to the one compiled in.
const EXPECTED_ARTIFACT_DIGEST_HEX: &str =
    "acc13c0d02624a0741cca5dffa7470a8971d3ecef5c6fb6f9e533ded684e7ed1";

/// The canonical serialization the digest is taken over: compact JSON, in
/// declaration order (serde's struct order), with no whitespace. Kept separate
/// from the pretty-printed form written to disk so that reformatting the file
/// on disk — which a human might do — does not change the identity, while
/// changing any *value* does.
pub fn canonical_bytes(file: &FixtureFile) -> Vec<u8> {
    serde_json::to_vec(file).expect("FixtureFile is always serializable")
}

/// SHA-256 of [`canonical_bytes`], lowercase hex.
pub fn artifact_digest(file: &FixtureFile) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(canonical_bytes(file));
    hex_encode(&h.finalize())
}

/// The digest this build expects, for `bin/generate` to print alongside the
/// one it just computed.
pub fn expected_artifact_digest() -> &'static str {
    EXPECTED_ARTIFACT_DIGEST_HEX
}

/// Checks one fixture's precommitted check-5 oracle (recipe §8).
///
/// `expected_text` is the recipe §2 literal restated in [`EXPECTED_FIXTURES`]
/// — deliberately **not** `f.resolved.text`, so a file whose text and
/// accessible name were drifted together still fails here. The role tables
/// come from [`crate::a11y`]'s own constants, which are source literals in
/// this crate rather than fields of the file being checked, and so are the
/// same class of authority as `TARGET_WIDTH` or `QUANTIZE_GRID` above.
fn check_accessibility(
    id: &str,
    expected_text: &str,
    a: &SpikeAccessibilityExpectation,
) -> Result<(), String> {
    if a.name != expected_text {
        return Err(format!(
            "{id}: accessible name is {:?}, recipe §2's source string is {expected_text:?} — the \
             accessibility tree must carry the source string, so this is where a normalized or \
             re-shaped name is caught",
            a.name
        ));
    }
    let expected_hex = a11y::hex_lower(expected_text.as_bytes());
    if a.name_bytes_hex != expected_hex {
        return Err(format!(
            "{id}: accessible name_bytes_hex is {} , expected {expected_hex}",
            a.name_bytes_hex
        ));
    }
    if a.name_byte_len != expected_text.len() {
        return Err(format!(
            "{id}: accessible name_byte_len is {}, expected {}",
            a.name_byte_len,
            expected_text.len()
        ));
    }
    if a.name_composition != a11y::NAME_COMPOSITION {
        return Err(format!("{id}: accessible name_composition rule drifted"));
    }
    if a.accepted_roles != a11y::accepted_roles() {
        return Err(format!(
            "{id}: accepted_roles disagrees with crate::a11y::ACCEPTED_ROLE_TABLE — check 5 is \
             disqualifying, and a widened accepted set is how a disqualifying check quietly stops \
             disqualifying anything"
        ));
    }
    if a.prohibited_roles != a11y::prohibited_roles() {
        return Err(format!(
            "{id}: prohibited_roles disagrees with crate::a11y::PROHIBITED_ROLE_TABLE"
        ));
    }
    let expected_outcomes: Vec<String> = a11y::PROHIBITED_OUTCOMES
        .iter()
        .map(|s| s.to_string())
        .collect();
    if a.prohibited_outcomes != expected_outcomes {
        return Err(format!(
            "{id}: prohibited_outcomes disagrees with crate::a11y::PROHIBITED_OUTCOMES"
        ));
    }
    if a.note != a11y::note_for(id)? {
        return Err(format!(
            "{id}: accessibility note disagrees with crate::a11y::note_for"
        ));
    }
    Ok(())
}

impl FixtureFile {
    /// Checks the loaded file against literals restated in this function —
    /// never against the file's own other fields (see the module doc
    /// comment). Returns the first disagreement found.
    pub fn validate(&self) -> Result<(), String> {
        if self.em_size_staff_space != EM_SIZE_STAFF_SPACE {
            return Err(format!(
                "em_size_staff_space is {}, not {EM_SIZE_STAFF_SPACE}",
                self.em_size_staff_space
            ));
        }
        if self.quantize_grid != QUANTIZE_GRID {
            return Err(format!(
                "quantize_grid is {}, not {QUANTIZE_GRID}",
                self.quantize_grid
            ));
        }
        if self.target.width != TARGET_WIDTH || self.target.height != TARGET_HEIGHT {
            return Err(format!(
                "target is {}x{}, not pin 4's {TARGET_WIDTH}x{TARGET_HEIGHT}",
                self.target.width, self.target.height
            ));
        }

        if self.faces.len() != EXPECTED_FACE_HASHES_HEX.len() {
            return Err(format!(
                "{} faces recorded, recipe §1 declares {}",
                self.faces.len(),
                EXPECTED_FACE_HASHES_HEX.len()
            ));
        }
        for (i, expected_hex) in EXPECTED_FACE_HASHES_HEX.iter().enumerate() {
            let face = &self.faces[i];
            let actual_hex = hex_encode(&face.identity.file_hash);
            if &actual_hex != expected_hex {
                return Err(format!(
                    "face[{i}] hash {actual_hex} disagrees with recipe §1's recorded {expected_hex}"
                ));
            }
        }

        if self.fixtures.len() != EXPECTED_FIXTURES.len() {
            return Err(format!(
                "{} fixtures recorded, recipe §2 names {}",
                self.fixtures.len(),
                EXPECTED_FIXTURES.len()
            ));
        }
        for (i, (expected_id, expected_text)) in EXPECTED_FIXTURES.iter().enumerate() {
            let f = &self.fixtures[i];
            if f.id != *expected_id {
                return Err(format!(
                    "fixtures[{i}] id is {:?}, recipe §2 names {expected_id:?}",
                    f.id
                ));
            }
            if f.resolved.text != *expected_text {
                return Err(format!(
                    "{}: text is {:?}, recipe §2's verbatim literal is {:?} — a silent \
                     normalization would show up exactly here",
                    f.id, f.resolved.text, expected_text
                ));
            }
            let (_, expected_purpose) = EXPECTED_PURPOSES[i];
            if f.purpose != expected_purpose {
                return Err(format!(
                    "{}: purpose is {:?}, expected {:?}. This string is the label a reader \
                     transcribes into the Round 2 criterion table, so a drifted purpose \
                     misclassifies a result even when every number below it is right — see \
                     EXPECTED_PURPOSES. For F-D specifically, restoring \"check 3 (bidi)\" \
                     contradicts the 2026-07-29 ruling (recipe §1.2): check 3 is NOT RUN, and \
                     F-D is a separate Supplementary row that must not upgrade it to PASS.",
                    f.id, f.purpose, expected_purpose
                ));
            }
            check_accessibility(&f.id, expected_text, &f.accessibility)?;
        }

        for (id, expected_segments) in EXPECTED_SEGMENT_COUNTS {
            let f = self
                .fixtures
                .iter()
                .find(|f| f.id == id)
                .ok_or_else(|| format!("missing fixture {id}"))?;
            if f.resolved.segments.len() != expected_segments {
                return Err(format!(
                    "{id}: {} segments, recipe §4 records {expected_segments}",
                    f.resolved.segments.len()
                ));
            }
        }
        for (id, expected_glyphs) in EXPECTED_GLYPH_COUNTS {
            let f = self
                .fixtures
                .iter()
                .find(|f| f.id == id)
                .ok_or_else(|| format!("missing fixture {id}"))?;
            let actual: usize = f.resolved.segments.iter().map(|s| s.glyphs.len()).sum();
            if actual != expected_glyphs {
                return Err(format!(
                    "{id}: {actual} total glyphs, recipe §4 records {expected_glyphs}"
                ));
            }
        }

        // Re-run every W3 §5 invariant on every loaded fixture — the
        // structural checks above establish this is *the recipe's* data;
        // this establishes it is still *internally coherent* data.
        for f in &self.fixtures {
            let expect_unresolved = f.id == "F-C";
            invariants::assert_utf8_boundaries(&f.id, &f.resolved)?;
            invariants::assert_segments_partition_totally(&f.id, &f.resolved)?;
            invariants::assert_clusters_carry_required_fields(&f.id, &f.resolved)?;
            invariants::assert_unresolved_clusters_are_diagnostic(
                &f.id,
                &f.resolved,
                expect_unresolved,
            )?;
            invariants::assert_positions_quantized(&f.id, &f.resolved)?;
        }

        // Last, on purpose. Every check above reports a *specific*
        // disagreement ("F-A: 25 total glyphs, recipe §4 records 26"), which
        // is far more useful than "the digest changed"; the digest is the
        // backstop that catches everything the named checks do not name, so it
        // must not pre-empt them.
        let actual = artifact_digest(self);
        if actual != EXPECTED_ARTIFACT_DIGEST_HEX {
            return Err(format!(
                "artifact digest is {actual}, expected {EXPECTED_ARTIFACT_DIGEST_HEX} — some \
                 field of this file differs from the recorded artifact in a way none of the \
                 named checks above covers (a glyph id, a quantized offset, a caret-stop \
                 position, a face path). Re-record the digest only after establishing why it \
                 changed."
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faces::{resolve_declared_chain, FaceResolution};
    use crate::fixtures::{build_fixture, FIXTURES};

    /// Builds a real, valid `FixtureFile` end to end against the actual
    /// declared faces on this machine — the same path `bin/generate.rs`
    /// takes. If either declared face is absent, the test is skipped rather
    /// than failed (pin 14: an environment absence, not a failure); on this
    /// development machine both are present, so every mutation test below
    /// actually runs.
    fn real_valid_file() -> Option<FixtureFile> {
        let resolved = resolve_declared_chain();
        let mut loaded = Vec::new();
        for r in resolved {
            match r {
                FaceResolution::Loaded(lf) => loaded.push(lf),
                FaceResolution::Missing { .. } => return None,
            }
        }
        let built: Vec<(String, String, SpikeResolvedText)> = FIXTURES
            .iter()
            .enumerate()
            .map(|(i, def)| {
                let rt = build_fixture(def, &loaded, i as u64);
                (def.id.to_string(), def.purpose.to_string(), rt)
            })
            .collect();
        Some(build_fixture_file(&loaded, built).expect("every fixture has a precommitted note"))
    }

    fn require_file() -> FixtureFile {
        real_valid_file().expect(
            "this test requires the two declared faces to be present on the machine running it",
        )
    }

    #[test]
    fn a_freshly_built_file_validates() {
        require_file().validate().unwrap();
    }

    #[test]
    fn validate_kills_a_wrong_em_size() {
        let mut f = require_file();
        f.em_size_staff_space = 0.64; // the recipe's original, now-superseded value
        let err = f.validate().unwrap_err();
        assert!(err.contains("em_size_staff_space"), "{err}");
    }

    #[test]
    fn validate_kills_a_wrong_quantize_grid() {
        let mut f = require_file();
        f.quantize_grid = 256.0;
        let err = f.validate().unwrap_err();
        assert!(err.contains("quantize_grid"), "{err}");
    }

    #[test]
    fn validate_kills_a_wrong_target_size() {
        let mut f = require_file();
        f.target.width = 1280.0;
        let err = f.validate().unwrap_err();
        assert!(err.contains("target"), "{err}");
    }

    #[test]
    fn validate_kills_a_missing_face() {
        let mut f = require_file();
        f.faces.pop();
        let err = f.validate().unwrap_err();
        assert!(err.contains("faces"), "{err}");
    }

    #[test]
    fn validate_kills_a_tampered_face_hash() {
        let mut f = require_file();
        f.faces[0].identity.file_hash[0] ^= 0xFF; // simulate a swapped-in font
        let err = f.validate().unwrap_err();
        assert!(err.contains("hash"), "{err}");
    }

    #[test]
    fn validate_kills_a_missing_fixture() {
        let mut f = require_file();
        f.fixtures.pop();
        let err = f.validate().unwrap_err();
        assert!(err.contains("fixtures"), "{err}");
    }

    #[test]
    fn validate_kills_a_wrong_fixture_id() {
        let mut f = require_file();
        f.fixtures[0].id = "F-Z".to_string();
        let err = f.validate().unwrap_err();
        assert!(err.contains("id"), "{err}");
    }

    #[test]
    fn validate_kills_a_silently_normalized_text() {
        let mut f = require_file();
        // F-E's whole reason for existing is NFD; simulate an implementation
        // that quietly normalized it to NFC before storing.
        f.fixtures[4].resolved.text = "Café \u{2014} resumé".to_string();
        let err = f.validate().unwrap_err();
        assert!(err.contains("normalization"), "{err}");
    }

    #[test]
    fn validate_kills_a_wrong_glyph_count() {
        let mut f = require_file();
        f.fixtures[0].resolved.segments[0].glyphs.pop(); // drop the last glyph of F-A
        let err = f.validate().unwrap_err();
        assert!(err.contains("glyphs"), "{err}");
    }

    #[test]
    fn validate_kills_a_wrong_segment_count() {
        let mut f = require_file();
        // F-D (index 3) must have 3 segments. Duplicate its last (empty-face)
        // segment rather than popping one, so total glyph count is untouched
        // and this mutation is caught specifically by the segment-count
        // check, not incidentally by the earlier glyph-count check.
        let dup = f.fixtures[3].resolved.segments.last().unwrap().clone();
        f.fixtures[3].resolved.segments.push(dup);
        let err = f.validate().unwrap_err();
        assert!(err.contains("segments"), "{err}");
    }

    #[test]
    fn validate_kills_a_broken_invariant_on_a_loaded_fixture() {
        let mut f = require_file();
        f.fixtures[0].resolved.clusters.clusters[0]
            .caret_stops
            .clear();
        let err = f.validate().unwrap_err();
        assert!(err.contains("invariant 3"), "{err}");
    }

    // ---- the check-5 accessibility oracle (recipe §8) ----

    #[test]
    fn validate_kills_a_normalized_accessible_name() {
        let mut f = require_file();
        // The exact failure check 5 exists to catch: the tree exposes NFC for
        // an NFD source. Both the string and its hex are mutated together, so
        // this is not caught by the redundant-hex check but by the name check
        // against the recipe literal.
        f.fixtures[4].accessibility.name = "Caf\u{e9} \u{2014} resum\u{e9}".to_string();
        f.fixtures[4].accessibility.name_bytes_hex =
            a11y::hex_lower(f.fixtures[4].accessibility.name.as_bytes());
        f.fixtures[4].accessibility.name_byte_len = f.fixtures[4].accessibility.name.len();
        let err = f.validate().unwrap_err();
        assert!(err.contains("accessible name"), "{err}");
    }

    #[test]
    fn validate_kills_a_name_whose_hex_no_longer_matches_it() {
        let mut f = require_file();
        f.fixtures[0]
            .accessibility
            .name_bytes_hex
            .replace_range(0..2, "ff");
        let err = f.validate().unwrap_err();
        assert!(err.contains("name_bytes_hex"), "{err}");
    }

    #[test]
    fn validate_kills_an_accepted_role_set_widened_to_admit_an_image() {
        let mut f = require_file();
        f.fixtures[0].accessibility.accepted_roles[0]
            .tokens
            .push("Image".to_string());
        let err = f.validate().unwrap_err();
        assert!(err.contains("accepted_roles"), "{err}");
    }

    #[test]
    fn validate_kills_a_dropped_prohibited_outcome() {
        let mut f = require_file();
        // Dropping "absent-from-tree" is the mutation that matters: it is the
        // outcome a canvas toolkit produces by default, so removing it would
        // let the most likely real failure through.
        f.fixtures[0]
            .accessibility
            .prohibited_outcomes
            .retain(|o| o != "absent-from-tree");
        let err = f.validate().unwrap_err();
        assert!(err.contains("prohibited_outcomes"), "{err}");
    }

    #[test]
    fn validate_kills_a_swapped_accessibility_note() {
        let mut f = require_file();
        let other = f.fixtures[1].accessibility.note.clone();
        f.fixtures[0].accessibility.note = other;
        let err = f.validate().unwrap_err();
        assert!(err.contains("note"), "{err}");
    }

    /// The exact mutation the 2026-07-29 ruling forbids: F-D relabelled back
    /// to "check 3 (bidi)" in the artifact candidates and readers consume.
    /// Everything else about the file stays valid, which is precisely why this
    /// needs its own named check — the numbers would all be right and the
    /// classification would be wrong.
    #[test]
    fn validate_kills_f_d_relabelled_as_check_3() {
        let mut f = require_file();
        assert_eq!(f.fixtures[3].id, "F-D", "anchor: index 3 must be F-D");
        f.fixtures[3].purpose = "check 3 (bidi)".to_string();
        let err = f.validate().unwrap_err();
        assert!(err.contains("purpose"), "{err}");
        assert!(err.contains("NOT RUN"), "{err}");
    }

    #[test]
    fn validate_kills_any_swapped_purpose() {
        let mut f = require_file();
        let other = f.fixtures[1].purpose.clone();
        f.fixtures[0].purpose = other;
        let err = f.validate().unwrap_err();
        assert!(err.contains("purpose"), "{err}");
    }

    /// The roster restated in this module must agree with the one
    /// `crate::fixtures` builds from — two hand-maintained lists that silently
    /// disagreed would make the validator check the file against a label no
    /// generator ever writes.
    #[test]
    fn the_restated_purposes_match_the_fixture_definitions() {
        for (i, def) in FIXTURES.iter().enumerate() {
            let (id, purpose) = EXPECTED_PURPOSES[i];
            assert_eq!(def.id, id);
            assert_eq!(def.purpose, purpose, "purpose drift for {id}");
        }
    }

    // ---- the whole-artifact digest ----

    #[test]
    fn validate_kills_a_changed_glyph_id() {
        let mut f = require_file();
        // The mutation the review named: F-A's ligature glyph replaced while
        // every count, index and header stays valid. Nothing above this line
        // sees it; the digest does.
        let g = &mut f.fixtures[0].resolved.segments[0].glyphs[3];
        assert_ne!(
            g.glyph_id, 9999,
            "anchor: the fixture must not already be 9999"
        );
        g.glyph_id = 9999;
        let err = f.validate().unwrap_err();
        assert!(err.contains("digest"), "{err}");
    }

    #[test]
    fn validate_kills_a_moved_quantized_offset() {
        let mut f = require_file();
        // One grid unit — the smallest legal move. Still on the 1/1024 grid,
        // so invariant 5 accepts it; the count checks accept it; the digest
        // does not.
        let g = &mut f.fixtures[0].resolved.segments[0].glyphs[3];
        g.offset.x += 1.0 / crate::QUANTIZE_GRID;
        let err = f.validate().unwrap_err();
        assert!(err.contains("digest"), "{err}");
    }

    #[test]
    fn validate_kills_a_changed_caret_stop_position() {
        let mut f = require_file();
        let s = &mut f.fixtures[0].resolved.clusters.clusters[0].caret_stops[0];
        s.position.x += 1.0 / crate::QUANTIZE_GRID;
        let err = f.validate().unwrap_err();
        assert!(err.contains("digest"), "{err}");
    }

    #[test]
    fn validate_kills_a_changed_face_path() {
        let mut f = require_file();
        f.faces[0].path = "/somewhere/else/texgyrepagella-regular.otf".to_string();
        let err = f.validate().unwrap_err();
        assert!(err.contains("digest"), "{err}");
    }

    #[test]
    fn json_round_trip_preserves_validity() {
        let f = require_file();
        let json = serde_json::to_string_pretty(&f).unwrap();
        let reloaded: FixtureFile = serde_json::from_str(&json).unwrap();
        reloaded.validate().unwrap();
    }

    #[test]
    fn summary_markdown_mentions_every_fixture_and_the_unicode_disagreement() {
        let f = require_file();
        let md = render_summary_markdown(&f);
        for def in FIXTURES {
            assert!(md.contains(def.id), "summary must mention {}", def.id);
        }
        assert!(
            md.contains("disagree"),
            "summary must surface the bidi/segmentation Unicode-version disagreement \
             (crate::findings::W3_F2)"
        );
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// A short, human-readable render of `FixtureFile` for `FIXTURES_SUMMARY.md`
/// — every fixture's segment/face/glyph/cluster facts, and the shaping
/// identity's two Unicode-component versions (recipe §6's "recorded twice
/// on purpose").
pub fn render_summary_markdown(file: &FixtureFile) -> String {
    let mut out = String::new();
    out.push_str("# Round 2 text fixtures — measured summary\n\n");
    out.push_str(&format!(
        "Generated by `round2-textkit`. Em size: `{}` staff space. Target: {}x{}.\n\n",
        file.em_size_staff_space, file.target.width, file.target.height
    ));

    out.push_str("## Faces\n\n");
    for f in &file.faces {
        out.push_str(&format!(
            "- chain[{}] `{}` — family `{}`, version `{:?}`, sha256 `{}`\n",
            f.chain_index,
            f.path,
            f.identity.family,
            f.identity.version,
            hex_encode(&f.identity.file_hash)
        ));
    }
    out.push('\n');

    out.push_str("## Shaping identity (from the first fixture; identical for all)\n\n");
    if let Some(first) = file.fixtures.first() {
        let s = &first.resolved.shaping;
        out.push_str(&format!(
            "- shaper: `{}` `{}.{}.{}`\n",
            s.shaper.0, s.shaper_version.major, s.shaper_version.minor, s.shaper_version.patch
        ));
        out.push_str(&format!(
            "- unicode-bidi: crate `{}`, Unicode `{}`\n",
            s.unicode_bidi.crate_version,
            s.unicode_bidi
                .unicode_version
                .as_deref()
                .unwrap_or("(none reported)")
        ));
        out.push_str(&format!(
            "- unicode-segmentation: crate `{}`, Unicode `{}`\n",
            s.unicode_segmentation.crate_version,
            s.unicode_segmentation
                .unicode_version
                .as_deref()
                .unwrap_or("(none reported)")
        ));
        if s.unicode_bidi.unicode_version != s.unicode_segmentation.unicode_version {
            out.push_str(
                "- **the two Unicode-data versions disagree** — reported, not reconciled \
                 (`crate::findings::W3_F2`).\n",
            );
        }
    }
    out.push('\n');

    out.push_str("## Fixtures\n\n");
    for f in &file.fixtures {
        let r = &f.resolved;
        let total_glyphs: usize = r.segments.iter().map(|s| s.glyphs.len()).sum();
        out.push_str(&format!("### {} — {}\n\n", f.id, f.purpose));
        out.push_str(&format!("text: `{:?}`\n\n", r.text));
        out.push_str(&format!(
            "{} codepoints, {} bytes, {} segments, {} glyphs, {} clusters\n\n",
            r.text.chars().count(),
            r.text.len(),
            r.segments.len(),
            total_glyphs,
            r.clusters.clusters.len()
        ));
        out.push_str("| segment | source | face | direction | script | glyphs |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for (i, seg) in r.segments.iter().enumerate() {
            out.push_str(&format!(
                "| {i} | {}..{} | {:?} | {:?} | {} | {} |\n",
                seg.source.start,
                seg.source.end,
                seg.face,
                seg.direction,
                seg.script.0,
                seg.glyphs.len()
            ));
        }
        out.push('\n');
        out.push_str(&format!(
            "check 5 (accessibility, disqualifying) — name must be exactly `{}` \
             ({} bytes, hex `{}`); {}\n\n",
            f.accessibility.name,
            f.accessibility.name_byte_len,
            f.accessibility.name_bytes_hex,
            f.accessibility.note
        ));
    }

    out.push_str("## Check-5 role vocabulary (identical for every fixture)\n\n");
    if let Some(first) = file.fixtures.first() {
        out.push_str("| platform | accepted | prohibited |\n|---|---|---|\n");
        for (acc, pro) in first
            .accessibility
            .accepted_roles
            .iter()
            .zip(first.accessibility.prohibited_roles.iter())
        {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                acc.platform,
                acc.tokens.join(", "),
                pro.tokens.join(", ")
            ));
        }
        out.push_str(&format!(
            "\nName composition: {}\n\nProhibited outcomes, whatever the role: {}\n",
            first.accessibility.name_composition,
            first.accessibility.prohibited_outcomes.join(", ")
        ));
    }

    out
}

/// Loads `fixtures.json` **and validates it**, in one call, because those two
/// steps must never be separable.
///
/// Round 1 learned this the hard way: the oracle's Rust types deserialized a
/// tampered file cleanly, and the fix was `deny_unknown_fields` on every type
/// *plus* a semantic `validate()` that every consumer calls before it renders.
/// Round 2's consumers arrive in packet 2B, so the entry point exists now,
/// before there is any consumer that could have forgotten it:
///
/// - `deny_unknown_fields` (on every deserialized type in this crate) catches
///   **structural** drift — serde ignores unknown fields by default, so
///   without it a field added to the file would load silently;
/// - [`FixtureFile::validate`] catches **semantic** drift, which is the
///   dangerous kind: a file that deserializes cleanly but no longer means what
///   Round 2 requires would be consumed faithfully and pass.
///
/// There is deliberately no public "load without validating".
pub fn load_fixtures(path: &std::path::Path) -> Result<FixtureFile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read fixtures at {}: {e}", path.display()))?;
    let file: FixtureFile = serde_json::from_str(&text)
        .map_err(|e| format!("failed to parse fixtures at {}: {e}", path.display()))?;
    file.validate()
        .map_err(|e| format!("fixtures at {} failed validation: {e}", path.display()))?;
    Ok(file)
}

#[cfg(test)]
mod load_tests {
    use super::*;

    fn fixtures_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures.json")
    }

    #[test]
    fn the_committed_file_loads_and_validates() {
        let p = fixtures_path();
        if !p.exists() {
            eprintln!(
                "NOT RUN: {} absent — run `cargo run -p round2-textkit --bin generate` first",
                p.display()
            );
            return;
        }
        load_fixtures(&p).unwrap();
    }

    /// Mutation: an unknown field must be refused, not ignored. Without
    /// `deny_unknown_fields` serde drops it silently and the "drift is a load
    /// error" claim above would be false.
    #[test]
    fn an_unknown_field_is_refused() {
        let p = fixtures_path();
        if !p.exists() {
            return;
        }
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("smuggled_field".into(), serde_json::json!(1));
        let err = serde_json::from_value::<FixtureFile>(v)
            .unwrap_err()
            .to_string();
        assert!(err.contains("smuggled_field"), "{err}");
    }

    /// Mutation: a semantically drifted file must be refused by `validate`
    /// even though it deserializes perfectly.
    #[test]
    fn a_semantically_drifted_file_is_refused() {
        let p = fixtures_path();
        if !p.exists() {
            return;
        }
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        v["em_size_staff_space"] = serde_json::json!(0.64);
        let file: FixtureFile = serde_json::from_value(v).unwrap();
        let err = file.validate().unwrap_err();
        assert!(err.contains("em_size_staff_space"), "{err}");
    }
}
