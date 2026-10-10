#![forbid(unsafe_code)]
//! `epiphany`: the first entry point a person can run on a file.
//!
//!     epiphany render <file.musicxml|file.musc> [--page N] [-o out.svg]
//!     epiphany import <file.musicxml>
//!     epiphany new <file.musicxml> -o <file.musc>
//!
//! `render` imports the file through the operation API, or opens it when it
//! is a saved document, engraves it and writes page N (default 1) as SVG to
//! `out.svg`, or to standard output. `new` imports a MusicXML file into a new
//! document, which must not exist yet: the import's operations are its log.
//! `import` prints what the import carried and what it could not: every
//! operation that did not apply, with the reducer's reason; the source
//! features not imported, by kind; the comparison against the source; and
//! the score's invariant violations.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use epiphany_cli::{
    engrave_loaded, engrave_score, is_document, load, new_document, open_document, page, svg,
    Loaded,
};
use epiphany_musicxml::source::FeatureClass;

const USAGE: &str =
    "usage: epiphany render <file.musicxml|file.musc> [--page N] [-o out.svg]\n       \
                     epiphany import <file.musicxml>\n       \
                     epiphany new <file.musicxml> -o <file.musc>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("render") => render_command(&args[1..]),
        Some("import") => import_command(&args[1..]),
        Some("new") => new_command(&args[1..]),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => fail(USAGE),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(2)
}

fn render_command(args: &[String]) -> ExitCode {
    let mut file: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut number: usize = 1;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--page" => match rest.next().and_then(|n| n.parse().ok()) {
                Some(n) if n >= 1 => number = n,
                _ => return fail("--page needs a page number from 1"),
            },
            "-o" => match rest.next() {
                Some(path) => output = Some(PathBuf::from(path)),
                None => return fail("-o needs a path"),
            },
            other if other.starts_with('-') => {
                return fail(&format!("unknown option {other}\n{USAGE}"))
            }
            other if file.is_none() => file = Some(PathBuf::from(other)),
            other => return fail(&format!("unexpected argument {other}\n{USAGE}")),
        }
    }
    let Some(file) = file else {
        return fail(USAGE);
    };
    match is_document(&file) {
        Ok(true) => return render_document(&file, number, output),
        Ok(false) => {}
        Err(e) => return fail(&format!("{}: {e}", file.display())),
    }
    let loaded = match load(&file) {
        Ok(loaded) => loaded,
        Err(e) => return fail(&format!("{}: {e}", file.display())),
    };
    let engraved = engrave_loaded(&loaded);
    let pages = engraved.layout.pages.len();
    let Some(layout) = page(&engraved.layout, number) else {
        return fail(&format!("{}: page {number} of {pages}", file.display()));
    };
    let text = svg(&layout);
    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, text) {
                return fail(&format!("{}: {e}", path.display()));
            }
        }
        None => print!("{text}"),
    }
    let rejected = loaded.reduced.rejected().count();
    eprintln!(
        "{}: page {number} of {pages}; {} operations, {rejected} not applied; fidelity {}",
        file.display(),
        loaded.import.envelopes.len(),
        if loaded.fidelity.passed() {
            "holds"
        } else {
            "FAILS"
        },
    );
    ExitCode::SUCCESS
}

