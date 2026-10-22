//! The expression family once its system is set (X4c): the room its members
//! take across, and the placement of those a solver moves off their staff.
//!
//! A member placed off its staff (a fermata, a dynamic, a syllable, a tempo
//! mark; `FamilyMark::placement`) stands where the projection put it, at
//! MuseScore's default distance from its staff, until the system is set.
//! Then, staff by staff, its ink is moved away from the staff until it
//! clears the ink already there by its `min_distance`, in MuseScore's order
//! (`SystemLayout::layoutSystemElements`), each member placed joining the
//! ink the next is placed against, so what comes first stands nearest the
//! staff, as `Autoplace::autoplaceSegmentElement` does. Members of one row in
//! a system (a verse's syllables, a dynamic and the hairpins it meets) take
//! the height the furthest of them needs. A member is never moved toward its
//! staff.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_layout_ir::{
    ConstrainedLayoutIR, FamilyMark, GlyphObjectId, PlaceOrder, Room, TextObject, VerticalBandId,
};

use crate::clearance::InkRect;

/// The glyphs of the family that take no room across: their ink meets other
/// ink freely, as MuseScore lets an articulation's or a dynamic's.
pub(crate) fn roomless_glyphs(input: &ConstrainedLayoutIR) -> BTreeSet<GlyphObjectId> {
    input
        .family
        .iter()
        .filter(|mark| mark.room == Room::None)
        .flat_map(|mark| mark.glyphs.iter().copied())
        .collect()
}

/// Each text run of the family with the room it takes.
pub(crate) fn texts_with_room(input: &ConstrainedLayoutIR) -> Vec<(&TextObject, Room)> {
    let room: BTreeMap<GlyphObjectId, Room> = input
        .family
        .iter()
        .flat_map(|mark| mark.texts.iter().map(move |id| (*id, mark.room)))
        .collect();
    input
        .texts
        .iter()
        .filter_map(|text| Some((text, *room.get(&text.id())?)))
        .collect()
}

/// Which member of the family each placed primitive belongs to, by id: its
/// index in `ConstrainedLayoutIR::family`.
pub(crate) struct Placed {
    pub glyphs: BTreeMap<GlyphObjectId, usize>,
    pub strokes: BTreeMap<GlyphObjectId, usize>,
    pub texts: BTreeMap<GlyphObjectId, usize>,
}

impl Placed {
    pub(crate) fn of(input: &ConstrainedLayoutIR) -> Self {
        let mut placed = Placed {
            glyphs: BTreeMap::new(),
            strokes: BTreeMap::new(),
            texts: BTreeMap::new(),
        };
        for (f, mark) in input.family.iter().enumerate() {
            if mark.placement.is_none() {
                continue;
            }
            placed.glyphs.extend(mark.glyphs.iter().map(|id| (*id, f)));
            placed
                .strokes
                .extend(mark.strokes.iter().map(|id| (*id, f)));
            placed.texts.extend(mark.texts.iter().map(|id| (*id, f)));
        }
        placed
    }
}

/// One system's share of a placed member: its ink there, where the system
/// sets it across, at the height the projection gave it.
pub(crate) struct Piece {
    pub family: usize,
    pub system: usize,
    pub boxes: Vec<InkRect>,
}

