//! Pixel-level golden-image comparator and bless machinery for the score raster
//! this GUI displays (`spec/CONTRACT_EDITOR_T1A_GOLDENS.md`; plan
//! `spec/PLAN_EDITOR_APP.md` §Ruling C, granted 2026-07-23 as amended).
//!
//! **Comparison contract (Ruling C, amended):** a golden is compared as
//! *decoded* RGBA pixels — dimensions first, then raw bytes — never as encoded
//! PNG file bytes. Comparing encoded bytes would also lock the PNG encoder's own
//! behavior (compression level, filter choice, …) and churn on an encoder
//! change even when every pixel is identical; `reencoding_with_different_settings_still_passes`
//! below makes that guarantee executable, not just asserted.
//!
//! This module is declared `#[cfg(test)]`-only at its `mod goldens;` site in
//! `main.rs`, so none of it ships in the built binary. It carries two kinds of
//! test: the comparator's **own** unit tests (T1a W1), which never touch
//! `goldens/` — they point [`assert_golden_at`] at private temp locations, so
//! they hold even before any baseline exists — and the **four golden-state**
//! tests (T1a W2: fixture-as-opened, after a scripted insert, after undo, after
//! casting-off), which call [`assert_golden`] against the three committed
//! baselines under `goldens/` (G3 reuses G1's file rather than owning one; see
//! its test's doc comment).

use std::fs;
use std::path::{Path, PathBuf};

use resvg::tiny_skia::Pixmap;

/// An opaque highlight color painted over every differing pixel in a `diff.png`.
const DIFF_HIGHLIGHT: [u8; 4] = [255, 0, 0, 255];

/// The committed baseline path for a named golden: `{CARGO_MANIFEST_DIR}/goldens/{name}.png`.
fn baseline_path(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/goldens")).join(format!("{name}.png"))
}

/// The failure-artifact directory for a named golden, resolved repo-root-relative
/// (`{CARGO_MANIFEST_DIR}/../../target/golden-failures/{name}/`, this crate being
/// two levels under the workspace root) so the path is the same regardless of the
/// working directory `cargo test` was invoked from — matching CI's
/// `target/golden-failures/**` upload path.
fn failure_dir_path(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/golden-failures"
    ))
    .join(name)
}

/// Compares `pixmap` against the committed baseline `goldens/{name}.png`,
/// writing any failure artifacts under `target/golden-failures/{name}/` (see
/// [`baseline_path`] / [`failure_dir_path`]).
///
/// **Bless policy:** setting `EPIPHANY_BLESS_GOLDENS=1` writes/overwrites the
/// baseline unconditionally instead of comparing, creating `goldens/` if
/// needed. This is a *reviewed decision* — the tranche's named user deep-dive
/// point on new baselines (plan §Ruling C) — **never** a mechanism for turning a
/// red test green. Do not set it to make a failing test pass; set it only after
/// visually inspecting the new PNG and deciding it is correct.
fn assert_golden(name: &str, pixmap: &Pixmap) {
    let baseline = baseline_path(name);
    if std::env::var("EPIPHANY_BLESS_GOLDENS").as_deref() == Ok("1") {
        write_pixmap(&baseline, pixmap);
        return;
    }
    assert_golden_at(&baseline, pixmap, &failure_dir_path(name));
}

/// The comparator's parameterized core: compares `pixmap` against the PNG
/// decoded from `baseline_path`, panicking with the mismatch description (which
/// names every artifact written) on failure. `assert_golden` is the thin
/// default-path wrapper above; tests call this directly with temp-directory
/// paths so they never depend on a committed baseline.
fn assert_golden_at(baseline_path: &Path, pixmap: &Pixmap, failure_dir: &Path) {
    if let Err(message) = compare(baseline_path, pixmap, failure_dir) {
        panic!("{message}");
    }
}

