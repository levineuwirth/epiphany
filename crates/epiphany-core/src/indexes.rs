//! The mandatory auxiliary indexes (Chapter 5 §"Indexes and Auxiliary
//! Structures"): *"Implementations MUST maintain at least the following
//! indexes: Event time index [mapping `(region, voice)` to a sorted view of
//! event positions, supporting O(log n) time-range queries], Cross-cutting
//! reference index [mapping each object identifier to the cross-cutting
//! structures referencing it], Measure index, Spelling attachment index."*
//!
//! [`ScoreIndexes`] builds all four from a [`Score`]. For v0 the indexes are
//! **rebuilt on demand** ([`ScoreIndexes::build`]); incremental
//! invalidation/update on edits is the editing layer's job (`epiphany-ops`,
//! Chapter 6, which specifies which operations invalidate which indexes). The
//! indexes are non-canonical caches (Appendix D §"Non-canonical cache
//! determinism") — discardable, never part of canonical state — so building
//! them from canonical state is always sound.

use std::collections::{BTreeMap, HashMap};

use crate::event::EventArena;
use crate::graph::{AnnotationAnchor, GestureAnchoring, Score};
use crate::ids::{EventId, MeasureId, PitchId, RegionId, StaffInstanceId, TypedObjectId, VoiceId};
use crate::pitch::SpellingAttachment;
use crate::time::{EventPosition, MusicalPosition, TimeAnchor, WallClockTime};

/// The four mandatory indexes over a score (Chapter 5 §"Indexes and Auxiliary
/// Structures"), rebuilt from canonical state.
#[derive(Clone, Debug, Default)]
pub struct ScoreIndexes {
    /// Event time index: `(region, voice)` → its events in ascending position
    /// order (the voice's own order, which the graph keeps position-sorted —
    /// invariant 3). Range queries binary-search this order, so they are
    /// `O(log n)`.
    event_time: HashMap<(RegionId, VoiceId), Vec<crate::ids::EventId>>,
    /// Cross-cutting reference index: each *referenced* object identifier
    /// ([`TypedObjectId`], any kind) → the cross-cutting structures (and other
    /// reference holders) that name it, for `O(1)`-amortized re-anchoring
    /// lookup. Covers every reference-bearing cross-cutting structure — slurs,
    /// ties, beams, tuplets, spanners, markers, repeats, analytical
    /// annotations, comments, graphic gestures, lyric lines, chord symbols —
    /// across all of the object kinds they reference (events, pitches,
    /// measures, regions, staves, graphic objects, analysis layers, tuplets).
    cross_refs: HashMap<TypedObjectId, Vec<TypedObjectId>>,
    /// Measure index: measure number → the measures carrying it, plus every
    /// measure's owning staff instance for fast navigation.
    measure_by_number: BTreeMap<u32, Vec<MeasureId>>,
    measure_instance: HashMap<MeasureId, StaffInstanceId>,
    /// Spelling-attachment index: pitch id → analysis layer (`None` = engraved)
    /// → indices into `score.spelling_attachments`.
    spelling: HashMap<PitchId, HashMap<Option<crate::ids::AnalysisLayerId>, Vec<usize>>>,
}

/// The [`TypedObjectId`] a [`TimeAnchor`] names, if any (a wall-clock anchor
/// names no graph object). Used to reverse-index anchor-bearing structures.
fn anchor_object(anchor: &TimeAnchor) -> Option<TypedObjectId> {
    match anchor {
        TimeAnchor::Event { id, .. } => Some(TypedObjectId::Event(*id)),
        TimeAnchor::Measure { id, .. } => Some(TypedObjectId::Measure(*id)),
        TimeAnchor::Region { id, .. } => Some(TypedObjectId::Region(*id)),
        TimeAnchor::WallClock { .. } => None,
    }
}

/// Appends the [`TypedObjectId`]s an [`AnnotationAnchor`] names.
fn annotation_anchor_objects(a: &AnnotationAnchor, out: &mut Vec<TypedObjectId>) {
    match a {
        AnnotationAnchor::Event(e) => out.push(TypedObjectId::Event(*e)),
        AnnotationAnchor::Region(r) => out.push(TypedObjectId::Region(*r)),
        AnnotationAnchor::Range { start, end } => {
            out.extend(anchor_object(start));
            out.extend(anchor_object(end));
        }
    }
}

