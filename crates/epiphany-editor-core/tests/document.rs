//! The document layer: a score created, written, edited, undone, saved, reopened
//! and edited again, through `EditorDocument` and the session it leases.

use std::io;

use epiphany_bundle::{
    encode_block, BlobId, BlobRef, BlockStore, Bundle, BundleCapabilities, ChunkKind,
    CompressionAlgorithm, DocumentId, ExtensionDeclaration, ExtensionId, FailedFlush, FileUuid,
    Manifest, MemStore, PageCacheStore, ReductionAlgorithmVersion, SchemaVersion, SemVer,
    StagedChunk,
};
use epiphany_core::{
    CmnNominal, EventDuration, EventPosition, IdentityContext, MusicalDuration, MusicalPosition,
    RationalTime, ReplicaId, Score, VoiceId,
};
use epiphany_determinism::CanonicalEncode;
use epiphany_editor_core::{
    DocumentError, EditorDocument, EditorError, EditorSession, ReadOnlyReason, Reconciled, Saved,
    ScoreSetup,
};
use epiphany_layout_ir::StubSolver;
use epiphany_ops::{
    BlockStamp, OperationEnvelope, OperationSet, CURRENT_REDUCTION_ALGORITHM_VERSION,
    HIGHEST_SCHEMA_MINOR,
};

fn whole(n: i64, d: i64) -> MusicalDuration {
    MusicalDuration(RationalTime::new(n, d).expect("a valid duration"))
}

fn at(n: i64, d: i64) -> MusicalPosition {
    MusicalPosition(RationalTime::new(n, d).expect("a valid position"))
}

fn setup_operations(measures: u32) -> Vec<OperationEnvelope> {
    ScoreSetup::single_staff("Flute", measures)
        .operations(ReplicaId::generate())
        .expect("4/4 in C is representable")
}

fn lease(document: &mut EditorDocument<impl BlockStore>) -> EditorSession {
    document
        .lease(Box::new(StubSolver))
        .expect("a writable document grants a lease")
}

/// The one voice of a single-staff score.
fn the_voice(session: &EditorSession) -> VoiceId {
    let (_, _, voice) = session
        .score()
        .voices()
        .next()
        .expect("the score has a voice");
    voice.id
}

