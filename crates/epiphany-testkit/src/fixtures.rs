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
    EventPosition, IdentifiedPitch, IdentityContext, LineStyle, Marker, Measure, MeasurePosition,
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

/// A C pitched quarter-note at `octave`, at musical position `index/4`.
fn c_at(eid: EventId, voice: VoiceId, pid: PitchId, index: i64, octave: i8) -> Event {
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
                        octave,
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

/// A one-measure, TWO-staff (treble/treble) metric score whose staves carry
/// vertically extreme content: the top staff dips to low ledger notes (down to
/// C2) while the bottom staff climbs to high ledger notes (up to C6) under a
/// slur that arcs further above them. At the engraver's default *fixed* staff
/// pitch the two staves' content nearly collides — the inter-staff pressure case
/// the vertical spring solve must separate. Invariant-clean.
pub fn two_staff_close_content(seed: u64) -> Score {
    let mut rng = SplitMix64::new(seed ^ 0x0002_57AF_F123);
    let replica =
        ReplicaId::from_entropy(rng.next_u64().to_le_bytes()).unwrap_or(ReplicaId(0x2ADF));
    let mut idc = IdentityContext::new(replica);

    let top_staff: StaffId = idc.mint();
    let bottom_staff: StaffId = idc.mint();
    let instrument: InstrumentId = idc.mint();
    let region_id: RegionId = idc.mint();
    let top_instance: StaffInstanceId = idc.mint();
    let bottom_instance: StaffInstanceId = idc.mint();
    let top_voice: VoiceId = idc.mint();
    let bottom_voice: VoiceId = idc.mint();

    let mut arena = EventArena::new();
    // Top staff descends into low ledgers; bottom staff climbs into high ones.
    let mut top_v = Voice::user(top_voice);
    let mut bot_v = Voice::user(bottom_voice);
    let mut bot_events: Vec<EventId> = Vec::new();
    for (i, oct) in [4i8, 3, 2, 3].into_iter().enumerate() {
        let (eid, pid): (EventId, PitchId) = (idc.mint(), idc.mint());
        arena
            .insert(c_at(eid, top_voice, pid, i as i64, oct))
            .unwrap();
        top_v.events.push(eid);
    }
    for (i, oct) in [4i8, 5, 6, 5].into_iter().enumerate() {
        let (eid, pid): (EventId, PitchId) = (idc.mint(), idc.mint());
        arena
            .insert(c_at(eid, bottom_voice, pid, i as i64, oct))
            .unwrap();
        bot_v.events.push(eid);
        bot_events.push(eid);
    }

    let measure = |id: MeasureId| Measure {
        id,
        start: TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        },
        time_signature: None,
        explicit_number: Some(1),
        number_visibility: Default::default(),
    };
    let mut top_inst = StaffInstance::new(top_instance, top_staff);
    top_inst.voices.push(top_v);
    top_inst.measures.push(measure(idc.mint()));
    let mut bot_inst = StaffInstance::new(bottom_instance, bottom_staff);
    bot_inst.voices.push(bot_v);
    bot_inst.measures.push(measure(idc.mint()));

    // A slur over the bottom staff's high notes, arcing further up — extra
    // upward extent that sharpens the collision with the top staff.
    let mut cross_cutting = CrossCuttingRegistry::default();
    cross_cutting.slurs.push(Slur {
        id: idc.mint::<SlurId>(),
        start_event: bot_events[1],
        end_event: bot_events[3],
        kind: SlurKind::Legato,
        curvature_override: None,
        style: SpanStyle::default(),
    });

    let region = epiphany_core::Region {
        id: region_id,
        time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![top_inst, bot_inst],
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
            staves: vec![top_staff, bottom_staff],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    let staff = |id: StaffId, name: &str| Staff {
        id,
        name: String::from(name),
        abbreviation: None,
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
        default_clef: epiphany_core::Clef::treble(),
    };
    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.instruments = vec![epiphany_core::Instrument::new(
        instrument,
        String::from("Keyboard"),
    )];
    score.staves = vec![staff(top_staff, "Right"), staff(bottom_staff, "Left")];
    score.events = arena;
    score.cross_cutting = cross_cutting;
    score.canvas = Canvas {
        regions: vec![region],
        ..Default::default()
    };
    score
}

/// A one-measure, THREE-staff metric score with **asymmetric** inter-staff
/// pressure, built to exercise the vertical solve's cumulative shift cascade.
///
/// The upper pair collides hard (staff 1 plunges to C1 while staff 2 towers to
/// C7); the lower pair collides gently (staff 2's C3 against staff 3's C6 and
/// the slur arcing over it). So the first correction is several times the
/// second — which is the point. A solve that computed each pair's correction
/// *independently* would shift staff 3 by only the small amount while staff 2
/// took the large one, pulling staff 3 back **through** staff 2 and closing
/// their staff-line gap to well under the fixed pitch. Only a solve whose shift
/// accumulates down the stack keeps every pair separated. Invariant-clean.
pub fn three_staff_close_content(seed: u64) -> Score {
    let mut rng = SplitMix64::new(seed ^ 0x0003_57AF_F123);
    let replica =
        ReplicaId::from_entropy(rng.next_u64().to_le_bytes()).unwrap_or(ReplicaId(0x3ADF));
    let mut idc = IdentityContext::new(replica);

    let staves: Vec<StaffId> = (0..3).map(|_| idc.mint()).collect();
    let instrument: InstrumentId = idc.mint();
    let region_id: RegionId = idc.mint();

    let measure = |id: MeasureId| Measure {
        id,
        start: TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        },
        time_signature: None,
        explicit_number: Some(1),
        number_visibility: Default::default(),
    };

    // Staff 1 dips to low ledgers; staff 2 spans C7 down to C3 (so it presses
    // upward on staff 1 and downward on staff 3); staff 3 climbs to C6.
    let octaves: [[i8; 4]; 3] = [[4, 3, 2, 1], [7, 3, 7, 3], [6, 5, 6, 5]];
    let mut arena = EventArena::new();
    let mut instances: Vec<StaffInstance> = Vec::new();
    let mut low_events: Vec<EventId> = Vec::new();
    for (staff_index, staff_id) in staves.iter().enumerate() {
        let instance_id: StaffInstanceId = idc.mint();
        let voice_id: VoiceId = idc.mint();
        let mut voice = Voice::user(voice_id);
        for (i, oct) in octaves[staff_index].into_iter().enumerate() {
            let (eid, pid): (EventId, PitchId) = (idc.mint(), idc.mint());
            arena
                .insert(c_at(eid, voice_id, pid, i as i64, oct))
                .unwrap();
            voice.events.push(eid);
            if staff_index == 2 {
                low_events.push(eid);
            }
        }
        let mut instance = StaffInstance::new(instance_id, *staff_id);
        instance.voices.push(voice);
        instance.measures.push(measure(idc.mint()));
        instances.push(instance);
    }

    // A slur over the bottom staff's high notes, arcing further up — the extra
    // upward extent that turns the lower pair's near-miss into real pressure.
    let mut cross_cutting = CrossCuttingRegistry::default();
    cross_cutting.slurs.push(Slur {
        id: idc.mint::<SlurId>(),
        start_event: low_events[0],
        end_event: low_events[2],
        kind: SlurKind::Legato,
        curvature_override: None,
        style: SpanStyle::default(),
    });

    let region = epiphany_core::Region {
        id: region_id,
        time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: instances,
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
            staves: staves.clone(),
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    let staff = |id: StaffId, name: &str| Staff {
        id,
        name: String::from(name),
        abbreviation: None,
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
        default_clef: epiphany_core::Clef::treble(),
    };
    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.instruments = vec![epiphany_core::Instrument::new(
        instrument,
        String::from("Organ"),
    )];
    score.staves = vec![
        staff(staves[0], "Upper"),
        staff(staves[1], "Middle"),
        staff(staves[2], "Lower"),
    ];
    score.events = arena;
    score.cross_cutting = cross_cutting;
    score.canvas = Canvas {
        regions: vec![region],
        ..Default::default()
    };
    score
}

/// A TWELVE-measure, TWO-staff metric score that wraps into more than one
/// system and carries its inter-staff pressure in the FIRST measure only: the
/// top staff dips to C2 and the bottom staff climbs to C6 there, while every
/// later measure is plain C4s on both staves.
///
/// So the two systems of the one region want genuinely different staff gaps —
/// the first must open to clear the colliding ledgers, the second is already
/// slack. The inter-staff solve sizes each system's gaps from that system's own
/// content, so a quality metric that measured only the first system realizing a
/// gap band would average the second one away. Invariant-clean.
pub fn two_staff_wrapping_pressure(seed: u64) -> Score {
    let mut rng = SplitMix64::new(seed ^ 0x0004_57AF_F123);
    let replica =
        ReplicaId::from_entropy(rng.next_u64().to_le_bytes()).unwrap_or(ReplicaId(0x4ADF));
    let mut idc = IdentityContext::new(replica);

    let top_staff: StaffId = idc.mint();
    let bottom_staff: StaffId = idc.mint();
    let instrument: InstrumentId = idc.mint();
    let region_id: RegionId = idc.mint();

    const MEASURES: i64 = 12;
    const BEATS: i64 = 4;

    // Measure 0 collides (top dives, bottom climbs); the rest sit inside the
    // staff, where the fixed staff pitch already leaves the gap slack.
    let octave = |staff_index: usize, index: i64| -> i8 {
        if index >= BEATS {
            return 4;
        }
        if staff_index == 0 {
            [4i8, 3, 2, 3][index as usize]
        } else {
            [4i8, 5, 6, 5][index as usize]
        }
    };

    let mut arena = EventArena::new();
    let mut instances: Vec<StaffInstance> = Vec::new();
    for (staff_index, staff_id) in [top_staff, bottom_staff].into_iter().enumerate() {
        let instance_id: StaffInstanceId = idc.mint();
        let voice_id: VoiceId = idc.mint();
        let mut voice = Voice::user(voice_id);
        for index in 0..(MEASURES * BEATS) {
            let (eid, pid): (EventId, PitchId) = (idc.mint(), idc.mint());
            arena
                .insert(c_at(eid, voice_id, pid, index, octave(staff_index, index)))
                .unwrap();
            voice.events.push(eid);
        }
        let mut instance = StaffInstance::new(instance_id, staff_id);
        instance.voices.push(voice);
        for m in 0..MEASURES {
            instance.measures.push(Measure {
                id: idc.mint(),
                start: TimeAnchor::Region {
                    id: region_id,
                    edge: RegionEdge::Start,
                    offset: AnchorOffset::Musical(MusicalDuration(
                        RationalTime::new(m, 1).unwrap(),
                    )),
                },
                time_signature: None,
                explicit_number: Some((m + 1) as u32),
                number_visibility: Default::default(),
            });
        }
        instances.push(instance);
    }

    let region = epiphany_core::Region {
        id: region_id,
        time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: instances,
            ..Default::default()
        }),
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(120_000_000),
            },
        },
        staff_extent: StaffExtent {
            staves: vec![top_staff, bottom_staff],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    let staff = |id: StaffId, name: &str| Staff {
        id,
        name: String::from(name),
        abbreviation: None,
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
        default_clef: epiphany_core::Clef::treble(),
    };
    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.instruments = vec![epiphany_core::Instrument::new(
        instrument,
        String::from("Keyboard"),
    )];
    score.staves = vec![staff(top_staff, "Right"), staff(bottom_staff, "Left")];
    score.events = arena;
    score.canvas = Canvas {
        regions: vec![region],
        ..Default::default()
    };
    score
}

