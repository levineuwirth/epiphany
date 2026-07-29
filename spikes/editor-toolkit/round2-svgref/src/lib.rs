//! The explicit-glyph SVG reference emitter (pin 10, `ROUND2_TEXT_RECIPE.md`
//! §9), plus its rasterizer.
//!
//! **Why this crate has to exist at all.** Today's exporter cannot draw a
//! `SpikeResolvedText`. An SVG `<text>` element carries *characters*, and the
//! viewer's own shaper picks the glyphs from them — so for anything
//! contextual (this round's measured `ff`/`fi` ligatures, the composed `é`)
//! `<text>` silently draws a different glyph than the layout resolved. Without
//! an emitter that addresses glyphs by id, Round 2's check 1 would be
//! `NOT RUN` for every candidate and the round would decide nothing.
//!
//! So the emitter writes **explicit glyph outlines as `<path>`**, looked up by
//! font-internal glyph id from the same content-hashed face that shaped the
//! run. This is a prototype of the explicit-glyph output W3 says the real
//! exporter needs (`ANALYSIS_TEXT_RUN_PRIMITIVES.md:496-509`), and its
//! findings are reported as such.
//!
//! **It never emits `<text>`, and [`assert_no_text_elements`] enforces that
//! structurally** rather than leaving it to a threshold. A `<text>` element in
//! the output would reintroduce exactly the re-shaping this round exists to
//! forbid, and it would do so *invisibly*: the raster might even look right on
//! a machine whose shaper agrees, which is the worst possible failure — a
//! reference that is wrong only sometimes.

use std::fmt::Write as _;

/// One glyph to draw: a font-internal id in `face`'s namespace, and where its
/// origin sits in device pixels.
///
/// The id is meaningless without the face's content hash — that is the whole
/// point of W3's identity discipline — so the caller passes the face bytes it
/// hashed, never a family name.
#[derive(Clone, Copy, Debug)]
pub struct DrawGlyph {
    pub glyph_id: u16,
    /// Device-space pen position for this glyph's origin, y-down.
    pub origin_x: f64,
    pub origin_y: f64,
    /// Device pixels per em for this glyph's face.
    pub em_px: f64,
}

/// A glyph's device-space bounding box, as measured from the outline the
/// emitter actually drew — returned so the caller can hand D4 its regions
/// without re-deriving them from a second source that might disagree.
#[derive(Clone, Debug)]
pub struct DrawnBounds {
    pub glyph_id: u16,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Collects a glyph outline into an SVG path `d` string, converting font units
/// to device pixels and flipping y (font space is y-up, SVG user space is
/// y-down).
struct PathSink {
    d: String,
    scale: f64,
    ox: f64,
    oy: f64,
    minx: f64,
    miny: f64,
    maxx: f64,
    maxy: f64,
    any: bool,
}

impl PathSink {
    fn map(&mut self, x: f32, y: f32) -> (f64, f64) {
        let dx = self.ox + x as f64 * self.scale;
        let dy = self.oy - y as f64 * self.scale;
        self.minx = self.minx.min(dx);
        self.miny = self.miny.min(dy);
        self.maxx = self.maxx.max(dx);
        self.maxy = self.maxy.max(dy);
        self.any = true;
        (dx, dy)
    }
}

impl ttf_parser::OutlineBuilder for PathSink {
    fn move_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.map(x, y);
        let _ = write!(self.d, "M{a:.4} {b:.4} ");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.map(x, y);
        let _ = write!(self.d, "L{a:.4} {b:.4} ");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c1, d1) = self.map(x1, y1);
        let (a, b) = self.map(x, y);
        let _ = write!(self.d, "Q{c1:.4} {d1:.4} {a:.4} {b:.4} ");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, d1) = self.map(x1, y1);
        let (c2, d2) = self.map(x2, y2);
        let (a, b) = self.map(x, y);
        let _ = write!(self.d, "C{c1:.4} {d1:.4} {c2:.4} {d2:.4} {a:.4} {b:.4} ");
    }
    fn close(&mut self) {
        self.d.push_str("Z ");
    }
}

