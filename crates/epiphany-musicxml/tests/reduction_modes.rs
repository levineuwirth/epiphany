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
//! an undo makes when it removes a member. Others migrate the region's time
//! model: a `Reassign` remapping read by later edits, a concurrent insert the
//! migration must find, and an insert into a region a migration made
//! non-metric, alone, rolled back and into a deleted voice. Each history also
//! asserts the verdict it exists for, so agreement cannot hold by both modes
//! refusing everything.

use epiphany_core::{
    check_invariants, Event, EventDuration, EventId, EventPosition, GraphInvariant,
    IdentityContext, MusicalDuration, MusicalPosition, OperationId, RationalTime, RegionId,
    RegionTimeModel, ReplicaId, Rest, Score, TransactionId, Tuplet, TupletId, TupletRatio,
    TypedObjectId, ViolationKind, WallClockDuration, WallClockTime, WellFormednessViolation,
};
use epiphany_musicxml::{import, Import};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, ChangeRegionTimeModelOp, ConflictKind, CreateTupletOp,
    DeleteEventOp, DeleteVoiceOp, HybridLogicalClock, InsertEventOp, MaterializedState,
    ModifyEventOp, NoOpReason, ObjectState, OperationEffect, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet, OperationStamp, PositionRemapping, PreconditionFailureReason,
    RepairKind, TransactionDescriptor, TupletCompensation, TupletCompensationKind, UndoPolicy,
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

/// The import, the four quarters as the import made them, the region and its
/// time model, and the authors' clock, which starts after the import's last
/// stamp.
struct Measure {
    import: Import,
    quarters: [Event; 4],
    region: RegionId,
    model: RegionTimeModel,
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
        let region = score.canvas.regions[0].id;
        let model = score.canvas.regions[0].time_model.clone();
        Self {
            import,
            quarters,
            region,
            model,
        }
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
                display: Default::default(),
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

    /// Quarter `i`'s position as the import made it.
    fn position(&self, i: usize) -> MusicalPosition {
        match self.quarters[i].position() {
            EventPosition::Musical(position) => position.clone(),
            other => panic!("a metric event, not {other:?}"),
        }
    }

