//! One history, two reductions. Base-free reduction (`reduce`) is what the
//! convergence suite checks; graph-aware reduction onto the importer's empty
//! base (`reduce_onto`) is what the importer and the editor compute. On a
//! valid history the two must agree in every effect, every object and the
//! canonical bytes, so each history here is reduced both ways and compared.
//!
//! The histories start from a hand-written measure of four quarters, imported
//! as one author's operations, and add two authors who have each seen the
//! import. Between them they take every path a `DeleteEvent`'s tuplet
//! compensation can take, alone and against concurrent edits, and the cascade
//! an undo makes when it removes a member. Each history also asserts the
//! verdict it exists for, so agreement cannot hold by both modes refusing
//! everything.

use epiphany_core::{
    check_invariants, Event, EventDuration, EventId, IdentityContext, MusicalDuration, OperationId,
    RationalTime, ReplicaId, Rest, Score, TransactionId, Tuplet, TupletId, TupletRatio,
    TypedObjectId, WallClockDuration, WallClockTime,
};
use epiphany_musicxml::{import, Import};
use epiphany_ops::{
    AuthorId, CausalContext, CreateTupletOp, DeleteEventOp, HybridLogicalClock, MaterializedState,
    ModifyEventOp, NoOpReason, ObjectState, OperationEffect, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet, OperationStamp, PreconditionFailureReason, RepairKind,
    TransactionDescriptor, TupletCompensation, TupletCompensationKind, UndoPolicy,
    UndoTransactionPayload,
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

const A: ReplicaId = ReplicaId(21);
const B: ReplicaId = ReplicaId(22);

/// The import, the four quarters as the import made them, and the authors'
/// clock, which starts after the import's last stamp.
struct Measure {
    import: Import,
    quarters: [Event; 4],
}

impl Measure {
    fn new() -> Self {
        let import = import(MEASURE).expect("the measure imports");
        let mut set = OperationSet::new();
        set.accept_all(import.envelopes.clone());
        let score = set
            .reduce_onto(&Score::empty(IdentityContext::new(import.replica)))
            .score;
        let quarters = std::array::from_fn(|i| {
            score
                .events
                .get(import.ids.events[0][i])
                .expect("each quarter is imported")
                .clone()
        });
        Self { import, quarters }
    }

    fn q(&self, i: usize) -> EventId {
        self.quarters[i].id()
    }

    /// The causal context of an author who has seen the import and `also`.
    fn seen(&self, also: &[OperationId]) -> CausalContext {
        let mut context = CausalContext::new()
            .with_seen(self.import.replica, self.import.envelopes.len() as u64 - 1);
        for id in also {
            context = context.with_seen(id.replica, id.counter);
        }
        context
    }

    /// An operation by `author`, stamped `at` ticks after the import.
    fn op(
        &self,
        author: ReplicaId,
        counter: u64,
        at: i64,
        seen: &[OperationId],
        payload: OperationPayload,
    ) -> OperationEnvelope {
        let id = OperationId::new(author, counter);
        let physical = self.import.envelopes.len() as i64 + at;
        OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
            causal_context: self.seen(seen),
            transaction: None,
            payload,
        }
    }

    /// A tuplet over `members`, its total the members' durations as the
    /// import made them.
    fn tuplet(&self, counter: u64, members: &[usize]) -> OperationKind {
        let required_total = members
            .iter()
            .map(|i| musical(self.quarters[*i].duration()))
            .fold(MusicalDuration::zero(), |sum, d| sum + d);
        OperationKind::CreateTuplet(CreateTupletOp {
            tuplet: Tuplet {
                id: TupletId::new(A, counter),
                ratio: TupletRatio::new(3, 2).expect("not degenerate"),
                members: members.iter().map(|i| self.q(*i)).collect(),
                parent: None,
                required_total,
            },
        })
    }

    /// A rest in quarter `i`'s place, of `duration`.
    fn rest(&self, counter: u64, i: usize, duration: EventDuration) -> Rest {
        Rest {
            id: EventId::new(A, counter),
            voice: self.quarters[i].voice(),
            position: self.quarters[i].position().clone(),
            duration,
            vertical_position: None,
            visible: true,
        }
    }

    /// Quarter `i` at `duration`.
    fn trim(&self, i: usize, duration: EventDuration) -> OperationKind {
        let mut event = self.quarters[i].clone();
        match &mut event {
            Event::Pitched(e) => e.duration = duration,
            other => panic!("a note, not {other:?}"),
        }
        OperationKind::ModifyEvent(ModifyEventOp { event })
    }

    /// Reduces the import and `authored` both ways and holds the two
    /// reductions to each other: every effect, every object and the
    /// canonical bytes. Returns the base-free state, which then equals the
    /// graph-aware one.
    fn agree(&self, history: &str, authored: &[OperationEnvelope]) -> MaterializedState {
        let mut set = OperationSet::new();
        set.accept_all(self.import.envelopes.iter().chain(authored).cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(self.import.replica)));
        let violations = check_invariants(&aware.score);
        assert!(violations.is_empty(), "{history}: {violations:?}");
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
        free
    }
}

