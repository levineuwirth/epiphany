//! M2 regression coverage for reducing operations onto Agent B's real score
//! graph rather than only the Chapter 6 bookkeeping projection.

use epiphany_core::{
    check_invariants, derive_promoted_voice_id, AnalyticalAnnotation, AnalyticalAnnotationId,
    AnchorOffset, AnnotationAnchor, Comment, CommentId, CueEvent, CueRendering, Event,
    EventDuration, EventId, EventPosition, GestureAnchoring, GraphicGesture, GraphicGestureId,
    Marker, MarkerId, MusicalDuration, MusicalPosition, OperationId, PitchId, RationalTime,
    RegionEdge, RegionId, RegionTimeModel, ReplicaId, Score, SlurId, StaffId, StaffInstanceId,
    TimeAnchor, TimeSignatureId, TransactionId, TypedObjectId, VoiceId, VoiceOrigin, WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, ChangeRegionTimeModelOp, ConflictKind, CreateCrossCuttingOp,
    CreateStaffInstanceOp, CreateStaffOp, CrossCuttingValue, DeleteEventOp, DeleteStaffInstanceOp,
    HybridLogicalClock, InsertEventOp, ModifyEventOp, NoOpReason, OperationEffect,
    OperationEnvelope, OperationKind, OperationPayload, OperationSet, OperationStamp,
    PositionRemapping, PreconditionFailureReason, ReanchorReason, RepairKind, RepairRecord,
    RespellPitchOp, SetMetricGridOp, SetStaffLayoutOp, SetTempoSegmentOp, SetTimeSignatureOp,
    SetUserSystemBreakOp, TransactionCategory, TransactionDescriptor, TupletCompensation,
    UndoPolicy, UndoTransactionPayload,
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

#[test]
fn cascade_delete_tuplet_prunes_dangling_decompositions() {
    // `valid_score_rich`'s metric region is a 3:2 triplet whose first member carries an
    // in-tuplet decomposition. Cascade-deleting the tuplet — member 0 carries the
    // `CascadeDeleteTuplets`, the rest delete as ordinary (no-longer-tuplet) events —
    // must also drop that decomposition; otherwise its tuplet reference would dangle
    // (invariant 6, cross-cutting refs resolve).
    let base = epiphany_core::generators::valid_score_rich(0x5EED);
    let tuplet = base.cross_cutting.tuplets[0].id;
    let members = base.cross_cutting.tuplets[0].members.clone();
    assert_eq!(members.len(), 3, "the fixture triplet has three members");
    assert!(
        base.decomposition_attachments
            .iter()
            .any(|d| d.components.iter().any(|c| c.tuplet == Some(tuplet))),
        "the fixture has an in-tuplet decomposition referencing the triplet"
    );

    // Three deletes in causal order: the structure-removing cascade first, so the
    // remaining members then delete as ordinary events.
    let mut ops = Vec::new();
    for (i, &member) in members.iter().enumerate() {
        let counter = (i + 1) as u64;
        let compensation = if i == 0 {
            TupletCompensation::CascadeDeleteTuplets {
                tuplets: vec![tuplet],
            }
        } else {
            TupletCompensation::NotInTuplet
        };
        ops.push(envelope(
            70,
            counter,
            10 + i as i64,
            CausalContext::new(),
            None,
            OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
                event: member,
                tuplet_compensation: compensation,
            })),
        ));
    }
    let mut set = OperationSet::new();
    set.accept_all(ops);
    let result = set.reduce_onto(&base);

    assert!(
        result.score.cross_cutting.tuplets.is_empty(),
        "the tuplet structure is removed"
    );
    assert!(
        !result
            .score
            .decomposition_attachments
            .iter()
            .any(|d| d.components.iter().any(|c| c.tuplet == Some(tuplet))),
        "the now-orphaned decomposition is pruned"
    );
    for &member in &members {
        assert!(
            matches!(
                result.state.objects.get(&TypedObjectId::Event(member)),
                Some(epiphany_ops::ObjectState::Tombstoned { .. })
            ),
            "each member is tombstoned"
        );
    }
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

#[test]
fn create_rejects_a_non_empty_carried_container() {
    let base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let (staff_instance, _) = target(&base);

    let rejected = |effect: OperationEffect| {
        matches!(
            effect,
            OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::ContainerNotEmpty,
                },
            }
        )
    };

    // A create mints an empty container; carrying a child the create does not
    // itself mint must be rejected (else the graph gains unminted objects).
    let fresh_region_id = epiphany_core::RegionId::new(ReplicaId(80), 0);
    let mut region_with_child = valuegen::region(fresh_region_id);
    region_with_child
        .content
        .staff_instances_mut()
        .expect("staff based")
        .push(valuegen::staff_instance(
            StaffInstanceId::new(ReplicaId(80), 1),
            base.staves[0].id,
        ));
    let create_region = envelope(
        80,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateRegion(epiphany_ops::CreateRegionOp {
            region: region_with_child,
        })),
    );

    let mut instance_with_child =
        valuegen::staff_instance(StaffInstanceId::new(ReplicaId(80), 2), base.staves[0].id);
    instance_with_child
        .voices
        .push(valuegen::voice(VoiceId::new(ReplicaId(80), 3)));
    let create_instance = envelope(
        81,
        0,
        11,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateStaffInstance(
            epiphany_ops::CreateStaffInstanceOp {
                region,
                instance: instance_with_child,
            },
        )),
    );

    let mut voice_with_child = valuegen::voice(VoiceId::new(ReplicaId(80), 4));
    voice_with_child.events.push(EventId::new(ReplicaId(80), 5));
    let create_voice = envelope(
        82,
        0,
        12,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateVoice(epiphany_ops::CreateVoiceOp {
            staff_instance,
            voice: voice_with_child,
        })),
    );

    let mut set = OperationSet::new();
    set.accept_all(vec![
        create_region.clone(),
        create_instance.clone(),
        create_voice.clone(),
    ]);
    let result = set.reduce_onto(&base);

    assert!(
        rejected(effect_of(&result, create_region.id)),
        "a region carrying a staff instance is rejected"
    );
    assert!(
        rejected(effect_of(&result, create_instance.id)),
        "a staff instance carrying a voice is rejected"
    );
    assert!(
        rejected(effect_of(&result, create_voice.id)),
        "a voice carrying an event is rejected"
    );
    assert!(
        !result
            .score
            .canvas
            .regions
            .iter()
            .any(|r| r.id == fresh_region_id),
        "the non-empty region is not materialized into the graph"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn create_rejects_carried_non_hierarchy_children() {
    let base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let staff = base.staves[0].id;

    let rejected = |effect: OperationEffect| {
        matches!(
            effect,
            OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::ContainerNotEmpty,
                },
            }
        )
    };

    // A region carrying a barline-alignment group (a typed object, not a staff
    // instance) must still be rejected.
    let mut region_with_barline = valuegen::region(epiphany_core::RegionId::new(ReplicaId(83), 0));
    region_with_barline
        .content
        .staff_based_mut()
        .expect("staff based")
        .barline_alignment_groups
        .push(epiphany_core::BarlineAlignmentGroup {
            id: epiphany_core::BarlineAlignmentGroupId::new(ReplicaId(83), 1),
            members: Vec::new(),
        });
    let create_barline = envelope(
        83,
        0,
        10,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateRegion(epiphany_ops::CreateRegionOp {
            region: region_with_barline,
        })),
    );

    // A free-graphic region carrying a graphic object.
    let mut region_with_graphic = valuegen::region(epiphany_core::RegionId::new(ReplicaId(83), 2));
    region_with_graphic.content =
        epiphany_core::RegionContent::FreeGraphic(epiphany_core::GraphicContent {
            objects: vec![epiphany_core::GraphicObject {
                id: epiphany_core::GraphicObjectId::new(ReplicaId(83), 3),
            }],
        });
    let create_graphic = envelope(
        83,
        1,
        11,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateRegion(epiphany_ops::CreateRegionOp {
            region: region_with_graphic,
        })),
    );

    // A staff instance carrying a measure (a typed object, not a voice).
    let mut instance_with_measure =
        valuegen::staff_instance(StaffInstanceId::new(ReplicaId(83), 4), staff);
    instance_with_measure.measures.push(epiphany_core::Measure {
        id: epiphany_core::MeasureId::new(ReplicaId(83), 5),
        start: TimeAnchor::WallClock {
            time: WallClockTime(0),
        },
        time_signature: None,
        explicit_number: None,
        number_visibility: Default::default(),
    });
    let create_measure = envelope(
        84,
        0,
        12,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::CreateStaffInstance(
            epiphany_ops::CreateStaffInstanceOp {
                region,
                instance: instance_with_measure,
            },
        )),
    );

    let mut set = OperationSet::new();
    set.accept_all(vec![
        create_barline.clone(),
        create_graphic.clone(),
        create_measure.clone(),
    ]);
    let result = set.reduce_onto(&base);

    assert!(
        rejected(effect_of(&result, create_barline.id)),
        "a region carrying a barline-alignment group is rejected"
    );
    assert!(
        rejected(effect_of(&result, create_graphic.id)),
        "a region carrying a graphic object is rejected"
    );
    assert!(
        rejected(effect_of(&result, create_measure.id)),
        "a staff instance carrying a measure is rejected"
    );
    assert!(check_invariants(&result.score).is_empty());
}

