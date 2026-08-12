//! The Chapter 5 graph invariants (Chapter 5 §"Graph Invariants").
//!
//! Violations carry a two-armed [`ViolationKind`]: `Invariant` for the Chapter 5
//! graph invariants, `Requirement` for a normative requirement named by its
//! label. A requirement failure is not an invariant failure, and neither arm is
//! a fallback for the other.
//!
//! The spec enumerates a set of structural invariants every well-formed score
//! graph must satisfy. They are *property tests in CI, not runtime assertions
//! in release builds* (QUICKSTART, Agent B): this module is the checker the
//! property tests and generators (see [`crate::generators`]) drive. Each
//! enumerated invariant has exactly one check returning a typed
//! [`WellFormednessViolation`] witness identifying the smallest offending objects.
//!
//! **Count.** [`GraphInvariant::all`] is the single origin: its length *is* the
//! count, and no prose here restates it — a restated count goes stale silently
//! the next time the enumeration grows, which it has twice.
//!
//! The QUICKSTART's summary count disagrees with `core_spec.tex`'s own Chapter 5
//! enumeration; that discrepancy is recorded as a Pass 11 candidate in
//! `DECISIONS.md` (the spec is the contract) and is not reconciled here. Beyond
//! the spec body's original set, two rungs have appended: Genesis tranche G3b
//! (`spec/CONTRACT_GENESIS_G3B_MEASURE.md`) added measure-meter consistency
//! (see [`GraphInvariant::MeasureMeterConsistency`]), and P13-S16
//! (`spec/CONTRACT_P13S16_PROJECTION.md`) added staff-group membership
//! agreement (see [`GraphInvariant::StaffGroupMembershipAgreement`]).
//!
//! **Scope of structural decidability.** A few invariants depend on resolving
//! [`crate::TimeAnchor`]s to absolute time (region time-overlap, anchor-offset
//! agreement), which in general needs the full tempo/measure machinery that is
//! out of this crate's scope. Those checks are *sound but incomplete*: they
//! flag the cases this prototype can resolve (notably wall-clock-anchored
//! extents) and never raise a false positive. This is documented per check.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::event::Event;
use crate::graph::{
    derive_promoted_voice_id, CoordinateDiscipline, MeterChange, Region, RegionTimeModel, Score,
    TieClass, TimeSignature, VoiceOrigin,
};
use crate::ids::{
    EventId, MeasureId, PitchId, RegionId, ReplicaId, StaffGroupId, StaffId, StaffInstanceId,
    TimeSignatureId, VoiceId,
};
use crate::pitch::{PitchSpaceId, SpellingDirective, SpellingScope};
use crate::time::{
    AnchorOffset, ConcreteDuration, EventDuration, EventPosition, MeasurePosition, MusicalDuration,
    MusicalPosition, OffsetKind, TimeAnchor, WallClockDuration,
};

/// The Chapter 5 graph invariants, numbered as in §"Graph Invariants".
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum GraphInvariant {
    /// 1. Every arena event's `voice` points to a voice that lists it.
    EventVoiceBacklink,
    /// 2. Every event in a voice's list back-points to that voice.
    VoiceEventBacklink,
    /// 3. Events within a voice are sorted by position and do not overlap.
    VoiceEventsSortedNonOverlap,
    /// 4. Event coordinate variants agree with the region's time model.
    EventCoordinateModel,
    /// 5. Containment is a tree: each voice/instance has exactly one parent.
    ContainmentTree,
    /// 6. Each `StaffInstance.staff` resolves; no `StaffId` twice in a region.
    StaffInstanceResolves,
    /// 7. Region extents don't both overlap; `staff_extent` matches instances.
    RegionExtents,
    /// 8. Each measure belongs to exactly one staff instance.
    MeasureSingleInstance,
    /// 9. Each anchor's offset variant agrees with its target's time model.
    AnchorOffsetModel,
    /// 10. Every graph reference resolves to an extant object, except where
    ///     the re-anchoring rules explicitly permit transient dangling states
    ///     during edits. This surface is **derived from the check bodies**, not
    ///     copied from prose: `spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md`
    ///     pin 1 is the sole origin for both this list and `core_spec.tex`'s
    ///     item 10, because the two were incomplete in different places and
    ///     neither could be repaired from the other (P13-S26).
    ///
    ///   - Slur.start_event — live event.
    ///   - Slur.end_event — live event.
    ///   - Tie.start_event — live event.
    ///   - Tie.end_event — live event.
    ///   - Beam.events — live event.
    ///   - SubBeam.events — live event.
    ///   - Tuplet.members — live event.
    ///   - Tuplet.parent — extant tuplet.
    ///   - Spanner.staves — declared staff.
    ///   - Spanner.start — anchor target.
    ///   - Spanner.end — anchor target.
    ///   - Marker.anchor — anchor target.
    ///   - RepeatStructure.start — anchor target.
    ///   - RepeatStructure.end — anchor target.
    ///   - RepeatStructure.kind — anchor target.
    ///   - RepeatStructure.voltas — anchor target.
    ///   - ChordSymbol.anchor — anchor target.
    ///   - AnalyticalAnnotation.anchor — anchor target, extant region, live event.
    ///   - AnalyticalAnnotation.layer — declared analysis layer.
    ///   - Comment.anchor — anchor target, extant region, live event.
    ///   - GraphicGesture.objects — stored graphic object.
    ///   - GraphicGesture.anchoring — anchor target, declared staff, live event.
    ///   - LyricLine.events — live event.
    ///   - Staff.instrument — declared instrument.
    ///   - StaffInstance.instrument_override — declared instrument.
    ///   - Staff.group — declared staff group.
    ///   - StaffGroup.members — declared staff.
    ///   - PartDefinition.staves — declared staff.
    ///   - ViewDefinition.active_layers — declared analysis layer.
    ///   - MetricTimeModel.meters — declared time signature.
    ///   - StaffBasedContent.default_metric_grid — declared time signature.
    ///   - Measure.time_signature — declared time signature.
    ///   - StaffInstance.local_metric_grid — declared time signature.
    ///   - NotatedComponent.tuplet — extant tuplet.
    ///   - IndeterminacyHints.alternatives — live event.
    ///   - TrajectoryEvent.start — live pitch.
    ///   - TrajectoryEvent.end — live pitch.
    ///   - GraphicEvent.graphics — stored graphic object.
    ///   - CueEvent.source — live event.
    ///   - TempoSegment.start — anchor target.
    ///   - TempoSegment.end — anchor target.
    ///
    ///     Beyond that surface, the checker reaches four rules that are NOT
    ///     part of the normative invariant 10 and **no longer report under this
    ///     tag**. Since P13-S29 each carries its own `req:` label in the
    ///     `Requirement` arm of [`ViolationKind`]: tempo segment shape
    ///     (Chapter 3, `req:time:tempo-segment-shape`); tempo segment ordering
    ///     and non-overlap (Chapter 3, `req:time:tempo-segment-order`);
    ///     aleatoric ordering and bounds region locality (Chapter 3,
    ///     `req:time:aleatoric-reference-locality`); and accidental
    ///     modification expressibility (Chapter 4,
    ///     `req:tuning:accidental-modification-compatibility`). Neither
    ///     `check_invariant` nor this violation's `Display` attributes them to
    ///     invariant 10 any more. [`check_invariants`] still returns them, so a
    ///     caller asking "is this graph well-formed" keeps its coverage.
    CrossCuttingRefsResolve,
    /// 11. Identifiers are unique within their kind (every id kind), with
    ///     reserved-namespace (`SYSTEM_DERIVED`) misuse, tombstone/live
    ///     collisions, and arena index/well-formedness integrity also enforced.
    UniqueIdentifiers,
    /// 12. Every embedded `PitchId` is unique in the pitch-identity index.
    PitchIdUnique,
    /// 13. Every `SpellingScope::Pitch` resolves to a live/tombstoned pitch.
    SpellingScopeResolves,
    /// 14. Every decomposition target resolves to a live/tombstoned event.
    DecompositionTargetResolves,
    /// 15. Live decomposition component durations sum to the event duration.
    DecompositionSum,
    /// 16. Tuplet member durations sum to the required total.
    TupletSum,
    /// 17. Tie pairings reference pitches of the endpoints; class rules hold.
    TiePairing,
    /// 18. Voice origin is consistent; promoted ids match the derivation.
    VoiceOriginConsistent,
    /// 19. Barline-group members stay within one region.
    BarlineGroupSameRegion,
    /// 20. Genesis tranche G3b (`spec/CONTRACT_GENESIS_G3B_MEASURE.md` pins
    ///     6/6b/6c/9b): a measure's declared time signature AGREES with the
    ///     effective metric grid's active signature at its start, and
    ///     consecutive measure starts are separated by the governing
    ///     signature's `measure_duration()` (BOUNDARY consistency).
    ///     `time_signature: None` exempts only the agreement clause — the
    ///     inherited meter still governs boundary consistency. This check
    ///     does NOT duplicate invariant 10's signature-*resolution* check
    ///     (`CrossCuttingRefsResolve`): it only compares ALREADY-RESOLVING
    ///     signatures. It ABSTAINS — emits no violation — wherever pin 6's
    ///     comparable relation or pin 6b's musical delta cannot decide the
    ///     comparison (base-ingested data may predate the rule); this is
    ///     deliberate abstention, not a soundness gap (pin 7). A
    ///     pickup/anacrusis first measure has no predecessor for the
    ///     boundary clause, which is vacuous for it; the agreement clause
    ///     has no such exemption and still applies. Its successor is
    ///     measured against the governing signature's full
    ///     `measure_duration()` and can be refused and flagged (P13-S19,
    ///     deferred).
    MeasureMeterConsistency,
    /// 21. `Staff.group` and `StaffGroup.members` **agree in both directions**:
    ///     every live staff naming a group appears in that group's `members`,
    ///     and every staff a group lists names that group in its own `group`
    ///     field. `Staff.group` is the sole authority; `members` is the
    ///     projection maintained from it under reduction (P13-S16 pins 1, 2, 5).
    ///
    ///     Witnesses name **both ids and the direction**, because the two
    ///     directions fail for different reasons and are checked by two
    ///     separately dispatched methods (P13-S16 pin 6b) — a maintenance gap
    ///     leaves a staff absent from `members`, while a stale projection lists
    ///     a staff that no longer names the group.
    StaffGroupMembershipAgreement,
}

impl GraphInvariant {
    /// This invariant's position in `core_spec.tex` §"Graph Invariants",
    /// numbered from 1. Deliberately count-free: the highest number is whatever
    /// the last arm below returns, not a figure restated in prose.
    pub fn number(self) -> u8 {
        use GraphInvariant::*;
        match self {
            EventVoiceBacklink => 1,
            VoiceEventBacklink => 2,
            VoiceEventsSortedNonOverlap => 3,
            EventCoordinateModel => 4,
            ContainmentTree => 5,
            StaffInstanceResolves => 6,
            RegionExtents => 7,
            MeasureSingleInstance => 8,
            AnchorOffsetModel => 9,
            CrossCuttingRefsResolve => 10,
            UniqueIdentifiers => 11,
            PitchIdUnique => 12,
            SpellingScopeResolves => 13,
            DecompositionTargetResolves => 14,
            DecompositionSum => 15,
            TupletSum => 16,
            TiePairing => 17,
            VoiceOriginConsistent => 18,
            BarlineGroupSameRegion => 19,
            MeasureMeterConsistency => 20,
            StaffGroupMembershipAgreement => 21,
        }
    }

    /// Every invariant, in enumeration order. The array's own length is the
    /// authoritative count — see the module header's **Count** note.
    pub fn all() -> [GraphInvariant; 21] {
        use GraphInvariant::*;
        [
            EventVoiceBacklink,
            VoiceEventBacklink,
            VoiceEventsSortedNonOverlap,
            EventCoordinateModel,
            ContainmentTree,
            StaffInstanceResolves,
            RegionExtents,
            MeasureSingleInstance,
            AnchorOffsetModel,
            CrossCuttingRefsResolve,
            UniqueIdentifiers,
            PitchIdUnique,
            SpellingScopeResolves,
            DecompositionTargetResolves,
            DecompositionSum,
            TupletSum,
            TiePairing,
            VoiceOriginConsistent,
            BarlineGroupSameRegion,
            MeasureMeterConsistency,
            StaffGroupMembershipAgreement,
        ]
    }
}

/// What a [`WellFormednessViolation`] failed. `Invariant` names a numbered
/// Chapter 5 graph invariant; `Requirement` names a normative requirement by its
/// label. A requirement failure is not an invariant failure, and there is no
/// third arm and no unclassified fallback.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ViolationKind {
    Invariant(GraphInvariant),
    Requirement(&'static str),
}

/// A well-formedness failure: one [`ViolationKind`] — `Invariant` or
/// `Requirement` — and the witness identifying the smallest offending objects.
/// A requirement failure is not an invariant failure.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WellFormednessViolation {
    pub kind: ViolationKind,
    pub witness: String,
}

impl WellFormednessViolation {
    fn invariant(which: GraphInvariant, witness: impl Into<String>) -> Self {
        WellFormednessViolation {
            kind: ViolationKind::Invariant(which),
            witness: witness.into(),
        }
    }

    fn requirement(label: &'static str, witness: impl Into<String>) -> Self {
        WellFormednessViolation {
            kind: ViolationKind::Requirement(label),
            witness: witness.into(),
        }
    }
}

impl core::fmt::Display for WellFormednessViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.kind {
            ViolationKind::Invariant(which) => write!(
                f,
                "invariant {} ({:?}) violated: {}",
                which.number(),
                which,
                self.witness
            ),
            ViolationKind::Requirement(label) => {
                write!(f, "requirement {label} violated: {}", self.witness)
            }
        }
    }
}

/// A check the graph could not decide, reported instead of silently passed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeferredCheck {
    /// The invariant whose decision was deferred.
    ///
    /// **Not affected by P13-S29's two-arm split**: every deferred check today is
    /// a genuine graph invariant, correctly attributed.
    pub invariant: GraphInvariant,
    /// A human-readable witness explaining why it could not be decided.
    pub reason: String,
}

impl core::fmt::Display for DeferredCheck {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "invariant {} ({:?}) deferred: {}",
            self.invariant.number(),
            self.invariant,
            self.reason
        )
    }
}

/// The well-formedness checks [`check_invariants`] could not decide for `score`.
///
/// Currently this is region-overlap pairs that share a staff extent but whose
/// time overlap does not resolve to a common timeline (symbolic anchors needing
/// tempo/measure resolution). An empty result means every modelled invariant was
/// fully decided. The core checker reports these rather than silently accepting
/// them as valid; the caller decides how strict to be.
pub fn deferred_checks(score: &Score) -> Vec<DeferredCheck> {
    let idx = GraphIndex::build(score);
    let mut out = Vec::new();
    idx.deferred_region_overlaps(&mut out);
    out
}

/// Checks every Chapter 5 graph invariant over `score`, returning all
/// violations found (empty iff the graph is well-formed).
///
/// **Comprehensive across both arms of [`ViolationKind`].** It returns graph
/// invariant failures *and* the Chapter 3/4 requirement failures the checker
/// reaches, so a caller that wants "is this score well-formed" keeps exactly the
/// coverage it had before P13-S29 split the tag. The two selectors,
/// [`check_invariant`] and [`check_requirement`], are projections of this
/// result; neither is a substitute for it.
///
/// This is *sound but incomplete* for the few invariants that need absolute-time
/// resolution: it never raises a false positive, and the cases it could not
/// decide are reported separately by [`deferred_checks`] rather than silently
/// passed.
pub fn check_invariants(score: &Score) -> Vec<WellFormednessViolation> {
    let idx = GraphIndex::build(score);
    let mut v = Vec::new();
    idx.check_event_voice_backlink(&mut v);
    idx.check_voice_event_backlink(&mut v);
    idx.check_voice_events_sorted_non_overlap(&mut v);
    idx.check_event_coordinate_model(&mut v);
    idx.check_containment_tree(&mut v);
    idx.check_staff_instance_resolves(&mut v);
    idx.check_region_extents(&mut v);
    idx.check_measure_single_instance(&mut v);
    idx.check_anchor_offset_model(&mut v);
    idx.check_cross_cutting_refs(&mut v);
    idx.check_tempo_maps(&mut v);
    idx.check_aleatoric_models(&mut v);
    idx.check_accidental_modification_compatibility(&mut v);
    idx.check_unique_identifiers(&mut v);
    idx.check_pitch_id_unique(&mut v);
    idx.check_spelling_scope_resolves(&mut v);
    idx.check_decomposition_target_resolves(&mut v);
    idx.check_decomposition_sum(&mut v);
    idx.check_tuplet_sum(&mut v);
    idx.check_tie_pairing(&mut v);
    idx.check_voice_origin_consistent(&mut v);
    idx.check_barline_group_same_region(&mut v);
    idx.check_measure_meter_consistency(&mut v);
    // Invariant 21's two directions, dispatched separately (P13-S16 pin 6b).
    // Each call site is independently deletable, which is what M6a and M6b
    // delete; a single call handling both would leave the mutation with no way
    // to fail one direction while the other still reports.
    idx.check_staff_names_absent_group(&mut v);
    idx.check_group_lists_unowned_staff(&mut v);
    v
}

/// Cross-crate agreement-test oracle hook for the pin 6/6b comparable
/// relation and musical delta (G3b packet 2 architecture note: exposed so
/// `epiphany-testkit`'s cross-crate agreement test can drive the SAME
/// anchor pairs through this crate's independent invariant-20
/// implementation ([`GraphIndex::measure20_comparable_order`] /
/// [`GraphIndex::measure20_musical_delta`]) and through `epiphany-ops`'s
/// `Reducer` (which computes the identical normative relation privately,
/// over operational write chains rather than a materialized graph) and
/// assert they agree. This is the guard against maintaining one normative
/// relation in two places without sharing code: the dependency direction
/// (`epiphany-ops` depends on `epiphany-core`, never the reverse) forbids
/// `epiphany-core` from calling into `epiphany-ops`, so invariant 20 cannot
/// reuse the reducer's private methods, and there is no third crate either
/// could delegate to.
pub fn measure_anchor_relation(
    score: &Score,
    a: &TimeAnchor,
    b: &TimeAnchor,
) -> (Option<Ordering>, Option<MusicalDuration>) {
    let idx = GraphIndex::build(score);
    (
        idx.measure20_comparable_order(a, b),
        idx.measure20_musical_delta(a, b),
    )
}

/// Checks a single invariant (useful for targeted negative property tests).
/// Every violation of one graph invariant.
///
/// **Projection of [`check_invariants`], filtered to the `Invariant` arm.** A
/// Chapter 3 or Chapter 4 requirement failure is *not* returned here, however
/// the checker happens to reach it — use [`check_requirement`] for those.
pub fn check_invariant(score: &Score, which: GraphInvariant) -> Vec<WellFormednessViolation> {
    check_invariants(score)
        .into_iter()
        .filter(|v| v.kind == ViolationKind::Invariant(which))
        .collect()
}

/// Every violation of one normative requirement, by its label.
///
/// **Projection of [`check_invariants`], filtered to the `Requirement` arm** —
/// the symmetric counterpart of [`check_invariant`].
pub fn check_requirement(score: &Score, label: &str) -> Vec<WellFormednessViolation> {
    check_invariants(score)
        .into_iter()
        .filter(|v| matches!(v.kind, ViolationKind::Requirement(l) if l == label))
        .collect()
}

/// Pre-computed cross-references over a score, built once per check pass.
struct GraphIndex<'a> {
    score: &'a Score,
    /// Voice id -> the voice (also flags duplicate voice ids).
    voice: HashMap<VoiceId, &'a crate::graph::Voice>,
    /// Voice id -> its (region, instance) parent; absent if the voice id is
    /// duplicated across instances (a containment violation).
    voice_parent: HashMap<VoiceId, (RegionId, StaffInstanceId)>,
    /// Region id -> coordinate discipline.
    region_discipline: HashMap<RegionId, CoordinateDiscipline>,
    /// Instance id -> its region (also flags duplicate instance ids).
    instance_region: HashMap<StaffInstanceId, RegionId>,
    /// Event id -> (voice, index) from the voices' ordered event lists.
    event_voice_index: HashMap<EventId, (VoiceId, usize)>,
    /// Event id -> the staff instance whose voice lists it.
    event_instance: HashMap<EventId, StaffInstanceId>,
    /// Event id -> the pitch ids it embeds.
    event_pitches: HashMap<EventId, BTreeSet<PitchId>>,
    /// Pitch id -> the embedded pitch.
    pitch: HashMap<PitchId, &'a crate::pitch::Pitch>,
    /// Live pitch ids in the arena.
    live_pitches: BTreeSet<PitchId>,
    /// Measure id -> the instance(s) listing it.
    measure_instances: HashMap<MeasureId, Vec<StaffInstanceId>>,
    /// Measure id -> its start anchor (for anchor resolution).
    measure_start: HashMap<MeasureId, &'a TimeAnchor>,
    /// Region id -> the region (for anchor resolution and reference checks).
    region_by_id: HashMap<RegionId, &'a Region>,
    /// Declared score-level staff ids.
    declared_staves: BTreeSet<StaffId>,
    /// Graphic-object ids stored across all regions' graphic content.
    graphic_objects: BTreeSet<crate::ids::GraphicObjectId>,
    /// Declared analysis-layer ids.
    analysis_layers: BTreeSet<crate::ids::AnalysisLayerId>,
    /// Tuplet ids (for `Tuplet::parent` resolution).
    tuplet_ratios: HashMap<crate::ids::TupletId, crate::graph::TupletRatio>,
}

impl<'a> GraphIndex<'a> {
    fn build(score: &'a Score) -> Self {
        let mut voice = HashMap::new();
        let mut voice_dup: HashSet<VoiceId> = HashSet::new();
        let mut voice_parent = HashMap::new();
        let mut region_discipline = HashMap::new();
        let mut instance_region = HashMap::new();
        let mut instance_dup: HashSet<StaffInstanceId> = HashSet::new();
        let mut event_voice_index = HashMap::new();
        let mut event_instance = HashMap::new();
        let mut measure_instances: HashMap<MeasureId, Vec<StaffInstanceId>> = HashMap::new();
        let mut measure_start = HashMap::new();
        let mut region_by_id = HashMap::new();

        for region in &score.canvas.regions {
            region_discipline.insert(region.id, region.time_model.coordinate_discipline());
            region_by_id.insert(region.id, region);
            for si in region.staff_instances() {
                if instance_region.insert(si.id, region.id).is_some() {
                    instance_dup.insert(si.id);
                }
                for m in &si.measures {
                    measure_instances.entry(m.id).or_default().push(si.id);
                    measure_start.entry(m.id).or_insert(&m.start);
                }
                for v in &si.voices {
                    if voice.insert(v.id, v).is_some() {
                        voice_dup.insert(v.id);
                    }
                    voice_parent.insert(v.id, (region.id, si.id));
                    for (ix, e) in v.events.iter().enumerate() {
                        event_voice_index.insert(*e, (v.id, ix));
                        event_instance.insert(*e, si.id);
                    }
                }
            }
        }
        // A duplicated voice/instance has no single parent: drop it so the
        // containment check (5) owns the report rather than other checks
        // silently picking one parent.
        for v in &voice_dup {
            voice_parent.remove(v);
        }

        let mut event_pitches: HashMap<EventId, BTreeSet<PitchId>> = HashMap::new();
        let mut pitch = HashMap::new();
        let mut live_pitches = BTreeSet::new();
        let mut buf = Vec::new();
        for e in score.events.iter() {
            buf.clear();
            e.collect_identified_pitches(&mut buf);
            let set = event_pitches.entry(e.id()).or_default();
            for ip in &buf {
                set.insert(ip.id);
                pitch.insert(ip.id, &ip.pitch);
                live_pitches.insert(ip.id);
            }
        }

        let declared_staves = score.staves.iter().map(|s| s.id).collect();
        let graphic_objects = score
            .canvas
            .regions
            .iter()
            .flat_map(|r| r.content.graphic_objects().iter().map(|o| o.id))
            .collect();
        let analysis_layers = score.analysis_layers.iter().map(|l| l.id).collect();
        let tuplet_ratios = score
            .cross_cutting
            .tuplets
            .iter()
            .map(|t| (t.id, t.ratio))
            .collect();

        GraphIndex {
            score,
            voice,
            voice_parent,
            region_discipline,
            instance_region,
            event_voice_index,
            event_instance,
            event_pitches,
            pitch,
            live_pitches,
            measure_instances,
            measure_start,
            region_by_id,
            declared_staves,
            graphic_objects,
            analysis_layers,
            tuplet_ratios,
        }
    }

    // --- Anchor resolution (shared by invariants 7, 9, 10). -----------------

    /// Resolves a [`TimeAnchor`] to an absolute coordinate on a common timeline
    /// where this prototype can — `WallClock` anchors directly, `Event` anchors
    /// via the target event's region origin **plus** its region-relative
    /// position, `Measure` *start* anchors and `Region` edges recursively —
    /// applying the anchor's offset. Returns `None` when the target is missing,
    /// the clocks disagree, or a coordinate cannot be placed without the
    /// deferred tempo/measure-length machinery (`Measure` *end*, a musical event
    /// position on a wall-clock-placed region, curved tempo, …). Depth-guarded
    /// against cyclic region/measure references. Sound, never a false coordinate.
    ///
    /// The result is an absolute **wall-clock** nanosecond coordinate. In this
    /// model the only absolute leaf is a [`TimeAnchor::WallClock`]; musical
    /// positions are region-relative and placing them on the canvas needs the
    /// deferred musical→wall-clock tempo map, so any musical local position or
    /// musical offset makes the anchor unresolvable (`None`) rather than wrong.
    fn resolve_anchor(&self, anchor: &TimeAnchor, depth: u8) -> Option<i64> {
        if depth == 0 {
            return None;
        }
        match anchor {
            TimeAnchor::WallClock { time } => Some(time.0),
            TimeAnchor::Event { id, offset } => {
                // Event positions are *region-relative* (Chapter 5
                // §"Event Position and Duration"), so the absolute coordinate is
                // the event's region origin plus its local position — resolvable
                // only when the local position is wall-clock (a musical local
                // position needs the deferred tempo map to place on the canvas).
                let si = self.event_instance.get(id)?;
                let region = self.region_by_id.get(self.instance_region.get(si)?)?;
                let origin = self.resolve_anchor(&region.time_extent.start, depth - 1)?;
                let local = match self.score.events.get(*id)?.position() {
                    EventPosition::WallClock(t) => t.0,
                    // A musical position is placed on the wall-clock timeline
                    // through the region's effective tempo map (its
                    // `local_tempo_map`, else the score map) — the conversion
                    // this prototype previously declined. When no tempo is
                    // defined, or the map is piecewise/curved beyond the stub,
                    // the conversion declines (`None`): sound but incomplete,
                    // never a false coordinate.
                    EventPosition::Musical(p) => {
                        let tm = region
                            .local_tempo_map
                            .as_ref()
                            .unwrap_or(&self.score.tempo_map);
                        tm.musical_to_wallclock(p).ok()?.0
                    }
                };
                // Checked: a pathological coordinate sum is reported as
                // unresolvable, never a panic (release builds have overflow
                // checks on).
                apply_offset(origin.checked_add(local)?, offset)
            }
            TimeAnchor::Measure {
                id,
                position,
                offset,
            } => {
                // Only the measure *start* is resolvable without the measure's
                // length (which needs the deferred decomposition/tempo machinery).
                if *position != crate::time::MeasurePosition::Start {
                    return None;
                }
                let start = self.measure_start.get(id)?;
                let base = self.resolve_anchor(start, depth - 1)?;
                apply_offset(base, offset)
            }
            TimeAnchor::Region { id, edge, offset } => {
                let region = self.region_by_id.get(id)?;
                let edge_anchor = match edge {
                    crate::time::RegionEdge::Start => &region.time_extent.start,
                    crate::time::RegionEdge::End => &region.time_extent.end,
                };
                let base = self.resolve_anchor(edge_anchor, depth - 1)?;
                apply_offset(base, offset)
            }
        }
    }

    /// Whether the object a non-wall-clock anchor points at exists in the graph
    /// (used by invariant 10 for cross-cutting anchor endpoints). Wall-clock
    /// anchors reference no object, so they always resolve.
    fn anchor_target_exists(&self, anchor: &TimeAnchor) -> bool {
        match anchor {
            TimeAnchor::WallClock { .. } => true,
            TimeAnchor::Event { id, .. } => self.score.events.contains(*id),
            TimeAnchor::Measure { id, .. } => self.measure_start.contains_key(id),
            TimeAnchor::Region { id, .. } => self.region_by_id.contains_key(id),
        }
    }

