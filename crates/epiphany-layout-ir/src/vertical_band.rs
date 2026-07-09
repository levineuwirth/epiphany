//! The vertical-band model (Chapter 7 §"Vertical Bands").
//!
//! "Vertical layout uses the same spring model. A *vertical band* is a
//! horizontal slice of the canvas (typically a staff or an inter-staff gap)
//! with its own spring parameters." The solver resolves vertical positions by
//! treating bands as springs, exactly as it treats horizontal time slots
//! (Chapter 7 §"Spring-based spacing").
//!
//! All dimensions are staff-space `f32` per the Chapter 7 §7.2 IR-coordinate
//! rule: the elastic spring parameters (`stretch_factor`, `compress_factor`) and
//! the band heights ([`StaffSpace`]) alike are layout-engine inputs, quantized
//! only if and when a band extent is serialized into canonical output.

use epiphany_core::{StaffId, TypedObjectId};
use epiphany_determinism::{DomainTag, Preimage};

use crate::constrained::GlyphObjectId;
use crate::provenance::{stable_layout_id, LayoutObjectId};
use crate::spatial::StaffSpace;

/// A stable identifier for a vertical band (Chapter 7: `VerticalBandId`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VerticalBandId(pub u128);

/// Derives an inter-staff gap id in its own type-prefixed preimage namespace.
/// It cannot alias the region's margin id by arithmetic construction.
pub fn inter_staff_gap_id(region: LayoutObjectId, gap_index: usize) -> VerticalBandId {
    let mut preimage = Preimage::new(DomainTag::CONFLICT);
    preimage.push_bytes(b"vertical-band/inter-staff-gap");
    preimage.push_u64_le((region.0 >> 64) as u64);
    preimage.push_u64_le(region.0 as u64);
    preimage.push_u64_le(gap_index as u64);
    VerticalBandId(preimage.finish_trunc128())
}

/// What a vertical band represents (Chapter 7: `VerticalBandKind`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VerticalBandKind {
    /// A staff's own band.
    Staff(StaffId),
    /// The gap between two staves of a system.
    InterStaffGap,
    /// The gap between two systems on a page.
    InterSystemGap,
    /// A page-margin band.
    MarginBand,
}

/// A vertical band: a horizontal slice of the canvas with its own spring
/// parameters (Chapter 7 §"Vertical Bands"). The constraint solver consumes
/// bands uniformly, just like horizontal spring slots.
#[derive(Clone, PartialEq, Debug)]
pub struct VerticalBand {
    pub id: VerticalBandId,
    pub kind: VerticalBandKind,
    /// The smallest the band may compress to, in staff spaces.
    pub min_height: StaffSpace,
    /// The band's natural height under unconstrained spacing, in staff spaces.
    pub preferred_height: StaffSpace,
    /// The largest the band may stretch to, if bounded, in staff spaces.
    pub max_height: Option<StaffSpace>,
    /// How readily the band stretches when more height is available.
    pub stretch_factor: f32,
    /// How readily the band compresses when less height is available.
    pub compress_factor: f32,
    /// The glyphs belonging to this band.
    pub members: Vec<GlyphObjectId>,
}

impl VerticalBand {
    /// A default staff band, with the band id derived from the staff source
    /// alone. Use this for a staff with a single manifestation; for a staff
    /// manifested in a specific region, use [`VerticalBand::staff_manifestation`]
    /// so two manifestations get two distinct band ids.
    pub fn staff(staff: StaffId, members: Vec<GlyphObjectId>) -> Self {
        Self::with_id(
            VerticalBandId(stable_layout_id(&TypedObjectId::Staff(staff)).0),
            staff,
            members,
        )
    }

    /// A staff band for a specific manifestation, identified by the staff
    /// layout object's stable id (`(staff, region)` — see
    /// [`crate::Provenance::manifested`]). Two manifestations of one staff thus
    /// get two distinct band ids, mirroring the two distinct staff layout
    /// objects.
    pub fn staff_manifestation(
        manifestation: LayoutObjectId,
        staff: StaffId,
        members: Vec<GlyphObjectId>,
    ) -> Self {
        Self::with_id(VerticalBandId(manifestation.0), staff, members)
    }

