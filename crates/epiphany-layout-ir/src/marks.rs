//! Marks at their notes (X4c.3): where a chord's articulations, techniques,
//! ornaments, tremolo strokes, jazz marks and arpeggio stand, by MuseScore
//! 4.7.5's default rules (`ChordLayout::layoutArticulations` and
//! `layoutArticulations2`, `TremoloLayout::layoutOneNoteTremolo`,
//! `ArpeggioLayout`), read in its source. The constrained pass places them
//! here, from the chord's own heads, stem, flag and beam and its staff's
//! lines, and the column's ink takes them in, so a slur passes over them as
//! MuseScore's does.
//!
//! Coordinates are the constrained frame's: staff spaces, y up, the staff's
//! bottom line at `yo` and its top line at `yo + 4`.

use epiphany_core::{ArpeggioDirection, EventMark, Ornament, OrnamentKind};

use super::{ColumnInk, StemSeg, STEM_THICKNESS};
use crate::glyph::metrics;
use crate::logical::VoicePlace;
use crate::spatial::{BoundingBox, Point};
use crate::text::{Symbol, SymbolPlace, Turn};

/// A mark's distance from its note's head or stem, and from the staff
/// (`propertyDistanceHead`, `propertyDistanceStem`, `propertyDistance`).
const PROPERTY_DISTANCE: f32 = 0.4;
/// The distance between two marks stacked on one side
/// (`articulationMinDistance`).
const ARTICULATION_MIN_DISTANCE: f32 = 0.4;
/// How much nearer a staccato standing outside the staff an accent set
/// straight after it on its side stands (`stacAccentKern`).
const STACCATO_ACCENT_KERN: f32 = 0.2;
/// How far in from its stem's tip a one-note tremolo's strokes start
/// (`tremoloOutSidePadding`), and how far a stemless note's stand off its
/// head (`tremoloNoteSidePadding`).
const TREMOLO_OUTSIDE_PADDING: f32 = 0.5;
const TREMOLO_NOTE_SIDE_PADDING: f32 = 1.25;
/// The gap between an arpeggio and the heads or accidentals right of it
/// (`arpeggioNoteDistance`), and, where a head stands on a ledger line and
/// none has an accidental, between it and the ledger line, which reaches
/// `LEDGER_LINE_LENGTH` past its head (MuseScore's padding from an arpeggio
/// to a ledger line, and `ledgerLineLength`; `ChordLayout`'s arpeggio gap).
const ARPEGGIO_NOTE_DISTANCE: f32 = 0.4;
const ARPEGGIO_LEDGER_DISTANCE: f32 = 0.3;
const LEDGER_LINE_LENGTH: f32 = 0.33;
/// The step from one arpeggio wiggle to the next, Bravura's
/// `wiggleArpeggiatoUp` advance, in staff spaces.
pub(super) const ARPEGGIO_WIGGLE_STEP: f32 = 1.02;
/// The gap between a jazz mark and the head it leaves or approaches.
const JAZZ_GAP: f32 = 0.2;
/// How much further from its head a close mark set from it stands where a
/// tie leaves room: a staccato and a tenuto
/// (`staccatoAdditionalTieDistance`, `tenutoAdditionalTieDistance`).
const STACCATO_TIE_DISTANCE: f32 = 0.4;
const TENUTO_TIE_DISTANCE: f32 = 0.6;

/// What a chord holds that its marks are placed by: its first component's
/// stem and heads, its voice's place, and its staff.
pub(super) struct Chord<'a> {
    pub seg: &'a StemSeg,
    /// The notehead glyph its first component draws.
    pub head: &'static str,
    /// Its heads' x offsets from the column (beside a second), and the
    /// leftmost ink of its heads and accidentals, from the column.
    pub heads_left: f32,
    pub heads_right: f32,
    /// The leftmost ink of the heads and accidentals of every chord of its
    /// staff's column, of its own voice or a later one, that its height
    /// meets, from the column.
    pub column_left: f32,
    pub voice: VoicePlace,
    /// The staff's bottom line.
    pub yo: f32,
    /// The chord's first component is in a beam.
    pub beamed: bool,
    /// A head of it draws an accidental.
    pub accidentals: bool,
    /// Its heads' accidentals' ink, each `[left, bottom, right, top]`, `x`
    /// from the column.
    pub accidental_ink: Vec<[f32; 4]>,
    /// A tie leaves or reaches its highest head arcing above, or its lowest
    /// arcing below: a close mark on that side leaves the tie room
    /// (`ChordLayout::leaveSpaceForTie`).
    pub tie_above: bool,
    pub tie_below: bool,
}