// === Re-anchoring rule-table coverage: markers, cue events, comments,
// analytical annotations, graphic gestures (core_spec §"The Re-Anchoring Rule
// Table", §"Total Ordering for Nearest"). All five kinds exist only via seeded
// base graphs (no operation creates them), so every scenario reduces onto a
// base. ========================================================================

/// The repairs of an `AppliedWithRepair` effect.
fn repairs_of(result: &epiphany_ops::GraphMaterialization, id: OperationId) -> Vec<RepairRecord> {
    match effect_of(result, id) {
        OperationEffect::AppliedWithRepair { repairs } => repairs,
        other => panic!("expected AppliedWithRepair, got {other:?}"),
    }
}

/// A plain (non-tuplet) DeleteEvent envelope.
fn delete_event(
    replica: u64,
    counter: u64,
    physical: i64,
    ctx: CausalContext,
    event: EventId,
) -> OperationEnvelope {
    envelope(
        replica,
        counter,
        physical,
        ctx,
        None,
        OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    )
}

/// The first voice's event list (the fixture voice all these scenarios edit).
fn first_voice_events(base: &Score) -> Vec<EventId> {
    base.canvas.regions[0].staff_instances()[0].voices[0]
        .events
        .clone()
}

/// Adds a cue event sourcing `sources` to `base`'s first voice at whole-note
/// `position` (clear of the fixture's quarter-note content in `[0, 1)`).
fn push_cue(base: &mut Score, id: EventId, sources: Vec<EventId>, position: i32) {
    let (_, voice) = target(base);
    base.events
        .insert(Event::Cue(CueEvent {
            id,
            voice,
            position: EventPosition::Musical(MusicalPosition(RationalTime::from_int(position))),
            duration: EventDuration::Musical(MusicalDuration::whole()),
            source: sources,
            rendering: CueRendering,
        }))
        .expect("fresh cue id");
    base.canvas.regions[0]
        .content
        .staff_instances_mut()
        .expect("fixture is staff based")[0]
        .voices[0]
        .events
        .push(id);
}

