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
    check_invariants, Event, EventDuration, EventId, EventPosition, IdentityContext,
    MusicalDuration, MusicalPosition, OperationId, RationalTime, RegionId, RegionTimeModel,
    ReplicaId, Rest, Score, TransactionId, Tuplet, TupletId, TupletRatio, TypedObjectId,
    WallClockDuration, WallClockTime, WellFormednessViolation,
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
        Self::of(MEASURE)
    }

    fn of(xml: &str) -> Self {
        let import = import(xml).expect("the measure imports");
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

    /// `region` migrated to `model`, positions kept.
    fn migrate_region(&self, region: RegionId, model: RegionTimeModel) -> OperationPayload {
        primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region,
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
    // The quarters, and since reduction version 3 the measure and the staff
    // instance with its clef and key, anchored in musical time, which a
    // proportional region does not admit.
    let mut stranded: Vec<TypedObjectId> = (0..4).map(|i| TypedObjectId::Event(m.q(i))).collect();
    stranded.push(TypedObjectId::StaffInstance(m.import.ids.instances[0][0]));
    stranded.push(TypedObjectId::Measure(m.import.ids.measures[0][0][0]));
    stranded.sort();
    assert_eq!(migration_failure(&state, proportional.id), stranded);
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
/// metric region was admitted, against invariant 4, and indexed at the
/// region's origin, where a metric target admitted it in both modes and
/// graph-aware reduction once judged it from the graph and conflicted. Since
/// reduction version 3 the insert is refused, so the graph keeps every
/// invariant and the migration applies.
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
    let state = m.agree(
        "a wall-clock rest in the metric region, then a metric target",
        &[deleted, inserted.clone(), migrated.clone()],
    );
    assert_eq!(
        effect(&state, inserted.id),
        refused(PreconditionFailureReason::WrongRegionTimeModel)
    );
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

/// An imported quarter-tone, which the importer spells as its file does, is
/// transposed alike in both modes, and its spelling moves with it, keeping
/// its arrow. Before reduction version 3 graph-aware reduction refused the
/// transpose, finding the authored spelling it could not rewrite, while
/// base-free reduction, which holds no spelling, applied it.
#[test]
fn an_imported_quarter_tone_transposes_alike_in_both_modes() {
    use epiphany_core::{
        AccidentalId, CmnNominal, PitchSpacePosition, PitchSpelling, SpellingDirective,
        SpellingNominal, SpellingScope, TranspositionInterval,
    };
    use epiphany_ops::TransposeIntervalOp;

    let import = import(QUARTER_TONES).expect("the measure imports");
    let pitch = import.ids.pitches[0][0][0];
    let transpose = OperationId::new(A, 0);
    let envelope = OperationEnvelope {
        id: transpose,
        author: AuthorId(0),
        stamp: OperationStamp::new(
            HybridLogicalClock::new(WallClockTime(import.envelopes.len() as i64 + 1), 0),
            transpose,
        ),
        causal_context: CausalContext::new()
            .with_seen(import.replica, import.envelopes.len() as u64 - 1),
        transaction: None,
        // Up a major second: a step, and four quarter-tones in `cmn-24`.
        payload: primitive(OperationKind::TransposeInterval(TransposeIntervalOp {
            targets: [pitch].into_iter().collect(),
            interval: TranspositionInterval {
                diatonic_steps: 1,
                chromatic_steps: 4,
            },
        })),
    };
    let mut set = OperationSet::new();
    set.accept_all(import.envelopes.iter().cloned().chain([envelope]));
    let free = set.reduce();
    let aware = set.reduce_onto(&Score::empty(IdentityContext::new(import.replica)));
    assert_eq!(effect(&free, transpose), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&aware.state, transpose),
        Some(OperationEffect::Applied)
    );
    assert_eq!(free.objects, aware.state.objects);
    assert!(free.canonical_bytes() == aware.state.canonical_bytes());
    let violations = check_invariants(&aware.score);
    assert!(violations.is_empty(), "{violations:?}");

    let Some(Event::Pitched(event)) = aware.score.events.get(import.ids.events[0][0]) else {
        panic!("the first quarter");
    };
    assert_eq!(
        event.pitches[0].pitch.scale_position.position,
        PitchSpacePosition::Cmn {
            nominal: CmnNominal::A,
            alteration: -1,
            octave: 4,
        }
    );
    let spelt: Vec<&PitchSpelling> = aware
        .score
        .spelling_attachments
        .iter()
        .filter(|a| matches!(&a.scope, SpellingScope::Pitch(p) if *p == pitch))
        .filter_map(|a| match &a.directive {
            SpellingDirective::Explicit(s) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        spelt
            .iter()
            .any(|s| s.nominal == SpellingNominal::Cmn(CmnNominal::A)
                && s.accidentals == vec![AccidentalId::new("flat-up")]),
        "the spelling moves to A flat-up: {spelt:?}"
    );
}

/// A region one author makes and another deletes while the first, unaware,
/// migrates it: the migration names a region the history minted and lost,
/// which graph-aware reduction finds missing. Before reduction version 3
/// base-free reduction, which has no universe, applied it; it now refuses a
/// referent the set itself mints that is not live, and still takes as live one
/// no envelope mints, which may come from a base.
#[test]
fn a_region_the_history_made_and_deleted_is_missing_in_both_modes() {
    use epiphany_ops::{CreateRegionOp, DeleteRegionOp};
    let m = Measure::new();
    let region = RegionId::new(A, 500);
    let create = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(region),
        })),
    );
    let delete = m.op(
        B,
        0,
        2,
        &[create.id],
        primitive(OperationKind::DeleteRegion(DeleteRegionOp { region })),
    );
    let migrate = m.op(
        A,
        1,
        3,
        &[create.id],
        primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region,
                new_time_model: valuegen::proportional_model(),
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::PreserveTime,
            },
        )),
    );
    let state = m.agree(
        "a region made, deleted and migrated",
        &[create, delete.clone(), migrate.clone()],
    );
    assert_eq!(effect(&state, delete.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, migrate.id),
        refused(PreconditionFailureReason::TargetMissing)
    );
}

/// An instrument made in a transaction that fails: its author empties a voice
/// in the same transaction while another author, concurrently, enters a rest
/// in it, so the transaction's delete finds the voice full and the instrument
/// never comes to be. Its author, who saw the transaction apply, then names
/// the instrument from a new staff. Graph-aware reduction finds it missing;
/// base-free reduction, since version 3, refuses it too, the set itself
/// minting the instrument, where it applied.
#[test]
fn an_instrument_minted_by_a_failed_transaction_is_missing_in_both_modes() {
    use epiphany_core::{InstrumentId, StaffId, VoiceId};
    use epiphany_ops::{CreateInstrumentOp, CreateStaffOp, CreateVoiceOp};
    let m = Measure::new();
    let voice = VoiceId::new(A, 600);
    let instrument = InstrumentId::new(A, 601);
    let tx = TransactionId::new(A, 602);
    let create_voice = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: m.import.ids.instances[0][0],
            voice: valuegen::voice(voice),
        })),
    );
    let mut rest = m.rest(603, 0, m.quarters[0].duration().clone());
    rest.id = EventId::new(B, 603);
    rest.voice = voice;
    let fill = m.op(B, 0, 2, &[create_voice.id], m.insert(rest));
    let mut declare = m.op(
        A,
        1,
        3,
        &[create_voice.id],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("instrument"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut mint = m.op(
        A,
        2,
        4,
        &[declare.id],
        primitive(OperationKind::CreateInstrument(CreateInstrumentOp {
            instrument: valuegen::instrument(instrument),
        })),
    );
    mint.transaction = Some(tx);
    let mut empty = m.op(
        A,
        3,
        5,
        &[mint.id],
        primitive(OperationKind::DeleteVoice(DeleteVoiceOp { voice })),
    );
    empty.transaction = Some(tx);
    let staff = m.op(
        A,
        4,
        6,
        &[empty.id],
        primitive(OperationKind::CreateStaff(CreateStaffOp {
            staff: valuegen::staff(StaffId::new(A, 604), instrument),
        })),
    );
    let state = m.agree(
        "an instrument minted by a failed transaction",
        &[
            create_voice,
            fill,
            declare,
            mint.clone(),
            empty,
            staff.clone(),
        ],
    );
    assert!(
        !live(&state, TypedObjectId::Instrument(instrument)),
        "the transaction fails, so the instrument never comes to be"
    );
    assert_eq!(
        effect(&state, staff.id),
        refused(PreconditionFailureReason::TargetMissing)
    );
}

/// One author transposes an imported quarter's pitch while another, unaware,
/// sets it to another pitch: two concurrent writes of one pitch's value, which
/// conflict in both modes. Before reduction version 3 base-free reduction,
/// which held no pitch values, could not resolve the transpose, recorded no
/// write for it, and applied the second write.
#[test]
fn a_transpose_and_a_concurrent_pitch_edit_conflict_in_both_modes() {
    use epiphany_core::TranspositionInterval;
    use epiphany_ops::{ModifyIdentifiedPitchOp, TransposeIntervalOp};
    let m = Measure::new();
    let pitch = m.import.ids.pitches[0][0][0];
    let transpose = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::TransposeInterval(TransposeIntervalOp {
            targets: [pitch].into_iter().collect(),
            interval: TranspositionInterval {
                diatonic_steps: 1,
                chromatic_steps: 2,
            },
        })),
    );
    let edit = m.op(
        B,
        0,
        2,
        &[],
        primitive(OperationKind::ModifyIdentifiedPitch(
            ModifyIdentifiedPitchOp {
                pitch,
                value: valuegen::pitch_value_nth(32),
            },
        )),
    );
    let state = m.agree(
        "a transpose and a concurrent pitch edit",
        &[transpose.clone(), edit.clone()],
    );
    assert_eq!(effect(&state, transpose.id), Some(OperationEffect::Applied));
    assert!(matches!(
        effect(&state, edit.id),
        Some(OperationEffect::Conflicted { .. })
    ));
}

