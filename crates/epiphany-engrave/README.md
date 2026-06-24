# epiphany-engrave

Agent I's **engraving constraint solver** (spec Chapter 9): turns a
`ConstrainedLayoutIR` into a `ResolvedLayoutIR` with real geometry. It is the
production-side replacement for `epiphany-layout-ir`'s interface-only
`StubSolver`; the two live in separate crates so the spec's core/product boundary
stays sharp.

## Status: honest scaffold (renderer-against-stub phase)

Per the QUICKSTART development pattern, Agent I builds the SVG renderer
([`epiphany-render-svg`](../epiphany-render-svg)) against the **stub solver**
first, then grows this crate into the real two-pass spring solver. This commit is
the first increment:

- `Engraver` runs a deterministic **horizontal spacing pass** (the first axis of
  the planned two-pass spring layout): each spring slot is placed left-to-right by
  its preferred width instead of being echoed verbatim.
- It honestly reports `SolverTier::Stub` — it does not yet evaluate the IR's
  declared hard constraints or compute quality metrics, so it has not earned
  `Minimal`. It is promoted to `Minimal` in the change that lands real constraint
  satisfaction.

The vertical spring pass, soft-constraint solve, hard-constraint evaluation, and
the quality-metric vector are the next-phase / Phase-3 work. See `DECISIONS.md`.

```rust
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{ConstraintSolver, SolverConfig};

let report = Engraver.solve(&constrained_ir, &SolverConfig::default());
assert!(report.satisfied_hard_constraints); // for constraint-free stub-pipeline input
let resolved = report.layout;               // hand to epiphany-render-svg
```
