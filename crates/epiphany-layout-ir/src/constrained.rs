//! Stage 2 — `ConstrainedLayoutIR` (Chapter 7 §"ConstrainedLayoutIR").
//!
//! The output of the spacing pass: the logical IR with composite objects
//! flattened to individual glyphs, each glyph carrying the anchor geometry that
//! is the constraint solver's input. v0 lays glyphs out left-to-right on the
//! canonical `1/1024` grid, assigns each region's glyphs to a vertical band
//! (Chapter 7 §"Vertical Bands"), carries the engraving decisions forward, and
//! stamps the catalog with a metrics hash over exactly the glyphs it references
//! (Chapter 7 §7.3.2), and emits the spring-slot and constraint interfaces the
//! solver consumes. The geometry here is what the stub solver returns verbatim.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    Clef, EventId, KeySignature, LineStyle, MeasureId, MeasurePosition, MusicalDuration, NoteValue,
    PitchId, PitchSpelling, RepeatStructureId, SpellingNominal, StaffId, TimeAnchor, TypedObjectId,
    WallClockTime,
};
use epiphany_determinism::{DomainTag, Preimage};

use crate::engrave_theory::{
    accidental_glyph, alteration_glyph, clef_glyph_for, flag_count, flag_glyph, has_stem,
    key_alteration, key_signature, notehead_glyph, rest_glyph, stack_alteration, staff_position,
    StaffStep,
};
use crate::engraving::{EngravingDecision, OverrideKind, OverridePriority, OverrideTarget};
use crate::glyph::{metrics, BravuraCatalog, GlyphCatalog, GlyphCatalogIdentity, GlyphReference};
use crate::logical::{
    apply_offset, BarlineKind, LayoutContent, LogicalLayoutIR, PlacedClef, PlacedKeySignature,
    RepeatContent, RepeatPlacement, ScoreVersion, SlurContent, SlurDirection, SlurEndpoint,
    StaffContent, VoicePlace,
};
use crate::provenance::{
    manifestation_layout_id, LayoutObjectId, Provenance, SynthesisInstanceKey, SynthesisKind,
    SynthesisRegistryId,
};
use crate::solver::{ConstraintStrength, SpringSlotId};
use crate::spatial::{BoundingBox, Point, Rect, Size2D, StaffSpace};
use crate::time_axis::{time_cmp, SlotPlacement, TimeAxisModel, TimePoint};
use crate::vertical_band::{inter_staff_gap_id, VerticalBand, VerticalBandId};

/// A stable identifier for a glyph-level object (Chapter 7: `GlyphObjectId`).
/// Shares the glyph's provenance `stable_id`, so it is stable across relayouts.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GlyphObjectId(pub u128);

/// A glyph with a baseline anchor, the input to the solver (Chapter 7
/// §"Glyph-Level Objects"). v0 references the glyph by SMuFL name and queries
/// its metrics from the in-tree catalog ([`crate::glyph`]); the `baseline` is
/// the staff-space geometry the stub solver returns verbatim.
#[derive(Clone, PartialEq, Debug)]
pub struct GlyphObject {
    pub provenance: Provenance,
    /// The SMuFL glyph whose metrics the solver consults.
    pub glyph: GlyphReference,
    /// Horizontal spring slot containing this glyph.
    pub horizontal_slot: SpringSlotId,
    pub baseline: Point,
    /// The vertical band this glyph belongs to (Chapter 7 §"Glyph-Level
    /// Objects": every glyph names exactly one `vertical_band`).
    pub vertical_band: VerticalBandId,
    pub bounding_box: BoundingBox,
    pub anchor: Point,
    pub layer: i32,
    pub style: GlyphStyle,
}

impl GlyphObject {
    /// This glyph's stable id (Chapter 7: `GlyphObjectId`).
    pub fn id(&self) -> GlyphObjectId {
        GlyphObjectId(self.provenance.stable_id.0)
    }
}

/// A straight stroke (a line/rule) the renderer draws directly — the notation
/// primitives that are *not* SMuFL glyphs: staff lines, stems, barlines, ledger
/// lines, and beams. Endpoints are in staff-space and `thickness` is the line
/// width in staff spaces; the stroke carries its own [`Provenance`] so it traces
/// like a glyph. It flows through the solver and is positioned by the engraver,
/// not invented by the renderer (Chapter 7 §"Non-overreach").
#[derive(Clone, PartialEq, Debug)]
pub struct Stroke {
    pub provenance: Provenance,
    pub from: Point,
    pub to: Point,
    pub thickness: StaffSpace,
    pub layer: i32,
    pub style: GlyphStyle,
    /// The vertical band this stroke belongs to, declared by the projection that
    /// emitted it — the same band its owning object's glyphs declare. A vertical
    /// solver reads *this*, never the stroke's geometry: a stem, a ledger line,
    /// and a staff line all name their staff outright, so no consumer has to
    /// guess an owner from proximity. Unlike a glyph, a stroke is not listed in
    /// [`VerticalBand::members`] (band membership drives the spring solve over
    /// glyphs); this is a one-way reference, validated only to name a real band.
    ///
    /// Content owned by no staff — a page-margin annotation, a repeat structure
    /// spanning several staves — names the region's margin band.
    pub vertical_band: VerticalBandId,
}

impl Stroke {
    /// This stroke's stable id (derived from its provenance, as a glyph's is).
    pub fn id(&self) -> GlyphObjectId {
        GlyphObjectId(self.provenance.stable_id.0)
    }
}

/// A cubic-bézier curve primitive — the third pipeline primitive kind, drawn as
/// a stroked (unfilled) path (Chapter 7 §"Non-overreach"). Slurs engrave to
/// one of these; ties and other span curves will follow. Like [`Stroke`] it
/// holds no spring slot — the solver re-spaces its control points by the
/// horizontal coordinate map, exactly as it does a spanning stroke's endpoints —
/// but it does declare its [`vertical_band`](Curve::vertical_band). The four
/// control points are world-space staff-space coordinates, `p0`→`p3` the drawing
/// order.
#[derive(Clone, PartialEq, Debug)]
pub struct Curve {
    pub provenance: Provenance,
    pub p0: Point,
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
    pub thickness: StaffSpace,
    pub layer: i32,
    pub style: GlyphStyle,
    /// The line pattern the renderer strokes the path with (a slur's authored
    /// `SpanStyle.line`). Solid, dashed, or dotted.
    pub line: LineStyle,
    /// The vertical band this curve belongs to — see [`Stroke::vertical_band`].
    ///
    /// This matters more for a curve than for any other primitive. A slur's
    /// endpoints are *lifted clear* of its own staff by construction (an above-
    /// slur sits a staff height plus a gap over the top line), so they land in
    /// the inter-staff zone where the nearest notehead can belong to the
    /// ADJACENT staff. No geometric rule recovers the owner. The projection
    /// knows it — a slur's staff is the staff of its notes — so it declares it.
    /// A slur whose notes span two staves is owned by neither and names the
    /// margin band.
    pub vertical_band: VerticalBandId,
}

impl Curve {
    /// This curve's stable id (derived from its provenance, as a glyph's is).
    pub fn id(&self) -> GlyphObjectId {
        GlyphObjectId(self.provenance.stable_id.0)
    }

    /// The four control points in drawing order — the shared iteration order
    /// for remapping, bounding, and flattening.
    pub fn control_points(&self) -> [Point; 4] {
        [self.p0, self.p1, self.p2, self.p3]
    }
}

/// The constrained IR: composite objects flattened to glyphs and strokes, with
/// the vertical bands and engraving decisions that the solver consumes alongside
/// them (Chapter 7 §"Constraints").
#[derive(Clone, PartialEq, Debug)]
pub struct ConstrainedLayoutIR {
    pub source: ScoreVersion,
    pub regions: Vec<ConstrainedLayoutRegion>,
    pub horizontal_slots: Vec<SpringSlot>,
    pub glyphs: Vec<GlyphObject>,
    /// Non-glyph line primitives (staff lines, stems, barlines, …).
    pub strokes: Vec<Stroke>,
    /// Cubic-bézier curve primitives (slurs, …).
    pub curves: Vec<Curve>,
    pub vertical_bands: Vec<VerticalBand>,
    pub constraints: Vec<LayoutConstraint>,
    /// The user-override attributions behind the projected break constraints in
    /// `constraints` (one entry per break constraint that originated in a user
    /// break override), so a casting-off solver can cite the override id in the
    /// decision it records. Engraver-independent constraints (tests, tools) have
    /// no entry here and are attributed `DecisionSource::Automatic`.
    pub break_origins: Vec<BreakOrigin>,
    pub engraving_decisions: Vec<EngravingDecision>,
    /// Engraving-coverage gaps surfaced rather than hidden: a pitch with no
    /// resolved spelling, a glyph the bundled metrics do not carry. Not a hard
    /// error — the object is still placed (a fallback notehead, a traced anchor)
    /// — but the gap is recorded so it is visible, not silently papered over.
    pub diagnostics: Vec<LayoutDiagnostic>,
    pub catalog: GlyphCatalogIdentity,
    /// The spring slots each spanning stroke or curve rides at its two ends
    /// (a beam on its outer stems), so a solver moves each end with its own
    /// column rather than stretching it with the columns between.
    pub span_anchors: Vec<SpanAnchor>,
    /// What each staff shows where a later system of its region starts: the
    /// clef and key signature in force there. The projection draws a region's
    /// first lead itself; a solver that breaks the region into systems draws
    /// these at each later system's start.
    pub system_leads: Vec<SystemLead>,
    /// The staff groups of each region, which a solver marks where each
    /// system's staves stand: a brace, a bracket or a sub-bracket at the left,
    /// and, for all but a choral group, barlines joined from staff to staff.
    pub staff_groups: Vec<GroupSpan>,
}

/// One staff group of a region (see [`ConstrainedLayoutIR::staff_groups`]).
#[derive(Clone, PartialEq, Debug)]
pub struct GroupSpan {
    /// Index into [`ConstrainedLayoutIR::regions`].
    pub region: usize,
    pub kind: GroupSign,
    /// Its staves in the region, top first.
    pub staves: Vec<StaffId>,
    /// Barlines run unbroken from staff to staff within the group.
    pub joined: bool,
    /// The group's provenance: what a solver draws for it is synthesized from
    /// it.
    pub provenance: Provenance,
}

/// What a staff group draws at its system's left.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GroupSign {
    Brace,
    Bracket,
    SubBracket,
}

/// One staff's system-start lead through its region (see
/// [`ConstrainedLayoutIR::system_leads`]).
#[derive(Clone, PartialEq, Debug)]
pub struct SystemLead {
    /// Index into [`ConstrainedLayoutIR::regions`].
    pub region: usize,
    pub staff: StaffId,
    pub band: VerticalBandId,
    /// The staff instance's provenance: each glyph a solver draws from this
    /// lead is synthesized from it.
    pub provenance: Provenance,
    /// In time order, each lead from its time on: its glyphs, at an x offset
    /// from the system's left edge and at their y in the constrained frame.
    pub entries: Vec<(TimePoint, Vec<LeadGlyph>)>,
}

/// One glyph of a system-start lead.
#[derive(Clone, PartialEq, Debug)]
pub struct LeadGlyph {
    pub name: GlyphReference,
    pub x: f32,
    pub y: f32,
}

/// The spring slots a spanning primitive's two ends ride. Each end keeps its
/// offset from its own slot's source through re-spacing and justification, so
/// a beam stays on the stems it joins however the columns between them are
/// spaced. A primitive with no anchor maps through the solver's coordinate map
/// as a whole.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct SpanAnchor {
    /// The stable id of the stroke or curve.
    pub primitive: GlyphObjectId,
    /// The slot the stroke's `from` (a curve's `p0`) rides.
    pub start: SpringSlotId,
    /// The slot the stroke's `to` (a curve's `p3`) rides.
    pub end: SpringSlotId,
}

/// An engraving-coverage gap the constrained pass surfaced (Chapter 7
/// §"Non-overreach": a missing decision is reported, not invented).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LayoutDiagnostic {
    /// The score-graph object the gap concerns.
    pub source: TypedObjectId,
    pub kind: LayoutDiagnosticKind,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LayoutDiagnosticKind {
    /// A pitch reached the constrained pass with no resolved (or non-CMN)
    /// spelling; its notehead is placed on the clef reference line as a
    /// fallback, but its true staff position is unknown.
    MissingSpelling,
    /// A glyph the bundled metrics do not carry (a percussion clef, a
    /// sixteenth-or-shorter rest); the object is carried as a traced anchor
    /// rather than drawn at a guessed shape.
    UnbundledGlyph(GlyphReference),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct GlyphStyle {
    /// RGBA color in `0xRRGGBBAA` form.
    pub rgba: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ConstrainedLayoutRegion {
    pub provenance: Provenance,
    pub glyphs: Vec<GlyphObjectId>,
    /// The region's time axis, populated with the time→slot placements of this
    /// region's spring slots (Chapter 7 §"The Time Axis"): `time_axis.project`
    /// maps a musical/wall-clock time to the slot covering it.
    pub time_axis: TimeAxisModel,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SpringSlot {
    pub id: SpringSlotId,
    pub time: TimePoint,
    pub min_width: StaffSpace,
    pub preferred_width: StaffSpace,
    pub max_width: Option<StaffSpace>,
    pub stretch_factor: f32,
    pub compress_factor: f32,
    pub members: Vec<GlyphObjectId>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BreakKind {
    Hard,
    Soft,
}

/// Which break-constraint family a [`BreakOrigin`] attributes: a
/// [`LayoutConstraint::SystemBreakAt`] or a [`LayoutConstraint::PageBreakAt`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BreakClass {
    System,
    Page,
}

/// The user-override origin of a projected break constraint (Chapter 7
/// §"Engraving Overrides"): the spring slot the override's anchor realized to,
/// which break family it projected into, and the override id. The
/// [`LayoutConstraint`] enum is the spec's normative shape and carries no
/// origin, so the projection records the attribution alongside the constraint
/// list; a casting-off solver that honours the break cites this id in its
/// engraving-decision record (`DecisionSource::UserOverride`, Chapter 7
/// §"Note Layout"). Non-canonical, like every constrained-stage value.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct BreakOrigin {
    pub slot: SpringSlotId,
    pub class: BreakClass,
    pub override_id: crate::engraving::EngravingOverrideId,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConstraintRegistryId(pub u128);

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ConstraintParameters(pub Vec<u8>);

#[derive(Clone, PartialEq, Debug)]
pub enum LayoutConstraint {
    NoCollision {
        a: GlyphObjectId,
        b: GlyphObjectId,
    },
    Align {
        a: GlyphObjectId,
        b: GlyphObjectId,
        axis: Axis,
    },
    PositionWithin {
        glyph: GlyphObjectId,
        region: Rect,
    },
    SystemBreakAt {
        slot: SpringSlotId,
        kind: BreakKind,
    },
    PageBreakAt {
        slot: SpringSlotId,
        kind: BreakKind,
    },
    Registered(ConstraintRegistryId, ConstraintParameters),
}

impl LayoutConstraint {
    /// The strength this constraint binds the solver with (Chapter 9 §"Strength
    /// Levels": [`ConstraintStrength`]).
    ///
    /// The spec's `LayoutConstraint` enum carries no strength field, and the
    /// "normalized form" Chapter 9 says the solver consumes does not specify how
    /// strength attaches to a constraint instance (a genuine spec gap — see
    /// DECISIONS.md), so v0 attaches strength **by rule** rather than widening
    /// the IR shape: a break constraint's own [`BreakKind`] is its strength
    /// (`Hard` → `Required`, `Soft` → `Preferred` at the default weight), the
    /// geometric constraints (no-collision, alignment, containment) are hard
    /// engraving obligations (`Required`), and a `Registered` extension
    /// constraint is conservatively `Required` — an obligation a solver cannot
    /// verify must not be silently demoted (Chapter 9: a solver MUST NOT treat
    /// `Required` as `Preferred`).
    pub fn strength(&self) -> ConstraintStrength {
        match self {
            LayoutConstraint::SystemBreakAt {
                kind: BreakKind::Soft,
                ..
            }
            | LayoutConstraint::PageBreakAt {
                kind: BreakKind::Soft,
                ..
            } => ConstraintStrength::Preferred { weight: 1.0 },
            LayoutConstraint::NoCollision { .. }
            | LayoutConstraint::Align { .. }
            | LayoutConstraint::PositionWithin { .. }
            | LayoutConstraint::SystemBreakAt {
                kind: BreakKind::Hard,
                ..
            }
            | LayoutConstraint::PageBreakAt {
                kind: BreakKind::Hard,
                ..
            }
            | LayoutConstraint::Registered(_, _) => ConstraintStrength::Required,
        }
    }
}

/// A structural defect in [`ConstrainedLayoutIR`] that prevents a solver from
/// treating the input as a valid constraint problem.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConstrainedValidationError {
    DuplicateGlyphId(GlyphObjectId),
    DuplicateBandId(VerticalBandId),
    UnknownBand(VerticalBandId),
    UnknownBandMember(GlyphObjectId),
    DuplicateBandMember(GlyphObjectId),
    BandMismatch(GlyphObjectId),
    InvalidGeometry(GlyphObjectId),
    InvalidBandGeometry(VerticalBandId),
    DuplicateSlotId(SpringSlotId),
    UnknownSlot(SpringSlotId),
    UnknownSlotMember(GlyphObjectId),
    DuplicateSlotMember(GlyphObjectId),
    SlotMismatch(GlyphObjectId),
    InvalidSlotGeometry(SpringSlotId),
    /// A spring slot has no member glyph; the spacing solver derives a slot's
    /// source x from a member, so an empty slot has a target it cannot map.
    EmptySlot(SpringSlotId),
    InvalidGlyphBounds(GlyphObjectId),
    /// A constraint references a glyph that is not in the glyph set.
    UnknownConstraintGlyph(GlyphObjectId),
    /// A break constraint references a spring slot that does not exist.
    UnknownConstraintSlot(SpringSlotId),
    /// A `PositionWithin` constraint carries a non-finite or inverted region.
    InvalidConstraintRegion(GlyphObjectId),
    /// A stroke has a non-finite endpoint or a non-finite/negative thickness.
    InvalidStrokeGeometry(GlyphObjectId),
    /// A curve has a non-finite control point or a non-finite/negative thickness.
    InvalidCurveGeometry(GlyphObjectId),
    /// A span anchor names no stroke or curve, or a slot that does not exist.
    DanglingSpanAnchor(GlyphObjectId),
}

/// A malformed logical-stage value that cannot be transformed without losing
/// content.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LayoutTransformError {
    RegionSourceIsNotRegion(LayoutObjectId),
    CrossRegionObjectHasNoRegion(LayoutObjectId),
}

impl ConstrainedLayoutIR {
    /// Validates the cross-reference and finite-geometry invariants consumed by
    /// every constraint solver. Invalid public values are rejected before a
    /// solver can report `Solved` or emit non-canonical geometry.
    pub fn validate(&self) -> Result<(), ConstrainedValidationError> {
        let mut glyphs_by_id = BTreeMap::new();
        for glyph in &self.glyphs {
            let id = glyph.id();
            if glyphs_by_id.insert(id, glyph).is_some() {
                return Err(ConstrainedValidationError::DuplicateGlyphId(id));
            }
            let bounds = glyph.bounding_box;
            let valid_bounds = [bounds.left.0, bounds.bottom.0, bounds.right.0, bounds.top.0]
                .iter()
                .all(|value| value.is_finite())
                && bounds.left.0 <= bounds.right.0
                && bounds.bottom.0 <= bounds.top.0;
            if !valid_bounds {
                return Err(ConstrainedValidationError::InvalidGlyphBounds(id));
            }
            if glyph.baseline.quantize().is_none() || glyph.anchor.quantize().is_none() {
                return Err(ConstrainedValidationError::InvalidGeometry(id));
            }
        }

        let mut slot_ids = BTreeSet::new();
        let mut slot_memberships = BTreeMap::new();
        for slot in &self.horizontal_slots {
            if !slot_ids.insert(slot.id) {
                return Err(ConstrainedValidationError::DuplicateSlotId(slot.id));
            }
            let min = slot.min_width.0;
            let preferred = slot.preferred_width.0;
            let max_valid = match slot.max_width {
                Some(maximum) => maximum.0.is_finite() && maximum.0 >= preferred,
                None => true,
            };
            if !min.is_finite()
                || !preferred.is_finite()
                || min < 0.0
                || preferred < min
                || !max_valid
                || !slot.stretch_factor.is_finite()
                || !slot.compress_factor.is_finite()
                || slot.stretch_factor < 0.0
                || slot.compress_factor < 0.0
            {
                return Err(ConstrainedValidationError::InvalidSlotGeometry(slot.id));
            }
            // A spring slot must contain at least one glyph: the spacing solver
            // derives each slot's source x from a member glyph, so an empty slot
            // is a slot whose target the engraver could not map back. A
            // stroke-only column carries no slot at all (Chapter 7 §"Constraints").
            if slot.members.is_empty() {
                return Err(ConstrainedValidationError::EmptySlot(slot.id));
            }
            for member in &slot.members {
                let Some(glyph) = glyphs_by_id.get(member) else {
                    return Err(ConstrainedValidationError::UnknownSlotMember(*member));
                };
                if slot_memberships.insert(*member, slot.id).is_some() {
                    return Err(ConstrainedValidationError::DuplicateSlotMember(*member));
                }
                if glyph.horizontal_slot != slot.id {
                    return Err(ConstrainedValidationError::SlotMismatch(*member));
                }
            }
        }

        let mut band_ids = BTreeSet::new();
        let mut memberships = BTreeMap::new();
        for band in &self.vertical_bands {
            if !band_ids.insert(band.id) {
                return Err(ConstrainedValidationError::DuplicateBandId(band.id));
            }
            let min = band.min_height.0;
            let preferred = band.preferred_height.0;
            let max = band.max_height.map(|height| height.0);
            let valid_heights = min.is_finite()
                && preferred.is_finite()
                && min >= 0.0
                && preferred >= min
                && match max {
                    Some(maximum) => maximum.is_finite() && maximum >= preferred,
                    None => true,
                };
            if !valid_heights
                || !band.stretch_factor.is_finite()
                || !band.compress_factor.is_finite()
                || band.stretch_factor < 0.0
                || band.compress_factor < 0.0
            {
                return Err(ConstrainedValidationError::InvalidBandGeometry(band.id));
            }
            for member in &band.members {
                let Some(glyph) = glyphs_by_id.get(member) else {
                    return Err(ConstrainedValidationError::UnknownBandMember(*member));
                };
                if memberships.insert(*member, band.id).is_some() {
                    return Err(ConstrainedValidationError::DuplicateBandMember(*member));
                }
                if glyph.vertical_band != band.id {
                    return Err(ConstrainedValidationError::BandMismatch(*member));
                }
            }
        }

        for glyph in &self.glyphs {
            if !slot_ids.contains(&glyph.horizontal_slot) {
                return Err(ConstrainedValidationError::UnknownSlot(
                    glyph.horizontal_slot,
                ));
            }
            if slot_memberships.get(&glyph.id()) != Some(&glyph.horizontal_slot) {
                return Err(ConstrainedValidationError::SlotMismatch(glyph.id()));
            }
            if !band_ids.contains(&glyph.vertical_band) {
                return Err(ConstrainedValidationError::UnknownBand(glyph.vertical_band));
            }
            if memberships.get(&glyph.id()) != Some(&glyph.vertical_band) {
                return Err(ConstrainedValidationError::BandMismatch(glyph.id()));
            }
        }

        // Constraints must reference objects that exist: a dangling glyph or
        // slot reference is a malformed problem, not a silently-accepted one.
        let glyph_exists = |id: GlyphObjectId| -> bool { glyphs_by_id.contains_key(&id) };
        for constraint in &self.constraints {
            match constraint {
                LayoutConstraint::NoCollision { a, b } | LayoutConstraint::Align { a, b, .. } => {
                    if !glyph_exists(*a) {
                        return Err(ConstrainedValidationError::UnknownConstraintGlyph(*a));
                    }
                    if !glyph_exists(*b) {
                        return Err(ConstrainedValidationError::UnknownConstraintGlyph(*b));
                    }
                }
                LayoutConstraint::PositionWithin { glyph, region } => {
                    if !glyph_exists(*glyph) {
                        return Err(ConstrainedValidationError::UnknownConstraintGlyph(*glyph));
                    }
                    let r = [
                        region.origin.x.0,
                        region.origin.y.0,
                        region.size.width.0,
                        region.size.height.0,
                    ];
                    let region_ok = r.iter().all(|v| v.is_finite())
                        && region.size.width.0 >= 0.0
                        && region.size.height.0 >= 0.0;
                    if !region_ok {
                        return Err(ConstrainedValidationError::InvalidConstraintRegion(*glyph));
                    }
                }
                LayoutConstraint::SystemBreakAt { slot, .. }
                | LayoutConstraint::PageBreakAt { slot, .. } => {
                    if !slot_ids.contains(slot) {
                        return Err(ConstrainedValidationError::UnknownConstraintSlot(*slot));
                    }
                }
                // A Registered (extension) constraint is opaque; treated
                // conservatively (not rejected) per "Behavior Under Unknown
                // Extensions".
                LayoutConstraint::Registered(_, _) => {}
            }
        }

        for stroke in &self.strokes {
            // Endpoints and thickness must all quantize (finite *and* in canonical
            // range) — a finite-but-out-of-range value would validate yet panic in
            // `canonical_bytes`. Thickness must additionally be non-negative.
            let geometry_quantizes = stroke.from.quantize().is_some()
                && stroke.to.quantize().is_some()
                && stroke.thickness.quantize().is_some();
            if !geometry_quantizes || stroke.thickness.0 < 0.0 {
                return Err(ConstrainedValidationError::InvalidStrokeGeometry(
                    stroke.id(),
                ));
            }
            // A stroke's band is a one-way reference (it is not in `members`),
            // so the only thing to enforce is that it names a band that exists —
            // a dangling one would silently drop the stroke out of the vertical
            // solve's attribution.
            if !band_ids.contains(&stroke.vertical_band) {
                return Err(ConstrainedValidationError::UnknownBand(
                    stroke.vertical_band,
                ));
            }
        }

        for curve in &self.curves {
            // Every control point and the thickness must quantize; thickness
            // non-negative — the same discipline as a stroke's geometry.
            let geometry_quantizes = curve
                .control_points()
                .iter()
                .all(|point| point.quantize().is_some())
                && curve.thickness.quantize().is_some();
            if !geometry_quantizes || curve.thickness.0 < 0.0 {
                return Err(ConstrainedValidationError::InvalidCurveGeometry(curve.id()));
            }
            if !band_ids.contains(&curve.vertical_band) {
                return Err(ConstrainedValidationError::UnknownBand(curve.vertical_band));
            }
        }

        let spanning: BTreeSet<GlyphObjectId> = self
            .strokes
            .iter()
            .map(Stroke::id)
            .chain(self.curves.iter().map(Curve::id))
            .collect();
        for anchor in &self.span_anchors {
            if !spanning.contains(&anchor.primitive)
                || !slot_ids.contains(&anchor.start)
                || !slot_ids.contains(&anchor.end)
            {
                return Err(ConstrainedValidationError::DanglingSpanAnchor(
                    anchor.primitive,
                ));
            }
        }
        Ok(())
    }
}

// Minimal-tier engraving geometry, in staff spaces (Chapter 7 §7.2). These are
// fixed defaults, not yet solver-negotiated: the stub solver returns this
// geometry verbatim, so it is what the renderer draws.
const STAFF_LINE_THICKNESS: f32 = 0.13;
const STEM_THICKNESS: f32 = 0.12;
// One octave from the outer notehead — but a stem on a note beyond the staff is
// drawn out to the middle line instead, so it never dangles in the ledger field.
const STEM_LENGTH: f32 = 3.5;
const STAFF_HEIGHT: f32 = 4.0; // 4 spaces between the outer lines of a 5-line staff
const SYSTEM_STAFF_PITCH: f32 = 12.0; // vertical distance between stacked staves
const CLEF_X: f32 = 0.0;
const FIRST_COLUMN_X: f32 = 3.0; // x of the first time column (right of the clef)
const COLUMN_X_STEP: f32 = 1.6; // x advance per distinct musical time column
const COLUMN_PREFERRED_WIDTH: f32 = 1.5; // a column's spring preferred width
const STAFF_LEFT_MARGIN: f32 = 1.0; // staff line extends this far left of the clef
const STAFF_RIGHT_MARGIN: f32 = 2.0; // …and this far right of the last column
const REGION_GAP: f32 = 4.0; // horizontal gap between regions (no page layout in v0)
                             // Fallback stem attachment when a notehead's metrics are absent; a bundled head
                             // uses its own bounding box (right edge for an up-stem, left for a down-stem —
                             // SMuFL's `stemUpSE` / `stemDownNW`, which for `noteheadBlack` are x = 1.18 / 0).
const NOTEHEAD_STEM_X: f32 = 1.15;
const ACCIDENTAL_GAP: f32 = 0.2; // an accidental's ink stands this far clear of a head or ledger line
const ACCIDENTAL_STACK_GAP: f32 = 0.15; // …and this far clear of another accidental
const SECOND_CLEARANCE: f32 = 0.02; // heads set beside each other stand this far apart
const KEY_GAP: f32 = 0.6; // the gap between a clef's ink and the key signature after it
const LEAD_GAP: f32 = 0.8; // the gap between a lead's ink and the first column after it
const KEY_ACC_X: f32 = 0.9; // x advance per key-signature accidental
const TIME_SIG_X: f32 = 0.5; // a time signature sits this far right of its barline
const SIGNATURE_GAP: f32 = 1.0; // the gap between a time signature's ink and the music after it
const CLEF_CHANGE_GAP: f32 = 0.5; // the gap between a clef change's ink and the barline or note after it
const CLEF_NUMERAL_OVERLAP: f32 = 0.1; // how far an octave numeral reaches into its clef's box, as Bravura's octave clefs draw it
const REST_VOICE_SHIFT: f32 = 1.0; // how far a rest beside another voice moves off its place
const TIME_DIGIT_X: f32 = 0.8; // x advance per time-signature digit
                               // Repeat/volta engraving defaults (Minimal tier; SMuFL engraving-default
                               // neighborhood, not solver-negotiated).
const REPEAT_DOTS_SEPARATION: f32 = 0.16; // gap between the dot pair and the barline it decorates
const VOLTA_Y: f32 = 6.5; // the bracket line, above the top staff's bottom line
const VOLTA_HOOK: f32 = 1.4; // the descending hook at each bracket end
const VOLTA_LINE_THICKNESS: f32 = 0.16; // SMuFL repeatEndingLineThickness default
const VOLTA_TEXT_X: f32 = 0.4; // the first ending digit sits this far right of the bracket start
const VOLTA_TEXT_DROP: f32 = 1.3; // ending-digit baseline, below the bracket line
const VOLTA_ENDING_GAP: f32 = 0.5; // extra gap between successive ending numbers
                                   // Slur engraving defaults (Minimal tier; a symmetric cubic arc — Push 3 refines
                                   // with collision-aware shaping).
                                   // A slur's endpoints and its arc clear the notes by this much, on the arc's side.
const SLUR_ENDPOINT_GAP: f32 = 0.7;
const SLUR_HEIGHT_FACTOR: f32 = 0.16; // auto arc apex height as a fraction of span width
const SLUR_MIN_HEIGHT: f32 = 0.8; // …clamped to at least this many staff spaces
const SLUR_MAX_HEIGHT: f32 = 3.0; // …and at most this many
const SLUR_THICKNESS: f32 = 0.12; // default line thickness when the style declares none

/// The horizontal half-reach of an emitted `PositionWithin` region, in staff
/// spaces. The constrained stage performs no casting-off, so a region imposes
/// no *horizontal* bound on its glyphs — a conformant solver may re-space
/// columns freely along the open canvas. The containment obligation this stage
/// can honestly state is the **vertical** envelope (which the spacing pass
/// computes from the very glyph geometry it emits), so the emitted rect pins
/// that envelope and leaves the horizontal span at canvas scale: wide enough
/// for any plausible re-spacing, finite because the validator rejects
/// non-finite constraint regions. Geometric constraints are expressed — and
/// evaluated — in *this stage's frame*: a casting-off solver that relocates
/// whole systems (a per-system rigid motion) evaluates them against its
/// pre-casting spaced geometry, where the obligation is meaningful (see
/// `epiphany-engrave`).
const POSITION_WITHIN_X_REACH: f32 = 1.0e6;

/// The registry id for the engraver's **structural-line synthesis** (staff
/// lines). The normative [`SynthesisKind`] set names *musical* synthesized
/// objects (cancellation accidentals, generated rests, …) but no purely visual
/// rule like a staff line; the codebase-wide convention is that a kind the core
/// vocabulary does not close is carried as a `Registered(...Id)` extension
/// (Chapter 7 §"Behavior Under Unknown Extensions"; see DECISIONS.md). A staff's
/// five lines share its source, so four of them must be synthesized to earn
/// distinct stable ids; this is the kind they declare.
const STAFF_LINE_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x5354_4146_465F_4C4E); // "STAFFLN"
const LEDGER_LINE_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x4C45_4447_4552_4C4E); // "LEDGERLN"
const LEDGER_LINE_EXTENSION: f32 = 0.3; // a ledger line reaches this far past the notehead, each side
const DOT_GAP: f32 = 0.25; // the first augmentation dot's gap right of its notehead or rest
const DOT_STEP: f32 = 0.5; // x advance per further augmentation dot
/// The registry id for an augmentation dot, synthesized from the notehead's (or
/// rest's) source and keyed by component and dot.
const DOT_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x4155_474D_444F_5453); // "AUGMDOTS"
/// The registry id for a flag, synthesized from its event and keyed by
/// component.
const FLAG_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x464C_4147_474C_5948); // "FLAGGLYH"
/// The registry id for an unpitched note's stem, synthesized from its event
/// (whose exact provenance its first notehead carries) and keyed by component.
const STEM_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x5354_454D_5354_524B); // "STEMSTRK"
/// The registry id for a beam stroke, synthesized from the score's beam, or
/// from the first note of a group the meter beams, and keyed by group, level
/// and run.
const BEAM_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x4245_414D_5354_524B); // "BEAMSTRK"
/// The registry id for a tie arc after a structure's first, or between the
/// tied components of one note, synthesized from the tie or the pitch.
const TIE_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x5449_4541_5243_5321); // "TIEARCS!"
const TIE_GAP: f32 = 0.15; // the gap between a tie's end and its notehead
/// A tuplet's number and the bracket beside it, synthesized from the tuplet
/// (its first digit carries the tuplet's own provenance).
const TUPLET_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x5455_504C_4554_4E4F); // "TUPLETNO"
const TUPLET_CLEARANCE: f32 = 0.5; // a tuplet's number or bracket stands this far clear of its notes' ink
const TUPLET_HOOK: f32 = 0.6; // the length of a bracket's end hooks, toward the notes
const TUPLET_NUMBER_GAP: f32 = 0.25; // the gap each side of the number in its bracket
const TUPLET_BRACKET_THICKNESS: f32 = 0.16; // SMuFL's tupletBracketThickness
const TIE_OFFSET: f32 = 0.4; // how far off its heads' centres a tie's ends sit
const TIE_MIN_HEIGHT: f32 = 0.3; // a tie's apex height, at least…
const TIE_MAX_HEIGHT: f32 = 0.8; // …and at most, in staff spaces
const TIE_THICKNESS: f32 = 0.14;
const BEAM_THICKNESS: f32 = 0.5; // SMuFL's beamThickness
const BEAM_STEP: f32 = 0.75; // centre to centre of stacked beams: a thickness and a 0.25 gap
const MAX_BEAM_RISE: f32 = 1.0; // the most a beam rises or falls across its group, in staff spaces
const BEAM_HOOK: f32 = 1.1; // the length of a lone note's partial beam

