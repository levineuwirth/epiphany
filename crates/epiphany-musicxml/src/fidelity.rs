//! Comparing a reduced import back against its source (roadmap X1.4).
//!
//! The comparison reads the reduced score, not the emitted operations, and
//! keys what it compares by position and content rather than by the ids the
//! importer minted: per staff, the multiset of voices, each voice the ordered
//! list of its events' exact onsets, durations and contents. It also compares
//! clefs, keys, meters, measures, ties, slurs and each instrument's
//! transposition, and checks the reader against a raw count of the file's
//! `<note>` elements, and that every quarter-tone is spelt as it sounds. Tuplets are compared twice: the score's against the
//! reader's, and the reader's against the census's own timed walk of the
//! file, tuplet by tuplet, by the staff of the first note, the ratio and each
//! member's onset, so a reader grouping the wrong notes under the right counts
//! is a failure. The census times a member within its measure and takes the
//! measure's start from the reader, as its quarter-tones do.
//!
//! A difference is a failure unless the operation that should have produced
//! the missing thing was refused, in which case it is reported as explained by
//! that rejection, which the outcome report already lists.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    AnchorOffset, Event, EventDuration, EventId, EventPosition, Pitch, PitchSpacePosition,
    RationalTime, Score, SpellingDirective, SpellingNominal, SpellingScope, StaffGroupKind,
    StaffId, TimeAnchor, TimeSignatureDisplay, VoiceId,
};

use crate::emit::{Import, Subject};
use crate::outcome::Reduced;
use crate::source::{Content, GroupKind, QuarterTone, SourceEvent};

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
    /// Sorted [`pitch_key`]s.
    Pitched(Vec<(u8, i16, i8)>),
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

/// A pitch as compared: its nominal, its alteration in quarter-tones and its
/// octave, so that a `cmn-24` quarter-flat and a `cmn-12` flat, each an
/// alteration of `-1` in its own space, differ. A pitch in any other space,
/// which the importer never writes, compares as `(u8::MAX, 0, 0)`.
fn pitch_key(pitch: &Pitch) -> (u8, i16, i8) {
    let space = pitch.scale_position.space.as_str();
    match &pitch.scale_position.position {
        PitchSpacePosition::Cmn {
            nominal,
            alteration,
            octave,
        } if space == "cmn-12" || space == "cmn-24" => {
            let per_step = if space == "cmn-12" { 2 } else { 1 };
            (*nominal as u8, per_step * i16::from(*alteration), *octave)
        }
        _ => (u8::MAX, 0, 0),
    }
}