    // --- 1. Event -> voice backlink. ----------------------------------------
    fn check_event_voice_backlink(&self, out: &mut Vec<WellFormednessViolation>) {
        for e in self.score.events.iter() {
            let vid = e.voice();
            match self.voice.get(&vid) {
                None => out.push(WellFormednessViolation::invariant(
                    GraphInvariant::EventVoiceBacklink,
                    format!(
                        "event {:?} names voice {:?}, which is not in the graph",
                        e.id(),
                        vid
                    ),
                )),
                Some(v) if !v.events.contains(&e.id()) => {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::EventVoiceBacklink,
                        format!(
                            "event {:?} names voice {:?}, which does not list it",
                            e.id(),
                            vid
                        ),
                    ))
                }
                _ => {}
            }
        }
    }

    // --- 2. Voice -> event backlink. ----------------------------------------
    fn check_voice_event_backlink(&self, out: &mut Vec<WellFormednessViolation>) {
        for (_r, _si, v) in self.score.voices() {
            for e in &v.events {
                match self.score.events.get(*e) {
                    None => out.push(WellFormednessViolation::invariant(
                        GraphInvariant::VoiceEventBacklink,
                        format!(
                            "voice {:?} lists event {:?}, absent from the arena",
                            v.id, e
                        ),
                    )),
                    Some(ev) if ev.voice() != v.id => out.push(WellFormednessViolation::invariant(
                        GraphInvariant::VoiceEventBacklink,
                        format!(
                            "voice {:?} lists event {:?} whose voice is {:?}",
                            v.id,
                            e,
                            ev.voice()
                        ),
                    )),
                    _ => {}
                }
            }
        }
    }

    // --- 3. Events sorted and non-overlapping within a voice. ---------------
    fn check_voice_events_sorted_non_overlap(&self, out: &mut Vec<WellFormednessViolation>) {
        for (_r, _si, v) in self.score.voices() {
            let mut prev: Option<(EventId, Endpoints)> = None;
            for e in &v.events {
                let Some(ev) = self.score.events.get(*e) else {
                    continue; // absence is invariant 2's report
                };
                let cur = Endpoints::of(ev);
                if let Some((pe, pep)) = &prev {
                    if let (Some(p_end), Some(c_start)) = (pep.end_key(), cur.start_key()) {
                        if !p_end.le_same_clock(&c_start) {
                            // Either out of order (start < prev start) or
                            // overlapping (prev end > cur start).
                            out.push(WellFormednessViolation::invariant(
                                GraphInvariant::VoiceEventsSortedNonOverlap,
                                format!(
                                    "in voice {:?}, event {:?} starts before event {:?} ends",
                                    v.id, e, pe
                                ),
                            ));
                        }
                    }
                }
                prev = Some((*e, cur));
            }
        }
    }

    // --- 4. Coordinate variants agree with the region's time model. ---------
    fn check_event_coordinate_model(&self, out: &mut Vec<WellFormednessViolation>) {
        for e in self.score.events.iter() {
            let Some((region, _)) = self.voice_parent.get(&e.voice()) else {
                continue; // unparented voice: invariant 1/5 reports it
            };
            let Some(disc) = self.region_discipline.get(region) else {
                continue;
            };
            if !coordinate_ok(e, *disc) {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::EventCoordinateModel,
                    format!(
                        "event {:?} coordinates {:?}/{:?} contradict region {:?} discipline {:?}",
                        e.id(),
                        e.position().kind(),
                        e.duration().concrete_kind(),
                        region,
                        disc
                    ),
                ));
            }
        }
    }

    // --- 5. Containment is a tree. ------------------------------------------
    fn check_containment_tree(&self, out: &mut Vec<WellFormednessViolation>) {
        // Voice id appearing under more than one instance.
        let mut voice_seen: HashMap<VoiceId, StaffInstanceId> = HashMap::new();
        for (_r, si, v) in self.score.voices() {
            if let Some(prev) = voice_seen.insert(v.id, si) {
                if prev != si {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::ContainmentTree,
                        format!(
                            "voice {:?} appears in instances {:?} and {:?}",
                            v.id, prev, si
                        ),
                    ));
                }
            }
        }
        // Instance id appearing under more than one region.
        let mut inst_seen: HashMap<StaffInstanceId, RegionId> = HashMap::new();
        for r in &self.score.canvas.regions {
            for si in r.staff_instances() {
                if let Some(prev) = inst_seen.insert(si.id, r.id) {
                    if prev != r.id {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::ContainmentTree,
                            format!(
                                "staff instance {:?} appears in regions {:?} and {:?}",
                                si.id, prev, r.id
                            ),
                        ));
                    }
                }
            }
        }
    }

    // --- 6. Instance.staff resolves; no StaffId twice in one region. --------
    fn check_staff_instance_resolves(&self, out: &mut Vec<WellFormednessViolation>) {
        for r in &self.score.canvas.regions {
            let mut staff_in_region: HashSet<StaffId> = HashSet::new();
            for si in r.staff_instances() {
                if !self.declared_staves.contains(&si.staff) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::StaffInstanceResolves,
                        format!(
                            "staff instance {:?} references undeclared staff {:?}",
                            si.id, si.staff
                        ),
                    ));
                }
                if !staff_in_region.insert(si.staff) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::StaffInstanceResolves,
                        format!(
                            "staff {:?} is manifested by two instances in region {:?}",
                            si.staff, r.id
                        ),
                    ));
                }
            }
        }
    }

    // --- 7. Region extents: staff_extent matches; no double overlap. --------
    fn check_region_extents(&self, out: &mut Vec<WellFormednessViolation>) {
        for r in &self.score.canvas.regions {
            // staff_extent must list exactly the manifested staves, no dups.
            let manifested: BTreeSet<StaffId> =
                r.staff_instances().iter().map(|si| si.staff).collect();
            let mut listed = BTreeSet::new();
            for s in &r.staff_extent.staves {
                if !listed.insert(*s) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::RegionExtents,
                        format!("region {:?} staff_extent lists staff {:?} twice", r.id, s),
                    ));
                }
            }
            if listed != manifested {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::RegionExtents,
                    format!(
                        "region {:?} staff_extent {:?} != manifested staves {:?}",
                        r.id, listed, manifested
                    ),
                ));
            }
        }
        // No two regions overlap in both time and staff extent. Time overlap
        // is decided on a common resolved timeline (wall-clock, event/region/
        // measure-start anchors). Pairs whose extents cannot be resolved to a
        // common clock are *not* silently passed: they are reported as undecided
        // by `deferred_checks` (via `deferred_region_overlaps`) rather than
        // treated as disjoint here. This check stays sound — it only flags a
        // proven overlap.
        let regions = &self.score.canvas.regions;
        for i in 0..regions.len() {
            for j in (i + 1)..regions.len() {
                let (a, b) = (&regions[i], &regions[j]);
                if !a.staff_extent_intersects(b) {
                    continue;
                }
                if self.regions_overlap_in_time(a, b) == Some(true) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::RegionExtents,
                        format!(
                            "regions {:?} and {:?} overlap in both time and staff extent",
                            a.id, b.id
                        ),
                    ));
                }
            }
        }
    }

    /// Region-overlap pairs that share a staff extent but whose time overlap is
    /// undecidable here (symbolic anchors needing tempo/measure resolution).
    /// Surfaced by [`deferred_checks`] so the undecided case is explicit, never
    /// silently accepted as disjoint.
    fn deferred_region_overlaps(&self, out: &mut Vec<DeferredCheck>) {
        let regions = &self.score.canvas.regions;
        for i in 0..regions.len() {
            for j in (i + 1)..regions.len() {
                let (a, b) = (&regions[i], &regions[j]);
                if !a.staff_extent_intersects(b) {
                    continue;
                }
                if self.regions_overlap_in_time(a, b).is_none() {
                    out.push(DeferredCheck {
                        invariant: GraphInvariant::RegionExtents,
                        reason: format!(
                            "regions {:?} and {:?} share a staff extent but their time \
                             overlap is undecidable (symbolic anchors need tempo/measure \
                             resolution)",
                            a.id, b.id
                        ),
                    });
                }
            }
        }
    }

    /// `Some(true)`/`Some(false)` when both regions' extents resolve to absolute
    /// wall-clock coordinates; `None` when they cannot be compared (deferred
    /// tempo/measure machinery). Half-open: touching at a boundary is not an
    /// overlap.
    fn regions_overlap_in_time(&self, a: &Region, b: &Region) -> Option<bool> {
        const MAX_DEPTH: u8 = 8;
        let a0 = self.resolve_anchor(&a.time_extent.start, MAX_DEPTH)?;
        let a1 = self.resolve_anchor(&a.time_extent.end, MAX_DEPTH)?;
        let b0 = self.resolve_anchor(&b.time_extent.start, MAX_DEPTH)?;
        let b1 = self.resolve_anchor(&b.time_extent.end, MAX_DEPTH)?;
        Some(a0 < b1 && b0 < a1)
    }

    // --- 8. Each measure belongs to exactly one instance. -------------------
    fn check_measure_single_instance(&self, out: &mut Vec<WellFormednessViolation>) {
        for (mid, owners) in &self.measure_instances {
            let distinct: BTreeSet<_> = owners.iter().copied().collect();
            if distinct.len() > 1 {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::MeasureSingleInstance,
                    format!("measure {:?} belongs to instances {:?}", mid, distinct),
                ));
            }
        }
    }

    // --- 9. Anchor offset variant agrees with target's time model. ----------
    fn check_anchor_offset_model(&self, out: &mut Vec<WellFormednessViolation>) {
        for a in self.collect_anchors() {
            if let Some(false) = self.offset_ok(a) {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::AnchorOffsetModel,
                    format!(
                        "anchor {:?} offset contradicts its target region's time model",
                        a
                    ),
                ));
            }
        }
    }

    /// Every stored [`TimeAnchor`] reachable in the graph whose offset is
    /// subject to invariant 9: region extents; metric-grid meter changes
    /// (region default, instance-local, and the time model's own); measure
    /// starts; clef and key changes; user system/page breaks; spanner
    /// endpoints; and spelling-range scope endpoints.
    fn collect_anchors(&self) -> Vec<&'a TimeAnchor> {
        let mut anchors: Vec<&TimeAnchor> = Vec::new();
        for r in &self.score.canvas.regions {
            anchors.push(&r.time_extent.start);
            anchors.push(&r.time_extent.end);
            if let RegionTimeModel::Metric(m) = &r.time_model {
                anchors.extend(m.meters.iter().map(|mc| &mc.anchor));
            }
            if let Some(c) = r.content.staff_based() {
                if let Some(g) = &c.default_metric_grid {
                    anchors.extend(g.meter_sequence.iter().map(|mc| &mc.anchor));
                }
                anchors.extend(c.user_system_breaks.iter());
                anchors.extend(c.user_page_breaks.iter());
            }
            for si in r.staff_instances() {
                if let Some(g) = &si.local_metric_grid {
                    anchors.extend(g.meter_sequence.iter().map(|mc| &mc.anchor));
                }
                anchors.extend(si.measures.iter().map(|m| &m.start));
                anchors.extend(si.clef_sequence.iter().map(|c| &c.anchor));
                anchors.extend(si.key_sequence.iter().map(|k| &k.anchor));
            }
        }
        let cc = &self.score.cross_cutting;
        for sp in &cc.spanners {
            anchors.push(&sp.start);
            anchors.push(&sp.end);
        }
        for m in &cc.markers {
            anchors.push(&m.anchor);
        }
        for rp in &cc.repeats {
            // The shared site-set walk (start/end, kind jump targets, volta
            // spans) — the same set reduction and the index consume.
            anchors.extend(rp.anchor_sites());
        }
        for cs in &cc.chord_symbols {
            anchors.push(&cs.anchor);
        }
        for an in &cc.analytical {
            if let crate::graph::AnnotationAnchor::Range { start, end } = &an.anchor {
                anchors.push(start);
                anchors.push(end);
            }
        }
        for cm in &cc.comments {
            if let crate::graph::AnnotationAnchor::Range { start, end } = &cm.anchor {
                anchors.push(start);
                anchors.push(end);
            }
        }
        for g in &cc.graphic_gestures {
            if let crate::graph::GestureAnchoring::Range { start, end, .. } = &g.anchoring {
                anchors.push(start);
                anchors.push(end);
            }
        }
        for a in &self.score.spelling_attachments {
            if let SpellingScope::Range { start, end, .. } = &a.scope {
                anchors.push(start);
                anchors.push(end);
            }
        }
        // Tempo-map segment boundaries are time anchors too (Chapter 3
        // §"Tempo and the Tempo Map"): their offsets are subject to invariant 9.
        for tm in self.tempo_maps() {
            for seg in &tm.segments {
                anchors.push(&seg.start);
                if let Some(end) = &seg.end {
                    anchors.push(end);
                }
            }
        }
        anchors
    }

    /// Every tempo map in the score: the score-level map plus each region's
    /// `local_tempo_map`.
    fn tempo_maps(&self) -> impl Iterator<Item = &'a crate::tempo::TempoMap> {
        std::iter::once(&self.score.tempo_map).chain(
            self.score
                .canvas
                .regions
                .iter()
                .filter_map(|r| r.local_tempo_map.as_ref()),
        )
    }

    /// `Some(true)`/`Some(false)` when the target region's discipline is
    /// determinable; `None` when it is not (a sound-but-incomplete result).
    fn offset_ok(&self, anchor: &TimeAnchor) -> Option<bool> {
        use crate::graph::AleatoricAnchoringDiscipline as A;
        let (offset, disc) = match anchor {
            TimeAnchor::WallClock { .. } => return Some(true), // no offset
            TimeAnchor::Region { id, offset, .. } => (offset, *self.region_discipline.get(id)?),
            TimeAnchor::Event { id, offset } => {
                let si = self.event_instance.get(id)?;
                let region = self.instance_region.get(si)?;
                let disc = *self.region_discipline.get(region)?;
                // In an `EitherPerEvent` region the *event* fixes the clock, so
                // an offset against it must match that event's coordinate kind —
                // not merely "either" (Chapter 3 §"Aleatoric Time": an event's
                // position and duration kinds must agree). This catches a
                // musical offset on a wall-clock event.
                if disc == CoordinateDiscipline::Aleatoric(A::EitherPerEvent) {
                    let event_kind = self.score.events.get(*id)?.position().kind();
                    return Some(offset_matches_kind(offset.kind(), event_kind));
                }
                (offset, disc)
            }
            TimeAnchor::Measure { id, offset, .. } => {
                let owners = self.measure_instances.get(id)?;
                let si = owners.first()?;
                let region = self.instance_region.get(si)?;
                (offset, *self.region_discipline.get(region)?)
            }
        };
        Some(offset_matches(offset.kind(), disc))
    }

    // --- 10. Cross-cutting references resolve. ------------------------------
    fn check_cross_cutting_refs(&self, out: &mut Vec<WellFormednessViolation>) {
        let live_event = |e: &EventId| self.score.events.contains(*e);
        let mut flag = |cond: bool, what: String| {
            if !cond {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::CrossCuttingRefsResolve,
                    what,
                ));
            }
        };
        for s in &self.score.cross_cutting.slurs {
            flag(
                live_event(&s.start_event),
                format!("slur {:?} start_event dangling", s.id),
            );
            flag(
                live_event(&s.end_event),
                format!("slur {:?} end_event dangling", s.id),
            );
        }
        for t in &self.score.cross_cutting.ties {
            flag(
                live_event(&t.start_event),
                format!("tie {:?} start_event dangling", t.id),
            );
            flag(
                live_event(&t.end_event),
                format!("tie {:?} end_event dangling", t.id),
            );
        }
        for b in &self.score.cross_cutting.beams {
            for e in &b.events {
                flag(
                    live_event(e),
                    format!("beam {:?} event {:?} dangling", b.id, e),
                );
            }
            // Schema major 2: sub-beam member events are references too.
            for sb in &b.sub_beams {
                for e in &sb.events {
                    flag(
                        live_event(e),
                        format!("beam {:?} sub-beam event {:?} dangling", b.id, e),
                    );
                }
            }
        }
        for tp in &self.score.cross_cutting.tuplets {
            for e in &tp.members {
                flag(
                    live_event(e),
                    format!("tuplet {:?} member {:?} dangling", tp.id, e),
                );
            }
            if let Some(parent) = tp.parent {
                flag(
                    self.tuplet_ratios.contains_key(&parent),
                    format!("tuplet {:?} parent {:?} does not exist", tp.id, parent),
                );
            }
        }
        for sp in &self.score.cross_cutting.spanners {
            for s in &sp.staves {
                flag(
                    self.declared_staves.contains(s),
                    format!("spanner {:?} staff {:?} not declared", sp.id, s),
                );
            }
            // The spanner's endpoint anchors are references too: their targeted
            // event / measure / region must exist.
            flag(
                self.anchor_target_exists(&sp.start),
                format!(
                    "spanner {:?} start anchor target {:?} dangling",
                    sp.id, sp.start
                ),
            );
            flag(
                self.anchor_target_exists(&sp.end),
                format!(
                    "spanner {:?} end anchor target {:?} dangling",
                    sp.id, sp.end
                ),
            );
        }
        // The remaining reference-bearing cross-cutting structures.
        let cc = &self.score.cross_cutting;
        let annotation_anchor_ok = |a: &crate::graph::AnnotationAnchor| match a {
            crate::graph::AnnotationAnchor::Event(e) => self.score.events.contains(*e),
            crate::graph::AnnotationAnchor::Region(r) => self.region_by_id.contains_key(r),
            crate::graph::AnnotationAnchor::Range { start, end } => {
                self.anchor_target_exists(start) && self.anchor_target_exists(end)
            }
        };
        for m in &cc.markers {
            flag(
                self.anchor_target_exists(&m.anchor),
                format!("marker {:?} anchor target dangling", m.id),
            );
        }
        for rp in &cc.repeats {
            flag(
                self.anchor_target_exists(&rp.start) && self.anchor_target_exists(&rp.end),
                format!("repeat {:?} anchor target dangling", rp.id),
            );
            // Schema major 2: the kind's jump anchors and each volta's span.
            let kind_ok = match &rp.kind {
                crate::graph::RepeatKind::DaCapo { end_target } => {
                    self.anchor_target_exists(end_target)
                }
                crate::graph::RepeatKind::DalSegno { segno, end_target } => {
                    self.anchor_target_exists(segno) && self.anchor_target_exists(end_target)
                }
                crate::graph::RepeatKind::SimpleRepeat { .. } | crate::graph::RepeatKind::Volta => {
                    true
                }
            };
            flag(
                kind_ok,
                format!("repeat {:?} kind anchor target dangling", rp.id),
            );
            for v in &rp.voltas {
                flag(
                    self.anchor_target_exists(&v.start) && self.anchor_target_exists(&v.end),
                    format!("repeat {:?} volta anchor target dangling", rp.id),
                );
            }
        }
        for cs in &cc.chord_symbols {
            flag(
                self.anchor_target_exists(&cs.anchor),
                format!("chord symbol {:?} anchor target dangling", cs.id),
            );
        }
        for an in &cc.analytical {
            flag(
                annotation_anchor_ok(&an.anchor),
                format!("analytical annotation {:?} anchor dangling", an.id),
            );
            if let Some(layer) = an.layer {
                flag(
                    self.analysis_layers.contains(&layer),
                    format!(
                        "analytical annotation {:?} layer {:?} does not exist",
                        an.id, layer
                    ),
                );
            }
        }
        for cm in &cc.comments {
            flag(
                annotation_anchor_ok(&cm.anchor),
                format!("comment {:?} anchor dangling", cm.id),
            );
        }
        for g in &cc.graphic_gestures {
            for o in &g.objects {
                flag(
                    self.graphic_objects.contains(o),
                    format!("gesture {:?} graphic object {:?} not stored", g.id, o),
                );
            }
            match &g.anchoring {
                crate::graph::GestureAnchoring::Events(es) => {
                    for e in es {
                        flag(
                            live_event(e),
                            format!("gesture {:?} event {:?} dangling", g.id, e),
                        );
                    }
                }
                crate::graph::GestureAnchoring::Range { start, end, staves } => {
                    flag(
                        self.anchor_target_exists(start) && self.anchor_target_exists(end),
                        format!("gesture {:?} range anchor dangling", g.id),
                    );
                    for s in staves {
                        flag(
                            self.declared_staves.contains(s),
                            format!("gesture {:?} staff {:?} not declared", g.id, s),
                        );
                    }
                }
                crate::graph::GestureAnchoring::Free => {}
            }
        }
        for ly in &cc.lyrics {
            for e in &ly.events {
                flag(
                    live_event(e),
                    format!("lyric line {:?} event {:?} dangling", ly.id, e),
                );
            }
        }

        // Structural references: a staff's (and any per-instance override's)
        // instrument must resolve to a declared `Instrument` (Chapter 5
        // §"Top-Level Score Structure" / §"Instruments").
        let instruments: BTreeSet<crate::ids::InstrumentId> =
            self.score.instruments.iter().map(|i| i.id).collect();
        for s in &self.score.staves {
            flag(
                instruments.contains(&s.instrument),
                format!(
                    "staff {:?} instrument {:?} is not declared",
                    s.id, s.instrument
                ),
            );
        }
        for (_r, si) in self.score.staff_instances() {
            if let Some(instr) = si.instrument_override {
                flag(
                    instruments.contains(&instr),
                    format!(
                        "staff instance {:?} instrument override {:?} is not declared",
                        si.id, instr
                    ),
                );
            }
        }

        // Staff-group / part / view structural references must resolve to
        // extant objects (Chapter 5 §"Top-Level Score Structure"): a staff's
        // group, a group's members, a part's staves, a view's active layers.
        let staff_groups: BTreeSet<crate::ids::StaffGroupId> =
            self.score.staff_groups.iter().map(|g| g.id).collect();
        for s in &self.score.staves {
            if let Some(group) = s.group {
                flag(
                    staff_groups.contains(&group),
                    format!("staff {:?} group {:?} is not declared", s.id, group),
                );
            }
        }
        for g in &self.score.staff_groups {
            for m in &g.members {
                flag(
                    self.declared_staves.contains(m),
                    format!(
                        "staff group {:?} member staff {:?} is not declared",
                        g.id, m
                    ),
                );
            }
        }
        for p in &self.score.parts {
            for s in &p.staves {
                flag(
                    self.declared_staves.contains(s),
                    format!("part {:?} staff {:?} is not declared", p.id, s),
                );
            }
        }
        for v in &self.score.views {
            for l in &v.active_layers {
                flag(
                    self.analysis_layers.contains(l),
                    format!("view {:?} active layer {:?} is not declared", v.id, l),
                );
            }
        }

        // Time-signature references must resolve to a declared `TimeSignature`
        // (Chapter 3 §"Time Signatures") — at every level a `MeterChange` can
        // appear: per-measure, instance-local grids, the region-default grid,
        // and the metric time model's own meter sequence.
        let time_sigs: BTreeSet<crate::ids::TimeSignatureId> =
            self.score.time_signatures.iter().map(|ts| ts.id).collect();
        for r in &self.score.canvas.regions {
            if let RegionTimeModel::Metric(m) = &r.time_model {
                for mc in &m.meters {
                    flag(
                        time_sigs.contains(&mc.time_signature),
                        format!(
                            "region {:?} time-model meter change time signature {:?} is not declared",
                            r.id, mc.time_signature
                        ),
                    );
                }
            }
            if let Some(c) = r.content.staff_based() {
                if let Some(g) = &c.default_metric_grid {
                    for mc in &g.meter_sequence {
                        flag(
                            time_sigs.contains(&mc.time_signature),
                            format!(
                                "region {:?} default-grid meter change time signature {:?} is not declared",
                                r.id, mc.time_signature
                            ),
                        );
                    }
                }
            }
            for si in r.staff_instances() {
                for m in &si.measures {
                    if let Some(ts) = m.time_signature {
                        flag(
                            time_sigs.contains(&ts),
                            format!("measure {:?} time signature {:?} is not declared", m.id, ts),
                        );
                    }
                }
                if let Some(g) = &si.local_metric_grid {
                    for mc in &g.meter_sequence {
                        flag(
                            time_sigs.contains(&mc.time_signature),
                            format!(
                                "instance {:?} meter change time signature {:?} is not declared",
                                si.id, mc.time_signature
                            ),
                        );
                    }
                }
            }
        }

        // Decomposition components' tuplet references must resolve (Chapter 3):
        // a dangling `TupletId` would otherwise be silently treated as "no
        // tuplet" (the component left unscaled), which can let an inconsistent
        // decomposition slip past invariant 15.
        for d in &self.score.decomposition_attachments {
            for c in &d.components {
                if let Some(t) = c.tuplet {
                    flag(
                        self.tuplet_ratios.contains_key(&t),
                        format!(
                            "decomposition of event {:?} references tuplet {:?}, which does not exist",
                            d.target, t
                        ),
                    );
                }
            }
        }

        // Event-internal references must resolve too (Chapter 5: the graph's
        // references resolve to extant objects). These are not cross-cutting
        // structures but they bear graph references that can dangle.
        for e in self.score.events.iter() {
            match e {
                Event::Indeterminate(ie) => {
                    for alt in &ie.hints.alternatives {
                        flag(
                            live_event(alt),
                            format!(
                                "indeterminate event {:?} alternative {:?} dangling",
                                ie.id, alt
                            ),
                        );
                    }
                }
                Event::Trajectory(te) => {
                    for ep in [&te.start, &te.end] {
                        if let crate::event::TrajectoryEndpoint::EventPitch(pid) = ep {
                            flag(
                                self.live_pitches.contains(pid),
                                format!("trajectory {:?} endpoint pitch {:?} dangling", te.id, pid),
                            );
                        }
                    }
                }
                Event::Graphic(ge) => {
                    for o in &ge.graphics {
                        flag(
                            self.graphic_objects.contains(o),
                            format!("graphic event {:?} object {:?} not stored", ge.id, o),
                        );
                    }
                }
                Event::Cue(ce) => {
                    for src in &ce.source {
                        flag(
                            live_event(src),
                            format!("cue {:?} source {:?} dangling", ce.id, src),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // --- Tempo-map well-formedness (Chapter 3 §"Tempo and the Tempo Map"). --
    //
    // The spec enumerates no dedicated tempo graph invariant, but the tempo
    // map's segment anchors are graph references and its segments carry
    // structural requirements. **Since P13-S29 these report under two different
    // arms**: the anchors stay invariant 10, while shape/`end_tempo`
    // compatibility reports `req:time:tempo-segment-shape` and ordering and
    // non-overlap report `req:time:tempo-segment-order`. Segment-anchor *offset*
    // agreement is invariant 9's (the anchors are in `collect_anchors`).
    fn check_tempo_maps(&self, out: &mut Vec<WellFormednessViolation>) {
        use crate::tempo::TempoShape;
        // P13-S29: anchor existence is invariant 10; shape, order and overlap
        // are Chapter 3 requirements and say so.
        let mut flag = |kind: ViolationKind, cond: bool, what: String| {
            if !cond {
                out.push(WellFormednessViolation {
                    kind,
                    witness: what,
                });
            }
        };
        // Self-contained musical position of a segment boundary: a region-start
        // anchor with a musical/zero offset (the natural region-local tempo
        // anchoring). Other anchors need the score timeline; ordering/overlap is
        // then skipped (sound but incomplete), never falsely flagged.
        let seg_pos = |a: &TimeAnchor| -> Option<crate::time::RationalTime> {
            match a {
                TimeAnchor::Region {
                    edge: crate::time::RegionEdge::Start,
                    offset: AnchorOffset::Musical(d),
                    ..
                } => Some(d.rational().clone()),
                TimeAnchor::Region {
                    edge: crate::time::RegionEdge::Start,
                    offset: AnchorOffset::Zero,
                    ..
                } => Some(crate::time::RationalTime::zero()),
                _ => None,
            }
        };
        for tm in self.tempo_maps() {
            let mut prev_start: Option<crate::time::RationalTime> = None;
            let mut prev_end: Option<crate::time::RationalTime> = None;
            for seg in &tm.segments {
                // Segment boundary anchor targets must resolve (invariant 10).
                flag(
                    ViolationKind::Invariant(GraphInvariant::CrossCuttingRefsResolve),
                    self.anchor_target_exists(&seg.start),
                    format!("tempo segment start anchor target {:?} dangling", seg.start),
                );
                if let Some(end) = &seg.end {
                    flag(
                        ViolationKind::Invariant(GraphInvariant::CrossCuttingRefsResolve),
                        self.anchor_target_exists(end),
                        format!("tempo segment end anchor target {end:?} dangling"),
                    );
                }
                // Missing end_tempo / shape consistency (Chapter 3).
                match seg.shape {
                    TempoShape::Constant => flag(
                        ViolationKind::Requirement("req:time:tempo-segment-shape"),
                        seg.end_tempo
                            .as_ref()
                            .is_none_or(|et| et == &seg.start_tempo),
                        "constant tempo segment has end_tempo != start_tempo".to_string(),
                    ),
                    TempoShape::Linear | TempoShape::Exponential | TempoShape::Curve => flag(
                        ViolationKind::Requirement("req:time:tempo-segment-shape"),
                        seg.end_tempo.is_some(),
                        "non-constant tempo segment is missing its end_tempo".to_string(),
                    ),
                }
                // Ordering and non-overlap, where resolvable.
                let start = seg_pos(&seg.start);
                if let (Some(ps), Some(s)) = (&prev_start, &start) {
                    flag(
                        ViolationKind::Requirement("req:time:tempo-segment-order"),
                        s >= ps,
                        "tempo segments are out of start order".to_string(),
                    );
                }
                if let (Some(pe), Some(s)) = (&prev_end, &start) {
                    flag(
                        ViolationKind::Requirement("req:time:tempo-segment-order"),
                        s >= pe,
                        "tempo segments overlap in musical time".to_string(),
                    );
                }
                prev_end = seg.end.as_ref().and_then(seg_pos).or_else(|| start.clone());
                prev_start = start;
            }
        }
    }

    // --- Aleatoric ordering / bounds well-formedness (Chapter 3 §"Aleatoric
    // Time"). The ordering DAG and the per-event bounds map are graph
    // references: they must name events that exist *in the region*, and each
    // bound window must be ordered (`min <= max`). **Since P13-S29 the locality
    // rule reports `req:time:aleatoric-reference-locality`**, not invariant 10 —
    // it asks about region membership, which is stronger than existence. A
    // reversed window remains a region-time-model defect (invariant 4). (The DAG's acyclicity is enforced at construction in `graph`.)
    fn check_aleatoric_models(&self, out: &mut Vec<WellFormednessViolation>) {
        for r in &self.score.canvas.regions {
            let RegionTimeModel::Aleatoric(model) = &r.time_model else {
                continue;
            };
            let in_region = |e: EventId| {
                self.event_instance
                    .get(&e)
                    .and_then(|si| self.instance_region.get(si))
                    .map(|rid| *rid == r.id)
                    .unwrap_or(false)
            };
            for e in model.ordering.referenced_events() {
                if !in_region(e) {
                    out.push(WellFormednessViolation::requirement(
                        "req:time:aleatoric-reference-locality",
                        format!(
                            "aleatoric region {:?} ordering references event {:?}, absent from the region",
                            r.id, e
                        ),
                    ));
                }
            }
            for (e, bounds) in &model.bounds {
                if !in_region(*e) {
                    out.push(WellFormednessViolation::requirement(
                        "req:time:aleatoric-reference-locality",
                        format!(
                            "aleatoric region {:?} bounds key event {:?} is absent from the region",
                            r.id, e
                        ),
                    ));
                }
                for tb in [bounds.start.as_ref(), bounds.end.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    if time_bounds_ordered(tb) == Some(false) {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::EventCoordinateModel,
                            format!(
                                "aleatoric region {:?} has a reversed (min > max) bound for event {:?}",
                                r.id, e
                            ),
                        ));
                    }
                }
            }
        }
    }

    // --- Accidental modification / pitch-space compatibility (Chapter 4
    // §"Accidental Registries", `req:tuning:accidental-modification-compatibility`;
    // Push 4b tranche 3a, `spec/CONTRACT_PUSH4B_ACCIDENTALS.md`). Not one of the
    // spec-enumerated Chapter 5 graph invariants, and **since P13-S29 it no
    // longer borrows one**: it reports under its own requirement label, which is
    // what it always meant. The witness does not restate that label — `Display`
    // renders it as the prefix.
    fn check_accidental_modification_compatibility(&self, out: &mut Vec<WellFormednessViolation>) {
        // Every pitch space the score's tuning context concretely
        // references: the default, plus any per-scope override's pitch
        // space (`crate::tuning::TuningOverride::pitch_space`). This
        // tranche has no built-in catalog linking an `AccidentalRegistryId`
        // to the pitch space(s) that declare it as their
        // `accidental_registry` (Push 4b tranche 1 built only
        // `built_in_position_structure`'s id -> structure map, not a full
        // `PitchSpace` catalog with that field populated) — so this is the
        // referencing relation the score can actually attest to, honestly,
        // rather than inventing catalog data (the `NOTEHEAD_ANCHORS`
        // failure this project has already paid for twice).
        let mut spaces: BTreeSet<&PitchSpaceId> = BTreeSet::new();
        spaces.insert(&self.score.tuning_context.default_pitch_space);
        for ov in &self.score.tuning_context.overrides {
            if let Some(space) = &ov.pitch_space {
                spaces.insert(space);
            }
        }
        for ext in &self.score.tuning_context.accidental_extensions {
            for def in ext.additions.iter().chain(ext.overrides.iter()) {
                for space in &spaces {
                    if !crate::accidental::accidental_modification_compatible_with_space(
                        &def.modification,
                        space,
                    ) {
                        out.push(WellFormednessViolation::requirement(
                            "req:tuning:accidental-modification-compatibility",
                            format!(
                                "accidental {:?} (registry {:?}) modification {:?} is not \
                                 expressible in pitch space {:?}'s interval algebra",
                                def.id, ext.base, def.modification, space
                            ),
                        ));
                    }
                }
            }
        }
    }

    // --- 11. Identifiers unique within their kind. --------------------------
    fn check_unique_identifiers(&self, out: &mut Vec<WellFormednessViolation>) {
        let mut regions = BTreeSet::new();
        for r in &self.score.canvas.regions {
            if !regions.insert(r.id) {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::UniqueIdentifiers,
                    format!("region id {:?} is used twice", r.id),
                ));
            }
        }
        let mut instances = BTreeSet::new();
        let mut voices = BTreeSet::new();
        let mut measures = BTreeSet::new();
        for r in &self.score.canvas.regions {
            for si in r.staff_instances() {
                if !instances.insert(si.id) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::UniqueIdentifiers,
                        format!("staff-instance id {:?} is used twice", si.id),
                    ));
                }
                for v in &si.voices {
                    if !voices.insert(v.id) {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::UniqueIdentifiers,
                            format!("voice id {:?} is used twice", v.id),
                        ));
                    }
                }
                for m in &si.measures {
                    if !measures.insert(m.id) {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::UniqueIdentifiers,
                            format!("measure id {:?} is used twice", m.id),
                        ));
                    }
                }
            }
        }
        let mut staves = BTreeSet::new();
        for s in &self.score.staves {
            if !staves.insert(s.id) {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::UniqueIdentifiers,
                    format!("staff id {:?} is used twice", s.id),
                ));
            }
        }

        // Cross-cutting structure ids, each unique within its kind.
        let cc = &self.score.cross_cutting;
        let mut dup = |used: bool, what: String| {
            if used {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::UniqueIdentifiers,
                    what,
                ));
            }
        };
        let mut slurs = BTreeSet::new();
        for x in &cc.slurs {
            dup(
                !slurs.insert(x.id),
                format!("slur id {:?} is used twice", x.id),
            );
        }
        let mut ties = BTreeSet::new();
        for x in &cc.ties {
            dup(
                !ties.insert(x.id),
                format!("tie id {:?} is used twice", x.id),
            );
        }
        let mut beams = BTreeSet::new();
        for x in &cc.beams {
            dup(
                !beams.insert(x.id),
                format!("beam id {:?} is used twice", x.id),
            );
        }
        let mut spanners = BTreeSet::new();
        for x in &cc.spanners {
            dup(
                !spanners.insert(x.id),
                format!("spanner id {:?} is used twice", x.id),
            );
        }
        let mut tuplets = BTreeSet::new();
        for x in &cc.tuplets {
            dup(
                !tuplets.insert(x.id),
                format!("tuplet id {:?} is used twice", x.id),
            );
        }
        let mut groups = BTreeSet::new();
        for r in &self.score.canvas.regions {
            for g in r.content.barline_alignment_groups() {
                dup(
                    !groups.insert(g.id),
                    format!("barline-group id {:?} is used twice", g.id),
                );
            }
        }
        let mut markers = BTreeSet::new();
        for x in &cc.markers {
            dup(
                !markers.insert(x.id),
                format!("marker id {:?} is used twice", x.id),
            );
        }
        let mut repeats = BTreeSet::new();
        for x in &cc.repeats {
            dup(
                !repeats.insert(x.id),
                format!("repeat id {:?} is used twice", x.id),
            );
        }
        let mut analytical = BTreeSet::new();
        for x in &cc.analytical {
            dup(
                !analytical.insert(x.id),
                format!("annotation id {:?} is used twice", x.id),
            );
        }
        let mut comments = BTreeSet::new();
        for x in &cc.comments {
            dup(
                !comments.insert(x.id),
                format!("comment id {:?} is used twice", x.id),
            );
        }
        let mut gestures = BTreeSet::new();
        for x in &cc.graphic_gestures {
            dup(
                !gestures.insert(x.id),
                format!("gesture id {:?} is used twice", x.id),
            );
        }
        let mut lyrics = BTreeSet::new();
        for x in &cc.lyrics {
            dup(
                !lyrics.insert(x.id),
                format!("lyric-line id {:?} is used twice", x.id),
            );
        }
        let mut chords = BTreeSet::new();
        for x in &cc.chord_symbols {
            dup(
                !chords.insert(x.id),
                format!("chord-symbol id {:?} is used twice", x.id),
            );
        }
        let mut graphic_obj = BTreeSet::new();
        for r in &self.score.canvas.regions {
            for o in r.content.graphic_objects() {
                dup(
                    !graphic_obj.insert(o.id),
                    format!("graphic-object id {:?} is used twice", o.id),
                );
            }
        }

        // Top-level object kinds.
        let mut instruments = BTreeSet::new();
        for x in &self.score.instruments {
            dup(
                !instruments.insert(x.id),
                format!("instrument id {:?} is used twice", x.id),
            );
        }
        let mut staff_groups = BTreeSet::new();
        for x in &self.score.staff_groups {
            dup(
                !staff_groups.insert(x.id),
                format!("staff-group id {:?} is used twice", x.id),
            );
        }
        let mut parts = BTreeSet::new();
        for x in &self.score.parts {
            dup(
                !parts.insert(x.id),
                format!("part id {:?} is used twice", x.id),
            );
        }
        let mut layers = BTreeSet::new();
        for x in &self.score.analysis_layers {
            dup(
                !layers.insert(x.id),
                format!("analysis-layer id {:?} is used twice", x.id),
            );
        }
        let mut views = BTreeSet::new();
        for x in &self.score.views {
            dup(
                !views.insert(x.id),
                format!("view id {:?} is used twice", x.id),
            );
        }
        let mut time_sig_ids = BTreeSet::new();
        for x in &self.score.time_signatures {
            dup(
                !time_sig_ids.insert(x.id),
                format!("time-signature id {:?} is used twice", x.id),
            );
        }

        // Identifier stability: a live id must not also be tombstoned (the
        // identifier would have been reused, which Chapter 5 §"Identifier
        // Stability" forbids — "never reassigned, even after deletion").
        for e in self.score.events.ids_canonical() {
            if self.score.tombstoned_events.contains(&e) {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::UniqueIdentifiers,
                    format!("event id {:?} is both live and tombstoned", e),
                ));
            }
        }
        for p in &self.live_pitches {
            if self.score.tombstoned_pitches.contains(p) {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::UniqueIdentifiers,
                    format!("pitch id {:?} is both live and tombstoned", p),
                ));
            }
        }

        // The SYSTEM_DERIVED replica namespace is reserved for
        // deterministically-derived system identifiers. Only two object kinds
        // legitimately use it: system-promoted voices (checked by invariant 18)
        // and system-derived synthetic pitches (`MUSCSPCH`, Chapter 5). Every
        // *other* kind in that namespace is misuse.
        let mut sysmisuse = |used: bool, what: String| {
            if used {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::UniqueIdentifiers,
                    what,
                ));
            }
        };
        for r in &self.score.canvas.regions {
            sysmisuse(
                r.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!("region {:?} uses the reserved SYSTEM_DERIVED replica", r.id),
            );
            for si in r.staff_instances() {
                sysmisuse(
                    si.id.replica() == ReplicaId::SYSTEM_DERIVED,
                    format!(
                        "staff instance {:?} uses the reserved SYSTEM_DERIVED replica",
                        si.id
                    ),
                );
                for m in &si.measures {
                    sysmisuse(
                        m.id.replica() == ReplicaId::SYSTEM_DERIVED,
                        format!(
                            "measure {:?} uses the reserved SYSTEM_DERIVED replica",
                            m.id
                        ),
                    );
                }
            }
        }
        for s in &self.score.staves {
            sysmisuse(
                s.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!("staff {:?} uses the reserved SYSTEM_DERIVED replica", s.id),
            );
        }
        for e in self.score.events.ids_canonical() {
            sysmisuse(
                e.replica() == ReplicaId::SYSTEM_DERIVED,
                format!("event {:?} uses the reserved SYSTEM_DERIVED replica", e),
            );
        }
        // The authoring identity context itself must not be the reserved
        // namespace (Chapter 5 §"System-Derived Identifier Namespace":
        // user-authored replicas MUST NOT use SYSTEM_DERIVED), since every id it
        // mints would otherwise land there.
        sysmisuse(
            self.score.identity.replica_id == ReplicaId::SYSTEM_DERIVED,
            "score identity uses the reserved SYSTEM_DERIVED replica".to_string(),
        );
        // The remaining object kinds with *no* system-derived form: any use of
        // the reserved namespace is misuse. (Embedded `PitchId`s are NOT checked
        // here — a `MUSCSPCH` synthetic pitch legitimately lives in the
        // namespace; voices are invariant 18's domain.)
        for i in &self.score.instruments {
            sysmisuse(
                i.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!(
                    "instrument {:?} uses the reserved SYSTEM_DERIVED replica",
                    i.id
                ),
            );
        }
        for g in &self.score.staff_groups {
            sysmisuse(
                g.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!(
                    "staff group {:?} uses the reserved SYSTEM_DERIVED replica",
                    g.id
                ),
            );
        }
        for p in &self.score.parts {
            sysmisuse(
                p.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!("part {:?} uses the reserved SYSTEM_DERIVED replica", p.id),
            );
        }
        for l in &self.score.analysis_layers {
            sysmisuse(
                l.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!(
                    "analysis layer {:?} uses the reserved SYSTEM_DERIVED replica",
                    l.id
                ),
            );
        }
        for v in &self.score.views {
            sysmisuse(
                v.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!("view {:?} uses the reserved SYSTEM_DERIVED replica", v.id),
            );
        }
        let cc = &self.score.cross_cutting;
        for id_is_sys in cc
            .slurs
            .iter()
            .map(|x| (x.id.replica(), "slur"))
            .chain(cc.ties.iter().map(|x| (x.id.replica(), "tie")))
            .chain(cc.beams.iter().map(|x| (x.id.replica(), "beam")))
            .chain(cc.spanners.iter().map(|x| (x.id.replica(), "spanner")))
            .chain(cc.tuplets.iter().map(|x| (x.id.replica(), "tuplet")))
            .chain(cc.markers.iter().map(|x| (x.id.replica(), "marker")))
            .chain(cc.repeats.iter().map(|x| (x.id.replica(), "repeat")))
            .chain(cc.analytical.iter().map(|x| (x.id.replica(), "annotation")))
            .chain(cc.comments.iter().map(|x| (x.id.replica(), "comment")))
            .chain(
                cc.graphic_gestures
                    .iter()
                    .map(|x| (x.id.replica(), "gesture")),
            )
            .chain(cc.lyrics.iter().map(|x| (x.id.replica(), "lyric")))
            .chain(
                cc.chord_symbols
                    .iter()
                    .map(|x| (x.id.replica(), "chord symbol")),
            )
        {
            sysmisuse(
                id_is_sys.0 == ReplicaId::SYSTEM_DERIVED,
                format!(
                    "{} id uses the reserved SYSTEM_DERIVED replica",
                    id_is_sys.1
                ),
            );
        }
        // Time-signature, barline-group, and graphic-object ids have no
        // system-derived form, so any use of the reserved namespace is misuse.
        for ts in &self.score.time_signatures {
            sysmisuse(
                ts.id.replica() == ReplicaId::SYSTEM_DERIVED,
                format!(
                    "time signature {:?} uses the reserved SYSTEM_DERIVED replica",
                    ts.id
                ),
            );
        }
        for r in &self.score.canvas.regions {
            for g in r.content.barline_alignment_groups() {
                sysmisuse(
                    g.id.replica() == ReplicaId::SYSTEM_DERIVED,
                    format!(
                        "barline group {:?} uses the reserved SYSTEM_DERIVED replica",
                        g.id
                    ),
                );
            }
            for o in r.content.graphic_objects() {
                sysmisuse(
                    o.id.replica() == ReplicaId::SYSTEM_DERIVED,
                    format!(
                        "graphic object {:?} uses the reserved SYSTEM_DERIVED replica",
                        o.id
                    ),
                );
            }
        }

        // A `SYSTEM_DERIVED` embedded pitch is legitimate only if it *proves* its
        // namespace: its counter must equal the deterministic `MUSCSPCH`
        // derivation of its own content (Chapter 5 §"System-Derived
        // Identifiers"). An arbitrary counter in the reserved namespace is
        // misuse — provenance-aware validation, not unconditional acceptance.
        for pid in &self.live_pitches {
            if pid.replica() == ReplicaId::SYSTEM_DERIVED {
                if let Some(pitch) = self.pitch.get(pid) {
                    sysmisuse(
                        *pid != crate::pitch::derive_system_pitch_id(pitch),
                        format!(
                            "system-derived pitch {pid:?} is not the MUSCSPCH derivation of its content"
                        ),
                    );
                }
            }
        }

        // Arena identity integrity: the index must agree with each event's own
        // id, and no live pitched event may be empty. `insert` enforces both,
        // but `get_mut` exposes the fields, so re-check here (catches an id or
        // pitch-list mutated after insertion).
        for id in self.score.events.index_inconsistencies() {
            out.push(WellFormednessViolation::invariant(
                GraphInvariant::UniqueIdentifiers,
                format!("arena index entry {id:?} disagrees with the stored event's id"),
            ));
        }
        for id in self.score.events.malformed_pitched_events() {
            out.push(WellFormednessViolation::invariant(
                GraphInvariant::UniqueIdentifiers,
                format!("pitched event {id:?} has no pitches (malformed; Chapter 5)"),
            ));
        }
    }

    // --- 12. Embedded PitchId uniqueness. -----------------------------------
    fn check_pitch_id_unique(&self, out: &mut Vec<WellFormednessViolation>) {
        let mut seen: BTreeSet<PitchId> = BTreeSet::new();
        let mut buf = Vec::new();
        for e in self.score.events.iter() {
            buf.clear();
            e.collect_identified_pitches(&mut buf);
            for ip in &buf {
                if !seen.insert(ip.id) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::PitchIdUnique,
                        format!("pitch id {:?} appears more than once", ip.id),
                    ));
                }
            }
        }
    }

    // --- 13. SpellingScope::Pitch resolves to live or tombstoned. -----------
    fn check_spelling_scope_resolves(&self, out: &mut Vec<WellFormednessViolation>) {
        for a in &self.score.spelling_attachments {
            if let SpellingScope::Pitch(pid) = &a.scope {
                let live = self.live_pitches.contains(pid);
                let tomb = self.score.tombstoned_pitches.contains(pid);
                if !live && !tomb {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::SpellingScopeResolves,
                        format!(
                            "spelling attachment targets pitch {:?}, neither live nor tombstoned",
                            pid
                        ),
                    ));
                }
            }
            // Explicit directives are only valid with a Pitch scope (Chapter 2);
            // surfacing the malformed pairing here keeps the attachment honest.
            if matches!(
                (&a.scope, &a.directive),
                (SpellingScope::Range { .. }, SpellingDirective::Explicit(_))
            ) {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::SpellingScopeResolves,
                    "explicit spelling on a range scope (only valid with a pitch scope)"
                        .to_string(),
                ));
            }
            // An explicit spelling's accidental stack must be well-formed: no
            // repeated `AccidentalId` (Chapter 2 §"Accidental Stack Semantics").
            // Wires `PitchSpelling::accidental_stack_is_well_formed` into
            // validation rather than leaving it advisory.
            if let SpellingDirective::Explicit(sp) = &a.directive {
                if !sp.accidental_stack_is_well_formed() {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::SpellingScopeResolves,
                        format!(
                            "spelling attachment has a repeated accidental in its stack: {:?}",
                            sp.accidentals
                        ),
                    ));
                }
            }
            // The attachment's analysis layer (if any) must resolve to a declared
            // `AnalysisLayer` (Chapter 5 §"Analysis Layers and Views").
            if let Some(layer) = a.layer {
                if !self.analysis_layers.contains(&layer) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::SpellingScopeResolves,
                        format!(
                            "spelling attachment layer {layer:?} is not a declared analysis layer"
                        ),
                    ));
                }
            }
        }
    }

    // --- 14. Decomposition target resolves to live or tombstoned. -----------
    fn check_decomposition_target_resolves(&self, out: &mut Vec<WellFormednessViolation>) {
        for d in &self.score.decomposition_attachments {
            let live = self.score.events.contains(d.target);
            let tomb = self.score.tombstoned_events.contains(&d.target);
            if !live && !tomb {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::DecompositionTargetResolves,
                    format!(
                        "decomposition targets event {:?}, neither live nor tombstoned",
                        d.target
                    ),
                ));
            }
        }
    }

    // --- 15. Live decomposition component sum == event duration. ------------
    fn check_decomposition_sum(&self, out: &mut Vec<WellFormednessViolation>) {
        for d in &self.score.decomposition_attachments {
            let Some(ev) = self.score.events.get(d.target) else {
                continue; // tombstoned target: invariant 14 territory
            };
            let EventDuration::Musical(dur) = ev.duration() else {
                continue; // only musical durations decompose into note values
            };
            // Sum each component's *sounding* duration: its notated value with
            // dots, scaled by its tuplet's ratio when it is in one (Chapter 3).
            let mut sum = crate::time::MusicalDuration::zero();
            for c in &d.components {
                let ratio = c.tuplet.and_then(|t| self.tuplet_ratios.get(&t).copied());
                sum = sum + c.sounding_duration(ratio);
            }
            if &sum != dur {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::DecompositionSum,
                    format!(
                        "decomposition of event {:?} sums to {:?}, event duration is {:?}",
                        d.target, sum, dur
                    ),
                ));
            }
        }
    }

    // --- 16. Tuplet member durations sum to required total. -----------------
    fn check_tuplet_sum(&self, out: &mut Vec<WellFormednessViolation>) {
        for t in &self.score.cross_cutting.tuplets {
            // Degenerate ratios (a zero term or actual == notated) are rejected
            // at construction by `TupletRatio::new` (Chapter 3 §"Tuplets",
            // `req:time:tuplet-ratio-construction`), so they are never a
            // representable graph state and are not re-checked here.
            //
            // Sum the members' musical durations; skip members that are absent
            // (invariant 10) or non-musical (cannot contribute to a rational
            // total).
            let mut sum = crate::time::MusicalDuration::zero();
            let mut measurable = !t.members.is_empty();
            for e in &t.members {
                match self.score.events.get(*e).map(|ev| ev.duration()) {
                    Some(EventDuration::Musical(d)) => sum = sum + d.clone(),
                    Some(_) => measurable = false,
                    None => measurable = false,
                }
            }
            if measurable && sum != t.required_total {
                out.push(WellFormednessViolation::invariant(
                    GraphInvariant::TupletSum,
                    format!(
                        "tuplet {:?} members sum to {:?}, required total is {:?}",
                        t.id, sum, t.required_total
                    ),
                ));
            }

            // Ratio consistency (Chapter 3 §"Tuplet Consistency"): the
            // actual:notated ratio MUST relate the members' *notation* to their
            // sounding duration. For each member whose entire notational
            // decomposition lies in this tuplet, scaling its notated duration by
            // `notated/actual` MUST reproduce its sounding (event) duration —
            // so a wrong ratio (e.g. 3:2 changed to 5:4) is caught. Members
            // without an in-tuplet decomposition are skipped (sound but
            // incomplete; the decomposition pre-pass is deferred).
            if t.ratio.actual() != 0 && t.ratio.notated() != 0 {
                let scale = crate::time::RationalTime::new(
                    t.ratio.notated() as i64,
                    t.ratio.actual() as i64,
                )
                .expect("nonzero ratio");
                for &member in &t.members {
                    let comps: Vec<&crate::graph::NotatedComponent> = self
                        .score
                        .decomposition_attachments
                        .iter()
                        .filter(|d| d.target == member)
                        .flat_map(|d| d.components.iter())
                        .collect();
                    if comps.is_empty() || !comps.iter().all(|c| c.tuplet == Some(t.id)) {
                        continue;
                    }
                    let Some(EventDuration::Musical(sd)) =
                        self.score.events.get(member).map(|ev| ev.duration())
                    else {
                        continue;
                    };
                    let mut notated = crate::time::RationalTime::zero();
                    for c in &comps {
                        let nd = c.notated_duration();
                        notated = notated.add(nd.rational());
                    }
                    let sounding = notated.mul(&scale);
                    if &sounding != sd.rational() {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::TupletSum,
                            format!(
                                "tuplet {:?} ratio {}:{} is inconsistent with member {:?}'s notation \
                                 (notated scaled to {:?}, sounding duration is {:?})",
                                t.id, t.ratio.actual(), t.ratio.notated(), member, sounding, sd
                            ),
                        ));
                    }
                }
            }
        }
    }

    // --- 17. Tie pairing references and class rules. ------------------------
    fn check_tie_pairing(&self, out: &mut Vec<WellFormednessViolation>) {
        for t in &self.score.cross_cutting.ties {
            let empty = BTreeSet::new();
            let start_pitches = self.event_pitches.get(&t.start_event).unwrap_or(&empty);
            let end_pitches = self.event_pitches.get(&t.end_event).unwrap_or(&empty);
            let requires_enharmonic = matches!(
                t.class,
                TieClass::Standard | TieClass::Editorial | TieClass::CrossVoice
            );

            match &t.pitch_pairing {
                Some(pairs) => {
                    // Explicit pairing: each entry must reference pitches of the
                    // respective events, and be enharmonic for the classes that
                    // require it.
                    for (sp, ep) in pairs {
                        if !start_pitches.contains(sp) {
                            out.push(WellFormednessViolation::invariant(
                                GraphInvariant::TiePairing,
                                format!("tie {:?} pairs pitch {:?} not in start event", t.id, sp),
                            ));
                        }
                        if !end_pitches.contains(ep) {
                            out.push(WellFormednessViolation::invariant(
                                GraphInvariant::TiePairing,
                                format!("tie {:?} pairs pitch {:?} not in end event", t.id, ep),
                            ));
                        }
                        if requires_enharmonic {
                            if let (Some(a), Some(b)) = (self.pitch.get(sp), self.pitch.get(ep)) {
                                if !a.enharmonic_equivalent(b) {
                                    out.push(WellFormednessViolation::invariant(
                                        GraphInvariant::TiePairing,
                                        format!(
                                            "tie {:?} pairs non-enharmonic pitches {:?}/{:?}",
                                            t.id, sp, ep
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
                None => {
                    // Implicit pairing: "all pitches tied by enharmonic matching
                    // in pitch-id-ascending order" (Chapter 5 §"Ties"). Each
                    // start pitch (ascending) is greedily matched to the
                    // lowest-id not-yet-used enharmonically-equivalent end pitch
                    // — a deterministic matching that survives chord reordering,
                    // not a positional zip.
                    if start_pitches.len() != end_pitches.len() {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::TiePairing,
                            format!(
                                "tie {:?} (implicit pairing): {} start vs {} end pitches",
                                t.id,
                                start_pitches.len(),
                                end_pitches.len()
                            ),
                        ));
                    }
                    let mut used: BTreeSet<PitchId> = BTreeSet::new();
                    for sp in start_pitches {
                        let sp_pitch = self.pitch.get(sp);
                        let matched = end_pitches.iter().find(|ep| {
                            if used.contains(*ep) {
                                return false;
                            }
                            match (sp_pitch, self.pitch.get(*ep)) {
                                (Some(a), Some(b)) => a.enharmonic_equivalent(b),
                                _ => false,
                            }
                        });
                        match matched {
                            Some(ep) => {
                                used.insert(*ep);
                            }
                            None => out.push(WellFormednessViolation::invariant(
                                GraphInvariant::TiePairing,
                                format!(
                                    "tie {:?} (implicit pairing): start pitch {:?} has no enharmonic end-event counterpart",
                                    t.id, sp
                                ),
                            )),
                        }
                    }
                }
            }
            // Class-specific adjacency / voice / position rules.
            self.check_tie_class_rules(t, out);
        }
    }

    /// An event's start position as a comparable key (for tie ordering rules).
    fn event_start_key(&self, eid: EventId) -> Option<TimeKey> {
        Endpoints::of(self.score.events.get(eid)?).start_key()
    }

    fn check_tie_class_rules(&self, t: &crate::graph::Tie, out: &mut Vec<WellFormednessViolation>) {
        let start = self.event_voice_index.get(&t.start_event);
        let end = self.event_voice_index.get(&t.end_event);
        match t.class {
            TieClass::Standard => {
                if let (Some((sv, si)), Some((ev, ei))) = (start, end) {
                    if sv != ev || *ei != si + 1 {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::TiePairing,
                            format!(
                                "standard tie {:?} is not same-voice immediately adjacent",
                                t.id
                            ),
                        ));
                    }
                }
            }
            TieClass::Editorial => {
                if let (Some((sv, si)), Some((ev, ei))) = (start, end) {
                    if sv != ev || ei <= si {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::TiePairing,
                            format!("editorial tie {:?} is not same-voice forward", t.id),
                        ));
                    }
                }
            }
            TieClass::CrossVoice => {
                let sinst = self.event_instance.get(&t.start_event);
                let einst = self.event_instance.get(&t.end_event);
                if let (Some(a), Some(b)) = (sinst, einst) {
                    if a != b {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::TiePairing,
                            format!("cross-voice tie {:?} crosses staff instances", t.id),
                        ));
                    }
                }
                // The start's resolved position MUST be <= the end's
                // (Chapter 5 §"Ties", CrossVoice). Comparable when both events
                // share a clock; skipped otherwise (invariant 4's concern).
                if let (Some(s), Some(e)) = (
                    self.event_start_key(t.start_event),
                    self.event_start_key(t.end_event),
                ) {
                    if !s.le_same_clock(&e) {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::TiePairing,
                            format!("cross-voice tie {:?} has start position after end", t.id),
                        ));
                    }
                }
            }
            TieClass::LaissezVibrer | TieClass::Registered(_) => {}
        }
    }

    // --- 18. Voice origin consistency / promoted-id derivation. -------------
    fn check_voice_origin_consistent(&self, out: &mut Vec<WellFormednessViolation>) {
        for (_r, si, v) in self.score.voices() {
            match &v.origin {
                VoiceOrigin::SystemPromoted {
                    winning_operation,
                    losing_operation,
                    original_voice,
                } => {
                    // A promoted voice's id MUST be the deterministic derivation
                    // (Chapter 5 §"System-Promoted Voices") from the complete
                    // provenance retained on the graph object.
                    let expected = derive_promoted_voice_id(
                        si,
                        *original_voice,
                        *winning_operation,
                        *losing_operation,
                    );
                    if v.id != expected {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::VoiceOriginConsistent,
                            format!(
                                "system-promoted voice {:?} != its derivation {:?}",
                                v.id, expected
                            ),
                        ));
                    }
                }
                VoiceOrigin::UserDeclared | VoiceOrigin::Imported { .. } => {
                    if v.id.replica() == ReplicaId::SYSTEM_DERIVED {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::VoiceOriginConsistent,
                            format!(
                                "user/imported voice {:?} uses the reserved SYSTEM_DERIVED replica",
                                v.id
                            ),
                        ));
                    }
                }
            }
        }
    }

    // --- 19. Barline group members stay within one region. ------------------
    fn check_barline_group_same_region(&self, out: &mut Vec<WellFormednessViolation>) {
        for r in &self.score.canvas.regions {
            let region_instances: BTreeSet<StaffInstanceId> =
                r.staff_instances().iter().map(|si| si.id).collect();
            let instance_measures: HashMap<StaffInstanceId, BTreeSet<MeasureId>> = r
                .staff_instances()
                .iter()
                .map(|si| (si.id, si.measures.iter().map(|m| m.id).collect()))
                .collect();
            for g in r.content.barline_alignment_groups() {
                for m in &g.members {
                    if !region_instances.contains(&m.staff_instance) {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::BarlineGroupSameRegion,
                            format!(
                                "barline group {:?} member instance {:?} is outside region {:?}",
                                g.id, m.staff_instance, r.id
                            ),
                        ));
                        continue;
                    }
                    if !instance_measures
                        .get(&m.staff_instance)
                        .map(|ms| ms.contains(&m.measure))
                        .unwrap_or(false)
                    {
                        out.push(WellFormednessViolation::invariant(
                            GraphInvariant::BarlineGroupSameRegion,
                            format!(
                                "barline group {:?} measure {:?} is not in instance {:?}",
                                g.id, m.measure, m.staff_instance
                            ),
                        ));
                    }
                }
            }
        }
    }

    // --- 20. Measure-meter agreement and boundary consistency (Genesis
    // tranche G3b, `spec/CONTRACT_GENESIS_G3B_MEASURE.md`). This mirrors
    // `epiphany-ops::reduce::Reducer`'s pin 6/6b/6c relation as an
    // INDEPENDENT implementation over the MATERIALIZED graph: there are no
    // operational write chains to reconstruct here (the architecture note in
    // the contract) — a `StaffInstance`'s effective grid is simply its own
    // `local_metric_grid`, falling back WHOLE to the enclosing region's
    // `default_metric_grid`, both already fully-resolved `MetricGrid`
    // values once persisted, and c3's "vector index" is literally
    // `StaffInstance.measures`' index, with no `minted_by` indirection.
    // Divergence from the ops-crate implementation is guarded by
    // `epiphany-testkit`'s cross-crate agreement test
    // (`measure_anchor_relation` above), not by code sharing — the
    // dependency direction (`epiphany-ops` -> `epiphany-core`, never the
    // reverse) forbids this crate from calling into that one.

    /// Pin 6: two `AnchorOffset`s are comparable iff they are the same
    /// clock, or at least one is `Zero` — read as the additive identity of
    /// whichever clock it is compared against. `Musical` against
    /// `WallClock` is never comparable (the deferred wall-clock/musical
    /// reconciliation).
    fn measure20_offset_order(a: &AnchorOffset, b: &AnchorOffset) -> Option<Ordering> {
        match (a, b) {
            (AnchorOffset::Musical(x), AnchorOffset::Musical(y)) => Some(x.cmp(y)),
            (AnchorOffset::WallClock(x), AnchorOffset::WallClock(y)) => Some(x.cmp(y)),
            (AnchorOffset::Zero, AnchorOffset::Zero) => Some(Ordering::Equal),
            (AnchorOffset::Zero, AnchorOffset::Musical(y)) => Some(MusicalDuration::zero().cmp(y)),
            (AnchorOffset::Musical(x), AnchorOffset::Zero) => Some(x.cmp(&MusicalDuration::zero())),
            (AnchorOffset::Zero, AnchorOffset::WallClock(y)) => Some(WallClockDuration(0).cmp(y)),
            (AnchorOffset::WallClock(x), AnchorOffset::Zero) => Some(x.cmp(&WallClockDuration(0))),
            (AnchorOffset::Musical(_), AnchorOffset::WallClock(_))
            | (AnchorOffset::WallClock(_), AnchorOffset::Musical(_)) => None,
        }
    }

    /// c3's "vector index" ordering between two DISTINCT measures, both
    /// anchored via `Measure{pos: Start, off: Zero}` (contract pin 6, c3):
    /// their relative position within the SAME `StaffInstance.measures`,
    /// read directly off the materialized graph — no ledger indirection
    /// needed (unlike `epiphany-ops`'s base-free branch, which has no
    /// materialized graph to read at all).
    fn measure20_vector_order(&self, a: MeasureId, b: MeasureId) -> Option<Ordering> {
        for region in &self.score.canvas.regions {
            for instance in region.staff_instances() {
                let pos_a = instance.measures.iter().position(|m| m.id == a);
                let pos_b = instance.measures.iter().position(|m| m.id == b);
                if let (Some(pa), Some(pb)) = (pos_a, pos_b) {
                    return Some(pa.cmp(&pb));
                }
            }
        }
        None
    }

    /// Pin 6: the comparable relation over `TimeAnchor`s, EXACTLY the five
    /// shapes c1-c5 (contract table). Everything else is NOT comparable,
    /// and no other relation may be invented — pin 6's prohibition. The
    /// boundary selector (`MeasurePosition`/`RegionEdge`) must be
    /// IDENTICAL; it is never ordered.
    fn measure20_comparable_order(&self, a: &TimeAnchor, b: &TimeAnchor) -> Option<Ordering> {
        match (a, b) {
            // c1: same Event id.
            (
                TimeAnchor::Event { id: ia, offset: oa },
                TimeAnchor::Event { id: ib, offset: ob },
            ) if ia == ib => Self::measure20_offset_order(oa, ob),
            // c2/c3: Measure anchors.
            (
                TimeAnchor::Measure {
                    id: ia,
                    position: pa,
                    offset: oa,
                },
                TimeAnchor::Measure {
                    id: ib,
                    position: pb,
                    offset: ob,
                },
            ) => {
                if pa != pb {
                    return None;
                }
                if ia == ib {
                    // c2: same id and same pos.
                    return Self::measure20_offset_order(oa, ob);
                }
                // c3: distinct ids, restricted to pos: Start, off: Zero.
                if *pa != MeasurePosition::Start
                    || !matches!(oa, AnchorOffset::Zero)
                    || !matches!(ob, AnchorOffset::Zero)
                {
                    return None;
                }
                self.measure20_vector_order(*ia, *ib)
            }
            // c4: same Region id and same edge.
            (
                TimeAnchor::Region {
                    id: ia,
                    edge: ea,
                    offset: oa,
                },
                TimeAnchor::Region {
                    id: ib,
                    edge: eb,
                    offset: ob,
                },
            ) if ia == ib && ea == eb => Self::measure20_offset_order(oa, ob),
            // c5: WallClock, no referent id.
            (TimeAnchor::WallClock { time: ta }, TimeAnchor::WallClock { time: tb }) => {
                Some(ta.cmp(tb))
            }
            // Never Event<->Measure, never Measure<->Region, never two
            // Events with different ids, never Musical against WallClock,
            // and never across differing pos/edge selectors.
            _ => None,
        }
    }

    /// Pin 6b: the musical delta `b - a`, computable ONLY in shape c1, c2,
    /// or c4 with BOTH offsets in the `Musical` clock (or `Zero`,
    /// normalized to `Musical(0)`), and only when the boundary selector is
    /// identical. c3 supplies no delta at all (a vector index gives order,
    /// never distance). `WallClock` deltas are never returned.
    fn measure20_musical_delta(&self, a: &TimeAnchor, b: &TimeAnchor) -> Option<MusicalDuration> {
        fn musical(o: &AnchorOffset) -> Option<MusicalDuration> {
            match o {
                AnchorOffset::Musical(d) => Some(d.clone()),
                AnchorOffset::Zero => Some(MusicalDuration::zero()),
                AnchorOffset::WallClock(_) => None,
            }
        }
        match (a, b) {
            (
                TimeAnchor::Event { id: ia, offset: oa },
                TimeAnchor::Event { id: ib, offset: ob },
            ) if ia == ib => Some(musical(ob)? - musical(oa)?),
            (
                TimeAnchor::Measure {
                    id: ia,
                    position: pa,
                    offset: oa,
                },
                TimeAnchor::Measure {
                    id: ib,
                    position: pb,
                    offset: ob,
                },
            ) if ia == ib && pa == pb => Some(musical(ob)? - musical(oa)?),
            (
                TimeAnchor::Region {
                    id: ia,
                    edge: ea,
                    offset: oa,
                },
                TimeAnchor::Region {
                    id: ib,
                    edge: eb,
                    offset: ob,
                },
            ) if ia == ib && ea == eb => Some(musical(ob)? - musical(oa)?),
            _ => None,
        }
    }

    /// Pin 6c steps 1-3: the unique maximum of `candidates` under
    /// [`Self::measure20_comparable_order`] — shared by the effective-grid
    /// oracle's governing meter change and (were it needed here) an
    /// append-only "current last element" search.
    fn measure20_unique_maximum<'x, T: Copy>(
        &self,
        candidates: impl IntoIterator<Item = (T, &'x TimeAnchor)>,
    ) -> Governing20<T> {
        let items: Vec<(T, &TimeAnchor)> = candidates.into_iter().collect();
        if items.is_empty() {
            return Governing20::None;
        }
        let mut maximal: Vec<T> = Vec::new();
        for (key, anchor) in &items {
            let dominated = items.iter().any(|(_, other)| {
                self.measure20_comparable_order(other, anchor) == Some(Ordering::Greater)
            });
            if !dominated {
                maximal.push(*key);
            }
        }
        if maximal.len() == 1 {
            Governing20::Unique(maximal[0])
        } else {
            Governing20::Indeterminate
        }
    }

    /// Pin 6c steps 0-3: the governing element among `candidates` relative
    /// to `reference`. **Step 0 comes before any candidate set**: if ANY
    /// candidate's anchor is incomparable to `reference`, the whole
    /// selection is indeterminate — even when the not-after-filtered set
    /// would have been empty (an incomparable change is unplaced, not
    /// absent, and it might have governed).
    fn measure20_governing_by_anchor<'x, T: Copy>(
        &self,
        reference: &TimeAnchor,
        candidates: impl IntoIterator<Item = (T, &'x TimeAnchor)>,
    ) -> Governing20<T> {
        let mut not_after: Vec<(T, &TimeAnchor)> = Vec::new();
        for (key, anchor) in candidates {
            match self.measure20_comparable_order(anchor, reference) {
                None => return Governing20::Indeterminate,
                Some(Ordering::Greater) => {}
                Some(_) => not_after.push((key, anchor)),
            }
        }
        self.measure20_unique_maximum(not_after)
    }

    /// Pin 6c: the effective grid's governing time signature at `start`,
    /// from an already-resolved `sequence` (this crate's effective grid is
    /// simply the instance's own local grid, or else the region default —
    /// see [`Self::check_measure_meter_consistency`]).
    fn measure20_governing_time_signature(
        &self,
        sequence: &[MeterChange],
        start: &TimeAnchor,
    ) -> Governing20<TimeSignatureId> {
        self.measure20_governing_by_anchor(
            start,
            sequence.iter().map(|c| (c.time_signature, &c.anchor)),
        )
    }

    /// 20. Agreement and boundary consistency (contract pins 6/6b/6c/9b).
    ///     ABSTAINS (emits no violation) wherever the comparison or delta is
    ///     not computable — base-ingested data may predate the rule (pin 7);
    ///     this is deliberate abstention, not a soundness gap. Does NOT
    ///     duplicate invariant 10's signature-resolution check: only
    ///     ALREADY-RESOLVING signatures are compared (pin 9b, "a resolving
    ///     `Some(id)`"), whether that is the measure's own declared
    ///     signature or a grid entry's.
    ///
    ///     **The nine non-success paths** below the body's two `match`
    ///     ladders, enumerated and classified by
    ///     `spec/CONTRACT_P13S18_MATRIX.md` pins 1–2 (a diagnostic listing:
    ///     descriptive of existing behaviour, not normative, and it moves no
    ///     line of the executable body below). Exactly three are genuine
    ///     abstentions; the rest are inapplicable, vacuous, delegated
    ///     elsewhere, or a separately-filed deferral:
    ///
    ///     | Id | Clause | Path | Class |
    ///     |---|---|---|---|
    ///     | A1 | agreement | `m.time_signature` is `None` | inapplicable — no declared signature can disagree with anything |
    ///     | A2 | agreement | declared signature does not resolve | delegated to invariant 10's per-measure arm |
    ///     | A3 | agreement | `Governing20::None` | vacuous — no governing signature exists to disagree with |
    ///     | A4 | agreement | `Governing20::Indeterminate` | genuine abstention — the relation cannot place a candidate |
    ///     | B1 | boundary | first measure (`i == 0`) | the pickup/anacrusis deferral, filed as `P13-S19` |
    ///     | B2 | boundary | governing signature does not resolve | delegated to invariant 10's grid-level arms |
    ///     | B3 | boundary | `Governing20::None` | vacuous, same as A3 |
    ///     | B4 | boundary | `Governing20::Indeterminate` | genuine abstention, same as A4 |
    ///     | B5 | boundary | musical delta not computable | genuine abstention — order without distance |
    ///
    ///     A2 and B2 are `delegated`, not merely unenforced, only because
    ///     invariant 10 actually reports the same unresolving reference on
    ///     the same graph (`spec/CONTRACT_P13S18_MATRIX.md` pin 3) — see
    ///     `g3b_measure20_tests::m39_unresolvable_reference_is_invariant_
    ///     10_only` (A2) and `matrix_b2_governing_signature_unresolving_
    ///     delegated` (B2) below, and M7/M8 in the contract's mutation
    ///     plan.
    fn check_measure_meter_consistency(&self, out: &mut Vec<WellFormednessViolation>) {
        let time_sigs: HashMap<TimeSignatureId, &TimeSignature> = self
            .score
            .time_signatures
            .iter()
            .map(|t| (t.id, t))
            .collect();
        for region in &self.score.canvas.regions {
            let default_grid = region
                .content
                .staff_based()
                .and_then(|c| c.default_metric_grid.as_ref());
            for instance in region.staff_instances() {
                let sequence: &[MeterChange] = instance
                    .local_metric_grid
                    .as_ref()
                    .map(|g| g.meter_sequence.as_slice())
                    .or_else(|| default_grid.map(|g| g.meter_sequence.as_slice()))
                    .unwrap_or(&[]);
                let measures = &instance.measures;
                for (i, m) in measures.iter().enumerate() {
                    // Agreement clause: ONLY a resolving `Some(id)` (pin
                    // 9b) — an unresolving reference is invariant 10's
                    // business, not this one's, and `None` avoids this
                    // clause entirely (but not the boundary clause below).
                    if let Some(sig) = m.time_signature {
                        if time_sigs.contains_key(&sig) {
                            match self.measure20_governing_time_signature(sequence, &m.start) {
                                Governing20::Unique(active) if active != sig => {
                                    out.push(WellFormednessViolation::invariant(
                                        GraphInvariant::MeasureMeterConsistency,
                                        format!(
                                            "measure {:?} declares time signature {:?} but the \
                                             effective grid's active signature at its start is \
                                             {:?}",
                                            m.id, sig, active
                                        ),
                                    ));
                                }
                                Governing20::Unique(_) | Governing20::None => {}
                                // Indeterminate selection: abstain (pin 7).
                                Governing20::Indeterminate => {}
                            }
                        }
                    }
                    // Boundary clause: vacuous for the first measure (no
                    // predecessor — pickup/anacrusis deferral, P13-S19).
                    if i == 0 {
                        continue;
                    }
                    let prev = &measures[i - 1];
                    match self.measure20_governing_time_signature(sequence, &prev.start) {
                        Governing20::Unique(sig) => {
                            let Some(ts) = time_sigs.get(&sig) else {
                                // The grid's own entry doesn't resolve:
                                // invariant 10's business, not this one's —
                                // abstain.
                                continue;
                            };
                            match self.measure20_musical_delta(&prev.start, &m.start) {
                                Some(delta) if &delta == ts.measure_duration() => {}
                                Some(_) => {
                                    out.push(WellFormednessViolation::invariant(
                                        GraphInvariant::MeasureMeterConsistency,
                                        format!(
                                            "measure {:?} start is not exactly one \
                                             measure_duration ({:?}, under governing \
                                             signature {:?}) after predecessor measure \
                                             {:?}'s start",
                                            m.id,
                                            ts.measure_duration(),
                                            sig,
                                            prev.id
                                        ),
                                    ));
                                }
                                // Delta not computable: abstain (pin 7).
                                None => {}
                            }
                        }
                        // No active signature governs the predecessor's
                        // start: vacuous, not a violation (pin 6c case 1).
                        Governing20::None => {}
                        // Indeterminate selection: abstain (pin 7).
                        Governing20::Indeterminate => {}
                    }
                }
            }
        }
    }

    // --- Invariant 21, `StaffGroupMembershipAgreement` (P13-S16 pins 6, 6b).
    // TWO separately dispatched methods, one per direction. The split is
    // required, not stylistic: each direction must have an independently
    // deletable call site so that deleting one leaves the other reporting
    // (M6a/M6b). A single shared comparison would satisfy every behavioural
    // assertion while making that observation impossible. Both emit the same
    // `GraphInvariant`, so both directions collapse to one variant in any
    // `kinds`-style set — which is why direction is asserted by name, never by
    // counting. -----------------------------------------------------------
    /// Direction **S→G**: every live staff naming a group appears in that
    /// group's `members`. A maintenance gap — pin 2 failing to append — shows up
    /// here.
    ///
    /// A staff naming a group that does not exist is **not** flagged here:
    /// dangling reference resolution belongs to the referential invariants, and
    /// abstaining keeps this invariant's witnesses about *agreement* only.
    fn check_staff_names_absent_group(&self, out: &mut Vec<WellFormednessViolation>) {
        let members: HashMap<StaffGroupId, BTreeSet<StaffId>> = self
            .score
            .staff_groups
            .iter()
            .map(|group| (group.id, group.members.iter().copied().collect()))
            .collect();
        for staff in &self.score.staves {
            let Some(group_id) = staff.group else {
                continue;
            };
            if let Some(ids) = members.get(&group_id) {
                if !ids.contains(&staff.id) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::StaffGroupMembershipAgreement,
                        format!(
                            "S->G: staff {:?} names group {:?}, but that group's members omit it",
                            staff.id, group_id
                        ),
                    ));
                }
            }
        }
    }

    /// Direction **G→S**: every staff a group lists names that group in its own
    /// `group` field. A stale projection — a member left behind, or one pointing
    /// at a different group — shows up here.
    ///
    /// A member id with no live staff is **not** flagged here, for the same
    /// reason as the S→G direction.
    fn check_group_lists_unowned_staff(&self, out: &mut Vec<WellFormednessViolation>) {
        let owner: HashMap<StaffId, Option<StaffGroupId>> = self
            .score
            .staves
            .iter()
            .map(|staff| (staff.id, staff.group))
            .collect();
        for group in &self.score.staff_groups {
            for member in &group.members {
                let Some(named) = owner.get(member) else {
                    continue;
                };
                if *named != Some(group.id) {
                    out.push(WellFormednessViolation::invariant(
                        GraphInvariant::StaffGroupMembershipAgreement,
                        format!(
                            "G->S: group {:?} lists staff {:?}, but that staff names {:?}",
                            group.id, member, named
                        ),
                    ));
                }
            }
        }
    }
}