/// The registry id for **notated-component synthesis**: a note/rest notated as a
/// tied decomposition (e.g. a quarter tied to an eighth across a barline) draws
/// one notehead/stem/rest *per component*, but the pitch and event each have only
/// one source. The first component carries that exact source; later components
/// are synthesized from it, again via the `Registered` hatch for a kind the
/// normative set does not name.
const COMPONENT_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x434F_4D50_4F4E_4E54); // "COMPONNT"

/// The registry id for **accidental synthesis**: a pitch's spelling accidental
/// (sharp, flat, natural, …) is a second glyph for the same pitch — the notehead
/// carries the pitch's exact provenance, so the accidental, needing a distinct
/// stable id, is synthesized from it via the same `Registered` hatch.
const ACCIDENTAL_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x4143_4349_4445_4E54); // "ACCIDENT"

/// The registry id for **key-signature synthesis**: the staff instance carries
/// the key, but its accidental glyphs (the sharp/flat zigzag) each need a
/// distinct stable id, so they are synthesized from the staff instance.
const KEY_SIG_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x4B45_5953_4947_4E5F); // "KEYSIGN_"

/// The registry id for a **clef change**: the staff instance carries its clef
/// sequence, and each change it draws within a system (the clef, and an
/// octave clef's numeral) is synthesized from it, keyed by the change's place
/// among the drawn changes and the glyph's within the change.
const CLEF_CHANGE_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x434C_4546_4348_4E47); // "CLEFCHNG"

/// The registry id for **time-signature synthesis**: the measure introduces the
/// meter, but its numerator/denominator digit glyphs each need a distinct stable
/// id, so they are synthesized from the measure.
const TIME_SIG_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x54_494D_4553_4947); // "TIMESIG"

/// The registry id for **repeat-barline synthesis**: a repeat sign drawn where
/// no measure barline stands (a mid-measure boundary, a region edge without a
/// final barline) or the dot pair beside a final barline. The repeat
/// structure's own exact provenance stays on its traced anchor, so every ink
/// primitive it owns is synthesized from it. The instance key is
/// `(boundary site << 32) | staff index` — a **semantic** identity (site 0 =
/// the owner's start boundary, 1 = its end; a structure has one of each, and
/// each lands on one column), so the id survives unrelated edits where a
/// positional column rank would re-derive.
const REPEAT_BARLINE_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x5245_5045_4154_424C); // "REPEATBL"

/// The registry id for **volta-bracket synthesis**: each bracket's three
/// strokes and its ending-number digit glyphs, synthesized from the owning
/// repeat structure. The instance key is `(volta index << 64) | element`,
/// elements `0..=2` the strokes (line, start hook, end hook) and `3 +` the
/// digits in drawing order — the element field is 64 bits wide so an
/// adversarially long endings list cannot bleed into the volta-index bits
/// (the same non-overlap discipline as [`ledger_line_key`]).
const VOLTA_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x564F_4C54_4142_524B); // "VOLTABRK"

/// Flattens [`LogicalLayoutIR`] into [`ConstrainedLayoutIR`], engraving each
/// layout object into the notation primitive that represents it: a **glyph** for
/// the SMuFL objects (a pitch's notehead at its clef-relative staff position, a
/// staff instance's clef, a rest, a measure's barline) and a **stroke** for the
/// line primitives (staff lines, stems). Every logical object is covered by
/// exactly one primitive carrying *its* provenance, so the round-trip's
/// source-set surjection holds; derived primitives a single object owns more than
/// one of (the four upper staff lines, a tied note's later components) are
/// [`Provenance::synthesized`] from it, earning distinct stable ids without
/// inventing a spurious source.
///
/// **Spacing** is column-based: the region's distinct musical times become
/// spring slots (one per column, a barline column sorting before the notes at the
/// same onset, the clef in a lead column), so chord/simultaneous glyphs share a
/// slot and the time axis maps musical time to its column. Regions are laid out
/// left-to-right (no page casting-off in v0), so every coordinate is globally
/// monotonic — which is what lets a real solver re-space glyphs *and* the strokes
/// that track them by a single coordinate map.
///
/// Every primitive — glyph, stroke, curve — is routed to the band of its own
/// staff (Chapter 7 §"Vertical Bands"), so a vertical solver reads a primitive's
/// owner rather than inferring it from geometry. Only glyphs become band
/// *members* (membership realizes the spring solve); a stroke's or curve's band
/// is a one-way declaration. Structural objects with no Minimal-tier glyph
/// (regions, voices, ties, slurs, beams, …) are carried as zero-extent traced
/// anchors so provenance survives, pending their engraving in a higher tier.
///
/// **Repeat structures** draw real ink: a boundary of a barline-drawing kind
/// morphs the coinciding measure barline into the composite SMuFL repeat sign
/// (or stands alone at its own column when no measure barline coincides; an
/// end repeat closing on the final barline adds the dot pair beside it), and
/// each volta draws a bracket above the top staff with its ending numbers as
/// digit glyphs. The structure's exact provenance stays on its traced anchor;
/// all of its ink is synthesized from it.
pub fn to_constrained(logical: &LogicalLayoutIR) -> ConstrainedLayoutIR {
    try_to_constrained(logical).expect("LogicalLayoutIR is malformed")
}

