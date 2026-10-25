//! Every holder of a time anchor answers to its target's time model, as a
//! region's extent does since X5b.8 (reduction version 5, X5b.9, D84): an
//! anchor's offset must be of a kind its target's region admits (graph
//! invariant 9, `AnchorOffsetModel`, read as `CoordinateDiscipline::
//! admits_offset`). A marker, a spanner, a user break, a measure, a meter
//! change, a tempo segment, a metric grid's meter change, a repeat structure,
//! a staff instance's carried clef, key and grid, and a region's own model,
//! grid, breaks and tempo map are refused `WrongRegionTimeModel` where
//! written, modified or carried so; an undo's restoration so is superseded by
//! the migration that gave the target its model; and a migration strands
//! every live holder its target model would contradict, out of musical time
//! and into it alike. An event, a measure or a region made where a live
//! holder already names it by an offset its model does not admit is refused
//! (D84 Q1).
//!
//! Each history is one author's, built by hand on a small score: a metric
//! region holding a staff instance, a measure, a voice and a note, and a
//! proportional region holding an instance of another staff, far apart in
//! time. Each is reduced both ways, the two held to each other in every
//! effect and the canonical bytes, the graph checked, and the verdict the
//! history exists for asserted, beside legal controls (no offset, or one of
//! the target's own kind, or a target model that admits it) that apply.

use epiphany_core::{
    check_invariants, AleatoricAnchoringDiscipline, AleatoricTimeModel, AnchorOffset, Clef,
    ClefChange, EventId, EventOrderingDAG, Fermata, FermataShape, IdentityContext, InstrumentId,
    KeySignature, KeySignatureChange, Marker, MarkerId, MarkerKind, Measure, MeasureId,
    MeasureNumberVisibility, MeasurePosition, MeterChange, MetricGrid, MetricTimeModel,
    MusicalDuration, MusicalPosition, OperationId, PitchId, RationalTime, RegionEdge, RegionId,
    RegionTimeModel, RepeatKind, RepeatStructure, RepeatStructureId, ReplicaId, Score, SpanStyle,
    Spanner, SpannerId, SpannerKind, StaffId, StaffInstanceId, TempoMap, TimeAnchor, TimeExtent,
    TimeSignatureId, TransactionId, TypedObjectId, VoiceId, WallClockDuration, WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, ChangeRegionTimeModelOp, ConflictKind, CreateCrossCuttingOp,
    CreateInstrumentOp, CreateMeasureOp, CreateRegionOp, CreateRepeatStructureOp,
    CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp, CrossCuttingValue, GraphMaterialization,
    HybridLogicalClock, InsertEventOp, MaterializedState, ModifyCrossCuttingOp, NoOpReason,
    OperationEffect, OperationEnvelope, OperationKind, OperationPayload, OperationSet,
    OperationStamp, PositionRemapping, PreconditionFailureReason, SetMetricGridOp,
    SetTempoSegmentOp, SetTimeSignatureOp, SetUserPageBreakOp, SetUserSystemBreakOp,
    TransactionDescriptor, UndoPolicy, UndoTransactionPayload,
};

const AUTHOR: ReplicaId = ReplicaId(5);
const METRIC: RegionId = RegionId::new(AUTHOR, 910);
const PROPORTIONAL: RegionId = RegionId::new(AUTHOR, 912);
const INSTANCE: StaffInstanceId = StaffInstanceId::new(AUTHOR, 920);
const PROPORTIONAL_INSTANCE: StaffInstanceId = StaffInstanceId::new(AUTHOR, 922);
const MEASURE: MeasureId = MeasureId::new(AUTHOR, 930);
const VOICE: VoiceId = VoiceId::new(AUTHOR, 940);
const NOTE: EventId = EventId::new(AUTHOR, 950);
const SIGNATURE: TimeSignatureId = TimeSignatureId::new(AUTHOR, 970);
const STAVES: [StaffId; 3] = [
    StaffId::new(AUTHOR, 901),
    StaffId::new(AUTHOR, 902),
    StaffId::new(AUTHOR, 903),
];

/// One author's history, each envelope seeing every one before it.
struct History {
    envelopes: Vec<OperationEnvelope>,
    next: u64,
}

