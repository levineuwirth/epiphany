//! The Chapter 5 graph invariants (Chapter 5 §"Graph Invariants").
//!
//! The spec enumerates a set of structural invariants every well-formed score
//! graph must satisfy. They are *property tests in CI, not runtime assertions
//! in release builds* (QUICKSTART, Agent B): this module is the checker the
//! property tests and generators (see [`crate::generators`]) drive. Each
//! enumerated invariant has exactly one check returning a typed
//! [`InvariantViolation`] witness identifying the smallest offending objects.
//!
//! **Count.** The QUICKSTART says "the 18 graph invariants enumerated in
//! Chapter 5"; the spec body actually enumerates **19** items (1–19 in
//! §"Graph Invariants"). We implement all 19 and record the discrepancy as a
//! Pass 11 candidate in `DECISIONS.md` (the spec is the contract).
//!
//! **Scope of structural decidability.** A few invariants depend on resolving
//! [`crate::TimeAnchor`]s to absolute time (region time-overlap, anchor-offset
//! agreement), which in general needs the full tempo/measure machinery that is
//! out of this crate's scope. Those checks are *sound but incomplete*: they
//! flag the cases this prototype can resolve (notably wall-clock-anchored
//! extents) and never raise a false positive. This is documented per check.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::event::Event;
use crate::graph::{
    derive_promoted_voice_id, CoordinateDiscipline, Region, RegionTimeModel, Score, TieClass,
    VoiceOrigin,
};
use crate::ids::{
    EventId, MeasureId, PitchId, RegionId, ReplicaId, StaffId, StaffInstanceId, VoiceId,
};
use crate::pitch::{SpellingDirective, SpellingScope};
use crate::time::{
    AnchorOffset, ConcreteDuration, EventDuration, EventPosition, MusicalPosition, OffsetKind,
    TimeAnchor,
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
    /// 10. Every graph reference resolves to an extant object: cross-cutting
    ///     structures (incl. anchor targets, annotation layers, tuplet parents,
    ///     graphic objects) and event-internal references (indeterminate
    ///     alternatives, trajectory event-pitches, graphic objects, cue sources).
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
}

impl GraphInvariant {
    /// The spec enumeration number (1–19).
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
        }
    }

    /// All 19 invariants in enumeration order.
    pub fn all() -> [GraphInvariant; 19] {
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
        ]
    }
}

/// A violation of a graph invariant: which invariant, and a short witness
/// naming the smallest offending objects (Chapter 5; QUICKSTART: "minimizes
/// invariant violations to a small witness for debugging").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InvariantViolation {
    pub invariant: GraphInvariant,
    pub witness: String,
}

impl InvariantViolation {
    fn new(invariant: GraphInvariant, witness: impl Into<String>) -> Self {
        InvariantViolation {
            invariant,
            witness: witness.into(),
        }
    }
}

impl core::fmt::Display for InvariantViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "invariant {} ({:?}) violated: {}",
            self.invariant.number(),
            self.invariant,
            self.witness
        )
    }
}

/// A Chapter 5 well-formedness check this crate could not *decide* for a given
/// score — e.g., a region-overlap test whose extents use symbolic
/// [`crate::TimeAnchor`]s that need the full tempo/measure machinery (out of this
/// crate's scope) to place on a common timeline.
///
/// [`check_invariants`] stays *sound* — it never raises a false positive on an
/// undecidable check — but a sound-and-silent checker would treat "couldn't
/// decide" identically to "clean". [`deferred_checks`] makes the undecided cases
/// explicit and testable instead, so a stricter conformance profile can choose
/// to reject them rather than have them pass unseen.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeferredCheck {
    /// The invariant whose decision was deferred.
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
/// This is *sound but incomplete* for the few invariants that need absolute-time
/// resolution: it never raises a false positive, and the cases it could not
/// decide are reported separately by [`deferred_checks`] rather than silently
/// passed.
pub fn check_invariants(score: &Score) -> Vec<InvariantViolation> {
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
    idx.check_unique_identifiers(&mut v);
    idx.check_pitch_id_unique(&mut v);
    idx.check_spelling_scope_resolves(&mut v);
    idx.check_decomposition_target_resolves(&mut v);
    idx.check_decomposition_sum(&mut v);
    idx.check_tuplet_sum(&mut v);
    idx.check_tie_pairing(&mut v);
    idx.check_voice_origin_consistent(&mut v);
    idx.check_barline_group_same_region(&mut v);
    v
}

