//! Durable completeness and referential-integrity checks for requirement labels.
//!
//! Requirement labels are a public citation surface. These tests derive their
//! inputs from every specification document instead of maintaining a second list
//! of labels, and scan repository text so a dangling citation cannot hide in the
//! Rust sources, crate documentation or the `.tex` suite itself. Markdown under
//! `spec/` is out of the citation scan: it is drafts and historical prose, which
//! may name labels that do not, or no longer, exist.
//!
//! No hand-typed total appears here. Each total used to be compared against the
//! scanner that produced the other side, and had to be edited whenever a `.tex`
//! gained a requirement. What they guarded, that the scanner sees every
//! requirement, is checked instead against sources the scanner does not
//! produce: a second tokenizer that reads control words the way TeX does
//! (`the_scanner_sees_every_requirement_tex_reads`), the hand-maintained
//! chapter-area table (`every_declared_chapter_holds_a_requirement`), and the
//! byte-level citation scanner (`every_requirement_citation_is_defined`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The normative chapter-to-area assignment. Keeping this as data makes adding a
/// requirement under the wrong chapter fail without encoding chapter names in
/// test control flow.
const CHAPTER_AREAS: &[(&str, &str, &str)] = &[
    ("core_spec.tex", "Pitch", "pitch"),
    ("core_spec.tex", "Time and Duration", "time"),
    ("core_spec.tex", "Tuning Systems and Pitch Spaces", "tuning"),
    ("core_spec.tex", "The Score Graph", "graph"),
    (
        "core_spec.tex",
        "Semantic Operations and Concurrent Reduction",
        "semops",
    ),
    (
        "core_spec.tex",
        "Layout Intermediate Representation",
        "layoutir",
    ),
    ("core_spec.tex", "File Format", "format"),
    ("core_spec.tex", "Constraint-Solver Interface", "solver"),
    ("core_spec.tex", "Performance Requirements", "perf"),
    ("core_spec.tex", "Extension Points", "ext"),
    (
        "core_spec.tex",
        "Intentionally Deferred Types and Specifications",
        "deferred",
    ),
    ("core_spec.tex", "Determinism Contract", "determinism"),
    ("binary_format.tex", "Encoding Conventions", "binfmt"),
    ("binary_format.tex", "Identifiers and Derivations", "binfmt"),
    ("binary_format.tex", "Graph Value Layouts", "binfmt"),
    ("binary_format.tex", "Operation Wire Forms", "binfmt"),
    ("binary_format.tex", "Bundle Physical Layout", "binfmt"),
    (
        "binary_format.tex",
        "Extension Declaration Blobs and Edit Barriers",
        "binfmt",
    ),
    ("binary_format.tex", "Golden Anchor Registry", "binfmt"),
    ("operation_catalog.tex", "The Catalog Framework", "catalog"),
    (
        "operation_catalog.tex",
        "K0 --- Representative Primitives",
        "opcat",
    ),
    (
        "operation_catalog.tex",
        "v0 \\texorpdfstring{$\\rightarrow$}{->} v1 Payload Migration",
        "migration",
    ),
    ("quality_metric_catalog.tex", "The Metric Model", "qmc"),
    (
        "quality_metric_catalog.tex",
        "The Nine Normative Metrics",
        "qmc",
    ),
    (
        "quality_metric_catalog.tex",
        "Default Tie-Breaking Weights",
        "qmc",
    ),
    (
        "quality_metric_catalog.tex",
        "Per-Tier Metric Thresholds",
        "qmc",
    ),
    (
        "quality_metric_catalog.tex",
        "The Registered Profile Catalog",
        "qmc",
    ),
    ("reference_suite.tex", "The Suite Entry Model", "refsuite"),
    ("reference_suite.tex", "The v0.1 Entry Set", "refsuite"),
    ("text_projection.tex", "The Canonical Text Form", "textproj"),
    ("text_projection.tex", "What Is Projected", "textproj"),
    ("text_projection.tex", "Requirements", "textproj"),
];

#[derive(Debug)]
struct RequirementBlock {
    chapter: String,
    line: usize,
    labels: Vec<String>,
}

