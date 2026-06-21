//! The score-graph structure (Chapter 5): the canvas/region model, the
//! distinct [`Staff`] and [`StaffInstance`] concepts, voices, measures,
//! barline-alignment groups, the reference-bearing cross-cutting structures,
//! and the [`Score`] root.
//!
//! The graph is a *tree of containment overlaid with cross-cutting structures
//! that hold references* (Chapter 5 §"Design Principles": "Hybrid topology").
//! The tree determines existence; cross-cutting references may dangle and
//! require re-anchoring. The spatial root is the [`Canvas`], partitioned into
//! [`Region`]s; staves live inside regions ("Canvas before staff").
//!
//! Engraving-display detail (clef/key sequences, stem direction, line styles,
//! spanner/marker visual kinds) is *fully defined in Chapter 7* per the spec
//! and belongs to Agent E; this module carries minimal placeholders for it.

use core::num::NonZeroU16;
use std::collections::BTreeSet;

use epiphany_determinism::SystemDomainTag;

use crate::event::EventArena;
use crate::ids::{
    derive_system_id, BarlineAlignmentGroupId, BeamId, ChordSymbolId, IdentityContext,
    InstrumentId, LyricLineId, MeasureId, OperationId, PartDefinitionId, PitchId, RegionId,
    RepeatStructureId, SlurId, SpannerId, StaffGroupId, StaffId, StaffInstanceId, TimeSignatureId,
    TupletId, ViewId, VoiceId,
};
use crate::pitch::{
    ForeignFormatId, PitchSpaceId, ReferencePitch, SpellingAttachment, TuningSystemId,
};
use crate::time::{MeasurePosition, MusicalDuration, TimeAnchor, WallClockDuration};

// --- Engraving-display placeholders (Chapter 7 / Agent E). ------------------

/// Default stem direction for a voice. Placeholder (Chapter 7).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum StemDirection {
    Up,
    Down,
}

/// Staff line configuration. Placeholder beyond the line count (Chapter 7).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StaffLineConfiguration {
    pub line_count: u8,
}

impl Default for StaffLineConfiguration {
    fn default() -> Self {
        StaffLineConfiguration { line_count: 5 }
    }
}

/// A clef placed at a point in a staff instance. Placeholder (Chapter 7).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ClefChange {
    pub anchor: TimeAnchor,
}

/// A key-signature change at a point in a staff instance. Placeholder
/// (Chapter 7).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeySignatureChange {
    pub anchor: TimeAnchor,
}

/// Whether and how a measure number is shown. Placeholder (Chapter 7).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum MeasureNumberVisibility {
    #[default]
    Auto,
    Always,
    Never,
}

/// A graphic object stored in a region's graphic content (Chapter 5
/// §"Graphic Objects"). Carries its identifier so [`GraphicGesture::objects`]
/// and [`crate::GraphicEvent::graphics`] references can resolve against it; the
/// geometry/style detail is Chapter 7's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GraphicObject {
    pub id: crate::ids::GraphicObjectId,
}

/// Free-graphic content of a region (Chapter 5 §"Graphic Content"): the graphic
/// objects placed in the region's coordinate space. The coordinate-system and
/// per-object geometry/style detail is Chapter 7's; this baseline carries the
/// object identities so references into the content can be resolved.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GraphicContent {
    pub objects: Vec<GraphicObject>,
}

/// Reference to a tempo map (score-level or local). Placeholder (Chapter 3
/// §"Tempo and the Tempo Map").
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TempoMapReference;

// --- Time models (Chapter 3 §"Region Time Models"). -------------------------

/// The anchoring discipline of an aleatoric region (Chapter 3 §"Aleatoric
/// Time"): which event coordinate kinds the region permits.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AleatoricAnchoringDiscipline {
    /// Musical-time coordinates only.
    Musical,
    /// Wall-clock coordinates only.
    WallClock,
    /// Either kind per event, but an event's position and duration kinds must
    /// agree and duration bounds use a single concrete variant.
    EitherPerEvent,
    /// Either kind freely; duration bounds may mix kinds.
    FreelyMixed,
}

/// A meter change within a metric grid (Chapter 5 §"Staff-Based Content").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MeterChange {
    /// Where this meter takes effect, within the enclosing staff or region.
    pub anchor: TimeAnchor,
    /// The active time signature until the next change.
    pub time_signature: TimeSignatureId,
}

/// A strictly-positive power-of-two time-signature denominator (Chapter 3
/// §"Time Signatures and Meter"): the note value `1` (whole), `2` (half), `4`
/// (quarter), …. The spec types a `Standard`/`Compound` denominator as
/// `PowerOfTwo`, so a zero or non-power-of-two value is unrepresentable by
/// construction — an irrational meter (`4/6`, `5/12`) uses the dedicated
/// [`TimeSignatureDisplay::Irrational`] variant instead.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PowerOfTwo(u16);

impl PowerOfTwo {
    /// Builds a power-of-two denominator, returning `None` unless `value` is a
    /// strictly-positive power of two.
    #[inline]
    pub const fn new(value: u16) -> Option<Self> {
        if value != 0 && value.is_power_of_two() {
            Some(PowerOfTwo(value))
        } else {
            None
        }
    }

    /// The denominator value (always a positive power of two).
    #[inline]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// How a time signature is displayed (Chapter 3 §"Time Signatures and Meter").
/// The denominator *type* per variant carries the spec's display constraint: a
/// `Standard`/`Compound` denominator is a [`PowerOfTwo`]; an `Irrational` or
/// `MixedDenominators` denominator is any [`NonZeroU16`]. Zero denominators are
/// unrepresentable. None of this affects the rational `measure_duration`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TimeSignatureDisplay {
    Standard {
        numerator: u16,
        denominator: PowerOfTwo,
    },
    Compound {
        numerators: Vec<u16>,
        denominator: PowerOfTwo,
    },
    /// Irrational meter: the denominator is *not* a power of two (`4/6`, `5/12`).
    Irrational {
        numerator: u16,
        denominator: NonZeroU16,
    },
    MixedDenominators {
        components: Vec<(u16, NonZeroU16)>,
    },
    None,
    /// Custom symbol (cut time, common time, grammar-specific). Placeholder id.
    Symbolic(u32),
}