/// One placed mark glyph: which of the chord's marks it draws (its wire
/// tag, or `32 +` an ornament's kind), the glyph, and its origin's x from
/// the column and y.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct MarkGlyph {
    pub tag: u8,
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    /// It is placed from the stem's end, so it rides with what that end
    /// meets: a beam across two staves.
    pub at_stem_end: bool,
    /// It is part of its chord's shape, taking room across as the chord's
    /// own ink does: a one-note tremolo or a chord line, which MuseScore's
    /// `ChordLayout::fillShape` adds to the chord's shape, as it adds no
    /// articulation.
    pub room: bool,
}

/// An arpeggio: its glyphs stand upright, from the bottom up, each wiggle a
/// `wiggleArpeggiatoUp` turned a quarter, the last an arrow where it has
/// one. `x` is its right edge from the column, `bottom` and `top` the span
/// of heads it rolls.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Arpeggio {
    pub direction: ArpeggioDirection,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
}

/// An ornament's accidental, drawn at 0.6 of its size above or below it.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct SmallAccidental {
    pub tag: u8,
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    /// As [`MarkGlyph::at_stem_end`].
    pub at_stem_end: bool,
    /// It is its ornament's upper accidental, not its lower.
    pub above: bool,
}

/// Everything placed for one chord.
#[derive(Clone, PartialEq, Debug, Default)]
pub(super) struct Placed {
    pub glyphs: Vec<MarkGlyph>,
    pub accidentals: Vec<SmallAccidental>,
    pub arpeggio: Option<Arpeggio>,
}

impl Placed {
    /// The vertical reach of what was placed above and below the chord: the
    /// column's ink takes it in.
    pub(super) fn reach(&self) -> Option<(f32, f32)> {
        let mut out: Option<(f32, f32)> = None;
        let mut take = |lo: f32, hi: f32| {
            out = Some(match out {
                None => (lo, hi),
                Some((a, b)) => (a.min(lo), b.max(hi)),
            });
        };
        for g in &self.glyphs {
            if let Some(b) = metrics(g.name).map(|m| m.bounding_box()) {
                take(g.y + b.bottom.0, g.y + b.top.0);
            }
        }
        for a in &self.accidentals {
            if let Some(b) = metrics(a.name).map(|m| m.bounding_box()) {
                take(a.y + 0.6 * b.bottom.0, a.y + 0.6 * b.top.0);
            }
        }
        out
    }
}