/// Fallible form of [`to_constrained`] for callers accepting externally built
/// logical IR. It rejects malformed provenance rather than silently dropping a
/// region or spanning object.
pub fn try_to_constrained(
    logical: &LogicalLayoutIR,
) -> Result<ConstrainedLayoutIR, LayoutTransformError> {
    let mut glyphs = Vec::new();
    let mut strokes = Vec::new();
    let mut curves = Vec::new();
    let mut diagnostics = Vec::new();
    let mut vertical_bands = Vec::new();
    let mut horizontal_slots = Vec::new();
    let mut constraints = Vec::new();
    let mut break_origins = Vec::new();
    let mut constrained_regions = Vec::new();
    let mut span_anchors: Vec<SpanAnchor> = Vec::new();
    let mut system_leads: Vec<SystemLead> = Vec::new();
    let mut staff_groups: Vec<GroupSpan> = Vec::new();
    // Regions tile left-to-right; this advances by each region's width so all
    // coordinates stay globally monotonic (the solver's coordinate remap relies
    // on it). v0 has no page casting-off, so this replaces region overlap.
    let mut region_x: f32 = 0.0;

    for (region_index, region) in logical.regions.iter().enumerate() {
        let region_id = match region.provenance.source {
            TypedObjectId::Region(id) => id,
            _ => {
                return Err(LayoutTransformError::RegionSourceIsNotRegion(
                    region.provenance.stable_id,
                ))
            }
        };
        let region_layout_id = region.provenance.stable_id;
        let band_of = |staff: Option<StaffId>| -> VerticalBandId {
            match staff {
                Some(s) => {
                    VerticalBandId(manifestation_layout_id(&TypedObjectId::Staff(s), region_id).0)
                }
                None => VerticalBandId(region_layout_id.0),
            }
        };

        // Vertical layout: stack the region's staves top-to-bottom, the first at
        // y = 0 and each later one `SYSTEM_STAFF_PITCH` below. A staff's bottom
        // line sits at its origin; a `StaffStep` is half a staff space above it.
        let mut staff_order: Vec<StaffId> = region.vertical_extent.staves.clone();
        for object in &region.objects {
            if let Some(staff) = object.staff() {
                if !staff_order.contains(&staff) {
                    staff_order.push(staff);
                }
            }
        }
        let y_origin = |staff: StaffId| -> f32 {
            -(staff_order.iter().position(|s| *s == staff).unwrap_or(0) as f32) * SYSTEM_STAFF_PITCH
        };

        // The clef *sequence* in force on each staff (a staff instance carries it).
        // The active clef at a given position is the latest change at or before it,
        // so a mid-staff clef change moves later pitches without affecting earlier
        // ones. An empty sequence falls back to the staff's own `default_clef`.
        let mut clef_seq_of: BTreeMap<StaffId, Vec<PlacedClef>> = BTreeMap::new();
        let mut clef_default_of: BTreeMap<StaffId, Clef> = BTreeMap::new();
        for object in &region.objects {
            if let (Some(staff), LayoutContent::Staff(content)) = (object.staff(), object.content())
            {
                clef_seq_of
                    .entry(staff)
                    .or_insert_with(|| content.clefs.clone());
                clef_default_of.entry(staff).or_insert(content.default_clef);
            }
        }
        let clef_seq = |staff: Option<StaffId>| -> &[PlacedClef] {
            staff
                .and_then(|s| clef_seq_of.get(&s))
                .map(Vec::as_slice)
                .unwrap_or(&[])
        };
        // A staff's own default clef, in force before its first `ClefChange`.
        let clef_default = |staff: Option<StaffId>| -> Clef {
            staff
                .and_then(|s| clef_default_of.get(&s))
                .copied()
                .unwrap_or_default()
        };

        // Accidentals in context: which accidental each pitch's first head shows.
        let shown_accidentals = context_accidentals(&region.objects);

        // Pass 1 — compute every glyph's notation, keyed for emission in pass 2,
        // and collect the distinct columns it occupies. A note/rest notated as a
        // multi-component (tied) decomposition yields one notehead/stem/rest per
        // component, each at `position + component.offset`.
        let mut pitch_heads: BTreeMap<PitchId, Vec<Head>> = BTreeMap::new();
        let mut unpitched_heads: BTreeMap<EventId, Vec<Head>> = BTreeMap::new();
        // Every head of each staff's column, whatever its voice: what an
        // accidental beside one of them must clear.
        let mut column_heads: BTreeMap<(Option<StaffId>, ColumnKey), Vec<HeadRef>> =
            BTreeMap::new();
        let mut event_stems: BTreeMap<EventId, Vec<StemSeg>> = BTreeMap::new();
        // Each note's and unpitched note's place among its staff's voices.
        let mut event_voices: BTreeMap<EventId, VoicePlace> = BTreeMap::new();
        // Per staff, the drawn extent of each note column — the obstacle field a
        // slur must arc clear of, and the stem direction it takes its side from.
        // Columns are shared between the staves of a system (they share an x), so
        // this is keyed by staff as well as column.
        let mut column_ink: BTreeMap<(StaffId, ColumnKey), ColumnInk> = BTreeMap::new();
        let mut event_rests: BTreeMap<EventId, Vec<RestSeg>> = BTreeMap::new();
        let mut sounding: Vec<Sounding> = Vec::new();
        // Every column that needs an x. A column earns a spring slot only if a
        // glyph actually lands in it (decided after emission, by occupancy), so a
        // stroke-only column — e.g. an unbundled rest, or a pitch-less note — gets
        // an x but never an empty slot the solver would have to position.
        let mut keys: BTreeSet<ColumnKey> = BTreeSet::new();
        // How far a column's content overhangs *left* of its noteheads (the
        // accidental zone). The source layout separates this column from the
        // previous one by this much extra, so a note's accidental does not overlap
        // the previous note (the engraver's monotonic remap cannot un-overlap it).
        let mut column_overhang: BTreeMap<ColumnKey, f32> = BTreeMap::new();
        // How far each clef-change column's ink reaches right of it.
        let mut clef_reach: BTreeMap<ColumnKey, f32> = BTreeMap::new();
        // The right edge of the widest lead (clef and key signature): the first
        // note column clears it.
        let mut lead_right = 0.0f32;
        // This region's repeat structures (engraving content projected by the
        // logical stage), and every (column, staff) a measure's own barline
        // occupies — repeat signs replace a coinciding measure barline (pass 2
        // morphs its glyph) and stand alone elsewhere.
        let mut repeats: Vec<(RepeatStructureId, &RepeatContent)> = Vec::new();
        let mut measure_cols: BTreeSet<(ColumnKey, StaffId)> = BTreeSet::new();
        // The staves whose region-closing barline is a final one (the rest
        // close on a single barline, their run continuing in a later region).
        let mut final_staves: BTreeSet<StaffId> = BTreeSet::new();

        for object in &region.objects {
            let staff = object.staff();
            let yo = staff.map(&y_origin).unwrap_or(0.0);
            match (object.provenance().source, object.content()) {
                (TypedObjectId::Event(eid), LayoutContent::Note(note)) => {
                    let mut stems = Vec::new();
                    for (comp, (offset, value, dots, tied)) in
                        components_of(&note.components).enumerate()
                    {
                        let time = shift_time(&note.position, &offset);
                        let key = ColumnKey::Timed(time.clone(), ColumnRole::Note);
                        keys.insert(key.clone());
                        let clef = active_clef_or(clef_seq(staff), &time, clef_default(staff));
                        let name = notehead_glyph(value);
                        let mut placed = Vec::with_capacity(note.pitches.len());
                        for pitch in &note.pitches {
                            let (step, missing) = spelling_step(&pitch.spelling, &clef);
                            if missing {
                                diagnostics.push(LayoutDiagnostic {
                                    source: TypedObjectId::Pitch(pitch.pitch),
                                    kind: LayoutDiagnosticKind::MissingSpelling,
                                });
                            }
                            // The accidental the key and the measure call for draws on
                            // the first component only; a spelling whose alteration
                            // is not whole semitones draws its own stack, and an
                            // unbundled (microtonal) one is surfaced, not guessed.
                            let accidentals = match (comp, shown_accidentals.get(&pitch.pitch)) {
                                (0, Some(shown)) => shown.clone(),
                                (0, None) => pitch_accidentals(
                                    &pitch.spelling,
                                    pitch.pitch,
                                    &mut diagnostics,
                                ),
                                _ => Vec::new(),
                            };
                            let alteration = pitch.spelling.as_ref().and_then(|spelling| {
                                matches!(spelling.nominal, SpellingNominal::Cmn(_))
                                    .then(|| stack_alteration(&spelling.accidentals))
                                    .flatten()
                            });
                            placed.push((pitch.pitch, step, accidentals, alteration));
                        }
                        let steps: Vec<StaffStep> =
                            placed.iter().map(|(_, step, _, _)| *step).collect();
                        sounding.push(Sounding {
                            staff,
                            event: eid,
                            start: time.clone(),
                            ys: steps.iter().map(|s| step_to_y(yo, *s)).collect(),
                        });
                        let dot_ys = dot_positions(yo, &steps, note.voice == VoicePlace::Lower);
                        for ((pitch, step, accidentals, alteration), dot_y) in
                            placed.into_iter().zip(dot_ys)
                        {
                            let heads = pitch_heads.entry(pitch).or_default();
                            column_heads
                                .entry((staff, key.clone()))
                                .or_default()
                                .push(HeadRef::Pitch(pitch, heads.len()));
                            heads.push(Head {
                                name,
                                key: key.clone(),
                                y: step_to_y(yo, step),
                                step,
                                comp,
                                accidentals,
                                accidental_x: Vec::new(),
                                dx: 0.0,
                                alteration,
                                dots,
                                dot_y,
                                dot_x: 0.0,
                                event: eid,
                                tied,
                            });
                        }
                        let fallback = step_to_y(yo, reference_step(&clef));
                        let (seg, ink) = note_stem(
                            value,
                            yo,
                            &steps,
                            fallback,
                            name,
                            key,
                            comp,
                            voiced_up(note.voice),
                        );
                        if let Some(staff) = staff {
                            merge_ink(&mut column_ink, staff, &seg.key, ink);
                        }
                        stems.push(seg);
                    }
                    event_stems.insert(eid, stems);
                    event_voices.insert(eid, note.voice);
                }
                (TypedObjectId::Event(eid), LayoutContent::Unpitched(unpitched)) => {
                    // An unpitched note: a notehead at its staff position, read
                    // as on a five-line staff, with the stem, flag and dots of
                    // its value; it has no accidental.
                    let mut stems = Vec::new();
                    let step = StaffStep::from(unpitched.staff_position.0);
                    for (comp, (offset, value, dots, tied)) in
                        components_of(&unpitched.components).enumerate()
                    {
                        let time = shift_time(&unpitched.position, &offset);
                        sounding.push(Sounding {
                            staff,
                            event: eid,
                            start: time.clone(),
                            ys: vec![step_to_y(yo, step)],
                        });
                        let key = ColumnKey::Timed(time, ColumnRole::Note);
                        keys.insert(key.clone());
                        let name = notehead_glyph(value);
                        let dot_y =
                            dot_positions(yo, &[step], unpitched.voice == VoicePlace::Lower)[0];
                        let heads = unpitched_heads.entry(eid).or_default();
                        column_heads
                            .entry((staff, key.clone()))
                            .or_default()
                            .push(HeadRef::Unpitched(eid, heads.len()));
                        heads.push(Head {
                            name,
                            key: key.clone(),
                            y: step_to_y(yo, step),
                            step,
                            comp,
                            accidentals: Vec::new(),
                            accidental_x: Vec::new(),
                            dx: 0.0,
                            alteration: None,
                            dots,
                            dot_y,
                            dot_x: 0.0,
                            event: eid,
                            tied,
                        });
                        let fallback = step_to_y(yo, step);
                        let (seg, ink) = note_stem(
                            value,
                            yo,
                            &[step],
                            fallback,
                            name,
                            key,
                            comp,
                            voiced_up(unpitched.voice),
                        );
                        if let Some(staff) = staff {
                            merge_ink(&mut column_ink, staff, &seg.key, ink);
                        }
                        stems.push(seg);
                    }
                    event_stems.insert(eid, stems);
                    event_voices.insert(eid, unpitched.voice);
                }
                (TypedObjectId::Event(eid), LayoutContent::Rest(rest)) => {
                    let mut segs = Vec::new();
                    for (comp, (offset, value, dots, _)) in
                        components_of(&rest.components).enumerate()
                    {
                        let time = shift_time(&rest.position, &offset);
                        // Every rest component occupies its musical onset column,
                        // whether or not a glyph is bundled for its value — an
                        // unbundled rest is a traced anchor *there*, not at a
                        // default x, and later components do not vanish. A
                        // hidden rest keeps its column and draws nothing.
                        let key = ColumnKey::Timed(time.clone(), ColumnRole::Note);
                        keys.insert(key.clone());
                        // A rest filling its measure is a whole rest in any meter,
                        // hanging from the fourth line; every other rest sits on
                        // the middle line. Beside another voice, an upper voice's
                        // rest moves up a space and a lower voice's down one.
                        let (name, dots) = if rest.whole_measure {
                            (Some("restWhole"), 0)
                        } else {
                            (rest_glyph(value), dots)
                        };
                        let y = if name == Some("restWhole") {
                            yo + STAFF_HEIGHT * 0.75
                        } else {
                            yo + STAFF_HEIGHT / 2.0
                        };
                        let y = match rest.voice {
                            VoicePlace::Alone => y,
                            VoicePlace::Upper => y + REST_VOICE_SHIFT,
                            VoicePlace::Lower => y - REST_VOICE_SHIFT,
                        };
                        segs.push(RestSeg {
                            name,
                            key,
                            y,
                            comp,
                            visible: rest.visible,
                            dots,
                            staff,
                            start: time,
                            voice: rest.voice,
                        });
                        if rest.whole_measure {
                            break;
                        }
                    }
                    event_rests.insert(eid, segs);
                }
                (TypedObjectId::Measure(_), LayoutContent::Measure(measure)) => {
                    let key = measure_column(measure);
                    keys.insert(key.clone());
                    if measure.time_signature.is_some() {
                        keys.insert(signature_column(measure));
                    }
                    if let Some(s) = staff {
                        if key == ColumnKey::End && measure.barline == BarlineKind::Final {
                            final_staves.insert(s);
                        }
                        measure_cols.insert((key, s));
                    }
                }
                (TypedObjectId::Measure(_), _) => {
                    // Pass 2 renders malformed/missing measure content as a final
                    // barline, so collect that fallback column here instead of
                    // letting the fallible conversion panic.
                    keys.insert(ColumnKey::End);
                    if let Some(s) = staff {
                        final_staves.insert(s);
                        measure_cols.insert((ColumnKey::End, s));
                    }
                }
                (TypedObjectId::RepeatStructure(id), LayoutContent::Repeat(content)) => {
                    // A barline-drawing repeat's boundaries are real spacing
                    // columns (a mid-measure boundary mints one of its own).
                    if content.barlines {
                        for placement in [&content.start, &content.end] {
                            if let Some(key) = placement_column_key(placement) {
                                keys.insert(key);
                            }
                        }
                    }
                    repeats.push((id, content));
                }
                (TypedObjectId::StaffInstance(_), LayoutContent::Staff(content)) => {
                    // The staff instance's clef and key signature occupy the lead
                    // column; the first note column clears the widest of them.
                    let glyphs = lead_glyphs(content, &origin(), yo);
                    if !glyphs.is_empty() {
                        keys.insert(ColumnKey::Lead);
                        lead_right = lead_right.max(lead_extent(&glyphs));
                    }
                    // Each clef change in a column of its own at its time,
                    // before the barline or notes there.
                    for (time, clef) in drawn_clef_changes(content) {
                        let key = ColumnKey::Timed(time, ColumnRole::Clef);
                        let reach = lead_extent(&clef_change_glyphs(&clef, yo));
                        let entry = clef_reach.entry(key.clone()).or_insert(0.0);
                        *entry = entry.max(reach);
                        keys.insert(key);
                    }
                    // Its later systems' leads, from each clef or key change on.
                    if let Some(staff) = staff {
                        let mut times: Vec<TimePoint> = std::iter::once(origin())
                            .chain(content.clefs.iter().map(|c| c.time.clone()))
                            .chain(content.keys.iter().map(|k| k.time.clone()))
                            .collect();
                        times.sort_by(time_total);
                        times.dedup();
                        system_leads.push(SystemLead {
                            region: region_index,
                            staff,
                            band: band_of(Some(staff)),
                            provenance: object.provenance().clone(),
                            entries: times
                                .into_iter()
                                .map(|time| {
                                    let glyphs = lead_glyphs(content, &time, yo)
                                        .into_iter()
                                        .map(|(name, x, y)| LeadGlyph {
                                            name: GlyphReference::borrowed(name),
                                            x,
                                            y,
                                        })
                                        .collect();
                                    (time, glyphs)
                                })
                                .collect(),
                        });
                    }
                }
                (TypedObjectId::StaffInstance(_), _) => {
                    // Pass 2 falls back to a default treble clef for malformed or
                    // absent staff-instance content; collect the lead column it
                    // will use.
                    keys.insert(ColumnKey::Lead);
                }
                _ => {}
            }
        }

        // A beam turns its stems before the heads are placed, since the side
        // a second's heads take follows the stem.
        for object in &region.objects {
            let (Some(staff), LayoutContent::Staff(content)) = (object.staff(), object.content())
            else {
                continue;
            };
            let middle = y_origin(staff) + STAFF_HEIGHT * 0.5;
            for group in &content.beams {
                let Some((members, up)) = beam_members(group, &event_stems, middle) else {
                    continue;
                };
                for event in members {
                    if let Some(seg) = event_stems.get_mut(&event).and_then(|s| s.first_mut()) {
                        seg.up = up;
                    }
                }
            }
        }

        // Each rest beside another voice, clear of that voice's notes.
        clear_rests(&mut event_rests, &sounding);

        // Each staff column's heads, set clear of each other, with the stem
        // of a voice that moves moving with it.
        for refs in column_heads.values() {
            let heads: Vec<&Head> = refs
                .iter()
                .map(|r| r.get(&pitch_heads, &unpitched_heads))
                .collect();
            let ups: Vec<bool> = heads
                .iter()
                .map(|head| {
                    event_stems
                        .get(&head.event)
                        .and_then(|segs| segs.iter().find(|seg| seg.comp == head.comp))
                        .is_none_or(|seg| seg.up)
                })
                .collect();
            let places: Vec<VoicePlace> = heads
                .iter()
                .map(|head| {
                    event_voices
                        .get(&head.event)
                        .copied()
                        .unwrap_or(VoicePlace::Alone)
                })
                .collect();
            let (dxs, shifts, dot_x) = place_heads(&heads, &ups, &places);
            for (r, dx) in refs.iter().zip(dxs) {
                let head = r.get_mut(&mut pitch_heads, &mut unpitched_heads);
                head.dx = dx;
                head.dot_x = dot_x;
            }
            for (event, comp, shift) in shifts {
                if let Some(seg) = event_stems
                    .get_mut(&event)
                    .and_then(|segs| segs.iter_mut().find(|seg| seg.comp == comp))
                {
                    seg.dx = shift;
                }
            }
        }

        // Each staff column's accidentals, placed together by their ink. The
        // source layout separates the column from the one before by as much
        // as they reach left of its heads (and a head set left of a stem).
        for ((_, key), refs) in &column_heads {
            let heads: Vec<&Head> = refs
                .iter()
                .map(|r| r.get(&pitch_heads, &unpitched_heads))
                .collect();
            let (origins, leftmost) = place_accidentals(&heads);
            for (r, xs) in refs.iter().zip(origins) {
                r.get_mut(&mut pitch_heads, &mut unpitched_heads)
                    .accidental_x = xs;
            }
            if leftmost < 0.0 {
                let entry = column_overhang.entry(key.clone()).or_insert(0.0);
                *entry = entry.max(-leftmost);
            }
        }

        // The repeat-barline marks: which columns carry a repeat boundary,
        // facing which way, owned by which structures. Marks from distinct
        // repeats merge (an end meeting a start draws the combined sign).
        let mut marks: BTreeMap<ColumnKey, RepeatMark> = BTreeMap::new();
        for (id, content) in &repeats {
            if !content.barlines {
                continue;
            }
            for (placement, is_start) in [(&content.start, true), (&content.end, false)] {
                let Some(key) = placement_column_key(placement) else {
                    continue;
                };
                let mark = marks.entry(key).or_default();
                let site = if is_start {
                    mark.start = true;
                    0u8
                } else {
                    mark.end = true;
                    1u8
                };
                if !mark.sources.contains(id) {
                    mark.sources.push(*id);
                }
                let candidate = (*id, site);
                mark.owner = Some(match mark.owner {
                    None => candidate,
                    Some(current) => current.min(candidate),
                });
            }
        }
        // An end-facing sign's ink reaches well left of its column (the plain
        // barline sits at the column; `repeat_sign_x` right-aligns the sign's
        // heavy line to it), so the column must clear the previous one by that
        // reach — the same separation mechanism accidentals use. Without it
        // the sign overlaps the preceding note column in the source geometry.
        for (key, mark) in &marks {
            if !mark.end || !matches!(key, ColumnKey::Timed(..)) {
                continue;
            }
            let name = repeat_sign_name(mark.start, mark.end);
            let left_reach = -(repeat_sign_x(name, 0.0)
                + metrics(name)
                    .expect("repeat sign metrics are bundled")
                    .bounding_box()
                    .left
                    .0);
            if left_reach > 0.0 {
                let entry = column_overhang.entry(key.clone()).or_insert(0.0);
                *entry = entry.max(left_reach);
            }
        }

        // A time signature stands in its own column after the barline at its
        // onset, so it clears the ink a repeat sign there reaches right of the
        // plain barline.
        let signature_times: Vec<TimePoint> = keys
            .iter()
            .filter_map(|key| match key {
                ColumnKey::Timed(time, ColumnRole::Signature) => Some(time.clone()),
                _ => None,
            })
            .collect();
        for time in signature_times {
            let barline = ColumnKey::Timed(time.clone(), ColumnRole::Barline);
            let reach = marks.get(&barline).map_or(0.0, |mark| {
                repeat_sign_right_extension(repeat_sign_name(mark.start, mark.end))
            });
            if reach > 0.0 {
                let entry = column_overhang
                    .entry(ColumnKey::Timed(time, ColumnRole::Signature))
                    .or_insert(0.0);
                *entry = entry.max(reach);
            }
        }

        // The column after a clef change clears the change's ink and gap.
        for (key, reach) in &clef_reach {
            let extra = reach + CLEF_CHANGE_GAP - COLUMN_X_STEP;
            let next = keys
                .range((std::ops::Bound::Excluded(key), std::ops::Bound::Unbounded))
                .next();
            if let (Some(next), true) = (next, extra > 0.0) {
                *column_overhang.entry(next.clone()).or_insert(0.0) += extra;
            }
        }

        // Pass 1b — turn the collected column keys into a table: each gets an x
        // (the lead at the clef, timed columns spread by rank, the final-barline
        // column at the right) and a spring slot. The table is sorted by
        // `ColumnKey`'s exact order.
        let timed_count = keys
            .iter()
            .filter(|k| matches!(k, ColumnKey::Timed(..)))
            .count();
        // The first note column clears the clef *and* the key signature; each
        // timed column additionally clears the previous one by its accidental
        // overhang, so the source layout is collision-free.
        let first_col = FIRST_COLUMN_X.max(lead_right + LEAD_GAP);
        let total_overhang: f32 = column_overhang.values().sum();
        let local_right =
            first_col + total_overhang + timed_count as f32 * COLUMN_X_STEP + STAFF_RIGHT_MARGIN;
        let staff_left = region_x + CLEF_X - STAFF_LEFT_MARGIN;
        let staff_right = region_x + local_right;
        let mut columns: BTreeMap<ColumnKey, ColumnInfo> = BTreeMap::new();
        let mut timed_x = first_col;
        for (rank, key) in keys.iter().enumerate() {
            let x = match key {
                ColumnKey::Lead => region_x + CLEF_X,
                ColumnKey::Timed(..) => {
                    // Push right of the previous column by this column's overhang.
                    timed_x += column_overhang.get(key).copied().unwrap_or(0.0);
                    let x = region_x + timed_x;
                    timed_x += COLUMN_X_STEP;
                    x
                }
                ColumnKey::End => staff_right - 0.5,
            };
            let time = match key {
                ColumnKey::Timed(t, _) => t.clone(),
                _ => TimePoint::WallClock(WallClockTime(rank as i64)),
            };
            columns.insert(
                key.clone(),
                ColumnInfo {
                    x,
                    // Every column has a candidate slot id; the slot is only
                    // *realized* (pushed to the IR) if a glyph lands in it.
                    slot: column_slot_id(region_layout_id, rank),
                    time,
                    note_column: matches!(key, ColumnKey::Timed(_, ColumnRole::Note)),
                },
            );
        }
        let column = |key: &ColumnKey| -> &ColumnInfo {
            columns
                .get(key)
                .expect("every emitted column was collected in pass 1")
        };
        let default_x = region_x + CLEF_X;

        // Beams. A group's stems all turn the way its note furthest from the
        // middle line asks (down on a tie), and end on one straight beam whose
        // rise is held to `MAX_BEAM_RISE` and is flat when an inner note is
        // more extreme than both ends; the beam lies far enough out that every
        // stem has its length (longer for three or more beams) and reaches the
        // middle line. A further beam joins each run of notes short enough for
        // it, and a lone short note takes a hook. Beamed notes take no flags.
        // Each beam rides the slots of the stems it joins (`SpanAnchor`).
        let mut beam_strokes: Vec<(Stroke, SpringSlotId, SpringSlotId)> = Vec::new();
        // Each drawn beam group's members, in event order: a tuplet whose
        // notes are exactly one of them shows its number alone.
        let mut beam_sets: BTreeSet<Vec<EventId>> = BTreeSet::new();
        for object in &region.objects {
            let (Some(staff), LayoutContent::Staff(content)) = (object.staff(), object.content())
            else {
                continue;
            };
            let yo = y_origin(staff);
            let middle = yo + STAFF_HEIGHT * 0.5;
            let head_box = metrics("noteheadBlack").map(|m| m.bounding_box());
            for (ordinal, group) in content.beams.iter().enumerate() {
                let Some((members, up)) = beam_members(group, &event_stems, middle) else {
                    continue;
                };
                let mut sorted = members.clone();
                sorted.sort();
                beam_sets.insert(sorted);
                let n = members.len();
                let segs: Vec<&StemSeg> = members.iter().map(|e| &event_stems[e][0]).collect();
                let his: Vec<f32> = segs.iter().map(|s| s.hi).collect();
                let los: Vec<f32> = segs.iter().map(|s| s.lo).collect();
                let counts: Vec<u8> = segs.iter().map(|s| s.beams).collect();
                let keys_of: Vec<ColumnKey> = segs.iter().map(|s| s.key.clone()).collect();
                let dxs: Vec<f32> = segs.iter().map(|s| s.dx).collect();
                let sign = if up { 1.0 } else { -1.0 };
                let x_off = if up {
                    head_box.map_or(NOTEHEAD_STEM_X, |b| b.right.0)
                } else {
                    head_box.map_or(0.0, |b| b.left.0)
                };
                let xs: Vec<f32> = keys_of
                    .iter()
                    .zip(&dxs)
                    .map(|(k, dx)| column(k).x + dx + x_off)
                    .collect();
                // The head each stem leaves from, nearest the beam.
                let near: Vec<f32> = if up { his.clone() } else { los.clone() };
                let most = counts.iter().copied().max().unwrap_or(1);
                let length = STEM_LENGTH + f32::from(most.saturating_sub(2)) * BEAM_STEP;
                let (x0, xn) = (xs[0], xs[n - 1]);
                let ends = [near[0], near[n - 1]];
                let inner_extreme = near[1..n - 1].iter().any(|y| {
                    if up {
                        *y > ends[0].max(ends[1])
                    } else {
                        *y < ends[0].min(ends[1])
                    }
                });
                let rise = if inner_extreme {
                    0.0
                } else {
                    (ends[1] - ends[0]).clamp(-MAX_BEAM_RISE, MAX_BEAM_RISE)
                };
                let slope = if xn > x0 { rise / (xn - x0) } else { 0.0 };
                let needs = (0..n).map(|i| {
                    let need = if up {
                        (near[i] + length).max(middle)
                    } else {
                        (near[i] - length).min(middle)
                    };
                    need - slope * (xs[i] - x0)
                });
                let intercept = if up {
                    needs.fold(f32::NEG_INFINITY, f32::max)
                } else {
                    needs.fold(f32::INFINITY, f32::min)
                };
                // The outer edge of the outermost beam.
                let edge = |x: f32| intercept + slope * (x - x0);
                for (i, e) in members.iter().enumerate() {
                    let seg = &mut event_stems.get_mut(e).expect("a member has a stem")[0];
                    seg.up = up;
                    seg.x_off = x_off;
                    seg.end = edge(xs[i]) - sign * STEM_THICKNESS;
                    seg.tip = seg.end;
                    seg.flag = None;
                    let entry =
                        column_ink
                            .entry((staff, keys_of[i].clone()))
                            .or_insert(ColumnInk {
                                top: seg.hi,
                                bottom: seg.lo,
                                stem_up: Some(up),
                                centre: x_off * 0.5,
                            });
                    entry.stem_up = Some(up);
                    if up {
                        entry.top = entry.top.max(edge(xs[i]));
                    } else {
                        entry.bottom = entry.bottom.min(edge(xs[i]));
                    }
                }
                let source = group
                    .beam
                    .map_or(TypedObjectId::Event(members[0]), TypedObjectId::Beam);
                let dependencies: Vec<TypedObjectId> =
                    members.iter().copied().map(TypedObjectId::Event).collect();
                let slot = |i: usize| column(&keys_of[i]).slot;
                for level in 1..=most {
                    // The centre line of this level's beam.
                    let centre = |x: f32| {
                        edge(x) - sign * (BEAM_THICKNESS / 2.0 + f32::from(level - 1) * BEAM_STEP)
                    };
                    let mut runs: Vec<(usize, usize)> = Vec::new();
                    for (i, &count) in counts.iter().enumerate() {
                        if count < level {
                            continue;
                        }
                        match runs.last_mut() {
                            Some((_, end)) if *end + 1 == i => *end = i,
                            _ => runs.push((i, i)),
                        }
                    }
                    for (run, &(a, b)) in runs.iter().enumerate() {
                        let (from_x, to_x, start, end) = if a < b {
                            (
                                xs[a] - STEM_THICKNESS / 2.0,
                                xs[b] + STEM_THICKNESS / 2.0,
                                slot(a),
                                slot(b),
                            )
                        } else if a + 1 == n {
                            // A lone short note last in its group hooks back.
                            (
                                xs[a] - BEAM_HOOK,
                                xs[a] + STEM_THICKNESS / 2.0,
                                slot(a),
                                slot(a),
                            )
                        } else {
                            (
                                xs[a] - STEM_THICKNESS / 2.0,
                                xs[a] + BEAM_HOOK,
                                slot(a),
                                slot(a),
                            )
                        };
                        let provenance = Provenance::synthesized(
                            source,
                            SynthesisKind::Registered(BEAM_SYNTHESIS),
                            SynthesisInstanceKey(
                                (ordinal as u128) << 32 | u128::from(level) << 16 | run as u128,
                            ),
                            dependencies.clone(),
                        );
                        beam_strokes.push((
                            line_stroke(
                                provenance,
                                Point::new(from_x, centre(from_x)),
                                Point::new(to_x, centre(to_x)),
                                BEAM_THICKNESS,
                                band_of(Some(staff)),
                            ),
                            start,
                            end,
                        ));
                    }
                }
            }
        }

        // (provenance, owning staff, engraving content) for the region object,
        // then its contents, then this region's spanning cross-region objects.
        let specs: Vec<(&Provenance, Option<StaffId>, Option<&LayoutContent>)> =
            std::iter::once((&region.provenance, None, None))
                .chain(
                    region
                        .objects
                        .iter()
                        .map(|o| (o.provenance(), o.staff(), Some(o.content()))),
                )
                .chain(
                    logical
                        .cross_region
                        .iter()
                        .filter(|object| object.regions.first() == Some(&region_id))
                        .map(|object| (&object.provenance, object.staff, None)),
                )
                .collect();

        // Where this region's glyphs begin in the global vector, so constraint
        // emission below can see exactly the glyphs pass 2 produced for it.
        let region_glyph_start = glyphs.len();
        let mut emit = Emit {
            glyphs: &mut glyphs,
            strokes: &mut strokes,
            curves: &mut curves,
            diagnostics: &mut diagnostics,
            column_members: BTreeMap::new(),
            region_glyphs: Vec::new(),
            staff_members: BTreeMap::new(),
            margin_members: Vec::new(),
            staves_in_order: Vec::new(),
        };

        // Regroup the note-column ink by staff: a slur's obstacle field is its own
        // staff's columns, never the sibling staff that shares their x.
        let mut staff_notes: BTreeMap<StaffId, BTreeMap<ColumnKey, ColumnInk>> = BTreeMap::new();
        for ((staff, key), column) in column_ink {
            staff_notes.entry(staff).or_default().insert(key, column);
        }
        let no_notes: BTreeMap<ColumnKey, ColumnInk> = BTreeMap::new();

        // Each event's heads by component, highest first: a tie takes its side
        // from its head's place among them.
        let mut chord_ys: BTreeMap<(EventId, usize), Vec<f32>> = BTreeMap::new();
        for head in pitch_heads
            .values()
            .chain(unpitched_heads.values())
            .flatten()
        {
            chord_ys
                .entry((head.event, head.comp))
                .or_default()
                .push(head.y);
        }
        for ys in chord_ys.values_mut() {
            ys.sort_by(|a, b| b.total_cmp(a));
        }
        // Each staff column's ink a tie's end must stand clear of.
        let mut tie_ink: BTreeMap<(Option<StaffId>, ColumnKey), Vec<InkBox>> = BTreeMap::new();
        for ((staff, key), refs) in &column_heads {
            let Some(info) = columns.get(key) else {
                continue;
            };
            let yo = staff.map(&y_origin).unwrap_or(0.0);
            let boxes = tie_ink.entry((*staff, key.clone())).or_default();
            let mut stems = BTreeSet::new();
            for r in refs {
                let head = r.get(&pitch_heads, &unpitched_heads);
                boxes.extend(column_ink_boxes(head, info.x, yo));
                if stems.insert((head.event, head.comp)) {
                    let seg = event_stems
                        .get(&head.event)
                        .and_then(|segs| segs.iter().find(|seg| seg.comp == head.comp))
                        .filter(|seg| seg.drawn);
                    if let Some(seg) = seg {
                        let x = info.x + seg.dx + seg.x_off;
                        let base = if seg.up { seg.lo } else { seg.hi };
                        boxes.push(InkBox {
                            left: x - STEM_THICKNESS / 2.0,
                            right: x + STEM_THICKNESS / 2.0,
                            bottom: base.min(seg.end),
                            top: base.max(seg.end),
                            behind: false,
                        });
                    }
                }
            }
        }
        let ties = TieSides {
            voices: &event_voices,
            chords: &chord_ys,
            stems: &event_stems,
            ink: &tie_ink,
        };

        // Pass 2 — emit. Each logical object's exact provenance lands on exactly
        // one primitive; the extras a multi-component object owns are synthesized.
        for (provenance, staff, content) in specs {
            let yo = staff.map(&y_origin).unwrap_or(0.0);
            match provenance.source {
                TypedObjectId::Staff(s) => {
                    // Five staff lines: the bottom line is the staff's own anchor;
                    // the four above are synthesized from it (distinct stable ids
                    // keyed on the manifestation and line index).
                    let manifestation =
                        manifestation_layout_id(&TypedObjectId::Staff(s), region_id);
                    for line in 0..5u32 {
                        let y = yo + line as f32;
                        let provenance = if line == 0 {
                            provenance.clone()
                        } else {
                            Provenance::synthesized(
                                TypedObjectId::Staff(s),
                                SynthesisKind::Registered(STAFF_LINE_SYNTHESIS),
                                staff_line_key(manifestation, line),
                                Vec::new(),
                            )
                        };
                        emit.stroke(line_stroke(
                            provenance,
                            Point::new(staff_left, y),
                            Point::new(staff_right, y),
                            STAFF_LINE_THICKNESS,
                            band_of(staff),
                        ));
                    }
                }
                TypedObjectId::StaffInstance(_) => {
                    // The displayed clef and key are those in force at the staff
                    // start, by time — the same query the notes use, so they
                    // always agree. The clef carries the instance's provenance;
                    // each key accidental is synthesized from it.
                    let default_content;
                    let content = match content {
                        Some(LayoutContent::Staff(c)) => c,
                        _ => {
                            default_content = StaffContent {
                                clefs: Vec::new(),
                                keys: Vec::new(),
                                default_clef: Clef::default(),
                                beams: Vec::new(),
                            };
                            &default_content
                        }
                    };
                    let clef = active_clef_or(&content.clefs, &origin(), content.default_clef);
                    let glyphs = lead_glyphs(content, &origin(), yo);
                    if glyphs.is_empty() {
                        emit.diag(provenance.source, unbundled(clef_label(clef.shape)));
                        emit.stroke(anchor(
                            provenance,
                            Point::new(default_x, yo),
                            band_of(staff),
                        ));
                    }
                    let info = column(&ColumnKey::Lead);
                    for (i, (name, x, y)) in glyphs.into_iter().enumerate() {
                        let owned;
                        let glyph_provenance = if i == 0 {
                            provenance
                        } else {
                            owned = Provenance::synthesized(
                                provenance.source,
                                SynthesisKind::Registered(KEY_SIG_SYNTHESIS),
                                SynthesisInstanceKey(i as u128 - 1),
                                provenance.dependencies.clone(),
                            );
                            &owned
                        };
                        emit.glyph(
                            glyph_provenance,
                            name,
                            Point::new(info.x + x, y),
                            band_of(staff),
                            staff,
                            info.slot,
                        );
                    }
                    // Each clef change, in its column.
                    for (c, (time, clef)) in drawn_clef_changes(content).into_iter().enumerate() {
                        let info = column(&ColumnKey::Timed(time, ColumnRole::Clef));
                        for (g, (name, x, y)) in
                            clef_change_glyphs(&clef, yo).into_iter().enumerate()
                        {
                            let glyph_provenance = Provenance::synthesized(
                                provenance.source,
                                SynthesisKind::Registered(CLEF_CHANGE_SYNTHESIS),
                                SynthesisInstanceKey((c as u128) << 8 | g as u128),
                                provenance.dependencies.clone(),
                            );
                            emit.glyph(
                                &glyph_provenance,
                                name,
                                Point::new(info.x + x, y),
                                band_of(staff),
                                staff,
                                info.slot,
                            );
                        }
                    }
                }
                TypedObjectId::Event(eid) => match content {
                    Some(LayoutContent::Note(_)) | Some(LayoutContent::Unpitched(_)) => {
                        let unpitched = matches!(content, Some(LayoutContent::Unpitched(_)));
                        let segs = event_stems.get(&eid).map(Vec::as_slice).unwrap_or(&[]);
                        if segs.is_empty() {
                            // A pitch-less, component-less note still needs its anchor.
                            emit.stroke(anchor(
                                provenance,
                                Point::new(default_x, yo),
                                band_of(staff),
                            ));
                        }
                        // An unpitched note has no pitches to carry its heads, so
                        // its first head carries the event's exact provenance and
                        // its stems are synthesized; a pitched note's first stem
                        // carries it.
                        if unpitched {
                            let heads = unpitched_heads.get(&eid).map(Vec::as_slice).unwrap_or(&[]);
                            for head in heads {
                                let head_provenance = component_provenance(provenance, head.comp);
                                emit_head(
                                    &mut emit,
                                    &head_provenance,
                                    head,
                                    column(&head.key),
                                    yo,
                                    band_of(staff),
                                    staff,
                                );
                            }
                            for (curve, start, end) in component_ties(
                                provenance,
                                heads,
                                &ties,
                                &columns,
                                (staff, yo),
                                band_of(staff),
                            ) {
                                span_anchors.push(SpanAnchor {
                                    primitive: curve.id(),
                                    start,
                                    end,
                                });
                                emit.curve(curve);
                            }
                        }
                        for seg in segs {
                            let info = column(&seg.key);
                            let stem_x = info.x + seg.dx + seg.x_off;
                            let (from, to) = if seg.drawn {
                                // The stem runs from the head it attaches to — the
                                // lowest for an up-stem, the highest for a down one —
                                // out to its end.
                                let base = if seg.up { seg.lo } else { seg.hi };
                                (Point::new(stem_x, base), Point::new(stem_x, seg.end))
                            } else {
                                // A stemless value (whole note): a zero-length stem.
                                let x = info.x + seg.dx;
                                (Point::new(x, seg.lo), Point::new(x, seg.lo))
                            };
                            let prov = if unpitched {
                                Provenance::synthesized(
                                    provenance.source,
                                    SynthesisKind::Registered(STEM_SYNTHESIS),
                                    SynthesisInstanceKey(seg.comp as u128),
                                    provenance.dependencies.clone(),
                                )
                            } else {
                                component_provenance(provenance, seg.comp)
                            };
                            emit.stroke(Stroke {
                                provenance: prov,
                                from,
                                to,
                                thickness: StaffSpace(STEM_THICKNESS),
                                layer: 0,
                                style: ink(),
                                vertical_band: band_of(staff),
                            });
                            // The flag hangs from the stem's normal tip, its left
                            // edge on the stem's.
                            if let Some(flag) = seg.flag {
                                let flag_provenance = Provenance::synthesized(
                                    provenance.source,
                                    SynthesisKind::Registered(FLAG_SYNTHESIS),
                                    SynthesisInstanceKey(seg.comp as u128),
                                    provenance.dependencies.clone(),
                                );
                                emit.glyph(
                                    &flag_provenance,
                                    flag,
                                    Point::new(stem_x - STEM_THICKNESS / 2.0, seg.tip),
                                    band_of(staff),
                                    staff,
                                    info.slot,
                                );
                            }
                        }
                    }
                    Some(LayoutContent::Rest(_)) => {
                        let segs = event_rests.get(&eid).map(Vec::as_slice).unwrap_or(&[]);
                        if segs.is_empty() {
                            emit.stroke(anchor(
                                provenance,
                                Point::new(default_x, yo),
                                band_of(staff),
                            ));
                        }
                        for seg in segs {
                            let info = column(&seg.key);
                            let owned;
                            let prov_ref = if seg.comp == 0 {
                                provenance
                            } else {
                                owned = component_provenance(provenance, seg.comp);
                                &owned
                            };
                            match seg.name {
                                // A hidden rest keeps its place as a traced anchor.
                                _ if !seg.visible => emit.stroke(anchor(
                                    prov_ref,
                                    Point::new(info.x, seg.y),
                                    band_of(staff),
                                )),
                                Some(name) => {
                                    emit.glyph(
                                        prov_ref,
                                        name,
                                        Point::new(info.x, seg.y),
                                        band_of(staff),
                                        staff,
                                        info.slot,
                                    );
                                    // Dots sit in the space above the middle line,
                                    // right of the rest.
                                    let right =
                                        metrics(name).map_or(1.0, |m| m.bounding_box().right.0);
                                    for dot in 0..seg.dots {
                                        let dot_provenance = Provenance::synthesized(
                                            provenance.source,
                                            SynthesisKind::Registered(DOT_SYNTHESIS),
                                            SynthesisInstanceKey(
                                                (seg.comp as u128) << 8 | u128::from(dot),
                                            ),
                                            provenance.dependencies.clone(),
                                        );
                                        emit.glyph(
                                            &dot_provenance,
                                            "augmentationDot",
                                            Point::new(
                                                info.x
                                                    + right
                                                    + DOT_GAP
                                                    + f32::from(dot) * DOT_STEP,
                                                yo + STAFF_HEIGHT / 2.0 + 0.5,
                                            ),
                                            band_of(staff),
                                            staff,
                                            info.slot,
                                        );
                                    }
                                }
                                // No bundled glyph for this value: a traced
                                // anchor at the rest's *own onset column* (not a
                                // default x), with the gap surfaced. The component
                                // keeps its place; later components do not vanish.
                                None => {
                                    emit.diag(prov_ref.source, unbundled(rest_label()));
                                    emit.stroke(anchor(
                                        prov_ref,
                                        Point::new(info.x, seg.y),
                                        band_of(staff),
                                    ));
                                }
                            }
                        }
                    }
                    // A non-pitched, non-rest event (trajectory / cue / …): not
                    // engraved in this tier; a traced anchor keeps it.
                    _ => emit.stroke(anchor(
                        provenance,
                        Point::new(default_x, yo),
                        band_of(staff),
                    )),
                },
                TypedObjectId::Pitch(pid) => match pitch_heads.get(&pid) {
                    Some(heads) => {
                        for head in heads {
                            let head_provenance = component_provenance(provenance, head.comp);
                            emit_head(
                                &mut emit,
                                &head_provenance,
                                head,
                                column(&head.key),
                                yo,
                                band_of(staff),
                                staff,
                            );
                        }
                        for (curve, start, end) in component_ties(
                            provenance,
                            heads,
                            &ties,
                            &columns,
                            (staff, yo),
                            band_of(staff),
                        ) {
                            span_anchors.push(SpanAnchor {
                                primitive: curve.id(),
                                start,
                                end,
                            });
                            emit.curve(curve);
                        }
                    }
                    None => {
                        // An unmatched pitch (no event content reached it): a black
                        // notehead on the clef reference line, with the gap surfaced.
                        emit.diag(provenance.source, LayoutDiagnosticKind::MissingSpelling);
                        let clef = active_clef_or(clef_seq(staff), &origin(), clef_default(staff));
                        emit.stroke(anchor(
                            provenance,
                            Point::new(default_x, step_to_y(yo, reference_step(&clef))),
                            band_of(staff),
                        ));
                    }
                },
                TypedObjectId::Measure(_) => {
                    let measure = match content {
                        Some(LayoutContent::Measure(measure)) => Some(measure),
                        _ => None,
                    };
                    let key = measure.map_or(ColumnKey::End, measure_column);
                    // A barline ends its measure. A repeat boundary on its
                    // column morphs it into the composite repeat sign — the
                    // sign *replaces* the plain barline, keeping the measure's
                    // exact provenance verbatim (the round-trip provenance
                    // floor compares it exactly; repeat-edit invalidation is
                    // carried by the score version, which any edit changes).
                    // The final barline never morphs — an end repeat there
                    // draws its dot pair beside it instead (emitted with the
                    // standalone signs below), so the casting-off solver's
                    // final-barline classification stays truthful. A region
                    // that closes on a single barline (its staff continuing in
                    // a later region) morphs only for an end repeat, since a
                    // start there opens nothing in this region.
                    let closes_final = key == ColumnKey::End
                        && measure.is_none_or(|m| m.barline == BarlineKind::Final);
                    let name = if closes_final {
                        "barlineFinal"
                    } else if key == ColumnKey::End {
                        match marks.get(&key) {
                            Some(mark) if mark.end => "repeatRight",
                            _ => "barlineSingle",
                        }
                    } else {
                        match marks.get(&key) {
                            Some(mark) => repeat_sign_name(mark.start, mark.end),
                            None => "barlineSingle",
                        }
                    };
                    let info = column(&key);
                    // The barline glyph's origin is its lower end — Bravura barlines
                    // run 0..4 staff spaces *up* from the origin — so anchoring it at
                    // the staff bottom (`yo`) makes it connect the bottom and top
                    // staff lines rather than float above the midline.
                    let baseline = Point::new(repeat_sign_x(name, info.x), yo);
                    emit.glyph(provenance, name, baseline, band_of(staff), staff, info.slot);
                    // The time signature this measure introduces: numerator over
                    // denominator in its own column at the measure's start, after
                    // the barline ending the measure before, each digit a
                    // synthesized glyph sharing that column's slot. An unbundled
                    // digit is surfaced (the bundled metrics carry only a
                    // representative subset).
                    if let Some(measure) = measure {
                        if let Some(time_signature) = measure.time_signature {
                            let info = column(&signature_column(measure));
                            let center_x = info.x + TIME_SIG_X;
                            // The digit glyphs are centred on their baseline, so the
                            // numerator's baseline sits on the upper half of the
                            // staff (≈ y 3) and the denominator's on the lower (≈ y 1).
                            let lines = [
                                (0u8, time_signature.numerator, yo + 3.0),
                                (1u8, time_signature.denominator, yo + 1.0),
                            ];
                            for (role, value, baseline_y) in lines {
                                let digits = digits_of(u32::from(value));
                                let count = digits.len() as f32;
                                for (i, digit) in digits.iter().enumerate() {
                                    let x =
                                        center_x + (i as f32 - (count - 1.0) / 2.0) * TIME_DIGIT_X;
                                    let digit_provenance = Provenance::synthesized(
                                        provenance.source,
                                        SynthesisKind::Registered(TIME_SIG_SYNTHESIS),
                                        SynthesisInstanceKey((role as u128) << 8 | i as u128),
                                        provenance.dependencies.clone(),
                                    );
                                    emit.glyph_if_bundled(
                                        &digit_provenance,
                                        time_digit(*digit),
                                        Point::new(x, baseline_y),
                                        band_of(staff),
                                        staff,
                                        info.slot,
                                    );
                                }
                            }
                        }
                    }
                }
                TypedObjectId::RepeatStructure(_) => {
                    // The structure's exact provenance rides its traced anchor
                    // (uniform with every other cross-cutting structure); all
                    // of its ink — the standalone signs below and the volta
                    // brackets here — is synthesized from it.
                    emit.stroke(anchor(
                        provenance,
                        Point::new(default_x, yo),
                        band_of(staff),
                    ));
                    let Some(LayoutContent::Repeat(repeat)) = content else {
                        continue;
                    };
                    // Volta brackets sit above the region's top staff: a
                    // horizontal line with a descending hook at each end and
                    // the ending numbers (time-signature digit glyphs — the
                    // Minimal tier has no text primitive) under its left end.
                    let top_staff = staff_order.first().copied();
                    let yo_top = top_staff.map(&y_origin).unwrap_or(0.0);
                    for (index, volta) in repeat.voltas.iter().enumerate() {
                        let Some(start) = placement_column(&volta.start, &columns) else {
                            continue;
                        };
                        let Some(end) = placement_column(&volta.end, &columns) else {
                            continue;
                        };
                        if end.x <= start.x {
                            // A reversed or zero-width span draws no bracket
                            // (advisory volta well-formedness is the authoring
                            // layer's jurisdiction, not the engraver's).
                            continue;
                        }
                        let y = yo_top + VOLTA_Y;
                        let volta_provenance = |element: u128| {
                            Provenance::synthesized(
                                provenance.source,
                                SynthesisKind::Registered(VOLTA_SYNTHESIS),
                                SynthesisInstanceKey(((index as u128) << 64) | element),
                                provenance.dependencies.clone(),
                            )
                        };
                        emit.stroke(line_stroke(
                            volta_provenance(0),
                            Point::new(start.x, y),
                            Point::new(end.x, y),
                            VOLTA_LINE_THICKNESS,
                            band_of(staff),
                        ));
                        for (element, x) in [(1u128, start.x), (2u128, end.x)] {
                            emit.stroke(line_stroke(
                                volta_provenance(element),
                                Point::new(x, y),
                                Point::new(x, y - VOLTA_HOOK),
                                VOLTA_LINE_THICKNESS,
                                band_of(staff),
                            ));
                        }
                        let mut cursor = start.x + VOLTA_TEXT_X;
                        let mut element = 3u128;
                        for ending in &volta.endings {
                            for digit in digits_of(*ending) {
                                emit.glyph(
                                    &volta_provenance(element),
                                    time_digit(digit),
                                    Point::new(cursor, y - VOLTA_TEXT_DROP),
                                    band_of(top_staff),
                                    top_staff,
                                    start.slot,
                                );
                                cursor += TIME_DIGIT_X;
                                element += 1;
                            }
                            cursor += VOLTA_ENDING_GAP;
                        }
                    }
                }
                TypedObjectId::StaffGroup(_) => {
                    // A staff group draws where a solver breaks the region into
                    // systems; here its exact provenance rides a traced anchor.
                    emit.stroke(anchor(provenance, Point::new(default_x, yo), band_of(None)));
                    if let Some(LayoutContent::Group(group)) = content {
                        use epiphany_core::StaffGroupKind;
                        let sign = match group.kind {
                            StaffGroupKind::GrandStaff => Some(GroupSign::Brace),
                            StaffGroupKind::Bracket | StaffGroupKind::Choral => {
                                Some(GroupSign::Bracket)
                            }
                            StaffGroupKind::SubBracket => Some(GroupSign::SubBracket),
                            StaffGroupKind::Registered(_) => None,
                        };
                        if let Some(kind) = sign {
                            staff_groups.push(GroupSpan {
                                region: region_index,
                                kind,
                                staves: group.staves.clone(),
                                joined: group.kind != StaffGroupKind::Choral,
                                provenance: provenance.clone(),
                            });
                        }
                    }
                }
                TypedObjectId::Tuplet(_) => {
                    // A tuplet's number, and its bracket unless its notes are
                    // one beam group, beside its members on its voice's side.
                    let marks = match (content, staff) {
                        (Some(LayoutContent::Tuplet(tuplet)), Some(st)) => tuplet_marks(
                            tuplet,
                            &TupletInk {
                                columns: &columns,
                                stems: &event_stems,
                                rests: &event_rests,
                                ink: staff_notes.get(&st).unwrap_or(&no_notes),
                                voices: &event_voices,
                                beams: &beam_sets,
                            },
                        ),
                        _ => None,
                    };
                    match marks {
                        Some(marks) => {
                            for (k, (name, at)) in marks.digits.into_iter().enumerate() {
                                let digit_provenance = if k == 0 {
                                    provenance.clone()
                                } else {
                                    Provenance::synthesized(
                                        provenance.source,
                                        SynthesisKind::Registered(TUPLET_SYNTHESIS),
                                        SynthesisInstanceKey(k as u128),
                                        provenance.dependencies.clone(),
                                    )
                                };
                                emit.glyph(
                                    &digit_provenance,
                                    name,
                                    at,
                                    band_of(staff),
                                    staff,
                                    marks.slot,
                                );
                            }
                            for (k, (from, to, start, end)) in marks.bracket.into_iter().enumerate()
                            {
                                let stroke = line_stroke(
                                    Provenance::synthesized(
                                        provenance.source,
                                        SynthesisKind::Registered(TUPLET_SYNTHESIS),
                                        SynthesisInstanceKey(1 << 16 | k as u128),
                                        provenance.dependencies.clone(),
                                    ),
                                    from,
                                    to,
                                    TUPLET_BRACKET_THICKNESS,
                                    band_of(staff),
                                );
                                span_anchors.push(SpanAnchor {
                                    primitive: stroke.id(),
                                    start,
                                    end,
                                });
                                emit.stroke(stroke);
                            }
                        }
                        None => emit.stroke(anchor(
                            provenance,
                            Point::new(default_x, yo),
                            band_of(staff),
                        )),
                    }
                }
                TypedObjectId::Tie(_) => {
                    // A tie arcs from each start head to the head it continues
                    // into, riding both heads' slots: beside another voice an
                    // upper voice's ties arc above and a lower voice's below;
                    // alone, in a chord the upper ties arc above and the lower
                    // below, a middle or lone tie away from its stem (by its
                    // staff position when it has none). The structure's exact provenance rides its first
                    // arc; a tie with no head to join keeps a traced anchor.
                    let mut joins: Vec<(&Head, &Head)> = Vec::new();
                    let mut start_event = None;
                    if let Some(LayoutContent::Tie(tie)) = content {
                        start_event = Some(tie.start);
                        if tie.pairs.is_empty() {
                            let a = unpitched_heads.get(&tie.start).and_then(|h| h.last());
                            let b = unpitched_heads.get(&tie.end).and_then(|h| h.first());
                            joins.extend(a.zip(b));
                        }
                        for (a, b) in &tie.pairs {
                            let a = pitch_heads.get(a).and_then(|h| h.last());
                            let b = pitch_heads.get(b).and_then(|h| h.first());
                            joins.extend(a.zip(b));
                        }
                    }
                    joins.sort_by(|x, y| y.0.y.total_cmp(&x.0.y));
                    let stem_up = start_event
                        .and_then(|e| event_stems.get(&e))
                        .and_then(|segs| segs.last())
                        .filter(|seg| seg.drawn)
                        .map(|seg| seg.up);
                    let voice = start_event.and_then(|e| event_voices.get(&e)).copied();
                    let n = joins.len();
                    for (i, (a, b)) in joins.iter().enumerate() {
                        let above = tie_above(voice, i, n, stem_up, a.y, yo);
                        let tie_provenance = if i == 0 {
                            provenance.clone()
                        } else {
                            Provenance::synthesized(
                                provenance.source,
                                SynthesisKind::Registered(TIE_SYNTHESIS),
                                SynthesisInstanceKey(i as u128),
                                provenance.dependencies.clone(),
                            )
                        };
                        let (from, to) = (column(&a.key), column(&b.key));
                        let ink = |key: &ColumnKey| {
                            tie_ink
                                .get(&(staff, key.clone()))
                                .map_or(&[][..], Vec::as_slice)
                        };
                        let curve = tie_curve(
                            tie_provenance,
                            a,
                            from,
                            b,
                            to,
                            above,
                            band_of(staff),
                            (ink(&a.key), ink(&b.key)),
                        );
                        span_anchors.push(SpanAnchor {
                            primitive: curve.id(),
                            start: from.slot,
                            end: to.slot,
                        });
                        emit.curve(curve);
                    }
                    if n == 0 {
                        emit.stroke(anchor(
                            provenance,
                            Point::new(default_x, yo),
                            band_of(staff),
                        ));
                    }
                }
                TypedObjectId::Slur(_) => {
                    // A slur engraves to a cubic-bézier curve arcing between its
                    // two endpoint columns. No curve is honest — the traced
                    // anchor keeps provenance instead — when: the slur resolved
                    // to no single staff (endpoints on different staves; the
                    // arc would float at `yo = 0` detached from a note on
                    // another staff — a Minimal boundary, cross-staff slurs
                    // defer to a later tranche), either endpoint is unresolved
                    // (dangling event, or an endpoint in another region), or the
                    // span is not left-to-right in this region.
                    let curve = match content {
                        Some(LayoutContent::Slur(slur)) if staff.is_some() => slur_curve(
                            provenance,
                            slur,
                            yo,
                            &columns,
                            band_of(staff),
                            staff
                                .and_then(|st| staff_notes.get(&st))
                                .unwrap_or(&no_notes),
                        ),
                        _ => None,
                    };
                    match curve {
                        Some(curve) => emit.curve(curve),
                        None => emit.stroke(anchor(
                            provenance,
                            Point::new(default_x, yo),
                            band_of(staff),
                        )),
                    }
                }
                // Region, Voice, GraphicObject, and every other cross-cutting
                // structure (ties, beams, tuplets, spanners, markers, …) have no
                // Minimal-tier glyph; a zero-extent traced anchor keeps them.
                _ => emit.stroke(anchor(
                    provenance,
                    Point::new(default_x, yo),
                    band_of(staff),
                )),
            }
        }

        for (stroke, start, end) in beam_strokes {
            span_anchors.push(SpanAnchor {
                primitive: stroke.id(),
                start,
                end,
            });
            emit.stroke(stroke);
        }

        // The repeat signs the measures could not carry: a composite sign at a
        // column with no measure barline on that staff (a mid-measure boundary,
        // a region edge without a final barline), and the dot pair beside a
        // final barline an end repeat closes on. One sign per (column, staff),
        // synthesized from the mark's owner under its semantic
        // `(boundary site, staff)` instance key, depending on every structure
        // that shares the mark.
        for (key, info) in columns.iter() {
            let Some(mark) = marks.get(key) else {
                continue;
            };
            // At the region-closing column only an END repeat has ink — the
            // dot pair beside a final barline, or the full sign where no
            // final barline stands on that staff. A START boundary there
            // draws nothing on any staff: a sign after the region close
            // would misstate the structure.
            if *key == ColumnKey::End && !mark.end {
                continue;
            }
            let (owner, site) = mark
                .owner
                .expect("a repeat mark records at least one owning boundary");
            let deps: Vec<TypedObjectId> = mark
                .sources
                .iter()
                .map(|id| TypedObjectId::RepeatStructure(*id))
                .collect();
            for (staff_index, staff) in staff_order.iter().enumerate() {
                let covered = measure_cols.contains(&(key.clone(), *staff));
                let (name, x) = if *key == ColumnKey::End {
                    if covered && !final_staves.contains(staff) {
                        // The region-closing single barline morphed into the
                        // end sign.
                        continue;
                    }
                    if covered {
                        let dots_width = metrics("repeatDots")
                            .expect("repeatDots metrics are bundled")
                            .bounding_box()
                            .right
                            .0;
                        ("repeatDots", info.x - REPEAT_DOTS_SEPARATION - dots_width)
                    } else {
                        // No final barline on this staff (its run continues in
                        // a later region): the full end sign, never a
                        // start-facing one.
                        ("repeatRight", repeat_sign_x("repeatRight", info.x))
                    }
                } else {
                    if covered {
                        // The measure's own barline morphed into the sign.
                        continue;
                    }
                    let name = repeat_sign_name(mark.start, mark.end);
                    (name, repeat_sign_x(name, info.x))
                };
                let provenance = Provenance::synthesized(
                    TypedObjectId::RepeatStructure(owner),
                    SynthesisKind::Registered(REPEAT_BARLINE_SYNTHESIS),
                    SynthesisInstanceKey(((site as u128) << 32) | staff_index as u128),
                    deps.clone(),
                );
                emit.glyph(
                    &provenance,
                    name,
                    Point::new(x, y_origin(*staff)),
                    band_of(Some(*staff)),
                    Some(*staff),
                    info.slot,
                );
            }
        }

        let Emit {
            column_members,
            region_glyphs,
            mut staff_members,
            margin_members,
            staves_in_order,
            ..
        } = emit;

        // One spring slot per glyph-bearing column, in column order (so the solver
        // accumulates a monotonic x), members = the column's glyphs. Stroke-only
        // columns have no slot. The time axis maps each musical *note* column with
        // a slot to it (barline/lead/end columns are visual, not musical query
        // points, so they are omitted from it).
        let mut region_placements = Vec::new();
        for (key, info) in &columns {
            let members = column_members.get(&info.slot).cloned().unwrap_or_default();
            // Realize a slot only if a glyph occupies the column — never an empty
            // slot (which would have a spacing target but no glyph the engraver
            // could derive a source x from).
            if members.is_empty() {
                continue;
            }
            // The spring slot's natural width is uniform, but for the lead's and a
            // time signature's, which reserve their ink and the gap after it;
            // the engraver computes the collision-aware advance (per-slot
            // bearings) when it re-spaces, measuring a slot's width from its
            // first glyph's baseline, and the *source* geometry below already
            // separates columns enough that accidentals do not overlap the
            // previous note.
            let reserve = |gap: f32| {
                let ink: Vec<&GlyphObject> = glyphs
                    .iter()
                    .filter(|g| g.horizontal_slot == info.slot)
                    .collect();
                ink.first().map_or(0.0, |first| {
                    ink.iter()
                        .map(|g| g.baseline.x.0 + g.bounding_box.right.0)
                        .fold(f32::NEG_INFINITY, f32::max)
                        - first.baseline.x.0
                        + gap
                })
            };
            let preferred = match key {
                ColumnKey::Lead => reserve(LEAD_GAP),
                ColumnKey::Timed(_, ColumnRole::Signature) => reserve(SIGNATURE_GAP),
                ColumnKey::Timed(_, ColumnRole::Clef) => reserve(CLEF_CHANGE_GAP),
                _ => 0.0,
            }
            .max(COLUMN_PREFERRED_WIDTH);
            horizontal_slots.push(SpringSlot {
                id: info.slot,
                time: info.time.clone(),
                min_width: StaffSpace(1.0),
                preferred_width: StaffSpace(preferred),
                max_width: None,
                stretch_factor: 1.0,
                compress_factor: 1.0,
                members,
            });
            if info.note_column {
                region_placements.push(SlotPlacement {
                    time: info.time.clone(),
                    slot: info.slot,
                });
            }
        }

        // --- Constraint emission (Chapter 7 §"Pipeline Overview": the spacing
        // pass "build[s] collision constraints"). Everything emitted here is
        // satisfiable on well-formed input by construction — the source layout
        // separates columns collision-free and a conformant re-spacing keeps
        // them so — and the order is deterministic: per region, no-collision
        // pairs (staff emission order, then column x / glyph id), containment
        // (glyph stable-id order), then projected breaks (override order).
        let region_glyph_objects = &glyphs[region_glyph_start..];
        let glyph_by_id: BTreeMap<GlyphObjectId, &GlyphObject> = region_glyph_objects
            .iter()
            .map(|glyph| (glyph.id(), glyph))
            .collect();

        // NoCollision between *successive notehead columns* within each staff:
        // adjacent pairs in (column x, id) order, one linear chain per staff,
        // not O(n²); and, within a column, between every two heads whose boxes
        // share height (a second or a unison, which the column's placement set
        // apart), except a unison two voices share, one head drawn twice.
        for staff in &staves_in_order {
            let mut heads: Vec<&GlyphObject> = staff_members
                .get(staff)
                .into_iter()
                .flatten()
                .filter_map(|id| glyph_by_id.get(id).copied())
                .filter(|glyph| glyph.glyph.as_str().starts_with("notehead"))
                .collect();
            heads.sort_by(|a, b| {
                a.baseline
                    .x
                    .0
                    .total_cmp(&b.baseline.x.0)
                    .then_with(|| a.id().cmp(&b.id()))
            });
            for pair in heads.windows(2) {
                if pair[0].horizontal_slot != pair[1].horizontal_slot {
                    constraints.push(LayoutConstraint::NoCollision {
                        a: pair[0].id(),
                        b: pair[1].id(),
                    });
                }
            }
            let mut columns: BTreeMap<SpringSlotId, Vec<&GlyphObject>> = BTreeMap::new();
            for head in &heads {
                columns.entry(head.horizontal_slot).or_default().push(head);
            }
            for column in columns.values() {
                for (i, a) in column.iter().enumerate() {
                    for b in &column[i + 1..] {
                        let level = a.baseline.y.0 + a.bounding_box.bottom.0
                            < b.baseline.y.0 + b.bounding_box.top.0
                            && b.baseline.y.0 + b.bounding_box.bottom.0
                                < a.baseline.y.0 + a.bounding_box.top.0;
                        let shared = a.glyph == b.glyph && a.baseline == b.baseline;
                        if level && !shared {
                            constraints.push(LayoutConstraint::NoCollision {
                                a: a.id(),
                                b: b.id(),
                            });
                        }
                    }
                }
            }
        }

        // PositionWithin: every glyph must stay inside its owning region's
        // envelope. The vertical extent is the exact envelope of the region's
        // own glyph boxes (both v0 solvers preserve glyph `y` verbatim, so this
        // is a real obligation a vertical pass must renegotiate); the
        // horizontal span is the open v0 canvas (see
        // [`POSITION_WITHIN_X_REACH`]).
        if !region_glyph_objects.is_empty() {
            let mut bottom = f32::INFINITY;
            let mut top = f32::NEG_INFINITY;
            for glyph in region_glyph_objects {
                bottom = bottom.min(glyph.baseline.y.0 + glyph.bounding_box.bottom.0);
                top = top.max(glyph.baseline.y.0 + glyph.bounding_box.top.0);
            }
            let envelope = Rect {
                origin: Point::new(-POSITION_WITHIN_X_REACH, bottom),
                size: Size2D {
                    width: StaffSpace(2.0 * POSITION_WITHIN_X_REACH),
                    height: StaffSpace(top - bottom),
                },
            };
            let mut ids: Vec<GlyphObjectId> = glyph_by_id.keys().copied().collect();
            ids.sort();
            for glyph in ids {
                constraints.push(LayoutConstraint::PositionWithin {
                    glyph,
                    region: envelope,
                });
            }
        }

        // Projected break overrides (the logical stage's `SystemBreak` /
        // `PageBreak` engraving overrides, Chapter 7 §"Engraving Overrides")
        // become break constraints on the spring slot that opens the system at
        // the break anchor's onset — the signature column at that time when one
        // carries ink, else the note column; the barline before them ends the
        // previous system. An anchor
        // no realized column represents — an event or measure outside this
        // region, a measure *end* (Minimal resolves measure starts only), a
        // region edge, or a column no glyph landed in — is skipped silently:
        // there is no slot for a solver to break at.
        let mut event_onsets: BTreeMap<EventId, TimePoint> = BTreeMap::new();
        let mut measure_starts: BTreeMap<MeasureId, TimePoint> = BTreeMap::new();
        for object in &region.objects {
            match (object.provenance().source, object.content()) {
                (TypedObjectId::Event(eid), LayoutContent::Note(note)) => {
                    event_onsets.insert(eid, note.position.clone());
                }
                (TypedObjectId::Event(eid), LayoutContent::Rest(rest)) => {
                    event_onsets.insert(eid, rest.position.clone());
                }
                (TypedObjectId::Measure(mid), LayoutContent::Measure(measure)) => {
                    measure_starts.insert(mid, measure.start.clone());
                }
                _ => {}
            }
        }
        for override_record in &logical.overrides {
            if override_record.target
                != OverrideTarget::ScoreGraph(TypedObjectId::Region(region_id))
            {
                continue;
            }
            let (anchor, system) = match &override_record.kind {
                OverrideKind::SystemBreak { anchor } => (anchor, true),
                OverrideKind::PageBreak { anchor } => (anchor, false),
                _ => continue,
            };
            let Some(time) = break_anchor_time(anchor, &event_onsets, &measure_starts) else {
                continue;
            };
            // A break opens a system after the barline at its onset, so it
            // names the first column that follows the barline there.
            let slot = [ColumnRole::Signature, ColumnRole::Note]
                .iter()
                .find_map(|role| {
                    let info = columns.get(&ColumnKey::Timed(time.clone(), *role))?;
                    column_members
                        .get(&info.slot)
                        .filter(|members| !members.is_empty())
                        .map(|_| info.slot)
                });
            let Some(slot) = slot else {
                continue;
            };
            // The override's binding strength is the break's kind: a `Hard`
            // override MUST be honored or error, a `Soft` one is a preference
            // (Chapter 7 §"Override Resolution"; the projection emits `Soft`).
            let kind = match override_record.priority {
                OverridePriority::Hard => BreakKind::Hard,
                OverridePriority::Soft => BreakKind::Soft,
            };
            constraints.push(if system {
                LayoutConstraint::SystemBreakAt { slot, kind }
            } else {
                LayoutConstraint::PageBreakAt { slot, kind }
            });
            // Record the attribution so the casting-off solver's decision can
            // cite the user override that asked for this break.
            break_origins.push(BreakOrigin {
                slot,
                class: if system {
                    BreakClass::System
                } else {
                    BreakClass::Page
                },
                override_id: override_record.id,
            });
        }

        // A staff band per staff of the region, in the region's own staff order —
        // the order `y_origin` stacks by, and exactly the set `band_of` can name.
        // (Driving this off the staves that emitted *glyphs* would leave a
        // stroke-only staff — one whose clef glyph is unbundled, so it engraves
        // to an anchor stroke — naming a band that does not exist.) An (empty)
        // inter-staff gap band sits between adjacent staves.
        //
        // The margin band is emitted unconditionally: a region's own traced
        // anchor is a *stroke*, and it names the margin band whether or not any
        // region-level glyph does. Both bands may carry no members; so may a gap
        // band. Membership drives the spring solve over glyphs, not existence.
        for staff in &staff_order {
            let layout_id = manifestation_layout_id(&TypedObjectId::Staff(*staff), region_id);
            let members = staff_members.remove(staff).unwrap_or_default();
            vertical_bands.push(VerticalBand::staff_manifestation(
                layout_id, *staff, members,
            ));
        }
        for gap in 1..staff_order.len() {
            let gap_id = inter_staff_gap_id(region_layout_id, gap);
            vertical_bands.push(VerticalBand::inter_staff_gap(gap_id));
        }
        vertical_bands.push(VerticalBand::margin(region_layout_id, margin_members));
        constrained_regions.push(ConstrainedLayoutRegion {
            provenance: region.provenance.clone(),
            glyphs: region_glyphs,
            // The region's kind-only logical axis, now populated with the real
            // time→slot placements resolved during spacing.
            time_axis: region.time_axis.clone().with_placements(region_placements),
        });

        region_x = staff_right + REGION_GAP;
    }

    let names: Vec<&str> = glyphs.iter().map(|glyph| glyph.glyph.as_str()).collect();
    let catalog = BravuraCatalog.identity(&names);
    if let Some(object) = logical
        .cross_region
        .iter()
        .find(|object| object.regions.is_empty())
    {
        return Err(LayoutTransformError::CrossRegionObjectHasNoRegion(
            object.provenance.stable_id,
        ));
    }

    Ok(ConstrainedLayoutIR {
        source: logical.source,
        regions: constrained_regions,
        horizontal_slots,
        glyphs,
        strokes,
        curves,
        vertical_bands,
        constraints,
        break_origins,
        engraving_decisions: logical.engraving_decisions.clone(),
        diagnostics,
        catalog,
        span_anchors,
        system_leads,
        staff_groups,
    })
}

