//! Stage 1 — `LogicalLayoutIR` (Chapter 7 §"LogicalLayoutIR").
//!
//! The structural projection of the score graph into layout objects, with
//! engraving decisions notionally made but spatial positions unresolved. It is
//! the output of the engraving pass and the input to the spacing pass.
//!
//! v0 projects every score-graph object that participates in the round-trip into
//! a thin [`LayoutObject`] carrying its [`Provenance`]; the full composite-object
//! taxonomy of Chapter 7 §"Layout Objects" (`NoteLayout`, `ChordLayout`, …) is a
//! layered engraving concern past v0. What v0 *does* guarantee is the contract
//! that matters for incremental layout: every object carries a complete
//! provenance back-reference (its `source` plus every score-graph object whose
//! change should invalidate it, Chapter 7 §7.1's requirement), and that
//! provenance survives the whole pipeline.

use std::collections::BTreeSet;

use epiphany_core::{AnnotationAnchor, RegionId, Score, StaffId, TimeAnchor, TypedObjectId};
use epiphany_determinism::{DomainTag, Preimage};

use crate::engraving::{EngravingDecision, EngravingDecisionKind, EngravingOverride};
use crate::provenance::{LayoutObjectId, Provenance};
use crate::spatial::Transform2D;
use crate::time_axis::{time_axis_of, TimeAxisModel};

/// A structural layout object before spacing (Chapter 7 §"Layout Objects"). v0
/// carries its [`Provenance`] and the staff it belongs to (used to route it to
/// the correct vertical band); the composite glyph content is materialized at
/// the [`crate::ConstrainedLayoutIR`] stage.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CompositeLayoutObject {
    pub provenance: Provenance,
    /// The staff this object belongs to, or `None` for region-level and
    /// score-level (cross-cutting / free-graphic) objects.
    pub staff: Option<StaffId>,
}

pub type NoteLayout = CompositeLayoutObject;
pub type ChordLayout = CompositeLayoutObject;
pub type RestLayout = CompositeLayoutObject;
pub type BeamGroupLayout = CompositeLayoutObject;
pub type TupletDisplayLayout = CompositeLayoutObject;
pub type SlurLayout = CompositeLayoutObject;
pub type TieLayout = CompositeLayoutObject;
pub type SpannerLayout = CompositeLayoutObject;
pub type MarkerLayout = CompositeLayoutObject;
pub type BarLineLayout = CompositeLayoutObject;
pub type ClefLayout = CompositeLayoutObject;
pub type KeySignatureLayout = CompositeLayoutObject;
pub type TimeSignatureDisplayLayout = CompositeLayoutObject;
pub type StaffLayout = CompositeLayoutObject;
pub type TextLayout = CompositeLayoutObject;
pub type GraphicLayout = CompositeLayoutObject;
pub type MultimeasureRestLayout = CompositeLayoutObject;
pub type CueLayout = CompositeLayoutObject;
pub type TrajectoryLayout = CompositeLayoutObject;
pub type GroupLayout = CompositeLayoutObject;

/// The complete Chapter 7 logical composite-object taxonomy. The prototype
/// payload shared by each variant is provenance/staff ownership; companion
/// engraving algorithms can refine the aliased payloads without changing the
/// stage container or variant vocabulary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LayoutObject {
    Note(NoteLayout),
    Chord(ChordLayout),
    Rest(RestLayout),
    BeamGroup(BeamGroupLayout),
    TupletDisplay(TupletDisplayLayout),
    Slur(SlurLayout),
    Tie(TieLayout),
    Spanner(SpannerLayout),
    Marker(MarkerLayout),
    BarLine(BarLineLayout),
    Clef(ClefLayout),
    KeySignature(KeySignatureLayout),
    TimeSignatureDisplay(TimeSignatureDisplayLayout),
    Staff(StaffLayout),
    Text(TextLayout),
    Graphic(GraphicLayout),
    MultimeasureRest(MultimeasureRestLayout),
    Cue(CueLayout),
    Trajectory(TrajectoryLayout),
    Group(GroupLayout),
}

