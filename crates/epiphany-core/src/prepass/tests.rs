//! Tests for the spelling and notational-decomposition pre-passes.
//!
//! Spelling correctness is asserted against *unambiguous* tonal cases (diatonic
//! scales in sharp and flat keys, chromatic context), where the line-of-fifths
//! preference rule has one musically-correct answer. Decomposition correctness
//! is asserted at the grid level (exact note-value sequences) for the cases
//! Phase-2 names: plain values, syncopation, barline crossing, dots, tuplets.

use super::*;

use crate::event::{Event, PitchedEvent, Rest, StemConfiguration, UnpitchedEvent};
use crate::graph::{
    Canvas, Region, RegionContent, RegionTimeModel, Score, Staff, StaffBasedContent, StaffExtent,
    StaffInstance, StaffLineConfiguration, TimeExtent, Tuplet,
};
use crate::graph::{Instrument, Voice};
use crate::ids::{
    IdentityContext, InstrumentId, PitchId, RegionId, ReplicaId, StaffId, StaffInstanceId,
    TupletId, VoiceId,
};
use crate::pitch::{
    AccidentalId, AcousticPitch, AcousticRealization, CmnNominal, IdentifiedPitch, Pitch,
    PitchSpaceId, PitchSpacePosition, ScalePosition, SpellingAttachment, SpellingDirective,
    SpellingNominal, SpellingPrecedence, SpellingScope, SpellingSource, TuningReference,
};
use crate::time::{
    EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime, TimeAnchor,
    WallClockTime,
};
use crate::EventArena;

// ---------------------------------------------------------------------------
// Construction helpers
// ---------------------------------------------------------------------------

fn integer_pitch(index: i32) -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Integer {
                space_size: 12,
                index,
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

/// A just-intonation pitch whose 12-TET class is not determinable: spelling
/// unavailable.
fn ji_pitch() -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("ji-5limit"),
            position: PitchSpacePosition::JiVector {
                components: vec![0, 1, -1],
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

fn r(n: i64, d: i64) -> RationalTime {
    RationalTime::new(n, d).unwrap()
}

fn pitched(
    id: crate::ids::EventId,
    voice: VoiceId,
    pos: RationalTime,
    dur: RationalTime,
    pitches: Vec<IdentifiedPitch>,
) -> Event {
    Event::Pitched(PitchedEvent {
        id,
        voice,
        position: EventPosition::Musical(MusicalPosition(pos)),
        duration: EventDuration::Musical(MusicalDuration(dur)),
        pitches,
        articulations: vec![],
        dynamic: None,
        ornaments: vec![],
        stem: StemConfiguration,
        grace: None,
    })
}

fn rest(id: crate::ids::EventId, voice: VoiceId, pos: RationalTime, dur: RationalTime) -> Event {
    Event::Rest(Rest {
        id,
        voice,
        position: EventPosition::Musical(MusicalPosition(pos)),
        duration: EventDuration::Musical(MusicalDuration(dur)),
        vertical_position: None,
        visible: true,
    })
}

fn unpitched(
    id: crate::ids::EventId,
    voice: VoiceId,
    pos: RationalTime,
    dur: RationalTime,
) -> Event {
    Event::Unpitched(UnpitchedEvent {
        id,
        voice,
        position: EventPosition::Musical(MusicalPosition(pos)),
        duration: EventDuration::Musical(MusicalDuration(dur)),
        staff_position: crate::event::StaffPosition(0),
        instrument_member: crate::event::UnpitchedMemberId(0),
        articulations: vec![],
        dynamic: None,
        stem: StemConfiguration,
        grace: None,
    })
}

/// Wires a list of single-voice events into a one-region, one-staff metric
/// score (4/4-equivalent: the measure length defaults to a whole note). Events
/// must already be in ascending-position order.
fn metric_score(
    make: impl FnOnce(&mut IdentityContext, VoiceId) -> (Vec<Event>, Vec<Tuplet>),
) -> Score {
    let mut idc = IdentityContext::new(ReplicaId(0xABCD));
    let staff: StaffId = idc.mint();
    let instrument: InstrumentId = idc.mint();
    let region: RegionId = idc.mint();
    let instance: StaffInstanceId = idc.mint();
    let voice_id: VoiceId = idc.mint();

    let (events, tuplets) = make(&mut idc, voice_id);

    let mut arena = EventArena::new();
    let mut voice = Voice::user(voice_id);
    for e in events {
        voice.events.push(e.id());
        arena.insert(e).unwrap();
    }
    let mut si = StaffInstance::new(instance, staff);
    si.voices.push(voice);

    let region_obj = Region {
        id: region,
        time_model: RegionTimeModel::Metric(Default::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![si],
            ..Default::default()
        }),
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(1_000_000),
            },
        },
        staff_extent: StaffExtent {
            staves: vec![staff],
        },
        local_tempo_map: None,
    };

    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.staves = vec![Staff {
        id: staff,
        name: String::from("Test"),
        abbreviation: None,
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
    }];
    score.instruments = vec![Instrument {
        id: instrument,
        name: String::from("Test"),
    }];
    score.cross_cutting.tuplets = tuplets;
    score.events = arena;
    score.canvas = Canvas {
        regions: vec![region_obj],
    };
    score
}

// ---------------------------------------------------------------------------
// Spelling: line-of-fifths algorithm on unambiguous tonal cases
// ---------------------------------------------------------------------------

