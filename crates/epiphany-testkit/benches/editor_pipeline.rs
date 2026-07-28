//! The staged interactive-edit latency bench + budget gate (Ruling A
//! criterion 2; `spec/PLAN_EDITOR_APP.md` Fact 8).
//!
//! The normative budget (`spec/core_spec.tex`,
//! `req:perf:single-system-edit-latency`):
//!
//! > The *core's portion* of a single-system edit (operation envelope
//! > construction, reduction, incremental layout through `ResolvedLayoutIR`)
//! > MUST complete within one frame (16.7 ms at 60 Hz) at the p99 percentile,
//! > measured on the reference hardware profile.
//!
//! ## Why this bench exists, and what it is allowed to conclude
//!
//! Ruling A's toolkit spike is bounded by six criteria, of which criterion 2 is
//! *staged* latency: "reduce / engrave / scene-build / paint measured
//! **separately** … a toolkit verdict from an end-to-end number is
//! uninformative while reduction or solving dominates." That sentence is a
//! sequencing claim nobody had measured. This bench measures it, so the
//! decision to run the spike — or to run T4b (incremental materialization)
//! first — rests on numbers instead of assumption.
//!
//! The stage split is not invented here; it is the real seam
//! `EditorSession::materialize` walks (`epiphany-editor-core/src/lib.rs`), read
//! off its private `render_score` and reproduced stage for stage:
//!
//! | stage | what runs | in the core's budget? |
//! |---|---|---|
//! | reduce | `OperationSet::accept` × log + `reduce_onto(&base)` | **yes** |
//! | engrave | `to_logical` → `to_constrained` → `Engraver::solve` | **yes** |
//! | scene-build | `to_render` + `hit_test_map` (+ SVG string) | no — product layer |
//! | paint | `usvg` parse + `resvg` rasterize | no — product layer |
//!
//! Only the first two are gated. `req:perf:single-system-edit-latency` bounds
//! "the core's portion" and says so explicitly — "End-to-end edit-to-pixel
//! latency (input handling, hit testing, render submission, display flip) is a
//! product-layer obligation" — so charging the SVG serializer and `resvg`
//! against a core budget would be a category error. They are measured and
//! printed because the ruling asks for the stages *separately*, and because
//! today's scene-build+paint is the path Ruling A demotes: the number is the
//! baseline a canvas must beat, not a budget to defend.
//!
//! ## What the scale points vary, and what they deliberately do not
//!
//! The variable is **log depth**: Fact 8's finding is that `apply` reduces the
//! *entire accumulated log* onto the pristine open-time base on every edit, so
//! the cost of one keystroke grows with the number of keystrokes before it.
//! The score stays a fixed 10-measure fixture, so the engrave column is held
//! still while the reduce column moves — which is what makes the two
//! attributable.
//!
//! **The score's *size* is held fixed; its *content* is not, and one column
//! reads that.** The edits are transpositions, so the reduced score differs
//! from the base by up to ±1 semitone per pitch, and a transposed pitch may
//! acquire an accidental. Because the edit log cycles the pitch list and
//! alternates direction per pass, the accidental count depends on the *parity
//! of the pass count* at that depth: depth 1,000 is 25 passes (odd — every
//! pitch sits one semitone off the base, most carrying an accidental), while
//! depth 10,000 is 250 passes (even — every pitch is back where it started).
//! That is why `paint` is **non-monotonic** in depth below (2.78 ms at 1,000,
//! 1.32 ms at 10,000): the deeper score simply has less ink. `reduce` is the
//! only column that tracks depth; `engrave`, `scene-build`, and `paint` track
//! score content. Reading paint's dip as a scaling win would be a mistake.
//!
//! **The honest limitation:** no orchestral-scale score fixture exists in the
//! testkit (the largest are three staves × ten measures), so the engrave and
//! scene-build columns are measured at *small score size* and are lower
//! bounds. The spec's budget contemplates "a 100-page orchestral score". This
//! bench therefore cannot prove the budget holds at scale; it can only show
//! where the time goes at the scale we can build, and any row that already
//! misses at this size misses by more at a realistic one. A score-size axis
//! belongs with the engraving-quality track that needs large fixtures anyway.
//!
//! The edits are ±1-semitone `TransposeInterval`s cycled across the fixture's
//! pitches, alternating direction **per pass** (see [`edit_envelope`] for why
//! per-pass and not per-operation): distinct operation ids so the log genuinely
//! deepens, drift bounded to one semitone so nothing wanders out of range at
//! depth 10,000, and the same operation the edit-loop slice already uses
//! (`src/editloop.rs`).
//!
//! Run: `cargo bench -p epiphany-testkit --bench editor_pipeline`. Add
//! `--features golden-gate` for the scene-build and paint columns (they need
//! `epiphany-render-svg` and `resvg`, which are optional for the MSRV reason
//! documented in `Cargo.toml`); without it those columns print an explicit
//! skip, never a silent absence. `EPIPHANY_BENCH_QUICK=1` gives the reduced
//! PR-CI shape. Under `cargo test --benches` criterion runs each measurement
//! once and the gate is skipped, exactly as the sibling benches do.

