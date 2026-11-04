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
//!    boundaries** — after a barline column (`to_constrained` draws each
//!    measure's barline where the measure ends, so a system ends on the
//!    barline and the next begins with what follows it; the region-final
//!    barline closes the region). A **hard** `SystemBreakAt`/`PageBreakAt`
//!    is *always* honoured at its slot (and bounds the search's segments); a
//!    **soft** one is honoured unless doing so would close a system with no
//!    musical content (no notehead/rest column) — the documented exceptional
//!    path, recorded as an [`EngravingDecision`] with
//!    [`DecisionSource::IrOverride`] per the spec's override-resolution rule (an
//!    unhonoured override is recorded, not silently dropped). A region with no
//!    measures has no break candidates: it stays one (possibly overfull) system
//!    unless breaks force otherwise. A single measure wider than the page yields
//!    an overfull system — Minimal does not break mid-measure on its own. In a
//!    region with break requirements, the measures between two of them (or
//!    between one and the region's start or end) are a system the score fixes:
//!    one that holds more than the line is kept whole where its floor fits
//!    (`fixed_systems`), and compressed toward that floor when placed
//!    (`compress_system`); where even the floor overflows, the search breaks
//!    it as any other, and each system it adds there is named
//!    (`AddedSystem`).
//! 2. **Vertical stacking.** A system's staves are set column by column by
//!    their outlines and a floor between their lines (`vertical::stack`, X5d.2);
//!    consecutive systems stand at least `vertical::SYSTEM_FLOOR` apart line to
//!    line, the whole system above's ink clear of the whole system below's. A
//!    page's first staff stands `vertical::PAGE_BORDER` below its top margin, or
//!    lower where its ink needs, and a further system goes on the page only
//!    where its last staff keeps that border above the bottom margin; then the
//!    page is spread by MuseScore's rule (`vertical::spread`, X5d.3). A system
//!    taller than a page stays alone on it, risen as far as its top border
//!    allows, and is named where its paper cannot hold it.
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
//! Page geometry is an **engraver-side parameter** ([`PageGeometry`], a
//! constructor argument of [`crate::Engraver`]) with a documented default; see
//! [`PageGeometry::default`] for the arithmetic. The score graph now holds a
//! page too (`Canvas.layout_defaults`, authored by `SetCanvasLayoutDefaults`,
//! whose default is the same page), and a caller drawing a score on its own
//! page builds the parameter from it (`PageGeometry::from`).

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use epiphany_core::{StaffId, TypedObjectId};
use epiphany_layout_ir::{
    continuation_instance_key, is_barline_glyph, is_rigid_width_stroke, metrics,
    synthesized_layout_id, tie_arc, BreakClass, BreakKind, ConstrainedLayoutIR, Curve,
    DecisionSource, EngravingDecision, EngravingDecisionKind, EngravingOverrideId, FamilyLine,
    GlyphObject, GlyphObjectId, GlyphReference, GlyphStyle, GroupSign, GroupSpan, HairpinShape,
    LayoutConstraint, LayoutObjectId, LeadGlyph, LineDashes, LineText, LineTextRole, Margins,
    MeasureMark, Point, PrimitiveIndices, Provenance, Rect, ResolvedGlyph, ResolvedMeasure,
    ResolvedPage, ResolvedStaff, ResolvedSystem, ResolvedText, Size2D, SpringSlotId, StaffSpace,
    Stroke, SynthesisInstanceKey, SynthesisKind, SynthesisRegistryId, SystemLead, TimePoint,
    Transform2D, VerticalBandId, VerticalBandKind, BAR_NOTE_DISTANCE, GLISSANDO_WIGGLE,
    HAIRPIN_CONT_HEIGHT, HAIRPIN_HEIGHT, HAIRPIN_MIN_LENGTH, LYRIC_DASH_MAX,
    LYRIC_DASH_MAX_DISTANCE, LYRIC_DASH_MIN, LYRIC_MELISMA_MIN, TRILL_WIGGLE,
};

use crate::clearance::InkRect;
use crate::owning_glyph;
use crate::spacing::Skyline;

/// What casting needs to space a system again tighter than the spacing pass
/// did: the slots' ink by kind, and each curve (parallel to the input's) where
/// the spacing set it before it passed any ink.
pub(crate) struct Respacing<'a> {
    pub skyline: &'a Skyline,
    pub curves: &'a [[Point; 4]],
}

/// A system the search added among a region's breaks, where the system they
/// fix holds more than the line even at its floor: by how much, in staff
/// spaces, and the measure it opens with.
pub(crate) struct AddedSystem {
    pub overflow: f32,
    pub opens: Option<TypedObjectId>,
}

/// A system whose ink its paper cannot hold, named rather than squeezed
/// (D109 Q5): how far its ink passes the paper, in staff spaces, and the
/// measure it opens with.
pub(crate) struct OverfullSystem {
    pub past: f32,
    pub opens: Option<TypedObjectId>,
}

/// How many halvings of the springs' scale a compressed system's search
/// takes before it settles between the two it has found.
const COMPRESSION_STEPS: usize = 16;

/// The registry id for the engraver's **system-continuation synthesis**: the
/// segment of a region-spanning stroke (a staff line) that casting-off places
/// in a system after the stroke's first. The normative [`SynthesisKind`] set
/// names no purely visual continuation rule, so — like the constrained stage's
/// staff-line/ledger/accidental syntheses — it is carried as a `Registered`
/// extension kind (Chapter 7 §"Behavior Under Unknown Extensions").
pub const SYSTEM_CONTINUATION_SYNTHESIS: SynthesisRegistryId =
    SynthesisRegistryId(0x5359_5354_4D53_4547); // "SYSTMSEG"

/// The registry id for a glyph of a later system's lead (its clef and key
/// signature), synthesized from the staff instance and keyed by the system's
/// region-local ordinal and the glyph.
pub const SYSTEM_LEAD_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x5359_534C_4541_4447); // "SYSLEADG"

/// A staff's height, bottom line to top, in staff spaces.
const STAFF_SPAN: f32 = 4.0;

/// The gap between a system-start lead's ink and the system's first column.
const LEAD_GAP: f32 = 0.8;

/// How far before its system's end a line over a break stops
/// (`lineEndToBarlineDistance`).
const LINE_END_GAP: f32 = 0.25;

/// How much shorter than its least length a melisma line may stand and
/// still be drawn (`LyricsLayout::layoutMelismaLine`'s tolerance).
const LYRIC_MELISMA_TOLERANCE: f32 = 0.05;

/// A tie continued into a later system starts this far clear of the system's
/// lead (the widest of its staves' clefs and key signatures)…
const TIE_LEAD_CLEARANCE: f32 = 0.4;
/// …and runs at least this far to its note: a system that opens on the note
/// widens its lead by what the gap after the lead leaves short.
const TIE_CONTINUATION: f32 = 1.5;

/// The registry id for a staff group's sign (its brace, its bracket's line and
/// ends, its sub-bracket's line and hooks) and a system's opening line,
/// synthesized from the group or the region and keyed by the system's
/// region-local ordinal and the element.
pub const GROUP_SIGN_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x4752_5053_4947_4E53); // "GRPSIGNS"
/// The registry id for a barline's continuation down to the next staff of its
/// group, synthesized from the barline's measure and keyed by its line.
pub const JOINED_BARLINE_SYNTHESIS: SynthesisRegistryId =
    SynthesisRegistryId(0x4A4F_494E_4241_524C); // "JOINBARL"
