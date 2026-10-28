//! What a removal leaves naming nothing is held (reduction version 5, C5b
//! review 1, D86 and D87): graph invariant 10 says every graph reference
//! resolves, and an object a live holder names is not removed from under it.
//! An undo that would remove a time signature a live region's grid names, as
//! the grid's chains give it once the undo's restorations apply, is held as
//! for a meter change at a key (D86 Q1): a strict or cascading undo
//! conflicts, and a best-effort one keeps the signature. A note a live tempo
//! segment of either map names is not deleted (`TargetAnchored`), and an undo
//! that would remove it is held so (D86 Q2). A holder of a time anchor is not
//! written naming an event, a measure or a region that is not live: its
//! write is refused `TargetMissing` (D87 Q2). And every holder outside the
//! cross-cutting structures (a time signature, a grid's meter change, a
//! measure's start, a region's extent, a staff instance's carried clef, key or
//! grid, a region's own model) holds the note, measure or region it names
//! against a delete and an undo under every policy, as a tempo segment does
//! (D87 Q1); a user break is excepted (D80). An undo that would remove a
//! tuplet a live nested tuplet names as its parent is held, and a member's
//! cascade removing a parent carries its children (D87 Q3). A region is not
//! created carrying a tempo map with two segments on one key, which the
//! holds, reading the tempo chains, would see one of (D89).
//!
//! Each history is one author's, built by hand on a small score: a metric
//! region holding a staff instance, a measure, a voice and a note, and a
//! second metric region far away in time. Each is reduced both ways, the two
//! held to each other in every effect and the canonical bytes, the graph
//! checked, and the verdict the history exists for asserted under each undo
//! policy, beside controls that apply.

use epiphany_core::{
    check_invariants, AnchorOffset, EventId, IdentityContext, InstrumentId, Measure, MeasureId,
    MeasureNumberVisibility, MeterChange, MetricGrid, MusicalDuration, MusicalPosition,
    OperationId, PitchId, RationalTime, RegionEdge, RegionId, RegionTimeModel, ReplicaId, Score,
    StaffId, StaffInstanceId, TempoMap, TimeAnchor, TimeExtent, TimeSignatureId, TransactionId,
    TypedObjectId, VoiceId, WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, ChangeRegionTimeModelOp, ConflictKind, CreateInstrumentOp,
    CreateMeasureOp, CreateRegionOp, CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp,
    DeleteEventOp, GraphMaterialization, HybridLogicalClock, InsertEventOp, MaterializedState,
    NoOpReason, OperationEffect, OperationEnvelope, OperationKind, OperationPayload, OperationSet,
    OperationStamp, PositionRemapping, PreconditionFailureReason, SetMetricGridOp,
    SetTempoSegmentOp, SetTimeSignatureOp, TransactionDescriptor, TupletCompensation, UndoPolicy,
    UndoTransactionPayload,
};

const AUTHOR: ReplicaId = ReplicaId(5);
const METRIC: RegionId = RegionId::new(AUTHOR, 910);
const OTHER: RegionId = RegionId::new(AUTHOR, 911);
const INSTANCE: StaffInstanceId = StaffInstanceId::new(AUTHOR, 920);
const MEASURE: MeasureId = MeasureId::new(AUTHOR, 930);
const VOICE: VoiceId = VoiceId::new(AUTHOR, 940);
const NOTE: EventId = EventId::new(AUTHOR, 950);
const LATER_NOTE: EventId = EventId::new(AUTHOR, 960);
const SIGNATURE: TimeSignatureId = TimeSignatureId::new(AUTHOR, 970);
const STAVES: [StaffId; 3] = [
    StaffId::new(AUTHOR, 901),
    StaffId::new(AUTHOR, 902),
    StaffId::new(AUTHOR, 903),
];
const POLICIES: [UndoPolicy; 3] = [
    UndoPolicy::StrictInverse,
    UndoPolicy::Cascade,
    UndoPolicy::BestEffort,
];

/// One author's history, each envelope seeing every one before it.
struct History {
    envelopes: Vec<OperationEnvelope>,
    next: u64,
}

