#![forbid(unsafe_code)]
//! # epiphany-core
//!
//! The Epiphany score graph: the in-memory representation of all musical
//! content in a score, implementing the normative requirements of
//! **Chapters 2–5** of the core specification. This is Agent B's crate per
//! `spec/QUICKSTART.md`; it depends only on [`epiphany_determinism`] (Agent A)
//! and the small set of crates sanctioned by the QUICKSTART's implementation
//! decisions.
//!
//! The graph is the canonical truth about the music (Chapter 5
//! §"Design Principles"); layout, serialization, and editing operations are
//! downstream projections and consumers of it.
//!
//! ## What lives here
//!
//! * `ids` — the typed 128-bit identifier family ([`EventId`], [`PitchId`],
//!   [`VoiceId`], …, [`TypedObjectId`]), [`ReplicaId`] with its reserved
//!   [`ReplicaId::SYSTEM_DERIVED`] namespace, [`OperationId`], the
//!   replica-plus-counter generation scheme, and the deterministic
//!   system-derived-counter derivation (Chapter 5 §"Identifiers").
//! * `time` — [`RationalTime`] (exact rational, inline-or-promoted),
//!   [`MusicalPosition`]/[`MusicalDuration`] with type-enforced algebra,
//!   [`WallClockTime`]/[`WallClockDuration`], [`TimeAnchor`]/[`AnchorOffset`],
//!   and the event temporal-coordinate unions [`EventPosition`],
//!   [`EventDuration`], [`ConcreteDuration`] (Chapter 3).
//! * `pitch` — [`Pitch`], [`ScalePosition`], [`IdentifiedPitch`],
//!   [`PitchSpelling`], and the spelling-attachment subsystem (Chapter 2;
//!   Chapter 4 for the tuning/pitch-space registry identifiers it references).
//! * `event` — the [`Event`] taxonomy and the [`EventArena`] (Chapter 5
//!   §"The Event Arena").
//! * `graph` — [`Canvas`], [`Region`], [`Staff`]/[`StaffInstance`] (distinct
//!   types), [`Voice`], [`Measure`], [`BarlineAlignmentGroup`], and the
//!   reference-bearing cross-cutting structures (Chapter 5).
//! * `prepass` — the spelling and notational-decomposition pre-passes
//!   ([`derive_annotations`]): canonical *derived annotations* recomputed on
//!   materialization, not stored graph state (Chapter 2 §"The Spelling
//!   Pre-Pass"; Chapter 3 §"Notational Decomposition").
//! * `invariants` — the Chapter 5 graph-invariant checker, with one check
//!   per enumerated invariant and a typed witness for each violation.
//!
//! ## Implementation decisions (per QUICKSTART "Decisions you'll need to make")
//!
//! See `DECISIONS.md`. In brief: replica entropy via `getrandom` (decision 1),
//! event-arena storage via `slotmap` (decision 2), fully sync (decision 4),
//! current stable Rust (decision 5). `unsafe` is forbidden crate-wide.

mod codec;
mod event;
mod graph;
mod ids;
mod indexes;
mod invariants;
mod pitch;
mod tempo;
mod textvalue_event;
mod textvalue_graph;
mod textvalue_impls;
mod textvalue_pitch;
mod textvalue_time;
mod time;

pub mod fuzz;
pub mod generators;
pub mod prepass;
pub mod textvalue;

pub use ids::{
    derive_system_id, AnalysisLayerId, AnalyticalAnnotationId, BarlineAlignmentGroupId, BeamId,
    ChordSymbolId, CommentId, EventId, GraphId, GraphicGestureId, GraphicObjectId, IdentityContext,
    InstrumentId, IntegrityAnomalyId, LyricLineId, MarkerId, MeasureId, ObjectKindRegistryId,
    OperationId, PartDefinitionId, PitchId, RegionId, RepeatStructureId, ReplicaId, SlurId,
    SpannerId, StaffGroupId, StaffId, StaffInstanceId, TieId, TimeSignatureId, TransactionId,
    TupletId, TypedObjectId, ViewId, VoiceId,
};

pub use time::{
    AnchorOffset, ConcreteDuration, CoordinateKind, DurationBounds, EventBounds, EventDuration,
    EventPosition, MeasurePosition, MusicalDuration, MusicalPosition, OffsetKind, RationalTime,
    RegionEdge, SmallRational, TimeAnchor, TimeBounds, WallClockDuration, WallClockTime,
};

