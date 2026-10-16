//! One page of a resolved layout: the primitives its systems own, so a page
//! can be rendered alone (an export writes each page by itself).

use epiphany_layout_ir::{PrimitiveIndices, ResolvedLayoutIR};

/// The layout restricted to the primitives page `number` (1-based) owns
/// through its systems, or `None` when there is no such page. A layout of one
/// page keeps the primitives no system owns as well.
pub fn page_layout(layout: &ResolvedLayoutIR, number: usize) -> Option<ResolvedLayoutIR> {
    let page = layout.pages.get(number.checked_sub(1)?)?;
    let mut owned = PrimitiveIndices::default();
    for system in &page.systems {
        owned.glyphs.extend(&system.primitives.glyphs);
        owned.strokes.extend(&system.primitives.strokes);
        owned.curves.extend(&system.primitives.curves);
    }
    if layout.pages.len() == 1 {
        owned.glyphs.extend(&layout.unowned.glyphs);
        owned.strokes.extend(&layout.unowned.strokes);
        owned.curves.extend(&layout.unowned.curves);
    }
    let pick = |indices: &mut Vec<u32>| {
        indices.sort_unstable();
        indices.dedup();
    };
    pick(&mut owned.glyphs);
    pick(&mut owned.strokes);
    pick(&mut owned.curves);
    let mut out = layout.clone();
    out.pages = vec![page.clone()];
    out.glyphs = owned
        .glyphs
        .iter()
        .map(|&i| layout.glyphs[i as usize].clone())
        .collect();
    out.strokes = owned
        .strokes
        .iter()
        .map(|&i| layout.strokes[i as usize].clone())
        .collect();
    out.curves = owned
        .curves
        .iter()
        .map(|&i| layout.curves[i as usize].clone())
        .collect();
    // The page's systems now index into the filtered arrays.
    let remap = |from: &[u32], kept: &[u32]| -> Vec<u32> {
        from.iter()
            .filter_map(|i| kept.binary_search(i).ok().map(|k| k as u32))
            .collect()
    };
    for system in &mut out.pages[0].systems {
        system.primitives = PrimitiveIndices {
            glyphs: remap(&system.primitives.glyphs, &owned.glyphs),
            strokes: remap(&system.primitives.strokes, &owned.strokes),
            curves: remap(&system.primitives.curves, &owned.curves),
        };
    }
    out.unowned = PrimitiveIndices::default();
    Some(out)
}
