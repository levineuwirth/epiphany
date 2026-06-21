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

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{StaffId, TypedObjectId, WallClockTime};

use crate::engraving::EngravingDecision;
use crate::glyph::{
    metrics, BravuraCatalog, GlyphCatalog, GlyphCatalogIdentity, GlyphReference, BRAVURA_METRICS,
};
use crate::logical::{LogicalLayoutIR, ScoreVersion};
use crate::provenance::{manifestation_layout_id, LayoutObjectId, Provenance};
use crate::solver::SpringSlotId;
use crate::spatial::{BoundingBox, Point, Rect, StaffSpace};
use crate::time_axis::TimePoint;
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

/// The constrained IR: composite objects flattened to glyphs, with the vertical
/// bands and engraving decisions that the solver consumes alongside them
/// (Chapter 7 §"Constraints").
#[derive(Clone, PartialEq, Debug)]
pub struct ConstrainedLayoutIR {
    pub source: ScoreVersion,
    pub regions: Vec<ConstrainedLayoutRegion>,
    pub horizontal_slots: Vec<SpringSlot>,
    pub glyphs: Vec<GlyphObject>,
    pub vertical_bands: Vec<VerticalBand>,
    pub constraints: Vec<LayoutConstraint>,
    pub engraving_decisions: Vec<EngravingDecision>,
    pub catalog: GlyphCatalogIdentity,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct GlyphStyle {
    /// RGBA color in `0xRRGGBBAA` form.
    pub rgba: u32,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConstrainedLayoutRegion {
    pub provenance: Provenance,
    pub glyphs: Vec<GlyphObjectId>,
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
    InvalidGlyphBounds(GlyphObjectId),
    /// A constraint references a glyph that is not in the glyph set.
    UnknownConstraintGlyph(GlyphObjectId),
    /// A break constraint references a spring slot that does not exist.
    UnknownConstraintSlot(SpringSlotId),
    /// A `PositionWithin` constraint carries a non-finite or inverted region.
    InvalidConstraintRegion(GlyphObjectId),
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
        Ok(())
    }
}

/// Picks a bundled SMuFL glyph for a source, deterministically (a pure function
/// of the source kind, so it never depends on traversal position).
pub(crate) fn glyph_name_for(source: &TypedObjectId) -> GlyphReference {
    GlyphReference::borrowed(
        BRAVURA_METRICS[(source.discriminant() as usize) % BRAVURA_METRICS.len()]
            .name
            .as_ref(),
    )
}

/// Flattens [`LogicalLayoutIR`] into [`ConstrainedLayoutIR`]: one glyph per
/// layout object (including the region object itself), each laid out
/// left-to-right on the `1/1024` grid, with provenance preserved
/// object-for-object.
///
/// **Each glyph is routed to the band of its own staff** (Chapter 7 §"Vertical
/// Bands"): a region manifesting two staves gets a staff band per staff, with
/// each glyph a member of exactly its staff's band — never every staff's band.
/// Region-level glyphs (the region object, cross-cutting, free-graphic) go to a
/// margin band. Multi-staff regions also carry empty `InterStaffGap` spring
/// bands between consecutive staves. Staff-band ids are the staff *layout
/// object's* manifestation id, so a staff manifested in two regions gets two
/// distinct bands.
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
    let mut vertical_bands = Vec::new();
    let mut horizontal_slots = Vec::new();
    let mut constrained_regions = Vec::new();
    let mut column: i64 = 0;

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

        // (provenance, owning staff) for the region object, then its contents.
        let mut specs: Vec<(&Provenance, Option<StaffId>)> =
            std::iter::once((&region.provenance, None))
                .chain(region.objects.iter().map(|o| (o.provenance(), o.staff())))
                .collect();
        specs.extend(
            logical
                .cross_region
                .iter()
                .filter(|object| object.regions.first() == Some(&region_id))
                .map(|object| (&object.provenance, object.staff)),
        );

        // Distinct staves in first-appearance order, and members per band.
        let mut staves_in_order: Vec<StaffId> = Vec::new();
        let mut staff_members: BTreeMap<StaffId, Vec<GlyphObjectId>> = BTreeMap::new();
        let mut margin_members: Vec<GlyphObjectId> = Vec::new();
        let mut region_glyphs = Vec::new();

