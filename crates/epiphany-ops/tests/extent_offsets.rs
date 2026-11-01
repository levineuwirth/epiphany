//! A region's time extent answers to its target's time model (reduction
//! version 5, X5b.8): an anchor's offset must be of a kind its target's
//! region admits (graph invariant 9, `AnchorOffsetModel`), so a wall-clock
//! offset from a metric region's edge, or a musical one from a proportional
//! region's, is refused. A `CreateRegion` whose extent anchors a region's
//! edge, an event or a measure so, or whose own model a live region's extent
//! already contradicts, is refused `WrongRegionTimeModel`; a migration strands
//! every live region whose extent anchors its region, or an event or a
//! measure in it, by an offset the target model does not admit, the region's
//! own bound included, as it strands a marker anchored there.
//!
//! Each history is one author's, built by hand: an instrument and a staff,
//! then regions. Each is reduced both ways, the two held to each other, the
//! graph checked, and the verdict the history exists for asserted, beside the
//! legal controls (no offset, or one of the target's own kind) that apply.

use epiphany_core::{
    check_invariants, AleatoricAnchoringDiscipline, AleatoricTimeModel, AnchorOffset,
    EventOrderingDAG, IdentityContext, InstrumentId, Measure, MeasureId, MeasureNumberVisibility,
    MeasurePosition, MusicalDuration, MusicalPosition, OperationId, PitchId, RationalTime,
    RegionEdge, RegionId, RegionTimeModel, ReplicaId, Score, StaffId, StaffInstanceId, TimeAnchor,
    TimeExtent, TransactionId, TypedObjectId, VoiceId, WallClockDuration, WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, ChangeRegionTimeModelOp, ConflictKind, CreateInstrumentOp,
    CreateMeasureOp, CreateRegionOp, CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp,
    DeleteRegionOp, GraphMaterialization, HybridLogicalClock, InsertEventOp, MaterializedState,
    NoOpReason, OperationEffect, OperationEnvelope, OperationKind, OperationPayload, OperationSet,
    OperationStamp, PositionRemapping, PreconditionFailureReason, SetTempoSegmentOp,
    TransactionDescriptor, UndoPolicy, UndoTransactionPayload,
};

const AUTHOR: ReplicaId = ReplicaId(5);

/// One author's history, each envelope seeing every one before it.
struct History {
    envelopes: Vec<OperationEnvelope>,
    next: u64,
    staff: StaffId,
}

impl History {
    /// An instrument and a staff.
    fn new() -> Self {
        let mut history = History {
            envelopes: Vec::new(),
            next: 0,
            staff: StaffId::new(AUTHOR, 901),
        };
        let instrument = InstrumentId::new(AUTHOR, 900);
        history.push(OperationKind::CreateInstrument(CreateInstrumentOp {
            instrument: valuegen::instrument(instrument),
        }));
        history.push(OperationKind::CreateStaff(CreateStaffOp {
            staff: valuegen::staff(history.staff, instrument),
        }));
        history
    }

    fn push(&mut self, kind: OperationKind) -> OperationId {
        self.push_payload(None, OperationPayload::Primitive(kind))
    }