impl LayoutObject {
    pub fn from_projection(provenance: Provenance, staff: Option<StaffId>) -> Self {
        let payload = CompositeLayoutObject { provenance, staff };
        match payload.provenance.source {
            TypedObjectId::Event(_) | TypedObjectId::Pitch(_) => LayoutObject::Note(payload),
            TypedObjectId::Beam(_) => LayoutObject::BeamGroup(payload),
            TypedObjectId::Tuplet(_) => LayoutObject::TupletDisplay(payload),
            TypedObjectId::Slur(_) => LayoutObject::Slur(payload),
            TypedObjectId::Tie(_) => LayoutObject::Tie(payload),
            TypedObjectId::Spanner(_) => LayoutObject::Spanner(payload),
            TypedObjectId::Marker(_) | TypedObjectId::RepeatStructure(_) => {
                LayoutObject::Marker(payload)
            }
            TypedObjectId::Measure(_) => LayoutObject::BarLine(payload),
            TypedObjectId::Staff(_) => LayoutObject::Staff(payload),
            TypedObjectId::GraphicObject(_) | TypedObjectId::GraphicGesture(_) => {
                LayoutObject::Graphic(payload)
            }
            TypedObjectId::LyricLine(_)
            | TypedObjectId::ChordSymbol(_)
            | TypedObjectId::Comment(_)
            | TypedObjectId::AnalyticalAnnotation(_) => LayoutObject::Text(payload),
            _ => LayoutObject::Group(payload),
        }
    }

    pub fn provenance(&self) -> &Provenance {
        self.payload().0
    }

    pub fn staff(&self) -> Option<StaffId> {
        self.payload().1
    }

    fn payload(&self) -> (&Provenance, Option<StaffId>) {
        let payload = match self {
            LayoutObject::Note(value)
            | LayoutObject::Chord(value)
            | LayoutObject::Rest(value)
            | LayoutObject::BeamGroup(value)
            | LayoutObject::TupletDisplay(value)
            | LayoutObject::Slur(value)
            | LayoutObject::Tie(value)
            | LayoutObject::Spanner(value)
            | LayoutObject::Marker(value)
            | LayoutObject::BarLine(value)
            | LayoutObject::Clef(value)
            | LayoutObject::KeySignature(value)
            | LayoutObject::TimeSignatureDisplay(value)
            | LayoutObject::Staff(value)
            | LayoutObject::Text(value)
            | LayoutObject::Graphic(value)
            | LayoutObject::MultimeasureRest(value)
            | LayoutObject::Cue(value)
            | LayoutObject::Trajectory(value)
            | LayoutObject::Group(value) => value,
        };
        (&payload.provenance, payload.staff)
    }
}

/// Opaque identity of the score version projected into a layout pipeline.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ScoreVersion(pub [u8; 32]);

/// Region-local coordinate system and its canvas transform.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct LocalCoordinateSystem {
    pub transform: Transform2D,
}

/// The globally identified staff bands occupied by a logical region.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VerticalExtent {
    pub staves: Vec<StaffId>,
}

/// A region projected into layout space, carrying its time axis (Chapter 7
/// §"Layout Regions"). All region kinds use this one container type
/// (Chapter 7 §"Region Uniformity").
#[derive(Clone, PartialEq, Debug)]
pub struct LayoutRegion {
    pub provenance: Provenance,
    pub coordinate_system: LocalCoordinateSystem,
    pub time_axis: TimeAxisModel,
    pub vertical_extent: VerticalExtent,
    pub objects: Vec<LayoutObject>,
}

/// A spanning object whose dependencies occupy more than one score region.
/// `regions` is in score-canvas order and identifies the complete span; the
/// spacing pass places its prototype glyph at the first anchored region while
/// preserving all regions in provenance dependencies.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CrossRegionObject {
    pub provenance: Provenance,
    pub regions: Vec<RegionId>,
    pub staff: Option<StaffId>,
}

