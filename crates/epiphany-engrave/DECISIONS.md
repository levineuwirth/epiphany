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

The Quality Metric Catalog companion (ratified at v0.1.0; currently v0.3.0 —
the axis set, anchors, and thresholds are unchanged across both minor
revisions, which narrowed *what is measured over*, not what it is normalized
against) ratified the nine normative axes' formal definitions, anchors,
thresholds, and the `QualityFloorApproached` trigger; `Engraver::resolve`
now computes the real
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
~~**Deferred:** compressing an OVER-wide fixed gap toward preferred (the solve
only expands, never pulls staves together — the fixed pitch is generous by
default, so this is rarely wanted).~~ **DONE** — see "Two-sided renegotiation"
below; "rarely wanted" was wrong, and the metric said so.
**Still deferred:** per-staff spring *stretch* to fill spare system height (the
inter-system justification carries the fill for now).

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

## Two-sided renegotiation, and the cascade defect it uncovered (2026-07-09)

`ENGRAVER_VERSION` 11 → 12. The inter-staff solve now **closes a slack pair as
well as opening a crowded one**, realizing the `InterStaffGap` band's declared
height exactly. The constrained stage's fixed `SYSTEM_STAFF_PITCH` is demoted
from a floor to an initial arrangement the solve fully renegotiates.

**Why:** the expand-only solve made `vertical_density_penalty` report honest
sprawl on every relaxed multi-staff system (0.739 on `two_staff_wrapping_pressure`).
The axis is symmetric — a gap wider than preferred is sprawl exactly as a
narrower one is crowding — so either the layout or the number was wrong. It was
both, in different ways.

**What the band's height MEANS.** It is an **ink clearance**: the separation
between the two staves' outermost content (ledgers, stems, slurs), which is the
unit `req:qmc:vertical` measures. The old `preferred = 2.0` was a placeholder
reconciled with nothing — neither the 8.0 staff-box gap the fixed pitch of 12
produces, nor the ~6.4 ink clearance that stacking leaves for plain content.
Realizing 2.0 would have crushed a relaxed system to a staff pitch of ~7.6.
`preferred` is now **5.0** (a staff height plus a space) and `min` **2.0** — plain
ledgered content settles near a pitch of 10.6, close to convention, while
ledgered or slurred content pushes the staves apart on its own. The user chose
this value; `4.0` ("one staff height") was rejected once its true consequence —
pitch 9.57, not the 11.04 an arithmetic slip of mine had projected — was measured
rather than inferred.

**The cascade was wrong, and had been since v11.** Making the solve two-sided
made `three_staff_close_content` report `vertical_density_penalty` **1.0**: its
lower pair realized 21.06 against a declared 4.0. The recurrence subtracted the
upper staff's shift from the measured gap and then added it back through the
accumulator:

```text
gap   = (upper_lo - shift_upper) - lower_hi     // WRONG
shift += target - gap                            // ⇒ realized = target + shift_upper
```

Both staves move, so the correct relation is
`shift_lower = shift_upper + target - (upper_lo - lower_hi)` — the **unshifted**
gap. Every pair below the first was over-separated by exactly the shift above it.
It was invisible on two-staff fixtures (`shift_upper = 0`) and invisible to
`inter_staff_shifts_cascade_down_three_staves`, which asserted only `s2 > s1` —
true under both the correct and the double-counting recurrence.

**What caught it was the metric.** Measuring the realized clearance back from the
BAKED output, rather than reading the solver's own `staff_ext`, is what made an
independent check possible; had the axis read back solver intent it would have
reported 0 and the defect would have shipped again. That design choice was made
one commit earlier for exactly this reason, and it paid immediately.

**Locking it.** Once the solve realizes each declared clearance exactly, every
inter-staff unit is `0` on a healthy solve — the axis becomes a *solver
self-check*, and its mean can no longer distinguish "measured every realization"
from "measured one". So `vertical_raw` is split into `vertical_units`, and the
regressions assert the **unit set**: `a_glyphless_staff_band_still_contributes_an_
inter_staff_unit` (2 units, one per system) and `each_system_realizing_a_gap_band_
contributes_its_own_unit` (2 units for the wrapping fixture, 2 for the three-staff
cascade), each unit ≈ 0. Four mutations verified: the double-counting recurrence,
expand-only, the glyph-`members` band filter, and first-system-only measurement
each fail a named test.

**Churn:** the two multi-staff engrave goldens only. `two_staff_close_content`
grew by exactly 3.0 (the target change, no cascade); `three_staff_close_content`
*shrank* by 9.06 — the same +3 per pair, less the 17.06 of over-separation the
cascade defect was adding. Single-staff and every stub golden are byte-stable.

**Still open:** genuinely staff-less content placed *between* two staves. No
primitive can name an `InterStaffGap` band today, so it remains unreachable
rather than latent.

## W1: casting-off publishes the per-system partition it already computes (Editor T4-pre, `CONTRACT_EDITOR_T4PRE_IR.md`, 2026-07-24)

`cast_off` already knew which system every glyph, stroke, and curve landed
in (`system_of_slot`/`stroke_system`/`curve_system`, private to `CastLayout`)
— it just never left the module. Three changes make it leave:

1. **`glyph_system: Vec<Option<usize>>` joins `CastLayout`**, computed once
   in the same pass that bakes glyph positions (the glyphs-building `map`
   that already reads `system_of_slot.get(&glyph.horizontal_slot)` for
   `dx`/`dy` now also records that same lookup's result per glyph, via
   `.unzip()`, instead of computing it twice). This is the single
   glyph→system derivation pin 7 asks for: the quality census (`quality.rs`,
   `census`) now reads `cast.glyph_system[index]` and no longer re-derives
   the answer from `cast.system_of_slot` itself.
