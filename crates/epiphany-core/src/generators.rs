//! Deterministic generators and shrinkers for the graph invariants
//! (QUICKSTART, Agent B: *"For each invariant, you must have a positive
//! generator that produces valid graphs and a negative shrinker that minimizes
//! invariant violations to a small witness for debugging"*).
//!
//! * [`valid_score`] is the **positive generator**: a seeded, well-formed score
//!   graph for which [`check_invariants`](crate::check_invariants) is empty.
//!   [`arbitrary_graph_corpus`] is the seeded corpus the property tests sweep.
//! * [`violating_score`] is the **negative generator**: for any
//!   [`GraphInvariant`] it produces a graph that violates that invariant.
//! * [`shrink`] is the **shrinker**: it greedily prunes a violating graph to a
//!   small witness that still violates the same invariant, retaining only the
//!   structure the violation needs.
//!
//! Generation is deterministic (a vendored SplitMix64, like Agent A's fuzz
//! harness) so a failing case reproduces exactly from its seed; no platform
//! entropy enters generation (Appendix D §"Randomness").

use epiphany_determinism::fuzz::SplitMix64;

use crate::event::{Event, PitchedEvent, StemConfiguration};
use crate::graph::{
    AleatoricAnchoringDiscipline, AleatoricTimeModel, Canvas, ChordSymbol, DecompositionAttachment,
    DecompositionSource, Instrument, Marker, Measure, MetricTimeModel, NotatedComponent, NoteValue,
    ProportionalTimeModel, Region, RegionContent, RegionTimeModel, Score, Slur, Spanner, Staff,
    StaffBasedContent, StaffExtent, StaffInstance, StaffLineConfiguration, Tie, TieClass,
    TimeExtent, Tuplet, TupletRatio, Voice,
};
use crate::ids::{
    ChordSymbolId, EventId, IdentityContext, InstrumentId, MarkerId, MeasureId, PitchId, RegionId,
    ReplicaId, SlurId, SpannerId, StaffId, StaffInstanceId, TieId, TupletId, VoiceId,
};
use crate::invariants::{check_invariant, GraphInvariant};
use crate::pitch::{
    AcousticPitch, AcousticRealization, CmnNominal, IdentifiedPitch, Pitch, PitchSpaceId,
    PitchSpacePosition, PitchSpelling, ScalePosition, SpellingAttachment, SpellingDirective,
    SpellingScope, SpellingSource, TuningReference,
};
use crate::time::{
    AnchorOffset, EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime,
    RegionEdge, TimeAnchor, WallClockDuration, WallClockTime,
};

const NOMINALS: [CmnNominal; 7] = [
    CmnNominal::C,
    CmnNominal::D,
    CmnNominal::E,
    CmnNominal::F,
    CmnNominal::G,
    CmnNominal::A,
    CmnNominal::B,
];

fn cmn_identified_pitch(pid: PitchId, step: usize) -> IdentifiedPitch {
    IdentifiedPitch {
        id: pid,
        pitch: Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new("cmn-12"),
                position: PitchSpacePosition::Cmn {
                    nominal: NOMINALS[step % 7],
                    alteration: 0,
                    octave: 4,
                },
            },
            acoustic: AcousticPitch {
                tuning: TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        },
    }
}

/// A single-note pitched event at musical position `index/4`, duration `1/4`.
fn pitched_event(eid: EventId, voice: VoiceId, pid: PitchId, index: i64) -> Event {
    Event::Pitched(PitchedEvent {
        id: eid,
        voice,
        position: EventPosition::Musical(MusicalPosition(RationalTime::new(index, 4).unwrap())),
        duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
        pitches: vec![cmn_identified_pitch(pid, index as usize)],
        articulations: Vec::new(),
        dynamic: None,
        ornaments: Vec::new(),
        stem: StemConfiguration,
        grace: None,
    })
}

