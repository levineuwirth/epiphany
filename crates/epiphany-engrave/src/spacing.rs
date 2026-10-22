//! The horizontal spacing pass — the first axis of the planned two-pass spring
//! layout (`epiphany-engrave`'s DECISIONS.md, decision 1).
//!
//! A `ConstrainedLayoutIR` carries horizontal spring slots — one slot per
//! *musical time column* (`to_constrained` groups simultaneous glyphs into a
//! shared column slot, with the clef in a lead column and barlines in their own
//! columns). This pass places each glyph-bearing slot left to right and yields
//! the coordinate-map control points the caller ([`crate::HorizontalRemap`])
//! applies to glyph baselines *and* the strokes that track them.
//!
//! A slot stands at least its predecessor's `preferred_width` after it (the
//! spring's natural width: a note column's comes from the durations sounding
//! through it, the lead's and a signature's from their ink and the gap after
//! it), and far enough right that its ink clears the ink already set at the
//! same height by the gap that ink's slot asks: a skyline of the rightmost ink
//! set so far in each quarter-space band of height, so a column whose notes
//! stand on one staff can sit close after a column whose notes stand on
//! another, as their onsets ask, while a note's accidental never overlaps the
//! previous note it would meet, keeps a signature's gap after it, and a stem
//! reaching across to another staff's beam is cleared there. The casting-off
//! pass (`crate::casting`) then breaks this spaced line into systems and pages
//! and justifies each system by stretching its note columns' springs.
//!
//! The same slots can be spaced again tighter ([`Skyline::place`]): a system
//! the score's breaks fix that holds more than the line compresses toward a
//! floor, its note columns' springs scaled by one factor, so the durations
//! keep their ratios, and the ink after a note column clearing later ink by
//! MuseScore's minimum distances ([`floor_gap`]) rather than by `SLOT_GAP`. At
//! a factor of zero the springs are gone and only those distances hold: the
//! floor. The lead's, a signature's, a change's and a barline's springs and
//! clearances are kept, as justification keeps them.

use std::collections::BTreeMap;

use epiphany_core::TypedObjectId;
use epiphany_layout_ir::{
    is_barline_glyph, is_rigid_width_stroke, ConstrainedLayoutIR, Room, SpringSlotId,
    VerticalBandId, VerticalBandKind,
};

use crate::casting::barline_lines;
use crate::clearance::{self, InkRect};
use crate::owning_glyph;

/// Inter-slot gap (staff spaces) reserved between a note column's right
/// content and a later slot's left content at the same height.
const SLOT_GAP: f32 = 0.3;

/// The least a breath mark or caesura stands clear of the ink before and
/// after it: MuseScore's padding for a breath against anything
/// (`PaddingTable::createTable`, 1.0 either side).
const BREATH_PADDING: f32 = 1.0;

/// The least a barline's ink stands clear of the ink after it, an
/// accidental's mostly: MuseScore's default distance from a barline to an
/// accidental (0.65 spaces). The note itself stands further, by the
/// barline's spring.
const BARLINE_CLEARANCE: f32 = 0.65;

/// The height of one band of the skyline, in staff spaces.
const SKYLINE_BAND: f32 = 0.25;

/// How far above and below its ink a slot's content keeps clear of earlier
/// ink: heads a third apart in neighbouring columns still clear each other
/// sideways, while a head an octave from the previous may stand under it.
/// Rounded out to the skyline's bands, two pieces of ink up to about 0.65
/// apart in height meet, where MuseScore pads only shapes within 0.2 of
/// each other (0.1 before an accidental), so the floor keeps more apart.
const SKYLINE_MARGIN: f32 = 0.2;

/// The least advance from one slot to the next, whatever their springs and
/// ink: two onsets a hair apart on different staves still read in order.
const MIN_ADVANCE: f32 = 0.1;

/// The least a tie runs between its ends, which stand clear of their
/// columns' ink, in staff spaces.
const TIE_MIN_SPAN: f32 = 1.0;

/// How much further the floor keeps a flagged tie's end than its start past
/// the flag and its least span ask, in staff spaces: a thousandth, so a tie
/// the floor sets at exactly that span is never left a rounding short of it,
/// which would keep it in its flag (`clearance::start_past_flag` keeps a tie
/// the move would leave shorter than its least span).
const FLAG_START_SLACK: f32 = 1e-3;

/// The least a beam across two staves runs between the stems it joins, whose
/// up stem right of its head can stand close beside the next down stem left
/// of its own, in staff spaces.
const CROSS_BEAM_MIN_SPAN: f32 = 1.5;

