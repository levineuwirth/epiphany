//! Packet 2B-A: the check-5 accessibility oracle's comparison-data half.
//!
//! `spec/CONTRACT_EDITOR_T4_SPIKE.md` pin 13 requires this oracle to be
//! committed and reviewed **before any candidate builds a tree against it** —
//! if a candidate wrote the verifier, the oracle would be shaped by that
//! candidate's tree, which is exactly what pin 13 forbids. This crate is
//! therefore a separate, candidate-neutral packet from either Round 2
//! candidate: it reads the already-committed, already-validated
//! `round2-textkit/fixtures.json` (`ROUND2_TEXT_RECIPE.md` §8;
//! `round2_textkit::a11y`) and derives every byte string a live AT-SPI
//! readback would need to compare against, so the verifier's classification
//! is a comparison against precommitted data, never a heuristic guess about
//! what a wrong name "looks like" (`ROUND2_TEXT_RECIPE.md` §8.1).
//!
//! ## What is derived, and what is restated
//!
//! `expected_name` / `expected_name_hex` / `expected_name_byte_len` and the
//! at-spi2 accepted/prohibited role sets are **restated** — they already
//! exist verbatim on each fixture's `SpikeAccessibilityExpectation`
//! (`round2_textkit::a11y`), computed and validated there. This crate does
//! not recompute them from `resolved.text` a second time; it reads the
//! oracle's own already-validated fields, the same discipline
//! `round2-textkit::output::FixtureFile::validate` uses for everything else.
//!
//! `alternative_forms` and `visual_order_name` are **derived** here, from
//! `SpikeResolvedText`'s own segment and cluster data — never hard-coded to a
//! particular codepoint or glyph id, so the derivation is reproducible from
//! `fixtures.json` alone and does not silently drift from it:
//!
//! * **`name-normalized`** — the NFC normalization of `text`
//!   (`unicode-normalization`). Differs only for F-E (recipe §2: F-E is
//!   deliberately NFD).
//! * **`name-drops-unresolved-codepoints`** — `text` with every segment whose
//!   `face` is `None` removed (`SpikeShapedSegment::face`, `W3-F3`). Differs
//!   only for F-C, whose U+0627 is covered by neither declared face.
//! * **`name-is-shaped-glyphs`** — two independently derived forms, both
//!   modelling "the tree exposes what was drawn rather than what was said":
//!   a cluster-collapse form ([`shaped_glyphs_form`]) and, where derivable, a
//!   standard-ligature presentation-form substitution
//!   ([`shaped_glyphs_presentation_form`]). F-A's `ff`/`fi` ligatures are the
//!   case this fixture set exercises for both. See each function's doc
//!   comment for exactly what it does and does not derive from the fixture
//!   record.
//! * **`visual_order_name`** — concatenates every segment's source text in
//!   the *stored* (logical) segment order, but reverses an `Rtl` segment's
//!   own text by extended grapheme cluster before appending it. This
//!   reproduces "a tree assembled by walking the visual runs left to right"
//!   (recipe §8.1) for every fixture in this set, all of which nest at most
//!   one `Rtl` run inside an `Ltr` base paragraph (recipe §4: base level 0,
//!   Hebrew segments at level 1) — a single odd-level run does not change
//!   the *order* of the run sequence under UAX#9 reordering, only the
//!   *internal* order of that run's own text. **This is not a general bidi
//!   run-reordering implementation**; it is correct for this fixture set and
//!   would need revisiting for a fixture with nested embedding levels beyond
//!   0/1, which none of F-A..F-E have (measured, recipe §4). It also
//!   diverges from the recipe's own claim about which fixture this
//!   is unique to — see [`findings::RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D`].
//!
//! ## Fail-closed on a colliding classification (O1)
//!
//! An earlier version of this crate could emit the *same string* under two
//! different `PROHIBITED_OUTCOMES` names for one fixture — F-C's unresolved
//! cluster produced `"Coro "` under both `name-drops-unresolved-codepoints`
//! and `name-is-shaped-glyphs`, because "drop the unresolved codepoint" and
//! "collapse a zero-glyph cluster" were, for that cluster, the same
//! operation. Which classification a verifier reported was then an artifact
//! of `BTreeMap` iteration (alphabetical) order, not a property of the
//! observation — the oracle was returning two different confident answers
//! for one input. That is fixed two ways, and both are load-bearing:
//!
//! 1. [`shaped_glyphs_form`] no longer collapses a *fully* unresolved
//!    cluster (zero glyphs) — collapsing to "what was drawn" presumes
//!    something was drawn; a wholly unresolved cluster's only legitimate
//!    classification is `name-drops-unresolved-codepoints`. This is enough
//!    to make F-C's two forms genuinely equal to `expected_name` again (no
//!    codepoint was shape-collapsed), so `name-is-shaped-glyphs` is correctly
//!    omitted for F-C by the ordinary omit-if-identical rule.
//! 2. [`build_expectation`] additionally **refuses to build** a fixture whose
//!    candidate forms collide across two different outcome names, panicking
//!    and naming the fixture and both outcomes — a generation-time backstop
//!    for any future fixture or derivation that reintroduces the same
//!    ambiguity, independent of whether fix 1 above happens to prevent it.

