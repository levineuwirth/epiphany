//! The family's lines on the page (X4d.1), each on a hand-written file:
//! the notes under an ottava at their written place.

use std::path::Path;

use epiphany_cli::{engrave_loaded, load, Loaded};
use epiphany_core::{Event, EventId, TypedObjectId};
use epiphany_layout_ir::{ResolvedGlyph, ResolvedLayoutIR};

/// A part of one treble staff in 4/4, an eighth per division, its measures
/// as written.
fn part(name: &str, measures: &[String]) -> Loaded {
    let body: String = measures
        .iter()
        .enumerate()
        .map(|(m, content)| {
            let attributes = if m == 0 {
                "<attributes><divisions>2</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>"
            } else {
                ""
            };
            format!(
                "<measure number=\"{}\">{attributes}{content}</measure>",
                m + 1
            )
        })
        .collect();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert!(loaded.fidelity.passed(), "{:#?}", loaded.fidelity.failures);
    loaded
}

/// A quarter note at a step and octave (`"C5"`).
fn q(pitch: &str) -> String {
    let (step, octave) = pitch.split_at(1);
    format!(
        "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
         <duration>2</duration><voice>1</voice><type>quarter</type></note>"
    )
}

/// An ottava's start, `"down"` an 8va, `"up"` an 8vb, of `size` 8 or 15, or
/// its stop.
fn octave_shift(kind: &str, size: u8) -> String {
    format!(
        "<direction><direction-type><octave-shift type=\"{kind}\" size=\"{size}\"/>\
         </direction-type></direction>"
    )
}

/// The part's events in the file's order.
fn events(loaded: &Loaded) -> Vec<EventId> {
    loaded.import.ids.events[0].clone()
}

/// An ink box, `[left, bottom, right, top]`.
type Ink = [f32; 4];

fn glyph_box(glyph: &ResolvedGlyph) -> Ink {
    let (x, y, b) = (glyph.position.x.0, glyph.position.y.0, &glyph.bounding_box);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

/// A note's head.
fn head(loaded: &Loaded, layout: &ResolvedLayoutIR, event: EventId) -> Ink {
    let Some(Event::Pitched(n)) = loaded.reduced.score.events.get(event) else {
        panic!("a note");
    };
    let found: Vec<Ink> = layout
        .glyphs
        .iter()
        .filter(|g| {
            matches!(&g.provenance.source, TypedObjectId::Pitch(p) if n.pitches.iter().any(|q| q.id == *p))
                && g.glyph.as_str().starts_with("notehead")
        })
        .map(glyph_box)
        .collect();
    assert_eq!(found.len(), 1, "one head");
    found[0]
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} against {b}");
}

/// The notes under an ottava stand at their written place, its octaves
/// nearer the staff (Gould, pp. 28 to 31; MuseScore's `Note::epitch`): the
/// file gives an 8va's C7 at its sounding pitch, drawn where a C6 stands; an
/// 8vb's C4 where a C5 stands; a 15ma's C8 where a C6 stands; a note after
/// the ottava at its own place.
#[test]
fn the_notes_under_an_ottava_stand_at_their_written_place() {
    for (name, kind, size, sounding, written) in [
        ("ottava_8va.musicxml", "down", 8, "C7", "C6"),
        ("ottava_8vb.musicxml", "up", 8, "C4", "C5"),
        ("ottava_15ma.musicxml", "down", 15, "C8", "C6"),
    ] {
        let loaded = part(
            name,
            &[format!(
                "{}{}{}{}{}{}",
                octave_shift(kind, size),
                q(sounding),
                q(sounding),
                octave_shift("stop", size),
                q(written),
                q(written),
            )],
        );
        let layout = engrave_loaded(&loaded).layout;
        let e = events(&loaded);
        let under = head(&loaded, &layout, e[0]);
        let after = head(&loaded, &layout, e[2]);
        near(
            under[1],
            after[1],
            &format!("{name}: {sounding} drawn at {written}"),
        );
    }
}