/// The spacing pass's output: the interpolation control points for spanning
/// strokes, and each glyph-bearing slot's exact `(source, target)` pair — the
/// rigid delta every member glyph translates by, so intra-slot offsets (a
/// time signature after its barline, key-signature accidentals after the
/// clef, an accidental left of its notehead) survive the re-spacing verbatim.
pub(crate) struct SpacedSlots {
    /// `(source_x, target_x)` control points, sorted by source, sources
    /// distinct — the piecewise-linear map for content that genuinely *spans*
    /// columns (staff lines, brackets).
    pub points: Vec<(f32, f32)>,
    /// Each glyph-bearing slot's own `(source_x, target_x)`.
    pub by_slot: BTreeMap<SpringSlotId, (f32, f32)>,
    /// The slots' ink by kind, to space a run of them again tighter.
    pub skyline: Skyline,
}

/// What a piece of ink is, for the distance it keeps from ink beside it at
/// the floor: MuseScore's padding table pairs kinds of item.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Ink {
    Head,
    Stem,
    Ledger,
    Accidental,
    Dot,
    Flag,
    Rest,
    Other,
    /// A breath mark or caesura.
    Breath,
    Barline,
    Clef,
    KeySig,
    TimeSig,
}

/// How many kinds of [`Ink`] there are.
const INKS: usize = 13;

impl Ink {
    /// The kinds a note column's ink takes, the ones [`floor_gap`] reads as
    /// the earlier of two.
    const NOTE_COLUMN: [Ink; 8] = [
        Ink::Head,
        Ink::Stem,
        Ink::Ledger,
        Ink::Accidental,
        Ink::Dot,
        Ink::Flag,
        Ink::Rest,
        Ink::Other,
    ];

    /// A glyph's kind by its name; an accidental in a slot whose spring is
    /// fixed (a lead, or a barline a key changes at) belongs to a key
    /// signature.
    fn of_glyph(name: &str, fixed: bool) -> Ink {
        if name.starts_with("notehead") {
            Ink::Head
        } else if name.starts_with("accidental") {
            if fixed {
                Ink::KeySig
            } else {
                Ink::Accidental
            }
        } else if name.starts_with("augmentationDot") {
            Ink::Dot
        } else if name.starts_with("flag") {
            Ink::Flag
        } else if name.starts_with("rest") {
            Ink::Rest
        } else if is_barline_glyph(name) || name.starts_with("repeat") {
            Ink::Barline
        } else if name.contains("Clef") || name.starts_with("clef") {
            Ink::Clef
        } else if name.starts_with("timeSig") {
            Ink::TimeSig
        } else if name.starts_with("breathMark") || name.starts_with("caesura") {
            Ink::Breath
        } else {
            Ink::Other
        }
    }

    /// Whether ink of this kind keeps clear of earlier ink of its own staff
    /// at any height, as MuseScore never kerns a barline, a clef or a
    /// signature (`HorizontalSpacing::isNeverKernable`); a clef here wherever
    /// it stands, where MuseScore kerns one that changes inside a measure.
    fn never_kerned(self) -> bool {
        matches!(self, Ink::Barline | Ink::Clef | Ink::KeySig | Ink::TimeSig)
    }

    /// Whether ink of this kind keeps clear of earlier ink of its kind and
    /// staff at any height, as MuseScore never kerns one voice's heads, stems,
    /// dots, rests and breath marks against each other
    /// (`HorizontalSpacing::isSameVoiceKerningLimited`; a staff stands in for
    /// the voice).
    fn kept_in_voice(self) -> bool {
        matches!(
            self,
            Ink::Head | Ink::Stem | Ink::Dot | Ink::Rest | Ink::Breath
        )
    }
}

