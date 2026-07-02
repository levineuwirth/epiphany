# epiphany-testkit — Decisions

Agent F's crate. The v0 decisions live in `spec/QUICKSTART.md` and the per-crate
DECISIONS files; this file records the **Phase 2** calls F makes as its mandate
broadens (`spec/PHASE2_QUICKSTART.md`, `spec/PHASE2_F_WEEK0_WORKLIST.md`).

## F0 — Where the new harnesses, benches, and the integration runner live

*Worklist item F0: "a 1-day call that everything else lands inside of." Decide
before writing F1/F3/F4/F5.*

**Decision: keep the examples + library-module pattern; do not adopt `xtask`;
put `benches/` in this crate; per-agent harnesses are library modules asserted
by `tests/` integration tests; the end-to-end integration harness is a `tests/`
integration test over a library `integration` module.**

Concretely:

- **No `xtask`, no `tests/reference-suite/`.** The QUICKSTART topology named
  both (HANDOFF §7-F), but their function is already met by per-crate
  `examples/*` fuzzers and `epiphany-testkit/examples/conformance_suite.rs`,
  which is the orchestration layer and the soak entry point. Adding `xtask`
  would introduce a second, parallel orchestration surface for no present gain;
  the workspace has one cross-cutting crate (this one) and `cargo` already
  drives every gate. Revisit only if cross-crate orchestration outgrows a single
  `examples/` runner (e.g. multi-binary pipelines that need a build graph).

- **Per-agent harnesses are library modules in `src/`**, alongside the v0
  harnesses (`roundtrip`, `convergence`, `equivocation`, `bundle_harness`,
  `negative`, `layout_stub`). Each new agent gets one module exposing
  `run_all()`-style entry points plus the granular asserts:
  - **H** → [`crate::prepass_harness`] (spelling/decomposition determinism,
    eligibility-taxonomy coverage, `RespellPitch` precedence, non-vacuity),
    driving the corpus in [`crate::corpus`]. **Live now** (H landed).
  - **I** → `engrave_harness` (hard-constraint validation against *declared* IR
    constraints + class-specific collision rules, provenance survival, SVG
    XML-validity, golden machine-acceptance snapshot). *Skeleton when I starts.*
  - **K** → `migration_harness` (deterministic + equivalence-preserving v0→v1
    migration, payload-schema completeness). *Skeleton when K starts.*
  - **J** → `wire_harness` (cross-impl decoder, canonicalization tests) +
    the wire-format fuzzer as an `examples/` soak target. *Skeleton when J
    starts.*

- **Each harness is asserted by a `tests/` integration test** (the same shape as
  `tests/acceptance.rs`) so it is a discrete `cargo test` target and a discrete
  CI job, and is **also** exercised at scale by `examples/conformance_suite.rs`
  for the nightly soak. The H harness lands as `tests/prepass.rs` + a
  `[prepass-harness]` stage in the conformance suite.

- **`benches/` lives in this crate** (not per-crate). The Chapter-10 budgets are
  workspace-level, and the marquee bench (the reducer's — at the time `O(n²)` —
  `canonical_reduction_order` at 10K+ envelopes, worklist F1) drives
  `epiphany-ops` *through* the testkit's envelope generators — exactly what this
  crate already does. A bench that lived in `epiphany-ops` could not reuse the
  generators without a dev-dependency cycle. Uses `criterion`; thresholds are
  written in the bench, with known-pending scale points marked `xfail` per F1.

- **The end-to-end integration harness (F5) is a `tests/` integration test** over
  a library `integration` module with documented stub swap-points, so real H/I/K/J
  stages replace stubs in place as they land, and the byte-identity assertion is
  wired from day one (trivially true on stubs, meaningful once stages are real).

**Why a library module + integration test, not a bare integration test:** the
harness logic (asserts, fingerprinting, the corpus) must be callable from both
the unit-budget `tests/` target *and* the at-scale `examples/conformance_suite`.
Bare `tests/` code is not importable across targets; library modules are. This
mirrors how v0's `convergence`/`roundtrip` modules are shared between
`tests/acceptance.rs` and the conformance example.

**Unblocks:** F1 (benches), F3 (corpus + taxonomy harness — done), F4 (per-agent
harness skeletons — H done), F5 (integration skeleton).

## F1 — The Chapter 10 budget benches (`benches/reduction.rs`, `benches/bundle.rs`)

