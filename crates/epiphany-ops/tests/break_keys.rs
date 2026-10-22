//! User breaks keyed by their anchors (reduction version 5, X5b.4): a break's
//! last-writer slot is its anchor itself, so two breaks anchored to different
//! measures or events of one region keep their own slots, and a break
//! anchored to an event keeps its slot when the event moves. A break naming an
//! event or a measure its region does not hold live is refused
//! `TargetMissing`; one whose event a later delete removes stays keyed.
//!
//! Each history is one author's, built by hand: an instrument, two staves, a
//! metric region with an instance of each, a measure of four quarters on each
//! instance at the region's start and another at four quarters, a voice on
//! the first instance and two notes in it. Each is reduced both ways and held
//! to agreement, the graph checked, and the verdict asserted.

use epiphany_core::{
    check_invariants, AnchorOffset, EventId, EventPosition, IdentityContext, InstrumentId, Measure,
    MeasureId, MeasureNumberVisibility, MeasurePosition, MusicalDuration, MusicalPosition,
    OperationId, RationalTime, RegionId, ReplicaId, Score, StaffId, StaffInstanceId, TimeAnchor,
    VoiceId, WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, BreakAnchor, CausalContext, CreateInstrumentOp, CreateMeasureOp,
    CreateRegionOp, CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp, DeleteEventOp,
    GraphMaterialization, HybridLogicalClock, InsertEventOp, MaterializedState, ModifyEventOp,
    NoOpReason, OperationEffect, OperationEnvelope, OperationKind, OperationPayload, OperationSet,
    OperationStamp, PreconditionFailureReason, SetUserPageBreakOp, SetUserSystemBreakOp,
    TupletCompensation,
};

const AUTHOR: ReplicaId = ReplicaId(5);

fn at(quarters: i64) -> MusicalPosition {
    MusicalPosition(RationalTime::new(quarters, 4).expect("a position"))
}

fn start_of(id: MeasureId) -> TimeAnchor {
    TimeAnchor::Measure {
        id,
        position: MeasurePosition::Start,
        offset: AnchorOffset::Zero,
    }
}

/// The scaffold's objects, and one author's history over them, each envelope
/// seeing every one before it.
struct History {
    envelopes: Vec<OperationEnvelope>,
    next: u64,
    region: RegionId,
    other_region: RegionId,
    /// Each instance's measures: at the region's start, and at four quarters.
    measures: [[MeasureId; 2]; 2],
    notes: [EventId; 2],
    voice: VoiceId,
}