2. **The index partition** (`owned: Vec<PrimitiveIndices>` plus one
   `unowned: PrimitiveIndices`) is built from `glyph_system`/`stroke_system`/
   `curve_system` right before `resolved_systems` is constructed, and handed
   to each `build_system` call (which now takes a `PrimitiveIndices`
   parameter — pushing it to 8 arguments, `#[allow(clippy::too_many_arguments)]`
   added alongside the crate's existing precedent on `walk_region`) and to
   the returned `CastLayout.unowned`. `epiphany-layout-ir`'s solve-report fold
   (`lib.rs`) copies `cast.unowned` straight into `ResolvedLayoutIR.unowned`
   with no further computation; the `None`-cast branch (structurally invalid
   input) gets `PrimitiveIndices::default()`, consistent with its empty
   glyphs/strokes/curves/pages.
3. **No primitive changes shape.** The three flat arrays (`glyphs`, `strokes`
   after continuation-append, `curves` after continuation-append) are exactly
   what casting-off already produced; W1 publishes indices into them and
   nothing else — including for the synthesized continuation segments a
   system-spanning stroke or curve splits into, which were already being
   pushed onto `stroke_system`/`curve_system` keyed by their *own* landing
   system (`segments[k].0` inside the `Split` arm), not the source segment's.
   W1 did not have to fix this; it only had to publish it faithfully, and the
   mutation that deliberately breaks that faithfulness (attribute the
   continuation to the source's system instead) kills a dedicated test
   (`continuation_segments_are_owned_by_the_system_they_split_into`).

**A second, un-migrated re-derivation was found while doing this packet, and
closed rather than named.** `quality.rs`'s `vertical_raw` (the inter-staff-gap
axis) had its own local closure, `system_of_glyph`, independently re-deriving
a glyph's system from the slot map — the exact second copy of the attribution
rule pin 7 exists to prevent, just not the one m4 tests for. The packet's
first pass migrated only the census (pin 7 names it specifically) and recorded
this as an open finding; coordinator review closed it instead, because a pin
whose stated goal is "two copies drift, so make that impossible" is not met by
migrating one of two copies.

**And closing it made `CastLayout::system_of_slot` dead**, which is the real
lesson: with both consumers on `glyph_system`, nothing outside the casting
pass read the raw slot→system map any more. So the field is **removed** from
the published struct — the map stays a local inside `cast_off`, where the
stroke-fate walk still needs it. Pin 7 is now structural rather than
conventional: a future consumer *cannot* grow a third copy of the glyph→system
rule from `CastLayout`, because the raw material is no longer on it. The
per-glyph answer is published, the derivation is not.

**Byte-neutral:** see the `epiphany-layout-ir` DECISIONS.md entry of the same
date for the full canonical-encoding exclusion rationale (two consequences:
fingerprint fragility, and what a future cross-implementation relayout test
may and may not compare) — `casting.rs`'s `encode_canonical` path
(`epiphany-layout-ir/src/resolved.rs`) is unchanged by this tranche.

**Locked by** `primitive_ownership_partitions_every_flat_array_totally_and_
disjointly`, `attribution_correctness_matches_the_real_per_system_counts`,
and `continuation_segments_are_owned_by_the_system_they_split_into`
(`casting.rs`), each against the real engraver on real fixtures — the real
per-system glyph/stroke counts are `[26, 25]` and `[51, 45]` for the
`ten_measure_single_staff` two-system split, not asserted as `> 0`.

## ENGRAVER_VERSION 12 → 13: a barline ends its measure (X2.1, 2026-10-01)

The constrained stage drew each measure's barline at the measure's start
column and the last measure's at the region end, so a barline followed the
opening clef, the last two measures shared a bar, and casting-off, which broke
*at* barline columns, opened every later system with one and closed none. A
barline now stands where its measure ends (`MeasureContent::end`, the next
measure's start), a time signature takes a column of its own after the
barline at its measure's start, and casting-off breaks *after* a barline
(`opens_measure`): a system ends on its last measure's barline. Each measure
now has a measure record, the last included. Locked by
`a_barline_ends_each_measure_and_none_follows_the_opening_clef`.

## ENGRAVER_VERSION 13 → 14: span anchors (X2.3, 2026-10-01)

A beam spans columns but its ends sit on stems, offset from their columns'
slot sources, and both the spacing pass's coordinate map and casting's
justification affine move an off-source point by interpolation, which would
pull a beam off its stems whenever the columns between re-space. The
constrained IR now names, for such a primitive, the slots its two ends ride
(`SpanAnchor`), and the engraver moves each end by its own slot's delta (a
curve's inner control points keep their fractions of the span). An anchored
curve whose two ends land in different systems — a tie across a system break —
draws as two half-arcs, from its start to the first system's right edge and
from the second system's left edge to its end, each note end still on its
slot. Locked by `beams_stay_on_their_stems_through_spacing_and_justification`
and `ties_cross_barlines_and_system_breaks` (`epiphany-cli`), which fail with
the anchors ignored, or the half-arcs replaced by a geometric split.

## ENGRAVER_VERSION 14 → 15: system-start leads (X2.6, 2026-10-01)

The constrained IR is one strip per region with a single lead (clef and key)
at its start; casting broke the strip into systems and drew no clef or key
after the first. The constrained IR now carries, per staff, the lead in force
from each clef or key change on (`SystemLead`), and casting draws it at each
later system's left margin, from the staff instance's provenance under
`SYSTEM_LEAD_SYNTHESIS`, moves the system's music right of it, reserves the
widest lead in its break search, extends the staff lines under it, and counts
it in the inter-staff solve and the quality census. Locked by
`every_system_starts_with_its_clef_and_key` (`epiphany-cli`).

## ENGRAVER_VERSION 15 → 16: staff groups and the systemic line (X2.7, 2026-10-01)

Staff groups reached the constrained IR as anchors and nothing else; a
system's staves stood unjoined, and every barline stopped at its own staff.
The constrained IR now carries each staff group's kind, staves and joining
(`GroupSpan`), and casting draws, per system: a line opening any system of two
or more staves, from the region under `GROUP_SIGN_SYNTHESIS`; left of it, a
brace scaled to its staves, a bracket's thick line and ends, or a sub-bracket's
thin line and hooks, from the group; and, for all but a choral group, each
barline's lines continued from staff to staff under `JOINED_BARLINE_SYNTHESIS`.
A single-staff score is unchanged. Locked by
`groups_mark_their_staves_and_join_their_barlines` (`epiphany-cli`).

## ENGRAVER_VERSION 16 → 17: the gaps after a lead and a time signature (X2.7, 2026-10-01)

The spacing pass separates neighbouring slots' ink by one gap of 0.3 staff
spaces, and every slot's natural width was the same 1.5; the constrained
stage's wider gap after the clef and key reached the page only through a
barline that stood there in error, and when X2.1 removed it an opening time
signature stood 0.3 from the clef and the first note 0.3 from the time
signature. The lead's slot and a time signature's now take as natural width
their ink and the gap after it, 0.8 after a lead and 1.0 after a time
signature. Every score's first system moves right of its lead; later systems,
which casting opens with their own lead, are unchanged. Locked by
`the_opening_time_signature_clears_the_lead_and_the_music` (`epiphany-cli`).

## ENGRAVER_VERSION 17 → 18: voices (X2.8, 2026-10-01)

A note's stem turned by its pitches alone, and every rest sat at the middle
of the staff, so two voices on a staff crossed stems and stacked rests. The
logical IR now says where each event's voice stands among its staff's
(`VoicePlace`): a voice's stated stem direction places it throughout;
otherwise the staff's first voice is upper wherever another voice shows a
note or a visible rest during the event, and each later voice is lower or
upper by turns. Beside another voice, an upper voice's stems and beams turn
up and its ties and unauthored slurs arc above, a lower voice's turn down and
arc below, its dots on a line take the space below, and a rest moves a space
off its place, up for an upper voice and down for a lower. A staff of one
voice is unchanged. Locked by
`voices_turn_their_stems_rests_ties_and_dots_apart` (`epiphany-cli`).

## ENGRAVER_VERSION 18 → 19: accidentals by their ink (X2 fix round, 2026-10-02)

Every accidental stood a fixed 1.1 staff spaces left of its notehead, 0.9
further for each element of one spelling's stack, whatever its width, the
column's ledger lines or the column's other accidentals. A sharp, flat or
double sharp crossed a ledger line of its column, two accidentals of a chord
or of two voices a third apart lay over each other, and a double flat ran
through its own head. The constrained stage now places a staff column's
accidentals together, by each glyph's box: 0.2 left of every head of the
column and of every ledger line its height spans, and 0.15 clear of every
accidental placed before it, a column further out each time, from the
outside in (the highest, the lowest, the next highest), so the highest
stands nearest the heads; a spelling's stack stays together. The column's
overhang, which separates it from the column before, is how far its
accidentals reach. No `NoCollision` obligation names an accidental: a ledger
line is a stroke, which the obligation cannot name, so the positional test is
the check. Locked by `accidentals_stand_clear_of_their_column_and_close_to_it`
(`epiphany-cli`).

## ENGRAVER_VERSION 19 → 20: heads a second apart (X2 fix round, 2026-10-02)

Every head of a column stood at the column's x, so two heads a second apart,
in a chord or in two voices, lay half over each other, and the constrained
stage exempted heads sharing a column from `NoCollision` on the ground that
a second may overlap by design. The constrained stage now sets a staff
column's heads clear of each other before it places their accidentals. In a
chord, taken outward from the head the stem leaves, a head a second or a
unison from one on the stem's usual side goes across the stem, 0.02 clear of
it, so a cluster alternates; a beam turns its stems before this, since the
side follows the stem. Then each voice in turn, upper first, stands right of
the heads before it that its heads would touch, its stem, beams and ties
moving with it, so a lower voice a second under an upper one stands to its
right with their stems in one line, as MuseScore sets it; a unison two voices
share (one glyph, one alteration, one count of dots) stays one head drawn
twice. A displaced head draws its own ledger lines, ties leave and meet it,
and the dots of a column stand right of all its heads. Within a column,
every two heads whose boxes share height, but a shared unison, now carry a
`NoCollision`, so the solve's validation sees a pair the placement failed to
part. Locked by `seconds_stand_either_side_of_the_stem` (`epiphany-cli`).

## ENGRAVER_VERSION 20 → 21: a tie continued into a system (X2 fix round, 2026-10-02)

A tie across a system break drew its second half from the second system's
left content edge, in the spaced frame, which has no lead; casting then
placed the lead before the content, so the half started at the system's
first column, ran from there to just before its note, and kept the whole
tie's lift. Where the note was the column's leftmost ink it drew as a mark
about 0.15 staff spaces long, or none at all, before its head. The second
half now starts 0.4 clear of the system's lead (the widest staff's ink, as
every staff's half starts at one x), or of the columns before its note that
hold no note (a time signature opening the system), and arcs by the tie
rule for its own length (`tie_arc`, shared with the constrained stage's
ties). It runs at least 1.5 staff spaces: where the gap after the lead
leaves less, the system's lead widens by the difference, the break search
reserves that room with the lead at each slot a system could open on, and
the opening columns keep their place after the lead, so the room stands
before the note. The first half is unchanged. Locked by
`a_tie_continued_into_a_system_starts_clear_of_its_lead` (`epiphany-cli`),
and the break search's reservation by
`optimal_breaks_reserves_the_room_a_continued_tie_needs` and
`the_overflow_net_reserves_the_room_a_continued_tie_needs`.

## ENGRAVER_VERSION 21 → 22: systems within the margins, staff lines to the closing barline (X3.0, 2026-10-03)

The break search measured a system by its columns' ink, while the system's
extent, which justification fills to the content width, also holds each
staff line's half-thickness at both ends and, on a region's first system, the
staff lines' start about a space left of the clef. A system the search fitted
with less slack than that ran past the right margin, and justification, which
only stretches, left it there. The search now reserves the widest staff or
ledger line's thickness from the width, and counts the first system from its
staff lines' start; the overflow net does the same. Separately, a region's
last system kept its staff lines where the spacing map extrapolated them,
which stopped them between a final barline's thin and thick lines. Each
system's staff lines now end at the right edge of the barline that closes it.
Locked by `a_score_takes_its_files_page_and_keeps_within_its_margins` and
`staff_lines_end_with_the_barline_that_closes_their_system` (`epiphany-cli`).

The command-line tools now cast a file onto the page its MusicXML
`<defaults><page-layout>` gives, in staff spaces (a tenth is a tenth of a
staff space), rather than the default A4 at an 8 mm staff: the page its
writer drew it on, so the two read side by side. The score graph does not
hold the page; `epiphany_cli::geometry` reads it from the source, and a file
without one takes the default.

## ENGRAVER_VERSION 22 → 23: ties clear of the ink beside their heads (X3.0, 2026-10-03)

A tie ran from 0.15 right of its head to 0.15 left of the next, whatever
stood beside them, so once a second's head stood across the stem, or another
voice's head beside it, a tie leaving or meeting the head next to it ran
through that head, the stem or the column's ledger lines; a tie leaving a
dotted note on a line ran through its dot; one arriving beside a chord-mate's
accidental ran through it. The constrained stage now gathers each staff
column's ink (heads where they are set, ledger lines, stems, dots,
accidentals), and a tie starts 0.15 right of every box of its first column
that meets its height (0.2 above and below its end) and does not stand wholly
left of its head, and ends 0.15 left of every such box of its second column,
accidentals included. Its ends ride their own slots, so the constrained
frame, where columns stand closer than the spacing will set them, no longer
clamps the end after the start. The spacing pass now gives every tie anchored
to two slots at least a staff space between its ends (`TIE_MIN_SPAN`), as a
least distance between those slots. A split note's own tie now takes its
side as a tie between two notes does, by its voice beside another, then its
place in the chord, then its stem (`tie_above`); it took it from the stem
alone, so in a two-voice staff each voice's split ties arced toward the
other. A measure record's edges now move with their slots' own deltas, as
the slots' glyphs do, not through the justification's affine, which set an
opening time signature's measure up to (a − 1) times its ink's offset right
of the ink. Locked by `a_tie_stands_clear_of_the_ink_beside_its_heads` and
`a_split_notes_tie_takes_its_voices_side` (`epiphany-cli`).

## ENGRAVER_VERSION 23 → 24: rests clear of another voice's notes (X3.0, 2026-10-03)

Beside another voice a rest moved one space off its place, up for an upper
voice and down for a lower, whatever the other voice held there, so a lower
voice's rest under an upper voice's low chord stood on its heads. A rest now
keeps moving a space at a time the same way until its glyph stands 0.25 clear
of every head of another note on its staff that starts with it, the notes
that stand in its column (MuseScore's rule, as its rests avoid the chords of
their segment). A note held from before stands to the rest's left and does
not move it; so a held note's tie can still pass over a rest beside it,
which this does not address. Locked by
`a_rest_stands_clear_of_another_voices_notes` (`epiphany-cli`).

## ENGRAVER_VERSION 24 → 25: the accidental after a tied one (X3.0, 2026-10-03)

A note a tie continues into shows no accidental and left its measure's state
as the key gave it, so after an F sharp tied over the barline a later F
natural in the bar stood bare: right by the strict rule, since the tie
carries the sharp to the tied note alone, but read as ambiguous, and
MuseScore prints the courtesy natural. Where the tied note's alteration is
not what its measure gave, its letter and octave are now marked as carried,
an alteration no note has, so the next note of that letter and octave shows
its own accidental: a courtesy natural, or the tied one's restated, as
Gould restates it after a tie. Locked by
`accidentals_are_drawn_against_the_key_and_the_measure` (`epiphany-cli`).

## ENGRAVER_VERSION 25 → 26: tuplets drawn (X3.2, 2026-10-03)

A tuplet reached the constrained IR as a traced anchor, and its members, which
the pre-pass decomposes to notated values through the tuplet's ratio, were
left unbeamed (a beamable note had to be outside a tuplet). The logical IR now
gives a tuplet its content (`TupletContent`: ratio and members), and the
constrained stage draws, for a tuplet whose members share one staff, its
number (SMuFL's `tuplet0`–`tuplet9`, now bundled, showing the ratio's actual
term) centered on its span from its first member's column to its last's. It
stands 0.5 clear of the ink of every column between (heads, stems and beams
on that staff, and its member rests): above for an upper voice, below for a
lower, and for a voice alone on the side most of its members' stems point,
above when none has a stem. A bracket of 0.16, gapped for the number, with
0.6 hooks toward the notes, is drawn unless the members are notes beamed
together as exactly one group — MuseScore's automatic rule, which its
exported `bracket` attributes follow. The number rides its middle
member's slot, the bracket's ends their members' slots. Tuplet members are now
beamable. A tuplet across staves still draws nothing. Also: `gClef15ma`,
`gClef15mb`, `fClef15ma` and `fClef15mb` are bundled and drawn for a clef two
octaves up or down, which drew without its mark. Locked by
`a_tuplet_draws_its_number_and_a_bracket_unless_beamed_alone`
(`epiphany-cli`); the rich fixture's golden gains its tuplet's number and
bracket.

## ENGRAVER_VERSION 26 → 27: the decomposition's version 2 (X3.3, X3.4, 2026-10-03)

What the engraver draws for a note's rhythm comes from the decomposition
pre-pass in `epiphany-core`, which version 2 of the default algorithm changes
(core specification `req:time:decomposition-algorithm`; `epiphany-core`'s
`DECISIONS.md`): bars from each staff instance's measures, so a note after a
meter change is written against its own bar; a pickup's notes keep their place
in the bar; and a span one value expresses within a bar is that value, up to
two dots. Locked by `notes_take_their_values_from_every_meter`
(`epiphany-cli`) and the pre-pass's unit tests.

## ENGRAVER_VERSION 27 → 28: clef changes drawn (X3.6, 2026-10-03)

A staff instance's clef sequence held every change the importer read, and the
notes after a change were already placed by the clef in force, but only a
system's lead showed a clef, so a change inside a system was invisible. Each
change now stands in a `Clef` column of its own at its time, ordered before
the barline and notes there: mid-measure it is drawn just before its note; at
a measure's start, before that measure's barline, so when the measure opens a
system the change ends the system before as a courtesy and the new system's
lead shows the clef. The glyphs are SMuFL's change-size clefs (`gClefChange`,
`fClefChange`, `cClefChange`); SMuFL has no change-size octave clef, so an
octave clef draws its `clef8` or `clef15` numeral centred over or under the
clef, reaching 0.1 staff space into its box as Bravura's own octave clefs do.
A change restating the clef in force draws nothing. The column reserves the
clef's ink and a 0.5 gap; in the source geometry the next column clears it.
Key changes inside a system are still not drawn (the lead shows the key in
force at each system's start). Locked by
`clef_changes_are_drawn_where_they_take_effect` (`epiphany-cli`); the omission
census now counts only real changes and finds a system's lead clef as its
leftmost.

## ENGRAVER_VERSION 28 → 29: a pickup's rests keep their values (X3.5, 2026-10-03)

X2 drew a rest that fills its measure as a whole rest in any meter, which is
the measure rest's convention; with pickups imported, a first measure shorter
than its bar is a measure too, so a quarter rest filling a one-beat pickup
drew as a whole rest. An exporter writes a pickup's rests at their values,
and the music reads them so. `measure_spans` (layout-ir) now leaves a first
measure shorter than its bar out of the measure-rest rule, its bar the
measure's own signature or the region's meter in force there; the omission
census applies the same rule, so the two cannot agree on the wrong glyph, as
they did. A mid-score short measure is still not modelled. Locked by
`a_rest_filling_a_pickup_keeps_its_value` (`epiphany-cli`).

## ENGRAVER_VERSION 29 → 30: a tuplet's bracket rides inked columns (X3.2, 2026-10-03)

A spring slot is realized only by a glyph in its column, and a hidden rest
keeps its column but draws nothing. A tuplet whose first or last member was a
hidden rest, alone in its column, anchored its bracket's end to that column's
slot, which did not exist: the constrained layout then failed validation
(`DanglingSpanAnchor`) and the engraver returned `InternalError` with no page
at all, for the whole score. The bracket's ends now ride the first and last
columns its members draw in; the number keeps its middle member's column,
which its own glyph realizes. Locked by
`a_tuplet_opening_on_a_hidden_rest_still_engraves` (`epiphany-cli`).

## ENGRAVER_VERSION 30 → 31: a stem rides its own heads' slot (X3 fix round 1, 2026-10-03)

A stem has no glyph of its own source (its heads are traced to pitches, it to
the event) and stands offset from its column, so the spacing pass and casting
gave it the slot of the glyph with the greatest baseline at or before its x,
on any staff. That glyph was usually a head of its own column, but not
always: a beamed sextuplet's number stands between its third and fourth
notes, in the fourth's column, just left of the third's up-stem; a chord's
displaced head on another staff can stand just right of the next column's
head; a volta's ending number, the next note's accidental, a change clef.
The stem then moved with that column, and wherever spacing or justification
moved the two columns apart it stood beside its head, either side, with its
head drawn stemless. The constrained stage now gives every stem a
`SpanAnchor` naming its heads' slot at both ends, and the solver, which reads
anchors first in the spacing remap, in casting and in assigning a stroke to
its system, moves the stem with its heads by identity. Only a stem whose
column drew no glyph would go unanchored, since its slot would not exist; a
pitched note has at least one pitch, so its heads always realize it. The
nearest-glyph rule is otherwise left only for zero-extent traced anchors,
which draw nothing. Locked by `a_stem_stands_on_its_own_heads` (`epiphany-cli`) and
`stem_offsets_from_the_notehead_survive_justification`, which now includes
the repeats fixture; the repeats golden's second ending had one such stem.

## ENGRAVER_VERSION 31 → 32: digits at their own advances, and an ending's bracket on its slots (X3b.0, 2026-10-05)

Digits set side by side advanced a fixed 0.8 staff spaces, though a
time-signature digit is 1.3 to 1.9 wide, so a time signature of several
digits (12/8, 11/8, 17/16) drew them over each other, and an ending for two
passes drew its "2" and "3" as one blot. Each digit now advances by its own
metric width. A signature whose lines are single digits keeps both on one
origin, as before, so no single-digit signature moves; where a line has
several digits, each line is centred under the widest, which starts where a
single digit would. An ending's numbers take a comma between them
(`timeSigComma`, now bundled), seated on the digits' bottom edge so it
reaches no lower than they do and stays clear of the staff.

The ending's numbers ride the slot of the column their bracket opens at, but
the bracket's line and hooks were mapped through the spacing remap, which
puts a column that spacing widens (an end repeat's) somewhere else: the
second ending's hook stood a space right of its barline, through its "2".
The line and both hooks now carry `SpanAnchor`s on the slots of the columns
they open and close at, so the bracket, its numbers and the barline it
starts at move together. Locked by
`an_endings_numbers_stand_apart_inside_their_bracket` (`epiphany-render-svg`)
and `a_time_signatures_digits_stand_apart_and_centred` (`epiphany-cli`). The
repeats golden is re-blessed (its second ending reads "2, 3", clear of its
hook); no other golden changes, since none has a volta or a signature of
several digits.

## ENGRAVER_VERSION 32 → 33: quarter-tone accidentals drawn (X3b.2, 2026-10-05)

The importer spells a quarter-tone with the accidental its file writes, by its
MusicXML name (`flat-up`, `quarter-flat` and the rest), and the engraver had
no glyph for any of them, so each was surfaced as `UnbundledGlyph` and left
undrawn. The fourteen MusicXML names now draw from bundled SMuFL glyphs, as
MusicXML assigns them: the ten arrowed ones from SMuFL's Gould arrow
accidentals (`accidentalQuarterToneFlatArrowUp` for `flat-up`, through
`accidentalFiveQuarterTonesFlatArrowDown` for `flat-flat-down`), Stein's
quarter and three-quarter sharps and quarter flat from Stein's glyphs, and the
three-quarter flat from Zimmermann's (`accidentalThreeQuarterTonesFlatZimmermann`).
Their boxes come from the regenerated metrics, so they join the
accidental columns as any other accidental does; several reach left of their
origin, or two spaces above or below their head, with the arrow. Locked by
`each_quarter_tone_accidental_draws_its_smufl_glyph` (`epiphany-cli`) and the
glyph table's unit test.

## ENGRAVER_VERSION 33 → 34: quarter-tones in the measure's accidental state (X3b.3, 2026-10-05)

X2's measure state counted alterations in semitones and left out any spelling
whose accidental was not a whole number of them, so a quarter-tone drew its
accidental on every note, carried or not, and a natural after one on its
letter was not shown. The state now counts quarter-tones
(`stack_quarter_tones`: a standard accidental two a semitone, a quarter-tone
accidental by its MusicXML name, over exactly the accidentals
`accidental_glyph` draws), the key's alteration doubled. A quarter-tone
accidental holds to the barline on its letter and octave across the staff's
voices, a change after it (a natural, the key's flat, another quarter-tone) is
shown, and the same alteration stated again, by either notation, is not. A
whole-semitone alteration draws its standard glyph; a quarter-tone one draws
the spelling's own, arrowed or Stein's as the file wrote it. A tie's carry
works in the same units. The head's alteration, which decides whether two
voices' heads on one step are one head drawn twice, is counted the same way:
two different quarter-tones there both read as unknown before, and so shared
a notehead. Simultaneous notes in two voices are still taken in the order
their objects come, as before, so a quarter-tone in one voice and a natural
on its letter in another at one instant show only the second's accidental
when the first comes first. Locked by
`a_quarter_tone_accidental_holds_to_the_barline_and_yields_to_a_change`
(`epiphany-cli`).

## ENGRAVER_VERSION 34 → 35: a unison of two alterations (X3c.0, 2026-10-06)

The measure's accidental state took the notes of one instant in their
objects' order, so where two voices sounded one letter and octave at once
with two alterations (a natural beside a flat, a natural beside a
quarter-tone), the first, which the key or the measure already gave, showed
nothing, and the second showed its own. The two heads stand side by side
behind that one accidental, and a player reads both as the second. The
notes starting at one time are now taken together: where they sound a letter
and octave with more than one alteration, each head shows its own
accidental, the natural included, whatever the measure gave, and the next
note there states its own, as after a tie of another alteration. A tie
continuation in such a unison shows its own too: bare beside a flat, it
read as flat, since the one accidental stood before both heads. Where every
note of the unison is a tie continuation, each is read by its own tie and
none shows, as before. One alteration in both voices is shown once, as
before. The accidental columns already set two accidentals of one step
apart, the one nearer the heads belonging to the left head. Locked by
`a_unison_of_two_alterations_shows_both_accidentals` (`epiphany-cli`).

## ENGRAVER_VERSION 35 → 36: beams across two staves (X3c.2, 2026-10-06)

A beam joining one voice's notes on both staves of a part, as MuseScore
writes the notes it moves to the other staff, was dropped: the logical stage
kept only beams whose notes all stood in one staff instance, so every note of
such a beam took a flag. Each run of a score's beam that joins notes of two
staff instances of a region is now a group of the staff of its first note
(`StaffContent::cross_beams`), split where a note cannot be beamed, as a
one-staff beam is. The constrained stage turns the upper staff's stems down
and the lower's up, to one beam between the staves: its primary beam
topmost, further beams below it as a group whose stems all turn up draws
them, so an up stem reaches the primary and a down stem the deepest beam its
own value takes. It rises with the line from its first member's head to its
last's, by at most `MAX_BEAM_RISE` and at most half a space per space of run
(the stems at a knee stand close, an up stem right of its head beside a down
stem left of the next), and stands midway between the heights its members
allow, each stem at least `CROSS_STEM_MIN` (2.25) from its head to the
nearest beam it meets. It is drawn in the constrained frame, riding the upper
staff, and recorded in `ConstrainedLayoutIR::cross_staff_beams` with its
ink, the stems reaching it from the lower staff, and how far the lower staff
may rise toward the upper before such a stem falls short of its least length
(`rise_limit`).

Casting reads the record three ways. The beam's ink is neither staff's
content in the inter-staff solve, nor in `vertical_density_penalty`'s
measurement, since it stands between them; the solve instead keeps the lower
staff from rising past `rise_limit` relative to the upper, a floor beside the
band's preferred clearance, in the system the beam lands in; and the bake
moves each reaching stem's beam end with the upper staff's shift while its
head end keeps its own. The spacing pass gives a beam across two staves at
least 1.5 spaces between the stems at its ends (`CROSS_BEAM_MIN_SPAN`), as it
gives a tie its least span. Nothing on one staff moves: a one-staff beam's
strokes keep their keys, and a cross-staff beam's are keyed apart (the top
bit, its level, its run and its first member). A group whose drawn members
stand on more than two staves is not drawn; none is known. Locked by
`a_beam_across_two_staves_joins_its_notes_between_them` (`epiphany-cli`),
over a hand-written piano fixture and a score whose staves would close past
the beam's room.

## ENGRAVER_VERSION 36 → 37: a tuplet by its beam across two staves (X3c.3, 2026-10-06)

A tuplet whose notes stand on two staves has no staff in the logical stage,
so it drew nothing. One whose notes are exactly a beam across two staves now
draws its number by that beam, and no bracket: centred on the group, above
the beam, where only the upper staff's stems come down to it, moved along
the beam to the place nearest the middle that no stem on that side crosses;
below only where no such place lies within the group's span and below needs
the shorter move. It stands `TUPLET_CLEARANCE` off the beam, rides the upper
staff and the slot of the member nearest it, and joins the beam's ink
between the staves. MuseScore sets such a number above the beam beside the
knee, as this does. A tuplet across two staves that is not one beam still
draws nothing. Locked by
`a_tuplet_on_a_beam_across_two_staves_takes_its_number_by_the_beam`
(`epiphany-cli`).

## ENGRAVER_VERSION 37 → 38: hidden tuplets (X3c.4, 2026-10-06)

A tuplet the file hides drew its number and bracket, since the score had no
field to hold the marking; schema major 4 gives it one (`Tuplet::display`).
A tuplet whose number is hidden now draws none, and one whose bracket is
hidden draws none; a bracket whose number is hidden runs unbroken from hook
to hook, one stroke riding its first and last members' slots. Where no digit
carries the tuplet's own provenance, its traced anchor does, as when nothing
is drawn. A tuplet on a beam across two staves draws its number by the beam
only when shown. Locked by
`a_tuplet_the_file_hides_draws_no_number_or_bracket` and
`a_tuplet_on_a_beam_across_two_staves_takes_its_number_by_the_beam`
(`epiphany-cli`).

## ENGRAVER_VERSION 38 → 39: tuplets across two staves that are not one beam (X3c fix round 1, 2026-10-06)

A tuplet whose members stand on two staves has no staff in the logical
stage, and version 37 drew one only where its notes were exactly a beam
across them; any other shape drew nothing, number and bracket alike, and
nothing counted it. Three shapes now draw. Tuplets sharing one beam across
the staves each take their number by the beam as version 37 placed it,
centred on their own notes, with no bracket, as MuseScore draws them. A
tuplet a drawn rest opens, or holds, takes a bracket as well, since the beam
does not show where it runs: from its first member's column to its last's,
above the beam, its notes' own ink and its rests by `TUPLET_CLEARANCE`, its
number in the gap and its hooks toward the notes, riding the upper staff with
the beam's ink, as MuseScore draws one. The bracket clears the tuplet's own
notes and not another voice's. A tuplet whose ink stands on one staff, a
hidden rest on the other, is drawn on that staff as a one-staff tuplet
opening on a hidden rest is, clearing that staff's rests alone. A tuplet
across two staves whose notes no beam joins still draws nothing; the
corpus report now counts every tuplet whose number is shown and has no ink,
or a bracket without its number, as an engraving omission. A tuplet across
two staves that draws nothing keeps its traced anchor in no staff's band:
versions 37 and 38 left a hidden one that is exactly its beam in the upper
staff's band at the frame's origin, which stretches that staff toward
whatever stands there; once every tuplet riding such a beam took the beam's
staff, the hidden ones of a passage with a staff above the piano moved its
staves apart. Locked by
`a_tuplet_across_two_staves_that_is_not_one_beam_is_drawn` and
`a_tuplet_across_two_staves_that_draws_nothing_keeps_its_anchor_off_the_staves`
(`epiphany-cli`) and, for the count,
`a_shown_tuplet_with_no_ink_is_counted_and_an_anchor_is_not_ink`.

## ENGRAVER_VERSION 39 → 40: a cross-staff tuplet's number clear of heads (X3c fix round 1, 2026-10-06)

Version 37 placed the number of a tuplet on a beam across two staves at the
place nearest its notes' middle that no stem on its side crosses, and
cleared nothing else. In a falling group the upper staff's notes come
first and descend toward the beam, so that place is beside the last upper
note, under its head: the number stood within a third of a space of a head,
and could meet one. The number now clears the upper staff's ink, every
head, ledger line, accidental, dot, stem and drawn rest, by
`TUPLET_NUMBER_GAP` beside it, as it cleared stems, and by
`TUPLET_CLEARANCE` above or below, as it stands off the beam; it moves along
the beam to the nearest place that does, within the beam's span. In a
falling group that is past the last upper note's head and ledger lines,
still above the beam and within the group. MuseScore lifts such a number
over the head instead, onto the ledger lines; standing by the beam keeps it
with the group. The lower staff's ink is cleared by `x` alone, and only by
a number below the beam: the placement runs in the constrained frame, where
the staves stand a fixed pitch apart, and where the upper staff's notes
stand deep enough to push the beam below the lower staff's place there, the
lower staff's heads and ledger lines stand above the beam until the vertical
solve opens the gap; read as they stand, they sent such a group's number
below the beam, into the lower staff. Only the upper staff's ink rides with
the beam. Where neither side of the beam has a clear place, the number
stands at the middle above all the upper staff's ink it would meet. A
rising group's number, already clear, is placed as before, and moves only
with the spacing a moved number changes around it. The bracket of a tuplet
a rest opens now rises over the beam and its upper staff's notes and rests
alone, for the same reason, and is raised further where its number would
meet the upper staff's ink. Locked by
`a_tuplet_number_across_two_staves_stands_clear_of_heads` (`epiphany-cli`),
over a hand-written fixture of falling groups, some far below each staff.

## ENGRAVER_VERSION 40 → 41: a shared beam's number over its own notes (X4a.0, 2026-10-07)

Version 40 moved a cross-staff tuplet's number along the beam to the clear
place nearest its notes' middle, anywhere within the beam's span. On a beam
two tuplets share, a number that found no clear place over its own notes
could therefore settle over its neighbour's, reading as theirs. Its search
now runs over its own notes' span alone; where nothing there is clear, it
stands at its middle above the upper staff's ink, as it already did where
the whole beam had no clear place. A tuplet that is exactly its beam is
placed as before, its notes' span being the beam's.
Locked by `a_shared_beam_number_stays_over_its_own_notes`
(`epiphany-layout-ir`), which plants ink over the first group's notes on
both sides of the beam and none over the second's.

## ENGRAVER_VERSION 41 → 42: no key before a staff's first (X4a.3, 2026-10-07)

The key in force at a time was the latest change at or before it, else the
staff's earliest change, so a staff whose first key signature comes after
its start was read in that key from its start: its lead drew it and its
notes were spelt against it. The importer now holds an open key
(`<mode>none</mode>`) where no key is in force as no key signature, which
made such staves common in exported scores, so the key before a staff's first
is now none. A staff whose first key stands at its start, every one the
importer made before, is unchanged, and so is one whose open key the
importer held as C before. Locked by
`a_staff_has_no_key_before_its_first` (`epiphany-cli`).

## ENGRAVER_VERSION 42 → 43: key changes drawn, with their cancellation (X4a.4, 2026-10-07)

A key change showed only in a later system's lead, which reads the key in
force at its start; one inside a system drew nothing, though its notes were
spelt against it (D33). It now draws in its barline's column, after the
barline's ink (a repeat sign's included) and the key gap: first a natural at
the place of each accidental of the old key the new one does not keep, then
the new key's accidentals, all `KEY_ACC_X` apart. A new key on the same side
keeps the old one's first accidentals, so only those past its count are
cancelled; a change of side, or to no key, cancels them all; a larger key on
the same side cancels none. Placing it in the barline's column rather than a
column after it means a change a system break falls at ends the system before,
after its closing barline, as a courtesy, the staff lines running under it,
while the new system's lead shows the new key alone, with no change repeated
at its start. The column after clears the change as it clears a clef change,
and the barline column reserves its ink only where a change stands in it, so
no other barline moves. A change inside a measure takes a barline column of
its own with no barline, which is never a break. A restatement draws nothing.
Locked by `a_key_change_is_drawn_with_its_cancellation` and
`a_staff_has_no_key_before_its_first` (`epiphany-cli`).

## ENGRAVER_VERSION 43 → 44: a slur or tie clears the accidentals under it (X4a.6, 2026-10-08)

A slur was shaped over the heads and stems of the columns it spans, and a tie
between its heads, but neither looked at an accidental away from the tie's own
ends, so a slur ending on a note with an accidental ran through it, and one
passing a note whose accidental reached past its head ran through that (D42).
The constrained pass shapes curves where columns stand closer than spacing
will set them, and an accidental keeps its distance from its head while the
curve stretches, so the clearance is taken after spacing, in the frame the
page is drawn in (`clearance::clear_accidentals`, from `curves`): each
accidental glyph of the curve's band that the curve stands past the near edge
of, and short of the far edge plus `ACCIDENTAL_CLEARANCE` (0.2), is passed on
the side the curve arcs to, the worst first. In the curve's middle the arc
rises, both inner control points moving off the chord alike, up to a third of
the span and at most four spaces; within a fifth of a slur's end, where the
arc rises too slowly, or past that bound, the slur lifts that end, its inner
points moving with the chord. A tie's ends stay at its heads, so a tie only
raises its arc, to at most 1.5 spaces off its chord, and a tie that cannot
pass every accidental it meets within that is left as it was rather than half
raised: a long tie under another voice's notes, which MuseScore draws
through them as well, keeps its shape. Key-signature glyphs are accidentals
too, so a curve across a key change passes them. Locked by the module's unit
tests and `a_slur_clears_the_accidentals_under_it` (`epiphany-cli`); no golden
draws a curve through an accidental, so none changed.

## ENGRAVER_VERSION 44 → 45: time columns spaced by their durations (X5c.1, 2026-10-10)

Every time column had the same natural width (`COLUMN_PREFERRED_WIDTH`, 1.5
spaces), and the spacing pass kept between two columns at least the widest
ink any staff had in the first plus the widest overhang any staff had in the
second, so a half note and an eighth took the same room, and in a full score
every onset of any staff, a septuplet's beside a sextuplet's, took a whole
column's width; justification then stretched every gap of a system alike,
and a region's last system stayed ragged however full. Four changes,
together:

- A note column's spring takes the width its durations ask
  (`duration_widths` in `epiphany-layout-ir`'s constrained stage): the
  shortest duration `s` sounding through the column, any staff's note or rest
  that has begun and not ended, asks `QUARTER_SPACE · (s / quarter)^log2(1.5)`
  for its whole length, each doubling half again as wide; a column that holds
  only part of it, because another staff's note begins before it ends, takes
  its part in proportion, `dt / s`. The ratio is MuseScore's default
  ("measure spacing", 1.5). The scale is fixed, a quarter's column 5 spaces,
  chosen by measuring the systems it casts against MuseScore's own; MuseScore
  instead takes each system's shortest note as its unit, so a full score,
  whose systems nearly always hold a short note somewhere, spreads wider
  there than here, while a measure's natural width here does not depend on
  where the line breaks. The power is taken from square roots and products
  (`ratio_power`), which IEEE rounds exactly, rather than a library `powf`,
  which need not agree across platforms. The constrained stage's own frame
  steps a note column by that width too, where it is more than
  `COLUMN_X_STEP`, so the slurs, ties and beams shaped there keep their
  proportions once spaced (a slur's arc is a share of its span, and with
  columns 1.6 apart and the spacing setting them wider, every slur came out
  flatter than its rule); the stub solver, which draws that frame, changes
  with it.
- The spacing pass clears earlier ink by its height (`spacing::space_slots`):
  each glyph, ledger line and slot-anchored stroke (a stem) widens its slot's
  extent in every quarter-space band its height reaches, give or take
  `SKYLINE_MARGIN`, and a slot stands clear of the rightmost ink already set
  in each band its own ink reaches by the clearance that ink's slot asks: a
  note column's `SLOT_GAP`, a barline's `BARLINE_CLEARANCE` (MuseScore's
  distance from a barline to an accidental), and a lead's, signature's or
  change's the gap its spring reserves after its ink, so an accidental after
  a time signature keeps the signature's whole gap. Columns whose ink stands
  on different staves may stand as close as their onsets ask; a stem reaching
  another staff's beam is cleared where it reaches.
- A justified system's slack goes to its note columns by their springs
  (`stretch_factor · preferred_width`; the lead's, a signature's, a change's
  and a barline's springs have `stretch_factor` 0), through a piecewise map
  (`Placement`), not one affine stretch: a note keeps its distance after its
  barline, and the durations keep their proportions, however much a system
  stretches.
- A region's last system is justified like the others once its natural ink
  fills `LAST_SYSTEM_FILL` (three tenths) of the content width, MuseScore's
  default ("last system fill threshold"); a shorter one stays ragged.

Locked by `columns_space_by_the_durations_sounding_through_them`,
`ink_on_one_staff_need_not_clear_ink_on_another`,
`a_justified_system_stretches_its_note_columns_and_not_its_barlines`,
`an_accidental_keeps_the_gap_a_signature_or_barline_asks` and
`a_last_system_filled_past_three_tenths_is_justified` (`epiphany-cli`, on the
hand-written `spacing.musicxml` and generated parts), and the constrained
stage's `ratio_power` and `duration_widths` unit tests. `spacing_distortion`,
the quality catalog's spacing axis, measures how far a system's rhythmic
advances depart from even, so it now reports the proportions duration
spacing makes on purpose; a duration-relative definition is the catalog's
open question (quality decision 8) and a change to the catalog, not made
here. Every engraving golden changed and was re-blessed with renders before
and after for the owner's reading.

## ENGRAVER_VERSION 45 → 46: a joined barline reserves its line in the gap (X5c.1, 2026-10-10)

Version 45's skyline took a barline's extent from its glyph on each staff,
while a group whose barlines join from staff to staff has the casting stage
draw each joining line across the gap between two staves, after spacing.
Ink in the gap after a barline, mostly an accidental on a note above or
below its staff, then stood only as far from the line as from earlier ink
there, and could stand on it: the column-wide clearance of version 44 had
kept it off. `spacing::space_slots` now reserves each joining line in its
barline slot's extent, in the gap between each two staves of a joined
group, from the lower staff's barline to the upper's, at each line's
thickness as `casting::barline_lines` gives it, so ink in the gap keeps a
barline's clearance as on the staff. Locked by
`an_accidental_in_a_gap_keeps_clear_of_the_barline_joined_across_it`
(`epiphany-cli`, a piano part whose bass staff's E-flat on its second ledger
line opens a measure). Two goldens moved, `notation` by at most 0.008 of a
space and `arrow_accidentals` by less, where a joining line's band within
the skyline's margin of a staff is a hair wider than the barline glyph's
box; re-blessed with renders before and after for the owner's reading.

## ENGRAVER_VERSION 46 → 47: a long tie passes the accidentals in its path (X5c.2, 2026-10-10)

Version 44 passed a tie over the accidentals of its staff by raising its arc
as far as `MAX_TIE_ARC`, 1.5 spaces, and left a tie that would have to arc
further running through them: a held note's tie through another voice's
notes near its pitch, mostly, at a voice crossing or under a run, which
MuseScore draws through them as well. Such a tie now takes a fuller arc:
its inner control points move to an eighth of its span from each end
(`FULL_TIE_SHARE`), so it leaves its heads more steeply and keeps its height
across its middle, and it may arc as far as a sixth of its span
(`LONG_TIE_SHARE`), half the third a slur may take, and never past three
spaces (`MAX_LONG_TIE_ARC`). A tie that passes within 1.5 spaces keeps its
own shape, and so a tie of nine spaces or less is unchanged; one that cannot
pass either way is still left as it was, not half raised. Slurs are
untouched. Locked by `clearance`'s
`a_long_tie_takes_a_fuller_arc_to_pass_and_a_short_one_does_not` and
`a_long_tie_clears_an_accidental_in_its_path` (`epiphany-cli`: a whole
note's tie over the other voice's A-flat a step above it). The
`arrow_accidentals` golden's whole-note tie, which ran through the arrow of
an arrowed natural, now arcs over it; re-blessed with renders before and
after for the owner's reading.

## ENGRAVER_VERSION 47 → 48: a curve passes the stems inside its span (X5c.3, 2026-10-10)

A slur or tie passed the accidentals of its staff after spacing but not its
stems, so a curve whose columns spacing had moved, or a tie at a voice
crossing, ran through a stem standing inside its span: the long stem of
another voice's note rising through a held note's tie, mostly, and a slur
ending past a beamed group's last stem. `HorizontalRemap::stems` gathers
each event's upright stroke whose ends ride one slot, where the spacing
sets it, by band, and a curve passes those of its own band standing more
than `STEM_END_MARGIN` (0.6 spaces) inside either end, the stems of the
notes it joins standing at its ends, with the accidentals and by the same
rules: a slur by its arc or by lifting an end, a tie by its arc within its
bounds, a long tie by a fuller arc. A tie that cannot pass a stem so, under
a beamed run whose stems reach a beam far above it, is left as it was.
Stems of another staff, which casting lengthens to their beams, are not
gathered. Locked by `a_tie_passes_a_stem_standing_in_its_path`
(`epiphany-cli`: a lower voice's whole-note tie under the stems an upper
voice's E4 raises through it at a crossing). No golden changed.

## ENGRAVER_VERSION 48 → 49: a tie starts past its first note's flag (X5c.4, 2026-10-10)

A tie leaves its head from the head's side, and where its first note is an
unbeamed note whose flag stands on the tie's side (an upper voice's eighth,
its stem up and its tie above), the tie ran into the flag.
`clearance::start_past_flag` now starts such a tie past the flag it would
run through, by `ACCIDENTAL_CLEARANCE`, at the same height, its inner
control points keeping their shares of the shorter span, before it passes
accidentals and stems; a tie the move would leave shorter than a space is
kept as it was. Locked by `a_tie_starts_past_its_first_notes_flag`
(`epiphany-cli`).

Not done: a tie across a barline still runs through a time signature
standing there, as MuseScore draws it. The signature's digits fill the staff
and stand about a space before the note the tie reaches, so a tie from a
note inside the staff would have to rise past the staff's edge and fall
back within that space; taking the digits as obstacles of the clearance
cleared none in the cases tried (a quarter's and a whole note's tie from a
D5 into a 2/4) and, since a tie keeps a new shape only when it passes every
obstacle, would have returned a tie that passes accidentals there to running
through them. Room after the signature, a tie broken around the digits, or
MuseScore's crossing is the owner's to choose. No golden changed.

## ENGRAVER_VERSION 49 → 50: a tie that cannot pass a stem still passes its accidentals (X5c.3, 2026-10-10)

Version 48 gave a curve its stems and accidentals as one set of obstacles,
and a tie keeps a new shape only when it passes every obstacle, so a held
chord's tie under a beamed run of another voice, whose stems reach a beam
far above any arc the tie may take, was returned whole to its version 46
shape and ran again through the accidentals version 47 had passed: the
corpus sweep after version 48 counted two such ties and four more tie
crossings of noteheads. `clearance::clear_obstacles` now passes both where
the tie can and, where it cannot pass a stem, the accidentals alone, as at
version 47. Slurs, which keep what they clear, are unchanged. Locked by
`clearance`'s `a_tie_that_cannot_pass_a_stem_still_passes_its_accidental`.
No golden changed.

## ENGRAVER_VERSION 50 → 51: a tie starts past its flag on its justified span (X5c fix round 1, 2026-10-11)

Version 49 decided a tie's start past its first note's flag in the spacing
pass, on the spaced line, and kept a tie the move would leave shorter than a
space running into its flag. Casting then justifies each system, giving its
note columns their share of the slack, and carries an anchored tie between
its end slots' new places, so on the page such a tie could have spaces past
the flag and still run through it (C5c review 1's M1: a flagged sixteenth
tied to a quarter, on every justified system). Casting now decides again for
each tie of a system where it places it: one still running into its flag,
with the span there to start past it, starts past it
(`restart_past_flag`), and since its shape has changed it passes the
accidentals and stems of its band again, gathered where the page sets them
(`casting::placed_ink`). A tie the spacing already started past its flag,
and every other curve, is placed as before. Locked by
`a_tie_starts_past_its_flag_on_its_justified_span` (`epiphany-cli`: the
review's shape, six measures on a page fifty spaces wide) and
`a_tie_restarted_past_its_flag_passes_its_accidentals_again` (a tie the
move brings onto a flat its old shape passed).

## ENGRAVER_VERSION 51 → 52: an undecomposed event spaces by its own duration (X5c fix round 1, 2026-10-11)

Duration spacing (version 45) took an event's spans from its placed
components, and an event the pre-pass does not decompose has none, though
the constrained stage still draws it, as an implicit quarter: notes whose
file gives a tuplet's time modification without the tuplet, or the notes of
an aleatoric region of musical discipline, whose decomposition is deferred.
Its column then took its width from another staff's durations, or the
uniform 1.5 spaces when nothing else sounded (C5c review 1's L2: a triplet
eighth's beat over a quarter took 5.0 spaces where the notated triplet
takes 7.89). The logical IR's note, rest and unpitched content now carries
the event's sounding duration in musical time (`duration`, `None` in
wall-clock time), and `component_spans` gives an event with no components
the span of that duration. Wall-clock columns still take none. Locked by
`an_undecomposed_event_spaces_by_its_own_duration` (`epiphany-cli`:
triplet eighths with and without their tuplet, over a quarter on the other
staff, take the same beat). The renderer's `valid_score_rich` goldens, by
engraver and by stub, moved: the two quarters of its aleatoric region now
stand a quarter's column apart; re-blessed with renders before and after
for the owner's reading.

## ENGRAVER_VERSION 52 → 53: a system the score's breaks fix compresses toward a floor (X5c fix round 2, 2026-10-18)

Duration spacing keeps a fixed scale (version 45, a quarter's column 5
spaces), and once a file's own line breaks were imported the measures
between two of them were often wider than the line at that scale: MuseScore,
which wrote the breaks, sets a quarter at 3.5 spaces before it justifies
(`HorizontalSpacing::chordRestSegmentNaturalWidth` in 4.7.5), so its
systems hold more. The break search then added a system inside the file's,
where the file has one. Now, in a region with break requirements, the
measures between two of them (or between one and the region's start or
end) are a system the score fixes (`casting::fixed_systems`), and one that
holds more than the line at its natural width is kept whole wherever its
floor fits the line, and compressed when placed:

- Its slots are spaced again (`spacing::Skyline::place`) with every note
  column's spring scaled by one factor, the largest in `[0, 1]` at which the
  system spans the line, found by halving and set exactly between the last
  placement too wide and the last that fits (`casting::compress_system`); so
  the durations keep their ratios wherever a spring, and not a floor,
  decides a gap. The lead's, a signature's, a change's and a barline's
  springs and clearances are kept, as justification keeps them.
- At a factor of zero only the floor holds: the ink after a note column
  keeps from later ink at its height MuseScore 4.7.5's least distance for the
  two kinds of item (`spacing::floor_gap`, from `PaddingTable::createTable`
  at the default style of `styledef.cpp`: a head 0.35 from a head, stem,
  ledger line or accidental after it (`minNoteDistance`), 0.5 from a rest,
  1.5 from a barline (`noteBarDistance`); a stem 0.85 from a stem; a dot
  0.65 from a head (`dotDotDistance`); a rest 1.65 from a barline; a ledger
  line's distances a head's less its reach, `ledgerLineLength` 0.33, at least
  0.25; a flag's, a dot's and a rest's to clefs and signatures as the table
  gives them; 0.1, its `minPadUnit`, for every pair it leaves at its
  default). On one staff, whatever their heights, a barline, clef or
  signature clears every earlier note column's ink, and a head, stem, dot or
  rest clears every earlier head, stem, dot and rest (MuseScore's
  `isNeverKernable` and `isSameVoiceKerningLimited`, a staff standing in for
  a voice), by the same distances but two heads' 0.1. Ties and beams across
  two staves keep their least spans as in the spacing pass (a tie's 1.0 is
  MuseScore's `minTieLength`). MuseScore itself, when a system overflows,
  re-spaces it with its paddings and duration stretch both scaled down in
  steps and then lets ink collide (`HorizontalSpacing::squeezeSystemToFit`);
  here the paddings are the floor and are never scaled.
- Where even the floor holds more than the line, the search breaks that
  system as it breaks any other, by natural widths, and each system it adds
  there is named in the solver's warnings with the floor's overflow
  (`casting::AddedSystem`). A system narrower than the line justifies as
  before; the search is unchanged in a region with no break requirement, and
  in one with them it differs only in keeping a fixed system whole (its
  overflow net stays out of one).
- A compressed system's slurs and ties are placed from where the spacing set
  them before they passed any ink and pass the flag, accidentals and stems
  of their band again where the compressed system sets them (`clear_curve`
  on `placed_ink`), so a tie the natural line started past its flag is not
  left shorter than a space; elsewhere casting decides only a tie left in
  its flag, as at version 51. A tie split at a system break keeps the
  spacing pass's decision.

Locked by `a_system_the_breaks_fix_is_compressed_to_hold` (three systems the
file breaks, each wider than the line at the fixed scale, held, filling the
line, a quarter's column still half again an eighth's),
`where_even_the_floor_overflows_one_system_is_added` (on the widest page
whose line one system's floor overflows, exactly one system is added there,
named with its overflow), `compression_at_the_floor_puts_no_ink_on_ink`
(dotted rhythms, seconds, accidentals, ledger lines, rests and a clef, key
and time change, on every page down to the floor, share no ink their
natural spacing does not, each onset's heads after the last's by 0.1),
`a_compressed_systems_ties_are_decided_where_it_sets_them` and
`a_systems_floor_is_measured_with_the_lead_it_opens_with` (a system held
down to the same narrowest page whatever lead a later system opens with)
(`epiphany-cli`).

## ENGRAVER_VERSION 53 → 54: a compressed system's slur passes the heads it meets (X5c fix round 2, 2026-10-18)

Version 53 passed a compressed system's slurs and ties again over the flag,
accidentals and stems of their band where the compressed system sets them.
The spacing sets each slur clear of the heads it spans, but compression
narrows a slur and keeps its heights, and the round's corpus sweep found a
steep slur (falling a seventh from a beamed sixteenth to the note after it)
cutting its own first head in a lightly compressed system, which version 52
had split and stretched. A compressed system's slur now passes the heads of
its band as it passes accidentals (`casting::pass_compressed`, by lifting an
end near its end and raising its arc in its middle), so compression brings
no slur onto a head; a tie keeps its own obstacles, and a slur outside a
compressed system its spacing pass's decision. Locked by `casting`'s
`a_compressed_systems_slur_passes_the_heads_it_meets`. No golden changed.

## ENGRAVER_VERSION 44 → 45: a visiting voice on its home's side (X4b.5, 2026-10-10)

A voice the importer splits across staves is a voice on each, the staff's own
voices ranked first and a visitor after them, so a visitor drew as a lower
voice: an upper-staff voice writing unbeamed notes on its part's lower staff
turned their stems down where MuseScore turns them up, and the lower staff's
own voice beside such a note drew as an upper voice where MuseScore draws it
alone (D46). Schema major 5 gives the score each visiting voice's
home staff (`Score::voice_homes`, written by `SetVoiceHome`, which the
importer emits for every voice at home on another staff). The projection's
voice rule (`voice_places`, `epiphany-layout-ir`) now leaves a visitor out of
its staff's ranking: it stands on its home's side throughout, above when its
home staff stands above in the region, below when below, its own stated stem
direction still winning; and its ink is no company for the staff's first
voice. Locked by `a_visiting_voice_names_its_home_and_draws_on_its_side`
(`epiphany-musicxml`), failing with the home unread, inverted, or not
emitted.

## ENGRAVER_VERSION 45 → 46: a grace note left undrawn until graces are drawn (X4b fix round 1, 2026-10-11)

Schema major 5 imports a grace note as an event of zero duration at its
principal's position, where the importer had skipped it. The projection drew
it through the paths every other note takes: the decomposition pre-pass gives
a zero-length event no components, and an event without components draws as
a single implicit quarter (`components_of`, `epiphany-layout-ir`), so each
grace stood in its principal's column as a full-size head on its stem, read
as a chord tone, with its accidentals, its beam and any slur from it. Drawing
grace notes is X4c's. Until then the projection lays out the score with every
grace note left out (`drawn_view`, at `to_logical` and
`laid_out_object_ids`, so the round trip's source set stays the laid-out
one), with the slurs and ties that name one and a beam's grace members,
dropping a beam left with fewer than two notes; the score version is still
the whole score's. A page then draws what it drew before the import brought
graces in. The corpus report names each such grace as "grace note left
undrawn", and a slur or tie on one likewise, where it had passed under "not
checked"; ink traced to a grace would be "grace note drawn as a plain note".
Locked by `a_grace_note_is_not_drawn_as_a_member_of_its_principals_chord`
(`epiphany-cli`), which fails with the graces laid out. X5c may take a
number too, and the second to merge renumbers.

## ENGRAVER_VERSION 46 → 47: a staff's own rest clear of a visiting voice (X4b fix round 3, 2026-10-16)

From 45 a voice visiting a staff from its home on another stands on its
home's side, and its ink is no company for the staff's first voice, which is
then alone beside it. Alone, that voice's rest keeps its own place, a
whole-measure rest at its measure's first column hanging from the fourth
line and any other on the middle line, so a visiting note at the rest's time
could stand in it, head, stem or accidental (D70); before 45 the visitor
ranked as the staff's second voice and turned the rest above it. Each staff
instance's content now names its visiting events (`StaffContent::visiting`,
`epiphany-layout-ir`, from the same home sides `voice_places` reads), and once
stems, beams, heads and accidentals are placed the constrained pass moves
each drawn rest of the staff's own voices whose glyph or dots meet the
visitors' ink in its column (heads, ledger lines, dots, accidentals, stems
and flags) a space at a time until it stands `REST_CLEARANCE` (0.25 space)
above or below every box of it, its dots moving with it (`clear_visitors`).
A rest alone on its staff moves to the side away from the stems it meets,
below a visitor from above, whose stem turns up toward its home, and above
one from below, as two voices on a staff turn apart; a rest a second voice
of its own staff has turned keeps that side, and passes over the end of a
visiting stem pointing its way rather than crossing to its partner's side.
The visitor keeps the stem its home, or its voice's stated direction, gives
it. Only the rest's own column is read: a column's glyphs share its slot and
move together under spacing. A rest moved outside the staff draws no ledger
line, as a rest moved by `clear_rests` draws none. Locked by
`a_staffs_own_rest_stands_clear_of_a_visiting_voice` and
`a_rest_moved_clear_of_a_visitor_keeps_its_dot_and_its_staffs_side`
(`epiphany-cli`), which fail with the move removed. Centering a
whole-measure rest in its measure, as MuseScore draws it, is X5d's (D70).
X5c takes its own numbers; the second to merge renumbers.

## The rest move bounded (X4b fix round 4, 2026-10-19; ENGRAVER_VERSION stays 47)

`clear_visitors` stepped a rest while any box of it met the visitors' ink,
with no bound: every box is finite and the rest and its dots move as one, so
it ended, but a defect in what moves (dots left behind, as fix round 3's
mutation `fr3d` held them) looped without end and hung the render tests
rather than failing them (C4b review 3's L2). The move now stops after as
many spaces as the column ink's vertical extent plus the rest's own height
and a clearance at either end, with a space to spare, the most a rigid body
needs to pass a set of boxes that stay, and a debug build asserts that the
rest then stands clear, so such a defect fails the render tests within
seconds. No input the engraver gives reaches the bound: every page draws as
before, so `ENGRAVER_VERSION` stays 47.

## ENGRAVER_VERSION 54 → 55: the two branches' engravers taken together (X5c fix round 3, 2026-10-21)

X4b raised the version from 44 to 47 on its branch while this one raised it
from 44 to 54, so `45` to `47` name different changes on the two: the
sections from `44 → 45: time columns spaced by their durations` to `53 → 54`
are this branch's, and the three after them X4b's, which reached `main`
first. Taking `main` here draws both at once: a visiting voice on its home's
side, a grace note left out and a staff's own rest moved clear of a visitor,
in systems spaced by duration and, where the file's breaks fix a system
wider than the line, compressed toward the floor. Neither branch's rules
changed in the merge; the version goes to `55`, past both, so a layout
either branch made is never taken for the other's.

## A held system keeps MuseScore's paddings where its natural spacing sets less (X5c fix round 3, 2026-10-21; ENGRAVER_VERSION stays 55)

The spacing pass clears a note column's ink from later ink at its height by
`SLOT_GAP`, 0.3, a barline's included. The floor clears it by MuseScore's
paddings instead, which exceed that for most pairs and most of all before a
barline (`noteBarDistance`, 1.5), and on one staff it keeps some ink apart
at any height, where the spacing pass keeps none. A compressed system sets
each slot at the larger of its scaled spring and the floor, so where the
floor holds a slot further from the one before it than the spacing pass
did, that gap stands wider than at its natural spacing while the rest of
the system shrinks further to pay for it: a compressed system's last notes
stand as far from its barlines as MuseScore's do, further than the same
music stands uncompressed, and now and then a whole measure stands a little
wider than its natural width. This is accepted beside D71 (C5c review 2's
L1, ruled under D76): the floor is MuseScore's own distance there, and
capping it at the natural spacing would set a compressed system's notes
nearer its barlines than MuseScore sets them. So the floor can exceed the
spacing it compresses; a lone measure dense enough to have a floor wider
than its natural width stays overfull at its natural width, as a lone
measure always has.

## ENGRAVER_VERSION 55 → 56: a flagged tie keeps its start's room at the floor (X5c fix round 3, 2026-10-21)

The floor kept a tie's least span, a space, between its ends. A tie whose
first note has a flag on the tie's side starts past the flag where it has
the span (`clearance::start_past_flag`: the flag's right edge,
`ACCIDENTAL_CLEARANCE` beyond, and `MIN_TIE_SPAN` from there to its end),
which takes the flag's width and 0.2 more than the floor kept. Between the
natural spacing and the floor there was therefore a band of widths at which
compression set a tie the natural spacing had started past its flag back
into it (C5c review 2's L3), each with under a space past the flag, the
rule kept as written. The floor now keeps such a tie that span too
(`clearance::flag_start`, read where `space_slots` gathers the floor's
ties): a flag riding the tie's start slot on its staff, in the window
`start_past_flag` reads, reaching past the tie's start toward the side it
arcs to, keeps the tie's end from its start slot by the flag's right edge,
0.2, the least span and a thousandth more (`FLAG_START_SLACK`), since a tie
set at exactly that span could be left a rounding short of it and kept in
its flag. A system the floor so widened may no longer fit its line and take
a system more, named as any other. The natural spacing is unchanged; so is
every tie in a system the breaks do not hold. Locked by
`compression_never_sets_a_tie_back_into_its_flag` (`epiphany-cli`): nine
measures broken every three, each with two flagged ties over a whole note,
every tie clear of every flag on every page down to the floor, which fails
with the flag's room left out of the floor or with the thousandth dropped.

## The compression tests see compression (X5c fix round 3, 2026-10-21; ENGRAVER_VERSION stays 56)

C5c review 2's L2 found four of the five compression tests passing with no
system compressed, and the floor's lead, a continued tie's room and most of
its distances pinned by no test. `compression_at_the_floor_puts_no_ink_on_ink`
and `a_compressed_systems_ties_are_decided_where_it_sets_them` now assert,
before reading ink or ties, that the system the file breaks fills its line
within 0.05 on every page and stands narrower there than at its natural
spacing, so a held system left uncompressed fails them.
`a_held_systems_floor_takes_its_lead_and_a_continued_ties_room` holds three
systems in a key of seven flats, the middle one opening on a tie continued
from the first, on every page down to the narrowest whose line its floor
with its lead and the tie's room fits, every system's ink within the margin
there, and the next page adds a named system inside the middle one; with
either the lead or the room left out of the floor's measure, a held system
runs past the margin unnamed. `a_held_systems_floor_keeps_musescores_least_distances`
finds the narrowest page, to a tenth of a space, that holds a system the
file breaks, and reads there four of `floor_gap`'s distances as the page
draws them: two whole notes at one height 0.35 apart, an up stem and the
down stem after it 0.85, a dot 0.65 before the next head, a head 1.5 before
the barline after it, each within the tenth the width steps by.
`a_compressed_systems_slurs_pass_the_ink_where_it_sets_them` holds three
systems of slurs over flats and a steep two-note slur from a beamed sixteenth
down to their floor, no slur entering an accidental or a head: once the floor
kept a flagged tie's room, the ties test no longer saw a compressed system's
curves left unpassed, and this one does. For the same reason the ties test
gained a tie with no flag, whose least span only the floor's tie span keeps.
All are in `epiphany-cli`.

## Where the floor departs from MuseScore (X5c fix round 3, 2026-10-21; ENGRAVER_VERSION stays 56)

The floor (version 53) takes MuseScore 4.7.5's paddings, but where it reads
which ink they apply to it departs from MuseScore in four places, each
keeping ink further apart than MuseScore would, never nearer:

- A staff stands in for a voice. MuseScore keeps one voice's heads, stems,
  dots and rests apart at any height (`isSameVoiceKerningLimited`); the floor
  keeps a staff's apart, so two voices on one staff kern less here than in
  MuseScore.
- Two heads whose heights meet keep `minNoteDistance`, 0.35, whatever their
  voices. MuseScore's table gives two notes `minPadUnit`, 0.1, and
  `computeNotePadding` raises it to 0.35 only where two notes of one voice,
  or two neighbours by duration, have shapes that intersect.
- Heights meet here when two pieces of ink, each widened by
  `SKYLINE_MARGIN`, 0.2, and rounded out to the skyline's quarter-space
  bands, share a band, so ink up to about 0.65 of a space apart in height
  keeps its distance. MuseScore pads only where two shapes come within 0.2 of
  each other vertically, 0.1 before an accidental
  (`computeVerticalClearance`).
- A clef keeps clear of every earlier note column's ink on its staff at any
  height wherever it stands. MuseScore's `isNeverKernable` kerns a clef that
  changes inside a measure.

Besides these, as version 53 states, the lead's, signatures', changes' and
barlines' springs are kept whole: the barline's own spring after it stands
1.5 from its glyph's origin, where MuseScore's `barNoteDistance` is 1.25 from
the barline's right edge.

## ENGRAVER_VERSION 56 → 57: text runs ride their slots (X4c.2, 2026-10-22)

The resolved layout carries shaped text runs (`texts`), a fourth primitive.
The spacing pass moves each run by its slot's rigid delta, as it moves the
slot's glyphs, and casting places it with the slot's system and its staff's
shift. A run's ink is its staff's content by height alone: it joins the
staff's extent, so the gap to the next staff makes room for it (D79's Q14),
and it takes no room across, so a run neither widens a column nor a system.
Lyrics, which do take room, are X4c.5's. No score draws a run yet, but the
runs' count enters every layout's canonical bytes, so the version moves.
Locked by `a_text_run_rides_its_slot_and_its_staff` and
`a_text_run_takes_its_staffs_room_on_the_page`.

## ENGRAVER_VERSION 57 → 58: marks at notes, fermatas and breath marks (X4c.3, 2026-10-22)

The projection now draws a note's articulations, techniques, ornaments,
tremolo strokes, jazz marks and arpeggio, and a fermata, breath mark or
caesura, and names in `ConstrainedLayoutIR::family` how a solver must treat
each. The engraver follows it:

- A member taking no room across (`Room::None`: every mark but the arpeggio,
  every fermata) gives its slot neither width nor its x: the spacing pass
  takes a slot's source from its first glyph that takes room, so a mark the
  projection emits before its note's head never moves the note. MuseScore's
  chord shape leaves its articulations out of spacing too
  (`ChordLayout::fillShape`; `an_arpeggio_takes_room_and_other_marks_take_none`,
  `marks_take_no_room_at_the_floor`). An arpeggio takes room as an
  accidental does (`Room::Ink`).
- A breath mark or caesura stands right-aligned in a fixed column of its
  own, whose spring is the 1.0 MuseScore pads a breath by after it. Before
  it, it keeps 1.0 from the ink where their heights meet and from its
  staff's heads, stems, dots and rests at any height, at natural spacing
  and at the floor, MuseScore never kerning a breath against its voice; an
  accidental after it keeps 1.0 where their heights meet (`Ink::Breath`;
  `a_breath_mark_takes_a_column_after_its_note`,
  `a_breath_mark_keeps_its_padding_at_the_floor`).
- Once a system is set across, the family's placed members (a fermata now;
  dynamics, syllables and tempo marks in later rows) are placed staff by
  staff in MuseScore's order, each moved away from its staff until it clears
  the ink under it by its distance, never toward it, a row's members at the
  height the furthest needs (`family::place`). Their ink then joins their
  staff's extent, so the gap to the next staff makes room for it
  (`a_fermata_between_two_staves_holds_them_apart`).
- A mark placed past the end of a stem meeting a beam across two staves
  rides the beam's staff and stands between the staves with it, the content
  of neither (`a_mark_past_a_beam_across_two_staves_stands_past_it`).

## ENGRAVER_VERSION 58 → 59: dynamics and hairpins (X4c.4, 2026-10-22)

The projection draws dynamics and hairpins, each a placed member of the
family, and the engraver places them as MuseScore 4.7.5 does once a system
is set:

- Members are placed one by one in MuseScore's order, each joining the ink
  the next is placed against; once all of one order are placed, the members
  of a row (a dynamic and the hairpins meeting it, end to end) move out
  until their optical centres stand at the outermost of theirs
  (`AlignmentLayout::alignItemsWithTheirSnappingChain`), a dynamic's centre
  half its x-height over its baseline and a hairpin's its axis
  (`a_row_aligns_its_members_centres_at_the_outermost`,
  `a_chain_of_dynamics_and_hairpins_stands_in_one_row`).
- A dynamic or hairpin of a grand staff's upper staff, below it, stands
  centred between the two staves once they are set, as far as the ink
  between them lets it, a row's members together
  (`dynamicsHairpinsAutoCenterOnGrandStaff`;
  `a_grand_staffs_dynamic_stands_between_its_staves`).
- A hairpin over a system break is drawn a piece in each system, each piece
  opening from its axis as MuseScore draws one: to the full opening at its
  wide end, and to the continuation's half space at a narrow end the break
  leaves open (`a_hairpin_over_a_system_break_opens_each_piece`); a piece
  the system sets shorter than a space has its end moved out to a space
  (`a_hairpin_is_at_least_a_space_long`).

## ENGRAVER_VERSION 59 → 60: lyrics (X4c.5, 2026-10-22)

- A syllable takes room against its staff's verse alone (`Room::Verse`, D79's
  Q13): it stands at least 0.25 after the verse's syllable before it
  (`lyricsMinDistance`), or 0.5 where a hyphen joins them, room for the
  shortest dash and its pads, at natural spacing and at the floor
  (`a_verse_keeps_its_syllables_apart`). A run that takes room, a syllable
  or an arpeggio, widens its slot in casting too, so a system holds it.
- Once a system is set, a verse is placed against the ink under it, never
  against another verse, its syllables aligned on their baseline a space
  clear of that ink; a verse moved moves the verses outside it as far
  (`LyricsLayout::checkCollisionsWithStaffElements`;
  `verses_move_with_the_verses_inside_them`).
- A stroke anchored in two systems (a hairpin, hyphen or melisma line over a
  break) splits where they break, rather than by where its spaced ends fall:
  a hairpin or line ending at a later system's first note no longer rides
  its first system whole. Its anchored ends ride their slots' deltas, as
  within one system, where it is placed as where it is drawn, so a
  justified system no longer stretches a hairpin's or line's end away from
  its note.
- A hyphen is drawn as dashes along each of its pieces, one in each 16
  spaces of it, each 0.4 to 0.6 long and centred in its share
  (`lyricsDashMaxDistance`, `lyricsDashMinLength`, `lyricsDashMaxLength`);
  one too short for its least dash grows to it about its centre. A hyphen
  or melisma line going on past its system stops a quarter space short of
  the system's end (`endingXForOpenEndedLines`); carried into a later system
  it starts at that system's first column (`firstNoteRestSegmentX`: the
  lesser of a space past the lead's ink and the first note), and a hyphen's
  piece there too short for its least dash grows back from its end, so the
  dash stands 0.05 before the syllable opening the system
  (`LyricsLayout::layoutDashes`). A melisma line's piece shorter than a
  space, less a tolerance of 0.05, is not drawn (`lyricsMelismaMinLength`,
  `lyricsMelismaForce` off).

## ENGRAVER_VERSION 60 → 61: tempo marks, staff text and rehearsal marks (X4c.6, 2026-10-22)

- The family is placed in MuseScore's order with the rehearsal marks after
  the tempo marks, so a rehearsal mark stands outside a tempo mark it meets
  (`SystemLayout::layoutSystemElements` places them last), and a segno or
  coda after both (`markersAndJumps`).
- A member at its measure's start (`MeasureStart`) is set across once its
  system is: a rehearsal mark centred on its barline's right edge, where
  its measure starts, and a segno or coda on the barline's middle, both by
  the ink of the barline column there; where the measure opens its system
  or follows no barline, a rehearsal mark is centred just after the ink of
  the system's lead and of the columns before its own (its signatures), and
  a segno or coda on the system's left edge (`TLayout::layoutRehearsalMark`,
  `MarkerLayout::doLayoutMarker`). Its runs and its frame move together, in
  its placing as in its drawing.
