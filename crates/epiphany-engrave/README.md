# epiphany-engrave

Agent I's **engraving constraint solver** (spec Chapter 9): turns a
`ConstrainedLayoutIR` into a `ResolvedLayoutIR` with real geometry. It is the
production-side replacement for `epiphany-layout-ir`'s interface-only
`StubSolver`; the two live in separate crates so the spec's core/product boundary
stays sharp.

## Status: `Minimal` tier, with casting-off

- `Engraver` runs a deterministic **horizontal spacing pass** (the first axis of
  the two-pass spring layout): each spring slot is placed left-to-right by a
  collision-aware advance derived from real glyph bearings.
- A **casting-off pass** (Phase 3's layout track) then breaks the spaced line
  into **systems** at measure boundaries (greedy first-fit against a
  `PageGeometry` — default A4 portrait at an 8 mm staff), stacks systems
  vertically at the vertical-band model's inter-system gap, assigns **pages**
  by content height, and populates the real `ResolvedPage`/`ResolvedSystem`
  tree. Every position is baked into a single y-up world frame (pages stacked
  vertically), so the SVG renderer and hit-testing consume the flat
  glyph/stroke lists unchanged.
- The IR's declared constraints are **evaluated** — geometric families against
  the pre-casting spaced frame, break constraints against the final break
  structure (hard breaks are always honoured; a pathological soft break is
  skipped and recorded as an `IrOverride` decision). Chosen breaks are recorded
  as `EngravingDecision`s with `SynthesisKind::EngravedBreak` targets,
  attributed to the user override that requested them when one did.
- It reports `SolverTier::Minimal`: hard constraints (break family included)
  satisfied, **no optimality claim** — the quality-metric vector stays the
  honest all-worst placeholder until the Quality Metric Catalog lands.

Deferred: the vertical soft-spring solve, per-system justification/stretch,
optimal break search, widow/orphan control, and casting-off caching. See
`DECISIONS.md`.

```rust
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{ConstraintSolver, SolverConfig};

let report = Engraver::default().solve(&constrained_ir, &SolverConfig::default());
assert!(report.satisfied_hard_constraints);
let resolved = report.layout; // real pages/systems; hand to epiphany-render-svg
```
