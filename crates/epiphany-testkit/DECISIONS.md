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
  workspace-level, and the marquee bench (the reducer's `O(n²)`
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