impl History {
    /// An instrument and three staves; a metric region holding an instance
    /// of the first, its measure, a voice and a quarter note at its start;
    /// another metric region, empty, far away.
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
        h.region(None, METRIC, valuegen::metric_model(), 1_000_000_000);
        h.region(None, OTHER, valuegen::metric_model(), 2_000_000_000);
        h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: METRIC,
            instance: valuegen::staff_instance(INSTANCE, STAVES[0]),
        }));
        h.push(OperationKind::CreateMeasure(CreateMeasureOp {
            instance: INSTANCE,
            measure: Measure {
                id: MEASURE,
                start: edge(METRIC, AnchorOffset::Zero),
                time_signature: None,
                explicit_number: None,
                number_visibility: MeasureNumberVisibility::Auto,
            },
        }));
        h.push(OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: INSTANCE,
            voice: valuegen::voice(VOICE),
        }));
        h.note(None, NOTE, 0);
        h
    }

    /// A quarter note `id` in the fixture's voice at quarter `at`.
    fn note(&mut self, transaction: Option<TransactionId>, id: EventId, at: i64) -> OperationId {
        self.push_in(
            transaction,
            OperationKind::InsertEvent(InsertEventOp {
                staff_instance: INSTANCE,
                event: valuegen::insert_event_value(
                    id,
                    VOICE,
                    MusicalPosition(RationalTime::new(at, 4).expect("a quarter")),
                    quarter(),
                    &[PitchId::new(AUTHOR, id.counter() + 1)],
                ),
            }),
        )
    }

    fn delete(&mut self, event: EventId) -> OperationId {
        self.push(OperationKind::DeleteEvent(DeleteEventOp {
            event,
            tuplet_compensation: TupletCompensation::NotInTuplet,
        }))
    }

    /// A tempo segment of `scope`'s map (the score's for `None`) starting at
    /// `start`, set or (with `present` false) removed.
    fn tempo(
        &mut self,
        transaction: Option<TransactionId>,
        scope: Option<RegionId>,
        start: TimeAnchor,
        present: bool,
    ) -> OperationId {
        let segment = present.then(|| {
            let mut segment = valuegen::tempo_segment(METRIC, MusicalPosition::origin(), 90.0);
            segment.start = start.clone();
            segment
        });
        self.tempo_segment(transaction, scope, start, segment)
    }

    /// A tempo segment of `scope`'s map from the metric region's start to
    /// `end`.
    fn tempo_ending(&mut self, scope: Option<RegionId>, end: TimeAnchor) -> OperationId {
        let start = edge(METRIC, AnchorOffset::Zero);
        let mut segment = valuegen::tempo_segment(METRIC, MusicalPosition::origin(), 90.0);
        segment.end = Some(end);
        self.tempo_segment(None, scope, start, Some(segment))
    }

    fn tempo_segment(
        &mut self,
        transaction: Option<TransactionId>,
        scope: Option<RegionId>,
        start: TimeAnchor,
        segment: Option<epiphany_core::TempoSegment>,
    ) -> OperationId {
        self.push_in(
            transaction,
            OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: scope,
                start,
                segment,
            }),
        )
    }

    fn push(&mut self, kind: OperationKind) -> OperationId {
        self.push_in(None, kind)
    }

    fn push_in(&mut self, transaction: Option<TransactionId>, kind: OperationKind) -> OperationId {
        self.push_payload(transaction, OperationPayload::Primitive(kind))
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

    /// An empty region `id` of `model`, a thousand units long from `start`.
    fn region(
        &mut self,
        transaction: Option<TransactionId>,
        id: RegionId,
        model: RegionTimeModel,
        start: i64,
    ) -> OperationId {
        let mut region = valuegen::region(id);
        region.time_model = model;
        region.time_extent = TimeExtent {
            start: wall(start),
            end: wall(start + 1000),
        };
        self.push_in(
            transaction,
            OperationKind::CreateRegion(CreateRegionOp { region }),
        )
    }

    /// The fixture's signature set in the metric region at its start.
    fn signature(&mut self, transaction: Option<TransactionId>) -> OperationId {
        self.push_in(
            transaction,
            OperationKind::SetTimeSignature(SetTimeSignatureOp {
                region: METRIC,
                anchor: edge(METRIC, AnchorOffset::Zero),
                time_signature: Some(valuegen::time_signature(SIGNATURE, 4)),
            }),
        )
    }

    /// `region`'s grid, one meter change of the fixture's signature at
    /// `anchor`.
    fn grid(
        &mut self,
        transaction: Option<TransactionId>,
        region: RegionId,
        anchor: TimeAnchor,
    ) -> OperationId {
        self.push_in(
            transaction,
            OperationKind::SetMetricGrid(SetMetricGridOp {
                region,
                grid: Some(MetricGrid {
                    meter_sequence: vec![MeterChange {
                        anchor,
                        time_signature: SIGNATURE,
                    }],
                }),
            }),
        )
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

fn edge(region: RegionId, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge: RegionEdge::Start,
        offset,
    }
}

fn at_note(id: EventId) -> TimeAnchor {
    TimeAnchor::Event {
        id,
        offset: AnchorOffset::Zero,
    }
}

fn quarter() -> MusicalDuration {
    MusicalDuration(RationalTime::new(1, 4).expect("a quarter"))
}

fn bars(n: i64) -> AnchorOffset {
    AnchorOffset::Musical(MusicalDuration(RationalTime::new(n, 1).expect("bars")))
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

/// Whether an undo conflicted as held, its conflict naming `held` among the
/// objects it affects.
fn held(state: &MaterializedState, undo: OperationId, held: TypedObjectId) -> bool {
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
                && record.affected_objects.contains(&held)
        })
}

fn declares(aware: &GraphMaterialization, signature: TimeSignatureId) -> bool {
    aware
        .score
        .time_signatures
        .iter()
        .any(|t| t.id == signature)
}

/// A signature set in a transaction and a grid naming it written after, by
/// `grid`; the transaction undone under each policy. A strict or cascading
/// undo conflicts, naming the signature and the grid's region, and a
/// best-effort one applies and keeps the signature, where before each
/// removed it and left the grid naming nothing (`CrossCuttingRefsResolve`).
fn a_signature_a_later_grid_names_is_held(grid: impl Fn(&mut History) -> RegionId) {
    for policy in POLICIES {
        let mut h = History::new();
        let transaction = h.transaction(500);
        h.signature(Some(transaction));
        let region = grid(&mut h);
        let undo = h.undo(transaction, policy);
        let (free, aware) = h.reduce();
        assert!(declares(&aware, SIGNATURE), "{policy:?}");
        match policy {
            UndoPolicy::BestEffort => {
                assert!(undone(&free, undo), "{policy:?}: {:?}", effect(&free, undo))
            }
            _ => {
                for object in [
                    TypedObjectId::TimeSignature(SIGNATURE),
                    TypedObjectId::Region(region),
                ] {
                    assert!(
                        held(&free, undo, object),
                        "{policy:?} {object:?}: {:?}",
                        effect(&free, undo)
                    );
                }
            }
        }
    }
}

/// D86 Q1: the grid on another region.
#[test]
fn an_undo_of_a_signature_another_region_s_grid_names_is_held() {
    a_signature_a_later_grid_names_is_held(|h| {
        h.grid(None, OTHER, edge(OTHER, AnchorOffset::Zero));
        OTHER
    });
}

/// D86 Q1: the grid on the signature's own region, at another position.
#[test]
fn an_undo_of_a_signature_its_region_s_grid_names_elsewhere_is_held() {
    a_signature_a_later_grid_names_is_held(|h| {
        h.grid(None, METRIC, edge(METRIC, bars(4)));
        METRIC
    });
}