- A tempo mark is not set right of a rehearsal mark at its measure, as
  MuseScore's `tempoAlignRightOfRehearsalMark` does; the two stand one over
  the other.

## ENGRAVER_VERSION 61 → 62: trill lines (X4c.7, 2026-10-22)

- A trill line's wiggles are set piece by piece once its system is: as many
  `wiggleTrill` as reach along the piece to the nearest, one each advance
  (`LineSegment::symbolLine`), from after the sign on its first piece and
  from the system's first column on a piece carried into a later one; the
  line's stroke then draws nothing, a traced anchor, and its wiggles' ink is
  its piece's ink in the family's placing.
- Every line of the family, a hairpin's legs as a hyphen, is split over a
  system break as MuseScore splits a line (`TLayout::layoutSystemSLine`): a
  piece going on stops a quarter space short of its system's end and a
  piece carried into a later system starts at its first column. A hairpin's
  first piece had run to the system's end, and its next from the system's
  first ink.

## ENGRAVER_VERSION 62 → 63: the family's strokes take no room (X4c.9, 2026-10-22)

- A stroke of a member taking no room across (a rehearsal mark's frame, a
  hairpin's legs, a lyric line, a trill line's carrier, a two-note tremolo's
  strokes) widens no slot in the spacing pass, as the member's glyphs do
  not. A rehearsal mark's frame rides its measure's first slot at both ends,
  so it had been folded into that slot's extent as a stem is, and a frame
  wider than the room before its first note pushed the measure's content
  right by the excess (`a_rehearsal_mark_takes_no_room`).

