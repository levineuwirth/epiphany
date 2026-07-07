//! Integration tests driving `epiphany-core` only through its public surface,
//! the way `epiphany-ops` (Agent C) and `epiphany-layout-ir` (Agent E) will
//! consume it. These complement the per-module unit tests by proving the
//! re-exported API is sufficient to build a score graph, check the Chapter 5
//! invariants, and round-trip the canonical primitives.

use epiphany_core::{
    check_invariants, derive_promoted_voice_id, generators, AcousticPitch, AcousticRealization,
    Canvas, CmnNominal, Event, EventArena, EventDuration, EventPosition, GraphInvariant,
    IdentifiedPitch, IdentityContext, InstrumentId, MusicalDuration, MusicalPosition, OperationId,
    Pitch, PitchId, PitchSpaceId, PitchSpacePosition, PitchedEvent, RationalTime, Region,
    RegionContent, RegionTimeModel, ReplicaId, ScalePosition, Score, StaffBasedContent,
    StaffExtent, StaffInstance, StaffLineConfiguration, StemConfiguration, TimeAnchor, TimeExtent,
    TuningReference, Voice, WallClockTime,
};
use epiphany_core::{Staff, StaffId, StaffInstanceId, VoiceId};

// epiphany-determinism's canonical-encoding traits are part of the contract
// downstream consumers use against core's identifier/time primitives.
use epiphany_determinism::{CanonicalDecode, CanonicalEncode};

/// Builds a one-measure, single-staff, single-voice metric score with four
/// quarter notes, entirely through the public API.
fn hand_built_score() -> Score {
    let mut idc = IdentityContext::new(ReplicaId(0xC0FFEE));

    let staff_id: StaffId = idc.mint();
    let instrument: InstrumentId = idc.mint();
    let instance_id: StaffInstanceId = idc.mint();
    let voice_id: VoiceId = idc.mint();

    let mut arena = EventArena::new();
    let mut voice = Voice::user(voice_id);
    for beat in 0..4i64 {
        let eid = idc.mint();
        let pid: PitchId = idc.mint();
        let event = Event::Pitched(PitchedEvent {
            id: eid,
            voice: voice_id,
            position: EventPosition::Musical(MusicalPosition(RationalTime::new(beat, 4).unwrap())),
            duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
            pitches: vec![IdentifiedPitch {
                id: pid,
                pitch: Pitch {
                    scale_position: ScalePosition {
                        space: PitchSpaceId::new("cmn-12"),
                        position: PitchSpacePosition::Cmn {
                            nominal: CmnNominal::C,
                            alteration: 0,
                            octave: 4,
                        },
                    },
                    acoustic: AcousticPitch {
                        tuning: TuningReference::Inherit,
                        realization: AcousticRealization::Implicit,
                    },
                },
            }],
            articulations: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: StemConfiguration,
            grace: None,
        });
        arena.insert(event).unwrap();
        voice.events.push(eid);
    }

    let mut instance = StaffInstance::new(instance_id, staff_id);
    instance.voices.push(voice);

    let region = Region {
        id: idc.mint(),
        time_model: RegionTimeModel::Metric(Default::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![instance],
            ..Default::default()
        }),
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(1_000),
            },
        },
        staff_extent: StaffExtent {
            staves: vec![staff_id],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.instruments = vec![epiphany_core::Instrument::new(instrument, "Flute")];
    score.staves = vec![Staff {
        id: staff_id,
        name: "Flute 1".into(),
        abbreviation: Some("Fl. 1".into()),
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
        default_clef: epiphany_core::Clef::treble(),
    }];
    score.events = arena;
    score.canvas = Canvas {
        regions: vec![region],
        ..Default::default()
    };
    score
}

#[test]
fn hand_built_score_is_well_formed() {
    let score = hand_built_score();
    let violations = check_invariants(&score);
    assert!(
        violations.is_empty(),
        "expected a clean graph, got {violations:?}"
    );
    assert_eq!(score.events.len(), 4);
    assert_eq!(score.live_pitch_ids().len(), 4);
}

#[test]
fn deleting_an_event_from_its_voice_list_is_caught() {
    let mut score = hand_built_score();
    // Drop the first event from its voice's listing but leave it in the arena.
    let first = score.events.ids_canonical()[0];
    if let RegionContent::StaffBased(c) = &mut score.canvas.regions[0].content {
        c.staff_instances[0].voices[0]
            .events
            .retain(|e| *e != first);
    }
    let v = check_invariants(&score);
    assert!(v
        .iter()
        .any(|x| x.invariant == GraphInvariant::EventVoiceBacklink));
}

#[test]
fn full_invariant_sweep_via_public_api() {
    // Every invariant: a clean valid graph passes, and the matching negative
    // generator's graph reports that invariant. Drives the public re-exports.
    for inv in GraphInvariant::all() {
        let bad = generators::violating_score(inv, 0xBEEF);
        assert!(
            check_invariants(&bad).iter().any(|v| v.invariant == inv),
            "{inv:?} not reported on its negative graph"
        );
        let shrunk = generators::shrink(&bad, inv);
        assert!(generators::element_count(&shrunk) <= generators::element_count(&bad));
    }
}

#[test]
fn canonical_primitives_round_trip_through_public_api() {
    // Identifier and time primitives encode/decode byte-stably (Appendix D).
    let id = epiphany_core::EventId::new(ReplicaId(7), 42);
    assert_eq!(
        epiphany_core::EventId::decode_canonical(&id.to_canonical_bytes()).unwrap(),
        id
    );
    let pos = MusicalPosition(RationalTime::new(3, 8).unwrap());
    assert_eq!(
        MusicalPosition::decode_canonical(&pos.to_canonical_bytes()).unwrap(),
        pos
    );
}

#[test]
fn promoted_voice_derivation_is_replica_independent_but_input_dependent() {
    // Two replicas independently derive the same promoted voice from the same
    // canonical inputs (Chapter 5 §"System-Promoted Voices").
    let si = StaffInstanceId::new(ReplicaId(1), 5);
    let ov = VoiceId::new(ReplicaId(1), 6);
    let win = OperationId::new(ReplicaId(2), 7);
    let lose = OperationId::new(ReplicaId(3), 8);
    let a = derive_promoted_voice_id(si, ov, win, lose);
    let b = derive_promoted_voice_id(si, ov, win, lose);
    assert_eq!(a, b);
    assert_eq!(a.replica(), ReplicaId::SYSTEM_DERIVED);
    assert_ne!(a, derive_promoted_voice_id(si, ov, lose, win));
}