/// A transpose its author undoes: the undo restores the pitch in both modes.
/// Before reduction version 3 base-free reduction recorded nothing for the
/// transpose, so the undo found nothing to restore and was refused.
#[test]
fn an_undone_transpose_restores_its_pitch_in_both_modes() {
    use epiphany_core::TranspositionInterval;
    use epiphany_ops::TransposeIntervalOp;
    let m = Measure::new();
    let pitch = m.import.ids.pitches[0][1][0];
    let tx = TransactionId::new(A, 700);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("transpose"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut transpose = m.op(
        A,
        1,
        2,
        &[declare.id],
        primitive(OperationKind::TransposeInterval(TransposeIntervalOp {
            targets: [pitch].into_iter().collect(),
            interval: TranspositionInterval {
                diatonic_steps: 2,
                chromatic_steps: 4,
            },
        })),
    );
    transpose.transaction = Some(tx);
    let undo = m.op(
        A,
        2,
        3,
        &[transpose.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let state = m.agree("an undone transpose", &[declare, transpose, undo.clone()]);
    assert_eq!(effect(&state, undo.id), Some(OperationEffect::Applied));
}

/// A transaction respells an imported quarter's pitch, the same author then
/// transposes the pitch, and then undoes the transaction: the transpose wrote
/// the pitch's spelling set after the respelling, so a strict undo conflicts,
/// in both modes. Before reduction version 3 base-free reduction kept no
/// spelling-set writes and undid the respelling.
#[test]
fn an_undo_of_a_respelling_a_transpose_superseded_conflicts_in_both_modes() {
    use epiphany_core::{CmnNominal, PitchSpelling, TranspositionInterval};
    use epiphany_ops::{RespellPitchOp, TransposeIntervalOp};
    let m = Measure::new();
    let pitch = m.import.ids.pitches[0][2][0];
    let tx = TransactionId::new(A, 800);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("respell"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut respell = m.op(
        A,
        1,
        2,
        &[declare.id],
        primitive(OperationKind::RespellPitch(RespellPitchOp {
            pitch,
            spelling: PitchSpelling::cmn(CmnNominal::E, 4),
        })),
    );
    respell.transaction = Some(tx);
    let transpose = m.op(
        A,
        2,
        3,
        &[respell.id],
        primitive(OperationKind::TransposeInterval(TransposeIntervalOp {
            targets: [pitch].into_iter().collect(),
            interval: TranspositionInterval {
                diatonic_steps: 1,
                chromatic_steps: 2,
            },
        })),
    );
    let undo = m.op(
        A,
        3,
        4,
        &[transpose.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let state = m.agree(
        "an undo of a superseded respelling",
        &[declare, respell, transpose, undo.clone()],
    );
    assert!(matches!(
        effect(&state, undo.id),
        Some(OperationEffect::Conflicted { .. })
    ));
}

/// A transaction sets the score's metadata and is undone twice: the first undo
/// restores the empty score's metadata, a write of its own, so the second
/// finds the transaction's write superseded and conflicts, in both modes.
/// Before reduction version 3 base-free reduction, seeding no metadata,
/// restored to absence and wrote nothing, so the second undo applied.
#[test]
fn a_second_undo_of_a_settings_transaction_conflicts_in_both_modes() {
    use epiphany_ops::SetMetadataOp;
    let m = Measure::new();
    let tx = TransactionId::new(A, 810);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("metadata"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut set = m.op(
        A,
        1,
        2,
        &[declare.id],
        primitive(OperationKind::SetMetadata(SetMetadataOp {
            metadata: valuegen::score_metadata(5),
        })),
    );
    set.transaction = Some(tx);
    let undo = |counter: u64, at: i64, after: OperationId| {
        m.op(
            A,
            counter,
            at,
            &[after],
            OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: UndoPolicy::StrictInverse,
            }),
        )
    };
    let first = undo(2, 3, set.id);
    let second = undo(3, 4, first.id);
    let state = m.agree(
        "a settings transaction undone twice",
        &[declare, set, first.clone(), second.clone()],
    );
    assert_eq!(effect(&state, first.id), Some(OperationEffect::Applied));
    assert!(matches!(
        effect(&state, second.id),
        Some(OperationEffect::Conflicted { .. })
    ));
}

/// A migration that keeps the region metric but reassigns two quarters to one
/// place would leave their voice overlapping (invariant 3): it conflicts,
/// naming both, in both modes. Before reduction version 3 it applied and broke
/// the invariant.
#[test]
fn a_reassignment_that_overlaps_a_voice_conflicts_in_both_modes() {
    let m = Measure::new();
    let migrate = m.op(A, 0, 1, &[], m.reassign(&[(0, 0), (1, 0), (2, 2), (3, 3)]));
    let state = m.agree(
        "an overlapping reassignment",
        std::slice::from_ref(&migrate),
    );
    assert_eq!(
        migration_failure(&state, migrate.id),
        vec![TypedObjectId::Event(m.q(0)), TypedObjectId::Event(m.q(1))]
    );
}

/// A reassignment that reorders a voice, the second and third quarters
/// changing places, applies in both modes and leaves the voice in position
/// order. Before reduction version 3 graph-aware reduction moved the events
/// and kept the voice's old order, breaking `VoiceEventsSortedNonOverlap`.
#[test]
fn a_reassignment_that_reorders_a_voice_keeps_it_sorted_in_both_modes() {
    let m = Measure::new();
    let swap = m.op(A, 0, 1, &[], m.reassign(&[(0, 0), (1, 2), (2, 1), (3, 3)]));
    let state = m.agree("two quarters swapped", std::slice::from_ref(&swap));
    assert_eq!(effect(&state, swap.id), Some(OperationEffect::Applied));
}

/// A region one author makes, breaks and then migrates to proportional time,
/// while another, unaware of the migration, breaks it again in musical time:
/// the migration drops the first break, written in musical time the region no
/// longer has, and the second is refused, in both modes. Before reduction
/// version 3 both stayed, anchored by musical offsets the region does not
/// admit (`AnchorOffsetModel`).
#[test]
fn a_region_out_of_musical_time_keeps_no_musical_break_in_both_modes() {
    use epiphany_ops::{CreateRegionOp, SetUserSystemBreakOp};
    let m = Measure::new();
    let region = RegionId::new(A, 900);
    let break_at = |n: i64| SetUserSystemBreakOp {
        region,
        anchor: valuegen::region_start_anchor(
            region,
            MusicalPosition(RationalTime::new(n, 1).expect("a bar")),
        ),
        present: true,
    };
    let create = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(region),
        })),
    );
    let first = m.op(
        A,
        1,
        2,
        &[create.id],
        primitive(OperationKind::SetUserSystemBreak(break_at(0))),
    );
    let migrate = m.op(
        A,
        2,
        3,
        &[first.id],
        m.migrate_region(region, valuegen::proportional_model()),
    );
    let second = m.op(
        B,
        0,
        4,
        &[create.id],
        primitive(OperationKind::SetUserSystemBreak(break_at(1))),
    );
    let state = m.agree(
        "breaks around a migration out of musical time",
        &[create, first.clone(), migrate.clone(), second.clone()],
    );
    assert_eq!(effect(&state, first.id), Some(OperationEffect::Applied));
    assert_eq!(effect(&state, migrate.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, second.id),
        refused(PreconditionFailureReason::WrongRegionTimeModel)
    );
    assert!(state.breaks.keys().all(|(r, _)| *r != region));
}

/// A transaction adds the measure after the import's, another author, who
/// has seen it, adds the one after that, and the transaction is undone:
/// removing the second measure would leave the third two bars from the first
/// (`MeasureMeterConsistency`), so a strict undo conflicts, in both modes.
/// Before reduction version 3 the undo removed it.
#[test]
fn an_undo_of_a_measure_with_a_later_one_conflicts_in_both_modes() {
    use epiphany_core::{Measure as Bar, MeasureId, MeasureNumberVisibility};
    use epiphany_ops::CreateMeasureOp;
    let m = Measure::new();
    let instance = m.import.ids.instances[0][0];
    let bar = |author: ReplicaId, counter: u64, at: i64| {
        OperationKind::CreateMeasure(CreateMeasureOp {
            instance,
            measure: Bar {
                id: MeasureId::new(author, counter),
                start: valuegen::region_start_anchor(
                    m.region,
                    MusicalPosition(RationalTime::new(at, 1).expect("a bar")),
                ),
                time_signature: None,
                explicit_number: None,
                number_visibility: MeasureNumberVisibility::Auto,
            },
        })
    };
    let tx = TransactionId::new(A, 950);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("a bar"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut second = m.op(A, 1, 2, &[declare.id], primitive(bar(A, 951, 1)));
    second.transaction = Some(tx);
    let third = m.op(B, 0, 3, &[second.id], primitive(bar(B, 952, 2)));
    let undo = m.op(
        A,
        2,
        4,
        &[second.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let state = m.agree(
        "an undone measure before a later one",
        &[declare, second, third.clone(), undo.clone()],
    );
    assert_eq!(effect(&state, third.id), Some(OperationEffect::Applied));
    assert!(matches!(
        effect(&state, undo.id),
        Some(OperationEffect::Conflicted { .. })
    ));
}

/// Three containers, each made in a transaction that another author, having
/// seen it, then fills: a region given a staff instance, an instance given a
/// voice, a voice given a rest. Undoing the transaction would leave the child
/// naming a removed parent, so a strict undo conflicts, in both modes, and
/// the container stays. Before reduction version 3 the undo removed it.
#[test]
fn an_undo_of_a_container_another_author_filled_conflicts_in_both_modes() {
    use epiphany_core::{StaffInstanceId, VoiceId};
    use epiphany_ops::{CreateRegionOp, CreateStaffInstanceOp, CreateVoiceOp};
    let m = Measure::new();
    let staff = m.import.ids.staves[0][0];
    let declare = |counter: u64, at: i64, seen: &[OperationId], tx: TransactionId| {
        let mut declare = m.op(
            A,
            counter,
            at,
            seen,
            primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("a container"),
                category: None,
            })),
        );
        declare.transaction = Some(tx);
        declare
    };
    let undo = |counter: u64, at: i64, made: OperationId, tx: TransactionId| {
        m.op(
            A,
            counter,
            at,
            &[made],
            OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: UndoPolicy::StrictInverse,
            }),
        )
    };
    let create_region = |region: RegionId| {
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(region),
        }))
    };
    let create_instance = |region: RegionId, instance: StaffInstanceId| {
        primitive(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region,
            instance: valuegen::staff_instance(instance, staff),
        }))
    };
    let create_voice = |instance: StaffInstanceId, voice: VoiceId| {
        primitive(OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: instance,
            voice: valuegen::voice(voice),
        }))
    };
    let check = |history: &str, authored: &[OperationEnvelope], filled: OperationId| {
        let undone = authored.last().expect("an undo").id;
        let state = m.agree(history, authored);
        assert_eq!(effect(&state, filled), Some(OperationEffect::Applied));
        assert!(
            matches!(
                effect(&state, undone),
                Some(OperationEffect::Conflicted { .. })
            ),
            "{history}: {:?}",
            effect(&state, undone)
        );
    };

    let tx = TransactionId::new(A, 960);
    let region = RegionId::new(A, 961);
    let opening = declare(0, 1, &[], tx);
    let mut made = m.op(A, 1, 2, &[opening.id], create_region(region));
    made.transaction = Some(tx);
    let filled = m.op(
        B,
        0,
        3,
        &[made.id],
        create_instance(region, StaffInstanceId::new(B, 962)),
    );
    let undone = undo(2, 4, made.id, tx);
    check(
        "an undone region holding another author's instance",
        &[opening, made, filled.clone(), undone],
        filled.id,
    );

    let tx = TransactionId::new(A, 963);
    let region = RegionId::new(A, 964);
    let instance = StaffInstanceId::new(A, 965);
    let before = m.op(A, 0, 1, &[], create_region(region));
    let opening = declare(1, 2, &[before.id], tx);
    let mut made = m.op(A, 2, 3, &[opening.id], create_instance(region, instance));
    made.transaction = Some(tx);
    let filled = m.op(
        B,
        0,
        4,
        &[made.id],
        create_voice(instance, VoiceId::new(B, 966)),
    );
    let undone = undo(3, 5, made.id, tx);
    check(
        "an undone instance holding another author's voice",
        &[before, opening, made, filled.clone(), undone],
        filled.id,
    );

    let tx = TransactionId::new(A, 967);
    let voice = VoiceId::new(A, 968);
    let opening = declare(0, 1, &[], tx);
    let mut made = m.op(
        A,
        1,
        2,
        &[opening.id],
        create_voice(m.import.ids.instances[0][0], voice),
    );
    made.transaction = Some(tx);
    let mut rest = m.rest(969, 0, eighth());
    rest.id = EventId::new(B, 969);
    rest.voice = voice;
    let filled = m.op(B, 0, 3, &[made.id], m.insert(rest));
    let undone = undo(2, 4, made.id, tx);
    check(
        "an undone voice holding another author's rest",
        &[opening, made, filled.clone(), undone],
        filled.id,
    );
}