/// Genesis tranche G3b (`spec/CONTRACT_GENESIS_G3B_MEASURE.md` pin 6c): the
/// outcome of finding the governing element of a partially-ordered set under
/// [`GraphIndex::measure20_comparable_order`] — an INDEPENDENT (core-only)
/// implementation of the SAME normative relation `epiphany-ops`'s `Reducer`
/// computes privately (see the architecture note on invariant 20's checker
/// methods, above).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Governing20<T> {
    /// No candidate at all (an empty, or wholly-filtered-away, set) —
    /// vacuous, not a violation.
    None,
    /// Exactly one candidate is the unique maximum.
    Unique(T),
    /// Either some candidate is incomparable to the reference point, or
    /// multiple maxima are mutually incomparable to each other — the check
    /// ABSTAINS rather than guessing.
    Indeterminate,
}

/// Whether an aleatoric interval bound is ordered (`min <= max`): `Some(true)`
/// when ordered, `Some(false)` when reversed, `None` for [`TimeBounds::Unbounded`]
/// (Chapter 3 §"Aleatoric Time").
fn time_bounds_ordered(tb: &crate::time::TimeBounds) -> Option<bool> {
    use crate::time::TimeBounds;
    match tb {
        TimeBounds::MusicalRange { min, max } => Some(min <= max),
        TimeBounds::WallClockRange { min, max } => Some(min <= max),
        TimeBounds::Unbounded => None,
    }
}

