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
//! | construct | mint the one new `OperationEnvelope` | **yes** |
//! | reduce | `OperationSet::accept` × log + `reduce_onto(&base)` | **yes** |
//! | engrave | `to_logical` → `to_constrained` → `Engraver::solve` | **yes** |
//! | scene-build | `to_render` + `hit_test_map` (+ SVG string) | no — product layer |
//! | paint | `usvg` parse + `resvg` rasterize | no — product layer |
//!
//! Only the first three are gated, and all three are gated: the requirement
//! names "operation envelope **construction**, reduction, incremental layout",
//! so `construct` is timed and summed even though it is tens of nanoseconds and
//! never moves the verdict. A gate that silently drops a named component is a
//! proxy for the requirement rather than the requirement.
//!
//! `req:perf:single-system-edit-latency` bounds "the core's portion" and says
//! so explicitly — "End-to-end edit-to-pixel latency (input handling, hit
//! testing, render submission, display flip) is a product-layer obligation" —
//! so charging the SVG serializer and `resvg` against a core budget would be a
//! category error. They are measured and printed because the ruling asks for
//! the stages *separately*, and because today's scene-build+paint is the path
//! Ruling A demotes: the number is the baseline a canvas must beat, not a
//! budget to defend.
//!
//! ## The log must be shaped like a session's, or the reduce column lies
//!
//! `reduce`'s cost is dominated by ordering work over the log's **causal
//! edges**, so a log whose envelopes carry empty causal contexts measures a
//! different algorithm than the one production runs. The first version of this
//! bench made exactly that mistake and understated `reduce` by ~3× at depth
//! 10,000 (17 ms rather than 54 ms), which moved the reported wall by more than
//! a factor of two and would have mis-sequenced T4b. [`edit_log`] now
//! reproduces `EditorSession`'s minting shape: counters from 0, only the root
//! context empty, every later envelope carrying the head's context extended by
//! the head.
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
//! **The score's *size* is held fixed; its *content* is not, and three columns
//! read that.** The edits are transpositions, so the reduced score differs from
//! the base by up to ±1 semitone per pitch, and a transposed pitch may acquire
//! an accidental. Because the edit log cycles the pitch list and alternates
//! direction per pass, the accidental count depends on the *parity of the pass
//! count* at that depth: depths 1,000 / 3,000 / 5,000 are 25 / 75 / 125 passes
//! (odd — every pitch sits one semitone off the base, most carrying an
//! accidental), while 10,000 is 250 (even — every pitch is back where it
//! started). That is why `paint` is **non-monotonic** in depth below: it sits
//! near 2.8 ms at every odd-parity depth and drops to 1.36 ms at 10,000, where
//! the score simply has less ink. `reduce` is the only column that tracks
//! depth; `engrave`, `scene-build`, and `paint` track score content. Reading
//! paint's dip at 10,000 as a scaling win would be a mistake.
//!
//! **Depth is document-lifetime, not per session.** The score-only
//! `EditorSession::open` does start with an empty `applied` log, but that is
//! the probe path, not the savable-document path: under Ruling B
//! (`spec/PLAN_EDITOR_APP.md`) reopen is **full replay** — stored envelopes
//! load as a committed partition and materialization reduces committed +
//! session operations together. Nothing resets the depth this bench varies
//! until the checkpoint/pruning machinery assigned to T4b exists to write a
//! new `canonical_base`. So the wall below is a budget on a **document's
//! whole edit history**, and note entry mints one operation per note.
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

