//! The Reference Suite companion's v0.1 entry set, asserted against the real
//! engraver: one test per entry (so a failure names its entry), the entry-set
//! shape, the declared solve configuration, and the printed metric table
//! (visible with `--nocapture`).
//!
//! The harness machinery lives in `epiphany_testkit::reference_suite`
//! (library-module-per-harness, DECISIONS F0); this file binds it to
//! `epiphany_engrave::Engraver` — the crate's dev-only dependency — under the
//! companion's declared default A4 geometry and default solver configuration.

use epiphany_engrave::Engraver;
use epiphany_testkit::reference_suite::{entries, evaluate_minimal, rs2_cited_builder, table};

/// The suite's solver under test: the reference engraver at its documented
/// default geometry — which `default_geometry_is_the_declared_a4_configuration`
/// pins to the companion's declared numbers.
fn solver() -> Engraver {
    Engraver::default()
}

fn run(id: &str) {
    let entry_set = entries();
    let entry = entry_set
        .iter()
        .find(|entry| entry.id == id)
        .expect("entry id");
    let outcome = evaluate_minimal(&solver(), entry);
    print!("{}", table(std::slice::from_ref(&outcome)));
}

#[test]
fn the_v01_entry_set_is_the_companions_six() {
    // Reference Suite companion, Table "entries": exactly RS-1..RS-6, all
    // required at Minimal (the harness evaluates every one; none is optional).
    let ids: Vec<&str> = entries().iter().map(|entry| entry.id).collect();
    assert_eq!(ids, ["RS-1", "RS-2", "RS-3", "RS-4", "RS-5", "RS-6"]);
}

#[test]
fn default_geometry_is_the_declared_a4_configuration() {
    // The companion's solve-configuration requirement declares every v0.1
    // entry solved at A4 portrait / 8 mm staff: page 105 x 148.5 staff spaces,
    // 7.5-staff-space margins, hence a 90 x 133.5 content area. The reference
    // engraver's default *is* that geometry; the suite runs on it.
    let geometry = solver().geometry();
    assert_eq!(geometry.size.width.0, 105.0);
    assert_eq!(geometry.size.height.0, 148.5);
    for margin in [
        geometry.margins.top,
        geometry.margins.right,
        geometry.margins.bottom,
        geometry.margins.left,
    ] {
        assert_eq!(margin.0, 7.5);
    }
    assert_eq!(geometry.content_width(), 90.0);
    assert_eq!(geometry.content_height(), 133.5);
}

#[test]
fn rs2_construction_reproduces_the_cited_builder() {
    // The companion cites `generators::valid_score_rich(0xF302)` and says the
    // corpus entry `gen_valid_score_rich` pins the same seed: the two must
    // reproduce the same score graph bit-for-bit (builder-and-seed
    // referencing, `req:refsuite:referencing`).
    let via_corpus = (entries()[1].build)();
    assert_eq!(
        via_corpus.canonical_bytes(),
        rs2_cited_builder().canonical_bytes()
    );
}

#[test]
fn rs1_ten_measure_single_staff_passes_minimal() {
    run("RS-1");
}

#[test]
fn rs2_rich_multi_region_score_passes_minimal() {
    run("RS-2");
}

#[test]
fn rs3_b_flat_major_scale_passes_minimal() {
    run("RS-3");
}

#[test]
fn rs4_two_voice_counterpoint_passes_minimal() {
    run("RS-4");
}

#[test]
fn rs5_notes_and_rests_passes_minimal() {
    run("RS-5");
}

#[test]
fn rs6_meter_three_four_passes_minimal() {
    run("RS-6");
}

#[test]
fn minimal_suite_metric_table() {
    // The whole suite in one aligned table (run with --nocapture): the
    // measured per-entry metric values behind the per-entry passes above.
    let solver = solver();
    let outcomes: Vec<_> = entries()
        .iter()
        .map(|entry| evaluate_minimal(&solver, entry))
        .collect();
    print!("{}", table(&outcomes));
}