impl ScoreIndexes {
    /// Builds all four indexes from `score`.
    pub fn build(score: &Score) -> Self {
        let mut event_time: HashMap<(RegionId, VoiceId), Vec<EventId>> = HashMap::new();
        let mut measure_by_number: BTreeMap<u32, Vec<MeasureId>> = BTreeMap::new();
        let mut measure_instance = HashMap::new();
        for region in &score.canvas.regions {
            for si in region.staff_instances() {
                for v in &si.voices {
                    event_time
                        .entry((region.id, v.id))
                        .or_default()
                        .extend(v.events.iter().copied());
                }
                for m in &si.measures {
                    measure_instance.insert(m.id, si.id);
                    if let Some(n) = m.explicit_number {
                        measure_by_number.entry(n).or_default().push(m.id);
                    }
                }
            }
        }

        // Cross-cutting reference index: every reference-bearing cross-cutting
        // structure, keyed by each object identifier it names (Chapter 5:
        // "Maps each object identifier to the cross-cutting structures
        // referencing it").
        let mut cross_refs: HashMap<TypedObjectId, Vec<TypedObjectId>> = HashMap::new();
        let cc = &score.cross_cutting;
        let mut add = |target: TypedObjectId, who: TypedObjectId| {
            cross_refs.entry(target).or_default().push(who);
        };
        let ev = TypedObjectId::Event;
        for s in &cc.slurs {
            let who = TypedObjectId::Slur(s.id);
            add(ev(s.start_event), who);
            add(ev(s.end_event), who);
        }
        for t in &cc.ties {
            let who = TypedObjectId::Tie(t.id);
            add(ev(t.start_event), who);
            add(ev(t.end_event), who);
            // Ties also name pitches (the explicit pairing).
            for (sp, ep) in t.pitch_pairing.iter().flatten() {
                add(TypedObjectId::Pitch(*sp), who);
                add(TypedObjectId::Pitch(*ep), who);
            }
        }
        for b in &cc.beams {
            let who = TypedObjectId::Beam(b.id);
            for e in &b.events {
                add(ev(*e), who);
            }
        }
        for tp in &cc.tuplets {
            let who = TypedObjectId::Tuplet(tp.id);
            for e in &tp.members {
                add(ev(*e), who);
            }
            if let Some(parent) = tp.parent {
                add(TypedObjectId::Tuplet(parent), who);
            }
        }
        for sp in &cc.spanners {
            let who = TypedObjectId::Spanner(sp.id);
            for o in [anchor_object(&sp.start), anchor_object(&sp.end)]
                .into_iter()
                .flatten()
            {
                add(o, who);
            }
            for s in &sp.staves {
                add(TypedObjectId::Staff(*s), who);
            }
        }
        for m in &cc.markers {
            if let Some(o) = anchor_object(&m.anchor) {
                add(o, TypedObjectId::Marker(m.id));
            }
        }
        for rp in &cc.repeats {
            let who = TypedObjectId::RepeatStructure(rp.id);
            for o in [anchor_object(&rp.start), anchor_object(&rp.end)]
                .into_iter()
                .flatten()
            {
                add(o, who);
            }
        }
        for an in &cc.analytical {
            let who = TypedObjectId::AnalyticalAnnotation(an.id);
            let mut targets = Vec::new();
            annotation_anchor_objects(&an.anchor, &mut targets);
            for o in targets {
                add(o, who);
            }
            if let Some(layer) = an.layer {
                add(TypedObjectId::AnalysisLayer(layer), who);
            }
        }
        for cm in &cc.comments {
            let who = TypedObjectId::Comment(cm.id);
            let mut targets = Vec::new();
            annotation_anchor_objects(&cm.anchor, &mut targets);
            for o in targets {
                add(o, who);
            }
        }
        for g in &cc.graphic_gestures {
            let who = TypedObjectId::GraphicGesture(g.id);
            for o in &g.objects {
                add(TypedObjectId::GraphicObject(*o), who);
            }
            match &g.anchoring {
                GestureAnchoring::Events(es) => {
                    for e in es {
                        add(ev(*e), who);
                    }
                }
                GestureAnchoring::Range { start, end, staves } => {
                    for o in [anchor_object(start), anchor_object(end)]
                        .into_iter()
                        .flatten()
                    {
                        add(o, who);
                    }
                    for s in staves {
                        add(TypedObjectId::Staff(*s), who);
                    }
                }
                GestureAnchoring::Free => {}
            }
        }
        for ly in &cc.lyrics {
            let who = TypedObjectId::LyricLine(ly.id);
            for e in &ly.events {
                add(ev(*e), who);
            }
        }
        for cs in &cc.chord_symbols {
            if let Some(o) = anchor_object(&cs.anchor) {
                add(o, TypedObjectId::ChordSymbol(cs.id));
            }
        }

        // Spelling-attachment index.
        let mut spelling: HashMap<PitchId, HashMap<_, Vec<usize>>> = HashMap::new();
        for (i, a) in score.spelling_attachments.iter().enumerate() {
            if let crate::pitch::SpellingScope::Pitch(pid) = &a.scope {
                spelling
                    .entry(*pid)
                    .or_default()
                    .entry(a.layer)
                    .or_default()
                    .push(i);
            }
        }

        ScoreIndexes {
            event_time,
            cross_refs,
            measure_by_number,
            measure_instance,
            spelling,
        }
    }

