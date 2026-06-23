# Phase 2 — Agent F Week-0 Worklist

*Companion to `spec/PHASE2_QUICKSTART.md`. Scope: the net-new F artifacts that the Phase 2 agents (G–K) need in order to **land**, ordered for front-loading.*

## Premise

F's v0 mandate is **complete and honest** — all six v0 acceptance criteria pass
(`cargo test -p epiphany-testkit --test acceptance` → 13/13, **0 ignored**), and
the gates were hardened from "green-but-projection" to real-`Score` last session.
Nothing here is a v0 fix.

Nothing on this list blocks Phase 2 from *beginning*. Every Phase 2 agent's
*start* is gated on G's Pass 11 byte conventions and/or a peer's output, never on
an unfinished F deliverable. G is already running (`b2f2e20`, `a7adbdc`,
`0d8ec61`). **This list is what F must produce so the agents can *land* and be
*gated* — front-loaded because several agents must develop against these
artifacts, not just be tested by them.**

The hard scheduling constraint: agents land ~Week 10–15; **per-agent harnesses
must be in CI by ~Week 6** so no agent outruns its gate.

---

## Priority-ordered items

### F0 — Structural decision: `benches/`, `xtask`, and the reference-suite gap *(do first; it's a 1-day call that everything else lands inside of)*

- **Why now:** F is about to add a `benches/` directory and the integration
  harness. The QUICKSTART topology names `xtask/` and `tests/reference-suite/`
  that **do not exist** (HANDOFF §7-F) — their function is currently met by
  per-crate `examples/*` fuzzers + `epiphany-testkit/examples/conformance_suite.rs`.
  Decide the home for the new harnesses *before* writing them.
- **Current state:** No `benches/`, no `xtask/`, no `tests/reference-suite/`.
- **Deliverable:** A one-paragraph decision recorded in
  `crates/epiphany-testkit/DECISIONS.md`: adopt `xtask` for orchestration or keep
  the examples pattern; where `benches/` lives (testkit vs. per-crate); whether
  the integration harness is a `tests/` integration test or an `examples/` runnable.
- **Acceptance:** Decision written; no code yet. Unblocks F1, F4, F5.

### F1 — `benches/` scaffold with xfail thresholds *(unblocks K's perf gate)*

- **Why:** K's acceptance requires "F's performance bench passes the documented
  budget" at 10K-envelope scale; F's mandate is to **set the threshold first as an
  xfail gate**, then K fixes the O(n²) `canonical_reduction_order` indegree
  construction when the bench goes red. The bench must exist before K can be held
  to it.
- **Current state:** No `benches/` directory anywhere in the workspace.
- **Deliverable:** `criterion`-based bench (location per F0) asserting Chapter-10
  budgets at documented scale points (1K = current criterion-5 scale, 10K, 50K),
  with known-pending points marked xfail with the budget written in the bench.
- **Acceptance:** `cargo bench` runs; 10K point is a documented xfail with a
  numeric budget; passing at 1K. F **surfaces**, K fixes.

### F2 — Frozen v0 envelope corpus *(unblocks K migration + J round-trip regression)*

- **Why:** K's v0→v1 migration must be proven deterministic and
  equivalence-preserving against a **v0 envelope corpus**; J's criterion-4 gate
  must hold byte-for-byte on the same v0 corpus. Both need a *frozen, persisted*
  corpus, not a re-generated one.
- **Current state:** Edit-session **generators** exist
  (`convergence.rs`: `two_staff_edit_session`, `graph_edit_session`), seeded and
  deterministic — but nothing is captured as a stable regression artifact.
- **Deliverable:** Capture a representative v0 envelope set (drawn from the
  existing generators at fixed seeds), freeze its canonical bytes as a golden
  fixture, and expose a `testkit` accessor (`fn v0_envelope_corpus() -> ...`) plus
  a golden-lock test that fails if the v0 byte shape drifts.
- **Acceptance:** Corpus accessor + golden-lock test in CI; documented seeds; the
  bytes are the regression guard K's migration and J's decoder run against.

### F3 — H's representative score corpus + kind-by-kind taxonomy harness *(most underbuilt dependency; unblocks H entirely)*

- **Why:** H's *whole* acceptance rests on "F's representative corpus (>20
  fixtures) spanning common / edge / torture cases" exercising the event-kind
  **eligibility taxonomy** — and H needs it to develop against, not just to be
  tested by. This is the single largest authoring gap.
- **Current state:** Exactly one hand-built fixture
  (`fixtures.rs::ten_measure_single_staff`) + two generators
  (`generators.rs::valid_score`, `valid_score_rich`). **None** exercise: rests,
  unpitched/percussion spelling, trajectory/graphic/indeterminate events,
  proportional/aleatoric regions, tuplets, ties across barlines/tempo changes,
  syncopation.
