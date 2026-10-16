//! The PDF renderer: pages of a [`ResolvedLayoutIR`] to a PDF 1.4 document.
//!
//! It draws what the SVG renderer draws, in the same order (by layer: strokes,
//! then curves, then glyphs) and from the same Bravura outlines, and makes the
//! same non-overreach promise: it encodes the resolved layout and decides
//! nothing about engraving. Each page is a world rectangle (a [`Frame`], where
//! the engraver stacked that page) mapped onto a PDF page of the same
//! proportions, [`PdfOptions::points_per_staff_space`] points to the staff
//! space. The score graph holds no physical scale, so the scale is the
//! caller's.
//!
//! Every distinct glyph is one form XObject, its outline filled with the
//! nonzero rule as SVG fills it, drawn wherever the glyph occurs; a glyph with
//! no bundled outline is drawn as its bounding box in red, with a
//! [`Diagnostic`], as in SVG. Content streams are written uncompressed, and
//! the output carries no date, so identical input yields identical bytes. The
//! writer has no dependency: the format's object, cross-reference and stream
//! syntax is written here.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use epiphany_glyphs::BravuraGlyphCatalog;
use epiphany_layout_ir::{GlyphCatalog, LineStyle, PathCommand, ResolvedLayoutIR, Transform2D};

use crate::svg::{Diagnostic, Frame};

/// How pages are scaled onto paper.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PdfOptions {
    /// Points (1/72 inch) per staff space. The default, 2 mm to the staff space,
    /// is the page the engraver's default geometry assumes: A4 at an 8 mm staff.
    pub points_per_staff_space: f32,
}

impl Default for PdfOptions {
    fn default() -> Self {
        PdfOptions {
            points_per_staff_space: 2.0 * 72.0 / 25.4,
        }
    }
}

/// One page to draw: the primitives it holds (typically one page's systems,
/// filtered out of a whole layout) and its frame in the layout's world.
#[derive(Copy, Clone, Debug)]
pub struct PdfPage<'a> {
    pub layout: &'a ResolvedLayoutIR,
    pub frame: Frame,
}

