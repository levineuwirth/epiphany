//! The **casting-off pass** — Minimal-tier system breaking, vertical stacking,
//! and page assignment (Chapter 9 §"The Constraint-Solving Stage": the solver
//! "resolve\[s\] page and system breaks"; Chapter 7 §"ResolvedLayoutIR" defines
//! the page/system tree this pass populates).
//!
//! ## The algorithm (optimal break search)
//!
//! [`SolverTier::Minimal`](epiphany_layout_ir::SolverTier) requires the break
//! constraint family to be supported and every hard constraint satisfied (or an
//! honest `Unsatisfiable`); it makes **no optimality claim**. Casting-off uses a
//! deterministic **badness-minimizing break search** (`optimal_breaks`, a
//! Knuth–Plass-style dynamic program) — an honest improvement on the earlier
//! greedy-first-fit-plus-widow-rebalance, not a formal optimality guarantee.
//!
//! 1. **System breaking.** Per region, partition the measures into systems to
//!    minimize the total squared normalized underfill over ALL systems — which
//!    evens them (no lopsided split) and fills them (no needless breaks), the
//!    additive analog of the Quality Metric Catalog's break/imbalance
//!    distribution cost; including the final system in the sum is what removes
//!    the old separate widow rebalance. Breaks fall only at **measure
//!    boundaries** — the barline columns (`to_constrained` draws each measure's
//!    barline at its start column; the region-final barline closes the region
//!    and is never a break candidate). A **hard** `SystemBreakAt`/`PageBreakAt`
//!    is *always* honoured at its slot (and bounds the search's segments); a
//!    **soft** one is honoured unless doing so would close a system with no
//!    musical content (no notehead/rest column) — the documented exceptional
//!    path, recorded as an [`EngravingDecision`] with
//!    [`DecisionSource::IrOverride`] per the spec's override-resolution rule (an
//!    unhonoured override is recorded, not silently dropped). A region with no
//!    measures has no break candidates: it stays one (possibly overfull) system
//!    unless breaks force otherwise. A single measure wider than the page yields
//!    an overfull system — Minimal does not break mid-measure on its own.
//! 2. **Vertical stacking.** Each system's height is its real content extent
//!    (glyph boxes plus stroke extents — the vertical spring solve that would
//!    renegotiate band heights is deferred, so the constrained `y` geometry is
//!    authoritative); consecutive systems are separated by the vertical-band
//!    model's **inter-system gap** ([`VerticalBand::inter_system_gap`], the
//!    preferred height — genuinely read from the band constructor so the two
//!    cannot drift). Systems that no longer fit the page content height start
//!    the next page.
//! 3. **Page assignment and the world frame.** Pages stack **vertically in one
//!    world**: page *n*'s top edge sits [`INTER_PAGE_GAP`] staff spaces below
//!    page *n−1*'s bottom edge, page 1's top-left corner at the origin (world
//!    is y-up, so pages grow downward in −y). Every glyph and stroke position
//!    is **baked** into this single world frame (each system is translated
//!    rigidly: x back to the left margin, y to its stacked position), so the
//!    flat glyph/stroke lists remain the renderer's and hit-tester's single
//!    coordinate space — no per-page transform exists anywhere downstream.
//!
//! ## Region-spanning strokes
//!
//! A stroke confined to one system (a stem, a ledger, a barline-anchored mark)
//! translates rigidly with it. A stroke spanning several systems — in practice
//! the five staff lines, which `to_constrained` draws across the whole region —
//! is **split** at the system boundaries: the first segment keeps the original
//! stroke's exact provenance (so the round-trip's preservation contract holds),
//! and each later segment is engraver-**synthesized** from the same source
//! ([`SynthesisKind::Registered`] under [`SYSTEM_CONTINUATION_SYNTHESIS`], the
//! codebase's convention for a synthesis kind the normative vocabulary does not
//! name), keyed by [`continuation_instance_key`] so segments of different lines
//! can never collide.
//!
//! ## Default page geometry
//!
//! The spec names `Canvas.layout_defaults` ("paper size, margins") but does not
//! define its type, and the core graph deliberately does not carry it yet (the
//! graph home is staged to the data-model schema major — see `DECISIONS.md`),
//! so page geometry is an **engraver-side parameter** ([`PageGeometry`], a
//! constructor argument of [`crate::Engraver`]) with a documented default; see
//! [`PageGeometry::default`] for the arithmetic.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{StaffId, TypedObjectId};
use epiphany_layout_ir::{
    continuation_instance_key, inter_staff_gap_id, is_barline_glyph, is_rigid_width_stroke,
    synthesized_layout_id, BreakClass, BreakKind, ConstrainedLayoutIR, Curve, DecisionSource,
    EngravingDecision, EngravingDecisionKind, EngravingOverrideId, GlyphObject, GlyphObjectId,
    LayoutConstraint, LayoutObjectId, Margins, Point, Provenance, Rect, ResolvedGlyph,
    ResolvedMeasure, ResolvedPage, ResolvedStaff, ResolvedSystem, Size2D, SpringSlotId, StaffSpace,
    Stroke, SynthesisInstanceKey, SynthesisKind, SynthesisRegistryId, VerticalBand, VerticalBandId,
    VerticalBandKind,
};

use crate::owning_glyph;

/// The registry id for the engraver's **system-continuation synthesis**: the
/// segment of a region-spanning stroke (a staff line) that casting-off places
/// in a system after the stroke's first. The normative [`SynthesisKind`] set
/// names no purely visual continuation rule, so — like the constrained stage's
/// staff-line/ledger/accidental syntheses — it is carried as a `Registered`
/// extension kind (Chapter 7 §"Behavior Under Unknown Extensions").
pub const SYSTEM_CONTINUATION_SYNTHESIS: SynthesisRegistryId =
    SynthesisRegistryId(0x5359_5354_4D53_4547); // "SYSTMSEG"

/// The vertical gap between consecutive **pages** in the single world frame, in
/// staff spaces. Pages are separate physical sheets; this gap exists only in
/// the continuous scroll-like world the renderer and hit-tester share, so it is
/// a presentation constant, not engraving geometry.
pub const INTER_PAGE_GAP: f32 = 8.0;

/// Namespace bit for a synthesized *system* provenance instance key (a region's
/// second and later systems), disjoint from the page namespace below and — by
/// 128-bit-hash construction — from the slot-identity keys of break decisions.
const KEY_NS_SYSTEM: u128 = 1;
/// Namespace bit for a synthesized *page* provenance instance key.
const KEY_NS_PAGE: u128 = 2;

/// Page geometry the engraver casts off against: the page size and margins, in
/// staff spaces (Chapter 7 §7.2: IR coordinates are staff spaces). A parameter
/// of [`crate::Engraver`] because the score graph has no home for it yet — the
/// spec's `Canvas.layout_defaults` is named but never defined, and adding a
/// graph field is a data-model schema-major change (see `DECISIONS.md`).
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PageGeometry {
    /// Full page size, in staff spaces.
    pub size: Size2D,
    /// Page margins, in staff spaces.
    pub margins: Margins,
}

impl PageGeometry {
    /// The horizontal content extent a system may fill: page width minus the
    /// left and right margins. Non-positive geometry disables automatic
    /// wrapping (treated as unbounded) rather than failing the solve.
    pub fn content_width(&self) -> f32 {
        self.size.width.0 - self.margins.left.0 - self.margins.right.0
    }

    /// The vertical content extent a page may fill: page height minus the top
    /// and bottom margins. Non-positive geometry disables page overflow
    /// (treated as unbounded) rather than failing the solve.
    pub fn content_height(&self) -> f32 {
        self.size.height.0 - self.margins.top.0 - self.margins.bottom.0
    }
}

impl Default for PageGeometry {
    /// **A4 portrait at an 8 mm staff height** (rastral ≈ size 1, a common
    /// full-size instrumental-part raster), 15 mm margins. The arithmetic, with
    /// 1 staff space = staff height / 4 = 2.0 mm:
    ///
    /// * page: 210 mm × 297 mm → **105 × 148.5** staff spaces;
    /// * margins: 15 mm each → **7.5** staff spaces;
    /// * content area: 180 mm × 267 mm → **90 × 133.5** staff spaces.
    ///
    /// 90 staff spaces of content width wraps the QUICKSTART's ten-measure
    /// hand-off fixture (whose spaced width is ≈ 99 staff spaces) into two
    /// systems — an honest multi-system default rather than one that only ever
    /// produces the degenerate single line.
    fn default() -> Self {
        PageGeometry {
            size: Size2D {
                width: StaffSpace(105.0),
                height: StaffSpace(148.5),
            },
            margins: Margins {
                top: StaffSpace(7.5),
                right: StaffSpace(7.5),
                bottom: StaffSpace(7.5),
                left: StaffSpace(7.5),
            },
        }
    }
}

/// What the casting-off pass produced: the final world-frame geometry, the
/// populated page/system tree, the engraver's appended break decisions, and the
/// break structure the constraint evaluation consults.
pub(crate) struct CastLayout {
    /// Final glyphs, in input order, positions baked into the world frame.
    pub glyphs: Vec<ResolvedGlyph>,
    /// Final strokes: the input strokes in order (each translated with its
    /// system; a system-spanning stroke replaced by its first segment), then
    /// the synthesized continuation segments.
    pub strokes: Vec<Stroke>,
    /// Final curves, in input order, each translated with its system. A curve
    /// spanning a system break is split into per-system sub-curves by de
    /// Casteljau subdivision (the first keeps the source's provenance, the rest
    /// are synthesized continuations, like system-spanning strokes).
    pub curves: Vec<Curve>,
    /// The populated page tree (empty when the input declares no regions).
    pub pages: Vec<ResolvedPage>,
    /// Break decisions this pass made (chosen breaks in reading order, then
    /// the skipped-soft `IrOverride` records in walk order).
    pub decisions: Vec<EngravingDecision>,
    /// Slots at which the final layout breaks: the first slot of every system.
    pub system_start_slots: BTreeSet<SpringSlotId>,
    /// Slots at which a page begins: the first slot of each page's first system.
    pub page_start_slots: BTreeSet<SpringSlotId>,
    /// Which system (global index, page order) each realized slot landed in —
    /// the casting pass's own assignment, which the quality-metric census
    /// ranges over (a slot absent here was claimed by no region and its glyphs
    /// belong to no per-system aggregate).
    pub system_of_slot: BTreeMap<SpringSlotId, usize>,
    /// The system each baked stroke landed in, parallel to `strokes` (including
    /// the appended continuation segments). A stroke carries no spring slot, so
    /// `system_of_slot` cannot answer for it; the casting pass records what it
    /// already knew. `None`: claimed by no region.
    pub stroke_system: Vec<Option<usize>>,
    /// The system each baked curve landed in, parallel to `curves`.
    pub curve_system: Vec<Option<usize>>,
    /// The region each system slices, indexed by global system index (the
    /// per-region grouping the casting-off quality metrics aggregate by).
    pub region_of_system: Vec<usize>,
}

