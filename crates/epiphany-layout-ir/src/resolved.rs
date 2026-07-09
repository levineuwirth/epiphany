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
    pub engraving_decisions: Vec<EngravingDecision>,
    /// The catalog identity under which this layout was produced — required for
    /// any byte-equal conformance claim (Chapter 7 §7.3.2).
    pub catalog: GlyphCatalogIdentity,
}

impl ResolvedLayoutIR {
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
            match glyph.transform {
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
            });
        }
        push_len(out, self.engraving_decisions.len());
        for decision in &self.engraving_decisions {
            encode_decision(out, decision);
        }
        encode_catalog(out, &self.catalog);
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
    out.extend_from_slice(&c.smufl_version.minor.to_le_bytes());
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
            engraving_decisions: decisions,
            catalog: GlyphCatalogIdentity::default(),
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
        // 40 bytes, shifting the catalog and lengthening the output by 20.
        let bytes = ir(vec![], vec![]).canonical_bytes();
        let source_len = ScoreVersion::default().0.len();
        assert_eq!(source_len, 32, "ScoreVersion source is 32 bytes");
        // The first count prefix (pages) is a 4-byte u32 zero — not 8 bytes.
        assert_eq!(&bytes[source_len..source_len + 4], &0u32.to_le_bytes());
        // The five count prefixes occupy exactly 5 × 4 bytes; then the catalog,
        // whose length we recompute independently (no magic number).
        let catalog_len = {
            let mut c = Vec::new();
            encode_catalog(&mut c, &GlyphCatalogIdentity::default());
            c.len()
        };
        assert_eq!(
            bytes.len(),
            source_len + 5 * 4 + catalog_len,
            "five u32 count prefixes (20 bytes), not u64 (40 bytes)"
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
}
