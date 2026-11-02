//! The family's placement (X4d.3, D85), each on a hand-written file: a
//! dynamic moved clear of a barline running through the staves
//! (`DynamicsLayout::manageBarlineCollisions`), and a mark at a note placed
//! against its column's ink on its staff, another voice's included
//! (`ChordLayout::layoutArticulations2`, `Autoplace::autoplaceSegmentElement`).

use std::path::Path;

use epiphany_cli::{engrave_loaded, load, Loaded};
use epiphany_core::TypedObjectId;
use epiphany_layout_ir::{ResolvedGlyph, ResolvedLayoutIR, ResolvedText};

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

/// A note of voice `voice` on the first staff.
fn voiced(pitch: &str, duration: u8, kind: &str, voice: u8, notations: &str) -> String {
    note(pitch, duration, kind, 1, notations)
        .replace("<voice>1</voice>", &format!("<voice>{voice}</voice>"))
}

/// A rest of voice `voice` on the first staff.
fn rest(duration: u8, kind: &str, voice: u8) -> String {
    format!(
        "<note><rest/><duration>{duration}</duration><voice>{voice}</voice><type>{kind}</type>\
         <staff>1</staff></note>"
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

fn glyph_box(glyph: &ResolvedGlyph) -> [f32; 4] {
    let (x, y, b) = (glyph.position.x.0, glyph.position.y.0, &glyph.bounding_box);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

/// The one glyph named `name`.
fn only(layout: &ResolvedLayoutIR, name: &str) -> [f32; 4] {
    let found: Vec<[f32; 4]> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == name)
        .map(glyph_box)
        .collect();
    assert_eq!(found.len(), 1, "{name}");
    found[0]
}

/// The top and bottom lines of the page's one staff.
fn staff_lines(layout: &ResolvedLayoutIR) -> (f32, f32) {
    let ys: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .map(|s| s.from.y.0)
        .collect();
    let top = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let bottom = ys.iter().copied().fold(f32::INFINITY, f32::min);
    near(top - bottom, 4.0, "one staff");
    (top, bottom)
}

/// The heads on the page, `pick` choosing one by its box.
fn head_by(layout: &ResolvedLayoutIR, pick: impl Fn(&[f32; 4], &[f32; 4]) -> bool) -> [f32; 4] {
    layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .map(glyph_box)
        .reduce(|a, b| if pick(&b, &a) { b } else { a })
        .expect("a head")
}

/// An accent outside the staff at a note stands clear of another voice's
/// ink in its column, as of its own chord's, by `articulationMinDistance`:
/// an upper voice's accent over the head of a lower voice crossing above
/// it, a lower voice's under the head of an upper voice crossing below.
/// With a rest there instead, each stands where its own chord sets it.
#[test]
fn an_accent_stands_clear_of_another_voices_ink_in_its_column() {
    let accent = "<articulations><accent/></articulations>";
    for (name, above) in [
        ("accent_column_above", true),
        ("accent_column_below", false),
    ] {
        let file = |crossing: bool| {
            let (own, other) = if above { ("E4", "A5") } else { ("G4", "F3") };
            let (marked_voice, other_voice) = if above { (1, 2) } else { (2, 1) };
            let marked = format!(
                "{}{}{}{}",
                voiced(own, 2, "quarter", marked_voice, accent),
                voiced(own, 2, "quarter", marked_voice, ""),
                voiced(own, 2, "quarter", marked_voice, ""),
                voiced(own, 2, "quarter", marked_voice, ""),
            );
            let first = if crossing {
                voiced(other, 2, "quarter", other_voice, "")
            } else {
                rest(2, "quarter", other_voice)
            };
            let other = format!(
                "{first}{}{}",
                rest(4, "half", other_voice),
                rest(2, "quarter", other_voice)
            );
            let measure = if above {
                format!("{marked}{}{other}", backup(8))
            } else {
                format!("{other}{}{marked}", backup(8))
            };
            let suffix = if crossing { "" } else { "_alone" };
            part(&format!("{name}{suffix}.musicxml"), 1, &[measure])
        };
        let glyph = if above {
            "articAccentAbove"
        } else {
            "articAccentBelow"
        };
        let layout = engrave_loaded(&file(true)).layout;
        let mark = only(&layout, glyph);
        if above {
            let crossing = head_by(&layout, |a, b| a[3] > b[3]);
            assert!(
                mark[0] < crossing[2] && mark[2] > crossing[0],
                "{name}: across"
            );
            near(
                mark[1],
                crossing[3] + 0.4,
                &format!("{name}: over the head"),
            );
        } else {
            let crossing = head_by(&layout, |a, b| a[1] < b[1]);
            assert!(
                mark[0] < crossing[2] && mark[2] > crossing[0],
                "{name}: across"
            );
            near(
                mark[3],
                crossing[1] - 0.4,
                &format!("{name}: under the head"),
            );
        }
        // From the staff: the staff stands lower on its page under the
        // crossing head's ink.
        let (top, bottom) = staff_lines(&layout);
        let alone_layout = engrave_loaded(&file(false)).layout;
        let alone = only(&alone_layout, glyph);
        let (alone_top, alone_bottom) = staff_lines(&alone_layout);
        if above {
            assert!(
                alone[1] - alone_top < mark[1] - top - 0.5,
                "{name}: alone it stands nearer"
            );
        } else {
            assert!(
                alone[3] - alone_bottom > mark[3] - bottom + 0.5,
                "{name}: alone it stands nearer"
            );
        }
    }
}

/// The stem of another voice stemmed the same way is ink of the column too:
/// a third voice's note above the first, its stem up beside the first's,
/// lifts the first voice's accent over its stem's end. With a rest there
/// instead, the accent stands over the staff.
#[test]
fn an_accent_stands_clear_of_another_voices_stem() {
    let accent = "<articulations><accent/></articulations>";
    let file = |third: bool| {
        let first = format!(
            "{}{}{}{}",
            voiced("E4", 2, "quarter", 1, accent),
            voiced("E4", 2, "quarter", 1, ""),
            voiced("E4", 2, "quarter", 1, ""),
            voiced("E4", 2, "quarter", 1, ""),
        );
        let opening = if third {
            voiced("C5", 2, "quarter", 3, "")
        } else {
            rest(2, "quarter", 3)
        };
        let measure = format!(
            "{first}{}{opening}{}{}{}{}",
            backup(8),
            rest(4, "half", 3),
            rest(2, "quarter", 3),
            backup(8),
            rest(8, "whole", 2),
        );
        let suffix = if third { "" } else { "_alone" };
        part(
            &format!("accent_column_stem{suffix}.musicxml"),
            1,
            &[measure],
        )
    };
    let layout = engrave_loaded(&file(true)).layout;
    let mark = only(&layout, "articAccentAbove");
    let (top, _) = staff_lines(&layout);
    // The highest stroke end across the accent: the third voice's stem.
    let stem_top = layout
        .strokes
        .iter()
        .filter(|s| s.from.x.0 > mark[0] && s.from.x.0 < mark[2])
        .map(|s| s.from.y.0.max(s.to.y.0))
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(stem_top > top + 1.5, "a stem rises over the staff");
    near(mark[1], stem_top + 0.4, "over the stem's end");
    let alone_layout = engrave_loaded(&file(false)).layout;
    let alone = only(&alone_layout, "articAccentAbove");
    let (alone_top, _) = staff_lines(&alone_layout);
    assert!(
        alone[1] - alone_top < mark[1] - top - 0.5,
        "alone it stands nearer"
    );
}

/// A dynamic drawn as a glyph moves off a barline through the staves as
/// dynamic words do: on a grand staff a wide pppppp at a measure's first note
/// stands a quarter space right of the barline before it; on one staff it
/// stays centred on its note, over the barline.
#[test]
fn a_dynamic_glyph_stands_clear_of_a_barline_through_the_staves() {
    let dynamic = |name: &str| {
        format!(
            "<direction placement=\"below\"><direction-type><dynamics><{name}/></dynamics>\
             </direction-type><staff>1</staff></direction>"
        )
    };
    let measure = |marked: bool, staves: u8| {
        let first = if marked {
            dynamic("pppppp")
        } else {
            String::new()
        };
        let upper = format!(
            "{first}{}{}{}{}",
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
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
        "dynamic_glyph_barline_grand.musicxml",
        2,
        &[measure(false, 2), measure(true, 2), measure(false, 2)],
    );
    let layout = engrave_loaded(&grand).layout;
    let soft = only(&layout, "dynamicPPPPPP");
    let bars = barlines(&layout, soft[3]);
    let before = bars
        .iter()
        .filter(|b| b.1 < (soft[0] + soft[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before");
    near(soft[0], before.1 + 0.25, "the pppppp right of the barline");

    let alone = part(
        "dynamic_glyph_barline_alone.musicxml",
        1,
        &[measure(false, 1), measure(true, 1), measure(false, 1)],
    );
    let layout = engrave_loaded(&alone).layout;
    let soft = only(&layout, "dynamicPPPPPP");
    let bars = barlines(&layout, soft[3]);
    let before = bars
        .iter()
        .filter(|b| b.1 < (soft[0] + soft[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before");
    assert!(
        soft[0] < before.1,
        "on one staff the pppppp stays over the barline"
    );
}

/// A dynamic moved off a barline is placed off its staff where it is moved
/// to: in two parts a bracket joins, a wide pppppp at a measure's first note,
/// moved right of the barline before it, reaches the ledger lines of the low
/// note after it, which it does not reach unmoved, and stands under that
/// note's ink.
#[test]
fn a_dynamic_moved_off_a_barline_stands_clear_of_the_ink_it_moves_to() {
    let dynamic = "<direction placement=\"below\"><direction-type><dynamics><pppppp/>\
                   </dynamics></direction-type></direction>";
    let quarter = |pitch: &str| {
        let (step, octave) = pitch.split_at(1);
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>2</duration><voice>1</voice><type>quarter</type></note>"
        )
    };
    let upper = |marked: bool| {
        format!(
            "{}{}{}{}{}",
            if marked { dynamic } else { "" },
            quarter("C5"),
            quarter("A3"),
            quarter("C5"),
            quarter("C5"),
        )
    };
    let lower = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration>\
                 <voice>1</voice><type>whole</type></note>";
    let attributes = "<attributes><divisions>2</divisions><time><beats>4</beats>\
                      <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                      </attributes>";
    let measures = |content: &dyn Fn(usize) -> String| -> String {
        (0..3)
            .map(|m| {
                format!(
                    "<measure number=\"{}\">{}{}</measure>",
                    m + 1,
                    if m == 0 { attributes } else { "" },
                    content(m)
                )
            })
            .collect()
    };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><part-group type=\"start\" number=\"1\">\
         <group-symbol>bracket</group-symbol><group-barline>yes</group-barline></part-group>\
         <score-part id=\"P1\"><part-name>A</part-name></score-part><score-part id=\"P2\">\
         <part-name>B</part-name></score-part><part-group type=\"stop\" number=\"1\"/>\
         </part-list><part id=\"P1\">{}</part><part id=\"P2\">{}</part></score-partwise>",
        measures(&|m| upper(m == 1)),
        measures(&|_| lower.to_string()),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dynamic_moved_onto_ink.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert!(loaded.fidelity.passed(), "{:#?}", loaded.fidelity.failures);
    let layout = engrave_loaded(&loaded).layout;
    let soft = only(&layout, "dynamicPPPPPP");
    let bars = barlines(&layout, soft[3]);
    let before = bars
        .iter()
        .filter(|b| b.1 < (soft[0] + soft[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before");
    near(soft[0], before.1 + 0.25, "moved right of the barline");
    // The low note's ledger lines, under the dynamic's moved span alone.
    let ledgers: Vec<[f32; 4]> = layout
        .strokes
        .iter()
        .filter(|s| (s.from.y.0 - s.to.y.0).abs() < 1e-4 && (s.to.x.0 - s.from.x.0).abs() < 2.0)
        .map(|s| {
            [
                s.from.x.0.min(s.to.x.0),
                s.from.y.0,
                s.from.x.0.max(s.to.x.0),
                s.from.y.0,
            ]
        })
        .filter(|l| l[0] < soft[2] && l[2] > soft[0] && l[1] > soft[1] - 2.0)
        .collect();
    assert!(
        !ledgers.is_empty(),
        "the low note's ledger lines under the moved span"
    );
    let lowest = ledgers.iter().map(|l| l[1]).fold(f32::INFINITY, f32::min);
    assert!(
        soft[3] < lowest,
        "under the ledger lines: {soft:?} {lowest}"
    );
}

/// A hairpin meeting a dynamic moved off a barline moves its end with it: on
/// a grand staff, a crescendo from a wide pppppp at a measure's first note,
/// moved right of the barline before it, starts half a space after it, and a
/// diminuendo to dynamic words at a measure's last note, moved left of the
/// barline after them, ends half a space before them, as each does at a
/// dynamic left in place.
#[test]
fn a_hairpin_moves_its_ends_with_dynamics_moved_off_barlines() {
    let direction = |content: &str| {
        format!(
            "<direction placement=\"below\"><direction-type>{content}</direction-type>\
             <staff>1</staff></direction>"
        )
    };
    let measure = |open: String, close: String| {
        format!(
            "{open}{}{}{}{close}{}{}{}",
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
            note("F5", 2, "quarter", 1, ""),
            backup(8),
            note("C3", 8, "whole", 2, ""),
        )
    };
    let stop = direction("<wedge type=\"stop\"/>");
    let loaded = part(
        "hairpin_moved_dynamics.musicxml",
        2,
        &[
            measure(String::new(), String::new()),
            measure(
                format!(
                    "{}{}",
                    direction("<dynamics><pppppp/></dynamics>"),
                    direction("<wedge type=\"crescendo\"/>")
                ),
                stop.clone(),
            ),
            measure(
                direction("<wedge type=\"diminuendo\"/>"),
                format!(
                    "{stop}{}",
                    direction(
                        "<dynamics><other-dynamics>pianissimo e dolcissimo</other-dynamics>\
                         </dynamics>"
                    )
                ),
            ),
            measure(String::new(), String::new()),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let soft = only(&layout, "dynamicPPPPPP");
    let words = text_box(run(&layout, "pianissimo e dolcissimo"));
    let before = barlines(&layout, soft[3])
        .into_iter()
        .filter(|b| b.1 < (soft[0] + soft[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before the pppppp");
    let after = barlines(&layout, words[3])
        .into_iter()
        .filter(|b| b.0 > (words[0] + words[2]) * 0.5)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline after the words");
    near(
        soft[0],
        before.1 + 0.25,
        "the pppppp moved right of its barline",
    );
    near(words[2], after.0 - 0.25, "the words moved left of theirs");
    // Each hairpin's legs, left and right across.
    let legs = |crescendo: bool| -> Vec<[f32; 2]> {
        let hairpin = loaded
            .reduced
            .score
            .cross_cutting
            .spanners
            .iter()
            .find(|s| match &s.kind {
                epiphany_core::SpannerKind::Hairpin(d) => {
                    (*d == epiphany_core::HairpinDirection::Crescendo) == crescendo
                }
                _ => false,
            })
            .expect("the hairpin");
        layout
            .strokes
            .iter()
            .filter(|s| s.provenance.source == TypedObjectId::Spanner(hairpin.id) && s.from != s.to)
            .map(|s| [s.from.x.0.min(s.to.x.0), s.from.x.0.max(s.to.x.0)])
            .collect()
    };
    let crescendo = legs(true);
    assert_eq!(crescendo.len(), 2, "two legs");
    for leg in crescendo {
        near(
            leg[0],
            soft[2] + 0.5,
            "the crescendo half a space after the pppppp",
        );
    }
    let diminuendo = legs(false);
    assert_eq!(diminuendo.len(), 2, "two legs");
    for leg in diminuendo {
        near(
            leg[1],
            words[0] - 0.5,
            "the diminuendo half a space before the words",
        );
    }
}

/// A hairpin a system break cuts moves its first piece's start and its last
/// piece's end with the dynamics at them moved off barlines: a crescendo from
/// a wide pppppp at a measure's first note, moved right of its barline, over
/// a break to dynamic words at the next measure's last note, moved left of
/// theirs.
#[test]
fn a_hairpin_over_a_system_break_moves_its_ends_with_its_dynamics() {
    let direction = |content: &str| {
        format!(
            "<direction placement=\"below\"><direction-type>{content}</direction-type>\
             <staff>1</staff></direction>"
        )
    };
    let measure = |open: String, close: String, break_before: bool| {
        format!(
            "{}{open}{}{}{}{close}{}{}{}",
            if break_before {
                "<print new-system=\"yes\"/>"
            } else {
                ""
            },
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
            note("F5", 2, "quarter", 1, ""),
            backup(8),
            note("C3", 8, "whole", 2, ""),
        )
    };
    let loaded = part(
        "hairpin_split_moved_dynamics.musicxml",
        2,
        &[
            measure(String::new(), String::new(), false),
            measure(
                format!(
                    "{}{}",
                    direction("<dynamics><pppppp/></dynamics>"),
                    direction("<wedge type=\"crescendo\"/>")
                ),
                String::new(),
                false,
            ),
            measure(
                String::new(),
                format!(
                    "{}{}",
                    direction("<wedge type=\"stop\"/>"),
                    direction(
                        "<dynamics><other-dynamics>pianissimo e dolcissimo</other-dynamics>\
                         </dynamics>"
                    )
                ),
                true,
            ),
            measure(String::new(), String::new(), false),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let soft = only(&layout, "dynamicPPPPPP");
    let words = text_box(run(&layout, "pianissimo e dolcissimo"));
    let hairpin = loaded
        .reduced
        .score
        .cross_cutting
        .spanners
        .iter()
        .find(|s| matches!(s.kind, epiphany_core::SpannerKind::Hairpin(_)))
        .expect("a hairpin");
    let pieces: Vec<[f32; 3]> = layout
        .strokes
        .iter()
        .filter(|s| s.provenance.source == TypedObjectId::Spanner(hairpin.id) && s.from != s.to)
        .map(|s| {
            [
                s.from.x.0.min(s.to.x.0),
                s.from.x.0.max(s.to.x.0),
                s.from.y.0,
            ]
        })
        .collect();
    assert_eq!(pieces.len(), 4, "two pieces of two legs: {pieces:?}");
    // The first system's pieces stand at the pppppp's height, the second's
    // at the words'.
    let first: Vec<&[f32; 3]> = pieces
        .iter()
        .filter(|p| (p[2] - soft[1]).abs() < 4.0)
        .collect();
    let last: Vec<&[f32; 3]> = pieces
        .iter()
        .filter(|p| (p[2] - words[1]).abs() < 4.0)
        .collect();
    assert_eq!((first.len(), last.len()), (2, 2), "{pieces:?}");
    for leg in first {
        near(
            leg[0],
            soft[2] + 0.5,
            "the first piece half a space after the pppppp",
        );
    }
    for leg in last {
        near(
            leg[1],
            words[0] - 0.5,
            "the last piece half a space before the words",
        );
    }
}

/// The staff highest on the page: its top line, and its bottom four spaces
/// under it.
fn upper(layout: &ResolvedLayoutIR) -> (f32, f32) {
    let top = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .map(|s| s.from.y.0)
        .fold(f32::NEG_INFINITY, f32::max);
    (top, top - 4.0)
}

/// The legs of the `k`th hairpin of the score, each stroke's box.
fn hairpin_legs(loaded: &Loaded, layout: &ResolvedLayoutIR, k: usize) -> Vec<[f32; 4]> {
    let ids: Vec<_> = loaded
        .reduced
        .score
        .cross_cutting
        .spanners
        .iter()
        .filter(|s| matches!(s.kind, epiphany_core::SpannerKind::Hairpin(_)))
        .map(|s| s.id)
        .collect();
    let id = ids[k];
    layout
        .strokes
        .iter()
        .filter(|s| s.provenance.source == TypedObjectId::Spanner(id) && s.from != s.to)
        .map(|s| {
            [
                s.from.x.0.min(s.to.x.0),
                s.from.y.0.min(s.to.y.0),
                s.from.x.0.max(s.to.x.0),
                s.from.y.0.max(s.to.y.0),
            ]
        })
        .collect()
}

/// A hairpin moved with dynamics moved off their barlines is placed over
/// the span it is drawn over (X4d fix round 1): a crescendo from a ppp to a
/// pp at measures' first notes, in the upper of two parts a bracket joins,
/// each dynamic moved right of its barline, stands as near its staff as the
/// same part's alone, where nothing moves. Placed over its unmoved span it
/// is set against notes the drawn hairpin does not reach, and its row,
/// dynamics with it, stands farther off. Gould keeps a hairpin and its
/// dynamics in one plane as near the notes as they go (*Behind Bars*,
/// p. 105); MuseScore lays a hairpin out from the dynamics it snaps to once
/// they move (`DynamicsLayout::manageBarlineCollisions`).
#[test]
fn a_moved_hairpin_is_placed_over_its_drawn_span() {
    let dynamic = |d: &str| {
        format!(
            "<direction placement=\"below\"><direction-type><dynamics><{d}/></dynamics>\
             </direction-type></direction>"
        )
    };
    let wedge = |t: &str| {
        format!(
            "<direction placement=\"below\"><direction-type><wedge type=\"{t}\"/>\
             </direction-type></direction>"
        )
    };
    let upper_part = |m: usize| {
        let mut s = String::new();
        if m == 1 {
            s += &dynamic("ppp");
            s += &wedge("crescendo");
        }
        if m == 2 {
            s += &wedge("stop");
            s += &dynamic("pp");
        }
        s += "<note><pitch><step>F</step><octave>5</octave></pitch><duration>6</duration>\
              <voice>1</voice><type>half</type><dot/></note>";
        s += "<note><rest/><duration>2</duration><voice>1</voice><type>quarter</type></note>";
        s
    };
    let lower = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration>\
                 <voice>1</voice><type>whole</type></note>";
    let attributes = "<attributes><divisions>2</divisions><time><beats>4</beats>\
                      <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line>\
                      </clef></attributes>";
    let measures = |content: &dyn Fn(usize) -> String| -> String {
        (0..4)
            .map(|m| {
                format!(
                    "<measure number=\"{}\">{}{}</measure>",
                    m + 1,
                    if m == 0 { attributes } else { "" },
                    content(m)
                )
            })
            .collect()
    };
    let write = |name: &str, xml: String| {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::write(&path, xml).expect("written");
        let loaded = load(&path).expect("loads");
        assert!(loaded.fidelity.passed(), "{:#?}", loaded.fidelity.failures);
        loaded
    };
    let joined = write(
        "hairpin_placed_joined.musicxml",
        format!(
            "<score-partwise version=\"4.0\"><part-list><part-group type=\"start\" number=\"1\">\
             <group-symbol>bracket</group-symbol><group-barline>yes</group-barline></part-group>\
             <score-part id=\"P1\"><part-name>A</part-name></score-part><score-part id=\"P2\">\
             <part-name>B</part-name></score-part><part-group type=\"stop\" number=\"1\"/>\
             </part-list><part id=\"P1\">{}</part><part id=\"P2\">{}</part></score-partwise>",
            measures(&upper_part),
            measures(&|_| lower.to_string()),
        ),
    );
    let alone = write(
        "hairpin_placed_alone.musicxml",
        format!(
            "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
             </part-name></score-part></part-list><part id=\"P1\">{}</part></score-partwise>",
            measures(&upper_part),
        ),
    );
    let row = |loaded: &Loaded| {
        let layout = engrave_loaded(loaded).layout;
        let soft = only(&layout, "dynamicPPP");
        let legs = hairpin_legs(loaded, &layout, 0);
        let (_, bottom) = upper(&layout);
        let low = legs.iter().map(|l| l[1]).fold(f32::INFINITY, f32::min);
        (bottom - soft[3], bottom - low, soft, layout)
    };
    let (d2, h2, soft2, layout2) = row(&joined);
    let (d1, h1, _, _) = row(&alone);
    let before = barlines(&layout2, soft2[3])
        .into_iter()
        .filter(|b| b.1 < (soft2[0] + soft2[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before");
    near(
        soft2[0],
        before.1 + 0.25,
        "the ppp moved right of its barline",
    );
    near(d2, d1, "the ppp's distance under its staff");
    near(h2, h1, "the hairpin's distance under its staff");
}

/// A note with a syllable under it.
fn lyric_note(pitch: &str, duration: u8, kind: &str, staff: u8) -> String {
    note(pitch, duration, kind, staff, "").replace(
        "</note>",
        "<lyric number=\"1\"><syllabic>single</syllabic><text>la</text></lyric></note>",
    )
}

/// A direction of `staff` holding `content`, placed above or below.
fn dir(content: &str, staff: u8, placement: &str) -> String {
    format!(
        "<direction placement=\"{placement}\"><direction-type>{content}</direction-type>\
         <staff>{staff}</staff></direction>"
    )
}

/// The barline before `ink` on its staff: the rightmost whose right edge
/// stands left of the ink's middle.
fn before_bar(layout: &ResolvedLayoutIR, ink: [f32; 4]) -> (f32, f32) {
    barlines(layout, ink[3])
        .into_iter()
        .filter(|b| b.1 < (ink[0] + ink[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before")
}

/// Words under a grand staff's lower staff, whose barline stops at it, stay
/// at their note over the barline's line (MuseScore's `barLineSpan`; Gould
/// moves a marking off a barline only where the barline runs through it,
/// *Behind Bars*, p. 103): a group's bottom staff runs on to no staff.
#[test]
fn words_under_a_groups_bottom_staff_stay_at_their_note() {
    let measure = |first: &str| {
        format!(
            "{}{}{}{}{}{}{}",
            note("C5", 8, "whole", 1, ""),
            backup(8),
            words(first, 2),
            note("C3", 2, "quarter", 2, ""),
            note("D3", 2, "quarter", 2, ""),
            note("E3", 2, "quarter", 2, ""),
            note("F3", 2, "quarter", 2, ""),
        )
    };
    let grand = part(
        "pin_bottom_staff.musicxml",
        2,
        &[measure(""), measure("molto espressivo"), measure("")],
    );
    let layout = engrave_loaded(&grand).layout;
    let first = text_box(run(&layout, "molto espressivo"));
    let before = before_bar(&layout, first);
    assert!(
        first[0] < before.1,
        "unmoved, over the barline's line: {first:?} {before:?}"
    );
}

/// Words over a voice's staff at the top of a grand staff, with no staff
/// above, stay at their note: a dynamic over its staff is judged by the
/// barline of the staff above, and none reaches above this one
/// (`manageBarlineCollisions` returns at the first staff; Gould, p. 103).
#[test]
fn words_over_the_top_staff_stay_at_their_note() {
    let measure = |first: &str| {
        let open = if first.is_empty() {
            String::new()
        } else {
            dir(
                &format!("<dynamics><other-dynamics>{first}</other-dynamics></dynamics>"),
                1,
                "above",
            )
        };
        format!(
            "{open}{}{}{}{}{}{}",
            lyric_note("C5", 2, "quarter", 1),
            lyric_note("D5", 2, "quarter", 1),
            lyric_note("E5", 2, "quarter", 1),
            lyric_note("F5", 2, "quarter", 1),
            backup(8),
            note("C3", 8, "whole", 2, ""),
        )
    };
    let grand = part(
        "pin_top_staff.musicxml",
        2,
        &[measure(""), measure("molto espressivo"), measure("")],
    );
    let layout = engrave_loaded(&grand).layout;
    let first = text_box(run(&layout, "molto espressivo"));
    let (top, _) = upper(&layout);
    assert!(first[1] > top, "over the staff: {first:?} {top}");
    // The barline glyphs: their x is every staff's.
    let before = barlines(&layout, -1.0e9)
        .into_iter()
        .filter(|b| b.1 < (first[0] + first[2]) * 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("the barline before");
    assert!(first[0] < before.1, "unmoved: {first:?} {before:?}");
}

/// A hairpin of the lower staff starting at a note where words of the upper
/// staff move off a barline keeps its start: it is in another row
/// (`DynamicsLayout` moves only the dynamic's own snapping chain; Gould
/// times a hairpin by its own notes and the dynamics at its ends, p. 104).
#[test]
fn a_hairpin_of_another_row_keeps_its_start() {
    let measure = |marked: bool, hairpin: bool| {
        let lower = if hairpin {
            format!(
                "{}{}{}",
                dir("<wedge type=\"crescendo\"/>", 2, "below"),
                note("C3", 8, "whole", 2, ""),
                dir("<wedge type=\"stop\"/>", 2, "below"),
            )
        } else {
            note("C3", 8, "whole", 2, "")
        };
        format!(
            "{}{}{}{}{}{}{lower}",
            if marked {
                words("molto espressivo", 1)
            } else {
                String::new()
            },
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
            note("F5", 2, "quarter", 1, ""),
            backup(8),
        )
    };
    let start = |marked: bool| {
        let loaded = part(
            if marked {
                "pin_rows_marked.musicxml"
            } else {
                "pin_rows_plain.musicxml"
            },
            2,
            &[
                measure(false, false),
                measure(marked, true),
                measure(false, false),
            ],
        );
        let layout = engrave_loaded(&loaded).layout;
        let legs = hairpin_legs(&loaded, &layout, 0);
        assert_eq!(legs.len(), 2, "two legs");
        let heads: Vec<f32> = layout
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("noteheadWhole"))
            .map(|g| g.position.x.0)
            .collect();
        (legs[0][0], heads)
    };
    let (marked, heads_m) = start(true);
    let (plain, heads_p) = start(false);
    // The same columns either way: the words take no room here.
    assert_eq!(heads_m.len(), heads_p.len());
    let head_m = heads_m[1];
    let head_p = heads_p[1];
    near(
        marked - head_m,
        plain - head_p,
        "the lower hairpin's start from its note",
    );
}

/// Expression text after a dynamic at a later note of a chain whose first
/// dynamic moves off a barline stays after its own dynamic, 0.2 after a
/// forte (MuseScore lays an expression out from its own dynamic's place).
#[test]
fn expression_text_moves_only_with_its_own_dynamic() {
    let measure = |marked: bool| {
        if !marked {
            return format!(
                "{}{}{}{}{}{}",
                note("C5", 2, "quarter", 1, ""),
                note("D5", 2, "quarter", 1, ""),
                note("E5", 2, "quarter", 1, ""),
                note("F5", 2, "quarter", 1, ""),
                backup(8),
                note("C3", 8, "whole", 2, ""),
            );
        }
        format!(
            "{}{}{}{}{}{}{}{}{}{}{}{}",
            words("molto espressivo", 1),
            dir("<wedge type=\"crescendo\"/>", 1, "below"),
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            dir("<wedge type=\"stop\"/>", 1, "below"),
            dir("<dynamics><f/></dynamics>", 1, "below"),
            dir("<words font-style=\"italic\">espress.</words>", 1, "below"),
            note("E5", 2, "quarter", 1, ""),
            note("F5", 2, "quarter", 1, ""),
            backup(8),
            note("C3", 8, "whole", 2, ""),
            String::new(),
        )
    };
    let loaded = part(
        "pin_expr_chain.musicxml",
        2,
        &[measure(false), measure(true), measure(false)],
    );
    let layout = engrave_loaded(&loaded).layout;
    let first = text_box(run(&layout, "molto espressivo"));
    let before = before_bar(&layout, first);
    near(
        first[0],
        before.1 + 0.25,
        "the first words moved right of the barline",
    );
    let forte = only(&layout, "dynamicForte");
    let expr = text_box(run(&layout, "espress."));
    assert!(
        (expr[0] - (forte[2] + 0.2)).abs() < 0.05,
        "after its own forte, 0.2: {expr:?} {forte:?}"
    );
}

/// Words wider than their measure at its first note stand right of the
/// barline before them, which wins (Gould: an initial dynamic to the
/// barline's right, p. 103; MuseScore checks the barline after, then the one
/// before).
#[test]
fn words_wider_than_their_measure_stand_right_of_the_barline_before() {
    let measure = |first: &str| {
        format!(
            "{}{}{}{}",
            words(first, 1),
            note("C5", 8, "whole", 1, ""),
            backup(8),
            note("C3", 8, "whole", 2, ""),
        )
    };
    let wide = "pianissimo e dolcissimo sempre molto legato";
    let grand = part(
        "pin_too_wide.musicxml",
        2,
        &[measure(""), measure(wide), measure(""), measure("")],
    );
    let layout = engrave_loaded(&grand).layout;
    let first = text_box(run(&layout, wide));
    let bars = barlines(&layout, first[3]);
    let before = before_bar(&layout, first);
    let after = bars
        .iter()
        .filter(|b| b.0 > before.1 + 0.01)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .copied()
        .expect("the barline after");
    assert!(
        first[2] - first[0] > after.0 - before.1 - 0.5,
        "the words wider than their measure's room: {first:?} {before:?} {after:?}"
    );
    near(first[0], before.1 + 0.25, "right of the barline before");
}

/// Two parts a bracket joins, their barlines through both, each of
/// `measures` given as the upper part's content and the lower's.
fn joined(name: &str, measures: &[(String, String)]) -> Loaded {
    let attributes = "<attributes><divisions>2</divisions><time><beats>4</beats>\
                      <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line>\
                      </clef></attributes>";
    let part_of = |pick: &dyn Fn(&(String, String)) -> String| -> String {
        measures
            .iter()
            .enumerate()
            .map(|(m, content)| {
                format!(
                    "<measure number=\"{}\">{}{}</measure>",
                    m + 1,
                    if m == 0 { attributes } else { "" },
                    pick(content)
                )
            })
            .collect()
    };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><part-group type=\"start\" number=\"1\">\
         <group-symbol>bracket</group-symbol><group-barline>yes</group-barline></part-group>\
         <score-part id=\"P1\"><part-name>A</part-name></score-part><score-part id=\"P2\">\
         <part-name>B</part-name></score-part><part-group type=\"stop\" number=\"1\"/>\
         </part-list><part id=\"P1\">{}</part><part id=\"P2\">{}</part></score-partwise>",
        part_of(&|m| m.0.clone()),
        part_of(&|m| m.1.clone()),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert!(loaded.fidelity.passed(), "{:#?}", loaded.fidelity.failures);
    loaded
}

/// A dynamic at a system's first note is judged by its own system's
/// barlines alone (X4d.3): words under the upper of two joined parts at the
/// first note of each of two systems stand as far from their notes in the
/// second as in the first, where no barline stands before them. The barline
/// ending the system before, judged with them, had moved them off it.
#[test]
fn a_dynamic_at_a_systems_first_note_is_judged_by_its_systems_barlines() {
    let upper = |words: bool, break_: bool| {
        format!(
            "{}{}{}{}{}{}",
            if break_ {
                "<print new-system=\"yes\"/>"
            } else {
                ""
            },
            if words {
                dir(
                    "<dynamics><other-dynamics>molto espressivo</other-dynamics></dynamics>",
                    1,
                    "below",
                )
            } else {
                String::new()
            },
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
            note("F5", 2, "quarter", 1, ""),
        )
    };
    let lower = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration>\
                 <voice>1</voice><type>whole</type></note>"
        .to_string();
    let loaded = joined(
        "systems_first_note.musicxml",
        &[
            (upper(true, false), lower.clone()),
            (upper(false, false), lower.clone()),
            (upper(true, true), lower.clone()),
            (upper(false, false), lower),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let mut found: Vec<&ResolvedText> = layout
        .texts
        .iter()
        .filter(|t| t.text == "molto espressivo")
        .collect();
    found.sort_by(|a, b| b.origin.y.0.total_cmp(&a.origin.y.0));
    assert_eq!(found.len(), 2, "the words at each system's first note");
    let offset = |t: &ResolvedText| {
        let words = text_box(t);
        let head = layout
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "noteheadBlack")
            .map(glyph_box)
            .filter(|h| h[1] > words[3] && h[1] - words[3] < 8.0)
            .min_by(|a, b| a[0].total_cmp(&b[0]))
            .expect("the first note over the words");
        words[0] - head[0]
    };
    near(
        offset(found[1]),
        offset(found[0]),
        "as far from their note in both systems",
    );
}

/// Expression text after a dynamic between a grand staff's staves stands on
/// the dynamic's baseline once its row is centred between them (X4d.6): a
/// p and "espress." after it under the upper staff, the p's row centred as
/// MuseScore centres a grand staff's dynamics
/// (`dynamicsHairpinsAutoCenterOnGrandStaff`), the words with it.
#[test]
fn expression_text_between_a_grand_staffs_staves_stays_on_its_dynamics_baseline() {
    let measure = |marked: bool| {
        format!(
            "{}{}{}{}{}{}{}",
            if marked {
                dir("<dynamics><p/></dynamics>", 1, "below")
            } else {
                String::new()
            },
            if marked {
                dir("<words font-style=\"italic\">espress.</words>", 1, "below")
            } else {
                String::new()
            },
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
            note("F5", 2, "quarter", 1, ""),
            backup(8) + &note("C3", 8, "whole", 2, ""),
        )
    };
    let grand = part(
        "grand_expression.musicxml",
        2,
        &[measure(false), measure(true), measure(false)],
    );
    let layout = engrave_loaded(&grand).layout;
    let p = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "dynamicPiano")
        .expect("the p");
    let words = run(&layout, "espress.");
    near(words.origin.y.0, p.position.y.0, "on the p's baseline");
}

/// A member the rule for marks at a measure's start has set stands where it
/// set it: the barline rule does not move it off its barline again (X4d.3).
/// No dynamic stands at its measure's start today, so the state is built by
/// hand: words under the upper of two joined parts at the second measure's
/// first note, given a rehearsal mark's place, centred on the barline's
/// right edge, stay so.
#[test]
fn a_member_set_at_its_measures_start_is_not_moved_off_its_barline_again() {
    use epiphany_layout_ir::{
        is_barline_glyph, to_constrained, to_logical, ConstraintSolver, MeasureMark, MeasureStart,
        SolverConfig,
    };
    let upper = |words: bool| {
        format!(
            "{}{}{}{}{}",
            if words {
                dir(
                    "<dynamics><other-dynamics>molto espressivo</other-dynamics></dynamics>",
                    1,
                    "below",
                )
            } else {
                String::new()
            },
            note("C5", 2, "quarter", 1, ""),
            note("D5", 2, "quarter", 1, ""),
            note("E5", 2, "quarter", 1, ""),
            note("F5", 2, "quarter", 1, ""),
        )
    };
    let lower = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration>\
                 <voice>1</voice><type>whole</type></note>"
        .to_string();
    let loaded = joined(
        "measure_start_kept.musicxml",
        &[
            (upper(false), lower.clone()),
            (upper(true), lower.clone()),
            (upper(false), lower),
        ],
    );
    let score = &loaded.reduced.score;
    let mut constrained = to_constrained(&to_logical(score));
    let words = constrained
        .texts
        .iter()
        .find(|t| t.text.text == "molto espressivo")
        .expect("the words")
        .clone();
    let barline = constrained
        .glyphs
        .iter()
        .filter(|g| {
            is_barline_glyph(g.glyph.as_str())
                && g.vertical_band == words.vertical_band
                && g.baseline.x.0 < words.text.origin.x.0
        })
        .max_by(|a, b| a.baseline.x.0.total_cmp(&b.baseline.x.0))
        .expect("the barline before")
        .horizontal_slot;
    let member = constrained
        .family
        .iter_mut()
        .find(|m| m.texts.contains(&words.id()))
        .expect("the words' member");
    let placement = member.placement.as_mut().expect("placed");
    placement.across = Some(MeasureStart {
        barline: Some(barline),
        kind: MeasureMark::Rehearsal,
    });
    let layout = epiphany_engrave::Engraver::with_geometry(epiphany_cli::page_of(score))
        .solve(&constrained, &SolverConfig::default())
        .layout;
    let drawn = text_box(run(&layout, "molto espressivo"));
    let centre = (drawn[0] + drawn[2]) * 0.5;
    // The barline whose right edge is the nearest left of half a space past
    // the words' centre.
    let before = before_bar(&layout, [centre + 0.5, drawn[1], centre + 0.5, drawn[3]]);
    near(centre, before.1, "centred on the barline's right edge");
}