/// A region made in a transaction that is then undone leaves the graph-aware
/// score with the transaction, where before reduction version 3 the score
/// kept a region its objects held tombstoned.
#[test]
fn an_undone_region_leaves_the_graph() {
    use epiphany_ops::CreateRegionOp;
    let m = Measure::new();
    let tx = TransactionId::new(A, 970);
    let region = RegionId::new(A, 971);
    let mut opening = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("a region"),
            category: None,
        })),
    );
    opening.transaction = Some(tx);
    let mut made = m.op(
        A,
        1,
        2,
        &[opening.id],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(region),
        })),
    );
    made.transaction = Some(tx);
    let undone = m.op(
        A,
        2,
        3,
        &[made.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let authored = [opening, made, undone.clone()];
    let state = m.agree("an undone region", &authored);
    assert!(matches!(
        effect(&state, undone.id),
        Some(OperationEffect::Applied | OperationEffect::AppliedWithRepair { .. })
    ));
    assert!(tombstoned(&state, TypedObjectId::Region(region)));
    let mut set = OperationSet::new();
    set.accept_all(m.import.envelopes.iter().chain(&authored).cloned());
    let aware = set.reduce_onto(&Score::empty(IdentityContext::new(m.import.replica)));
    assert!(
        aware.score.canvas.regions.iter().all(|r| r.id != region),
        "the undone region is still drawn"
    );
}

/// A meter in a region out of musical time: a time signature and a metric
/// grid in a region migrated to proportional time are refused, in both
/// modes, where before reduction version 3 each applied and the graph held a
/// musical meter in a region with no musical offsets (`AnchorOffsetModel`).
/// Clearing the grid still applies.
#[test]
fn a_meter_in_a_region_out_of_musical_time_is_refused_in_both_modes() {
    use epiphany_core::TimeSignatureId;
    use epiphany_ops::{CreateRegionOp, SetMetricGridOp, SetTimeSignatureOp};
    let m = Measure::new();
    let region = RegionId::new(A, 980);
    let create = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(region),
        })),
    );
    let migrate = m.op(
        A,
        1,
        2,
        &[create.id],
        m.migrate_region(region, valuegen::proportional_model()),
    );
    let signature = m.op(
        A,
        2,
        3,
        &[migrate.id],
        primitive(OperationKind::SetTimeSignature(SetTimeSignatureOp {
            region,
            anchor: valuegen::region_start_anchor(region, MusicalPosition::origin()),
            time_signature: Some(valuegen::time_signature(TimeSignatureId::new(A, 981), 3)),
        })),
    );
    let grid = |counter: u64, at: i64, grid| {
        m.op(
            A,
            counter,
            at,
            &[migrate.id],
            primitive(OperationKind::SetMetricGrid(SetMetricGridOp {
                region,
                grid,
            })),
        )
    };
    let set_grid = grid(3, 4, Some(valuegen::metric_grid()));
    let cleared = grid(4, 5, None);
    let state = m.agree(
        "a meter in a proportional region",
        &[
            create,
            migrate.clone(),
            signature.clone(),
            set_grid.clone(),
            cleared.clone(),
        ],
    );
    assert_eq!(effect(&state, migrate.id), Some(OperationEffect::Applied));
    for refused_op in [&signature, &set_grid] {
        assert_eq!(
            effect(&state, refused_op.id),
            refused(PreconditionFailureReason::WrongRegionTimeModel)
        );
    }
    assert_eq!(effect(&state, cleared.id), Some(OperationEffect::Applied));
}

