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
    // Its dashes run from its first note to the hook.
    let dashes: Vec<(f32, f32)> = layout
        .strokes
        .iter()
        .filter(|s| {
            s.from.y == s.to.y
                && s.from.x != s.to.x
                && matches!(
                    s.provenance.source,
                    epiphany_core::TypedObjectId::Spanner(_)
                )
        })
        .map(|s| (s.from.x.0, s.to.x.0))
        .collect();
    let (start, end) = dashes
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), (from, to)| {
            (a.min(*from), b.max(*to))
        });
    let c6 = ink(&layout, "noteheadWhole");
    assert!(
        start < c6.1 + 1.0 && (end - hook).abs() < 0.2,
        "the line from {start} to {end}, from its first note {c6:?} to the hook {hook}"
    );
}

/// A fermata clears its rest where it stands: over a rest raised off the staff
/// at the bar's middle, not over the column the rest left.
#[test]
fn a_fermata_clears_its_rest_where_it_stands() {
    let high = "<note><pitch><step>A</step><octave>5</octave></pitch><duration>8</duration><voice>2</voice><type>half</type></note>";
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><fermata type=\"upright\"/></notations></note>";
    let middle = format!("{rest}<backup><duration>16</duration></backup>{high}{high}");
    let layout = engrave("fermata_clears.musicxml", &around(&middle));
    let (r, f) = (ink(&layout, "rest"), ink(&layout, "fermata"));
    assert!(r.2 > top_line(&layout), "the rest above the staff: {r:?}");
    assert!(
        f.2 >= r.3 - 1e-3,
        "the fermata {f:?} over the rest {r:?} as it stands"
    );
    assert!(
        (centre(f) - centre(r)).abs() < 0.05,
        "the fermata {f:?} over its rest {r:?}"
    );
}

/// A whole-bar rest's space opens after its bar's key signature and time
/// signature: the room Gould centres it in (p. 160), in a bar each opens.
#[test]
fn a_measure_rest_centres_after_a_key_change_opening_its_bar() {
    let key = "<attributes><key><fifths>4</fifths></key></attributes>";
    let rest = measure_rest(16);
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{key}{rest}</measure><measure number=\"3\">{WHOLE}</measure>"
    ));
    let layout = engrave("rest_key.musicxml", &xml);
    let r = ink(&layout, "restWhole");
    let sig = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "accidentalSharp")
        .map(|g| g.position.x.0 + g.bounding_box.right.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let closing = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "barlineSingle")
        .map(|g| g.position.x.0 + g.bounding_box.left.0)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (centre(r) - (sig + closing) * 0.5).abs() < 0.05,
        "the rest {r:?} between the signature {sig} and the barline {closing}"
    );
}

/// As it does after a time change.
#[test]
fn a_measure_rest_centres_after_a_time_change_opening_its_bar() {
    let time = "<attributes><time><beats>3</beats><beat-type>4</beat-type></time></attributes>";
    let rest = measure_rest(12);
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{time}{rest}</measure><measure number=\"3\"><note><pitch><step>C</step><octave>5</octave></pitch><duration>12</duration><voice>1</voice></note></measure>"
    ));
    let layout = engrave("rest_time.musicxml", &xml);
    let r = ink(&layout, "restWhole");
    let sig = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("timeSig"))
        .map(|g| g.position.x.0 + g.bounding_box.right.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let closing = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "barlineSingle")
        .map(|g| g.position.x.0 + g.bounding_box.left.0)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (centre(r) - (sig + closing) * 0.5).abs() < 0.05,
        "the rest {r:?} between the signature {sig} and the barline {closing}"
    );
}

/// The staff's top line, the highest of the first staff's lines.
fn top_line(layout: &ResolvedLayoutIR) -> f32 {
    layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, epiphany_core::TypedObjectId::Staff(_))
                && s.from.y == s.to.y
        })
        .map(|s| s.from.y.0)
        .reduce(f32::max)
        .expect("a staff line")
}

/// The name of the rest a layout draws, and the height of its origin.
fn rest_at(layout: &ResolvedLayoutIR) -> (&str, f32) {
    let g = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("rest"))
        .expect("a rest");
    (g.glyph.as_str(), g.position.y.0)
}

/// A rest filling its measure clears another voice where it stands, its
/// accidental included (Gould, pp. 36 and 41): MuseScore's rule leaves it on
/// the sharp, which it takes out of what it reads.
#[test]
fn a_centred_rest_clears_another_voices_accidental_where_it_stands() {
    let low = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><stem>down</stem></note>";
    let high = "<note><pitch><step>F</step><alter>1</alter><octave>5</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><accidental>sharp</accidental><stem>down</stem></note>";
    let middle = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{low}{high}"
    );
    let layout = engrave("rest_on_sharp.musicxml", &around(&middle));
    let r = ink(&layout, "rest");
    for g in &layout.glyphs {
        let name = g.glyph.as_str();
        if name.starts_with("notehead") || name.starts_with("accidental") {
            let (x, y, b) = (g.position.x.0, g.position.y.0, g.bounding_box);
            let across = r.1.min(x + b.right.0) - r.0.max(x + b.left.0) > -4.0;
            if across {
                assert!(
                    r.2 - (y + b.top.0) >= 0.75 - 1e-3,
                    "the rest {r:?} 0.75 over {name} at ({x}, {y})"
                );
            }
        }
    }
}

/// A rest a first column raised returns to its line where nothing stands near
/// it at the bar's middle.
#[test]
fn a_rest_away_from_a_high_note_stands_on_its_line() {
    let high = "<note><pitch><step>A</step><octave>5</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type></note>";
    let middle = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{high}"
    );
    let layout = engrave("rest_returned.musicxml", &around(&middle));
    let (name, y) = rest_at(&layout);
    assert_eq!(name, "restWhole", "no ledger line");
    assert!(
        (y - top_line(&layout)).abs() < 0.05,
        "an upper voice's rest hanging from the top line: {y}, {}",
        top_line(&layout)
    );
}

/// A rest beside a voice reads the glyph it will draw: a half rest raised off
/// the staff draws a ledger line, whose overhang reaches the accidental the
/// plain glyph clears (Gould, pp. 37 and 41).
#[test]
fn a_half_rests_ledger_stands_clear_of_an_accidental() {
    let rest = "<note><rest/><duration>8</duration><voice>1</voice><type>half</type></note><note><pitch><step>C</step><octave>6</octave></pitch><duration>8</duration><voice>1</voice><type>half</type></note>";
    let sharp = "<note><pitch><step>F</step><alter>1</alter><octave>5</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><accidental>sharp</accidental><stem>down</stem></note><note><pitch><step>G</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><stem>down</stem></note>";
    let middle = format!("{rest}<backup><duration>16</duration></backup>{sharp}");
    let layout = engrave("half_on_sharp.musicxml", &around(&middle));
    let r = ink(&layout, "restHalfLegerLine");
    let a = ink(&layout, "accidentalSharp");
    let across = r.1.min(a.1) - r.0.max(a.0);
    assert!(across > 0.0, "the ledger {r:?} over the sharp {a:?} across");
    assert!(
        r.2 - a.3 >= 0.25 - 1e-3,
        "the ledgered rest {r:?} clear of the sharp {a:?}"
    );
}

/// A measure rest drawn ledgered carries its ledgered glyph's box to the page,
/// which what reads the page reads: bars so narrow that each rest stands by its
/// bar's high note.
#[test]
fn a_rest_drawn_ledgered_keeps_the_box_it_read() {
    let high = "<note><pitch><step>C</step><octave>6</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type></note>";
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note>";
    let bar = |n: u32| {
        format!(
            "<measure number=\"{n}\">{}{rest}<backup><duration>16</duration></backup>{high}</measure>",
            if n == 1 { HEAD } else { "" }
        )
    };
    let xml = single(&(1..=12).map(bar).collect::<String>());
    let layout = engrave("rest_ledgered.musicxml", &xml);
    for g in layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
    {
        let wide = g.glyph.as_str() == "restWholeLegerLine";
        let box_wide = g.bounding_box.left.0 < -0.1;
        assert_eq!(
            wide,
            box_wide,
            "the box of the glyph drawn: {}",
            g.glyph.as_str()
        );
    }
}

/// Three staves of one part: a rest filling its measure on the middle one,
/// its voice its own, with visiting notes from the staves above and below
/// at the bar's middle.
fn between_visitors(upper: &str, lower: &str) -> String {
    let clefs = "<staves>3</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>G</sign><line>2</line></clef><clef number=\"3\"><sign>F</sign><line>4</line></clef>";
    let attrs = format!(
        "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time>{clefs}</attributes>"
    );
    let open = |staff: u32, voice: u32, step: &str, octave: u32| {
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>16</duration><voice>{voice}</voice><type>whole</type><staff>{staff}</staff></note><backup><duration>16</duration></backup>"
        )
    };
    let first = format!(
        "<measure number=\"1\">{attrs}{}{}{}</measure>",
        open(1, 1, "C", 5),
        open(2, 5, "B", 4),
        open(3, 9, "C", 3)
    );
    let quarter = |staff: u32, voice: u32, pitch: &str| {
        let (step, octave) = pitch.split_at(1);
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>4</duration><voice>{voice}</voice><type>quarter</type><staff>{staff}</staff></note>"
        )
    };
    let second = format!(
        "<measure number=\"2\">{a}{b}{a}{b}<backup><duration>16</duration></backup><note><rest measure=\"yes\"/><duration>16</duration><voice>5</voice><staff>2</staff></note><backup><duration>16</duration></backup>{c}{d}{c}{d}</measure>",
        a = quarter(2, 1, upper),
        b = quarter(1, 1, "C5"),
        c = quarter(2, 9, lower),
        d = quarter(3, 9, "C3")
    );
    let third = format!(
        "<measure number=\"3\">{}{}{}</measure>",
        open(1, 1, "C", 5),
        open(2, 5, "B", 4),
        open(3, 9, "C", 3)
    );
    format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list><part id=\"P1\">{first}{second}{third}</part></score-partwise>"
    )
}

/// The height of the rest on the second of three staves, from that staff's
/// middle line.
fn middle_staff_rest(layout: &ResolvedLayoutIR) -> f32 {
    let g = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("rest"))
        .expect("the rest");
    let mut lines: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, epiphany_core::TypedObjectId::Staff(_))
                && s.from.y == s.to.y
        })
        .map(|s| s.from.y.0)
        .collect();
    lines.sort_by(|a, b| b.total_cmp(a));
    lines.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    g.position.y.0 - lines[7]
}

