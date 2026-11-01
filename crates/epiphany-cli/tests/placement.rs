//! The family's placement (X4d.3, D85), each on a hand-written file: a
//! dynamic moved clear of a barline running through the staves
//! (`DynamicsLayout::manageBarlineCollisions`), and a mark at a note placed
//! against its column's ink on its staff, another voice's included
//! (`ChordLayout::layoutArticulations2`, `Autoplace::autoplaceSegmentElement`).

use std::path::Path;

use epiphany_cli::{engrave_loaded, load, Loaded};
use epiphany_layout_ir::{ResolvedLayoutIR, ResolvedText};

/// A part of `staves` staves (a treble and, with two, a bass), in 4/4 with
/// an eighth per division, its measures as written.
fn part(name: &str, staves: u8, measures: &[String]) -> Loaded {
    let clefs = if staves == 2 {
        "<staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef>\
         <clef number=\"2\"><sign>F</sign><line>4</line></clef>"
    } else {
        "<clef><sign>G</sign><line>2</line></clef>"
    };
    let body: String = measures
        .iter()
        .enumerate()
        .map(|(m, content)| {
            let attributes = if m == 0 {
                format!(
                    "<attributes><divisions>2</divisions><time><beats>4</beats>\
                     <beat-type>4</beat-type></time>{clefs}</attributes>"
                )
            } else {
                String::new()
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

fn note(pitch: &str, duration: u8, kind: &str, staff: u8, notations: &str) -> String {
    let (step, octave) = pitch.split_at(1);
    let notations = if notations.is_empty() {
        String::new()
    } else {
        format!("<notations>{notations}</notations>")
    };
    format!(
        "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
         <duration>{duration}</duration><voice>{staff}</voice><type>{kind}</type>\
         <staff>{staff}</staff>{notations}</note>"
    )
}

/// Dynamic words (`<other-dynamics>`) under staff `staff`, at the next note.
fn words(text: &str, staff: u8) -> String {
    format!(
        "<direction placement=\"below\"><direction-type><dynamics><other-dynamics>{text}\
         </other-dynamics></dynamics></direction-type><staff>{staff}</staff></direction>"
    )
}

fn backup(duration: u8) -> String {
    format!("<backup><duration>{duration}</duration></backup>")
}

fn text_box(text: &ResolvedText) -> [f32; 4] {
    let (x, y, b) = (text.origin.x.0, text.origin.y.0, &text.bounds);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

/// The run whose words are `text`.
fn run<'a>(layout: &'a ResolvedLayoutIR, text: &str) -> &'a ResolvedText {
    let found: Vec<&ResolvedText> = layout.texts.iter().filter(|t| t.text == text).collect();
    assert_eq!(found.len(), 1, "{text}");
    found[0]
}

/// The ink across of the page's barline glyphs nearest under `y`, left to
/// right: each `(left, right)`.
fn barlines(layout: &ResolvedLayoutIR, y: f32) -> Vec<(f32, f32)> {
    let mut ys: Vec<f32> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("barline") && g.position.y.0 >= y)
        .map(|g| g.position.y.0)
        .collect();
    ys.sort_by(f32::total_cmp);
    let Some(&nearest) = ys.first() else {
        return Vec::new();
    };
    let mut found: Vec<(f32, f32)> = layout
        .glyphs
        .iter()
        .filter(|g| {
            g.glyph.as_str().starts_with("barline") && (g.position.y.0 - nearest).abs() < 1e-3
        })
        .map(|g| {
            (
                g.position.x.0 + g.bounding_box.left.0,
                g.position.x.0 + g.bounding_box.right.0,
            )
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} against {b}");
}

/// A dynamic under a grand staff's upper staff, whose barline runs on
/// through the lower, stands a quarter space clear of it: wide words at a
/// measure's first note move right of the barline before (Gould: an initial
/// dynamic to the barline's right, *Behind Bars*, p. 103), and at its last
/// note left of the barline after. On one staff, whose barline runs through
/// nothing, the same words stand centred on their notes.
#[test]
fn a_dynamic_stands_clear_of_a_barline_through_the_staves() {
    let measure = |first: &str, last: &str, staves: u8| {
        let upper = format!(
            "{}{}{}{}{}{}",
            words(first, 1),
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
            words(last, 1),
            note("F5", 2, "quarter", 1, ""),
        );
        let lower = if staves == 2 {
            format!("{}{}", backup(8), note("C3", 8, "whole", 2, ""))
        } else {
            String::new()
        };
        format!("{upper}{lower}")
    };
    let grand = part(
        "dynamic_barline_grand.musicxml",
        2,
        &[
            measure("", "", 2),
            measure("molto espressivo", "pianissimo e dolcissimo", 2),
            measure("", "", 2),
        ],
    );
    let alone = part(
        "dynamic_barline_alone.musicxml",
        1,
        &[
            measure("", "", 1),
            measure("molto espressivo", "pianissimo e dolcissimo", 1),
            measure("", "", 1),
        ],
    );
    let layout = engrave_loaded(&grand).layout;
    let first = text_box(run(&layout, "molto espressivo"));
    let last = text_box(run(&layout, "pianissimo e dolcissimo"));
    let bars = barlines(&layout, first[3]);
    let before = bars
        .iter()
        .filter(|b| b.1 < (first[0] + first[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before");
    let after = bars
        .iter()
        .filter(|b| b.0 > (last[0] + last[2]) * 0.5)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline after");
    near(
        first[0],
        before.1 + 0.25,
        "the first words right of the barline",
    );
    near(
        last[2],
        after.0 - 0.25,
        "the last words left of the barline",
    );

    let layout = engrave_loaded(&alone).layout;
    let first = text_box(run(&layout, "molto espressivo"));
    let bars = barlines(&layout, first[3]);
    let before = bars
        .iter()
        .filter(|b| b.1 < (first[0] + first[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before");
    assert!(
        first[0] < before.1,
        "on one staff the words stay over the barline"
    );
    let last = text_box(run(&layout, "pianissimo e dolcissimo"));
    let after = bars
        .iter()
        .filter(|b| b.0 > (last[0] + last[2]) * 0.5)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline after");
    assert!(
        last[2] > after.0 - 0.25,
        "on one staff the last words reach the barline"
    );
}
