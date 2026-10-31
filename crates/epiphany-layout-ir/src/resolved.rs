//! Stage 3 — `ResolvedLayoutIR` (Chapter 7 §"ResolvedLayoutIR").
//!
//! The output of the constraint solver: every glyph has a definitive position.
//! This is the IR the renderer consumes. v0 carries the resolved glyphs, the
//! engraving decisions (including any the solver itself made), and the catalog
//! identity under which the solve ran (Chapter 7 §7.3.2 / Chapter 9
//! within-implementation determinism), together with the page/system interface
//! populated by casting-off implementations.
//!
//! ## Canonical serialization
//!
//! Positions are working `f32` staff-space coordinates ([`Point`]); the
//! **canonical** form quantizes them to the `1/1024` grid at serialization time
//! (Appendix D §"Quantized Layout Coordinates"). [`ResolvedLayoutIR`] implements
//! [`CanonicalEncode`] over its *full* content — every glyph's provenance
//! (source, stable id, synthesis kind, dependencies) and quantized position,
//! every engraving decision, and the complete catalog identity — so two layouts
//! that differ in any of these produce different canonical bytes. A non-finite or
//! out-of-range coordinate is a determinism violation; it is **rejected** with a
//! panic (faulting in every build, debug and release alike), never silently
//! normalized to the origin (Appendix D: invalid geometry is rejected, not
//! aliased).

use epiphany_core::{MeasureId, StaffId, TypedObjectId};
use epiphany_determinism::{CanonicalEncode, CanonicalF64, QuantizedCoord};

use crate::constrained::{Curve, GlyphObjectId, GlyphStyle, Stroke};
use crate::engraving::{DecisionSource, EngravingDecision, EngravingDecisionKind};
use crate::glyph::{GlyphCatalogIdentity, GlyphReference};
use crate::logical::ScoreVersion;
use crate::provenance::{Provenance, SynthesisKind};
use crate::spatial::{BoundingBox, Margins, Point, Rect, Size2D, StaffSpace, Transform2D};
use crate::text::{CaretAffinity, FaceSynthesis, ResolvedText, TextAlign, TextDirection};
use crate::StemDirection;