/// A rest between visitors from both sides takes the side whose first clear
/// place stands nearer its staff's middle line where it now stands (D109 Q8).
/// Each visitor's stem turns toward its home, up from the upper and down from
/// the lower, at the bar's middle, so a place past a stem is a place past that
/// stem's whole length. D5 and B4: under the B4's stem, -5 from the line, its
/// glyph's centre 5.25 off; over the D5's it would be 6, 5.75 off. B4 and G4:
/// over the B4's stem, 5, 4.75 off; under the G4's it would be -6, 6.25 off.
/// F5 and D4 leave it clear between them, a space under its line.
#[test]
fn a_rest_between_visitors_at_its_centre_takes_the_nearer_side() {
    let under = middle_staff_rest(&engrave(
        "between_under.musicxml",
        &between_visitors("D5", "B4"),
    ));
    assert!((under + 5.0).abs() < 0.01, "under the B4: {under}");
    let over = middle_staff_rest(&engrave(
        "between_over.musicxml",
        &between_visitors("B4", "G4"),
    ));
    assert!((over - 5.0).abs() < 0.01, "over the B4: {over}");
    let between = middle_staff_rest(&engrave(
        "between_clear.musicxml",
        &between_visitors("F5", "D4"),
    ));
    assert!(between.abs() < 0.01, "between the two: {between}");
}

/// One measure's rest in a meter, between two whole-note bars of that meter.
fn in_meter(beats: u32, beat_type: u32, middle: &str) -> String {
    let attrs = format!(
        "<attributes><divisions>4</divisions><time><beats>{beats}</beats><beat-type>{beat_type}</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>"
    );
    let filling = 16 * beats / beat_type;
    let note = format!(
        "<note><pitch><step>C</step><octave>5</octave></pitch><duration>{filling}</duration><voice>1</voice></note>"
    );
    single(&format!(
        "<measure number=\"1\">{attrs}{note}</measure><measure number=\"2\">{middle}</measure><measure number=\"3\">{note}</measure>"
    ))
}

fn measure_rest(duration: u32) -> String {
    format!("<note><rest measure=\"yes\"/><duration>{duration}</duration><voice>1</voice></note>")
}

/// A whole-bar rest in 2/1 or more is a breve rest (Gould, p. 160; MuseScore's
/// `Rest::getSymbol`; LilyPond), filling the space above the middle line and
/// centred as any whole-bar rest, its box the breve's.
#[test]
fn a_whole_bar_rest_in_four_two_is_a_breve_rest() {
    let layout = engrave("breve_4_2.musicxml", &in_meter(4, 2, &measure_rest(32)));
    let (name, y) = rest_at(&layout);
    assert_eq!(name, "restDoubleWhole");
    assert!(
        (y - (top_line(&layout) - 2.0)).abs() < 0.05,
        "on the middle line: {y}, the top line {}",
        top_line(&layout)
    );
    let r = ink(&layout, "restDoubleWhole");
    assert!((r.1 - r.0 - 0.5).abs() < 0.01, "its box the breve's: {r:?}");
    assert!(
        (centre(r) - second_bar_middle(&layout)).abs() < 0.05,
        "the rest {r:?} at the bar's middle"
    );
}

/// In 3/2, short of 2/1, it stays a semibreve rest.
#[test]
fn a_whole_bar_rest_in_three_two_is_a_whole_rest() {
    let layout = engrave("whole_3_2.musicxml", &in_meter(3, 2, &measure_rest(24)));
    assert_eq!(rest_at(&layout).0, "restWhole");
}

/// In 2/1 exactly it is a breve rest.
#[test]
fn a_whole_bar_rest_in_two_one_is_a_breve_rest() {
    let layout = engrave("breve_2_1.musicxml", &in_meter(2, 1, &measure_rest(32)));
    assert_eq!(rest_at(&layout).0, "restDoubleWhole");
}

/// Outside its staff it draws its ledgered glyph, through the clearing of a
/// rest filling its measure: over the other part's high note, at the bar's
/// middle.
#[test]
fn a_breve_rest_outside_its_staff_is_ledgered() {
    let high = "<note><pitch><step>A</step><octave>5</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type></note>";
    let middle = format!(
        "{}<backup><duration>32</duration></backup>{high}{high}",
        measure_rest(32)
    );
    let layout = engrave("breve_ledgered.musicxml", &in_meter(4, 2, &middle));
    let (name, y) = rest_at(&layout);
    assert_eq!(name, "restDoubleWholeLegerLine");
    assert!(y > top_line(&layout), "above the staff: {y}");
    let r = ink(&layout, "restDoubleWholeLegerLine");
    let a = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadWhole")
        .map(|g| g.position.y.0 + g.bounding_box.top.0)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        r.2 - a >= 0.75 - 1e-3,
        "0.75 over the note's top {a}: {r:?}"
    );
    assert!(
        (centre(r) - second_bar_middle(&layout)).abs() < 0.05,
        "its box centred, the ledger line's overhang counted: {r:?}"
    );
}

/// On a staff of one line it hangs from the line, as MuseScore's page and
/// LilyPond's stand it.
#[test]
fn a_breve_rest_on_a_staff_of_one_line_hangs_from_it() {
    let perc = "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>2</beat-type></time><clef><sign>percussion</sign></clef><staff-details><staff-lines>1</staff-lines></staff-details></attributes>";
    let xml = single(&format!(
        "<measure number=\"1\">{perc}{}</measure>",
        measure_rest(32)
    ));
    let layout = engrave("breve_one_line.musicxml", &xml);
    let (name, y) = rest_at(&layout);
    assert_eq!(name, "restDoubleWholeLegerLine");
    assert!(
        (y - (top_line(&layout) - 1.0)).abs() < 0.05,
        "a space under its one line: {y}"
    );
}

/// A fermata on one goes with it, as on any measure rest.
#[test]
fn a_fermata_on_a_breve_rest_stands_over_it() {
    let rest = "<note><rest measure=\"yes\"/><duration>32</duration><voice>1</voice><notations><fermata type=\"upright\"/></notations></note>";
    let layout = engrave("breve_fermata.musicxml", &in_meter(4, 2, rest));
    let (r, f) = (ink(&layout, "restDoubleWhole"), ink(&layout, "fermata"));
    assert!(
        (centre(f) - centre(r)).abs() < 0.05,
        "the fermata {f:?} over its rest {r:?}"
    );
}

/// A caesura closing the bar does not narrow a measure rest's free space: only
/// a clef, a key or time signature or a barline does (Gould, p. 160; MuseScore's
/// `getMeasureStartEndPos`).
#[test]
fn a_caesura_leaves_a_measure_rest_at_the_bars_middle() {
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><articulations><caesura/></articulations></notations></note>";
    let layout = engrave("rest_caesura.musicxml", &around(rest));
    assert!(
        layout
            .glyphs
            .iter()
            .any(|g| g.glyph.as_str().starts_with("caesura")),
        "the caesura is drawn"
    );
    let r = ink(&layout, "restWhole");
    assert!(
        (centre(r) - second_bar_middle(&layout)).abs() < 0.02,
        "the rest {r:?} at the bar's middle {}",
        second_bar_middle(&layout)
    );
}

/// Nor does a breath mark.
#[test]
fn a_breath_mark_inside_the_bar_leaves_a_measure_rest_at_its_middle() {
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><notations><articulations><breath-mark/></articulations></notations></note>";
    let layout = engrave("rest_breath.musicxml", &around(rest));
    assert!(
        layout
            .glyphs
            .iter()
            .any(|g| g.glyph.as_str().starts_with("breathMark")),
        "the breath mark is drawn"
    );
    let r = ink(&layout, "restWhole");
    assert!(
        (centre(r) - second_bar_middle(&layout)).abs() < 0.02,
        "the rest {r:?} at the bar's middle {}",
        second_bar_middle(&layout)
    );
}

/// A clef closing the bar does: the rest stands left of the bar's middle.
#[test]
fn a_clef_closing_the_bar_bounds_a_measure_rest() {
    let rest = "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><attributes><clef><sign>F</sign><line>4</line></clef></attributes>";
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{rest}</measure><measure number=\"3\"><note><pitch><step>C</step><octave>3</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type></note></measure>"
    ));
    let layout = engrave("rest_clef.musicxml", &xml);
    let r = ink(&layout, "restWhole");
    let clef = ink(&layout, "fClef");
    let bars: Vec<f32> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "barlineSingle")
        .map(|g| g.position.x.0 + g.bounding_box.right.0)
        .collect();
    let opening = bars.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(
        clef.0 > opening,
        "the clef {clef:?} inside the bar from {opening}"
    );
    assert!(
        (centre(r) - (opening + clef.0) * 0.5).abs() < 0.05,
        "the rest {r:?} between the barline {opening} and the clef {clef:?}"
    );
}

/// An ottava whose last note stands in a measure rest's column, in another
/// voice, ends past that note: MuseScore's staff shape leaves out a full measure
/// rest beside other voices (`Segment::createShape`).
#[test]
fn an_ottava_ending_on_a_note_beside_a_measure_rest_stays_with_the_note() {
    let start = "<direction placement=\"above\"><direction-type><octave-shift type=\"down\" size=\"8\" number=\"1\"/></direction-type></direction>";
    let stop = "<direction><direction-type><octave-shift type=\"stop\" size=\"8\" number=\"1\"/></direction-type></direction>";
    let high = "<note><pitch><step>C</step><octave>6</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type></note>";
    let beside = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup><note><pitch><step>C</step><octave>6</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type><stem>down</stem></note>{stop}"
    );
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}{high}</measure><measure number=\"2\">{beside}</measure><measure number=\"3\">{WHOLE}</measure>"
    ));
    let layout = engrave("ottava_beside.musicxml", &xml);
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
        hook < centre(r) - 3.0,
        "the hook at {hook} by its note, the rest {r:?} centred"
    );
}

