//! M2 regression coverage for reducing operations onto Agent B's real score
//! graph rather than only the Chapter 6 bookkeeping projection.

use epiphany_core::{
    check_invariants, derive_promoted_voice_id, AnchorOffset, EventId, MusicalDuration,
    MusicalPosition, OperationId, PitchId, RationalTime, RegionEdge, RegionTimeModel, ReplicaId,
    Score, SlurId, StaffInstanceId, TimeAnchor, TransactionId, TypedObjectId, VoiceId, VoiceOrigin,
    WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, ChangeRegionTimeModelOp, ConflictKind, CreateCrossCuttingOp,
    CrossCuttingValue, DeleteEventOp, HybridLogicalClock, InsertEventOp, NoOpReason,
    OperationEffect, OperationEnvelope, OperationKind, OperationPayload, OperationSet,
    OperationStamp, PositionRemapping, PreconditionFailureReason, SetUserSystemBreakOp,
    TransactionCategory, TransactionDescriptor, TupletCompensation, UndoPolicy,
    UndoTransactionPayload,
};

fn envelope(
    replica: u64,
    counter: u64,
    physical: i64,
    context: CausalContext,
    transaction: Option<TransactionId>,
    payload: OperationPayload,
) -> OperationEnvelope {
    let id = OperationId::new(ReplicaId(replica), counter);
    OperationEnvelope {
        id,
        author: AuthorId(0),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
        causal_context: context,
        transaction,
        payload,
    }
}

fn target(score: &Score) -> (StaffInstanceId, VoiceId) {
    let instance = &score.canvas.regions[0].staff_instances()[0];
    (instance.id, instance.voices[0].id)
}

fn insert(
    staff_instance: StaffInstanceId,
    voice: VoiceId,
    event: EventId,
    pitch: PitchId,
    position: i32,
) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
        staff_instance,
        event: valuegen::insert_event_value(
            event,
            voice,
            MusicalPosition(RationalTime::from_int(position)),
            MusicalDuration::whole(),
            &[pitch],
        ),
    }))
}

fn voice(score: &Score, id: VoiceId) -> Option<&epiphany_core::Voice> {
    score
        .voices()
        .find_map(|(_, _, voice)| (voice.id == id).then_some(voice))
}

#[test]
fn insert_materializes_in_the_real_arena_and_voice() {
    let base = epiphany_core::generators::valid_score(100);
    let (staff_instance, target_voice) = target(&base);
    let event = EventId::new(ReplicaId(50), 0);
    let pitch = PitchId::new(ReplicaId(50), 1);
    let op = envelope(
        50,
        0,
        10,
        CausalContext::new(),
        None,
        insert(staff_instance, target_voice, event, pitch, 100),
    );
    let mut set = OperationSet::new();
    set.accept(op);

    let result = set.reduce_onto(&base);

    assert!(result.score.events.contains(event));
    assert!(voice(&result.score, target_voice)
        .expect("target voice remains present")
        .events
        .contains(&event));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn graph_reduction_rejects_an_unknown_voice_without_creating_it() {
    let base = epiphany_core::generators::valid_score(101);
    let (staff_instance, _) = target(&base);
    let missing_voice = VoiceId::new(ReplicaId(51), 99);
    let event = EventId::new(ReplicaId(51), 0);
    let op = envelope(
        51,
        0,
        10,
        CausalContext::new(),
        None,
        insert(
            staff_instance,
            missing_voice,
            event,
            PitchId::new(ReplicaId(51), 1),
            100,
        ),
    );
    let mut set = OperationSet::new();
    set.accept(op.clone());

    let result = set.reduce_onto(&base);

    assert!(!result.score.events.contains(event));
    assert!(voice(&result.score, missing_voice).is_none());
    assert_eq!(
        result.state.effects,
        vec![(
            op.id,
            OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::VoiceMissing,
                },
            },
        )]
    );
}

