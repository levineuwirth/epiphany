#![forbid(unsafe_code)]
//! # epiphany-render-svg
//!
//! Agent I's **SVG renderer** behind the Epiphany `RenderIR` interface (spec
//! **Chapter 7** §"RenderIR"): it turns a
//! [`ResolvedLayoutIR`](epiphany_layout_ir::ResolvedLayoutIR) into well-formed
//! **SVG 1.1**, drawing each glyph as a **genuine Bravura SMuFL outline**
//! `<path>`. It is the visible end of the v0 `Score → layout IR` pipeline: from a
//! resolved layout, produce an image a musician would recognise.
//!
//! ## Scope of this phase (renderer-against-stub)
//!
//! Per the QUICKSTART development pattern (`spec/PHASE2_QUICKSTART.md`, Agent I),
//! the renderer is built and golden-locked against the **stub solver's** output
//! first, before the real engraving solver lands. The stub returns the
//! constrained IR's geometry verbatim — a structural projection, not yet real
//! notation — so this phase proves the renderer is *correct and faithful* (every
//! glyph drawn from its real Bravura outline, provenance preserved, output
//! XML-valid and deterministic), independently of engraving quality. The real
//! [`epiphany_engrave`](../epiphany_engrave/index.html) solver and the
//! score→real-notation engraving pass are the next phase; the renderer already
//! consumes any solver's `ResolvedLayoutIR`.
//!
//! ## What it draws, and the non-overreach rule
//!
//! The bundled outlines are extracted from the official OFL `Bravura.otf` (see
//! `tools/extract_bravura_outlines.py` and `tools/OFL.txt`) in staff-space,
//! y-up coordinates. The renderer makes SVG-encoding choices only and never
//! engraving-semantic ones; see [`svg`] for the coordinate system, the
//! provenance-tracing contract, and the diagnostic-not-paper-over rule.
//!
//! ## Font availability
//!
//! The default and only mode this phase is [`GlyphMode::PathOutline`] — inline
//! outlines, so the SVG is self-contained and needs no font installed
//! (QUICKSTART, Agent I, recommendation). An embedded-`@font-face` mode is a
//! future option; it is intentionally not implemented yet rather than stubbed
//! dishonestly.

mod outline;
mod outlines_generated;
mod svg;
pub mod xml;

pub use outline::{bundled_glyph_count, smufl_codepoint};
pub use svg::{
    render, Diagnostic, GlyphClass, GlyphMode, RenderOptions, RenderOutput, RenderStats,
};
pub use xml::{check_well_formed, XmlError};

// Re-exported so callers can name the renderer's input without also importing
// epiphany-layout-ir directly.
pub use epiphany_layout_ir::{ResolvedLayoutIR, ScaleContext};