/// SMuFL's thin barline and bracket thicknesses, and the separation between
/// a thin and a thick barline.
const THIN_BARLINE: f32 = 0.16;
const BRACKET_THICKNESS: f32 = 0.5;
const BARLINE_SEPARATION: f32 = 0.4;
/// The gap between a system's opening line and a group sign left of it, and a
/// sub-bracket's hooks' length.
const GROUP_SIGN_GAP: f32 = 0.5;
const SUB_BRACKET_HOOK: f32 = 0.5;

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
/// of [`crate::Engraver`]; a score's own page, its canvas's layout defaults,
/// converts into one.
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

    /// Where the page at `index` (from 0) lies in the layout's world: the
    /// rectangle the engraver stacked it in, a full page height plus
    /// [`INTER_PAGE_GAP`] below the page before it, page 1's top-left corner at
    /// the origin. Its origin is its bottom-left corner (the world is y-up).
    pub fn page_frame(&self, index: usize) -> Rect {
        let top = -(index as f32) * (self.size.height.0 + INTER_PAGE_GAP);
        Rect {
            origin: Point::new(0.0, top - self.size.height.0),
            size: self.size,
        }
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

impl From<&epiphany_core::CanvasLayoutDefaults> for PageGeometry {
    /// The page a score's canvas sets it on (`Canvas.layout_defaults`, authored by
    /// `SetCanvasLayoutDefaults`): its size and margins, in staff spaces. The
    /// canvas's default is this type's default.
    fn from(defaults: &epiphany_core::CanvasLayoutDefaults) -> Self {
        let ss = |v: f64| StaffSpace(v as f32);
        PageGeometry {
            size: Size2D {
                width: ss(defaults.page_size.width.get()),
                height: ss(defaults.page_size.height.get()),
            },
            margins: Margins {
                top: ss(defaults.margins.top.get()),
                right: ss(defaults.margins.right.get()),
                bottom: ss(defaults.margins.bottom.get()),
                left: ss(defaults.margins.left.get()),
            },
        }
    }
}

/// What the casting-off pass produced: the final world-frame geometry, the
/// populated page/system tree, the engraver's appended break decisions, and the
/// break structure the constraint evaluation consults.
pub(crate) struct CastLayout {
    /// Final glyphs, in input order, positions baked into the world frame, then
    /// the glyphs the casting pass adds: later systems' leads and staff groups'
    /// signs.
    pub glyphs: Vec<ResolvedGlyph>,
    /// The vertical band of each glyph the casting pass adds after the
    /// input's (a later system's lead, a staff group's sign), in order.
    pub appended_bands: Vec<VerticalBandId>,
    /// The vertical band of each text the casting pass adds after the
    /// input's (a line's runs, drawn piece by piece: the band of the line
    /// they ride), in order.
    pub appended_text_bands: Vec<VerticalBandId>,
    /// Final strokes: the input strokes in order (each translated with its
    /// system; a system-spanning stroke replaced by its first segment), then
    /// the synthesized continuation segments.
    pub strokes: Vec<Stroke>,
    /// Final curves, in input order, each translated with its system. A curve
    /// spanning a system break is split into per-system sub-curves by de
    /// Casteljau subdivision (the first keeps the source's provenance, the rest
    /// are synthesized continuations, like system-spanning strokes).
    pub curves: Vec<Curve>,
    /// Final text runs, in input order, each moved with its slot's system.
    pub texts: Vec<ResolvedText>,
    /// The populated page tree (empty when the input declares no regions).
    pub pages: Vec<ResolvedPage>,
    /// Break decisions this pass made (chosen breaks in reading order, then
    /// the skipped-soft `IrOverride` records in walk order).
    pub decisions: Vec<EngravingDecision>,
    /// Slots at which the final layout breaks: the first slot of every system.
    pub system_start_slots: BTreeSet<SpringSlotId>,
    /// Slots at which a page begins: the first slot of each page's first system.
    pub page_start_slots: BTreeSet<SpringSlotId>,
    /// The system each baked glyph landed in, parallel to `glyphs` — derived
    /// once, inside the casting pass, from the slot→system assignment that
    /// pass computes for its own use. **This is the glyph→system attribution,
    /// full stop** (W1 pin 7): the raw slot map is deliberately *not*
    /// published, so no consumer can grow a second copy of the rule that then
    /// drifts. `None`: the glyph's slot was claimed by no region, so it
    /// belongs to no per-system aggregate.
    pub glyph_system: Vec<Option<usize>>,
    /// The system each baked stroke landed in, parallel to `strokes` (including
    /// the appended continuation segments). A stroke carries no spring slot, so
    /// the slot map cannot answer for it; the casting pass records what it
    /// already knew. `None`: claimed by no region.
    pub stroke_system: Vec<Option<usize>>,
    /// The system each baked curve landed in, parallel to `curves`.
    pub curve_system: Vec<Option<usize>>,
    /// The region each system slices, indexed by global system index (the
    /// per-region grouping the casting-off quality metrics aggregate by).
    pub region_of_system: Vec<usize>,
    /// The primitives no system claims — `glyph_system`/`stroke_system`/
    /// `curve_system` entries of `None`, gathered into the same shape
    /// [`ResolvedSystem::primitives`] uses (W1 pin 3: unowned is a first-class
    /// bucket, never coerced onto a system).
    pub unowned: PrimitiveIndices,
    /// Each system the search added among a region's breaks where even the
    /// floor overflows.
    pub added: Vec<AddedSystem>,
    /// Each system whose ink passes its paper.
    pub overfull: Vec<OverfullSystem>,
    /// Each glissando of one piece the cast page leaves too short for its
    /// notes, shorter than its line is thick or turned back, and so left
    /// undrawn (X4d fix round 2).
    pub short_glissandi: BTreeSet<TypedObjectId>,
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
    /// The column carries a measure's barline — a measure boundary, after
    /// which a system may break.
    barline: bool,
    /// The column carries the region-final barline (nothing follows it).
    final_barline: bool,
    /// The column carries musical content (a notehead or a rest).
    note: bool,
    /// The directly-manifested barline glyph that ends a measure (glyph
    /// index), for the per-system measure records.
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
/// map `world_x = x + dx + stretch(x)`.
///
/// A rigid (unjustified) system has no stretch — a pure translation by `dx`.
/// A **justified** system spreads the horizontal slack (content width minus
/// natural ink width) over its note columns' springs, each interval between
/// two slots taking its share by its spring (the earlier slot's
/// `stretch_factor · preferred_width`), so the durations' proportions hold
/// and the gaps after a lead, a barline or a signature keep their width.
/// `stretch` is piecewise linear through `knots`, `(spaced slot x, slack
/// taken before it)`, and constant beyond them (a glyph's bearing overhang, a
/// staff line drawn to the ink edge), so the mapped ink extremes agree
/// exactly with the per-slot deltas at the first and last slots. The map is
/// applied SLOT-RELATIVELY to glyphs — each slot's members translate by the
/// map evaluated at the slot's source, so intra-slot offsets (a time
/// signature after its barline, an accidental left of its notehead) survive
/// verbatim — directly to spanning-stroke and curve endpoints, and via the
/// owning slot for a rigid-width stroke (a stem or ledger that must stay
/// attached to its notehead, not stretch).
#[derive(Clone)]
struct Placement {
    dx: f32,
    dy: f32,
    /// `(slot source x, slack before it)`, increasing in x, the slack rising
    /// in a justified system and falling (below zero) in a compressed one;
    /// empty for a rigid system.
    knots: Rc<[(f32, f32)]>,
}

impl Placement {
    /// A pure translation (an unjustified system, or the identity fallback for
    /// content no system claims).
    fn rigid(dx: f32, dy: f32) -> Self {
        Placement {
            dx,
            dy,
            knots: Rc::from(Vec::new()),
        }
    }
    /// The slack the map adds at a spaced x: interpolated between the knots
    /// around it, constant before the first and after the last.
    fn stretch(&self, x: f32) -> f32 {
        let k = &self.knots;
        let (Some(first), Some(last)) = (k.first(), k.last()) else {
            return 0.0;
        };
        if x <= first.0 {
            return first.1;
        }
        if x >= last.0 {
            return last.1;
        }
        let i = k.partition_point(|knot| knot.0 <= x);
        let ((x0, s0), (x1, s1)) = (k[i - 1], k[i]);
        if x1 - x0 <= f32::EPSILON {
            s1
        } else {
            s0 + (s1 - s0) * (x - x0) / (x1 - x0)
        }
    }
    /// The world x of a spaced x.
    fn x(&self, x: f32) -> f32 {
        x + self.dx + self.stretch(x)
    }
    /// The rigid delta every glyph in a slot whose source is `slot_x`
    /// translates by — constant per slot, so intra-slot offsets are preserved.
    fn slot_dx(&self, slot_x: f32) -> f32 {
        self.dx + self.stretch(slot_x)
    }
    /// The same placement sunk downward by `shift` — the inter-staff solve
    /// pushes a staff's content down within its system (y-down is decreasing y).
    fn sunk(&self, shift: f32) -> Self {
        Placement {
            dy: self.dy - shift,
            ..self.clone()
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
    spaced_texts: &[ResolvedText],
    respacing: &Respacing,
    geometry: &PageGeometry,
) -> CastLayout {
    // ---- Slot table (spaced coordinates) --------------------------------
    // The family's glyphs that take no room across keep their slot but give
    // it no width.
    let roomless = crate::family::roomless_glyphs(input);
    let mut slots: BTreeMap<SpringSlotId, SlotInfo> = BTreeMap::new();
    for (i, (glyph, spaced)) in input.glyphs.iter().zip(spaced_glyphs).enumerate() {
        let name = glyph.glyph.as_str();
        let x = spaced.position.x.0;
        let (lo, hi) = if roomless.contains(&glyph.id()) {
            (f32::INFINITY, f32::NEG_INFINITY)
        } else {
            (
                x + glyph.bounding_box.left.0,
                x + glyph.bounding_box.right.0,
            )
        };
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
            }
            if entry.measure_barline.is_none() {
                entry.measure_barline = Some(i);
            }
        }
        if name.starts_with("notehead") || name.starts_with("rest") {
            entry.note = true;
        }
    }
    // A slot's reference x is its first glyph not of the family, as its
    // source is in the spacing pass: the projection emits a note's marks
    // before its head, and a mark standing off its note (a scoop left of its
    // head), even one taking room, must neither order its column nor take
    // its share of a justified stretch.
    let family = crate::family::family_glyphs(input);
    for info in slots.values_mut() {
        if let Some(&i) = info
            .members
            .iter()
            .find(|&&i| !family.contains(&input.glyphs[i].id()))
        {
            info.x = spaced_glyphs[i].position.x.0;
        }
    }
    // A slot holding only such glyphs stands at its source.
    for info in slots.values_mut() {
        if info.lo > info.hi {
            (info.lo, info.hi) = (info.x, info.x);
        }
    }
    // A run of the family that takes room, an arpeggio, a syllable or a
    // grace's mark kept off a barline, widens its slot as the spacing pass
    // lets it.
    let room: BTreeSet<GlyphObjectId> = crate::family::texts_with_room(input)
        .into_iter()
        .filter(|(_, room)| *room != epiphany_layout_ir::Room::None)
        .map(|(text, _)| text.id())
        .collect();
    for (text, spaced) in input.texts.iter().zip(spaced_texts) {
        if !room.contains(&text.id()) {
            continue;
        }
        if let Some(entry) = slots.get_mut(&text.horizontal_slot) {
            let x = spaced.origin.x.0;
            entry.lo = entry.lo.min(x + spaced.bounds.left.0);
            entry.hi = entry.hi.max(x + spaced.bounds.right.0);
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
    // Each region's system-start leads, and the widest of them: a later
    // system's content starts after its lead, so breaking reserves it.
    let mut leads_of: Vec<Vec<&SystemLead>> = vec![Vec::new(); input.regions.len()];
    for lead in &input.system_leads {
        if let Some(leads) = leads_of.get_mut(lead.region) {
            leads.push(lead);
        }
    }
    let lead_max: Vec<f32> = leads_of
        .iter()
        .map(|leads| {
            leads
                .iter()
                .flat_map(|lead| &lead.entries)
                .map(|(_, glyphs)| lead_width(glyphs))
                .fold(0.0, f32::max)
        })
        .collect();
    // The room a tie continued into a later system needs, by the slot that
    // system would open on. The tie's second half starts `TIE_LEAD_CLEARANCE`
    // clear of the lead, or of the columns before its note there, which hold
    // no note (a time signature), and runs at least `TIE_CONTINUATION` to its
    // note; the gap the spacing leaves may fall short. The system's lead
    // widens by the room, and its opening columns keep their place after the
    // lead, so the room stands before the note.
    let anchors = crate::span_anchors(input);
    let slot_index: BTreeMap<SpringSlotId, (usize, usize)> = region_slots
        .iter()
        .enumerate()
        .flat_map(|(r, infos)| infos.iter().enumerate().map(move |(i, s)| (s.id, (r, i))))
        .collect();
    let mut room: Vec<Vec<f32>> = region_slots
        .iter()
        .map(|infos| vec![0.0; infos.len()])
        .collect();
    for curve in spaced_curves {
        let Some((start, end)) = anchors.get(&curve.id()) else {
            continue;
        };
        let (Some(&(r, a)), Some(&(rb, b))) = (slot_index.get(start), slot_index.get(end)) else {
            continue;
        };
        let slots = &region_slots[r];
        if r != rb || a >= b {
            continue;
        }
        let Some(i) = (a + 1..=b).rev().find(|&i| opens_measure(slots, i)) else {
            continue;
        };
        if slots[i..b].iter().any(|info| info.note) {
            continue;
        }
        let before = slots[i..b]
            .iter()
            .map(|info| info.hi)
            .fold(slots[i].lo - LEAD_GAP, f32::max);
        let short = TIE_CONTINUATION - (curve.p3.x.0 - before - TIE_LEAD_CLEARANCE);
        room[r][i] = room[r][i].max(short);
    }
    // A system's staff lines, and a ledger line at either end, reach half
    // their thickness past the columns' ink, which the break search measures
    // by: it reserves the widest such stroke's thickness, a half at each end.
    let edge = input
        .strokes
        .iter()
        .filter(|s| {
            is_rigid_width_stroke(s) || matches!(s.provenance.source, TypedObjectId::Staff(_))
        })
        .map(|s| s.thickness.0)
        .fold(0.0, f32::max);
    // A region's first system opens with its staff lines, which start left of
    // its first column's ink.
    let staff_left: Vec<f32> = region_slots
        .iter()
        .map(|infos| {
            let Some(first) = infos.first() else {
                return f32::INFINITY;
            };
            spaced_strokes
                .iter()
                .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
                .map(|s| (s.from.x.0.min(s.to.x.0), s.from.x.0.max(s.to.x.0)))
                .filter(|&(lo, hi)| lo <= first.lo && hi >= first.lo)
                .map(|(lo, _)| lo)
                .fold(first.lo, f32::min)
        })
        .collect();
    let slot_time: BTreeMap<SpringSlotId, &TimePoint> = input
        .horizontal_slots
        .iter()
        .map(|slot| (slot.id, &slot.time))
        .collect();
    // The width of the lead a later system opening at a slot draws: each
    // staff's lead in force at its time, the widest.
    let opening_lead = |r: usize, slot: SpringSlotId| -> f32 {
        let Some(time) = slot_time.get(&slot) else {
            return lead_max[r];
        };
        leads_of[r]
            .iter()
            .filter_map(|lead| {
                lead.entries
                    .iter()
                    .rev()
                    .find(|(from, _)| at_or_before(from, time))
                    .or(lead.entries.first())
                    .map(|(_, glyphs)| lead_width(glyphs))
            })
            .fold(0.0, f32::max)
    };
    let mut systems: Vec<SystemPlan> = Vec::new();
    let mut skipped: Vec<EngravingDecision> = Vec::new();
    // The systems each region's breaks fix that hold more than the line.
    let mut fixed: Vec<Fixed> = Vec::with_capacity(region_slots.len());
    let none = BTreeSet::new();
    for (r, infos) in region_slots.iter().enumerate() {
        let region_source = input.regions[r].provenance.source;
        let bounds = Bounds {
            width_limit: width_limit - edge,
            left: staff_left[r],
            lead: lead_max[r],
            room: &room[r],
            held: &none,
        };
        let region_fixed = fixed_systems(infos, &reqs, &bounds, respacing.skyline, &|slot| {
            opening_lead(r, slot)
        });
        walk_region(
            r,
            infos,
            &reqs,
            &origins,
            region_source,
            Bounds {
                held: &region_fixed.held,
                ..bounds
            },
            &mut systems,
            &mut skipped,
        );
        fixed.push(region_fixed);
    }
    // Which systems are one the breaks fix and the search kept whole, and
    // which the search added inside one whose floor overflows, each named by
    // the measure it opens with.
    let mut held_system = vec![false; systems.len()];
    let mut added_systems: Vec<AddedSystem> = Vec::new();
    for (s, plan) in systems.iter().enumerate() {
        let (Some(&first), Some(&last)) = (plan.slots.first(), plan.slots.last()) else {
            continue;
        };
        let region = &fixed[plan.region];
        held_system[s] = region.held.contains(&(first, last + 1));
        let slots = &region_slots[plan.region];
        if let Some(&(_, _, overflow)) = region
            .overflowing
            .iter()
            .find(|(lo, hi, _)| *lo < first && first < *hi)
        {
            added_systems.push(AddedSystem {
                overflow,
                opens: plan
                    .slots
                    .iter()
                    .find_map(|&i| slots[i].measure_barline)
                    .map(|g| input.glyphs[g].provenance.source),
            });
        }
    }

    // What each system draws at its start: nothing for a region's first,
    // whose lead the projection drew; for a later one, each staff's lead in
    // force at its first slot's time.
    let system_leads: Vec<Vec<(&SystemLead, &[LeadGlyph])>> = systems
        .iter()
        .map(|plan| {
            let Some(time) = plan
                .slots
                .first()
                .and_then(|&i| slot_time.get(&region_slots[plan.region][i].id))
                .filter(|_| plan.local > 0)
            else {
                return Vec::new();
            };
            leads_of[plan.region]
                .iter()
                .filter_map(|lead| {
                    lead.entries
                        .iter()
                        .rev()
                        .find(|(from, _)| at_or_before(from, time))
                        .or(lead.entries.first())
                        .map(|(_, glyphs)| (*lead, glyphs.as_slice()))
                })
                .collect()
        })
        .collect();
    // Each system's lead width: its widest staff's lead and gap, and the room
    // a tie continued into the system needs there.
    let lead_w: Vec<f32> = system_leads
        .iter()
        .zip(&systems)
        .map(|(leads, plan)| {
            let extra = match plan.slots.first() {
                Some(&i) if plan.local > 0 => room[plan.region][i],
                _ => 0.0,
            };
            leads
                .iter()
                .map(|(_, glyphs)| lead_width(glyphs))
                .fold(0.0, f32::max)
                + extra
        })
        .collect();
    // The columns a later system opens with before its first note or rest (a
    // time signature), each drawn back by the room its system's lead made, so
    // it keeps its place after the lead.
    let mut opening_shift: BTreeMap<SpringSlotId, f32> = BTreeMap::new();
    for plan in &systems {
        let slots = &region_slots[plan.region];
        if let Some(&first) = plan.slots.first().filter(|_| plan.local > 0) {
            let shift = room[plan.region][first];
            for &i in plan.slots.iter().take_while(|&&i| !slots[i].note) {
                if shift > 0.0 {
                    opening_shift.insert(slots[i].id, shift);
                }
            }
        }
    }
    let shift_of = |slot: SpringSlotId| opening_shift.get(&slot).copied().unwrap_or(0.0);
    // The right edge of each later system's lead ink, the widest staff's, from
    // the left margin.
    let lead_ink: Vec<Option<f32>> = system_leads
        .iter()
        .map(|leads| {
            leads
                .iter()
                .flat_map(|(_, glyphs)| glyphs.iter())
                .filter_map(|g| Some(g.x + metrics(g.name.as_str())?.bounding_box().right.0))
                .reduce(f32::max)
        })
        .collect();

    // Where each system's staff lines end: the right edge of the barline that
    // closes it, a final barline's thick line included, when one does.
    let staff_end: Vec<Option<f32>> = systems
        .iter()
        .map(|plan| {
            let slots = &region_slots[plan.region];
            let &last = plan.slots.last()?;
            slots[last].barline.then(|| {
                plan.slots
                    .iter()
                    .map(|&i| slots[i].hi)
                    .fold(f32::NEG_INFINITY, f32::max)
            })
        })
        .collect();

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
    let anchored_system = |id: GlyphObjectId| -> Option<usize> {
        let (start, end) = anchors.get(&id)?;
        let s = *system_of_slot.get(start)?;
        (system_of_slot.get(end) == Some(&s)).then_some(s)
    };
    let fates: Vec<StrokeFate> = input
        .strokes
        .iter()
        .zip(spaced_strokes)
        .map(|(stroke, spaced)| {
            // A stroke anchored at both ends within one system rides it.
            if let Some(s) = anchored_system(stroke.id()) {
                return StrokeFate::Rigid(Some(s));
            }
            // One anchored in two systems (a hairpin, hyphen or melisma line
            // over a break) splits where they break: to its first system's
            // end, through any between, and from its last system's start.
            if let Some((a, b)) = anchors.get(&stroke.id()).and_then(|(start, end)| {
                Some((*system_of_slot.get(start)?, *system_of_slot.get(end)?))
            }) {
                if a < b {
                    let (from, to) = (spaced.from, spaced.to);
                    let mut segments = vec![(a, from, Point::new(clips[a].1, from.y.0))];
                    segments.extend((a + 1..b).map(|s| {
                        (
                            s,
                            Point::new(clips[s].0, from.y.0),
                            Point::new(clips[s].1, to.y.0),
                        )
                    }));
                    segments.push((b, Point::new(clips[b].0, to.y.0), to));
                    return StrokeFate::Split(segments);
                }
            }
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
    // An ending whose barline closes a system while its line goes on into the
    // next is drawn wholly on the next, as MuseScore lays a spanner out only
    // on a system its start precedes the end of (`SystemLayout`): each such
    // ending's system, and each part of its start (its hook, numbers and
    // line) by id. Its hook rides that system, and its line's piece at the
    // earlier system's end, which would hold only its start, is dropped.
    let mut ending_to: BTreeMap<usize, usize> = BTreeMap::new();
    for (e, ending) in input.endings.iter().enumerate() {
        let Some(&a) = system_of_slot.get(&ending.barline) else {
            continue;
        };
        let plan = &systems[a];
        let closes = plan
            .slots
            .last()
            .is_some_and(|&i| region_slots[plan.region][i].id == ending.barline);
        let goes_on = anchors
            .get(&ending.line)
            .and_then(|(_, end)| system_of_slot.get(end))
            .is_some_and(|&b| b > a);
        if closes
            && goes_on
            && systems
                .get(a + 1)
                .is_some_and(|next| next.region == plan.region)
        {
            ending_to.insert(e, a + 1);
        }
    }
    let ending_of: BTreeMap<GlyphObjectId, usize> = ending_to
        .keys()
        .flat_map(|&e| {
            let ending = &input.endings[e];
            ending
                .numbers
                .iter()
                .chain([&ending.hook, &ending.line])
                .map(move |id| (*id, e))
        })
        .collect();
    let moved_hooks: BTreeSet<GlyphObjectId> =
        ending_to.keys().map(|&e| input.endings[e].hook).collect();
    let mut fates = fates;
    for (si, stroke) in input.strokes.iter().enumerate() {
        let Some(e) = ending_of.get(&stroke.id()) else {
            continue;
        };
        let to = ending_to[e];
        fates[si] = match &fates[si] {
            StrokeFate::Split(segments) => StrokeFate::Split(
                segments
                    .iter()
                    .filter(|(s, _, _)| *s >= to)
                    .cloned()
                    .collect(),
            ),
            _ => StrokeFate::Rigid(Some(to)),
        };
    }
    // A curve rides one system whole when it fits within one, or splits into
    // per-system sub-cubics (de Casteljau) when it spans a break — the same
    // nearest-region / clip-overlap logic strokes use.
    // An anchored curve whose ends land in two systems (a tie across a system
    // break) becomes two half-arcs: one from its start to the first system's
    // right edge, one from clear of the second system's lead to its end. Its
    // ends at the notes keep riding their slots (`anchored_halves`).
    let mut anchored_halves: BTreeSet<usize> = BTreeSet::new();
    let curve_fates: Vec<CurveFate> = spaced_curves
        .iter()
        .enumerate()
        .map(|(ci, curve)| {
            if let Some(s) = anchored_system(curve.id()) {
                return CurveFate::Rigid(Some(s));
            }
            let halves = anchors.get(&curve.id()).and_then(|(start, end)| {
                let (a, b) = (*system_of_slot.get(start)?, *system_of_slot.get(end)?);
                let (edge_a, edge_b) = (clips[a].1, clips[b].0);
                (a < b && edge_a.is_finite() && edge_b.is_finite()).then(|| {
                    // The spaced frame has no lead: its ink ends as far before
                    // the system's left edge as the lead's gap reaches.
                    let lead = edge_b - lead_w[b] + lead_ink[b].unwrap_or(0.0);
                    let plan = &systems[b];
                    let before = ink_before(plan, &region_slots[plan.region], *end, |info| {
                        info.hi - shift_of(info.id)
                    });
                    let from = lead.max(before.unwrap_or(f32::NEG_INFINITY)) + TIE_LEAD_CLEARANCE;
                    (a, b, half_arcs(curve.control_points(), edge_a, from))
                })
            });
            match halves {
                Some((a, b, (first, second))) => {
                    anchored_halves.insert(ci);
                    CurveFate::Split(vec![(a, first), (b, second)])
                }
                None => curve_fate(curve, &region_spans, &region_systems, &clips),
            }
        })
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
    let text_staff_of: Vec<Option<StaffId>> = input
        .texts
        .iter()
        .map(|t| staff_of(t.vertical_band))
        .collect();
    let text_system: Vec<Option<usize>> = input
        .texts
        .iter()
        .map(|t| system_of_slot.get(&t.horizontal_slot).copied())
        .collect();

    // The ink of each beam across two staves stands between them and is
    // neither staff's content: the solve keeps the lower staff from rising
    // past its `rise_limit` instead, in the system the beam lands in.
    let between: BTreeSet<GlyphObjectId> = input
        .cross_staff_beams
        .iter()
        .flat_map(|beam| beam.ink.iter().copied())
        .collect();
    let stroke_index: BTreeMap<GlyphObjectId, usize> = input
        .strokes
        .iter()
        .enumerate()
        .map(|(i, stroke)| (stroke.id(), i))
        .collect();
    // Per system, each (upper, lower) pair's least downward shift of the
    // lower staff relative to the upper.
    let mut floors: BTreeMap<(usize, StaffId, StaffId), f32> = BTreeMap::new();
    for beam in &input.cross_staff_beams {
        let system = beam
            .ink
            .iter()
            .find_map(|id| match fates.get(*stroke_index.get(id)?) {
                Some(StrokeFate::Rigid(Some(s))) => Some(*s),
                _ => None,
            });
        if let Some(s) = system {
            let floor = floors
                .entry((s, beam.upper, beam.lower))
                .or_insert(f32::NEG_INFINITY);
            *floor = floor.max(-beam.rise_limit);
        }
    }
    // Each stem reaching such a beam from the lower staff, by the staff its
    // beam end rides.
    let reach_upper: BTreeMap<GlyphObjectId, StaffId> = input
        .cross_staff_beams
        .iter()
        .flat_map(|beam| beam.reaching.iter().map(|id| (*id, beam.upper)))
        .collect();

    // Pass A: system extents (unshifted), and per (system, staff) content
    // y-extents plus the staff-line reference y (for ordering).
    let mut extents: Vec<Extent> = vec![Extent::empty(); systems.len()];
    let mut staff_ext: BTreeMap<(usize, StaffId), (f32, f32)> = BTreeMap::new();
    let mut staff_ref: BTreeMap<(usize, StaffId), f32> = BTreeMap::new();
    // Each staff's outer lines in each system, their centres, bottom and top:
    // the floor between two staves stands between their facing lines.
    let mut staff_lines: BTreeMap<(usize, StaffId), (f32, f32)> = BTreeMap::new();
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
    // The family a solver places off its staves joins them below, once
    // placed; the rest of the family's ink takes height on its staff and no
    // room across.
    let placed = crate::family::Placed::of(input);
    for (s, plan) in systems.iter().enumerate() {
        for &i in &plan.slots {
            for &g in &region_slots[plan.region][i].members {
                let id = input.glyphs[g].id();
                if placed.glyphs.contains_key(&id) {
                    continue;
                }
                // A moved ending's numbers stand on its system, where their
                // place across is not yet known: they take its height alone.
                let moved = ending_of.get(&id).map(|e| ending_to[e]);
                let s = moved.unwrap_or(s);
                let glyph = &spaced_glyphs[g];
                let (x, y) = (glyph.position.x.0, glyph.position.y.0);
                let (lo_y, hi_y) = (
                    y + glyph.bounding_box.bottom.0,
                    y + glyph.bounding_box.top.0,
                );
                if !roomless.contains(&id) && moved.is_none() {
                    extents[s].add_x(
                        x + glyph.bounding_box.left.0,
                        x + glyph.bounding_box.right.0,
                    );
                }
                if between.contains(&input.glyphs[g].id()) {
                    continue;
                }
                match glyph_staff_of[g] {
                    Some(_) => into_staff(&mut staff_ext, s, glyph_staff_of[g], lo_y, hi_y),
                    None => extents[s].add_y(lo_y, hi_y),
                }
            }
        }
    }
    for (si, (fate, spaced)) in fates.iter().zip(spaced_strokes).enumerate() {
        if placed.strokes.contains_key(&spaced.id()) {
            continue;
        }
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
            // A staff line ends with its system's closing barline.
            let right = match staff_end[s] {
                Some(end) if is_staff_line => end,
                _ => to.x.0,
            };
            if !moved_hooks.contains(&spaced.id()) {
                extents[s].add_x(from.x.0 - half, right + half);
            }
            if between.contains(&spaced.id()) {
                continue;
            }
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
                    let y = from.y.0;
                    staff_lines
                        .entry((s, st))
                        .and_modify(|(lo, hi)| {
                            *lo = lo.min(y);
                            *hi = hi.max(y);
                        })
                        .or_insert((y, y));
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
            // A tie's second half reaches back over its system's lead, which is
            // placed before the system's content, not within it.
            let left = if anchored_halves.contains(&ci) {
                clips[s].0
            } else {
                f32::NEG_INFINITY
            };
            for p in cp {
                let x = p.x.0.max(left);
                extents[s].add_x(x - half, x + half);
                match staff {
                    Some(_) => into_staff(&mut staff_ext, s, staff, p.y.0 - half, p.y.0 + half),
                    None => extents[s].add_y(p.y.0 - half, p.y.0 + half),
                }
            }
        }
    }

    // A text run's ink is its staff's content, by height alone: a run takes
    // no room across, beside the slot it rides.
    for (ti, text) in spaced_texts.iter().enumerate() {
        let Some(s) = text_system[ti].filter(|_| !placed.texts.contains_key(&text.id())) else {
            continue;
        };
        if between.contains(&text.id()) {
            continue;
        }
        let (y, b) = (text.origin.y.0, text.bounds);
        match text_staff_of[ti] {
            Some(_) => into_staff(
                &mut staff_ext,
                s,
                text_staff_of[ti],
                y + b.bottom.0,
                y + b.top.0,
            ),
            None => extents[s].add_y(y + b.bottom.0, y + b.top.0),
        }
    }

    // ---- The family placed off its staves (X4c) -----------------------------
    // Each system's map across depends on its extent across alone, so it is
    // set here, before the family a solver places off its staves is placed
    // against each staff's ink where the system sets it (`family::place`);
    // that ink then joins its staff's extent, so the gap to the next staff
    // makes room for it.
    let springs: BTreeMap<SpringSlotId, f32> = input
        .horizontal_slots
        .iter()
        .map(|slot| (slot.id, slot.stretch_factor * slot.preferred_width.0))
        .collect();
    let hplace: Vec<(Placement, bool)> = systems
        .iter()
        .enumerate()
        .map(|(s, plan)| {
            let ext = extents[s].normalized();
            // A later system's content starts after its lead.
            let base_dx = geometry.margins.left.0 + lead_w[s] - ext.min_x;
            let width = width_limit - lead_w[s];
            let squeezed = held_system[s]
                .then(|| {
                    compress_system(
                        plan,
                        &ext,
                        base_dx,
                        0.0,
                        &region_slots,
                        respacing.skyline,
                        width,
                    )
                })
                .flatten();
            let compressed = squeezed.is_some();
            let placement = squeezed.unwrap_or_else(|| {
                justify_system(
                    plan,
                    &ext,
                    base_dx,
                    0.0,
                    &region_slots,
                    &region_systems,
                    &springs,
                    width,
                )
            });
            (placement, compressed)
        })
        .collect();
    // ---- Measure rests (X5d.5) ---------------------------------------------
    // A rest filling its measure stands centred in its staff's free space in
    // the measure, as Gould sets a whole-bar rest (pp. 159-160): between the
    // right edge of its staff's ink in the columns before it that hold no
    // note, back to the barline opening the measure (or the system's lead
    // where the measure opens the system), and the left edge of its staff's
    // ink in the first such column after it, a clef, key or time signature or
    // the barline closing the measure, as MuseScore takes them
    // (`MeasureLayout::getMeasureStartEndPos`). Its column keeps its room.
    // This needs the system's map across alone, so it is set here, and the
    // sky, the family and the outlines read the rest where it is drawn. What
    // stands on it goes with it (`MeasureRest`): its fermata, by `family_dx`,
    // and a slur's end, by `curve_carry`.
    let glyph_index: BTreeMap<GlyphObjectId, usize> = input
        .glyphs
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id(), i))
        .collect();
    let placed_x = |g: usize, s: usize| -> f32 {
        let glyph = &input.glyphs[g];
        let spaced = spaced_glyphs[g].position.x.0;
        let sx = slot_source_x
            .get(&glyph.horizontal_slot)
            .copied()
            .unwrap_or(spaced);
        spaced + hplace[s].0.slot_dx(sx) - shift_of(glyph.horizontal_slot)
    };
    let text_at: BTreeMap<GlyphObjectId, usize> = input
        .texts
        .iter()
        .enumerate()
        .map(|(i, text)| (text.id(), i))
        .collect();
    let mut rest_dx: BTreeMap<usize, f32> = BTreeMap::new();
    let mut carried_marks: Vec<(usize, f32)> = Vec::new();
    // The move across of a slur's first and last ends, each to its rest's
    // centre, from where the system's map sets it.
    let mut curve_carry: BTreeMap<GlyphObjectId, (f32, f32)> = BTreeMap::new();
    let curve_index: BTreeMap<GlyphObjectId, usize> = spaced_curves
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id(), i))
        .collect();
    // A curve's pieces where the system's map sets them, the staff's and the
    // system's height aside, as the sky and the bake read them.
    let curve_pieces = |ci: usize| -> Vec<(usize, [Point; 4])> {
        let curve = &spaced_curves[ci];
        match &curve_fates[ci] {
            CurveFate::Rigid(Some(s)) => {
                let p = &hplace[*s].0;
                let slot_dx =
                    |slot: &SpringSlotId| slot_source_x.get(slot).map(|&sx| p.slot_dx(sx));
                let cp = match anchors
                    .get(&curve.id())
                    .and_then(|(start, end)| Some((slot_dx(start)?, slot_dx(end)?)))
                {
                    Some((start, end)) => crate::anchored_curve(curve.control_points(), start, end),
                    None => curve
                        .control_points()
                        .map(|pt| Point::new(p.x(pt.x.0), pt.y.0)),
                };
                vec![(*s, cp)]
            }
            CurveFate::Rigid(None) => Vec::new(),
            CurveFate::Split(segments) => segments
                .iter()
                .map(|(s, cp)| {
                    let p = &hplace[*s].0;
                    (*s, cp.map(|pt| Point::new(p.x(pt.x.0), pt.y.0)))
                })
                .collect(),
        }
    };
    for rest in &input.measure_rests {
        let Some(&s) = system_of_slot.get(&rest.slot) else {
            continue;
        };
        let plan = &systems[s];
        let slots = &region_slots[plan.region];
        let Some(k) = plan.slots.iter().position(|&i| slots[i].id == rest.slot) else {
            continue;
        };
        // A column holding no note: its ink on the rest's staff across.
        let furniture = |i: usize| -> Option<(f32, f32)> {
            if slots[i].note {
                return None;
            }
            slots[i]
                .members
                .iter()
                .filter(|&&m| {
                    input.glyphs[m].vertical_band == rest.band
                        && !family.contains(&input.glyphs[m].id())
                })
                .map(|&m| {
                    let (x, b) = (placed_x(m, s), input.glyphs[m].bounding_box);
                    (x + b.left.0, x + b.right.0)
                })
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        };
        let mut left: Option<f32> = None;
        let mut opened = false;
        for &i in plan.slots[..k].iter().rev() {
            if let Some((_, r)) = furniture(i) {
                left = Some(left.map_or(r, |l| l.max(r)));
            }
            if slots[i].barline {
                opened = true;
                break;
            }
        }
        if !opened {
            let lead = geometry.margins.left.0 + lead_ink[s].unwrap_or(0.0);
            left = Some(left.map_or(lead, |l| l.max(lead)));
        }
        let right = plan.slots[k + 1..]
            .iter()
            .find_map(|&i| furniture(i))
            .map(|(l, _)| l);
        let (Some(left), Some(right)) = (left, right) else {
            continue;
        };
        if right - left <= rest.width {
            continue;
        }
        let centre = (left + right) * 0.5;
        let centre_of = |g: usize| {
            let b = input.glyphs[g].bounding_box;
            placed_x(g, s) + (b.left.0 + b.right.0) * 0.5
        };
        if let Some(&g) = rest.glyph.as_ref().and_then(|id| glyph_index.get(id)) {
            rest_dx.insert(g, centre - centre_of(g));
        }
        for id in &rest.marks {
            if let (Some(&g), Some(&f)) = (glyph_index.get(id), placed.glyphs.get(id)) {
                carried_marks.push((f, centre - centre_of(g)));
            }
        }
        for id in &rest.runs {
            let (Some(&ti), Some(&f)) = (text_at.get(id), placed.texts.get(id)) else {
                continue;
            };
            let dx = match rest.glyph.as_ref().and_then(|id| glyph_index.get(id)) {
                // A syllable keeps its place about its rest; on a hidden rest,
                // which has none, it centres as its rest would.
                Some(&g) => centre - centre_of(g),
                None => {
                    let (text, spaced) = (&input.texts[ti], &spaced_texts[ti]);
                    let sx = slot_source_x
                        .get(&text.horizontal_slot)
                        .copied()
                        .unwrap_or(spaced.origin.x.0);
                    centre
                        - (spaced.origin.x.0 + hplace[s].0.slot_dx(sx)
                            - shift_of(text.horizontal_slot)
                            + (spaced.bounds.left.0 + spaced.bounds.right.0) * 0.5)
                }
            };
            carried_marks.push((f, dx));
        }
        for (ids, first) in [(&rest.slur_starts, true), (&rest.slur_ends, false)] {
            for id in ids {
                let Some(&ci) = curve_index.get(id) else {
                    continue;
                };
                let pieces = curve_pieces(ci);
                let at = if first {
                    pieces.first().map(|(_, cp)| cp[0].x.0)
                } else {
                    pieces.last().map(|(_, cp)| cp[3].x.0)
                };
                let Some(x) = at else {
                    continue;
                };
                let carry = curve_carry.entry(*id).or_insert((0.0, 0.0));
                if first {
                    carry.0 = centre - x;
                } else {
                    carry.1 = centre - x;
                }
            }
        }
    }
    // A slur's pieces with their ends carried: `k` of `n`, sheared as an
    // anchored curve's ends are.
    let carried_curve = |id: GlyphObjectId, k: usize, n: usize, cp: [Point; 4]| -> [Point; 4] {
        match curve_carry.get(&id) {
            Some(&(start, end)) => crate::anchored_curve(
                cp,
                if k == 0 { start } else { 0.0 },
                if k + 1 == n { end } else { 0.0 },
            ),
            None => cp,
        }
    };

    // Each line of the family (a hairpin's leg, a hyphen, a melisma line, a
    // trill line's wiggles), by its stroke.
    let family_lines: BTreeMap<GlyphObjectId, FamilyLine> = input
        .family
        .iter()
        .filter(|mark| mark.line.is_some())
        .flat_map(|mark| {
            mark.strokes
                .iter()
                .map(move |id| (*id, mark.line.expect("a line")))
        })
        .collect();
    // Each line's runs drawn piece by piece (X4d.1), by its stroke: a
    // continuation, a glissando's words.
    let mut line_texts: BTreeMap<GlyphObjectId, Vec<&LineText>> = BTreeMap::new();
    for text in &input.line_texts {
        line_texts.entry(text.line).or_default().push(text);
    }
    let continuation_of = |id: GlyphObjectId| -> Option<(&ResolvedText, f32)> {
        line_texts.get(&id)?.iter().find_map(|t| match t.role {
            LineTextRole::Continuation { gap } => Some((&t.run, gap)),
            LineTextRole::Along { .. } => None,
        })
    };
    let words_of = |id: GlyphObjectId| -> Option<(&ResolvedText, f32)> {
        line_texts.get(&id)?.iter().find_map(|t| match t.role {
            LineTextRole::Along { raise, .. } => Some((&t.run, raise - t.run.bounds.bottom.0)),
            LineTextRole::Continuation { .. } => None,
        })
    };
    // A glissando's parallel set and its rank there (X4d fix round 2).
    let words_set = |id: GlyphObjectId| -> Option<(u128, usize)> {
        line_texts.get(&id)?.iter().find_map(|t| match t.role {
            LineTextRole::Along { set, rank, .. } => Some((set, rank)),
            LineTextRole::Continuation { .. } => None,
        })
    };
    // The slot each extension line's strokes may not pass, and its ends held
    // short of that slot's column where it stands in their system (X4d.1:
    // an ottava's corner never passes the next note).
    let line_before: BTreeMap<GlyphObjectId, SpringSlotId> = input
        .family
        .iter()
        .filter_map(|mark| match mark.line {
            Some(FamilyLine::Extension {
                before: Some(slot), ..
            }) => Some((&mark.strokes, slot)),
            _ => None,
        })
        .flat_map(|(ids, slot)| ids.iter().map(move |id| (*id, slot)))
        .collect();
    let clamp_before = |id: GlyphObjectId,
                        s: usize,
                        p: &Placement,
                        from: Point,
                        to: Point,
                        half: f32|
     -> (Point, Point) {
        let limit = line_before
            .get(&id)
            .filter(|slot| system_of_slot.get(slot) == Some(&s))
            .and_then(|slot| slot_source_x.get(slot))
            .map(|&sx| p.x(sx));
        match limit {
            // A hook stands half its width short of it, a line's end on it.
            Some(limit) if from.x == to.x => {
                let x = from.x.0.min(limit - half);
                (Point::new(x, from.y.0), Point::new(x, to.y.0))
            }
            Some(limit) => {
                let x = to.x.0.min(limit).max(from.x.0);
                (from, Point::new(x, to.y.0))
            }
            None => (from, to),
        }
    };
    // A glissando's pieces' heights where its system or systems set it.
    let glissando_slope =
        |source: &Stroke, _spaced: &Stroke, xs: &[(f32, f32)]| -> Option<Vec<(f32, f32)>> {
            match family_lines.get(&source.id()) {
                Some(FamilyLine::Glissando { heads, .. }) => {
                    Some(glissando_heights(*heads, source, xs))
                }
                _ => None,
            }
        };
    // How far across each moved ending's start moves from where the spacing
    // set it, so its hook stands `BAR_NOTE_DISTANCE` before its system's
    // first note or rest, as MuseScore starts an ending opening a system
    // (`Volta::linePos`), its line's half width further on; and so, for any
    // part of it, the system it stands on and that shift.
    let ending_dx: BTreeMap<usize, f32> = ending_to
        .iter()
        .filter_map(|(&e, &t)| {
            let plan = &systems[t];
            let first = plan
                .slots
                .iter()
                .map(|&i| &region_slots[plan.region][i])
                .find(|info| info.note)?;
            let left = first
                .members
                .iter()
                .filter(|&&g| {
                    let name = input.glyphs[g].glyph.as_str();
                    name.starts_with("notehead") || name.starts_with("rest")
                })
                .map(|&g| spaced_glyphs[g].position.x.0 + spaced_glyphs[g].bounding_box.left.0)
                .reduce(f32::min)?;
            let x = left + hplace[t].0.slot_dx(first.x) - shift_of(first.id);
            let hook = input
                .strokes
                .iter()
                .position(|stroke| stroke.id() == input.endings[e].hook)?;
            let hook = &spaced_strokes[hook];
            Some((
                e,
                x - BAR_NOTE_DISTANCE + hook.thickness.0 * 0.5 - hook.from.x.0,
            ))
        })
        .collect();
    let ending_shift = |id: GlyphObjectId| -> Option<(usize, f32)> {
        let e = ending_of.get(&id)?;
        Some((ending_to[e], *ending_dx.get(e)?))
    };
    // Where a split stroke's piece stands across in its system, in the
    // family's placing as in its drawing.
    // A later piece of a family line opens at its system's first column; a
    // glissando's, which ends at its chord's ink, a quarter space after the
    // ink of the system's lead, as MuseScore opens a glissando's later
    // segment before the first column's leading ink
    // (`System::firstNoteRestSegmentX`), and draws nothing where its chord's
    // ink leaves it no room.
    let piece_x =
        |source: &Stroke, k: usize, n: usize, s: usize, p: &Placement, from: f32, to: f32| {
            let glissando = matches!(
                family_lines.get(&source.id()),
                Some(FamilyLine::Glissando { .. })
            );
            let (a, b) = split_piece_x(
                k,
                n,
                (from, to),
                p,
                anchors.get(&source.id()),
                &slot_source_x,
                family_lines.contains_key(&source.id()).then(|| {
                    if glissando {
                        let lead = if lead_w[s] > 0.0 {
                            geometry.margins.left.0 + lead_w[s] - LEAD_GAP
                        } else {
                            geometry.margins.left.0
                        };
                        return lead + GLISSANDO_LEAD_GAP;
                    }
                    let plan = &systems[s];
                    plan.slots
                        .first()
                        .map_or(geometry.margins.left.0 + lead_w[s], |&i| {
                            let x = region_slots[plan.region][i].x;
                            x + p.slot_dx(x)
                        })
                }),
            );
            if glissando && k > 0 {
                (a, b.max(a))
            } else {
                (a, b)
            }
        };
    // Each member standing at its measure's start, set across once its
    // system is: centred on its barline's ink there, on its right edge for a
    // rehearsal mark (where the measure starts) and on its middle for a
    // segno or coda; or, where the measure opens its system, just after the
    // ink of the system's lead and of the columns before the member's, or on
    // the system's left edge.
    let mut family_dx: BTreeMap<usize, f32> = carried_marks.iter().copied().collect();
    for (f, mark) in input.family.iter().enumerate() {
        let Some(across) = mark.placement.and_then(|p| p.across) else {
            continue;
        };
        // Its runs' ink across where their slot sets them, and their slot.
        let mut ink: Option<(usize, SpringSlotId, f32, f32)> = None;
        for i in mark.texts.iter().filter_map(|id| text_at.get(id)) {
            let (text, spaced) = (&input.texts[*i], &spaced_texts[*i]);
            let Some(s) = text_system[*i] else {
                continue;
            };
            let slot = text.horizontal_slot;
            let sx = slot_source_x
                .get(&slot)
                .copied()
                .unwrap_or(spaced.origin.x.0);
            let x = spaced.origin.x.0 + hplace[s].0.slot_dx(sx) - shift_of(slot);
            let (l, r) = (x + spaced.bounds.left.0, x + spaced.bounds.right.0);
            ink = Some(match ink {
                Some((s, slot, a, b)) => (s, slot, a.min(l), b.max(r)),
                None => (s, slot, l, r),
            });
        }
        let Some((s, slot, l, r)) = ink else {
            continue;
        };
        // The ink of the glyphs in this system's slots matching `pick`.
        let glyph_ink = |pick: &dyn Fn(SpringSlotId) -> bool| {
            input
                .glyphs
                .iter()
                .zip(spaced_glyphs)
                .filter(|(g, _)| {
                    system_of_slot.get(&g.horizontal_slot) == Some(&s) && pick(g.horizontal_slot)
                })
                .map(|(g, spaced)| {
                    let sx = slot_source_x
                        .get(&g.horizontal_slot)
                        .copied()
                        .unwrap_or(spaced.position.x.0);
                    let x =
                        spaced.position.x.0 + hplace[s].0.slot_dx(sx) - shift_of(g.horizontal_slot);
                    (x + g.bounding_box.left.0, x + g.bounding_box.right.0)
                })
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        };
        let barline = across
            .barline
            .filter(|b| system_of_slot.get(b) == Some(&s))
            .and_then(|b| glyph_ink(&|slot| slot == b));
        let target = match (barline, across.kind) {
            (Some((_, right)), MeasureMark::Rehearsal) => right,
            (Some((left, right)), MeasureMark::Repeat) => (left + right) * 0.5,
            (None, MeasureMark::Rehearsal) => {
                let lead = if lead_w[s] > 0.0 {
                    geometry.margins.left.0 + lead_w[s] - LEAD_GAP
                } else {
                    geometry.margins.left.0
                };
                let own = slot_source_x.get(&slot).copied().unwrap_or(f32::INFINITY);
                let before =
                    glyph_ink(&|other| slot_source_x.get(&other).is_some_and(|&x| x < own));
                before.map_or(lead, |(_, right)| lead.max(right))
            }
            (None, MeasureMark::Repeat) => geometry.margins.left.0,
        };
        family_dx.insert(f, target - (l + r) * 0.5);
    }
    // A dynamic moved across a quarter space clear of a barline that runs
    // through the staves at it, as MuseScore moves one whose offset no user
    // changed (`DynamicsLayout::manageBarlineCollisions`; X4d.3), every
    // dynamic here, the model keeping no author's offset: under its staff,
    // its own staff's barline where it runs on to the next staff; over it,
    // the barline of the staff above where that one runs on to it. Away from
    // the barline after it, then from the one before it; Gould keeps barlines
    // whole by moving a dynamic off them, an initial one to the barline's
    // right (*Behind Bars*, p. 103).
    // Each staff whose barline runs on to the next staff of a joined group,
    // and the staff above each other staff of one.
    let joined_down: BTreeSet<StaffId> = input
        .staff_groups
        .iter()
        .filter(|g| g.joined)
        .flat_map(|g| g.staves.iter().rev().skip(1).copied())
        .collect();
    let staff_above: BTreeMap<StaffId, StaffId> = input
        .staff_groups
        .iter()
        .filter(|g| g.joined)
        .flat_map(|g| {
            g.staves
                .windows(2)
                .map(|w| (w[1], w[0]))
                .collect::<Vec<_>>()
        })
        .collect();
    let staff_band: BTreeMap<StaffId, VerticalBandId> = band_to_staff
        .iter()
        .map(|(band, staff)| (*staff, *band))
        .collect();
    // Each hairpin leg whose start or end moves with a dynamic moved off a
    // barline: by its stroke, the move of its start and of its end.
    let mut hairpin_ends: BTreeMap<GlyphObjectId, (f32, f32)> = BTreeMap::new();
    // So does an ottava's end whose last ink is a rest filling its measure,
    // a space past the rest where it is drawn (`Ottava::linePos`), never past
    // the next note, its line and its end's hook moved alike.
    for rest in &input.measure_rests {
        let (Some(&g), Some(&s)) = (
            rest.glyph.as_ref().and_then(|id| glyph_index.get(id)),
            system_of_slot.get(&rest.slot),
        ) else {
            continue;
        };
        let Some(&dx) = rest_dx.get(&g) else {
            continue;
        };
        let Some(body) = rest.line_ends.first().and_then(|id| stroke_index.get(id)) else {
            continue;
        };
        let (source, spaced) = (&input.strokes[*body], &spaced_strokes[*body]);
        let p = &hplace[s].0;
        let end = place_stroke(
            source,
            spaced,
            p,
            &slot_source_x,
            &input.glyphs,
            anchors.get(&source.id()),
        )
        .to
        .x
        .0;
        let limit = line_before
            .get(&source.id())
            .filter(|slot| system_of_slot.get(slot) == Some(&s))
            .and_then(|slot| slot_source_x.get(slot))
            .map_or(f32::INFINITY, |&sx| p.x(sx));
        let dx = dx.min(limit - end);
        for (k, id) in rest.line_ends.iter().enumerate() {
            hairpin_ends.insert(*id, (if k == 0 { 0.0 } else { dx }, dx));
        }
    }
    // Each verbal crescendo's words moved with its start (X4d fix round 1).
    let mut verbal_words_dx: BTreeMap<GlyphObjectId, f32> = BTreeMap::new();
    for (f, mark) in input.family.iter().enumerate() {
        let Some(p) = mark.placement else {
            continue;
        };
        if p.order != epiphany_layout_ir::PlaceOrder::Dynamic
            || mark.line.is_some()
            || family_dx.contains_key(&f)
        {
            continue;
        }
        let Some(staff) = staff_of(mark.band) else {
            continue;
        };
        let barline_staff = if p.above {
            staff_above.get(&staff).copied()
        } else {
            Some(staff)
        };
        let Some(barline_staff) = barline_staff.filter(|s| joined_down.contains(s)) else {
            continue;
        };
        let barline_band = staff_band.get(&barline_staff).copied();
        // Its ink across where its system sets it, its slot, and its system.
        let mut ink: Option<(usize, SpringSlotId, f32, f32)> = None;
        let mut take = |s: usize, slot: SpringSlotId, l: f32, r: f32| {
            ink = Some(match ink {
                Some((s0, slot0, a, b)) => (s0, slot0, a.min(l), b.max(r)),
                None => (s, slot, l, r),
            });
        };
        for (glyph, spaced) in input.glyphs.iter().zip(spaced_glyphs) {
            if !mark.glyphs.contains(&glyph.id()) {
                continue;
            }
            let Some(&s) = system_of_slot.get(&glyph.horizontal_slot) else {
                continue;
            };
            let sx = slot_source_x
                .get(&glyph.horizontal_slot)
                .copied()
                .unwrap_or(spaced.position.x.0);
            let x = spaced.position.x.0 + hplace[s].0.slot_dx(sx) - shift_of(glyph.horizontal_slot);
            take(
                s,
                glyph.horizontal_slot,
                x + glyph.bounding_box.left.0,
                x + glyph.bounding_box.right.0,
            );
        }
        for i in mark.texts.iter().filter_map(|id| text_at.get(id)) {
            let (text, spaced) = (&input.texts[*i], &spaced_texts[*i]);
            let Some(s) = text_system[*i] else {
                continue;
            };
            let slot = text.horizontal_slot;
            let sx = slot_source_x
                .get(&slot)
                .copied()
                .unwrap_or(spaced.origin.x.0);
            let x = spaced.origin.x.0 + hplace[s].0.slot_dx(sx) - shift_of(slot);
            take(s, slot, x + spaced.bounds.left.0, x + spaced.bounds.right.0);
        }
        let Some((s, slot, left, right)) = ink else {
            continue;
        };
        let own = slot_source_x.get(&slot).copied().unwrap_or(left);
        // The barlines of the staff in this system: each one's ink across
        // and its slot's source x.
        let barlines: Vec<(f32, f32, f32)> = input
            .glyphs
            .iter()
            .zip(spaced_glyphs)
            .filter(|(g, _)| {
                is_barline_glyph(g.glyph.as_str())
                    && Some(g.vertical_band) == barline_band
                    && system_of_slot.get(&g.horizontal_slot) == Some(&s)
            })
            .filter_map(|(g, spaced)| {
                let sx = *slot_source_x.get(&g.horizontal_slot)?;
                let x = spaced.position.x.0 + hplace[s].0.slot_dx(sx) - shift_of(g.horizontal_slot);
                Some((sx, x + g.bounding_box.left.0, x + g.bounding_box.right.0))
            })
            .collect();
        let mut dx = 0.0f32;
        if let Some(&(_, bar_left, _)) = barlines
            .iter()
            .filter(|(sx, ..)| *sx > own)
            .min_by(|a, b| a.0.total_cmp(&b.0))
        {
            let margin = bar_left - right - DYNAMIC_BARLINE_CLEARANCE;
            if margin < 0.0 {
                dx += margin;
            }
        }
        if let Some(&(_, _, bar_right)) = barlines
            .iter()
            .filter(|(sx, ..)| *sx <= own)
            .max_by(|a, b| a.0.total_cmp(&b.0))
        {
            let margin = left + dx - bar_right - DYNAMIC_BARLINE_CLEARANCE;
            if margin < 0.0 {
                dx -= margin;
            }
        }
        if dx != 0.0 {
            family_dx.insert(f, dx);
            // Expression text set after it in its row, at its note, moves
            // with it, as MuseScore lays out an expression from its
            // dynamic's moved place (`TLayout::layoutExpression`; X4d.6).
            if let Some(row) = p.row {
                for (g, other) in input.family.iter().enumerate() {
                    let follows = other.placement.is_some_and(|q| {
                        q.order == epiphany_layout_ir::PlaceOrder::Expression
                            && q.row.map(|r| r.id) == Some(row.id)
                    }) && other
                        .texts
                        .iter()
                        .filter_map(|id| text_at.get(id))
                        .any(|i| input.texts[*i].horizontal_slot == slot);
                    if follows {
                        family_dx.entry(g).or_insert(dx);
                    }
                    // A hairpin of its row starting at its note starts as far
                    // past it as before, and one ending there ends as far
                    // before it, as MuseScore lays a hairpin out from the
                    // dynamics it snaps to once they are moved.
                    let hairpin = matches!(other.line, Some(FamilyLine::Hairpin(_)))
                        && other
                            .placement
                            .is_some_and(|q| q.row.map(|r| r.id) == Some(row.id));
                    // So does a verbal crescendo of its row, its words with
                    // its start (X4d fix round 1).
                    let verbal = matches!(other.line, Some(FamilyLine::Extension { .. }))
                        && other.placement.is_some_and(|q| {
                            q.order == epiphany_layout_ir::PlaceOrder::Hairpin
                                && q.row.map(|r| r.id) == Some(row.id)
                        });
                    if !hairpin && !verbal {
                        continue;
                    }
                    let starts_here = other
                        .strokes
                        .iter()
                        .any(|id| anchors.get(id).is_some_and(|&(start, _)| start == slot));
                    if verbal && starts_here && dx > 0.0 {
                        for id in &other.texts {
                            let e = verbal_words_dx.entry(*id).or_insert(0.0);
                            *e = e.max(dx);
                        }
                    }
                    for id in &other.strokes {
                        let Some(&(start, end)) = anchors.get(id) else {
                            continue;
                        };
                        let e = hairpin_ends.entry(*id).or_insert((0.0, 0.0));
                        if start == slot && dx > 0.0 {
                            e.0 = e.0.max(dx);
                        }
                        if end == slot && dx < 0.0 {
                            e.1 = e.1.min(dx);
                        }
                    }
                }
            }
        }
    }
    // A stroke's ends moved as its hairpin's are, its left end by the start's
    // move and its right by the end's.
    let hairpin_moved = |id: GlyphObjectId, from: Point, to: Point| -> (Point, Point) {
        let Some(&(start, end)) = hairpin_ends.get(&id) else {
            return (from, to);
        };
        let (l, r) = if from.x.0 <= to.x.0 {
            (start, end)
        } else {
            (end, start)
        };
        (
            Point::new(from.x.0 + l, from.y.0),
            Point::new(to.x.0 + r, to.y.0),
        )
    };
    let member_dx = |f: Option<&usize>| f.and_then(|f| family_dx.get(f)).copied().unwrap_or(0.0);
    let mut family_dy: BTreeMap<(usize, usize), f32> = BTreeMap::new();
    // The placed family's pieces and the ink each was placed against, kept
    // for centring a grand staff's once its staves are set.
    // Every system's ink is gathered as rectangles by its declared band, in
    // the system's placed frame across: the sky the family is placed
    // against, and the outlines the staves of a system are set by.
    let family_pieces: Vec<crate::family::Piece>;
    let family_sky: BTreeMap<(usize, VerticalBandId), Vec<InkRect>>;
    // The family's pieces where they are placed, each with its member.
    let mut placed_boxes: Vec<(usize, usize, InkRect)> = Vec::new();
    // Each hairpin leg, by its stroke: its hairpin's shape, and which leg,
    // up 1 and down -1.
    let hairpin_legs: BTreeMap<GlyphObjectId, (HairpinShape, f32)> = input
        .family
        .iter()
        .filter_map(|mark| match mark.line {
            Some(FamilyLine::Hairpin(shape)) => Some((shape, &mark.strokes)),
            _ => None,
        })
        .filter(|(_, legs)| legs.len() == 2)
        .flat_map(|(shape, legs)| [(legs[0], (shape, 1.0)), (legs[1], (shape, -1.0))])
        .collect();
    // Each piece of a placed hairpin over a system break as the bake draws
    // it (`hairpin_piece_ys`), with its member and system: the piece is
    // placed by its flat cut, but the staves and the system are set by the
    // ink drawn (X5d.7).
    let mut drawn_legs: Vec<(usize, usize, InkRect)> = Vec::new();
    {
        let mut pieces: Vec<crate::family::Piece> = Vec::new();
        let mut piece_of: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut piece = |pieces: &mut Vec<crate::family::Piece>, family: usize, system: usize| {
            *piece_of.entry((family, system)).or_insert_with(|| {
                pieces.push(crate::family::Piece {
                    family,
                    system,
                    boxes: Vec::new(),
                });
                pieces.len() - 1
            })
        };
        let mut sky: BTreeMap<(usize, VerticalBandId), Vec<InkRect>> = BTreeMap::new();
        let rect = |l: f32, r: f32, b: f32, t: f32| InkRect {
            left: l.min(r),
            right: l.max(r),
            bottom: b.min(t),
            top: b.max(t),
        };
        // Glyphs, each moved with its slot, or a moved ending's start; a
        // member of the family moved across (a dynamic off a barline) by that
        // move as well.
        for (gi, (glyph, spaced)) in input.glyphs.iter().zip(spaced_glyphs).enumerate() {
            let Some(&s) = system_of_slot.get(&glyph.horizontal_slot) else {
                continue;
            };
            let sx = slot_source_x
                .get(&glyph.horizontal_slot)
                .copied()
                .unwrap_or(spaced.position.x.0);
            let (s, x) = match ending_shift(glyph.id()) {
                Some((t, dx)) => (t, spaced.position.x.0 + dx),
                None => (
                    s,
                    spaced.position.x.0 + hplace[s].0.slot_dx(sx) - shift_of(glyph.horizontal_slot)
                        + rest_dx.get(&gi).copied().unwrap_or(0.0)
                        + member_dx(placed.glyphs.get(&glyph.id())),
                ),
            };
            let (y, b) = (spaced.position.y.0, glyph.bounding_box);
            let ink = rect(x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0);
            match placed.glyphs.get(&glyph.id()) {
                Some(&f) => {
                    let i = piece(&mut pieces, f, s);
                    pieces[i].boxes.push(ink);
                }
                None if !between.contains(&glyph.id()) => {
                    sky.entry((s, glyph.vertical_band)).or_default().push(ink)
                }
                None => {}
            }
        }
        // Strokes, each where its system sets it; a traced anchor draws
        // nothing.
        for (si, ((stroke, spaced), fate)) in input
            .strokes
            .iter()
            .zip(spaced_strokes)
            .zip(&fates)
            .enumerate()
        {
            let _ = si;
            if stroke.from == stroke.to || between.contains(&stroke.id()) {
                continue;
            }
            let half = (spaced.thickness.0 * 0.5).max(0.0);
            let segments: Vec<(usize, Point, Point)> = match fate {
                StrokeFate::Rigid(Some(s)) => {
                    let p = place_stroke(
                        stroke,
                        spaced,
                        &hplace[*s].0,
                        &slot_source_x,
                        &input.glyphs,
                        anchors.get(&stroke.id()),
                    );
                    let dx = member_dx(placed.strokes.get(&stroke.id()));
                    let (from_x, to_x) = match ending_shift(stroke.id()) {
                        Some((_, shift)) => (spaced.from.x.0 + shift, spaced.to.x.0 + shift),
                        None => (p.from.x.0 + dx, p.to.x.0 + dx),
                    };
                    let (from, to) = clamp_before(
                        stroke.id(),
                        *s,
                        &hplace[*s].0,
                        Point::new(from_x, p.from.y.0),
                        Point::new(to_x, p.to.y.0),
                        half,
                    );
                    let (from, to) = hairpin_moved(stroke.id(), from, to);
                    vec![(*s, from, to)]
                }
                StrokeFate::Rigid(None) => Vec::new(),
                StrokeFate::Split(segments) => {
                    let mut segs: Vec<(usize, Point, Point)> = segments
                        .iter()
                        .enumerate()
                        .map(|(k, (s, from, to))| {
                            let p = &hplace[*s].0;
                            let (mut fx, tx) =
                                piece_x(stroke, k, segments.len(), *s, p, from.x.0, to.x.0);
                            if let (0, Some((_, shift))) = (k, ending_shift(stroke.id())) {
                                fx = spaced.from.x.0 + shift;
                            }
                            // A later piece opens with the line's continuation
                            // (X4d.1), its line after it.
                            if let (true, Some((run, gap))) = (k > 0, continuation_of(stroke.id()))
                            {
                                let ink = rect(
                                    fx + run.bounds.left.0,
                                    fx + run.bounds.right.0,
                                    run.origin.y.0 + run.bounds.bottom.0,
                                    run.origin.y.0 + run.bounds.top.0,
                                );
                                match placed.strokes.get(&stroke.id()) {
                                    Some(&f) => {
                                        let i = piece(&mut pieces, f, *s);
                                        pieces[i].boxes.push(ink);
                                    }
                                    None => {
                                        sky.entry((*s, stroke.vertical_band)).or_default().push(ink)
                                    }
                                }
                                fx = (fx + gap).min(tx);
                            }
                            (*s, Point::new(fx, from.y.0), Point::new(tx, to.y.0))
                        })
                        .collect();
                    if let Some((s, a, b)) = segs.last_mut() {
                        (*a, *b) = clamp_before(stroke.id(), *s, &hplace[*s].0, *a, *b, half);
                    }
                    if let Some(&(start, end)) = hairpin_ends.get(&stroke.id()) {
                        if let Some((_, a, _)) = segs.first_mut() {
                            *a = Point::new(a.x.0 + start, a.y.0);
                        }
                        if let Some((_, _, b)) = segs.last_mut() {
                            *b = Point::new(b.x.0 + end, b.y.0);
                        }
                    }
                    let xs: Vec<(f32, f32)> = segs.iter().map(|(_, a, b)| (a.x.0, b.x.0)).collect();
                    if let Some(ys) = glissando_slope(stroke, spaced, &xs) {
                        for ((_, a, b), (ya, yb)) in segs.iter_mut().zip(ys) {
                            *a = Point::new(a.x.0, ya);
                            *b = Point::new(b.x.0, yb);
                        }
                    }
                    segs
                }
            };
            let pieces_n = segments.len();
            for (k, (s, from, to)) in segments.into_iter().enumerate() {
                if let (true, Some(&f), Some(&(shape, sign))) = (
                    pieces_n > 1,
                    placed.strokes.get(&stroke.id()),
                    hairpin_legs.get(&stroke.id()),
                ) {
                    let (fy, ty) = hairpin_piece_ys(shape, sign, k, pieces_n);
                    drawn_legs.push((
                        f,
                        s,
                        rect(
                            from.x.0 - half,
                            to.x.0 + half,
                            fy.min(ty) - half,
                            fy.max(ty) + half,
                        ),
                    ));
                }
                let ink = if family_lines.get(&stroke.id()) == Some(&FamilyLine::Trill) {
                    // A trill line's ink is its wiggles' along it.
                    let w = metrics(TRILL_WIGGLE)
                        .expect("the wiggle is bundled")
                        .bounding_box();
                    rect(from.x.0, to.x.0, from.y.0 + w.bottom.0, from.y.0 + w.top.0)
                } else {
                    rect(
                        from.x.0 - half,
                        to.x.0 + half,
                        from.y.0.min(to.y.0) - half,
                        from.y.0.max(to.y.0) + half,
                    )
                };
                match placed.strokes.get(&stroke.id()) {
                    Some(&f) => {
                        let i = piece(&mut pieces, f, s);
                        pieces[i].boxes.push(ink);
                    }
                    None => sky.entry((s, stroke.vertical_band)).or_default().push(ink),
                }
            }
        }
        // Curves, sampled along their arcs where their systems set them.
        for (ci, curve) in spaced_curves.iter().enumerate() {
            let half = (curve.thickness.0 * 0.5).max(0.0);
            let segments = curve_pieces(ci);
            let n = segments.len();
            for (k, (s, cp)) in segments.into_iter().enumerate() {
                let cp = carried_curve(curve.id(), k, n, cp);
                let at = |t: f32| {
                    let u = 1.0 - t;
                    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    (
                        (0..4).map(|k| w[k] * cp[k].x.0).sum::<f32>(),
                        (0..4).map(|k| w[k] * cp[k].y.0).sum::<f32>(),
                    )
                };
                const SAMPLES: usize = 12;
                let band = sky.entry((s, curve.vertical_band)).or_default();
                let mut last = at(0.0);
                for k in 1..=SAMPLES {
                    let next = at(k as f32 / SAMPLES as f32);
                    band.push(rect(
                        last.0 - half,
                        next.0 + half,
                        last.1.min(next.1) - half,
                        last.1.max(next.1) + half,
                    ));
                    last = next;
                }
            }
        }
        // Text runs, each moved with its slot.
        for ((text, spaced), system) in input.texts.iter().zip(spaced_texts).zip(&text_system) {
            let Some(s) = *system else {
                continue;
            };
            let sx = slot_source_x
                .get(&text.horizontal_slot)
                .copied()
                .unwrap_or(spaced.origin.x.0);
            let x = spaced.origin.x.0 + hplace[s].0.slot_dx(sx) - shift_of(text.horizontal_slot)
                + member_dx(placed.texts.get(&text.id()));
            let (y, b) = (spaced.origin.y.0, spaced.bounds);
            let ink = rect(x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0);
            match placed.texts.get(&text.id()) {
                Some(&f) => {
                    let i = piece(&mut pieces, f, s);
                    pieces[i].boxes.push(ink);
                }
                None if !between.contains(&text.id()) => {
                    sky.entry((s, text.vertical_band)).or_default().push(ink)
                }
                None => {}
            }
        }
        let shifts = crate::family::place(&input.family, &pieces, &sky);
        for (p, by) in pieces.iter().zip(shifts) {
            family_dy.insert((p.family, p.system), by);
            let staff = staff_of(input.family[p.family].band);
            for b in &p.boxes {
                match staff {
                    Some(_) => {
                        into_staff(&mut staff_ext, p.system, staff, b.bottom + by, b.top + by)
                    }
                    None => extents[p.system].add_y(b.bottom + by, b.top + by),
                }
                placed_boxes.push((
                    p.family,
                    p.system,
                    InkRect {
                        bottom: b.bottom + by,
                        top: b.top + by,
                        ..*b
                    },
                ));
            }
        }
        for (f, s, b) in drawn_legs {
            let by = family_dy.get(&(f, s)).copied().unwrap_or(0.0);
            let staff = staff_of(input.family[f].band);
            match staff {
                Some(_) => into_staff(&mut staff_ext, s, staff, b.bottom + by, b.top + by),
                None => extents[s].add_y(b.bottom + by, b.top + by),
            }
            placed_boxes.push((
                f,
                s,
                InkRect {
                    bottom: b.bottom + by,
                    top: b.top + by,
                    ..b
                },
            ));
        }
        family_pieces = pieces;
        family_sky = sky;
    }

    // A later system's leads are its staves' content too, at the left
    // margin where the bake draws them.
    let mut lead_rects: Vec<(usize, StaffId, InkRect)> = Vec::new();
    for (s, leads) in system_leads.iter().enumerate() {
        for (lead, glyphs) in leads {
            for glyph in *glyphs {
                if let Some(m) = metrics(glyph.name.as_str()) {
                    let b = m.bounding_box();
                    into_staff(
                        &mut staff_ext,
                        s,
                        Some(lead.staff),
                        glyph.y + b.bottom.0,
                        glyph.y + b.top.0,
                    );
                    let x = geometry.margins.left.0 + glyph.x;
                    lead_rects.push((
                        s,
                        lead.staff,
                        InkRect {
                            left: x + b.left.0,
                            right: x + b.right.0,
                            bottom: glyph.y + b.bottom.0,
                            top: glyph.y + b.top.0,
                        },
                    ));
                }
            }
        }
    }

    // Each staff's ink in each system, as it faces the staff below and the
    // staff above: everything its band owns, the family where it is placed
    // and its lead. A verse keeps further from the next staff's ink, on the
    // side it stands.
    let mut staff_ink: BTreeMap<(usize, StaffId), (Vec<InkRect>, Vec<InkRect>)> = BTreeMap::new();
    // The ink of no staff, which only the gap between systems reads.
    let mut free_ink: BTreeMap<usize, Vec<InkRect>> = BTreeMap::new();
    for ((s, band), rects) in &family_sky {
        if let Some(st) = staff_of(*band) {
            let entry = staff_ink.entry((*s, st)).or_default();
            entry.0.extend(rects.iter().copied());
            entry.1.extend(rects.iter().copied());
        } else {
            free_ink
                .entry(*s)
                .or_default()
                .extend(rects.iter().copied());
        }
    }
    for (f, s, b) in &placed_boxes {
        let mark = &input.family[*f];
        let Some(st) = staff_of(mark.band) else {
            free_ink.entry(*s).or_default().push(*b);
            continue;
        };
        let lyric = mark
            .placement
            .filter(|p| p.order == epiphany_layout_ir::PlaceOrder::Lyric);
        let entry = staff_ink.entry((*s, st)).or_default();
        let mut down = *b;
        let mut up = *b;
        match lyric {
            Some(p) if p.above => up.top += crate::vertical::LYRIC_PADDING,
            Some(_) => down.bottom -= crate::vertical::LYRIC_PADDING,
            None => {}
        }
        entry.0.push(down);
        entry.1.push(up);
    }
    for (s, st, b) in &lead_rects {
        let entry = staff_ink.entry((*s, *st)).or_default();
        entry.0.push(*b);
        entry.1.push(*b);
    }

    // Solve each system's staves (X5d.2): order them top to bottom by their
    // reference y (staff line, else content mid), keep that order, and set
    // each as high as it can stand while its top line keeps the floor below
    // the bottom line of the staff above, its ink the clearance below the
    // merged outline of every staff above where they meet across, and a beam
    // joining it to a staff above its bound (`vertical::stack`).
    // `staff_shift[(system, staff)]` is the downward shift (subtracted from
    // y); the top staff's is 0. The shift is two-sided: the constrained
    // stage's fixed `SYSTEM_STAFF_PITCH` is an initial arrangement, not a
    // floor. The floor is Gould's stave height, compressed toward
    // MuseScore's least only in a system that would otherwise stand taller
    // than its page's content (D109 Q1, `vertical::stack_fitting`).
    let content_height = geometry.content_height();
    let mut staff_shift: BTreeMap<(usize, StaffId), f32> = BTreeMap::new();
    let no_ink: (Vec<InkRect>, Vec<InkRect>) = (Vec::new(), Vec::new());
    // Each system's staves top first, with their bottom and top lines.
    let mut system_staves: Vec<Vec<(StaffId, f32, f32)>> = Vec::with_capacity(systems.len());
    for (s, ext) in extents.iter().enumerate() {
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
        let inks: Vec<crate::vertical::StaffInk> = staves
            .iter()
            .map(|&(st, (_, hi))| {
                let (bottom, top) = staff_lines
                    .get(&(s, st))
                    .copied()
                    .unwrap_or((hi - STAFF_SPAN, hi));
                // A staff of one line is boxed two spaces either side of it,
                // as MuseScore boxes one (`SystemLayout`, a one-line staff's
                // bar line span), so it keeps a five-line staff's room.
                let (bottom, top) = if top - bottom < 1e-3 {
                    (bottom - STAFF_SPAN * 0.5, top + STAFF_SPAN * 0.5)
                } else {
                    (bottom, top)
                };
                let (down, up) = staff_ink.get(&(s, st)).unwrap_or(&no_ink);
                crate::vertical::StaffInk {
                    top,
                    bottom,
                    down: down.clone(),
                    up: up.clone(),
                }
            })
            .collect();
        let index: BTreeMap<StaffId, usize> = staves
            .iter()
            .enumerate()
            .map(|(i, (st, _))| (*st, i))
            .collect();
        let beams: Vec<crate::vertical::BeamBound> = floors
            .range((s, StaffId::from_raw(0), StaffId::from_raw(0))..)
            .take_while(|((sys, ..), _)| *sys == s)
            .filter_map(|(&(_, above, below), &least)| {
                Some(crate::vertical::BeamBound {
                    upper: *index.get(&above)?,
                    lower: *index.get(&below)?,
                    least,
                })
            })
            .collect();
        // The system's height with its staves so shifted: its staves' ink
        // and the ink of no staff, as it is folded into its extent below.
        let free = (ext.min_y, ext.max_y);
        let fits = |shifts: &[f32]| {
            if content_height <= 0.0 {
                return true;
            }
            let (mut lo, mut hi) = free;
            for (&(_, (a, b)), sh) in staves.iter().zip(shifts) {
                lo = lo.min(a - sh);
                hi = hi.max(b - sh);
            }
            hi - lo <= content_height
        };
        let (_, shifts) = crate::vertical::stack_fitting(&inks, &beams, fits);
        for (&(st, _), shift) in staves.iter().zip(shifts) {
            staff_shift.insert((s, st), shift);
        }
        system_staves.push(
            staves
                .iter()
                .zip(&inks)
                .map(|(&(st, _), ink)| (st, ink.bottom, ink.top))
                .collect(),
        );
    }

    // ---- Between systems and the page (X5d.3) -----------------------------
    // Each system's ink, up and down, with its staves where the solve set
    // them: its staves' extents and the ink of no staff.
    type Shifts = BTreeMap<(usize, StaffId), f32>;
    let ink_y = |s: usize, shifts: &Shifts| -> (f32, f32) {
        let (mut lo, mut hi) = (extents[s].min_y, extents[s].max_y);
        for &(st, _, _) in &system_staves[s] {
            if let Some(&(a, b)) = staff_ext.get(&(s, st)) {
                let sh = shifts.get(&(s, st)).copied().unwrap_or(0.0);
                lo = lo.min(a - sh);
                hi = hi.max(b - sh);
            }
        }
        if lo > hi {
            (0.0, 0.0)
        } else {
            (lo, hi)
        }
    };
    // Each system's top line and bottom line, where the solve set them.
    let lines_of = |s: usize, shifts: &Shifts| -> (f32, f32) {
        let staves = &system_staves[s];
        let shift = |st: StaffId| shifts.get(&(s, st)).copied().unwrap_or(0.0);
        match (staves.first(), staves.last()) {
            (Some(&(first, _, top)), Some(&(last, bottom, _))) => {
                (top - shift(first), bottom - shift(last))
            }
            _ => {
                let (lo, hi) = ink_y(s, shifts);
                (hi, lo)
            }
        }
    };
    // How far apart two consecutive systems stand, the bottom line of the
    // one above to the top line of the one below, as `dy` of the upper
    // less `dy` of the lower: at least the floor between systems, and the
    // whole system above's ink the clearance from the whole system below's
    // wherever they meet across (D109 Q3's gap between systems, Q2's
    // merged outline).
    // A system's ink as it faces the system below (`down`) or above, its
    // staves where the solve set them, with the ink of no staff.
    let system_rects = |s: usize, shifts: &Shifts, down: bool| -> Vec<InkRect> {
        let mut rects: Vec<InkRect> = free_ink.get(&s).cloned().unwrap_or_default();
        for &(st, _, _) in &system_staves[s] {
            let sh = shifts.get(&(s, st)).copied().unwrap_or(0.0);
            let (d, u) = staff_ink.get(&(s, st)).unwrap_or(&no_ink);
            rects.extend((if down { d } else { u }).iter().map(|r| InkRect {
                bottom: r.bottom - sh,
                top: r.top - sh,
                ..*r
            }));
        }
        rects
    };
    let system_gap = |upper: usize, lower: usize, shifts: &Shifts| -> f32 {
        let (_, bottom) = lines_of(upper, shifts);
        let (top, _) = lines_of(lower, shifts);
        let outline = crate::vertical::Outline::below(
            &system_rects(upper, shifts, true),
            crate::vertical::INK_REACH,
        );
        let by_ink = crate::vertical::need_below(&outline, &system_rects(lower, shifts, false));
        (crate::vertical::SYSTEM_FLOOR + top - bottom).max(by_ink)
    };

    // ---- Vertical stacking and page assignment ----------------------------
    // A page's first system stands with its first staff's top line
    // `PAGE_BORDER` below the top margin, or lower where its ink needs; each
    // further system stands its gap below the one above, and goes on the
    // page only where its last staff's bottom line then keeps `PAGE_BORDER`
    // above the bottom margin, or its ink does where it reaches further
    // (MuseScore's `staffUpperBorder` and `staffLowerBorder`, D109 Q4). A
    // system taller than a page stays alone on the page it opens.
    let content_height = geometry.content_height();
    let bounded = content_height > 0.0;
    let border = crate::vertical::PAGE_BORDER;
    let mut placements: Vec<Placement> = Vec::with_capacity(systems.len());
    // Whether each system is compressed, its curves then passed again.
    let mut compressed: Vec<bool> = Vec::with_capacity(systems.len());
    let mut page_systems: Vec<Vec<usize>> = Vec::new();
    let mut page_floor = 0.0_f32;
    for (s, plan) in systems.iter().enumerate() {
        let (lo, hi) = ink_y(s, &staff_shift);
        let (top, bottom) = lines_of(s, &staff_shift);
        // Where it would stand after the system above on this page.
        let after = page_systems
            .last()
            .and_then(|page| page.last())
            .map(|&u: &usize| placements[u].dy - system_gap(u, s, &staff_shift));
        let fits = |dy: f32| bottom + dy - (bottom - lo).max(border) >= page_floor;
        let dy = match after {
            Some(dy) if !plan.page_forced && (!bounded || fits(dy)) => dy,
            _ => {
                let p = page_systems.len();
                let page_top = page_top_content(p, geometry);
                page_floor = page_top - content_height.max(0.0);
                page_systems.push(Vec::new());
                page_top - (hi - top).max(border) - top
            }
        };
        // The system's map across, set before the family was placed; its
        // height now.
        let (mut placement, squeezed) = hplace[s].clone();
        placement.dy = dy;
        compressed.push(squeezed);
        placements.push(placement);
        page_systems
            .last_mut()
            .expect("a page was opened above")
            .push(s);
    }

    // ---- Vertical justification -------------------------------------------
    // A page whose system passes its bottom margin (one alone, taller than
    // the page) rises as far as its top border lets it, its ink no higher
    // than the top margin: the borders are aims, which cost a system no fit
    // (D109 Q4). Every other page, the last included, is spread by
    // MuseScore's rule until its last staff's bottom line stands
    // `PAGE_BORDER` above its bottom margin or its ink reaches the margin
    // (D109 Q3): a gap between systems first raised to the widest gap
    // between staves of a system beside it (Gould, p. 488), then the
    // smallest gaps raised level and the rest shared by weight
    // (`vertical::spread`).
    let mut overfull: Vec<OverfullSystem> = Vec::new();
    if bounded {
        for (p, page) in page_systems.iter().enumerate() {
            let page_top = page_top_content(p, geometry);
            let floor = page_top - content_height;
            let (first, last) = (page[0], *page.last().expect("a page holds a system"));
            let lowest = ink_y(last, &staff_shift).0 + placements[last].dy;
            if lowest < floor {
                let (_, hi) = ink_y(first, &staff_shift);
                let (top, _) = lines_of(first, &staff_shift);
                let room = (hi - top).max(border) - (hi - top);
                let rise = (floor - lowest).min(room);
                for &s in page {
                    placements[s].dy += rise;
                }
                continue;
            }
            let bottom_line = lines_of(last, &staff_shift).1 + placements[last].dy;
            let space = (lowest - floor).min(bottom_line - border - floor);
            if space <= 0.0 {
                continue;
            }
            spread_page(
                page,
                space,
                input,
                &systems,
                &system_staves,
                &mut staff_shift,
                &mut placements,
            );
        }
        // A system whose ink its paper cannot hold is named, not squeezed
        // (D109 Q5).
        for (p, page) in page_systems.iter().enumerate() {
            let frame = geometry.page_frame(p);
            let (paper_bottom, paper_top) =
                (frame.origin.y.0, frame.origin.y.0 + frame.size.height.0);
            for &s in page {
                let (lo, hi) = ink_y(s, &staff_shift);
                let past =
                    (paper_bottom - (lo + placements[s].dy)).max(hi + placements[s].dy - paper_top);
                if past > 0.0 {
                    let plan = &systems[s];
                    let slots = &region_slots[plan.region];
                    overfull.push(OverfullSystem {
                        past,
                        opens: plan
                            .slots
                            .iter()
                            .find_map(|&i| slots[i].measure_barline)
                            .map(|g| input.glyphs[g].provenance.source),
                    });
                }
            }
        }
    }

    // A dynamic or hairpin of a grand staff stands centred between its two
    // staves once they are set and spread, as far as the ink between them
    // lets it, the members of a row together
    // (`AlignmentLayout::alignStaffCenteredItems`, which MuseScore runs once
    // a page is spread).
    let mut centred: BTreeMap<(usize, u128), Vec<usize>> = BTreeMap::new();
    for (i, p) in family_pieces.iter().enumerate() {
        let Some(place) = input.family[p.family].placement else {
            continue;
        };
        if place.between.is_some() {
            let row = place.row.map_or(u128::MAX - i as u128, |row| row.id);
            centred.entry((p.system, row)).or_default().push(i);
        }
    }
    for ((s, _), members) in centred {
        let first = &input.family[family_pieces[members[0]].family];
        let Some(place) = first.placement else {
            continue;
        };
        let (own, Some(other)) = (first.band, place.between) else {
            continue;
        };
        let line = |band: VerticalBandId| {
            let st = staff_of(band)?;
            Some((
                *staff_ref.get(&(s, st))?,
                staff_shift.get(&(s, st)).copied().unwrap_or(0.0),
            ))
        };
        let (Some((own_top, own_shift)), Some((other_top, other_shift))) = (line(own), line(other))
        else {
            continue;
        };
        // The other staff in this one's frame, and the middle of the gap
        // between their lines, a reference being a top line's ink.
        let off = own_shift - other_shift;
        let half = |band: VerticalBandId| {
            input
                .strokes
                .iter()
                .find(|s| {
                    s.vertical_band == band
                        && matches!(s.provenance.source, TypedObjectId::Staff(_))
                })
                .map_or(0.0, |s| s.thickness.0 * 0.5)
        };
        let target = (own_top - half(own) + other_top - half(other) + off - STAFF_SPAN) * 0.5;
        let centre = place.row.map_or(0.0, |row| row.centre)
            + family_dy
                .get(&(first_of(&family_pieces, &members), s))
                .copied()
                .unwrap_or(0.0);
        // As far as its own staff's ink and the other's let it, the family
        // already placed on either included where it now stands (X5d.7), the
        // row's own members aside, as MuseScore's centring reads both
        // staves' skylines whole.
        let row: BTreeSet<usize> = members.iter().map(|&i| family_pieces[i].family).collect();
        let (own_staff, other_staff) = (staff_of(own), staff_of(other));
        let mut placed_own: Vec<InkRect> = Vec::new();
        let mut placed_other: Vec<InkRect> = Vec::new();
        for q in family_pieces
            .iter()
            .filter(|q| q.system == s && !row.contains(&q.family))
        {
            let staff = staff_of(input.family[q.family].band);
            let lift = family_dy.get(&(q.family, s)).copied().unwrap_or(0.0);
            let into = if staff == own_staff {
                &mut placed_own
            } else if staff == other_staff {
                &mut placed_other
            } else {
                continue;
            };
            into.extend(q.boxes.iter().map(|b| InkRect {
                bottom: b.bottom + lift,
                top: b.top + lift,
                ..*b
            }));
        }
        let (mut up, mut down) = (f32::INFINITY, f32::NEG_INFINITY);
        for &i in &members {
            let p = &family_pieces[i];
            let dy = family_dy.get(&(p.family, s)).copied().unwrap_or(0.0);
            let min = input.family[p.family]
                .placement
                .map_or(0.0, |place| place.min_distance);
            for b in &p.boxes {
                let meets = |r: &&InkRect| r.left < b.right && r.right > b.left;
                for r in family_sky
                    .get(&(s, own))
                    .into_iter()
                    .flatten()
                    .chain(&placed_own)
                    .filter(meets)
                {
                    if place.above {
                        down = down.max(r.top + min - (b.bottom + dy));
                    } else {
                        up = up.min(r.bottom - min - (b.top + dy));
                    }
                }
                for r in family_sky
                    .get(&(s, other))
                    .into_iter()
                    .flatten()
                    .chain(&placed_other)
                    .filter(meets)
                {
                    if place.above {
                        up = up.min(r.bottom + off - min - (b.top + dy));
                    } else {
                        down = down.max(r.top + off + min - (b.bottom + dy));
                    }
                }
            }
        }
        if down > up {
            continue;
        }
        let delta = (target - centre).clamp(down, up);
        for &i in &members {
            *family_dy.entry((family_pieces[i].family, s)).or_insert(0.0) += delta;
        }
    }

    // Fold each staff's SHIFTED content y-extent into its system extent, so the
    // stacking/justification below sees the taller, separated system.
    for ((s, st), (lo, hi)) in &staff_ext {
        let sh = staff_shift.get(&(*s, *st)).copied().unwrap_or(0.0);
        extents[*s].add_y(lo - sh, hi - sh);
    }
    let extents: Vec<Extent> = extents.into_iter().map(Extent::normalized).collect();

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
    // Computed once, alongside the positioning it also drives (W1 pin 7): the
    // quality-metric census consumes this published vector rather than
    // re-deriving the same attribution from `system_of_slot` itself.
    let (glyphs, glyph_system): (Vec<ResolvedGlyph>, Vec<Option<usize>>) = spaced_glyphs
        .iter()
        .zip(&input.glyphs)
        .enumerate()
        .map(|(gi, (spaced, glyph))| {
            let moved = ending_shift(glyph.id());
            let system = match moved {
                Some((t, _)) => Some(t),
                None => system_of_slot.get(&glyph.horizontal_slot).copied(),
            };
            let (dx, dy) = match system {
                Some(s) if moved.is_some() => (
                    moved.map_or(0.0, |(_, dx)| dx),
                    placements[s].dy - staff_dy(s, glyph_staff_of[gi]),
                ),
                Some(s) => {
                    // Slot-relative: every member of a slot translates by the
                    // map at the slot's source, so intra-slot offsets survive.
                    let sx = slot_source_x
                        .get(&glyph.horizontal_slot)
                        .copied()
                        .unwrap_or(spaced.position.x.0);
                    let lift = placed
                        .glyphs
                        .get(&glyph.id())
                        .and_then(|f| family_dy.get(&(*f, s)))
                        .copied()
                        .unwrap_or(0.0);
                    (
                        placements[s].slot_dx(sx) - shift_of(glyph.horizontal_slot)
                            + rest_dx.get(&gi).copied().unwrap_or(0.0)
                            + member_dx(placed.glyphs.get(&glyph.id())),
                        placements[s].dy - staff_dy(s, glyph_staff_of[gi]) + lift,
                    )
                }
                None => (0.0, 0.0),
            };
            let resolved = ResolvedGlyph {
                position: Point::new(spaced.position.x.0 + dx, spaced.position.y.0 + dy),
                ..spaced.clone()
            };
            (resolved, system)
        })
        .unzip();

    // Each later system's leads, at its left margin, on their staves.
    let mut glyphs = glyphs;
    let mut glyph_system = glyph_system;
    let mut appended_bands = Vec::new();
    for (s, leads) in system_leads.iter().enumerate() {
        for (lead, lead_glyphs) in leads {
            let dy = placements[s].dy - staff_dy(s, Some(lead.staff));
            for (i, glyph) in lead_glyphs.iter().enumerate() {
                let Some(m) = metrics(glyph.name.as_str()) else {
                    continue;
                };
                glyphs.push(ResolvedGlyph {
                    provenance: Provenance::synthesized(
                        lead.provenance.source,
                        SynthesisKind::Registered(SYSTEM_LEAD_SYNTHESIS),
                        SynthesisInstanceKey((systems[s].local as u128) << 16 | i as u128),
                        lead.provenance.dependencies.clone(),
                    ),
                    glyph: glyph.name.clone(),
                    position: Point::new(geometry.margins.left.0 + glyph.x, glyph.y + dy),
                    transform: None,
                    bounding_box: m.bounding_box(),
                    style: GlyphStyle { rgba: 0x0000_00ff },
                    layer: 0,
                });
                glyph_system.push(Some(s));
                appended_bands.push(lead.band);
            }
        }
    }

    // Per-system staff-line marks, for the resolved staff records below.
    let mut staff_marks: BTreeMap<(usize, StaffId), StaffAgg> = BTreeMap::new();
    let mut strokes: Vec<Stroke> = Vec::with_capacity(spaced_strokes.len());
    let mut continuations: Vec<Stroke> = Vec::new();
    // The system each baked stroke landed in, parallel to `strokes` (a quality
    // metric measures a system's realized per-staff content extents, and a
    // stroke carries no spring slot to look one up with).
    let mut stroke_system: Vec<Option<usize>> = Vec::with_capacity(spaced_strokes.len());
    let mut continuation_system: Vec<Option<usize>> = Vec::new();
    // The runs the family's lines draw piece by piece (X4d.1), each with its
    // system.
    let mut line_runs: Vec<(ResolvedText, Option<usize>)> = Vec::new();
    // The band of the line each run rides, parallel to `line_runs`.
    let mut line_run_bands: Vec<VerticalBandId> = Vec::new();
    // Each glissando's words among them, with its piece's ends, its source,
    // the words' width and height before they were turned to the line, and
    // its parallel set, rank and piece.
    let mut gliss_words: Vec<GlissWords> = Vec::new();
    // Each glissando of one piece the cast page leaves no room.
    let mut short_glissandi: BTreeSet<TypedObjectId> = BTreeSet::new();
    for (si, ((source, spaced), fate)) in input
        .strokes
        .iter()
        .zip(spaced_strokes)
        .zip(&fates)
        .enumerate()
    {
        match fate {
            StrokeFate::Rigid(sys) => {
                let lift = |s: usize| {
                    placed
                        .strokes
                        .get(&source.id())
                        .and_then(|f| family_dy.get(&(*f, s)))
                        .copied()
                        .unwrap_or(0.0)
                };
                let mut stroke = match sys {
                    Some(s) => {
                        let p = placements[*s].sunk(staff_dy(*s, stroke_staff_of[si]) - lift(*s));
                        let mut stroke = place_stroke(
                            source,
                            spaced,
                            &p,
                            &slot_source_x,
                            &input.glyphs,
                            anchors.get(&source.id()),
                        );
                        end_staff_line(&mut stroke, staff_end[*s].map(|x| p.x(x)));
                        (stroke.from, stroke.to) =
                            hairpin_moved(source.id(), stroke.from, stroke.to);
                        if hairpin_legs.contains_key(&source.id()) {
                            keep_hairpin_length(&mut stroke);
                        }
                        (stroke.from, stroke.to) = clamp_before(
                            source.id(),
                            *s,
                            &p,
                            stroke.from,
                            stroke.to,
                            stroke.thickness.0 * 0.5,
                        );
                        let dx = member_dx(placed.strokes.get(&source.id()));
                        stroke.from = Point::new(stroke.from.x.0 + dx, stroke.from.y.0);
                        stroke.to = Point::new(stroke.to.x.0 + dx, stroke.to.y.0);
                        // A moved ending's hook stands where its start does.
                        if let Some((_, shift)) = ending_shift(source.id()) {
                            stroke.from = Point::new(spaced.from.x.0 + shift, stroke.from.y.0);
                            stroke.to = Point::new(spaced.to.x.0 + shift, stroke.to.y.0);
                        }
                        // A stem reaching a beam across two staves: its beam
                        // end rides the staff the beam does.
                        if let Some(&upper) = reach_upper.get(&source.id()) {
                            let dy = staff_dy(*s, stroke_staff_of[si]) - staff_dy(*s, Some(upper));
                            stroke.to = Point::new(stroke.to.x.0, stroke.to.y.0 + dy);
                        }
                        stroke
                    }
                    None => spaced.clone(),
                };
                if let (Some(s), TypedObjectId::Staff(staff)) = (sys, spaced.provenance.source) {
                    mark_staff(&mut staff_marks, *s, staff, &stroke);
                }
                let more = lyric_line(&mut stroke, family_lines.get(&source.id()), false);
                for (j, dash) in more.into_iter().enumerate() {
                    continuations.push(Stroke {
                        provenance: extra_dash(spaced, 0, j),
                        ..dash
                    });
                    continuation_system.push(*sys);
                }
                if family_lines.get(&source.id()) == Some(&FamilyLine::Trill) {
                    for (i, x) in trill_wiggles(&mut stroke).into_iter().enumerate() {
                        glyphs.push(wiggle(spaced, 0, i, Point::new(x, stroke.from.y.0)));
                        glyph_system.push(*sys);
                        appended_bands.push(stroke.vertical_band);
                    }
                }
                match family_lines.get(&source.id()) {
                    Some(FamilyLine::Extension {
                        dashes: Some(dashes),
                        ..
                    }) => {
                        for (j, dash) in dash_line(&mut stroke, *dashes).into_iter().enumerate() {
                            continuations.push(Stroke {
                                provenance: extra_dash(spaced, 0, j),
                                ..dash
                            });
                            continuation_system.push(*sys);
                        }
                    }
                    Some(FamilyLine::Glissando { wavy, .. }) => {
                        if let Some(ys) =
                            glissando_slope(source, spaced, &[(stroke.from.x.0, stroke.to.x.0)])
                        {
                            let dy = stroke.from.y.0 - spaced.from.y.0;
                            stroke.from = Point::new(stroke.from.x.0, ys[0].0 + dy);
                            stroke.to = Point::new(stroke.to.x.0, ys[0].1 + dy);
                        }
                        // A piece the system leaves too short for its notes
                        // is left undrawn and named (X4d fix round 2).
                        if too_short(&stroke) {
                            stroke.to = stroke.from;
                            short_glissandi.insert(source.provenance.source);
                        } else {
                            let (from, to, words_id) =
                                (stroke.from, stroke.to, line_run_id(&stroke, 0, 2));
                            let size = words_extent(words_of(source.id()));
                            let set = words_set(source.id()).map(|(set, rank)| (set, rank, 0));
                            for run in glissando_runs(&mut stroke, *wavy, words_of(source.id()), 0)
                            {
                                if run.provenance.stable_id == words_id {
                                    gliss_words.push((
                                        line_runs.len(),
                                        from,
                                        to,
                                        source.provenance.source,
                                        size,
                                        set,
                                    ));
                                }
                                line_runs.push((run, *sys));
                                line_run_bands.push(source.vertical_band);
                            }
                        }
                    }
                    _ => {}
                }
                strokes.push(stroke);
                stroke_system.push(*sys);
            }
            StrokeFate::Split(segments) => {
                let leg = hairpin_legs.get(&source.id());
                // A glissando keeps its gradient across its pieces (X4d.1).
                let xs: Vec<(f32, f32)> = segments
                    .iter()
                    .enumerate()
                    .map(|(k, (s, from, to))| {
                        let p = &placements[*s];
                        piece_x(source, k, segments.len(), *s, p, from.x.0, to.x.0)
                    })
                    .collect();
                let slope = glissando_slope(source, spaced, &xs);
                for (k, (s, from, to)) in segments.iter().enumerate() {
                    // A split stroke spans systems — a staff line or volta
                    // bracket, a hairpin — so each segment stretches with its
                    // system, a placed one lifted as its piece there is.
                    let lift = placed
                        .strokes
                        .get(&source.id())
                        .and_then(|f| family_dy.get(&(*f, *s)))
                        .copied()
                        .unwrap_or(0.0);
                    let p = placements[*s].sunk(staff_dy(*s, stroke_staff_of[si]) - lift);
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
                    // A staff line reaches back under a later system's lead,
                    // and a moved ending's line starts where its hook stands.
                    let (mut from_x, mut to_x) =
                        piece_x(source, k, segments.len(), *s, &p, from.x.0, to.x.0);
                    if let (0, Some((_, shift))) = (k, ending_shift(source.id())) {
                        from_x = spaced.from.x.0 + shift;
                    }
                    // A hairpin's first piece starts, and its last ends, as
                    // its dynamics there are moved.
                    if let Some(&(start, end)) = hairpin_ends.get(&source.id()) {
                        if k == 0 {
                            from_x += start;
                        }
                        if k + 1 == segments.len() {
                            to_x += end;
                        }
                    }
                    if lead_w[*s] > 0.0
                        && matches!(spaced.provenance.source, TypedObjectId::Staff(_))
                    {
                        from_x = geometry.margins.left.0;
                    }
                    // A hairpin's piece opens from its axis as MuseScore draws
                    // each piece of one over a system break: to its full
                    // opening at its wide end, and to its continuation's at a
                    // narrow end the break leaves open.
                    let (from_y, to_y) = match leg {
                        Some(&(shape, sign)) => hairpin_piece_ys(shape, sign, k, segments.len()),
                        None => match &slope {
                            Some(ys) => ys[k],
                            None => (from.y.0, to.y.0),
                        },
                    };
                    // A later piece opens with the line's continuation
                    // (X4d.1), its line after it.
                    if let (true, Some((run, gap))) = (k > 0, continuation_of(source.id())) {
                        let mut run = line_run(run, spaced, k, 1);
                        run.origin = Point::new(from_x, run.origin.y.0 + p.dy);
                        line_runs.push((run, Some(*s)));
                        line_run_bands.push(source.vertical_band);
                        from_x = (from_x + gap).min(to_x);
                    }
                    let mut stroke = Stroke {
                        provenance,
                        from: Point::new(from_x, from_y + p.dy),
                        to: Point::new(to_x, to_y + p.dy),
                        thickness: spaced.thickness,
                        layer: spaced.layer,
                        style: spaced.style,
                        vertical_band: spaced.vertical_band,
                    };
                    end_staff_line(&mut stroke, staff_end[*s].map(|x| p.x(x)));
                    if leg.is_some() {
                        keep_hairpin_length(&mut stroke);
                    }
                    if k + 1 == segments.len() {
                        (stroke.from, stroke.to) = clamp_before(
                            source.id(),
                            *s,
                            &p,
                            stroke.from,
                            stroke.to,
                            stroke.thickness.0 * 0.5,
                        );
                    }
                    if let TypedObjectId::Staff(staff) = spaced.provenance.source {
                        mark_staff(&mut staff_marks, *s, staff, &stroke);
                    }
                    // A piece after the first ends at a syllable opening its
                    // system, or spans one whole, longer than any dash.
                    let more = lyric_line(&mut stroke, family_lines.get(&source.id()), k > 0);
                    for (j, dash) in more.into_iter().enumerate() {
                        continuations.push(Stroke {
                            provenance: extra_dash(spaced, k, j),
                            ..dash
                        });
                        continuation_system.push(Some(*s));
                    }
                    if family_lines.get(&source.id()) == Some(&FamilyLine::Trill) {
                        for (i, x) in trill_wiggles(&mut stroke).into_iter().enumerate() {
                            glyphs.push(wiggle(spaced, k, i, Point::new(x, stroke.from.y.0)));
                            glyph_system.push(Some(*s));
                            appended_bands.push(stroke.vertical_band);
                        }
                    }
                    match family_lines.get(&source.id()) {
                        Some(FamilyLine::Extension {
                            dashes: Some(dashes),
                            ..
                        }) => {
                            for (j, dash) in dash_line(&mut stroke, *dashes).into_iter().enumerate()
                            {
                                continuations.push(Stroke {
                                    provenance: extra_dash(spaced, k, j),
                                    ..dash
                                });
                                continuation_system.push(Some(*s));
                            }
                        }
                        Some(FamilyLine::Glissando { wavy, .. }) => {
                            let (from, to, words_id) =
                                (stroke.from, stroke.to, line_run_id(&stroke, k, 2));
                            let size = words_extent(words_of(source.id()));
                            let set = words_set(source.id()).map(|(set, rank)| (set, rank, k));
                            for run in glissando_runs(&mut stroke, *wavy, words_of(source.id()), k)
                            {
                                if run.provenance.stable_id == words_id {
                                    gliss_words.push((
                                        line_runs.len(),
                                        from,
                                        to,
                                        source.provenance.source,
                                        size,
                                        set,
                                    ));
                                }
                                line_runs.push((run, Some(*s)));
                                line_run_bands.push(source.vertical_band);
                            }
                        }
                        _ => {}
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
    let ink = placed_ink(
        input,
        &glyphs,
        &glyph_system,
        &strokes,
        &stroke_system,
        &anchors,
    );
    let no_ink = PlacedInk::default();
    for (ci, (curve, fate)) in spaced_curves.iter().zip(&curve_fates).enumerate() {
        let curve_staff = curve_staff_of[ci];
        // A slur has no intra-slot structure, so its control points map straight
        // through the affine: the endpoints follow their anchor notes (which sit
        // at slot sources) and the arc stretches horizontally with the span.
        let shift =
            |cp: [Point; 4], p: &Placement| cp.map(|pt| Point::new(p.x(pt.x.0), pt.y.0 + p.dy));
        match fate {
            CurveFate::Rigid(system) => {
                let p = system
                    .map(|s| placements[s].sunk(staff_dy(s, curve_staff)))
                    .unwrap_or(Placement::rigid(0.0, 0.0));
                // An anchored curve's ends ride their slots' deltas; any other
                // maps through the system's affine whole.
                let slot_dx =
                    |slot: &SpringSlotId| slot_source_x.get(slot).map(|&sx| p.slot_dx(sx));
                let anchored = anchors
                    .get(&curve.id())
                    .filter(|_| system.is_some())
                    .and_then(|(start, end)| Some((slot_dx(start)?, slot_dx(end)?)));
                // A compressed system's slur or tie is placed from where the
                // spacing set it before it passed any ink, and passes the ink
                // of its band where the compressed system sets it
                // (`pass_compressed`).
                let curved = matches!(
                    curve.provenance.source,
                    TypedObjectId::Slur(_) | TypedObjectId::Tie(_)
                );
                let again = system.filter(|&s| compressed[s] && curved);
                let from = match again {
                    Some(_) => respacing.curves[ci],
                    None => curve.control_points(),
                };
                let placed = match anchored {
                    Some((start, end)) => crate::anchored_curve(from, start, end)
                        .map(|pt| Point::new(pt.x.0, pt.y.0 + p.dy)),
                    None => shift(from, &p),
                };
                // A tie the spacing left running into its first note's flag,
                // the spaced line too short to start it past the flag, starts
                // past it where its system's justification gives it the span,
                // and passes the accidentals and stems of its band again there.
                let [p0, p1, p2, p3] = match (system, curve.provenance.source) {
                    (Some(s), source) if again.is_some() => {
                        let ink = ink.get(&(*s, curve.vertical_band)).unwrap_or(&no_ink);
                        pass_compressed(placed, ink, matches!(source, TypedObjectId::Tie(_)))
                    }
                    (Some(s), TypedObjectId::Tie(_)) => {
                        let ink = ink.get(&(*s, curve.vertical_band)).unwrap_or(&no_ink);
                        crate::restart_past_flag(placed, &ink.flags, &ink.accidentals, &ink.stems)
                    }
                    _ => placed,
                };
                let [p0, p1, p2, p3] = carried_curve(curve.id(), 0, 1, [p0, p1, p2, p3]);
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
                    let p = placements[*s].sunk(staff_dy(*s, curve_staff));
                    let [mut p0, mut p1, mut p2, mut p3] = shift(*cp, &p);
                    // A tie's half-arcs keep their note ends on their slots,
                    // and the second starts clear of its system's lead, and of
                    // any column before its note's (a time signature).
                    if anchored_halves.contains(&ci) {
                        if let Some((start, end)) = anchors.get(&curve.id()) {
                            let slot_dx = |slot: &SpringSlotId| {
                                slot_source_x.get(slot).map(|&sx| p.slot_dx(sx))
                            };
                            if k == 0 {
                                if let Some(dx) = slot_dx(start) {
                                    p0 = Point::new(cp[0].x.0 + dx, p0.y.0);
                                }
                            } else if let Some(dx) = slot_dx(end) {
                                let x3 = cp[3].x.0 + dx;
                                let plan = &systems[*s];
                                let lead = geometry.margins.left.0 + lead_ink[*s].unwrap_or(0.0);
                                let before =
                                    ink_before(plan, &region_slots[plan.region], *end, |info| {
                                        info.hi + p.slot_dx(info.x) - shift_of(info.id)
                                    });
                                let x0 = lead.max(before.unwrap_or(f32::NEG_INFINITY))
                                    + TIE_LEAD_CLEARANCE;
                                let above = cp[1].y.0 > cp[0].y.0;
                                [p0, p1, p2, p3] = tie_arc(x0.min(x3), x3, p3.y.0, above);
                            }
                        }
                    }
                    let [p0, p1, p2, p3] =
                        carried_curve(curve.id(), k, segments.len(), [p0, p1, p2, p3]);
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

    // Text runs: each moves with its slot, as the glyphs of its slot do, and
    // with its staff's shift.
    let texts: Vec<ResolvedText> = spaced_texts
        .iter()
        .zip(&input.texts)
        .enumerate()
        .map(|(ti, (spaced, text))| {
            let (dx, dy) = match text_system[ti] {
                Some(s) => {
                    let sx = slot_source_x
                        .get(&text.horizontal_slot)
                        .copied()
                        .unwrap_or(spaced.origin.x.0);
                    let lift = placed
                        .texts
                        .get(&text.id())
                        .and_then(|f| family_dy.get(&(*f, s)))
                        .copied()
                        .unwrap_or(0.0);
                    (
                        placements[s].slot_dx(sx) - shift_of(text.horizontal_slot)
                            + member_dx(placed.texts.get(&text.id()))
                            + verbal_words_dx.get(&text.id()).copied().unwrap_or(0.0),
                        placements[s].dy - staff_dy(s, text_staff_of[ti]) + lift,
                    )
                }
                None => (0.0, 0.0),
            };
            ResolvedText {
                origin: Point::new(spaced.origin.x.0 + dx, spaced.origin.y.0 + dy),
                ..spaced.clone()
            }
        })
        .collect();
    let mut texts = texts;
    let mut text_system = text_system;
    // A parallel set's words stand, piece by piece, on its topmost line whose
    // piece holds them (X4d fix round 2): each line offered them, and those
    // of a lower line where a higher one drew them are dropped.
    let mut topmost: BTreeMap<(u128, usize), usize> = BTreeMap::new();
    for (.., set) in &gliss_words {
        if let Some((set, rank, piece)) = set {
            let least = topmost.entry((*set, *piece)).or_insert(*rank);
            *least = (*least).min(*rank);
        }
    }
    let dropped: BTreeSet<usize> = gliss_words
        .iter()
        .filter(|(.., set)| {
            set.is_some_and(|(set, rank, piece)| topmost.get(&(set, piece)) != Some(&rank))
        })
        .map(|(i, ..)| *i)
        .collect();
    gliss_words.retain(|(i, ..)| !dropped.contains(i));
    // A glissando's words kept off barlines and other ink (X4d fix round 1):
    // where, centred along their piece, they meet other ink of their system,
    // they slide along the piece and then stand further off it, by quarter
    // spaces, to the first place clear of it; where none is, they stay.
    if !gliss_words.is_empty() {
        let rect = |x: f32, y: f32, b: &epiphany_layout_ir::BoundingBox| {
            [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
        };
        let mut ink: BTreeMap<usize, Vec<(TypedObjectId, [f32; 4])>> = BTreeMap::new();
        for (g, s) in glyphs.iter().zip(&glyph_system) {
            if let Some(s) = s {
                ink.entry(*s).or_default().push((
                    g.provenance.source,
                    rect(g.position.x.0, g.position.y.0, &g.bounding_box),
                ));
            }
        }
        for (stroke, s) in strokes.iter().zip(&stroke_system) {
            let Some(s) = s else {
                continue;
            };
            if stroke.from == stroke.to
                || matches!(stroke.provenance.source, TypedObjectId::Staff(_))
            {
                continue;
            }
            let half = stroke.thickness.0 * 0.5;
            ink.entry(*s).or_default().push((
                stroke.provenance.source,
                [
                    stroke.from.x.0.min(stroke.to.x.0) - half,
                    stroke.from.y.0.min(stroke.to.y.0) - half,
                    stroke.from.x.0.max(stroke.to.x.0) + half,
                    stroke.from.y.0.max(stroke.to.y.0) + half,
                ],
            ));
        }
        for (text, s) in texts.iter().zip(&text_system) {
            if let Some(s) = s {
                ink.entry(*s).or_default().push((
                    text.provenance.source,
                    rect(text.origin.x.0, text.origin.y.0, &text.bounds),
                ));
            }
        }
        for (i, ..) in &gliss_words {
            if let Some(s) = line_runs[*i].1 {
                let run = &line_runs[*i].0;
                ink.entry(s).or_default().push((
                    run.provenance.source,
                    rect(run.origin.x.0, run.origin.y.0, &run.bounds),
                ));
            }
        }
        for &(i, from, to, source, [w, h], _) in &gliss_words {
            let Some(s) = line_runs[i].1 else {
                continue;
            };
            let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
            let length = (dx * dx + dy * dy).sqrt();
            if length <= 0.0 {
                continue;
            }
            let (ux, uy) = (dx / length, dy / length);
            let ink = ink.get(&s).map_or(&[][..], Vec::as_slice);
            let run = &line_runs[i].0;
            let at = |dx: f32, dy: f32| rect(run.origin.x.0 + dx, run.origin.y.0 + dy, &run.bounds);
            // The words as turned to the line: a box `w` along it and `h`
            // across, centred where their run's box is, meeting an ink box
            // unless one of the four axes parts them.
            let meets = |b: [f32; 4]| {
                let (cx, cy) = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
                ink.iter().any(|(src, i)| {
                    *src != source
                        && [(1.0, 0.0), (0.0, 1.0), (ux, uy), (-uy, ux)].iter().all(
                            |&(ax, ay): &(f32, f32)| {
                                let c = cx * ax + cy * ay;
                                let r = 0.5 * w * (ux * ax + uy * ay).abs()
                                    + 0.5 * h * (-uy * ax + ux * ay).abs();
                                let corners = [
                                    i[0] * ax + i[1] * ay,
                                    i[0] * ax + i[3] * ay,
                                    i[2] * ax + i[1] * ay,
                                    i[2] * ax + i[3] * ay,
                                ];
                                let lo = corners.iter().copied().fold(f32::INFINITY, f32::min);
                                let hi = corners.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                                c - r < hi && lo < c + r
                            },
                        )
                })
            };
            if !meets(at(0.0, 0.0)) {
                continue;
            }
            // Which side of the line the words stand on, and how far along.
            let b = at(0.0, 0.0);
            let (cx, cy) = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
            let side = if (cx - from.x.0) * -uy + (cy - from.y.0) * ux >= 0.0 {
                1.0
            } else {
                -1.0
            };
            let along = (cx - from.x.0) * ux + (cy - from.y.0) * uy;
            let half = w * 0.5;
            let within =
                |t: f32| length <= 2.0 * half || (t - half >= -1e-3 && t + half <= length + 1e-3);
            let place = (0..=4)
                .flat_map(|m| {
                    (0..=16).map(move |k| {
                        let step =
                            0.25 * ((k + 1) / 2) as f32 * if k % 2 == 1 { 1.0 } else { -1.0 };
                        (0.25 * m as f32, if k == 0 { 0.0 } else { step })
                    })
                })
                .find(|&(off, slide)| {
                    within(along + slide)
                        && !meets(at(
                            ux * slide - uy * off * side,
                            uy * slide + ux * off * side,
                        ))
                });
            if let Some((off, slide)) = place {
                let run = &mut line_runs[i].0;
                run.origin = Point::new(
                    run.origin.x.0 + ux * slide - uy * off * side,
                    run.origin.y.0 + uy * slide + ux * off * side,
                );
            }
        }
    }
    let mut appended_text_bands = Vec::new();
    for (i, ((run, system), band)) in line_runs.into_iter().zip(line_run_bands).enumerate() {
        if dropped.contains(&i) {
            continue;
        }
        texts.push(run);
        text_system.push(system);
        appended_text_bands.push(band);
    }

    // ---- Staff groups and the systemic barline ------------------------------
    // A system of two or more staves opens with a line joining them; each
    // staff group marks its staves left of that line (a brace, a bracket, a
    // thin sub-bracket) and, unless choral, joins its barlines from staff to
    // staff. All of it is drawn here, where each system's staves stand.
    let mut groups_of: Vec<Vec<&GroupSpan>> = vec![Vec::new(); input.regions.len()];
    for group in &input.staff_groups {
        if let Some(groups) = groups_of.get_mut(group.region) {
            groups.push(group);
        }
    }
    let ink = GlyphStyle { rgba: 0x0000_00ff };
    let mut added: Vec<(Stroke, usize)> = Vec::new();
    for (s, plan) in systems.iter().enumerate() {
        let region = &input.regions[plan.region];
        let margin = VerticalBandId(region.provenance.stable_id.0);
        let marks: BTreeMap<StaffId, &StaffAgg> = staff_marks
            .range((s, StaffId::from_raw(0))..=(s, StaffId::from_raw(u128::MAX)))
            .map(|(&(_, staff), agg)| (staff, agg))
            .collect();
        let Some(left) = marks.values().map(|a| a.min_x).reduce(f32::min) else {
            continue;
        };
        let sign = |element: u128, source: &Provenance| {
            Provenance::synthesized(
                source.source,
                SynthesisKind::Registered(GROUP_SIGN_SYNTHESIS),
                SynthesisInstanceKey((plan.local as u128) << 16 | element),
                source.dependencies.clone(),
            )
        };
        let mut line = |provenance: Provenance, from: Point, to: Point, thickness: f32| {
            added.push((
                Stroke {
                    provenance,
                    from,
                    to,
                    thickness: StaffSpace(thickness),
                    layer: 0,
                    style: ink,
                    vertical_band: margin,
                },
                s,
            ));
        };
        if marks.len() >= 2 {
            let bottom = marks
                .values()
                .map(|a| a.min_y)
                .fold(f32::INFINITY, f32::min);
            let top = marks
                .values()
                .map(|a| a.max_y)
                .fold(f32::NEG_INFINITY, f32::max);
            line(
                sign(0, &region.provenance),
                Point::new(left, bottom),
                Point::new(left, top),
                THIN_BARLINE,
            );
        }
        for (g, group) in groups_of[plan.region].iter().enumerate() {
            let present: Vec<&StaffAgg> = group
                .staves
                .iter()
                .filter_map(|st| marks.get(st).copied())
                .collect();
            let (Some(bottom), Some(top)) = (
                present.iter().map(|a| a.min_y).reduce(f32::min),
                present.iter().map(|a| a.max_y).reduce(f32::max),
            ) else {
                continue;
            };
            let element = |k: u128| (g as u128 + 1) << 8 | k;
            match group.kind {
                GroupSign::Brace => {
                    if let Some(m) = metrics("brace") {
                        let b = m.bounding_box();
                        let scale_y = (top - bottom) / (b.top.0 - b.bottom.0).max(f32::EPSILON);
                        let scale_x = scale_y.sqrt().clamp(1.0, 2.5);
                        let x = left - GROUP_SIGN_GAP - b.right.0 * scale_x;
                        glyphs.push(ResolvedGlyph {
                            provenance: sign(element(0), &group.provenance),
                            glyph: GlyphReference::borrowed("brace"),
                            position: Point::new(x, bottom),
                            transform: Some(Transform2D {
                                matrix: [[scale_x, 0.0, 0.0], [0.0, scale_y, 0.0], [0.0, 0.0, 1.0]],
                            }),
                            bounding_box: b,
                            style: ink,
                            layer: 0,
                        });
                        glyph_system.push(Some(s));
                        appended_bands.push(margin);
                    }
                }
                GroupSign::Bracket => {
                    let x = left - GROUP_SIGN_GAP - BRACKET_THICKNESS / 2.0;
                    line(
                        sign(element(0), &group.provenance),
                        Point::new(x, bottom),
                        Point::new(x, top),
                        BRACKET_THICKNESS,
                    );
                    for (k, name, y) in [(1u128, "bracketTop", top), (2, "bracketBottom", bottom)] {
                        if let Some(m) = metrics(name) {
                            glyphs.push(ResolvedGlyph {
                                provenance: sign(element(k), &group.provenance),
                                glyph: GlyphReference::borrowed(name),
                                position: Point::new(x - BRACKET_THICKNESS / 2.0, y),
                                transform: None,
                                bounding_box: m.bounding_box(),
                                style: ink,
                                layer: 0,
                            });
                            glyph_system.push(Some(s));
                            appended_bands.push(margin);
                        }
                    }
                }
                GroupSign::SubBracket => {
                    let x = left - GROUP_SIGN_GAP;
                    line(
                        sign(element(0), &group.provenance),
                        Point::new(x, bottom),
                        Point::new(x, top),
                        THIN_BARLINE,
                    );
                    for (k, y) in [(1u128, top), (2, bottom)] {
                        line(
                            sign(element(k), &group.provenance),
                            Point::new(x, y),
                            Point::new(x + SUB_BRACKET_HOOK, y),
                            THIN_BARLINE,
                        );
                    }
                }
            }
            if !group.joined || present.len() < 2 {
                continue;
            }
            // Each barline of a staff of the group continues down to the
            // next staff below it in the group.
            let mut order: Vec<(StaffId, &StaffAgg)> = group
                .staves
                .iter()
                .filter_map(|st| marks.get(st).map(|a| (*st, *a)))
                .collect();
            order.sort_by(|a, b| b.1.max_y.total_cmp(&a.1.max_y));
            for pair in order.windows(2) {
                let ((upper, above), (_, below)) = (pair[0], pair[1]);
                for (gi, glyph) in input.glyphs.iter().enumerate() {
                    if glyph_system[gi] != Some(s) || glyph_staff_of[gi] != Some(upper) {
                        continue;
                    }
                    let name = glyph.glyph.as_str();
                    let resolved = &glyphs[gi];
                    let x = resolved.position.x.0;
                    for (k, (dx, thickness)) in barline_lines(name, &glyph.bounding_box)
                        .into_iter()
                        .enumerate()
                    {
                        let provenance = Provenance::synthesized(
                            glyph.provenance.source,
                            SynthesisKind::Registered(JOINED_BARLINE_SYNTHESIS),
                            SynthesisInstanceKey(k as u128),
                            vec![group.provenance.source],
                        );
                        line(
                            provenance,
                            Point::new(x + dx, below.max_y),
                            Point::new(x + dx, above.min_y),
                            thickness,
                        );
                    }
                }
            }
        }
    }

    for (stroke, s) in added {
        strokes.push(stroke);
        stroke_system.push(Some(s));
    }

    // ---- Per-system primitive ownership (W1) -------------------------------
    // The partition already exists in `glyph_system`/`stroke_system`/
    // `curve_system` above; this just stops discarding it. For each flat
    // array, every index lands in exactly one system's list or in `unowned`
    // (pin 4: a total, disjoint partition, tested in this module below).
    let mut owned: Vec<PrimitiveIndices> = (0..systems.len())
        .map(|_| PrimitiveIndices::default())
        .collect();
    let mut unowned = PrimitiveIndices::default();
    for (i, system) in glyph_system.iter().enumerate() {
        match system {
            Some(s) => owned[*s].glyphs.push(i as u32),
            None => unowned.glyphs.push(i as u32),
        }
    }
    for (i, system) in stroke_system.iter().enumerate() {
        match system {
            Some(s) => owned[*s].strokes.push(i as u32),
            None => unowned.strokes.push(i as u32),
        }
    }
    for (i, system) in curve_system.iter().enumerate() {
        match system {
            Some(s) => owned[*s].curves.push(i as u32),
            None => unowned.curves.push(i as u32),
        }
    }
    for (i, system) in text_system.iter().enumerate() {
        match system {
            Some(s) => owned[*s].texts.push(i as u32),
            None => unowned.texts.push(i as u32),
        }
    }

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
                owned[s].clone(),
                lead_w[s],
                plan.slots
                    .first()
                    .map_or(0.0, |&i| shift_of(region_slots[plan.region][i].id)),
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
        appended_bands,
        appended_text_bands,
        strokes,
        curves,
        texts,
        pages,
        decisions,
        system_start_slots,
        page_start_slots,
        glyph_system,
        stroke_system,
        curve_system,
        region_of_system: systems.iter().map(|plan| plan.region).collect(),
        unowned,
        added: added_systems,
        overfull,
        short_glissandi,
    }
}

/// Spreads one page by `space` (D109 Q3): each of its gaps, the page's top
/// border fixed, a gap between systems weighing `SPREAD_SYSTEM` and capped at
/// `MAX_SYSTEM_SPREAD`, a gap at a bracket's or a brace's edge weighing
/// `SPREAD_BRACKET` or `SPREAD_BRACE`, one inside a brace capped at
/// `MAX_BRACE_SPREAD`, as MuseScore classes them (`distributeStaves`,
/// `VerticalGapData`); a gap between systems first raised toward the widest
/// gap between staves of either system beside it (Gould, p. 488), as far as
/// the space allows, then `vertical::spread`. Each staff below a gap that
/// grows sinks with it, and each system below.
fn spread_page(
    page: &[usize],
    space: f32,
    input: &ConstrainedLayoutIR,
    systems: &[SystemPlan],
    system_staves: &[Vec<(StaffId, f32, f32)>],
    staff_shift: &mut BTreeMap<(usize, StaffId), f32>,
    placements: &mut [Placement],
) {
    use crate::vertical::{
        spread, SpreadGap, MAX_BRACE_SPREAD, MAX_STAFF_SPREAD, MAX_SYSTEM_SPREAD, PAGE_BORDER,
        SPREAD_BRACE, SPREAD_BRACKET, SPREAD_SYSTEM,
    };
    use epiphany_layout_ir::GroupSign;
    let shift = |s: usize, st: StaffId| staff_shift.get(&(s, st)).copied().unwrap_or(0.0);
    let mut gaps = vec![SpreadGap {
        distance: PAGE_BORDER,
        factor: 1.0,
        cap: MAX_STAFF_SPREAD,
        fixed: true,
    }];
    // Each gap between systems, by its index in `gaps`, and the widest gap
    // between staves of each system.
    let mut between: Vec<(usize, usize, usize)> = Vec::new();
    let mut widest: BTreeMap<usize, f32> = BTreeMap::new();
    for (i, &s) in page.iter().enumerate() {
        let staves = &system_staves[s];
        if i > 0 {
            let u = page[i - 1];
            let (Some(&(last, bottom, _)), Some(&(first, _, top))) =
                (system_staves[u].last(), staves.first())
            else {
                continue;
            };
            let distance = (bottom - shift(u, last) + placements[u].dy)
                - (top - shift(s, first) + placements[s].dy);
            between.push((gaps.len(), u, s));
            gaps.push(SpreadGap {
                distance,
                factor: SPREAD_SYSTEM,
                cap: MAX_SYSTEM_SPREAD / SPREAD_SYSTEM,
                fixed: false,
            });
        }
        let groups: Vec<_> = input
            .staff_groups
            .iter()
            .filter(|g| g.region == systems[s].region)
            .collect();
        for w in staves.windows(2) {
            let ((upper, bottom, _), (lower, _, top)) = (w[0], w[1]);
            let distance = (bottom - shift(s, upper)) - (top - shift(s, lower));
            let edge = |kind: GroupSign| {
                groups.iter().any(|g| {
                    g.kind == kind && g.staves.contains(&upper) != g.staves.contains(&lower)
                })
            };
            let inside_brace = groups.iter().any(|g| {
                g.kind == GroupSign::Brace && g.staves.contains(&upper) && g.staves.contains(&lower)
            });
            let mut factor = 1.0_f32;
            if edge(GroupSign::Bracket) {
                factor = factor.max(SPREAD_BRACKET);
            }
            if edge(GroupSign::Brace) {
                factor = factor.max(SPREAD_BRACE);
            }
            let cap = if inside_brace && !edge(GroupSign::Brace) {
                MAX_BRACE_SPREAD / factor
            } else {
                MAX_STAFF_SPREAD
            };
            let entry = widest.entry(s).or_insert(f32::NEG_INFINITY);
            *entry = entry.max(distance);
            gaps.push(SpreadGap {
                distance,
                factor,
                cap,
                fixed: false,
            });
        }
    }
    // Gould first: a gap between systems wider than the widest gap between
    // staves beside it, as far as the space allows, each short of it raised
    // by the same share of what it lacks.
    let lack: Vec<f32> = between
        .iter()
        .map(|&(g, u, s)| {
            let beside = widest
                .get(&u)
                .copied()
                .unwrap_or(f32::NEG_INFINITY)
                .max(widest.get(&s).copied().unwrap_or(f32::NEG_INFINITY))
                .min(MAX_SYSTEM_SPREAD);
            (beside - gaps[g].distance).max(0.0)
        })
        .collect();
    let total: f32 = lack.iter().sum();
    let share = if total > 0.0 {
        (space / total).min(1.0)
    } else {
        0.0
    };
    let original: Vec<f32> = gaps.iter().map(|g| g.distance).collect();
    for (&(g, _, _), &l) in between.iter().zip(&lack) {
        gaps[g].distance += l * share;
    }
    let spread = spread(&gaps, space - total * share);
    let added: Vec<f32> = spread.iter().zip(&original).map(|(d, o)| d - o).collect();
    // Each staff below a gap that grew sinks with it, and each system below.
    let mut k = 1;
    let mut above = 0.0_f32;
    for (i, &s) in page.iter().enumerate() {
        if i > 0 && between.iter().any(|&(_, _, lower)| lower == s) {
            above += added[k];
            k += 1;
        }
        placements[s].dy -= above;
        let mut within = 0.0_f32;
        for &(st, _, _) in system_staves[s].iter().skip(1) {
            within += added[k];
            k += 1;
            *staff_shift.entry((s, st)).or_insert(0.0) += within;
        }
        above += within;
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
    bounds: &Bounds,
) -> BTreeSet<SpringSlotId> {
    let &Bounds {
        width_limit,
        left,
        lead,
        room,
        held,
    } = bounds;
    let mut automatic = BTreeSet::new();
    if !width_limit.is_finite() || width_limit <= 0.0 || slots.is_empty() {
        return automatic; // unbounded width: nothing wraps
    }
    // Measure-boundary positions in slot-index space: region start, each slot
    // that follows a barline, region end. `forced[k]` marks a boundary
    // carrying a break requirement (the DP may not span it). The region end is
    // a boundary.
    let mut pts: Vec<usize> = vec![0];
    let mut forced: Vec<bool> = vec![false];
    for (i, slot) in slots.iter().enumerate() {
        if opens_measure(slots, i) {
            pts.push(i);
            forced.push(reqs.contains_key(&slot.id));
        }
    }
    pts.push(slots.len());
    forced.push(true);
    let n = pts.len(); // n - 1 measures between the n boundaries

    // A system spanning boundaries [a, b): its ink extent over slots
    // `[pts[a] .. pts[b])`, from the staff lines' start for the first, and a
    // later system's lead before it, with the room a tie continued into its
    // first slot needs there.
    let width = |a: usize, b: usize| -> f32 {
        let range = &slots[pts[a]..pts[b]];
        let start = if a == 0 { left } else { f32::INFINITY };
        let lo = range.iter().map(|s| s.lo).fold(start, f32::min);
        let hi = range.iter().map(|s| s.hi).fold(f32::NEG_INFINITY, f32::max);
        (hi - lo).max(0.0) + if a > 0 { lead + room[pts[a]] } else { 0.0 }
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
            let bad = if held.contains(&(pts[a], pts[b])) {
                0.0 // a system the breaks fix, compressed to the width
            } else if w <= width_limit {
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

/// Whether slot `i` opens a measure a system may start with: it follows a
/// barline that is not the region's final one. A barline ends its measure, so
/// a system ends on a barline and the next begins after it.
fn opens_measure(slots: &[SlotInfo], i: usize) -> bool {
    i > 0 && slots[i - 1].barline && !slots[i - 1].final_barline
}

/// What a region's systems must fit: the content width; where the region's
/// staff lines start, which its first system's width counts from; the widest
/// lead a later system opens with; by slot, the room a tie continued into a
/// system opening there needs; and the systems the region's breaks fix that
/// are kept whole however wide (`fixed_systems`), by their slots `[lo, hi)`.
struct Bounds<'a> {
    width_limit: f32,
    left: f32,
    lead: f32,
    room: &'a [f32],
    held: &'a BTreeSet<(usize, usize)>,
}

/// The systems a region's breaks fix (the slots `[lo, hi)` between two of
/// its break requirements, or between one and the region's start or end, in
/// a region with one) that hold more than the line at their natural width:
/// `held`, whose floor fits the line, which the search keeps whole and
/// casting compresses (`compress_system`); and `overflowing`, whose floor
/// holds more than the line too, by how much, which the search breaks as it
/// would any other.
struct Fixed {
    held: BTreeSet<(usize, usize)>,
    overflowing: Vec<(usize, usize, f32)>,
}

/// A region's [`Fixed`] systems: those the break search, measuring a later
/// system with the widest lead its region opens one with, finds wider than
/// the line, held where their floor from `skyline` at a scale of 0 fits it
/// with the lead they open with (`opening_lead`, by the slot they open at).
fn fixed_systems(
    slots: &[SlotInfo],
    reqs: &BTreeMap<SpringSlotId, Vec<BreakReq>>,
    bounds: &Bounds,
    skyline: &Skyline,
    opening_lead: &dyn Fn(SpringSlotId) -> f32,
) -> Fixed {
    let mut fixed = Fixed {
        held: BTreeSet::new(),
        overflowing: Vec::new(),
    };
    let &Bounds {
        width_limit,
        left,
        lead,
        room,
        ..
    } = bounds;
    let mut ends: Vec<usize> = (0..slots.len())
        .filter(|&i| opens_measure(slots, i) && reqs.contains_key(&slots[i].id))
        .collect();
    if ends.is_empty() || !width_limit.is_finite() || width_limit <= 0.0 {
        return fixed;
    }
    ends.insert(0, 0);
    ends.push(slots.len());
    for pair in ends.windows(2) {
        let (lo, hi) = (pair[0], pair[1]);
        let range = &slots[lo..hi];
        let Some(first) = range.first() else {
            continue;
        };
        let start = if lo == 0 { left } else { f32::INFINITY };
        let natural = range.iter().map(|s| s.hi).fold(f32::NEG_INFINITY, f32::max)
            - range.iter().map(|s| s.lo).fold(start, f32::min);
        if natural.max(0.0) + if lo > 0 { lead + room[lo] } else { 0.0 } <= width_limit {
            continue;
        }
        let ids: Vec<SpringSlotId> = range.iter().map(|s| s.id).collect();
        let at = skyline.place(&ids, 0.0);
        let start = if lo == 0 {
            left - first.x
        } else {
            f32::INFINITY
        };
        let floor = range
            .iter()
            .zip(&at)
            .map(|(s, a)| a + (s.hi - s.x))
            .fold(f32::NEG_INFINITY, f32::max)
            - range
                .iter()
                .zip(&at)
                .map(|(s, a)| a + (s.lo - s.x))
                .fold(start, f32::min);
        let floor = floor.max(0.0)
            + if lo > 0 {
                opening_lead(first.id) + room[lo]
            } else {
                0.0
            };
        if floor <= width_limit {
            fixed.held.insert((lo, hi));
        } else {
            fixed.overflowing.push((lo, hi, floor - width_limit));
        }
    }
    fixed
}

/// Walks one region's slots, opening a system at each break requirement and at
/// each optimal automatic break (`optimal_breaks`), within `bounds`.
#[allow(clippy::too_many_arguments)]
fn walk_region(
    region: usize,
    slots: &[SlotInfo],
    reqs: &BTreeMap<SpringSlotId, Vec<BreakReq>>,
    origins: &BTreeMap<(u128, bool), EngravingOverrideId>,
    region_source: TypedObjectId,
    bounds: Bounds,
    systems: &mut Vec<SystemPlan>,
    skipped: &mut Vec<EngravingDecision>,
) {
    let Bounds {
        width_limit,
        left,
        lead,
        room,
        held,
    } = bounds;
    // The optimal automatic breaks (a global badness-minimizing partition,
    // bounded by the break requirements); the walk opens a system at each.
    let automatic = optimal_breaks(slots, reqs, &bounds);

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
    let mut chunk_hi = vec![f32::NEG_INFINITY; slots.len()];
    for i in (0..slots.len()).rev() {
        let next = if i + 1 < slots.len() && !opens_measure(slots, i + 1) {
            chunk_hi[i + 1]
        } else {
            f32::NEG_INFINITY
        };
        chunk_hi[i] = slots[i].hi.max(next);
    }
    // Inside a system the breaks fix and the search keeps whole, the net
    // stays out of the way: casting compresses that system to the width.
    let mut kept = vec![false; slots.len()];
    for &(lo, hi) in held {
        for k in kept.iter_mut().take(hi).skip(lo + 1) {
            *k = true;
        }
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
            current_lo = slot.lo.min(left);
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
                || (opens_measure(slots, i)
                    && !kept[i]
                    && chunk_hi[i] - current_lo
                        > width_limit
                            - if local > 0 {
                                lead + room[current[0]]
                            } else {
                                0.0
                            }))
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
        if best.is_none_or(|(_, d)| distance < d) {
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
        if best.is_none_or(|(_, d)| distance < d) {
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

/// Where piece `k` of a stroke split into `n` over systems stands across in
/// its system, from the spaced ends of its piece: an anchored end rides its
/// slot's delta, as it does within one system, so a justified system keeps
/// it at its note; a line of the family (`line_start` its system's
/// first column) carried into a later system starts at that column, the
/// lesser of a space past the lead's ink and the first note
/// (`Measure::firstNoteRestSegmentX`), the lead's gap being under a space,
/// and one going on stops short of its system's end
/// (`endingXForOpenEndedLines`).
fn split_piece_x(
    k: usize,
    n: usize,
    (from, to): (f32, f32),
    p: &Placement,
    anchor: Option<&(SpringSlotId, SpringSlotId)>,
    slot_source_x: &BTreeMap<SpringSlotId, f32>,
    line_start: Option<f32>,
) -> (f32, f32) {
    let slot_dx = |slot: &SpringSlotId| slot_source_x.get(slot).map(|&sx| p.slot_dx(sx));
    let from_x = match line_start {
        Some(x) if k > 0 => x,
        _ => anchor
            .filter(|_| k == 0)
            .and_then(|(start, _)| slot_dx(start))
            .map_or_else(|| p.x(from), |dx| from + dx),
    };
    let to_x = if line_start.is_some() && k + 1 < n {
        p.x(to) - LINE_END_GAP
    } else {
        anchor
            .filter(|_| k + 1 == n)
            .and_then(|(_, end)| slot_dx(end))
            .map_or_else(|| p.x(to), |dx| to + dx)
    };
    (from_x, to_x)
}

/// A trill line's piece where the system sets it: the origins across of the
/// wiggles along it, one in each wiggle's advance, as many as it holds to
/// the nearest (`LineSegment::symbolLine`); the stroke itself then draws
/// nothing, a traced anchor.
fn trill_wiggles(line: &mut Stroke) -> Vec<f32> {
    let advance = metrics(TRILL_WIGGLE).map_or(1.0, |m| m.advance as f32 / 1024.0);
    let (x0, x1) = (line.from.x.0, line.to.x.0);
    let n = ((x1 - x0) / advance).round().max(0.0) as usize;
    line.to = line.from;
    (0..n).map(|i| x0 + advance * i as f32).collect()
}

/// A trill line's wiggle `i` of its piece `piece`, at `at`: synthesized from
/// the line.
fn wiggle(spaced: &Stroke, piece: usize, i: usize, at: Point) -> ResolvedGlyph {
    ResolvedGlyph {
        provenance: Provenance::synthesized(
            spaced.provenance.source,
            SynthesisKind::Registered(TRILL_WIGGLE_SYNTHESIS),
            SynthesisInstanceKey(((piece as u128) << 32) | i as u128),
            spaced.provenance.dependencies.clone(),
        ),
        glyph: GlyphReference::borrowed(TRILL_WIGGLE),
        position: at,
        transform: None,
        bounding_box: metrics(TRILL_WIGGLE)
            .expect("the wiggle is bundled")
            .bounding_box(),
        style: GlyphStyle { rgba: 0x0000_00ff },
        layer: 0,
    }
}

/// The registry id for a trill line's wiggles, synthesized from the line,
/// keyed by piece and place.
const TRILL_WIGGLE_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x5452_4957_4947_4C45); // "TRIWIGLE"

/// A hyphen's or melisma line's piece where the system sets it: a hyphen's
/// as dashes, one in each `LYRIC_DASH_MAX_DISTANCE` of it, each between
/// `LYRIC_DASH_MIN` and `LYRIC_DASH_MAX` long and centred in its share, the
/// first in `line`'s place and the rest returned; one too short for its
/// least dash grows to it about its centre, or, `before` a syllable opening
/// its system, back from its end (`LyricsLayout::layoutDashes`). A melisma
/// line's piece is kept only where it is at least `LYRIC_MELISMA_MIN` long,
/// and no ink otherwise.
fn lyric_line(line: &mut Stroke, kind: Option<&FamilyLine>, before: bool) -> Vec<Stroke> {
    let (x0, x1, y) = (line.from.x.0, line.to.x.0, line.from.y.0);
    let span = x1 - x0;
    match kind {
        Some(FamilyLine::Melisma) => {
            if span < LYRIC_MELISMA_MIN - LYRIC_MELISMA_TOLERANCE {
                line.to = line.from;
            }
            Vec::new()
        }
        Some(FamilyLine::Hyphen) => {
            let n = (span / LYRIC_DASH_MAX_DISTANCE).ceil().max(1.0);
            if before && span < LYRIC_DASH_MIN {
                line.from = Point::new(x1 - LYRIC_DASH_MIN, y);
                return Vec::new();
            }
            let share = span / n;
            let length = share.clamp(LYRIC_DASH_MIN, LYRIC_DASH_MAX);
            let dash = |k: f32| {
                let centre = x0 + share * (k + 0.5);
                Stroke {
                    from: Point::new(centre - length * 0.5, y),
                    to: Point::new(centre + length * 0.5, y),
                    ..line.clone()
                }
            };
            let more = (1..n as usize).map(|k| dash(k as f32)).collect();
            *line = dash(0.0);
            more
        }
        _ => Vec::new(),
    }
}

/// A dashed line's piece where the system sets it (X4d.1), as MuseScore
/// distributes a dash pattern over a line (`distributedDashPattern`): as
/// many pairs of a dash `dash` long and a gap `gap` long as its length holds
/// whole, at least one, and one dash more, the gaps shared out evenly so its
/// first and last dash stand at its ends; the first in `line`'s place and the
/// rest returned. A piece shorter than two dashes is drawn solid.
fn dash_line(line: &mut Stroke, dashes: LineDashes) -> Vec<Stroke> {
    let (from, to) = (line.from, line.to);
    let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 2.0 * dashes.dash {
        return Vec::new();
    }
    let pairs = (length / (dashes.dash + dashes.gap)).floor().max(1.0);
    let gap = (length - dashes.dash * (pairs + 1.0)) / pairs;
    let at = |t: f32| Point::new(from.x.0 + dx * t / length, from.y.0 + dy * t / length);
    let dash = |k: f32| {
        let start = k * (dashes.dash + gap);
        Stroke {
            from: at(start),
            to: at(start + dashes.dash),
            ..line.clone()
        }
    };
    let more = (1..=pairs as usize).map(|k| dash(k as f32)).collect();
    *line = dash(0.0);
    more
}

/// A glissando's pieces' heights where its systems set them (X4d.1): the
/// line's two ends' heights `y0` and `y1`, and each piece's ends across, in
/// order; each piece's ends' heights, the gradient kept over the pieces'
/// whole length, as MuseScore interpolates a note-anchored line over a system
/// break (`TLayout::layoutNoteAnchoredLine`) and Gould keeps the gradient of
/// the interval (*Behind Bars*, p. 142).
fn sloped(y0: f32, y1: f32, pieces: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let total: f32 = pieces.iter().map(|(a, b)| (b - a).max(0.0)).sum();
    let mut done = 0.0f32;
    pieces
        .iter()
        .map(|(a, b)| {
            let at = |d: f32| {
                if total > 0.0 {
                    y0 + (y1 - y0) * d / total
                } else {
                    y0
                }
            };
            let from = at(done);
            done += (b - a).max(0.0);
            (from, at(done))
        })
        .collect()
}

/// A glissando's pieces' heights where its system or systems set it (X4d.1):
/// `heads` its heads' centres and `line` its stroke in the projection's
/// frame, `xs` each piece's ends across where they stand now, in order. Its
/// ends keep their distance across from their heads, so the heads stand that
/// far before its first piece's start and after its last piece's end; the
/// line joining them, its gradient kept over the pieces between ([`sloped`]),
/// gives each piece's heights.
fn glissando_heights(heads: [Point; 2], line: &Stroke, xs: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let (Some(first), Some(last)) = (xs.first(), xs.last()) else {
        return Vec::new();
    };
    let lead = line.from.x.0 - heads[0].x.0;
    let tail = heads[1].x.0 - line.to.x.0;
    let mut whole: Vec<(f32, f32)> = xs.to_vec();
    let n = whole.len();
    whole[0].0 = first.0 - lead;
    whole[n - 1].1 = last.1 + tail;
    let mut ys = sloped(heads[0].y.0, heads[1].y.0, &whole);
    let within = |(a, b): (f32, f32), (ya, yb): (f32, f32), x: f32| {
        if b > a {
            ya + (yb - ya) * (x - a) / (b - a)
        } else {
            ya
        }
    };
    ys[0].0 = within(whole[0], ys[0], first.0);
    ys[n - 1].1 = within(whole[n - 1], ys[n - 1], last.1);
    ys
}

/// A line's words' width and height before they are turned to it.
fn words_extent(words: Option<(&ResolvedText, f32)>) -> [f32; 2] {
    words.map_or([0.0, 0.0], |(t, _)| {
        [
            t.bounds.right.0 - t.bounds.left.0,
            t.bounds.top.0 - t.bounds.bottom.0,
        ]
    })
}

/// The stable id of the run a line draws in its piece `piece` in a role.
fn line_run_id(line: &Stroke, piece: usize, role: u128) -> LayoutObjectId {
    Provenance::synthesized(
        line.provenance.source,
        SynthesisKind::Registered(LINE_PIECE_SYNTHESIS),
        SynthesisInstanceKey(((piece as u128) << 8) | role),
        line.provenance.dependencies.clone(),
    )
    .stable_id
}

/// A run a line draws in its piece `piece` (X4d.1), synthesized from the
/// line, keyed by the piece and the run's role there.
fn line_run(template: &ResolvedText, line: &Stroke, piece: usize, role: u128) -> ResolvedText {
    ResolvedText {
        provenance: Provenance::synthesized(
            line.provenance.source,
            SynthesisKind::Registered(LINE_PIECE_SYNTHESIS),
            SynthesisInstanceKey(((piece as u128) << 8) | role),
            line.provenance.dependencies.clone(),
        ),
        ..template.clone()
    }
}

/// Whether a glissando's piece, as its system sets it, is too short for its
/// notes (X4d fix round 2): shorter than its line is thick, or turned back,
/// its end before its start. Such a piece is left undrawn and its glissando
/// named, where it had drawn a speck or a stroke backward.
fn too_short(piece: &Stroke) -> bool {
    let (dx, dy) = (piece.to.x.0 - piece.from.x.0, piece.to.y.0 - piece.from.y.0);
    dx <= 0.0 || (dx * dx + dy * dy).sqrt() < piece.thickness.0
}

/// A glissando's words drawn in a piece: their index among the line runs,
/// the piece's ends, the line's source, the words' width and height before
/// they were turned to the line, and the line's parallel set, its rank there
/// and the piece (X4d fix round 2).
type GlissWords = (
    usize,
    Point,
    Point,
    TypedObjectId,
    [f32; 2],
    Option<(u128, usize, usize)>,
);

/// How far after the ink of its system's lead a glissando's later piece
/// opens: as far as its ends stand from its chords' ink.
const GLISSANDO_LEAD_GAP: f32 = 0.25;

/// How far a dynamic stands clear of a barline running through the staves
/// at it (`DynamicsLayout::manageBarlineCollisions`).
const DYNAMIC_BARLINE_CLEARANCE: f32 = 0.25;

/// The registry id for the runs a line draws piece by piece: its
/// continuation (role 1), a glissando's words (2) and wiggles (3).
const LINE_PIECE_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x4C49_4E45_5049_4543); // "LINEPIEC"

/// A glissando piece's runs where its system sets it (X4d.1): wavy, its
/// wiggles centred along it, as many as fit whole, turned to its slope, the
/// stroke itself then drawing nothing; and its words centred along it where
/// they are shorter than it, their baseline `raise` over it
/// (`TDraw::draw(GlissandoSegment)`).
fn glissando_runs(
    line: &mut Stroke,
    wavy: bool,
    words: Option<(&ResolvedText, f32)>,
    piece: usize,
) -> Vec<ResolvedText> {
    let (from, to) = (line.from, line.to);
    let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.0 {
        return Vec::new();
    }
    let angle = dy.atan2(dx);
    let (ux, uy) = (dx / length, dy / length);
    let along =
        |t: f32, up: f32| Point::new(from.x.0 + ux * t - uy * up, from.y.0 + uy * t + ux * up);
    let mut runs = Vec::new();
    if wavy {
        let advance = 0.96f32;
        let n = (length / advance).floor() as usize;
        let start = (length - n as f32 * advance) * 0.5;
        // The wiggle's box from 0 to 0.444 high: its middle on the line.
        let symbols: Vec<epiphany_layout_ir::Symbol> = (0..n)
            .map(|i| epiphany_layout_ir::Symbol {
                codepoint: GLISSANDO_WIGGLE,
                at: epiphany_layout_ir::SymbolPlace::Origin(Point::new(advance * i as f32, -0.222)),
                scale: 1.0,
                turn: epiphany_layout_ir::Turn::None,
            })
            .collect();
        if n > 0 {
            let run = epiphany_layout_ir::symbol_run(
                Provenance::synthesized(
                    line.provenance.source,
                    SynthesisKind::Registered(LINE_PIECE_SYNTHESIS),
                    SynthesisInstanceKey(((piece as u128) << 8) | 3),
                    line.provenance.dependencies.clone(),
                ),
                &symbols,
                along(start, 0.0),
            );
            runs.push(epiphany_layout_ir::rotated(run, angle));
        }
        line.to = line.from;
    }
    if let Some((template, raise)) = words {
        let width = template.bounds.right.0 - template.bounds.left.0;
        if width < length {
            let mut run = line_run(template, line, piece, 2);
            run.origin = along((length - width) * 0.5 - template.bounds.left.0, raise);
            runs.push(epiphany_layout_ir::rotated(run, angle));
        }
    }
    runs
}

/// A hyphen's further dash's provenance: synthesized from its first, keyed
/// by the piece it stands in and its place there.
fn extra_dash(spaced: &Stroke, piece: usize, k: usize) -> Provenance {
    Provenance::synthesized(
        spaced.provenance.source,
        SynthesisKind::Registered(SYSTEM_CONTINUATION_SYNTHESIS),
        continuation_instance_key(
            spaced.provenance.stable_id,
            (1 << 20) + (piece as u32) * 1024 + k as u32 + 1,
        ),
        spaced.provenance.dependencies.clone(),
    )
}

/// The heights at which piece `k` of `n` of a hairpin's leg (`sign` 1 for
/// the upper leg, -1 for the lower) starts and ends over a system break, in
/// the frame of its shape's axis, as MuseScore draws each piece: to its full
/// opening at its wide end, and to its continuation's at a narrow end the
/// break leaves open.
fn hairpin_piece_ys(shape: HairpinShape, sign: f32, k: usize, n: usize) -> (f32, f32) {
    let (full, cont) = (HAIRPIN_HEIGHT * 0.5, HAIRPIN_CONT_HEIGHT * 0.5);
    let (open, last) = (k > 0, k + 1 < n);
    if shape.crescendo {
        (
            shape.axis + sign * if open { cont } else { 0.0 },
            shape.axis + sign * full,
        )
    } else {
        (
            shape.axis + sign * full,
            shape.axis + sign * if last { cont } else { 0.0 },
        )
    }
}

/// A hairpin's piece stands at least its least length, its end moved out
/// where the system set it shorter (`TLayout::layoutHairpinSegment`).
fn keep_hairpin_length(leg: &mut Stroke) {
    let least = leg.from.x.0 + HAIRPIN_MIN_LENGTH;
    if leg.to.x.0 < least {
        leg.to = Point::new(least, leg.to.y.0);
    }
}

/// The family member of a group's first piece.
fn first_of(pieces: &[crate::family::Piece], members: &[usize]) -> usize {
    pieces[members[0]].family
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

/// A tie cut at a system break into two half-arcs, each a complete small arc
/// ending level with its note's end: the first, of the tie's own lift, from
/// its start to `edge_a`, the first system's right content edge; the second,
/// a tie of its own length, from `from_b`, clear of the second system's lead,
/// to its end.
fn half_arcs(cp: [Point; 4], edge_a: f32, from_b: f32) -> ([Point; 4], [Point; 4]) {
    let lift = cp[1].y.0 - cp[0].y.0;
    let (x0, y0) = (cp[0].x.0, cp[0].y.0);
    let x3 = edge_a.max(x0);
    let span = x3 - x0;
    (
        [
            Point::new(x0, y0),
            Point::new(x0 + span * 0.25, y0 + lift),
            Point::new(x3 - span * 0.25, y0 + lift),
            Point::new(x3, y0),
        ],
        tie_arc(from_b.min(cp[3].x.0), cp[3].x.0, cp[3].y.0, lift > 0.0),
    )
}

/// The right ink edge, each mapped by `x`, of the columns of `plan` that
/// stand before `end`'s.
fn ink_before(
    plan: &SystemPlan,
    slots: &[SlotInfo],
    end: SpringSlotId,
    x: impl Fn(&SlotInfo) -> f32,
) -> Option<f32> {
    plan.slots
        .iter()
        .map(|&i| &slots[i])
        .take_while(|info| info.id != end)
        .map(x)
        .reduce(f32::max)
}

/// The vertical lines of a barline glyph, as `(x offset from its origin,
/// thickness)`: a single barline's thin line; a final barline's thin and
/// thick; a repeat sign's thick and thin on the side its dots do not take.
/// Any other glyph has none.
pub(crate) fn barline_lines(name: &str, b: &epiphany_layout_ir::BoundingBox) -> Vec<(f32, f32)> {
    let (left, right) = (b.left.0, b.right.0);
    let thin = THIN_BARLINE / 2.0;
    let thick = BRACKET_THICKNESS / 2.0;
    match name {
        "barlineSingle" => vec![((left + right) / 2.0, THIN_BARLINE)],
        "barlineFinal" => vec![
            (left + thin, THIN_BARLINE),
            (right - thick, BRACKET_THICKNESS),
        ],
        "repeatRight" => vec![
            (
                right - 2.0 * thick - BARLINE_SEPARATION - thin,
                THIN_BARLINE,
            ),
            (right - thick, BRACKET_THICKNESS),
        ],
        "repeatLeft" => vec![
            (left + thick, BRACKET_THICKNESS),
            (left + 2.0 * thick + BARLINE_SEPARATION + thin, THIN_BARLINE),
        ],
        "repeatRightLeft" => {
            let middle = (left + right) / 2.0;
            vec![
                (middle, BRACKET_THICKNESS),
                (middle - thick - BARLINE_SEPARATION - thin, THIN_BARLINE),
                (middle + thick + BARLINE_SEPARATION + thin, THIN_BARLINE),
            ]
        }
        _ => Vec::new(),
    }
}

/// Whether `a` is at or before `b` (times of one kind; across kinds, never).
fn at_or_before(a: &TimePoint, b: &TimePoint) -> bool {
    match (a, b) {
        (TimePoint::Musical(a), TimePoint::Musical(b)) => a <= b,
        (TimePoint::WallClock(a), TimePoint::WallClock(b)) => a <= b,
        _ => false,
    }
}

/// The width a system-start lead takes: its ink's right edge and the gap
/// before the system's first column.
fn lead_width(glyphs: &[LeadGlyph]) -> f32 {
    let right = glyphs
        .iter()
        .filter_map(|g| Some(g.x + metrics(g.name.as_str())?.bounding_box().right.0))
        .fold(0.0, f32::max);
    if glyphs.is_empty() {
        0.0
    } else {
        right + LEAD_GAP
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

/// The least share of the content width a region's last system's natural ink
/// fills for it to be justified like the others: MuseScore's default ("last
/// system fill threshold", 30%).
const LAST_SYSTEM_FILL: f32 = 0.3;

/// A system's placement: rigid (translated to the left margin) unless the
/// system JUSTIFIES — narrower than the content width, with a positive slot
/// span, and either not its region's last or a last one whose ink fills at
/// least `LAST_SYSTEM_FILL` of the width — in which case the horizontal
/// slack is spread over its slots' springs (`springs`, each slot's
/// `stretch_factor · preferred_width`; evenly by width when no slot has one)
/// so the system's ink fills the content width (its leftmost ink at the left
/// margin, its rightmost at the right margin). A region's last system shorter
/// than that stays ragged-right, as MuseScore leaves it; a system already at
/// or over width is not compressed into overlap.
#[allow(clippy::too_many_arguments)]
fn justify_system(
    plan: &SystemPlan,
    ext: &Extent,
    base_dx: f32,
    dy: f32,
    region_slots: &[Vec<SlotInfo>],
    region_systems: &[Vec<usize>],
    springs: &BTreeMap<SpringSlotId, f32>,
    width_limit: f32,
) -> Placement {
    let is_last = plan.local + 1 >= region_systems[plan.region].len();
    if !width_limit.is_finite()
        || (is_last && ext.max_x - ext.min_x < LAST_SYSTEM_FILL * width_limit)
    {
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
    // Each interval between consecutive slots takes the slack by the earlier
    // slot's spring; with no spring among them, by its width.
    let members: Vec<&SlotInfo> = plan.slots.iter().map(|&i| &slots[i]).collect();
    let weight = |k: usize| -> f32 {
        springs
            .get(&members[k].id)
            .copied()
            .filter(|w| w.is_finite() && *w > 0.0)
            .unwrap_or(0.0)
    };
    let total: f32 = (0..members.len().saturating_sub(1)).map(weight).sum();
    let mut knots = Vec::with_capacity(members.len());
    let mut taken = 0.0_f32;
    for k in 0..members.len() {
        knots.push((members[k].x, taken));
        if k + 1 < members.len() {
            taken += if total > f32::EPSILON {
                extra * weight(k) / total
            } else {
                extra * (members[k + 1].x - members[k].x) / span
            };
        }
    }
    if let Some(end) = knots.last_mut() {
        end.1 = extra;
    }
    Placement {
        dx: base_dx,
        dy,
        knots: Rc::from(knots),
    }
}

/// A system the breaks fix that holds more than the content width, compressed
/// to it: its slots spaced again ([`Skyline::place`]) with every note column's
/// spring scaled by the one factor in `[0, 1]` at which its first slot to its
/// last spans the width, so the durations keep their ratios but where the
/// floor's distances hold them apart; the lead's, signatures', changes' and
/// barlines' springs kept. The factor is found by halving
/// (`COMPRESSION_STEPS`) and the system set between the last placement too
/// wide and the last that fits, both clear of the floor and so every point
/// between them, at the one that spans the width exactly. Where even the
/// floor spans more, it stays at the floor; where every spring at its full
/// width fits by the floor's distances alone, it is set between that and its
/// natural spacing. `None` for a system the width holds.
fn compress_system(
    plan: &SystemPlan,
    ext: &Extent,
    base_dx: f32,
    dy: f32,
    region_slots: &[Vec<SlotInfo>],
    skyline: &Skyline,
    width_limit: f32,
) -> Option<Placement> {
    let extra = width_limit - (ext.max_x - ext.min_x);
    if !width_limit.is_finite() || extra >= 0.0 || plan.slots.len() < 2 {
        return None;
    }
    let slots = &region_slots[plan.region];
    let members: Vec<&SlotInfo> = plan.slots.iter().map(|&i| &slots[i]).collect();
    let first = members[0].x;
    let ids: Vec<SpringSlotId> = members.iter().map(|m| m.id).collect();
    let natural: Vec<f32> = members.iter().map(|m| m.x - first).collect();
    let end = natural.len() - 1;
    // How much shorter than its natural span a placement sets the system.
    let slack = |at: &[f32]| at[end] - natural[end];
    let full = skyline.place(&ids, 1.0);
    let (wide, fits) = if slack(&full) <= extra {
        (natural.clone(), full)
    } else {
        let floor = skyline.place(&ids, 0.0);
        if slack(&floor) > extra {
            (floor.clone(), floor)
        } else {
            let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
            let (mut fits, mut wide) = (floor, full);
            for _ in 0..COMPRESSION_STEPS {
                let mid = 0.5 * (lo + hi);
                let at = skyline.place(&ids, mid);
                if slack(&at) <= extra {
                    (lo, fits) = (mid, at);
                } else {
                    (hi, wide) = (mid, at);
                }
            }
            (wide, fits)
        }
    };
    let (a, b) = (slack(&wide), slack(&fits));
    let share = if a - b > f32::EPSILON {
        ((a - extra) / (a - b)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let knots: Vec<(f32, f32)> = members
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let at = wide[k] + share * (fits[k] - wide[k]);
            (m.x, at - natural[k])
        })
        .collect();
    Some(Placement {
        dx: base_dx,
        dy,
        knots: Rc::from(knots),
    })
}

/// The ink a curve passes once its system is placed: the flags, accidentals,
/// stems and heads of one system's band where the page sets them.
#[derive(Default)]
struct PlacedInk {
    flags: Vec<InkRect>,
    accidentals: Vec<InkRect>,
    stems: Vec<InkRect>,
    heads: Vec<InkRect>,
}

/// A compressed system's slur or tie, placed from where the spacing set it
/// before it passed any ink, passed over the ink of its band where the
/// compressed system sets it ([`crate::clear_curve`]): a tie its flag,
/// accidentals and stems, a slur its accidentals and stems and the heads too,
/// as it passes accidentals. The spacing sets each slur clear of the heads it
/// spans; compression narrows a slur and keeps its heights, so a steep one
/// can come onto a head, its own first one above all.
fn pass_compressed(placed: [Point; 4], ink: &PlacedInk, tie: bool) -> [Point; 4] {
    if tie {
        crate::clear_curve(placed, &ink.flags, &ink.accidentals, &ink.stems, true)
    } else {
        let passed: Vec<InkRect> = ink.accidentals.iter().chain(&ink.heads).copied().collect();
        crate::clear_curve(placed, &ink.flags, &passed, &ink.stems, false)
    }
}

/// Each system's [`PlacedInk`], by band, from the baked glyphs and strokes
/// (each parallel to the input's for as many as the input holds).
fn placed_ink(
    input: &ConstrainedLayoutIR,
    glyphs: &[ResolvedGlyph],
    glyph_system: &[Option<usize>],
    strokes: &[Stroke],
    stroke_system: &[Option<usize>],
    anchors: &BTreeMap<GlyphObjectId, (SpringSlotId, SpringSlotId)>,
) -> BTreeMap<(usize, VerticalBandId), PlacedInk> {
    let mut ink: BTreeMap<(usize, VerticalBandId), PlacedInk> = BTreeMap::new();
    for ((glyph, placed), system) in input.glyphs.iter().zip(glyphs).zip(glyph_system) {
        let name = glyph.glyph.as_str();
        let flag = name.starts_with("flag");
        let head = name.starts_with("notehead");
        let Some(s) = system.filter(|_| flag || head || name.starts_with("accidental")) else {
            continue;
        };
        let (x, y, b) = (
            placed.position.x.0,
            placed.position.y.0,
            placed.bounding_box,
        );
        let rect = InkRect {
            left: x + b.left.0,
            right: x + b.right.0,
            bottom: y + b.bottom.0,
            top: y + b.top.0,
        };
        let entry = ink.entry((s, glyph.vertical_band)).or_default();
        if flag {
            entry.flags.push(rect);
        } else if head {
            entry.heads.push(rect);
        } else {
            entry.accidentals.push(rect);
        }
    }
    for ((stroke, placed), system) in input.strokes.iter().zip(strokes).zip(stroke_system) {
        let (Some(s), Some(_)) = (system, crate::stem_slot(stroke, anchors)) else {
            continue;
        };
        let (x, half) = (placed.from.x.0, placed.thickness.0 * 0.5);
        ink.entry((*s, stroke.vertical_band))
            .or_default()
            .stems
            .push(InkRect {
                left: x - half,
                right: x + half,
                bottom: placed.from.y.0.min(placed.to.y.0),
                top: placed.from.y.0.max(placed.to.y.0),
            });
    }
    ink
}

/// Ends a staff line at `end`, the right edge of the barline closing its
/// system, so the staff runs under the whole barline and no further. Any
/// other stroke, or a system no barline closes, is left as placed.
fn end_staff_line(stroke: &mut Stroke, end: Option<f32>) {
    let (TypedObjectId::Staff(_), Some(end)) = (stroke.provenance.source, end) else {
        return;
    };
    if stroke.to.x.0 >= stroke.from.x.0 {
        stroke.to = Point::new(end, stroke.to.y.0);
    } else {
        stroke.from = Point::new(end, stroke.from.y.0);
    }
}

/// Places a whole stroke under a system's justification. An anchored stroke
/// rides its slots: a beam's ends move with the stems it joins, a stem with
/// its heads. A per-event component stroke (a ledger) tracks its notehead:
/// both endpoints translate by the owning slot's delta, so it stays attached
/// without stretching its offset. A spanning stroke (a staff line, a volta
/// bracket) stretches with the system: each endpoint maps through the affine.
fn place_stroke(
    source: &Stroke,
    spaced: &Stroke,
    p: &Placement,
    slot_source_x: &BTreeMap<SpringSlotId, f32>,
    glyphs: &[GlyphObject],
    anchor: Option<&(SpringSlotId, SpringSlotId)>,
) -> Stroke {
    // An anchored stroke (a beam, a stem): each end rides its own slot's delta.
    let slot_dx = |slot: &SpringSlotId| slot_source_x.get(slot).map(|&sx| p.slot_dx(sx));
    if let Some((from_dx, to_dx)) =
        anchor.and_then(|(start, end)| Some((slot_dx(start)?, slot_dx(end)?)))
    {
        return Stroke {
            provenance: spaced.provenance.clone(),
            from: Point::new(spaced.from.x.0 + from_dx, spaced.from.y.0 + p.dy),
            to: Point::new(spaced.to.x.0 + to_dx, spaced.to.y.0 + p.dy),
            thickness: spaced.thickness,
            layer: spaced.layer,
            style: spaced.style,
            vertical_band: spaced.vertical_band,
        };
    }
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
/// a measure record per barline the system carries, each for the measure the
/// barline ends. What the pipeline does not know is left empty, never
/// fabricated: a staff with no engraved lines yields no staff record. The
/// system's first measure reaches back by `opening`, the shift its opening
/// columns took to keep their place after the lead.
#[allow(clippy::too_many_arguments)]
fn build_system(
    system: usize,
    plan: &SystemPlan,
    input: &ConstrainedLayoutIR,
    region_slots: &[Vec<SlotInfo>],
    extents: &[Extent],
    placements: &[Placement],
    staff_marks: &BTreeMap<(usize, StaffId), StaffAgg>,
    primitives: PrimitiveIndices,
    lead: f32,
    opening: f32,
) -> ResolvedSystem {
    let region = &input.regions[plan.region];
    let p = &placements[system];
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
        // width, and a later system's lead before it.
        origin: Point::new(p.x(ext.min_x) - lead, ext.min_y + p.dy),
        size: Size2D {
            width: StaffSpace(p.x(ext.max_x) - p.x(ext.min_x) + lead),
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

    // Measures: each barline column closes the measure its barline ends, which
    // spans from the previous barline in this system (or the system's start)
    // to it.
    // A slot's ink edges move with its glyphs, by the slot's own delta.
    let slots = &region_slots[plan.region];
    let lo = |i: usize| slots[i].lo + p.slot_dx(slots[i].x);
    let hi = |i: usize| slots[i].hi + p.slot_dx(slots[i].x);
    let mut span_start = plan
        .slots
        .iter()
        .map(|&i| lo(i))
        .fold(f32::INFINITY, f32::min);
    let mut measures: Vec<ResolvedMeasure> = Vec::new();
    let mut back = opening;
    for &i in &plan.slots {
        let Some(g) = slots[i].measure_barline else {
            continue;
        };
        let glyph = &input.glyphs[g];
        let TypedObjectId::Measure(measure) = glyph.provenance.source else {
            continue;
        };
        let (start, end) = (span_start, hi(i));
        span_start = end;
        measures.push(ResolvedMeasure {
            provenance: glyph.provenance.clone(),
            measure,
            bounding_box: Rect {
                origin: Point::new(start - back, ext.min_y + p.dy),
                size: Size2D {
                    width: StaffSpace(end - start + back),
                    height: StaffSpace(ext.max_y - ext.min_y),
                },
            },
        });
        back = 0.0;
    }

    ResolvedSystem {
        provenance,
        bounding_box,
        staves,
        measures,
        primitives,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(from: (f32, f32), to: (f32, f32)) -> Stroke {
        Stroke {
            provenance: Provenance::projected(
                TypedObjectId::Staff(StaffId::from_raw(1)),
                Vec::new(),
            ),
            from: Point::new(from.0, from.1),
            to: Point::new(to.0, to.1),
            thickness: StaffSpace(0.11),
            layer: 0,
            style: GlyphStyle { rgba: 0x0000_00ff },
            vertical_band: VerticalBandId(1),
        }
    }

    /// A dashed piece is laid as MuseScore distributes a dash pattern: one
    /// more dash than the whole pairs of dash and gap its length holds, its
    /// first dash at its start and its last at its end, the gaps even; one
    /// shorter than two dashes is drawn solid (X4d.1).
    #[test]
    fn a_dashed_piece_spreads_its_dashes_to_its_ends() {
        let dashes = LineDashes {
            dash: 0.66,
            gap: 0.66,
        };
        let mut piece = line((2.0, 1.0), (12.0, 1.0));
        let more = dash_line(&mut piece, dashes);
        // 10 holds 7 whole pairs of 1.32: 8 dashes, gaps of (10 - 8 * 0.66) / 7.
        assert_eq!(more.len(), 7);
        assert_eq!((piece.from.x.0, piece.to.x.0), (2.0, 2.66));
        let last = more.last().expect("dashes");
        assert!((last.to.x.0 - 12.0).abs() < 1e-4, "{last:?}");
        let gap = (10.0 - 8.0 * 0.66) / 7.0;
        assert!(
            (more[0].from.x.0 - (2.66 + gap)).abs() < 1e-4,
            "{:?}",
            more[0]
        );
        let mut short = line((2.0, 1.0), (3.0, 1.0));
        assert!(dash_line(&mut short, dashes).is_empty());
        assert_eq!((short.from.x.0, short.to.x.0), (2.0, 3.0), "solid");
    }

    /// A glissando's words stand along a piece only where they are shorter
    /// than it, centred, turned to its slope (X4d.1).
    #[test]
    fn a_glissandos_words_stand_only_where_they_fit() {
        let (words, _) = epiphany_layout_ir::shape_text(
            Provenance::projected(TypedObjectId::Staff(StaffId::from_raw(1)), Vec::new()),
            "gliss.",
            epiphany_layout_ir::TextFace::Italic,
            1.613,
            epiphany_layout_ir::TextAlign::Left,
            Point::new(0.0, 0.0),
        );
        let width = words.bounds.right.0 - words.bounds.left.0;
        let mut long = line((0.0, 0.0), (width + 2.0, 1.0));
        let runs = glissando_runs(&mut long, false, Some((&words, 0.1)), 0);
        assert_eq!(runs.len(), 1);
        assert!(
            runs[0].segments[0].glyphs[0].transform.is_some(),
            "turned to its slope"
        );
        let mut short = line((0.0, 0.0), (width * 0.9, 0.0));
        assert!(glissando_runs(&mut short, false, Some((&words, 0.1)), 0).is_empty());
    }

    /// A glissando's heights over pieces keep one gradient from its first
    /// head's centre to its last's, its ends their distance across from the
    /// heads (X4d.1).
    #[test]
    fn a_glissandos_heights_keep_one_gradient_across_its_pieces() {
        let heads = [Point::new(0.0, 0.0), Point::new(10.0, 4.0)];
        let stroke = line((1.0, 0.4), (9.0, 3.6));
        // Two pieces, the first from 1 to 4, the second from 20 to 24 (5 to
        // 9 spaced along the line): 3 + 4 across, 1 and 1 to the heads.
        let ys = glissando_heights(heads, &stroke, &[(1.0, 4.0), (20.0, 24.0)]);
        let rate = 4.0 / 9.0;
        let want = [(rate, 4.0 * rate), (4.0 * rate, 8.0 * rate)];
        for (got, want) in ys.iter().zip(want) {
            assert!(
                (got.0 - want.0).abs() < 1e-4 && (got.1 - want.1).abs() < 1e-4,
                "{ys:?}"
            );
        }
    }

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

    /// A compressed system's slur passes the heads of its band as it passes
    /// accidentals, and a tie keeps to its own obstacles: a slur falling
    /// steeply from above a head across its top right corner is lifted clear
    /// of it, and the same curve as a tie is left as it was.
    #[test]
    fn a_compressed_systems_slur_passes_the_heads_it_meets() {
        let cp =
            [(0.3, 0.6), (0.7, 0.67), (1.6, -0.33), (2.7, -2.4)].map(|(x, y)| Point::new(x, y));
        let head = InkRect {
            left: 0.0,
            right: 1.18,
            bottom: -0.5,
            top: 0.5,
        };
        let meets = |cp: [Point; 4]| {
            (1..64).any(|k| {
                let t = k as f32 / 64.0;
                let at = |a: f32, b: f32, c: f32, d: f32| {
                    let u = 1.0 - t;
                    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
                };
                let x = at(cp[0].x.0, cp[1].x.0, cp[2].x.0, cp[3].x.0);
                let y = at(cp[0].y.0, cp[1].y.0, cp[2].y.0, cp[3].y.0);
                x > head.left && x < head.right && y > head.bottom && y < head.top
            })
        };
        assert!(meets(cp), "the slur as placed runs through the head");
        let ink = PlacedInk {
            heads: vec![head],
            ..PlacedInk::default()
        };
        assert!(
            !meets(pass_compressed(cp, &ink, false)),
            "the slur passes the head"
        );
        assert_eq!(
            pass_compressed(cp, &ink, true),
            cp,
            "a tie keeps its own obstacles"
        );
    }

    /// A page's frame is where its content was stacked: its top edge one top
    /// margin above the content's top, its size the page's.
    #[test]
    fn a_page_frame_holds_the_page_s_content_top() {
        let geometry = PageGeometry::default();
        for p in 0..4 {
            let frame = geometry.page_frame(p);
            let top = frame.origin.y.0 + frame.size.height.0;
            assert!((top - geometry.margins.top.0 - page_top_content(p, &geometry)).abs() < 1e-3);
            assert_eq!(frame.size, geometry.size);
            assert_eq!(frame.origin.x.0, 0.0);
        }
    }

    /// A uniform test measure: one break-candidate barline slot per measure,
    /// spanning `[i·10, i·10 + 9]` (each measure ~9 wide, step 10).
    /// Bounds with no lead, and no staff lines left of the first column.
    fn bounds(width_limit: f32, room: &[f32]) -> Bounds<'_> {
        static NONE_HELD: BTreeSet<(usize, usize)> = BTreeSet::new();
        Bounds {
            width_limit,
            left: f32::INFINITY,
            lead: 0.0,
            room,
            held: &NONE_HELD,
        }
    }

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

    /// A justified system none of whose slots has a spring spreads its slack
    /// over its intervals by their widths: here slots at 0, 10, 20 and 30,
    /// each interval taking a third.
    #[test]
    fn a_system_without_springs_takes_its_slack_by_width() {
        let slots: Vec<SlotInfo> = (0..4).map(measure_slot).collect();
        let plan = SystemPlan {
            region: 0,
            local: 0,
            slots: vec![0, 1, 2, 3],
            boundary: None,
            page_forced: false,
            page_source: DecisionSource::Automatic,
        };
        let ext = Extent {
            min_x: 0.0,
            min_y: -4.0,
            max_x: 39.0,
            max_y: 0.0,
            any: true,
        };
        let placement = justify_system(
            &plan,
            &ext,
            0.0,
            0.0,
            &[slots],
            &[vec![0, 1]],
            &BTreeMap::new(),
            48.0,
        );
        let taken: Vec<f32> = placement.knots.iter().map(|knot| knot.1).collect();
        assert_eq!(taken.len(), 4, "a knot at each slot");
        for (k, slack) in taken.iter().enumerate() {
            assert!(
                (slack - 3.0 * k as f32).abs() < 1e-4,
                "the slack before slot {k} is {slack}: {taken:?}"
            );
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
        let breaks = optimal_breaks(&slots, &BTreeMap::new(), &bounds(42.0, &[0.0; 6]));
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
    fn optimal_breaks_reserves_the_room_a_continued_tie_needs() {
        // Six uniform measures balance to [3, 3]. A tie continued into the
        // fourth needs 14 more of its system's lead there, which no system of
        // three or more measures opening on it has: the search breaks before
        // the third instead, [2, 4], as wide as [4, 2] and with the larger
        // final system.
        let slots: Vec<SlotInfo> = (0..6).map(measure_slot).collect();
        let mut room = [0.0; 6];
        room[3] = 14.0;
        let breaks = optimal_breaks(&slots, &BTreeMap::new(), &bounds(42.0, &room));
        assert_eq!(breaks, BTreeSet::from([slots[2].id]), "{breaks:?}");
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
        let breaks = optimal_breaks(&slots, &reqs, &bounds(42.0, &[0.0; 6]));
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
        let a = optimal_breaks(&slots, &BTreeMap::new(), &bounds(42.0, &[0.0; 6]));
        let b = optimal_breaks(&slots, &BTreeMap::new(), &bounds(42.0, &[0.0; 6]));
        assert_eq!(a, b, "a pure function of the inputs");
        assert!(
            optimal_breaks(&slots, &BTreeMap::new(), &bounds(f32::INFINITY, &[0.0; 6])).is_empty(),
            "an unbounded width wraps nothing"
        );
    }

    #[test]
    fn the_overflow_net_reserves_the_room_a_continued_tie_needs() {
        // A note-less M0 whose soft break the walk skips, so the search's plan
        // ([M1..M4] in one system) never opens, and the overflow net breaks
        // before M3. M3's system then reserves the 25 a tie continued into it
        // needs, so M4 no longer fits beside it and opens a third system.
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
        let slots = vec![
            mk(0, 0.0, 20.0, false),
            mk(1, 21.0, 30.0, true),
            mk(2, 31.0, 40.0, true),
            mk(3, 41.0, 50.0, true),
            mk(4, 51.0, 60.0, true),
        ];
        let mut reqs: BTreeMap<SpringSlotId, Vec<BreakReq>> = BTreeMap::new();
        reqs.insert(
            slots[1].id,
            vec![BreakReq {
                page: false,
                hard: false,
            }],
        );
        let mut room = [0.0; 5];
        room[3] = 25.0;
        let mut systems = Vec::new();
        walk_region(
            0,
            &slots,
            &reqs,
            &BTreeMap::new(),
            TypedObjectId::Region(RegionId::new(ReplicaId(1), 1)),
            bounds(42.0, &room),
            &mut systems,
            &mut Vec::new(),
        );
        let opened: Vec<Vec<usize>> = systems.into_iter().map(|plan| plan.slots).collect();
        assert_eq!(opened, [vec![0, 1, 2], vec![3], vec![4]]);
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
            bounds(width_limit, &[0.0; 6]),
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
        // ten records, one per measure, each from the barline that ends it.
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
        assert_eq!(measure_count(&plain), 10);
        assert_eq!(measure_count(&repeats), 10);
        // The volta brackets sit above the staff, so the systems carrying them
        // stand taller than the repeat-free fixture's (whose first holds its
        // rehearsal mark, as the repeat fixture's does).
        let height = |report: &crate::SolveReport| -> f32 {
            report
                .layout
                .pages
                .iter()
                .flat_map(|page| &page.systems)
                .map(|system| system.bounding_box.size.height.0)
                .sum()
        };
        assert!(height(&repeats) > height(&plain));
    }

    /// An ending whose first measure opens a system is drawn wholly on that
    /// system, as MuseScore lays a spanner out only on a system its start
    /// precedes the end of (`SystemLayout`): the repeat fixture's first
    /// ending, whose measure opens the third system, has its hook, its number
    /// and its line there, the hook `BAR_NOTE_DISTANCE` before the system's
    /// first note, nothing at the end of the system before, and the system's
    /// notes where they stand without it; its second ending, opening
    /// mid-system, stands at its barline. Begun a measure earlier, in the
    /// second system, the first ending stays there and runs on into the third.
    #[test]
    fn an_ending_opening_a_system_is_drawn_wholly_on_it() {
        use crate::Engraver;
        use epiphany_core::{AnchorOffset, MeasurePosition, RepeatKind, TimeAnchor};
        use epiphany_layout_ir::{
            to_constrained, to_logical, ConstraintSolver, ResolvedLayoutIR, SolverConfig,
            BAR_NOTE_DISTANCE,
        };
        let solve = |score: &epiphany_core::Score| {
            let input = to_constrained(&to_logical(score));
            let layout = Engraver::default()
                .solve(&input, &SolverConfig::default())
                .layout;
            (input, layout)
        };
        // Where each primitive of `input` is drawn: its system, by id.
        fn system_of(
            layout: &ResolvedLayoutIR,
            glyph: Option<GlyphObjectId>,
            stroke: Option<GlyphObjectId>,
        ) -> usize {
            let systems: Vec<&ResolvedSystem> =
                layout.pages.iter().flat_map(|page| &page.systems).collect();
            if let Some(id) = stroke {
                let i = layout
                    .strokes
                    .iter()
                    .position(|s| s.id() == id)
                    .expect("drawn");
                return systems
                    .iter()
                    .position(|sys| sys.primitives.strokes.contains(&(i as u32)))
                    .expect("owned");
            }
            let id = glyph.expect("a glyph or a stroke");
            let i = layout
                .glyphs
                .iter()
                .position(|g| GlyphObjectId(g.provenance.stable_id.0) == id)
                .expect("drawn");
            systems
                .iter()
                .position(|sys| sys.primitives.glyphs.contains(&(i as u32)))
                .expect("owned")
        }
        let barline_system = |input: &ConstrainedLayoutIR, layout: &ResolvedLayoutIR, slot| {
            let glyph = input
                .glyphs
                .iter()
                .find(|g| g.horizontal_slot == slot && is_barline_glyph(g.glyph.as_str()))
                .expect("a barline");
            system_of(layout, Some(glyph.id()), None)
        };
        let first_note = |layout: &ResolvedLayoutIR, s: usize| {
            let system = layout
                .pages
                .iter()
                .flat_map(|page| &page.systems)
                .nth(s)
                .expect("a system");
            system
                .primitives
                .glyphs
                .iter()
                .map(|&i| &layout.glyphs[i as usize])
                .filter(|g| g.glyph.as_str().starts_with("notehead"))
                .map(|g| g.position.x.0 + g.bounding_box.left.0)
                .fold(f32::INFINITY, f32::min)
        };

        let mut score = epiphany_testkit::fixtures::ten_measure_with_repeats(0x000A_11CE);
        let (input, layout) = solve(&score);
        assert_eq!(input.endings.len(), 2, "two endings");
        let first = &input.endings[0];
        let hook = &layout.strokes[layout
            .strokes
            .iter()
            .position(|s| s.id() == first.hook)
            .expect("drawn")];
        let line = &layout.strokes[layout
            .strokes
            .iter()
            .position(|s| s.id() == first.line)
            .expect("drawn")];
        assert_eq!(
            system_of(&layout, None, Some(first.hook)),
            2,
            "the first ending on the third system"
        );
        assert_eq!(
            system_of(&layout, None, Some(first.line)),
            2,
            "its line too"
        );
        assert_eq!(
            barline_system(&input, &layout, first.barline),
            1,
            "its barline ends the second"
        );
        for number in &first.numbers {
            assert_eq!(system_of(&layout, Some(*number), None), 2, "and its number");
        }
        let half = hook.thickness.0 * 0.5;
        assert!(
            (hook.from.x.0 - half - (first_note(&layout, 2) - BAR_NOTE_DISTANCE)).abs() < 1e-3,
            "the hook {} before the first note",
            hook.from.x.0
        );
        assert!(
            (line.from.x.0 - hook.from.x.0).abs() < 1e-3,
            "the line from the hook"
        );
        // The third system's notes stand where they stand without it, but for
        // the half width of the line's piece there, which joins the system's
        // extent as before.
        let mut without = score.clone();
        without.cross_cutting.repeats[1].voltas.remove(0);
        let (_, alone) = solve(&without);
        assert!((first_note(&layout, 2) - first_note(&alone, 2)).abs() < 0.05);
        // Nothing of it at the end of the second system.
        let second_system = layout
            .pages
            .iter()
            .flat_map(|page| &page.systems)
            .nth(1)
            .expect("a system");
        let at_end = second_system
            .primitives
            .strokes
            .iter()
            .map(|&i| &layout.strokes[i as usize])
            .filter(|s| s.provenance.source == line.provenance.source && s.from.y == line.from.y)
            .count();
        assert_eq!(
            at_end, 0,
            "no piece of an ending at the second system's end"
        );
        // The second ending opens mid-system, at its barline.
        let second = &input.endings[1];
        assert_eq!(
            system_of(&layout, None, Some(second.hook)),
            barline_system(&input, &layout, second.barline)
        );

        // Begun at the sixth measure, mid-system, it runs on from there.
        let sixth = score.canvas.regions[0].staff_instances()[0].measures[5].id;
        let RepeatKind::Volta = score.cross_cutting.repeats[1].kind else {
            panic!("the volta repeat");
        };
        score.cross_cutting.repeats[1].voltas[0].start = TimeAnchor::Measure {
            id: sixth,
            position: MeasurePosition::Start,
            offset: AnchorOffset::Zero,
        };
        let (input, layout) = solve(&score);
        let first = &input.endings[0];
        assert_eq!(barline_system(&input, &layout, first.barline), 1);
        assert_eq!(
            system_of(&layout, None, Some(first.hook)),
            1,
            "it starts on the second"
        );
        assert_eq!(
            system_of(&layout, None, Some(first.line)),
            1,
            "its line's first piece too"
        );
    }

    #[test]
    fn the_widow_rebalance_evens_the_final_system() {
        use crate::Engraver;
        use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};
        // The ten-measure fixture wraps under the default A4 geometry. Greedy
        // first-fit alone leaves a stub final system; the widow rebalance
        // evens the split so the final system carries a substantial share of
        // the measures — while the system *count* is unchanged. (Justification
        // stretches the systems to the full content width, so the rebalance's
        // effect shows in the MEASURE distribution, not the baked widths.)
        let input = to_constrained(&to_logical(
            &epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
        ));
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        let page = &report.layout.pages[0];
        assert!(page.systems.len() >= 2, "the fixture wraps");
        let first = page
            .systems
            .iter()
            .map(|system| system.measures.len())
            .max()
            .expect("a system");
        let last = page.systems[page.systems.len() - 1].measures.len();
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
        // different systems (the fixture wraps), so the curve spans the breaks.
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
        assert!(
            report.layout.pages[0].systems.len() >= 2,
            "the fixture wraps"
        );

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

    /// (m1) For each of the layout's three flat arrays, every system's owned
    /// index list plus the layout's `unowned` bucket covers `0..len` exactly
    /// once — pin 4's total, disjoint partition. The load-bearing invariant,
    /// checked directly rather than assumed from construction.
    fn assert_total_disjoint_partition(layout: &epiphany_layout_ir::ResolvedLayoutIR) {
        let check = |label: &str, len: usize, owned: Vec<&Vec<u32>>, unowned: &[u32]| {
            let mut seen = vec![0u8; len];
            for &i in owned.iter().flat_map(|v| v.iter()).chain(unowned.iter()) {
                let idx = i as usize;
                assert!(idx < len, "{label}: index {i} out of range (len {len})");
                seen[idx] += 1;
            }
            for (i, &count) in seen.iter().enumerate() {
                assert_eq!(
                    count, 1,
                    "{label}: index {i} covered {count} times (want exactly 1)"
                );
            }
        };
        let glyph_lists: Vec<&Vec<u32>> = layout.systems().map(|s| &s.primitives.glyphs).collect();
        check(
            "glyphs",
            layout.glyphs.len(),
            glyph_lists,
            &layout.unowned.glyphs,
        );
        let stroke_lists: Vec<&Vec<u32>> =
            layout.systems().map(|s| &s.primitives.strokes).collect();
        check(
            "strokes",
            layout.strokes.len(),
            stroke_lists,
            &layout.unowned.strokes,
        );
        let curve_lists: Vec<&Vec<u32>> = layout.systems().map(|s| &s.primitives.curves).collect();
        check(
            "curves",
            layout.curves.len(),
            curve_lists,
            &layout.unowned.curves,
        );
    }

    #[test]
    fn primitive_ownership_partitions_every_flat_array_totally_and_disjointly() {
        use crate::Engraver;
        use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};

        // The wrapping ten-measure fixture: real glyphs and strokes, no curves.
        let wrapping = Engraver::default().solve(
            &to_constrained(&to_logical(
                &epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
            )),
            &SolverConfig::default(),
        );
        assert!(
            wrapping.layout.pages[0].systems.len() >= 2,
            "the fixture wraps"
        );
        assert!(!wrapping.layout.glyphs.is_empty());
        assert!(!wrapping.layout.strokes.is_empty());
        assert_total_disjoint_partition(&wrapping.layout);

        // The slurred fixture: also exercises curves, including a
        // system-spanning split (G4's own construction).
        let slurred = Engraver::default().solve(
            &to_constrained(&to_logical(
                &epiphany_testkit::fixtures::ten_measure_with_slurs(0),
            )),
            &SolverConfig::default(),
        );
        let slurred_systems: usize = slurred.layout.pages.iter().map(|p| p.systems.len()).sum();
        assert!(
            slurred_systems > 1,
            "casting-off wraps the slurred fixture too"
        );
        assert!(
            !slurred.layout.curves.is_empty(),
            "the slur produces real curves"
        );
        assert_total_disjoint_partition(&slurred.layout);
    }

    #[test]
    fn attribution_correctness_matches_the_real_per_system_counts() {
        // (m3) The *actual* per-system glyph/stroke counts of the wrapping
        // fixture — real numbers, not `> 0` — value-asserted directly against
        // what casting-off computed.
        use crate::Engraver;
        use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};

        let report = Engraver::default().solve(
            &to_constrained(&to_logical(
                &epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
            )),
            &SolverConfig::default(),
        );
        let systems: Vec<_> = report.layout.systems().collect();
        assert_eq!(systems.len(), 3, "three systems");
        let glyph_counts: Vec<usize> = systems.iter().map(|s| s.primitives.glyphs.len()).collect();
        let stroke_counts: Vec<usize> =
            systems.iter().map(|s| s.primitives.strokes.len()).collect();
        assert_eq!(
            glyph_counts,
            vec![16, 21, 16],
            "the three/four/three measure split's real per-system glyph counts \
             (each later system's lead adds its clef)"
        );
        assert_eq!(
            stroke_counts,
            vec![37, 37, 29],
            "the three/four/three measure split's real per-system stroke counts \
             (the tie in the first system is a curve; the rehearsal mark's frame, \
             four strokes, takes its traced anchor's place)"
        );
        assert_eq!(
            glyph_counts.iter().sum::<usize>(),
            report.layout.glyphs.len()
        );
        assert_eq!(
            stroke_counts.iter().sum::<usize>(),
            report.layout.strokes.len()
        );
        assert!(
            report.layout.unowned.glyphs.is_empty(),
            "the whole score is inside the one region this fixture declares"
        );
    }

    #[test]
    fn continuation_segments_are_owned_by_the_system_they_split_into() {
        // (m5) A slur crossing a system break: its synthesized continuation
        // segment is owned by the system it was split INTO, not the source
        // segment's system.
        use crate::Engraver;
        use epiphany_core::{Slur, SlurId, SlurKind, SpanStyle, TypedObjectId};
        use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};

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
        assert!(
            report.layout.pages[0].systems.len() >= 2,
            "the fixture wraps"
        );

        let original_index = report
            .layout
            .curves
            .iter()
            .position(|c| {
                c.provenance.source == TypedObjectId::Slur(slur_id)
                    && c.provenance.synthesis.is_none()
            })
            .expect("one segment keeps the slur's exact provenance");
        let continuation_index = report
            .layout
            .curves
            .iter()
            .position(|c| {
                c.provenance.source == TypedObjectId::Slur(slur_id)
                    && c.provenance.synthesis.is_some()
            })
            .expect("the break-spanning slur splits and synthesizes a continuation");

        let owner_of = |index: usize| -> Option<usize> {
            report
                .layout
                .systems()
                .position(|s| s.primitives.curves.contains(&(index as u32)))
        };
        let owner_first = owner_of(original_index).expect("the original segment is owned");
        let owner_continuation = owner_of(continuation_index).expect("the continuation is owned");
        assert_eq!(
            owner_first, 0,
            "the original segment starts in the first system"
        );
        assert_eq!(
            owner_continuation, 1,
            "the continuation is owned by the system it was split INTO, not the source's"
        );
    }

    /// A melisma line's piece stands where it is a space long less MuseScore's
    /// tolerance, and not shorter; a hyphen's piece too short for its least
    /// dash grows to it about its centre, or, before a syllable opening its
    /// system, back from its end (`LyricsLayout::layoutMelismaLine`,
    /// `LyricsLayout::layoutDashes`).
    #[test]
    fn a_lyric_lines_piece_keeps_its_least_lengths() {
        use epiphany_core::{RegionId, ReplicaId};
        let line = |x1: f32| Stroke {
            provenance: Provenance::projected(
                TypedObjectId::Region(RegionId::new(ReplicaId(1), 1)),
                Vec::new(),
            ),
            from: Point::new(0.0, 0.0),
            to: Point::new(x1, 0.0),
            thickness: StaffSpace(0.1),
            layer: 0,
            style: GlyphStyle::default(),
            vertical_band: VerticalBandId(0),
        };
        let ends = |s: &Stroke| (s.from.x.0, s.to.x.0);
        let melisma = |x1: f32, before: bool| {
            let mut s = line(x1);
            lyric_line(&mut s, Some(&FamilyLine::Melisma), before);
            ends(&s)
        };
        assert_eq!(melisma(0.97, false), (0.0, 0.97));
        assert_eq!(melisma(0.94, false), (0.0, 0.0));
        let hyphen = |x1: f32, before: bool| {
            let mut s = line(x1);
            let more = lyric_line(&mut s, Some(&FamilyLine::Hyphen), before);
            assert!(more.is_empty());
            ends(&s)
        };
        let near = |(a, b): (f32, f32), (c, d): (f32, f32)| {
            assert!((a - c).abs() < 1e-5 && (b - d).abs() < 1e-5, "{a} {b}");
        };
        near(hyphen(0.2, false), (-0.1, 0.3));
        near(hyphen(0.2, true), (-0.2, 0.2));
        near(hyphen(0.5, true), (0.0, 0.5));
    }

    /// A trill line's piece holds as many wiggles as reach to the nearest
    /// along it, none where it is shorter than half of one or its ends
    /// cross, and then draws nothing itself (`LineSegment::symbolLine`).
    #[test]
    fn a_trill_lines_piece_holds_its_wiggles_to_the_nearest() {
        use epiphany_core::{RegionId, ReplicaId};
        let advance = 971.0 / 1024.0;
        let line = |x1: f32| Stroke {
            provenance: Provenance::projected(
                TypedObjectId::Region(RegionId::new(ReplicaId(1), 1)),
                Vec::new(),
            ),
            from: Point::new(1.0, 2.0),
            to: Point::new(x1, 2.0),
            thickness: StaffSpace(0.1),
            layer: 0,
            style: GlyphStyle::default(),
            vertical_band: VerticalBandId(0),
        };
        for (x1, n) in [
            (1.0 + 0.49 * advance, 0),
            (1.0 + 0.51 * advance, 1),
            (1.0 + 3.4 * advance, 3),
            (1.0 + 3.6 * advance, 4),
            (0.5, 0),
        ] {
            let mut stroke = line(x1);
            let wiggles = trill_wiggles(&mut stroke);
            assert_eq!(wiggles.len(), n, "{x1}");
            for (i, x) in wiggles.iter().enumerate() {
                assert!((x - (1.0 + i as f32 * advance)).abs() < 1e-5);
            }
            assert_eq!(stroke.to, stroke.from, "a traced anchor");
        }
    }
}