/// Spells a melodic sequence of absolute 12-TET semitones (integer/chromatic
/// input — no authored letter), returning the spellings in input order.
fn spell_line(semitones: &[i32]) -> Vec<PitchSpelling> {
    let replica = ReplicaId(7);
    let slots: Vec<Vec<SpellEntry>> = semitones
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            vec![SpellEntry {
                pid: PitchId::new(replica, i as u64 + 1),
                semitone: s,
                authored_cmn: None,
            }]
        })
        .collect();
    let mut out = BTreeMap::new();
    spell_run(&slots, &mut out);
    (0..semitones.len())
        .map(|i| out[&PitchId::new(replica, i as u64 + 1)].clone())
        .collect()
}

/// `(nominal, accidental-name)` of a spelling, for compact assertions.
fn describe(s: &PitchSpelling) -> (CmnNominal, String) {
    let nominal = match s.nominal {
        SpellingNominal::Cmn(n) => n,
        _ => panic!("expected CMN nominal"),
    };
    let acc = match s.accidentals.as_slice() {
        [] => String::new(),
        [one] => one.as_str().to_string(),
        many => many
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join("+"),
    };
    (nominal, acc)
}

fn spelled(semitones: &[i32]) -> Vec<(CmnNominal, String)> {
    spell_line(semitones).iter().map(describe).collect()
}

#[test]
fn c_major_scale_spells_all_naturals() {
    use CmnNominal::*;
    // C4 D4 E4 F4 G4 A4 B4 C5 as chromatic input.
    let got = spelled(&[48, 50, 52, 53, 55, 57, 59, 60]);
    let want = [C, D, E, F, G, A, B, C].map(|n| (n, String::new()));
    assert_eq!(got, want);
}

#[test]
fn d_major_scale_spells_f_sharp_and_c_sharp() {
    use CmnNominal::*;
    // D major: D E F# G A B C# D.
    let got = spelled(&[50, 52, 54, 55, 57, 59, 61, 62]);
    let want = [
        (D, ""),
        (E, ""),
        (F, "sharp"),
        (G, ""),
        (A, ""),
        (B, ""),
        (C, "sharp"),
        (D, ""),
    ]
    .map(|(n, a)| (n, a.to_string()));
    assert_eq!(got, want);
}

#[test]
fn b_flat_major_spells_flats() {
    use CmnNominal::*;
    // Bb major from Bb3: Bb C D Eb F G A.
    let got = spelled(&[46, 48, 50, 51, 53, 55, 57]);
    let want = [
        (B, "flat"),
        (C, ""),
        (D, ""),
        (E, "flat"),
        (F, ""),
        (G, ""),
        (A, ""),
    ]
    .map(|(n, a)| (n, a.to_string()));
    assert_eq!(got, want);
}

#[test]
fn flat_context_spells_accidental_as_flat() {
    use CmnNominal::*;
    // A clearly flat melody (F Bb Ab) spells the black key (pc 8) as Ab, not G#.
    let got = spelled(&[53, 58, 56]); // F4, Bb4, Ab4
    assert_eq!(got[2], (A, "flat".to_string()));
}

#[test]
fn sharp_context_spells_accidental_as_sharp() {
    use CmnNominal::*;
    // A clearly sharp melody (E B F#) spells pc 6 as F#, not Gb.
    let got = spelled(&[52, 59, 54]); // E4, B4, F#4
    assert_eq!(got[2], (F, "sharp".to_string()));
}

#[test]
fn enharmonic_naturals_wrap_octave_correctly() {
    // B# sounds as the C above it: B#3 (octave 3) sounds as semitone 48 (C4).
    let b_sharp = spelling_from_lof(12, 48);
    assert_eq!(describe(&b_sharp), (CmnNominal::B, "sharp".to_string()));
    assert_eq!(b_sharp.octave, 3);
    // Cb sounds as the B below it: Cb4 (octave 4) sounds as semitone 47 (B3).
    let c_flat = spelling_from_lof(-7, 47);
    assert_eq!(describe(&c_flat), (CmnNominal::C, "flat".to_string()));
    assert_eq!(c_flat.octave, 4);
}

#[test]
fn sharp_context_can_force_b_sharp() {
    // A run of unambiguous sharps drives the centre of gravity far enough sharp
    // that the natural pitch class 0 spells as B#, not C. Seed with naturals of a
    // sharp key, then climb into sharps and resolve onto pc 0.
    // G A B C# D# E# F## ... is awkward to seed cleanly; instead assert via the
    // centre directly: with a strongly sharp window, choose_lof(0) prefers B#.
    let lof = choose_lof(
        0,
        /*sum*/ 60,
        /*count*/ 6,
        std::cmp::Ordering::Greater,
    );
    assert_eq!(lof, 12, "a sharp centre of gravity spells pc 0 as B#");
}

#[test]
fn authored_cmn_letter_is_preserved_not_respelled() {
    // An authored C# in an otherwise flat context keeps its sharp spelling.
    let replica = ReplicaId(9);
    let slots = vec![
        vec![SpellEntry {
            pid: PitchId::new(replica, 1),
            semitone: 51, // Eb4/D#4, authored as Eb
            authored_cmn: Some((CmnNominal::E, -1, 4)),
        }],
        vec![SpellEntry {
            pid: PitchId::new(replica, 2),
            semitone: 49, // pc 1, authored as C#
            authored_cmn: Some((CmnNominal::C, 1, 4)),
        }],
    ];
    let mut out = BTreeMap::new();
    spell_run(&slots, &mut out);
    assert_eq!(
        describe(&out[&PitchId::new(replica, 1)]),
        (CmnNominal::E, "flat".to_string())
    );
    assert_eq!(
        describe(&out[&PitchId::new(replica, 2)]),
        (CmnNominal::C, "sharp".to_string())
    );
}