#[test]
fn concurrent_overlap_materializes_an_invariant_clean_promoted_voice() {
    let base = epiphany_core::generators::valid_score(102);
    let (staff_instance, target_voice) = target(&base);
    let winner = envelope(
        52,
        0,
        10,
        CausalContext::new(),
        None,
        insert(
            staff_instance,
            target_voice,
            EventId::new(ReplicaId(52), 10),
            PitchId::new(ReplicaId(52), 11),
            100,
        ),
    );
    let loser = envelope(
        53,
        0,
        10,
        CausalContext::new(),
        None,
        insert(
            staff_instance,
            target_voice,
            EventId::new(ReplicaId(53), 10),
            PitchId::new(ReplicaId(53), 11),
            100,
        ),
    );
    let promoted = derive_promoted_voice_id(staff_instance, target_voice, winner.id, loser.id);
    let mut set = OperationSet::new();
    set.accept_all(vec![loser.clone(), winner.clone()]);
    let mut reversed = OperationSet::new();
    reversed.accept_all(vec![winner.clone(), loser.clone()]);

    let result = set.reduce_onto(&base);
    assert_eq!(result, reversed.reduce_onto(&base));
    let promoted_voice = voice(&result.score, promoted).expect("promoted voice was materialized");

    assert!(promoted_voice
        .events
        .contains(&EventId::new(ReplicaId(53), 10)));
    assert_eq!(
        promoted_voice.origin,
        VoiceOrigin::SystemPromoted {
            winning_operation: winner.id,
            losing_operation: loser.id,
            original_voice: target_voice,
        }
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn delete_removes_the_event_and_records_graph_tombstones() {
    let base = epiphany_core::generators::valid_score(103);
    let (staff_instance, target_voice) = target(&base);
    let event = EventId::new(ReplicaId(54), 10);
    let pitch = PitchId::new(ReplicaId(54), 11);
    let insertion = envelope(
        54,
        0,
        10,
        CausalContext::new(),
        None,
        insert(staff_instance, target_voice, event, pitch, 100),
    );
    let deletion = envelope(
        54,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(54), 0),
        None,
        OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![deletion, insertion]);

    let result = set.reduce_onto(&base);

    assert!(!result.score.events.contains(event));
    assert!(!voice(&result.score, target_voice)
        .expect("target voice remains present")
        .events
        .contains(&event));
    assert!(result.score.tombstoned_events.contains(&event));
    assert!(result.score.tombstoned_pitches.contains(&pitch));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn failed_transaction_rolls_back_real_graph_mutations() {
    let base = epiphany_core::generators::valid_score(104);
    let (staff_instance, target_voice) = target(&base);
    let tx = TransactionId::from_raw(77);
    let descriptor = envelope(
        55,
        0,
        10,
        CausalContext::new(),
        Some(tx),
        OperationPayload::Primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("graph rollback"),
            category: Some(TransactionCategory::NoteEntry),
        })),
    );
    let tx_context = CausalContext::new().with_seen(ReplicaId(55), 0);
    let inserted_event = EventId::new(ReplicaId(55), 10);
    let insertion = envelope(
        55,
        1,
        11,
        tx_context.clone(),
        Some(tx),
        insert(
            staff_instance,
            target_voice,
            inserted_event,
            PitchId::new(ReplicaId(55), 11),
            100,
        ),
    );
    let failing = envelope(
        55,
        2,
        12,
        tx_context,
        Some(tx),
        OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: EventId::new(ReplicaId(55), 999),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![failing, insertion, descriptor]);

    let result = set.reduce_onto(&base);

    assert!(!result.score.events.contains(inserted_event));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn forward_undo_removes_transaction_mints_from_the_graph() {
    let base = epiphany_core::generators::valid_score(105);
    let (staff_instance, target_voice) = target(&base);
    let tx = TransactionId::from_raw(78);
    let descriptor = envelope(
        56,
        0,
        10,
        CausalContext::new(),
        Some(tx),
        OperationPayload::Primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("graph undo"),
            category: None,
        })),
    );
    let inserted_event = EventId::new(ReplicaId(56), 10);
    let insertion = envelope(
        56,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(56), 0),
        Some(tx),
        insert(
            staff_instance,
            target_voice,
            inserted_event,
            PitchId::new(ReplicaId(56), 11),
            100,
        ),
    );
    let undo = envelope(
        56,
        2,
        12,
        CausalContext::new().with_seen(ReplicaId(56), 1),
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![undo, insertion, descriptor]);

    let result = set.reduce_onto(&base);

    assert!(!result.score.events.contains(inserted_event));
    assert!(result.score.tombstoned_events.contains(&inserted_event));
    assert!(matches!(
        result
            .state
            .objects
            .get(&TypedObjectId::Event(inserted_event)),
        Some(epiphany_ops::ObjectState::Tombstoned { .. })
    ));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn system_break_lww_state_is_materialized_in_the_region() {
    let base = epiphany_core::generators::valid_score(106);
    let region = base.canvas.regions[0].id;
    let position = MusicalPosition(RationalTime::from_int(8));
    let operation = envelope(
        57,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
            region,
            anchor: valuegen::region_start_anchor(region, position.clone()),
            present: true,
        })),
    );
    let mut set = OperationSet::new();
    set.accept(operation);

    let result = set.reduce_onto(&base);
    let breaks = &result.score.canvas.regions[0]
        .content
        .staff_based()
        .expect("fixture is staff based")
        .user_system_breaks;

    assert_eq!(
        breaks,
        &[TimeAnchor::Region {
            id: region,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration(position.0)),
        }]
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn migration_computes_incompatible_events_from_the_graph() {
    let base = epiphany_core::generators::valid_score(107);
    let region = base.canvas.regions[0].id;
    let operation = envelope(
        58,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region,
                new_time_model: valuegen::proportional_model(),
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::PreserveTime,
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept(operation);

    let result = set.reduce_onto(&base);

    assert_eq!(result.score, base);
    assert!(result
        .state
        .conflicts
        .records()
        .iter()
        .any(|record| matches!(record.kind, ConflictKind::TimeModelMigrationFailure { .. })));
}

#[test]
fn create_cross_cutting_materializes_supported_graph_structures() {
    let base = epiphany_core::generators::valid_score(108);
    let endpoints = base.canvas.regions[0].staff_instances()[0].voices[0].events[..2].to_vec();
    let slur = SlurId::new(ReplicaId(59), 10);
    let operation = envelope(
        59,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: CrossCuttingValue::Slur(valuegen::slur(slur, endpoints[0], endpoints[1])),
        })),
    );
    let mut set = OperationSet::new();
    set.accept(operation);

    let result = set.reduce_onto(&base);

    assert!(result
        .score
        .cross_cutting
        .slurs
        .iter()
        .any(|value| value.id == slur));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn causally_ordered_time_migrations_do_not_conflict() {
    let base = epiphany_core::generators::valid_score(109);
    let region = base.canvas.regions[0].id;
    let first = envelope(
        60,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region,
                new_time_model: valuegen::aleatoric_model(),
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::PreserveTime,
            },
        )),
    );
    let second = envelope(
        60,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(60), 0),
        None,
        OperationPayload::Primitive(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region,
                new_time_model: valuegen::metric_model(),
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::PreserveTime,
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![second, first]);

    let result = set.reduce_onto(&base);

    assert!(result.state.conflicts.is_empty());
    assert!(matches!(
        result.score.canvas.regions[0].time_model,
        RegionTimeModel::Metric(_)
    ));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn graph_materialization_is_deterministic_across_base_corpus_and_delivery_order() {
    for seed in 0..64_u64 {
        let base = epiphany_core::generators::valid_score(1_000 + seed);
        let (staff_instance, target_voice) = target(&base);
        let winner = envelope(
            0xC001,
            seed,
            10,
            CausalContext::new(),
            None,
            insert(
                staff_instance,
                target_voice,
                EventId::new(ReplicaId(0xC001), seed),
                PitchId::new(ReplicaId(0xC001), seed),
                100,
            ),
        );
        let loser = envelope(
            0xC002,
            seed,
            10,
            CausalContext::new(),
            None,
            insert(
                staff_instance,
                target_voice,
                EventId::new(ReplicaId(0xC002), seed),
                PitchId::new(ReplicaId(0xC002), seed),
                100,
            ),
        );
        let mut forward = OperationSet::new();
        forward.accept_all(vec![winner.clone(), loser.clone()]);
        let mut backward = OperationSet::new();
        backward.accept_all(vec![loser, winner]);

        let expected = forward.reduce_onto(&base);
        let actual = backward.reduce_onto(&base);
        assert_eq!(actual, expected, "base seed {seed}");
        assert!(
            check_invariants(&actual.score).is_empty(),
            "base seed {seed}"
        );
    }
}