/// A rest carrying a wall-clock position, inserted into a voice of the
/// metric region, is refused `WrongRegionTimeModel` in both modes; a metric
/// region places events in musical time. Before reduction version 3 it was
/// admitted, indexed at the region's origin, and the graph held a wall-clock
/// event in a metric region.
#[test]
fn an_insert_at_a_wall_clock_position_is_refused_in_both_modes() {
    use epiphany_core::VoiceId;
    use epiphany_ops::CreateVoiceOp;
    let m = Measure::new();
    let voice = VoiceId::new(A, 990);
    let made = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: m.import.ids.instances[0][0],
            voice: valuegen::voice(voice),
        })),
    );
    let mut rest = m.rest(991, 0, eighth());
    rest.voice = voice;
    rest.position = EventPosition::WallClock(WallClockTime(5));
    let inserted = m.op(A, 1, 2, &[made.id], m.insert(rest));
    let state = m.agree(
        "a wall-clock rest in a metric region",
        &[made.clone(), inserted.clone()],
    );
    assert_eq!(effect(&state, made.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, inserted.id),
        refused(PreconditionFailureReason::WrongRegionTimeModel)
    );
}

/// A tempo segment anchored by a musical offset in a region, and the region
/// migrated to proportional time, in either order: written after the
/// migration, the segment is refused `WrongRegionTimeModel`; written before
/// it, it strands the migration, which conflicts naming the region, the
/// segment having no id of its own. Both in both modes. Before reduction
/// version 3 each applied and left a tempo anchored in musical time in a
/// region with none (`AnchorOffsetModel`).
#[test]
fn a_tempo_in_a_region_out_of_musical_time_is_refused_in_both_modes() {
    use epiphany_ops::{CreateRegionOp, SetTempoSegmentOp};
    let m = Measure::new();
    let tempo = |region: RegionId| {
        primitive(OperationKind::SetTempoSegment(SetTempoSegmentOp {
            region: None,
            start: valuegen::region_start_anchor(region, MusicalPosition::origin()),
            segment: Some(valuegen::tempo_segment(
                region,
                MusicalPosition::origin(),
                96.0,
            )),
        }))
    };
    let create = |counter: u64, region: RegionId| {
        m.op(
            A,
            counter,
            counter as i64 + 1,
            &[],
            primitive(OperationKind::CreateRegion(CreateRegionOp {
                region: valuegen::region(region),
            })),
        )
    };

    let region = RegionId::new(A, 1000);
    let made = create(0, region);
    let migrated = m.op(
        A,
        1,
        2,
        &[made.id],
        m.migrate_region(region, valuegen::proportional_model()),
    );
    let after = m.op(A, 2, 3, &[migrated.id], tempo(region));
    let state = m.agree(
        "a tempo after a migration out of musical time",
        &[made, migrated.clone(), after.clone()],
    );
    assert_eq!(effect(&state, migrated.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, after.id),
        refused(PreconditionFailureReason::WrongRegionTimeModel)
    );

    let region = RegionId::new(A, 1001);
    let made = create(0, region);
    let before = m.op(A, 1, 2, &[made.id], tempo(region));
    let migrated = m.op(
        A,
        2,
        3,
        &[before.id],
        m.migrate_region(region, valuegen::proportional_model()),
    );
    let state = m.agree(
        "a tempo before a migration out of musical time",
        &[made, before.clone(), migrated.clone()],
    );
    assert_eq!(effect(&state, before.id), Some(OperationEffect::Applied));
    assert_eq!(
        migration_failure(&state, migrated.id),
        vec![TypedObjectId::Region(region)]
    );
    // The conflict names the region once, so the state decodes as written.
    assert_eq!(
        MaterializedState::decode_canonical(&state.canonical_bytes()).as_ref(),
        Ok(&state)
    );
}

const REPEATED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>2</divisions><key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type></note>
    </measure>
  </part>
</score-partwise>
"#;

