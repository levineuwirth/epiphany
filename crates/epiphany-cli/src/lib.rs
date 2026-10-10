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
    Margins, ResolvedLayoutIR, Size2D, SolverConfig, StaffSpace,
};
use epiphany_musicxml::fidelity::{self, Fidelity};
use epiphany_musicxml::outcome::{self, Reduced};
use epiphany_musicxml::{Import, SourceScore};
use epiphany_render_svg::{
    export_pages, render, render_pdf, Frame, PdfOptions, PdfPage, RenderOptions,
};

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
    /// The page it was cast off against.
    pub geometry: PageGeometry,
    /// Millimeters to the staff space on paper, where the source says: an
    /// import's file. A saved document holds no physical scale.
    pub staff_space_mm: Option<f32>,
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
    let mut engraved = if loaded.import.source.concert {
        engrave_on(&loaded.reduced.score, geometry)
    } else {
        engrave_on(&written_view(&loaded.reduced.score), geometry)
    };
    engraved.staff_space_mm = loaded.import.source.scaling.map(|s| s.staff_space_mm);
    engraved
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
        geometry,
        staff_space_mm: None,
        time,
    }
}

/// The layout restricted to the primitives page `number` (1-based) owns
/// through its systems, or `None` when there is no such page. A layout of one
/// page keeps the primitives no system owns as well.
pub fn page(layout: &ResolvedLayoutIR, number: usize) -> Option<ResolvedLayoutIR> {
    epiphany_render_svg::page_layout(layout, number)
}

/// What [`export`] writes.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ExportOptions {
    /// Every page as an SVG framed by its page, `<prefix>-<n>.svg`.
    pub svg: bool,
    /// Every page in one PDF, `<prefix>.pdf`.
    pub pdf: bool,
    /// Millimeters to the staff space on the PDF's paper. `None` takes the
    /// scale the source sets (an import's file), else 2 mm, the page the
    /// default geometry assumes: the score graph holds no physical scale.
    pub staff_space_mm: Option<f32>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            svg: true,
            pdf: true,
            staff_space_mm: None,
        }
    }
}

/// What [`export`] wrote.
#[derive(Clone, PartialEq, Debug)]
pub struct Exported {
    /// The paths written: the pages' SVGs in order, then the PDF.
    pub paths: Vec<std::path::PathBuf>,
    /// The pages whose ink runs past their paper, by number from 1, and how far
    /// in staff spaces: each is drawn in a frame extended to hold it.
    pub overruns: Vec<(usize, f32)>,
}

/// Writes every page of an engraved score beside `prefix`: each as an SVG
/// framed by its page rectangle, and all of them in one PDF, as `options`
/// asks. A page whose ink runs past its page is framed to hold it, and named
/// in what is returned.
pub fn export(
    engraved: &Engraved,
    prefix: &Path,
    options: &ExportOptions,
) -> std::io::Result<Exported> {
    let pages = export_pages(&engraved.layout, |index| {
        Frame::from(engraved.geometry.page_frame(index))
    });
    let overruns = pages
        .iter()
        .enumerate()
        .filter(|(_, page)| page.overrun() > 0.0)
        .map(|(index, page)| (index + 1, page.overrun()))
        .collect();
    let mut paths = Vec::new();
    let stem = prefix
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let beside = |suffix: String| prefix.with_file_name(format!("{stem}{suffix}"));
    if options.svg {
        for (index, page) in pages.iter().enumerate() {
            let text = render(
                &page.layout,
                &RenderOptions {
                    frame: Some(page.frame),
                    ..RenderOptions::default()
                },
            )
            .svg;
            let path = beside(format!("-{}.svg", index + 1));
            std::fs::write(&path, text)?;
            paths.push(path);
        }
    }
    if options.pdf {
        let pdf_pages: Vec<PdfPage<'_>> = pages
            .iter()
            .map(|page| PdfPage {
                layout: &page.layout,
                frame: page.frame,
            })
            .collect();
        let pdf = render_pdf(
            &pdf_pages,
            &PdfOptions {
                points_per_staff_space: options
                    .staff_space_mm
                    .or(engraved.staff_space_mm)
                    .unwrap_or(2.0)
                    * 72.0
                    / 25.4,
            },
        );
        let path = beside(".pdf".to_owned());
        std::fs::write(&path, pdf.bytes)?;
        paths.push(path);
    }
    Ok(Exported { paths, overruns })
}

/// Renders a layout to SVG with provenance traces.
pub fn svg(layout: &ResolvedLayoutIR) -> String {
    render(layout, &RenderOptions::default()).svg
}
