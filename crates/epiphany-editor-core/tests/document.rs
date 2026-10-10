//! The document layer: a score created, written, edited, undone, saved, reopened
//! and edited again, through `EditorDocument` and the session it leases.

use std::io;

use epiphany_bundle::{
    encode_block, BlockStore, Bundle, BundleCapabilities, DocumentId, ExtensionDeclaration,
    ExtensionId, FileUuid, Manifest, MemStore, ReductionAlgorithmVersion, SchemaVersion, SemVer,
    StagedChunk,
};
use epiphany_core::{
    CmnNominal, EventDuration, EventPosition, IdentityContext, MusicalDuration, MusicalPosition,
    RationalTime, ReplicaId, Score, VoiceId,
};
use epiphany_editor_core::{
    DocumentError, EditorDocument, EditorSession, ReadOnlyReason, Reconciled, ScoreSetup,
};
use epiphany_layout_ir::StubSolver;
use epiphany_ops::{OperationEnvelope, OperationSet, CURRENT_REDUCTION_ALGORITHM_VERSION};

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
/// and E, a quarter rest and quarter F in the first measure; a half G, a quarter
/// rest and eighths A and B in the second.
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
        (Some(CmnNominal::F), (1, 4)),
        (Some(CmnNominal::G), (1, 2)),
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

    // Continue: a third measure's worth of entries after the phrase.
    let voice = the_voice(&reopened);
    reopened
        .set_caret(voice, at(1, 1), whole(1, 2))
        .expect("the caret goes at the second measure");
    reopened
        .enter_nominal(CmnNominal::E)
        .expect("the reopened document takes an edit");
    let continued = music(reopened.score());
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

/// A document holding an envelope in a historical layout is refused by name.
#[test]
fn a_historical_layout_is_refused_by_name() {
    let historical = epiphany_ops::vectors::decode_vectors()
        .into_iter()
        .find(|(_, _, _, name, _)| name == "create_tuplet_before_major_4")
        .map(|(.., bytes)| bytes)
        .expect("the ops corpus carries the vector");
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
                encode_block(&[historical]),
                SchemaVersion::for_major(3),
            )],
            bundle.superblock().manifest_schema_version,
            |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots.extend(ctx.new_chunks.iter().copied());
                m
            },
        )
        .unwrap();
    let store = MemStore::from_bytes(bundle.into_store().into_bytes());
    assert!(matches!(
        EditorDocument::open(store),
        Err(DocumentError::UnsupportedHistoricalEncoding { layout: "Tuplet" })
    ));
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

/// A store whose chosen flushes fail. `keep` decides what a failed flush leaves:
/// with it, the writes it was to make durable stay readable (an `fsync` that
/// reports an error after the data reached the page cache); without it, they are
/// lost (the kernel dropped them).
struct FlakyStore {
    live: Vec<u8>,
    durable: Vec<u8>,
    flushes: u32,
    fail: Vec<u32>,
    keep: bool,
}

impl FlakyStore {
    fn new(image: Vec<u8>, fail: Vec<u32>, keep: bool) -> Self {
        FlakyStore {
            live: image.clone(),
            durable: image,
            flushes: 0,
            fail,
            keep,
        }
    }
}

impl BlockStore for FlakyStore {
    fn len(&self) -> u64 {
        self.live.len() as u64
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let end = offset as usize + buf.len();
        let bytes = self
            .live
            .get(offset as usize..end)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "past the end"))?;
        buf.copy_from_slice(bytes);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        let end = offset as usize + data.len();
        if self.live.len() < end {
            self.live.resize(end, 0);
        }
        self.live[offset as usize..end].copy_from_slice(data);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if self.fail.contains(&self.flushes) {
            if !self.keep {
                self.live = self.durable.clone();
            }
            return Err(io::Error::other("injected flush failure"));
        }
        self.durable = self.live.clone();
        Ok(())
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

// A save's commit runs three flushes: after the chunks, after the manifest, and
// at the superblock, the commit point.

#[test]
fn a_commit_failing_before_its_commit_point_saves_nothing() {
    let (mut document, mut session) = flaky_document(vec![2], false);
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
    let (document, outcome) = document.reconcile(&mut session).expect("reopens");
    assert!(matches!(outcome, Reconciled::Saved(saved) if saved.generation == 2));
    assert!(!session.is_dirty() && !session.can_undo());
    assert_eq!(document.generation(), 2);
}

#[test]
fn an_indeterminate_commit_that_did_not_land_leaves_the_units_unsaved() {
    let (mut document, mut session) = flaky_document(vec![3], false);
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    let (mut document, outcome) = document.reconcile(&mut session).expect("reopens");
    assert_eq!(outcome, Reconciled::NotSaved);
    assert!(session.is_dirty() && session.can_undo());
    assert_eq!(document.generation(), 1);
    let saved = document.save(&mut session).expect("the units save later");
    assert_eq!(saved.generation, 2);
}

#[test]
fn a_failing_fresh_barrier_stays_indeterminate() {
    let (mut document, mut session) = flaky_document(vec![3, 4], true);
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::Indeterminate(_))
    ));
    let (mut document, outcome) = document.reconcile(&mut session).expect("reopens");
    assert_eq!(outcome, Reconciled::StillIndeterminate);
    assert!(session.is_dirty(), "neither saved nor unsaved is claimed");
    assert!(matches!(
        document.save(&mut session),
        Err(DocumentError::RecoveryRequired)
    ));
    let (_, outcome) = document.reconcile(&mut session).expect("reopens");
    assert!(
        matches!(outcome, Reconciled::Saved(_)),
        "a later barrier settles it"
    );
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
