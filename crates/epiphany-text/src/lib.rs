#![forbid(unsafe_code)]
//! # epiphany-text
//!
//! The text faces the engraver sets words in, and the shaper that sets them
//! (disposition E of `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`; Chapter 7,
//! `req:layoutir:text-run` and `req:layoutir:text-faces`).
//!
//! The faces are files, unmodified, in `faces/` beside their licences: Edwin
//! in four faces, MuseScore's text face, and Bravura Text behind each for the
//! music symbols set in words (a metronome mark's note). Each is identified by
//! the BLAKE3 hash of its bytes under the `MUSCFNTF` tag
//! ([`Face::file_hash`]), and a test locks every hash, so a face that changes
//! is a deliberate, reviewed change. No face is looked up on the host.
//!
//! [`shape`] sets a string in a chain of faces: each character takes the
//! first face of the chain that covers it, each run of characters taking one
//! face is shaped by harfrust at the face's own units, left to right, and a
//! character no face of the chain covers is returned unresolved, never drawn
//! from another face. The result is integer font units, so it is the same on
//! every platform; the engraver scales it to staff spaces. [`Face::outline`] gives a
//! glyph's outline from the same bytes, for the exports.

use std::ops::Range;
use std::sync::OnceLock;

use epiphany_determinism::{DomainTag, Preimage};
use harfrust::{Buffer, BufferFlags, Direction, ShapeOptions, ShaperFont};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::MetadataProvider;

/// The shaper, by name and version, as a run's shaping identity records it.
pub const SHAPER: &str = "harfrust";
/// harfrust's version, pinned with `=` in this crate's manifest.
pub const SHAPER_VERSION: (u32, u32, u32) = (0, 14, 0);

/// The Unicode version of the grapheme segmentation that fixes a run's caret
/// stops. The shaper's own character tables (its script itemization) are
/// fixed by [`SHAPER_VERSION`].
pub const UNICODE_VERSION: (u64, u64, u64) = unicode_segmentation::UNICODE_VERSION;

/// A text face the engraver may shape with.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Face {
    /// Edwin Roman.
    EdwinRoman,
    /// Edwin Italic.
    EdwinItalic,
    /// Edwin Bold.
    EdwinBold,
    /// Edwin Bold Italic.
    EdwinBoldItalic,
    /// Bravura Text: SMuFL's music symbols, sized to set in words.
    BravuraText,
}

impl Face {
    /// Every face, in declaration order.
    pub const ALL: [Face; 5] = [
        Face::EdwinRoman,
        Face::EdwinItalic,
        Face::EdwinBold,
        Face::EdwinBoldItalic,
        Face::BravuraText,
    ];