/// A two-staff score whose LOWER staff engraves **no glyphs at all**: it is a
/// percussion-clef placeholder — a staff instance with a `ClefChange` to
/// `ClefShape::Percussion`, which has no bundled SMuFL glyph (it engraves to a
/// traced anchor stroke), and no voices or measures. Its vertical band therefore
/// owns five staff-line strokes plus that anchor, and **zero** members.
///
/// The upper staff carries twelve plain measures of C4, so it wraps and its gap
/// to the placeholder is slack everywhere. Together: a valid score on which any
/// consumer that identifies a region's staff bands by their glyph `members`
/// silently loses the lower staff. Invariant-clean.
pub fn percussion_placeholder_staff(seed: u64) -> Score {
    let mut rng = SplitMix64::new(seed ^ 0x0005_57AF_F123);
    let replica =
        ReplicaId::from_entropy(rng.next_u64().to_le_bytes()).unwrap_or(ReplicaId(0x5ADF));
    let mut idc = IdentityContext::new(replica);

    let top_staff: StaffId = idc.mint();
    let drum_staff: StaffId = idc.mint();
    let instrument: InstrumentId = idc.mint();
    let region_id: RegionId = idc.mint();

    const MEASURES: i64 = 12;
    const BEATS: i64 = 4;

    let mut arena = EventArena::new();
    let top_instance: StaffInstanceId = idc.mint();
    let top_voice: VoiceId = idc.mint();
    let mut voice = Voice::user(top_voice);
    for index in 0..(MEASURES * BEATS) {
        let (eid, pid): (EventId, PitchId) = (idc.mint(), idc.mint());
        arena.insert(quarter(eid, top_voice, pid, index)).unwrap();
        voice.events.push(eid);
    }
    let mut top = StaffInstance::new(top_instance, top_staff);
    top.voices.push(voice);
    for m in 0..MEASURES {
        top.measures.push(Measure {
            id: idc.mint(),
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

    // The placeholder: a percussion clef, no voices, no measures.
    let drum_instance: StaffInstanceId = idc.mint();
    let mut drums = StaffInstance::new(drum_instance, drum_staff);
    drums.clef_sequence.push(epiphany_core::ClefChange {
        anchor: TimeAnchor::WallClock {
            time: WallClockTime(0),
        },
        clef: epiphany_core::Clef {
            shape: epiphany_core::ClefShape::Percussion,
            line: 3,
            octave_shift: 0,
        },
    });

    let region = epiphany_core::Region {
        id: region_id,
        time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![top, drums],
            ..Default::default()
        }),
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(120_000_000),
            },
        },
        staff_extent: StaffExtent {
            staves: vec![top_staff, drum_staff],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    let staff = |id: StaffId, name: &str, clef: epiphany_core::Clef| Staff {
        id,
        name: String::from(name),
        abbreviation: None,
        instrument,
        default_staff_lines: StaffLineConfiguration::default(),
        group: None,
        default_clef: clef,
    };
    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.instruments = vec![epiphany_core::Instrument::new(
        instrument,
        String::from("Ensemble"),
    )];
    score.staves = vec![
        staff(top_staff, "Melody", epiphany_core::Clef::treble()),
        staff(
            drum_staff,
            "Drums",
            epiphany_core::Clef {
                shape: epiphany_core::ClefShape::Percussion,
                line: 3,
                octave_shift: 0,
            },
        ),
    ];
    score.events = arena;
    score.canvas = Canvas {
        regions: vec![region],
        ..Default::default()
    };
    score
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
        // An authored dashed line — rendered faithfully as a dashed cubic
        // (`stroke-dasharray`), so the fixture exercises the line-pattern path.
        style: SpanStyle {
            line: LineStyle::Dashed,
            thickness: None,
        },
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
    fn two_staff_close_content_is_invariant_clean_and_two_staves() {
        let s = two_staff_close_content(1);
        let v = check_invariants(&s);
        assert!(v.is_empty(), "two-staff fixture has violations: {v:?}");
        assert_eq!(s.staves.len(), 2, "two staves");
        assert_eq!(
            s.canvas.regions[0].staff_instances().len(),
            2,
            "two staff instances in the region"
        );
        assert_eq!(s.events.len(), 8, "four notes per staff");
        assert_eq!(s.cross_cutting.slurs.len(), 1, "a slur over the high notes");
    }

    #[test]
    fn percussion_placeholder_staff_is_invariant_clean_and_glyphless_below() {
        use epiphany_layout_ir::{to_constrained, to_logical, VerticalBandKind};
        let s = percussion_placeholder_staff(1);
        let v = check_invariants(&s);
        assert!(v.is_empty(), "percussion fixture has violations: {v:?}");
        let c = to_constrained(&to_logical(&s));
        let glyphless: Vec<_> = c
            .vertical_bands
            .iter()
            .filter(|b| matches!(b.kind, VerticalBandKind::Staff(_)))
            .filter(|b| !c.glyphs.iter().any(|g| g.vertical_band == b.id))
            .collect();
        assert_eq!(
            glyphless.len(),
            1,
            "exactly one staff band owns no glyph (the percussion placeholder)"
        );
        assert!(
            glyphless[0].members.is_empty(),
            "and therefore no band members either"
        );
        assert!(
            c.strokes
                .iter()
                .filter(|st| st.vertical_band == glyphless[0].id)
                .count()
                >= 5,
            "but it does own its staff lines"
        );
    }

    #[test]
    fn two_staff_wrapping_pressure_is_invariant_clean_and_front_loaded() {
        let s = two_staff_wrapping_pressure(1);
        let v = check_invariants(&s);
        assert!(
            v.is_empty(),
            "wrapping two-staff fixture has violations: {v:?}"
        );
        assert_eq!(s.staves.len(), 2, "two staves");
        assert_eq!(s.events.len(), 96, "12 measures x 4 beats x 2 staves");
        assert_eq!(
            s.canvas.regions[0].staff_instances().len(),
            2,
            "two staff instances in ONE region -- so both staves share every system"
        );
    }

    #[test]
    fn three_staff_close_content_is_invariant_clean_and_asymmetric() {
        let s = three_staff_close_content(1);
        let v = check_invariants(&s);
        assert!(v.is_empty(), "three-staff fixture has violations: {v:?}");
        assert_eq!(s.staves.len(), 3, "three staves");
        assert_eq!(
            s.canvas.regions[0].staff_instances().len(),
            3,
            "three staff instances in ONE region — so all three land in one system"
        );
        assert_eq!(s.events.len(), 12, "four notes per staff");
        assert_eq!(s.cross_cutting.slurs.len(), 1, "a slur over the low staff");
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
