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
    Autoplace, ConstrainedLayoutIR, FamilyMark, GlyphObjectId, PlaceOrder, Room, TextObject,
    VerticalBandId,
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
#[derive(Clone)]
pub(crate) struct Piece {
    pub family: usize,
    pub system: usize,
    pub boxes: Vec<InkRect>,
}

/// How far each piece moves, up positive: in each system and staff, every
/// piece placed in order (`PlaceOrder`, then left to right, then by member),
/// away from the staff until it clears the ink under it by its distance,
/// each joining the ink the next is placed against; then, once all of one
/// order are placed, a row's pieces in one system moved further out until
/// their optical centres stand at the outermost of theirs
/// (`AlignmentLayout::alignItemsWithTheirSnappingChain`). `family` is the
/// layout's (`ConstrainedLayoutIR::family`), and `ink` each system's and
/// staff's ink the family does not place.
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
    let placement = |i: usize| -> Autoplace {
        family[pieces[i].family]
            .placement
            .expect("grouped pieces are placed")
    };
    for (key, mut members) in groups {
        members.sort_by(|&a, &b| {
            placement(a)
                .order
                .cmp(&placement(b).order)
                .then(left(a).total_cmp(&left(b)))
                .then(pieces[a].family.cmp(&pieces[b].family))
        });
        let mut sky: Vec<InkRect> = ink.get(&key).cloned().unwrap_or_default();
        for same in members.chunk_by(|&a, &b| placement(a).order == placement(b).order) {
            // A verse is placed against the ink under it, not against the
            // other verses, which keep their lines from it
            // (`LyricsLayout::checkCollisionsWithStaffElements`).
            let verses = placement(same[0]).order == PlaceOrder::Lyric;
            let under = sky.len();
            // Each piece of this order, where its boxes join the sky.
            let mut joined: Vec<(usize, std::ops::Range<usize>)> = Vec::new();
            for &i in same {
                let p = placement(i);
                let mut need = 0.0f32;
                let seen = if verses { &sky[..under] } else { &sky[..] };
                for b in &pieces[i].boxes {
                    for s in seen.iter().filter(|s| s.left < b.right && s.right > b.left) {
                        if p.above {
                            need = need.max(s.top + p.min_distance - b.bottom);
                        } else {
                            need = need.min(s.bottom - p.min_distance - b.top);
                        }
                    }
                }
                shift[i] = need;
                let from = sky.len();
                sky.extend(pieces[i].boxes.iter().map(|b| InkRect {
                    bottom: b.bottom + need,
                    top: b.top + need,
                    ..*b
                }));
                joined.push((i, from..sky.len()));
            }
            // Each row of this order, aligned at its outermost centre.
            let mut rows: BTreeMap<u128, Vec<(usize, std::ops::Range<usize>)>> = BTreeMap::new();
            for (i, at) in joined {
                if let Some(row) = placement(i).row {
                    rows.entry(row.id).or_default().push((i, at));
                }
            }
            // A verse moved moves the verses outside it as far
            // (`moveThisVerseAndOuterOnes`): in a row's order, the verses' own.
            let mut outer = 0.0f32;
            for row in rows.values() {
                let centres: Vec<f32> = row
                    .iter()
                    .map(|&(i, _)| placement(i).row.expect("a row's member").centre + shift[i])
                    .collect();
                let above = placement(row[0].0).above;
                let target = if above {
                    centres.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                } else {
                    centres.iter().copied().fold(f32::INFINITY, f32::min)
                };
                // How far the row's members moved, at most; for verses, at
                // least as far as the verse inside.
                let moved = row
                    .iter()
                    .zip(&centres)
                    .map(|(&(i, _), &c)| target - c + shift[i])
                    .fold(0.0f32, |a, m| if above { a.max(m) } else { a.min(m) });
                let extra = if verses {
                    let carried = if above {
                        outer.max(moved)
                    } else {
                        outer.min(moved)
                    };
                    outer = carried;
                    carried - moved
                } else {
                    0.0
                };
                for ((i, at), centre) in row.iter().zip(centres) {
                    let delta = target - centre + extra;
                    shift[*i] += delta;
                    for b in &mut sky[at.clone()] {
                        b.bottom += delta;
                        b.top += delta;
                    }
                }
            }
        }
    }
    shift
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_layout_ir::Row;

    const STAFF: VerticalBandId = VerticalBandId(7);

    fn member(above: bool, order: PlaceOrder, row: Option<Row>) -> FamilyMark {
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
                between: None,
            }),
            line: None,
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
        let row = Some(Row {
            id: 1,
            centre: -2.5,
        });
        let family = [
            member(false, PlaceOrder::Lyric, row),
            member(false, PlaceOrder::Lyric, row),
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

    /// A row aligns its members on their optical centres, not their ink: a
    /// dynamic, whose centre stands half its x-height over its baseline, and
    /// the hairpin it meets, whose centre is its axis, each placed clear of
    /// what is under it, then both moved out to the outermost centre; a
    /// member of another row stays where it was placed.
    #[test]
    fn a_row_aligns_its_members_centres_at_the_outermost() {
        let row = |id: u128, centre: f32| Some(Row { id, centre });
        let family = [
            // A dynamic, baseline at -2.0, centre at -1.54.
            member(false, PlaceOrder::Dynamic, row(7, -1.54)),
            // Its hairpin, axis at -1.75, over a note reaching -3.0.
            member(false, PlaceOrder::Dynamic, row(7, -1.75)),
            // Another chain's dynamic, alone.
            member(false, PlaceOrder::Dynamic, row(8, -1.54)),
        ];
        let pieces = [
            piece(0, vec![rect(0.0, 1.5, -2.4, -0.9)]),
            piece(1, vec![rect(2.0, 8.0, -2.325, -1.175)]),
            piece(2, vec![rect(20.0, 21.5, -2.4, -0.9)]),
        ];
        let ink = sky(vec![rect(4.0, 5.0, -3.0, 4.0)]);
        let shift = place(&family, &pieces, &ink);
        // The hairpin clears the note by its distance: its top to -3.5.
        assert!((shift[1] - (-3.5 - -1.175)).abs() < 1e-5, "{shift:?}");
        // The dynamic moves so that its centre meets the hairpin's.
        let centre = -1.75 + shift[1];
        assert!((shift[0] - (centre - -1.54)).abs() < 1e-5, "{shift:?}");
        // The other chain stays.
        assert_eq!(shift[2], 0.0);

        // A member of a later order is placed against the row as aligned:
        // a syllable under the dynamic clears it where the row set it.
        let mut family = family.to_vec();
        family.push(member(false, PlaceOrder::Lyric, None));
        let mut pieces = pieces.to_vec();
        pieces.push(piece(3, vec![rect(0.0, 1.5, -3.0, -2.0)]));
        let shift = place(&family, &pieces, &ink);
        let dynamic_bottom = -2.4 + shift[0];
        assert!(
            (shift[3] - (dynamic_bottom - 0.5 - -2.0)).abs() < 1e-5,
            "{shift:?}"
        );
    }

    /// A verse is placed against the ink under it, never against another
    /// verse, which keeps its line from it: where a low note moves the first
    /// verse down, the second moves as far, and where only the second's own
    /// ink needs it, the second alone.
    #[test]
    fn verses_move_with_the_verses_inside_them() {
        let verse =
            |v: u128, centre: f32| member(false, PlaceOrder::Lyric, Some(Row { id: v, centre }));
        // Two syllables in each of two verses, a line apart.
        let family = [
            verse(1, -3.0),
            verse(1, -3.0),
            verse(2, -5.4),
            verse(2, -5.4),
        ];
        let pieces = [
            piece(0, vec![rect(0.0, 2.0, -3.2, -2.1)]),
            piece(1, vec![rect(5.0, 7.0, -3.2, -2.1)]),
            piece(2, vec![rect(0.0, 2.0, -5.6, -4.5)]),
            piece(3, vec![rect(5.0, 7.0, -5.6, -4.5)]),
        ];
        // A low note over the second syllable: the first verse clears it by
        // a space, and the second keeps its line.
        let ink = sky(vec![rect(5.0, 6.0, -3.0, 4.0)]);
        let shift = place(&family, &pieces, &ink);
        let need = -3.0 - 0.5 - -2.1;
        for (i, s) in shift.iter().enumerate() {
            assert!((s - need).abs() < 1e-5, "{i}: {shift:?}");
        }
        // Ink reaching only the second verse's height, beside the first's
        // syllables: the second moves alone.
        let ink = sky(vec![rect(8.0, 9.0, -6.0, -5.0)]);
        let pieces = [
            piece(0, vec![rect(0.0, 2.0, -3.2, -2.1)]),
            piece(1, vec![rect(5.0, 7.0, -3.2, -2.1)]),
            piece(2, vec![rect(0.0, 2.0, -5.6, -4.5)]),
            piece(3, vec![rect(7.5, 9.5, -5.6, -4.5)]),
        ];
        let shift = place(&family, &pieces, &ink);
        assert_eq!((shift[0], shift[1]), (0.0, 0.0), "{shift:?}");
        let need = -6.0 - 0.5 - -4.5;
        assert!(
            (shift[2] - need).abs() < 1e-5 && (shift[3] - need).abs() < 1e-5,
            "{shift:?}"
        );
    }
}
