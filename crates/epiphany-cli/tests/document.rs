//! An imported score as a document: the import's operations become the
//! document's log, so it opens, takes an edit, saves and reopens like any other.

use std::path::{Path, PathBuf};

use epiphany_cli::{engrave_loaded, engrave_score, load, new_document, open_document, page_of};
use epiphany_core::{CmnNominal, IdentityContext, MusicalDuration, MusicalPosition, RationalTime};
use epiphany_core::{ReplicaId, Score};
use epiphany_editor_core::EditorSession;
use epiphany_engrave::Engraver;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../epiphany-musicxml/tests/fixtures")
        .join(name)
}

/// A path for a new document under the test target's scratch directory.
fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{name}-{}-{}.musc",
        std::process::id(),
        ReplicaId::generate().0
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// `score` with its replica-scoped identity cursor set aside.
fn graph(score: &Score) -> Score {
    let mut score = score.clone();
    score.identity = IdentityContext::new(ReplicaId(7));
    score
}

/// The envelope set, as sorted (id, hash) pairs.
fn membership(envelopes: &[epiphany_ops::OperationEnvelope]) -> Vec<String> {
    let mut out: Vec<String> = envelopes
        .iter()
        .map(|e| format!("{:?} {}", e.id, e.envelope_hash().to_hex()))
        .collect();
    out.sort();
    out
}

fn quarter() -> MusicalDuration {
    MusicalDuration(RationalTime::new(1, 4).expect("1/4"))
}

#[test]
fn an_import_becomes_a_document_that_takes_an_edit_and_reopens() {
    let source = fixture("grand_staff.musicxml");
    let target = scratch("grand-staff");
    let loaded = load(&source).expect("the fixture imports");

    let mut document = new_document(&source, &target).expect("the document is made");
    assert!(
        !document.is_read_only(),
        "{:?}",
        document.read_only_reasons()
    );
    assert_eq!(document.generation(), 1);
    assert_eq!(
        document.committed_operations(),
        &loaded.import.envelopes[..],
        "the import's operations are the document's log"
    );
    assert_eq!(
        graph(&document.score()),
        graph(&loaded.reduced.score),
        "the document is the imported score"
    );

    // It takes an edit: a C over the first staff's first beat.
    let score = document.score();
    let mut session: EditorSession = document
        .lease(Box::new(Engraver::with_geometry(page_of(&score))))
        .expect("an imported document is writable");
    let voice = session.score().voices().next().expect("a voice").2.id;
    session
        .set_caret(voice, MusicalPosition::origin(), quarter())
        .unwrap();
    let outcome = session
        .enter_nominal(CmnNominal::C)
        .expect("the edit applies");
    assert!(outcome.graph_changed);
    let edited = graph(session.score());
    assert_ne!(edited, graph(&loaded.reduced.score));
    document.save(&mut session).expect("the edit saves");
    drop(document);

    // And reopens with it.
    let reopened = open_document(&target).expect("the document reopens");
    assert_eq!(reopened.generation(), 2);
    assert_eq!(
        membership(reopened.committed_operations()),
        membership(session.committed_operations()),
        "the import's operations and the edit's: the manifest orders blocks by \
         content, so the set is compared"
    );
    assert!(reopened.committed_operations().len() > loaded.import.envelopes.len());
    assert_eq!(
        graph(&reopened.score()),
        edited,
        "the edit survives the reopen"
    );
    let engraved = engrave_score(&reopened.score());
    assert!(!engraved.layout.pages.is_empty());
    std::fs::remove_file(&target).expect("removed");
}

#[test]
fn a_document_is_not_made_over_an_existing_file() {
    let source = fixture("single_part.musicxml");
    let target = scratch("existing");
    std::fs::write(&target, b"someone's work").unwrap();
    assert!(new_document(&source, &target).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"someone's work");
    std::fs::remove_file(&target).unwrap();
}

/// A one-part score whose file sets a page: 1,800 by 1,500 tenths with 75-tenth
/// margins, so 180 by 150 staff spaces with 7.5 at each edge.
const PAGED: &str = r#"<score-partwise version="4.0">
<defaults><scaling><millimeters>7</millimeters><tenths>40</tenths></scaling>
<page-layout><page-width>1800</page-width><page-height>1500</page-height>
<page-margins type="both"><left-margin>75</left-margin><right-margin>75</right-margin>
<top-margin>75</top-margin><bottom-margin>75</bottom-margin></page-margins></page-layout>
</defaults>
<part-list><score-part id="P1"><part-name>A</part-name></score-part></part-list>
<part id="P1"><measure number="1"><attributes><divisions>1</divisions>
<time><beats>4</beats><beat-type>4</beat-type></time>
<clef><sign>G</sign><line>2</line></clef></attributes>
<note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><type>whole</type></note>
</measure></part></score-partwise>"#;

#[test]
fn an_imported_document_keeps_its_file_s_page() {
    let source = Path::new(env!("CARGO_TARGET_TMPDIR")).join("paged-document.musicxml");
    std::fs::write(&source, PAGED).unwrap();
    let target = scratch("paged");
    let loaded = load(&source).expect("imports");
    assert!(
        loaded
            .import
            .source
            .features
            .of_class(epiphany_musicxml::source::FeatureClass::Presentation)
            .all(|(kind, _)| kind != "defaults: page-layout"),
        "a whole page is imported, not recorded as missing"
    );
    let document = new_document(&source, &target).expect("made");
    let page = page_of(&document.score());
    assert_eq!(
        (page.size.width.0, page.size.height.0),
        (180.0, 150.0),
        "the document holds its file's page"
    );
    assert_eq!(page.margins.left.0, 7.5);
    let from_document = engrave_score(&document.score()).layout;
    let from_import = engrave_loaded(&loaded).layout;
    assert_eq!(
        from_document.pages.len(),
        from_import.pages.len(),
        "the document is drawn on the page its import is"
    );
    assert_eq!(
        epiphany_cli::svg(&from_document),
        epiphany_cli::svg(&from_import)
    );
    std::fs::remove_file(&target).unwrap();
}

/// A saved document is told from MusicXML by its bytes, and `engrave_path`
/// draws each as what it is.
#[test]
fn a_document_is_recognized_and_drawn_by_path() {
    let source = fixture("changes.musicxml");
    let target = scratch("changes");
    let document = new_document(&source, &target).expect("made");
    let score = document.score();
    drop(document);
    assert!(!epiphany_cli::is_document(&source).unwrap());
    assert!(epiphany_cli::is_document(&target).unwrap());
    let drawn = epiphany_cli::engrave_path(&target).expect("the document draws");
    assert_eq!(
        epiphany_cli::svg(&drawn.layout),
        epiphany_cli::svg(&engrave_score(&score).layout)
    );
    let imported = epiphany_cli::engrave_path(&source).expect("the import draws");
    assert_eq!(
        epiphany_cli::svg(&imported.layout),
        epiphany_cli::svg(&drawn.layout),
        "a concert-pitch import and its document draw alike"
    );
    std::fs::remove_file(&target).unwrap();
}