use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, SamplingMode};
use epiphany_core::{OperationId, PitchId, ReplicaId, Score, TranspositionInterval, WallClockTime};
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{
    to_constrained, to_logical, to_render, ConstraintSolver, ResolvedLayoutIR, SolverConfig,
};
use epiphany_ops::{
    AuthorId, CausalContext, HybridLogicalClock, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet, OperationStamp, TransposeIntervalOp,
};
use epiphany_testkit::budget::{self, Expectation};
use epiphany_testkit::fixtures;

/// `req:perf:single-system-edit-latency`: one frame at 60 Hz.
const FRAME_BUDGET: Duration = Duration::from_micros(16_700);

/// One documented scale point of THE STAGE TABLE below.
struct ScalePoint {
    /// Log depth: how many edits precede the one being timed.
    depth: usize,
    /// The documented expectation for `edit/core/<depth>` against
    /// [`FRAME_BUDGET`].
    expectation: Expectation,
    /// Budget-gate timed iterations (full mode, quick mode); `0` skips the row
    /// in that mode, printed as an explicit skip.
    gate_iters: (usize, usize),
    /// Criterion measurement time (full mode), or `None` to leave the point
    /// gate-only.
    criterion_time: Option<Duration>,
}

/// THE STAGE TABLE. Budget: the core's portion (reduce + engrave) within
/// 16.7 ms, `req:perf:single-system-edit-latency`.
///
/// Measured, dev profile, 2026-07-28, `--features golden-gate`:
///
/// | depth | reduce | engrave | **core** | scene-build | paint | verdict |
/// |-------|--------|---------|----------|-------------|-------|---------|
/// | 100    | 194 µs   | 276 µs | **471 µs**  | 135 µs | 2.12 ms | Pass, ~35x margin |
/// | 1,000  | 1.74 ms  | 311 µs | **2.06 ms** | 155 µs | 2.78 ms | Pass, ~8x margin |
/// | 10,000 | 16.99 ms | 263 µs | **17.26 ms** | 123 µs | 1.32 ms | Xfail, 3% over |
///
/// What the table says, in the order it matters:
///
/// 1. **`reduce` is the only column that scales with depth**, and it does so
///    close to linearly (194 µs → 1.74 ms → 16.99 ms for 100× the log). It is
///    99% of the core's portion at depth 10,000 and 41% of it at depth 100.
/// 2. **`engrave` is flat** — 263–311 µs regardless of depth, because the score
///    it engraves is the same size at every point. At *shallow* depth it is the
///    larger half of the core's portion, which qualifies Ruling A criterion 2's
///    "uninformative while reduction dominates": reduction does not dominate
///    until roughly depth 500.
/// 3. **The budget breaks at ~10,000 edits in one session** — and only just
///    (17.26 ms against 16.7 ms, on a dev box rather than the reference
///    hardware profile, at median rather than the requirement's p99). Read it
///    as "the wall is at this order of magnitude", not as a precise crossing.
/// 4. **`paint` is the largest single cost at every realistic depth** — 2.12 ms
///    at depth 100 is 4.5× the entire core portion. That is the SVG-string
///    path Ruling A demotes to export, and it is measured here as the number a
///    canvas has to beat.
/// 5. **Almost all of `scene-build` is the SVG serializer, not the IR work.**
///    Running the same rows *without* `golden-gate` — which drops the SVG
///    string and leaves only `to_render` + `hit_test_map` — gives **3.5 µs** at
///    depth 100 against the 135 µs above. So building the `RenderIR` and the
///    hit-test map costs ~3.5 µs and serializing it to SVG costs ~130 µs. A
///    canvas that consumes the IR directly (Ruling A) skips the 130 µs *and*
///    the 2.12 ms rasterize; together that is ~98% of today's per-edit cost at
///    depth 100, none of it in the core. Worth stating plainly because
///    "scene-build 135 µs" invites attributing the cost to IR construction,
///    which is off by a factor of nearly 40.
///
/// A row that starts missing after being marked `Pass` is a fresh regression —
/// fix the pipeline, do not re-mark it `Xfail` without a written decision (the
/// discipline `benches/reduction.rs` records).
const SCALE_POINTS: &[ScalePoint] = &[
    ScalePoint {
        depth: 100,
        expectation: Expectation::Pass,
        gate_iters: (9, 5),
        criterion_time: Some(Duration::from_secs(6)),
    },
    ScalePoint {
        // Drafted `Xfail` on the assumption that Fact 8 would already bite
        // here; it does not, with ~8x margin, and the gate's XPASS notice said
        // so. Promoted on first measurement rather than left stale.
        depth: 1_000,
        expectation: Expectation::Pass,
        gate_iters: (5, 3),
        criterion_time: Some(Duration::from_secs(10)),
    },
    ScalePoint {
        depth: 10_000,
        expectation: Expectation::Xfail(
            "Fact 8: `apply` re-reduces the whole log onto the pristine base on \
             every edit, so one keystroke costs a frame once the session is ~10k \
             edits deep (measured 17.26 ms against a 16.7 ms budget — a 3% miss, \
             so treat the depth as an order of magnitude, not a threshold). T4b \
             (checkpointed reduction + per-system re-engrave) owns the fix; \
             engrave is NOT implicated at 263 µs",
        ),
        gate_iters: (3, 0),
        criterion_time: None,
    },
];