fn musical(duration: &EventDuration) -> MusicalDuration {
    match duration {
        EventDuration::Musical(d) => d.clone(),
        other => panic!("a metric event, not {other:?}"),
    }
}

fn eighth() -> EventDuration {
    EventDuration::Musical(MusicalDuration(RationalTime::new(1, 8).expect("an eighth")))
}

fn delete(event: EventId, tuplet_compensation: TupletCompensation) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
        event,
        tuplet_compensation,
    }))
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

fn compensated(state: &MaterializedState, id: OperationId) -> Vec<TupletCompensationKind> {
    match effect(state, id) {
        Some(OperationEffect::AppliedWithRepair { repairs }) => repairs
            .iter()
            .filter_map(|r| match r.kind {
                RepairKind::TupletCompensated { compensation_kind } => Some(compensation_kind),
                _ => None,
            })
            .collect(),
        other => panic!("expected an applied compensation, got {other:?}"),
    }
}

fn live(state: &MaterializedState, object: TypedObjectId) -> bool {
    matches!(state.objects.get(&object), Some(ObjectState::Live))
}

fn tombstoned(state: &MaterializedState, object: TypedObjectId) -> bool {
    matches!(
        state.objects.get(&object),
        Some(ObjectState::Tombstoned { .. })
    )
}

