//! Invariant 10's reference surface, compared against both documents that
//! summarise it.
//!
//! P13-S26. `core_spec.tex`'s item 10 and `invariants.rs`' `/// 10.` doc block
//! are summaries of one thing: the reference classes
//! `GraphIndex::check_cross_cutting_refs` and the tempo-map check enforce.
//! Before this rung they were incomplete in *different* places, so neither
//! could be repaired from the other. `INVARIANT_TEN_SURFACE` below is the
//! contract's pin-1 table in machine-readable form, derived from the check
//! bodies, and both documents are compared against **it** rather than against
//! each other.
//!
//! It is not a second list of something derivable: the derivation source is
//! Rust control flow, which is not parseable. The contract's gate 8 re-derives
//! this table by hand after every edit; that is the standing compensation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Pin 1's (token, target) table, verbatim. The ratified origin.
const INVARIANT_TEN_SURFACE: &[(&str, &str)] = &[
    ("Slur.start_event", "live event"),
    ("Slur.end_event", "live event"),
    ("Tie.start_event", "live event"),
    ("Tie.end_event", "live event"),
    ("Beam.events", "live event"),
    ("SubBeam.events", "live event"),
    ("Tuplet.members", "live event"),
    ("Tuplet.parent", "extant tuplet"),
    ("Spanner.staves", "declared staff"),
    ("Spanner.start", "anchor target"),
    ("Spanner.end", "anchor target"),
    ("Marker.anchor", "anchor target"),
    ("RepeatStructure.start", "anchor target"),
    ("RepeatStructure.end", "anchor target"),
    ("RepeatStructure.kind", "anchor target"),
    ("RepeatStructure.voltas", "anchor target"),
    ("ChordSymbol.anchor", "anchor target"),
    (
        "AnalyticalAnnotation.anchor",
        "anchor target, extant region, live event",
    ),
    ("AnalyticalAnnotation.layer", "declared analysis layer"),
    ("Comment.anchor", "anchor target, extant region, live event"),
    ("GraphicGesture.objects", "stored graphic object"),
    (
        "GraphicGesture.anchoring",
        "anchor target, declared staff, live event",
    ),
    ("LyricLine.events", "live event"),
    ("Staff.instrument", "declared instrument"),
    ("StaffInstance.instrument_override", "declared instrument"),
    ("Staff.group", "declared staff group"),
    ("StaffGroup.members", "declared staff"),
    ("PartDefinition.staves", "declared staff"),
    ("ViewDefinition.active_layers", "declared analysis layer"),
    ("MetricTimeModel.meters", "declared time signature"),
    (
        "StaffBasedContent.default_metric_grid",
        "declared time signature",
    ),
    ("Measure.time_signature", "declared time signature"),
    ("StaffInstance.local_metric_grid", "declared time signature"),
    ("NotatedComponent.tuplet", "extant tuplet"),
    ("IndeterminacyHints.alternatives", "live event"),
    ("TrajectoryEvent.start", "live pitch"),
    ("TrajectoryEvent.end", "live pitch"),
    ("GraphicEvent.graphics", "stored graphic object"),
    ("CueEvent.source", "live event"),
    ("TempoSegment.start", "anchor target"),
    ("TempoSegment.end", "anchor target"),
];

/// Pin 1a's closed target vocabulary. A term outside it fails, and that is a
/// *separate* assertion from ordering: an out-of-vocabulary term sorts
/// perfectly well, so the canonical-form check cannot see it.
const TARGET_VOCABULARY: &[&str] = &[
    "anchor target",
    "declared analysis layer",
    "declared instrument",
    "declared staff",
    "declared staff group",
    "declared time signature",
    "extant region",
    "extant tuplet",
    "live event",
    "live pitch",
    "stored graphic object",
];

/// Item 10's opening sentence, pinned as the **complete** literal rather than a
/// prefix, so that pin 3's retention of it is machine-observed. Whitespace is
/// normalised on both sides before matching, because the `.tex` source is
/// hard-wrapped and rewrapping is presentational.
const ITEM_TEN_ANCHOR: &str = "\\item Except where the re-anchoring rules of \
     Chapter~\\ref{ch:semops} explicitly permit transient dangling states during \
     edits, every graph reference resolves to an extant object.";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = repository_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// Collapse every run of whitespace to one space.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sort a comma-separated target into pin 1a's canonical order.
