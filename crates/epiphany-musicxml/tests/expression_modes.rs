//! Schema major 5's verdicts (X4b), each held alike in both reduction modes:
//! a grace note's timing, a lyric's one syllable per event and verse, an
//! event-anchored marker's re-anchoring, and a voice's home staff. Each
//! history starts from an imported measure and holds base-free and
//! graph-aware reduction to each other: every effect, every object and the
//! canonical bytes, with the graph-aware score keeping every invariant.

use epiphany_core::{
    check_invariants, AnchorOffset, Dynamic, Event, EventDuration, EventId, EventPosition, Grace,
    GraceKind, IdentifiedPitch, IdentityContext, Lyric, LyricLineId, Marker, MarkerId, MarkerKind,
    MusicalDuration, MusicalPosition, NoteValue, OperationId, PitchId, RationalTime, ReplicaId,
    Score, StaffId, Syllabic, Text, TimeAnchor, TransactionId, Tuplet, TupletId, TupletRatio,
    TypedObjectId, VoiceId, WallClockTime,
};
use epiphany_musicxml::{import, Import};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, CreateCrossCuttingOp, CreateStaffOp, CreateTupletOp,
    CrossCuttingValue, DeleteEventOp, DeleteVoiceOp, HybridLogicalClock, InsertEventOp,
    MaterializedState, ModifyCrossCuttingOp, ModifyEventOp, NoOpReason, ObjectState,
    OperationEffect, OperationEnvelope, OperationKind, OperationPayload, OperationSet,
    OperationStamp, PreconditionFailureReason, RepairKind, SetVoiceHomeOp, TransactionDescriptor,
    TupletCompensation, UndoPolicy, UndoTransactionPayload,
};

const MEASURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>2</divisions><key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
      <note><pitch><step>F</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
    </measure>
  </part>
</score-partwise>
"#;

/// A part on two staves, a voice on each.
const TWO_STAVES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>2</divisions><key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves>
        <clef number="1"><sign>G</sign><line>2</line></clef>
        <clef number="2"><sign>F</sign><line>4</line></clef></attributes>
      <note><pitch><step>C</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><type>whole</type><staff>1</staff></note>
      <backup><duration>8</duration></backup>
      <note><pitch><step>C</step><octave>3</octave></pitch><duration>8</duration><voice>5</voice><type>whole</type><staff>2</staff></note>
    </measure>
  </part>
</score-partwise>
"#;

const A: ReplicaId = ReplicaId(21);
const B: ReplicaId = ReplicaId(22);

/// The import and its events, and the authors' clock, which starts after the
/// import's last stamp.
struct Measure {
    import: Import,
    events: Vec<Event>,
}

impl Measure {
    fn of(xml: &str) -> Self {
        let import = import(xml).expect("the measure imports");
        let score = graph_of(&import, &[]);
        let events = import.ids.events[0]
            .iter()
            .map(|id| score.events.get(*id).expect("imported").clone())
            .collect();
        Self { import, events }
    }

    fn q(&self, i: usize) -> EventId {
        self.events[i].id()
    }

    fn voice(&self) -> VoiceId {
        self.events[0].voice()
    }

    fn position(&self, i: usize) -> MusicalPosition {
        match self.events[i].position() {
            EventPosition::Musical(position) => position.clone(),
            other => panic!("a metric event, not {other:?}"),
        }
    }

    /// An operation by `author`, stamped `at` ticks after the import, its
    /// author having seen the import and `seen`.
    fn op(
        &self,
        author: ReplicaId,
        counter: u64,
        at: i64,
        seen: &[OperationId],
        payload: OperationPayload,
    ) -> OperationEnvelope {
        let id = OperationId::new(author, counter);
        let mut context = CausalContext::new()
            .with_seen(self.import.replica, self.import.envelopes.len() as u64 - 1);
        for id in seen {
            context = context.with_seen(id.replica, id.counter);
        }
        let physical = self.import.envelopes.len() as i64 + at;
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
            causal_context: context,
            transaction: None,
            payload,
        }
    }

    /// `event` inserted into the measure's first staff.
    fn insert(&self, event: Event) -> OperationPayload {
        primitive(OperationKind::InsertEvent(InsertEventOp {
            staff_instance: self.import.ids.instances[0][0],
            event,
        }))
    }

    /// A grace note of the first voice at `at`, in `order`.
    fn grace(&self, counter: u64, at: MusicalPosition, order: u16) -> Event {
        note(
            counter,
            self.voice(),
            at,
            MusicalDuration::zero(),
            Some(order),
        )
    }

    /// Reduces the import and `authored` both ways and holds the two
    /// reductions to each other, the graph-aware score keeping every
    /// invariant. Returns the base-free state and the graph-aware score.
    fn agree(&self, history: &str, authored: &[OperationEnvelope]) -> (MaterializedState, Score) {
        let mut set = OperationSet::new();
        set.accept_all(self.import.envelopes.iter().chain(authored).cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(self.import.replica)));
        for envelope in authored {
            assert_eq!(
                effect(&free, envelope.id),
                effect(&aware.state, envelope.id),
                "{history}: {:?} reduces differently base-free and graph-aware",
                envelope.id
            );
        }
        assert_eq!(
            free.objects, aware.state.objects,
            "{history}: the objects differ by mode"
        );
        assert!(
            free.canonical_bytes() == aware.state.canonical_bytes(),
            "{history}: the canonical bytes differ by mode"
        );
        let violations = check_invariants(&aware.score);
        assert!(violations.is_empty(), "{history}: {violations:?}");
        (free, aware.score)
    }
}

