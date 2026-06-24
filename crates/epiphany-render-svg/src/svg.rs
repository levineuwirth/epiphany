//! The SVG renderer: a [`ResolvedLayoutIR`] to well-formed SVG 1.1.
//!
//! ## What this renderer is, and is not
//!
//! It is a **renderer**, not an engraver. Per the QUICKSTART non-overreach rule
//! (`spec/PHASE2_QUICKSTART.md`, Agent I), every emitted SVG element traces to a
//! [`ResolvedGlyph`] (and thus to its score-graph source) or to a declared
//! renderer wrapper (the `<svg>` root, a metadata comment, the y-flip group, a
//! per-layer `<g>`). The renderer makes *SVG-encoding* choices only — grouping,
//! `<path>` vs `<rect>` fallback, the viewBox, the coordinate flip, colour
//! representation. It makes **no engraving-semantic** choices (stem direction,
//! spacing, beam slope, accidental placement, glyph selection): those are the
//! solver's and the IR's. If a glyph it is asked to draw has no bundled outline,
//! it surfaces a [`Diagnostic`] and falls back to a visible bounding-box rect —
//! it never invents geometry.
//!
//! ## Coordinate system
//!
//! The IR is in **staff spaces, y-up** (musical convention: larger `y` = higher
//! pitch). SVG is y-down. The renderer keeps every coordinate in staff-space,
//! y-up world units and applies a single y-flip on the content group
//! (`translate(-min_x, max_y) scale(1, -1)`), so a world point `(x, y)` maps to
//! screen `(x - min_x, max_y - y)`. The `viewBox` is `0 0 W H` in staff spaces;
//! `width`/`height` carry the display scale in px
//! ([`RenderOptions::px_per_staff_space`]). Glyph outlines are bundled in the
//! same staff-space/y-up frame, so each is placed with a plain
//! `translate(x, y)`.
//!
//! ## Determinism
//!
//! Outline `d` data is fixed bundled text; every computed number is formatted to
//! at most 4 decimals with `-0` normalised to `0`, so identical input yields
//! byte-identical SVG (the acceptance snapshot golden-locks this).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use epiphany_layout_ir::{BoundingBox, Provenance, ResolvedGlyph, ResolvedLayoutIR};

use crate::outline::outline;
use crate::xml::{check_well_formed, escape_attr};

/// How glyphs are drawn.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum GlyphMode {
    /// Inline genuine Bravura outline `<path>`s (default). Self-contained: the
    /// SVG renders in any viewer with no font dependency (QUICKSTART, Agent I:
    /// "inline path outlines for golden fixtures and the demonstrable
    /// deliverable").
    #[default]
    PathOutline,
}

/// Renderer configuration. SVG-encoding choices only — nothing here changes
/// engraving.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RenderOptions {
    /// Display scale: points (px) per staff space, applied as the `<svg>`
    /// width/height while the viewBox stays in staff spaces.
    pub px_per_staff_space: f32,
    /// Blank margin around the content, in staff spaces.
    pub margin: f32,
    /// How glyphs are drawn.
    pub glyph_mode: GlyphMode,
    /// Emit `data-*` provenance attributes tracing each element to its source.
    pub emit_provenance: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            px_per_staff_space: 10.0,
            margin: 2.0,
            glyph_mode: GlyphMode::PathOutline,
            emit_provenance: true,
        }
    }
}

/// A coarse glyph category, used for the machine acceptance snapshot's
/// bounding-box-class counts.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum GlyphClass {
    Notehead,
    Clef,
    Accidental,
    Rest,
    Flag,
    TimeSignature,
    Barline,
    Dynamic,
    AugmentationDot,
    Other,
}

impl GlyphClass {
    /// Classifies a SMuFL glyph name by its category prefix.
    pub fn of(name: &str) -> GlyphClass {
        if name.starts_with("notehead") {
            GlyphClass::Notehead
        } else if name.ends_with("Clef") {
            GlyphClass::Clef
        } else if name.starts_with("accidental") {
            GlyphClass::Accidental
        } else if name.starts_with("rest") {
            GlyphClass::Rest
        } else if name.starts_with("flag") {
            GlyphClass::Flag
        } else if name.starts_with("timeSig") {
            GlyphClass::TimeSignature
        } else if name.starts_with("barline") {
            GlyphClass::Barline
        } else if name.starts_with("dynamic") {
            GlyphClass::Dynamic
        } else if name == "augmentationDot" {
            GlyphClass::AugmentationDot
        } else {
            GlyphClass::Other
        }
    }

