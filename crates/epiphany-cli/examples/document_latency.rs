//! Measures an imported score as a document: how long it takes to open, to
//! take an edit, to save and to reopen.
//!
//!     cargo run --release -p epiphany-cli --example document_latency -- <file.musicxml> [edits]
//!
//! The MusicXML file is imported into a document in memory, which is leased
//! with the real engraver on the score's page. Each edit enters a note at the
//! start of the first staff's voice, alternating two pitches, and is timed
//! whole: minting, the reduction of every committed and applied operation, the
//! engraving and the render. The reduction alone is timed beside it on the
//! same operations. The document is then saved, reopened from its bytes and
//! leased again. Every edit re-reduces the whole log; the latency budget is a
//! later phase's.

use std::time::{Duration, Instant};

use epiphany_bundle::MemStore;
use epiphany_cli::page_of;
use epiphany_core::{CmnNominal, IdentityContext, MusicalDuration, MusicalPosition};
use epiphany_core::{RationalTime, ReplicaId, Score};
use epiphany_editor_core::EditorDocument;
use epiphany_engrave::Engraver;
use epiphany_ops::OperationSet;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: document_latency <file.musicxml> [edits]");
        std::process::exit(2);
    };
    let edits: usize = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(10);

    let xml = std::fs::read_to_string(path).expect("the file reads");
    let start = Instant::now();
    let import = epiphany_musicxml::import(&xml).expect("the file imports");
    let read = start.elapsed();
    let operations = import.envelopes.len();

    let start = Instant::now();
    let mut document =
        EditorDocument::create(MemStore::new(), import.envelopes).expect("the document is made");
    let create = start.elapsed();
    assert!(
        !document.is_read_only(),
        "read-only: {:?}",
        document.read_only_reasons()
    );
    let score = document.score();
    let start = Instant::now();
    let mut session = document
        .lease(Box::new(Engraver::with_geometry(page_of(&score))))
        .expect("the document leases");
    let lease = start.elapsed();
    let pages = session.resolved().pages.len();

    let voice = session.score().voices().next().expect("a voice").2.id;
    let quarter = MusicalDuration(RationalTime::new(1, 4).expect("1/4"));
    let mut whole_edit = Vec::with_capacity(edits);
    let mut reduce_only = Vec::with_capacity(edits);
    for i in 0..edits {
        session
            .set_caret(voice, MusicalPosition::origin(), quarter.clone())
            .expect("the caret goes at the start");
        let nominal = if i % 2 == 0 {
            CmnNominal::C
        } else {
            CmnNominal::D
        };
        let start = Instant::now();
        session.enter_nominal(nominal).expect("the edit applies");
        whole_edit.push(start.elapsed());

        let mut set = OperationSet::new();
        for envelope in session
            .committed_operations()
            .iter()
            .chain(session.applied_operations())
        {
            set.accept(envelope.clone());
        }
        let base = Score::empty(IdentityContext::new(ReplicaId(7)));
        let start = Instant::now();
        let reduced = set.reduce_onto(&base);
        reduce_only.push(start.elapsed());
        assert!(reduced.state.is_clean());
    }

    let start = Instant::now();
    let saved = document.save(&mut session).expect("the document saves");
    let save = start.elapsed();
    let bytes = document.into_store().into_bytes();

    let start = Instant::now();
    let mut reopened =
        EditorDocument::open(MemStore::from_bytes(bytes.clone())).expect("the document reopens");
    let reopen = start.elapsed();
    let score = reopened.score();
    let start = Instant::now();
    let session = reopened
        .lease(Box::new(Engraver::with_geometry(page_of(&score))))
        .expect("the reopened document leases");
    let release = start.elapsed();
    assert_eq!(
        session.committed_operations().len(),
        operations + saved.envelopes
    );

    println!("file: {path}");
    println!(
        "operations {operations}, pages {pages}, document {} bytes after {} saved envelopes",
        bytes.len(),
        saved.envelopes
    );
    println!("read and import {}", ms(read));
    println!("create (commit the import) {}", ms(create));
    println!("lease (reduce and engrave) {}", ms(lease));
    println!("edits {edits}: whole edit {}", stats(&mut whole_edit));
    println!("        reduction alone {}", stats(&mut reduce_only));
    println!("save {}", ms(save));
    println!("reopen (decode and reduce) {}", ms(reopen));
    println!("lease after reopen {}", ms(release));
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1e3)
}

fn stats(samples: &mut [Duration]) -> String {
    samples.sort();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    format!(
        "median {}, p90 {}, max {}",
        ms(at(0.5)),
        ms(at(0.9)),
        ms(*samples.last().expect("at least one edit"))
    )
}