/// A voice's rest does not read what the family places after it: a fermata on
/// the other voice's note over the staff at the bar's middle leaves it on its
/// line, and the fermata stands clear of it.
#[test]
fn a_voices_rest_does_not_read_the_family_placed_after_it() {
    let low = "<note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><stem>down</stem></note>";
    let marked = "<note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><stem>down</stem><notations><fermata type=\"upright\"/></notations></note>";
    let middle = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{low}{marked}"
    );
    let layout = engrave("rest_family.musicxml", &around(&middle));
    let (name, y) = rest_at(&layout);
    assert_eq!(name, "restWhole");
    assert!(
        (y - top_line(&layout)).abs() < 0.05,
        "hanging from the top line: {y}, {}",
        top_line(&layout)
    );
    let (r, f) = (ink(&layout, "restWhole"), ink(&layout, "fermata"));
    assert!(
        r.1.min(f.1) - r.0.max(f.0) > -4.0,
        "the fermata {f:?} within the rest's reach {r:?}"
    );
    assert!(f.2 >= r.3, "the fermata {f:?} over the rest {r:?}");
}

/// Nor a hairpin's legs: a lower voice's rest over a hairpin under the staff at
/// the bar's middle stays at its voice's place.
#[test]
fn a_voices_rest_does_not_read_a_hairpin_placed_after_it() {
    let high = "<note><pitch><step>A</step><octave>4</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type><stem>up</stem></note>";
    let pair = "<direction placement=\"below\"><direction-type><wedge type=\"crescendo\"/></direction-type></direction>";
    let stop = "<direction><direction-type><wedge type=\"stop\"/></direction-type></direction>";
    let middle = format!(
        "{pair}{high}<backup><duration>16</duration></backup><note><rest measure=\"yes\"/><duration>16</duration><voice>2</voice></note>{stop}"
    );
    let layout = engrave("rest_hairpin.musicxml", &around(&middle));
    let (name, y) = rest_at(&layout);
    assert_eq!(name, "restWhole");
    assert!(
        (y - (top_line(&layout) - 2.0)).abs() < 0.05,
        "a lower voice's rest hanging from the middle line: {y}, the top line {}",
        top_line(&layout)
    );
}

/// A bar's width in a short meter, with a low note in the other voice: the
/// rest's reach holds what stands at each side of it.
fn narrow_bar(after: &str, closing: &str) -> String {
    let rest = "<note><rest measure=\"yes\"/><duration>2</duration><voice>1</voice></note>";
    let low = "<note><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><voice>2</voice><type>eighth</type><stem>down</stem></note>";
    let attrs = "<attributes><divisions>4</divisions><time><beats>1</beats><beat-type>8</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>";
    let eighth = "<note><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>eighth</type></note>";
    single(&format!(
        "<measure number=\"1\">{attrs}{eighth}</measure><measure number=\"2\">{rest}<backup><duration>2</duration></backup>{low}{closing}</measure><measure number=\"3\">{after}</measure>"
    ))
}

/// The ink of the bar it stands in alone: a high note opening the next bar is
/// within its reach and is not read.
#[test]
fn a_measure_rest_reads_no_ink_of_the_next_bar() {
    let high = "<note><pitch><step>A</step><octave>5</octave></pitch><duration>2</duration><voice>1</voice><type>eighth</type></note>";
    let layout = engrave("rest_next_bar.musicxml", &narrow_bar(high, ""));
    let (name, y) = rest_at(&layout);
    let r = ink(&layout, "restWhole");
    let a = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadBlack")
        .max_by(|p, q| p.position.y.0.total_cmp(&q.position.y.0))
        .expect("the A5");
    assert!(
        a.position.x.0 - r.1 < 4.0,
        "the A5 at {} within the rest's reach {r:?}",
        a.position.x.0
    );
    assert_eq!(name, "restWhole");
    assert!((y - top_line(&layout)).abs() < 0.05, "on its line: {y}");
}

/// Nor a clef closing its bar, which bounds its space and which it does not
/// stand in: within its reach, and not read.
#[test]
fn a_measure_rest_does_not_read_a_clef_closing_its_bar() {
    let clef = "<attributes><clef><sign>F</sign><line>4</line></clef></attributes>";
    let next = "<note><pitch><step>E</step><octave>3</octave></pitch><duration>2</duration><voice>1</voice><type>eighth</type></note>";
    let layout = engrave("rest_clef_read.musicxml", &narrow_bar(next, clef));
    let (name, y) = rest_at(&layout);
    let r = ink(&layout, "restWhole");
    let f = ink(&layout, "fClef");
    assert!(
        f.0 - r.1 < 4.0,
        "the clef {f:?} within the rest's reach {r:?}"
    );
    assert!(f.3 > r.2 - 0.75, "and tall enough to lift it: {f:?} {r:?}");
    assert_eq!(name, "restWhole");
    assert!((y - top_line(&layout)).abs() < 0.05, "on its line: {y}");
}

/// A rest alone reads no visitor at its first column once it stands at the bar's
/// middle, and stays on its line.
#[test]
fn a_rest_away_from_a_visitor_at_its_first_column_stands_on_its_line() {
    let clefs = "<staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>G</sign><line>2</line></clef>";
    let attrs = format!(
        "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time>{clefs}</attributes>"
    );
    let staff = |p: &str, o: u32, v: u32, st: u32, d: u32, t: &str| {
        format!(
            "<note><pitch><step>{p}</step><octave>{o}</octave></pitch><duration>{d}</duration><voice>{v}</voice><type>{t}</type><staff>{st}</staff></note>"
        )
    };
    let first = format!(
        "{}<backup><duration>16</duration></backup>{}",
        staff("C", 5, 1, 1, 16, "whole"),
        staff("C", 5, 5, 2, 16, "whole")
    );
    let second = format!(
        "{}{}<backup><duration>16</duration></backup><note><rest measure=\"yes\"/><duration>16</duration><voice>5</voice><staff>2</staff></note>",
        staff("D", 5, 1, 2, 2, "eighth"),
        staff("C", 5, 1, 1, 14, "half")
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">{attrs}{first}</measure><measure number=\"2\">{second}</measure><measure number=\"3\">{first}</measure></part></score-partwise>"
    );
    let layout = engrave("rest_visitor_first.musicxml", &xml);
    let mut lines: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, epiphany_core::TypedObjectId::Staff(_))
                && s.from.y == s.to.y
        })
        .map(|s| s.from.y.0)
        .collect();
    lines.sort_by(|a, b| b.total_cmp(a));
    lines.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    let (name, y) = rest_at(&layout);
    assert_eq!(name, "restWhole");
    assert!(
        (y - lines[6]).abs() < 0.05,
        "hanging from its staff's fourth line {}: {y}",
        lines[6]
    );
}

/// A visitor in the next bar within a rest's reach does not turn it: only the
/// stems of its own bar do. The C4's stem up leaves it under the C4, its first
/// clear place; a stem down read from the next bar would take it over.
#[test]
fn a_rest_alone_turns_from_the_stems_of_its_own_bar_alone() {
    let clefs = "<staves>3</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>G</sign><line>2</line></clef><clef number=\"3\"><sign>F</sign><line>4</line></clef>";
    let attrs = format!(
        "<attributes><divisions>4</divisions><time><beats>1</beats><beat-type>8</beat-type></time>{clefs}</attributes>"
    );
    let staff = |p: &str, o: u32, v: u32, st: u32| {
        format!(
            "<note><pitch><step>{p}</step><octave>{o}</octave></pitch><duration>2</duration><voice>{v}</voice><type>eighth</type><staff>{st}</staff></note><backup><duration>2</duration></backup>"
        )
    };
    let first = format!(
        "{}{}{}",
        staff("C", 5, 1, 1),
        staff("B", 4, 5, 2),
        staff("C", 3, 9, 3)
    );
    let second = format!(
        "{}<note><rest measure=\"yes\"/><duration>2</duration><voice>5</voice><staff>2</staff></note><backup><duration>2</duration></backup>{}",
        staff("C", 4, 1, 2),
        staff("C", 3, 9, 3)
    );
    let third = format!(
        "{}{}{}",
        staff("C", 5, 1, 1),
        staff("B", 4, 5, 2),
        staff("F", 5, 9, 2)
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">{attrs}{first}</measure><measure number=\"2\">{second}</measure><measure number=\"3\">{third}</measure></part></score-partwise>"
    );
    let layout = engrave("rest_own_bar_stems.musicxml", &xml);
    let rest = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("rest"))
        .expect("the rest");
    let visitors: Vec<(f32, f32)> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadBlack")
        .map(|g| (g.position.x.0, g.position.y.0))
        .collect();
    let right = rest.position.x.0 + rest.bounding_box.right.0;
    assert!(
        visitors.iter().any(|(x, _)| *x > right && *x - right < 3.5),
        "the next bar's notes {visitors:?} within the rest's reach from {right}"
    );
    assert!(
        rest.position.y.0
            < visitors
                .iter()
                .filter(|(x, _)| (*x - rest.position.x.0).abs() < 3.0)
                .map(|(_, y)| *y)
                .fold(f32::NEG_INFINITY, f32::max),
        "the rest {} under its bar's C4, {visitors:?}",
        rest.position.y.0
    );
}

/// A voice's rest on a staff a beam leaves does not read the stem reaching that
/// beam, whose end the gap between the staves sets later: it stays at its voice's
/// place by its own staff's other voice, the stem at the bar's middle.
#[test]
fn a_voices_rest_does_not_read_a_stem_reaching_a_beam_on_another_staff() {
    let clefs = "<staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign><line>4</line></clef>";
    let attrs = format!(
        "<attributes><divisions>2</divisions><time><beats>3</beats><beat-type>8</beat-type></time>{clefs}</attributes>"
    );
    let beamed = |p: &str, o: u32, st: u32, b: &str| {
        format!(
            "<note><pitch><step>{p}</step><octave>{o}</octave></pitch><duration>1</duration><voice>1</voice><type>eighth</type><beam number=\"1\">{b}</beam><staff>{st}</staff></note>"
        )
    };
    let notes = format!(
        "{}{}{}",
        beamed("A", 4, 1, "begin"),
        beamed("C", 3, 2, "continue"),
        beamed("A", 4, 1, "end")
    );
    let lows = "<note><pitch><step>E</step><octave>2</octave></pitch><duration>3</duration><voice>6</voice><type>quarter</type><dot/><staff>2</staff></note>";
    let rest = "<note><rest measure=\"yes\"/><duration>3</duration><voice>5</voice><staff>2</staff></note>";
    let m = format!(
        "{notes}<backup><duration>3</duration></backup>{rest}<backup><duration>3</duration></backup>{lows}"
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">{attrs}{m}</measure></part></score-partwise>"
    );
    let layout = engrave("rest_reaching_stem.musicxml", &xml);
    let mut lines: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, epiphany_core::TypedObjectId::Staff(_))
                && s.from.y == s.to.y
        })
        .map(|s| s.from.y.0)
        .collect();
    lines.sort_by(|a, b| b.total_cmp(a));
    lines.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    let (_, y) = rest_at(&layout);
    assert!(
        (y - lines[5]).abs() < 0.05,
        "an upper voice's rest hanging from its staff's top line {}: {y}",
        lines[5]
    );
}