/// Controls. The grid written in the same transaction is restored away by
/// its undo; one written after at the signature's own key loses its meter
/// there to the undo's restoration of that key, which is later; one written
/// after on a region the same undo removes goes with it; and a meter change
/// cleared at its key under a grid naming its signature tombstones no
/// signature. Each applies and leaves nothing naming a removed signature.
#[test]
fn an_undo_leaving_no_grid_naming_its_signature_applies() {
    let made = RegionId::new(AUTHOR, 13);
    for policy in POLICIES {
        for case in ["in the transaction", "at its key after", "on its region"] {
            let mut h = History::new();
            let transaction = h.transaction(500);
            h.signature(Some(transaction));
            match case {
                "in the transaction" => {
                    h.grid(Some(transaction), METRIC, edge(METRIC, AnchorOffset::Zero));
                }
                "at its key after" => {
                    h.grid(None, METRIC, edge(METRIC, AnchorOffset::Zero));
                }
                _ => {
                    h.region(
                        Some(transaction),
                        made,
                        valuegen::metric_model(),
                        3_000_000_000,
                    );
                    h.grid(None, made, edge(made, AnchorOffset::Zero));
                }
            }
            let undo = h.undo(transaction, policy);
            let (free, aware) = h.reduce();
            assert!(
                undone(&free, undo),
                "{policy:?} {case}: {:?}",
                effect(&free, undo)
            );
            assert!(!declares(&aware, SIGNATURE), "{policy:?} {case}");
        }
    }
    let mut h = History::new();
    h.signature(None);
    let grid = h.grid(None, METRIC, edge(METRIC, AnchorOffset::Zero));
    let clear = h.push(OperationKind::SetTimeSignature(SetTimeSignatureOp {
        region: METRIC,
        anchor: edge(METRIC, AnchorOffset::Zero),
        time_signature: None,
    }));
    let (free, aware) = h.reduce();
    assert!(applied(&free, grid) && applied(&free, clear));
    assert!(declares(&aware, SIGNATURE));
}

/// Each tempo map: the score's, and the metric region's own.
const MAPS: [Option<RegionId>; 2] = [None, Some(METRIC)];

fn refused(state: &MaterializedState, id: OperationId, reason: PreconditionFailureReason) -> bool {
    *effect(state, id)
        == OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction { reason },
        }
}

/// D86 Q2: a note a live tempo segment of either map starts or ends at is
/// not deleted, `TargetAnchored`, where before the delete applied and left
/// the segment naming nothing (`CrossCuttingRefsResolve`).
#[test]
fn a_note_a_tempo_segment_names_is_not_deleted() {
    for scope in MAPS {
        for ending in [false, true] {
            let mut h = History::new();
            h.note(None, LATER_NOTE, 1);
            let segment = if ending {
                h.tempo_ending(scope, at_note(LATER_NOTE))
            } else {
                h.tempo(None, scope, at_note(LATER_NOTE), true)
            };
            let delete = h.delete(LATER_NOTE);
            let (free, aware) = h.reduce();
            assert!(
                applied(&free, segment),
                "{scope:?} ending {ending}: {:?}",
                effect(&free, segment)
            );
            assert!(
                refused(&free, delete, PreconditionFailureReason::TargetAnchored),
                "{scope:?} ending {ending}: {:?}",
                effect(&free, delete)
            );
            assert!(aware.score.events.contains(LATER_NOTE), "{scope:?}");
        }
    }
}

/// D86 Q2's undo route: a note entered in a transaction, a live tempo
/// segment of either map starting at it since, and the transaction undone.
/// A strict or cascading undo conflicts, naming the note (and the region
/// whose map holds the segment), and a best-effort one keeps the note, where
/// before each removed it and left the segment naming nothing.
#[test]
fn an_undo_removing_a_note_a_tempo_segment_names_is_held() {
    for scope in MAPS {
        for policy in POLICIES {
            let mut h = History::new();
            let transaction = h.transaction(500);
            h.note(Some(transaction), LATER_NOTE, 1);
            h.tempo(None, scope, at_note(LATER_NOTE), true);
            let undo = h.undo(transaction, policy);
            let (free, aware) = h.reduce();
            assert!(
                aware.score.events.contains(LATER_NOTE),
                "{scope:?} {policy:?}"
            );
            match policy {
                UndoPolicy::BestEffort => assert!(undone(&free, undo), "{scope:?}"),
                _ => {
                    let holder =
                        scope.map_or(TypedObjectId::Event(LATER_NOTE), TypedObjectId::Region);
                    for object in [TypedObjectId::Event(LATER_NOTE), holder] {
                        assert!(
                            held(&free, undo, object),
                            "{scope:?} {policy:?} {object:?}: {:?}",
                            effect(&free, undo)
                        );
                    }
                }
            }
        }
    }
}

/// Controls: a note a tempo segment names no more, or never did, is removed.
/// A segment at the region's start, or one removed before the delete, holds
/// no note; one written in the undone transaction is restored away by its
/// undo, which removes the note; and one of a region's map, the region and
/// the note made in the undone transaction, goes with its region.
#[test]
fn a_note_no_live_tempo_segment_names_is_removed() {
    let made = RegionId::new(AUTHOR, 13);
    let made_note = EventId::new(AUTHOR, 990);
    for policy in POLICIES {
        let mut h = History::new();
        let transaction = h.transaction(500);
        h.region(
            Some(transaction),
            made,
            valuegen::metric_model(),
            3_000_000_000,
        );
        let instance = StaffInstanceId::new(AUTHOR, 924);
        let voice = VoiceId::new(AUTHOR, 944);
        h.push_in(
            Some(transaction),
            OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region: made,
                instance: valuegen::staff_instance(instance, STAVES[2]),
            }),
        );
        h.push_in(
            Some(transaction),
            OperationKind::CreateVoice(CreateVoiceOp {
                staff_instance: instance,
                voice: valuegen::voice(voice),
            }),
        );
        h.push_in(
            Some(transaction),
            OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event: valuegen::insert_event_value(
                    made_note,
                    voice,
                    MusicalPosition::origin(),
                    quarter(),
                    &[PitchId::new(AUTHOR, 991)],
                ),
            }),
        );
        let segment = h.tempo(None, Some(made), at_note(made_note), true);
        let undo = h.undo(transaction, policy);
        let (free, aware) = h.reduce();
        assert!(applied(&free, segment), "{policy:?}");
        assert!(undone(&free, undo), "{policy:?}: {:?}", effect(&free, undo));
        assert!(!aware.score.events.contains(made_note), "{policy:?}");
    }
    for scope in MAPS {
        for start in [edge(METRIC, AnchorOffset::Zero), at_note(NOTE)] {
            let mut h = History::new();
            h.tempo(None, scope, start.clone(), true);
            if matches!(start, TimeAnchor::Event { .. }) {
                h.tempo(None, scope, start, false);
            }
            let delete = h.delete(NOTE);
            let (free, aware) = h.reduce();
            assert!(
                applied(&free, delete),
                "{scope:?}: {:?}",
                effect(&free, delete)
            );
            assert!(!aware.score.events.contains(NOTE), "{scope:?}");
        }
        for policy in POLICIES {
            let mut h = History::new();
            let transaction = h.transaction(500);
            h.note(Some(transaction), LATER_NOTE, 1);
            h.tempo(Some(transaction), scope, at_note(LATER_NOTE), true);
            let undo = h.undo(transaction, policy);
            let (free, aware) = h.reduce();
            assert!(
                undone(&free, undo),
                "{scope:?} {policy:?}: {:?}",
                effect(&free, undo)
            );
            assert!(
                !aware.score.events.contains(LATER_NOTE),
                "{scope:?} {policy:?}"
            );
        }
    }
}

