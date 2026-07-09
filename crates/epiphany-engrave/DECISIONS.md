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

**Phase 2 shipped the renderer-against-stub slice** (QUICKSTART, Agent I,
"Development pattern": build the renderer against the stub solver first, then
grow the real solver): a genuine deterministic *horizontal spacing pass* — the
first axis of the planned two-pass spring layout — later joined by real
hard-constraint evaluation (which earned the `Minimal` tier).

**Phase 3's layout track adds CASTING-OFF** (see "Casting-off (2026-07)" below):
system breaking at measure boundaries, vertical system stacking, page
assignment, a populated `ResolvedPage`/`ResolvedSystem` tree, and full break-
constraint evaluation. **Push 3's Standard-tier track then adds per-system
JUSTIFICATION** (see "Per-system justification (Push 3)" below): every non-final
system stretches to fill the content width; and **vertical justification** (same
section): the systems of a non-final page spread to fill the page height; and
**optimal break search** (see "Optimal break search (Push 3)" below) replaces
greedy first-fit + widow rebalance; and the **inter-staff vertical solve** (see
"Inter-staff vertical solve (Push 3)" below) renegotiates the gaps between a
system's staves. This completes the Standard-tier layout story end to end;
per-system justification (horizontal), vertical justification (inter-system),
optimal breaks, and the inter-staff solve (intra-system vertical) now all
compose.

### Honest tier

By the same rule `layout-ir`'s `StubSolver` follows, a solver that does not
evaluate the declared hard constraints and computes no quality metrics MUST
report `SolverTier::Stub`, never `Minimal` (Chapter 9 §"Conformance Tiers").
`Engraver::tier()` reported `Stub` until real hard-constraint satisfaction
landed and now reports `Minimal` — which it fully earns after casting-off: the
break constraint family is genuinely supported (spec §"Conformance Tiers",
Minimal row), and `Minimal` makes no optimality claim, so the break search
(originally greedy first-fit, now a badness-minimizing search) is legitimate. Since the Quality Metric Catalog companion's
ratification, the solve also reports a **real quality-metric vector** —
accurate metric vectors are part of the Minimal claim — computed per the
catalog's formulas (see "Quality metrics (2026-07)" below). The all-worst
placeholder (`QualityMetricVector::unmeasured`) remains only for malformed
inputs the solver cannot measure.

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

## Casting-off (2026-07) — decisions

1. **System breaking at measure boundaries.** Breaks fall only before a
   **barline column** (this projection draws each measure's barline at its
   *start* column, so breaking before the barline keeps every measure intact;
   the region-final barline closes the region and is never a candidate).
   *Originally greedy first-fit; replaced in Push 3 by an optimal break search
   — see "Optimal break search (Push 3)" below.* Consequences accepted and
   documented: a region with no measures never wraps automatically; a single
   measure wider than the page yields an overfull system (no mid-measure
   emergency break).
2. **Break-constraint semantics: "breaks at slot S" ⇔ S starts a system.** A
   `SystemBreakAt`/`PageBreakAt` is satisfied iff the final layout starts a
   system/page at that slot (a region's first slot is trivially at a
   boundary). Hard breaks are always honoured (mid-measure if necessary — a
   `Required` constraint binds absolutely); soft breaks are honoured unless
   the closing system would carry **no musical content** (no notehead/rest
   column) — the pathological path: the break is skipped, the soft violation
   warned, and the unhonoured preference recorded as an
   `EngravingDecision` with `DecisionSource::IrOverride` (spec's
   override-resolution rule: record, never silently drop).
3. **Frame of constraint evaluation.** Geometric constraints (no-collision,
   alignment, position-within) are evaluated against the **pre-casting spaced
   geometry** — the frame they are expressed in. Casting-off then relocates
   whole systems by per-system rigid motions, which cannot un-satisfy an
   intra-system geometric obligation; evaluating post-casting would instead
   make *every* `PositionWithin` (whose rect pins the region's source-frame
   vertical envelope) unsatisfiable for any casting-off solver, which cannot
   be the spec's intent. Break constraints are evaluated against the **final
   break structure**.
4. **Page geometry is an engraver parameter (`PageGeometry`), defaulted to A4
   at an 8 mm staff.** Adding a `Canvas` graph field is a schema-major change
   under the companion's frozen-layout rule, so it was staged to the data-model
   schema major. **Schema major 1 now defines the type** (`CanvasLayoutDefaults
   { page_size: CanvasSize, margins: CanvasMargins }`, staff spaces, A4/8mm
   default) and ratifies its wire form (core spec + Binary Format 0.3.0,
   Phase A); the **code graph home landed in Phase C** (`Canvas` gained the
   field). Wiring the engraver to *read* it (Phase C′) is a byte-neutral
   follow-up **deferred** until a custom-geometry producer exists, so the
   engraver still takes the geometry as a constructor parameter (every score's
   `layout_defaults` is the A4 default today). Default arithmetic (1 staff space = staff height / 4 = 2.0 mm at
   an 8 mm staff): A4 210 × 297 mm → **105 × 148.5** staff spaces; 15 mm
   margins → **7.5** staff spaces; content area 180 × 267 mm → **90 × 133.5**
   staff spaces. 90 staff spaces wraps the ten-measure hand-off fixture
   (≈ 99 staff spaces spaced) into two systems — an honest multi-system
   default golden.
5. **World-frame convention: pages stacked vertically in one world.** Page 1's
   top-left corner sits at the origin; page *n*'s frame begins a full page
   height plus `INTER_PAGE_GAP` (8 staff spaces, a presentation constant)
   below page *n − 1*'s. Every glyph/stroke position is **baked** into this
   single y-up world frame (per-system rigid translation: x back to the left
   margin, y to the stacked position), so the SVG renderer and the hit-test
   map work unchanged on the flat lists — no per-page transform exists
   anywhere downstream. The inter-*system* gap is read from
   `VerticalBand::inter_system_gap` (preferred 4.0 staff spaces), so the
   casting-off gap and the band model cannot drift.
6. **System-spanning strokes are split; the split is provenance-honest.** The
   five staff lines span the whole region; a break cuts them at the systems'
   content edges. The first segment keeps the original stroke's exact
   provenance (round-trip preservation); each later segment is synthesized
   (`SynthesisKind::Registered(SYSTEM_CONTINUATION_SYNTHESIS)`, the codebase's
   convention for kinds the normative vocabulary does not name) with a
   `continuation_instance_key(original stable id, ordinal)` instance key. The
   round-trip contract in `layout-ir` was relaxed accordingly (containment +
   declared-synthesis additions; the stub still must add nothing).