    fn push_payload(
        &mut self,
        transaction: Option<TransactionId>,
        payload: OperationPayload,
    ) -> OperationId {
        let id = OperationId::new(AUTHOR, self.next);
        let causal_context = match self.next {
            0 => CausalContext::new(),
            n => CausalContext::new().with_seen(AUTHOR, n - 1),
        };
        self.next += 1;
        self.envelopes.push(OperationEnvelope {
            id,
            author: AuthorId(5),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(self.next as i64), 0),
                id,
            ),
            causal_context,
            transaction,
            payload,
        });
        id
    }

    /// A region `n` of `model` over `[start, end)`, and its create.
    fn region(
        &mut self,
        n: u64,
        model: RegionTimeModel,
        start: TimeAnchor,
        end: TimeAnchor,
    ) -> (RegionId, OperationId) {
        let id = RegionId::new(AUTHOR, n);
        let mut region = valuegen::region(id);
        region.time_model = model;
        region.time_extent = TimeExtent { start, end };
        let create = self.push(OperationKind::CreateRegion(CreateRegionOp { region }));
        (id, create)
    }

    /// A region `n` of `model` over a wall-clock second.
    fn placed(&mut self, n: u64, model: RegionTimeModel) -> RegionId {
        self.region(n, model, wall(0), wall(1000)).0
    }

    fn migrate(&mut self, region: RegionId, model: RegionTimeModel) -> OperationId {
        self.push(OperationKind::ChangeRegionTimeModel(
            ChangeRegionTimeModelOp {
                region,
                new_time_model: model,
                declared_incompatible: Vec::new(),
                remapping: PositionRemapping::PreserveTime,
            },
        ))
    }

    /// Both reductions, held to each other in every effect and the canonical
    /// bytes; the graph-aware score breaks no invariant.
    fn reduce(&self) -> (MaterializedState, GraphMaterialization) {
        let mut set = OperationSet::new();
        set.accept_all(self.envelopes.iter().cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(0))));
        assert_eq!(free.effects, aware.state.effects, "the modes' effects");
        assert_eq!(
            free.canonical_bytes(),
            aware.state.canonical_bytes(),
            "the modes' canonical bytes"
        );
        let violations = check_invariants(&aware.score);
        assert!(violations.is_empty(), "{violations:#?}");
        (free, aware)
    }
}

fn wall(t: i64) -> TimeAnchor {
    TimeAnchor::WallClock {
        time: WallClockTime(t),
    }
}

fn edge(region: RegionId, edge: RegionEdge, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge,
        offset,
    }
}

fn musical() -> AnchorOffset {
    AnchorOffset::Musical(MusicalDuration(RationalTime::new(1, 4).expect("a quarter")))
}

fn wall_clock() -> AnchorOffset {
    AnchorOffset::WallClock(WallClockDuration(500))
}

fn aleatoric(anchoring: AleatoricAnchoringDiscipline) -> RegionTimeModel {
    RegionTimeModel::Aleatoric(AleatoricTimeModel {
        ordering: EventOrderingDAG::default(),
        anchoring,
        bounds: Default::default(),
        duration_hint: WallClockDuration(1),
    })
}

fn effect(state: &MaterializedState, id: OperationId) -> &OperationEffect {
    &state
        .effects
        .iter()
        .find(|(e, _)| *e == id)
        .expect("reduced")
        .1
}

fn applied(state: &MaterializedState, id: OperationId) -> bool {
    matches!(effect(state, id), OperationEffect::Applied)
}

fn refused(state: &MaterializedState, id: OperationId) -> bool {
    *effect(state, id)
        == OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::WrongRegionTimeModel,
            },
        }
}

/// Whether an operation was refused for naming a target that is not live.
fn missing(state: &MaterializedState, id: OperationId) -> bool {
    *effect(state, id)
        == OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::TargetMissing,
            },
        }
}

/// What a migration's `TimeModelMigrationFailure` names, `None` where it did
/// not conflict so.
fn stranded(state: &MaterializedState, id: OperationId) -> Option<Vec<TypedObjectId>> {
    let OperationEffect::Conflicted { conflict } = effect(state, id) else {
        return None;
    };
    state
        .conflicts
        .records()
        .iter()
        .find(|record| record.id == *conflict)
        .and_then(|record| match &record.kind {
            ConflictKind::TimeModelMigrationFailure {
                incompatible_events,
                ..
            } => Some(incompatible_events.clone()),
            _ => None,
        })
}