/// The positive generator: a seeded, well-formed metric score graph.
///
/// Shape: one metric region placed on a wall-clock extent, 1–2 staves, each
/// manifested by one staff instance carrying 1–2 voices of 2–4 single-note
/// pitched events at non-overlapping quarter-note positions. Every Chapter 5
/// invariant holds.
pub fn valid_score(seed: u64) -> Score {
    let mut rng = SplitMix64::new(seed ^ 0x5EED_C0DE_1234_5678);
    // A non-reserved replica (re-draw on the astronomically unlikely reserved
    // value).
    let replica =
        ReplicaId::from_entropy(rng.next_u64().to_le_bytes()).unwrap_or(ReplicaId(0x1234));
    let mut idc = IdentityContext::new(replica);

    let staff_count = 1 + (rng.next_u64() % 2) as usize; // 1..=2
    let region_id: RegionId = idc.mint();

    let mut staves = Vec::new();
    let mut instruments = Vec::new();
    let mut instances = Vec::new();
    let mut arena = crate::event::EventArena::new();
    let mut staff_extent = Vec::new();

    for _ in 0..staff_count {
        let staff_id: StaffId = idc.mint();
        let instrument: InstrumentId = idc.mint();
        // Declare the instrument so the staff's reference resolves (invariant 10).
        instruments.push(Instrument {
            id: instrument,
            name: String::from("instrument"),
            range: None,
        });
        staves.push(Staff {
            id: staff_id,
            name: String::from("staff"),
            abbreviation: None,
            instrument,
            default_staff_lines: StaffLineConfiguration::default(),
            group: None,
        });
        staff_extent.push(staff_id);

        let instance_id: StaffInstanceId = idc.mint();
        let mut instance = StaffInstance::new(instance_id, staff_id);

        let voice_count = 1 + (rng.next_u64() % 2) as usize; // 1..=2
        for _ in 0..voice_count {
            let voice_id: VoiceId = idc.mint();
            let mut voice = Voice::user(voice_id);
            let event_count = 2 + (rng.next_u64() % 3) as i64; // 2..=4
            for index in 0..event_count {
                let eid: EventId = idc.mint();
                let pid: PitchId = idc.mint();
                arena
                    .insert(pitched_event(eid, voice_id, pid, index))
                    .expect("fresh event id");
                voice.events.push(eid);
            }
            instance.voices.push(voice);
        }
        instances.push(instance);
    }

    let region = Region {
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
                time: WallClockTime(1_000_000),
            },
        },
        staff_extent: StaffExtent {
            staves: staff_extent,
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.staves = staves;
    score.instruments = instruments;
    score.events = arena;
    score.canvas = Canvas {
        regions: vec![region],
        ..Default::default()
    };
    score
}

/// `count` valid graphs from successive seeds — the "arbitrary-graph corpus"
/// the hand-off gate runs clean against.
pub fn arbitrary_graph_corpus(count: u64, base_seed: u64) -> impl Iterator<Item = Score> {
    (0..count).map(move |i| valid_score(base_seed.wrapping_add(i).wrapping_mul(0x9E37_79B9)))
}

