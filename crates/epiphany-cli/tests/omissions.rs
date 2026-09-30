//! The engraving-omission count on the importer's hand-written fixtures.
//! Each test changes a real layout and checks that the count moves with it,
//! so it holds whatever the engraver draws today.

use std::path::{Path, PathBuf};

use epiphany_cli::omissions::omissions;
use epiphany_cli::{engrave, load, Engraved, Loaded};
use epiphany_core::{Event, TypedObjectId};
use epiphany_layout_ir::GlyphReference;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../epiphany-musicxml/tests/fixtures")
        .join(name)
}

fn loaded(name: &str) -> (Loaded, Engraved) {
    let loaded = load(&fixture(name)).expect("the fixture loads");
    let engraved = engrave(&loaded.reduced.score);
    (loaded, engraved)
}

fn count(loaded: &Loaded, engraved: &Engraved, kind: &str) -> usize {
    omissions(
        &loaded.reduced.score,
        &engraved.layout,
        &engraved.diagnostics,
    )
    .kinds
    .get(kind)
    .copied()
    .unwrap_or(0)
}

#[test]
fn a_slur_with_no_ink_is_counted_and_an_anchor_is_not_ink() {
    let (loaded, mut engraved) = loaded("ties_and_slurs.musicxml");
    let slurs = loaded.reduced.score.cross_cutting.slurs.len();
    assert_eq!(slurs, 2);
    let is_slur = |source: &TypedObjectId| matches!(source, TypedObjectId::Slur(_));
    let anchor = engraved
        .layout
        .strokes
        .iter()
        .find(|s| s.from == s.to)
        .cloned()
        .expect("the layout carries a traced anchor");
    // Re-attributed rather than removed, so the systems' indices stay valid.
    let region = TypedObjectId::Region(loaded.reduced.score.canvas.regions[0].id);
    for g in &mut engraved.layout.glyphs {
        if is_slur(&g.provenance.source) {
            g.provenance.source = region;
        }
    }
    for s in &mut engraved.layout.strokes {
        if is_slur(&s.provenance.source) {
            s.provenance.source = region;
        }
    }
    for c in &mut engraved.layout.curves {
        if is_slur(&c.provenance.source) {
            c.provenance.source = region;
        }
    }
    assert_eq!(count(&loaded, &engraved, "slur not drawn"), slurs);
    // A zero-length stroke traced to a slur is still no ink for it.
    let mut traced = anchor;
    traced.provenance.source = TypedObjectId::Slur(loaded.reduced.score.cross_cutting.slurs[0].id);
    engraved.layout.strokes.push(traced);
    assert_eq!(count(&loaded, &engraved, "slur not drawn"), slurs);
}

#[test]
fn a_notehead_removed_is_counted_and_a_dot_supplied_is_not() {
    let (loaded, mut engraved) = loaded("single_part.musicxml");
    let score = &loaded.reduced.score;
    let voice = &score.canvas.regions[0].staff_instances()[0].voices[0];
    let Some(Event::Pitched(dotted)) = score.events.get(voice.events[0]) else {
        panic!("the first event is the dotted D5")
    };
    let pitch = TypedObjectId::Pitch(dotted.pitches[0].id);

    // Re-attributed rather than removed, so the systems' indices stay valid.
    let region = TypedObjectId::Region(score.canvas.regions[0].id);
    let before = count(&loaded, &engraved, "notehead not drawn");
    let mut without = engraved.layout.clone();
    for g in &mut without.glyphs {
        if g.provenance.source == pitch && g.glyph.as_str().starts_with("notehead") {
            g.provenance.source = region;
        }
    }
    let spoiled = Engraved {
        layout: without,
        diagnostics: engraved.diagnostics.clone(),
        time: engraved.time,
    };
    assert_eq!(count(&loaded, &spoiled, "notehead not drawn"), before + 1);

    // Whatever dots the engraver draws for this note are taken away first.
    let event = TypedObjectId::Event(voice.events[0]);
    for g in &mut engraved.layout.glyphs {
        if (g.provenance.source == pitch || g.provenance.source == event)
            && g.glyph.as_str() == "augmentationDot"
        {
            g.provenance.source = region;
        }
    }
    let dots = count(&loaded, &engraved, "augmentation dot");
    let mut dot = engraved
        .layout
        .glyphs
        .iter()
        .find(|g| g.provenance.source == pitch)
        .cloned()
        .expect("the dotted note has ink");
    dot.glyph = GlyphReference::owned("augmentationDot");
    engraved.layout.glyphs.push(dot);
    let after = count(&loaded, &engraved, "augmentation dot");
    assert!(dots >= 1, "the dotted quarter without a dot is counted");
    assert_eq!(after, dots - 1, "a drawn dot satisfies the check");
}

#[test]
fn an_octave_clef_drawn_without_its_mark_is_counted() {
    // The bass clarinet and the double bass read 8vb clefs.
    let (loaded, mut engraved) = loaded("concert_transposing.musicxml");
    let score = &loaded.reduced.score;
    let octave: Vec<TypedObjectId> = score.canvas.regions[0]
        .staff_instances()
        .iter()
        .filter(|i| i.clef_sequence.iter().any(|c| c.clef.octave_shift != 0))
        .map(|i| TypedObjectId::StaffInstance(i.id))
        .collect();
    assert_eq!(octave.len(), 2);
    let set = |engraved: &mut Engraved, suffix: &str| {
        for g in &mut engraved.layout.glyphs {
            if octave.contains(&g.provenance.source) && g.glyph.as_str().contains("Clef") {
                let base = if g.glyph.as_str().starts_with('g') {
                    "gClef"
                } else {
                    "fClef"
                };
                g.glyph = GlyphReference::owned(format!("{base}{suffix}"));
            }
        }
    };
    set(&mut engraved, "");
    assert_eq!(count(&loaded, &engraved, "clef octave mark"), 2);
    set(&mut engraved, "8vb");
    assert_eq!(count(&loaded, &engraved, "clef octave mark"), 0);
    assert_eq!(
        count(
            &loaded,
            &engraved,
            "clef of another shape at a system start"
        ),
        0
    );
}