/// A notehead the constrained pass will emit for a pitch: its glyph, the column
/// it sits in, its `y`, and which component of the note it belongs to.
struct Head {
    name: &'static str,
    key: ColumnKey,
    y: f32,
    /// The note's diatonic staff position, for ledger-line emission.
    step: StaffStep,
    comp: usize,
    /// The spelling's accidental stack (innermost — nearest the notehead — first),
    /// each drawn left of the notehead. Present only on the first component (a tie
    /// carries it; later components do not repeat it).
    accidentals: Vec<&'static str>,
    /// Each accidental's origin `x`, from the column's, in the order of
    /// `accidentals`; set once the column's accidentals are placed together.
    accidental_x: Vec<f32>,
    /// The head's `x` from its column's: across the stem from its chord-mate
    /// a second away, or beside another voice's head, and `0.0` otherwise.
    dx: f32,
    /// The alteration its spelling gives (`None` unpitched or unspelled): two
    /// voices' heads share a unison only at the same alteration.
    alteration: Option<i8>,
    /// The component's augmentation dots, the `y` of the space they sit in,
    /// and the first dot's `x` from the column's, right of every head of the
    /// staff's column.
    dots: u8,
    dot_y: f32,
    dot_x: f32,
    /// The event the head belongs to, and whether its component is tied to
    /// the event's next one.
    event: EventId,
    tied: bool,
}

/// Where a head is kept: the `index`th head of a pitch, or of an unpitched
/// note.
#[derive(Clone, Copy)]
enum HeadRef {
    Pitch(PitchId, usize),
    Unpitched(EventId, usize),
}

impl HeadRef {
    fn get<'a>(
        self,
        pitched: &'a BTreeMap<PitchId, Vec<Head>>,
        unpitched: &'a BTreeMap<EventId, Vec<Head>>,
    ) -> &'a Head {
        match self {
            HeadRef::Pitch(pitch, index) => &pitched[&pitch][index],
            HeadRef::Unpitched(event, index) => &unpitched[&event][index],
        }
    }

    fn get_mut<'a>(
        self,
        pitched: &'a mut BTreeMap<PitchId, Vec<Head>>,
        unpitched: &'a mut BTreeMap<EventId, Vec<Head>>,
    ) -> &'a mut Head {
        let heads = match self {
            HeadRef::Pitch(pitch, _) => pitched.get_mut(&pitch),
            HeadRef::Unpitched(event, _) => unpitched.get_mut(&event),
        };
        let index = match self {
            HeadRef::Pitch(_, index) | HeadRef::Unpitched(_, index) => index,
        };
        &mut heads.expect("a recorded head is kept")[index]
    }
}

/// One component's stem geometry, computed before column x is known (carried as
/// the column key plus the staff-space `y` extent).
struct StemSeg {
    key: ColumnKey,
    /// The lowest and highest notehead centre of the component's chord.
    lo: f32,
    hi: f32,
    drawn: bool,
    comp: usize,
    /// Stem direction: up (right of the heads) or down (left of them).
    up: bool,
    /// The direction its voice gives it beside another voice, if any.
    voiced: Option<bool>,
    /// Where the stem attaches, as an x offset from the column's notehead x.
    x_off: f32,
    /// How far its voice stands right of the column's x, beside another
    /// voice's head a second away.
    dx: f32,
    /// The stem's free end at its normal length, where a flag attaches.
    tip: f32,
    /// Where the stem is drawn to: its tip, lengthened to reach the far edge of
    /// a flag whose ink passes the tip (a 32nd's or shorter).
    end: f32,
    /// The flag an unbeamed eighth or shorter carries.
    flag: Option<&'static str>,
    /// How many flags, or beams, its value takes.
    beams: u8,
}

/// The drawn extent of one staff's note column: what a slur arcing over or under
/// it must clear, and which way its stem points.
#[derive(Clone, Copy)]
struct ColumnInk {
    /// Highest and lowest ink, noteheads and stem alike, in staff spaces.
    top: f32,
    bottom: f32,
    /// `Some(true)` when the column's stem is drawn and points up; `None` when
    /// the column draws no stem (a whole note).
    stem_up: Option<bool>,
    /// The notehead's horizontal centre, as an offset from the column's x.
    centre: f32,
}

/// One component's rest glyph (absent when the value has no bundled rest glyph).
struct RestSeg {
    name: Option<&'static str>,
    key: ColumnKey,
    y: f32,
    comp: usize,
    /// A hidden rest keeps its column and draws no ink.
    visible: bool,
    dots: u8,
    /// Its staff, onset and voice's place, which decide which other notes it
    /// stands clear of.
    staff: Option<StaffId>,
    start: TimePoint,
    voice: VoicePlace,
}

/// One component of a note or unpitched note, starting at `start` on
/// `staff`, with the `y` of each of its heads: a rest of another voice
/// starting with it stands clear of them.
struct Sounding {
    staff: Option<StaffId>,
    event: EventId,
    start: TimePoint,
    ys: Vec<f32>,
}

/// How far a rest moved off its place beside another voice stands clear of
/// that voice's heads.
const REST_CLEARANCE: f32 = 0.25;

/// Moves each rest of a voice beside another, a space at a time away from
/// the middle of the staff (up for an upper voice, down for a lower), until
/// its glyph stands `REST_CLEARANCE` clear of every head of another note on
/// its staff that starts with it, and so stands in its column. A note held
/// from before stands to its left.
fn clear_rests(rests: &mut BTreeMap<EventId, Vec<RestSeg>>, sounding: &[Sounding]) {
    for (eid, segs) in rests.iter_mut() {
        for seg in segs.iter_mut() {
            let up = match seg.voice {
                VoicePlace::Upper => true,
                VoicePlace::Lower => false,
                VoicePlace::Alone => continue,
            };
            let Some(bounds) = seg.name.and_then(metrics).map(|m| m.bounding_box()) else {
                continue;
            };
            let start = &seg.start;
            let heads = sounding
                .iter()
                .filter(|s| s.staff == seg.staff && s.event != *eid)
                .filter(|s| time_total(&s.start, start) == Ordering::Equal)
                .flat_map(|s| s.ys.iter().copied());
            if up {
                let Some(top) = heads.reduce(f32::max) else {
                    continue;
                };
                while seg.y + bounds.bottom.0 < top + 0.5 + REST_CLEARANCE {
                    seg.y += 1.0;
                }
            } else {
                let Some(bottom) = heads.reduce(f32::min) else {
                    continue;
                };
                while seg.y + bounds.top.0 > bottom - 0.5 - REST_CLEARANCE {
                    seg.y -= 1.0;
                }
            }
        }
    }
}

/// A horizontal column the spacing pass tiles left-to-right. The clef sits in the
/// `Lead` column; barlines, time signatures and notes occupy `Timed` columns (at
/// one onset, the barline ending the previous measure, then the signature of
/// the measure starting there, then its notes); the barline of a staff
/// instance's last measure closes the region in `End`.
#[derive(Clone, PartialEq, Eq)]
enum ColumnKey {
    Lead,
    Timed(TimePoint, ColumnRole),
    End,
}

/// Within one musical time: a clef change taking effect there, then the
/// barline ending the measure before it, then the signatures the measure
/// starting there introduces, then its notes. A system break falls between
/// the barline and what follows it, so a clef change at a measure's start
/// ends the system before it, as a courtesy, when that measure opens the next.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ColumnRole {
    Clef,
    Barline,
    Signature,
    Note,
}

impl Ord for ColumnKey {
    fn cmp(&self, other: &Self) -> Ordering {
        use ColumnKey::*;
        match (self, other) {
            (Lead, Lead) | (End, End) => Ordering::Equal,
            (Lead, _) => Ordering::Less,
            (_, Lead) => Ordering::Greater,
            (End, _) => Ordering::Greater,
            (_, End) => Ordering::Less,
            (Timed(ta, ra), Timed(tb, rb)) => time_total(ta, tb).then(ra.cmp(rb)),
        }
    }
}

impl PartialOrd for ColumnKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A resolved column: its x, its candidate spring slot (realized only if a glyph
/// occupies the column), the time it represents, and whether it is a musical note
/// column (the only kind the time axis indexes).
struct ColumnInfo {
    x: f32,
    slot: SpringSlotId,
    time: TimePoint,
    note_column: bool,
}

/// The accumulators a region's engraving emits into. Every primitive declares its
/// vertical band; glyphs alone carry band *membership* and a spring slot.
struct Emit<'a> {
    glyphs: &'a mut Vec<GlyphObject>,
    strokes: &'a mut Vec<Stroke>,
    curves: &'a mut Vec<Curve>,
    diagnostics: &'a mut Vec<LayoutDiagnostic>,
    column_members: BTreeMap<SpringSlotId, Vec<GlyphObjectId>>,
    region_glyphs: Vec<GlyphObjectId>,
    staff_members: BTreeMap<StaffId, Vec<GlyphObjectId>>,
    margin_members: Vec<GlyphObjectId>,
    staves_in_order: Vec<StaffId>,
}

impl Emit<'_> {
    /// Emits one glyph at `baseline`, in `band` and the column's spring `slot`,
    /// recording its band and slot membership. Chord/simultaneous glyphs sharing
    /// a column share one slot. Recording membership is what *realizes* the slot:
    /// a column no glyph reaches stays slot-less.
    fn glyph(
        &mut self,
        provenance: &Provenance,
        name: &'static str,
        baseline: Point,
        band: VerticalBandId,
        staff: Option<StaffId>,
        slot: SpringSlotId,
    ) {
        let bounding_box = metrics(name)
            .expect("engraved glyph names are bundled")
            .bounding_box();
        let glyph = GlyphObject {
            bounding_box,
            glyph: GlyphReference::borrowed(name),
            horizontal_slot: slot,
            baseline,
            vertical_band: band,
            anchor: Point::ORIGIN,
            layer: 0,
            style: ink(),
            provenance: provenance.clone(),
        };
        let gid = glyph.id();
        self.column_members.entry(slot).or_default().push(gid);
        self.region_glyphs.push(gid);
        match staff {
            Some(s) => {
                if !self.staves_in_order.contains(&s) {
                    self.staves_in_order.push(s);
                }
                self.staff_members.entry(s).or_default().push(gid);
            }
            None => self.margin_members.push(gid),
        }
        self.glyphs.push(glyph);
    }

    fn stroke(&mut self, stroke: Stroke) {
        self.strokes.push(stroke);
    }

    fn curve(&mut self, curve: Curve) {
        self.curves.push(curve);
    }

    fn diag(&mut self, source: TypedObjectId, kind: LayoutDiagnosticKind) {
        self.diagnostics.push(LayoutDiagnostic { source, kind });
    }

    /// Emits a glyph if its metrics are bundled, else surfaces the gap as an
    /// `UnbundledGlyph` diagnostic (the bundled metrics carry a representative
    /// subset — e.g. not every time-signature digit).
    fn glyph_if_bundled(
        &mut self,
        provenance: &Provenance,
        name: &'static str,
        baseline: Point,
        band: VerticalBandId,
        staff: Option<StaffId>,
        slot: SpringSlotId,
    ) {
        if metrics(name).is_some() {
            self.glyph(provenance, name, baseline, band, staff, slot);
        } else {
            self.diag(
                provenance.source,
                LayoutDiagnosticKind::UnbundledGlyph(GlyphReference::borrowed(name)),
            );
        }
    }
}

/// Solid black, the default ink for engraved primitives.
fn ink() -> GlyphStyle {
    GlyphStyle { rgba: 0x0000_00ff }
}

/// A solid black line stroke between two points.
fn line_stroke(
    provenance: Provenance,
    from: Point,
    to: Point,
    thickness: f32,
    band: VerticalBandId,
) -> Stroke {
    Stroke {
        provenance,
        from,
        to,
        thickness: StaffSpace(thickness),
        layer: 0,
        style: ink(),
        vertical_band: band,
    }
}

/// A zero-extent, zero-width stroke at `at`: an invisible traced anchor that
/// keeps a structural object (with no Minimal-tier glyph) provenance-tracked.
fn anchor(provenance: &Provenance, at: Point, band: VerticalBandId) -> Stroke {
    Stroke {
        provenance: provenance.clone(),
        from: at,
        to: at,
        thickness: StaffSpace(0.0),
        layer: 0,
        style: ink(),
        vertical_band: band,
    }
}

/// The provenance for a notated component: the object's own (exact) for the first
/// component, a synthesis from it for each later one.
fn component_provenance(base: &Provenance, comp: usize) -> Provenance {
    if comp == 0 {
        base.clone()
    } else {
        Provenance::synthesized(
            base.source,
            SynthesisKind::Registered(COMPONENT_SYNTHESIS),
            SynthesisInstanceKey(comp as u128),
            base.dependencies.clone(),
        )
    }
}

/// Resolves a projected break override's [`TimeAnchor`] to the region-local
/// [`TimePoint`] whose spacing column carries it, using the onsets this
/// region's own objects resolved to. Returns `None` — the break is skipped
/// silently — when the anchor addresses something no spacing column
/// represents: an event or measure outside this region, a measure *end* (the
/// Minimal slice resolves measure starts only), a region edge, or an offset
/// whose clock does not match its base.
fn break_anchor_time(
    anchor: &TimeAnchor,
    event_onsets: &BTreeMap<EventId, TimePoint>,
    measure_starts: &BTreeMap<MeasureId, TimePoint>,
) -> Option<TimePoint> {
    match anchor {
        TimeAnchor::WallClock { time } => Some(TimePoint::WallClock(*time)),
        TimeAnchor::Event { id, offset } => apply_offset(event_onsets.get(id)?.clone(), offset),
        TimeAnchor::Measure {
            id,
            position: MeasurePosition::Start,
            offset,
        } => apply_offset(measure_starts.get(id)?.clone(), offset),
        TimeAnchor::Measure { .. } | TimeAnchor::Region { .. } => None,
    }
}

/// The column a measure's barline occupies. A barline ends its measure: it
/// stands where the next measure of its staff instance starts, before that
/// measure's signatures and notes, and the instance's last measure (or one
/// whose end is unknown) closes the region at the right.
fn measure_column(measure: &crate::logical::MeasureContent) -> ColumnKey {
    match (&measure.end, measure.barline) {
        (Some(end), BarlineKind::Interior) => ColumnKey::Timed(end.clone(), ColumnRole::Barline),
        _ => ColumnKey::End,
    }
}

/// The column a measure's time signature occupies: at the measure's start,
/// after the barline ending the measure before it.
fn signature_column(measure: &crate::logical::MeasureContent) -> ColumnKey {
    ColumnKey::Timed(measure.start.clone(), ColumnRole::Signature)
}

/// A repeat boundary landing on one spacing column: which way its sign faces
/// (a coinciding end+start faces both) and the structures that own it, in
/// region emission order.
#[derive(Default)]
struct RepeatMark {
    start: bool,
    end: bool,
    sources: Vec<RepeatStructureId>,
    /// The synthesis owner: the smallest `(structure id, boundary site)` that
    /// landed on this column, `site` 0 for the structure's start boundary and
    /// 1 for its end. A structure has one boundary of each site, so the pair
    /// is a **semantic** instance identity — stable under unrelated edits,
    /// where a positional (column-rank) key would re-derive whenever any
    /// earlier column appeared or disappeared.
    owner: Option<(RepeatStructureId, u8)>,
}

/// The spacing column a repeat boundary's *sign* occupies, if placeable: the
/// barline-role column at its resolved time (minting one is pass 1's job), or
/// the region-closing column.
fn placement_column_key(placement: &RepeatPlacement) -> Option<ColumnKey> {
    match placement {
        RepeatPlacement::At(time) => Some(ColumnKey::Timed(time.clone(), ColumnRole::Barline)),
        RepeatPlacement::RegionEnd => Some(ColumnKey::End),
        RepeatPlacement::Unresolved => None,
    }
}

/// The realized column a volta boundary aligns with: the barline column at its
/// resolved time when one exists, else the note column there, else the
/// region-closing column for a region-end placement. `None` — the bracket
/// draws no ink — when nothing at that time was laid out.
fn placement_column<'a>(
    placement: &RepeatPlacement,
    columns: &'a BTreeMap<ColumnKey, ColumnInfo>,
) -> Option<&'a ColumnInfo> {
    match placement {
        RepeatPlacement::At(time) => [ColumnRole::Barline, ColumnRole::Note]
            .iter()
            .find_map(|role| columns.get(&ColumnKey::Timed(time.clone(), *role))),
        RepeatPlacement::RegionEnd => columns.get(&ColumnKey::End),
        RepeatPlacement::Unresolved => None,
    }
}

/// The composite SMuFL sign for a repeat boundary: a start opens the passage to
/// its right (`repeatLeft`), an end closes the passage to its left
/// (`repeatRight`), and a coinciding end+start draws the combined sign. The
/// no-facing case is unreachable (a [`RepeatMark`] records at least one), but
/// falls back to the plain barline rather than panicking on malformed input.
fn repeat_sign_name(start: bool, end: bool) -> &'static str {
    match (start, end) {
        (true, false) => "repeatLeft",
        (false, true) => "repeatRight",
        (true, true) => "repeatRightLeft",
        (false, false) => "barlineSingle",
    }
}

/// The baseline x aligning a repeat sign's **heavy line** with the boundary the
/// plain barline marks (a Minimal approximation from the glyph boxes): a start
/// sign's heavy line is its left edge, so it draws from the column; an end
/// sign's is its right edge, so it right-aligns to the plain barline's span;
/// the combined sign centers its shared heavy line on it.
fn repeat_sign_x(name: &str, column_x: f32) -> f32 {
    let right = |name: &str| metrics(name).map_or(0.0, |m| m.bounding_box().right.0);
    match name {
        "repeatRight" => column_x + right("barlineSingle") - right("repeatRight"),
        "repeatRightLeft" => column_x + (right("barlineSingle") - right("repeatRightLeft")) / 2.0,
        _ => column_x,
    }
}

/// How far a repeat sign's ink extends **right** of the plain-barline span it
/// replaced, in staff spaces — zero for the plain barlines themselves, so
/// repeat-free geometry is untouched. The morphed measure's time-signature
/// digits shift right by this so they clear the sign.
fn repeat_sign_right_extension(name: &str) -> f32 {
    match name {
        "repeatLeft" | "repeatRight" | "repeatRightLeft" => {
            let right = |name: &str| metrics(name).map_or(0.0, |m| m.bounding_box().right.0);
            (repeat_sign_x(name, 0.0) + right(name) - right("barlineSingle")).max(0.0)
        }
        _ => 0.0,
    }
}

/// The cubic-bézier curve for a slur, or `None` when it cannot be honestly
/// drawn in this region: either endpoint unresolved (dangling event, or an
/// endpoint whose column was laid out in another region), or a span that is not
/// left-to-right. `yo` is the slur's staff origin (its bottom line); `notes` is
/// that staff's note columns, the obstacle field the arc must clear.
///
/// **Side.** An authored direction wins. `Auto` places the slur *opposite* the
/// stems, the single-voice engraving rule — all stems up puts it below the
/// noteheads, all down puts it above. A span with stems both ways has no
/// notehead side, and goes above.
///
/// **Endpoints.** They sit a gap outside the endpoint column's ink on the chosen
/// side, horizontally at the notehead's centre. On the notehead side that is
/// just clear of the head; where the stem points the same way as the slur (a
/// mixed-stem span), the column's ink includes the stem, so the endpoint clears
/// the stem tip instead — which is what an engraver draws.
///
/// **Apex.** Span-proportional by default, then *raised until the arc clears
/// every column between the endpoints*. Because the control points sit on the
/// chord at thirds, `x` is linear in `t` and the arc's departure from the chord
/// is exactly `3·lift·t·(1−t)`; a column at parameter `t` needing `d` more
/// clearance therefore forces an apex of at least `d / (4·t·(1−t))`. An authored
/// height is a floor, never a ceiling: clearance may raise it, so honouring the
/// author cannot draw a slur through a note.
fn slur_curve(
    provenance: &Provenance,
    slur: &SlurContent,
    yo: f32,
    columns: &BTreeMap<ColumnKey, ColumnInfo>,
    band: VerticalBandId,
    notes: &BTreeMap<ColumnKey, ColumnInk>,
) -> Option<Curve> {
    let start_key = slur_endpoint_key(&slur.start)?;
    let end_key = slur_endpoint_key(&slur.end)?;
    let start_x = columns.get(&start_key)?.x;
    let end_x = columns.get(&end_key)?.x;

    // The staff's columns in the span, by x, so a shared-x sibling staff's notes
    // never enter this slur's obstacle field.
    // Each column is placed at its NOTEHEAD CENTRE, the same x the endpoints use,
    // so a column's curve parameter `t` is the one the arc actually passes it at.
    let spanned: Vec<(f32, ColumnInk)> = notes
        .iter()
        .filter_map(|(key, column)| columns.get(key).map(|info| (info.x, *column)))
        .filter(|(x, _)| *x >= start_x && *x <= end_x)
        .map(|(x, column)| (x + column.centre, column))
        .collect();

    let default_centre = NOTEHEAD_STEM_X * 0.5;
    let head = |key: &ColumnKey| notes.get(key).copied();
    let (p0x, p3x) = (
        start_x + head(&start_key).map_or(default_centre, |i| i.centre),
        end_x + head(&end_key).map_or(default_centre, |i| i.centre),
    );
    if p3x <= p0x {
        return None;
    }

    // Side: an authored direction wins; `Auto` goes opposite the stems.
    let above = match slur.direction {
        SlurDirection::Below => false,
        SlurDirection::Above => true,
        SlurDirection::Auto => {
            let ups = spanned
                .iter()
                .filter(|(_, i)| i.stem_up == Some(true))
                .count();
            let downs = spanned
                .iter()
                .filter(|(_, i)| i.stem_up == Some(false))
                .count();
            // All stems up ⇒ the noteheads are below ⇒ the slur is too. Anything
            // else (all down, mixed, or stemless) goes above.
            !(ups > 0 && downs == 0)
        }
    };

    // Endpoint y: a gap outside the endpoint column's ink, on the arc's side.
    // Absent ink (a column that drew no notehead) falls back to the staff edge.
    let endpoint_y = |key: &ColumnKey| -> f32 {
        match (head(key), above) {
            (Some(i), true) => i.top + SLUR_ENDPOINT_GAP,
            (Some(i), false) => i.bottom - SLUR_ENDPOINT_GAP,
            (None, true) => yo + STAFF_HEIGHT + SLUR_ENDPOINT_GAP,
            (None, false) => yo - SLUR_ENDPOINT_GAP,
        }
    };
    let (p0y, p3y) = (endpoint_y(&start_key), endpoint_y(&end_key));

    let span = p3x - p0x;
    let chord = |t: f32| p0y + t * (p3y - p0y);

    // Apex: an authored *positive* height, else span-proportional and clamped. A
    // non-positive authored height is out of range — it would flip or collapse
    // the arc — so it falls back to the default rather than producing a downward
    // "above" slur (authoring-validation may flag it separately).
    let default_height = (span * SLUR_HEIGHT_FACTOR).clamp(SLUR_MIN_HEIGHT, SLUR_MAX_HEIGHT);
    let mut height = slur
        .height
        .map(|h| h.0.get() as f32)
        .filter(|h| *h > 0.0)
        .unwrap_or(default_height);

    // Raise the apex until every column between the endpoints clears. `x` is
    // linear in `t` (the control points sit on the chord at thirds), and the
    // arc's departure from the chord at `t` is `4·height·t·(1−t)`.
    for (x, column) in &spanned {
        let t = (x - p0x) / span;
        if !(1e-3..=1.0 - 1e-3).contains(&t) {
            continue;
        }
        let needed = if above {
            column.top + SLUR_ENDPOINT_GAP - chord(t)
        } else {
            chord(t) - (column.bottom - SLUR_ENDPOINT_GAP)
        };
        if needed > 0.0 {
            height = height.max(needed / (4.0 * t * (1.0 - t)));
        }
    }

    // Lift the two control points so the cubic's apex (t = 0.5) sits `height`
    // from the chord: B(0.5) lifts the control y by 0.75, so the lift is
    // 4/3 · height (negated below the staff).
    let lift = if above { height } else { -height } * 4.0 / 3.0;
    // Thickness: an authored *positive* value, else the default. A
    // non-positive one is skipped — a zero would draw an invisible,
    // unhittable slur, and a negative one would fail geometry validation and
    // blank the whole layout; neither may reach the primitive.
    let thickness = slur
        .thickness
        .map(|t| t.0.get() as f32)
        .filter(|t| *t > 0.0)
        .unwrap_or(SLUR_THICKNESS);
    Some(Curve {
        provenance: provenance.clone(),
        p0: Point::new(p0x, p0y),
        p1: Point::new(p0x + span / 3.0, chord(1.0 / 3.0) + lift),
        p2: Point::new(p3x - span / 3.0, chord(2.0 / 3.0) + lift),
        p3: Point::new(p3x, p3y),
        thickness: StaffSpace(thickness),
        layer: 0,
        style: ink(),
        // The authored line pattern is rendered faithfully (dashed/dotted),
        // not deferred — the renderer strokes the path with it.
        line: slur.line,
        // The slur's OWN staff band — its notes' band, not whichever staff its
        // lifted endpoints happen to land nearest.
        vertical_band: band,
    })
}

/// A slur endpoint's resolved x: the note column at its resolved onset, or
/// `None` when the endpoint is unresolved or its column was not laid out in
/// this region.
fn slur_endpoint_key(endpoint: &SlurEndpoint) -> Option<ColumnKey> {
    match endpoint {
        SlurEndpoint::At(time) => Some(ColumnKey::Timed(time.clone(), ColumnRole::Note)),
        SlurEndpoint::Unresolved => None,
    }
}

/// The decimal digits of a displayed number (time-signature numerals, volta
/// ending numbers), most significant first.
fn digits_of(value: u32) -> Vec<u8> {
    if value == 0 {
        return vec![0];
    }
    let mut digits = Vec::new();
    let mut remaining = value;
    while remaining > 0 {
        digits.push((remaining % 10) as u8);
        remaining /= 10;
    }
    digits.reverse();
    digits
}

/// The SMuFL time-signature glyph for a decimal digit.
fn time_digit(digit: u8) -> &'static str {
    match digit {
        0 => "timeSig0",
        1 => "timeSig1",
        2 => "timeSig2",
        3 => "timeSig3",
        4 => "timeSig4",
        5 => "timeSig5",
        6 => "timeSig6",
        7 => "timeSig7",
        8 => "timeSig8",
        _ => "timeSig9",
    }
}

/// The `(offset, base value, dots, tied to the next)` of each notated
/// component, or a single implicit undotted quarter at offset zero when the
/// event carries no decomposition.
fn components_of(
    components: &[crate::logical::PlacedComponent],
) -> impl Iterator<Item = (MusicalDuration, NoteValue, u8, bool)> + '_ {
    let implicit = components.is_empty();
    let mapped = components.iter().map(|c| {
        (
            c.offset.clone(),
            c.component.base_value,
            c.component.dots,
            c.component.tied_to_next,
        )
    });
    let fallback = std::iter::once((MusicalDuration::zero(), NoteValue::Quarter, 0, false));
    mapped
        .chain(fallback.filter(move |_| implicit))
        .take(if implicit { 1 } else { usize::MAX })
}

/// The stem direction a voice beside another gives: up for an upper voice,
/// down for a lower; none alone.
fn voiced_up(place: VoicePlace) -> Option<bool> {
    match place {
        VoicePlace::Alone => None,
        VoicePlace::Upper => Some(true),
        VoicePlace::Lower => Some(false),
    }
}

