#![forbid(unsafe_code)]
//! # epiphany-layout-ir
//!
//! The Epiphany **layout intermediate representation** and the
//! **constraint-solver interface**, implementing the normative requirements of
//! **Chapter 7** ("Layout Intermediate Representation") and the interface of
//! **Chapter 9** ("Constraint-Solver Interface") of the core specification. This
//! is Agent E's crate per `spec/QUICKSTART.md`; it lands last among the
//! implementation crates, building on Agent A's [`epiphany_determinism`] and
//! Agent B's [`epiphany_core`] (and on Agent C's `OperationKindTag` for the
//! edit-barrier types — see `DECISIONS.md`).
//!
//! The IR is a pipeline of four stages, each with its own type and a
//! deterministic, provenance-preserving contract for the next (Chapter 7
//! §"Design Principles"):
//!
//! 1. [`LogicalLayoutIR`] — the structural projection of the score graph, with
//!    engraving decisions made and positions unresolved.
//! 2. [`ConstrainedLayoutIR`] — composite objects flattened to glyphs, each with
//!    the anchor geometry the solver consumes, plus the vertical-band model.
//! 3. [`ResolvedLayoutIR`] — every glyph positioned, the solver's output.
//! 4. [`RenderIR`] — renderer-bound primitives (**interface only**; no
//!    rendering — QUICKSTART, Agent E).
//!
//! ## What v0 implements (QUICKSTART, Agent E)
//!
//! * The four IR stages and the [`TimeAxisModel`] tagged enum (Metric /
//!   Proportional / Aleatoric / Registered — *not* `Box<dyn TimeAxis>`).
//! * The [`Provenance`] back-references every layout object carries — what makes
//!   incremental layout possible and what the round-trip proves is preserved.
//! * [`EngravingDecision`] records (Chapter 7 §"Engraving Decisions").
//! * The vertical-band model ([`VerticalBand`], Chapter 7 §"Vertical Bands").
//! * [`GlyphCatalogIdentity`] with Bravura metrics bundled in-tree
//!   ([`glyph::BRAVURA_METRICS`]) and the `MUSCFNTM`-domain-tagged
//!   [`metrics_hash_for`] (Chapter 7 §7.3.2).
//! * The [`EditBarrier`] types keyed on Agent C's [`OperationKindTag`]
//!   (Chapter 8 §"Forward Compatibility and Edit Barriers").
//! * The [`ConstraintSolver`] interface (Chapter 9) and the v0 [`StubSolver`],
//!   which "returns `SolveStatus::Solved` with the input geometry verbatim."
//!
//! The Chapter 7 interface surface includes the composite-object taxonomy,
//! spring slots and constraints, pages/systems, engraving overrides, and the
//! incremental dependency/cache model. The v0 algorithms remain intentionally
//! simple: they do not perform production engraving, casting-off, quality-metric
//! computation, or rendering.
//!
//! ## Determinism
//!
//! IR coordinates are single-precision staff spaces ([`StaffSpace`], Chapter 7
//! §7.2). The **canonical** `ResolvedLayoutIR` output quantizes them to the
//! `1/1024` grid ([`epiphany_determinism::QuantizedCoord`]) at serialization
//! time ([`ResolvedLayoutIR::canonical_bytes`]), exactly as Appendix D
//! §"Quantized Layout Coordinates" prescribes — quantization absorbs all f32
//! variation below `1/2048` staff space, and a non-finite coordinate is
//! *rejected* (a panic), never normalized. That canonical encoding covers the
//! full resolved layout (every glyph's provenance and quantized position, the
//! engraving decisions, and the catalog identity), so any change that
//! distinguishes two layouts changes their canonical bytes.
//!
//! ## The round-trip (v0 acceptance criterion 6)
//!
//! [`round_trip`] runs graph → logical → constrained → stub-solved → render and
//! asserts the pipeline completes without losing any provenance back-reference,
//! recovering exactly the laid-out graph objects ([`laid_out_object_ids`]). The
//! testkit's layout harness drives this entry point.

pub mod barrier;
pub mod cache;
pub mod constrained;
pub mod engrave_theory;
pub mod engraving;
pub mod glyph;
pub mod logical;
pub mod provenance;
pub mod render;
pub mod resolved;
pub mod roundtrip;
pub mod solver;
pub mod spatial;
pub mod time_axis;
pub mod vertical_band;