/// D81's first route: an extent anchored to a metric region's edge by a
/// wall-clock offset is refused, where version 5 as reviewed applied it and
/// the graph broke invariant 9. No offset and a musical one apply.
#[test]
fn an_extent_at_a_metric_region_s_edge_by_a_wall_clock_offset_is_refused() {
    let mut h = History::new();
    let metric = h.placed(10, valuegen::metric_model());
    let at = |offset| edge(metric, RegionEdge::End, offset);
    let (_, by_wall_clock) = h.region(
        11,
        valuegen::metric_model(),
        at(AnchorOffset::Zero),
        at(wall_clock()),
    );
    let (_, by_nothing) = h.region(
        12,
        valuegen::metric_model(),
        at(AnchorOffset::Zero),
        wall(5000),
    );
    let (_, by_music) = h.region(
        13,
        valuegen::metric_model(),
        at(AnchorOffset::Zero),
        at(musical()),
    );
    let (free, aware) = h.reduce();
    assert!(refused(&free, by_wall_clock));
    assert!(applied(&free, by_nothing));
    assert!(applied(&free, by_music));
    assert_eq!(aware.score.canvas.regions.len(), 3);
}

/// D81's first route, the other way: an extent anchored to a proportional
/// region's edge by a musical offset is refused. No offset and a wall-clock
/// one apply.
#[test]
fn an_extent_at_a_proportional_region_s_edge_by_a_musical_offset_is_refused() {
    let mut h = History::new();
    let proportional = h.placed(10, valuegen::proportional_model());
    let at = |offset| edge(proportional, RegionEdge::End, offset);
    let (_, by_music) = h.region(
        11,
        valuegen::proportional_model(),
        at(AnchorOffset::Zero),
        at(musical()),
    );
    let (_, by_nothing) = h.region(
        12,
        valuegen::proportional_model(),
        at(AnchorOffset::Zero),
        wall(5000),
    );
    let (_, by_wall_clock) = h.region(
        13,
        valuegen::proportional_model(),
        at(AnchorOffset::Zero),
        at(wall_clock()),
    );
    let (free, _) = h.reduce();
    assert!(refused(&free, by_music));
    assert!(applied(&free, by_nothing));
    assert!(applied(&free, by_wall_clock));
}

/// D81's third route: a migration out of musical time of a region another
/// region's extent anchors by a musical offset conflicts, naming that
/// region, as it names a marker anchored there. With no offset the
/// migration applies, and so does one that keeps musical time.
#[test]
fn a_migration_out_of_musical_time_strands_a_region_anchored_to_it_by_a_musical_offset() {
    let stranding = |offset: AnchorOffset, model: RegionTimeModel| {
        let mut h = History::new();
        let target = h.placed(10, valuegen::metric_model());
        let (anchored, _) = h.region(
            11,
            valuegen::metric_model(),
            edge(target, RegionEdge::End, AnchorOffset::Zero),
            edge(target, RegionEdge::End, offset),
        );
        let migration = h.migrate(target, model);
        let (free, aware) = h.reduce();
        (
            stranded(&free, migration),
            anchored,
            applied(&free, migration),
            aware,
        )
    };
    let (named, anchored, _, aware) = stranding(musical(), valuegen::proportional_model());
    assert_eq!(named, Some(vec![TypedObjectId::Region(anchored)]));
    assert!(matches!(
        aware.score.canvas.regions[0].time_model,
        RegionTimeModel::Metric(_)
    ));
    let (_, _, migrated, _) = stranding(AnchorOffset::Zero, valuegen::proportional_model());
    assert!(migrated, "no offset");
    let (_, _, migrated, _) =
        stranding(musical(), aleatoric(AleatoricAnchoringDiscipline::Musical));
    assert!(migrated, "a target in musical time");
}