/// A glyph with a definitive position (Chapter 7 §"ResolvedLayoutIR":
/// `ResolvedGlyph`). Carries the SMuFL [`GlyphReference`] so the renderer knows
/// *what symbol to draw*, and
/// the `f32` staff-space position; canonical output quantizes the position (see
/// the module's canonical-serialization note).
#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedGlyph {
    pub provenance: Provenance,
    /// The SMuFL glyph to draw (carried from the constrained glyph).
    pub glyph: GlyphReference,
    pub position: Point,
    pub transform: Option<Transform2D>,
    pub bounding_box: BoundingBox,
    pub style: GlyphStyle,
    pub layer: i32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedPage {
    pub provenance: Provenance,
    pub number: u32,
    pub size: Size2D,
    pub margins: Margins,
    pub systems: Vec<ResolvedSystem>,
    pub free_objects: Vec<GlyphObjectId>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedSystem {
    pub provenance: Provenance,
    pub bounding_box: Rect,
    pub staves: Vec<ResolvedStaff>,
    pub measures: Vec<ResolvedMeasure>,
    /// Which of the layout's flat `glyphs`/`strokes`/`curves` this system
    /// owns — index lists, not copies (see [`PrimitiveIndices`]). A primitive
    /// no system claims is not omitted; it is in [`ResolvedLayoutIR::unowned`]
    /// instead, so the partition stays total (Chapter 7's flat arrays are
    /// otherwise untouched by this field's existence — no primitive is split,
    /// merged, reordered, or renumbered to populate it).
    pub primitives: PrimitiveIndices,
}

/// Index lists into a [`ResolvedLayoutIR`]'s flat `glyphs`/`strokes`/`curves`
/// arrays — never the primitives themselves. `u32`: matches the flat arrays'
/// own canonical count-prefix width and comfortably exceeds any real layout
/// (no resolved layout nears 4 billion primitives).
///
/// One of these lives on every [`ResolvedSystem`] (what it owns) and one on
/// [`ResolvedLayoutIR`] itself (`unowned`: claimed by no system — e.g. every
/// primitive under the stub solver, which resolves no per-system geometry
/// and so publishes everything unowned rather than fabricating an
/// attribution). For each of the three flat arrays, the union of every
/// system's list plus the unowned bucket is exactly `0..len`, each index
/// appearing exactly once — a tested invariant (`epiphany-engrave`'s casting
/// module), not merely a convention.
///
/// **Not part of the canonical encoding** (see the exclusion note on
/// [`ResolvedLayoutIR::canonical_bytes`]): this partition draws nothing, so
/// two layouts differing only in it render identically and hash alike.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PrimitiveIndices {
    pub glyphs: Vec<u32>,
    pub strokes: Vec<u32>,
    pub curves: Vec<u32>,
    pub texts: Vec<u32>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedStaff {
    pub provenance: Provenance,
    pub staff: StaffId,
    pub bounding_box: Rect,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedMeasure {
    pub provenance: Provenance,
    pub measure: MeasureId,
    pub bounding_box: Rect,
}

/// The resolved IR: every glyph positioned (Chapter 7 §"ResolvedLayoutIR").
#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedLayoutIR {
    pub source: ScoreVersion,
    pub pages: Vec<ResolvedPage>,
    pub glyphs: Vec<ResolvedGlyph>,
    /// Resolved non-glyph line primitives (staff lines, stems, barlines, …),
    /// positioned by the solver alongside the glyphs.
    pub strokes: Vec<Stroke>,
    /// Resolved cubic-bézier curve primitives (slurs, …), positioned by the
    /// solver alongside the glyphs and strokes.
    pub curves: Vec<Curve>,
    /// Shaped text runs, positioned alongside the glyphs
    /// (`req:layoutir:text-run`).
    pub texts: Vec<ResolvedText>,
    pub engraving_decisions: Vec<EngravingDecision>,
    /// The catalog identity under which this layout was produced — required for
    /// any byte-equal conformance claim (Chapter 7 §7.3.2).
    pub catalog: GlyphCatalogIdentity,
    /// The primitives no system claims — the other half of the total,
    /// disjoint partition [`ResolvedSystem::primitives`] describes. See
    /// [`PrimitiveIndices`].
    pub unowned: PrimitiveIndices,
}

impl ResolvedLayoutIR {
    /// Every system across every page, in page order (a page's systems in
    /// their own stored order, pages visited in `pages` order) — top to
    /// bottom, reading order. `epiphany-editor-core` hand-rolls this same
    /// flatten today (`editor-core/src/lib.rs`); it adopts this accessor at
    /// T4, not before.
    pub fn systems(&self) -> impl Iterator<Item = &ResolvedSystem> {
        self.pages.iter().flat_map(|page| page.systems.iter())
    }

    /// The canonical serialized output (Appendix D §"Quantized Layout
    /// Coordinates"): the layout's *rendering fingerprint*, with glyph positions
    /// quantized to the `1/1024` grid. Equivalent to
    /// [`CanonicalEncode::to_canonical_bytes`].
    ///
    /// It encodes what a conformant renderer draws and what a conformance claim
    /// compares — every primitive's provenance, geometry, style, and layer — and
    /// **excludes non-canonical layout-attribution metadata**. Concretely:
    /// [`ResolvedGlyph`] drops its band on the way out of the constrained stage,
    /// while [`Stroke`] and [`Curve`] (whose types are shared with that stage)
    /// carry `vertical_band` through but do not encode it. Band ownership tells a
    /// vertical solver which staff owns a primitive; it draws nothing, so two
    /// layouts differing only in it are the same rendered layout and hash alike.
    /// **Per-system primitive ownership is excluded for the identical reason**:
    /// [`ResolvedSystem::primitives`] and [`ResolvedLayoutIR::unowned`] name
    /// which system (or no system) a primitive belongs to, and draw nothing —
    /// two layouts differing only in that partition are the same rendered
    /// layout and hash alike. Pinning it on the wire (if a future normative
    /// requirement wants that) is a spec-side schema-major decision, not one
    /// this type makes silently; see `DECISIONS.md`.
    ///
    /// Two solves whose internal f32 computations agree to better than `1/2048`
    /// staff space at every coordinate produce identical bytes; two layouts that
    /// differ in any provenance, engraving decision, or catalog field produce
    /// different bytes. Panics on a non-finite or out-of-range coordinate (a
    /// determinism violation that must be rejected, not normalized).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_canonical_bytes()
    }
}