/// Every compensation a `DeleteEvent` declares, on a tuplet member and on an
/// event no tuplet holds, well-formed and not: each reduces alike in both
/// modes, to the verdict the catalog gives it.
#[test]
fn every_tuplet_compensation_reduces_alike_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let t1 = TypedObjectId::Tuplet(TupletId::new(A, 1));
    let t2 = TypedObjectId::Tuplet(TupletId::new(A, 2));
    let create = m.op(A, 0, 1, &[], primitive(m.tuplet(1, &[0, 1])));
    let after_create =
        |counter, compensation| m.op(A, counter, 2, &[create.id], delete(m.q(0), compensation));

    // An event no tuplet holds: no compensation needed, and any but a rest
    // of its own duration refused.
    let plain = m.op(
        A,
        0,
        1,
        &[],
        delete(m.q(2), TupletCompensation::NotInTuplet),
    );
    let state = m.agree("no tuplet, none declared", std::slice::from_ref(&plain));
    assert_eq!(effect(&state, plain.id), Some(OperationEffect::Applied));
    let rest = m.rest(1000, 2, quarter.clone());
    let replaced = m.op(
        A,
        0,
        1,
        &[],
        delete(
            m.q(2),
            TupletCompensation::ReplaceWithRest { rest: rest.clone() },
        ),
    );
    let state = m.agree(
        "no tuplet, a rest of its duration",
        std::slice::from_ref(&replaced),
    );
    assert_eq!(
        compensated(&state, replaced.id),
        vec![TupletCompensationKind::ReplaceWithRest]
    );
    assert!(live(&state, TypedObjectId::Event(rest.id)));
    for (history, compensation) in [
        (
            "no tuplet, a rest of another duration",
            TupletCompensation::ReplaceWithRest {
                rest: m.rest(1000, 2, eighth()),
            },
        ),
        (
            "no tuplet, a rewrite",
            TupletCompensation::RewriteTuplets {
                tuplets: vec![TupletId::new(A, 1)],
            },
        ),
        (
            "no tuplet, a cascade naming a tuplet",
            TupletCompensation::CascadeDeleteTuplets {
                tuplets: vec![TupletId::new(A, 1)],
            },
        ),
    ] {
        let op = m.op(A, 0, 1, &[], delete(m.q(2), compensation));
        let state = m.agree(history, std::slice::from_ref(&op));
        assert_eq!(
            effect(&state, op.id),
            refused(PreconditionFailureReason::TupletCompensationInvalid),
            "{history}"
        );
        assert!(live(&state, TypedObjectId::Event(m.q(2))), "{history}");
    }

    // A member of one tuplet.
    let rest = m.rest(1000, 0, quarter.clone());
    let replaced = after_create(
        1,
        TupletCompensation::ReplaceWithRest { rest: rest.clone() },
    );
    let state = m.agree(
        "a member, a rest of its duration",
        &[create.clone(), replaced.clone()],
    );
    assert_eq!(
        compensated(&state, replaced.id),
        vec![TupletCompensationKind::ReplaceWithRest]
    );
    assert!(live(&state, t1));
    // The rest is a member now, and its own delete needs compensation.
    let rest_deleted = m.op(
        A,
        2,
        3,
        &[replaced.id],
        delete(rest.id, TupletCompensation::NotInTuplet),
    );
    let state = m.agree(
        "a member's rest deleted without compensation",
        &[create.clone(), replaced.clone(), rest_deleted.clone()],
    );
    assert_eq!(
        effect(&state, rest_deleted.id),
        refused(PreconditionFailureReason::TupletCompensationInvalid)
    );
    let cascaded = after_create(
        1,
        TupletCompensation::CascadeDeleteTuplets {
            tuplets: vec![TupletId::new(A, 1)],
        },
    );
    let state = m.agree(
        "a member, a cascade naming its tuplet",
        &[create.clone(), cascaded.clone()],
    );
    assert_eq!(
        compensated(&state, cascaded.id),
        vec![TupletCompensationKind::CascadeDeleteTuplets]
    );
    assert!(tombstoned(&state, t1));
    for (history, compensation) in [
        ("a member, none declared", TupletCompensation::NotInTuplet),
        (
            "a member, a rest of another duration",
            TupletCompensation::ReplaceWithRest {
                rest: m.rest(1000, 0, eighth()),
            },
        ),
        (
            "a member, a rest of no musical duration",
            TupletCompensation::ReplaceWithRest {
                rest: m.rest(1000, 0, EventDuration::WallClock(WallClockDuration(500))),
            },
        ),
        (
            "a member, a rewrite",
            TupletCompensation::RewriteTuplets {
                tuplets: vec![TupletId::new(A, 1)],
            },
        ),
        (
            "a member, an empty cascade",
            TupletCompensation::CascadeDeleteTuplets { tuplets: vec![] },
        ),
        (
            "a member, a cascade naming another tuplet",
            TupletCompensation::CascadeDeleteTuplets {
                tuplets: vec![TupletId::new(A, 2)],
            },
        ),
    ] {
        let op = after_create(1, compensation);
        let state = m.agree(history, &[create.clone(), op.clone()]);
        assert_eq!(
            effect(&state, op.id),
            refused(PreconditionFailureReason::TupletCompensationInvalid),
            "{history}"
        );
        assert!(live(&state, TypedObjectId::Event(m.q(0))), "{history}");
        assert!(live(&state, t1), "{history}");
    }

    // A member of two tuplets: a cascade names both, and a rest takes the
    // member's place in both.
    let second = m.op(A, 1, 2, &[create.id], primitive(m.tuplet(2, &[0, 1])));
    let both_made =
        |counter, compensation| m.op(A, counter, 3, &[second.id], delete(m.q(0), compensation));
    let partial = both_made(
        2,
        TupletCompensation::CascadeDeleteTuplets {
            tuplets: vec![TupletId::new(A, 1)],
        },
    );
    let state = m.agree(
        "a member of two, a cascade naming one",
        &[create.clone(), second.clone(), partial.clone()],
    );
    assert_eq!(
        effect(&state, second.id),
        Some(OperationEffect::Applied),
        "a second tuplet over the same members mints"
    );
    assert_eq!(
        effect(&state, partial.id),
        refused(PreconditionFailureReason::TupletCompensationInvalid)
    );
    let whole = both_made(
        2,
        TupletCompensation::CascadeDeleteTuplets {
            tuplets: vec![TupletId::new(A, 1), TupletId::new(A, 2)],
        },
    );
    let state = m.agree(
        "a member of two, a cascade naming both",
        &[create.clone(), second.clone(), whole.clone()],
    );
    assert!(tombstoned(&state, t1) && tombstoned(&state, t2));
    let rest = m.rest(1000, 0, quarter);
    let replaced = both_made(
        2,
        TupletCompensation::ReplaceWithRest { rest: rest.clone() },
    );
    let rest_deleted = m.op(
        A,
        3,
        4,
        &[replaced.id],
        delete(
            rest.id,
            TupletCompensation::CascadeDeleteTuplets {
                tuplets: vec![TupletId::new(A, 1), TupletId::new(A, 2)],
            },
        ),
    );
    let state = m.agree(
        "a member of two, a rest, then the rest cascading both",
        &[
            create.clone(),
            second.clone(),
            replaced.clone(),
            rest_deleted.clone(),
        ],
    );
    assert_eq!(
        compensated(&state, replaced.id),
        vec![TupletCompensationKind::ReplaceWithRest]
    );
    assert_eq!(
        compensated(&state, rest_deleted.id),
        vec![
            TupletCompensationKind::CascadeDeleteTuplets,
            TupletCompensationKind::CascadeDeleteTuplets
        ],
        "the rest is a member of both"
    );
}