/// D81's fourth route: a region whose own end is its start plus a musical
/// length strands its own migration out of musical time, named once though a
/// tempo segment anchored to it by a musical offset strands it too. A
/// region bounded by its own start with no offset, or one migrated within
/// musical time, migrates; and a region cannot be created bounded by its own
/// start through an offset its own model does not admit.
#[test]
fn a_migration_out_of_musical_time_strands_a_region_bounded_by_its_own_start() {
    let bounded = |offset: AnchorOffset, model: RegionTimeModel, tempo: bool| {
        let mut h = History::new();
        let id = RegionId::new(AUTHOR, 10);
        let (_, create) = h.region(
            10,
            valuegen::metric_model(),
            wall(0),
            edge(id, RegionEdge::Start, offset),
        );
        if tempo {
            let segment = valuegen::tempo_segment(id, MusicalPosition::origin(), 90.0);
            h.push(OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: None,
                start: segment.start.clone(),
                segment: Some(segment),
            }));
        }
        let migration = h.migrate(id, model);
        let (free, _) = h.reduce();
        assert!(applied(&free, create));
        (stranded(&free, migration), id, applied(&free, migration))
    };
    let (named, id, _) = bounded(musical(), valuegen::proportional_model(), false);
    assert_eq!(named, Some(vec![TypedObjectId::Region(id)]));
    let (named, id, _) = bounded(musical(), valuegen::proportional_model(), true);
    assert_eq!(named, Some(vec![TypedObjectId::Region(id)]), "named once");
    let (_, _, migrated) = bounded(AnchorOffset::Zero, valuegen::proportional_model(), false);
    assert!(migrated, "no offset");
    let (_, _, migrated) = bounded(
        musical(),
        aleatoric(AleatoricAnchoringDiscipline::FreelyMixed),
        false,
    );
    assert!(migrated, "a target in musical time");

    let mut h = History::new();
    let id = RegionId::new(AUTHOR, 10);
    let (_, create) = h.region(
        10,
        valuegen::metric_model(),
        wall(0),
        edge(id, RegionEdge::Start, wall_clock()),
    );
    let (free, _) = h.reduce();
    assert!(
        refused(&free, create),
        "its own start, by a wall-clock offset"
    );
}

/// The third route's mirror: a migration into a model admitting no
/// wall-clock offset (metric) of a proportional region another region's
/// extent anchors by a wall-clock offset conflicts, naming that region. With
/// no offset it applies, and so does a target that admits wall-clock time.
#[test]
fn a_migration_into_musical_time_strands_a_region_anchored_to_it_by_a_wall_clock_offset() {
    let stranding = |offset: AnchorOffset, model: RegionTimeModel| {
        let mut h = History::new();
        let target = h.placed(10, valuegen::proportional_model());
        let (anchored, _) = h.region(
            11,
            valuegen::proportional_model(),
            edge(target, RegionEdge::End, AnchorOffset::Zero),
            edge(target, RegionEdge::End, offset),
        );
        let migration = h.migrate(target, model);
        let (free, _) = h.reduce();
        (
            stranded(&free, migration),
            anchored,
            applied(&free, migration),
        )
    };
    let (named, anchored, _) = stranding(wall_clock(), valuegen::metric_model());
    assert_eq!(named, Some(vec![TypedObjectId::Region(anchored)]));
    let (_, _, migrated) = stranding(AnchorOffset::Zero, valuegen::metric_model());
    assert!(migrated, "no offset");
    let (_, _, migrated) = stranding(
        wall_clock(),
        aleatoric(AleatoricAnchoringDiscipline::WallClock),
    );
    assert!(migrated, "a target in wall-clock time");
}

/// The fourth route's mirror: a proportional region whose own end is its
/// start plus a wall-clock length strands its own migration to metric time.
/// A target that admits wall-clock time migrates.
#[test]
fn a_migration_into_musical_time_strands_a_region_bounded_by_its_own_start() {
    let bounded = |model: RegionTimeModel| {
        let mut h = History::new();
        let id = RegionId::new(AUTHOR, 10);
        h.region(
            10,
            valuegen::proportional_model(),
            wall(0),
            edge(id, RegionEdge::Start, wall_clock()),
        );
        let migration = h.migrate(id, model);
        let (free, _) = h.reduce();
        (stranded(&free, migration), id, applied(&free, migration))
    };
    let (named, id, _) = bounded(valuegen::metric_model());
    assert_eq!(named, Some(vec![TypedObjectId::Region(id)]));
    let (_, _, migrated) = bounded(aleatoric(AleatoricAnchoringDiscipline::FreelyMixed));
    assert!(migrated, "a target in wall-clock time");
}

