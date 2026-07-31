//! `generate_a11y_expectations` — Packet 2B-A's entry point.
//!
//! Loads `round2-textkit/fixtures.json` (validating it against its own
//! embedded digest via `round2_textkit::output::load_fixtures`), derives this
//! machine's check-5 comparison data for all five fixtures, and writes
//! `round2-a11y-oracle/a11y_expectations.json`.
//!
//! Exit behavior mirrors `round2-textkit`'s own `bin/generate`: a missing
//! `fixtures.json` (never generated, or generated on a machine without the
//! declared faces) is reported and this binary exits non-zero rather than
//! writing a partial or empty file — pin 13's ordering requires the oracle to
//! exist and be reviewed before a candidate consumes it, so silently writing
//! nothing would be worse than a loud failure.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let textkit_dir = manifest_dir
        .parent()
        .expect("round2-a11y-oracle has a parent directory")
        .join("round2-textkit");
    let fixtures_path = textkit_dir.join("fixtures.json");

    let fixtures = round2_textkit::output::load_fixtures(&fixtures_path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} — run `cargo run -p round2-textkit --bin generate` first",
            fixtures_path.display()
        )
    });

    let expectations = round2_a11y_oracle::build_expectations_file(&fixtures);

    let out_path = manifest_dir.join("a11y_expectations.json");
    let json = serde_json::to_string_pretty(&expectations)
        .expect("ExpectationsFile is always serializable");
    std::fs::write(&out_path, format!("{json}\n"))
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));

    println!(
        "wrote {} ({} fixtures, platform {})",
        out_path.display(),
        expectations.fixtures.len(),
        expectations.platform
    );
    for f in &expectations.fixtures {
        println!(
            "  {}: {} alternative form(s), visual_order_name {}",
            f.fixture_id,
            f.alternative_forms.len(),
            if f.visual_order_name.is_some() {
                "present"
            } else {
                "omitted (identical to expected_name)"
            }
        );
    }
}
