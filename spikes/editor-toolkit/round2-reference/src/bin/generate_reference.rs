//! `generate_reference` — Packet 2A-iii, Deliverable 2.
//!
//! For each of the five committed fixtures: builds `DrawGlyph`s from its
//! `SpikeResolvedText`, emits an explicit-glyph SVG (composing per-face
//! output through `round2_svgref::emit_glyph_paths` + `wrap_document`, which
//! is what F-B and F-D need since each mixes two faces in one document),
//! asserts no `<text>`, rasterizes
//! at 1920x1080, derives `GlyphRegion`s from the emitter's own returned
//! bounds, runs the differential against itself as a self-check, and prints
//! the required per-fixture numbers.

use std::path::PathBuf;

use round2_textkit::faces::{self, FaceResolution, LoadedFace};
use round2_textkit::output;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let textkit_dir = manifest_dir
        .parent()
        .expect("round2-reference has a parent directory")
        .join("round2-textkit");
    let fixtures_path = textkit_dir.join("fixtures.json");

    let fixtures = output::load_fixtures(&fixtures_path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} — run `cargo run -p round2-textkit --bin generate` first",
            fixtures_path.display()
        )
    });

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
            "{} of {} declared faces are absent (pin 14: environment absence, not a failure). \
             No reference rasters were generated.",
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

    let out_dir = manifest_dir.join("output");
    std::fs::create_dir_all(&out_dir).expect("create output dir");

    const WIDTH: u32 = round2_reference::WIDTH;
    const HEIGHT: u32 = round2_reference::HEIGHT;
    const MIN_INK_PIXELS: usize = 1000;

    let mut any_selfcheck_failed = false;
    let mut any_ink_too_low = false;

    println!(
        "=== Packet 2A-iii Deliverable 2: reference rasters + D4 regions ({WIDTH}x{HEIGHT}) ===\n"
    );

    for f in &fixtures.fixtures {
        let result = round2_reference::build_fixture_raster(
            &f.id,
            &f.resolved,
            &loaded,
            WIDTH,
            HEIGHT,
            round2_reference::FacePolicy::Enforce,
        )
        .unwrap_or_else(|e| panic!("{}: {e}", f.id));

        round2_diff::validate_rgba(&result.rgba, WIDTH, HEIGHT)
            .unwrap_or_else(|e| panic!("{}: rasterized buffer is malformed: {e}", f.id));

        let ink_pixels = round2_reference::count_ink_pixels(&result.rgba);

        // Self-check (task step 5): the reference must pass the differential
        // against itself. If it does not, the reference or the differential
        // is broken and nothing downstream is trustworthy.
        let selfdiff =
            round2_diff::diff(&result.rgba, &result.rgba, WIDTH, HEIGHT, &result.regions)
                .unwrap_or_else(|e| panic!("{}: self-diff call failed: {e}", f.id));
        let selfcheck_ok = selfdiff.pass();
        if !selfcheck_ok {
            any_selfcheck_failed = true;
        }

        println!("--- {} ({}) ---", f.id, f.purpose);
        println!(
            "  stored glyphs (SpikeResolvedText): {}",
            result.stored_glyph_count
        );
        println!(
            "  glyphs drawn (path emitted):       {}",
            result.drawn_glyph_count
        );
        println!(
            "  glyphs empty (outline-less, e.g. space): {}",
            result.empty_glyph_count
        );
        println!(
            "  unresolved segments (no face, e.g. F-C's Arabic letter): {}",
            result.unresolved_segment_count
        );
        println!(
            "  regions (D4):                      {}",
            result.regions.len()
        );
        println!("  ink pixels:                        {ink_pixels}");
        println!(
            "  self-diff (reference vs itself): d1={} d2={:.6}% d3={:?} d4_pass={} overall_pass={}",
            selfdiff.d1_pixels_outside_band_differing,
            selfdiff.d2_relative_delta * 100.0,
            selfdiff.d3_delta,
            selfdiff.d4_pass,
            selfcheck_ok
        );
        if !selfcheck_ok {
            println!("  FINDING (LOUD): reference does NOT pass the differential against itself.");
        }
        if ink_pixels < MIN_INK_PIXELS {
            any_ink_too_low = true;
            println!(
                "  FINDING (LOUD): only {ink_pixels} ink pixels, below the {MIN_INK_PIXELS} \
                 sanity floor — a reference this blank would make every later comparison \
                 meaningless while looking green (Round 1's unregistered-texture failure)."
            );
        }

        let stem = out_dir.join(&f.id);
        let svg_path = stem.with_extension("svg");
        std::fs::write(&svg_path, &result.svg).expect("write svg");
        let rgba_path = stem.with_extension("rgba");
        std::fs::write(&rgba_path, &result.rgba).expect("write raw rgba");
        let regions_path = stem.with_extension("regions.json");
        let region_records: Vec<round2_reference::RegionRecord> = result
            .regions
            .iter()
            .map(round2_reference::RegionRecord::from)
            .collect();
        std::fs::write(
            &regions_path,
            serde_json::to_string_pretty(&region_records).unwrap(),
        )
        .expect("write regions.json");
        println!(
            "  wrote {} ({} bytes), {} ({}x{}x4 = {} bytes), {}",
            svg_path.display(),
            result.svg.len(),
            rgba_path.display(),
            WIDTH,
            HEIGHT,
            result.rgba.len(),
            regions_path.display()
        );
        println!();
    }

    println!("=== summary ===");
    if any_selfcheck_failed {
        println!(
            "FINDING: at least one fixture's reference did NOT pass the differential against \
             itself — the reference or the differential is broken; nothing above is trustworthy \
             until this is fixed."
        );
    } else {
        println!("every fixture's reference passes the differential against itself.");
    }
    if any_ink_too_low {
        println!(
            "FINDING: at least one fixture's raster carries suspiciously little ink (below \
             {MIN_INK_PIXELS} px)."
        );
    } else {
        println!("every fixture's raster carries non-trivial ink (>= {MIN_INK_PIXELS} px).");
    }

    // Same defect `round2-diff`'s selftest had: this binary used to print its
    // FINDINGs and exit 0, so a blank or self-inconsistent reference would
    // have been generated, reported, and recorded as a successful run.
    if any_selfcheck_failed || any_ink_too_low {
        eprintln!("\ngenerate_reference FAILED: see the FINDINGs above");
        std::process::exit(1);
    }
}
