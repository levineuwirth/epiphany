//! The glyph catalog (Chapter 7 §"Glyph Catalog Interface") and its
//! reproducibility identity (§7.3.2), with Bravura metrics bundled in-tree for
//! testing (QUICKSTART, Agent E).
//!
//! The IR references glyphs by name and queries metrics from a font catalog;
//! metrics are never embedded in pipeline objects (Chapter 7 §"Glyph metrics
//! live elsewhere"). For reproducible layout, the exact catalog consumed by a
//! solve MUST be identifiable: [`GlyphCatalogIdentity`] carries the font id, its
//! version, the SMuFL version, and a content hash over the canonical
//! serialization of every consulted glyph's metrics (bounding box, advance
//! width, **and named anchors**), computed with the Appendix D domain tag
//! `MUSCFNTM` ([`DomainTag::FONT_METRICS`]).
//!
//! v0 bundles a small but representative slice of the real
//! [Bravura](https://github.com/steinbergmedia/bravura) SMuFL font's metrics, in
//! `1/1024`-staff-space units (the catalog's exact, hashable unit), so the
//! catalog identity is exercised end to end without shipping a font file. A full
//! catalog (and the render-data side of the interface) is an out-of-core concern.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use epiphany_determinism::{DomainTag, Preimage};

use crate::spatial::{BoundingBox, Point};

/// The SMuFL version a catalog targets (Chapter 7: `SmuflVersion`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SmuflVersion {
    pub major: u16,
    pub minor: u16,
}

/// Identifier of a specific SMuFL font (Chapter 7: `FontId`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FontId(pub Cow<'static, str>);

impl FontId {
    /// The reference font v0 bundles metrics for.
    pub const BRAVURA: FontId = FontId(Cow::Borrowed("Bravura"));

    /// Constructs an identifier for a catalog loaded at runtime.
    pub fn owned(name: impl Into<String>) -> Self {
        FontId(Cow::Owned(name.into()))
    }
}

/// A font-catalog glyph identifier that may be bundled or loaded at runtime.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GlyphReference(pub Cow<'static, str>);

impl GlyphReference {
    pub const fn borrowed(name: &'static str) -> Self {
        GlyphReference(Cow::Borrowed(name))
    }

    pub fn owned(name: impl Into<String>) -> Self {
        GlyphReference(Cow::Owned(name.into()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

/// A semantic font version (Chapter 7 §7.3.2: the optional `font_version`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        SemVer {
            major,
            minor,
            patch,
        }
    }
}

/// A reproducibility-quality identifier for the glyph catalog used to produce a
/// layout (Chapter 7 §7.3.2). Required for any layout-conformance claim that
/// depends on byte-equal output across runs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphCatalogIdentity {
    /// SMuFL version targeted.
    pub smufl_version: SmuflVersion,
    /// The specific font in use.
    pub font_id: FontId,
    /// The font's release version, if the publisher uses versioned releases
    /// (Chapter 7 §7.3.2: the spec's optional `font_version`). Bravura is
    /// versioned, so v0 records the release the bundled metrics track.
    pub font_version: Option<SemVer>,
    /// Content hash (BLAKE3 / `MUSCFNTM`) over the canonical serialization of
    /// every consulted glyph's metrics (Chapter 7 §7.3.2).
    pub metrics_hash: [u8; 32],
}

/// The Bravura release the in-tree metrics table is extracted from — the *same*
/// SHA-pinned font (`bravura-1.392`) the renderer's outlines come from, so the
/// reserved advances/bboxes and the drawn ink agree.
///
/// **Version mapping (canonical):** the font declares a single decimal version —
/// name-table ID 5 is literally `"Version 1.392"` and `head.fontRevision ≈ 1.392`
/// — so it is recorded verbatim as `major.minor = 1.392`, i.e. `SemVer { major: 1,
/// minor: 392, patch: 0 }`. The minor field carries the font's `392` unchanged
/// rather than inventing a `1.39.2` patch split the font never declares, so this
/// identifier round-trips to the font's own version string.
pub const BRAVURA_VERSION: SemVer = SemVer::new(1, 392, 0);

impl Default for GlyphCatalogIdentity {
    /// The bundled Bravura identity, with `metrics_hash` over the *whole*
    /// in-tree table. A [`crate::ConstrainedLayoutIR`] overrides the hash with
    /// one over only the glyphs it references (the solve's true inputs).
    fn default() -> Self {
        GlyphCatalogIdentity {
            smufl_version: SmuflVersion { major: 1, minor: 4 },
            font_id: FontId::BRAVURA,
            font_version: Some(BRAVURA_VERSION),
            metrics_hash: metrics_hash_for(BRAVURA_METRICS.iter().map(|m| m.name.as_ref())),
        }
    }
}

