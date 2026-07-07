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

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::prepass::{derive_annotations, DerivedAnnotations, PrePassProfile};
use epiphany_core::{
    AleatoricAnchoringDiscipline, AnchorOffset, AnnotationAnchor, CanonicalValue, Clef,
    CoordinateDiscipline, Event, EventId, EventPosition, KeySignature, MeasurePosition,
    MusicalDuration, MusicalPosition, NotatedComponent, PitchId, PitchSpelling, Region, RegionEdge,
    RegionId, RegionTimeModel, Score, StaffId, StaffPosition, TimeAnchor, TimeSignatureDisplay,
    TupletId, TupletRatio, TypedObjectId, WallClockTime,
};
use epiphany_determinism::{DomainTag, Preimage};

use crate::engraving::{
    DecisionSource, EngravingDecision, EngravingDecisionKind, EngravingOverride, OverrideKind,
};
use crate::provenance::{LayoutObjectId, Provenance};
use crate::spatial::Transform2D;
use crate::time_axis::{time_axis_of, TimeAxisModel, TimePoint};

/// The engraving content of a layout object beyond its provenance and staff —
/// the note value, spelled pitches, clef, key, or measure data the constrained
/// pass needs to choose glyphs and compute staff positions (Chapter 7 §"Engraving
/// Decisions": the decisions are recorded in the IR). Structural objects (staves,
/// voices, the per-pitch back-references, cross-cutting structures) carry
/// [`LayoutContent::Structural`]. This payload is *authoritative* for engraving;
/// the [`LayoutObject`] variant remains the structural classification.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum LayoutContent {
    /// No engraving content beyond provenance/staff.
    #[default]
    Structural,
    /// A staff instance's resolved clef and key-signature *sequences*; the
    /// constrained pass chooses the active clef/key per position and defaults to
    /// treble / C major when a sequence is empty.
    Staff(StaffContent),
    /// A note or chord: its note value and spelled pitches (one notehead each).
    Note(NoteContent),
    /// A rest: its note value and optional explicit staff position.
    Rest(RestContent),
    /// A measure: whether it ends the staff (a final barline) and the time
    /// signature in force, when this measure introduces one.
    Measure(MeasureContent),
}

/// The clef and key-signature sequences in force across a staff instance,
/// carried at resolved [`TimePoint`]s so the constrained pass can choose the
/// *active* clef/key at any position without going back to the score graph.
/// Empty sequences mean the score declares none — the constrained pass then
/// defaults to treble / C major.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StaffContent {
    pub clefs: Vec<PlacedClef>,
    pub keys: Vec<PlacedKeySignature>,
}

/// A clef change with its score anchor resolved into the layout time axis.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlacedClef {
    pub time: TimePoint,
    pub clef: Clef,
}

/// A key-signature change with its score anchor resolved into the layout time
/// axis.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlacedKeySignature {
    pub time: TimePoint,
    pub key: KeySignature,
}

/// A note or chord's notated content: its resolved start position, its placed
/// notated components (one notehead/tie segment each, at successive offsets — a
/// multi-component decomposition is *not* collapsed), and its spelled pitches.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NoteContent {
    pub position: TimePoint,
    pub components: Vec<PlacedComponent>,
    pub pitches: Vec<NotePitch>,
}

/// One notated component placed within a note or rest: its offset from the
/// owning event's start position, the component itself (base value, dots, tuplet
/// membership, tie), and the resolved tuplet ratio when it is in a tuplet (the
/// `TupletId` inside the component does not carry the ratio).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlacedComponent {
    pub offset: MusicalDuration,
    pub component: NotatedComponent,
    pub tuplet: Option<TupletRatio>,
}

/// One pitch of a note — its identity (for the notehead's provenance) and its
/// resolved spelling, or `None` when the pre-pass produced none. A `None`
/// spelling is *preserved*, not dropped, so the constrained pass surfaces a
/// missing-spelling diagnostic rather than silently losing musical content.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotePitch {
    pub pitch: PitchId,
    pub spelling: Option<PitchSpelling>,
}

/// A rest's notated content: its resolved start position, its placed notated
/// components, and any explicit vertical position.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RestContent {
    pub position: TimePoint,
    pub components: Vec<PlacedComponent>,
    pub staff_position: Option<StaffPosition>,
}

