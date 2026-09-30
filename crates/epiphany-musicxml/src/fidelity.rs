//! Comparing a reduced import back against its source (roadmap X1.4).
//!
//! The comparison reads the reduced score, not the emitted operations, and
//! keys what it compares by position and content rather than by the ids the
//! importer minted: per staff, the multiset of voices, each voice the ordered
//! list of its events' exact onsets, durations and contents. It also compares
//! clefs, keys, meters, measures, ties, slurs and each instrument's
//! transposition, and checks the reader against a raw count of the file's
//! `<note>` elements.
//!
//! A difference is a failure unless the operation that should have produced
//! the missing thing was refused, in which case it is reported as explained by
//! that rejection, which the outcome report already lists.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    AnchorOffset, Event, EventDuration, EventId, EventPosition, PitchSpacePosition, RationalTime,
    Score, StaffId, TimeAnchor, TimeSignatureDisplay, VoiceId,
};

use crate::emit::{Import, Subject};
use crate::outcome::Reduced;
use crate::source::{Content, SourceEvent};

/// Counts of one part in one measure.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Counts {
    /// Sounding notes: each pitch of a chord, and each unpitched note.
    pub notes: usize,
    pub rests: usize,
    /// Events with two or more pitches.
    pub chords: usize,
    /// Voices with an event starting in the measure.
    pub voices: usize,
    /// Staves with an event starting in the measure.
    pub staves: usize,
}

impl std::ops::AddAssign for Counts {
    fn add_assign(&mut self, other: Counts) {
        self.notes += other.notes;
        self.rests += other.rests;
        self.chords += other.chords;
        self.voices += other.voices;
        self.staves += other.staves;
    }
}

/// The result of the comparison.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Fidelity {
    /// Differences no rejected operation explains. Any entry is a failure.
    pub failures: Vec<String>,
    /// Differences whose producing operation was refused.
    pub explained: Vec<String>,
    /// Per part, per measure: the counts read from the reduced score.
    pub counts: Vec<Vec<Counts>>,
    /// Per part: every event the reduced score holds for it.
    pub events: Vec<usize>,
}

impl Fidelity {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// An event as compared: onset, duration and content.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Key {
    onset: RationalTime,
    duration: RationalTime,
    content: ContentKey,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum ContentKey {
    Rest {
        visible: bool,
    },
    /// Sorted `(nominal, alteration, octave)` triples.
    Pitched(Vec<(u8, i8, i8)>),
    Unpitched {
        step: i16,
        member: u32,
    },
    /// A kind the importer never emits.
    Other(String),
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} for {}: {:?}",
            show(&self.onset),
            show(&self.duration),
            self.content
        )
    }
}

/// A rational as `n/d` (or `n`).
pub fn show(time: &RationalTime) -> String {
    match time {
        RationalTime::Small(s) if s.denominator() == 1 => s.numerator().to_string(),
        RationalTime::Small(s) => format!("{}/{}", s.numerator(), s.denominator()),
        RationalTime::Large(big) => big.to_string(),
    }
}

fn source_key(event: &SourceEvent) -> Key {
    let content = match &event.content {
        Content::Rest { visible } => ContentKey::Rest { visible: *visible },
        Content::Pitched(pitches) => {
            let mut triples: Vec<(u8, i8, i8)> = pitches
                .iter()
                .filter_map(|p| match &p.pitch.scale_position.position {
                    PitchSpacePosition::Cmn {
                        nominal,
                        alteration,
                        octave,
                    } => Some((*nominal as u8, *alteration, *octave)),
                    _ => None,
                })
                .collect();
            triples.sort_unstable();
            ContentKey::Pitched(triples)
        }
        Content::Unpitched { step, member } => ContentKey::Unpitched {
            step: *step,
            member: *member as u32,
        },
    };
    Key {
        onset: event.onset.clone(),
        duration: event.duration.clone(),
        content,
    }
}

