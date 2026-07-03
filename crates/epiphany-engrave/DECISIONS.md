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
greedy system breaking at measure boundaries, vertical system stacking, page
assignment, a populated `ResolvedPage`/`ResolvedSystem` tree, and full break-
constraint evaluation. Still deferred: the vertical soft-spring solve within a
system, per-system justification/stretch, and optimal break search.

### Honest tier

By the same rule `layout-ir`'s `StubSolver` follows, a solver that does not
evaluate the declared hard constraints and computes no quality metrics MUST
report `SolverTier::Stub`, never `Minimal` (Chapter 9 §"Conformance Tiers").
`Engraver::tier()` reported `Stub` until real hard-constraint satisfaction
landed and now reports `Minimal` — which it fully earns after casting-off: the
break constraint family is genuinely supported (spec §"Conformance Tiers",
Minimal row), and `Minimal` makes no optimality claim, so greedy first-fit
casting-off is legitimate. The quality-metric vector stays the conservative
all-worst placeholder (`QualityMetricVector::unmeasured`) until the Quality Metric
Catalog lands (`Standard` tier work).

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

1. **Greedy first-fit system breaking, at measure boundaries.** The casting-off
   walk visits each region's spaced spring-slot columns in x order and breaks
   before a **barline column** (this projection draws each measure's barline at
   its *start* column, so breaking before the barline keeps every measure
   intact; the region-final barline closes the region and is never a
   candidate) whenever the measure beginning there would overflow the page
   content width. Rationale: `SolverTier::Minimal` requires the break family
   supported and hard constraints satisfied, with **no optimality claim**
   (Chapter 9 §"Conformance Tiers"), so an optimal (Knuth–Plass-style) search
   is deliberately rejected at this tier — greedy first-fit is deterministic,
   linear, and easy to validate. Consequences accepted and documented:
   a region with no measures never wraps automatically; a single measure wider
   than the page yields an overfull system (no mid-measure emergency break).
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
   at an 8 mm staff.** The spec names `Canvas.layout_defaults` ("paper size,
   margins") but never defines the type, and core does not implement it;
   adding a graph field now would violate the companion's frozen-layout rule,
   so the graph home (`CanvasLayoutDefaults`) is **staged to the data-model
   schema major** and the engraver takes the geometry as a constructor
   parameter. Default arithmetic (1 staff space = staff height / 4 = 2.0 mm at
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
9. **Deferred refinements** (named, not implied): per-system justification
   (stretching the soft springs so every full system ends at the right
   margin); the vertical spring solve (band heights are carried, not yet
   renegotiated; systems stack by real content extents); widow/orphan control
   and optimal/lookahead casting-off quality (a `Standard`-tier concern, with
   `casting_off_quality` in the metric vector); casting-off caching /
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