/// The fixture every scale point edits: the QUICKSTART 10-measure single-staff
/// score (40 quarter notes), fixed so the engrave column is held still while
/// log depth moves.
fn base_score() -> Score {
    fixtures::ten_measure_single_staff(0x0000_ED17_5EED)
}

/// Every pitch in the fixture, in arena order — the targets the edit log
/// cycles through.
fn pitches(score: &Score) -> Vec<PitchId> {
    let mut out = Vec::new();
    for event in score.events.iter() {
        let mut ips = Vec::new();
        event.collect_identified_pitches(&mut ips);
        out.extend(ips.into_iter().map(|ip| ip.id));
    }
    out
}

/// One `TransposeInterval` envelope: `counter` gives it a distinct operation id
/// (so the log genuinely deepens rather than re-delivering one operation), and
/// `direction` alternates **per pass over the pitch list**, so a pitch edited
/// many times oscillates by one semitone instead of drifting.
///
/// The alternation must key on the pass, not on `counter`: with an even number
/// of target pitches, `counter % 2` is *constant for a given pitch*, so every
/// edit to it pushes the same way. That version of this fixture drifted each
/// pitch by ±25 semitones at depth 1,000 (and would have by ±250 at 10,000),
/// which silently inflated the engrave and paint columns with ledger lines and
/// accidentals — a score-content change masquerading as a log-depth cost.
fn edit_envelope(counter: u64, pitch: PitchId, direction: i32) -> OperationEnvelope {
    let id = OperationId::new(ReplicaId(1), counter);
    let chromatic_steps = direction;
    OperationEnvelope {
        id,
        author: AuthorId(0),
        stamp: OperationStamp::new(
            HybridLogicalClock::new(WallClockTime(counter as i64 + 1), 0),
            id,
        ),
        causal_context: CausalContext::new(),
        transaction: None,
        payload: OperationPayload::Primitive(OperationKind::TransposeInterval(
            TransposeIntervalOp {
                targets: [pitch].into_iter().collect(),
                interval: TranspositionInterval {
                    diatonic_steps: 0,
                    chromatic_steps,
                },
            },
        )),
    }
}

