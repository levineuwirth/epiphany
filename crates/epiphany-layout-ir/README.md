# epiphany-layout-ir

The Epiphany **layout intermediate representation** and **constraint-solver
interface**, implementing the normative requirements of **Chapter 7** ("Layout
Intermediate Representation") and the interface of **Chapter 9**
("Constraint-Solver Interface") of the core specification
(`spec/core_spec.pdf`). This is Agent E's crate per `spec/QUICKSTART.md` — it
lands last among the implementation crates, building on Agent A's
`epiphany-determinism` and Agent B's `epiphany-core` (and on Agent C's
`OperationKindTag` for the edit-barrier types — see `DECISIONS.md`).

> The IR sits between the score graph and two downstream consumers: the
> constraint solver, which resolves spacing and positioning, and the renderer,
> which produces final visual output. — Chapter 7

The score graph is the canonical truth about the music; this crate is a
downstream **projection** of it. The transformation is a pipeline of four
stages, each with its own type and a deterministic, provenance-preserving
contract for the next.

## What's here

| Area | Items | Spec |
|------|-------|------|
| Stage 1 — logical | `LogicalLayoutIR`, `LayoutRegion`, composite `LayoutObject`, overrides/cross-region objects, `to_logical` | Ch. 7 §"LogicalLayoutIR" |
| Stage 2 — constrained | `ConstrainedLayoutIR`, `SpringSlot`, `LayoutConstraint`, `GlyphObject`, `to_constrained` | Ch. 7 §"ConstrainedLayoutIR" |
| Stage 3 — resolved | `ResolvedLayoutIR`, pages/systems/staves/measures, `ResolvedGlyph` | Ch. 7 §"ResolvedLayoutIR" |
| Stage 4 — render (interface only) | `RenderIR`, `RenderPrimitive`, `RenderIRProducer`, `to_render` | Ch. 7 §"RenderIR" |
| Time axis | canonical `TimeAxisModel` plus dynamic `TimeAxis`, including registered payload preservation | Ch. 7 §"Layout Regions" |
| Provenance | `Provenance`, `LayoutObjectId`, `SynthesisKind`, `stable_layout_id` (a pure function of the source — stable across relayouts) | Ch. 7 §"Provenance" |
| Engraving decisions | `EngravingDecision`/`EngravingDecisionId`/`EngravingDecisionKind`, `DecisionSource` | Ch. 7 §"Engraving Decisions" |
| Vertical bands | `VerticalBand`/`VerticalBandId`/`VerticalBandKind` | Ch. 7 §"Vertical Bands" |
| Incremental cache | `LayoutCache`, `DependencyIndex`, granular stage caches and invalidation | Ch. 7 §"Incremental Layout and Caching" |
| Glyph catalog | `GlyphCatalog` (metric-lookup interface) + in-tree `BravuraCatalog`, `GlyphCatalogIdentity`, `SmuflVersion`, `FontId`, `GlyphMetric`/`GlyphAnchor`, `BRAVURA_METRICS`/`BRAVURA_VERSION`, `metrics_hash_for` (`MUSCFNTM`-tagged) | Ch. 7 §"Glyph Catalog Interface" / §7.3.2 |
| Edit barriers | `EditBarrier`, `BarrierScope`, `BarrierCondition`, `ObjectKind`, `EditContext` (precise scope evaluation), keyed on Agent C's `OperationKindTag` | Ch. 8 §"Edit Barriers" |
| Solver interface | `ConstraintSolver` (`solve`/`solve_incremental`, `Send + Sync`), `SolverConfig`/`SolverBudget`, `SolverState`, `InvalidationSet`, `SolveReport`, `SolveStatus`, `SolverTier`/`SolverVersion`, the v0 `StubSolver` | Ch. 9 |
| Round-trip | `round_trip`, `RoundTripReport`, `laid_out_object_ids` | Ch. 7 (v0 acceptance criterion 6) |

## The stub solver

Per the QUICKSTART, the v0 constraint solver is a **stub**: `StubSolver` returns
`SolveStatus::Solved` with the input geometry **verbatim** (each glyph's resolved
position is exactly its constrained baseline), preserves provenance, and reports
all hard constraints satisfied. The real solver — Cassowary or otherwise — comes
later (Chapter 9 specifies the *interface*, not the algorithm); v0 only needs to
round-trip IR through the solver interface to prove the contracts hold. The one
stub validates the full Chapter 7 §7.3.2 catalog identity and all slot, band,
and geometry cross-references before reporting `Solved`. Explicit constraints
are rejected because this interface-only solver cannot honestly evaluate them.

## The round-trip (v0 acceptance criterion 6)

`round_trip` runs graph → `LogicalLayoutIR` → `ConstrainedLayoutIR` →
stub-solved `ResolvedLayoutIR` → `RenderIR` and asserts the contract every stage
must satisfy:

- the stub solver reports `Solved` with all hard constraints satisfied;
- the **complete** `Provenance` of every object — `source`, `synthesis`,
  `dependencies`, and `stable_id` — survives every stage unchanged;
- no two objects ever share a `stable_id`, so manifestation multiplicity is
  preserved (a source manifested in two regions stays two layout objects);
- the stub solver returns the input geometry verbatim;
- the *set* of score-graph sources recovered from the `RenderIR` is exactly the
  set laid out — a surjection onto graph identity (one source may back several
  manifestations, each with its own stable id).

Agent F's testkit drives this same entry point (`layout_stub::round_trip`) on the
10-measure single-staff hand-off fixture and the rich multi-region generator.

## Algorithmic scope

Per the QUICKSTART ("a prototype baseline, not the product"), this crate
implements the Chapter 7 IR contracts and interface types, not a production
engraving engine. The real spacing and casting-off algorithms, quality-metric
computation, constraint solver, and renderer remain later implementations of
these interfaces.

## Determinism

IR coordinates are single-precision staff spaces (`StaffSpace(f32)`,
Chapter 7 §7.2); the **canonical** `ResolvedLayoutIR` output quantizes them to
the `1/1024` grid at serialization time
(`ResolvedLayoutIR::canonical_bytes`), exactly as Appendix D §"Quantized Layout
Coordinates" prescribes — quantization absorbs all f32 variation below `1/2048`
staff space, so the canonical output is independent of the floating-point
environment. The glyph-catalog identity hashes its consulted metrics (advance,
bounding box, named anchors) under the `MUSCFNTM` domain tag, and the
edit-barrier types carry a canonical encoding with set-valued fields emitted in
canonical byte order (sorted and de-duplicated). See `DECISIONS.md`.

## Tests

`cargo test -p epiphany-layout-ir` covers each module plus the round-trip on
Agent B's `valid_score` / `valid_score_rich` generators. The end-to-end v0
acceptance gate (criterion 6) lives in the testkit.