7. **Engraved-break decisions.** Every chosen system/page break appends an
   `EngravingDecision` whose **target** is the `MUSCLOID` id synthesized from
   the owning region's source under `SynthesisKind::EngravedBreak`, keyed by
   the breaking slot's (content-derived) identity. Source attribution:
   `UserOverride(id)` when the break constraint was projected from a user
   break override (the id flows through the new
   `ConstrainedLayoutIR.break_origins`), else `Automatic`; a skipped soft
   break records `IrOverride`. A boundary that actually opens a page records
   `PageBreak`; a later page opening at a region's own first system records
   `PageBreak` too; other boundaries record `SystemBreak`.
8. **Inverted tests.** Two tests that pinned the single-system semantics were
   deliberately inverted and renamed:
   `a_hard_break_cannot_be_honoured_by_single_system_minimal` →
   `a_hard_break_is_honoured_by_casting_off` (Unsatisfiable → Solved with the
   system count increasing), and
   `a_users_break_flows_to_a_soft_violation_not_a_failure` →
   `a_users_break_is_honoured_and_recorded_with_its_override` (soft-violation
   warning → clean Solved, break at the anchor's column, decision recorded
   with the user's override id). The pathological-soft path keeps the old
   warning semantics under a new, honest name
   (`a_pathological_soft_break_is_skipped_and_recorded_as_ir_override`).
9. **Widow rebalance (casting-off phase 2) — the honest P12-I11 fix.** Greedy
   first-fit (decision 1) is optimal for *page fill* — it packs each non-final
   system as full as the width allows — but that leaves a region's **final**
   system whatever is left over, often a narrow stub (a "widow") the
   `casting_off_quality` axis penalizes as a global casting-off failure. A
   second phase (`casting::rebalance_widows`) evens the split: it moves whole
   trailing measures from a region's penultimate system into its final one,
   choosing the shift that **minimizes the larger of the two distribution
   penalties the Quality Metric Catalog defines for the break family** — the
   width imbalance (`casting_off_quality`, the CV of the region's system widths)
   and the non-final break penalty (`system_break_penalty`, the mean of
   `|W − w|/W` over non-final systems) — each computed by the same formula as
   the axis it stands in for, so the rebalance optimizes the values the metric
   census will report, not a proxy. The
   two axes pull against each other (filling non-final systems worsens
   imbalance; equalizing widths worsens underfill) and both share the catalog's
   `0.5` anchor, so the raw quantities compare directly and the minimizer of
   their maximum is the width that best satisfies both. Scope: only a region's
   **last** boundary moves, and only when greedy placed it (an `Automatic`
   boundary with no break requirement or page force pinned to its slot); a
   user/IR-anchored or page-forced boundary is never disturbed, the penultimate
   system keeps ≥ 1 measure, and the final system never grows past its
   predecessor. The **system count is unchanged**, so decision 2's break
   structure, decision 5's page assignment, and every break-count test invariant
   hold. Result on **P12-I11**: RS-1 casts six/four instead of eight/two
   (`casting_off` 1.0 → 0.4463, every axis ≤ 0.90), the suite's asserted Xfail
   row is promoted to a plain Pass, with **no Quality Metric Catalog change** —
   the engraver improved, the `0.5` anchor and the `0.90` Minimal column stood.
   `ENGRAVER_VERSION` 2 → 3 (a wrapping score's baked geometry differs from pure
   greedy); the `ten_measure` render goldens were regenerated. Still a Minimal
   heuristic, not an optimality claim.
10. **Deferred refinements** (named, not implied): ~~per-system justification~~
   (**landed in Push 3** — see below); the vertical spring solve (band heights
   are carried, not yet renegotiated; systems stack by real content extents);
   orphan control and
   optimal/lookahead casting-off quality beyond decision 9's tail-only widow
   rebalance — a `Standard`-tier concern (full-region rebalancing, and
   justification-aware casting-off once systems can stretch); casting-off caching /
   incremental re-cast (the spec's incremental-layout section names the
   casting-off cache; `solve_incremental` currently re-solves from scratch,
   which remains observationally equivalent); per-system clef/key restatement
   (cautionary signatures at system starts, `SynthesisKind::Cautionary`);
   multi-system-aware x→time inversion for editor click-to-insert
   (`epiphany-editor-core`'s `position_at` interpolates one global x axis and
   is correct only within the first system of a wrapped region).

## Pass 12 candidates

See `spec/PASS12_BATCH.md` (rows P12-I1, P12-I2, P12-I3) — all three are now
resolved:

- **P12-I1 (resolved by I-1)** — the v0 pipeline was a *structural placeholder*
  (one arbitrary glyph per object at `y = 0`). `to_constrained` now builds real
  notation (clef-relative noteheads, accidentals, key/time signatures, rests,
  barlines, stems) and the Engraver re-spaces it; the Ch 7 engraving boundary
  resolved to notation-construction-in-`to_constrained`, spacing-in-the-Engraver.
- **P12-I2 (resolved)** — the `MUSCLOID` layout-object id derivation is wired
  (`epiphany-determinism` reserves the built-in tag; `layout-ir` provenance and the
  engraving-decision id route through it).
- **P12-I3 (resolved by I-4a)** — `BRAVURA_METRICS` is re-extracted from the same
  SHA-pinned `bravura-1.392` font the outlines come from, with bboxes rounded
  outward to contain the drawn ink (a `render-svg` test proves containment).

### New candidates from the casting-off slice (proposed rows; spec not edited)

- **P12 (proposed) — `Canvas.layout_defaults` is named but never defined.** The
  spec references layout defaults ("paper size, margins") on the canvas, but no
  chapter defines the type, its units, or its defaulting rules, and the core
  graph does not carry it. Proposal: define `CanvasLayoutDefaults { page_size:
  Size2D, margins: Margins }` in staff spaces in the data-model chapter,
  staged to the **data-model schema major** (adding the field changes the
  canonical graph encoding); until then, page geometry is a solver parameter
  (this crate's `PageGeometry`) and the spec should say a solver MAY default
  it.
- **P12 (proposed) — break-constraint satisfaction semantics.** Chapter 7
  defines `SystemBreakAt { slot }` but not what geometric fact makes it
  *satisfied*. This crate pins: satisfied iff the final layout **starts a
  system at that slot** (page analog for `PageBreakAt`); a region's first slot
  is trivially at a boundary. The spec should ratify (or correct) this
  predicate, since `Unsatisfiable`-vs-`Solved` conformance hangs on it.
- **P12 (proposed) — user-override attribution across IR stages.** The decision
  record for an honoured break must cite `DecisionSource::UserOverride(id)`,
  but the normative `LayoutConstraint` carries no origin, so the override id
  has no channel from the logical stage's `EngravingOverride` to the solver.
  This implementation carries a non-canonical `break_origins` sidecar on
  `ConstrainedLayoutIR`; the spec should bless that channel (or widen the
  normalized constraint record).
- **P12 (proposed) — synthesis kind for split continuations.** Casting-off
  splits region-spanning strokes (staff lines) at system boundaries; the
  segments in later systems are engraver-synthesized objects whose kind the
  normative `SynthesisKind` set does not name (`EngravedBreak` is the break
  itself, not its artefacts). Carried as
  `Registered(SYSTEM_CONTINUATION_SYNTHESIS)`; the spec should either add a
  continuation kind or bless the registered id.

## Quality metrics (2026-07) — decisions

The Quality Metric Catalog companion (v0.2.0) ratified the nine normative
axes' formal definitions, anchors, thresholds, and the
`QualityFloorApproached` trigger; `Engraver::resolve` now computes the real
vector (the private `quality` module), replacing the all-worst placeholder.
The catalog's normative constants (anchors, the Minimal/Standard threshold
table, the 0.8 warning fraction, the tier/profile→column mappings) are
transcribed once in `epiphany_layout_ir::quality` and consumed here and by the
testkit's reference-suite harness.

1. **Where each axis's inputs come from.** All nine are pure functions of the
   constrained input, the cast layout, and the declared page geometry — data
   the pipeline already had (see the `quality` module docs for the per-axis
   map). The casting pass exposes its own glyph→system assignment
   (`CastLayout::system_of_slot`, `region_of_system`) so the census ranges
   over what the solve actually did, never a reconstruction. Slot identity
   (the collision axis's same-column exclusion) is the glyph's
   `horizontal_slot` in the constrained input, index-parallel to the resolved
   glyph list. Widths/columns/densities use glyph **ink boxes** per the
   catalog's measurement domain (strokes are not glyphs); page spans use the
   resolved page tree's system bounding boxes.
2. **Vacuous axes.** `slur_shape_penalty` and `beam_slope_penalty` are exactly
   `0.0`: the pipeline draws no slur or beam geometry (both exist logically,
   not as curves/segments), so their contributing-unit sets are empty and the
   catalog's vacuous-geometry rule (`req:qmc:vacuous`) applies. The catalog's
   "notated-but-unrendered" open question explicitly owns this honesty edge;
   the axes are wired so the first slur/beam-drawing release is measured from
   day one.
3. **Vertical density's unit set.** `to_constrained` declares `InterStaffGap`
   bands but **no** `InterSystemGap` bands (the casting pass reads
   `VerticalBand::inter_system_gap` directly). Implemented units: (a) the
   input's `InterStaffGap` bands, adjacency reconstructed from
   `inter_staff_gap_id(region, g)` (gap *g* separates the region's staves
   *g−1*/*g*), realized separation measured between the adjacent staff bands'
   resolved ink extents within a common system — i.e. what the resolved
   geometry actually shows, since constrained `y` is pass-through; (b) the
   casting pass's realized inter-system gaps (consecutive systems on a page),
   measured from the resolved page tree against the same constructor's
   preferred height the stacking consulted. Today (b) measures realized ≡
   preferred (raw 0), and (a) is empty for every single-staff-per-region
   score; a multi-staff region honestly measures ~1.0 because the constrained
   stage's fixed 12-staff-space pitch is far from the band model's preferred
   2.0 gap — the metric is truthful, the vertical spring solve that would
   negotiate it is the deferred work.
4. **Floor warnings never change the status.** Catalog
   `req:qmc:floor-warning`: the `QualityFloorApproached` warning "is
   diagnostic: emitting it does not change the solve's status". Implemented
   literally: `status` is computed before the metric census, and quality
   warnings are appended after — a solve with clean constraints stays
   `Solved` even when it carries quality diagnostics. (This is also
   load-bearing for downstream regression locks that assert `Solved` on
   fixtures whose casting-off quality honestly warns.) The applicable
   threshold column is the one the config's profile selects
   (`profile_thresholds`: Draft→Minimal, Standard/Publication→Standard;
   default profile Standard), so `SolverConfig` is now threaded into
   `resolve`.
5. **Malformed inputs stay unmeasured.** A structurally invalid or
   forged-catalog input has no trustworthy geometry (the census would sweep
   unverified boxes), so it keeps `QualityMetricVector::unmeasured()` and
   earns no floor diagnostics. An `Unsatisfiable` solve of a *valid* problem
   is measured honestly — its real geometry exists.
6. **No-flip verification.** Existing tests asserting `Solved` on healthy
   fixtures were re-run against the real metrics: none flipped (warnings
   cannot flip status, and no metric enters the status computation). Two
   engrave tests asserting `warnings.is_empty()` after an honoured break were
   narrowed to "no `LargeSoftConstraintViolation`": their micro-fixtures
   (two-note scores broken at the last note column) honestly cast off into
   wildly uneven system widths, so the casting-off axis fires its SHOULD-level
   floor diagnostic — the metric is telling the truth about the layout, and
   the tests' actual claim (an honoured break is not a *soft violation*) is
   preserved exactly.
7. **Measured reality on the reference suite (first real vectors).** The six
   v0.1 entries now measure clean on every Minimal axis. RS-1's
   `casting_off_quality` was 1.0 under engraver **v2**'s pure greedy first-fit
   (the stub last line, above the Minimal 0.90 threshold — carried as a
   documented xfail row in the testkit harness, P12-I11); the **v3**
   widow-rebalance phase (casting-off decision 9) evens the split to
   `casting_off` = 0.4463, clearing the miss with no catalog change, and the
   xfail row is promoted to a plain Pass. The second finding (P12-I12): three
   short entries measured `spacing_distortion` 0.36–0.41, above the Standard
   0.32 warning floor — a spurious *diagnostic* (never a Minimal failure). It
   was resolved by a catalog refinement (QMC 0.1.0 → 0.2.0), see quality
   decision 8: `spacing_distortion` is scoped to rhythmic (note/rest) columns,
   dropping the three to 0.2188 / 0.0819 / 0.0856 — below the floor, no code
   layout change.
8. **Rhythmic-column spacing (`spacing_distortion` scoped) — the honest P12-I12
   fix.** The measured false positive was that a short healthy line's wide
   clef-to-first-note lead advance (furniture width, not note spacing) inflated
   the per-system advance CV above the Standard warning floor. The catalog
   (QMC 0.2.0) scopes the axis to **rhythmic columns** — spring slots bearing a
   notehead or rest — excluding the clef/key/time lead and treating barlines
   transparently (a note-to-note advance spans them). `quality::census` now
   builds `columns` only from slots in the precomputed rhythmic set
   (`is_rhythmic`: a `notehead*`/`rest*` glyph anywhere in the slot); the CV and
   contributing-unit rule (≥ 3 rhythmic columns) are otherwise unchanged. This
   is the mirror of the I11 resolution — measure the right thing rather than
   relax the threshold — but here the defect lived in the normative metric
   definition, so it *is* a catalog change (unlike I11). Measurement-only: the
   resolved layout, canonical bytes, render goldens, and `ENGRAVER_VERSION` are
   untouched; only the reported `spacing_distortion` value changes (RS-3/5/6
   drop below the floor, RS-2/RS-4 go vacuous-0.0 as their systems carry < 3
   rhythmic columns — honestly "too little to measure"). The floor-column
   regression test was re-pointed from b-flat's spacing (which no longer warns)
   to RS-1's casting-off (which still sits between the Standard and Minimal
   floors); a new `short_scores_do_not_trip_the_standard_spacing_floor` locks
   the fix. The duration-aware optical-spacing open question stays open.

### Pass 12 candidates (quality metrics)

- **P12-I11 — RESOLVED (engraver v3 widow rebalance).** First measured vectors
  (engraver v2) cast the RS-1 fixture into glyph spans ~78.6/18.8 staff spaces
  → width CV 0.61 ≥ the 0.5 anchor → clamped 1.0 > the Minimal 0.90 threshold.
  Resolved the **honest way (option (a))**: casting-off decision 9's
  widow-rebalance phase evens the split to ~59.5/37.8 (`casting_off` = 0.4463),
  so every axis passes and the testkit Xfail row is promoted to a Pass. Option
  (b) (a QMC anchor/threshold revision — raise the anchor, relax Minimal, or add
  an RS-1 override) was deliberately **not** taken: the `0.5` anchor and the
  `0.90` Minimal column stood. `ENGRAVER_VERSION` 2 → 3; `ten_measure` render
  goldens regenerated.
- **P12-I12 — RESOLVED (QMC 0.2.0, rhythmic-column spacing).** With uniform
  preferred widths, few-column systems (3–8 columns with a wide clef/key lead)
  measured spacing CV 0.36–0.41 — above the Standard column's 0.8 × 0.40 = 0.32
  warning floor, so the default profile emitted `QualityFloorApproached(Spacing)`
  on tiny, healthy scores. Resolved the **lead-aware way**: the catalog scopes
  `spacing_distortion` to rhythmic (note/rest) columns, excluding the
  clef/key/time furniture lead and treating barlines transparently (quality
  decision 8). The three entries drop to 0.2188 / 0.0819 / 0.0856 (below the
  floor). The alternative — a duration-proportional (optical) redefinition —
  needs the pipeline's deferred duration-aware preferred widths and stays the
  catalog's open question.

## Break-constraint satisfaction predicate (Pass 12 P12-I8, ratified)

The break-constraint satisfaction predicate — a `SystemBreakAt`/`PageBreakAt` at
`slot` is satisfied iff the final `ResolvedLayoutIR` starts a system/page at that
slot (a region-first slot trivially) — was ratified into the core spec by the
schema-major-1 track's Phase F (2026-07-06; core spec
`req:layoutir:break-satisfaction`; `spec/PASS12_RATIFICATION_LOG.md`,
schema-major-1 tranche). Satisfaction is a predicate on the output layout, not
the solver's spring state; casting-off evaluates the declared hard break
constraints as part of its tier claim.

## ENGRAVER_VERSION 3 → 4: repeat barlines + volta brackets (E1, 2026-07-07)

The layout pipeline now draws repeat signs and volta brackets (layout-ir E1
decision), so a repeat-bearing score's baked geometry differs from version 3's
invisible traced anchors — a version bump per the constant's own rule.
Repeat-free scores are byte-identical (the existing `ten_measure` /
`valid_score_rich` render goldens passed unchanged; only the new
`ten_measure_with_repeats` goldens were added). Casting-off changes:

- Barline-column classification routes through layout-ir's
  `is_barline_glyph` instead of the `"barline"` name prefix, **and now
  requires a directly-manifested `Measure` source** (no synthesis): a morphed
  repeat sign remains a break candidate and its measure keeps its record,
  while a repeat-synthesized standalone sign (a mid-measure boundary, a
  region edge without a final barline) and the `repeatDots` pair classify
  nothing — the casting contract breaks systems at measure boundaries, and a
  phantom candidate could tear off a degenerate lone-sign trailing system or
  split a measure. Locked by
  `repeat_signs_keep_measure_records_honest_and_raise_their_system`, and the
  criterion-6 round-trip now runs the repeat fixture through the real
  Engraver.
- Volta bracket strokes raise their system's extent, so vertical stacking
  and page overflow account for them with no engraver change (system height
  is computed from every member box and stroke).
- **Same-slot spacing preservation (review follow-up, folded into version
  4 before release).** The spacing pass reserves a slot's full content extent
  (its companions included) in the slot's advance, but the remap moved every
  glyph independently by piecewise interpolation — so a same-slot companion
  whose absolute x crossed the next slot's *source* (E1 made this reachable:
  a time signature after a morphed `repeatLeft` sits `TIME_SIG_X` + the
  sign's right extension ≈ 1.8 staff spaces right of its barline, past the
  1.6-space constrained column step) was dragged by the wrong interval and
  collapsed into the following note. Glyphs now translate by **their own
  slot's rigid delta** (`spacing::space_slots` returns the per-slot
  `(source, target)` pairs beside the interpolation control points), which
  is what honors the reservation; intra-slot offsets survive verbatim.
  Spanning strokes keep endpoint interpolation; rigid (ledger) strokes now
  translate by the owning glyph's slot delta exactly. Regression:
  `time_signature_digits_ride_their_barline_slot_past_a_repeat_sign`
  (unbounded page so x-disjointness compares one line; verified to fail
  against the interpolated remap).

## ENGRAVER_VERSION 4 → 5: slur curves (E2, 2026-07-08)

The pipeline draws slurs as cubic-bézier `Curve` primitives (layout-ir E2), so
the resolved output carries a third primitive kind and a slur-bearing score's
baked geometry differs from version 4's traced anchors — a version bump per
the constant's own rule. Slur-free scores draw the same ink; only the empty
`curves` count prefix enters their canonical bytes (self-consistent). The
existing render SVG goldens are byte-identical; the six snapshot goldens gained
a `curve_count=0` line (a new tracked primitive kind), and the new
`ten_measure_with_slurs` fixture got its own goldens. Engrave changes:

- `HorizontalRemap::curves` re-maps each curve's four control-point x's through
  the same coordinate map as a spanning stroke's endpoints (a slur is never
  rigid-width); y preserved.
- Casting: a `curve_fate` (originally `curve_system`) rides a curve WHOLE in one
  system when it fits, and — since Push 3 (see below) — **splits** a curve
  spanning a system break into per-system sub-cubics by de Casteljau. A curve's
  control-point hull grows its system's extent, so a slur above the staff raises
  the system height for page overflow, like a volta bracket.
- `slur_shape_penalty` stopped being *vacuous* 0.0 and became *0.0 by
  construction* at E2; Push 3 (below) makes it a **real measurement**.

## ENGRAVER_VERSION 6 → 7: curve splitting across systems (Push 3, 2026-07-08)

A slur spanning a system break now **splits** into per-system sub-curves
(`curve_fate` → `CurveFate::Split`) by **de Casteljau** subdivision at the
parameters where the curve crosses each system's content clip edges, replacing
E2's draw-whole-in-start-system (the floating-end Minimal boundary). The first
segment carries the slur's exact provenance (the round-trip source surjection
recovers it once); later segments are synthesized continuations under
`SYSTEM_CONTINUATION_SYNTHESIS` with a `(stable_id, ordinal)` key — exactly a
split stroke's discipline. The split needs an x-monotonic curve to invert
`x → t` (a slur is, its control points x-ascending by construction; `param_at_x`
bisects); a non-monotonic curve (not produced by the engraver) falls back to
riding its start system whole. `ENGRAVER_VERSION` 6 → 7 — but only a
break-spanning slur's baked geometry changes; a slur that fits in one system is
`CurveFate::Rigid`, byte-identical to version 6, so the existing goldens are
unchanged (the fixture's slurs are short).

## slur_shape_penalty measured, not pinned (Push 3, 2026-07-08)

The `slur_shape` quality axis moved off its `0.0` placeholder to a real
measurement per the Quality Metric Catalog (`req:qmc:slur`): for each drawn
slur `Curve` with chord `c > 0` (the segment between its endpoints) and apex
height `h` (max perpendicular distance from the curve to that chord, sampled at
`SLUR_APEX_SAMPLES = 32` points), the arc ratio `ρ = h/c` is penalized by its
distance outside the shallow-arc band `[0.08, 0.25]`
(`max(0, 0.08 − ρ, ρ − 0.25)`), meaned over units and normalized by
`R_worst = 0.25`.

**The unit is the WHOLE SPACED slur curve** — post-horizontal-remap (the drawn
shape) and pre-cast-split (one unit per slur) — **not the cast output's
per-system fragments** (review fixes). A break-spanning slur casts into
per-system sub-cubics whose *diagonal* chords each read flatter than the whole
arc, so measuring fragments would spuriously penalize (and double-count) a slur
that is ideally shaped as a whole — violating the catalog's "a tier that draws
the ideal shallow arc measures 0" property. The whole arc is the unit. An
earlier take measured the *constrained* `input.curves` (pre-remap); a second
audit noted the catalog's units are "drawn slurs," so measurement moved to the
**spaced** curves (`spaced_curves`, threaded into `measure`) — horizontal
re-spacing that flattens or steepens a drawn slur is now honestly captured
rather than hidden behind the intended (pre-remap) shape.

Honest outcome: the Minimal tier's mid-span slurs sit at `ρ = height/span =
0.16` (the `SLUR_HEIGHT_FACTOR`; in band → 0), but the fixed
`SLUR_MIN_HEIGHT`/`SLUR_MAX_HEIGHT` clamps push a **short** slur above the band
(a tall arc on a tiny chord, too bulgy) and a **very long** one below it (too
flat) — so the axis is genuinely non-zero for clamped spans, which a
duration-aware Standard-tier height would improve. A curve-free layout measures
0 by the vacuous-geometry rule. **No `ENGRAVER_VERSION` bump** — a
measurement-only change that leaves the resolved geometry, canonical bytes, and
render goldens untouched (the same rule quality decision 8 followed); the RS
reference suite is unaffected (no RS entry carries a slur).

## Per-system justification (Push 3, 2026-07-08)

The Standard-tier track's first block: every **non-final** system of a
multi-system region stretches its horizontal slack so its ink fills the content
width, instead of sitting at its natural left-aligned width. `ENGRAVER_VERSION`
7 → 8 (any wrapping score's baked geometry differs; a single-system score is
unchanged — its only system is ragged-right by convention).

**The affine, clamped map.** Casting bakes each system by a `Placement`: a
vertical shift `dy` plus a horizontal map `world_x = a·x + b`. A rigid
(unjustified) system has `a = 1, b = dx` — the old pure translation. A justified
system spreads the slack linearly: `a = 1 + extra/span`, `b = base_dx −
extra·x0/span`, where `span = x_last − x0` is the slot-source range, `extra =
content_width − natural_ink_width`, and `base_dx` puts the leftmost ink at the
left margin. The map is **clamped** to `[x0, x1]`: within the slot range it is
affine; beyond it (a glyph's bearing overhang, a staff line drawn to the ink
edge) it is rigid slope-1 — so the mapped ink extremes agree *exactly* with the
per-slot deltas at the first/last slots, and the justified ink spans exactly
`[left_margin, left_margin + content_width]` (no over/undershoot).

**Slot-relative, like E1.** Glyphs translate by the map evaluated at their
SLOT's source (`Placement::slot_dx`), constant per slot, so intra-slot offsets
(a time signature after its barline, an accidental left of its notehead) survive
verbatim — never scaled by `a`. A spanning stroke (staff line, volta bracket)
maps each endpoint through the affine (it stretches). A per-event **component
stroke** (a stem or ledger) translates by its owning slot's delta, so it stays
attached without stretching its offset.

**Component-stroke classification (`component_glyph`), a review fix.** The first
justification cut used `is_rigid_width_stroke` (LEDGER-only) to pick slot-anchored
strokes, on the false premise that it also covered stems. It does not: a stem is
an `Event`-sourced stroke drawn at `notehead_x + 1.15` — no same-source glyph
(noteheads are `Pitch`-sourced) and no baseline in its x-span — so it fell to the
affine branch and its offset was scaled by `a`, detaching it ~0.75 ss (up to
~1.5) from its head in every justified system (and, latently, a smaller drift in
the spacing pass, which had the same classification). `component_glyph` now
classifies a stroke: a `Staff` (staff line) or `RepeatStructure` (volta bracket,
whose ending-number glyphs share its source) source SPANS → affine; else the
`owning_glyph` (a ledger over its notehead, same `Pitch` source, overlapping in
x); else the glyph with the greatest baseline ≤ the stroke's x — a stem's own
in-column notehead/dot, all in the one slot (`stem_offset 1.15 < column step
1.6`, so this is exactly the stem's slot). Applied in BOTH the spacing remap and
casting, so stems stay on their heads through the whole pipeline. Locked by
`stem_offsets_from_the_notehead_survive_justification`.

**A slur's control points map straight through the affine** — its endpoints
follow their anchor notes and the arc stretches with the span. **Known minor gap
(deferred):** the endpoints carry an authored `SLUR_INSET` (0.6 ss) tuck from
their notes; the affine scales that offset by `a`, so a slur in a stretched
system tucks ~`0.6·(a−1)` further from its heads (≈0.3 ss at `a≈1.5`) — still
reading as "near" the note, same root cause as the stem drift but on a soft
connector. A proper fix would slot-anchor `p0`/`p3` to their event slots while
stretching the interior; deferred until slur fidelity warrants it (the curve
carries no per-endpoint slot today).

**Which systems justify.** Not the last system of a region (ragged-right, as
engraving convention wants); not a system with no finite width target, a
degenerate (single-slot) span, or already at/over width (`extra ≤ 0` — never
compressed into overlap). Greedy first-fit fills non-final systems near-full, so
the stretch factor `a` stays near 1 in practice.

**Quality-metric consequences (honest, noted).** Justification drives
`system_break_penalty` to near zero (a non-final system now fills the width — the
point). As a side effect the width-uniformity axes RISE: the full non-final
system contrasts with the ragged last line, so `casting_off_quality` and
`symbol_density_uniformity` measure that contrast (for the ten-measure fixture,
casting-off ~0.45 → ~0.80). This is honest for justified layout (full lines + a
short last line), but the axes arguably penalize the *intentional* raggedness of
the final system. **Follow-up (not this tranche):** score casting-off on natural
(pre-justification) widths, or exclude / fill-fraction-weight the final system —
a metric-semantics change needing catalog alignment. **Also noted:**
`slur_shape` measures the *spaced* (pre-justification) whole curves, so
justification's horizontal stretch of a slur is not reflected — a second-order
gap in the same family as the spaced-vs-constrained one, a follow-up if slur
fidelity warrants measuring the post-justification whole curve.

## Vertical justification (Push 3, 2026-07-08)

The vertical analog of per-system justification, and the first piece of the
deferred vertical spring solve: the systems of every **non-final page** spread
so the last system's bottom reaches the content bottom, filling the page height.
A second pass after the top-down stacking loop, once page membership is known:
for each non-final page with ≥2 systems it computes the vertical slack (the last
system's natural bottom above the content bottom) and distributes it evenly
across the inter-system gaps — system `i` (0-based on the page) sinks by
`i/(n−1)` of the slack, so the first stays at the content top and the last lands
on the content bottom. Only `Placement::dy` changes, so it composes cleanly with
horizontal justification (independent axes). `ENGRAVER_VERSION` 8 → 9.

**Which pages.** The last page stays ragged-bottom (top-aligned), as engraving
convention wants — a single-page score is therefore unchanged (its only page is
the last), so every existing single-page golden is byte-identical. A page with
one system has no inter-system gap to grow; an already-full or overfull page has
no positive slack. Locked by `vertical_justification_fills_non_final_pages` (a
small custom `PageGeometry` forces the multi-page path; the non-final page fills,
the last stays ragged).

**Quality-metric trade-off (honest, tested).** Vertical justification drives
`page_fill_efficiency` to ~0 on justified pages (they fill the height — the
point), but the same stretch grows their inter-system gaps beyond the band
model's preferred height, which `vertical_density_penalty` measures directly
(its inter-system-gap term, quality.rs `vertical_raw`). So the two axes TRADE:
filling the page is paid for in inter-system density, and a *sparse* justified
page (few systems, large per-gap stretch) is charged more — which is a defensible
signal (a 2-systems-on-a-tall-page layout genuinely reads thin) but over-charges
a *moderate*, uniform stretch that is good justification. Same family as the
horizontal `casting_off` / justified-raggedness note; the catalog refinement
(score only EXCESS stretch, or measure gap UNIFORMITY rather than deviation from
preferred) is the deferred follow-up, needing catalog alignment. Pinned by
`vertical_justification_trades_page_fill_for_inter_system_density` so the
interaction is not silent (review finding).

**Still deferred (the rest of the vertical spring solve).** Inter-staff
band-height renegotiation *within* a multi-staff system (today the constrained
stage's staff stacking is preserved verbatim; `vertical_density_penalty`
measures it but nothing renegotiates it). That needs vertical pressure — a
collision or a target — to be meaningful, and multi-staff systems to exercise;
a later tranche.

## Optimal break search (Push 3, 2026-07-08)

Casting-off's greedy first-fit + tail-only widow rebalance is replaced by a
deterministic **badness-minimizing break search** (`optimal_breaks`, a
Knuth–Plass-style dynamic program). `ENGRAVER_VERSION` 9 → 10 (a wrapping
score's measures partition into different, more balanced systems, so its breaks
and all flowing geometry differ; a non-wrapping score is unchanged).

**Objective.** Minimize the sum over ALL systems of the squared normalized
underfill `((width_limit − w)/width_limit)²`. Squaring evens the systems (a
lopsided split costs more than a balanced one); including the FINAL system in
the sum is what subsumes the old `rebalance_widows` (the optimizer will not
leave a narrow final stub if a more balanced partition is cheaper). It is the
additive, DP-tractable analog of the retired `distribution_cost` (max of the
catalog's break penalty and width-CV imbalance): both reward filled, even
systems, but the additive form admits a polynomial DP over the measure
boundaries. On the ten-measure fixture the search settles on **5/4 measures**
where greedy + rebalance left a fuller-then-shorter split, which fills the final
system more and pulls `casting_off_quality` down (~0.80 → ~0.61) — the payoff,
visible now that horizontal justification has driven `system_break` to ~0 so
the last system's fullness is what `casting_off` mostly sees.

**Requirements and overfull.** The break REQUIREMENTS (hard / soft / page)
bound the DP's segments — a system may not span a forced break — and
`walk_region` still honours them (and records a skipped content-less soft break
as an `IrOverride`, unchanged); `optimal_breaks` reports only the AUTOMATIC
breaks. A system may not exceed the content width unless it is a **single
unsplittable measure** (an overfull lone measure, which greedy also emitted; not
charged, since nothing can be done). `Minimal` still makes **no optimality
claim** — this is a deterministic global heuristic, an honest improvement on
first-fit, not a formal guarantee.

**Overflow safety net (review fix).** `walk_region` skips a planned break —
a soft requirement, or the DP's own automatic break — when the closing system
carries no musical content (the lead-only exception, `has_note`). The DP treats
that boundary as a real system start and optimizes each side independently, so
it cannot foresee the skip: a note-less leading measure would then be absorbed
into the following optimizer-filled system, which could **silently** overflow
into a multi-measure overfull system (a review finding — the greedy overflow
check that used to catch this was removed with the greedy pass). The net
restores exactly that check as a fallback: `walk_region` also breaks before a
measure that would overflow the content width (`chunk_hi[i] − current_lo >
width_limit`, guarded by `has_note`). In the common content-full case the DP's
break fires first, so the net never triggers and the geometry is the
optimizer's (zero golden churn). Locked by
`a_content_less_measure_before_a_soft_break_never_overflows`.

**Determinism.** A pure function of the slot extents and requirements; the DP
minimizes lexicographic `(cost, system_count)` (fewer systems — hence fewer
pages — breaks ties), and among equal `(cost, count)` the earliest-considered
predecessor wins. Tests: `optimal_breaks_balances_systems_and_avoids_a_final_
widow`, `optimal_breaks_never_spans_a_forced_break`,
`optimal_breaks_is_deterministic_and_empty_when_unbounded`; the widow test now
checks the balanced measure distribution the DP produces.

## Inter-staff vertical solve (Push 3, 2026-07-08)

The last vertical-spring piece: the gaps BETWEEN a system's staves are
renegotiated, so tightly ledgered or slurred adjacent staves — which the
constrained stage stacks at a fixed pitch — separate. `ENGRAVER_VERSION` 10 →
11 (a multi-staff score whose staves press together shifts them apart; a
single-staff score, with no inter-staff pair, is byte-identical).

**Attribution is DECLARED, not inferred.** Every primitive — glyph, stroke,
curve — carries a `vertical_band`, and the solve reads its owning staff straight
out of it (`VerticalBandKind::Staff` → `StaffId`). Content owned by no staff
names a non-`Staff` band and is attributed to `None`, taking no staff shift.
There is no geometry in the attribution path at all: no distances, no epsilons,
no fallbacks. The projection that emitted the primitive already knew the answer —
a stem's band is its note's, a slur's is its notes' — so it says so.
(The geometric `round(-y / pitch)` alternative was rejected early: an extreme
ledgered note on the top or bottom staff rounds to a non-existent neighbour.)

**Why: inferring the owner from proximity failed twice, in ways a gate missed.**
Both bugs shipped into a committed golden and were caught only by review.

1. *Stems.* The first cut reused `component_glyph`, whose fallback picks the
   nearest glyph **by x alone**. That is right for a *slot* — both staves of a
   system share their x columns, hence their spring slots, so the horizontal
   delta is the same either way — but wrong for a *staff*: it handed a lower-staff
   stem to the UPPER staff's notehead, so the stem kept the wrong vertical shift
   and tore off its own head by several staff spaces (and polluted the upper
   staff's content extent, inflating the computed gap). Patched with a 2-D
   nearest.
2. *Slurs.* The same class one layer deeper, and unfixable by any distance
   metric. A slur's start endpoint is deliberately *lifted off* its notes —
   `staff_top + gap` above, `staff_bottom - gap` below — into the inter-staff
   zone, where the nearest glyph is frequently a note on the ADJACENT staff (in
   `two_staff_close_content`, a top-staff ledger note). The bottom staff's slur
   was attributed to the top staff, kept shift 0, and tore off its own notes.
   Patched with an arc-direction rule read against the staff-line bands.

Both patches were *correct* and both were the wrong shape: they reconstructed, by
geometric inference, a fact the projection had in hand and discarded. `Stroke`
and `Curve` now declare `vertical_band` exactly as `GlyphObject` always has, and
the two rules above are deleted. The engraver's attribution is three map lookups.
Locked by `multi_staff_stems_stay_on_their_own_staff` and
`a_slur_travels_with_its_own_staff` (both kept — they now assert an outcome the
data model *guarantees*, which is where a regression would surface if the
declaration were ever dropped), and by layout-ir's
`every_stroke_and_curve_names_a_band_that_exists`. Adopting it churned **no
golden**: the declared owner agrees with the inferred one on every fixture.

A related correction from the same review: staff-attributed primitives contribute
their y ONLY through the shifted path (`Extent::add_x` for x, `add_y` for the
shifted staff extent), so a lower staff's unshifted content can no longer inflate
a system's `max_y`.

**What the band model had to grow to carry this.** Two bands were previously
emitted only when a *glyph* needed them, which left strokes naming bands that did
not exist (validation now rejects that outright, as `UnknownBand`):

- A **staff band** is emitted for every staff of the region, in the region's own
  staff order — the order `y_origin` stacks by — rather than only for staves that
  emitted a glyph. A staff whose clef is unbundled engraves to an anchor *stroke*
  and no glyph, and would otherwise have had no band.
- The **margin band** is emitted unconditionally. A region's own traced anchor is
  a stroke, and it names the margin band whether or not any region-level glyph
  puts a member in it.

Both may carry zero members, as an inter-staff gap band already did: band
*membership* drives the spring solve over glyphs; band *existence* is what
attribution needs. Strokes and curves are deliberately NOT added to
`VerticalBand::members` — their band reference is one-way, validated only to name
a real band.

**Known gaps (reviewed, not bugs today).**

- ~~`vertical_density_penalty` **saturates at 1.0** on the two-staff fixture.~~
  **RESOLVED** (see "The gap band is a height model" below). It was not the
  metric-vs-solver tension I first filed it as, and needed no catalog refinement:
  the engraver was simply **non-conforming**. The catalog always defined the
  realized gap over the *content extents* the band separates; the implementation
  measured the band's glyph `members`, because until primitive band ownership was
  ratified a band listed no strokes or curves to own.
- **Staff-less content takes no staff shift** — margin-band glyphs, and a volta
  bracket whose repeat spans several staves. It stays put while the staves below
  it descend, which is right for content that sits above the top staff (the top
  staff's shift is always 0). A volta anchored to a *single* staff declares that
  staff's band and moves with it. This used to be an accident of geometry — a
  volta near a notehead was dragged onto that notehead's staff by the nearest-
  glyph fallback — and is now a declared property; see "Attribution is DECLARED"
  above. What remains undecided is genuinely staff-less content placed *between*
  two staves: it holds still while the lower staff descends away from it. That
  wants a height model for the inter-staff gap band, not an attribution rule.
- ~~The preferred gap is read from `VerticalBand::inter_staff_gap(VerticalBandId(0))`
  rather than from the region's *declared* inter-staff band.~~ **RESOLVED**: the
  solve now reads the `InterStaffGap` band `to_constrained` emitted for that
  staff pair, the same band the metric scores against.
The 3+-staff cascade was the last of these to be closed; see below.

**The solve.** Per system, per staff, the real content y-extent is collected
(glyphs, strokes, curves — ledgers and slurs included, not just noteheads). The
staves are ordered top-to-bottom by their staff-line reference y and that order
is kept fixed; each staff is then shifted DOWN by the cumulative amount needed
to bring its gap to the one above up to the band model's preferred inter-staff
gap (`VerticalBand::inter_staff_gap`). `staff_shift[(system, staff)]` — the top
staff's is 0 — is a per-staff `dy` the bake applies (via `Placement::sunk`) on
top of the per-system `dy`, so glyphs, strokes, curves, the staff/measure/system
records, content bounds, hit-test regions, and the quality metrics all read the
same shifted geometry (they all consume the baked output). The shifts grow each
system's extent, which the vertical stacking and justification then consume — so
the inter-staff (intra-system) and inter-system passes compose.

**Scope.** Enforces the preferred gap (which subsumes non-overlap). Only `dy`
per staff changes; staff order is fixed; a single-staff system has no pair and
is untouched. Locked by `inter_staff_solve_separates_colliding_staves` (the
two-staff pressure fixture's staff-line gap opens past the fixed pitch while a
single-staff score keeps one staff per system) and the `two_staff_close_content`
render golden (the visible before/after: slice 1 tight, slice 2 separated).
**Deferred:** compressing an OVER-wide fixed gap toward preferred (the solve
only expands, never pulls staves together — the fixed pitch is generous by
default, so this is rarely wanted); per-staff spring *stretch* to fill spare
system height (the inter-system justification carries the fill for now).

**The cascade, and why it grows faster than the raw corrections.** A pair's gap
is measured against the upper staff's **already shifted** bottom, so staff *i*'s
shift is the sum of every correction above it plus its own. A consequence worth
stating because it is counter-intuitive: an *increment* `shift(i+1) - shift(i)`
generally EXCEEDS the lower pair's own raw correction, because the upper staff's
descent has itself eaten into that pair's gap and must be undone. (The first
version of the cascade test asserted the opposite — that a gently-pressed lower
pair's increment would be the small one — and failed against a correct solve.)
Locked by `inter_staff_shifts_cascade_down_three_staves` over the new
`three_staff_close_content` fixture: three staves in ONE region (so all three
land in one system), with **asymmetric** pressure — the upper pair collides hard
(C1 against C7), the lower pair only gently. Sizing each pair independently — the
plausible wrong implementation — measures the lower pair against the middle
staff's ORIGINAL position and hands the bottom staff only its small raw
correction, dragging it back up through the middle staff. **Verified by
mutation:** with the cascade removed the bottom staff's shift collapses from
34.68 to 4.56 (against the middle staff's 15.06) and both the shift ordering and
the staff-line-gap assertions fail — while `two_staff_close_content` still
passes, which is exactly why the three-staff fixture was needed. The fixture also
pins curve attribution against a THREE-band choice (the slur must still find the
bottom staff, not merely the nearer of two) and carries its own render golden.

## The gap band is a height model (Push 3 residue, 2026-07-09)

Two items deferred by the inter-staff solve — a saturated
`vertical_density_penalty` and a preferred gap read from a constructor rather
than the region's declared band — turned out to be one thing, and neither was
the "metric-vs-solver tension" I filed them as.

**The solve reads the declared band.** Per system, per adjacent staff pair, the
gap it targets is the `preferred_height` of the `InterStaffGap` band
`to_constrained` emitted for that pair (`inter_staff_gap_id(region, g)`, gap `g`
separating the region's staves `g-1` and `g`), not `VerticalBand::inter_staff_gap`'s
default. That is what makes the band a *height model* rather than a constant: a
region declaring a wider gap gets one, and the metric — which scores against the
same declared band — agrees with the solve by construction rather than because
both call the same constructor. Every staff of a region carries content in every
system of that region (its staff lines are per-staff strokes, split into each
system), so the staves present in a system are the region's full staff order and
the window index is the gap index.

**The metric was non-conforming, and the catalog was right.** `req:qmc:vertical`
has always defined the realized gap as the separation "between the adjacent
**content extents** the band separates". `vertical_raw` measured the separation
between the two bands' glyph `members` — because until
`req:layoutir:primitive-band-ownership` landed, a band listed no strokes or curves
to own. A staff's outermost ink is usually not a glyph: on
`two_staff_close_content` the solve cleared the declared 2.0 gap exactly (content
gap 2.0), while the glyph-ink gap was **5.06**, giving `|5.06 - 2|/2 = 1.53`,
clamped to a saturated **1.0** — and a Standard-tier floor warning on a layout
that was correct. The metric was charging the solver for the ledger and slur ink
it had made room for.

`vertical_raw` now measures each staff band's full content — glyphs, strokes, and
curves, each attributed by its declared `vertical_band` — and the axis reads
`2.7e-7` on that fixture. Two consequences worth stating:

- **No catalog version move.** The formula, contributing units, anchor, and
  normalization are unchanged; only a wrong measurement was. This is the P12-I11
  precedent (engrave-side resolution), not P12-I12 (a definition defect). The
  catalog gains a *clarification* of what "content extent" means, plus a
  rationale refresh — the v0.1 rationale still claimed the vertical spring solve
  was deferred.
- **The geometry is read back from the BAKED output**, not from the solve's own
  `staff_ext`. Reading back the solver's intent would make the axis circular and
  blind to exactly the bug class that bit twice: a shift the bake fails to apply
  to some primitive class now surfaces as a real deviation. `CastLayout` gained
  `stroke_system` / `curve_system` for this (a stroke carries no spring slot, so
  `system_of_slot` cannot answer for it).

**Still a real trade-off, not a defect:** the axis's *inter-system* half. Vertical
justification deliberately stretches inter-system gaps past preferred to fill a
non-final page, trading this axis against `page_fill_efficiency`. Both are
reported; neither is wrong. Recorded in the catalog rationale so it is not
re-filed as a bug.

**Still open:** staff-less content placed *between* two staves would hold still
while the lower staff descends. No primitive can name an `InterStaffGap` band
today (`band_of` yields a staff band or the region's margin band), so this is
unreachable rather than latent — building the machinery now would be speculative.

### Review follow-up: two more glyph-members assumptions (2026-07-09)

An adversarial review found the content-extent correction incomplete in two
places. Both were right; the second falsified a comment written in the same
commit that introduced it.

1. **Region staff bands were still identified by glyph `members`.** `vertical_raw`
   measured content over all primitives but decided *which* staff bands belong to
   a region by glyph membership — the very assumption the change exists to shed.
   A staff band is allowed to own no glyphs: `to_constrained` emits one per staff
   of the region regardless, and a percussion-clef staff (no bundled glyph, so it
   engraves to a traced anchor stroke) with no notes owns only its staff-line
   strokes. Region membership now comes from **content presence** in one of the
   region's systems, which identifies the band exactly, because a staff band is
   per-`(staff, region)` manifestation and its content can land nowhere else.
   Locked by `percussion_placeholder_staff` + `a_staff_band_owning_no_glyphs_
   still_contributes_its_gap`. Mutation-verified: the members filter scores
   `4.8e-7` where the fix reports real deviation.

2. **Only the first realizing system was measured.** The code took one system per
   gap band, justified by a comment claiming rigid system translation makes every
   realization agree. The inter-staff solve had *just* falsified that: it sizes
   each system's gaps from that system's own content. `req:qmc:vertical` now
   counts **one unit per realization** (QMC 0.2.0 → 0.3.0), matching how realized
   inter-system gaps were already counted. Locked by `two_staff_wrapping_pressure`
   (staff-line gap 15.93 in the pressured system, 7.87 in the slack one) +
   `inter_staff_gaps_are_measured_in_every_system_that_realizes_them`.
   Mutation-verified: first-system-only scores `1.3e-7`.

**What this exposes, and it is not comfortable.** `two_staff_wrapping_pressure`
scores `vertical_density_penalty` **0.739**. Its pressured system solves to the
declared gap exactly; its slack system sits at ~5 staff spaces of content gap
against a preferred 2.0. The axis is symmetric — a gap wider than preferred is
sprawl exactly as a narrower one is crowding — and this solve **only expands,
never compresses**. The deferral recorded above ("compressing an OVER-wide fixed
gap toward preferred… the fixed pitch is generous by default, so this is rarely
wanted") is therefore promoted from *rarely wanted* to **measurably wrong**: any
un-pressured multi-staff system now reports honest sprawl until the solve can pull
staves together. Named here rather than fixed in the same breath — compression is
a layout change (golden churn, `ENGRAVER_VERSION` move), not a measurement one.
