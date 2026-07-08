//! Hand-built score fixtures the testkit needs but Agent B's generators do not
//! provide in the exact shape an acceptance criterion names.
//!
//! In particular the layout hand-off case (QUICKSTART, Agent E) is *"a 10-measure
//! single-staff score"*, which [`ten_measure_single_staff`] builds — Agent B's
//! [`valid_score`](epiphany_core::generators::valid_score) is 1–2 staves with no
//! measures, and [`valid_score_rich`](epiphany_core::generators::valid_score_rich)
//! is three staves. The fixture is invariant-clean and carries measures plus a
//! tie, spanner, marker, and chord symbol so the layout projection exercises the
//! cross-cutting objects.

use epiphany_core::{
    AcousticPitch, AcousticRealization, AnchorOffset, Canvas, ChordSymbol, CmnNominal,
    CrossCuttingRegistry, CurvatureOverride, CurveDirection, Event, EventArena, EventDuration,
    EventPosition, IdentifiedPitch, IdentityContext, Marker, Measure, MeasurePosition,
    MetricTimeModel, MusicalDuration, MusicalPosition, Pitch, PitchSpaceId, PitchSpacePosition,
    RationalTime, RegionContent, RegionEdge, RegionTimeModel, RepeatKind, RepeatStructure,
    ScalePosition, Score, Slur, SlurKind, SpaceUnit, SpanStyle, Spanner, Staff, StaffBasedContent,
    StaffExtent, StaffInstance, StaffLineConfiguration, StemConfiguration, Tie, TieClass,
    TimeAnchor, TimeExtent, TuningReference, Voice, Volta, WallClockTime,
};
use epiphany_core::{
    ChordSymbolId, EventId, InstrumentId, MarkerId, MeasureId, PitchId, RegionId,
    RepeatStructureId, ReplicaId, SlurId, SpannerId, StaffId, StaffInstanceId, TieId, VoiceId,
};
use epiphany_determinism::CanonicalF64;

use epiphany_determinism::fuzz::SplitMix64;

/// A C4 pitched quarter-note at musical position `index/4`. All notes are C4 so
/// the tie between adjacent events is enharmonically valid.
fn quarter(eid: EventId, voice: VoiceId, pid: PitchId, index: i64) -> Event {
    Event::Pitched(epiphany_core::PitchedEvent {
        id: eid,
        voice,
        position: EventPosition::Musical(MusicalPosition(RationalTime::new(index, 4).unwrap())),
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
    })
}

/// A 10-measure, single-staff, single-voice metric score with 40 quarter notes
/// (four per measure), plus a tie, a spanner, a marker, and a chord symbol. The
/// QUICKSTART layout hand-off case. Invariant-clean (the returned graph passes
/// [`check_invariants`](epiphany_core::check_invariants)).
pub fn ten_measure_single_staff(seed: u64) -> Score {
    let mut rng = SplitMix64::new(seed ^ 0x0010_3EA5_5AFF);
    let replica =
        ReplicaId::from_entropy(rng.next_u64().to_le_bytes()).unwrap_or(ReplicaId(0x10AF));
    let mut idc = IdentityContext::new(replica);

    let staff_id: StaffId = idc.mint();
    let instrument: InstrumentId = idc.mint();
    let region_id: RegionId = idc.mint();
    let instance_id: StaffInstanceId = idc.mint();
    let voice_id: VoiceId = idc.mint();

    const MEASURES: i64 = 10;
    const BEATS_PER_MEASURE: i64 = 4;

    let mut arena = EventArena::new();
    let mut voice = Voice::user(voice_id);
    let mut events: Vec<EventId> = Vec::new();
    for index in 0..(MEASURES * BEATS_PER_MEASURE) {
        let eid: EventId = idc.mint();
        let pid: PitchId = idc.mint();
        arena.insert(quarter(eid, voice_id, pid, index)).unwrap();
        voice.events.push(eid);
        events.push(eid);
    }

    let mut instance = StaffInstance::new(instance_id, staff_id);
    instance.voices.push(voice);
    for m in 0..MEASURES {
        let mid: MeasureId = idc.mint();
        instance.measures.push(Measure {
            id: mid,
            // Region-anchored at a whole-note musical offset (one measure of 4/4).
            start: TimeAnchor::Region {
                id: region_id,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::new(m, 1).unwrap())),
            },
            time_signature: None,
            explicit_number: Some((m + 1) as u32),
            number_visibility: Default::default(),
        });
    }

    // Cross-cutting: a tie between the first two (C4) events, a spanner across
    // the staff, a marker, and a chord symbol — all region-anchored validly.
    let mut cross_cutting = CrossCuttingRegistry::default();
    cross_cutting.ties.push(Tie {
        id: idc.mint::<TieId>(),
        start_event: events[0],
        end_event: events[1],
        pitch_pairing: None,
        class: TieClass::Standard,
        style: Default::default(),
    });
    cross_cutting.spanners.push(Spanner {
        id: idc.mint::<SpannerId>(),
        start: TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration::zero()),
        },
        end: TimeAnchor::WallClock {
            time: WallClockTime(10),
        },
        staves: vec![staff_id],
        kind: Default::default(),
        style: Default::default(),
    });
    cross_cutting.markers.push(Marker {
        id: idc.mint::<MarkerId>(),
        anchor: TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        },
    });
    cross_cutting.chord_symbols.push(ChordSymbol {
        id: idc.mint::<ChordSymbolId>(),
        anchor: TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        },
    });
    // (A slur id is minted lazily elsewhere; keep the registry otherwise empty.)
    let _ = SlurId::new(replica, 0);

    let region = epiphany_core::Region {
        id: region_id,
        time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![instance],
            ..Default::default()
        }),
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(10_000_000),
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
    score.instruments = vec![epiphany_core::Instrument::new(
        instrument,
        String::from("Flute"),
    )];
    score.staves = vec![Staff {
        id: staff_id,
        name: String::from("Flute"),
        abbreviation: Some(String::from("Fl.")),
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
        default_clef: epiphany_core::Clef::treble(),
    }];
    score.events = arena;
    score.cross_cutting = cross_cutting;
    score.canvas = Canvas {
        regions: vec![region],
        ..Default::default()
    };
    score
}