/// The least distance at the floor from a note column's ink of kind
/// `earlier` to later ink of kind `later` it meets at the same height, in
/// staff spaces: MuseScore 4.7.5's padding table
/// (`PaddingTable::createTable`) at its default style, where 0.1 (its
/// `minPadUnit`) stands for every pair the table leaves at its default.
/// `minNoteDistance` is 0.35, `noteBarDistance` 1.5, `accidentalNoteDistance`
/// 0.25, `dotNoteDistance` 0.5, `dotDotDistance` 0.65 and `ledgerLineLength`
/// 0.33 (`styledef.cpp`); a head meeting a head keeps `minNoteDistance`
/// whatever their voices, where `HorizontalSpacing::computeNotePadding`
/// raises the table's 0.1 to it only for two notes of one voice, or two
/// neighbours by duration, whose shapes meet.
pub(crate) fn floor_gap(earlier: Ink, later: Ink) -> f32 {
    use Ink::*;
    // The ledger line's reach past its head, which the table takes off a
    // head's distance for a ledger line, at least a quarter of a space.
    let ledger = |head: f32, by: f32| (head - by).max(0.25);
    match (earlier, later) {
        // A breath mark's column is fixed: the clearance it keeps after its
        // ink is its slot's.
        (_, Breath) => BREATH_PADDING,
        (Head, Head | Stem | Ledger | Accidental) => 0.35,
        (Head | Stem, Rest) => 0.5,
        (Head | Stem, Barline) => 1.5,
        (Head | Stem, Clef) => 0.8,
        (Head | Stem, KeySig | TimeSig) => 0.75,
        (Stem, Head | Ledger | Accidental) => 0.35,
        (Stem, Stem) => 0.85,
        (Ledger, Head | Stem) => 0.35,
        (Ledger, Ledger) => 0.25,
        (Ledger, Accidental) => ledger(0.35, 0.33 / 2.0),
        (Ledger, Rest) => 0.35,
        (Ledger, Barline) => ledger(1.5, 0.33),
        (Ledger, Clef) => ledger(0.8, 0.33 / 2.0),
        (Ledger, KeySig | TimeSig) => ledger(0.75, 0.33 / 2.0),
        (Accidental, Head | Stem) => 0.25,
        (Accidental, Ledger) => 0.18,
        (Dot, Head | Stem | Rest) => 0.65,
        (Dot, Ledger) => ledger(0.65, 0.33),
        (Dot, Accidental) => 0.35,
        (Dot, Barline) => 0.8,
        (Dot, Clef) => 1.0,
        (Dot, KeySig | TimeSig) => 1.35,
        (Flag, Head | Stem | Accidental | Rest) => 0.35,
        (Flag, Ledger) => ledger(0.35, 0.33),
        (Flag, Barline) => 1.0,
        (Flag, Clef) => 0.5,
        (Flag, KeySig | TimeSig) => 1.15,
        (Rest, Head | Rest) => 0.5,
        (Rest, Stem) => 0.35,
        (Rest, Ledger) => ledger(0.5, 0.33 / 2.0),
        (Rest, Accidental) => 0.45,
        (Rest, Barline) => 1.65,
        (Rest, Clef) => 0.8,
        (Rest, KeySig | TimeSig) => 1.5,
        _ => 0.1,
    }
}

/// The least distance at the floor from a note column's ink of kind
/// `earlier` to later ink of kind `later` on the same staff whatever their
/// heights, where MuseScore keeps them apart so ([`Ink::never_kerned`],
/// [`Ink::kept_in_voice`]): [`floor_gap`], but for two heads of one voice at
/// different heights, which keep only the table's 0.1.
fn staff_gap(earlier: Ink, later: Ink) -> Option<f32> {
    if later.never_kerned() || (earlier.kept_in_voice() && later.kept_in_voice()) {
        Some(match (earlier, later) {
            (Ink::Head, Ink::Head) => 0.1,
            _ => floor_gap(earlier, later),
        })
    } else {
        None
    }
}

/// One slot as [`Skyline::place`] spaces it: its spring, the clearance a
/// fixed slot keeps after its ink, and its ink by band (all of it, and by
/// kind) and by staff, each edge as an offset from the slot's source.
struct FloorSlot {
    preferred: f32,
    fixed: bool,
    clearance: f32,
    bands: Vec<(i32, f32, f32)>,
    floor: Vec<(i32, Ink, f32, f32)>,
    staff: Vec<(usize, Ink, f32, f32)>,
    /// `(earlier slot's index, least distance)` for each tie or beam across
    /// two staves ending here, and for each tie whose first note's flag
    /// stands on its side, the span its start past the flag needs.
    ties: Vec<(usize, f32)>,
}

/// Every glyph-bearing slot's ink by kind, in source order, to space a run
/// of them again tighter than the spacing pass did.
pub(crate) struct Skyline {
    slots: Vec<FloorSlot>,
    index: BTreeMap<SpringSlotId, usize>,
    staves: usize,
}

