//! Emitting operations for a [`SourceScore`].
//!
//! The score is built from empty by operations alone, through the public
//! operation API, never by assembling a `Score`: instruments, staves, one
//! metric region, its meter changes, staff instances (carrying their clef and
//! key sequences), voices, measures, events, then ties and slurs. Every
//! operation comes from one replica, numbered contiguously, each carrying a
//! causal context that covers every operation before it: one author's history.

use std::collections::BTreeMap;

use epiphany_core::{
    AnchorOffset, Beam, BeamId, BeatGroup, Clef, ClefChange, Event, EventDuration, EventId,
    EventPosition, ForeignFormatId, IdentifiedPitch, IdentityContext, Instrument, InstrumentId,
    KeySignature, KeySignatureChange, Measure, MeasureId, MeasureNumberVisibility, MetricTimeModel,
    MusicalDuration, MusicalPosition, OperationId, PitchId, PitchedEvent, PowerOfTwo, RationalTime,
    Region, RegionContent, RegionEdge, RegionId, RegionTimeModel, ReplicaId, Rest, ScoreMetadata,
    Slur, SlurId, SlurKind, Staff, StaffExtent, StaffId, StaffInstance, StaffInstanceId,
    StaffLineConfiguration, StaffPosition, StemConfiguration, Tie, TieClass, TieId, TimeAnchor,
    TimeExtent, TimeSignature, TimeSignatureDisplay, TimeSignatureId, Timestamp, UnpitchedEvent,
    UnpitchedMember, UnpitchedMemberId, Voice, VoiceId, VoiceOrigin, WallClockTime,
};
use epiphany_ops::{
    AuthorId, CausalContext, CreateCrossCuttingOp, CreateInstrumentOp, CreateMeasureOp,
    CreateRegionOp, CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp, CrossCuttingValue,
    HybridLogicalClock, InsertEventOp, OperationEnvelope, OperationKind, OperationPayload,
    OperationStamp, SetMetadataOp, SetTimeSignatureOp,
};

use crate::source::{Content, FeatureClass, Meter, Place, SourceScore};

/// The replica an import authors from unless told otherwise.
pub const DEFAULT_REPLICA: ReplicaId = ReplicaId(0x6D75_7369_6378_6D6C);

/// What an emitted operation is about, for reporting its outcome.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Subject {
    Score,
    /// A part, by index.
    Part(usize),
    /// A staff: part and staff index.
    Staff(usize, usize),
    /// A meter change, by index into [`SourceScore::meters`].
    Meter(usize),
    /// A voice: part, staff and the file's voice.
    Voice(usize, usize, String),
    /// A measure on a staff: part, staff, measure index.
    Measure(usize, usize, usize),
    /// An event: part and index into its events.
    Event(usize, usize),
    /// A tie: part, and the indices of its start and end events.
    Tie(usize, usize, usize),
    /// A slur: part and index into its slurs.
    Slur(usize, usize),
    /// A beam: part and index into its beams.
    Beam(usize, usize),
}

/// What one emitted operation carries.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Label {
    /// The operation kind, as named in the catalog.
    pub kind: &'static str,
    pub subject: Subject,
}

/// The identifiers the import minted, by source position.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Ids {
    pub region: Option<RegionId>,
    pub instruments: Vec<InstrumentId>,
    /// Per part, per staff.
    pub staves: Vec<Vec<StaffId>>,
    /// Per part, per staff.
    pub instances: Vec<Vec<StaffInstanceId>>,
    /// Keyed by part, staff and the file's voice.
    pub voices: BTreeMap<(usize, usize, String), VoiceId>,
    /// Per part, per source event.
    pub events: Vec<Vec<EventId>>,
    /// Per part, per source event, per pitch.
    pub pitches: Vec<Vec<Vec<PitchId>>>,
    /// Per meter change.
    pub time_signatures: Vec<TimeSignatureId>,
    /// Per part, per staff, per measure.
    pub measures: Vec<Vec<Vec<MeasureId>>>,
}