/// Every event of the score as (onset, duration, pitched), in time order: the
/// music, independent of the ids each session minted.
fn music(score: &Score) -> Vec<(MusicalPosition, MusicalDuration, bool)> {
    let mut out: Vec<_> = score
        .events
        .iter()
        .filter_map(|event| match (event.position(), event.duration()) {
            (EventPosition::Musical(p), EventDuration::Musical(d)) => Some((
                p.clone(),
                d.clone(),
                matches!(event, epiphany_core::Event::Pitched(_)),
            )),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The canonical reducer state of `envelopes` reduced from an empty score, and
/// the score itself with its replica-scoped identity cursor set aside (Ruling
/// on genesis persistence §3: byte equality is claimed of the reduced state, and
/// the score's `identity` names whichever replica reduced it).
fn reduced(envelopes: &[OperationEnvelope]) -> (Vec<u8>, Score) {
    let mut set = OperationSet::new();
    for envelope in envelopes {
        set.accept(envelope.clone());
    }
    let identity = IdentityContext::new(ReplicaId(7));
    let out = set.reduce_onto(&Score::empty(identity.clone()));
    assert!(out.state.is_clean(), "the document reduces cleanly");
    let mut score = out.score;
    score.identity = identity;
    (out.state.canonical_bytes(), score)
}

/// The committed and applied envelopes of a session: what it materializes.
fn session_log(session: &EditorSession) -> Vec<OperationEnvelope> {
    session
        .committed_operations()
        .iter()
        .chain(session.applied_operations())
        .cloned()
        .collect()
}

/// The envelope set, as sorted (id, hash) pairs.
fn membership(envelopes: &[OperationEnvelope]) -> Vec<String> {
    let mut out: Vec<String> = envelopes
        .iter()
        .map(|e| format!("{:?} {}", e.id, e.envelope_hash().to_hex()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Enters the exercise's phrase into a two-measure score of 4/4: mixed durations
/// and rests, running across the barline at whole note 1 — quarter C, eighths D
/// and E, a quarter rest, and a half F on beat 4 that crosses the barline and is
/// split there and tied; then a quarter G, a quarter rest and eighths A and B.
fn enter_phrase(session: &mut EditorSession) {
    let voice = the_voice(session);
    session
        .set_caret(voice, at(0, 1), whole(1, 4))
        .expect("the caret goes at the score's start");
    let steps: [(Option<CmnNominal>, (i64, i64)); 9] = [
        (Some(CmnNominal::C), (1, 4)),
        (Some(CmnNominal::D), (1, 8)),
        (Some(CmnNominal::E), (1, 8)),
        (None, (1, 4)),
        (Some(CmnNominal::F), (1, 2)),
        (Some(CmnNominal::G), (1, 4)),
        (None, (1, 4)),
        (Some(CmnNominal::A), (1, 8)),
        (Some(CmnNominal::B), (1, 8)),
    ];
    for (nominal, (n, d)) in steps {
        session
            .set_entry_duration(whole(n, d))
            .expect("a positive duration");
        let outcome = match nominal {
            Some(nominal) => session.enter_nominal(nominal),
            None => session.enter_rest(),
        }
        .expect("the entry applies");
        assert!(outcome.graph_changed);
    }
    assert_eq!(session.caret().map(|c| c.position), Some(at(2, 1)));
    assert_eq!(
        music(session.score()),
        vec![
            (at(0, 1), whole(1, 4), true),
            (at(1, 4), whole(1, 8), true),
            (at(3, 8), whole(1, 8), true),
            (at(1, 2), whole(1, 4), false),
            (at(3, 4), whole(1, 4), true),
            (at(1, 1), whole(1, 4), true),
            (at(5, 4), whole(1, 4), true),
            (at(3, 2), whole(1, 4), false),
            (at(7, 4), whole(1, 8), true),
            (at(15, 8), whole(1, 8), true),
        ],
        "the phrase, its half F split at the barline"
    );
    assert_eq!(session.score().cross_cutting.ties.len(), 1, "and tied");
}

/// The number of measures on each staff.
fn measures(score: &Score) -> Vec<usize> {
    score
        .staff_instances()
        .map(|(_, si)| si.measures.len())
        .collect()
}

/// The exercise, over any store: create, enter the phrase, edit, undo, save,
/// reopen, and continue; the saved set, the reducer state and the graph are
/// asserted after each reopen. Returns the store after the second save.
fn exercise<S: BlockStore>(store: S, reopen: impl Fn(S) -> S) -> S {
    let mut document =
        EditorDocument::create(store, setup_operations(2)).expect("the document is created");
    assert!(!document.is_read_only());
    assert_eq!(
        document.generation(),
        1,
        "the setup is the first generation"
    );
    let mut session = lease(&mut document);
    enter_phrase(&mut session);

    // An edit: overwrite the quarter rest on beat 2 of the second measure with D.
    let voice = the_voice(&session);
    session
        .set_caret(voice, at(3, 2), whole(1, 4))
        .expect("the caret moves back");
    let before_edit = music(session.score());
    session
        .enter_nominal(CmnNominal::D)
        .expect("the overwrite applies");
    assert_ne!(music(session.score()), before_edit);
    // Undo it.
    session.undo().expect("the overwrite undoes");
    assert_eq!(music(session.score()), before_edit);
    assert!(session.is_dirty());

    let applied = session.applied_operations().len();
    let saved = document.save(&mut session).expect("the save commits");
    assert_eq!(saved.generation, 2);
    assert_eq!(
        saved.envelopes, applied,
        "the save commits exactly the applied units"
    );
    assert!(!session.is_dirty(), "a save leaves the session clean");
    assert!(!session.can_undo(), "undo stops at the save");
    assert!(session.can_redo(), "the undone overwrite stays redoable");

    let expected_log = session_log(&session);
    let (expected_state, expected_score) = reduced(&expected_log);
    assert_eq!(&expected_score, &{
        let mut s = session.score().clone();
        s.identity = IdentityContext::new(ReplicaId(7));
        s
    });

    assert_blocks_stamped_minimally(document.bundle());

    // Reopen from the stored bytes alone.
    let store = reopen(document.into_store());
    let mut document = EditorDocument::open(store).expect("the saved document reopens");
    assert_eq!(document.generation(), 2);
    assert_eq!(
        membership(document.committed_operations()),
        membership(&expected_log),
        "the reopened document holds exactly the saved envelopes"
    );
    let mut reopened = lease(&mut document);
    let (state, score) = reduced(&session_log(&reopened));
    assert_eq!(
        state, expected_state,
        "the reducer state survives the reopen"
    );
    assert_eq!(score, expected_score, "the graph survives the reopen");
    assert_eq!(music(reopened.score()), music(session.score()));
    assert_ne!(
        reopened.replica(),
        session.replica(),
        "each lease mints under a fresh replica"
    );

    // Continue past the last measure: a half E opens a third measure, and a
    // dotted half F after it crosses into a fourth, split and tied.
    let voice = the_voice(&reopened);
    reopened
        .set_caret(voice, at(2, 1), whole(1, 2))
        .expect("the caret goes at the end");
    reopened
        .enter_nominal(CmnNominal::E)
        .expect("the reopened document takes an edit");
    reopened.set_entry_duration(whole(3, 4)).unwrap();
    reopened.enter_nominal(CmnNominal::F).unwrap();
    assert_eq!(measures(reopened.score()), vec![4]);
    let continued = music(reopened.score());
    assert_eq!(
        continued[10..],
        [
            (at(2, 1), whole(1, 2), true),
            (at(5, 2), whole(1, 2), true),
            (at(3, 1), whole(1, 4), true),
        ]
    );
    assert_eq!(reopened.score().cross_cutting.ties.len(), 2);
    let saved = document
        .save(&mut reopened)
        .expect("the second save commits");
    assert_eq!(saved.generation, 3);
    let expected_log = session_log(&reopened);
    let (expected_state, expected_score) = reduced(&expected_log);

    let store = reopen(document.into_store());
    let document = EditorDocument::open(store).expect("the document reopens again");
    assert_eq!(
        membership(document.committed_operations()),
        membership(&expected_log)
    );
    let (state, score) = reduced(document.committed_operations());
    assert_eq!(state, expected_state);
    assert_eq!(score, expected_score);
    let view = document
        .view(Box::new(StubSolver))
        .expect("the document draws");
    assert_eq!(music(view.score()), continued);
    assert_eq!(measures(view.score()), vec![4]);
    document.into_store()
}

/// Every operation block is stamped with the highest schema major among its
/// own envelopes, at the epoch of the newest discriminant they carry: minimal
/// stamping, so a reader is never told a block needs less than it does, nor more.
fn assert_blocks_stamped_minimally<S: BlockStore>(bundle: &Bundle<S>) {
    for root in &bundle.manifest().operation_roots {
        let envelopes: Vec<OperationEnvelope> = bundle
            .read_operation_block(root)
            .expect("the block reads")
            .iter()
            .map(|bytes| epiphany_ops::decode_envelope(bytes).expect("decodes"))
            .collect();
        let major = envelopes
            .iter()
            .map(OperationEnvelope::schema_major)
            .max()
            .expect("a block holds an envelope");
        assert_eq!(
            root.schema_version,
            SchemaVersion::for_major_at_epoch(
                major,
                epiphany_ops::operation_block_introduced_minor(&envelopes)
            ),
            "a block is stamped with its envelopes' major"
        );
    }
}

#[test]
fn the_authoring_exercise_survives_save_and_reopen_in_memory() {
    exercise(MemStore::new(), |store| {
        MemStore::from_bytes(store.into_bytes())
    });
}

#[cfg(unix)]
#[test]
fn the_authoring_exercise_survives_save_and_reopen_in_a_file() {
    use epiphany_bundle::FileStore;
    let path = std::env::temp_dir().join(format!(
        "epiphany-document-exercise-{}-{}.musc",
        std::process::id(),
        ReplicaId::generate().0
    ));
    let store = FileStore::create_new(&path).expect("the file is created");
    let reopen_path = path.clone();
    let store = exercise(store, move |store| {
        drop(store);
        FileStore::open(&reopen_path).expect("the file reopens")
    });
    drop(store);
    std::fs::remove_file(&path).expect("the test file is removed");
}

#[test]
fn a_session_not_leased_from_the_document_is_not_saved() {
    let mut document =
        EditorDocument::create(MemStore::new(), setup_operations(1)).expect("created");
    let mut other = EditorDocument::create(MemStore::new(), setup_operations(1)).expect("created");

    // A probe-mode session over the same score.
    let view = document.view(Box::new(StubSolver)).expect("draws");
    let mut probe =
        EditorSession::open(view.score().clone(), Box::new(StubSolver)).expect("renders");
    assert!(matches!(
        document.save(&mut probe),
        Err(DocumentError::NotLeased)
    ));
    // A view is not leased either.
    let mut view = view;
    assert!(matches!(
        document.save(&mut view),
        Err(DocumentError::NotLeased)
    ));

    // A session leased from another document.
    let mut foreign = lease(&mut other);
    assert!(matches!(
        document.save(&mut foreign),
        Err(DocumentError::ForeignLease)
    ));

    // A lease revoked by a later one.
    let mut first = lease(&mut document);
    let voice = the_voice(&first);
    first.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
    first.enter_nominal(CmnNominal::C).unwrap();
    let mut second = lease(&mut document);
    assert!(matches!(
        document.save(&mut first),
        Err(DocumentError::LeaseRevoked)
    ));
    assert_eq!(document.generation(), 1, "nothing was committed");
    assert!(document.save(&mut second).is_ok());
}

#[test]
fn a_redo_after_a_save_applies_and_saves() {
    let mut document =
        EditorDocument::create(MemStore::new(), setup_operations(1)).expect("created");
    let mut session = lease(&mut document);
    let voice = the_voice(&session);
    session.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    session.enter_nominal(CmnNominal::D).unwrap();
    session.undo().expect("D undoes");
    document.save(&mut session).expect("saves C");
    assert!(!session.can_undo());
    session.redo().expect("D redoes over the saved C");
    assert!(session.is_dirty());
    session.undo().expect("and undoes again, back to the save");
    assert!(!session.is_dirty());
    session.redo().expect("redoes once more");
    document.save(&mut session).expect("saves D");
    let store = document.into_store();
    let document = EditorDocument::open(MemStore::from_bytes(store.into_bytes())).unwrap();
    let view = document.view(Box::new(StubSolver)).unwrap();
    assert_eq!(music(view.score()).len(), 2, "C and D were both saved");
}

#[test]
fn the_first_edit_after_a_reopen_extends_the_stored_frontier() {
    let mut document =
        EditorDocument::create(MemStore::new(), setup_operations(1)).expect("created");
    let committed: Vec<_> = document
        .committed_operations()
        .iter()
        .map(|e| e.id)
        .collect();
    let mut session = lease(&mut document);
    let voice = the_voice(&session);
    session.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    let first = session.applied_operations()[0].clone();
    for id in committed {
        assert!(
            first.causal_context.covers(id),
            "the first edit covers committed {id:?}"
        );
    }
}

/// A store holding one operation block of `payloads`, stamped `declared`: what a
/// later or a non-conforming writer might have left.
fn store_with_block(payloads: &[Vec<u8>], declared: SchemaVersion) -> MemStore {
    let mut bundle = Bundle::create(
        MemStore::new(),
        FileUuid([1; 16]),
        Manifest::empty(DocumentId([2; 16])),
        caps(),
    )
    .unwrap();
    bundle
        .commit_versioned(
            &[StagedChunk::operation_block_versioned(
                encode_block(payloads),
                declared,
            )],
            bundle.superblock().manifest_schema_version,
            |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots.extend(ctx.new_chunks.iter().copied());
                m
            },
        )
        .unwrap();
    MemStore::from_bytes(bundle.into_store().into_bytes())
}

/// The canonical bytes of each of `operations`, and the stamp minimal stamping
/// derives for a block of them.
fn block_of(operations: &[OperationEnvelope]) -> (Vec<Vec<u8>>, BlockStamp) {
    (
        operations
            .iter()
            .map(CanonicalEncode::to_canonical_bytes)
            .collect(),
        BlockStamp::derive(operations),
    )
}

/// The decode corpus's vector `surface`/`name`, from the committed file.
fn corpus_vector(surface: &str, name: &str) -> Vec<u8> {
    let line = include_str!("../../../spec/vectors/decode_vectors.txt")
        .lines()
        .find(|line| {
            let mut fields = line.split(' ');
            fields.next() == Some(surface) && fields.nth(2) == Some(name)
        })
        .unwrap_or_else(|| panic!("the corpus carries {surface}/{name}"));
    let hex = line.rsplit(' ').next().expect("a hex field");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

/// A document holding an envelope in a historical layout is refused by name:
/// its block's declared version, and the value whose layout was recognized.
#[test]
fn a_historical_layout_is_refused_by_name() {
    let historical = epiphany_ops::vectors::decode_vectors()
        .into_iter()
        .find(|(_, _, _, name, _)| name == "create_tuplet_before_major_4")
        .map(|(.., bytes)| bytes)
        .expect("the ops corpus carries the vector");
    assert!(matches!(
        EditorDocument::open(store_with_block(&[historical], SchemaVersion::for_major(3))),
        Err(DocumentError::UnsupportedLayout {
            declared: BlockStamp { major: 3, minor: 0 },
            value: Some("Tuplet"),
        })
    ));
}

/// An envelope this build cannot decode at all, in a block whose hash verified,
/// is refused by its block's version, no value named.
#[test]
fn an_undecodable_envelope_is_refused_by_its_block_s_version() {
    let (mut payloads, stamp) = block_of(&setup_operations(1));
    payloads[0].pop();
    let declared = SchemaVersion::new(stamp.major, stamp.minor);
    assert!(matches!(
        EditorDocument::open(store_with_block(&payloads, declared)),
        Err(DocumentError::UnsupportedLayout { declared: d, value: None }) if d == stamp
    ));
}

/// A block from a later build, its minor past this build's last epoch, is
/// refused by name before its envelopes are read.
#[test]
fn a_block_of_a_later_minor_is_refused_by_name() {
    let (payloads, stamp) = block_of(&setup_operations(1));
    let later = HIGHEST_SCHEMA_MINOR + 1;
    assert!(matches!(
        EditorDocument::open(store_with_block(&payloads, SchemaVersion::new(stamp.major, later))),
        Err(DocumentError::UnsupportedSchemaMinor { major, minor })
            if major == stamp.major && minor == later
    ));
    // The same operations stamped as derived open.
    let document = EditorDocument::open(store_with_block(
        &payloads,
        SchemaVersion::new(stamp.major, stamp.minor),
    ))
    .expect("a block stamped as derived opens");
    assert!(
        !document.is_read_only(),
        "{:?}",
        document.read_only_reasons()
    );
}

/// A block stamped above or below what its envelopes derive is refused by name,
/// though every envelope in it decodes.
#[test]
fn a_mis_stamped_block_is_refused_by_name() {
    let (payloads, derived) = block_of(&setup_operations(1));
    for declared in [
        BlockStamp {
            major: derived.major + 1,
            ..derived
        },
        BlockStamp {
            major: derived.major - 1,
            ..derived
        },
        BlockStamp {
            minor: derived.minor + 1,
            ..derived
        },
    ] {
        let store = store_with_block(
            &payloads,
            SchemaVersion::new(declared.major, declared.minor),
        );
        assert!(matches!(
            EditorDocument::open(store),
            Err(DocumentError::MisStamped { declared: d, derived: e }) if d == declared && e == derived
        ));
    }
}

/// A legacy (format 0) container, which the bundle layer still decodes, is
/// refused by name: the decode corpus's `document.container` vectors, read
/// through the document layer as the text projection reads them.
#[test]
fn a_legacy_container_is_refused_by_name() {
    let legacy = corpus_vector("document.container", "empty_legacy");
    let bundle = Bundle::open(MemStore::from_bytes(legacy.clone()), caps())
        .expect("the bundle layer decodes a legacy container");
    assert_eq!(bundle.header().format_major, 0);
    assert!(matches!(
        EditorDocument::open(MemStore::from_bytes(legacy)),
        Err(DocumentError::LegacyContainer)
    ));
    let current = corpus_vector("document.container", "empty_current");
    EditorDocument::open(MemStore::from_bytes(current)).expect("the current container opens");
}

fn caps() -> BundleCapabilities {
    BundleCapabilities {
        current_reduction_version: ReductionAlgorithmVersion(CURRENT_REDUCTION_ALGORITHM_VERSION),
    }
}

/// An extension-bearing document opens read-only: it draws, and grants no lease.
#[test]
fn an_extension_bearing_document_is_read_only() {
    let document = EditorDocument::create(MemStore::new(), setup_operations(1)).expect("created");
    let mut bundle = Bundle::open(document.into_store(), caps()).unwrap();
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
    let mut document =
        EditorDocument::open(MemStore::from_bytes(bundle.into_store().into_bytes())).unwrap();
    assert_eq!(document.read_only_reasons(), &[ReadOnlyReason::Extensions]);
    assert!(matches!(
        document.lease(Box::new(StubSolver)),
        Err(DocumentError::ReadOnly(_))
    ));
    assert!(document.view(Box::new(StubSolver)).is_ok());
}

/// A store whose chosen flushes fail, a page cache over a disk
/// ([`PageCacheStore`]). `keep` decides what a failed flush leaves: with it, the
/// writes it was to make durable stay readable but are marked clean, so they
/// reach the disk only if written again (ext4 and XFS); without it, reads see
/// the disk again (Btrfs). `unreadable` reads fail after each failed flush.
struct FlakyStore {
    cache: PageCacheStore,
    unreadable: u32,
    refuse: std::cell::Cell<u32>,
}

impl FlakyStore {
    fn new(image: Vec<u8>, fail: Vec<u32>, keep: bool) -> Self {
        let on_failure = if keep {
            FailedFlush::MarksClean
        } else {
            FailedFlush::Reverts
        };
        FlakyStore {
            cache: PageCacheStore::new(image, fail, on_failure),
            unreadable: 0,
            refuse: std::cell::Cell::new(0),
        }
    }
}

impl BlockStore for FlakyStore {
    fn len(&self) -> u64 {
        self.cache.len()
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if self.refuse.get() > 0 {
            self.refuse.set(self.refuse.get() - 1);
            return Err(io::Error::other("injected read failure"));
        }
        self.cache.read_exact_at(offset, buf)
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.cache.write_at(offset, data)
    }

    fn flush(&mut self) -> io::Result<()> {
        let flushed = self.cache.flush();
        if flushed.is_err() {
            self.refuse.set(self.unreadable);
        }
        flushed
    }
}

/// A one-measure document over a flaky store, with a session holding one entry.
fn flaky_document(fail: Vec<u32>, keep: bool) -> (EditorDocument<FlakyStore>, EditorSession) {
    let created = EditorDocument::create(MemStore::new(), setup_operations(1)).unwrap();
    let image = created.into_store().into_bytes();
    let mut document = EditorDocument::open(FlakyStore::new(image, fail, keep)).unwrap();
    let mut session = lease(&mut document);
    let voice = the_voice(&session);
    session.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    (document, session)
}

/// The file as the disk holds it: what a crash, or a page evicted and read
/// again, leaves of `document`'s store.
fn on_disk(document: &EditorDocument<FlakyStore>) -> EditorDocument<MemStore> {
    let disk = document.bundle().store().cache.durable().to_vec();
    EditorDocument::open(MemStore::from_bytes(disk)).expect("the disk opens")
}

/// The file reopened from `document`'s store as the store reads it, checked
/// against the file on disk: once a save has settled, the two hold one
/// generation and one set of envelopes, and the disk's opens writable.
fn reopened(document: &EditorDocument<FlakyStore>) -> EditorDocument<MemStore> {
    let live = document.bundle().store().cache.live();
    let file = EditorDocument::open(MemStore::from_bytes(live)).expect("the file reopens");
    let disk = on_disk(document);
    assert_eq!(
        disk.generation(),
        file.generation(),
        "the disk holds the generation the store reads"
    );
    assert_eq!(
        membership(disk.committed_operations()),
        membership(file.committed_operations())
    );
    assert!(
        disk.read_only_reasons().is_empty(),
        "the file on disk opens writable: {:?}",
        disk.read_only_reasons()
    );
    file
}

// A save's commit runs three flushes: after the chunks, after the manifest, and
// at the superblock, the commit point.

/// A commit whose chunks' or manifest's flush fails saves nothing, and the next
/// save commits. It appends its chunks and manifest where the failed commit did
/// and writes its superblock into the same slot, so everything it names is
/// written again and reaches the disk, whether the failed flush left those
/// writes readable or not.
#[test]
fn a_commit_failing_before_its_commit_point_saves_nothing() {
    for (fail, keep) in [(1, true), (1, false), (2, true), (2, false)] {
        let (mut document, mut session) = flaky_document(vec![fail], keep);
        assert!(matches!(
            document.save(&mut session),
            Err(DocumentError::Bundle(_))
        ));
        assert!(
            session.is_dirty() && session.can_undo(),
            "the units stay unsaved"
        );
        assert_eq!(document.generation(), 1);
        // The store recovers, and the next save commits.
        let saved = document.save(&mut session).expect("a later save commits");
        assert_eq!(saved.generation, 2);
        let file = reopened(&document);
        assert_eq!(file.generation(), 2, "flush {fail} failed, keep {keep}");
        assert_eq!(
            reduced(file.committed_operations()),
            reduced(&session_log(&session))
        );
    }
}

#[test]
fn an_indeterminate_commit_that_landed_is_saved_after_a_fresh_barrier() {
    let (mut document, mut session) = flaky_document(vec![3], true);
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    assert!(session.is_dirty(), "nothing is promoted while unknown");
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::RecoveryRequired)
    ));
    let outcome = document.reconcile(&mut session).expect("reopens");
    assert!(matches!(outcome, Reconciled::Saved(saved) if saved.generation == 2));
    assert!(!session.is_dirty() && !session.can_undo());
    assert_eq!(document.generation(), 2);
    assert_eq!(reopened(&document).generation(), 2);
}

#[test]
fn an_indeterminate_commit_that_did_not_land_leaves_the_units_unsaved() {
    let (mut document, mut session) = flaky_document(vec![3], false);
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    let outcome = document.reconcile(&mut session).expect("reopens");
    assert_eq!(outcome, Reconciled::NotSaved);
    assert!(session.is_dirty() && session.can_undo());
    assert_eq!(document.generation(), 1);
    let saved = document.save(&mut session).expect("the units save later");
    assert_eq!(saved.generation, 2);
    assert_eq!(reopened(&document).generation(), 2);
}

/// Review 2's F8: the commit point's flush fails on ext4 or XFS, which leave
/// the new superblock readable and mark its page clean, so the disk keeps the
/// generation before. A reconcile reports the save landed only once that
/// superblock is on disk, and the next save leaves a disk that opens writable,
/// holding both notes. A bare barrier wrote nothing: the save was reported
/// landed with the disk a generation behind, and the next left the disk's slots
/// three generations apart, read-only.
#[test]
fn a_save_reconciled_as_landed_is_on_disk_though_its_failed_flush_left_it_readable() {
    let (mut document, mut session) = flaky_document(vec![3], true);
    let staged = session.applied_operations().len();
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    assert_eq!(
        on_disk(&document).generation(),
        1,
        "the disk keeps the generation before"
    );

    let outcome = document.reconcile(&mut session).expect("reconciles");
    assert_eq!(
        outcome,
        Reconciled::Saved(Saved {
            generation: 2,
            envelopes: staged,
        })
    );
    let disk = on_disk(&document);
    assert_eq!(disk.generation(), 2, "what is reported saved is on disk");
    assert_eq!(
        reduced(disk.committed_operations()),
        reduced(&session_log(&session))
    );

    session.enter_nominal(CmnNominal::D).unwrap();
    let saved = document.save(&mut session).expect("the next save");
    assert_eq!(saved.generation, 3);
    let disk = on_disk(&document);
    assert_eq!(disk.generation(), 3);
    assert!(
        disk.read_only_reasons().is_empty(),
        "the disk opens writable: {:?}",
        disk.read_only_reasons()
    );
    assert_eq!(music(&disk.score()), music(session.score()));
    assert_eq!(music(&disk.score()).len(), 2, "holding both notes");
}

/// Review 3's E1: the failed commit point leaves the new superblock readable
/// and its page clean, and a clean page is readable only until it is evicted.
/// Evicted once the reopen has read both slots (its reads 1 to 3 after the
/// failure: the header and the slots) and selected the staged generation, the
/// page is read again from the disk, which holds the superblock before. The
/// barrier writes the superblock the reopen selected, so the save reported
/// landed is on disk and the next save's disk opens writable, whether the store
/// writes back by byte or by page. Rewriting the slot as read a second time put
/// the disk's own superblock back: the save was reported landed with the disk a
/// generation behind, and the next left it read-only. Evicted before the
/// reopen's first read, the page shows the generation before, and the save did
/// not land.
#[test]
fn a_save_reconciled_as_landed_is_on_disk_though_its_clean_page_was_evicted() {
    for unit in [1, 4096] {
        for (evict_before, landed) in [(1, false), (4, true)] {
            let case = format!("unit {unit}, evicted before read {evict_before}");
            let created = EditorDocument::create(MemStore::new(), setup_operations(1)).unwrap();
            let store = FlakyStore {
                cache: PageCacheStore::new(
                    created.into_store().into_bytes(),
                    vec![3],
                    FailedFlush::MarksClean,
                )
                .with_unit(unit)
                .evicting_before(vec![evict_before]),
                unreadable: 0,
                refuse: std::cell::Cell::new(0),
            };
            let mut document = EditorDocument::open(store).unwrap();
            let mut session = lease(&mut document);
            let voice = the_voice(&session);
            session.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
            session.enter_nominal(CmnNominal::C).unwrap();
            let staged = session.applied_operations().len();
            assert!(matches!(
                document.save(&mut session),
                Err(DocumentError::Indeterminate(_))
            ));

            let outcome = document.reconcile(&mut session).expect("reconciles");
            assert_eq!(
                document.bundle().store().cache.evictions(),
                1,
                "{case}: evicted within the reconcile"
            );
            let disk = on_disk(&document);
            if landed {
                assert_eq!(
                    outcome,
                    Reconciled::Saved(Saved {
                        generation: 2,
                        envelopes: staged,
                    }),
                    "{case}"
                );
                assert_eq!(
                    disk.generation(),
                    2,
                    "{case}: what is reported saved is on disk"
                );
                assert_eq!(
                    reduced(disk.committed_operations()),
                    reduced(&session_log(&session))
                );
            } else {
                assert_eq!(outcome, Reconciled::NotSaved, "{case}");
                assert_eq!(disk.generation(), 1, "{case}");
            }

            session.enter_nominal(CmnNominal::D).unwrap();
            let saved = document.save(&mut session).expect("the next save");
            assert_eq!(saved.generation, if landed { 3 } else { 2 }, "{case}");
            let file = reopened(&document);
            assert_eq!(file.generation(), saved.generation, "{case}");
            assert_eq!(music(&file.score()), music(session.score()), "{case}");
            assert_eq!(music(&file.score()).len(), 2, "{case}: holding both notes");
        }
    }
}

/// The fresh barrier fails too. With the selected superblock written again it
/// has something to write, so its failure is reported and the save stays
/// unsettled, the disk a generation behind, until a barrier succeeds; then the
/// save is on disk, and so is the next.
#[test]
fn a_failing_fresh_barrier_stays_indeterminate() {
    let (mut document, mut session) = flaky_document(vec![3, 4], true);
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    let outcome = document.reconcile(&mut session).expect("reopens");
    assert_eq!(outcome, Reconciled::StillIndeterminate);
    assert!(
        session.is_dirty() && session.has_unsettled_save(),
        "neither saved nor unsaved is claimed"
    );
    assert_eq!(on_disk(&document).generation(), 1, "the disk is behind");
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::RecoveryRequired)
    ));
    let outcome = document.reconcile(&mut session).expect("reopens");
    assert!(
        matches!(outcome, Reconciled::Saved(saved) if saved.generation == 2),
        "a later barrier settles it"
    );
    assert_eq!(reopened(&document).generation(), 2, "on disk");
    session.enter_nominal(CmnNominal::D).unwrap();
    document.save(&mut session).expect("the next save");
    let file = reopened(&document);
    assert_eq!(file.generation(), 3);
    assert_eq!(music(&file.score()), music(session.score()));
}