/// A page's first staff stands below a rest that is its highest ink, its extent
/// reading the rest where it is cleared.
#[test]
fn a_cleared_rest_joins_its_staffs_extent() {
    let high = "<note><pitch><step>A</step><octave>7</octave></pitch><duration>8</duration><voice>2</voice><type>half</type></note>";
    let middle = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{high}{high}"
    );
    let layout = engrave("rest_extent.musicxml", &around(&middle));
    let r = ink(&layout, "rest");
    let system = layout.systems().next().expect("a system");
    let top = system.bounding_box.origin.y.0 + system.bounding_box.size.height.0;
    assert!(
        (top - r.3).abs() < 0.02,
        "the system's top {top} at the rest's {r:?}"
    );
}

/// A tie through a rest at the bar's middle moves it, its voice's way, as far as
/// the tie needs: another voice's high note held across the bar and over the next
/// (Gould, p. 41). MuseScore's rule reads no curve.
#[test]
fn a_measure_rest_clears_a_tie_passing_through_it() {
    let tied = |stop: bool, start: bool| {
        let (a, b) = (
            if stop { "<tie type=\"stop\"/>" } else { "" },
            if start { "<tie type=\"start\"/>" } else { "" },
        );
        let (c, d) = (
            if stop { "<tied type=\"stop\"/>" } else { "" },
            if start { "<tied type=\"start\"/>" } else { "" },
        );
        format!(
            "<note><pitch><step>A</step><octave>5</octave></pitch><duration>16</duration>{a}{b}<voice>2</voice><type>whole</type><notations>{c}{d}</notations></note>"
        )
    };
    let middle = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{}",
        tied(true, true)
    );
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{}</measure><measure number=\"2\">{middle}</measure><measure number=\"3\">{}</measure>",
        tied(false, true),
        tied(true, false)
    ));
    let layout = engrave("rest_on_tie.musicxml", &xml);
    let r = ink(&layout, "rest");
    let mut passed = 0;
    for c in layout
        .curves
        .iter()
        .filter(|c| matches!(c.provenance.source, epiphany_core::TypedObjectId::Tie(_)))
    {
        let cp = c.control_points();
        for k in 0..=200 {
            let t = k as f32 / 200.0;
            let u = 1.0 - t;
            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            let x: f32 = (0..4).map(|i| w[i] * cp[i].x.0).sum();
            let y: f32 = (0..4).map(|i| w[i] * cp[i].y.0).sum();
            if x > r.0 && x < r.1 {
                passed += 1;
                assert!(
                    y + c.thickness.0 * 0.5 <= r.2 + 1e-3 || y - c.thickness.0 * 0.5 >= r.3 - 1e-3,
                    "the tie at ({x}, {y}) on the rest {r:?}"
                );
            }
        }
    }
    assert!(passed > 0, "a tie passes across the rest {r:?}");
}

/// What a measure's content is, for the slurs on raised rests below.
fn raised_rest(voice: u8, slur: &str, other: &str, octave: u8) -> String {
    let n = format!("<note><pitch><step>{other}</step><octave>{octave}</octave></pitch><duration>8</duration><voice>{}</voice><type>half</type></note>", 3 - voice);
    format!("<note><rest measure=\"yes\"/><duration>16</duration><voice>{voice}</voice>{slur}</note><backup><duration>16</duration></backup>{n}{n}")
}

fn slur_mark(kind: &str) -> String {
    format!("<notations><slur type=\"{kind}\" number=\"1\"/></notations>")
}

fn rest_heights(layout: &ResolvedLayoutIR) -> Vec<f32> {
    let mut rests: Vec<(f32, f32)> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
        .map(|g| (g.position.x.0, g.position.y.0))
        .collect();
    rests.sort_by(|a, b| a.0.total_cmp(&b.0));
    rests.into_iter().map(|r| r.1).collect()
}

/// A rest's own slur stands on it and goes where it goes, so it gives the rest
/// no reason to move: a slur's end and a slur's start leave their rests level
/// with one that has none.
#[test]
fn a_measure_rest_reads_no_slur_of_its_own() {
    let start = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("start")));
    let stop = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("stop")));
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{}</measure><measure number=\"3\">{}</measure><measure number=\"4\">{stop}</measure><measure number=\"5\">{}</measure>",
        raised_rest(1, &slur_mark("stop"), "C", 6),
        raised_rest(1, &slur_mark("start"), "C", 6),
        raised_rest(1, "", "C", 6),
    ));
    let layout = engrave("rest_own_slur.musicxml", &xml);
    let y = rest_heights(&layout);
    eprintln!("{y:?}");
    assert_eq!(y.len(), 3);
    assert_eq!(y[0], y[2], "a slur's end leaves it level: {y:?}");
    assert_eq!(y[1], y[2], "a slur's start leaves it level: {y:?}");
}

/// A slur's end follows a rest moved toward its arc and stands off its ink on
/// the arc's side, keeping its arc over the chord it now has.
#[test]
fn a_slurs_end_stands_off_a_rest_raised_from_its_height() {
    let start = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("start")));
    let stop = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("stop")));
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{}</measure><measure number=\"3\">{}</measure><measure number=\"4\">{stop}</measure>",
        raised_rest(1, &slur_mark("stop"), "C", 6),
        raised_rest(1, &slur_mark("start"), "C", 6),
    ));
    let layout = engrave("rest_raised_slur.musicxml", &xml);
    let mut rests: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
        .collect();
    rests.sort_by(|a, b| a.position.x.0.total_cmp(&b.position.x.0));
    let tops: Vec<f32> = rests
        .iter()
        .map(|r| r.position.y.0 + r.bounding_box.top.0)
        .collect();
    let mut curves: Vec<_> = layout.curves.iter().map(|c| c.control_points()).collect();
    curves.sort_by(|a, b| a[0].x.0.total_cmp(&b[0].x.0));
    assert_eq!(curves.len(), 2);
    assert!(
        (curves[0][3].y.0 - (tops[0] + 0.9)).abs() < 1e-3,
        "the slur's end over its rest: {} against {}",
        curves[0][3].y.0,
        tops[0]
    );
    assert!(
        (curves[1][0].y.0 - (tops[1] + 0.9)).abs() < 1e-3,
        "the slur's start over its rest: {} against {}",
        curves[1][0].y.0,
        tops[1]
    );
    for c in &curves {
        let third = (c[3].y.0 - c[0].y.0) / 3.0;
        let (one, two) = (c[1].y.0 - c[0].y.0 - third, c[2].y.0 - c[3].y.0 + third);
        assert!(
            one > 1.0 && (one - two).abs() < 1e-3,
            "its arc over its chord: {one} and {two}"
        );
    }
}

/// And under a lower voice's rest moved down, a slur below it, the slur's other
/// end staying at its note.
#[test]
fn a_slurs_end_stands_off_a_rest_lowered_from_its_height() {
    let note = "<note><pitch><step>E</step><octave>4</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type><stem>up</stem><notations><slur type=\"start\" number=\"1\"/></notations></note>";
    let xml = |rest: String| {
        single(&format!(
            "<measure number=\"1\">{HEAD}{note}</measure><measure number=\"2\">{rest}</measure><measure number=\"3\">{WHOLE}</measure>"
        ))
    };
    let layout = engrave(
        "rest_lowered_slur.musicxml",
        &xml(raised_rest(2, &slur_mark("stop"), "A", 3)),
    );
    let r = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("rest"))
        .expect("the rest");
    let c = layout.curves[0].control_points();
    assert!(c[1].y.0 < c[0].y.0, "a slur below");
    let bottom = r.position.y.0 + r.bounding_box.bottom.0;
    assert!(
        (c[3].y.0 - (bottom - 0.9)).abs() < 1e-3,
        "the slur's end {} under the rest's bottom {bottom}",
        c[3].y.0
    );
    let head = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadWhole")
        .min_by(|a, b| a.position.x.0.total_cmp(&b.position.x.0))
        .expect("the E");
    assert!(
        c[0].y.0 < head.position.y.0 && c[0].y.0 > head.position.y.0 - 2.0,
        "its start {} under its note {}",
        c[0].y.0,
        head.position.y.0
    );
}

/// A rest that nothing moves leaves its slur's end at its default where that
/// already stands further from the rest than the floor: over the staff, not at
/// the rest's ink.
#[test]
fn a_slurs_end_stays_on_a_rest_at_its_height() {
    let start = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("start")));
    let stop = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice>{}</note>",
        slur_mark("stop")
    );
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{stop}</measure><measure number=\"3\">{WHOLE}</measure>"
    ));
    let layout = engrave("rest_unmoved_slur.musicxml", &xml);
    let top = top_line(&layout);
    let c = layout.curves[0].control_points();
    assert!(
        (c[3].y.0 - (top + 0.7)).abs() < 1e-3,
        "the slur's end {} over the staff's top line {top}",
        c[3].y.0
    );
}

const BACK: &str = "<backup><duration>16</duration></backup>";

/// A whole note on the upper of two staves, its first voice at home.
fn upper_whole(step: &str, octave: u8, notations: &str) -> String {
    format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type><staff>1</staff>{notations}</note>")
}

/// A whole note of the second voice, at home on the lower staff or visiting the
/// upper, with no drawn stem.
fn second_voice_whole(step: &str, octave: u8, staff: u8) -> String {
    format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type><staff>{staff}</staff></note>")
}

/// A note of the second voice sounding with the last, up on the upper staff.
fn second_voice_chord(step: &str, octave: u8) -> String {
    second_voice_whole(step, octave, 1).replacen("<note>", "<note><chord/>", 1)
}

/// The second voice's half notes on the upper staff, stems drawn, so a rest alone
/// beside them stays at its height.
fn second_voice_halves(step: &str, octave: u8) -> String {
    format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><stem>up</stem><staff>1</staff></note>").repeat(2)
}

