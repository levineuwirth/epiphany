//! A rest filling its measure and what stands on it: where it is centred, what
//! goes with it across and what stays at its column.

use std::path::Path;

use epiphany_cli::load;
use epiphany_engrave::{Engraver, PageGeometry};
use epiphany_layout_ir::{
    to_constrained, to_logical, ConstraintSolver, ResolvedLayoutIR, SolverConfig,
};

fn engrave(name: &str, xml: &str) -> ResolvedLayoutIR {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    Engraver::with_geometry(PageGeometry::default())
        .solve(
            &to_constrained(&to_logical(&loaded.reduced.score)),
            &SolverConfig::default(),
        )
        .layout
}

const HEAD: &str = "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>";

const WHOLE: &str = "<note><pitch><step>C</step><octave>5</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type></note>";

fn single(measures: &str) -> String {
    format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list><part id=\"P1\">{measures}</part></score-partwise>"
    )
}

/// Three measures, the middle one's content given, whole notes around it.
fn around(middle: &str) -> String {
    single(&format!(
        "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{middle}</measure><measure number=\"3\">{WHOLE}</measure>"
    ))
}

/// The ink of the first glyph whose name starts so: left, right, bottom, top.
fn ink(layout: &ResolvedLayoutIR, starts: &str) -> (f32, f32, f32, f32) {
    let g = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with(starts))
        .unwrap_or_else(|| panic!("a glyph named {starts}"));
    let (x, y, b) = (g.position.x.0, g.position.y.0, g.bounding_box);
    (x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0)
}

fn centre(ink: (f32, f32, f32, f32)) -> f32 {
    (ink.0 + ink.1) * 0.5
}

/// The middle of the second measure of three, between the ink of its barlines.
fn second_bar_middle(layout: &ResolvedLayoutIR) -> f32 {
    let mut bars: Vec<(f32, f32)> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "barlineSingle")
        .map(|g| {
            (
                g.position.x.0 + g.bounding_box.left.0,
                g.position.x.0 + g.bounding_box.right.0,
            )
        })
        .collect();
    bars.sort_by(|a, b| a.0.total_cmp(&b.0));
    (bars[0].1 + bars[1].0) * 0.5
}

/// A fermata on a rest filling its measure centres on the rest where it is
/// drawn (Gould, p. 188; MuseScore's `layoutFermata`; LilyPond's `R1\fermata`).
#[test]
fn a_fermata_stands_over_its_measure_rest() {
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><fermata type=\"upright\"/></notations></note>";
    let layout = engrave("fermata.musicxml", &around(rest));
    let (r, f) = (ink(&layout, "restWhole"), ink(&layout, "fermata"));
    assert!(
        (centre(r) - second_bar_middle(&layout)).abs() < 0.05,
        "the rest {r:?} at the bar's middle"
    );
    assert!(
        (centre(f) - centre(r)).abs() < 0.05,
        "the fermata {f:?} over its rest {r:?}"
    );
}

/// A lower voice's inverted fermata goes with its rest, under it.
#[test]
fn a_lower_voices_inverted_fermata_stands_under_its_measure_rest() {
    let middle = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup><note><rest measure=\"yes\"/><duration>16</duration><voice>2</voice><notations><fermata type=\"inverted\"/></notations></note>";
    let layout = engrave("lower_fermata.musicxml", &around(middle));
    let f = ink(&layout, "fermataBelow");
    let lower = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
        .min_by(|a, b| a.position.y.0.total_cmp(&b.position.y.0))
        .expect("two rests");
    let r = (
        lower.position.x.0 + lower.bounding_box.left.0,
        lower.position.x.0 + lower.bounding_box.right.0,
    );
    assert!(
        (centre(f) - (r.0 + r.1) * 0.5).abs() < 0.05,
        "the fermata {f:?} under its rest {r:?}"
    );
    assert!(f.3 < lower.position.y.0, "it stands below its rest");
}