impl History {
    /// An instrument and three staves; a metric region holding an instance
    /// of the first, its measure, a voice and a quarter note at its start; a
    /// proportional region holding an instance of the second, far away.
    fn new() -> Self {
        let mut h = History {
            envelopes: Vec::new(),
            next: 0,
        };
        let instrument = InstrumentId::new(AUTHOR, 900);
        h.push(OperationKind::CreateInstrument(CreateInstrumentOp {
            instrument: valuegen::instrument(instrument),
        }));
        for staff in STAVES {
            h.push(OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(staff, instrument),
            }));
        }
        h.region(
            METRIC.counter(),
            valuegen::metric_model(),
            wall(1_000_000_000),
            wall(1_000_001_000),
        );
        h.region(
            PROPORTIONAL.counter(),
            valuegen::proportional_model(),
            wall(2_000_000_000),
            wall(2_000_001_000),
        );
        h.instance(METRIC, INSTANCE, STAVES[0]);
        h.instance(PROPORTIONAL, PROPORTIONAL_INSTANCE, STAVES[1]);
        h.measure(INSTANCE, MEASURE, edge(METRIC, AnchorOffset::Zero));
        h.push(OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: INSTANCE,
            voice: valuegen::voice(VOICE),
        }));
        h.note(NOTE.counter(), 0);
        h
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

    /// A region `n` of `model` over `[start, end)`, empty.
    fn region(
        &mut self,
        n: u64,
        model: RegionTimeModel,
        start: TimeAnchor,
        end: TimeAnchor,
    ) -> OperationId {
        let mut region = valuegen::region(RegionId::new(AUTHOR, n));
        region.time_model = model;
        region.time_extent = TimeExtent { start, end };
        self.push(OperationKind::CreateRegion(CreateRegionOp { region }))
    }

    /// A region `n` of `model` far from the fixture's, as `shape` leaves its
    /// payload.
    fn shaped_region(
        &mut self,
        n: u64,
        model: RegionTimeModel,
        shape: impl FnOnce(&mut epiphany_core::Region),
    ) -> OperationId {
        let mut region = valuegen::region(RegionId::new(AUTHOR, n));
        region.time_model = model;
        region.time_extent = TimeExtent {
            start: wall(n as i64 * 10_000_000),
            end: wall(n as i64 * 10_000_000 + 1000),
        };
        shape(&mut region);
        self.push(OperationKind::CreateRegion(CreateRegionOp { region }))
    }

    fn instance(&mut self, region: RegionId, id: StaffInstanceId, staff: StaffId) -> OperationId {
        self.shaped_instance(region, id, staff, |_| {})
    }

    fn shaped_instance(
        &mut self,
        region: RegionId,
        id: StaffInstanceId,
        staff: StaffId,
        shape: impl FnOnce(&mut epiphany_core::StaffInstance),
    ) -> OperationId {
        let mut instance = valuegen::staff_instance(id, staff);
        shape(&mut instance);
        self.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region,
            instance,
        }))
    }

    fn measure(
        &mut self,
        instance: StaffInstanceId,
        id: MeasureId,
        start: TimeAnchor,
    ) -> OperationId {
        self.push(OperationKind::CreateMeasure(CreateMeasureOp {
            instance,
            measure: Measure {
                id,
                start,
                time_signature: None,
                explicit_number: None,
                number_visibility: MeasureNumberVisibility::Auto,
            },
        }))
    }

    /// A quarter note `n` in the fixture's voice at quarter `at`.
    fn note(&mut self, n: u64, at: i64) -> OperationId {
        self.push(OperationKind::InsertEvent(InsertEventOp {
            staff_instance: INSTANCE,
            event: valuegen::insert_event_value(
                EventId::new(AUTHOR, n),
                VOICE,
                MusicalPosition(RationalTime::new(at, 4).expect("a quarter")),
                quarter(),
                &[PitchId::new(AUTHOR, n + 1)],
            ),
        }))
    }

    fn marker(&mut self, n: u64, anchor: TimeAnchor) -> OperationId {
        self.push(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: marker(n, anchor),
        }))
    }

    fn modify(
        &mut self,
        transaction: Option<TransactionId>,
        structure: CrossCuttingValue,
    ) -> OperationId {
        self.push_payload(
            transaction,
            OperationPayload::Primitive(OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                structure,
            })),
        )
    }

    fn spanner(
        &mut self,
        n: u64,
        start: TimeAnchor,
        end: TimeAnchor,
        staff: StaffId,
    ) -> OperationId {
        self.push(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: spanner(n, start, end, staff),
        }))
    }

    fn system_break(
        &mut self,
        transaction: Option<TransactionId>,
        region: RegionId,
        anchor: TimeAnchor,
        present: bool,
    ) -> OperationId {
        self.push_payload(
            transaction,
            OperationPayload::Primitive(OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
                region,
                anchor,
                present,
            })),
        )
    }

    /// The fixture's signature set in `region` at `anchor`.
    fn time_signature(&mut self, region: RegionId, anchor: TimeAnchor) -> OperationId {
        self.push(OperationKind::SetTimeSignature(SetTimeSignatureOp {
            region,
            anchor,
            time_signature: Some(valuegen::time_signature(SIGNATURE, 4)),
        }))
    }

    /// The metric region's grid, one meter change of the fixture's
    /// signature at `anchor`, the signature declared first at its start.
    fn grid(&mut self, anchor: TimeAnchor) -> OperationId {
        self.time_signature(METRIC, edge(METRIC, AnchorOffset::Zero));
        self.push(OperationKind::SetMetricGrid(SetMetricGridOp {
            region: METRIC,
            grid: Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor,
                    time_signature: SIGNATURE,
                }],
            }),
        }))
    }

    /// A tempo segment in `scope`'s map (the score's for `None`) starting at
    /// `start`, set or (with `present` false) removed.
    fn tempo(
        &mut self,
        transaction: Option<TransactionId>,
        scope: Option<RegionId>,
        start: TimeAnchor,
        present: bool,
    ) -> OperationId {
        self.push_payload(
            transaction,
            OperationPayload::Primitive(OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: scope,
                start: start.clone(),
                segment: present.then(|| segment_at(start)),
            })),
        )
    }

    fn repeat(&mut self, n: u64, start: TimeAnchor, end: TimeAnchor) -> OperationId {
        self.push(OperationKind::CreateRepeatStructure(
            CreateRepeatStructureOp {
                repeat: RepeatStructure {
                    id: RepeatStructureId::new(AUTHOR, n),
                    start,
                    end,
                    kind: RepeatKind::migration_default(),
                    voltas: Vec::new(),
                },
            },
        ))
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

    /// A transaction `n`, declared.
    fn transaction(&mut self, n: u64) -> TransactionId {
        let id = TransactionId::new(AUTHOR, n);
        self.push_payload(
            Some(id),
            OperationPayload::Primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
                id,
                label: String::from("an edit"),
                category: None,
            })),
        );
        id
    }

    fn undo(&mut self, target: TransactionId, policy: UndoPolicy) -> OperationId {
        self.push_payload(
            None,
            OperationPayload::UndoTransaction(UndoTransactionPayload { target, policy }),
        )
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

/// `region`'s start plus `offset`.
fn edge(region: RegionId, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge: RegionEdge::Start,
        offset,
    }
}

fn at_note(id: EventId, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Event { id, offset }
}

fn at_measure(id: MeasureId, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Measure {
        id,
        position: MeasurePosition::Start,
        offset,
    }
}

fn quarter() -> MusicalDuration {
    MusicalDuration(RationalTime::new(1, 4).expect("a quarter"))
}

fn musical() -> AnchorOffset {
    AnchorOffset::Musical(quarter())
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

fn either() -> RegionTimeModel {
    aleatoric(AleatoricAnchoringDiscipline::FreelyMixed)
}

fn marker(n: u64, anchor: TimeAnchor) -> CrossCuttingValue {
    CrossCuttingValue::Marker(Marker {
        id: MarkerId::new(AUTHOR, n),
        anchor,
        kind: MarkerKind::Fermata(Fermata {
            shape: FermataShape::Normal,
            inverted: false,
        }),
    })
}

fn spanner(n: u64, start: TimeAnchor, end: TimeAnchor, staff: StaffId) -> CrossCuttingValue {
    CrossCuttingValue::Spanner(Spanner {
        id: SpannerId::new(AUTHOR, n),
        start,
        end,
        staves: vec![staff],
        kind: SpannerKind::Generic,
        style: SpanStyle::default(),
    })
}

fn segment_at(start: TimeAnchor) -> epiphany_core::TempoSegment {
    let mut segment = valuegen::tempo_segment(METRIC, MusicalPosition::origin(), 90.0);
    segment.start = start;
    segment
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

/// Whether an undo applied, repairing or not.
fn undone(state: &MaterializedState, id: OperationId) -> bool {
    matches!(
        effect(state, id),
        OperationEffect::Applied | OperationEffect::AppliedWithRepair { .. }
    )
}

fn refused(state: &MaterializedState, id: OperationId) -> bool {
    *effect(state, id)
        == OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::WrongRegionTimeModel,
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

/// Whether a strict undo conflicts, the conflict caused by `superseder`.
fn superseded_by(state: &MaterializedState, undo: OperationId, superseder: OperationId) -> bool {
    let OperationEffect::Conflicted { conflict } = effect(state, undo) else {
        return false;
    };
    state
        .conflicts
        .records()
        .iter()
        .find(|record| record.id == *conflict)
        .is_some_and(|record| {
            matches!(record.kind, ConflictKind::TransactionConflict { .. })
                && record.caused_by.contains(&superseder)
        })
}

/// A write of some holder at an anchor.
type Carrier = dyn Fn(&mut History, TimeAnchor);

/// One holder written once by an offset of each kind, `write` building it
/// from an anchor `anchor_by` gives: the wall-clock one refused where the
/// target admits none, no offset and the admitted kind applied.
fn written(
    write: impl Fn(&mut History, TimeAnchor) -> OperationId,
    anchor_by: impl Fn(AnchorOffset) -> TimeAnchor,
    refused_kind: AnchorOffset,
    admitted_kind: AnchorOffset,
) {
    let mut h = History::new();
    let by_refused = write(&mut h, anchor_by(refused_kind));
    let (free, _) = h.reduce();
    assert!(
        refused(&free, by_refused),
        "{:?}",
        effect(&free, by_refused)
    );
    for offset in [AnchorOffset::Zero, admitted_kind] {
        let mut h = History::new();
        let control = write(&mut h, anchor_by(offset.clone()));
        let (free, _) = h.reduce();
        assert!(
            applied(&free, control),
            "{offset:?}: {:?}",
            effect(&free, control)
        );
    }
}

// --- D84 Q2's routes, a marker's ----------------------------------------

/// A marker at a metric region's start by a wall-clock offset is refused,
/// where version 5 at C5b's addendum applied it and broke invariant 9. No
/// offset and a musical one apply.
#[test]
fn a_marker_at_a_metric_region_s_start_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.marker(40, anchor),
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

/// A marker at a metric region's note by a wall-clock offset is refused.
#[test]
fn a_marker_at_a_note_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.marker(40, anchor),
        |offset| at_note(NOTE, offset),
        wall_clock(),
        musical(),
    );
}

/// A marker at a metric region's measure by a wall-clock offset is refused.
#[test]
fn a_marker_at_a_measure_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.marker(40, anchor),
        |offset| at_measure(MEASURE, offset),
        wall_clock(),
        musical(),
    );
}