/// Author B trims the first quarter to an eighth; author A, who has not seen
/// the trim, makes the first two quarters a tuplet and replaces the first
/// with a quarter rest, as a member's delete must. Wherever the trim falls in
/// canonical order, both modes reach one verdict. Trimmed first, the tuplet
/// cannot fill its total and the rest no longer matches its note, so both
/// refuse; reduction version 2 refuses the rest base-free as graph-aware
/// reduction always has.
#[test]
fn a_rest_made_stale_by_a_concurrent_trim_is_refused_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let rest = m.rest(1000, 0, quarter.clone());
    let create = |at| m.op(A, 0, at, &[], primitive(m.tuplet(1, &[0, 1])));
    let replace = |at, create: &OperationEnvelope| {
        m.op(
            A,
            1,
            at,
            &[create.id],
            delete(
                m.q(0),
                TupletCompensation::ReplaceWithRest { rest: rest.clone() },
            ),
        )
    };
    let trim = |at| m.op(B, 0, at, &[], primitive(m.trim(0, eighth())));

    // The trim first: the create and the rest refused, the note still live.
    let (c, t) = (create(2), trim(1));
    let r = replace(4, &c);
    let later = m.op(B, 1, 5, &[c.id, r.id, t.id], primitive(m.trim(0, quarter)));
    let state = m.agree(
        "the trim before the tuplet",
        &[t.clone(), c.clone(), r.clone(), later.clone()],
    );
    assert_eq!(effect(&state, t.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, c.id),
        refused(PreconditionFailureReason::EventDurationInvalid)
    );
    assert_eq!(
        effect(&state, r.id),
        refused(PreconditionFailureReason::TupletCompensationInvalid)
    );
    assert_eq!(effect(&state, later.id), Some(OperationEffect::Applied));
    assert!(live(&state, TypedObjectId::Event(m.q(0))));
    assert_eq!(state.objects.get(&TypedObjectId::Event(rest.id)), None);

    // The trim between them: a member's duration change refused, the rest
    // applied.
    let (c, t) = (create(1), trim(2));
    let r = replace(4, &c);
    let state = m.agree(
        "the trim between the tuplet and the rest",
        &[c.clone(), t.clone(), r.clone()],
    );
    assert_eq!(
        effect(&state, t.id),
        refused(PreconditionFailureReason::EventDurationInvalid)
    );
    assert_eq!(
        compensated(&state, r.id),
        vec![TupletCompensationKind::ReplaceWithRest]
    );

    // The trim last: its note is gone.
    let (c, t) = (create(1), trim(5));
    let r = replace(3, &c);
    let state = m.agree(
        "the trim after the rest",
        &[c.clone(), r.clone(), t.clone()],
    );
    assert_eq!(
        effect(&state, t.id),
        Some(OperationEffect::NoOp {
            reason: NoOpReason::TargetTombstoned
        })
    );
    assert!(live(&state, TypedObjectId::Event(rest.id)));
}