/// Checks a single invariant (useful for targeted negative property tests).
pub fn check_invariant(score: &Score, which: GraphInvariant) -> Vec<InvariantViolation> {
    check_invariants(score)
        .into_iter()
        .filter(|v| v.invariant == which)
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
    fn check_event_voice_backlink(&self, out: &mut Vec<InvariantViolation>) {
        for e in self.score.events.iter() {
            let vid = e.voice();
            match self.voice.get(&vid) {
                None => out.push(InvariantViolation::new(
                    GraphInvariant::EventVoiceBacklink,
                    format!(
                        "event {:?} names voice {:?}, which is not in the graph",
                        e.id(),
                        vid
                    ),
                )),
                Some(v) if !v.events.contains(&e.id()) => out.push(InvariantViolation::new(
                    GraphInvariant::EventVoiceBacklink,
                    format!(
                        "event {:?} names voice {:?}, which does not list it",
                        e.id(),
                        vid
                    ),
                )),
                _ => {}
            }
        }
    }

    // --- 2. Voice -> event backlink. ----------------------------------------
    fn check_voice_event_backlink(&self, out: &mut Vec<InvariantViolation>) {
        for (_r, _si, v) in self.score.voices() {
            for e in &v.events {
                match self.score.events.get(*e) {
                    None => out.push(InvariantViolation::new(
                        GraphInvariant::VoiceEventBacklink,
                        format!(
                            "voice {:?} lists event {:?}, absent from the arena",
                            v.id, e
                        ),
                    )),
                    Some(ev) if ev.voice() != v.id => out.push(InvariantViolation::new(
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
    fn check_voice_events_sorted_non_overlap(&self, out: &mut Vec<InvariantViolation>) {
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
                            out.push(InvariantViolation::new(
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
    fn check_event_coordinate_model(&self, out: &mut Vec<InvariantViolation>) {
        for e in self.score.events.iter() {
            let Some((region, _)) = self.voice_parent.get(&e.voice()) else {
                continue; // unparented voice: invariant 1/5 reports it
            };
            let Some(disc) = self.region_discipline.get(region) else {
                continue;
            };
            if !coordinate_ok(e, *disc) {
                out.push(InvariantViolation::new(
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
    fn check_containment_tree(&self, out: &mut Vec<InvariantViolation>) {
        // Voice id appearing under more than one instance.
        let mut voice_seen: HashMap<VoiceId, StaffInstanceId> = HashMap::new();
        for (_r, si, v) in self.score.voices() {
            if let Some(prev) = voice_seen.insert(v.id, si) {
                if prev != si {
                    out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
    fn check_staff_instance_resolves(&self, out: &mut Vec<InvariantViolation>) {
        for r in &self.score.canvas.regions {
            let mut staff_in_region: HashSet<StaffId> = HashSet::new();
            for si in r.staff_instances() {
                if !self.declared_staves.contains(&si.staff) {
                    out.push(InvariantViolation::new(
                        GraphInvariant::StaffInstanceResolves,
                        format!(
                            "staff instance {:?} references undeclared staff {:?}",
                            si.id, si.staff
                        ),
                    ));
                }
                if !staff_in_region.insert(si.staff) {
                    out.push(InvariantViolation::new(
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
    fn check_region_extents(&self, out: &mut Vec<InvariantViolation>) {
        for r in &self.score.canvas.regions {
            // staff_extent must list exactly the manifested staves, no dups.
            let manifested: BTreeSet<StaffId> =
                r.staff_instances().iter().map(|si| si.staff).collect();
            let mut listed = BTreeSet::new();
            for s in &r.staff_extent.staves {
                if !listed.insert(*s) {
                    out.push(InvariantViolation::new(
                        GraphInvariant::RegionExtents,
                        format!("region {:?} staff_extent lists staff {:?} twice", r.id, s),
                    ));
                }
            }
            if listed != manifested {
                out.push(InvariantViolation::new(
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
                    out.push(InvariantViolation::new(
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
    fn check_measure_single_instance(&self, out: &mut Vec<InvariantViolation>) {
        for (mid, owners) in &self.measure_instances {
            let distinct: BTreeSet<_> = owners.iter().copied().collect();
            if distinct.len() > 1 {
                out.push(InvariantViolation::new(
                    GraphInvariant::MeasureSingleInstance,
                    format!("measure {:?} belongs to instances {:?}", mid, distinct),
                ));
            }
        }
    }

    // --- 9. Anchor offset variant agrees with target's time model. ----------
    fn check_anchor_offset_model(&self, out: &mut Vec<InvariantViolation>) {
        for a in self.collect_anchors() {
            if let Some(false) = self.offset_ok(a) {
                out.push(InvariantViolation::new(
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
            anchors.push(&rp.start);
            anchors.push(&rp.end);
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
    fn check_cross_cutting_refs(&self, out: &mut Vec<InvariantViolation>) {
        let live_event = |e: &EventId| self.score.events.contains(*e);
        let mut flag = |cond: bool, what: String| {
            if !cond {
                out.push(InvariantViolation::new(
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
    // structural requirements; both are surfaced here under invariant 10
    // (reference resolution + graph integrity). Segment-anchor *offset*
    // agreement is invariant 9's (the anchors are in `collect_anchors`).
    fn check_tempo_maps(&self, out: &mut Vec<InvariantViolation>) {
        use crate::tempo::TempoShape;
        let mut flag = |cond: bool, what: String| {
            if !cond {
                out.push(InvariantViolation::new(
                    GraphInvariant::CrossCuttingRefsResolve,
                    what,
                ));
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
                    self.anchor_target_exists(&seg.start),
                    format!("tempo segment start anchor target {:?} dangling", seg.start),
                );
                if let Some(end) = &seg.end {
                    flag(
                        self.anchor_target_exists(end),
                        format!("tempo segment end anchor target {end:?} dangling"),
                    );
                }
                // Missing end_tempo / shape consistency (Chapter 3).
                match seg.shape {
                    TempoShape::Constant => flag(
                        seg.end_tempo
                            .as_ref()
                            .map_or(true, |et| et == &seg.start_tempo),
                        "constant tempo segment has end_tempo != start_tempo".to_string(),
                    ),
                    TempoShape::Linear | TempoShape::Exponential | TempoShape::Curve => flag(
                        seg.end_tempo.is_some(),
                        "non-constant tempo segment is missing its end_tempo".to_string(),
                    ),
                }
                // Ordering and non-overlap, where resolvable.
                let start = seg_pos(&seg.start);
                if let (Some(ps), Some(s)) = (&prev_start, &start) {
                    flag(s >= ps, "tempo segments are out of start order".to_string());
                }
                if let (Some(pe), Some(s)) = (&prev_end, &start) {
                    flag(
                        s >= pe,
                        "tempo segments overlap in musical time".to_string(),
                    );
                }
                prev_end = seg
                    .end
                    .as_ref()
                    .and_then(&seg_pos)
                    .or_else(|| start.clone());
                prev_start = start;
            }
        }
    }

    // --- Aleatoric ordering / bounds well-formedness (Chapter 3 §"Aleatoric
    // Time"). The ordering DAG and the per-event bounds map are graph
    // references: they must name events that exist *in the region*, and each
    // bound window must be ordered (`min <= max`). Dangling references go under
    // invariant 10; a reversed window is a region-time-model defect (invariant
    // 4). (The DAG's acyclicity is enforced at construction in `graph`.)
    fn check_aleatoric_models(&self, out: &mut Vec<InvariantViolation>) {
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
                    out.push(InvariantViolation::new(
                        GraphInvariant::CrossCuttingRefsResolve,
                        format!(
                            "aleatoric region {:?} ordering references event {:?}, absent from the region",
                            r.id, e
                        ),
                    ));
                }
            }
            for (e, bounds) in &model.bounds {
                if !in_region(*e) {
                    out.push(InvariantViolation::new(
                        GraphInvariant::CrossCuttingRefsResolve,
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
                        out.push(InvariantViolation::new(
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

    // --- 11. Identifiers unique within their kind. --------------------------
    fn check_unique_identifiers(&self, out: &mut Vec<InvariantViolation>) {
        let mut regions = BTreeSet::new();
        for r in &self.score.canvas.regions {
            if !regions.insert(r.id) {
                out.push(InvariantViolation::new(
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
                    out.push(InvariantViolation::new(
                        GraphInvariant::UniqueIdentifiers,
                        format!("staff-instance id {:?} is used twice", si.id),
                    ));
                }
                for v in &si.voices {
                    if !voices.insert(v.id) {
                        out.push(InvariantViolation::new(
                            GraphInvariant::UniqueIdentifiers,
                            format!("voice id {:?} is used twice", v.id),
                        ));
                    }
                }
                for m in &si.measures {
                    if !measures.insert(m.id) {
                        out.push(InvariantViolation::new(
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
                out.push(InvariantViolation::new(
                    GraphInvariant::UniqueIdentifiers,
                    format!("staff id {:?} is used twice", s.id),
                ));
            }
        }

        // Cross-cutting structure ids, each unique within its kind.
        let cc = &self.score.cross_cutting;
        let mut dup = |used: bool, what: String| {
            if used {
                out.push(InvariantViolation::new(
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
                out.push(InvariantViolation::new(
                    GraphInvariant::UniqueIdentifiers,
                    format!("event id {:?} is both live and tombstoned", e),
                ));
            }
        }
        for p in &self.live_pitches {
            if self.score.tombstoned_pitches.contains(p) {
                out.push(InvariantViolation::new(
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
                out.push(InvariantViolation::new(
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
            out.push(InvariantViolation::new(
                GraphInvariant::UniqueIdentifiers,
                format!("arena index entry {id:?} disagrees with the stored event's id"),
            ));
        }
        for id in self.score.events.malformed_pitched_events() {
            out.push(InvariantViolation::new(
                GraphInvariant::UniqueIdentifiers,
                format!("pitched event {id:?} has no pitches (malformed; Chapter 5)"),
            ));
        }
    }

    // --- 12. Embedded PitchId uniqueness. -----------------------------------
    fn check_pitch_id_unique(&self, out: &mut Vec<InvariantViolation>) {
        let mut seen: BTreeSet<PitchId> = BTreeSet::new();
        let mut buf = Vec::new();
        for e in self.score.events.iter() {
            buf.clear();
            e.collect_identified_pitches(&mut buf);
            for ip in &buf {
                if !seen.insert(ip.id) {
                    out.push(InvariantViolation::new(
                        GraphInvariant::PitchIdUnique,
                        format!("pitch id {:?} appears more than once", ip.id),
                    ));
                }
            }
        }
    }

    // --- 13. SpellingScope::Pitch resolves to live or tombstoned. -----------
    fn check_spelling_scope_resolves(&self, out: &mut Vec<InvariantViolation>) {
        for a in &self.score.spelling_attachments {
            if let SpellingScope::Pitch(pid) = &a.scope {
                let live = self.live_pitches.contains(pid);
                let tomb = self.score.tombstoned_pitches.contains(pid);
                if !live && !tomb {
                    out.push(InvariantViolation::new(
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
                out.push(InvariantViolation::new(
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
                    out.push(InvariantViolation::new(
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
                    out.push(InvariantViolation::new(
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
    fn check_decomposition_target_resolves(&self, out: &mut Vec<InvariantViolation>) {
        for d in &self.score.decomposition_attachments {
            let live = self.score.events.contains(d.target);
            let tomb = self.score.tombstoned_events.contains(&d.target);
            if !live && !tomb {
                out.push(InvariantViolation::new(
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
    fn check_decomposition_sum(&self, out: &mut Vec<InvariantViolation>) {
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
                out.push(InvariantViolation::new(
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
    fn check_tuplet_sum(&self, out: &mut Vec<InvariantViolation>) {
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
                out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
    fn check_tie_pairing(&self, out: &mut Vec<InvariantViolation>) {
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
                            out.push(InvariantViolation::new(
                                GraphInvariant::TiePairing,
                                format!("tie {:?} pairs pitch {:?} not in start event", t.id, sp),
                            ));
                        }
                        if !end_pitches.contains(ep) {
                            out.push(InvariantViolation::new(
                                GraphInvariant::TiePairing,
                                format!("tie {:?} pairs pitch {:?} not in end event", t.id, ep),
                            ));
                        }
                        if requires_enharmonic {
                            if let (Some(a), Some(b)) = (self.pitch.get(sp), self.pitch.get(ep)) {
                                if !a.enharmonic_equivalent(b) {
                                    out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
                            None => out.push(InvariantViolation::new(
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

    fn check_tie_class_rules(&self, t: &crate::graph::Tie, out: &mut Vec<InvariantViolation>) {
        let start = self.event_voice_index.get(&t.start_event);
        let end = self.event_voice_index.get(&t.end_event);
        match t.class {
            TieClass::Standard => {
                if let (Some((sv, si)), Some((ev, ei))) = (start, end) {
                    if sv != ev || *ei != si + 1 {
                        out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
    fn check_voice_origin_consistent(&self, out: &mut Vec<InvariantViolation>) {
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
                        out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
    fn check_barline_group_same_region(&self, out: &mut Vec<InvariantViolation>) {
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
                        out.push(InvariantViolation::new(
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
                        out.push(InvariantViolation::new(
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
        derive_promoted_voice_id, MetricTimeModel, ProportionalTimeModel, Region, RegionContent,
        Spanner, StaffBasedContent, StaffExtent, StaffInstance, Tie, TieClass, TimeExtent, Voice,
        VoiceOrigin,
    };
    use crate::ids::{OperationId, ReplicaId, SpannerId, TieId};
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
        });
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
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
        s.instruments.push(Instrument {
            id: iid,
            name: "a".into(),
        });
        s.instruments.push(Instrument {
            id: iid,
            name: "b".into(),
        });
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
    //! tempo-map segment invariants (F4), metric overlap via tempo conversion
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
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));
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
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

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
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

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
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

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
        assert!(fires(&s, GraphInvariant::CrossCuttingRefsResolve));

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
        s.instruments.push(crate::graph::Instrument {
            id: instr,
            name: "y".into(),
        });
        s.staves.push(crate::graph::Staff {
            id: staff_y,
            name: "Y".into(),
            abbreviation: None,
            instrument: instr,
            default_staff_lines: Default::default(),
            group: None,
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