/// An extent anchored to an event or a measure answers to its region's time
/// model: a wall-clock offset from either in a metric region is refused, and
/// no offset or a musical one applies. In an aleatoric region where each
/// event fixes its own clock, an event's offset answers to the event, in
/// musical time, though the region's edge admits a wall-clock offset.
#[test]
fn an_extent_at_an_event_or_a_measure_answers_to_its_region_s_time_model() {
    let mut h = History::new();
    let region = h.placed(10, valuegen::metric_model());
    let instance = StaffInstanceId::new(AUTHOR, 20);
    h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
        region,
        instance: valuegen::staff_instance(instance, h.staff),
    }));
    let measure = MeasureId::new(AUTHOR, 30);
    h.push(OperationKind::CreateMeasure(CreateMeasureOp {
        instance,
        measure: Measure {
            id: measure,
            start: valuegen::region_start_anchor(region, MusicalPosition::origin()),
            time_signature: None,
            explicit_number: None,
            number_visibility: MeasureNumberVisibility::Auto,
        },
    }));
    let voice = VoiceId::new(AUTHOR, 40);
    h.push(OperationKind::CreateVoice(CreateVoiceOp {
        staff_instance: instance,
        voice: valuegen::voice(voice),
    }));
    let note = epiphany_core::EventId::new(AUTHOR, 50);
    h.push(OperationKind::InsertEvent(InsertEventOp {
        staff_instance: instance,
        event: valuegen::insert_event_value(
            note,
            voice,
            MusicalPosition::origin(),
            MusicalDuration(RationalTime::new(1, 4).expect("a quarter")),
            &[PitchId::new(AUTHOR, 51)],
        ),
    }));
    let at_event = |offset| TimeAnchor::Event { id: note, offset };
    let at_measure = |offset| TimeAnchor::Measure {
        id: measure,
        position: MeasurePosition::Start,
        offset,
    };
    let mut n = 100;
    let mut extent = |h: &mut History, start: TimeAnchor| {
        n += 1;
        h.region(n, valuegen::proportional_model(), start, wall(9000))
            .1
    };
    let refused_in_metric = [
        extent(&mut h, at_event(wall_clock())),
        extent(&mut h, at_measure(wall_clock())),
    ];
    let applied_in_metric = [
        extent(&mut h, at_event(AnchorOffset::Zero)),
        extent(&mut h, at_event(musical())),
        extent(&mut h, at_measure(AnchorOffset::Zero)),
        extent(&mut h, at_measure(musical())),
    ];
    let migration = h.migrate(
        region,
        aleatoric(AleatoricAnchoringDiscipline::EitherPerEvent),
    );
    let refused_per_event = extent(&mut h, at_event(wall_clock()));
    let applied_per_event = [
        extent(&mut h, at_event(musical())),
        extent(&mut h, edge(region, RegionEdge::Start, wall_clock())),
    ];
    let (free, _) = h.reduce();
    assert!(applied(&free, migration));
    assert!(refused_in_metric.iter().all(|id| refused(&free, *id)));
    assert!(applied_in_metric.iter().all(|id| applied(&free, *id)));
    assert!(refused(&free, refused_per_event));
    assert!(applied_per_event.iter().all(|id| applied(&free, *id)));
}

/// A region's extent naming a region before it is made is refused
/// `TargetMissing`, by an offset of any kind, and the named region is then
/// made under any model (D87 Q2: before it the namer applied naming nothing,
/// and X5b.8 refused the named region a model its namer's offset
/// contradicted).
#[test]
fn a_region_s_extent_naming_a_region_not_yet_made_is_refused() {
    for offset in [musical(), AnchorOffset::Zero] {
        for model in [valuegen::proportional_model(), valuegen::metric_model()] {
            let mut h = History::new();
            let named = RegionId::new(AUTHOR, 10);
            let (_, namer) = h.region(
                11,
                valuegen::metric_model(),
                edge(named, RegionEdge::Start, AnchorOffset::Zero),
                edge(named, RegionEdge::Start, offset.clone()),
            );
            let (_, create) = h.region(10, model.clone(), wall(0), wall(1000));
            let (free, _) = h.reduce();
            assert!(
                missing(&free, namer),
                "{offset:?}: {:?}",
                effect(&free, namer)
            );
            assert!(applied(&free, create), "{offset:?} {model:?}");
        }
    }
}