/// One realized spring slot in spaced (pre-casting) coordinates, with the
/// classification the greedy walk needs.
struct SlotInfo {
    id: SpringSlotId,
    /// Reference x: the first member glyph's spaced baseline.
    x: f32,
    /// Leftmost content edge (member glyph boxes plus their rigid strokes).
    lo: f32,
    /// Rightmost content edge.
    hi: f32,
    /// Member glyph indices into the (parallel) input/spaced glyph vectors.
    members: Vec<usize>,
    /// The column carries a barline glyph — a measure boundary.
    barline: bool,
    /// The column carries the region-final barline (never a break candidate).
    final_barline: bool,
    /// The column carries musical content (a notehead or a rest).
    note: bool,
    /// The directly-manifested barline glyph of a measure *start* (glyph
    /// index), for the per-system measure records. `None` at the final
    /// barline: that measure's start is not marked by any column in this
    /// projection, so its record is omitted rather than fabricated.
    measure_barline: Option<usize>,
}

/// A break requirement a constraint declares at a slot.
#[derive(Copy, Clone)]
struct BreakReq {
    page: bool,
    hard: bool,
}

/// The boundary decision that opened a system (absent at a region's first).
#[derive(Copy, Clone)]
struct Boundary {
    slot: SpringSlotId,
    source: DecisionSource,
}

/// One cast-off system: which region it slices and which of that region's
/// slots it carries.
struct SystemPlan {
    region: usize,
    /// Region-local ordinal (0-based).
    local: usize,
    /// Indices into the region's ordered slot vector.
    slots: Vec<usize>,
    boundary: Option<Boundary>,
    /// A page must start at this system (a page-break request sits here).
    page_forced: bool,
    /// Attribution for a forced page start (the page-break decision's source).
    page_source: DecisionSource,
}

/// A stroke's casting fate: ride one system rigidly, or split at system
/// boundaries.
enum StrokeFate {
    /// Translate the whole stroke with this system (`None`: not covered by any
    /// region — left untransformed in the spaced frame, on no page).
    Rigid(Option<usize>),
    /// Per-system segments, ascending system order: `(system, from, to)` in
    /// spaced coordinates.
    Split(Vec<(usize, Point, Point)>),
}

/// A curve's casting fate: ride one system rigidly, or split at system
/// boundaries into per-system sub-cubics (de Casteljau).
enum CurveFate {
    /// Translate the whole curve with this system (`None`: not covered by any
    /// region).
    Rigid(Option<usize>),
    /// Per-system sub-cubics, ascending system order: `(system, control points)`
    /// in spaced coordinates.
    Split(Vec<(usize, [Point; 4])>),
}

/// A system's world-frame placement: a vertical shift `dy` plus a horizontal
/// affine map `world_x = a·x + b`.
///
/// A rigid (unjustified) system has `a = 1`, `b = dx` — a pure translation. A
/// **justified** system has `a > 1`: the horizontal slack (content width minus
/// natural ink width) is spread linearly across the line so its ink fills the
/// content width. The map is applied SLOT-RELATIVELY to glyphs — each slot's
/// members translate by the map evaluated at the slot's source, so intra-slot
/// offsets (a time signature after its barline, an accidental left of its
/// notehead) survive verbatim — directly to spanning-stroke and curve
/// endpoints, and via the owning slot for a rigid-width stroke (a stem or ledger
/// that must stay attached to its notehead, not stretch).
#[derive(Copy, Clone)]
struct Placement {
    a: f32,
    b: f32,
    dy: f32,
    /// The system's slot-source range `[x0, x1]`. The affine stretch acts only
    /// WITHIN it; beyond it (a glyph's bearing overhang, a staff line drawn to
    /// the ink edge) the map is rigid slope-1, so the mapped ink extremes agree
    /// exactly with the per-slot deltas at the first/last slots.
    x0: f32,
    x1: f32,
}

impl Placement {
    /// A pure translation (an unjustified system, or the identity fallback for
    /// content no system claims). `a = 1`, so the clamp range is irrelevant.
    fn rigid(dx: f32, dy: f32) -> Self {
        Placement {
            a: 1.0,
            b: dx,
            dy,
            x0: 0.0,
            x1: 0.0,
        }
    }
    /// The world x of a spaced x: affine within the slot-source range, rigid
    /// (slope 1) beyond it.
    fn x(&self, x: f32) -> f32 {
        let c = x.clamp(self.x0, self.x1);
        self.a * c + self.b + (x - c)
    }
    /// The rigid delta every glyph in a slot whose source is `slot_x`
    /// translates by — constant per slot, so intra-slot offsets are preserved.
    /// Slot sources lie in `[x0, x1]`, so no clamp is needed.
    fn slot_dx(&self, slot_x: f32) -> f32 {
        (self.a - 1.0) * slot_x + self.b
    }
    /// The same placement sunk downward by `shift` — the inter-staff solve
    /// pushes a staff's content down within its system (y-down is decreasing y).
    fn sunk(&self, shift: f32) -> Self {
        Placement {
            dy: self.dy - shift,
            ..*self
        }
    }
}

/// The content extent of a system in spaced (pre-casting) coordinates.
#[derive(Copy, Clone)]
struct Extent {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    any: bool,
}

impl Extent {
    fn empty() -> Self {
        Extent {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
            any: false,
        }
    }

    /// Extend only the vertical extent (the inter-staff solve grows a system's
    /// height by shifting staves apart, without touching its x-span).
    fn add_y(&mut self, y0: f32, y1: f32) {
        if y0.is_finite() && y1.is_finite() {
            self.min_y = self.min_y.min(y0.min(y1));
            self.max_y = self.max_y.max(y0.max(y1));
            self.any = true;
        }
    }

    /// Extend only the horizontal extent. Staff-attributed content contributes
    /// its y through the inter-staff solve (SHIFTED), never here.
    fn add_x(&mut self, x0: f32, x1: f32) {
        if x0.is_finite() && x1.is_finite() {
            self.min_x = self.min_x.min(x0.min(x1));
            self.max_x = self.max_x.max(x0.max(x1));
            self.any = true;
        }
    }

    /// Normalized: a content-less system is a zero box at the origin.
    fn normalized(self) -> Self {
        if self.any {
            self
        } else {
            Extent {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
                any: false,
            }
        }
    }
}

/// The MUSCLOID target of an engraved break decision: synthesized from the
/// owning region's source under [`SynthesisKind::EngravedBreak`], keyed by the
/// breaking slot's identity (the slot id is itself content-derived from the
/// region and its column, so the key is the column's semantic identity, never a
/// layout-position ordinal).
fn break_target(region_source: TypedObjectId, slot: SpringSlotId) -> LayoutObjectId {
    synthesized_layout_id(
        &region_source,
        SynthesisKind::EngravedBreak,
        SynthesisInstanceKey(slot.0),
    )
}

/// The decision source for a break honoured at `slot`: the user override that
/// asked for it when the projection recorded one, else `Automatic`.
fn origin_source(
    origins: &BTreeMap<(u128, bool), EngravingOverrideId>,
    slot: SpringSlotId,
    page: bool,
) -> DecisionSource {
    match origins.get(&(slot.0, page)) {
        Some(id) => DecisionSource::UserOverride(*id),
        None => DecisionSource::Automatic,
    }
}

