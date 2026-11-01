//! The region refusal (reduction version 5, X5b): a `CreateStaffInstance` whose
//! region overlaps in time another live region holding a live instance of its
//! staff is refused `RegionsOverlap`, in base-free and graph-aware reduction
//! alike, so `RegionExtents` holds for every history. The extents are compared
//! as the ledger places them: a wall-clock anchor is its time, a region's edge
//! is that region's edge plus a wall-clock offset, and anything else (an event,
//! a measure, a musical offset) counts as overlapping.
//!
//! Each history here is one author's, built by hand: an instrument and two
//! staves, then regions and their fills. Each is reduced both ways, the two
//! held to each other, the graph checked, and the verdict the history exists
//! for asserted.

use epiphany_core::{
    check_invariants, AnchorOffset, EventId, IdentityContext, InstrumentId, MusicalDuration,
    MusicalPosition, OperationId, PitchId, RationalTime, RegionEdge, RegionId, ReplicaId, Score,
    StaffId, StaffInstanceId, TimeAnchor, TimeExtent, TransactionId, VoiceId, WallClockDuration,
    WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, CreateInstrumentOp, CreateRegionOp, CreateStaffInstanceOp,
    CreateStaffOp, CreateVoiceOp, DeleteRegionOp, DeleteStaffInstanceOp, GraphMaterialization,
    HybridLogicalClock, InsertEventOp, MaterializedState, NoOpReason, OperationEffect,
    OperationEnvelope, OperationKind, OperationPayload, OperationSet, OperationStamp,
    PreconditionFailureReason, TransactionDescriptor,
};

const AUTHOR: ReplicaId = ReplicaId(5);

/// One author's history, each envelope seeing every one before it.
struct History {
    envelopes: Vec<OperationEnvelope>,
    next: u64,
    staves: [StaffId; 2],
}

impl History {
    /// An instrument and two staves.
    fn new() -> Self {
        let mut history = History {
            envelopes: Vec::new(),
            next: 0,
            staves: [StaffId::new(AUTHOR, 901), StaffId::new(AUTHOR, 902)],
        };
        let instrument = InstrumentId::new(AUTHOR, 900);
        history.push(
            None,
            OperationKind::CreateInstrument(CreateInstrumentOp {
                instrument: valuegen::instrument(instrument),
            }),
        );
        for staff in history.staves {
            history.push(
                None,
                OperationKind::CreateStaff(CreateStaffOp {
                    staff: valuegen::staff(staff, instrument),
                }),
            );
        }
        history
    }

    fn push(&mut self, transaction: Option<TransactionId>, kind: OperationKind) -> OperationId {
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
            payload: OperationPayload::Primitive(kind),
        });
        id
    }

    /// A region of `extent`, `n` its counter.
    fn region(&mut self, n: u64, start: TimeAnchor, end: TimeAnchor) -> RegionId {
        let id = RegionId::new(AUTHOR, n);
        let mut region = valuegen::region(id);
        region.time_extent = TimeExtent { start, end };
        self.push(None, OperationKind::CreateRegion(CreateRegionOp { region }));
        id
    }

    /// A fill of `region` with an instance of staff `s`, `n` its counter.
    fn fill(&mut self, n: u64, region: RegionId, s: usize) -> OperationId {
        let instance = valuegen::staff_instance(StaffInstanceId::new(AUTHOR, n), self.staves[s]);
        self.push(
            None,
            OperationKind::CreateStaffInstance(CreateStaffInstanceOp { region, instance }),
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

fn edge(region: RegionId, edge: RegionEdge, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge,
        offset,
    }
}

fn effect(state: &MaterializedState, id: OperationId) -> &OperationEffect {
    &state
        .effects
        .iter()
        .find(|(e, _)| *e == id)
        .expect("reduced")
        .1
}

fn refused(state: &MaterializedState, id: OperationId) -> bool {
    *effect(state, id)
        == OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::RegionsOverlap,
            },
        }
}

fn applied(state: &MaterializedState, id: OperationId) -> bool {
    matches!(effect(state, id), OperationEffect::Applied)
}

/// Two regions over one span, each filled with the same staff: the later fill
/// in canonical order is refused, the earlier stands, and the graph holds one
/// instance of the staff in that span. Version 4 applied both, and the graph
/// broke `RegionExtents`.
#[test]
fn a_fill_overlapping_a_region_that_holds_its_staff_is_refused() {
    let mut h = History::new();
    let a = h.region(10, wall(0), wall(1000));
    let b = h.region(11, wall(500), wall(1500));
    let first = h.fill(20, a, 0);
    let second = h.fill(21, b, 0);
    let (free, aware) = h.reduce();
    assert!(applied(&free, first));
    assert!(refused(&free, second));
    let staves: Vec<usize> = aware
        .score
        .canvas
        .regions
        .iter()
        .map(|r| r.staff_instances().len())
        .collect();
    assert_eq!(staves, [1, 0]);
}

/// Extents that touch do not overlap (`[0, 1000)` and `[1000, 2000)`), and a
/// region over the same span holding another staff overlaps nothing it holds:
/// both fills apply.
#[test]
fn touching_extents_and_other_staves_are_admitted() {
    let mut h = History::new();
    let a = h.region(10, wall(0), wall(1000));
    let b = h.region(11, wall(1000), wall(2000));
    let c = h.region(12, wall(0), wall(1000));
    let fills = [h.fill(20, a, 0), h.fill(21, b, 0), h.fill(22, c, 1)];
    let (free, _) = h.reduce();
    assert!(fills.iter().all(|id| applied(&free, *id)));
}