    /// A short, stable token for the SVG `data-class` attribute and snapshots.
    pub fn token(self) -> &'static str {
        match self {
            GlyphClass::Notehead => "notehead",
            GlyphClass::Clef => "clef",
            GlyphClass::Accidental => "accidental",
            GlyphClass::Rest => "rest",
            GlyphClass::Flag => "flag",
            GlyphClass::TimeSignature => "timeSig",
            GlyphClass::Barline => "barline",
            GlyphClass::Dynamic => "dynamic",
            GlyphClass::AugmentationDot => "augmentationDot",
            GlyphClass::Other => "other",
        }
    }
}

/// A non-fatal note the renderer surfaces instead of papering over a problem
/// (QUICKSTART, Agent I: "surface it via a diagnostic, don't paper over it").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    pub message: String,
    pub glyph: Option<String>,
}

/// Machine-readable facts about a render, for the golden-locked acceptance
/// snapshot (object/glyph/path/provenance/class counts + bounds).
#[derive(Clone, PartialEq, Debug)]
pub struct RenderStats {
    /// Glyphs in the resolved layout (the renderer's input objects).
    pub glyph_count: usize,
    /// `<path>` elements emitted (glyphs drawn from a bundled outline).
    pub path_count: usize,
    /// Fallback `<rect>` elements emitted (glyphs with no bundled outline).
    pub fallback_rect_count: usize,
    /// Elements carrying a `data-prov` trace back to a score-graph source.
    pub provenance_count: usize,
    /// Distinct layers, each rendered as one `<g>` group.
    pub layer_count: usize,
    /// Per-class glyph counts.
    pub class_counts: BTreeMap<GlyphClass, usize>,
    /// The content viewBox `[min_x, min_y, width, height]`, in staff spaces.
    pub view_box: [f32; 4],
}

/// The result of a render: the SVG text, machine stats, and any diagnostics.
#[derive(Clone, PartialEq, Debug)]
pub struct RenderOutput {
    pub svg: String,
    pub stats: RenderStats,
    pub diagnostics: Vec<Diagnostic>,
}

impl RenderOutput {
    /// Whether the emitted SVG passes the in-crate well-formedness check.
    pub fn is_well_formed(&self) -> bool {
        check_well_formed(&self.svg).is_ok()
    }
}

