//! M2 regression coverage for reducing operations onto Agent B's real score
//! graph rather than only the Chapter 6 bookkeeping projection.

use epiphany_core::{
    check_invariants, derive_promoted_voice_id, AnchorOffset, EventId, MusicalDuration,
    MusicalPosition, OperationId, PitchId, RationalTime, RegionEdge, RegionTimeModel, ReplicaId,
    Score, SlurId, StaffInstanceId, TimeAnchor, TransactionId, TypedObjectId, VoiceId, VoiceOrigin,
    WallClockTime,
};
use epiphany_ops::{
    AuthorId, CausalContext, ChangeRegionTimeModelOp, ConflictKind, CreateCrossCuttingOp,
    CrossCuttingRef, DeleteEventOp, HybridLogicalClock, InsertEventOp, NoOpReason, OperationEffect,
    OperationEnvelope, OperationKind, OperationPayload, OperationSet, OperationStamp,
    PositionRemapping, PreconditionFailureReason, RegionTimeModelTag, SetUserSystemBreakOp,
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
        voice,
        staff_instance,
        event,
        position: MusicalPosition(RationalTime::from_int(position)),
        duration: MusicalDuration::whole(),
        pitches: vec![pitch],
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
            anchor: position.clone(),
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
                new_time_model: RegionTimeModelTag::Proportional,
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
            structure: CrossCuttingRef {
                id: TypedObjectId::Slur(slur),
                endpoints: endpoints
                    .iter()
                    .copied()
                    .map(TypedObjectId::Event)
                    .collect(),
            },
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
                new_time_model: RegionTimeModelTag::Aleatoric,
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
                new_time_model: RegionTimeModelTag::Metric,
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