impl History {
    fn new() -> Self {
        let instrument = InstrumentId::new(AUTHOR, 900);
        let staves = [StaffId::new(AUTHOR, 901), StaffId::new(AUTHOR, 902)];
        let region = RegionId::new(AUTHOR, 910);
        let other_region = RegionId::new(AUTHOR, 911);
        let instances = [
            StaffInstanceId::new(AUTHOR, 920),
            StaffInstanceId::new(AUTHOR, 921),
        ];
        let measures = [
            [MeasureId::new(AUTHOR, 930), MeasureId::new(AUTHOR, 931)],
            [MeasureId::new(AUTHOR, 932), MeasureId::new(AUTHOR, 933)],
        ];
        let voice = VoiceId::new(AUTHOR, 940);
        let notes = [EventId::new(AUTHOR, 950), EventId::new(AUTHOR, 951)];
        let mut h = History {
            envelopes: Vec::new(),
            next: 0,
            region,
            other_region,
            measures,
            notes,
            voice,
        };
        h.push(OperationKind::CreateInstrument(CreateInstrumentOp {
            instrument: valuegen::instrument(instrument),
        }));
        for staff in staves {
            h.push(OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(staff, instrument),
            }));
        }
        let mut other = valuegen::region(other_region);
        other.time_extent.start = TimeAnchor::WallClock {
            time: WallClockTime(2_000_000_000),
        };
        other.time_extent.end = TimeAnchor::WallClock {
            time: WallClockTime(2_000_001_000),
        };
        for value in [valuegen::region(region), other] {
            h.push(OperationKind::CreateRegion(CreateRegionOp {
                region: value,
            }));
        }
        for (i, instance) in instances.into_iter().enumerate() {
            h.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region,
                instance: valuegen::staff_instance(instance, staves[i]),
            }));
            for (j, id) in measures[i].into_iter().enumerate() {
                h.push(OperationKind::CreateMeasure(CreateMeasureOp {
                    instance,
                    measure: Measure {
                        id,
                        start: valuegen::region_start_anchor(region, at(4 * j as i64)),
                        time_signature: None,
                        explicit_number: None,
                        number_visibility: MeasureNumberVisibility::Auto,
                    },
                }));
            }
        }
        h.push(OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: instances[0],
            voice: valuegen::voice(voice),
        }));
        for (i, note) in notes.into_iter().enumerate() {
            h.push(OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instances[0],
                event: valuegen::insert_event_value(
                    note,
                    voice,
                    at(i as i64),
                    MusicalDuration(RationalTime::new(1, 4).expect("a quarter")),
                    &[epiphany_core::PitchId::new(AUTHOR, 960 + i as u64)],
                ),
            }));
        }
        h
    }

    fn push(&mut self, kind: OperationKind) -> OperationId {
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
            transaction: None,
            payload: OperationPayload::Primitive(kind),
        });
        id
    }

    fn system_break(&mut self, region: RegionId, anchor: TimeAnchor, present: bool) -> OperationId {
        self.push(OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
            region,
            anchor,
            present,
        }))
    }

    fn page_break(&mut self, region: RegionId, anchor: TimeAnchor, present: bool) -> OperationId {
        self.push(OperationKind::SetUserPageBreak(SetUserPageBreakOp {
            region,
            anchor,
            present,
        }))
    }

    /// Both reductions, held to each other in every effect and the canonical
    /// bytes, every envelope reduced; the graph-aware score breaks no
    /// invariant.
    fn reduce(&self) -> (MaterializedState, GraphMaterialization) {
        let mut set = OperationSet::new();
        set.accept_all(self.envelopes.iter().cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(0))));
        assert_eq!(free.effects.len(), self.envelopes.len());
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

fn target_missing(state: &MaterializedState, id: OperationId) -> bool {
    *effect(state, id)
        == OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::TargetMissing,
            },
        }
}

/// The region's system and page breaks in the graph.
fn graph_breaks(
    aware: &GraphMaterialization,
    region: RegionId,
) -> (Vec<TimeAnchor>, Vec<TimeAnchor>) {
    let content = aware
        .score
        .canvas
        .regions
        .iter()
        .find(|r| r.id == region)
        .and_then(|r| r.content.staff_based())
        .expect("a staff-based region");
    (
        content.user_system_breaks.clone(),
        content.user_page_breaks.clone(),
    )
}

/// Every setup operation applied: the scaffold is sound.
#[test]
fn the_scaffold_applies_whole() {
    let h = History::new();
    let (free, _) = h.reduce();
    assert!(free
        .effects
        .iter()
        .all(|(_, e)| matches!(e, OperationEffect::Applied)));
}

/// Two breaks anchored to the starts of two measures at one position (each
/// instance's measure at four quarters), and a third at the region-start
/// offset of four quarters, keep three slots, system and page alike; version
/// 4 keyed the measure anchors by the region's origin and the offset by four
/// quarters, so the two measure breaks shared one slot.
#[test]
fn breaks_at_distinct_anchors_keep_their_own_slots() {
    let mut h = History::new();
    let region = h.region;
    let anchors = [
        start_of(h.measures[0][1]),
        start_of(h.measures[1][1]),
        valuegen::region_start_anchor(region, at(4)),
    ];
    let writes: Vec<OperationId> = anchors
        .iter()
        .flat_map(|a| {
            [
                h.system_break(region, a.clone(), true),
                h.page_break(region, a.clone(), true),
            ]
        })
        .collect();
    let (free, aware) = h.reduce();
    assert!(writes.iter().all(|id| applied(&free, *id)));
    let keys = |map: &std::collections::BTreeMap<(RegionId, BreakAnchor), bool>| {
        map.iter()
            .map(|((r, key), present)| (*r, key.anchor().clone(), *present))
            .collect::<Vec<_>>()
    };
    for map in [&free.breaks, &free.page_breaks] {
        let mut want: Vec<_> = anchors.iter().map(|a| (region, a.clone(), true)).collect();
        want.sort_by_key(|(_, a, _)| BreakAnchor::new(a.clone()));
        assert_eq!(keys(map), want);
    }
    let (system, page) = graph_breaks(&aware, region);
    assert_eq!(system, anchors);
    assert_eq!(page, anchors);
}