/// Which side a mark of this kind stands on, MuseScore's way: the string,
/// brass and snap-pizzicato signs above always (`articulationAnchorOther`);
/// beside another voice at the stem; alone, an ornament and a marcato
/// above (Gould, p. 117), and any other at the head, opposite the stem.
/// MuseScore sets an ornament above in an odd voice and below in an even
/// one, but a chord of any voice past the first has voices at its note, so
/// alone it is always the first's, and above.
fn above(kind: Kind, chord: &Chord) -> bool {
    let voices = chord.voice != VoicePlace::Alone;
    match kind {
        Kind::Top => true,
        _ if voices => chord.seg.up,
        Kind::Ornament | Kind::Marcato => true,
        Kind::Close | Kind::Outside => !chord.seg.up,
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Kind {
    /// Staccato and tenuto: in the staff, close to the note.
    Close,
    /// Outside the staff, stacked.
    Outside,
    Marcato,
    /// Anchored to the top always.
    Top,
    Ornament,
}

/// A mark's glyph by side, its kind, and whether it is one of the basic
/// articulations MuseScore aligns with the stem on the stem's side
/// (`Articulation::isBasicArticulation`, a stress mark among them).
fn articulation(mark: &EventMark) -> Option<(&'static str, &'static str, Kind, bool)> {
    use EventMark::*;
    Some(match mark {
        Staccato => (
            "articStaccatoAbove",
            "articStaccatoBelow",
            Kind::Close,
            true,
        ),
        Staccatissimo => (
            "articStaccatissimoAbove",
            "articStaccatissimoBelow",
            Kind::Outside,
            true,
        ),
        Spiccato => (
            "articStaccatissimoStrokeAbove",
            "articStaccatissimoStrokeBelow",
            Kind::Outside,
            true,
        ),
        Tenuto => ("articTenutoAbove", "articTenutoBelow", Kind::Close, true),
        DetachedLegato => (
            "articTenutoStaccatoAbove",
            "articTenutoStaccatoBelow",
            Kind::Outside,
            true,
        ),
        Accent => ("articAccentAbove", "articAccentBelow", Kind::Outside, true),
        Marcato => (
            "articMarcatoAbove",
            "articMarcatoBelow",
            Kind::Marcato,
            true,
        ),
        Stress => ("articStressAbove", "articStressBelow", Kind::Outside, true),
        Unstress => (
            "articUnstressAbove",
            "articUnstressBelow",
            Kind::Outside,
            false,
        ),
        UpBow => ("stringsUpBow", "stringsUpBow", Kind::Top, false),
        DownBow => ("stringsDownBow", "stringsDownBow", Kind::Top, false),
        Harmonic => ("stringsHarmonic", "stringsHarmonic", Kind::Top, false),
        OpenString => ("brassMuteOpen", "brassMuteOpen", Kind::Top, false),
        Stopped => ("brassMuteClosed", "brassMuteClosed", Kind::Top, false),
        SnapPizzicato => (
            "pluckedSnapPizzicatoAbove",
            "pluckedSnapPizzicatoBelow",
            Kind::Top,
            false,
        ),
        _ => return None,
    })
}

/// A one-note tremolo's glyph, by its strokes.
pub(super) fn tremolo_glyph(strokes: u8) -> &'static str {
    match strokes {
        0 | 1 => "tremolo1",
        2 => "tremolo2",
        3 => "tremolo3",
        4 => "tremolo4",
        _ => "tremolo5",
    }
}

/// The one-note tremolo among a note's marks, as its glyph.
pub(super) fn one_note_tremolo(marks: &[EventMark]) -> Option<&'static str> {
    marks.iter().find_map(|m| match m {
        EventMark::Tremolo { strokes } => Some(tremolo_glyph(*strokes)),
        _ => None,
    })
}

/// An ornament's glyph.
pub(super) fn ornament_glyph(kind: OrnamentKind) -> &'static str {
    match kind {
        OrnamentKind::Trill => "ornamentTrill",
        OrnamentKind::Mordent => "ornamentMordent",
        OrnamentKind::InvertedMordent => "ornamentShortTrill",
        OrnamentKind::Turn => "ornamentTurn",
        OrnamentKind::InvertedTurn => "ornamentTurnInverted",
    }
}

fn bbox(name: &str) -> BoundingBox {
    metrics(name)
        .map(|m| m.bounding_box())
        .unwrap_or_else(|| BoundingBox::new(0.0, 0.0, 0.0, 0.0))
}

/// The half-space index of `y` down from the staff's top line, as MuseScore
/// counts lines (`0` the top line, `8` the bottom).
fn index(chord: &Chord, y: f32) -> i32 {
    ((chord.yo + 4.0 - y) * 2.0).round() as i32
}

fn at_index(chord: &Chord, i: i32) -> f32 {
    chord.yo + 4.0 - i as f32 * 0.5
}

/// The stem's or flag's far end, and a beam's outer edge, where the stem
/// side's marks start; `None` for a chord drawing no stem.
fn stem_end(chord: &Chord) -> Option<f32> {
    let seg = chord.seg;
    if !seg.drawn {
        return None;
    }
    let sign = if seg.up { 1.0 } else { -1.0 };
    let flag = seg.flag.map(bbox);
    Some(match flag {
        Some(b) if seg.up => seg.end.max(seg.tip + b.top.0),
        Some(b) => seg.end.min(seg.tip + b.bottom.0),
        // A beamed stem ends a stem's thickness inside the beam's outer edge.
        None if chord.beamed => seg.end + sign * STEM_THICKNESS,
        None => seg.end,
    })
}