*Worklist item F1: a criterion bench asserting Chapter-10 budgets at documented
scale points, known-pending points marked xfail with the numeric budget written
in the bench. Home per F0: this crate's `benches/`.*

**Criterion version: `0.5` (locks to 0.5.1), default features off plus
`cargo_bench_support`.** The 0.5 line's MSRV (1.70) fits the workspace's pinned
`rust-version = "1.77"`; criterion 0.6+ requires 1.80. Disabling default
features drops the plotting stack (`plotters`) and `rayon` — dead weight for
budget gates — while `cargo_bench_support` keeps plain `cargo bench` as the
entry point. Two transitive pins in `Cargo.lock` keep the tree MSRV-clean,
because the resolver (v2) is not MSRV-aware: `clap 4.5.53` (MSRV 1.74; 4.6
requires 1.85) and `half 2.4.1` (MSRV 1.70; 2.5+ requires 1.81). Re-check those
two if `cargo update` touches the criterion tree.

**The gate mechanism.** Criterion measures but never asserts, so each bench is
`harness = false` and its `main()` runs criterion's measurements first, then
the budget gate in [`crate::budget`] — a library module (the F0 pattern: shared
logic lives in `src/`, and it deliberately uses no criterion types, so the
library itself takes no new dependency). Every budget row carries an
`Expectation` written in the bench source next to its numeric threshold:

- `Pass` — must hold today; a miss exits nonzero (the CI tripwire).
- `Xfail(reason)` — documented known-pending miss; the reason names the defect
  and owner. A miss prints an expected-failure line; a **pass** prints an XPASS
  promotion notice, so a stale marking is loud in both directions.

Skipped rows (the 50K point in quick mode) print an explicit `skip` line, never
silence. Under `cargo test --benches` (criterion test mode, detected by the
absence of the `--bench` flag `cargo bench` passes) the measurements run once
and the gate is skipped — the gate belongs to `cargo bench`.

**Sampling calibration (a documented deviation from Chapter 10).** The
conformance methodology — p99 over ≥1000 iterations on the reference hardware
profile — is the reference suite's job, not this gate's: 1000 iterations of the
(pre-fix ~29 s) 50K cold reduction was not a CI shape. The gate takes the
**median of a small per-row iteration count** (reduction: 9/3/1 iterations at
1K/10K/50K; bundle: 40 commit / 25 read; cut to 5/2/skip and 12/8 under
`EPIPHANY_BENCH_QUICK=1`),
cold (no in-gate warm-up — the marquee budget is an explicitly *cold* rate).
Criterion uses flat sampling with `sample_size(10)` for the reduction group;
the 50K point is gate-only (calibrated when a single iteration took ~29 s and
criterion's ≥10-sample loop would have blown the wall-clock budget; ~0.58 s
post-fix). Full `cargo bench -p epiphany-testkit` stays under ~2 minutes of
measurement; quick mode under ~15 seconds.

**Measured on the dev profile (2026-07, post-K-fix), the table's ground
truth:**

| row | budget | measured | verdict |
|-----|--------|----------|---------|
| `reduction/1000` | > 10,000 env/s cold | ~674,000 env/s (~1.5 ms) | Pass |
| `reduction/10000` | > 10,000 env/s cold | ~257,000 env/s (~39 ms) | Pass |
| `reduction/50000` | > 10,000 env/s cold | ~87,000 env/s (~0.58 s) | Pass (promoted from Xfail) |
| `bundle/typical_edit_commit` | ≤ 50 ms | ~14.7 ms (real fsync) | Pass |
| `bundle/open_bootstrap_read` | ≤ 200 ms | ~63 µs (moderate corpus) | Pass |

As F1 first measured it (2026-07, pre-fix), the O(n²)
`canonical_reduction_order` indegree construction was unambiguous in the curve
(~155K / ~12.5K / ~1.7K env/s at 1K/10K/50K — 10x envelopes cost ~120x time),
sinking the 50K row (~29 s per cold reduce, the documented xfail) and leaving
10K only ~25% over budget. Agent K's subquadratic rewrite
(threshold/frontier readiness, byte-identical order — see
`epiphany-ops/DECISIONS.md`) triggered the gate's XPASS promotion notice, and
the 50K row was flipped to Pass in the same change (**F surfaces, K fixes**,
round-tripped). Any future red row is a fresh regression: fix the reducer, do
not re-mark rows xfail without a written decision here.

