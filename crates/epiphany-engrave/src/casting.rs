//! The **casting-off pass** — Minimal-tier system breaking, vertical stacking,
//! and page assignment (Chapter 9 §"The Constraint-Solving Stage": the solver
//! "resolve\[s\] page and system breaks"; Chapter 7 §"ResolvedLayoutIR" defines
//! the page/system tree this pass populates).
//!
//! ## The algorithm (greedy first-fit, then a widow rebalance)
//!
//! [`SolverTier::Minimal`](epiphany_layout_ir::SolverTier) requires the break
//! constraint family to be supported and every hard constraint satisfied (or an
//! honest `Unsatisfiable`); it makes **no optimality claim**, so casting-off is
//! a deterministic two-phase heuristic, not an optimal (Knuth–Plass-style) break
//! search. Phase 1 is greedy first-fit; phase 2 (`rebalance_widows`) evens the
//! system widths so the final system is not left a narrow stub. Phase 1:
//!
//! 1. **System breaking.** Per region, walk the spaced spring-slot columns in x
//!    order. Break into systems at **measure boundaries** — the barline columns
//!    (`to_constrained` draws each measure's barline at its start column; the
//!    region-final barline closes the region and is never a break candidate) —
//!    whenever the measure beginning at a barline would overflow the page
//!    content width. A **hard** `SystemBreakAt`/`PageBreakAt` is *always*
//!    honoured at its slot (the slot begins a new system/page); a **soft** one
//!    is honoured unless doing so would close a system with no musical content
//!    (no notehead/rest column) — the documented exceptional path, recorded as
//!    an [`EngravingDecision`] with [`DecisionSource::IrOverride`] per the
//!    spec's override-resolution rule (an unhonoured override is recorded, not
//!    silently dropped). A region with no measures has no automatic break
//!    candidates: it stays one (possibly overfull) system unless breaks force
//!    otherwise. A single measure wider than the page yields an overfull
//!    system — Minimal does not break mid-measure on its own.
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
//! Phase 2 (`rebalance_widows`, run between system breaking and stacking)
//! moves whole trailing measures from a region's penultimate system into its
//! final one to even their widths — the anti-widow refinement, choosing the
//! shift that minimizes the larger of the two distribution penalties the
//! Quality Metric Catalog defines for the break family (width imbalance vs
//! non-final underfill). It leaves the system *count* unchanged and never
//! disturbs a constraint-pinned boundary, so the break structure phase 1
//! established still holds.
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
    continuation_instance_key, is_barline_glyph, is_rigid_width_stroke, synthesized_layout_id,
    BreakClass, BreakKind, ConstrainedLayoutIR, Curve, DecisionSource, EngravingDecision,
    EngravingDecisionKind, EngravingOverrideId, GlyphObjectId, LayoutConstraint, LayoutObjectId,
    Margins, Point, Provenance, Rect, ResolvedGlyph, ResolvedMeasure, ResolvedPage, ResolvedStaff,
    ResolvedSystem, Size2D, SpringSlotId, StaffSpace, Stroke, SynthesisInstanceKey, SynthesisKind,
    SynthesisRegistryId, VerticalBand, VerticalBandId,
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
    /// spanning a system break is drawn whole in its start system (Minimal
    /// boundary: an honest cubic split needs de Casteljau, deferred).
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

    fn add(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        if [x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            self.min_x = self.min_x.min(x0.min(x1));
            self.max_x = self.max_x.max(x0.max(x1));
            self.min_y = self.min_y.min(y0.min(y1));
            self.max_y = self.max_y.max(y0.max(y1));
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

    // ---- Widow rebalance (casting-off phase 2) ----------------------------
    // Greedy first-fit fills every non-final system maximally, which can leave
    // a region's final system a narrow stub; even the system widths without
    // moving any constraint-pinned boundary or changing the system count.
    rebalance_widows(&mut systems, &region_slots, &reqs, width_limit);

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
    // Curves ride one system whole (Minimal boundary: no de Casteljau split) —
    // the system whose clip interval contains the start control point, found
    // via the same nearest-region logic strokes use.
    let curve_systems: Vec<Option<usize>> = spaced_curves
        .iter()
        .map(|curve| {
            curve_system(
                curve,
                &system_of_slot,
                &region_spans,
                &region_systems,
                &clips,
            )
        })
        .collect();

    // ---- System extents ----------------------------------------------------
    let mut extents: Vec<Extent> = vec![Extent::empty(); systems.len()];
    for (s, plan) in systems.iter().enumerate() {
        for &i in &plan.slots {
            for &g in &region_slots[plan.region][i].members {
                let glyph = &spaced_glyphs[g];
                let (x, y) = (glyph.position.x.0, glyph.position.y.0);
                extents[s].add(
                    x + glyph.bounding_box.left.0,
                    y + glyph.bounding_box.bottom.0,
                    x + glyph.bounding_box.right.0,
                    y + glyph.bounding_box.top.0,
                );
            }
        }
    }
    for (fate, spaced) in fates.iter().zip(spaced_strokes) {
        let half = (spaced.thickness.0 * 0.5).max(0.0);
        match fate {
            StrokeFate::Rigid(Some(s)) => extents[*s].add(
                spaced.from.x.0 - half,
                spaced.from.y.0.min(spaced.to.y.0) - half,
                spaced.to.x.0 + half,
                spaced.from.y.0.max(spaced.to.y.0) + half,
            ),
            StrokeFate::Rigid(None) => {}
            StrokeFate::Split(segments) => {
                for (s, from, to) in segments {
                    extents[*s].add(
                        from.x.0 - half,
                        from.y.0.min(to.y.0) - half,
                        to.x.0 + half,
                        from.y.0.max(to.y.0) + half,
                    );
                }
            }
        }
    }
    // A curve's control-point hull (± half-thickness) grows its system's
    // extent, so a slur above the staff raises the system height (page overflow
    // accounts for it) exactly as the volta bracket strokes do.
    for (system, curve) in curve_systems.iter().zip(spaced_curves) {
        let Some(s) = system else { continue };
        let half = (curve.thickness.0 * 0.5).max(0.0);
        for point in curve.control_points() {
            extents[*s].add(
                point.x.0 - half,
                point.y.0 - half,
                point.x.0 + half,
                point.y.0 + half,
            );
        }
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
    let mut placements: Vec<(f32, f32)> = Vec::with_capacity(systems.len());
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
        let dx = geometry.margins.left.0 - ext.min_x;
        let dy = cursor - ext.max_y;
        placements.push((dx, dy));
        page_systems
            .last_mut()
            .expect("a page was opened above")
            .push(s);
        cursor -= height + gap;
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
    let glyphs: Vec<ResolvedGlyph> = spaced_glyphs
        .iter()
        .zip(&input.glyphs)
        .map(|(spaced, glyph)| {
            let (dx, dy) = system_of_slot
                .get(&glyph.horizontal_slot)
                .map(|&s| placements[s])
                .unwrap_or((0.0, 0.0));
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
    for (spaced, fate) in spaced_strokes.iter().zip(&fates) {
        match fate {
            StrokeFate::Rigid(sys) => {
                let (dx, dy) = sys.map(|s| placements[s]).unwrap_or((0.0, 0.0));
                let stroke = translated(spaced, dx, dy);
                if let (Some(s), TypedObjectId::Staff(staff)) = (sys, spaced.provenance.source) {
                    mark_staff(&mut staff_marks, *s, staff, &stroke);
                }
                strokes.push(stroke);
            }
            StrokeFate::Split(segments) => {
                for (k, (s, from, to)) in segments.iter().enumerate() {
                    let (dx, dy) = placements[*s];
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
                        from: Point::new(from.x.0 + dx, from.y.0 + dy),
                        to: Point::new(to.x.0 + dx, to.y.0 + dy),
                        thickness: spaced.thickness,
                        layer: spaced.layer,
                        style: spaced.style,
                    };
                    if let TypedObjectId::Staff(staff) = spaced.provenance.source {
                        mark_staff(&mut staff_marks, *s, staff, &stroke);
                    }
                    if k == 0 {
                        strokes.push(stroke);
                    } else {
                        continuations.push(stroke);
                    }
                }
            }
        }
    }
    strokes.extend(continuations);

    // Curves: each translated whole by its start system's placement (or left
    // in the spaced frame if no region claimed it — same fallback as a
    // Rigid(None) stroke). A curve whose span crosses an internal system break
    // is a documented Minimal boundary: it is drawn whole in its start system,
    // so its end control point lands at (spaced end x + start-system delta) —
    // visually detached from its end note, which cast to a later system. The
    // curve is kept regardless (dropping it would break the round-trip source
    // surjection: the slur's source must be recovered from a resolved
    // primitive); an honest split needs de Casteljau subdivision, deferred to
    // a later tier.
    let curves: Vec<Curve> = spaced_curves
        .iter()
        .zip(&curve_systems)
        .map(|(curve, system)| {
            let (dx, dy) = system.map(|s| placements[s]).unwrap_or((0.0, 0.0));
            let shift = |point: Point| Point::new(point.x.0 + dx, point.y.0 + dy);
            Curve {
                p0: shift(curve.p0),
                p1: shift(curve.p1),
                p2: shift(curve.p2),
                p3: shift(curve.p3),
                ..curve.clone()
            }
        })
        .collect();

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
        region_of_system: systems.iter().map(|plan| plan.region).collect(),
    }
}

/// The world-frame y of page `p`'s content top: pages stack downward from the
/// origin, each a full page height plus [`INTER_PAGE_GAP`] below the previous.
fn page_top_content(p: usize, geometry: &PageGeometry) -> f32 {
    -(p as f32) * (geometry.size.height.0 + INTER_PAGE_GAP) - geometry.margins.top.0
}

/// Greedy first-fit walk over one region's slots (see the module docs).
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
    // Measure look-ahead: `chunk_hi[i]` is the rightmost content edge of the
    // chunk beginning at slot `i` — through the slot before the next breakable
    // barline (the region-final barline closes the last chunk, so it never
    // starts one).
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
        // Greedy first-fit: at a measure boundary, break when the measure
        // beginning here would overflow the content width.
        if !break_here && breakable(slot) && has_note && chunk_hi[i] - current_lo > width_limit {
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

/// **Widow rebalance** — the casting-off pass's second phase (module docs). The
/// greedy first-fit walk fills each non-final system as full as the content
/// width allows, which is optimal for *page fill* but can leave the region's
/// **final** system a narrow stub (a "widow") — exactly what the Quality Metric
/// Catalog's `casting_off_quality` axis penalizes. This pass evens the region's
/// system widths by moving whole trailing measures from the penultimate system
/// into the final one.
///
/// The shift is chosen to **minimize the larger of the two distribution
/// penalties the catalog defines for the break family**: the system-width
/// *imbalance* (`casting_off_quality`, the coefficient of variation of the
/// region's system widths) and the non-final *break* penalty
/// (`system_break_penalty`, the mean of `|W − w|/W` over non-final systems).
/// Each is computed by the same formula the metric census uses (see
/// [`distribution_cost`]). The two axes pull against each other —
/// filling non-final systems (few, wide systems) worsens imbalance; equalizing
/// widths (empty non-final systems) worsens underfill — and both share the
/// catalog's `0.5` worst-tolerable anchor, so their raw quantities are compared
/// directly and the minimizer of their maximum is the width that best satisfies
/// both. It is not a claim of optimality (Minimal makes none); it is a
/// deterministic anti-widow heuristic.
///
/// Only a region's **last** boundary moves, and only when greedy placed it — an
/// `Automatic` boundary with no break requirement or page force pinned to its
/// slot. A user/IR-anchored or page-forced boundary is never disturbed, and the
/// **system count is unchanged**, so page assignment and every break-count
/// invariant the walk established still hold. The penultimate system keeps at
/// least its own first measure (never emptied), and the final system never
/// grows wider than its predecessor (no mirror-image imbalance).
fn rebalance_widows(
    systems: &mut [SystemPlan],
    region_slots: &[Vec<SlotInfo>],
    reqs: &BTreeMap<SpringSlotId, Vec<BreakReq>>,
    width_limit: f32,
) {
    if !(width_limit.is_finite() && width_limit > 0.0) {
        return; // unbounded width: nothing wraps, nothing to even out
    }
    let w_limit = f64::from(width_limit);
    // A region's systems are a contiguous run in `systems` (walk_region appends
    // them per region, in region order); rebalance each run independently.
    let mut start = 0;
    while start < systems.len() {
        let region = systems[start].region;
        let mut end = start;
        while end < systems.len() && systems[end].region == region {
            end += 1;
        }
        rebalance_region(
            &mut systems[start..end],
            &region_slots[region],
            reqs,
            w_limit,
        );
        start = end;
    }
}

/// Rebalances one region's contiguous run of systems (see [`rebalance_widows`]).
fn rebalance_region(
    run: &mut [SystemPlan],
    slots: &[SlotInfo],
    reqs: &BTreeMap<SpringSlotId, Vec<BreakReq>>,
    w_limit: f64,
) {
    let n = run.len();
    if n < 2 {
        return; // a single system has no widow to fix
    }
    let (prev, last) = (n - 2, n - 1);
    // The final boundary must be a greedy one to move it: an `Automatic` system
    // break with no break requirement or page force pinned to its slot.
    let Some(boundary) = run[last].boundary else {
        return;
    };
    if boundary.source != DecisionSource::Automatic
        || run[last].page_forced
        || reqs.contains_key(&boundary.slot)
    {
        return;
    }
    let width = |idx: &[usize]| -> f64 {
        let lo = idx
            .iter()
            .map(|&k| slots[k].lo)
            .fold(f32::INFINITY, f32::min);
        let hi = idx
            .iter()
            .map(|&k| slots[k].hi)
            .fold(f32::NEG_INFINITY, f32::max);
        if hi > lo {
            f64::from(hi - lo)
        } else {
            0.0
        }
    };
    // Widths of the systems before the penultimate stay fixed (only the last
    // boundary moves); the objective's coefficient of variation ranges over all.
    let fixed: Vec<f64> = run[..prev].iter().map(|p| width(&p.slots)).collect();
    // Measure-start positions within the penultimate system — local indices into
    // its slot list. The first is the system's own opening (immovable); a split
    // at a later one moves that measure and the rest into the final system.
    let starts: Vec<usize> = run[prev]
        .slots
        .iter()
        .enumerate()
        .filter(|(_, &k)| slots[k].measure_barline.is_some())
        .map(|(local, _)| local)
        .collect();
    if starts.len() < 2 {
        return; // the penultimate system has one measure — nothing to lend
    }
    // Baseline: the greedy split (move nothing). Iterate candidate splits from
    // the fewest measures moved (latest start) so ties keep the fuller
    // predecessor; accept only a strict improvement.
    let mut best_cost = distribution_cost(
        &fixed,
        width(&run[prev].slots),
        width(&run[last].slots),
        w_limit,
    );
    let mut best_split: Option<usize> = None;
    for &split in starts.iter().skip(1).rev() {
        let kept = &run[prev].slots[..split];
        let moved = &run[prev].slots[split..];
        let last_slots: Vec<usize> = moved.iter().chain(&run[last].slots).copied().collect();
        let (w_prev, w_last) = (width(kept), width(&last_slots));
        if w_last > w_prev {
            continue; // never grow the final system past its predecessor
        }
        let cost = distribution_cost(&fixed, w_prev, w_last, w_limit);
        if cost < best_cost - 1e-9 {
            best_cost = cost;
            best_split = Some(split);
        }
    }
    if let Some(split) = best_split {
        let moved: Vec<usize> = run[prev].slots[split..].to_vec();
        let new_boundary_slot = slots[run[prev].slots[split]].id;
        run[prev].slots.truncate(split);
        let mut new_last = moved;
        new_last.extend_from_slice(&run[last].slots);
        run[last].slots = new_last;
        run[last].boundary = Some(Boundary {
            slot: new_boundary_slot,
            source: DecisionSource::Automatic,
        });
    }
}

/// The rebalance objective (see [`rebalance_widows`]): the larger of the two raw
/// distribution penalties over a region's system widths — the **break** penalty
/// and the width **imbalance**. Both normalize against the catalog's shared
/// `0.5` anchor, so comparing and taking the max of the raw quantities orders
/// candidates exactly as the max of the two normalized metrics does. Each raw is
/// computed by the *same* formula as the axis it stands in for, so the rebalance
/// optimizes against the values the `quality` module will report:
///
/// * **break** — `quality::system_break_raw`'s mean of `|W − w| / W` over the
///   **non-final** systems (absolute, so an overfull non-final system is
///   penalized too);
/// * **imbalance** — `quality::casting_off_raw`'s coefficient of variation over
///   **all** the region's system widths.
fn distribution_cost(fixed: &[f64], w_prev: f64, w_last: f64, w_limit: f64) -> f64 {
    let mut widths: Vec<f64> = fixed.to_vec();
    widths.push(w_prev);
    widths.push(w_last);
    let count = widths.len();
    // Break penalty: mean absolute deviation from the content width over the
    // non-final systems (the final system is exempt) — `system_break_raw`.
    let non_final = &widths[..count - 1];
    let breaks = if non_final.is_empty() {
        0.0
    } else {
        non_final
            .iter()
            .map(|&w| (w_limit - w).abs() / w_limit)
            .sum::<f64>()
            / non_final.len() as f64
    };
    // Imbalance: the coefficient of variation of all system widths — `casting_off_raw`.
    let mean = widths.iter().sum::<f64>() / count as f64;
    let imbalance = if mean > 0.0 {
        let variance = widths.iter().map(|w| (w - mean) * (w - mean)).sum::<f64>() / count as f64;
        variance.sqrt() / mean
    } else {
        0.0
    };
    breaks.max(imbalance)
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

/// The system a curve rides whole: the nearest region's system whose clip
/// interval contains the curve's **start** control point (its drawing origin),
/// else that region's nearest system, else `None` (no region claimed it —
/// left in the spaced frame, on no page). A curve is never split — an honest
/// cubic split across a system break needs de Casteljau subdivision, deferred
/// to a later tier; here a break-spanning slur draws whole in its start system.
fn curve_system(
    curve: &Curve,
    system_of_slot: &BTreeMap<SpringSlotId, usize>,
    region_spans: &[Option<(f32, f32)>],
    region_systems: &[Vec<usize>],
    clips: &[(f32, f32)],
) -> Option<usize> {
    let _ = system_of_slot;
    let xs = curve.control_points().map(|p| p.x.0);
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
    let (region, _) = best?;
    // The start control point pins which system the whole curve rides.
    let start_x = curve.p0.x.0;
    region_systems[region].iter().copied().min_by(|&a, &b| {
        let da = interval_distance(start_x, start_x, clips[a]);
        let db = interval_distance(start_x, start_x, clips[b]);
        da.total_cmp(&db).then(a.cmp(&b))
    })
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
    placements: &[(f32, f32)],
    staff_marks: &BTreeMap<(usize, StaffId), StaffAgg>,
) -> ResolvedSystem {
    let region = &input.regions[plan.region];
    let (dx, dy) = placements[system];
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
        origin: Point::new(ext.min_x + dx, ext.min_y + dy),
        size: Size2D {
            width: StaffSpace(ext.max_x - ext.min_x),
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
                    origin: Point::new(start + dx, ext.min_y + dy),
                    size: Size2D {
                        width: StaffSpace(end - start),
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

    #[test]
    fn distribution_cost_prefers_the_even_split_over_greedy_and_full_balance() {
        // The RS-1 two-system candidates, glyph-ink widths (staff spaces) from
        // the ten-measure fixture at the default 90-wide content area. The
        // widow rebalance minimizes the larger of imbalance (CV) and worst
        // non-final underfill; the greedy 8/2 stub and the fully balanced 5/5
        // both score worse than the six/four split it settles on.
        let w = 90.0;
        let greedy = distribution_cost(&[], 78.57, 18.76, w); // 8/2 stub
        let six_four = distribution_cost(&[], 59.52, 37.80, w); // rebalanced
        let five_five = distribution_cost(&[], 50.00, 47.33, w); // full balance
        assert!(
            six_four < greedy && six_four < five_five,
            "6/4 ({six_four:.4}) must beat greedy ({greedy:.4}) and 5/5 ({five_five:.4})"
        );
    }

    #[test]
    fn distribution_cost_uses_the_mean_break_penalty_not_the_worst() {
        // For three or more systems the break term must be the MEAN of |W-w|/W
        // over the non-final systems — the same quantity `quality::system_break_raw`
        // reports — not the worst single system. With one full leading system
        // fixed at W, the balanced 45/45 tail must beat the 60/30 tail: its width
        // CV is lower, and the mean break penalty (diluted by the full leading
        // system) does not dominate. A worst-underfill proxy would wrongly prefer
        // 60/30 (its lone short system is less empty), inverting the choice.
        let w = 90.0;
        let fixed = [90.0]; // one full non-final system
        let balanced = distribution_cost(&fixed, 45.0, 45.0, w);
        let uneven = distribution_cost(&fixed, 60.0, 30.0, w);
        assert!(
            balanced < uneven,
            "the mean break penalty prefers the balanced tail: {balanced:.4} vs {uneven:.4}"
        );
        // Pin the mean-not-max semantics exactly: over the non-final systems
        // [90, 45] the break penalty is mean(0, 0.5) = 0.25, below the width CV,
        // so the objective here is the CV of [90, 45, 45].
        let widths = [90.0_f64, 45.0, 45.0];
        let mean = widths.iter().sum::<f64>() / 3.0;
        let cv = (widths.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 3.0).sqrt() / mean;
        assert!(
            (balanced - cv).abs() < 1e-12,
            "the objective should equal the width CV here: {balanced} vs {cv}"
        );
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
        // system (its width barely a quarter of the first's); the widow
        // rebalance evens the split so the final system is a substantial
        // fraction of its predecessor — while the system *count* is unchanged.
        let input = to_constrained(&to_logical(
            &epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
        ));
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        let page = &report.layout.pages[0];
        assert_eq!(page.systems.len(), 2, "the fixture wraps into two systems");
        let first = page.systems[0].bounding_box.size.width.0;
        let last = page.systems[1].bounding_box.size.width.0;
        assert!(
            last > 0.5 * first,
            "the rebalanced final system is not a stub: {last} vs {first}"
        );
    }
}