/// The logical IR: the structural projection of the score graph (Chapter 7
/// §"LogicalLayoutIR"), plus the engraving decisions made during this pass.
#[derive(Clone, PartialEq, Debug)]
pub struct LogicalLayoutIR {
    pub source: ScoreVersion,
    pub regions: Vec<LayoutRegion>,
    /// Engraving decisions made during the engraving pass (Chapter 7
    /// §"Engraving Decisions"), carried forward through the pipeline.
    pub engraving_decisions: Vec<EngravingDecision>,
    /// User engraving overrides projected from the score graph. Agent B's
    /// current graph exposes no override registry, so the projection is empty.
    pub overrides: Vec<EngravingOverride>,
    /// Objects spanning two or more layout regions.
    pub cross_region: Vec<CrossRegionObject>,
}

/// Projects a score graph into [`LogicalLayoutIR`].
///
/// Every layout object carries a [`Provenance`] whose `source` is the
/// score-graph object it represents, with dependency back-references for
/// incremental layout. One [`LayoutRegion`] per score region carries that
/// region's [`TimeAxisModel`]. The set of projected sources is exactly
/// [`crate::laid_out_object_ids`] — the two are kept in lockstep so the
/// round-trip's source-set surjection (each source recovered; manifestation
/// multiplicity carried by distinct stable ids) holds.
///
/// A score-graph object manifested within a region is laid out **per
/// manifestation**: its stable id derives from `(source, region)`
/// ([`Provenance::manifested`]), so a staff manifested in two time-disjoint
/// regions (Chapter 5 §"Region Overlap and Concurrency") yields *two* distinct
/// layout objects — both visual staves are preserved, neither is dropped. A
/// stable-id collision (the same `(source, region)` reached twice, e.g. a staff
/// listed twice in one staff extent) is de-duplicated.
pub fn to_logical(score: &Score) -> LogicalLayoutIR {
    let mut regions = Vec::new();
    let mut engraving_decisions = Vec::new();
    let mut cross_region = Vec::new();
    let mut seen: BTreeSet<LayoutObjectId> = BTreeSet::new();

    for region in &score.canvas.regions {
        let region_id = region.id;
        let mut objects = Vec::new();
        let mut push =
            |source: TypedObjectId, dependencies: Vec<TypedObjectId>, staff: Option<StaffId>| {
                let provenance = Provenance::manifested(source, region_id, dependencies);
                if seen.insert(provenance.stable_id) {
                    objects.push(LayoutObject::from_projection(provenance, staff));
                }
            };

        // Staves manifested in this region (via the staff extent).
        for staff_id in &region.staff_extent.staves {
            push(TypedObjectId::Staff(*staff_id), vec![], Some(*staff_id));
        }

        // Staff instances, voices, and their events + pitches — all belong to
        // the instance's staff.
        for si in region.staff_instances() {
            let staff = Some(si.staff);
            let si_src = TypedObjectId::StaffInstance(si.id);
            push(si_src, vec![TypedObjectId::Staff(si.staff)], staff);
            for voice in &si.voices {
                let v_src = TypedObjectId::Voice(voice.id);
                push(v_src, vec![si_src], staff);
                for eid in &voice.events {
                    let e_src = TypedObjectId::Event(*eid);
                    // The event's pitches become its invalidation dependencies.
                    let pitches = identified_pitch_ids(score, *eid);
                    let mut deps = vec![v_src];
                    deps.extend(pitches.iter().copied().map(TypedObjectId::Pitch));
                    push(e_src, deps, staff);
                    // And the pitches themselves, as their own objects.
                    for pid in pitches {
                        push(TypedObjectId::Pitch(pid), vec![e_src], staff);
                    }
                }
            }
        }

        // Measures, per staff instance (Chapter 5 §"Measures").
        for si in region.staff_instances() {
            for measure in &si.measures {
                push(
                    TypedObjectId::Measure(measure.id),
                    vec![TypedObjectId::StaffInstance(si.id)],
                    Some(si.staff),
                );
            }
        }

        // Free-graphic and hybrid-overlay graphic objects (Chapter 5 §"Graphic
        // Content"; Chapter 7 §"Region Uniformity"). These are region-level, not
        // staff-owned.
        for go in region.content.graphic_objects() {
            push(TypedObjectId::GraphicObject(go.id), vec![], None);
        }

        let r_src = TypedObjectId::Region(region.id);
        let region_provenance = Provenance::projected(
            r_src,
            region
                .staff_extent
                .staves
                .iter()
                .map(|s| TypedObjectId::Staff(*s))
                .collect(),
        );
        // Each region notionally begins a system: record that decision against
        // the region's stable layout id (Chapter 7 §"Engraving Decisions").
        engraving_decisions.push(EngravingDecision::automatic(
            region_provenance.stable_id,
            EngravingDecisionKind::SystemBreak,
        ));
        regions.push(LayoutRegion {
            provenance: region_provenance,
            coordinate_system: LocalCoordinateSystem::default(),
            time_axis: time_axis_of(region),
            vertical_extent: VerticalExtent {
                staves: region.staff_extent.staves.clone(),
            },
            objects,
        });
    }

    // Place spanning structures according to the locations of their real
    // dependencies. A single-region object joins that region and, when all
    // located dependencies agree, that staff. A multi-region object uses the
    // dedicated cross-region collection instead of being misfiled in region 0.
    for (src, deps) in cross_cutting_objects(score) {
        let provenance = Provenance::projected(src, deps.clone());
        if !seen.insert(provenance.stable_id) {
            continue;
        }
        let mut anchored_regions = Vec::new();
        let mut anchored_staves = BTreeSet::new();
        for region in &regions {
            let TypedObjectId::Region(region_id) = region.provenance.source else {
                continue;
            };
            let mut touches_region = deps.contains(&region.provenance.source);
            for object in &region.objects {
                if deps.contains(&object.provenance().source) {
                    touches_region = true;
                    if let Some(staff) = object.staff() {
                        anchored_staves.insert(staff);
                    }
                }
            }
            if touches_region {
                anchored_regions.push(region_id);
            }
        }
        let staff = if anchored_staves.len() == 1 {
            anchored_staves.iter().next().copied()
        } else {
            None
        };
        match anchored_regions.as_slice() {
            [region_id] => {
                let region = regions
                    .iter_mut()
                    .find(|region| region.provenance.source == TypedObjectId::Region(*region_id))
                    .expect("anchored region was collected from this vector");
                region
                    .objects
                    .push(LayoutObject::from_projection(provenance, staff));
            }
            [] => {
                // Wall-clock-only annotations have no graph anchor from which
                // to infer a region; retain deterministic fallback placement.
                if let Some(first) = regions.first_mut() {
                    first
                        .objects
                        .push(LayoutObject::from_projection(provenance, staff));
                }
            }
            _ => cross_region.push(CrossRegionObject {
                provenance,
                regions: anchored_regions,
                staff,
            }),
        }
    }

    let source = derive_score_version(score);
    LogicalLayoutIR {
        source,
        regions,
        engraving_decisions,
        overrides: Vec::new(),
        cross_region,
    }
}