use std::collections::BTreeMap;

use round2_textkit::a11y::PROHIBITED_OUTCOMES;
use round2_textkit::output::{FixtureFile, FixtureRecord};
use round2_textkit::types::{SpikeResolvedText, SpikeTextDirection};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

pub mod findings;

/// The one platform row this oracle emits: this machine's live AT client is
/// AT-SPI2 (recipe §8.2, round0-evidence's precedent). Candidates targeting
/// another platform stay covered by the recipe's own table; encoding all five
/// rows here would not make them checkable on a machine that cannot reach
/// them.
pub const PLATFORM: &str = "at-spi2";

/// One fixture's precommitted check-5 comparison data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureExpectation {
    pub fixture_id: String,
    /// The exact source string, restated from
    /// `SpikeAccessibilityExpectation::name` (`round2_textkit::a11y`), not
    /// recomputed — see the module doc comment.
    pub expected_name: String,
    pub expected_name_hex: String,
    pub expected_name_byte_len: usize,
    /// This machine's platform row (`at-spi2`) of recipe §8.2's accepted-role
    /// table, restated from the fixture's own
    /// `SpikeAccessibilityExpectation::accepted_roles`.
    pub accepted_roles: Vec<String>,
    pub prohibited_roles: Vec<String>,
    /// D1: the per-segment source texts, in the segments' own stored
    /// (logical, ascending-source) order — e.g. F-C's `["Coro ", "ا"]`,
    /// F-D's `["Allegro ", "אבג", " con brio"]`. §8.1 explicitly permits "a
    /// tree that exposes one text node per direction run," and §8.3
    /// requires an unresolved codepoint (F-C's `ا`) to appear in the name
    /// regardless of whether it drew ink — but a lone unresolved segment can
    /// be a single character, which falls below any reasonable
    /// coincidence-guarded length floor a verifier-side substring rule would
    /// use. This field lets the verifier match a node's name against a
    /// precommitted exact component instead of guessing from length alone —
    /// the same "precommitted comparison data, not a heuristic" discipline
    /// this whole struct already uses everywhere else. `"".join(source_atoms)
    /// == expected_name` always holds (see `source_atoms`'s own doc comment
    /// and its test coverage).
    pub source_atoms: Vec<String>,
    /// Keyed by a `PROHIBITED_OUTCOMES` name; every precommitted string that
    /// classification would produce for this fixture, matched if the
    /// observed name equals **any** entry in the list (O2: a single outcome
    /// can have more than one plausible precommitted rendering — e.g.
    /// `name-is-shaped-glyphs` carries both a cluster-collapse form and a
    /// standard-ligature presentation-form substitution for F-A). An outcome
    /// absent from this map produced no form distinguishable from
    /// `expected_name` for this fixture (see the module doc comment) and so
    /// cannot classify anything. The same string never appears under two
    /// different outcome keys for one fixture — [`build_expectation`]
    /// refuses to build a file where it would (O1).
    pub alternative_forms: BTreeMap<String, Vec<String>>,
    /// The concatenation a tree assembled by walking visual runs left to
    /// right would produce, only when it differs from `expected_name` (§8.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_order_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_order_name_hex: Option<String>,
}

/// The complete artifact `a11y_expectations.json` carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectationsFile {
    pub contract: String,
    pub recipe: String,
    pub platform: String,
    /// Traceability to the exact `fixtures.json` this file was derived from
    /// (`round2_textkit::output::artifact_digest`) — so a verifier run
    /// against a stale copy of either file is a detectable mismatch rather
    /// than a silent one, the same discipline `fixtures.json` itself uses for
    /// the two declared face hashes.
    pub source_fixtures_digest: String,
    pub fixtures: Vec<FixtureExpectation>,
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// NFC normalization of `text`. §8.3's `name-normalized`: F-E's NFD source is
/// the only fixture where this differs from `text`.
pub fn nfc_form(text: &str) -> String {
    text.nfc().collect()
}

/// `text` with every segment whose `face` is `None` removed, in the
/// segments' own stored (logical, ascending-source) order. §8.3's
/// `name-drops-unresolved-codepoints`: F-C's U+0627 (covered by neither
/// declared face) is the only case in this fixture set.
///
/// Derived entirely from `resolved.segments[*].face` and `.source` — never
/// from a hard-coded codepoint, so a future fixture with a different
/// uncovered span is handled the same way without a code change.
pub fn drop_unresolved_codepoints_form(resolved: &SpikeResolvedText) -> String {
    let mut out = String::new();
    for seg in &resolved.segments {
        if seg.face.is_none() {
            continue;
        }
        let start = seg.source.start as usize;
        let end = seg.source.end as usize;
        out.push_str(&resolved.text[start..end]);
    }
    out
}

