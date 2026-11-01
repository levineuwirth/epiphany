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

/// What a vertical band represents (Chapter 7: `VerticalBandKind`), and so
/// what its height means. Heights are in staff spaces, and each kind's is
/// realized as follows:
///
/// - A **staff** band's height is its staff's box, the four spaces between
///   its outer lines. It is not solved: the staff's content (ledger lines,
///   stems, slurs, its clef) reaches beyond it, and that content, not the box,
///   is what the gap bands on either side measure from.
/// - An **inter-staff gap** band's height is a *line distance*: from the
///   bottom line of the upper of two adjacent staves of a system to the top
///   line of the lower. Its `min_height` and `preferred_height` are the floor
///   between them, its `max_height` the cap a page's spread raises it to, its
///   `stretch_factor` the spread's weight for a plain gap. The engraver sets
///   the staves by their outlines, never nearer than the floor but where a
///   system would otherwise stand taller than its page; the quality census
///   (`req:qmc:vertical`) measures the realized line distance against the
///   floor and what the ink asks.
/// - An **inter-system gap** band's height is likewise the line distance
///   between two consecutive systems on a page, from the last staff's bottom
///   line to the next system's first staff's top line, with its floor, its
///   cap and its weight in the spread.
/// - A **margin** band holds a region's content that belongs to no staff; its
///   height is not solved.
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
    /// **Its height is a line distance**, from the upper staff's bottom line
    /// to the lower's top line (X5d). Its floor is a stave's height, 4.0, as
    /// Gould asks staves stand at least (*Behind Bars*, p. 488): two staves
    /// stand no nearer, but in a system that would otherwise stand taller than
    /// its page, where the engraver compresses it toward 3.5, MuseScore's
    /// `minStaffSpread` (D109 Q1), which the quality census reads as the
    /// crowding it is. It stretches no further than MuseScore's
    /// `maxStaffSpread` 20 when a page is spread, at a weight of 1.0; a gap at
    /// a bracket's or a brace's edge weighs more, and one inside a brace is
    /// capped at 6.5, by the staff groups the engraver reads.
    ///
    /// (Through X5d it was an ink clearance between whole extents, 5.0
    /// preferred, which no orchestral system could realize on its paper.)
    pub fn inter_staff_gap(id: VerticalBandId) -> Self {
        VerticalBand {
            id,
            kind: VerticalBandKind::InterStaffGap,
            min_height: StaffSpace(4.0),
            preferred_height: StaffSpace(4.0),
            max_height: Some(StaffSpace(20.0)),
            stretch_factor: 1.0,
            compress_factor: 1.0,
            members: Vec::new(),
        }
    }

    /// An inter-system gap band used during page casting-off: a line distance
    /// whose floor is MuseScore's `minSystemSpread` 8.5, its cap
    /// `maxSystemSpread` 32 and its weight in a page's spread `spreadSystem`
    /// 2.5 (D109 Q3).
    pub fn inter_system_gap(id: VerticalBandId) -> Self {
        VerticalBand {
            id,
            kind: VerticalBandKind::InterSystemGap,
            min_height: StaffSpace(8.5),
            preferred_height: StaffSpace(8.5),
            max_height: Some(StaffSpace(32.0)),
            stretch_factor: 2.5,
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