/// Derives the [`ScoreVersion`] from the **whole score's canonical content**
/// (Agent B's whole-score codec), not merely the layout projection's object
/// identities. Any score edit — including one that changes an event's content
/// without changing any identifier (e.g. a respelling or a duration change) —
/// therefore yields a different version, which is what incremental-layout cache
/// invalidation depends on (Chapter 7 §"Incremental Layout"). The former
/// derivation keyed on layout-object `stable_id`s alone, so a pure content edit
/// left the version unchanged.
fn derive_score_version(score: &Score) -> ScoreVersion {
    let mut preimage = Preimage::new(DomainTag::CONFLICT);
    preimage.push_bytes(b"layout-score-version");
    preimage.push_bytes(&score.canonical_bytes());
    ScoreVersion(*preimage.finish().as_bytes())
}

/// The identified-pitch ids of an event, in arena order (empty if the event is
/// absent or carries no pitches).
pub(crate) fn identified_pitch_ids(
    score: &Score,
    event: epiphany_core::EventId,
) -> Vec<epiphany_core::PitchId> {
    let mut ids = Vec::new();
    if let Some(event) = score.events.get(event) {
        let mut buf = Vec::new();
        event.collect_identified_pitches(&mut buf);
        ids.extend(buf.iter().map(|p| p.id));
    }
    ids
}