#[test]
fn chord_pitches_all_spelled() {
    // A C-E-G triad as one slot: all three spelled naturally.
    let replica = ReplicaId(11);
    let slots = vec![vec![
        SpellEntry {
            pid: PitchId::new(replica, 1),
            semitone: 48,
            authored_cmn: None,
        },
        SpellEntry {
            pid: PitchId::new(replica, 2),
            semitone: 52,
            authored_cmn: None,
        },
        SpellEntry {
            pid: PitchId::new(replica, 3),
            semitone: 55,
            authored_cmn: None,
        },
    ]];
    let mut out = BTreeMap::new();
    spell_run(&slots, &mut out);
    assert_eq!(out.len(), 3);
    assert_eq!(
        describe(&out[&PitchId::new(replica, 1)]),
        (CmnNominal::C, String::new())
    );
    assert_eq!(
        describe(&out[&PitchId::new(replica, 3)]),
        (CmnNominal::G, String::new())
    );
}

// ---------------------------------------------------------------------------
// Decomposition: grid-exact note-value sequences
// ---------------------------------------------------------------------------

const WHOLE: i64 = GRID_DEN;

fn lengths(start: i64, dur: i64) -> Vec<(NoteValue, u8)> {
    decompose_metric(start, dur, WHOLE).unwrap()
}

#[test]
fn quarter_on_a_beat_is_a_single_quarter() {
    assert_eq!(lengths(0, WHOLE / 4), vec![(NoteValue::Quarter, 0)]);
    assert_eq!(lengths(WHOLE / 4, WHOLE / 4), vec![(NoteValue::Quarter, 0)]);
}

#[test]
fn half_note_on_beat_one_is_a_single_half() {
    assert_eq!(lengths(0, WHOLE / 2), vec![(NoteValue::Half, 0)]);
}

#[test]
fn dotted_half_on_beat_one() {
    // 3/4 of a whole, on the downbeat.
    assert_eq!(lengths(0, WHOLE * 3 / 4), vec![(NoteValue::Half, 1)]);
}

#[test]
fn whole_note_fills_the_measure() {
    assert_eq!(lengths(0, WHOLE), vec![(NoteValue::Whole, 0)]);
}

#[test]
fn half_note_on_beat_two_ties_across_the_mid_measure() {
    // A half note starting on beat 2 of 4/4 crosses the strong mid-measure point
    // and must be written as two tied quarters.
    assert_eq!(
        lengths(WHOLE / 4, WHOLE / 2),
        vec![(NoteValue::Quarter, 0), (NoteValue::Quarter, 0)]
    );
}

#[test]
fn syncopation_eighth_then_quarter() {
    // [1/8, 1/2): an eighth tied to a quarter.
    assert_eq!(
        lengths(WHOLE / 8, WHOLE * 3 / 8),
        vec![(NoteValue::Eighth, 0), (NoteValue::Quarter, 0)]
    );
}

#[test]
fn classic_offbeat_eighth_quarter_eighth() {
    // [1/8, 5/8): eighth, quarter, eighth across beat 3.
    assert_eq!(
        lengths(WHOLE / 8, WHOLE / 2),
        vec![
            (NoteValue::Eighth, 0),
            (NoteValue::Quarter, 0),
            (NoteValue::Eighth, 0)
        ]
    );
}

#[test]
fn note_tied_across_a_barline() {
    // A half note starting on beat 4 of 4/4 crosses the barline into the next
    // measure: quarter (beat 4) tied to quarter (next downbeat).
    assert_eq!(
        lengths(WHOLE * 3 / 4, WHOLE / 2),
        vec![(NoteValue::Quarter, 0), (NoteValue::Quarter, 0)]
    );
}

#[test]
fn multi_measure_note_splits_at_every_barline() {
    // Two and a half whole notes from a downbeat: whole, whole, half.
    let got = lengths(0, WHOLE * 5 / 2);
    assert_eq!(
        got,
        vec![
            (NoteValue::Whole, 0),
            (NoteValue::Whole, 0),
            (NoteValue::Half, 0)
        ]
    );
}

#[test]
fn sub_sixtyfourth_duration_is_ungriddable_not_silently_dropped() {
    // 1/128 of a whole note is finer than the grid's smallest representable value
    // (a sixty-fourth): it is an exact grid multiple (so `to_grid_units` accepts
    // it) but not a note-value multiple. It must report as ungriddable — never
    // decompose to an empty/short list that violates invariant 15.
    let u = to_grid_units(&r(1, 128)).unwrap();
    assert_eq!(u, 32, "1/128 is below the sixty-fourth (64-unit) floor");
    assert_eq!(decompose_metric(0, u, WHOLE), None);
    // A clean head (a quarter) plus a sub-grid tail (1/128) must also be rejected,
    // not emitted as a quarter whose components fall short of the duration.
    assert_eq!(decompose_metric(0, WHOLE / 4 + u, WHOLE), None);
    // A whole-note (clean) decomposition is unaffected by the reconstruction check.
    assert_eq!(
        decompose_metric(0, WHOLE, WHOLE),
        Some(vec![(NoteValue::Whole, 0)])
    );
}

