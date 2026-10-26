//! What a removal leaves naming nothing is held (reduction version 5, C5b
//! review 1, D86 and D87): graph invariant 10 says every graph reference
//! resolves, and an object a live holder names is not removed from under it.
//! An undo that would remove a time signature a live region's grid names, as
//! the grid's chains give it once the undo's restorations apply, is held as
//! for a meter change at a key (D86 Q1): a strict or cascading undo
//! conflicts, and a best-effort one keeps the signature.
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
    StaffId, StaffInstanceId, TimeAnchor, TimeExtent, TimeSignatureId, TransactionId,
    TypedObjectId, VoiceId, WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, ConflictKind, CreateInstrumentOp, CreateMeasureOp,
    CreateRegionOp, CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp, GraphMaterialization,
    HybridLogicalClock, InsertEventOp, MaterializedState, OperationEffect, OperationEnvelope,
    OperationKind, OperationPayload, OperationSet, OperationStamp, SetMetricGridOp,
    SetTimeSignatureOp, TransactionDescriptor, UndoPolicy, UndoTransactionPayload,
};

const AUTHOR: ReplicaId = ReplicaId(5);
const METRIC: RegionId = RegionId::new(AUTHOR, 910);
const OTHER: RegionId = RegionId::new(AUTHOR, 911);
const INSTANCE: StaffInstanceId = StaffInstanceId::new(AUTHOR, 920);
const MEASURE: MeasureId = MeasureId::new(AUTHOR, 930);
const VOICE: VoiceId = VoiceId::new(AUTHOR, 940);
const NOTE: EventId = EventId::new(AUTHOR, 950);
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
        h.push(OperationKind::InsertEvent(InsertEventOp {
            staff_instance: INSTANCE,
            event: valuegen::insert_event_value(
                NOTE,
                VOICE,
                MusicalPosition::origin(),
                quarter(),
                &[PitchId::new(AUTHOR, NOTE.counter() + 1)],
            ),
        }));
        h
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