fn canonical_target(target: &str) -> String {
    let mut terms: Vec<&str> = target.split(',').map(str::trim).collect();
    terms.sort_unstable();
    terms.join(", ")
}

/// Step 0. Validate the oracle before using it as one: a duplicate token would
/// vanish when the expected side becomes a set, and an out-of-vocabulary target
/// would be asserted and never observed.
fn validated_oracle() -> BTreeSet<(String, String)> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (token, _) in INVARIANT_TEN_SURFACE {
        *seen.entry(token).or_default() += 1;
    }
    let repeated: Vec<&str> = seen
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(token, _)| *token)
        .collect();
    assert!(
        repeated.is_empty(),
        "INVARIANT_TEN_SURFACE repeats {repeated:?}; a repeat collapses silently \
         into the expected set and would let both documents drop a class"
    );

    for (token, target) in INVARIANT_TEN_SURFACE {
        for term in target.split(',').map(str::trim) {
            assert!(
                TARGET_VOCABULARY.contains(&term),
                "INVARIANT_TEN_SURFACE target {target:?} for {token} uses \
                 {term:?}, which is outside pin 1a's vocabulary"
            );
        }
        assert_eq!(
            *target,
            canonical_target(target),
            "INVARIANT_TEN_SURFACE target for {token} is not in canonical order"
        );
    }

    INVARIANT_TEN_SURFACE
        .iter()
        .map(|(token, target)| ((*token).to_owned(), (*target).to_owned()))
        .collect()
}

/// Duplicate-token check on a raw extraction, plus the canonical-form checks.
/// Run before the set comparison, which cannot see any of them.
fn check_extraction(pairs: &[(String, String)], where_: &str) {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (token, _) in pairs {
        *seen.entry(token.as_str()).or_default() += 1;
    }
    let repeated: Vec<&str> = seen
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(token, _)| *token)
        .collect();
    assert!(
        repeated.is_empty(),
        "{where_} lists {repeated:?} more than once; set comparison cannot see a \
         duplicate, so it is checked here"
    );

    for (token, target) in pairs {
        assert_eq!(
            *target,
            canonical_target(target),
            "{where_}: target for {token} is not in pin 1a's canonical order"
        );
        for term in target.split(',').map(str::trim) {
            assert!(
                TARGET_VOCABULARY.contains(&term),
                "{where_}: target for {token} uses {term:?}, outside pin 1a's \
                 vocabulary. Ordering cannot catch this -- a bad term sorts fine"
            );
        }
    }
}

/// The `requirement` block that carries the graph invariants, normalised.
///
/// Slicing is mandatory, and stated here because it is not obvious: unsliced,
/// the extractor would collect every `\texttt{}` in `core_spec.tex` and
/// equality would fail on a flood of spurious pairs. The slice is what makes
/// the guard *function*, not what gives it teeth.
fn graph_invariants_block() -> String {
    let spec = normalise(&read("spec/core_spec.tex"));
    let label = r"\label{req:graph:score-graph-invariants}";
    let at = spec
        .find(label)
        .expect("core_spec.tex declares req:graph:score-graph-invariants");
    let start = spec[..at]
        .rfind(r"\begin{requirement}")
        .expect("the graph-invariants label sits inside a requirement block");
    let end = spec[start..]
        .find(r"\end{requirement}")
        .map(|offset| start + offset)
        .expect("that requirement block is closed");
    spec[start..end].to_owned()
}