/// A tie gives way (D48): every operation that moves a pitch or an event
/// applies, and a tie whose pairing or adjacency it breaks is removed, the
/// operation recording a `CascadeDeleted` repair for it, in both modes, the
/// score keeping every invariant. An edit that leaves the tie whole keeps it.
/// Before reduction version 3 every such edit applied and left the tie in
/// place, breaking `TiePairing`.
#[test]
fn a_tie_gives_way_to_an_edit_that_breaks_it_in_both_modes() {
    use epiphany_core::{
        IdentifiedPitch, Pitch, PitchId, Tie, TieClass, TieId, TranspositionInterval,
    };
    use epiphany_ops::{
        CreateCrossCuttingOp, CrossCuttingValue, DeleteIdentifiedPitchOp, InsertIdentifiedPitchOp,
        ModifyCrossCuttingOp, ModifyIdentifiedPitchOp, TransposeIntervalOp, TransposeOp,
    };
    // C4 C4 D4 E4, the tie from the first C to the second.
    let m = Measure::of(REPEATED);
    let pitch = |i: usize| m.import.ids.pitches[0][i][0];
    let value = |i: usize| -> Pitch {
        match &m.quarters[i] {
            Event::Pitched(e) => e.pitches[0].pitch.clone(),
            other => panic!("a note, not {other:?}"),
        }
    };
    let tie_id = TieId::new(A, 900);
    let tied = TypedObjectId::Tie(tie_id);
    let tie_value = |start: usize, end: usize, pairing: Pairing| {
        CrossCuttingValue::Tie(Tie {
            id: tie_id,
            start_event: m.q(start),
            end_event: m.q(end),
            pitch_pairing: pairing,
            class: TieClass::Standard,
            style: Default::default(),
        })
    };
    type Pairing = Option<Vec<(PitchId, PitchId)>>;
    let explicit = || Some(vec![(pitch(0), pitch(1))]);
    let tie = |pairing: Pairing| {
        m.op(
            A,
            0,
            1,
            &[],
            primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                structure: tie_value(0, 1, pairing),
            })),
        )
    };
    let tie_seen = OperationId::new(A, 0);
    let up_a_second = |targets: &[usize]| {
        primitive(OperationKind::TransposeInterval(TransposeIntervalOp {
            targets: targets.iter().map(|i| pitch(*i)).collect(),
            interval: TranspositionInterval {
                diatonic_steps: 1,
                chromatic_steps: 2,
            },
        }))
    };
    let set_pitch = |i: usize, to: Pitch| {
        primitive(OperationKind::ModifyIdentifiedPitch(
            ModifyIdentifiedPitchOp {
                pitch: pitch(i),
                value: to,
            },
        ))
    };
    let with_value = |i: usize, to: Pitch| {
        let mut event = m.quarters[i].clone();
        match &mut event {
            Event::Pitched(e) => e.pitches[0].pitch = to,
            other => panic!("a note, not {other:?}"),
        }
        primitive(OperationKind::ModifyEvent(ModifyEventOp { event }))
    };
    let added = |i: usize| {
        primitive(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp {
                event: m.q(i),
                pitch: IdentifiedPitch {
                    id: PitchId::new(A, 950),
                    pitch: value(3),
                },
            },
        ))
    };
    let gave_way = |state: &MaterializedState, id: OperationId| {
        tombstoned(state, tied)
            && matches!(
                effect(state, id),
                Some(OperationEffect::AppliedWithRepair { repairs })
                    if repairs.iter().any(|r| r.kind == RepairKind::CascadeDeleted && r.target == tied)
            )
    };

    // Each edit by A, after the tie: the edit applies and the tie gives way.
    let edits: Vec<(&str, Pairing, OperationPayload)> = vec![
        ("a transpose of one end", explicit(), up_a_second(&[1])),
        (
            "a replay transpose of one end",
            explicit(),
            primitive(OperationKind::Transpose(TransposeOp {
                targets: vec![pitch(0)],
                chromatic_steps: 1,
            })),
        ),
        (
            "a pitch edit of one end",
            explicit(),
            set_pitch(1, value(2)),
        ),
        (
            "a whole-event edit of one end's pitch",
            explicit(),
            with_value(0, value(3)),
        ),
        (
            "a delete of an end's pitch",
            explicit(),
            primitive(OperationKind::DeleteIdentifiedPitch(
                DeleteIdentifiedPitchOp { pitch: pitch(1) },
            )),
        ),
        ("a pitch added to an implicitly paired end", None, added(1)),
        (
            "its end moved past the next quarter",
            explicit(),
            m.reassign(&[(0, 0), (1, 2), (2, 1), (3, 3)]),
        ),
        (
            "the tie rewritten onto a quarter not next",
            explicit(),
            primitive(OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                structure: tie_value(0, 2, None),
            })),
        ),
    ];
    for (name, pairing, payload) in edits {
        let tie = tie(pairing);
        let edit = m.op(A, 1, 2, &[tie_seen], payload);
        let state = m.agree(name, &[tie, edit.clone()]);
        assert!(
            gave_way(&state, edit.id),
            "{name}: {:?}",
            effect(&state, edit.id)
        );
    }

    // An insert between the ends, into the time the start's trim freed.
    let trim = m.op(A, 1, 2, &[tie_seen], primitive(m.trim(0, eighth())));
    let mut rest = m.rest(960, 0, eighth());
    rest.position = EventPosition::Musical(MusicalPosition(
        RationalTime::new(1, 8).expect("an eighth in"),
    ));
    let insert = m.op(B, 0, 3, &[tie_seen, trim.id], m.insert(rest));
    let state = m.agree(
        "an insert between the ends",
        &[tie(explicit()), trim.clone(), insert.clone()],
    );
    assert_eq!(
        effect(&state, trim.id),
        Some(OperationEffect::Applied),
        "a trim keeps the tie"
    );
    assert!(
        gave_way(&state, insert.id),
        "{:?}",
        effect(&state, insert.id)
    );

    // A tie created over a concurrent edit that breaks it gives way at once.
    let edit = m.op(B, 0, 1, &[], set_pitch(1, value(2)));
    let late = m.op(
        A,
        0,
        2,
        &[],
        primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: tie_value(0, 1, explicit()),
        })),
    );
    let state = m.agree("a tie over a concurrent pitch edit", &[edit, late.clone()]);
    assert!(gave_way(&state, late.id), "{:?}", effect(&state, late.id));

    // An undo restoring the pitch the tie was made for: the D made a C, tied
    // from the C before it, then the edit undone.
    let tx = TransactionId::new(A, 970);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("edit"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut make_c = m.op(A, 1, 2, &[declare.id], set_pitch(2, value(1)));
    make_c.transaction = Some(tx);
    let later_tie = m.op(
        A,
        2,
        3,
        &[make_c.id],
        primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: tie_value(1, 2, None),
        })),
    );
    let undo = m.op(
        A,
        3,
        4,
        &[later_tie.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let state = m.agree(
        "an undo restoring an end's pitch",
        &[declare, make_c, later_tie.clone(), undo.clone()],
    );
    assert_eq!(effect(&state, later_tie.id), Some(OperationEffect::Applied));
    assert!(gave_way(&state, undo.id), "{:?}", effect(&state, undo.id));

    // Edits that leave the tie whole keep it: both ends transposed together,
    // a pitch added to an explicitly paired end, the next quarter lengthened.
    let keeps: Vec<(&str, OperationPayload)> = vec![
        ("both ends transposed", up_a_second(&[0, 1])),
        ("a pitch added beside an explicit pair", added(1)),
        (
            "the quarter after the tie trimmed",
            primitive(m.trim(2, eighth())),
        ),
    ];
    for (name, payload) in keeps {
        let edit = m.op(A, 1, 2, &[tie_seen], payload);
        let state = m.agree(name, &[tie(explicit()), edit.clone()]);
        assert_eq!(
            effect(&state, edit.id),
            Some(OperationEffect::Applied),
            "{name}"
        );
        assert!(live(&state, tied), "{name}: the tie stays");
    }
}

/// The pitch one author adds to the first quarter and transposes, which
/// leaves it a propagated spelling: its insert and its transpose.
fn added_pitch(
    m: &Measure,
    counter: u64,
    at: i64,
    seen: &[OperationId],
) -> (epiphany_core::PitchId, [OperationEnvelope; 2]) {
    use epiphany_core::{IdentifiedPitch, PitchId, TranspositionInterval};
    use epiphany_ops::{InsertIdentifiedPitchOp, TransposeIntervalOp};
    let pitch = PitchId::new(B, 600);
    let insert = m.op(
        B,
        counter,
        at,
        seen,
        primitive(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp {
                event: m.q(0),
                pitch: IdentifiedPitch {
                    id: pitch,
                    pitch: valuegen::pitch_value_nth(4),
                },
            },
        )),
    );
    let mut after = seen.to_vec();
    after.push(insert.id);
    let transpose = m.op(
        B,
        counter + 1,
        at + 1,
        &after,
        primitive(OperationKind::TransposeInterval(TransposeIntervalOp {
            targets: [pitch].into_iter().collect(),
            interval: TranspositionInterval {
                diatonic_steps: 1,
                chromatic_steps: 2,
            },
        })),
    );
    (pitch, [insert, transpose])
}

/// Whether the graph-aware score's event holds `pitch`, with a spelling
/// attachment scoped to it.
fn holds_with_spelling(
    m: &Measure,
    authored: &[OperationEnvelope],
    event: EventId,
    pitch: epiphany_core::PitchId,
) -> bool {
    use epiphany_core::SpellingScope;
    let mut set = OperationSet::new();
    set.accept_all(m.import.envelopes.iter().chain(authored).cloned());
    let score = set
        .reduce_onto(&Score::empty(IdentityContext::new(m.import.replica)))
        .score;
    let held = matches!(score.events.get(event), Some(Event::Pitched(e))
        if e.pitches.iter().any(|ip| ip.id == pitch));
    let spelt = score
        .spelling_attachments
        .iter()
        .any(|a| matches!(&a.scope, SpellingScope::Pitch(p) if *p == pitch));
    held && spelt
}

/// A whole-event modify keeps a pitch a concurrent author added and it never
/// saw, with the pitch's attachments (add wins, D48), in both modes. Before
/// reduction version 3 the modify's value replaced the event's pitches, so the
/// added pitch left the graph while it stayed live and its spelling attachment
/// named nothing (`SpellingScopeResolves`).
#[test]
fn a_modify_keeps_a_pitch_its_author_never_saw_in_both_modes() {
    let m = Measure::new();
    let (pitch, [insert, transpose]) = added_pitch(&m, 0, 1, &[]);
    let trim = m.op(A, 0, 3, &[], primitive(m.trim(0, eighth())));
    let authored = [insert.clone(), transpose, trim.clone()];
    let state = m.agree("a trim beside an unseen pitch", &authored);
    assert_eq!(effect(&state, trim.id), Some(OperationEffect::Applied));
    assert!(live(&state, TypedObjectId::Pitch(pitch)));
    assert!(
        holds_with_spelling(&m, &authored, m.q(0), pitch),
        "the trimmed quarter keeps the added pitch and its spelling"
    );
}

