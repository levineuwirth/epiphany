//! Drives the whole conformance suite at scale from the command line, so the
//! heavy gates can run outside the unit-test timeout — the analogue of
//! `epiphany-determinism`'s `fuzz_roundtrip` and `epiphany-bundle`'s
//! `fuzz_crash`. Every gate is deterministic, so any failure reproduces from
//! its seed.
//!
//! Usage:
//!   cargo run --release --example conformance_suite [SCALE]
//!
//! `SCALE` (default 1) multiplies the iteration counts. `SCALE=10` is a soak
//! run; `SCALE=0` runs a fast smoke pass. Exits non-zero (via panic) on the
//! first violation.

use epiphany_testkit::{
    bundle_harness, convergence, corpus, editloop, equivocation, fixtures, generators, layout_stub,
    negative, prepass_harness, roundtrip, textproj, Rng,
};

/// The suite's total gate count, printed in every `[N/TOTAL_GATES]` line and
/// the final summary. T2 W3 adds gate 9 (`golden-gate` feature); without the
/// feature this stays at 8, so every print in this file — the final `ok`
/// summary included — is byte-identical to before this tranche
/// (`spec/CONTRACT_EDITOR_T2_SELECTION.md` §W3: "Without the feature the
/// suite builds and prints exactly today's 8/8").
#[cfg(feature = "golden-gate")]
const TOTAL_GATES: u32 = 9;
#[cfg(not(feature = "golden-gate"))]
const TOTAL_GATES: u32 = 8;