#[test]
fn grid_units_agree_with_canonical_note_value_math() {
    // `note_value_units`/`dotted_units` are the integer-grid mirror of the
    // canonical rational math in `NoteValue::whole_note_fraction` /
    // `NotatedComponent::notated_duration` (graph.rs). Pin the two domains so a
    // change to either is caught here rather than silently diverging — a divergence
    // would make the decomposition's components disagree with the event's own
    // `sounding_duration`, breaking invariant 15.
    for &v in &ALL_NOTE_VALUES {
        assert_eq!(
            note_value_units(v),
            to_grid_units(v.whole_note_fraction().rational())
                .expect("a base note value is a grid multiple"),
            "note_value_units disagrees with whole_note_fraction for {v:?}"
        );
        for dots in 0..=MAX_DOTS {
            let component = crate::graph::NotatedComponent {
                base_value: v,
                dots,
                tuplet: None,
                tied_to_next: false,
            };
            assert_eq!(
                dotted_units(v, dots),
                to_grid_units(component.notated_duration().rational())
                    .expect("a dotted value within MAX_DOTS is a grid multiple"),
                "dotted_units disagrees with notated_duration for {v:?} + {dots} dot(s)"
            );
        }
    }
}

#[test]
fn degenerate_zero_measure_is_ungriddable_not_a_panic() {
    // A zero-length measure is constructible: a `TimeSignature` with no beat groups
    // sums to a zero `measure_duration`, so `resolve_measure_units` can yield 0.
    // Such a measure has no barline grid, so a positive-duration event under it
    // must report ungriddable rather than dividing by zero in the offset reduction.
    assert_eq!(decompose_metric(0, WHOLE / 4, 0), None);
    assert_eq!(decompose_metric(WHOLE / 2, WHOLE, 0), None);
    // A zero-duration event is still vacuously representable regardless of measure.
    assert_eq!(decompose_metric(0, 0, 0), Some(Vec::new()));
}

#[test]
fn derive_counts_sub_sixtyfourth_event_as_ungriddable() {
    // The same case through the public entry point: the pitch still spells, but the
    // duration is honestly classified as ungriddable rather than silently dropped
    // or mis-bucketed as non-musical.
    let score = metric_score(|idc, voice| {
        let eid = idc.mint();
        let pid = idc.mint::<PitchId>();
        let ev = pitched(
            eid,
            voice,
            r(0, 1),
            r(1, 128), // finer than a sixty-fourth
            vec![IdentifiedPitch {
                id: pid,
                pitch: integer_pitch(48),
            }],
        );
        (vec![ev], vec![])
    });

    let ann = derive_annotations(&score, &PrePassProfile::default());
    assert_eq!(ann.spellings.len(), 1, "pitch still spells");
    assert_eq!(
        ann.decompositions.len(),
        0,
        "no broken (short) decomposition emitted"
    );
    assert_eq!(ann.taxonomy.decomposition_ungriddable, 1);
    assert_eq!(ann.taxonomy.decompositions_inferred, 0);
    assert_eq!(ann.taxonomy.decomposition_skipped_nonmusical, 0);
}

#[test]
fn off_grid_position_is_ungriddable_not_downbeat_aligned() {
    // A non-tuplet metric event whose *position* is off the grid (1/3 of a whole
    // note, e.g. sitting mid-tuplet) must report ungriddable rather than silently
    // decomposing as if it began on the barline. Its duration here is a clean
    // quarter, so only the position is at fault.
    let score = metric_score(|idc, voice| {
        let eid = idc.mint();
        let pid = idc.mint::<PitchId>();
        let ev = pitched(
            eid,
            voice,
            r(1, 3), // off-grid position (non-dyadic)
            r(1, 4),
            vec![IdentifiedPitch {
                id: pid,
                pitch: integer_pitch(48),
            }],
        );
        (vec![ev], vec![])
    });

    let ann = derive_annotations(&score, &PrePassProfile::default());
    assert_eq!(ann.spellings.len(), 1, "pitch still spells");
    assert_eq!(
        ann.decompositions.len(),
        0,
        "no downbeat-aligned guess emitted"
    );
    assert_eq!(ann.taxonomy.decomposition_ungriddable, 1);
    assert_eq!(ann.taxonomy.decompositions_inferred, 0);
}

#[test]
fn triplet_eighth_decomposes_in_the_notated_domain() {
    // A 3:2 eighth-note triplet member sounds 1/12; its notated value is an
    // eighth, tagged with the tuplet.
    let ratio = crate::graph::TupletRatio::new(3, 2).unwrap();
    let got = decompose_tuplet_member(&r(1, 12), ratio).unwrap();
    assert_eq!(got, vec![(NoteValue::Eighth, 0)]);
}

#[test]
fn quintuplet_sixteenth_decomposes_in_the_notated_domain() {
    // A 5:4 quintuplet sixteenth (5 in the time of 4) sounds 1/20; its notated
    // value is a sixteenth (1/20 * 5/4 = 1/16), tagged with the tuplet.
    let ratio = crate::graph::TupletRatio::new(5, 4).unwrap();
    let got = decompose_tuplet_member(&r(1, 20), ratio).unwrap();
    assert_eq!(got, vec![(NoteValue::Sixteenth, 0)]);
}

// ---------------------------------------------------------------------------
// Integration: derive_annotations, taxonomy, precedence, determinism
// ---------------------------------------------------------------------------