/// An indeterminate save that landed, with two notes entered before it is
/// reconciled: reconciling promotes exactly the one note the save staged, and
/// the two entered since stay applied, unsaved and undoable. The file holds what
/// is reported saved, and once the later notes are saved too it holds what the
/// session draws.
#[test]
fn reconciling_promotes_exactly_what_the_indeterminate_save_staged() {
    let (mut document, mut session) = flaky_document(vec![3], true);
    let staged = session.applied_operations().to_vec();
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    session.enter_nominal(CmnNominal::D).unwrap();
    session.enter_nominal(CmnNominal::E).unwrap();
    let later = session.applied_operations()[staged.len()..].to_vec();
    let on_screen = music(session.score());
    assert_eq!(on_screen.len(), 3);

    let outcome = document.reconcile(&mut session).expect("reconciles");
    assert_eq!(
        outcome,
        Reconciled::Saved(Saved {
            generation: 2,
            envelopes: staged.len(),
        }),
        "exactly the staged envelopes are reported saved"
    );
    assert_eq!(
        membership(session.applied_operations()),
        membership(&later),
        "the notes entered since stay applied"
    );
    assert!(session.is_dirty(), "and unsaved");
    assert_eq!(music(session.score()), on_screen, "nothing moves on screen");

    // The file holds exactly the session's committed partition.
    let file = reopened(&document);
    assert_eq!(
        membership(file.committed_operations()),
        membership(session.committed_operations()),
        "what is reported saved is in the file, and nothing more"
    );
    assert_eq!(
        reduced(file.committed_operations()),
        reduced(session.committed_operations())
    );

    // The later notes undo, back to the save, and redo.
    session.undo().expect("E undoes");
    session.undo().expect("D undoes");
    assert!(!session.can_undo(), "undo stops at what was saved");
    assert_eq!(music(session.score()), music(&file.score()));
    session.redo().unwrap();
    session.redo().unwrap();

    // Saved too, the file holds what the session draws.
    let saved = document.save(&mut session).expect("the later notes save");
    assert_eq!(saved.envelopes, later.len());
    let file = reopened(&document);
    let (state, graph) = reduced(file.committed_operations());
    let (expected_state, expected_graph) = reduced(&session_log(&session));
    assert_eq!(state, expected_state, "the reducer state");
    assert_eq!(graph, expected_graph, "the graph");
    assert_eq!(music(&file.score()), on_screen);
}