/// A marker modified onto a metric region's start by a wall-clock offset is
/// refused, the marker staying where it stood; a modify onto a musical
/// offset applies.
#[test]
fn a_marker_modified_onto_a_wall_clock_offset_in_a_metric_region_is_refused() {
    written(
        |h, anchor| {
            h.marker(40, edge(METRIC, AnchorOffset::Zero));
            h.modify(None, marker(40, anchor))
        },
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

/// An undo restoring a marker to a proportional region's start by a
/// wall-clock offset, the region migrated to metric since, is superseded by
/// that migration (Q17's mirror): a best-effort undo leaves the marker at
/// the note it was moved to, and a strict one conflicts, caused by the
/// migration. A migration to a model admitting wall-clock time leaves the
/// restoration standing.
#[test]
fn an_undo_restoring_a_marker_by_a_wall_clock_offset_into_a_region_migrated_to_metric_is_superseded(
) {
    let undo_of = |model: RegionTimeModel, policy: UndoPolicy| {
        let mut h = History::new();
        h.marker(40, edge(PROPORTIONAL, wall_clock()));
        let tx = h.transaction(30);
        h.modify(Some(tx), marker(40, at_note(NOTE, AnchorOffset::Zero)));
        let migration = h.migrate(PROPORTIONAL, model);
        let undo = h.undo(tx, policy);
        let (free, aware) = h.reduce();
        assert!(applied(&free, migration));
        let anchor = aware.score.cross_cutting.markers[0].anchor.clone();
        (free, undo, migration, anchor)
    };
    let (free, undo, _, anchor) = undo_of(valuegen::metric_model(), UndoPolicy::BestEffort);
    assert!(undone(&free, undo), "{:?}", effect(&free, undo));
    assert_eq!(anchor, at_note(NOTE, AnchorOffset::Zero));
    let (free, undo, migration, _) = undo_of(valuegen::metric_model(), UndoPolicy::StrictInverse);
    assert!(
        superseded_by(&free, undo, migration),
        "{:?}",
        effect(&free, undo)
    );
    let (free, undo, _, anchor) = undo_of(either(), UndoPolicy::StrictInverse);
    assert!(undone(&free, undo), "{:?}", effect(&free, undo));
    assert_eq!(anchor, edge(PROPORTIONAL, wall_clock()), "restored");
}

/// A marker at a proportional region's start by a wall-clock offset strands
/// the region's migration to metric, named; with no offset, or to a model
/// admitting wall-clock time, the migration applies.
#[test]
fn a_migration_into_musical_time_strands_a_marker_anchored_by_a_wall_clock_offset() {
    let migrated = |offset: AnchorOffset, model: RegionTimeModel| {
        let mut h = History::new();
        h.marker(40, edge(PROPORTIONAL, offset));
        let migration = h.migrate(PROPORTIONAL, model);
        let (free, _) = h.reduce();
        (stranded(&free, migration), applied(&free, migration))
    };
    let (named, _) = migrated(wall_clock(), valuegen::metric_model());
    assert_eq!(
        named,
        Some(vec![TypedObjectId::Marker(MarkerId::new(AUTHOR, 40))])
    );
    assert!(
        migrated(AnchorOffset::Zero, valuegen::metric_model()).1,
        "no offset"
    );
    assert!(
        migrated(wall_clock(), either()).1,
        "a model admitting wall-clock time"
    );
}

// --- A spanner's ---------------------------------------------------------

/// A spanner starting at a metric region's start by a wall-clock offset is
/// refused; no offset and a musical one apply.
#[test]
fn a_spanner_at_a_metric_region_s_start_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.spanner(41, anchor, at_note(NOTE, AnchorOffset::Zero), STAVES[0]),
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

/// A spanner modified onto a metric region's start by a wall-clock offset is
/// refused; onto a musical offset it applies.
#[test]
fn a_spanner_modified_onto_a_wall_clock_offset_in_a_metric_region_is_refused() {
    written(
        |h, anchor| {
            let end = at_note(NOTE, AnchorOffset::Zero);
            h.spanner(41, end.clone(), end.clone(), STAVES[0]);
            h.modify(None, spanner(41, anchor, end, STAVES[0]))
        },
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

/// A spanner at a proportional region's start by a wall-clock offset strands
/// the region's migration to metric, named; with no offset it applies.
#[test]
fn a_migration_into_musical_time_strands_a_spanner_anchored_by_a_wall_clock_offset() {
    let migrated = |offset: AnchorOffset| {
        let mut h = History::new();
        h.spanner(
            41,
            edge(PROPORTIONAL, offset),
            edge(PROPORTIONAL, AnchorOffset::Zero),
            STAVES[1],
        );
        let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
        let (free, _) = h.reduce();
        (stranded(&free, migration), applied(&free, migration))
    };
    assert_eq!(
        migrated(wall_clock()).0,
        Some(vec![TypedObjectId::Spanner(SpannerId::new(AUTHOR, 41))])
    );
    assert!(migrated(AnchorOffset::Zero).1, "no offset");
}

/// An undo restoring a spanner to a proportional region's start by a
/// wall-clock offset, the region migrated to metric since, is superseded by
/// the migration; to a model admitting wall-clock time it is restored.
#[test]
fn an_undo_restoring_a_spanner_into_a_region_migrated_to_metric_is_superseded() {
    let undo_of = |model: RegionTimeModel| {
        let mut h = History::new();
        let at = edge(PROPORTIONAL, wall_clock());
        let end = edge(PROPORTIONAL, AnchorOffset::Zero);
        h.spanner(41, at, end, STAVES[1]);
        let tx = h.transaction(30);
        let note = at_note(NOTE, AnchorOffset::Zero);
        h.modify(Some(tx), spanner(41, note.clone(), note, STAVES[0]));
        let migration = h.migrate(PROPORTIONAL, model);
        let undo = h.undo(tx, UndoPolicy::StrictInverse);
        let (free, aware) = h.reduce();
        (
            free,
            undo,
            migration,
            aware.score.cross_cutting.spanners[0].start.clone(),
        )
    };
    let (free, undo, migration, _) = undo_of(valuegen::metric_model());
    assert!(
        superseded_by(&free, undo, migration),
        "{:?}",
        effect(&free, undo)
    );
    let (free, undo, _, start) = undo_of(either());
    assert!(undone(&free, undo), "{:?}", effect(&free, undo));
    assert_eq!(start, edge(PROPORTIONAL, wall_clock()), "restored");
}

// --- A user break's -------------------------------------------------------

/// A system break at a metric region's start by a wall-clock offset is
/// refused; no offset and a musical one apply.
#[test]
fn a_system_break_at_a_metric_region_s_start_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.system_break(None, METRIC, anchor, true),
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

/// A page break at a metric region's note by a wall-clock offset is refused.
#[test]
fn a_page_break_at_a_note_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| {
            h.push(OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                region: METRIC,
                anchor,
                present: true,
            }))
        },
        |offset| at_note(NOTE, offset),
        wall_clock(),
        musical(),
    );
}

