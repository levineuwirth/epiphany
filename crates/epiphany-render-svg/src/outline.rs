//! Lookup over the bundled Bravura outlines ([`crate::outlines_generated`]).

use crate::outlines_generated::{BravuraOutline, BRAVURA_OUTLINES};

/// The genuine Bravura outline for a SMuFL glyph name, if bundled. The table is
/// sorted by name, so this is a binary search.
pub(crate) fn outline(name: &str) -> Option<&'static BravuraOutline> {
    BRAVURA_OUTLINES
        .binary_search_by(|o| o.name.cmp(name))
        .ok()
        .map(|i| &BRAVURA_OUTLINES[i])
}

/// How many glyph outlines are bundled.
pub fn bundled_glyph_count() -> usize {
    BRAVURA_OUTLINES.len()
}

/// The SMuFL codepoint of a bundled glyph name, if bundled. Useful for a future
/// embedded-font rendering mode (which references glyphs by codepoint) and for
/// debugging glyph identity.
pub fn smufl_codepoint(name: &str) -> Option<u32> {
    outline(name).map(|o| o.codepoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_searchable() {
        // Binary search depends on the generator emitting names in order.
        assert!(BRAVURA_OUTLINES.windows(2).all(|w| w[0].name < w[1].name));
        assert!(outline("noteheadBlack").is_some());
        assert!(outline("gClef").is_some());
        assert!(outline("noSuchGlyph").is_none());
    }

    #[test]
    fn every_pipeline_glyph_has_a_bundled_outline() {
        // Non-vacuity: every glyph the v0 layout pipeline can name (the
        // layout-ir BRAVURA_METRICS set) is drawable. If the metrics table grows
        // a glyph, the generator must be re-run — this test fails until it is.
        for m in epiphany_layout_ir::BRAVURA_METRICS {
            assert!(
                outline(m.name.as_ref()).is_some(),
                "no bundled outline for pipeline glyph {}",
                m.name
            );
        }
    }

    #[test]
    fn metric_bboxes_contain_the_drawn_outlines() {
        // The engraver evaluates collisions from a glyph's metric bounding box,
        // while the renderer draws (and bounds) its outline. If a metric box were a
        // hair smaller than the ink — e.g. from rounding the bbox to the *nearest*
        // 1/1024 — a hard no-collision verdict could be microscopically false on
        // paper. The metrics are extracted as the outline bounds rounded *outward*
        // to the grid, so every metric box must contain its outline box.
        for m in epiphany_layout_ir::BRAVURA_METRICS {
            let Some(o) = outline(m.name.as_ref()) else {
                continue;
            };
            let mb = m.bounding_box();
            let [ol, ob, oright, otop] = o.bbox;
            assert!(
                mb.left.0 <= ol && mb.bottom.0 <= ob && mb.right.0 >= oright && mb.top.0 >= otop,
                "metric bbox {:?} for {} must contain its outline bbox {:?}",
                [mb.left.0, mb.bottom.0, mb.right.0, mb.top.0],
                m.name,
                o.bbox,
            );
        }
    }

    #[test]
    fn outlines_have_finite_bounds_and_nonempty_paths() {
        for o in BRAVURA_OUTLINES {
            assert!(o.bbox.iter().all(|v| v.is_finite()));
            assert!(o.bbox[0] <= o.bbox[2] && o.bbox[1] <= o.bbox[3]);
            assert!(!o.path.is_empty());
            assert!(o.path.starts_with('M'), "{} path must start with M", o.name);
        }
    }
}
