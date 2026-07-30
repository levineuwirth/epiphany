//! Genesis tranche G3b packet 2 (`spec/CONTRACT_GENESIS_G3B_MEASURE.md`,
//! architecture note): invariant 20's pin 6/6b comparable relation and
//! musical delta are implemented TWICE — once in `epiphany-core`'s
//! `invariants.rs` (over a materialized `Score`, no operational chains to
//! reconstruct) and once in `epiphany-ops`'s `Reducer` (over operational
//! write chains, both graph-aware and base-free). `epiphany-ops` depends on
//! `epiphany-core`, never the reverse, so invariant 20 cannot call the
//! reducer's private methods, and there is no third crate either could
//! delegate to instead. Two independent implementations of one normative
//! relation is a divergence hazard, and this is the guard against it: drive
//! the SAME anchor pairs through both (`epiphany_core::invariants::
//! measure_anchor_relation` and `epiphany_ops::
//! measure_anchor_relation_for_agreement_test`) and assert they agree, on
//! BOTH the comparable-or-not verdict and (when comparable) the ordering —
//! plus the musical delta, pin 6b's companion relation.
//!
//! **Mutation:** perturb ONE implementation only (e.g. let core's relation
//! order across differing `pos`/`edge` selectors, exactly the unsoundness
//! contract pin 6 rules out) and observe this test go red. Counted as an
//! extra mutation beyond M34-M47, reported separately.

use std::cmp::Ordering;

use epiphany_core::{
    AnchorOffset, EventId, IdentityContext, Measure, MeasureId, MeasureNumberVisibility,
    MeasurePosition, MetricTimeModel, MusicalDuration, RationalTime, Region, RegionContent,
    RegionEdge, RegionId, RegionTimeModel, ReplicaId, Score, StaffBasedContent, StaffExtent,
    StaffId, StaffInstance, StaffInstanceId, TimeAnchor, TimeExtent, WallClockDuration,
    WallClockTime,
};

/// Builds a fixture `Score` with TWO staff instances (in the SAME region,
/// to keep it minimal): `inst_a` carries measures `[m1, m2, m3]` in that
/// vector order (positions 0, 1, 2 — c3's "vector index"); `inst_b` carries
/// a single measure `m4`, so a `Measure` pair spanning `inst_a`/`inst_b` is
/// the cross-instance case c3 must refuse.
fn fixture() -> (
    Score,
    RegionId,
    StaffInstanceId,
    StaffInstanceId,
    MeasureId,
    MeasureId,
    MeasureId,
    MeasureId,
) {
    let replica = ReplicaId(9);
    let mut idc = IdentityContext::new(replica);
    let region_id: RegionId = idc.mint();
    let staff_a: StaffId = idc.mint();
    let staff_b: StaffId = idc.mint();
    let inst_a: StaffInstanceId = idc.mint();
    let inst_b: StaffInstanceId = idc.mint();

    let m1 = MeasureId::new(replica, 101);
    let m2 = MeasureId::new(replica, 102);
    let m3 = MeasureId::new(replica, 103);
    let m4 = MeasureId::new(replica, 104);

    let bare = |id: MeasureId, offset_wholes: i32| Measure {
        id,
        start: TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: if offset_wholes == 0 {
                AnchorOffset::Zero
            } else {
                AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(offset_wholes)))
            },
        },
        time_signature: None,
        explicit_number: None,
        number_visibility: MeasureNumberVisibility::Auto,
    };

    let mut instance_a = StaffInstance::new(inst_a, staff_a);
    instance_a.measures = vec![bare(m1, 0), bare(m2, 1), bare(m3, 2)];
    let mut instance_b = StaffInstance::new(inst_b, staff_b);
    instance_b.measures = vec![bare(m4, 0)];

    let region = Region {
        id: region_id,
        time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: vec![instance_a, instance_b],
            ..Default::default()
        }),
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(1_000_000),
            },
        },
        staff_extent: StaffExtent {
            staves: vec![staff_a, staff_b],
        },
        local_tempo_map: None,
        permits_spanning_slurs: false,
    };

    let mut score = Score::empty(idc.clone());
    score.identity = idc;
    score.canvas.regions = vec![region];
    (score, region_id, inst_a, inst_b, m1, m2, m3, m4)
}

fn measure_anchor(id: MeasureId, pos: MeasurePosition, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Measure {
        id,
        position: pos,
        offset,
    }
}

fn region_anchor(region: RegionId, edge: RegionEdge, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge,
        offset,
    }
}

fn event_anchor(id: EventId, offset: AnchorOffset) -> TimeAnchor {
    TimeAnchor::Event { id, offset }
}