#[test]
fn deleting_a_cue_source_cascade_deletes_the_cue() {
    // Rule table, "Cue event / Source event": cascade-delete ("a cue with no
    // source is meaningless") — ledger tombstone, graph removal, and a
    // CascadeDeleted repair in the triggering delete's effect, all in the same
    // reduction step.
    let mut base = epiphany_core::generators::valid_score(100);
    let source = first_voice_events(&base)[0];
    let cue = EventId::new(ReplicaId(90), 0);
    push_cue(&mut base, cue, vec![source], 40);

    let del = delete_event(91, 0, 10, CausalContext::new(), source);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, del.id)
            .iter()
            .any(|r| r.kind == RepairKind::CascadeDeleted && r.target == TypedObjectId::Event(cue)),
        "the cue cascade is recorded in the triggering delete's effect"
    );
    assert!(
        matches!(
            result.state.objects.get(&TypedObjectId::Event(cue)),
            Some(epiphany_ops::ObjectState::Tombstoned { .. })
        ),
        "the cue's event id is tombstoned in the ledger"
    );
    assert!(
        !result.score.events.contains(cue),
        "the cue is removed from the event arena"
    );
    assert!(
        result.score.tombstoned_events.contains(&cue),
        "the cue is a graph tombstone"
    );
    assert!(
        !first_voice_events(&result.score).contains(&cue),
        "the cue is removed from its voice"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn a_cue_with_multiple_sources_cascades_on_any_source_deletion() {
    // The table's action is the plain "cascade-delete" on a source deletion —
    // not truncate-while-any-source-survives (the rationale-vs-action tension
    // for multi-source cues is a proposed Pass-12 row). Either source's
    // deletion cascades the cue.
    for victim_index in [0usize, 1] {
        let mut base = epiphany_core::generators::valid_score(100);
        let events = first_voice_events(&base);
        let cue = EventId::new(ReplicaId(90), 1);
        push_cue(&mut base, cue, vec![events[0], events[1]], 40);

        let del = delete_event(91, 0, 10, CausalContext::new(), events[victim_index]);
        let mut set = OperationSet::new();
        set.accept(del.clone());
        let result = set.reduce_onto(&base);

        assert!(
            repairs_of(&result, del.id)
                .iter()
                .any(|r| r.kind == RepairKind::CascadeDeleted
                    && r.target == TypedObjectId::Event(cue)),
            "deleting source #{victim_index} cascades the two-source cue"
        );
        assert!(!result.score.events.contains(cue));
        assert!(check_invariants(&result.score).is_empty());
    }
}

#[test]
fn a_cascaded_cue_reanchors_its_own_referents_transitively() {
    // A cascaded cue is itself a tombstoned event, so the same re-anchoring
    // pass runs over *its* referents in the same reduction step: a cue-of-a-cue
    // cascades along.
    let mut base = epiphany_core::generators::valid_score(100);
    let source = first_voice_events(&base)[0];
    let cue1 = EventId::new(ReplicaId(90), 2);
    let cue2 = EventId::new(ReplicaId(90), 3);
    push_cue(&mut base, cue1, vec![source], 40);
    push_cue(&mut base, cue2, vec![cue1], 44);

    let del = delete_event(91, 0, 10, CausalContext::new(), source);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    let repairs = repairs_of(&result, del.id);
    for cue in [cue1, cue2] {
        assert!(
            repairs
                .iter()
                .any(|r| r.kind == RepairKind::CascadeDeleted
                    && r.target == TypedObjectId::Event(cue)),
            "cue {cue:?} cascades in the same reduction step"
        );
        assert!(
            matches!(
                result.state.objects.get(&TypedObjectId::Event(cue)),
                Some(epiphany_ops::ObjectState::Tombstoned { .. })
            ),
            "cue {cue:?} is tombstoned in the ledger"
        );
        assert!(!result.score.events.contains(cue));
    }
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn deleting_a_comment_anchor_orphans_the_comment() {
    // Rule table, "Comment / Anchor": orphan — user content never silently
    // deleted. The comment survives (ledger Live, graph present); its anchor
    // degrades to the containing region so invariant 10 keeps holding.
    let mut base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let anchor_event = first_voice_events(&base)[0];
    let comment_id = CommentId::new(ReplicaId(90), 4);
    base.cross_cutting.comments.push(Comment {
        id: comment_id,
        anchor: AnnotationAnchor::Event(anchor_event),
        resolved: false,
    });

    let del = delete_event(91, 0, 10, CausalContext::new(), anchor_event);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, del.id)
            .iter()
            .any(|r| r.kind == RepairKind::Orphaned
                && r.target == TypedObjectId::Comment(comment_id)),
        "the orphaning is a recorded repair"
    );
    assert_eq!(
        result
            .state
            .objects
            .get(&TypedObjectId::Comment(comment_id)),
        Some(&epiphany_ops::ObjectState::Live),
        "the orphaned comment stays live in the ledger"
    );
    let comment = result
        .score
        .cross_cutting
        .comments
        .iter()
        .find(|c| c.id == comment_id)
        .expect("the orphaned comment survives in the graph");
    assert_eq!(
        comment.anchor,
        AnnotationAnchor::Region(region),
        "the dangling event anchor degrades to the containing region"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn annotation_reanchors_to_a_range_preserving_the_events_extent() {
    // Rule table, "Analytical annotation / Anchor": re-anchor to a time range
    // preserving the original extent. The fixture's second event spans
    // [1/4, 1/2), so the reconstructed range is region-start + 1/4 .. + 1/2.
    let mut base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let anchor_event = first_voice_events(&base)[1];
    let annotation_id = AnalyticalAnnotationId::new(ReplicaId(90), 5);
    base.cross_cutting.analytical.push(AnalyticalAnnotation {
        id: annotation_id,
        anchor: AnnotationAnchor::Event(anchor_event),
        layer: None,
    });

    let del = delete_event(91, 0, 10, CausalContext::new(), anchor_event);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, del.id).iter().any(|r| {
            r.target == TypedObjectId::AnalyticalAnnotation(annotation_id)
                && r.kind
                    == RepairKind::Reanchored {
                        from: TypedObjectId::Event(anchor_event),
                        to: TypedObjectId::Region(region),
                        reason: ReanchorReason::ExplicitFallback,
                    }
        }),
        "the range reconstruction is a recorded repair"
    );
    let annotation = result
        .score
        .cross_cutting
        .analytical
        .iter()
        .find(|a| a.id == annotation_id)
        .expect("the annotation survives");
    let offset_at = |num: i64, den: i64| {
        AnchorOffset::Musical(MusicalDuration(RationalTime::new(num, den).unwrap()))
    };
    assert_eq!(
        annotation.anchor,
        AnnotationAnchor::Range {
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: offset_at(1, 4),
            },
            end: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: offset_at(1, 2),
            },
        },
        "the reconstructed range covers the deleted event's exact span"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn annotation_orphans_when_the_range_cannot_be_reconstructed() {
    // The extent of a wall-clock event is not expressible as a stored
    // region-relative range in this prototype (the expressibility gap is a
    // proposed Pass-12 row), so the annotation orphans: kept, anchor degraded
    // to the containing region.
    let mut base = epiphany_core::generators::valid_score_rich(0x5EED);
    let (region, wall_clock_event) = base
        .voices()
        .find_map(|(region, _, v)| {
            v.events
                .iter()
                .copied()
                .find(|e| {
                    matches!(
                        base.events.get(*e).map(Event::position),
                        Some(EventPosition::WallClock(_))
                    )
                })
                .map(|e| (region, e))
        })
        .expect("the rich fixture has a proportional region with wall-clock events");
    let annotation_id = AnalyticalAnnotationId::new(ReplicaId(90), 6);
    base.cross_cutting.analytical.push(AnalyticalAnnotation {
        id: annotation_id,
        anchor: AnnotationAnchor::Event(wall_clock_event),
        layer: None,
    });

    let del = delete_event(91, 0, 10, CausalContext::new(), wall_clock_event);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, del.id)
            .iter()
            .any(|r| r.kind == RepairKind::Orphaned
                && r.target == TypedObjectId::AnalyticalAnnotation(annotation_id)),
        "an unreconstructable range orphans the annotation"
    );
    let annotation = result
        .score
        .cross_cutting
        .analytical
        .iter()
        .find(|a| a.id == annotation_id)
        .expect("the orphaned annotation survives");
    assert_eq!(annotation.anchor, AnnotationAnchor::Region(region));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn gesture_event_references_retarget_to_the_nearest_survivor() {
    // Rule table, "Graphic gesture / Anchor event": re-anchor to the nearest
    // surviving event of the same staff instance.
    let mut base = epiphany_core::generators::valid_score(100);
    let events = first_voice_events(&base);
    let (dead, survivor) = (events[0], events[1]);
    let gesture_id = GraphicGestureId::new(ReplicaId(90), 7);
    base.cross_cutting.graphic_gestures.push(GraphicGesture {
        id: gesture_id,
        objects: Vec::new(),
        anchoring: GestureAnchoring::Events(vec![dead]),
    });

    let del = delete_event(91, 0, 10, CausalContext::new(), dead);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, del.id).iter().any(|r| {
            r.target == TypedObjectId::GraphicGesture(gesture_id)
                && r.kind
                    == RepairKind::Reanchored {
                        from: TypedObjectId::Event(dead),
                        to: TypedObjectId::Event(survivor),
                        reason: ReanchorReason::SameVoiceNearer,
                    }
        }),
        "the gesture re-target is a recorded repair"
    );
    let gesture = result
        .score
        .cross_cutting
        .graphic_gestures
        .iter()
        .find(|g| g.id == gesture_id)
        .expect("the gesture survives");
    assert_eq!(
        gesture.anchoring,
        GestureAnchoring::Events(vec![survivor]),
        "the graph reference list agrees with the recorded repair"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn free_anchored_gestures_ignore_event_deletion() {
    // Rule table: "for Free anchoring, no action" — a free gesture follows no
    // score content, so the delete reduces with no gesture repair.
    let mut base = epiphany_core::generators::valid_score(100);
    let dead = first_voice_events(&base)[0];
    let gesture_id = GraphicGestureId::new(ReplicaId(90), 8);
    base.cross_cutting.graphic_gestures.push(GraphicGesture {
        id: gesture_id,
        objects: Vec::new(),
        anchoring: GestureAnchoring::Free,
    });

    let del = delete_event(91, 0, 10, CausalContext::new(), dead);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    assert_eq!(
        effect_of(&result, del.id),
        OperationEffect::Applied,
        "no repair is recorded for a free-anchored gesture"
    );
    let gesture = result
        .score
        .cross_cutting
        .graphic_gestures
        .iter()
        .find(|g| g.id == gesture_id)
        .expect("the gesture survives");
    assert_eq!(gesture.anchoring, GestureAnchoring::Free);
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn gesture_range_anchoring_truncates_to_the_region_edge() {
    // Rule table: "for Range anchoring, truncate" — the deterministic reading:
    // a dead start endpoint moves to its region's start edge (an end endpoint
    // would move to the end edge); the underdetermined "truncate" semantics is
    // a proposed Pass-12 row.
    let mut base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let dead = first_voice_events(&base)[0];
    let gesture_id = GraphicGestureId::new(ReplicaId(90), 9);
    let end_anchor = TimeAnchor::Region {
        id: region,
        edge: RegionEdge::End,
        offset: AnchorOffset::Zero,
    };
    base.cross_cutting.graphic_gestures.push(GraphicGesture {
        id: gesture_id,
        objects: Vec::new(),
        anchoring: GestureAnchoring::Range {
            start: TimeAnchor::Event {
                id: dead,
                offset: AnchorOffset::Zero,
            },
            end: end_anchor.clone(),
            staves: Vec::new(),
        },
    });

    let del = delete_event(91, 0, 10, CausalContext::new(), dead);
    let mut set = OperationSet::new();
    set.accept(del.clone());
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, del.id).iter().any(|r| {
            r.target == TypedObjectId::GraphicGesture(gesture_id)
                && r.kind
                    == RepairKind::Reanchored {
                        from: TypedObjectId::Event(dead),
                        to: TypedObjectId::Region(region),
                        reason: ReanchorReason::ExplicitFallback,
                    }
        }),
        "the range truncation is a recorded repair"
    );
    let gesture = result
        .score
        .cross_cutting
        .graphic_gestures
        .iter()
        .find(|g| g.id == gesture_id)
        .expect("the gesture survives");
    assert_eq!(
        gesture.anchoring,
        GestureAnchoring::Range {
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Zero,
            },
            end: end_anchor,
            staves: Vec::new(),
        },
        "the dead start endpoint moved to the region's start edge"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn marker_reanchor_breaks_full_ties_by_ascending_event_id() {
    // Four-key ordering, key 4: with equal proximity rank, distance, and
    // direction, the ascending typed-id byte order decides. The referent sits
    // alone in its own voice; two candidates in two sibling voices share its
    // exact position (rank 1, distance 0, forward) — the smaller EventId wins
    // even though it was authored later and lives in the higher-id voice.
    let mut base = epiphany_core::generators::valid_score(100);
    let (staff_instance, _) = target(&base);
    let referent_voice = VoiceId::new(ReplicaId(9), 77);
    let voice_b = VoiceId::new(ReplicaId(9), 78);
    let voice_c = VoiceId::new(ReplicaId(9), 79);
    {
        let instances = base.canvas.regions[0]
            .content
            .staff_instances_mut()
            .expect("fixture is staff based");
        instances[0].voices.push(valuegen::voice(referent_voice));
        instances[0].voices.push(valuegen::voice(voice_b));
        instances[0].voices.push(valuegen::voice(voice_c));
    }
    let referent = EventId::new(ReplicaId(95), 50);
    let larger_id = EventId::new(ReplicaId(95), 9);
    let smaller_id = EventId::new(ReplicaId(95), 3);
    let marker_id = MarkerId::new(ReplicaId(90), 10);
    base.cross_cutting.markers.push(Marker {
        id: marker_id,
        anchor: TimeAnchor::Event {
            id: referent,
            offset: AnchorOffset::Zero,
        },
    });

    let ins = |counter: u64, event: EventId, voice: VoiceId, pitch: u64| {
        let ctx = if counter == 0 {
            CausalContext::new()
        } else {
            CausalContext::new().with_seen(ReplicaId(95), counter - 1)
        };
        envelope(
            95,
            counter,
            10 + counter as i64,
            ctx,
            None,
            insert(
                staff_instance,
                voice,
                event,
                PitchId::new(ReplicaId(95), 100 + pitch),
                100,
            ),
        )
    };
    let del = delete_event(
        95,
        3,
        20,
        CausalContext::new().with_seen(ReplicaId(95), 2),
        referent,
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![
        ins(0, referent, referent_voice, 0),
        ins(1, larger_id, voice_b, 1),
        ins(2, smaller_id, voice_c, 2),
        del.clone(),
    ]);
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, del.id).iter().any(|r| {
            r.target == TypedObjectId::Marker(marker_id)
                && r.kind
                    == RepairKind::Reanchored {
                        from: TypedObjectId::Event(referent),
                        to: TypedObjectId::Event(smaller_id),
                        reason: ReanchorReason::SameStaffInstanceNearer,
                    }
        }),
        "a full tie falls to the ascending typed-id byte order"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn marker_orphans_when_the_staff_instance_has_no_other_live_event() {
    // Rule table, "Marker / Anchor": proximity max is the same staff instance,
    // orphan on failure. Every event of the marker's staff instance is deleted
    // (the anchored one last), so no candidate survives within the bound; the
    // marker is kept and its anchor degrades to the region start.
    let mut base = epiphany_core::generators::valid_score(100);
    let region = base.canvas.regions[0].id;
    let instance_events: Vec<EventId> = base.canvas.regions[0].staff_instances()[0]
        .voices
        .iter()
        .flat_map(|v| v.events.clone())
        .collect();
    let marked = instance_events[0];
    let marker_id = MarkerId::new(ReplicaId(90), 11);
    base.cross_cutting.markers.push(Marker {
        id: marker_id,
        anchor: TimeAnchor::Event {
            id: marked,
            offset: AnchorOffset::Zero,
        },
    });

    let mut order: Vec<EventId> = instance_events
        .iter()
        .copied()
        .filter(|e| *e != marked)
        .collect();
    order.push(marked);
    let ops: Vec<OperationEnvelope> = order
        .iter()
        .enumerate()
        .map(|(i, &event)| {
            let ctx = if i == 0 {
                CausalContext::new()
            } else {
                CausalContext::new().with_seen(ReplicaId(96), i as u64 - 1)
            };
            delete_event(96, i as u64, 10 + i as i64, ctx, event)
        })
        .collect();
    let last = ops.last().expect("at least one delete").clone();
    let mut set = OperationSet::new();
    set.accept_all(ops);
    let result = set.reduce_onto(&base);

    assert!(
        repairs_of(&result, last.id).iter().any(|r| r.kind == RepairKind::Orphaned
            && r.target == TypedObjectId::Marker(marker_id)),
        "the marker orphans when its staff instance has no other live event"
    );
    assert_eq!(
        result.state.objects.get(&TypedObjectId::Marker(marker_id)),
        Some(&epiphany_ops::ObjectState::Live),
        "the orphaned marker stays live in the ledger"
    );
    let marker = result
        .score
        .cross_cutting
        .markers
        .iter()
        .find(|m| m.id == marker_id)
        .expect("the orphaned marker survives in the graph");
    assert_eq!(
        marker.anchor,
        TimeAnchor::Region {
            id: region,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        },
        "the dangling anchor degrades to the region start"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn slur_reanchor_reason_names_the_survivors_containment_rank() {
    // The Reanchored reason on a slur's surviving-endpoint collapse names the
    // survivor's actual containment proximity to the tombstoned endpoint —
    // same voice → SameVoiceNearer, sibling voice in the same staff instance →
    // SameStaffInstanceNearer — instead of a hardcoded same-voice claim.
    let mut base = epiphany_core::generators::valid_score(100);
    let (staff_instance, voice_a) = target(&base);
    let voice_b = VoiceId::new(ReplicaId(9), 80);
    base.canvas.regions[0]
        .content
        .staff_instances_mut()
        .expect("fixture is staff based")[0]
        .voices
        .push(valuegen::voice(voice_b));

    let r = 97;
    let e1 = EventId::new(ReplicaId(r), 0);
    let e2 = EventId::new(ReplicaId(r), 1);
    let e3 = EventId::new(ReplicaId(r), 2);
    let cross_slur = SlurId::new(ReplicaId(r), 10);
    let same_slur = SlurId::new(ReplicaId(r), 11);
    let step = |counter: u64, payload: OperationPayload| {
        let ctx = if counter == 0 {
            CausalContext::new()
        } else {
            CausalContext::new().with_seen(ReplicaId(r), counter - 1)
        };
        envelope(r, counter, 10 + counter as i64, ctx, None, payload)
    };
    let create = |slur: SlurId, a: EventId, b: EventId| {
        OperationPayload::Primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: CrossCuttingValue::Slur(valuegen::slur(slur, a, b)),
        }))
    };
    let del = step(
        5,
        OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: e1,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![
        step(
            0,
            insert(
                staff_instance,
                voice_a,
                e1,
                PitchId::new(ReplicaId(r), 100),
                100,
            ),
        ),
        step(
            1,
            insert(
                staff_instance,
                voice_b,
                e2,
                PitchId::new(ReplicaId(r), 101),
                101,
            ),
        ),
        step(
            2,
            insert(
                staff_instance,
                voice_a,
                e3,
                PitchId::new(ReplicaId(r), 102),
                102,
            ),
        ),
        step(3, create(cross_slur, e1, e2)),
        step(4, create(same_slur, e3, e1)),
        del.clone(),
    ]);
    let result = set.reduce_onto(&base);

    let repairs = repairs_of(&result, del.id);
    assert!(
        repairs.iter().any(|rec| {
            rec.target == TypedObjectId::Slur(cross_slur)
                && rec.kind
                    == RepairKind::Reanchored {
                        from: TypedObjectId::Event(e1),
                        to: TypedObjectId::Event(e2),
                        reason: ReanchorReason::SameStaffInstanceNearer,
                    }
        }),
        "a sibling-voice survivor is SameStaffInstanceNearer: {repairs:?}"
    );
    assert!(
        repairs.iter().any(|rec| {
            rec.target == TypedObjectId::Slur(same_slur)
                && rec.kind
                    == RepairKind::Reanchored {
                        from: TypedObjectId::Event(e1),
                        to: TypedObjectId::Event(e3),
                        reason: ReanchorReason::SameVoiceNearer,
                    }
        }),
        "a same-voice survivor is SameVoiceNearer: {repairs:?}"
    );
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn referent_reanchoring_is_permutation_invariant() {
    // The new rule-table rows are functions of canonical order and canonical
    // state: a marker re-anchor (with distance and direction tie-breaks), a
    // cue cascade, and a comment orphan reduce to byte-identical materialized
    // state under any delivery permutation.
    let mut base = epiphany_core::generators::valid_score(100);
    let (staff_instance, voice_a) = target(&base);
    let source = first_voice_events(&base)[0];
    let cue = EventId::new(ReplicaId(90), 20);
    push_cue(&mut base, cue, vec![source], 40);
    let comment_id = CommentId::new(ReplicaId(90), 21);
    base.cross_cutting.comments.push(Comment {
        id: comment_id,
        anchor: AnnotationAnchor::Event(source),
        resolved: false,
    });
    let referent = EventId::new(ReplicaId(98), 1);
    let marker_id = MarkerId::new(ReplicaId(90), 22);
    base.cross_cutting.markers.push(Marker {
        id: marker_id,
        anchor: TimeAnchor::Event {
            id: referent,
            offset: AnchorOffset::Zero,
        },
    });

    let ins = |counter: u64, event: u64, position: i32| {
        let ctx = if counter == 0 {
            CausalContext::new()
        } else {
            CausalContext::new().with_seen(ReplicaId(98), counter - 1)
        };
        envelope(
            98,
            counter,
            10 + counter as i64,
            ctx,
            None,
            insert(
                staff_instance,
                voice_a,
                EventId::new(ReplicaId(98), event),
                PitchId::new(ReplicaId(98), 100 + event),
                position,
            ),
        )
    };
    let envelopes = vec![
        ins(0, 0, 10),
        ins(1, 1, 12),
        ins(2, 2, 14),
        delete_event(
            98,
            3,
            20,
            CausalContext::new().with_seen(ReplicaId(98), 2),
            referent,
        ),
        delete_event(
            98,
            4,
            21,
            CausalContext::new().with_seen(ReplicaId(98), 3),
            source,
        ),
    ];

    let mut reference_set = OperationSet::new();
    reference_set.accept_all(envelopes.clone());
    let reference = reference_set.reduce_onto(&base);
    assert!(check_invariants(&reference.score).is_empty());
    // Non-vacuity: all three rows actually fired.
    assert!(!reference.score.events.contains(cue), "the cue cascaded");
    let permutations: [[usize; 5]; 4] = [
        [4, 3, 2, 1, 0],
        [2, 4, 0, 3, 1],
        [3, 0, 4, 1, 2],
        [1, 2, 3, 4, 0],
    ];
    for (k, permutation) in permutations.iter().enumerate() {
        let mut set = OperationSet::new();
        set.accept_all(permutation.iter().map(|&i| envelopes[i].clone()));
        let got = set.reduce_onto(&base);
        assert_eq!(
            got, reference,
            "delivery permutation #{k} changed the materialized graph"
        );
        assert_eq!(
            got.state.canonical_bytes(),
            reference.state.canonical_bytes(),
            "delivery permutation #{k} changed the canonical bytes"
        );
    }
}

// ===========================================================================
// Phase-3 first tranche: CreateStaff, SetTimeSignature, SetTempoSegment,
// SetStaffLayout (operation_catalog §CreateStaff, §"Meter and Tempo
// Overwrites", §SetStaffLayout), and value-restoring undo (§UndoTransaction).
// ===========================================================================

fn seen(replica: u64, counter: u64) -> CausalContext {
    CausalContext::new().with_seen(ReplicaId(replica), counter)
}

fn prim(kind: OperationKind) -> OperationPayload {
    OperationPayload::Primitive(kind)
}

/// A single-replica causal chain of primitives (each member sees its
/// predecessor), optionally under one transaction.
fn chain(
    replica: u64,
    start_physical: i64,
    tx: Option<TransactionId>,
    kinds: Vec<OperationKind>,
) -> Vec<OperationEnvelope> {
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let counter = index as u64;
            let ctx = if counter == 0 {
                CausalContext::new()
            } else {
                seen(replica, counter - 1)
            };
            envelope(
                replica,
                counter,
                start_physical + counter as i64,
                ctx,
                tx,
                prim(kind),
            )
        })
        .collect()
}