impl CanonicalEncode for ResolvedLayoutIR {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.source.0);
        push_len(out, self.pages.len());
        for page in &self.pages {
            encode_page(out, page);
        }
        push_len(out, self.glyphs.len());
        for glyph in &self.glyphs {
            encode_provenance(out, &glyph.provenance);
            // The glyph reference itself (so swapping two glyphs' symbols, even
            // with the consulted-name set unchanged, changes the canonical bytes
            // — the encoding is injective in glyph identity).
            let name = glyph.glyph.as_str().as_bytes();
            push_len(out, name.len());
            out.extend_from_slice(name);
            let (qx, qy) = quantize(glyph.position);
            qx.encode_canonical(out);
            qy.encode_canonical(out);
            encode_transform(out, &glyph.transform);
            encode_bounding_box(out, glyph.bounding_box);
            out.extend_from_slice(&glyph.style.rgba.to_le_bytes());
            out.extend_from_slice(&glyph.layer.to_le_bytes());
        }
        push_len(out, self.strokes.len());
        for stroke in &self.strokes {
            encode_provenance(out, &stroke.provenance);
            let (fx, fy) = quantize(stroke.from);
            fx.encode_canonical(out);
            fy.encode_canonical(out);
            let (tx, ty) = quantize(stroke.to);
            tx.encode_canonical(out);
            ty.encode_canonical(out);
            encode_staff_space(out, stroke.thickness);
            out.extend_from_slice(&stroke.style.rgba.to_le_bytes());
            out.extend_from_slice(&stroke.layer.to_le_bytes());
        }
        push_len(out, self.curves.len());
        for curve in &self.curves {
            encode_provenance(out, &curve.provenance);
            for point in curve.control_points() {
                let (qx, qy) = quantize(point);
                qx.encode_canonical(out);
                qy.encode_canonical(out);
            }
            encode_staff_space(out, curve.thickness);
            out.extend_from_slice(&curve.style.rgba.to_le_bytes());
            out.extend_from_slice(&curve.layer.to_le_bytes());
            out.push(match curve.line {
                epiphany_core::LineStyle::Solid => 0,
                epiphany_core::LineStyle::Dashed => 1,
                epiphany_core::LineStyle::Dotted => 2,
                epiphany_core::LineStyle::Wavy => 3,
            });
        }
        push_len(out, self.texts.len());
        for text in &self.texts {
            encode_text(out, text);
        }
        push_len(out, self.engraving_decisions.len());
        for decision in &self.engraving_decisions {
            encode_decision(out, decision);
        }
        encode_catalog(out, &self.catalog);
    }
}