fn graph_key(event: &Event) -> Key {
    let onset = match event.position() {
        EventPosition::Musical(p) => p.0.clone(),
        EventPosition::WallClock(_) => RationalTime::from_int(-1),
    };
    let duration = match event.duration() {
        EventDuration::Musical(d) => d.0.clone(),
        _ => RationalTime::from_int(-1),
    };
    let content = match event {
        Event::Rest(rest) => ContentKey::Rest {
            visible: rest.visible,
        },
        Event::Pitched(pitched) => {
            let mut triples: Vec<(u8, i8, i8)> = pitched
                .pitches
                .iter()
                .filter_map(|p| match &p.pitch.scale_position.position {
                    PitchSpacePosition::Cmn {
                        nominal,
                        alteration,
                        octave,
                    } => Some((*nominal as u8, *alteration, *octave)),
                    _ => None,
                })
                .collect();
            triples.sort_unstable();
            ContentKey::Pitched(triples)
        }
        Event::Unpitched(u) => ContentKey::Unpitched {
            step: u.staff_position.0,
            member: u.instrument_member.0,
        },
        other => ContentKey::Other(format!("{other:?}")),
    };
    Key {
        onset,
        duration,
        content,
    }
}

fn anchor_offset(anchor: &TimeAnchor) -> Option<RationalTime> {
    match anchor {
        TimeAnchor::Region {
            offset: AnchorOffset::Musical(d),
            ..
        } => Some(d.0.clone()),
        TimeAnchor::Region {
            offset: AnchorOffset::Zero,
            ..
        } => Some(RationalTime::zero()),
        _ => None,
    }
}

/// A tied pitch as compared: its staff, the start's onset, the pitch as
/// `(nominal, alteration, octave)`, and the end's onset.
type TiedPitch = (StaffId, RationalTime, (u8, i8, i8), RationalTime);

/// Where a graph event sits: its staff, its voice, and its key.
struct Placed {
    staff: StaffId,
    voice: VoiceId,
    key: Key,
}