    /// A margin band (for region content with no staff, e.g. a free-graphic
    /// region), identified by `id` (typically the region's layout id).
    pub fn margin(id: LayoutObjectId, members: Vec<GlyphObjectId>) -> Self {
        let four_spaces = StaffSpace(4.0);
        VerticalBand {
            id: VerticalBandId(id.0),
            kind: VerticalBandKind::MarginBand,
            min_height: four_spaces,
            preferred_height: four_spaces,
            max_height: None,
            stretch_factor: 1.0,
            compress_factor: 1.0,
            members,
        }
    }

    /// An inter-staff gap band: the empty (member-less) spacing region between
    /// two staves of a system (Chapter 7 §"Vertical Bands": `InterStaffGap`). Its
    /// height is a spring the solver resolves; it carries no glyphs.
    ///
    /// **Its height is an INK CLEARANCE**, not a distance between staff lines:
    /// the vertical separation between the two staves' outermost *content* —
    /// ledger lines, stems, slurs, everything. That is the unit the Quality
    /// Metric Catalog's `vertical_density_penalty` measures against
    /// (`req:qmc:vertical`, "the adjacent content extents the band separates"),
    /// so the solve and the metric read one number.
    ///
    /// `preferred` is a staff height plus a space: two staves whose ink is that
    /// far apart read as separate systems of lines without wasting the page.
    /// Plain ledgered content then lands at a staff *pitch* of about 10.6 staff
    /// spaces — near conventional two-staff spacing — while ledgered or slurred
    /// content pushes the staves further apart on its own. `min` is the hardest
    /// squeeze a compressing solve may apply.
    ///
    /// (The earlier `preferred = 2.0` was a placeholder reconciled with nothing:
    /// it is neither the 8.0 staff-box gap the constrained stage's fixed
    /// `SYSTEM_STAFF_PITCH` produces, nor the ~6.4 ink clearance that stacking
    /// leaves for plain content. Realizing it would have crushed a relaxed
    /// system to a pitch of ~7.6.)
    pub fn inter_staff_gap(id: VerticalBandId) -> Self {
        VerticalBand {
            id,
            kind: VerticalBandKind::InterStaffGap,
            min_height: StaffSpace(2.0),
            preferred_height: StaffSpace(5.0),
            max_height: None,
            stretch_factor: 1.0,
            compress_factor: 1.0,
            members: Vec::new(),
        }
    }

    /// An inter-system gap band used during page casting-off.
    pub fn inter_system_gap(id: VerticalBandId) -> Self {
        VerticalBand {
            id,
            kind: VerticalBandKind::InterSystemGap,
            min_height: StaffSpace(2.0),
            preferred_height: StaffSpace(4.0),
            max_height: None,
            stretch_factor: 1.0,
            compress_factor: 1.0,
            members: Vec::new(),
        }
    }

    fn with_id(id: VerticalBandId, staff: StaffId, members: Vec<GlyphObjectId>) -> Self {
        // 4 staff spaces between the outer lines of a 5-line staff.
        let four_spaces = StaffSpace(4.0);
        VerticalBand {
            id,
            kind: VerticalBandKind::Staff(staff),
            min_height: four_spaces,
            preferred_height: four_spaces,
            max_height: None,
            stretch_factor: 1.0,
            compress_factor: 1.0,
            members,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staff_band_is_stable_and_carries_members() {
        let staff = StaffId::from_raw(0x42);
        let members = vec![GlyphObjectId(1), GlyphObjectId(2)];
        let a = VerticalBand::staff(staff, members.clone());
        let b = VerticalBand::staff(staff, members);
        assert_eq!(a.id, b.id);
        assert_eq!(a.kind, VerticalBandKind::Staff(staff));
        assert_eq!(a.members.len(), 2);
        // A different staff → a different band id.
        let other = VerticalBand::staff(StaffId::from_raw(0x43), vec![]);
        assert_ne!(a.id, other.id);
    }

    #[test]
    fn inter_staff_gap_ids_are_stable_and_separate_from_the_region() {
        let region = LayoutObjectId(42);
        assert_eq!(inter_staff_gap_id(region, 1), inter_staff_gap_id(region, 1));
        assert_ne!(inter_staff_gap_id(region, 1), inter_staff_gap_id(region, 2));
        assert_ne!(inter_staff_gap_id(region, 1), VerticalBandId(region.0));
    }
}
