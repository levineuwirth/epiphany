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