const CARRIER: RegionId = RegionId::new(AUTHOR, 916);

/// A metric region far away in time created carrying a local tempo map, one
/// segment at each of `starts` in turn; returns the create.
fn carrying(h: &mut History, starts: Vec<TimeAnchor>) -> OperationId {
    let mut region = valuegen::region(CARRIER);
    region.time_model = valuegen::metric_model();
    region.time_extent = TimeExtent {
        start: wall(7_000_000_000),
        end: wall(7_000_001_000),
    };
    region.local_tempo_map = Some(TempoMap {
        initial: None,
        segments: starts
            .into_iter()
            .map(|start| {
                let mut segment = valuegen::tempo_segment(CARRIER, MusicalPosition::origin(), 90.0);
                segment.start = start;
                segment
            })
            .collect(),
    });
    h.push(OperationKind::CreateRegion(CreateRegionOp { region }))
}

/// The carrier's own start, a key its other segment shares when that one
/// starts at a note, a measure or a region's edge.
fn carrier_start() -> TimeAnchor {
    edge(CARRIER, AnchorOffset::Zero)
}

/// C5b review 2's M1: a `CreateRegion` whose carried tempo map holds two
/// segments whose starts resolve to one position, the key `SetTempoSegment`
/// writes by, is refused `TempoMapMalformed` in both modes, as two writes at
/// that key would leave one segment. Before it the create applied, the tempo
/// chain seeded with the first segment alone while the graph kept both, so
/// the removal of what the second names applied and left it naming nothing
/// (`CrossCuttingRefsResolve`), whichever hold the removal met. Every route:
/// the note, the measure or the region the second segment names deleted or
/// undone under each policy, the carrier's own start the other segment; and
/// the order reversed, which before let the first segment hold its target,
/// refused alike, since the rule reads the key, not the order.
#[test]
fn a_region_carrying_two_tempo_segments_on_one_key_is_refused() {
    let own_start_first: Holder = |h, a| carrying(h, vec![carrier_start(), a]);
    let own_start_last: Holder = |h, a| carrying(h, vec![a, carrier_start()]);
    for (order, write) in [
        ("own start first", own_start_first),
        ("own start last", own_start_last),
    ] {
        for removal in removals() {
            let (h, created, removed, object) = removing(write, removal, false);
            let (free, aware) = h.reduce();
            assert!(
                refused(&free, created, PreconditionFailureReason::TempoMapMalformed),
                "{order} {removal:?}: {:?}",
                effect(&free, created)
            );
            assert!(
                undone(&free, removed),
                "{order} {removal:?}: {:?}",
                effect(&free, removed)
            );
            assert!(!live(&aware, object), "{order} {removal:?}");
        }
    }
}

/// M1's two notes: a region carrying segments at two notes of the metric
/// region, which key alike though the notes stand apart, is refused in both
/// orders, and the later note's delete then applies.
#[test]
fn a_region_carrying_tempo_segments_at_two_notes_is_refused() {
    for later_first in [false, true] {
        let mut h = History::new();
        h.note(None, LATER_NOTE, 1);
        let mut starts = vec![at_note(NOTE), at_note(LATER_NOTE)];
        if later_first {
            starts.reverse();
        }
        let created = carrying(&mut h, starts);
        let delete = h.delete(LATER_NOTE);
        let (free, aware) = h.reduce();
        assert!(
            refused(&free, created, PreconditionFailureReason::TempoMapMalformed),
            "later first {later_first}: {:?}",
            effect(&free, created)
        );
        assert!(applied(&free, delete), "later first {later_first}");
        assert!(!aware.score.events.contains(LATER_NOTE));
    }
}

/// Controls: a carried map whose segments keep distinct keys applies, the
/// carrier's own start and a bar into it, or a note and a bar into it; and
/// each segment then holds what it names against every removal, as a
/// segment `SetTempoSegment` writes does. Two `SetTempoSegment` writes at one
/// key leave the second segment alone, so the first's note is then free.
#[test]
fn a_region_carrying_tempo_segments_on_distinct_keys_applies_and_holds() {
    let mut h = History::new();
    let created = carrying(&mut h, vec![carrier_start(), edge(CARRIER, bars(1))]);
    let (free, aware) = h.reduce();
    assert!(applied(&free, created), "{:?}", effect(&free, created));
    let carried = aware
        .score
        .canvas
        .regions
        .iter()
        .find(|region| region.id == CARRIER)
        .and_then(|region| region.local_tempo_map.as_ref())
        .map(|map| map.segments.len());
    assert_eq!(carried, Some(2));
    let a_bar_in: Holder = |h, a| carrying(h, vec![a, edge(CARRIER, bars(1))]);
    for removal in removals() {
        let (h, created, removed, object) = removing(a_bar_in, removal, false);
        let (free, aware) = h.reduce();
        assert!(
            applied(&free, created),
            "{removal:?}: {:?}",
            effect(&free, created)
        );
        assert!(
            live(&aware, object),
            "{removal:?}: {:?}",
            effect(&free, removed)
        );
        match removal {
            Removal::NoteDeleted => assert!(
                refused(&free, removed, PreconditionFailureReason::TargetAnchored),
                "{:?}",
                effect(&free, removed)
            ),
            Removal::RegionDeleted => assert!(
                refused(&free, removed, PreconditionFailureReason::ContainerNotEmpty),
                "{:?}",
                effect(&free, removed)
            ),
            Removal::NoteUndone(UndoPolicy::BestEffort)
            | Removal::MeasureUndone(UndoPolicy::BestEffort)
            | Removal::RegionUndone(UndoPolicy::BestEffort) => assert!(
                undone(&free, removed),
                "{removal:?}: {:?}",
                effect(&free, removed)
            ),
            _ => assert!(
                held(&free, removed, object),
                "{removal:?}: {:?}",
                effect(&free, removed)
            ),
        }
    }
    let mut h = History::new();
    let first = h.tempo(None, Some(METRIC), at_note(NOTE), true);
    let second = h.tempo(None, Some(METRIC), edge(METRIC, AnchorOffset::Zero), true);
    let delete = h.delete(NOTE);
    let (free, aware) = h.reduce();
    assert!(applied(&free, first) && applied(&free, second));
    assert!(applied(&free, delete), "{:?}", effect(&free, delete));
    assert!(!aware.score.events.contains(NOTE));
}