/// A reproducible edit log of `depth` envelopes over `score`'s own pitches.
fn edit_log(score: &Score, depth: usize) -> Vec<OperationEnvelope> {
    let targets = pitches(score);
    assert!(
        !targets.is_empty(),
        "the fixture must carry pitches to transpose"
    );
    (0..depth)
        .map(|i| {
            let pass = i / targets.len();
            let direction = if pass % 2 == 0 { 1 } else { -1 };
            edit_envelope(i as u64 + 1, targets[i % targets.len()], direction)
        })
        .collect()
}

/// **Stage 1 — reduce.** `EditorSession::materialize`'s first half: accept the
/// whole log into a fresh set and reduce it onto the pristine base. This is
/// the work Fact 8 identifies as growing with every keystroke.
fn reduce(base: &Score, log: Vec<OperationEnvelope>) -> Score {
    let mut set = OperationSet::new();
    for envelope in log {
        set.accept(envelope);
    }
    set.reduce_onto(base).score
}

/// **Stage 2 — engrave.** `materialize`'s second half, up to the resolved
/// layout: project to logical, to constrained, and solve. `to_render` and the
/// hit-test map belong to scene-build below.
fn engrave(score: &Score, engraver: &Engraver) -> Option<ResolvedLayoutIR> {
    let logical = to_logical(score);
    let report = engraver.solve(&to_constrained(&logical), &SolverConfig::default());
    report.status.is_renderable().then_some(report.layout)
}

/// **Stage 3 — scene-build.** The `RenderIR` and hit-test map every consumer
/// needs, plus (with `golden-gate`) the SVG string today's GUI feeds to the
/// rasterizer. Not in the core's budget; see the module note.
fn scene_build(resolved: &ResolvedLayoutIR) -> usize {
    let render = to_render(resolved);
    let map = render.hit_test_map();
    #[cfg(feature = "golden-gate")]
    {
        let svg =
            epiphany_render_svg::render(resolved, &epiphany_render_svg::RenderOptions::default());
        map.regions.len() + svg.svg.len()
    }
    #[cfg(not(feature = "golden-gate"))]
    {
        map.regions.len()
    }
}

/// **Stage 4 — paint.** The `usvg` parse + `resvg` rasterize the demo binary
/// performs per edit — the path Ruling A demotes to export. Not in the core's
/// budget, and measured as the baseline a canvas must beat.
#[cfg(feature = "golden-gate")]
fn paint(svg: &str) -> Option<usize> {
    use resvg::tiny_skia;
    use resvg::usvg;

    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Some(pixmap.data().len())
}

/// The pre-built inputs for one scale point, produced **outside** every timed
/// section: the base score, the log, the already-reduced score (the engrave
/// stage's input), and its resolved layout (scene-build's input).
struct StageInputs {
    base: Score,
    log: Vec<OperationEnvelope>,
    edited: Score,
    resolved: ResolvedLayoutIR,
    #[cfg(feature = "golden-gate")]
    svg: String,
}

fn stage_inputs(depth: usize, engraver: &Engraver) -> StageInputs {
    let base = base_score();
    let log = edit_log(&base, depth);
    let edited = reduce(&base, log.clone());
    let resolved = engrave(&edited, engraver).expect("the fixture engraves renderably");
    #[cfg(feature = "golden-gate")]
    let svg =
        epiphany_render_svg::render(&resolved, &epiphany_render_svg::RenderOptions::default()).svg;
    StageInputs {
        base,
        log,
        edited,
        resolved,
        #[cfg(feature = "golden-gate")]
        svg,
    }
}