    /// The region migrated to `model`, positions kept.
    fn migrate(&self, model: RegionTimeModel) -> OperationPayload {
        primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region: self.region,
                new_time_model: model,
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::PreserveTime,
            },
        ))
    }

    /// The region kept metric, each `(quarter, slot)` pair moving the quarter
    /// to where the import put quarter `slot`.
    fn reassign(&self, pairs: &[(usize, usize)]) -> OperationPayload {
        primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region: self.region,
                new_time_model: self.model.clone(),
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::Reassign(
                    pairs
                        .iter()
                        .map(|(quarter, slot)| (self.q(*quarter), self.position(*slot)))
                        .collect(),
                ),
            },
        ))
    }

    /// `rest` inserted into the measure's staff.
    fn insert(&self, rest: Rest) -> OperationPayload {
        primitive(OperationKind::InsertEvent(InsertEventOp {
            staff_instance: self.import.ids.instances[0][0],
            event: Event::Rest(rest),
        }))
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
    /// canonical bytes, with the graph-aware score keeping every invariant.
    /// Returns the base-free state, which then equals the graph-aware one.
    fn agree(&self, history: &str, authored: &[OperationEnvelope]) -> MaterializedState {
        let (free, violations) = self.compare(history, authored);
        assert!(violations.is_empty(), "{history}: {violations:?}");
        free
    }

    /// As [`Self::agree`], for a history whose graph-aware score breaks an
    /// invariant: returns the base-free state and the violations.
    fn compare(
        &self,
        history: &str,
        authored: &[OperationEnvelope],
    ) -> (MaterializedState, Vec<WellFormednessViolation>) {
        let mut set = OperationSet::new();
        set.accept_all(self.import.envelopes.iter().chain(authored).cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(self.import.replica)));
        let violations = check_invariants(&aware.score);
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
        (free, violations)
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

/// The record a conflicted migration names, as its incompatible events.
fn migration_failure(state: &MaterializedState, id: OperationId) -> Vec<TypedObjectId> {
    let Some(OperationEffect::Conflicted { conflict }) = effect(state, id) else {
        panic!(
            "expected a conflicted migration, got {:?}",
            effect(state, id)
        );
    };
    let record = state
        .conflicts
        .records()
        .iter()
        .find(|record| record.id == conflict)
        .expect("the conflict is recorded");
    match &record.kind {
        ConflictKind::TimeModelMigrationFailure {
            incompatible_events,
            ..
        } => incompatible_events.clone(),
        other => panic!("expected a migration failure, got {other:?}"),
    }
}

/// One author deletes the fourth quarter and shifts the other three a beat
/// later with a `Reassign` remapping. The remapping moves the occupancy index
/// in both modes, so a rest on the freed first beat, and the moved first
/// quarter trimmed where it now stands, each apply in both.
#[test]
fn a_reassigned_measure_is_read_at_its_new_placements_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let deleted = m.op(
        A,
        0,
        1,
        &[],
        delete(m.q(3), TupletCompensation::NotInTuplet),
    );
    let shifted = m.op(
        A,
        1,
        2,
        &[deleted.id],
        m.reassign(&[(0, 1), (1, 2), (2, 3)]),
    );
    let state = m.agree("the shift alone", &[deleted.clone(), shifted.clone()]);
    assert_eq!(effect(&state, shifted.id), Some(OperationEffect::Applied));

    let rest = m.rest(2000, 0, quarter);
    let inserted = m.op(A, 2, 3, &[deleted.id, shifted.id], m.insert(rest.clone()));
    let state = m.agree(
        "a rest on the freed first beat",
        &[deleted.clone(), shifted.clone(), inserted.clone()],
    );
    assert_eq!(effect(&state, inserted.id), Some(OperationEffect::Applied));
    assert!(live(&state, TypedObjectId::Event(rest.id)));

    let mut moved = m.quarters[0].clone();
    match &mut moved {
        Event::Pitched(e) => {
            e.position = EventPosition::Musical(m.position(1));
            e.duration = eighth();
        }
        other => panic!("a note, not {other:?}"),
    }
    let trimmed = m.op(
        A,
        2,
        3,
        &[deleted.id, shifted.id],
        primitive(OperationKind::ModifyEvent(ModifyEventOp { event: moved })),
    );
    let state = m.agree(
        "the moved first quarter trimmed where it stands",
        &[deleted, shifted, trimmed.clone()],
    );
    assert_eq!(effect(&state, trimmed.id), Some(OperationEffect::Applied));
}

/// A migration finds the region's events from the indices both modes keep.
/// B, concurrently and stamped first, replaces the fourth quarter with a rest
/// while A remaps the four quarters A has seen to their own places: B's rest
/// is in the region and not in A's remapping, so the migration conflicts in
/// both modes, naming it. A proportional target conflicts in both over the
/// four quarters themselves.
#[test]
fn a_migration_finds_its_regions_events_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let deleted = m.op(
        B,
        0,
        1,
        &[],
        delete(m.q(3), TupletCompensation::NotInTuplet),
    );
    let rest = Rest {
        id: EventId::new(B, 3000),
        ..m.rest(3000, 3, quarter)
    };
    let inserted = m.op(B, 1, 2, &[deleted.id], m.insert(rest.clone()));
    let remapped = m.op(A, 0, 3, &[], m.reassign(&[(0, 0), (1, 1), (2, 2), (3, 3)]));
    let state = m.agree(
        "a remapping concurrent with a new rest",
        &[deleted, inserted.clone(), remapped.clone()],
    );
    assert_eq!(effect(&state, inserted.id), Some(OperationEffect::Applied));
    assert_eq!(
        migration_failure(&state, remapped.id),
        vec![TypedObjectId::Event(rest.id)]
    );

    let proportional = m.op(A, 0, 1, &[], m.migrate(valuegen::proportional_model()));
    let state = m.agree(
        "the quarters against a proportional target",
        std::slice::from_ref(&proportional),
    );
    let mut quarters: Vec<TypedObjectId> = (0..4).map(|i| TypedObjectId::Event(m.q(i))).collect();
    quarters.sort();
    assert_eq!(migration_failure(&state, proportional.id), quarters);
}