fn main() {
    let scale: u64 = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("SCALE must be an integer"))
        .unwrap_or(1);
    let n = |base: u64| (base * scale).max(if base == 0 { 0 } else { 1 });

    // 0. Agent A's headline hand-off gate: 1,000,000 round-trip iterations.
    //    (Scaled; at SCALE=0 a smaller smoke count still runs.)
    let det_iters = if scale == 0 {
        50_000
    } else {
        1_000_000 * scale
    };
    eprintln!("[0/{TOTAL_GATES}] determinism round-trip gate: {det_iters} iters");
    roundtrip::run_determinism_roundtrip_gate(det_iters, 0x0A11_CE5E_EDED_2024);

    // 1. Canonical round-trip (criterion 4): type-level corpus.
    let iters = (50_000 * scale).max(2_000);
    eprintln!("[1/{TOTAL_GATES}] canonical round-trip corpus: {iters} iters");
    roundtrip::run_roundtrip_corpus(iters, 0x00C0_FFEE_1234_5678);

    // 1b. Bundle manifest + reducer-bookkeeping serialization + full-Score byte
    //     round-trip (item 5's whole-score codec, via reduce_onto).
    eprintln!("[1b ] manifest + bookkeeping + full-Score serialization stability");
    for seed in 0..n(64) {
        roundtrip::assert_manifest_roundtrip(&roundtrip::committed_manifest(seed));
        let mut rng = Rng::new(seed.wrapping_mul(0x0100_0193).wrapping_add(17));
        roundtrip::assert_manifest_roundtrip(&generators::rich_manifest(&mut rng));
        let session = generators::operation_envelopes(&mut rng, 40, 3, 6, 6);
        roundtrip::assert_reduction_serialization_stable(&session, seed);
        let (score, frontier) = convergence::materialized_score(seed.wrapping_add(101));
        roundtrip::assert_score_serialization_stable(&score, &frontier, seed);
        let envs = generators::operation_envelopes(&mut rng, 24, 3, 8, 8);
        roundtrip::assert_operation_block_summary_survives_storage(&envs, seed.wrapping_add(202));
    }
    roundtrip::assert_content_mutation_changes_serialization();

    // 2. Crash safety (criterion 2): the testkit driver + Agent D's gate.
    let crash_iters = (10_000 * scale).max(1_000);
    eprintln!("[2/{TOTAL_GATES}] crash recovery: {crash_iters} iters (testkit) + bundle gate");
    bundle_harness::run_crash_recovery(crash_iters, 0xF00D_BEEF_1234_5678);
    bundle_harness::bundle_crash_recovery_fuzz(crash_iters, 0x0123_4567_89AB_CDEF);

    // 3. Equivocation (criterion 3): testkit driver + Agent C's gate.
    eprintln!("[3/{TOTAL_GATES}] equivocation order-independence");
    for seed in 0..n(500) {
        equivocation::run_equivocation(16, seed.wrapping_mul(0x9E37_79B9).wrapping_add(1));
    }
    equivocation::ops_equivocation_fuzz((10_000 * scale).max(1_000), 0x1234_5678);

    // 4. Manifest selection.
    eprintln!("[4/{TOTAL_GATES}] manifest selection");
    for seed in 0..n(32) {
        bundle_harness::run_manifest_selection(seed);
    }

    // 5. Convergence (criterion 1): real-Score convergence through reduce_onto,
    //    plus the reducer-bookkeeping projection convergence.
    eprintln!("[5/{TOTAL_GATES}] convergence across delivery orders (real Score + bookkeeping)");
    for seed in 0..n(64) {
        convergence::run_graph_convergence(6, seed.wrapping_mul(0x9E37_79B9).wrapping_add(11));
    }
    for seed in 0..n(500) {
        convergence::run_convergence(24, 8, seed.wrapping_mul(0x9E37_79B9));
    }
    for seed in 0..n(32) {
        convergence::run_two_staff_convergence(8, seed.wrapping_mul(0x9E37_79B9).wrapping_add(7));
    }

    // 5b. Audit regression guards (every defect the Agent C audit surfaced).
    eprintln!("[5b ] audit defect regression guards");
    negative::run_all();

    // 6. Reduction determinism (criterion 5): a large set reduced many ways, the
    //    testkit's authoritative causal-order gate, + Agent C's own gate.
    let big = (1_000 * scale).max(1_000) as usize;
    eprintln!("[6/{TOTAL_GATES}] reduction determinism: {big}-envelope set, 10 orders");
    {
        let mut rng = Rng::new(0x5EED_0006_0F0F_0F0F);
        let envelopes = generators::operation_envelopes(&mut rng, big, 3, 40, 40);
        convergence::assert_causal_order_respected(&envelopes);
        convergence::assert_reduction_determinism(&envelopes, 10, &mut rng);
    }
    // Authoritative: causal-order correctness over many conformant sets.
    convergence::run_authoritative_reduction_gate(
        (10_000 * scale).max(1_000) as usize,
        3,
        0x0CA0_05A1,
    );
    // Supplementary: Agent C's own hand-off gate (permutation invariance).
    convergence::ops_reduction_determinism_fuzz((10_000 * scale).max(1_000), 0x00C0_FFEE);

    // 7. Layout round-trip (criterion 6).
    eprintln!("[7/{TOTAL_GATES}] layout round-trip");
    for seed in 0..n(128) {
        layout_stub::round_trip(&fixtures::ten_measure_single_staff(seed));
        layout_stub::round_trip(&generators::graph::valid_score_rich(seed));
    }

    // 7b. Track A — Agent H pre-pass merge gate (spelling + decomposition):
    //     the representative-corpus eligibility taxonomy (F3) and the H harness
    //     (F4) — determinism, spelling correctness, decomposition reconstruction,
    //     RespellPitch precedence, and the non-vacuity tripwire — at scale.
    eprintln!("[7b ] Agent H pre-pass gate: taxonomy coverage + merge gate");
    corpus::run_all();
    prepass_harness::run_all(scale.max(1));

    // 7c. Track A — the UI seam: the editing-loop vertical slice (hit-test → score
    //     object → operation → reduce → re-layout → re-render → re-resolve
    //     selection). Every fixture must drive a click→sharpen→re-render cycle whose
    //     selection survives the relayout — the contract a GUI's correctness rests
    //     on. (The harness `epiphany-editor-core` packages as a callable API.)
    eprintln!("[7c ] UI-seam gate: editing loop over the corpus");
    for seed in 0..n(48) {
        for score in [
            fixtures::ten_measure_single_staff(seed),
            generators::graph::valid_score_rich(seed),
        ] {
            let report = editloop::run_edit_loop(&score).unwrap_or_else(|| {
                panic!("seed {seed}: no clickable notehead to drive the editing loop")
            });
            assert!(
                report.graph_changed,
                "edit-loop seed {seed}: graph unchanged"
            );
            assert!(
                report.selection_preserved,
                "edit-loop seed {seed}: selection lost across relayout"
            );
            assert!(
                report.render_changed,
                "edit-loop seed {seed}: edit not visible"
            );
        }
    }

    eprintln!("[7d ] cross-implementation decode conformance vectors");
    {
        use epiphany_testkit::vectors;
        assert_eq!(
            vectors::COMMITTED,
            vectors::render(),
            "{} is stale; regenerate with `cargo run -q -p epiphany-testkit \
             --example generate_vectors`",
            vectors::PATH
        );
        match vectors::verify(vectors::COMMITTED) {
            Ok(n) => eprintln!("       {n} vectors, every verdict agreed"),
            Err(failures) => panic!(
                "{} decode-vector disagreement(s):\n{}",
                failures.len(),
                failures.join("\n")
            ),
        }
    }

    // 7e. Whole-document Text Projection vectors. The strong equation is over
    //     text bytes, never bundle identity; the generated-operation pass also
    //     checks the weaker semantic equation by reducing both sides.
    eprintln!("[7e ] Text Projection whole-document conformance vectors");
    for seed in 0..n(16) {
        textproj::assert_semantics_preserved(seed);
    }
    {
        use epiphany_textproj::vectors;
        assert_eq!(
            vectors::COMMITTED,
            vectors::render(),
            "{} is stale; regenerate with `cargo run -q -p epiphany-testkit \
             --example generate_vectors`",
            vectors::PATH
        );
        match vectors::verify(vectors::COMMITTED) {
            Ok(n) => eprintln!("       {n} vectors, every verdict agreed"),
            Err(failures) => panic!(
                "{} Text Projection vector disagreement(s):\n{}",
                failures.len(),
                failures.join("\n")
            ),
        }
    }

    // 8. T2 W3 — the [9/9] golden conformance gate (`golden-gate` feature
    //    only; absent without it, and the line above prints "8/8" unchanged).
    //    Re-derives the three T1a golden states headlessly and rasterizes
    //    them exactly as `epiphany-editor-gui`'s demo binary does, comparing
    //    decoded RGBA against the baselines committed there. See
    //    `spec/CONTRACT_EDITOR_T2_SELECTION.md` §W3 and this crate's
    //    `DECISIONS.md` (T2 W3).
    #[cfg(feature = "golden-gate")]
    {
        eprintln!("[8/{TOTAL_GATES}] golden conformance: T1a baselines rasterized headlessly");
        golden_gate::run();
    }

    eprintln!("[{TOTAL_GATES}/{TOTAL_GATES}] ok: full conformance suite passed (scale {scale})");
}