/// Casts the spaced layout off into systems and pages. Pure and deterministic:
/// a function of the input IR, the spaced geometry, and the page geometry.
pub(crate) fn cast_off(
    input: &ConstrainedLayoutIR,
    spaced_glyphs: &[ResolvedGlyph],
    spaced_strokes: &[Stroke],
    spaced_curves: &[Curve],
    geometry: &PageGeometry,
) -> CastLayout {
    // ---- Slot table (spaced coordinates) --------------------------------
    let mut slots: BTreeMap<SpringSlotId, SlotInfo> = BTreeMap::new();
    for (i, (glyph, spaced)) in input.glyphs.iter().zip(spaced_glyphs).enumerate() {
        let name = glyph.glyph.as_str();
        let x = spaced.position.x.0;
        let lo = x + glyph.bounding_box.left.0;
        let hi = x + glyph.bounding_box.right.0;
        let entry = slots.entry(glyph.horizontal_slot).or_insert(SlotInfo {
            id: glyph.horizontal_slot,
            x,
            lo,
            hi,
            members: Vec::new(),
            barline: false,
            final_barline: false,
            note: false,
            measure_barline: None,
        });
        entry.lo = entry.lo.min(lo);
        entry.hi = entry.hi.max(hi);
        entry.members.push(i);
        // Barline classification by the engraver's own name vocabulary (which
        // includes the composite repeat signs a repeat boundary morphs a
        // measure barline into) — but only for a **directly-manifested measure
        // barline**: the casting contract breaks systems at measure
        // boundaries, so a repeat-synthesized standalone sign (a mid-measure
        // boundary, a region edge without a final barline) must not become a
        // phantom break candidate that could tear off a degenerate lone-sign
        // trailing system or split a measure.
        if is_barline_glyph(name)
            && glyph.provenance.synthesis.is_none()
            && matches!(glyph.provenance.source, TypedObjectId::Measure(_))
        {
            entry.barline = true;
            if name == "barlineFinal" {
                entry.final_barline = true;
            } else if entry.measure_barline.is_none() {
                entry.measure_barline = Some(i);
            }
        }
        if name.starts_with("notehead") || name.starts_with("rest") {
            entry.note = true;
        }
    }
    // Fold each rigid stroke (a ledger line) into its owning slot's extent, so
    // an overhanging ledger widens the measure it belongs to (mirrors the
    // spacing pass's extent rule).
    for (stroke, spaced) in input.strokes.iter().zip(spaced_strokes) {
        if !is_rigid_width_stroke(stroke) {
            continue;
        }
        if let Some(glyph) = owning_glyph(stroke, &input.glyphs) {
            if let Some(entry) = slots.get_mut(&glyph.horizontal_slot) {
                entry.lo = entry.lo.min(spaced.from.x.0.min(spaced.to.x.0));
                entry.hi = entry.hi.max(spaced.from.x.0.max(spaced.to.x.0));
            }
        }
    }

    // ---- Region partition ------------------------------------------------
    let mut region_of_glyph: BTreeMap<GlyphObjectId, usize> = BTreeMap::new();
    for (r, region) in input.regions.iter().enumerate() {
        for id in &region.glyphs {
            region_of_glyph.entry(*id).or_insert(r);
        }
    }
    let mut region_slots: Vec<Vec<SlotInfo>> =
        (0..input.regions.len()).map(|_| Vec::new()).collect();
    for (_, info) in slots {
        let region = info
            .members
            .first()
            .and_then(|&i| region_of_glyph.get(&input.glyphs[i].id()))
            .copied();
        // A slot no region claims (out-of-pipeline input) is left out: its
        // glyphs stay in the spaced frame, on no page.
        if let Some(r) = region {
            region_slots[r].push(info);
        }
    }
    for infos in &mut region_slots {
        infos.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.id.cmp(&b.id)));
    }
    // Each slot's spaced reference x, for the slot-relative justification delta.
    let slot_source_x: BTreeMap<SpringSlotId, f32> = region_slots
        .iter()
        .flatten()
        .map(|info| (info.id, info.x))
        .collect();

    // ---- Break requirements ----------------------------------------------
    let mut reqs: BTreeMap<SpringSlotId, Vec<BreakReq>> = BTreeMap::new();
    for constraint in &input.constraints {
        let (slot, page, kind) = match constraint {
            LayoutConstraint::SystemBreakAt { slot, kind } => (*slot, false, *kind),
            LayoutConstraint::PageBreakAt { slot, kind } => (*slot, true, *kind),
            _ => continue,
        };
        reqs.entry(slot).or_default().push(BreakReq {
            page,
            hard: kind == BreakKind::Hard,
        });
    }
    let mut origins: BTreeMap<(u128, bool), EngravingOverrideId> = BTreeMap::new();
    for origin in &input.break_origins {
        origins
            .entry((origin.slot.0, origin.class == BreakClass::Page))
            .or_insert(origin.override_id);
    }

    // ---- System breaking (greedy first-fit per region) --------------------
    let width_limit = {
        let w = geometry.content_width();
        if w > 0.0 {
            w
        } else {
            f32::INFINITY
        }
    };
    let mut systems: Vec<SystemPlan> = Vec::new();
    let mut skipped: Vec<EngravingDecision> = Vec::new();
    for (r, infos) in region_slots.iter().enumerate() {
        let region_source = input.regions[r].provenance.source;
        walk_region(
            r,
            infos,
            &reqs,
            &origins,
            region_source,
            width_limit,
            &mut systems,
            &mut skipped,
        );
    }

    // (The old greedy pass needed a second widow-rebalance phase here; the
    // optimal break search evens the final system directly — see
    // `optimal_breaks`.)

    // ---- Stroke fates ------------------------------------------------------
    // Which system each slot landed in, and each region's slot span / per-system
    // clip intervals (the interior cut points for system-spanning strokes).
    let mut system_of_slot: BTreeMap<SpringSlotId, usize> = BTreeMap::new();
    for (s, plan) in systems.iter().enumerate() {
        for &i in &plan.slots {
            system_of_slot.insert(region_slots[plan.region][i].id, s);
        }
    }
    let region_spans: Vec<Option<(f32, f32)>> = region_slots
        .iter()
        .map(|infos| {
            infos
                .iter()
                .map(|s| (s.lo, s.hi))
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        })
        .collect();
    let mut region_systems: Vec<Vec<usize>> = vec![Vec::new(); input.regions.len()];
    for (s, plan) in systems.iter().enumerate() {
        region_systems[plan.region].push(s);
    }
    let mut clips: Vec<(f32, f32)> = vec![(f32::NEG_INFINITY, f32::INFINITY); systems.len()];
    for (r, sys_of_region) in region_systems.iter().enumerate() {
        let last = sys_of_region.len().saturating_sub(1);
        for (local, &s) in sys_of_region.iter().enumerate() {
            let lo = if local == 0 {
                f32::NEG_INFINITY
            } else {
                systems[s]
                    .slots
                    .iter()
                    .map(|&i| region_slots[r][i].lo)
                    .fold(f32::INFINITY, f32::min)
            };
            let hi = if local == last {
                f32::INFINITY
            } else {
                systems[s]
                    .slots
                    .iter()
                    .map(|&i| region_slots[r][i].hi)
                    .fold(f32::NEG_INFINITY, f32::max)
            };
            clips[s] = (lo, hi);
        }
    }
    let fates: Vec<StrokeFate> = input
        .strokes
        .iter()
        .zip(spaced_strokes)
        .map(|(stroke, spaced)| {
            stroke_fate(
                stroke,
                spaced,
                input,
                &system_of_slot,
                &region_spans,
                &region_systems,
                &clips,
            )
        })
        .collect();
    // A curve rides one system whole when it fits within one, or splits into
    // per-system sub-cubics (de Casteljau) when it spans a break — the same
    // nearest-region / clip-overlap logic strokes use.
    let curve_fates: Vec<CurveFate> = spaced_curves
        .iter()
        .map(|curve| curve_fate(curve, &region_spans, &region_systems, &clips))
        .collect();

    // ---- Inter-staff vertical solve + system extents -----------------------
    // Attribute every primitive to its owning staff so the gaps BETWEEN a
    // system's staves can be renegotiated: the constrained stage stacks staves
    // at a fixed pitch, so tightly ledgered or slurred adjacent staves collide.
    //
    // Attribution is a BAND LOOKUP, not a geometric guess. Every primitive —
    // glyph, stroke, curve — declares the vertical band it belongs to, and the
    // projection that emitted it knew the answer: a stem's band is its note's, a
    // slur's is its notes'. Content owned by no staff (a page-margin annotation,
    // a repeat structure spanning several staves) names a non-`Staff` band and
    // is attributed to `None` — it takes no staff shift.
    //
    // Inferring the owner from proximity instead is a trap this code fell into
    // twice. A stem sits under its notehead but shares x columns with the staff
    // above; a slur's endpoints are lifted clear of its own staff by design, so
    // the nearest notehead is routinely on the ADJACENT staff. Neither is
    // recoverable from geometry, and both silently tore primitives off their
    // notes. See DECISIONS.md, "Why attribution is declared, not inferred".
    let band_to_staff: BTreeMap<VerticalBandId, StaffId> = input
        .vertical_bands
        .iter()
        .filter_map(|b| match b.kind {
            VerticalBandKind::Staff(s) => Some((b.id, s)),
            _ => None,
        })
        .collect();
    let staff_of = |band: VerticalBandId| band_to_staff.get(&band).copied();
    let glyph_staff_of: Vec<Option<StaffId>> = input
        .glyphs
        .iter()
        .map(|g| staff_of(g.vertical_band))
        .collect();
    let stroke_staff_of: Vec<Option<StaffId>> = input
        .strokes
        .iter()
        .map(|s| staff_of(s.vertical_band))
        .collect();
    let curve_staff_of: Vec<Option<StaffId>> = input
        .curves
        .iter()
        .map(|c| staff_of(c.vertical_band))
        .collect();

    // Pass A: system extents (unshifted), and per (system, staff) content
    // y-extents plus the staff-line reference y (for ordering).
    let mut extents: Vec<Extent> = vec![Extent::empty(); systems.len()];
    let mut staff_ext: BTreeMap<(usize, StaffId), (f32, f32)> = BTreeMap::new();
    let mut staff_ref: BTreeMap<(usize, StaffId), f32> = BTreeMap::new();
    let into_staff = |m: &mut BTreeMap<(usize, StaffId), (f32, f32)>,
                      s: usize,
                      staff: Option<StaffId>,
                      lo_y: f32,
                      hi_y: f32| {
        if let Some(st) = staff {
            m.entry((s, st))
                .and_modify(|e| {
                    e.0 = e.0.min(lo_y);
                    e.1 = e.1.max(hi_y);
                })
                .or_insert((lo_y, hi_y));
        }
    };
    for (s, plan) in systems.iter().enumerate() {
        for &i in &plan.slots {
            for &g in &region_slots[plan.region][i].members {
                let glyph = &spaced_glyphs[g];
                let (x, y) = (glyph.position.x.0, glyph.position.y.0);
                let (lo_y, hi_y) = (
                    y + glyph.bounding_box.bottom.0,
                    y + glyph.bounding_box.top.0,
                );
                extents[s].add_x(
                    x + glyph.bounding_box.left.0,
                    x + glyph.bounding_box.right.0,
                );
                match glyph_staff_of[g] {
                    Some(_) => into_staff(&mut staff_ext, s, glyph_staff_of[g], lo_y, hi_y),
                    None => extents[s].add_y(lo_y, hi_y),
                }
            }
        }
    }
    for (si, (fate, spaced)) in fates.iter().zip(spaced_strokes).enumerate() {
        let half = (spaced.thickness.0 * 0.5).max(0.0);
        let staff = stroke_staff_of[si];
        let is_staff_line = matches!(spaced.provenance.source, TypedObjectId::Staff(_));
        let segs: Vec<(usize, Point, Point)> = match fate {
            StrokeFate::Rigid(Some(s)) => vec![(*s, spaced.from, spaced.to)],
            StrokeFate::Rigid(None) => vec![],
            StrokeFate::Split(segments) => segments.clone(),
        };
        for (s, from, to) in segs {
            let (lo_y, hi_y) = (from.y.0.min(to.y.0) - half, from.y.0.max(to.y.0) + half);
            extents[s].add_x(from.x.0 - half, to.x.0 + half);
            match staff {
                Some(_) => into_staff(&mut staff_ext, s, staff, lo_y, hi_y),
                None => extents[s].add_y(lo_y, hi_y),
            }
            if is_staff_line {
                if let Some(st) = staff {
                    staff_ref
                        .entry((s, st))
                        .and_modify(|r| *r = r.max(hi_y))
                        .or_insert(hi_y);
                }
            }
        }
    }
    for (ci, (fate, curve)) in curve_fates.iter().zip(spaced_curves).enumerate() {
        let half = (curve.thickness.0 * 0.5).max(0.0);
        let staff = curve_staff_of[ci];
        let segs: Vec<(usize, [Point; 4])> = match fate {
            CurveFate::Rigid(Some(s)) => vec![(*s, curve.control_points())],
            CurveFate::Rigid(None) => vec![],
            CurveFate::Split(segments) => segments.clone(),
        };
        for (s, cp) in segs {
            for p in cp {
                extents[s].add_x(p.x.0 - half, p.x.0 + half);
                match staff {
                    Some(_) => into_staff(&mut staff_ext, s, staff, p.y.0 - half, p.y.0 + half),
                    None => extents[s].add_y(p.y.0 - half, p.y.0 + half),
                }
            }
        }
    }

    // Solve each system's inter-staff gaps: order the staves top-to-bottom by
    // their reference y (staff line, else content mid), keep that order fixed,
    // and shift each staff so its INK CLEARANCE to the one above realizes the
    // gap band's declared height. `staff_shift[(system, staff)]` is the downward
    // shift (subtracted from y); the top staff's is 0.
    //
    // The renegotiation is TWO-SIDED. A pair whose content collides is pushed
    // apart; a pair the constrained stage left slack is pulled together. The
    // fixed `SYSTEM_STAFF_PITCH` that stage stacks by is therefore an initial
    // arrangement, not a floor: the band model is the height model, and the solve
    // realizes it. (Expanding only was the earlier behaviour, and it was
    // measurably wrong — `vertical_density_penalty` scored honest sprawl on every
    // relaxed multi-staff system, because a gap wider than preferred is sprawl
    // exactly as a narrower one is crowding.)
    //
    // The target is the gap band's `preferred_height`, held at or above its
    // `min_height` — the hardest squeeze permitted. Validation already brackets
    // preferred by min and max, so the clamp is belt-and-braces rather than a
    // second policy. The band is the one the REGION DECLARED, not the
    // constructor's default, so the solve and `vertical_density_penalty` — which
    // scores the realized clearance against that same band — read one number.
    //
    // Gap `g` separates the region's staves `g-1` and `g` (see `to_constrained`).
    // Every staff of a region carries content in every system of that region —
    // its staff lines are per-staff strokes, split into each system — so the
    // staves present here are the region's full staff order and the window index
    // is the gap index. A band that somehow does not exist falls back to the
    // constructor's default rather than silently skipping the pair.
    let fallback = VerticalBand::inter_staff_gap(VerticalBandId(0));
    let mut staff_shift: BTreeMap<(usize, StaffId), f32> = BTreeMap::new();
    for (s, plan) in systems.iter().enumerate() {
        let region_layout_id = input.regions[plan.region].provenance.stable_id;
        let target_gap = |gap_index: usize| -> f32 {
            let id = inter_staff_gap_id(region_layout_id, gap_index);
            let band = input
                .vertical_bands
                .iter()
                .find(|band| band.id == id)
                .unwrap_or(&fallback);
            band.preferred_height.0.max(band.min_height.0)
        };
        let mut staves: Vec<(StaffId, (f32, f32))> = staff_ext
            .iter()
            .filter(|((sys, _), _)| *sys == s)
            .map(|((_, st), ext)| (*st, *ext))
            .collect();
        // Top first: larger reference y is higher on the page.
        staves.sort_by(|a, b| {
            let key = |st: StaffId, ext: (f32, f32)| {
                staff_ref
                    .get(&(s, st))
                    .copied()
                    .unwrap_or((ext.0 + ext.1) * 0.5)
            };
            key(b.0, b.1).total_cmp(&key(a.0, a.1)).then(a.0.cmp(&b.0))
        });
        let mut shift = 0.0_f32;
        for (g, w) in staves.windows(2).enumerate() {
            let (upper, (upper_lo, _)) = w[0];
            let (lower, (_, lower_hi)) = w[1];
            staff_shift.insert((s, upper), shift);
            // Both staves move, so solve the recurrence rather than guessing it.
            // With `shift` the upper staff's cumulative shift, the realized
            // clearance is `(upper_lo - shift_upper) - (lower_hi - shift_lower)`,
            // and setting that equal to the target gives
            //
            //     shift_lower = shift_upper + target - (upper_lo - lower_hi)
            //
            // — the UNSHIFTED gap. Subtracting `shift_upper` from the gap here
            // and adding it back through `shift +=` would count it twice, which
            // over-separated every pair below the first by exactly the shift
            // above it (invisible on two staves, where that shift is 0). The
            // correction is signed: positive opens a crowded pair, negative
            // closes a slack one, and it accumulates down the stack.
            let gap = upper_lo - lower_hi;
            shift += target_gap(g + 1) - gap;
            staff_shift.insert((s, lower), shift);
        }
        if staves.len() == 1 {
            staff_shift.insert((s, staves[0].0), 0.0);
        }
    }

    // Fold each staff's SHIFTED content y-extent into its system extent, so the
    // stacking/justification below sees the taller, separated system.
    for ((s, st), (lo, hi)) in &staff_ext {
        let sh = staff_shift.get(&(*s, *st)).copied().unwrap_or(0.0);
        extents[*s].add_y(lo - sh, hi - sh);
    }
    let extents: Vec<Extent> = extents.into_iter().map(Extent::normalized).collect();

    // ---- Vertical stacking and page assignment ----------------------------
    // The inter-system spacing comes from the vertical-band model's own
    // constructor, so the casting-off gap and the band spring cannot drift.
    let gap = VerticalBand::inter_system_gap(VerticalBandId(0))
        .preferred_height
        .0;
    let content_height = geometry.content_height();
    let bounded = content_height > 0.0;
    let mut placements: Vec<Placement> = Vec::with_capacity(systems.len());
    let mut page_systems: Vec<Vec<usize>> = Vec::new();
    let mut cursor = 0.0_f32;
    let mut page_floor = 0.0_f32;
    for (s, plan) in systems.iter().enumerate() {
        let ext = &extents[s];
        let height = ext.max_y - ext.min_y;
        // Every opened page immediately receives a system, so an overflow test
        // against a non-empty page list never opens an empty page — a system
        // taller than a whole page stays (overfull) on the page it opens.
        let overflow = bounded && !page_systems.is_empty() && cursor - height < page_floor;
        if page_systems.is_empty() || plan.page_forced || overflow {
            let p = page_systems.len();
            cursor = page_top_content(p, geometry);
            page_floor = cursor - content_height.max(0.0);
            page_systems.push(Vec::new());
        }
        let base_dx = geometry.margins.left.0 - ext.min_x;
        let dy = cursor - ext.max_y;
        placements.push(justify_system(
            plan,
            ext,
            base_dx,
            dy,
            &region_slots,
            &region_systems,
            width_limit,
        ));
        page_systems
            .last_mut()
            .expect("a page was opened above")
            .push(s);
        cursor -= height + gap;
    }

    // ---- Vertical justification -------------------------------------------
    // Spread the systems of every NON-FINAL page so the last system's bottom
    // reaches the content bottom, filling the page height — the vertical analog
    // of per-system horizontal justification, distributing the slack evenly
    // across the inter-system gaps. The last page stays ragged-bottom
    // (top-aligned), as engraving convention wants; a page with a single system
    // has no gap to grow, and an already-full (or overfull) page is left alone.
    if bounded {
        let last_page = page_systems.len().saturating_sub(1);
        for (p, page) in page_systems.iter().enumerate() {
            if p == last_page || page.len() < 2 {
                continue;
            }
            let content_bottom = page_top_content(p, geometry) - content_height;
            let last = *page.last().expect("a page carries at least one system");
            let natural_bottom = placements[last].dy + extents[last].min_y;
            let slack = natural_bottom - content_bottom;
            if slack <= 0.0 {
                continue;
            }
            // System i (0-based on the page) sinks by i/(n-1) of the slack, so
            // the first stays at the content top and the last lands on the
            // content bottom (y-down is decreasing y in this world frame).
            let step = slack / (page.len() - 1) as f32;
            for (i, &s) in page.iter().enumerate() {
                placements[s].dy -= i as f32 * step;
            }
        }
    }

    // ---- Break structure and decisions -------------------------------------
    let mut system_start_slots = BTreeSet::new();
    for plan in &systems {
        if let Some(&i) = plan.slots.first() {
            system_start_slots.insert(region_slots[plan.region][i].id);
        }
    }
    let mut page_start_slots = BTreeSet::new();
    let mut decisions = Vec::new();
    for (p, on_page) in page_systems.iter().enumerate() {
        for (j, &s) in on_page.iter().enumerate() {
            let plan = &systems[s];
            let starts_page = j == 0;
            if starts_page {
                if let Some(&i) = plan.slots.first() {
                    page_start_slots.insert(region_slots[plan.region][i].id);
                }
            }
            let region_source = input.regions[plan.region].provenance.source;
            if let Some(boundary) = plan.boundary {
                // A chosen intra-region break: a page decision when the system
                // actually opens a page, a system decision otherwise.
                decisions.push(EngravingDecision::with_source(
                    break_target(region_source, boundary.slot),
                    if starts_page {
                        EngravingDecisionKind::PageBreak
                    } else {
                        EngravingDecisionKind::SystemBreak
                    },
                    boundary.source,
                ));
            } else if starts_page && p > 0 {
                // A later page opening at a region's first system: the page
                // start is itself an engraved decision (forced or overflow).
                if let Some(&i) = plan.slots.first() {
                    decisions.push(EngravingDecision::with_source(
                        break_target(region_source, region_slots[plan.region][i].id),
                        EngravingDecisionKind::PageBreak,
                        plan.page_source,
                    ));
                }
            }
        }
    }
    decisions.extend(skipped);

    // ---- Bake the world frame ----------------------------------------------
    // A primitive's additional downward shift from the inter-staff solve.
    let staff_dy = |s: usize, staff: Option<StaffId>| -> f32 {
        staff
            .and_then(|st| staff_shift.get(&(s, st)))
            .copied()
            .unwrap_or(0.0)
    };
    let glyphs: Vec<ResolvedGlyph> = spaced_glyphs
        .iter()
        .zip(&input.glyphs)
        .enumerate()
        .map(|(gi, (spaced, glyph))| {
            let (dx, dy) = match system_of_slot.get(&glyph.horizontal_slot) {
                Some(&s) => {
                    // Slot-relative: every member of a slot translates by the
                    // map at the slot's source, so intra-slot offsets survive.
                    let sx = slot_source_x
                        .get(&glyph.horizontal_slot)
                        .copied()
                        .unwrap_or(spaced.position.x.0);
                    (
                        placements[s].slot_dx(sx),
                        placements[s].dy - staff_dy(s, glyph_staff_of[gi]),
                    )
                }
                None => (0.0, 0.0),
            };
            ResolvedGlyph {
                position: Point::new(spaced.position.x.0 + dx, spaced.position.y.0 + dy),
                ..spaced.clone()
            }
        })
        .collect();

    // Per-system staff-line marks, for the resolved staff records below.
    let mut staff_marks: BTreeMap<(usize, StaffId), StaffAgg> = BTreeMap::new();
    let mut strokes: Vec<Stroke> = Vec::with_capacity(spaced_strokes.len());
    let mut continuations: Vec<Stroke> = Vec::new();
    // The system each baked stroke landed in, parallel to `strokes` (a quality
    // metric measures a system's realized per-staff content extents, and a
    // stroke carries no spring slot to look one up with).
    let mut stroke_system: Vec<Option<usize>> = Vec::with_capacity(spaced_strokes.len());
    let mut continuation_system: Vec<Option<usize>> = Vec::new();
    for (si, ((source, spaced), fate)) in input
        .strokes
        .iter()
        .zip(spaced_strokes)
        .zip(&fates)
        .enumerate()
    {
        match fate {
            StrokeFate::Rigid(sys) => {
                let stroke = match sys {
                    Some(s) => place_stroke(
                        source,
                        spaced,
                        placements[*s].sunk(staff_dy(*s, stroke_staff_of[si])),
                        &slot_source_x,
                        &input.glyphs,
                    ),
                    None => spaced.clone(),
                };
                if let (Some(s), TypedObjectId::Staff(staff)) = (sys, spaced.provenance.source) {
                    mark_staff(&mut staff_marks, *s, staff, &stroke);
                }
                strokes.push(stroke);
                stroke_system.push(*sys);
            }
            StrokeFate::Split(segments) => {
                for (k, (s, from, to)) in segments.iter().enumerate() {
                    // A split stroke spans systems — a staff line or volta
                    // bracket — so each segment stretches with its system.
                    let p = placements[*s].sunk(staff_dy(*s, stroke_staff_of[si]));
                    let provenance = if k == 0 {
                        // The first segment carries the original stroke's exact
                        // provenance: the object survives, re-shaped.
                        spaced.provenance.clone()
                    } else {
                        Provenance::synthesized(
                            spaced.provenance.source,
                            SynthesisKind::Registered(SYSTEM_CONTINUATION_SYNTHESIS),
                            continuation_instance_key(spaced.provenance.stable_id, k as u32),
                            spaced.provenance.dependencies.clone(),
                        )
                    };
                    let stroke = Stroke {
                        provenance,
                        from: Point::new(p.x(from.x.0), from.y.0 + p.dy),
                        to: Point::new(p.x(to.x.0), to.y.0 + p.dy),
                        thickness: spaced.thickness,
                        layer: spaced.layer,
                        style: spaced.style,
                        vertical_band: spaced.vertical_band,
                    };
                    if let TypedObjectId::Staff(staff) = spaced.provenance.source {
                        mark_staff(&mut staff_marks, *s, staff, &stroke);
                    }
                    if k == 0 {
                        strokes.push(stroke);
                        stroke_system.push(Some(*s));
                    } else {
                        continuations.push(stroke);
                        continuation_system.push(Some(*s));
                    }
                }
            }
        }
    }
    strokes.extend(continuations);
    stroke_system.extend(continuation_system);

    // Curves: a curve that fits in one system is translated whole by that
    // system's placement (or left in the spaced frame if no region claimed it).
    // A curve that spans a system break is split into per-system sub-cubics: the
    // first segment carries the slur's exact provenance (the object survives,
    // re-shaped — the round-trip source surjection recovers it), later segments
    // are synthesized continuations under `SYSTEM_CONTINUATION_SYNTHESIS`, as a
    // split stroke's are.
    let mut curves: Vec<Curve> = Vec::with_capacity(spaced_curves.len());
    let mut curve_continuations: Vec<Curve> = Vec::new();
    let mut curve_system: Vec<Option<usize>> = Vec::with_capacity(spaced_curves.len());
    let mut curve_continuation_system: Vec<Option<usize>> = Vec::new();
    for (ci, (curve, fate)) in spaced_curves.iter().zip(&curve_fates).enumerate() {
        let curve_staff = curve_staff_of[ci];
        // A slur has no intra-slot structure, so its control points map straight
        // through the affine: the endpoints follow their anchor notes (which sit
        // at slot sources) and the arc stretches horizontally with the span.
        let shift =
            |cp: [Point; 4], p: Placement| cp.map(|pt| Point::new(p.x(pt.x.0), pt.y.0 + p.dy));
        match fate {
            CurveFate::Rigid(system) => {
                let p = system
                    .map(|s| placements[s].sunk(staff_dy(s, curve_staff)))
                    .unwrap_or(Placement::rigid(0.0, 0.0));
                let [p0, p1, p2, p3] = shift(curve.control_points(), p);
                curves.push(Curve {
                    p0,
                    p1,
                    p2,
                    p3,
                    ..curve.clone()
                });
                curve_system.push(*system);
            }
            CurveFate::Split(segments) => {
                for (k, (s, cp)) in segments.iter().enumerate() {
                    let [p0, p1, p2, p3] =
                        shift(*cp, placements[*s].sunk(staff_dy(*s, curve_staff)));
                    let provenance = if k == 0 {
                        curve.provenance.clone()
                    } else {
                        Provenance::synthesized(
                            curve.provenance.source,
                            SynthesisKind::Registered(SYSTEM_CONTINUATION_SYNTHESIS),
                            continuation_instance_key(curve.provenance.stable_id, k as u32),
                            curve.provenance.dependencies.clone(),
                        )
                    };
                    let segment = Curve {
                        provenance,
                        p0,
                        p1,
                        p2,
                        p3,
                        thickness: curve.thickness,
                        layer: curve.layer,
                        style: curve.style,
                        vertical_band: curve.vertical_band,
                        line: curve.line,
                    };
                    if k == 0 {
                        curves.push(segment);
                        curve_system.push(Some(*s));
                    } else {
                        curve_continuations.push(segment);
                        curve_continuation_system.push(Some(*s));
                    }
                }
            }
        }
    }
    curves.extend(curve_continuations);
    curve_system.extend(curve_continuation_system);

    // ---- The resolved page tree ---------------------------------------------
    let resolved_systems: Vec<ResolvedSystem> = systems
        .iter()
        .enumerate()
        .map(|(s, plan)| {
            build_system(
                s,
                plan,
                input,
                &region_slots,
                &extents,
                &placements,
                &staff_marks,
            )
        })
        .collect();
    let mut resolved_systems: Vec<Option<ResolvedSystem>> =
        resolved_systems.into_iter().map(Some).collect();
    let pages: Vec<ResolvedPage> = page_systems
        .iter()
        .enumerate()
        .map(|(p, on_page)| {
            let first_region = systems[on_page[0]].region;
            let region_provenance = &input.regions[first_region].provenance;
            let provenance = if p == 0 {
                // Page 1 carries the first region's own provenance, as the
                // degenerate single-page output always did.
                input.regions[0].provenance.clone()
            } else {
                Provenance::synthesized(
                    region_provenance.source,
                    SynthesisKind::EngravedBreak,
                    SynthesisInstanceKey((KEY_NS_PAGE << 64) | (p as u128 + 1)),
                    region_provenance.dependencies.clone(),
                )
            };
            ResolvedPage {
                provenance,
                number: p as u32 + 1,
                size: geometry.size,
                margins: geometry.margins,
                systems: on_page
                    .iter()
                    .map(|&s| resolved_systems[s].take().expect("each system on one page"))
                    .collect(),
                // Nothing in the Minimal pipeline is a page-level free object
                // (region content is all system-bound); left empty rather than
                // fabricated.
                free_objects: Vec::new(),
            }
        })
        .collect();

    CastLayout {
        glyphs,
        strokes,
        curves,
        pages,
        decisions,
        system_start_slots,
        page_start_slots,
        system_of_slot,
        stroke_system,
        curve_system,
        region_of_system: systems.iter().map(|plan| plan.region).collect(),
    }
}