#[derive(Debug)]
struct SpecDocument {
    name: String,
    text: String,
    requirements: Vec<RequirementBlock>,
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn command_arguments(text: &str, command: &str) -> Vec<(usize, String)> {
    let needle = format!(r"\{command}{{");
    let mut arguments = Vec::new();
    let mut cursor = 0;

    while let Some(relative) = text[cursor..].find(&needle) {
        let command_start = cursor + relative;
        let argument_start = command_start + needle.len();
        let bytes = text.as_bytes();
        let mut depth = 1usize;
        let mut end = argument_start;

        while end < bytes.len() && depth != 0 {
            match bytes[end] {
                b'{' if end == 0 || bytes[end - 1] != b'\\' => depth += 1,
                b'}' if end == 0 || bytes[end - 1] != b'\\' => depth -= 1,
                _ => {}
            }
            end += 1;
        }

        assert_eq!(
            depth, 0,
            "unterminated \\{command} argument at byte {command_start}"
        );
        arguments.push((command_start, text[argument_start..end - 1].to_owned()));
        cursor = end;
    }

    arguments
}

fn labels(text: &str) -> Vec<String> {
    command_arguments(text, "label")
        .into_iter()
        .map(|(_, label)| label)
        .filter(|label| label.starts_with("req:"))
        .collect()
}

fn line_number(text: &str, byte: usize) -> usize {
    text[..byte].bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn load_spec(path: &Path) -> SpecDocument {
    let text = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    });
    let chapters = command_arguments(&text, "chapter");
    let begin = r"\begin{requirement}";
    let end = r"\end{requirement}";
    let mut requirements = Vec::new();
    let mut cursor = 0;

    while let Some(relative) = text[cursor..].find(begin) {
        let block_start = cursor + relative;
        let body_start = block_start + begin.len();
        let body_end = text[body_start..]
            .find(end)
            .map(|relative_end| body_start + relative_end)
            .unwrap_or_else(|| panic!("unterminated requirement in {}", path.display()));
        let chapter = chapters
            .iter()
            .rev()
            .find(|(position, _)| *position < block_start)
            .map(|(_, title)| title.clone())
            .unwrap_or_else(|| {
                panic!(
                    "requirement before first chapter in {}:{}",
                    path.display(),
                    line_number(&text, block_start)
                )
            });
        requirements.push(RequirementBlock {
            chapter,
            line: line_number(&text, block_start),
            labels: labels(&text[body_start..body_end]),
        });
        cursor = body_end + end.len();
    }

    SpecDocument {
        name: path
            .file_name()
            .expect("specification path has a file name")
            .to_string_lossy()
            .into_owned(),
        text,
        requirements,
    }
}