/// A read score and the operations that build it.
#[derive(Clone, Debug)]
pub struct Import {
    pub source: SourceScore,
    pub replica: ReplicaId,
    pub envelopes: Vec<OperationEnvelope>,
    /// One per envelope, in the same order.
    pub labels: Vec<Label>,
    pub ids: Ids,
    /// Per part: the tie starts recorded instead of tied for want of a
    /// matching end.
    pub unended_ties: Vec<usize>,
    /// Per part: the tie starts recorded instead of tied because the core
    /// cannot tie quarter-tones.
    pub quarter_tone_ties: Vec<usize>,
}

struct Emitter {
    replica: ReplicaId,
    identity: IdentityContext,
    envelopes: Vec<OperationEnvelope>,
    labels: Vec<Label>,
}

impl Emitter {
    fn emit(&mut self, kind: &'static str, subject: Subject, op: OperationKind) {
        let counter = self.envelopes.len() as u64;
        let id = OperationId::new(self.replica, counter);
        let causal_context = match counter {
            0 => CausalContext::new(),
            n => CausalContext::new().with_seen(self.replica, n - 1),
        };
        self.envelopes.push(OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(counter as i64), 0),
                id,
            ),
            causal_context,
            transaction: None,
            payload: OperationPayload::Primitive(op),
        });
        self.labels.push(Label { kind, subject });
    }
}

/// A position `offset` whole notes after the region's start.
pub fn region_anchor(region: RegionId, offset: &RationalTime) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge: RegionEdge::Start,
        offset: AnchorOffset::Musical(MusicalDuration(offset.clone())),
    }
}

/// The time signature a meter maps to: a standard display over a power-of-two
/// denominator (compound for an additive numerator like `3+2`), otherwise an
/// irrational one; beat groups of the additive parts, or of dotted beats for
/// 6, 9 and 12 over 8 or shorter, or else one per beat.
pub fn time_signature(id: TimeSignatureId, meter: &Meter) -> Option<TimeSignature> {
    let denominator = i64::from(meter.denominator);
    let unit = RationalTime::new(1, denominator)?;
    let group = |beats: u16, accent: u8| BeatGroup {
        duration: MusicalDuration(
            RationalTime::new(i64::from(beats), denominator).expect("a nonzero denominator"),
        ),
        subdivision: Some(MusicalDuration(unit.clone())),
        accent,
    };
    let beat_groups: Vec<BeatGroup> = if meter.numerators.len() > 1 {
        meter
            .numerators
            .iter()
            .enumerate()
            .map(|(i, &n)| group(n, u8::from(i == 0)))
            .collect()
    } else {
        let n = meter.numerators[0];
        if meter.denominator >= 8 && n > 3 && n % 3 == 0 {
            (0..n / 3).map(|i| group(3, u8::from(i == 0))).collect()
        } else {
            (0..n).map(|i| group(1, u8::from(i == 0))).collect()
        }
    };
    let display = match (PowerOfTwo::new(meter.denominator), meter.numerators.len()) {
        (Some(denominator), 1) => TimeSignatureDisplay::Standard {
            numerator: meter.numerators[0],
            denominator,
        },
        (Some(denominator), _) => TimeSignatureDisplay::Compound {
            numerators: meter.numerators.clone(),
            denominator,
        },
        (None, _) => TimeSignatureDisplay::Irrational {
            numerator: meter.numerators.iter().sum(),
            denominator: std::num::NonZeroU16::new(meter.denominator)?,
        },
    };
    TimeSignature::new(
        id,
        display,
        MusicalDuration(meter.measure_length()),
        beat_groups,
    )
}

fn staff_lines(lines: Option<u8>) -> StaffLineConfiguration {
    StaffLineConfiguration {
        line_count: lines.unwrap_or(5),
        ..StaffLineConfiguration::default()
    }
}

/// Whether the model can tie `pitch` to its equal: a tie's paired pitches
/// must be enharmonically equivalent, which the core answers only in a
/// twelve-chromatic space, so not for a quarter-tone in `cmn-24`.
pub(crate) fn tieable(pitch: &epiphany_core::Pitch) -> bool {
    pitch.enharmonic_equivalent(pitch)
}