/// A measure's notated content: its resolved start position, which barline ends
/// it, and the time signature it introduces, if any.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MeasureContent {
    pub start: TimePoint,
    pub barline: BarlineKind,
    pub time_signature: Option<TimeSignatureContent>,
}

/// Which barline ends a measure. A staff manifested across several regions
/// continues at each region boundary, so only the last measure of the *last*
/// region manifesting the staff is truly [`Final`](BarlineKind::Final).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BarlineKind {
    /// A measure within the staff's run.
    Interior,
    /// The last measure of this staff instance in this region; the staff
    /// continues in a later region.
    RegionEnd,
    /// The last measure of the last region manifesting this staff (the true end).
    Final,
}

/// A time signature reduced to its displayed numerator and denominator.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TimeSignatureContent {
    pub numerator: u16,
    pub denominator: u16,
}

/// A structural layout object before spacing (Chapter 7 §"Layout Objects"). It
/// carries its [`Provenance`], the staff it belongs to (used to route it to the
/// correct vertical band), and its [`LayoutContent`] (the engraving payload the
/// constrained pass materializes into glyphs).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CompositeLayoutObject {
    pub provenance: Provenance,
    /// The staff this object belongs to, or `None` for region-level and
    /// score-level (cross-cutting / free-graphic) objects.
    pub staff: Option<StaffId>,
    /// The engraving content materialized into glyphs at the constrained stage.
    pub content: LayoutContent,
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
        let payload = CompositeLayoutObject {
            provenance,
            staff,
            content: LayoutContent::Structural,
        };
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

    /// Projects an object and attaches its engraving content in one step.
    pub fn from_projection_with_content(
        provenance: Provenance,
        staff: Option<StaffId>,
        content: LayoutContent,
    ) -> Self {
        let mut object = Self::from_projection(provenance, staff);
        object.payload_mut().content = content;
        object
    }

    pub fn provenance(&self) -> &Provenance {
        &self.payload().provenance
    }

    pub fn staff(&self) -> Option<StaffId> {
        self.payload().staff
    }

    /// The engraving content of this object (authoritative over the variant).
    pub fn content(&self) -> &LayoutContent {
        &self.payload().content
    }

    fn payload(&self) -> &CompositeLayoutObject {
        match self {
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
        }
    }

    fn payload_mut(&mut self) -> &mut CompositeLayoutObject {
        match self {
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
        }
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
    /// User engraving overrides projected from the score graph: each region's
    /// authoritative `user_system_breaks` / `user_page_breaks` lists (Chapter 5
    /// §"Staff-Based Content") become Soft, `Internal`-origin break overrides
    /// targeting the owning region, ordered by (region id, kind, anchor
    /// canonical bytes). Each carries a paired [`EngravingDecision`] with
    /// [`DecisionSource::UserOverride`] in `engraving_decisions`.
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
    // The projected break overrides, keyed by owning region for the final
    // deterministic ordering (canvas order need not be region-id order).
    let mut projected_breaks: Vec<(RegionId, EngravingOverride)> = Vec::new();
    let mut cross_region = Vec::new();
    let mut seen: BTreeSet<LayoutObjectId> = BTreeSet::new();
    // The resolved spellings and decompositions the notation engraving consumes
    // (Agent H's pre-pass): which notehead a note draws, where its pitches sit,
    // and which accidentals its spelling carries. Recomputed deterministically
    // from the score with the default profile.
    let annotations = derive_annotations(score, &PrePassProfile::default())
        .expect("the default pre-pass algorithms are supported");
    // The last region index that manifests each staff, so a measure can tell a
    // mid-staff region boundary (continuation) from the true final barline.
    let mut staff_last_region: BTreeMap<StaffId, usize> = BTreeMap::new();
    for (index, region) in score.canvas.regions.iter().enumerate() {
        for staff_id in &region.staff_extent.staves {
            staff_last_region.insert(*staff_id, index);
        }
    }

    for (region_index, region) in score.canvas.regions.iter().enumerate() {
        let region_id = region.id;
        let mut objects = Vec::new();
        let mut push = |source: TypedObjectId,
                        dependencies: Vec<TypedObjectId>,
                        staff: Option<StaffId>,
                        content: LayoutContent| {
            let provenance = Provenance::manifested(source, region_id, dependencies);
            if seen.insert(provenance.stable_id) {
                objects.push(LayoutObject::from_projection_with_content(
                    provenance, staff, content,
                ));
            }
        };

        // Staves manifested in this region (via the staff extent).
        for staff_id in &region.staff_extent.staves {
            push(
                TypedObjectId::Staff(*staff_id),
                vec![],
                Some(*staff_id),
                LayoutContent::Structural,
            );
        }

        // Staff instances, voices, and their events + pitches — all belong to
        // the instance's staff. The staff instance carries the clef/key in force.
        for si in region.staff_instances() {
            let staff = Some(si.staff);
            let si_src = TypedObjectId::StaffInstance(si.id);
            let mut si_deps = vec![TypedObjectId::Staff(si.staff)];
            si_deps.extend(
                si.clef_sequence
                    .iter()
                    .filter_map(|change| time_anchor_dep(&change.anchor)),
            );
            si_deps.extend(
                si.key_sequence
                    .iter()
                    .filter_map(|change| time_anchor_dep(&change.anchor)),
            );
            push(si_src, si_deps, staff, staff_content(score, si));
            for voice in &si.voices {
                let v_src = TypedObjectId::Voice(voice.id);
                push(v_src, vec![si_src], staff, LayoutContent::Structural);
                for eid in &voice.events {
                    let e_src = TypedObjectId::Event(*eid);
                    // The event's pitches become its invalidation dependencies.
                    let pitches = identified_pitch_ids(score, *eid);
                    let mut deps = vec![v_src];
                    deps.extend(pitches.iter().copied().map(TypedObjectId::Pitch));
                    // The event carries the notated content (note value + spelled
                    // pitches); the per-pitch objects are structural provenance.
                    push(e_src, deps, staff, event_content(score, *eid, &annotations));
                    for pid in pitches {
                        push(
                            TypedObjectId::Pitch(pid),
                            vec![e_src],
                            staff,
                            LayoutContent::Structural,
                        );
                    }
                }
            }
        }

        // Measures, per staff instance (Chapter 5 §"Measures"). The last measure
        // of an instance ends this region's run; it is the true final barline only
        // when this is the last region manifesting the staff.
        for si in region.staff_instances() {
            let last = si.measures.len().saturating_sub(1);
            let staff_ends_here = staff_last_region.get(&si.staff) == Some(&region_index);
            for (index, measure) in si.measures.iter().enumerate() {
                let barline = if index != last {
                    BarlineKind::Interior
                } else if staff_ends_here {
                    BarlineKind::Final
                } else {
                    BarlineKind::RegionEnd
                };
                // The measure depends on its staff instance, the time signature it
                // displays (so a display change with the same id invalidates the
                // measure and its synthesized time-signature glyphs), and whatever
                // its start anchor resolves through.
                let mut measure_deps = vec![TypedObjectId::StaffInstance(si.id)];
                if let Some(time_signature) = measure.time_signature {
                    measure_deps.push(TypedObjectId::TimeSignature(time_signature));
                }
                if let Some(anchor_dep) = time_anchor_dep(&measure.start) {
                    measure_deps.push(anchor_dep);
                }
                push(
                    TypedObjectId::Measure(measure.id),
                    measure_deps,
                    Some(si.staff),
                    measure_content(score, measure, barline),
                );
            }
        }

        // Free-graphic and hybrid-overlay graphic objects (Chapter 5 §"Graphic
        // Content"; Chapter 7 §"Region Uniformity"). These are region-level, not
        // staff-owned.
        for go in region.content.graphic_objects() {
            push(
                TypedObjectId::GraphicObject(go.id),
                vec![],
                None,
                LayoutContent::Structural,
            );
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
        // The region's authoritative user break lists project as engraving
        // overrides (Chapter 7 §"Engraving Overrides": a break override
        // addresses a *position* — its kind carries the break's `TimeAnchor`,
        // its `ScoreGraph` target names the owning region). Each applied
        // override records a paired decision with
        // `DecisionSource::UserOverride(id)` (Chapter 7 §"Override
        // Resolution") against the region's stable layout id.
        if let Some(content) = region.content.staff_based() {
            for anchor in &content.user_system_breaks {
                let projected =
                    EngravingOverride::projected_system_break(region_id, anchor.clone());
                engraving_decisions.push(EngravingDecision::with_source(
                    region_provenance.stable_id,
                    EngravingDecisionKind::SystemBreak,
                    DecisionSource::UserOverride(projected.id),
                ));
                projected_breaks.push((region_id, projected));
            }
            for anchor in &content.user_page_breaks {
                let projected = EngravingOverride::projected_page_break(region_id, anchor.clone());
                engraving_decisions.push(EngravingDecision::with_source(
                    region_provenance.stable_id,
                    EngravingDecisionKind::PageBreak,
                    DecisionSource::UserOverride(projected.id),
                ));
                projected_breaks.push((region_id, projected));
            }
        }
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

    // Deterministic override order: by (region id, kind discriminant, anchor
    // canonical bytes) — independent of canvas order and of the break lists'
    // internal order.
    projected_breaks.sort_by(|(region_a, a), (region_b, b)| {
        (region_a, a.kind.discriminant(), break_anchor_bytes(a)).cmp(&(
            region_b,
            b.kind.discriminant(),
            break_anchor_bytes(b),
        ))
    });

    let source = derive_score_version(score);
    LogicalLayoutIR {
        source,
        regions,
        engraving_decisions,
        overrides: projected_breaks
            .into_iter()
            .map(|(_, projected)| projected)
            .collect(),
        cross_region,
    }
}

/// The canonical bytes of a projected break override's anchor (its ordering
/// key alongside the owning region and kind).
fn break_anchor_bytes(projected: &EngravingOverride) -> Vec<u8> {
    match &projected.kind {
        OverrideKind::SystemBreak { anchor } | OverrideKind::PageBreak { anchor } => {
            anchor.canonical_bytes()
        }
        _ => Vec::new(),
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

/// The clef and key-signature sequences of a staff instance, carried with
/// resolved layout times. Empty sequences (a score that declares no clef/key)
/// are carried as-is — the constrained pass defaults the *active* clef/key to
/// treble / C major.
fn staff_content(score: &Score, si: &epiphany_core::StaffInstance) -> LayoutContent {
    LayoutContent::Staff(StaffContent {
        clefs: si
            .clef_sequence
            .iter()
            .map(|change| PlacedClef {
                time: resolve_time_anchor(score, &change.anchor),
                clef: change.clef,
            })
            .collect(),
        keys: si
            .key_sequence
            .iter()
            .map(|change| PlacedKeySignature {
                time: resolve_time_anchor(score, &change.anchor),
                key: change.key,
            })
            .collect(),
    })
}

/// The notated content of an event: a note (its position, decomposition, and
/// spelled pitches) for a pitched event, a rest for a rest, and structural for
/// the kinds this Minimal slice does not yet engrave (unpitched / indeterminate
/// / trajectory / graphic / cue). Every pitch is kept; an unspelled one carries
/// `spelling: None` rather than being dropped.
fn event_content(score: &Score, event: EventId, annotations: &DerivedAnnotations) -> LayoutContent {
    let Some(graph_event) = score.events.get(event) else {
        return LayoutContent::Structural;
    };
    let components = placed_components(score, components_of(annotations, event));
    match graph_event {
        Event::Pitched(pitched) => {
            let pitches = pitched
                .pitches
                .iter()
                .map(|identified| NotePitch {
                    pitch: identified.id,
                    spelling: annotations
                        .spellings
                        .get(&identified.id)
                        .map(|resolved| resolved.spelling.clone()),
                })
                .collect();
            LayoutContent::Note(NoteContent {
                position: event_time(&pitched.position),
                components,
                pitches,
            })
        }
        Event::Rest(rest) => LayoutContent::Rest(RestContent {
            position: event_time(&rest.position),
            components,
            staff_position: rest.vertical_position,
        }),
        _ => LayoutContent::Structural,
    }
}

/// An event's concrete position as a layout [`TimePoint`] (the two share the
/// musical/wall-clock shape).
fn event_time(position: &EventPosition) -> TimePoint {
    match position {
        EventPosition::Musical(p) => TimePoint::Musical(p.clone()),
        EventPosition::WallClock(t) => TimePoint::WallClock(*t),
    }
}

/// Places each notated component at its successive offset from the event start,
/// resolving its tuplet ratio. The offset of a component is the summed sounding
/// duration of the components before it (base value × dot factor × tuplet
/// scale), so a multi-component (e.g. tied-across-a-barline) note yields separate
/// noteheads at the right positions.
fn placed_components(score: &Score, components: Vec<NotatedComponent>) -> Vec<PlacedComponent> {
    let mut placed = Vec::with_capacity(components.len());
    let mut offset = MusicalDuration::zero();
    for component in components {
        let tuplet = component.tuplet.and_then(|id| tuplet_ratio(score, id));
        let duration = component_duration(&component, tuplet);
        placed.push(PlacedComponent {
            offset: offset.clone(),
            component,
            tuplet,
        });
        offset = offset + duration;
    }
    placed
}

/// The resolved ratio of a tuplet, looked up by id in the score's cross-cutting
/// registry.
fn tuplet_ratio(score: &Score, id: TupletId) -> Option<TupletRatio> {
    score
        .cross_cutting
        .tuplets
        .iter()
        .find(|tuplet| tuplet.id == id)
        .map(|tuplet| tuplet.ratio)
}

/// The sounding duration of a notated component. The core graph model owns the
/// exact dotted-duration semantics, including large dot counts that require
/// arbitrary precision, so layout delegates instead of duplicating the math.
fn component_duration(
    component: &NotatedComponent,
    tuplet: Option<TupletRatio>,
) -> MusicalDuration {
    component.sounding_duration(tuplet)
}

/// Resolves a [`TimeAnchor`] to a concrete layout [`TimePoint`] for placement.
/// Event anchors use the event's own region-local position plus the anchor
/// offset; measure anchors recurse through the referenced measure boundary; and
/// region anchors resolve to the referenced region edge in that region's local
/// time discipline. Cycles, missing targets, unknown metric region ends, and
/// clock-mismatched offsets fall back to the musical origin — surfaced as a
/// Minimal-slice boundary rather than panicking or inventing a false coordinate.
fn resolve_time_anchor(score: &Score, anchor: &TimeAnchor) -> TimePoint {
    const DEPTH: u8 = 16;
    resolve_time_anchor_inner(score, anchor, DEPTH).unwrap_or_else(origin_time)
}

fn resolve_time_anchor_inner(score: &Score, anchor: &TimeAnchor, depth: u8) -> Option<TimePoint> {
    if depth == 0 {
        return None;
    }
    match anchor {
        TimeAnchor::WallClock { time } => Some(TimePoint::WallClock(*time)),
        TimeAnchor::Event { id, offset } => {
            let event = score.events.get(*id)?;
            apply_offset(event_time(event.position()), offset)
        }
        TimeAnchor::Measure {
            id,
            position,
            offset,
        } => {
            let base = measure_anchor_time(score, *id, *position, depth - 1)?;
            apply_offset(base, offset)
        }
        TimeAnchor::Region { id, edge, offset } => {
            let region = score
                .canvas
                .regions
                .iter()
                .find(|region| region.id == *id)?;
            let base = region_edge_time(region, *edge, offset)?;
            apply_offset(base, offset)
        }
    }
}

/// Applies an [`AnchorOffset`] to a resolved base time; `None` when the
/// offset's clock does not match the base. Shared with the constrained stage's
/// break-anchor resolution.
pub(crate) fn apply_offset(base: TimePoint, offset: &AnchorOffset) -> Option<TimePoint> {
    match (base, offset) {
        (base, AnchorOffset::Zero) => Some(base),
        (TimePoint::Musical(position), AnchorOffset::Musical(duration)) => {
            Some(TimePoint::Musical(position + duration.clone()))
        }
        (TimePoint::WallClock(time), AnchorOffset::WallClock(duration)) => time
            .0
            .checked_add(duration.0)
            .map(WallClockTime)
            .map(TimePoint::WallClock),
        _ => None,
    }
}

fn measure_anchor_time(
    score: &Score,
    id: epiphany_core::MeasureId,
    position: MeasurePosition,
    depth: u8,
) -> Option<TimePoint> {
    for (_, instance) in score.staff_instances() {
        let Some(index) = instance
            .measures
            .iter()
            .position(|measure| measure.id == id)
        else {
            continue;
        };
        return match position {
            MeasurePosition::Start => {
                resolve_time_anchor_inner(score, &instance.measures[index].start, depth)
            }
            MeasurePosition::End => instance
                .measures
                .get(index + 1)
                .and_then(|next| resolve_time_anchor_inner(score, &next.start, depth)),
        };
    }
    None
}

fn region_edge_time(region: &Region, edge: RegionEdge, offset: &AnchorOffset) -> Option<TimePoint> {
    match edge {
        RegionEdge::Start => Some(region_origin_time(region, offset)),
        RegionEdge::End => region_end_time(region),
    }
}

fn region_origin_time(region: &Region, offset: &AnchorOffset) -> TimePoint {
    match offset {
        AnchorOffset::Musical(_) => TimePoint::Musical(MusicalPosition::origin()),
        AnchorOffset::WallClock(_) => TimePoint::WallClock(WallClockTime(0)),
        AnchorOffset::Zero => match region.time_model.coordinate_discipline() {
            CoordinateDiscipline::Musical => TimePoint::Musical(MusicalPosition::origin()),
            CoordinateDiscipline::WallClock => TimePoint::WallClock(WallClockTime(0)),
            CoordinateDiscipline::Aleatoric(AleatoricAnchoringDiscipline::WallClock) => {
                TimePoint::WallClock(WallClockTime(0))
            }
            CoordinateDiscipline::Aleatoric(_) => TimePoint::Musical(MusicalPosition::origin()),
        },
    }
}

fn region_end_time(region: &Region) -> Option<TimePoint> {
    match &region.time_model {
        RegionTimeModel::Proportional(model) => {
            Some(TimePoint::WallClock(WallClockTime(model.duration.0)))
        }
        _ => None,
    }
}

/// The musical origin as a [`TimePoint`] (the placement fallback).
fn origin_time() -> TimePoint {
    TimePoint::Musical(MusicalPosition::origin())
}

/// The full notated decomposition of an event (base values, dots, tuplets, ties)
/// from the pre-pass; empty when the event has no decomposition (non-metric or
/// ineligible) — the constrained pass surfaces that rather than inventing a value.
fn components_of(annotations: &DerivedAnnotations, event: EventId) -> Vec<NotatedComponent> {
    annotations
        .decompositions
        .get(&event)
        .map(|decomposition| decomposition.components.clone())
        .unwrap_or_default()
}

/// The notated content of a measure: its start anchor, its ending barline, and
/// the time signature it introduces, resolved to numerator/denominator when
/// standard or irrational (compound / mixed / symbolic meters are not engraved
/// in I-1).
fn measure_content(
    score: &Score,
    measure: &epiphany_core::Measure,
    barline: BarlineKind,
) -> LayoutContent {
    let time_signature = measure
        .time_signature
        .and_then(|id| time_signature_content(score, id));
    LayoutContent::Measure(MeasureContent {
        start: resolve_time_anchor(score, &measure.start),
        barline,
        time_signature,
    })
}

/// Resolves a time-signature id to its displayed numerator/denominator, for the
/// meter shapes I-1 engraves.
fn time_signature_content(
    score: &Score,
    id: epiphany_core::TimeSignatureId,
) -> Option<TimeSignatureContent> {
    let signature = score.time_signatures.iter().find(|t| t.id == id)?;
    match &signature.display {
        TimeSignatureDisplay::Standard {
            numerator,
            denominator,
        } => Some(TimeSignatureContent {
            numerator: *numerator,
            denominator: denominator.get(),
        }),
        TimeSignatureDisplay::Irrational {
            numerator,
            denominator,
        } => Some(TimeSignatureContent {
            numerator: *numerator,
            denominator: denominator.get(),
        }),
        _ => None,
    }
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
    use epiphany_core::{
        AnchorOffset, Clef, ClefChange, KeySignature, KeySignatureChange, NoteValue, RationalTime,
        RegionEdge, Spanner, SpannerId, TimeAnchor, WallClockTime,
    };

    fn duration(numerator: i64, denominator: i64) -> MusicalDuration {
        MusicalDuration(RationalTime::new(numerator, denominator).expect("nonzero"))
    }

    fn position(numerator: i64, denominator: i64) -> MusicalPosition {
        MusicalPosition(RationalTime::new(numerator, denominator).expect("nonzero"))
    }

    #[test]
    fn to_logical_enriches_notes_staves_and_measures() {
        let score = valid_score_rich(7);
        let ir = to_logical(&score);
        let objects: Vec<&LayoutObject> = ir
            .regions
            .iter()
            .flat_map(|region| region.objects.iter())
            .collect();

        // A pitched event projects a note carrying its decomposition and at least
        // one spelled pitch (and its position is recorded).
        let spelled_note = objects.iter().any(|object| {
            matches!(object.content(), LayoutContent::Note(note)
                if !note.components.is_empty()
                    && note.pitches.iter().any(|pitch| pitch.spelling.is_some()))
        });
        assert!(
            spelled_note,
            "expected an enriched note with a decomposition and a spelled pitch"
        );

        // Staff instances carry their clef/key sequences. valid_score declares no
        // clef, so the sequences are empty (the constrained pass defaults them to
        // treble / C major).
        let staves: Vec<&StaffContent> = objects
            .iter()
            .filter_map(|object| match object.content() {
                LayoutContent::Staff(staff) => Some(staff),
                _ => None,
            })
            .collect();
        assert!(!staves.is_empty(), "staff instances carry staff content");
        assert!(
            staves.iter().all(|staff| staff.clefs.is_empty()),
            "a score with no declared clef carries an empty clef sequence"
        );

        // Measures project measure content carrying a start anchor; the last
        // region manifesting each staff ends with a true Final barline (never
        // every region end).
        let measures: Vec<&MeasureContent> = objects
            .iter()
            .filter_map(|object| match object.content() {
                LayoutContent::Measure(measure) => Some(measure),
                _ => None,
            })
            .collect();
        assert!(!measures.is_empty(), "measures project measure content");
        assert!(
            measures
                .iter()
                .any(|measure| measure.barline == BarlineKind::Final),
            "the staff's last region ends with a final barline"
        );
    }

    #[test]
    fn placed_components_accumulate_successive_offsets() {
        // A note notated as a quarter tied to an eighth: the second component
        // starts a quarter-note's duration after the first (offsets are summed,
        // not collapsed).
        let score = valid_score(1);
        let components = vec![
            NotatedComponent {
                base_value: NoteValue::Quarter,
                dots: 0,
                tuplet: None,
                tied_to_next: true,
            },
            NotatedComponent {
                base_value: NoteValue::Eighth,
                dots: 0,
                tuplet: None,
                tied_to_next: false,
            },
        ];
        let placed = placed_components(&score, components);
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].offset, MusicalDuration::zero());
        assert_eq!(placed[1].offset, duration(1, 4));
    }

    #[test]
    fn placed_components_uses_core_duration_for_large_dot_counts() {
        let score = valid_score(1);
        let component = NotatedComponent {
            base_value: NoteValue::SixtyFourth,
            dots: 80,
            tuplet: None,
            tied_to_next: true,
        };
        let placed = placed_components(&score, vec![component.clone(), component.clone()]);
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[1].offset, component.sounding_duration(None));
    }

    #[test]
    fn resolve_time_anchor_applies_event_offsets() {
        let score = valid_score(1);
        let event = score
            .events
            .iter_canonical()
            .find(|event| matches!(event.position(), EventPosition::Musical(_)))
            .expect("valid_score contains musical events");
        let EventPosition::Musical(base) = event.position() else {
            unreachable!("filtered for musical events");
        };
        let offset = duration(1, 4);
        let resolved = resolve_time_anchor(
            &score,
            &TimeAnchor::Event {
                id: event.id(),
                offset: AnchorOffset::Musical(offset.clone()),
            },
        );
        assert_eq!(resolved, TimePoint::Musical(base.clone() + offset));
    }

    #[test]
    fn resolve_time_anchor_uses_referenced_region_edge() {
        let score = valid_score_rich(7);
        let (region_id, duration_ns) = score
            .canvas
            .regions
            .iter()
            .find_map(|region| match &region.time_model {
                RegionTimeModel::Proportional(model) => Some((region.id, model.duration.0)),
                _ => None,
            })
            .expect("valid_score_rich contains a proportional region");

        let resolved = resolve_time_anchor(
            &score,
            &TimeAnchor::Region {
                id: region_id,
                edge: RegionEdge::End,
                offset: AnchorOffset::Zero,
            },
        );
        assert_eq!(resolved, TimePoint::WallClock(WallClockTime(duration_ns)));
    }

    #[test]
    fn staff_content_resolves_clef_and_key_anchors() {
        let mut score = valid_score(1);
        let region_id = score.canvas.regions[0].id;
        let anchor = TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(duration(1, 4)),
        };
        let staff_instance = score.canvas.regions[0]
            .content
            .staff_instances_mut()
            .expect("valid_score is staff based")
            .first_mut()
            .expect("valid_score contains a staff instance");
        staff_instance.clef_sequence.push(ClefChange {
            anchor: anchor.clone(),
            clef: Clef::bass(),
        });
        staff_instance.key_sequence.push(KeySignatureChange {
            anchor,
            key: KeySignature::new(-3).expect("valid key signature"),
        });

        let ir = to_logical(&score);
        let staff = ir
            .regions
            .iter()
            .flat_map(|region| region.objects.iter())
            .find_map(|object| match object.content() {
                LayoutContent::Staff(staff) if !staff.clefs.is_empty() => Some(staff),
                _ => None,
            })
            .expect("staff content carries clef/key changes");
        assert_eq!(
            staff.clefs[0],
            PlacedClef {
                time: TimePoint::Musical(position(1, 4)),
                clef: Clef::bass(),
            }
        );
        assert_eq!(
            staff.keys[0],
            PlacedKeySignature {
                time: TimePoint::Musical(position(1, 4)),
                key: KeySignature::new(-3).expect("valid key signature"),
            }
        );
    }

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
    fn user_breaks_project_as_overrides_with_paired_decisions() {
        use crate::engraving::{
            DecisionSource, EngravingDecisionKind, OverrideKind, OverrideOrigin, OverridePriority,
            OverrideTarget,
        };
        let mut score = valid_score(5);
        let region_id = score.canvas.regions[0].id;
        let anchor = TimeAnchor::WallClock {
            time: WallClockTime(42),
        };
        let content = score.canvas.regions[0]
            .content
            .staff_based_mut()
            .expect("valid_score is staff based");
        content.user_system_breaks.push(anchor.clone());
        content.user_page_breaks.push(anchor.clone());

        // Deterministic across two runs.
        let ir = to_logical(&score);
        assert_eq!(ir.overrides, to_logical(&score).overrides);

        // One override per break, in the pinned projected shape: the kind
        // carries the anchor, the ScoreGraph target names the owning region,
        // Soft binding, Internal origin.
        assert_eq!(ir.overrides.len(), 2);
        for projected in &ir.overrides {
            assert_eq!(
                projected.target,
                OverrideTarget::ScoreGraph(TypedObjectId::Region(region_id))
            );
            assert_eq!(projected.priority, OverridePriority::Soft);
            assert_eq!(projected.origin, OverrideOrigin::Internal);
        }
        assert!(ir.overrides.iter().any(|o| matches!(
            &o.kind,
            OverrideKind::SystemBreak { anchor: got } if *got == anchor
        )));
        assert!(ir.overrides.iter().any(|o| matches!(
            &o.kind,
            OverrideKind::PageBreak { anchor: got } if *got == anchor
        )));
        assert_ne!(ir.overrides[0].id, ir.overrides[1].id);

        // Each applied override records a paired decision sourced to it
        // (Chapter 7 §"Override Resolution").
        for projected in &ir.overrides {
            let kind = match &projected.kind {
                OverrideKind::SystemBreak { .. } => EngravingDecisionKind::SystemBreak,
                OverrideKind::PageBreak { .. } => EngravingDecisionKind::PageBreak,
                other => panic!("unexpected projected override kind {other:?}"),
            };
            assert!(
                ir.engraving_decisions.iter().any(|decision| decision.source
                    == DecisionSource::UserOverride(projected.id)
                    && decision.kind == kind),
                "no paired UserOverride decision for {projected:?}"
            );
        }
        // The automatic per-region system-break decision is still present.
        assert!(ir
            .engraving_decisions
            .iter()
            .any(|decision| decision.source == DecisionSource::Automatic
                && decision.kind == EngravingDecisionKind::SystemBreak));
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
