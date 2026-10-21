//! Note entry across the barline: an entry that would cross a barline is split
//! there and tied, and an entry past the last measure opens the next one on every
//! staff, as MuseScore enters notes.

use epiphany_bundle::MemStore;
use epiphany_core::{
    Clef, CmnNominal, Event, EventDuration, EventId, EventPosition, MusicalDuration,
    MusicalPosition, RationalTime, ReplicaId, Score, VoiceId,
};
use epiphany_editor_core::{EditorDocument, EditorSession, ScoreSetup, StaffSetup};
use epiphany_layout_ir::StubSolver;

fn whole(n: i64, d: i64) -> MusicalDuration {
    MusicalDuration(RationalTime::new(n, d).expect("a valid duration"))
}

fn at(n: i64, d: i64) -> MusicalPosition {
    MusicalPosition(RationalTime::new(n, d).expect("a valid position"))
}

/// A session over a new score of `staves` treble staves in 4/4 with `measures`
/// measures.
fn session(staves: usize, measures: u32) -> EditorSession {
    let mut setup = ScoreSetup::single_staff("Violin", measures);
    setup.staves = (0..staves)
        .map(|i| StaffSetup {
            name: format!("Violin {}", i + 1),
            clef: Clef::treble(),
        })
        .collect();
    let operations = setup.operations(ReplicaId::generate()).unwrap();
    let mut document = EditorDocument::create(MemStore::new(), operations).unwrap();
    document.lease(Box::new(StubSolver)).unwrap()
}

/// The first staff's voice.
fn first_voice(session: &EditorSession) -> VoiceId {
    session.score().voices().next().expect("a voice").2.id
}