/// The per-segment source texts, in the segments' own stored (logical,
/// ascending-source) order (D1). Every segment contributes an atom,
/// resolved or not — F-C's unresolved `ا` is included exactly like any
/// other segment, because the property this field exists to let a verifier
/// check ("does some node's name match one exact source component") is
/// just as true for an unresolved segment as a resolved one, and singling
/// it out would be exactly the kind of per-fixture special case this crate
/// avoids elsewhere.
///
/// Derived entirely from `resolved.segments[*].source` — never from a
/// hard-coded codepoint or fixture id, so a future fixture's own segment
/// boundaries are picked up the same way without a code change.
/// `source_atoms(resolved).concat() == resolved.text` always holds, because
/// W3 invariant 2 (asserted elsewhere in this pipeline) requires segment
/// source ranges to partition the whole string totally, in logical order.
pub fn source_atoms(resolved: &SpikeResolvedText) -> Vec<String> {
    resolved
        .segments
        .iter()
        .map(|seg| {
            let start = seg.source.start as usize;
            let end = seg.source.end as usize;
            resolved.text[start..end].to_string()
        })
        .collect()
}

/// The run's text as a tree exposing "what was drawn" rather than "what was
/// said" would read it, by collapsing each cluster to as many leading
/// graphemes as it has glyphs. §8.3's `name-is-shaped-glyphs`: F-A's `ff`/`fi`
/// ligatures are the case this fixture set exercises.
///
/// Walks `resolved.clusters.clusters` in its own documented ascending-source
/// order (`SpikeClusterMap`'s doc comment). For a cluster whose glyph count is
/// **strictly between zero and** its `grapheme_count` — a genuine ligature
/// drew fewer, but more than zero, glyphs than there are graphemes to
/// report — only that many leading graphemes of the cluster's own source text
/// are kept. A cluster with `glyphs == graphemes` (ordinary) or `glyphs == 0`
/// (**wholly unresolved** — O1: nothing was drawn, so there is no partial
/// "what was drawn" to report; that is `name-drops-unresolved-codepoints`'s
/// classification, not this one) contributes its whole source text unchanged.
/// Nothing here is specific to `ff`/`fi`: the rule is "one reportable unit per
/// glyph, when at least one glyph exists," derived purely from each cluster's
/// own `glyph_indices.len()` and `grapheme_count`.
pub fn shaped_glyphs_form(resolved: &SpikeResolvedText) -> String {
    let mut out = String::new();
    for cluster in &resolved.clusters.clusters {
        let start = cluster.source.start as usize;
        let end = cluster.source.end as usize;
        let chunk = &resolved.text[start..end];
        let glyph_count = cluster.glyph_indices.len() as u32;
        if glyph_count > 0 && glyph_count < cluster.grapheme_count {
            let kept: String = chunk.graphemes(true).take(glyph_count as usize).collect();
            out.push_str(&kept);
        } else {
            out.push_str(chunk);
        }
    }
    out
}

/// The standard Unicode Latin ligature presentation forms (Alphabetic
/// Presentation Forms block, U+FB00-U+FB06) that a shaper's default `liga`
/// feature can produce. This table is **fixed Unicode data, not derived from
/// `fixtures.json`** — this crate deliberately carries no font/cmap
/// dependency (see the crate doc comment on why: it never touches a live
/// tree, and adding one here would be the wrong layer for it), so there is no
/// way to derive "this glyph id denotes U+FB00" from the fixture record
/// alone. What **is** derived from the fixture, for every entry
/// [`shaped_glyphs_presentation_form`] produces, is *which* clusters this
/// table applies to (the same glyph-count-vs-grapheme-count ligature
/// detection [`shaped_glyphs_form`] uses) and *what source text* each one
/// spans; the table is only ever consulted as a lookup keyed by that
/// already-derived source text, never used to invent a cluster boundary of
/// its own.
const LATIN_LIGATURE_PRESENTATION_FORMS: &[(&str, char)] = &[
    ("ff", '\u{FB00}'),
    ("fi", '\u{FB01}'),
    ("fl", '\u{FB02}'),
    ("ffi", '\u{FB03}'),
    ("ffl", '\u{FB04}'),
    ("st", '\u{FB06}'),
];