/// [`ten_measure_single_staff`] plus three repeat structures — the
/// repeat-rendering acceptance fixture (schema-major-2 E1). Measure-anchored so
/// every boundary coincides with a barline column, it exercises each visual
/// form at once: a simple repeat over measures 2–4 whose end meets the volta
/// repeat's start (the combined `repeatRightLeft` sign), a `Volta`-kind repeat
/// over measures 5–7 with a first ending over measure 7 and a "2 3" ending
/// over measure 8 (brackets + ending numerals), and a simple repeat of the
/// last measure closing on the region end (the dot pair beside the final
/// barline). Invariant-clean.
pub fn ten_measure_with_repeats(seed: u64) -> Score {
    let mut score = ten_measure_single_staff(seed);
    let measures: Vec<MeasureId> = score.canvas.regions[0].staff_instances()[0]
        .measures
        .iter()
        .map(|measure| measure.id)
        .collect();
    let at_start = |index: usize| TimeAnchor::Measure {
        id: measures[index],
        position: MeasurePosition::Start,
        offset: AnchorOffset::Zero,
    };

    score.cross_cutting.repeats.push(RepeatStructure {
        id: score.identity.mint::<RepeatStructureId>(),
        start: at_start(1),
        end: at_start(4),
        kind: RepeatKind::SimpleRepeat { count: 2 },
        voltas: Vec::new(),
    });
    score.cross_cutting.repeats.push(RepeatStructure {
        id: score.identity.mint::<RepeatStructureId>(),
        start: at_start(4),
        end: at_start(7),
        kind: RepeatKind::Volta,
        voltas: vec![
            Volta {
                endings: vec![1],
                start: at_start(6),
                end: at_start(7),
            },
            Volta {
                endings: vec![2, 3],
                start: at_start(7),
                end: at_start(8),
            },
        ],
    });
    score.cross_cutting.repeats.push(RepeatStructure {
        id: score.identity.mint::<RepeatStructureId>(),
        start: at_start(9),
        end: TimeAnchor::Measure {
            id: measures[9],
            position: MeasurePosition::End,
            offset: AnchorOffset::Zero,
        },
        kind: RepeatKind::SimpleRepeat { count: 2 },
        voltas: Vec::new(),
    });
    score
}