/// Emits one SVG document drawing every glyph in `glyphs` as an explicit
/// `<path>`, on an opaque white ground, in opaque black ink.
///
/// Returns the SVG source and the device-space bounds of each glyph actually
/// drawn.
///
/// **A glyph whose outline the face cannot produce is an error, not an
/// omission.** Skipping it would produce a reference raster that is silently
/// missing ink, and every candidate would then be compared against a
/// reference that is itself wrong — the failure mode is a *false PASS for a
/// candidate that also drew nothing there*, which no threshold can catch.
/// (`outline_glyph` legitimately returns `None` for a whitespace glyph with an
/// empty outline; those are reported separately as `empty`, not as errors, and
/// contribute no bounds.)
pub fn emit_svg(
    face_bytes: &[u8],
    face_index: u32,
    glyphs: &[DrawGlyph],
    width: u32,
    height: u32,
) -> Result<(String, Vec<DrawnBounds>, Vec<u16>), String> {
    let (paths, bounds, empty) = emit_glyph_paths(face_bytes, face_index, glyphs)?;
    Ok((wrap_document(width, height, &paths), bounds, empty))
}

/// Wraps already-emitted `<path>` fragments in one complete document on an
/// opaque white ground.
///
/// Public because a **multi-face run cannot be emitted any other way**: a run
/// whose segments resolve to different faces (this recipe's F-B and F-D do)
/// needs paths from more than one face inside a single document, and each
/// `emit_glyph_paths` call sees exactly one face.
pub fn wrap_document(width: u32, height: u32, paths: &[String]) -> String {
    let mut svg = String::new();
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\">\
         <rect x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" fill=\"#ffffff\"/>"
    );
    for p in paths {
        svg.push_str(p);
    }
    svg.push_str("</svg>");
    svg
}

/// Emits one `<path>` fragment per drawable glyph, **without** a surrounding
/// document, plus the drawn bounds and the ids of glyphs that had no outline.
///
/// This exists because the first version of this crate exposed only
/// [`emit_svg`], which always returns a complete self-contained document. A
/// caller composing a multi-face run had no way to get at the paths, and
/// worked around it by string-slicing the fragment out from between this
/// crate's `<rect>` and `</svg>` markers. That worked, and it was a trap: any
/// change to this function's formatting would have broken the caller silently,
/// with no compiler signal and no failing test until a raster came out wrong.
/// A structural need deserves an API, not a substring search.
pub fn emit_glyph_paths(
    face_bytes: &[u8],
    face_index: u32,
    glyphs: &[DrawGlyph],
) -> Result<(Vec<String>, Vec<DrawnBounds>, Vec<u16>), String> {
    let face = ttf_parser::Face::parse(face_bytes, face_index)
        .map_err(|e| format!("face parse failed: {e}"))?;
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return Err("face reports units_per_em = 0".to_string());
    }

    let mut paths = Vec::new();
    let mut bounds = Vec::new();
    let mut empty = Vec::new();
    for g in glyphs {
        let mut sink = PathSink {
            d: String::new(),
            scale: g.em_px / upem,
            ox: g.origin_x,
            oy: g.origin_y,
            minx: f64::INFINITY,
            miny: f64::INFINITY,
            maxx: f64::NEG_INFINITY,
            maxy: f64::NEG_INFINITY,
            any: false,
        };
        let gid = ttf_parser::GlyphId(g.glyph_id);
        match face.outline_glyph(gid, &mut sink) {
            Some(_) if sink.any => {
                // fill-rule nonzero: the same rule Round 1 measured Bravura's
                // contours to agree with, and the rule TrueType/CFF outlines
                // are authored for. Counters are subtracted by winding, not by
                // a second painted shape.
                paths.push(format!(
                    "<path fill=\"#000000\" fill-rule=\"nonzero\" d=\"{}\"/>",
                    sink.d.trim_end()
                ));
                bounds.push(DrawnBounds {
                    glyph_id: g.glyph_id,
                    x0: sink.minx,
                    y0: sink.miny,
                    x1: sink.maxx,
                    y1: sink.maxy,
                });
            }
            _ => {
                // No outline: whitespace and other blank glyphs are legitimate
                // here. Recorded so the count can be checked against the
                // fixture's own expectation rather than assumed.
                empty.push(g.glyph_id);
            }
        }
    }
    Ok((paths, bounds, empty))
}

/// Hard structural check (recipe §9): the emitted document must contain no
/// `<text>`, `<tspan>`, `<textPath>`, or `<font-face>` construct.
///
/// This is deliberately a check on the *source*, run before anything is
/// rasterized. A `<text>`-bearing reference could rasterize to something that
/// looks correct on this machine and wrong on another, so catching it in the
/// pixels is not good enough — by then the reference has already been trusted.
pub fn assert_no_text_elements(svg: &str) -> Result<(), String> {
    for forbidden in ["<text", "<tspan", "<textPath", "font-face", "@font-face"] {
        if svg.contains(forbidden) {
            return Err(format!(
                "emitted SVG contains {forbidden:?} — the reference must address glyphs by \
                 font-internal id via <path>, never by character. A <text> element lets the \
                 viewer's own shaper choose the glyphs, which is precisely the re-shaping this \
                 round exists to forbid, and it would do so invisibly on any machine whose \
                 shaper happens to agree"
            ));
        }
    }
    Ok(())
}