/// A named anchor on a glyph (Chapter 7 §"Glyph Catalog Interface":
/// `GlyphMetrics.anchors`), e.g. a notehead's stem-attachment point, in
/// `1/1024`-staff-space units relative to the glyph origin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphAnchor {
    pub name: Cow<'static, str>,
    pub x: i32,
    pub y: i32,
}

/// One glyph's metrics: advance width, bounding box, and named anchors, in
/// `1/1024`-staff-space units (Chapter 7 §"Glyph Catalog Interface":
/// `GlyphMetrics`). Units are exact integers (hashable, deterministic);
/// [`GlyphMetric::bounding_box`] converts to staff-space `f32` on demand.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphMetric {
    pub name: Cow<'static, str>,
    pub advance: i32,
    pub bbox: [i32; 4],
    pub anchors: Cow<'static, [GlyphAnchor]>,
}

impl GlyphMetric {
    /// Constructs a metrics entry with no named anchors.
    pub const fn new(name: &'static str, advance: i32, bbox: [i32; 4]) -> Self {
        GlyphMetric {
            name: Cow::Borrowed(name),
            advance,
            bbox,
            anchors: Cow::Borrowed(&[]),
        }
    }

    /// Constructs a metrics entry with named anchors.
    pub const fn anchored(
        name: &'static str,
        advance: i32,
        bbox: [i32; 4],
        anchors: &'static [GlyphAnchor],
    ) -> Self {
        GlyphMetric {
            name: Cow::Borrowed(name),
            advance,
            bbox,
            anchors: Cow::Borrowed(anchors),
        }
    }

    /// The bounding box in staff spaces (Chapter 7: `GlyphMetrics.bounding_box`).
    pub fn bounding_box(&self) -> BoundingBox {
        let g = |u: i32| u as f32 / 1024.0;
        let [l, b, r, t] = self.bbox;
        BoundingBox::new(g(l), g(b), g(r), g(t))
    }
}

const STEM_UP_NW: GlyphAnchor = GlyphAnchor {
    name: Cow::Borrowed("stemUpNW"),
    x: 0,
    y: 0,
};
const STEM_DOWN_SE: GlyphAnchor = GlyphAnchor {
    name: Cow::Borrowed("stemDownSE"),
    x: 1180,
    y: 0,
};
const NOTEHEAD_ANCHORS: &[GlyphAnchor] = &[STEM_UP_NW, STEM_DOWN_SE];