/// P1b's shape, under the rule that undo stops at an unsettled save. While the
/// save's outcome is unknown, undoing its note is refused by name, and the note
/// stays drawn and unsaved. Reconciled as landed, the note is committed, undo
/// stops at the save, and the file reopened from the store holds what the
/// session draws; reconciled as not landed, the note is an ordinary unsaved unit
/// again and undoes, and the file never held it.
#[test]
fn undoing_what_an_unsettled_save_staged_is_refused_until_it_settles() {
    for landed in [true, false] {
        let (mut document, mut session) = flaky_document(vec![3], landed);
        let staged = session.applied_operations().len();
        let with_the_note = music(session.score());
        assert!(matches!(
            document.save(&mut session),
            Err(DocumentError::Indeterminate(_))
        ));
        assert!(session.has_unsettled_save() && !session.can_undo());
        assert_eq!(
            session.undo(),
            Err(EditorError::UnsettledSave),
            "the staged note is not undone"
        );
        assert_eq!(music(session.score()), with_the_note, "it stays drawn");
        assert!(session.is_dirty(), "and unsaved until the save settles");

        let outcome = document.reconcile(&mut session).expect("reconciles");
        assert!(!session.has_unsettled_save());
        if landed {
            assert_eq!(
                outcome,
                Reconciled::Saved(Saved {
                    generation: 2,
                    envelopes: staged,
                })
            );
            assert!(!session.is_dirty());
            assert_eq!(
                session.undo(),
                Err(EditorError::NothingToUndo),
                "the note is saved, and undo stops at the save"
            );
            assert_eq!(music(session.score()), with_the_note);
        } else {
            assert_eq!(outcome, Reconciled::NotSaved);
            assert!(session.is_dirty() && session.can_undo());
            session.undo().expect("the note undoes");
            assert!(music(session.score()).is_empty());
        }
        let file = reopened(&document);
        let (state, graph) = reduced(file.committed_operations());
        let (expected_state, expected_graph) = reduced(&session_log(&session));
        assert_eq!(state, expected_state, "the reducer state");
        assert_eq!(graph, expected_graph, "the graph");
        assert_eq!(music(&file.score()), music(session.score()));
    }
}