/// A system break at a proportional region's start by a wall-clock offset
/// strands the region's migration to metric, the region named as its
/// break's holder; with no offset it applies.
#[test]
fn a_migration_into_musical_time_strands_a_system_break_anchored_by_a_wall_clock_offset() {
    let migrated = |offset: AnchorOffset| {
        let mut h = History::new();
        let set = h.system_break(None, PROPORTIONAL, edge(PROPORTIONAL, offset), true);
        let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
        let (free, _) = h.reduce();
        assert!(applied(&free, set));
        (stranded(&free, migration), applied(&free, migration))
    };
    assert_eq!(
        migrated(wall_clock()).0,
        Some(vec![TypedObjectId::Region(PROPORTIONAL)])
    );
    assert!(migrated(AnchorOffset::Zero).1, "no offset");
}

/// An undo restoring a system break at a proportional region's start by a
/// wall-clock offset, the region migrated to metric since, is superseded by
/// the migration: the break stays removed. To a model admitting wall-clock
/// time it is restored.
#[test]
fn an_undo_restoring_a_system_break_into_a_region_migrated_to_metric_is_superseded() {
    let undo_of = |model: RegionTimeModel, policy: UndoPolicy| {
        let mut h = History::new();
        let at = edge(PROPORTIONAL, wall_clock());
        h.system_break(None, PROPORTIONAL, at.clone(), true);
        let tx = h.transaction(30);
        h.system_break(Some(tx), PROPORTIONAL, at, false);
        let migration = h.migrate(PROPORTIONAL, model);
        let undo = h.undo(tx, policy);
        let (free, aware) = h.reduce();
        let breaks = aware
            .score
            .canvas
            .regions
            .iter()
            .find(|r| r.id == PROPORTIONAL)
            .and_then(|r| r.content.staff_based())
            .map(|c| c.user_system_breaks.len());
        (free, undo, migration, breaks)
    };
    let (free, undo, _, breaks) = undo_of(valuegen::metric_model(), UndoPolicy::BestEffort);
    assert!(undone(&free, undo));
    assert_eq!(breaks, Some(0), "not restored");
    let (free, undo, migration, _) = undo_of(valuegen::metric_model(), UndoPolicy::StrictInverse);
    assert!(
        superseded_by(&free, undo, migration),
        "{:?}",
        effect(&free, undo)
    );
    let (free, undo, _, breaks) = undo_of(either(), UndoPolicy::StrictInverse);
    assert!(undone(&free, undo));
    assert_eq!(breaks, Some(1), "restored");
}

// --- A measure's -----------------------------------------------------------

/// A proportional region's first measure, started at its start by a
/// wall-clock offset, strands the region's migration to metric, named; with
/// no offset the migration applies.
#[test]
fn a_migration_into_musical_time_strands_a_measure_started_by_a_wall_clock_offset() {
    let migrated = |offset: AnchorOffset| {
        let mut h = History::new();
        let measure = MeasureId::new(AUTHOR, 932);
        let create = h.measure(PROPORTIONAL_INSTANCE, measure, edge(PROPORTIONAL, offset));
        let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
        let (free, _) = h.reduce();
        assert!(applied(&free, create));
        (stranded(&free, migration), applied(&free, migration))
    };
    assert_eq!(
        migrated(wall_clock()).0,
        Some(vec![TypedObjectId::Measure(MeasureId::new(AUTHOR, 932))])
    );
    assert!(migrated(AnchorOffset::Zero).1, "no offset");
}

/// A measure started at a metric region's start by a wall-clock offset, the
/// first of a proportional region's instance, is refused; no offset and a
/// musical one apply.
#[test]
fn a_measure_started_at_a_metric_region_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.measure(PROPORTIONAL_INSTANCE, MeasureId::new(AUTHOR, 932), anchor),
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

// --- A meter change's and a grid's ----------------------------------------

/// A time signature set at a metric region's start by a wall-clock offset is
/// refused; no offset and a musical one apply.
#[test]
fn a_time_signature_at_a_metric_region_s_start_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.time_signature(METRIC, anchor),
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

/// A metric region's time signature anchored to a proportional region's
/// start by a wall-clock offset (which that region admits) strands the
/// proportional region's migration to metric, naming the metric region as
/// its holder; with no offset it applies.
#[test]
fn a_migration_into_musical_time_strands_a_time_signature_anchored_to_it_by_a_wall_clock_offset() {
    let migrated = |offset: AnchorOffset| {
        let mut h = History::new();
        let set = h.time_signature(METRIC, edge(PROPORTIONAL, offset));
        let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
        let (free, _) = h.reduce();
        assert!(applied(&free, set));
        (stranded(&free, migration), applied(&free, migration))
    };
    assert_eq!(
        migrated(wall_clock()).0,
        Some(vec![TypedObjectId::Region(METRIC)])
    );
    assert!(migrated(AnchorOffset::Zero).1, "no offset");
}

/// A metric grid whose meter change stands at its region's start by a
/// wall-clock offset is refused; no offset and a musical one apply.
#[test]
fn a_metric_grid_s_meter_change_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.grid(anchor),
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
}

