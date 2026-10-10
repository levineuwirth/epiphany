//! The window's document: opened from a MusicXML file or a saved document,
//! saved, saved as, and exported. Kept apart from the window so it can be
//! tested without one.
//!
//! A MusicXML file becomes a new document held in memory, its log the
//! import's operations, until it is saved as a file. A saved document is
//! opened from its file under an advisory lock on that file, so a second
//! window does not write the same document; a document another window holds,
//! or one that opens read-only, is drawn and not leased. Saving commits the
//! session's edits; saving as saves, writes the document's bytes to a new
//! file and opens that, under a fresh lease (undo stops at a save anyway).

use std::path::{Path, PathBuf};

use epiphany_bundle::{BlockStore, FileStore, MemStore};
use epiphany_cli::{export, is_document, page_of, Engraved, ExportOptions};
use epiphany_core::ReplicaId;
use epiphany_editor_core::{EditorDocument, EditorSession, ScoreSetup};
use epiphany_engrave::Engraver;

/// A document behind either kind of store.
pub type Document = EditorDocument<Box<dyn BlockStore>>;

/// The document a window edits, and where it lives.
pub struct Opened {
    pub document: Document,
    /// The file it is saved in; `None` for a new score or an import not yet
    /// saved.
    pub path: Option<PathBuf>,
    /// Millimeters to the staff space on paper, from the MusicXML file it was
    /// imported from, for its PDF; a saved document holds none.
    pub staff_space_mm: Option<f32>,
    /// Whether the window may save it: the document is writable and this
    /// window holds its lease and its file's lock.
    pub writable: bool,
    /// The advisory lock on the file, held while the document is open.
    _lock: Option<std::fs::File>,
}

/// A new score in memory: one treble staff in 4/4, eight measures.
pub fn new_score() -> Result<(Opened, EditorSession), String> {
    let operations = ScoreSetup::single_staff("Piano", 8)
        .operations(ReplicaId::generate())
        .ok_or("the setup is representable")?;
    let document =
        EditorDocument::create(Box::new(MemStore::new()) as Box<dyn BlockStore>, operations)
            .map_err(|e| e.to_string())?;
    lease(document, None, None, None)
}

/// Opens `path`: a saved document, or a MusicXML file imported into a new one.
pub fn open(path: &Path) -> Result<(Opened, EditorSession), String> {
    if is_document(path).map_err(|e| e.to_string())? {
        let lock = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let held = lock.try_lock().is_ok();
        let store = FileStore::open(path).map_err(|e| e.to_string())?;
        let document = EditorDocument::open(Box::new(store) as Box<dyn BlockStore>)
            .map_err(|e| e.to_string())?;
        if !held {
            return view(document, path, "another window holds it");
        }
        lease(document, Some(path.to_path_buf()), None, Some(lock))
    } else {
        let xml = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let import = epiphany_musicxml::import(&xml).map_err(|e| e.to_string())?;
        let scale = import.source.scaling.map(|s| s.staff_space_mm);
        let document = EditorDocument::create(
            Box::new(MemStore::new()) as Box<dyn BlockStore>,
            import.envelopes,
        )
        .map_err(|e| e.to_string())?;
        lease(document, None, scale, None)
    }
}

/// A writable document leased to a session drawn on its own page; a read-only
/// one drawn and not leased.
fn lease(
    mut document: Document,
    path: Option<PathBuf>,
    staff_space_mm: Option<f32>,
    lock: Option<std::fs::File>,
) -> Result<(Opened, EditorSession), String> {
    let engraver = Box::new(Engraver::with_geometry(page_of(&document.score())));
    if document.is_read_only() {
        let session = document.view(engraver).map_err(|e| e.to_string())?;
        return Ok((
            Opened {
                document,
                path,
                staff_space_mm,
                writable: false,
                _lock: lock,
            },
            session,
        ));
    }
    let session = document.lease(engraver).map_err(|e| e.to_string())?;
    Ok((
        Opened {
            document,
            path,
            staff_space_mm,
            writable: true,
            _lock: lock,
        },
        session,
    ))
}

/// A document another window holds, drawn and not leased.
fn view(document: Document, path: &Path, why: &str) -> Result<(Opened, EditorSession), String> {
    let engraver = Box::new(Engraver::with_geometry(page_of(&document.score())));
    let session = document.view(engraver).map_err(|e| format!("{why}: {e}"))?;
    Ok((
        Opened {
            document,
            path: Some(path.to_path_buf()),
            staff_space_mm: None,
            writable: false,
            _lock: None,
        },
        session,
    ))
}

impl Opened {
    /// Saves the session's edits into the document's file. A document with no
    /// file yet is saved with [`save_as`].
    pub fn save(&mut self, session: &mut EditorSession) -> Result<String, String> {
        if !self.writable {
            return Err("the document is read-only here".to_owned());
        }
        let Some(path) = &self.path else {
            return Err("not saved yet: give a path and save as".to_owned());
        };
        let saved = self.document.save(session).map_err(|e| e.to_string())?;
        Ok(format!(
            "saved {} operation(s) to {} (generation {})",
            saved.envelopes,
            path.display(),
            saved.generation
        ))
    }
}

