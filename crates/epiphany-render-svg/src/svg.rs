//! The SVG renderer: a [`ResolvedLayoutIR`] to well-formed SVG 1.1.
//!
//! ## What this renderer is, and is not
//!
//! It is a **renderer**, not an engraver. Per the QUICKSTART non-overreach rule
//! (`spec/PHASE2_QUICKSTART.md`, Agent I), every emitted SVG element traces to a
//! [`ResolvedGlyph`] (and thus to its score-graph source) or to a declared
//! renderer wrapper (the `<svg>` root, a metadata comment, the y-flip group, a
//! per-layer `<g>`). Provenance traces are on by default; turning them off (when
//! [`RenderOptions::emit_provenance`] is `false`) is an explicit display-only mode
//! that the metadata comment *declares*, so a trace-free SVG announces itself
//! rather than passing as an archival one. The renderer makes *SVG-encoding*
//! choices only — grouping,
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

use epiphany_layout_ir::{
    BoundingBox, LineStyle, Provenance, ResolvedGlyph, ResolvedLayoutIR, Transform2D,
};

use crate::font_subset_generated::{
    BRAVURA_SUBSET_FAMILY, BRAVURA_SUBSET_MIME, BRAVURA_SUBSET_OTF_BASE64,
};
use crate::outline::{outline, smufl_codepoint};
use crate::xml::{check_well_formed, escape_attr};