    /// The face file's bytes, exactly as fetched from its source.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Face::EdwinRoman => include_bytes!("../faces/Edwin-Roman.otf"),
            Face::EdwinItalic => include_bytes!("../faces/Edwin-Italic.otf"),
            Face::EdwinBold => include_bytes!("../faces/Edwin-Bold.otf"),
            Face::EdwinBoldItalic => include_bytes!("../faces/Edwin-BdIta.otf"),
            Face::BravuraText => include_bytes!("../faces/BravuraText.otf"),
        }
    }

    /// The face's name, for diagnostics only: a face is identified by its
    /// [`Face::file_hash`], never by its name.
    pub fn family(self) -> &'static str {
        match self {
            Face::EdwinRoman => "Edwin Roman",
            Face::EdwinItalic => "Edwin Italic",
            Face::EdwinBold => "Edwin Bold",
            Face::EdwinBoldItalic => "Edwin Bold Italic",
            Face::BravuraText => "Bravura Text",
        }
    }

    /// The face's version as its file declares it (`head.fontRevision`):
    /// Edwin 0.54 and Bravura Text 1.392, the minor carried whole as the
    /// glyph catalog carries Bravura's.
    pub fn version(self) -> (u32, u32, u32) {
        match self {
            Face::BravuraText => (1, 392, 0),
            _ => (0, 54, 0),
        }
    }

    /// The face file's identity: BLAKE3 over its bytes under the `MUSCFNTF`
    /// tag (`req:layoutir:text-faces`).
    pub fn file_hash(self) -> [u8; 32] {
        let mut preimage = Preimage::new(DomainTag::FONT_FILE);
        preimage.push_bytes(self.bytes());
        *preimage.finish().as_bytes()
    }

    /// The face as harfrust and skrifa read it, parsed once.
    fn font(self) -> &'static Loaded {
        static FACES: [OnceLock<Loaded>; 5] = [
            OnceLock::new(),
            OnceLock::new(),
            OnceLock::new(),
            OnceLock::new(),
            OnceLock::new(),
        ];
        FACES[self as usize].get_or_init(|| Loaded::new(self.bytes()))
    }

    /// The size of the face's em square, in its units.
    pub fn units_per_em(self) -> u16 {
        self.font().upem
    }

    /// The glyph the face maps `c` to, if it maps it.
    pub fn glyph(self, c: char) -> Option<u32> {
        self.font()
            .shaping
            .charmap()
            .map_unicode(c as u32)
            .map(|g| g.to_u32())
            .filter(|&g| g != 0)
    }

    /// Whether the face maps `c` to a glyph.
    pub fn covers(self, c: char) -> bool {
        self.glyph(c).is_some()
    }

    /// The face's ascender and descender, in its units (the descender
    /// negative): the room a line of it takes.
    pub fn vertical_metrics(self) -> (i32, i32) {
        let m = self
            .font()
            .skrifa()
            .metrics(Size::unscaled(), LocationRef::default());
        (m.ascent.round() as i32, m.descent.round() as i32)
    }

    /// A glyph's ink bounds `[left, bottom, right, top]` in the face's units,
    /// y up; `None` for a glyph with no ink (a space).
    pub fn glyph_bounds(self, glyph: u32) -> Option<[f32; 4]> {
        let mut pen = Bounds::default();
        self.draw(glyph, &mut pen);
        pen.bounds
    }

    /// A glyph's outline in the face's units, y up.
    pub fn outline(self, glyph: u32) -> Vec<PathCommand> {
        let mut pen = Commands::default();
        self.draw(glyph, &mut pen);
        pen.0
    }

    fn draw(self, glyph: u32, pen: &mut impl OutlinePen) {
        let font = self.font().skrifa();
        let outlines = font.outline_glyphs();
        if let Some(outline) = outlines.get(skrifa::GlyphId::new(glyph)) {
            // An outline the face cannot draw draws nothing: the bytes are
            // locked by hash, so this never varies between runs.
            let _ = outline.draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                pen,
            );
        }
    }
}

/// A face's bytes, parsed for shaping.
struct Loaded {
    bytes: &'static [u8],
    shaping: harfrust::Font,
    upem: u16,
}

impl Loaded {
    fn new(bytes: &'static [u8]) -> Self {
        let shaping = harfrust::Font::new(bytes, 0).expect("a bundled face parses");
        let upem = shaping.units_per_em();
        Loaded {
            bytes,
            shaping,
            upem,
        }
    }

    fn skrifa(&self) -> skrifa::FontRef<'static> {
        skrifa::FontRef::new(self.bytes).expect("a bundled face parses")
    }
}

/// One drawing command of a glyph's outline, in its face's units, y up.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PathCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CurveTo(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Default)]
struct Commands(Vec<PathCommand>);