/// A beat group within a measure (Chapter 3 §"Time Signatures and Meter").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BeatGroup {
    pub duration: MusicalDuration,
    pub subdivision: Option<MusicalDuration>,
    /// Accent strength relative to other beat groups; higher is stronger.
    pub accent: u8,
}

/// A time signature object in the score graph (Chapter 3 §"Time Signatures and
/// Meter"): not merely a numerator/denominator pair. The beat-group durations
/// must sum to `measure_duration` — enforced by [`TimeSignature::new`], the
/// spec's "reject at construction" discipline ("Implementations MUST reject time
/// signatures whose beat groups do not sum to the measure duration").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TimeSignature {
    pub id: TimeSignatureId,
    pub display: TimeSignatureDisplay,
    measure_duration: MusicalDuration,
    beat_groups: Vec<BeatGroup>,
}

impl TimeSignature {
    /// Builds a time signature, returning `None` unless the beat-group durations
    /// sum exactly to `measure_duration` (Chapter 3). Fields are private so the
    /// invariant cannot be broken after construction.
    pub fn new(
        id: TimeSignatureId,
        display: TimeSignatureDisplay,
        measure_duration: MusicalDuration,
        beat_groups: Vec<BeatGroup>,
    ) -> Option<Self> {
        let sum = MusicalDuration::sum(beat_groups.iter().map(|b| &b.duration));
        if sum == measure_duration {
            Some(TimeSignature {
                id,
                display,
                measure_duration,
                beat_groups,
            })
        } else {
            None
        }
    }

    /// The total duration of one measure under this signature.
    pub fn measure_duration(&self) -> &MusicalDuration {
        &self.measure_duration
    }

    /// The beat groups (whose durations sum to [`Self::measure_duration`]).
    pub fn beat_groups(&self) -> &[BeatGroup] {
        &self.beat_groups
    }
}

/// A per-staff (or per-region-default) metric organization (Chapter 5).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MetricGrid {
    pub meter_sequence: Vec<MeterChange>,
}

/// The metric time model of a region (Chapter 3 §"Metric Time").
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MetricTimeModel {
    pub meters: Vec<MeterChange>,
    pub tempo: TempoMapReference,
}

/// The proportional time model of a region (Chapter 3 §"Proportional Time").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProportionalTimeModel {
    /// Total wall-clock duration of the region.
    pub duration: WallClockDuration,
}

/// An acyclic ordering of events in an aleatoric region (Chapter 3 §"Aleatoric
/// Time"): an edge `a -> b` means "a precedes b"; two events with no path
/// between them are unordered. The DAG **must be acyclic** (Chapter 3:
/// "Cycles MUST be rejected at construction"), so the only constructors —
/// [`EventOrderingDAG::default`] (empty) and [`EventOrderingDAG::try_new`]
/// (cycle-checked) — cannot produce a cyclic graph, and the adjacency is
/// private so it stays that way.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct EventOrderingDAG {
    /// `edges[a]` lists the events that directly follow `a`.
    edges: std::collections::BTreeMap<crate::ids::EventId, Vec<crate::ids::EventId>>,
}

impl EventOrderingDAG {
    /// Builds a DAG from adjacency, returning `None` if it contains a cycle
    /// (Chapter 3: cycles are rejected at construction).
    pub fn try_new(
        edges: std::collections::BTreeMap<crate::ids::EventId, Vec<crate::ids::EventId>>,
    ) -> Option<Self> {
        let dag = EventOrderingDAG { edges };
        if dag.is_acyclic() {
            Some(dag)
        } else {
            None
        }
    }

    /// The events that directly follow `event`.
    pub fn successors(&self, event: crate::ids::EventId) -> &[crate::ids::EventId] {
        self.edges.get(&event).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Every event the ordering names — DAG nodes (sources) and edge targets.
    /// Used by the invariant checker to confirm the ordering only references
    /// events that exist in the region (invariant 10).
    pub fn referenced_events(&self) -> BTreeSet<crate::ids::EventId> {
        let mut set = BTreeSet::new();
        for (source, targets) in &self.edges {
            set.insert(*source);
            set.extend(targets.iter().copied());
        }
        set
    }

    /// Whether the ordering is acyclic (always true for a value built through
    /// the public constructors).
    pub fn is_acyclic(&self) -> bool {
        // Iterative DFS three-colour cycle detection over the adjacency.
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Open,
            Done,
        }
        let mut state: std::collections::BTreeMap<crate::ids::EventId, Mark> =
            std::collections::BTreeMap::new();
        // Stack entries: (node, expanded?) — expanded marks the post-visit.
        let nodes: Vec<crate::ids::EventId> = self.edges.keys().copied().collect();
        for root in nodes {
            if state.contains_key(&root) {
                continue;
            }
            let mut stack = vec![(root, false)];
            while let Some((node, expanded)) = stack.pop() {
                if expanded {
                    state.insert(node, Mark::Done);
                    continue;
                }
                if let Some(Mark::Done) = state.get(&node) {
                    continue;
                }
                state.insert(node, Mark::Open);
                stack.push((node, true));
                for &next in self.successors(node) {
                    match state.get(&next) {
                        Some(Mark::Open) => return false, // back-edge: cycle
                        Some(Mark::Done) => {}
                        None => stack.push((next, false)),
                    }
                }
            }
        }
        true
    }
}

/// The aleatoric time model of a region (Chapter 3 §"Aleatoric Time").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AleatoricTimeModel {
    /// Ordering constraints among events (acyclic by construction).
    pub ordering: EventOrderingDAG,
    /// Which event coordinate kinds this region permits.
    pub anchoring: AleatoricAnchoringDiscipline,
    /// Optional per-event interval bounds.
    pub bounds: std::collections::BTreeMap<crate::ids::EventId, crate::time::EventBounds>,
    /// Approximate or maximum total duration, for layout.
    pub duration_hint: WallClockDuration,
}