/// A bar's first voice on the upper staff with the second voice given.
fn upper_rest_beside(notations: &str, second_voice: &str) -> String {
    format!("<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice><staff>1</staff>{notations}</note>{BACK}{second_voice}")
}

/// Twelve bars on two staves, narrow enough that a voice visiting the upper staff
/// stands within a rest's reach: the bars given, the rest whole notes on each.
fn narrow_system(bars: &[String]) -> String {
    let attributes = "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>";
    let filler = at_home("");
    let mut measures = String::new();
    for k in 0..12 {
        let first = if k == 0 { attributes } else { "" };
        let bar = bars.get(k).unwrap_or(&filler);
        measures += &format!("<measure number=\"{}\">{first}{bar}</measure>", k + 1);
    }
    single(&measures)
}

/// A bar of whole notes, one on each staff, the upper carrying what is given.
fn at_home(notations: &str) -> String {
    format!(
        "{}{BACK}{}",
        upper_whole("C", 5, notations),
        second_voice_whole("C", 3, 2)
    )
}

/// A slur ending on a rest alone in the second bar, from a whole note before it.
fn slur_ending_on_rest(from: (&str, u8), second_voice: &str) -> String {
    narrow_system(&[
        format!(
            "{}{BACK}{}",
            upper_whole(from.0, from.1, &slur_mark("start")),
            second_voice_whole("C", 3, 2)
        ),
        upper_rest_beside(&slur_mark("stop"), second_voice),
    ])
}

/// A slur starting on a rest alone in the second bar, to the third bar's first note.
fn slur_starting_on_rest(second_voice: &str, below: bool) -> String {
    let to = if below {
        let half = |notations: &str| {
            format!("<note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><type>half</type><stem>up</stem><staff>1</staff>{notations}</note>")
        };
        format!(
            "{}{}{BACK}{}",
            half(&slur_mark("stop")),
            half(""),
            second_voice_whole("C", 3, 2)
        )
    } else {
        at_home(&slur_mark("stop"))
    };
    narrow_system(&[
        at_home(""),
        upper_rest_beside(&slur_mark("start"), second_voice),
        to,
    ])
}

/// The ink of each head in the second bar of the first system's upper staff, by
/// height: left, right, bottom, top.
fn second_bar_heads(layout: &ResolvedLayoutIR) -> Vec<(f32, f32, f32, f32)> {
    let top = top_line(layout);
    let mut bars: Vec<f32> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "barlineSingle" && g.position.y.0 > top - 10.0)
        .map(|g| g.position.x.0)
        .collect();
    bars.sort_by(f32::total_cmp);
    bars.dedup();
    let mut heads: Vec<(f32, f32, f32, f32)> = layout
        .glyphs
        .iter()
        .filter(|g| {
            let (x, y) = (g.position.x.0, g.position.y.0);
            g.glyph.as_str().starts_with("notehead") && x > bars[0] && x < bars[1] && y > top - 10.0
        })
        .map(|g| {
            let (x, y, b) = (g.position.x.0, g.position.y.0, g.bounding_box);
            (x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0)
        })
        .collect();
    heads.sort_by(|a, b| a.2.total_cmp(&b.2));
    heads
}

/// How near a curve's line comes to a box of ink: left, right, bottom, top.
fn curve_off(c: [epiphany_layout_ir::Point; 4], ink: (f32, f32, f32, f32)) -> f32 {
    (0..=120)
        .map(|k| {
            let (t, u) = (k as f32 / 120.0, 1.0 - k as f32 / 120.0);
            let along = |p: [f32; 4]| {
                u * u * u * p[0]
                    + 3.0 * u * u * t * p[1]
                    + 3.0 * u * t * t * p[2]
                    + t * t * t * p[3]
            };
            let x = along(c.map(|p| p.x.0));
            let y = along(c.map(|p| p.y.0));
            let dx = (ink.0 - x).max(x - ink.1).max(0.0);
            let dy = (ink.2 - y).max(y - ink.3).max(0.0);
            (dx * dx + dy * dy).sqrt()
        })
        .fold(f32::INFINITY, f32::min)
}

/// The slur's end on the rest, and how far its rest stands from its height, in
/// spaces: a rest alone on this staff hangs a space under its top line.
fn end_on_rest(layout: &ResolvedLayoutIR, ending: bool) -> (f32, f32) {
    let c = layout.curves[0].control_points();
    (
        if ending { c[3].y.0 } else { c[0].y.0 },
        rest_at(layout).1 - top_line(layout) + 1.0,
    )
}

/// A rest sent down from its slur's arc leaves the slur's end at its default, over
/// the head that sent it, where following the rest would pull the arc through that
/// head. The end stands far more than the floor off the rest.
#[test]
fn a_slurs_end_keeps_its_default_off_a_rest_sent_down_from_its_arc() {
    let beside = second_voice_whole("F", 5, 1);
    for (name, ending, xml) in [
        (
            "floor_down_end.musicxml",
            true,
            slur_ending_on_rest(("C", 5), &beside),
        ),
        (
            "floor_down_start.musicxml",
            false,
            slur_starting_on_rest(&beside, false),
        ),
    ] {
        let layout = engrave(name, &xml);
        let (end, moved) = end_on_rest(&layout, ending);
        assert_eq!(moved, -1.0, "{name}: the head sends the rest a space down");
        let c = layout.curves[0].control_points();
        assert!(c[1].y.0 > c[0].y.0, "{name}: its arc above");
        let head = second_bar_heads(&layout)[0];
        assert!(
            (end - (head.3 + 0.7)).abs() < 1e-3,
            "{name}: its end {end} at its default over the head's top {}",
            head.3
        );
        let rest = ink(&layout, "rest");
        assert!(
            end - rest.3 > 0.9,
            "{name}: its end {end} further out than the floor over the rest's top {}",
            rest.3
        );
        let off = curve_off(c, head);
        assert!(off > 1.0, "{name}: the arc {off} clear of the head");
    }
}

/// And its mirror: a rest sent up from an arc below keeps the end at its default,
/// under the head that sent it. An import reaches it both ways, a note's stem turning
/// the arc under.
#[test]
fn a_slurs_end_keeps_its_default_off_a_rest_sent_up_from_its_arc() {
    let beside = second_voice_whole("A", 4, 1);
    for (name, ending, xml) in [
        (
            "floor_up_end.musicxml",
            true,
            slur_ending_on_rest(("E", 4), &beside),
        ),
        (
            "floor_up_start.musicxml",
            false,
            slur_starting_on_rest(&beside, true),
        ),
    ] {
        let layout = engrave(name, &xml);
        let (end, moved) = end_on_rest(&layout, ending);
        assert_eq!(moved, 1.0, "{name}: the head sends the rest a space up");
        let c = layout.curves[0].control_points();
        assert!(c[1].y.0 < c[0].y.0, "{name}: its arc below");
        let head = second_bar_heads(&layout)[0];
        assert!(
            (end - (head.2 - 0.7)).abs() < 1e-3,
            "{name}: its end {end} at its default under the head's bottom {}",
            head.2
        );
        let rest = ink(&layout, "rest");
        assert!(
            rest.2 - end > 0.9,
            "{name}: its end {end} further out than the floor under the rest's bottom {}",
            rest.2
        );
        let off = curve_off(c, head);
        assert!(off > 1.0, "{name}: the arc {off} clear of the head");
    }
}

/// A rest nothing moved still holds its slur's end off its ink: the default reads
/// the column's heads and can stand on the rest, where the floor lifts it.
#[test]
fn a_slurs_end_stands_off_a_rest_nothing_moved_from_an_arc_above() {
    let beside = second_voice_halves("G", 4);
    for (name, ending, xml) in [
        (
            "floor_unmoved_end.musicxml",
            true,
            slur_ending_on_rest(("C", 5), &beside),
        ),
        (
            "floor_unmoved_start.musicxml",
            false,
            slur_starting_on_rest(&beside, false),
        ),
    ] {
        let layout = engrave(name, &xml);
        let (end, moved) = end_on_rest(&layout, ending);
        assert_eq!(moved, 0.0, "{name}: nothing moves the rest");
        let c = layout.curves[0].control_points();
        assert!(c[1].y.0 > c[0].y.0, "{name}: its arc above");
        let rest = ink(&layout, "rest");
        assert!(
            (end - (rest.3 + 0.9)).abs() < 1e-3,
            "{name}: its end {end} off the rest's top {}",
            rest.3
        );
    }
}

/// And under an arc below, a head high over the rest setting the default on the
/// rest's far side: the floor brings the end round under it.
#[test]
fn a_slurs_end_stands_off_a_rest_nothing_moved_from_an_arc_below() {
    let beside = second_voice_whole("C", 6, 1);
    for (name, ending, xml) in [
        (
            "floor_unmoved_below_end.musicxml",
            true,
            slur_ending_on_rest(("E", 4), &beside),
        ),
        (
            "floor_unmoved_below_start.musicxml",
            false,
            slur_starting_on_rest(&beside, true),
        ),
    ] {
        let layout = engrave(name, &xml);
        let (end, moved) = end_on_rest(&layout, ending);
        assert_eq!(moved, 0.0, "{name}: nothing moves the rest");
        let c = layout.curves[0].control_points();
        assert!(
            c[1].y.0 + c[2].y.0 < c[0].y.0 + c[3].y.0,
            "{name}: its arc below"
        );
        let rest = ink(&layout, "rest");
        assert!(
            (end - (rest.2 - 0.9)).abs() < 1e-3,
            "{name}: its end {end} off the rest's bottom {}",
            rest.2
        );
    }
}