/// A fermata on a hidden measure rest stands where the rest would: MuseScore
/// centres a hidden rest as any other.
#[test]
fn a_fermata_on_a_hidden_measure_rest_stands_at_the_bars_middle() {
    let rest = "<note print-object=\"no\"><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><fermata type=\"upright\"/></notations></note>";
    let layout = engrave("hidden_fermata.musicxml", &around(rest));
    assert!(
        layout
            .glyphs
            .iter()
            .all(|g| !g.glyph.as_str().starts_with("rest")),
        "the rest draws nothing"
    );
    let f = ink(&layout, "fermata");
    assert!(
        (centre(f) - second_bar_middle(&layout)).abs() < 0.05,
        "the fermata {f:?} at the bar's middle {}",
        second_bar_middle(&layout)
    );
}

/// The first and last control points of a layout's only slur: x then y.
fn slur_ends(layout: &ResolvedLayoutIR) -> (f32, f32) {
    let slurs: Vec<_> = layout
        .curves
        .iter()
        .filter(|c| matches!(c.provenance.source, epiphany_core::TypedObjectId::Slur(_)))
        .collect();
    assert_eq!(slurs.len(), 1, "one slur");
    (slurs[0].p0.x.0, slurs[0].p3.x.0)
}

/// A slur's end on a measure rest reads the rest's own place, as MuseScore's
/// `slurPos` does: it ends at the rest's centre.
#[test]
fn a_slur_ending_on_a_measure_rest_ends_at_the_rest() {
    let start = "<note><pitch><step>C</step><octave>5</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type><notations><slur type=\"start\" number=\"1\"/></notations></note>";
    let stop = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><slur type=\"stop\" number=\"1\"/></notations></note>";
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{stop}</measure><measure number=\"3\">{WHOLE}</measure>"
    ));
    let layout = engrave("slur_end.musicxml", &xml);
    let r = ink(&layout, "restWhole");
    let (_, end) = slur_ends(&layout);
    assert!(
        (end - centre(r)).abs() < 0.05,
        "the slur's end {end} at the rest {r:?}"
    );
}

/// And a slur's start on one leaves from it.
#[test]
fn a_slur_starting_on_a_measure_rest_starts_at_the_rest() {
    let start = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><slur type=\"start\" number=\"1\"/></notations></note>";
    let stop = "<note><pitch><step>C</step><octave>5</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type><notations><slur type=\"stop\" number=\"1\"/></notations></note>";
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{start}</measure><measure number=\"3\">{stop}</measure>"
    ));
    let layout = engrave("slur_start.musicxml", &xml);
    let r = ink(&layout, "restWhole");
    let (start_x, _) = slur_ends(&layout);
    assert!(
        (start_x - centre(r)).abs() < 0.05,
        "the slur's start {start_x} at the rest {r:?}"
    );
}

/// A slur of another voice ending in the rest's column stays with its note.
#[test]
fn a_slur_of_another_voice_stays_with_its_note() {
    let start = "<note><pitch><step>C</step><octave>5</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type><stem>down</stem><notations><slur type=\"start\" number=\"1\"/></notations></note>";
    let stop = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup><note><pitch><step>D</step><octave>4</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type><stem>down</stem><notations><slur type=\"stop\" number=\"1\"/></notations></note>";
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{stop}</measure><measure number=\"3\">{WHOLE}</measure>"
    ));
    let layout = engrave("slur_other.musicxml", &xml);
    let head = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadWhole")
        .min_by(|a, b| a.position.y.0.total_cmp(&b.position.y.0))
        .map(|g| g.position.x.0)
        .expect("the D, the lowest whole note");
    let (_, end) = slur_ends(&layout);
    assert!(
        (end - head).abs() < 2.5,
        "the slur's end {end} at its note {head}"
    );
    let r = ink(&layout, "restWhole");
    assert!(centre(r) > end + 3.0, "the rest {r:?} centred beyond it");
}