/// The stem of one component of a note, and the ink its column carries: the
/// head furthest from the middle line decides the direction (a chord straddling
/// it evenly, or a note on it, stems down), the stem reaches an octave from the
/// outer head and never stops short of the middle line, and an eighth or
/// shorter takes its flag at that tip, the stem lengthened to the flag's far
/// edge where its ink passes the tip. `fallback` is where a head-less
/// component's stem would sit. A voice beside another (`voiced`) turns the
/// stem its own way instead.
#[allow(clippy::too_many_arguments)]
fn note_stem(
    value: NoteValue,
    yo: f32,
    steps: &[StaffStep],
    fallback: f32,
    name: &'static str,
    key: ColumnKey,
    comp: usize,
    voiced: Option<bool>,
) -> (StemSeg, ColumnInk) {
    let ys: Vec<f32> = steps.iter().map(|step| step_to_y(yo, *step)).collect();
    let drawn = has_stem(value) && !ys.is_empty();
    let bottom = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let bottom = if ys.is_empty() { fallback } else { bottom };
    let top = ys
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max)
        .max(bottom);
    let middle = yo + STAFF_HEIGHT * 0.5;
    let up = voiced.unwrap_or((top - middle) < (middle - bottom));
    // Attachment: an up-stem rides the right edge of the lowest head, a
    // down-stem the left edge of the highest.
    let head_box = metrics(name).map(|m| m.bounding_box());
    let x_off = if up {
        head_box.map_or(NOTEHEAD_STEM_X, |b| b.right.0)
    } else {
        head_box.map_or(0.0, |b| b.left.0)
    };
    let tip = if up {
        (top + STEM_LENGTH).max(middle)
    } else {
        (bottom - STEM_LENGTH).min(middle)
    };
    let direction = if up {
        epiphany_core::StemDirection::Up
    } else {
        epiphany_core::StemDirection::Down
    };
    let flag = drawn.then(|| flag_glyph(value, direction)).flatten();
    let flag_box = flag.and_then(metrics).map(|m| m.bounding_box());
    // A 32nd's or shorter flag reaches back past the tip, and the stem is
    // lengthened to meet its far edge; an eighth's or sixteenth's ends at the
    // tip (its curl's hair of overshoot does not move the stem).
    let end = match flag_box {
        Some(b) if flag_count(value) >= 3 && up => tip + b.top.0.max(0.0),
        Some(b) if flag_count(value) >= 3 => tip + b.bottom.0.min(0.0),
        _ => tip,
    };
    let head_top = head_box.map_or(0.5, |b| b.top.0);
    let head_bottom = head_box.map_or(-0.5, |b| b.bottom.0);
    let centre = head_box.map_or(NOTEHEAD_STEM_X * 0.5, |b| (b.left.0 + b.right.0) * 0.5);
    let mut ink = ColumnInk {
        top: top + head_top,
        bottom: bottom + head_bottom,
        stem_up: drawn.then_some(up),
        centre,
    };
    if drawn {
        let reach = match flag_box {
            Some(b) if up => end.max(tip + b.top.0),
            Some(b) => end.min(tip + b.bottom.0),
            None => end,
        };
        if up {
            ink.top = ink.top.max(reach);
        } else {
            ink.bottom = ink.bottom.min(reach);
        }
    }
    let seg = StemSeg {
        key,
        lo: bottom,
        hi: top,
        drawn,
        comp,
        up,
        voiced,
        x_off,
        dx: 0.0,
        tip,
        end,
        flag,
        beams: flag_count(value),
    };
    (seg, ink)
}

/// Folds one component's ink into its staff's column record.
fn merge_ink(
    column_ink: &mut BTreeMap<(StaffId, ColumnKey), ColumnInk>,
    staff: StaffId,
    key: &ColumnKey,
    ink: ColumnInk,
) {
    let entry = column_ink.entry((staff, key.clone())).or_insert(ink);
    entry.top = entry.top.max(ink.top);
    entry.bottom = entry.bottom.min(ink.bottom);
}

/// The `y` of the augmentation dots of each head of a chord, in the order the
/// steps are given: a head on a line puts its dots in the space above (below,
/// in a lower voice beside another, `lower`), a head
/// in a space puts them beside it, and a second head wanting a space already
/// taken goes to the next space down, so no two heads' dots coincide.
fn dot_positions(yo: f32, steps: &[StaffStep], lower: bool) -> Vec<f32> {
    let mut order: Vec<usize> = (0..steps.len()).collect();
    order.sort_by(|a, b| steps[*b].cmp(&steps[*a]).then(a.cmp(b)));
    let mut taken: BTreeSet<StaffStep> = BTreeSet::new();
    let mut ys = vec![0.0; steps.len()];
    for i in order {
        let mut space = match (steps[i].rem_euclid(2) == 0, lower) {
            (true, false) => steps[i] + 1,
            (true, true) => steps[i] - 1,
            (false, _) => steps[i],
        };
        while taken.contains(&space) {
            space -= 2;
        }
        taken.insert(space);
        ys[i] = step_to_y(yo, space);
    }
    ys
}

/// The clef and key signature a staff shows at `at`: the clef in force there
/// at x 0 on its line, and the key's accidentals after it, `KEY_GAP` clear of
/// the clef's ink, `KEY_ACC_X` apart. Empty when the clef has no bundled
/// glyph; no key accidental for a clef with no diatonic positions.
fn lead_glyphs(content: &StaffContent, at: &TimePoint, yo: f32) -> Vec<(&'static str, f32, f32)> {
    let clef = active_clef_or(&content.clefs, at, content.default_clef);
    let Some(name) = clef_glyph_for(&clef) else {
        return Vec::new();
    };
    let mut glyphs = vec![(name, 0.0, yo + (clef.line as f32 - 1.0))];
    let clef_right = metrics(name).map_or(0.0, |m| m.bounding_box().right.0);
    if let Some(key) = key_at(&content.keys, at) {
        for (i, accidental) in key_signature(key, &clef).iter().enumerate() {
            glyphs.push((
                accidental.glyph,
                clef_right + KEY_GAP + i as f32 * KEY_ACC_X,
                step_to_y(yo, accidental.position),
            ));
        }
    }
    glyphs
}

/// The right edge of a lead's ink, from its glyphs' x offsets and metrics.
fn lead_extent(glyphs: &[(&'static str, f32, f32)]) -> f32 {
    glyphs
        .iter()
        .map(|(name, x, _)| x + metrics(name).map_or(0.0, |m| m.bounding_box().right.0))
        .fold(0.0, f32::max)
}

/// The clef changes a staff draws within its systems, in time order: each
/// change after the staff's start whose clef differs from the one in force
/// before it. A change restating the clef in force draws nothing; at a
/// system's start the lead shows the clef in force either way.
fn drawn_clef_changes(content: &StaffContent) -> Vec<(TimePoint, Clef)> {
    let mut times: Vec<TimePoint> = content
        .clefs
        .iter()
        .map(|c| c.time.clone())
        .filter(|t| time_total(t, &origin()) == Ordering::Greater)
        .collect();
    times.sort_by(time_total);
    times.dedup();
    let mut current = active_clef_or(&content.clefs, &origin(), content.default_clef);
    let mut out = Vec::new();
    for time in times {
        let clef = active_clef_or(&content.clefs, &time, content.default_clef);
        if clef != current {
            out.push((time, clef));
        }
        current = clef;
    }
    out
}

/// A clef change's glyphs, smaller than a staff's leading clef: the change
/// clef on its line at x 0 and, for an octave clef, its numeral centred over
/// or under it, since SMuFL has no change-size octave clef. A shape with no
/// change glyph draws its full clef; none without a bundled glyph.
fn clef_change_glyphs(clef: &Clef, yo: f32) -> Vec<(&'static str, f32, f32)> {
    let y = yo + (clef.line as f32 - 1.0);
    let name = match clef.shape {
        epiphany_core::ClefShape::G => "gClefChange",
        epiphany_core::ClefShape::F => "fClefChange",
        epiphany_core::ClefShape::C => "cClefChange",
        epiphany_core::ClefShape::Percussion => {
            return clef_glyph_for(clef).map_or(Vec::new(), |name| vec![(name, 0.0, y)]);
        }
    };
    let mut glyphs = vec![(name, 0.0, y)];
    let numeral = match clef.octave_shift.unsigned_abs() {
        1 => "clef8",
        2 => "clef15",
        _ => return glyphs,
    };
    if let (Some(c), Some(n)) = (metrics(name), metrics(numeral)) {
        let (c, n) = (c.bounding_box(), n.bounding_box());
        let x = (c.left.0 + c.right.0 - n.left.0 - n.right.0) / 2.0;
        let ny = if clef.octave_shift > 0 {
            y + c.top.0 - CLEF_NUMERAL_OVERLAP - n.bottom.0
        } else {
            y + c.bottom.0 + CLEF_NUMERAL_OVERLAP - n.top.0
        };
        glyphs.push((numeral, x, ny));
    }
    glyphs
}

/// The measure state of a letter and octave a tie has carried an
/// accidental into: no alteration, so the next such note shows its own.
const CARRIED: i8 = i8::MIN;

/// The accidental each pitch's first head shows, by its staff's key and the
/// earlier notes of its measure: none where the key, or an accidental earlier
/// in the measure on the same letter and octave, already gives the pitch's
/// alteration; the accidental of the alteration (a natural to cancel) where
/// they give another, which then holds to the barline; and none on a note a
/// tie continues into. Where that note's alteration is not what the measure
/// gave, the next note of its letter and octave in the measure shows its own
/// accidental, the tied one's restated or a courtesy natural. Every voice of
/// a staff shares its state, taken in time order. A pitch whose spelling is
/// not whole semitones is absent, and draws its own stack.
fn context_accidentals(
    objects: &[crate::logical::LayoutObject],
) -> BTreeMap<PitchId, Vec<&'static str>> {
    struct Staff<'a> {
        keys: &'a [PlacedKeySignature],
        measures: Vec<&'a TimePoint>,
        notes: Vec<&'a crate::logical::NoteContent>,
    }
    let mut staves: BTreeMap<StaffId, Staff> = BTreeMap::new();
    let mut tied_into: BTreeSet<PitchId> = BTreeSet::new();
    for object in objects {
        if let LayoutContent::Tie(tie) = object.content() {
            tied_into.extend(tie.pairs.iter().map(|(_, end)| *end));
        }
        let Some(staff) = object.staff() else {
            continue;
        };
        let entry = staves.entry(staff).or_insert(Staff {
            keys: &[],
            measures: Vec::new(),
            notes: Vec::new(),
        });
        match object.content() {
            LayoutContent::Staff(content) => entry.keys = content.keys.as_slice(),
            LayoutContent::Measure(measure) => entry.measures.push(&measure.start),
            LayoutContent::Note(note) => entry.notes.push(note),
            _ => {}
        }
    }
    let mut shown = BTreeMap::new();
    for staff in staves.values_mut() {
        staff.measures.sort_by(|a, b| time_total(a, b));
        staff
            .notes
            .sort_by(|a, b| time_total(&a.position, &b.position));
        let mut measure = None;
        let mut state: BTreeMap<(epiphany_core::CmnNominal, i8), i8> = BTreeMap::new();
        for note in &staff.notes {
            let index = staff
                .measures
                .partition_point(|start| time_total(start, &note.position) != Ordering::Greater);
            if measure != Some(index) {
                measure = Some(index);
                state.clear();
            }
            let key = key_at(staff.keys, &note.position);
            for pitch in &note.pitches {
                let Some(spelling) = &pitch.spelling else {
                    continue;
                };
                let SpellingNominal::Cmn(nominal) = spelling.nominal else {
                    continue;
                };
                let Some(alteration) = stack_alteration(&spelling.accidentals) else {
                    continue;
                };
                let place = (nominal, spelling.octave);
                let current = state
                    .get(&place)
                    .copied()
                    .unwrap_or_else(|| key.map_or(0, |k| key_alteration(k, nominal)));
                if tied_into.contains(&pitch.pitch) {
                    // A tie carries its accidental to the tied note alone: a
                    // later note of its letter and octave in the bar states
                    // its own, a courtesy where the key would give it.
                    shown.insert(pitch.pitch, Vec::new());
                    if alteration != current {
                        state.insert(place, CARRIED);
                    }
                    continue;
                }
                let glyphs = if alteration == current {
                    Vec::new()
                } else {
                    state.insert(place, alteration);
                    alteration_glyph(alteration).into_iter().collect()
                };
                shown.insert(pitch.pitch, glyphs);
            }
        }
    }
    shown
}

/// The key signature in force at `at` (the latest change at or before it,
/// else the earliest), or `None` when the staff declares none.
fn key_at(keys: &[PlacedKeySignature], at: &TimePoint) -> Option<KeySignature> {
    keys.iter()
        .filter(|placed| {
            matches!(
                time_cmp(&placed.time, at),
                Some(Ordering::Less | Ordering::Equal)
            )
        })
        .max_by(|a, b| time_total(&a.time, &b.time))
        .or_else(|| keys.iter().min_by(|a, b| time_total(&a.time, &b.time)))
        .map(|placed| placed.key)
}

/// A tie's control points from `x0` to `x3`, its ends level at `y`, arcing
/// above or below and rising with its length to at most `TIE_MAX_HEIGHT`.
pub fn tie_arc(x0: f32, x3: f32, y: f32, above: bool) -> [Point; 4] {
    let sign = if above { 1.0 } else { -1.0 };
    let span = x3 - x0;
    let height = (span * 0.15).clamp(TIE_MIN_HEIGHT, TIE_MAX_HEIGHT);
    // A cubic's control points sit 4/3 of the apex height off the chord.
    let lift = sign * height * 4.0 / 3.0;
    [
        Point::new(x0, y),
        Point::new(x0 + span * 0.25, y + lift),
        Point::new(x3 - span * 0.25, y + lift),
        Point::new(x3, y),
    ]
}

/// A tie's arc from head `a` (in column `from`) to head `b` (in column `to`):
/// from just right of `a` to just left of `b`, a little off the heads on the
/// side it arcs to, rising with its length to at most `TIE_MAX_HEIGHT`.
#[allow(clippy::too_many_arguments)]
fn tie_curve(
    provenance: Provenance,
    a: &Head,
    from: &ColumnInfo,
    b: &Head,
    to: &ColumnInfo,
    above: bool,
    band: VerticalBandId,
    (from_ink, to_ink): (&[InkBox], &[InkBox]),
) -> Curve {
    let extent = |head: &Head, x: f32| {
        metrics(head.name).map_or((x, x + NOTEHEAD_STEM_X), |m| {
            let b = m.bounding_box();
            (x + b.left.0, x + b.right.0)
        })
    };
    let sign = if above { 1.0 } else { -1.0 };
    let y = a.y + sign * TIE_OFFSET;
    let (x0, x3) = tie_ends(
        from_ink,
        extent(a, from.x + a.dx),
        to_ink,
        extent(b, to.x + b.dx),
        y,
    );
    // Each end rides its own column's slot, and the spacing gives the tie
    // its length, so in this frame, where columns stand closer than they
    // will, the end may fall before the start; the arc keeps its control
    // points' fractions of the span, which must not vanish.
    let x3 = if from.slot == to.slot {
        x3.max(x0 + TIE_GAP)
    } else if (x3 - x0).abs() < 1e-3 {
        x0 + 1e-3
    } else {
        x3
    };
    let [p0, p1, p2, p3] = tie_arc(x0, x3, y, above);
    Curve {
        provenance,
        p0,
        p1,
        p2,
        p3,
        thickness: StaffSpace(TIE_THICKNESS),
        layer: 0,
        style: ink(),
        line: LineStyle::Solid,
        vertical_band: band,
    }
}

/// What a tuplet's marks are placed by: the columns, each event's stems and
/// rests (their columns and extents), the drawn ink of its staff's columns
/// (heads, stems and beams), each event's voice place, and the staff's drawn
/// beam groups by their sorted members.
struct TupletInk<'a> {
    columns: &'a BTreeMap<ColumnKey, ColumnInfo>,
    stems: &'a BTreeMap<EventId, Vec<StemSeg>>,
    rests: &'a BTreeMap<EventId, Vec<RestSeg>>,
    ink: &'a BTreeMap<ColumnKey, ColumnInk>,
    voices: &'a BTreeMap<EventId, VoicePlace>,
    beams: &'a BTreeSet<Vec<EventId>>,
}

/// A tuplet's marks: its number's digit glyphs and where they stand, the slot
/// the number rides (its middle member's column), and the bracket's strokes,
/// each with the slots its two ends ride.
struct TupletMarks {
    digits: Vec<(&'static str, Point)>,
    slot: SpringSlotId,
    bracket: Vec<(Point, Point, SpringSlotId, SpringSlotId)>,
}

/// A tuplet's number and bracket, from its first member's column to its last
/// member's, standing `TUPLET_CLEARANCE` clear of the ink of every column
/// between: above for an upper voice, below for a lower, and alone on the
/// side most of its stems point (above when none has a stem). The number
/// shows the ratio's `actual` term, centered on the span; the bracket, with
/// a gap for the number and its ends hooked toward the notes, is left out
/// when the members are notes beamed together as one group. `None` when no
/// member has a column on the staff.
fn tuplet_marks(tuplet: &crate::logical::TupletContent, at: &TupletInk) -> Option<TupletMarks> {
    // A member's columns; with `inked`, only those it draws in, since a
    // column's slot is realized only by a glyph, and a bracket end anchored
    // to a hidden rest's column would name a slot that does not exist.
    let keys_of = |event: &EventId, inked: bool| -> Vec<ColumnKey> {
        let stems = at
            .stems
            .get(event)
            .into_iter()
            .flatten()
            .map(|s| s.key.clone());
        let rests = at
            .rests
            .get(event)
            .into_iter()
            .flatten()
            .filter(|r| !inked || (r.visible && r.name.is_some()))
            .map(|r| r.key.clone());
        stems.chain(rests).collect()
    };
    let mut keys: Vec<ColumnKey> = tuplet
        .members
        .iter()
        .flat_map(|e| keys_of(e, true))
        .collect();
    keys.sort();
    let (first, last) = (keys.first()?.clone(), keys.last()?.clone());
    // The number's own glyph realizes its slot, so any member column serves.
    let middle = tuplet.members.get(tuplet.members.len() / 2).and_then(|e| {
        let mut own = keys_of(e, false);
        own.sort();
        own.into_iter().next()
    })?;
    let (left, right, slot) = (
        at.columns.get(&first)?,
        at.columns.get(&last)?,
        at.columns.get(&middle)?.slot,
    );
    let head_right = metrics("noteheadBlack").map_or(NOTEHEAD_STEM_X, |m| m.bounding_box().right.0);
    let (x0, x1) = (left.x, right.x + head_right);

    let segs: Vec<&StemSeg> = tuplet
        .members
        .iter()
        .filter_map(|e| at.stems.get(e)?.first())
        .filter(|seg| seg.drawn)
        .collect();
    let ups = segs.iter().filter(|seg| seg.up).count();
    let above = match tuplet.members.first().and_then(|e| at.voices.get(e)) {
        Some(VoicePlace::Upper) => true,
        Some(VoicePlace::Lower) => false,
        _ => 2 * ups >= segs.len(),
    };

    // The ink the marks clear: every column between the first member's and
    // the last's, and each member rest's glyph.
    let mut top = f32::NEG_INFINITY;
    let mut bottom = f32::INFINITY;
    for ink in at.ink.range(first..=last).map(|(_, ink)| ink) {
        top = top.max(ink.top);
        bottom = bottom.min(ink.bottom);
    }
    for rest in tuplet
        .members
        .iter()
        .filter_map(|e| at.rests.get(e))
        .flatten()
    {
        if let Some(b) = rest.name.and_then(metrics).map(|m| m.bounding_box()) {
            top = top.max(rest.y + b.top.0);
            bottom = bottom.min(rest.y + b.bottom.0);
        }
    }
    if !top.is_finite() || !bottom.is_finite() {
        return None;
    }

    let names: Vec<&'static str> = digits_of(tuplet.ratio.actual())
        .into_iter()
        .map(tuplet_digit)
        .collect();
    let boxes: Vec<BoundingBox> = names
        .iter()
        .map(|name| metrics(name).map(|m| m.bounding_box()))
        .collect::<Option<_>>()?;
    let width: f32 = boxes.iter().map(|b| b.right.0 - b.left.0).sum();
    let height = boxes.iter().map(|b| b.top.0).fold(0.0, f32::max);
    let base = if above {
        top + TUPLET_CLEARANCE
    } else {
        bottom - TUPLET_CLEARANCE
    };
    let baseline = if above { base } else { base - height };
    let mut x = (x0 + x1) / 2.0 - width / 2.0;
    let mut digits = Vec::with_capacity(names.len());
    for (name, b) in names.iter().zip(&boxes) {
        digits.push((*name, Point::new(x - b.left.0, baseline)));
        x += b.right.0 - b.left.0;
    }

    let mut members = tuplet.members.clone();
    members.sort();
    let beamed =
        tuplet.members.iter().all(|e| at.rests.get(e).is_none()) && at.beams.contains(&members);
    let mut bracket = Vec::new();
    if !beamed {
        let line = baseline + height / 2.0;
        let hook = if above {
            line - TUPLET_HOOK
        } else {
            line + TUPLET_HOOK
        };
        let (gap0, gap1) = (
            (x0 + x1) / 2.0 - width / 2.0 - TUPLET_NUMBER_GAP,
            (x0 + x1) / 2.0 + width / 2.0 + TUPLET_NUMBER_GAP,
        );
        let (start, end) = (left.slot, right.slot);
        bracket.push((Point::new(x0, hook), Point::new(x0, line), start, start));
        bracket.push((
            Point::new(x0, line),
            Point::new(gap0.max(x0), line),
            start,
            slot,
        ));
        bracket.push((
            Point::new(gap1.min(x1), line),
            Point::new(x1, line),
            slot,
            end,
        ));
        bracket.push((Point::new(x1, line), Point::new(x1, hook), end, end));
    }
    Some(TupletMarks {
        digits,
        slot,
        bracket,
    })
}

/// The SMuFL tuplet digit glyph for `digit` (0–9).
fn tuplet_digit(digit: u8) -> &'static str {
    match digit {
        0 => "tuplet0",
        1 => "tuplet1",
        2 => "tuplet2",
        3 => "tuplet3",
        4 => "tuplet4",
        5 => "tuplet5",
        6 => "tuplet6",
        7 => "tuplet7",
        8 => "tuplet8",
        _ => "tuplet9",
    }
}

/// Whether the `i`th of `n` ties leaving one chord, highest first, arcs
/// above: by its voice's place beside another voice; then, in a chord, the
/// upper half above and the lower half below; then, for the middle tie of
/// an odd chord or a single note, away from its stem, or from the middle
/// line when it has none.
fn tie_above(
    voice: Option<VoicePlace>,
    i: usize,
    n: usize,
    stem_up: Option<bool>,
    y: f32,
    yo: f32,
) -> bool {
    match voice {
        Some(VoicePlace::Upper) => true,
        Some(VoicePlace::Lower) => false,
        _ if n > 1 && 2 * i + 1 < n => true,
        _ if n > 1 && 2 * i + 1 > n => false,
        _ => stem_up.map_or(y >= yo + STAFF_HEIGHT * 0.5, |up| !up),
    }
}

/// What a tie between a note's components takes its side from: each event's
/// voice place, its heads by component (highest first) and its stems.
struct TieSides<'a> {
    voices: &'a BTreeMap<EventId, VoicePlace>,
    chords: &'a BTreeMap<(EventId, usize), Vec<f32>>,
    stems: &'a BTreeMap<EventId, Vec<StemSeg>>,
    /// Each staff column's ink, which a tie's ends stand clear of.
    ink: &'a BTreeMap<(Option<StaffId>, ColumnKey), Vec<InkBox>>,
}

/// A box of a staff column's ink on the page, which a tie's end stands clear
/// of: a head, a ledger line, a stem, a dot or an accidental. An accidental
/// stands `behind` its head, left of the column, and only meets a tie's
/// arriving end.
#[derive(Clone, Copy)]
struct InkBox {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    behind: bool,
}

/// How far above and below a tie's end its ink reaches, near its head.
const TIE_END_REACH: f32 = 0.2;

/// The ink a head brings to its column: the head, its ledger lines, its dots
/// and its accidentals, in page coordinates, from the column's `x` and its
/// staff's `yo`.
fn column_ink_boxes(head: &Head, x: f32, yo: f32) -> Vec<InkBox> {
    let at = |name: &str, ox: f32, oy: f32, behind: bool| {
        metrics(name).map(|m| {
            let b = m.bounding_box();
            InkBox {
                left: ox + b.left.0,
                right: ox + b.right.0,
                bottom: oy + b.bottom.0,
                top: oy + b.top.0,
                behind,
            }
        })
    };
    let hx = x + head.dx;
    let mut boxes: Vec<InkBox> = at(head.name, hx, head.y, false).into_iter().collect();
    let head_box = metrics(head.name).map(|m| m.bounding_box());
    let left = head_box.map_or(0.0, |b| b.left.0);
    let right = head_box.map_or(NOTEHEAD_STEM_X, |b| b.right.0);
    for step in ledger_steps(head.step) {
        let y = step_to_y(yo, step);
        boxes.push(InkBox {
            left: hx + left - LEDGER_LINE_EXTENSION,
            right: hx + right + LEDGER_LINE_EXTENSION,
            bottom: y - STAFF_LINE_THICKNESS / 2.0,
            top: y + STAFF_LINE_THICKNESS / 2.0,
            behind: false,
        });
    }
    for dot in 0..head.dots {
        let dx = x + head.dot_x + f32::from(dot) * DOT_STEP;
        boxes.extend(at("augmentationDot", dx, head.dot_y, false));
    }
    for (name, offset) in head.accidentals.iter().zip(&head.accidental_x) {
        boxes.extend(at(name, x + offset, head.y, true));
    }
    boxes
}

/// Where a tie at height `y` from head `a` (left edge `a_left`, right edge
/// `a_right`) to head `b` (`b_left`, `b_right`) may start and end: right of
/// every box of `from`'s ink its end meets there that does not stand wholly
/// left of `a`, and left of every box of `to`'s ink its end meets that does
/// not stand wholly right of `b`, each by `TIE_GAP`. A head set beside its
/// own across a stem, another voice's head beside it, a stem, a ledger line,
/// a dot or an accidental at the tie's height is passed, not run through.
fn tie_ends(
    from: &[InkBox],
    (a_left, a_right): (f32, f32),
    to: &[InkBox],
    (b_left, b_right): (f32, f32),
    y: f32,
) -> (f32, f32) {
    let meets = |b: &&InkBox| b.bottom < y + TIE_END_REACH && b.top > y - TIE_END_REACH;
    let start = from
        .iter()
        .filter(|b| !b.behind && b.right > a_left)
        .filter(meets)
        .map(|b| b.right)
        .fold(a_right, f32::max);
    let end = to
        .iter()
        .filter(|b| b.left < b_right)
        .filter(meets)
        .map(|b| b.left)
        .fold(b_left, f32::min);
    (start + TIE_GAP, end - TIE_GAP)
}

/// The ties between the successive components of one pitch (or unpitched
/// note) that its decomposition ties, each taking its side as a tie between
/// two events does (`tie_above`), synthesized from `provenance`, with the
/// slots its ends ride.
fn component_ties(
    provenance: &Provenance,
    heads: &[Head],
    sides: &TieSides,
    columns: &BTreeMap<ColumnKey, ColumnInfo>,
    (staff, yo): (Option<StaffId>, f32),
    band: VerticalBandId,
) -> Vec<(Curve, SpringSlotId, SpringSlotId)> {
    let mut ties = Vec::new();
    for pair in heads.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if !a.tied || b.comp != a.comp + 1 {
            continue;
        }
        let (Some(from), Some(to)) = (columns.get(&a.key), columns.get(&b.key)) else {
            continue;
        };
        let stem_up = sides
            .stems
            .get(&a.event)
            .and_then(|segs| segs.iter().find(|seg| seg.comp == a.comp))
            .filter(|seg| seg.drawn)
            .map(|seg| seg.up);
        let chord = sides
            .chords
            .get(&(a.event, a.comp))
            .map_or(&[][..], Vec::as_slice);
        let i = chord.iter().position(|&y| y == a.y).unwrap_or(0);
        let voice = sides.voices.get(&a.event).copied();
        let above = tie_above(voice, i, chord.len(), stem_up, a.y, yo);
        let tie_provenance = Provenance::synthesized(
            provenance.source,
            SynthesisKind::Registered(TIE_SYNTHESIS),
            SynthesisInstanceKey(1 << 64 | a.comp as u128),
            provenance.dependencies.clone(),
        );
        let ink = |key: &ColumnKey| {
            sides
                .ink
                .get(&(staff, key.clone()))
                .map_or(&[][..], Vec::as_slice)
        };
        ties.push((
            tie_curve(
                tie_provenance,
                a,
                from,
                b,
                to,
                above,
                band,
                (ink(&a.key), ink(&b.key)),
            ),
            from.slot,
            to.slot,
        ));
    }
    ties
}

/// The heads of one staff's column, set clear of each other: each head's `x`
/// from the column's, the shift of each event (by component) that stands
/// right of another voice's, and the `x` of the column's first dot.
///
/// In a chord, taken outward from the head the stem leaves (up from the
/// lowest for an up-stem, down from the highest for a down-stem), a head a
/// second or unison from one on the stem's usual side goes across the stem,
/// clear of it: right of an up-stem, left of a down-stem, so a cluster
/// alternates.
/// Then each voice in turn (upper voices first) stands right of the heads
/// before it that its heads would touch, its stem with it, so a lower voice a
/// second under an upper one stands to its right with their stems in one
/// line. A unison two voices share (one glyph, one alteration, one count of
/// dots) is one head drawn twice. The dots of every head stand right of all
/// of the column's heads. `ups` and `places` are each head's stem direction
/// and voice place.
fn place_heads(
    heads: &[&Head],
    ups: &[bool],
    places: &[VoicePlace],
) -> (Vec<f32>, Vec<(EventId, usize, f32)>, f32) {
    let ink = |head: &Head| {
        metrics(head.name).map_or([0.0, -0.5, NOTEHEAD_STEM_X, 0.5], |m| {
            let b = m.bounding_box();
            [b.left.0, b.bottom.0, b.right.0, b.top.0]
        })
    };
    let mut dx = vec![0.0f32; heads.len()];
    let mut events: Vec<EventId> = heads.iter().map(|head| head.event).collect();
    events.sort();
    events.dedup();
    let of = |event: EventId| -> Vec<usize> {
        (0..heads.len())
            .filter(|i| heads[*i].event == event)
            .collect()
    };
    for &event in &events {
        let mut chord = of(event);
        let up = ups[chord[0]];
        chord.sort_by_key(|i| heads[*i].step);
        if !up {
            chord.reverse();
        }
        let mut previous: Option<(StaffStep, bool)> = None;
        for i in chord {
            let across =
                matches!(previous, Some((step, false)) if (heads[i].step - step).abs() <= 1);
            if across {
                let [left, _, right, _] = ink(heads[i]);
                dx[i] = if up {
                    right - left + SECOND_CLEARANCE
                } else {
                    left - right - SECOND_CLEARANCE
                };
            }
            previous = Some((heads[i].step, across));
        }
    }
    let rank = |place: VoicePlace| match place {
        VoicePlace::Upper => 0,
        VoicePlace::Alone => 1,
        VoicePlace::Lower => 2,
    };
    events.sort_by_key(|event| (rank(places[of(*event)[0]]), *event));
    let mut shifts = Vec::new();
    let mut standing: Vec<usize> = Vec::new();
    for &event in &events {
        let mine = of(event);
        let mut shift = 0.0f32;
        // Each pass moves it right of the furthest head it touches, so it
        // ends within one pass per head standing.
        for _ in 0..=standing.len() {
            let mut reach = f32::NEG_INFINITY;
            for &i in &mine {
                let a = ink(heads[i]);
                let (al, ar) = (shift + dx[i] + a[0], shift + dx[i] + a[2]);
                let (ab, at) = (heads[i].y + a[1], heads[i].y + a[3]);
                for &j in &standing {
                    let b = ink(heads[j]);
                    let (bl, br) = (dx[j] + b[0], dx[j] + b[2]);
                    let (bb, bt) = (heads[j].y + b[1], heads[j].y + b[3]);
                    let shared = heads[i].step == heads[j].step
                        && heads[i].name == heads[j].name
                        && heads[i].dots == heads[j].dots
                        && heads[i].alteration == heads[j].alteration
                        && shift + dx[i] == dx[j];
                    if !shared && al < br && bl < ar && ab < bt && bb < at {
                        reach = reach.max(br);
                    }
                }
            }
            if !reach.is_finite() {
                break;
            }
            let left = mine
                .iter()
                .map(|&i| dx[i] + ink(heads[i])[0])
                .fold(f32::INFINITY, f32::min);
            shift = reach - left + SECOND_CLEARANCE;
        }
        if shift != 0.0 {
            for &i in &mine {
                dx[i] += shift;
            }
            shifts.push((event, heads[mine[0]].comp, shift));
        }
        standing.extend(mine);
    }
    let right = (0..heads.len())
        .map(|i| dx[i] + ink(heads[i])[2])
        .fold(f32::NEG_INFINITY, f32::max);
    let dot_x = if right.is_finite() { right } else { 0.0 } + DOT_GAP;
    (dx, shifts, dot_x)
}

/// A beam group's members, one drawn stem each, and the way their stems
/// turn: a voice beside another turns the whole group its way; otherwise the
/// note furthest from the middle line decides (down on a tie). `None` for a
/// group of fewer than two.
fn beam_members(
    group: &crate::logical::BeamGroup,
    event_stems: &BTreeMap<EventId, Vec<StemSeg>>,
    middle: f32,
) -> Option<(Vec<EventId>, bool)> {
    let members: Vec<EventId> = group
        .events
        .iter()
        .copied()
        .filter(|e| matches!(event_stems.get(e).map(Vec::as_slice), Some([seg]) if seg.drawn))
        .collect();
    if members.len() < 2 {
        return None;
    }
    let segs: Vec<&StemSeg> = members.iter().map(|e| &event_stems[e][0]).collect();
    let voiced = segs.iter().find_map(|seg| seg.voiced);
    let above = segs
        .iter()
        .map(|seg| seg.hi - middle)
        .fold(f32::NEG_INFINITY, f32::max);
    let below = segs
        .iter()
        .map(|seg| middle - seg.lo)
        .fold(f32::NEG_INFINITY, f32::max);
    Some((members, voiced.unwrap_or(below > above)))
}

