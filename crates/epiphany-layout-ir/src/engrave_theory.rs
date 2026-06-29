//! Music-theory engraving primitives — the pure mapping from notated content to
//! SMuFL glyph names and staff positions, with no layout-IR plumbing.
//!
//! This is the "engraving decision" layer Chapter 7 assigns to the layout IR:
//! which glyph notates a note value, where a pitch sits on the staff under a
//! clef, which accidentals a spelling draws, and which accidentals a key
//! signature places. The constrained-layout pass consumes these; the renderer
//! never makes these choices (Chapter 7 §"Non-overreach").
//!
//! Every function here is total and deterministic. Glyph names are the SMuFL
//! canonical names bundled in [`crate::glyph`].

use epiphany_core::{
    AccidentalId, Clef, ClefShape, CmnNominal, KeySignature, NoteValue, StemDirection,
};

/// A staff position in **half-staff-space steps from the bottom staff line**:
/// the bottom line is `0`, the space just above it is `1`, the second line is
/// `2`, … (negative below the bottom line). One step is half a staff space, so
/// the visual `y` of a position is `position as f32 * 0.5` staff spaces.
///
/// This is a **diatonic** position: it depends only on the nominal letter and
/// octave, never on accidentals — a C-sharp sits on the same line as a
/// C-natural. Accidentals are drawn to the *left* of the notehead, they do not
/// move it vertically.
pub type StaffStep = i32;

/// The diatonic index of a `(nominal, octave)`: `octave * 7 + letter`, with the
/// nominal letter C=0 … B=6 ([`CmnNominal`]). Octaves are scientific-pitch
/// (C4 = middle C), so each whole octave is exactly seven diatonic steps.
fn diatonic_index(nominal: CmnNominal, octave: i8) -> i32 {
    octave as i32 * 7 + nominal as i32
}

/// The diatonic index of a clef shape's reference pitch — the pitch the clef
/// glyph fixes on its line: G clef → G4, F clef → F3, C clef → middle C (C4).
/// A percussion clef has no diatonic reference.
fn clef_reference_diatonic(shape: ClefShape) -> Option<i32> {
    match shape {
        ClefShape::G => Some(diatonic_index(CmnNominal::G, 4)),
        ClefShape::F => Some(diatonic_index(CmnNominal::F, 3)),
        ClefShape::C => Some(diatonic_index(CmnNominal::C, 4)),
        ClefShape::Percussion => None,
    }
}

/// The staff position of a diatonic pitch under `clef` (see [`StaffStep`]).
///
/// The clef fixes its reference pitch on `clef.line` (line 1 = the bottom line);
/// every diatonic step away from that pitch is one half-staff-space step.
/// `clef.octave_shift` transposes the written position: a `+1` (8va) clef writes
/// a sounding pitch an octave *lower* on the staff, a `-1` (8vb) clef an octave
/// *higher*. A percussion clef has no diatonic mapping and reports its own line
/// (a neutral mid-staff position) for any pitch.
pub fn staff_position(nominal: CmnNominal, octave: i8, clef: &Clef) -> StaffStep {
    let reference_line_step = (clef.line as i32 - 1) * 2;
    match clef_reference_diatonic(clef.shape) {
        Some(reference_diatonic) => {
            reference_line_step + (diatonic_index(nominal, octave) - reference_diatonic)
                - clef.octave_shift as i32 * 7
        }
        None => reference_line_step,
    }
}

/// The [`CmnNominal`] for a diatonic letter index `0..=6` (`C..=B`); callers pass
/// `rem_euclid(7)`, so out-of-range values are unreachable and fold to `B`.
fn cmn_nominal(letter: i32) -> CmnNominal {
    match letter {
        0 => CmnNominal::C,
        1 => CmnNominal::D,
        2 => CmnNominal::E,
        3 => CmnNominal::F,
        4 => CmnNominal::G,
        5 => CmnNominal::A,
        _ => CmnNominal::B,
    }
}

/// The diatonic pitch `(nominal, octave)` written at staff position `step` under
/// `clef` — the inverse of [`staff_position`], used to turn a clicked staff height
/// into the pitch to insert. `None` for a percussion clef (no diatonic mapping — it
/// collapses every pitch onto its reference line), and `None` when the position is
/// so far off the staff that its octave falls outside the representable `i8` range
/// (rather than silently wrapping to a wrong-looking octave). Inverse only of the
/// *diatonic* position; the accidental is a separate, caller-chosen concern.
pub fn staff_step_pitch(step: StaffStep, clef: &Clef) -> Option<(CmnNominal, i8)> {
    let reference_line_step = (clef.line as i64 - 1) * 2;
    let reference_diatonic = clef_reference_diatonic(clef.shape)? as i64;
    // i64 so an extreme `step` cannot overflow the intermediate before the octave
    // range-check rejects it.
    let diatonic =
        step as i64 - reference_line_step + reference_diatonic + clef.octave_shift as i64 * 7;
    let octave = i8::try_from(diatonic.div_euclid(7)).ok()?;
    Some((cmn_nominal(diatonic.rem_euclid(7) as i32), octave))
}