/// T2 W3: the `[9/9]` golden conformance gate. Promotes the T1a visual
/// goldens (`spec/CONTRACT_EDITOR_T1A_GOLDENS.md`,
/// `crates/epiphany-editor-gui/src/goldens.rs`) to a numbered gate in this
/// suite, entirely behind the `golden-gate` feature so the raster stack
/// (`resvg`, pulled in only via the optional dependencies in `Cargo.toml`)
/// never enters the MSRV build's dependency closure
/// (`spec/CONTRACT_EDITOR_T2_SELECTION.md` §W3).
///
/// **Compare-only.** Unlike `epiphany-editor-gui`'s own comparator (which can
/// bless and writes failure artifacts), this gate only ever compares: no
/// `EPIPHANY_BLESS_GOLDENS` path, no artifact writing. A mismatch panics with
/// a message pointing at `cargo test -p epiphany-editor-gui goldens` — the
/// crate that owns the baselines, the bless mechanism, and the reviewable
/// `actual.png`/`expected.png`/`diff.png` artifacts — as the diagnostic
/// surface.
///
/// **Cross-crate baseline coupling.** The three committed PNGs live in
/// `epiphany-editor-gui/goldens/`, not in this crate — this crate compares
/// against them without owning them, resolving the path from *this* crate's
/// `CARGO_MANIFEST_DIR` (the only directory `env!` can see at compile time)
/// up to the sibling crate directory. Documented here and in `DECISIONS.md`
/// (T2 W3) and `epiphany-editor-gui/DECISIONS.md` (T1a): a `git mv` of either
/// crate, or a rename of `epiphany-editor-gui/goldens/`, breaks this path
/// silently unless both DECISIONS files are updated together.
#[cfg(feature = "golden-gate")]
mod golden_gate {
    use std::fs;
    use std::path::PathBuf;