/// Applies an [`AnchorOffset`] to a resolved absolute wall-clock coordinate
/// (nanoseconds), or `None` if the offset is musical — a musical offset on a
/// wall-clock coordinate needs the deferred tempo map to convert.
fn apply_offset(base: i64, offset: &AnchorOffset) -> Option<i64> {
    match offset {
        AnchorOffset::Zero => Some(base),
        // Checked so a pathological offset is unresolvable, not a panic.
        AnchorOffset::WallClock(d) => base.checked_add(d.0),
        AnchorOffset::Musical(_) => None,
    }
}

/// Whether an event's coordinate kinds satisfy a region's discipline
/// (invariant 4).
fn coordinate_ok(e: &Event, disc: CoordinateDiscipline) -> bool {
    use crate::time::CoordinateKind::*;
    let pos = e.position().kind();
    let dur = e.duration();
    match disc {
        CoordinateDiscipline::Musical => {
            matches!(pos, Musical) && matches!(dur, EventDuration::Musical(_))
        }
        CoordinateDiscipline::WallClock => {
            matches!(pos, WallClock) && matches!(dur, EventDuration::WallClock(_))
        }
        CoordinateDiscipline::Aleatoric(a) => aleatoric_ok(e, a),
    }
}

fn aleatoric_ok(e: &Event, a: crate::graph::AleatoricAnchoringDiscipline) -> bool {
    use crate::graph::AleatoricAnchoringDiscipline as D;
    use crate::time::CoordinateKind::{Musical, WallClock};
    let pos = e.position().kind();
    let dur_kind = e.duration().concrete_kind(); // None for indeterminate
    let bounds_kind = match e.duration() {
        EventDuration::Indeterminate(b) => duration_bounds_kind(b),
        _ => BoundsKind::Concrete(dur_kind),
    };
    match a {
        D::Musical => pos == Musical && bounds_kind.allows_only(Musical),
        D::WallClock => pos == WallClock && bounds_kind.allows_only(WallClock),
        D::EitherPerEvent => {
            // Position and duration kinds must agree; bounds single-variant.
            match (dur_kind, bounds_kind) {
                (Some(k), _) => pos == k,
                (None, BoundsKind::Single(k)) => pos == k,
                (None, BoundsKind::Mixed) => false,
                (None, BoundsKind::Empty) => true,
                (None, BoundsKind::Concrete(_)) => true,
            }
        }
        D::FreelyMixed => true,
    }
}

enum BoundsKind {
    Empty,
    Single(crate::time::CoordinateKind),
    Mixed,
    Concrete(Option<crate::time::CoordinateKind>),
}

impl BoundsKind {
    fn allows_only(&self, k: crate::time::CoordinateKind) -> bool {
        match self {
            BoundsKind::Empty => true,
            BoundsKind::Single(x) | BoundsKind::Concrete(Some(x)) => *x == k,
            BoundsKind::Concrete(None) => true,
            BoundsKind::Mixed => false,
        }
    }
}

fn duration_bounds_kind(b: &crate::time::DurationBounds) -> BoundsKind {
    let k = |c: &ConcreteDuration| c.kind();
    match (&b.lower, &b.upper) {
        (None, None) => BoundsKind::Empty,
        (Some(l), None) => BoundsKind::Single(k(l)),
        (None, Some(u)) => BoundsKind::Single(k(u)),
        (Some(l), Some(u)) => {
            if k(l) == k(u) {
                BoundsKind::Single(k(l))
            } else {
                BoundsKind::Mixed
            }
        }
    }
}

/// Whether an anchor offset kind matches a region's coordinate discipline
/// (invariant 9).
fn offset_matches(offset: OffsetKind, disc: CoordinateDiscipline) -> bool {
    use crate::graph::AleatoricAnchoringDiscipline as A;
    match offset {
        OffsetKind::Zero => true,
        OffsetKind::Musical => matches!(
            disc,
            CoordinateDiscipline::Musical
                | CoordinateDiscipline::Aleatoric(A::Musical)
                | CoordinateDiscipline::Aleatoric(A::EitherPerEvent)
                | CoordinateDiscipline::Aleatoric(A::FreelyMixed)
        ),
        OffsetKind::WallClock => matches!(
            disc,
            CoordinateDiscipline::WallClock
                | CoordinateDiscipline::Aleatoric(A::WallClock)
                | CoordinateDiscipline::Aleatoric(A::EitherPerEvent)
                | CoordinateDiscipline::Aleatoric(A::FreelyMixed)
        ),
    }
}

/// Whether an anchor offset kind matches a *specific* coordinate clock — used
/// for `EitherPerEvent` regions, where the targeted event (not the region)
/// fixes the clock (invariant 9; finding on `EitherPerEvent`).
fn offset_matches_kind(offset: OffsetKind, kind: crate::time::CoordinateKind) -> bool {
    use crate::time::CoordinateKind::{Musical, WallClock};
    match offset {
        OffsetKind::Zero => true,
        OffsetKind::Musical => kind == Musical,
        OffsetKind::WallClock => kind == WallClock,
    }
}

/// A comparable (start, end) interval for an event, within whichever clock its
/// position uses. Mismatched position/duration clocks yield `Unknown` (that is
/// invariant 4's concern, not 3's). Indeterminate durations collapse to a
/// point so they never spuriously trigger an overlap.
enum Endpoints {
    Musical(MusicalPosition, MusicalPosition),
    Wall(i64, i64),
    Unknown,
}

#[derive(PartialEq)]
enum TimeKey {
    Musical(MusicalPosition),
    Wall(i64),
}

impl TimeKey {
    /// `self <= other` when both are the same clock; `true` (don't flag) when
    /// clocks differ (invariant 4 owns that mismatch).
    fn le_same_clock(&self, other: &TimeKey) -> bool {
        match (self, other) {
            (TimeKey::Musical(a), TimeKey::Musical(b)) => a <= b,
            (TimeKey::Wall(a), TimeKey::Wall(b)) => a <= b,
            _ => true,
        }
    }
}

impl Endpoints {
    fn of(e: &Event) -> Endpoints {
        match e.position() {
            EventPosition::Musical(p) => {
                let end = match e.duration() {
                    EventDuration::Musical(d) => p.clone() + d.clone(),
                    EventDuration::Indeterminate(_) => p.clone(),
                    EventDuration::WallClock(_) => return Endpoints::Unknown,
                };
                Endpoints::Musical(p.clone(), end)
            }
            EventPosition::WallClock(t) => {
                let end = match e.duration() {
                    // Overflow is unresolvable, not a saturated (wrong) endpoint
                    // that could mask an ordering violation — report Unknown.
                    EventDuration::WallClock(d) => match t.0.checked_add(d.0) {
                        Some(end) => end,
                        None => return Endpoints::Unknown,
                    },
                    EventDuration::Indeterminate(_) => t.0,
                    EventDuration::Musical(_) => return Endpoints::Unknown,
                };
                Endpoints::Wall(t.0, end)
            }
        }
    }

    fn start_key(&self) -> Option<TimeKey> {
        match self {
            Endpoints::Musical(s, _) => Some(TimeKey::Musical(s.clone())),
            Endpoints::Wall(s, _) => Some(TimeKey::Wall(*s)),
            Endpoints::Unknown => None,
        }
    }

    fn end_key(&self) -> Option<TimeKey> {
        match self {
            Endpoints::Musical(_, e) => Some(TimeKey::Musical(e.clone())),
            Endpoints::Wall(_, e) => Some(TimeKey::Wall(*e)),
            Endpoints::Unknown => None,
        }
    }
}

#[cfg(test)]
mod review_fix_tests {
    //! Targeted tests for the strengthened checks (overlap resolution, dangling
    //! spanner anchors, tie None-pairing / cross-voice, promoted-id derivation,
    //! tombstone collisions, comprehensive anchor offsets).
    use super::*;
    use crate::event::{Event, PitchedEvent, StemConfiguration};
    use crate::generators::valid_score;
    use crate::graph::{
        derive_promoted_voice_id, Beam, MetricTimeModel, ProportionalTimeModel, Region,
        RegionContent, RepeatKind, RepeatStructure, Spanner, StaffBasedContent, StaffExtent,
        StaffInstance, SubBeam, Tie, TieClass, TimeExtent, Voice, VoiceOrigin, Volta,
    };
    use crate::ids::{BeamId, OperationId, RepeatStructureId, ReplicaId, SpannerId, TieId};
    use crate::pitch::{
        AcousticPitch, AcousticRealization, CmnNominal, IdentifiedPitch, Pitch, PitchSpaceId,
        PitchSpacePosition, ScalePosition, TuningReference,
    };
    use crate::time::{
        AnchorOffset, EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime,
        RegionEdge, WallClockDuration, WallClockTime,
    };

    fn fires(score: &Score, inv: GraphInvariant) -> bool {
        !check_invariant(score, inv).is_empty()
    }

    fn cmn_ip(r: ReplicaId, c: u64, nominal: CmnNominal, oct: i8) -> IdentifiedPitch {
        IdentifiedPitch {
            id: crate::ids::PitchId::new(r, c),
            pitch: Pitch {
                scale_position: ScalePosition {
                    space: PitchSpaceId::new("cmn-12"),
                    position: PitchSpacePosition::Cmn {
                        nominal,
                        alteration: 0,
                        octave: oct,
                    },
                },
                acoustic: AcousticPitch {
                    tuning: TuningReference::Inherit,
                    realization: AcousticRealization::Implicit,
                },
            },
        }
    }