fn set_meter(
    region: RegionId,
    at: i32,
    signature: Option<epiphany_core::TimeSignature>,
) -> OperationKind {
    OperationKind::SetTimeSignature(SetTimeSignatureOp {
        region,
        anchor: valuegen::region_start_anchor(region, MusicalPosition(RationalTime::from_int(at))),
        time_signature: signature,
    })
}

fn set_tempo(
    region_scope: Option<RegionId>,
    anchor_region: RegionId,
    at: i32,
    segment: Option<epiphany_core::TempoSegment>,
) -> OperationKind {
    OperationKind::SetTempoSegment(SetTempoSegmentOp {
        region: region_scope,
        start: valuegen::region_start_anchor(
            anchor_region,
            MusicalPosition(RationalTime::from_int(at)),
        ),
        segment,
    })
}

fn effect_for(result: &epiphany_ops::GraphMaterialization, id: OperationId) -> &OperationEffect {
    result
        .state
        .effects
        .iter()
        .find(|(e, _)| *e == id)
        .map(|(_, effect)| effect)
        .expect("every accepted operation produces an effect")
}

fn precondition_noop(reason: PreconditionFailureReason) -> OperationEffect {
    OperationEffect::NoOp {
        reason: NoOpReason::PreconditionFailedUnderReduction { reason },
    }
}