/// Renders a resolved layout to SVG. Pure and deterministic.
pub fn render(resolved: &ResolvedLayoutIR, options: &RenderOptions) -> RenderOutput {
    let mut diagnostics = Vec::new();
    let mut class_counts: BTreeMap<GlyphClass, usize> = BTreeMap::new();
    for g in &resolved.glyphs {
        *class_counts
            .entry(GlyphClass::of(g.glyph.as_str()))
            .or_insert(0) += 1;
    }

    // World bounds over each glyph's drawn extent (its outline bbox, or the IR
    // bounding box for a glyph with no bundled outline).
    let bounds = content_bounds(resolved, options.margin);

    let (min_x, min_y, width, height) = match bounds {
        Some(b) => b,
        // Empty layout: a minimal, valid, honest empty canvas.
        None => {
            let svg = empty_svg();
            let well_formed = check_well_formed(&svg).is_ok();
            debug_assert!(well_formed);
            return RenderOutput {
                stats: RenderStats {
                    glyph_count: 0,
                    path_count: 0,
                    fallback_rect_count: 0,
                    provenance_count: 0,
                    layer_count: 0,
                    class_counts,
                    view_box: [0.0, 0.0, 1.0, 1.0],
                },
                svg,
                diagnostics,
            };
        }
    };
    let max_y = min_y + height;

    // Group glyph indices by layer (ascending), preserving input order within.
    let mut layers: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, g) in resolved.glyphs.iter().enumerate() {
        layers.entry(g.layer).or_default().push(i);
    }

    let mut path_count = 0;
    let mut fallback_rect_count = 0;
    let mut provenance_count = 0;

    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        num(width * options.px_per_staff_space),
        num(height * options.px_per_staff_space),
        num(width),
        num(height),
    );
    // Declared metadata wrapper (a comment — honest about what this is).
    s.push_str(
        "  <!-- epiphany-render-svg: glyphs are genuine Bravura SMuFL outlines; \
         geometry is the resolved layout verbatim, no engraving performed here -->\n",
    );
    // The single y-flip wrapper: staff-space/y-up world -> SVG y-down.
    let _ = writeln!(
        s,
        "  <g transform=\"translate({} {}) scale(1 -1)\">",
        num(-min_x),
        num(max_y),
    );

    for (layer, indices) in &layers {
        let _ = writeln!(s, "    <g data-layer=\"{}\">", layer);
        for &i in indices {
            let g = &resolved.glyphs[i];
            let name = g.glyph.as_str();
            let (x, y) = (g.position.x.0, g.position.y.0);
            let (fill, opacity) = colour(g.style.rgba);
            let prov = if options.emit_provenance {
                provenance_count += 1;
                provenance_attrs(&g.provenance, name, GlyphClass::of(name))
            } else {
                String::new()
            };
            match outline(name) {
                Some(o) => {
                    path_count += 1;
                    let _ = writeln!(
                        s,
                        "      <path d=\"{}\" transform=\"translate({} {})\" fill=\"{}\"{}{}/>",
                        o.path,
                        num(x),
                        num(y),
                        fill,
                        opacity,
                        prov,
                    );
                }
                None => {
                    // No outline: surface it and draw the IR bounding box so the
                    // missing glyph is visible, not silently absent.
                    fallback_rect_count += 1;
                    diagnostics.push(Diagnostic {
                        message: "no bundled Bravura outline; drew bounding-box fallback"
                            .to_owned(),
                        glyph: Some(name.to_owned()),
                    });
                    let bb = g.bounding_box;
                    let _ = writeln!(
                        s,
                        "      <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
                         fill=\"none\" stroke=\"#cc0000\" stroke-width=\"0.05\"{}/>",
                        num(x + bb.left.0),
                        num(y + bb.bottom.0),
                        num((bb.right.0 - bb.left.0).max(0.0)),
                        num((bb.top.0 - bb.bottom.0).max(0.0)),
                        prov,
                    );
                }
            }
        }
        s.push_str("    </g>\n");
    }

    s.push_str("  </g>\n");
    s.push_str("</svg>\n");

    RenderOutput {
        stats: RenderStats {
            glyph_count: resolved.glyphs.len(),
            path_count,
            fallback_rect_count,
            provenance_count,
            layer_count: layers.len(),
            class_counts,
            view_box: [num_f(min_x), num_f(min_y), num_f(width), num_f(height)],
        },
        svg: s,
        diagnostics,
    }
}

/// The content bounds `(min_x, min_y, width, height)` in staff spaces, padded by
/// `margin`, or `None` if there is nothing to draw.
fn content_bounds(resolved: &ResolvedLayoutIR, margin: f32) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut any = false;
    for g in &resolved.glyphs {
        let bb = drawn_bbox(g);
        let (x, y) = (g.position.x.0, g.position.y.0);
        for (px, py) in [
            (x + bb.left.0, y + bb.bottom.0),
            (x + bb.right.0, y + bb.top.0),
        ] {
            if px.is_finite() && py.is_finite() {
                any = true;
                min_x = min_x.min(px);
                min_y = min_y.min(py);
                max_x = max_x.max(px);
                max_y = max_y.max(py);
            }
        }
    }
    if !any {
        return None;
    }
    min_x -= margin;
    min_y -= margin;
    max_x += margin;
    max_y += margin;
    // Guard against a degenerate zero-area box (e.g. a single zero-size glyph).
    let width = (max_x - min_x).max(f32::EPSILON);
    let height = (max_y - min_y).max(f32::EPSILON);
    Some((num_f(min_x), num_f(min_y), num_f(width), num_f(height)))
}

/// The bounding box the renderer uses for a glyph: its real outline extent when
/// bundled (what is actually drawn), else its IR bounding box.
fn drawn_bbox(g: &ResolvedGlyph) -> BoundingBox {
    match outline(g.glyph.as_str()) {
        Some(o) => BoundingBox::new(o.bbox[0], o.bbox[1], o.bbox[2], o.bbox[3]),
        None => g.bounding_box,
    }
}

