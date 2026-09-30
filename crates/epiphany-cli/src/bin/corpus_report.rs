#![forbid(unsafe_code)]
//! `corpus-report`: imports and engraves every score `scripts/corpus-export`
//! wrote, and reports per score its parts, measures and imported events; the
//! three columns (source features not imported, operations the reducer did
//! not apply, what the engraver left out), each by kind; the comparison with
//! the source; invariant violations; and read, reduce and engrave times.
//!
//!     corpus-report [--cache DIR] [--render NAME]...
//!
//! DIR defaults to `${XDG_CACHE_HOME:-~/.cache}/epiphany-corpus`. A score is
//! `DIR/<group>/<name>/<name>.musicxml` with its `<name>.exported` stamp.
//! Every score in the `excerpts` group, `s7`, and each `--render NAME` is
//! rendered in full, page by page, as SVG and (through `rsvg-convert`) PNG,
//! with an `index.html` that sets each page beside MuseScore's own pages of
//! the same score. Everything is written under `DIR/report/`, and the printed
//! report is also saved there as `report.txt`. The scores are private: this
//! writes nothing outside that directory.
//!
//! The process holds a whole score in memory; run it under a memory cap
//! (`systemd-run --user --scope -p MemoryMax=...`) on a shared machine.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use epiphany_cli::omissions::omissions;
use epiphany_cli::{engrave, load, page, svg, Loaded};
use epiphany_musicxml::source::FeatureClass;

struct Score {
    group: String,
    name: String,
    musicxml: PathBuf,
}

