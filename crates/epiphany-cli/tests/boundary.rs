//! The supported-layout boundary (roadmap D4): files the current writers make,
//! mixed-major ones included, reopen through both production readers, the
//! document layer and the text projection, with the reducer state and the
//! graph they were saved with.
//!
//! Minimal stamping stamps each operation block at the lowest major its values
//! need, so an ordinary import and its edits already mix majors: the import's
//! blocks at 5, an edit's at 0 or 2 or 5. The text projection's writer stamps
//! one block per document, which reaches the majors the others do not (1, 3
//! and 4).

use std::collections::BTreeSet;
use std::path::Path;

use epiphany_bundle::{Bundle, BundleCapabilities, FileUuid, MemStore, ReductionAlgorithmVersion};
use epiphany_cli::load;
use epiphany_core::{
    EventId, IdentityContext, OperationId, RegionId, RepeatStructureId, ReplicaId, Score, SlurId,
    TranspositionInterval, WallClockTime,
};
use epiphany_determinism::CanonicalEncode;
use epiphany_editor_core::EditorDocument;
use epiphany_layout_ir::StubSolver;
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, CreateCrossCuttingOp, CreateRegionOp,
    CreateRepeatStructureOp, CrossCuttingValue, HybridLogicalClock, ModifyEventOp,
    OperationEnvelope, OperationKind, OperationPayload, OperationSet, OperationStamp,
    TransposeIntervalOp, CURRENT_REDUCTION_ALGORITHM_VERSION,
};

fn caps() -> BundleCapabilities {
    BundleCapabilities {
        current_reduction_version: ReductionAlgorithmVersion(CURRENT_REDUCTION_ALGORITHM_VERSION),
    }
}

/// The reduced state's canonical bytes and the graph's, the replica-scoped
/// identity cursor set aside, of `envelopes` reduced from an empty score in
/// graph-aware mode, as a document reduces its log.
fn reduced(envelopes: &[OperationEnvelope]) -> (Vec<u8>, Vec<u8>) {
    let mut set = OperationSet::new();
    for envelope in envelopes {
        set.accept(envelope.clone());
    }
    let materialized = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(7))));
    let mut score = materialized.score;
    score.identity = IdentityContext::new(ReplicaId(7));
    (
        materialized.state.canonical_bytes(),
        score.canonical_bytes(),
    )
}

/// The envelope set, as sorted canonical bytes.
fn membership(envelopes: &[OperationEnvelope]) -> BTreeSet<Vec<u8>> {
    envelopes.iter().map(|e| e.to_canonical_bytes()).collect()
}

/// Each operation block's declared version, as `(major, minor)`.
fn block_versions(image: &[u8]) -> Vec<(u16, u16)> {
    let bundle = Bundle::open(MemStore::from_bytes(image.to_vec()), caps()).expect("opens");
    bundle
        .manifest()
        .operation_roots
        .iter()
        .map(|root| (root.schema_version.major, root.schema_version.minor))
        .collect()
}

/// Reopens `image` through both production readers and holds each to the
/// envelopes `saved`: the same set, reducing to the same state and graph.
fn reopens_as_saved(image: &[u8], saved: &[OperationEnvelope], what: &str) {
    let expected = reduced(saved);

    let document = EditorDocument::open(MemStore::from_bytes(image.to_vec()))
        .unwrap_or_else(|e| panic!("{what}: the document layer refuses it: {e}"));
    assert_eq!(
        membership(document.committed_operations()),
        membership(saved),
        "{what}: the document layer's operations"
    );
    assert_eq!(
        reduced(document.committed_operations()),
        expected,
        "{what}: the document layer's state and graph"
    );
    let mut score = document.score();
    score.identity = IdentityContext::new(ReplicaId(7));
    assert_eq!(
        score.canonical_bytes(),
        expected.1,
        "{what}: the document's own graph"
    );

    let bundle = Bundle::open(MemStore::from_bytes(image.to_vec()), caps()).expect("opens");
    let text = epiphany_textproj::project::document_from_bundle(&bundle)
        .unwrap_or_else(|e| panic!("{what}: the text projection refuses it: {e}"));
    assert_eq!(
        membership(&text.envelopes),
        membership(saved),
        "{what}: the text projection's operations"
    );
    assert_eq!(
        reduced(&text.envelopes),
        expected,
        "{what}: the text projection's state and graph"
    );
}