/// A second positive generator exercising the breadth the simple
/// [`valid_score`] does not: three **concurrent** regions on disjoint staves —
/// metric (with measures, an eighth-note triplet [`Tuplet`], a standard [`Tie`],
/// a [`Spanner`], a [`Marker`], a [`ChordSymbol`], and a
/// [`DecompositionAttachment`]), proportional (wall-clock events), and aleatoric
/// (musical-discipline events) — plus tombstoned pitch/event ids and a spelling
/// attachment that resolves to the tombstoned pitch. Every Chapter 5 invariant
/// holds; this is the corpus that exercises the reference, anchor, tuplet, tie,
/// decomposition, and tombstone paths.
pub fn valid_score_rich(seed: u64) -> Score {
    let mut rng = SplitMix64::new(seed ^ 0x1CE_B00D_A11C_E5EE);
    let replica =
        ReplicaId::from_entropy(rng.next_u64().to_le_bytes()).unwrap_or(ReplicaId(0x7777));
    let mut idc = IdentityContext::new(replica);

    let extent = || TimeExtent {
        start: TimeAnchor::WallClock {
            time: WallClockTime(0),
        },
        end: TimeAnchor::WallClock {
            time: WallClockTime(1_000_000),
        },
    };
    let mut staves = Vec::new();
    let mut instruments = Vec::new();
    let mk_staff = |idc: &mut IdentityContext,
                    staves: &mut Vec<Staff>,
                    instruments: &mut Vec<Instrument>|
     -> StaffId {
        let id: StaffId = idc.mint();
        let instrument: InstrumentId = idc.mint();
        instruments.push(Instrument {
            id: instrument,
            name: String::from("instrument"),
            range: None,
        });
        staves.push(Staff {
            id,
            name: String::from("staff"),
            abbreviation: None,
            instrument,
            default_staff_lines: StaffLineConfiguration::default(),
            group: None,
        });
        id
    };
    let staff_a = mk_staff(&mut idc, &mut staves, &mut instruments);
    let staff_b = mk_staff(&mut idc, &mut staves, &mut instruments);
    let staff_c = mk_staff(&mut idc, &mut staves, &mut instruments);

    let mut arena = crate::event::EventArena::new();
    let mut cross_cutting = crate::graph::CrossCuttingRegistry::default();
    let mut decomposition_attachments = Vec::new();

    // --- Metric region on staff A: triplet of three C4 eighths (each 1/12). --
    let region_a: RegionId = idc.mint();
    let inst_a: StaffInstanceId = idc.mint();
    let voice_a: VoiceId = idc.mint();
    let mut va = Voice::user(voice_a);
    let mut triplet_members = Vec::new();
    for k in 0..3i64 {
        let eid: EventId = idc.mint();
        let pid: PitchId = idc.mint();
        let ev = Event::Pitched(PitchedEvent {
            id: eid,
            voice: voice_a,
            position: EventPosition::Musical(MusicalPosition(RationalTime::new(k, 12).unwrap())),
            duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 12).unwrap())),
            // All C4 so any adjacent pair is enharmonically equivalent (valid tie).
            pitches: vec![cmn_identified_pitch(pid, 0)],
            articulations: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: StemConfiguration,
            grace: None,
        });
        arena.insert(ev).unwrap();
        va.events.push(eid);
        triplet_members.push(eid);
    }
    let measure_a: MeasureId = idc.mint();
    let mut sia = StaffInstance::new(inst_a, staff_a);
    sia.voices.push(va);
    sia.measures.push(Measure {
        id: measure_a,
        // Region-anchored with a musical offset: valid offset kind for a metric
        // region (invariant 9), target resolves (invariant 10).
        start: TimeAnchor::Region {
            id: region_a,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration::zero()),
        },
        time_signature: None,
        explicit_number: Some(1),
        number_visibility: Default::default(),
    });
    // Triplet, tie, decomposition, spanner, marker, chord symbol.
    let triplet_id: TupletId = idc.mint();
    cross_cutting.tuplets.push(Tuplet {
        id: triplet_id,
        ratio: TupletRatio::new(3, 2).expect("3:2 is a valid tuplet ratio"),
        members: triplet_members.clone(),
        parent: None,
        required_total: MusicalDuration(RationalTime::new(1, 4).unwrap()),
    });
    cross_cutting.ties.push(Tie {
        id: idc.mint::<TieId>(),
        start_event: triplet_members[0],
        end_event: triplet_members[1],
        pitch_pairing: None,
        class: TieClass::Standard,
    });
    // The first triplet member is an eighth in a 3:2 triplet, so its sounding
    // duration is 1/8 × 2/3 = 1/12 — matching the event's duration (invariant 15).
    decomposition_attachments.push(DecompositionAttachment {
        target: triplet_members[0],
        components: vec![NotatedComponent {
            base_value: NoteValue::Eighth,
            dots: 0,
            tuplet: Some(triplet_id),
            tied_to_next: false,
        }],
        source: DecompositionSource::Inferred,
    });
    cross_cutting.spanners.push(Spanner {
        id: idc.mint::<SpannerId>(),
        start: TimeAnchor::Region {
            id: region_a,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration::zero()),
        },
        end: TimeAnchor::WallClock {
            time: WallClockTime(10),
        },
        staves: vec![staff_a],
    });
    cross_cutting.markers.push(Marker {
        id: idc.mint::<MarkerId>(),
        anchor: TimeAnchor::Region {
            id: region_a,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        },
    });
    cross_cutting.chord_symbols.push(ChordSymbol {
        id: idc.mint::<ChordSymbolId>(),
        anchor: TimeAnchor::Region {
            id: region_a,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        },
    });
    let region_a = Region {
        id: region_a,
        time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![sia],
            ..Default::default()
        }),
        time_extent: extent(),
        staff_extent: StaffExtent {
            staves: vec![staff_a],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    // --- Proportional region on staff B: wall-clock events. ------------------
    let inst_b: StaffInstanceId = idc.mint();
    let voice_b: VoiceId = idc.mint();
    let mut vb = Voice::user(voice_b);
    for k in 0..2i64 {
        let eid: EventId = idc.mint();
        let pid: PitchId = idc.mint();
        arena
            .insert(Event::Pitched(PitchedEvent {
                id: eid,
                voice: voice_b,
                position: EventPosition::WallClock(WallClockTime(k * 1000)),
                duration: EventDuration::WallClock(WallClockDuration(1000)),
                pitches: vec![cmn_identified_pitch(pid, k as usize)],
                articulations: vec![],
                dynamic: None,
                ornaments: vec![],
                stem: StemConfiguration,
                grace: None,
            }))
            .unwrap();
        vb.events.push(eid);
    }
    let mut sib = StaffInstance::new(inst_b, staff_b);
    sib.voices.push(vb);
    let region_b = Region {
        id: idc.mint(),
        time_model: RegionTimeModel::Proportional(ProportionalTimeModel {
            duration: WallClockDuration(1_000_000),
        }),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![sib],
            ..Default::default()
        }),
        time_extent: extent(),
        staff_extent: StaffExtent {
            staves: vec![staff_b],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    // --- Aleatoric region on staff C (musical discipline). -------------------
    let inst_c: StaffInstanceId = idc.mint();
    let voice_c: VoiceId = idc.mint();
    let mut vc = Voice::user(voice_c);
    for k in 0..2i64 {
        let eid: EventId = idc.mint();
        let pid: PitchId = idc.mint();
        arena
            .insert(Event::Pitched(PitchedEvent {
                id: eid,
                voice: voice_c,
                position: EventPosition::Musical(MusicalPosition(RationalTime::new(k, 4).unwrap())),
                duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
                pitches: vec![cmn_identified_pitch(pid, k as usize)],
                articulations: vec![],
                dynamic: None,
                ornaments: vec![],
                stem: StemConfiguration,
                grace: None,
            }))
            .unwrap();
        vc.events.push(eid);
    }
    let mut sic = StaffInstance::new(inst_c, staff_c);
    sic.voices.push(vc);
    let region_c = Region {
        id: idc.mint(),
        time_model: RegionTimeModel::Aleatoric(AleatoricTimeModel {
            ordering: Default::default(),
            anchoring: AleatoricAnchoringDiscipline::Musical,
            bounds: Default::default(),
            duration_hint: WallClockDuration(1_000_000),
        }),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![sic],
            ..Default::default()
        }),
        time_extent: extent(),
        staff_extent: StaffExtent {
            staves: vec![staff_c],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    // --- Tombstones + a spelling attachment resolving to a tombstoned pitch. -
    let tomb_pitch: PitchId = idc.mint();
    let tomb_event: EventId = idc.mint();
    let spelling_attachments = vec![SpellingAttachment {
        scope: SpellingScope::Pitch(tomb_pitch),
        directive: SpellingDirective::Explicit(PitchSpelling::cmn(CmnNominal::C, 4)),
        source: SpellingSource::UserChosen,
        priority: 0,
        layer: None,
    }];

    let mut score = Score::empty(idc);
    score.staves = staves;
    score.instruments = instruments;
    score.events = arena;
    score.cross_cutting = cross_cutting;
    score.decomposition_attachments = decomposition_attachments;
    score.spelling_attachments = spelling_attachments;
    score.tombstoned_pitches.insert(tomb_pitch);
    score.tombstoned_events.insert(tomb_event);
    score.canvas = Canvas {
        regions: vec![region_a, region_b, region_c],
        ..Default::default()
    };
    score
}