/// An undo of a modify keeps a pitch another author added to the event
/// since, with its attachments: the undo reverses its own transaction, not
/// the pitch's insert. Before reduction version 3 the restored value replaced
/// the event's pitches, dropping the added one from the graph.
#[test]
fn an_undo_of_a_modify_keeps_a_pitch_added_since_in_both_modes() {
    let m = Measure::new();
    let tx = TransactionId::new(A, 650);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("trim"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut trim = m.op(A, 1, 2, &[declare.id], primitive(m.trim(0, eighth())));
    trim.transaction = Some(tx);
    let (pitch, [insert, transpose]) = added_pitch(&m, 0, 3, &[trim.id]);
    let undo = m.op(
        A,
        2,
        5,
        &[transpose.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let authored = [declare, trim, insert, transpose, undo.clone()];
    let state = m.agree("an undone trim beside a pitch added since", &authored);
    assert_eq!(effect(&state, undo.id), Some(OperationEffect::Applied));
    assert!(
        holds_with_spelling(&m, &authored, m.q(0), pitch),
        "the restored quarter keeps the added pitch and its spelling"
    );
}

/// A clef or key change is written at a musical position, so a region out of
/// musical time takes none: `SetClef` and `SetKeySignature` there, and a
/// `CreateStaffInstance` carrying such a change, are refused
/// `WrongRegionTimeModel`, and a live change strands a migration of its
/// region out of musical time, which conflicts naming the instance, in both
/// modes. Before reduction version 3 each applied and left a change anchored
/// by a musical offset the region no longer admits (`AnchorOffsetModel`).
#[test]
fn a_clef_or_key_in_a_region_out_of_musical_time_is_refused_in_both_modes() {
    use epiphany_core::{Clef, ClefChange, ClefShape, KeySignature, StaffInstanceId, TimeAnchor};
    use epiphany_ops::{CreateRegionOp, CreateStaffInstanceOp, SetClefOp, SetKeySignatureOp};
    let m = Measure::new();
    let staff = m.import.ids.staves[0][0];
    let alto = Clef {
        shape: ClefShape::C,
        line: 3,
        octave_shift: 0,
    };
    let made = |counter: u64, region: RegionId| {
        m.op(
            A,
            counter,
            counter as i64 + 1,
            &[],
            primitive(OperationKind::CreateRegion(CreateRegionOp {
                region: valuegen::region(region),
            })),
        )
    };
    let instance_op = |counter: u64, seen: &[OperationId], region, instance, clefs| {
        let mut value = valuegen::staff_instance(instance, staff);
        value.clef_sequence = clefs;
        m.op(
            A,
            counter,
            counter as i64 + 1,
            seen,
            primitive(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region,
                instance: value,
            })),
        )
    };
    let clef = |instance| {
        primitive(OperationKind::SetClef(SetClefOp {
            instance,
            offset: RationalTime::zero(),
            clef: Some(alto),
        }))
    };
    let key = |instance| {
        primitive(OperationKind::SetKeySignature(SetKeySignatureOp {
            instance,
            offset: RationalTime::zero(),
            key: KeySignature::new(2),
        }))
    };

    // After the migration: a clef, a key and an instance carrying a clef in
    // musical time are refused.
    let region = RegionId::new(A, 1100);
    let instance = StaffInstanceId::new(A, 1101);
    let create = made(0, region);
    let quiet = instance_op(1, &[create.id], region, instance, Vec::new());
    let migrated = m.op(
        A,
        2,
        3,
        &[quiet.id],
        m.migrate_region(region, valuegen::proportional_model()),
    );
    let set_clef = m.op(A, 3, 4, &[migrated.id], clef(instance));
    let set_key = m.op(A, 4, 5, &[migrated.id], key(instance));
    // The carrying instance goes into a second region, the first already
    // manifesting the staff.
    let other = RegionId::new(A, 1103);
    let create_other = m.op(
        A,
        5,
        6,
        &[set_key.id],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(other),
        })),
    );
    let migrated_other = m.op(
        A,
        6,
        7,
        &[create_other.id],
        m.migrate_region(other, valuegen::proportional_model()),
    );
    let carried = vec![ClefChange {
        anchor: TimeAnchor::Region {
            id: other,
            edge: epiphany_core::RegionEdge::Start,
            offset: epiphany_core::AnchorOffset::Musical(MusicalDuration::zero()),
        },
        clef: alto,
    }];
    let second = instance_op(
        7,
        &[migrated_other.id],
        other,
        StaffInstanceId::new(A, 1102),
        carried,
    );
    let state = m.agree(
        "a clef, a key and an instance in a proportional region",
        &[
            create,
            quiet,
            migrated.clone(),
            set_clef.clone(),
            set_key.clone(),
            create_other,
            migrated_other.clone(),
            second.clone(),
        ],
    );
    assert_eq!(effect(&state, migrated.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, migrated_other.id),
        Some(OperationEffect::Applied)
    );
    for op in [&set_clef, &set_key, &second] {
        assert_eq!(
            effect(&state, op.id),
            refused(PreconditionFailureReason::WrongRegionTimeModel)
        );
    }

    // Before the migration: a live clef, or key, strands it.
    let changes: [&dyn Fn(StaffInstanceId) -> OperationPayload; 2] = [&clef, &key];
    for (n, change) in changes.into_iter().enumerate() {
        let region = RegionId::new(A, 1110 + n as u64);
        let instance = StaffInstanceId::new(A, 1120 + n as u64);
        let create = made(0, region);
        let quiet = instance_op(1, &[create.id], region, instance, Vec::new());
        let set = m.op(A, 2, 3, &[quiet.id], change(instance));
        let migrated = m.op(
            A,
            3,
            4,
            &[set.id],
            m.migrate_region(region, valuegen::proportional_model()),
        );
        let state = m.agree(
            "a migration a clef or key strands",
            &[create, quiet, set.clone(), migrated.clone()],
        );
        assert_eq!(effect(&state, set.id), Some(OperationEffect::Applied));
        assert_eq!(
            migration_failure(&state, migrated.id),
            vec![TypedObjectId::StaffInstance(instance)]
        );
    }
}

/// A staff named by a live part definition or spanner is not undone: an undo
/// of the transaction that made it, after another author named it, conflicts
/// in both modes, naming the referencer; and a spanner naming a staff an undo
/// has removed is refused `TargetMissing` in both, its staves read as
/// referents. Before reduction version 3 the undo removed the staff and the
/// spanner applied, each leaving a reference to a staff the score does not
/// declare (`CrossCuttingRefsResolve`).
#[test]
fn an_undo_of_a_staff_a_part_or_spanner_names_conflicts_in_both_modes() {
    use epiphany_core::{PartDefinitionId, SpannerId, StaffId};
    use epiphany_ops::{
        ConflictKind, CreateCrossCuttingOp, CreatePartDefinitionOp, CreateStaffOp,
        CrossCuttingValue,
    };
    let m = Measure::new();
    let instrument = m.import.ids.instruments[0];
    let make_staff = |tx: TransactionId, staff: StaffId| {
        let mut declare = m.op(
            A,
            0,
            1,
            &[],
            primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("a staff"),
                category: None,
            })),
        );
        declare.transaction = Some(tx);
        let mut made = m.op(
            A,
            1,
            2,
            &[declare.id],
            primitive(OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(staff, instrument),
            })),
        );
        made.transaction = Some(tx);
        (declare, made)
    };
    let undo = |counter: u64, at: i64, seen: &[OperationId], tx: TransactionId| {
        m.op(
            A,
            counter,
            at,
            seen,
            OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: UndoPolicy::StrictInverse,
            }),
        )
    };
    let part = |staff: StaffId| {
        primitive(OperationKind::CreatePartDefinition(
            CreatePartDefinitionOp {
                part: valuegen::part_definition(PartDefinitionId::new(B, 1201), vec![staff]),
            },
        ))
    };
    let spanner = |staff: StaffId| {
        primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: CrossCuttingValue::Spanner(epiphany_core::Spanner {
                id: SpannerId::new(B, 1202),
                start: valuegen::event_anchor(m.q(0)),
                end: valuegen::event_anchor(m.q(1)),
                staves: vec![staff],
                kind: Default::default(),
                style: Default::default(),
            }),
        }))
    };

    for (n, (name, named)) in [
        ("a part", &part as &dyn Fn(StaffId) -> OperationPayload),
        ("a spanner", &spanner),
    ]
    .into_iter()
    .enumerate()
    {
        let tx = TransactionId::new(A, 1210 + n as u64);
        let staff = StaffId::new(A, 1220 + n as u64);
        let (declare, made) = make_staff(tx, staff);
        let naming = m.op(B, 0, 3, &[made.id], named(staff));
        let undone = undo(2, 4, &[naming.id], tx);
        let state = m.agree(
            &format!("an undone staff {name} names"),
            &[declare, made, naming.clone(), undone.clone()],
        );
        assert_eq!(effect(&state, naming.id), Some(OperationEffect::Applied));
        let Some(OperationEffect::Conflicted { conflict }) = effect(&state, undone.id) else {
            panic!("{name}: {:?}", effect(&state, undone.id));
        };
        let record = state
            .conflicts
            .records()
            .iter()
            .find(|r| r.id == conflict)
            .expect("recorded");
        assert!(matches!(
            record.kind,
            ConflictKind::TransactionConflict { .. }
        ));
        assert!(
            live(&state, TypedObjectId::Staff(staff)),
            "{name}: the staff stays"
        );
    }

    // A spanner after the undo, its author unaware of it, names a dead staff.
    let tx = TransactionId::new(A, 1230);
    let staff = StaffId::new(A, 1231);
    let (declare, made) = make_staff(tx, staff);
    let undone = undo(2, 3, &[made.id], tx);
    let late = m.op(B, 0, 4, &[made.id], spanner(staff));
    let state = m.agree(
        "a spanner naming an undone staff",
        &[declare, made, undone.clone(), late.clone()],
    );
    assert!(tombstoned(&state, TypedObjectId::Staff(staff)));
    assert!(matches!(
        effect(&state, undone.id),
        Some(OperationEffect::AppliedWithRepair { .. })
    ));
    assert_eq!(
        effect(&state, late.id),
        refused(PreconditionFailureReason::TargetMissing)
    );
}