/// The world-frame y of page `p`'s content top: pages stack downward from the
/// origin, each a full page height plus [`INTER_PAGE_GAP`] below the previous.
fn page_top_content(p: usize, geometry: &PageGeometry) -> f32 {
    -(p as f32) * (geometry.size.height.0 + INTER_PAGE_GAP) - geometry.margins.top.0
}

/// Optimal automatic system breaks for one region: a badness-minimizing
/// (Knuth–Plass-style) partition of the region's measures into systems,
/// replacing greedy first-fit. Returns the slot ids at which an AUTOMATIC break
/// opens a system — the break REQUIREMENTS (hard / soft / page, which bound the
/// DP's segments) are honoured by [`walk_region`] itself, and never appear here.
///
/// **Objective.** Minimize the sum over ALL systems of the squared normalized
/// underfill `((width_limit − w) / width_limit)²`. Squaring evens the systems
/// (a lopsided split costs more than a balanced one), and including the *final*
/// system in the sum is what subsumes the old tail-only widow rebalance — the
/// optimizer will not leave a narrow final stub if a more even partition is
/// cheaper. It is the additive, DP-tractable analog of the catalog's
/// break/imbalance distribution cost (`distribution_cost`, now retired): both
/// reward filled, even systems. A system may not exceed the content width unless
/// it is a **single unsplittable measure** (an overfull lone measure, which the
/// greedy pass also emitted). `Minimal` still makes no optimality *claim*; this
/// is a deterministic global heuristic, an honest improvement on first-fit.
///
/// **Determinism.** A pure function of the slot extents and requirements; the
/// DP minimizes the lexicographic `(cost, system_count)` (fewer systems breaks
/// ties, so ties favour fewer pages), and among equal `(cost, count)` the
/// earliest-considered predecessor (the largest final system) wins.
fn optimal_breaks(
    slots: &[SlotInfo],
    reqs: &BTreeMap<SpringSlotId, Vec<BreakReq>>,
    width_limit: f32,
) -> BTreeSet<SpringSlotId> {
    let mut automatic = BTreeSet::new();
    if !width_limit.is_finite() || width_limit <= 0.0 || slots.is_empty() {
        return automatic; // unbounded width: nothing wraps
    }
    let breakable = |slot: &SlotInfo| slot.barline && !slot.final_barline;
    // Measure-boundary positions in slot-index space: region start, each
    // breakable barline, region end. `forced[k]` marks a boundary carrying a
    // break requirement (the DP may not span it). The region end is a boundary.
    let mut pts: Vec<usize> = vec![0];
    let mut forced: Vec<bool> = vec![false];
    for (i, slot) in slots.iter().enumerate() {
        if i > 0 && breakable(slot) {
            pts.push(i);
            forced.push(reqs.contains_key(&slot.id));
        }
    }
    pts.push(slots.len());
    forced.push(true);
    let n = pts.len(); // n - 1 measures between the n boundaries

    // A system spanning boundaries [a, b): its ink extent over slots
    // `[pts[a] .. pts[b])`.
    let width = |a: usize, b: usize| -> f32 {
        let range = &slots[pts[a]..pts[b]];
        let lo = range.iter().map(|s| s.lo).fold(f32::INFINITY, f32::min);
        let hi = range.iter().map(|s| s.hi).fold(f32::NEG_INFINITY, f32::max);
        (hi - lo).max(0.0)
    };

    // dp[b] = the min `(cost, system_count)` to partition measures [0, b).
    let mut dp: Vec<(f64, usize)> = vec![(f64::INFINITY, usize::MAX); n];
    let mut from: Vec<usize> = vec![0; n];
    dp[0] = (0.0, 0);
    for b in 1..n {
        for a in 0..b {
            // A system may not skip a forced break at an interior boundary.
            if (a + 1..b).any(|k| forced[k]) {
                continue;
            }
            let (prev_cost, prev_count) = dp[a];
            if !prev_cost.is_finite() {
                continue;
            }
            let w = width(a, b);
            let bad = if w <= width_limit {
                let u = f64::from((width_limit - w) / width_limit);
                u * u
            } else if b - a == 1 {
                0.0 // a lone measure wider than the page: unavoidable, not charged
            } else {
                continue; // overfull and splittable: not a valid system
            };
            let cand = (prev_cost + bad, prev_count + 1);
            if cand < dp[b] {
                dp[b] = cand;
                from[b] = a;
            }
        }
    }

    // Reconstruct the partition; its non-forced boundaries are the automatic
    // breaks `walk_region` adds to its requirement-driven ones.
    if dp[n - 1].0.is_finite() {
        let mut b = n - 1;
        while b > 0 {
            let a = from[b];
            if a > 0 && !forced[a] {
                automatic.insert(slots[pts[a]].id);
            }
            b = a;
        }
    }
    automatic
}

