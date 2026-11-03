//! Expression text on the page (X4d.6), each case on a hand-written file:
//! italic, under its staff or over a voice's staff, after a dynamic at its
//! note, and outside the dynamics and hairpins where a hairpin starts there,
//! as Gould sets expression marks (*Behind Bars*, pp. 489, 495 and 434) and
//! the owner ruled where MuseScore 4.7.5 sets them otherwise.

use std::path::Path;

use epiphany_cli::{engrave_loaded, load, Loaded};
use epiphany_core::{Event, EventId, SpannerKind, TypedObjectId};
use epiphany_layout_ir::{ResolvedGlyph, ResolvedLayoutIR, ResolvedText};

/// An ink box, `[left, bottom, right, top]`.
type Ink = [f32; 4];

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

/// A quarter note, with a syllable where `syllable` names one.
fn q(pitch: &str, syllable: Option<&str>) -> String {
    let (step, octave) = pitch.split_at(1);
    let lyric = syllable.map_or(String::new(), |text| {
        format!("<lyric number=\"1\"><syllabic>single</syllabic><text>{text}</text></lyric>")
    });
    format!(
        "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
         <duration>2</duration><voice>1</voice><type>quarter</type>{lyric}</note>"
    )
}

fn direction(content: &str) -> String {
    format!("<direction><direction-type>{content}</direction-type></direction>")
}

/// Expression text, as MuseScore writes it: italic words.
fn expression(words: &str) -> String {
    direction(&format!("<words font-style=\"italic\">{words}</words>"))
}

fn dynamic(name: &str) -> String {
    direction(&format!("<dynamics><{name}/></dynamics>"))
}

fn wedge(kind: &str) -> String {
    direction(&format!("<wedge type=\"{kind}\"/>"))
}

fn events(loaded: &Loaded) -> Vec<EventId> {
    loaded.import.ids.events[0].clone()
}

fn glyph_box(glyph: &ResolvedGlyph) -> Ink {
    let (x, y, b) = (glyph.position.x.0, glyph.position.y.0, &glyph.bounding_box);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

fn text_box(text: &ResolvedText) -> Ink {
    let (x, y, b) = (text.origin.x.0, text.origin.y.0, &text.bounds);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

/// The one run whose string is `words`.
fn run<'a>(layout: &'a ResolvedLayoutIR, words: &str) -> &'a ResolvedText {
    let found: Vec<&ResolvedText> = layout.texts.iter().filter(|t| t.text == words).collect();
    assert_eq!(found.len(), 1, "{words}");
    found[0]
}

fn italic(text: &ResolvedText) -> bool {
    format!("{:?}", text.shaping.faces[0].family).contains("Italic")
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

/// The glyphs named `name`, left to right, each with its baseline.
fn named(layout: &ResolvedLayoutIR, name: &str) -> Vec<(Ink, f32)> {
    let mut found: Vec<(Ink, f32)> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == name)
        .map(|g| (glyph_box(g), g.position.y.0))
        .collect();
    found.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
    found
}

/// Each hairpin's ink, left to right.
fn hairpins(loaded: &Loaded, layout: &ResolvedLayoutIR) -> Vec<Ink> {
    let mut found: Vec<Ink> = loaded
        .reduced
        .score
        .cross_cutting
        .spanners
        .iter()
        .filter(|s| matches!(s.kind, SpannerKind::Hairpin(_)))
        .filter_map(|s| {
            layout
                .strokes
                .iter()
                .filter(|k| k.provenance.source == TypedObjectId::Spanner(s.id))
                .map(|k| {
                    [
                        k.from.x.0.min(k.to.x.0),
                        k.from.y.0.min(k.to.y.0),
                        k.from.x.0.max(k.to.x.0),
                        k.from.y.0.max(k.to.y.0),
                    ]
                })
                .reduce(|a, b| {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                })
        })
        .collect();
    found.sort_by(|a, b| a[0].total_cmp(&b[0]));
    found
}

/// The bottom and top lines of the page's first staff.
fn staff(layout: &ResolvedLayoutIR) -> (f32, f32) {
    let ys: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .map(|s| s.from.y.0)
        .collect();
    let top = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    (top - 4.0, top)
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} against {b}");
}