/// A metric grid's meter change anchored to a proportional region by a
/// wall-clock offset strands that region's migration to metric, naming the
/// grid's region; and one anchored to another metric region by a musical
/// offset strands that region's migration out of musical time.
#[test]
fn a_migration_strands_a_grid_s_meter_change_anchored_to_it() {
    let mut h = History::new();
    h.grid(edge(PROPORTIONAL, wall_clock()));
    let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
    let (free, _) = h.reduce();
    assert_eq!(
        stranded(&free, migration),
        Some(vec![TypedObjectId::Region(METRIC)])
    );

    let out = |offset: AnchorOffset| {
        let mut h = History::new();
        let other = RegionId::new(AUTHOR, 914);
        h.region(
            914,
            valuegen::metric_model(),
            wall(7_000_000_000),
            wall(7_000_001_000),
        );
        h.grid(edge(other, offset));
        let migration = h.migrate(other, valuegen::proportional_model());
        let (free, _) = h.reduce();
        (stranded(&free, migration), applied(&free, migration))
    };
    assert_eq!(out(musical()).0, Some(vec![TypedObjectId::Region(METRIC)]));
    assert!(out(AnchorOffset::Zero).1, "no offset");
}

// --- A tempo segment's -----------------------------------------------------

/// A tempo segment of a metric region's map at its start by a wall-clock
/// offset is refused, and so is one of the score's map at a metric note so;
/// no offset and a musical one apply.
#[test]
fn a_tempo_segment_by_a_wall_clock_offset_in_a_metric_region_is_refused() {
    written(
        |h, anchor| h.tempo(None, Some(METRIC), anchor, true),
        |offset| edge(METRIC, offset),
        wall_clock(),
        musical(),
    );
    written(
        |h, anchor| h.tempo(None, None, anchor, true),
        |offset| at_note(NOTE, offset),
        wall_clock(),
        musical(),
    );
}

/// A tempo segment at a proportional region's start by a wall-clock offset
/// strands the region's migration to metric, whether the region's own map
/// or the score's holds it, the region named either way; with no offset it
/// applies.
#[test]
fn a_migration_into_musical_time_strands_a_tempo_segment_anchored_by_a_wall_clock_offset() {
    for scope in [Some(PROPORTIONAL), None] {
        let migrated = |offset: AnchorOffset| {
            let mut h = History::new();
            let set = h.tempo(None, scope, edge(PROPORTIONAL, offset), true);
            let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
            let (free, _) = h.reduce();
            assert!(applied(&free, set));
            (stranded(&free, migration), applied(&free, migration))
        };
        assert_eq!(
            migrated(wall_clock()).0,
            Some(vec![TypedObjectId::Region(PROPORTIONAL)]),
            "{scope:?}"
        );
        assert!(migrated(AnchorOffset::Zero).1, "no offset, {scope:?}");
    }
}

/// An undo restoring a proportional region's tempo segment at its start by
/// a wall-clock offset, the region migrated to metric since, is superseded
/// by the migration; to a model admitting wall-clock time it is restored.
#[test]
fn an_undo_restoring_a_tempo_segment_into_a_region_migrated_to_metric_is_superseded() {
    let undo_of = |model: RegionTimeModel| {
        let mut h = History::new();
        let at = edge(PROPORTIONAL, wall_clock());
        h.tempo(None, Some(PROPORTIONAL), at.clone(), true);
        let tx = h.transaction(30);
        h.tempo(Some(tx), Some(PROPORTIONAL), at, false);
        let migration = h.migrate(PROPORTIONAL, model);
        let undo = h.undo(tx, UndoPolicy::StrictInverse);
        let (free, _) = h.reduce();
        (free, undo, migration)
    };
    let (free, undo, migration) = undo_of(valuegen::metric_model());
    assert!(
        superseded_by(&free, undo, migration),
        "{:?}",
        effect(&free, undo)
    );
    let (free, undo, _) = undo_of(either());
    assert!(undone(&free, undo), "{:?}", effect(&free, undo));
}

// --- A region's own model, content and tempo map ---------------------------

/// A region created with its own metric model's meter change at its start
/// by a wall-clock offset is refused; no offset and a musical one apply.
#[test]
fn a_region_s_own_meter_change_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| {
            h.time_signature(METRIC, edge(METRIC, AnchorOffset::Zero));
            h.shaped_region(13, own_meter(anchor), |_| {})
        },
        |offset| edge(RegionId::new(AUTHOR, 13), offset),
        wall_clock(),
        musical(),
    );
}

/// A region created carrying a system break at its start by a wall-clock
/// offset is refused; no offset and a musical one apply.
#[test]
fn a_region_s_own_content_break_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.shaped_region(13, valuegen::metric_model(), |r| carry_break(r, anchor)),
        |offset| edge(RegionId::new(AUTHOR, 13), offset),
        wall_clock(),
        musical(),
    );
}

/// A region created carrying a tempo segment at its start by a wall-clock
/// offset is refused; no offset and a musical one apply.
#[test]
fn a_region_s_own_tempo_segment_by_a_wall_clock_offset_is_refused() {
    written(
        |h, anchor| h.shaped_region(13, valuegen::metric_model(), |r| carry_tempo(r, anchor)),
        |offset| edge(RegionId::new(AUTHOR, 13), offset),
        wall_clock(),
        musical(),
    );
}

/// A region whose own model's meter change, carried break or carried tempo
/// segment stands at a proportional region's start by a wall-clock offset
/// strands that region's migration to metric, naming the region holding it;
/// with no offset the migration applies.
#[test]
fn a_migration_into_musical_time_strands_a_region_s_payload_anchored_to_it() {
    let holder = RegionId::new(AUTHOR, 13);
    let migrated = |carry: &Carrier| {
        let mut out = Vec::new();
        for offset in [wall_clock(), AnchorOffset::Zero] {
            let mut h = History::new();
            h.time_signature(METRIC, edge(METRIC, AnchorOffset::Zero));
            carry(&mut h, edge(PROPORTIONAL, offset));
            let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
            let (free, _) = h.reduce();
            out.push((stranded(&free, migration), applied(&free, migration)));
        }
        out
    };
    let carriers: [&Carrier; 3] = [
        &|h, anchor| {
            h.shaped_region(13, own_meter(anchor), |_| {});
        },
        &|h, anchor| {
            h.shaped_region(13, valuegen::metric_model(), |r| carry_break(r, anchor));
        },
        &|h, anchor| {
            h.shaped_region(13, valuegen::metric_model(), |r| carry_tempo(r, anchor));
        },
    ];
    for (i, carry) in carriers.into_iter().enumerate() {
        let out = migrated(carry);
        assert_eq!(
            out[0].0,
            Some(vec![TypedObjectId::Region(holder)]),
            "carrier {i}"
        );
        assert!(out[1].1, "carrier {i}, no offset");
    }
}

/// A migration to a metric model whose own meter change stands at the
/// region's start by a wall-clock offset is refused; with no offset or a
/// musical one it applies.
#[test]
fn a_migration_to_a_model_whose_own_meter_change_its_region_will_not_admit_is_refused() {
    written(
        |h, anchor| {
            h.time_signature(METRIC, edge(METRIC, AnchorOffset::Zero));
            h.migrate(PROPORTIONAL, own_meter(anchor))
        },
        |offset| edge(PROPORTIONAL, offset),
        wall_clock(),
        musical(),
    );
}

fn own_meter(anchor: TimeAnchor) -> RegionTimeModel {
    RegionTimeModel::Metric(MetricTimeModel {
        meters: vec![MeterChange {
            anchor,
            time_signature: SIGNATURE,
        }],
        ..Default::default()
    })
}