/// A text run's rendering fingerprint (`req:layoutir:resolved-primitives`):
/// its provenance, source string and shaping identity, then its positioned
/// glyphs, clusters and boxes, quantized as glyph positions are.
fn encode_text(out: &mut Vec<u8>, text: &ResolvedText) {
    encode_provenance(out, &text.provenance);
    encode_str(out, &text.text);
    let shaping = &text.shaping;
    push_len(out, shaping.faces.len());
    for face in &shaping.faces {
        encode_str(out, &face.family.0);
        match face.version {
            None => out.push(0),
            Some(v) => {
                out.push(1);
                out.extend_from_slice(&v.major.to_le_bytes());
                out.extend_from_slice(&v.minor.to_le_bytes());
                out.extend_from_slice(&v.patch.to_le_bytes());
            }
        }
        out.extend_from_slice(&face.file_hash);
        out.extend_from_slice(&face.face_index.to_le_bytes());
        push_len(out, face.variations.len());
        for (axis, value) in &face.variations {
            out.extend_from_slice(&axis.0);
            value.encode_canonical(out);
        }
        out.push(match face.synthesis {
            FaceSynthesis::None => 0,
            FaceSynthesis::Emboldened => 1,
            FaceSynthesis::Slanted => 2,
        });
    }
    encode_str(out, &shaping.shaper.0);
    let v = shaping.shaper_version;
    for part in [v.major, v.minor, v.patch] {
        out.extend_from_slice(&part.to_le_bytes());
    }
    push_len(out, shaping.features.len());
    for feature in &shaping.features {
        out.extend_from_slice(&feature.tag);
        out.extend_from_slice(&feature.value.to_le_bytes());
    }
    let u = shaping.unicode_version;
    out.extend_from_slice(&[u.major, u.minor, u.update]);
    push_len(out, text.segments.len());
    for segment in &text.segments {
        out.extend_from_slice(&segment.face.to_le_bytes());
        push_len(out, segment.glyphs.len());
        for glyph in &segment.glyphs {
            out.extend_from_slice(&glyph.glyph_id.to_le_bytes());
            let (x, y) = quantize(glyph.offset);
            x.encode_canonical(out);
            y.encode_canonical(out);
            encode_transform(out, &glyph.transform);
        }
        out.extend_from_slice(&segment.source.start.to_le_bytes());
        out.extend_from_slice(&segment.source.end.to_le_bytes());
        out.push(match segment.direction {
            TextDirection::LeftToRight => 0,
            TextDirection::RightToLeft => 1,
        });
        out.extend_from_slice(&segment.script.0);
        encode_str(out, &segment.language.0);
        encode_staff_space(out, segment.size);
    }
    push_len(out, text.clusters.clusters.len());
    for cluster in &text.clusters.clusters {
        out.extend_from_slice(&cluster.source.start.to_le_bytes());
        out.extend_from_slice(&cluster.source.end.to_le_bytes());
        out.extend_from_slice(&cluster.segment.to_le_bytes());
        out.extend_from_slice(&cluster.glyphs.start.to_le_bytes());
        out.extend_from_slice(&cluster.glyphs.end.to_le_bytes());
        out.push(cluster.resolved as u8);
        push_len(out, cluster.carets.len());
        for caret in &cluster.carets {
            out.extend_from_slice(&caret.offset.to_le_bytes());
            encode_staff_space(out, caret.x);
            out.push(match caret.affinity {
                CaretAffinity::Downstream => 0,
                CaretAffinity::Upstream => 1,
            });
        }
    }
    encode_bounding_box(out, text.bounds);
    encode_bounding_box(out, text.reserved_box);
    let (x, y) = quantize(text.origin);
    x.encode_canonical(out);
    y.encode_canonical(out);
    out.push(match text.align {
        TextAlign::Left => 0,
        TextAlign::Center => 1,
        TextAlign::Right => 2,
    });
    out.extend_from_slice(&text.style.rgba.to_le_bytes());
    out.extend_from_slice(&text.layer.to_le_bytes());
}

/// A length-prefixed UTF-8 string.
fn encode_str(out: &mut Vec<u8>, s: &str) {
    push_len(out, s.len());
    out.extend_from_slice(s.as_bytes());
}

/// An optional transform: its presence, then its matrix row by row.
fn encode_transform(out: &mut Vec<u8>, transform: &Option<Transform2D>) {
    match transform {
        None => out.push(0),
        Some(transform) => {
            out.push(1);
            for row in transform.matrix {
                for value in row {
                    encode_f32(out, value);
                }
            }
        }
    }
}

fn encode_page(out: &mut Vec<u8>, page: &ResolvedPage) {
    encode_provenance(out, &page.provenance);
    out.extend_from_slice(&page.number.to_le_bytes());
    encode_staff_space(out, page.size.width);
    encode_staff_space(out, page.size.height);
    for margin in [
        page.margins.top,
        page.margins.right,
        page.margins.bottom,
        page.margins.left,
    ] {
        encode_staff_space(out, margin);
    }
    push_len(out, page.systems.len());
    for system in &page.systems {
        encode_provenance(out, &system.provenance);
        encode_rect(out, system.bounding_box);
        push_len(out, system.staves.len());
        for staff in &system.staves {
            encode_provenance(out, &staff.provenance);
            out.extend_from_slice(&staff.staff.canonical_bytes());
            encode_rect(out, staff.bounding_box);
        }
        push_len(out, system.measures.len());
        for measure in &system.measures {
            encode_provenance(out, &measure.provenance);
            out.extend_from_slice(&measure.measure.canonical_bytes());
            encode_rect(out, measure.bounding_box);
        }
    }
    push_len(out, page.free_objects.len());
    for object in &page.free_objects {
        push_u128(out, object.0);
    }
}

fn encode_rect(out: &mut Vec<u8>, rect: Rect) {
    let (x, y) = quantize(rect.origin);
    x.encode_canonical(out);
    y.encode_canonical(out);
    encode_staff_space(out, rect.size.width);
    encode_staff_space(out, rect.size.height);
}

fn encode_bounding_box(out: &mut Vec<u8>, bounds: BoundingBox) {
    for coordinate in [bounds.left, bounds.bottom, bounds.right, bounds.top] {
        encode_staff_space(out, coordinate);
    }
}