/// A region's time model (Chapter 3 §"Region Time Models").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RegionTimeModel {
    Metric(MetricTimeModel),
    Proportional(ProportionalTimeModel),
    Aleatoric(AleatoricTimeModel),
}

/// How a region constrains event coordinate kinds, derived from its time model
/// (Chapter 5 invariant 4).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CoordinateDiscipline {
    /// Musical coordinates only (metric regions).
    Musical,
    /// Wall-clock coordinates only (proportional regions).
    WallClock,
    /// Governed by the region's aleatoric anchoring discipline.
    Aleatoric(AleatoricAnchoringDiscipline),
}

impl RegionTimeModel {
    /// The coordinate discipline this time model imposes on its events.
    pub fn coordinate_discipline(&self) -> CoordinateDiscipline {
        match self {
            RegionTimeModel::Metric(_) => CoordinateDiscipline::Musical,
            RegionTimeModel::Proportional(_) => CoordinateDiscipline::WallClock,
            RegionTimeModel::Aleatoric(a) => CoordinateDiscipline::Aleatoric(a.anchoring),
        }
    }
}

// --- Staves, instances, voices, measures (Chapter 5). -----------------------

/// The provenance of a voice (Chapter 5 §"Voices"). A voice's `origin` must be
/// consistent with how it was created (invariant 18).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VoiceOrigin {
    /// Created by an explicit user action.
    UserDeclared,
    /// Imported from a foreign format.
    Imported { format: ForeignFormatId },
    /// System-promoted to resolve a concurrent-edit collision (Chapter 5
    /// §"System-Promoted Voices").
    SystemPromoted {
        /// The lower-id concurrent operation that retained the original voice.
        winning_operation: OperationId,
        /// The greater-id concurrent operation moved into this voice.
        losing_operation: OperationId,
        original_voice: VoiceId,
    },
}

/// Derives the deterministic [`VoiceId`] of a system-promoted voice from the
/// fixed function of (staff instance, original voice, winning op, losing op)
/// in Chapter 5 §"System-Promoted Voices", placed in the
/// [`crate::ReplicaId::SYSTEM_DERIVED`] namespace via the `MUSCSVCE` domain
/// tag.
///
/// The spec defers "the exact derivation function" to the semantic-operations
/// companion (Agent C); this is the prototype's concrete, deterministic
/// realization — the canonical inputs are the four identifiers' 16-byte forms
/// concatenated in the listed order. Recorded as a Pass 11 candidate in
/// `DECISIONS.md` so the companion can pin it.
pub fn derive_promoted_voice_id(
    staff_instance: StaffInstanceId,
    original_voice: VoiceId,
    winning_op: OperationId,
    losing_op: OperationId,
) -> VoiceId {
    let mut inputs = Vec::with_capacity(64);
    inputs.extend_from_slice(&staff_instance.canonical_bytes());
    inputs.extend_from_slice(&original_voice.canonical_bytes());
    inputs.extend_from_slice(&winning_op.canonical_bytes());
    inputs.extend_from_slice(&losing_op.canonical_bytes());
    derive_system_id::<VoiceId>(SystemDomainTag::VOICE, &inputs)
}

/// A polyphonic line within a staff instance (Chapter 5 §"Voices"). Holds
/// ordered references to events; the events live in the arena.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Voice {
    pub id: VoiceId,
    /// Ordered references to events in this voice, sorted by position. Each
    /// referenced event has `voice == this voice's id`.
    pub events: Vec<crate::ids::EventId>,
    pub default_stem_direction: Option<StemDirection>,
    pub is_primary: bool,
    pub origin: VoiceOrigin,
}

impl Voice {
    /// A new user-declared voice with no events.
    pub fn user(id: VoiceId) -> Self {
        Voice {
            id,
            events: Vec::new(),
            default_stem_direction: None,
            is_primary: false,
            origin: VoiceOrigin::UserDeclared,
        }
    }
}

/// A measure belonging to exactly one staff instance (Chapter 5
/// §"Staff-Based Content").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Measure {
    pub id: MeasureId,
    pub start: TimeAnchor,
    pub time_signature: Option<TimeSignatureId>,
    pub explicit_number: Option<u32>,
    pub number_visibility: MeasureNumberVisibility,
}

/// A region-local manifestation of a globally-identified [`Staff`]
/// (Chapter 5 §"Staves: Identity Versus Instance"). Distinct from `Staff`:
/// a `Staff` is the abstract identity persisting across the score; a
/// `StaffInstance` is its content for the duration of one region.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StaffInstance {
    pub id: StaffInstanceId,
    /// The globally-identified staff this instance manifests.
    pub staff: StaffId,
    pub voices: Vec<Voice>,
    pub clef_sequence: Vec<ClefChange>,
    pub key_sequence: Vec<KeySignatureChange>,
    pub local_metric_grid: Option<MetricGrid>,
    /// Measures belonging to this instance, in order (per-staff; admits
    /// polymeter).
    pub measures: Vec<Measure>,
    pub instrument_override: Option<InstrumentId>,
    pub staff_lines_override: Option<StaffLineConfiguration>,
    pub visible: bool,
}

impl StaffInstance {
    /// A new empty instance of `staff`.
    pub fn new(id: StaffInstanceId, staff: StaffId) -> Self {
        StaffInstance {
            id,
            staff,
            voices: Vec::new(),
            clef_sequence: Vec::new(),
            key_sequence: Vec::new(),
            local_metric_grid: None,
            measures: Vec::new(),
            instrument_override: None,
            staff_lines_override: None,
            visible: true,
        }
    }
}

/// A declaration that the measure boundaries of two or more staff instances
/// coincide (Chapter 5 §"Staff-Based Content").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BarlineAlignmentGroup {
    pub id: BarlineAlignmentGroupId,
    pub members: Vec<BarlineAlignmentMember>,
}