/// A second, independently plausible rendering of "the tree exposes what was
/// drawn" (§8.3's `name-is-shaped-glyphs`): a tree that reverse-mapped glyph
/// ids through a cmap would most plausibly emit the *standard ligature
/// presentation-form codepoint* (e.g. U+FB00 for `ff`) rather than
/// [`shaped_glyphs_form`]'s truncate-to-glyph-count text. Returns `None` if
/// this fixture has no ligature cluster, **or** if it has one whose source
/// text is not in [`LATIN_LIGATURE_PRESENTATION_FORMS`] — this function never
/// guesses a codepoint it cannot look up.
pub fn shaped_glyphs_presentation_form(resolved: &SpikeResolvedText) -> Option<String> {
    let mut out = String::new();
    let mut substituted_any = false;
    for cluster in &resolved.clusters.clusters {
        let start = cluster.source.start as usize;
        let end = cluster.source.end as usize;
        let chunk = &resolved.text[start..end];
        let glyph_count = cluster.glyph_indices.len() as u32;
        let is_ligature = glyph_count > 0 && glyph_count < cluster.grapheme_count;
        if is_ligature {
            match LATIN_LIGATURE_PRESENTATION_FORMS
                .iter()
                .find(|(seq, _)| *seq == chunk)
            {
                Some((_, presentation_char)) => {
                    out.push(*presentation_char);
                    substituted_any = true;
                }
                // A ligature cluster whose source text has no known
                // presentation-form codepoint: this function cannot honestly
                // produce a full-string answer, so it produces none at all
                // rather than a partially-substituted guess.
                None => return None,
            }
        } else {
            out.push_str(chunk);
        }
    }
    substituted_any.then_some(out)
}

/// The concatenation a tree assembled by walking the run's visual runs left
/// to right would produce (§8.1). See the module doc comment for exactly
/// what this does and does not model.
pub fn visual_order_form(resolved: &SpikeResolvedText) -> String {
    let mut out = String::new();
    for seg in &resolved.segments {
        let start = seg.source.start as usize;
        let end = seg.source.end as usize;
        let chunk = &resolved.text[start..end];
        match seg.direction {
            SpikeTextDirection::Rtl => {
                let reversed: String = chunk.graphemes(true).rev().collect();
                out.push_str(&reversed);
            }
            SpikeTextDirection::Ltr => out.push_str(chunk),
        }
    }
    out
}

