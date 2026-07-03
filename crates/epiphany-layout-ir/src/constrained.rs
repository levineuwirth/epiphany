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
    Clef, EventId, KeySignature, MeasureId, MeasurePosition, MusicalDuration, NoteValue, PitchId,
    PitchSpelling, SpellingNominal, StaffId, TimeAnchor, TypedObjectId, WallClockTime,
};
use epiphany_determinism::{DomainTag, Preimage};

use crate::engrave_theory::{
    accidental_glyph, clef_glyph, has_stem, key_signature, notehead_glyph, rest_glyph,
    staff_position, KeyAccidental, StaffStep,
};
use crate::engraving::{EngravingDecision, OverrideKind, OverridePriority, OverrideTarget};
use crate::glyph::{metrics, BravuraCatalog, GlyphCatalog, GlyphCatalogIdentity, GlyphReference};
use crate::logical::{
    apply_offset, BarlineKind, LayoutContent, LogicalLayoutIR, PlacedClef, PlacedKeySignature,
    ScoreVersion, StaffContent,
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
}

impl Stroke {
    /// This stroke's stable id (derived from its provenance, as a glyph's is).
    pub fn id(&self) -> GlyphObjectId {
        GlyphObjectId(self.provenance.stable_id.0)
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
        }
        Ok(())
    }
}

// Minimal-tier engraving geometry, in staff spaces (Chapter 7 §7.2). These are
// fixed defaults, not yet solver-negotiated: the stub solver returns this
// geometry verbatim, so it is what the renderer draws.
const STAFF_LINE_THICKNESS: f32 = 0.13;
const STEM_THICKNESS: f32 = 0.12;
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
const NOTEHEAD_STEM_X: f32 = 1.15; // a stem-up attaches at the notehead's right edge
const ACCIDENTAL_X: f32 = 1.1; // the innermost accidental sits this far left of its notehead
const ACC_STACK_X: f32 = 0.9; // each further-out stacked accidental steps left by this
const KEY_SIG_START: f32 = 2.7; // x where a key signature begins (just after the clef)
const KEY_ACC_X: f32 = 0.9; // x advance per key-signature accidental
const TIME_SIG_X: f32 = 0.5; // a time signature sits this far right of its barline
const TIME_DIGIT_X: f32 = 0.8; // x advance per time-signature digit

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

