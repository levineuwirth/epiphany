//! A shaped text run's glyphs, placed for drawing (`req:layoutir:text-run`):
//! each is drawn from the face its segment names, at the run's origin plus its
//! offset, never from the string. Both exports draw text through this.

use epiphany_layout_ir::{face_by_hash, ResolvedText, Transform2D};
use epiphany_text::{Face, PathCommand};

/// One glyph of a run, placed: its face and id, where its origin stands in
/// the layout's world, and the scale from its face's units to staff spaces.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct TextGlyph {
    pub face: Face,
    pub glyph: u32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    /// A turn or slant about its origin, applied after the scale.
    pub transform: Option<Transform2D>,
}

/// Each glyph of `run`, placed; or the name of a face of its chain this build
/// does not carry, whose glyphs cannot be drawn.
pub(crate) fn placed(run: &ResolvedText) -> Result<Vec<TextGlyph>, String> {
    let mut out = Vec::new();
    for segment in &run.segments {
        let identity = run
            .shaping
            .faces
            .get(segment.face as usize)
            .ok_or_else(|| String::from("a segment naming no face of its chain"))?;
        let face = face_by_hash(&identity.file_hash)
            .ok_or_else(|| identity.family.0.clone().into_owned())?;
        let scale = segment.size.0 / f32::from(face.units_per_em());
        for g in &segment.glyphs {
            out.push(TextGlyph {
                face,
                glyph: g.glyph_id,
                x: run.origin.x.0 + g.offset.x.0,
                y: run.origin.y.0 + g.offset.y.0,
                scale,
                transform: g.transform,
            });
        }
    }
    Ok(out)
}

/// A run's ink, placed in the layout's world: `(left, bottom, right, top)`.
pub(crate) fn ink(run: &ResolvedText) -> (f32, f32, f32, f32) {
    let (x, y, b) = (run.origin.x.0, run.origin.y.0, run.bounds);
    (x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0)
}

/// A glyph's outline as SVG path data, in its face's units, y up.
pub(crate) fn svg_path(face: Face, glyph: u32, num: impl Fn(f32) -> String) -> String {
    let mut d = String::new();
    for command in face.outline(glyph) {
        let part = match command {
            PathCommand::MoveTo(x, y) => format!("M{} {}", num(x), num(y)),
            PathCommand::LineTo(x, y) => format!("L{} {}", num(x), num(y)),
            PathCommand::QuadTo(cx, cy, x, y) => {
                format!("Q{} {} {} {}", num(cx), num(cy), num(x), num(y))
            }
            PathCommand::CurveTo(c1x, c1y, c2x, c2y, x, y) => format!(
                "C{} {} {} {} {} {}",
                num(c1x),
                num(c1y),
                num(c2x),
                num(c2y),
                num(x),
                num(y)
            ),
            PathCommand::Close => String::from("Z"),
        };
        d.push_str(&part);
    }
    d
}

/// A scale factor written with enough digits to keep a glyph's size: a
/// face's units are about a thousandth of an em, so four decimals are not.
pub(crate) fn scale_number(v: f32) -> String {
    let mut s = format!("{v:.8}");
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

/// The linear part of a glyph's transform as `a b c d`, the matrix SVG's
/// `matrix(a b c d e f)` and PDF's `cm` take, its translation dropped (a
/// glyph's offset carries it); `None` for no transform.
pub(crate) fn linear(transform: &Option<Transform2D>) -> Option<[f32; 4]> {
    transform.map(|t| {
        let m = t.matrix;
        [m[0][0], m[1][0], m[0][1], m[1][1]]
    })
}