/// One member of a [`BarlineAlignmentGroup`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BarlineAlignmentMember {
    pub staff_instance: StaffInstanceId,
    pub measure: MeasureId,
    pub position: MeasurePosition,
}

/// The staff-based content of a region (Chapter 5 §"Staff-Based Content").
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct StaffBasedContent {
    pub staff_instances: Vec<StaffInstance>,
    pub default_metric_grid: Option<MetricGrid>,
    pub barline_alignment_groups: Vec<BarlineAlignmentGroup>,
    pub user_system_breaks: Vec<TimeAnchor>,
    pub user_page_breaks: Vec<TimeAnchor>,
}

/// A region's content model (Chapter 5 §"Regions").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RegionContent {
    /// Staff-based notation.
    StaffBased(StaffBasedContent),
    /// Free graphic content: no staves.
    FreeGraphic(GraphicContent),
    /// Staves with overlaid graphic content.
    Hybrid {
        staves: StaffBasedContent,
        overlay: GraphicContent,
        overlay_below_staves: bool,
    },
}

impl RegionContent {
    /// The staff instances of this content, or `&[]` for free-graphic content.
    pub fn staff_instances(&self) -> &[StaffInstance] {
        match self {
            RegionContent::StaffBased(c) => &c.staff_instances,
            RegionContent::Hybrid { staves, .. } => &staves.staff_instances,
            RegionContent::FreeGraphic(_) => &[],
        }
    }

    /// The graphic objects this content places, if any (free-graphic content,
    /// or a hybrid region's overlay). Empty for purely staff-based content.
    pub fn graphic_objects(&self) -> &[GraphicObject] {
        match self {
            RegionContent::FreeGraphic(g) => &g.objects,
            RegionContent::Hybrid { overlay, .. } => &overlay.objects,
            RegionContent::StaffBased(_) => &[],
        }
    }

    /// The staff-based content, if this region is staff-based or hybrid.
    pub fn staff_based(&self) -> Option<&StaffBasedContent> {
        match self {
            RegionContent::StaffBased(c) => Some(c),
            RegionContent::Hybrid { staves, .. } => Some(staves),
            RegionContent::FreeGraphic(_) => None,
        }
    }

    /// Mutable access to the staff instances, if this content has any (used by
    /// editing and by the invariant shrinker in [`crate::generators`]).
    pub fn staff_instances_mut(&mut self) -> Option<&mut Vec<StaffInstance>> {
        match self {
            RegionContent::StaffBased(c) => Some(&mut c.staff_instances),
            RegionContent::Hybrid { staves, .. } => Some(&mut staves.staff_instances),
            RegionContent::FreeGraphic(_) => None,
        }
    }

    /// The barline-alignment groups of this content, if staff-based.
    pub fn barline_alignment_groups(&self) -> &[BarlineAlignmentGroup] {
        match self {
            RegionContent::StaffBased(c) => &c.barline_alignment_groups,
            RegionContent::Hybrid { staves, .. } => &staves.barline_alignment_groups,
            RegionContent::FreeGraphic(_) => &[],
        }
    }
}

/// The time range a region occupies (Chapter 5 §"Regions").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TimeExtent {
    pub start: TimeAnchor,
    pub end: TimeAnchor,
}

impl TimeExtent {
    /// The extent as an absolute wall-clock interval, when both endpoints are
    /// [`TimeAnchor::WallClock`] anchors (the only extents this prototype can
    /// resolve without the full tempo/measure machinery; see
    /// [`Region::overlaps_in_time`]).
    pub fn as_wallclock(&self) -> Option<(i64, i64)> {
        match (&self.start, &self.end) {
            (TimeAnchor::WallClock { time: a }, TimeAnchor::WallClock { time: b }) => {
                Some((a.0, b.0))
            }
            _ => None,
        }
    }
}

/// The vertical staff range a region occupies (Chapter 5 §"Regions").
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct StaffExtent {
    /// The globally-identified staves whose content this region carries.
    pub staves: Vec<StaffId>,
}

/// A region of the canvas (Chapter 5 §"Regions").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Region {
    pub id: RegionId,
    pub time_model: RegionTimeModel,
    pub content: RegionContent,
    pub time_extent: TimeExtent,
    pub staff_extent: StaffExtent,
    pub local_tempo_map: Option<crate::tempo::TempoMap>,
}

impl Region {
    /// The staff instances manifested in this region.
    pub fn staff_instances(&self) -> &[StaffInstance] {
        self.content.staff_instances()
    }

    /// Whether two regions demonstrably overlap in time. Resolvable only for
    /// wall-clock extents in this prototype; symbolic (event/measure/region)
    /// anchors return `false` (cannot prove overlap without the full tempo and
    /// measure machinery — a sound but incomplete check). Half-open intervals:
    /// touching at a boundary does not overlap.
    pub fn overlaps_in_time(&self, other: &Region) -> bool {
        match (
            self.time_extent.as_wallclock(),
            other.time_extent.as_wallclock(),
        ) {
            (Some((a0, a1)), Some((b0, b1))) => a0 < b1 && b0 < a1,
            _ => false,
        }
    }

    /// Whether the staff extents of two regions intersect.
    pub fn staff_extent_intersects(&self, other: &Region) -> bool {
        let mine: BTreeSet<StaffId> = self.staff_extent.staves.iter().copied().collect();
        other.staff_extent.staves.iter().any(|s| mine.contains(s))
    }
}

/// A global, abstract staff identity persisting across the score (Chapter 5
/// §"Staves: Identity Versus Instance").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Staff {
    pub id: StaffId,
    pub name: String,
    pub abbreviation: Option<String>,
    pub instrument: InstrumentId,
    pub default_staff_lines: StaffLineConfiguration,
    pub group: Option<StaffGroupId>,
}

/// The spatial root of the score (Chapter 5 §"The Canvas").
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Canvas {
    pub regions: Vec<Region>,
}