/// A region whose extent is another region's edge plus a wall-clock offset is
/// placed through the ledger's index of extents: one starting at the first's
/// end is admitted, one starting inside it refused. The first region is
/// proportional, so a wall-clock offset from its edge agrees with its time
/// model (invariant 9).
#[test]
fn an_extent_anchored_to_a_region_s_edge_is_placed_through_the_index() {
    let mut h = History::new();
    let a = RegionId::new(AUTHOR, 10);
    let mut region = valuegen::region(a);
    region.time_model = valuegen::proportional_model();
    region.time_extent = TimeExtent {
        start: wall(0),
        end: wall(1000),
    };
    h.push(None, OperationKind::CreateRegion(CreateRegionOp { region }));
    let after = h.region(
        11,
        edge(a, RegionEdge::End, AnchorOffset::Zero),
        edge(
            a,
            RegionEdge::End,
            AnchorOffset::WallClock(WallClockDuration(500)),
        ),
    );
    let inside = h.region(
        12,
        edge(
            a,
            RegionEdge::Start,
            AnchorOffset::WallClock(WallClockDuration(200)),
        ),
        edge(
            a,
            RegionEdge::Start,
            AnchorOffset::WallClock(WallClockDuration(700)),
        ),
    );
    let first = h.fill(20, a, 0);
    let later = h.fill(21, after, 0);
    let within = h.fill(22, inside, 0);
    let (free, _) = h.reduce();
    assert!(applied(&free, first));
    assert!(applied(&free, later));
    assert!(refused(&free, within));
}

/// An extent the ledger cannot place counts as overlapping: a region anchored
/// to an event, or by a musical offset, is refused a staff another live
/// region holds, however far away it would stand once a tempo map placed it;
/// with no other region holding the staff, it is filled. The event is made
/// first, as an extent names only a live target (D87 Q2).
#[test]
fn an_extent_the_ledger_cannot_place_counts_as_overlapping() {
    let mut h = History::new();
    let a = h.region(10, wall(0), wall(1000));
    let first = h.fill(20, a, 0);
    let voice = VoiceId::new(AUTHOR, 30);
    let note = EventId::new(AUTHOR, 999);
    h.push(
        None,
        OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: StaffInstanceId::new(AUTHOR, 20),
            voice: valuegen::voice(voice),
        }),
    );
    h.push(
        None,
        OperationKind::InsertEvent(InsertEventOp {
            staff_instance: StaffInstanceId::new(AUTHOR, 20),
            event: valuegen::insert_event_value(
                note,
                voice,
                MusicalPosition::origin(),
                MusicalDuration(RationalTime::new(1, 4).expect("a quarter")),
                &[PitchId::new(AUTHOR, 1000)],
            ),
        }),
    );
    let by_event = h.region(
        11,
        TimeAnchor::Event {
            id: note,
            offset: AnchorOffset::Zero,
        },
        wall(9_000_000),
    );
    let musical = h.region(
        12,
        edge(
            a,
            RegionEdge::End,
            AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(100))),
        ),
        wall(9_000_000),
    );
    let unplaced = [h.fill(21, by_event, 0), h.fill(22, musical, 0)];
    let alone = h.fill(23, by_event, 1);
    let (free, _) = h.reduce();
    assert!(applied(&free, first));
    assert!(unplaced.iter().all(|id| refused(&free, *id)));
    assert!(applied(&free, alone));
}

/// A refused fill in a transaction conflicts the transaction whole: the
/// region it created with the fill goes with it, and nothing is repaired.
#[test]
fn a_refused_fill_conflicts_its_transaction_whole() {
    let mut h = History::new();
    let a = h.region(10, wall(0), wall(1000));
    let first = h.fill(20, a, 0);
    let tx = TransactionId::new(AUTHOR, 30);
    h.push(
        Some(tx),
        OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("a passage"),
            category: None,
        }),
    );
    let b = RegionId::new(AUTHOR, 31);
    let mut region = valuegen::region(b);
    region.time_extent = TimeExtent {
        start: wall(500),
        end: wall(1500),
    };
    let create = h.push(
        Some(tx),
        OperationKind::CreateRegion(CreateRegionOp { region }),
    );
    let fill = h.push(
        Some(tx),
        OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: b,
            instance: valuegen::staff_instance(StaffInstanceId::new(AUTHOR, 32), h.staves[0]),
        }),
    );
    let (free, aware) = h.reduce();
    assert!(applied(&free, first));
    for id in [create, fill] {
        assert_eq!(
            *effect(&free, id),
            OperationEffect::NoOp {
                reason: NoOpReason::TransactionConflict
            }
        );
    }
    assert_eq!(aware.score.canvas.regions.len(), 1, "the region went too");
}

/// Only live regions and live instances count: a region emptied and deleted,
/// or an instance deleted, leaves its span to a later fill.
#[test]
fn a_removed_region_or_instance_leaves_its_span() {
    let mut h = History::new();
    let a = h.region(10, wall(0), wall(1000));
    let first = h.fill(20, a, 0);
    h.push(
        None,
        OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
            staff_instance: StaffInstanceId::new(AUTHOR, 20),
        }),
    );
    let b = h.region(11, wall(0), wall(1000));
    let second = h.fill(21, b, 0);
    let c = h.region(12, wall(2000), wall(3000));
    let third = h.fill(22, c, 1);
    h.push(
        None,
        OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
            staff_instance: StaffInstanceId::new(AUTHOR, 22),
        }),
    );
    h.push(
        None,
        OperationKind::DeleteRegion(DeleteRegionOp { region: c }),
    );
    let d = h.region(13, wall(2000), wall(3000));
    let fourth = h.fill(23, d, 1);
    let (free, _) = h.reduce();
    for id in [first, second, third, fourth] {
        assert!(applied(&free, id), "{id:?}: {:?}", effect(&free, id));
    }
}