impl Skyline {
    /// The slots `ids` (in source order) spaced again from the first, which
    /// stands at 0: each note column's spring scaled by `scale`, each fixed
    /// slot's kept; ink after a note column clearing later ink by
    /// [`floor_gap`] where their heights meet and by [`staff_gap`] on one
    /// staff at any height, ink after a fixed slot by the clearance the
    /// spacing pass gives it; ties and beams across two staves their least
    /// span where both ends are among `ids`, and a tie whose first note has a
    /// flag on its side the span to start past it. A slot it does not know
    /// stands where the one before it does. At `scale` 0, the floor.
    pub(crate) fn place(&self, ids: &[SpringSlotId], scale: f32) -> Vec<f32> {
        let members: Vec<Option<usize>> =
            ids.iter().map(|id| self.index.get(id).copied()).collect();
        let (mut lo, mut hi) = (i32::MAX, i32::MIN);
        for &k in members.iter().flatten() {
            let slot = &self.slots[k];
            for band in slot
                .bands
                .iter()
                .map(|b| b.0)
                .chain(slot.floor.iter().map(|b| b.0))
            {
                lo = lo.min(band);
                hi = hi.max(band);
            }
        }
        let bands = if lo <= hi { (hi - lo + 1) as usize } else { 0 };
        let at = |band: i32| (band - lo) as usize;
        // The rightmost ink set so far: a fixed slot's with its clearance, by
        // band; a note column's by band and kind, and by staff and kind.
        let mut fixed_edge = vec![f32::NEG_INFINITY; bands];
        let mut band_edge = vec![f32::NEG_INFINITY; bands * INKS];
        let mut staff_edge = vec![f32::NEG_INFINITY; self.staves * INKS];
        let mut placed: Vec<(usize, f32)> = Vec::with_capacity(ids.len());
        let mut out = Vec::with_capacity(ids.len());
        let mut previous: Option<(f32, f32)> = None;
        for member in members {
            let Some(k) = member else {
                out.push(previous.map_or(0.0, |(target, _)| target));
                continue;
            };
            let slot = &self.slots[k];
            let mut target =
                previous.map_or(0.0, |(target, advance)| target + advance.max(MIN_ADVANCE));
            for &(band, left, _) in &slot.bands {
                target = target.max(fixed_edge[at(band)] - left);
            }
            for &(band, later, left, _) in &slot.floor {
                let row = &band_edge[at(band) * INKS..][..INKS];
                for earlier in Ink::NOTE_COLUMN {
                    target = target.max(row[earlier as usize] + floor_gap(earlier, later) - left);
                }
            }
            for &(staff, later, left, _) in &slot.staff {
                let row = &staff_edge[staff * INKS..][..INKS];
                for earlier in Ink::NOTE_COLUMN {
                    if let Some(gap) = staff_gap(earlier, later) {
                        target = target.max(row[earlier as usize] + gap - left);
                    }
                }
            }
            for &(start, need) in &slot.ties {
                if let Ok(j) = placed.binary_search_by_key(&start, |p| p.0) {
                    target = target.max(placed[j].1 + need);
                }
            }
            if slot.fixed {
                for &(band, _, right) in &slot.bands {
                    let edge = &mut fixed_edge[at(band)];
                    *edge = edge.max(target + right + slot.clearance);
                }
            } else {
                for &(band, kind, _, right) in &slot.floor {
                    let edge = &mut band_edge[at(band) * INKS + kind as usize];
                    *edge = edge.max(target + right);
                }
                for &(staff, kind, _, right) in &slot.staff {
                    let edge = &mut staff_edge[staff * INKS + kind as usize];
                    *edge = edge.max(target + right);
                }
            }
            placed.push((k, target));
            out.push(target);
            let advance = if slot.fixed {
                slot.preferred
            } else {
                scale * slot.preferred
            };
            previous = Some((target, advance));
        }
        out
    }
}

