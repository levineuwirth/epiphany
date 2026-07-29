//! `generate` — Packet 2A-i's entry point: resolves the declared faces,
//! shapes the five committed fixtures, asserts every W3 §5 invariant and
//! every recipe §4 precommitted expectation, then writes
//! `round2-textkit/fixtures.json` and `round2-textkit/FIXTURES_SUMMARY.md`.
//!
//! Exit behavior:
//! * A declared face **missing from disk** is an environment absence (pin
//!   14): prints `NOT RUN: <path>` for each missing face and exits `0`
//!   without writing anything.
//! * A declared face **present but hash-mismatched**, or any invariant/
//!   recipe-expectation disagreement, is a hard failure: `faces::resolve_one`
//!   or `fixtures::build_fixture` panics, and this binary exits non-zero.

use round2_textkit::faces::{self, FaceResolution, LoadedFace};
use round2_textkit::fixtures::{self, FIXTURES};
use round2_textkit::output;

fn main() {
    let resolved = faces::resolve_declared_chain();
    let missing: Vec<&std::path::PathBuf> = resolved
        .iter()
        .filter_map(|r| match r {
            FaceResolution::Missing { path } => Some(path),
            FaceResolution::Loaded(_) => None,
        })
        .collect();
    if !missing.is_empty() {
        for path in &missing {
            println!("NOT RUN: {}", path.display());
        }
        println!(
            "{} of {} declared faces are absent from this machine (pin 14: environment absence, \
             not a failure). No fixtures.json was written.",
            missing.len(),
            resolved.len()
        );
        return;
    }

    let loaded: Vec<LoadedFace> = resolved
        .into_iter()
        .map(|r| match r {
            FaceResolution::Loaded(lf) => lf,
            FaceResolution::Missing { .. } => unreachable!("handled above"),
        })
        .collect();

    println!("Resolved {} declared faces:", loaded.len());
    for (i, f) in loaded.iter().enumerate() {
        println!(
            "  [{i}] {} — family {:?}, version {:?}, {} bytes",
            f.path.display(),
            f.identity.family,
            f.identity.version,
            f.bytes.len()
        );
    }

    let mut built = Vec::with_capacity(FIXTURES.len());
    for (ordinal, def) in FIXTURES.iter().enumerate() {
        println!("Shaping {} ({}) ...", def.id, def.purpose);
        let rt = fixtures::build_fixture(def, &loaded, ordinal as u64);
        let total_glyphs: usize = rt.segments.iter().map(|s| s.glyphs.len()).sum();
        println!(
            "  {} codepoints, {} bytes, {} segments, {} glyphs, {} clusters — OK",
            rt.text.chars().count(),
            rt.text.len(),
            rt.segments.len(),
            total_glyphs,
            rt.clusters.clusters.len()
        );
        built.push((def.id.to_string(), def.purpose.to_string(), rt));
    }

    let file = output::build_fixture_file(&loaded, built)
        .expect("every fixture must have a precommitted accessibility note");

    // Printed BEFORE validation, and unconditionally: when the digest is what
    // disagrees, the number needed to fix it must be on screen even though
    // `validate()` is about to fail. Re-record it only after establishing why
    // it changed — see `EXPECTED_ARTIFACT_DIGEST_HEX`'s doc comment.
    let digest = output::artifact_digest(&file);
    println!("artifact digest (sha256 of canonical JSON): {digest}");
    if digest != output::expected_artifact_digest() {
        println!(
            "  compiled-in expectation:                    {}",
            output::expected_artifact_digest()
        );
        println!("  ^ these differ — validation below will fail until the constant is re-recorded");
    }

    file.validate()
        .expect("freshly generated fixtures.json must pass its own validator");

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let json_path = manifest_dir.join("fixtures.json");
    let summary_path = manifest_dir.join("FIXTURES_SUMMARY.md");

    let json = serde_json::to_string_pretty(&file).expect("fixtures.json must serialize");
    std::fs::write(&json_path, json).expect("write fixtures.json");
    println!("wrote {}", json_path.display());

    let summary = output::render_summary_markdown(&file);
    std::fs::write(&summary_path, summary).expect("write FIXTURES_SUMMARY.md");
    println!("wrote {}", summary_path.display());
}
