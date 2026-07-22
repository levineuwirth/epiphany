//! Durable completeness and referential-integrity checks for requirement labels.
//!
//! Requirement labels are a public citation surface. These tests derive their
//! inputs from every specification document instead of maintaining a second list
//! of labels, and scan repository text so a dangling citation cannot hide in a
//! decision record or other consumer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const CORE_REQUIREMENT_COUNT: usize = 212;
const SUITE_REQUIREMENT_COUNT: usize = 282;
const SUITE_LABEL_COUNT: usize = 282;

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
    let core = documents
        .iter()
        .find(|document| document.name == "core_spec.tex")
        .expect("core_spec.tex was not scanned");
    assert_eq!(core.requirements.len(), CORE_REQUIREMENT_COUNT);

    let suite_count: usize = documents
        .iter()
        .map(|document| document.requirements.len())
        .sum();
    assert_eq!(suite_count, SUITE_REQUIREMENT_COUNT);

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
    assert_eq!(all_labels.len(), SUITE_LABEL_COUNT);

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
    assert_eq!(locations.len(), SUITE_LABEL_COUNT);
}

fn is_citation_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_')
}

/// Labels that are deliberately named in prose but are **not** requirements.
///
/// The citation scan cannot tell "cite this requirement" from "name a label that
/// does not exist" — and documenting a dangling label is a legitimate thing to do.
/// Without this escape the check forces prose to become vaguer than the finding it
/// records: it already rewrote a scoping plan's `req:layoutir:vertical-bands`
/// into a euphemism to make itself pass.
///
/// A row here is a claim that the string is discussed, never cited. Keep it short,
/// and give the reason.
const DISCUSSED_NOT_CITED: &[(&str, &str)] = &[(
    "req:layoutir:vertical-bands",
    "never existed; the Pass-12 log cited it for a behavioural fix no requirement \
     governs. Named in spec/PLAN_P13S1_LABELS.md as the finding that motivated \
     this checker.",
)];

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
    assert_eq!(defined.len(), SUITE_LABEL_COUNT);

    let root = repository_root();
    let mut paths = Vec::new();
    repository_text_files(&root, &mut paths);
    paths.sort();

    let mut cited = BTreeSet::new();
    let mut undefined: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in paths {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for citation in requirement_strings(&text) {
            cited.insert(citation.clone());
            if !defined.contains(&citation) {
                undefined.entry(citation).or_default().push(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }

    assert!(
        cited.len() >= SUITE_LABEL_COUNT,
        "citation scan found only {} distinct requirement strings",
        cited.len()
    );
    for (allowed, _) in DISCUSSED_NOT_CITED {
        undefined.remove(*allowed);
    }
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