/// Groups a fixture's candidate `(outcome, form)` pairs into
/// `alternative_forms`, in three steps:
///
/// 1. drop any candidate whose form is byte-identical to `expected_name` (it
///    cannot classify anything — see the module doc comment);
/// 2. **refuse** (panic, naming `fixture_id` and both outcomes) if the same
///    remaining form string is produced by two *different* outcome names —
///    O1's fail-closed backstop, independent of whichever derivation bug did
///    or did not cause it;
/// 3. otherwise group by outcome, deduplicating repeated identical forms
///    within one outcome's own list (the same classification derived twice is
///    not a collision), and drop any outcome left with an empty list.
///
/// Kept as its own function, separate from [`build_expectation`], so it has a
/// unit test that can hand-construct a collision without needing a real
/// `FixtureRecord` to provoke one.
fn group_alternative_forms(
    fixture_id: &str,
    expected_name: &str,
    candidates: Vec<(&'static str, String)>,
) -> BTreeMap<String, Vec<String>> {
    let mut owner_of: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (outcome, form) in candidates {
        debug_assert!(
            PROHIBITED_OUTCOMES.contains(&outcome),
            "{outcome} must be one of round2_textkit::a11y::PROHIBITED_OUTCOMES"
        );
        if form == expected_name {
            continue;
        }
        match owner_of.get(&form) {
            Some(&existing_outcome) if existing_outcome != outcome => {
                panic!(
                    "{fixture_id}: alternative forms {existing_outcome:?} and {outcome:?} both \
                     produce {form:?} — an oracle that returns two different classifications for \
                     the same observed string must fail closed, not pick one by BTreeMap \
                     iteration order (O1). Fix the derivation so the two outcomes do not collide, \
                     or establish that only one of them legitimately applies to this fixture."
                );
            }
            Some(_) => {
                // Same outcome producing an identical form a second time
                // (e.g. two independent derivations that happen to agree) —
                // not a collision, just redundant; skip the duplicate.
            }
            None => {
                owner_of.insert(form.clone(), outcome);
                grouped.entry(outcome.to_string()).or_default().push(form);
            }
        }
    }
    grouped
}

/// Builds one fixture's [`FixtureExpectation`] from its already-validated
/// `FixtureRecord`.
///
/// `expected_name` and the role sets are restated from
/// `record.accessibility`, not recomputed from `record.resolved.text` — that
/// field was already checked against the recipe §2 literal by
/// `FixtureFile::validate` (via `load_fixtures`) before this function ever
/// runs, so re-deriving it here would be a second, redundant source of
/// truth rather than a check.
///
/// Panics (via [`group_alternative_forms`]) if two different outcome names
/// would classify the same observed string for this fixture (O1).
pub fn build_expectation(record: &FixtureRecord) -> FixtureExpectation {
    let a = &record.accessibility;
    let resolved = &record.resolved;

    let accepted_roles = a
        .accepted_roles
        .iter()
        .find(|m| m.platform == PLATFORM)
        .unwrap_or_else(|| panic!("{}: no {PLATFORM} row in accepted_roles", record.id))
        .tokens
        .clone();
    let prohibited_roles = a
        .prohibited_roles
        .iter()
        .find(|m| m.platform == PLATFORM)
        .unwrap_or_else(|| panic!("{}: no {PLATFORM} row in prohibited_roles", record.id))
        .tokens
        .clone();

    let mut candidates: Vec<(&'static str, String)> = vec![
        ("name-normalized", nfc_form(&a.name)),
        (
            "name-drops-unresolved-codepoints",
            drop_unresolved_codepoints_form(resolved),
        ),
        ("name-is-shaped-glyphs", shaped_glyphs_form(resolved)),
    ];
    if let Some(presentation) = shaped_glyphs_presentation_form(resolved) {
        candidates.push(("name-is-shaped-glyphs", presentation));
    }
    let alternative_forms = group_alternative_forms(&record.id, &a.name, candidates);

    let visual = visual_order_form(resolved);
    let (visual_order_name, visual_order_name_hex) = if visual != a.name {
        let hex = hex_lower(visual.as_bytes());
        (Some(visual), Some(hex))
    } else {
        (None, None)
    };

    FixtureExpectation {
        fixture_id: record.id.clone(),
        expected_name: a.name.clone(),
        expected_name_hex: a.name_bytes_hex.clone(),
        expected_name_byte_len: a.name_byte_len,
        accepted_roles,
        prohibited_roles,
        source_atoms: source_atoms(resolved),
        alternative_forms,
        visual_order_name,
        visual_order_name_hex,
    }
}

/// Builds the complete [`ExpectationsFile`] from an already-loaded, already-
/// validated `FixtureFile` (`round2_textkit::output::load_fixtures`).
pub fn build_expectations_file(file: &FixtureFile) -> ExpectationsFile {
    ExpectationsFile {
        contract: "spec/CONTRACT_EDITOR_T4_SPIKE.md pin 13".to_string(),
        recipe: "spikes/editor-toolkit/ROUND2_TEXT_RECIPE.md §8".to_string(),
        platform: PLATFORM.to_string(),
        source_fixtures_digest: round2_textkit::output::artifact_digest(file),
        fixtures: file.fixtures.iter().map(build_expectation).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use round2_textkit::faces::{resolve_declared_chain, FaceResolution, LoadedFace};
    use round2_textkit::fixtures::{build_fixture, FIXTURES};
    use round2_textkit::output::build_fixture_file;

    /// Builds a real `FixtureFile` end to end against the actual declared
    /// faces on this machine, the same path `round2-textkit`'s own tests and
    /// `bin/generate.rs` take. `None` (test skipped, not failed — pin 14) if
    /// either declared face is absent; on this development machine both are
    /// present.
    fn real_fixture_file() -> Option<FixtureFile> {
        let resolved = resolve_declared_chain();
        let mut loaded: Vec<LoadedFace> = Vec::new();
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
        real_fixture_file().expect(
            "this test requires the two round2-textkit declared faces to be present on the \
             machine running it",
        )
    }

    fn expectation_for<'a>(exp: &'a ExpectationsFile, id: &str) -> &'a FixtureExpectation {
        exp.fixtures
            .iter()
            .find(|f| f.fixture_id == id)
            .unwrap_or_else(|| panic!("no expectation built for {id}"))
    }

    /// F-E's NFC form must differ from its (NFD) source text — the whole
    /// reason F-E exists (recipe §2). If `nfc_form` stopped normalizing, or
    /// F-E's source stopped being NFD, this fails.
    #[test]
    fn f_e_nfc_form_differs_from_its_text() {
        let file = require_file();
        let f_e = file.fixtures.iter().find(|f| f.id == "F-E").unwrap();
        let nfc = nfc_form(&f_e.resolved.text);
        assert_ne!(
            nfc, f_e.resolved.text,
            "F-E's NFC form must differ from its NFD source"
        );
        // And it must actually surface in alternative_forms, keyed correctly.
        let exp = build_expectations_file(&file);
        let e = expectation_for(&exp, "F-E");
        assert_eq!(
            e.alternative_forms.get("name-normalized"),
            Some(&vec![nfc]),
            "F-E must carry a name-normalized alternative form equal to its NFC"
        );
    }

    /// F-C's dropped-codepoint form must be shorter than its source by
    /// *exactly* the byte span of its unresolved (face: None) segment — not
    /// merely shorter by some amount.
    #[test]
    fn f_c_dropped_codepoint_form_is_shorter_by_exactly_the_unresolved_span() {
        let file = require_file();
        let f_c = file.fixtures.iter().find(|f| f.id == "F-C").unwrap();
        let unresolved_span: usize = f_c
            .resolved
            .segments
            .iter()
            .filter(|s| s.face.is_none())
            .map(|s| (s.source.end - s.source.start) as usize)
            .sum();
        assert!(
            unresolved_span > 0,
            "anchor: F-C must have at least one unresolved segment"
        );
        let dropped = drop_unresolved_codepoints_form(&f_c.resolved);
        assert_eq!(
            f_c.resolved.text.len() - dropped.len(),
            unresolved_span,
            "F-C's dropped-codepoint form must be shorter by exactly its unresolved span"
        );
        assert_ne!(dropped, f_c.resolved.text);

        let exp = build_expectations_file(&file);
        let e = expectation_for(&exp, "F-C");
        assert_eq!(
            e.alternative_forms.get("name-drops-unresolved-codepoints"),
            Some(&vec![dropped])
        );
    }

    /// O1's regression lock: F-C must carry exactly one alternative-outcome
    /// classification (`name-drops-unresolved-codepoints`), never a second,
    /// colliding `name-is-shaped-glyphs` entry for the same string. Before
    /// the O1 fix, [`shaped_glyphs_form`] collapsed F-C's wholly-unresolved
    /// cluster to nothing, which is byte-identical to the dropped-codepoint
    /// form — this pins that `name-is-shaped-glyphs` is now correctly absent
    /// for F-C (because it is byte-identical to `expected_name` once
    /// zero-glyph clusters are left untouched), not merely that it happens
    /// to agree with the other outcome.
    #[test]
    fn f_c_carries_no_shaped_glyphs_alternative_form() {
        let file = require_file();
        let f_c = file.fixtures.iter().find(|f| f.id == "F-C").unwrap();
        assert_eq!(
            shaped_glyphs_form(&f_c.resolved),
            f_c.resolved.text,
            "anchor: with the O1 fix, F-C's cluster-collapse form must equal its source text"
        );
        let exp = build_expectations_file(&file);
        let e = expectation_for(&exp, "F-C");
        assert!(
            !e.alternative_forms.contains_key("name-is-shaped-glyphs"),
            "F-C must not carry a name-is-shaped-glyphs alternative form: {:?}",
            e.alternative_forms
        );
        assert_eq!(e.alternative_forms.len(), 1);
    }

    /// F-A's shaped-glyphs forms must differ from its source text — the
    /// `ff`/`fi` ligature case §8.3 names — and both the cluster-collapse
    /// form and the standard-ligature presentation-form substitution (O2)
    /// must be present in the list.
    #[test]
    fn f_a_shaped_glyphs_forms_differ_from_its_text() {
        let file = require_file();
        let f_a = file.fixtures.iter().find(|f| f.id == "F-A").unwrap();
        let has_ligature_cluster = f_a
            .resolved
            .clusters
            .clusters
            .iter()
            .any(|c| (c.glyph_indices.len() as u32) < c.grapheme_count);
        assert!(
            has_ligature_cluster,
            "anchor: F-A must have at least one cluster with fewer glyphs than graphemes"
        );
        let collapsed = shaped_glyphs_form(&f_a.resolved);
        assert_ne!(collapsed, f_a.resolved.text);
        let presentation = shaped_glyphs_presentation_form(&f_a.resolved).expect(
            "F-A's ligature clusters (ff, fi) are both in LATIN_LIGATURE_PRESENTATION_FORMS",
        );
        assert_ne!(presentation, f_a.resolved.text);
        assert_ne!(
            presentation, collapsed,
            "the two shaped-glyphs forms must be genuinely distinct renderings"
        );
        assert!(
            presentation.contains('\u{FB00}'),
            "F-A's presentation form must substitute U+FB00 for the ff ligature: {presentation:?}"
        );
        assert!(
            presentation.contains('\u{FB01}'),
            "F-A's presentation form must substitute U+FB01 for the fi ligature: {presentation:?}"
        );

        let exp = build_expectations_file(&file);
        let e = expectation_for(&exp, "F-A");
        let forms = e
            .alternative_forms
            .get("name-is-shaped-glyphs")
            .expect("F-A must carry a name-is-shaped-glyphs entry");
        assert!(forms.contains(&collapsed), "{forms:?}");
        assert!(forms.contains(&presentation), "{forms:?}");
        assert_eq!(forms.len(), 2, "{forms:?}");
    }

    /// F-D's visual-order form must differ from its logical text — the
    /// composition trap §8.1 names.
    #[test]
    fn f_d_visual_order_form_differs_from_its_logical_text() {
        let file = require_file();
        let f_d = file.fixtures.iter().find(|f| f.id == "F-D").unwrap();
        let has_rtl_segment = f_d
            .resolved
            .segments
            .iter()
            .any(|s| matches!(s.direction, SpikeTextDirection::Rtl));
        assert!(has_rtl_segment, "anchor: F-D must have an Rtl segment");
        let visual = visual_order_form(&f_d.resolved);
        assert_ne!(visual, f_d.resolved.text);

        let exp = build_expectations_file(&file);
        let e = expectation_for(&exp, "F-D");
        assert_eq!(e.visual_order_name.as_ref(), Some(&visual));
        assert_eq!(
            e.visual_order_name_hex.as_deref(),
            Some(hex_lower(visual.as_bytes()).as_str())
        );
    }

    /// D1: F-C's source atoms must be exactly its two segments, and the
    /// unresolved one must stand alone as a single character — the specific
    /// case a verifier-side length-2 substring rule cannot catch, and the
    /// whole reason this field exists.
    #[test]
    fn f_c_source_atoms_are_its_two_segments_one_of_them_single_character() {
        let file = require_file();
        let f_c = file.fixtures.iter().find(|f| f.id == "F-C").unwrap();
        assert_eq!(
            f_c.resolved.segments.len(),
            2,
            "anchor: F-C must have two segments"
        );
        let expected: Vec<String> = f_c
            .resolved
            .segments
            .iter()
            .map(|s| f_c.resolved.text[s.source.start as usize..s.source.end as usize].to_string())
            .collect();
        let atoms = source_atoms(&f_c.resolved);
        assert_eq!(atoms, expected);

        let (_, unresolved_atom) = f_c
            .resolved
            .segments
            .iter()
            .zip(atoms.iter())
            .find(|(s, _)| s.face.is_none())
            .expect("anchor: F-C must have an unresolved segment");
        assert_eq!(
            unresolved_atom.chars().count(),
            1,
            "F-C's unresolved atom must be exactly one character: {unresolved_atom:?}"
        );

        let exp = build_expectations_file(&file);
        let e = expectation_for(&exp, "F-C");
        assert_eq!(e.source_atoms, atoms);
    }

    /// D1: F-D's source atoms must be exactly its three segments.
    #[test]
    fn f_d_source_atoms_are_its_three_segments() {
        let file = require_file();
        let f_d = file.fixtures.iter().find(|f| f.id == "F-D").unwrap();
        assert_eq!(
            f_d.resolved.segments.len(),
            3,
            "anchor: F-D must have three segments"
        );
        let expected: Vec<String> = f_d
            .resolved
            .segments
            .iter()
            .map(|s| f_d.resolved.text[s.source.start as usize..s.source.end as usize].to_string())
            .collect();
        let atoms = source_atoms(&f_d.resolved);
        assert_eq!(atoms, expected);

        let exp = build_expectations_file(&file);
        let e = expectation_for(&exp, "F-D");
        assert_eq!(e.source_atoms, atoms);
    }

    /// Every fixture's source atoms must concatenate back to its own
    /// `expected_name` — the general partition property `source_atoms`'s own
    /// doc comment claims, checked here on the real generated data rather
    /// than only asserted in prose.
    #[test]
    fn source_atoms_concatenate_to_expected_name_for_every_fixture() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        for f in &exp.fixtures {
            let joined: String = f.source_atoms.concat();
            assert_eq!(
                joined, f.expected_name,
                "{}: source_atoms must concatenate to expected_name",
                f.fixture_id
            );
        }
    }

    /// An alternative form byte-identical to `expected_name` must be omitted
    /// entirely, never present with a value equal to the expectation — the
    /// mutation this guards against is a verifier that reports a "match" as
    /// a diagnosed FAIL because a no-op entry happened to be present.
    #[test]
    fn identical_alternative_forms_are_omitted_not_recorded_as_equal() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        for f in &exp.fixtures {
            for (outcome, forms) in &f.alternative_forms {
                assert!(
                    !forms.is_empty(),
                    "{}: {outcome} must not be present with an empty list",
                    f.fixture_id
                );
                for form in forms {
                    assert_ne!(
                        form, &f.expected_name,
                        "{}: alternative form {outcome} must not be recorded when byte-identical \
                         to expected_name",
                        f.fixture_id
                    );
                }
            }
            if let Some(v) = &f.visual_order_name {
                assert_ne!(v, &f.expected_name, "{}: visual_order_name", f.fixture_id);
            }
        }
    }

    /// No fixture's `alternative_forms` may contain the same string under two
    /// different outcome keys (O1) — re-checked here on the real, generated
    /// data, in addition to [`group_alternative_forms_refuses_a_collision`]'s
    /// synthetic unit test.
    #[test]
    fn no_fixture_has_the_same_form_under_two_outcomes() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        for f in &exp.fixtures {
            let mut seen: BTreeMap<&String, &String> = BTreeMap::new();
            for (outcome, forms) in &f.alternative_forms {
                for form in forms {
                    if let Some(existing) = seen.insert(form, outcome) {
                        panic!(
                            "{}: {form:?} appears under both {existing:?} and {outcome:?}",
                            f.fixture_id
                        );
                    }
                }
            }
        }
    }

    /// Every alternative-form key must be one of `PROHIBITED_OUTCOMES` — a
    /// typo'd or invented key would silently fail to classify anything the
    /// verifier actually checks for.
    #[test]
    fn every_alternative_form_key_is_a_prohibited_outcome() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        for f in &exp.fixtures {
            for outcome in f.alternative_forms.keys() {
                assert!(
                    PROHIBITED_OUTCOMES.contains(&outcome.as_str()),
                    "{}: {outcome:?} is not in PROHIBITED_OUTCOMES",
                    f.fixture_id
                );
            }
        }
    }

    /// The at-spi2 role rows restated here must equal
    /// `round2_textkit::a11y`'s own at-spi2 row — this is the platform this
    /// machine's live AT-SPI2 client actually queries (recipe §8.2,
    /// round0-evidence's precedent).
    #[test]
    fn accepted_and_prohibited_roles_match_the_atspi2_row() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        let expected_accepted: Vec<String> = round2_textkit::a11y::ACCEPTED_ROLE_TABLE
            .iter()
            .find(|(p, _)| *p == PLATFORM)
            .unwrap()
            .1
            .iter()
            .map(|s| s.to_string())
            .collect();
        let expected_prohibited: Vec<String> = round2_textkit::a11y::PROHIBITED_ROLE_TABLE
            .iter()
            .find(|(p, _)| *p == PLATFORM)
            .unwrap()
            .1
            .iter()
            .map(|s| s.to_string())
            .collect();
        for f in &exp.fixtures {
            assert_eq!(f.accepted_roles, expected_accepted);
            assert_eq!(f.prohibited_roles, expected_prohibited);
        }
    }

    /// JSON round-trips without loss — the shape a consumer other than this
    /// crate (`a11y-verifier/verify.py`) will actually read.
    #[test]
    fn json_round_trip_preserves_the_expectations() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        let json = serde_json::to_string_pretty(&exp).unwrap();
        let reloaded: ExpectationsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded, exp);
    }

    /// All five fixtures must be present, in order.
    #[test]
    fn all_five_fixtures_are_present_in_order() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        let ids: Vec<&str> = exp.fixtures.iter().map(|f| f.fixture_id.as_str()).collect();
        assert_eq!(ids, vec!["F-A", "F-B", "F-C", "F-D", "F-E"]);
    }

    /// B2: `source_fixtures_digest` must equal
    /// `round2_textkit::output::expected_artifact_digest()` — the same
    /// literal `round2-textkit`'s own `bin/generate` prints and its
    /// `FixtureFile::validate` checks the *loaded* file against. This is the
    /// generation-time half of B2's staleness guard: if `fixtures.json` ever
    /// legitimately changes (a new frozen digest), this test catches that
    /// `round2-a11y-oracle` was not regenerated against it, at test time,
    /// before `a11y-verifier/verify.py`'s `--expect-source-digest` check
    /// would ever catch it live.
    #[test]
    fn source_fixtures_digest_matches_round2_textkit_expected_digest() {
        let file = require_file();
        let exp = build_expectations_file(&file);
        assert_eq!(
            exp.source_fixtures_digest,
            round2_textkit::output::expected_artifact_digest()
        );
    }

    // ---- O1: group_alternative_forms, exercised directly (no live fixture
    // data required, so the collision-refusal logic itself is under test
    // regardless of whether any current fixture happens to trigger it). ----

    #[test]
    fn group_alternative_forms_refuses_a_collision() {
        let result = std::panic::catch_unwind(|| {
            group_alternative_forms(
                "F-TEST",
                "expected",
                vec![
                    ("name-normalized", "same-string".to_string()),
                    ("name-is-shaped-glyphs", "same-string".to_string()),
                ],
            )
        });
        let err = result.expect_err("a collision between two outcomes must panic");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .expect("panic payload must be a string");
        assert!(msg.contains("F-TEST"), "{msg}");
        assert!(msg.contains("name-normalized"), "{msg}");
        assert!(msg.contains("name-is-shaped-glyphs"), "{msg}");
    }

    /// Mutation guard: the same outcome producing the same form twice (e.g.
    /// two derivations that happen to agree) must NOT panic — only a
    /// cross-outcome collision is refused. Without this test, a mutation that
    /// made the collision check fire on any duplicate (not just a
    /// cross-outcome one) would still pass
    /// `group_alternative_forms_refuses_a_collision` above.
    #[test]
    fn group_alternative_forms_deduplicates_a_same_outcome_repeat_without_panicking() {
        let grouped = group_alternative_forms(
            "F-TEST",
            "expected",
            vec![
                ("name-normalized", "same-string".to_string()),
                ("name-normalized", "same-string".to_string()),
            ],
        );
        assert_eq!(
            grouped.get("name-normalized"),
            Some(&vec!["same-string".to_string()])
        );
    }

    #[test]
    fn group_alternative_forms_omits_forms_identical_to_expected_name() {
        let grouped = group_alternative_forms(
            "F-TEST",
            "expected",
            vec![
                ("name-normalized", "expected".to_string()),
                ("name-is-shaped-glyphs", "different".to_string()),
            ],
        );
        assert!(!grouped.contains_key("name-normalized"));
        assert_eq!(
            grouped.get("name-is-shaped-glyphs"),
            Some(&vec!["different".to_string()])
        );
    }
}