    fn wc(a: i64, b: i64) -> TimeExtent {
        TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(a),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(b),
            },
        }
    }

    #[test]
    fn inv7_detects_overlap_on_resolvable_wallclock_extents() {
        let mut s = valid_score(1);
        let staff = s.staves[0].id;
        // A second region manifesting the same staff, overlapping in wall-clock
        // time with the first region's [0, 1_000_000) extent.
        let rid = s.identity.mint();
        let inst = StaffInstance::new(s.identity.mint(), staff);
        let region = Region {
            id: rid,
            time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![inst],
                ..Default::default()
            }),
            time_extent: wc(500, 1500),
            staff_extent: StaffExtent {
                staves: vec![staff],
            },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        };
        s.canvas.regions.push(region);
        assert!(fires(&s, GraphInvariant::RegionExtents));
        // Disjoint-in-time (touching at the far end) must NOT fire.
        s.canvas.regions.last_mut().unwrap().time_extent = wc(1_000_000, 2_000_000);
        assert!(!fires(&s, GraphInvariant::RegionExtents));
    }

    #[test]
    fn inv7_unresolvable_overlap_is_deferred_not_silently_valid() {
        let mut s = valid_score(1);
        let staff = s.staves[0].id;
        let r0 = s.canvas.regions[0].id;
        let rid = s.identity.mint();
        let inst = StaffInstance::new(s.identity.mint(), staff);
        // A second region on the same staff whose extent is anchored
        // region-relative with a *musical* offset; with no tempo map it cannot
        // resolve to wall-clock, so its overlap with region 0's wall-clock extent
        // is undecidable.
        let symbolic = TimeExtent {
            start: TimeAnchor::Region {
                id: r0,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration::zero()),
            },
            end: TimeAnchor::Region {
                id: r0,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration::whole()),
            },
        };
        s.canvas.regions.push(Region {
            id: rid,
            time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![inst],
                ..Default::default()
            }),
            time_extent: symbolic,
            staff_extent: StaffExtent {
                staves: vec![staff],
            },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        });

        // Sound: the undecidable overlap is NOT raised as a (false-positive)
        // violation — `check_invariants` stays clean...
        assert!(
            check_invariants(&s).is_empty(),
            "unexpected violations: {:?}",
            check_invariants(&s)
        );
        // ...but it is surfaced as a deferred check naming both regions, rather
        // than silently treated as disjoint/valid.
        let deferred = deferred_checks(&s);
        assert_eq!(deferred.len(), 1, "{deferred:?}");
        assert_eq!(deferred[0].invariant, GraphInvariant::RegionExtents);
        assert!(deferred[0].reason.contains(&format!("{r0:?}")));
        assert!(deferred[0].reason.contains(&format!("{rid:?}")));

        // A wall-clock (resolvable), disjoint second region is *decided*, so it is
        // neither a violation nor deferred.
        s.canvas.regions.last_mut().unwrap().time_extent = wc(2_000_000, 3_000_000);
        assert!(deferred_checks(&s).is_empty());
        assert!(check_invariants(&s).is_empty());
    }

    #[test]
    fn inv9_flags_offset_on_event_anchored_meter_change() {
        // A proportional region whose own meter list is empty; place a metric
        // region and give a spanner a wall-clock offset against it (already
        // covered by the generator). Here exercise the *measure-start* path:
        // a metric region, with a spanner anchored to one of its events
        // carrying a musical offset is fine; a wall-clock offset is not.
        let mut s = valid_score(2);
        let rid = s.canvas.regions[0].id;
        let staff = s.staves[0].id;
        let spanner_ok = Spanner {
            id: SpannerId::new(s.identity.replica_id, 1),
            start: TimeAnchor::Region {
                id: rid,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration::whole()),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(1),
            },
            staves: vec![staff],
            kind: Default::default(),
            style: Default::default(),
        };
        s.cross_cutting.spanners.push(spanner_ok);
        assert!(!fires(&s, GraphInvariant::AnchorOffsetModel));
        // Now a wall-clock offset against the metric region: invalid.
        s.cross_cutting.spanners[0].start = TimeAnchor::Region {
            id: rid,
            edge: RegionEdge::Start,
            offset: AnchorOffset::WallClock(WallClockDuration(1)),
        };
        assert!(fires(&s, GraphInvariant::AnchorOffsetModel));
    }

    #[test]
    fn inv10_flags_dangling_spanner_anchor() {
        let mut s = valid_score(3);
        let staff = s.staves[0].id;
        let ghost_event = crate::ids::EventId::new(s.identity.replica_id, 9_000_001);
        s.cross_cutting.spanners.push(Spanner {
            id: SpannerId::new(s.identity.replica_id, 1),
            start: TimeAnchor::Event {
                id: ghost_event,
                offset: AnchorOffset::Zero,
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            staves: vec![staff],
            kind: Default::default(),
            style: Default::default(),
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn inv10_flags_dangling_sub_beam_event() {
        // Schema major 2: sub-beam member events are references the invariant
        // must resolve, exactly like the owning beam's events.
        let mut s = valid_score(3);
        let live: Vec<_> = s
            .voices()
            .flat_map(|(_, _, v)| v.events.clone())
            .take(2)
            .collect();
        let ghost_event = crate::ids::EventId::new(s.identity.replica_id, 9_000_002);
        s.cross_cutting.beams.push(Beam {
            id: BeamId::new(s.identity.replica_id, 1),
            events: live.clone(),
            level: 1,
            sub_beams: vec![SubBeam {
                level: 2,
                events: vec![ghost_event],
            }],
            geometry_override: None,
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn inv10_flags_dangling_repeat_kind_and_volta_anchors() {
        // Schema major 2: a DalSegno's segno/end_target and each volta's span
        // are anchors the invariant must resolve.
        let mut s = valid_score(3);
        let r = s.identity.replica_id;
        let ghost = crate::ids::EventId::new(r, 9_000_003);
        let dead_anchor = TimeAnchor::Event {
            id: ghost,
            offset: AnchorOffset::Zero,
        };
        let ok = TimeAnchor::WallClock {
            time: WallClockTime(0),
        };
        s.cross_cutting.repeats.push(RepeatStructure {
            id: RepeatStructureId::new(r, 1),
            start: ok.clone(),
            end: ok.clone(),
            kind: RepeatKind::DalSegno {
                segno: dead_anchor.clone(),
                end_target: ok.clone(),
            },
            voltas: vec![],
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        let mut s2 = valid_score(3);
        s2.cross_cutting.repeats.push(RepeatStructure {
            id: RepeatStructureId::new(r, 2),
            start: ok.clone(),
            end: ok.clone(),
            kind: RepeatKind::Volta,
            voltas: vec![Volta {
                endings: vec![1],
                start: dead_anchor,
                end: ok,
            }],
        });
        assert!(fires(&s2, GraphInvariant::CrossCuttingRefsResolve));
    }

    /// Builds a single-voice score with two adjacent pitched chords and returns
    /// (score, e0, e1) for tie tests.
    fn two_chord_score(
        seed: u64,
        start_pitch: (CmnNominal, i8),
        end_pitch: (CmnNominal, i8),
    ) -> (Score, crate::ids::EventId, crate::ids::EventId) {
        let mut s = valid_score(seed);
        let r = s.identity.replica_id;
        let voice_id = s.canvas.regions[0].staff_instances()[0].voices[0].id;
        // Replace the voice's events with exactly two adjacent pitched events.
        let old: Vec<_> = s.canvas.regions[0].staff_instances()[0].voices[0]
            .events
            .clone();
        for e in old {
            s.events.remove(e);
        }
        let e0 = s.identity.mint();
        let e1 = s.identity.mint();
        let mk = |id, pos: i64, p: IdentifiedPitch| {
            Event::Pitched(PitchedEvent {
                id,
                voice: voice_id,
                position: EventPosition::Musical(MusicalPosition(
                    RationalTime::new(pos, 4).unwrap(),
                )),
                duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
                pitches: vec![p],
                articulations: vec![],
                dynamic: None,
                ornaments: vec![],
                stem: StemConfiguration,
                grace: None,
            })
        };
        s.events
            .insert(mk(e0, 0, cmn_ip(r, 100, start_pitch.0, start_pitch.1)))
            .unwrap();
        s.events
            .insert(mk(e1, 1, cmn_ip(r, 101, end_pitch.0, end_pitch.1)))
            .unwrap();
        let insts = s.canvas.regions[0].content.staff_instances_mut().unwrap();
        insts[0].voices[0].events = vec![e0, e1];
        (s, e0, e1)
    }

    #[test]
    fn inv17_none_pairing_checks_enharmonic_implicit_pairs() {
        // C4 -> D4 with implicit (None) pairing is not enharmonic -> fires.
        let (mut s, e0, e1) = two_chord_score(10, (CmnNominal::C, 4), (CmnNominal::D, 4));
        s.cross_cutting.ties.push(Tie {
            id: TieId::new(s.identity.replica_id, 1),
            start_event: e0,
            end_event: e1,
            pitch_pairing: None,
            class: TieClass::Standard,
            style: Default::default(),
        });
        assert!(fires(&s, GraphInvariant::TiePairing));

        // C4 -> C4 with None pairing is a valid standard tie.
        let (mut s2, e0, e1) = two_chord_score(11, (CmnNominal::C, 4), (CmnNominal::C, 4));
        s2.cross_cutting.ties.push(Tie {
            id: TieId::new(s2.identity.replica_id, 1),
            start_event: e0,
            end_event: e1,
            pitch_pairing: None,
            class: TieClass::Standard,
            style: Default::default(),
        });
        assert!(!fires(&s2, GraphInvariant::TiePairing));
    }

    #[test]
    fn inv11_flags_tombstone_live_collision_and_duplicate_cc_id() {
        // Tombstone an event id that is still live -> identifier reuse.
        let mut s = valid_score(20);
        let live = s.events.ids_canonical()[0];
        s.tombstoned_events.insert(live);
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));

        // Two slurs with the same id.
        let mut s2 = valid_score(21);
        let staff_event = s2.events.ids_canonical()[0];
        let sid = crate::ids::SlurId::new(s2.identity.replica_id, 1);
        for _ in 0..2 {
            s2.cross_cutting.slurs.push(crate::graph::Slur {
                id: sid,
                start_event: staff_event,
                end_event: staff_event,
                kind: Default::default(),
                curvature_override: None,
                style: Default::default(),
            });
        }
        assert!(fires(&s2, GraphInvariant::UniqueIdentifiers));
    }

    #[test]
    fn inv11_flags_system_derived_misuse_on_region() {
        let mut s = valid_score(22);
        s.canvas.regions[0].id = crate::ids::RegionId::new(ReplicaId::SYSTEM_DERIVED, 1);
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));
    }

    #[test]
    fn inv18_flags_fabricated_promoted_voice_id_and_accepts_the_derivation() {
        let mut s = valid_score(30);
        let si = s.canvas.regions[0].staff_instances()[0].id;
        let original = s.canvas.regions[0].staff_instances()[0].voices[0].id;
        let winner = OperationId::new(s.identity.replica_id, 5);
        let loser = OperationId::new(s.identity.replica_id, 6);
        let correct = derive_promoted_voice_id(si, original, winner, loser);

        // A SystemPromoted voice with the *correct* derived id is accepted.
        let good = Voice {
            id: correct,
            events: vec![],
            default_stem_direction: None,
            is_primary: false,
            origin: VoiceOrigin::SystemPromoted {
                winning_operation: winner,
                losing_operation: loser,
                original_voice: original,
            },
        };
        s.canvas.regions[0].content.staff_instances_mut().unwrap()[0]
            .voices
            .push(good);
        assert!(!fires(&s, GraphInvariant::VoiceOriginConsistent));

        // A fabricated id (even in the SYSTEM_DERIVED namespace) is rejected:
        // it does not equal the deterministic derivation.
        let fabricated = crate::ids::VoiceId::new(ReplicaId::SYSTEM_DERIVED, 0xDEAD);
        s.canvas.regions[0].content.staff_instances_mut().unwrap()[0]
            .voices
            .push(Voice {
                id: fabricated,
                events: vec![],
                default_stem_direction: None,
                is_primary: false,
                origin: VoiceOrigin::SystemPromoted {
                    winning_operation: winner,
                    losing_operation: loser,
                    original_voice: original,
                },
            });
        assert!(fires(&s, GraphInvariant::VoiceOriginConsistent));
    }

    #[test]
    fn inv10_flags_dangling_marker_lyric_and_gesture_refs() {
        use crate::graph::{
            AnnotationAnchor, ChordSymbol, Comment, GestureAnchoring, GraphicGesture, LyricLine,
            Marker, RepeatStructure,
        };
        let r = valid_score(50).identity.replica_id;
        let ghost_e = crate::ids::EventId::new(r, 9_100_001);

        // Marker anchored to a non-existent region.
        let mut s = valid_score(50);
        s.cross_cutting.markers.push(Marker {
            id: crate::ids::MarkerId::new(r, 1),
            anchor: TimeAnchor::Region {
                id: crate::ids::RegionId::new(r, 9_100_002),
                edge: crate::time::RegionEdge::Start,
                offset: AnchorOffset::Zero,
            },
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Lyric line referencing a dangling event.
        let mut s = valid_score(51);
        s.cross_cutting.lyrics.push(LyricLine {
            id: crate::ids::LyricLineId::new(r, 1),
            events: vec![ghost_e],
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Graphic gesture anchored to a dangling event.
        let mut s = valid_score(52);
        s.cross_cutting.graphic_gestures.push(GraphicGesture {
            id: crate::ids::GraphicGestureId::new(r, 1),
            objects: vec![],
            anchoring: GestureAnchoring::Events(vec![ghost_e]),
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Repeat / comment / chord symbol with a dangling event anchor.
        let mut s = valid_score(53);
        s.cross_cutting.repeats.push(RepeatStructure {
            id: crate::ids::RepeatStructureId::new(r, 1),
            start: TimeAnchor::Event {
                id: ghost_e,
                offset: AnchorOffset::Zero,
            },
            end: TimeAnchor::WallClock {
                time: crate::time::WallClockTime(0),
            },
            kind: crate::graph::RepeatKind::migration_default(),
            voltas: Vec::new(),
        });
        s.cross_cutting.comments.push(Comment {
            id: crate::ids::CommentId::new(r, 1),
            anchor: AnnotationAnchor::Event(ghost_e),
            resolved: false,
        });
        s.cross_cutting.chord_symbols.push(ChordSymbol {
            id: crate::ids::ChordSymbolId::new(r, 1),
            anchor: TimeAnchor::Event {
                id: ghost_e,
                offset: AnchorOffset::Zero,
            },
        });
        assert!(check_invariant(&s, GraphInvariant::CrossCuttingRefsResolve).len() >= 3);
    }

    #[test]
    fn proportional_region_resolver_round_trips() {
        // Sanity: a proportional region's wall-clock extents resolve and a
        // non-overlapping pair on a shared staff does not fire inv7.
        let mut s = valid_score(40);
        let staff = s.staves[0].id;
        let inst = StaffInstance::new(s.identity.mint(), staff);
        s.canvas.regions.push(Region {
            id: s.identity.mint(),
            time_model: RegionTimeModel::Proportional(ProportionalTimeModel {
                duration: WallClockDuration(1000),
            }),
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![inst],
                ..Default::default()
            }),
            time_extent: wc(2_000_000, 3_000_000),
            staff_extent: StaffExtent {
                staves: vec![staff],
            },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        });
        assert!(check_invariants(&s).is_empty());
    }
}

#[cfg(test)]
mod review_fix_tests_2 {
    //! Tests for the second review pass: complete reference resolution
    //! (annotation layer, tuplet parent, gesture/graphic-event objects,
    //! event-internal refs), comprehensive id uniqueness + reserved-namespace
    //! and arena-integrity checks, and the hardened identity context.
    use super::*;
    use crate::event::{
        CueEvent, CueRendering, Event, GraphicEvent, IndeterminacyHints, IndeterminacyKind,
        IndeterminateEvent, TrajectoryDisplay, TrajectoryEndpoint, TrajectoryEvent,
        TrajectoryShape,
    };
    use crate::generators::valid_score;
    use crate::graph::{
        AnalysisLayer, AnalyticalAnnotation, AnnotationAnchor, GestureAnchoring, GraphicContent,
        GraphicGesture, GraphicObject, Instrument, Marker, RegionContent, Tuplet, TupletRatio,
    };
    use crate::ids::{
        AnalysisLayerId, AnalyticalAnnotationId, EventId, GraphicGestureId, GraphicObjectId,
        IdentityContext, InstrumentId, MarkerId, PitchId, ReplicaId, TupletId, VoiceId,
    };
    use crate::time::{
        EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime,
    };

    fn fires(s: &Score, inv: GraphInvariant) -> bool {
        !check_invariant(s, inv).is_empty()
    }

    #[test]
    fn inv10_resolves_annotation_layer_and_tuplet_parent() {
        let mut s = valid_score(60);
        let r = s.identity.replica_id;
        // Annotation on a non-existent analysis layer.
        s.cross_cutting.analytical.push(AnalyticalAnnotation {
            id: AnalyticalAnnotationId::new(r, 1),
            anchor: AnnotationAnchor::Region(s.canvas.regions[0].id),
            layer: Some(AnalysisLayerId::new(r, 7_000_001)),
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
        // Declaring the layer clears it.
        s.analysis_layers.push(AnalysisLayer {
            id: AnalysisLayerId::new(r, 7_000_001),
            name: "x".into(),
        });
        assert!(!fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Tuplet with a non-existent parent.
        let e = s.events.ids_canonical()[0];
        s.cross_cutting.tuplets.push(Tuplet {
            id: TupletId::new(r, 1),
            ratio: TupletRatio::new(3, 2).expect("3:2 is a valid tuplet ratio"),
            members: vec![e],
            parent: Some(TupletId::new(r, 7_000_002)),
            required_total: MusicalDuration(RationalTime::new(1, 4).unwrap()),
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn inv10_resolves_graphic_object_references() {
        let mut s = valid_score(61);
        let r = s.identity.replica_id;
        let stored = GraphicObjectId::new(r, 1);
        let missing = GraphicObjectId::new(r, 2);
        // Store one graphic object in a free-graphic region. Free-graphic
        // regions have no staff instances, so the staff extent is empty (and it
        // is disjoint in time from region 0, so no overlap).
        s.canvas.regions.push(crate::graph::Region {
            id: crate::ids::RegionId::new(r, 9_003),
            time_model: crate::graph::RegionTimeModel::Metric(Default::default()),
            content: RegionContent::FreeGraphic(GraphicContent {
                objects: vec![GraphicObject { id: stored }],
            }),
            time_extent: crate::graph::TimeExtent {
                start: TimeAnchor::WallClock {
                    time: crate::time::WallClockTime(5_000_000),
                },
                end: TimeAnchor::WallClock {
                    time: crate::time::WallClockTime(6_000_000),
                },
            },
            staff_extent: crate::graph::StaffExtent { staves: vec![] },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        });
        // A gesture referencing a stored object resolves; a missing one fires.
        s.cross_cutting.graphic_gestures.push(GraphicGesture {
            id: GraphicGestureId::new(r, 1),
            objects: vec![stored],
            anchoring: GestureAnchoring::Free,
        });
        assert!(!fires(&s, GraphInvariant::CrossCuttingRefsResolve));
        s.cross_cutting.graphic_gestures[0].objects = vec![missing];
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    /// Adds a pitched event to voice 0 and returns its id (helper for ref tests).
    fn add_event(s: &mut Score, ev: Event) -> EventId {
        let id = ev.id();
        let voice = s.canvas.regions[0].staff_instances()[0].voices[0].id;
        let mut ev = ev;
        ev.set_voice(voice);
        s.events.insert(ev).unwrap();
        s.canvas.regions[0].content.staff_instances_mut().unwrap()[0].voices[0]
            .events
            .push(id);
        id
    }

    #[test]
    fn inv10_resolves_event_internal_references() {
        // Cue with a dangling source.
        let mut s = valid_score(62);
        let r = s.identity.replica_id;
        let v = VoiceId::new(r, 0); // overwritten by add_event
        let ghost = EventId::new(r, 8_000_001);
        add_event(
            &mut s,
            Event::Cue(CueEvent {
                id: EventId::new(r, 8_000_010),
                voice: v,
                position: EventPosition::Musical(MusicalPosition(
                    RationalTime::new(100, 4).unwrap(),
                )),
                duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
                source: vec![ghost],
                rendering: CueRendering,
            }),
        );
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Indeterminate event with a dangling alternative.
        let mut s = valid_score(63);
        let r = s.identity.replica_id;
        add_event(
            &mut s,
            Event::Indeterminate(IndeterminateEvent {
                id: EventId::new(r, 8_000_020),
                voice: VoiceId::new(r, 0),
                position: EventPosition::Musical(MusicalPosition(
                    RationalTime::new(100, 4).unwrap(),
                )),
                duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
                indeterminacy: IndeterminacyKind::Choice,
                hints: IndeterminacyHints {
                    alternatives: vec![EventId::new(r, 8_000_099)],
                    ..Default::default()
                },
            }),
        );
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Trajectory referencing a dangling event-pitch.
        let mut s = valid_score(64);
        let r = s.identity.replica_id;
        add_event(
            &mut s,
            Event::Trajectory(TrajectoryEvent {
                id: EventId::new(r, 8_000_030),
                voice: VoiceId::new(r, 0),
                position: EventPosition::Musical(MusicalPosition(
                    RationalTime::new(100, 4).unwrap(),
                )),
                duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
                start: TrajectoryEndpoint::EventPitch(PitchId::new(r, 8_000_098)),
                end: TrajectoryEndpoint::EventPitch(PitchId::new(r, 8_000_097)),
                shape: TrajectoryShape::Linear,
                display: TrajectoryDisplay,
            }),
        );
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Graphic event referencing an unstored graphic object.
        let mut s = valid_score(65);
        let r = s.identity.replica_id;
        add_event(
            &mut s,
            Event::Graphic(GraphicEvent {
                id: EventId::new(r, 8_000_040),
                voice: VoiceId::new(r, 0),
                position: EventPosition::Musical(MusicalPosition(
                    RationalTime::new(100, 4).unwrap(),
                )),
                duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
                graphics: vec![GraphicObjectId::new(r, 8_000_096)],
                playback_bindings: vec![],
            }),
        );
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn inv11_covers_more_id_kinds_and_identity_namespace() {
        // Duplicate marker id.
        let mut s = valid_score(70);
        let r = s.identity.replica_id;
        let mid = MarkerId::new(r, 1);
        for _ in 0..2 {
            s.cross_cutting.markers.push(Marker {
                id: mid,
                anchor: TimeAnchor::Region {
                    id: s.canvas.regions[0].id,
                    edge: crate::time::RegionEdge::Start,
                    offset: crate::time::AnchorOffset::Zero,
                },
            });
        }
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));

        // Duplicate instrument id.
        let mut s = valid_score(71);
        let r = s.identity.replica_id;
        let iid = InstrumentId::new(r, 1);
        s.instruments.push(Instrument::new(iid, "a"));
        s.instruments.push(Instrument::new(iid, "b"));
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));

        // Score identity in the reserved namespace.
        let mut s = valid_score(72);
        s.identity = IdentityContext {
            replica_id: ReplicaId::SYSTEM_DERIVED,
            next_counter: 0,
        };
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));
    }

    #[test]
    fn inv11_catches_get_mut_corruption() {
        // Clearing a pitched event's pitches via get_mut is caught (malformed).
        let mut s = valid_score(80);
        let e = s.events.ids_canonical()[0];
        if let Some(Event::Pitched(p)) = s.events.get_mut(e) {
            p.pitches.clear();
        }
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));

        // Mutating an event's own id via get_mut desyncs the arena index.
        let mut s = valid_score(81);
        let e = s.events.ids_canonical()[0];
        let r = s.identity.replica_id;
        if let Some(Event::Pitched(p)) = s.events.get_mut(e) {
            p.id = EventId::new(r, 9_999_999);
        }
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));
    }
}

#[cfg(test)]
mod review_fix_tests_3 {
    //! Tests for the third review pass: enharmonic implicit tie matching,
    //! degenerate tuplet ratios, legitimate system-derived pitches vs reserved
    //! misuse, `EitherPerEvent` offset clocks, and dangling instrument refs.
    use super::*;
    use crate::event::{Event, PitchedEvent, StemConfiguration};
    use crate::generators::valid_score;
    use crate::graph::{
        AleatoricAnchoringDiscipline, AleatoricTimeModel, Instrument, Region, RegionContent,
        RegionTimeModel, Spanner, StaffBasedContent, StaffExtent, StaffInstance, Tie, TieClass,
        TimeExtent, TupletRatio, Voice,
    };
    use crate::ids::{
        EventId, InstrumentId, PitchId, RegionId, ReplicaId, SpannerId, StaffInstanceId, TieId,
        VoiceId,
    };
    use crate::pitch::{
        AcousticPitch, AcousticRealization, CmnNominal, IdentifiedPitch, Pitch, PitchSpaceId,
        PitchSpacePosition, ScalePosition, TuningReference,
    };
    use crate::time::{
        AnchorOffset, EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime,
        WallClockDuration, WallClockTime,
    };

    fn fires(s: &Score, inv: GraphInvariant) -> bool {
        !check_invariant(s, inv).is_empty()
    }

    fn pitch_at(r: ReplicaId, pid_counter: u64, nominal: CmnNominal, alt: i8) -> IdentifiedPitch {
        IdentifiedPitch {
            id: PitchId::new(r, pid_counter),
            pitch: Pitch {
                scale_position: ScalePosition {
                    space: PitchSpaceId::new("cmn-12"),
                    position: PitchSpacePosition::Cmn {
                        nominal,
                        alteration: alt,
                        octave: 4,
                    },
                },
                acoustic: AcousticPitch {
                    tuning: TuningReference::Inherit,
                    realization: AcousticRealization::Implicit,
                },
            },
        }
    }

    #[test]
    fn inv17_implicit_pairing_uses_enharmonic_matching_not_zip() {
        // Build a fresh single-voice score with two adjacent two-note chords.
        // Start chord (by id order): C4@10, E4@11. End chord: E4@20, C4@21.
        // A positional zip (C4↔E4, E4↔C4) would be non-enharmonic and rejected;
        // deterministic enharmonic matching pairs C4↔C4 and E4↔E4 -> accepted.
        let mut s = valid_score(90);
        let r = s.identity.replica_id;
        let voice = s.canvas.regions[0].staff_instances()[0].voices[0].id;
        for e in s.canvas.regions[0].staff_instances()[0].voices[0]
            .events
            .clone()
        {
            s.events.remove(e);
        }
        let e0 = EventId::new(r, 1000);
        let e1 = EventId::new(r, 1001);
        let chord = |id, pos: i64, ps: Vec<IdentifiedPitch>| {
            Event::Pitched(PitchedEvent {
                id,
                voice,
                position: EventPosition::Musical(MusicalPosition(
                    RationalTime::new(pos, 4).unwrap(),
                )),
                duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
                pitches: ps,
                articulations: vec![],
                dynamic: None,
                ornaments: vec![],
                stem: StemConfiguration,
                grace: None,
            })
        };
        s.events
            .insert(chord(
                e0,
                0,
                vec![
                    pitch_at(r, 10, CmnNominal::C, 0),
                    pitch_at(r, 11, CmnNominal::E, 0),
                ],
            ))
            .unwrap();
        s.events
            .insert(chord(
                e1,
                1,
                vec![
                    pitch_at(r, 20, CmnNominal::E, 0),
                    pitch_at(r, 21, CmnNominal::C, 0),
                ],
            ))
            .unwrap();
        s.canvas.regions[0].content.staff_instances_mut().unwrap()[0].voices[0].events =
            vec![e0, e1];
        s.cross_cutting.ties.push(Tie {
            id: TieId::new(r, 1),
            start_event: e0,
            end_event: e1,
            pitch_pairing: None,
            class: TieClass::Standard,
            style: Default::default(),
        });
        assert!(
            !fires(&s, GraphInvariant::TiePairing),
            "reordered chord should match"
        );

        // Replace the end chord's C4 with a G4 (no counterpart for the start C4).
        if let Some(Event::Pitched(p)) = s.events.get_mut(e1) {
            p.pitches[1] = pitch_at(r, 21, CmnNominal::G, 0);
        }
        assert!(
            fires(&s, GraphInvariant::TiePairing),
            "missing counterpart should fire"
        );
    }

    #[test]
    fn degenerate_tuplet_ratio_is_rejected_at_construction() {
        // Pass 11, item 3.5 / Tuplet honesty: a degenerate ratio is rejected by
        // `TupletRatio::new` at construction, so it can never enter the graph
        // and is no longer a runtime invariant. A zero term or `actual ==
        // notated` is refused; a well-formed ratio is accepted.
        assert!(TupletRatio::new(0, 2).is_none(), "n:0-form rejected");
        assert!(TupletRatio::new(2, 0).is_none(), "0:n-form rejected");
        assert!(TupletRatio::new(0, 0).is_none(), "0:0 rejected");
        assert!(
            TupletRatio::new(4, 4).is_none(),
            "actual == notated rejected"
        );
        let ok = TupletRatio::new(3, 2).expect("3:2 is valid");
        assert_eq!((ok.actual(), ok.notated()), (3, 2));
    }

    #[test]
    fn inv11_accepts_proven_system_pitch_but_flags_arbitrary_one() {
        // A MUSCSPCH synthetic pitch is legitimate only when its counter is the
        // deterministic content derivation of its own pitch.
        let mut s = valid_score(92);
        let e = s.events.ids_canonical()[0];
        if let Some(Event::Pitched(p)) = s.events.get_mut(e) {
            let derived = crate::derive_system_pitch_id(&p.pitches[0].pitch);
            p.pitches[0].id = derived;
        }
        assert!(
            !fires(&s, GraphInvariant::UniqueIdentifiers),
            "a proven MUSCSPCH derivation is legitimate"
        );

        // An *arbitrary* counter in the reserved namespace is misuse: it does
        // not prove the MUSCSPCH derivation.
        let mut s = valid_score(92);
        let e = s.events.ids_canonical()[0];
        if let Some(Event::Pitched(p)) = s.events.get_mut(e) {
            p.pitches[0].id = PitchId::new(ReplicaId::SYSTEM_DERIVED, 123);
        }
        assert!(
            fires(&s, GraphInvariant::UniqueIdentifiers),
            "an arbitrary system-derived pitch counter is misuse"
        );

        // An instrument in the reserved namespace is misuse.
        let mut s = valid_score(93);
        s.instruments.push(Instrument {
            id: InstrumentId::new(ReplicaId::SYSTEM_DERIVED, 1),
            name: "x".into(),
            range: None,
            abbreviation: None,
            sound_config: Default::default(),
            transposition: None,
            default_clef: crate::graph::Clef::treble(),
            default_staff_lines: Default::default(),
            unpitched_members: Vec::new(),
        });
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));
    }

    #[test]
    fn inv10_flags_dangling_staff_instrument() {
        let mut s = valid_score(94);
        let r = s.identity.replica_id;
        // Repoint the staff at an undeclared instrument.
        s.staves[0].instrument = InstrumentId::new(r, 7_777_001);
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn inv9_either_per_event_offset_matches_event_clock() {
        // Aleatoric EitherPerEvent region with a wall-clock event; a spanner
        // anchored to that event with a musical offset is wrong, with a
        // wall-clock offset is fine.
        let mut s = valid_score(95);
        let r = s.identity.replica_id;
        let staff = s.staves[0].id;
        let evid = EventId::new(r, 5000);
        let region_id = RegionId::new(r, 5001);
        let inst_id = StaffInstanceId::new(r, 5002);
        let voice_id = VoiceId::new(r, 5003);
        let mut voice = Voice::user(voice_id);
        voice.events.push(evid);
        let mut inst = StaffInstance::new(inst_id, staff);
        // Distinct staff to avoid a same-staff overlap with region 0.
        let staff2 = crate::ids::StaffId::new(r, 5009);
        s.staves.push(crate::graph::Staff {
            id: staff2,
            name: "w".into(),
            abbreviation: None,
            instrument: s.staves[0].instrument,
            default_staff_lines: Default::default(),
            group: None,
            default_clef: crate::graph::Clef::treble(),
        });
        inst.staff = staff2;
        inst.voices.push(voice);
        s.events
            .insert(Event::Pitched(PitchedEvent {
                id: evid,
                voice: voice_id,
                position: EventPosition::WallClock(WallClockTime(10)),
                duration: EventDuration::WallClock(WallClockDuration(5)),
                pitches: vec![pitch_at(r, 5100, CmnNominal::C, 0)],
                articulations: vec![],
                dynamic: None,
                ornaments: vec![],
                stem: StemConfiguration,
                grace: None,
            }))
            .unwrap();
        s.canvas.regions.push(Region {
            id: region_id,
            time_model: RegionTimeModel::Aleatoric(AleatoricTimeModel {
                anchoring: AleatoricAnchoringDiscipline::EitherPerEvent,
                ordering: Default::default(),
                bounds: Default::default(),
                duration_hint: WallClockDuration(1000),
            }),
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![inst],
                ..Default::default()
            }),
            time_extent: TimeExtent {
                start: TimeAnchor::WallClock {
                    time: WallClockTime(2_000_000),
                },
                end: TimeAnchor::WallClock {
                    time: WallClockTime(3_000_000),
                },
            },
            staff_extent: StaffExtent {
                staves: vec![staff2],
            },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        });
        // Musical offset against a wall-clock event -> invariant 9 fires.
        s.cross_cutting.spanners.push(Spanner {
            id: SpannerId::new(r, 1),
            start: TimeAnchor::Event {
                id: evid,
                offset: AnchorOffset::Musical(MusicalDuration::whole()),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            staves: vec![staff2],
            kind: Default::default(),
            style: Default::default(),
        });
        assert!(fires(&s, GraphInvariant::AnchorOffsetModel));
        // A wall-clock offset matches the event's clock -> ok.
        s.cross_cutting.spanners[0].start = TimeAnchor::Event {
            id: evid,
            offset: AnchorOffset::WallClock(WallClockDuration(1)),
        };
        assert!(!fires(&s, GraphInvariant::AnchorOffsetModel));
    }

    #[test]
    fn inv10_flags_unresolved_time_signature_reference() {
        use crate::generators::valid_score_rich;
        use crate::graph::{TimeSignature, TimeSignatureDisplay};
        use crate::ids::TimeSignatureId;
        let mut s = valid_score_rich(96);
        let r = s.identity.replica_id;
        let ts_id = TimeSignatureId::new(r, 4_4);
        // Point region A's measure at an undeclared time signature.
        s.canvas.regions[0].content.staff_instances_mut().unwrap()[0].measures[0].time_signature =
            Some(ts_id);
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
        // Declaring a valid 4/4 time signature resolves it.
        s.time_signatures.push(
            TimeSignature::new(
                ts_id,
                TimeSignatureDisplay::Standard {
                    numerator: 4,
                    denominator: crate::graph::PowerOfTwo::new(4).unwrap(),
                },
                MusicalDuration::whole(),
                vec![crate::graph::BeatGroup {
                    duration: MusicalDuration::whole(),
                    subdivision: None,
                    accent: 0,
                }],
            )
            .unwrap(),
        );
        assert!(!fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }
}

#[cfg(test)]
mod review_fix_tests_4 {
    //! Tests for this review pass: aleatoric ordering/bounds validation (F3),
    //! tempo-map segment conditions (F4) — **requirements, not invariants, since
    //! P13-S29** — metric overlap via tempo conversion
    //! (F5), inv-11 time-signature/barline/graphic namespace + uniqueness (F6),
    //! inv-10 staff-group/part/view/meter reference resolution (F8), dangling
    //! decomposition tuplet refs (F9), and tuplet ratio consistency (F10).
    use super::*;
    use crate::generators::{valid_score, valid_score_rich};
    use crate::graph::{
        EventOrderingDAG, MetricTimeModel, Region, RegionContent, RegionTimeModel,
        StaffBasedContent, StaffExtent, StaffInstance, TupletRatio,
    };
    use crate::ids::{EventId, ReplicaId};
    use crate::tempo::{Tempo, TempoMap, TempoSegment, TempoShape};
    use crate::time::{
        AnchorOffset, EventBounds, MusicalDuration, MusicalPosition, RationalTime, RegionEdge,
        TimeAnchor, TimeBounds, WallClockTime,
    };

    fn fires(s: &Score, inv: GraphInvariant) -> bool {
        !check_invariant(s, inv).is_empty()
    }

    /// P13-S29: the Chapter 3/4 riders no longer answer to `check_invariant`.
    fn fires_req(s: &Score, label: &str) -> bool {
        !check_requirement(s, label).is_empty()
    }

    fn aleatoric_region_events(s: &Score) -> (usize, Vec<EventId>) {
        // valid_score_rich's region C (index 2) is aleatoric (musical).
        let idx = s
            .canvas
            .regions
            .iter()
            .position(|r| matches!(r.time_model, RegionTimeModel::Aleatoric(_)))
            .unwrap();
        let evs = s.canvas.regions[idx].staff_instances()[0].voices[0]
            .events
            .clone();
        (idx, evs)
    }

    #[test]
    fn f3_aleatoric_dag_referencing_absent_event_fires() {
        let mut s = valid_score_rich(200);
        let (idx, evs) = aleatoric_region_events(&s);
        let ghost = EventId::new(s.identity.replica_id, 9_300_001);
        let mut edges = std::collections::BTreeMap::new();
        edges.insert(evs[0], vec![ghost]); // real -> ghost (acyclic)
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.ordering = EventOrderingDAG::try_new(edges).unwrap();
        }
        assert!(fires_req(&s, "req:time:aleatoric-reference-locality"));
    }

    #[test]
    fn f3_aleatoric_bounds_key_absent_and_reversed_window_fire() {
        // A bounds key naming a non-region event.
        let mut s = valid_score_rich(201);
        let (idx, _evs) = aleatoric_region_events(&s);
        let ghost = EventId::new(s.identity.replica_id, 9_300_002);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.bounds.insert(ghost, EventBounds::default());
        }
        assert!(fires_req(&s, "req:time:aleatoric-reference-locality"));

        // A reversed (min > max) window on a real region event.
        let mut s = valid_score_rich(202);
        let (idx, evs) = aleatoric_region_events(&s);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.bounds.insert(
                evs[0],
                EventBounds {
                    start: Some(TimeBounds::MusicalRange {
                        min: MusicalPosition(RationalTime::new(1, 2).unwrap()),
                        max: MusicalPosition::origin(),
                    }),
                    end: None,
                },
            );
        }
        assert!(fires(&s, GraphInvariant::EventCoordinateModel));
    }

    fn region_seg_anchor(rid: crate::ids::RegionId, whole_notes: RationalTime) -> TimeAnchor {
        TimeAnchor::Region {
            id: rid,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration(whole_notes)),
        }
    }

    #[test]
    fn f4_tempo_segment_structural_defects_fire() {
        let base = valid_score(210);
        let rid = base.canvas.regions[0].id;

        // Non-constant segment missing its end_tempo.
        let mut s = base.clone();
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_seg_anchor(rid, RationalTime::zero()),
                end: Some(region_seg_anchor(rid, RationalTime::from_int(1))),
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: None,
                shape: TempoShape::Linear,
            }],
        };
        assert!(fires_req(&s, "req:time:tempo-segment-shape"));

        // Constant segment whose end_tempo disagrees with start_tempo.
        let mut s = base.clone();
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_seg_anchor(rid, RationalTime::zero()),
                end: None,
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: Some(Tempo::quarter(120.0).unwrap()),
                shape: TempoShape::Constant,
            }],
        };
        assert!(fires_req(&s, "req:time:tempo-segment-shape"));

        // Out-of-order segments (start 2 then start 1).
        let mut s = base.clone();
        let seg = |from, to| TempoSegment {
            start: region_seg_anchor(rid, RationalTime::from_int(from)),
            end: Some(region_seg_anchor(rid, RationalTime::from_int(to))),
            start_tempo: Tempo::quarter(60.0).unwrap(),
            end_tempo: None,
            shape: TempoShape::Constant,
        };
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![seg(2, 3), seg(1, 2)],
        };
        assert!(fires_req(&s, "req:time:tempo-segment-order"));

        // Segment anchored to a non-existent region (dangling anchor target).
        let mut s = base.clone();
        let ghost_region = crate::ids::RegionId::new(s.identity.replica_id, 9_400_001);
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_seg_anchor(ghost_region, RationalTime::zero()),
                end: None,
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: None,
                shape: TempoShape::Constant,
            }],
        };
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn f4_tempo_segment_offset_kind_is_checked_by_invariant_9() {
        // valid_score's region 0 is metric: a tempo segment anchored to it with
        // a wall-clock offset contradicts the time model (invariant 9).
        let mut s = valid_score(211);
        let rid = s.canvas.regions[0].id;
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: TimeAnchor::Region {
                    id: rid,
                    edge: RegionEdge::Start,
                    offset: AnchorOffset::WallClock(crate::time::WallClockDuration(1)),
                },
                end: None,
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: None,
                shape: TempoShape::Constant,
            }],
        };
        assert!(fires(&s, GraphInvariant::AnchorOffsetModel));
    }

    #[test]
    fn f5_overlapping_metric_regions_caught_via_tempo_conversion() {
        // Region 0 (metric, wall-clock extent, musical events) grounds the
        // events. Two regions on a fresh staff Y anchor their extents to those
        // musical events; with a tempo map they resolve and overlap is caught.
        let mut s = valid_score(212);
        let r = s.identity.replica_id;
        let ground_events = s.canvas.regions[0].staff_instances()[0].voices[0]
            .events
            .clone();
        let (e0, e1) = (ground_events[0], ground_events[1]);

        // A fresh staff Y (declared) with its instrument.
        let staff_y = s.identity.mint();
        let instr = s.identity.mint();
        s.instruments
            .push(crate::graph::Instrument::new(instr, "y"));
        s.staves.push(crate::graph::Staff {
            id: staff_y,
            name: "Y".into(),
            abbreviation: None,
            instrument: instr,
            default_staff_lines: Default::default(),
            group: None,
            default_clef: crate::graph::Clef::treble(),
        });
        let mk_region = |s: &mut Score, start: TimeAnchor, end: TimeAnchor| Region {
            id: s.identity.mint(),
            time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![StaffInstance::new(s.identity.mint(), staff_y)],
                ..Default::default()
            }),
            time_extent: crate::graph::TimeExtent { start, end },
            staff_extent: StaffExtent {
                staves: vec![staff_y],
            },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        };
        // R1 spans event e0 (musical 0) .. e1 (musical 1/4): wall-clock [0, 5e8].
        let r1 = mk_region(
            &mut s,
            TimeAnchor::Event {
                id: e0,
                offset: AnchorOffset::Zero,
            },
            TimeAnchor::Event {
                id: e1,
                offset: AnchorOffset::Zero,
            },
        );
        // R2 wall-clock [2e8, 8e8] overlaps R1 on staff Y.
        let r2 = mk_region(
            &mut s,
            TimeAnchor::WallClock {
                time: WallClockTime(200_000_000),
            },
            TimeAnchor::WallClock {
                time: WallClockTime(800_000_000),
            },
        );
        s.canvas.regions.push(r1);
        s.canvas.regions.push(r2);
        let _ = r;

        // Without a tempo, the musical-event extent cannot be placed: sound but
        // incomplete — overlap is NOT (falsely) reported.
        s.tempo_map = TempoMap::default();
        assert!(!fires(&s, GraphInvariant::RegionExtents));

        // With a constant tempo, e0/e1 resolve and the overlap is caught.
        s.tempo_map = TempoMap::constant(Tempo::quarter(120.0).unwrap());
        assert!(fires(&s, GraphInvariant::RegionExtents));
    }

    #[test]
    fn f6_time_signature_uniqueness_and_namespaces() {
        use crate::graph::{BeatGroup, TimeSignature, TimeSignatureDisplay};
        use crate::ids::TimeSignatureId;
        let mk_ts = |id| {
            TimeSignature::new(
                id,
                TimeSignatureDisplay::Standard {
                    numerator: 4,
                    denominator: crate::graph::PowerOfTwo::new(4).unwrap(),
                },
                MusicalDuration::whole(),
                vec![BeatGroup {
                    duration: MusicalDuration::whole(),
                    subdivision: None,
                    accent: 0,
                }],
            )
            .unwrap()
        };

        // Duplicate TimeSignatureId.
        let mut s = valid_score(220);
        let id = TimeSignatureId::new(s.identity.replica_id, 1);
        s.time_signatures.push(mk_ts(id));
        s.time_signatures.push(mk_ts(id));
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));

        // A TimeSignatureId in the reserved namespace.
        let mut s = valid_score(221);
        s.time_signatures
            .push(mk_ts(TimeSignatureId::new(ReplicaId::SYSTEM_DERIVED, 1)));
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));

        // A barline group / graphic object in the reserved namespace.
        let mut s = valid_score(222);
        if let RegionContent::StaffBased(c) = &mut s.canvas.regions[0].content {
            c.barline_alignment_groups
                .push(crate::graph::BarlineAlignmentGroup {
                    id: crate::ids::BarlineAlignmentGroupId::new(ReplicaId::SYSTEM_DERIVED, 1),
                    members: vec![],
                });
        }
        assert!(fires(&s, GraphInvariant::UniqueIdentifiers));
    }

    #[test]
    fn f8_structural_reference_resolution() {
        // Staff.group dangling.
        let mut s = valid_score(230);
        s.staves[0].group = Some(crate::ids::StaffGroupId::new(s.identity.replica_id, 9_001));
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // StaffGroup.members dangling.
        let mut s = valid_score(231);
        let r = s.identity.replica_id;
        s.staff_groups.push(crate::graph::StaffGroup {
            id: crate::ids::StaffGroupId::new(r, 1),
            name: None,
            kind: crate::graph::StaffGroupKind::Bracket,
            members: vec![crate::ids::StaffId::new(r, 9_002)],
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // PartDefinition.staves dangling.
        let mut s = valid_score(232);
        let r = s.identity.replica_id;
        s.parts.push(crate::graph::PartDefinition {
            id: crate::ids::PartDefinitionId::new(r, 1),
            name: "p".into(),
            staves: vec![crate::ids::StaffId::new(r, 9_003)],
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // ViewDefinition.active_layers dangling.
        let mut s = valid_score(233);
        let r = s.identity.replica_id;
        s.views.push(crate::graph::ViewDefinition {
            id: crate::ids::ViewId::new(r, 1),
            name: "v".into(),
            active_layers: vec![crate::ids::AnalysisLayerId::new(r, 9_004)],
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

        // Region-default-grid meter change referencing an undeclared time sig.
        let mut s = valid_score(234);
        let r = s.identity.replica_id;
        let ts = crate::ids::TimeSignatureId::new(r, 9_005);
        if let RegionContent::StaffBased(c) = &mut s.canvas.regions[0].content {
            c.default_metric_grid = Some(crate::graph::MetricGrid {
                meter_sequence: vec![crate::graph::MeterChange {
                    anchor: TimeAnchor::WallClock {
                        time: WallClockTime(0),
                    },
                    time_signature: ts,
                }],
            });
        }
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn f9_dangling_decomposition_tuplet_reference_fires() {
        // The rich score's decomposition references a real tuplet; repoint it at
        // a non-existent tuplet id.
        let mut s = valid_score_rich(240);
        let ghost = crate::ids::TupletId::new(s.identity.replica_id, 9_500_001);
        s.decomposition_attachments[0].components[0].tuplet = Some(ghost);
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
    }

    #[test]
    fn f10_tuplet_ratio_inconsistent_with_member_notation_fires() {
        // The rich score's eighth-in-3:2 triplet member is consistent.
        let s = valid_score_rich(241);
        assert!(!fires(&s, GraphInvariant::TupletSum));

        // Changing the ratio to 5:4 (member notation unchanged) now has a
        // validation effect: the notated eighth no longer scales to 1/12.
        let mut s = valid_score_rich(241);
        s.cross_cutting.tuplets[0].ratio =
            TupletRatio::new(5, 4).expect("5:4 is a valid tuplet ratio");
        assert!(fires(&s, GraphInvariant::TupletSum));
    }
}

#[cfg(test)]
mod accidental_compatibility_tests {
    //! `check_accidental_modification_compatibility` (Push 4b tranche 3a,
    //! `spec/CONTRACT_PUSH4B_ACCIDENTALS.md`, proof-of-life item 3): "a
    //! `CmnChromatic` accidental in a `cmn-12` (`DiatonicOverChromatic`)
    //! space passes; the same accidental in an `edo-31` (`Chromatic`) space
    //! is rejected with the violation."
    use super::*;
    use crate::accidental::{fixture_extensions, PitchSpaceModification};
    use crate::generators::valid_score;
    use crate::pitch::PitchSpaceId;

    /// P13-S29 pin 11b. The label moved from the witness text to the selector,
    /// so these observations select by label instead of grepping the witness —
    /// a witness search would pass on *any* violation once pin 8a removed the
    /// suffix, leaving both negatives green and vacuous.
    fn compatibility_violations(s: &Score) -> Vec<WellFormednessViolation> {
        check_requirement(s, "req:tuning:accidental-modification-compatibility")
    }

    #[test]
    fn cmn_chromatic_accidental_in_cmn_12_does_not_fire() {
        let mut s = valid_score(300);
        s.tuning_context.default_pitch_space = PitchSpaceId::new("cmn-12");
        s.tuning_context
            .accidental_extensions
            .push(fixture_extensions(
                "cmn-accidentals",
                PitchSpaceModification::CmnChromatic(1),
            ));
        assert!(
            compatibility_violations(&s).is_empty(),
            "a CmnChromatic accidental in cmn-12 must not violate the compatibility \
             requirement, got: {:?}",
            compatibility_violations(&s)
        );
    }

    #[test]
    fn cmn_chromatic_accidental_in_edo_31_fires() {
        // A test that only checked the accept case above would miss this
        // reject — the contract's own warning.
        let mut s = valid_score(300);
        s.tuning_context.default_pitch_space = PitchSpaceId::new("edo-31");
        s.tuning_context
            .accidental_extensions
            .push(fixture_extensions(
                "cmn-accidentals",
                PitchSpaceModification::CmnChromatic(1),
            ));
        let violations = compatibility_violations(&s);
        assert!(
            !violations.is_empty(),
            "expected an accidental-modification-compatibility violation"
        );
        // Pin 8a: the label is the Display prefix now, so the witness must not
        // carry it. Label-free and exact in the two ways that matter.
        assert!(
            violations[0].witness.ends_with("interval algebra"),
            "witness must end with the algebra clause, got: {:?}",
            violations[0].witness
        );
        assert!(
            !violations[0].witness.contains("req:"),
            "witness must not restate its own label, got: {:?}",
            violations[0].witness
        );
    }

    #[test]
    fn a_score_with_no_accidental_extensions_never_fires_this_check() {
        // Every generator-produced score has an empty `accidental_extensions`
        // (this tranche adds no data to any generator), so the new check must
        // be silent across the existing test/property-test corpus.
        let s = valid_score(301);
        assert!(s.tuning_context.accidental_extensions.is_empty());
        assert!(compatibility_violations(&s).is_empty());
    }
}

/// Genesis tranche G3a (`spec/CONTRACT_GENESIS_G3A_ENTITIES.md` pin 6): the
/// invariant-10 doc-comment reconciliation guard.
#[cfg(test)]
mod g3a_tests {
    const SOURCE: &str = include_str!("invariants.rs");

    /// The production portion of this file only, ending right before the
    /// first `#[cfg(test)]` module. Every needle this module searches for is
    /// itself written, as a string literal, inside a test module — so
    /// searching the whole file risks the exact self-matching trap
    /// `spec/CONTRACT_GENESIS_G3A_ENTITIES.md` §4 warns about (G2b hit it
    /// twice: once via a needle matching the guard's own source, once via an
    /// assertion message). Restricting the haystack structurally removes
    /// that risk rather than relying on needle length alone.
    fn production_source() -> &'static str {
        SOURCE
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("this file contains at least one #[cfg(test)] module")
    }

    /// (t12) Invariant 10's doc comment names the four reference classes its
    /// body checks (pin 6). Grep-assert the repaired prose is present
    /// **within the invariant-10 doc block only** — slice the source from
    /// the `/// 10.` line to the `CrossCuttingRefsResolve,` line and search
    /// *that*, since searching the whole file would pass on the
    /// implementation body, which contains the same identifiers the doc
    /// comment is supposed to gain.
    ///
    /// **Mutation:** revert the doc comment to its pre-G3a text (naming only
    /// cross-cutting structures and event-internal references); must fail.
    #[test]
    fn violation_types_declare_their_pinned_derives() {
        // P13-S29 pin 1b. Placed HERE, not beside pin 10's tests, because
        // `production_source()` is private to this module.
        //
        // Six equalities, not needles: a guard that merely searches for `Eq`,
        // `Hash` or one phrase passes every deletion mutation while failing
        // every promise the pin makes.
        let src = production_source();

        let before = |decl: &str| -> Vec<String> {
            let at = src
                .find(decl)
                .unwrap_or_else(|| panic!("{decl} is declared"));
            src[..at]
                .lines()
                .rev()
                .take_while(|l| {
                    let t = l.trim();
                    t.starts_with("///") || t.starts_with("#[derive")
                })
                .map(|l| l.trim().to_owned())
                .collect()
        };

        let struct_lines = before("pub struct WellFormednessViolation {");
        let enum_lines = before("pub enum ViolationKind {");

        // 1 and 2: the derive line immediately preceding each declaration.
        assert_eq!(
            struct_lines.first().map(String::as_str),
            Some("#[derive(Clone, PartialEq, Eq, Debug)]"),
            "WellFormednessViolation's derives are pinned"
        );
        assert_eq!(
            enum_lines.first().map(String::as_str),
            Some("#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]"),
            "ViolationKind's derives are pinned"
        );

        // 4 and 5: the /// block immediately preceding THAT derive.
        let doc = |lines: &[String]| -> String {
            lines
                .iter()
                .skip(1)
                .rev()
                .map(|l| l.trim_start_matches("///").trim())
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(
            doc(&struct_lines),
            "A well-formedness failure: one [`ViolationKind`] — `Invariant` or \
             `Requirement` — and the witness identifying the smallest offending objects. \
             A requirement failure is not an invariant failure.",
            "WellFormednessViolation's rustdoc is pinned"
        );
        assert_eq!(
            doc(&enum_lines),
            "What a [`WellFormednessViolation`] failed. `Invariant` names a numbered \
             Chapter 5 graph invariant; `Requirement` names a normative requirement by its \
             label. A requirement failure is not an invariant failure, and there is no \
             third arm and no unclassified fallback.",
            "ViolationKind's rustdoc is pinned"
        );

        // 3: the module header's two-arm paragraph, first line to next blank //!.
        let head = src
            .find("//! Violations carry a two-armed")
            .expect("the module header states the two-arm split");
        let para_end = src[head..].find("\n//!\n").expect("the paragraph ends") + head;
        let para = src[head..para_end]
            .lines()
            .map(|l| l.trim_start_matches("//!").trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            para,
            "Violations carry a two-armed [`ViolationKind`]: `Invariant` for the Chapter 5 \
             graph invariants, `Requirement` for a normative requirement named by its \
             label. A requirement failure is not an invariant failure, and neither arm is \
             a fallback for the other.",
            "the module header's two-arm paragraph is pinned"
        );

        // 6: the whole of the integration test, so its no-call constraint is
        // observed rather than assumed.
        assert_eq!(
            include_str!("../../epiphany-core/tests/public_surface.rs"),
            include_str!("../tests/public_surface.rs"),
            "public_surface.rs is read from one path"
        );
        let surface = include_str!("../tests/public_surface.rs");
        assert!(
            surface.contains(
                "use epiphany_core::{check_requirement, ViolationKind, WellFormednessViolation};"
            ),
            "the integration test must import all three names from the root"
        );
        assert!(
            !surface.contains("check_requirement(&"),
            "public_surface.rs must stay type-level: a call whose result is asserted \
             would widen M3's and M9's radii"
        );
    }

    #[test]
    fn t12_invariant_10_doc_block_slices_and_is_non_empty() {
        // Narrowed by P13-S26 pin 8. The exact (token, target) comparison lives
        // in epiphany-testkit's `invariant_ten_surface` guard, which reads both
        // this block and `core_spec.tex`. This one stays because
        // `cargo test -p epiphany-core` must still fail when the block is
        // destroyed, and testkit is a different crate.
        let source = production_source();
        let start = source
            .find("    /// 10. Every graph reference resolves")
            .expect("invariant 10's doc comment is present");
        let end = source[start..]
            .find("CrossCuttingRefsResolve,")
            .map(|offset| start + offset)
            .expect("the CrossCuttingRefsResolve variant follows its doc comment");
        let doc_block = &source[start..end];

        let tokens: Vec<&str> = doc_block
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("/// "))
            .filter_map(|rest| rest.trim_start().strip_prefix("- "))
            .filter_map(|rest| rest.split_whitespace().next())
            .collect();

        assert!(
            !tokens.is_empty(),
            "invariant 10's doc block must list its reference surface, one \
             `- Token — target.` line per class; block was:\n{doc_block}"
        );
    }
}

/// Genesis tranche G3b packet 2 (`spec/CONTRACT_GENESIS_G3B_MEASURE.md`):
/// mutations M34-M39 for invariant 20 (M40 lives in `g3b_dispatch_tests`
/// below, since it targets `check_invariants`' dispatch rather than the
/// check body).
#[cfg(test)]
mod g3b_measure20_tests {
    use super::*;
    use crate::event::Rest;
    use crate::graph::{
        BeatGroup, Measure, MeterChange, MetricGrid, MetricTimeModel, PowerOfTwo, Region,
        RegionContent, RegionTimeModel, StaffBasedContent, StaffExtent, StaffInstance, TimeExtent,
        TimeSignature, TimeSignatureDisplay,
    };
    use crate::ids::{
        IdentityContext, MeasureId, RegionId, ReplicaId, StaffId, StaffInstanceId, TimeSignatureId,
    };
    use crate::time::{
        AnchorOffset, MusicalDuration, RationalTime, RegionEdge, WallClockDuration, WallClockTime,
    };

    fn fires(score: &Score, inv: GraphInvariant) -> bool {
        !check_invariant(score, inv).is_empty()
    }

    /// A time signature with `measure_duration` one whole note.
    fn sig(replica: ReplicaId, n: u64) -> (TimeSignatureId, TimeSignature) {
        let id = TimeSignatureId::new(replica, n);
        let measure_duration = MusicalDuration::whole();
        let ts = TimeSignature::new(
            id,
            TimeSignatureDisplay::Standard {
                numerator: 4,
                denominator: PowerOfTwo::new(4).unwrap(),
            },
            measure_duration.clone(),
            vec![BeatGroup {
                duration: measure_duration,
                subdivision: None,
                accent: 0,
            }],
        )
        .unwrap();
        (id, ts)
    }

    /// A minimal metric-region score: one staff instance carrying
    /// `measures`, and (if `active` is `Some`) a region-default grid naming
    /// it, active from the region's own start.
    fn score_with(
        active: Option<TimeSignatureId>,
        declared: Vec<TimeSignature>,
        measures: Vec<Measure>,
    ) -> (Score, RegionId) {
        let replica = ReplicaId(7);
        let mut idc = IdentityContext::new(replica);
        let region_id: RegionId = idc.mint();
        let staff_id: StaffId = idc.mint();
        let instance_id: StaffInstanceId = idc.mint();
        let mut instance = StaffInstance::new(instance_id, staff_id);
        instance.measures = measures;
        let region_start = TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Zero,
        };
        let default_metric_grid = active.map(|active_sig| MetricGrid {
            meter_sequence: vec![MeterChange {
                anchor: region_start,
                time_signature: active_sig,
            }],
        });
        let region = Region {
            id: region_id,
            time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![instance],
                default_metric_grid,
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
                staves: vec![staff_id],
            },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        };
        let mut score = Score::empty(idc.clone());
        score.identity = idc;
        score.time_signatures = declared;
        score.canvas.regions = vec![region];
        (score, region_id)
    }

    /// A measure anchored at `whole_notes` whole notes after `region`'s
    /// start (c4-comparable to a grid entry built the same way by
    /// [`score_with`], since both use `TimeAnchor::Region{edge: Start, ..}`).
    fn measure_at(
        id: MeasureId,
        region: RegionId,
        whole_notes: i32,
        declared: Option<TimeSignatureId>,
    ) -> Measure {
        Measure {
            id,
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: if whole_notes == 0 {
                    AnchorOffset::Zero
                } else {
                    AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(whole_notes)))
                },
            },
            time_signature: declared,
            explicit_number: None,
            number_visibility: Default::default(),
        }
    }

    /// Discovers the region id `score_with` will mint (its `IdentityContext`
    /// is deterministic given no prior mints), so a test can build
    /// [`Measure`]s referencing it before constructing the real score.
    fn probe_region_id() -> RegionId {
        let (_, region) = score_with(None, vec![], vec![]);
        region
    }

    /// Inserts a minimal LIVE `Rest` event into `score` and returns its id
    /// (`spec/CONTRACT_P13S18_MATRIX.md` pin 4, S6/S7): `Measure.start` and
    /// `MeterChange.anchor` are unrestricted `TimeAnchor`s, so an
    /// `Event`-anchored measure is a legal graph, not a hypothetical one --
    /// the event this mints and inserts is genuinely live in `score.events`,
    /// not a dangling/ghost id (contrast `inv10_flags_dangling_spanner_
    /// anchor`'s `ghost_event`, in `review_fix_tests` above, which is the
    /// deliberately-NOT-live case).
    fn insert_live_event(score: &mut Score) -> crate::ids::EventId {
        let event_id: crate::ids::EventId = score.identity.mint();
        let voice_id: crate::ids::VoiceId = score.identity.mint();
        score
            .events
            .insert(Event::Rest(Rest {
                id: event_id,
                voice: voice_id,
                position: EventPosition::Musical(MusicalPosition(RationalTime::from_int(0))),
                duration: EventDuration::Musical(MusicalDuration::whole()),
                vertical_position: None,
                visible: true,
            }))
            .unwrap();
        event_id
    }

    #[test]
    fn agreement_and_boundary_hold_together() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, Some(active));
        let m1 = measure_at(MeasureId::new(replica, 11), region, 1, Some(active));
        let (score, _) = score_with(Some(active), vec![ts_active], vec![m0, m1]);
        assert!(
            !fires(&score, GraphInvariant::MeasureMeterConsistency),
            "agreeing, correctly-spaced measures must not violate invariant 20"
        );
    }

    /// M34: removing the agreement clause must let this go undetected.
    #[test]
    fn m34_agreement_flags_disagreement() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (declared, ts_declared) = sig(replica, 2);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, Some(declared));
        let (score, _) = score_with(Some(active), vec![ts_active, ts_declared], vec![m0]);
        assert!(
            fires(&score, GraphInvariant::MeasureMeterConsistency),
            "a measure declaring a signature that disagrees with the effective grid's \
             active signature must violate invariant 20"
        );
    }

    /// M35: this IS the pickup/anacrusis demonstration (spec/CONTRACT_P13S19_
    /// PARTIAL.md pin 1). `m0` is a first measure occupying only half a bar
    /// -- a pickup -- and by itself is flagged by neither clause: it has no
    /// predecessor, so the boundary clause is vacuous, and it avoids the
    /// agreement clause here by declaring `None` (not by any first-measure
    /// exemption -- there is none). `m1` is its successor, comparable and
    /// correctly ordered, but only HALF a `measure_duration` away -- the
    /// pickup's own actual length, not the governing signature's full bar.
    /// Removing the boundary clause must let THAT go undetected. Both
    /// measures avoid the agreement clause (`None`) so only boundary can
    /// fire.
    #[test]
    fn m35_pickup_successor_boundary_flags_wrong_distance() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, None);

        // The pickup by itself: a first measure has no predecessor, so the
        // boundary clause is vacuous for it, and `None` separately avoids
        // the agreement clause -- neither is an exemption granted TO the
        // agreement clause itself (pin 1).
        let (lone, _) = score_with(Some(active), vec![ts_active.clone()], vec![m0.clone()]);
        assert!(
            !fires(&lone, GraphInvariant::MeasureMeterConsistency),
            "the pickup by itself, first measure, no predecessor, must not be flagged"
        );

        // Half a whole note later — not a full measure_duration away.
        let m1 = Measure {
            id: MeasureId::new(replica, 11),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::new(1, 2).unwrap())),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (score, _) = score_with(Some(active), vec![ts_active], vec![m0, m1]);
        assert!(
            fires(&score, GraphInvariant::MeasureMeterConsistency),
            "a measure at the wrong distance from its predecessor must violate invariant 20 \
             even though neither declares a time signature"
        );
    }

    /// M36: `None` avoids only agreement, not boundary consistency.
    #[test]
    fn m36_none_still_bound_by_boundary() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, Some(active));
        // Wrong distance, and `None` (so agreement can never be the cause).
        let m1 = Measure {
            id: MeasureId::new(replica, 11),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::new(3, 2).unwrap())),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (score, _) = score_with(Some(active), vec![ts_active], vec![m0, m1]);
        assert!(
            fires(&score, GraphInvariant::MeasureMeterConsistency),
            "None exempts only agreement -- a None measure at the wrong distance must still \
             violate invariant 20's boundary clause"
        );
    }

    /// M37: an incomparable case (cross-clock offsets) must ABSTAIN, not
    /// flag.
    #[test]
    fn m37_incomparable_abstains() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let region = probe_region_id();
        // The grid's own entry is anchored at the region's start with a
        // WallClock offset — cross-clock against a Musical-offset measure
        // start sharing the same id/edge, so it is incomparable (pin 6).
        let grid = MetricGrid {
            meter_sequence: vec![MeterChange {
                anchor: TimeAnchor::Region {
                    id: region,
                    edge: RegionEdge::Start,
                    offset: AnchorOffset::WallClock(WallClockDuration(0)),
                },
                time_signature: active,
            }],
        };
        let m0 = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration::whole()),
            },
            time_signature: Some(active),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (mut score, _) = score_with(None, vec![ts_active], vec![]);
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(grid);
            content.staff_instances[0].measures.push(m0);
        }
        assert!(
            !fires(&score, GraphInvariant::MeasureMeterConsistency),
            "an incomparable grid entry must make the check ABSTAIN, not flag -- an \
             incomparable change is unplaced, not absent, and might have governed"
        );
    }

    /// M38: a pickup (partial) first measure declaring no time signature
    /// must never be flagged BY THE BOUNDARY CLAUSE -- it has no
    /// predecessor, so that clause is vacuous for it. This is narrower than
    /// "never flagged" in general: the agreement clause is not
    /// predecessor-dependent and would apply to this same measure if it
    /// declared a disagreeing signature instead of `None` (spec/CONTRACT_
    /// P13S19_PARTIAL.md pin 1) -- this fixture's `None` avoids agreement
    /// separately, not as a consequence of being a first measure.
    ///
    /// **On the mutation's failure mode:** the M38 mutation (removing the
    /// `if i == 0 { continue; }` guard in `check_measure_meter_consistency`)
    /// is observed as an `attempt to subtract with overflow` PANIC, not a
    /// wrong-flag assertion failure. This is expected and still a valid red
    /// signal, not a weak one: that `i == 0` guard is simultaneously the
    /// pickup-measure boundary-clause exemption AND the only thing standing
    /// between `i - 1` and a `usize` underflow, so any mutation that removes
    /// or weakens it crashes before it could ever produce a wrong (but
    /// well-formed) verdict to assert against.
    #[test]
    fn m38_pickup_first_measure_boundary_clause_not_flagged() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let region = probe_region_id();
        // A single, lone first measure at a nonzero, arbitrary offset --
        // there is no predecessor to be "the wrong distance" from.
        let m0 = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::new(1, 3).unwrap())),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (score, _) = score_with(Some(active), vec![ts_active], vec![m0]);
        assert!(
            !fires(&score, GraphInvariant::MeasureMeterConsistency),
            "a lone first (pickup) measure declaring no time signature must not be flagged \
             by invariant 20's boundary clause -- vacuous, no predecessor (its agreement \
             clause is separately avoided by `None`, not exempted by being first)"
        );
    }

    /// M39: an unresolvable measure time-signature reference is invariant
    /// 10's business, not invariant 20's -- invariant 20 must NOT duplicate
    /// the resolution check.
    #[test]
    fn m39_unresolvable_reference_is_invariant_10_only() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let undeclared = TimeSignatureId::new(replica, 999);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, Some(undeclared));
        let (score, _) = score_with(Some(active), vec![ts_active], vec![m0]);
        assert!(
            fires(&score, GraphInvariant::CrossCuttingRefsResolve),
            "an undeclared measure time-signature reference must violate invariant 10"
        );
        assert!(
            !fires(&score, GraphInvariant::MeasureMeterConsistency),
            "invariant 20 must NOT duplicate invariant 10's resolution check -- an \
             unresolving reference is only invariant 10's violation"
        );
    }

    // -------------------------------------------------------------------
    // spec/CONTRACT_P13S18_MATRIX.md: the outcome matrix (9 shapes x 2
    // clauses = 18 cells) plus the four non-shape-driven paths (A1, A3, B2,
    // B3) that pin 4's shapes do not themselves exercise. A2 and B1 reuse
    // `m39_unresolvable_reference_is_invariant_10_only` and
    // `m38_pickup_first_measure_boundary_clause_not_flagged` above
    // respectively; S3 reuses `m34_agreement_flags_disagreement` and
    // `m35_pickup_successor_boundary_flags_wrong_distance` above (Region
    // same id, same edge, Musical offsets is exactly their shape). Every
    // other cell gets a dedicated fixture below.
    // -------------------------------------------------------------------

    /// Matrix cell A1: `m.time_signature` is `None` -- inapplicable, not
    /// concealed disagreement. A single (first) measure keeps the boundary
    /// clause vacuous (B1) so only A1 is live here. M4 flips this measure to
    /// a resolving, DISAGREEING `Some(id)` and must turn this from silent to
    /// a violation.
    #[test]
    fn matrix_a1_none_time_signature_inapplicable() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, None);
        let (score, _) = score_with(Some(active), vec![ts_active], vec![m0]);
        assert!(
            !fires(&score, GraphInvariant::MeasureMeterConsistency),
            "A1: a measure with no declared time signature must not be \
             flagged by the agreement clause"
        );
    }

    /// Matrix cell A3: `Governing20::None` -- vacuous, not a violation,
    /// because the region-default grid is empty (pin 6c case 1) so there is
    /// no candidate at all. A single (first) measure keeps the boundary
    /// clause vacuous (B1) so only A3 is live here. A bare `fires` boolean
    /// cannot distinguish "vacuous" from any of the other eight paths, so
    /// this uses a paired positive control: fixture 1 is the claimed
    /// abstention (`m0` declares a DISAGREEING, resolving signature against
    /// an EMPTY grid); fixture 2 changes ONLY the grid -- from empty to a
    /// real entry naming the very signature `m0` disagrees with -- and must
    /// decide and flag. The flip from fixture 1 to fixture 2 is exactly
    /// M10's edit, now inside the test.
    #[test]
    fn matrix_a3_vacuous_agreement() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, Some(wrong));
        // Fixture 1: no grid at all, so there is no candidate for the
        // governing search to find regardless of what m0 declares.
        let (empty_score, _) = score_with(
            None,
            vec![ts_active.clone(), ts_wrong.clone()],
            vec![m0.clone()],
        );
        let empty_violations =
            check_invariant(&empty_score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            empty_violations.len(),
            0,
            "A3: with no grid entries at all, the agreement clause must \
             abstain as vacuous -- got {empty_violations:?}"
        );
        // Fixture 2 (positive control): ONLY the grid changes, from empty
        // to a real, disagreeing entry.
        let (populated_score, _) = score_with(Some(active), vec![ts_active, ts_wrong], vec![m0]);
        let populated_violations =
            check_invariant(&populated_score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            populated_violations.len(),
            1,
            "A3 positive control: a non-empty, disagreeing grid must decide \
             and flag -- got {populated_violations:?}"
        );
        assert!(
            populated_violations[0]
                .witness
                .contains("declares time signature"),
            "A3 positive control: the violation must be the agreement \
             clause's -- witness was {:?}",
            populated_violations[0].witness
        );
    }

    /// Matrix cell B2 (delegated -- verified, not asserted, pin 3): the
    /// SAME graph that makes invariant 20's boundary clause abstain (B2,
    /// the grid's OWN governing signature does not resolve) must
    /// independently violate invariant 10's grid-level resolution arm.
    /// `m39_unresolvable_reference_is_invariant_10_only` (above) is the twin
    /// observation for A2 (the MEASURE's own declared signature failing to
    /// resolve); this is the grid-entry case, and M8 shows the SAME
    /// condition go unreported by the whole suite once invariant 10's
    /// grid-level arm is deleted.
    #[test]
    fn matrix_b2_governing_signature_unresolving_delegated() {
        let replica = ReplicaId(7);
        let undeclared = TimeSignatureId::new(replica, 999);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, None);
        let m1 = measure_at(MeasureId::new(replica, 11), region, 1, None);
        // The INSTANCE-LOCAL grid's own entry (not the region default --
        // M8 targets invariant 10's instance-local-grid arm specifically)
        // names a signature that is never declared -- unresolvable by
        // either invariant's own logic, but reported ONLY by invariant 10.
        let (mut score, _) = score_with(None, vec![], vec![m0, m1]);
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.staff_instances[0].local_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Region {
                        id: region,
                        edge: RegionEdge::Start,
                        offset: AnchorOffset::Zero,
                    },
                    time_signature: undeclared,
                }],
            });
        }
        assert!(
            fires(&score, GraphInvariant::CrossCuttingRefsResolve),
            "the instance-local grid's undeclared time-signature reference \
             must violate invariant 10's instance-local-grid arm"
        );
        assert!(
            !fires(&score, GraphInvariant::MeasureMeterConsistency),
            "B2: invariant 20's boundary clause must abstain rather than \
             duplicate invariant 10's resolution check -- the grid's own \
             governing signature does not resolve"
        );
    }

    /// Matrix cell B3: `Governing20::None` at the boundary clause -- vacuous,
    /// same as A3, because the region-default grid is empty (pin 6c case
    /// 1). A wrong boundary distance (2 whole notes, not 1) is baked in
    /// deliberately so the abstention is load-bearing. As with A3, a bare
    /// boolean cannot distinguish "vacuous" from the other eight paths, so
    /// this uses a paired positive control: fixture 1 is the claimed
    /// abstention (empty grid, wrong distance already staged); fixture 2
    /// changes ONLY the grid -- from empty to a real, comparable entry --
    /// and the SAME wrong distance must now be flagged. The flip from
    /// fixture 1 to fixture 2 is M10's edit, now inside the test.
    #[test]
    fn matrix_b3_vacuous_boundary() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let region = probe_region_id();
        let m0 = measure_at(MeasureId::new(replica, 10), region, 0, None);
        let m1 = measure_at(MeasureId::new(replica, 11), region, 2, None);
        // Fixture 1: no grid at all.
        let (empty_score, _) =
            score_with(None, vec![ts_active.clone()], vec![m0.clone(), m1.clone()]);
        let empty_violations =
            check_invariant(&empty_score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            empty_violations.len(),
            0,
            "B3: with no grid entries at all, the boundary clause must \
             abstain as vacuous -- got {empty_violations:?}"
        );
        // Fixture 2 (positive control): ONLY the grid changes, from empty
        // to a real, comparable entry -- the SAME wrong distance (2 whole
        // notes, not 1) is untouched.
        let (populated_score, _) = score_with(Some(active), vec![ts_active], vec![m0, m1]);
        let populated_violations =
            check_invariant(&populated_score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            populated_violations.len(),
            1,
            "B3 positive control: a non-empty, comparable grid must decide \
             and flag the wrong distance -- got {populated_violations:?}"
        );
        assert!(
            populated_violations[0]
                .witness
                .contains("is not exactly one"),
            "B3 positive control: the violation must be the boundary \
             clause's -- witness was {:?}",
            populated_violations[0].witness
        );
    }

    /// Matrix row S1 (`WallClock` measures, `WallClock`-anchored meter
    /// changes): agreement DECIDES (`D`) and boundary abstains as B5 --
    /// pin 5's split. `m0`'s agreement clause and `m1`'s boundary clause
    /// share the EXACT SAME governing search
    /// (`measure20_governing_time_signature(sequence, &m0.start)`), so
    /// `m0`'s violation firing is itself the proof that search decided
    /// `Unique`, not `Indeterminate` -- ruling out B4 and pinning the
    /// boundary abstention to B5 (the `WallClock` delta, structurally never
    /// computable) rather than an incomparable governing search. `m1` sits
    /// at a deliberately wrong `WallClock` distance from `m0`, so a live
    /// boundary clause would flag it; only ONE violation (agreement, on
    /// `m0`) is observed.
    #[test]
    fn matrix_s1_wallclock_measures_wallclock_meter_changes() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let m0 = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let m1 = Measure {
            id: MeasureId::new(replica, 11),
            start: TimeAnchor::WallClock {
                time: WallClockTime(999_999_999),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (mut score, _) = score_with(None, vec![ts_active, ts_wrong], vec![]);
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::WallClock {
                        time: WallClockTime(0),
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![m0, m1];
        }
        let violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            violations.len(),
            1,
            "S1: expected exactly one violation (m0's agreement) -- got {violations:?}"
        );
        assert!(
            violations[0].witness.contains("declares time signature"),
            "S1: the sole violation must be the agreement clause's, not the \
             boundary clause's -- witness was {:?}",
            violations[0].witness
        );
    }

    /// Matrix row S2, agreement cell (`WallClock` measures, `Region`-
    /// anchored meter changes): `comparable_order` has no arm for
    /// `WallClock`<->`Region` at all (falls to the catch-all `_ => None`)
    /// -- structurally incomparable regardless of any offset or timestamp.
    /// `x` (a single, `WallClock`-anchored measure) declares a resolving,
    /// disagreeing signature and is not flagged: A4. Paired positive
    /// control: fixture 2 changes ONLY `x.start`, from `WallClock` to the
    /// grid's own `Region` shape, and the SAME disagreement must decide and
    /// flag -- M1 is fixture 1's `x.start` edit.
    #[test]
    fn matrix_s2_agreement_a4() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let x_wallclock = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        // Fixture 1: `x` is WallClock-anchored -- incomparable to the
        // Region-anchored grid.
        let (mut score, region) = score_with(
            Some(active),
            vec![ts_active.clone(), ts_wrong.clone()],
            vec![],
        );
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.staff_instances[0].measures = vec![x_wallclock];
        }
        let wallclock_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            wallclock_violations.len(),
            0,
            "S2 agreement: a WallClock-anchored measure against a \
             Region-anchored grid must abstain (A4) -- got \
             {wallclock_violations:?}"
        );
        // Fixture 2 (positive control): ONLY `x.start` changes, to the
        // grid's own Region{Start, Zero} shape.
        let x_region = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Zero,
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (mut score2, _) = score_with(Some(active), vec![ts_active, ts_wrong], vec![]);
        if let RegionContent::StaffBased(content) = &mut score2.canvas.regions[0].content {
            content.staff_instances[0].measures = vec![x_region];
        }
        let region_violations = check_invariant(&score2, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            region_violations.len(),
            1,
            "S2 agreement positive control: a Region-anchored measure \
             matching the grid must decide and flag -- got \
             {region_violations:?}"
        );
        assert!(
            region_violations[0]
                .witness
                .contains("declares time signature"),
            "S2 agreement positive control: the violation must be the \
             agreement clause's -- witness was {:?}",
            region_violations[0].witness
        );
    }

    /// Matrix row S2, boundary cell: `x` (index 1, `WallClock`-anchored,
    /// carrying a resolving disagreeing signature so it is structurally
    /// eligible for A4 too -- see `matrix_s2_agreement_a4`) is preceded by
    /// `prev` (index 0, also `WallClock`-anchored): B4. A `WallClock`
    /// measure's boundary clause can NEVER decide-and-flag (`measure20_
    /// musical_delta` has no `WallClock` arm at all, pin 5/S1), so the
    /// positive control cannot be "x's boundary now flags" -- instead it
    /// changes ONLY `prev.start` to the grid's `Region` shape and observes
    /// `prev`'s OWN agreement clause, which reuses the IDENTICAL governing
    /// search `x`'s boundary clause performs on `prev.start`. That the
    /// identical search decides once `prev.start` alone is comparable is
    /// the proof that the silence in fixture 1 was genuine indeterminacy
    /// (B4), not vacuity (B3) or non-resolution (B2) -- and `x`'s own
    /// boundary clause is asserted silent in BOTH fixtures, confirming the
    /// change didn't leak into a delta becoming computable. M2 is fixture
    /// 1's `prev.start` edit.
    #[test]
    fn matrix_s2_boundary_b4() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let prev_wallclock = Measure {
            id: MeasureId::new(replica, 9),
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let x = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::WallClock {
                time: WallClockTime(500),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        // Fixture 1: `prev` is WallClock-anchored -- incomparable to the
        // Region-anchored grid.
        let (mut score, region) = score_with(
            Some(active),
            vec![ts_active.clone(), ts_wrong.clone()],
            vec![],
        );
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.staff_instances[0].measures = vec![prev_wallclock, x.clone()];
        }
        let wallclock_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            wallclock_violations.len(),
            0,
            "S2 boundary: with `prev` WallClock-anchored against a \
             Region-anchored grid, both `prev`'s agreement and `x`'s \
             boundary must abstain (A4/B4) -- got {wallclock_violations:?}"
        );
        // Fixture 2 (positive control): ONLY `prev.start` changes, to the
        // grid's own Region{Start, Zero} shape. `x` is untouched.
        let prev_region = Measure {
            id: MeasureId::new(replica, 9),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Zero,
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (mut score2, _) = score_with(Some(active), vec![ts_active, ts_wrong], vec![]);
        if let RegionContent::StaffBased(content) = &mut score2.canvas.regions[0].content {
            content.staff_instances[0].measures = vec![prev_region, x];
        }
        let region_violations = check_invariant(&score2, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            region_violations.len(),
            1,
            "S2 boundary positive control: with `prev` now comparable to \
             the grid, `prev`'s OWN agreement clause (the identical \
             governing search `x`'s boundary performs) must decide and \
             flag -- got {region_violations:?}"
        );
        assert!(
            region_violations[0]
                .witness
                .contains("declares time signature"),
            "S2 boundary positive control: the violation must be `prev`'s \
             agreement clause's, not a new boundary violation on `x` -- a \
             WallClock delta is structurally never computable (pin 5), so \
             `x`'s boundary stays silent even though `prev`'s governing \
             search now decides -- witness was {:?}",
            region_violations[0].witness
        );
    }

    /// Matrix row S4: `Measure` **same id**, `pos: End` on both sides,
    /// `Musical` offsets (c2) -- the shape that falsifies "any `Measure`
    /// end anchor is incomparable": same id AND same `pos` decides via c2's
    /// offset comparison, exactly like same-id `Start`. Both clauses DECIDE
    /// (`D`/`D`), deliberately wrong so the decision is observed as a
    /// violation rather than a pass that could have come from anywhere.
    #[test]
    fn matrix_s4_measure_same_id_end_end_decides() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let shared_end = MeasureId::new(replica, 500);
        let m0 = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Measure {
                id: shared_end,
                position: MeasurePosition::End,
                offset: AnchorOffset::Musical(MusicalDuration::zero()),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let m1 = Measure {
            id: MeasureId::new(replica, 11),
            start: TimeAnchor::Measure {
                id: shared_end,
                position: MeasurePosition::End,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(2))),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (mut score, _) = score_with(None, vec![ts_active, ts_wrong], vec![]);
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Measure {
                        id: shared_end,
                        position: MeasurePosition::End,
                        offset: AnchorOffset::Musical(MusicalDuration::zero()),
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![m0, m1];
        }
        let violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            violations.len(),
            2,
            "S4: same-id End<->End must decide BOTH clauses (D/D) -- got \
             {violations:?}"
        );
        assert!(
            violations
                .iter()
                .any(|v| v.witness.contains("declares time signature")),
            "S4: expected an agreement violation on m0 -- got {violations:?}"
        );
        assert!(
            violations
                .iter()
                .any(|v| v.witness.contains("is not exactly one")),
            "S4: expected a boundary violation on m1 -- got {violations:?}"
        );
    }

    /// Matrix row S5 (`Measure` **distinct** ids, `Start`, `Zero` -- c3):
    /// the contrast to S4. c3's vector-index ordering DOES decide
    /// (agreement: `D`) but supplies no distance at all (boundary: B5) --
    /// pin 10's second deficiency. Three measures in one instance:
    /// `filler` (index 0, purely an anchor target), `prev` (index 1) and
    /// `m` (index 2, under test). The grid's sole entry is anchored to
    /// `filler`'s id -- DISTINCT from both `prev`'s and `m`'s own
    /// self-referencing ids -- so every governing search below goes
    /// through c3's vector order, never c2. `m` declares a disagreeing
    /// signature (agreement fires); the boundary delta between `prev` and
    /// `m` is structurally impossible (distinct ids), so it must abstain
    /// regardless of distance -- no wrong distance is even staged here.
    #[test]
    fn matrix_s5_measure_distinct_ids_start_zero() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let filler_id = MeasureId::new(replica, 20);
        let prev_id = MeasureId::new(replica, 21);
        let m_id = MeasureId::new(replica, 22);
        let filler = Measure {
            id: filler_id,
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let prev = Measure {
            id: prev_id,
            start: TimeAnchor::Measure {
                id: prev_id,
                position: MeasurePosition::Start,
                offset: AnchorOffset::Zero,
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let m = Measure {
            id: m_id,
            start: TimeAnchor::Measure {
                id: m_id,
                position: MeasurePosition::Start,
                offset: AnchorOffset::Zero,
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let (mut score, _) = score_with(None, vec![ts_active, ts_wrong], vec![]);
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Measure {
                        id: filler_id,
                        position: MeasurePosition::Start,
                        offset: AnchorOffset::Zero,
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![filler, prev, m];
        }
        let violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            violations.len(),
            1,
            "S5: expected exactly one violation (m's agreement, decided via \
             c3's vector order) -- got {violations:?}"
        );
        assert!(
            violations[0].witness.contains("declares time signature"),
            "S5: the sole violation must be the agreement clause's -- \
             boundary (B5) has no distance to report even though its \
             governing search also decided via c3 -- witness was {:?}",
            violations[0].witness
        );
    }

    /// Matrix row S6 (`Event` **same id**, a LIVE event, `Musical` offsets
    /// -- c1): `Measure.start` and `MeterChange.anchor` are unrestricted
    /// `TimeAnchor`s, so an `Event`-anchored measure is a legal (if
    /// unusual) graph, not a hypothetical one -- the event inserted here is
    /// genuinely live in `score.events`. Both clauses DECIDE (`D`/`D`),
    /// deliberately wrong.
    #[test]
    fn matrix_s6_event_same_id_live_event_decides() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let (mut score, _) = score_with(None, vec![ts_active, ts_wrong], vec![]);
        let live = insert_live_event(&mut score);
        let m0 = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Event {
                id: live,
                offset: AnchorOffset::Musical(MusicalDuration::zero()),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let m1 = Measure {
            id: MeasureId::new(replica, 11),
            start: TimeAnchor::Event {
                id: live,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(2))),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Event {
                        id: live,
                        offset: AnchorOffset::Musical(MusicalDuration::zero()),
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![m0, m1];
        }
        let violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            violations.len(),
            2,
            "S6: same-id live-Event anchors must decide BOTH clauses (D/D) \
             -- got {violations:?}"
        );
        assert!(
            violations
                .iter()
                .any(|v| v.witness.contains("declares time signature")),
            "S6: expected an agreement violation on m0 -- got {violations:?}"
        );
        assert!(
            violations
                .iter()
                .any(|v| v.witness.contains("is not exactly one")),
            "S6: expected a boundary violation on m1 -- got {violations:?}"
        );
    }

    /// Matrix row S7, agreement cell (`Event`, DISTINCT ids, otherwise
    /// identical to S6): isolates the distinct-`Event` fall-through --
    /// unlike `Measure`'s c3, there is no vector-index fallback for
    /// `Event`s, so distinct ids are UNCONDITIONALLY incomparable. `x` (a
    /// single, live-`Event`-anchored measure referencing `event_a`)
    /// declares a resolving, disagreeing signature against a grid entry
    /// referencing the DISTINCT `event_b`: A4. Paired positive control:
    /// fixture 2 changes ONLY the grid's referent, from `event_b` to
    /// `event_a` (matching `x`), and the SAME disagreement must decide and
    /// flag -- exactly the case the retained P11-C5 citation at
    /// `CONTRACT_GENESIS_G3B_MEASURE.md:223` exists for. (M1/M2, the
    /// contract's ratified A4/B4 mutation pair, target S8 below; this cell
    /// has no separately-numbered mutation, matching A3/A2's ratified
    /// scope.)
    #[test]
    fn matrix_s7_agreement_a4() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let (mut score, _) = score_with(None, vec![ts_active.clone(), ts_wrong.clone()], vec![]);
        let event_a = insert_live_event(&mut score);
        let event_b = insert_live_event(&mut score);
        let x = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Event {
                id: event_a,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(2))),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        // Fixture 1: the grid references `event_b`, distinct from `x`'s
        // own `event_a`.
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Event {
                        id: event_b,
                        offset: AnchorOffset::Musical(MusicalDuration::zero()),
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![x];
        }
        let distinct_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            distinct_violations.len(),
            0,
            "S7 agreement: a distinct-Event-id grid entry must abstain \
             (A4) -- got {distinct_violations:?}"
        );
        // Fixture 2 (positive control): ONLY the grid's referent changes,
        // from `event_b` to `event_a`, matching `x`.
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Event {
                        id: event_a,
                        offset: AnchorOffset::Musical(MusicalDuration::zero()),
                    },
                    time_signature: active,
                }],
            });
        }
        let matching_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            matching_violations.len(),
            1,
            "S7 agreement positive control: a matching-Event-id grid entry \
             must decide and flag -- got {matching_violations:?}"
        );
        assert!(
            matching_violations[0]
                .witness
                .contains("declares time signature"),
            "S7 agreement positive control: the violation must be the \
             agreement clause's -- witness was {:?}",
            matching_violations[0].witness
        );
    }

    /// Matrix row S7, boundary cell: `x` (index 1, `Event`-anchored to
    /// `event_a`, carrying a resolving disagreeing signature so it is
    /// structurally eligible for A4 too -- see `matrix_s7_agreement_a4`) is
    /// preceded by `prev` (index 0, ALSO `Event`-anchored to `event_a` --
    /// `prev` and `x` already share a referent; only the grid's `event_b`
    /// is the outlier): B4. Unlike S2's structurally-forced `WallClock`
    /// delta (never computable, pin 5), `measure20_musical_delta`'s
    /// `Event` arm decides fine once ids match, so the paired positive
    /// control here changes ONLY the grid's referent (`event_b` ->
    /// `event_a`, matching what `prev`, and `x`, already share) and THREE
    /// clauses decide and flag together: `prev`'s own agreement, `x`'s own
    /// agreement (it too references `event_a` and disagrees), and `x`'s
    /// boundary -- a stronger, triply-confirmed observation than S2's
    /// single witness, for the same underlying governing-search reason.
    #[test]
    fn matrix_s7_boundary_b4() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let (mut score, _) = score_with(None, vec![ts_active.clone(), ts_wrong.clone()], vec![]);
        let event_a = insert_live_event(&mut score);
        let event_b = insert_live_event(&mut score);
        let prev = Measure {
            id: MeasureId::new(replica, 9),
            start: TimeAnchor::Event {
                id: event_a,
                offset: AnchorOffset::Musical(MusicalDuration::zero()),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let x = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Event {
                id: event_a,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(2))),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        // Fixture 1: the grid references `event_b`, distinct from `prev`'s
        // own `event_a`.
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Event {
                        id: event_b,
                        offset: AnchorOffset::Musical(MusicalDuration::zero()),
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![prev.clone(), x.clone()];
        }
        let distinct_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            distinct_violations.len(),
            0,
            "S7 boundary: with `prev` referencing a distinct Event id from \
             the grid, both `prev`'s agreement and `x`'s boundary must \
             abstain (A4/B4) -- got {distinct_violations:?}"
        );
        // Fixture 2 (positive control): ONLY the grid's referent changes,
        // from `event_b` to `event_a`, matching `prev` (and `x`, since both
        // still reference `event_a`).
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Event {
                        id: event_a,
                        offset: AnchorOffset::Musical(MusicalDuration::zero()),
                    },
                    time_signature: active,
                }],
            });
        }
        let matching_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            matching_violations.len(),
            3,
            "S7 boundary positive control: with the grid now matching \
             `prev`, and `x` (same event id), THREE clauses all newly \
             comparable at once decide and flag: `prev`'s own agreement, \
             `x`'s own agreement (it too shares `event_a` and disagrees), \
             and `x`'s boundary (now a computable Event delta, unlike S2's \
             structurally-impossible WallClock case) -- got \
             {matching_violations:?}"
        );
        assert_eq!(
            matching_violations
                .iter()
                .filter(|v| v.witness.contains("declares time signature"))
                .count(),
            2,
            "S7 boundary positive control: expected agreement violations \
             on BOTH prev and x -- got {matching_violations:?}"
        );
        assert!(
            matching_violations
                .iter()
                .any(|v| v.witness.contains("is not exactly one")),
            "S7 boundary positive control: expected x's boundary violation \
             -- got {matching_violations:?}"
        );
    }

    /// Matrix row S8, agreement cell (matching referent, DIFFERING
    /// `pos`/`edge` selector): the `ia == ib && ea == eb` conjunction
    /// (Region) requires an IDENTICAL selector, never merely an identical
    /// id. `x` (a single, `Region`-anchored measure at `End`) declares a
    /// resolving, disagreeing signature against a grid entry at `Start`,
    /// same `Region` id: A4. Paired positive control: fixture 2 changes
    /// ONLY `x`'s own edge, from `End` to `Start` (matching the grid), and
    /// the SAME disagreement must decide and flag. M1 is fixture 1's
    /// `x`-edge edit.
    #[test]
    fn matrix_s8_agreement_a4() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let (mut score, region) =
            score_with(None, vec![ts_active.clone(), ts_wrong.clone()], vec![]);
        let x_end = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::End,
                offset: AnchorOffset::Zero,
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Region {
                        id: region,
                        edge: RegionEdge::Start,
                        offset: AnchorOffset::Zero,
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![x_end];
        }
        let end_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            end_violations.len(),
            0,
            "S8 agreement: an End-anchored measure against a Start-anchored \
             grid entry (same Region id) must abstain (A4) -- got \
             {end_violations:?}"
        );
        // Fixture 2 (positive control): ONLY `x`'s own edge changes, from
        // `End` to `Start`, matching the grid.
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.staff_instances[0].measures[0].start = TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Zero,
            };
        }
        let start_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            start_violations.len(),
            1,
            "S8 agreement positive control: a Start-anchored measure \
             matching the grid must decide and flag -- got \
             {start_violations:?}"
        );
        assert!(
            start_violations[0]
                .witness
                .contains("declares time signature"),
            "S8 agreement positive control: the violation must be the \
             agreement clause's -- witness was {:?}",
            start_violations[0].witness
        );
    }

    /// Matrix row S8, boundary cell: `x` (index 1, `Region`-anchored at
    /// `End`, carrying a resolving disagreeing signature so it is
    /// structurally eligible for A4 too -- see `matrix_s8_agreement_a4`) is
    /// preceded by `prev` (index 0, also `Region`-anchored at `End`): B4.
    /// The positive control moves the GRID entry's edge to `End`, not
    /// `prev`'s. Both levers restore the governing search, but only this one
    /// leaves `prev` and `x` c4-comparable to each other, so
    /// `measure20_musical_delta`'s Region arm (`ia == ib && ea == eb`) still
    /// yields a delta and **`x`'s own boundary clause fires**. Moving `prev`
    /// instead would break that match and leave the boundary silent for a
    /// second reason, signing this cell by inference from a shared call
    /// rather than by observing it. **S2 has no such lever** -- a `WallClock`
    /// delta is never computable (pin 5) -- so its control legitimately
    /// observes `prev`'s agreement, and that exception is S2's alone.
    /// M2 is fixture 1's grid-edge edit.
    #[test]
    fn matrix_s8_boundary_b4() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let (mut score, region) =
            score_with(None, vec![ts_active.clone(), ts_wrong.clone()], vec![]);
        let prev_end = Measure {
            id: MeasureId::new(replica, 9),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::End,
                offset: AnchorOffset::Zero,
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let x = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::End,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(2))),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Region {
                        id: region,
                        edge: RegionEdge::Start,
                        offset: AnchorOffset::Zero,
                    },
                    time_signature: active,
                }],
            });
            content.staff_instances[0].measures = vec![prev_end, x];
        }
        let end_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            end_violations.len(),
            0,
            "S8 boundary: with `prev` End-anchored against a Start-anchored \
             grid entry, both `prev`'s agreement and `x`'s boundary must \
             abstain (A4/B4) -- got {end_violations:?}"
        );
        // Fixture 2 (positive control): ONLY the grid entry's edge changes,
        // from `Start` to `End`, matching both measures. Neither measure
        // moves -- which is the point: `prev` and `x` stay Region{End} and
        // therefore stay c4-comparable to EACH OTHER, so the delta survives
        // and `x`'s boundary clause itself decides. Moving `prev` instead
        // would restore the governing search while breaking `prev`<->`x`,
        // leaving the boundary silent for a second reason and signing this
        // cell by inference rather than observation. (S2 has no such option:
        // a WallClock delta is structurally never computable, pin 5, so its
        // control legitimately observes `prev`'s agreement instead. That
        // exception is S2's alone.)
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content
                .default_metric_grid
                .as_mut()
                .expect("the grid was just installed")
                .meter_sequence[0]
                .anchor = TimeAnchor::Region {
                id: region,
                edge: RegionEdge::End,
                offset: AnchorOffset::Zero,
            };
        }
        let matching_violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            matching_violations.len(),
            3,
            "S8 boundary positive control: with the grid now End-anchored, \
             three clauses newly decide at once -- `prev`'s agreement, \
             `x`'s agreement, and `x`'s BOUNDARY (a computable c4 delta, \
             since both measures are still Region{{End}}) -- got \
             {matching_violations:?}"
        );
        assert_eq!(
            matching_violations
                .iter()
                .filter(|v| v.witness.contains("declares time signature"))
                .count(),
            2,
            "S8 boundary positive control: expected agreement violations on \
             BOTH prev and x -- got {matching_violations:?}"
        );
        assert!(
            matching_violations
                .iter()
                .any(|v| v.witness.contains("is not exactly one")),
            "S8 boundary positive control: expected `x`'s OWN boundary \
             violation, which is what attributes fixture 1's silence to B4 \
             rather than to a downstream delta failure -- got \
             {matching_violations:?}"
        );
    }

    /// Matrix row S9 (pin 7's behavioural lock): one instance, heterogeneous
    /// measure anchors. THREE measures, deliberately distinct roles so the
    /// pin-7 contrast and the row's own A4/B4 cell pair don't get
    /// conflated into one false "same measure" claim (the mistake defect 2
    /// found in this row's first draft):
    ///
    /// - `q` (index 0, `Region`-anchored, matching the grid) is pin 7's
    ///   REQUIRED contrast -- "another measure ... reaches a decision" --
    ///   and decides (deliberately wrong, so it flags).
    /// - `r` (index 1, `WallClock`-anchored, `None` declared) is purely
    ///   `p`'s predecessor; it carries no claim of its own.
    /// - `p` (index 2, `WallClock`-anchored like `r`, resolving disagreeing
    ///   signature) is the row's A4/B4 EXHIBIT: its own agreement clause
    ///   abstains (A4, `p.start` incomparable to the grid) AND its own
    ///   boundary clause abstains (B4, `r.start` -- `p`'s predecessor --
    ///   ALSO incomparable to the grid), on ONE measure, per defect 2.
    ///
    /// `q` cannot double as `p`'s predecessor: if it did, `p`'s boundary
    /// governing search would use `q.start`, which IS comparable to the
    /// grid (that's why `q` decides) -- and `p`'s boundary would then
    /// decide too, not abstain. Splitting the roles across THREE measures
    /// is what makes both claims true at once. Exactly ONE violation --
    /// naming `q`, never `p` or `r` -- is pin 7's behavioural proof; a
    /// prose claim that "one incomparable change disables the whole
    /// instance" would predict zero. M6 gives `q` the SAME `WallClock`
    /// shape as `r`/`p` and only then does its agreement clause stop
    /// deciding.
    #[test]
    fn matrix_s9_heterogeneous_measure_anchors() {
        let replica = ReplicaId(7);
        let (active, ts_active) = sig(replica, 1);
        let (wrong, ts_wrong) = sig(replica, 2);
        let (mut score, region) = score_with(Some(active), vec![ts_active, ts_wrong], vec![]);
        let q = Measure {
            id: MeasureId::new(replica, 9),
            start: TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration::zero()),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let q_id = q.id;
        let r = Measure {
            id: MeasureId::new(replica, 10),
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let p = Measure {
            id: MeasureId::new(replica, 11),
            start: TimeAnchor::WallClock {
                time: WallClockTime(500),
            },
            time_signature: Some(wrong),
            explicit_number: None,
            number_visibility: Default::default(),
        };
        if let RegionContent::StaffBased(content) = &mut score.canvas.regions[0].content {
            content.staff_instances[0].measures = vec![q, r, p];
        }
        let violations = check_invariant(&score, GraphInvariant::MeasureMeterConsistency);
        assert_eq!(
            violations.len(),
            1,
            "S9: exactly one measure's agreement clause (q's) must decide \
             in this instance -- got {violations:?}"
        );
        assert!(
            violations[0].witness.contains(&format!("{q_id:?}")),
            "S9: the sole violation must name q, proving p's abstention \
             (A4) and r's presence did not disable the whole instance -- \
             witness was {:?}",
            violations[0].witness
        );
    }
}