/// An insert reads the region's time model alike in both modes. A makes the
/// region aleatoric, which admits every event it holds, while B, concurrently
/// and stamped after, replaces the fourth quarter with a rest: the region is
/// no longer metric, so the insert is refused in both. A migration a failing
/// transaction rolls back leaves the region metric in both, so the same insert
/// applies; and an insert into a voice deleted before the migration is refused
/// as a missing voice in both.
#[test]
fn an_insert_reads_its_regions_time_model_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let rest = Rest {
        id: EventId::new(B, 3000),
        ..m.rest(3000, 3, quarter)
    };
    let b_deleted = m.op(
        B,
        0,
        10,
        &[],
        delete(m.q(3), TupletCompensation::NotInTuplet),
    );
    let b_inserted = m.op(B, 1, 11, &[b_deleted.id], m.insert(rest.clone()));

    let migrated = m.op(A, 0, 1, &[], m.migrate(valuegen::aleatoric_model()));
    let state = m.agree(
        "a rest after a concurrent migration",
        &[migrated.clone(), b_deleted.clone(), b_inserted.clone()],
    );
    assert_eq!(effect(&state, migrated.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, b_inserted.id),
        refused(PreconditionFailureReason::WrongRegionTimeModel)
    );

    let tx = TransactionId::new(A, 900);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("migrate"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut migrated = m.op(
        A,
        1,
        2,
        &[declare.id],
        m.migrate(valuegen::aleatoric_model()),
    );
    migrated.transaction = Some(tx);
    let mut failing = m.op(
        A,
        2,
        3,
        &[migrated.id],
        delete(EventId::new(A, 4000), TupletCompensation::NotInTuplet),
    );
    failing.transaction = Some(tx);
    let state = m.agree(
        "a rest after a migration rolled back",
        &[
            declare,
            migrated.clone(),
            failing,
            b_deleted.clone(),
            b_inserted.clone(),
        ],
    );
    assert_eq!(
        effect(&state, migrated.id),
        Some(OperationEffect::NoOp {
            reason: NoOpReason::TransactionConflict,
        })
    );
    assert_eq!(
        effect(&state, b_inserted.id),
        Some(OperationEffect::Applied)
    );

    let voice = m.quarters[0].voice();
    let mut emptied: Vec<OperationEnvelope> = (0..4)
        .map(|i| {
            m.op(
                A,
                i as u64,
                1 + i as i64,
                &[],
                delete(m.q(i), TupletCompensation::NotInTuplet),
            )
        })
        .collect();
    let seen: Vec<OperationId> = emptied.iter().map(|op| op.id).collect();
    let voice_deleted = m.op(
        A,
        4,
        5,
        &seen,
        primitive(OperationKind::DeleteVoice(DeleteVoiceOp { voice })),
    );
    let migrated = m.op(
        A,
        5,
        6,
        &[voice_deleted.id],
        m.migrate(valuegen::aleatoric_model()),
    );
    emptied.extend([
        voice_deleted.clone(),
        migrated.clone(),
        b_deleted,
        b_inserted.clone(),
    ]);
    let state = m.agree("a rest into a voice deleted before a migration", &emptied);
    assert_eq!(
        effect(&state, voice_deleted.id),
        Some(OperationEffect::Applied)
    );
    assert_eq!(effect(&state, migrated.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, b_inserted.id),
        refused(PreconditionFailureReason::VoiceMissing)
    );
}