#[test]
fn derive_spells_and_decomposes_a_simple_metric_score() {
    let score = metric_score(|idc, voice| {
        let mut events = Vec::new();
        // Four quarter notes: C D E F (chromatic integer input).
        for (i, sem) in [48, 50, 52, 53].iter().enumerate() {
            let eid = idc.mint();
            let pid = idc.mint::<PitchId>();
            events.push(pitched(
                eid,
                voice,
                r(i as i64, 4),
                r(1, 4),
                vec![IdentifiedPitch {
                    id: pid,
                    pitch: integer_pitch(*sem),
                }],
            ));
        }
        (events, vec![])
    });

    let ann = derive_annotations(&score, &PrePassProfile::default());
    // Every pitch got a spelling; every event a single-quarter decomposition.
    assert_eq!(ann.spellings.len(), 4);
    assert_eq!(ann.decompositions.len(), 4);
    assert_eq!(ann.taxonomy.spellings_inferred, 4);
    assert_eq!(ann.taxonomy.decompositions_inferred, 4);
    assert_eq!(ann.taxonomy.pitched_events, 4);
    for dec in ann.decompositions.values() {
        assert_eq!(dec.components.len(), 1);
        assert_eq!(dec.components[0].base_value, NoteValue::Quarter);
        assert!(!dec.components[0].tied_to_next);
        assert_eq!(dec.source, DecompositionSource::Inferred);
    }
}

#[test]
fn taxonomy_classifies_each_kind_and_counts_ineligible_explicitly() {
    let score = metric_score(|idc, voice| {
        let p = {
            let eid = idc.mint();
            let pid = idc.mint::<PitchId>();
            pitched(
                eid,
                voice,
                r(0, 4),
                r(1, 4),
                vec![IdentifiedPitch {
                    id: pid,
                    pitch: integer_pitch(48),
                }],
            )
        };
        let rst = rest(idc.mint(), voice, r(1, 4), r(1, 4));
        let unp = unpitched(idc.mint(), voice, r(2, 4), r(1, 4));
        // A pitched event whose pitch space declares spelling unavailable.
        let ji = {
            let eid = idc.mint();
            let pid = idc.mint::<PitchId>();
            pitched(
                eid,
                voice,
                r(3, 4),
                r(1, 4),
                vec![IdentifiedPitch {
                    id: pid,
                    pitch: ji_pitch(),
                }],
            )
        };
        (vec![p, rst, unp, ji], vec![])
    });

    let ann = derive_annotations(&score, &PrePassProfile::default());
    let t = &ann.taxonomy;
    assert_eq!(t.pitched_events, 2);
    assert_eq!(t.rest_events, 1);
    assert_eq!(t.unpitched_events, 1);
    // One spelled pitch (the cmn-12 one); one unavailable (the JI one).
    assert_eq!(t.spellings_inferred, 1);
    assert_eq!(t.spelling_unavailable, 1);
    // Rest, unpitched, and both pitched events have determinate musical
    // durations in a metric region → all four decompose.
    assert_eq!(t.decompositions_inferred, 4);
    // No ineligible/deferred buckets here.
    assert_eq!(t.decomposition_deferred_nonmetric, 0);
    assert_eq!(t.decomposition_inapplicable, 0);
}

#[test]
fn nonmetric_region_defers_decomposition_but_still_spells() {
    // A proportional region: events are not decomposed, but their pitches are
    // still spelled (pitch identity is region-independent).
    let mut idc = IdentityContext::new(ReplicaId(0x5151));
    let staff: StaffId = idc.mint();
    let instrument: InstrumentId = idc.mint();
    let region: RegionId = idc.mint();
    let instance: StaffInstanceId = idc.mint();
    let voice_id: VoiceId = idc.mint();
    let eid = idc.mint();
    let pid = idc.mint::<PitchId>();
    let ev = pitched(
        eid,
        voice_id,
        r(0, 1),
        r(1, 4),
        vec![IdentifiedPitch {
            id: pid,
            pitch: integer_pitch(50),
        }],
    );
    let mut arena = EventArena::new();
    let mut voice = Voice::user(voice_id);
    voice.events.push(eid);
    arena.insert(ev).unwrap();
    let mut si = StaffInstance::new(instance, staff);
    si.voices.push(voice);
    let region_obj = Region {
        id: region,
        time_model: RegionTimeModel::Proportional(crate::graph::ProportionalTimeModel {
            duration: crate::time::WallClockDuration(1000),
        }),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![si],
            ..Default::default()
        }),
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(1000),
            },
        },
        staff_extent: StaffExtent {
            staves: vec![staff],
        },
        local_tempo_map: None,
    };
    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.staves = vec![Staff {
        id: staff,
        name: "T".into(),
        abbreviation: None,
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
    }];
    score.instruments = vec![Instrument {
        id: instrument,
        name: "T".into(),
    }];
    score.events = arena;
    score.canvas = Canvas {
        regions: vec![region_obj],
    };

    let ann = derive_annotations(&score, &PrePassProfile::default());
    assert_eq!(ann.spellings.len(), 1, "pitch is still spelled");
    assert_eq!(
        ann.decompositions.len(),
        0,
        "no decomposition off the metric grid"
    );
    assert_eq!(ann.taxonomy.decomposition_deferred_nonmetric, 1);
}

#[test]
fn authored_override_takes_precedence_over_inferred() {
    // resolve_spelling: an engraved-layer UserChosen explicit attachment for the
    // pitch outranks the inferred default.
    let mut idc = IdentityContext::new(ReplicaId(0x7777));
    let pid = idc.mint::<PitchId>();
    let mut score = Score::empty(idc.clone());
    let override_spelling = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::D),
        accidentals: vec![AccidentalId::new("flat")],
        octave: 4,
        render_hints: Default::default(),
    };
    score.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(override_spelling.clone()),
        source: SpellingSource::UserChosen,
        priority: 0,
        layer: None,
    });

    let inferred = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::C),
        accidentals: vec![AccidentalId::new("sharp")],
        octave: 4,
        render_hints: Default::default(),
    };
    let resolved = resolve_spelling(
        &score,
        pid,
        inferred.clone(),
        &SpellingPrecedence::default(),
    );
    assert_eq!(resolved.spelling, override_spelling);
    assert_eq!(
        resolved.provenance,
        SpellingProvenance::Authored(crate::pitch::SpellingSourceKind::UserChosen)
    );

    // A different pitch with no attachment keeps the inferred spelling.
    let other = idc.mint::<PitchId>();
    let resolved2 = resolve_spelling(
        &score,
        other,
        inferred.clone(),
        &SpellingPrecedence::default(),
    );
    assert_eq!(resolved2.spelling, inferred);
    assert_eq!(resolved2.provenance, SpellingProvenance::Inferred);
}

