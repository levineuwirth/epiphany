#![forbid(unsafe_code)]
//! The pipeline behind the command-line tools: read a MusicXML file, import
//! it through the operation API, reduce it, compare it against its source,
//! check the score's invariants, engrave it and render pages to SVG.

use std::path::Path;
use std::time::{Duration, Instant};

use epiphany_bundle::FileStore;
use epiphany_core::{check_invariants, Score, WellFormednessViolation};
use epiphany_editor_core::{DocumentError, EditorDocument};
use epiphany_engrave::{Engraver, PageGeometry};
use epiphany_layout_ir::{
    constrained::LayoutDiagnostic, to_constrained, to_logical, written_view, ConstraintSolver,
    Margins, PrimitiveIndices, ResolvedLayoutIR, Size2D, SolverConfig, StaffSpace,
};
use epiphany_musicxml::fidelity::{self, Fidelity};
use epiphany_musicxml::outcome::{self, Reduced};
use epiphany_musicxml::{Import, SourceScore};
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
    Document(DocumentError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "{e}"),
            LoadError::Read(e) => write!(f, "{e}"),
            LoadError::Document(e) => write!(f, "{e}"),
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

/// Whether the file at `path` is a saved document (a bundle) rather than
/// MusicXML: whether it begins with the bundle's magic bytes.
pub fn is_document(path: &Path) -> Result<bool, LoadError> {
    use std::io::Read;
    let mut head = [0u8; 8];
    let mut file = std::fs::File::open(path).map_err(LoadError::Io)?;
    match file.read_exact(&mut head) {
        Ok(()) => Ok(head == epiphany_determinism::BUNDLE_MAGIC),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(LoadError::Io(e)),
    }
}

/// Opens the saved document at `path`.
pub fn open_document(path: &Path) -> Result<EditorDocument<FileStore>, LoadError> {
    let store = FileStore::open(path).map_err(LoadError::Io)?;
    EditorDocument::open(store).map_err(LoadError::Document)
}

/// Imports the MusicXML file at `source` into a new document at `target`, which
/// must not exist: the import's operations become the document's first
/// generation, its log.
pub fn new_document(source: &Path, target: &Path) -> Result<EditorDocument<FileStore>, LoadError> {
    let xml = std::fs::read_to_string(source).map_err(LoadError::Io)?;
    let import = epiphany_musicxml::import(&xml).map_err(LoadError::Read)?;
    let store = FileStore::create_new(target).map_err(LoadError::Io)?;
    EditorDocument::create(store, import.envelopes).map_err(LoadError::Document)
}

/// The score a file holds, imported or reduced from a saved document, engraved
/// on its page: an import at the pitch its file sets it in (see
/// [`engrave_loaded`]), a document as its operations make it.
pub fn engrave_path(path: &Path) -> Result<Engraved, LoadError> {
    if is_document(path)? {
        Ok(engrave_score(&open_document(path)?.score()))
    } else {
        Ok(engrave_loaded(&load(path)?))
    }
}

pub mod omissions;

/// A score engraved by the real solver.
pub struct Engraved {
    pub layout: ResolvedLayoutIR,
    /// The projection's coverage diagnostics: what it could not engrave.
    pub diagnostics: Vec<LayoutDiagnostic>,
    pub time: Duration,
}

/// Engraves a score with the real solver at its default configuration, on
/// the default page.
pub fn engrave(score: &Score) -> Engraved {
    engrave_on(score, PageGeometry::default())
}

/// Engraves an imported score on the page its file sets it on, or the
/// default page when the file gives none, and at the pitch its file sets it
/// in: a concert score at concert pitch, a transposed one with each
/// transposing part at written pitch under its written key.
pub fn engrave_loaded(loaded: &Loaded) -> Engraved {
    let geometry = page_of(&loaded.reduced.score);
    if loaded.import.source.concert {
        engrave_on(&loaded.reduced.score, geometry)
    } else {
        engrave_on(&written_view(&loaded.reduced.score), geometry)
    }
}

/// The page a score is set on: its canvas's layout defaults, which an import
/// sets from its file's page, and which are the default page otherwise.
pub fn page_of(score: &Score) -> PageGeometry {
    PageGeometry::from(&score.canvas.layout_defaults)
}

/// Engraves a score on its own page ([`page_of`]): how a document is drawn.
pub fn engrave_score(score: &Score) -> Engraved {
    engrave_on(score, page_of(score))
}

/// The page a file sets its score on, in staff spaces, or the default page
/// when it gives none: the page its writer drew it on, so the two read side
/// by side.
pub fn geometry(source: &SourceScore) -> PageGeometry {
    let Some(page) = source.page else {
        return PageGeometry::default();
    };
    PageGeometry {
        size: Size2D {
            width: StaffSpace(page.width),
            height: StaffSpace(page.height),
        },
        margins: Margins {
            top: StaffSpace(page.top),
            right: StaffSpace(page.right),
            bottom: StaffSpace(page.bottom),
            left: StaffSpace(page.left),
        },
    }
}

/// Engraves a score with the real solver at its default configuration, on
/// the given page.
pub fn engrave_on(score: &Score, geometry: PageGeometry) -> Engraved {
    let start = Instant::now();
    let constrained = to_constrained(&to_logical(score));
    let report = Engraver::with_geometry(geometry).solve(&constrained, &SolverConfig::default());
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
