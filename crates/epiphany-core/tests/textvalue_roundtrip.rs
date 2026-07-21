//! The text projection's own injectivity, exercised over generated scores.
//!
//! `req:textproj:roundtrip` states two equations. The one a test can check with
//! byte equality is the *text's* injectivity:
//!
//! ```text
//! project(serialize(parse(T))) = T
//! ```
//!
//! For a single value that reads: rendering a value, reading it back, and
//! rendering again must give the same bytes; and the value that comes back must
//! equal the value that went in. Both directions are asserted here for every
//! `Event` of every score `epiphany_core::generators` can build, which reaches the
//! whole of the Chapter-5 value graph that operations embed.
//!
//! # What this test cannot see
//!
//! A `project`/`parse` pair that agrees with *itself* on a wrong field order round
//! trips perfectly. Two adjacent fields of the same type, swapped in both
//! directions, are invisible here and to the compiler both. Order is pinned
//! elsewhere:
//!
//! * for the 82 `struct_codec!` structs, by construction — the same field list
//!   generates the binary codec and the projection, so they cannot disagree;
//! * for the 44 hand-written impls, by a mechanical diff of the identifier
//!   sequence in `impl Codec for T`'s `fn enc` against the one in `project`.
//!   `enc`'s order *is* the ratified declaration order, and all 44 agreed.
//!
//! Saying so is the point. A green round-trip is not evidence of correct order,
//! and this file must not be read as if it were.
//!
//! `textvalue_names.rs` covers the neighbouring blind spot: a mistyped
//! constructor symbol, which is equally invisible to a round trip because `parse`
//! reads back whatever `project` wrote.

use std::collections::BTreeSet;

use epiphany_core::generators::{arbitrary_graph_corpus, valid_score_rich};
use epiphany_core::textvalue::{read_sexp, TextValue};
use epiphany_core::Score;

/// Renders `value`, reads it back, and asserts both the value and its rendering
/// survive unchanged.
#[track_caller]
fn round_trips<T: TextValue + PartialEq + std::fmt::Debug>(value: &T) {
    let text = value.project().render();
    let sexp = read_sexp(&text).unwrap_or_else(|e| panic!("rejected its own output: {text}: {e}"));
    let back = T::parse(&sexp).unwrap_or_else(|e| panic!("rejected its own output: {text}: {e}"));
    assert_eq!(*value, back, "value changed across the projection");
    assert_eq!(
        back.project().render(),
        text,
        "re-projection is not byte-identical"
    );
}

/// Every event of every generated score, through text and back.
#[test]
fn every_generated_event_round_trips_through_its_projection() {
    let mut events = 0usize;
    for score in arbitrary_graph_corpus(24, 0xF0F0_1234) {
        for event in score.events.iter_canonical() {
            round_trips(event);
            events += 1;
        }
    }
    // A round-trip test that round-trips nothing is a test that passes for the
    // wrong reason. The corpus must actually carry events.
    assert!(
        events > 100,
        "the corpus reached only {events} events; it proves almost nothing"
    );
}

/// The arena projects as a sequence, so it exercises the ordering rule as well as
/// every event.
#[test]
fn a_whole_event_arena_round_trips() {
    let score = valid_score_rich(7);
    assert!(
        score.events.iter_canonical().count() > 1,
        "need at least two events to exercise the arena's ordering"
    );
    round_trips(&score.events);
}

/// Distinct values must render to distinct text: the projection determines the
/// document, so two documents may not share a projection
/// (`req:textproj:canonical-text`).
#[test]
fn distinct_scores_project_to_distinct_text() {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut arenas = 0usize;
    for score in arbitrary_graph_corpus(16, 0x5EED) {
        let text = score.events.project().render();
        if !seen.insert(text) {
            // Two generated scores may legitimately share an event arena; only a
            // *different* arena rendering to the same text would be a defect. Check
            // that directly rather than assuming distinctness.
            continue;
        }
        arenas += 1;
    }
    assert!(
        arenas > 1,
        "the corpus produced only {arenas} distinct arenas"
    );

    // The real statement: equal text implies equal value.
    let corpus: Vec<Score> = arbitrary_graph_corpus(16, 0x5EED).collect();
    for a in &corpus {
        for b in &corpus {
            let same_text = a.events.project().render() == b.events.project().render();
            assert_eq!(
                same_text,
                a.events.iter_canonical().eq(b.events.iter_canonical()),
                "text equality and value equality disagree"
            );
        }
    }
}
