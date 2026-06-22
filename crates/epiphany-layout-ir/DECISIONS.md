# epiphany-layout-ir — decisions and Pass 11 candidates

This file records (a) the implementation decisions the QUICKSTART asked each
agent to make once and document, and (b) the ambiguities discovered while
building `epiphany-layout-ir`, batched as **Pass 11 candidates** for the spec
rather than improvised in code (QUICKSTART, Process notes: *"Ambiguities go into
a batch, not into code … Don't open Pass 11 until you have at least three such
items batched."*).

> **RATIFIED (Pass 11, 2026-06-21).** layout P11-2 (`LayoutObjectId` derivation)
> is ratified into `core_spec.tex` §"Provenance"
> (`req:layoutir:object-id-derivation`): a `MUSCLOID`-tagged derivation keying
> multiply-manifested objects on `(source, region)` and synthesized objects on
> `(source, synthesis_kind, stable_semantic_instance_key)`. Layout ids are
> non-canonical, so this is flagged for Track A (solver/renderer). layout P11-1
> (layout→ops dependency) stays a crate-topology call for the G–K re-cut. See
> `spec/PASS11_RATIFICATION_LOG.md`.

## Scope

The crate implements the Chapter 7 interface surface: all four stages, the
logical composite taxonomy, overrides and cross-region objects, time-axis
payloads and trait, spring slots and constraints, vertical bands, resolved
pages/systems, render configuration, glyph-catalog identity, and incremental
cache/dependency types. Per the QUICKSTART's prototype framing, the algorithms
behind those interfaces remain simple; the constraint solver still returns
validated input geometry verbatim and performs no production engraving,
casting-off, quality optimization, or rendering.

The round-trip's strict stage-equality assertion (the full `Provenance` of every
object is preserved object-for-object) reflects this prototype's **1:1**
projection — one layout object per laid-out score-graph object. A later stage
that flattens a composite object into multiple glyphs would relax that assertion
to source-coverage (every glyph's source is a laid-out object, every laid-out
object is covered); the provenance-preservation contract itself is unchanged.

## Implementation decisions (QUICKSTART "Decisions you'll need to make")

1. **Replica ID entropy / 2. event-arena storage / 3. chunk store** — N/A to
   this crate (Agents B and D).
4. **Async or sync — sync only.** No async traits anywhere; `#![forbid(unsafe_code)]`.
5. **MSRV — workspace 1.77.** No exotic features.

### Local decisions