/// The accidentals of one staff's column, placed together by their ink (each
/// glyph's box), with `x` from the column's: each stands `ACCIDENTAL_GAP`
/// left of every head of the column and of every ledger line its height
/// spans, and `ACCIDENTAL_STACK_GAP` clear of every accidental placed before
/// it that it would otherwise touch, a column further out each time. They are
/// placed from the outside in (the highest, the lowest, the next highest, and
/// so on), so the highest stands nearest the heads; a pitch's own stack stays
/// together, innermost nearest. Returns each head's accidental origins, and
/// the leftmost ink of the column's heads and accidentals.
fn place_accidentals(heads: &[&Head]) -> (Vec<Vec<f32>>, f32) {
    let ledger_half = STAFF_LINE_THICKNESS / 2.0;
    let mut heads_left = f32::INFINITY;
    // Each ledger line's left end and `y`.
    let mut ledgers: Vec<(f32, f32)> = Vec::new();
    for head in heads {
        let left = head.dx + metrics(head.name).map_or(0.0, |m| m.bounding_box().left.0);
        heads_left = heads_left.min(left);
        for step in ledger_steps(head.step) {
            let y = head.y + (step - head.step) as f32 * 0.5;
            ledgers.push((left - LEDGER_LINE_EXTENSION, y));
        }
    }
    if !heads_left.is_finite() {
        heads_left = 0.0;
    }
    let mut order: Vec<usize> = (0..heads.len())
        .filter(|i| !heads[*i].accidentals.is_empty())
        .collect();
    order.sort_by(|a, b| heads[*b].y.total_cmp(&heads[*a].y).then(a.cmp(b)));
    let mut outside_in = Vec::with_capacity(order.len());
    let (mut top, mut bottom) = (0, order.len());
    while top < bottom {
        outside_in.push(order[top]);
        top += 1;
        if top < bottom {
            bottom -= 1;
            outside_in.push(order[bottom]);
        }
    }
    // Each placed stack's box: left, bottom, right, top.
    let mut placed: Vec<[f32; 4]> = Vec::new();
    let mut origins = vec![Vec::new(); heads.len()];
    let mut leftmost = heads_left;
    for i in outside_in {
        let head = heads[i];
        let boxes: Vec<[f32; 4]> = head
            .accidentals
            .iter()
            .map(|name| {
                metrics(name).map_or([0.0, -1.0, 1.0, 1.0], |m| {
                    let b = m.bounding_box();
                    [b.left.0, b.bottom.0, b.right.0, b.top.0]
                })
            })
            .collect();
        let low = head.y + boxes.iter().map(|b| b[1]).fold(f32::INFINITY, f32::min);
        let high = head.y + boxes.iter().map(|b| b[3]).fold(f32::NEG_INFINITY, f32::max);
        let width = boxes.iter().map(|b| b[2] - b[0]).sum::<f32>()
            + boxes.len().saturating_sub(1) as f32 * ACCIDENTAL_STACK_GAP;
        let mut right = heads_left - ACCIDENTAL_GAP;
        for (end, y) in &ledgers {
            if y + ledger_half > low && y - ledger_half < high {
                right = right.min(end - ACCIDENTAL_GAP);
            }
        }
        // Step out past each placed stack it would touch.
        loop {
            let clash = placed
                .iter()
                .filter(|p| {
                    p[3] > low
                        && p[1] < high
                        && right - width < p[2] + ACCIDENTAL_STACK_GAP
                        && right > p[0] - ACCIDENTAL_STACK_GAP
                })
                .map(|p| p[0])
                .fold(f32::INFINITY, f32::min);
            if !clash.is_finite() {
                break;
            }
            right = clash - ACCIDENTAL_STACK_GAP;
        }
        placed.push([right - width, low, right, high]);
        leftmost = leftmost.min(right - width);
        let mut edge = right;
        for b in &boxes {
            let origin = edge - b[2];
            origins[i].push(origin);
            edge = origin + b[0] - ACCIDENTAL_STACK_GAP;
        }
    }
    (origins, leftmost)
}

/// Draws a notehead with what rides with it: the ledger lines it needs, its
/// accidental stack, and its augmentation dots. `provenance` is the head's own;
/// every other primitive is synthesized from its source.
fn emit_head(
    emit: &mut Emit<'_>,
    provenance: &Provenance,
    head: &Head,
    info: &ColumnInfo,
    yo: f32,
    band: VerticalBandId,
    staff: Option<StaffId>,
) {
    let x = info.x + head.dx;
    emit.glyph(
        provenance,
        head.name,
        Point::new(x, head.y),
        band,
        staff,
        info.slot,
    );
    // Ledger lines: short strokes continuing the staff to a notehead above or
    // below it, one per whole step between the staff and the note, reaching
    // `LEDGER_LINE_EXTENSION` past each side of *this notehead's* drawn box — so
    // a wider head (a whole note) gets a wider ledger. render-svg draws strokes
    // under glyphs at a layer, so the notehead sits over them.
    let head_box = metrics(head.name).map(|m| m.bounding_box());
    let head_left = head_box.map_or(0.0, |b| b.left.0);
    let head_right = head_box.map_or(NOTEHEAD_STEM_X, |b| b.right.0);
    for ledger_step in ledger_steps(head.step) {
        let y = step_to_y(yo, ledger_step);
        let ledger_provenance = Provenance::synthesized(
            provenance.source,
            SynthesisKind::Registered(LEDGER_LINE_SYNTHESIS),
            ledger_line_key(head.comp, ledger_step),
            provenance.dependencies.clone(),
        );
        emit.stroke(line_stroke(
            ledger_provenance,
            Point::new(x + head_left - LEDGER_LINE_EXTENSION, y),
            Point::new(x + head_right + LEDGER_LINE_EXTENSION, y),
            STAFF_LINE_THICKNESS,
            band,
        ));
    }
    // The spelling's accidental stack: synthesized glyphs left of the notehead
    // (innermost nearest it), where its column placed them, at its staff
    // position, sharing the notehead's column slot. Emitted *after* the
    // notehead so the slot's source x stays the notehead's.
    for (stack, (accidental, offset)) in head.accidentals.iter().zip(&head.accidental_x).enumerate()
    {
        let acc_provenance = Provenance::synthesized(
            provenance.source,
            SynthesisKind::Registered(ACCIDENTAL_SYNTHESIS),
            SynthesisInstanceKey((head.comp as u128) << 8 | stack as u128),
            provenance.dependencies.clone(),
        );
        let x = info.x + offset;
        emit.glyph(
            &acc_provenance,
            accidental,
            Point::new(x, head.y),
            band,
            staff,
            info.slot,
        );
    }
    // Augmentation dots, right of every head of the column, in the space its
    // dot `y` names.
    for dot in 0..head.dots {
        let dot_provenance = Provenance::synthesized(
            provenance.source,
            SynthesisKind::Registered(DOT_SYNTHESIS),
            SynthesisInstanceKey((head.comp as u128) << 8 | u128::from(dot)),
            provenance.dependencies.clone(),
        );
        let x = info.x + head.dot_x + f32::from(dot) * DOT_STEP;
        emit.glyph(
            &dot_provenance,
            "augmentationDot",
            Point::new(x, head.dot_y),
            band,
            staff,
            info.slot,
        );
    }
}

/// A musical time shifted by a component offset (a wall-clock base has no musical
/// offset, so it is unchanged).
fn shift_time(base: &TimePoint, offset: &MusicalDuration) -> TimePoint {
    match base {
        TimePoint::Musical(position) => TimePoint::Musical(position.clone() + offset.clone()),
        TimePoint::WallClock(time) => TimePoint::WallClock(*time),
    }
}

/// The clef in force at `at`, by **resolved time, not vector order**.
///
/// **Model (Minimal tier):** a staff's *initial* clef — the earliest-timed change
/// — applies from the staff start, even at positions before its own anchor; a
/// later change takes effect from its anchor onward. So the clef at `at` is the
/// change with the greatest time at or before `at`, else the earliest-timed
/// change (the initial clef), else treble when none is declared. This treats the
/// declared clefs as the staff's clef *plan* rather than "treble until the first
/// anchor"; in practice a score's first clef is anchored at the start, so the two
/// readings coincide, and the lead clef glyph uses this same query so it always
/// agrees with the notes. The sequence is not assumed sorted: `[bass@1, treble@0]`
/// resolves a note after time 1 to bass and one before to treble.
pub fn active_clef(clefs: &[PlacedClef], at: &TimePoint) -> Clef {
    active_clef_or(clefs, at, Clef::default())
}

/// The clef in force at `at`, falling back to `default` — the staff's own
/// `Staff::default_clef` — when the sequence names none. A staff that declares
/// its clef only on the `Staff` (no `ClefChange` at all) engraves in that clef;
/// `active_clef` is this with the treble default, kept for callers that have no
/// staff to hand.
pub fn active_clef_or(clefs: &[PlacedClef], at: &TimePoint, default: Clef) -> Clef {
    clefs
        .iter()
        .filter(|placed| {
            matches!(
                time_cmp(&placed.time, at),
                Some(Ordering::Less | Ordering::Equal)
            )
        })
        .max_by(|a, b| time_total(&a.time, &b.time))
        .or_else(|| clefs.iter().min_by(|a, b| time_total(&a.time, &b.time)))
        .map(|p| p.clef)
        .unwrap_or(default)
}

/// The musical origin (the active-clef query for an unanchored pitch).
fn origin() -> TimePoint {
    TimePoint::Musical(epiphany_core::MusicalPosition::origin())
}

/// A total order over column times: exact within a kind, musical before
/// wall-clock across kinds (a region is single-kind in practice).
fn time_total(a: &TimePoint, b: &TimePoint) -> Ordering {
    time_cmp(a, b).unwrap_or(match (a, b) {
        (TimePoint::Musical(_), TimePoint::WallClock(_)) => Ordering::Less,
        (TimePoint::WallClock(_), TimePoint::Musical(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    })
}

/// The visual `y` (staff spaces) of a [`StaffStep`] above a staff whose bottom
/// line is at `y_origin`: each step is half a staff space, `+y` up.
fn step_to_y(y_origin: f32, step: StaffStep) -> f32 {
    y_origin + step as f32 * 0.5
}

/// The staff steps at which a note at `step` needs ledger lines: the even steps
/// strictly outside the five-line staff (whose lines are the even steps `0..=8`),
/// from the staff out to the note. Empty when the note is on or within the staff,
/// or one space just outside it (an odd step at `±1` from the nearest line). At
/// most one of the two loops runs, since a step cannot be both above and below.
fn ledger_steps(step: StaffStep) -> Vec<StaffStep> {
    let mut steps = Vec::new();
    let mut above = 10;
    while above <= step {
        steps.push(above);
        above += 2;
    }
    let mut below = -2;
    while below >= step {
        steps.push(below);
        below -= 2;
    }
    steps
}

/// A distinct synthesis key for a ledger line on component `comp` at diatonic
/// `step`. The component occupies the high 64 bits and the signed step the low 64,
/// so the two fields never overlap — including a step below `-128`, whose
/// two's-complement low bits would otherwise reach into the component field and let
/// two components of a very low pitch mint colliding stable ids.
fn ledger_line_key(comp: usize, step: StaffStep) -> SynthesisInstanceKey {
    SynthesisInstanceKey(((comp as u128) << 64) | (step as i64 as u64 as u128))
}

/// Whether a stroke must keep a **fixed width** when a solver resolves horizontal
/// spacing: its length is a glyph-relative constant, not a span across the columns
/// the spacing pass stretches. A solver should translate such a stroke (preserving
/// its length) rather than re-map both endpoints, which would scale it. Ledger lines
/// are the case today — a fixed-width mark centered on one notehead, unlike a staff
/// line or barline that genuinely spans the system. Public so the constraint solver
/// can honor it without hard-coding the ledger synthesis identity.
pub fn is_rigid_width_stroke(stroke: &Stroke) -> bool {
    matches!(
        stroke.provenance.synthesis,
        Some(SynthesisKind::Registered(k)) if k == LEDGER_LINE_SYNTHESIS
    )
}

/// Whether a stroke is a beam (or a beam's hook). Its provenance names the
/// notes it joins among its dependencies.
pub fn is_beam_stroke(stroke: &Stroke) -> bool {
    matches!(
        stroke.provenance.synthesis,
        Some(SynthesisKind::Registered(k)) if k == BEAM_SYNTHESIS
    )
}

/// The staff step of a clef's reference line — a neutral fallback position for a
/// pitch whose spelling does not resolve to a CMN nominal.
fn reference_step(clef: &Clef) -> StaffStep {
    (clef.line as i32 - 1) * 2
}

/// The staff step of a spelled pitch under `clef`, and whether it is a fallback:
/// the clef reference line when the spelling is absent or non-CMN (its diatonic
/// position is unknown), which the caller surfaces as a diagnostic.
fn spelling_step(spelling: &Option<PitchSpelling>, clef: &Clef) -> (StaffStep, bool) {
    match spelling {
        Some(s) => match s.nominal {
            SpellingNominal::Cmn(nominal) => (staff_position(nominal, s.octave, clef), false),
            _ => (reference_step(clef), true),
        },
        None => (reference_step(clef), true),
    }
}

/// The bundled glyphs for a spelling's full accidental stack (innermost first),
/// in stack order. An accidental the bundled metrics do not carry (a microtonal
/// one) is surfaced as a diagnostic rather than drawn at a guessed shape and
/// omitted from the result; the notehead is still drawn at its
/// (accidental-independent) staff position. v0 draws exactly what the spelling
/// carries — it does not yet apply CMN accidental-state suppression (a repeated
/// sharp in a bar is shown again).
fn pitch_accidentals(
    spelling: &Option<PitchSpelling>,
    pitch: PitchId,
    diagnostics: &mut Vec<LayoutDiagnostic>,
) -> Vec<&'static str> {
    let Some(spelling) = spelling else {
        return Vec::new();
    };
    let mut glyphs = Vec::new();
    for accidental in &spelling.accidentals {
        match accidental_glyph(accidental) {
            Some(name) => glyphs.push(name),
            None => {
                diagnostics.push(LayoutDiagnostic {
                    source: TypedObjectId::Pitch(pitch),
                    kind: LayoutDiagnosticKind::UnbundledGlyph(GlyphReference::owned(
                        accidental.as_str(),
                    )),
                });
            }
        }
    }
    glyphs
}

/// A label for an unbundled clef shape, for its diagnostic.
fn clef_label(shape: epiphany_core::ClefShape) -> &'static str {
    match shape {
        epiphany_core::ClefShape::Percussion => "percussionClef",
        epiphany_core::ClefShape::G => "gClef",
        epiphany_core::ClefShape::F => "fClef",
        epiphany_core::ClefShape::C => "cClef",
    }
}

fn rest_label() -> &'static str {
    "rest (unbundled value)"
}

/// An `UnbundledGlyph` diagnostic kind for a glyph name.
fn unbundled(name: &'static str) -> LayoutDiagnosticKind {
    LayoutDiagnosticKind::UnbundledGlyph(GlyphReference::borrowed(name))
}

/// The synthesis instance key for an upper staff line: distinct per manifestation
/// (so a staff manifested in two regions does not collide) and per line index.
fn staff_line_key(manifestation: LayoutObjectId, line: u32) -> SynthesisInstanceKey {
    SynthesisInstanceKey((manifestation.0 << 3) | line as u128)
}

