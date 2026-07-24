//! The real glyph-render catalog (Editor T4-pre W2 pin 1). Metrics delegate
//! to `epiphany_layout_ir::BRAVURA_METRICS`, which stays in `epiphany-layout-ir`
//! unmoved and unchanged (pin 3) — this crate never edits or re-derives it.
//! `render_data` returns genuine outlines, parsed once from this crate's
//! bundled `d` strings: unlike `epiphany_layout_ir::BravuraCatalog` (which
//! bundles no render data and honestly returns `None` for every glyph), this
//! catalog can answer for real because the outline data lives here.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use epiphany_layout_ir::{
    metrics, metrics_hash_for, GlyphCatalog, GlyphCatalogIdentity, GlyphMetric, GlyphRenderData,
    PathCommand, SmuflVersion,
};

use crate::outlines_generated::BRAVURA_OUTLINES;
use crate::path::parse_d;

/// The bundled Bravura catalog with genuine render data (Editor T4-pre W2).
pub struct BravuraGlyphCatalog;

/// The lazily-built, cached name -> parsed-outline table (pin 6: caching is
/// permitted as long as it does not change results and two calls agree).
/// Parsing happens once per process; [`parse_d`] is pure, so the cached
/// `Vec<PathCommand>` a lookup clones is exactly what a fresh parse of the
/// same bundled string would produce — caching changes only *when* the work
/// happens, never *what* it returns.
fn render_data_table() -> &'static BTreeMap<&'static str, Vec<PathCommand>> {
    static TABLE: OnceLock<BTreeMap<&'static str, Vec<PathCommand>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        BRAVURA_OUTLINES
            .iter()
            .map(|o| (o.name, parse_d(o.path)))
            .collect()
    })
}

impl GlyphCatalog for BravuraGlyphCatalog {
    fn metrics(&self, name: &str) -> Option<&GlyphMetric> {
        metrics(name)
    }

    fn render_data(&self, name: &str) -> Option<GlyphRenderData> {
        render_data_table()
            .get(name)
            .map(|outline| GlyphRenderData {
                outline: outline.clone(),
                bitmap: None,
            })
    }

    fn smufl_version(&self) -> SmuflVersion {
        SmuflVersion::from_decimal(1, "4").expect("1.4 is a valid SMuFL version")
    }

    fn identity(&self, consulted: &[&str]) -> GlyphCatalogIdentity {
        GlyphCatalogIdentity {
            metrics_hash: metrics_hash_for(consulted.iter().copied()),
            ..GlyphCatalogIdentity::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extent::outline_extent;
    use epiphany_layout_ir::BRAVURA_METRICS;

    /// (g3) Every pipeline glyph (the `BRAVURA_METRICS` name set) resolves
    /// through the new catalog's `render_data` — mirroring
    /// `epiphany-render-svg`'s existing `every_pipeline_glyph_has_a_bundled_outline`,
    /// but against the real `GlyphCatalog` seam this packet populates rather
    /// than the raw outline table directly.
    #[test]
    fn every_pipeline_glyph_has_render_data() {
        let catalog = BravuraGlyphCatalog;
        for m in BRAVURA_METRICS {
            let data = catalog.render_data(m.name.as_ref());
            assert!(
                data.is_some(),
                "no render data for pipeline glyph {}",
                m.name
            );
            assert!(
                !data.unwrap().outline.is_empty(),
                "{}: render data has an empty outline",
                m.name
            );
        }
    }

    /// (g6, pin 6) Two calls return equal data — the cache must not change
    /// results, and `render_data` is deterministic and side-effect-free.
    #[test]
    fn render_data_is_deterministic_across_calls() {
        let catalog = BravuraGlyphCatalog;
        for m in BRAVURA_METRICS {
            let a = catalog.render_data(m.name.as_ref());
            let b = catalog.render_data(m.name.as_ref());
            assert_eq!(a, b, "{}: two render_data calls disagreed", m.name);
        }
    }

    /// (g4) Outline ink fits the declared metrics bbox: `layout-ir/src/glyph.rs:107-109`
    /// claims the metrics and outlines come from the same font release "so
    /// reserved advances/bboxes and the drawn ink agree" — this is the test
    /// that actually checks it, computed from the *typed* outline geometry
    /// (real cubic-bezier extrema, not just control points or the
    /// separately-stored `bbox` field), against `BRAVURA_METRICS`'s bounding
    /// box (converted from 1/1024-staff-space integers to `f32` staff
    /// spaces).
    ///
    /// Tolerance: the `d` string's coordinates are individually rounded to
    /// at most 4 decimals before this test re-derives extrema from them, and
    /// `BRAVURA_METRICS`'s integer bbox is independently floor/ceil-rounded
    /// to the *coarser* 1/1024 (~0.000977) staff-space grid from the
    /// generator's own (also 4-decimal-rounded) outline bbox — two
    /// independent roundings on each side of the comparison. `TOLERANCE`
    /// generously covers both: it is asserted, not tuned after the fact, and
    /// the test additionally reports the actual worst-case deviation so a
    /// real violation cannot hide inside a widened tolerance.
    #[test]
    fn outline_ink_fits_the_declared_metrics_bbox() {
        const TOLERANCE: f32 = 0.005;
        let catalog = BravuraGlyphCatalog;
        let mut worst: f32 = f32::NEG_INFINITY;
        let mut worst_glyph = "";
        let mut worst_side = "";
        for m in BRAVURA_METRICS {
            let data = catalog
                .render_data(m.name.as_ref())
                .unwrap_or_else(|| panic!("{}: no render data", m.name));
            let [ox_min, oy_min, ox_max, oy_max] = outline_extent(&data.outline)
                .unwrap_or_else(|| panic!("{}: empty outline", m.name));
            let mb = m.bounding_box();
            // Positive = the outline pokes out past that side of the metric
            // box by this many staff spaces; negative = margin to spare.
            let overflow = [
                ("left", mb.left.0 - ox_min),
                ("bottom", mb.bottom.0 - oy_min),
                ("right", ox_max - mb.right.0),
                ("top", oy_max - mb.top.0),
            ];
            for (side, amount) in overflow {
                if amount > worst {
                    worst = amount;
                    worst_glyph = m.name.as_ref();
                    worst_side = side;
                }
                assert!(
                    amount <= TOLERANCE,
                    "{}: outline overflows the metrics bbox on the {} side by {} \
                     staff spaces (tolerance {}) — outline extent [{ox_min}, {oy_min}, \
                     {ox_max}, {oy_max}], metrics bbox [{}, {}, {}, {}]",
                    m.name,
                    side,
                    amount,
                    TOLERANCE,
                    mb.left.0,
                    mb.bottom.0,
                    mb.right.0,
                    mb.top.0,
                );
            }
        }
        // Reported per the contract (g4): the real worst-case deviation
        // across every bundled glyph, not just "within tolerance". Printed
        // to 8 decimals (well past the data's own 4-decimal precision) so a
        // near-zero value is not mistaken for exactly zero.
        eprintln!(
            "g4 worst-case deviation: {worst:.8} staff spaces ({worst_glyph}, {worst_side} side; \
             negative = margin to spare, positive = overflow)"
        );
        assert!(
            worst.is_finite(),
            "worst-case deviation must be a real computed number"
        );
    }
}