/// Units entered while a save is unsettled undo as ever, down to the two units
/// it staged and no further, and redo. Reconciled as landed, the staged units
/// are committed and the later one stays applied and undoable, down to the save;
/// saved too, the file holds what the session draws. No staged unit is undone
/// before a later edit, so none is forked away from one.
#[test]
fn a_unit_entered_after_an_unsettled_save_undoes_down_to_the_staged_ones() {
    let (mut document, mut session) = flaky_document(vec![3], true);
    session.enter_nominal(CmnNominal::D).unwrap();
    let staged = session.applied_operations().to_vec();
    let saved_music = music(session.score());
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    let voice = the_voice(&session);
    session.set_caret(voice, at(1, 2), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::G).unwrap();
    let later = session.applied_operations()[staged.len()..].to_vec();
    let on_screen = music(session.score());
    assert_eq!(on_screen.len(), 3);

    session.undo().expect("G, entered since, undoes");
    assert_eq!(music(session.score()), saved_music);
    assert_eq!(
        session.undo(),
        Err(EditorError::UnsettledSave),
        "D, staged, does not"
    );
    assert_eq!(music(session.score()), saved_music, "and nothing moves");
    session.redo().expect("G redoes");
    assert_eq!(music(session.score()), on_screen);

    let outcome = document.reconcile(&mut session).expect("reconciles");
    assert_eq!(
        outcome,
        Reconciled::Saved(Saved {
            generation: 2,
            envelopes: staged.len(),
        })
    );
    assert_eq!(
        membership(session.applied_operations()),
        membership(&later),
        "G stays applied"
    );
    session.undo().expect("G undoes");
    assert_eq!(
        session.undo(),
        Err(EditorError::NothingToUndo),
        "and undo stops at the save"
    );
    session.redo().expect("G redoes");
    document.save(&mut session).expect("G saves");
    let file = reopened(&document);
    assert_eq!(
        reduced(file.committed_operations()),
        reduced(&session_log(&session))
    );
    assert_eq!(music(&file.score()), on_screen);
}