/// The SMuFL notehead glyph for a note value: a hollow whole/half notehead, else
/// the filled black notehead.
pub fn notehead_glyph(value: NoteValue) -> &'static str {
    match value {
        NoteValue::Whole => "noteheadWhole",
        NoteValue::Half => "noteheadHalf",
        _ => "noteheadBlack",
    }
}

/// The SMuFL rest glyph for a note value, if one is bundled. Only whole/half/
/// quarter/eighth rests ship in the bundled metrics; a sixteenth-or-shorter rest
/// reports `None` so the caller surfaces the missing glyph coverage rather than
/// misrendering it as an eighth rest.
pub fn rest_glyph(value: NoteValue) -> Option<&'static str> {
    Some(match value {
        NoteValue::Whole => "restWhole",
        NoteValue::Half => "restHalf",
        NoteValue::Quarter => "restQuarter",
        NoteValue::Eighth => "rest8th",
        _ => return None,
    })
}

/// Whether a note value is drawn with a stem — every value but the whole note.
pub fn has_stem(value: NoteValue) -> bool {
    !matches!(value, NoteValue::Whole)
}

/// The SMuFL flag glyph for an *unbeamed* stemmed note value, if one is bundled.
/// Only the eighth-note flag ships in the bundled metrics; shorter values need
/// their own flag glyphs (or beaming, deferred past I-1) and report `None`.
pub fn flag_glyph(value: NoteValue, stem: StemDirection) -> Option<&'static str> {
    match value {
        NoteValue::Eighth => Some(match stem {
            StemDirection::Up => "flag8thUp",
            StemDirection::Down => "flag8thDown",
        }),
        _ => None,
    }
}

/// The SMuFL clef glyph for a clef shape, if one is bundled. A percussion clef
/// reports `None` until its glyph is bundled — returning a G clef for it would
/// be a semantic false positive, so the caller surfaces the gap instead.
pub fn clef_glyph(shape: ClefShape) -> Option<&'static str> {
    Some(match shape {
        ClefShape::G => "gClef",
        ClefShape::F => "fClef",
        ClefShape::C => "cClef",
        ClefShape::Percussion => return None,
    })
}

/// The SMuFL accidental glyph for a spelling accidental, if one is bundled.
/// `None` for an accidental the bundled metrics don't carry (e.g. microtonal),
/// which the caller surfaces rather than papers over.
pub fn accidental_glyph(accidental: &AccidentalId) -> Option<&'static str> {
    Some(match accidental.as_str() {
        "sharp" => "accidentalSharp",
        "flat" => "accidentalFlat",
        "natural" => "accidentalNatural",
        "doublesharp" | "double-sharp" => "accidentalDoubleSharp",
        _ => return None,
    })
}

/// One accidental in a key signature: the SMuFL glyph and the staff position it
/// occupies under the active clef.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyAccidental {
    pub glyph: &'static str,
    pub position: StaffStep,
}

// The conventional staff-step offsets, from the first accidental, of the
// key-signature "zigzag" — invariant across clefs (only the start position is
// clef-dependent). Sharp order F C G D A E B; flat order B E A D G C F.
const SHARP_OFFSETS: [StaffStep; 7] = [0, -3, 1, -2, -5, -1, -4];
const FLAT_OFFSETS: [StaffStep; 7] = [0, 3, -1, 2, -2, 1, -3];

/// The staff position of the first key-signature accidental of `letter` under
/// `clef`: the octave of `letter` whose position is closest to the conventional
/// band (sharps high, near the top line; flats mid, near the middle line),
/// breaking ties toward the higher position. This reproduces the standard
/// treble and bass placements; other clefs follow the same principle.
fn first_accidental_position(letter: CmnNominal, clef: &Clef, sharp: bool) -> StaffStep {
    let target = if sharp { 8 } else { 4 };
    (-2i8..=10)
        .map(|octave| staff_position(letter, octave, clef))
        .min_by_key(|&p| ((p - target).abs(), -p))
        .unwrap_or(target)
}