/// A write of some holder at an anchor; returns the holder's write.
type Holder = fn(&mut History, TimeAnchor) -> OperationId;

/// The holders whose writes did not read their anchors' targets: a time
/// signature, a grid's meter change, a region's extent (whole, or its end
/// alone, its start elsewhere), a staff instance's carried clef, key and
/// grid, a region's own model, carried grid, break and tempo map, and a
/// migration's target model. Each names the fixture's signature where it
/// holds a meter change, declared first. The extent's end and the carried
/// tempo map are C5b review 2's L3: no test held either.
fn holders() -> Vec<(&'static str, Holder)> {
    fn region_with(h: &mut History, shape: impl FnOnce(&mut epiphany_core::Region)) -> OperationId {
        let id = RegionId::new(AUTHOR, 13);
        let mut region = valuegen::region(id);
        region.time_extent = TimeExtent {
            start: wall(3_000_000_000),
            end: wall(3_000_001_000),
        };
        shape(&mut region);
        h.push(OperationKind::CreateRegion(CreateRegionOp { region }))
    }
    fn instance_with(
        h: &mut History,
        shape: impl FnOnce(&mut epiphany_core::StaffInstance),
    ) -> OperationId {
        let mut instance = valuegen::staff_instance(StaffInstanceId::new(AUTHOR, 924), STAVES[2]);
        shape(&mut instance);
        h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: METRIC,
            instance,
        }))
    }
    fn meter(anchor: TimeAnchor) -> MeterChange {
        MeterChange {
            anchor,
            time_signature: SIGNATURE,
        }
    }
    fn model(anchor: TimeAnchor) -> RegionTimeModel {
        RegionTimeModel::Metric(epiphany_core::MetricTimeModel {
            meters: vec![meter(anchor)],
            ..Default::default()
        })
    }
    vec![
        ("time signature", |h, a| {
            h.push(OperationKind::SetTimeSignature(SetTimeSignatureOp {
                region: METRIC,
                anchor: a,
                time_signature: Some(valuegen::time_signature(
                    TimeSignatureId::new(AUTHOR, 971),
                    3,
                )),
            }))
        }),
        ("grid meter", |h, a| {
            h.signature(None);
            h.grid(None, OTHER, a)
        }),
        ("region extent", |h, a| {
            region_with(h, |r| {
                r.time_extent = TimeExtent {
                    start: a.clone(),
                    end: a,
                }
            })
        }),
        ("region extent's end", |h, a| {
            region_with(h, |r| r.time_extent.end = a)
        }),
        ("carried clef", |h, a| {
            instance_with(h, |i| {
                i.clef_sequence.push(epiphany_core::ClefChange {
                    anchor: a,
                    clef: epiphany_core::Clef::treble(),
                })
            })
        }),
        ("carried key", |h, a| {
            instance_with(h, |i| {
                i.key_sequence.push(epiphany_core::KeySignatureChange {
                    anchor: a,
                    key: epiphany_core::KeySignature::new(0).expect("C"),
                })
            })
        }),
        ("instance grid", |h, a| {
            h.signature(None);
            instance_with(h, |i| {
                i.local_metric_grid = Some(MetricGrid {
                    meter_sequence: vec![meter(a)],
                })
            })
        }),
        ("region's own meter", |h, a| {
            h.signature(None);
            region_with(h, |r| r.time_model = model(a))
        }),
        ("region's carried grid", |h, a| {
            h.signature(None);
            region_with(h, |r| {
                r.content
                    .staff_based_mut()
                    .expect("staff-based")
                    .default_metric_grid = Some(MetricGrid {
                    meter_sequence: vec![meter(a)],
                })
            })
        }),
        ("region's carried break", |h, a| {
            region_with(h, |r| {
                r.content
                    .staff_based_mut()
                    .expect("staff-based")
                    .user_system_breaks
                    .push(a)
            })
        }),
        ("region's carried tempo", |h, a| {
            region_with(h, |r| {
                let mut segment = valuegen::tempo_segment(r.id, MusicalPosition::origin(), 90.0);
                segment.start = a;
                r.local_tempo_map = Some(TempoMap {
                    initial: None,
                    segments: vec![segment],
                });
            })
        }),
        ("migration's target model", |h, a| {
            h.signature(None);
            h.push(OperationKind::ChangeRegionTimeModel(
                ChangeRegionTimeModelOp {
                    region: OTHER,
                    new_time_model: model(a),
                    declared_incompatible: Vec::new(),
                    remapping: PositionRemapping::PreserveTime,
                },
            ))
        }),
    ]
}

/// D87 Q2: each holder written at a note deleted before it, or at a region
/// deleted before it, is refused `TargetMissing` in both modes, as a tempo
/// segment, a measure, a user break and a cross-cutting structure are, where
/// before it applied naming nothing (C5b review 1's M4). Written at the live
/// note, each applies.
#[test]
fn a_holder_written_naming_a_target_that_is_not_live_is_refused() {
    let gone = RegionId::new(AUTHOR, 14);
    for (name, write) in holders() {
        let mut h = History::new();
        h.delete(NOTE);
        let dead = write(&mut h, at_note(NOTE));
        let (free, _) = h.reduce();
        assert!(
            refused(&free, dead, PreconditionFailureReason::TargetMissing),
            "{name} at a deleted note: {:?}",
            effect(&free, dead)
        );

        let mut h = History::new();
        h.region(None, gone, valuegen::metric_model(), 4_000_000_000);
        h.push(OperationKind::DeleteRegion(epiphany_ops::DeleteRegionOp {
            region: gone,
        }));
        let dead = write(&mut h, edge(gone, AnchorOffset::Zero));
        let (free, _) = h.reduce();
        assert!(
            refused(&free, dead, PreconditionFailureReason::TargetMissing),
            "{name} at a deleted region: {:?}",
            effect(&free, dead)
        );

        let mut h = History::new();
        let live = write(&mut h, at_note(NOTE));
        let (free, _) = h.reduce();
        assert!(
            applied(&free, live),
            "{name} at the live note: {:?}",
            effect(&free, live)
        );
    }
}