/// Genesis tranche G3b packet 2: M40, asserted BEHAVIOURALLY against
/// `check_invariants`' dispatch (`spec/CONTRACT_GENESIS_G3B_MEASURE.md` pin
/// 11) -- an `all().len()` count passes even with the dispatch arm deleted, so
/// this row must instead show a score violating ONLY invariant 20 is
/// actually flagged by the top-level `check_invariants` entry point.
#[cfg(test)]
mod g3b_dispatch_tests {
    use super::*;
    use crate::graph::{
        BeatGroup, Measure, MeterChange, MetricGrid, PowerOfTwo, RegionContent, TimeSignature,
        TimeSignatureDisplay,
    };
    use crate::ids::{MeasureId, TimeSignatureId};
    use crate::time::{AnchorOffset, MusicalDuration, RationalTime, RegionEdge};

    /// M40: deleting invariant 20's arm from `check_invariants`' dispatch
    /// must be observed here -- an `all().len()` count alone would not notice.
    #[test]
    fn m40_check_invariants_dispatches_invariant_20() {
        assert_eq!(GraphInvariant::all().len(), 21);
        let mut s = crate::generators::valid_score(4242);
        let replica = s.identity.replica_id;
        // Corrupt ONLY invariant 20: two measures, both `None` (so
        // agreement never fires), at the wrong boundary distance from each
        // other.
        let region_id = s.canvas.regions[0].id;
        let sig_id = TimeSignatureId::new(replica, 900);
        let measure_duration = MusicalDuration::whole();
        let ts_val = TimeSignature::new(
            sig_id,
            TimeSignatureDisplay::Standard {
                numerator: 4,
                denominator: PowerOfTwo::new(4).unwrap(),
            },
            measure_duration.clone(),
            vec![BeatGroup {
                duration: measure_duration,
                subdivision: None,
                accent: 0,
            }],
        )
        .unwrap();
        s.time_signatures.push(ts_val);
        let m0 = Measure {
            id: MeasureId::new(replica, 501),
            start: TimeAnchor::Region {
                id: region_id,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Zero,
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        let m1 = Measure {
            id: MeasureId::new(replica, 502),
            start: TimeAnchor::Region {
                id: region_id,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Musical(MusicalDuration(RationalTime::new(1, 3).unwrap())),
            },
            time_signature: None,
            explicit_number: None,
            number_visibility: Default::default(),
        };
        if let RegionContent::StaffBased(content) = &mut s.canvas.regions[0].content {
            content.default_metric_grid = Some(MetricGrid {
                meter_sequence: vec![MeterChange {
                    anchor: TimeAnchor::Region {
                        id: region_id,
                        edge: RegionEdge::Start,
                        offset: AnchorOffset::Zero,
                    },
                    time_signature: sig_id,
                }],
            });
            content.staff_instances[0].measures = vec![m0, m1];
        }
        assert!(
            !check_invariant(&s, GraphInvariant::MeasureMeterConsistency).is_empty(),
            "the score must violate invariant 20 directly"
        );
        assert!(
            check_invariants(&s).iter().any(
                |v| v.kind == ViolationKind::Invariant(GraphInvariant::MeasureMeterConsistency)
            ),
            "check_invariants (the top-level dispatch) must surface the invariant-20 \
             violation -- this is the behavioural assertion M40 needs, since \
             an all().len() count alone passes even with the dispatch arm deleted"
        );
    }
}

/// P13-S16 pins 6/6a/6b: invariant 21's **two directions**, each surfaced
/// through `check_invariants`' dispatch and each isolated to its own direction.
///
/// Both directions carry the same `GraphInvariant`, so **no `all()`-driven test
/// can tell them apart** — every such test is satisfied by either one. These two
/// are the only permanent guarantee that both are dispatched, which is what pin
/// 6b's two independently removable methods exist for.
///
/// **Each fixture violates its own direction ONLY**, asserted as an exact set.
/// A fixture disagreeing both ways would be reported after either arm was
/// deleted, so it would sign neither.
#[cfg(test)]
mod s16_agreement_dispatch_tests {
    use super::*;
    use crate::graph::{StaffGroup, StaffGroupKind};
    use crate::ids::StaffGroupId;

    /// (m41) **Breaks S→G**: a staff whose `group` names a group whose `members`
    /// omit it — the shape a pin-2 maintenance gap produces.
    ///
    /// **Holds G→S**: `members` is empty, so that direction has nothing to
    /// disagree about. **Holds every other invariant**, which is what the exact
    /// cardinality assertion proves and what `any()` could never touch.
    ///
    /// **Mutation (M6a):** delete the `check_staff_names_absent_group` call from
    /// `check_invariants`. This fixture then goes **unreported** — the assertion
    /// prints `0` against `1` with an empty vector, which is the observation M6a
    /// owes, quoted rather than inferred. `m41b` must still pass.
    #[test]
    fn m41_check_invariants_dispatches_invariant_21_staff_names_absent_group() {
        let mut s = crate::generators::valid_score(4243);
        let replica = s.identity.replica_id;
        let group_id = StaffGroupId::new(replica, 21_001);
        s.staff_groups.push(StaffGroup {
            id: group_id,
            name: None,
            kind: StaffGroupKind::Bracket,
            members: Vec::new(),
        });
        let staff_id = s.staves[0].id;
        s.staves[0].group = Some(group_id);

        // Bound before ANY assertion: the opposite-direction fact this fixture
        // depends on, then the violations. Nothing is asserted until the
        // cardinality check, which is the one M6a trips.
        let group_members: Option<Vec<StaffId>> = s
            .staff_groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.members.clone());
        let violations = check_invariants(&s);

        assert_eq!(
            violations.len(),
            1,
            "expected exactly the invariant-21 S->G violation and nothing else, \
             got {violations:?}"
        );
        assert_eq!(
            violations[0].kind,
            ViolationKind::Invariant(GraphInvariant::StaffGroupMembershipAgreement),
            "the single violation must be invariant 21, got {violations:?}"
        );
        assert!(
            violations[0].witness.starts_with("S->G:"),
            "the witness must name the S->G direction so this test cannot pass \
             on m41b's fixture, got {violations:?}"
        );
        assert!(
            violations[0].witness.contains(&format!("{staff_id:?}"))
                && violations[0].witness.contains(&format!("{group_id:?}")),
            "the witness must name both the staff and the group id, got \
             {violations:?}"
        );
        // The G->S direction holds, asserted directly rather than left to follow
        // from the cardinality above: the group must genuinely list nobody, and
        // no G->S witness may be present.
        assert_eq!(
            group_members.as_deref(),
            Some(&[][..]),
            "fixture: the group must list nobody, so only S->G disagrees"
        );
        assert!(
            !violations.iter().any(|v| v.witness.starts_with("G->S:")),
            "the G->S direction must hold on this fixture, got {violations:?}"
        );
    }

    /// (m41b) **Breaks G→S**: a group listing a staff whose own `group` is not
    /// that group — the shape a stale projection produces.
    ///
    /// **Holds S→G**: the listed staff's `group` is `None`, so it names no group
    /// and that direction abstains. **Holds every other invariant** — note the
    /// listed staff is genuinely declared, so this is a *disagreement*, not a
    /// dangling reference (invariant 21 abstains on those; invariant 10 owns
    /// them).
    ///
    /// **Mutation (M6b):** delete the `check_group_lists_unowned_staff` call from
    /// `check_invariants`. This fixture then goes unreported, printing `0`
    /// against `1`. **`m41b` is the ONLY test M6b breaks** — `m41`, the generator
    /// direction test and all four `all()` consumers use S→G fixtures, so
    /// without this test the G→S arm could be deleted and the suite would stay
    /// green.
    #[test]
    fn m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff() {
        let mut s = crate::generators::valid_score(4244);
        let replica = s.identity.replica_id;
        let group_id = StaffGroupId::new(replica, 21_002);
        let staff_id = s.staves[0].id;
        s.staff_groups.push(StaffGroup {
            id: group_id,
            name: None,
            kind: StaffGroupKind::Bracket,
            members: vec![staff_id],
        });

        // Bound before ANY assertion: the opposite-direction fact (`valid_score`
        // leaves every staff's `group` as `None`, which is what keeps S->G
        // abstaining), then the violations. This was asserted *before* the
        // binding in the first draft, which put a check ahead of the cardinality
        // assertion M6b trips — if the precondition ever broke, M6b's evidence
        // would be replaced by a fixture complaint.
        let listed_staff_group = s.staves[0].group;
        let violations = check_invariants(&s);

        assert_eq!(
            violations.len(),
            1,
            "expected exactly the invariant-21 G->S violation and nothing else, \
             got {violations:?}"
        );
        assert_eq!(
            violations[0].kind,
            ViolationKind::Invariant(GraphInvariant::StaffGroupMembershipAgreement),
            "the single violation must be invariant 21, got {violations:?}"
        );
        assert!(
            violations[0].witness.starts_with("G->S:"),
            "the witness must name the G->S direction so this test cannot pass \
             on m41's fixture, got {violations:?}"
        );
        assert!(
            violations[0].witness.contains(&format!("{staff_id:?}"))
                && violations[0].witness.contains(&format!("{group_id:?}")),
            "the witness must name both the staff and the group id, got \
             {violations:?}"
        );
        // The S->G direction holds, asserted directly: the listed staff must name
        // no group at all, and no S->G witness may be present. Without the first
        // of these the fixture could silently become a both-directions one, which
        // would be reported after EITHER arm was deleted and so would sign
        // neither.
        assert_eq!(
            listed_staff_group, None,
            "fixture: the listed staff must name no group, so only G->S disagrees"
        );
        assert!(
            !violations.iter().any(|v| v.witness.starts_with("S->G:")),
            "the S->G direction must hold on this fixture, got {violations:?}"
        );
    }
}

#[cfg(test)]
mod s29_violation_kind_tests {
    //! P13-S29 pin 10: the closed per-condition matrix, one fixture per emitted
    //! condition, plus the whole-surface tests.
    //!
    //! Requirement-arm assertions are on `(kind, witness)` **pairs**, never on
    //! the label alone: three labels are shared by two conditions each, and C6's
    //! natural fixture emits C6 *and* C7, so a label-only assertion survives
    //! mislabelling one of them.
    use super::*;
    use crate::accidental::{fixture_extensions, PitchSpaceModification};
    use crate::generators::valid_score;
    use crate::graph::{EventOrderingDAG, RegionTimeModel};
    use crate::pitch::PitchSpaceId;
    use crate::tempo::{Tempo, TempoMap, TempoSegment, TempoShape};
    use crate::time::{AnchorOffset, RationalTime, RegionEdge, TimeAnchor};