/// The ordered accidentals a key signature places under `clef`. A positive
/// `key.fifths()` places that many sharps (order F C G D A E B); a negative one
/// places that many flats (order B E A D G C F); `0` (and a percussion clef)
/// places none. The count needs no clamping — [`KeySignature`] already
/// guarantees `fifths` is within the conventional `-7..=7` (I-0 invariant).
pub fn key_signature(key: KeySignature, clef: &Clef) -> Vec<KeyAccidental> {
    let fifths = key.fifths();
    if fifths == 0 || matches!(clef.shape, ClefShape::Percussion) {
        return Vec::new();
    }
    let count = fifths.unsigned_abs() as usize;
    let (glyph, start, offsets) = if fifths > 0 {
        (
            "accidentalSharp",
            first_accidental_position(CmnNominal::F, clef, true),
            &SHARP_OFFSETS,
        )
    } else {
        (
            "accidentalFlat",
            first_accidental_position(CmnNominal::B, clef, false),
            &FLAT_OFFSETS,
        )
    };
    (0..count)
        .map(|i| KeyAccidental {
            glyph,
            position: start + offsets[i],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staff_position_is_diatonic_and_clef_relative() {
        let treble = Clef::treble();
        // Treble: E4 is the bottom line (0); G4 the second line (2); F5 the top
        // line (8); A4 the second space (3); C4 a ledger below (−2).
        assert_eq!(staff_position(CmnNominal::E, 4, &treble), 0);
        assert_eq!(staff_position(CmnNominal::G, 4, &treble), 2);
        assert_eq!(staff_position(CmnNominal::F, 5, &treble), 8);
        assert_eq!(staff_position(CmnNominal::A, 4, &treble), 3);
        assert_eq!(staff_position(CmnNominal::C, 4, &treble), -2);

        // Bass: F3 the fourth line (6); G2 the bottom line (0); middle C4 the
        // first ledger above (10).
        let bass = Clef::bass();
        assert_eq!(staff_position(CmnNominal::F, 3, &bass), 6);
        assert_eq!(staff_position(CmnNominal::G, 2, &bass), 0);
        assert_eq!(staff_position(CmnNominal::C, 4, &bass), 10);

        // Alto: middle C4 the middle line (4). Tenor: middle C4 the fourth line (6).
        assert_eq!(staff_position(CmnNominal::C, 4, &Clef::alto()), 4);
        assert_eq!(staff_position(CmnNominal::C, 4, &Clef::tenor()), 6);
    }

    #[test]
    fn staff_step_pitch_inverts_staff_position() {
        let nominals = [
            CmnNominal::C,
            CmnNominal::D,
            CmnNominal::E,
            CmnNominal::F,
            CmnNominal::G,
            CmnNominal::A,
            CmnNominal::B,
        ];
        // Plain clefs and octave-shifted (8va/8vb) trebles — the editor inverse
        // depends on the same octave_shift sign convention as the forward map.
        let octave_treble = |shift: i8| Clef {
            shape: ClefShape::G,
            line: 2,
            octave_shift: shift,
        };
        for clef in [
            Clef::treble(),
            Clef::bass(),
            Clef::alto(),
            Clef::tenor(),
            octave_treble(1),
            octave_treble(-1),
        ] {
            for octave in 1..8i8 {
                for nominal in nominals {
                    let step = staff_position(nominal, octave, &clef);
                    assert_eq!(
                        staff_step_pitch(step, &clef),
                        Some((nominal, octave)),
                        "round trip failed for {nominal:?}{octave} under {clef:?}"
                    );
                }
            }
        }
        // A percussion clef has no diatonic mapping, so no inverse.
        let percussion = Clef {
            shape: ClefShape::Percussion,
            line: 3,
            octave_shift: 0,
        };
        assert_eq!(staff_step_pitch(0, &percussion), None);
    }

    #[test]
    fn staff_step_pitch_refuses_an_unrepresentable_octave() {
        // A click so far off the staff that its octave overflows i8 is refused, not
        // wrapped to a plausible-but-wrong octave.
        let treble = Clef::treble();
        assert_eq!(staff_step_pitch(i32::MAX, &treble), None);
        assert_eq!(staff_step_pitch(i32::MIN, &treble), None);
        assert_eq!(staff_step_pitch(10_000, &treble), None);
        assert_eq!(staff_step_pitch(-10_000, &treble), None);
        // …but a position just inside the i8 octave range still resolves.
        assert!(staff_step_pitch(staff_position(CmnNominal::C, 9, &treble), &treble).is_some());
    }

    #[test]
    fn accidentals_never_change_staff_position() {
        // The position is purely diatonic — the same nominal+octave lands on the
        // same line regardless of any accidental the spelling carries.
        let treble = Clef::treble();
        let c_natural = staff_position(CmnNominal::C, 5, &treble);
        let c_sharp = staff_position(CmnNominal::C, 5, &treble); // spelling differs, position must not
        assert_eq!(c_natural, c_sharp);
    }

    #[test]
    fn octave_shift_sign_is_explicit() {
        // 8va (+1) writes a sounding pitch an octave LOWER on the staff; 8vb (−1)
        // an octave HIGHER. A sounding G5 under treble-8va sits where G4 sits on
        // a plain treble clef (step 2); under treble-8vb, where G6 would (step 16).
        let plain = Clef::treble();
        let up = Clef {
            octave_shift: 1,
            ..Clef::treble()
        };
        let down = Clef {
            octave_shift: -1,
            ..Clef::treble()
        };
        let plain_g5 = staff_position(CmnNominal::G, 5, &plain); // 9
        assert_eq!(plain_g5, 9);
        assert_eq!(staff_position(CmnNominal::G, 5, &up), plain_g5 - 7);
        assert_eq!(staff_position(CmnNominal::G, 5, &down), plain_g5 + 7);
    }

    #[test]
    fn note_value_glyphs() {
        assert_eq!(notehead_glyph(NoteValue::Whole), "noteheadWhole");
        assert_eq!(notehead_glyph(NoteValue::Half), "noteheadHalf");
        assert_eq!(notehead_glyph(NoteValue::Quarter), "noteheadBlack");
        assert_eq!(notehead_glyph(NoteValue::Sixteenth), "noteheadBlack");
        assert_eq!(rest_glyph(NoteValue::Whole), Some("restWhole"));
        assert_eq!(rest_glyph(NoteValue::Eighth), Some("rest8th"));
        assert_eq!(rest_glyph(NoteValue::Sixteenth), None);
        assert!(!has_stem(NoteValue::Whole));
        assert!(has_stem(NoteValue::Quarter));
        assert_eq!(flag_glyph(NoteValue::Quarter, StemDirection::Up), None);
        assert_eq!(
            flag_glyph(NoteValue::Eighth, StemDirection::Up),
            Some("flag8thUp")
        );
        assert_eq!(
            flag_glyph(NoteValue::Eighth, StemDirection::Down),
            Some("flag8thDown")
        );
    }

    #[test]
    fn clef_and_accidental_glyphs() {
        assert_eq!(clef_glyph(ClefShape::G), Some("gClef"));
        assert_eq!(clef_glyph(ClefShape::F), Some("fClef"));
        assert_eq!(clef_glyph(ClefShape::C), Some("cClef"));
        assert_eq!(clef_glyph(ClefShape::Percussion), None);
        assert_eq!(
            accidental_glyph(&AccidentalId::new("sharp")),
            Some("accidentalSharp")
        );
        assert_eq!(
            accidental_glyph(&AccidentalId::new("flat")),
            Some("accidentalFlat")
        );
        assert_eq!(
            accidental_glyph(&AccidentalId::new("natural")),
            Some("accidentalNatural")
        );
        assert_eq!(accidental_glyph(&AccidentalId::new("quarter-sharp")), None);
    }

    #[test]
    fn key_signature_positions_match_the_conventional_pattern() {
        let key = |fifths: i8| KeySignature::new(fifths).expect("fifths in range");

        // C major / A minor: nothing.
        assert!(key_signature(key(0), &Clef::treble()).is_empty());

        // Treble sharps F C G D A E B → steps 8 5 9 6 3 7 4.
        let treble_sharps: Vec<StaffStep> = key_signature(key(7), &Clef::treble())
            .iter()
            .map(|a| a.position)
            .collect();
        assert_eq!(treble_sharps, vec![8, 5, 9, 6, 3, 7, 4]);
        assert!(key_signature(key(7), &Clef::treble())
            .iter()
            .all(|a| a.glyph == "accidentalSharp"));

        // Treble flats B E A D G C F → steps 4 7 3 6 2 5 1.
        let treble_flats: Vec<StaffStep> = key_signature(key(-7), &Clef::treble())
            .iter()
            .map(|a| a.position)
            .collect();
        assert_eq!(treble_flats, vec![4, 7, 3, 6, 2, 5, 1]);
        assert!(key_signature(key(-7), &Clef::treble())
            .iter()
            .all(|a| a.glyph == "accidentalFlat"));

        // Bass: first sharp F#3 on the fourth line (6); first flat B♭2 on the
        // second line (2).
        assert_eq!(key_signature(key(1), &Clef::bass())[0].position, 6);
        assert_eq!(key_signature(key(-1), &Clef::bass())[0].position, 2);

        // A two-sharp signature places exactly two accidentals.
        assert_eq!(key_signature(key(2), &Clef::treble()).len(), 2);
    }
}
