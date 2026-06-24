# epiphany-engrave — decisions and Pass 12 candidates

This file records (a) the implementation decisions the Phase-2 QUICKSTART asked
Agent I to make once and document, and (b) ambiguities discovered while building
the crate, batched as **Pass 12 candidates** (`spec/PASS12_BATCH.md`) rather than
improvised in code.

## Scope and phase status

`epiphany-engrave` is the production-side **constraint solver** (Chapter 9): it
turns a `ConstrainedLayoutIR` into a `ResolvedLayoutIR` with real geometry. It is
a separate crate from `epiphany-layout-ir` deliberately — `layout-ir` is the
*interface* layer (the graph↔renderer contract); the actual constraint-solving is
on the product side of the spec's core/product boundary, so replacing the
`StubSolver` *inside* `layout-ir` would blur it (`spec/PHASE2_QUICKSTART.md`,
crate topology).

**This phase ships the renderer-against-stub slice** (QUICKSTART, Agent I,
"Development pattern": build the renderer against the stub solver first, then
grow the real solver). So this crate is an **honest scaffold**: `Engraver` runs a
genuine deterministic *horizontal spacing pass* — the first axis of the planned
two-pass spring layout — placing each spring slot left-to-right by its preferred
width, rather than echoing the stub's input columns. It does **not yet** run the
vertical pass, the soft-spring stretch/compress solve, or evaluate the IR's
declared hard constraints.

### Honest tier

By the same rule `layout-ir`'s `StubSolver` follows, a solver that does not
evaluate the declared hard constraints and computes no quality metrics MUST
report `SolverTier::Stub`, never `Minimal` (Chapter 9 §"Conformance Tiers").
`Engraver::tier()` therefore reports `Stub` today; it is promoted to `Minimal` in
the same change that lands real hard-constraint satisfaction. A regression test
(`reports_the_honest_stub_tier_until_it_earns_minimal`) guards this so the tier
cannot be silently inflated. The quality-metric vector stays the conservative
all-worst placeholder (`QualityMetricVector::unmeasured`) until the Quality Metric
Catalog lands (Phase 3, explicitly out of Agent I's scope).

## Implementation decisions (QUICKSTART "Decisions you'll need to make")

1. **Spelling algorithm** — N/A (Agent H, `epiphany-core`).
2. **Solver architecture for engraving — two-pass spring layout (horizontal then
   vertical), constraint graph derived from the existing `ConstrainedLayoutIR`.**
   This is the QUICKSTART's recommendation: it matches the IR's spring-slot /
   vertical-band shape, and the spec's deterministic-output requirement makes a
   global optimization solver expensive to validate (hard to make bit-reproducible)
   and a rule-based fallback brittle. The horizontal pass implemented here
   (`spacing::slot_positions`) is that architecture's first axis. Global
   optimization and rule-based fallback are **rejected**.
3. **Renderer SVG dialect** — N/A here (see `epiphany-render-svg/DECISIONS.md`).
4. **Catalog versioning / 5. Binary Format versioning** — N/A (Agents K, J).

### Local decisions

- **`#![forbid(unsafe_code)]`; sync only; MSRV = workspace 1.77.** Same as every
  implementation crate.
- **Solver version `1` (`ENGRAVER_VERSION`), distinct from the stub's `0`.**
  Chapter 9: within a fixed version, identical input produces identical output.
  The horizontal pass is a pure function of the slot sequence and preferred
  widths, so this holds; a determinism test asserts byte-identical
  `canonical_bytes()` across solves.
- **Well-formedness gate mirrors the stub.** An invalid structure, an unknown
  glyph, a forged catalog identity, or an explicit hard constraint this scaffold
  cannot yet evaluate yields `SolveStatus::InternalError` (diagnostic-only),
  never a panic and never a false `Solved`. When constraints are present the
  scaffold additionally attaches a `SolverWarning` naming the limitation, rather
  than silently ignoring them.
- **Horizontal spacing preserves provenance, glyph identity, bounds, style, and
  layer; it changes only `position`.** It assigns each glyph the `x` of its
  spring slot and keeps its baseline `y` (the vertical pass is future work).

## Pass 12 candidates

See `spec/PASS12_BATCH.md` (rows P12-I1, P12-I2, P12-I3). In brief:

- **P12-I1** — the v0 `to_logical`/`to_constrained` pipeline is a *structural
  placeholder* (each layout object → one arbitrary glyph by `discriminant % N`,
  laid out at `y = 0`), not real notation. Chapter 7 says the *logical* stage has
  "engraving decisions made"; the spec should clarify which engraving decisions
  (glyph-by-duration selection, pitch→staff-position, clef/key/meter/barline
  realization, stems/beams) are core-IR construction versus solver work, so the
  real-notation engraving has a defined home before it is built next phase.
- **P12-I3** — `layout-ir`'s bundled `BRAVURA_METRICS` are *approximations* and
  disagree with the genuine Bravura outlines the renderer now bundles (e.g.
  `timeSig4` vertical registration). Real spacing needs exact metrics; the metrics
  table should be regenerated from the font or reconciled with the outline source.
