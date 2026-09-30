#![forbid(unsafe_code)]
//! The pipeline behind the command-line tools: read a MusicXML file, import
//! it through the operation API, reduce it, compare it against its source,
//! check the score's invariants, engrave it and render pages to SVG.

use std::path::Path;
use std::time::{Duration, Instant};

use epiphany_core::{check_invariants, Score, WellFormednessViolation};
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{
    constrained::LayoutDiagnostic, to_constrained, to_logical, ConstraintSolver, PrimitiveIndices,
    ResolvedLayoutIR, SolverConfig,
};
use epiphany_musicxml::fidelity::{self, Fidelity};
use epiphany_musicxml::outcome::{self, Reduced};
use epiphany_musicxml::Import;
use epiphany_render_svg::{render, RenderOptions};

/// A file imported, reduced, compared and checked.
pub struct Loaded {
    pub import: Import,
    pub reduced: Reduced,
    pub fidelity: Fidelity,
    pub violations: Vec<WellFormednessViolation>,
    pub read_time: Duration,
    pub reduce_time: Duration,
}

/// Why a file could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Read(epiphany_musicxml::ReadError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "{e}"),
            LoadError::Read(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Reads, imports, reduces, compares and checks a MusicXML file.
pub fn load(path: &Path) -> Result<Loaded, LoadError> {
    let xml = std::fs::read_to_string(path).map_err(LoadError::Io)?;
    let start = Instant::now();
    let import = epiphany_musicxml::import(&xml).map_err(LoadError::Read)?;
    let read_time = start.elapsed();
    let start = Instant::now();
    let reduced = outcome::reduce(&import);
    let reduce_time = start.elapsed();
    let fidelity = fidelity::compare(&import, &reduced);
    let violations = check_invariants(&reduced.score);
    Ok(Loaded {
        import,
        reduced,
        fidelity,
        violations,
        read_time,
        reduce_time,
    })
}

/// A score engraved by the real solver.
pub struct Engraved {
    pub layout: ResolvedLayoutIR,
    /// The projection's coverage diagnostics: what it could not engrave.
    pub diagnostics: Vec<LayoutDiagnostic>,
    pub time: Duration,
}

/// Engraves a score with the real solver at its default configuration.
pub fn engrave(score: &Score) -> Engraved {
    let start = Instant::now();
    let constrained = to_constrained(&to_logical(score));
    let report = Engraver::default().solve(&constrained, &SolverConfig::default());
    let time = start.elapsed();
    Engraved {
        diagnostics: constrained.diagnostics.clone(),
        layout: report.layout,
        time,
    }
}

/// The layout restricted to the primitives page `number` (1-based) owns
/// through its systems, or `None` when there is no such page. A layout of one
/// page keeps the primitives no system owns as well.
pub fn page(layout: &ResolvedLayoutIR, number: usize) -> Option<ResolvedLayoutIR> {
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

/// Renders a layout to SVG with provenance traces.
pub fn svg(layout: &ResolvedLayoutIR) -> String {
    render(layout, &RenderOptions::default()).svg
}