impl OutlinePen for Commands {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(PathCommand::MoveTo(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(PathCommand::LineTo(x, y));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push(PathCommand::QuadTo(cx0, cy0, x, y));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(PathCommand::CurveTo(cx0, cy0, cx1, cy1, x, y));
    }
    fn close(&mut self) {
        self.0.push(PathCommand::Close);
    }
}

/// An outline's bounds over its points, control points included: a box that
/// holds the ink, as the glyph catalog's boxes do.
#[derive(Default)]
struct Bounds {
    bounds: Option<[f32; 4]>,
}

impl Bounds {
    fn add(&mut self, x: f32, y: f32) {
        let b = self.bounds.get_or_insert([x, y, x, y]);
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
}

impl OutlinePen for Bounds {
    fn move_to(&mut self, x: f32, y: f32) {
        self.add(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.add(x, y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.add(cx0, cy0);
        self.add(x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.add(cx0, cy0);
        self.add(cx1, cy1);
        self.add(x, y);
    }
    fn close(&mut self) {}
}

/// One glyph shaping placed, in its face's units: its id, the byte offset in
/// the shaped string of the cluster it belongs to, and its advance and offset.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub cluster: u32,
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}

/// A run of a string set in one face of the chain, or a run no face covers.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Segment {
    /// The face's index in the chain; for a run no face covers, the chain's
    /// first face, which draws nothing of it.
    pub face: usize,
    /// The half-open byte range of the string it covers.
    pub source: Range<usize>,
    /// The glyphs, in visual order (left to right).
    pub glyphs: Vec<ShapedGlyph>,
    /// False for a run of characters no face of the chain covers: no glyph is
    /// drawn for it, and its characters are reported.
    pub resolved: bool,
    /// The ISO 15924 script the shaper read the run as (`Zyyy` for common).
    pub script: [u8; 4],
    /// Whether the run reads left to right. The engraver shapes left to right
    /// only, and reports a run that would read otherwise.
    pub left_to_right: bool,
}

/// Sets `text` in the chain of `faces`: each character in the first face
/// that covers it, each run of characters in one face shaped by harfrust at
/// the face's own units, left to right, with its default features. A run of
/// characters no face covers is a segment of its own, unresolved and with no
/// glyph. The segments partition the string in order.
pub fn shape(text: &str, faces: &[Face]) -> Vec<Segment> {
    // Each character's face, by its index in the chain.
    let owner = |c: char| faces.iter().position(|face| face.covers(c));
    let mut runs: Vec<(Option<usize>, Range<usize>)> = Vec::new();
    for (at, c) in text.char_indices() {
        let face = owner(c);
        let end = at + c.len_utf8();
        match runs.last_mut() {
            Some((last, range)) if *last == face => range.end = end,
            _ => runs.push((face, at..end)),
        }
    }
    runs.into_iter()
        .map(|(face, source)| match face {
            Some(face) => shape_run(text, source, faces[face], face),
            None => Segment {
                face: 0,
                source,
                glyphs: Vec::new(),
                resolved: false,
                script: *b"Zzzz",
                left_to_right: true,
            },
        })
        .collect()
}

fn shape_run(text: &str, source: Range<usize>, face: Face, index: usize) -> Segment {
    let font = face.font();
    let shaper = ShaperFont::new(&font.shaping);
    let mut buffer = Buffer::new();
    buffer.push_str(&text[source.clone()]);
    buffer.set_pre_context(&text[..source.start]);
    buffer.set_post_context(&text[source.end..]);
    let mut flags = BufferFlags::empty();
    if source.start == 0 {
        flags |= BufferFlags::BEGINNING_OF_TEXT;
    }
    if source.end == text.len() {
        flags |= BufferFlags::END_OF_TEXT;
    }
    buffer.set_flags(flags);
    buffer.guess_segment_properties();
    let left_to_right = buffer.direction() == Direction::LeftToRight;
    // Shaped left to right whatever the script reads: a run that reads
    // otherwise is reported by the engraver, which shapes no other direction.
    buffer.set_direction(Direction::LeftToRight);
    let script = buffer
        .script()
        .map(|s| s.tag().to_be_bytes())
        .unwrap_or(*b"Zyyy");
    harfrust::shape(&shaper, &mut buffer, ShapeOptions::new())
        .expect("a buffer of text with a direction shapes");
    let glyphs = buffer
        .glyph_infos()
        .iter()
        .zip(buffer.glyph_positions())
        .map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id,
            cluster: info.cluster + source.start as u32,
            x_advance: pos.x_advance,
            y_advance: pos.y_advance,
            x_offset: pos.x_offset,
            y_offset: pos.y_offset,
        })
        .collect();
    Segment {
        face: index,
        source,
        glyphs,
        resolved: true,
        script,
        left_to_right,
    }
}

/// The byte offsets of `text`'s extended grapheme cluster boundaries, the
/// start and the end included: where a caret may stand.
pub fn grapheme_boundaries(text: &str) -> Vec<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out: Vec<usize> = text.grapheme_indices(true).map(|(at, _)| at).collect();
    out.push(text.len());
    out
}