    const SHAPE: &str = "req:time:tempo-segment-shape";
    const ORDER: &str = "req:time:tempo-segment-order";
    const LOCALITY: &str = "req:time:aleatoric-reference-locality";
    const TUNING: &str = "req:tuning:accidental-modification-compatibility";

    fn anchor(id: crate::ids::RegionId, at: i32) -> TimeAnchor {
        TimeAnchor::Region {
            id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(crate::time::MusicalDuration(RationalTime::from_int(at))),
        }
    }

    /// A region id no score declares.
    fn ghost_region(s: &Score, n: u64) -> crate::ids::RegionId {
        crate::ids::RegionId::new(s.identity.replica_id, n)
    }

    fn tempo(bpm: f64) -> Tempo {
        Tempo::quarter(bpm).unwrap()
    }

    /// Pin 10's requirement-arm template: the aggregate carries the pair, the
    /// invariant selector does **not** carry it, `check_requirement` does, and
    /// the fixture's invariant arm is **empty** — the last being what keeps
    /// these seven structurally out of M2a's radius.
    fn assert_requirement_condition(s: &Score, label: &'static str, witness: &str) {
        let all = check_invariants(s);
        let expected = (ViolationKind::Requirement(label), witness.to_owned());
        let pairs: Vec<_> = all.iter().map(|v| (v.kind, v.witness.clone())).collect();
        assert!(
            pairs.contains(&expected),
            "aggregate must carry {expected:?}; got {pairs:?}"
        );
        assert!(
            check_invariant(s, GraphInvariant::CrossCuttingRefsResolve).is_empty(),
            "the rider must not answer to invariant 10; got {:?}",
            check_invariant(s, GraphInvariant::CrossCuttingRefsResolve)
        );
        let via = check_requirement(s, label);
        assert!(
            via.iter().any(|v| v.witness == witness),
            "check_requirement({label}) must return the pair; got {via:?}"
        );
        assert!(
            !all.iter()
                .any(|v| matches!(v.kind, ViolationKind::Invariant(_))),
            "fixture must violate no invariant at all; got {all:?}"
        );
    }