/// Writes `text` to `output`, or to standard output.
fn write_output(output: Option<PathBuf>, text: &str) -> Result<(), ExitCode> {
    match output {
        Some(path) => {
            std::fs::write(&path, text).map_err(|e| fail(&format!("{}: {e}", path.display())))
        }
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

/// `render` on a saved document: its committed operations reduced and engraved
/// on the document's page.
fn render_document(file: &Path, number: usize, output: Option<PathBuf>) -> ExitCode {
    let document = match open_document(file) {
        Ok(document) => document,
        Err(e) => return fail(&format!("{}: {e}", file.display())),
    };
    let engraved = engrave_score(&document.score());
    let pages = engraved.layout.pages.len();
    let Some(layout) = page(&engraved.layout, number) else {
        return fail(&format!("{}: page {number} of {pages}", file.display()));
    };
    if let Err(code) = write_output(output, &svg(&layout)) {
        return code;
    }
    eprintln!(
        "{}: page {number} of {pages}; {} operations, generation {}{}",
        file.display(),
        document.committed_operations().len(),
        document.generation(),
        if document.is_read_only() {
            format!("; read-only: {:?}", document.read_only_reasons())
        } else {
            String::new()
        }
    );
    ExitCode::SUCCESS
}

fn new_command(args: &[String]) -> ExitCode {
    let [source, flag, target] = args else {
        return fail(USAGE);
    };
    if flag != "-o" {
        return fail(USAGE);
    }
    let (source, target) = (PathBuf::from(source), PathBuf::from(target));
    match new_document(&source, &target) {
        Ok(document) => {
            eprintln!(
                "{}: {} operations into {}{}",
                source.display(),
                document.committed_operations().len(),
                target.display(),
                if document.is_read_only() {
                    format!(" (read-only: {:?})", document.read_only_reasons())
                } else {
                    String::new()
                }
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("{}: {e}", source.display())),
    }
}

fn import_command(args: &[String]) -> ExitCode {
    let [file] = args else {
        return fail(USAGE);
    };
    match load(&PathBuf::from(file)) {
        Ok(loaded) => {
            print_import(&loaded);
            if loaded.fidelity.passed() && loaded.violations.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => fail(&format!("{file}: {e}")),
    }
}

/// Prints an import's summary.
pub fn print_import(loaded: &Loaded) {
    let source = &loaded.import.source;
    let events: usize = source.parts.iter().map(|p| p.events.len()).sum();
    println!(
        "parts {}, staves {}, measures {}, source events {}, concert score {}",
        source.parts.len(),
        source.parts.iter().map(|p| p.staves.len()).sum::<usize>(),
        source.measures.len(),
        events,
        source.concert
    );
    let mut classes: BTreeMap<&str, usize> = BTreeMap::new();
    for verdict in &loaded.reduced.verdicts {
        *classes.entry(verdict.class()).or_default() += 1;
    }
    println!(
        "operations {}: {}",
        loaded.import.envelopes.len(),
        classes
            .iter()
            .map(|(k, v)| format!("{v} {k}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut rejected: BTreeMap<(&str, &str, String), usize> = BTreeMap::new();
    for i in loaded.reduced.rejected() {
        let verdict = &loaded.reduced.verdicts[i];
        *rejected
            .entry((
                loaded.import.labels[i].kind,
                verdict.class(),
                verdict.reason().to_owned(),
            ))
            .or_default() += 1;
    }
    for ((kind, class, reason), count) in &rejected {
        println!("  rejected: {count} {kind} {class}: {reason}");
    }
    for class in [
        FeatureClass::Content,
        FeatureClass::Notation,
        FeatureClass::Presentation,
    ] {
        let kinds: Vec<String> = source
            .features
            .of_class(class)
            .map(|(kind, f)| format!("{kind} ×{}", f.places.len()))
            .collect();
        if !kinds.is_empty() {
            println!("not imported ({class:?}): {}", kinds.join(", "));
        }
    }
    println!(
        "fidelity: {} failures, {} explained by rejected operations",
        loaded.fidelity.failures.len(),
        loaded.fidelity.explained.len()
    );
    for failure in &loaded.fidelity.failures {
        println!("  FAIL {failure}");
    }
    for explained in loaded.fidelity.explained.iter().take(10) {
        println!("  explained: {explained}");
    }
    println!("invariant violations: {}", loaded.violations.len());
    for violation in loaded.violations.iter().take(10) {
        println!("  {:?}: {}", violation.kind, violation.witness);
    }
    println!(
        "read {:.2?}, reduce {:.2?}",
        loaded.read_time, loaded.reduce_time
    );
}