fn graph_of(import: &Import, authored: &[OperationEnvelope]) -> Score {
    let mut set = OperationSet::new();
    set.accept_all(import.envelopes.iter().chain(authored).cloned());
    set.reduce_onto(&Score::empty(IdentityContext::new(import.replica)))
        .score
}

/// A note of `voice`, a grace note in `order` when one is given.
fn note(
    counter: u64,
    voice: VoiceId,
    at: MusicalPosition,
    duration: MusicalDuration,
    order: Option<u16>,
) -> Event {
    let Event::Pitched(mut note) = valuegen::insert_event_value(
        EventId::new(A, counter),
        voice,
        at,
        duration,
        &[PitchId::new(A, counter)],
    ) else {
        unreachable!("a note");
    };
    note.pitches = vec![IdentifiedPitch {
        id: PitchId::new(A, counter),
        pitch: valuegen::pitch_value_nth(3),
    }];
    note.grace = order.map(|order| Grace {
        kind: GraceKind::Acciaccatura,
        value: NoteValue::Eighth,
        dots: 0,
        order,
    });
    Event::Pitched(note)
}

fn eighth() -> MusicalDuration {
    MusicalDuration(RationalTime::new(1, 8).expect("an eighth"))
}

fn primitive(kind: OperationKind) -> OperationPayload {
    OperationPayload::Primitive(kind)
}

fn effect(state: &MaterializedState, id: OperationId) -> Option<OperationEffect> {
    state
        .effects
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, e)| e.clone())
}

fn refused(reason: PreconditionFailureReason) -> Option<OperationEffect> {
    Some(OperationEffect::NoOp {
        reason: NoOpReason::PreconditionFailedUnderReduction { reason },
    })
}

fn applied() -> Option<OperationEffect> {
    Some(OperationEffect::Applied)
}

fn live(state: &MaterializedState, object: TypedObjectId) -> bool {
    matches!(state.objects.get(&object), Some(ObjectState::Live))
}

/// The first voice's events, in the graph's order.
fn voice_events(score: &Score, voice: VoiceId) -> Vec<EventId> {
    score
        .voices()
        .find(|(_, _, v)| v.id == voice)
        .map(|(_, _, v)| v.events.clone())
        .expect("the voice is in the graph")
}

/// A grace note takes no voice time: graces stand at their note's position,
/// before it, in their order and then by id, two authors' graces of one
/// order both standing, in both modes.
#[test]
fn graces_stand_before_their_note_in_their_order_in_both_modes() {
    let m = Measure::of(MEASURE);
    let at = m.position(1);
    let authored = vec![
        m.op(A, 0, 1, &[], m.insert(m.grace(3600, at.clone(), 1))),
        m.op(A, 1, 2, &[], m.insert(m.grace(3601, at.clone(), 0))),
        m.op(B, 0, 2, &[], m.insert(m.grace(3602, at, 0))),
    ];
    let (state, score) = m.agree("three graces before the second quarter", &authored);
    for envelope in &authored {
        assert_eq!(effect(&state, envelope.id), applied());
    }
    let grace = |counter| EventId::new(A, counter);
    assert_eq!(
        voice_events(&score, m.voice()),
        vec![
            m.q(0),
            grace(3601),
            grace(3602),
            grace(3600),
            m.q(1),
            m.q(2),
            m.q(3)
        ]
    );
}