/// Result-returning core of the comparison, so tests can observe a mismatch
/// without needing to catch a panic. Reads and decodes the baseline PNG, then:
///
/// 1. compares dimensions — a mismatch fails here, before any pixel is looked
///    at, and writes `actual.png` + `expected.png` (no `diff.png`: a per-pixel
///    map is not meaningful across differing dimensions);
/// 2. compares decoded RGBA bytes exactly — a mismatch writes all three
///    artifacts (`actual.png`, `expected.png`, `diff.png`) and names them in the
///    returned message.
///
/// A missing or undecodable baseline file is a harness/setup error, not a
/// reviewable visual diff, so it panics directly rather than returning `Err`.
fn compare(baseline_path: &Path, actual: &Pixmap, failure_dir: &Path) -> Result<(), String> {
    let baseline_bytes = fs::read(baseline_path).unwrap_or_else(|err| {
        panic!(
            "no golden baseline at {} ({err}); run with EPIPHANY_BLESS_GOLDENS=1, after visually \
             reviewing the new image, to create it",
            baseline_path.display()
        )
    });
    let expected = Pixmap::decode_png(&baseline_bytes).unwrap_or_else(|err| {
        panic!(
            "golden baseline at {} is not a decodable PNG: {err}",
            baseline_path.display()
        )
    });

    if actual.width() != expected.width() || actual.height() != expected.height() {
        let actual_path = failure_dir.join("actual.png");
        let expected_path = failure_dir.join("expected.png");
        write_pixmap(&actual_path, actual);
        write_pixmap(&expected_path, &expected);
        return Err(format!(
            "golden mismatch: dimensions differ (actual {}x{} vs expected {}x{}); no pixel \
             comparison performed. Artifacts: {}, {}",
            actual.width(),
            actual.height(),
            expected.width(),
            expected.height(),
            actual_path.display(),
            expected_path.display(),
        ));
    }

    // The comparison contract itself (Ruling C, amended): decoded pixels, never
    // encoded file bytes. An encoder-settings change that leaves every pixel
    // identical must never fail this comparison.
    let pixels_equal = actual.data() == expected.data();
    if pixels_equal {
        return Ok(());
    }

    let (diff, differing) = diff_pixmap(actual, &expected);
    let actual_path = failure_dir.join("actual.png");
    let expected_path = failure_dir.join("expected.png");
    let diff_path = failure_dir.join("diff.png");
    write_pixmap(&actual_path, actual);
    write_pixmap(&expected_path, &expected);
    write_pixmap(&diff_path, &diff);
    Err(format!(
        "golden mismatch: {differing} of {} decoded pixels differ. Artifacts: {}, {}, {}",
        u64::from(actual.width()) * u64::from(actual.height()),
        actual_path.display(),
        expected_path.display(),
        diff_path.display(),
    ))
}

/// Builds a per-pixel highlight image: [`DIFF_HIGHLIGHT`] where `actual` and
/// `expected` disagree, `actual`'s own pixel where they agree (so the diff stays
/// legible against the surrounding score). Returns the image and the count of
/// differing pixels. Callers must have already established equal dimensions.
fn diff_pixmap(actual: &Pixmap, expected: &Pixmap) -> (Pixmap, usize) {
    let mut diff = Pixmap::new(actual.width(), actual.height())
        .expect("dimensions already validated equal and nonzero");
    let mut differing = 0usize;
    for ((out, a), b) in diff
        .data_mut()
        .chunks_exact_mut(4)
        .zip(actual.data().chunks_exact(4))
        .zip(expected.data().chunks_exact(4))
    {
        if a == b {
            out.copy_from_slice(a);
        } else {
            differing += 1;
            out.copy_from_slice(&DIFF_HIGHLIGHT);
        }
    }
    (diff, differing)
}