/// The save's entry opened a measure past the end. While the save is unsettled
/// that entry cannot be undone, so no later entry opens the measure a second
/// time: the next note follows it into the opened measure. Reconciled as landed,
/// both stand and the later one undoes; as not landed, both undo, the measure
/// with its entry. Either way the file and the screen agree.
#[test]
fn an_entry_that_opened_a_measure_stays_while_its_save_is_unsettled() {
    for landed in [true, false] {
        let created = EditorDocument::create(MemStore::new(), setup_operations(1)).unwrap();
        let store = FlakyStore::new(created.into_store().into_bytes(), vec![3], landed);
        let mut document = EditorDocument::open(store).unwrap();
        let mut session = lease(&mut document);
        let voice = the_voice(&session);
        session.set_caret(voice, at(1, 1), whole(1, 4)).unwrap();
        session.enter_nominal(CmnNominal::C).unwrap();
        assert_eq!(measures(session.score()), vec![2], "C opened a measure");
        assert!(matches!(
            document.save(&mut session),
            Err(DocumentError::Indeterminate(_))
        ));
        assert_eq!(session.undo(), Err(EditorError::UnsettledSave));
        assert_eq!(measures(session.score()), vec![2], "and it stays open");
        session
            .enter_nominal(CmnNominal::D)
            .expect("D follows C in the opened measure");
        assert_eq!(measures(session.score()), vec![2]);
        let on_screen = music(session.score());
        assert_eq!(on_screen.len(), 2);

        let outcome = document.reconcile(&mut session).expect("reconciles");
        if landed {
            assert!(matches!(outcome, Reconciled::Saved(_)), "{outcome:?}");
            assert_eq!(music(session.score()), on_screen, "nothing is dropped");
            session.undo().expect("D undoes");
            assert_eq!(session.undo(), Err(EditorError::NothingToUndo));
            assert_eq!(measures(session.score()), vec![2]);
        } else {
            assert_eq!(outcome, Reconciled::NotSaved);
            session.undo().expect("D undoes");
            session.undo().expect("C undoes, and its measure with it");
            assert_eq!(measures(session.score()), vec![1]);
            assert!(music(session.score()).is_empty());
        }
        let file = reopened(&document);
        assert_eq!(
            reduced(file.committed_operations()),
            reduced(&session_log(&session))
        );
    }
}

/// Save As copies what a session draws, saved and unsaved, into a new document
/// under a fresh file identity and the same work's, and writes nothing into the
/// source. A view's edits are carried too; a session the document neither
/// leased nor drew is refused.
#[test]
fn save_as_copies_what_the_session_draws_into_a_new_document() {
    let mut document = EditorDocument::create(MemStore::new(), setup_operations(1)).unwrap();
    let mut session = lease(&mut document);
    let voice = the_voice(&session);
    session.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    document.save(&mut session).expect("C saves");
    session.enter_nominal(CmnNominal::D).unwrap();

    let copy = document
        .save_as(&session)
        .expect("the leased session copies");
    assert_eq!(
        membership(copy.operations()),
        membership(&session_log(&session)),
        "the copy holds the committed envelopes and the unsaved edit"
    );
    let copied = copy.create(MemStore::new()).expect("the copy is written");
    assert_eq!(copied.generation(), 1);
    assert_ne!(copied.bundle().file_uuid(), document.bundle().file_uuid());
    assert_eq!(
        copied.bundle().manifest().document_id,
        document.bundle().manifest().document_id
    );
    assert_eq!(
        reduced(copied.committed_operations()),
        reduced(&session_log(&session))
    );
    assert_eq!(document.generation(), 2, "the source is not written");
    assert!(session.is_dirty(), "and its session keeps its unsaved edit");

    let mut view = document.view(Box::new(StubSolver)).unwrap();
    let voice = the_voice(&view);
    view.set_caret(voice, at(1, 2), whole(1, 4)).unwrap();
    view.enter_nominal(CmnNominal::E).unwrap();
    let copy = document.save_as(&view).expect("a view copies");
    assert_eq!(
        membership(copy.operations()),
        membership(&session_log(&view))
    );

    let probe = EditorSession::open(document.score(), Box::new(StubSolver)).unwrap();
    assert!(matches!(
        document.save_as(&probe),
        Err(DocumentError::NotLeased)
    ));
    let other = EditorDocument::create(MemStore::new(), setup_operations(1)).unwrap();
    let foreign = other.view(Box::new(StubSolver)).unwrap();
    assert!(matches!(
        document.save_as(&foreign),
        Err(DocumentError::ForeignLease)
    ));
}

/// A document holding a blob is not saved as a copy of its operations, which
/// would not carry it.
#[test]
fn a_document_holding_blobs_is_not_saved_as() {
    let created = EditorDocument::create(MemStore::new(), setup_operations(1)).unwrap();
    let mut bundle = Bundle::open(created.into_store(), caps()).unwrap();
    let blob = StagedChunk {
        kind: ChunkKind::Blob,
        schema_version: SchemaVersion::V0,
        payload: b"an image".to_vec(),
    };
    bundle
        .commit(&[blob], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            let r = ctx.new_chunks[0];
            m.blob_roots.push(BlobRef {
                blob_id: BlobId(r.hash),
                media_type: "application/octet-stream".to_string(),
                offset: r.offset,
                compressed_length: r.compressed_length,
                uncompressed_length: r.uncompressed_length,
                compression: CompressionAlgorithm::None,
                hash: r.hash,
                declared_max_uncompressed_length: None,
            });
            m
        })
        .unwrap();
    let mut document =
        EditorDocument::open(MemStore::from_bytes(bundle.into_store().into_bytes())).unwrap();
    assert!(
        !document.is_read_only(),
        "a blob does not make it read-only"
    );
    let session = lease(&mut document);
    assert!(matches!(
        document.save_as(&session),
        Err(DocumentError::Blobs { count: 1 })
    ));
}