/// How far each piece moves, up positive: in each system and staff, every
/// piece placed in order (`PlaceOrder`, then left to right, then by member),
/// away from the staff until it clears the ink under it by its distance; a
/// row's pieces in one system together, at the furthest any of them needs.
/// `family` is the layout's (`ConstrainedLayoutIR::family`), and `ink` each
/// system's and staff's ink the family does not place.
pub(crate) fn place(
    family: &[FamilyMark],
    pieces: &[Piece],
    ink: &BTreeMap<(usize, VerticalBandId), Vec<InkRect>>,
) -> Vec<f32> {
    let mut shift = vec![0.0f32; pieces.len()];
    // Pieces by their system and staff, in placing order.
    let mut groups: BTreeMap<(usize, VerticalBandId), Vec<usize>> = BTreeMap::new();
    for (i, piece) in pieces.iter().enumerate() {
        if family[piece.family].placement.is_some() && !piece.boxes.is_empty() {
            groups
                .entry((piece.system, family[piece.family].band))
                .or_default()
                .push(i);
        }
    }
    let left = |i: usize| {
        pieces[i]
            .boxes
            .iter()
            .map(|b| b.left)
            .fold(f32::INFINITY, f32::min)
    };
    let order = |i: usize| -> PlaceOrder {
        family[pieces[i].family]
            .placement
            .expect("grouped pieces are placed")
            .order
    };
    for (key, mut members) in groups {
        members.sort_by(|&a, &b| {
            order(a)
                .cmp(&order(b))
                .then(left(a).total_cmp(&left(b)))
                .then(pieces[a].family.cmp(&pieces[b].family))
        });
        let mut sky: Vec<InkRect> = ink.get(&key).cloned().unwrap_or_default();
        let mut done = vec![false; members.len()];
        for k in 0..members.len() {
            if done[k] {
                continue;
            }
            let first = members[k];
            let placement = family[pieces[first].family]
                .placement
                .expect("grouped pieces are placed");
            // The piece, and the rest of its row in this system.
            let row: Vec<usize> = match placement.row {
                Some(r) => (k..members.len())
                    .filter(|&j| {
                        !done[j]
                            && family[pieces[members[j]].family]
                                .placement
                                .and_then(|p| p.row)
                                == Some(r)
                    })
                    .collect(),
                None => vec![k],
            };
            let need = |i: usize| -> f32 {
                let p = family[pieces[i].family]
                    .placement
                    .expect("grouped pieces are placed");
                let mut need = 0.0f32;
                for b in &pieces[i].boxes {
                    for s in sky.iter().filter(|s| s.left < b.right && s.right > b.left) {
                        if p.above {
                            need = need.max(s.top + p.min_distance - b.bottom);
                        } else {
                            need = need.min(s.bottom - p.min_distance - b.top);
                        }
                    }
                }
                need
            };
            let needs: Vec<f32> = row.iter().map(|&j| need(members[j])).collect();
            let by = if placement.above {
                needs.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            } else {
                needs.iter().copied().fold(f32::INFINITY, f32::min)
            };
            for &j in &row {
                done[j] = true;
                let i = members[j];
                shift[i] = by;
                sky.extend(pieces[i].boxes.iter().map(|b| InkRect {
                    bottom: b.bottom + by,
                    top: b.top + by,
                    ..*b
                }));
            }
        }
    }
    shift
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_layout_ir::Autoplace;

    const STAFF: VerticalBandId = VerticalBandId(7);

    fn member(above: bool, order: PlaceOrder, row: Option<u128>) -> FamilyMark {
        FamilyMark {
            band: STAFF,
            glyphs: Vec::new(),
            strokes: Vec::new(),
            texts: Vec::new(),
            room: Room::None,
            placement: Some(Autoplace {
                above,
                order,
                min_distance: 0.5,
                row,
            }),
        }
    }

    fn rect(left: f32, right: f32, bottom: f32, top: f32) -> InkRect {
        InkRect {
            left,
            right,
            bottom,
            top,
        }
    }

    fn piece(family: usize, boxes: Vec<InkRect>) -> Piece {
        Piece {
            family,
            system: 0,
            boxes,
        }
    }

    fn sky(rects: Vec<InkRect>) -> BTreeMap<(usize, VerticalBandId), Vec<InkRect>> {
        BTreeMap::from([((0, STAFF), rects)])
    }

    /// A member above its staff rises until it clears by its distance the
    /// ink it stands over, and only that ink; one already clear stays where
    /// the projection put it, and below the staff a member sinks.
    #[test]
    fn a_member_moves_off_the_ink_it_meets_and_never_toward_its_staff() {
        let family = [
            member(true, PlaceOrder::Fermata, None),
            member(true, PlaceOrder::Fermata, None),
            member(false, PlaceOrder::Dynamic, None),
        ];
        let pieces = [
            // Over a note reaching 6.0: its bottom must reach 6.5.
            piece(0, vec![rect(0.0, 1.0, 5.0, 6.0)]),
            // Already 3 spaces clear of the ink under it.
            piece(1, vec![rect(10.0, 11.0, 9.0, 10.0)]),
            // Under a note reaching -2.0: its top must reach -2.5.
            piece(2, vec![rect(20.0, 21.0, -2.2, -1.2)]),
        ];
        let ink = sky(vec![
            rect(0.5, 1.5, 0.0, 6.0),
            rect(10.0, 11.0, 0.0, 6.0),
            rect(20.0, 21.0, -2.0, 4.0),
            // Tall, but beside the first member: not under it.
            rect(1.5, 2.0, 0.0, 30.0),
        ]);
        let shift = place(&family, &pieces, &ink);
        assert!((shift[0] - 1.5).abs() < 1e-6, "{shift:?}");
        assert_eq!(shift[1], 0.0);
        assert!((shift[2] + 1.3).abs() < 1e-6, "{shift:?}");
    }

    /// Members are placed in MuseScore's order, each joining the ink the
    /// next is placed against, so one placed first stands nearer its staff
    /// whatever their order in the family; and the members of one row stand
    /// at the height the furthest of them needs.
    #[test]
    fn earlier_members_stand_nearer_and_a_row_stands_together() {
        // A tempo mark listed before a fermata over the same note.
        let family = [
            member(true, PlaceOrder::Tempo, None),
            member(true, PlaceOrder::Fermata, None),
        ];
        let pieces = [
            piece(0, vec![rect(0.0, 1.0, 4.5, 5.5)]),
            piece(1, vec![rect(0.0, 1.0, 4.5, 5.5)]),
        ];
        let ink = sky(vec![rect(0.0, 1.0, 0.0, 4.0)]);
        let shift = place(&family, &pieces, &ink);
        // The fermata first, already clear of the note; the tempo mark over
        // it, its bottom at the fermata's top, 5.5, plus the distance.
        assert!(shift[1].abs() < 1e-6, "{shift:?}");
        assert!((shift[0] - 1.5).abs() < 1e-6, "{shift:?}");

        // Two syllables of a verse, one over a low note and one over a high
        // one: both take the height the high one needs.
        let family = [
            member(false, PlaceOrder::Lyric, Some(1)),
            member(false, PlaceOrder::Lyric, Some(1)),
        ];
        let pieces = [
            piece(0, vec![rect(0.0, 1.0, -3.0, -2.0)]),
            piece(1, vec![rect(5.0, 6.0, -3.0, -2.0)]),
        ];
        let ink = sky(vec![rect(0.0, 1.0, -1.0, 4.0), rect(5.0, 6.0, -4.0, 4.0)]);
        let shift = place(&family, &pieces, &ink);
        assert!((shift[0] + 2.5).abs() < 1e-6, "{shift:?}");
        assert_eq!(shift[0], shift[1]);
    }
}