/// A migration judges every event the occupancy index holds by its indexed
/// placement, in both modes. An insert carrying a wall-clock position into the
/// metric region is admitted today, against invariant 4, and indexed at the
/// region's origin; a metric target then admits it in both modes, where
/// graph-aware reduction once judged it from the graph and conflicted.
#[test]
fn a_migration_judges_an_indexed_event_by_its_placement_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let deleted = m.op(
        A,
        0,
        1,
        &[],
        delete(m.q(0), TupletCompensation::NotInTuplet),
    );
    let rest = Rest {
        position: EventPosition::WallClock(WallClockTime(0)),
        ..m.rest(2000, 0, quarter)
    };
    let inserted = m.op(A, 1, 2, &[deleted.id], m.insert(rest));
    let migrated = m.op(
        A,
        2,
        3,
        &[deleted.id, inserted.id],
        m.migrate(m.model.clone()),
    );
    let (state, violations) = m.compare(
        "a wall-clock rest in the metric region, then a metric target",
        &[deleted, inserted.clone(), migrated.clone()],
    );
    assert_eq!(effect(&state, inserted.id), Some(OperationEffect::Applied));
    assert!(violations.iter().all(|violation| matches!(
        violation.kind,
        ViolationKind::Invariant(GraphInvariant::EventCoordinateModel)
    )));
    assert!(!violations.is_empty());
    assert_eq!(effect(&state, migrated.id), Some(OperationEffect::Applied));
}

/// Two quarter-tone quarters on one line, untied, as a file writes them:
/// each with its own accidental, since one applies to its own note alone.
const QUARTER_TONES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>1</divisions><key><fifths>0</fifths></key>
        <time><beats>2</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef></attributes>
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type><accidental>flat-up</accidental></note>
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type><accidental>flat-up</accidental></note>
    </measure>
  </part>
</score-partwise>
"#;

/// A tie between two quarter-tones applies in both reduction modes, and the
/// score it leaves keeps every invariant, the tie's pairing among them. No
/// reduction checks a tie's pitches, so the verdict is the one reduction gave
/// before the core decided quarter-tone equality; what changed is the
/// invariant the score is held to, which once called the tie unpaired.
#[test]
fn a_tie_between_quarter_tones_applies_and_pairs_in_both_modes() {
    use epiphany_core::{Tie, TieClass, TieId};
    use epiphany_ops::{CreateCrossCuttingOp, CrossCuttingValue};

    let import = import(QUARTER_TONES).expect("the measure imports");
    let events = &import.ids.events[0];
    assert_eq!(events.len(), 2, "two quarters");
    let tie = OperationId::new(A, 1);
    let envelope = OperationEnvelope {
        id: tie,
        author: AuthorId(0),
        stamp: OperationStamp::new(
            HybridLogicalClock::new(WallClockTime(import.envelopes.len() as i64 + 1), 0),
            tie,
        ),
        causal_context: CausalContext::new()
            .with_seen(import.replica, import.envelopes.len() as u64 - 1),
        transaction: None,
        payload: primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: CrossCuttingValue::Tie(Tie {
                id: TieId::new(A, 1),
                start_event: events[0],
                end_event: events[1],
                pitch_pairing: None,
                class: TieClass::Standard,
                style: Default::default(),
            }),
        })),
    };
    let mut set = OperationSet::new();
    set.accept_all(import.envelopes.iter().cloned().chain([envelope]));
    let free = set.reduce();
    let aware = set.reduce_onto(&Score::empty(IdentityContext::new(import.replica)));
    assert_eq!(effect(&free, tie), Some(OperationEffect::Applied));
    assert_eq!(effect(&aware.state, tie), Some(OperationEffect::Applied));
    assert!(free.canonical_bytes() == aware.state.canonical_bytes());
    assert_eq!(aware.score.cross_cutting.ties.len(), 1);
    let violations = check_invariants(&aware.score);
    assert!(violations.is_empty(), "{violations:?}");
}