/// A rendered PDF and what the renderer could not draw.
#[derive(Clone, PartialEq, Debug)]
pub struct PdfOutput {
    pub bytes: Vec<u8>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Renders `pages` to one PDF document, one PDF page each, in order. Pure and
/// deterministic.
pub fn render_pdf(pages: &[PdfPage<'_>], options: &PdfOptions) -> PdfOutput {
    let catalog = BravuraGlyphCatalog;
    let mut diagnostics = Vec::new();

    // Every glyph name drawn, each with its outline if bundled.
    let names: BTreeSet<&str> = pages
        .iter()
        .flat_map(|page| page.layout.glyphs.iter().map(|g| g.glyph.as_str()))
        .collect();
    let mut outlines: BTreeMap<&str, Vec<PathCommand>> = BTreeMap::new();
    for name in &names {
        match catalog.render_data(name) {
            Some(data) => {
                outlines.insert(name, data.outline);
            }
            None => diagnostics.push(Diagnostic {
                message: "no bundled Bravura glyph for this name; drew bounding-box fallback"
                    .to_owned(),
                glyph: Some((*name).to_owned()),
            }),
        }
    }
    let alphas: BTreeSet<u32> = pages
        .iter()
        .flat_map(|page| {
            let layout = page.layout;
            layout
                .glyphs
                .iter()
                .map(|g| g.style.rgba)
                .chain(layout.strokes.iter().map(|s| s.style.rgba))
                .chain(layout.curves.iter().map(|c| c.style.rgba))
        })
        .map(|rgba| rgba & 0xff)
        .filter(|alpha| *alpha != 0xff)
        .collect();

    // Object numbers: 1 the catalog, 2 the page tree, then one per glyph form,
    // one per opacity state, and two per page (the page and its content).
    let mut writer = Writer::new();
    let first_form = 3;
    let form_number: BTreeMap<&str, usize> = outlines
        .keys()
        .enumerate()
        .map(|(i, name)| (*name, first_form + i))
        .collect();
    let first_state = first_form + outlines.len();
    let state_number: BTreeMap<u32, usize> = alphas
        .iter()
        .enumerate()
        .map(|(i, alpha)| (*alpha, first_state + i))
        .collect();
    let first_page = first_state + alphas.len();
    let info = first_page + 2 * pages.len();

    writer.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", first_page + 2 * i))
        .collect();
    let mut resources = String::from("<< /ProcSet [/PDF] /XObject <<");
    for (name, number) in &form_number {
        let _ = write!(resources, " /{} {number} 0 R", form_name(name, number));
    }
    resources.push_str(" >> /ExtGState <<");
    for (alpha, number) in &state_number {
        let _ = write!(resources, " /A{alpha} {number} 0 R");
    }
    resources.push_str(" >> >>");
    writer.object(
        2,
        &format!(
            "<< /Type /Pages /Kids [{}] /Count {} /Resources {resources} >>",
            kids.join(" "),
            pages.len()
        ),
    );

    for (name, outline) in &outlines {
        let mut path = String::new();
        let (mut left, mut bottom, mut right, mut top) = (0f32, 0f32, 0f32, 0f32);
        for command in outline {
            let mut see = |x: f32, y: f32| {
                left = left.min(x);
                bottom = bottom.min(y);
                right = right.max(x);
                top = top.max(y);
            };
            match command {
                PathCommand::MoveTo(p) => {
                    see(p.x.0, p.y.0);
                    let _ = writeln!(path, "{} {} m", num(p.x.0), num(p.y.0));
                }
                PathCommand::LineTo(p) => {
                    see(p.x.0, p.y.0);
                    let _ = writeln!(path, "{} {} l", num(p.x.0), num(p.y.0));
                }
                PathCommand::CurveTo {
                    control1,
                    control2,
                    to,
                } => {
                    for p in [control1, control2, to] {
                        see(p.x.0, p.y.0);
                    }
                    let _ = writeln!(
                        path,
                        "{} {} {} {} {} {} c",
                        num(control1.x.0),
                        num(control1.y.0),
                        num(control2.x.0),
                        num(control2.y.0),
                        num(to.x.0),
                        num(to.y.0)
                    );
                }
                PathCommand::Close => path.push_str("h\n"),
            }
        }
        path.push_str("f\n");
        writer.stream(
            form_number[name],
            &format!(
                "/Type /XObject /Subtype /Form /BBox [{} {} {} {}]",
                num(left - 0.1),
                num(bottom - 0.1),
                num(right + 0.1),
                num(top + 0.1)
            ),
            path.as_bytes(),
        );
    }
    for (alpha, number) in &state_number {
        let a = num(*alpha as f32 / 255.0);
        writer.object(*number, &format!("<< /Type /ExtGState /ca {a} /CA {a} >>"));
    }

    let scale = options.points_per_staff_space;
    for (i, page) in pages.iter().enumerate() {
        let number = first_page + 2 * i;
        let frame = page.frame;
        writer.object(
            number,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents {} 0 R >>",
                num(frame.width * scale),
                num(frame.height * scale),
                number + 1
            ),
        );
        let content = page_content(page, scale, &form_number);
        writer.stream(number + 1, "", content.as_bytes());
    }
    writer.object(info, "<< /Producer (epiphany-render-svg) >>");

    PdfOutput {
        bytes: writer.finish(info),
        diagnostics,
    }
}

/// The resource name of a glyph's form: `G` and its object number (glyph names
/// are not all valid PDF names, so the name is not used).
fn form_name(_glyph: &str, number: &usize) -> String {
    format!("G{number}")
}

/// A layer's strokes, curves and glyphs, by index into the layout's arrays.
type LayerIndices = (Vec<usize>, Vec<usize>, Vec<usize>);

/// One page's content stream: the frame mapped onto the page, then every layer
/// in ascending order, its strokes, curves and glyphs in that order.
fn page_content(page: &PdfPage<'_>, scale: f32, forms: &BTreeMap<&str, usize>) -> String {
    let layout = page.layout;
    let mut layers: BTreeMap<i32, LayerIndices> = BTreeMap::new();
    for (i, stroke) in layout.strokes.iter().enumerate() {
        layers.entry(stroke.layer).or_default().0.push(i);
    }
    for (i, curve) in layout.curves.iter().enumerate() {
        layers.entry(curve.layer).or_default().1.push(i);
    }
    for (i, glyph) in layout.glyphs.iter().enumerate() {
        layers.entry(glyph.layer).or_default().2.push(i);
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "q {} 0 0 {} {} {} cm",
        num(scale),
        num(scale),
        num(-page.frame.left * scale),
        num(-page.frame.bottom * scale)
    );
    for (strokes, curves, glyphs) in layers.values() {
        for &i in strokes {
            let stroke = &layout.strokes[i];
            let _ = writeln!(
                s,
                "q {}{} RG {} w {} {} m {} {} l S Q",
                state(stroke.style.rgba),
                rgb(stroke.style.rgba),
                num(stroke.thickness.0),
                num(stroke.from.x.0),
                num(stroke.from.y.0),
                num(stroke.to.x.0),
                num(stroke.to.y.0)
            );
        }
        for &i in curves {
            let curve = &layout.curves[i];
            let dash = match curve.line {
                LineStyle::Solid => "",
                LineStyle::Dashed => "[0.5 0.35] 0 d ",
                LineStyle::Dotted => "1 J [0 0.28] 0 d ",
            };
            let _ = writeln!(
                s,
                "q {}{} RG {} w {dash}{} {} m {} {} {} {} {} {} c S Q",
                state(curve.style.rgba),
                rgb(curve.style.rgba),
                num(curve.thickness.0),
                num(curve.p0.x.0),
                num(curve.p0.y.0),
                num(curve.p1.x.0),
                num(curve.p1.y.0),
                num(curve.p2.x.0),
                num(curve.p2.y.0),
                num(curve.p3.x.0),
                num(curve.p3.y.0)
            );
        }
        for &i in glyphs {
            let glyph = &layout.glyphs[i];
            let placement = placement(glyph.position.x.0, glyph.position.y.0, &glyph.transform);
            match forms.get(glyph.glyph.as_str()) {
                Some(number) => {
                    let _ = writeln!(
                        s,
                        "q {}{} rg {placement} /{} Do Q",
                        state(glyph.style.rgba),
                        rgb(glyph.style.rgba),
                        form_name(glyph.glyph.as_str(), number)
                    );
                }
                None => {
                    let bb = glyph.bounding_box;
                    let _ = writeln!(
                        s,
                        "q 0.8 0 0 RG 0.05 w {placement} {} {} {} {} re S Q",
                        num(bb.left.0),
                        num(bb.bottom.0),
                        num((bb.right.0 - bb.left.0).max(0.0)),
                        num((bb.top.0 - bb.bottom.0).max(0.0))
                    );
                }
            }
        }
    }
    s.push_str("Q\n");
    s
}

/// The `cm` operators placing a glyph at `(x, y)` with its resolved affine
/// applied about its origin, as the SVG renderer places it (the projective
/// bottom row, if any, dropped).
fn placement(x: f32, y: f32, transform: &Option<Transform2D>) -> String {
    let mut s = format!("1 0 0 1 {} {} cm", num(x), num(y));
    if let Some(t) = transform {
        let m = t.matrix;
        let _ = write!(
            s,
            " {} {} {} {} {} {} cm",
            num(m[0][0]),
            num(m[1][0]),
            num(m[0][1]),
            num(m[1][1]),
            num(m[0][2]),
            num(m[1][2])
        );
    }
    s
}

/// An `0xRRGGBBAA` colour's red, green and blue as PDF components.
fn rgb(rgba: u32) -> String {
    let c = |shift: u32| num(((rgba >> shift) & 0xff) as f32 / 255.0);
    format!("{} {} {}", c(24), c(16), c(8))
}

/// The graphics state setting a colour's opacity, when it is not opaque.
fn state(rgba: u32) -> String {
    match rgba & 0xff {
        0xff => String::new(),
        alpha => format!("/A{alpha} gs "),
    }
}

/// A number with at most four decimals, as the SVG renderer formats one.
fn num(v: f32) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
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

/// Accumulates numbered objects and writes the cross-reference table.
struct Writer {
    bytes: Vec<u8>,
    offsets: BTreeMap<usize, usize>,
}

impl Writer {
    fn new() -> Self {
        // The binary comment marks the file as binary to transfer tools.
        let mut bytes = b"%PDF-1.4\n%".to_vec();
        bytes.extend_from_slice(&[0xE2, 0xE3, 0xCF, 0xD3, b'\n']);
        Writer {
            bytes,
            offsets: BTreeMap::new(),
        }
    }