/// The x a mark is centred on: the head's centre on the head's side, and on
/// the stem's side midway between the head's centre and the stem
/// (`articulationStemHAlign`, `AVERAGE`).
fn centre_x(chord: &Chord, stem_side: bool) -> f32 {
    let head = bbox(chord.head);
    let centre = chord.seg.dx + (head.left.0 + head.right.0) * 0.5;
    if stem_side && chord.seg.drawn {
        let stem = chord.seg.dx + chord.seg.x_off;
        (centre + stem) * 0.5
    } else {
        centre
    }
}

/// A glyph's origin x centring it on `x`.
fn centred(name: &str, x: f32) -> f32 {
    let b = bbox(name);
    x - (b.left.0 + b.right.0) * 0.5
}

/// Places a chord's articulations, techniques and ornaments, its one-note
/// tremolo and its jazz marks, and its arpeggio, in MuseScore's order: the
/// close marks first (a staccato before a tenuto), then the rest, each kind
/// in its set's order, the ornaments last.
pub(super) fn place(chord: &Chord, marks: &[EventMark], ornaments: &[Ornament]) -> Placed {
    let mut placed = Placed::default();
    let seg = chord.seg;
    let head = bbox(chord.head);
    let (low, high) = (seg.lo, seg.hi);
    let head_bottom = low + head.bottom.0;
    let head_top = high + head.top.0;
    let top_line = chord.yo + 4.0;
    let bottom_line = chord.yo;

    // Which marks stand where.
    struct Mark {
        tag: u8,
        above: bool,
        name: &'static str,
        kind: Kind,
        basic: bool,
        accidental: Option<(Option<&'static str>, Option<&'static str>)>,
    }
    let mut list: Vec<Mark> = Vec::new();
    for mark in marks {
        if let Some((up_name, down_name, kind, basic)) = articulation(mark) {
            let up = above(kind, chord);
            list.push(Mark {
                tag: mark.tag(),
                above: up,
                name: if up { up_name } else { down_name },
                kind,
                basic,
                accidental: None,
            });
        }
    }
    // The close marks first, a staccato first among them; then the rest.
    list.sort_by_key(|m| (m.kind != Kind::Close, m.tag));
    for ornament in ornaments {
        let up = above(Kind::Ornament, chord);
        list.push(Mark {
            tag: super::NoteMarkPart::ornament_tag(ornament.kind),
            above: up,
            name: ornament_glyph(ornament.kind),
            kind: Kind::Ornament,
            basic: false,
            accidental: Some((
                ornament
                    .accidental_above
                    .as_ref()
                    .and_then(crate::engrave_theory::accidental_glyph),
                ornament
                    .accidental_below
                    .as_ref()
                    .and_then(crate::engrave_theory::accidental_glyph),
            )),
        });
    }

    let close_above = list
        .iter()
        .filter(|m| m.kind == Kind::Close && m.above)
        .count() as i32;
    let close_below = list
        .iter()
        .filter(|m| m.kind == Kind::Close && !m.above)
        .count() as i32;
    let outside_above = list.iter().any(|m| m.kind != Kind::Close && m.above);
    let outside_below = list.iter().any(|m| m.kind != Kind::Close && !m.above);

    // The close marks, in the staff's spaces near the note.
    // The outer edge and the height of the last close mark on each side.
    let mut close_edge_above: Option<f32> = None;
    let mut close_edge_below: Option<f32> = None;
    let mut close_height_above: Option<f32> = None;
    let mut close_height_below: Option<f32> = None;
    let mut staccato_outside = false;
    for m in list.iter().filter(|m| m.kind == Kind::Close) {
        let b = bbox(m.name);
        let h = b.top.0 - b.bottom.0;
        let head_side = m.above != seg.up || !seg.drawn;
        let x = centred(m.name, centre_x(chord, !head_side));
        // A tie on the mark's side leaves room: in the staff a head in a
        // space sets the mark a line further out; set from the head, it
        // stands further off by its kind.
        let tied = if m.above {
            chord.tie_above
        } else {
            chord.tie_below
        };
        let tie_distance = match (tied, m.tag == EventMark::Staccato.tag()) {
            (false, _) => 0.0,
            (true, true) => STACCATO_TIE_DISTANCE,
            (true, false) => TENUTO_TIE_DISTANCE,
        };
        let y_bottom = if m.above {
            let bottom = if head_side {
                let mut line = index(chord, high).min(9);
                if outside_above && close_above > 0 {
                    line = line.min(1 + close_above * 2);
                }
                if line > 1 {
                    if tied && line % 2 != 0 {
                        line -= 1;
                    }
                    at_index(chord, ((line + 1) & !1) - 3) - h * 0.5
                } else {
                    head_top + PROPERTY_DISTANCE + tie_distance
                }
            } else {
                stem_side_above(chord, h)
            };
            // A further mark on the side, each from where it would stand
            // alone: a space further in the staff (by its note, or its stem
            // side's place under the top line), else past the one before by
            // its height and `articulationMinDistance`
            // (`ChordLayout::layoutArticulations`).
            let in_staff = if head_side {
                index(chord, high) > 2
            } else {
                bottom < top_line - 1e-3
            };
            match close_height_above {
                Some(_) if in_staff => bottom + 1.0,
                Some(previous) => bottom + previous + ARTICULATION_MIN_DISTANCE,
                None => bottom,
            }
        } else {
            let top = if head_side {
                let mut line = index(chord, low).max(-1);
                if outside_below && close_below > 0 {
                    line = line.max(8 - (1 + close_below * 2));
                }
                if line < 7 {
                    if tied && line % 2 != 0 {
                        line += 1;
                    }
                    at_index(chord, (line & !1) + 3) + h * 0.5
                } else {
                    head_bottom - PROPERTY_DISTANCE - tie_distance
                }
            } else {
                stem_side_below(chord, h)
            };
            let in_staff = if head_side {
                index(chord, low) < 6
            } else {
                top > bottom_line + 0.5 + 1e-3
            };
            let top = match close_height_below {
                Some(_) if in_staff => top - 1.0,
                Some(previous) => top - previous - ARTICULATION_MIN_DISTANCE,
                None => top,
            };
            top - h
        };
        let y = y_bottom - b.bottom.0;
        if m.above {
            close_edge_above = Some(y_bottom + h);
            close_height_above = Some(h);
        } else {
            close_edge_below = Some(y_bottom);
            close_height_below = Some(h);
        }
        if m.tag == EventMark::Staccato.tag() {
            staccato_outside = if m.above {
                y_bottom >= top_line - 1e-3
            } else {
                y_bottom + h <= bottom_line + 1e-3
            };
        }
        placed.glyphs.push(MarkGlyph {
            tag: m.tag,
            name: m.name,
            x,
            y,
            at_stem_end: !head_side,
            room: false,
        });
    }

    // An accent the stack sets straight after a staccato on its side, the
    // staccato standing outside the staff, stands `STACCATO_ACCENT_KERN`
    // nearer it, as MuseScore kerns the two; a tenuto or another mark
    // between them parts them (`ChordLayout::layoutArticulations2`).
    let close: Vec<&Mark> = list.iter().filter(|m| m.kind == Kind::Close).collect();
    let kerned = match (
        close.as_slice(),
        list.iter().find(|m| m.kind != Kind::Close),
    ) {
        ([staccato], Some(accent))
            if staccato.tag == EventMark::Staccato.tag()
                && accent.tag == EventMark::Accent.tag()
                && accent.above == staccato.above
                && staccato_outside =>
        {
            Some(accent.tag)
        }
        _ => None,
    };

    // The outside marks, stacked outward from the staff or the chord,
    // whichever stands further out, and clear of the close marks.
    let stem = stem_end(chord);
    let mut base_above = {
        let chord_top = match stem {
            Some(end) if seg.up => end,
            _ => head_top,
        } + PROPERTY_DISTANCE;
        let chord_top = match close_edge_above {
            Some(edge) => chord_top.max(edge + ARTICULATION_MIN_DISTANCE),
            None => chord_top,
        };
        (top_line + PROPERTY_DISTANCE).max(chord_top)
    };
    let mut base_below = {
        let chord_bottom = match stem {
            Some(end) if !seg.up => end,
            _ => head_bottom,
        } - PROPERTY_DISTANCE;
        let chord_bottom = match close_edge_below {
            Some(edge) => chord_bottom.min(edge - ARTICULATION_MIN_DISTANCE),
            None => chord_bottom,
        };
        (bottom_line - PROPERTY_DISTANCE).min(chord_bottom)
    };
    for m in list.iter().filter(|m| m.kind != Kind::Close) {
        let b = bbox(m.name);
        let stem_side = m.basic && (m.above == seg.up) && seg.drawn;
        let at_stem_end = m.above == seg.up && stem.is_some();
        let x = centred(m.name, centre_x(chord, stem_side));
        // MuseScore's autoplace then lifts the mark clear of what stands
        // under it across its width, its chord's own accidentals among them,
        // by `articulationMinDistance` (`Autoplace::autoplaceSegmentElement`).
        let across = chord
            .accidental_ink
            .iter()
            .filter(|a| a[0] < x + b.right.0 && a[2] > x + b.left.0);
        // An ornament's accidentals, at 0.6 of their size, centred over and
        // under it with 0.35 between.
        let small = |name: &'static str| bbox(name);
        let kern = if kerned == Some(m.tag) {
            STACCATO_ACCENT_KERN
        } else {
            0.0
        };
        if m.above {
            let mut bottom = across
                .map(|a| a[3] + ARTICULATION_MIN_DISTANCE)
                .fold(base_above - kern, f32::max);
            if let Some((_, Some(below))) = m.accidental {
                let a = small(below);
                let y = bottom - 0.6 * a.bottom.0;
                placed.accidentals.push(SmallAccidental {
                    tag: m.tag,
                    name: below,
                    x: x + (b.left.0 + b.right.0) * 0.5 - 0.6 * (a.left.0 + a.right.0) * 0.5,
                    y,
                    at_stem_end,
                    above: false,
                });
                bottom += 0.6 * (a.top.0 - a.bottom.0) + 0.35;
            }
            let y = bottom - b.bottom.0;
            placed.glyphs.push(MarkGlyph {
                tag: m.tag,
                name: m.name,
                x,
                y,
                at_stem_end,
                room: false,
            });
            let mut top = y + b.top.0;
            if let Some((Some(above_name), _)) = m.accidental {
                let a = small(above_name);
                let y = top + 0.35 - 0.6 * a.bottom.0;
                placed.accidentals.push(SmallAccidental {
                    tag: m.tag,
                    name: above_name,
                    x: x + (b.left.0 + b.right.0) * 0.5 - 0.6 * (a.left.0 + a.right.0) * 0.5,
                    y,
                    at_stem_end,
                    above: true,
                });
                top = y + 0.6 * a.top.0;
            }
            base_above = top + ARTICULATION_MIN_DISTANCE;
        } else {
            let mut top = across
                .map(|a| a[1] - ARTICULATION_MIN_DISTANCE)
                .fold(base_below + kern, f32::min);
            if let Some((Some(above_name), _)) = m.accidental {
                let a = small(above_name);
                let y = top - 0.6 * a.top.0;
                placed.accidentals.push(SmallAccidental {
                    tag: m.tag,
                    name: above_name,
                    x: x + (b.left.0 + b.right.0) * 0.5 - 0.6 * (a.left.0 + a.right.0) * 0.5,
                    y,
                    at_stem_end,
                    above: true,
                });
                top -= 0.6 * (a.top.0 - a.bottom.0) + 0.35;
            }
            let y = top - b.top.0;
            placed.glyphs.push(MarkGlyph {
                tag: m.tag,
                name: m.name,
                x,
                y,
                at_stem_end,
                room: false,
            });
            let mut bottom = y + b.bottom.0;
            if let Some((_, Some(below))) = m.accidental {
                let a = small(below);
                let y = bottom - 0.35 - 0.6 * a.top.0;
                placed.accidentals.push(SmallAccidental {
                    tag: m.tag,
                    name: below,
                    x: x + (b.left.0 + b.right.0) * 0.5 - 0.6 * (a.left.0 + a.right.0) * 0.5,
                    y,
                    at_stem_end,
                    above: false,
                });
                bottom = y + 0.6 * a.bottom.0;
            }
            base_below = bottom - ARTICULATION_MIN_DISTANCE;
        }
    }

    // A one-note tremolo: its strokes on the stem, 0.5 in from its tip and
    // further in past a flag or beams, kept off the staff's outer line on
    // the head's side; over a stemless note's head, 1.25 off it.
    for mark in marks {
        let EventMark::Tremolo { strokes } = mark else {
            continue;
        };
        let name = tremolo_glyph(*strokes);
        let b = bbox(name);
        let h = b.top.0 - b.bottom.0;
        let centre_y;
        let x;
        if seg.drawn {
            x = seg.dx + seg.x_off;
            let mut inset = TREMOLO_OUTSIDE_PADDING;
            if seg.flag.is_some() {
                inset += if seg.up { 1.5 } else { 1.0 };
                if seg.beams >= 2 {
                    inset += 0.5;
                }
            } else if chord.beamed {
                inset += f32::from(seg.beams) * 0.75 - 0.25;
            }
            if seg.up {
                let top = seg.end - inset;
                centre_y = (top - h * 0.5).max(bottom_line + h * 0.5);
            } else {
                let bottom = seg.end + inset;
                centre_y = (bottom + h * 0.5).min(top_line - h * 0.5);
            }
        } else {
            x = seg.dx + (head.left.0 + head.right.0) * 0.5;
            let bottom = high + TREMOLO_NOTE_SIDE_PADDING - TREMOLO_OUTSIDE_PADDING;
            centre_y = bottom + h * 0.5;
        }
        placed.glyphs.push(MarkGlyph {
            tag: mark.tag(),
            name,
            x: x - (b.left.0 + b.right.0) * 0.5,
            y: centre_y - (b.bottom.0 + b.top.0) * 0.5,
            at_stem_end: seg.drawn,
            room: true,
        });
    }

    // Jazz marks beside the head: a scoop and a plop into it from the left,
    // below and above, a doit and a fall out of it to the right, above and
    // below.
    for mark in marks {
        let (name, left, upper) = match mark {
            EventMark::Scoop => ("brassScoop", true, false),
            EventMark::Plop => ("brassPlop", true, true),
            EventMark::Doit => ("brassDoitShort", false, true),
            EventMark::Falloff => ("brassFallLipShort", false, false),
            _ => continue,
        };
        let b = bbox(name);
        let (head_y, head_x) = if upper {
            (high, (chord.heads_left, chord.heads_right))
        } else {
            (low, (chord.heads_left, chord.heads_right))
        };
        let x = if left {
            head_x.0 - JAZZ_GAP - b.right.0
        } else {
            head_x.1 + JAZZ_GAP - b.left.0
        };
        let y = if upper {
            head_y - b.bottom.0
        } else {
            head_y - b.top.0
        };
        placed.glyphs.push(MarkGlyph {
            tag: mark.tag(),
            name,
            x,
            y,
            at_stem_end: false,
            room: true,
        });
    }

    // An arpeggio left of the chord, from its lowest head to its highest,
    // and further left past the ledger line a head stands on, the first a
    // space under the staff or over it; and left of any later voice's chord
    // in its staff's column that its height meets and that reaches further
    // left, as MuseScore sets an arpeggio left of the leftmost chord its span,
    // its own track and those after it, covers (`minChordX`, `maxChordPad`).
    let ledgered =
        !chord.accidentals && (low <= chord.yo - 1.0 + 1e-3 || high >= chord.yo + 5.0 - 1e-3);
    let gap = if ledgered {
        ARPEGGIO_LEDGER_DISTANCE + LEDGER_LINE_LENGTH
    } else {
        ARPEGGIO_NOTE_DISTANCE
    };
    for mark in marks {
        if let EventMark::Arpeggio { direction } = mark {
            placed.arpeggio = Some(Arpeggio {
                direction: *direction,
                right: chord.heads_left.min(chord.column_left) - gap,
                bottom: low - 0.5,
                top: high + 0.5,
            });
        }
    }
    placed
}