/// A rest moved toward its arc takes the end along only as far as the floor: a head
/// beyond it holds the default further out, and the end keeps that, where drawing it
/// in to the rest would run the arc through that head.
#[test]
fn a_slurs_end_keeps_a_default_further_out_than_a_rest_moved_toward_its_arc() {
    let over = format!(
        "{}{}",
        second_voice_whole("A", 4, 1),
        second_voice_chord("D", 6)
    );
    let under = format!(
        "{}{}",
        second_voice_whole("C", 4, 1),
        second_voice_chord("F", 5)
    );
    for (name, ending, xml, up, moved_by) in [
        (
            "floor_toward_up_end.musicxml",
            true,
            slur_ending_on_rest(("C", 5), &over),
            true,
            1.0,
        ),
        (
            "floor_toward_up_start.musicxml",
            false,
            slur_starting_on_rest(&over, false),
            true,
            1.0,
        ),
        (
            "floor_toward_down_end.musicxml",
            true,
            slur_ending_on_rest(("E", 4), &under),
            false,
            -1.0,
        ),
        (
            "floor_toward_down_start.musicxml",
            false,
            slur_starting_on_rest(&under, true),
            false,
            -1.0,
        ),
    ] {
        let layout = engrave(name, &xml);
        let (end, moved) = end_on_rest(&layout, ending);
        assert_eq!(moved, moved_by, "{name}: the near head moves the rest");
        let heads = second_bar_heads(&layout);
        let c = layout.curves[0].control_points();
        let (head, default) = if up {
            let h = heads[heads.len() - 1];
            (h, h.3 + 0.7)
        } else {
            (heads[0], heads[0].2 - 0.7)
        };
        assert!(
            (end - default).abs() < 1e-3,
            "{name}: its end {end} at its default {default}"
        );
        let off = curve_off(c, head);
        assert!(off > 0.7, "{name}: the arc {off} clear of the far head");
    }
}

/// A rest under another's slur reads it where it is drawn: up, so one a raised
/// rest carries over it stands clear over it and leaves it level with that rest,
/// where the arc as first set runs through its next height.
#[test]
fn a_rest_reads_a_slur_at_the_height_its_end_is_carried_to() {
    let start = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("start")));
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{}</measure><measure number=\"3\">{}</measure><measure number=\"4\">{WHOLE}</measure>",
        raised_rest(1, "", "E", 6),
        raised_rest(1, &slur_mark("stop"), "E", 6),
    ));
    let y = rest_heights(&engrave("rest_under_carried.musicxml", &xml));
    assert_eq!(y[0], y[1], "level under the slur: {y:?}");
}

/// And across: the slur's end carried to the next bar's rest passes this one a
/// little lower than the arc as first set, near enough to move it a space more.
#[test]
fn a_rest_reads_a_slur_at_the_place_its_end_is_carried_to() {
    let start = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("start")));
    let stop = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice>{}</note>",
        slur_mark("stop")
    );
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{}</measure><measure number=\"3\">{stop}</measure><measure number=\"4\">{WHOLE}</measure>",
        raised_rest(1, "", "G", 5),
    ));
    let y = rest_heights(&engrave("rest_under_carried_across.musicxml", &xml));
    assert_eq!(y[0] - y[1], 6.0, "over the arc as drawn: {y:?}");
}

/// A hidden rest's traced anchor is no ink: an upper voice's measure rest stays on
/// its line beside hidden rests of a third voice at the bar's middle.
#[test]
fn a_measure_rest_reads_no_hidden_rests_anchor() {
    let low: String = ["E", "F", "F", "E"]
        .iter()
        .map(|p| format!("<note><pitch><step>{p}</step><octave>4</octave></pitch><duration>4</duration><voice>2</voice><type>quarter</type><stem>down</stem></note>"))
        .collect();
    let hidden = "<note print-object=\"no\"><rest/><duration>8</duration><voice>3</voice><type>half</type></note>";
    let middle = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{low}<backup><duration>16</duration></backup>{hidden}{hidden}"
    );
    let layout = engrave("rest_hidden_anchor.musicxml", &around(&middle));
    assert_eq!(rest_at(&layout).0, "restWhole");
}

/// A slur over a system's break carries only the end standing on a rest, on the
/// piece that holds it: the other piece stays on its own line, whichever end.
#[test]
fn a_slur_over_a_break_keeps_its_other_piece() {
    let on_note = |kind: &str| {
        WHOLE.replace(
            "<type>whole</type>",
            &format!("<type>whole</type>{}", slur_mark(kind)),
        )
    };
    let on_rest = |kind: &str| {
        format!(
            "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice>{}</note>",
            slur_mark(kind)
        )
    };
    let broken = |m2: String, m3: String| {
        single(&format!(
            "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{m2}</measure><measure number=\"3\"><print new-system=\"yes\"/>{m3}</measure><measure number=\"4\">{WHOLE}</measure>"
        ))
    };
    let pieces = |layout: &ResolvedLayoutIR| -> Vec<[epiphany_layout_ir::Point; 4]> {
        let mut c: Vec<_> = layout.curves.iter().map(|c| c.control_points()).collect();
        assert_eq!(c.len(), 2, "two pieces");
        // The earlier system first, standing higher.
        c.sort_by(|a, b| b[0].y.0.total_cmp(&a[0].y.0));
        c
    };
    let page = PageGeometry::default();
    let right = page.size.width.0 - page.margins.right.0;
    // Ending on a rest: its first piece stays on its line.
    let ended = engrave(
        "slur_break_end.musicxml",
        &broken(on_note("start"), on_rest("stop")),
    );
    let p = pieces(&ended);
    assert!(
        p[0][3].x.0 <= right + 0.1,
        "the first piece ends on its line, {right}: {:?}",
        p[0]
    );
    // Starting on a rest: its last piece opens at its system's start.
    let started = engrave(
        "slur_break_start.musicxml",
        &broken(on_rest("start"), on_note("stop")),
    );
    let p = pieces(&started);
    assert!(
        p[1][0].x.0 < 15.0,
        "the last piece opens its system: {:?}",
        p[1]
    );
}

/// A whole note's bar on each side of an ending over `over` bars, each bar of the
/// ending two voices with a measure rest above.
fn score_with_an_ending(name: &str, over: usize) -> epiphany_core::Score {
    use epiphany_core::{
        AnchorOffset, MeasureId, MeasurePosition, RepeatKind, RepeatStructure, RepeatStructureId,
        TimeAnchor, Volta,
    };
    let low: String = ["E", "F"]
        .iter()
        .map(|p| format!("<note><pitch><step>{p}</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><type>half</type><stem>down</stem></note>"))
        .collect();
    let two = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{low}"
    );
    let body: String = (0..over)
        .map(|k| format!("<measure number=\"{}\">{two}</measure>", k + 2))
        .collect();
    let xml = single(&format!(
        "<measure number=\"1\">{HEAD}{WHOLE}</measure>{body}<measure number=\"{}\">{WHOLE}</measure>",
        over + 2
    ));
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, xml).expect("written");
    let mut score = load(&path).expect("loads").reduced.score.clone();
    let measures: Vec<MeasureId> = score.canvas.regions[0].staff_instances()[0]
        .measures
        .iter()
        .map(|m| m.id)
        .collect();
    let at = |i: usize| TimeAnchor::Measure {
        id: measures[i],
        position: MeasurePosition::Start,
        offset: AnchorOffset::Zero,
    };
    let id = score.identity.mint::<RepeatStructureId>();
    score.cross_cutting.repeats.push(RepeatStructure {
        id,
        start: at(0),
        end: at(over + 1),
        kind: RepeatKind::Volta,
        voltas: vec![Volta {
            endings: vec![1],
            start: at(1),
            end: at(over + 1),
        }],
    });
    score
}

fn rest_under_an_ending(name: &str, over: usize) -> ResolvedLayoutIR {
    Engraver::with_geometry(PageGeometry::default())
        .solve(
            &to_constrained(&to_logical(&score_with_an_ending(name, over))),
            &SolverConfig::default(),
        )
        .layout
}

/// An ending's bracket is no other part: an upper voice's rest under an ending of
/// one bar stays on its line, as the same rest does with no bracket, its middle
/// standing inside the rest's own bar.
#[test]
fn a_measure_rest_stays_under_a_one_bar_ending() {
    let layout = rest_under_an_ending("rest_under_one_bar_ending.musicxml", 1);
    assert!(
        layout.strokes.iter().any(|s| matches!(
            s.provenance.source,
            epiphany_core::TypedObjectId::RepeatStructure(_)
        ) && s.from.y == s.to.y),
        "the bracket's line is drawn"
    );
    assert_eq!(rest_at(&layout).0, "restWhole");
}

/// And under an ending of three bars, whose line's middle stands inside the middle
/// bar: that bar's rest stays level with the others.
#[test]
fn a_measure_rest_stays_under_an_ending_of_three_bars() {
    let layout = rest_under_an_ending("rest_under_three_bar_ending.musicxml", 3);
    let rests: Vec<&str> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
        .map(|g| g.glyph.as_str())
        .collect();
    assert_eq!(rests, ["restWhole", "restWhole", "restWhole"]);
}

/// Nor does a rest read a repeat structure's glyph, by its source. Every glyph one
/// draws rides a barline's or the end's column, none of whose members a rest reads,
/// so the constrained input is amended: the head of a whole note on the top line, set
/// across to the rest it would otherwise make climb, named as the structure's.
#[test]
fn a_measure_rest_reads_no_glyph_of_a_repeat_structure() {
    use epiphany_core::{RepeatStructureId, TypedObjectId};
    let top = "<note><pitch><step>F</step><octave>5</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type></note>";
    let middle = format!(
        "<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice></note><backup><duration>16</duration></backup>{top}"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rest_beside_structure_glyph.musicxml");
    std::fs::write(&path, around(&middle)).expect("written");
    let mut score = load(&path).expect("loads").reduced.score.clone();
    let structure = score.identity.mint::<RepeatStructureId>();
    let mut constrained = to_constrained(&to_logical(&score));
    let solve = |constrained: &epiphany_layout_ir::ConstrainedLayoutIR| {
        Engraver::with_geometry(PageGeometry::default())
            .solve(constrained, &SolverConfig::default())
            .layout
    };
    let high = |g: &&epiphany_layout_ir::ResolvedGlyph| g.glyph.as_str() == "noteheadWhole";
    let first = solve(&constrained);
    let head_x = first
        .glyphs
        .iter()
        .filter(high)
        .max_by(|a, b| a.position.y.0.total_cmp(&b.position.y.0))
        .expect("the high whole note")
        .position
        .x
        .0;
    let rest_x = ink(&first, "rest").0;
    let head = constrained
        .glyphs
        .iter_mut()
        .filter(|g| g.glyph.as_str() == "noteheadWhole")
        .max_by(|a, b| a.baseline.y.0.total_cmp(&b.baseline.y.0))
        .expect("the high whole note");
    head.baseline.x.0 += rest_x - head_x;
    assert_eq!(
        rest_at(&solve(&constrained)).0,
        "restWholeLegerLine",
        "it climbs a note's head"
    );
    let head = constrained
        .glyphs
        .iter_mut()
        .filter(|g| g.glyph.as_str() == "noteheadWhole")
        .max_by(|a, b| a.baseline.y.0.total_cmp(&b.baseline.y.0))
        .expect("the high whole note");
    head.provenance.source = TypedObjectId::RepeatStructure(structure);
    assert_eq!(
        rest_at(&solve(&constrained)).0,
        "restWhole",
        "it reads none of the structure's"
    );
}

fn syllable(kind: &str, text: &str) -> String {
    format!("<lyric number=\"1\"><syllabic>{kind}</syllabic><text>{text}</text></lyric>")
}

fn sung(step: &str, lyric: &str) -> String {
    format!(
        "<note><pitch><step>{step}</step><octave>5</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type>{lyric}</note>"
    )
}

fn sung_rest(lyric: &str) -> String {
    format!("<note><rest measure=\"yes\"/><duration>16</duration><voice>1</voice>{lyric}</note>")
}

/// Bars in order, the attributes opening the first.
fn bars(content: &[String]) -> String {
    let body: String = content
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let head = if i == 0 { HEAD } else { "" };
            format!("<measure number=\"{}\">{head}{c}</measure>", i + 1)
        })
        .collect();
    single(&body)
}