/// Walks one region's slots, opening a system at each break requirement and at
/// each optimal automatic break (`optimal_breaks`).
#[allow(clippy::too_many_arguments)]
fn walk_region(
    region: usize,
    slots: &[SlotInfo],
    reqs: &BTreeMap<SpringSlotId, Vec<BreakReq>>,
    origins: &BTreeMap<(u128, bool), EngravingOverrideId>,
    region_source: TypedObjectId,
    width_limit: f32,
    systems: &mut Vec<SystemPlan>,
    skipped: &mut Vec<EngravingDecision>,
) {
    // The optimal automatic breaks (a global badness-minimizing partition,
    // bounded by the break requirements); the walk opens a system at each.
    let automatic = optimal_breaks(slots, reqs, width_limit);

    // Overflow safety net. A lead-only (note-less) run can defer a *planned*
    // break past its barline — the DP treats a requirement, or its own chosen
    // automatic break, as a real system start, but the walk skips it when the
    // closing system carries no musical content (the soft-break exception, and
    // the `has_note` guard on the automatic break below). The DP optimizes each
    // requirement-bounded segment independently and cannot foresee that skip, so
    // without a net the following DP-filled system would absorb the furniture
    // measures and overflow. `chunk_hi[i]` — the rightmost content edge of the
    // measure beginning at slot `i` — lets the walk still break before a measure
    // that would overflow the content width, exactly as first-fit did. In the
    // common (content-full) case the DP's break fires first, so the net never
    // triggers and the geometry is the optimizer's.
    let breakable = |slot: &SlotInfo| slot.barline && !slot.final_barline;
    let mut chunk_hi = vec![f32::NEG_INFINITY; slots.len()];
    for i in (0..slots.len()).rev() {
        let next = if i + 1 < slots.len() && !breakable(&slots[i + 1]) {
            chunk_hi[i + 1]
        } else {
            f32::NEG_INFINITY
        };
        chunk_hi[i] = slots[i].hi.max(next);
    }

    let mut local = 0usize;
    let mut current: Vec<usize> = Vec::new();
    let mut has_note = false;
    let mut current_lo = f32::INFINITY;
    let mut open_boundary: Option<Boundary> = None;
    let mut open_page_forced = false;
    let mut open_page_source = DecisionSource::Automatic;

    for (i, slot) in slots.iter().enumerate() {
        let slot_reqs = reqs.get(&slot.id).map(Vec::as_slice).unwrap_or(&[]);
        if current.is_empty() {
            // The region's first slot is already at a system boundary, so a
            // system break here is trivially honoured; a page break still
            // forces this (first) system onto a fresh page.
            for req in slot_reqs {
                if req.page {
                    open_page_forced = true;
                    if open_page_source == DecisionSource::Automatic {
                        open_page_source = origin_source(origins, slot.id, true);
                    }
                }
            }
            current.push(i);
            has_note = slot.note;
            current_lo = slot.lo;
            continue;
        }
        let mut break_here = false;
        let mut page_here = false;
        let mut source = DecisionSource::Automatic;
        for req in slot_reqs {
            if !req.hard && !has_note {
                // The documented exceptional path: honouring this *soft* break
                // would close a system with no musical content (e.g. a bare
                // clef/barline line). It is skipped, and the unhonoured
                // override is recorded as an IR-stage-overridden decision
                // (never silently dropped).
                skipped.push(EngravingDecision::with_source(
                    break_target(region_source, slot.id),
                    if req.page {
                        EngravingDecisionKind::PageBreak
                    } else {
                        EngravingDecisionKind::SystemBreak
                    },
                    DecisionSource::IrOverride,
                ));
                continue;
            }
            break_here = true;
            page_here |= req.page;
            if !matches!(source, DecisionSource::UserOverride(_)) {
                source = origin_source(origins, slot.id, req.page);
            }
        }
        // Optimal casting-off: open a system at a chosen automatic break — or,
        // as the overflow net, before a measure that would overflow the content
        // width — as long as the closing system carries musical content (a
        // lead-only system is never torn off, matching the requirement rule).
        if !break_here
            && has_note
            && (automatic.contains(&slot.id)
                || (breakable(slot) && chunk_hi[i] - current_lo > width_limit))
        {
            break_here = true;
        }
        if break_here {
            systems.push(SystemPlan {
                region,
                local,
                slots: std::mem::take(&mut current),
                boundary: open_boundary.take(),
                page_forced: open_page_forced,
                page_source: open_page_source,
            });
            local += 1;
            open_boundary = Some(Boundary {
                slot: slot.id,
                source,
            });
            open_page_forced = page_here;
            open_page_source = if page_here {
                source
            } else {
                DecisionSource::Automatic
            };
            current.push(i);
            has_note = slot.note;
            current_lo = slot.lo;
        } else {
            current.push(i);
            has_note |= slot.note;
            current_lo = current_lo.min(slot.lo);
        }
    }
    // The region's last system — or, for a region with no slots at all, its
    // single (empty) system, preserving one-system-per-region as the minimum.
    systems.push(SystemPlan {
        region,
        local,
        slots: current,
        boundary: open_boundary,
        page_forced: open_page_forced,
        page_source: open_page_source,
    });
}