/// A close mark on the stem's side above an up stem: in the staff's space
/// past the stem's end, a space further where a beam's end leaves it too
/// close, or 0.4 past the end outside the staff. Returns its bottom edge.
fn stem_side_above(chord: &Chord, h: f32) -> f32 {
    let end = stem_end(chord).unwrap_or(chord.seg.hi);
    let d = chord.yo + 4.0 - end;
    let mut line = (2.0 * d - 1.0).round() as i32;
    if line > 0 && line % 2 == 0 {
        line -= 1;
    }
    let dist = d - line as f32 * 0.5;
    if line > 0 && chord.beamed && dist < PROPERTY_DISTANCE {
        line = (line - 2).max(0);
    }
    if line > 0 {
        at_index(chord, line) - h * 0.5
    } else if line == 0 {
        chord.yo + 4.0 + PROPERTY_DISTANCE
    } else {
        end + PROPERTY_DISTANCE
    }
}

/// A close mark on the stem's side below a down stem; returns its top edge.
fn stem_side_below(chord: &Chord, h: f32) -> f32 {
    let end = stem_end(chord).unwrap_or(chord.seg.lo);
    let d = chord.yo + 4.0 - end;
    let mut line = (2.0 * d + 1.0).round() as i32;
    if line < 8 && line % 2 == 0 {
        line += 1;
    }
    let dist = line as f32 * 0.5 - d;
    if line < 8 && chord.beamed && dist < PROPERTY_DISTANCE {
        line = (line + 2).min(8);
    }
    if line < 8 {
        at_index(chord, line) + h * 0.5
    } else if line == 8 {
        chord.yo - PROPERTY_DISTANCE
    } else {
        end - PROPERTY_DISTANCE
    }
}