/// A grace note has zero duration and only a grace note does: an insert or a
/// modify that breaks either is refused `EventDurationInvalid`, in both modes.
#[test]
fn a_grace_and_its_duration_must_agree_in_both_modes() {
    let m = Measure::of(MEASURE);
    let invalid = refused(PreconditionFailureReason::EventDurationInvalid);
    let grace = m.grace(3600, m.position(1), 0);
    let mut long_grace = grace.clone();
    if let Event::Pitched(p) = &mut long_grace {
        p.duration = EventDuration::Musical(eighth());
    }
    let unmarked = note(
        3601,
        m.voice(),
        m.position(1),
        MusicalDuration::zero(),
        None,
    );
    let inserted = m.op(A, 0, 1, &[], m.insert(grace.clone()));
    let mut quarter_as_point = m.events[2].clone();
    if let Event::Pitched(p) = &mut quarter_as_point {
        p.duration = EventDuration::Musical(MusicalDuration::zero());
    }
    let authored = vec![
        m.op(B, 0, 1, &[], m.insert(long_grace.clone())),
        m.op(B, 1, 2, &[], m.insert(unmarked)),
        inserted.clone(),
        m.op(
            A,
            1,
            2,
            &[inserted.id],
            primitive(OperationKind::ModifyEvent(ModifyEventOp {
                event: Event::Pitched(match long_grace {
                    Event::Pitched(mut p) => {
                        p.id = grace.id();
                        p
                    }
                    _ => unreachable!("a note"),
                }),
            })),
        ),
        m.op(
            A,
            2,
            3,
            &[],
            primitive(OperationKind::ModifyEvent(ModifyEventOp {
                event: quarter_as_point,
            })),
        ),
    ];
    let (state, score) = m.agree("graces at odds with their durations", &authored);
    assert_eq!(effect(&state, authored[0].id), invalid, "a long grace");
    assert_eq!(effect(&state, authored[1].id), invalid, "a point note");
    assert_eq!(effect(&state, authored[2].id), applied());
    assert_eq!(
        effect(&state, authored[3].id),
        invalid,
        "a grace lengthened"
    );
    assert_eq!(effect(&state, authored[4].id), invalid, "a quarter shrunk");
    assert_eq!(score.events.get(grace.id()), Some(&grace));
    assert_eq!(score.events.get(m.q(2)), Some(&m.events[2]));

    // Where no placement check reads it: a quarter made a grace at its own
    // span, and a grace standing alone lengthened into free time.
    let mut quarter_as_grace = m.events[2].clone();
    if let Event::Pitched(p) = &mut quarter_as_grace {
        p.grace = Some(Grace {
            kind: GraceKind::Appoggiatura,
            value: NoteValue::Eighth,
            dots: 0,
            order: 0,
        });
    }
    let deleted = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: m.q(1),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let alone = m.op(
        A,
        1,
        2,
        &[deleted.id],
        m.insert(m.grace(3600, m.position(1), 0)),
    );
    let mut lengthened = m.grace(3600, m.position(1), 0);
    if let Event::Pitched(p) = &mut lengthened {
        p.duration = EventDuration::Musical(eighth());
    }
    let authored = vec![
        deleted,
        alone.clone(),
        m.op(
            A,
            2,
            3,
            &[alone.id],
            primitive(OperationKind::ModifyEvent(ModifyEventOp {
                event: lengthened,
            })),
        ),
        m.op(
            A,
            3,
            4,
            &[],
            primitive(OperationKind::ModifyEvent(ModifyEventOp {
                event: quarter_as_grace,
            })),
        ),
    ];
    let (state, score) = m.agree("graces made where nothing else refuses", &authored);
    assert_eq!(effect(&state, alone.id), applied());
    assert_eq!(
        effect(&state, authored[2].id),
        invalid,
        "a lone grace lengthened"
    );
    assert_eq!(
        effect(&state, authored[3].id),
        invalid,
        "a quarter made a grace"
    );
    assert_eq!(score.events.get(m.q(2)), Some(&m.events[2]));
}