#[test]
fn delete_last_identified_pitch_degrades_the_note_to_a_rest() {
    // A single-pitch note whose only pitch is deleted must NOT materialize as an
    // empty (invalid) pitched event; Chapter 5 forbids that, so it degrades to a
    // rest of the same placement (and `check_invariants` would reject otherwise).
    let base = epiphany_core::generators::valid_score(100);
    let (staff_instance, target_voice) = target(&base);
    let event = EventId::new(ReplicaId(60), 0);
    let pitch = PitchId::new(ReplicaId(60), 1);
    let insert_note = envelope(
        60,
        0,
        10,
        CausalContext::new(),
        None,
        insert(staff_instance, target_voice, event, pitch, 100),
    );
    let delete_pitch = envelope(
        60,
        1,
        20,
        CausalContext::new().with_seen(ReplicaId(60), 0),
        None,
        OperationPayload::Primitive(OperationKind::DeleteIdentifiedPitch(
            epiphany_ops::DeleteIdentifiedPitchOp { pitch },
        )),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![insert_note, delete_pitch]);

    let result = set.reduce_onto(&base);

    assert!(
        matches!(
            result.score.events.get(event),
            Some(epiphany_core::Event::Rest(_))
        ),
        "a note whose last pitch is deleted must become a rest, not an empty chord"
    );
    assert!(
        matches!(
            result.state.objects.get(&TypedObjectId::Pitch(pitch)),
            Some(epiphany_ops::ObjectState::Tombstoned { .. })
        ),
        "the deleted pitch is tombstoned in the bookkeeping projection"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn insert_identified_pitch_into_a_rest_promotes_it_to_a_note() {
    // Adding a pitch to a rest turns the rest into a note — the dual of the
    // last-pitch delete — so the graph holds the pitch the bookkeeping minted
    // (otherwise the live pitch object would have no graph counterpart).
    let base = epiphany_core::generators::valid_score(100);
    let (staff_instance, target_voice) = target(&base);
    let rest = EventId::new(ReplicaId(61), 0);
    let insert_rest = envelope(
        61,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
            staff_instance,
            event: valuegen::insert_event_value(
                rest,
                target_voice,
                MusicalPosition(RationalTime::from_int(100)),
                MusicalDuration::whole(),
                &[],
            ),
        })),
    );
    let pitch = PitchId::new(ReplicaId(61), 1);
    let add_pitch = envelope(
        61,
        1,
        20,
        CausalContext::new().with_seen(ReplicaId(61), 0),
        None,
        OperationPayload::Primitive(OperationKind::InsertIdentifiedPitch(
            epiphany_ops::InsertIdentifiedPitchOp {
                event: rest,
                pitch: valuegen::identified_pitch(pitch),
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![insert_rest, add_pitch]);

    let result = set.reduce_onto(&base);

    match result.score.events.get(rest) {
        Some(epiphany_core::Event::Pitched(pe)) => assert!(
            pe.pitches.iter().any(|ip| ip.id == pitch),
            "the inserted pitch is present on the promoted note"
        ),
        other => panic!("expected the rest to become a note, got {other:?}"),
    }
    assert!(check_invariants(&result.score).is_empty());
}

/// Three non-overlapping events in the target voice plus a cross-cutting
/// structure over them (built by `structure`) — a self-contained corpus authored
/// on one replica so each op causally follows (and therefore sees) the events it
/// depends on. Returns the events, the create envelope, and the three inserts;
/// tests append their own op at counter 4 (causally after the create).
fn cross_cutting_fixture(
    base: &Score,
    structure: impl FnOnce(EventId, EventId, EventId) -> CrossCuttingValue,
) -> (
    EventId,
    EventId,
    EventId,
    OperationEnvelope,
    Vec<OperationEnvelope>,
) {
    let (staff_instance, target_voice) = target(base);
    let r = 70;
    let (e1, e2, e3) = (
        EventId::new(ReplicaId(r), 0),
        EventId::new(ReplicaId(r), 1),
        EventId::new(ReplicaId(r), 2),
    );
    let ins = |counter: u64, ev: EventId, pos: i32| {
        let ctx = if counter == 0 {
            CausalContext::new()
        } else {
            CausalContext::new().with_seen(ReplicaId(r), counter - 1)
        };
        envelope(
            r,
            counter,
            10 + counter as i64,
            ctx,
            None,
            insert(
                staff_instance,
                target_voice,
                ev,
                PitchId::new(ReplicaId(r), 100 + counter),
                pos,
            ),
        )
    };
    let inserts = vec![ins(0, e1, 100), ins(1, e2, 101), ins(2, e3, 102)];
    let create = envelope(
        r,
        3,
        14,
        CausalContext::new().with_seen(ReplicaId(r), 2),
        None,
        OperationPayload::Primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: structure(e1, e2, e3),
        })),
    );
    (e1, e2, e3, create, inserts)
}