/// A break names what its region holds live: a measure or an event of
/// another region, an event or a measure no operation made, and an anchor to
/// another region are refused `TargetMissing` in both modes; the same anchors
/// in their own region apply.
#[test]
fn a_break_naming_what_its_region_does_not_hold_is_refused() {
    let mut h = History::new();
    let (region, other) = (h.region, h.other_region);
    let foreign = [
        start_of(h.measures[0][1]),
        TimeAnchor::Event {
            id: h.notes[1],
            offset: AnchorOffset::Zero,
        },
        valuegen::region_start_anchor(region, at(4)),
    ];
    let mut refused: Vec<OperationId> = foreign
        .iter()
        .map(|anchor| h.system_break(other, anchor.clone(), true))
        .collect();
    refused.push(h.page_break(region, start_of(MeasureId::new(AUTHOR, 999)), true));
    refused.push(h.page_break(
        region,
        TimeAnchor::Event {
            id: EventId::new(AUTHOR, 999),
            offset: AnchorOffset::Zero,
        },
        true,
    ));
    let admitted: Vec<OperationId> = foreign
        .iter()
        .map(|anchor| h.system_break(region, anchor.clone(), true))
        .collect();
    let (free, aware) = h.reduce();
    assert!(refused.iter().all(|id| target_missing(&free, *id)));
    assert!(admitted.iter().all(|id| applied(&free, *id)));
    assert_eq!(graph_breaks(&aware, other), (vec![], vec![]));
}

/// A break anchored to an event keeps its slot when the event moves: a
/// whole-event modify moving the note leaves the break where its anchor
/// names it, and a later delete of the note leaves it keyed, drawing nothing,
/// in both modes; the region's other break is untouched.
#[test]
fn a_break_anchored_to_an_event_keeps_its_slot_as_the_event_moves_and_goes() {
    let mut h = History::new();
    let region = h.region;
    let note = h.notes[1];
    let anchor = TimeAnchor::Event {
        id: note,
        offset: AnchorOffset::Zero,
    };
    let set = h.system_break(region, anchor.clone(), true);
    let measure = h.system_break(region, start_of(h.measures[0][1]), true);
    let moved = {
        let mut event = valuegen::insert_event_value(
            note,
            h.voice,
            at(3),
            MusicalDuration(RationalTime::new(1, 4).expect("a quarter")),
            &[epiphany_core::PitchId::new(AUTHOR, 961)],
        );
        if let epiphany_core::Event::Pitched(p) = &mut event {
            p.position = EventPosition::Musical(at(3));
        }
        h.push(OperationKind::ModifyEvent(ModifyEventOp { event }))
    };
    let (free, aware) = h.reduce();
    assert!([set, measure, moved].iter().all(|id| applied(&free, *id)));
    let key = (region, BreakAnchor::new(anchor.clone()));
    assert_eq!(
        free.breaks.get(&key),
        Some(&true),
        "the slot after the move"
    );
    assert_eq!(
        graph_breaks(&aware, region).0,
        [anchor.clone(), start_of(h.measures[0][1])]
    );

    let deleted = h.push(OperationKind::DeleteEvent(DeleteEventOp {
        event: note,
        tuplet_compensation: TupletCompensation::NotInTuplet,
    }));
    let (free, aware) = h.reduce();
    assert!(
        applied(&free, deleted)
            || matches!(
                effect(&free, deleted),
                OperationEffect::AppliedWithRepair { .. }
            )
    );
    assert_eq!(
        free.breaks.get(&key),
        Some(&true),
        "the slot after the delete"
    );
    assert_eq!(
        graph_breaks(&aware, region).0,
        [anchor, start_of(h.measures[0][1])]
    );
}