// --- Cross-cutting structures bearing graph references (Chapter 5). ---------
//
// Modeled here are the structures whose references the invariants check
// (invariant 10) plus ties and tuplets (invariants 16, 17). The full
// cross-cutting registry (markers, repeats, analytical annotations, comments,
// graphic gestures, lyrics, chord symbols) extends this with the same
// reference-resolution discipline.

/// A slur / phrase mark over a span of events (Chapter 5 §"Slurs").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Slur {
    pub id: SlurId,
    pub start_event: crate::ids::EventId,
    pub end_event: crate::ids::EventId,
}

/// A beam over a sequence of events (Chapter 5 §"Beams").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Beam {
    pub id: BeamId,
    pub events: Vec<crate::ids::EventId>,
    pub level: u8,
}

/// A generic spanning mark anchored by time (Chapter 5 §"Spanners").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Spanner {
    pub id: SpannerId,
    pub start: TimeAnchor,
    pub end: TimeAnchor,
    /// Which staves this spanner attaches to.
    pub staves: Vec<StaffId>,
}

/// The class of a tie, fixing its validation profile (Chapter 5 §"Ties").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TieClass {
    /// Same voice, immediate adjacency in voice order, enharmonic pitches.
    Standard,
    /// Same voice, start precedes end, intervening events permitted.
    Editorial,
    /// Across voices on the same staff instance; start position <= end.
    CrossVoice,
    /// Trailing tie with no notated end (`end_event` may equal `start_event`).
    LaissezVibrer,
    /// Registered class with custom validation behaviour.
    Registered(crate::pitch::TieClassRegistryId),
}

/// A tie between two events, pairing pitches by stable [`PitchId`] (Chapter 5
/// §"Ties").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Tie {
    pub id: crate::ids::TieId,
    pub start_event: crate::ids::EventId,
    pub end_event: crate::ids::EventId,
    /// Explicit `(start_pitch, end_pitch)` pairing; `None` means pair all
    /// pitches by enharmonic matching in pitch-id-ascending order.
    pub pitch_pairing: Option<Vec<(PitchId, PitchId)>>,
    pub class: TieClass,
}

/// The actual:notated ratio of a tuplet (Chapter 3 §"Tuplets").
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TupletRatio {
    pub actual: u32,
    pub notated: u32,
}

/// A tuplet grouping object (Chapter 3 §"Tuplets as Grouping Objects").
/// Tuplets do not modify member sounding durations; the ratio is notational.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Tuplet {
    pub id: TupletId,
    pub ratio: TupletRatio,
    pub members: Vec<crate::ids::EventId>,
    pub parent: Option<TupletId>,
    /// The structurally-required total sounding duration of the members
    /// (Chapter 3 §"Tuplet Consistency"; invariant 16). For a 3:2 eighth-note
    /// triplet of three members this is `1/4`.
    pub required_total: MusicalDuration,
}

/// Where a point/range annotation attaches (Chapter 5 §"Analytical
/// Annotations"). Shared by annotations and comments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AnnotationAnchor {
    Event(crate::ids::EventId),
    Range { start: TimeAnchor, end: TimeAnchor },
    Region(RegionId),
}

/// A point marker: rehearsal mark, segno, tempo text, … (Chapter 5 §"Markers").
/// The visual `kind` is Chapter 7's; the load-bearing field is the anchor.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Marker {
    pub id: crate::ids::MarkerId,
    pub anchor: TimeAnchor,
}

/// A repeat structure: simple repeat, da capo, dal segno, volta (Chapter 5
/// §"Repeat Structures"). Spanned by two time anchors.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RepeatStructure {
    pub id: RepeatStructureId,
    pub start: TimeAnchor,
    pub end: TimeAnchor,
}

/// An analytical annotation (Roman numeral, form label, …) (Chapter 5
/// §"Analytical Annotations").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AnalyticalAnnotation {
    pub id: crate::ids::AnalyticalAnnotationId,
    pub anchor: AnnotationAnchor,
    pub layer: Option<crate::ids::AnalysisLayerId>,
}

/// A review-mode comment thread anchored to a point or range (Chapter 5
/// §"Comments").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Comment {
    pub id: crate::ids::CommentId,
    pub anchor: AnnotationAnchor,
    pub resolved: bool,
}

/// How a graphic gesture is positioned relative to score content (Chapter 5
/// §"Graphic Gestures").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GestureAnchoring {
    /// Anchored to events: the gesture moves with them.
    Events(Vec<crate::ids::EventId>),
    /// Anchored to a time and staff range.
    Range {
        start: TimeAnchor,
        end: TimeAnchor,
        staves: Vec<StaffId>,
    },
    /// Free canvas coordinates: does not follow score edits.
    Free,
}

/// A drawn gesture spanning events, staves, or regions (Chapter 5
/// §"Graphic Gestures").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GraphicGesture {
    pub id: crate::ids::GraphicGestureId,
    pub objects: Vec<crate::ids::GraphicObjectId>,
    pub anchoring: GestureAnchoring,
}

/// A lyric line attached to a sequence of events (Chapter 5
/// §"Cross-Cutting Structures"). Baseline: the event references it carries.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LyricLine {
    pub id: LyricLineId,
    pub events: Vec<crate::ids::EventId>,
}

/// A chord symbol anchored to a point in time (Chapter 5
/// §"Cross-Cutting Structures").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChordSymbol {
    pub id: ChordSymbolId,
    pub anchor: TimeAnchor,
}

/// The cross-cutting structures of a score (Chapter 5 §"Cross-Cutting
/// Structures"): musical phenomena that hold references spanning the
/// containment tree.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CrossCuttingRegistry {
    pub slurs: Vec<Slur>,
    pub ties: Vec<Tie>,
    pub beams: Vec<Beam>,
    pub tuplets: Vec<Tuplet>,
    pub spanners: Vec<Spanner>,
    pub markers: Vec<Marker>,
    pub repeats: Vec<RepeatStructure>,
    pub analytical: Vec<AnalyticalAnnotation>,
    pub comments: Vec<Comment>,
    pub graphic_gestures: Vec<GraphicGesture>,
    pub lyrics: Vec<LyricLine>,
    pub chord_symbols: Vec<ChordSymbol>,
}