- **f32 IR coordinates, quantized at serialization.** IR coordinates are
  single-precision staff spaces (`StaffSpace(f32)`, Chapter 7 §7.2: "Single-
  precision floating point MUST be used for IR coordinates"). Quantization to the
  canonical `1/1024` grid happens **only when serializing** canonical
  `ResolvedLayoutIR` output — exactly as Appendix D §"Quantized Layout
  Coordinates" prescribes: "Internal solvers MAY use floating point during
  computation; canonical serialization rounds to `QuantizedCoord`."
  [`ResolvedLayoutIR::canonical_bytes`] is that boundary; it round-trips f32
  jitter below `1/2048` staff space to identical bytes. (An earlier draft of this
  crate quantized *throughout* the pipeline; that contradicted the explicit
  Chapter 7 f32 requirement and is corrected here.)

- **Depend on `epiphany-ops` for `OperationKindTag`.** The QUICKSTART lists
  Agent E's dependencies as "A and B," but also assigns Agent E the *edit-barrier
  types with `OperationKindTag`-based `prohibited_operation_kinds`*.
  `OperationKindTag` is Agent C's canonical discriminator type (Chapter 6);
  reproducing it here would create a second definition that could drift. We
  therefore take a single, narrow dependency on `epiphany-ops` for that one type.
  This is sound: `epiphany-ops` does not depend on this crate (no cycle), and
  Agent E lands after Agent C. See Pass 11 candidate 1.

- **`ObjectKind` for edit barriers is the `TypedObjectId` discriminant.** The
  spec's `EditBarrier.affected_object_kinds: Vec<ObjectKind>` needs a
  *score-graph object class* key. The `ObjectKind` in `epiphany-ops` is a narrow
  *system-counter-collision* kind (Voice/Pitch/Registered), semantically
  unrelated, so we define a local `ObjectKind(pub u16)` over the `TypedObjectId`
  discriminant — the natural object-class key in this codebase.

- **Edit-barrier scopes and conditions are evaluated precisely.** A
  `Region`/`StaffInstance`/`AnalysisLayer`/`PitchSpace` barrier prohibits only
  objects within that scope; the editor (which holds the score) supplies the
  candidate object's structural location via `EditContext`. The *known*
  conditions `ObjectExists` and `ObjectHasExtensionData` are evaluated via an
  `EditOracle` the editor implements, **not** hardcoded to `true`. Only genuinely
  unknown narrowing — a `Registered` scope or an unknown `Registered` condition —
  is treated conservatively (as matching), per Chapter 8 §"Behavior Under Unknown
  Extensions". This avoids over-prohibiting edits to objects demonstrably outside
  a known scope or to objects a known condition excludes.

- **`stable_layout_id` and the engraving-decision id borrow a domain tag.** A
  layout object's stable id is `trunc128(BLAKE3(source.canonical_bytes()))` — a
  pure function of its source, so it is invariant under insertion/removal/
  reordering of other objects (Chapter 7 §"Provenance"). It is not domain-
  separated, and the engraving-decision id borrows the `MUSCCONF` tag with a
  literal `engraving-decision` type prefix, because the frozen determinism crate
  (Agent A) defines no layout-object domain tag. See Pass 11 candidate 3.

- **Repeated manifestations get per-`(source, region)` ids.** A score-graph
  object manifested within a region is laid out **per manifestation**: its stable
  id derives from `(source, region)` via `manifestation_layout_id` /
  `Provenance::manifested`. A staff manifested in two time-disjoint regions
  (Chapter 5 §"Region Overlap and Concurrency") therefore yields *two* distinct
  layout objects — both visual staves are preserved, neither dropped — and the
  ids do not collide. The id is still independent of traversal *position* (it
  depends on region *identity*, not order), so it stays stable across relayouts.
  Score-level cross-cutting objects, which have a single manifestation, keep a
  source-only id (`Provenance::projected`).

- **`GlyphObjectId`/`VerticalBandId` reuse the provenance hash.** A glyph's
  `GlyphObjectId` is its provenance `stable_id` (already manifestation-aware); a
  staff band's `VerticalBandId` is the staff *layout object's* manifestation id
  (`VerticalBand::staff_manifestation`), so two manifestations of a staff get two
  distinct bands. Both are stable across relayouts.

- **Bundled Bravura metrics are a representative slice.** `BRAVURA_METRICS` holds
  ~two dozen real-Bravura glyphs (noteheads, clefs, accidentals, rests, flags,
  time signatures, barlines, dynamics) with advance, bounding box, and named
  anchors, in `1/1024`-staff-space units, tracking the `BRAVURA_VERSION` release.
  Enough to exercise the `MUSCFNTM` catalog identity and the `GlyphCatalog`
  metric-lookup interface end to end without shipping a font file; render-data
  lookup and a full catalog are out-of-core concerns (Chapter 7 §"Glyph Catalog
  Interface").

- **Glyph identity flows to the resolved/render stages.** `ResolvedGlyph` and
  `RenderPrimitive` carry an owned-or-borrowed `GlyphReference`, so
  the renderer knows *what symbol to draw* and the canonical encoding is
  **injective in glyph identity** — swapping two glyphs' names (even with the
  consulted-name *set*, and so the metrics hash, unchanged) changes the bytes.

- **Comprehensive, rejecting canonical encoding for `ResolvedLayoutIR`.** The
  canonical output (`ResolvedLayoutIR::canonical_bytes`, via `CanonicalEncode`)
  covers the *full* resolved layout — every glyph's provenance (source, stable
  id, synthesis kind, sorted/deduped dependencies), **glyph name**, and quantized
  position, every engraving decision, and the complete catalog identity — so any
  change that distinguishes two layouts (a swapped glyph, an altered engraving
  decision, a different manifestation id, a different font version) changes the
  bytes. A non-finite or out-of-range coordinate is **rejected** with a panic
  (faulting in every build), never normalized to the origin (Appendix D: invalid
  geometry is rejected).

- **Each glyph is routed to *its own staff's* band.** `GlyphObject.vertical_band`
  (Chapter 7 §"Glyph-Level Objects") points at the band of the staff the glyph
  belongs to — `LayoutObject` carries that staff association, so a region
  manifesting two staves gets a staff band per staff with each glyph in exactly
  one (no cross-staff contamination). Region-level glyphs (the region object,
  cross-cutting, free-graphic) go to a margin band; multi-staff regions also carry
  empty `InterStaffGap` spring bands between staves. Staff-band ids are the staff
  layout object's manifestation id (distinct per region).

- **Free-graphic and hybrid graphic objects are projected.** `to_logical` and
  `laid_out_object_ids` project `region.content.graphic_objects()` (Chapter 5
  §"Graphic Content"), so free-graphic and hybrid regions are not silently
  dropped.

- **Synthesized-object ids include kind and a stable semantic key.**
  `Provenance::synthesized(source, kind, instance_key, deps)` derives its id
  from `(source, synthesis_kind, instance_key)`. The key describes the object's
  role and never traversal order, so insertion or reordering cannot renumber
  existing synthesized objects.

- **Glyph-catalog interface: `Send + Sync`, metrics + render data, `SemVer`
  version, anchors as a map.** `GlyphCatalog` is `Send + Sync` (shareable across
  parallel re-engraving) with both `metrics` and `render_data`. The in-tree
  Bravura catalog bundles metrics but **no** outlines/bitmaps, so its
  `render_data` honestly returns `None` (reporting `Some` would claim data that
  does not exist). `font_version` is `Option<SemVer>`, set to the latest stable
  Bravura release (`1.38.0`). Glyph anchors are a *map* keyed by name: the catalog
  hash sorts them by name and **rejects** a duplicate name (a panic), so the hash
  never depends on anchor slice order (Appendix D §"Ordered Iteration").
  every catalog method, including `identity`, is object-safe; owned font,
  glyph, and anchor names support a runtime-loaded `dyn GlyphCatalog` without
  leaking strings.