/// A tuplet another author made over an event fixes its duration, so an undo
/// of the transaction that trimmed the event, which would restore its earlier
/// duration, is superseded by the tuplet in both modes: a strict undo
/// conflicts and a best-effort one leaves the duration, the tuplet's members
/// still filling its total. Before reduction version 3 the undo restored the
/// duration and broke the tuplet's sum (`TupletSum`).
#[test]
fn an_undo_restoring_a_tuplet_members_duration_is_superseded_in_both_modes() {
    for policy in [UndoPolicy::StrictInverse, UndoPolicy::BestEffort] {
        let m = Measure::new();
        let tx = TransactionId::new(A, 1300);
        let mut declare = m.op(
            A,
            0,
            1,
            &[],
            primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("trim"),
                category: None,
            })),
        );
        declare.transaction = Some(tx);
        let mut trim = m.op(A, 1, 2, &[declare.id], primitive(m.trim(1, eighth())));
        trim.transaction = Some(tx);
        let tuplet = m.op(
            B,
            0,
            3,
            &[trim.id],
            primitive(OperationKind::CreateTuplet(CreateTupletOp {
                tuplet: Tuplet {
                    id: TupletId::new(B, 1301),
                    ratio: TupletRatio::new(3, 2).expect("not degenerate"),
                    members: vec![m.q(0), m.q(1)],
                    parent: None,
                    required_total: musical(m.quarters[0].duration()) + musical(&eighth()),
                    display: Default::default(),
                },
            })),
        );
        let undo = m.op(
            A,
            2,
            4,
            &[tuplet.id],
            OperationPayload::UndoTransaction(UndoTransactionPayload { target: tx, policy }),
        );
        let state = m.agree(
            &format!("an undone trim of a tuplet member, {policy:?}"),
            &[declare, trim, tuplet.clone(), undo.clone()],
        );
        assert_eq!(effect(&state, tuplet.id), Some(OperationEffect::Applied));
        match policy {
            UndoPolicy::BestEffort => {
                assert_eq!(effect(&state, undo.id), Some(OperationEffect::Applied))
            }
            _ => assert!(
                matches!(
                    effect(&state, undo.id),
                    Some(OperationEffect::Conflicted { .. })
                ),
                "{:?}",
                effect(&state, undo.id)
            ),
        }
    }
}

/// An undo of the transaction that made a region, after a tempo segment was
/// anchored to it, or of the one that made an instrument, after a staff
/// instance was set to it, is blocked in both modes: a strict undo conflicts,
/// a best-effort one keeps the region or instrument. Before reduction version
/// 3 each undo removed it and left the anchor or the override naming nothing
/// (`CrossCuttingRefsResolve`).
#[test]
fn an_undo_of_a_region_or_instrument_still_named_is_blocked_in_both_modes() {
    use epiphany_core::InstrumentId;
    use epiphany_ops::{CreateInstrumentOp, CreateRegionOp, SetStaffLayoutOp, SetTempoSegmentOp};
    let m = Measure::new();
    let made = |tx: TransactionId, payload: OperationPayload| {
        let mut declare = m.op(
            A,
            0,
            1,
            &[],
            primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("make"),
                category: None,
            })),
        );
        declare.transaction = Some(tx);
        let mut make = m.op(A, 1, 2, &[declare.id], payload);
        make.transaction = Some(tx);
        (declare, make)
    };
    let region = RegionId::new(A, 1400);
    let instrument = InstrumentId::new(A, 1401);
    let cases: [(&str, OperationPayload, OperationPayload, TypedObjectId); 2] = [
        (
            "a region a tempo segment anchors to",
            primitive(OperationKind::CreateRegion(CreateRegionOp {
                region: valuegen::region(region),
            })),
            primitive(OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: None,
                start: valuegen::region_start_anchor(region, MusicalPosition::origin()),
                segment: Some(valuegen::tempo_segment(
                    region,
                    MusicalPosition::origin(),
                    96.0,
                )),
            })),
            TypedObjectId::Region(region),
        ),
        (
            "an instrument a staff instance is set to",
            primitive(OperationKind::CreateInstrument(CreateInstrumentOp {
                instrument: valuegen::instrument(instrument),
            })),
            primitive(OperationKind::SetStaffLayout(SetStaffLayoutOp {
                staff_instance: m.import.ids.instances[0][0],
                instrument_override: Some(instrument),
                staff_lines_override: None,
                visible: true,
            })),
            TypedObjectId::Instrument(instrument),
        ),
    ];
    for (n, (name, make, naming, object)) in cases.into_iter().enumerate() {
        for policy in [UndoPolicy::StrictInverse, UndoPolicy::BestEffort] {
            let tx = TransactionId::new(A, 1410 + n as u64);
            let (declare, make) = made(tx, make.clone());
            let named = m.op(B, 0, 3, &[make.id], naming.clone());
            let undo = m.op(
                A,
                2,
                4,
                &[named.id],
                OperationPayload::UndoTransaction(UndoTransactionPayload { target: tx, policy }),
            );
            let state = m.agree(
                &format!("an undo of {name}, {policy:?}"),
                &[declare, make, named.clone(), undo.clone()],
            );
            assert_eq!(
                effect(&state, named.id),
                Some(OperationEffect::Applied),
                "{name}"
            );
            assert!(live(&state, object), "{name}, {policy:?}: kept");
            if policy == UndoPolicy::StrictInverse {
                assert!(
                    matches!(
                        effect(&state, undo.id),
                        Some(OperationEffect::Conflicted { .. })
                    ),
                    "{name}: {:?}",
                    effect(&state, undo.id)
                );
                // The conflict names the region once, so the state decodes as
                // written.
                assert_eq!(
                    MaterializedState::decode_canonical(&state.canonical_bytes()).as_ref(),
                    Ok(&state)
                );
            }
        }
    }
}

/// A whole-event modify neither removes nor revives a pitch, in both modes:
/// one its author saw and left out stays in the event with its attachments,
/// and one it carries that an undo removed concurrently stays gone. Before
/// reduction version 3 the first left the graph while it stayed live, its
/// spelling naming nothing (`SpellingScopeResolves`), and the second came
/// back into the graph while tombstoned (`UniqueIdentifiers`).
#[test]
fn a_whole_event_modify_neither_removes_nor_revives_a_pitch_in_both_modes() {
    use epiphany_core::{IdentifiedPitch, PitchId};
    use epiphany_ops::InsertIdentifiedPitchOp;
    let m = Measure::new();

    // Left out by an author who saw it.
    let (pitch, [insert, transpose]) = added_pitch(&m, 0, 1, &[]);
    let trim = m.op(A, 0, 3, &[transpose.id], primitive(m.trim(0, eighth())));
    let authored = [insert, transpose, trim.clone()];
    let state = m.agree("a trim leaving out a pitch it saw", &authored);
    assert_eq!(effect(&state, trim.id), Some(OperationEffect::Applied));
    assert!(
        holds_with_spelling(&m, &authored, m.q(0), pitch),
        "the trimmed quarter keeps the pitch it left out, and its spelling"
    );

    // Carried after an undo removed it.
    let tx = TransactionId::new(A, 1500);
    let added = PitchId::new(A, 1501);
    let mut declare = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("add"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let mut add = m.op(
        A,
        1,
        2,
        &[declare.id],
        primitive(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp {
                event: m.q(0),
                pitch: IdentifiedPitch {
                    id: added,
                    pitch: valuegen::pitch_value_nth(4),
                },
            },
        )),
    );
    add.transaction = Some(tx);
    let undo = m.op(
        B,
        0,
        3,
        &[add.id],
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let mut chord = m.quarters[0].clone();
    match &mut chord {
        Event::Pitched(e) => {
            e.duration = eighth();
            e.pitches.push(IdentifiedPitch {
                id: added,
                pitch: valuegen::pitch_value_nth(4),
            });
        }
        other => panic!("a note, not {other:?}"),
    }
    let carried = m.op(
        A,
        2,
        4,
        &[add.id],
        primitive(OperationKind::ModifyEvent(ModifyEventOp { event: chord })),
    );
    let authored = [declare, add, undo.clone(), carried.clone()];
    let state = m.agree("a trim carrying a pitch an undo removed", &authored);
    assert!(matches!(
        effect(&state, undo.id),
        Some(OperationEffect::AppliedWithRepair { .. })
    ));
    assert!(tombstoned(&state, TypedObjectId::Pitch(added)));
    let mut set = OperationSet::new();
    set.accept_all(m.import.envelopes.iter().chain(&authored).cloned());
    let score = set
        .reduce_onto(&Score::empty(IdentityContext::new(m.import.replica)))
        .score;
    let Some(Event::Pitched(quarter)) = score.events.get(m.q(0)) else {
        panic!("the first quarter");
    };
    assert!(
        quarter.pitches.iter().all(|ip| ip.id != added),
        "the removed pitch stays out of the event"
    );
}

/// A measure starts in musical time, so a `CreateMeasure` into a staff
/// instance of a region out of musical time is refused `WrongRegionTimeModel`
/// in both modes. Before reduction version 3 it applied, its start anchored by
/// a musical offset the region does not admit (`AnchorOffsetModel`).
#[test]
fn a_measure_in_a_region_out_of_musical_time_is_refused_in_both_modes() {
    use epiphany_core::{MeasureId, StaffInstanceId, TimeSignatureId};
    use epiphany_ops::{CreateMeasureOp, CreateRegionOp, CreateStaffInstanceOp};
    let m = Measure::new();
    let region = RegionId::new(A, 1600);
    let instance = StaffInstanceId::new(A, 1601);
    let create = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(region),
        })),
    );
    let staff = m.op(
        A,
        1,
        2,
        &[create.id],
        primitive(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region,
            instance: valuegen::staff_instance(instance, m.import.ids.staves[0][0]),
        })),
    );
    let migrated = m.op(
        A,
        2,
        3,
        &[staff.id],
        m.migrate_region(region, valuegen::proportional_model()),
    );
    let mut measure = valuegen::measure(MeasureId::new(A, 1602), TimeSignatureId::new(A, 1603), 1);
    measure.time_signature = None;
    measure.start = valuegen::region_start_anchor(region, MusicalPosition::origin());
    let made = m.op(
        A,
        3,
        4,
        &[migrated.id],
        primitive(OperationKind::CreateMeasure(CreateMeasureOp {
            instance,
            measure,
        })),
    );
    let state = m.agree(
        "a measure in a proportional region",
        &[create, staff, migrated.clone(), made.clone()],
    );
    assert_eq!(effect(&state, migrated.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, made.id),
        refused(PreconditionFailureReason::WrongRegionTimeModel)
    );
}