fn musical(n: i32) -> AnchorOffset {
    AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(n)))
}

fn wallclock(n: i64) -> AnchorOffset {
    AnchorOffset::WallClock(WallClockDuration(n))
}

/// Asserts both implementations agree on the comparable-or-not verdict, the
/// ordering when comparable, and the musical delta — for one anchor pair,
/// in BOTH orientations (a,b) and (b,a), since the relation and delta are
/// meant to be antisymmetric.
fn assert_agrees(score: &Score, label: &str, a: &TimeAnchor, b: &TimeAnchor) {
    let (core_order, core_delta) = epiphany_core::measure_anchor_relation(score, a, b);
    let (ops_order, ops_delta) =
        epiphany_ops::measure_anchor_relation_for_agreement_test(score, a, b);
    assert_eq!(
        core_order, ops_order,
        "{label}: comparable-order verdict disagrees (core: {core_order:?}, ops: {ops_order:?})"
    );
    assert_eq!(
        core_delta, ops_delta,
        "{label}: musical-delta verdict disagrees (core: {core_delta:?}, ops: {ops_delta:?})"
    );

    // Antisymmetric check, the opposite direction.
    let (core_order_rev, core_delta_rev) = epiphany_core::measure_anchor_relation(score, b, a);
    let (ops_order_rev, ops_delta_rev) =
        epiphany_ops::measure_anchor_relation_for_agreement_test(score, b, a);
    assert_eq!(
        core_order_rev, ops_order_rev,
        "{label} (reversed): comparable-order verdict disagrees"
    );
    assert_eq!(
        core_delta_rev, ops_delta_rev,
        "{label} (reversed): musical-delta verdict disagrees"
    );
}