/// A representative in-tree slice of Bravura's SMuFL metrics
/// (`(name, advance, [left, bottom, right, top])`, `1/1024`-staff-space units),
/// extracted from the SHA-pinned `bravura-1.392` font by `epiphany-render-svg`'s
/// `tools/extract_bravura_outlines.py` — the **same** font the renderer's outlines
/// come from. Each `bbox` is the corresponding outline's bounds rounded *outward*
/// to the `1/1024` grid (floor the mins, ceil the maxes), so the metric box always
/// **contains** the drawn ink: the engraver evaluates collisions from this box, and
/// a containing box keeps a no-collision result honest on paper (a `render-svg` test
/// proves the containment). Every glyph the v0 pipeline names is in this table; the
/// stub solver checks that, so a missing entry surfaces as
/// [`crate::SolveStatus::InternalError`]. The named anchors are SMuFL
/// engraving-default approximations (font metadata, not glyf bounds — so not part of
/// the outline extraction).
pub const BRAVURA_METRICS: &[GlyphMetric] = &[
    GlyphMetric::anchored(
        "noteheadBlack",
        1208,
        [0, -512, 1209, 512],
        NOTEHEAD_ANCHORS,
    ),
    GlyphMetric::anchored("noteheadHalf", 1208, [0, -512, 1209, 512], NOTEHEAD_ANCHORS),
    GlyphMetric::new("noteheadWhole", 1729, [0, -512, 1729, 512]),
    GlyphMetric::new("noteheadDoubleWhole", 2454, [0, -635, 2454, 635]),
    GlyphMetric::new("gClef", 2748, [0, -2696, 2749, 4498]),
    GlyphMetric::new("fClef", 2802, [-21, -2601, 2802, 1074]),
    GlyphMetric::new("cClef", 2863, [0, -2073, 2864, 2073]),
    GlyphMetric::new("accidentalSharp", 1020, [0, -1426, 1020, 1434]),
    GlyphMetric::new("accidentalFlat", 926, [0, -717, 926, 1799]),
    GlyphMetric::new("accidentalNatural", 688, [0, -1373, 689, 1397]),
    GlyphMetric::new("accidentalDoubleSharp", 1024, [0, -512, 1012, 521]),
    GlyphMetric::new("restWhole", 1159, [0, -553, 1156, 37]),
    GlyphMetric::new("restHalf", 1159, [0, -9, 1156, 582]),
    GlyphMetric::new("restQuarter", 1106, [4, -1536, 1106, 1528]),
    GlyphMetric::new("rest8th", 1024, [0, -1029, 1012, 713]),
    GlyphMetric::new("flag8thUp", 1081, [0, -3319, 1082, 37]),
    GlyphMetric::new("flag8thDown", 1253, [0, -58, 1254, 3310]),
    GlyphMetric::new("augmentationDot", 410, [0, -205, 410, 205]),
    // Time-signature digits and the common-time C — centred on the baseline
    // (y ≈ ±1), so a numerator/denominator pair straddles the staff midline.
    GlyphMetric::new("timeSig0", 1925, [81, -1024, 1844, 1029]),
    GlyphMetric::new("timeSig1", 1368, [81, -1024, 1287, 1029]),
    GlyphMetric::new("timeSig2", 1827, [81, -1053, 1745, 1041]),
    GlyphMetric::new("timeSig3", 1724, [81, -1029, 1643, 1020]),
    GlyphMetric::new("timeSig4", 1925, [81, -1024, 1844, 1029]),
    GlyphMetric::new("timeSig5", 1651, [81, -1029, 1569, 1008]),
    GlyphMetric::new("timeSig6", 1778, [81, -1020, 1696, 1029]),
    GlyphMetric::new("timeSig7", 1806, [81, -1024, 1725, 1020]),
    GlyphMetric::new("timeSig8", 1786, [81, -1061, 1704, 1061]),
    GlyphMetric::new("timeSig9", 1778, [81, -1020, 1696, 1029]),
    GlyphMetric::new("timeSigCommon", 1737, [20, -1020, 1737, 1029]),
    // Barlines: the origin is the lower end; the line runs 0..4096 (four staff
    // spaces) *up* from it, spanning the staff when anchored at the bottom line.
    GlyphMetric::new("barlineSingle", 147, [0, 0, 148, 4096]),
    GlyphMetric::new("barlineFinal", 938, [0, 0, 934, 4096]),
    // Repeat signs: composite barline glyphs (heavy/thin lines plus dots),
    // staff-spanning like the barlines above; `repeatDots` is the bare dot
    // pair drawn beside a barline (its box sits in the middle two staff
    // spaces when anchored at the bottom line).
    GlyphMetric::new("repeatLeft", 1507, [0, 0, 1500, 4096]),
    GlyphMetric::new("repeatRight", 1503, [4, 0, 1504, 4096]),
    GlyphMetric::new("repeatRightLeft", 2490, [4, 0, 2491, 4096]),
    GlyphMetric::new("repeatDots", 410, [0, 1302, 410, 2745]),
    GlyphMetric::new("dynamicForte", 1491, [-578, -623, 1491, 1819]),
    GlyphMetric::new("dynamicPiano", 1495, [-365, -582, 1500, 1123]),
];

/// Looks up one glyph's metrics by SMuFL name, if bundled.
pub fn metrics(name: &str) -> Option<&'static GlyphMetric> {
    BRAVURA_METRICS.iter().find(|m| m.name.as_ref() == name)
}

/// Whether every name in `names` has bundled metrics.
pub fn all_available<'a>(names: impl IntoIterator<Item = &'a str>) -> bool {
    names.into_iter().all(|n| metrics(n).is_some())
}

/// Whether a glyph name draws a **measure-boundary barline**: the plain and
/// final barlines plus the composite repeat signs (which replace a plain
/// barline at a repeat boundary). The casting-off solver classifies
/// measure-boundary columns with this instead of hard-coding the engraver's
/// name choices. `repeatDots` is decoration drawn *beside* a barline, not a
/// barline itself, so it is excluded.
pub fn is_barline_glyph(name: &str) -> bool {
    name.starts_with("barline") || matches!(name, "repeatLeft" | "repeatRight" | "repeatRightLeft")
}

