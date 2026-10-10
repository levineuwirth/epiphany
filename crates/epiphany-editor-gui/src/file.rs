//! The window's document: opened from a MusicXML file or a saved document,
//! saved, saved as, and exported. Kept apart from the window so it can be
//! tested without one.
//!
//! A MusicXML file becomes a new document held in memory, its log the
//! import's operations, until it is saved as a file. A saved document is
//! opened from its file under an advisory lock on that file, so a second
//! window does not write the same document; a document another window holds,
//! or one that opens read-only, is drawn and not leased. Saving commits the
//! session's edits; saving as writes what the window draws into a new
//! document at a new path and opens that under a fresh lease, never writing
//! the file the document came from.

use std::path::{Path, PathBuf};

use epiphany_bundle::{BlockStore, FileStore, MemStore};
use epiphany_cli::{export, is_document, page_of, Engraved, ExportOptions};
use epiphany_core::ReplicaId;
use epiphany_editor_core::{
    DocumentError, EditorDocument, EditorSession, Reconciled, Saved, ScoreSetup,
};
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
    /// file yet is saved with [`save_as`]. A save whose outcome is unknown (its
    /// commit point's flush failed), or one made while an earlier such save is
    /// unsettled, reconciles the document there and then and says how it settled.
    pub fn save(&mut self, session: &mut EditorSession) -> Result<String, String> {
        if !self.writable {
            return Err("the document is read-only here".to_owned());
        }
        let Some(path) = self.path.clone() else {
            return Err("not saved yet: give a path and save as".to_owned());
        };
        match self.document.save(session) {
            Ok(saved) => Ok(saved_to(&path, saved)),
            Err(DocumentError::Indeterminate(e)) => self.reconcile(
                session,
                &path,
                &format!("the save's commit point failed ({e})"),
            ),
            Err(DocumentError::RecoveryRequired) => {
                self.reconcile(session, &path, "an earlier save's outcome was unknown")
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Settles an unsettled save and says how, after `why`: `Ok` when what it
    /// staged is saved, `Err` when it is not, or is still unknown.
    fn reconcile(
        &mut self,
        session: &mut EditorSession,
        path: &Path,
        why: &str,
    ) -> Result<String, String> {
        match self.document.reconcile(session) {
            Ok(Reconciled::Saved {
                saved,
                restored,
                dropped,
            }) => {
                let mut message = format!("{why}; it landed: {}", saved_to(path, saved));
                if restored > 0 {
                    message.push_str(&format!(
                        "; {restored} edit(s) undone since are in the file and drawn again"
                    ));
                }
                if dropped > 0 {
                    message.push_str(&format!(
                        "; {dropped} later edit(s) conflicted with them and were discarded"
                    ));
                }
                if session.is_dirty() {
                    message.push_str("; the edits made since are unsaved");
                }
                Ok(message)
            }
            Ok(Reconciled::NotSaved) => Err(format!(
                "{why}; it did not land: the edits are unsaved, save again"
            )),
            Ok(Reconciled::StillIndeterminate) => Err(format!(
                "{why}; whether it landed is still unknown, and nothing more is saved until it \
                 is: save again to retry"
            )),
            Err(e) => Err(format!(
                "{why}; it could not be settled ({e}): save again to retry"
            )),
        }
    }
}

/// What a save reports.
fn saved_to(path: &Path, saved: Saved) -> String {
    format!(
        "saved {} operation(s) to {} (generation {})",
        saved.envelopes,
        path.display(),
        saved.generation
    )
}

/// Saves what the window draws as a new document at `path`, which must not
/// exist: everything on screen, saved and unsaved edits alike, a read-only
/// window's included, written there under a fresh file identity and opened under
/// a fresh lease, which replaces `opened` and `session`. The file the document
/// was opened from is never written. A document read-only for a reason of its
/// own, rather than because another window holds its file, is refused: its copy
/// could not carry it faithfully. On an error `opened` and `session` are left as
/// they were.
pub fn save_as(
    opened: &mut Opened,
    session: &mut EditorSession,
    path: &Path,
) -> Result<(), String> {
    let copy = opened
        .document
        .save_as(session)
        .map_err(|e| format!("not saved as: {e}"))?;
    let store = FileStore::create_new(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => {
            format!("{} exists; save as a new file", path.display())
        }
        _ => format!("{}: {e}", path.display()),
    })?;
    // The file is this window's from here: one it cannot finish is removed.
    let written = std::fs::File::open(path)
        .map_err(|e| e.to_string())
        .and_then(|lock| match lock.try_lock() {
            Ok(()) => Ok(lock),
            Err(e) => Err(e.to_string()),
        })
        .and_then(|lock| {
            let document = copy
                .create(Box::new(store) as Box<dyn BlockStore>)
                .map_err(|e| e.to_string())?;
            Ok((document, lock))
        });
    let (document, lock) = match written {
        Ok(written) => written,
        Err(e) => {
            let _ = std::fs::remove_file(path);
            return Err(format!("{} could not be written: {e}", path.display()));
        }
    };
    // The old document stays open until the new one is leased, so a failure
    // leaves the window as it was.
    let (reopened, reopened_session) = lease(
        document,
        Some(path.to_path_buf()),
        opened.staff_space_mm,
        Some(lock),
    )
    .map_err(|e| format!("{} was written but did not open: {e}", path.display()))?;
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
        edit_at(session, 0);
    }

    /// Enters a C quarter on beat `beat` (from 0) of the first voice.
    fn edit_at(session: &mut EditorSession, beat: i64) {
        let voice = session.score().voices().next().expect("a voice").2.id;
        session
            .set_caret(
                voice,
                MusicalPosition(RationalTime::new(beat, 4).unwrap()),
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

    /// A store whose chosen flushes fail, the writes they were to make durable
    /// staying readable.
    struct FlakyStore {
        bytes: Vec<u8>,
        flushes: u32,
        fail: Vec<u32>,
    }

    impl BlockStore for FlakyStore {
        fn len(&self) -> u64 {
            self.bytes.len() as u64
        }

        fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
            let end = offset as usize + buf.len();
            buf.copy_from_slice(&self.bytes[offset as usize..end]);
            Ok(())
        }

        fn write_at(&mut self, offset: u64, data: &[u8]) -> std::io::Result<()> {
            let end = offset as usize + data.len();
            if self.bytes.len() < end {
                self.bytes.resize(end, 0);
            }
            self.bytes[offset as usize..end].copy_from_slice(data);
            Ok(())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            if self.fail.contains(&self.flushes) {
                return Err(std::io::Error::other("injected flush failure"));
            }
            Ok(())
        }
    }

    /// A window on a saved new score whose store fails the flushes `fail`
    /// names. A save's commit flushes three times, the third its commit point,
    /// and reconciling flushes once more.
    fn flaky_window(fail: Vec<u32>) -> (Opened, EditorSession) {
        let operations = ScoreSetup::single_staff("Piano", 2)
            .operations(ReplicaId::generate())
            .unwrap();
        let created = EditorDocument::create(MemStore::new(), operations).unwrap();
        let store = FlakyStore {
            bytes: created.into_store().into_bytes(),
            flushes: 0,
            fail,
        };
        let document = EditorDocument::open(Box::new(store) as Box<dyn BlockStore>).unwrap();
        lease(document, Some(PathBuf::from("flaky.musc")), None, None).unwrap()
    }

    #[test]
    fn a_save_whose_outcome_is_unknown_is_settled_and_reported() {
        let (mut opened, mut session) = flaky_window(vec![3]);
        edit(&mut session);
        let message = opened
            .save(&mut session)
            .expect("the save landed, and says so");
        assert!(message.contains("commit point failed"), "{message}");
        assert!(message.contains("it landed"), "{message}");
        assert!(!session.is_dirty());
        assert_eq!(opened.document.generation(), 2);
        edit_at(&mut session, 1);
        let message = opened
            .save(&mut session)
            .expect("the next save is ordinary");
        assert!(message.starts_with("saved"), "{message}");
    }

    #[test]
    fn a_save_still_unsettled_is_settled_at_the_next_save() {
        // The commit point fails, and so does the fresh barrier after it.
        let (mut opened, mut session) = flaky_window(vec![3, 4]);
        edit(&mut session);
        let message = opened.save(&mut session).expect_err("still unknown");
        assert!(message.contains("still unknown"), "{message}");
        assert!(session.is_dirty(), "nothing is claimed saved");
        let message = opened.save(&mut session).expect("the next save settles it");
        assert!(message.contains("it landed"), "{message}");
        assert!(!session.is_dirty());
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

    /// A saved document's generation and number of events, read from its file
    /// without the window's lock.
    fn on_disk(path: &Path) -> (u64, usize) {
        let document = EditorDocument::open(FileStore::open(path).unwrap()).unwrap();
        (
            document.generation(),
            document.score().events.iter().count(),
        )
    }

    /// A saved document's file and document identities.
    fn identity(path: &Path) -> (epiphany_bundle::FileUuid, epiphany_bundle::DocumentId) {
        let document = EditorDocument::open(FileStore::open(path).unwrap()).unwrap();
        (
            document.bundle().file_uuid(),
            document.bundle().manifest().document_id,
        )
    }

    /// A new score saved as `name` in `dir`, the window closed.
    fn saved_score(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let (mut opened, mut session) = new_score().unwrap();
        save_as(&mut opened, &mut session, &path).unwrap();
        path
    }

    #[test]
    fn save_as_writes_a_new_file_and_never_the_one_opened() {
        let dir = scratch("never-source");
        let a = saved_score(&dir, "a.musc");
        let before = on_disk(&a);
        assert_eq!(before, (1, 0));
        let (mut opened, mut session) = open(&a).unwrap();
        edit(&mut session);
        let drawn = graph(session.score());
        let b = dir.join("b.musc");
        save_as(&mut opened, &mut session, &b).unwrap();
        assert_eq!(on_disk(&a), before, "the file opened from is unchanged");
        assert_eq!(on_disk(&b), (1, 1), "the new file holds the edit");
        let (a_file, a_document) = identity(&a);
        let (b_file, b_document) = identity(&b);
        assert_ne!(b_file, a_file, "a fresh file identity");
        assert_eq!(b_document, a_document, "the same work");
        assert_eq!(opened.path.as_deref(), Some(b.as_path()));
        assert!(opened.writable && !session.is_dirty());
        assert_eq!(graph(session.score()), drawn, "the window draws the copy");
        drop((opened, session));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_window_read_only_for_the_lock_saves_its_edits_as_a_new_file() {
        let dir = scratch("held");
        let held = saved_score(&dir, "held.musc");
        let (first, _first_session) = open(&held).unwrap();
        assert!(first.writable);
        let (mut second, mut session) = open(&held).unwrap();
        assert!(!second.writable, "the first window holds the lock");
        edit(&mut session);
        let copy = dir.join("copy.musc");
        save_as(&mut second, &mut session, &copy).expect("the edit goes into the copy");
        assert_eq!(on_disk(&copy), (1, 1), "the copy holds the edit");
        assert_eq!(on_disk(&held), (1, 0), "the held file is unchanged");
        assert!(second.writable, "the copy is the second window's own");
        assert_eq!(session.score().events.iter().count(), 1);
        drop((first, second, session));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_refused_save_as_leaves_the_file_and_the_edits() {
        let dir = scratch("refused");
        let a = saved_score(&dir, "a.musc");
        let existing = dir.join("existing.musc");
        std::fs::write(&existing, b"someone's work").unwrap();
        let (mut opened, mut session) = open(&a).unwrap();
        edit(&mut session);
        let before = on_disk(&a);
        save_as(&mut opened, &mut session, &existing).expect_err("an existing file is refused");
        assert_eq!(on_disk(&a), before, "a refused save as writes nothing");
        assert_eq!(std::fs::read(&existing).unwrap(), b"someone's work");
        assert_eq!(opened.path.as_deref(), Some(a.as_path()));
        assert!(session.is_dirty(), "the edit is still the window's");
        drop((opened, session));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_document_read_only_of_itself_is_not_saved_as() {
        use epiphany_bundle::{
            Bundle, BundleCapabilities, ExtensionDeclaration, ExtensionId,
            ReductionAlgorithmVersion, SemVer,
        };
        let dir = scratch("read-only");
        let path = saved_score(&dir, "extended.musc");
        // A manifest declaring an extension: the document opens read-only.
        let caps = BundleCapabilities {
            current_reduction_version: ReductionAlgorithmVersion(
                epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION,
            ),
        };
        let mut bundle = Bundle::open(FileStore::open(&path).unwrap(), caps).unwrap();
        bundle
            .commit(&[], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.extension_declarations.push(ExtensionDeclaration {
                    extension_id: ExtensionId([5; 16]),
                    version: SemVer {
                        major: 1,
                        minor: 0,
                        patch: 0,
                    },
                    required: false,
                    preserved_chunk_roots: Vec::new(),
                    affected_object_kinds: Vec::new(),
                    edit_barriers: Vec::new(),
                });
                m
            })
            .unwrap();
        drop(bundle);
        let (mut opened, mut session) = open(&path).unwrap();
        assert!(!opened.writable && opened.document.is_read_only());
        let target = dir.join("copy.musc");
        let message = save_as(&mut opened, &mut session, &target)
            .expect_err("its copy would not carry the extension");
        assert!(message.contains("read-only"), "{message}");
        assert!(!target.exists(), "nothing is written");
        drop((opened, session));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