#[test]
fn create_staff_mints_into_the_graph_and_checks_references() {
    let base = epiphany_core::generators::valid_score(300);
    let instrument = base.instruments[0].id;
    let staff_id = StaffId::new(ReplicaId(60), 1);
    let envelopes = chain(
        60,
        10,
        None,
        vec![
            OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(staff_id, instrument),
            }),
            // A staff naming an undeclared instrument is refused (graph-aware
            // reference-resolution precondition).
            OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(
                    StaffId::new(ReplicaId(60), 2),
                    epiphany_core::InstrumentId::new(ReplicaId(60), 99),
                ),
            }),
        ],
    );
    let mut set = OperationSet::new();
    set.accept_all(envelopes.clone());
    let result = set.reduce_onto(&base);

    assert_eq!(
        effect_for(&result, envelopes[0].id),
        &OperationEffect::Applied
    );
    assert_eq!(
        effect_for(&result, envelopes[1].id),
        &precondition_noop(PreconditionFailureReason::TargetMissing),
    );
    assert!(result.score.staves.iter().any(|s| s.id == staff_id));
    assert!(matches!(
        result.state.objects.get(&TypedObjectId::Staff(staff_id)),
        Some(epiphany_ops::ObjectState::Live)
    ));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn create_staff_instance_now_requires_a_live_staff() {
    let base = epiphany_core::generators::valid_score(301);
    let region = base.canvas.regions[0].id;
    let instrument = base.instruments[0].id;
    let minted_staff = StaffId::new(ReplicaId(61), 1);
    let envelopes = chain(
        61,
        10,
        None,
        vec![
            // Referencing a staff that was never minted: refused.
            OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region,
                instance: valuegen::staff_instance(
                    StaffInstanceId::new(ReplicaId(61), 5),
                    StaffId::new(ReplicaId(61), 9),
                ),
            }),
            // Mint the staff, then an instance referencing it applies.
            OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(minted_staff, instrument),
            }),
            OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region,
                instance: valuegen::staff_instance(
                    StaffInstanceId::new(ReplicaId(61), 6),
                    minted_staff,
                ),
            }),
        ],
    );
    let mut set = OperationSet::new();
    set.accept_all(envelopes.clone());
    let result = set.reduce_onto(&base);

    assert_eq!(
        effect_for(&result, envelopes[0].id),
        &precondition_noop(PreconditionFailureReason::TargetMissing),
    );
    assert_eq!(
        effect_for(&result, envelopes[2].id),
        &OperationEffect::Applied
    );
    assert!(result
        .score
        .staff_instances()
        .any(|(_, si)| si.id == StaffInstanceId::new(ReplicaId(61), 6)));
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn set_time_signature_sets_replaces_and_removes_in_the_grid() {
    let base = epiphany_core::generators::valid_score(302);
    let region = base.canvas.regions[0].id;
    let ts_a = valuegen::time_signature(TimeSignatureId::new(ReplicaId(62), 1), 4);
    let ts_b = valuegen::time_signature(TimeSignatureId::new(ReplicaId(62), 2), 3);
    let envelopes = chain(
        62,
        10,
        None,
        vec![
            set_meter(region, 0, Some(ts_a.clone())),
            set_meter(region, 0, Some(ts_b.clone())),
            set_meter(region, 0, None),
        ],
    );

    // Set: the grid is created around the meter change; the signature mints.
    let mut set = OperationSet::new();
    set.accept_all(envelopes[..1].to_vec());
    let result = set.reduce_onto(&base);
    let grid = |result: &epiphany_ops::GraphMaterialization| {
        result.score.canvas.regions[0]
            .content
            .staff_based()
            .expect("fixture is staff based")
            .default_metric_grid
            .clone()
    };
    let after_set = grid(&result).expect("a set creates the grid");
    assert_eq!(after_set.meter_sequence.len(), 1);
    assert_eq!(after_set.meter_sequence[0].time_signature, ts_a.id);
    assert!(result.score.time_signatures.iter().any(|t| t.id == ts_a.id));
    assert!(check_invariants(&result.score).is_empty());

    // Replace: the causally-later write overwrites the single slot.
    let mut set = OperationSet::new();
    set.accept_all(envelopes[..2].to_vec());
    let result = set.reduce_onto(&base);
    let after_replace = grid(&result).expect("still present after replace");
    assert_eq!(after_replace.meter_sequence.len(), 1);
    assert_eq!(after_replace.meter_sequence[0].time_signature, ts_b.id);
    assert!(check_invariants(&result.score).is_empty());

    // Remove: the slot empties; the grid the set created normalizes away.
    let mut set = OperationSet::new();
    set.accept_all(envelopes.to_vec());
    let result = set.reduce_onto(&base);
    assert_eq!(grid(&result), None);
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn a_mid_region_meter_change_reduces_cleanly_p12_c5() {
    // P12-C5: a mid-region SetTimeSignature reduces cleanly and materializes
    // into the grid; the decomposition pre-pass still honors only the first
    // governing meter (P12-H4) but must not crash on the multi-meter grid.
    let base = epiphany_core::generators::valid_score(303);
    let region = base.canvas.regions[0].id;
    let ts_a = valuegen::time_signature(TimeSignatureId::new(ReplicaId(63), 1), 4);
    let ts_b = valuegen::time_signature(TimeSignatureId::new(ReplicaId(63), 2), 3);
    let envelopes = chain(
        63,
        10,
        None,
        vec![
            set_meter(region, 0, Some(ts_a.clone())),
            set_meter(region, 8, Some(ts_b.clone())),
        ],
    );
    let mut set = OperationSet::new();
    set.accept_all(envelopes.clone());
    let result = set.reduce_onto(&base);

    for env in &envelopes {
        assert_eq!(effect_for(&result, env.id), &OperationEffect::Applied);
    }
    let grid = result.score.canvas.regions[0]
        .content
        .staff_based()
        .expect("fixture is staff based")
        .default_metric_grid
        .as_ref()
        .expect("the sets created the grid");
    assert_eq!(
        grid.meter_sequence
            .iter()
            .map(|m| m.time_signature)
            .collect::<Vec<_>>(),
        vec![ts_a.id, ts_b.id],
        "meter changes stay ordered by resolved position"
    );
    assert!(check_invariants(&result.score).is_empty());
    // The pre-pass tolerates the multi-meter grid (P12-H4 owns honoring it).
    let _ =
        epiphany_core::derive_annotations(&result.score, &epiphany_core::PrePassProfile::default())
            .expect("the default pre-pass algorithms are supported");
}

#[test]
fn set_tempo_segment_materializes_in_score_and_region_scope() {
    let base = epiphany_core::generators::valid_score(304);
    let region = base.canvas.regions[0].id;
    let score_seg = valuegen::tempo_segment(region, MusicalPosition::origin(), 120.0);
    let local_seg = valuegen::tempo_segment(region, MusicalPosition::origin(), 90.0);
    let mut ramp =
        valuegen::tempo_segment(region, MusicalPosition(RationalTime::from_int(4)), 60.0);
    ramp.shape = epiphany_core::TempoShape::Linear; // no end data: malformed
    let envelopes = chain(
        64,
        10,
        None,
        vec![
            set_tempo(None, region, 0, Some(score_seg.clone())),
            set_tempo(Some(region), region, 0, Some(local_seg.clone())),
            set_tempo(None, region, 4, Some(ramp)),
            set_tempo(Some(region), region, 0, None),
        ],
    );
    let mut set = OperationSet::new();
    set.accept_all(envelopes.clone());
    let result = set.reduce_onto(&base);

    assert_eq!(
        effect_for(&result, envelopes[0].id),
        &OperationEffect::Applied
    );
    assert_eq!(
        effect_for(&result, envelopes[1].id),
        &OperationEffect::Applied
    );
    assert_eq!(
        effect_for(&result, envelopes[2].id),
        &precondition_noop(PreconditionFailureReason::TempoMapMalformed),
    );
    assert_eq!(
        effect_for(&result, envelopes[3].id),
        &OperationEffect::Applied
    );
    assert_eq!(result.score.tempo_map.segments, vec![score_seg]);
    // The local map was created by the set and normalized away by the remove.
    assert_eq!(result.score.canvas.regions[0].local_tempo_map, None);
    assert!(check_invariants(&result.score).is_empty());

    // The region-scoped set alone creates (and keeps) the local map.
    let mut set = OperationSet::new();
    set.accept_all(envelopes[..2].to_vec());
    let result = set.reduce_onto(&base);
    let local = result.score.canvas.regions[0]
        .local_tempo_map
        .as_ref()
        .expect("a set on a region with no local map creates one");
    assert_eq!(local.segments, vec![local_seg]);
    assert!(check_invariants(&result.score).is_empty());
}

#[test]
fn set_staff_layout_is_an_advisory_lww_with_tombstone_noop() {
    let base = epiphany_core::generators::valid_score(305);
    let region = base.canvas.regions[0].id;
    let instance = base.canvas.regions[0].staff_instances()[0].id;
    let staff = base.canvas.regions[0].staff_instances()[0].staff;
    let instrument = base.instruments[0].id;

    // Two concurrent differing writes: advisory LWW — no conflict; the later
    // in canonical order (greater replica at an equal stamp) wins.
    let earlier = envelope(
        65,
        0,
        10,
        CausalContext::new(),
        None,
        prim(OperationKind::SetStaffLayout(SetStaffLayoutOp {
            staff_instance: instance,
            instrument_override: Some(instrument),
            staff_lines_override: None,
            visible: true,
        })),
    );
    let later = envelope(
        66,
        0,
        10,
        CausalContext::new(),
        None,
        prim(OperationKind::SetStaffLayout(SetStaffLayoutOp {
            staff_instance: instance,
            instrument_override: None,
            staff_lines_override: Some(epiphany_core::StaffLineConfiguration { line_count: 1 }),
            visible: false,
        })),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![earlier.clone(), later.clone()]);
    let result = set.reduce_onto(&base);

    assert!(
        result.state.conflicts.is_empty(),
        "advisory LWW never conflicts"
    );
    assert_eq!(effect_for(&result, earlier.id), &OperationEffect::Applied);
    assert_eq!(effect_for(&result, later.id), &OperationEffect::Applied);
    let materialized = result
        .score
        .staff_instances()
        .find(|(_, si)| si.id == instance)
        .expect("instance survives")
        .1
        .clone();
    assert_eq!(materialized.instrument_override, None);
    assert_eq!(
        materialized.staff_lines_override,
        Some(epiphany_core::StaffLineConfiguration { line_count: 1 })
    );
    assert!(!materialized.visible);
    assert!(check_invariants(&result.score).is_empty());

    // A tombstoned target is a no-op: mint an empty instance, delete it, then
    // aim a layout write at it.
    let fresh = StaffInstanceId::new(ReplicaId(67), 1);
    let envelopes = chain(
        67,
        10,
        None,
        vec![
            OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region,
                instance: valuegen::staff_instance(fresh, staff),
            }),
            OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
                staff_instance: fresh,
            }),
            OperationKind::SetStaffLayout(SetStaffLayoutOp {
                staff_instance: fresh,
                instrument_override: None,
                staff_lines_override: None,
                visible: false,
            }),
        ],
    );
    let mut set = OperationSet::new();
    set.accept_all(envelopes.clone());
    let result = set.reduce_onto(&base);
    assert_eq!(
        effect_for(&result, envelopes[2].id),
        &OperationEffect::NoOp {
            reason: NoOpReason::TargetTombstoned,
        },
    );
}