#[test]
fn specification_item_ten_names_exactly_the_derived_surface() {
    let expected = validated_oracle();
    let block = graph_invariants_block();

    // The anchor is the complete opening sentence, not a prefix of it, so that
    // pin 3's retention of that sentence is machine-observed. It must occur
    // exactly once in the block, or the outer slice is ambiguous and nothing
    // else would notice.
    let anchor = normalise(ITEM_TEN_ANCHOR);
    let occurrences = block.matches(anchor.as_str()).count();
    assert_eq!(
        occurrences, 1,
        "item 10's opening sentence must occur exactly once inside \
         req:graph:score-graph-invariants; found {occurrences}"
    );

    let outer_start = block.find(anchor.as_str()).expect("checked above");
    let outer_end = outer_start + item_ten_length(&block[outer_start..]);
    let outer = &block[outer_start..outer_end];

    // Pin 3 forbids both of these inside item 10, and nothing else would catch
    // them: an accidental well-formed label is absorbed the moment pin 4's
    // count constants are remeasured. Recognition is whitespace-tolerant
    // because TeX accepts `\label {x}` and the repository's own parser does
    // not -- a guard shaped like that parser would inherit its blind spot.
    assert!(
        !contains_command(outer, "label"),
        "item 10 must contain no \\label: pin 3 adds no label and no \
         requirement block.\nSlice was:\n{outer}"
    );
    assert!(
        !contains_begin_requirement(outer),
        "item 10 must contain no requirement block.\nSlice was:\n{outer}"
    );

    let inner_start = outer
        .find(r"\begin{itemize}")
        .map(|offset| offset + r"\begin{itemize}".len())
        .expect("item 10 carries its nested itemize");
    let inner_end = outer[inner_start..]
        .find(r"\end{itemize}")
        .map(|offset| inner_start + offset)
        .expect("that itemize is closed");
    let inner = &outer[inner_start..inner_end];

    let pairs = extract_tex_pairs(inner);
    check_extraction(&pairs, "core_spec.tex item 10");
    let actual: BTreeSet<(String, String)> = pairs.into_iter().collect();
    assert_eq!(actual, expected);
}

/// Item 10 runs from its opening sentence to the next `\item` **at the
/// enumeration's own level**. Nested `itemize` environments carry `\item`s of
/// their own, so the scan tracks depth; a naive "next `\item`" ends the slice
/// inside the nested list and loses everything after it.
fn item_ten_length(rest: &str) -> usize {
    let mut depth = 0usize;
    let mut cursor = 0usize;
    while cursor < rest.len() {
        let tail = &rest[cursor..];
        if tail.starts_with(r"\begin{itemize}") {
            depth += 1;
            cursor += r"\begin{itemize}".len();
        } else if tail.starts_with(r"\end{itemize}") {
            depth = depth.saturating_sub(1);
            cursor += r"\end{itemize}".len();
        } else if depth == 0 && cursor > 0 && tail.starts_with(r"\item ") {
            return cursor;
        } else {
            cursor += tail.chars().next().map_or(1, char::len_utf8);
        }
    }
    rest.len()
}

/// Whitespace-tolerant `\command{` recognition.
fn contains_command(text: &str, command: &str) -> bool {
    let needle = format!("\\{command}");
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find(&needle) {
        let after = cursor + relative + needle.len();
        if text[after..].trim_start().starts_with('{') || command.ends_with('}') {
            return true;
        }
        cursor = after;
    }
    false
}

/// `\begin` followed by optional whitespace then `{requirement}`.
fn contains_begin_requirement(text: &str) -> bool {
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find("\\begin") {
        let after = cursor + relative + "\\begin".len();
        if text[after..].trim_start().starts_with("{requirement}") {
            return true;
        }
        cursor = after;
    }
    false
}

/// Per `\item`: the first `\texttt{}` argument is the token, the text between
/// `---` and the terminating period is the target.
fn extract_tex_pairs(inner: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for chunk in inner.split(r"\item ").skip(1) {
        let Some(open) = chunk.find(r"\texttt{") else {
            continue;
        };
        let token_start = open + r"\texttt{".len();
        let Some(close) = chunk[token_start..].find('}') else {
            continue;
        };
        let token = chunk[token_start..token_start + close].replace("\\_", "_");
        let rest = &chunk[token_start + close..];
        let Some(dash) = rest.find("---") else {
            continue;
        };
        let after_dash = &rest[dash + "---".len()..];
        let Some(period) = after_dash.find('.') else {
            continue;
        };
        pairs.push((token, after_dash[..period].trim().to_owned()));
    }
    pairs
}