fn encode_staff_space(out: &mut Vec<u8>, value: StaffSpace) {
    value
        .quantize()
        .unwrap_or_else(|| panic!("invalid staff-space value in canonical layout"))
        .encode_canonical(out);
}

fn encode_f32(out: &mut Vec<u8>, value: f32) {
    CanonicalF64::new(value as f64)
        .unwrap_or_else(|| panic!("non-finite transform in canonical layout"))
        .encode_canonical(out);
}

/// Appends a `u32` little-endian length/count prefix (schema major 1: the
/// resolved-layout unifies its length prefixes to `u32`, matching the core
/// codec's `put_len`; no resolved-layout count nears 4 GB). The resolved layout
/// is a non-canonical, encode-only determinism fingerprint (Appendix D
/// §"Quantized Layout Coordinates"), so this width change has no persisted-format
/// migration — a cross-major layout cache is regenerated, never decoded.
fn push_len(out: &mut Vec<u8>, n: usize) {
    debug_assert!(n <= u32::MAX as usize, "resolved-layout length exceeds u32");
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

fn push_u128(out: &mut Vec<u8>, v: u128) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Quantizes a working f32 position to the canonical grid, **rejecting** a
/// non-finite or out-of-range coordinate with a panic (Appendix D: invalid
/// geometry must be rejected, not aliased to the origin).
fn quantize(p: Point) -> (QuantizedCoord, QuantizedCoord) {
    p.quantize().unwrap_or_else(|| {
        panic!("non-finite or out-of-range resolved coordinate in canonical output")
    })
}

/// Length-prefixes an id's canonical bytes (self-delimiting).
fn encode_source(out: &mut Vec<u8>, source: &TypedObjectId) {
    let bytes = source.to_canonical_bytes();
    push_len(out, bytes.len());
    out.extend_from_slice(&bytes);
}

fn encode_provenance(out: &mut Vec<u8>, p: &Provenance) {
    encode_source(out, &p.source);
    push_u128(out, p.stable_id.0);
    match p.synthesis {
        None => out.push(0),
        Some(kind) => {
            out.push(1);
            encode_synthesis(out, kind);
        }
    }
    // Dependencies are a set: canonical (sorted) order, deduplicated.
    let mut deps: Vec<Vec<u8>> = p
        .dependencies
        .iter()
        .map(|d| d.to_canonical_bytes())
        .collect();
    deps.sort();
    deps.dedup();
    push_len(out, deps.len());
    for bytes in deps {
        push_len(out, bytes.len());
        out.extend_from_slice(&bytes);
    }
}

fn encode_synthesis(out: &mut Vec<u8>, kind: SynthesisKind) {
    match kind {
        SynthesisKind::CancellationAccidental => out.push(0),
        SynthesisKind::KeySignatureNatural => out.push(1),
        SynthesisKind::GeneratedRest => out.push(2),
        SynthesisKind::EngravedBreak => out.push(3),
        SynthesisKind::MultimeasureRest => out.push(4),
        SynthesisKind::Cautionary => out.push(5),
        SynthesisKind::Registered(id) => {
            out.push(6);
            push_u128(out, id.0);
        }
    }
}

fn encode_decision(out: &mut Vec<u8>, d: &EngravingDecision) {
    push_u128(out, d.id.0);
    push_u128(out, d.target.0);
    match &d.kind {
        EngravingDecisionKind::StemDirection(dir) => {
            out.push(0);
            out.push(matches!(dir, StemDirection::Up) as u8);
        }
        EngravingDecisionKind::LedgerLineCount(n) => {
            out.push(1);
            out.push(*n);
        }
        EngravingDecisionKind::SystemBreak => out.push(2),
        EngravingDecisionKind::PageBreak => out.push(3),
        EngravingDecisionKind::Registered(id) => {
            out.push(4);
            push_u128(out, id.0);
        }
    }
    match d.source {
        DecisionSource::Automatic => out.push(0),
        DecisionSource::UserOverride(id) => {
            out.push(1);
            push_u128(out, id.0);
        }
        DecisionSource::IrOverride => out.push(2),
    }
}

fn encode_catalog(out: &mut Vec<u8>, c: &GlyphCatalogIdentity) {
    out.extend_from_slice(&c.smufl_version.major.to_le_bytes());
    out.extend_from_slice(&c.smufl_version.minor_centi.to_le_bytes());
    let font = c.font_id.0.as_bytes();
    push_len(out, font.len());
    out.extend_from_slice(font);
    match c.font_version {
        None => out.push(0),
        Some(v) => {
            out.push(1);
            out.extend_from_slice(&v.major.to_le_bytes());
            out.extend_from_slice(&v.minor.to_le_bytes());
            out.extend_from_slice(&v.patch.to_le_bytes());
        }
    }
    out.extend_from_slice(&c.metrics_hash);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engraving::EngravingDecisionKind;
    use crate::provenance::{LayoutObjectId, Provenance};
    use epiphany_core::EventId;

    fn glyph(raw: u128, x: f32) -> ResolvedGlyph {
        glyph_named(raw, x, "noteheadBlack")
    }

    fn glyph_named(raw: u128, x: f32, name: &'static str) -> ResolvedGlyph {
        let source = TypedObjectId::Event(EventId::from_raw(raw));
        ResolvedGlyph {
            provenance: Provenance::projected(source, vec![]),
            glyph: GlyphReference::borrowed(name),
            position: Point::new(x, 0.0),
            transform: None,
            bounding_box: BoundingBox::default(),
            style: GlyphStyle { rgba: 0x0000_00ff },
            layer: 0,
        }
    }

    fn ir(glyphs: Vec<ResolvedGlyph>, decisions: Vec<EngravingDecision>) -> ResolvedLayoutIR {
        ResolvedLayoutIR {
            source: ScoreVersion::default(),
            pages: vec![],
            glyphs,
            strokes: vec![],
            curves: vec![],
            texts: vec![],
            engraving_decisions: decisions,
            catalog: GlyphCatalogIdentity::default(),
            unowned: PrimitiveIndices::default(),
        }
    }

    #[test]
    fn canonical_bytes_are_quantized_and_stable() {
        let base = ir(vec![glyph(1, 1.0), glyph(2, 2.5)], vec![]);
        let a = base.canonical_bytes();
        assert_eq!(a, base.canonical_bytes(), "canonical bytes must be stable");

        // Sub-grid f32 jitter is absorbed by quantization.
        let mut jittered = base.clone();
        jittered.glyphs[1].position = Point::new(2.5 + 1.0 / 4096.0, 0.0);
        assert_eq!(a, jittered.canonical_bytes());

        // A full grid unit changes the output.
        let mut moved = base.clone();
        moved.glyphs[1].position = Point::new(2.5 + 1.0 / 1024.0, 0.0);
        assert_ne!(a, moved.canonical_bytes());
    }

    #[test]
    fn count_prefixes_are_u32_width_locked() {
        // Schema major 1 unifies the resolved-layout length/count prefixes to
        // u32 (Binary Format companion §"Schema Major 1"). This locks the byte
        // shape so a revert to the old u64 prefixes fails: an empty layout
        // encodes its five counts — pages, glyphs, strokes, curves,
        // engraving_decisions — as u32 zeros (20 bytes) right after the 32-byte
        // ScoreVersion source, then the catalog. Under u64 that region would be
        // 48 bytes, shifting the catalog and lengthening the output by 24. (The
        // texts' count joined the four primitive counts and the decisions'.)
        let bytes = ir(vec![], vec![]).canonical_bytes();
        let source_len = ScoreVersion::default().0.len();
        assert_eq!(source_len, 32, "ScoreVersion source is 32 bytes");
        // The first count prefix (pages) is a 4-byte u32 zero — not 8 bytes.
        assert_eq!(&bytes[source_len..source_len + 4], &0u32.to_le_bytes());
        // The six count prefixes occupy exactly 6 × 4 bytes; then the catalog,
        // whose length we recompute independently (no magic number).
        let catalog_len = {
            let mut c = Vec::new();
            encode_catalog(&mut c, &GlyphCatalogIdentity::default());
            c.len()
        };
        assert_eq!(
            bytes.len(),
            source_len + 6 * 4 + catalog_len,
            "six u32 count prefixes (24 bytes), not u64 (48 bytes)"
        );
    }

    #[test]
    fn canonical_bytes_capture_engraving_decisions_and_catalog() {
        let base = ir(vec![glyph(1, 1.0)], vec![]);
        // Adding/altering an engraving decision changes the bytes.
        let with_decision = ir(
            vec![glyph(1, 1.0)],
            vec![EngravingDecision::automatic(
                LayoutObjectId(7),
                EngravingDecisionKind::SystemBreak,
            )],
        );
        assert_ne!(base.canonical_bytes(), with_decision.canonical_bytes());

        // A different catalog identity changes the bytes.
        let mut other_catalog = base.clone();
        other_catalog.catalog.metrics_hash[0] ^= 1;
        assert_ne!(base.canonical_bytes(), other_catalog.canonical_bytes());

        let mut other_source = base.clone();
        other_source.source.0[0] = 1;
        assert_ne!(base.canonical_bytes(), other_source.canonical_bytes());

        let mut other_style = base.clone();
        other_style.glyphs[0].style.rgba ^= 1;
        assert_ne!(base.canonical_bytes(), other_style.canonical_bytes());

        let mut other_bounds = base.clone();
        other_bounds.glyphs[0].bounding_box.right = StaffSpace(1.0);
        assert_ne!(base.canonical_bytes(), other_bounds.canonical_bytes());

        let mut transformed = base.clone();
        transformed.glyphs[0].transform = Some(Transform2D::default());
        assert_ne!(base.canonical_bytes(), transformed.canonical_bytes());
    }

    #[test]
    fn swapping_glyph_names_changes_canonical_bytes() {
        // Two glyphs whose names are swapped between their sources — the
        // consulted-name *set* (and so the metrics hash) is unchanged, but the
        // per-glyph assignment differs, so the canonical bytes MUST differ
        // (the encoding is injective in glyph identity).
        let a = ir(
            vec![
                glyph_named(1, 1.0, "noteheadBlack"),
                glyph_named(2, 2.0, "gClef"),
            ],
            vec![],
        );
        let b = ir(
            vec![
                glyph_named(1, 1.0, "gClef"),
                glyph_named(2, 2.0, "noteheadBlack"),
            ],
            vec![],
        );
        assert_ne!(a.canonical_bytes(), b.canonical_bytes());
    }

    #[test]
    fn synthesis_and_stable_id_are_part_of_canonical_bytes() {
        let src = TypedObjectId::Event(EventId::from_raw(1));
        let mut plain = glyph(1, 1.0);
        plain.provenance = Provenance::projected(src, vec![]);
        let mut synth = glyph(1, 1.0);
        synth.provenance = Provenance::synthesized(
            src,
            SynthesisKind::Cautionary,
            crate::SynthesisInstanceKey(0),
            vec![],
        );
        // Same source and position, but synthesis kind + stable id differ.
        assert_ne!(
            ir(vec![plain], vec![]).canonical_bytes(),
            ir(vec![synth], vec![]).canonical_bytes()
        );
    }

    #[test]
    #[should_panic(expected = "non-finite")]
    fn non_finite_geometry_is_rejected_not_normalized() {
        let bad = ir(vec![glyph(1, f32::NAN)], vec![]);
        let _ = bad.canonical_bytes();
    }

    #[test]
    fn primitive_ownership_is_excluded_from_canonical_bytes() {
        // (m6) Perturbing ONLY the ownership lists — a system's `primitives`
        // and the layout's `unowned` bucket — must not change
        // `canonical_bytes()`, even though `PartialEq` sees the difference:
        // the same exclusion `vertical_band` gets (Chapter 7's ownership
        // partition draws nothing).
        let system = ResolvedSystem {
            provenance: Provenance::projected(TypedObjectId::Event(EventId::from_raw(9)), vec![]),
            bounding_box: Rect::default(),
            staves: vec![],
            measures: vec![],
            primitives: PrimitiveIndices::default(),
        };
        let page = ResolvedPage {
            provenance: Provenance::projected(TypedObjectId::Event(EventId::from_raw(10)), vec![]),
            number: 1,
            size: Size2D::default(),
            margins: Margins::default(),
            systems: vec![system],
            free_objects: vec![],
        };
        let mut base = ir(vec![glyph(1, 1.0)], vec![]);
        base.pages = vec![page];
        let base_bytes = base.canonical_bytes();

        let mut perturbed = base.clone();
        perturbed.pages[0].systems[0].primitives = PrimitiveIndices {
            glyphs: vec![0],
            strokes: vec![],
            curves: vec![],
            texts: vec![],
        };
        perturbed.unowned = PrimitiveIndices {
            glyphs: vec![],
            strokes: vec![7],
            curves: vec![3, 4],
            texts: vec![2],
        };

        assert_ne!(
            base, perturbed,
            "PartialEq must see the ownership difference"
        );
        assert_eq!(
            base_bytes,
            perturbed.canonical_bytes(),
            "canonical bytes must not see it"
        );
    }

    fn run(raw: u128, words: &str, face: crate::text::TextFace) -> ResolvedText {
        crate::text::shape_text(
            Provenance::projected(
                TypedObjectId::Marker(epiphany_core::MarkerId::from_raw(raw)),
                vec![],
            ),
            words,
            face,
            2.016,
            TextAlign::Left,
            Point::new(10.0, -3.5),
        )
        .0
    }

    fn with_texts(texts: Vec<ResolvedText>) -> ResolvedLayoutIR {
        let mut layout = ir(vec![glyph(1, 1.0)], vec![]);
        layout.texts = texts;
        layout
    }

    #[test]
    fn canonical_bytes_capture_text_runs() {
        use crate::text::TextFace;
        let base = with_texts(vec![run(1, "Allegro", TextFace::Roman)]);
        let a = base.canonical_bytes();
        assert_ne!(a, with_texts(vec![]).canonical_bytes());

        let mut jittered = base.clone();
        jittered.texts[0].origin = Point::new(10.0 + 1.0 / 4096.0, -3.5);
        assert_eq!(a, jittered.canonical_bytes(), "sub-grid jitter is absorbed");

        let mut moved = base.clone();
        moved.texts[0].origin = Point::new(10.0 + 1.0 / 1024.0, -3.5);
        assert_ne!(a, moved.canonical_bytes());

        let mut nudged = base.clone();
        let g = &mut nudged.texts[0].segments[0].glyphs[2];
        g.offset = Point::new(g.offset.x.0 + 1.0 / 1024.0, g.offset.y.0);
        assert_ne!(a, nudged.canonical_bytes(), "a glyph's offset is encoded");

        let mut regid = base.clone();
        regid.texts[0].segments[0].glyphs[2].glyph_id += 1;
        assert_ne!(a, regid.canonical_bytes(), "a glyph's id is encoded");

        let mut reworded = base.clone();
        reworded.texts[0].text = String::from("Allegra");
        assert_ne!(a, reworded.canonical_bytes(), "the string is encoded");

        let mut other_face = base.clone();
        let mut shaping = (*other_face.texts[0].shaping).clone();
        shaping.faces[0].file_hash[0] ^= 1;
        other_face.texts[0].shaping = std::sync::Arc::new(shaping);
        assert_ne!(
            a,
            other_face.canonical_bytes(),
            "the faces' hashes are encoded"
        );

        let mut caret = base.clone();
        let stop = &mut caret.texts[0].clusters.clusters[0].carets[0];
        stop.affinity = crate::text::CaretAffinity::Upstream;
        assert_ne!(a, caret.canonical_bytes(), "the caret stops are encoded");

        let mut box_ = base.clone();
        box_.texts[0].reserved_box.top = StaffSpace(9.0);
        assert_ne!(a, box_.canonical_bytes(), "the reserved box is encoded");
    }

    /// Shaped runs give the same canonical bytes on every run, and on every
    /// toolchain and profile the suite runs under: the shaping, its scale to
    /// staff spaces, the clusters and their encoding together. A change to any
    /// of them moves this digest and is then a deliberate change.
    #[test]
    fn text_runs_encode_the_same_on_every_run() {
        use crate::text::TextFace;
        let layout = || {
            with_texts(vec![
                run(1, "Andante con moto", TextFace::Roman),
                run(2, "dolce", TextFace::Italic),
                run(3, "Allegro \u{ECA5} = 132", TextFace::Bold),
                run(4, "Kyrie e-", TextFace::BoldItalic),
                run(5, "a\u{4E2D}b \u{0301}", TextFace::Roman),
            ])
        };
        // The runs' own encoding, apart from the rest of the layout, whose
        // catalog identity hashes the whole metrics table.
        let digest = |l: &ResolvedLayoutIR| -> String {
            let mut bytes = Vec::new();
            for text in &l.texts {
                encode_text(&mut bytes, text);
            }
            let mut preimage =
                epiphany_determinism::Preimage::new(epiphany_determinism::DomainTag::FONT_FILE);
            preimage.push_bytes(&bytes);
            preimage
                .finish()
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        };
        let first = digest(&layout());
        assert_eq!(first, digest(&layout()));
        assert_eq!(
            first,
            "5b7599651187eaf8951360eacfee1991c637630d5f8947ab4d452853c5da3fba"
        );
    }
}