## ENGRAVER_VERSION 63 → 64: a column's place is its first glyph that takes room (X4c.9, 2026-10-22)

- Casting reads a column's reference x, by which it orders a region's
  columns and shares a justified system's stretch among them, from its
  first glyph that takes room, as the spacing pass reads a slot's source.
  It had read the first glyph the projection emitted there, and the
  projection emits a note's marks before its head: a scoop left of a note
  opening a measure set its column before the barline it follows, and the
  barline was drawn after the note, over it
  (`a_mark_gives_its_column_no_place`). A column of marks alone keeps its
  first glyph's x.

## ENGRAVER_VERSION 64 → 65: a two-note tremolo laid out as a beam (X4c.10, 2026-10-24)

- The projection lays a two-note tremolo out as MuseScore does, as a beam of
  its two notes whose strokes are its levels: both stems turn one way and
  reach the outermost stroke, and the strokes stand a space inside each
  stem (`a_two_note_tremolo_is_a_beam_inset_from_its_stems`). They had been
  drawn about the line joining the stems' midpoints, from stem to stem, so
  they crossed the second chord's accidentals. Each stroke's ends ride the
  two notes' columns, as a beam's do; the engraver itself is unchanged.

## ENGRAVER_VERSION 65 → 66: a one-note tremolo lengthens its stem (X4c.10, 2026-10-24)

- A stem carrying a one-note tremolo is as long as MuseScore's
  `StemLayout::calcMinStemLength` makes it to hold the strokes, and a beam
  over such a stem stands far enough out for it; three strokes or more had
  run to the head on a stem of the plain length
  (`a_one_note_tremolo_lengthens_its_stem`). The engraver itself is
  unchanged.

## ENGRAVER_VERSION 66 → 67: a one-note tremolo and a chord line take room (X4c.10, 2026-10-24)

- A member of the family taking room as its chord's ink (`Room::Ink`: a
  one-note tremolo, a chord line, an arpeggio) widens its slot as the
  chord's own ink does, as MuseScore's `ChordLayout::fillShape` adds a
  tremolo and a chord line to the chord's shape; the other marks still take
  none.
- No member of the family gives its column its place: a slot's source in
  the spacing pass, and its reference x in casting, is its first glyph not
  of the family, which a member taking room no longer is by taking it
  (`a_mark_gives_its_column_no_place`).
- A chord line keeps 0.35 from the ink either side of it at its height and
  0.65 before a barline, and a one-note tremolo 0.35 from the ink after it,
  MuseScore's padding for a chord line and, as for a beam, for a tremolo
  (`PaddingTable::createTable`; `Ink::ChordLine`, `Ink::Tremolo`;
  `a_tremolo_and_a_chord_line_take_room`).

## ENGRAVER_VERSION 67 → 68: a visiting chord's marks by its own staff's voices (X4c.10, 2026-10-24)

- A chord visiting another staff sets its marks at its stem only where the
  staff it belongs to shows another voice while it sounds, and otherwise as
  a chord alone does, at its head, as MuseScore reads that staff's voices
  (`Measure::hasVoices`; `a_visiting_chords_marks_follow_its_own_staffs_voices`).
  Every visiting chord had taken its marks at its stem. The engraver itself
  is unchanged.

## ENGRAVER_VERSION 68 → 69: a tuplet on one staff stands out of it (X4c.10, 2026-10-24)

- A tuplet whose ends share a staff keeps its bracket's line, where its
  number is centred, at least 1.25 outside the staff on its side,
  bracketed or not, as MuseScore's default `tupletOutOfStaff` sets it
  (`TupletLayout::layoutBracket`); it had cleared only its columns' ink, so
  over low notes stemmed up it stood in the staff, its number on the top
  line (`a_tuplet_on_one_staff_stands_out_of_it`). A tuplet riding a beam
  across two staves is placed as before. The rich fixture's tuplet moves
  out of its staff: its two golden files are re-blessed under D10.

## ENGRAVER_VERSION 69 → 70: an ending opening a system is drawn on it (X4c.10, 2026-10-24)

- An ending whose barline closes a system, its line going on into the
  next, is drawn wholly on the next system, as MuseScore lays a spanner out
  only on a system its start precedes the end of (`SystemLayout`,
  `tick() >= etick`): its hook, numbers and line start there,
  `BAR_NOTE_DISTANCE` (1.25) before the system's first note or rest, its
  hook's half width further on (`Volta::linePos`), and the piece of its line
  at the earlier system's end is dropped (`ConstrainedLayoutIR::endings`;
  `an_ending_opening_a_system_is_drawn_wholly_on_it`). Its hook and numbers
  had ridden the barline, so they stood at the end of the system before.
  Its numbers take height on the system they move to and no room across,
  where their place is not yet known; an ending opening mid-system is drawn
  as before. The repeat fixture's first ending opens a system: its engraved
  golden and snapshot are re-blessed under D10.

## ENGRAVER_VERSION 70 → 71: a text's lines drawn stacked (X4c.10, 2026-10-24)