// --- Small read accessors into a generated score. ---------------------------

fn first_region_id(s: &Score) -> RegionId {
    s.canvas.regions[0].id
}
fn first_two_event_ids(s: &Score) -> (EventId, EventId) {
    let evs = &s.canvas.regions[0].staff_instances()[0].voices[0].events;
    (evs[0], evs[1])
}
fn first_pitch_id(s: &Score, event: EventId) -> PitchId {
    let mut buf = Vec::new();
    s.events
        .get(event)
        .unwrap()
        .collect_identified_pitches(&mut buf);
    buf[0].id
}
fn first_voice_mut(s: &mut Score) -> &mut Voice {
    s.canvas.regions[0].content.staff_instances_mut().unwrap()[0]
        .voices
        .get_mut(0)
        .unwrap()
}

/// The negative generator: a graph that violates `inv`. Built from
/// [`valid_score`] then corrupted in the smallest way that triggers the
/// targeted invariant.
pub fn violating_score(inv: GraphInvariant, seed: u64) -> Score {
    use GraphInvariant::*;
    let mut s = valid_score(seed);
    let replica = s.identity.replica_id;
    match inv {
        EventVoiceBacklink => {
            // Arena event whose voice no longer lists it: drop it from the list.
            let (e0, _) = first_two_event_ids(&s);
            first_voice_mut(&mut s).events.retain(|e| *e != e0);
        }
        VoiceEventBacklink => {
            // A voice lists an event that is not in the arena.
            let ghost = EventId::new(replica, 9_999_999);
            first_voice_mut(&mut s).events.push(ghost);
        }
        VoiceEventsSortedNonOverlap => {
            // Stretch event 0 so it overlaps event 1.
            let (e0, _) = first_two_event_ids(&s);
            if let Some(Event::Pitched(p)) = s.events.get_mut(e0) {
                p.duration =
                    EventDuration::Musical(MusicalDuration(RationalTime::new(1, 1).unwrap()));
            }
        }
        EventCoordinateModel => {
            // A wall-clock event inside a metric region.
            let (e0, _) = first_two_event_ids(&s);
            if let Some(Event::Pitched(p)) = s.events.get_mut(e0) {
                p.position = EventPosition::WallClock(WallClockTime(0));
                p.duration = EventDuration::WallClock(crate::time::WallClockDuration(1000));
            }
        }
        ContainmentTree => {
            // The same voice id under a second instance in the same region.
            let dup = s.canvas.regions[0].staff_instances()[0].voices[0].clone();
            let new_inst_id: StaffInstanceId = s.identity.mint();
            let staff = s.canvas.regions[0].staff_instances()[0].staff;
            let mut inst = StaffInstance::new(new_inst_id, staff);
            inst.voices.push(dup);
            s.canvas.regions[0]
                .content
                .staff_instances_mut()
                .unwrap()
                .push(inst);
        }
        StaffInstanceResolves => {
            // Instance references an undeclared staff; keep staff_extent in sync
            // so only invariant 6 fires.
            let bogus = StaffId::new(replica, 8_888_888);
            s.canvas.regions[0].content.staff_instances_mut().unwrap()[0].staff = bogus;
            s.canvas.regions[0].staff_extent.staves = vec![bogus];
        }
        RegionExtents => {
            // staff_extent lists a staff that no instance manifests.
            let extra = StaffId::new(replica, 7_777_777);
            s.canvas.regions[0].staff_extent.staves.push(extra);
        }
        MeasureSingleInstance => {
            // The same measure id in two instances. Needs two instances; add a
            // second instance carrying a measure that already exists in the
            // first (build both measures with one shared id).
            let mid = crate::ids::MeasureId::new(replica, 4_242_424);
            let r = &mut s.canvas.regions[0];
            let staff = r.staff_instances()[0].staff;
            let measure = crate::graph::Measure {
                id: mid,
                start: TimeAnchor::WallClock {
                    time: WallClockTime(0),
                },
                time_signature: None,
                explicit_number: None,
                number_visibility: Default::default(),
            };
            r.content.staff_instances_mut().unwrap()[0]
                .measures
                .push(measure.clone());
            let new_inst_id: StaffInstanceId = s.identity.mint();
            let mut inst = StaffInstance::new(new_inst_id, staff);
            inst.measures.push(measure);
            // Avoid a second-staff-in-region collision (invariant 6) by giving
            // this instance a distinct, declared staff.
            let staff2: StaffId = s.identity.mint();
            inst.staff = staff2;
            s.staves.push(Staff {
                id: staff2,
                name: String::from("s2"),
                abbreviation: None,
                instrument: s.identity.mint(),
                default_staff_lines: StaffLineConfiguration::default(),
                group: None,
            });
            s.canvas.regions[0].staff_extent.staves.push(staff2);
            s.canvas.regions[0]
                .content
                .staff_instances_mut()
                .unwrap()
                .push(inst);
        }
        AnchorOffsetModel => {
            // A spanner anchored to the metric region with a wall-clock offset.
            let rid = first_region_id(&s);
            let staff = s.staves[0].id;
            s.cross_cutting.spanners.push(Spanner {
                id: SpannerId::new(replica, 1),
                start: TimeAnchor::Region {
                    id: rid,
                    edge: RegionEdge::Start,
                    offset: AnchorOffset::WallClock(crate::time::WallClockDuration(5)),
                },
                end: TimeAnchor::WallClock {
                    time: WallClockTime(10),
                },
                staves: vec![staff],
            });
        }
        CrossCuttingRefsResolve => {
            // A slur pointing at an event that does not exist.
            let ghost = EventId::new(replica, 6_666_666);
            s.cross_cutting.slurs.push(Slur {
                id: SlurId::new(replica, 1),
                start_event: ghost,
                end_event: ghost,
            });
        }
        UniqueIdentifiers => {
            // Two staves with the same id.
            let dup = s.staves[0].clone();
            s.staves.push(dup);
        }
        PitchIdUnique => {
            // Reuse event 0's pitch id inside event 1's chord.
            let (e0, e1) = first_two_event_ids(&s);
            let pid = first_pitch_id(&s, e0);
            if let Some(Event::Pitched(p)) = s.events.get_mut(e1) {
                p.pitches.push(cmn_identified_pitch(pid, 0));
            }
        }
        SpellingScopeResolves => {
            // A spelling attachment targeting a pitch that is neither live nor
            // tombstoned.
            let ghost = PitchId::new(replica, 5_555_555);
            s.spelling_attachments.push(SpellingAttachment {
                scope: SpellingScope::Pitch(ghost),
                directive: SpellingDirective::Explicit(crate::pitch::PitchSpelling::cmn(
                    CmnNominal::C,
                    4,
                )),
                source: SpellingSource::UserChosen,
                priority: 0,
                layer: None,
            });
        }
        DecompositionTargetResolves => {
            // A decomposition attachment targeting a non-existent event.
            let ghost = EventId::new(replica, 4_444_444);
            s.decomposition_attachments.push(DecompositionAttachment {
                target: ghost,
                components: vec![NotatedComponent {
                    base_value: NoteValue::Whole,
                    dots: 0,
                    tuplet: None,
                    tied_to_next: false,
                }],
                source: DecompositionSource::Inferred,
            });
        }
        DecompositionSum => {
            // Components sum to the wrong total for a real event: the base
            // events are quarter notes (1/4), but a whole-note component is 1/1.
            let (e0, _) = first_two_event_ids(&s);
            s.decomposition_attachments.push(DecompositionAttachment {
                target: e0,
                components: vec![NotatedComponent {
                    base_value: NoteValue::Whole,
                    dots: 0,
                    tuplet: None,
                    tied_to_next: false,
                }],
                source: DecompositionSource::Inferred,
            });
        }
        TupletSum => {
            // A tuplet over two 1/4 events claiming a required total of 1/1.
            let (e0, e1) = first_two_event_ids(&s);
            s.cross_cutting.tuplets.push(Tuplet {
                id: TupletId::new(replica, 1),
                ratio: TupletRatio::new(3, 2).expect("3:2 is a valid tuplet ratio"),
                members: vec![e0, e1],
                parent: None,
                required_total: MusicalDuration::whole(),
            });
        }
        TiePairing => {
            // A tie pairing a pitch that is not in the start event.
            let (e0, e1) = first_two_event_ids(&s);
            let end_pid = first_pitch_id(&s, e1);
            let ghost = PitchId::new(replica, 3_333_333);
            s.cross_cutting.ties.push(Tie {
                id: TieId::new(replica, 1),
                start_event: e0,
                end_event: e1,
                pitch_pairing: Some(vec![(ghost, end_pid)]),
                class: TieClass::Editorial,
            });
        }
        VoiceOriginConsistent => {
            // A user-declared voice that uses the reserved SYSTEM_DERIVED
            // replica namespace.
            let bad = VoiceId::new(ReplicaId::SYSTEM_DERIVED, 1);
            let inst = &mut s.canvas.regions[0].content.staff_instances_mut().unwrap()[0];
            inst.voices.push(Voice::user(bad));
        }
        BarlineGroupSameRegion => {
            // A barline-alignment group referencing an instance outside the
            // region.
            let outside = StaffInstanceId::new(replica, 2_222_222);
            let gid = crate::ids::BarlineAlignmentGroupId::new(replica, 1);
            if let RegionContent::StaffBased(c) = &mut s.canvas.regions[0].content {
                c.barline_alignment_groups
                    .push(crate::graph::BarlineAlignmentGroup {
                        id: gid,
                        members: vec![crate::graph::BarlineAlignmentMember {
                            staff_instance: outside,
                            measure: crate::ids::MeasureId::new(replica, 1),
                            position: crate::time::MeasurePosition::Start,
                        }],
                    });
            }
        }
    }
    s
}