/// The events of a part by staff and onset, each list in source order.
pub(crate) fn event_starts(
    events: &[crate::source::SourceEvent],
) -> BTreeMap<(usize, &RationalTime), Vec<usize>> {
    let mut starts: BTreeMap<(usize, &RationalTime), Vec<usize>> = BTreeMap::new();
    for (i, event) in events.iter().enumerate() {
        starts
            .entry((event.staff, &event.onset))
            .or_default()
            .push(i);
    }
    starts
}

/// Emits the operations that build `source` from an empty score.
pub fn emit(mut source: SourceScore, replica: ReplicaId) -> Import {
    let mut e = Emitter {
        replica,
        identity: IdentityContext::new(replica),
        envelopes: Vec::new(),
        labels: Vec::new(),
    };
    let mut ids = Ids::default();

    if source.title.is_some() || source.composer.is_some() {
        e.emit(
            "SetMetadata",
            Subject::Score,
            OperationKind::SetMetadata(SetMetadataOp {
                metadata: ScoreMetadata {
                    title: source.title.clone(),
                    composer: source.composer.clone(),
                    copyright: None,
                    subtitle: None,
                    lyricist: None,
                    arranger: None,
                    creation_timestamp: Timestamp(0),
                    modification_timestamp: Timestamp(0),
                    additional: Vec::new(),
                },
            }),
        );
    }

    // Instruments and their staves.
    for (p, part) in source.parts.iter().enumerate() {
        let instrument_id: InstrumentId = e.identity.mint();
        let first_clef = part
            .staves
            .first()
            .and_then(|s| s.clefs.first())
            .map_or(Clef::treble(), |c| c.clef);
        let mut instrument = Instrument::new(instrument_id, part.name.clone());
        instrument.abbreviation = part.abbreviation.clone();
        instrument.transposition = part.transposition;
        instrument.default_clef = first_clef;
        instrument.default_staff_lines = staff_lines(part.staves.first().and_then(|s| s.lines));
        instrument.unpitched_members = part
            .members
            .iter()
            .enumerate()
            .map(|(m, member)| UnpitchedMember {
                member: UnpitchedMemberId(m as u32),
                name: member.name.clone(),
                staff_position: StaffPosition(member.step),
            })
            .collect();
        e.emit(
            "CreateInstrument",
            Subject::Part(p),
            OperationKind::CreateInstrument(CreateInstrumentOp { instrument }),
        );
        ids.instruments.push(instrument_id);
        let mut staves = Vec::new();
        for (s, staff) in part.staves.iter().enumerate() {
            let staff_id: StaffId = e.identity.mint();
            e.emit(
                "CreateStaff",
                Subject::Staff(p, s),
                OperationKind::CreateStaff(CreateStaffOp {
                    staff: Staff {
                        id: staff_id,
                        name: part.name.clone(),
                        abbreviation: part.abbreviation.clone(),
                        instrument: instrument_id,
                        default_staff_lines: staff_lines(staff.lines),
                        group: None,
                        default_clef: staff.clefs.first().map_or(Clef::treble(), |c| c.clef),
                    },
                }),
            );
            staves.push(staff_id);
        }
        ids.staves.push(staves);
    }

    // One metric region holds the whole score.
    let region_id: RegionId = e.identity.mint();
    ids.region = Some(region_id);
    e.emit(
        "CreateRegion",
        Subject::Score,
        OperationKind::CreateRegion(CreateRegionOp {
            region: Region {
                id: region_id,
                time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
                content: RegionContent::StaffBased(Default::default()),
                time_extent: TimeExtent {
                    start: TimeAnchor::WallClock {
                        time: WallClockTime(0),
                    },
                    end: TimeAnchor::WallClock {
                        time: WallClockTime(1),
                    },
                },
                staff_extent: StaffExtent { staves: Vec::new() },
                local_tempo_map: None,
                permits_spanning_slurs: false,
            },
        }),
    );

    // Meter changes, before any measure they govern.
    let mut signature_at: BTreeMap<usize, TimeSignatureId> = BTreeMap::new();
    for (m, change) in source.meters.iter().enumerate() {
        let id: TimeSignatureId = e.identity.mint();
        ids.time_signatures.push(id);
        match time_signature(id, &change.meter) {
            Some(signature) => {
                if change.onset == source.measures[change.measure].onset {
                    signature_at.insert(change.measure, id);
                }
                e.emit(
                    "SetTimeSignature",
                    Subject::Meter(m),
                    OperationKind::SetTimeSignature(SetTimeSignatureOp {
                        region: region_id,
                        anchor: region_anchor(region_id, &change.onset),
                        time_signature: Some(signature),
                    }),
                );
            }
            None => source.features.record(
                FeatureClass::Content,
                "meter with no time signature",
                Place {
                    part: String::from("(score)"),
                    measure: source.measures[change.measure].number.clone(),
                },
            ),
        }
    }

    // Staff instances with their clefs and keys, then voices.
    for (p, part) in source.parts.iter().enumerate() {
        let mut instances = Vec::new();
        for (s, staff) in part.staves.iter().enumerate() {
            let instance_id: StaffInstanceId = e.identity.mint();
            let mut instance = StaffInstance::new(instance_id, ids.staves[p][s]);
            instance.clef_sequence = staff
                .clefs
                .iter()
                .map(|c| ClefChange {
                    anchor: region_anchor(region_id, &c.onset),
                    clef: c.clef,
                })
                .collect();
            instance.key_sequence = staff
                .keys
                .iter()
                .map(|k| KeySignatureChange {
                    anchor: region_anchor(region_id, &k.onset),
                    key: KeySignature::new(k.fifths).expect("the reader keeps fifths in -7..=7"),
                })
                .collect();
            e.emit(
                "CreateStaffInstance",
                Subject::Staff(p, s),
                OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                    region: region_id,
                    instance,
                }),
            );
            instances.push(instance_id);

            let mut voices: Vec<&str> = part
                .events
                .iter()
                .filter(|ev| ev.staff == s)
                .map(|ev| ev.voice.as_str())
                .collect();
            voices.sort_unstable();
            voices.dedup();
            for (v, voice) in voices.iter().enumerate() {
                let voice_id: VoiceId = e.identity.mint();
                ids.voices.insert((p, s, (*voice).to_owned()), voice_id);
                e.emit(
                    "CreateVoice",
                    Subject::Voice(p, s, (*voice).to_owned()),
                    OperationKind::CreateVoice(CreateVoiceOp {
                        staff_instance: instance_id,
                        voice: Voice {
                            id: voice_id,
                            events: Vec::new(),
                            default_stem_direction: None,
                            is_primary: v == 0,
                            origin: VoiceOrigin::Imported {
                                format: ForeignFormatId::new("musicxml"),
                            },
                        },
                    }),
                );
            }
        }
        ids.instances.push(instances);
    }

    // Measures, on every staff.
    for (p, part) in source.parts.iter().enumerate() {
        let mut per_staff = Vec::new();
        for s in 0..part.staves.len() {
            let mut measures = Vec::new();
            for (m, measure) in source.measures.iter().enumerate() {
                let measure_id: MeasureId = e.identity.mint();
                e.emit(
                    "CreateMeasure",
                    Subject::Measure(p, s, m),
                    OperationKind::CreateMeasure(CreateMeasureOp {
                        instance: ids.instances[p][s],
                        measure: Measure {
                            id: measure_id,
                            start: region_anchor(region_id, &measure.onset),
                            time_signature: signature_at.get(&m).copied(),
                            explicit_number: measure.number.parse().ok(),
                            number_visibility: MeasureNumberVisibility::Auto,
                        },
                    }),
                );
                measures.push(measure_id);
            }
            per_staff.push(measures);
        }
        ids.measures.push(per_staff);
    }

    // Events, part by part in source order.
    for (p, part) in source.parts.iter().enumerate() {
        let mut event_ids = Vec::with_capacity(part.events.len());
        let mut pitch_ids = Vec::with_capacity(part.events.len());
        for (i, event) in part.events.iter().enumerate() {
            let id: EventId = e.identity.mint();
            let voice = ids.voices[&(p, event.staff, event.voice.clone())];
            let position = EventPosition::Musical(MusicalPosition(event.onset.clone()));
            let duration = EventDuration::Musical(MusicalDuration(event.duration.clone()));
            let mut minted = Vec::new();
            let value = match &event.content {
                Content::Rest { visible } => Event::Rest(Rest {
                    id,
                    voice,
                    position,
                    duration,
                    vertical_position: None,
                    visible: *visible,
                }),
                Content::Pitched(pitches) => Event::Pitched(PitchedEvent {
                    id,
                    voice,
                    position,
                    duration,
                    pitches: pitches
                        .iter()
                        .map(|p| {
                            let pid: PitchId = e.identity.mint();
                            minted.push(pid);
                            IdentifiedPitch {
                                id: pid,
                                pitch: p.pitch.clone(),
                            }
                        })
                        .collect(),
                    articulations: Vec::new(),
                    dynamic: None,
                    ornaments: Vec::new(),
                    stem: StemConfiguration,
                    grace: None,
                }),
                Content::Unpitched { step, member, .. } => Event::Unpitched(UnpitchedEvent {
                    id,
                    voice,
                    position,
                    duration,
                    staff_position: StaffPosition(*step),
                    instrument_member: UnpitchedMemberId(*member as u32),
                    articulations: Vec::new(),
                    dynamic: None,
                    stem: StemConfiguration,
                    grace: None,
                }),
            };
            e.emit(
                "InsertEvent",
                Subject::Event(p, i),
                OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: ids.instances[p][event.staff],
                    event: value,
                }),
            );
            event_ids.push(id);
            pitch_ids.push(minted);
        }
        ids.events.push(event_ids);
        ids.pitches.push(pitch_ids);
    }

    // Ties: each tied pitch continues into an event that starts where it
    // ends on its staff, holding the same pitch with a tie stop: in its own
    // voice when one does, else in another, since a chord's notes may part
    // into voices across a tie. One tie per end event. A tied unpitched note
    // continues into one of the same member at the same staff step; its tie
    // pairs no pitch, which the model admits, having none to pair.
    let mut unended_ties = vec![0; source.parts.len()];
    let mut quarter_tone_ties = vec![0; source.parts.len()];
    for (p, part) in source.parts.iter().enumerate() {
        let starts = event_starts(&part.events);
        for (i, event) in part.events.iter().enumerate() {
            if let Content::Unpitched {
                step,
                member,
                tie_start: true,
                ..
            } = &event.content
            {
                let end = event.onset.add(&event.duration);
                let at_end = starts
                    .get(&(event.staff, &end))
                    .map_or(&[][..], Vec::as_slice);
                let continues = |j: &&usize| {
                    matches!(&part.events[**j].content, Content::Unpitched {
                        step: s, member: m, tie_stop: true, ..
                    } if s == step && m == member)
                };
                let found = at_end
                    .iter()
                    .filter(continues)
                    .find(|&&j| part.events[j].voice == event.voice)
                    .or_else(|| at_end.iter().find(continues));
                match found {
                    Some(&j) => {
                        let class = if part.events[j].voice == event.voice {
                            TieClass::Standard
                        } else {
                            TieClass::CrossVoice
                        };
                        let tie_id: TieId = e.identity.mint();
                        e.emit(
                            "CreateCrossCutting(Tie)",
                            Subject::Tie(p, i, j),
                            OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                                structure: CrossCuttingValue::Tie(Tie {
                                    id: tie_id,
                                    start_event: ids.events[p][i],
                                    end_event: ids.events[p][j],
                                    pitch_pairing: None,
                                    class,
                                    style: Default::default(),
                                }),
                            }),
                        );
                    }
                    None => {
                        unended_ties[p] += 1;
                        source.features.record(
                            FeatureClass::Content,
                            "tie without a matching end",
                            Place {
                                part: part.name.clone(),
                                measure: source.measures[event.measure].number.clone(),
                            },
                        );
                    }
                }
                continue;
            }
            let Content::Pitched(pitches) = &event.content else {
                continue;
            };
            if !pitches.iter().any(|x| x.tie_start) {
                continue;
            }
            let end = event.onset.add(&event.duration);
            let at_end = starts
                .get(&(event.staff, &end))
                .map_or(&[][..], Vec::as_slice);
            let mut ends: BTreeMap<usize, Vec<(PitchId, PitchId)>> = BTreeMap::new();
            let mut used: BTreeMap<usize, Vec<bool>> = BTreeMap::new();
            for (a, x) in pitches.iter().enumerate().filter(|(_, x)| x.tie_start) {
                let found = [true, false].iter().find_map(|&same_voice| {
                    at_end.iter().find_map(|&j| {
                        let next = &part.events[j];
                        if (next.voice == event.voice) != same_voice {
                            return None;
                        }
                        let Content::Pitched(next_pitches) = &next.content else {
                            return None;
                        };
                        let taken = used.get(&j);
                        next_pitches
                            .iter()
                            .enumerate()
                            .position(|(b, y)| {
                                !taken.is_some_and(|t| t[b])
                                    && y.tie_stop
                                    && y.pitch.scale_position == x.pitch.scale_position
                            })
                            .map(|b| (j, b, next_pitches.len()))
                    })
                });
                match found {
                    Some((j, b, len)) => {
                        used.entry(j).or_insert_with(|| vec![false; len])[b] = true;
                        if tieable(&x.pitch) {
                            ends.entry(j)
                                .or_default()
                                .push((ids.pitches[p][i][a], ids.pitches[p][j][b]));
                        } else {
                            quarter_tone_ties[p] += 1;
                            let measure = source.measures[event.measure].number.clone();
                            source.features.record(
                                FeatureClass::Content,
                                "tie on a quarter-tone pitch",
                                Place {
                                    part: part.name.clone(),
                                    measure,
                                },
                            );
                        }
                    }
                    None => {
                        unended_ties[p] += 1;
                        let measure = source.measures[event.measure].number.clone();
                        source.features.record(
                            FeatureClass::Content,
                            "tie without a matching end",
                            Place {
                                part: part.name.clone(),
                                measure,
                            },
                        );
                    }
                }
            }
            for (j, pairing) in ends {
                let class = if part.events[j].voice == event.voice {
                    TieClass::Standard
                } else {
                    TieClass::CrossVoice
                };
                let tie_id: TieId = e.identity.mint();
                e.emit(
                    "CreateCrossCutting(Tie)",
                    Subject::Tie(p, i, j),
                    OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                        structure: CrossCuttingValue::Tie(Tie {
                            id: tie_id,
                            start_event: ids.events[p][i],
                            end_event: ids.events[p][j],
                            pitch_pairing: Some(pairing),
                            class,
                            style: Default::default(),
                        }),
                    }),
                );
            }
        }
        for (k, slur) in part.slurs.iter().enumerate() {
            let slur_id: SlurId = e.identity.mint();
            e.emit(
                "CreateCrossCutting(Slur)",
                Subject::Slur(p, k),
                OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Slur(Slur {
                        id: slur_id,
                        start_event: ids.events[p][slur.start],
                        end_event: ids.events[p][slur.end],
                        kind: SlurKind::Legato,
                        curvature_override: None,
                        style: Default::default(),
                    }),
                }),
            );
        }
        for (k, beam) in part.beams.iter().enumerate() {
            let beam_id: BeamId = e.identity.mint();
            e.emit(
                "CreateCrossCutting(Beam)",
                Subject::Beam(p, k),
                OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Beam(Beam {
                        id: beam_id,
                        events: beam.events.iter().map(|&i| ids.events[p][i]).collect(),
                        level: 1,
                        sub_beams: Vec::new(),
                        geometry_override: None,
                    }),
                }),
            );
        }
    }

    Import {
        source,
        replica,
        envelopes: e.envelopes,
        labels: e.labels,
        ids,
        unended_ties,
        quarter_tone_ties,
    }
}