#[test]
fn extreme_priority_does_not_overflow_precedence() {
    // Two competing engraved `UserChosen` overrides for one pitch. `priority` is a
    // public `i32`, so `i32::MIN` is a legal value; the priority tiebreak must not
    // panic on it (negating `i32::MIN` overflows — the tiebreak uses `Reverse`
    // instead). The `i32::MIN` candidate is pushed first so it is the incumbent
    // `cur` when the second is compared against it. The higher priority wins.
    let mut idc = IdentityContext::new(ReplicaId(0x7777));
    let pid = idc.mint::<PitchId>();
    let mut score = Score::empty(idc);

    let losing = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::D),
        accidentals: vec![AccidentalId::new("flat")],
        octave: 4,
        render_hints: Default::default(),
    };
    let winning = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::C),
        accidentals: vec![AccidentalId::new("sharp")],
        octave: 4,
        render_hints: Default::default(),
    };
    score.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(losing),
        source: SpellingSource::UserChosen,
        priority: i32::MIN,
        layer: None,
    });
    score.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(winning.clone()),
        source: SpellingSource::UserChosen,
        priority: 5,
        layer: None,
    });

    let inferred = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::C),
        accidentals: Vec::new(),
        octave: 4,
        render_hints: Default::default(),
    };
    let resolved = resolve_spelling(&score, pid, inferred, &SpellingPrecedence::default());
    assert_eq!(
        resolved.spelling, winning,
        "higher priority wins, no overflow"
    );
}

#[test]
fn authored_triple_sharp_is_preserved_not_clamped() {
    // An authored triple-sharp (alteration = 3) takes the authored-CMN path, which
    // preserves the letter and renders the alteration via `accidental_ids`. The
    // glyph stack must reconstruct +3 (double-sharp + sharp), not clamp to a single
    // double accidental — clamping would silently spell the pitch a semitone flat,
    // violating "authored CMN positions are preserved, not re-spelled".
    let p = Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: 3,
                octave: 4,
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    };
    let s = simplest_spelling(&p).expect("a cmn-12 pitch is spellable");
    assert_eq!(s.nominal, SpellingNominal::Cmn(CmnNominal::C));
    assert_eq!(s.octave, 4);
    assert_eq!(
        s.accidentals,
        vec![
            AccidentalId::new("double-sharp"),
            AccidentalId::new("sharp")
        ],
        "triple-sharp must be a double-sharp + sharp stack, not a clamped double"
    );
}

#[test]
fn derivation_is_deterministic_across_runs() {
    let build = || {
        metric_score(|idc, voice| {
            let mut events = Vec::new();
            for (i, sem) in [50, 54, 49, 51, 58].iter().enumerate() {
                let eid = idc.mint();
                let pid = idc.mint::<PitchId>();
                events.push(pitched(
                    eid,
                    voice,
                    r(i as i64, 8),
                    r(1, 8),
                    vec![IdentifiedPitch {
                        id: pid,
                        pitch: integer_pitch(*sem),
                    }],
                ));
            }
            (events, vec![])
        })
    };
    // Same identity seed → identical scores → identical annotations, twice.
    let a = derive_annotations(&build(), &PrePassProfile::default());
    let b = derive_annotations(&build(), &PrePassProfile::default());
    assert_eq!(a, b);
}

#[test]
fn tuplet_member_event_decomposes_with_tuplet_membership() {
    let score = metric_score(|idc, voice| {
        let tuplet_id: TupletId = idc.mint();
        let mut events = Vec::new();
        let mut members = Vec::new();
        // Three triplet eighths (sounding 1/12 each) at 0, 1/12, 2/12.
        for i in 0..3 {
            let eid = idc.mint();
            let pid = idc.mint::<PitchId>();
            members.push(eid);
            events.push(pitched(
                eid,
                voice,
                r(i, 12),
                r(1, 12),
                vec![IdentifiedPitch {
                    id: pid,
                    pitch: integer_pitch(48 + i as i32),
                }],
            ));
        }
        let tuplet = Tuplet {
            id: tuplet_id,
            ratio: crate::graph::TupletRatio::new(3, 2).unwrap(),
            members,
            parent: None,
            required_total: MusicalDuration(r(1, 4)),
        };
        (events, vec![tuplet])
    });

    let ann = derive_annotations(&score, &PrePassProfile::default());
    assert_eq!(ann.decompositions.len(), 3);
    for dec in ann.decompositions.values() {
        assert_eq!(dec.components.len(), 1);
        assert_eq!(dec.components[0].base_value, NoteValue::Eighth);
        assert!(dec.components[0].tuplet.is_some(), "tagged with the tuplet");
    }
    assert_eq!(ann.taxonomy.decomposition_ungriddable, 0);
}

#[test]
fn decomposition_components_sum_to_event_duration() {
    // The sounding durations of an event's decomposition components must sum to
    // the event's sounding duration (Chapter 3 invariant 15).
    let cases = [
        (WHOLE / 4, WHOLE / 2),     // half on beat 2
        (WHOLE / 8, WHOLE / 2),     // off-beat syncopation
        (WHOLE * 3 / 4, WHOLE / 2), // across a barline
    ];
    for (start, dur) in cases {
        let comps = lengths(start, dur);
        let total: i64 = comps.iter().map(|&(v, d)| dotted_units(v, d)).sum();
        assert_eq!(total, dur, "components sum to the event duration");
    }
}