/// Only a live region's extent strands a migration: a region deleted since
/// keeps its extent in the ledger's index but holds nothing.
#[test]
fn a_removed_region_s_extent_strands_nothing() {
    let mut h = History::new();
    let target = h.placed(10, valuegen::metric_model());
    let (anchored, _) = h.region(
        11,
        valuegen::metric_model(),
        edge(target, RegionEdge::End, AnchorOffset::Zero),
        edge(target, RegionEdge::End, musical()),
    );
    let delete = h.push(OperationKind::DeleteRegion(DeleteRegionOp {
        region: anchored,
    }));
    let migration = h.migrate(target, valuegen::proportional_model());
    let (free, _) = h.reduce();
    assert!(applied(&free, delete));
    assert!(applied(&free, migration));
}

/// A removed target is not judged for an extent anchored to it, and since
/// D87 Q2 such an extent is not written: a region an undo removed, whose
/// discipline the ledger's index still holds, or a measure an undo removed,
/// whose value it still holds, contradicts nothing, and an extent anchored to
/// either is refused `TargetMissing` (before it the extent applied naming
/// nothing).
#[test]
fn a_removed_target_judges_no_extent_anchored_to_it() {
    let mut h = History::new();
    let undone = |h: &mut History, n: u64, kind: OperationKind| {
        let tx = TransactionId::new(AUTHOR, n);
        h.push_payload(
            Some(tx),
            OperationPayload::Primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("a passage"),
                category: None,
            })),
        );
        h.push_payload(Some(tx), OperationPayload::Primitive(kind));
        h.push_payload(
            None,
            OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: UndoPolicy::StrictInverse,
            }),
        )
    };
    let removed = RegionId::new(AUTHOR, 10);
    let mut region = valuegen::region(removed);
    region.time_model = valuegen::proportional_model();
    let region_undo = undone(
        &mut h,
        30,
        OperationKind::CreateRegion(CreateRegionOp { region }),
    );
    let metric = h.placed(12, valuegen::metric_model());
    let instance = StaffInstanceId::new(AUTHOR, 20);
    h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
        region: metric,
        instance: valuegen::staff_instance(instance, h.staff),
    }));
    let measure = MeasureId::new(AUTHOR, 40);
    let measure_undo = undone(
        &mut h,
        31,
        OperationKind::CreateMeasure(CreateMeasureOp {
            instance,
            measure: Measure {
                id: measure,
                start: valuegen::region_start_anchor(metric, MusicalPosition::origin()),
                time_signature: None,
                explicit_number: None,
                number_visibility: MeasureNumberVisibility::Auto,
            },
        }),
    );
    let (_, at_region) = h.region(
        11,
        valuegen::metric_model(),
        edge(removed, RegionEdge::End, AnchorOffset::Zero),
        edge(removed, RegionEdge::End, musical()),
    );
    let (_, at_measure) = h.region(
        13,
        valuegen::proportional_model(),
        TimeAnchor::Measure {
            id: measure,
            position: MeasurePosition::Start,
            offset: wall_clock(),
        },
        wall(9000),
    );
    let (free, aware) = h.reduce();
    for undo in [region_undo, measure_undo] {
        assert!(!matches!(effect(&free, undo), OperationEffect::NoOp { .. }));
    }
    assert!(missing(&free, at_region), "{:?}", effect(&free, at_region));
    assert!(
        missing(&free, at_measure),
        "{:?}",
        effect(&free, at_measure)
    );
    assert_eq!(aware.score.canvas.regions.len(), 1);
}