/// A glyph's rendering data (Chapter 7 §"Glyph Catalog Interface":
/// `GlyphRenderData`). The full outline/bitmap vocabulary (`PathCommand`,
/// `GlyphBitmap`) belongs to the out-of-core renderer; v0 carries an opaque
/// marker so the interface is complete without bundling outlines.
#[derive(Clone, PartialEq, Debug)]
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    CurveTo {
        control1: Point,
        control2: Point,
        to: Point,
    },
    Close,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct GlyphRenderData {
    pub outline: Vec<PathCommand>,
    pub bitmap: Option<GlyphBitmap>,
}

/// The font-catalog query interface (Chapter 7 §"Glyph Catalog Interface":
/// `GlyphCatalog`). `Send + Sync` per the spec, so a catalog can be shared
/// across threads during parallel re-engraving.
pub trait GlyphCatalog: Send + Sync {
    /// Resolve a glyph name to its metrics.
    fn metrics(&self, name: &str) -> Option<&GlyphMetric>;
    /// Resolve a glyph name to its rendering data, if any.
    fn render_data(&self, name: &str) -> Option<GlyphRenderData>;
    /// The SMuFL version this catalog supports.
    fn smufl_version(&self) -> SmuflVersion;
    /// This catalog's reproducibility identity over the given consulted names.
    fn identity(&self, consulted: &[&str]) -> GlyphCatalogIdentity;
}

/// The bundled in-tree Bravura catalog. **Metric-only**: it bundles no render
/// data (outlines/bitmaps are a renderer concern), so
/// [`BravuraCatalog::render_data`] honestly returns `None` for every glyph.
pub struct BravuraCatalog;

impl GlyphCatalog for BravuraCatalog {
    fn metrics(&self, name: &str) -> Option<&GlyphMetric> {
        metrics(name)
    }

    fn render_data(&self, _name: &str) -> Option<GlyphRenderData> {
        // No outlines or bitmaps are bundled; reporting `Some` would claim render
        // data that does not exist.
        None
    }

    fn smufl_version(&self) -> SmuflVersion {
        SmuflVersion { major: 1, minor: 4 }
    }

    fn identity(&self, consulted: &[&str]) -> GlyphCatalogIdentity {
        GlyphCatalogIdentity {
            metrics_hash: metrics_hash_for(consulted.iter().copied()),
            ..GlyphCatalogIdentity::default()
        }
    }
}

/// The catalog metrics identity (Chapter 7 §7.3.2): a **domain-tagged**
/// (`MUSCFNTM`) BLAKE3 hash over the canonical serialization of the consulted
/// glyph metrics (advance, bounding box, and named anchors), rather than a raw
/// hash of a descriptive string.
///
/// Names are de-duplicated and put in canonical (sorted) order first, so two
/// solves consulting the same metric set hash identically regardless of glyph
/// ordering (Appendix D §"Ordered Iteration"). Panics if a name has no bundled
/// metrics — every glyph delivered to a solve MUST name available metrics.
pub fn metrics_hash_for<'a>(names: impl IntoIterator<Item = &'a str>) -> [u8; 32] {
    let names: BTreeSet<&str> = names.into_iter().collect();
    let mut p = Preimage::new(DomainTag::FONT_METRICS);
    p.push_u64_le(names.len() as u64);
    for name in names {
        let m = metrics(name).expect("every consulted glyph must name bundled metrics");
        p.push_u64_le(name.len() as u64);
        p.push_bytes(name.as_bytes());
        p.push_u64_le(m.advance as u64);
        for coord in m.bbox {
            p.push_u64_le(coord as u64);
        }
        // Anchors are a *map* keyed by name (Chapter 7 §"Glyph Catalog
        // Interface": `anchors: HashMap<AnchorName, Point2D>`), so hash them in
        // canonical name order (Appendix D §"Ordered Iteration over Sets and
        // Maps"). A duplicate anchor name is invalid map data and is **rejected**
        // (a panic), not silently order-collapsed — so the hash never depends on
        // anchor slice order.
        let mut anchors: BTreeMap<&str, (i32, i32)> = BTreeMap::new();
        for a in m.anchors.iter() {
            assert!(
                anchors.insert(a.name.as_ref(), (a.x, a.y)).is_none(),
                "glyph {} has a duplicate anchor name {}",
                name,
                a.name
            );
        }
        p.push_u64_le(anchors.len() as u64);
        for (anchor_name, (x, y)) in anchors {
            p.push_u64_le(anchor_name.len() as u64);
            p.push_bytes(anchor_name.as_bytes());
            p.push_u64_le(x as u64);
            p.push_u64_le(y as u64);
        }
    }
    *p.finish().as_bytes()
}