/// An arpeggio's symbols, from its run's origin at its right edge and
/// bottom: wiggles turned upright a quarter anticlockwise, Bravura's
/// `wiggleArpeggiatoUp` tiling at its advance, covering its span; an up
/// arpeggio's last an arrow pointing up, a down one's first an arrow
/// pointing down (each of Bravura's arrowed wiggles turned the same way).
pub(super) fn arpeggio_symbols(arpeggio: &Arpeggio) -> Vec<Symbol> {
    const WIGGLE: char = '\u{EAA9}';
    const UP: char = '\u{EAAD}';
    const DOWN: char = '\u{EAAE}';
    // A plain wiggle reaches 0.13 before its origin and 1.17 after it; the
    // down arrow 2.26 after its own, its wiggle ending where the next one's
    // starts at 2.11.
    let height = (arpeggio.top - arpeggio.bottom).max(1.3);
    let symbol = |codepoint: char, y: f32| Symbol {
        codepoint,
        at: SymbolPlace::Origin(Point::new(0.0, y)),
        scale: 1.0,
        turn: Turn::Anticlockwise,
    };
    let mut out = Vec::new();
    let mut y = 0.13;
    if arpeggio.direction == ArpeggioDirection::Down {
        out.push(symbol(DOWN, 0.0));
        y = 2.106;
    }
    while out.is_empty() || y - 1.02 + 1.17 < height {
        out.push(symbol(WIGGLE, y));
        y += ARPEGGIO_WIGGLE_STEP;
    }
    if arpeggio.direction == ArpeggioDirection::Up {
        if let Some(last) = out.last_mut() {
            last.codepoint = UP;
        }
    }
    out
}

/// The SMuFL codepoint of an accidental an ornament may carry, drawn small.
pub(super) fn accidental_codepoint(name: &str) -> Option<char> {
    Some(match name {
        "accidentalFlat" => '\u{E260}',
        "accidentalNatural" => '\u{E261}',
        "accidentalSharp" => '\u{E262}',
        "accidentalDoubleSharp" => '\u{E263}',
        "accidentalDoubleFlat" => '\u{E264}',
        _ => return None,
    })
}

/// Folds what was placed for a chord into its column's ink, so a curve
/// passes over it.
pub(super) fn into_ink(ink: &mut ColumnInk, placed: &Placed) {
    if let Some((bottom, top)) = placed.reach() {
        ink.top = ink.top.max(top);
        ink.bottom = ink.bottom.min(bottom);
    }
}