**Two honesty notes on the bundle rows.** (1) They run on the build target's
filesystem, *not* `std::env::temp_dir()`: `/tmp` is commonly tmpfs, where fsync
is a near-no-op — measured, the commit row is ~54 µs on tmpfs vs ~14.7 ms on a
real NVMe filesystem, a 270x difference that would have made the 50 ms budget
vacuous. (2) The read budget is spec'd against "a 100-page orchestral score";
no such corpus generator exists yet, so the row is a moderate-corpus (1K
envelopes + canonical-base snapshot) stand-in with margin to spare, and says so
in its docs.

**Scope, and the budget rows deliberately not built yet.** F1 covers the three
budgets that are measurable against shipped subsystems: the reduction rate and
the two file-format budgets. The remaining Chapter 10 rows are blocked on
unimplemented or in-flight subsystems and become future benches in this same
`benches/` + gate shape: the interactive keystroke→frame budgets (blocked on
Agent I's engrave/render pipeline maturing past the visible slice), cold
solving of a 100-page score within 2 s p99 and the incremental-propagation
bound (blocked on the real Chapter 9 solver and a 100-page corpus generator —
the latter also unblocks the honest read-row corpus), and the memory ceilings
(blocked on the same corpus). CI runs the gates in the `conformance` job with
`EPIPHANY_BENCH_QUICK=1` (50K skipped) and in the nightly `soak` job in full.

## F3 — The representative score corpus + eligibility-taxonomy harness (Agent H)

*Worklist item F3 — "the most underbuilt dependency; unblocks H entirely."*

The corpus lives in [`crate::corpus`] as ≥20 deterministic, `check_invariants`-clean
fixtures tagged by tier (common / edge / torture) and by the event-kind
eligibility taxonomy of `PHASE2_QUICKSTART §H`. Rather than re-deriving the
taxonomy, the harness runs Agent H's own `epiphany_core::derive_annotations` and
reads its [`epiphany_core::TaxonomyReport`], then (a) independently recounts
events by kind and **cross-checks** H's counts (so a miscount is caught, not
trusted), and (b) aggregates per-bucket counts across the corpus and asserts every
taxonomy bucket is non-empty or explicitly deferred. The corpus also re-uses the
existing positive generators (`valid_score`, `valid_score_rich`,
`ten_measure_single_staff`) as fixtures so F's taxonomy harness runs over the same
graphs Agents H and I develop and render against.

**Deferred buckets** (documented, not required non-empty by the clean corpus):
none — every bucket including `decomposition_skipped_nonmusical` (zero-duration
grace) and `decomposition_ungriddable` (off-grid / sub-sixty-fourth torture
cases) is exercised by a dedicated fixture, so the honest-classification paths are
all proven reachable. If a future invariant change makes a bucket unreachable by
clean input, move it to `corpus::DEFERRED_BUCKETS` with a written reason rather
than dropping the assertion.

## F4 (H) — The H merge gate

[`crate::prepass_harness`] is H's merge gate (worklist F4). It asserts H's stated
`PHASE2_QUICKSTART` acceptance criterion over the corpus: every *eligible*
`IdentifiedPitch` carries a **non-trivial** spelling (verified by pitch-class
correctness, which the old constant-`C4` stub fails); every *eligible* determinate
metric duration carries a `Decomposition` whose components reconstruct the
duration (invariant 15); ineligible cases are classified and counted; derivation
is deterministic across runs (asserted by structural equality **and** a canonical
textual fingerprint, since `DerivedAnnotations` deliberately has no codec);
`RespellPitch`-style authored overrides take precedence; and the derivation stays
deterministic when run on materialized scores in the criterion-5 pipeline.

**Non-vacuity guard** (the F discipline — the gate must go red if H were stubbed):
across the corpus the harness requires multiple distinct spelled nominals and at
least one accidental (a constant-`C4` stub yields one nominal, zero accidentals),
at least one multi-component (tied) decomposition and multiple distinct note
values (an empty/no-op decomposition map yields neither), and per-pitch
pitch-class correctness (the stub mis-spells the first non-`C` pitch).

## Pass 12 batch tracker

Per F's mandate, the Pass 12 batch is tracked in `spec/PASS12_BATCH.md`. It opens
once ≥3 ambiguities accumulate (same rule as v0 → Pass 11). Agent H's landing
contributed five candidates (P12-H1…P12-H5, recorded in
`crates/epiphany-core/DECISIONS.md`), which crosses the threshold, so the batch is
open. F does not resolve these; F collects them.