/// A grace stands at a note's onset, never inside its span: an insert there
/// is refused, and so is a note's move or trim over a grace that stands
/// alone, in both modes.
#[test]
fn a_grace_inside_a_notes_span_is_refused_in_both_modes() {
    let m = Measure::of(MEASURE);
    let invalid = refused(PreconditionFailureReason::EventDurationInvalid);
    // Inside the first quarter.
    let inside = m.op(
        A,
        0,
        1,
        &[],
        m.insert(m.grace(3600, MusicalPosition(eighth().0), 0)),
    );
    // The second quarter deleted and a grace left at its place; the first
    // quarter then lengthened over it.
    let deleted = m.op(
        A,
        1,
        1,
        &[],
        primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: m.q(1),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let alone = m.op(
        A,
        2,
        2,
        &[deleted.id],
        m.insert(m.grace(3601, m.position(1), 0)),
    );
    let mut half = m.events[0].clone();
    if let Event::Pitched(p) = &mut half {
        p.duration =
            EventDuration::Musical(MusicalDuration(RationalTime::new(1, 2).expect("a half")));
    }
    let lengthened = m.op(
        A,
        3,
        3,
        &[alone.id],
        primitive(OperationKind::ModifyEvent(ModifyEventOp { event: half })),
    );
    let authored = vec![inside.clone(), deleted, alone.clone(), lengthened.clone()];
    let (state, score) = m.agree("graces inside a note's span", &authored);
    assert_eq!(effect(&state, inside.id), invalid);
    assert_eq!(effect(&state, alone.id), applied());
    assert_eq!(effect(&state, lengthened.id), invalid);
    assert_eq!(score.events.get(m.q(0)), Some(&m.events[0]));
}

/// A grace's principal is positional: graces survive their note's delete,
/// standing alone at its position, in both modes.
#[test]
fn graces_survive_their_notes_delete_in_both_modes() {
    let m = Measure::of(MEASURE);
    let grace = m.op(A, 0, 1, &[], m.insert(m.grace(3600, m.position(1), 0)));
    let deleted = m.op(
        A,
        1,
        2,
        &[grace.id],
        primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: m.q(1),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let (state, score) = m.agree("a grace whose note is deleted", &[grace, deleted]);
    assert!(live(&state, TypedObjectId::Event(EventId::new(A, 3600))));
    assert_eq!(
        voice_events(&score, m.voice()),
        vec![m.q(0), EventId::new(A, 3600), m.q(2), m.q(3)]
    );
}

/// A modify that changes a grace's order alone re-sorts its voice, in both
/// modes.
#[test]
fn a_graces_new_order_resorts_its_voice_in_both_modes() {
    let m = Measure::of(MEASURE);
    let at = m.position(1);
    let first = m.op(A, 0, 1, &[], m.insert(m.grace(3600, at.clone(), 0)));
    let second = m.op(A, 1, 2, &[], m.insert(m.grace(3601, at.clone(), 1)));
    let moved = m.op(
        A,
        2,
        3,
        &[first.id, second.id],
        primitive(OperationKind::ModifyEvent(ModifyEventOp {
            event: m.grace(3600, at, 2),
        })),
    );
    let (state, score) = m.agree(
        "a grace moved after another",
        &[first, second, moved.clone()],
    );
    assert_eq!(effect(&state, moved.id), applied());
    assert_eq!(
        voice_events(&score, m.voice()),
        vec![
            m.q(0),
            EventId::new(A, 3601),
            EventId::new(A, 3600),
            m.q(1),
            m.q(2),
            m.q(3)
        ]
    );
}

/// A grace note is no tuplet's member, though it adds nothing to the
/// members' sum, in both modes.
#[test]
fn a_grace_is_no_tuplets_member_in_both_modes() {
    let m = Measure::of(MEASURE);
    let grace = m.op(A, 0, 1, &[], m.insert(m.grace(3600, m.position(1), 0)));
    let tuplet = |counter: u64, members: Vec<EventId>| {
        primitive(OperationKind::CreateTuplet(CreateTupletOp {
            tuplet: Tuplet {
                id: TupletId::new(A, counter),
                ratio: TupletRatio::new(3, 2).expect("not degenerate"),
                members,
                parent: None,
                required_total: MusicalDuration(RationalTime::new(1, 4).expect("a quarter")),
                display: Default::default(),
            },
        }))
    };
    let with_grace = m.op(
        A,
        1,
        2,
        &[grace.id],
        tuplet(3700, vec![EventId::new(A, 3600), m.q(1)]),
    );
    let without = m.op(A, 2, 3, &[grace.id], tuplet(3701, vec![m.q(1)]));
    let (state, _) = m.agree(
        "a tuplet over a grace and its note",
        &[grace, with_grace.clone(), without.clone()],
    );
    assert_eq!(
        effect(&state, with_grace.id),
        refused(PreconditionFailureReason::EventDurationInvalid)
    );
    assert_eq!(effect(&state, without.id), applied());
}

/// A standard tie joins two adjacent notes of a voice, the graces before
/// either taking no place between them: a tie over graces at its start's or
/// its end's position applies, in both modes, as the core's tie check reads
/// it.
#[test]
fn a_tie_passes_over_graces_in_both_modes() {
    let m = Measure::of(&MEASURE.replace("<step>E</step>", "<step>D</step>"));
    for (n, at) in [1, 2].into_iter().enumerate() {
        let grace = m.op(A, 0, 1, &[], m.insert(m.grace(3600, m.position(at), 0)));
        let mut tie = valuegen::tie(epiphany_core::TieId::new(A, 3700), m.q(1), m.q(2));
        tie.class = epiphany_core::TieClass::Standard;
        let tied = m.op(A, 1, 2, &[grace.id], create(CrossCuttingValue::Tie(tie)));
        let (state, score) = m.agree(&format!("a tie over a grace ({n})"), &[grace, tied.clone()]);
        assert_eq!(
            effect(&state, tied.id),
            applied(),
            "a grace at quarter {at}"
        );
        assert_eq!(score.cross_cutting.ties.len(), 1);
    }
}

/// A grace note keeps a pitch, with none being a rest of no length: a delete
/// of its last is refused, as is a modify leaving it none and an undo taking
/// its last; a delete of one of two applies, and an undo that brings back a
/// pitch a modify replaced applies. In both modes.
#[test]
fn a_grace_note_keeps_a_pitch_in_both_modes() {
    use epiphany_ops::{DeleteIdentifiedPitchOp, InsertIdentifiedPitchOp};
    let m = Measure::of(MEASURE);
    let invalid = refused(PreconditionFailureReason::EventDurationInvalid);
    let grace = m.grace(3600, m.position(1), 0);
    let first = PitchId::new(A, 3600);
    let second = IdentifiedPitch {
        id: PitchId::new(A, 3601),
        pitch: valuegen::pitch_value_nth(5),
    };
    let delete = |pitch: PitchId| {
        primitive(OperationKind::DeleteIdentifiedPitch(
            DeleteIdentifiedPitchOp { pitch },
        ))
    };
    let inserted = m.op(A, 0, 1, &[], m.insert(grace.clone()));

    // Its only pitch: refused.
    let only = m.op(A, 1, 2, &[inserted.id], delete(first));
    let (state, score) = m.agree(
        "a grace's only pitch deleted",
        &[inserted.clone(), only.clone()],
    );
    assert_eq!(effect(&state, only.id), invalid);
    assert_eq!(score.events.get(grace.id()), Some(&grace));

    // One of two: applies; then the other: refused; then a modify carrying
    // the deleted one alone: refused.
    let added = m.op(
        A,
        1,
        2,
        &[inserted.id],
        primitive(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp {
                event: grace.id(),
                pitch: second.clone(),
            },
        )),
    );
    let one = m.op(A, 2, 3, &[added.id], delete(first));
    let other = m.op(A, 3, 4, &[one.id], delete(second.id));
    let mut emptied = grace.clone();
    if let Event::Pitched(p) = &mut emptied {
        p.pitches[0].id = first;
    }
    let written = m.op(
        A,
        4,
        5,
        &[other.id],
        primitive(OperationKind::ModifyEvent(ModifyEventOp { event: emptied })),
    );
    let (state, _) = m.agree(
        "a grace's pitches deleted",
        &[
            inserted.clone(),
            added.clone(),
            one.clone(),
            other.clone(),
            written.clone(),
        ],
    );
    assert_eq!(effect(&state, one.id), applied());
    assert_eq!(effect(&state, other.id), invalid);
    assert_eq!(effect(&state, written.id), invalid);
    assert!(live(&state, TypedObjectId::Pitch(second.id)));

    // An undo of the pitch's insert, the first deleted since: a strict undo
    // conflicts and a best-effort one keeps it.
    let tx = TransactionId::new(A, 700);
    let mut declare = m.op(
        A,
        1,
        2,
        &[inserted.id],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("pitch"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut in_tx = m.op(
        A,
        2,
        3,
        &[declare.id],
        primitive(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp {
                event: grace.id(),
                pitch: second.clone(),
            },
        )),
    );
    in_tx.transaction = Some(tx);
    let gone = m.op(A, 3, 4, &[in_tx.id], delete(first));
    for (policy, conflicts) in [
        (UndoPolicy::StrictInverse, true),
        (UndoPolicy::BestEffort, false),
    ] {
        let undo = m.op(
            A,
            4,
            5,
            &[gone.id],
            OperationPayload::UndoTransaction(UndoTransactionPayload { target: tx, policy }),
        );
        let (state, score) = m.agree(
            "an undo taking a grace's last pitch",
            &[
                inserted.clone(),
                declare.clone(),
                in_tx.clone(),
                gone.clone(),
                undo.clone(),
            ],
        );
        assert_eq!(
            matches!(
                effect(&state, undo.id),
                Some(OperationEffect::Conflicted { .. })
            ),
            conflicts,
            "{policy:?}"
        );
        assert!(live(&state, TypedObjectId::Pitch(second.id)), "{policy:?}");
        assert!(matches!(
            score.events.get(grace.id()),
            Some(Event::Pitched(_))
        ));
    }

    // A modify replacing its pitch, undone: the pitch comes back, and the
    // undo applies.
    let mut replaced = grace.clone();
    if let Event::Pitched(p) = &mut replaced {
        p.pitches = vec![second.clone()];
    }
    let mut modified = m.op(
        A,
        2,
        3,
        &[declare.id],
        primitive(OperationKind::ModifyEvent(ModifyEventOp {
            event: replaced,
        })),
    );
    modified.transaction = Some(tx);
    let undo = m.op(
        A,
        3,
        4,
        &[modified.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let (state, score) = m.agree(
        "a grace's replaced pitch restored",
        &[
            inserted.clone(),
            declare.clone(),
            modified.clone(),
            undo.clone(),
        ],
    );
    assert!(matches!(
        effect(&state, modified.id),
        Some(OperationEffect::AppliedWithRepair { .. })
    ));
    assert!(matches!(
        effect(&state, undo.id),
        Some(OperationEffect::Applied | OperationEffect::AppliedWithRepair { .. })
    ));
    assert_eq!(score.events.get(grace.id()), Some(&grace));
}

/// A marker anchored to a region by a musical offset is held to the
/// region's time, as a clef or key change is: refused in a region out of
/// musical time; a live one holds its region against a delete, and strands
/// a migration out of musical time. In both modes.
#[test]
fn a_marker_on_a_region_is_held_to_its_time_in_both_modes() {
    use epiphany_core::RegionId;
    use epiphany_ops::{
        ChangeRegionTimeModelOp, ConflictKind, CreateRegionOp, DeleteRegionOp, PositionRemapping,
    };
    let m = Measure::of(MEASURE);
    let region = RegionId::new(A, 900);
    let created = |counter: u64, proportional: bool| {
        let mut value = valuegen::region(region);
        if proportional {
            value.time_model = valuegen::proportional_model();
        }
        m.op(
            A,
            counter,
            1,
            &[],
            primitive(OperationKind::CreateRegion(CreateRegionOp {
                region: value,
            })),
        )
    };
    let marker = |at: &OperationEnvelope| {
        m.op(
            A,
            1,
            2,
            &[at.id],
            create(CrossCuttingValue::Marker(Marker {
                id: MarkerId::new(A, 901),
                anchor: TimeAnchor::Region {
                    id: region,
                    edge: epiphany_core::RegionEdge::Start,
                    offset: AnchorOffset::Musical(eighth()),
                },
                kind: MarkerKind::Segno,
            })),
        )
    };

    // Into a region out of musical time: refused.
    let proportional = created(0, true);
    let refused_mark = marker(&proportional);
    let (state, _) = m.agree(
        "a marker at a musical offset in proportional time",
        &[proportional, refused_mark.clone()],
    );
    assert_eq!(
        effect(&state, refused_mark.id),
        refused(PreconditionFailureReason::WrongRegionTimeModel)
    );

    // In a metric region: applies, and holds the region against a delete
    // and a migration out of musical time.
    let metric = created(0, false);
    let mark = marker(&metric);
    let deleted = m.op(
        A,
        2,
        3,
        &[mark.id],
        primitive(OperationKind::DeleteRegion(DeleteRegionOp { region })),
    );
    let migrated = m.op(
        A,
        3,
        4,
        &[deleted.id],
        primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region,
                new_time_model: valuegen::proportional_model(),
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::PreserveTime,
            },
        )),
    );
    let (state, _) = m.agree(
        "a marker holding its region",
        &[metric, mark.clone(), deleted.clone(), migrated.clone()],
    );
    assert_eq!(effect(&state, mark.id), applied());
    assert_eq!(
        effect(&state, deleted.id),
        refused(PreconditionFailureReason::ContainerNotEmpty)
    );
    let Some(OperationEffect::Conflicted { conflict }) = effect(&state, migrated.id) else {
        panic!("the migration conflicts");
    };
    let record = state
        .conflicts
        .records()
        .iter()
        .find(|r| r.id == conflict)
        .expect("recorded");
    assert!(
        matches!(&record.kind, ConflictKind::TimeModelMigrationFailure { incompatible_events, .. }
        if incompatible_events.contains(&TypedObjectId::Marker(MarkerId::new(A, 901))))
    );

    // An undo of the region's create, another author's marker on it: a
    // strict undo conflicts, and the region stays.
    let tx = TransactionId::new(A, 700);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("region"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut value = valuegen::region(region);
    value.time_model = valuegen::metric_model();
    let mut in_tx = m.op(
        A,
        1,
        2,
        &[declare.id],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: value,
        })),
    );
    in_tx.transaction = Some(tx);
    let by_b = m.op(
        B,
        0,
        3,
        &[in_tx.id],
        create(CrossCuttingValue::Marker(Marker {
            id: MarkerId::new(B, 902),
            anchor: TimeAnchor::Region {
                id: region,
                edge: epiphany_core::RegionEdge::Start,
                offset: AnchorOffset::Musical(eighth()),
            },
            kind: MarkerKind::Coda,
        })),
    );
    let undo = m.op(
        A,
        2,
        4,
        &[in_tx.id, by_b.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let (state, _) = m.agree(
        "an undo of a marked region's create",
        &[declare, in_tx, by_b.clone(), undo.clone()],
    );
    assert_eq!(effect(&state, by_b.id), applied());
    assert!(matches!(
        effect(&state, undo.id),
        Some(OperationEffect::Conflicted { .. })
    ));
    assert!(live(&state, TypedObjectId::Region(region)));
}

fn lyric(counter: u64, event: EventId, verse: u16) -> Lyric {
    Lyric {
        id: LyricLineId::new(A, counter),
        event,
        verse,
        text: Text::new("la"),
        syllabic: Syllabic::Single,
        extension: false,
    }
}

fn create(structure: CrossCuttingValue) -> OperationPayload {
    primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
        structure,
    }))
}