/// Two authors compensate concurrently against one tuplet: A replaces the
/// first member with a rest while B deletes the second and cascades the
/// tuplet, and B deletes the first member while A replaces it. In either
/// canonical order both modes agree.
#[test]
fn concurrent_compensations_reduce_alike_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let t1 = TypedObjectId::Tuplet(TupletId::new(A, 1));
    let create = m.op(A, 0, 1, &[], primitive(m.tuplet(1, &[0, 1])));
    let rest = m.rest(1000, 0, quarter);
    let replace = |at| {
        m.op(
            A,
            1,
            at,
            &[create.id],
            delete(
                m.q(0),
                TupletCompensation::ReplaceWithRest { rest: rest.clone() },
            ),
        )
    };

    for (a_first, history) in [
        (true, "a rest, then a concurrent cascade"),
        (false, "a cascade, then a concurrent rest"),
    ] {
        let (a_at, b_at) = if a_first { (2, 3) } else { (3, 2) };
        let r = replace(a_at);
        let cascade = m.op(
            B,
            0,
            b_at,
            &[create.id],
            delete(
                m.q(1),
                TupletCompensation::CascadeDeleteTuplets {
                    tuplets: vec![TupletId::new(A, 1)],
                },
            ),
        );
        let state = m.agree(history, &[create.clone(), r.clone(), cascade.clone()]);
        assert_eq!(
            compensated(&state, cascade.id),
            vec![TupletCompensationKind::CascadeDeleteTuplets],
            "{history}"
        );
        assert!(tombstoned(&state, t1), "{history}");
        assert_eq!(
            compensated(&state, r.id),
            vec![TupletCompensationKind::ReplaceWithRest],
            "{history}: a rest of the note's duration replaces it, tuplet or none"
        );
    }

    for (a_first, history) in [
        (true, "a rest, then a concurrent plain delete"),
        (false, "a plain delete, then a concurrent rest"),
    ] {
        let (a_at, b_at) = if a_first { (2, 3) } else { (3, 2) };
        let r = replace(a_at);
        let plain = m.op(
            B,
            0,
            b_at,
            &[create.id],
            delete(m.q(0), TupletCompensation::NotInTuplet),
        );
        let state = m.agree(history, &[create.clone(), r.clone(), plain.clone()]);
        let (first, second) = if a_first { (&r, &plain) } else { (&plain, &r) };
        if a_first {
            assert_eq!(
                compensated(&state, first.id),
                vec![TupletCompensationKind::ReplaceWithRest],
                "{history}"
            );
        } else {
            assert_eq!(
                effect(&state, first.id),
                refused(PreconditionFailureReason::TupletCompensationInvalid),
                "{history}: a member's delete without compensation"
            );
        }
        if a_first {
            assert_eq!(
                effect(&state, second.id),
                Some(OperationEffect::NoOp {
                    reason: NoOpReason::AlreadyApplied
                }),
                "{history}"
            );
        } else {
            assert_eq!(
                compensated(&state, second.id),
                vec![TupletCompensationKind::ReplaceWithRest],
                "{history}"
            );
        }
        assert!(live(&state, t1), "{history}");
    }
}

/// An undo that removes a member cascades its tuplet, as no compensation
/// could be declared for it: undoing the transaction that replaced a member
/// with a rest removes the rest and the tuplet, alike in both modes.
#[test]
fn an_undone_replacement_cascades_alike_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let t1 = TypedObjectId::Tuplet(TupletId::new(A, 1));
    let rest = m.rest(1000, 0, quarter);
    let tx = TransactionId::new(A, 900);
    let create = m.op(A, 0, 1, &[], primitive(m.tuplet(1, &[0, 1])));
    let mut declare = m.op(
        A,
        1,
        2,
        &[create.id],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("replace"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut replaced = m.op(
        A,
        2,
        3,
        &[declare.id],
        delete(
            m.q(0),
            TupletCompensation::ReplaceWithRest { rest: rest.clone() },
        ),
    );
    replaced.transaction = Some(tx);
    let undo = m.op(
        A,
        3,
        4,
        &[replaced.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let state = m.agree(
        "a replacement undone",
        &[create, declare, replaced.clone(), undo.clone()],
    );
    assert_eq!(
        compensated(&state, replaced.id),
        vec![TupletCompensationKind::ReplaceWithRest]
    );
    assert!(tombstoned(&state, TypedObjectId::Event(rest.id)));
    assert!(tombstoned(&state, t1), "the undo cascades the tuplet");
}
