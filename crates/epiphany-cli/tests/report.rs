//! `corpus-report` over a cache laid out as `corpus-export` writes it,
//! built from the importer's hand-written fixtures.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../epiphany-musicxml/tests/fixtures")
        .join(name)
}

/// Writes `DIR/<group>/<name>/<name>.musicxml`, stamped or not.
fn export(cache: &Path, group: &str, name: &str, xml: &str, stamped: bool) {
    let dir = cache.join(group).join(name);
    std::fs::create_dir_all(&dir).expect("a score directory");
    std::fs::write(dir.join(format!("{name}.musicxml")), xml).expect("written");
    if stamped {
        std::fs::write(dir.join(format!("{name}.exported")), "").expect("stamped");
    }
}

#[test]
fn the_report_reads_complete_and_incomplete_exports_and_names_each() {
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("report-cache");
    let _ = std::fs::remove_dir_all(&cache);
    let single = std::fs::read_to_string(fixture("single_part.musicxml")).expect("fixture");
    let pickup = std::fs::read_to_string(fixture("pickup.musicxml")).expect("fixture");
    export(&cache, "Scores", "stamped", &single, true);
    export(&cache, "Scores", "unstamped", &pickup, false);
    export(
        &cache,
        "Scores",
        "truncated",
        &single[..single.len() / 2],
        false,
    );

    let out = Command::new(env!("CARGO_BIN_EXE_corpus-report"))
        .arg("--cache")
        .arg(&cache)
        .output()
        .expect("runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");

    let block = |name: &str| -> String {
        let start = text
            .find(&format!("== Scores/{name}\n"))
            .unwrap_or_else(|| panic!("no block for {name}:\n{text}"));
        let rest = &text[start + 1..];
        rest[..rest.find("\n==").unwrap_or(rest.len())].to_owned()
    };
    let stamped = block("stamped");
    assert!(!stamped.contains("incomplete export"), "{stamped}");
    assert!(stamped.contains("fidelity: holds"), "{stamped}");

    let unstamped = block("unstamped");
    assert!(unstamped.contains("an incomplete export"), "{unstamped}");
    assert!(
        unstamped.contains("CreateMeasure refused: MeasureMeterMismatch ×2"),
        "{unstamped}"
    );

    let truncated = block("truncated");
    assert!(
        truncated.contains("INCOMPLETE EXPORT, NOT READ"),
        "{truncated}"
    );
    assert!(std::fs::read_to_string(cache.join("report/report.txt"))
        .expect("the report is saved")
        .contains("== summary"));
}