- **Chapter 9 interface in full shape; no quality-metric computation.** The
  `ConstraintSolver` interface is implemented as the
  spec defines it: `Send + Sync`, `solve`/`solve_incremental`, a `SolverConfig`
  with `profile`/`budget`/`tie_breaking`, a `SolveReport` with
  `unsatisfied_constraints`/`warnings`/`metric_vector`/`budget_used`/`state` (and
  the warning kinds, including `QualityFloorApproached`/`ExtensionWarning`), and
  an `InvalidationSet` with `slots`/`bands`/`constraints`/`glyphs`. The render
  boundary's `RenderIRProducer::produce(resolved, scale, config)` takes the
  spec's `ScaleContext`/`RenderConfiguration`. The quality-metric/tie-breaking
  *types* exist; what the QUICKSTART defers is normalization computation. The
  exact non-optional interface is preserved: the stub reports the
  non-conformance `SolverTier::Stub` rung (M5 follow-up — *not* `Minimal`, since a
  passthrough that evaluates no constraints and computes no quality metrics must
  not claim the lowest conformance tier; `Stub` orders below `Minimal`), an
  all-worst `QualityMetricVector`, and rejects explicit constraints it cannot
  evaluate rather than claiming them satisfied.

- **Constraint references are validated (M5 follow-up).**
  `ConstrainedLayoutIR::validate()` now also checks the `LayoutConstraint`
  vector: `NoCollision`/`Align`/`PositionWithin` must name glyphs in the set,
  `SystemBreakAt`/`PageBreakAt` must name existing spring slots, and a
  `PositionWithin` region must be finite/non-negative. Dangling constraint
  references are rejected (`UnknownConstraintGlyph`/`UnknownConstraintSlot`/
  `InvalidConstraintRegion`) rather than silently accepted. `Registered`
  (extension) constraints stay opaque/conservative. Score-graph *source*
  validation (that a `Provenance::source` names a real graph object) still
  belongs at the `to_logical` boundary, which holds the `Score`.

- **`ScoreVersion` is content-sensitive (M5 follow-up).** It is now derived from
  the whole score's canonical bytes (Agent B's whole-score codec) rather than the
  layout projection's object identities, so a pure content edit (a respelling, a
  duration change) that changes no identifier still changes the version —
  required for correct incremental-layout cache invalidation (Chapter 7
  §"Incremental Layout").

- **The time axis has real behavior (M5 follow-up).** Previously the
  `TimeAxisModel` carried bare `Vec<SpringSlotId>` and `project`/`affected_slots`
  ignored their arguments (returning the first slot / all slots) — inert payload.
  Each axis now holds ordered `SlotPlacement { time, slot }` entries:
  `project(time)` returns the slot *covering* a time (the greatest placement at or
  before it), `affected_slots(range)` returns the slots in a half-open time range,
  and `slots()` lists them in time order. The spacing stage (`to_constrained`)
  populates each region's axis from its resolved spring slots
  (`TimeAxisModel::with_placements`), and the populated axis is carried on
  `ConstrainedLayoutRegion`, so the axis is a real, consumed artifact rather than
  an empty placeholder. (The slot *times* are still the prototype's wall-clock
  spacing columns; mapping a metric region's measure/beat grid to musical times
  is the next layer, but the axis machinery now genuinely consumes whatever times
  the spacing assigns.)

## Pass 11 candidates (ambiguities for the spec, not resolved in code)

1. **Agent E's stated dependency set vs. the edit-barrier types.** The QUICKSTART
   says Agent E "depends on A and B," but assigns it the edit-barrier types,
   which reference Agent C's `OperationKindTag`, and the spec's `EditBarrier`
   additionally references `ObjectKind` and `ExtensionId` (Chapter 8, the
   bundle's chapter). The dependency note, the type assignment, and the type's
   chapter home are in tension; the spec should either bless a layout→ops
   dependency for the discriminator type or relocate the edit-barrier types.

2. **Provenance / layout-object id derivation is unspecified.** Chapter 7
   declares `LayoutObjectId(pub u128)` and requires stability across relayouts but
   specifies neither the derivation, whether it is domain-separated (Appendix D
   §"Domain-Separated Preimages" would suggest a dedicated `MUSC*` tag), how a
   multiply-manifested object (a staff in two regions) is identified — v0 keys it
   on `(source, region)` — nor how synthesized objects are keyed — v0 uses
   `(source, synthesis_kind, stable_semantic_instance_key)`. The spec should pin
   the derivation, manifestation-context key, and synthesized-object key, and
   register a layout domain tag if separation is required.