/// `data-*` provenance attributes tracing an element to its score-graph source.
fn provenance_attrs(p: &Provenance, glyph: &str, class: GlyphClass) -> String {
    format!(
        " data-prov=\"{:032x}\" data-source-kind=\"{}\" data-glyph=\"{}\" data-class=\"{}\"",
        p.stable_id.0,
        p.source.discriminant(),
        escape_attr(glyph),
        class.token(),
    )
}

/// Splits an `0xRRGGBBAA` colour into an SVG `fill` value and an optional
/// `fill-opacity` attribute (empty when fully opaque).
fn colour(rgba: u32) -> (String, String) {
    let r = (rgba >> 24) & 0xff;
    let g = (rgba >> 16) & 0xff;
    let b = (rgba >> 8) & 0xff;
    let a = rgba & 0xff;
    let fill = format!("#{r:02x}{g:02x}{b:02x}");
    let opacity = if a == 0xff {
        String::new()
    } else {
        format!(" fill-opacity=\"{}\"", num(a as f32 / 255.0))
    };
    (fill, opacity)
}

/// A minimal, valid empty SVG for a layout with nothing to draw.
fn empty_svg() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
     <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" viewBox=\"0 0 1 1\">\n\
     \x20\x20<!-- epiphany-render-svg: empty resolved layout (no glyphs) -->\n\
     </svg>\n"
        .to_owned()
}

/// Formats a coordinate to at most 4 decimals, trimming trailing zeros and
/// normalising `-0` to `0` for deterministic, compact output.
fn num(v: f32) -> String {
    let v = num_f(v);
    let mut s = format!("{v:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

/// Normalises a coordinate value: `-0.0` and tiny `±0` round to `0.0`; other
/// values pass through. Keeps the formatted form and the stored stats agreeing.
fn num_f(v: f32) -> f32 {
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{
        to_constrained, to_logical, ConstraintSolver, SolverConfig, StubSolver,
    };

    fn stub_layout(seed: u64) -> ResolvedLayoutIR {
        let constrained = to_constrained(&to_logical(&valid_score_rich(seed)));
        StubSolver
            .solve(&constrained, &SolverConfig::default())
            .layout
    }

    #[test]
    fn renders_well_formed_svg_with_one_path_per_glyph() {
        let layout = stub_layout(11);
        let out = render(&layout, &RenderOptions::default());
        assert!(out.is_well_formed(), "emitted SVG must be well-formed");
        assert_eq!(out.stats.glyph_count, layout.glyphs.len());
        // The stub pipeline only names bundled glyphs, so every glyph is a path.
        assert_eq!(out.stats.path_count, layout.glyphs.len());
        assert_eq!(out.stats.fallback_rect_count, 0);
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.stats.provenance_count, layout.glyphs.len());
        assert!(out.svg.contains("<svg"));
        assert!(out.svg.contains("data-prov="));
    }

    #[test]
    fn render_is_deterministic() {
        let layout = stub_layout(3);
        assert_eq!(
            render(&layout, &RenderOptions::default()).svg,
            render(&layout, &RenderOptions::default()).svg
        );
    }

    #[test]
    fn empty_layout_renders_a_valid_empty_canvas() {
        let layout = ResolvedLayoutIR {
            source: Default::default(),
            pages: vec![],
            glyphs: vec![],
            engraving_decisions: vec![],
            catalog: Default::default(),
        };
        let out = render(&layout, &RenderOptions::default());
        assert!(out.is_well_formed());
        assert_eq!(out.stats.glyph_count, 0);
        assert_eq!(out.stats.path_count, 0);
    }

    #[test]
    fn provenance_can_be_suppressed() {
        let layout = stub_layout(5);
        let out = render(
            &layout,
            &RenderOptions {
                emit_provenance: false,
                ..RenderOptions::default()
            },
        );
        assert_eq!(out.stats.provenance_count, 0);
        assert!(!out.svg.contains("data-prov="));
        assert!(out.is_well_formed());
    }

    #[test]
    fn classes_partition_the_glyphs() {
        let layout = stub_layout(7);
        let out = render(&layout, &RenderOptions::default());
        let total: usize = out.stats.class_counts.values().sum();
        assert_eq!(total, layout.glyphs.len());
    }
}