        for (provenance, staff) in specs {
            let band = band_of(staff);
            let glyph = make_glyph(provenance, column, band);
            column += 1;
            let gid = glyph.id();
            horizontal_slots.push(SpringSlot {
                id: glyph.horizontal_slot,
                time: TimePoint::WallClock(WallClockTime(column - 1)),
                min_width: StaffSpace(1.0),
                preferred_width: StaffSpace(1.5),
                max_width: None,
                stretch_factor: 1.0,
                compress_factor: 1.0,
                members: vec![gid],
            });
            region_glyphs.push(gid);
            match staff {
                Some(s) => {
                    if !staves_in_order.contains(&s) {
                        staves_in_order.push(s);
                    }
                    staff_members.entry(s).or_default().push(gid);
                }
                None => margin_members.push(gid),
            }
            glyphs.push(glyph);
        }

        // A staff band per manifested staff, in first-appearance order.
        for staff in &staves_in_order {
            let layout_id = manifestation_layout_id(&TypedObjectId::Staff(*staff), region_id);
            let members = staff_members.remove(staff).unwrap_or_default();
            vertical_bands.push(VerticalBand::staff_manifestation(
                layout_id, *staff, members,
            ));
        }
        // An (empty) inter-staff gap band between each pair of adjacent staves.
        for gap in 1..staves_in_order.len() {
            let gap_id = inter_staff_gap_id(region_layout_id, gap);
            vertical_bands.push(VerticalBand::inter_staff_gap(gap_id));
        }
        // A margin band for region-level glyphs, if any.
        if !margin_members.is_empty() {
            vertical_bands.push(VerticalBand::margin(region_layout_id, margin_members));
        }
        constrained_regions.push(ConstrainedLayoutRegion {
            provenance: region.provenance.clone(),
            glyphs: region_glyphs,
        });
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
        vertical_bands,
        constraints: Vec::new(),
        engraving_decisions: logical.engraving_decisions.clone(),
        catalog,
    })
}

/// Builds a glyph for a provenance at horizontal `column`, baseline one staff
/// space apart per column (Chapter 7 §7.2 staff-space coordinates), in `band`.
fn make_glyph(provenance: &Provenance, column: i64, band: VerticalBandId) -> GlyphObject {
    let glyph = glyph_name_for(&provenance.source);
    GlyphObject {
        bounding_box: metrics(glyph.as_str())
            .expect("pipeline glyph names are bundled")
            .bounding_box(),
        glyph,
        horizontal_slot: SpringSlotId(provenance.stable_id.0),
        baseline: Point::new(column as f32, 0.0),
        vertical_band: band,
        anchor: Point::ORIGIN,
        layer: 0,
        style: GlyphStyle { rgba: 0x0000_00ff },
        provenance: provenance.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logical::to_logical;
    use epiphany_core::generators::valid_score_rich;
    use std::collections::BTreeSet;

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
    /// that staff's band only.
    #[test]
    fn multi_staff_region_routes_per_staff_with_a_gap_band() {
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR};
        use crate::provenance::Provenance;
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use crate::vertical_band::VerticalBandKind;
        use epiphany_core::{EventId, RegionId, StaffId};

        let region = RegionId::from_raw(1);
        let region_src = TypedObjectId::Region(region);
        let staff_a = StaffId::from_raw(10);
        let staff_b = StaffId::from_raw(20);
        let manifested = |src: TypedObjectId, staff: StaffId| {
            LayoutObject::from_projection(Provenance::manifested(src, region, vec![]), Some(staff))
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(region_src, vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff_a, staff_b],
                },
                objects: vec![
                    manifested(TypedObjectId::Staff(staff_a), staff_a),
                    manifested(TypedObjectId::Staff(staff_b), staff_b),
                    manifested(TypedObjectId::Event(EventId::from_raw(1)), staff_a),
                    manifested(TypedObjectId::Event(EventId::from_raw(2)), staff_b),
                ],
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

        // Staff A's two glyphs (staff object + event) are in A's band only.
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
