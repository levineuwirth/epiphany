# epiphany-layout-ir — decisions and Pass 11 candidates

This file records (a) the implementation decisions the QUICKSTART asked each
agent to make once and document, and (b) the ambiguities discovered while
building `epiphany-layout-ir`, batched as **Pass 11 candidates** for the spec
rather than improvised in code (QUICKSTART, Process notes: *"Ambiguities go into
a batch, not into code … Don't open Pass 11 until you have at least three such
items batched."*).

> **RATIFIED (Pass 11, 2026-06-21); WIRED (Pass 12, P12-I2).** layout P11-2
> (`LayoutObjectId` derivation) is ratified into `core_spec.tex` §"Provenance"
> (`req:layoutir:object-id-derivation`): the spec **pins** a `MUSCLOID`-tagged
> derivation keying multiply-manifested objects on `(source, region)` and
> synthesized objects on `(source, synthesis_kind, stable_semantic_instance_key)`.
> This is now **wired**: `epiphany-determinism` exposes the reserved built-in
> `DomainTag::LAYOUT_OBJECT_ID` (`MUSCLOID`), and all three derivations in
> `provenance.rs` route through it (`stable_layout_id`, `manifestation_layout_id`,
> `synthesized_layout_id` — the last no longer borrows `MUSCCONF`). Layout ids stay
> non-canonical (not document state, in no content hash), so realizing the
> derivation changed layout-id *values* (and the `data-prov` hex in the SVG goldens)
> but no durable or interchanged artifact. layout P11-1 (layout→ops dependency)
> stays a crate-topology call for the G–K re-cut. See
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

- **`stable_layout_id` and the engraving-decision id are `MUSCLOID`-tagged
  (P12-I2 wired).** A layout object's stable id is a pure function of its source
  (and, for manifestations/synthesized objects, the region or the synthesis
  kind+instance key), so it is invariant under insertion/removal/reordering of
  other objects (Chapter 7 §"Provenance"). Both ids are now domain-separated under
  the reserved built-in `DomainTag::LAYOUT_OBJECT_ID` (`MUSCLOID`), the spec's
  non-canonical layout namespace (`req:layoutir:object-id-derivation`); the
  engraving-decision id keeps its literal `engraving-decision` discriminator prefix
  so it cannot alias a layout-object id within that namespace. Neither borrows
  `MUSCCONF` any longer. See the header note for the realization details.

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

- **The render-to-hit-test contract lives at the RenderIR boundary, in world
  coordinates.** `RenderIR::hit_test_map` (`hittest.rs`) turns the provenance the
  spec calls "the basis of hit-testing, selection, and back-reference navigation"
  (Chapter 7 §"RenderIR") into a structured map an editor can use directly: one
  `HitRegion` per glyph/stroke carrying the full chain — rendered primitive →
  layout object (`stable_id`) → score object (`source`) — plus a selectable
  `HitShape` (a glyph's placed `bounding_box`, or a stroke's segment + half-width)
  with `contains`/`aabb` and `hit`/`within` queries. Two deliberate boundaries:
  (1) shapes are in **staff-space world coords** (the same frame as
  `RenderPrimitive.position`, before any renderer's world→screen transform), so the
  contract is renderer-independent and a GUI applies the inverse of the same
  transform its renderer uses; (2) a glyph's region is its **IR `bounding_box`**
  (the boundary's granularity, which I-4a made contain the drawn ink), not the
  render-only outline. Tested against the real pipeline, not guessed by the GUI.

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
  does not exist). `font_version` is `Option<SemVer>`, set to the SHA-pinned
  `bravura-1.392` release the in-tree metrics are extracted from — the same font
  the renderer's outlines come from, so reserved metrics and drawn ink agree. The
  font declares a single decimal version (`"Version 1.392"`), recorded verbatim as
  `SemVer { major: 1, minor: 392, patch: 0 }`; see `BRAVURA_VERSION` for the
  canonical mapping rule. Glyph anchors are a *map* keyed by name: the catalog
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

- **`ConstraintStrength` is attached by rule, not by widening the IR.** Chapter 9
  §"Strength Levels" defines `ConstraintStrength { Required, Preferred { weight } }`,
  but the spec's `LayoutConstraint` enum carries no strength field and the
  "normalized form" the solver consumes never says how strength attaches to a
  constraint instance (a genuine gap — see Pass 12 candidates below). Rather than
  invent an IR shape the spec doesn't have, `LayoutConstraint::strength()` derives
  strength from the constraint's own shape: a break's `BreakKind` *is* its
  strength (`Hard` → `Required`, `Soft` → `Preferred { weight: 1.0 }`), the
  geometric constraints (no-collision / alignment / containment) are `Required`,
  and a `Registered` extension constraint is conservatively `Required` — an
  obligation a solver cannot verify must never be silently demoted (Chapter 9:
  a solver MUST NOT treat `Required` as `Preferred`).

- **The spacing pass emits real constraints (Chapter 7 pipeline: "Build collision
  constraints").** `try_to_constrained` now populates `constraints`, per region and
  in a deterministic order: (1) **NoCollision** chains over *successive notehead
  columns* within each staff — adjacent pairs in (column x, glyph id) order, linear
  in the noteheads, never the O(n²) closure; chord members share a slot (a second
  or unison may genuinely overlap by design), so only cross-column neighbours carry
  the obligation. These hold under both v0 solvers: the source layout separates
  columns collision-free and the engraver's collision-aware advance keeps
  successive columns separated after its remap. (2) **PositionWithin** per glyph
  against its region's envelope: the vertical extent is the exact envelope of the
  region's glyph boxes (both v0 solvers preserve glyph `y` verbatim, so this is a
  genuine obligation a future vertical pass must renegotiate); the horizontal span
  is the open v0 canvas (`POSITION_WITHIN_X_REACH`) because v0 does no casting-off
  — a region imposes no honest horizontal bound. (3) **Soft break constraints**
  projected from the logical stage's break overrides, on the spring slot carrying
  the break anchor's onset (the barline column at that time when one exists, else
  the note column); an anchor no realized column represents — an event/measure
  outside the region, a measure *end*, a region edge — is skipped silently, since
  there is no slot for a solver to break at. The four SVG golden snapshots'
  `hard_constraint_count` moved off 0 accordingly; the golden SVG *bytes* are
  unchanged (emission does not touch geometry).

- **The stub stays honest — and renderable — under declared constraints.** The
  old `StubSolver` flipped to `InternalError` whenever a constraint was present,
  which conflated "constraints I did not evaluate" with "a malformed input". Now:
  geometry still passes through verbatim, `satisfied_hard_constraints` is `false`
  (nothing was checked), a warning names the gap, and the status is
  `SolvedWithWarnings` — the closest *non-claiming* renderable status, since
  Chapter 9 defines no status for "renderable, constraints unevaluated" (a Pass 12
  candidate below). Editors gate on `SolveStatus::is_renderable()`, so stub-driven
  pipelines (editor-core sessions, the edit-loop harness, the acceptance goldens)
  keep working; the round-trip harness asserts the stub claims satisfaction
  exactly when the problem is constraint-free.

- **Break overrides carry their anchor; projected from the graph's break lists.**
  Per the updated Chapter 7 §"Engraving Overrides", `OverrideKind::SystemBreak` /
  `PageBreak` now carry `anchor: TimeAnchor` — a break addresses a *position*,
  while the override's `ScoreGraph` target names the owning region. `to_logical`
  projects each region's authoritative `user_system_breaks` / `user_page_breaks`
  (Chapter 5) into `Soft`, `Internal`-origin overrides (authorship lives in the
  op log until P11-C8), ordered by (region id, kind discriminant, anchor canonical
  bytes), each with a paired `EngravingDecision` under
  `DecisionSource::UserOverride(id)` (Chapter 7 §"Override Resolution" MUST). The
  override id reuses the `MUSCLOID` derivation with a literal `engraving-override`
  prefix (mirroring the decision id), keyed on (region, kind discriminant, anchor
  canonical bytes). The variant-shape change is byte-visible only in memory:
  overrides appear in **no** codec today (the layout IR chunks cache no override
  records), so no stored or interchanged artifact changes — the stale
  "graph exposes no override registry" comment this replaces predated the graph's
  break lists.

- **Edit-barrier decode mirrors + the manifest blob codec (PROVISIONAL byte
  form, Push 3).** The barrier tree was encode-only; `barrier.rs` now carries
  the exact inverse and the codec for the two opaque manifest fields the bundle
  preserves verbatim (`ExtensionDeclaration.edit_barriers` /
  `.affected_object_kinds` — the bundle stays semantics-free; no bundle change).
  The spec defines **no normative byte form** for `EditBarrier`/`BarrierScope`/
  `BarrierCondition`, so this is a provisional canonical encoding on the
  established pattern (define concretely, golden-lock, submit to the Binary
  Format companion): both blobs are canonical **sets** in the crate's existing
  `push_set` framing — `u64` LE count, then per element a `u64` LE length
  prefix and the element's canonical bytes, elements strictly ascending
  byte-lexicographic, duplicates removed — an `edit_barriers` element being an
  `EditBarrier`'s canonical bytes (scope, affected-kind set, prohibited-tag
  set, condition, in that order), an `affected_object_kinds` element being the
  kind's 2 LE bytes. Golden literal-byte tests
  (`edit_barriers_blob_bytes_are_golden`,
  `affected_object_kinds_blob_bytes_are_golden`) lock the layout; the testkit
  adds a generator-driven round-trip gate. Decode discipline is
  reject-never-normalize (`BarrierDecodeError`): unknown scope/condition/
  operation-kind discriminants, unsorted or duplicated set elements, non-NFC
  pitch-space text (`PitchSpaceId::new` would re-spell it, so the bytes are
  non-canonical), truncation, and trailing bytes are all typed errors, and a
  decoded barrier must re-encode byte-identically. Two deliberate choices:
  (1) **`ObjectKind` decodes any `u16`** — the payload is an open discriminant
  space (a future core kind or an extension-registered kind is a *value*, not
  a decode branch), so there is nothing to reject without breaking append-only
  forward compatibility; (2) **`MAX_CONDITION_DEPTH = 64`** bounds the
  recursive `BarrierCondition` decode — the spec places no bound on the tree,
  a decoder needs one against adversarial bytes, and 64 is far past any real
  barrier (spec examples are depth 1–2). Both are named Binary Format
  companion candidates. Evaluation wiring (the §"Behavior Under Unknown
  Extensions" MUST) lives in epiphany-editor-core, which decodes injected
  declarations and gates `apply`/`apply_transaction` through
  `EditBarrier::prohibits_edit`.

  > **Ratified (2026-07-02):** `spec/binary_format.tex` v0.1.0 Chapter 8
  > ratifies the blob byte form (P12-E1, `req:binfmt:ext-blobs`), pins
  > `MAX_CONDITION_DEPTH = 64` as the normative recursion bound (P12-E2,
  > `req:binfmt:condition-depth`), and adopts the open-value `ObjectKind`
  > decode stance (P12-E3, `req:binfmt:object-kind-open`).

- **Casting-off support surface (2026-07, the engrave casting-off slice).**
  Three small, non-canonical additions made for `epiphany-engrave`'s
  casting-off pass, kept here because they are IR-shape/contract concerns:
  1. **`ConstrainedLayoutIR.break_origins` (`BreakOrigin`)** — the spec's
     `LayoutConstraint` enum is normative and carries no origin field, but a
     casting-off solver that honours a user break must record the decision with
     `DecisionSource::UserOverride(id)` (Chapter 7 §"Note Layout" /
     §"Engraving Overrides"), and the override id would otherwise be lost at the
     logical→constrained boundary. The projection therefore records the
     attribution *alongside* the constraint list (slot, break class, override
     id) rather than widening the normative enum. Non-canonical, like every
     constrained-stage value.
  2. **`continuation_instance_key`** — the stable
     `SynthesisInstanceKey` derivation for engraver-synthesized *continuations*
     of an existing object (the per-system segments a casting-off break cuts a
     region-spanning staff line into): keyed on the original object's stable id
     plus the 1-based continuation ordinal, hashed under `MUSCLOID` domain
     separation so segments of different lines cannot collide for one
     `(source, kind)` pair.
  3. **Round-trip contract: solver-synthesized additions.** `round_trip_with`
     previously asserted the constrained→resolved provenance maps *equal*; a
     casting-off solver legitimately synthesizes new objects (staff-line
     continuation segments), which Chapter 7 §"Provenance" explicitly allows
     for engraver-synthesized objects. The contract is now: every constrained
     object survives with its exact provenance (containment, not equality);
     every solver addition must declare a `SynthesisKind` and derive from an
     already-laid-out source (so the recovered source set is unchanged); the
     `Stub` tier must add nothing.

## Pass 12 candidates (ambiguities for the spec, not resolved in code)

1. **Strength attachment to constraint instances.** Chapter 9 §"Strength Levels"
   defines `ConstraintStrength`, and §"Constraint Families" says the solver
   consumes constraints "in normalized form" — but the normalized form is never
   specified, and Chapter 7's `LayoutConstraint` enum has no strength field, so
   there is no normative channel by which a constraint instance carries its
   strength. v0 attaches strength by rule (`LayoutConstraint::strength()`, above);
   the spec should either bless that rule (breaks strength = `BreakKind`, all
   other core families `Required`, extensions conservative) or add an explicit
   strength/weight field to the normalized constraint record.

2. **No renderable status for "constraints not evaluated".** A `Stub`-tier
   (below-conformance) solver that preserves geometry but evaluates nothing has
   no honest `SolveStatus`: every renderable status is documented as "all hard
   constraints satisfied", and the failure statuses mark the layout
   diagnostic-only, which a verbatim passthrough is not. v0 uses
   `SolvedWithWarnings` with `satisfied_hard_constraints == false` and a warning;
   the spec should either define the report shape for a non-evaluating tier or
   state that `SolvedWithWarnings` + `satisfied_hard_constraints == false` is the
   sanctioned encoding.

## Pass 11 candidates (ambiguities for the spec, not resolved in code)

1. **Agent E's stated dependency set vs. the edit-barrier types.** The QUICKSTART
   says Agent E "depends on A and B," but assigns it the edit-barrier types,
   which reference Agent C's `OperationKindTag`, and the spec's `EditBarrier`
   additionally references `ObjectKind` and `ExtensionId` (Chapter 8, the
   bundle's chapter). The dependency note, the type assignment, and the type's
   chapter home are in tension; the spec should either bless a layout→ops
   dependency for the discriminator type or relocate the edit-barrier types.

2. **Provenance / layout-object id derivation. — RESOLVED (ratified Pass 11;
   wired P12-I2).** Chapter 7 originally declared `LayoutObjectId(pub u128)` and
   required stability across relayouts without specifying the derivation, its
   domain separation, or how multiply-manifested / synthesized objects are keyed.
   Pass 11 ratified the `MUSCLOID`-tagged derivation
   (`req:layoutir:object-id-derivation`) — single objects keyed on
   `source.canonical_bytes()`, multiply-manifested on `(source, region)`,
   synthesized on `(source, synthesis_kind, stable_semantic_instance_key)` — and
   P12-I2 wired it: `epiphany-determinism` reserves the built-in
   `DomainTag::LAYOUT_OBJECT_ID` and `provenance.rs` (and the engraving-decision
   id) route through it. See the ratified-block note at the top of this file.

## Quality Metric Catalog constants (`src/quality.rs`, 2026-07)

**Decision: the catalog's normative constants live in this crate, as a pure
transcription.** The Quality Metric Catalog companion (v0.2.0) pins the nine
axes' normalization anchors (`R_worst`), the clamped-linear normalization form
`n = min(1, raw / R_worst)`, the Minimal/Standard threshold table, the
`QualityFloorApproached` warning fraction (0.8), and the tier/profile →
threshold-column mappings (Minimal has its own column; Standard and Advanced
use the Standard column; profiles Draft → Minimal column, Standard and
Publication → Standard column, Standard the default). Both consumers — the
`epiphany-engrave` solver (computing vectors and floor diagnostics) and the
`epiphany-testkit` reference-suite harness (asserting per-tier thresholds) —
need the same numbers, and this crate is the only one both already depend on,
so the constants live here (`quality.rs`) with doc comments citing the
companion by chapter/section. **Every value is transcribed, none invented**;
a change to any of them is a catalog revision first, mirrored here. The
module is additive: no canonical encoding is touched (metric values remain
diagnostic-only, structurally outside `ResolvedLayoutIR` — the catalog's own
requirement), and the `StubSolver` still computes nothing and keeps its
all-worst `unmeasured()` vector, which a transcription test pins as excluded
by the Minimal column ("measuring is part of the Minimal claim"). The catalog
also blesses the existing `TieBreakingWeights::default()` (all 1.0) as the
normative defaults — pinned by test rather than re-declared.

## Break-origin attribution and system-continuation synthesis (Pass 12 P12-I9/I10, ratified)

Two long-standing layout-ir dispositions were ratified into the core spec by the
schema-major-1 track's Phase F (2026-07-06; `spec/PASS12_RATIFICATION_LOG.md`,
schema-major-1 tranche):

- **P12-I9 — break-override attribution via a sidecar.** Honouring a user break
  carries `DecisionSource::UserOverride(id)`, but a normalized break *constraint*
  (`SystemBreakAt`/`PageBreakAt`) carries no override identity. Attribution is
  threaded through a `ConstrainedLayoutIR.break_origins` sidecar populated by
  `to_constrained`; the normalized constraint record is deliberately **not**
  widened (attribution is a projection concern, not a solver input). Ratified as
  core spec `req:layoutir:break-origin-attribution`.
- **P12-I10 — system-continuation synthesis.** A stroke spanning a system
  boundary is split; the post-first segments are synthesized under
  `SynthesisKind::Registered(SYSTEM_CONTINUATION_SYNTHESIS)` with a
  `(original, ordinal)` `stable_semantic_instance_key`. Since `LayoutObjectId`s
  are non-canonical and re-derived per layout, the key need only be stable within
  a layout. Ratified as core spec `req:layoutir:continuation-synthesis`.

## Pass 12 G-pass (2026-07-07): I4/I5/I6 are ratified

Dispositions in `spec/PASS12_RATIFICATION_LOG.md` ("G-pass tranche"), all
adopt-as-implemented; these are deliberate Standard-tier design inputs.
**I4** strength is kind-determined (`req:solver:kind-strength`): no instance
strength field; breaks by `BreakKind` (Hard→Required, Soft→Preferred{1.0}),
other core families Required, `Registered` conservative Required; future
constraint families declare their strength in their normative definitions.
**I5** the stub's constraints-present-but-unevaluated report
(`SolvedWithWarnings` + `satisfied_hard_constraints == false` + warning) is
sanctioned (`req:solver:subconformant-report`). **I6** the implemented
emission set (successive-notehead no-collision chains + per-glyph containment
+ user-break constraints) is the normative Minimal-tier floor
(`req:layoutir:constraint-floor`).

## Repeat barlines and volta brackets (schema major 2, E1, 2026-07-07)

The first repeat-structure ink (Chapter 5 `RepeatStructure` / `RepeatKind` /
`Volta`, ratified by the major-2 Phase A). Rendering is spec-unconstrained
(Ch7's `BarLine` payload is undefined and voltas have no layout variant), so
these are E1 implementation decisions for the Phase-F ratification pass:

- **Kind → ink mapping.** `SimpleRepeat` and `Volta` draw repeat barlines at
  their boundaries; `DaCapo`/`DalSegno` draw **no Minimal-tier ink** (segno /
  coda / instruction marks need a text primitive — a later tranche) but keep
  their traced anchors. Volta brackets draw for the `voltas` list of **any**
  kind.
- **Morph, standalone, or dots.** A boundary whose column carries a measure's
  own barline **morphs** that barline into the precomposed SMuFL sign
  (`repeatLeft` / `repeatRight` / `repeatRightLeft` when an end meets a
  start) — a *name* change only: the measure's exact provenance is preserved
  verbatim because the round-trip provenance floor compares it exactly;
  repeat-edit invalidation is carried by the `ScoreVersion` (v0 relayouts
  wholesale), and an incremental tranche would add the dependency at the
  logical stage where dependencies are established. A boundary with no
  coinciding measure barline stands alone as a repeat-synthesized sign at its
  own barline-role column (`REPEAT_BARLINE_SYNTHESIS`; one per (column,
  staff); coinciding structures merge into one sign whose synthesis owner is
  the smallest `(structure id, boundary site)` — a **semantic** instance key,
  `(site << 32) | staff index`, stable under unrelated edits where a
  positional column rank would re-derive). The **region-closing column**: an
  end repeat there adds the `repeatDots` pair beside a staff's final barline
  (the final barline never morphs, keeping the casting-off solver's
  final-barline classification truthful), or draws the full end sign on a
  staff whose run continues (no final barline there); a *start* repeat at the
  region close draws nothing on any staff — a sign after the close would
  misstate the structure.
- **Source-geometry clearance.** An end-facing sign's ink reaches ~1.1–1.3
  staff spaces left of its column (its heavy line right-aligns to the plain
  barline's span), so the mark's column reserves that reach through the
  accidental-overhang mechanism and the source layout stays collision-free.
  A morphed measure's time-signature digits shift right by the sign's right
  extension (`repeatLeft`/`repeatRightLeft` are wider than the barline they
  replace); both adjustments are zero for the plain barlines, so repeat-free
  geometry is untouched.
- **Honest placement.** Repeat boundaries resolve via `RepeatPlacement` at
  projection time (`to_constrained` has no `Score`): `At(time)`,
  `RegionEnd` (zero-offset anchors to an existing region's end edge or to the
  end of an instance's last measure — the *column* is knowable where the
  *time* is not; zero-ness is judged **by value**, so a `Musical(0)` offset
  earns the same verdict as the `Zero` variant), or `Unresolved`, which draws
  **no ink** — unlike `resolve_time_anchor`'s origin fallback, a repeat sign
  at a false position would misstate the musical structure. A bare
  **wall-clock boundary is `Unresolved`**: it references no graph object, so
  nothing pins it to the region it would draw in — the sign would land
  wherever its time happens to *sort* among that region's columns (repeat ink
  for wall-clock-synchronized material is a later tranche; wall-clock
  `TimePoint`s reached *through* an object anchor place normally).
  Cross-region repeats keep the traced anchor only (content is dropped on the
  cross-region path — a documented Minimal boundary until repeat ink learns
  to split). Repeat dependencies now come from
  `RepeatStructure::anchor_sites()` (THE single site-set walk), so volta
  spans and jump targets are real invalidation and region-membership
  evidence.
- **Volta brackets.** Three strokes above the *top* staff (line at
  `VOLTA_Y = 6.5` staff spaces, two descending hooks) plus the ending numbers
  as `timeSig0..9` digit glyphs (the Minimal tier has no text primitive),
  all synthesized under `VOLTA_SYNTHESIS`, endings drawn verbatim in authored
  order. A reversed / zero-width / unresolvable span draws no bracket
  (advisory volta well-formedness is the authoring layer's jurisdiction).
  Bracket strokes are ordinary re-spaceable strokes, so the engraver's
  system-splitting (`StrokeFate::Split`) applies unchanged.
- **Glyphs.** The precomposed Bravura signs over hand-compositing
  heavy/thin/dot primitives; metrics extracted from the same SHA-pinned
  `bravura-1.392` as the rest of the table. The heavy line is aligned to the
  plain barline's span by a box approximation (`repeat_sign_x`: start signs
  left-aligned, end signs right-aligned, the combined sign centred).
  `is_barline_glyph` is exported so the casting-off solver classifies
  measure-boundary columns from this crate's name vocabulary instead of a
  string prefix.
