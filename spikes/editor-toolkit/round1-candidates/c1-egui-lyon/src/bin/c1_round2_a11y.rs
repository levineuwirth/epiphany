//! `ReportPart::FixtureAndReportPlumbing` — CLI parsing and fixture loading
//! for the check-5 windowed probe. The window/event-loop/adapter-lifecycle
//! code is `c1_egui_lyon::a11y_app` (`AccessibilityIntegrationWiring`); the
//! accessible node itself is `c1_egui_lyon::a11y_node`
//! (`AccessibilityTreeConstruction`). This file is deliberately thin: it
//! reads `--fixture`, loads that one fixture's resolved text from the
//! frozen `fixtures.json`, and hands off.
//!
//! Round 1's binary (and `c1_round2_text.rs`) are headless: offscreen wgpu
//! only, no window, nothing an AT-SPI client could ever see. Check 5
//! requires "a real window on the AT-SPI bus" (the contract's own words),
//! so this is a second, separate windowed mode — the same first-party
//! AccessKit route `probe-egui`'s Round 0 binary demonstrated a readback
//! for (see `round0-evidence/c1-egui-readback.txt`).
//!
//! **This binary carries no visual rendering.** An earlier revision of this
//! packet painted the fixture's glyphs here too, "for visual confirmation
//! only" — which is exactly the kind of non-semantic content the F3 finding
//! named as wrongly mixed into this binary's `ReportPart` attribution.
//! Scoring is not this binary's job either way: `a11y-verifier/verify.py`,
//! run out-of-process against the live AT-SPI2 bus by `c1_round2_text.rs`
//! (`c1_egui_lyon::a11y_subprocess`), is the actual readback and verdict.

use std::path::PathBuf;

fn spike_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn parse_fixture_arg() -> String {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--fixture" && i + 1 < args.len() {
            return args[i + 1].clone();
        }
        i += 1;
    }
    eprintln!("usage: c1_round2_a11y --fixture <F-A|F-B|F-C|F-D|F-E>");
    std::process::exit(2);
}

fn main() -> eframe::Result {
    let fixture_id = parse_fixture_arg();
    let root = spike_root();

    let fixtures_path = root.join("round2-textkit/fixtures.json");
    let fixtures = round2_textkit::output::load_fixtures(&fixtures_path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", fixtures_path.display()));
    let record = fixtures
        .fixtures
        .iter()
        .find(|f| f.id == fixture_id)
        .unwrap_or_else(|| panic!("no fixture {fixture_id:?} in fixtures.json"));
    let resolved = record.resolved.clone();

    c1_egui_lyon::a11y_app::run(fixture_id, resolved)
}