/// A tuplet's display (schema major 4) is held in the graph alone: one
/// history with the tuplet shown and with it hidden, a member then deleted
/// with the tuplet rewritten, reduces to the same effects, objects and
/// canonical bytes in both modes, and the graph-aware scores differ only in
/// the tuplet's display. So schema major 4 changes no reduction verdict and
/// no canonical reduced state.
#[test]
fn a_tuplets_display_changes_no_verdict_or_canonical_state() {
    use epiphany_core::TupletDisplay;
    let m = Measure::new();
    let history = |display: TupletDisplay| {
        let OperationKind::CreateTuplet(mut op) = m.tuplet(1, &[0, 1, 2]) else {
            unreachable!("a CreateTuplet")
        };
        op.tuplet.display = display;
        let create = m.op(A, 0, 1, &[], primitive(OperationKind::CreateTuplet(op)));
        let removed = m.op(
            A,
            1,
            2,
            &[create.id],
            delete(
                m.q(2),
                TupletCompensation::RewriteTuplets {
                    tuplets: vec![TupletId::new(A, 1)],
                },
            ),
        );
        vec![create, removed]
    };
    let (shown, hidden) = (
        history(TupletDisplay::default()),
        history(TupletDisplay::HIDDEN),
    );
    let free_shown = m.agree("shown", &shown);
    let free_hidden = m.agree("hidden", &hidden);
    for envelope in &shown {
        assert_eq!(
            effect(&free_shown, envelope.id),
            effect(&free_hidden, envelope.id)
        );
    }
    assert_eq!(
        effect(&free_shown, shown[0].id),
        Some(OperationEffect::Applied)
    );
    assert_eq!(free_shown.canonical_bytes(), free_hidden.canonical_bytes());
    let aware = |authored: &[OperationEnvelope]| {
        let mut set = OperationSet::new();
        set.accept_all(m.import.envelopes.iter().chain(authored).cloned());
        set.reduce_onto(&Score::empty(IdentityContext::new(m.import.replica)))
    };
    let (a, b) = (aware(&shown), aware(&hidden));
    assert_eq!(a.state.canonical_bytes(), b.state.canonical_bytes());
    assert_eq!(b.score.cross_cutting.tuplets.len(), 1);
    assert_eq!(
        b.score.cross_cutting.tuplets[0].display,
        TupletDisplay::HIDDEN
    );
    let mut unhidden = b.score.clone();
    unhidden.cross_cutting.tuplets[0].display = TupletDisplay::default();
    assert_eq!(a.score, unhidden);
}

/// Two authors each replace the same imported quarter with a rest of their
/// own, neither having seen the other: the two rests collide in the quarter's
/// voice, and the later is promoted to a system voice, in both modes. Before
/// reduction version 3 the promotion pre-pass took only inserts whose voice
/// the graph held before anything applied, so over the importer's empty base
/// graph-aware reduction promoted nothing and refused the second rest.
#[test]
fn two_replacements_of_one_quarter_promote_alike_in_both_modes() {
    let m = Measure::new();
    let quarter = m.quarters[0].duration().clone();
    let rest_a = m.rest(1000, 0, quarter.clone());
    let mut rest_b = m.rest(1000, 0, quarter);
    rest_b.id = EventId::new(B, 1000);
    let delete_a = m.op(
        A,
        0,
        1,
        &[],
        delete(m.q(0), TupletCompensation::NotInTuplet),
    );
    let insert_a = m.op(A, 1, 2, &[delete_a.id], m.insert(rest_a.clone()));
    let delete_b = m.op(
        B,
        0,
        1,
        &[],
        delete(m.q(0), TupletCompensation::NotInTuplet),
    );
    let insert_b = m.op(B, 1, 2, &[delete_b.id], m.insert(rest_b.clone()));
    let state = m.agree(
        "two replacements of one quarter",
        &[delete_a, insert_a.clone(), delete_b, insert_b.clone()],
    );
    assert_eq!(effect(&state, insert_a.id), Some(OperationEffect::Applied));
    let Some(OperationEffect::AppliedWithRepair { repairs }) = effect(&state, insert_b.id) else {
        panic!("the later rest is promoted");
    };
    assert!(repairs
        .iter()
        .any(|r| matches!(r.kind, RepairKind::VoicePromoted { .. })));
    assert!(live(&state, TypedObjectId::Event(rest_a.id)));
    assert!(live(&state, TypedObjectId::Event(rest_b.id)));
}