/// How glyphs are drawn.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum GlyphMode {
    /// Inline genuine Bravura outline `<path>`s (default). Self-contained: the
    /// SVG renders in any viewer with no font dependency (QUICKSTART, Agent I:
    /// "inline path outlines for golden fixtures and the demonstrable
    /// deliverable"). This is the byte-golden-locked, pixel-verified reference
    /// mode.
    #[default]
    PathOutline,
    /// Reference each glyph by its SMuFL codepoint with a `<text>` element, drawn
    /// from an `@font-face`-embedded subset of Bravura (the same SHA-pinned font
    /// the outlines come from, base64 in `font_subset_generated`). The result is
    /// self-contained — the font travels in the SVG — and text-selectable, at the
    /// cost of a larger file. Glyph placement is consistent with
    /// [`GlyphMode::PathOutline`] by construction (same origin, em = 4 staff
    /// spaces); exact glyph rasterisation is then the consumer's font renderer's.
    EmbeddedFont,
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
    /// `true` (the default) is the archival/traceable mode; `false` is an explicit
    /// display-only mode that the emitted SVG's metadata comment *declares*, so the
    /// absence of traces is announced rather than silently produced.
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
    Repeat,
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
        } else if name.starts_with("repeat") {
            GlyphClass::Repeat
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
            GlyphClass::Repeat => "repeat",
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
    /// `<path>` elements emitted (glyphs drawn from a bundled outline, the
    /// default [`GlyphMode::PathOutline`]).
    pub path_count: usize,
    /// `<text>` elements emitted (glyphs set in the embedded font, the
    /// [`GlyphMode::EmbeddedFont`] mode). Zero in the default path mode.
    pub text_count: usize,
    /// Fallback `<rect>` elements emitted (glyphs with no bundled outline).
    pub fallback_rect_count: usize,
    /// `<line>` elements emitted (one per resolved stroke: staff line, stem, …).
    pub stroke_count: usize,
    /// `<path>` curve elements emitted (one per resolved curve: slur, …).
    pub curve_count: usize,
    /// Elements carrying a `data-prov` trace back to a score-graph source.
    pub provenance_count: usize,
    /// Distinct layers, each rendered as one `<g>` group.
    pub layer_count: usize,
    /// Per-class glyph counts.
    pub class_counts: BTreeMap<GlyphClass, usize>,
    /// The padded content bounds `[min_x, min_y, width, height]`, in staff
    /// spaces. Note this is the *content extent*, not the emitted `viewBox`
    /// attribute: the SVG is translated so its `viewBox` is always `0 0 W H`
    /// (the `min_x`/`min_y` here are folded into the y-flip group's translate).
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
            let svg = empty_svg(options.glyph_mode, options.emit_provenance);
            let well_formed = check_well_formed(&svg).is_ok();
            debug_assert!(well_formed);
            return RenderOutput {
                stats: RenderStats {
                    glyph_count: 0,
                    path_count: 0,
                    text_count: 0,
                    fallback_rect_count: 0,
                    stroke_count: 0,
                    curve_count: 0,
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

    // Group glyphs and strokes by layer (ascending), preserving input order
    // within. Strokes draw before glyphs at the same layer, so a staff line sits
    // under the noteheads on it.
    let mut glyph_layers: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, g) in resolved.glyphs.iter().enumerate() {
        glyph_layers.entry(g.layer).or_default().push(i);
    }
    let mut stroke_layers: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, stroke) in resolved.strokes.iter().enumerate() {
        stroke_layers.entry(stroke.layer).or_default().push(i);
    }
    let mut curve_layers: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, curve) in resolved.curves.iter().enumerate() {
        curve_layers.entry(curve.layer).or_default().push(i);
    }
    let layer_ids: std::collections::BTreeSet<i32> = glyph_layers
        .keys()
        .chain(stroke_layers.keys())
        .chain(curve_layers.keys())
        .copied()
        .collect();

    let mut path_count = 0;
    let mut text_count = 0;
    let mut fallback_rect_count = 0;
    let mut stroke_count = 0;
    let mut curve_count = 0;
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
    // Declared metadata wrapper (a comment — honest about what this is, including
    // how glyphs are drawn and whether provenance traces are present: suppressing
    // them is an explicit display-only choice the output announces, not drops).
    let _ = writeln!(
        s,
        "  <!-- epiphany-render-svg: {}; geometry is the resolved layout \
         verbatim, no engraving performed here; {} -->",
        glyph_note(options.glyph_mode),
        provenance_note(options.emit_provenance),
    );
    // In embedded-font mode, declare the Bravura subset once via `@font-face`; the
    // `<text>` glyphs below reference it by its (non-reserved) family name (see the
    // font subset's own header for its provenance and the OFL terms it carries).
    if options.glyph_mode == GlyphMode::EmbeddedFont {
        let _ = writeln!(
            s,
            "  <defs><style>@font-face {{ font-family: \"{}\"; \
             src: url(\"data:{};base64,{}\") format(\"opentype\"); }}</style></defs>",
            BRAVURA_SUBSET_FAMILY, BRAVURA_SUBSET_MIME, BRAVURA_SUBSET_OTF_BASE64,
        );
    }
    // The single y-flip wrapper: staff-space/y-up world -> SVG y-down.
    let _ = writeln!(
        s,
        "  <g transform=\"translate({} {}) scale(1 -1)\">",
        num(-min_x),
        num(max_y),
    );

    for layer in &layer_ids {
        let _ = writeln!(s, "    <g data-layer=\"{}\">", layer);

        // Strokes (staff lines, stems, barlines, …) — drawn first so glyphs on
        // the same layer sit over them.
        if let Some(indices) = stroke_layers.get(layer) {
            for &i in indices {
                let stroke = &resolved.strokes[i];
                let (stroke_fill, opacity) = stroke_colour(stroke.style.rgba);
                let prov = if options.emit_provenance {
                    provenance_count += 1;
                    stroke_provenance_attrs(&stroke.provenance)
                } else {
                    String::new()
                };
                stroke_count += 1;
                let _ = writeln!(
                    s,
                    "      <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" \
                     stroke=\"{}\" stroke-width=\"{}\"{}{}/>",
                    num(stroke.from.x.0),
                    num(stroke.from.y.0),
                    num(stroke.to.x.0),
                    num(stroke.to.y.0),
                    stroke_fill,
                    num(stroke.thickness.0),
                    opacity,
                    prov,
                );
            }
        }

        // Curves (slurs, …) — drawn after strokes, before glyphs, as a stroked
        // (unfilled) cubic-bézier `<path>`.
        if let Some(indices) = curve_layers.get(layer) {
            for &i in indices {
                let curve = &resolved.curves[i];
                let (stroke_fill, opacity) = stroke_colour(curve.style.rgba);
                let prov = if options.emit_provenance {
                    provenance_count += 1;
                    curve_provenance_attrs(&curve.provenance)
                } else {
                    String::new()
                };
                curve_count += 1;
                let _ = writeln!(
                    s,
                    "      <path d=\"M {} {} C {} {} {} {} {} {}\" \
                     fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{}{}{}/>",
                    num(curve.p0.x.0),
                    num(curve.p0.y.0),
                    num(curve.p1.x.0),
                    num(curve.p1.y.0),
                    num(curve.p2.x.0),
                    num(curve.p2.y.0),
                    num(curve.p3.x.0),
                    num(curve.p3.y.0),
                    stroke_fill,
                    num(curve.thickness.0),
                    dash_attrs(curve.line),
                    opacity,
                    prov,
                );
            }
        }

        if let Some(indices) = glyph_layers.get(layer) {
            for &i in indices {
                let g = &resolved.glyphs[i];
                let name = g.glyph.as_str();
                let (x, y) = (g.position.x.0, g.position.y.0);
                // The glyph's resolved transform (scale/rotate/skew about its
                // origin), composed after the placement translate. `None` ⇒ a bare
                // translate, the common case.
                let placement = placement_transform(x, y, &g.transform);
                if let Some(t) = &g.transform {
                    if !is_affine(t) {
                        diagnostics.push(Diagnostic {
                            message: "non-affine (projective) glyph transform is not \
                                      representable in SVG; rendered its affine projection"
                                .to_owned(),
                            glyph: Some(name.to_owned()),
                        });
                    }
                }
                let (fill, opacity) = colour(g.style.rgba);
                let prov = if options.emit_provenance {
                    provenance_count += 1;
                    provenance_attrs(&g.provenance, name, GlyphClass::of(name))
                } else {
                    String::new()
                };
                // The drawn element depends on the mode: an inline outline `<path>`
                // (the self-contained default) or a `<text>` referencing the embedded
                // Bravura by SMuFL codepoint. Both anchor at the same `(x, y)` origin,
                // so the two modes are geometrically consistent. `None` (no bundled
                // outline / no codepoint) falls through to the visible bbox rect.
                let element = match options.glyph_mode {
                    GlyphMode::PathOutline => outline(name).map(|o| {
                        path_count += 1;
                        format!(
                            "<path d=\"{}\" transform=\"{}\" fill=\"{}\"{}{}/>",
                            o.path, placement, fill, opacity, prov,
                        )
                    }),
                    GlyphMode::EmbeddedFont => smufl_codepoint(name).map(|cp| {
                        text_count += 1;
                        // The font glyph is drawn upright by a per-glyph counter-flip
                        // (`scale(1 -1)`, innermost) cancelling the outer y-flip; the
                        // em is four staff spaces (SMuFL), so `font-size="4"`.
                        format!(
                            "<text transform=\"{placement} scale(1 -1)\" \
                             font-family=\"{BRAVURA_SUBSET_FAMILY}\" font-size=\"4\" \
                             fill=\"{fill}\"{opacity}{prov}>&#x{cp:X};</text>",
                        )
                    }),
                };
                match element {
                    Some(el) => {
                        let _ = writeln!(s, "      {el}");
                    }
                    None => {
                        // Unrenderable in this mode: surface it and draw the IR
                        // bounding box so the missing glyph is visible, not silent.
                        fallback_rect_count += 1;
                        diagnostics.push(Diagnostic {
                            message: "no bundled Bravura glyph for this name; drew \
                                      bounding-box fallback"
                                .to_owned(),
                            glyph: Some(name.to_owned()),
                        });
                        let bb = g.bounding_box;
                        let _ = writeln!(
                            s,
                            "      <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
                             transform=\"{}\" fill=\"none\" stroke=\"#cc0000\" \
                             stroke-width=\"0.05\"{}/>",
                            num(bb.left.0),
                            num(bb.bottom.0),
                            num((bb.right.0 - bb.left.0).max(0.0)),
                            num((bb.top.0 - bb.bottom.0).max(0.0)),
                            placement,
                            prov,
                        );
                    }
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
            text_count,
            fallback_rect_count,
            stroke_count,
            curve_count,
            provenance_count,
            layer_count: layer_ids.len(),
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
        // All four bbox corners mapped through the *same* placement transform the
        // renderer applies — a scale/rotation can push drawn geometry past the
        // untransformed axis-aligned extent, which would crop it.
        for (lx, ly) in [
            (bb.left.0, bb.bottom.0),
            (bb.right.0, bb.bottom.0),
            (bb.left.0, bb.top.0),
            (bb.right.0, bb.top.0),
        ] {
            let (px, py) = placed_point(x, y, &g.transform, lx, ly);
            if px.is_finite() && py.is_finite() {
                any = true;
                min_x = min_x.min(px);
                min_y = min_y.min(py);
                max_x = max_x.max(px);
                max_y = max_y.max(py);
            }
        }
    }
    // Strokes extend the bounds by a half-thickness box around each endpoint, so a
    // thick rule is not clipped perpendicular to its direction even at margin 0.
    for stroke in &resolved.strokes {
        let half = (stroke.thickness.0 * 0.5).max(0.0);
        for point in [stroke.from, stroke.to] {
            let (cx, cy) = (point.x.0, point.y.0);
            for (px, py) in [(cx - half, cy - half), (cx + half, cy + half)] {
                if px.is_finite() && py.is_finite() {
                    any = true;
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px);
                    max_y = max_y.max(py);
                }
            }
        }
    }
    // A cubic bézier's ink stays within its control points' convex hull, so
    // bounding by the four control points (± half-thickness) is correct and
    // conservative — the drawn arc never bows past it.
    for curve in &resolved.curves {
        let half = (curve.thickness.0 * 0.5).max(0.0);
        for point in curve.control_points() {
            let (cx, cy) = (point.x.0, point.y.0);
            for (px, py) in [(cx - half, cy - half), (cx + half, cy + half)] {
                if px.is_finite() && py.is_finite() {
                    any = true;
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px);
                    max_y = max_y.max(py);
                }
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

/// `data-*` provenance attributes for a stroke (a non-glyph line primitive).
fn stroke_provenance_attrs(p: &Provenance) -> String {
    format!(
        " data-prov=\"{:032x}\" data-source-kind=\"{}\" data-kind=\"stroke\"",
        p.stable_id.0,
        p.source.discriminant(),
    )
}

/// The `stroke-dasharray` (and, for dotted, `stroke-linecap`) attribute for a
/// curve's line pattern, in staff-space units (the viewBox is staff-space). A
/// solid line adds nothing. Dashed is a dash/gap pair; dotted is round-capped
/// zero-length dashes, drawing round dots of the stroke's own width.
fn dash_attrs(line: LineStyle) -> &'static str {
    match line {
        LineStyle::Solid => "",
        LineStyle::Dashed => " stroke-dasharray=\"0.5 0.35\"",
        LineStyle::Dotted => " stroke-dasharray=\"0 0.28\" stroke-linecap=\"round\"",
    }
}

/// `data-*` provenance attributes for a curve (a cubic-bézier primitive).
fn curve_provenance_attrs(p: &Provenance) -> String {
    format!(
        " data-prov=\"{:032x}\" data-source-kind=\"{}\" data-kind=\"curve\"",
        p.stable_id.0,
        p.source.discriminant(),
    )
}

/// Whether a [`Transform2D`] is a pure 2-D affine — its bottom row is `[0, 0, 1]`
/// (within an `f32` tolerance). SVG transforms are affine, so a non-affine
/// (projective) transform cannot be represented; the renderer diagnoses it and
/// renders its affine projection.
fn is_affine(transform: &Transform2D) -> bool {
    let bottom = transform.matrix[2];
    bottom[0].abs() < 1e-6 && bottom[1].abs() < 1e-6 && (bottom[2] - 1.0).abs() < 1e-6
}

/// A glyph-local point mapped to world space through the renderer's placement
/// transform — the glyph's resolved affine applied about the origin, then
/// translated to `(px, py)`. Matches [`placement_transform`]'s SVG output (it
/// uses the affine projection, dropping any projective bottom row).
fn placed_point(px: f32, py: f32, transform: &Option<Transform2D>, lx: f32, ly: f32) -> (f32, f32) {
    let (tx, ty) = match transform {
        None => (lx, ly),
        Some(t) => {
            let m = t.matrix;
            (
                m[0][0] * lx + m[0][1] * ly + m[0][2],
                m[1][0] * lx + m[1][1] * ly + m[1][2],
            )
        }
    };
    (px + tx, py + ty)
}

/// The SVG `transform` placing a glyph at `(x, y)` and applying its resolved
/// affine (`scale`/`rotate`/`skew` about the glyph origin) when present. The
/// affine is the inner (rightmost) transform, so the glyph's local outline is
/// transformed, then translated into place. `None` is the common case: a bare
/// translate, byte-identical to the pre-transform output. A non-affine transform
/// is rendered as its affine projection (the bottom row is dropped); the render
/// loop emits a diagnostic for that case.
fn placement_transform(x: f32, y: f32, transform: &Option<Transform2D>) -> String {
    match transform {
        None => format!("translate({} {})", num(x), num(y)),
        Some(t) => {
            let m = t.matrix;
            // SVG matrix(a b c d e f) is the affine [[a c e],[b d f],[0 0 1]];
            // map it from the row-major 3×3 (the bottom row is implicit).
            format!(
                "translate({} {}) matrix({} {} {} {} {} {})",
                num(x),
                num(y),
                num(m[0][0]),
                num(m[1][0]),
                num(m[0][1]),
                num(m[1][1]),
                num(m[0][2]),
                num(m[1][2]),
            )
        }
    }
}

/// Splits an `0xRRGGBBAA` colour into an SVG `stroke` value and an optional
/// `stroke-opacity` attribute (empty when fully opaque).
fn stroke_colour(rgba: u32) -> (String, String) {
    let r = (rgba >> 24) & 0xff;
    let g = (rgba >> 16) & 0xff;
    let b = (rgba >> 8) & 0xff;
    let a = rgba & 0xff;
    let stroke = format!("#{r:02x}{g:02x}{b:02x}");
    let opacity = if a == 0xff {
        String::new()
    } else {
        format!(" stroke-opacity=\"{}\"", num(a as f32 / 255.0))
    };
    (stroke, opacity)
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

/// The glyph-mode clause of the metadata comment: which drawing strategy produced
/// the SVG. Shared by the main render and [`empty_svg`] so the declared mode
/// boundary is the same on the empty path.
fn glyph_note(mode: GlyphMode) -> &'static str {
    match mode {
        GlyphMode::PathOutline => "glyphs are genuine Bravura SMuFL outlines inlined as paths",
        GlyphMode::EmbeddedFont => "glyphs are Bravura SMuFL codepoints set in the embedded font",
    }
}

/// The provenance-state clause of the metadata comment. Archival mode declares
/// traces present; display-only mode declares them suppressed — so a trace-free
/// SVG (including the empty canvas) announces itself rather than passing as
/// archival. Shared by the main render and [`empty_svg`] so neither can drift.
fn provenance_note(emit_provenance: bool) -> &'static str {
    if emit_provenance {
        "every glyph, stroke, and curve carries a data-prov trace to its score-graph source"
    } else {
        "provenance traces suppressed (display-only output, not archival)"
    }
}

/// A minimal, valid empty SVG for a layout with nothing to draw — still declaring
/// its glyph mode and provenance state, so an empty render is honest like a full
/// one (the metadata is the same declared boundary on both paths).
fn empty_svg(glyph_mode: GlyphMode, emit_provenance: bool) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" viewBox=\"0 0 1 1\">\n\
         \x20\x20<!-- epiphany-render-svg: empty resolved layout (no glyphs); {}; {} -->\n\
         </svg>\n",
        glyph_note(glyph_mode),
        provenance_note(emit_provenance),
    )
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

/// Normalises a coordinate value: `-0.0` (which compares equal to `0.0`) maps to
/// `0.0`; every other value passes through unchanged. Keeps the formatted form
/// and the stored stats agreeing.
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
        to_constrained, to_logical, ConstraintSolver, GlyphStyle, Point, SolverConfig, StaffSpace,
        Stroke, StubSolver, Transform2D,
    };

    fn stub_layout(seed: u64) -> ResolvedLayoutIR {
        let constrained = to_constrained(&to_logical(&valid_score_rich(seed)));
        StubSolver
            .solve(&constrained, &SolverConfig::default())
            .layout
    }

    #[test]
    fn a_stroke_renders_as_a_traced_line() {
        let mut layout = stub_layout(11);
        let glyph_count = layout.glyphs.len();
        // The engraver already emits strokes (staff lines, stems, …); this adds
        // one more and checks it renders and traces on top of them.
        let base_strokes = layout.strokes.len();
        layout.strokes.push(Stroke {
            provenance: layout.glyphs[0].provenance.clone(),
            from: Point::new(0.0, 0.0),
            to: Point::new(4.0, 0.0),
            thickness: StaffSpace(0.13),
            layer: -1,
            style: GlyphStyle { rgba: 0x0000_00ff },
        });
        let out = render(&layout, &RenderOptions::default());
        assert!(
            out.is_well_formed(),
            "SVG with a stroke must be well-formed"
        );
        assert_eq!(out.stats.stroke_count, base_strokes + 1);
        assert!(
            out.svg.contains("<line "),
            "the stroke is drawn as a <line>"
        );
        assert!(
            out.svg.contains("data-kind=\"stroke\""),
            "the stroke carries a provenance trace"
        );
        assert_eq!(
            out.stats.provenance_count,
            glyph_count + base_strokes + 1,
            "every glyph and stroke is traced"
        );
    }

    #[test]
    fn a_curve_renders_as_a_path_and_a_dashed_curve_carries_a_dasharray() {
        use epiphany_layout_ir::Curve;
        let mut layout = stub_layout(11);
        let prov = layout.glyphs[0].provenance.clone();
        let make = |line| Curve {
            provenance: prov.clone(),
            p0: Point::new(0.0, 0.0),
            p1: Point::new(1.0, 2.0),
            p2: Point::new(3.0, 2.0),
            p3: Point::new(4.0, 0.0),
            thickness: StaffSpace(0.12),
            layer: 1,
            style: GlyphStyle { rgba: 0x0000_00ff },
            line,
        };
        // A solid curve: a stroked, unfilled <path> with no dasharray.
        layout.curves = vec![make(LineStyle::Solid)];
        let solid = render(&layout, &RenderOptions::default());
        assert!(solid.is_well_formed());
        assert_eq!(solid.stats.curve_count, 1);
        assert!(solid.svg.contains("<path d=\"M 0 0 C"));
        assert!(solid.svg.contains("data-kind=\"curve\""));
        assert!(
            !solid.svg.contains("stroke-dasharray"),
            "a solid curve has no dasharray"
        );
        // A dashed curve carries stroke-dasharray; a dotted one round-caps.
        layout.curves = vec![make(LineStyle::Dashed)];
        assert!(render(&layout, &RenderOptions::default())
            .svg
            .contains("stroke-dasharray=\"0.5 0.35\""));
        layout.curves = vec![make(LineStyle::Dotted)];
        let dotted = render(&layout, &RenderOptions::default()).svg;
        assert!(dotted.contains("stroke-dasharray=\"0 0.28\""));
        assert!(dotted.contains("stroke-linecap=\"round\""));
    }

    #[test]
    fn a_glyph_transform_is_applied_not_dropped() {
        let mut layout = stub_layout(11);
        // A 2× scale about the glyph origin: the renderer must emit it, not ignore
        // it (otherwise a future solver's transform would silently disappear).
        layout.glyphs[0].transform = Some(Transform2D {
            matrix: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 1.0]],
        });
        let out = render(&layout, &RenderOptions::default());
        assert!(out.is_well_formed());
        assert!(
            out.svg.contains("matrix(2 0 0 2 0 0)"),
            "the glyph's resolved transform is applied"
        );
    }

    #[test]
    fn glyph_transform_expands_the_view_box() {
        let base = stub_layout(11);
        let base_width = render(&base, &RenderOptions::default()).stats.view_box[2];
        let mut transformed = base.clone();
        // Translate one glyph far to the right via its transform; the bounds must
        // grow to contain it (untransformed bounds would crop it).
        transformed.glyphs[0].transform = Some(Transform2D {
            matrix: [[1.0, 0.0, 1000.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        });
        let width = render(&transformed, &RenderOptions::default())
            .stats
            .view_box[2];
        assert!(
            width > base_width + 900.0,
            "a transformed glyph widens the viewBox ({width} vs {base_width})"
        );
    }

    #[test]
    fn thick_stroke_expands_bounds_by_half_width() {
        let mut layout = stub_layout(11);
        let provenance = layout.glyphs[0].provenance.clone();
        layout.glyphs.clear();
        layout.strokes.push(Stroke {
            provenance,
            from: Point::new(0.0, 0.0),
            to: Point::new(4.0, 0.0),
            thickness: StaffSpace(2.0),
            layer: 0,
            style: GlyphStyle::default(),
        });
        let options = RenderOptions {
            margin: 0.0,
            ..RenderOptions::default()
        };
        let height = render(&layout, &options).stats.view_box[3];
        // A horizontal rule of thickness 2 spans y ∈ [−1, 1]: a half-width each
        // side, so the perpendicular extent is at least the full thickness.
        assert!(
            height >= 2.0,
            "half-thickness expands the perpendicular extent (got {height})"
        );
    }

    #[test]
    fn non_affine_transform_is_diagnosed() {
        let mut layout = stub_layout(11);
        // A non-zero bottom row makes the transform projective, not affine.
        layout.glyphs[0].transform = Some(Transform2D {
            matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.0, 1.0]],
        });
        let out = render(&layout, &RenderOptions::default());
        assert!(out.is_well_formed());
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.message.contains("non-affine")),
            "a projective transform is surfaced as a diagnostic, not silently mis-rendered"
        );
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
        assert_eq!(
            out.stats.provenance_count,
            layout.glyphs.len() + layout.strokes.len()
        );
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
    fn embedded_font_mode_sets_text_from_the_embedded_subset() {
        let layout = stub_layout(11);
        let out = render(
            &layout,
            &RenderOptions {
                glyph_mode: GlyphMode::EmbeddedFont,
                ..RenderOptions::default()
            },
        );
        assert!(
            out.is_well_formed(),
            "embedded-font SVG must be well-formed"
        );

        // The font is declared exactly once via @font-face with the base64 subset,
        // under its non-reserved family name (OFL: not the Reserved Font Name).
        assert_eq!(out.svg.matches("@font-face").count(), 1);
        assert!(out.svg.contains("font-family: \"EpiphanyBravuraSubset\""));
        assert!(!out.svg.contains("font-family: \"Bravura\""));
        assert!(out.svg.contains("data:font/otf;base64,"));

        // Every glyph is a `<text>` (the stub names only bundled glyphs), none a
        // path or a fallback rect, and each carries a SMuFL codepoint reference.
        assert_eq!(out.stats.text_count, layout.glyphs.len());
        assert_eq!(out.stats.path_count, 0);
        assert_eq!(out.stats.fallback_rect_count, 0);
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.svg.matches("<text ").count(), layout.glyphs.len());
        assert_eq!(out.svg.matches("&#x").count(), layout.glyphs.len());

        // The metadata comment declares the embedded-font mode, and provenance is
        // preserved exactly as in path mode.
        assert!(out.svg.contains("set in the embedded font"));
        assert_eq!(
            out.stats.provenance_count,
            layout.glyphs.len() + layout.strokes.len()
        );
        assert!(out.svg.contains("data-prov="));
    }

    #[test]
    fn embedded_font_mode_is_deterministic_and_draws_every_glyph() {
        let layout = stub_layout(7);
        let a = render(
            &layout,
            &RenderOptions {
                glyph_mode: GlyphMode::EmbeddedFont,
                ..RenderOptions::default()
            },
        );
        let b = render(
            &layout,
            &RenderOptions {
                glyph_mode: GlyphMode::EmbeddedFont,
                ..RenderOptions::default()
            },
        );
        assert_eq!(a.svg, b.svg, "embedded-font render must be deterministic");
        // Every glyph is accounted for (text or fallback rect), none dropped — the
        // same no-silent-drop contract as path mode.
        assert_eq!(
            a.stats.text_count + a.stats.fallback_rect_count,
            a.stats.glyph_count
        );
        // The default path mode draws no `<text>`; the modes do not bleed.
        let path = render(&layout, &RenderOptions::default());
        assert_eq!(path.stats.text_count, 0);
        assert!(!path.svg.contains("@font-face"));
    }

    #[test]
    fn empty_layout_renders_a_valid_empty_canvas() {
        let layout = ResolvedLayoutIR {
            source: Default::default(),
            pages: vec![],
            glyphs: vec![],
            strokes: vec![],
            curves: vec![],
            engraving_decisions: vec![],
            catalog: Default::default(),
        };
        let out = render(&layout, &RenderOptions::default());
        assert!(out.is_well_formed());
        assert_eq!(out.stats.glyph_count, 0);
        assert_eq!(out.stats.path_count, 0);

        // Even with nothing to draw, suppressing provenance is declared display-only
        // — the empty canvas is held to the same honesty contract as a full render.
        let suppressed = render(
            &layout,
            &RenderOptions {
                emit_provenance: false,
                ..RenderOptions::default()
            },
        );
        assert!(suppressed.is_well_formed());
        assert!(suppressed.svg.contains("provenance traces suppressed"));

        // The empty canvas also declares its glyph mode (the same boundary as a
        // full render), so an empty embedded render is not mistaken for a path one.
        let empty_embedded = render(
            &layout,
            &RenderOptions {
                glyph_mode: GlyphMode::EmbeddedFont,
                ..RenderOptions::default()
            },
        );
        assert!(empty_embedded.svg.contains("set in the embedded font"));
    }

    #[test]
    fn provenance_can_be_suppressed_but_is_declared() {
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
        // Suppression is announced, not silent: the metadata comment declares the
        // output display-only, so it can't be mistaken for a traceable/archival one.
        assert!(
            out.svg.contains("provenance traces suppressed"),
            "a trace-free SVG must declare itself display-only"
        );

        // The default (archival) render instead declares that traces are present.
        let archival = render(&layout, &RenderOptions::default());
        assert!(archival.svg.contains("data-prov trace"));
        assert!(!archival.svg.contains("provenance traces suppressed"));
    }

    #[test]
    fn classes_partition_the_glyphs() {
        let layout = stub_layout(7);
        let out = render(&layout, &RenderOptions::default());
        let total: usize = out.stats.class_counts.values().sum();
        assert_eq!(total, layout.glyphs.len());
    }
}