fn scores(cache: &Path) -> std::io::Result<Vec<Score>> {
    let mut out = Vec::new();
    for group in std::fs::read_dir(cache)? {
        let group = group?.path();
        let Some(group_name) = group.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !group.is_dir() || group_name.starts_with('.') || group_name == "report" {
            continue;
        }
        for dir in std::fs::read_dir(&group)? {
            let dir = dir?.path();
            let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let musicxml = dir.join(format!("{name}.musicxml"));
            if musicxml.is_file() && dir.join(format!("{name}.exported")).is_file() {
                out.push(Score {
                    group: group_name.to_owned(),
                    name: name.to_owned(),
                    musicxml,
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
    Ok(out)
}

fn joined<'a>(items: impl Iterator<Item = (&'a str, usize)>) -> String {
    let parts: Vec<String> = items.map(|(k, n)| format!("{k} ×{n}")).collect();
    if parts.is_empty() {
        String::from("none")
    } else {
        parts.join(", ")
    }
}

/// Writes the start of one score's block; returns its events and rejections.
fn report(loaded: &Loaded, report: &mut String) -> (usize, usize) {
    let source = &loaded.import.source;
    let fidelity = &loaded.fidelity;
    let mut totals = epiphany_musicxml::fidelity::Counts::default();
    for part in &fidelity.counts {
        for measure in part {
            totals += *measure;
        }
    }
    let events: usize = fidelity.events.iter().sum();
    let _ = writeln!(
        report,
        "parts {}, staves {}, measures {}{}",
        source.parts.len(),
        source.parts.iter().map(|p| p.staves.len()).sum::<usize>(),
        source.measures.len(),
        if source.concert {
            ", concert score"
        } else {
            ""
        }
    );
    let rejected: Vec<usize> = loaded.reduced.rejected().collect();
    let _ = writeln!(
        report,
        "imported: {events} events ({} notes, {} rests, {} chords) by {} operations, {} applied",
        totals.notes,
        totals.rests,
        totals.chords,
        loaded.import.envelopes.len(),
        loaded.import.envelopes.len() - rejected.len()
    );
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for &i in &rejected {
        let verdict = &loaded.reduced.verdicts[i];
        *by_kind
            .entry(format!(
                "{} {}: {}",
                loaded.import.labels[i].kind,
                verdict.class(),
                verdict.reason()
            ))
            .or_default() += 1;
    }
    let _ = writeln!(
        report,
        "  rejected operations: {}",
        joined(by_kind.iter().map(|(k, n)| (k.as_str(), *n)))
    );
    let features = |class| {
        source
            .features
            .of_class(class)
            .map(|(k, f)| (k, f.places.len()))
    };
    let _ = writeln!(
        report,
        "  unsupported source features: {}",
        joined(features(FeatureClass::Content))
    );
    let _ = writeln!(
        report,
        "    notation not imported: {}",
        joined(features(FeatureClass::Notation))
    );
    let presentation: usize = features(FeatureClass::Presentation).map(|(_, n)| n).sum();
    let _ = writeln!(
        report,
        "    presentation and playback not imported: {} kinds, {presentation} elements",
        features(FeatureClass::Presentation).count()
    );
    (events, rejected.len())
}

fn main() -> ExitCode {
    let mut cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache")))
        .map(|c| c.join("epiphany-corpus"))
        .unwrap_or_default();
    let mut extra: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cache" => match args.next() {
                Some(dir) => cache = PathBuf::from(dir),
                None => return fail("--cache needs a directory"),
            },
            "--render" => match args.next() {
                Some(name) => extra.push(name),
                None => return fail("--render needs a score name"),
            },
            "-h" | "--help" => {
                println!("usage: corpus-report [--cache DIR] [--render NAME]...");
                return ExitCode::SUCCESS;
            }
            other => return fail(&format!("unknown argument {other}")),
        }
    }
    let list = match scores(&cache) {
        Ok(list) if !list.is_empty() => list,
        Ok(_) => return fail(&format!("no exported score under {}", cache.display())),
        Err(e) => return fail(&format!("{}: {e}", cache.display())),
    };
    let out_root = cache.join("report");
    if let Err(e) = std::fs::create_dir_all(&out_root) {
        return fail(&format!("{}: {e}", out_root.display()));
    }
    let partial = out_root.join("report.txt.partial");
    let _ = std::fs::remove_file(&partial);

    let mut text = String::new();
    let mut summary = Vec::new();
    let mut failed = false;
    for score in &list {
        let title = format!("{}/{}", score.group, score.name);
        let _ = writeln!(text, "\n== {title}");
        let loaded = match load(&score.musicxml) {
            Ok(loaded) => loaded,
            Err(e) => {
                let _ = writeln!(text, "REFUSED: {e}");
                summary.push(format!("{title:40} refused: {e}"));
                print!("{text}");
                text.clear();
                continue;
            }
        };
        let (events, rejected) = report(&loaded, &mut text);
        let engraved = engrave(&loaded.reduced.score);
        let omitted = omissions(
            &loaded.reduced.score,
            &engraved.layout,
            &engraved.diagnostics,
        );
        let _ = writeln!(
            text,
            "  engraving omissions: {}",
            joined(omitted.kinds.iter().map(|(k, n)| (k.as_str(), *n)))
        );
        let _ = writeln!(
            text,
            "    not checked: {}",
            joined(omitted.unchecked.iter().map(|(k, n)| (k.as_str(), *n)))
        );
        let fidelity = if loaded.fidelity.passed() {
            format!(
                "holds ({} differences explained by rejected operations)",
                loaded.fidelity.explained.len()
            )
        } else {
            failed = true;
            format!("FAILS with {} differences", loaded.fidelity.failures.len())
        };
        let _ = writeln!(text, "  fidelity: {fidelity}");
        for failure in &loaded.fidelity.failures {
            let _ = writeln!(text, "    FAIL {failure}");
        }
        let _ = writeln!(text, "  invariant violations: {}", loaded.violations.len());
        for violation in loaded.violations.iter().take(20) {
            let _ = writeln!(text, "    {:?}: {}", violation.kind, violation.witness);
        }
        failed |= !loaded.violations.is_empty();
        let pages = engraved.layout.pages.len();
        let _ = writeln!(
            text,
            "  times: read {:.1?}, reduce {:.1?}, engrave {:.1?}; {pages} pages",
            loaded.read_time, loaded.reduce_time, engraved.time
        );
        let omitted_total: usize = omitted.kinds.values().sum();
        summary.push(format!(
            "{title:40} events {events:6}  rejected {rejected:4}  unsupported kinds {:3}  \
             omissions {omitted_total:6}  fidelity {}  violations {}",
            loaded
                .import
                .source
                .features
                .of_class(FeatureClass::Content)
                .count(),
            if loaded.fidelity.passed() {
                "holds"
            } else {
                "FAILS"
            },
            loaded.violations.len()
        ));

        if score.group == "excerpts" || score.name == "s7" || extra.contains(&score.name) {
            match render(&out_root, score, &engraved.layout) {
                Ok(line) => {
                    let _ = writeln!(text, "  rendered: {line}");
                }
                Err(e) => {
                    let _ = writeln!(text, "  render FAILED: {e}");
                    failed = true;
                }
            }
        }
        print!("{text}");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial)
            .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()));
        text.clear();
    }
    let mut closing = String::from("\n== summary\n");
    for line in &summary {
        let _ = writeln!(closing, "{line}");
    }
    print!("{closing}");
    let whole = std::fs::read_to_string(&partial).unwrap_or_default() + &closing;
    let _ = std::fs::write(out_root.join("report.txt"), whole);
    let _ = std::fs::remove_file(partial);
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Renders every page of a score, converts each to PNG, and writes an
/// `index.html` setting the pages beside MuseScore's.
fn render(
    out_root: &Path,
    score: &Score,
    layout: &epiphany_layout_ir::ResolvedLayoutIR,
) -> Result<String, String> {
    let dir = out_root.join(&score.group).join(&score.name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut ours = Vec::new();
    let mut png_note = String::new();
    for number in 1..=layout.pages.len() {
        let Some(one) = page(layout, number) else {
            break;
        };
        let svg_path = dir.join(format!("page-{number}.svg"));
        std::fs::write(&svg_path, svg(&one)).map_err(|e| format!("{}: {e}", svg_path.display()))?;
        let png_path = dir.join(format!("page-{number}.png"));
        let converted = Command::new("rsvg-convert")
            .args(["-b", "white", "-w", "1600", "-o"])
            .arg(&png_path)
            .arg(&svg_path)
            .status();
        match converted {
            Ok(status) if status.success() => ours.push(format!("page-{number}.png")),
            Ok(status) => png_note = format!("; rsvg-convert exited {status}"),
            Err(e) => png_note = format!("; no PNG: rsvg-convert: {e}"),
        }
    }
    let source_dir = score.musicxml.parent().unwrap_or(Path::new("."));
    let mut theirs = Vec::new();
    for n in 1.. {
        let png = source_dir.join(format!("{}-{n}.png", score.name));
        if !png.is_file() {
            break;
        }
        theirs.push(png);
    }
    let mut html = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>{name}</title>\n\
         <style>body{{font-family:sans-serif;margin:16px}}td{{vertical-align:top;width:50%}}\
         img{{width:100%;border:1px solid #ccc}}</style>\n\
         <h1>{name}</h1><p>Epiphany, imported from MuseScore's MusicXML export, \
         beside MuseScore's own rendering. Pagination differs: compare the same \
         measures, not the same page numbers.</p>\n<table><tr><th>Epiphany</th><th>MuseScore</th></tr>\n",
        name = score.name
    );
    for row in 0..ours.len().max(theirs.len()) {
        let left = ours
            .get(row)
            .map(|p| format!("<img src=\"{p}\">"))
            .unwrap_or_default();
        let right = theirs
            .get(row)
            .map(|p| format!("<img src=\"file://{}\">", p.display()))
            .unwrap_or_default();
        let _ = writeln!(html, "<tr><td>{left}</td><td>{right}</td></tr>");
    }
    html.push_str("</table>\n");
    let index = dir.join("index.html");
    std::fs::write(&index, html).map_err(|e| format!("{}: {e}", index.display()))?;
    Ok(format!(
        "{} pages to {} ({} MuseScore pages beside){png_note}",
        layout.pages.len(),
        index.display(),
        theirs.len()
    ))
}

fn fail(message: &str) -> ExitCode {
    eprintln!("corpus-report: {message}");
    ExitCode::from(2)
}