// --- Notational decomposition attachments (Chapter 3; invariants 14, 15). ---

/// Provenance of a decomposition attachment (Chapter 3 §"Notational
/// Decomposition"), mirroring [`crate::SpellingSource`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DecompositionSource {
    UserChosen,
    Inferred,
    Imported { format: ForeignFormatId },
    Propagated { from: crate::ids::EventId },
}

/// A base notated note value (Chapter 3 §"Sounding Duration and Notational
/// Decomposition"). Each is half the previous; the whole note is `1/1`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    /// The undotted value as a fraction of a whole note (`Quarter` -> `1/4`).
    pub fn whole_note_fraction(self) -> MusicalDuration {
        let denom: i64 = match self {
            NoteValue::Whole => 1,
            NoteValue::Half => 2,
            NoteValue::Quarter => 4,
            NoteValue::Eighth => 8,
            NoteValue::Sixteenth => 16,
            NoteValue::ThirtySecond => 32,
            NoteValue::SixtyFourth => 64,
        };
        MusicalDuration(crate::time::RationalTime::new(1, denom).expect("nonzero denom"))
    }
}

/// One notated component of an event's rhythm (Chapter 3 §"The Notational
/// Decomposition"): a base value with augmentation dots, optional tuplet
/// membership, and a tie flag.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotatedComponent {
    pub base_value: NoteValue,
    /// Augmentation dots (each adds half the previous increment).
    pub dots: u8,
    /// Tuplet membership, if any (scales the sounding duration).
    pub tuplet: Option<TupletId>,
    /// Whether this component is tied to the next.
    pub tied_to_next: bool,
}

impl NotatedComponent {
    /// The *notated* duration (base value with dots), ignoring any tuplet
    /// scaling: the undotted value plus each augmentation dot's
    /// successively-halved increment, i.e. `base * (2 - 2^(-dots))`.
    ///
    /// Computed by exact halving rather than the closed `(2^(dots+1)-1)/2^dots`
    /// form, so *every* dot count receives its defined augmentation semantics:
    /// the count is never silently clamped, and [`crate::RationalTime`] promotes
    /// to arbitrary precision rather than overflowing the shift (Chapter 3
    /// §"Sounding Duration and Notational Decomposition").
    pub fn notated_duration(&self) -> MusicalDuration {
        let base = self.base_value.whole_note_fraction();
        let half = crate::time::RationalTime::new(1, 2).expect("nonzero");
        let mut increment = base.rational().clone();
        let mut total = base.rational().clone();
        for _ in 0..self.dots {
            increment = increment.mul(&half);
            total = total.add(&increment);
        }
        MusicalDuration(total)
    }

    /// The *sounding* duration: the notated duration scaled by a tuplet ratio
    /// (`actual:notated` ⇒ ×`notated/actual`) when the component is in one. An
    /// eighth in a 3:2 triplet sounds `1/8 × 2/3 = 1/12`.
    pub fn sounding_duration(&self, tuplet_ratio: Option<TupletRatio>) -> MusicalDuration {
        let notated = self.notated_duration();
        match tuplet_ratio {
            Some(r) if r.actual != 0 && r.notated != 0 => {
                let scale = crate::time::RationalTime::new(r.notated as i64, r.actual as i64)
                    .expect("validated nonzero");
                MusicalDuration(notated.rational().mul(&scale))
            }
            _ => notated,
        }
    }
}

/// A notational-decomposition attachment on an event (Chapter 3): the sequence
/// of notated components whose sounding durations sum to the event's sounding
/// duration (invariant 15). The base-value/dots → duration mapping is here; the
/// *choice* of decomposition (the pre-pass) is deferred (Appendix D §"Open
/// Algorithm Hooks").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DecompositionAttachment {
    pub target: crate::ids::EventId,
    pub components: Vec<NotatedComponent>,
    pub source: DecompositionSource,
}

// --- Top-level score structure (Chapter 5 §"Top-Level Score Structure"). ----
//
// These carry the Chapter 5 top-level shape. Their deep bodies (tuning
// resolution, tempo curves, part layout, view recipes) are Chapters 3/4/7 and
// later companions; this baseline models the identity- and reference-bearing
// skeleton and leaves the rest as documented placeholders.

/// Bibliographic and authorship metadata (Chapter 5 §"Score Metadata"). The
/// structure is deliberately small.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScoreMetadata {
    pub title: Option<String>,
    pub composer: Option<String>,
    pub copyright: Option<String>,
}

/// An abstract instrument definition (Chapter 5 §"Instruments"). Baseline: the
/// identity and name; sound configuration and ranges are the audio engine's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Instrument {
    pub id: InstrumentId,
    pub name: String,
}

/// The kind of a staff grouping (Chapter 5 §"Top-Level Score Structure").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StaffGroupKind {
    GrandStaff,
    Bracket,
    SubBracket,
    Choral,
    /// Custom group defined by a layout extension.
    Registered(crate::pitch::StaffGroupKindRegistryId),
}

/// A staff grouping (grand staff, bracket, choral group) (Chapter 5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StaffGroup {
    pub id: StaffGroupId,
    pub name: Option<String>,
    pub kind: StaffGroupKind,
    pub members: Vec<StaffId>,
}

/// A per-part view definition for extraction (Chapter 5 §"Parts"). Parts are
/// projections, not storage: only references and overrides.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PartDefinition {
    pub id: PartDefinitionId,
    pub name: String,
    pub staves: Vec<StaffId>,
}

/// A first-class analysis layer (Chapter 5 §"Analysis Layers and Views").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AnalysisLayer {
    pub id: crate::ids::AnalysisLayerId,
    pub name: String,
}