/// The bundled Bravura catalog identity (the [`GlyphCatalogIdentity::default`]).
pub fn bravura_catalog_identity() -> GlyphCatalogIdentity {
    GlyphCatalogIdentity::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_hash_is_domain_tagged_and_nonzero() {
        let h = bravura_catalog_identity().metrics_hash;
        assert_ne!(h, [0u8; 32]);
        assert_eq!(GlyphCatalogIdentity::default().metrics_hash, h);
        assert_eq!(
            GlyphCatalogIdentity::default().font_version,
            Some(BRAVURA_VERSION)
        );
    }

    #[test]
    fn metrics_hash_is_order_and_duplicate_independent() {
        let a = metrics_hash_for(["gClef", "noteheadBlack", "accidentalSharp"]);
        let b = metrics_hash_for(["accidentalSharp", "gClef", "noteheadBlack", "gClef"]);
        assert_eq!(a, b);
    }

    #[test]
    fn anchors_participate_in_the_hash() {
        // noteheadBlack carries stem anchors; noteheadWhole does not. Even with
        // equal bbox/advance they must hash differently.
        assert_ne!(
            metrics_hash_for(["noteheadBlack"]),
            metrics_hash_for(["noteheadWhole"])
        );
        assert!(!metrics("noteheadBlack").unwrap().anchors.is_empty());
        assert!(metrics("noteheadWhole").unwrap().anchors.is_empty());
    }

    #[test]
    fn catalog_trait_resolves_and_identifies() {
        let cat = BravuraCatalog;
        assert_eq!(cat.metrics("gClef"), metrics("gClef"));
        assert!(cat.metrics("noSuchGlyph").is_none());
        assert_eq!(
            cat.identity(&["gClef"]).metrics_hash,
            metrics_hash_for(["gClef"])
        );
    }

    #[test]
    fn runtime_catalog_can_use_owned_names_and_report_identity_through_dyn_trait() {
        struct RuntimeCatalog {
            metric: GlyphMetric,
        }
        impl GlyphCatalog for RuntimeCatalog {
            fn metrics(&self, name: &str) -> Option<&GlyphMetric> {
                (self.metric.name.as_ref() == name).then_some(&self.metric)
            }
            fn render_data(&self, _name: &str) -> Option<GlyphRenderData> {
                None
            }
            fn smufl_version(&self) -> SmuflVersion {
                SmuflVersion { major: 1, minor: 4 }
            }
            fn identity(&self, _consulted: &[&str]) -> GlyphCatalogIdentity {
                GlyphCatalogIdentity {
                    smufl_version: self.smufl_version(),
                    font_id: FontId::owned("Runtime Font"),
                    font_version: None,
                    metrics_hash: [7; 32],
                }
            }
        }
        let catalog: Box<dyn GlyphCatalog> = Box::new(RuntimeCatalog {
            metric: GlyphMetric {
                name: Cow::Owned("runtimeGlyph".to_owned()),
                advance: 1024,
                bbox: [0, 0, 1024, 1024],
                anchors: Cow::Owned(vec![GlyphAnchor {
                    name: Cow::Owned("runtimeAnchor".to_owned()),
                    x: 0,
                    y: 0,
                }]),
            },
        });
        assert!(catalog.metrics("runtimeGlyph").is_some());
        assert_eq!(catalog.identity(&["runtimeGlyph"]).metrics_hash, [7; 32]);
    }

    #[test]
    fn barline_classification_covers_the_repeat_signs_but_not_the_dots() {
        for name in [
            "barlineSingle",
            "barlineFinal",
            "repeatLeft",
            "repeatRight",
            "repeatRightLeft",
        ] {
            assert!(
                is_barline_glyph(name),
                "{name} is a measure-boundary barline"
            );
        }
        for name in ["repeatDots", "noteheadBlack", "timeSig2", "gClef"] {
            assert!(!is_barline_glyph(name), "{name} is not a barline");
        }
    }

    #[test]
    fn every_bundled_name_is_unique_and_resolvable() {
        let mut seen = BTreeSet::new();
        for m in BRAVURA_METRICS {
            assert!(
                seen.insert(m.name.as_ref()),
                "duplicate bundled glyph {}",
                m.name
            );
            assert_eq!(metrics(m.name.as_ref()), Some(m));
        }
        assert!(all_available(
            BRAVURA_METRICS.iter().map(|m| m.name.as_ref())
        ));
        assert!(!all_available(["noSuchGlyph"]));
    }
}