#[test]
fn implementation_doc_names_exactly_the_derived_surface() {
    let expected = validated_oracle();
    let source = read("crates/epiphany-core/src/invariants.rs");
    let start = source
        .find("    /// 10. Every graph reference resolves")
        .expect("invariant 10's doc comment is present");
    let end = source[start..]
        .find("CrossCuttingRefsResolve,")
        .map(|offset| start + offset)
        .expect("the CrossCuttingRefsResolve variant follows its doc comment");

    let mut pairs = Vec::new();
    for line in source[start..end].lines() {
        let Some(rest) = line.trim_start().strip_prefix("/// ") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix("- ") else {
            continue;
        };
        let Some(token) = rest.split_whitespace().next() else {
            continue;
        };
        let Some(dash) = rest.find('\u{2014}') else {
            continue;
        };
        let after_dash = &rest[dash + '\u{2014}'.len_utf8()..];
        let Some(period) = after_dash.find('.') else {
            continue;
        };
        pairs.push((token.to_owned(), after_dash[..period].trim().to_owned()));
    }

    check_extraction(&pairs, "invariants.rs invariant-10 doc block");
    let actual: BTreeSet<(String, String)> = pairs.into_iter().collect();
    assert_eq!(actual, expected);
}

/// Select the requirement's **normative clause** -- the sentence carrying its
/// sole `\MUST{}` -- from a normalised requirement block.
///
/// Both halves of the start rule are load-bearing (P13-S26 amendment 3, 8.3).
/// The last-period rule is what makes the clause follow `\MUST{}` when the
/// normative force moves to a later sentence; without it, a slice anchored
/// unconditionally after the label would span label to recap and contain every
/// needle. The fallback is needed because the normative sentence is *first* in
/// this requirement, so no preceding ". " exists and a block-start default
/// would swallow the label, which is not a sentence.
fn normative_clause<'a>(block: &'a str, label: &str) -> &'a str {
    let occurrences = block.matches("\\MUST{}").count();
    assert_eq!(
        occurrences, 1,
        "the requirement must carry exactly one \\MUST{{}}; found {occurrences}. \
         More than one leaves the normative clause ambiguous, and an \
         implementation that silently took the first would scope every other \
         assertion to whichever sentence happened to come first.\nBlock was:\n{block}"
    );
    let p = block.find("\\MUST{}").expect("checked above");

    let start = match block[..p].rfind(". ") {
        Some(period) => period + ". ".len(),
        None => {
            let at = block.find(label).expect("the block declares its label");
            at + label.len()
        }
    };
    let end = match block[p..].find(". ") {
        Some(period) => p + period + 1,
        None => block.len(),
    };
    block[start..end].trim()
}

#[test]
fn aleatoric_reference_locality_states_both_referents_and_locality() {
    // Phrase presence, not exact comparison -- weaker than tests 1 and 2, and
    // stated as such. What it buys, since P13-S26 amendment 3: the phrases must
    // appear in the NORMATIVE CLAUSE, not merely somewhere in the block.
    // Execution measured the escape that motivated this: a referent deleted
    // from the clause but left standing in the closing recap passed the
    // block-scoped form.
    let spec = normalise(&read("spec/core_spec.tex"));
    let label = r"\label{req:time:aleatoric-reference-locality}";
    let at = spec
        .find(label)
        .expect("core_spec.tex declares req:time:aleatoric-reference-locality");
    let start = spec[..at]
        .rfind(r"\begin{requirement}")
        .expect("that label sits inside a requirement block");
    let end = spec[start..]
        .find(r"\end{requirement}")
        .map(|offset| start + offset)
        .expect("that requirement block is closed");
    let block = &spec[start..end];

    let clause = normative_clause(block, label);

    for needle in ["ordering", "bounds", "same region", "\\MUST{}"] {
        assert!(
            clause.contains(needle),
            "req:time:aleatoric-reference-locality's normative clause must state \
             {needle:?}; the clause is the sentence carrying \\MUST{{}}, and a \
             phrase surviving elsewhere in the block does not count.\n\
             Clause was:\n{clause}"
        );
    }
}