/// The full value-restoring undo sweep: one transaction overwrites every LWW
/// family (and mints an event), and its undo restores each family to the base
/// state (operation_catalog §UndoTransaction "Value restoration").
#[test]
fn undo_restores_overwritten_values_across_every_family() {
    let base = epiphany_core::generators::valid_score(306);
    let region = base.canvas.regions[0].id;
    let (staff_instance, target_voice) = target(&base);
    let base_instance = base
        .staff_instances()
        .find(|(_, si)| si.id == staff_instance)
        .expect("fixture instance")
        .1
        .clone();
    let base_event_id = base_instance.voices[0].events[0];
    let base_event = base.events.get(base_event_id).expect("base event").clone();
    let mut pitches = Vec::new();
    base_event.collect_identified_pitches(&mut pitches);
    let base_pitch = pitches[0].id;

    // A same-placement replacement value with different pitch content.
    let mut replacement = base_event.clone();
    if let Event::Pitched(pe) = &mut replacement {
        pe.pitches[0].pitch = valuegen::pitch_value_nth(5);
    }
    let ts = valuegen::time_signature(TimeSignatureId::new(ReplicaId(70), 1), 4);
    let segment = valuegen::tempo_segment(region, MusicalPosition::origin(), 120.0);
    let inserted_event = EventId::new(ReplicaId(70), 100);
    let inserted_pitch = PitchId::new(ReplicaId(70), 101);

    let tx = TransactionId::from_raw(70);
    let mut kinds = vec![
        OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("overwrite everything"),
            category: Some(TransactionCategory::Structural),
        }),
        OperationKind::ModifyEvent(ModifyEventOp {
            event: replacement.clone(),
        }),
        OperationKind::RespellPitch(RespellPitchOp {
            pitch: base_pitch,
            spelling: valuegen::spelling(3),
        }),
        OperationKind::SetMetricGrid(SetMetricGridOp {
            region,
            grid: Some(valuegen::metric_grid()),
        }),
        OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
            region,
            anchor: valuegen::region_start_anchor(
                region,
                MusicalPosition(RationalTime::from_int(8)),
            ),
            present: true,
        }),
        set_meter(region, 0, Some(ts.clone())),
        set_tempo(None, region, 0, Some(segment)),
        OperationKind::SetStaffLayout(SetStaffLayoutOp {
            staff_instance,
            instrument_override: None,
            staff_lines_override: None,
            visible: false,
        }),
        OperationKind::SetMetadata(epiphany_ops::SetMetadataOp {
            metadata: valuegen::score_metadata(7),
        }),
    ];
    kinds.push(OperationKind::InsertEvent(InsertEventOp {
        staff_instance,
        event: valuegen::insert_event_value(
            inserted_event,
            target_voice,
            MusicalPosition(RationalTime::from_int(100)),
            MusicalDuration::whole(),
            &[inserted_pitch],
        ),
    }));
    let n = kinds.len() as u64;
    let mut envelopes = chain(70, 10, Some(tx), kinds);
    let undo = envelope(
        70,
        n,
        10 + n as i64,
        seen(70, n - 1),
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    envelopes.push(undo.clone());

    let mut set = OperationSet::new();
    set.accept_all(envelopes);
    let result = set.reduce_onto(&base);

    // Effect: the tombstone repairs only (the inserted event + pitch, and the
    // transaction-minted time signature); restorations are not repairs.
    match effect_for(&result, undo.id) {
        OperationEffect::AppliedWithRepair { repairs } => {
            assert!(repairs
                .iter()
                .all(|r| matches!(r.kind, RepairKind::CascadeDeleted)));
            let targets: Vec<TypedObjectId> = repairs.iter().map(|r| r.target).collect();
            assert!(targets.contains(&TypedObjectId::Event(inserted_event)));
            assert!(targets.contains(&TypedObjectId::TimeSignature(ts.id)));
        }
        other => panic!("expected AppliedWithRepair, got {other:?}"),
    }
    assert!(result.state.conflicts.is_empty());

    // Every overwritten family is back at its base value.
    assert_eq!(
        result.score.events.get(base_event_id),
        Some(&base_event),
        "the modified event's base value is restored"
    );
    assert_eq!(result.state.spellings.get(&base_pitch), None);
    assert!(result.score.spelling_attachments.is_empty());
    let content = result.score.canvas.regions[0]
        .content
        .staff_based()
        .expect("fixture is staff based");
    assert_eq!(content.default_metric_grid, None);
    assert!(content.user_system_breaks.is_empty());
    assert!(result.state.breaks.is_empty());
    assert!(result.score.tempo_map.segments.is_empty());
    assert!(!result.score.time_signatures.iter().any(|t| t.id == ts.id));
    let restored_instance = result
        .score
        .staff_instances()
        .find(|(_, si)| si.id == staff_instance)
        .expect("instance survives")
        .1
        .clone();
    assert_eq!(
        (
            restored_instance.instrument_override,
            restored_instance.staff_lines_override.clone(),
            restored_instance.visible
        ),
        (
            base_instance.instrument_override,
            base_instance.staff_lines_override.clone(),
            base_instance.visible
        )
    );
    assert_eq!(result.score.metadata, base.metadata);
    assert!(!result.score.events.contains(inserted_event));
    assert!(check_invariants(&result.score).is_empty());
}