/// An op authored after the fixture's create (counter 4, sees counters 0..=3).
fn after_create(payload: OperationPayload) -> OperationEnvelope {
    envelope(
        70,
        4,
        15,
        CausalContext::new().with_seen(ReplicaId(70), 3),
        None,
        payload,
    )
}

/// An event-anchored spanner over `a`..`b` (the reduction reads its endpoints
/// from the two [`TimeAnchor::Event`] anchors).
fn spanner_over(id: epiphany_core::SpannerId, a: EventId, b: EventId) -> epiphany_core::Spanner {
    epiphany_core::Spanner {
        id,
        start: TimeAnchor::Event {
            id: a,
            offset: AnchorOffset::Zero,
        },
        end: TimeAnchor::Event {
            id: b,
            offset: AnchorOffset::Zero,
        },
        staves: Vec::new(),
    }
}

/// Create a structure, then delete it; assert it leaves the graph and the
/// structure id is tombstoned (delete-wins), graph invariants intact.
fn assert_delete_removes_and_tombstones(
    structure: impl FnOnce(EventId, EventId, EventId) -> CrossCuttingValue,
    sid: TypedObjectId,
    still_present: impl Fn(&Score) -> bool,
) {
    let base = epiphany_core::generators::valid_score(100);
    let (_e1, _e2, _e3, create, inserts) = cross_cutting_fixture(&base, structure);
    let delete = after_create(OperationPayload::Primitive(
        OperationKind::DeleteCrossCutting(epiphany_ops::DeleteCrossCuttingOp { structure: sid }),
    ));
    let mut set = OperationSet::new();
    set.accept_all(inserts.into_iter().chain([create, delete]));
    let result = set.reduce_onto(&base);
    assert!(
        !still_present(&result.score),
        "DeleteCrossCutting removes {sid:?} from the graph"
    );
    assert!(
        matches!(
            result.state.objects.get(&sid),
            Some(epiphany_ops::ObjectState::Tombstoned { .. })
        ),
        "DeleteCrossCutting tombstones {sid:?} (delete-wins)"
    );
    assert!(check_invariants(&result.score).is_empty());
}