    fn object(&mut self, number: usize, body: &str) {
        self.offsets.insert(number, self.bytes.len());
        self.bytes
            .extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    fn stream(&mut self, number: usize, dictionary: &str, data: &[u8]) {
        self.offsets.insert(number, self.bytes.len());
        let separator = if dictionary.is_empty() { "" } else { " " };
        self.bytes.extend_from_slice(
            format!(
                "{number} 0 obj\n<< {dictionary}{separator}/Length {} >>\nstream\n",
                data.len()
            )
            .as_bytes(),
        );
        self.bytes.extend_from_slice(data);
        self.bytes.extend_from_slice(b"\nendstream\nendobj\n");
    }

    /// Writes the cross-reference table and trailer. Every object number from 1
    /// to the highest written must have been written.
    fn finish(mut self, info: usize) -> Vec<u8> {
        let count = self.offsets.keys().next_back().copied().unwrap_or(0) + 1;
        let xref = self.bytes.len();
        let mut table = format!("xref\n0 {count}\n0000000000 65535 f \n");
        for number in 1..count {
            let offset = self.offsets[&number];
            let _ = writeln!(table, "{offset:010} 00000 n ");
        }
        let _ = write!(
            table,
            "trailer\n<< /Size {count} /Root 1 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        );
        self.bytes.extend_from_slice(table.as_bytes());
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};

    fn stub_layout(seed: u64) -> ResolvedLayoutIR {
        let logical = to_logical(&valid_score_rich(seed));
        epiphany_layout_ir::StubSolver
            .solve(&to_constrained(&logical), &SolverConfig::default())
            .layout
    }

    fn frame() -> Frame {
        Frame {
            left: -10.0,
            bottom: -40.0,
            width: 120.0,
            height: 80.0,
        }
    }

    /// The object a cross-reference entry points at begins with its number.
    #[test]
    fn every_cross_reference_points_at_its_object() {
        let layout = stub_layout(3);
        let out = render_pdf(
            &[
                PdfPage {
                    layout: &layout,
                    frame: frame(),
                },
                PdfPage {
                    layout: &layout,
                    frame: frame(),
                },
            ],
            &PdfOptions::default(),
        );
        // Offsets are byte offsets, so the file is read as bytes: the binary
        // marker on its second line is not UTF-8.
        let bytes = &out.bytes;
        assert!(bytes.starts_with(b"%PDF-1.4\n"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|w| w == marker)
            .expect("a startxref");
        let tail = std::str::from_utf8(&bytes[at + marker.len()..]).expect("ASCII");
        let start: usize = tail.lines().next().unwrap().parse().expect("an offset");
        let table = std::str::from_utf8(&bytes[start..]).expect("the table is ASCII");
        assert!(table.starts_with("xref\n"));
        let lines: Vec<&str> = table.lines().collect();
        let count: usize = lines[1]
            .split(' ')
            .nth(1)
            .and_then(|n| n.parse().ok())
            .expect("an entry count");
        for (number, entry) in lines[3..3 + count - 1].iter().enumerate() {
            let offset: usize = entry[..10].parse().expect("an offset");
            assert!(
                bytes[offset..].starts_with(format!("{} 0 obj\n", number + 1).as_bytes()),
                "entry {} points at its object",
                number + 1
            );
        }
        let text = String::from_utf8_lossy(bytes);
        assert_eq!(text.matches("/Type /Page ").count(), 2);
        assert!(text.contains("/Count 2"));
    }

    /// Each glyph is drawn once through its form, each stroke and curve once.
    #[test]
    fn every_primitive_is_drawn_once() {
        let layout = stub_layout(5);
        let out = render_pdf(
            &[PdfPage {
                layout: &layout,
                frame: frame(),
            }],
            &PdfOptions::default(),
        );
        let text = String::from_utf8_lossy(&out.bytes);
        let drawn = text.matches(" Do Q").count() + text.matches(" re S Q").count();
        assert_eq!(drawn, layout.glyphs.len());
        assert_eq!(text.matches(" l S Q").count(), layout.strokes.len());
        assert_eq!(text.matches(" c S Q").count(), layout.curves.len());
        let distinct: BTreeSet<&str> = layout.glyphs.iter().map(|g| g.glyph.as_str()).collect();
        assert_eq!(
            text.matches("/Subtype /Form").count() + out.diagnostics.len(),
            distinct.len(),
            "one form per distinct bundled glyph"
        );
    }

    /// A page is as large as its frame at the chosen scale.
    #[test]
    fn a_page_is_its_frame_at_scale() {
        let layout = stub_layout(1);
        let out = render_pdf(
            &[PdfPage {
                layout: &layout,
                frame: frame(),
            }],
            &PdfOptions {
                points_per_staff_space: 2.0,
            },
        );
        let text = String::from_utf8_lossy(&out.bytes);
        assert!(text.contains("/MediaBox [0 0 240 160]"));
        assert!(text.contains("q 2 0 0 2 20 80 cm"));
    }

    #[test]
    fn rendering_is_deterministic() {
        let layout = stub_layout(9);
        let page = [PdfPage {
            layout: &layout,
            frame: frame(),
        }];
        assert_eq!(
            render_pdf(&page, &PdfOptions::default()),
            render_pdf(&page, &PdfOptions::default())
        );
    }
}