// ---------------------------------------------------------------------------
// Decomposition precedence: authored attachments outrank the derived default
// ---------------------------------------------------------------------------

/// A two-event metric score — two half notes on beats 1 and 3 — plus the two
/// event ids. Each half note sits on a boundary of its own strength, so the
/// pre-pass infers a single (undotted) half for both.
fn two_half_note_score() -> (Score, crate::ids::EventId, crate::ids::EventId) {
    let mut ids = Vec::new();
    let score = metric_score(|idc, voice| {
        let mut events = Vec::new();
        for i in 0..2i64 {
            let eid = idc.mint();
            let pid = idc.mint::<PitchId>();
            ids.push(eid);
            events.push(pitched(
                eid,
                voice,
                r(i, 2),
                r(1, 2),
                vec![IdentifiedPitch {
                    id: pid,
                    pitch: integer_pitch(48 + i as i32),
                }],
            ));
        }
        (events, vec![])
    });
    (score, ids[0], ids[1])
}

/// An authored two-tied-quarters decomposition for `target` — a legitimate
/// alternative to the inferred single half note (components sum to `1/2`).
fn tied_quarters(
    target: crate::ids::EventId,
    source: DecompositionSource,
) -> DecompositionAttachment {
    let quarter = |tied_to_next| NotatedComponent {
        base_value: NoteValue::Quarter,
        dots: 0,
        tuplet: None,
        tied_to_next,
    };
    DecompositionAttachment {
        target,
        components: vec![quarter(true), quarter(false)],
        source,
    }
}

/// Another authored alternative summing to `1/2`: dotted quarter tied to an
/// eighth. Distinct from [`tied_quarters`] so rank ties are observable.
fn dotted_quarter_eighth(
    target: crate::ids::EventId,
    source: DecompositionSource,
) -> DecompositionAttachment {
    DecompositionAttachment {
        target,
        components: vec![
            NotatedComponent {
                base_value: NoteValue::Quarter,
                dots: 1,
                tuplet: None,
                tied_to_next: true,
            },
            NotatedComponent {
                base_value: NoteValue::Eighth,
                dots: 0,
                tuplet: None,
                tied_to_next: false,
            },
        ],
        source,
    }
}

#[test]
fn authored_decomposition_outranks_the_inferred_default() {
    // Chapter 3: the pre-pass produces inferred decompositions only "for events
    // that lack a higher-precedence attachment" — an authored UserChosen
    // attachment must not be shadowed by the derived one.
    let (mut score, e1, e2) = two_half_note_score();
    let authored = tied_quarters(e1, DecompositionSource::UserChosen);
    score.decomposition_attachments.push(authored.clone());

    let ann = derive_annotations(&score, &PrePassProfile::default());
    // The authored attachment is the effective decomposition for its event; no
    // derived (single-half-note) decomposition shadows it.
    assert_eq!(ann.decompositions[&e1], authored, "authored override wins");
    // The un-overridden event keeps the algorithm's inferred half note.
    let other = &ann.decompositions[&e2];
    assert_eq!(other.source, DecompositionSource::Inferred);
    assert_eq!(other.components.len(), 1);
    assert_eq!(other.components[0].base_value, NoteValue::Half);
    // Taxonomy counts the two outcomes distinctly, and together they cover the
    // effective map (the accounting the H harness checks).
    assert_eq!(ann.taxonomy.decompositions_authored, 1);
    assert_eq!(ann.taxonomy.decompositions_inferred, 1);
    assert_eq!(
        ann.decompositions.len(),
        ann.taxonomy.decompositions_inferred + ann.taxonomy.decompositions_authored
    );
}

#[test]
fn inferred_source_attachment_does_not_outrank_the_prepass() {
    // An attachment whose source is `Inferred` does not outrank the pre-pass's
    // own output (the rank gate, mirroring `resolve_spelling`): the derived
    // half note stands.
    let (mut score, e1, _e2) = two_half_note_score();
    score
        .decomposition_attachments
        .push(tied_quarters(e1, DecompositionSource::Inferred));

    let ann = derive_annotations(&score, &PrePassProfile::default());
    let dec = &ann.decompositions[&e1];
    assert_eq!(dec.components.len(), 1, "the pre-pass's half note stands");
    assert_eq!(dec.components[0].base_value, NoteValue::Half);
    assert_eq!(dec.source, DecompositionSource::Inferred);
    assert_eq!(ann.taxonomy.decompositions_authored, 0);
    assert_eq!(ann.taxonomy.decompositions_inferred, 2);
}

#[test]
fn decomposition_precedence_ranks_sources_then_canonical_order() {
    // UserChosen outranks Imported regardless of attachment order (the spec's
    // default precedence over the decomposition sources)...
    let (mut score, e1, _) = two_half_note_score();
    let imported = dotted_quarter_eighth(
        e1,
        DecompositionSource::Imported {
            format: crate::pitch::ForeignFormatId::new("musicxml"),
        },
    );
    let user = tied_quarters(e1, DecompositionSource::UserChosen);
    score.decomposition_attachments.push(imported); // listed first — must lose
    score.decomposition_attachments.push(user.clone());
    let ann = derive_annotations(&score, &PrePassProfile::default());
    assert_eq!(
        ann.decompositions[&e1], user,
        "UserChosen outranks Imported regardless of attachment order"
    );

    // ...and a full rank tie keeps the first attachment in the score's
    // canonical `decomposition_attachments` order (deterministic across
    // replicas; the attachment carries no `priority` axis).
    let (mut score2, f1, _) = two_half_note_score();
    let first = tied_quarters(f1, DecompositionSource::UserChosen);
    let second = dotted_quarter_eighth(f1, DecompositionSource::UserChosen);
    score2.decomposition_attachments.push(first.clone());
    score2.decomposition_attachments.push(second);
    let ann2 = derive_annotations(&score2, &PrePassProfile::default());
    assert_eq!(
        ann2.decompositions[&f1], first,
        "rank ties keep the earlier attachment in canonical order"
    );
}