/// A view recipe (Chapter 5 §"Views"). Baseline: identity, name, and active
/// layers; the view-kind detail is Chapter 7's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ViewDefinition {
    pub id: ViewId,
    pub name: String,
    pub active_layers: Vec<crate::ids::AnalysisLayerId>,
}

/// The score's tuning environment (Chapter 4 §"Score Tuning Context"). Baseline:
/// the default pitch space, tuning system, and reference pitch every score must
/// declare; per-scope overrides and accidental extensions are deferred.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScoreTuningContext {
    pub default_pitch_space: PitchSpaceId,
    pub default_tuning_system: TuningSystemId,
    pub reference: ReferencePitch,
}

impl Default for ScoreTuningContext {
    fn default() -> Self {
        // The default score configuration (Chapter 4 §"Default Score
        // Configuration"): cmn-12 / tet-12 / A4 = 440 Hz.
        ScoreTuningContext {
            default_pitch_space: PitchSpaceId::new("cmn-12"),
            default_tuning_system: TuningSystemId::new("tet-12"),
            reference: ReferencePitch::a440(),
        }
    }
}

/// The root object of a score (Chapter 5 §"Top-Level Score Structure").
///
/// This carries the full Chapter 5 top-level shape. The invariant-bearing
/// structure (canvas, staves, events, cross-cutting, attachments, identity) is
/// modeled in depth; the remaining top-level fields (metadata, instruments,
/// staff groups, parts, tuning context, tempo map, analysis layers, views)
/// carry their Chapter 5 identity/reference skeleton with deeper bodies left to
/// the consuming crates (Agents C/E) and later companions.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Score {
    pub metadata: ScoreMetadata,
    pub canvas: Canvas,
    pub instruments: Vec<Instrument>,
    /// Globally-identified staves; each region's instances reference these.
    pub staves: Vec<Staff>,
    pub staff_groups: Vec<StaffGroup>,
    pub parts: Vec<PartDefinition>,
    pub cross_cutting: CrossCuttingRegistry,
    /// Time-signature objects referenced by measures and meter changes.
    pub time_signatures: Vec<TimeSignature>,
    pub tuning_context: ScoreTuningContext,
    pub tempo_map: crate::tempo::TempoMap,
    pub events: EventArena,
    pub spelling_attachments: Vec<SpellingAttachment>,
    pub decomposition_attachments: Vec<DecompositionAttachment>,
    pub spelling_precedence: crate::pitch::SpellingPrecedence,
    pub analysis_layers: Vec<AnalysisLayer>,
    pub views: Vec<ViewDefinition>,
    pub identity: IdentityContext,
    /// Pitch identifiers retained as tombstones (Chapter 6 territory; referenced
    /// by invariants 13/14). A tombstoned id is preserved, never re-matched.
    pub tombstoned_pitches: BTreeSet<PitchId>,
    /// Event identifiers retained as tombstones.
    pub tombstoned_events: BTreeSet<crate::ids::EventId>,
}

impl Score {
    /// An empty score for the given identity context, with the default tuning
    /// context (cmn-12 / tet-12 / A4 = 440).
    pub fn empty(identity: IdentityContext) -> Self {
        Score {
            metadata: ScoreMetadata::default(),
            canvas: Canvas::default(),
            instruments: Vec::new(),
            staves: Vec::new(),
            staff_groups: Vec::new(),
            parts: Vec::new(),
            cross_cutting: CrossCuttingRegistry::default(),
            time_signatures: Vec::new(),
            tuning_context: ScoreTuningContext::default(),
            tempo_map: crate::tempo::TempoMap::default(),
            events: EventArena::new(),
            spelling_attachments: Vec::new(),
            decomposition_attachments: Vec::new(),
            spelling_precedence: crate::pitch::SpellingPrecedence::default(),
            analysis_layers: Vec::new(),
            views: Vec::new(),
            identity,
            tombstoned_pitches: BTreeSet::new(),
            tombstoned_events: BTreeSet::new(),
        }
    }

    /// Iterates every staff instance in the score, paired with its region id.
    pub fn staff_instances(&self) -> impl Iterator<Item = (RegionId, &StaffInstance)> {
        self.canvas
            .regions
            .iter()
            .flat_map(|r| r.staff_instances().iter().map(move |si| (r.id, si)))
    }

    /// Iterates every voice in the score, paired with its region and staff
    /// instance.
    pub fn voices(&self) -> impl Iterator<Item = (RegionId, StaffInstanceId, &Voice)> {
        self.canvas.regions.iter().flat_map(|r| {
            r.staff_instances()
                .iter()
                .flat_map(move |si| si.voices.iter().map(move |v| (r.id, si.id, v)))
        })
    }

    /// The set of live pitch identifiers across the arena (every embedded
    /// [`crate::IdentifiedPitch`]).
    pub fn live_pitch_ids(&self) -> BTreeSet<PitchId> {
        let mut set = BTreeSet::new();
        let mut buf = Vec::new();
        for e in self.events.iter() {
            buf.clear();
            e.collect_identified_pitches(&mut buf);
            for ip in &buf {
                set.insert(ip.id);
            }
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ReplicaId;
    use crate::time::WallClockTime;

    fn wc_extent(a: i64, b: i64) -> TimeExtent {
        TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(a),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(b),
            },
        }
    }

    fn region(id: RegionId, staves: Vec<StaffId>, ext: TimeExtent) -> Region {
        Region {
            id,
            time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
            content: RegionContent::StaffBased(StaffBasedContent::default()),
            time_extent: ext,
            staff_extent: StaffExtent { staves },
            local_tempo_map: None,
        }
    }

    #[test]
    fn promoted_voice_id_is_deterministic_and_system_namespaced() {
        let r = ReplicaId(3);
        let si = StaffInstanceId::new(r, 1);
        let ov = VoiceId::new(r, 2);
        let win = OperationId::new(r, 10);
        let lose = OperationId::new(ReplicaId(4), 11);
        let a = derive_promoted_voice_id(si, ov, win, lose);
        let b = derive_promoted_voice_id(si, ov, win, lose);
        assert_eq!(a, b);
        assert_eq!(a.replica(), ReplicaId::SYSTEM_DERIVED);
        // Swapping winner and loser changes the identity (order matters).
        let c = derive_promoted_voice_id(si, ov, lose, win);
        assert_ne!(a, c);
    }