/// The score-graph object a [`TimeAnchor`] depends on, if any (a wall-clock
/// anchor depends on no object). Anchors are real invalidation dependencies: if
/// the anchored event/measure/region changes, the spanning object must relayout.
fn time_anchor_dep(anchor: &TimeAnchor) -> Option<TypedObjectId> {
    match anchor {
        TimeAnchor::Event { id, .. } => Some(TypedObjectId::Event(*id)),
        TimeAnchor::Measure { id, .. } => Some(TypedObjectId::Measure(*id)),
        TimeAnchor::Region { id, .. } => Some(TypedObjectId::Region(*id)),
        TimeAnchor::WallClock { .. } => None,
    }
}

/// The score-graph objects an [`AnnotationAnchor`] depends on.
fn annotation_anchor_deps(anchor: &AnnotationAnchor) -> Vec<TypedObjectId> {
    match anchor {
        AnnotationAnchor::Event(id) => vec![TypedObjectId::Event(*id)],
        AnnotationAnchor::Range { start, end } => [start, end]
            .iter()
            .filter_map(|a| time_anchor_dep(a))
            .collect(),
        AnnotationAnchor::Region(id) => vec![TypedObjectId::Region(*id)],
    }
}

/// The score's cross-cutting objects as `(source, dependencies)` pairs, in the
/// canonical order the projection emits them. Every cross-cutting registry
/// (Chapter 5 §"Cross-Cutting Structures") is projected, and each object's
/// dependencies are its real references — member events, anchored objects, and
/// attached staves — so an edit to any of them invalidates the spanning layout
/// object (Chapter 7 §"Invalidation Rules").
pub(crate) fn cross_cutting_objects(score: &Score) -> Vec<(TypedObjectId, Vec<TypedObjectId>)> {
    let cc = &score.cross_cutting;
    let mut out: Vec<(TypedObjectId, Vec<TypedObjectId>)> = Vec::new();
    for t in &cc.ties {
        out.push((
            TypedObjectId::Tie(t.id),
            vec![
                TypedObjectId::Event(t.start_event),
                TypedObjectId::Event(t.end_event),
            ],
        ));
    }
    for s in &cc.slurs {
        out.push((
            TypedObjectId::Slur(s.id),
            vec![
                TypedObjectId::Event(s.start_event),
                TypedObjectId::Event(s.end_event),
            ],
        ));
    }
    for b in &cc.beams {
        out.push((
            TypedObjectId::Beam(b.id),
            b.events.iter().map(|e| TypedObjectId::Event(*e)).collect(),
        ));
    }
    for tu in &cc.tuplets {
        out.push((
            TypedObjectId::Tuplet(tu.id),
            tu.members
                .iter()
                .map(|e| TypedObjectId::Event(*e))
                .collect(),
        ));
    }
    for sp in &cc.spanners {
        let mut deps: Vec<TypedObjectId> = [&sp.start, &sp.end]
            .iter()
            .filter_map(|a| time_anchor_dep(a))
            .collect();
        deps.extend(sp.staves.iter().map(|s| TypedObjectId::Staff(*s)));
        out.push((TypedObjectId::Spanner(sp.id), deps));
    }
    for mk in &cc.markers {
        out.push((
            TypedObjectId::Marker(mk.id),
            time_anchor_dep(&mk.anchor).into_iter().collect(),
        ));
    }
    for rp in &cc.repeats {
        let deps = [&rp.start, &rp.end]
            .iter()
            .filter_map(|a| time_anchor_dep(a))
            .collect();
        out.push((TypedObjectId::RepeatStructure(rp.id), deps));
    }
    for an in &cc.analytical {
        let mut deps = annotation_anchor_deps(&an.anchor);
        deps.extend(an.layer.map(TypedObjectId::AnalysisLayer));
        out.push((TypedObjectId::AnalyticalAnnotation(an.id), deps));
    }
    for cm in &cc.comments {
        out.push((
            TypedObjectId::Comment(cm.id),
            annotation_anchor_deps(&cm.anchor),
        ));
    }
    for gg in &cc.graphic_gestures {
        out.push((
            TypedObjectId::GraphicGesture(gg.id),
            gg.objects
                .iter()
                .map(|o| TypedObjectId::GraphicObject(*o))
                .collect(),
        ));
    }
    for ly in &cc.lyrics {
        out.push((
            TypedObjectId::LyricLine(ly.id),
            ly.events.iter().map(|e| TypedObjectId::Event(*e)).collect(),
        ));
    }
    for ch in &cc.chord_symbols {
        out.push((
            TypedObjectId::ChordSymbol(ch.id),
            time_anchor_dep(&ch.anchor).into_iter().collect(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::{valid_score, valid_score_rich};
    use epiphany_core::{AnchorOffset, RegionEdge, Spanner, SpannerId, TimeAnchor};

    #[test]
    fn score_version_tracks_content_not_just_identifiers() {
        let score = valid_score(7);
        // Deterministic: the same score yields the same version.
        assert_eq!(to_logical(&score).source, to_logical(&score).source);
        // Distinct scores yield distinct versions.
        assert_ne!(
            to_logical(&valid_score(7)).source,
            to_logical(&valid_score(8)).source
        );
        // A pure content edit that changes NO identifier still changes the
        // version (the old identity-only derivation missed this).
        let mut edited = score.clone();
        edited.metadata.title = Some("a different title".to_owned());
        assert_ne!(
            to_logical(&score).source,
            to_logical(&edited).source,
            "a content edit with unchanged ids must change the score version"
        );
    }

    #[test]
    fn spanning_object_uses_cross_region_collection() {
        let mut score = valid_score_rich(5);
        let first = score.canvas.regions[0].id;
        let second = score.canvas.regions[1].id;
        let first_staff = score.canvas.regions[0].staff_extent.staves[0];
        let second_staff = score.canvas.regions[1].staff_extent.staves[0];
        let id: SpannerId = score.identity.mint();
        score.cross_cutting.spanners.push(Spanner {
            id,
            start: TimeAnchor::Region {
                id: first,
                edge: RegionEdge::Start,
                offset: AnchorOffset::Zero,
            },
            end: TimeAnchor::Region {
                id: second,
                edge: RegionEdge::End,
                offset: AnchorOffset::Zero,
            },
            staves: vec![first_staff, second_staff],
        });

        let logical = to_logical(&score);
        let spanning = logical
            .cross_region
            .iter()
            .find(|object| object.provenance.source == TypedObjectId::Spanner(id))
            .expect("cross-region spanner must not be assigned to region zero");
        assert_eq!(spanning.regions, vec![first, second]);
        assert_eq!(spanning.staff, None);
    }

    #[test]
    fn same_region_tie_is_attached_to_its_real_staff() {
        let score = valid_score_rich(6);
        let tie = score.cross_cutting.ties[0].id;
        let expected_staff = score.canvas.regions[0].staff_extent.staves[0];
        let logical = to_logical(&score);
        let object = logical.regions[0]
            .objects
            .iter()
            .find(|object| object.provenance().source == TypedObjectId::Tie(tie))
            .expect("tie must be in its events' region");
        assert_eq!(object.staff(), Some(expected_staff));
    }
}
