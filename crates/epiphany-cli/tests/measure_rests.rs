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