/// The holders a removal is held for (D87 Q1): `holders()` but a user
/// break, which stays keyed and draws nothing (D80), and a measure's start.
fn held_holders() -> Vec<(&'static str, Holder)> {
    let mut held: Vec<(&'static str, Holder)> = holders()
        .into_iter()
        .filter(|(name, _)| !name.contains("break"))
        .collect();
    held.push(("measure start", |h, a| {
        let instance = StaffInstanceId::new(AUTHOR, 925);
        h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: METRIC,
            instance: valuegen::staff_instance(instance, STAVES[1]),
        }));
        h.push(OperationKind::CreateMeasure(CreateMeasureOp {
            instance,
            measure: Measure {
                id: MeasureId::new(AUTHOR, 935),
                start: a,
                time_signature: None,
                explicit_number: None,
                number_visibility: MeasureNumberVisibility::Auto,
            },
        }))
    }));
    held
}

const SPARE_MEASURE: MeasureId = MeasureId::new(AUTHOR, 931);
const EMPTY: RegionId = RegionId::new(AUTHOR, 915);

/// What a history removes, and how.
#[derive(Clone, Copy, Debug)]
enum Removal {
    NoteDeleted,
    NoteUndone(UndoPolicy),
    MeasureUndone(UndoPolicy),
    RegionDeleted,
    RegionUndone(UndoPolicy),
}

fn removals() -> Vec<Removal> {
    let mut all = vec![Removal::NoteDeleted, Removal::RegionDeleted];
    for policy in POLICIES {
        all.extend([
            Removal::NoteUndone(policy),
            Removal::MeasureUndone(policy),
            Removal::RegionUndone(policy),
        ]);
    }
    all
}

/// A history in which `write` names what `removal` then removes, written
/// outside the removing transaction unless `inside`; the write's and the
/// removal's ids and the removed object.
fn removing(
    write: Holder,
    removal: Removal,
    inside: bool,
) -> (History, OperationId, OperationId, TypedObjectId) {
    let mut h = History::new();
    if inside {
        // The signature a holder names, declared before the transaction, so
        // the undo removes no signature; the holder's own declaration of it
        // in the transaction finds it already there.
        h.signature(None);
    }
    let transaction = match removal {
        Removal::NoteDeleted | Removal::RegionDeleted => None,
        _ => Some(h.transaction(500)),
    };
    let (anchor, object) = match removal {
        Removal::NoteDeleted => (at_note(NOTE), TypedObjectId::Event(NOTE)),
        Removal::NoteUndone(_) => {
            h.note(transaction, LATER_NOTE, 1);
            (at_note(LATER_NOTE), TypedObjectId::Event(LATER_NOTE))
        }
        Removal::MeasureUndone(_) => {
            h.push_in(
                transaction,
                OperationKind::CreateMeasure(CreateMeasureOp {
                    instance: INSTANCE,
                    measure: Measure {
                        id: SPARE_MEASURE,
                        start: edge(METRIC, bars(1)),
                        time_signature: None,
                        explicit_number: None,
                        number_visibility: MeasureNumberVisibility::Auto,
                    },
                }),
            );
            (
                TimeAnchor::Measure {
                    id: SPARE_MEASURE,
                    position: epiphany_core::MeasurePosition::Start,
                    offset: AnchorOffset::Zero,
                },
                TypedObjectId::Measure(SPARE_MEASURE),
            )
        }
        Removal::RegionDeleted | Removal::RegionUndone(_) => {
            h.region(transaction, EMPTY, valuegen::metric_model(), 5_000_000_000);
            (
                edge(EMPTY, AnchorOffset::Zero),
                TypedObjectId::Region(EMPTY),
            )
        }
    };
    if inside {
        // The holder's write in the removing transaction: its envelopes are
        // the transaction's members.
        let before = h.envelopes.len();
        let written = write(&mut h, anchor);
        for envelope in &mut h.envelopes[before..] {
            envelope.transaction = transaction;
        }
        let removed = undo_or_delete(&mut h, removal, transaction);
        return (h, written, removed, object);
    }
    let written = write(&mut h, anchor);
    let removed = undo_or_delete(&mut h, removal, transaction);
    (h, written, removed, object)
}

fn undo_or_delete(
    h: &mut History,
    removal: Removal,
    transaction: Option<TransactionId>,
) -> OperationId {
    match removal {
        Removal::NoteDeleted => h.delete(NOTE),
        Removal::RegionDeleted => {
            h.push(OperationKind::DeleteRegion(epiphany_ops::DeleteRegionOp {
                region: EMPTY,
            }))
        }
        Removal::NoteUndone(policy)
        | Removal::MeasureUndone(policy)
        | Removal::RegionUndone(policy) => h.undo(transaction.expect("a transaction"), policy),
    }
}

fn live(aware: &GraphMaterialization, object: TypedObjectId) -> bool {
    match object {
        TypedObjectId::Event(id) => aware.score.events.contains(id),
        TypedObjectId::Region(id) => aware.score.canvas.regions.iter().any(|r| r.id == id),
        TypedObjectId::Measure(id) => aware
            .score
            .staff_instances()
            .any(|(_, si)| si.measures.iter().any(|m| m.id == id)),
        _ => unreachable!("a removed object"),
    }
}

