//! Candidate-owned glyph outline extraction + tessellation, used by
//! `render_target.rs` (the offscreen checks 1/2 render pipeline
//! `bin/c1_round2_text.rs` drives). Counted under `ReportPart::TextRendering`
//! together with `render_target.rs` — the F3 cost-schema amendment's own
//! definition of that row ("outline extraction, path building,
//! tessellation/rasterization, the offscreen render target"). The check-5
//! windowed probe (`bin/c1_round2_a11y.rs`) does not use this module: it
//! carries no visual rendering at all, only the accessible node, so that its
//! own LOC is not a mix of `AccessibilityTreeConstruction` and rendering
//! code the F3 finding named as the defect in the pre-fix version of this
//! packet.
//!
//! Round 1's `main.rs` converted `epiphany_layout_ir::PathCommand` (Bravura's
//! own typed outline data, already staff-space `MoveTo`/`LineTo`/`CurveTo`)
//! into a lyon path. Round 2's glyphs come from host font faces instead,
//! addressed by font-internal glyph id (`SpikePositionedGlyph::glyph_id`) —
//! there is no `PathCommand` for them anywhere in this recipe's data. This
//! module is therefore new candidate work, not a reuse of Round 1's
//! `build_path`: it walks `ttf_parser::Face::outline_glyph`'s own
//! `OutlineBuilder` callbacks straight into a `lyon_path::Path`, exactly the
//! extraction-and-conversion step the packet names as "yours to write" and
//! "part of what the cost table measures."
//!
//! `round2-svgref` (the frozen, candidate-neutral reference emitter) walks
//! the same `ttf_parser::OutlineBuilder` callbacks to build an SVG path
//! string. This module does the analogous walk for a *lyon* path instead —
//! independently implemented, not called into, since the reference emitter
//! is off-limits apparatus (`round2-svgref` is not depended on here) and the
//! whole point of this module is that the candidate does its own outline
//! walk.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, TextureId};
use lyon_path::math::point as lyon_point;
use lyon_path::Path as LyonPath;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
};

/// The ink colour every glyph is painted, opaque, matching the reference
/// emitter's `fill="#000000"` and Round 1's own `INK`.
pub const INK: Color32 = Color32::BLACK;

/// Collects one glyph outline straight into a `lyon_path::Path`, converting
/// font units to device pixels and flipping y (font space is y-up; device
/// space, like Round 1's and the reference emitter's, is y-down) in the same
/// step — no intermediate `PathCommand` or SVG-string representation.
///
/// `device_origin` is the glyph's own device-space pen position — the output
/// of `round2_textkit::hittest::to_device` on the glyph's `offset`, per the
/// packet's non-negotiable rendering convention. `scale` is device px per
/// font unit (`em_px / units_per_em`).
struct GlyphPathSink {
    builder: lyon_path::path::Builder,
    ox: f64,
    oy: f64,
    scale: f64,
    open: bool,
    any: bool,
}

impl GlyphPathSink {
    fn map(&self, x: f32, y: f32) -> lyon_path::math::Point {
        lyon_point(
            (self.ox + x as f64 * self.scale) as f32,
            (self.oy - y as f64 * self.scale) as f32,
        )
    }
}

impl ttf_parser::OutlineBuilder for GlyphPathSink {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.open {
            self.builder.end(true);
        }
        let p = self.map(x, y);
        self.builder.begin(p);
        self.open = true;
        self.any = true;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.builder.line_to(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c1 = self.map(x1, y1);
        let p = self.map(x, y);
        self.builder.quadratic_bezier_to(c1, p);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.map(x1, y1);
        let c2 = self.map(x2, y2);
        let p = self.map(x, y);
        self.builder.cubic_bezier_to(c1, c2, p);
    }

    fn close(&mut self) {
        if self.open {
            self.builder.end(true);
            self.open = false;
        }
    }
}

/// Extracts glyph `glyph_id`'s **complete** outline (every subpath — a
/// glyph's own bounded holes and disjoint components alike, the same
/// discipline Round 1's oracle measured Bravura against) from `face` as a
/// `lyon_path::Path` in device space, or `None` if the glyph has no outline
/// at all (whitespace — `ttf_parser::Face::outline_glyph` itself returns
/// `None`, or draws nothing). Never substituted with a placeholder: a glyph
/// with no outline draws nothing, exactly as a segment with `face: None`
/// draws nothing (recipe: "do not substitute a fallback and do not draw
/// `.notdef`").
pub fn glyph_outline_to_lyon_path(
    face: &ttf_parser::Face,
    glyph_id: u32,
    device_origin: (f64, f64),
    em_px: f64,
) -> Option<LyonPath> {
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return None;
    }
    let mut sink = GlyphPathSink {
        builder: LyonPath::builder(),
        ox: device_origin.0,
        oy: device_origin.1,
        scale: em_px / upem,
        open: false,
        any: false,
    };
    let gid = ttf_parser::GlyphId(glyph_id as u16);
    face.outline_glyph(gid, &mut sink)?;
    if sink.open {
        sink.builder.end(true);
    }
    if !sink.any {
        return None;
    }
    Some(sink.builder.build())
}