/// Expression text is italic and stands under its staff: alone at its note,
/// its baseline 2.5 under the staff and its words from the note's left; a
/// dynamic at its note, after the dynamic on its baseline, half a space
/// after a piano and 0.2 after a forte; where a dynamic and a hairpin start
/// at its note, under the two, from the dynamic's left, the hairpin starting
/// after the dynamic as it does alone; and where a hairpin starts with no
/// dynamic, under the hairpin, from the note's left.
#[test]
fn expression_text_stands_italic_under_its_staff_after_its_dynamic() {
    let notes = |first: &str| format!("{first}{}{}{}", q("A4", None), q("G4", None), q("A4", None));
    let loaded = part(
        "expression_text.musicxml",
        &[
            format!("{}{}", expression("dolce"), notes(&q("G4", None))),
            format!(
                "{}{}{}",
                dynamic("p"),
                expression("cantabile"),
                notes(&q("G4", None))
            ),
            format!(
                "{}{}{}",
                dynamic("f"),
                expression("espressivo"),
                notes(&q("G4", None))
            ),
            format!(
                "{}{}{}{}{}{}{}{}{}",
                dynamic("p"),
                wedge("crescendo"),
                expression("leggiero"),
                q("G4", None),
                q("A4", None),
                wedge("stop"),
                dynamic("f"),
                q("G4", None),
                q("A4", None),
            ),
            format!(
                "{}{}{}{}{}{}{}",
                wedge("diminuendo"),
                expression("sostenuto"),
                q("G4", None),
                q("A4", None),
                wedge("stop"),
                q("G4", None),
                q("A4", None),
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let (bottom, _) = staff(&layout);
    for words in ["dolce", "cantabile", "espressivo", "leggiero", "sostenuto"] {
        let r = run(&layout, words);
        assert!(italic(r), "{words}: italic");
        assert!(text_box(r)[3] < bottom, "{words}: under the staff");
    }

    let dolce = run(&layout, "dolce");
    near(dolce.origin.y.0, bottom - 2.5, "alone, 2.5 under the staff");
    near(
        dolce.origin.x.0,
        head(&loaded, &layout, e[0])[0],
        "alone, from its note's left",
    );

    // The dynamic nearest a run on its system: the file holds two.
    let nearest = |name: &str, run: &ResolvedText| {
        let (x, y) = (run.origin.x.0, run.origin.y.0);
        named(&layout, name)
            .into_iter()
            .filter(|(_, baseline)| (baseline - y).abs() < 4.0)
            .min_by(|a, b| (a.0[0] - x).abs().total_cmp(&(b.0[0] - x).abs()))
            .expect("a dynamic on its system")
    };
    let cantabile = run(&layout, "cantabile");
    let piano = nearest("dynamicPiano", cantabile);
    near(
        cantabile.origin.x.0,
        piano.0[2] + 0.5,
        "half a space after the piano",
    );
    near(cantabile.origin.y.0, piano.1, "on the piano's baseline");
    let espressivo = run(&layout, "espressivo");
    let forte = nearest("dynamicForte", espressivo);
    near(
        espressivo.origin.x.0,
        forte.0[2] + 0.2,
        "0.2 after the forte",
    );
    near(espressivo.origin.y.0, forte.1, "on the forte's baseline");

    let wedges = hairpins(&loaded, &layout);
    assert_eq!(wedges.len(), 2, "two hairpins");
    let leggiero = run(&layout, "leggiero");
    let piano = nearest("dynamicPiano", leggiero);
    near(
        leggiero.origin.x.0,
        piano.0[0],
        "under the piano, from its left",
    );
    near(
        wedges[0][0],
        piano.0[2] + 0.5,
        "the hairpin after the piano",
    );
    let leggiero = text_box(leggiero);
    assert!(
        leggiero[3] <= wedges[0][1].min(piano.0[1]) - 0.5 + 0.01,
        "under the piano and the hairpin: {leggiero:?} {:?}",
        wedges[0]
    );
    let sostenuto = run(&layout, "sostenuto");
    near(
        sostenuto.origin.x.0,
        head(&loaded, &layout, e[16])[0],
        "from its note's left",
    );
    assert!(
        text_box(sostenuto)[3] <= wedges[1][1] - 0.5 + 0.01,
        "under the hairpin"
    );
}

/// Over a voice's staff, which takes its dynamics over it (D79 Q10),
/// expression text stands over the staff, its baseline 1.5 over it.
#[test]
fn expression_text_stands_over_a_voices_staff() {
    let loaded = part(
        "expression_vocal.musicxml",
        &[format!(
            "{}{}{}{}{}",
            expression("dolce"),
            q("C5", Some("la")),
            q("D5", Some("la")),
            q("E5", Some("la")),
            q("F5", Some("la")),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let (_, top) = staff(&layout);
    let dolce = run(&layout, "dolce");
    assert!(italic(dolce), "italic");
    near(dolce.origin.y.0, top + 1.5, "1.5 over the staff");
}

/// Expression text after a dynamic moves with it: on a grand staff, wide
/// dynamic words at a measure's first note, moved right of the barline the
/// staves share (X4d.3), keep their expression half a space after them.
#[test]
fn expression_text_moves_with_its_dynamic_off_a_barline() {
    let upper = |words: bool| {
        let marks = if words {
            format!(
                "{}{}",
                direction("<dynamics><other-dynamics>molto espressivo</other-dynamics></dynamics>"),
                expression("dolce")
            )
            .replace(
                "</direction-type></direction>",
                "</direction-type><staff>1</staff></direction>",
            )
        } else {
            String::new()
        };
        format!(
            "{marks}{}{}{}{}",
            q("G4", None),
            q("A4", None),
            q("G4", None),
            q("A4", None)
        )
        .replace("</type>", "</type><staff>1</staff>")
    };
    let lower = "<backup><duration>8</duration></backup><note><pitch><step>C</step>\
                 <octave>3</octave></pitch><duration>8</duration><voice>5</voice>\
                 <type>whole</type><staff>2</staff></note>";
    let body: String = [false, true, false]
        .iter()
        .enumerate()
        .map(|(m, &words)| {
            let attributes = if m == 0 {
                "<attributes><divisions>2</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><staves>2</staves><clef number=\"1\">\
                 <sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign>\
                 <line>4</line></clef></attributes>"
            } else {
                ""
            };
            format!(
                "<measure number=\"{}\">{attributes}{}{lower}</measure>",
                m + 1,
                upper(words)
            )
        })
        .collect();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("expression_barline.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert!(loaded.fidelity.passed(), "{:#?}", loaded.fidelity.failures);
    let layout = engrave_loaded(&loaded).layout;
    let words = text_box(run(&layout, "molto espressivo"));
    let barline = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("barline"))
        .map(glyph_box)
        .filter(|b| b[0] < words[0] + 1.0)
        .max_by(|a, b| a[0].total_cmp(&b[0]))
        .expect("the barline before");
    near(
        words[0],
        barline[2] + 0.25,
        "the words moved right of the barline",
    );
    near(
        run(&layout, "dolce").origin.x.0,
        words[2] + 0.5,
        "the expression half a space after them",
    );
}

/// A verbal crescendo is a hairpin of the line type (X4d.1): expression text
/// at the note one starts at stands outside its words and line, from the
/// note's left, as under a hairpin.
#[test]
fn expression_text_stands_outside_a_verbal_crescendo_starting_with_it() {
    let line = |content: &str| {
        format!(
            "<direction><direction-type>{content}</direction-type>\
             <direction-type><dashes type=\"start\"/></direction-type></direction>"
        )
    };
    let loaded = part(
        "expression_verbal.musicxml",
        &[format!(
            "{}{}{}{}{}{}{}",
            line("<words font-style=\"italic\">cresc.</words>"),
            expression("tranquillo"),
            q("G4", None),
            q("A4", None),
            q("G4", None),
            direction("<dashes type=\"stop\"/>"),
            q("A4", None),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let cresc = text_box(run(&layout, "cresc."));
    let tranquillo = run(&layout, "tranquillo");
    near(
        tranquillo.origin.x.0,
        head(&loaded, &layout, e[0])[0],
        "from its note's left",
    );
    assert!(
        text_box(tranquillo)[3] <= cresc[1] - 0.5 + 0.01,
        "under the crescendo's words: {:?} {cresc:?}",
        text_box(tranquillo)
    );
}