/// Two chains written independently, a region's whole grid and one of its
/// meter changes, compare by which was written later as the reduction applies
/// them. A transaction applies where its first member falls, so a grid write
/// it stamps after another author's concurrent time signature is applied
/// before it, and the signature governs the grid the next measure is placed
/// by: a measure a bar too far is refused `MeasureMeterMismatch` in both
/// modes. Before reduction version 3 the comparison read the stamps, took the
/// grid write for the later, placed the measure by no signature and applied
/// it, a bar too far under the signature the graph held
/// (`MeasureMeterConsistency`).
#[test]
fn a_transactions_grid_write_is_as_recent_as_it_applied_in_both_modes() {
    use epiphany_core::{MeasureId, StaffInstanceId, TimeSignatureId};
    use epiphany_ops::{
        CreateMeasureOp, CreateRegionOp, CreateStaffInstanceOp, SetMetricGridOp, SetTimeSignatureOp,
    };
    let m = Measure::new();
    let region = RegionId::new(A, 1700);
    let instance = StaffInstanceId::new(A, 1701);
    let at = |offset: i64| {
        valuegen::region_start_anchor(
            region,
            MusicalPosition(RationalTime::new(offset, 1).expect("whole notes")),
        )
    };
    let measure = |id: u64, offset: i64| {
        let mut measure = valuegen::measure(MeasureId::new(A, id), TimeSignatureId::new(A, 0), 1);
        measure.time_signature = None;
        measure.start = at(offset);
        primitive(OperationKind::CreateMeasure(CreateMeasureOp {
            instance,
            measure,
        }))
    };
    let create = m.op(
        A,
        0,
        1,
        &[],
        primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(region),
        })),
    );
    let staff = m.op(
        A,
        1,
        2,
        &[create.id],
        primitive(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region,
            instance: valuegen::staff_instance(instance, m.import.ids.staves[0][0]),
        })),
    );
    let first = m.op(A, 2, 3, &[staff.id], measure(1702, 1));
    // A's transaction: its first grid write before B's signature, its second
    // after; the block applies where its first member falls.
    let tx = TransactionId::new(A, 1703);
    let mut declare = m.op(
        A,
        3,
        4,
        &[first.id],
        primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("grid"),
            category: None,
        })),
    );
    declare.transaction = Some(tx);
    let clear = |counter: u64, at: i64, seen: OperationId| {
        let mut write = m.op(
            A,
            counter,
            at,
            &[seen],
            primitive(OperationKind::SetMetricGrid(SetMetricGridOp {
                region,
                grid: None,
            })),
        );
        write.transaction = Some(tx);
        write
    };
    let early = clear(4, 5, declare.id);
    let signature = m.op(
        B,
        0,
        6,
        &[first.id],
        primitive(OperationKind::SetTimeSignature(SetTimeSignatureOp {
            region,
            anchor: at(1),
            time_signature: Some(valuegen::time_signature(TimeSignatureId::new(B, 1704), 2)),
        })),
    );
    let grid = clear(5, 7, early.id);
    // A bar of 2/4 after the first measure is a half; this is a whole.
    let second = m.op(A, 6, 8, &[grid.id, signature.id], measure(1705, 2));
    let state = m.agree(
        "a measure placed by the grid as it applied",
        &[
            create,
            staff,
            first,
            declare,
            early,
            signature.clone(),
            grid.clone(),
            second.clone(),
        ],
    );
    assert_eq!(effect(&state, signature.id), Some(OperationEffect::Applied));
    assert_eq!(effect(&state, grid.id), Some(OperationEffect::Applied));
    assert_eq!(
        effect(&state, second.id),
        refused(PreconditionFailureReason::MeasureMeterMismatch)
    );
}

/// An undo that keeps one of its transaction's mints for what names it keeps
/// what that mint names in turn, in both modes: a transaction makes an
/// instrument and a staff on it, another author puts an instance of the staff
/// in the region, and an undo of the transaction keeps the staff for its
/// instance and so the instrument for the staff (strict: conflicted; best
/// effort: both kept). Before reduction version 3 a best-effort undo kept the
/// staff and removed the instrument, judging the staff's reference as going
/// with it, and the staff named an instrument the score did not declare
/// (`CrossCuttingRefsResolve`).
#[test]
fn an_undo_keeps_what_a_kept_mint_names_in_both_modes() {
    use epiphany_core::{InstrumentId, StaffId, StaffInstanceId};
    use epiphany_ops::{CreateInstrumentOp, CreateStaffInstanceOp, CreateStaffOp};
    let m = Measure::new();
    for (n, policy) in [UndoPolicy::StrictInverse, UndoPolicy::BestEffort]
        .into_iter()
        .enumerate()
    {
        let n = n as u64;
        let tx = TransactionId::new(A, 1800 + 10 * n);
        let instrument = InstrumentId::new(A, 1801 + 10 * n);
        let staff = StaffId::new(A, 1802 + 10 * n);
        let mut declare = m.op(
            A,
            0,
            1,
            &[],
            primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("an instrument and its staff"),
                category: None,
            })),
        );
        declare.transaction = Some(tx);
        let mut made_instrument = m.op(
            A,
            1,
            2,
            &[declare.id],
            primitive(OperationKind::CreateInstrument(CreateInstrumentOp {
                instrument: valuegen::instrument(instrument),
            })),
        );
        made_instrument.transaction = Some(tx);
        let mut made_staff = m.op(
            A,
            2,
            3,
            &[made_instrument.id],
            primitive(OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(staff, instrument),
            })),
        );
        made_staff.transaction = Some(tx);
        let instance = m.op(
            B,
            0,
            4,
            &[made_staff.id],
            primitive(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region: m.region,
                instance: valuegen::staff_instance(StaffInstanceId::new(B, 1803 + 10 * n), staff),
            })),
        );
        let undo = m.op(
            A,
            3,
            5,
            &[instance.id],
            OperationPayload::UndoTransaction(UndoTransactionPayload { target: tx, policy }),
        );
        let state = m.agree(
            &format!("an undo of an instrument and its staff, {policy:?}"),
            &[
                declare,
                made_instrument,
                made_staff,
                instance.clone(),
                undo.clone(),
            ],
        );
        assert_eq!(effect(&state, instance.id), Some(OperationEffect::Applied));
        assert!(live(&state, TypedObjectId::Staff(staff)), "{policy:?}");
        assert!(
            live(&state, TypedObjectId::Instrument(instrument)),
            "{policy:?}: the kept staff keeps its instrument"
        );
        match policy {
            UndoPolicy::StrictInverse => assert!(
                matches!(
                    effect(&state, undo.id),
                    Some(OperationEffect::Conflicted { .. })
                ),
                "{:?}",
                effect(&state, undo.id)
            ),
            _ => assert_eq!(effect(&state, undo.id), Some(OperationEffect::Applied)),
        }
    }
}
