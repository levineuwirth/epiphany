//! Demo harness (NOT an application): render a fixture score to SVG on stdout.
//!
//! This is the "show the work" tool from the QUICKSTART (Agent I, "Demo binary
//! discipline"): it drives the full v0 pipeline
//! `Score → to_logical → to_constrained → solver → render` for a named fixture
//! and writes the SVG to stdout (stats + diagnostics to stderr). It is invaluable
//! for visual regression review and triage; it is not a viewer or a GUI host.
//!
//! Usage:
//!
//! ```text
//! cargo run -p epiphany-render-svg --example render_fixture -- \
//!     ten_measure_single_staff [--solver=stub|real] [--seed=N] \
//!     [--glyph-mode=path|embedded] [--no-provenance] > out.svg
//! ```
//!
//! The `--solver` flag selects the interface-only stub (`stub`, the default this
//! phase) or Agent I's engrave solver (`real`); keeping the renderer working
//! against both is how a renderer-vs-solver bug is bisected with a one-flag
//! change (QUICKSTART, Agent I, "Development pattern"). The `--glyph-mode` flag
//! selects inline outline `<path>`s (`path`, default) or the embedded-font
//! `<text>` mode (`embedded`).

use std::process::ExitCode;

use epiphany_engrave::Engraver;
use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig, StubSolver};
use epiphany_render_svg::{render, GlyphMode, RenderOptions};

const FIXTURES: &str = "ten_measure_single_staff, valid_score_rich, valid_score";
const SOLVERS: &str = "stub, real";

fn main() -> ExitCode {
    let mut fixture: Option<String> = None;
    let mut solver = String::from("stub");
    let mut seed: u64 = 0x000A_11CE;
    let mut emit_provenance = true;
    let mut glyph_mode = GlyphMode::PathOutline;

    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--solver=") {
            solver = v.to_owned();
        } else if let Some(v) = arg.strip_prefix("--seed=") {
            match v.parse() {
                Ok(n) => seed = n,
                Err(_) => return fail(&format!("invalid --seed value: {v}")),
            }
        } else if let Some(v) = arg.strip_prefix("--glyph-mode=") {
            glyph_mode = match v {
                "path" => GlyphMode::PathOutline,
                "embedded" => GlyphMode::EmbeddedFont,
                other => {
                    return fail(&format!(
                        "unknown --glyph-mode {other:?}; known: path, embedded"
                    ))
                }
            };
        } else if arg == "--no-provenance" {
            emit_provenance = false;
        } else if arg == "--help" || arg == "-h" {
            usage();
            return ExitCode::SUCCESS;
        } else if arg.starts_with("--") {
            return fail(&format!("unknown flag: {arg}"));
        } else if fixture.is_none() {
            fixture = Some(arg);
        } else {
            return fail(&format!("unexpected argument: {arg}"));
        }
    }

    let Some(fixture) = fixture else {
        usage();
        return fail("a fixture name is required");
    };

    let score = match fixture.as_str() {
        "ten_measure_single_staff" => epiphany_testkit::fixtures::ten_measure_single_staff(seed),
        "valid_score_rich" => epiphany_core::generators::valid_score_rich(seed),
        "valid_score" => epiphany_core::generators::valid_score(seed),
        other => return fail(&format!("unknown fixture {other:?}; known: {FIXTURES}")),
    };

    let constrained = to_constrained(&to_logical(&score));
    let config = SolverConfig::default();
    let (report, tier) = match solver.as_str() {
        "stub" => (StubSolver.solve(&constrained, &config), StubSolver.tier()),
        "real" => (
            Engraver::default().solve(&constrained, &config),
            Engraver::default().tier(),
        ),
        other => return fail(&format!("unknown solver {other:?}; known: {SOLVERS}")),
    };

    let out = render(
        &report.layout,
        &RenderOptions {
            emit_provenance,
            glyph_mode,
            ..RenderOptions::default()
        },
    );

    eprintln!(
        "fixture={fixture} seed={seed:#x} solver={solver} tier={tier:?} status={:?}",
        report.status
    );
    eprintln!(
        "glyphs={} paths={} texts={} fallback_rects={} provenance={} layers={} hard_constraints={} well_formed={}",
        out.stats.glyph_count,
        out.stats.path_count,
        out.stats.text_count,
        out.stats.fallback_rect_count,
        out.stats.provenance_count,
        out.stats.layer_count,
        constrained.constraints.len(),
        out.is_well_formed(),
    );
    for d in &out.diagnostics {
        eprintln!("diagnostic: {} (glyph: {:?})", d.message, d.glyph);
    }

    print!("{}", out.svg);
    ExitCode::SUCCESS
}

fn usage() {
    eprintln!(
        "usage: render_fixture <fixture> [--solver=stub|real] [--seed=N] \
         [--glyph-mode=path|embedded] [--no-provenance]\n\
         fixtures: {FIXTURES}\n\
         solvers:  {SOLVERS} (default: stub)\n\
         glyph-mode: path, embedded (default: path)"
    );
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(2)
}