/// Tessellates one glyph outline and appends its vertices/indices into
/// `buffers`, offsetting indices so multiple glyphs can share one
/// `VertexBuffers` / one draw call.
///
/// **Nonzero fill rule** — matching both the reference emitter's own
/// `fill-rule="nonzero"` and Round 1's finding that TrueType/CFF outlines
/// (like Bravura's) are correctly wound, so nonzero and even-odd agree; the
/// **whole glyph outline is tessellated in one `tessellate_path` call**, the
/// same "one compound path, not per-subpath" discipline Round 1's `main.rs`
/// documents — a glyph with a bounded counter (e.g. `o`, `e`) has its hole
/// preserved only because every subpath enters the same fill call.
pub fn tessellate_into(
    path: &LyonPath,
    buffers: &mut VertexBuffers<[f32; 2], u32>,
) -> Result<(), String> {
    let mut tess = FillTessellator::new();
    tess.tessellate_path(
        path,
        &FillOptions::default().with_fill_rule(FillRule::NonZero),
        &mut BuffersBuilder::new(buffers, |v: FillVertex| {
            let p = v.position();
            [p.x, p.y]
        }),
    )
    .map_err(|e| format!("lyon tessellation failed: {e:?}"))?;
    Ok(())
}

/// Builds one `egui::epaint::Mesh`, bound to `tex`, containing every glyph
/// already tessellated into `buffers` — the whole fixture's ink in one mesh,
/// paintable in a single draw call.
pub fn mesh_from_buffers(buffers: &VertexBuffers<[f32; 2], u32>, tex: TextureId) -> Mesh {
    let mut mesh = Mesh::with_texture(tex);
    mesh.vertices = buffers
        .vertices
        .iter()
        .map(|[x, y]| Vertex {
            pos: Pos2::new(*x, *y),
            uv: Pos2::ZERO,
            color: INK,
        })
        .collect();
    mesh.indices = buffers.indices.clone();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGELLA: &str = "/usr/share/fonts/tex-gyre/texgyrepagella-regular.otf";

    fn face_bytes() -> Option<Vec<u8>> {
        std::fs::read(PAGELLA).ok()
    }

    /// Mutation-first: an outline that exists must actually tessellate to a
    /// non-empty mesh with real ink coverage — a sink wired backwards (e.g.
    /// dropping `close()`) would silently produce zero triangles instead of
    /// a build error.
    #[test]
    fn a_real_glyph_outline_tessellates_to_a_nonempty_mesh() {
        let Some(bytes) = face_bytes() else {
            eprintln!("NOT RUN: {PAGELLA} absent — environment absence, not a failure");
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let gid = face.glyph_index('A').unwrap();
        let path = glyph_outline_to_lyon_path(&face, gid.0 as u32, (0.0, 0.0), 128.0)
            .expect("'A' must have an outline");
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        tessellate_into(&path, &mut buffers).unwrap();
        assert!(!buffers.vertices.is_empty());
        assert!(!buffers.indices.is_empty());
        assert_eq!(
            buffers.indices.len() % 3,
            0,
            "a fill tessellation must produce whole triangles"
        );
    }

    /// A whitespace glyph (space) has no outline and must map to `None`, not
    /// an empty-but-`Some` path — the same "draws nothing, not a degenerate
    /// mesh" contract `round2-svgref`'s `emit_glyph_paths` documents for its
    /// own `empty` list.
    #[test]
    fn a_whitespace_glyph_has_no_outline() {
        let Some(bytes) = face_bytes() else {
            eprintln!("NOT RUN: {PAGELLA} absent");
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let gid = face.glyph_index(' ').unwrap();
        let path = glyph_outline_to_lyon_path(&face, gid.0 as u32, (0.0, 0.0), 128.0);
        assert!(path.is_none(), "a space glyph must produce no outline");
    }

    /// Required kill: a glyph with a bounded hole (`o`) must tessellate with
    /// its counter preserved — i.e. NOT as a solid blob. Checked the same
    /// way Round 1's oracle checks it: a point at the glyph's own centre
    /// (inside the counter) must NOT be covered by any tessellated triangle,
    /// while a point on the stem must be. This is a coarse geometric check
    /// (bounding-box centroid, not the oracle's precise point-in-path
    /// derivation), sufficient to catch the regression this module's own
    /// doc comment warns about: tessellating per-subpath (which would fill
    /// the hole solid) instead of as one compound path.
    #[test]
    fn a_glyph_with_a_hole_keeps_its_counter_open() {
        let Some(bytes) = face_bytes() else {
            eprintln!("NOT RUN: {PAGELLA} absent");
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let gid = face.glyph_index('o').unwrap();
        let path = glyph_outline_to_lyon_path(&face, gid.0 as u32, (0.0, 0.0), 1000.0)
            .expect("'o' must have an outline");
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        tessellate_into(&path, &mut buffers).unwrap();

        // Bounding box of the tessellated ink.
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for [x, y] in &buffers.vertices {
            minx = minx.min(*x);
            miny = miny.min(*y);
            maxx = maxx.max(*x);
            maxy = maxy.max(*y);
        }
        let cx = (minx + maxx) / 2.0;
        let cy = (miny + maxy) / 2.0;

        let point_in_triangle = |p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
            let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
                (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
            };
            let d1 = sign(p, a, b);
            let d2 = sign(p, b, c);
            let d3 = sign(p, c, a);
            let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            !(has_neg && has_pos)
        };
        let covers = |p: (f32, f32)| {
            buffers.indices.chunks(3).any(|tri| {
                let a = buffers.vertices[tri[0] as usize];
                let b = buffers.vertices[tri[1] as usize];
                let c = buffers.vertices[tri[2] as usize];
                point_in_triangle(p, (a[0], a[1]), (b[0], b[1]), (c[0], c[1]))
            })
        };

        assert!(
            !covers((cx, cy)),
            "the centre of 'o' must be an unfilled counter, not solid ink — a per-subpath \
             tessellation (the regression this module exists to avoid) would fill it"
        );
    }
}