/// D87 Q1: every holder outside the cross-cutting structures holds the note,
/// measure or region it names. A note's delete is refused `TargetAnchored`
/// and a region's `ContainerNotEmpty`; a strict or cascading undo removing
/// one conflicts, naming it, and a best-effort one keeps it; each object
/// stays. Before it each removal applied and left the holder's anchor naming
/// nothing, which the checker did not read. A time signature or a grid at a
/// measure is not written (its write is refused), and holds nothing.
#[test]
fn every_holder_holds_what_it_names_against_a_delete_and_an_undo() {
    for (name, write) in held_holders() {
        for removal in removals() {
            let (h, written, removed, object) = removing(write, removal, false);
            let (free, aware) = h.reduce();
            if !applied(&free, written) {
                assert!(
                    matches!(removal, Removal::MeasureUndone(_))
                        && ["time signature", "grid meter"].contains(&name),
                    "{name} {removal:?}: the write {:?}",
                    effect(&free, written)
                );
                continue;
            }
            assert!(
                live(&aware, object),
                "{name} {removal:?}: {:?}",
                effect(&free, removed)
            );
            match removal {
                Removal::NoteDeleted => assert!(
                    refused(&free, removed, PreconditionFailureReason::TargetAnchored),
                    "{name}: {:?}",
                    effect(&free, removed)
                ),
                Removal::RegionDeleted => assert!(
                    refused(&free, removed, PreconditionFailureReason::ContainerNotEmpty),
                    "{name}: {:?}",
                    effect(&free, removed)
                ),
                Removal::NoteUndone(UndoPolicy::BestEffort)
                | Removal::MeasureUndone(UndoPolicy::BestEffort)
                | Removal::RegionUndone(UndoPolicy::BestEffort) => assert!(
                    undone(&free, removed),
                    "{name} {removal:?}: {:?}",
                    effect(&free, removed)
                ),
                _ => assert!(
                    held(&free, removed, object),
                    "{name} {removal:?}: {:?}",
                    effect(&free, removed)
                ),
            }
        }
    }
}

/// Controls: a holder written in the removing transaction goes with its
/// undo, and the object is removed, but a migration's target model, which no
/// undo restores (D84 Q3), still names it and holds it; one the removed
/// region is itself (its own
/// model's meter change at its own start) holds nothing; one rewritten off
/// the note before the delete holds nothing; and a user break at the note is
/// excepted (D80).
#[test]
fn a_holder_that_names_nothing_the_removal_takes_holds_nothing() {
    for (name, write) in held_holders() {
        for removal in removals() {
            // A time signature or a grid at a measure is refused, and its
            // transaction with it.
            let refused_write = matches!(removal, Removal::MeasureUndone(_))
                && ["time signature", "grid meter"].contains(&name);
            if refused_write || matches!(removal, Removal::NoteDeleted | Removal::RegionDeleted) {
                continue;
            }
            let (h, _, removed, object) = removing(write, removal, true);
            let (free, aware) = h.reduce();
            // A region's carried tempo map naming a region the same undo
            // removes still holds it: a strict or cascading undo conflicts
            // and a best-effort one keeps the region. The undo's region arm
            // reads a tempo segment as a region's delete does (reduction
            // version 3), excepting the removed region's own map but not a
            // map whose region the undo removes too. Conservative, nothing
            // left naming nothing, and older than X5b.
            if name == "region's carried tempo" && matches!(removal, Removal::RegionUndone(_)) {
                assert!(
                    live(&aware, object),
                    "{name} {removal:?}: {:?}",
                    effect(&free, removed)
                );
                continue;
            }
            if name == "migration's target model" {
                let kept = match removal {
                    Removal::NoteUndone(UndoPolicy::BestEffort)
                    | Removal::MeasureUndone(UndoPolicy::BestEffort)
                    | Removal::RegionUndone(UndoPolicy::BestEffort) => undone(&free, removed),
                    _ => held(&free, removed, object),
                };
                assert!(
                    kept && live(&aware, object),
                    "{name} {removal:?}: {:?}",
                    effect(&free, removed)
                );
                continue;
            }
            assert!(
                undone(&free, removed),
                "{name} {removal:?} in the transaction: {:?}",
                effect(&free, removed)
            );
            assert!(
                !live(&aware, object),
                "{name} {removal:?} in the transaction"
            );
        }
    }
    // Its own model's meter change at its own start.
    let mut h = History::new();
    h.signature(None);
    let own = RegionId::new(AUTHOR, 13);
    let mut region = valuegen::region(own);
    region.time_model = RegionTimeModel::Metric(epiphany_core::MetricTimeModel {
        meters: vec![MeterChange {
            anchor: edge(own, AnchorOffset::Zero),
            time_signature: SIGNATURE,
        }],
        ..Default::default()
    });
    region.time_extent = TimeExtent {
        start: wall(3_000_000_000),
        end: wall(3_000_001_000),
    };
    h.push(OperationKind::CreateRegion(CreateRegionOp { region }));
    let delete = h.push(OperationKind::DeleteRegion(epiphany_ops::DeleteRegionOp {
        region: own,
    }));
    let (free, _) = h.reduce();
    assert!(applied(&free, delete), "{:?}", effect(&free, delete));
    // A grid rewritten off the note, and a break at it.
    let mut h = History::new();
    h.signature(None);
    h.grid(None, OTHER, at_note(NOTE));
    h.push(OperationKind::SetMetricGrid(SetMetricGridOp {
        region: OTHER,
        grid: None,
    }));
    h.push(OperationKind::SetUserSystemBreak(
        epiphany_ops::SetUserSystemBreakOp {
            region: METRIC,
            anchor: at_note(NOTE),
            present: true,
        },
    ));
    let delete = h.delete(NOTE);
    let (free, _) = h.reduce();
    assert!(applied(&free, delete), "{:?}", effect(&free, delete));
    // A carried clef a `SetClef` has since replaced: the change it writes is
    // anchored at its region's start, so it names the note no more.
    let mut h = History::new();
    let instance = StaffInstanceId::new(AUTHOR, 924);
    let mut carrying = valuegen::staff_instance(instance, STAVES[2]);
    carrying.clef_sequence.push(epiphany_core::ClefChange {
        anchor: at_note(NOTE),
        clef: epiphany_core::Clef::treble(),
    });
    h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
        region: METRIC,
        instance: carrying,
    }));
    let replaced = h.push(OperationKind::SetClef(epiphany_ops::SetClefOp {
        instance,
        offset: RationalTime::new(0, 1).expect("the start"),
        clef: Some(epiphany_core::Clef::bass()),
    }));
    let delete = h.delete(NOTE);
    let (free, _) = h.reduce();
    assert!(applied(&free, replaced), "{:?}", effect(&free, replaced));
    assert!(applied(&free, delete), "{:?}", effect(&free, delete));
}

fn tuplet(
    n: u64,
    members: Vec<EventId>,
    parent: Option<u64>,
    quarters: i64,
) -> epiphany_core::Tuplet {
    epiphany_core::Tuplet {
        id: epiphany_core::TupletId::new(AUTHOR, n),
        ratio: epiphany_core::TupletRatio::new(3, 2).expect("3:2"),
        members,
        parent: parent.map(|p| epiphany_core::TupletId::new(AUTHOR, p)),
        required_total: MusicalDuration(RationalTime::new(quarters, 4).expect("quarters")),
        display: epiphany_core::TupletDisplay::default(),
    }
}