fn carry_break(region: &mut epiphany_core::Region, anchor: TimeAnchor) {
    region
        .content
        .staff_based_mut()
        .expect("staff-based")
        .user_system_breaks
        .push(anchor);
}

fn carry_tempo(region: &mut epiphany_core::Region, anchor: TimeAnchor) {
    region.local_tempo_map = Some(TempoMap {
        initial: None,
        segments: vec![segment_at(anchor)],
    });
}

// --- A repeat structure's --------------------------------------------------

/// A repeat starting at a metric note by a wall-clock offset is refused, and
/// so is one at a proportional region's start by a musical offset, which
/// version 4 never read; no offset and the admitted kind apply.
#[test]
fn a_repeat_by_an_offset_its_target_will_not_admit_is_refused() {
    written(
        |h, anchor| h.repeat(80, anchor, at_note(NOTE, AnchorOffset::Zero)),
        |offset| at_note(NOTE, offset),
        wall_clock(),
        musical(),
    );
    written(
        |h, anchor| h.repeat(80, anchor, edge(PROPORTIONAL, AnchorOffset::Zero)),
        |offset| edge(PROPORTIONAL, offset),
        musical(),
        wall_clock(),
    );
}

/// A repeat at a proportional region's start by a wall-clock offset strands
/// its migration to metric, and one at an empty metric region's start by a
/// musical offset strands its migration out of musical time, the repeat
/// named; with no offset each applies.
#[test]
fn a_migration_strands_a_repeat_anchored_to_it() {
    let migrated = |region: RegionId, offset: AnchorOffset, model: RegionTimeModel| {
        let mut h = History::new();
        h.region(
            914,
            valuegen::metric_model(),
            wall(7_000_000_000),
            wall(7_000_001_000),
        );
        h.repeat(80, edge(region, offset), edge(region, AnchorOffset::Zero));
        let migration = h.migrate(region, model);
        let (free, _) = h.reduce();
        (stranded(&free, migration), applied(&free, migration))
    };
    let repeat = TypedObjectId::RepeatStructure(RepeatStructureId::new(AUTHOR, 80));
    let empty = RegionId::new(AUTHOR, 914);
    assert_eq!(
        migrated(PROPORTIONAL, wall_clock(), valuegen::metric_model()).0,
        Some(vec![repeat])
    );
    assert!(migrated(PROPORTIONAL, AnchorOffset::Zero, valuegen::metric_model()).1);
    assert_eq!(
        migrated(empty, musical(), valuegen::proportional_model()).0,
        Some(vec![repeat])
    );
    assert!(migrated(empty, AnchorOffset::Zero, valuegen::proportional_model()).1);
}

// --- A staff instance's carried clef, key and grid --------------------------

/// A staff instance created in a metric region carrying a clef change, a key
/// change or its own grid's meter change at the region's start by a
/// wall-clock offset is refused; no offset and a musical one apply.
#[test]
fn a_staff_instance_s_carried_change_by_a_wall_clock_offset_is_refused() {
    let instance = StaffInstanceId::new(AUTHOR, 924);
    let carriers: [fn(&mut epiphany_core::StaffInstance, TimeAnchor); 3] = [
        |i, anchor| {
            i.clef_sequence.push(ClefChange {
                anchor,
                clef: Clef::treble(),
            })
        },
        |i, anchor| {
            i.key_sequence.push(KeySignatureChange {
                anchor,
                key: KeySignature::new(0).expect("C major"),
            })
        },
        |i, anchor| {
            i.local_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor,
                    time_signature: SIGNATURE,
                }],
            })
        },
    ];
    for carry in carriers {
        written(
            |h, anchor| {
                h.time_signature(METRIC, edge(METRIC, AnchorOffset::Zero));
                h.shaped_instance(METRIC, instance, STAVES[2], |i| carry(i, anchor))
            },
            |offset| edge(METRIC, offset),
            wall_clock(),
            musical(),
        );
    }
}

/// A staff instance carrying a clef change at a proportional region's start
/// by a wall-clock offset strands that region's migration to metric, the
/// instance named, until a clef written at its place takes it over; with no
/// offset the migration applies.
#[test]
fn a_migration_into_musical_time_strands_a_staff_instance_s_carried_clef() {
    let instance = StaffInstanceId::new(AUTHOR, 924);
    let migrated = |offset: AnchorOffset, overwrite: bool| {
        let mut h = History::new();
        h.shaped_region(13, valuegen::metric_model(), |_| {});
        h.shaped_instance(RegionId::new(AUTHOR, 13), instance, STAVES[2], |i| {
            i.clef_sequence.push(ClefChange {
                anchor: edge(PROPORTIONAL, offset),
                clef: Clef::treble(),
            })
        });
        if overwrite {
            h.push(OperationKind::SetClef(epiphany_ops::SetClefOp {
                instance,
                offset: RationalTime::zero(),
                clef: Some(Clef::bass()),
            }));
        }
        let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
        let (free, _) = h.reduce();
        (stranded(&free, migration), applied(&free, migration))
    };
    assert_eq!(
        migrated(wall_clock(), false).0,
        Some(vec![TypedObjectId::StaffInstance(instance)])
    );
    assert!(migrated(AnchorOffset::Zero, false).1, "no offset");
    assert!(migrated(wall_clock(), true).1, "the clef written over");
}

// --- The clock an event fixes ------------------------------------------------

/// In a region where each event fixes its clock, a marker at a note by a
/// wall-clock offset is refused, the note's clock musical; and a marker at a
/// note of a freely mixed region by a wall-clock offset, which that region
/// admits, strands the region's migration back to metric.
#[test]
fn a_note_s_holder_answers_to_the_clock_its_region_gives_it() {
    let mut h = History::new();
    h.migrate(
        METRIC,
        aleatoric(AleatoricAnchoringDiscipline::EitherPerEvent),
    );
    let by_wall_clock = h.marker(40, at_note(NOTE, wall_clock()));
    let by_music = h.marker(41, at_note(NOTE, musical()));
    let (free, _) = h.reduce();
    assert!(refused(&free, by_wall_clock));
    assert!(applied(&free, by_music));

    let back = |offset: AnchorOffset| {
        let mut h = History::new();
        h.migrate(METRIC, either());
        let set = h.marker(40, at_note(NOTE, offset));
        let migration = h.migrate(METRIC, valuegen::metric_model());
        let (free, _) = h.reduce();
        assert!(applied(&free, set));
        (stranded(&free, migration), applied(&free, migration))
    };
    assert_eq!(
        back(wall_clock()).0,
        Some(vec![TypedObjectId::Marker(MarkerId::new(AUTHOR, 40))])
    );
    assert!(back(musical()).1, "a musical offset");
}

// --- D84 Q1: named before it is made ------------------------------------------