/// Decides how a stroke rides the cast systems (see [`StrokeFate`]).
fn stroke_fate(
    stroke: &Stroke,
    spaced: &Stroke,
    input: &ConstrainedLayoutIR,
    system_of_slot: &BTreeMap<SpringSlotId, usize>,
    region_spans: &[Option<(f32, f32)>],
    region_systems: &[Vec<usize>],
    clips: &[(f32, f32)],
) -> StrokeFate {
    // A rigid-width stroke (a ledger line) rides its owning glyph's system, so
    // it translates by exactly the same delta as its notehead.
    if is_rigid_width_stroke(stroke) {
        if let Some(glyph) = owning_glyph(stroke, &input.glyphs) {
            return StrokeFate::Rigid(system_of_slot.get(&glyph.horizontal_slot).copied());
        }
    }
    let lo = spaced.from.x.0.min(spaced.to.x.0);
    let hi = spaced.from.x.0.max(spaced.to.x.0);
    // The owning region: the one whose slot span is nearest (ties to the first).
    let mut best: Option<(usize, f32)> = None;
    for (r, span) in region_spans.iter().enumerate() {
        let Some((rlo, rhi)) = span else { continue };
        let distance = if hi < *rlo {
            rlo - hi
        } else if lo > *rhi {
            lo - rhi
        } else {
            0.0
        };
        if best.map_or(true, |(_, d)| distance < d) {
            best = Some((r, distance));
        }
    }
    let Some((region, _)) = best else {
        return StrokeFate::Rigid(None);
    };
    // The systems of that region the stroke's span overlaps.
    let overlapped: Vec<usize> = region_systems[region]
        .iter()
        .copied()
        .filter(|&s| lo <= clips[s].1 && hi >= clips[s].0)
        .collect();
    match overlapped.len() {
        0 => {
            // In the sliver between two systems' content: nearest system.
            let nearest = region_systems[region]
                .iter()
                .copied()
                .min_by(|&a, &b| {
                    let da = interval_distance(lo, hi, clips[a]);
                    let db = interval_distance(lo, hi, clips[b]);
                    da.total_cmp(&db).then(a.cmp(&b))
                })
                .expect("every region has at least one system");
            StrokeFate::Rigid(Some(nearest))
        }
        1 => StrokeFate::Rigid(Some(overlapped[0])),
        _ => {
            // A system-spanning stroke (a staff line): one segment per system,
            // cut at the systems' content edges, y interpolated along the
            // stroke so a (hypothetical) sloped spanner splits consistently.
            let (x0, y0) = (spaced.from.x.0, spaced.from.y.0);
            let (x1, y1) = (spaced.to.x.0, spaced.to.y.0);
            let point_at = |x: f32| -> Point {
                if (x1 - x0).abs() < f32::EPSILON {
                    Point::new(x, y0)
                } else {
                    let t = (x - x0) / (x1 - x0);
                    Point::new(x, y0 + t * (y1 - y0))
                }
            };
            let segments = overlapped
                .into_iter()
                .map(|s| {
                    let a = lo.max(clips[s].0);
                    let b = hi.min(clips[s].1);
                    (s, point_at(a), point_at(b))
                })
                .collect();
            StrokeFate::Split(segments)
        }
    }
}

/// A curve's casting fate. A curve overlapping ONE system rides it whole
/// (`Rigid(Some(s))`) — the nearest region's system whose clip interval
/// contains the curve's **start** control point, else that region's nearest
/// system; a curve no region claims is `Rigid(None)` (left in the spaced frame,
/// on no page). A curve spanning MULTIPLE systems is `Split` into per-system
/// sub-curves by de Casteljau subdivision at the parameters where its
/// x-monotonic path crosses each system's content-clip edges (a non-monotonic
/// curve — not produced by the engraver — cannot be honestly split and rides
/// its start system whole).
fn curve_fate(
    curve: &Curve,
    region_spans: &[Option<(f32, f32)>],
    region_systems: &[Vec<usize>],
    clips: &[(f32, f32)],
) -> CurveFate {
    let cp = curve.control_points();
    let xs = cp.map(|p| p.x.0);
    let lo = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    // The owning region: the one whose slot span is nearest (ties to the first).
    let mut best: Option<(usize, f32)> = None;
    for (r, span) in region_spans.iter().enumerate() {
        let Some((rlo, rhi)) = span else { continue };
        let distance = interval_distance(lo, hi, (*rlo, *rhi));
        if best.map_or(true, |(_, d)| distance < d) {
            best = Some((r, distance));
        }
    }
    let Some((region, _)) = best else {
        return CurveFate::Rigid(None);
    };
    // The systems of that region the curve's x-span overlaps.
    let overlapped: Vec<usize> = region_systems[region]
        .iter()
        .copied()
        .filter(|&s| lo <= clips[s].1 && hi >= clips[s].0)
        .collect();
    // The start control point pins which single system the curve rides when it
    // does not span a break.
    let start_system = || {
        region_systems[region].iter().copied().min_by(|&a, &b| {
            let da = interval_distance(cp[0].x.0, cp[0].x.0, clips[a]);
            let db = interval_distance(cp[0].x.0, cp[0].x.0, clips[b]);
            da.total_cmp(&db).then(a.cmp(&b))
        })
    };
    match overlapped.len() {
        0 => CurveFate::Rigid(start_system()),
        1 => CurveFate::Rigid(Some(overlapped[0])),
        _ => {
            // A curve spanning a system break is split into per-system
            // sub-curves by de Casteljau subdivision at the parameters where it
            // crosses each system's content clip edges. This needs an
            // x-monotonic curve to invert `x -> t`; a slur is (its control
            // points are x-ascending by construction). A non-monotonic curve
            // (not produced by the engraver) cannot be honestly split, so it
            // rides its start system whole.
            if !is_x_monotonic(cp) {
                return CurveFate::Rigid(start_system());
            }
            let segments = overlapped
                .into_iter()
                .map(|s| {
                    let (clo, chi) = clips[s];
                    let x0 = clo.max(cp[0].x.0);
                    let x1 = chi.min(cp[3].x.0);
                    let t0 = param_at_x(cp, x0);
                    let t1 = param_at_x(cp, x1);
                    (s, sub_cubic(cp, t0, t1))
                })
                .collect();
            CurveFate::Split(segments)
        }
    }
}

/// Whether a cubic's control points ascend in x (so `x -> t` is invertible by
/// bisection), with a non-trivial x-span.
fn is_x_monotonic(cp: [Point; 4]) -> bool {
    cp[0].x.0 <= cp[1].x.0
        && cp[1].x.0 <= cp[2].x.0
        && cp[2].x.0 <= cp[3].x.0
        && cp[3].x.0 > cp[0].x.0
}