/// Replaced (rather than first-written) keys restore the *pre-transaction*
/// writes, not absence.
#[test]
fn undo_restores_the_pre_transaction_writes_for_replaced_keys() {
    let base = epiphany_core::generators::valid_score(307);
    let region = base.canvas.regions[0].id;
    let (staff_instance, _) = target(&base);
    let ts_pre = valuegen::time_signature(TimeSignatureId::new(ReplicaId(71), 1), 4);
    let ts_tx = valuegen::time_signature(TimeSignatureId::new(ReplicaId(71), 2), 3);
    let seg_pre = valuegen::tempo_segment(region, MusicalPosition::origin(), 120.0);
    let seg_tx = valuegen::tempo_segment(region, MusicalPosition::origin(), 90.0);
    let tx = TransactionId::from_raw(71);

    let kinds = vec![
        // Pre-transaction writers.
        set_meter(region, 0, Some(ts_pre.clone())),
        set_tempo(None, region, 0, Some(seg_pre.clone())),
        OperationKind::SetStaffLayout(SetStaffLayoutOp {
            staff_instance,
            instrument_override: None,
            staff_lines_override: None,
            visible: false,
        }),
        // The transaction replaces all three keys.
        OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("replace"),
            category: None,
        }),
        set_meter(region, 0, Some(ts_tx.clone())),
        set_tempo(None, region, 0, Some(seg_tx)),
        OperationKind::SetStaffLayout(SetStaffLayoutOp {
            staff_instance,
            instrument_override: None,
            staff_lines_override: None,
            visible: true,
        }),
    ];
    let mut envelopes: Vec<OperationEnvelope> = kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let counter = index as u64;
            let ctx = if counter == 0 {
                CausalContext::new()
            } else {
                seen(71, counter - 1)
            };
            let tx_of = (counter >= 3).then_some(tx);
            envelope(71, counter, 10 + counter as i64, ctx, tx_of, prim(kind))
        })
        .collect();
    let undo = envelope(
        71,
        7,
        20,
        seen(71, 6),
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    envelopes.push(undo.clone());

    let mut set = OperationSet::new();
    set.accept_all(envelopes);
    let result = set.reduce_onto(&base);

    assert!(result.state.conflicts.is_empty());
    let grid = result.score.canvas.regions[0]
        .content
        .staff_based()
        .expect("fixture is staff based")
        .default_metric_grid
        .as_ref()
        .expect("the pre-transaction meter survives");
    assert_eq!(grid.meter_sequence.len(), 1);
    assert_eq!(grid.meter_sequence[0].time_signature, ts_pre.id);
    // The transaction-minted signature is tombstoned and gone; the
    // pre-transaction one survives.
    assert!(!result
        .score
        .time_signatures
        .iter()
        .any(|t| t.id == ts_tx.id));
    assert!(result
        .score
        .time_signatures
        .iter()
        .any(|t| t.id == ts_pre.id));
    assert_eq!(result.score.tempo_map.segments, vec![seg_pre]);
    let instance = result
        .score
        .staff_instances()
        .find(|(_, si)| si.id == staff_instance)
        .expect("instance survives")
        .1
        .clone();
    assert!(
        !instance.visible,
        "the pre-transaction layout write returns"
    );
    assert!(check_invariants(&result.score).is_empty());
}