#[test]
fn derivation_with_authored_decomposition_is_deterministic() {
    // The authored override is reflected deterministically: the same overridden
    // score derives byte-identically twice, and the fingerprint distinguishes
    // the overridden derivation from the un-overridden one.
    let build = || {
        let (mut score, e1, _) = two_half_note_score();
        score
            .decomposition_attachments
            .push(tied_quarters(e1, DecompositionSource::UserChosen));
        score
    };
    let a = derive_annotations(&build(), &PrePassProfile::default());
    let b = derive_annotations(&build(), &PrePassProfile::default());
    assert_eq!(a, b);
    assert_eq!(a.canonical_fingerprint(), b.canonical_fingerprint());

    let (plain, _, _) = two_half_note_score();
    let c = derive_annotations(&plain, &PrePassProfile::default());
    assert_ne!(
        a.canonical_fingerprint(),
        c.canonical_fingerprint(),
        "the override is visible in the derivation fingerprint"
    );
}

#[test]
fn authored_attachment_on_an_ungriddable_event_does_not_surface() {
    // The resolution step mirrors the spelling side: it layers authored
    // overrides above the pre-pass's *inferred* output. An event the pre-pass
    // cannot grid emits nothing to override, so an authored attachment for it
    // stays in canonical score state without surfacing as a derived annotation,
    // and the event stays honestly counted as ungriddable. (Whether authored
    // decompositions should surface for events the algorithm cannot infer for
    // is a Pass-12 question — see DECISIONS.md.)
    let mut ids = Vec::new();
    let mut score = metric_score(|idc, voice| {
        let eid = idc.mint();
        let pid = idc.mint::<PitchId>();
        ids.push(eid);
        let ev = pitched(
            eid,
            voice,
            r(0, 1),
            r(1, 128), // finer than a sixty-fourth: ungriddable
            vec![IdentifiedPitch {
                id: pid,
                pitch: integer_pitch(48),
            }],
        );
        (vec![ev], vec![])
    });
    score
        .decomposition_attachments
        .push(tied_quarters(ids[0], DecompositionSource::UserChosen));

    let ann = derive_annotations(&score, &PrePassProfile::default());
    assert!(ann.decompositions.is_empty());
    assert_eq!(ann.taxonomy.decomposition_ungriddable, 1);
    assert_eq!(ann.taxonomy.decompositions_authored, 0);
    assert_eq!(ann.taxonomy.decompositions_inferred, 0);
}

#[test]
fn unknown_algorithm_ids_are_not_honored() {
    // A profile requesting an algorithm the pre-pass does not implement must not
    // receive default output labeled as that algorithm; the unhonored pre-pass
    // derives nothing while the requested id stays in the result profile.
    let score = metric_score(|idc, voice| {
        let eid = idc.mint();
        let pid = idc.mint::<PitchId>();
        let ev = pitched(
            eid,
            voice,
            r(0, 1),
            r(1, 4),
            vec![IdentifiedPitch {
                id: pid,
                pitch: integer_pitch(48),
            }],
        );
        (vec![ev], vec![])
    });

    let default = derive_annotations(&score, &PrePassProfile::default());
    assert!(!default.spellings.is_empty(), "the default profile spells");
    assert!(
        !default.decompositions.is_empty(),
        "the default profile decomposes"
    );

    // Unknown spelling algorithm: no spellings; decomposition (still default) runs.
    let unknown_spelling = PrePassProfile {
        spelling_algorithm: SpellingAlgorithmId::new("future-v2"),
        decomposition_algorithm: DecompositionAlgorithmId::default_id(),
    };
    let a = derive_annotations(&score, &unknown_spelling);
    assert!(
        a.spellings.is_empty(),
        "an unknown spelling algorithm is not honored (no default output)"
    );
    assert_eq!(
        a.decompositions.len(),
        default.decompositions.len(),
        "the supported decomposition pre-pass still runs"
    );
    assert_eq!(
        a.profile.spelling_algorithm,
        SpellingAlgorithmId::new("future-v2"),
        "the requested id is preserved in the result profile"
    );
    // The canonical fingerprint distinguishes differing derivations (it is not a
    // degenerate constant the determinism gate would pass vacuously).
    assert_ne!(
        a.canonical_fingerprint(),
        default.canonical_fingerprint(),
        "the canonical fingerprint discriminates differing annotations"
    );

    // Unknown decomposition algorithm: no decompositions; spelling runs.
    let unknown_decomp = PrePassProfile {
        spelling_algorithm: SpellingAlgorithmId::default_id(),
        decomposition_algorithm: DecompositionAlgorithmId::new("future-v2"),
    };
    let b = derive_annotations(&score, &unknown_decomp);
    assert!(
        b.decompositions.is_empty(),
        "an unknown decomposition algorithm is not honored"
    );
    assert_eq!(
        b.spellings.len(),
        default.spellings.len(),
        "the supported spelling pre-pass still runs"
    );
}