/// The parameter `t` at which an x-monotonic cubic's x-coordinate equals `x`
/// (bisection; `x` is clamped to the curve's x-range by the caller).
fn param_at_x(cp: [Point; 4], x: f32) -> f32 {
    let cubic_x = |t: f32| {
        let u = 1.0 - t;
        u * u * u * cp[0].x.0
            + 3.0 * u * u * t * cp[1].x.0
            + 3.0 * u * t * t * cp[2].x.0
            + t * t * t * cp[3].x.0
    };
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if cubic_x(mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Linear interpolation between two points.
fn lerp_point(a: Point, b: Point, t: f32) -> Point {
    Point::new(a.x.0 + (b.x.0 - a.x.0) * t, a.y.0 + (b.y.0 - a.y.0) * t)
}

/// de Casteljau split of a cubic at `t`: `(left [0, t], right [t, 1])`.
fn split_cubic(cp: [Point; 4], t: f32) -> ([Point; 4], [Point; 4]) {
    let a = lerp_point(cp[0], cp[1], t);
    let b = lerp_point(cp[1], cp[2], t);
    let c = lerp_point(cp[2], cp[3], t);
    let d = lerp_point(a, b, t);
    let e = lerp_point(b, c, t);
    let f = lerp_point(d, e, t);
    ([cp[0], a, d, f], [f, e, c, cp[3]])
}

/// The sub-cubic of `cp` over the parameter range `[t0, t1]` (two de Casteljau
/// splits: take `[0, t1]`, then within it the `[t0/t1, 1]` tail).
fn sub_cubic(cp: [Point; 4], t0: f32, t1: f32) -> [Point; 4] {
    let (left, _) = split_cubic(cp, t1);
    let tt = if t1 > f32::EPSILON {
        (t0 / t1).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (_, right) = split_cubic(left, tt);
    right
}

/// Distance from the span `[lo, hi]` to a clip interval (0 when they overlap).
fn interval_distance(lo: f32, hi: f32, clip: (f32, f32)) -> f32 {
    if hi < clip.0 {
        clip.0 - hi
    } else if lo > clip.1 {
        lo - clip.1
    } else {
        0.0
    }
}

/// A stroke translated rigidly by `(dx, dy)`.
fn translated(stroke: &Stroke, dx: f32, dy: f32) -> Stroke {
    Stroke {
        provenance: stroke.provenance.clone(),
        from: Point::new(stroke.from.x.0 + dx, stroke.from.y.0 + dy),
        to: Point::new(stroke.to.x.0 + dx, stroke.to.y.0 + dy),
        thickness: stroke.thickness,
        layer: stroke.layer,
        style: stroke.style,
        vertical_band: stroke.vertical_band,
    }
}

/// A system's placement: rigid (translated to the left margin) unless the
/// system JUSTIFIES — a non-final system of its region, narrower than the
/// content width, with a positive slot span — in which case the horizontal
/// slack is spread linearly so the system's ink fills the content width (its
/// leftmost ink at the left margin, its rightmost at the right margin). A
/// region's last system stays ragged-right, as engraving convention wants; a
/// system already at or over width is not compressed into overlap.
fn justify_system(
    plan: &SystemPlan,
    ext: &Extent,
    base_dx: f32,
    dy: f32,
    region_slots: &[Vec<SlotInfo>],
    region_systems: &[Vec<usize>],
    width_limit: f32,
) -> Placement {
    let is_last = plan.local + 1 >= region_systems[plan.region].len();
    if is_last || !width_limit.is_finite() {
        return Placement::rigid(base_dx, dy);
    }
    let slots = &region_slots[plan.region];
    let (Some(&first), Some(&last)) = (plan.slots.first(), plan.slots.last()) else {
        return Placement::rigid(base_dx, dy);
    };
    let x0 = slots[first].x;
    let x1 = slots[last].x;
    let span = x1 - x0;
    let extra = width_limit - (ext.max_x - ext.min_x);
    if span <= f32::EPSILON || extra <= f32::EPSILON {
        return Placement::rigid(base_dx, dy);
    }
    // Within [x0, x1]: world_x(x) = x + base_dx + extra·(x − x0)/span, i.e.
    // a·x + b. Beyond it, `Placement::x` falls back to rigid slope 1.
    Placement {
        a: 1.0 + extra / span,
        b: base_dx - extra * x0 / span,
        dy,
        x0,
        x1,
    }
}

/// Places a whole stroke under a system's justification. A per-event component
/// stroke (a stem or ledger) tracks its notehead: both endpoints translate by
/// the owning slot's delta, so it stays attached without stretching its offset.
/// A spanning stroke (a staff line, a volta bracket) stretches with the system:
/// each endpoint maps through the affine.
fn place_stroke(
    source: &Stroke,
    spaced: &Stroke,
    p: Placement,
    slot_source_x: &BTreeMap<SpringSlotId, f32>,
    glyphs: &[GlyphObject],
) -> Stroke {
    if let Some(dx) = crate::component_glyph(source, glyphs)
        .and_then(|g| slot_source_x.get(&g.horizontal_slot))
        .map(|&sx| p.slot_dx(sx))
    {
        return translated(spaced, dx, p.dy);
    }
    Stroke {
        provenance: spaced.provenance.clone(),
        from: Point::new(p.x(spaced.from.x.0), spaced.from.y.0 + p.dy),
        to: Point::new(p.x(spaced.to.x.0), spaced.to.y.0 + p.dy),
        thickness: spaced.thickness,
        layer: spaced.layer,
        style: spaced.style,
        vertical_band: spaced.vertical_band,
    }
}

/// Accumulated staff-line geometry within one system, for the resolved staff
/// record: the extent of the staff's line segments and the provenance of its
/// bottom line (the segment that anchors the staff in this system).
struct StaffAgg {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    bottom: (f32, Provenance),
}

/// Folds a world-frame staff-line stroke into its `(system, staff)` aggregate.
fn mark_staff(
    marks: &mut BTreeMap<(usize, StaffId), StaffAgg>,
    system: usize,
    staff: StaffId,
    stroke: &Stroke,
) {
    let half = (stroke.thickness.0 * 0.5).max(0.0);
    let (lo_x, hi_x) = (
        stroke.from.x.0.min(stroke.to.x.0),
        stroke.from.x.0.max(stroke.to.x.0),
    );
    let (lo_y, hi_y) = (
        stroke.from.y.0.min(stroke.to.y.0) - half,
        stroke.from.y.0.max(stroke.to.y.0) + half,
    );
    marks
        .entry((system, staff))
        .and_modify(|agg| {
            agg.min_x = agg.min_x.min(lo_x);
            agg.max_x = agg.max_x.max(hi_x);
            agg.min_y = agg.min_y.min(lo_y);
            agg.max_y = agg.max_y.max(hi_y);
            if lo_y < agg.bottom.0 {
                agg.bottom = (lo_y, stroke.provenance.clone());
            }
        })
        .or_insert_with(|| StaffAgg {
            min_x: lo_x,
            max_x: hi_x,
            min_y: lo_y,
            max_y: hi_y,
            bottom: (lo_y, stroke.provenance.clone()),
        });
}

/// Builds one populated [`ResolvedSystem`]: a real world-frame bounding box, a
/// staff record per staff whose lines reach this system (top staff first), and
/// a measure record per measure-start barline column the system carries. What
/// the pipeline does not know is left empty, never fabricated: a staff with no
/// engraved lines yields no staff record, and the final-barline measure (whose
/// start no column marks) yields no measure record.
fn build_system(
    system: usize,
    plan: &SystemPlan,
    input: &ConstrainedLayoutIR,
    region_slots: &[Vec<SlotInfo>],
    extents: &[Extent],
    placements: &[Placement],
    staff_marks: &BTreeMap<(usize, StaffId), StaffAgg>,
) -> ResolvedSystem {
    let region = &input.regions[plan.region];
    let p = placements[system];
    let ext = &extents[system];
    let provenance = if plan.local == 0 {
        region.provenance.clone()
    } else {
        // A region's second and later systems are engraver-created objects:
        // synthesized from the region under `EngravedBreak`, keyed by the
        // region-local system ordinal in its own key namespace.
        Provenance::synthesized(
            region.provenance.source,
            SynthesisKind::EngravedBreak,
            SynthesisInstanceKey((KEY_NS_SYSTEM << 64) | plan.local as u128),
            region.provenance.dependencies.clone(),
        )
    };
    let bounding_box = Rect {
        // Justification stretches the horizontal extent: the box spans the
        // system's world-frame ink, which for a justified system is the content
        // width.
        origin: Point::new(p.x(ext.min_x), ext.min_y + p.dy),
        size: Size2D {
            width: StaffSpace(p.x(ext.max_x) - p.x(ext.min_x)),
            height: StaffSpace(ext.max_y - ext.min_y),
        },
    };

    let mut staves: Vec<ResolvedStaff> = staff_marks
        .range((system, StaffId::from_raw(0))..=(system, StaffId::from_raw(u128::MAX)))
        .map(|(&(_, staff), agg)| ResolvedStaff {
            provenance: agg.bottom.1.clone(),
            staff,
            bounding_box: Rect {
                origin: Point::new(agg.min_x, agg.min_y),
                size: Size2D {
                    width: StaffSpace(agg.max_x - agg.min_x),
                    height: StaffSpace(agg.max_y - agg.min_y),
                },
            },
        })
        .collect();
    // Top staff first — the reading order of the system.
    staves.sort_by(|a, b| {
        let top_a = a.bounding_box.origin.y.0 + a.bounding_box.size.height.0;
        let top_b = b.bounding_box.origin.y.0 + b.bounding_box.size.height.0;
        top_b.total_cmp(&top_a)
    });

    // Measures: each measure-start barline column opens a span that runs to the
    // next such column in this system, or to the system's content edge.
    let slots = &region_slots[plan.region];
    let marks: Vec<(usize, usize)> = plan
        .slots
        .iter()
        .filter_map(|&i| slots[i].measure_barline.map(|g| (i, g)))
        .collect();
    let measures: Vec<ResolvedMeasure> = marks
        .iter()
        .enumerate()
        .filter_map(|(k, &(i, g))| {
            let glyph = &input.glyphs[g];
            let TypedObjectId::Measure(measure) = glyph.provenance.source else {
                return None;
            };
            let start = slots[i].lo;
            let end = marks
                .get(k + 1)
                .map(|&(next, _)| slots[next].lo)
                .unwrap_or(ext.max_x);
            Some(ResolvedMeasure {
                provenance: glyph.provenance.clone(),
                measure,
                bounding_box: Rect {
                    origin: Point::new(p.x(start), ext.min_y + p.dy),
                    size: Size2D {
                        width: StaffSpace(p.x(end) - p.x(start)),
                        height: StaffSpace(ext.max_y - ext.min_y),
                    },
                },
            })
        })
        .collect();

    ResolvedSystem {
        provenance,
        bounding_box,
        staves,
        measures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_geometry_matches_the_documented_arithmetic() {
        // A4 at an 8 mm staff: 1 staff space = 2 mm.
        let geometry = PageGeometry::default();
        assert_eq!(geometry.size.width.0, 210.0 / 2.0);
        assert_eq!(geometry.size.height.0, 297.0 / 2.0);
        for margin in [
            geometry.margins.top,
            geometry.margins.right,
            geometry.margins.bottom,
            geometry.margins.left,
        ] {
            assert_eq!(margin.0, 15.0 / 2.0);
        }
        assert_eq!(geometry.content_width(), 90.0);
        assert_eq!(geometry.content_height(), 133.5);
    }

    #[test]
    fn pages_stack_downward_with_the_inter_page_gap() {
        let geometry = PageGeometry::default();
        assert_eq!(page_top_content(0, &geometry), -7.5);
        assert_eq!(
            page_top_content(1, &geometry),
            -(148.5 + INTER_PAGE_GAP) - 7.5
        );
    }

    /// A uniform test measure: one break-candidate barline slot per measure,
    /// spanning `[i·10, i·10 + 9]` (each measure ~9 wide, step 10).
    fn measure_slot(i: usize) -> SlotInfo {
        SlotInfo {
            id: SpringSlotId(i as u128 + 1),
            x: i as f32 * 10.0,
            lo: i as f32 * 10.0,
            hi: i as f32 * 10.0 + 9.0,
            members: Vec::new(),
            barline: true,
            final_barline: false,
            note: true,
            measure_barline: None,
        }
    }

    #[test]
    fn optimal_breaks_balances_systems_and_avoids_a_final_widow() {
        // Six uniform measures; the content width fits four (4 measures span 39,
        // 5 span 49). Greedy first-fit packs [4, 2] — a short final system;
        // the optimal search balances to [3, 3] (lower total squared underfill),
        // subsuming the old widow rebalance. One automatic break, before the
        // fourth measure.
        let slots: Vec<SlotInfo> = (0..6).map(measure_slot).collect();
        let breaks = optimal_breaks(&slots, &BTreeMap::new(), 42.0);
        assert_eq!(
            breaks.len(),
            1,
            "one automatic break → two systems: {breaks:?}"
        );
        assert!(
            breaks.contains(&slots[3].id),
            "the break is before the 4th measure (a 3/3 split): {breaks:?}"
        );
    }

    #[test]
    fn optimal_breaks_never_spans_a_forced_break() {
        // A break requirement at the 2nd measure partitions the DP: the first
        // segment is a lone measure [0,1); the optimizer works only within
        // [1,6). So measure 0 stands alone even though it would pack with more,
        // and no automatic break coincides with the forced one.
        let slots: Vec<SlotInfo> = (0..6).map(measure_slot).collect();
        let mut reqs: BTreeMap<SpringSlotId, Vec<BreakReq>> = BTreeMap::new();
        reqs.insert(
            slots[1].id,
            vec![BreakReq {
                page: false,
                hard: true,
            }],
        );
        let breaks = optimal_breaks(&slots, &reqs, 42.0);
        assert!(
            !breaks.contains(&slots[1].id),
            "the forced break is walk_region's, never reported here: {breaks:?}"
        );
        // The remaining measures [1..6) (5 of them, width 49 > 42) split
        // optimally within their segment — every reported break is inside it.
        for id in &breaks {
            assert!(
                slots[2..].iter().any(|s| s.id == *id),
                "an automatic break stays inside the post-requirement segment: {id:?}"
            );
        }
    }

    #[test]
    fn optimal_breaks_is_deterministic_and_empty_when_unbounded() {
        let slots: Vec<SlotInfo> = (0..6).map(measure_slot).collect();
        let a = optimal_breaks(&slots, &BTreeMap::new(), 42.0);
        let b = optimal_breaks(&slots, &BTreeMap::new(), 42.0);
        assert_eq!(a, b, "a pure function of the inputs");
        assert!(
            optimal_breaks(&slots, &BTreeMap::new(), f32::INFINITY).is_empty(),
            "an unbounded width wraps nothing"
        );
    }

    #[test]
    fn a_content_less_measure_before_a_soft_break_never_overflows() {
        // Review Finding 1: a note-less leading measure (M0, clef/key/time only)
        // whose barline carries a SOFT break. `walk_region` skips the break (the
        // closing system has no content) and the DP, which treated that barline
        // as a forced segment boundary, cannot foresee the skip. Without the
        // overflow net the optimizer-filled measures after it would absorb M0
        // into a MULTI-measure overfull system; the net breaks before the measure
        // that would overflow instead. Verify no non-final system is both
        // multi-measure and wider than the content width.
        use epiphany_core::{RegionId, ReplicaId};
        let mk = |i: usize, lo: f32, hi: f32, note: bool| SlotInfo {
            id: SpringSlotId(i as u128 + 1),
            x: lo,
            lo,
            hi,
            members: Vec::new(),
            barline: true,
            final_barline: false,
            note,
            measure_barline: None,
        };
        // A wide note-less M0; then three narrow measures and two wide ones, so
        // the optimizer groups [M1,M2,M3,M4] (its first system, ~39 ≤ 42) and
        // [M5] — which, with M0 prepended by the skipped break, would span
        // M0..M4 ≈ 60 ≫ 42 without the net.
        let slots = vec![
            mk(0, 0.0, 20.0, false),
            mk(1, 21.0, 26.0, true),
            mk(2, 27.0, 32.0, true),
            mk(3, 33.0, 38.0, true),
            mk(4, 39.0, 60.0, true),
            mk(5, 61.0, 82.0, true),
        ];
        let mut reqs: BTreeMap<SpringSlotId, Vec<BreakReq>> = BTreeMap::new();
        reqs.insert(
            slots[1].id,
            vec![BreakReq {
                page: false,
                hard: false,
            }],
        ); // SOFT
        let width_limit = 42.0;
        let mut systems = Vec::new();
        let mut skipped = Vec::new();
        walk_region(
            0,
            &slots,
            &reqs,
            &BTreeMap::new(),
            TypedObjectId::Region(RegionId::new(ReplicaId(1), 1)),
            width_limit,
            &mut systems,
            &mut skipped,
        );
        for (s, plan) in systems.iter().enumerate() {
            let lo = plan
                .slots
                .iter()
                .map(|&k| slots[k].lo)
                .fold(f32::INFINITY, f32::min);
            let hi = plan
                .slots
                .iter()
                .map(|&k| slots[k].hi)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                hi - lo <= width_limit + 1e-3 || plan.slots.len() <= 1,
                "system {s} spans {} measures at width {} > {width_limit}",
                plan.slots.len(),
                hi - lo
            );
        }
        assert!(!skipped.is_empty(), "the skipped soft break is recorded");
    }

    #[test]
    fn repeat_signs_keep_measure_records_honest_and_raise_their_system() {
        use crate::Engraver;
        use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};
        // The repeat fixture draws morphed repeat barlines, a standalone sign,
        // the final-barline dot pair, and volta brackets. None of that may
        // mint a phantom measure record (a standalone sign and the dot pair
        // are repeat-synthesized, not measure barlines) or lose one (a morphed
        // barline still marks its measure): both fixtures cast off to the same
        // nine records — one per measure-*start* barline column; the final
        // measure's barline closes the region and yields none, by convention.
        let solve = |score| {
            Engraver::default().solve(
                &to_constrained(&to_logical(&score)),
                &SolverConfig::default(),
            )
        };
        let plain = solve(epiphany_testkit::fixtures::ten_measure_single_staff(
            0x000A_11CE,
        ));
        let repeats = solve(epiphany_testkit::fixtures::ten_measure_with_repeats(
            0x000A_11CE,
        ));
        let measure_count = |report: &crate::SolveReport| -> usize {
            report
                .layout
                .pages
                .iter()
                .flat_map(|page| &page.systems)
                .map(|system| system.measures.len())
                .sum()
        };
        assert_eq!(measure_count(&plain), 9);
        assert_eq!(measure_count(&repeats), 9);
        // The volta brackets sit above the staff, so the system carrying them
        // is taller than any repeat-free system.
        let max_height = |report: &crate::SolveReport| -> f32 {
            report
                .layout
                .pages
                .iter()
                .flat_map(|page| &page.systems)
                .map(|system| system.bounding_box.size.height.0)
                .fold(0.0, f32::max)
        };
        assert!(max_height(&repeats) > max_height(&plain));
    }

    #[test]
    fn the_widow_rebalance_evens_the_final_system() {
        use crate::Engraver;
        use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};
        // The ten-measure fixture wraps into two systems under the default A4
        // geometry. Greedy first-fit alone leaves a two-measure stub final
        // system; the widow rebalance evens the split so the final system
        // carries a substantial share of the measures — while the system
        // *count* is unchanged. (Justification now stretches every non-final
        // system to the full content width, so the rebalance's effect shows in
        // the MEASURE distribution, not the baked widths — the non-final system
        // fills the width regardless.)
        let input = to_constrained(&to_logical(
            &epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
        ));
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        let page = &report.layout.pages[0];
        assert_eq!(page.systems.len(), 2, "the fixture wraps into two systems");
        let first = page.systems[0].measures.len();
        let last = page.systems[1].measures.len();
        assert!(
            last * 2 >= first,
            "the rebalanced final system carries a substantial share of the \
             measures, not a stub: {last} vs {first}"
        );
    }

    #[test]
    fn sub_cubic_reproduces_the_original_curve_on_its_sub_range() {
        // de Casteljau correctness: the sub-cubic over [t0, t1], evaluated at
        // its own parameter u in [0, 1], equals the original evaluated at
        // t0 + u·(t1 - t0). A slur-shaped x-ascending cubic.
        let cp = [
            Point::new(0.0, 0.0),
            Point::new(2.0, 3.0),
            Point::new(6.0, 3.0),
            Point::new(8.0, 0.0),
        ];
        let eval = |p: [Point; 4], t: f32| -> Point {
            let u = 1.0 - t;
            Point::new(
                u * u * u * p[0].x.0
                    + 3.0 * u * u * t * p[1].x.0
                    + 3.0 * u * t * t * p[2].x.0
                    + t * t * t * p[3].x.0,
                u * u * u * p[0].y.0
                    + 3.0 * u * u * t * p[1].y.0
                    + 3.0 * u * t * t * p[2].y.0
                    + t * t * t * p[3].y.0,
            )
        };
        let (t0, t1) = (0.3_f32, 0.75_f32);
        let sub = sub_cubic(cp, t0, t1);
        for i in 0..=10 {
            let u = i as f32 / 10.0;
            let on_sub = eval(sub, u);
            let on_orig = eval(cp, t0 + u * (t1 - t0));
            assert!(
                (on_sub.x.0 - on_orig.x.0).abs() < 1e-4 && (on_sub.y.0 - on_orig.y.0).abs() < 1e-4,
                "sub-cubic diverges from the original at u={u}: {on_sub:?} vs {on_orig:?}"
            );
        }
        // And `param_at_x` inverts the x-monotonic curve: the point at the found
        // parameter has the requested x.
        assert!(is_x_monotonic(cp));
        let t = param_at_x(cp, 5.0);
        assert!((eval(cp, t).x.0 - 5.0).abs() < 1e-3);
    }

    #[test]
    fn a_slur_spanning_a_system_break_splits_into_per_system_sub_curves() {
        use crate::Engraver;
        use epiphany_core::{Slur, SlurId, SlurKind, SpanStyle, TypedObjectId};
        use epiphany_layout_ir::{
            to_constrained, to_logical, ConstraintSolver, SolverConfig, SynthesisKind,
        };
        // A slur over the whole ten-measure score — its endpoints cast into
        // different systems (the fixture wraps into two), so the curve spans the
        // break.
        let mut score = epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE);
        let events: Vec<_> = score.canvas.regions[0].staff_instances()[0].voices[0]
            .events
            .clone();
        let slur_id: SlurId = score.identity.mint();
        score.cross_cutting.slurs.push(Slur {
            id: slur_id,
            start_event: events[0],
            end_event: events[events.len() - 1],
            kind: SlurKind::Legato,
            curvature_override: None,
            style: SpanStyle::default(),
        });
        let report = Engraver::default().solve(
            &to_constrained(&to_logical(&score)),
            &SolverConfig::default(),
        );
        assert_eq!(report.layout.pages[0].systems.len(), 2, "two systems");

        let slur_curves: Vec<_> = report
            .layout
            .curves
            .iter()
            .filter(|c| c.provenance.source == TypedObjectId::Slur(slur_id))
            .collect();
        // The slur split into ≥2 sub-cubics (one per spanned system).
        assert!(
            slur_curves.len() >= 2,
            "a break-spanning slur splits, got {} segment(s)",
            slur_curves.len()
        );
        // Exactly one segment carries the slur's exact provenance (the surjection
        // recovers the source once); the rest are synthesized continuations.
        let originals = slur_curves
            .iter()
            .filter(|c| c.provenance.synthesis.is_none())
            .count();
        assert_eq!(
            originals, 1,
            "one segment keeps the slur's exact provenance"
        );
        assert!(slur_curves
            .iter()
            .filter(|c| c.provenance.synthesis.is_some())
            .all(|c| matches!(c.provenance.synthesis, Some(SynthesisKind::Registered(_)))));
        // The segments sit in different systems, which casting stacks
        // vertically (each system is translated down and restarts x at the left
        // margin), so a real split separates them in Y — one curve overhanging
        // into the next system would keep a single y-band.
        let y_centroids: Vec<f32> = slur_curves
            .iter()
            .map(|c| (c.p0.y.0 + c.p1.y.0 + c.p2.y.0 + c.p3.y.0) / 4.0)
            .collect();
        let (lo, hi) = (
            y_centroids.iter().copied().fold(f32::INFINITY, f32::min),
            y_centroids
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max),
        );
        assert!(
            hi - lo > 1.0,
            "the segments span distinct system y-bands (a real split), spread {}",
            hi - lo
        );
    }
}