/// A mixed mint + overwrite transaction: both compensation parts compose, and
/// `StrictInverse` refuses the whole undo when *either* part fails while
/// `BestEffort` compensates what it cleanly can.
#[test]
fn mixed_transaction_undo_composes_and_conflicts_by_policy() {
    let base = epiphany_core::generators::valid_score(308);
    let (staff_instance, target_voice) = target(&base);
    let base_instance = base
        .staff_instances()
        .find(|(_, si)| si.id == staff_instance)
        .expect("fixture instance")
        .1
        .clone();
    let base_pitch = {
        let event = base.events.get(base_instance.voices[0].events[0]).unwrap();
        let mut pitches = Vec::new();
        event.collect_identified_pitches(&mut pitches);
        pitches[0].id
    };
    let inserted_event = EventId::new(ReplicaId(72), 100);
    let tx = TransactionId::from_raw(72);
    let tx_ops = |replica: u64| {
        chain(
            replica,
            10,
            Some(tx),
            vec![
                OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("mixed"),
                    category: None,
                }),
                OperationKind::InsertEvent(InsertEventOp {
                    staff_instance,
                    event: valuegen::insert_event_value(
                        EventId::new(ReplicaId(replica), 100),
                        target_voice,
                        MusicalPosition(RationalTime::from_int(100)),
                        MusicalDuration::whole(),
                        &[PitchId::new(ReplicaId(replica), 101)],
                    ),
                }),
                OperationKind::RespellPitch(RespellPitchOp {
                    pitch: base_pitch,
                    spelling: valuegen::spelling(2),
                }),
            ],
        )
    };

    // (A) A superseding respell after the transaction: StrictInverse refuses
    // the whole undo — the minted event is NOT tombstoned either.
    let mut envelopes = tx_ops(72);
    envelopes.push(envelope(
        72,
        3,
        20,
        seen(72, 2),
        None,
        prim(OperationKind::RespellPitch(RespellPitchOp {
            pitch: base_pitch,
            spelling: valuegen::spelling(4),
        })),
    ));
    let strict = envelope(
        72,
        4,
        30,
        seen(72, 3),
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    envelopes.push(strict.clone());
    let mut set = OperationSet::new();
    set.accept_all(envelopes.clone());
    let result = set.reduce_onto(&base);
    assert!(matches!(
        effect_for(&result, strict.id),
        OperationEffect::Conflicted { .. }
    ));
    assert!(
        result.score.events.contains(inserted_event),
        "a refused strict undo tombstones nothing"
    );
    assert_eq!(
        result.state.spellings.get(&base_pitch),
        Some(&valuegen::spelling(4))
    );
    assert!(check_invariants(&result.score).is_empty());

    // (B) The same set under BestEffort: the mint is tombstoned, the
    // superseded spelling is skipped.
    let best = envelope(
        72,
        5,
        40,
        seen(72, 4),
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::BestEffort,
        }),
    );
    envelopes.push(best.clone());
    let mut set = OperationSet::new();
    set.accept_all(envelopes);
    let result = set.reduce_onto(&base);
    assert!(matches!(
        effect_for(&result, best.id),
        OperationEffect::AppliedWithRepair { .. }
    ));
    assert!(!result.score.events.contains(inserted_event));
    assert_eq!(
        result.state.spellings.get(&base_pitch),
        Some(&valuegen::spelling(4)),
        "the superseding write stands under BestEffort"
    );
    assert!(check_invariants(&result.score).is_empty());
}

/// Undoing a staff mint refuses while a live (non-member) instance still
/// references the staff (operation_catalog §CreateStaff undo semantics), and
/// `BestEffort` keeps the staff alive rather than stranding the instance.
#[test]
fn undo_of_a_staff_mint_refuses_while_an_instance_references_it() {
    let base = epiphany_core::generators::valid_score(309);
    let region = base.canvas.regions[0].id;
    let instrument = base.instruments[0].id;
    let staff_id = StaffId::new(ReplicaId(73), 1);
    let instance_id = StaffInstanceId::new(ReplicaId(73), 2);
    let tx = TransactionId::from_raw(73);
    let mut envelopes = chain(
        73,
        10,
        Some(tx),
        vec![
            OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("staff mint"),
                category: None,
            }),
            OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(staff_id, instrument),
            }),
        ],
    );
    // A non-member instance referencing the minted staff.
    let mut instance_env = envelope(
        73,
        2,
        20,
        seen(73, 1),
        None,
        prim(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region,
            instance: valuegen::staff_instance(instance_id, staff_id),
        })),
    );
    instance_env.transaction = None;
    envelopes.push(instance_env);
    let strict = envelope(
        73,
        3,
        30,
        seen(73, 2),
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );
    let best = envelope(
        73,
        4,
        40,
        seen(73, 3),
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::BestEffort,
        }),
    );
    envelopes.push(strict.clone());
    envelopes.push(best.clone());

    let mut set = OperationSet::new();
    set.accept_all(envelopes);
    let result = set.reduce_onto(&base);

    assert!(matches!(
        effect_for(&result, strict.id),
        OperationEffect::Conflicted { .. }
    ));
    // BestEffort skips the stranding tombstone: nothing left to compensate,
    // but the undo itself applies cleanly with no repairs.
    assert_eq!(effect_for(&result, best.id), &OperationEffect::Applied);
    assert!(result.score.staves.iter().any(|s| s.id == staff_id));
    assert!(matches!(
        result.state.objects.get(&TypedObjectId::Staff(staff_id)),
        Some(epiphany_ops::ObjectState::Live)
    ));
    assert!(check_invariants(&result.score).is_empty());
}