/// A migration strands a region whose extent anchors an event or a measure of
/// the migrated region by an offset the target does not admit, as the
/// invariant reads the target once migrated: a region whose model admits
/// either clock holds a note and a measure, and extents anchored to each by a
/// wall-clock offset apply there, then a migration into musical time
/// conflicts, naming them (history `188`, which D50's bar found). An event in
/// a region where each event fixes its clock answers to its own, musical, so
/// a migration there strands an extent at the note, not one at the measure;
/// an extent at the note by a musical offset strands nothing.
#[test]
fn a_migration_strands_a_region_anchored_to_its_event_or_measure() {
    let stranding = |anchor: fn(epiphany_core::EventId, MeasureId) -> TimeAnchor,
                     model: RegionTimeModel| {
        let mut h = History::new();
        let region = h.placed(10, valuegen::metric_model());
        let instance = StaffInstanceId::new(AUTHOR, 20);
        h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region,
            instance: valuegen::staff_instance(instance, h.staff),
        }));
        let measure = MeasureId::new(AUTHOR, 30);
        h.push(OperationKind::CreateMeasure(CreateMeasureOp {
            instance,
            measure: Measure {
                id: measure,
                start: valuegen::region_start_anchor(region, MusicalPosition::origin()),
                time_signature: None,
                explicit_number: None,
                number_visibility: MeasureNumberVisibility::Auto,
            },
        }));
        let voice = VoiceId::new(AUTHOR, 40);
        h.push(OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: instance,
            voice: valuegen::voice(voice),
        }));
        let note = epiphany_core::EventId::new(AUTHOR, 50);
        h.push(OperationKind::InsertEvent(InsertEventOp {
            staff_instance: instance,
            event: valuegen::insert_event_value(
                note,
                voice,
                MusicalPosition::origin(),
                MusicalDuration(RationalTime::new(1, 4).expect("a quarter")),
                &[PitchId::new(AUTHOR, 51)],
            ),
        }));
        let mixed = h.migrate(region, aleatoric(AleatoricAnchoringDiscipline::FreelyMixed));
        let (anchored, create) = h.region(
            11,
            valuegen::proportional_model(),
            anchor(note, measure),
            wall(9000),
        );
        let migration = h.migrate(region, model);
        let (free, _) = h.reduce();
        assert!(applied(&free, mixed));
        assert!(applied(&free, create));
        (
            stranded(&free, migration),
            anchored,
            applied(&free, migration),
        )
    };
    fn note_by_wall_clock(note: epiphany_core::EventId, _: MeasureId) -> TimeAnchor {
        TimeAnchor::Event {
            id: note,
            offset: wall_clock(),
        }
    }
    fn note_by_music(note: epiphany_core::EventId, _: MeasureId) -> TimeAnchor {
        TimeAnchor::Event {
            id: note,
            offset: musical(),
        }
    }
    fn measure_by_wall_clock(_: epiphany_core::EventId, measure: MeasureId) -> TimeAnchor {
        TimeAnchor::Measure {
            id: measure,
            position: MeasurePosition::Start,
            offset: wall_clock(),
        }
    }
    for (anchor, model, strands) in [
        (
            note_by_wall_clock as fn(_, _) -> _,
            valuegen::metric_model(),
            true,
        ),
        (measure_by_wall_clock, valuegen::metric_model(), true),
        (
            note_by_wall_clock,
            aleatoric(AleatoricAnchoringDiscipline::Musical),
            true,
        ),
        (
            note_by_wall_clock,
            aleatoric(AleatoricAnchoringDiscipline::EitherPerEvent),
            true,
        ),
        (
            measure_by_wall_clock,
            aleatoric(AleatoricAnchoringDiscipline::EitherPerEvent),
            false,
        ),
        (note_by_music, valuegen::metric_model(), false),
    ] {
        let (named, anchored, migrated) = stranding(anchor, model);
        if strands {
            assert_eq!(named, Some(vec![TypedObjectId::Region(anchored)]));
        } else {
            assert!(migrated, "{named:?}");
        }
    }
}