/// The registry id for **time-signature synthesis**: the measure introduces the
/// meter, but its numerator/denominator digit glyphs each need a distinct stable
/// id, so they are synthesized from the measure.
const TIME_SIG_SYNTHESIS: SynthesisRegistryId = SynthesisRegistryId(0x54_494D_4553_4947); // "TIMESIG"

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
/// Glyphs (only) are routed to the band of their own staff (Chapter 7 §"Vertical
/// Bands"); strokes are free line primitives the solver positions but the band
/// model does not contain. Structural objects with no Minimal-tier glyph
/// (regions, voices, ties, slurs, beams, …) are carried as zero-extent traced
/// anchors so provenance survives, pending their engraving in a higher tier.
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
    let mut diagnostics = Vec::new();
    let mut vertical_bands = Vec::new();
    let mut horizontal_slots = Vec::new();
    let mut constraints = Vec::new();
    let mut break_origins = Vec::new();
    let mut constrained_regions = Vec::new();
    // Regions tile left-to-right; this advances by each region's width so all
    // coordinates stay globally monotonic (the solver's coordinate remap relies
    // on it). v0 has no page casting-off, so this replaces region overlap.
    let mut region_x: f32 = 0.0;

    for region in &logical.regions {
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
        // ones. An empty sequence defaults to treble.
        let mut clef_seq_of: BTreeMap<StaffId, Vec<PlacedClef>> = BTreeMap::new();
        for object in &region.objects {
            if let (Some(staff), LayoutContent::Staff(content)) = (object.staff(), object.content())
            {
                clef_seq_of
                    .entry(staff)
                    .or_insert_with(|| content.clefs.clone());
            }
        }
        let clef_seq = |staff: Option<StaffId>| -> &[PlacedClef] {
            staff
                .and_then(|s| clef_seq_of.get(&s))
                .map(Vec::as_slice)
                .unwrap_or(&[])
        };

        // Pass 1 — compute every glyph's notation, keyed for emission in pass 2,
        // and collect the distinct columns it occupies. A note/rest notated as a
        // multi-component (tied) decomposition yields one notehead/stem/rest per
        // component, each at `position + component.offset`.
        let mut pitch_heads: BTreeMap<PitchId, Vec<Head>> = BTreeMap::new();
        let mut event_stems: BTreeMap<EventId, Vec<StemSeg>> = BTreeMap::new();
        let mut event_rests: BTreeMap<EventId, Vec<RestSeg>> = BTreeMap::new();
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
        // The widest key signature in the region (in accidentals): the lead area
        // between clef and first note must fit it, so the first note column shifts
        // right by it (zero when no staff declares a key — layout unchanged).
        let mut key_sig_accs = 0usize;

        for object in &region.objects {
            let staff = object.staff();
            let yo = staff.map(&y_origin).unwrap_or(0.0);
            match (object.provenance().source, object.content()) {
                (TypedObjectId::Event(eid), LayoutContent::Note(note)) => {
                    let mut stems = Vec::new();
                    for (comp, (offset, value)) in components_of(&note.components).enumerate() {
                        let time = shift_time(&note.position, &offset);
                        let key = ColumnKey::Timed(time.clone(), ColumnRole::Note);
                        keys.insert(key.clone());
                        let clef = active_clef(clef_seq(staff), &time);
                        let name = notehead_glyph(value);
                        let mut ys = Vec::new();
                        for pitch in &note.pitches {
                            let (step, missing) = spelling_step(&pitch.spelling, &clef);
                            if missing {
                                diagnostics.push(LayoutDiagnostic {
                                    source: TypedObjectId::Pitch(pitch.pitch),
                                    kind: LayoutDiagnosticKind::MissingSpelling,
                                });
                            }
                            // The spelling's accidentals draw on the first component
                            // only; an unbundled (microtonal) one is surfaced, not
                            // guessed.
                            let accidentals = if comp == 0 {
                                pitch_accidentals(&pitch.spelling, pitch.pitch, &mut diagnostics)
                            } else {
                                Vec::new()
                            };
                            if !accidentals.is_empty() {
                                // The leftmost accidental's left edge, measured from
                                // the notehead (innermost at ACCIDENTAL_X, each
                                // further-out one ACC_STACK_X beyond).
                                let overhang =
                                    ACCIDENTAL_X + (accidentals.len() - 1) as f32 * ACC_STACK_X;
                                let entry = column_overhang.entry(key.clone()).or_insert(0.0);
                                *entry = entry.max(overhang);
                            }
                            let y = step_to_y(yo, step);
                            ys.push(y);
                            pitch_heads.entry(pitch.pitch).or_default().push(Head {
                                name,
                                key: key.clone(),
                                y,
                                step,
                                comp,
                                accidentals,
                            });
                        }
                        let drawn = has_stem(value) && !ys.is_empty();
                        let lo = ys
                            .iter()
                            .copied()
                            .fold(f32::INFINITY, f32::min)
                            .min(step_to_y(yo, reference_step(&clef)));
                        let hi = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max).max(lo);
                        stems.push(StemSeg {
                            key,
                            lo: if ys.is_empty() {
                                step_to_y(yo, reference_step(&clef))
                            } else {
                                ys.iter().copied().fold(f32::INFINITY, f32::min)
                            },
                            hi,
                            drawn,
                            comp,
                        });
                    }
                    event_stems.insert(eid, stems);
                }
                (TypedObjectId::Event(eid), LayoutContent::Rest(rest)) => {
                    let mut segs = Vec::new();
                    for (comp, (offset, value)) in components_of(&rest.components).enumerate() {
                        let time = shift_time(&rest.position, &offset);
                        // Every rest component occupies its musical onset column,
                        // whether or not a glyph is bundled for its value — an
                        // unbundled (short) rest is a traced anchor *there*, not at
                        // a default x, and later components do not vanish.
                        let key = ColumnKey::Timed(time, ColumnRole::Note);
                        keys.insert(key.clone());
                        // A bundled rest draws a glyph into the column; an unbundled
                        // one is a stroke-only anchor at the same column (so the
                        // column earns no slot — decided by occupancy below).
                        segs.push(RestSeg {
                            name: rest_glyph(value),
                            key,
                            y: yo + STAFF_HEIGHT / 2.0,
                            comp,
                        });
                    }
                    event_rests.insert(eid, segs);
                }
                (TypedObjectId::Measure(_), LayoutContent::Measure(measure)) => {
                    keys.insert(measure_column(measure));
                }
                (TypedObjectId::Measure(_), _) => {
                    // Pass 2 renders malformed/missing measure content as a final
                    // barline, so collect that fallback column here instead of
                    // letting the fallible conversion panic.
                    keys.insert(ColumnKey::End);
                }
                (TypedObjectId::StaffInstance(_), LayoutContent::Staff(content)) => {
                    // The staff instance's clef glyph occupies the lead column. The
                    // *displayed* clef is the one in force at the staff start, by
                    // time — consistent with how notes resolve their active clef.
                    let clef = active_clef(&content.clefs, &origin());
                    if clef_glyph(clef.shape).is_some() {
                        keys.insert(ColumnKey::Lead);
                        // The key signature shares the lead column; reserve its width.
                        key_sig_accs = key_sig_accs.max(key_accidentals_for(content).len());
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
        let first_col = FIRST_COLUMN_X + key_sig_accs as f32 * KEY_ACC_X;
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
            diagnostics: &mut diagnostics,
            column_members: BTreeMap::new(),
            region_glyphs: Vec::new(),
            staff_members: BTreeMap::new(),
            margin_members: Vec::new(),
            staves_in_order: Vec::new(),
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
                        ));
                    }
                }
                TypedObjectId::StaffInstance(_) => {
                    // The displayed clef is the one in force at the staff start, by
                    // time — the same query the notes use, so they always agree.
                    let clef = match content {
                        Some(LayoutContent::Staff(c)) => active_clef(&c.clefs, &origin()),
                        _ => Clef::default(),
                    };
                    match clef_glyph(clef.shape) {
                        Some(name) => {
                            let info = column(&ColumnKey::Lead);
                            let baseline = Point::new(info.x, yo + (clef.line as f32 - 1.0));
                            emit.glyph(
                                provenance,
                                name,
                                baseline,
                                band_of(staff),
                                staff,
                                info.slot,
                            );
                            // The key signature's sharp/flat zigzag: each accidental
                            // a synthesized glyph in the lead area after the clef,
                            // at its clef-relative staff position, sharing the lead
                            // column slot.
                            if let Some(LayoutContent::Staff(c)) = content {
                                for (i, accidental) in key_accidentals_for(c).iter().enumerate() {
                                    let key_provenance = Provenance::synthesized(
                                        provenance.source,
                                        SynthesisKind::Registered(KEY_SIG_SYNTHESIS),
                                        SynthesisInstanceKey(i as u128),
                                        provenance.dependencies.clone(),
                                    );
                                    emit.glyph(
                                        &key_provenance,
                                        accidental.glyph,
                                        Point::new(
                                            region_x + KEY_SIG_START + i as f32 * KEY_ACC_X,
                                            step_to_y(yo, accidental.position),
                                        ),
                                        band_of(staff),
                                        staff,
                                        info.slot,
                                    );
                                }
                            }
                        }
                        None => {
                            emit.diag(provenance.source, unbundled(clef_label(clef.shape)));
                            emit.stroke(anchor(provenance, Point::new(default_x, yo)));
                        }
                    }
                }
                TypedObjectId::Event(eid) => match content {
                    Some(LayoutContent::Note(_)) => {
                        let segs = event_stems.get(&eid).map(Vec::as_slice).unwrap_or(&[]);
                        if segs.is_empty() {
                            // A pitch-less, component-less note still needs its anchor.
                            emit.stroke(anchor(provenance, Point::new(default_x, yo)));
                        }
                        for seg in segs {
                            let info = column(&seg.key);
                            let stem_x = info.x + NOTEHEAD_STEM_X;
                            let (from, to) = if seg.drawn {
                                (
                                    Point::new(stem_x, seg.lo),
                                    Point::new(stem_x, seg.hi + STEM_LENGTH),
                                )
                            } else {
                                // A stemless value (whole note): a zero-length stem.
                                (Point::new(info.x, seg.lo), Point::new(info.x, seg.lo))
                            };
                            let prov = component_provenance(provenance, seg.comp);
                            emit.stroke(Stroke {
                                provenance: prov,
                                from,
                                to,
                                thickness: StaffSpace(STEM_THICKNESS),
                                layer: 0,
                                style: ink(),
                            });
                        }
                    }
                    Some(LayoutContent::Rest(_)) => {
                        let segs = event_rests.get(&eid).map(Vec::as_slice).unwrap_or(&[]);
                        if segs.is_empty() {
                            emit.stroke(anchor(provenance, Point::new(default_x, yo)));
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
                                Some(name) => emit.glyph(
                                    prov_ref,
                                    name,
                                    Point::new(info.x, seg.y),
                                    band_of(staff),
                                    staff,
                                    info.slot,
                                ),
                                // No bundled glyph for this (short) value: a traced
                                // anchor at the rest's *own onset column* (not a
                                // default x), with the gap surfaced. The component
                                // keeps its place; later components do not vanish.
                                None => {
                                    emit.diag(prov_ref.source, unbundled(rest_label()));
                                    emit.stroke(anchor(prov_ref, Point::new(info.x, seg.y)));
                                }
                            }
                        }
                    }
                    // A non-pitched, non-rest event (unpitched / trajectory / cue):
                    // not engraved in this tier; a traced anchor keeps it.
                    _ => emit.stroke(anchor(provenance, Point::new(default_x, yo))),
                },
                TypedObjectId::Pitch(pid) => match pitch_heads.get(&pid) {
                    Some(heads) => {
                        for head in heads {
                            let info = column(&head.key);
                            let owned;
                            let prov_ref = if head.comp == 0 {
                                provenance
                            } else {
                                owned = component_provenance(provenance, head.comp);
                                &owned
                            };
                            emit.glyph(
                                prov_ref,
                                head.name,
                                Point::new(info.x, head.y),
                                band_of(staff),
                                staff,
                                info.slot,
                            );
                            // Ledger lines: short strokes continuing the staff to a
                            // notehead above or below it, one per whole step between
                            // the staff and the note, reaching `LEDGER_LINE_EXTENSION`
                            // past each side of *this notehead's* drawn box — so a
                            // wider head (a whole note) gets a wider ledger. Synthesized
                            // from the pitch; render-svg draws strokes under glyphs at a
                            // layer, so the notehead sits over them.
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
                                    Point::new(info.x + head_left - LEDGER_LINE_EXTENSION, y),
                                    Point::new(info.x + head_right + LEDGER_LINE_EXTENSION, y),
                                    STAFF_LINE_THICKNESS,
                                ));
                            }
                            // The spelling's accidental stack: synthesized glyphs
                            // left of the notehead (innermost nearest it), at its
                            // staff position, sharing the notehead's column slot.
                            // Emitted *after* the notehead so the slot's source x
                            // stays the notehead's.
                            for (stack, accidental) in head.accidentals.iter().enumerate() {
                                let acc_provenance = Provenance::synthesized(
                                    provenance.source,
                                    SynthesisKind::Registered(ACCIDENTAL_SYNTHESIS),
                                    SynthesisInstanceKey((head.comp as u128) << 8 | stack as u128),
                                    provenance.dependencies.clone(),
                                );
                                let x = info.x - ACCIDENTAL_X - stack as f32 * ACC_STACK_X;
                                emit.glyph(
                                    &acc_provenance,
                                    accidental,
                                    Point::new(x, head.y),
                                    band_of(staff),
                                    staff,
                                    info.slot,
                                );
                            }
                        }
                    }
                    None => {
                        // An unmatched pitch (no event content reached it): a black
                        // notehead on the clef reference line, with the gap surfaced.
                        emit.diag(provenance.source, LayoutDiagnosticKind::MissingSpelling);
                        let clef = active_clef(clef_seq(staff), &origin());
                        emit.stroke(anchor(
                            provenance,
                            Point::new(default_x, step_to_y(yo, reference_step(&clef))),
                        ));
                    }
                },
                TypedObjectId::Measure(_) => {
                    let key = match content {
                        Some(LayoutContent::Measure(measure)) => measure_column(measure),
                        _ => ColumnKey::End,
                    };
                    let name = if key == ColumnKey::End {
                        "barlineFinal"
                    } else {
                        "barlineSingle"
                    };
                    let info = column(&key);
                    // The barline glyph's origin is its lower end — Bravura barlines
                    // run 0..4 staff spaces *up* from the origin — so anchoring it at
                    // the staff bottom (`yo`) makes it connect the bottom and top
                    // staff lines rather than float above the midline.
                    let baseline = Point::new(info.x, yo);
                    emit.glyph(provenance, name, baseline, band_of(staff), staff, info.slot);
                    // The time signature this measure introduces: numerator over
                    // denominator, just right of the barline, each digit a
                    // synthesized glyph sharing the barline's column slot. An
                    // unbundled digit is surfaced (the bundled metrics carry only a
                    // representative subset).
                    if let Some(LayoutContent::Measure(measure)) = content {
                        if let Some(time_signature) = measure.time_signature {
                            let center_x = info.x + TIME_SIG_X;
                            // The digit glyphs are centred on their baseline, so the
                            // numerator's baseline sits on the upper half of the
                            // staff (≈ y 3) and the denominator's on the lower (≈ y 1).
                            let lines = [
                                (0u8, time_signature.numerator, yo + 3.0),
                                (1u8, time_signature.denominator, yo + 1.0),
                            ];
                            for (role, value, baseline_y) in lines {
                                let digits = digits_of(value);
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
                // Region, Voice, GraphicObject, and every cross-cutting structure
                // (ties, slurs, beams, tuplets, spanners, markers, …) have no
                // Minimal-tier glyph; a zero-extent traced anchor keeps them.
                _ => emit.stroke(anchor(provenance, Point::new(default_x, yo))),
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
        for info in columns.values() {
            let members = column_members.get(&info.slot).cloned().unwrap_or_default();
            // Realize a slot only if a glyph occupies the column — never an empty
            // slot (which would have a spacing target but no glyph the engraver
            // could derive a source x from).
            if members.is_empty() {
                continue;
            }
            // The spring slot's natural width is uniform; the engraver computes the
            // collision-aware advance (per-slot bearings) when it re-spaces, and
            // the *source* geometry below already separates columns enough that
            // accidentals do not overlap the previous note.
            horizontal_slots.push(SpringSlot {
                id: info.slot,
                time: info.time.clone(),
                min_width: StaffSpace(1.0),
                preferred_width: StaffSpace(COLUMN_PREFERRED_WIDTH),
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
        // not O(n²). Chord members share a column slot — a second or unison may
        // genuinely overlap by design — so only cross-column neighbours carry
        // the obligation.
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
        // become break constraints on the spring slot that carries the break
        // anchor's onset — the barline column at that time when one exists
        // (a break belongs at the boundary), else the note column. An anchor
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
            let slot = [ColumnRole::Barline, ColumnRole::Note]
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

        // A staff band per manifested staff that carries glyphs, in first-glyph
        // order; an (empty) inter-staff gap band between adjacent staves; and a
        // margin band for any region-level glyphs.
        for staff in &staves_in_order {
            let layout_id = manifestation_layout_id(&TypedObjectId::Staff(*staff), region_id);
            let members = staff_members.remove(staff).unwrap_or_default();
            vertical_bands.push(VerticalBand::staff_manifestation(
                layout_id, *staff, members,
            ));
        }
        for gap in 1..staves_in_order.len() {
            let gap_id = inter_staff_gap_id(region_layout_id, gap);
            vertical_bands.push(VerticalBand::inter_staff_gap(gap_id));
        }
        if !margin_members.is_empty() {
            vertical_bands.push(VerticalBand::margin(region_layout_id, margin_members));
        }
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
        vertical_bands,
        constraints,
        break_origins,
        engraving_decisions: logical.engraving_decisions.clone(),
        diagnostics,
        catalog,
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
}

/// One component's stem geometry, computed before column x is known (carried as
/// the column key plus the staff-space `y` extent).
struct StemSeg {
    key: ColumnKey,
    lo: f32,
    hi: f32,
    drawn: bool,
    comp: usize,
}

/// One component's rest glyph (absent when the value has no bundled rest glyph).
struct RestSeg {
    name: Option<&'static str>,
    key: ColumnKey,
    y: f32,
    comp: usize,
}

/// A horizontal column the spacing pass tiles left-to-right. The clef sits in the
/// `Lead` column; notes and barlines occupy `Timed` columns (a barline before the
/// notes at the same onset); the final barline closes the region in `End`.
#[derive(Clone, PartialEq, Eq)]
enum ColumnKey {
    Lead,
    Timed(TimePoint, ColumnRole),
    End,
}

/// Within one musical time, a barline column precedes the note column.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ColumnRole {
    Barline,
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

/// The accumulators a region's engraving emits into. Glyphs (only) carry band and
/// spring-slot membership; strokes are free line primitives.
struct Emit<'a> {
    glyphs: &'a mut Vec<GlyphObject>,
    strokes: &'a mut Vec<Stroke>,
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
fn line_stroke(provenance: Provenance, from: Point, to: Point, thickness: f32) -> Stroke {
    Stroke {
        provenance,
        from,
        to,
        thickness: StaffSpace(thickness),
        layer: 0,
        style: ink(),
    }
}

/// A zero-extent, zero-width stroke at `at`: an invisible traced anchor that
/// keeps a structural object (with no Minimal-tier glyph) provenance-tracked.
fn anchor(provenance: &Provenance, at: Point) -> Stroke {
    Stroke {
        provenance: provenance.clone(),
        from: at,
        to: at,
        thickness: StaffSpace(0.0),
        layer: 0,
        style: ink(),
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

/// The column a measure's barline occupies: the final barline closes the region
/// at the right; an interior/region-end barline sits before its measure's notes.
fn measure_column(measure: &crate::logical::MeasureContent) -> ColumnKey {
    if measure.barline == BarlineKind::Final {
        ColumnKey::End
    } else {
        ColumnKey::Timed(measure.start.clone(), ColumnRole::Barline)
    }
}

/// The key signature in force at a staff's start, by resolved time (the same
/// rule as the active clef), or `None` when the staff declares no key. Absence
/// means no signature drawn — distinct from a declared C-major (which also draws
/// nothing, via an empty accidental set).
fn active_key(keys: &[PlacedKeySignature]) -> Option<KeySignature> {
    keys.iter()
        .filter(|placed| {
            matches!(
                time_cmp(&placed.time, &origin()),
                Some(Ordering::Less | Ordering::Equal)
            )
        })
        .max_by(|a, b| time_total(&a.time, &b.time))
        .or_else(|| keys.iter().min_by(|a, b| time_total(&a.time, &b.time)))
        .map(|placed| placed.key)
}

/// The key signature's accidentals (the clef-relative zigzag) at a staff's start:
/// the active key resolved under the active clef. Empty when no key is declared,
/// the key is C major, or the clef has no diatonic positions (percussion).
fn key_accidentals_for(content: &StaffContent) -> Vec<KeyAccidental> {
    match active_key(&content.keys) {
        Some(key) => key_signature(key, &active_clef(&content.clefs, &origin())),
        None => Vec::new(),
    }
}

/// The decimal digits of a time-signature number, most significant first.
fn digits_of(value: u16) -> Vec<u8> {
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

/// The `(offset, base value)` of each notated component, or a single implicit
/// quarter at offset zero when the event carries no decomposition.
fn components_of(
    components: &[crate::logical::PlacedComponent],
) -> impl Iterator<Item = (MusicalDuration, NoteValue)> + '_ {
    let implicit = components.is_empty();
    let mapped = components
        .iter()
        .map(|c| (c.offset.clone(), c.component.base_value));
    let fallback = std::iter::once((MusicalDuration::zero(), NoteValue::Quarter));
    mapped
        .chain(fallback.filter(move |_| implicit))
        .take(if implicit { 1 } else { usize::MAX })
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
        .unwrap_or_default()
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
        // Every no-collision endpoint is a notehead in a distinct column slot.
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
                assert_ne!(first.horizontal_slot, second.horizontal_slot);
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

    #[test]
    fn out_of_range_finite_stroke_thickness_is_rejected() {
        let mut c = to_constrained(&to_logical(&valid_score_rich(11)));
        let provenance = c.glyphs[0].provenance.clone();
        c.strokes.push(Stroke {
            provenance,
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
            clefs: vec![],
            keys: vec![PlacedKeySignature {
                time: TimePoint::Musical(MusicalPosition::origin()),
                key: KeySignature::new(2).expect("two sharps is a valid key"),
            }],
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
    /// denominator digit pair right of its barline, each digit synthesized from
    /// the measure and sharing the barline's column slot. An unbundled digit is
    /// surfaced as a diagnostic, not drawn at a guessed shape.
    #[test]
    fn a_time_signature_draws_a_digit_pair_after_the_barline() {
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
        assert!(fours.iter().all(|g| g.baseline.x.0 > barline.baseline.x.0));
        assert!(fours
            .iter()
            .all(|g| g.horizontal_slot == barline.horizontal_slot));
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

    /// A rest with no bundled glyph (a sixteenth) is a traced anchor at its *own
    /// onset column*, not a default x, and every component is kept (later ones do
    /// not vanish), with each unbundled value surfaced as a diagnostic.
    #[test]
    fn unbundled_rest_components_anchor_at_their_onset() {
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
                base_value: NoteValue::Sixteenth, // no bundled rest glyph
                dots: 0,
                tuplet: None,
                tied_to_next: false,
            },
            tuplet: None,
        };
        let rest = LayoutContent::Rest(RestContent {
            position: TimePoint::Musical(MusicalPosition::origin()),
            components: vec![component(0, 1), component(1, 16)],
            staff_position: None,
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

        // No rest glyph (the value is unbundled).
        assert!(c
            .glyphs
            .iter()
            .all(|g| !g.glyph.as_str().starts_with("rest")));
        // Both unbundled components are surfaced.
        let unbundled = c
            .diagnostics
            .iter()
            .filter(|d| matches!(d.kind, LayoutDiagnosticKind::UnbundledGlyph(_)))
            .count();
        assert_eq!(unbundled, 2, "each unbundled rest component is diagnosed");
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
            "an unbundled rest anchors at its onset column, not the default x"
        );
        // Stroke-only columns earn no spring slot: this region has no glyphs, so
        // no slots — the engraver's remap never faces an empty slot with no
        // source→target point.
        assert!(
            c.horizontal_slots.is_empty(),
            "a stroke-only (unbundled-rest) column creates no spring slot"
        );
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
                        clefs: vec![],
                        keys: vec![],
                    }),
                ),
                with_content(
                    TypedObjectId::Event(EventId::from_raw(eid)),
                    staff,
                    LayoutContent::Note(NoteContent {
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
}