    /// The events of `(region, voice)` in ascending position order.
    pub fn events_for_voice(&self, region: RegionId, voice: VoiceId) -> &[EventId] {
        self.event_time
            .get(&(region, voice))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Events of `(region, voice)` whose musical position lies in `[lo, hi)`,
    /// resolved against `arena`. `O(log n)` via binary search: the voice's
    /// stored order is position-sorted (invariant 3), so the half-open range is
    /// a pair of `partition_point`s. A wall-clock voice yields an empty result
    /// (use [`ScoreIndexes::events_in_wallclock_range`]).
    pub fn events_in_musical_range(
        &self,
        region: RegionId,
        voice: VoiceId,
        lo: &MusicalPosition,
        hi: &MusicalPosition,
        arena: &EventArena,
    ) -> Vec<EventId> {
        let events = self.events_for_voice(region, voice);
        // Musical position of an event, or `None` for a wall-clock/absent one.
        let musical = |e: &EventId| match arena.get(*e).map(|ev| ev.position()) {
            Some(EventPosition::Musical(p)) => Some(p.clone()),
            _ => None,
        };
        // `partition_point` needs a monotonic predicate; on the position-sorted
        // order, "position < bound" is true for a prefix then false. A
        // wall-clock entry maps to "below every bound", collapsing the slice to
        // empty.
        let start = events.partition_point(|e| musical(e).map(|p| &p < lo).unwrap_or(true));
        let end = events.partition_point(|e| musical(e).map(|p| &p < hi).unwrap_or(true));
        events[start..end.max(start)].to_vec()
    }

    /// Events of `(region, voice)` whose wall-clock position lies in `[lo, hi)`,
    /// resolved against `arena`. `O(log n)` binary search, the wall-clock
    /// counterpart of [`ScoreIndexes::events_in_musical_range`] (Chapter 5: the
    /// event time index serves a region's native clock — proportional regions
    /// use wall-clock positions). A musical voice yields an empty result.
    pub fn events_in_wallclock_range(
        &self,
        region: RegionId,
        voice: VoiceId,
        lo: WallClockTime,
        hi: WallClockTime,
        arena: &EventArena,
    ) -> Vec<EventId> {
        let events = self.events_for_voice(region, voice);
        let wall = |e: &EventId| match arena.get(*e).map(|ev| ev.position()) {
            Some(EventPosition::WallClock(t)) => Some(*t),
            _ => None,
        };
        let start = events.partition_point(|e| wall(e).map(|t| t < lo).unwrap_or(true));
        let end = events.partition_point(|e| wall(e).map(|t| t < hi).unwrap_or(true));
        events[start..end.max(start)].to_vec()
    }

    /// The cross-cutting structures (and other reference holders) naming
    /// `target` — an object identifier of any kind (for re-anchoring lookups).
    pub fn cross_cutting_referencing(&self, target: TypedObjectId) -> &[TypedObjectId] {
        self.cross_refs
            .get(&target)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// The measures bearing measure number `n`.
    pub fn measures_with_number(&self, n: u32) -> &[MeasureId] {
        self.measure_by_number
            .get(&n)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// The staff instance that owns `measure`.
    pub fn measure_owner(&self, measure: MeasureId) -> Option<StaffInstanceId> {
        self.measure_instance.get(&measure).copied()
    }

    /// The spelling attachments for `pitch` on analysis layer `layer`
    /// (`None` = the engraved layer), resolved against `score`.
    pub fn spellings_for<'a>(
        &self,
        score: &'a Score,
        pitch: PitchId,
        layer: Option<crate::ids::AnalysisLayerId>,
    ) -> Vec<&'a SpellingAttachment> {
        self.spelling
            .get(&pitch)
            .and_then(|by_layer| by_layer.get(&layer))
            .map(|ixs| {
                ixs.iter()
                    .filter_map(|i| score.spelling_attachments.get(*i))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::valid_score_rich;

    #[test]
    fn indexes_build_and_answer_queries() {
        let s = valid_score_rich(1);
        let idx = ScoreIndexes::build(&s);

        // Event-time index: every voice's events are indexed under its region.
        for (rid, _si, v) in s.voices() {
            assert_eq!(idx.events_for_voice(rid, v.id), v.events.as_slice());
        }

        // Cross-cutting index: the tie's endpoints know they're referenced,
        // keyed by the event's TypedObjectId.
        let tie = &s.cross_cutting.ties[0];
        assert!(idx
            .cross_cutting_referencing(TypedObjectId::Event(tie.start_event))
            .iter()
            .any(|o| matches!(o, TypedObjectId::Tie(_))));
        // The spanner anchored to region A is indexed under that region, and the
        // marker too — non-event, non-slur/tie/beam/tuplet references resolve.
        let region_a = s.canvas.regions[0].id;
        let refs = idx.cross_cutting_referencing(TypedObjectId::Region(region_a));
        assert!(refs.iter().any(|o| matches!(o, TypedObjectId::Spanner(_))));
        assert!(refs.iter().any(|o| matches!(o, TypedObjectId::Marker(_))));
        assert!(refs
            .iter()
            .any(|o| matches!(o, TypedObjectId::ChordSymbol(_))));
        // A staff named by the spanner is indexed too.
        let staff_a = s.staves[0].id;
        assert!(idx
            .cross_cutting_referencing(TypedObjectId::Staff(staff_a))
            .iter()
            .any(|o| matches!(o, TypedObjectId::Spanner(_))));

        // Measure index: the rich score declares measure number 1.
        assert!(!idx.measures_with_number(1).is_empty());
        let mid = idx.measures_with_number(1)[0];
        assert!(idx.measure_owner(mid).is_some());

        // Spelling index: the tombstone-targeting attachment is indexed under
        // its pitch on the engraved (None) layer.
        if let crate::pitch::SpellingScope::Pitch(pid) = s.spelling_attachments[0].scope {
            assert_eq!(idx.spellings_for(&s, pid, None).len(), 1);
        }
    }

    #[test]
    fn musical_range_query_filters_by_position() {
        let s = valid_score_rich(2);
        let idx = ScoreIndexes::build(&s);
        // Region A's voice holds three triplet events at 0, 1/12, 2/12.
        let (rid, _si, v) = s.voices().next().unwrap();
        let lo = crate::time::MusicalPosition::origin();
        let hi = crate::time::MusicalPosition(crate::time::RationalTime::new(2, 12).unwrap());
        let hits = idx.events_in_musical_range(rid, v.id, &lo, &hi, &s.events);
        // Positions 0 and 1/12 are in [0, 2/12); 2/12 is excluded (half-open).
        assert_eq!(hits.len(), 2);
        // The full half-open range to 3/12 catches all three.
        let all = crate::time::MusicalPosition(crate::time::RationalTime::new(3, 12).unwrap());
        assert_eq!(
            idx.events_in_musical_range(rid, v.id, &lo, &all, &s.events)
                .len(),
            3
        );
    }

    #[test]
    fn wallclock_range_query_filters_by_position() {
        let s = valid_score_rich(3);
        let idx = ScoreIndexes::build(&s);
        // The proportional region (B) holds two wall-clock events at 0 and 1000.
        let (rid, _si, v) = s
            .voices()
            .find(|(rid, _, _)| {
                matches!(
                    s.canvas
                        .regions
                        .iter()
                        .find(|r| r.id == *rid)
                        .map(|r| &r.time_model),
                    Some(crate::graph::RegionTimeModel::Proportional(_))
                )
            })
            .expect("a proportional region exists");
        // [0, 1000) catches the first; [0, 2000) catches both. A musical query
        // on this wall-clock voice is empty.
        assert_eq!(
            idx.events_in_wallclock_range(
                rid,
                v.id,
                WallClockTime(0),
                WallClockTime(1000),
                &s.events
            )
            .len(),
            1
        );
        assert_eq!(
            idx.events_in_wallclock_range(
                rid,
                v.id,
                WallClockTime(0),
                WallClockTime(2000),
                &s.events
            )
            .len(),
            2
        );
        let lo = MusicalPosition::origin();
        assert!(idx
            .events_in_musical_range(rid, v.id, &lo, &lo, &s.events)
            .is_empty());
    }
}