- A staff text or tempo mark written in lines is drawn a run a line, the
  first on its baseline and each further line Edwin's line (1.2 of its
  size) lower, as MuseScore sets a text on its baseline (`TextLayout`,
  `BASELINE`); a tempo mark's metronome follows its last line. The runs
  move together, so the whole text clears the staff as one
  (`a_text_in_lines_is_drawn_in_lines`). The importer now keeps a text's
  line breaks (D81's Q2); a line break had been an uncovered codepoint.

## ENGRAVER_VERSION 71 → 72: an arpeggio clears its chord's ledger lines (X4c.10, 2026-10-24)

- An arpeggio before a chord with a head on a ledger line and no
  accidental stands 0.63 left of the heads, MuseScore's padding from an
  arpeggio to a ledger line (0.3) past the line's reach (0.33), not 0.4, so
  it no longer meets the ledger line
  (`an_arpeggio_stands_clear_of_its_chords_ledger_lines`). The engraver
  itself is unchanged.

## ENGRAVER_VERSION 72 → 73: an arpeggio's arrow stands left of its gap (X4c.10, 2026-10-25)

- An arpeggio's ink ends at its right edge, an arrow's too, so its whole
  width and its gap stand left of the chord, as MuseScore sets them
  (`ChordLayout`, the arpeggio's width and gap). An arrow, wider than a
  wiggle, had reached 0.32 past the edge toward the heads, so an arpeggio
  with an arrow stood that much nearer its chord than its gap, and met a
  ledger line its gap clears
  (`an_arpeggio_stands_clear_of_its_chords_ledger_lines`). The engraver
  itself is unchanged.

## ENGRAVER_VERSION 73 → 74: a two-note tremolo's notes at their written value (X4c.11, 2026-10-27)

- Each note of a two-note tremolo is drawn at its written value, one value
  longer than it sounds and its dots kept, as MuseScore keeps the pair
  (`Chord::setTremoloTwoChord`): its head, stem, flag and beams by that
  value, its sounding duration and the spacing by it unchanged. A pair
  MuseScore writes as halves at a 2:1 time modification had drawn as
  quarters, and as wholes as halves on stems; a sounding whole is written a
  breve (`a_two_note_tremolos_notes_are_drawn_at_their_written_value`). A
  stemless note's strokes are inset from its head's edge, with no stem's
  width beside it (`chordBeamAnchorX`). The engraver itself is unchanged.

## ENGRAVER_VERSION 74 → 75: a slur over a stemless tremolo pair on its pair's side (X4c.11, 2026-10-27)

- A slur over a two-note tremolo's stemless notes stands away from the way
  its pair turns, as MuseScore sets a slur opposite its first chord's
  direction, its stem drawn or not (`SlurTieLayout::computeUp`). Drawn at
  their written value, such notes had given their columns no stem, and a
  slur over stemless notes goes above, so under a low pair it had flipped
  over the heads and through the strokes
  (`a_slur_over_a_stemless_tremolo_pair_stands_away_from_its_strokes`). The
  engraver itself is unchanged.

## ENGRAVER_VERSION 75 → 76: a slur clears a stemless tremolo pair's strokes (X4c.11, 2026-10-28)

- A slur on the side a two-note tremolo's stemless notes turn to passes
  over their strokes, as MuseScore's slur clears the tremolo's shape
  (`SlurTieLayout::getSegmentShape`). A stemless note's column had reached
  no further than its head, where a stemmed note's stem reaches the
  strokes, so an upper voice's slur over such a pair ran through them
  (`a_slur_on_a_stemless_tremolo_pairs_strokes_side_passes_over_them`).
  The engraver itself is unchanged.

## ENGRAVER_VERSION 76 → 77: a close mark leaves its note's tie room (X4c fix round 1, 2026-10-28)

- A staccato or tenuto on the side a tie leaves or reaches its note's outer
  head stands clear of the tie, as MuseScore's `leaveSpaceForTie` sets it:
  a line further out in the staff where its head stands in a space, and 0.4
  further for a staccato and 0.6 for a tenuto where it is set from the
  head. It had stood where it stands on an untied note
  (`a_close_mark_leaves_its_notes_tie_room`). The engraver itself is
  unchanged.

## ENGRAVER_VERSION 77 → 78: a unison two voices do not share stands apart (X4c fix round 1, 2026-10-28)

- Two voices' heads at a unison are one head only where both draw a stem,
  as MuseScore shares them (`calculateChordOffsets`); two whole notes, a
  two-note tremolo's written whole among them, had shared one. A unison not
  shared stands apart as MuseScore sets it, the lower voice 0.15 right of
  the upper's head, or an upper voice with dots 0.1 right of a lower one
  without, where it had stood 0.02 right
  (`a_unison_two_voices_do_not_share_stands_apart`). The engraver itself
  is unchanged.

## ENGRAVER_VERSION 78 → 79: a slur takes its side as MuseScore computes it (X4c fix round 1, 2026-10-28)

- A slur goes opposite its first note's direction, a whole note's as its
  stem would turn; above where its voice's notes turn both ways between its
  ends; and by voice where its staff shows another voice anywhere in its
  span, as MuseScore's `SlurTieLayout::computeUp` sets it. Slurs over whole
  notes had gone above, a whole note turned the other way had counted no
  direction, and a later voice entering after the first note had not set
  the side (`a_slur_takes_its_side_as_musescore_computes_it`). The engraver
  itself is unchanged.

## ENGRAVER_VERSION 79 → 80: a tremolo pair's tuplet at its own ratio (X4c fix round 1, 2026-10-28)

- A tuplet a two-note tremolo's note opens is read at its own ratio, the
  pair's 2:1 divided out of the time modification, and numbered as
  MuseScore numbers it, 2 where it was 4; every pair member, in a tuplet or
  not, is drawn at its written value, so a pair in a triplet another note
  opens draws halves where it drew the quarters it sounds
  (`a_tremolo_pairs_tuplet_is_numbered_as_its_own`). The engraver itself
  is unchanged; the importer and the layout IR's drawn value change.

## ENGRAVER_VERSION 80 → 81: an outside mark clears its chord's accidentals (X4c fix round 1, 2026-10-28)

- A mark outside the staff that meets its chord's own accidental across
  stands 0.4 clear of it, as MuseScore's autoplace lifts it: a trill mark or
  mordent over a note with an accidental had stood on the accidental's top
  (`an_outside_mark_clears_its_chords_accidentals`). The engraver itself is
  unchanged.

## ENGRAVER_VERSION 81 → 82: an accent kerns toward a staccato outside the staff (X4c fix round 1, 2026-10-28)

- An accent set straight after a staccato on its side stands 0.2 nearer it
  where the staccato stands outside the staff, as MuseScore kerns the two
  (`stacAccentKern`); it had stood the full 0.4 off
  (`an_accent_kerns_toward_a_staccato_outside_the_staff`). The engraver
  itself is unchanged.

## ENGRAVER_VERSION 82 → 83: a stress mark aligns with the stem (X4c fix round 1, 2026-10-28)

- A stress mark on its note's stem side stands centred between the head and
  the stem, as an accent does, MuseScore counting it a basic articulation;
  it had stood centred on the head
  (`a_stress_mark_aligns_with_the_stem_as_an_accent_does`). The engraver
  itself is unchanged.

## ENGRAVER_VERSION 83 → 84: a second close mark steps as MuseScore sets it (X4c fix round 1, 2026-10-28)

- A second staccato or tenuto on a side stands a space past where it would
  stand alone in the staff, its centre a space past the first's, and
  outside the staff past the first by its height and 0.4, as MuseScore's
  `layoutArticulations` sets it; in the staff it had stood a space past the
  first mark's outer edge (`a_second_close_mark_steps_as_musescore_sets_it`).
  The engraver itself is unchanged.

## ENGRAVER_VERSION 84 → 85: a slur's ends read as MuseScore reads them (X4c fix round 1, 2026-10-29)

- A slur from a chord in a beam across two staves goes above; the voices a
  slur's side is read from are read to the end of its last note or rest,
  drawn or not; a slur ending on a rest reads no mixture of its voice's
  directions, and one starting on a rest reads no voices, as MuseScore's
  `computeUp` reads them. A second voice's slur ending on a hidden rest had
  arced over its staff (`a_slur_reads_its_span_to_a_hidden_rest_and_a_beam_across_staves`).
  The engraver itself is unchanged.

## ENGRAVER_VERSION 85 → 86: a unison where the lower voice crosses moves the upper voice (X4c fix round 1, 2026-10-29)

- Where a lower voice crosses an upper one at a unison, reaching above its
  lowest head, the upper voice stands 0.15 right of the lower's head, as
  MuseScore's `calculateChordOffsets` moves the up voice there, and the two
  share no head unless their heads where they meet pair off as unisons they
  could share. The lower voice had stood right of the upper's heads, past
  any second of its chord
  (`a_unison_where_the_lower_voice_crosses_moves_the_upper_voice`). The
  engraver itself is unchanged.

## ENGRAVER_VERSION 86 → 87: an arpeggio clears a later voice's chord in its column (X4c fix round 1, 2026-10-29)

- An arpeggio stands left of every chord of its staff's column, of its own
  voice or a later one, that its height meets, by its gap from the furthest
  of their heads and accidentals, as MuseScore sets an arpeggio left of the
  chords its span covers. It had stood by its own chord alone, so an upper
  voice moved right of a lower one at a unison set its arpeggio among the
  lower voice's heads, and a later voice's accidental further out met it
  (`an_arpeggio_stands_left_of_a_later_voices_chord_in_its_column`). The
  engraver itself is unchanged.

## ENGRAVER_VERSION 87 → 88: an arpeggio stands left of its column's origin and clear of other voices' accidentals (X4c fix round 1's addendum, 2026-10-29)

- An arpeggio stands left of its own chord and, where that chord stands
  right of another voice's, left of its column's origin, as MuseScore
  clamps the chord's `x` to its segment's origin; then once clear of every
  accidental and ledger line of the column's other chords that its box
  meets, whatever their voice, as MuseScore's `clearAccidentals` moves it.
  Version 87 read its own and later voices' chords as a span, which
  MuseScore's default span of one voice does not cover, so a lower voice
  moved right of an upper one at a unison or a second set its arpeggio over
  the upper voice's head, and an earlier voice's accidental went unread
  (`an_arpeggio_stands_by_its_own_chord_and_clear_of_other_voices_accidentals`).
  The engraver itself is unchanged.

## ENGRAVER_VERSION 88 → 89: a dotted upper voice a second above an undotted lower goes right (X4c fix round 1's addendum, 2026-10-29)

- An upper voice with dots a second above a lower one without, or crossed
  by it with a second or a unison where they meet, stands 0.1 right of the
  lower's head, its dots after its own, as MuseScore's
  `calculateChordOffsets` moves the up voice there. The lower voice had
  stood right of it, the upper's dots after the lower's head; fix round 1
  had taken the branch at a unison alone
  (`a_dotted_upper_voice_a_second_above_an_undotted_lower_goes_right`). The
  engraver itself is unchanged.

## ENGRAVER_VERSION 89 → 90: a stemless note's tremolo stands on its stem's side (X4c fix round 1's addendum, 2026-10-29)

- A one-note tremolo on a note that draws no stem stands beyond the head
  its stem would leave, on that stem's side, centred on the head or on the
  stem's line where a head stands across it, and kept off the staff's outer
  line on the head's side, as MuseScore's `TremoloLayout::layout` sets it.
  It had stood over the chord whichever way its stem would turn
  (`a_stemless_notes_tremolo_stands_on_its_stems_side`, D96). The engraver
  itself is unchanged.

## ENGRAVER_VERSION 90 → 91: an arpeggio stands 0.3 off an accidental (X4c fix round 2, 2026-10-30)

- An arpeggio stands 0.3 of a space left of its own chord's accidentals and
  of another chord's that it meets, as MuseScore keeps
  `arpeggioAccidentalDistance` from both; it had stood 0.4 off, the gap it
  keeps from a head (D97 Q1,
  `an_arpeggio_stands_its_accidental_distance_off_an_accidental`). The
  engraver itself is unchanged.

## ENGRAVER_VERSION 91 → 92: a stemless note's tremolo stands a space clear of its head and within the staff (X4c fix round 2, 2026-10-30)

- A one-note tremolo on a note that draws no stem stands with its strokes
  at least a space clear of the head's ink and within the staff, as Gould
  sets it (D100 Q1); where both cannot hold, within the staff, but never
  nearer the head's ink than 0.25 (D101). It had stood 0.75 off the head's
  centre, held at the line on the head's side alone
  (`an_ornaments_accidentals_and_a_stemless_tremolo_stand_their_distances_off`,
  `a_stemless_notes_tremolo_stands_on_its_stems_side`). The engraver itself
  is unchanged.

## ENGRAVER_VERSION 92 → 93: each accidental of an altered unison stands before its own head (X4c fix round 2, 2026-10-30)

- Two voices on one step with two alterations keep two heads, and the
  voice set right takes its accidentals before its own heads, standing
  where they clear the other's head and ledger lines, as Gould sets an
  altered unison (D100 Q2); every accidental of the column had stood
  before all its heads
  (`an_altered_unisons_accidentals_stand_each_before_its_own_head`).
  Version 78's entry says a unison is shared "as MuseScore shares them":
  MuseScore also shares a unison of one spelling whose notes differ only by
  a quarter-tone accidental, comparing their `tpc`, which the engraver,
  following Gould, keeps as two heads; the quarter-tone fixture's unison is
  such a one, which fix round 1 said MuseScore does not share. The
  engraver itself is unchanged.

## ENGRAVER_VERSION 93 → 94: a lone tie takes its side by Gould's rules (X4c fix round 2, 2026-10-30)

- A tie from a single note alone into two-part writing curves as the part
  it joins, above into the upper part and below into the lower; a tie
  between two single notes alone whose stems, drawn or as they would turn,
  go opposite ways curves away from the middle line, and above on it, as
  Gould sets them (D100 Q3). Each had taken the side opposite its first
  note's stem, or by the middle line where it drew none. A close mark's tie
  room follows the side drawn
  (`a_lone_tie_takes_its_side_as_gould_sets_it`,
  `a_close_mark_leaves_its_notes_tie_room`). The engraver itself is
  unchanged.

## ENGRAVER_VERSION 94 → 95: the notes under an ottava at their written place (X4d.1, 2026-11-01)

- The model holds every note at its sounding pitch, as the file gives it
  with its octave shift, and the layout drew a note under an ottava there,
  an 8va's an octave high on ledger lines no score shows. The layout now
  draws the score through a view (`drawn_view`) in which each note of an
  ottava's staff whose onset is at or after the ottava's start and before
  its end, in the start's region, stands its octaves nearer the staff, its
  explicit spelling moved with it (`written::move_pitches`, shared with a
  transposing part's written pitch). Gould sets the notes under the sign at
  their written place (*Behind Bars*, pp. 28 to 31), as MuseScore keeps a
  note's written pitch apart from the shift it plays at (`Note::epitch`)
  (`the_notes_under_an_ottava_stand_at_their_written_place`).

## ENGRAVER_VERSION 95 → 96: pedal lines, ottavas, text lines, brackets and glissandi (X4d.1, 2026-11-01)

The projection draws the family's other lines, each a placed member a
solver splits where a system break cuts it (`FamilyLine::Extension`,
`FamilyLine::Glissando`), by MuseScore 4.7.5's defaults where Gould agrees
or gives no figure (`epiphany-layout-ir`'s `lines.rs` cites each). A probe
exported by MuseScore 4.7.5 itself (`x4d/probes/ms1/` in the private
corpus cache) gives where each stands once autoplace clears it, and the
engraver's own placing reproduces it:

- A pedal line: the pedal sign half a space before its note, at least 2.5
  under the staff; a solid line 0.11 wide from half a space after the sign,
  its foot on the sign's baseline, to 0.75 before the column its release
  falls in (the barline, where a measure starts there), or 1.25 before a
  note where the staff's next pedal starts, where it rises 1.2. A pedal
  drawn without its sign opens with that hook. Every pedal line of a staff
  in a system stands at one height (`processLines`, `align`; Gould lets
  consecutive signs align where there is room, p. 333). Over a system break
  its next piece opens with "(Ped.)" (Gould's reminder, p. 332).
- An ottava: its numeral at its first note, over the staff for an 8va or
  15ma and under it for an 8vb, at least 2.0 off it; a dashed line from half
  a space after the numeral, hanging from its top over the staff and on its
  baseline under it (Gould, p. 28), to a corner 1.0 toward the staff a space
  past its last note's ink, riding that note's column, never past the next
  note once the system is set (`Ottava::linePos`; Gould, p. 30); its
  numeral again at a new system. The numerals and the pedal sign are
  Bravura's symbols at Bravura's size, as MuseScore sets a line's `<sym>`
  text at its music font's.
- A text line: over its staff at least 1.0 off it, its words upright and
  its line through their middle, dashed as its style says, ending a space
  before the column it ends at (`SLine::linePos`). One whose words open with
  a dynamic term is a verbal crescendo, MuseScore's hairpin of the line
  type: below by the hairpin rule (D79's Q11), italic on the line's
  baseline, clear of a dynamic at either end and in its chain's row, its
  next piece opening with "(cresc.)" or "(dim.)" (Gould, pp. 101 and 106).
  A bracket draws its line alone, hooked 1.5 toward the staff at its end,
  the model keeping no hook of its own.
- Dashes are laid piece by piece as MuseScore distributes a dash pattern:
  one more dash than the whole pairs of dash and gap a piece holds, its
  first and last at its ends.
- A glissando: on the line joining its heads' centres, from a quarter space
  past its first note's ink at its head to a quarter space before its last
  note's at the half its line comes from, so short of an accidental (Gould,
  p. 141); between heads on one line a quarter space up or down at each end
  the way it slides (p. 142); its heights read again on the line joining
  its heads where the system sets them, and over a system break its
  gradient kept across the pieces (p. 142); "gliss." in Edwin italic at 8
  points centred along each piece it is shorter than, turned to its slope;
  wavy, its pieces set as `wiggleGlissando`s. The model keeps no head for a
  glissando: the `k`th of the glissandi joining two notes takes the `k`th
  head of each chord from the top. A glissando whose notes stand on other
  staves is left undrawn.
- The corpus report names each of these lines the engraver leaves undrawn
  (`the_lines_undrawn_are_counted`).

## ENGRAVER_VERSION 96 → 97: a gradual tempo change drawn once over its system, bold (X4d.1, 2026-11-01)

- A text line whose words open with a term of gradual tempo change (rit.,
  rall., accel., allargando and the like, `epiphany_layout_ir::tempo_term`)
  is drawn as MuseScore draws its gradual tempo change: a system object over
  the system's top staff, drawn once for the parts that repeat its words and
  times, its words in Edwin bold at 12 points on the line's baseline, at
  least 2.0 over the staff and half a space clear of the ink under it, the
  line 0.15 wide in dashes 0.9 long spread with gaps of 1.35, no hook and no
  continuation. MuseScore's export writes each of these as words and dashes
  (`system="only-top"`), and its own page sets them bold at 12, as Gould sets
  a gradual tempo change as a tempo indication, bold roman above the top
  staff, a continuing change shown by its dashes (*Behind Bars*, pp. 182 and
  185). They had been drawn as plain text lines, upright at 10 points over
  their own staves. The corpus report counts a part's copy of one drawn
  for another as drawn
  (`a_gradual_tempo_change_stands_bold_over_the_staff`,
  `a_gradual_tempo_change_in_two_parts_is_drawn_once_over_the_system`,
  `a_gradual_tempo_change_drawn_once_for_two_parts_is_no_omission`).

## ENGRAVER_VERSION 97 → 98: grace notes drawn small before their principals (X4d.2, 2026-11-01)

- A grace note is drawn, replacing D65's rule that left it out: the logical
  pass gives it its own content (`LayoutContent::Grace`), kept out of the
  beams, slurs and ties drawn for notes and out of the decompositions, and
  the projection sets each voice's graces at an onset right to left before
  the ink of the principal's column on their staff (its heads with their
  accidentals, ledger lines and dots, its stems and flags, its rests): the
  group wholly left of it by 0.45 and each grace wholly left of the next by
  0.3 (MuseScore's `graceToMainNoteDist`, `graceToGraceNoteDist`), as Gould
  sets graces before the beat (*Behind Bars*, p. 127). Its heads,
  accidentals and flag are Bravura's at 0.7 (`graceNoteMag`; Gould: smaller
  than a cue note's three quarters, p. 125), set from Bravura Text as runs,
  each head carrying its pitch's exact provenance; its stem, ledger lines,
  slash and beams are strokes at 0.7 of a note's widths. Every part rides
  the principal's slot, and the grace is one member of the family taking
  room as an accidental does (D79's Q13), so the principal's column stands
  further right where the space before it is short.
- Its stem is 2¼ spaces from its head's centre (Gould, p. 125: ruled by the
  owner on 2026-11-01 over MuseScore's and LilyPond's 3.5 scaled to about
  2.45), up but in a lower part (p. 126). An unbeamed grace of an eighth or
  shorter takes its flag; a run of graces one beam joins (MuseScore's
  export, the model's beam) is beamed, its slope held to a quarter space a
  space and its stems reaching the beam, at least their own length.
- An acciaccatura's slash stands on an unbeamed grace or a beam's first
  (`ChordLayout`; Gould: at the beginning of a group, p. 126), as MuseScore
  sets it (`TLayout::layoutStemSlash`): at 40°, from half a head's width left
  of the stem's right edge and 1.4 under the tip to the flag's right edge, or
  two spaces along a beam from two thirds as far. Its stem is lengthened
  until the slash stands a quarter space clear of the grace's ledger lines
  (Gould, p. 126).
- A grace joins its measure's accidental state before its principal, in its
  order, as Gould holds accidentals good for grace and measured notes alike
  (p. 84), so a grace's sharp holds for the note after it on its line.
- A slur or tie naming a grace is still left out, and the corpus report
  names it; the report counts a grace drawn small as drawn, one with a
  full-size glyph as drawn as a plain note
  (`a_grace_note_is_drawn_small_before_its_principal`, the `graces` suite).

## ENGRAVER_VERSION 98 → 99: the dynamics placed before the hairpins (X4d.3, 2026-11-01)

- The family's dynamics, expression text and hairpins are one stage placed
  in MuseScore's order (`SystemLayout::layoutDynamicExpressionAndHairpins`):
  every dynamic of a staff in a system first, then the expression text, then
  the hairpins, verbal ones included, each joining the ink the next is
  placed against; once all are placed, each chain of a dynamic and the
  hairpins meeting it is aligned at its outermost centre
  (`AlignmentLayout::alignItemsWithTheirSnappingChain`). They had been placed
  as one order left to right, so a hairpin starting left of a dynamic of
  another chain it ran over pushed the dynamic out
  (`dynamics_are_placed_before_the_hairpins`). Gould asks only that a
  sequence of dynamics and hairpins keep one horizontal plane where it can
  (*Behind Bars*, p. 105), which the chains' alignment keeps.

## ENGRAVER_VERSION 99 → 100: a dynamic clear of a barline through the staves (X4d.3, 2026-11-01)

- A dynamic stands a quarter space clear of a barline that runs through
  the staves at it, as MuseScore moves one whose offset no user changed
  (`DynamicsLayout::manageBarlineCollisions`, 0.25 spaces); the model keeps
  no offset an author gave a dynamic, so every dynamic is moved (corrected
  in X4d fix round 1, where this said a dynamic no one moved). Under its
  staff it is judged by its own staff's barline where that runs on to the
  next staff of a joined group; over it, by the barline of the staff above. It moves left of the
  barline after it, then right of the one before it, which wins. Gould keeps
  barlines whole where a marking can move before or after one, moving an
  initial dynamic right of its barline and breaking the barline only for
  one that cannot move (*Behind Bars*, p. 103). LilyPond 2.26.0 reaches the
  same page another way: a dynamic takes room in the horizontal spacing, so
  the measure widens rather than the dynamic moving, while its plain text
  scripts cross a span bar (a hand-written probe). On one staff, or where
  the barline stops at its staff, nothing moves
  (`a_dynamic_stands_clear_of_a_barline_through_the_staves`).

## ENGRAVER_VERSION 100 → 101: a mark at a note clear of its column's other chords (X4d.3, 2026-11-01)

- A mark set outside a chord's close marks stands `articulationMinDistance`
  (0.4) clear of the ink of every other chord in its staff's column, another
  voice's and a visiting chord's included: their heads, ledger lines, dots
  and accidentals, stems and flags, wherever they meet it across its width.
  MuseScore autoplaces such a mark against its staff's skyline, every
  voice's chords in it (`ChordLayout::layoutArticulations2`,
  `Autoplace::autoplaceSegmentElement`); it had stood clear of its own
  chord's ink alone, so an upper voice's accent could stand on the head of
  a lower voice crossing above it. As in MuseScore, a close mark (a staccato
  or tenuto set near its note) does not move, and one chord's marks pass
  another's (`itemsShouldIgnoreEachOther` lets an articulation ignore
  another); another voice's rests and beams, in MuseScore's skyline, are not
  yet in the column's ink here. Gould gives no figure; she sets each part's
  marks at its own stem's end in double-stemmed writing so that it is clear
  which note each belongs to (*Behind Bars*, pp. 117-118), which a mark on
  another part's head defeats. LilyPond 2.26.0 draws an accent over the
  crossing head, its accent placed against its own voice and the staff
  alone (a hand-written probe); MuseScore's export of the same file lifts
  it clear. The owner's ruling D85 named MuseScore's rule
  (`an_accent_stands_clear_of_another_voices_ink_in_its_column`).

## ENGRAVER_VERSION 101 → 102: a stemmed tremolo a space clear of its head (X4d.5, D102, 2026-11-01)

- A stemmed one-note tremolo of two strokes or more stands a space clear of
  its head's ink, its stem lengthened past the outermost stroke, as Gould
  sets it (*Behind Bars*, p. 221: with two strokes or more a space is kept
  from the head, which they would otherwise hide; p. 222: the stem runs on
  past the last stroke, the strokes clear of a flag or beam). The stem's
  least length is counted as MuseScore counts it
  (`StemLayout::calcMinStemLength`) with 1.5 spaces from the centre of the
  head nearest its end where MuseScore
  keeps 1.25 (`tremoloNoteSidePadding`, about 0.75 from the ink), and in a
  beam a stem's thickness more, the stem ending that far inside the beam's
  outer edge. One stroke, which Gould sets across a staff line and gives no
  distance, keeps MuseScore's. LilyPond 2.26.0 keeps its strokes about 0.22
  from the head (the coordinator's probe at D102). The owner ruled Gould's
  rule over both engines (D102). `a_one_note_tremolo_lengthens_its_stem`
  is rewritten to it: three strokes take a quarter's stem to 4.25, four to
  5.0, a beamed eighth's beam 4.75 and a stem's thickness out.

## ENGRAVER_VERSION 102 → 103: an arpeggio right of its barline at any height (X4d.5, D102, 2026-11-01)

- An arpeggio keeps a barline's clearance (0.65) after the barline before
  it at any height, over or under the staff as on it, in the spacing pass
  and at the compression floor alike. A barline's clearance had held only
  where its own ink's height met the later ink's, so an arpeggio on a chord
  over the staff, and the flat after it, tucked left of the barline. Gould
  sets the arpeggio sign after the barline (*Behind Bars*, p. 131);
  MuseScore never kerns ink past a barline and pads an arpeggio after one
  by 0.65 (`HorizontalSpacing::computeKerning`, by `isNeverKernable`, and
  `PaddingTable`; this entry first cited `doComputeKerningType`, corrected
  in X4d fix round 2);
  LilyPond 2.26.0 sets the same chord's arpeggio right of the barline (a
  hand-written probe). Other ink over or under the staff, which MuseScore
  keeps right of a barline too, keeps the band rule here
  (`an_arpeggio_stands_right_of_its_measures_barline`,
  `a_compressed_systems_arpeggios_stand_right_of_their_barlines`).

## ENGRAVER_VERSION 103 → 104: expression text italic under its staff (X4d.6, D79 Q9, 2026-11-01)

- Expression text, the marker kind X5b appended, is drawn in Edwin italic
  at staff text's size under its staff, its baseline 2.5 under it, or over
  a voice's staff (D79 Q10), 1.5 over it, a half space clear of the ink it
  meets; it had been drawn as staff text, upright and over (D105). Gould
  keeps italic for expression marks, roman for instructions and bold for
  tempi (*Behind Bars*, p. 489), sets them under the stave after the
  dynamics (p. 495) and over it in vocal music (p. 434); MuseScore's
  expression style agrees (`expressionFontStyle`, `expressionPlacement`,
  `expressionPosBelow`, `expressionPosAbove`, `expressionMinDistance`);
  LilyPond 2.26.0 sets a text script under its staff by default, upright
  unless its markup is italic (`TextScript`).
- After a dynamic at its note it stands on the dynamic's baseline, in its
  row, half a space after the dynamic's ink, or 0.2 after one ending in a
  forte (`Expression::computeDynamicExpressionDistance`), as Gould and
  MuseScore place it, where LilyPond stacks it outside the dynamics' line.
  Where a hairpin starts at its note it stands outside the dynamics and
  hairpins, placed once they stand (`PlaceOrder::OuterExpression`), from
  the dynamic's left where one stands there: Gould aligns a hairpin with
  the dynamic it follows and sets the expression mark under them (p. 495),
  as LilyPond's stacking does, where MuseScore sets the dynamic, the words
  and the hairpin in one row. The owner ruled Gould's place, and the same
  where a hairpin starts with no dynamic, which Gould does not cover
  (2026-11-01). A dynamic moved off a barline (X4d.3) takes the expression
  after it along.
- The tests: `expression_text_stands_italic_under_its_staff_after_its_dynamic`,
  `expression_text_stands_over_a_voices_staff`,
  `expression_text_moves_with_its_dynamic_off_a_barline`,
  `expression_text_stands_outside_a_verbal_crescendo_starting_with_it`; the
  staff text test reads its italic words under the lower staff.

## ENGRAVER_VERSION 104 → 105: a dynamic glyph moved off a barline (X4d.3, 2026-11-01)

- A dynamic drawn as a glyph moves off a barline through the staves as
  dynamic words do (version 100's rule): the move across was given to a
  placed member's text runs and strokes, and not to its glyphs, so a
  dynamic glyph stood where it was while the rule judged it moved. It is
  drawn moved, and placed off its staff against the ink where it is moved
  to (`a_dynamic_glyph_stands_clear_of_a_barline_through_the_staves`,
  `a_dynamic_moved_off_a_barline_stands_clear_of_the_ink_it_moves_to`).
  X4d.4's sweep of every page found it: a glyph dynamic within a quarter
  space of a barline's line through the gap it stands in.

## ENGRAVER_VERSION 105 → 106: a glissando's later piece after its system's lead (X4d.1, 2026-11-01)

- A glissando's piece in a later system opens a quarter space after the
  ink of the system's lead, and ends, as before, a quarter space before
  its chord's ink at its end. It had opened at the system's first column,
  which for the end note is that note's own, so the piece ran back over
  the note's accidental; MuseScore opens a later segment before the first
  column's leading ink (`System::firstNoteRestSegmentX`). Where the
  chord's ink leaves no room the piece draws nothing; with the lead's gap
  of 0.8 that leaves 0.3 at least, so the guard binds on no state the
  importer reaches today: every lead it builds draws a clef. A lead holding
  nothing, as one would once a clef marked hidden is honoured, reaches it,
  and a test on such a lead built by hand pins it (X4d fix round 1;
  `a_glissando_piece_after_an_empty_lead_never_runs_back`,
  `a_glissando_continued_into_a_system_stands_between_its_lead_and_its_chord`).
  X4d.4's sweep of every page found it: every glissando end standing
  nearer a head than a quarter space was such a piece.

## ENGRAVER_VERSION 106 → 107: a hairpin moved with a dynamic moved off a barline (X4d.3, 2026-11-01)

- A hairpin of a dynamic's row that starts at the dynamic's note starts as
  far past it once the dynamic moves right off a barline, and one ending at
  its note ends as far before it once it moves left, whole or cut by a
  system break, as MuseScore lays a hairpin out from the dynamics it snaps
  to after they move (`DynamicsLayout::manageBarlineCollisions` runs in the
  dynamic's layout, before the hairpins). The hairpin had stayed where the
  unmoved dynamic left it, its opening on the moved dynamic's ink. A moved
  hairpin is placed off its staff over its moved span. Placed over its
  unmoved span instead, it is set against ink only that span meets, the
  notes under a dynamic moved off its barline, and its row stands farther
  off the staff with it: the placement's half is reachable, where this
  entry first said its mutation survived for want of a page to tell it
  (corrected in X4d fix round 1, whose test pins it,
  `a_moved_hairpin_is_placed_over_its_drawn_span`;
  `a_hairpin_moves_its_ends_with_dynamics_moved_off_barlines`,
  `a_hairpin_over_a_system_break_moves_its_ends_with_its_dynamics`). X4d.4's
  sweep of every page found the hairpins meeting their dynamics' ink once
  dynamic glyphs moved (version 105).

## ENGRAVER_VERSION 107 → 108: a grace note after the barline before its principal at any height (X4d fix round 1, 2026-11-02)

- A grace note's ink, its heads, accidentals and flag, its stem, ledger
  lines, slash and beam, keeps a barline's clearance (0.65) after the
  barline before its principal at any height, over or under the staff as
  on it, in the spacing pass and at the compression floor alike, as an
  arpeggio does (version 103). A barline's clearance had held only where
  its own ink's height met the grace's, so a grace standing off the staff
  at a bar's first beat, on ledger lines over it or a beamed pair high
  over it, stood over or before the barline, where it reads as the end of
  the bar before. Gould places grace notes before a bar's first beat after
  its barline, and always so for graces sounding on the beat, a group of
  three or more sounding before it being free to stand before the barline
  so the beat is not pushed far from it (*Behind Bars*, p. 127); the model
  keeps no such timing, so every group stands after. MuseScore never kerns
  ink past a barline (`HorizontalSpacing::computeKerning`, by
  `isNeverKernable`, which holds key and time signatures and a clef but a
  mid-measure one too; this entry first cited `doComputeKerningType`,
  corrected in X4d fix round 2, version 120 taking the signatures) and sets
  them after it; LilyPond 2.26.0 sets a high grace and a high beamed pair
  after theirs (C4d review 1's hand-written probe). A trill's finishing
  notes, which Gould sets before the barline (p. 127), would need graces
  the model keeps after their note; it holds none. A grace is told by its
  room, `Room::Grace`, which otherwise takes room as `Room::Ink` does
  (`a_grace_off_the_staff_at_a_bars_first_beat_stands_after_its_barline`,
  `a_compressed_systems_graces_stand_right_of_their_barlines`).

## ENGRAVER_VERSION 108 → 109: a grace note's marks drawn outside the staff (X4d fix round 1, 2026-11-02)

- A grace note's articulations and techniques are drawn, at the grace's
  size (0.7), where Gould puts them: outside the staff, the most
  conspicuous place, since small marks are hard to see on it (*Behind
  Bars*, p. 130). Read from her examples there, a run of graces (a lone
  grace, or graces one beam joins) takes its marks on the side where a
  mark set outside the staff and the run's ink stands nearer its heads:
  under heads under the staff and under a grace low on it, over a beam
  over it and over the staff beyond the stem of a grace high on it; where
  both stand as near, on the stem's side. Beside another voice they stand
  at the stem, as MuseScore sets them and as Gould sets each part's marks
  at its own stem (pp. 117 and 118); a marcato, and a string, brass or
  snap-pizzicato sign, stand above, as a note's do. Each stands 0.4 off the
  staff and its grace's ink (`propertyDistance`), centred on its head, or
  on the stem's side midway between the head and the stem
  (`articulationStemHAlign`), the next on its side 0.4 past it, and, as
  X4d.3's column rule sets a note's outside marks, 0.4 clear of the ink of
  its column it meets across its width, the principal's and every grace's
  (`articulationMinDistance`). Each run carries its part's id, as a note's
  mark does; it takes no room. They had been drawn nowhere, the logical
  pass giving a grace no marks. MuseScore lays a grace's marks out as a
  note's at its size, a staccato at its head, on the staff where the head
  stands; LilyPond 2.26.0 sets a grace's staccato at its head too (C4d
  review 1's hand-written probe). The owner ruled Gould's place (D116).
  An ornament, a tremolo or an arpeggio on a grace is not drawn.
- The corpus report checks a grace's marks as a note's, so a mark on a
  grace drawn nowhere is named; the check had skipped graces, written while
  a grace was left out with its marks
  (`a_graces_marks_stand_small_outside_the_staff`,
  `a_graces_marks_take_the_side_nearer_its_heads_outside_the_staff`,
  `a_graces_mark_stands_clear_of_its_columns_ink`,
  `a_graces_marks_are_drawn_and_counted_undrawn`).

## X4d fix round 1: the barline, hairpin, grace and glissando clauses pinned (2026-11-02, no version change)

- C4d's review named clauses of X4d's rules that no test pinned. Each now
  has one, seen failing with the clause's mutation: of the barline rule, a
  dynamic over its staff judged by the staff above's barline
  (`words_over_the_top_staff_stay_at_their_note`), a group's bottom staff
  running on to none (`words_under_a_groups_bottom_staff_stay_at_their_note`),
  the barline before winning over the one after
  (`words_wider_than_their_measure_stand_right_of_the_barline_before`), its
  own system's barlines alone
  (`a_dynamic_at_a_systems_first_note_is_judged_by_its_systems_barlines`); a
  hairpin moved only with its own row's dynamic
  (`a_hairpin_of_another_row_keeps_its_start`), and placed over its drawn
  span (version 107); expression text moved only with its own dynamic
  (`expression_text_moves_only_with_its_own_dynamic`) and held between a
  grand staff's staves with its dynamic's row
  (`expression_text_between_a_grand_staffs_staves_stays_on_its_dynamics_baseline`);
  a grace group's reach widening its principal's column in the projection's
  frame, where the curves and beams are first shaped
  (`a_grace_groups_reach_widens_its_columns_place_in_the_projection`); the
  `k`th glissando between chords taking each chord's `k`th head from the top
  (`parallel_glissandi_between_chords_join_heads_of_one_rank`).
- Four clauses the review found changing no corpus page. A mark at a note
  is set from its own chord by that chord's rules and kept clear of the
  column's other chords: its own column ink is left out, and is reachable,
  a wide ornament over a dotted note at the staff's top line or above
  meeting its own dot
  (`an_outside_mark_is_set_from_its_own_chord_its_dot_left_out`). A line
  released at a measure's start ends before the barline there even where a
  clef change or a breath mark stands before the barline at that time
  (`a_pedal_released_at_a_clef_change_ends_before_the_barline`). The barline
  rule leaves a member the rule for a measure's start has set: no dynamic
  takes a measure-start place today, only rehearsal marks, a segno and a
  coda, so the guard binds on no state the projection builds; a test giving
  a dynamic one by hand pins it
  (`a_member_set_at_its_measures_start_is_not_moved_off_its_barline_again`).
  A glissando's guard that both its notes stand on its staff is removed: it
  is equivalent on every state, since the logical pass gives a line
  anchored to two staves no staff, the projection draws no glissando
  without one, and each end's ink is read on the line's staff, which holds
  no other staff's heads (`a_glissando_between_two_staves_is_left_undrawn`).
- A glissando's later piece after a lead holding nothing is pinned with
  version 106's guard, above.

## ENGRAVER_VERSION 109 → 110: an ottava starting at a grace numbered at the grace (X4d fix round 1, 2026-11-02)

- An ottava whose first note on its staff is opened by grace notes sets its
  numeral at the first grace's head, and its line from half a space after
  it, where it had stood at the principal, the graces outside it. Gould
  places the numeral just left of the first note it applies to (*Behind
  Bars*, p. 28), which is the grace, drawn under the sign at its written
  place (version 95); LilyPond 2.26.0 starts its "8" at the grace (C4d
  review 1's hand-written probe). MuseScore starts the line at the
  principal: a departure from it, which the owner ruled (D116). A grace
  later under the ottava, or one at its start on another staff, leaves the
  numeral at the first note
  (`an_ottava_starting_at_a_grace_sets_its_numeral_at_the_grace`,
  `only_a_grace_opening_an_ottava_on_its_staff_moves_its_numeral`).

## ENGRAVER_VERSION 110 → 111: one "gliss." for parallel glissandi, and a guessed head named (X4d fix round 1, 2026-11-02)

- Glissandi between the same two notes take one "gliss.", on the line from
  their top heads, as Gould gives a set of parallel lines one instruction
  whatever the number of notes (*Behind Bars*, p. 143), her example's word
  over the top line; each had drawn its own, as MuseScore draws one on each
  line: a departure from it, which the owner ruled (D116)
  (`parallel_glissandi_take_one_gliss`).
- The model keeps no head for a glissando, so one drawn from or to a chord
  takes the chord's head by its rank among the glissandi between the same
  notes (version 96), which may not be the file's; Gould has the line
  follow the pitches exactly (p. 141). The corpus report names each such
  glissando as drawn with its head guessed, until the model keeps the head
  (`a_glissando_at_a_chord_is_named_as_drawn_with_its_head_guessed`).

## X4d fix round 1: an undrawn glissando named by why (2026-11-02, no version change)

- A glissando whose notes' ink leaves its line no room between them is
  still left undrawn, and the projection now says so with a diagnostic
  (`LayoutDiagnosticKind::GlissandoTooShort`). Gould sets such a line above
  or below the notes, away from the stems, parallel to them, with "gliss."
  close by (*Behind Bars*, p. 141); the room is judged here in the
  projection's frame, before the spacing sets the page, so drawing her
  line needs a solver to choose between the two lines once the system
  stands, which is left for later. The corpus report names an undrawn
  glissando by why: between staves, of unpitched notes, too short for its
  notes, or else plainly (`an_undrawn_glissando_is_named_by_why`).

## ENGRAVER_VERSION 111 → 112: a grace group clear of its principal's arpeggio (X4d fix round 1, 2026-11-02)

- A group of graces stands its 0.45 (`graceToMainNoteDist`) clear of what
  its principal's column places that takes room as its chords' ink, an
  arpeggio above all, and a tremolo, a chord line or a jazz mark left of a
  head. The group is set out before its column's marks are, against the
  chords' heads, accidentals, ledger lines, dots, stems, flags and rests,
  so it had stood on an arpeggio drawn as a run before the chord; it now
  moves left as far as that ink reaches past it, as MuseScore keeps a
  grace clear of its chord's shape, which takes the arpeggio in. Gould sets
  the arpeggio before the chord (*Behind Bars*, p. 131) and the graces
  before the beat (p. 127), so the graces stand before the arpeggio
  (`a_grace_before_a_rolled_chord_stands_clear_of_its_arpeggio`).

## ENGRAVER_VERSION 112 → 113: a grace note clear of an earlier mark and of a beam (X4d fix round 1, 2026-11-02; the beam's half removed at version 118)

- A grace note's ink keeps the slot gap (0.3) from an earlier column's note
  marks where their heights meet, in the spacing pass and at the floor, as
  MuseScore's skyline holds a chord's articulations; a note's marks still
  take no room against other ink. A beamed pair over the staff after a
  sixteenth with a marcato had stood under the marcato
  (`a_grace_keeps_clear_of_the_mark_before_it`,
  `a_compressed_systems_graces_keep_clear_of_the_marks_before_them`).
- (Removed at version 118.) A grace standing under or over a beam of its
  staff, one joining its principal to the note before it, moved left once
  its system was set, its marks with it, until its own ink stood a quarter
  space clear of the beam, by tenths of a space and three spaces at most.
  This entry said that, a beam over two notes rising a space at most,
  moving cleared only a grace that met the beam a little. That held of the
  beam and not of what the moved grace then met: the move read the beam
  alone, so in plain beamed eighths at their own spacing it set the grace
  over the note before it, which C4d review 2 found (its L1). The owner
  ruled the move removed (D119).

## ENGRAVER_VERSION 113 → 114: a verbal crescendo moved with a dynamic moved off a barline (X4d fix round 1, 2026-11-02)

- A verbal crescendo of a dynamic's row moves with the dynamic moved off a
  barline as a hairpin does (version 107): starting at its note, its words
  and the start of its dashes as far right, and ending there, its end as far
  left, whole or cut by a system break. It had stayed where the unmoved
  dynamic left it, its words on the moved dynamic's ink. MuseScore lays a
  line of the hairpin type out from the dynamics it snaps to once they move
  (`DynamicsLayout::manageBarlineCollisions` runs before the hairpins), and
  Gould keeps a dynamic and the words of a gradual change after it in one
  plane (*Behind Bars*, pp. 101 and 105)
  (`a_verbal_crescendo_moves_its_start_with_its_dynamic_off_a_barline`).

## ENGRAVER_VERSION 114 → 115: a glissando's words off other ink, its start read again (X4d fix round 1, 2026-11-02)

- A glissando's words are kept off barlines and other ink: where, centred
  along their piece, they meet other ink of their system (a glyph, a stroke
  but a staff line, a run, another glissando's words), they slide along the
  piece by quarter spaces and then stand further off it, up to a space, to
  the first place clear of it, and where none is they stay centred. Gould
  sets the words close to the line, legible, and lets them stand on the
  staff only so (*Behind Bars*, pp. 140 and 141); MuseScore centres them
  along the line whatever they meet. The words are read as turned to the
  line, not as the box that holds them turned
  (`a_glissandos_words_stand_off_the_barline_it_crosses`,
  `a_glissandos_words_clear_of_ink_stay_centred_on_it`).
- A glissando's start is read again where its line, begun past its own
  note's ink at its head, would cross other ink of its first column, a
  stem or a head of another voice set beside it: past each box of it the
  line meets, a quarter space on. It had read its own note's ink alone and
  so could run through a head beside its own, which Gould's line between
  the notes never does (p. 141)
  (`a_glissando_starts_past_a_head_its_line_would_cross`).

## ENGRAVER_VERSION 115 → 116: a glissando's start clear of a head it would graze (X4d fix round 1, 2026-11-02)

- Version 115 reads a glissando's start again past ink of its first column
  that its line would cross; the line now also starts past a box it would
  pass nearer than its half width and a quarter space, the room it keeps
  from its own chords' ink (version 96). Where the upper of two voices a
  second apart stands right of the lower, its dots after its own head
  (version 89), a line falling from the lower head had run just under the
  upper's, its ink touching that head without crossing its box, and so
  started where it had. Gould's line runs between the notes it joins, clear
  of the others (*Behind Bars*, p. 141); MuseScore starts it past its own
  chord's shape at the head's height alone
  (`TLayout::layoutNoteAnchoredLine`), whatever another voice sets beside it
  (`a_glissando_starts_past_a_head_it_would_graze`).

## ENGRAVER_VERSION 116 → 117: a grace's marks clear of the barline as the grace is (X4d fix round 1, 2026-11-02)

- A grace note's marks (version 109) keep the barline's clearance (0.65)
  that its other ink keeps at any height after the barline before its
  principal (version 108), in the spacing pass and at the compression
  floor, a measure widening by them as by that room; against other ink
  they still take none, as a note's marks take none (`Room::GraceMark`).
  A tenuto or an accent reaches a little left of its grace's head, and so
  had stood nearer the barline than the head by that reach. MuseScore's
  grace chord's shape holds its articulations, and Gould sets the grace,
  marks and all, after the barline (*Behind Bars*, pp. 127 and 130)
  (`a_graces_mark_keeps_the_barlines_clearance`).

## ENGRAVER_VERSION 117 → 118: the grace beam move removed (X4d fix round 2, 2026-11-03)

- A grace under or over a beam of its staff stands where the spacing sets
  it, before its principal and after the note before, its flag meeting the
  beam where they cross, as MuseScore draws it; version 113's move left
  is removed. Moving the grace to clear the beam took no account of the
  note before it, so between two beamed eighths stemmed up, the first
  higher, the grace stood over the first note's head beside its stem,
  where it reads as that note's ornament or a chord. Gould sets a grace
  before the beat it belongs to (*Behind Bars*, p. 127) and lets no symbol
  cut through another (p. 29); LilyPond 2.26.0 keeps the grace between the
  notes and lifts the beam over it, which waits on the decide-list (D119).
  Version 113's other half stands: a grace's ink keeps the slot gap from an
  earlier column's note marks
  (`a_grace_between_two_beamed_notes_stands_after_the_earlier_note`).

## ENGRAVER_VERSION 118 → 119: a parallel set's "gliss." on its topmost line long enough (X4d fix round 2, 2026-11-03)

- Glissandi between the same two notes take their one "gliss." on the
  topmost of their lines whose piece holds it once the system is set, piece
  by piece. Version 111 put it on the line from the top heads alone, and
  words stand only along a piece longer than they are (version 96), so where
  the top line alone was too short, an accidental before its end note
  shortening it, the set printed none. Each line of a set now offers the
  words, tagged with its set and its rank from the top heads
  (`LineTextRole::Along`), and casting keeps them on the line of least rank
  that drew them. Gould gives parallel lines one instruction (*Behind Bars*,
  p. 143) and asks for it at least at a line's first appearances (p. 140);
  MuseScore prints one on each line that holds it, the departure the owner
  ruled at version 111 (D116), and LilyPond prints none unless asked
  (`a_parallel_sets_gliss_stands_on_its_topmost_line_long_enough`,
  `parallel_glissandi_take_one_gliss`).

## X4d fix round 2: four clauses of entries 109 and 117 pinned (2026-11-03, no version change)

- C4d review 2 found four clauses surviving their mutations (its L3), each
  now held by a hand-built test that fails with its mutation alone:
  a marcato on a grace above whatever side the grace's other marks take
  (entry 109; `a_graces_marcato_stands_above_whatever_side_its_marks_take`);
  a run whose marks would stand as near its heads either way taking its
  stem's side (entry 109; a unit test on shapes built so,
  `a_runs_marks_take_its_stems_side_where_both_stand_as_near`); a grace's
  marks beside another voice at its stem, not on Gould's side for a grace
  alone (entry 109; `a_graces_marks_beside_another_voice_stand_at_its_stem`);
  and a grace's marks widening the slot they stand in, the clause fix round
  1 kept by removing casting's exclusion of them (entry 117): at a later
  system's opening the measure starts at a tenuto reaching left of its
  grace's head, the grace set right by that reach
  (`a_graces_mark_widens_its_slot_at_a_systems_opening`). A first voice's
  grace counts as beside another voice only while that voice holds a note
  through it (`voice_places`), so the test sets one there.

## ENGRAVER_VERSION 119 → 120: a grace note and an arpeggio clear of a key or time signature at any height (X4d fix round 2, 2026-11-03)

- A grace note's ink and its marks, and an arpeggio, keep a barline's
  clearance (0.65) at any height after a key signature or a time signature
  as after a barline (versions 103, 108 and 117), in the spacing pass and
  at the compression floor alike. A signature held them only where its own
  ink's height met theirs, so a beamed pair high over a mid-system change
  of time, a grace high over a system's opening time signature, and an
  arpeggio on a chord over the staff after a change of time stood over the
  signature; after a key change, which shares its barline's column, the
  barline's clearance at any height had not held either, a grace standing
  as near the key signature as the band rule let it. MuseScore never kerns
  past a barline, a key or time signature, or a clef but a mid-measure one
  (`HorizontalSpacing::computeKerning`, by `isNeverKernable`); Gould keeps
  every sign clear of the next, at least half a space between them (*Behind
  Bars*, p. 41) and allows about two spaces after a time or key signature
  in mid-system (p. 42), and sets graces before a bar's first beat after
  the barline (p. 127); LilyPond 2.26.0 sets graces after a mid-system key
  and time change and after a system's opening time signature (C4d review
  2's hand-written probe). A clef changing at a measure's start stands
  before its barline, whose clearance then holds after it; one changing
  inside a measure is kerned past, as MuseScore kerns it. A slot is told by
  the key or time signature it holds (`Extent::signature`)
  (`a_grace_off_the_staff_after_a_signature_keeps_the_barlines_clearance`,
  `an_arpeggio_stands_clear_of_a_time_signature_before_it`,
  `a_compressed_systems_graces_and_arpeggios_stand_clear_of_their_signatures`).

## ENGRAVER_VERSION 120 → 121: a glissando's end clear of a head it would cross or graze (X4d fix round 2, 2026-11-03)

- A glissando's end is read again as its start is (versions 115 and 116):
  before each box of the ink of its last column's other voices that its
  line would cross or pass nearer than its half width and a quarter space,
  a quarter space short of it. Where the upper of two voices a second apart
  is dotted and the lower not, the upper is set right of the lower (version
  89), so a line rising or falling to the upper's head passed the lower's
  first, running into it or grazing it. Gould keeps the line between the
  notes it joins (*Behind Bars*, p. 141); MuseScore ends it by its own
  chord's shape alone (`TLayout::layoutNoteAnchoredLine`), and LilyPond
  2.26.0 runs it through the other voice's head in this shape (C4d review
  2's hand-written probe): a departure from both, which the owner ruled
  (D119). The end's own chord is cleared at its head alone, as before: read
  against it too, a parallel set's lower line stopped short of its upper
  note's accidental and lost the words version 119 gives it
  (`a_glissando_ends_before_a_head_beside_its_own`).

## ENGRAVER_VERSION 121 → 122: a glissando its system leaves too short for its notes left undrawn and named (X4d fix round 2, 2026-11-03)

- A glissando of one piece whose line, as its system sets it, is shorter
  than it is thick or turns back, its end before its start, is left
  undrawn: its stroke is a traced anchor, its words and wiggles none. The
  engraver reports each such glissando (`GLISSANDO_TOO_SHORT`, an unusual
  layout decision naming it), and `epiphany-cli` names it as the projection
  names one its notes leave no room (version 111's `GlissandoTooShort`), so
  the corpus report counts it "glissando too short for its notes" with
  those. The projection judges room in its own frame, and the spacing and
  casting then set the page, so a line it gave room could be drawn as a
  speck, or backward, between a note and an accidental the next note
  carries, where it read as an augmentation dot. Gould sets a line too
  short for its notes over or under them with its words (*Behind Bars*,
  p. 141), which waits on the follow-ups with fix round 1's attempt; until
  then it is named, not drawn. A glissando split over a system break keeps
  its pieces as version 106 sets them, a later piece opening after its
  system's lead
  (`a_compressed_systems_glissando_too_short_for_its_notes_is_named_undrawn`).

## ENGRAVER_VERSION 122 → 123: staves set by their outlines (X5d.2, 2026-11-01)

- A system's staves are set column by column (`vertical::stack`), no
  longer between whole extents: each staff stands as high as it can while
  its top line keeps the floor below the bottom line of the staff above,
  its ink 0.5 below the merged outline of every staff above it wherever
  the two come within 0.25 across, and every beam joining it to a staff
  above keeps its `rise_limit` (D109 Q1, Q2). Whole extents added a high
  note in one bar to a low note in another, and fitted no orchestral
  system on its paper. The outline keeps each rectangle at its own edges,
  a curve in the 12 chords the family's sky already used; the family
  placed off its staves, a later system's lead and every primitive of a
  staff's band feed it, by declared band, and the ink of a beam across two
  staves feeds neither.
- The floor is Gould's: staves at least a stave's height apart, 4.0 line
  to line (*Behind Bars*, p. 488; LilyPond's least between ungrouped
  staves). It compresses toward MuseScore's `minStaffSpread`, 3.5, only in
  a system whose ink would otherwise stand taller than its page's content,
  as far as it needs and no further (`vertical::stack_fitting`). D63's
  floors by score kind are withdrawn (D109 Q1): MuseScore's one floor with
  its spread on is 3.5 between every pair, and its spread's cap inside a
  brace is 6.5 (`maxAkkoladeDistance`).
- The clearances are MuseScore's (`minVerticalDistance` 0.5,
  `skylineMinHorizontalClearance` 0.25), Gould giving none (D99).
  Departures named: LilyPond pads 1.0 between staves and nothing across;
  MuseScore compares a staff with the one above it alone, where a staff
  here clears every staff above it (LilyPond's `align-interface.cc`, D109
  Q2), which matters only where ink passes a staff that draws no lines
  there.
- A syllable, and a verse's lines, keep 1.5 from the next staff's ink on
  the side they stand, as MuseScore pads a verse in its skyline
  (`LyricsLayout::addToSkyline`, `lyricsMinBottomDistance`) and LilyPond
  keeps a lyric line from an unrelated staff; the two agree.
- Tests: `vertical::tests` (the outline, the reach, the clearance, the
  merged outline, the beam bound, the floor and its compression) and
  `vertical::solve_tests`, which read every gap back from the baked output
  pairwise; `a_syllable_keeps_its_room_from_the_next_staffs_ink`,
  `a_later_systems_lead_sets_its_staves_apart` and
  `the_floor_compresses_only_as_far_as_the_system_needs` in the CLI's
  render tests. The quality census still reads the whole extents' unit,
  which no solve now realizes, until X5d.4 amends `req:qmc:vertical`; the
  tests that asserted it realized assert the rule instead. Four goldens
  change, re-blessed with their renders before and after recorded (D10).

## ENGRAVER_VERSION 123 → 124: between systems and the page (X5d.3, 2026-11-01)

- Two systems stand at least 8.5 apart line to line (MuseScore's
  `minSystemSpread`, its floor with its spread on), the whole system
  above's ink 0.5 from the whole system below's where they come within
  0.25 across (`SystemLayout::minDistance` reads the last staff above and
  the first below; with staff lines across every system the two agree).
  LilyPond keeps 4 at least and 8 by default; MuseScore's is nearer
  Gould's p. 488 (D109 Q3).
- A page's first staff's top line stands 7 below its top margin, or lower
  where its ink needs (`staffUpperBorder`), and a further system goes on
  the page only where its last staff's bottom line keeps 7 above the
  bottom margin, or its ink does where it reaches further
  (`staffLowerBorder`), as MuseScore collects a page (D109 Q4). A system
  taller than its page stays alone on it; there the top border yields, the
  system rising until its ink stands at the top margin, so the borders are
  aims that cost no system its fit, as Q4's figures assume (MuseScore keeps
  the top border even then); one whose ink its paper still cannot hold is
  named in a warning, not squeezed (D109 Q5).
- Every page, the last included, is spread by MuseScore's rule
  (`PageLayout::distributeStaves`, `VerticalGapData`), its numbers its
  style defaults: the gaps between staves and between systems, in units of
  their weights (2.5 between systems, 1.2 at a bracket's edge, 1.1 at a
  brace's), the smallest raised level first, then the rest shared by
  weight, at most 6 a gap and 6 for each gap of the page in all; caps 20
  between staves, 6.5 inside a brace, 32 between systems, each in those
  units, as MuseScore keeps its `m_maxActualSpacing`: a gap at a bracket's
  edge stops at 24 on the page, the staff cap undivided by its weight, and a
  gap inside a brace and between systems at their stated figure. A bracket
  inside a bracket weighs its edges 1.2, where MuseScore's skips one starting
  within another, a departure kept (the owner's ruling at D123): the distances
  it sets stand under the owner's reading. Until the last
  staff's bottom line stands 7 above the bottom margin or its ink reaches
  it. Before it, a gap between systems is raised toward the widest gap
  between staves of a system beside it, as far as the page allows (Gould,
  p. 488). Departures named:
  the levelling is solved exactly, where MuseScore's loop raises by steps,
  undoes the step that would overspend and hands what is left to the fill,
  stops after twenty passes and treats spacings of one integer ceiling as
  level, so its gaps can stand uneven where these stand level, sometimes by
  whole spaces on a page, a soloist's gap over a braced part above all (the
  owner kept the exact levelling at D123 Q3, neither Gould nor LilyPond
  asking for the unevenness; the pages it touches are counted in the vault's
  pass, X5d fix round 1); Gould's more room above the strings
  (p. 520) is met only where the score brackets its sections; the old
  justification spread only non-final pages, only between systems.
- A grand staff's centred dynamics are centred once the page is spread, as
  MuseScore centres them after `distributeStaves`.
- The spread's chosen sprawl is what `req:qmc:vertical` reads as
  deviation, so a sparse page reads near 1.0 on that axis; the Reference
  Suite's RS-1 and RS-2, each one sparse A4 page, carry it as a measured
  Minimal miss (the owner's ruling at X5d.3), and the tier test claims its
  thresholds on a page nothing spreads.
- Tests: `vertical::tests` (the spread's levelling, weights, caps and
  fill, and that it spends no more than it is given),
  `vertical::solve_tests::systems_stand_their_floor_or_their_inks_clearance_apart`,
  `the_spread_fills_every_page_to_its_border` and
  `a_hard_page_break_starts_a_new_page` here; the CLI's
  `systems_clear_each_others_ink`,
  `a_system_taller_than_its_page_rises_and_is_named`,
  `a_brace_keeps_its_staves_within_its_cap`,
  `a_brackets_edge_takes_more_of_the_spread`,
  `a_gap_between_systems_is_raised_first_and_a_system_keeps_the_border` and
  the marks suite's `a_grand_staffs_dynamic_stands_between_its_staves`. The
  goldens it changes are re-blessed with their renders before and after
  recorded (D10).

## The vertical measure reads what a gap asks (X5d.4, 2026-11-01; ENGRAVER_VERSION stays 124)

- `req:qmc:vertical` is amended to the catalog's 0.4.0, as X5d.1 drafted it
  (D109 Q7): each realized gap's line distance `d`, read from the baked
  staff lines, against `d* = max(f, s + c, b)`: the band's floor, the
  skyline separation of the inks either side read column by column (every
  staff above an inter-staff gap; the whole systems either side of an
  inter-system one) with `c` 0.5 and inks meeting within `h` 0.25 across,
  and a beam across the two staves' least distance. Crowding and sprawl
  both deviate. Three things the draft left unsaid are stated: the axis
  is still the mean over its units, as every axis's is; a curve is read
  as twelve chords and a box of positive extent only; a syllable and its
  verse's lines reach 1.0 further on their side, as the solve keeps them
  (X5d.2), so a lyric's room reads as asked, not as sprawl.
- The gap bands become line distances carrying the solve's numbers
  (`VerticalBand::inter_staff_gap`: floor 4.0 as both `min_height` and
  `preferred_height`, cap 20, weight 1.0; `inter_system_gap`: floor 8.5,
  cap 32, weight 2.5), as X5d.1's note set the band. The floor the measure
  reads is Gould's 4.0, so a system casting compresses toward 3.5 to fit its
  page reads the crowding it is (0.25 in 4.0 a gap), and a plain gap at the
  floor reads none.
- The census reads the baked output with code of its own (`separation`,
  step functions over the boxes' edges, merged staff by staff down a
  system), sharing none with the solve's outline: unspread, it reads no
  deviation on every fixture the solve sets, which makes it a check on the
  solve and the bake together.
- The engraver's ink is unchanged, so `ENGRAVER_VERSION` stays 124; the
  metric vector changes.
- Tests: `quality::tests::an_unspread_solve_realizes_what_every_gap_asks`,
  `a_staff_the_bake_left_behind_reads_as_crowding`,
  `the_separation_is_the_greatest_over_meeting_pairs` (against every pair,
  on boxes drawn at random), `inks_meet_across_within_a_quarter_space`,
  `a_gap_reads_every_staff_above_it` (ink planted past the lines' end,
  where only the merged outline sees it),
  `vertical::tests::the_gap_bands_carry_the_solves_numbers`,
  and the CLI's beam, lyric and compression tests reading the measure.

## ENGRAVER_VERSION 124 → 125: rests (X5d.5, 2026-11-01)

- A rest filling its measure stands centred in its staff's free space in
  the measure, as Gould sets a whole-bar rest (*Behind Bars*, pp. 159-160,
  in the room left where the bar holds a clef, key or time signature):
  between the right edge of its staff's ink in the columns before it that
  hold no note, back to the barline opening the measure (or the system's
  lead where the measure opens the system), and the left edge of its
  staff's ink in the first such column after it, as MuseScore takes them
  (`MeasureLayout::getMeasureStartEndPos`) and LilyPond centres a measure
  rest between its break-alignment groups. Casting sets it once the
  system's map across is known; its column keeps its room. The constrained
  stage names each such rest (`ConstrainedLayoutIR::measure_rests`). It had
  stood at its measure's first column (D70).
- A whole or half rest standing outside its staff, the line it hangs from
  or sits on beyond the staff's lines, takes its ledgered glyph
  (`restWholeLegerLine`, `restHalfLegerLine`, now bundled from the pinned
  Bravura with their metrics), chosen on its final line after it has moved
  clear, as Gould draws it (p. 37) and MuseScore (`Rest::getSymbol`) and
  LilyPond (`rest.cc`) choose it (D73). A rest centred in its measure and
  moved clear of another voice in its column keeps that height where it
  stands; a visiting voice's ink elsewhere in the measure is not cleared,
  named for X5d.7's sweep.
- A staff's rest whose column holds voices visiting from the staves above
  and below at once, their stems turning both ways, takes the side whose
  first clear place stands nearer its staff's middle line, a tie keeping
  the side it took before (D109 Q8); it had always turned down, past the
  lower visitor's stem (X4b review 3's q4). Gould's principle is the same
  (a rest's level follows the pitches around it, p. 37); neither engine
  treats a cross-staff visitor so.
- Tests: the CLI's `a_measure_rest_stands_centred_in_its_measure`,
  `a_measure_rest_opening_a_system_centres_after_its_lead`,
  `a_rest_outside_its_staff_takes_its_ledger_line` and
  `a_rest_between_visitors_from_both_sides_takes_the_nearer_side`; the
  constrained stage's `a_rest_between_visitors_level_both_ways_keeps_its_side`;
  `a_rest_filling_its_measure_is_one_span` now reads its column through a
  hidden rest's anchor, a drawn one standing centred. Two goldens change,
  re-blessed with their renders before and after recorded (D10).

## ENGRAVER_VERSION 125 → 126: a staff of one line (X5d.6, 2026-11-01)

- A staff whose line count is one (the importer reads it from the file's
  `staff-lines`) draws that line alone, where a five-line staff draws its
  middle line, so its clef, time signature and barlines stand about it, the
  stems, beams and rests its middle line decides turn by it, and a roll on a
  note there takes its strokes on the side that line gives (D97: the roll
  MuseScore sets under its head). Its unpitched notes, which MusicXML and
  MuseScore's export place from the line as a five-line staff's bottom line
  (E4), move up by the two spaces between, so they stand on the line the
  file gives. A note a space or more off the line takes ledger lines from
  it, as MuseScore's one-line staff shows them; a whole rest hangs from the
  line (Gould, *Behind Bars*, p. 38). Gould sets a single line for
  instruments of indefinite pitch (p. 5); LilyPond's reference for a staff
  of one line is its line; MuseScore's lines follow its staff type's count.
- For the floor between staves a staff of one line is boxed two spaces
  either side of its line, so it keeps a five-line staff's room;
  `req:qmc:vertical` counts it so (its catalog text says so, the PDF rebuilt).
  The floor is LilyPond's: its least distance between reference points gives
  the same room (Gould's p. 488 reads a stave's height, and a percussion line
  keeps its). It departs from MuseScore, which boxes the staff so for its
  bracket but reads the floor from the line (`SystemLayout`, `dist` from
  `staffHeight`, nil on one line, plus the staff distance), so its line can stand
  the bare distance from the next staff's top line, and a line opening a page
  stands nearer the margin.
- Not changed, on the owner's ruling at X5d.6: a three-line staff keeps
  five lines, to the decide-list. MuseScore can draw such a staff with its
  lines two spaces apart and its notes a space per step, which an export
  records only by its `staff-size` and its two-step shift of the unpitched
  notes.
- The importer records a staff the file hides in a system
  (`<staff-details print-object="no"/>`) as the feature "hidden staff",
  drawing it regardless, where it dropped it silently (D109 Q6).
- Tests: the CLI's `a_staff_of_one_line_draws_its_line_and_stands_about_it`;
  `ledger_steps_cover_only_lines_outside_the_staff` for a one-line frame;
  the importer's `a_hidden_staff_is_recorded_not_dropped`.

## X5d takes `main` (X5d.7, 2026-11-03; ENGRAVER_VERSION 126)

- X4d's lines and grace notes meet X5d's vertical spacing. X5d's entries,
  numbered 95 to 98 on its branch, follow X4d's last (122) as 123 to 126,
  and `ENGRAVER_VERSION` is 126.
- The vertical measure reads a text the casting pass adds after the
  input's (a line's runs, drawn piece by piece: a later piece's reminder,
  a glissando's words) by the band of the line it rides
  (`CastLayout::appended_text_bands`), as it reads an added glyph by
  `appended_bands`. It had read every text's band from the input's, so a
  page holding a line's runs failed (X4d's line tests).
- A grace note's ledger lines stand outside a five-line staff's outer
  lines, as X4d drew them; `ledger_steps` now takes the staff's frame.

## X5d.7: a system's fit reads its ink of no staff, pinned (2026-11-03, no version change)

- The fit that decides whether a system's floor compresses (entry 123)
  reads the ink of no staff with its staves' ink. No test held that: the
  margin band of every score drawn holds only ink inside its staves' range,
  so the fit read the same without it. Pinned by a hand-built test that
  lifts the region's traced anchor, the one primitive its margin band holds,
  three spaces over the top staff, on a page that then holds the system half
  a space short; the floor compresses by that half
  (`ink_of_no_staff_counts_toward_a_systems_fit`).

## ENGRAVER_VERSION 126 → 127: a grace on a staff of one line stands as its notes do (X5d.7, 2026-11-03)

- An unpitched grace on a staff of one line moves up from a five-line
  staff's bottom line, where the file places it, onto the line, and takes
  its ledger lines from that line, as the staff's own notes have since
  entry 126; it had stood two spaces under the line. A pitched grace keeps
  the five-line frame, as a pitched note does. Gould sets a grace before its
  note at the note's own height (*Behind Bars*, pp. 125-126) and a single
  line for indefinite pitch (p. 5). MuseScore lays each grace chord of a
  percussion chord out by the drumset's line as it lays the chord out
  (`SegmentLayout`, `layoutDrumset`), and LilyPond's drum note engraver
  sets every head's staff position from its drum's entry, a grace's too;
  no departure (`an_unpitched_grace_on_a_staff_of_one_line_stands_as_its_notes_do`).

## ENGRAVER_VERSION 127 → 128: a hairpin over a break sets the staves by its pieces as drawn (X5d.7, 2026-11-03)

- A hairpin over a system break is drawn a piece to a system, each opening
  from its axis as MuseScore draws it (entry 59's pieces; now
  `hairpin_piece_ys`, which the bake and the outline share): a crescendo's
  first piece from its tip to its full mouth, a diminuendo's last from its
  full mouth to its tip. The family places each piece by the stroke as cut
  at the break, flat at the leg's one end, and the outlines that set the
  staves (entry 123) and a system's extent read that cut: the drawn mouth
  stood up to its half height (0.575) nearer the next staff's ink than the
  outline held, across the clearance where that ink set the distance.
  X5d.7's sweep found it (every staff's ink against the merged ink above
  it, drawn). Each such piece now feeds the outline and the system's
  extent as drawn, lifted as its piece is; where the family places it is
  unchanged. MuseScore adds each spanner segment's shape, a hairpin's piece
  among them, to its staff's skyline once the segment is laid out
  (`SystemLayout`), so its skyline holds each piece as drawn;
  LilyPond takes a hairpin's skylines from its printed stencil, each
  broken piece's own (`Hairpin`'s `vertical-skylines`); Gould keeps every
  sign clear of the next (*Behind Bars*, p. 41)
  (`a_hairpin_over_a_break_sets_the_staff_below_by_its_mouth`).

## ENGRAVER_VERSION 128 → 129: a grand staff's centred dynamic clears the family placed between its staves (X5d.7, 2026-11-03)

- A dynamic or hairpin of a grand staff is centred between its staves
  "as far as the ink between them lets it" (entry 59), after the spread
  (entry 124). That ink was the sky alone, which holds no member of the
  family once placed: a syllable, a fermata, another dynamic. X5d.7's sweep
  found centred dynamics and hairpins on the syllables of a sung grand
  staff, more often once the spread widens the gap and its middle moves
  onto them. The centring now reads, besides the sky, every family member
  placed on either staff in that system where it stands, the centred row's
  own members aside: one beyond the member on its own staff limits its move
  toward that staff, one on the other staff its move toward the other.
  MuseScore's centring reads both staves' skylines whole, lyrics and marks
  among them, keeping each item's least distance
  (`SystemLayout::centerElementBetweenStaves`); LilyPond centres a
  `Dynamics` line between two staves (`staff-affinity` CENTER) as a line of
  its own, set by skylines beside a `Lyrics` line as any line is; Gould
  keeps every sign clear of the next (*Behind Bars*,
  p. 41) (`a_centred_dynamic_stands_clear_of_the_syllables_between_the_staves`,
  `a_centred_dynamic_stands_clear_of_its_own_staffs_family_outside_it`).

## ENGRAVER_VERSION 129 → 130: a centred rest carries what stands on it (X5d fix round 1, 2026-11-04)

- Entry 125 centres a rest filling its measure and moved its one glyph at the
  bake, after the family, the sky and the outlines had read it at its first
  column. The centre is now set as soon as the system's map across is, which
  is all it needs, and every later step reads the rest where it is drawn: the
  fermata's clearance, the sky, the staves' outlines, the centring between a
  grand staff's staves.
- What goes with it, `MeasureRest` naming each by its event: a fermata, centred
  on its rest (*Behind Bars*, p. 188; MuseScore's `TLayout::layoutFermata`;
  LilyPond's `R1\fermata`), a member of the family moved across as a dynamic off
  a barline is; a slur's end, which stands at the rest's centre, the arc sheared
  as an anchored curve's is (MuseScore's `slurPos` reads the rest's own place);
  a hidden rest's fermata, since a hidden rest is centred as any other for what
  stands on it, as MuseScore centres one. Two more are settled by probes beside
  MuseScore's page: a syllable on the rest, a child of
  the rest there though `LyricsLayout::layout` reads as if it were not (it
  negates the rest's move at a time the rest has not moved); and an ottava whose
  last ink is the rest, a space past it where it is drawn (`Ottava::linePos`
  reads the segment's shape), never past the next note. Gould is silent on those
  two and on the slur; LilyPond attaches no syllable to a rest.
- What stays at its column, by what it is anchored to and not by a measure of
  ours: a dynamic and words (`DynamicsLayout` anchors to the segment, probe), a
  trill line and its sign (`SLine::linePos` reads the segment's x, probe), a
  tempo or rehearsal mark, and a hairpin's ends, which are a time's. A melisma
  line does not end on a rest: `lyrics_of` ends it on its last note. The rest's
  box stays in what a grace's principal reads at its column, whose left edge a
  rest's does not pass, and in what a glissando's ends read there.

## ENGRAVER_VERSION 130 → 131: a measure rest is cleared where it stands (X5d fix round 1, 2026-11-04)

- Entry 125 centres a rest filling its measure with the height `clear_rests` and
  `clear_visitors` gave it at its first column: it could stand on ink at its
  centre, and stood off its line in empty space for a note it had left. Its
  height is now decided once it stands across (`rests::clear`), from its default
  height, by whole spaces, reading the ink of its staff's note columns in its
  measure where it is drawn. Gould moves a rest only as far as it takes to
  keep the two parts apart (*Behind Bars*, p. 36), by exact spaces (p. 35), and
  keeps every sign clear of the next (p. 41). No figures are given: these are MuseScore's
  (`RestLayout::checkFullMeasureRestCollisions`, under D99), 0.75 up and down
  within 4 across.
- Its exclusions are not copied. MuseScore takes accidentals out of what it
  reads, and a rest stands on a sharp; it runs only where the measure has voices,
  and a rest stands on a visitor's head. Read here: every glyph and stroke of the
  staff in a note column of the measure, a visitor's and a grace's and a tuplet's
  included. Not read, each for its reason: what the family places after it, which
  is placed against the rest; the columns of a clef or a key or time signature,
  which bound its free space and which it does not stand in; another rest filling
  the measure, as MuseScore; the stem whose end rides another staff, whose height
  the gap between the staves sets later, as the ink of a beam across two staves is
  read on neither. A tie or slur is read as well, only where it passes through the
  rest, a quarter space up and down, and the rest moves its voice's way: MuseScore
  reads none and a rest stands on a held note's tie, Gould keeps every sign clear
  of the next, and a curve near it is no reason to move.
- A voice's rest keeps past the other part, above all of that ink for an upper
  voice and below it for a lower, as Gould takes a ledger line to the far side of
  the other part (p. 37) and MuseScore's one-sided clearance reads. A rest
  alone on its staff has no part to keep outside: it moves only where ink stands
  within that box, from the stems it meets, and D109 Q8 holds at the new place:
  between visiting stems of both ways within 4 across it takes the side whose
  first clear place stands nearer its staff's middle line, each place read with
  the glyph it would draw.
- A whole or half rest's ledgered glyph is decided with its height, after it, so
  its box is read at the height that draws it (`restHalfLegerLine` is 2.07 wide
  against `restHalf`'s 1.13): in `rests::clear`, in `clear_rests` and
  `clear_visitors` for a rest not filling its measure, and in `clear_ledgers`,
  which lifts a ledgered rest beside another voice off that voice's ink, an
  accidental above all, which the ledger line reaches and the plain glyph,
  beside the heads `clear_rests` reads, does not.

## ENGRAVER_VERSION 131 → 132: a group's sign spans a staff of one line as boxed (X5d fix round 1, 2026-11-04)

- Casting spanned a brace, a bracket and the system's opening line from each
  staff's lowest and highest line, one height on a staff of one line, so a
  bracket ended at the line and its hook stood on the staff's clef. They now read
  that staff boxed two spaces either side of its line, as the solve boxes it
  (entry 126). MuseScore spans a bracket from the box of its first staff to the
  box of its last (`SystemLayout`, `sy` and `ey`). LilyPond ends a bracket at the
  line and insets the clef, a way we do not take, since our clef stands flush;
  Gould keeps every sign clear of the next (*Behind Bars*, p. 41). A joined barline runs
  between the same spans, which is where a one-line staff's barline already ends.

## ENGRAVER_VERSION 132 → 133: a whole-bar rest in 2/1 or more is a breve rest (X5d fix round 1, 2026-11-04)

- A rest filling its measure drew a semibreve rest in every meter. From 2/1 it
  draws a breve rest (*Behind Bars*, p. 160; MuseScore's `Rest::getSymbol` reads
  a bar of two whole notes or more; LilyPond's whole-bar rest takes it at 2/1),
  filling the space above the middle line (p. 35), or, on a staff of one line,
  hanging from it (MuseScore's `computeWholeOrBreveRestOffset`, LilyPond's
  `staff_position_internal`). `restDoubleWholeLegerLine` joins the bundled glyphs
  from the pinned font, the others reproduced byte for byte. Its height, ledgered
  glyph and box go through entry 131's rule. Where it hangs from a staff's last
  line MuseScore draws the ledgered glyph and LilyPond the plain one (`rest.cc`:
  a breve rest needs a ledger only if neither lying on nor hanging from a line);
  this follows MuseScore, one test for every whole and half rest, and a probe is
  all that reads it. A rest of a
  pickup's whole length is not touched: it stays on the decide-list.

## ENGRAVER_VERSION 133 → 134: gaps rise level with a gap past its cap (X5d fix round 1, 2026-11-04)

- `vertical::spread` levelled the page's smaller gaps up to the widest gap still
  under its cap. A braced part's staves held apart by their ink stand past theirs,
  and MuseScore's `smallest` passes over a gap only at its cap exactly, so its
  other gaps rise level with one past it. The level now reads every gap the spread
  may move, one past its cap too. Gould asks only that distances follow the
  notation (*Behind Bars*, p. 488); LilyPond's single force has no such case; this
  is MuseScore's reading where it is a rule rather than a loop's bound (entry 124)
  (`gaps_rise_level_with_a_gap_past_its_cap`,
  `a_soloist_rises_level_with_a_braced_parts_held_gap`).

## ENGRAVER_VERSION 134 → 135: a caesura leaves a measure rest at the bar's middle (X5d fix round 1, 2026-11-04)

- A measure rest's free space read its staff's ink in any column holding no note,
  so a caesura or breath mark closing the bar, or standing inside it, narrowed it.
  Only a clef, a key or time signature or a barline does now: Gould centres the
  rest in the space they leave (*Behind Bars*, p. 160), MuseScore's end point stops
  at those and the end barline (`getMeasureStartEndPos`), and LilyPond centres
  between break-alignment groups. Reading those names leaves a member of the family
  out as well as the rest, so the earlier filter on the family is gone with it
  (`a_caesura_leaves_a_measure_rest_at_the_bars_middle`,
  `a_breath_mark_inside_the_bar_leaves_a_measure_rest_at_its_middle`,
  `a_clef_closing_the_bar_bounds_a_measure_rest`).

## ENGRAVER_VERSION 135 → 136: a centred mark stays inside its own staff's family (X5d fix round 1, 2026-11-04)

- In a wide gap a grand staff's centred dynamic moved past a pedal line of its own
  staff that stood further out across it, and stood beyond it, clear of it and out
  of order. A row with a member of its own staff's family further out across any of
  its members now stays where it was placed: Gould keeps dynamics between the staff
  and what stands further out (*Behind Bars*, p. 493), MuseScore returns before the
  move (`SystemLayout::elementHasAnotherStackedOutside`), and LilyPond has no such
  case (`a_centred_dynamic_stays_inside_a_pedal_further_out_across_it`,
  `a_pedal_elsewhere_in_the_bar_leaves_a_dynamic_centred`,
  `a_sung_staffs_centred_dynamic_stays_inside_its_words_further_out`).

## ENGRAVER_VERSION 136 → 137: a later system's lead joins the centring's bounds (X5d fix round 1, 2026-11-04)

- The centring's bounds read the sky and the family placed on either staff, and no
  later system's lead, which is its staff's ink at the left margin and which the
  sky does not hold (entry 123's outlines read it). A centred mark that a first
  column's note set across that clef's reach could stand within its least distance
  of it. The leads join both staves' bounds, as MuseScore's centring reads both
  skylines whole (`SystemLayout::centerElementBetweenStaves`)
  (`a_centred_dynamic_keeps_its_distance_from_a_later_systems_lead`).

## ENGRAVER_VERSION 137 → 138: a centred text in no row is centred by its own box (X5d fix round 1, 2026-11-04)

- The centring read a row's centre, and a mark in no row, an expression text with no
  dynamic beside it, from a centre of 0, a place on no page: its move came out as
  the distance from the page's origin to the gap's middle, and the clamp alone
  stopped it, a space or more off the middle. It now reads its own box's centre. X4d
  gave such a text its `between` and no row; MuseScore centres the element's shape
  (`SystemLayout::centerElementBetweenStaves`)
  (`a_centred_text_with_no_dynamic_stands_at_the_gaps_middle`).

## The pins of the phase's clauses, and what was not reachable (X5d fix round 1, 2026-11-04; ENGRAVER_VERSION stays 138)

- Every clause of the phase's rules that a mutation showed no test holding is now
  held by a test of its own, each seen failing with its mutation:
  `crates/epiphany-cli/tests/vertical_rules.rs`, the clauses of the staves'
  distances, the spread, the rests, the centring and a hairpin's lifted piece.
- Removed, each proved unreachable and changing no page. A brace's edge read
  inside another brace: a staff holds one group (D26), so no brace's edge falls
  inside another brace, though a brace can span two parts. A system past its
  paper's top: the rise that sets it stops at the
  top margin, so only its bottom can pass. A split hairpin's piece index, and the
  guard to split ones: each leg's outer edge stands at the mouth whichever piece
  it is, and a whole hairpin's box is that rectangle already, so the outline reads
  the mouth (entry 128). A hairpin of no staff in the extent: a hairpin with no
  staff draws a traced anchor. A measure rest's check that its free space holds it:
  its column keeps its room. A family glyph in a rest's furniture went with entry
  135.
- Kept and pinned by an amended constrained input, as entry 125's fit is, since the
  importer makes neither: the ink of no staff and a family mark of no staff in the
  gap between systems.
- The level-both-ways test of `clear_visitors` also runs on a half rest, whose
  centre stands off its line, so the glyph's centre is what it reads.

## ENGRAVER_VERSION 138 → 139: a tie passing through a measure rest moves it (X5d fix round 1, 2026-11-04)

- Entry 131 returned a rest to its line where nothing stands near it at its centre,
  and one came down onto a tie held across its bar. A tie or slur of its staff that
  passes across it within a quarter space now moves it its voice's way, by whole
  spaces, only as far as that curve needs (the entry's wording stands above). The
  sky reads the curve at the bake's chords, before the bake reshapes a slur past
  what it passes (`a_measure_rest_clears_a_tie_passing_through_it`,
  `a_curve_moves_a_rest_only_where_it_passes_through_it`).

## ENGRAVER_VERSION 139 → 140: a dotted rest keeps its dot beside it (X5d fix round 2, 2026-11-05)

- A rest's dot stood where a lone rest's stands whatever moved the rest: a voice's
  shift at construction moved the rest alone, and `clear_rests` moved it alone off
  the other voice's heads, so a dot could stand spaces from its rest, beside
  another note, which reads as that note's. Older than the phase (X3).
- `dots_y` now takes the voice's shift and each of `clear_rests`'s steps, as
  `clear_visitors` and `clear_ledgers` already moved it: the dot stays half a space
  over a half rest, its place against its rest wherever the rest stands (*Behind
  Bars*, p. 38). MuseScore lays a rest's dots from the rest's own place
  (`RestLayout`, `dotLine`), and LilyPond hangs them from their rhythmic head
  (`dots-engraver.cc`), a rest being one.
- The dot's box is read where it now stands: `clear_visitors` reads it as the rest
  moves, and it enters its staff's outline, so a dot's new place can move a staff
  by what it adds to it.
- Each move and its dot, `a_voices_dotted_rest_keeps_its_dot_beside_it`,
  `a_dotted_rest_moved_off_a_voices_heads_takes_its_dot`,
  `a_ledgered_rests_dot_moves_with_it_off_an_accidental`; the last read to the exact
  half space, since a dot left one step behind still stands within a space of its
  rest.

## ENGRAVER_VERSION 140 → 141: a measure rest reads no slur of its own, and a slur's end follows it (X5d fix round 2, 2026-11-05)

- Entry 139's rest read every curve of its staff, its own slur among them, at the
  place the system's map sets the slur before entry 130 carries its end to the rest:
  a slur on a raised rest moved it further for nothing, two raised rests beside
  each other standing two spaces apart for it. A rest now leaves out the slurs
  standing on it, since they go where it goes.
- A slur's end on a rest moved off its height follows it: it stands 0.9 off the
  rest's ink on the arc's side, MuseScore's figure (`SlurTieLayout::slurPos`),
  LilyPond's `slur-scoring.cc` also reading a rest in its column as the head a
  slur's end stands on, Gould silent. The arc keeps its shape over the chord it now
  has. An end on a rest that nothing moved stays over the staff, where it was.
- Every other rest reads a carried slur where it is drawn, by order: each rest's
  place across is set first, across being all it reads, so the carry across is known
  to all; then the rests carrying a slur's end are cleared first, so the rests under
  that slur read it at its height.
- Named, not settled: two rests that each carry a slur over the other cannot each
  read the other's as drawn. The later cleared does; the earlier reads the other's
  slur with its carry across, at its height before the carry.
- `a_measure_rest_reads_no_slur_of_its_own`,
  `a_slurs_end_stands_off_a_rest_raised_from_its_height` and its lowered mirror,
  `a_slurs_end_stays_on_a_rest_at_its_height`,
  `a_rest_reads_a_slur_at_the_height_its_end_is_carried_to`,
  `a_rest_reads_a_slur_at_the_place_its_end_is_carried_to`.

## ENGRAVER_VERSION 141 → 142: a measure rest reads no ink of a repeat structure (X5d fix round 2, 2026-11-05)

- Entry 131 left out of what a rest reads "what an ending carries", by a map that
  holds only an ending moved to the next system, so every other ending's line read
  as the other part's ink: an upper voice's rest climbed over a one-bar ending's
  bracket, outside it. The rest now leaves out a repeat structure's ink by its source,
  the stroke and the glyph, whichever system or bar it stands on. MuseScore reads a
  measure's segments alone, which hold no volta (`restlayout.cpp`), LilyPond's rest
  collision reads note columns, and Gould moves a rest only where parts would collide
  (p. 36).
- No import reaches it: the importer records endings unmade, so the tests add the
  ending through the model: `a_measure_rest_stays_under_a_one_bar_ending`, and
  `a_measure_rest_stays_under_an_ending_of_three_bars`, the line's middle inside the
  middle bar.
- Every glyph a repeat structure draws rides a barline's or the end's column, whose
  members a rest does not read, so no score reaches the glyph's half. It stays, as
  the source's whole statement, pinned by an amended constrained input,
  `a_measure_rest_reads_no_glyph_of_a_repeat_structure`.

## ENGRAVER_VERSION 142 → 143: a syllable carried with its rest carries its hyphen and melisma line (X5d fix round 2, 2026-11-05)

- Entry 130 carried a syllable on a measure rest and nothing that follows it: a hyphen
  stood before the syllable it follows, and a melisma line started five spaces left of
  its word and ran under it. `MeasureRest` now names the lines leaving and reaching
  each syllable on it, and casting moves them by the route a line's end already takes:
  a hyphen from the syllable moves its start, a hyphen to it its end, a hyphen between
  syllables on two rests both, and a melisma line its start. The dashes are then
  shared out between the syllables as they stand, midway (*Behind Bars*, p. 448), and a
  melisma line follows its word (p. 447). MuseScore's page draws both so; LilyPond
  attaches no syllable to a rest.
- A hidden rest's syllable stays at its column, having no glyph to ride, so its lines
  stay too.
- Named, not settled: a long syllable's room. A bar's width is set with the syllable at
  its column, so a long syllable centred with its rest can overlap the next bar's
  syllable. Its room belongs to the spacing, before any bar's width and so its middle
  is known; a carry cut short at the neighbour would read the verse row where the
  carry is set, and neither is a few lines.
- `a_hyphen_stands_after_its_syllable_on_a_rest`,
  `a_hyphen_stands_before_its_syllable_on_a_rest`,
  `hyphens_stand_on_either_side_of_a_syllable_on_a_rest`,
  `a_hyphen_between_syllables_on_two_rests_moves_both_its_ends`,
  `a_melisma_line_starts_after_its_syllable_on_a_rest`,
  `two_verses_on_a_rest_stand_together_under_it`,
  `a_hyphen_reaching_a_syllable_on_a_hidden_rest_stays_before_it`.

## ENGRAVER_VERSION 143 → 144: a stemless visitor turns no rest (X5d fix round 2, 2026-11-05)

- A rest alone on its staff turned from the stems it met, and every visiting box
  carried its component's stem direction, a whole note's too, so a rest climbed past a
  visitor's head, turned by a stem that is not there, where a space down cleared it.
  A box now carries its stem's direction only where that stem is drawn: `MeasureRest`
  lists no stemless visitor, and `rests::clear` and `clear_visitors` read none.
- With no drawn stem to turn from, a rest alone takes the nearer side, as between stems
  of both ways (D109 Q8), a tie going down: it goes no further than the parts need
  (Gould, p. 36). Where a stem is drawn, the side it gives stands. `clear_visitors`
  decides by what the rest meets: a stem elsewhere in its column that it does not meet
  does not turn it from a stemless head it does. A curve through a rest with no stem
  near it reads the same way.
- MuseScore does not move such a rest at all (`hasVoices`, `restlayout.cpp`), so it can
  stand on the head; LilyPond's rest reads no note from another staff. Both leave ink
  meeting that Gould's rule does not (p. 41), so the rest still moves.
- `a_rest_alone_beside_a_stemless_visitor_takes_the_nearer_side`,
  `a_shorter_rest_beside_a_stemless_visitor_takes_the_nearer_side`,
  `a_rest_alone_with_no_stem_near_takes_the_nearer_side`,
  `a_rest_alone_meeting_only_a_stemless_head_takes_the_nearer_side`.

## The rest's clauses pinned (X5d fix round 2, 2026-11-05; ENGRAVER_VERSION stays 144)

- Clauses of entries 130, 131 and 136 that a mutation showed no test holding, each with
  a test seen failing with its mutation, in `vertical_rules.rs`'s `rest_clauses` and
  `rests.rs`: a tie read at its arc and at its thickness; each half of the measure
  filter, the bar before and the bar after; a slur's carry moving its own end alone,
  either end; "across", a mark ended before the item; a stem far to the left of a rest
  alone; a slur on a hidden rest carried, a syllable on one staying; an ottava's end
  reading its own staff's rest, and the drawn rest beside a hidden one; a measure rest's
  stems holding its own bar alone, read at the constrained stage, since at the layout a
  visitor's bar before stands past the rest's reach.

## What no input reaches, and what stays named (X5d fix round 2, 2026-11-05; ENGRAVER_VERSION stays 144)

- Removed, each by its proof and changing no page. A measure's right barline drawn back
  by `shift_of`: only the columns before a later system's first note or rest are drawn
  back, and that barline stands after the rest's own column, so nothing is. `clear_ledgers`'s
  `event != eid`: the ink it reads is heads and what they carry, keyed by a note's event,
  and `eid` is a rest's, so the event is gone from the map as well.
- `rests::clear`'s bound keeps its loop and gains a `debug_assert!`, as `clear_visitors`
  has: walking up, a rest needs at most the ink's top, the clearance and its own depth
  below its origin, plus one, which `bound` exceeds for every height and glyph, and down
  alike, so a move past it fails a debug build instead of settling on ink.
- `clear_ledgers`'s `seg.visible` stays, named: a hidden rest draws nothing, and the clause
  keeps its undrawn anchor at its height.
- `RestInk::within`'s zero-width branch stays, no guard against an impossible case: a
  stroke's edges stand a thickness apart, but a curve's chord has no thickness, so an
  upright chord reaches it (`an_upright_chord_reads_at_its_first_end`).