/// Compares the reduced score of `import` against its source.
pub fn compare(import: &Import, reduced: &Reduced) -> Fidelity {
    let source = &import.source;
    let score: &Score = &reduced.score;
    let mut fidelity = Fidelity::default();
    let label_index: BTreeMap<&Subject, usize> = import
        .labels
        .iter()
        .enumerate()
        .map(|(i, l)| (&l.subject, i))
        .collect();
    let refused = |subject: &Subject| -> Option<String> {
        let index = *label_index.get(subject)?;
        let verdict = &reduced.verdicts[index];
        (!verdict.applied()).then(|| {
            format!(
                "{} #{index} {}: {}",
                import.labels[index].kind,
                verdict.class(),
                verdict.reason()
            )
        })
    };

    // The reader against the raw element count.
    for (p, part) in source.parts.iter().enumerate() {
        let census = source.census[p];
        let dropped = part.dropped_notes;
        let (mut notes, mut rests, mut extra) = (0, 0, 0);
        for event in &part.events {
            match &event.content {
                Content::Rest { .. } => rests += 1,
                Content::Pitched(pitches) => {
                    notes += pitches.len();
                    extra += pitches.len() - 1;
                }
                Content::Unpitched { .. } => notes += 1,
            }
        }
        if notes + dropped != census.pitched + census.unpitched
            || rests != census.rests
            || extra + dropped != census.chord_members
        {
            fidelity.failures.push(format!(
                "{}: the reader holds {notes} notes, {rests} rests and {extra} chord members \
                 ({dropped} dropped and recorded), but the file has {} pitched and {} unpitched \
                 notes, {} rests and {} chord members",
                part.name, census.pitched, census.unpitched, census.rests, census.chord_members
            ));
        }
    }

    // Every graph event, placed by staff and voice.
    let mut placed: Vec<Placed> = Vec::new();
    let mut event_place: BTreeMap<EventId, (StaffId, RationalTime)> = BTreeMap::new();
    let mut instances_by_staff: BTreeMap<StaffId, Vec<&epiphany_core::StaffInstance>> =
        BTreeMap::new();
    for region in &score.canvas.regions {
        for instance in region.staff_instances() {
            instances_by_staff
                .entry(instance.staff)
                .or_default()
                .push(instance);
            for voice in &instance.voices {
                for id in &voice.events {
                    match score.events.get(*id) {
                        Some(event) => {
                            let key = graph_key(event);
                            event_place.insert(*id, (instance.staff, key.onset.clone()));
                            placed.push(Placed {
                                staff: instance.staff,
                                voice: voice.id,
                                key,
                            });
                        }
                        None => fidelity
                            .failures
                            .push(format!("voice {:?} lists a missing event {id:?}", voice.id)),
                    }
                }
            }
        }
    }

    let staff_of: BTreeMap<StaffId, (usize, usize)> = import
        .ids
        .staves
        .iter()
        .enumerate()
        .flat_map(|(p, staves)| staves.iter().enumerate().map(move |(s, id)| (*id, (p, s))))
        .collect();
    for staff in &score.staves {
        if !staff_of.contains_key(&staff.id) {
            fidelity.failures.push(format!(
                "the score has a staff {:?} the source lacks",
                staff.id
            ));
        }
    }

    for (p, part) in source.parts.iter().enumerate() {
        let name = &part.name;
        // The instrument and its transposition.
        let instrument = score
            .staves
            .iter()
            .find(|st| Some(&st.id) == import.ids.staves[p].first())
            .and_then(|st| score.instruments.iter().find(|i| i.id == st.instrument));
        match instrument {
            None => match refused(&Subject::Part(p)) {
                Some(why) => fidelity
                    .explained
                    .push(format!("{name}: no instrument ({why})")),
                None => fidelity.failures.push(format!("{name}: no instrument")),
            },
            Some(instrument) if instrument.transposition != part.transposition => {
                fidelity.failures.push(format!(
                    "{name}: transposition {:?}, the source's {:?}",
                    instrument.transposition, part.transposition
                ));
            }
            Some(_) => {}
        }

        for (s, source_staff) in part.staves.iter().enumerate() {
            let staff_id = import.ids.staves[p][s];
            let label = format!("{name}, staff {}", s + 1);
            let instances = instances_by_staff
                .get(&staff_id)
                .cloned()
                .unwrap_or_default();
            if instances.len() != 1 {
                let why = refused(&Subject::Staff(p, s));
                let line = format!("{label}: {} staff instances, not 1", instances.len());
                match why {
                    Some(why) => fidelity.explained.push(format!("{line} ({why})")),
                    None => fidelity.failures.push(line),
                }
                continue;
            }
            let instance = instances[0];

            // Voices: the multiset of ordered event lists.
            let mut source_voices: BTreeMap<&str, Vec<(Key, usize)>> = BTreeMap::new();
            for (i, event) in part.events.iter().enumerate() {
                if event.staff == s {
                    source_voices
                        .entry(event.voice.as_str())
                        .or_default()
                        .push((source_key(event), i));
                }
            }
            let mut graph_voices: BTreeMap<VoiceId, Vec<Key>> = BTreeMap::new();
            for pl in placed.iter().filter(|pl| pl.staff == staff_id) {
                graph_voices
                    .entry(pl.voice)
                    .or_default()
                    .push(pl.key.clone());
            }
            let sorted = |mut v: Vec<Vec<Key>>| {
                for list in &mut v {
                    list.sort();
                }
                v.sort();
                v
            };
            let graph_side = sorted(graph_voices.into_values().collect());
            let source_side = sorted(
                source_voices
                    .values()
                    .map(|v| v.iter().map(|(k, _)| k.clone()).collect())
                    .collect(),
            );
            if graph_side != source_side {
                // Drop the source events whose insertion was refused, and
                // compare again: what remains different is unexplained.
                let mut explained = Vec::new();
                let kept: Vec<Vec<Key>> = source_voices
                    .values()
                    .map(|v| {
                        v.iter()
                            .filter(|(k, i)| match refused(&Subject::Event(p, *i)) {
                                Some(why) => {
                                    explained.push(format!("{label}: event {k} absent ({why})"));
                                    false
                                }
                                None => true,
                            })
                            .map(|(k, _)| k.clone())
                            .collect()
                    })
                    .filter(|v: &Vec<Key>| !v.is_empty())
                    .collect();
                if sorted(kept) == graph_side {
                    fidelity.explained.extend(explained);
                } else {
                    let flat = |side: &Vec<Vec<Key>>| -> BTreeMap<Key, isize> {
                        let mut m = BTreeMap::new();
                        for k in side.iter().flatten() {
                            *m.entry(k.clone()).or_default() += 1;
                        }
                        m
                    };
                    let (g, src) = (flat(&graph_side), flat(&source_side));
                    let mut differences = Vec::new();
                    for key in g.keys().chain(src.keys()).collect::<BTreeSet<_>>() {
                        let delta =
                            g.get(key).copied().unwrap_or(0) - src.get(key).copied().unwrap_or(0);
                        if delta != 0 {
                            differences.push(format!("{key} ({delta:+})"));
                        }
                    }
                    if differences.is_empty() {
                        fidelity.failures.push(format!(
                            "{label}: the events agree but their voices differ \
                             ({} voices in the score, {} in the source)",
                            graph_side.len(),
                            source_side.len()
                        ));
                    } else {
                        let shown = differences.len().min(6);
                        fidelity.failures.push(format!(
                            "{label}: {} events differ (score minus source): {}{}",
                            differences.len(),
                            differences[..shown].join("; "),
                            if differences.len() > shown {
                                "; …"
                            } else {
                                ""
                            }
                        ));
                    }
                }
            }

            // Clefs and keys.
            let graph_clefs: Vec<(Option<RationalTime>, epiphany_core::Clef)> = instance
                .clef_sequence
                .iter()
                .map(|c| (anchor_offset(&c.anchor), c.clef))
                .collect();
            let source_clefs: Vec<(Option<RationalTime>, epiphany_core::Clef)> = source_staff
                .clefs
                .iter()
                .map(|c| (Some(c.onset.clone()), c.clef))
                .collect();
            if graph_clefs != source_clefs {
                fidelity.failures.push(format!(
                    "{label}: clefs {graph_clefs:?}, the source's {source_clefs:?}"
                ));
            }
            let graph_keys: Vec<(Option<RationalTime>, i8)> = instance
                .key_sequence
                .iter()
                .map(|k| (anchor_offset(&k.anchor), k.key.fifths()))
                .collect();
            let source_keys: Vec<(Option<RationalTime>, i8)> = source_staff
                .keys
                .iter()
                .map(|k| (Some(k.onset.clone()), k.fifths))
                .collect();
            if graph_keys != source_keys {
                fidelity.failures.push(format!(
                    "{label}: keys {graph_keys:?}, the source's {source_keys:?}"
                ));
            }

            // Measures.
            let graph_starts: BTreeSet<RationalTime> = instance
                .measures
                .iter()
                .filter_map(|m| anchor_offset(&m.start))
                .collect();
            for (m, measure) in source.measures.iter().enumerate() {
                if !graph_starts.contains(&measure.onset) {
                    let line = format!("{label}: measure {} absent", measure.number);
                    match refused(&Subject::Measure(p, s, m)) {
                        Some(why) => fidelity.explained.push(format!("{line} ({why})")),
                        None => fidelity.failures.push(line),
                    }
                }
            }
            if instance.measures.len() > source.measures.len() {
                fidelity.failures.push(format!(
                    "{label}: {} measures, the source's {}",
                    instance.measures.len(),
                    source.measures.len()
                ));
            }
        }

        // Ties: (staff, start onset, pitch, end onset) on each side.
        let mut graph_ties: BTreeMap<TiedPitch, isize> = BTreeMap::new();
        for tie in &score.cross_cutting.ties {
            let (Some((staff, start)), Some((_, end))) = (
                event_place.get(&tie.start_event),
                event_place.get(&tie.end_event),
            ) else {
                continue;
            };
            if !import.ids.staves[p].contains(staff) {
                continue;
            }
            let Some(Event::Pitched(first)) = score.events.get(tie.start_event) else {
                fidelity
                    .failures
                    .push(format!("{name}: a tie starts on a non-pitched event"));
                continue;
            };
            for (a, _) in tie.pitch_pairing.clone().unwrap_or_default() {
                if let Some(ip) = first.pitches.iter().find(|ip| ip.id == a) {
                    if let PitchSpacePosition::Cmn {
                        nominal,
                        alteration,
                        octave,
                    } = &ip.pitch.scale_position.position
                    {
                        *graph_ties
                            .entry((
                                *staff,
                                start.clone(),
                                (*nominal as u8, *alteration, *octave),
                                end.clone(),
                            ))
                            .or_default() += 1;
                    }
                }
            }
        }
        let mut source_ties = BTreeMap::new();
        let mut tie_explained = Vec::new();
        for (i, event) in part.events.iter().enumerate() {
            let Content::Pitched(pitches) = &event.content else {
                continue;
            };
            let end = event.onset.add(&event.duration);
            for pitch in pitches.iter().filter(|x| x.tie_start) {
                let ends = part.events.iter().any(|next| {
                    next.staff == event.staff
                        && next.onset == end
                        && matches!(&next.content, Content::Pitched(ps)
                            if ps.iter().any(|y| y.tie_stop && y.pitch.scale_position == pitch.pitch.scale_position))
                });
                if !ends {
                    continue; // recorded by the importer as a tie without an end
                }
                if let Some(why) = refused(&Subject::Tie(p, i)) {
                    tie_explained.push(format!("{name}: tie at {} ({why})", show(&event.onset)));
                    continue;
                }
                if let PitchSpacePosition::Cmn {
                    nominal,
                    alteration,
                    octave,
                } = &pitch.pitch.scale_position.position
                {
                    *source_ties
                        .entry((
                            import.ids.staves[p][event.staff],
                            event.onset.clone(),
                            (*nominal as u8, *alteration, *octave),
                            end.clone(),
                        ))
                        .or_default() += 1;
                }
            }
        }
        if graph_ties != source_ties {
            fidelity.failures.push(format!(
                "{name}: {} tied pitches in the score, {} in the source",
                graph_ties.values().sum::<isize>(),
                source_ties.values().sum::<isize>()
            ));
        }
        fidelity.explained.extend(tie_explained);

        // Slurs: (start staff, start onset, end staff, end onset).
        let mut graph_slurs: BTreeMap<(StaffId, RationalTime, StaffId, RationalTime), isize> =
            BTreeMap::new();
        for slur in &score.cross_cutting.slurs {
            if let (Some(a), Some(b)) = (
                event_place.get(&slur.start_event),
                event_place.get(&slur.end_event),
            ) {
                if import.ids.staves[p].contains(&a.0) {
                    *graph_slurs
                        .entry((a.0, a.1.clone(), b.0, b.1.clone()))
                        .or_default() += 1;
                }
            }
        }
        let mut source_slurs = BTreeMap::new();
        for (k, slur) in part.slurs.iter().enumerate() {
            if let Some(why) = refused(&Subject::Slur(p, k)) {
                fidelity.explained.push(format!(
                    "{name}: slur at {} ({why})",
                    show(&part.events[slur.start].onset)
                ));
                continue;
            }
            let (a, b) = (&part.events[slur.start], &part.events[slur.end]);
            *source_slurs
                .entry((
                    import.ids.staves[p][a.staff],
                    a.onset.clone(),
                    import.ids.staves[p][b.staff],
                    b.onset.clone(),
                ))
                .or_default() += 1;
        }
        if graph_slurs != source_slurs {
            fidelity.failures.push(format!(
                "{name}: {} slurs in the score, {} in the source",
                graph_slurs.values().sum::<isize>(),
                source_slurs.values().sum::<isize>()
            ));
        }

        // Counts per measure, from the score and from the source.
        let measure_of = |onset: &RationalTime| -> usize {
            source
                .measures
                .partition_point(|m| &m.onset <= onset)
                .saturating_sub(1)
        };
        let staves: BTreeSet<StaffId> = import.ids.staves[p].iter().copied().collect();
        let mut counts = vec![Counts::default(); source.measures.len()];
        let mut voices: Vec<BTreeSet<VoiceId>> = vec![BTreeSet::new(); source.measures.len()];
        let mut staff_sets: Vec<BTreeSet<StaffId>> = vec![BTreeSet::new(); source.measures.len()];
        let mut total = 0;
        for pl in placed.iter().filter(|pl| staves.contains(&pl.staff)) {
            total += 1;
            let m = measure_of(&pl.key.onset);
            if m >= counts.len() {
                continue;
            }
            count_into(&mut counts[m], &pl.key.content);
            voices[m].insert(pl.voice);
            staff_sets[m].insert(pl.staff);
        }
        for m in 0..counts.len() {
            counts[m].voices = voices[m].len();
            counts[m].staves = staff_sets[m].len();
        }
        let mut expected = vec![Counts::default(); source.measures.len()];
        let mut source_voices: Vec<BTreeSet<(usize, &str)>> =
            vec![BTreeSet::new(); source.measures.len()];
        let mut source_staves: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); source.measures.len()];
        for (i, event) in part.events.iter().enumerate() {
            if refused(&Subject::Event(p, i)).is_some() {
                continue;
            }
            let m = measure_of(&event.onset);
            count_into(&mut expected[m], &source_key(event).content);
            source_voices[m].insert((event.staff, event.voice.as_str()));
            source_staves[m].insert(event.staff);
        }
        let mut differing = Vec::new();
        for m in 0..expected.len() {
            expected[m].voices = source_voices[m].len();
            expected[m].staves = source_staves[m].len();
            if expected[m] != counts[m] {
                differing.push(m);
            }
        }
        if let Some(&m) = differing.first() {
            fidelity.failures.push(format!(
                "{name}: counts differ in {} measures; measure {}: {:?}, the source's {:?}",
                differing.len(),
                source.measures[m].number,
                counts[m],
                expected[m]
            ));
        }
        fidelity.counts.push(counts);
        fidelity.events.push(total);
    }

    // Meters.
    let region = import
        .ids
        .region
        .and_then(|id| score.canvas.regions.iter().find(|r| r.id == id));
    let graph_meters: Vec<(Option<RationalTime>, RationalTime, String)> = region
        .and_then(|r| r.content.staff_based())
        .and_then(|c| c.default_metric_grid.as_ref())
        .map(|grid| {
            grid.meter_sequence
                .iter()
                .map(|change| {
                    let signature = score
                        .time_signatures
                        .iter()
                        .find(|t| t.id == change.time_signature);
                    (
                        anchor_offset(&change.anchor),
                        signature.map_or(RationalTime::from_int(-1), |t| {
                            t.measure_duration().0.clone()
                        }),
                        signature.map_or(String::from("?"), |t| display(&t.display)),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let source_meters: Vec<(Option<RationalTime>, RationalTime, String)> = source
        .meters
        .iter()
        .enumerate()
        .filter(|(m, _)| refused(&Subject::Meter(*m)).is_none())
        .map(|(_, change)| {
            let numerators: Vec<String> =
                change.meter.numerators.iter().map(u16::to_string).collect();
            (
                Some(change.onset.clone()),
                change.meter.measure_length(),
                format!("{}/{}", numerators.join("+"), change.meter.denominator),
            )
        })
        .collect();
    if graph_meters != source_meters {
        fidelity.failures.push(format!(
            "meters {graph_meters:?}, the source's {source_meters:?}"
        ));
    }
    fidelity
}

fn count_into(counts: &mut Counts, content: &ContentKey) {
    match content {
        ContentKey::Rest { .. } => counts.rests += 1,
        ContentKey::Pitched(pitches) => {
            counts.notes += pitches.len();
            if pitches.len() > 1 {
                counts.chords += 1;
            }
        }
        ContentKey::Unpitched { .. } => counts.notes += 1,
        ContentKey::Other(_) => {}
    }
}

fn display(display: &TimeSignatureDisplay) -> String {
    match display {
        TimeSignatureDisplay::Standard {
            numerator,
            denominator,
        } => format!("{numerator}/{}", denominator.get()),
        TimeSignatureDisplay::Compound {
            numerators,
            denominator,
        } => {
            let n: Vec<String> = numerators.iter().map(u16::to_string).collect();
            format!("{}/{}", n.join("+"), denominator.get())
        }
        TimeSignatureDisplay::Irrational {
            numerator,
            denominator,
        } => format!("{numerator}/{denominator}"),
        other => format!("{other:?}"),
    }
}