/// A syllable's edges across.
fn syllable_at(layout: &ResolvedLayoutIR, text: &str) -> (f32, f32) {
    let run = layout
        .texts
        .iter()
        .find(|t| t.text.as_str() == text)
        .unwrap_or_else(|| panic!("the syllable {text}"));
    (
        run.origin.x.0 + run.bounds.left.0,
        run.origin.x.0 + run.bounds.right.0,
    )
}

/// Each dash of a hyphen or stretch of a melisma line, by its ends across.
fn lyric_lines(layout: &ResolvedLayoutIR) -> Vec<(f32, f32)> {
    let mut lines: Vec<(f32, f32)> = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(
                s.provenance.source,
                epiphany_core::TypedObjectId::LyricLine(_)
            ) && s.from.x != s.to.x
        })
        .map(|s| (s.from.x.0, s.to.x.0))
        .collect();
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines
}

/// A hyphen stands midway between its two syllables (Gould, p. 448): every dash
/// between their inner edges, and their middle the gap's.
fn assert_hyphen_between(lines: &[(f32, f32)], left: (f32, f32), right: (f32, f32)) {
    let dashes: Vec<&(f32, f32)> = lines
        .iter()
        .filter(|l| l.0 > left.0 && l.1 < right.1)
        .collect();
    assert!(!dashes.is_empty(), "a hyphen in {lines:?}");
    for d in &dashes {
        assert!(
            d.0 >= left.1 && d.1 <= right.0,
            "a dash {d:?} between {left:?} and {right:?}"
        );
    }
    let first = dashes.first().expect("a dash").0;
    let last = dashes.last().expect("a dash").1;
    let (middle, gap) = ((first + last) * 0.5, (left.1 + right.0) * 0.5);
    assert!(
        (middle - gap).abs() < 0.02,
        "the hyphen's middle {middle} in the gap's {gap}"
    );
}

/// A hyphen leaving a syllable carried with its rest starts after it.
#[test]
fn a_hyphen_stands_after_its_syllable_on_a_rest() {
    let layout = engrave(
        "hyphen_from_rest.musicxml",
        &bars(&[
            sung("C", &syllable("single", "one")),
            sung_rest(&syllable("begin", "fi")),
            sung("D", &syllable("end", "ne")),
        ]),
    );
    assert_hyphen_between(
        &lyric_lines(&layout),
        syllable_at(&layout, "fi"),
        syllable_at(&layout, "ne"),
    );
}

/// And one reaching such a syllable ends before it.
#[test]
fn a_hyphen_stands_before_its_syllable_on_a_rest() {
    let layout = engrave(
        "hyphen_to_rest.musicxml",
        &bars(&[
            sung("C", &syllable("begin", "fi")),
            sung_rest(&syllable("end", "ne")),
            sung("D", &syllable("single", "next")),
        ]),
    );
    assert_hyphen_between(
        &lyric_lines(&layout),
        syllable_at(&layout, "fi"),
        syllable_at(&layout, "ne"),
    );
}

/// A syllable on a rest between two others holds both hyphens to it.
#[test]
fn hyphens_stand_on_either_side_of_a_syllable_on_a_rest() {
    let layout = engrave(
        "hyphen_both_sides.musicxml",
        &bars(&[
            sung("C", &syllable("begin", "pa")),
            sung_rest(&syllable("middle", "ri")),
            sung("D", &syllable("end", "so")),
        ]),
    );
    let (lines, pa, ri, so) = (
        lyric_lines(&layout),
        syllable_at(&layout, "pa"),
        syllable_at(&layout, "ri"),
        syllable_at(&layout, "so"),
    );
    assert_hyphen_between(&lines, pa, ri);
    assert_hyphen_between(&lines, ri, so);
}

/// And a hyphen between syllables on two rests moves both its ends.
#[test]
fn a_hyphen_between_syllables_on_two_rests_moves_both_its_ends() {
    let layout = engrave(
        "hyphen_two_rests.musicxml",
        &bars(&[
            sung("C", &syllable("single", "one")),
            sung_rest(&syllable("begin", "fi")),
            sung_rest(&syllable("end", "ne")),
            sung("D", &syllable("single", "two")),
        ]),
    );
    assert_hyphen_between(
        &lyric_lines(&layout),
        syllable_at(&layout, "fi"),
        syllable_at(&layout, "ne"),
    );
}

/// A melisma line leaving a syllable carried with its rest starts just after it
/// (Gould, p. 447), and still ends at its note.
#[test]
fn a_melisma_line_starts_after_its_syllable_on_a_rest() {
    let extend = "<lyric number=\"1\"><syllabic>single</syllabic><text>two</text><extend type=\"start\"/></lyric>";
    let stop = "<lyric number=\"1\"><extend type=\"stop\"/></lyric>";
    let layout = engrave(
        "melisma_from_rest.musicxml",
        &bars(&[
            sung("C", &syllable("single", "one")),
            sung_rest(extend),
            sung("D", stop),
            sung("E", &syllable("single", "three")),
        ]),
    );
    let lines = lyric_lines(&layout);
    assert_eq!(lines.len(), 1, "one melisma line: {lines:?}");
    let two = syllable_at(&layout, "two");
    assert!(
        lines[0].0 >= two.1 && lines[0].0 < two.1 + 1.0,
        "the line {:?} just after the syllable {two:?}",
        lines[0]
    );
    let d = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadWhole")
        .map(|g| g.position.x.0)
        .filter(|x| *x > two.1)
        .reduce(f32::min)
        .expect("the D");
    assert!(
        lines[0].1 > d && lines[0].1 < d + 3.0,
        "the line {:?} ends at its note {d}",
        lines[0]
    );
}

/// Two verses on a rest go with it together.
#[test]
fn two_verses_on_a_rest_stand_together_under_it() {
    let verses = "<lyric number=\"1\"><syllabic>end</syllabic><text>bb</text></lyric><lyric number=\"2\"><syllabic>single</syllabic><text>zz</text></lyric>";
    let layout = engrave(
        "two_verses_on_rest.musicxml",
        &bars(&[
            sung("C", &syllable("begin", "aa")),
            sung_rest(verses),
            sung("D", &syllable("single", "cc")),
        ]),
    );
    let r = centre(ink(&layout, "restWhole"));
    for text in ["bb", "zz"] {
        let (left, right) = syllable_at(&layout, text);
        assert!(
            ((left + right) * 0.5 - r).abs() < 0.3,
            "{text} at {left}..{right}, its rest at {r}"
        );
    }
}

/// A hidden rest's syllable stays at its column, and the hyphen reaching it with it.
#[test]
fn a_hyphen_reaching_a_syllable_on_a_hidden_rest_stays_before_it() {
    let hidden = format!(
        "<note print-object=\"no\"><rest measure=\"yes\"/><duration>16</duration><voice>1</voice>{}</note>",
        syllable("end", "ne")
    );
    let quarters = "<note><pitch><step>C</step><octave>5</octave></pitch><duration>4</duration><voice>1</voice><type>quarter</type></note>".repeat(4);
    let part = |content: &[String]| {
        content
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let head = if i == 0 { HEAD } else { "" };
                format!("<measure number=\"{}\">{head}{c}</measure>", i + 1)
            })
            .collect::<String>()
    };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part><score-part id=\"P2\"><part-name>B</part-name></score-part></part-list><part id=\"P1\">{}</part><part id=\"P2\">{}</part></score-partwise>",
        part(&[sung("C", &syllable("begin", "fi")), hidden, sung("D", &syllable("single", "two"))]),
        part(&[quarters.clone(), quarters.clone(), quarters]),
    );
    let layout = engrave("hyphen_hidden_rest.musicxml", &xml);
    let bar = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "barlineSingle")
        .map(|g| g.position.x.0)
        .reduce(f32::min)
        .expect("the first barline");
    let (fi, ne) = (syllable_at(&layout, "fi"), syllable_at(&layout, "ne"));
    assert!(
        ne.0 < bar + 6.0,
        "the syllable {ne:?} at its column after {bar}"
    );
    assert_hyphen_between(&lyric_lines(&layout), fi, ne);
}

/// A breve rest alone in 4/2 beside a voice visiting from the staff below, a whole
/// note at the bar's middle, each pitch given, with a bar of the voice at home.
fn rest_beside_a_visiting_whole_note(name: &str, step: &str, octave: u8) -> ResolvedLayoutIR {
    let attributes = "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>2</beat-type></time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>";
    let whole = |p: &str, o: u8, staff: u8| {
        format!("<note><pitch><step>{p}</step><octave>{o}</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type><staff>{staff}</staff></note>")
    };
    let xml = single(&format!(
        "<measure number=\"1\">{attributes}<note><rest measure=\"yes\"/><duration>32</duration><voice>1</voice><staff>1</staff></note><backup><duration>32</duration></backup>{}{}</measure><measure number=\"2\"><note><pitch><step>C</step><octave>5</octave></pitch><duration>32</duration><voice>1</voice><type>breve</type><staff>1</staff></note><backup><duration>32</duration></backup><note><pitch><step>C</step><octave>3</octave></pitch><duration>32</duration><voice>2</voice><type>breve</type><staff>2</staff></note></measure>",
        whole("C", 3, 2),
        whole(step, octave, 1)
    ));
    engrave(name, &xml)
}