fn create_tuplet(
    h: &mut History,
    transaction: Option<TransactionId>,
    value: epiphany_core::Tuplet,
) -> OperationId {
    h.push_in(
        transaction,
        OperationKind::CreateTuplet(epiphany_ops::CreateTupletOp { tuplet: value }),
    )
}

fn tuplets(aware: &GraphMaterialization) -> Vec<u64> {
    let mut ids: Vec<u64> = aware
        .score
        .cross_cutting
        .tuplets
        .iter()
        .map(|t| t.id.counter())
        .collect();
    ids.sort_unstable();
    ids
}

/// D87 Q3: a tuplet created in a transaction, a tuplet nested in it created
/// after, and the transaction undone. A strict or cascading undo conflicts,
/// naming the parent and the child, and a best-effort one keeps the parent,
/// where before each removed it and left the child naming nothing
/// (`CrossCuttingRefsResolve`). A child made in the same transaction goes
/// with its parent.
#[test]
fn an_undo_of_a_tuplet_a_nested_tuplet_names_is_held() {
    let parent = TypedObjectId::Tuplet(epiphany_core::TupletId::new(AUTHOR, 70));
    let child = TypedObjectId::Tuplet(epiphany_core::TupletId::new(AUTHOR, 71));
    for policy in POLICIES {
        for inside in [false, true] {
            let mut h = History::new();
            let transaction = h.transaction(500);
            create_tuplet(&mut h, Some(transaction), tuplet(70, vec![NOTE], None, 1));
            let nested = create_tuplet(
                &mut h,
                inside.then_some(transaction),
                tuplet(71, vec![NOTE], Some(70), 1),
            );
            let undo = h.undo(transaction, policy);
            let (free, aware) = h.reduce();
            if inside {
                assert!(
                    undone(&free, undo),
                    "{policy:?} inside: {:?}",
                    effect(&free, undo)
                );
                assert!(tuplets(&aware).is_empty(), "{policy:?} inside");
                continue;
            }
            assert!(applied(&free, nested), "{policy:?}");
            assert_eq!(
                tuplets(&aware),
                [70, 71],
                "{policy:?}: {:?}",
                effect(&free, undo)
            );
            match policy {
                UndoPolicy::BestEffort => assert!(undone(&free, undo)),
                _ => {
                    for object in [parent, child] {
                        assert!(
                            held(&free, undo, object),
                            "{policy:?} {object:?}: {:?}",
                            effect(&free, undo)
                        );
                    }
                }
            }
        }
    }
}

/// D87 Q3: a member's cascade carries the children of the tuplets it
/// removes, and theirs. A tuplet over two notes, one nested in it over the
/// second, and one nested in that; the first note deleted with its tuplet
/// cascaded removes all three, the two nested ones `CascadeDeleted`, where
/// before the children stayed naming their removed parent. An undo removing
/// the first note, which cascades its tuplet, carries them likewise. With no
/// tuplet nested, the cascade removes the one it names alone.
#[test]
fn a_member_s_cascade_carries_the_children_of_the_tuplet_it_removes() {
    use epiphany_ops::{RepairKind, TupletCompensation};
    let cascade = |h: &mut History| {
        h.push(OperationKind::DeleteEvent(DeleteEventOp {
            event: NOTE,
            tuplet_compensation: TupletCompensation::CascadeDeleteTuplets {
                tuplets: vec![epiphany_core::TupletId::new(AUTHOR, 70)],
            },
        }))
    };
    for nested in [true, false] {
        let mut h = History::new();
        h.note(None, LATER_NOTE, 1);
        create_tuplet(&mut h, None, tuplet(70, vec![NOTE, LATER_NOTE], None, 2));
        if nested {
            create_tuplet(&mut h, None, tuplet(71, vec![LATER_NOTE], Some(70), 1));
            create_tuplet(&mut h, None, tuplet(72, vec![LATER_NOTE], Some(71), 1));
        }
        let delete = cascade(&mut h);
        let (free, aware) = h.reduce();
        let OperationEffect::AppliedWithRepair { repairs } = effect(&free, delete) else {
            panic!("nested {nested}: {:?}", effect(&free, delete));
        };
        let carried: Vec<u64> = repairs
            .iter()
            .filter(|r| r.kind == RepairKind::CascadeDeleted)
            .filter_map(|r| match r.target {
                TypedObjectId::Tuplet(id) => Some(id.counter()),
                _ => None,
            })
            .collect();
        if nested {
            assert_eq!(carried, [71, 72], "{repairs:?}");
            for n in [71, 72] {
                assert!(matches!(
                    free.objects
                        .get(&TypedObjectId::Tuplet(epiphany_core::TupletId::new(
                            AUTHOR, n
                        ))),
                    Some(epiphany_ops::ObjectState::Tombstoned { .. })
                ));
            }
        } else {
            assert!(carried.is_empty(), "{repairs:?}");
        }
        assert!(tuplets(&aware).is_empty(), "nested {nested}");
    }
    for policy in POLICIES {
        let mut h = History::new();
        let transaction = h.transaction(500);
        h.note(Some(transaction), LATER_NOTE, 1);
        create_tuplet(&mut h, None, tuplet(70, vec![NOTE, LATER_NOTE], None, 2));
        create_tuplet(&mut h, None, tuplet(71, vec![NOTE], Some(70), 1));
        let undo = h.undo(transaction, policy);
        let (free, aware) = h.reduce();
        let OperationEffect::AppliedWithRepair { repairs } = effect(&free, undo) else {
            panic!("{policy:?}: {:?}", effect(&free, undo));
        };
        // The ledger takes the child as the graph does, each mode alike.
        let nested = TypedObjectId::Tuplet(epiphany_core::TupletId::new(AUTHOR, 71));
        assert!(
            repairs
                .iter()
                .any(|r| r.kind == RepairKind::CascadeDeleted && r.target == nested),
            "{policy:?}: {repairs:?}"
        );
        assert!(
            matches!(
                free.objects.get(&nested),
                Some(epiphany_ops::ObjectState::Tombstoned { .. })
            ),
            "{policy:?}"
        );
        assert!(tuplets(&aware).is_empty(), "{policy:?}");
    }
}