/// The events of `voice` as (onset, duration, pitched), in time order.
fn events(score: &Score, voice: VoiceId) -> Vec<(MusicalPosition, MusicalDuration, bool)> {
    let mut out: Vec<_> = score
        .events
        .iter()
        .filter(|e| e.voice() == voice)
        .filter_map(|event| match (event.position(), event.duration()) {
            (EventPosition::Musical(p), EventDuration::Musical(d)) => {
                Some((p.clone(), d.clone(), matches!(event, Event::Pitched(_))))
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The ties of the score as (start onset, end onset).
fn ties(score: &Score) -> Vec<(MusicalPosition, MusicalPosition)> {
    let onset = |id| match score.events.get(id).map(Event::position) {
        Some(EventPosition::Musical(p)) => p.clone(),
        other => panic!("a tied event with a musical onset, not {other:?}"),
    };
    let mut out: Vec<_> = score
        .cross_cutting
        .ties
        .iter()
        .map(|tie| (onset(tie.start_event), onset(tie.end_event)))
        .collect();
    out.sort();
    out
}

/// The number of measures on each staff, top to bottom.
fn measures(score: &Score) -> Vec<usize> {
    score
        .staff_instances()
        .map(|(_, si)| si.measures.len())
        .collect()
}

fn note(n: i64, d: i64, len_n: i64, len_d: i64) -> (MusicalPosition, MusicalDuration, bool) {
    (at(n, d), whole(len_n, len_d), true)
}

#[test]
fn a_note_across_a_barline_is_split_there_and_tied() {
    let mut session = session(1, 2);
    let voice = first_voice(&session);
    session.set_caret(voice, at(3, 4), whole(1, 2)).unwrap();
    session.enter_nominal(CmnNominal::F).unwrap();
    assert_eq!(
        events(session.score(), voice),
        vec![note(3, 4, 1, 4), note(1, 1, 1, 4)],
        "a half on beat 4 is a quarter in each measure"
    );
    assert_eq!(ties(session.score()), vec![(at(3, 4), at(1, 1))]);
    assert_eq!(session.caret().unwrap().position, at(5, 4));
    // Both parts sound the one pitch.
    let pitches: Vec<_> = session
        .score()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Pitched(p) => Some(p.pitches[0].pitch.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(pitches.len(), 2);
    assert_eq!(pitches[0], pitches[1]);
}

#[test]
fn a_rest_across_a_barline_is_split_and_not_tied() {
    let mut session = session(1, 2);
    let voice = first_voice(&session);
    session.set_caret(voice, at(1, 2), whole(3, 4)).unwrap();
    session.enter_rest().unwrap();
    assert_eq!(
        events(session.score(), voice),
        vec![
            (at(1, 2), whole(1, 2), false),
            (at(1, 1), whole(1, 4), false)
        ]
    );
    assert!(ties(session.score()).is_empty());
}

#[test]
fn a_note_across_two_barlines_is_tied_through_each() {
    let mut session = session(1, 3);
    let voice = first_voice(&session);
    session.set_caret(voice, at(3, 4), whole(2, 1)).unwrap();
    session.enter_nominal(CmnNominal::A).unwrap();
    assert_eq!(
        events(session.score(), voice),
        vec![note(3, 4, 1, 4), note(1, 1, 1, 1), note(2, 1, 3, 4)]
    );
    assert_eq!(
        ties(session.score()),
        vec![(at(3, 4), at(1, 1)), (at(1, 1), at(2, 1))]
    );
}

#[test]
fn entry_at_the_last_measure_s_end_opens_the_next_on_every_staff() {
    let mut session = session(2, 1);
    let voice = first_voice(&session);
    assert_eq!(measures(session.score()), vec![1, 1]);
    session.set_caret(voice, at(1, 1), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    assert_eq!(
        measures(session.score()),
        vec![2, 2],
        "a measure opens on both staves"
    );
    assert_eq!(events(session.score(), voice), vec![note(1, 1, 1, 4)]);
    // Filling the opened measure opens no other.
    for nominal in [CmnNominal::D, CmnNominal::E, CmnNominal::F] {
        session.enter_nominal(nominal).unwrap();
    }
    assert_eq!(measures(session.score()), vec![2, 2]);
    assert_eq!(session.caret().unwrap().position, at(2, 1));
}

#[test]
fn a_note_past_the_end_opens_measures_and_ties_across_them() {
    let mut session = session(1, 1);
    let voice = first_voice(&session);
    session.set_caret(voice, at(1, 2), whole(3, 2)).unwrap();
    session.enter_nominal(CmnNominal::G).unwrap();
    assert_eq!(measures(session.score()), vec![2]);
    assert_eq!(
        events(session.score(), voice),
        vec![note(1, 2, 1, 2), note(1, 1, 1, 1)]
    );
    assert_eq!(ties(session.score()), vec![(at(1, 2), at(1, 1))]);
}

#[test]
fn undo_takes_back_the_whole_entry_with_the_measures_it_opened() {
    let mut session = session(2, 1);
    let voice = first_voice(&session);
    session.set_caret(voice, at(3, 4), whole(1, 2)).unwrap();
    session.enter_nominal(CmnNominal::B).unwrap();
    assert_eq!(measures(session.score()), vec![2, 2]);
    assert_eq!(events(session.score(), voice).len(), 2);
    session.undo().expect("the entry undoes");
    assert_eq!(measures(session.score()), vec![1, 1]);
    assert!(events(session.score(), voice).is_empty());
    assert!(ties(session.score()).is_empty());
    session.redo().expect("and redoes");
    assert_eq!(measures(session.score()), vec![2, 2]);
    assert_eq!(ties(session.score()).len(), 1);
}

#[test]
fn an_entry_over_a_tied_note_makes_room_as_any_other() {
    let mut session = session(1, 2);
    let voice = first_voice(&session);
    session.set_caret(voice, at(3, 4), whole(1, 2)).unwrap();
    session.enter_nominal(CmnNominal::F).unwrap();
    // Overwrite the first measure's last beat: the head of the tie goes.
    session.set_caret(voice, at(3, 4), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::E).unwrap();
    assert_eq!(
        events(session.score(), voice),
        vec![note(3, 4, 1, 4), note(1, 1, 1, 4)]
    );
    assert!(
        ties(session.score()).is_empty(),
        "the tie gives way with its start"
    );
}

/// Inserts, as one transaction, a grace note (an eighth acciaccatura, D5)
/// before each of `principals`, at its onset and in the order given: a grace
/// as the import makes one, which the editor neither draws nor selects.
fn add_graces(session: &mut EditorSession, principals: &[EventId]) {
    use epiphany_core::{Grace, GraceKind, IdentifiedPitch, NoteValue, PitchId};
    use epiphany_ops::{InsertEventOp, OperationKind};
    let (_, instance) = session.score().staff_instances().next().expect("a staff");
    let instance = instance.id;
    let replica = ReplicaId(0x4B5A);
    let mut order = std::collections::BTreeMap::new();
    let inserts = principals
        .iter()
        .enumerate()
        .map(|(k, principal)| {
            let Some(Event::Pitched(mut grace)) = session.score().events.get(*principal).cloned()
            else {
                panic!("a note");
            };
            let n = order.entry(*principal).or_insert(0u16);
            grace.id = EventId::new(replica, 2 * k as u64);
            grace.duration = EventDuration::Musical(MusicalDuration::zero());
            grace.pitches = vec![IdentifiedPitch {
                id: PitchId::new(replica, 2 * k as u64 + 1),
                pitch: epiphany_editor_core::midi_note_to_pitch(74),
            }];
            grace.grace = Some(Grace {
                kind: GraceKind::Acciaccatura,
                value: NoteValue::Eighth,
                dots: 0,
                order: *n,
            });
            *n += 1;
            OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event: Event::Pitched(grace),
            })
        })
        .collect();
    session
        .apply_transaction("graces", None, inserts)
        .expect("the graces apply");
}

/// The onsets of the grace notes standing in `score`, in order.
fn grace_onsets(score: &Score) -> Vec<MusicalPosition> {
    let mut out: Vec<_> = score
        .events
        .iter()
        .filter(|e| e.grace().is_some())
        .map(|e| match e.position() {
            EventPosition::Musical(p) => p.clone(),
            other => panic!("a musical onset, not {other:?}"),
        })
        .collect();
    out.sort();
    out
}

/// [`events`] with a grace before the note at its onset: ordered by onset,
/// then length.
fn by_onset_and_length(
    score: &Score,
    voice: VoiceId,
) -> Vec<(MusicalPosition, MusicalDuration, bool)> {
    let mut out = events(score, voice);
    out.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    out
}

/// The note of `voice` starting at `onset`, graces aside.
fn note_at(score: &Score, voice: VoiceId, onset: MusicalPosition) -> EventId {
    score
        .events
        .iter()
        .find(|e| {
            e.voice() == voice
                && e.grace().is_none()
                && e.position() == &EventPosition::Musical(onset.clone())
        })
        .map(Event::id)
        .expect("a note there")
}

/// X4b's grace notes meet entry across the barline (D69's M2: graces go with
/// their note). A half split at the barline and tied replaces the note the
/// second measure opens with, and takes that note's two grace notes in its one
/// transaction, so none is left inside the tied pieces; the half whose tail it
/// cuts keeps its grace, and so does the note after it. One undo brings every
/// note and grace back.
#[test]
fn an_entry_across_a_barline_takes_the_graces_of_the_note_it_replaces() {
    let mut session = session(1, 2);
    let voice = first_voice(&session);
    session.set_caret(voice, at(1, 2), whole(1, 2)).unwrap();
    session.enter_nominal(CmnNominal::E).unwrap();
    session.set_entry_duration(whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::G).unwrap();
    session.enter_nominal(CmnNominal::A).unwrap();
    let score = session.score();
    let [half, opening, after] =
        [at(1, 2), at(1, 1), at(5, 4)].map(|onset| note_at(score, voice, onset));
    add_graces(&mut session, &[half, opening, opening, after]);
    let graced = by_onset_and_length(session.score(), voice);
    assert_eq!(
        grace_onsets(session.score()),
        vec![at(1, 2), at(1, 1), at(1, 1), at(5, 4)]
    );
    assert!(epiphany_core::check_invariants(session.score()).is_empty());

    session.set_caret(voice, at(3, 4), whole(1, 2)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    let grace = |n, d| (at(n, d), MusicalDuration::zero(), true);
    assert_eq!(
        by_onset_and_length(session.score(), voice),
        vec![
            grace(1, 2),
            note(1, 2, 1, 4),
            note(3, 4, 1, 4),
            note(1, 1, 1, 4),
            grace(5, 4),
            note(5, 4, 1, 4)
        ],
        "the half cut to a quarter keeps its grace, the replaced note's two go, \
         and the next note's stays"
    );
    assert_eq!(ties(session.score()), vec![(at(3, 4), at(1, 1))]);
    assert_eq!(grace_onsets(session.score()), vec![at(1, 2), at(5, 4)]);
    assert!(epiphany_core::check_invariants(session.score()).is_empty());

    session
        .undo()
        .expect("the entry undoes, its graces with it");
    assert_eq!(by_onset_and_length(session.score(), voice), graced);
    assert_eq!(
        grace_onsets(session.score()),
        vec![at(1, 2), at(1, 1), at(1, 1), at(5, 4)]
    );
    assert!(ties(session.score()).is_empty());
}