/// One syllable per event and verse: a second is refused `SlotOccupied`, by
/// a create or a modify, and of two concurrent ones the later in canonical
/// order; another verse applies; a deleted event takes its syllables with
/// it. In both modes.
#[test]
fn one_syllable_per_event_and_verse_in_both_modes() {
    let m = Measure::of(MEASURE);
    let occupied = refused(PreconditionFailureReason::SlotOccupied);
    let first = m.op(
        A,
        0,
        1,
        &[],
        create(CrossCuttingValue::Lyric(lyric(1, m.q(0), 1))),
    );
    let second = m.op(
        A,
        1,
        2,
        &[first.id],
        create(CrossCuttingValue::Lyric(lyric(2, m.q(0), 1))),
    );
    let verse_two = m.op(
        A,
        2,
        3,
        &[first.id],
        create(CrossCuttingValue::Lyric(lyric(3, m.q(0), 2))),
    );
    let moved = m.op(
        A,
        3,
        4,
        &[verse_two.id],
        primitive(OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
            structure: CrossCuttingValue::Lyric(lyric(3, m.q(0), 1)),
        })),
    );
    // Two authors' syllables on the second quarter, neither seeing the other.
    let by_a = m.op(
        A,
        4,
        5,
        &[],
        create(CrossCuttingValue::Lyric(lyric(4, m.q(1), 1))),
    );
    let mut b_lyric = lyric(5, m.q(1), 1);
    b_lyric.id = LyricLineId::new(B, 5);
    let by_b = m.op(B, 0, 5, &[], create(CrossCuttingValue::Lyric(b_lyric)));
    let authored = vec![
        first.clone(),
        second.clone(),
        verse_two.clone(),
        moved.clone(),
        by_a.clone(),
        by_b.clone(),
    ];
    let (state, score) = m.agree("syllables on one event and verse", &authored);
    assert_eq!(effect(&state, first.id), applied());
    assert_eq!(effect(&state, second.id), occupied);
    assert_eq!(effect(&state, verse_two.id), applied());
    assert_eq!(effect(&state, moved.id), occupied);
    let concurrent = [effect(&state, by_a.id), effect(&state, by_b.id)];
    assert_eq!(
        concurrent.iter().filter(|e| **e == applied()).count(),
        1,
        "one of two concurrent syllables stands: {concurrent:?}"
    );
    assert!(concurrent.contains(&occupied));
    assert_eq!(score.cross_cutting.lyrics.len(), 3);

    // The first quarter deleted: its two syllables go with it.
    let mut seen: Vec<OperationId> = authored.iter().map(|e| e.id).collect();
    let deleted = m.op(
        A,
        5,
        6,
        &seen,
        primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: m.q(0),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    seen.push(deleted.id);
    let mut with_delete = authored.clone();
    with_delete.push(deleted);
    let (state, score) = m.agree("a syllable's event deleted", &with_delete);
    for counter in [1, 3] {
        let syllable = TypedObjectId::LyricLine(LyricLineId::new(A, counter));
        assert!(!live(&state, syllable), "{syllable:?} goes with its event");
    }
    assert!(score.cross_cutting.lyrics.iter().all(|l| l.event != m.q(0)));
    assert_eq!(score.cross_cutting.lyrics.len(), 1);
}