    fn assert_invariant_condition(s: &Score, witness_fragment: &str) {
        let all = check_invariants(s);
        assert!(
            all.iter().any(|v| v.kind
                == ViolationKind::Invariant(GraphInvariant::CrossCuttingRefsResolve)
                && v.witness.contains(witness_fragment)),
            "aggregate must carry invariant 10 for {witness_fragment:?}; got {all:?}"
        );
        assert!(
            check_invariant(s, GraphInvariant::CrossCuttingRefsResolve)
                .iter()
                .any(|v| v.witness.contains(witness_fragment)),
            "the invariant selector must return it"
        );
    }

    /// The production half of this file — everything before the first test
    /// module. `g3a_tests` has its own copy; that one is private to it, which
    /// pin 1b records as the reason its derives guard lives there rather than
    /// here.
    fn production_source() -> &'static str {
        include_str!("invariants.rs")
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("this file contains at least one #[cfg(test)] module")
    }

    fn one_segment(seed: u64, seg: impl Fn(crate::ids::RegionId) -> TempoSegment) -> Score {
        let mut s = valid_score(seed);
        let rid = s.canvas.regions[0].id;
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![seg(rid)],
        };
        s
    }

    // ---- C1-C3: the invariant arm -------------------------------------------

    #[test]
    fn cross_cutting_refs_stay_invariant_ten() {
        // Any condition of `check_cross_cutting_refs` serves; a staff's
        // declared instrument is the most stable across generator changes.
        let mut s = valid_score(400);
        let ghost = crate::ids::InstrumentId::new(s.identity.replica_id, 9_400_101);
        s.staves[0].instrument = ghost;
        assert_invariant_condition(&s, "is not declared");
    }

    #[test]
    fn tempo_start_anchor_stays_invariant_ten() {
        let base = valid_score(401);
        let ghost = ghost_region(&base, 9_400_201);
        let s = one_segment(401, |_| TempoSegment {
            start: anchor(ghost, 0),
            end: None,
            start_tempo: tempo(60.0),
            end_tempo: None,
            shape: TempoShape::Constant,
        });
        assert_invariant_condition(&s, "start anchor target");
    }

    #[test]
    fn tempo_end_anchor_stays_invariant_ten() {
        let base = valid_score(402);
        let ghost = ghost_region(&base, 9_400_202);
        let s = one_segment(402, |rid| TempoSegment {
            start: anchor(rid, 0),
            end: Some(anchor(ghost, 1)),
            start_tempo: tempo(60.0),
            end_tempo: None,
            shape: TempoShape::Constant,
        });
        assert_invariant_condition(&s, "end anchor target");
    }

    // ---- C4-C10: the requirement arm ----------------------------------------

    #[test]
    fn tempo_constant_mismatch_reports_shape() {
        let s = one_segment(403, |rid| TempoSegment {
            start: anchor(rid, 0),
            end: None,
            start_tempo: tempo(60.0),
            end_tempo: Some(tempo(120.0)),
            shape: TempoShape::Constant,
        });
        assert_requirement_condition(
            &s,
            SHAPE,
            "constant tempo segment has end_tempo != start_tempo",
        );
    }

    #[test]
    fn tempo_nonconstant_missing_end_reports_shape() {
        let s = one_segment(404, |rid| TempoSegment {
            start: anchor(rid, 0),
            end: Some(anchor(rid, 1)),
            start_tempo: tempo(60.0),
            end_tempo: None,
            shape: TempoShape::Linear,
        });
        assert_requirement_condition(
            &s,
            SHAPE,
            "non-constant tempo segment is missing its end_tempo",
        );
    }

    fn two_segments(seed: u64, a: (i32, i32), b: (i32, i32)) -> Score {
        let mut s = valid_score(seed);
        let rid = s.canvas.regions[0].id;
        let seg = |from: i32, to: i32| TempoSegment {
            start: anchor(rid, from),
            end: Some(anchor(rid, to)),
            start_tempo: tempo(60.0),
            end_tempo: None,
            shape: TempoShape::Constant,
        };
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![seg(a.0, a.1), seg(b.0, b.1)],
        };
        s
    }

    #[test]
    fn tempo_out_of_order_reports_order() {
        let s = two_segments(405, (2, 3), (1, 2));
        assert_requirement_condition(&s, ORDER, "tempo segments are out of start order");
    }

    #[test]
    fn tempo_overlap_reports_order() {
        let s = two_segments(406, (2, 3), (1, 2));
        assert_requirement_condition(&s, ORDER, "tempo segments overlap in musical time");
    }
    // ---- C8-C10 need their fixtures from the existing corpus ----------------

    fn aleatoric_idx_and_events(s: &Score) -> (usize, Vec<crate::ids::EventId>) {
        let idx = s
            .canvas
            .regions
            .iter()
            .position(|r| matches!(r.time_model, RegionTimeModel::Aleatoric(_)))
            .expect("valid_score_rich region C is aleatoric");
        let evs = s.canvas.regions[idx].staff_instances()[0].voices[0]
            .events
            .clone();
        (idx, evs)
    }

    #[test]
    fn aleatoric_ordering_outside_region_reports_locality() {
        let mut s = crate::generators::valid_score_rich(407);
        let (idx, evs) = aleatoric_idx_and_events(&s);
        let ghost = crate::ids::EventId::new(s.identity.replica_id, 9_400_401);
        let mut edges = std::collections::BTreeMap::new();
        edges.insert(evs[0], vec![ghost]);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.ordering = EventOrderingDAG::try_new(edges).unwrap();
        }
        assert_requirement_condition(
            &s,
            LOCALITY,
            &format!(
                "aleatoric region {:?} ordering references event {:?}, absent from the region",
                s.canvas.regions[idx].id, ghost
            ),
        );
    }

    #[test]
    fn aleatoric_bounds_outside_region_reports_locality() {
        let mut s = crate::generators::valid_score_rich(408);
        let (idx, _) = aleatoric_idx_and_events(&s);
        let ghost = crate::ids::EventId::new(s.identity.replica_id, 9_400_402);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.bounds.insert(ghost, crate::time::EventBounds::default());
        }
        assert_requirement_condition(
            &s,
            LOCALITY,
            &format!(
                "aleatoric region {:?} bounds key event {:?} is absent from the region",
                s.canvas.regions[idx].id, ghost
            ),
        );
    }

    #[test]
    fn accidental_incompatible_reports_tuning_requirement() {
        let mut s = valid_score(410);
        s.tuning_context.default_pitch_space = PitchSpaceId::new("edo-31");
        s.tuning_context
            .accidental_extensions
            .push(fixture_extensions(
                "cmn-accidentals",
                PitchSpaceModification::CmnChromatic(1),
            ));
        let via = check_requirement(&s, TUNING);
        assert!(!via.is_empty(), "expected a compatibility violation");
        assert!(
            via[0].witness.ends_with("interval algebra") && !via[0].witness.contains("req:"),
            "witness must be label-free and end with the algebra clause; got {:?}",
            via[0].witness
        );
        assert!(
            check_invariant(&s, GraphInvariant::CrossCuttingRefsResolve).is_empty(),
            "the rider must not answer to invariant 10"
        );
    }

    // ---- whole-surface -------------------------------------------------------

    #[test]
    fn graph_invariant_all_is_unchanged() {
        let seq: Vec<(GraphInvariant, u8)> = GraphInvariant::all()
            .into_iter()
            .map(|i| (i, i.number()))
            .collect();
        let expected: Vec<(GraphInvariant, u8)> = vec![
            (GraphInvariant::EventVoiceBacklink, 1),
            (GraphInvariant::VoiceEventBacklink, 2),
            (GraphInvariant::VoiceEventsSortedNonOverlap, 3),
            (GraphInvariant::EventCoordinateModel, 4),
            (GraphInvariant::ContainmentTree, 5),
            (GraphInvariant::StaffInstanceResolves, 6),
            (GraphInvariant::RegionExtents, 7),
            (GraphInvariant::MeasureSingleInstance, 8),
            (GraphInvariant::AnchorOffsetModel, 9),
            (GraphInvariant::CrossCuttingRefsResolve, 10),
            (GraphInvariant::UniqueIdentifiers, 11),
            (GraphInvariant::PitchIdUnique, 12),
            (GraphInvariant::SpellingScopeResolves, 13),
            (GraphInvariant::DecompositionTargetResolves, 14),
            (GraphInvariant::DecompositionSum, 15),
            (GraphInvariant::TupletSum, 16),
            (GraphInvariant::TiePairing, 17),
            (GraphInvariant::VoiceOriginConsistent, 18),
            (GraphInvariant::BarlineGroupSameRegion, 19),
            (GraphInvariant::MeasureMeterConsistency, 20),
            (GraphInvariant::StaffGroupMembershipAgreement, 21),
        ];
        assert_eq!(
            seq, expected,
            "GraphInvariant::all() is frozen by P13-S29 pin 8"
        );

        // `all()` is not the enum: a variant declared and omitted from `all()`
        // leaves the sequence untouched. Independent declaration inventory.
        let src = production_source();
        let a = src.find("pub enum GraphInvariant {").expect("declaration");
        let b = src[a..].find("\n}").expect("close") + a;
        let declared: Vec<&str> = src[a..b]
            .lines()
            .filter_map(|l| l.trim().strip_suffix(','))
            .filter(|l| !l.starts_with("///") && !l.starts_with("//"))
            .collect();
        let names: Vec<String> = expected.iter().map(|(i, _)| format!("{i:?}")).collect();
        assert_eq!(
            declared, names,
            "the enum declares exactly all()'s variants, in order"
        );
    }

    #[test]
    fn display_renders_each_arm_exactly() {
        // Invariant side: a reversed aleatoric bound (invariant 4), acquired
        // from the aggregate -- pinned, because acquisition decides M3's radius.
        let s = reversed_aleatoric_bound(411);
        let inv = check_invariants(&s)
            .into_iter()
            .find(|v| v.kind == ViolationKind::Invariant(GraphInvariant::EventCoordinateModel))
            .expect("an invariant-arm violation");
        assert_eq!(
            inv.to_string(),
            format!(
                "invariant 4 (EventCoordinateModel) violated: {}",
                inv.witness
            )
        );

        // Requirement side: a real accidental violation, from the aggregate.
        let mut s = valid_score(412);
        s.tuning_context.default_pitch_space = PitchSpaceId::new("edo-31");
        s.tuning_context
            .accidental_extensions
            .push(fixture_extensions(
                "cmn-accidentals",
                PitchSpaceModification::CmnChromatic(1),
            ));
        let req = check_invariants(&s)
            .into_iter()
            .find(|v| v.kind == ViolationKind::Requirement(TUNING))
            .expect("a requirement-arm violation");
        assert!(
            !req.witness.contains("req:"),
            "independent oracle: the witness must not carry its own label"
        );
        assert_eq!(
            req.to_string(),
            format!("requirement {TUNING} violated: {}", req.witness)
        );
    }

    #[test]
    fn mixed_fixture_splits_by_arm() {
        // Pinned fixture: a dangling tempo START anchor (C2, invariant arm) and
        // an out-of-region aleatoric ORDERING event (C8, requirement arm).
        let mut s = crate::generators::valid_score_rich(409);
        let (idx, evs) = aleatoric_idx_and_events(&s);
        let ghost_event = crate::ids::EventId::new(s.identity.replica_id, 9_400_501);
        let mut edges = std::collections::BTreeMap::new();
        edges.insert(evs[0], vec![ghost_event]);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.ordering = EventOrderingDAG::try_new(edges).unwrap();
        }
        let ghost_rid = ghost_region(&s, 9_400_502);
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: anchor(ghost_rid, 0),
                end: None,
                start_tempo: tempo(60.0),
                end_tempo: None,
                shape: TempoShape::Constant,
            }],
        };

        // All three surfaces, not the aggregate alone.
        let all = check_invariants(&s);
        assert!(
            all.iter().any(
                |v| v.kind == ViolationKind::Invariant(GraphInvariant::CrossCuttingRefsResolve)
            ),
            "aggregate must carry the anchor as invariant 10; got {all:?}"
        );
        assert!(
            all.iter()
                .any(|v| v.kind == ViolationKind::Requirement(LOCALITY)),
            "aggregate must carry the rider as its requirement; got {all:?}"
        );

        let via_inv = check_invariant(&s, GraphInvariant::CrossCuttingRefsResolve);
        assert!(
            via_inv.iter().all(|v| v.witness.contains("anchor target")),
            "the invariant selector must return the anchor only; got {via_inv:?}"
        );
        let via_req = check_requirement(&s, LOCALITY);
        assert!(
            !via_req.is_empty()
                && via_req
                    .iter()
                    .all(|v| v.witness.contains("ordering references")),
            "the requirement selector must return the rider only; got {via_req:?}"
        );
    }

    #[test]
    /// Pin 3a. The `.tex` requirement block equals pin 3's pinned source, by
    /// whitespace-collapsed **equality** — not required phrases plus a
    /// forbidden-stem list. A stem inventory cannot anticipate the sentence
    /// someone will actually write, and M14e is exactly that sentence: it uses
    /// no forbidden stem and settles P13-S8 inside a requirement minted not to.
    fn tempo_segment_shape_requirement_states_its_clauses_and_stays_s8_neutral() {
        const TEX: &str = include_str!("../../../spec/core_spec.tex");
        const LABEL: &str = r"\label{req:time:tempo-segment-shape}";

        let at = TEX.find(LABEL).expect("the requirement is minted");
        let start = TEX[..at]
            .rfind(r"\begin{requirement}")
            .expect("its block opens");
        let end_tag = r"\end{requirement}";
        let end = TEX[at..].find(end_tag).expect("its block closes") + at + end_tag.len();
        let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");

        assert_eq!(
            collapse(&TEX[start..end]),
            collapse(
                r"\begin{requirement}
  \label{req:time:tempo-segment-shape}
  A tempo segment's \texttt{shape} and its \texttt{end\_tempo} \MUST{} be
  compatible. If \texttt{shape} is \texttt{Constant} and \texttt{end\_tempo}
  is present, it \MUST{} equal \texttt{start\_tempo}. If \texttt{shape} is
  \texttt{Linear}, \texttt{Exponential} or \texttt{Curve}, \texttt{end\_tempo}
  \MUST{} be present.

  This requirement states the compatibility that is enforced. It does not
  determine whether a constant segment records an \texttt{end\_tempo} at all.
\end{requirement}"
            ),
            "pin 3's block is frozen; any edit must be deliberate"
        );
    }

    /// A region-local aleatoric bound whose window is reversed (`min > max`).
    /// The bounds key is **in** the region, so this trips invariant 4 and
    /// **not** C9's locality requirement.
    fn reversed_aleatoric_bound(seed: u64) -> Score {
        let mut s = crate::generators::valid_score_rich(seed);
        let (idx, evs) = aleatoric_idx_and_events(&s);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.bounds.insert(
                evs[0],
                crate::time::EventBounds {
                    start: Some(crate::time::TimeBounds::MusicalRange {
                        min: crate::time::MusicalPosition(RationalTime::new(1, 2).unwrap()),
                        max: crate::time::MusicalPosition::origin(),
                    }),
                    end: None,
                },
            );
        }
        s
    }

    #[test]
    /// Pin 10's eleventh row: the reversed-bounds condition is **unchanged** by
    /// this rung and stays in the invariant arm as invariant 4.
    fn reversed_aleatoric_bounds_stay_invariant_four() {
        let s = reversed_aleatoric_bound(414);
        let all = check_invariants(&s);
        let pair = all
            .iter()
            .find(|v| v.witness.contains("reversed"))
            .expect("the reversed bound must be reported");
        assert_eq!(
            pair.kind,
            ViolationKind::Invariant(GraphInvariant::EventCoordinateModel),
            "reversed bounds are invariant 4, not a requirement; got {pair:?}"
        );
        let four = check_invariant(&s, GraphInvariant::EventCoordinateModel);
        assert!(
            four.iter().any(|v| v.witness.contains("reversed")),
            "invariant 4's selector must return it; got {four:?}"
        );
        // Two assertions, not three: pin 10's invariant-arm template is the
        // aggregate and the selector. A `check_requirement` negative here would
        // put this test in M3's radius, which §3 pinned without it.
    }

    #[test]
    fn invariant_selector_discriminates_its_payload() {
        // Two different invariant variants in one score: a dangling staff
        // instrument (10) and a reversed aleatoric bound (4).
        let mut s = crate::generators::valid_score_rich(413);
        let ghost = crate::ids::InstrumentId::new(s.identity.replica_id, 9_400_601);
        s.staves[0].instrument = ghost;
        let (idx, evs) = aleatoric_idx_and_events(&s);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.bounds.insert(
                evs[0],
                crate::time::EventBounds {
                    start: Some(crate::time::TimeBounds::MusicalRange {
                        min: crate::time::MusicalPosition(RationalTime::new(1, 2).unwrap()),
                        max: crate::time::MusicalPosition::origin(),
                    }),
                    end: None,
                },
            );
        }

        let ten = check_invariant(&s, GraphInvariant::CrossCuttingRefsResolve);
        assert!(
            !ten.is_empty() && ten.iter().all(|v| v.witness.contains("is not declared")),
            "invariant 10's selector must return only its own; got {ten:?}"
        );
        let four = check_invariant(&s, GraphInvariant::EventCoordinateModel);
        assert!(
            !four.is_empty() && four.iter().all(|v| v.witness.contains("reversed")),
            "invariant 4's selector must return only its own; got {four:?}"
        );
    }

    #[test]
    fn requirement_selector_discriminates_its_payload() {
        // Two different requirement labels in one score: C5 (shape) and C8
        // (locality).
        let mut s = crate::generators::valid_score_rich(414);
        let (idx, evs) = aleatoric_idx_and_events(&s);
        let ghost_event = crate::ids::EventId::new(s.identity.replica_id, 9_400_701);
        let mut edges = std::collections::BTreeMap::new();
        edges.insert(evs[0], vec![ghost_event]);
        if let RegionTimeModel::Aleatoric(m) = &mut s.canvas.regions[idx].time_model {
            m.ordering = EventOrderingDAG::try_new(edges).unwrap();
        }
        let rid = s.canvas.regions[0].id;
        s.tempo_map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: anchor(rid, 0),
                end: Some(anchor(rid, 1)),
                start_tempo: tempo(60.0),
                end_tempo: None,
                shape: TempoShape::Linear,
            }],
        };

        let shape = check_requirement(&s, SHAPE);
        assert!(
            !shape.is_empty() && shape.iter().all(|v| v.witness.contains("end_tempo")),
            "the shape label must return only its own; got {shape:?}"
        );
        let locality = check_requirement(&s, LOCALITY);
        assert!(
            !locality.is_empty()
                && locality
                    .iter()
                    .all(|v| v.witness.contains("ordering references")),
            "the locality label must return only its own; got {locality:?}"
        );
    }

    #[test]
    fn violation_kind_has_exactly_two_arms() {
        let src = production_source();
        let a = src.find("pub enum ViolationKind {").expect("declaration");
        let b = src[a..].find("\n}").expect("close") + a;
        let arms: Vec<&str> = src[a..b]
            .lines()
            .filter_map(|l| l.trim().strip_suffix(','))
            .collect();
        assert_eq!(
            arms,
            vec!["Invariant(GraphInvariant)", "Requirement(&'static str)"],
            "ViolationKind has exactly two arms and no unclassified fallback"
        );
    }
}