pub use pitch::{
    canonical_pitch_bytes, derive_system_pitch_id, spell, AccidentalId, AccidentalRegistryId,
    AcousticPitch, AcousticRealization, CmnNominal, DecompositionAlgorithmId, ForeignFormatId,
    IdentifiedPitch, NominalRegistryId, Pitch, PitchRange, PitchSpaceId, PitchSpacePosition,
    PitchSpelling, PositionRegistryId, ReferencePitch, ScalePosition, SpellingAlgorithmId,
    SpellingAttachment, SpellingContext, SpellingDirective, SpellingNominal, SpellingPrecedence,
    SpellingRenderHints, SpellingRule, SpellingRuleSetId, SpellingScope, SpellingSource,
    SpellingSourceKind, StaffGroupKindRegistryId, TieClassRegistryId, TransposeRefusal,
    TranspositionInterval, TuningReference, TuningSystemId, VoiceSelector,
};

pub use prepass::{
    derive_annotations, resolve_decomposition, resolve_spelling, simplest_spelling,
    DerivedAnnotations, PrePassError, PrePassProfile, ResolvedSpelling, SpellingProvenance,
    TaxonomyReport,
};

pub use event::{
    ArenaError, ArticulationMark, CueEvent, CueRendering, DynamicMark, Event, EventArena, EventKey,
    GraceKind, GraphicEvent, IndeterminacyHints, IndeterminacyKind, IndeterminateEvent,
    OrnamentMark, PitchedEvent, PlaybackBinding, Rest, StaffPosition, StemConfiguration,
    TrajectoryDisplay, TrajectoryEndpoint, TrajectoryEvent, TrajectoryShape, UnpitchedEvent,
    UnpitchedMemberId,
};

pub use graph::{
    derive_promoted_voice_id, AleatoricAnchoringDiscipline, AleatoricTimeModel, AnalysisLayer,
    AnalyticalAnnotation, AnnotationAnchor, BarlineAlignmentGroup, BarlineAlignmentMember, Beam,
    BeamGeometryOverride, BeatGroup, BracketKind, Canvas, CanvasLayoutDefaults, CanvasMargins,
    CanvasSize, ChordSymbol, Clef, ClefChange, ClefShape, Comment, CoordinateDiscipline,
    CrossCuttingRegistry, CurvatureOverride, CurveDirection, DecompositionAttachment,
    DecompositionSource, EventOrderingDAG, GestureAnchoring, GraphicContent, GraphicGesture,
    GraphicObject, HairpinDirection, Instrument, KeySignature, KeySignatureChange, LineStyle,
    LyricLine, Marker, Measure, MeasureNumberVisibility, MetadataEntry, MetadataValue, MeterChange,
    MetricGrid, MetricTimeModel, NotatedComponent, NoteValue, OctaveOffset, PartDefinition,
    PedalKind, PowerOfTwo, ProportionalTimeModel, Region, RegionContent, RegionTimeModel,
    RepeatKind, RepeatStructure, Score, ScoreMetadata, ScoreTuningContext, Slur, SlurKind,
    SoundConfiguration, SpaceUnit, SpanStyle, Spanner, SpannerKind, Staff, StaffBasedContent,
    StaffBracketKind, StaffExtent, StaffGroup, StaffGroupKind, StaffInstance,
    StaffLineConfiguration, StemDirection, SubBeam, TempoMapReference, TextLineDefinition, Tie,
    TieClass, TimeExtent, TimeSignature, TimeSignatureDisplay, Timestamp, Tuplet, TupletRatio,
    UnpitchedMember, ViewDefinition, Voice, VoiceOrigin, Volta,
};

pub use tempo::{
    inversion_tolerance, Tempo, TempoError, TempoMap, TempoSegment, TempoShape,
    INVERSION_MAX_DENOMINATOR, INVERSION_MAX_ITERATIONS,
};

pub use codec::{CanonicalValue, ScoreDecodeError};

pub use indexes::ScoreIndexes;

pub use invariants::{
    check_invariant, check_invariants, deferred_checks, DeferredCheck, GraphInvariant,
    InvariantViolation,
};