- **Deliverable:** ≥20 invariant-clean fixtures in `fixtures.rs` (or a
  `fixtures/` submodule) tagged by the taxonomy from PHASE2_QUICKSTART §H "What H
  produces, by event kind," plus a harness that **classifies and counts** each
  case (so "ineligible" is explicit and counted, never silently absent).
- **Acceptance:** ≥20 fixtures, each `check_invariants`-clean; harness emits
  per-kind counts; the taxonomy buckets (eligible-pitch, eligible-duration,
  rest, unpitched, trajectory/graphic/indeterminate, proportional/aleatoric) are
  each non-empty or explicitly marked deferred.
- **Note:** This is the long pole. Start immediately even though H lands ~Week 10.

### F4 — Per-agent harness templates + skeletons in CI *(the ~Week-6 deadline; unblocks gating for all of G–K)*

- **Why:** "Treat F's harness for your scope as your merge gate." If an agent
  races ahead of its harness, it merges ungated. Templates first (Week 0), real
  harnesses wired as stages land (by ~Week 6).
- **Current state:** v0 harnesses exist (`roundtrip`, `convergence`,
  `equivocation`, `bundle_harness`, `negative`, `layout_stub`). No H/I/K/J-specific
  harnesses.
- **Deliverable:** Skeleton modules + CI jobs (initially xfail/empty) for:
  - **H:** spelling/decomposition determinism across runs; `RespellPitch`
    precedence; non-vacuity.
  - **I:** hard-constraint validation against *declared* IR constraints (not
    bounding-box heuristics) + class-specific collision rules
    (accidental-vs-notehead, stem-vs-beam, staff-line-vs-glyph); provenance
    survival; SVG XML-validity; golden-locked machine acceptance snapshot
    (object/glyph/bbox-class/provenance/constraint counts).
  - **K:** v0→v1 migration determinism + equivalence; payload-schema completeness
    (every K0 primitive has an `epiphany-ops` payload type); K1 framework present.
  - **J:** cross-impl decoder test; wire-format fuzzer; canonicalization tests
    (map order, NFC, `-0.0`, rational reduction, unknown-field preservation).
- **Acceptance:** Each module exists with a **non-vacuity guard** (would fail if
  the agent's work were stubbed); each is a CI job; each asserts its agent's
  stated PHASE2_QUICKSTART acceptance criterion once the agent's stage is real.

### F5 — End-to-end integration-harness skeleton *(the Phase-2 close gate; skeleton now, real by close)*

- **Why:** The marquee F deliverable — proves the two tracks *compose* at the
  seam (both can be green while the serialized visible score is broken between
  them). Skeleton Week 0 against stub stages; real stages swapped in as H/I/K/J
  land; fully real by Phase-2 close.
- **Current state:** Does not exist.
- **Deliverable:** A single-fixture pipeline runner (home per F0):
  `Score → reduction → H → layout IR → I solver → SVG → J write → J read →
  reduction → H → layout → SVG`, asserting canonical state and SVG are
  byte-identical between passes (modulo explicitly allowed non-canonical caches).
  Stages stubbed where the agent hasn't landed.
- **Acceptance:** Skeleton runs end-to-end with stub stages now; documented swap
  points for each real stage; the byte-identity assertion is wired (trivially
  passing on stubs, meaningfully once stages are real).

---

## Explicitly *not* Week-0 (landing gates that mature later)

- Real cross-implementation decoder written from J's companion **text** — waits on
  J's Binary Format companion existing.
- Wire-format fuzzer at 1M-iteration CI soak — F4 stands up the skeleton; the
  real fuzzer matures with J's codec.
- Integration harness with **all real** stages — that's the Phase-2 close
  condition (F5 is just the skeleton).
- Pass 12 batch tracker — created when the first ambiguity lands; don't open Pass
  12 until ≥3 items (same rule as v0).

## Ordering

```
F0 (decision)  ──┬─> F1 (benches)        ── unblocks K perf gate
                 ├─> F2 (v0 corpus)       ── unblocks K migration + J roundtrip
                 ├─> F4 (harness skeletons) ── ~Week-6 CI deadline for all gates
                 └─> F5 (integration skeleton) ── Phase-2 close gate
F3 (H corpus)  ──── start in parallel; long pole; H can't develop without it
```

F3 and F0 can start the same day. F0 must precede F1/F4/F5 (it decides where they
live). F3 is independent of F0 and is the item most likely to slip if deferred.
