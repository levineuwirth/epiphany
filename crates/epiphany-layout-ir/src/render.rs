//! Stage 4 — `RenderIR` (Chapter 7 §"RenderIR (Interface Only)").
//!
//! The renderer's input. Its full specification (page templates, draw calls,
//! rasterization) is delivered separately; this module defines only the
//! interface contract and the one obligation that crosses it: "every renderer
//! primitive MUST be traceable to its originating `ResolvedGlyph` (and therefore
//! to its score graph source)" (Chapter 7 §"RenderIR"). v0 implements that
//! interface — provenance and position flow through — and performs **no actual
//! rendering** (QUICKSTART, Agent E).

use crate::provenance::Provenance;
use crate::resolved::ResolvedLayoutIR;
use crate::spatial::{Point, ScaleContext};
use crate::{BoundingBox, Curve, GlyphReference, GlyphStyle, Stroke, Transform2D};

/// A single renderer primitive (Chapter 7 §"RenderIR"). Interface only — it
/// carries just enough to prove the provenance-preservation contract: every
/// primitive traces back to its `ResolvedGlyph`'s source.
#[derive(Clone, PartialEq, Debug)]
pub struct RenderPrimitive {
    pub provenance: Provenance,
    /// The SMuFL glyph to draw — what the renderer needs to know to produce
    /// output (Chapter 7 §"RenderIR": every primitive is traceable to its
    /// `ResolvedGlyph`, including its glyph reference).
    pub glyph: GlyphReference,
    pub position: Point,
    pub transform: Option<Transform2D>,
    pub bounding_box: BoundingBox,
    pub style: GlyphStyle,
    pub layer: i32,
}

/// The render IR interface output (Chapter 7 §"RenderIR").
#[derive(Clone, PartialEq, Debug)]
pub struct RenderIR {
    pub primitives: Vec<RenderPrimitive>,
    /// Non-glyph line primitives (staff lines, stems, barlines, …), traced like
    /// the glyph primitives so the round-trip recovers their sources too.
    pub strokes: Vec<Stroke>,
    /// Cubic-bézier curve primitives (slurs, …), traced like the strokes.
    pub curves: Vec<Curve>,
}

/// The render target (Chapter 7 §"RenderIR": `RenderConfiguration.target`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RenderTarget {
    Pdf,
    Svg,
    Screen,
    Print,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ColorSpace {
    Srgb,
    DisplayP3,
    Cmyk,
    Grayscale,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ColorConfiguration {
    pub color_space: ColorSpace,
    pub embed_profile: bool,
}

impl Default for ColorConfiguration {
    fn default() -> Self {
        ColorConfiguration {
            color_space: ColorSpace::Srgb,
            embed_profile: true,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RasterizationConfiguration {
    pub antialias: bool,
    pub dpi: u32,
}

impl Default for RasterizationConfiguration {
    fn default() -> Self {
        RasterizationConfiguration {
            antialias: true,
            dpi: 300,
        }
    }
}

/// Render configuration (Chapter 7 §"RenderIR": `RenderConfiguration`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RenderConfiguration {
    pub target: RenderTarget,
    pub color: ColorConfiguration,
    pub rasterization: RasterizationConfiguration,
}

/// The render-projection interface (Chapter 7 §"RenderIR": `RenderIRProducer`).
/// The spec's `produce(resolved, scale, config)` signature is honored; v0
/// performs **no** rendering (the primitive vocabulary belongs to the
/// out-of-core renderer) but guarantees provenance and glyph identity flow
/// across the boundary.
pub trait RenderIRProducer {
    /// Converts resolved IR to renderer-bound primitives, preserving provenance.
    fn produce(
        &self,
        resolved: &ResolvedLayoutIR,
        scale: ScaleContext,
        config: RenderConfiguration,
    ) -> RenderIR;
}

/// The v0 render producer: one primitive per resolved glyph, provenance, glyph
/// identity, and position preserved, no rendering performed. The `scale` and
/// `config` are accepted (interface fidelity) but not consumed in v0.
pub struct PassthroughRenderProducer;

impl RenderIRProducer for PassthroughRenderProducer {
    fn produce(
        &self,
        resolved: &ResolvedLayoutIR,
        _scale: ScaleContext,
        _config: RenderConfiguration,
    ) -> RenderIR {
        to_render(resolved)
    }
}

/// The RenderIR interface call (Chapter 7 §"RenderIR"): one primitive per
/// resolved glyph, provenance and position preserved.
pub fn to_render(resolved: &ResolvedLayoutIR) -> RenderIR {
    RenderIR {
        primitives: resolved
            .glyphs
            .iter()
            .map(|g| RenderPrimitive {
                provenance: g.provenance.clone(),
                glyph: g.glyph.clone(),
                position: g.position,
                transform: g.transform,
                bounding_box: g.bounding_box,
                style: g.style,
                layer: g.layer,
            })
            .collect(),
        strokes: resolved.strokes.clone(),
        curves: resolved.curves.clone(),
    }
}