/// A store that cannot be read just after its failed flush: reconciling fails,
/// the document stays in recovery as it was, and a later reconcile settles it.
#[test]
fn a_reconcile_that_cannot_read_the_store_leaves_the_document_in_recovery() {
    let created = EditorDocument::create(MemStore::new(), setup_operations(1)).unwrap();
    let mut store = FlakyStore::new(created.into_store().into_bytes(), vec![3], true);
    store.unreadable = 1;
    let mut document = EditorDocument::open(store).unwrap();
    let mut session = lease(&mut document);
    let voice = the_voice(&session);
    session.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    assert!(matches!(
        document.reconcile(&mut session),
        Err(DocumentError::Bundle(_))
    ));
    assert!(
        matches!(
            document.save(&mut session),
            Err(DocumentError::RecoveryRequired)
        ),
        "the document is still in recovery"
    );
    assert!(session.is_dirty());
    let outcome = document
        .reconcile(&mut session)
        .expect("the store reads again");
    assert!(matches!(outcome, Reconciled::Saved(saved) if saved.generation == 2));
    assert!(!session.is_dirty());
}

/// A log holding an operation pending on a missing predecessor does not reduce
/// cleanly: the document opens read-only, drawn but not leased.
#[test]
fn an_unclean_log_opens_read_only() {
    let mut operations = setup_operations(1);
    let mut stray = operations.last().expect("a setup operation").clone();
    // A later operation of a replica the log holds nothing else of, claiming a
    // predecessor the log lacks.
    let replica = ReplicaId(0x5eed);
    stray.id = epiphany_core::OperationId::new(replica, 1);
    stray.stamp = epiphany_ops::OperationStamp::new(stray.stamp.hlc, stray.id);
    stray.causal_context = epiphany_ops::CausalContext::new().with_seen(replica, 0);
    operations.push(stray);
    let mut document = EditorDocument::create(MemStore::new(), operations).expect("created");
    assert!(matches!(
        document.read_only_reasons(),
        [ReadOnlyReason::Unclean { pending: 1, .. }]
    ));
    assert!(matches!(
        document.lease(Box::new(StubSolver)),
        Err(DocumentError::ReadOnly(_))
    ));
    assert!(document.view(Box::new(StubSolver)).is_ok());
}

/// A session that resumes a replica the committed partition holds mints its
/// operations past that replica's committed counters, and its entity ids past
/// every id the partition gave out: a pitch added to a chord and removed again
/// leaves no trace in the score, only in the committed operations.
#[test]
fn a_resumed_replica_mints_past_its_committed_ids() {
    use epiphany_core::{IdentifiedPitch, PitchId};
    use epiphany_ops::{DeleteIdentifiedPitchOp, InsertIdentifiedPitchOp, OperationKind};

    let replica = ReplicaId::generate();
    let operations = ScoreSetup::single_staff("Flute", 1)
        .operations(replica)
        .unwrap();
    let setup_len = operations.len() as u64;
    let mut document = EditorDocument::create(MemStore::new(), operations).unwrap();

    // Under the setup's replica: enter C, add E to it as a chord, remove the E.
    let mut session = lease(&mut document).with_identity(replica, epiphany_ops::AuthorId(0));
    let voice = the_voice(&session);
    session.set_caret(voice, at(0, 1), whole(1, 4)).unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    assert_eq!(
        session.applied_operations()[0].id,
        epiphany_core::OperationId::new(replica, setup_len),
        "the first operation follows the committed ones"
    );
    let (event, c) = match &session.applied_operations()[0].payload {
        epiphany_ops::OperationPayload::Primitive(OperationKind::InsertEvent(op)) => {
            (op.event_id(), op.pitch_ids()[0])
        }
        other => panic!("an insert, not {other:?}"),
    };
    let e = PitchId::new(replica, c.counter() + 1);
    session
        .apply(OperationKind::InsertIdentifiedPitch(
            InsertIdentifiedPitchOp {
                event,
                pitch: IdentifiedPitch {
                    id: e,
                    pitch: epiphany_editor_core::midi_note_to_pitch(64),
                },
            },
        ))
        .expect("E joins the chord");
    session
        .apply(OperationKind::DeleteIdentifiedPitch(
            DeleteIdentifiedPitchOp { pitch: e },
        ))
        .expect("and leaves it");
    assert!(
        !session.score().live_pitch_ids().contains(&e)
            && !session.score().tombstoned_pitches.contains(&e),
        "the score keeps no trace of the removed pitch"
    );
    document.save(&mut session).unwrap();

    // A new lease resuming the same replica enters a note: its pitch is fresh.
    let mut resumed = lease(&mut document).with_identity(replica, epiphany_ops::AuthorId(0));
    let voice = the_voice(&resumed);
    resumed.set_caret(voice, at(1, 4), whole(1, 4)).unwrap();
    resumed.enter_nominal(CmnNominal::G).unwrap();
    let fresh = match &resumed.applied_operations()[0].payload {
        epiphany_ops::OperationPayload::Primitive(OperationKind::InsertEvent(op)) => op.pitch_ids(),
        other => panic!("an insert, not {other:?}"),
    };
    assert!(
        !fresh.contains(&c) && !fresh.contains(&e),
        "{fresh:?} reuses a committed pitch id"
    );
    document
        .save(&mut resumed)
        .expect("the resumed session saves");
    let store = document.into_store();
    let document = EditorDocument::open(MemStore::from_bytes(store.into_bytes())).unwrap();
    assert!(
        !document.is_read_only(),
        "no equivocation or conflict was written"
    );
    let view = document.view(Box::new(StubSolver)).unwrap();
    assert_eq!(music(view.score()).len(), 2, "C and G");
}

/// A two-measure grand score in 4/4: two staves, one voice each.
fn two_staff_operations() -> Vec<OperationEnvelope> {
    let mut setup = ScoreSetup::single_staff("Piano", 2);
    setup.staves.push(epiphany_editor_core::StaffSetup {
        name: "Piano".to_owned(),
        clef: epiphany_core::Clef::bass(),
    });
    setup
        .operations(ReplicaId::generate())
        .expect("two staves in 4/4 are representable")
}