/// Spaces the glyph-bearing slots left to right. Each slot's source is its
/// column reference (its first member glyph's baseline); its target is the
/// least that keeps its predecessor's spring, clears the ink set so far at
/// each height its own ink reaches (its accidentals and other left overhang
/// included) by the clearance that ink's slot asks, and gives each tie and
/// cross-staff beam ending there its least span.
/// Deterministic: a pure function of the glyphs, their bounding boxes and the
/// slots' springs.
pub(crate) fn space_slots(input: &ConstrainedLayoutIR) -> SpacedSlots {
    let anchors = crate::span_anchors(input);
    /// One slot's horizontal extent, from its member glyphs and the strokes
    /// that ride it, by height.
    struct Extent {
        /// Column reference x (the first member's baseline).
        source: f32,
        /// Each skyline band's leftmost and rightmost content edge.
        bands: BTreeMap<i32, (f32, f32)>,
        /// The spring's natural width.
        preferred: f32,
        /// Whether the spring keeps its width when a system is justified:
        /// a lead's, a signature's, a change's or a barline's.
        fixed: bool,
        /// Whether every member is a barline or a repeat sign.
        barline: bool,
        /// Each band's leftmost and rightmost edge of each kind of ink.
        floor: BTreeMap<(i32, Ink), (f32, f32)>,
        /// Each staff's leftmost and rightmost edge of each kind of ink, at
        /// any height.
        staff: BTreeMap<(VerticalBandId, Ink), (f32, f32)>,
    }

    let preferred_of: BTreeMap<SpringSlotId, (f32, bool)> = input
        .horizontal_slots
        .iter()
        .map(|s| (s.id, (s.preferred_width.0, s.stretch_factor == 0.0)))
        .collect();
    let staff_of_band: BTreeMap<_, _> = input
        .vertical_bands
        .iter()
        .filter_map(|band| match band.kind {
            VerticalBandKind::Staff(staff) => Some((band.id, staff)),
            _ => None,
        })
        .collect();
    let mut by_slot: BTreeMap<SpringSlotId, Extent> = BTreeMap::new();
    // The bands a height (and the margin about it) reaches.
    let bands_of = |(bottom, top): (f32, f32)| {
        let lo = ((bottom - SKYLINE_MARGIN) / SKYLINE_BAND).floor() as i32;
        let hi = ((top + SKYLINE_MARGIN) / SKYLINE_BAND).floor() as i32;
        lo..=hi.max(lo)
    };
    let stretch = |e: &mut (f32, f32), (left, right): (f32, f32)| {
        e.0 = e.0.min(left);
        e.1 = e.1.max(right);
    };
    // Widens a slot's extent by a box of ink of kind `ink`, in every band its
    // height reaches, and on its staff, if it stands on one.
    let widen = |extent: &mut Extent,
                 ink: Ink,
                 staff: Option<VerticalBandId>,
                 (left, right): (f32, f32),
                 height: (f32, f32)| {
        for band in bands_of(height) {
            extent
                .bands
                .entry(band)
                .and_modify(|e| {
                    e.0 = e.0.min(left);
                    e.1 = e.1.max(right);
                })
                .or_insert((left, right));
            extent
                .floor
                .entry((band, ink))
                .and_modify(|e| stretch(e, (left, right)))
                .or_insert((left, right));
        }
        if let Some(staff) = staff.filter(|band| staff_of_band.contains_key(band)) {
            extent
                .staff
                .entry((staff, ink))
                .and_modify(|e| stretch(e, (left, right)))
                .or_insert((left, right));
        }
    };
    // The family's glyphs that take no room across (an articulation, a
    // fermata, a dynamic): they keep their slot, and their ink meets other
    // ink freely, as MuseScore's does. A slot's source is its first glyph
    // that takes room, never a mark standing off its note.
    let roomless = crate::family::roomless_glyphs(input);
    let (rooming, marks): (Vec<_>, Vec<_>) = input
        .glyphs
        .iter()
        .partition(|glyph| !roomless.contains(&glyph.id()));
    for glyph in rooming.into_iter().chain(marks) {
        let x = glyph.baseline.x.0;
        let y = glyph.baseline.y.0;
        let b = glyph.bounding_box;
        let (preferred, fixed) = preferred_of
            .get(&glyph.horizontal_slot)
            .copied()
            .unwrap_or((0.0, false));
        let extent = by_slot.entry(glyph.horizontal_slot).or_insert(Extent {
            source: x,
            bands: BTreeMap::new(),
            preferred,
            fixed,
            barline: true,
            floor: BTreeMap::new(),
            staff: BTreeMap::new(),
        });
        if roomless.contains(&glyph.id()) {
            continue;
        }
        let name = glyph.glyph.as_str();
        extent.barline &= name.starts_with("barline") || name.starts_with("repeat");
        widen(
            extent,
            Ink::of_glyph(name, fixed),
            Some(glyph.vertical_band),
            (x + b.left.0, x + b.right.0),
            (y + b.bottom.0, y + b.top.0),
        );
    }

    // A group whose barlines join from staff to staff has each joined across
    // the gap between two of its staves by the casting stage, after spacing,
    // so the joining line is reserved here: a barline slot's extent takes,
    // in each such gap, each line of the upper staff's barline at its own
    // thickness, from the lower staff's barline to the upper's, as casting
    // draws it, and an accidental on a note above or below its staff keeps a
    // barline's clearance in the gap as on the staff.
    for group in input
        .staff_groups
        .iter()
        .filter(|group| group.joined && group.staves.len() > 1)
    {
        // Each barline slot's barlines on the group's staves, top first.
        let mut barlines: BTreeMap<SpringSlotId, Vec<&epiphany_layout_ir::GlyphObject>> =
            BTreeMap::new();
        for glyph in &input.glyphs {
            if is_barline_glyph(glyph.glyph.as_str())
                && staff_of_band
                    .get(&glyph.vertical_band)
                    .is_some_and(|staff| group.staves.contains(staff))
            {
                barlines
                    .entry(glyph.horizontal_slot)
                    .or_default()
                    .push(glyph);
            }
        }
        for (slot, mut glyphs) in barlines {
            let Some(extent) = by_slot.get_mut(&slot) else {
                continue;
            };
            glyphs.sort_by(|a, b| b.baseline.y.0.total_cmp(&a.baseline.y.0));
            for pair in glyphs.windows(2) {
                let (upper, lower) = (pair[0], pair[1]);
                let (x, b) = (upper.baseline.x.0, &upper.bounding_box);
                let from = lower.baseline.y.0 + lower.bounding_box.top.0;
                let to = upper.baseline.y.0 + b.bottom.0;
                for (dx, thickness) in barline_lines(upper.glyph.as_str(), b) {
                    let half = thickness / 2.0;
                    widen(
                        extent,
                        Ink::Barline,
                        None,
                        (x + dx - half, x + dx + half),
                        (from, to),
                    );
                }
            }
        }
    }

    // Fold into its slot's extent each ledger line (a fixed-width stroke, by its
    // notehead: the same-source glyph whose baseline lies within the stroke's
    // span, its accidentals standing outside it to the left), so a ledger that
    // overhangs the notehead reserves room, and each stroke that rides one
    // slot at both ends (a stem), so a stem reaching another staff's beam is
    // cleared where it reaches.
    for stroke in &input.strokes {
        let rigid = is_rigid_width_stroke(stroke);
        let slot = if rigid {
            owning_glyph(stroke, &input.glyphs).map(|glyph| glyph.horizontal_slot)
        } else {
            anchors
                .get(&stroke.id())
                .filter(|(start, end)| start == end)
                .map(|(start, _)| *start)
        };
        let Some(extent) = slot.and_then(|slot| by_slot.get_mut(&slot)) else {
            continue;
        };
        let ink = if rigid {
            Ink::Ledger
        } else if crate::stem_slot(stroke, &anchors).is_some() {
            Ink::Stem
        } else {
            Ink::Other
        };
        let half = stroke.thickness.0 * 0.5;
        widen(
            extent,
            ink,
            Some(stroke.vertical_band),
            (
                stroke.from.x.0.min(stroke.to.x.0) - half,
                stroke.from.x.0.max(stroke.to.x.0) + half,
            ),
            (
                stroke.from.y.0.min(stroke.to.y.0) - half,
                stroke.from.y.0.max(stroke.to.y.0) + half,
            ),
        );
    }

    // A text run of the family that takes room as ink does (an arpeggio, left
    // of its chord as an accidental stands).
    for (text, room) in crate::family::texts_with_room(input) {
        if room != Room::Ink {
            continue;
        }
        let Some(extent) = by_slot.get_mut(&text.horizontal_slot) else {
            continue;
        };
        let (x, y, b) = (text.text.origin.x.0, text.text.origin.y.0, text.text.bounds);
        widen(
            extent,
            Ink::Accidental,
            Some(text.vertical_band),
            (x + b.left.0, x + b.right.0),
            (y + b.bottom.0, y + b.top.0),
        );
    }

    let mut slots: Vec<(SpringSlotId, Extent)> = by_slot.into_iter().collect();
    slots.sort_by(|a, b| {
        a.1.source
            .partial_cmp(&b.1.source)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // A tie anchored to two slots runs at least `TIE_MIN_SPAN` between its
    // ends: the later slot stands at least that far, and its end's and the
    // earlier end's offsets from their slots, after the earlier. Each
    // requirement is `(earlier slot, distance)`, by the later slot (a beam
    // across two staves adds its own below). At the floor a tie whose first
    // note has a flag on the tie's side keeps, besides, the span its start
    // past that flag needs (`clearance::flag_start`, then
    // `clearance::MIN_TIE_SPAN`), so compression never sets a tie the
    // natural spacing starts past its flag back into it (`flag_ties`).
    let source_of: BTreeMap<SpringSlotId, f32> =
        slots.iter().map(|(id, e)| (*id, e.source)).collect();
    let mut flags: BTreeMap<(SpringSlotId, VerticalBandId), Vec<InkRect>> = BTreeMap::new();
    for glyph in input
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("flag"))
    {
        let (x, y, b) = (glyph.baseline.x.0, glyph.baseline.y.0, glyph.bounding_box);
        flags
            .entry((glyph.horizontal_slot, glyph.vertical_band))
            .or_default()
            .push(InkRect {
                left: x + b.left.0,
                right: x + b.right.0,
                bottom: y + b.bottom.0,
                top: y + b.top.0,
            });
    }
    let mut ties: BTreeMap<SpringSlotId, Vec<(SpringSlotId, f32)>> = BTreeMap::new();
    let mut flag_ties: BTreeMap<SpringSlotId, Vec<(SpringSlotId, f32)>> = BTreeMap::new();
    for curve in &input.curves {
        let Some((start, end)) = anchors.get(&curve.id()) else {
            continue;
        };
        let (Some(&s0), Some(&s1)) = (source_of.get(start), source_of.get(end)) else {
            continue;
        };
        if start == end || s1 <= s0 {
            continue;
        }
        let need = (curve.p0.x.0 - s0) + TIE_MIN_SPAN + (s1 - curve.p3.x.0);
        ties.entry(*end).or_default().push((*start, need));
        if !matches!(curve.provenance.source, TypedObjectId::Tie(_)) {
            continue;
        }
        let at_start = flags.get(&(*start, curve.vertical_band));
        if let Some(from) = at_start.and_then(|f| clearance::flag_start(curve.control_points(), f))
        {
            let need =
                (from - s0) + clearance::MIN_TIE_SPAN + FLAG_START_SLACK + (s1 - curve.p3.x.0);
            flag_ties.entry(*end).or_default().push((*start, need));
        }
    }
    // Likewise a beam across two staves, between the stems at its ends.
    let crossing: std::collections::BTreeSet<_> = input
        .cross_staff_beams
        .iter()
        .flat_map(|beam| beam.ink.iter().copied())
        .collect();
    for stroke in input.strokes.iter().filter(|s| crossing.contains(&s.id())) {
        let Some((start, end)) = anchors.get(&stroke.id()) else {
            continue;
        };
        let (Some(&s0), Some(&s1)) = (source_of.get(start), source_of.get(end)) else {
            continue;
        };
        if start == end || s1 <= s0 {
            continue;
        }
        let need = (stroke.from.x.0 - s0) + CROSS_BEAM_MIN_SPAN + (s1 - stroke.to.x.0);
        ties.entry(*end).or_default().push((*start, need));
    }

    // The clearance a slot asks after its ink: a note column `SLOT_GAP`, a
    // barline `BARLINE_CLEARANCE`, and a lead, signature, change or breath
    // mark the gap its spring reserves after its ink, so a following
    // accidental keeps that gap too.
    let clearance_of = |extent: &Extent| {
        let bearing = extent
            .bands
            .values()
            .map(|(_, right)| right - extent.source)
            .fold(0.0_f32, f32::max);
        match (extent.fixed, extent.barline) {
            (false, _) => SLOT_GAP,
            (true, true) => BARLINE_CLEARANCE,
            (true, false) => (extent.preferred - bearing).max(SLOT_GAP),
        }
    };
    let mut points = Vec::with_capacity(slots.len());
    let mut placed: BTreeMap<SpringSlotId, (f32, f32)> = BTreeMap::new();
    // Each band's rightmost ink set so far, in the target frame, with the
    // clearance its slot asks after it.
    let mut frontier: BTreeMap<i32, f32> = BTreeMap::new();
    // A breath mark keeps `BREATH_PADDING` from the ink before it where
    // their heights meet, and from its staff's heads, stems, dots and rests
    // at any height ([`Ink::kept_in_voice`]). After it, its fixed column's
    // spring keeps the next column so far from its right edge, and the
    // clearance that spring sets keeps an accidental there. Each band's
    // rightmost ink, and each staff's rightmost such ink, without clearance.
    let is_breath = |extent: &Extent| extent.staff.keys().any(|(_, ink)| *ink == Ink::Breath);
    let mut bare: BTreeMap<i32, f32> = BTreeMap::new();
    let mut voice_edge: BTreeMap<VerticalBandId, f32> = BTreeMap::new();
    // The previous slot's target and spring.
    let mut previous: Option<(f32, f32)> = None;
    for (id, extent) in &slots {
        let mut target = previous.map_or(0.0, |(at, preferred)| at + preferred.max(MIN_ADVANCE));
        for (band, (left, _)) in &extent.bands {
            if let Some(&edge) = frontier.get(band) {
                target = target.max(edge + (extent.source - left));
            }
        }
        if is_breath(extent) {
            for (band, (left, _)) in &extent.bands {
                if let Some(&edge) = bare.get(band) {
                    target = target.max(edge + BREATH_PADDING + (extent.source - left));
                }
            }
            for ((staff, ink), (left, _)) in &extent.staff {
                if let (Ink::Breath, Some(&edge)) = (ink, voice_edge.get(staff)) {
                    target = target.max(edge + BREATH_PADDING + (extent.source - left));
                }
            }
        }
        for (start, need) in ties.get(id).into_iter().flatten() {
            if let Some(&(_, at)) = placed.get(start) {
                target = target.max(at + need);
            }
        }
        let clearance = clearance_of(extent);
        for (band, (_, right)) in &extent.bands {
            let edge = target + (right - extent.source) + clearance;
            frontier
                .entry(*band)
                .and_modify(|e| *e = e.max(edge))
                .or_insert(edge);
            let edge = target + (right - extent.source);
            bare.entry(*band)
                .and_modify(|e| *e = e.max(edge))
                .or_insert(edge);
        }
        for ((staff, ink), (_, right)) in &extent.staff {
            if *ink == Ink::Breath || !ink.kept_in_voice() {
                continue;
            }
            let edge = target + (right - extent.source);
            voice_edge
                .entry(*staff)
                .and_modify(|e| *e = e.max(edge))
                .or_insert(edge);
        }
        points.push((extent.source, target));
        placed.insert(*id, (extent.source, target));
        previous = Some((target, extent.preferred));
    }
    points.dedup_by(|a, b| a.0 == b.0);

    let index: BTreeMap<SpringSlotId, usize> = slots
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let mut staff_index: BTreeMap<VerticalBandId, usize> = BTreeMap::new();
    for (_, extent) in &slots {
        for (staff, _) in extent.staff.keys() {
            let next = staff_index.len();
            staff_index.entry(*staff).or_insert(next);
        }
    }
    let skyline = Skyline {
        slots: slots
            .iter()
            .map(|(id, extent)| {
                let s = extent.source;
                FloorSlot {
                    preferred: extent.preferred,
                    fixed: extent.fixed,
                    clearance: clearance_of(extent),
                    bands: extent
                        .bands
                        .iter()
                        .map(|(band, (l, r))| (*band, l - s, r - s))
                        .collect(),
                    floor: extent
                        .floor
                        .iter()
                        .map(|(&(band, ink), (l, r))| (band, ink, l - s, r - s))
                        .collect(),
                    staff: extent
                        .staff
                        .iter()
                        .map(|(&(staff, ink), (l, r))| (staff_index[&staff], ink, l - s, r - s))
                        .collect(),
                    ties: ties
                        .get(id)
                        .into_iter()
                        .chain(flag_ties.get(id))
                        .flatten()
                        .filter_map(|(start, need)| Some((*index.get(start)?, *need)))
                        .collect(),
                }
            })
            .collect(),
        index,
        staves: staff_index.len(),
    };
    SpacedSlots {
        points,
        by_slot: placed,
        skyline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{to_constrained, to_logical};

    #[test]
    fn control_points_are_monotonic_in_source_and_target() {
        let c = to_constrained(&to_logical(&valid_score_rich(7)));
        let spaced = space_slots(&c);
        assert!(!spaced.points.is_empty());
        for w in spaced.points.windows(2) {
            assert!(w[1].0 > w[0].0, "sources strictly increase");
            assert!(w[1].1 > w[0].1, "targets strictly increase");
        }
        // The two views describe one spacing: every placed slot's pair is one
        // of the control points (this fixture's slot sources are all distinct,
        // so the equal-source dedup removes nothing).
        for (source, target) in spaced.by_slot.values() {
            assert!(
                spaced
                    .points
                    .iter()
                    .any(|(s, t)| s == source && t == target),
                "slot pair ({source}, {target}) must be a control point"
            );
        }
    }

    #[test]
    fn spacing_re_spaces_rather_than_echoing_sources() {
        // A wide lead (clef) advances by more than a uniform note slot, so the
        // engraved targets are not a copy of the source columns.
        let c = to_constrained(&to_logical(&valid_score_rich(7)));
        let spaced = space_slots(&c);
        assert!(
            spaced.points.iter().any(|(s, t)| (s - t).abs() > 1e-3),
            "targets must differ from sources (re-spacing happened)"
        );
    }

    #[test]
    fn spacing_is_deterministic() {
        let c = to_constrained(&to_logical(&valid_score_rich(3)));
        let (a, b) = (space_slots(&c), space_slots(&c));
        assert_eq!(a.points, b.points);
        assert_eq!(a.by_slot, b.by_slot);
    }
}