/// A region's extent naming a note before it is made by a wall-clock offset
/// refuses the note's insert into a metric region; by a musical offset the
/// insert applies, and an extent at a live note by a wall-clock offset is
/// refused at the extent's create (X5b.8's own reading).
#[test]
fn a_note_an_extent_names_before_it_is_made_by_a_wall_clock_offset_is_refused() {
    let named = |offset: AnchorOffset| {
        let mut h = History::new();
        let create = h.region(
            13,
            valuegen::metric_model(),
            at_note(EventId::new(AUTHOR, 959), offset),
            wall(9_000_000_000),
        );
        let insert = h.note(959, 2);
        let (free, _) = h.reduce();
        assert!(applied(&free, create));
        (refused(&free, insert), applied(&free, insert))
    };
    assert!(named(wall_clock()).0);
    assert!(named(musical()).1, "a musical offset");
    assert!(named(AnchorOffset::Zero).1, "no offset");
}

/// A region's extent naming a measure before it is made by a wall-clock
/// offset refuses the measure's create in a metric region; by a musical
/// offset the create applies.
#[test]
fn a_measure_an_extent_names_before_it_is_made_by_a_wall_clock_offset_is_refused() {
    let named = |offset: AnchorOffset| {
        let mut h = History::new();
        let measure = MeasureId::new(AUTHOR, 931);
        h.region(
            13,
            valuegen::metric_model(),
            at_measure(measure, offset),
            wall(9_000_000_000),
        );
        let create = h.measure(
            INSTANCE,
            measure,
            edge(
                METRIC,
                AnchorOffset::Musical(MusicalDuration(RationalTime::new(1, 1).expect("a whole"))),
            ),
        );
        let (free, _) = h.reduce();
        (refused(&free, create), applied(&free, create))
    };
    assert!(named(wall_clock()).0);
    assert!(named(musical()).1, "a musical offset");
}

/// Every other holder that may name an object before it is made does so as
/// an extent does: a time signature, a grid's meter change, a region's
/// carried break and a staff instance's carried clef naming a note by a
/// wall-clock offset refuse its insert, and a time signature naming a region
/// by one refuses its create as metric. By a musical offset each insert and
/// create applies. A tempo segment names no note before it is made: it is
/// refused, naming nothing yet.
#[test]
fn an_object_another_holder_names_before_it_is_made_is_refused_a_model_that_contradicts_it() {
    let unmade = EventId::new(AUTHOR, 959);
    let namers: [&Carrier; 4] = [
        &|h, anchor| {
            h.time_signature(METRIC, anchor);
        },
        &|h, anchor| {
            h.grid(anchor);
        },
        &|h, anchor| {
            h.shaped_region(13, valuegen::metric_model(), |r| carry_break(r, anchor));
        },
        &|h, anchor| {
            h.shaped_region(13, valuegen::metric_model(), |_| {});
            h.shaped_instance(
                RegionId::new(AUTHOR, 13),
                StaffInstanceId::new(AUTHOR, 924),
                STAVES[2],
                |i| {
                    i.clef_sequence.push(ClefChange {
                        anchor,
                        clef: Clef::treble(),
                    })
                },
            );
        },
    ];
    for (i, name) in namers.into_iter().enumerate() {
        for (offset, refuses) in [(wall_clock(), true), (musical(), false)] {
            let mut h = History::new();
            name(&mut h, at_note(unmade, offset.clone()));
            let insert = h.note(959, 2);
            let (free, _) = h.reduce();
            assert_eq!(refused(&free, insert), refuses, "namer {i}, {offset:?}");
            assert_eq!(applied(&free, insert), !refuses, "namer {i}, {offset:?}");
        }
    }
    for (offset, refuses) in [(wall_clock(), true), (musical(), false)] {
        let mut h = History::new();
        let unmade = RegionId::new(AUTHOR, 13);
        h.time_signature(METRIC, edge(unmade, offset.clone()));
        let create = h.shaped_region(13, valuegen::metric_model(), |_| {});
        let (free, _) = h.reduce();
        assert_eq!(refused(&free, create), refuses, "{offset:?}");
    }
}

/// A tempo segment anchored to a note or a measure that is not live names
/// nothing and is refused `TargetMissing`, as one anchored to a region that
/// is not live is since version 3, so the note it would name before it is
/// made can be made; at a live note it applies.
#[test]
fn a_tempo_segment_naming_a_note_or_measure_not_yet_made_is_refused() {
    let missing = OperationEffect::NoOp {
        reason: NoOpReason::PreconditionFailedUnderReduction {
            reason: PreconditionFailureReason::TargetMissing,
        },
    };
    let unmade = EventId::new(AUTHOR, 959);
    let mut h = History::new();
    let set = h.tempo(None, None, at_note(unmade, musical()), true);
    let insert = h.note(959, 2);
    let (free, _) = h.reduce();
    assert_eq!(*effect(&free, set), missing);
    assert!(applied(&free, insert));

    let mut h = History::new();
    let measure = MeasureId::new(AUTHOR, 931);
    let set = h.tempo(None, None, at_measure(measure, AnchorOffset::Zero), true);
    h.measure(
        INSTANCE,
        measure,
        edge(
            METRIC,
            AnchorOffset::Musical(MusicalDuration(RationalTime::new(1, 1).expect("a whole"))),
        ),
    );
    let (free, _) = h.reduce();
    assert_eq!(*effect(&free, set), missing);

    let mut h = History::new();
    let set = h.tempo(None, None, at_note(NOTE, musical()), true);
    let (free, _) = h.reduce();
    assert!(applied(&free, set));
}

/// One step past: a marker at a note of a freely mixed region by a
/// wall-clock offset, the note deleted, is re-anchored to the region's other
/// note with its offset, which the region admits, and the region's
/// migration back to metric then strands it there.
#[test]
fn a_marker_its_note_s_delete_re_anchors_keeps_an_offset_its_region_admits() {
    use epiphany_ops::{DeleteEventOp, TupletCompensation};
    let marked = MarkerId::new(AUTHOR, 40);
    for back in [false, true] {
        let mut h = History::new();
        h.note(959, 2);
        h.migrate(METRIC, either());
        let set = h.marker(40, at_note(NOTE, wall_clock()));
        h.push(OperationKind::DeleteEvent(DeleteEventOp {
            event: NOTE,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        }));
        let migration = back.then(|| h.migrate(METRIC, valuegen::metric_model()));
        let (free, aware) = h.reduce();
        assert!(applied(&free, set));
        assert_eq!(
            aware.score.cross_cutting.markers[0].anchor,
            at_note(EventId::new(AUTHOR, 959), wall_clock())
        );
        if let Some(migration) = migration {
            assert_eq!(
                stranded(&free, migration),
                Some(vec![TypedObjectId::Marker(marked)])
            );
        }
    }
}