/// THE STAGE TABLE. Budget: the core's portion (envelope construction +
/// reduce + engrave) within 16.7 ms, `req:perf:single-system-edit-latency`.
///
/// Measured, dev profile, 2026-07-28, `--features golden-gate`, on a
/// **session-shaped log** (see the module note — the first published table used
/// empty causal contexts and understated `reduce` by ~3× at depth 10,000):
///
/// | depth | construct | reduce | engrave | **core** | scene-build | paint | verdict |
/// |-------|-----------|--------|---------|----------|-------------|-------|---------|
/// | 100    | 40 ns | 223 µs   | 268 µs | **491 µs**  | 133 µs | 2.21 ms | Pass, 34× margin |
/// | 1,000  | 40 ns | 2.30 ms  | 314 µs | **2.61 ms** | 165 µs | 2.86 ms | Pass, 6.4× margin |
/// | 3,000  | 40 ns | 8.79 ms  | 327 µs | **9.11 ms** | 152 µs | 2.80 ms | Pass, 55% of budget |
/// | 5,000  | 40 ns | 17.53 ms | 317 µs | **17.84 ms** | 151 µs | 2.78 ms | **Xfail, 107%** |
/// | 10,000 | 40 ns | 54.07 ms | 260 µs | **54.33 ms** | 123 µs | 1.36 ms | Xfail, 3.3× over |
///
/// (Depth 4,000, measured clean but not gated — see the note on the scale
/// points: core **12.99 ms**, 78% of budget.)
///
/// What the table says, in the order it matters:
///
/// 1. **`reduce` is the only column that scales with depth, and it is
///    superlinear** — 10× the log costs ~23.5× the time between depths 1,000
///    and 10,000, roughly `O(n^1.4)`. It is 45% of the core's portion at depth
///    100 and 99.5% at depth 10,000. (This does not contradict
///    `benches/reduction.rs`'s subquadratic result at 50K envelopes: that log
///    is generated across three replicas with a different causal shape. Two
///    logs of equal length are not equal work.)
/// 2. **The frame budget breaks between 3,000 and 5,000 edits** — 9.11 ms
///    (55%), 12.99 ms at 4,000 (78%), then 17.84 ms (107%). Call the wall
///    ~4,500 edits of document history, on a dev box rather than the reference hardware
///    profile and at median rather than the requirement's p99, so treat it as
///    an order of magnitude rather than a threshold.
/// 3. **`engrave` is flat and small** — 260–327 µs at every depth, because the
///    score it engraves is the same size throughout. At depth 100 it is the
///    *larger* half of the core's portion, so criterion 2's "uninformative
///    while reduction dominates" holds only past roughly depth 500, not from
///    the start.
/// 4. **`paint` dominates early and is overtaken by depth ~1,000.** At depth
///    100 it is 2.21 ms against a 491 µs core — 4.5×. By 1,000 they are level
///    (2.86 ms vs 2.61 ms). Past that the core runs away. The earlier claim
///    that the render path dominates "at realistic depths" holds only for the
///    first thousand-odd edits of a session.
/// 5. **Almost all of `scene-build` is the SVG serializer, not the IR work.**
///    The same rows *without* `golden-gate` — which drop the SVG string and
///    leave only `to_render` + `hit_test_map` — measure **3–5 µs**, against
///    133–165 µs with it. So the `RenderIR` and hit-test map cost a few
///    microseconds and serializing to SVG costs ~130 µs. Stating that plainly
///    matters because "scene-build 133 µs" invites attributing the cost to IR
///    construction, which is off by a factor of ~30.
/// 6. **What a direct-IR canvas avoids, with the denominator named.** It skips
///    the ~129 µs serialize and the 2.21 ms rasterize: 2.34 ms at depth 100.
///    That is **83% of the full measured per-edit pipeline** (2.83 ms) and
///    **99.8% of the render path alone** (2.34 ms). Both figures are worth
///    having and they answer different questions; an unqualified "98%" was
///    supported by neither.
///
/// **Sequencing, stated carefully.** T4 (the canvas) still comes first: it
/// removes the cost that dominates a session's first ~1,000 edits, and it is
/// the architecture every later tranche builds on. But T4b's trigger is much
/// nearer than the first version of this table suggested — ~4,500 edits in one
/// sitting, not ~10,000 — and the two are no longer comfortably separated.
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
        // here; it does not, and the gate's XPASS notice said so. Promoted on
        // first measurement rather than left stale.
        depth: 1_000,
        expectation: Expectation::Pass,
        gate_iters: (5, 3),
        criterion_time: Some(Duration::from_secs(10)),
    },
    // 3,000 and 5,000 bracket the crossing. They exist because the first
    // version of this bench put the wall at ~10,000 on a context-free log; with
    // production-shaped contexts it arrives here instead, and a table that only
    // sampled decades would have reported the wrong order of magnitude for the
    // trigger T4b is sequenced against.
    //
    // The last `Pass` row is 3,000 rather than 4,000 deliberately. A clean run
    // puts 4,000 at 12.99 ms — a real pass, but only 78% of budget, and a row
    // that close flaps the moment the machine is doing anything else (a
    // load-contaminated run measured it at 22.77 ms, *above* the 5,000 row,
    // which is impossible clean). A `Pass` row that fails under load teaches
    // people to ignore the gate. 4,000's clean number is kept as data in THE
    // STAGE TABLE instead of as a gated row.
    ScalePoint {
        depth: 3_000,
        expectation: Expectation::Pass,
        gate_iters: (5, 3),
        criterion_time: Some(Duration::from_secs(12)),
    },
    ScalePoint {
        depth: 5_000,
        expectation: Expectation::Xfail(
            "Fact 8: `apply` re-reduces the whole log onto the pristine base on \
             every edit, and each envelope's causal context makes that ordering \
             work real, so one keystroke costs more than a frame from roughly \
             this depth. T4b (checkpointed reduction + per-system re-engrave) \
             owns the fix; engrave is NOT implicated, staying flat in the \
             hundreds of microseconds at every depth",
        ),
        gate_iters: (5, 3),
        criterion_time: Some(Duration::from_secs(12)),
    },
    ScalePoint {
        depth: 10_000,
        expectation: Expectation::Xfail(
            "Fact 8, well past the wall — see the 5,000 row. Kept as the \
             order-of-magnitude datum, and gate-only because a single timed \
             reduction here is tens of milliseconds",
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
fn edit_envelope(
    counter: u64,
    pitch: PitchId,
    direction: i32,
    causal_context: CausalContext,
) -> OperationEnvelope {
    let id = OperationId::new(REPLICA, counter);
    let chromatic_steps = direction;
    OperationEnvelope {
        id,
        author: AuthorId(0),
        stamp: OperationStamp::new(
            HybridLogicalClock::new(WallClockTime(counter as i64 + 1), 0),
            id,
        ),
        causal_context,
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

/// The replica the synthetic session authors as.
const REPLICA: ReplicaId = ReplicaId(1);

/// `EditorSession`'s `extend_context`, reproduced (`editor-core/src/lib.rs`):
/// a context grows by absorbing the head into its contiguous vector when the
/// head continues that replica's run, and by a dot otherwise. A single-replica
/// session with no undo always takes the contiguous branch.
fn extend_context(context: CausalContext, op: OperationId) -> CausalContext {
    let continues = context
        .vector
        .get(&op.replica)
        .map_or(op.counter == 0, |&high| op.counter == high + 1);
    if continues {
        context.with_seen(op.replica, op.counter)
    } else {
        context.with_dot(op)
    }
}

/// A reproducible edit log of `depth` envelopes over `score`'s own pitches,
/// **shaped like a real `EditorSession` log**.
///
/// Two details are load-bearing, and the first version of this bench got both
/// wrong — with the empty-context version understating `reduce` by ~3x at depth
/// 10,000, because a context-free log gives the reducer no causal edges to
/// order and so skips most of `canonical_reduction_order`'s work:
///
/// * **Counters start at 0.** `EditorSession` mints with
///   `counter = self.authored.len()`, so the root op is counter 0 — which is
///   also what `extend_context` recognises as the start of a contiguous run.
/// * **Only the root context is empty.** Every later envelope carries
///   `active_prior_context()` — the head's own context extended by the head —
///   so it covers the whole active prefix. This is what makes two sequential
///   edits to one target read as intentional overwrites rather than concurrent
///   conflicts, and it is what the reducer's topological ordering consumes.
fn edit_log(score: &Score, depth: usize) -> Vec<OperationEnvelope> {
    let targets = pitches(score);
    assert!(
        !targets.is_empty(),
        "the fixture must carry pitches to transpose"
    );
    let mut log: Vec<OperationEnvelope> = Vec::with_capacity(depth);
    let mut context = CausalContext::new();
    for i in 0..depth {
        let pass = i / targets.len();
        let direction = if pass % 2 == 0 { 1 } else { -1 };
        let envelope = edit_envelope(
            i as u64,
            targets[i % targets.len()],
            direction,
            context.clone(),
        );
        context = extend_context(context, envelope.id);
        log.push(envelope);
    }
    log
}

/// **Stage 0 — envelope construction.** The requirement names it first
/// ("operation envelope construction, reduction, incremental layout"), so the
/// gated core includes it rather than treating it as setup: this builds the
/// *one new* envelope an edit mints, on top of a log already `depth` deep.
fn construct(targets: &[PitchId], depth: usize, context: &CausalContext) -> OperationEnvelope {
    let pass = depth / targets.len();
    let direction = if pass % 2 == 0 { 1 } else { -1 };
    edit_envelope(
        depth as u64,
        targets[depth % targets.len()],
        direction,
        context.clone(),
    )
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
    /// The construct stage's inputs: the fixture's pitch list and the causal
    /// context the *next* edit would carry (the head's, extended by the head).
    targets: Vec<PitchId>,
    next_context: CausalContext,
    #[cfg(feature = "golden-gate")]
    svg: String,
}

fn stage_inputs(depth: usize, engraver: &Engraver) -> StageInputs {
    let base = base_score();
    let log = edit_log(&base, depth);
    let edited = reduce(&base, log.clone());
    let resolved = engrave(&edited, engraver).expect("the fixture engraves renderably");
    let targets = pitches(&base);
    let next_context = match log.last() {
        None => CausalContext::new(),
        Some(head) => extend_context(head.causal_context.clone(), head.id),
    };
    #[cfg(feature = "golden-gate")]
    let svg =
        epiphany_render_svg::render(&resolved, &epiphany_render_svg::RenderOptions::default()).svg;
    StageInputs {
        base,
        log,
        edited,
        resolved,
        targets,
        next_context,
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
            BenchmarkId::new("construct", point.depth),
            &inputs,
            |b, inputs| b.iter(|| construct(&inputs.targets, point.depth, &inputs.next_context)),
        );
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

/// The budget-gate side. Gates the **core's portion** (envelope construction +
/// reduce + engrave — all three the requirement names) against
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

        // Envelope construction is the requirement's first named component, so
        // it is timed and summed rather than treated as setup — even though it
        // is sub-microsecond and never moves the verdict, because a gate that
        // silently drops a named component is a proxy, not the gate.
        let construct_median = budget::median_time(
            iters,
            || (),
            |()| construct(&inputs.targets, point.depth, &inputs.next_context),
        );
        let reduce_median = budget::median_time(
            iters,
            || inputs.log.clone(),
            |log| reduce(&inputs.base, log),
        );
        let engrave_median =
            budget::median_time(iters, || (), |()| engrave(&inputs.edited, &engraver));
        let scene_median = budget::median_time(iters, || (), |()| scene_build(&inputs.resolved));

        // The gated row: the core's portion, which is exactly what the
        // requirement bounds — "operation envelope construction, reduction,
        // incremental layout through ResolvedLayoutIR".
        let core = construct_median + reduce_median + engrave_median;
        println!(
            "stage edit/{}: construct {:.2?} + reduce {:.2?} + engrave {:.2?} = core {:.2?}; \
             scene-build {:.2?} (product layer, no core budget)",
            point.depth, construct_median, reduce_median, engrave_median, core, scene_median
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