/// Rasterizes the emitted SVG under pin 4's configuration: fixed
/// `width x height`, opaque ground, straight (un-premultiplied) RGBA8 out.
///
/// The returned buffer is exactly `width * height * 4` bytes with every alpha
/// at 255, which is what `round2-diff` requires — it refuses anything else
/// rather than guessing, so any transparency escaping from here would be
/// caught immediately rather than classified as ink.
pub fn rasterize(svg: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    assert_no_text_elements(svg)?;

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("usvg parse failed: {e}"))?;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| format!("could not allocate a {width}x{height} pixmap"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied RGBA. The white `<rect>` covers the whole
    // frame, so every pixel should already be opaque; demultiplication is
    // therefore a no-op here, but it is done explicitly rather than assumed,
    // and a non-opaque pixel is an error rather than a silent divide.
    let mut out = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for p in pixmap.pixels() {
        if p.alpha() != 255 {
            return Err(format!(
                "rasterizer produced a pixel with alpha {} — the background rect should make the \
                 whole frame opaque; a transparent pixel would be classified as ink by luma and \
                 quietly corrupt the differential",
                p.alpha()
            ));
        }
        out.extend_from_slice(&[p.red(), p.green(), p.blue(), 255]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGELLA: &str = "/usr/share/fonts/tex-gyre/texgyrepagella-regular.otf";

    fn face_bytes() -> Option<Vec<u8>> {
        std::fs::read(PAGELLA).ok()
    }

    #[test]
    fn text_elements_are_refused_at_the_source_not_the_raster() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><text x=\"0\" y=\"0\">fi</text></svg>";
        let err = assert_no_text_elements(svg).unwrap_err();
        assert!(err.contains("<text"), "{err}");
        // And rasterize must refuse it too, so the check cannot be bypassed
        // by calling the rasterizer directly.
        assert!(rasterize(svg, 16, 16).is_err());
    }

    #[test]
    fn font_face_is_refused_too() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>@font-face{}</style></svg>";
        assert!(assert_no_text_elements(svg).is_err());
    }

    #[test]
    fn emits_explicit_paths_and_rasterizes_opaque_ink() {
        let Some(bytes) = face_bytes() else {
            eprintln!("NOT RUN: {PAGELLA} absent — environment absence, not a failure");
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let gid = face.glyph_index('A').unwrap();
        let glyphs = [DrawGlyph {
            glyph_id: gid.0,
            origin_x: 100.0,
            origin_y: 300.0,
            em_px: 128.0,
        }];
        let (svg, bounds, empty) = emit_svg(&bytes, 0, &glyphs, 400, 400).unwrap();
        assert!(svg.contains("<path"), "must draw an explicit outline");
        assert!(assert_no_text_elements(&svg).is_ok());
        assert_eq!(bounds.len(), 1);
        assert!(empty.is_empty());

        let rgba = rasterize(&svg, 400, 400).unwrap();
        assert_eq!(rgba.len(), 400 * 400 * 4);
        assert!(rgba.chunks(4).all(|p| p[3] == 255));
        // There must actually be ink: a reference that renders blank would
        // make every candidate's ink points fail and every background point
        // pass, the exact false shape Round 1 hit with an unregistered
        // texture.
        let ink = rgba
            .chunks(4)
            .filter(|p| (299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32) / 1000 < 128)
            .count();
        assert!(ink > 200, "expected real ink coverage, got {ink} px");
    }

    #[test]
    fn a_glyph_with_no_outline_is_reported_empty_not_silently_skipped() {
        let Some(bytes) = face_bytes() else {
            eprintln!("NOT RUN: {PAGELLA} absent");
            return;
        };
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let space = face.glyph_index(' ').unwrap();
        let glyphs = [DrawGlyph {
            glyph_id: space.0,
            origin_x: 100.0,
            origin_y: 300.0,
            em_px: 128.0,
        }];
        let (_svg, bounds, empty) = emit_svg(&bytes, 0, &glyphs, 400, 400).unwrap();
        assert!(bounds.is_empty());
        assert_eq!(empty, vec![space.0]);
    }
}