    use epiphany_core::TypedObjectId;
    use epiphany_editor_core::EditorSession;
    use epiphany_engrave::Engraver;
    use epiphany_layout_ir::{HitShape, Point};
    use epiphany_render_svg::{render, RenderOptions};
    use epiphany_testkit::fixtures;
    use resvg::tiny_skia::Pixmap;

    /// Runs the three T1a golden-state comparisons. Panics on the first
    /// mismatch or missing baseline (never a silent skip — the `fs::read`
    /// failure branch below panics directly, exactly like
    /// `epiphany-editor-gui/src/goldens.rs::compare`'s own missing-baseline
    /// path).
    pub fn run() {
        // G1 — as opened: `ten_measure_single_staff(0)`, exactly the demo's
        // open path (`epiphany-editor-gui/src/main.rs:210`,
        // `EditorApp::new`).
        let score = fixtures::ten_measure_single_staff(0);
        let session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the ten-measure fixture renders under the real engraver");
        assert_matches_baseline("ten_measure_open", &render_pixmap(&session));

        // G2 — after the scripted pencil insert. The click point is
        // re-derived through the session's own inverses, replicating
        // `goldens.rs::scripted_insert_target` faithfully: the last system
        // by lowest `bounding_box.origin.y`, the rightmost Pitch-sourced
        // notehead within that system's vertical band, offset +0.5 staff
        // spaces right / +2.0 staff spaces up from that notehead's
        // right-center.
        let score = fixtures::ten_measure_single_staff(0);
        let mut session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the ten-measure fixture renders under the real engraver");
        let target = scripted_insert_target(&session);
        let grid = session
            .default_grid_at(target)
            .expect("the target point sits over a metric region");
        let outcome = session
            .insert_note_at(target, &grid)
            .expect("the target is a clean, unoccupied insert slot");
        assert!(
            outcome.graph_changed,
            "gate 9: the scripted insert must change the score graph"
        );
        assert_matches_baseline("ten_measure_insert", &render_pixmap(&session));

        // G4 — casting-off: `ten_measure_with_slurs(0)`
        // (`fixtures.rs:777`), the multi-system layout and cross-system
        // slur-split path the T1a baseline locks.
        let score = fixtures::ten_measure_with_slurs(0);
        let session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the slurred ten-measure fixture renders under the real engraver");
        assert_matches_baseline("ten_measure_slurs_castoff", &render_pixmap(&session));
    }

    /// Renders `session`'s resolved layout at `px_per_staff_space: 12.0` (the
    /// demo's default; `RenderOptions`'s other fields stay at their default,
    /// `GlyphMode::PathOutline` — no fonts) and rasterizes it, matching
    /// `render_pixmap` in `epiphany-editor-gui/src/goldens.rs`.
    fn render_pixmap(session: &EditorSession) -> Pixmap {
        let options = RenderOptions {
            px_per_staff_space: 12.0,
            ..Default::default()
        };
        let output = render(session.resolved(), &options);
        rasterize(&output.svg)
    }