/// A coarse size metric: the number of structural elements in the graph.
pub fn element_count(s: &Score) -> usize {
    let mut n = s.staves.len()
        + s.canvas.regions.len()
        + s.events.len()
        + s.spelling_attachments.len()
        + s.decomposition_attachments.len()
        + s.cross_cutting.slurs.len()
        + s.cross_cutting.ties.len()
        + s.cross_cutting.beams.len()
        + s.cross_cutting.tuplets.len()
        + s.cross_cutting.spanners.len();
    for r in &s.canvas.regions {
        for si in r.staff_instances() {
            n += 1 + si.voices.len();
        }
    }
    n
}

/// Removes `events` from a voice's listing and from the arena, keeping the
/// arena/voice consistent during a shrink step.
fn drop_events(score: &mut Score, events: &[EventId]) {
    for e in events {
        score.events.remove(*e);
    }
    for r in &mut score.canvas.regions {
        if let Some(insts) = r.content.staff_instances_mut() {
            for si in insts.iter_mut() {
                for v in &mut si.voices {
                    v.events.retain(|x| !events.contains(x));
                }
            }
        }
    }
}

/// Produces one-element-smaller candidate clones of `score`, each kept
/// internally consistent (removing a container also prunes its arena events and
/// staff-extent entry), ordered cheapest-removal first.
fn shrink_candidates(score: &Score) -> Vec<Score> {
    let mut out = Vec::new();
    // Attachments and cross-cutting structures: free-standing, remove directly.
    for i in 0..score.spelling_attachments.len() {
        let mut c = score.clone();
        c.spelling_attachments.remove(i);
        out.push(c);
    }
    for i in 0..score.decomposition_attachments.len() {
        let mut c = score.clone();
        c.decomposition_attachments.remove(i);
        out.push(c);
    }
    macro_rules! shrink_vec {
        ($field:ident) => {
            for i in 0..score.cross_cutting.$field.len() {
                let mut c = score.clone();
                c.cross_cutting.$field.remove(i);
                out.push(c);
            }
        };
    }
    shrink_vec!(slurs);
    shrink_vec!(ties);
    shrink_vec!(beams);
    shrink_vec!(tuplets);
    shrink_vec!(spanners);

    // Individual events.
    for e in score.events.ids_canonical() {
        let mut c = score.clone();
        drop_events(&mut c, &[e]);
        out.push(c);
    }

    // Voices (with their events).
    for (ri, r) in score.canvas.regions.iter().enumerate() {
        for (si_ix, si) in r.staff_instances().iter().enumerate() {
            for (vi, v) in si.voices.iter().enumerate() {
                let mut c = score.clone();
                let evs = v.events.clone();
                drop_events(&mut c, &evs);
                if let Some(insts) = c.canvas.regions[ri].content.staff_instances_mut() {
                    insts[si_ix].voices.remove(vi);
                }
                out.push(c);
            }
        }
    }

    // Staff instances (with their events and staff-extent entry).
    for (ri, r) in score.canvas.regions.iter().enumerate() {
        for (si_ix, si) in r.staff_instances().iter().enumerate() {
            let mut c = score.clone();
            let evs: Vec<EventId> = si.voices.iter().flat_map(|v| v.events.clone()).collect();
            let staff = si.staff;
            drop_events(&mut c, &evs);
            if let Some(insts) = c.canvas.regions[ri].content.staff_instances_mut() {
                insts.remove(si_ix);
            }
            // Keep the staff-extent consistent unless another instance still
            // manifests the staff.
            let still = c.canvas.regions[ri]
                .staff_instances()
                .iter()
                .any(|x| x.staff == staff);
            if !still {
                c.canvas.regions[ri]
                    .staff_extent
                    .staves
                    .retain(|x| *x != staff);
            }
            out.push(c);
        }
    }

    // Whole regions (with their events).
    for (ri, r) in score.canvas.regions.iter().enumerate() {
        let mut c = score.clone();
        let evs: Vec<EventId> = r
            .staff_instances()
            .iter()
            .flat_map(|si| si.voices.iter().flat_map(|v| v.events.clone()))
            .collect();
        drop_events(&mut c, &evs);
        c.canvas.regions.remove(ri);
        out.push(c);
    }

    // Staves.
    for i in 0..score.staves.len() {
        let mut c = score.clone();
        c.staves.remove(i);
        out.push(c);
    }
    out
}