/// [`ten_measure_single_staff`] plus three slurs — the slur-rendering
/// acceptance fixture (schema-major-2 E2). All endpoints are events on the one
/// staff, so each resolves to a note column: a default (Legato, auto direction
/// = above) slur over the first four notes, an authored below slur with an
/// explicit apex height over the middle four, and an editorial slur over a
/// later pair. Invariant-clean.
pub fn ten_measure_with_slurs(seed: u64) -> Score {
    let mut score = ten_measure_single_staff(seed);
    let events: Vec<EventId> = score.canvas.regions[0].staff_instances()[0].voices[0]
        .events
        .clone();

    score.cross_cutting.slurs.push(Slur {
        id: score.identity.mint::<SlurId>(),
        start_event: events[0],
        end_event: events[3],
        kind: SlurKind::Legato,
        curvature_override: None,
        style: SpanStyle::default(),
    });
    score.cross_cutting.slurs.push(Slur {
        id: score.identity.mint::<SlurId>(),
        start_event: events[5],
        end_event: events[8],
        kind: SlurKind::Phrase,
        curvature_override: Some(CurvatureOverride {
            direction: Some(CurveDirection::Below),
            height: Some(SpaceUnit(CanonicalF64::new(2.0).expect("2.0 is finite"))),
        }),
        style: SpanStyle::default(),
    });
    score.cross_cutting.slurs.push(Slur {
        id: score.identity.mint::<SlurId>(),
        start_event: events[10],
        end_event: events[12],
        kind: SlurKind::Editorial,
        curvature_override: None,
        style: SpanStyle::default(),
    });
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::check_invariants;

    #[test]
    fn fixture_is_invariant_clean_and_shaped() {
        let s = ten_measure_single_staff(1);
        let v = check_invariants(&s);
        assert!(v.is_empty(), "ten-measure fixture has violations: {v:?}");
        assert_eq!(s.staves.len(), 1, "single staff");
        assert_eq!(s.canvas.regions[0].staff_instances()[0].measures.len(), 10);
        assert_eq!(s.events.len(), 40);
        assert_eq!(s.cross_cutting.ties.len(), 1);
        assert_eq!(s.cross_cutting.spanners.len(), 1);
        assert_eq!(s.cross_cutting.markers.len(), 1);
        assert_eq!(s.cross_cutting.chord_symbols.len(), 1);
    }

    #[test]
    fn repeat_fixture_is_invariant_clean_and_carries_three_repeats() {
        let s = ten_measure_with_repeats(1);
        let v = check_invariants(&s);
        assert!(v.is_empty(), "repeat fixture has violations: {v:?}");
        assert_eq!(s.cross_cutting.repeats.len(), 3);
        // One volta structure with two brackets; the rest simple repeats.
        assert_eq!(
            s.cross_cutting
                .repeats
                .iter()
                .map(|rp| rp.voltas.len())
                .sum::<usize>(),
            2
        );
    }

    #[test]
    fn slur_fixture_is_invariant_clean_and_carries_three_slurs() {
        let s = ten_measure_with_slurs(1);
        let v = check_invariants(&s);
        assert!(v.is_empty(), "slur fixture has violations: {v:?}");
        assert_eq!(s.cross_cutting.slurs.len(), 3);
        // One slur authors a below-direction curvature override; the rest use
        // the engraver's default (above).
        assert_eq!(
            s.cross_cutting
                .slurs
                .iter()
                .filter(|slur| slur.curvature_override.is_some())
                .count(),
            1
        );
    }

    /// A measure that references a time signature lists it (and its start
    /// anchor's target) among its invalidation dependencies, so a time-signature
    /// display change with an unchanged id invalidates the measure and its
    /// synthesized time-signature glyphs.
    #[test]
    fn a_measure_depends_on_its_time_signature_and_start_anchor() {
        use epiphany_core::{TimeSignatureId, TypedObjectId};
        use epiphany_layout_ir::to_logical;

        let mut score = ten_measure_single_staff(1);
        let time_signature: TimeSignatureId = score.identity.mint();
        let (measure_id, region_id) = {
            let region = &mut score.canvas.regions[0];
            let region_id = region.id;
            let instance = region
                .content
                .staff_instances_mut()
                .expect("the fixture is staff-based")
                .first_mut()
                .expect("a staff instance");
            instance.measures[0].time_signature = Some(time_signature);
            (instance.measures[0].id, region_id)
        };

        let logical = to_logical(&score);
        let measure = logical
            .regions
            .iter()
            .flat_map(|r| r.objects.iter())
            .find(|o| o.provenance().source == TypedObjectId::Measure(measure_id))
            .expect("measure 0 is projected");
        let deps = &measure.provenance().dependencies;
        assert!(
            deps.contains(&TypedObjectId::TimeSignature(time_signature)),
            "a measure depends on the time signature it displays"
        );
        // Each fixture measure is region-anchored, so the region is a dep too.
        assert!(
            deps.contains(&TypedObjectId::Region(region_id)),
            "a measure depends on its start anchor's target"
        );
    }
}