/// Create a structure, then overwrite it with `modified`; run `verify` on the
/// resulting graph (which receives the fixture's three events), invariants intact.
fn assert_modify_updates(
    initial: impl FnOnce(EventId, EventId, EventId) -> CrossCuttingValue,
    modified: impl FnOnce(EventId, EventId, EventId) -> CrossCuttingValue,
    verify: impl FnOnce(&Score, EventId, EventId, EventId),
) {
    let base = epiphany_core::generators::valid_score(100);
    let (e1, e2, e3, create, inserts) = cross_cutting_fixture(&base, initial);
    let modify = after_create(OperationPayload::Primitive(
        OperationKind::ModifyCrossCutting(epiphany_ops::ModifyCrossCuttingOp {
            structure: modified(e1, e2, e3),
        }),
    ));
    let mut set = OperationSet::new();
    set.accept_all(inserts.into_iter().chain([create, modify]));
    let result = set.reduce_onto(&base);
    verify(&result.score, e1, e2, e3);
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn delete_cross_cutting_removes_the_structure_from_the_graph() {
    let slur = SlurId::new(ReplicaId(70), 9);
    // First confirm the create actually materializes (the delete assertions below
    // would pass vacuously if it didn't).
    let base = epiphany_core::generators::valid_score(100);
    let (_e1, _e2, _e3, create, inserts) = cross_cutting_fixture(&base, |e1, e2, _| {
        CrossCuttingValue::Slur(valuegen::slur(slur, e1, e2))
    });
    let mut created = OperationSet::new();
    created.accept_all(inserts.into_iter().chain([create]));
    assert!(
        created
            .reduce_onto(&base)
            .score
            .cross_cutting
            .slurs
            .iter()
            .any(|s| s.id == slur),
        "CreateCrossCutting materializes the slur in the graph"
    );

    assert_delete_removes_and_tombstones(
        |e1, e2, _| CrossCuttingValue::Slur(valuegen::slur(slur, e1, e2)),
        TypedObjectId::Slur(slur),
        move |score| score.cross_cutting.slurs.iter().any(|s| s.id == slur),
    );
}

#[test]
fn delete_cross_cutting_handles_tie_beam_and_spanner() {
    let tie = epiphany_core::TieId::new(ReplicaId(70), 1);
    assert_delete_removes_and_tombstones(
        |e1, e2, _| CrossCuttingValue::Tie(valuegen::tie(tie, e1, e2)),
        TypedObjectId::Tie(tie),
        move |score| score.cross_cutting.ties.iter().any(|t| t.id == tie),
    );
    let beam = epiphany_core::BeamId::new(ReplicaId(70), 1);
    assert_delete_removes_and_tombstones(
        |e1, e2, e3| CrossCuttingValue::Beam(valuegen::beam(beam, vec![e1, e2, e3])),
        TypedObjectId::Beam(beam),
        move |score| score.cross_cutting.beams.iter().any(|b| b.id == beam),
    );
    let spanner = epiphany_core::SpannerId::new(ReplicaId(70), 1);
    assert_delete_removes_and_tombstones(
        |e1, e2, _| CrossCuttingValue::Spanner(spanner_over(spanner, e1, e2)),
        TypedObjectId::Spanner(spanner),
        move |score| score.cross_cutting.spanners.iter().any(|s| s.id == spanner),
    );
}

#[test]
fn modify_cross_cutting_updates_the_structure_in_the_graph() {
    let slur = SlurId::new(ReplicaId(70), 9);
    // Re-point the slur's end from e2 to e3 (a different live endpoint).
    assert_modify_updates(
        |e1, e2, _| CrossCuttingValue::Slur(valuegen::slur(slur, e1, e2)),
        |e1, _, e3| CrossCuttingValue::Slur(valuegen::slur(slur, e1, e3)),
        move |score, _e1, _e2, e3| {
            let s = score
                .cross_cutting
                .slurs
                .iter()
                .find(|s| s.id == slur)
                .expect("the slur is still present after a modify");
            assert_eq!(s.end_event, e3, "modify updates the slur's endpoint");
        },
    );
}

#[test]
fn modify_cross_cutting_updates_tie_beam_and_spanner() {
    let tie = epiphany_core::TieId::new(ReplicaId(70), 1);
    assert_modify_updates(
        |e1, e2, _| CrossCuttingValue::Tie(valuegen::tie(tie, e1, e2)),
        |e1, _, e3| CrossCuttingValue::Tie(valuegen::tie(tie, e1, e3)),
        move |score, _e1, _e2, e3| {
            let t = score
                .cross_cutting
                .ties
                .iter()
                .find(|t| t.id == tie)
                .expect("the tie is still present after a modify");
            assert_eq!(t.end_event, e3, "modify updates the tie's endpoint");
        },
    );
    let beam = epiphany_core::BeamId::new(ReplicaId(70), 1);
    assert_modify_updates(
        |e1, e2, _| CrossCuttingValue::Beam(valuegen::beam(beam, vec![e1, e2])),
        |e1, e2, e3| CrossCuttingValue::Beam(valuegen::beam(beam, vec![e1, e2, e3])),
        move |score, _e1, _e2, _e3| {
            let b = score
                .cross_cutting
                .beams
                .iter()
                .find(|b| b.id == beam)
                .expect("the beam is still present after a modify");
            assert_eq!(b.events.len(), 3, "modify grows the beam to three events");
        },
    );
    let spanner = epiphany_core::SpannerId::new(ReplicaId(70), 1);
    assert_modify_updates(
        |e1, e2, _| CrossCuttingValue::Spanner(spanner_over(spanner, e1, e2)),
        |e1, _, e3| CrossCuttingValue::Spanner(spanner_over(spanner, e1, e3)),
        move |score, _e1, _e2, e3| {
            let s = score
                .cross_cutting
                .spanners
                .iter()
                .find(|s| s.id == spanner)
                .expect("the spanner is still present after a modify");
            assert!(
                matches!(s.end, TimeAnchor::Event { id, .. } if id == e3),
                "modify re-points the spanner's end anchor"
            );
        },
    );
}

#[test]
fn modify_cross_cutting_rejects_an_undersized_beam() {
    // Dropping a beam below two events is a precondition NoOp (mirrors
    // CreateCrossCutting); the graph keeps the original beam, hitting the
    // `beam.events.len() < 2` branch of modify_cross_cutting.
    let base = epiphany_core::generators::valid_score(100);
    let beam = epiphany_core::BeamId::new(ReplicaId(70), 1);
    let (e1, e2, _e3, create, inserts) = cross_cutting_fixture(&base, |e1, e2, _| {
        CrossCuttingValue::Beam(valuegen::beam(beam, vec![e1, e2]))
    });
    let shrink = after_create(OperationPayload::Primitive(
        OperationKind::ModifyCrossCutting(epiphany_ops::ModifyCrossCuttingOp {
            structure: CrossCuttingValue::Beam(valuegen::beam(beam, vec![e1])),
        }),
    ));
    let shrink_id = shrink.id;
    let mut set = OperationSet::new();
    set.accept_all(inserts.into_iter().chain([create, shrink]));
    let result = set.reduce_onto(&base);
    assert!(
        matches!(
            result
                .state
                .effects
                .iter()
                .find(|(id, _)| *id == shrink_id)
                .map(|(_, e)| e),
            Some(OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction { .. }
            })
        ),
        "a beam-modify dropping below two events is a precondition NoOp"
    );
    let materialized = result
        .score
        .cross_cutting
        .beams
        .iter()
        .find(|b| b.id == beam)
        .expect("the original beam survives the rejected modify");
    assert_eq!(
        materialized.events,
        vec![e1, e2],
        "the rejected modify left the original two-event beam"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn deleting_a_slur_endpoint_reanchors_in_both_graph_and_ledger() {
    // Deleting one endpoint of a slur re-anchors it onto the survivor: the slur
    // stays Live in the ledger AND present in the graph (collapsed onto the
    // surviving endpoint), so the two never disagree on its existence.
    let base = epiphany_core::generators::valid_score(100);
    let slur = SlurId::new(ReplicaId(70), 9);
    let (e1, e2, _e3, create, inserts) = cross_cutting_fixture(&base, |a, b, _| {
        CrossCuttingValue::Slur(valuegen::slur(slur, a, b))
    });
    let delete_e1 = after_create(OperationPayload::Primitive(OperationKind::DeleteEvent(
        DeleteEventOp {
            event: e1,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        },
    )));
    let mut set = OperationSet::new();
    set.accept_all(inserts.into_iter().chain([create, delete_e1]));
    let result = set.reduce_onto(&base);

    assert_eq!(
        result.state.objects.get(&TypedObjectId::Slur(slur)),
        Some(&epiphany_ops::ObjectState::Live),
        "a re-anchored slur stays live in the ledger"
    );
    let materialized = result
        .score
        .cross_cutting
        .slurs
        .iter()
        .find(|s| s.id == slur)
        .expect("the re-anchored slur is still present in the graph");
    assert_eq!(
        (materialized.start_event, materialized.end_event),
        (e2, e2),
        "the slur collapses onto the surviving endpoint"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn deleting_both_slur_endpoints_cascades_in_both_graph_and_ledger() {
    // With no endpoint surviving, the slur cascade-deletes: tombstoned in the
    // ledger AND removed from the graph (the other side of the same coin).
    let base = epiphany_core::generators::valid_score(100);
    let slur = SlurId::new(ReplicaId(70), 9);
    let (e1, e2, _e3, create, inserts) = cross_cutting_fixture(&base, |a, b, _| {
        CrossCuttingValue::Slur(valuegen::slur(slur, a, b))
    });
    let delete_e1 = after_create(OperationPayload::Primitive(OperationKind::DeleteEvent(
        DeleteEventOp {
            event: e1,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        },
    )));
    let delete_e2 = envelope(
        70,
        5,
        16,
        CausalContext::new().with_seen(ReplicaId(70), 4),
        None,
        OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: e2,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let mut set = OperationSet::new();
    set.accept_all(inserts.into_iter().chain([create, delete_e1, delete_e2]));
    let result = set.reduce_onto(&base);

    assert!(
        matches!(
            result.state.objects.get(&TypedObjectId::Slur(slur)),
            Some(epiphany_ops::ObjectState::Tombstoned { .. })
        ),
        "a slur with no surviving endpoint is tombstoned in the ledger"
    );
    assert!(
        !result
            .score
            .cross_cutting
            .slurs
            .iter()
            .any(|s| s.id == slur),
        "the cascaded slur is removed from the graph"
    );
    assert!(check_invariants(&result.score).is_empty());
}

/// Helper: the effect recorded for `id` in a reduction.
fn effect_of(result: &epiphany_ops::GraphMaterialization, id: OperationId) -> OperationEffect {
    result
        .state
        .effects
        .iter()
        .find(|(e, _)| *e == id)
        .map(|(_, eff)| eff.clone())
        .expect("the operation has an effect")
}

#[test]
fn structural_containers_create_and_empty_only_delete_in_the_graph() {
    use epiphany_core::{RegionId, StaffInstanceId};
    let base = epiphany_core::generators::valid_score(100);
    let staff = base.staves[0].id;
    let region = RegionId::new(ReplicaId(72), 0);
    let instance = StaffInstanceId::new(ReplicaId(72), 1);
    let v = VoiceId::new(ReplicaId(72), 2);

    let r = 72;
    let create_region = envelope(
        r,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateRegion(epiphany_ops::CreateRegionOp {
            region: valuegen::region(region),
        })),
    );
    let create_instance = envelope(
        r,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(r), 0),
        None,
        OperationPayload::Primitive(OperationKind::CreateStaffInstance(
            epiphany_ops::CreateStaffInstanceOp {
                region,
                instance: valuegen::staff_instance(instance, staff),
            },
        )),
    );
    let create_voice = envelope(
        r,
        2,
        12,
        CausalContext::new().with_seen(ReplicaId(r), 1),
        None,
        OperationPayload::Primitive(OperationKind::CreateVoice(epiphany_ops::CreateVoiceOp {
            staff_instance: instance,
            voice: valuegen::voice(v),
        })),
    );
    let creates = [
        create_region.clone(),
        create_instance.clone(),
        create_voice.clone(),
    ];

    // Creates materialize the container subtree, invariant-clean.
    let mut after_create = OperationSet::new();
    after_create.accept_all(creates.clone());
    let created = after_create.reduce_onto(&base);
    let materialized_region = created
        .score
        .canvas
        .regions
        .iter()
        .find(|rg| rg.id == region)
        .expect("the region is materialized");
    assert!(
        materialized_region
            .staff_instances()
            .iter()
            .any(|i| i.id == instance),
        "the staff instance is materialized in its region"
    );
    assert!(
        materialized_region
            .staff_instances()
            .iter()
            .flat_map(|i| &i.voices)
            .any(|vo| vo.id == v),
        "the voice is materialized in its staff instance"
    );
    assert!(check_invariants(&created.score).is_empty());

    // Empty-only: deleting the non-empty region (and instance) is refused.
    let del_region_early = envelope(
        r,
        3,
        13,
        CausalContext::new().with_seen(ReplicaId(r), 2),
        None,
        OperationPayload::Primitive(OperationKind::DeleteRegion(epiphany_ops::DeleteRegionOp {
            region,
        })),
    );
    let mut early = OperationSet::new();
    early.accept_all(creates.iter().cloned().chain([del_region_early.clone()]));
    let early_res = early.reduce_onto(&base);
    assert!(
        early_res
            .score
            .canvas
            .regions
            .iter()
            .any(|rg| rg.id == region),
        "a non-empty region delete is refused and leaves the region in the graph"
    );
    assert_eq!(
        effect_of(&early_res, del_region_early.id),
        OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::ContainerNotEmpty,
            },
        },
        "the refused delete reports ContainerNotEmpty"
    );

    // Ordered teardown (voice, instance, region) clears the subtree from graph
    // and ledger, invariant-clean.
    let del_voice = envelope(
        r,
        3,
        13,
        CausalContext::new().with_seen(ReplicaId(r), 2),
        None,
        OperationPayload::Primitive(OperationKind::DeleteVoice(epiphany_ops::DeleteVoiceOp {
            voice: v,
        })),
    );
    let del_instance = envelope(
        r,
        4,
        14,
        CausalContext::new().with_seen(ReplicaId(r), 3),
        None,
        OperationPayload::Primitive(OperationKind::DeleteStaffInstance(
            epiphany_ops::DeleteStaffInstanceOp {
                staff_instance: instance,
            },
        )),
    );
    let del_region = envelope(
        r,
        5,
        15,
        CausalContext::new().with_seen(ReplicaId(r), 4),
        None,
        OperationPayload::Primitive(OperationKind::DeleteRegion(epiphany_ops::DeleteRegionOp {
            region,
        })),
    );
    let mut teardown = OperationSet::new();
    teardown.accept_all(
        creates
            .into_iter()
            .chain([del_voice, del_instance, del_region]),
    );
    let result = teardown.reduce_onto(&base);
    assert!(
        !result.score.canvas.regions.iter().any(|rg| rg.id == region),
        "the region is removed from the graph after the ordered teardown"
    );
    for obj in [
        TypedObjectId::Region(region),
        TypedObjectId::StaffInstance(instance),
        TypedObjectId::Voice(v),
    ] {
        assert!(
            matches!(
                result.state.objects.get(&obj),
                Some(epiphany_ops::ObjectState::Tombstoned { .. })
            ),
            "{obj:?} is tombstoned in the ledger after teardown"
        );
    }
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn score_settings_materialize_in_the_graph_and_ledger() {
    let base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let r = 73;
    let set_metadata = envelope(
        r,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetMetadata(epiphany_ops::SetMetadataOp {
            metadata: valuegen::score_metadata(5),
        })),
    );
    let set_grid = envelope(
        r,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(r), 0),
        None,
        OperationPayload::Primitive(OperationKind::SetMetricGrid(
            epiphany_ops::SetMetricGridOp {
                region,
                grid: Some(valuegen::metric_grid()),
            },
        )),
    );
    let set_page = envelope(
        r,
        2,
        12,
        CausalContext::new().with_seen(ReplicaId(r), 1),
        None,
        OperationPayload::Primitive(OperationKind::SetUserPageBreak(
            epiphany_ops::SetUserPageBreakOp {
                region,
                anchor: valuegen::region_start_anchor(
                    region,
                    MusicalPosition(RationalTime::from_int(0)),
                ),
                present: true,
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![set_metadata, set_grid, set_page]);
    let result = set.reduce_onto(&base);

    assert_eq!(
        result.score.metadata.title.as_deref(),
        Some("title-5"),
        "SetMetadata overwrites the score metadata in the graph"
    );
    let materialized_region = result
        .score
        .canvas
        .regions
        .iter()
        .find(|rg| rg.id == region)
        .expect("the region is present");
    assert!(
        matches!(
            &materialized_region.content,
            epiphany_core::RegionContent::StaffBased(c) if c.default_metric_grid.is_some()
        ),
        "SetMetricGrid sets the region's default metric grid"
    );
    assert!(
        matches!(
            &materialized_region.content,
            epiphany_core::RegionContent::StaffBased(c) if !c.user_page_breaks.is_empty()
        ),
        "SetUserPageBreak adds the anchor to the region's user page breaks"
    );
    assert!(
        !result.state.page_breaks.is_empty(),
        "the page break is recorded in the canonical MaterializedState.page_breaks"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn concurrent_differing_set_metadata_is_advisory_lww() {
    let base = epiphany_core::generators::valid_score(100);
    // Two concurrent SetMetadata (neither sees the other) with differing values.
    let a = envelope(
        74,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetMetadata(epiphany_ops::SetMetadataOp {
            metadata: valuegen::score_metadata(1),
        })),
    );
    let b = envelope(
        75,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetMetadata(epiphany_ops::SetMetadataOp {
            metadata: valuegen::score_metadata(2),
        })),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![a.clone(), b.clone()]);
    let result = set.reduce_onto(&base);

    // Metadata is an advisory last-writer-wins field: a clean concurrent edit
    // raises no conflict and leaves the materialized state clean (matching the
    // catalog/core-spec "LWW advisory" classification).
    assert!(
        result.state.conflicts.records().is_empty(),
        "concurrent differing SetMetadata is advisory — it records no conflict"
    );
    assert!(
        result.state.is_clean(),
        "an advisory metadata edit keeps the materialized state clean"
    );
    assert!(
        result
            .state
            .effects
            .iter()
            .all(|(_, effect)| matches!(effect, OperationEffect::Applied)),
        "both writes apply; the last in canonical order silently wins"
    );

    // The resolved value is one of the two writes and is permutation-independent.
    let resolved = result.score.metadata.clone();
    assert!(
        resolved == valuegen::score_metadata(1) || resolved == valuegen::score_metadata(2),
        "the resolved metadata is one of the concurrent writes"
    );
    let mut reversed = OperationSet::new();
    reversed.accept_all(vec![b, a]);
    assert_eq!(
        reversed.reduce_onto(&base).score.metadata,
        resolved,
        "metadata resolution is independent of acceptance order"
    );
    assert!(check_invariants(&result.score).is_empty());
}

/// Whether an effect is the precondition NoOp the layout ops use for a target
/// that is missing, tombstoned, or not staff-based.
fn is_target_missing(effect: &OperationEffect) -> bool {
    matches!(
        effect,
        OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::TargetMissing,
            },
        }
    )
}

#[test]
fn set_user_page_break_on_a_missing_region_is_a_consistent_noop() {
    let base = epiphany_core::generators::valid_score(100);
    let ghost = epiphany_core::RegionId::new(ReplicaId(123), 7); // absent from the base
    let op = envelope(
        58,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetUserPageBreak(
            epiphany_ops::SetUserPageBreakOp {
                region: ghost,
                anchor: valuegen::region_start_anchor(
                    ghost,
                    MusicalPosition(RationalTime::from_int(0)),
                ),
                present: true,
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept(op.clone());

    // Graph-aware: the absent region has no break slot, so the op is a NoOp and
    // nothing enters the canonical page_breaks.
    let graph = set.reduce_onto(&base);
    assert!(is_target_missing(&effect_of(&graph, op.id)));
    assert!(
        graph.state.page_breaks.is_empty(),
        "no canonical page break is recorded for a missing region"
    );

    // Base-free: with no region ever minted the reducer reaches the same verdict,
    // so reduce() and reduce_onto() agree.
    let bookkeeping = set.reduce();
    let effect = bookkeeping
        .effects
        .iter()
        .find(|(e, _)| *e == op.id)
        .map(|(_, eff)| eff)
        .expect("the operation has an effect");
    assert!(is_target_missing(effect));
    assert!(bookkeeping.page_breaks.is_empty());
}

#[test]
fn layout_ops_on_a_free_graphic_region_are_rejected() {
    let mut base = epiphany_core::generators::valid_score(100);
    // A staff-less FreeGraphic region: it has neither a metric-grid nor a break
    // slot, so both layout ops must reject it. The staff-based index is read with
    // or without a graph, so reduce() and reduce_onto() reach the same verdict.
    let fg_id = epiphany_core::RegionId::new(ReplicaId(99), 0);
    let mut fg = valuegen::region(fg_id);
    fg.content = epiphany_core::RegionContent::FreeGraphic(epiphany_core::GraphicContent {
        objects: Vec::new(),
    });
    base.canvas.regions.push(fg);

    let page = envelope(
        59,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetUserPageBreak(
            epiphany_ops::SetUserPageBreakOp {
                region: fg_id,
                anchor: valuegen::region_start_anchor(
                    fg_id,
                    MusicalPosition(RationalTime::from_int(0)),
                ),
                present: true,
            },
        )),
    );
    let grid = envelope(
        59,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(59), 0),
        None,
        OperationPayload::Primitive(OperationKind::SetMetricGrid(
            epiphany_ops::SetMetricGridOp {
                region: fg_id,
                grid: Some(valuegen::metric_grid()),
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![page.clone(), grid.clone()]);
    let result = set.reduce_onto(&base);

    assert!(
        is_target_missing(&effect_of(&result, page.id)),
        "a page break on a FreeGraphic region is rejected"
    );
    assert!(
        is_target_missing(&effect_of(&result, grid.id)),
        "a metric grid on a FreeGraphic region is rejected"
    );
    assert!(
        result.state.page_breaks.is_empty(),
        "nothing is recorded for the FreeGraphic region"
    );
}

#[test]
fn set_metric_grid_rejects_an_undeclared_time_signature_reference() {
    let base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let grid_before = base.canvas.regions[0]
        .content
        .staff_based()
        .expect("fixture is staff based")
        .default_metric_grid
        .clone();

    // A grid whose single meter change names a time signature the score never
    // declares — the graph invariant (epiphany-core) forbids installing it.
    let bogus = epiphany_core::TimeSignatureId::new(ReplicaId(200), 1);
    let bad_grid = epiphany_core::MetricGrid {
        meter_sequence: vec![epiphany_core::MeterChange {
            anchor: valuegen::region_start_anchor(
                region,
                MusicalPosition(RationalTime::from_int(0)),
            ),
            time_signature: bogus,
        }],
    };
    let op = envelope(
        60,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetMetricGrid(
            epiphany_ops::SetMetricGridOp {
                region,
                grid: Some(bad_grid),
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept(op.clone());
    let result = set.reduce_onto(&base);

    assert!(
        is_target_missing(&effect_of(&result, op.id)),
        "a grid referencing an undeclared time signature is rejected"
    );
    let grid_after = result
        .score
        .canvas
        .regions
        .iter()
        .find(|r| r.id == region)
        .expect("the region survives")
        .content
        .staff_based()
        .expect("still staff based")
        .default_metric_grid
        .clone();
    assert_eq!(
        grid_before, grid_after,
        "the rejected grid leaves the region's metric grid unchanged"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn user_breaks_at_one_resolved_position_collapse_to_a_single_anchor() {
    let base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let offset = RationalTime::from_int(4);
    // Two structurally distinct anchors (region start vs. end) that resolve to the
    // *same* musical position — the canonical LWW key — both set present.
    let start_anchor = TimeAnchor::Region {
        id: region,
        edge: RegionEdge::Start,
        offset: AnchorOffset::Musical(MusicalDuration(offset.clone())),
    };
    let end_anchor = TimeAnchor::Region {
        id: region,
        edge: RegionEdge::End,
        offset: AnchorOffset::Musical(MusicalDuration(offset.clone())),
    };
    let first = envelope(
        61,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::SetUserPageBreak(
            epiphany_ops::SetUserPageBreakOp {
                region,
                anchor: start_anchor,
                present: true,
            },
        )),
    );
    let second = envelope(
        61,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(61), 0),
        None,
        OperationPayload::Primitive(OperationKind::SetUserPageBreak(
            epiphany_ops::SetUserPageBreakOp {
                region,
                anchor: end_anchor.clone(),
                present: true,
            },
        )),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![first, second]);
    let result = set.reduce_onto(&base);

    let breaks = &result
        .score
        .canvas
        .regions
        .iter()
        .find(|r| r.id == region)
        .expect("the region survives")
        .content
        .staff_based()
        .expect("fixture is staff based")
        .user_page_breaks;
    assert_eq!(
        breaks.as_slice(),
        &[end_anchor],
        "two anchors at one resolved position collapse to the single last writer"
    );
    assert_eq!(
        result.state.page_breaks.len(),
        1,
        "exactly one canonical LWW slot for the resolved position"
    );
    assert!(check_invariants(&result.score).is_empty());
}