/// A deterministic spring-slot id for a region's column (by rank), distinct
/// across regions.
fn column_slot_id(region: LayoutObjectId, rank: usize) -> SpringSlotId {
    let mut preimage = Preimage::new(DomainTag::CONFLICT);
    preimage.push_bytes(b"layout/column-slot");
    preimage.push_u64_le((region.0 >> 64) as u64);
    preimage.push_u64_le(region.0 as u64);
    preimage.push_u64_le(rank as u64);
    SpringSlotId(preimage.finish_trunc128())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logical::to_logical;
    use crate::time_axis::TimeAxis;
    use epiphany_core::generators::valid_score_rich;
    use std::collections::BTreeSet;

    #[test]
    fn constraint_strength_attaches_by_rule() {
        // The spec's constraint enum carries no strength field, so strength is
        // a rule over the constraint's own shape (Chapter 9 §"Strength Levels").
        let glyph = GlyphObjectId(1);
        let slot = SpringSlotId(2);
        let required = [
            LayoutConstraint::NoCollision { a: glyph, b: glyph },
            LayoutConstraint::Align {
                a: glyph,
                b: glyph,
                axis: Axis::Vertical,
            },
            LayoutConstraint::PositionWithin {
                glyph,
                region: Rect {
                    origin: Point::ORIGIN,
                    size: Size2D::default(),
                },
            },
            LayoutConstraint::SystemBreakAt {
                slot,
                kind: BreakKind::Hard,
            },
            LayoutConstraint::PageBreakAt {
                slot,
                kind: BreakKind::Hard,
            },
            // Conservative: an unverifiable extension obligation is never demoted.
            LayoutConstraint::Registered(ConstraintRegistryId(3), ConstraintParameters::default()),
        ];
        for constraint in required {
            assert_eq!(constraint.strength(), ConstraintStrength::Required);
        }
        // A soft break is a preference at the default weight.
        for soft in [
            LayoutConstraint::SystemBreakAt {
                slot,
                kind: BreakKind::Soft,
            },
            LayoutConstraint::PageBreakAt {
                slot,
                kind: BreakKind::Soft,
            },
        ] {
            assert_eq!(
                soft.strength(),
                ConstraintStrength::Preferred { weight: 1.0 }
            );
        }
    }

    #[test]
    fn constraint_emission_is_deterministic_and_principled() {
        let logical = to_logical(&valid_score_rich(11));
        let a = try_to_constrained(&logical).expect("well-formed logical IR");
        let b = try_to_constrained(&logical).expect("well-formed logical IR");
        assert_eq!(
            a.constraints, b.constraints,
            "two runs emit identical constraint vectors"
        );
        assert!(!a.constraints.is_empty(), "the pipeline emits constraints");
        // Every constraint references real glyphs/slots and finite geometry.
        assert!(a.validate().is_ok());

        // Containment: one PositionWithin per glyph, against its region envelope.
        let contained = a
            .constraints
            .iter()
            .filter(|c| matches!(c, LayoutConstraint::PositionWithin { .. }))
            .count();
        assert_eq!(contained, a.glyphs.len());

        // No-collision: a linear chain over successive notehead columns, never
        // the O(n²) all-pairs closure.
        let pairs = a
            .constraints
            .iter()
            .filter(|c| matches!(c, LayoutConstraint::NoCollision { .. }))
            .count();
        let noteheads = a
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("notehead"))
            .count();
        assert!(pairs > 0, "successive noteheads earn no-collision pairs");
        assert!(pairs < noteheads, "the chain is linear in the noteheads");
        // Every no-collision endpoint is a notehead, the pair in two column
        // slots or set apart within one.
        let by_id: BTreeMap<GlyphObjectId, &GlyphObject> =
            a.glyphs.iter().map(|g| (g.id(), g)).collect();
        for constraint in &a.constraints {
            if let LayoutConstraint::NoCollision {
                a: first,
                b: second,
            } = constraint
            {
                let (first, second) = (by_id[first], by_id[second]);
                assert!(first.glyph.as_str().starts_with("notehead"));
                assert!(second.glyph.as_str().starts_with("notehead"));
                let apart = first.baseline.x.0 + first.bounding_box.right.0
                    <= second.baseline.x.0 + second.bounding_box.left.0
                    || second.baseline.x.0 + second.bounding_box.right.0
                        <= first.baseline.x.0 + first.bounding_box.left.0;
                assert!(first.horizontal_slot != second.horizontal_slot || apart);
            }
        }
        // No break constraints without projected break overrides.
        assert!(!a.constraints.iter().any(|c| matches!(
            c,
            LayoutConstraint::SystemBreakAt { .. } | LayoutConstraint::PageBreakAt { .. }
        )));
    }

    #[test]
    fn user_break_overrides_become_soft_break_constraints() {
        use epiphany_core::generators::valid_score;
        use epiphany_core::{AnchorOffset, Event, RegionEdge, TimeAnchor};

        let mut score = valid_score(3);
        let region_id = score.canvas.regions[0].id;
        // A pitched event in region 0 whose onset column is realized (it draws
        // noteheads), so the break has a spring slot to land on.
        let event = score.canvas.regions[0]
            .staff_instances()
            .iter()
            .flat_map(|si| si.voices.iter())
            .flat_map(|voice| voice.events.iter().copied())
            .find(|eid| {
                matches!(score.events.get(*eid), Some(Event::Pitched(p)) if !p.pitches.is_empty())
            })
            .expect("valid_score has a pitched event");
        let anchor = TimeAnchor::Event {
            id: event,
            offset: AnchorOffset::Zero,
        };
        let content = score.canvas.regions[0]
            .content
            .staff_based_mut()
            .expect("valid_score is staff based");
        content.user_system_breaks.push(anchor.clone());
        content.user_page_breaks.push(anchor);
        // An anchor no spacing column represents — a region edge — is skipped
        // silently rather than mis-assigned to some column.
        content.user_system_breaks.push(TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        });

        let constrained = to_constrained(&to_logical(&score));
        assert!(constrained.validate().is_ok());
        let system_breaks: Vec<&LayoutConstraint> = constrained
            .constraints
            .iter()
            .filter(|c| matches!(c, LayoutConstraint::SystemBreakAt { .. }))
            .collect();
        let page_breaks: Vec<&LayoutConstraint> = constrained
            .constraints
            .iter()
            .filter(|c| matches!(c, LayoutConstraint::PageBreakAt { .. }))
            .collect();
        assert_eq!(
            system_breaks.len(),
            1,
            "the event-anchored break lands; the region-edge one is skipped"
        );
        assert_eq!(page_breaks.len(), 1);
        for projected in system_breaks.iter().chain(&page_breaks) {
            let (LayoutConstraint::SystemBreakAt { slot, kind }
            | LayoutConstraint::PageBreakAt { slot, kind }) = projected
            else {
                unreachable!("filtered to break constraints");
            };
            // A Soft override projects a Soft break — a Preferred obligation.
            assert_eq!(*kind, BreakKind::Soft);
            assert_eq!(
                projected.strength(),
                ConstraintStrength::Preferred { weight: 1.0 }
            );
            // The slot is the event's own (realized) onset column.
            let slot = constrained
                .horizontal_slots
                .iter()
                .find(|s| s.id == *slot)
                .expect("break constraints name realized slots");
            assert!(!slot.members.is_empty());
        }
    }

    #[test]
    fn ledger_steps_cover_only_lines_outside_the_staff() {
        // Within the five-line staff (steps 0..=8) and one space just outside: none.
        for step in [-1, 0, 4, 8, 9] {
            assert!(ledger_steps(step).is_empty(), "no ledger at step {step}");
        }
        // First ledger above (step 10) and below (-2): exactly one line, on the note.
        assert_eq!(ledger_steps(10), vec![10]);
        assert_eq!(ledger_steps(-2), vec![-2]);
        // A note in the space above the first ledger still needs just that line.
        assert_eq!(ledger_steps(11), vec![10]);
        // Two lines above / below, in order from the staff outward.
        assert_eq!(ledger_steps(12), vec![10, 12]);
        assert_eq!(ledger_steps(-4), vec![-2, -4]);
    }

    #[test]
    fn ledger_line_keys_are_distinct_across_components_and_low_steps() {
        // The high/low 64-bit split keeps the component and step fields disjoint —
        // including a step below -128, whose two's-complement low bits are large and,
        // under a naive `(comp << small) | (step + bias)`, would reach into the
        // component field and collide with another component's key.
        let mut seen = std::collections::HashSet::new();
        for comp in 0..3usize {
            for step in [-300, -200, -130, -128, -2, 10, 200] {
                assert!(
                    seen.insert(ledger_line_key(comp, step).0),
                    "ledger key collision at comp={comp} step={step}"
                );
            }
        }
    }

    #[test]
    fn a_ledger_line_spans_its_notehead() {
        // The ledger reaches past the notehead on both sides, for any notehead width
        // — a whole note's head is wider than a black head, and the ledger must use
        // the real bounding box, not a fixed notehead width.
        let mut checked = 0;
        for seed in 0..32 {
            let c = to_constrained(&to_logical(&valid_score_rich(seed)));
            for s in c.strokes.iter().filter(|s| is_rigid_width_stroke(s)) {
                let lo = s.from.x.0.min(s.to.x.0);
                let hi = s.from.x.0.max(s.to.x.0);
                // The owning notehead: same source, baseline within the stroke span.
                if let Some(g) = c.glyphs.iter().find(|g| {
                    g.provenance.source == s.provenance.source
                        && g.baseline.x.0 >= lo
                        && g.baseline.x.0 <= hi
                }) {
                    let head_left = g.baseline.x.0 + g.bounding_box.left.0;
                    let head_right = g.baseline.x.0 + g.bounding_box.right.0;
                    assert!(
                        lo <= head_left + 1e-4 && hi >= head_right - 1e-4,
                        "seed {seed}: ledger [{lo}, {hi}] does not span notehead [{head_left}, {head_right}]"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "no ledger/notehead pairs to check");
    }

    #[test]
    fn a_note_far_above_the_staff_gets_ledger_strokes() {
        // A constrained layout of the rich corpus has noteheads across the range; at
        // least one sits far enough off the staff to earn a ledger line, emitted as a
        // synthesized stroke sourced from its pitch.
        let mut any_ledger = false;
        for seed in 0..16 {
            let c = to_constrained(&to_logical(&valid_score_rich(seed)));
            if c.strokes.iter().any(|s| {
                matches!(
                    s.provenance.synthesis,
                    Some(SynthesisKind::Registered(k)) if k == LEDGER_LINE_SYNTHESIS
                )
            }) {
                any_ledger = true;
                break;
            }
        }
        assert!(
            any_ledger,
            "no ledger-line strokes across 16 rich-corpus seeds"
        );
    }

    /// Every stroke and curve names a band that exists. A vertical solver reads
    /// that band to find a primitive's owning staff, so a dangling reference
    /// would silently drop the primitive out of the solve — it would keep its
    /// source y while its staff moved, tearing off its notes.
    ///
    /// Two bands exist precisely so nothing dangles. A staff band is emitted for
    /// every staff of the region, not only those that emitted a glyph (a staff
    /// whose clef is unbundled engraves to an anchor *stroke* and no glyph). The
    /// margin band is emitted unconditionally, because a region's own traced
    /// anchor is a stroke that names it whether or not a margin glyph does.
    #[test]
    fn every_stroke_and_curve_names_a_band_that_exists() {
        use epiphany_core::generators::valid_score;
        for (label, score) in [
            ("valid_score_rich", valid_score_rich(11)),
            ("valid_score", valid_score(4)),
        ] {
            let c = to_constrained(&to_logical(&score));
            let bands: BTreeSet<VerticalBandId> = c.vertical_bands.iter().map(|b| b.id).collect();
            assert!(!bands.is_empty(), "{label}: the projection emits bands");
            for stroke in &c.strokes {
                assert!(
                    bands.contains(&stroke.vertical_band),
                    "{label}: stroke {:?} names a band that does not exist",
                    stroke.id()
                );
            }
            for curve in &c.curves {
                assert!(
                    bands.contains(&curve.vertical_band),
                    "{label}: curve {:?} names a band that does not exist",
                    curve.id()
                );
            }
            // The region's own anchor stroke is staff-less, so a margin band must
            // exist even when no region-level *glyph* put a member in it.
            assert!(
                c.vertical_bands
                    .iter()
                    .any(|b| b.kind == crate::VerticalBandKind::MarginBand),
                "{label}: a margin band exists for the region-level anchor"
            );
            assert!(c.validate().is_ok(), "{label}: the projection validates");
        }
    }

    #[test]
    fn out_of_range_finite_stroke_thickness_is_rejected() {
        let mut c = to_constrained(&to_logical(&valid_score_rich(11)));
        let provenance = c.glyphs[0].provenance.clone();
        let band = c.glyphs[0].vertical_band;
        c.strokes.push(Stroke {
            provenance,
            vertical_band: band,
            from: Point::new(0.0, 0.0),
            to: Point::new(1.0, 0.0),
            // Finite but far outside the canonical 1/1024 grid range: it passes a
            // bare finite/non-negative check yet would panic in `canonical_bytes`.
            thickness: StaffSpace(f32::MAX),
            layer: 0,
            style: GlyphStyle::default(),
        });
        assert!(
            matches!(
                c.validate(),
                Err(ConstrainedValidationError::InvalidStrokeGeometry(_))
            ),
            "a finite-but-out-of-range stroke thickness is rejected"
        );
    }

    /// The contract the engraver's coordinate remap relies on: every spring slot
    /// has a member glyph (a source x). An externally-built IR with an empty slot
    /// — valid in every other respect — is rejected, not silently accepted to be
    /// hit later as a "target with no source" remap hole.
    #[test]
    fn an_empty_spring_slot_is_rejected() {
        let mut c = to_constrained(&to_logical(&valid_score_rich(11)));
        assert!(c.validate().is_ok());
        c.horizontal_slots.push(SpringSlot {
            id: SpringSlotId(0xDEAD_BEEF),
            time: TimePoint::WallClock(WallClockTime(0)),
            min_width: StaffSpace(1.0),
            preferred_width: StaffSpace(1.5),
            max_width: None,
            stretch_factor: 1.0,
            compress_factor: 1.0,
            members: vec![],
        });
        assert!(
            matches!(c.validate(), Err(ConstrainedValidationError::EmptySlot(_))),
            "an empty spring slot is rejected"
        );
    }

    /// The direct logical-IR way to reach the empty-slot shape: a pitched event
    /// with no pitches marks a column but emits only a stem (no notehead). The
    /// column must stay slot-less, so `to_constrained`'s own output validates.
    #[test]
    fn pitchless_note_produces_no_empty_slot() {
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent};
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{EventId, MusicalPosition, RegionId, StaffId};

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let note = LayoutContent::Note(NoteContent {
            voice: crate::logical::VoicePlace::Alone,
            position: TimePoint::Musical(MusicalPosition::origin()),
            components: vec![],
            pitches: vec![],
        });
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![LayoutObject::from_projection_with_content(
                    Provenance::manifested(
                        TypedObjectId::Event(EventId::from_raw(1)),
                        region,
                        vec![],
                    ),
                    Some(staff),
                    note,
                )],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);
        assert!(
            c.validate().is_ok(),
            "to_constrained must not produce an empty slot from a pitchless note"
        );
        assert!(c.horizontal_slots.iter().all(|s| !s.members.is_empty()));
        // The event is still covered — by its (zero-length) stem anchor.
        assert!(c
            .strokes
            .iter()
            .any(|s| s.provenance.source == TypedObjectId::Event(EventId::from_raw(1))));
    }

    /// The fallible public conversion must not panic when externally built
    /// logical IR pairs a source kind with non-matching content. Pass 2 has
    /// explicit fallbacks for these cases (default treble clef / final barline),
    /// so pass 1 must collect the columns those fallbacks use.
    #[test]
    fn source_content_mismatches_use_fallback_columns() {
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR};
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{MeasureId, RegionId, StaffId, StaffInstanceId};

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let staff_instance = StaffInstanceId::from_raw(20);
        let measure = MeasureId::from_raw(30);
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    LayoutObject::from_projection_with_content(
                        Provenance::manifested(
                            TypedObjectId::StaffInstance(staff_instance),
                            region,
                            vec![],
                        ),
                        Some(staff),
                        LayoutContent::Structural,
                    ),
                    LayoutObject::from_projection_with_content(
                        Provenance::manifested(TypedObjectId::Measure(measure), region, vec![]),
                        Some(staff),
                        LayoutContent::Structural,
                    ),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };

        let c = try_to_constrained(&logical)
            .expect("mismatched public logical IR should use fallback columns");
        assert!(c.validate().is_ok());
        assert!(c.glyphs.iter().any(|g| {
            g.provenance.source == TypedObjectId::StaffInstance(staff_instance)
                && g.glyph.as_str() == "gClef"
        }));
        assert!(c.glyphs.iter().any(|g| {
            g.provenance.source == TypedObjectId::Measure(measure)
                && g.glyph.as_str() == "barlineFinal"
        }));
    }

    /// A spelling's full accidental *stack* draws — every element, innermost
    /// nearest the notehead — not just the first, with distinct synthesized ids.
    #[test]
    fn a_stacked_accidental_draws_every_element() {
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch};
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            AccidentalId, CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling, RegionId,
            StaffId,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch = PitchId::from_raw(100);
        let mut spelling = PitchSpelling::cmn(CmnNominal::C, 5);
        // Innermost (nearest the notehead) first, then an outer element.
        spelling.accidentals.push(AccidentalId::new("sharp"));
        spelling.accidentals.push(AccidentalId::new("flat"));
        let note = LayoutContent::Note(NoteContent {
            voice: crate::logical::VoicePlace::Alone,
            position: TimePoint::Musical(MusicalPosition::origin()),
            components: vec![],
            pitches: vec![NotePitch {
                pitch,
                spelling: Some(spelling),
            }],
        });
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    manifested(TypedObjectId::Event(EventId::from_raw(1)), note),
                    manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);

        let accidentals: Vec<_> = c
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("accidental"))
            .collect();
        assert_eq!(accidentals.len(), 2, "both stack elements are drawn");
        let sharp = accidentals
            .iter()
            .find(|g| g.glyph.as_str() == "accidentalSharp")
            .expect("innermost sharp drawn");
        let flat = accidentals
            .iter()
            .find(|g| g.glyph.as_str() == "accidentalFlat")
            .expect("outer flat drawn");
        // The innermost (sharp, stack index 0) sits nearer the notehead.
        assert!(
            sharp.baseline.x.0 > flat.baseline.x.0,
            "the innermost accidental is nearer the notehead than the outer"
        );
        assert_ne!(sharp.provenance.stable_id, flat.provenance.stable_id);
        assert!(accidentals
            .iter()
            .all(|g| g.provenance.source == TypedObjectId::Pitch(pitch)));
        assert!(c.validate().is_ok());
    }

    /// A pitch whose spelling carries an accidental draws it as a synthesized
    /// glyph just left of the notehead, at the same staff position, sharing the
    /// notehead's column slot — the notehead keeps the pitch's exact provenance.
    #[test]
    fn a_spelled_accidental_draws_left_of_its_notehead() {
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch};
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            AccidentalId, CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling, RegionId,
            StaffId,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch = PitchId::from_raw(100);
        let mut spelling = PitchSpelling::cmn(CmnNominal::C, 5);
        spelling.accidentals.push(AccidentalId::new("sharp"));
        let note = LayoutContent::Note(NoteContent {
            voice: crate::logical::VoicePlace::Alone,
            position: TimePoint::Musical(MusicalPosition::origin()),
            components: vec![],
            pitches: vec![NotePitch {
                pitch,
                spelling: Some(spelling),
            }],
        });
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    manifested(TypedObjectId::Event(EventId::from_raw(1)), note),
                    manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);

        let notehead = c
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str().starts_with("notehead"))
            .expect("a notehead is drawn");
        let accidental = c
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str() == "accidentalSharp")
            .expect("the sharp accidental is drawn");
        // The notehead carries the pitch's exact provenance; the accidental is a
        // distinct, synthesized glyph from the same source.
        assert!(notehead.provenance.synthesis.is_none());
        assert_eq!(notehead.provenance.source, TypedObjectId::Pitch(pitch));
        assert!(accidental.provenance.synthesis.is_some());
        assert_eq!(accidental.provenance.source, TypedObjectId::Pitch(pitch));
        assert_ne!(
            accidental.provenance.stable_id,
            notehead.provenance.stable_id
        );
        // Left of the notehead, same staff position, same column slot.
        assert!(accidental.baseline.x.0 < notehead.baseline.x.0);
        assert_eq!(accidental.baseline.y, notehead.baseline.y);
        assert_eq!(accidental.horizontal_slot, notehead.horizontal_slot);
        // The accidental is a proper slot/band member — the IR validates.
        assert!(c.validate().is_ok());
    }

    /// A tie arcs between the heads it joins, riding their slots: in a chord
    /// the upper tie arcs above and the lower below, the middle one away from
    /// the stem; the structure's provenance rides its first arc. A note whose
    /// value splits into tied components draws a tie between its own heads.
    #[test]
    fn ties_arc_between_their_heads_by_the_chords_rule() {
        use crate::logical::{
            LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch, PlacedComponent,
            TieContent,
        };
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            CmnNominal, EventId, MusicalPosition, NotatedComponent, PitchId, PitchSpelling,
            RationalTime, RegionId, StaffId, TieId,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let at = |n: i64| {
            TimePoint::Musical(
                MusicalPosition::origin() + MusicalDuration(RationalTime::new(n, 4).expect("n/4")),
            )
        };
        let component = |base_value, offset: i64, tied_to_next| PlacedComponent {
            offset: MusicalDuration(RationalTime::new(offset, 8).expect("n/8")),
            component: NotatedComponent {
                base_value,
                dots: 0,
                tuplet: None,
                tied_to_next,
            },
            tuplet: None,
        };
        // Two quarter chords, C5 E5 G5, each tied to the next.
        let steps = [CmnNominal::C, CmnNominal::E, CmnNominal::G];
        let mut objects = Vec::new();
        for (e, start) in [(1u128, 0i64), (2, 1)] {
            let pitches: Vec<NotePitch> = steps
                .iter()
                .enumerate()
                .map(|(i, nominal)| NotePitch {
                    pitch: PitchId::from_raw(e * 10 + i as u128),
                    spelling: Some(PitchSpelling::cmn(*nominal, 5)),
                })
                .collect();
            for pitch in &pitches {
                objects.push(manifested(
                    TypedObjectId::Pitch(pitch.pitch),
                    LayoutContent::Structural,
                ));
            }
            objects.push(manifested(
                TypedObjectId::Event(EventId::from_raw(e)),
                LayoutContent::Note(NoteContent {
                    voice: crate::logical::VoicePlace::Alone,
                    position: at(start),
                    components: vec![component(NoteValue::Quarter, 0, false)],
                    pitches,
                }),
            ));
        }
        let tie = TieId::from_raw(5);
        objects.push(manifested(
            TypedObjectId::Tie(tie),
            LayoutContent::Tie(TieContent {
                start: EventId::from_raw(1),
                end: EventId::from_raw(2),
                pairs: (0..3)
                    .map(|i| (PitchId::from_raw(10 + i), PitchId::from_raw(20 + i)))
                    .collect(),
            }),
        ));
        // A note on A4 whose value splits into a half tied to an eighth.
        let split = PitchId::from_raw(30);
        objects.push(manifested(
            TypedObjectId::Pitch(split),
            LayoutContent::Structural,
        ));
        objects.push(manifested(
            TypedObjectId::Event(EventId::from_raw(3)),
            LayoutContent::Note(NoteContent {
                voice: crate::logical::VoicePlace::Alone,
                position: at(2),
                components: vec![
                    component(NoteValue::Half, 0, true),
                    component(NoteValue::Eighth, 4, false),
                ],
                pitches: vec![NotePitch {
                    pitch: split,
                    spelling: Some(PitchSpelling::cmn(CmnNominal::A, 4)),
                }],
            }),
        ));
        let c = to_constrained(&LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects,
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        });
        assert!(c.validate().is_ok());
        let mut arcs: Vec<&Curve> = c
            .curves
            .iter()
            .filter(|curve| curve.provenance.source == TypedObjectId::Tie(tie))
            .collect();
        assert_eq!(arcs.len(), 3);
        assert_eq!(
            arcs.iter()
                .filter(|a| a.provenance.synthesis.is_none())
                .count(),
            1,
            "the tie's provenance rides one arc"
        );
        arcs.sort_by(|a, b| b.p0.y.0.total_cmp(&a.p0.y.0));
        // The chord sits above the middle line, so it stems down: the top and
        // middle ties arc above, the bottom one below.
        let above: Vec<bool> = arcs.iter().map(|a| a.p1.y.0 > a.p0.y.0).collect();
        assert_eq!(above, [true, true, false]);
        for arc in &arcs {
            assert!(arc.p3.x.0 > arc.p0.x.0);
            assert!(c
                .span_anchors
                .iter()
                .any(|anchor| anchor.primitive == arc.id()));
        }
        // The split note's tie, between its own two heads, under them (its
        // half's stem points up from below the middle line).
        let own: Vec<&Curve> = c
            .curves
            .iter()
            .filter(|curve| curve.provenance.source == TypedObjectId::Pitch(split))
            .collect();
        assert_eq!(own.len(), 1);
        assert!(own[0].provenance.synthesis.is_some());
        assert!(own[0].p1.y.0 < own[0].p0.y.0, "it arcs below");
        assert!(c
            .span_anchors
            .iter()
            .any(|anchor| anchor.primitive == own[0].id()));
    }

    /// A note's value reaches the page: an eighth or shorter takes its flag at
    /// the stem's tip, a 32nd's stem is lengthened to its flag's far edge, each
    /// augmentation dot sits right of the head in a space, and an unpitched note
    /// draws its head at its staff position with the event's own provenance.
    #[test]
    fn flags_and_dots_ride_their_notes() {
        use crate::logical::{
            LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch, PlacedComponent,
            UnpitchedContent,
        };
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            CmnNominal, EventId, MusicalPosition, NotatedComponent, PitchId, PitchSpelling,
            RegionId, StaffId, StaffPosition,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let component = |base_value, dots| PlacedComponent {
            offset: MusicalDuration::zero(),
            component: NotatedComponent {
                base_value,
                dots,
                tuplet: None,
                tied_to_next: false,
            },
            tuplet: None,
        };
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let solve = |objects: Vec<LayoutObject>| {
            to_constrained(&LogicalLayoutIR {
                source: ScoreVersion::default(),
                regions: vec![LayoutRegion {
                    provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                    coordinate_system: crate::LocalCoordinateSystem::default(),
                    time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                    vertical_extent: crate::VerticalExtent {
                        staves: vec![staff],
                    },
                    objects,
                }],
                engraving_decisions: vec![],
                overrides: vec![],
                cross_region: vec![],
            })
        };
        // A dotted eighth on the bottom line (E4 in the treble clef).
        let note = |value, dots| {
            let pitch = PitchId::from_raw(100);
            vec![
                manifested(
                    TypedObjectId::Event(EventId::from_raw(1)),
                    LayoutContent::Note(NoteContent {
                        voice: crate::logical::VoicePlace::Alone,
                        position: TimePoint::Musical(MusicalPosition::origin()),
                        components: vec![component(value, dots)],
                        pitches: vec![NotePitch {
                            pitch,
                            spelling: Some(PitchSpelling::cmn(CmnNominal::E, 4)),
                        }],
                    }),
                ),
                manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
            ]
        };
        let c = solve(note(NoteValue::Eighth, 1));
        let named = |c: &ConstrainedLayoutIR, name: &str| -> GlyphObject {
            c.glyphs
                .iter()
                .find(|g| g.glyph.as_str() == name)
                .cloned()
                .unwrap_or_else(|| panic!("{name} is drawn"))
        };
        let head = named(&c, "noteheadBlack");
        let flag = named(&c, "flag8thUp");
        let dot = named(&c, "augmentationDot");
        let stem = c
            .strokes
            .iter()
            .find(|s| s.provenance.source == TypedObjectId::Event(EventId::from_raw(1)))
            .expect("a stem");
        assert_eq!(
            flag.baseline.y, stem.to.y,
            "the flag hangs from the stem's tip"
        );
        assert!((flag.baseline.x.0 - (stem.to.x.0 - STEM_THICKNESS / 2.0)).abs() < 1e-6);
        assert_eq!(
            flag.provenance.source,
            TypedObjectId::Event(EventId::from_raw(1))
        );
        assert!(flag.provenance.synthesis.is_some());
        // The head is on a line, so its dot is in the space above.
        assert_eq!(dot.baseline.y.0, head.baseline.y.0 + 0.5);
        assert!(dot.baseline.x.0 > head.baseline.x.0 + head.bounding_box.right.0);
        assert_eq!(dot.provenance.source, head.provenance.source);
        for glyph in [&flag, &dot] {
            assert_eq!(glyph.horizontal_slot, head.horizontal_slot);
        }
        assert!(c.validate().is_ok());

        // A 32nd's stem reaches past the tip its flag hangs from.
        let c = solve(note(NoteValue::ThirtySecond, 0));
        let flag = named(&c, "flag32ndUp");
        let stem = c
            .strokes
            .iter()
            .find(|s| s.provenance.source == TypedObjectId::Event(EventId::from_raw(1)))
            .expect("a stem");
        assert!(stem.to.y.0 > flag.baseline.y.0);
        // A quarter takes no flag.
        let c = solve(note(NoteValue::Quarter, 0));
        assert!(c
            .glyphs
            .iter()
            .all(|g| !g.glyph.as_str().starts_with("flag")));

        // An unpitched sixteenth on the middle line: its head carries the
        // event's exact provenance, its stem and flag are synthesized.
        let event = EventId::from_raw(2);
        let c = solve(vec![manifested(
            TypedObjectId::Event(event),
            LayoutContent::Unpitched(UnpitchedContent {
                voice: crate::logical::VoicePlace::Alone,
                position: TimePoint::Musical(MusicalPosition::origin()),
                components: vec![component(NoteValue::Sixteenth, 0)],
                staff_position: StaffPosition(4),
            }),
        )]);
        let head = named(&c, "noteheadBlack");
        assert_eq!(head.provenance.source, TypedObjectId::Event(event));
        assert!(head.provenance.synthesis.is_none());
        assert_eq!(head.baseline.y.0, 2.0);
        let flag = named(&c, "flag16thDown");
        assert!(flag.provenance.synthesis.is_some());
        assert!(c
            .strokes
            .iter()
            .any(|s| s.provenance.source == TypedObjectId::Event(event)
                && s.provenance.synthesis.is_some()
                && s.from != s.to));
        assert!(c.validate().is_ok());
    }

    /// A key signature draws its sharp/flat zigzag in the lead area after the
    /// clef, each accidental a synthesized glyph at its clef-relative staff
    /// position, sharing the clef's column slot.
    #[test]
    fn a_key_signature_draws_its_accidentals_in_the_lead() {
        use crate::logical::{
            LayoutObject, LayoutRegion, LogicalLayoutIR, PlacedKeySignature, StaffContent,
        };
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{KeySignature, MusicalPosition, RegionId, StaffId, StaffInstanceId};

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let instance = StaffInstanceId::from_raw(1);
        // D major (two sharps), default treble clef.
        let content = LayoutContent::Staff(StaffContent {
            default_clef: Clef::default(),
            clefs: vec![],
            keys: vec![PlacedKeySignature {
                time: TimePoint::Musical(MusicalPosition::origin()),
                key: KeySignature::new(2).expect("two sharps is a valid key"),
            }],
            beams: Vec::new(),
        });
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![LayoutObject::from_projection_with_content(
                    Provenance::manifested(TypedObjectId::StaffInstance(instance), region, vec![]),
                    Some(staff),
                    content,
                )],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);

        let sharps: Vec<_> = c
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "accidentalSharp")
            .collect();
        assert_eq!(
            sharps.len(),
            2,
            "a two-sharp key signature draws two sharps"
        );
        // Synthesized from the staff instance, distinct ids.
        assert!(sharps.iter().all(|g| g.provenance.synthesis.is_some()));
        assert!(sharps
            .iter()
            .all(|g| g.provenance.source == TypedObjectId::StaffInstance(instance)));
        assert_ne!(
            sharps[0].provenance.stable_id,
            sharps[1].provenance.stable_id
        );
        // In the lead, right of the clef, left-to-right, sharing the clef's slot.
        let clef = c
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str() == "gClef")
            .expect("clef");
        assert!(sharps.iter().all(|g| g.baseline.x.0 > clef.baseline.x.0));
        assert!(sharps[0].baseline.x.0 < sharps[1].baseline.x.0);
        assert!(sharps
            .iter()
            .all(|g| g.horizontal_slot == clef.horizontal_slot));
        // The conventional treble placement: F♯ on the top line (step 8 → y 4),
        // C♯ in the third space (step 5 → y 2.5).
        assert_eq!(sharps[0].baseline.y.0, 4.0);
        assert_eq!(sharps[1].baseline.y.0, 2.5);
        assert!(c.validate().is_ok());
    }

    /// A measure that introduces a time signature draws a numerator-over-
    /// denominator digit pair at its start, in a column of its own before the
    /// barline that ends it, each digit synthesized from the measure. An
    /// unbundled digit is surfaced as a diagnostic, not drawn at a guessed
    /// shape.
    #[test]
    fn a_time_signature_draws_a_digit_pair_at_its_measures_start() {
        use crate::logical::{
            BarlineKind, LayoutObject, LayoutRegion, LogicalLayoutIR, MeasureContent,
            TimeSignatureContent,
        };
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{MeasureId, MusicalPosition, RegionId, StaffId};

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let measure = MeasureId::from_raw(7);
        let build = |numerator: u16, denominator: u16| {
            let content = LayoutContent::Measure(MeasureContent {
                start: TimePoint::Musical(MusicalPosition::origin()),
                end: Some(TimePoint::Musical(
                    MusicalPosition::origin()
                        + epiphany_core::MusicalDuration(epiphany_core::RationalTime::from_int(1)),
                )),
                barline: BarlineKind::Interior,
                time_signature: Some(TimeSignatureContent {
                    numerator,
                    denominator,
                }),
            });
            LogicalLayoutIR {
                source: ScoreVersion::default(),
                regions: vec![LayoutRegion {
                    provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                    coordinate_system: crate::LocalCoordinateSystem::default(),
                    time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                    vertical_extent: crate::VerticalExtent {
                        staves: vec![staff],
                    },
                    objects: vec![LayoutObject::from_projection_with_content(
                        Provenance::manifested(TypedObjectId::Measure(measure), region, vec![]),
                        Some(staff),
                        content,
                    )],
                }],
                engraving_decisions: vec![],
                overrides: vec![],
                cross_region: vec![],
            }
        };

        // 4/4: both digits are bundled, so two '4' glyphs are drawn.
        let c = to_constrained(&build(4, 4));
        let fours: Vec<_> = c
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "timeSig4")
            .collect();
        assert_eq!(fours.len(), 2, "4/4 draws two '4' digits");
        assert!(fours.iter().all(|g| g.provenance.synthesis.is_some()));
        assert!(fours
            .iter()
            .all(|g| g.provenance.source == TypedObjectId::Measure(measure)));
        assert_ne!(fours[0].provenance.stable_id, fours[1].provenance.stable_id);
        let barline = c
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str() == "barlineSingle")
            .expect("a barline is drawn");
        assert!(
            fours.iter().all(|g| g.baseline.x.0 < barline.baseline.x.0),
            "the signature opens the measure its barline ends"
        );
        assert!(fours
            .iter()
            .all(|g| g.horizontal_slot == fours[0].horizontal_slot
                && g.horizontal_slot != barline.horizontal_slot));
        // Numerator above the denominator (distinct vertical positions).
        let upper = fours
            .iter()
            .map(|g| g.baseline.y.0)
            .fold(f32::MIN, f32::max);
        let lower = fours
            .iter()
            .map(|g| g.baseline.y.0)
            .fold(f32::MAX, f32::min);
        assert!(upper > lower, "numerator sits above the denominator");
        assert!(c.validate().is_ok());
        assert!(c.diagnostics.is_empty(), "4/4 digits are all bundled");

        // 3/4: every digit (0–9) is now bundled, so both draw with no diagnostic.
        let c3 = to_constrained(&build(3, 4));
        assert!(c3.glyphs.iter().any(|g| g.glyph.as_str() == "timeSig3"));
        assert!(c3.glyphs.iter().any(|g| g.glyph.as_str() == "timeSig4"));
        assert!(
            c3.diagnostics.is_empty(),
            "all single-digit time-signature values are bundled"
        );

        // A two-digit number lays its digits out side by side (e.g. 12/8).
        let c12 = to_constrained(&build(12, 8));
        let ones = c12
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "timeSig1")
            .count();
        assert_eq!(ones, 1, "the '1' of 12 is drawn");
        assert!(c12.glyphs.iter().any(|g| g.glyph.as_str() == "timeSig2"));
        assert!(c12.glyphs.iter().any(|g| g.glyph.as_str() == "timeSig8"));
    }

    /// A note notated as a multi-component (tied) decomposition draws one
    /// notehead per component at its own offset — not a single notehead at the
    /// event start. The first component carries the pitch's exact provenance, the
    /// rest are synthesized from it.
    #[test]
    fn tied_decomposition_draws_a_notehead_per_component() {
        use crate::logical::{
            LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch, PlacedComponent,
        };
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            CmnNominal, EventId, MusicalPosition, NotatedComponent, PitchId, PitchSpelling,
            RationalTime, RegionId, StaffId,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch = PitchId::from_raw(100);
        let component = |base, num, den, tied| PlacedComponent {
            offset: MusicalDuration(RationalTime::new(num, den).unwrap()),
            component: NotatedComponent {
                base_value: base,
                dots: 0,
                tuplet: None,
                tied_to_next: tied,
            },
            tuplet: None,
        };
        // A quarter tied to an eighth: two components at offsets 0 and 1/4.
        let note = LayoutContent::Note(NoteContent {
            voice: crate::logical::VoicePlace::Alone,
            position: TimePoint::Musical(MusicalPosition::origin()),
            components: vec![
                component(NoteValue::Quarter, 0, 1, true),
                component(NoteValue::Eighth, 1, 4, false),
            ],
            pitches: vec![NotePitch {
                pitch,
                spelling: Some(PitchSpelling::cmn(CmnNominal::C, 4)),
            }],
        });
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    manifested(TypedObjectId::Event(EventId::from_raw(1)), note),
                    manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);
        let heads: Vec<_> = c
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("notehead"))
            .collect();
        assert_eq!(
            heads.len(),
            2,
            "two components → two noteheads (not collapsed)"
        );
        assert_ne!(
            heads[0].baseline.x, heads[1].baseline.x,
            "the second component sits at a later column (its offset is honored)"
        );
        // The pitch's exact source is on one notehead; the other is synthesized
        // from it — so the round-trip recovers the pitch once, no duplicate id.
        let exact = heads
            .iter()
            .filter(|g| g.provenance.synthesis.is_none())
            .count();
        let synth = heads
            .iter()
            .filter(|g| g.provenance.synthesis.is_some())
            .count();
        assert_eq!((exact, synth), (1, 1));
        assert!(heads
            .iter()
            .all(|g| g.provenance.source == TypedObjectId::Pitch(pitch)));
        // Two stems too — one per component (the event's, plus a synthesized one).
        let stems = c
            .strokes
            .iter()
            .filter(|s| s.provenance.source == TypedObjectId::Event(EventId::from_raw(1)))
            .count();
        assert_eq!(stems, 2, "one stem per component");
    }

    /// `active_clef` resolves by time, not vector order: an unsorted clef
    /// sequence still yields the latest change at or before the query.
    #[test]
    fn active_clef_resolves_by_time_not_vector_order() {
        use crate::logical::PlacedClef;
        use epiphany_core::{MusicalPosition, RationalTime};

        let at = |n, d| TimePoint::Musical(MusicalPosition(RationalTime::new(n, d).unwrap()));
        // Authored out of order: bass at time 1, treble at time 0.
        let clefs = vec![
            PlacedClef {
                time: at(1, 1),
                clef: Clef::bass(),
            },
            PlacedClef {
                time: at(0, 1),
                clef: Clef::treble(),
            },
        ];
        // After time 1 → bass (latest change ≤ query), not treble (last in vector).
        assert_eq!(active_clef(&clefs, &at(2, 1)), Clef::bass());
        // At time 1/2 → treble (the change at time 0).
        assert_eq!(active_clef(&clefs, &at(1, 2)), Clef::treble());
        // Before any change → the earliest-timed clef (treble@0).
        assert_eq!(active_clef(&clefs, &at(-1, 1)), Clef::treble());
        // Empty → default treble.
        assert_eq!(active_clef(&[], &at(0, 1)), Clef::default());
    }

    /// The displayed lead clef agrees with the notes' active clef: an unsorted
    /// `[bass@1, treble@0]` sequence draws a treble clef at the start (the clef in
    /// force at the staff start, by time), not bass (the vector-first entry).
    #[test]
    fn lead_clef_glyph_uses_time_order_not_vector_order() {
        use crate::logical::{
            LayoutObject, LayoutRegion, LogicalLayoutIR, PlacedClef, StaffContent,
        };
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{MusicalPosition, RationalTime, RegionId, StaffId, StaffInstanceId};
        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let at = |n, d| TimePoint::Musical(MusicalPosition(RationalTime::new(n, d).unwrap()));
        let content = LayoutContent::Staff(StaffContent {
            default_clef: Clef::default(),
            // Authored out of order: bass at time 1, treble at time 0.
            clefs: vec![
                PlacedClef {
                    time: at(1, 1),
                    clef: Clef::bass(),
                },
                PlacedClef {
                    time: at(0, 1),
                    clef: Clef::treble(),
                },
            ],
            keys: vec![],
            beams: Vec::new(),
        });
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![LayoutObject::from_projection_with_content(
                    Provenance::manifested(
                        TypedObjectId::StaffInstance(StaffInstanceId::from_raw(1)),
                        region,
                        vec![],
                    ),
                    Some(staff),
                    content,
                )],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);
        let clef = c
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str().ends_with("Clef"))
            .expect("a clef glyph is drawn");
        assert_eq!(
            clef.glyph.as_str(),
            "gClef",
            "the lead clef is the treble in force at the start, not the vector-first bass"
        );
    }

    /// A hidden rest is a traced anchor at its *own onset column*, not a
    /// default x, and every component is kept (later ones do not vanish); it
    /// draws no glyph and raises no diagnostic.
    #[test]
    fn hidden_rest_components_anchor_at_their_onset() {
        use crate::logical::{
            LayoutObject, LayoutRegion, LogicalLayoutIR, PlacedComponent, RestContent,
        };
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            EventId, MusicalPosition, NotatedComponent, RationalTime, RegionId, StaffId,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let eid = EventId::from_raw(1);
        let component = |num, den| PlacedComponent {
            offset: MusicalDuration(RationalTime::new(num, den).unwrap()),
            component: NotatedComponent {
                base_value: NoteValue::Sixteenth,
                dots: 0,
                tuplet: None,
                tied_to_next: false,
            },
            tuplet: None,
        };
        let rest = LayoutContent::Rest(RestContent {
            voice: crate::logical::VoicePlace::Alone,
            position: TimePoint::Musical(MusicalPosition::origin()),
            components: vec![component(0, 1), component(1, 16)],
            staff_position: None,
            visible: false,
            whole_measure: false,
        });
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![LayoutObject::from_projection_with_content(
                    Provenance::manifested(TypedObjectId::Event(eid), region, vec![]),
                    Some(staff),
                    rest,
                )],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);

        // No rest glyph (the rest is hidden), and nothing to surface.
        assert!(c
            .glyphs
            .iter()
            .all(|g| !g.glyph.as_str().starts_with("rest")));
        assert!(c.diagnostics.is_empty(), "a hidden rest is not a gap");
        // Both components are kept, anchored at distinct onset columns — not piled
        // at a default x.
        let anchors: Vec<_> = c
            .strokes
            .iter()
            .filter(|s| s.provenance.source == TypedObjectId::Event(eid))
            .collect();
        assert_eq!(anchors.len(), 2, "no component vanishes");
        assert_ne!(
            anchors[0].from.x, anchors[1].from.x,
            "components anchor at their distinct onset columns"
        );
        assert!(
            anchors.iter().all(|a| a.from.x.0 >= FIRST_COLUMN_X),
            "a hidden rest anchors at its onset column, not the default x"
        );
        // Stroke-only columns earn no spring slot: this region has no glyphs, so
        // no slots — the engraver's remap never faces an empty slot with no
        // source→target point.
        assert!(
            c.horizontal_slots.is_empty(),
            "a stroke-only (hidden-rest) column creates no spring slot"
        );

        // Shown, the same rest draws a sixteenth rest at each onset, and a
        // dotted eighth draws its rest and dot in one column.
        let shown = |components: Vec<PlacedComponent>, whole_measure: bool| {
            let mut logical = logical.clone();
            logical.regions[0].objects = vec![LayoutObject::from_projection_with_content(
                Provenance::manifested(TypedObjectId::Event(eid), region, vec![]),
                Some(staff),
                LayoutContent::Rest(RestContent {
                    voice: crate::logical::VoicePlace::Alone,
                    position: TimePoint::Musical(MusicalPosition::origin()),
                    components,
                    staff_position: None,
                    visible: true,
                    whole_measure,
                }),
            )];
            to_constrained(&logical)
        };
        let c = shown(vec![component(0, 1), component(1, 16)], false);
        let names: Vec<&str> = c.glyphs.iter().map(|g| g.glyph.as_str()).collect();
        assert_eq!(names, ["rest16th", "rest16th"]);
        assert_ne!(c.glyphs[0].baseline.x, c.glyphs[1].baseline.x);
        let dotted = PlacedComponent {
            offset: MusicalDuration::zero(),
            component: NotatedComponent {
                base_value: NoteValue::Eighth,
                dots: 1,
                tuplet: None,
                tied_to_next: false,
            },
            tuplet: None,
        };
        let c = shown(vec![dotted.clone()], false);
        let names: Vec<&str> = c.glyphs.iter().map(|g| g.glyph.as_str()).collect();
        assert_eq!(names, ["rest8th", "augmentationDot"]);
        assert_eq!(c.glyphs[0].horizontal_slot, c.glyphs[1].horizontal_slot);
        assert!(c.glyphs[1].baseline.x.0 > c.glyphs[0].baseline.x.0);
        // A rest filling its measure is a whole rest hanging from the fourth
        // line, whatever its value, and draws no dot.
        let c = shown(vec![dotted], true);
        let names: Vec<&str> = c.glyphs.iter().map(|g| g.glyph.as_str()).collect();
        assert_eq!(names, ["restWhole"]);
        assert_eq!(c.glyphs[0].baseline.y.0, 3.0);
    }

    /// No spring slot is ever empty: a slot exists only for a glyph-bearing
    /// column, so the engraver's coordinate remap always has a source point for
    /// every slot.
    #[test]
    fn no_spring_slot_is_empty() {
        for seed in 0..32u64 {
            let c = to_constrained(&to_logical(&valid_score_rich(seed)));
            for slot in &c.horizontal_slots {
                assert!(
                    !slot.members.is_empty(),
                    "a spring slot has no glyph members (would break the remap)"
                );
            }
        }
    }

    #[test]
    fn spacing_populates_a_consumable_time_axis_per_region() {
        let c = to_constrained(&to_logical(&valid_score_rich(11)));
        assert!(!c.regions.is_empty());
        let slot_ids: BTreeSet<_> = c.horizontal_slots.iter().map(|s| s.id).collect();
        // Every glyph names one of the IR's real spring slots (its column).
        for glyph in &c.glyphs {
            assert!(slot_ids.contains(&glyph.horizontal_slot));
        }
        for region in &c.regions {
            // The axis indexes musical *note* columns; each placement's time
            // projects back to that column's slot (not a constant).
            for placement in region.time_axis.placements() {
                assert!(slot_ids.contains(&placement.slot));
                assert_eq!(
                    region.time_axis.project(placement.time.clone()),
                    placement.slot
                );
            }
            // Distinct note columns have distinct times (project is a real
            // function of the query, not "always the first slot").
            if region.time_axis.placements().len() >= 2 {
                let p = region.time_axis.placements();
                assert_ne!(
                    region.time_axis.project(p[0].time.clone()),
                    region.time_axis.project(p[1].time.clone())
                );
            }
        }
    }

    /// Chord/simultaneous glyphs share one column slot — the per-musical-column
    /// contract — rather than each getting its own.
    #[test]
    fn simultaneous_glyphs_share_one_column_slot() {
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch};
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling, RegionId, StaffId,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch_a = PitchId::from_raw(100);
        let pitch_b = PitchId::from_raw(101);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        // One event, two pitches at the same onset (a chord).
        let note = LayoutContent::Note(NoteContent {
            voice: crate::logical::VoicePlace::Alone,
            position: TimePoint::Musical(MusicalPosition::origin()),
            components: vec![],
            pitches: vec![
                NotePitch {
                    pitch: pitch_a,
                    spelling: Some(PitchSpelling::cmn(CmnNominal::C, 4)),
                },
                NotePitch {
                    pitch: pitch_b,
                    spelling: Some(PitchSpelling::cmn(CmnNominal::E, 4)),
                },
            ],
        });
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    manifested(TypedObjectId::Event(EventId::from_raw(1)), note),
                    manifested(TypedObjectId::Pitch(pitch_a), LayoutContent::Structural),
                    manifested(TypedObjectId::Pitch(pitch_b), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);
        let heads: Vec<_> = c
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("notehead"))
            .collect();
        assert_eq!(heads.len(), 2, "both chord pitches draw a notehead");
        assert_eq!(
            heads[0].horizontal_slot, heads[1].horizontal_slot,
            "chord noteheads share one column slot"
        );
        assert_eq!(
            heads[0].baseline.x, heads[1].baseline.x,
            "…and therefore share an x"
        );
        assert_ne!(
            heads[0].baseline.y, heads[1].baseline.y,
            "but sit at distinct staff positions"
        );
    }

    /// Band membership is a correct partition: every glyph names an existing
    /// band, no glyph is a member of two bands, and a glyph's `vertical_band`
    /// equals the band that lists it — so a glyph is never placed in another
    /// staff's band.
    #[test]
    fn glyphs_are_routed_to_exactly_their_band() {
        for seed in 0..48u64 {
            let c = to_constrained(&to_logical(&valid_score_rich(seed)));
            let band_ids: BTreeSet<_> = c.vertical_bands.iter().map(|b| b.id).collect();

            let mut member_band: BTreeMap<GlyphObjectId, VerticalBandId> = BTreeMap::new();
            for b in &c.vertical_bands {
                for m in &b.members {
                    assert!(
                        member_band.insert(*m, b.id).is_none(),
                        "a glyph is a member of two bands"
                    );
                }
            }
            for g in &c.glyphs {
                assert!(
                    band_ids.contains(&g.vertical_band),
                    "glyph names an unknown band"
                );
                assert_eq!(
                    member_band.get(&g.id()),
                    Some(&g.vertical_band),
                    "glyph is not a member of the band it names"
                );
            }
        }
    }

    /// A two-staff region yields a staff band per staff (no cross-staff
    /// contamination) plus an inter-staff gap band; each staff's glyphs land in
    /// that staff's band only. The glyph-bearing objects are a staff instance
    /// (its clef) and a pitched event's pitch (its notehead) per staff — staff
    /// objects and stems engrave to free stroke lines, not band members.
    #[test]
    fn multi_staff_region_routes_per_staff_with_a_gap_band() {
        use crate::logical::{
            LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch, StaffContent,
        };
        use crate::provenance::Provenance;
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use crate::vertical_band::VerticalBandKind;
        use epiphany_core::{
            CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling, RegionId, StaffId,
            StaffInstanceId,
        };

        let region = RegionId::from_raw(1);
        let region_src = TypedObjectId::Region(region);
        let staff_a = StaffId::from_raw(10);
        let staff_b = StaffId::from_raw(20);
        let with_content = |src: TypedObjectId, staff: StaffId, content: LayoutContent| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        // Per staff: a staff instance (its clef glyph) and a single-pitch note
        // whose pitch draws a notehead — two band glyphs each.
        let staff_objects = |staff: StaffId, si: u128, eid: u128, pid: u128| {
            let pitch = PitchId::from_raw(pid);
            vec![
                with_content(
                    TypedObjectId::StaffInstance(StaffInstanceId::from_raw(si)),
                    staff,
                    LayoutContent::Staff(StaffContent {
                        default_clef: Clef::default(),
                        clefs: vec![],
                        keys: vec![],
                        beams: Vec::new(),
                    }),
                ),
                with_content(
                    TypedObjectId::Event(EventId::from_raw(eid)),
                    staff,
                    LayoutContent::Note(NoteContent {
                        voice: crate::logical::VoicePlace::Alone,
                        position: TimePoint::Musical(MusicalPosition::origin()),
                        components: vec![],
                        pitches: vec![NotePitch {
                            pitch,
                            spelling: Some(PitchSpelling::cmn(CmnNominal::C, 4)),
                        }],
                    }),
                ),
                with_content(
                    TypedObjectId::Pitch(pitch),
                    staff,
                    LayoutContent::Structural,
                ),
            ]
        };
        let mut objects = staff_objects(staff_a, 1, 1, 100);
        objects.extend(staff_objects(staff_b, 2, 2, 200));
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(region_src, vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff_a, staff_b],
                },
                objects,
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        let c = to_constrained(&logical);

        let staff_bands: Vec<_> = c
            .vertical_bands
            .iter()
            .filter(|b| matches!(b.kind, VerticalBandKind::Staff(_)))
            .collect();
        let gap_bands = c
            .vertical_bands
            .iter()
            .filter(|b| matches!(b.kind, VerticalBandKind::InterStaffGap))
            .count();
        assert_eq!(staff_bands.len(), 2, "one staff band per staff");
        assert_eq!(gap_bands, 1, "one inter-staff gap band between two staves");

        // Staff A's two glyphs (clef + notehead) are in A's band only.
        let band_a = staff_bands
            .iter()
            .find(|b| b.kind == VerticalBandKind::Staff(staff_a))
            .unwrap();
        let band_b = staff_bands
            .iter()
            .find(|b| b.kind == VerticalBandKind::Staff(staff_b))
            .unwrap();
        assert_eq!(band_a.members.len(), 2);
        assert_eq!(band_b.members.len(), 2);
        let a_set: BTreeSet<_> = band_a.members.iter().collect();
        assert!(
            band_b.members.iter().all(|m| !a_set.contains(m)),
            "no glyph is in both staves' bands"
        );
    }

    #[test]
    fn malformed_region_provenance_is_rejected_not_dropped() {
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use crate::LayoutRegion;
        use epiphany_core::EventId;

        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(
                    TypedObjectId::Event(EventId::from_raw(9)),
                    vec![],
                ),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent::default(),
                objects: vec![],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };
        assert!(matches!(
            try_to_constrained(&logical),
            Err(LayoutTransformError::RegionSourceIsNotRegion(_))
        ));
    }

    // --- Repeat barlines and volta brackets (schema-major-2 E1) ------------

    use epiphany_core::{
        AnchorOffset, BeatGroup, MusicalDuration, PowerOfTwo, RationalTime, RegionEdge, RepeatKind,
        RepeatStructure, RepeatStructureId, Score, TimeSignature, TimeSignatureDisplay,
        TimeSignatureId, Volta,
    };

    /// `valid_score_rich` with four extra measures appended to region A's first
    /// staff instance (region-anchored at whole-note offsets 1..=4), so repeat
    /// boundaries have real barline columns to land on. Returns the score and
    /// the five measure ids in order; the last measure's barline is the
    /// region-final one (region A's staff manifests nowhere else).
    fn repeat_ready_score(seed: u64) -> (Score, Vec<MeasureId>) {
        let mut score = valid_score_rich(seed);
        let region_id = score.canvas.regions[0].id;
        let extra: Vec<MeasureId> = (0..4).map(|_| score.identity.mint()).collect();
        let instance = &mut score.canvas.regions[0]
            .content
            .staff_instances_mut()
            .expect("region A is staff-based")[0];
        let mut ids = vec![instance.measures[0].id];
        for (index, id) in extra.iter().enumerate() {
            instance.measures.push(epiphany_core::Measure {
                id: *id,
                start: TimeAnchor::Region {
                    id: region_id,
                    edge: RegionEdge::Start,
                    offset: AnchorOffset::Musical(MusicalDuration(
                        RationalTime::new(index as i64 + 1, 1).expect("nonzero"),
                    )),
                },
                time_signature: None,
                explicit_number: None,
                number_visibility: Default::default(),
            });
        }
        ids.extend(extra);
        (score, ids)
    }

    fn measure_start(id: MeasureId) -> TimeAnchor {
        TimeAnchor::Measure {
            id,
            position: MeasurePosition::Start,
            offset: AnchorOffset::Zero,
        }
    }

    fn named<'a>(constrained: &'a ConstrainedLayoutIR, name: &str) -> Vec<&'a GlyphObject> {
        constrained
            .glyphs
            .iter()
            .filter(|glyph| glyph.glyph.as_str() == name)
            .collect()
    }

    #[test]
    fn repeat_boundaries_morph_their_measure_barlines_and_voltas_draw_brackets() {
        let (mut score, m) = repeat_ready_score(21);
        // The measure gaining the start sign also introduces a 4/4, so the
        // test covers the time signature clearing the wider sign's ink.
        let ts_id: TimeSignatureId = score.identity.mint();
        let beat = || BeatGroup {
            duration: MusicalDuration(RationalTime::new(1, 4).expect("nonzero")),
            subdivision: None,
            accent: 1,
        };
        score.time_signatures.push(
            TimeSignature::new(
                ts_id,
                TimeSignatureDisplay::Standard {
                    numerator: 4,
                    denominator: PowerOfTwo::new(4).expect("4 is a power of two"),
                },
                MusicalDuration(RationalTime::new(1, 1).expect("nonzero")),
                vec![beat(), beat(), beat(), beat()],
            )
            .expect("4/4 beat groups sum to a whole note"),
        );
        score.canvas.regions[0]
            .content
            .staff_instances_mut()
            .expect("region A is staff-based")[0]
            .measures
            .iter_mut()
            .find(|measure| measure.id == m[1])
            .expect("m1 exists")
            .time_signature = Some(ts_id);
        let a: RepeatStructureId = score.identity.mint();
        let b: RepeatStructureId = score.identity.mint();
        score.cross_cutting.repeats.push(RepeatStructure {
            id: a,
            start: measure_start(m[1]),
            end: measure_start(m[2]),
            kind: RepeatKind::SimpleRepeat { count: 2 },
            voltas: Vec::new(),
        });
        score.cross_cutting.repeats.push(RepeatStructure {
            id: b,
            start: measure_start(m[2]),
            end: measure_start(m[3]),
            kind: RepeatKind::Volta,
            voltas: vec![Volta {
                endings: vec![2, 3],
                start: measure_start(m[2]),
                end: measure_start(m[3]),
            }],
        });
        let constrained = to_constrained(&to_logical(&score));

        // The three boundary columns morph their measure barlines: a start
        // sign, the combined sign where A's end meets B's start, and an end
        // sign — each keeping the measure's own exact provenance (a morph is a
        // name change, not a new primitive; the round-trip provenance floor
        // depends on that).
        for name in ["repeatLeft", "repeatRightLeft", "repeatRight"] {
            let signs = named(&constrained, name);
            assert_eq!(signs.len(), 1, "exactly one {name} sign");
            let sign = signs[0];
            assert!(
                matches!(sign.provenance.source, TypedObjectId::Measure(_)),
                "{name} is the measure's own (morphed) barline"
            );
            assert!(sign.provenance.synthesis.is_none());
        }
        // Exactly three plain barlines morphed away: five measures draw
        // m0..=m3 at their start columns and m4 at the region end.
        assert_eq!(named(&constrained, "barlineSingle").len(), 1);
        assert_eq!(named(&constrained, "barlineFinal").len(), 1);

        // The volta bracket: three strokes (line + two hooks) above the staff,
        // synthesized from the repeat, spanning m2's column to m3's.
        let bracket: Vec<&Stroke> = constrained
            .strokes
            .iter()
            .filter(|stroke| {
                matches!(
                    stroke.provenance.synthesis,
                    Some(SynthesisKind::Registered(k)) if k == VOLTA_SYNTHESIS
                ) && matches!(stroke.provenance.source, TypedObjectId::RepeatStructure(id) if id == b)
            })
            .collect();
        assert_eq!(bracket.len(), 3, "bracket line plus two hooks");
        let line = bracket
            .iter()
            .find(|stroke| stroke.from.y == stroke.to.y)
            .expect("the bracket has a horizontal line");
        assert!(
            line.from.y.0 > STAFF_HEIGHT,
            "the bracket sits above the staff"
        );
        assert!(
            line.to.x.0 > line.from.x.0,
            "the bracket spans left to right"
        );
        for hook in bracket.iter().filter(|stroke| stroke.from.y != stroke.to.y) {
            assert_eq!(hook.from.x, hook.to.x, "hooks are vertical");
            assert!(hook.to.y.0 < hook.from.y.0, "hooks descend from the line");
        }
        // The ending numbers "2 3" draw as digit glyphs synthesized from the
        // repeat, under the bracket line.
        for digit in ["timeSig2", "timeSig3"] {
            let glyphs = named(&constrained, digit);
            let volta_digit = glyphs
                .iter()
                .find(|glyph| {
                    matches!(glyph.provenance.source, TypedObjectId::RepeatStructure(id) if id == b)
                })
                .unwrap_or_else(|| panic!("volta ending digit {digit} is drawn"));
            assert!(volta_digit.baseline.y.0 > STAFF_HEIGHT);
            assert!(volta_digit.baseline.y.0 < line.from.y.0);
        }

        // The 4/4 the morphed measure introduces clears the sign's ink: every
        // measure-owned digit starts right of the repeatLeft's right edge.
        let start_sign = named(&constrained, "repeatLeft")[0];
        let sign_right = start_sign.baseline.x.0 + start_sign.bounding_box.right.0;
        let measure_digits: Vec<&GlyphObject> = constrained
            .glyphs
            .iter()
            .filter(|glyph| {
                glyph.glyph.as_str().starts_with("timeSig")
                    && matches!(glyph.provenance.source, TypedObjectId::Measure(_))
            })
            .collect();
        assert!(!measure_digits.is_empty(), "the 4/4 draws digit glyphs");
        for digit in measure_digits {
            assert!(
                digit.baseline.x.0 + digit.bounding_box.left.0 >= sign_right,
                "time-signature digits clear the repeat sign's ink"
            );
        }

        // The full pipeline round-trips: every source recovered exactly once,
        // every synthesized primitive with a distinct stable id.
        crate::roundtrip::round_trip(&score);
    }

    #[test]
    fn off_grid_and_region_end_boundaries_stand_alone() {
        let (mut score, m) = repeat_ready_score(22);
        // Region A's triplet events sit at 0, 1/12, 2/12 — the second event is
        // mid-measure, off every barline column.
        let mid_measure_event = score.canvas.regions[0].staff_instances()[0].voices[0].events[1];
        let c: RepeatStructureId = score.identity.mint();
        score.cross_cutting.repeats.push(RepeatStructure {
            id: c,
            start: TimeAnchor::Event {
                id: mid_measure_event,
                offset: AnchorOffset::Zero,
            },
            end: TimeAnchor::Measure {
                id: m[4],
                position: MeasurePosition::End,
                offset: AnchorOffset::Zero,
            },
            kind: RepeatKind::SimpleRepeat { count: 2 },
            voltas: Vec::new(),
        });
        let constrained = to_constrained(&to_logical(&score));

        // The mid-measure start mints its own column and stands alone,
        // synthesized from the repeat (no measure barline morphs).
        let left = named(&constrained, "repeatLeft");
        assert_eq!(left.len(), 1);
        assert!(matches!(left[0].provenance.source, TypedObjectId::RepeatStructure(id) if id == c));
        assert!(matches!(
            left[0].provenance.synthesis,
            Some(SynthesisKind::Registered(k)) if k == REPEAT_BARLINE_SYNTHESIS
        ));

        // The region-end close keeps the final barline and adds the dot pair
        // beside it (never a morph, never a second barline).
        let finals = named(&constrained, "barlineFinal");
        assert_eq!(finals.len(), 1, "the final barline stands");
        let dots = named(&constrained, "repeatDots");
        assert_eq!(dots.len(), 1, "one dot pair beside it");
        assert!(named(&constrained, "repeatRight").is_empty());
        assert!(
            dots[0].baseline.x.0 < finals[0].baseline.x.0,
            "the dots face the repeated passage, left of the final barline"
        );
        assert_eq!(
            dots[0].horizontal_slot, finals[0].horizontal_slot,
            "the dots share the region-closing column"
        );

        crate::roundtrip::round_trip(&score);
    }

    #[test]
    fn two_repeats_merge_one_standalone_sign_that_clears_the_notes() {
        let (mut score, m) = repeat_ready_score(24);
        // Region A's triplet events sit at 0, 1/12, 2/12; the second event is
        // an off-grid boundary two structures share: one ends there, the
        // other starts there.
        let mid_measure_event = score.canvas.regions[0].staff_instances()[0].voices[0].events[1];
        let event_anchor = TimeAnchor::Event {
            id: mid_measure_event,
            offset: AnchorOffset::Zero,
        };
        let g1: RepeatStructureId = score.identity.mint();
        let g2: RepeatStructureId = score.identity.mint();
        score.cross_cutting.repeats.push(RepeatStructure {
            id: g1,
            start: measure_start(m[1]),
            end: event_anchor.clone(),
            kind: RepeatKind::SimpleRepeat { count: 2 },
            voltas: Vec::new(),
        });
        score.cross_cutting.repeats.push(RepeatStructure {
            id: g2,
            start: event_anchor,
            end: measure_start(m[2]),
            kind: RepeatKind::SimpleRepeat { count: 2 },
            voltas: Vec::new(),
        });
        let constrained = to_constrained(&to_logical(&score));

        // One merged combined sign, owned by the smallest structure id,
        // depending on both structures.
        let combined = named(&constrained, "repeatRightLeft");
        assert_eq!(combined.len(), 1, "the shared boundary draws one sign");
        let sign = combined[0];
        assert!(matches!(
            sign.provenance.synthesis,
            Some(SynthesisKind::Registered(k)) if k == REPEAT_BARLINE_SYNTHESIS
        ));
        assert!(
            matches!(sign.provenance.source, TypedObjectId::RepeatStructure(id) if id == g1.min(g2))
        );
        for id in [g1, g2] {
            assert!(sign
                .provenance
                .dependencies
                .contains(&TypedObjectId::RepeatStructure(id)));
        }
        // The end-facing sign's leftward ink reserved its reach (the
        // accidental-overhang mechanism), so it crosses no notehead's ink.
        let sign_left = sign.baseline.x.0 + sign.bounding_box.left.0;
        let sign_right = sign.baseline.x.0 + sign.bounding_box.right.0;
        for head in constrained
            .glyphs
            .iter()
            .filter(|glyph| glyph.glyph.as_str().starts_with("notehead"))
        {
            let head_left = head.baseline.x.0 + head.bounding_box.left.0;
            let head_right = head.baseline.x.0 + head.bounding_box.right.0;
            assert!(
                sign_right <= head_left || head_right <= sign_left,
                "the repeat sign's ink must not cross a notehead's"
            );
        }

        crate::roundtrip::round_trip(&score);
    }

    #[test]
    fn jump_kinds_and_unresolved_boundaries_draw_no_ink() {
        let (mut score, m) = repeat_ready_score(23);
        let replica = score.identity.replica_id;
        // A DalSegno repeat: barlines are a jump-mark tranche, not E1 — only
        // the traced anchor is emitted. Its volta bracket still draws: the
        // voltas list is kind-independent.
        let d: RepeatStructureId = score.identity.mint();
        score.cross_cutting.repeats.push(RepeatStructure {
            id: d,
            start: measure_start(m[1]),
            end: measure_start(m[3]),
            kind: RepeatKind::DalSegno {
                segno: measure_start(m[2]),
                end_target: measure_start(m[1]),
            },
            voltas: vec![Volta {
                endings: vec![1],
                start: measure_start(m[2]),
                end: measure_start(m[3]),
            }],
        });
        // A simple repeat whose start dangles (a decoded score may hold one);
        // the resolved end still morphs, the dangling side draws nothing.
        let e: RepeatStructureId = score.identity.mint();
        score.cross_cutting.repeats.push(RepeatStructure {
            id: e,
            start: TimeAnchor::Measure {
                id: MeasureId::new(replica, 9_999_999),
                position: MeasurePosition::Start,
                offset: AnchorOffset::Zero,
            },
            end: measure_start(m[2]),
            kind: RepeatKind::SimpleRepeat { count: 2 },
            voltas: Vec::new(),
        });
        // A bare wall-clock repeat: nothing pins it to a region, so it draws
        // no ink anywhere (its placements are Unresolved by rule).
        let f: RepeatStructureId = score.identity.mint();
        score.cross_cutting.repeats.push(RepeatStructure {
            id: f,
            start: TimeAnchor::WallClock {
                time: WallClockTime(5),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(10),
            },
            kind: RepeatKind::SimpleRepeat { count: 2 },
            voltas: Vec::new(),
        });
        let constrained = to_constrained(&to_logical(&score));

        assert!(named(&constrained, "repeatLeft").is_empty());
        assert!(named(&constrained, "repeatRightLeft").is_empty());
        assert!(named(&constrained, "repeatDots").is_empty());
        let right = named(&constrained, "repeatRight");
        assert_eq!(right.len(), 1, "the resolved end still closes");
        // The jump kind's volta bracket draws even though its barlines do not.
        let bracket_strokes = constrained
            .strokes
            .iter()
            .filter(|stroke| {
                matches!(
                    stroke.provenance.synthesis,
                    Some(SynthesisKind::Registered(k)) if k == VOLTA_SYNTHESIS
                )
            })
            .count();
        assert_eq!(bracket_strokes, 3, "the DalSegno's volta bracket draws");
        // All three structures keep their traced anchors.
        for id in [d, e, f] {
            assert!(
                constrained.strokes.iter().any(|stroke| {
                    stroke.from == stroke.to
                        && matches!(stroke.provenance.source,
                            TypedObjectId::RepeatStructure(s) if s == id)
                }),
                "repeat {id:?} keeps its zero-extent traced anchor"
            );
        }
    }

    // --- Slur curves (schema-major-2 E2) -----------------------------------

    use epiphany_core::{
        CurvatureOverride, CurveDirection, EventId, Slur, SlurId, SpaceUnit, SpanStyle,
    };

    /// The events of region A's first voice (all on its one staff), for slur
    /// endpoints that resolve to note columns.
    fn region_a_events(score: &Score) -> Vec<EventId> {
        score.canvas.regions[0].staff_instances()[0].voices[0]
            .events
            .clone()
    }

    /// Re-pitches every event in reading order, cycling `octaves` (all C naturals).
    /// The corpus generator writes only low notes, so a test that needs a
    /// down-stem — or a note tall enough to obstruct a slur — must say so.
    fn repitch(score: &mut Score, octaves: &[i8]) {
        use epiphany_core::{CmnNominal, Event, PitchSpacePosition};
        let ids: Vec<_> = score.events.iter().map(|e| e.id()).collect();
        for (index, id) in ids.iter().enumerate() {
            if let Some(Event::Pitched(note)) = score.events.get_mut(*id) {
                for pitch in &mut note.pitches {
                    if let PitchSpacePosition::Cmn {
                        nominal, octave, ..
                    } = &mut pitch.pitch.scale_position.position
                    {
                        *nominal = CmnNominal::C;
                        *octave = octaves[index % octaves.len()];
                    }
                }
            }
        }
    }

    fn slur(id: SlurId, start: EventId, end: EventId, over: Option<CurvatureOverride>) -> Slur {
        Slur {
            id,
            start_event: start,
            end_event: end,
            kind: epiphany_core::SlurKind::Legato,
            curvature_override: over,
            style: SpanStyle::default(),
        }
    }

    fn slur_curve_of(constrained: &ConstrainedLayoutIR, id: SlurId) -> Option<&Curve> {
        constrained
            .curves
            .iter()
            .find(|curve| curve.provenance.source == TypedObjectId::Slur(id))
    }

    /// An `Auto` slur is placed OPPOSITE the stems — the single-voice engraving
    /// rule. All stems up (low notes) puts it under the noteheads; all stems down
    /// (high notes) puts it over them. It used to arc above unconditionally,
    /// which drew it straight through the stems of every stem-up passage.
    #[test]
    fn a_default_slur_takes_the_side_opposite_the_stems() {
        for (octave, expect_above) in [(4i8, false), (6, true)] {
            let (mut score, _) = repeat_ready_score(41);
            repitch(&mut score, &[octave]);
            let events = region_a_events(&score);
            let id: SlurId = score.identity.mint();
            score
                .cross_cutting
                .slurs
                .push(slur(id, events[0], events[2], None));
            let constrained = to_constrained(&to_logical(&score));

            let curve = slur_curve_of(&constrained, id).expect("the slur draws a curve");
            // Its exact provenance rides the curve — one primitive per slur, no
            // synthesis (a slur owns a single curve).
            assert!(curve.provenance.synthesis.is_none());
            assert!(curve.p3.x.0 > curve.p0.x.0, "left to right");
            assert_eq!(curve.p0.y, curve.p3.y, "equal pitches share a baseline");

            let arcs_up = curve.p1.y.0 > curve.p0.y.0 && curve.p2.y.0 > curve.p0.y.0;
            assert_eq!(
                arcs_up,
                expect_above,
                "C{octave}: stems point {}, so the slur goes {}",
                if expect_above { "down" } else { "up" },
                if expect_above { "above" } else { "below" }
            );
            // The endpoints hug the ENDPOINT NOTE, not the staff. A C6's head
            // centre sits at y = 6 and its ink tops out half a space above; a C4's
            // centre at −1, its ink half a space below. Endpoints a fixed gap from
            // the staff edge instead (y = 4.7 / −0.7) would leave an above-slur
            // hanging *under* its own ledger notes — the original defect.
            let head_centre = if expect_above { 6.0 } else { -1.0 };
            let want = if expect_above {
                head_centre + 0.5 + SLUR_ENDPOINT_GAP
            } else {
                head_centre - 0.5 - SLUR_ENDPOINT_GAP
            };
            assert!(
                (curve.p0.y.0 - want).abs() < 1e-3,
                "C{octave}: the endpoint hugs its note at {want}, not the staff: {}",
                curve.p0.y.0
            );
            // No traced anchor for a drawn slur (the curve carries the provenance).
            assert!(!constrained
                .strokes
                .iter()
                .any(|s| s.provenance.source == TypedObjectId::Slur(id)));
            crate::roundtrip::round_trip(&score);
        }
    }

    /// The arc is raised until it clears every column between its endpoints. Two
    /// C4s (stems up) around a C6 (stem down) is a mixed-stem span, so the slur
    /// goes above — and the C6 stands three ledger lines inside it.
    #[test]
    fn a_slur_arcs_clear_of_a_note_between_its_endpoints() {
        let (mut score, _) = repeat_ready_score(45);
        repitch(&mut score, &[4, 6, 4]);
        let events = region_a_events(&score);
        let id: SlurId = score.identity.mint();
        score
            .cross_cutting
            .slurs
            .push(slur(id, events[0], events[2], None));
        let constrained = to_constrained(&to_logical(&score));
        let curve = slur_curve_of(&constrained, id).expect("the slur draws a curve");

        assert!(
            curve.p1.y.0 > curve.p0.y.0,
            "a mixed-stem span has no notehead side, so it arcs above"
        );
        // The C6 notehead's top: three ledgers above a staff whose top line is 4.
        let c6_top = curve
            .control_points()
            .iter()
            .map(|p| p.y.0)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(c6_top > 6.0, "the arc reaches over the C6: {c6_top}");
        // Sample the cubic at its apex (t = 0.5) and check it clears the note.
        let at = |t: f32| {
            let (u, cp) = (1.0 - t, curve.control_points());
            u * u * u * cp[0].y.0
                + 3.0 * u * u * t * cp[1].y.0
                + 3.0 * u * t * t * cp[2].y.0
                + t * t * t * cp[3].y.0
        };
        // The C6's head centre sits at y = 6 (step 12); its ink tops out at ~6.5.
        // The arc clears it by exactly the endpoint gap and no more: the lift is
        // the least that suffices. Evaluating the obstacle at its column x rather
        // than its notehead CENTRE skews `t` and silently over-lifts, which this
        // upper bound catches.
        let required = 6.5 + SLUR_ENDPOINT_GAP;
        assert!(
            at(0.5) >= required - 1e-3,
            "the apex clears the intervening notehead: {} < {required}",
            at(0.5)
        );
        assert!(
            at(0.5) <= required + 0.05,
            "and does not overshoot it: {} > {required}",
            at(0.5)
        );
    }

    /// An authored direction overrides the stem rule, and an authored height is
    /// honoured — as a FLOOR. Clearance may raise it (a slur must never be drawn
    /// through a note to obey an author), but nothing lowers it.
    #[test]
    fn an_authored_direction_and_height_override_the_defaults() {
        let (mut score, _) = repeat_ready_score(42);
        repitch(&mut score, &[4]); // all C4: stems up, so Auto would go below
        let events = region_a_events(&score);
        let forced_above: SlurId = score.identity.mint();
        let forced_below: SlurId = score.identity.mint();
        let height = SpaceUnit(epiphany_determinism::CanonicalF64::new(2.0).expect("finite"));
        score.cross_cutting.slurs.push(slur(
            forced_above,
            events[0],
            events[2],
            Some(CurvatureOverride {
                direction: Some(CurveDirection::Above),
                height: None,
            }),
        ));
        score.cross_cutting.slurs.push(slur(
            forced_below,
            events[0],
            events[2],
            Some(CurvatureOverride {
                direction: Some(CurveDirection::Below),
                height: Some(height),
            }),
        ));
        let constrained = to_constrained(&to_logical(&score));

        let up = slur_curve_of(&constrained, forced_above).expect("above slur draws");
        let down = slur_curve_of(&constrained, forced_below).expect("below slur draws");
        // The authored `Above` wins over the stem rule, which wanted below.
        assert!(
            up.p1.y.0 > up.p0.y.0 && up.p2.y.0 > up.p0.y.0,
            "an authored Above arcs upward even over stem-up notes"
        );
        assert!(down.p1.y.0 < down.p0.y.0 && down.p2.y.0 < down.p0.y.0);
        assert!(down.p0.y.0 < 0.0, "a below slur sits under the staff");
        // The authored apex height is 2.0: the control lift is 4/3 · height, so
        // the apex sits 0.75 · lift from the chord. Equal pitches make the chord
        // flat and the intervening column no obstacle, so nothing raises it.
        let lift = down.p1.y.0 - down.p0.y.0;
        assert!(
            (0.75 * -lift - 2.0).abs() < 1e-4,
            "authored apex height honoured (2.0 staff spaces): lift {lift}"
        );
    }

    /// An authored height that clearance must overrule: a below-slur whose span
    /// dips over a C2. The author asked for a shallow 0.5, but honouring it would
    /// draw the arc straight through the low note, so the apex is raised.
    #[test]
    fn clearance_raises_an_authored_height_it_would_otherwise_violate() {
        let (mut score, _) = repeat_ready_score(46);
        repitch(&mut score, &[4, 2, 4]); // stems all up ⇒ Auto below; C2 dips low
        let events = region_a_events(&score);
        let id: SlurId = score.identity.mint();
        let shallow = SpaceUnit(epiphany_determinism::CanonicalF64::new(0.5).expect("finite"));
        score.cross_cutting.slurs.push(slur(
            id,
            events[0],
            events[2],
            Some(CurvatureOverride {
                direction: None,
                height: Some(shallow),
            }),
        ));
        let constrained = to_constrained(&to_logical(&score));
        let curve = slur_curve_of(&constrained, id).expect("the slur draws");

        let lift = curve.p1.y.0 - curve.p0.y.0;
        let apex = 0.75 * -lift;
        assert!(
            apex > 0.5 + 1e-3,
            "the shallow authored height was raised to clear the C2: {apex}"
        );
        // And it really does clear: sample the cubic at the obstructing column.
        let at = |t: f32| {
            let (u, cp) = (1.0 - t, curve.control_points());
            u * u * u * cp[0].y.0
                + 3.0 * u * u * t * cp[1].y.0
                + 3.0 * u * t * t * cp[2].y.0
                + t * t * t * cp[3].y.0
        };
        // C2's head centre is at y = -8 (step -16); its ink bottoms out at ~-8.5.
        assert!(
            at(0.5) <= -8.5 - SLUR_ENDPOINT_GAP + 1e-3,
            "the apex passes under the C2: {}",
            at(0.5)
        );
    }

    /// A staff that declares its clef ONLY on the `Staff` — no `ClefChange` at
    /// all — engraves in that clef. `Staff::default_clef` used to be decorative:
    /// the projection took the active clef from the staff instance's sequence and
    /// fell back to `Clef::default()`, so a bass staff drew a treble clef and put
    /// every note two steps wrong (P13-I2).
    #[test]
    fn a_staff_declaring_only_a_default_clef_engraves_in_it() {
        let (mut score, _) = repeat_ready_score(48);
        let staff_id = score.canvas.regions[0].staff_instances()[0].staff;
        for staff in &mut score.staves {
            if staff.id == staff_id {
                staff.default_clef = Clef::bass();
            }
        }
        assert!(
            score.canvas.regions[0].staff_instances()[0]
                .clef_sequence
                .is_empty(),
            "the staff declares no ClefChange — only its default"
        );
        let instance_id = score.canvas.regions[0].staff_instances()[0].id;
        let c = to_constrained(&to_logical(&score));

        // THIS staff's clef glyph is the bass clef, not the treble default. The
        // score's other staves keep their own defaults, so scope by provenance.
        let drawn: Vec<&str> = c
            .glyphs
            .iter()
            .filter(|g| g.provenance.source == TypedObjectId::StaffInstance(instance_id))
            .map(|g| g.glyph.as_str())
            .collect();
        assert_eq!(
            drawn,
            vec!["fClef"],
            "a staff whose only clef is its default draws it"
        );
        // And its notes are placed against that clef: `active_clef_or` is what the
        // staff-position computation reads, so the two can never disagree.
        let at = TimePoint::Musical(epiphany_core::MusicalPosition::origin());
        assert_eq!(active_clef_or(&[], &at, Clef::bass()), Clef::bass());
        assert_eq!(
            active_clef(&[], &at),
            Clef::default(),
            "the old API is intact"
        );
    }

    /// `req:layoutir:coverage-diagnostics`: an object the projection cannot
    /// engrave faithfully is **recorded and still placed** — never guessed at,
    /// never dropped. A pitch spelled with a microtonal accidental draws its
    /// notehead, while an `UnbundledGlyph` diagnostic names the accidental it
    /// could not draw and no other accidental stands in for it.
    #[test]
    fn an_unengravable_object_is_recorded_and_still_placed() {
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR, NoteContent, NotePitch};
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{
            AccidentalId, CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling, RegionId,
            StaffId,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch = PitchId::from_raw(100);
        let mut spelling = PitchSpelling::cmn(CmnNominal::E, 5);
        spelling.accidentals.push(AccidentalId::new("quarter-flat"));
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let c = to_constrained(&LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(1)),
                        LayoutContent::Note(NoteContent {
                            voice: crate::logical::VoicePlace::Alone,
                            position: TimePoint::Musical(MusicalPosition::origin()),
                            components: vec![],
                            pitches: vec![NotePitch {
                                pitch,
                                spelling: Some(spelling),
                            }],
                        }),
                    ),
                    manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        });
        let source = TypedObjectId::Pitch(pitch);

        // Recorded: the gap names the object and the glyph it wanted.
        let diagnostic = c
            .diagnostics
            .iter()
            .find(|d| d.source == source)
            .expect("the unbundled accidental is surfaced, not hidden");
        assert!(
            matches!(&diagnostic.kind,
                LayoutDiagnosticKind::UnbundledGlyph(g) if g.as_str() == "quarter-flat"),
            "and says why: {:?}",
            diagnostic.kind
        );
        // Still placed: the notehead stands; not guessed: no accidental does.
        let glyphs: Vec<&str> = c
            .glyphs
            .iter()
            .filter(|g| g.provenance.source == source)
            .map(|g| g.glyph.as_str())
            .collect();
        assert_eq!(glyphs, ["noteheadBlack"]);
        assert!(c.validate().is_ok());
    }

    /// A stem points AWAY from the middle line — up for a head below it, down
    /// for a head above it or on it — and attaches on the side it points: an
    /// up-stem at the head's right edge, a down-stem at its left. A stem on a
    /// note beyond an octave from the middle line is drawn out TO that line,
    /// rather than dangling a fixed octave into the ledger field.
    ///
    /// Every stem in the engine used to point up, on the right, at a constant
    /// octave — an upward stem on a C6 three ledgers above the staff. It is also
    /// why an `Auto` slur, which is placed *opposite* the stems, could not be
    /// given a correct side until this landed.
    #[test]
    fn a_stem_points_away_from_the_middle_line_and_reaches_it() {
        use epiphany_core::{CmnNominal, Event, PitchSpacePosition};
        // The corpus generator writes only low notes, so spread the octaves to
        // straddle the middle line and reach well past a stem's length.
        let mut score = valid_score_rich(11);
        let ids: Vec<_> = score.events.iter().map(|e| e.id()).collect();
        for (index, id) in ids.iter().enumerate() {
            if let Some(Event::Pitched(note)) = score.events.get_mut(*id) {
                for pitch in &mut note.pitches {
                    if let PitchSpacePosition::Cmn {
                        nominal, octave, ..
                    } = &mut pitch.pitch.scale_position.position
                    {
                        *nominal = CmnNominal::C;
                        *octave = [3i8, 4, 5, 6][index % 4];
                    }
                }
            }
        }
        let c = to_constrained(&to_logical(&score));

        // Each staff's middle line, from that staff's own staff-line strokes.
        let mut lines: BTreeMap<VerticalBandId, f32> = BTreeMap::new();
        for stroke in &c.strokes {
            if matches!(stroke.provenance.source, TypedObjectId::Staff(_)) {
                let entry = lines.entry(stroke.vertical_band).or_insert(f32::INFINITY);
                *entry = entry.min(stroke.from.y.0);
            }
        }

        let stems: Vec<&Stroke> = c
            .strokes
            .iter()
            .filter(|s| (s.thickness.0 - STEM_THICKNESS).abs() < 1e-6)
            .filter(|s| (s.to.y.0 - s.from.y.0).abs() > 1e-6)
            .collect();
        assert!(!stems.is_empty(), "the fixture draws stems");

        let (mut saw_up, mut saw_down, mut saw_extended) = (false, false, false);
        for stem in stems {
            let middle = lines[&stem.vertical_band] + STAFF_HEIGHT * 0.5;
            let (base, tip) = (stem.from.y.0, stem.to.y.0);
            let up = tip > base;
            assert_eq!(
                up,
                base < middle,
                "a head below the middle line stems up, else down: base {base} middle {middle}"
            );
            if up {
                assert!(tip >= middle - 1e-4, "an up-stem reaches the middle line");
                saw_up = true;
            } else {
                assert!(tip <= middle + 1e-4, "a down-stem reaches the middle line");
                saw_down = true;
            }
            // Beyond an octave from the middle line, the stem stops AT that line.
            if (base - middle).abs() > STEM_LENGTH {
                assert!(
                    (tip - middle).abs() < 1e-4,
                    "a far-out note's stem stops at the middle line: {tip} vs {middle}"
                );
                saw_extended = true;
            }
        }
        assert!(saw_up && saw_down, "the fixture exercises both directions");
        assert!(
            saw_extended,
            "and at least one stem drawn out to the middle line"
        );
    }

    #[test]
    fn a_slur_with_an_unresolved_or_reversed_endpoint_keeps_its_anchor() {
        let (mut score, _) = repeat_ready_score(43);
        let events = region_a_events(&score);
        let replica = score.identity.replica_id;
        // A dangling end event: nothing to arc to.
        let dangling: SlurId = score.identity.mint();
        score.cross_cutting.slurs.push(slur(
            dangling,
            events[0],
            EventId::new(replica, 9_999_999),
            None,
        ));
        // A zero-span slur (both endpoints the same event): no left-to-right arc.
        let degenerate: SlurId = score.identity.mint();
        score
            .cross_cutting
            .slurs
            .push(slur(degenerate, events[0], events[0], None));
        let constrained = to_constrained(&to_logical(&score));

        for id in [dangling, degenerate] {
            assert!(
                slur_curve_of(&constrained, id).is_none(),
                "an unresolved/degenerate slur draws no curve"
            );
            assert!(
                constrained
                    .strokes
                    .iter()
                    .any(|s| { s.from == s.to && s.provenance.source == TypedObjectId::Slur(id) }),
                "…it keeps its zero-extent traced anchor"
            );
        }
    }

    #[test]
    fn a_cross_staff_slur_keeps_its_anchor_rather_than_floating() {
        use epiphany_core::generators::valid_score;
        // A `valid_score` seed with two staff instances in its single region.
        let mut score = (0..64u64)
            .map(valid_score)
            .find(|s| s.canvas.regions[0].staff_instances().len() == 2)
            .expect("some seed yields a two-staff region");
        let instances = score.canvas.regions[0].staff_instances();
        // One endpoint on each staff — the slur resolves to no single staff.
        let top = instances[0].voices[0].events[0];
        let bottom = instances[1].voices[0].events[0];
        let id: SlurId = score.identity.mint();
        score.cross_cutting.slurs.push(slur(id, top, bottom, None));
        let constrained = to_constrained(&to_logical(&score));

        // No curve floating at yo = 0 detached from a note on the other staff;
        // the traced anchor keeps provenance (a Minimal boundary).
        assert!(
            slur_curve_of(&constrained, id).is_none(),
            "a cross-staff slur draws no curve"
        );
        assert!(constrained
            .strokes
            .iter()
            .any(|s| s.from == s.to && s.provenance.source == TypedObjectId::Slur(id)));
        crate::roundtrip::round_trip(&score);
    }

    #[test]
    fn an_authored_dashed_slur_renders_a_dashed_curve() {
        use epiphany_core::{LineStyle, SpanStyle};
        let (mut score, _) = repeat_ready_score(45);
        let events = region_a_events(&score);
        let solid_id: SlurId = score.identity.mint();
        let dashed_id: SlurId = score.identity.mint();
        score
            .cross_cutting
            .slurs
            .push(slur(solid_id, events[0], events[2], None));
        let mut dashed = slur(dashed_id, events[0], events[2], None);
        dashed.style = SpanStyle {
            line: LineStyle::Dashed,
            thickness: None,
        };
        score.cross_cutting.slurs.push(dashed);
        let constrained = to_constrained(&to_logical(&score));

        // The authored line pattern is carried on the drawn curve — rendered
        // faithfully, not deferred to a diagnostic.
        assert_eq!(
            slur_curve_of(&constrained, dashed_id)
                .expect("dashed slur draws")
                .line,
            LineStyle::Dashed
        );
        assert_eq!(
            slur_curve_of(&constrained, solid_id)
                .expect("solid slur draws")
                .line,
            LineStyle::Solid
        );
        // No line-style diagnostic remains — the style is rendered, not surfaced.
        assert!(constrained
            .diagnostics
            .iter()
            .all(|d| d.source != TypedObjectId::Slur(dashed_id)));
    }

    #[test]
    fn out_of_range_authored_slur_dimensions_fall_back_to_defaults() {
        let (mut score, _) = repeat_ready_score(44);
        // High notes: stems down, so an Auto slur takes the side above them.
        repitch(&mut score, &[6]);
        let events = region_a_events(&score);
        let neg: epiphany_determinism::CanonicalF64 =
            epiphany_determinism::CanonicalF64::new(-1.0).expect("finite");
        let zero: epiphany_determinism::CanonicalF64 =
            epiphany_determinism::CanonicalF64::new(0.0).expect("finite");
        // A slur authoring a negative height and a zero thickness — pathological
        // out-of-range values that must NOT flip the arc, draw an invisible
        // curve, or (for a negative thickness) fail geometry validation and
        // blank the layout.
        let id: SlurId = score.identity.mint();
        let mut bad = slur(
            id,
            events[0],
            events[2],
            Some(CurvatureOverride {
                direction: None, // Auto: opposite the (down) stems ⇒ above
                height: Some(SpaceUnit(neg)),
            }),
        );
        bad.style.thickness = Some(SpaceUnit(zero));
        score.cross_cutting.slurs.push(bad);
        let constrained = to_constrained(&to_logical(&score));

        let curve = slur_curve_of(&constrained, id).expect("the slur still draws");
        // The negative height fell back to the positive default: the arc keeps
        // its side and its curvature.
        assert!(
            curve.p1.y.0 > curve.p0.y.0,
            "a non-positive authored height falls back, arc stays upward"
        );
        // The zero thickness fell back to the visible, hittable default.
        assert!(
            curve.thickness.0 > 0.0,
            "thickness falls back to a positive default"
        );
        // Geometry validates — the layout is not blanked.
        assert!(constrained.validate().is_ok());
    }
}