/// An import whose blocks hold major-5 values, edited and saved three times by
/// the editor, each save its own block: a pitch transposed (major 0), a slur
/// (major 2) and an event rewritten with its grace (major 5). The file mixes
/// majors, and reopens as saved after every save.
#[test]
fn an_import_and_its_edits_reopen_as_saved_through_both_readers() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../epiphany-musicxml/tests/fixtures/grace_in_tuplet.musicxml");
    let loaded = load(&source).expect("the fixture imports");
    let import = loaded.import.envelopes.clone();

    let mut document =
        EditorDocument::create(MemStore::new(), import.clone()).expect("the import is a document");
    let mut session = document
        .lease(Box::new(StubSolver))
        .expect("an imported document is writable");

    // What the edits name, from the import.
    let primitive = |e: &OperationEnvelope| match &e.payload {
        OperationPayload::Primitive(kind) => Some(kind.clone()),
        _ => None,
    };
    let graced = import
        .iter()
        .filter(|e| e.schema_major() == 5)
        .find_map(|e| match primitive(e) {
            Some(OperationKind::InsertEvent(op)) => Some(op.event),
            _ => None,
        })
        .expect("the import inserts an event holding a major-5 value");
    let score = session.score().clone();
    let (first, second, pitch) = score
        .voices()
        .find_map(|(_, _, voice)| {
            let pitched: Vec<_> = voice
                .events
                .iter()
                .filter_map(|id| match score.events.get(*id) {
                    Some(epiphany_core::Event::Pitched(event)) if !event.pitches.is_empty() => {
                        Some((event.id, event.pitches[0].id))
                    }
                    _ => None,
                })
                .collect();
            (pitched.len() >= 2).then(|| (pitched[0].0, pitched[1].0, pitched[0].1))
        })
        .expect("a voice with two pitched events");

    let edits = [
        (
            0,
            OperationKind::TransposeInterval(TransposeIntervalOp {
                targets: [pitch].into(),
                interval: TranspositionInterval {
                    diatonic_steps: 1,
                    chromatic_steps: 2,
                },
            }),
        ),
        (
            2,
            OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                structure: CrossCuttingValue::Slur(valuegen::slur(
                    SlurId::new(session.replica(), 1 << 40),
                    first,
                    second,
                )),
            }),
        ),
        (
            5,
            OperationKind::ModifyEvent(ModifyEventOp { event: graced }),
        ),
    ];
    let mut majors = BTreeSet::new();
    let mut blocks = block_versions(document.bundle().image());
    assert!(
        blocks.iter().all(|(major, _)| *major == 5),
        "the import's blocks: {blocks:?}"
    );
    for (major, edit) in edits {
        session.apply(edit).expect("the edit applies");
        document.save(&mut session).expect("the edit saves");
        let image = document.bundle().image().to_vec();
        let versions = block_versions(&image);
        let mut new: Vec<_> = versions.clone();
        for old in &blocks {
            let at = new.iter().position(|v| v == old).expect("a block is kept");
            new.remove(at);
        }
        assert!(
            matches!(new[..], [(m, _)] if m == major),
            "the save writes one major-{major} block: {new:?}"
        );
        majors.extend(versions.iter().map(|(major, _)| *major));
        blocks = versions;
        reopens_as_saved(
            &image,
            session.committed_operations(),
            &format!("after the major-{major} save"),
        );
    }
    assert_eq!(
        majors,
        BTreeSet::from([0, 2, 5]),
        "the file mixes the import's major and its edits'"
    );
}

/// An envelope of `kind` at `counter`, one replica's, with no causal context.
fn envelope(counter: u64, kind: OperationKind) -> OperationEnvelope {
    let id = OperationId::new(ReplicaId(9), counter);
    OperationEnvelope {
        id,
        author: AuthorId(9),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(1), 1), id),
        causal_context: CausalContext::new(),
        transaction: None,
        payload: OperationPayload::Primitive(kind),
    }
}

/// Every document of the text projection's corpus, written by its serializer
/// as one block, reopens as written through both readers, with two more the
/// corpus does not hold: a region alone (a major-1 block) and a repeat
/// structure (major 2). Together they reach every major minimal stamping
/// assigns.
#[test]
fn the_text_projection_s_documents_reopen_as_written_through_both_readers() {
    let mut majors = BTreeSet::new();
    let mut versions = BTreeSet::new();
    let mut documents = Vec::new();
    for (_, verdict, _, name, text) in epiphany_textproj::vectors::document_vectors() {
        if verdict != "accept" {
            continue;
        }
        let text = String::from_utf8(text).expect("UTF-8");
        let document = epiphany_textproj::parse::parse_document(&text).expect("parses");
        if name == "minimal" {
            let region = RegionId::new(ReplicaId(9), 1);
            let (start, end) = (EventId::new(ReplicaId(9), 1), EventId::new(ReplicaId(9), 2));
            for (what, kind) in [
                (
                    "a region alone",
                    OperationKind::CreateRegion(CreateRegionOp {
                        region: valuegen::region(region),
                    }),
                ),
                (
                    "a repeat structure",
                    OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
                        repeat: valuegen::repeat_structure(
                            RepeatStructureId::new(ReplicaId(9), 1),
                            start,
                            end,
                        ),
                    }),
                ),
            ] {
                let mut extended = epiphany_textproj::parse::parse_document(&text).expect("parses");
                extended.envelopes.push(envelope(1, kind));
                documents.push((what.to_string(), extended));
            }
        }
        documents.push((name.to_string(), document));
    }
    for (name, document) in documents {
        if document.canonical_base.is_some() || !document.extensions.is_empty() {
            continue;
        }
        let bundle = epiphany_textproj::serialize::serialize_document(
            &document,
            MemStore::new(),
            FileUuid([0x5B; 16]),
        )
        .expect("serializes");
        let image = bundle.into_store().into_bytes();
        for version in block_versions(&image) {
            majors.insert(version.0);
            versions.insert(version);
        }
        reopens_as_saved(&image, &document.envelopes, &name);
    }
    assert_eq!(
        majors,
        (0..=5).collect::<BTreeSet<u16>>(),
        "the documents reach every major: {versions:?}"
    );
}