    #[test]
    fn wallclock_time_overlap_is_half_open() {
        let r = ReplicaId(1);
        let s0 = StaffId::new(r, 0);
        let a = region(RegionId::new(r, 0), vec![s0], wc_extent(0, 100));
        let b = region(RegionId::new(r, 1), vec![s0], wc_extent(100, 200));
        let c = region(RegionId::new(r, 2), vec![s0], wc_extent(50, 150));
        assert!(
            !a.overlaps_in_time(&b),
            "touching at boundary is not overlap"
        );
        assert!(a.overlaps_in_time(&c));
        assert!(a.staff_extent_intersects(&c));
    }

    #[test]
    fn coordinate_discipline_follows_time_model() {
        assert_eq!(
            RegionTimeModel::Metric(MetricTimeModel::default()).coordinate_discipline(),
            CoordinateDiscipline::Musical
        );
        assert_eq!(
            RegionTimeModel::Proportional(ProportionalTimeModel {
                duration: WallClockDuration(0)
            })
            .coordinate_discipline(),
            CoordinateDiscipline::WallClock
        );
    }

    #[test]
    fn time_signature_rejects_mismatched_beat_groups() {
        let r = ReplicaId(1);
        let id = TimeSignatureId::new(r, 1);
        let q = || MusicalDuration(crate::time::RationalTime::new(1, 4).unwrap());
        let bg = |d| BeatGroup {
            duration: d,
            subdivision: None,
            accent: 0,
        };
        // 4/4: four quarter beat groups sum to a whole-note measure.
        assert!(TimeSignature::new(
            id,
            TimeSignatureDisplay::Standard {
                numerator: 4,
                denominator: PowerOfTwo::new(4).unwrap()
            },
            MusicalDuration::whole(),
            vec![bg(q()), bg(q()), bg(q()), bg(q())],
        )
        .is_some());
        // Three quarters do not sum to a whole note -> rejected.
        assert!(TimeSignature::new(
            id,
            TimeSignatureDisplay::Standard {
                numerator: 4,
                denominator: PowerOfTwo::new(4).unwrap()
            },
            MusicalDuration::whole(),
            vec![bg(q()), bg(q()), bg(q())],
        )
        .is_none());
    }

    #[test]
    fn notated_component_durations() {
        // Undotted quarter = 1/4.
        let c = NotatedComponent {
            base_value: NoteValue::Quarter,
            dots: 0,
            tuplet: None,
            tied_to_next: false,
        };
        assert_eq!(
            c.notated_duration(),
            MusicalDuration(crate::time::RationalTime::new(1, 4).unwrap())
        );
        // Dotted quarter = 3/8.
        let dotted = NotatedComponent {
            dots: 1,
            ..c.clone()
        };
        assert_eq!(
            dotted.notated_duration(),
            MusicalDuration(crate::time::RationalTime::new(3, 8).unwrap())
        );
        // An eighth in a 3:2 triplet sounds 1/8 × 2/3 = 1/12.
        let trip = NotatedComponent {
            base_value: NoteValue::Eighth,
            dots: 0,
            tuplet: None,
            tied_to_next: false,
        };
        assert_eq!(
            trip.sounding_duration(Some(TupletRatio {
                actual: 3,
                notated: 2
            })),
            MusicalDuration(crate::time::RationalTime::new(1, 12).unwrap())
        );
    }

    #[test]
    fn power_of_two_denominator_rejects_zero_and_non_powers() {
        assert!(PowerOfTwo::new(0).is_none());
        assert!(PowerOfTwo::new(3).is_none());
        assert!(PowerOfTwo::new(6).is_none());
        assert_eq!(PowerOfTwo::new(1).unwrap().get(), 1);
        assert_eq!(PowerOfTwo::new(8).unwrap().get(), 8);
    }

    #[test]
    fn dot_counts_are_not_silently_clamped() {
        let comp = |dots| NotatedComponent {
            base_value: NoteValue::Quarter,
            dots,
            tuplet: None,
            tied_to_next: false,
        };
        // Each dot count yields its defined augmentation, distinct from the next:
        // dots=8 and dots=9 differ (the old code clamped both to 8).
        assert_ne!(comp(8).notated_duration(), comp(9).notated_duration());
        assert_ne!(comp(8).notated_duration(), comp(255).notated_duration());
        // Triple-dotted quarter = 1/4 · (1 + 1/2 + 1/4 + 1/8) = 15/32.
        assert_eq!(
            comp(3).notated_duration(),
            MusicalDuration(crate::time::RationalTime::new(15, 32).unwrap())
        );
    }

    #[test]
    fn event_ordering_dag_rejects_cycles_at_construction() {
        use std::collections::BTreeMap;
        let r = ReplicaId(1);
        let a = crate::ids::EventId::new(r, 1);
        let b = crate::ids::EventId::new(r, 2);
        let c = crate::ids::EventId::new(r, 3);
        // a -> b -> c is acyclic.
        let mut acyclic = BTreeMap::new();
        acyclic.insert(a, vec![b]);
        acyclic.insert(b, vec![c]);
        assert!(EventOrderingDAG::try_new(acyclic).is_some());
        // a -> b -> a is a cycle, rejected.
        let mut cyclic = BTreeMap::new();
        cyclic.insert(a, vec![b]);
        cyclic.insert(b, vec![a]);
        assert!(EventOrderingDAG::try_new(cyclic).is_none());
        // A self-loop is a cycle.
        let mut selfloop = BTreeMap::new();
        selfloop.insert(a, vec![a]);
        assert!(EventOrderingDAG::try_new(selfloop).is_none());
        // The empty DAG is acyclic.
        assert!(EventOrderingDAG::default().is_acyclic());
    }
}
