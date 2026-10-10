//! A score as a transposed score shows it. The model holds every pitch and
//! key at concert pitch; a score whose file is set transposed draws each
//! transposing part at written pitch under its written key. [`written_view`]
//! makes that score from the concert one, for the projection to draw.

use std::collections::BTreeMap;

use epiphany_core::{
    Event, KeySignature, PitchId, Score, SpellingDirective, SpellingScope, StaffId,
    TranspositionInterval,
};

/// The fifths a transposition adds to a key: `7` per semitone less `12` per
/// diatonic step, so a B-flat instrument's `(-1, -2)` takes two flats and an
/// octave none.
fn key_shift(interval: TranspositionInterval) -> i32 {
    7 * interval.chromatic_steps - 12 * interval.diatonic_steps
}

/// `interval` in a pitch's own space: a quarter-tone space counts twice the
/// steps per semitone.
fn in_space(interval: TranspositionInterval, space: &str) -> TranspositionInterval {
    if space == "cmn-24" {
        TranspositionInterval {
            diatonic_steps: interval.diatonic_steps,
            chromatic_steps: interval.chromatic_steps * 2,
        }
    } else {
        interval
    }
}

/// `score` with each transposing part at written pitch: every pitch of a
/// staff whose instrument transposes moved back by the transposition, its
/// explicit spelling with it, and each key of the staff by the same interval.
/// A part that does not transpose is unchanged, as is a pitch the interval
/// cannot move (it keeps its concert pitch) and a spelling that cannot follow
/// (the pitch then takes its own). A key that the move would take past seven
/// accidentals is written enharmonically, twelve fifths nearer.
pub fn written_view(score: &Score) -> Score {
    let mut view = score.clone();
    let to_written: BTreeMap<StaffId, TranspositionInterval> = score
        .staves
        .iter()
        .filter_map(|staff| {
            let instrument = score
                .instruments
                .iter()
                .find(|i| i.id == staff.instrument)?;
            Some((staff.id, instrument.transposition?.inverse()?))
        })
        .collect();
    if to_written.is_empty() {
        return view;
    }
    let mut events = Vec::new();
    for region in &mut view.canvas.regions {
        let Some(instances) = region.content.staff_instances_mut() else {
            continue;
        };
        for instance in instances {
            let Some(interval) = to_written.get(&instance.staff).copied() else {
                continue;
            };
            for change in &mut instance.key_sequence {
                let mut fifths = i32::from(change.key.fifths()) + key_shift(interval);
                while fifths > 7 {
                    fifths -= 12;
                }
                while fifths < -7 {
                    fifths += 12;
                }
                change.key = i8::try_from(fifths)
                    .ok()
                    .and_then(KeySignature::new)
                    .unwrap_or(change.key);
            }
            for voice in &instance.voices {
                events.extend(voice.events.iter().map(|event| (*event, interval)));
            }
        }
    }
    let mut moved: BTreeMap<PitchId, (TranspositionInterval, epiphany_core::Pitch)> =
        BTreeMap::new();
    for (id, interval) in events {
        let Some(Event::Pitched(event)) = view.events.get_mut(id) else {
            continue;
        };
        for identified in &mut event.pitches {
            let space = identified.pitch.scale_position.space.as_str().to_owned();
            let interval = in_space(interval, &space);
            if let Ok(pitch) = identified.pitch.transposed(interval) {
                identified.pitch = pitch.clone();
                moved.insert(identified.id, (interval, pitch));
            }
        }
    }
    view.spelling_attachments.retain_mut(|attachment| {
        let SpellingScope::Pitch(id) = attachment.scope else {
            return true;
        };
        let Some((interval, pitch)) = moved.get(&id) else {
            return true;
        };
        let SpellingDirective::Explicit(spelling) = &attachment.directive else {
            return true;
        };
        let rewritten = match pitch.twelve_tet_semitone() {
            Some(semitone) => spelling.transposed(*interval, semitone),
            None => pitch
                .quarter_tone_position()
                .and_then(|position| spelling.transposed_by_quarter_tones(*interval, position)),
        };
        match rewritten {
            Some(spelling) => {
                attachment.directive = SpellingDirective::Explicit(spelling);
                true
            }
            None => false,
        }
    });
    view
}