/// What is anchored to the time stays at the rest's column, as MuseScore
/// anchors a dynamic and words to the segment: only the rest moves.
#[test]
fn a_dynamic_and_words_at_a_measure_rest_stay_at_its_column() {
    let middle = "<direction placement=\"below\"><direction-type><dynamics><p/></dynamics></direction-type></direction><direction placement=\"above\"><direction-type><words>dolce</words></direction-type></direction><note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note>";
    let layout = engrave("dynamic_rest.musicxml", &around(middle));
    let r = ink(&layout, "restWhole");
    let p = ink(&layout, "dynamicPiano");
    assert!(
        p.0 < centre(r) - 3.0,
        "the p {p:?} at the column, the rest {r:?} centred"
    );
    let words = layout
        .texts
        .iter()
        .find(|t| t.text.as_str() == "dolce")
        .expect("the words");
    let x = words.origin.x.0 + words.bounds.left.0;
    assert!(
        x < centre(r) - 3.0,
        "the words at {x}, the rest {r:?} centred"
    );
}

/// A trill line on a measure rest starts at the column, as MuseScore's
/// `SLine::linePos` reads the segment's x.
#[test]
fn a_trill_line_on_a_measure_rest_starts_at_its_column() {
    let middle = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><ornaments><trill-mark/><wavy-line type=\"start\" number=\"1\"/></ornaments></notations></note>";
    let next = "<note><pitch><step>C</step><octave>5</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type><notations><ornaments><wavy-line type=\"stop\" number=\"1\"/></ornaments></notations></note>";
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{middle}</measure><measure number=\"3\">{next}</measure>"
    ));
    let layout = engrave("trill_rest.musicxml", &xml);
    let r = ink(&layout, "restWhole");
    let t = ink(&layout, "ornamentTrill");
    assert!(
        t.1 < centre(r) - 2.0,
        "the trill sign {t:?} at the column, the rest {r:?} centred"
    );
}

/// A syllable on a measure rest goes with it, as it stands on MuseScore's page.
#[test]
fn a_syllable_on_a_measure_rest_stands_under_it() {
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><lyric number=\"1\"><syllabic>single</syllabic><text>two</text></lyric></note>";
    let layout = engrave("syllable.musicxml", &around(rest));
    let r = ink(&layout, "restWhole");
    let run = layout
        .texts
        .iter()
        .find(|t| t.text.as_str() == "two")
        .expect("the syllable");
    let x = run.origin.x.0 + (run.bounds.left.0 + run.bounds.right.0) * 0.5;
    assert!(
        (x - centre(r)).abs() < 0.3,
        "the syllable at {x}, its rest {r:?}"
    );
}

/// An ottava whose last ink is a measure rest ends a space past it where it
/// is drawn, as `Ottava::linePos` reads the segment's shape.
#[test]
fn an_ottava_ending_on_a_measure_rest_ends_past_the_rest() {
    let start = "<direction placement=\"above\"><direction-type><octave-shift type=\"down\" size=\"8\" number=\"1\"/></direction-type></direction>";
    let stop = "<direction><direction-type><octave-shift type=\"stop\" size=\"8\" number=\"1\"/></direction-type></direction>";
    let high = "<note><pitch><step>C</step><octave>6</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type></note>";
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note>";
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}{high}</measure><measure number=\"2\">{rest}{stop}</measure><measure number=\"3\">{WHOLE}</measure>"
    ));
    let layout = engrave("ottava_rest.musicxml", &xml);
    let r = ink(&layout, "restWhole");
    let hook = layout
        .strokes
        .iter()
        .filter(|s| {
            s.from.x == s.to.x
                && s.from.y != s.to.y
                && matches!(
                    s.provenance.source,
                    epiphany_core::TypedObjectId::Spanner(_)
                )
        })
        .map(|s| s.from.x.0)
        .reduce(f32::max)
        .expect("the ottava's hook");
    assert!(
        (hook - (r.1 + 1.0)).abs() < 0.2,
        "the hook at {hook}, a space past the rest {r:?}"
    );
}