/// Enters X4b's values (schema major 5) as the import writes them: a C on the
/// upper staff's first beat carrying marks and an ornament, a grace note before
/// it, a lyric syllable and a dynamic on it; and an E on the lower staff whose
/// voice is at home on the upper (`SetVoiceHome`). The notes are entered at the
/// caret; the rest is one transaction.
fn enter_expression(session: &mut EditorSession) {
    use epiphany_core::{
        AccidentalId, AnchorOffset, Dynamic, Event, EventId, EventMark, Grace, GraceKind,
        IdentifiedPitch, Lyric, LyricLineId, Marker, MarkerId, MarkerKind, NoteValue, Ornament,
        OrnamentKind, PitchId, Syllabic, Text, TimeAnchor,
    };
    use epiphany_ops::{
        CreateCrossCuttingOp, CrossCuttingValue, InsertEventOp, ModifyEventOp, OperationKind,
        SetVoiceHomeOp,
    };

    let instances: Vec<_> = session
        .score()
        .staff_instances()
        .map(|(_, si)| (si.id, si.staff, si.voices[0].id))
        .collect();
    let [(upper, upper_staff, upper_voice), (_, _, lower_voice)] = instances[..] else {
        panic!("two staves, one voice each");
    };
    session
        .set_caret(upper_voice, at(0, 1), whole(1, 4))
        .unwrap();
    session.enter_nominal(CmnNominal::C).unwrap();
    session
        .set_caret(lower_voice, at(1, 4), whole(1, 4))
        .unwrap();
    session.enter_nominal(CmnNominal::E).unwrap();

    let Some(Event::Pitched(note)) = session
        .score()
        .events
        .iter()
        .find(|e| e.voice() == upper_voice)
        .cloned()
    else {
        panic!("the upper staff's C");
    };
    let replica = ReplicaId(0x4B5A);
    let mut marked = note.clone();
    marked.marks = vec![EventMark::Staccato, EventMark::Accent];
    marked.ornaments = vec![Ornament {
        kind: OrnamentKind::Trill,
        accidental_above: Some(AccidentalId::new("flat")),
        accidental_below: None,
    }];
    let mut grace = note.clone();
    grace.id = EventId::new(replica, 1);
    grace.duration = EventDuration::Musical(MusicalDuration::zero());
    grace.pitches = vec![IdentifiedPitch {
        id: PitchId::new(replica, 2),
        pitch: epiphany_editor_core::midi_note_to_pitch(74),
    }];
    grace.grace = Some(Grace {
        kind: GraceKind::Acciaccatura,
        value: NoteValue::Eighth,
        dots: 0,
        order: 0,
    });
    session
        .apply_transaction(
            "expression and text",
            None,
            vec![
                OperationKind::ModifyEvent(ModifyEventOp {
                    event: Event::Pitched(marked),
                }),
                OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: upper,
                    event: Event::Pitched(grace),
                }),
                OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Lyric(Lyric {
                        id: LyricLineId::new(replica, 3),
                        event: note.id,
                        verse: 1,
                        text: Text::new("la"),
                        syllabic: Syllabic::Single,
                        extension: false,
                    }),
                }),
                OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Marker(Marker {
                        id: MarkerId::new(replica, 4),
                        anchor: TimeAnchor::Event {
                            id: note.id,
                            offset: AnchorOffset::Zero,
                        },
                        kind: MarkerKind::Dynamic(Dynamic::Mf),
                    }),
                }),
                OperationKind::SetVoiceHome(SetVoiceHomeOp {
                    voice: lower_voice,
                    home: Some(upper_staff),
                }),
            ],
        )
        .expect("the values apply");
}

/// X4b's values a score holds: marks, ornaments, grace notes, lyric
/// syllables, markers and voice homes, counted.
fn expression(score: &Score) -> [usize; 6] {
    let (mut marks, mut ornaments, mut graces) = (0, 0, 0);
    for event in score.events.iter() {
        if let epiphany_core::Event::Pitched(p) = event {
            marks += p.marks.len();
            ornaments += p.ornaments.len();
            graces += usize::from(p.grace.is_some());
        }
    }
    [
        marks,
        ornaments,
        graces,
        score.cross_cutting.lyrics.len(),
        score.cross_cutting.markers.len(),
        score.voice_homes.len(),
    ]
}

const ENTERED: [usize; 6] = [2, 1, 1, 1, 1, 1];

/// X4b's values in X5a's document: saved, their blocks born at schema major 5
/// and stamped minimally; reopened, the reducer state and the graph equal to
/// what the session drew; leased again, edited and saved, and reopened equal
/// again, the values kept.
#[test]
fn expression_and_text_save_reopen_and_save_again() {
    let mut document = EditorDocument::create(MemStore::new(), two_staff_operations()).unwrap();
    let mut session = lease(&mut document);
    enter_expression(&mut session);
    assert_eq!(expression(session.score()), ENTERED);
    document.save(&mut session).expect("saves");
    assert!(
        document
            .bundle()
            .manifest()
            .operation_roots
            .iter()
            .any(|root| root.schema_version.major == 5),
        "a block holding the values is born at schema major 5"
    );
    assert_blocks_stamped_minimally(document.bundle());
    let drawn = session_log(&session);

    let mut file =
        EditorDocument::open(MemStore::from_bytes(document.into_store().into_bytes())).unwrap();
    assert!(file.read_only_reasons().is_empty());
    assert_eq!(
        reduced(file.committed_operations()),
        reduced(&drawn),
        "reopened: the reducer state and the graph"
    );
    assert_eq!(expression(&file.score()), ENTERED);

    let mut again = lease(&mut file);
    let voice = the_voice(&again);
    again.set_caret(voice, at(1, 1), whole(1, 4)).unwrap();
    again.enter_nominal(CmnNominal::G).unwrap();
    file.save(&mut again).expect("saves again");
    assert_blocks_stamped_minimally(file.bundle());
    let file = EditorDocument::open(MemStore::from_bytes(file.into_store().into_bytes())).unwrap();
    assert_eq!(
        reduced(file.committed_operations()),
        reduced(&session_log(&again))
    );
    assert_eq!(expression(&file.score()), ENTERED, "the values stay");
}

/// Save as carries X4b's values, saved and unsaved, into the new document.
#[test]
fn save_as_carries_expression_and_text() {
    let mut document = EditorDocument::create(MemStore::new(), two_staff_operations()).unwrap();
    let mut session = lease(&mut document);
    enter_expression(&mut session);
    let copied = document
        .save_as(&session)
        .expect("copies")
        .create(MemStore::new())
        .expect("the copy is written");
    assert_blocks_stamped_minimally(copied.bundle());
    let copied =
        EditorDocument::open(MemStore::from_bytes(copied.into_store().into_bytes())).unwrap();
    assert_eq!(
        reduced(copied.committed_operations()),
        reduced(&session_log(&session))
    );
    assert_eq!(expression(&copied.score()), ENTERED);
    assert_eq!(document.generation(), 1, "the source is not written");
    assert_eq!(expression(&document.score()), [0; 6]);
}

/// D64's floor over X4b's values: while the save that staged them is
/// unsettled, undo stops at them and they stay drawn. Reconciled as landed,
/// they are saved and the file holds them; as not landed, they undo and the
/// file never held them.
#[test]
fn undo_stops_at_expression_and_text_an_unsettled_save_staged() {
    for landed in [true, false] {
        let created = EditorDocument::create(MemStore::new(), two_staff_operations()).unwrap();
        let store = FlakyStore::new(created.into_store().into_bytes(), vec![3], landed);
        let mut document = EditorDocument::open(store).unwrap();
        let mut session = lease(&mut document);
        enter_expression(&mut session);
        assert!(matches!(
            document.save(&mut session),
            Err(DocumentError::Indeterminate(_))
        ));
        assert_eq!(session.undo(), Err(EditorError::UnsettledSave));
        assert_eq!(expression(session.score()), ENTERED, "the values stay");

        let outcome = document.reconcile(&mut session).expect("reconciles");
        if landed {
            assert!(matches!(outcome, Reconciled::Saved(_)), "{outcome:?}");
            assert!(!session.is_dirty());
            assert_eq!(session.undo(), Err(EditorError::NothingToUndo));
            assert_eq!(expression(session.score()), ENTERED);
        } else {
            assert_eq!(outcome, Reconciled::NotSaved);
            session.undo().expect("the values undo, one transaction");
            assert_eq!(expression(session.score()), [0; 6]);
            assert!(session.is_dirty(), "the two notes stay, unsaved");
        }
        let file = reopened(&document);
        assert_eq!(
            reduced(file.committed_operations()),
            reduced(session.committed_operations()),
            "the file holds what the session has saved"
        );
        assert_eq!(
            expression(&file.score()),
            if landed { ENTERED } else { [0; 6] }
        );
    }
}
