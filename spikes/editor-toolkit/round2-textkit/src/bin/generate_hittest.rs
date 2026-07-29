//! `generate_hittest` — Packet 2A-iii, Deliverable 1: builds the recipe §7
//! closing-paragraph hit-test *probe table* from the already-committed
//! `fixtures.json` and writes it to a sibling `hittest_probes.json`.
//!
//! **Why a sibling file, not extra fields on `fixtures.json`.**
//! `fixtures.json` is the candidate-neutral §3E mirror — the shaped-text
//! shape itself, precommitted before any candidate. The probe table is
//! *derived, candidate-testing apparatus* built on top of that data (recipe
//! §7's own wording: the probes are how a later packet checks a candidate's
//! hit-test function, not part of what a candidate is asked to reproduce).
//! Keeping them separate means `fixtures.json` stays exactly what
//! `ROUND2_TEXT_RECIPE.md` §5's invariants describe and nothing else, and a
//! probe-table regeneration never touches the file whose hash/structure
//! other tooling already depends on.
//!
//! **Why this reads `fixtures.json` rather than re-shaping from the font
//! files.** The probe table's whole job is to be checkable against the
//! *committed* fixture data — reading it back with `output::load_fixtures`
//! (which validates structurally and semantically before this ever sees it)
//! ties the probes directly to the file every consumer actually reads, and
//! means this binary needs no font files at all, only `fixtures.json`.

use round2_textkit::hittest;
use round2_textkit::output;

fn main() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures_path = manifest_dir.join("fixtures.json");

    let fixtures = output::load_fixtures(&fixtures_path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} — run `cargo run -p round2-textkit --bin generate` first",
            fixtures_path.display()
        )
    });

    let file = hittest::build_hittest_probe_file(&fixtures);
    file.validate(&fixtures)
        .expect("freshly generated hittest_probes.json must pass its own validator");

    println!("Hit-test probe table (recipe §7):");
    let mut total_probes = 0usize;
    for ft in &file.fixtures {
        println!("  {}: {} probes", ft.fixture_id, ft.probes.len());
        total_probes += ft.probes.len();
    }
    println!(
        "  total: {total_probes} probes across {} fixtures",
        file.fixtures.len()
    );

    if file.dropped.is_empty() {
        println!(
            "  0 probes dropped for the {} px separation floor",
            hittest::MIN_STOP_SEPARATION_DEVICE_PX
        );
    } else {
        println!(
            "  {} probe(s) DROPPED for the {} px separation floor:",
            file.dropped.len(),
            hittest::MIN_STOP_SEPARATION_DEVICE_PX
        );
        for d in &file.dropped {
            println!("    [{}] {}", d.fixture_id, d.reason);
        }
    }

    let out_path = manifest_dir.join("hittest_probes.json");
    let json = serde_json::to_string_pretty(&file).expect("serialize hittest_probes.json");
    std::fs::write(&out_path, json).expect("write hittest_probes.json");
    println!("wrote {}", out_path.display());
}