    /// Rasterizes a rendered SVG string to a `tiny_skia` pixmap, replicating
    /// `epiphany-editor-gui/src/main.rs::rasterize_pixmap` exactly: usvg
    /// parse, dimensions rounded up to whole pixels, an opaque white
    /// background (so premultiplied alpha stays fully opaque), then
    /// `resvg::render` over that background. This crate cannot call the
    /// original directly (it is private to a different crate's binary), so
    /// the algorithm is copied rather than shared — see the module doc for
    /// why that coupling is acceptable and documented rather than factored
    /// out.
    fn rasterize(svg: &str) -> Pixmap {
        let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
            .expect("a rendered score's SVG parses");
        let size = tree.size();
        let width = size.width().ceil().max(1.0) as u32;
        let height = size.height().ceil().max(1.0) as u32;
        let mut pixmap = Pixmap::new(width, height).expect("nonzero rasterized dimensions");
        pixmap.fill(resvg::tiny_skia::Color::WHITE);
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        pixmap
    }

    /// The click point G2 scripts — derived from `session`'s own rendered
    /// geometry, never a magic screen constant. Faithfully replicates
    /// `epiphany-editor-gui/src/goldens.rs::scripted_insert_target`; see that
    /// function's doc comment for the full derivation rationale (why the
    /// *last* system is the one with the lowest `bounding_box.origin.y`, and
    /// why the target is the rightmost Pitch-sourced notehead within that
    /// system's vertical band rather than the page-wide rightmost notehead).
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

    /// Compares `actual` against the committed baseline
    /// `epiphany-editor-gui/goldens/{name}.png`: decoded RGBA, dimensions
    /// first (the Ruling-C contract) — never encoded PNG bytes. Compare-only:
    /// no bless, no artifact writing (see the module doc). A missing or
    /// undecodable baseline panics directly, the same "loud failure, never a
    /// silent skip" contract as `goldens.rs::compare`.
    fn assert_matches_baseline(name: &str, actual: &Pixmap) {
        let path = baseline_path(name);
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "gate 9: no golden baseline at {} ({err}) — `epiphany-editor-gui/goldens/` must \
                 contain the committed T1a baselines for this gate to compare against; \
                 diagnose with `cargo test -p epiphany-editor-gui goldens`",
                path.display()
            )
        });
        let expected = Pixmap::decode_png(&bytes).unwrap_or_else(|err| {
            panic!(
                "gate 9: baseline at {} is not a decodable PNG: {err}",
                path.display()
            )
        });
        assert_eq!(
            (actual.width(), actual.height()),
            (expected.width(), expected.height()),
            "gate 9: {name} dimensions differ from the committed baseline at {} (actual {}x{} \
             vs expected {}x{}); diagnose with `cargo test -p epiphany-editor-gui goldens`",
            path.display(),
            actual.width(),
            actual.height(),
            expected.width(),
            expected.height(),
        );
        // A plain `assert_eq!` here would dump both full byte slices (millions
        // of bytes for a real score raster) into the panic message; a
        // differing-byte count is the useful signal, and `cargo test -p
        // epiphany-editor-gui goldens` is where the actual/expected/diff PNG
        // artifacts to inspect visually already live.
        if actual.data() != expected.data() {
            let differing = actual
                .data()
                .iter()
                .zip(expected.data())
                .filter(|(a, b)| a != b)
                .count();
            panic!(
                "gate 9: {name} decoded RGBA differs from the committed baseline at {} \
                 ({differing} of {} bytes differ); diagnose with `cargo test -p \
                 epiphany-editor-gui goldens`",
                path.display(),
                actual.data().len(),
            );
        }
    }

    /// The committed baseline path for a named golden:
    /// `{CARGO_MANIFEST_DIR}/../epiphany-editor-gui/goldens/{name}.png` — this
    /// crate's `CARGO_MANIFEST_DIR` is `crates/epiphany-testkit`, and the
    /// baselines live in the sibling crate `crates/epiphany-editor-gui`. See
    /// the module doc's "Cross-crate baseline coupling" note.
    fn baseline_path(name: &str) -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../epiphany-editor-gui/goldens"
        ))
        .join(format!("{name}.png"))
    }
}