fn specification_documents() -> Vec<SpecDocument> {
    let spec = repository_root().join("spec");
    let mut paths: Vec<_> = fs::read_dir(&spec)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", spec.display()))
        .map(|entry| entry.expect("failed to read spec directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "tex"))
        .collect();
    paths.sort();
    paths.iter().map(|path| load_spec(path)).collect()
}

fn label_parts(label: &str) -> Option<(&str, &str)> {
    let mut parts = label.split(':');
    if parts.next()? != "req" {
        return None;
    }
    let area = parts.next()?;
    let slug = parts.next()?;
    if parts.next().is_some()
        || area.is_empty()
        || !area
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !slug
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return None;
    }
    Some((area, slug))
}

fn all_defined_labels(documents: &[SpecDocument]) -> BTreeSet<String> {
    documents
        .iter()
        .flat_map(|document| labels(&document.text))
        .collect()
}

#[test]
fn every_requirement_block_has_one_label() {
    let documents = specification_documents();
    let failures: Vec<_> = documents
        .iter()
        .flat_map(|document| {
            document
                .requirements
                .iter()
                .filter(|requirement| requirement.labels.len() != 1)
                .map(|requirement| {
                    format!(
                        "{}:{} has {} requirement labels",
                        document.name,
                        requirement.line,
                        requirement.labels.len()
                    )
                })
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn requirement_labels_follow_the_grammar() {
    let documents = specification_documents();
    let all_labels: Vec<_> = documents
        .iter()
        .flat_map(|document| labels(&document.text))
        .collect();
    let malformed: Vec<_> = all_labels
        .iter()
        .filter(|label| label_parts(label).is_none())
        .collect();
    assert!(
        malformed.is_empty(),
        "malformed requirement labels: {malformed:?}"
    );
}

#[test]
fn requirement_label_areas_match_their_chapters() {
    let documents = specification_documents();
    let expected: BTreeMap<_, _> = CHAPTER_AREAS
        .iter()
        .map(|(file, chapter, area)| ((*file, *chapter), *area))
        .collect();
    assert_eq!(expected.len(), CHAPTER_AREAS.len(), "duplicate area data");

    let mut failures = Vec::new();
    for document in &documents {
        for requirement in &document.requirements {
            let Some(label) = requirement.labels.first() else {
                continue;
            };
            let Some((area, _)) = label_parts(label) else {
                continue;
            };
            let expected_area = expected
                .get(&(document.name.as_str(), requirement.chapter.as_str()))
                .unwrap_or_else(|| {
                    panic!(
                        "missing chapter-area data for {} chapter {:?}",
                        document.name, requirement.chapter
                    )
                });
            if area != *expected_area {
                failures.push(format!(
                    "{}:{} chapter {:?} requires area {:?}, found {label}",
                    document.name, requirement.line, requirement.chapter, expected_area
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn requirement_labels_are_unique_across_the_suite() {
    let documents = specification_documents();
    let mut locations: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for document in &documents {
        for requirement in &document.requirements {
            for label in &requirement.labels {
                locations
                    .entry(label.clone())
                    .or_default()
                    .push(format!("{}:{}", document.name, requirement.line));
            }
        }
    }

    let duplicates: Vec<_> = locations
        .iter()
        .filter(|(_, occurrences)| occurrences.len() > 1)
        .map(|(label, occurrences)| format!("{label}: {}", occurrences.join(", ")))
        .collect();
    assert!(duplicates.is_empty(), "{}", duplicates.join("\n"));

    // A `req:` label outside every requirement block is invisible to the checks
    // above, which walk blocks.
    let outside: Vec<_> = all_defined_labels(&documents)
        .into_iter()
        .filter(|label| !locations.contains_key(label))
        .collect();
    assert!(
        outside.is_empty(),
        "requirement labels outside any requirement block: {outside:?}"
    );
}

/// The brace argument of every `\<word>` in `text`, found the way TeX reads a
/// control word rather than the way [`command_arguments`] does.
///
/// Deliberately a second implementation sharing no code with
/// `command_arguments`, because it is what that scanner is checked against. TeX
/// ends a control word at the first non-letter and skips the whitespace after
/// it, so `\label {req:x}` and `\begin {requirement}` are the same commands to
/// TeX as their unspaced spellings, while `command_arguments` matches only the
/// unspaced literal. A requirement written in a spelling the scanner cannot see
/// would otherwise drop out of every other check here without a trace.
fn tex_control_word_arguments(text: &str, word: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut arguments = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        if bytes[cursor] != b'\\' {
            cursor += 1;
            continue;
        }
        let name_start = cursor + 1;
        let mut name_end = name_start;
        while name_end < bytes.len() && bytes[name_end].is_ascii_alphabetic() {
            name_end += 1;
        }
        if name_end == name_start {
            // A control symbol (`\\`, `\%`, `\{`) is one character long.
            cursor = name_start + 1;
            continue;
        }
        cursor = name_end;
        if &text[name_start..name_end] != word {
            continue;
        }

        let mut open = name_end;
        while open < bytes.len() && bytes[open].is_ascii_whitespace() {
            open += 1;
        }
        if open >= bytes.len() || bytes[open] != b'{' {
            continue;
        }
        let mut depth = 0usize;
        let mut close = open;
        while close < bytes.len() {
            match bytes[close] {
                b'\\' => close += 1,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            close += 1;
        }
        assert!(
            close < bytes.len(),
            "unterminated \\{word} argument at byte {name_start}"
        );
        arguments.push(text[open + 1..close].to_owned());
        cursor = close + 1;
    }

    arguments
}

/// The completeness check the hand-typed totals used to make, against a source
/// the scanner does not produce: TeX's reading of the same text. Every
/// requirement label, `\begin{requirement}` and `\end{requirement}` that TeX
/// would see must be one the scanner saw, per document, in order.
#[test]
fn the_scanner_sees_every_requirement_tex_reads() {
    let documents = specification_documents();
    let mut failures = Vec::new();
    for document in &documents {
        let tex_labels: Vec<_> = tex_control_word_arguments(&document.text, "label")
            .into_iter()
            .filter(|label| label.starts_with("req:"))
            .collect();
        let scanned_labels = labels(&document.text);
        if tex_labels != scanned_labels {
            let tex_set: BTreeSet<_> = tex_labels.iter().collect();
            let scanned_set: BTreeSet<_> = scanned_labels.iter().collect();
            failures.push(format!(
                "{}: TeX reads {} requirement labels and the scanner {}; only TeX: {:?}; \
                 only the scanner: {:?}",
                document.name,
                tex_labels.len(),
                scanned_labels.len(),
                tex_set.difference(&scanned_set).collect::<Vec<_>>(),
                scanned_set.difference(&tex_set).collect::<Vec<_>>(),
            ));
        }

        let blocks = document.requirements.len();
        for boundary in ["begin", "end"] {
            let tex_count = tex_control_word_arguments(&document.text, boundary)
                .iter()
                .filter(|argument| *argument == "requirement")
                .count();
            if tex_count != blocks {
                failures.push(format!(
                    "{}: TeX reads {tex_count} \\{boundary}{{requirement}} and the scanner \
                     found {blocks} requirement blocks",
                    document.name
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `CHAPTER_AREAS` is hand-maintained and changes only when a chapter gains its
/// first requirement, so it is a statement of where requirements live that the
/// scanner does not produce. A document or chapter the scanner stops reading
/// shows here as a declared chapter with nothing in it.
#[test]
fn every_declared_chapter_holds_a_requirement() {
    let documents = specification_documents();
    let populated: BTreeSet<(&str, &str)> = documents
        .iter()
        .flat_map(|document| {
            document
                .requirements
                .iter()
                .map(|requirement| (document.name.as_str(), requirement.chapter.as_str()))
        })
        .collect();
    let empty: Vec<_> = CHAPTER_AREAS
        .iter()
        .filter(|(file, chapter, _)| !populated.contains(&(*file, *chapter)))
        .map(|(file, chapter, _)| format!("{file} chapter {chapter:?}"))
        .collect();
    assert!(
        empty.is_empty(),
        "declared in CHAPTER_AREAS, but the scanner found no requirement there:\n{}",
        empty.join("\n")
    );
}

fn is_citation_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_')
}

fn requirement_strings(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut found = BTreeSet::new();
    let mut cursor = 0;

    while cursor + 4 <= bytes.len() {
        if &bytes[cursor..cursor + 4] != b"req:"
            || (cursor > 0 && is_citation_byte(bytes[cursor - 1]))
        {
            cursor += 1;
            continue;
        }

        let mut end = cursor + 4;
        while end < bytes.len() && is_citation_byte(bytes[end]) {
            end += 1;
        }
        let candidate = &text[cursor..end];
        if candidate.bytes().filter(|byte| *byte == b':').count() >= 2 && !candidate.ends_with(':')
        {
            found.insert(candidate.to_owned());
        }
        cursor = end;
    }

    found
}

fn is_generated_artifact(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_str(),
            Some("aux" | "fdb_latexmk" | "fls" | "log" | "out" | "pdf" | "toc" | "xdv")
        )
    })
}

/// Whether the citation scan reads `relative`, a path from the repository root.
///
/// Everything is read except Markdown under `spec/`: contract drafts, evidence
/// files, ledgers and handoffs, which are drafts and historical prose and may
/// legitimately name a label that does not, or no longer, exist. The `.tex`
/// suite, the Rust sources (whose `ViolationKind::Requirement` carries
/// requirement names as public identifiers) and crate documentation stay in.
fn in_citation_scope(relative: &Path) -> bool {
    let historical_prose = relative.starts_with("spec")
        && relative
            .extension()
            .is_some_and(|extension| extension == "md");
    !historical_prose
}

#[test]
fn the_citation_scope_excludes_only_markdown_under_spec() {
    for excluded in [
        "spec/PASS13_CANDIDATES.md",
        "spec/CONTRACT_EXAMPLE_DRAFT.md",
        "spec/archive/EVIDENCE_EXAMPLE.md",
    ] {
        assert!(
            !in_citation_scope(Path::new(excluded)),
            "{excluded} is historical prose and must be out of scope"
        );
    }
    for included in [
        "spec/core_spec.tex",
        "spec/vectors/decode_vectors.txt",
        "crates/epiphany-core/src/invariants.rs",
        "crates/epiphany-ops/DECISIONS.md",
        "docs/invariants.md",
        "CLAUDE.md",
        "specimen/notes.md",
    ] {
        assert!(
            in_citation_scope(Path::new(included)),
            "{included} must stay in the citation scan"
        );
    }
}

fn repository_text_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
    {
        let path = entry.expect("failed to read repository entry").path();
        if path.is_dir() {
            let name = path.file_name().and_then(|name| name.to_str());
            if !matches!(name, Some(".git" | "target")) {
                repository_text_files(&path, files);
            }
        } else if !is_generated_artifact(&path) {
            files.push(path);
        }
    }
}

#[test]
fn every_requirement_citation_is_defined() {
    let documents = specification_documents();
    let defined = all_defined_labels(&documents);

    let root = repository_root();
    let mut paths = Vec::new();
    repository_text_files(&root, &mut paths);
    paths.sort();

    let mut cited = BTreeSet::new();
    let mut undefined: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in paths {
        let relative = path.strip_prefix(&root).unwrap_or(&path);
        if !in_citation_scope(relative) {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for citation in requirement_strings(&text) {
            cited.insert(citation.clone());
            if !defined.contains(&citation) {
                undefined
                    .entry(citation)
                    .or_default()
                    .push(relative.display().to_string());
            }
        }
    }

    // The scan reads the `.tex` that defines each label, so it must have seen
    // every one. This checks the byte-level scanner and the scope against the
    // label scanner, a separate implementation; a scope that dropped the suite
    // fails here.
    let unseen: Vec<_> = defined.difference(&cited).collect();
    assert!(
        unseen.is_empty(),
        "the citation scan did not see these defined labels: {unseen:?}"
    );
    let failures: Vec<_> = undefined
        .iter()
        .map(|(citation, paths)| format!("{citation}: {}", paths.join(", ")))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Each document must step the requirement counter with a `code=` key, and must
/// **not** use tcolorbox's own `auto counter`.
///
/// This looks like a style preference and is not. `auto counter` steps its
/// counter for `\label` purposes inside an internal `\sbox`, and
/// `\refstepcounter`'s effect on `\@currentlabel` is a *local* assignment that is
/// discarded when that box closes — before a `\label` written in the box body
/// ever runs. Every requirement in this suite is labelled that way. The result is
/// the failure mode this counter exists to fix, wearing a disguise: the box
/// titles number 1, 2, 3 correctly while the cross-references bind to the last
/// sectioning unit and silently point at the wrong requirement.
///
/// Measured on a three-box test document: titles rendered `1.1 1.2 1.3` while the
/// three refs resolved to `1.1 1.1 1.2`.
///
/// So this is a regression lock, not a lint. `every_requirement_block_has_one_label`
/// would stay green through that change, and so would every uniqueness check —
/// the labels remain unique, they merely resolve to the wrong numbers.
/// Strips LaTeX `%` comments, honouring `\%`.
///
/// Needed because the box definitions carry a comment *naming* `auto counter` to
/// explain why it is not used. A check that cannot tell code from a comment about
/// the code fires on its own documentation.
fn without_latex_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let bytes = line.as_bytes();
        let mut end = line.len();
        for (i, _) in line.char_indices() {
            if bytes[i] == b'%' && (i == 0 || bytes[i - 1] != b'\\') {
                end = i;
                break;
            }
        }
        out.push_str(&line[..end]);
        out.push('\n');
    }
    out
}

#[test]
fn requirement_counters_are_stepped_where_the_label_can_see_it() {
    let documents = specification_documents();
    let mut checked = 0usize;
    for document in &documents {
        let text = without_latex_comments(&document.text);
        if !text.contains("\\newtcolorbox{requirement}") {
            continue;
        }
        checked += 1;
        let name = &document.name;
        assert!(
            text.contains("code={\\refstepcounter{requirement}}"),
            "{name}: the requirement box must step its counter via `code=`, which runs \
             in the environment's own group so a `\\label` in the body sees it"
        );
        assert!(
            !text.contains("auto counter"),
            "{name}: tcolorbox's `auto counter` steps the counter inside an \\sbox, so \
             every `\\label` in a box body silently binds to the enclosing section \
             instead. Titles look right; cross-references do not. See the comment at \
             the box definition."
        );
    }
    assert_eq!(
        checked,
        documents.len(),
        "every specification document defines a requirement box; if one stopped, this \
         lock silently stopped covering it"
    );
}
