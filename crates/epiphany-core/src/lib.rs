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
//!   [`PitchSpelling`], the spelling-attachment subsystem, and the spelling
//!   pre-pass stub (Chapter 2; Chapter 4 for the tuning/pitch-space registry
//!   identifiers it references).
//! * `event` — the [`Event`] taxonomy and the [`EventArena`] (Chapter 5
//!   §"The Event Arena").
//! * `graph` — [`Canvas`], [`Region`], [`Staff`]/[`StaffInstance`] (distinct
//!   types), [`Voice`], [`Measure`], [`BarlineAlignmentGroup`], and the
//!   reference-bearing cross-cutting structures (Chapter 5).
//! * `invariants` — the Chapter 5 graph-invariant checker, with one check
//!   per enumerated invariant and a typed witness for each violation.
//!
//! ## Implementation decisions (per QUICKSTART "Decisions you'll need to make")
//!
//! See `DECISIONS.md`. In brief: replica entropy via `getrandom` (decision 1),
//! event-arena storage via `slotmap` (decision 2), fully sync (decision 4),
//! current stable Rust (decision 5). `unsafe` is forbidden crate-wide.

mod event;
mod graph;
mod ids;
mod indexes;
mod invariants;
mod pitch;
mod tempo;
mod time;

pub mod generators;

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
    derive_system_pitch_id, spell, AccidentalId, AccidentalRegistryId, AcousticPitch,
    AcousticRealization, CmnNominal, ForeignFormatId, IdentifiedPitch, NominalRegistryId, Pitch,
    PitchSpaceId, PitchSpacePosition, PitchSpelling, PositionRegistryId, ReferencePitch,
    ScalePosition, SpellingAlgorithmId, SpellingAttachment, SpellingContext, SpellingDirective,
    SpellingNominal, SpellingPrecedence, SpellingRenderHints, SpellingRule, SpellingRuleSetId,
    SpellingScope, SpellingSource, SpellingSourceKind, StaffGroupKindRegistryId,
    TieClassRegistryId, TuningReference, TuningSystemId, VoiceSelector,
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
    BeatGroup, Canvas, ChordSymbol, ClefChange, Comment, CoordinateDiscipline,
    CrossCuttingRegistry, DecompositionAttachment, DecompositionSource, EventOrderingDAG,
    GestureAnchoring, GraphicContent, GraphicGesture, GraphicObject, Instrument,
    KeySignatureChange, LyricLine, Marker, Measure, MeasureNumberVisibility, MeterChange,
    MetricGrid, MetricTimeModel, NotatedComponent, NoteValue, PartDefinition, PowerOfTwo,
    ProportionalTimeModel, Region, RegionContent, RegionTimeModel, RepeatStructure, Score,
    ScoreMetadata, ScoreTuningContext, Slur, Spanner, Staff, StaffBasedContent, StaffExtent,
    StaffGroup, StaffGroupKind, StaffInstance, StaffLineConfiguration, StemDirection,
    TempoMapReference, Tie, TieClass, TimeExtent, TimeSignature, TimeSignatureDisplay, Tuplet,
    TupletRatio, ViewDefinition, Voice, VoiceOrigin,
};

pub use tempo::{
    Tempo, TempoError, TempoMap, TempoSegment, TempoShape, INVERSION_MAX_DENOMINATOR,
    INVERSION_MAX_ITERATIONS, INVERSION_TOLERANCE_WHOLE_NOTES,
};

pub use indexes::ScoreIndexes;

pub use invariants::{check_invariant, check_invariants, GraphInvariant, InvariantViolation};