/// A marker an operation anchors to an event moves to the nearest live event
/// when its event is deleted, the repair recorded alike in both modes: base-
/// free reduction reads the event's place from the indices both keep.
#[test]
fn an_event_anchored_marker_follows_its_events_delete_in_both_modes() {
    let m = Measure::of(MEASURE);
    let marker = MarkerId::new(A, 1);
    let created = m.op(
        A,
        0,
        1,
        &[],
        create(CrossCuttingValue::Marker(Marker {
            id: marker,
            anchor: TimeAnchor::Event {
                id: m.q(1),
                offset: AnchorOffset::Zero,
            },
            kind: MarkerKind::Dynamic(Dynamic::Mf),
        })),
    );
    let deleted = m.op(
        A,
        1,
        2,
        &[created.id],
        primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: m.q(1),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let (state, score) = m.agree("a marked event deleted", &[created, deleted.clone()]);
    let Some(OperationEffect::AppliedWithRepair { repairs }) = effect(&state, deleted.id) else {
        panic!("the delete repairs the marker");
    };
    let to = repairs
        .iter()
        .find_map(|r| match r.kind {
            RepairKind::Reanchored {
                to: TypedObjectId::Event(to),
                ..
            } if r.target == TypedObjectId::Marker(marker) => Some(to),
            _ => None,
        })
        .expect("the marker is re-anchored");
    assert!(live(&state, TypedObjectId::Marker(marker)));
    let value = score
        .cross_cutting
        .markers
        .iter()
        .find(|v| v.id == marker)
        .expect("the marker stands");
    assert!(matches!(value.anchor, TimeAnchor::Event { id, .. } if id == to));
}

/// A voice's home staff is a register: set, cleared and overwritten as a
/// last-writer-wins value, concurrent differing writes conflicting, an undo
/// restoring the earlier home, a deleted voice refusing it, in both modes.
#[test]
fn a_voice_home_is_a_register_in_both_modes() {
    let m = Measure::of(TWO_STAVES);
    let staves = &m.import.ids.staves[0];
    let (upper, lower) = (staves[0], staves[1]);
    let voice = m.events[0].voice();
    let home = |staff: Option<StaffId>| {
        primitive(OperationKind::SetVoiceHome(SetVoiceHomeOp {
            voice,
            home: staff,
        }))
    };
    let tx = TransactionId::new(A, 700);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("home"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut set = m.op(A, 1, 2, &[declare.id], home(Some(lower)));
    set.transaction = Some(tx);
    let (state, score) = m.agree("a home set", &[declare.clone(), set.clone()]);
    assert_eq!(effect(&state, set.id), applied());
    assert_eq!(score.voice_homes.get(&voice), Some(&lower));

    // Undone: the voice has no home again.
    let undo = m.op(
        A,
        2,
        3,
        &[set.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let (state, score) = m.agree(
        "a home set and undone",
        &[declare.clone(), set.clone(), undo.clone()],
    );
    assert_eq!(effect(&state, undo.id), applied());
    assert_eq!(score.voice_homes.get(&voice), None);

    // Two authors' differing homes, neither seeing the other: one conflict,
    // the later in canonical order standing.
    let by_a = m.op(A, 3, 4, &[], home(Some(upper)));
    let by_b = m.op(B, 0, 4, &[], home(Some(lower)));
    let (state, score) = m.agree("two concurrent homes", &[by_a.clone(), by_b.clone()]);
    let effects = [effect(&state, by_a.id), effect(&state, by_b.id)];
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Some(OperationEffect::Conflicted { .. }))),
        "concurrent differing homes conflict: {effects:?}"
    );
    let standing = score.voice_homes.get(&voice).copied();
    assert!(standing == Some(upper) || standing == Some(lower));

    // A homed voice deleted, its note first: it loses its home, and a later
    // home is refused.
    let homed = m.op(B, 1, 5, &[by_b.id], home(Some(lower)));
    let emptied = m.op(
        B,
        2,
        6,
        &[homed.id],
        primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: m.q(0),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let gone = m.op(
        B,
        3,
        7,
        &[emptied.id],
        primitive(OperationKind::DeleteVoice(DeleteVoiceOp { voice })),
    );
    let after = m.op(B, 4, 8, &[gone.id], home(Some(lower)));
    let mut first = by_b.clone();
    first.payload = home(None);
    let (state, score) = m.agree(
        "a home on a deleted voice",
        &[first, homed.clone(), emptied, gone.clone(), after.clone()],
    );
    assert_eq!(effect(&state, homed.id), applied());
    assert_eq!(effect(&state, gone.id), applied());
    assert_eq!(
        effect(&state, after.id),
        Some(OperationEffect::NoOp {
            reason: NoOpReason::TargetTombstoned
        })
    );
    assert!(score.voice_homes.is_empty());
}

/// An undo cannot remove a staff a live voice's home names: a strict undo of
/// the staff's create conflicts, and a best-effort one keeps the staff; a
/// home naming a staff an undo removed is refused. In both modes.
#[test]
fn a_home_holds_its_staff_against_an_undo_in_both_modes() {
    let m = Measure::of(TWO_STAVES);
    let voice = m.events[0].voice();
    let staff = StaffId::new(A, 900);
    let tx = TransactionId::new(A, 700);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("staff"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut created = m.op(
        A,
        1,
        2,
        &[declare.id],
        primitive(OperationKind::CreateStaff(CreateStaffOp {
            staff: valuegen::staff(staff, m.import.ids.instruments[0]),
        })),
    );
    created.transaction = Some(tx);
    let homed = m.op(
        B,
        0,
        3,
        &[created.id],
        primitive(OperationKind::SetVoiceHome(SetVoiceHomeOp {
            voice,
            home: Some(staff),
        })),
    );
    let undo = |counter: u64, policy: UndoPolicy, seen: &[OperationId]| {
        m.op(
            A,
            counter,
            4,
            seen,
            OperationPayload::UndoTransaction(UndoTransactionPayload { target: tx, policy }),
        )
    };
    let strict = undo(2, UndoPolicy::StrictInverse, &[created.id, homed.id]);
    let (state, score) = m.agree(
        "a strict undo of a homed staff",
        &[
            declare.clone(),
            created.clone(),
            homed.clone(),
            strict.clone(),
        ],
    );
    assert!(matches!(
        effect(&state, strict.id),
        Some(OperationEffect::Conflicted { .. })
    ));
    assert!(live(&state, TypedObjectId::Staff(staff)));
    assert_eq!(score.voice_homes.get(&voice), Some(&staff));

    let best = undo(2, UndoPolicy::BestEffort, &[created.id, homed.id]);
    let (state, _) = m.agree(
        "a best-effort undo of a homed staff",
        &[declare.clone(), created.clone(), homed.clone(), best],
    );
    assert!(live(&state, TypedObjectId::Staff(staff)));

    // Undone before any home names it: a later home naming it is refused.
    let removed = undo(2, UndoPolicy::StrictInverse, &[created.id]);
    let late = m.op(
        B,
        1,
        5,
        &[removed.id],
        primitive(OperationKind::SetVoiceHome(SetVoiceHomeOp {
            voice,
            home: Some(staff),
        })),
    );
    let (state, score) = m.agree(
        "a home naming a removed staff",
        &[declare, created, removed.clone(), late.clone()],
    );
    assert!(matches!(
        effect(&state, removed.id),
        Some(OperationEffect::AppliedWithRepair { .. })
    ));
    assert!(!live(&state, TypedObjectId::Staff(staff)));
    assert_eq!(
        effect(&state, late.id),
        refused(PreconditionFailureReason::TargetMissing)
    );
    assert!(score.voice_homes.is_empty());
}