fn criterion_measurements(criterion: &mut Criterion, quick: bool) {
    let engraver = Engraver::default();
    let mut group = criterion.benchmark_group("editor_pipeline");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    for point in SCALE_POINTS {
        let Some(time) = point.criterion_time else {
            continue; // the deepest point is gate-only; see THE STAGE TABLE.
        };
        if quick && point.depth > 100 {
            continue; // quick mode: the gate still measures the deeper points.
        }
        let inputs = stage_inputs(point.depth, &engraver);
        group.measurement_time(if quick { Duration::from_secs(2) } else { time });
        group.warm_up_time(Duration::from_millis(if quick { 500 } else { 1500 }));

        group.bench_with_input(
            BenchmarkId::new("reduce", point.depth),
            &inputs,
            |b, inputs| {
                b.iter_batched(
                    || inputs.log.clone(),
                    |log| reduce(&inputs.base, log),
                    BatchSize::PerIteration,
                )
            },
        );
        group.bench_with_input(
            BenchmarkId::new("engrave", point.depth),
            &inputs,
            |b, inputs| b.iter(|| engrave(&inputs.edited, &engraver)),
        );
        group.bench_with_input(
            BenchmarkId::new("scene_build", point.depth),
            &inputs,
            |b, inputs| b.iter(|| scene_build(&inputs.resolved)),
        );
        #[cfg(feature = "golden-gate")]
        group.bench_with_input(
            BenchmarkId::new("paint", point.depth),
            &inputs,
            |b, inputs| b.iter(|| paint(&inputs.svg)),
        );
    }
    group.finish();
}

/// The budget-gate side. Gates the **core's portion** (reduce + engrave) against
/// `req:perf:single-system-edit-latency`; prints the product-layer stages as
/// attributed measurements with no budget attached.
fn budget_gate(quick: bool) -> Vec<budget::GateReport> {
    let engraver = Engraver::default();
    let mut reports = Vec::new();
    println!("\n== staged edit latency (Ruling A criterion 2) ==");
    for point in SCALE_POINTS {
        let iters = if quick {
            point.gate_iters.1
        } else {
            point.gate_iters.0
        };
        if iters == 0 {
            println!(
                "skip  edit/core/{}: deepest scale point; full/nightly runs only \
                 (unset EPIPHANY_BENCH_QUICK)",
                point.depth
            );
            continue;
        }
        let inputs = stage_inputs(point.depth, &engraver);

        let reduce_median = budget::median_time(
            iters,
            || inputs.log.clone(),
            |log| reduce(&inputs.base, log),
        );
        let engrave_median =
            budget::median_time(iters, || (), |()| engrave(&inputs.edited, &engraver));
        let scene_median = budget::median_time(iters, || (), |()| scene_build(&inputs.resolved));

        // The gated row: the core's portion, which is exactly what the
        // requirement bounds.
        let core = reduce_median + engrave_median;
        println!(
            "stage edit/{}: reduce {:.2?} + engrave {:.2?} = core {:.2?}; \
             scene-build {:.2?} (product layer, no core budget)",
            point.depth, reduce_median, engrave_median, core, scene_median
        );
        #[cfg(feature = "golden-gate")]
        {
            let paint_median = budget::median_time(iters, || (), |()| paint(&inputs.svg));
            println!(
                "stage edit/{}: paint {:.2?} (product layer, no core budget; the \
                 SVG-string path Ruling A demotes to export)",
                point.depth, paint_median
            );
        }
        #[cfg(not(feature = "golden-gate"))]
        println!(
            "skip  edit/{}: scene-build's SVG column and the paint column need \
             `--features golden-gate` (epiphany-render-svg + resvg)",
            point.depth
        );

        reports.push(budget::latency_gate(
            format!("edit/core/{}", point.depth),
            core,
            FRAME_BUDGET,
            point.expectation,
        ));
    }
    reports
}

fn main() {
    let bench_mode = std::env::args().any(|arg| arg == "--bench");
    let quick = budget::quick_mode();

    let mut criterion = Criterion::default().configure_from_args();
    criterion_measurements(&mut criterion, quick);
    criterion.final_summary();

    if !bench_mode {
        return;
    }
    if !budget::verdict(&budget_gate(quick)) {
        std::process::exit(1);
    }
}