/// One step past, across regions: where the only other note stands on the
/// same staff in a metric region, the marker is orphaned to its own region's
/// start with no offset, never carried there by its wall-clock offset.
#[test]
fn a_marker_its_note_s_delete_orphans_keeps_no_offset() {
    use epiphany_ops::{DeleteEventOp, TupletCompensation};
    let other = RegionId::new(AUTHOR, 13);
    let other_instance = StaffInstanceId::new(AUTHOR, 924);
    let other_voice = VoiceId::new(AUTHOR, 944);
    let mut h = History::new();
    h.shaped_region(13, valuegen::metric_model(), |_| {});
    h.instance(other, other_instance, STAVES[0]);
    h.push(OperationKind::CreateVoice(CreateVoiceOp {
        staff_instance: other_instance,
        voice: valuegen::voice(other_voice),
    }));
    h.push(OperationKind::InsertEvent(InsertEventOp {
        staff_instance: other_instance,
        event: valuegen::insert_event_value(
            EventId::new(AUTHOR, 959),
            other_voice,
            MusicalPosition::origin(),
            quarter(),
            &[PitchId::new(AUTHOR, 960)],
        ),
    }));
    h.migrate(METRIC, either());
    let set = h.marker(40, at_note(NOTE, wall_clock()));
    h.push(OperationKind::DeleteEvent(DeleteEventOp {
        event: NOTE,
        tuplet_compensation: TupletCompensation::NotInTuplet,
    }));
    let (free, aware) = h.reduce();
    assert!(applied(&free, set));
    assert_eq!(
        aware.score.cross_cutting.markers[0].anchor,
        edge(METRIC, AnchorOffset::Zero)
    );
}

/// An undo restoring a metric grid, or a meter change, anchored to a
/// proportional region's start by a wall-clock offset, that region migrated
/// to metric since, is superseded by the migration; to a model admitting
/// wall-clock time each is restored.
#[test]
fn an_undo_restoring_a_grid_or_a_meter_change_into_a_region_migrated_to_metric_is_superseded() {
    for whole in [true, false] {
        let undo_of = |model: RegionTimeModel| {
            let mut h = History::new();
            let at = edge(PROPORTIONAL, wall_clock());
            if whole {
                h.grid(at);
            } else {
                h.time_signature(METRIC, at);
            }
            let tx = h.transaction(30);
            let clear = if whole {
                OperationKind::SetMetricGrid(SetMetricGridOp {
                    region: METRIC,
                    grid: None,
                })
            } else {
                OperationKind::SetTimeSignature(SetTimeSignatureOp {
                    region: METRIC,
                    anchor: edge(PROPORTIONAL, wall_clock()),
                    time_signature: None,
                })
            };
            h.push_payload(Some(tx), OperationPayload::Primitive(clear));
            let migration = h.migrate(PROPORTIONAL, model);
            let undo = h.undo(tx, UndoPolicy::StrictInverse);
            let (free, _) = h.reduce();
            assert!(applied(&free, migration), "whole {whole}");
            (free, undo, migration)
        };
        let (free, undo, migration) = undo_of(valuegen::metric_model());
        assert!(
            superseded_by(&free, undo, migration),
            "whole {whole}: {:?}",
            effect(&free, undo)
        );
        let (free, undo, _) = undo_of(either());
        assert!(
            undone(&free, undo),
            "whole {whole}: {:?}",
            effect(&free, undo)
        );
    }
}

/// An undo restoring a page break at a proportional region's start by a
/// wall-clock offset, the region migrated to metric since, is superseded by
/// the migration; to a model admitting wall-clock time it is restored.
#[test]
fn an_undo_restoring_a_page_break_into_a_region_migrated_to_metric_is_superseded() {
    let undo_of = |model: RegionTimeModel| {
        let mut h = History::new();
        let page_break = |present| {
            OperationPayload::Primitive(OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                region: PROPORTIONAL,
                anchor: edge(PROPORTIONAL, wall_clock()),
                present,
            }))
        };
        h.push_payload(None, page_break(true));
        let tx = h.transaction(30);
        h.push_payload(Some(tx), page_break(false));
        let migration = h.migrate(PROPORTIONAL, model);
        let undo = h.undo(tx, UndoPolicy::StrictInverse);
        let (free, _) = h.reduce();
        (free, undo, migration)
    };
    let (free, undo, migration) = undo_of(valuegen::metric_model());
    assert!(
        superseded_by(&free, undo, migration),
        "{:?}",
        effect(&free, undo)
    );
    let (free, undo, _) = undo_of(either());
    assert!(undone(&free, undo), "{:?}", effect(&free, undo));
}

/// A staff instance's own grid, its meter change at a proportional region's
/// start by a wall-clock offset, strands that region's migration to metric,
/// the instance named, and so does a key change it carries there; with no
/// offset the migration applies.
#[test]
fn a_migration_into_musical_time_strands_a_staff_instance_s_grid_or_key() {
    let instance = StaffInstanceId::new(AUTHOR, 924);
    for grid in [true, false] {
        let migrated = |offset: AnchorOffset| {
            let mut h = History::new();
            h.time_signature(METRIC, edge(METRIC, AnchorOffset::Zero));
            h.shaped_region(13, valuegen::metric_model(), |_| {});
            let anchor = edge(PROPORTIONAL, offset);
            h.shaped_instance(RegionId::new(AUTHOR, 13), instance, STAVES[2], |i| {
                if grid {
                    i.local_metric_grid = Some(MetricGrid {
                        meter_sequence: vec![MeterChange {
                            anchor,
                            time_signature: SIGNATURE,
                        }],
                    });
                } else {
                    i.key_sequence.push(KeySignatureChange {
                        anchor,
                        key: KeySignature::new(2).expect("D major"),
                    });
                }
            });
            let migration = h.migrate(PROPORTIONAL, valuegen::metric_model());
            let (free, _) = h.reduce();
            (stranded(&free, migration), applied(&free, migration))
        };
        assert_eq!(
            migrated(wall_clock()).0,
            Some(vec![TypedObjectId::StaffInstance(instance)]),
            "grid {grid}"
        );
        assert!(migrated(AnchorOffset::Zero).1, "grid {grid}, no offset");
    }
}

/// What a migration out of musical time drops with its region's breaks and
/// grid is not stranded: an empty metric region holding a break and a grid
/// at its start by a musical offset migrates to a proportional model.
#[test]
fn a_migration_out_of_musical_time_strands_not_what_it_drops() {
    let mut h = History::new();
    let empty = RegionId::new(AUTHOR, 13);
    h.shaped_region(13, valuegen::metric_model(), |_| {});
    h.time_signature(empty, edge(empty, AnchorOffset::Zero));
    let set_grid = h.push(OperationKind::SetMetricGrid(SetMetricGridOp {
        region: empty,
        grid: Some(MetricGrid {
            meter_sequence: vec![MeterChange {
                anchor: edge(empty, musical()),
                time_signature: SIGNATURE,
            }],
        }),
    }));
    let set_break = h.system_break(None, empty, edge(empty, musical()), true);
    let migration = h.migrate(empty, valuegen::proportional_model());
    let (free, aware) = h.reduce();
    assert!(applied(&free, set_grid));
    assert!(applied(&free, set_break));
    assert!(applied(&free, migration), "{:?}", effect(&free, migration));
    let content = aware
        .score
        .canvas
        .regions
        .iter()
        .find(|r| r.id == empty)
        .and_then(|r| r.content.staff_based())
        .expect("staff-based");
    assert!(content.user_system_breaks.is_empty());
    assert!(content.default_metric_grid.is_none());
}