fn source_key(event: &SourceEvent) -> Key {
    let content = match &event.content {
        Content::Rest { visible } => ContentKey::Rest { visible: *visible },
        Content::Pitched(pitches) => {
            let mut triples: Vec<(u8, i16, i8)> =
                pitches.iter().map(|p| pitch_key(&p.pitch)).collect();
            triples.sort_unstable();
            ContentKey::Pitched(triples)
        }
        Content::Unpitched { step, member, .. } => ContentKey::Unpitched {
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
            let mut triples: Vec<(u8, i16, i8)> = pitched
                .pitches
                .iter()
                .map(|p| pitch_key(&p.pitch))
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

/// What a tie holds at its start: a pitch, by its [`pitch_key`], or an
/// unpitched note, by its staff step and member.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Tied {
    Pitch((u8, i16, i8)),
    Unpitched { step: i16, member: u32 },
}

/// A tied note as compared: its staff, the start's onset, what is tied, and
/// the end's onset.
type TiedNote = (StaffId, RationalTime, Tied, RationalTime);

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

    // A tie is emitted per end event, so a tied pitch's absence is explained
    // by the refusal of any tie from its start event.
    let refused_tie = |p: usize, i: usize| -> Option<String> {
        let (low, high) = (Subject::Tie(p, i, 0), Subject::Tie(p, i, usize::MAX));
        label_index
            .range::<&Subject, _>(&low..=&high)
            .find_map(|(subject, _)| refused(subject))
    };

    // The reader against the census's counts of the file's notes, kind by
    // kind, less the chord notes the census finds the model cannot hold; and
    // the reader's own count of the notes it dropped, against the census's.
    for (p, part) in source.parts.iter().enumerate() {
        let census = &source.census[p];
        let (mut pitched, mut unpitched, mut rests, mut extra) = (0, 0, 0, 0);
        for event in &part.events {
            match &event.content {
                Content::Rest { .. } => rests += 1,
                Content::Pitched(pitches) => {
                    pitched += pitches.len();
                    extra += pitches.len() - 1;
                }
                Content::Unpitched { .. } => unpitched += 1,
            }
        }
        let dropped = census.dropped;
        let all_dropped = dropped.pitched + dropped.unpitched + dropped.rests;
        if pitched + dropped.pitched != census.pitched
            || unpitched + dropped.unpitched != census.unpitched
            || rests + dropped.rests != census.rests
            || extra + all_dropped != census.chord_members
        {
            fidelity.failures.push(format!(
                "{}: the reader holds {pitched} pitched and {unpitched} unpitched notes, \
                 {rests} rests and {extra} chord members, but the file has {} pitched and {} \
                 unpitched notes, {} rests and {} chord members, of which {} pitched, {} \
                 unpitched and {} rests are chord notes the model cannot hold",
                part.name,
                census.pitched,
                census.unpitched,
                census.rests,
                census.chord_members,
                dropped.pitched,
                dropped.unpitched,
                dropped.rests
            ));
        }
        if part.dropped_notes != all_dropped {
            fidelity.failures.push(format!(
                "{}: the reader dropped and recorded {} chord notes, but the file has {} \
                 the model cannot hold",
                part.name, part.dropped_notes, all_dropped
            ));
        }
        if census.keys.len() != part.staves.len() {
            fidelity.failures.push(format!(
                "{}: the reader holds {} staves, the file's <staves> declare {}",
                part.name,
                part.staves.len(),
                census.keys.len()
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
            // The same keys and clefs held to the file's own elements, each
            // where the census finds it stated, which the reader's placement
            // of them cannot hide. A measure starts where the reader puts it.
            let census = &source.census[p];
            let at = |stated_measure: usize, offset: &RationalTime| {
                source
                    .measures
                    .get(stated_measure)
                    .map(|m| m.onset.add(offset))
            };
            let mut graph_fifths: Vec<(Option<RationalTime>, i8)> = instance
                .key_sequence
                .iter()
                .map(|k| (anchor_offset(&k.anchor), k.key.fifths()))
                .collect();
            let mut file_fifths: Vec<(Option<RationalTime>, i8)> =
                census.keys.get(s).map_or_else(Vec::new, |ks| {
                    ks.iter()
                        .map(|k| (at(k.measure, &k.offset), k.value))
                        .collect()
                });
            graph_fifths.sort_unstable();
            file_fifths.sort_unstable();
            if graph_fifths != file_fifths {
                fidelity.failures.push(format!(
                    "{label}: keys {graph_fifths:?} in the score, {file_fifths:?} in the file"
                ));
            }
            let clef_key = |c: &epiphany_core::Clef| (c.shape as u8, c.line, c.octave_shift);
            type ClefAt = (Option<RationalTime>, (u8, i8, i8));
            let mut graph_clefs: Vec<ClefAt> = instance
                .clef_sequence
                .iter()
                .map(|c| (anchor_offset(&c.anchor), clef_key(&c.clef)))
                .collect();
            let mut file_clefs: Vec<ClefAt> = census.clefs.get(s).map_or_else(Vec::new, |cs| {
                cs.iter()
                    .map(|c| (at(c.measure, &c.offset), clef_key(&c.value)))
                    .collect()
            });
            graph_clefs.sort_unstable();
            file_clefs.sort_unstable();
            if graph_clefs != file_clefs {
                fidelity.failures.push(format!(
                    "{label}: clefs {graph_clefs:?} in the score, {file_clefs:?} in the file"
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

        // Ties: (staff, start onset, what is tied, end onset) on each side.
        let mut graph_ties: BTreeMap<TiedNote, isize> = BTreeMap::new();
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
            match score.events.get(tie.start_event) {
                Some(Event::Pitched(first)) => {
                    for (a, _) in tie.pitch_pairing.clone().unwrap_or_default() {
                        if let Some(ip) = first.pitches.iter().find(|ip| ip.id == a) {
                            let tied = Tied::Pitch(pitch_key(&ip.pitch));
                            *graph_ties
                                .entry((*staff, start.clone(), tied, end.clone()))
                                .or_default() += 1;
                        }
                    }
                }
                Some(Event::Unpitched(u)) => {
                    let tied = Tied::Unpitched {
                        step: u.staff_position.0,
                        member: u.instrument_member.0,
                    };
                    *graph_ties
                        .entry((*staff, start.clone(), tied, end.clone()))
                        .or_default() += 1;
                }
                _ => fidelity
                    .failures
                    .push(format!("{name}: a tie starts on a rest")),
            }
        }
        let starts = crate::emit::event_starts(&part.events);
        let mut source_ties = BTreeMap::new();
        let mut tie_explained = Vec::new();
        for (i, event) in part.events.iter().enumerate() {
            let end = event.onset.add(&event.duration);
            let at_end = starts
                .get(&(event.staff, &end))
                .map_or(&[][..], Vec::as_slice);
            if let Content::Unpitched {
                step,
                member,
                tie_start: true,
                ..
            } = &event.content
            {
                let ends = at_end.iter().any(|&j| {
                    matches!(&part.events[j].content, Content::Unpitched {
                        step: s, member: m, tie_stop: true, ..
                    } if s == step && m == member)
                });
                if !ends {
                    continue; // recorded by the importer as a tie without an end
                }
                if let Some(why) = refused_tie(p, i) {
                    tie_explained.push(format!("{name}: tie at {} ({why})", show(&event.onset)));
                    continue;
                }
                let tied = Tied::Unpitched {
                    step: *step,
                    member: *member as u32,
                };
                *source_ties
                    .entry((
                        import.ids.staves[p][event.staff],
                        event.onset.clone(),
                        tied,
                        end.clone(),
                    ))
                    .or_default() += 1;
                continue;
            }
            let Content::Pitched(pitches) = &event.content else {
                continue;
            };
            for pitch in pitches.iter().filter(|x| x.tie_start) {
                let ends = at_end.iter().map(|&j| &part.events[j]).any(|next| {
                    matches!(&next.content, Content::Pitched(ps)
                            if ps.iter().any(|y| y.tie_stop && y.pitch.scale_position == pitch.pitch.scale_position))
                });
                if !ends {
                    continue; // recorded by the importer as a tie without an end
                }
                if !crate::emit::tieable(&pitch.pitch) {
                    continue; // recorded by the importer as a tie on a quarter-tone
                }
                if let Some(why) = refused_tie(p, i) {
                    tie_explained.push(format!("{name}: tie at {} ({why})", show(&event.onset)));
                    continue;
                }
                *source_ties
                    .entry((
                        import.ids.staves[p][event.staff],
                        event.onset.clone(),
                        Tied::Pitch(pitch_key(&pitch.pitch)),
                        end.clone(),
                    ))
                    .or_default() += 1;
            }
        }
        if graph_ties != source_ties {
            fidelity.failures.push(format!(
                "{name}: {} tied notes in the score, {} in the source",
                graph_ties.values().sum::<isize>(),
                source_ties.values().sum::<isize>()
            ));
        }
        // And held to the census's count of the file's tie starts, taken
        // apart from the reader: each is tied in the score, explained by a
        // refused tie, or recorded by the importer as without an end, on a
        // quarter-tone, or on a note the reader dropped; and each of those
        // three records to the census's own count of its kind.
        let census = &source.census[p];
        let tied: isize = graph_ties.values().sum();
        let unended = import.unended_ties.get(p).copied().unwrap_or(0);
        let quarter = import.quarter_tone_ties.get(p).copied().unwrap_or(0);
        let (refused_ties, dropped) = (tie_explained.len(), part.dropped_tie_starts);
        if tied.unsigned_abs() + refused_ties + unended + quarter + dropped != census.tie_starts
            || unended != census.unended_ties
            || quarter != census.quarter_tone_ties
            || dropped != census.dropped_tie_starts
        {
            fidelity.failures.push(format!(
                "{name}: {tied} tie starts tied in the score and {refused_ties} refused; \
                 {unended} recorded without an end, {quarter} on quarter-tones and {dropped} on \
                 dropped notes; but the file has {} tie starts, {} without an end, {} on \
                 quarter-tones and {} on chord notes the model cannot hold",
                census.tie_starts,
                census.unended_ties,
                census.quarter_tone_ties,
                census.dropped_tie_starts
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

        // Beams: (staff, each event's onset) on each side, and the reader's
        // beams made and recorded unmade, each held to the census's own.
        let mut graph_beams: BTreeMap<(StaffId, Vec<RationalTime>), isize> = BTreeMap::new();
        for beam in &score.cross_cutting.beams {
            let places: Option<Vec<&(StaffId, RationalTime)>> =
                beam.events.iter().map(|e| event_place.get(e)).collect();
            let Some(places) = places else {
                continue;
            };
            let Some(&&(staff, _)) = places.first() else {
                continue;
            };
            if !import.ids.staves[p].contains(&staff) {
                continue;
            }
            let onsets = places.iter().map(|(_, onset)| onset.clone()).collect();
            *graph_beams.entry((staff, onsets)).or_default() += 1;
        }
        let mut source_beams: BTreeMap<(StaffId, Vec<RationalTime>), isize> = BTreeMap::new();
        for (k, beam) in part.beams.iter().enumerate() {
            let Some(&first) = beam.events.first() else {
                continue;
            };
            if let Some(why) = refused(&Subject::Beam(p, k)) {
                fidelity.explained.push(format!(
                    "{name}: beam at {} ({why})",
                    show(&part.events[first].onset)
                ));
                continue;
            }
            let staff = import.ids.staves[p][part.events[first].staff];
            let onsets = beam
                .events
                .iter()
                .map(|&i| part.events[i].onset.clone())
                .collect();
            *source_beams.entry((staff, onsets)).or_default() += 1;
        }
        if graph_beams != source_beams {
            fidelity.failures.push(format!(
                "{name}: {} beams in the score, {} in the source",
                graph_beams.values().sum::<isize>(),
                source_beams.values().sum::<isize>()
            ));
        }
        if part.beams.len() != census.beams || part.unmade_beams != census.unmade_beams {
            fidelity.failures.push(format!(
                "{name}: the reader made {} beams and recorded {} unmade, but the file \
                 makes {} and leaves {} unmade",
                part.beams.len(),
                part.unmade_beams,
                census.beams,
                census.unmade_beams
            ));
        }

        // Tuplets: (staff, each member's onset, ratio) on each side, and the
        // reader's tuplets made, by ratio, and recorded unmade, each held to
        // the census's own walk of the file.
        type TupletKey = (StaffId, Vec<RationalTime>, (u32, u32));
        let mut graph_tuplets: BTreeMap<TupletKey, isize> = BTreeMap::new();
        for tuplet in &score.cross_cutting.tuplets {
            let places: Option<Vec<&(StaffId, RationalTime)>> =
                tuplet.members.iter().map(|e| event_place.get(e)).collect();
            let Some(places) = places else {
                continue;
            };
            let Some(&&(staff, _)) = places.first() else {
                continue;
            };
            if !import.ids.staves[p].contains(&staff) {
                continue;
            }
            let onsets = places.iter().map(|(_, onset)| onset.clone()).collect();
            let ratio = (tuplet.ratio.actual(), tuplet.ratio.notated());
            *graph_tuplets.entry((staff, onsets, ratio)).or_default() += 1;
        }
        let mut source_tuplets: BTreeMap<TupletKey, isize> = BTreeMap::new();
        let mut made: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for (k, tuplet) in part.tuplets.iter().enumerate() {
            *made.entry((tuplet.actual, tuplet.normal)).or_default() += 1;
            let Some(&first) = tuplet.events.first() else {
                continue;
            };
            if let Some(why) = refused(&Subject::Tuplet(p, k)) {
                fidelity.explained.push(format!(
                    "{name}: tuplet {}:{} at {} ({why})",
                    tuplet.actual,
                    tuplet.normal,
                    show(&part.events[first].onset)
                ));
                continue;
            }
            let staff = import.ids.staves[p][part.events[first].staff];
            let onsets = tuplet
                .events
                .iter()
                .map(|&i| part.events[i].onset.clone())
                .collect();
            *source_tuplets
                .entry((staff, onsets, (tuplet.actual, tuplet.normal)))
                .or_default() += 1;
        }
        if graph_tuplets != source_tuplets {
            fidelity.failures.push(format!(
                "{name}: {} tuplets in the score, {} in the source",
                graph_tuplets.values().sum::<isize>(),
                source_tuplets.values().sum::<isize>()
            ));
        }
        // Every tuplet the reader made, refused or not, held to the census's
        // own timed walk: the staff of its first note, its ratio and each
        // member's onset. A reader that grouped other notes under the same
        // counts and ratios differs here. The census times a member within
        // its measure; the measure's start is the reader's.
        type PlacedTuplet = (usize, Vec<RationalTime>, (u32, u32));
        let mut file_tuplets: BTreeMap<PlacedTuplet, usize> = BTreeMap::new();
        for tuplet in &census.tuplet_places {
            let onsets = tuplet
                .members
                .iter()
                .map(|(m, offset)| {
                    source
                        .measures
                        .get(*m)
                        .map_or_else(|| RationalTime::from_int(-1), |m| m.onset.add(offset))
                })
                .collect();
            *file_tuplets
                .entry((tuplet.staff, onsets, tuplet.ratio))
                .or_default() += 1;
        }
        let mut read_tuplets: BTreeMap<PlacedTuplet, usize> = BTreeMap::new();
        for tuplet in &part.tuplets {
            let Some(&first) = tuplet.events.first() else {
                continue;
            };
            let onsets = tuplet
                .events
                .iter()
                .map(|&i| part.events[i].onset.clone())
                .collect();
            *read_tuplets
                .entry((
                    part.events[first].staff,
                    onsets,
                    (tuplet.actual, tuplet.normal),
                ))
                .or_default() += 1;
        }
        if read_tuplets != file_tuplets {
            let alone = |a: &BTreeMap<PlacedTuplet, usize>, b: &BTreeMap<PlacedTuplet, usize>| {
                a.iter().find(|(at, n)| b.get(*at) != Some(n)).map_or_else(
                    || String::from("none"),
                    |((staff, onsets, (actual, normal)), _)| {
                        format!(
                            "{actual}:{normal} on staff {} with {} members from {}",
                            staff + 1,
                            onsets.len(),
                            onsets.first().map_or_else(|| String::from("?"), show)
                        )
                    },
                )
            };
            fidelity.failures.push(format!(
                "{name}: the reader's tuplets are not the file's; first in the file alone: {}; \
                 first read alone: {}",
                alone(&file_tuplets, &read_tuplets),
                alone(&read_tuplets, &file_tuplets),
            ));
        }
        if made != census.tuplets || part.unmade_tuplets != census.unmade_tuplets {
            fidelity.failures.push(format!(
                "{name}: the reader made {} tuplets ({:?}) and recorded {} unmade, but the \
                 file makes {} ({:?}) and leaves {} unmade",
                part.tuplets.len(),
                made,
                part.unmade_tuplets,
                census.tuplets.values().sum::<usize>(),
                census.tuplets,
                census.unmade_tuplets
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

        // Quarter-tones, each held at its onset, staff and sounding pitch to
        // the census's reading of the file's `<alter>`, `<accidental>` and
        // `<transpose>`, apart from the reader: those the score holds, those
        // of refused events, and the dropped notes that were.
        let in_file = |q: &QuarterTone| -> SoundingAt {
            let onset = source
                .measures
                .get(q.measure)
                .map_or_else(|| RationalTime::from_int(-1), |m| m.onset.add(&q.offset));
            (onset, q.staff, (q.nominal, q.quarter_tones, q.octave))
        };
        let mut file_side: BTreeMap<SoundingAt, usize> = BTreeMap::new();
        for q in &census.quarter_tones {
            *file_side.entry(in_file(q)).or_default() += 1;
        }
        let mut score_side: BTreeMap<SoundingAt, usize> = BTreeMap::new();
        let mut held = 0;
        for pl in placed.iter().filter(|pl| staves.contains(&pl.staff)) {
            let ContentKey::Pitched(pitches) = &pl.key.content else {
                continue;
            };
            let staff = import.ids.staves[p].iter().position(|s| *s == pl.staff);
            for pitch in pitches.iter().filter(|k| k.1 % 2 != 0) {
                held += 1;
                let at = (pl.key.onset.clone(), staff.unwrap_or(usize::MAX), *pitch);
                *score_side.entry(at).or_default() += 1;
            }
        }
        let mut refused_quarter_tones = 0;
        for (i, event) in part.events.iter().enumerate() {
            let ContentKey::Pitched(pitches) = source_key(event).content else {
                continue;
            };
            if refused(&Subject::Event(p, i)).is_none() {
                continue;
            }
            for pitch in pitches.into_iter().filter(|k| k.1 % 2 != 0) {
                refused_quarter_tones += 1;
                let at = (event.onset.clone(), event.staff, pitch);
                *score_side.entry(at).or_default() += 1;
            }
        }
        for q in &part.dropped_quarter_tones {
            *score_side.entry(in_file(q)).or_default() += 1;
        }
        if score_side != file_side {
            let alone = |a: &BTreeMap<SoundingAt, usize>, b: &BTreeMap<SoundingAt, usize>| {
                a.iter()
                    .find(|(at, n)| b.get(*at) != Some(n))
                    .map_or_else(|| String::from("none"), |(at, _)| show_sounding(at))
            };
            fidelity.failures.push(format!(
                "{name}: {held} quarter-tones in the score ({refused_quarter_tones} more refused, \
                 {} dropped and recorded) against the {} the file makes; first in the file \
                 alone: {}; first in the score alone: {}",
                part.dropped_quarter_tones.len(),
                census.quarter_tones.len(),
                alone(&file_side, &score_side),
                alone(&score_side, &file_side),
            ));
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

    // Staff groups: each group's kind and staves, the score's against the
    // reader's, and the reader's groups made and unmade against the census's.
    let kind_of = |kind: &StaffGroupKind| -> &'static str {
        match kind {
            StaffGroupKind::GrandStaff => "brace",
            StaffGroupKind::Bracket => "bracket",
            StaffGroupKind::SubBracket => "sub-bracket",
            StaffGroupKind::Choral => "choral",
            StaffGroupKind::Registered(_) => "registered",
        }
    };
    let mut graph_groups: Vec<(&str, Vec<StaffId>)> = score
        .staff_groups
        .iter()
        .map(|g| {
            let mut members = g.members.clone();
            members.sort();
            (kind_of(&g.kind), members)
        })
        .collect();
    let mut source_groups: Vec<(&str, Vec<StaffId>)> = Vec::new();
    for (k, group) in source.groups.iter().enumerate() {
        if let Some(why) = refused(&Subject::Group(k)) {
            fidelity.explained.push(format!("staff group {k} ({why})"));
            continue;
        }
        let mut members: Vec<StaffId> = group
            .staves
            .iter()
            .map(|&(p, s)| import.ids.staves[p][s])
            .collect();
        members.sort();
        let kind = match group.kind {
            GroupKind::Brace => "brace",
            GroupKind::Bracket => "bracket",
            GroupKind::SubBracket => "sub-bracket",
        };
        source_groups.push((kind, members));
    }
    graph_groups.sort();
    source_groups.sort();
    if graph_groups != source_groups {
        let sizes = |groups: &[(&str, Vec<StaffId>)]| -> Vec<String> {
            groups
                .iter()
                .map(|(kind, members)| format!("{kind} of {}", members.len()))
                .collect()
        };
        fidelity.failures.push(format!(
            "staff groups {:?} in the score, {:?} in the source (by kind and staves)",
            sizes(&graph_groups),
            sizes(&source_groups)
        ));
    }
    let census = source.group_census;
    let mut made = [0usize; 3];
    for group in &source.groups {
        made[group.kind as usize] += 1;
    }
    if made != census.made || source.unmade_groups != census.unmade {
        fidelity.failures.push(format!(
            "the reader made {made:?} staff groups (braces, brackets, sub-brackets) and \
             recorded {} unmade, but the file makes {:?} and leaves {} unmade",
            source.unmade_groups, census.made, census.unmade
        ));
    }

    // Every quarter-tone the score holds is spelt as it sounds: an authored
    // spelling of its letter and octave whose one accidental alters the
    // letter by its alteration, valued here by the accidental's own name.
    let spellings: BTreeMap<_, _> = score
        .spelling_attachments
        .iter()
        .filter(|a| a.layer.is_none())
        .filter_map(|a| match (&a.scope, &a.directive) {
            (SpellingScope::Pitch(pitch), SpellingDirective::Explicit(spelling)) => {
                Some((*pitch, spelling))
            }
            _ => None,
        })
        .collect();
    let (mut quarter_tones, mut misspelt) = (0usize, Vec::new());
    for event in score.events.iter() {
        let Event::Pitched(event) = event else {
            continue;
        };
        for ip in &event.pitches {
            let PitchSpacePosition::Cmn {
                nominal,
                alteration,
                octave,
            } = ip.pitch.scale_position.position
            else {
                continue;
            };
            if ip.pitch.scale_position.space.as_str() != "cmn-24" || alteration % 2 == 0 {
                continue;
            }
            quarter_tones += 1;
            let agrees = spellings.get(&ip.id).is_some_and(|spelling| {
                spelling.nominal == SpellingNominal::Cmn(nominal)
                    && spelling.octave == octave
                    && matches!(spelling.accidentals.as_slice(),
                        [only] if accidental_quarter_tones(only.as_str()) == Some(alteration))
            });
            if !agrees {
                misspelt.push(format!("{nominal:?}{alteration:+}q{octave}"));
            }
        }
    }
    if !misspelt.is_empty() {
        fidelity.failures.push(format!(
            "{} of {quarter_tones} quarter-tones are not spelt as they sound, the first {}",
            misspelt.len(),
            misspelt[0]
        ));
    }
    fidelity
}

/// The alteration in quarter-tones a quarter-tone accidental gives its
/// letter, from its MusicXML name: Stein's by name, an arrowed one as the
/// accidental under the arrow, a quarter-tone up or down. Kept apart from the
/// reader's tables, so a reader that spells a quarter-tone with the wrong
/// accidental differs here.
fn accidental_quarter_tones(name: &str) -> Option<i8> {
    match name {
        "quarter-flat" => return Some(-1),
        "quarter-sharp" => return Some(1),
        "three-quarters-flat" => return Some(-3),
        "three-quarters-sharp" => return Some(3),
        _ => {}
    }
    let (under, arrow) = match name.rsplit_once('-')? {
        (under, "up") => (under, 1),
        (under, "down") => (under, -1),
        _ => return None,
    };
    let semitones: i8 = match under {
        "flat-flat" => -2,
        "flat" => -1,
        "natural" => 0,
        "sharp" => 1,
        "double-sharp" => 2,
        _ => return None,
    };
    Some(2 * semitones + arrow)
}

/// A quarter-tone as compared: its onset, its staff within the part, and its
/// sounding pitch by [`pitch_key`].
type SoundingAt = (RationalTime, usize, (u8, i16, i8));

/// A quarter-tone as `G-1q4 at 5/4 on staff 1`, its alteration in signed
/// quarter-tones.
fn show_sounding((onset, staff, (nominal, quarter_tones, octave)): &SoundingAt) -> String {
    let letter = b"CDEFGAB"
        .get(usize::from(*nominal))
        .map_or('?', |c| char::from(*c));
    format!(
        "{letter}{quarter_tones:+}q{octave} at {} on staff {}",
        show(onset),
        staff.wrapping_add(1)
    )
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