/// Writes `pixmap` to `path` as a PNG, creating parent directories as needed.
fn write_pixmap(path: &Path, pixmap: &Pixmap) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|err| panic!("creating {}: {err}", parent.display()));
    }
    let bytes = pixmap
        .encode_png()
        .unwrap_or_else(|err| panic!("encoding {}: {err}", path.display()));
    fs::write(path, bytes).unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use epiphany_core::{
        CmnNominal, MusicalDuration, MusicalPosition, RationalTime, TypedObjectId,
    };
    use epiphany_editor_core::{EditorSession, GridResolution};
    use epiphany_engrave::Engraver;
    use epiphany_layout_ir::{HitShape, Point};
    use epiphany_render_svg::{render, RenderOptions};
    use epiphany_testkit::fixtures;

    /// A private, per-call-unique scratch directory under the OS temp dir — W1
    /// has no committed baselines, so every test exercises the comparator
    /// against its own throwaway files rather than `goldens/`.
    fn unique_temp_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("epiphany-goldens-test-{tag}-{nanos}-{n}"))
    }

    /// A small pixmap with varied (non-uniform) content, alpha 255 throughout so
    /// premultiply/demultiply round-trips through PNG encode/decode exactly —
    /// deterministic pseudo-noise, not a flat fill, so PNG row filtering and
    /// compression have something to actually differ over.
    fn varied_pixmap(width: u32, height: u32) -> Pixmap {
        let mut pm = Pixmap::new(width, height).expect("nonzero test dimensions");
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let r = ((x.wrapping_mul(37)).wrapping_add(y.wrapping_mul(11)) % 256) as u8;
                let g = ((x.wrapping_mul(59)).wrapping_add(y.wrapping_mul(3)) % 256) as u8;
                let b = ((x.wrapping_mul(13)).wrapping_add(y.wrapping_mul(91)) % 256) as u8;
                pm.data_mut()[idx..idx + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        pm
    }

    #[test]
    fn identical_pixmaps_pass() {
        let pixmap = varied_pixmap(5, 4);
        let dir = unique_temp_dir("identical");
        let baseline = dir.join("baseline.png");
        write_pixmap(&baseline, &pixmap);
        let failure_dir = dir.join("failures");

        assert_golden_at(&baseline, &pixmap, &failure_dir);

        assert!(
            !failure_dir.exists(),
            "a passing comparison must not write failure artifacts"
        );
    }

    #[test]
    fn altered_pixel_fails_and_writes_all_three_artifacts() {
        let baseline_pixmap = varied_pixmap(5, 4);
        let mut actual_pixmap = baseline_pixmap.clone();
        // Alter exactly one pixel (the second one) in the actual image.
        actual_pixmap.data_mut()[4..8].copy_from_slice(&[9, 8, 7, 255]);

        let dir = unique_temp_dir("altered");
        let baseline = dir.join("baseline.png");
        write_pixmap(&baseline, &baseline_pixmap);
        let failure_dir = dir.join("failures");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_golden_at(&baseline, &actual_pixmap, &failure_dir);
        }));
        let payload = result.expect_err("a single-pixel diff must panic");
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .expect("panic payload is a string message");

        let actual_path = failure_dir.join("actual.png");
        let expected_path = failure_dir.join("expected.png");
        let diff_path = failure_dir.join("diff.png");
        assert!(actual_path.is_file(), "actual.png was written");
        assert!(expected_path.is_file(), "expected.png was written");
        assert!(diff_path.is_file(), "diff.png was written");
        assert!(
            message.contains(&actual_path.display().to_string()),
            "panic message names actual.png's path: {message}"
        );
        assert!(
            message.contains(&expected_path.display().to_string()),
            "panic message names expected.png's path: {message}"
        );
        assert!(
            message.contains(&diff_path.display().to_string()),
            "panic message names diff.png's path: {message}"
        );
    }

    #[test]
    fn reencoding_with_different_settings_still_passes() {
        // Encode the *same* pixels twice, with deliberately different PNG
        // encoder settings (filter type and compression level), via the `png`
        // crate directly — `tiny_skia::Pixmap::encode_png` exposes no such
        // knobs, so this is the only way to prove the comparator does not
        // secretly depend on encoder behavior. Pinned to the exact `png` version
        // already resolved via `tiny-skia` (Cargo.lock): a dev-only edge to an
        // existing node, not a new dependency.
        let pixmap = varied_pixmap(6, 5);

        let encode_with = |filter: png::FilterType, compression: png::Compression| -> Vec<u8> {
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, pixmap.width(), pixmap.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_filter(filter);
            encoder.set_compression(compression);
            let mut writer = encoder.write_header().expect("valid PNG header");
            writer
                .write_image_data(pixmap.data())
                .expect("valid image data");
            drop(writer);
            bytes
        };
        let bytes_a = encode_with(png::FilterType::NoFilter, png::Compression::Fast);
        let bytes_b = encode_with(png::FilterType::Paeth, png::Compression::Best);
        assert_ne!(
            bytes_a, bytes_b,
            "the two encodings must differ byte-wise, or this test is vacuous"
        );

        let dir = unique_temp_dir("reencode");
        let failure_dir = dir.join("failures");

        let baseline_a = dir.join("a.png");
        fs::create_dir_all(&dir).expect("temp dir created");
        fs::write(&baseline_a, &bytes_a).expect("baseline a written");
        assert_golden_at(&baseline_a, &pixmap, &failure_dir);

        let baseline_b = dir.join("b.png");
        fs::write(&baseline_b, &bytes_b).expect("baseline b written");
        assert_golden_at(&baseline_b, &pixmap, &failure_dir);

        assert!(
            !failure_dir.exists(),
            "both differently-encoded baselines must decode to the same pixels and pass"
        );
    }

    #[test]
    fn mismatched_dimensions_fail_before_any_pixel_comparison() {
        let baseline_pixmap = varied_pixmap(4, 3);
        let differently_sized = varied_pixmap(5, 3);

        let dir = unique_temp_dir("dims");
        let baseline = dir.join("baseline.png");
        write_pixmap(&baseline, &baseline_pixmap);
        let failure_dir = dir.join("failures");

        let err = compare(&baseline, &differently_sized, &failure_dir)
            .expect_err("differing dimensions must fail the comparison");

        assert!(
            err.contains("dimensions"),
            "failure mentions dimensions: {err}"
        );
        assert!(
            !err.contains("decoded pixels"),
            "failure must not describe a pixel diff: {err}"
        );
        assert!(
            !failure_dir.join("diff.png").exists(),
            "no pixel diff is computable across differing dimensions, so none is written"
        );
    }

    // ---- The four golden states (T1a W2) ------------------------------------

    /// Renders `session`'s resolved layout at `px_per_staff_space` (the demo's
    /// default is `12.0`; `RenderOptions`'s other fields default to
    /// `GlyphMode::PathOutline`, no fonts) and rasterizes it through
    /// `crate::rasterize_pixmap` — the exact pixmap `main.rs`'s
    /// `EditorApp::rerender` displays (`main.rs:247`). Returns the SVG string
    /// alongside the pixmap so callers can run the determinism double.
    fn render_pixmap(session: &EditorSession, px_per_staff_space: f32) -> (String, Pixmap) {
        let options = RenderOptions {
            px_per_staff_space,
            ..Default::default()
        };
        let output = render(session.resolved(), &options);
        let (pixmap, _logical) =
            crate::rasterize_pixmap(&output.svg).expect("a rendered score's SVG rasterizes");
        (output.svg, pixmap)
    }

    /// The click point G2 scripts (and G3 replays before undoing) — derived from
    /// `session`'s own rendered geometry, never a magic screen constant.
    ///
    /// At this geometry, ten measures of quarter notes at `px_per_staff_space:
    /// 12.0` don't fit one line, so `ten_measure_single_staff` itself casts off
    /// into two systems (not only the slurred G4 fixture). That means the
    /// score's *temporally* last note is not simply "the rightmost notehead
    /// box": the first system happens to render wider than the second, so its
    /// notes reach further right on the page even though they come first in
    /// time. The last note is instead the rightmost `Pitch`-sourced notehead
    /// **within the system with the lowest `bounding_box.origin.y`** — systems
    /// stack top-to-bottom in this y-up world, so the lowest one is the last.
    ///
    /// The click point sits half a staff space past that notehead's right edge
    /// (clearly past it, still read as the same system) and two staff spaces
    /// above its vertical center — four diatonic steps (a fifth) above the
    /// existing all-C4 content under treble clef, landing on a different,
    /// mid-staff pitch rather than repeating the fixture's own notes. `staff_pitch_at` /
    /// `default_grid_at` / `position_at` resolve this point to exact values,
    /// asserted in `g2_ten_measure_insert_matches_baseline`.
    fn scripted_insert_target(session: &EditorSession) -> Point {
        let last_system = session
            .resolved()
            .pages
            .iter()
            .flat_map(|page| &page.systems)
            .min_by(|a, b| {
                a.bounding_box
                    .origin
                    .y
                    .0
                    .total_cmp(&b.bounding_box.origin.y.0)
            })
            .expect("casting-off produced at least one system");
        let sys_bottom = last_system.bounding_box.origin.y.0;
        let sys_top = sys_bottom + last_system.bounding_box.size.height.0;

        let last_notehead = session
            .hit_test()
            .regions
            .iter()
            .filter_map(|r| match (&r.source, r.shape) {
                (TypedObjectId::Pitch(_), HitShape::Box(b)) => Some(b),
                _ => None,
            })
            .filter(|b| {
                let mid_y = (b.bottom.0 + b.top.0) / 2.0;
                (sys_bottom..=sys_top).contains(&mid_y)
            })
            .max_by(|a, b| a.right.0.total_cmp(&b.right.0))
            .expect("the last system renders at least one notehead");

        Point::new(
            last_notehead.right.0 + 0.5,
            (last_notehead.bottom.0 + last_notehead.top.0) / 2.0 + 2.0,
        )
    }

    /// **G1 — as opened.** `ten_measure_single_staff(0)`, exactly the demo's
    /// open path (`main.rs:197`), locked against `goldens/ten_measure_open.png`.
    /// This is also the file G3 (below) compares its post-undo raster against.
    #[test]
    fn g1_ten_measure_open_matches_baseline() {
        let score = fixtures::ten_measure_single_staff(0);
        let session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the ten-measure fixture renders under the real engraver");

        let (svg1, pixmap1) = render_pixmap(&session, 12.0);
        let (svg2, pixmap2) = render_pixmap(&session, 12.0);
        assert_eq!(svg1, svg2, "G1 determinism double: SVG bytes must match");
        assert_eq!(
            pixmap1.data(),
            pixmap2.data(),
            "G1 determinism double: rasterized pixels must match"
        );

        assert_golden("ten_measure_open", &pixmap1);
    }

    /// **G2 — after a scripted pencil insert.** See [`scripted_insert_target`]
    /// for the click-point derivation. `staff_pitch_at` / `default_grid_at` /
    /// `position_at` are asserted to their exact values *before* the insert
    /// runs, then the insert is applied and the result locked against
    /// `goldens/ten_measure_insert.png`.
    #[test]
    fn g2_ten_measure_insert_matches_baseline() {
        let score = fixtures::ten_measure_single_staff(0);
        let mut session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the ten-measure fixture renders under the real engraver");

        let target = scripted_insert_target(&session);

        let pitch = session
            .staff_pitch_at(target)
            .expect("the target point sits over the staff");
        assert_eq!(
            (pitch.nominal, pitch.octave),
            (CmnNominal::G, 4),
            "the target resolves to G4 — a fifth above the fixture's all-C4 content"
        );

        let grid = session
            .default_grid_at(target)
            .expect("the target point sits over a metric region");
        assert_eq!(
            grid,
            GridResolution {
                step: MusicalDuration(RationalTime::new(1, 4).expect("1/4 is valid"))
            },
            "the 4/4 meter's default grid is a quarter-note step"
        );

        let placed = session
            .position_at(target, &grid)
            .expect("the target snaps to a musical position");
        assert_eq!(
            placed.position,
            MusicalPosition(RationalTime::new(10, 1).expect("10/1 is valid")),
            "the insert lands exactly at whole-note 10 — immediately after the fixture's \
             last note ends (measure 10, beat 4 of 4), with no gap and nothing to overwrite"
        );

        let outcome = session
            .insert_note_at(target, &grid)
            .expect("the target is a clean, unoccupied insert slot");
        assert!(
            outcome.graph_changed,
            "the insert must change the score graph"
        );

        let (svg1, pixmap1) = render_pixmap(&session, 12.0);
        let (svg2, pixmap2) = render_pixmap(&session, 12.0);
        assert_eq!(svg1, svg2, "G2 determinism double: SVG bytes must match");
        assert_eq!(
            pixmap1.data(),
            pixmap2.data(),
            "G2 determinism double: rasterized pixels must match"
        );

        assert_golden("ten_measure_insert", &pixmap1);
    }

    /// **G3 — after undo of G2's insert. No third baseline:** this compares the
    /// post-undo raster against **G1's own baseline file**, reached by calling
    /// [`assert_golden_at`] directly rather than [`assert_golden`] — deliberately
    /// bypassing the bless path. Going through `assert_golden` would let
    /// `EPIPHANY_BLESS_GOLDENS=1` overwrite `ten_measure_open.png`: an initial
    /// bless run performed before undo is known to be correct would silently
    /// bless a broken undo's post-undo pixels as the new "as opened" baseline,
    /// after which every future run would compare undo against its own bug
    /// instead of against G1. Bypassing `assert_golden` makes that impossible —
    /// this comparison always compares, never blesses. A fresh session (rather
    /// than continuing G2's) keeps the two tests independent of each other's
    /// mutations.
    #[test]
    fn g3_undo_matches_g1_baseline() {
        let score = fixtures::ten_measure_single_staff(0);
        let mut session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the ten-measure fixture renders under the real engraver");

        let target = scripted_insert_target(&session);
        let grid = session
            .default_grid_at(target)
            .expect("the target point sits over a metric region");
        session
            .insert_note_at(target, &grid)
            .expect("the target is a clean, unoccupied insert slot");

        let outcome = session.undo().expect("there is one edit to undo");
        assert!(outcome.graph_changed, "undo must revert the inserted note");

        let (svg1, pixmap1) = render_pixmap(&session, 12.0);
        let (svg2, pixmap2) = render_pixmap(&session, 12.0);
        assert_eq!(svg1, svg2, "G3 determinism double: SVG bytes must match");
        assert_eq!(
            pixmap1.data(),
            pixmap2.data(),
            "G3 determinism double: rasterized pixels must match"
        );

        assert_golden_at(
            &baseline_path("ten_measure_open"),
            &pixmap1,
            &failure_dir_path("ten_measure_undo"),
        );
    }

    /// **G4 — casting-off.** `ten_measure_with_slurs(0)` (`fixtures.rs:777`):
    /// the same ten-measure content as G1–G3 plus three slurs, which at this
    /// geometry casts off into multiple systems **and** forces the second slur
    /// (events 5..8) across the system break — the cross-system slur-split path
    /// (`casting.rs:2262`). The system count is asserted first, as a named
    /// value: if casting-off ever stops triggering here, this fails as a
    /// reported system count, not a silent pixel diff easily misread as an
    /// unrelated rendering regression.
    #[test]
    fn g4_ten_measure_slurs_castoff_matches_baseline() {
        let score = fixtures::ten_measure_with_slurs(0);
        let session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the slurred ten-measure fixture renders under the real engraver");

        let system_count: usize = session
            .resolved()
            .pages
            .iter()
            .map(|page| page.systems.len())
            .sum();
        assert!(
            system_count > 1,
            "casting-off must produce more than one system, got {system_count}"
        );

        let (svg1, pixmap1) = render_pixmap(&session, 12.0);
        let (svg2, pixmap2) = render_pixmap(&session, 12.0);
        assert_eq!(svg1, svg2, "G4 determinism double: SVG bytes must match");
        assert_eq!(
            pixmap1.data(),
            pixmap2.data(),
            "G4 determinism double: rasterized pixels must match"
        );

        assert_golden("ten_measure_slurs_castoff", &pixmap1);
    }
}