/// Shrinks a graph that violates `inv` to a small witness that still violates
/// it. Greedy: repeatedly takes the first one-element-smaller candidate that
/// still violates `inv`, to a fixpoint. Necessary structure is retained
/// because a removal that repairs the violation is never taken.
pub fn shrink(score: &Score, inv: GraphInvariant) -> Score {
    assert!(
        !check_invariant(score, inv).is_empty(),
        "shrink starting point must violate the target invariant"
    );
    let mut best = score.clone();
    loop {
        let mut improved = false;
        for cand in shrink_candidates(&best) {
            if !check_invariant(&cand, inv).is_empty() {
                best = cand;
                improved = true;
                break;
            }
        }
        if !improved {
            break;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_invariants;

    #[test]
    fn positive_corpus_runs_clean() {
        // The hand-off gate: the arbitrary-graph corpus is well-formed.
        for score in arbitrary_graph_corpus(500, 0xA11CE) {
            let v = check_invariants(&score);
            assert!(v.is_empty(), "valid graph reported violations: {v:?}");
        }
    }

    #[test]
    fn rich_corpus_runs_clean() {
        // The breadth corpus: concurrent metric/proportional/aleatoric regions,
        // measures, a triplet, a tie, a spanner, marker, chord symbol,
        // decomposition, and tombstones — all well-formed.
        for seed in 0..200u64 {
            let score = valid_score_rich(seed.wrapping_mul(0x9E37_79B9));
            let v = check_invariants(&score);
            assert!(v.is_empty(), "rich graph reported violations: {v:?}");
        }
        // Confirm it actually exercises the broad features (not silently empty).
        let s = valid_score_rich(1);
        assert_eq!(s.canvas.regions.len(), 3);
        assert_eq!(s.cross_cutting.tuplets.len(), 1);
        assert_eq!(s.cross_cutting.ties.len(), 1);
        assert_eq!(s.cross_cutting.spanners.len(), 1);
        assert_eq!(s.decomposition_attachments.len(), 1);
        assert!(!s.tombstoned_pitches.is_empty());
        assert!(s.events.len() >= 7);
    }

    #[test]
    fn every_invariant_has_a_negative_generator() {
        for inv in GraphInvariant::all() {
            let s = violating_score(inv, 0x1234_5678);
            let violations = check_invariant(&s, inv);
            assert!(
                !violations.is_empty(),
                "negative generator for {inv:?} did not violate it; full report: {:?}",
                check_invariants(&s)
            );
        }
    }

    #[test]
    fn every_invariant_shrinks_to_a_small_witness() {
        for inv in GraphInvariant::all() {
            let big = violating_score(inv, 0x0F0F_0F0F);
            let before = element_count(&big);
            let small = shrink(&big, inv);
            let after = element_count(&small);
            // Still violates the target.
            assert!(
                !check_invariant(&small, inv).is_empty(),
                "{inv:?}: shrunk witness no longer violates the invariant"
            );
            // Never grows, and the witness is genuinely small.
            assert!(after <= before, "{inv:?}: shrink grew the graph");
            assert!(
                after <= 16,
                "{inv:?}: witness not minimized enough ({after} elements)"
            );
        }
    }

    #[test]
    fn shrink_is_idempotent() {
        for inv in GraphInvariant::all() {
            let s = shrink(&violating_score(inv, 7), inv);
            let again = shrink(&s, inv);
            assert_eq!(
                element_count(&s),
                element_count(&again),
                "{inv:?}: shrink not at a fixpoint"
            );
        }
    }

    #[test]
    fn negative_generators_are_reasonably_targeted() {
        // Each corruption should be tightly scoped: at most a couple of
        // invariants fire, and the target is always among them. (Some
        // invariants overlap by construction — e.g. duplicating an id breaks
        // both uniqueness and the containment tree — so we allow a small set.)
        for inv in GraphInvariant::all() {
            let s = violating_score(inv, 99);
            let all = check_invariants(&s);
            let kinds: std::collections::BTreeSet<_> = all.iter().map(|v| v.invariant).collect();
            assert!(kinds.contains(&inv), "{inv:?} not among {kinds:?}");
            assert!(
                kinds.len() <= 3,
                "{inv:?} corruption fired too many invariants: {kinds:?}"
            );
        }
    }
}