pub use barrier::{
    AlwaysLiveOracle, BarrierCondition, BarrierConditionRegistryId, BarrierScope,
    BarrierScopeRegistryId, EditBarrier, EditContext, EditOracle, ExtensionRef, ObjectKind,
};
pub use cache::{
    ConstrainedRegionCache, DependencyIndex, FineLayoutCache, LayoutCache, LogicalRegionCache,
    ResolvedSystemCache, SystemId,
};
pub use constrained::{
    to_constrained, try_to_constrained, Axis, BreakKind, ConstrainedLayoutIR,
    ConstrainedLayoutRegion, ConstrainedValidationError, ConstraintParameters,
    ConstraintRegistryId, GlyphObject, GlyphObjectId, GlyphStyle, LayoutConstraint,
    LayoutTransformError, SpringSlot, Stroke,
};
pub use engrave_theory::{
    accidental_glyph, clef_glyph, flag_glyph, has_stem, key_signature, notehead_glyph, rest_glyph,
    staff_position, KeyAccidental, StaffStep,
};
pub use engraving::{
    AuthorId, DecisionSource, EngravingDecision, EngravingDecisionId, EngravingDecisionKind,
    EngravingDecisionRegistryId, EngravingOverride, EngravingOverrideId, ForeignFormatId,
    OverrideKind, OverrideOrigin, OverridePriority, OverrideTarget, PluginId, Timestamp,
};
pub use glyph::{
    all_available, bravura_catalog_identity, metrics, metrics_hash_for, BravuraCatalog, FontId,
    GlyphAnchor, GlyphBitmap, GlyphCatalog, GlyphCatalogIdentity, GlyphMetric, GlyphReference,
    GlyphRenderData, PathCommand, SemVer, SmuflVersion, BRAVURA_METRICS, BRAVURA_VERSION,
};
pub use logical::{
    to_logical, BarLineLayout, BarlineKind, BeamGroupLayout, ChordLayout, ClefLayout,
    CompositeLayoutObject, CrossRegionObject, CueLayout, GraphicLayout, GroupLayout,
    KeySignatureLayout, LayoutContent, LayoutObject, LayoutRegion, LocalCoordinateSystem,
    LogicalLayoutIR, MarkerLayout, MeasureContent, MultimeasureRestLayout, NoteContent, NoteLayout,
    NotePitch, PlacedClef, PlacedComponent, PlacedKeySignature, RestContent, RestLayout,
    ScoreVersion, SlurLayout, SpannerLayout, StaffContent, StaffLayout, TextLayout, TieLayout,
    TimeSignatureContent, TimeSignatureDisplayLayout, TrajectoryLayout, TupletDisplayLayout,
    VerticalExtent,
};
pub use provenance::{
    manifestation_layout_id, stable_layout_id, synthesized_layout_id, LayoutObjectId, Provenance,
    SynthesisInstanceKey, SynthesisKind, SynthesisRegistryId,
};
pub use render::{
    to_render, ColorConfiguration, ColorSpace, PassthroughRenderProducer,
    RasterizationConfiguration, RenderConfiguration, RenderIR, RenderIRProducer, RenderPrimitive,
    RenderTarget,
};
pub use resolved::{
    ResolvedGlyph, ResolvedLayoutIR, ResolvedMeasure, ResolvedPage, ResolvedStaff, ResolvedSystem,
};
pub use roundtrip::{laid_out_object_ids, round_trip, RoundTripReport};
pub use solver::{
    ConstraintId, ConstraintSolver, ExtensionMetric, ExtensionMetricId, ExtensionWarningId,
    InvalidationScope, InvalidationSet, NormalizedMetric, QualityMetricKind, QualityMetricVector,
    SolveReport, SolveStatus, SolverBudget, SolverBudgetUsed, SolverConfig, SolverProfile,
    SolverState, SolverTier, SolverVersion, SolverWarning, SolverWarningKind, SpringSlotId,
    StubSolver, TieBreakingWeights,
};
pub use spatial::{
    BoundingBox, Margins, Point, Rect, ScaleContext, Size2D, StaffSpace, Transform2D,
};
pub use time_axis::{
    time_axis_of, AleatoricTimeAxis, MetricTimeAxis, ProportionalTimeAxis,
    SerializedRegisteredAxis, TimeAxis, TimeAxisKind, TimeAxisModel, TimeAxisRegistryId, TimePoint,
    TimeRange,
};
pub use vertical_band::{inter_staff_gap_id, VerticalBand, VerticalBandId, VerticalBandKind};

// Re-exported so doc links and the `OperationKindTag`-keyed edit-barrier API are
// reachable from this crate's root.
pub use epiphany_ops::OperationKindTag;
// `StemDirection` (Agent B) is the payload of `EngravingDecisionKind::StemDirection`;
// re-exported so callers constructing that decision need not also import from core.
pub use epiphany_core::StemDirection;