#[test]
fn cross_crate_anchor_relation_agrees_on_every_table_row() {
    let (score, region, _inst_a, _inst_b, m1, m2, m3, m4) = fixture();
    let e1 = EventId::new(ReplicaId(9), 1);
    let e2 = EventId::new(ReplicaId(9), 2);

    // c1: Event, same id.
    assert_agrees(
        &score,
        "c1 same event id, Zero vs Musical(1)",
        &event_anchor(e1, AnchorOffset::Zero),
        &event_anchor(e1, musical(1)),
    );
    // c1: Event, different id -- not comparable.
    assert_agrees(
        &score,
        "c1 different event ids",
        &event_anchor(e1, AnchorOffset::Zero),
        &event_anchor(e2, AnchorOffset::Zero),
    );
    // Cross-clock, same event id -- not comparable.
    assert_agrees(
        &score,
        "c1 same event id, Musical vs WallClock",
        &event_anchor(e1, musical(1)),
        &event_anchor(e1, wallclock(1)),
    );

    // c2: Measure, same id, same pos (Start), differing Musical offsets.
    assert_agrees(
        &score,
        "c2 same measure id, Start, Musical(0) vs Musical(2)",
        &measure_anchor(m1, MeasurePosition::Start, musical(0)),
        &measure_anchor(m1, MeasurePosition::Start, musical(2)),
    );
    // c2: Measure, same id, but DIFFERENT pos -- not comparable.
    assert_agrees(
        &score,
        "c2 same measure id, Start vs End",
        &measure_anchor(m1, MeasurePosition::Start, AnchorOffset::Zero),
        &measure_anchor(m1, MeasurePosition::End, AnchorOffset::Zero),
    );

    // c3: distinct measure ids, Start/Zero, SAME instance's vector,
    // adjacent and non-adjacent pairs.
    assert_agrees(
        &score,
        "c3 same-vector adjacent (m1, m2)",
        &measure_anchor(m1, MeasurePosition::Start, AnchorOffset::Zero),
        &measure_anchor(m2, MeasurePosition::Start, AnchorOffset::Zero),
    );
    assert_agrees(
        &score,
        "c3 same-vector non-adjacent (m1, m3)",
        &measure_anchor(m1, MeasurePosition::Start, AnchorOffset::Zero),
        &measure_anchor(m3, MeasurePosition::Start, AnchorOffset::Zero),
    );
    // c3: distinct measure ids, Start/Zero, CROSS-INSTANCE -- not
    // comparable (m2 in inst_a's vector, m4 in inst_b's).
    assert_agrees(
        &score,
        "c3 cross-instance (m2, m4)",
        &measure_anchor(m2, MeasurePosition::Start, AnchorOffset::Zero),
        &measure_anchor(m4, MeasurePosition::Start, AnchorOffset::Zero),
    );
    // c3's restriction: nonzero offset -- not comparable even though both
    // are Start and in the same vector.
    assert_agrees(
        &score,
        "c3 restriction: nonzero offset",
        &measure_anchor(m1, MeasurePosition::Start, musical(1)),
        &measure_anchor(m2, MeasurePosition::Start, AnchorOffset::Zero),
    );
    // c3's restriction: End position -- not comparable.
    assert_agrees(
        &score,
        "c3 restriction: End position",
        &measure_anchor(m1, MeasurePosition::End, AnchorOffset::Zero),
        &measure_anchor(m2, MeasurePosition::End, AnchorOffset::Zero),
    );

    // c4: Region, same id, same edge, differing offsets (Musical and
    // WallClock clocks separately, plus Zero-normalization).
    assert_agrees(
        &score,
        "c4 same region/edge, Musical(0) vs Musical(3)",
        &region_anchor(region, RegionEdge::Start, musical(0)),
        &region_anchor(region, RegionEdge::Start, musical(3)),
    );
    assert_agrees(
        &score,
        "c4 same region/edge, WallClock(10) vs WallClock(20)",
        &region_anchor(region, RegionEdge::Start, wallclock(10)),
        &region_anchor(region, RegionEdge::Start, wallclock(20)),
    );
    assert_agrees(
        &score,
        "c4 same region/edge, Zero vs Musical(5)",
        &region_anchor(region, RegionEdge::Start, AnchorOffset::Zero),
        &region_anchor(region, RegionEdge::Start, musical(5)),
    );
    // c4: same id, DIFFERENT edge -- not comparable, even at Zero/Zero.
    assert_agrees(
        &score,
        "c4 same region id, Start vs End",
        &region_anchor(region, RegionEdge::Start, AnchorOffset::Zero),
        &region_anchor(region, RegionEdge::End, AnchorOffset::Zero),
    );
    // c4: differing region id -- not comparable.
    assert_agrees(
        &score,
        "c4 different region ids",
        &region_anchor(region, RegionEdge::Start, AnchorOffset::Zero),
        &region_anchor(
            RegionId::new(ReplicaId(9), 999),
            RegionEdge::Start,
            AnchorOffset::Zero,
        ),
    );
    // Draft 3's unsoundness, explicitly regression-locked here too: Start
    // vs End with a nonzero offset must NOT be ordered by selector.
    assert_agrees(
        &score,
        "c4 Start/Musical(100) vs End/Zero -- must be unverifiable in both",
        &region_anchor(region, RegionEdge::Start, musical(100)),
        &region_anchor(region, RegionEdge::End, AnchorOffset::Zero),
    );

    // c5: WallClock, no referent id.
    assert_agrees(
        &score,
        "c5 WallClock(0) vs WallClock(500)",
        &TimeAnchor::WallClock {
            time: WallClockTime(0),
        },
        &TimeAnchor::WallClock {
            time: WallClockTime(500),
        },
    );

    // Cross-variant pairs -- never comparable.
    assert_agrees(
        &score,
        "Event vs Measure",
        &event_anchor(e1, AnchorOffset::Zero),
        &measure_anchor(m1, MeasurePosition::Start, AnchorOffset::Zero),
    );
    assert_agrees(
        &score,
        "Measure vs Region",
        &measure_anchor(m1, MeasurePosition::Start, AnchorOffset::Zero),
        &region_anchor(region, RegionEdge::Start, AnchorOffset::Zero),
    );
    assert_agrees(
        &score,
        "Region vs WallClock",
        &region_anchor(region, RegionEdge::Start, AnchorOffset::Zero),
        &TimeAnchor::WallClock {
            time: WallClockTime(0),
        },
    );
}

/// Direct, minimal sanity check that the two functions actually AGREE on a
/// concrete verdict (not merely on each other's None/None) -- guards
/// against a degenerate agreement where both sides trivially return `None`
/// for everything.
#[test]
fn cross_crate_anchor_relation_actually_produces_non_trivial_verdicts() {
    let (score, region, ..) = fixture();
    let a = region_anchor(region, RegionEdge::Start, musical(0));
    let b = region_anchor(region, RegionEdge::Start, musical(3));
    let (core_order, core_delta) = epiphany_core::measure_anchor_relation(&score, &a, &b);
    let (ops_order, ops_delta) =
        epiphany_ops::measure_anchor_relation_for_agreement_test(&score, &a, &b);
    assert_eq!(core_order, Some(Ordering::Less));
    assert_eq!(ops_order, Some(Ordering::Less));
    assert_eq!(core_delta, Some(MusicalDuration(RationalTime::from_int(3))));
    assert_eq!(ops_delta, Some(MusicalDuration(RationalTime::from_int(3))));
}