/// Saves the document as a new file at `path`, which must not exist: the
/// session's edits are committed, the document's bytes written there, and the
/// new file opened under a fresh lease, which replaces `opened` and `session`.
/// On an error they are left as they were.
pub fn save_as(
    opened: &mut Opened,
    session: &mut EditorSession,
    path: &Path,
) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} exists; save as a new file", path.display()));
    }
    if opened.writable {
        opened.document.save(session).map_err(|e| e.to_string())?;
    }
    let store = opened.document.bundle().store();
    let mut bytes = vec![0; store.len() as usize];
    store
        .read_exact_at(0, &mut bytes)
        .map_err(|e| e.to_string())?;
    FileStore::create_new(path)
        .and_then(|mut file| {
            file.write_at(0, &bytes)?;
            file.flush()
        })
        .map_err(|e| e.to_string())?;
    // The old document stays open until the new file reopens, so a failure
    // leaves the window as it was.
    let (mut reopened, reopened_session) = open(path)
        .map_err(|e| format!("{} was written but did not reopen: {e}", path.display()))?;
    reopened.staff_space_mm = opened.staff_space_mm;
    *opened = reopened;
    *session = reopened_session;
    Ok(())
}

/// Exports every page of the session's score beside `prefix`: SVGs framed by
/// their pages, and one PDF.
pub fn export_pages(
    opened: &Opened,
    session: &EditorSession,
    prefix: &Path,
) -> Result<String, String> {
    let engraved = Engraved {
        layout: session.resolved().clone(),
        diagnostics: Vec::new(),
        geometry: page_of(session.score()),
        staff_space_mm: opened.staff_space_mm,
        time: std::time::Duration::ZERO,
    };
    let exported =
        export(&engraved, prefix, &ExportOptions::default()).map_err(|e| e.to_string())?;
    let mut message = format!(
        "exported {} page(s) to {}",
        engraved.layout.pages.len(),
        exported
            .paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !exported.overruns.is_empty() {
        message.push_str(&format!(
            "; {} page(s) extended to hold music past the paper",
            exported.overruns.len()
        ));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{CmnNominal, MusicalDuration, MusicalPosition, RationalTime};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "epiphany-gui-file-{name}-{}-{}",
            std::process::id(),
            ReplicaId::generate().0
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../epiphany-musicxml/tests/fixtures")
            .join(name)
    }

    /// `score` with its replica-scoped identity cursor set aside.
    fn graph(score: &epiphany_core::Score) -> epiphany_core::Score {
        let mut score = score.clone();
        score.identity = epiphany_core::IdentityContext::new(ReplicaId(7));
        score
    }

    /// Enters a C at the start of the first voice.
    fn edit(session: &mut EditorSession) {
        let voice = session.score().voices().next().expect("a voice").2.id;
        session
            .set_caret(
                voice,
                MusicalPosition::origin(),
                MusicalDuration(RationalTime::new(1, 4).unwrap()),
            )
            .unwrap();
        session.enter_nominal(CmnNominal::C).unwrap();
    }

    #[test]
    fn a_musicxml_file_opens_edits_saves_as_reopens_and_exports() {
        let dir = scratch("roundtrip");
        let (mut opened, mut session) = open(&fixture("grand_staff.musicxml")).unwrap();
        assert!(opened.writable && opened.path.is_none());
        edit(&mut session);
        assert!(opened.save(&mut session).is_err(), "no file yet");
        let before = graph(session.score());
        let target = dir.join("piece.musc");
        save_as(&mut opened, &mut session, &target).unwrap();
        assert_eq!(opened.path.as_deref(), Some(target.as_path()));
        assert_eq!(
            graph(session.score()),
            before,
            "the edit made before saving as is in the new file"
        );
        // A second edit, saved into the file.
        let voice = session.score().voices().next().unwrap().2.id;
        session
            .set_caret(
                voice,
                MusicalPosition(RationalTime::new(1, 4).unwrap()),
                MusicalDuration(RationalTime::new(1, 4).unwrap()),
            )
            .unwrap();
        session.enter_nominal(CmnNominal::E).unwrap();
        let after = session.score().clone();
        opened.save(&mut session).unwrap();
        drop((opened, session));

        let (opened, session) = open(&target).unwrap();
        assert!(opened.writable);
        assert_eq!(
            graph(session.score()),
            graph(&after),
            "the file reopens as it was saved"
        );
        let message = export_pages(&opened, &session, &dir.join("out")).unwrap();
        assert!(message.starts_with("exported"));
        assert!(dir.join("out.pdf").exists() && dir.join("out-1.svg").exists());
        drop((opened, session));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_document_another_window_holds_opens_read_only() {
        let dir = scratch("locked");
        let (mut first, mut session) = new_score().unwrap();
        let target = dir.join("held.musc");
        save_as(&mut first, &mut session, &target).unwrap();
        assert!(first.writable);
        let (mut second, mut session) = open(&target).unwrap();
        assert!(!second.writable, "the first window holds the lock");
        assert!(second.save(&mut session).is_err());
        drop(first);
        let (third, _) = open(&target).unwrap();
        assert!(third.writable, "the lock is released with its window");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_as_never_overwrites() {
        let dir = scratch("exists");
        let target = dir.join("someone.musc");
        std::fs::write(&target, b"someone's work").unwrap();
        let (mut opened, mut session) = new_score().unwrap();
        let message =
            save_as(&mut opened, &mut session, &target).expect_err("an existing file is refused");
        assert!(message.contains("exists"));
        assert_eq!(std::fs::read(&target).unwrap(), b"someone's work");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