/// A visitor with no drawn stem turns no rest: the rest goes the nearer way, as far
/// as the parts need (Gould, p. 36), from over its visitor to under it as the pitch
/// falls past it.
#[test]
fn a_rest_alone_beside_a_stemless_visitor_takes_the_nearer_side() {
    let rest_y = |name: &str, step: &str, octave: u8| {
        let layout = rest_beside_a_visiting_whole_note(name, step, octave);
        let (name, y) = rest_at(&layout);
        (name.to_owned(), y - top_line(&layout))
    };
    // Far under it, nothing moves it: the default, below its top line.
    let default = rest_y("vis_low.musicxml", "E", 4).1;
    let moved = |name: &str, step: &str, octave: u8| rest_y(name, step, octave).1 - default;
    assert_eq!(moved("vis_f5.musicxml", "F", 5), -1.0, "F5, one space down");
    assert_eq!(moved("vis_e5.musicxml", "E", 5), -1.0, "E5, one space down");
    assert_eq!(moved("vis_d5.musicxml", "D", 5), -2.0, "D5, two down");
    assert_eq!(moved("vis_c5.musicxml", "C", 5), -2.0, "C5, two down");
    assert_eq!(moved("vis_a4.musicxml", "A", 4), 1.0, "A4, one up");
}

/// So too a rest that does not fill its measure, beside the same visitor at its
/// column: a half rest on the middle line under a whole D5 goes down, the nearer way.
#[test]
fn a_shorter_rest_beside_a_stemless_visitor_takes_the_nearer_side() {
    let attributes = "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>";
    let upper = |content: &str| {
        format!("<note><pitch><step>{content}</step><octave>5</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type><staff>1</staff></note>")
    };
    let lower = |step: &str, octave: u8, staff: u8| {
        format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>16</duration><voice>2</voice><type>whole</type><staff>{staff}</staff></note>")
    };
    let back = "<backup><duration>16</duration></backup>";
    let xml = |step: &str, octave: u8, staff: u8| {
        single(&format!(
            "<measure number=\"1\">{attributes}<note><rest/><duration>8</duration><voice>1</voice><type>half</type><staff>1</staff></note><note><pitch><step>C</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><type>half</type><staff>1</staff></note>{back}{}</measure><measure number=\"2\">{}{back}{}</measure><measure number=\"3\">{}{back}{}</measure>",
            lower(step, octave, staff),
            upper("C"),
            lower("C", 3, 2),
            upper("C"),
            lower("C", 3, 2),
        ))
    };
    let y = |name: &str, step: &str, octave: u8, staff: u8| {
        let layout = engrave(name, &xml(step, octave, staff));
        rest_at(&layout).1 - top_line(&layout)
    };
    let home = y("short_vis_home.musicxml", "C", 3, 2);
    let beside = y("short_vis_d5.musicxml", "D", 5, 1);
    assert_eq!(beside - home, -1.0, "under the D5 by a space");
}

/// A slur over a break lifts only the piece on the raised rest: the piece on the
/// other system keeps the shape it has with no carry at all, whichever end, the rest
/// amended to name no slur.
#[test]
fn a_slur_over_a_break_lifts_only_the_piece_at_its_rest() {
    let on_note = |kind: &str| {
        WHOLE.replace(
            "<type>whole</type>",
            &format!("<type>whole</type>{}", slur_mark(kind)),
        )
    };
    let check = |name: &str, ending: bool| {
        let rest = raised_rest(1, &slur_mark(if ending { "stop" } else { "start" }), "E", 6);
        let (m2, m3) = if ending {
            (on_note("start"), rest)
        } else {
            (rest, on_note("stop"))
        };
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::write(
            &path,
            single(&format!(
                "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{m2}</measure><measure number=\"3\"><print new-system=\"yes\"/>{m3}</measure><measure number=\"4\">{WHOLE}</measure>"
            )),
        )
        .expect("written");
        let loaded = load(&path).expect("loads");
        let mut constrained = to_constrained(&to_logical(&loaded.reduced.score));
        let solve = |c: &epiphany_layout_ir::ConstrainedLayoutIR| {
            Engraver::with_geometry(PageGeometry::default())
                .solve(c, &SolverConfig::default())
                .layout
        };
        let pieces = |layout: &ResolvedLayoutIR| {
            let mut c: Vec<_> = layout.curves.iter().map(|c| c.control_points()).collect();
            assert_eq!(c.len(), 2, "two pieces");
            c.sort_by(|a, b| b[0].y.0.total_cmp(&a[0].y.0));
            c
        };
        let carried = solve(&constrained);
        let rest_top = carried
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str().starts_with("rest"))
            .map(|g| g.position.y.0 + g.bounding_box.top.0)
            .expect("the rest");
        let carried = pieces(&carried);
        for rest in &mut constrained.measure_rests {
            rest.slur_starts.clear();
            rest.slur_ends.clear();
        }
        let plain = pieces(&solve(&constrained));
        let (at_rest, other) = if ending {
            (carried[1][3].y.0, (&carried[0], &plain[0]))
        } else {
            (carried[0][0].y.0, (&carried[1], &plain[1]))
        };
        assert!(
            (at_rest - (rest_top + 0.9)).abs() < 1e-3,
            "{name}: its end {at_rest} over its rest's top {rest_top}"
        );
        for k in 0..4 {
            let (a, b) = (other.0[k], other.1[k]);
            assert!(
                (a.x.0 - b.x.0).abs() < 1e-3
                    && (a.y.0 - other.0[0].y.0 - (b.y.0 - other.1[0].y.0)).abs() < 1e-3,
                "{name}: the other piece keeps its shape: {:?} and {:?}",
                other.0,
                other.1
            );
        }
    };
    check("slur_lift_end.musicxml", true);
    check("slur_lift_start.musicxml", false);
}

/// A rest under a slur that runs over a break reads each piece as it is drawn: the
/// later piece with no carry of the end on the earlier system, and the earlier with
/// none of the end on the later. Each height is the first clear of its piece.
#[test]
fn a_rest_under_a_slur_over_a_break_reads_each_piece_as_drawn() {
    let start = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("start")));
    let stop = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("stop")));
    let over_its_staff = |layout: &ResolvedLayoutIR, system: usize, y: f32| {
        let s = layout.systems().nth(system).expect("the system");
        let top = s
            .primitives
            .strokes
            .iter()
            .map(|&i| &layout.strokes[i as usize])
            .filter(|t| matches!(t.provenance.source, epiphany_core::TypedObjectId::Staff(_)))
            .map(|t| t.from.y.0)
            .fold(f32::NEG_INFINITY, f32::max);
        y - top
    };
    let rest_on = |layout: &ResolvedLayoutIR, system: usize| {
        let s = layout.systems().nth(system).expect("the system");
        s.primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .find(|g| g.glyph.as_str().starts_with("rest"))
            .expect("its rest")
            .position
            .y
            .0
    };
    let later = engrave(
        "rest_under_later_piece.musicxml",
        &single(&format!(
            "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{}</measure><measure number=\"3\"><print new-system=\"yes\"/>{}</measure><measure number=\"4\">{stop}</measure>",
            raised_rest(1, &slur_mark("start"), "C", 6),
            raised_rest(1, "", "G", 5),
        )),
    );
    assert_eq!(
        over_its_staff(&later, 1, rest_on(&later, 1)),
        5.0,
        "under the later piece, with no carry of its start"
    );
    let earlier = engrave(
        "rest_under_earlier_piece.musicxml",
        &single(&format!(
            "<measure number=\"1\">{HEAD}{start}</measure><measure number=\"2\">{}</measure><measure number=\"3\"><print new-system=\"yes\"/>{}</measure><measure number=\"4\">{WHOLE}</measure>",
            raised_rest(1, "", "D", 6),
            raised_rest(1, &slur_mark("stop"), "C", 6),
        )),
    );
    assert_eq!(
        over_its_staff(&earlier, 0, rest_on(&earlier, 0)),
        6.0,
        "under the earlier piece, with no carry of its end"
    );
}

/// The rest carrying a slur's start clears before one under that slur, whatever
/// order the input holds them in, so the other reads the slur at its height. The
/// importer holds them in time, so the constrained input is amended, the two swapped.
#[test]
fn a_rest_clears_after_the_rest_its_slur_starts_on() {
    let stop = WHOLE.replace("</type>", &format!("</type>{}", slur_mark("stop")));
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rest_after_slur_start.musicxml");
    std::fs::write(
        &path,
        single(&format!(
            "<measure number=\"1\">{HEAD}{WHOLE}</measure><measure number=\"2\">{}</measure><measure number=\"3\">{}</measure><measure number=\"4\">{stop}</measure>",
            raised_rest(1, &slur_mark("start"), "C", 6),
            raised_rest(1, "", "C", 6),
        )),
    )
    .expect("written");
    let loaded = load(&path).expect("loads");
    let mut constrained = to_constrained(&to_logical(&loaded.reduced.score));
    let heights = |c: &epiphany_layout_ir::ConstrainedLayoutIR| {
        rest_heights(
            &Engraver::with_geometry(PageGeometry::default())
                .solve(c, &SolverConfig::default())
                .layout,
        )
    };
    let in_time = heights(&constrained);
    assert_eq!(in_time[0], in_time[1], "level: {in_time:?}");
    constrained.measure_rests.swap(0, 1);
    assert_eq!(heights(&constrained), in_time, "the order held makes none");
}

/// A melisma line keeps its end at its last note: a next syllable carried with its
/// own rest moves a hyphen to it, and no melisma line.
#[test]
fn a_melisma_line_keeps_its_end_beside_a_next_syllable_on_a_rest() {
    let extend = "<lyric number=\"1\"><syllabic>single</syllabic><text>two</text><extend type=\"start\"/></lyric>";
    let stop = "<lyric number=\"1\"><extend type=\"stop\"/></lyric>";
    let layout = engrave(
        "melisma_before_rest.musicxml",
        &bars(&[
            sung("C", extend),
            sung("D", stop),
            sung_rest(&syllable("single", "next")),
        ]),
    );
    let lines = lyric_lines(&layout);
    assert_eq!(lines.len(), 1, "one melisma line: {lines:?}");
    let d = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadWhole")
        .map(|g| g.position.x.0)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        lines[0].1 > d && lines[0].1 < d + 3.0,
        "the line {:?} ends at its note {d}",
        lines[0]
    );
}
