//! The expression family on the page: marks at their notes, fermatas, breath
//! marks and caesuras (X4c.3), dynamics and hairpins (X4c.4), and lyrics
//! (X4c.5), each on a hand-written file, against MuseScore 4.7.5's default
//! rules as `marks.rs` and the constrained pass cite them.

use std::collections::BTreeSet;
use std::path::Path;

use epiphany_cli::{engrave, load, Loaded};
use epiphany_core::{Event, EventId, TypedObjectId};
use epiphany_layout_ir::{is_beam_stroke, ResolvedGlyph, ResolvedLayoutIR, ResolvedText};

/// A part of one treble staff, or of a treble and a bass staff, in 4/4 with
/// an eighth per division, its measures as written.
fn part(name: &str, staves: u8, measures: &[String]) -> Loaded {
    part_in(name, staves, 2, measures)
}

/// [`part`] with `divisions` to the quarter.
fn part_in(name: &str, staves: u8, divisions: u8, measures: &[String]) -> Loaded {
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
                    "<attributes><divisions>{divisions}</divisions><time><beats>4</beats>\
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

/// A note: its pitch as a step and octave (`"C5"`, `"F#4"`) or `"rest"`, its
/// duration in eighths and type, its voice and staff, and what its
/// `<notations>` hold, if anything.
fn note(pitch: &str, duration: u8, kind: &str, voice: u8, staff: u8, notations: &str) -> String {
    let what = if pitch == "rest" {
        String::from("<rest/>")
    } else {
        let (step, rest) = pitch.split_at(1);
        let (alter, octave) = match rest.strip_prefix('#') {
            Some(octave) => ("<alter>1</alter>", octave),
            None => ("", rest),
        };
        format!("<pitch><step>{step}</step>{alter}<octave>{octave}</octave></pitch>")
    };
    let notations = if notations.is_empty() {
        String::new()
    } else {
        format!("<notations>{notations}</notations>")
    };
    format!(
        "<note>{what}<duration>{duration}</duration><voice>{voice}</voice><type>{kind}</type>\
         <staff>{staff}</staff>{notations}</note>"
    )
}

/// The same note sounding with the one before it.
fn chord(pitch: &str, duration: u8, kind: &str, voice: u8, staff: u8, notations: &str) -> String {
    note(pitch, duration, kind, voice, staff, notations).replacen("<note>", "<note><chord/>", 1)
}

/// A direction on `staff` holding `content`, standing at the next note.
fn direction(staff: u8, content: &str) -> String {
    format!(
        "<direction><direction-type>{content}</direction-type><staff>{staff}</staff></direction>"
    )
}

fn dynamic(name: &str, staff: u8) -> String {
    direction(staff, &format!("<dynamics><{name}/></dynamics>"))
}

fn wedge(kind: &str, staff: u8) -> String {
    direction(staff, &format!("<wedge type=\"{kind}\"/>"))
}

/// `note` sung to `text` as syllable `syllabic` of verse `verse`, extending
/// a melisma when `extend`.
fn syllable(note: String, verse: u8, syllabic: &str, text: &str, extend: bool) -> String {
    let extend = if extend { "<extend/>" } else { "" };
    note.replacen(
        "</note>",
        &format!(
            "<lyric number=\"{verse}\"><syllabic>{syllabic}</syllabic><text>{text}</text>\
             {extend}</lyric></note>"
        ),
        1,
    )
}

/// The page's runs whose words are `words`, left to right.
fn runs<'a>(layout: &'a ResolvedLayoutIR, words: &str) -> Vec<&'a ResolvedText> {
    let mut found: Vec<&ResolvedText> = layout.texts.iter().filter(|t| t.text == words).collect();
    found.sort_by(|a, b| a.origin.x.0.total_cmp(&b.origin.x.0));
    found
}

/// The lines a syllable draws, its hyphen's dashes and its melisma line, by
/// the syllable they belong to: each as `(left, right, y)`, left to right.
fn lyric_lines(layout: &ResolvedLayoutIR) -> Vec<(f32, f32, f32)> {
    let mut found: Vec<(f32, f32, f32)> = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::LyricLine(_)) && s.from != s.to)
        .map(|s| (s.from.x.0, s.to.x.0, s.from.y.0))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found
}

/// `note` sung to a syllable.
fn sung(note: String) -> String {
    note.replacen(
        "</note>",
        "<lyric number=\"1\"><syllabic>single</syllabic><text>la</text></lyric></note>",
        1,
    )
}

/// The resolved glyph named `name`, the only one.
fn only<'a>(layout: &'a ResolvedLayoutIR, name: &str) -> &'a ResolvedGlyph {
    let found: Vec<&ResolvedGlyph> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == name)
        .collect();
    assert_eq!(found.len(), 1, "{name}");
    found[0]
}

/// A stroke's ends, `(from, to)`, each as `(x, y)`.
type Leg = ((f32, f32), (f32, f32));

/// A hairpin's legs, upper first: each `(from, to)` as `(x, y)` pairs, by
/// the spanner it draws, every piece of it on the page.
fn hairpin_legs(layout: &ResolvedLayoutIR) -> Vec<Leg> {
    let mut legs: Vec<Leg> = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Spanner(_)) && s.from != s.to)
        .map(|s| ((s.from.x.0, s.from.y.0), (s.to.x.0, s.to.y.0)))
        .collect();
    legs.sort_by(|a, b| a.0 .0.total_cmp(&b.0 .0).then(b.1 .1.total_cmp(&a.1 .1)));
    legs
}

fn backup(duration: u8) -> String {
    format!("<backup><duration>{duration}</duration></backup>")
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

fn text_box(text: &ResolvedText) -> Ink {
    let (x, y, b) = (text.origin.x.0, text.origin.y.0, &text.bounds);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

fn centre_x(ink: Ink) -> f32 {
    (ink[0] + ink[2]) * 0.5
}

/// Every glyph named `name` that `event` draws, its own or synthesized from
/// it, left to right.
fn glyphs(layout: &ResolvedLayoutIR, event: EventId, name: &str) -> Vec<Ink> {
    let mut found: Vec<Ink> = layout
        .glyphs
        .iter()
        .filter(|g| g.provenance.source == TypedObjectId::Event(event) && g.glyph.as_str() == name)
        .map(glyph_box)
        .collect();
    found.sort_by(|a, b| a[0].total_cmp(&b[0]));
    found
}

/// The one glyph named `name` that `event` draws.
fn the_glyph(layout: &ResolvedLayoutIR, event: EventId, name: &str) -> Ink {
    let found = glyphs(layout, event, name);
    assert_eq!(found.len(), 1, "{name} of {event:?}");
    found[0]
}

/// Every glyph named `name` on the page, left to right: a fermata, breath
/// mark or caesura is its marker's, not its note's.
fn named(layout: &ResolvedLayoutIR, name: &str) -> Vec<Ink> {
    let mut found: Vec<Ink> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == name)
        .map(glyph_box)
        .collect();
    found.sort_by(|a, b| a[0].total_cmp(&b[0]));
    found
}

/// A note's heads, lowest first.
fn heads(loaded: &Loaded, layout: &ResolvedLayoutIR, event: EventId) -> Vec<Ink> {
    let Some(Event::Pitched(n)) = loaded.reduced.score.events.get(event) else {
        panic!("a note");
    };
    let mut found: Vec<Ink> = layout
        .glyphs
        .iter()
        .filter(|g| {
            matches!(&g.provenance.source, TypedObjectId::Pitch(p) if n.pitches.iter().any(|q| q.id == *p))
                && g.glyph.as_str().starts_with("notehead")
        })
        .map(glyph_box)
        .collect();
    found.sort_by(|a, b| a[1].total_cmp(&b[1]));
    found
}

fn head(loaded: &Loaded, layout: &ResolvedLayoutIR, event: EventId) -> Ink {
    let found = heads(loaded, layout, event);
    assert_eq!(found.len(), 1, "one head");
    found[0]
}

/// A note's ledger lines.
fn ledgers(loaded: &Loaded, layout: &ResolvedLayoutIR, event: EventId) -> Vec<Ink> {
    let Some(Event::Pitched(n)) = loaded.reduced.score.events.get(event) else {
        panic!("a note");
    };
    layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(&s.provenance.source, TypedObjectId::Pitch(p) if n.pitches.iter().any(|q| q.id == *p))
                && s.from.y == s.to.y
                && s.from.x != s.to.x
        })
        .map(|s| {
            let half = s.thickness.0 * 0.5;
            [s.from.x.0.min(s.to.x.0), s.from.y.0 - half, s.from.x.0.max(s.to.x.0), s.from.y.0 + half]
        })
        .collect()
}

/// A note's stem: its x, and its lower and upper ends.
fn stem(layout: &ResolvedLayoutIR, event: EventId) -> (f32, f32, f32) {
    let stems: Vec<_> = layout
        .strokes
        .iter()
        .filter(|s| {
            s.provenance.source == TypedObjectId::Event(event)
                && s.from.x == s.to.x
                && s.from.y != s.to.y
                && !is_beam_stroke(s)
        })
        .map(|s| {
            (
                s.from.x.0,
                s.from.y.0.min(s.to.y.0),
                s.from.y.0.max(s.to.y.0),
            )
        })
        .collect();
    assert_eq!(stems.len(), 1, "one stem of {event:?}");
    stems[0]
}

/// The bottom and top lines of each staff of the first system, top staff
/// first.
fn staff_lines(layout: &ResolvedLayoutIR) -> Vec<(f32, f32)> {
    let mut ys: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .map(|s| s.from.y.0)
        .collect();
    ys.sort_by(|a, b| b.total_cmp(a));
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    ys.chunks(5)
        .take_while(|lines| lines.len() == 5)
        .map(|lines| (lines[4], lines[0]))
        .collect()
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} against {b}");
}

/// Alone in its staff, a staccato stands on the head's side in the first
/// space clear of it, an accent outside the staff clear of the head, a
/// marcato and an ornament above whatever the stem's side, past the stem's
/// end, and an up-bow above always; a scoop leads into its head from the
/// left.
#[test]
fn marks_stand_on_their_side_of_a_note_alone() {
    let measure = [
        note(
            "C5",
            2,
            "quarter",
            1,
            1,
            "<articulations><staccato/></articulations>",
        ),
        note(
            "E4",
            2,
            "quarter",
            1,
            1,
            "<articulations><accent/><scoop/></articulations>",
        ),
        note(
            "G4",
            2,
            "quarter",
            1,
            1,
            "<articulations><strong-accent/></articulations>",
        ),
        note("E4", 2, "quarter", 1, 1, "<technical><up-bow/></technical>"),
    ]
    .concat();
    let second = [
        note(
            "G4",
            2,
            "quarter",
            1,
            1,
            "<ornaments><trill-mark/></ornaments>",
        ),
        note("rest", 2, "quarter", 1, 1, ""),
        note("rest", 4, "half", 1, 1, ""),
    ]
    .concat();
    let loaded = part("marks_alone.musicxml", 1, &[measure, second]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let (bottom_line, top_line) = staff_lines(&layout)[0];

    // Over C5, stem down: centred in the space under the top line.
    let staccato = the_glyph(&layout, e[0], "articStaccatoAbove");
    near(
        (staccato[1] + staccato[3]) * 0.5,
        top_line - 0.5,
        "staccato's centre",
    );
    near(
        centre_x(staccato),
        centre_x(head(&loaded, &layout, e[0])),
        "staccato's x",
    );

    // Under E4, stem up: 0.4 under its head, which stands below the staff.
    let accent = the_glyph(&layout, e[1], "articAccentBelow");
    let e4 = head(&loaded, &layout, e[1]);
    near(accent[3], e4[1] - 0.4, "accent's top");
    assert!(accent[3] < bottom_line - 0.4 + 1e-3);
    let scoop = the_glyph(&layout, e[1], "brassScoop");
    near(scoop[2], e4[0] - 0.2, "scoop's right edge");

    // Over G4's up stem, 0.4 past its end.
    let marcato = the_glyph(&layout, e[2], "articMarcatoAbove");
    near(marcato[1], stem(&layout, e[2]).2 + 0.4, "marcato's bottom");

    // An up-bow over E4 too, though its head's side is below: outside the
    // staff, its up stem ending inside it.
    let bow = the_glyph(&layout, e[3], "stringsUpBow");
    assert!(stem(&layout, e[3]).2 < top_line);
    near(bow[1], top_line + 0.4, "up-bow's bottom");

    // A trill over G4's up stem, 0.4 past its end.
    let trill = the_glyph(&layout, e[4], "ornamentTrill");
    near(trill[1], stem(&layout, e[4]).2 + 0.4, "trill's bottom");
}

/// With another voice at the note, each voice's marks stand at its stem:
/// the upper voice's above and the lower's below; an ornament stands above
/// in the upper voice and below in the lower.
#[test]
fn with_another_voice_marks_stand_at_the_stem() {
    let measure = [
        note(
            "C5",
            4,
            "half",
            1,
            1,
            "<articulations><accent/></articulations>",
        ),
        note(
            "C5",
            4,
            "half",
            1,
            1,
            "<ornaments><trill-mark/></ornaments>",
        ),
        backup(8),
        note(
            "E4",
            4,
            "half",
            2,
            1,
            "<articulations><accent/></articulations>",
        ),
        note(
            "E4",
            4,
            "half",
            2,
            1,
            "<ornaments><trill-mark/></ornaments>",
        ),
    ]
    .concat();
    let loaded = part("marks_voices.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let (bottom_line, top_line) = staff_lines(&layout)[0];

    let upper = the_glyph(&layout, e[0], "articAccentAbove");
    near(
        upper[1],
        stem(&layout, e[0]).2 + 0.4,
        "upper accent's bottom",
    );
    let lower = the_glyph(&layout, e[2], "articAccentBelow");
    near(lower[3], stem(&layout, e[2]).1 - 0.4, "lower accent's top");

    let trill = the_glyph(&layout, e[1], "ornamentTrill");
    assert!(
        trill[1] > top_line,
        "the upper voice's trill above: {trill:?}"
    );
    let trill = the_glyph(&layout, e[3], "ornamentTrill");
    assert!(
        trill[3] < bottom_line,
        "the lower voice's trill below: {trill:?}"
    );
}

/// A fermata stands centred over its note or rest, half a space off the
/// staff or 0.4 clear of a note beyond it, and of the marks on its side;
/// an inverted one the same below.
#[test]
fn a_fermata_stands_off_the_staff_and_clear_of_its_note() {
    let measure = [
        note("E4", 2, "quarter", 1, 1, "<fermata/>"),
        note("A5", 2, "quarter", 1, 1, "<fermata/>"),
        note("E4", 2, "quarter", 1, 1, "<fermata type=\"inverted\"/>"),
        note(
            "C5",
            2,
            "quarter",
            1,
            1,
            "<articulations><accent/></articulations><fermata/>",
        ),
    ]
    .concat();
    let second = [
        note("rest", 4, "half", 1, 1, "<fermata/>"),
        note("rest", 4, "half", 1, 1, ""),
    ]
    .concat();
    let loaded = part("fermatas.musicxml", 1, &[measure, second]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let (bottom_line, top_line) = staff_lines(&layout)[0];

    let above = named(&layout, "fermataAbove");
    assert_eq!(above.len(), 4, "{above:?}");
    let low = above[0];
    near(low[1], top_line + 0.5, "over a note in the staff");
    near(
        centre_x(low),
        centre_x(head(&loaded, &layout, e[0])),
        "centred on its head",
    );

    let high = above[1];
    near(
        high[1],
        head(&loaded, &layout, e[1])[3] + 0.4,
        "over a note above the staff",
    );

    let below = named(&layout, "fermataBelow");
    assert_eq!(below.len(), 1);
    let e4 = head(&loaded, &layout, e[2]);
    near(
        below[0][3],
        (bottom_line - 0.5).min(e4[1] - 0.4),
        "under a note",
    );

    let accent = the_glyph(&layout, e[3], "articAccentAbove");
    near(above[2][1], accent[3] + 0.4, "over an accent");

    let rest = named(&layout, "restHalf");
    near(centre_x(above[3]), centre_x(rest[0]), "centred on its rest");
}

/// A breath mark stands half a space over the staff after its note, in a
/// column of its own a space clear of its note's head and of the next
/// note's, at any height, as MuseScore pads it; a caesura stands across
/// the staff's top line, a space clear of the next note's accidental.
#[test]
fn a_breath_mark_takes_a_column_after_its_note() {
    let measure = |mark: &str| {
        [
            note("C5", 2, "quarter", 1, 1, mark),
            note("D#5", 2, "quarter", 1, 1, ""),
            note("rest", 4, "half", 1, 1, ""),
        ]
        .concat()
    };
    let plain = part("no_breath.musicxml", 1, &[measure("")]);
    let breath = part(
        "breath.musicxml",
        1,
        &[measure("<articulations><breath-mark/></articulations>")],
    );
    let caesura = part(
        "caesura.musicxml",
        1,
        &[measure("<articulations><caesura/></articulations>")],
    );
    let accidental_left = |loaded: &Loaded, layout: &ResolvedLayoutIR| {
        let e = events(loaded);
        layout
            .glyphs
            .iter()
            .filter(|g| match &g.provenance.source {
                TypedObjectId::Event(id) => *id == e[1],
                TypedObjectId::Pitch(_) => g.glyph.as_str() == "accidentalSharp",
                _ => false,
            })
            .map(|g| glyph_box(g)[0])
            .fold(f32::INFINITY, f32::min)
    };

    let plain_layout = engrave(&plain.reduced.score).layout;
    let layout = engrave(&breath.reduced.score).layout;
    let e = events(&breath);
    let (_, top_line) = staff_lines(&layout)[0];
    let mark = named(&layout, "breathMarkComma")[0];
    near(mark[1], top_line + 0.5, "the breath mark's bottom");
    assert!(
        mark[0] >= head(&breath, &layout, e[0])[2] + 1.0 - 1e-3,
        "after its note"
    );
    let next = head(&breath, &layout, e[1]);
    assert!(
        next[0] >= mark[2] + 1.0 - 1e-3,
        "the next head {next:?} after {mark:?}"
    );
    assert!(
        head(&breath, &layout, e[1])[0] > head(&plain, &plain_layout, events(&plain)[1])[0] + 0.5,
        "the column widens the measure"
    );

    let layout = engrave(&caesura.reduced.score).layout;
    let (_, top_line) = staff_lines(&layout)[0];
    let mark = named(&layout, "caesura")[0];
    near((mark[1] + mark[3]) * 0.5, top_line, "the caesura's centre");
    let acc = accidental_left(&caesura, &layout);
    assert!(acc >= mark[2] + 1.0 - 1e-3, "{acc} after {mark:?}");
}

/// A chord visiting another staff sets its marks by the voices of the staff
/// it belongs to, as MuseScore's `Measure::hasVoices` reads that staff: an
/// accent on a note of the upper staff's voice drawn on the lower staff, its
/// stem up toward its home, stands under its head where the upper staff
/// shows no other voice meanwhile, and over its stem where it does, the
/// upper staff's second voice drawn on it or visiting the lower staff too.
#[test]
fn a_visiting_chords_marks_follow_its_own_staffs_voices() {
    let accent = "<articulations><accent/></articulations>";
    let measure = |company: &str| {
        [
            note("C5", 2, "quarter", 1, 1, ""),
            note("G3", 2, "quarter", 1, 2, accent),
            note("C5", 4, "half", 1, 1, ""),
            backup(8),
            note("C3", 8, "whole", 5, 2, ""),
            company.to_owned(),
        ]
        .concat()
    };
    // The second voice's half note starts with the visiting note, between
    // rests that touch it and no more.
    let beside = [
        backup(8),
        note("rest", 2, "quarter", 2, 1, ""),
        note("E4", 4, "half", 2, 1, ""),
        note("rest", 2, "quarter", 2, 1, ""),
    ]
    .concat();
    let elsewhere = [backup(8), note("E3", 8, "whole", 2, 2, "")].concat();
    let loaded = part(
        "visiting_marks.musicxml",
        2,
        &[measure(""), measure(&beside), measure(&elsewhere)],
    );
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);

    let (x, _, _) = stem(&layout, e[1]);
    let h = head(&loaded, &layout, e[1]);
    assert!(x >= h[2] - 0.1, "its stem up, toward its home");
    let below = the_glyph(&layout, e[1], "articAccentBelow");
    assert!(
        below[3] <= h[1],
        "alone at home: under its head {below:?} {h:?}"
    );

    for k in [5, 12] {
        let (_, _, high) = stem(&layout, e[k]);
        let above = the_glyph(&layout, e[k], "articAccentAbove");
        assert!(
            above[1] >= high,
            "with company: over its stem {above:?} {high}"
        );
    }
}

/// A tuplet whose ends share a staff stands out of it, as MuseScore's
/// `tupletOutOfStaff` sets it: its bracket's line, where its number is
/// centred, at least 1.25 outside the staff on its side (the bracket's ends
/// 0.5 out and its hooks 0.75 long there), bracketed or not, though its
/// notes' ink would let it stand inside: over low notes stemmed up and over
/// rests, and under high notes stemmed down.
#[test]
fn a_tuplet_on_one_staff_stands_out_of_it() {
    let triplet = |pitch: &str, kind: &str, beam: &str| {
        let tuplet = match kind {
            "start" | "stop" => format!("<notations><tuplet type=\"{kind}\"/></notations>"),
            _ => String::new(),
        };
        let beam = if beam.is_empty() {
            String::new()
        } else {
            format!("<beam number=\"1\">{beam}</beam>")
        };
        note(pitch, 4, "eighth", 1, 1, "").replacen(
            "</staff>",
            &format!(
                "</staff>{beam}<time-modification><actual-notes>3</actual-notes>\
                 <normal-notes>2</normal-notes></time-modification>{tuplet}"
            ),
            1,
        )
    };
    let measure = [
        triplet("C4", "start", "begin"),
        triplet("D4", "", "continue"),
        triplet("C4", "stop", "end"),
        triplet("rest", "start", ""),
        triplet("rest", "", ""),
        triplet("rest", "stop", ""),
        triplet("A5", "start", "begin"),
        triplet("G5", "", "continue"),
        triplet("F5", "stop", "end"),
        note("C5", 12, "quarter", 1, 1, ""),
    ]
    .concat();
    let loaded = part_in("tuplets_out_of_staff.musicxml", 1, 12, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let (bottom, top) = staff_lines(&layout)[0];
    let numbers = named(&layout, "tuplet3");
    assert_eq!(numbers.len(), 3, "three numbers");
    let centre = |ink: Ink| (ink[1] + ink[3]) * 0.5;
    // A digit's ink dips 0.03 under its baseline, so its middle stands that
    // much under the line it is centred on.
    assert!(
        centre(numbers[0]) >= top + 1.25 - 0.05,
        "{numbers:?} over {top}"
    );
    assert!(
        centre(numbers[1]) >= top + 1.25 - 0.05,
        "{numbers:?} over {top}"
    );
    assert!(
        centre(numbers[2]) <= bottom - 1.25 + 0.05,
        "{numbers:?} under {bottom}"
    );
    // The one bracket, over the rests' triplet, on the number's line.
    let line: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, TypedObjectId::Tuplet(_))
                && s.from.y == s.to.y
                && s.from.x != s.to.x
        })
        .map(|s| s.from.y.0)
        .collect();
    assert_eq!(line.len(), 2, "a bracket broken by its number");
    for y in line {
        assert!(y >= top + 1.25 - 0.01, "the bracket's line {y} over {top}");
        assert!(
            (y - centre(numbers[1])).abs() < 0.05,
            "the number centred on its bracket's line: {y}, {numbers:?}"
        );
    }
}

/// An arpeggio is a run of turned wiggles left of its chord, 0.4 clear of
/// its heads, reaching from its lowest head to its highest, and takes room
/// as an accidental does; every other mark takes none, so the heads stand
/// where they stand without them.
#[test]
fn an_arpeggio_takes_room_and_other_marks_take_none() {
    let measure = |arpeggio: &str, marks: &str| {
        [
            note("C4", 2, "quarter", 1, 1, arpeggio),
            chord("E4", 2, "quarter", 1, 1, arpeggio),
            chord("G4", 2, "quarter", 1, 1, arpeggio),
            note("C5", 2, "quarter", 1, 1, marks),
            note("E5", 2, "quarter", 1, 1, marks),
            note("G4", 2, "quarter", 1, 1, marks),
        ]
        .concat()
    };
    let marks = "<articulations><accent/><staccato/><tenuto/></articulations><fermata/>";
    let plain = part("no_arpeggio.musicxml", 1, &[measure("", "")]);
    let marked = part("marked.musicxml", 1, &[measure("", marks)]);
    let rolled = part("arpeggio.musicxml", 1, &[measure("<arpeggiate/>", "")]);
    let x_of = |loaded: &Loaded| -> Vec<f32> {
        let layout = engrave(&loaded.reduced.score).layout;
        events(loaded)
            .iter()
            .filter_map(|e| heads(loaded, &layout, *e).first().map(|h| h[0]))
            .collect()
    };
    let plain_x = x_of(&plain);
    assert_eq!(plain_x, x_of(&marked), "marks take no room");

    let layout = engrave(&rolled.reduced.score).layout;
    let e = events(&rolled);
    let chord_heads = heads(&rolled, &layout, e[0]);
    let runs: Vec<Ink> = layout
        .texts
        .iter()
        .filter(|t| t.provenance.source == TypedObjectId::Event(e[0]))
        .map(text_box)
        .collect();
    assert_eq!(runs.len(), 1, "one arpeggio");
    let run = runs[0];
    let left = chord_heads
        .iter()
        .map(|h| h[0])
        .fold(f32::INFINITY, f32::min);
    assert!(
        run[2] <= left - 0.4 + 0.01,
        "left of its heads: {run:?}, {left}"
    );
    let (low, high) = (chord_heads[0], chord_heads[2]);
    assert!(run[1] <= (low[1] + low[3]) * 0.5 && run[3] >= (high[1] + high[3]) * 0.5);
    // At its measure's start, the chord stands right of the time signature
    // by the arpeggio's room as well.
    let rolled_x = x_of(&rolled);
    assert!(
        rolled_x[0] > plain_x[0] + (run[2] - run[0]) * 0.5,
        "room for it: {rolled_x:?} against {plain_x:?}"
    );
}

/// A one-note tremolo's strokes stand on its stem between head and tip.
#[test]
fn tremolo_strokes_stand_on_their_stems() {
    let measure = [
        note(
            "G4",
            2,
            "quarter",
            1,
            1,
            "<ornaments><tremolo type=\"single\">3</tremolo></ornaments>",
        ),
        note("rest", 6, "half", 1, 1, ""),
    ]
    .concat()
    .replacen("<type>half</type>", "<type>half</type><dot/>", 1);
    let loaded = part("tremolos.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);

    let strokes = the_glyph(&layout, e[0], "tremolo3");
    let (x, _, high) = stem(&layout, e[0]);
    near(centre_x(strokes), x, "on its stem");
    assert!(
        strokes[1] > head(&loaded, &layout, e[0])[3] && strokes[3] < high,
        "{strokes:?}"
    );
}

/// A stem carrying a one-note tremolo is as long as it must be to hold it,
/// as MuseScore's `StemLayout::calcMinStemLength` counts it in quarter
/// spaces from the head nearest its end: the strokes' height, half a space
/// at the stem's end and 1.25 at the head. Three strokes (Bravura's 2.23
/// tall, nine quarters, as MuseScore's own three) take a quarter's stem from
/// 3.5 to 4.0 and four (twelve quarters) to 4.75, while two leave it; a head
/// two spaces outside the staff on the stem's side keeps its strokes two
/// spaces from it, its stem 4.75; a flag adds its height less its curl's
/// cutout, six quarters up and four down for one flag, eight up for two;
/// and a beamed eighth asks the beam to stand 4.5 out, its strokes inside
/// the beam.
#[test]
fn a_one_note_tremolo_lengthens_its_stem() {
    let trem = |n: u8| format!("<ornaments><tremolo type=\"single\">{n}</tremolo></ornaments>");
    let beam = |note: String, kind: &str| {
        note.replacen(
            "</staff>",
            &format!("</staff><beam number=\"1\">{kind}</beam>"),
            1,
        )
    };
    let first = [
        note("G4", 4, "quarter", 1, 1, ""),
        note("G4", 4, "quarter", 1, 1, &trem(2)),
        note("G4", 4, "quarter", 1, 1, &trem(3)),
        note("G4", 4, "quarter", 1, 1, &trem(4)),
    ]
    .concat();
    let second = [
        note("A3", 4, "quarter", 1, 1, &trem(3)),
        note("G4", 2, "eighth", 1, 1, &trem(3)),
        note("rest", 2, "eighth", 1, 1, ""),
        beam(note("G4", 2, "eighth", 1, 1, &trem(3)), "begin"),
        beam(note("G4", 2, "eighth", 1, 1, ""), "end"),
        note("C6", 4, "quarter", 1, 1, &trem(3)),
    ]
    .concat();
    let third = [
        note("G4", 1, "16th", 1, 1, &trem(3)),
        note("rest", 1, "16th", 1, 1, ""),
        note("rest", 2, "eighth", 1, 1, ""),
        note("B4", 2, "eighth", 1, 1, &trem(3)),
        note("rest", 2, "eighth", 1, 1, ""),
        note("rest", 8, "half", 1, 1, ""),
    ]
    .concat();
    let loaded = part_in("one_note_tremolos.musicxml", 1, 4, &[first, second, third]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    // From the head's centre to the stem's far end, whichever way it turns.
    let length = |event: EventId| {
        let h = head(&loaded, &layout, event);
        let centre = (h[1] + h[3]) * 0.5;
        let (_, low, high) = stem(&layout, event);
        (high - centre).max(centre - low)
    };
    near(length(e[0]), 3.5, "a quarter's stem");
    near(length(e[1]), 3.5, "two strokes fit it");
    near(length(e[2]), 4.0, "three strokes lengthen it");
    near(length(e[3]), 4.75, "four strokes more");
    near(length(e[4]), 4.75, "two spaces under the staff");
    near(length(e[5]), 5.5, "past an eighth's flag");
    // A beamed stem ends a stem's thickness inside the beam's outer edge.
    near(length(e[7]), 4.5 - 0.12, "a beam stands out for it");
    near(length(e[8]), 4.5 - 0.12, "and its fellow reaches it");
    near(length(e[9]), 4.75, "two spaces over the staff");
    near(length(e[10]), 6.0, "past a sixteenth's flags");
    near(length(e[13]), 5.0, "past an eighth's flag turned down");
    // The strokes stand clear of their heads by 1.25 or more.
    for event in [e[2], e[5], e[10]] {
        let strokes = the_glyph(&layout, event, "tremolo3");
        let h = head(&loaded, &layout, event);
        assert!(
            strokes[1] >= (h[1] + h[3]) * 0.5 + 1.25 - 0.01,
            "{strokes:?} over {h:?}"
        );
    }
    let strokes = the_glyph(&layout, e[3], "tremolo4");
    let h = head(&loaded, &layout, e[3]);
    assert!(strokes[1] >= (h[1] + h[3]) * 0.5 + 1.25 - 0.01);
}

/// The strokes a two-note tremolo `event` starts, each `(from, to)` as
/// `(x, y)` along its centre, the outermost first by its height on `up`.
fn tremolo_lines(layout: &ResolvedLayoutIR, event: EventId, up: bool) -> Vec<Leg> {
    let mut found: Vec<Leg> = layout
        .strokes
        .iter()
        .filter(|s| s.provenance.source == TypedObjectId::Event(event) && s.from.x != s.to.x)
        .map(|s| ((s.from.x.0, s.from.y.0), (s.to.x.0, s.to.y.0)))
        .collect();
    found.sort_by(|a, b| {
        let (a, b) = (a.0 .1, b.0 .1);
        if up {
            b.total_cmp(&a)
        } else {
            a.total_cmp(&b)
        }
    });
    found
}

/// A two-note tremolo is laid out as a beam of its two notes, as MuseScore
/// lays it out: both stems turn the way a beam group's would and reach the
/// outermost stroke, which runs along their ends from a space inside the
/// first stem to a space inside the second, each further stroke 0.75 nearer
/// the heads; the stems are as long as a beam of that many levels asks. Two
/// whole notes' strokes stand where their stems would.
#[test]
fn a_two_note_tremolo_is_a_beam_inset_from_its_stems() {
    let start = "<ornaments><tremolo type=\"start\">3</tremolo></ornaments>";
    let stop = "<ornaments><tremolo type=\"stop\">3</tremolo></ornaments>";
    let halves = [
        note("G4", 4, "half", 1, 1, start),
        note("B4", 4, "half", 1, 1, stop),
    ]
    .concat();
    let low = [
        note("C6", 4, "half", 1, 1, start),
        note("A5", 4, "half", 1, 1, stop),
    ]
    .concat();
    let wholes = [
        note("F4", 8, "whole", 1, 1, start),
        note("A4", 8, "whole", 1, 1, stop),
    ];
    let loaded = part(
        "two_note_tremolos.musicxml",
        1,
        &[halves, low, wholes[0].clone(), wholes[1].clone()],
    );
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);

    for (pair, up) in [((e[0], e[1]), true), ((e[2], e[3]), false)] {
        let lines = tremolo_lines(&layout, pair.0, up);
        assert_eq!(lines.len(), 3, "three strokes");
        let sign = if up { 1.0 } else { -1.0 };
        let (a, a_low, a_high) = stem(&layout, pair.0);
        let (b, b_low, b_high) = stem(&layout, pair.1);
        // The stems turn one way, by the heads furthest from the middle line.
        for (event, x) in [(pair.0, a), (pair.1, b)] {
            let h = head(&loaded, &layout, event);
            if up {
                assert!(x >= h[2] - 0.1, "an up stem right of its head");
            } else {
                assert!(x <= h[0] + 0.1, "a down stem left of its head");
            }
        }
        let ((x0, y0), (x1, y1)) = lines[0];
        near(x0, a - 0.06 + 1.0, "a space inside the first stem");
        near(x1, b + 0.06 - 1.0, "a space inside the second");
        let along = |x: f32| y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        // The stems reach the outermost stroke's outer edge, within the tenth
        // of a space the strokes' slope moves by once spacing sets their ends
        // apart otherwise than the projection did.
        let (a_end, b_end) = if up { (a_high, b_high) } else { (a_low, b_low) };
        let reaches = |end: f32, x: f32, what: &str| {
            let edge = along(x) + sign * 0.25;
            assert!((end - edge).abs() < 0.1, "{what}: {end} against {edge}");
        };
        reaches(a_end, a, "the first stem reaches the stroke");
        reaches(b_end, b, "the second stem reaches it");
        // As long as a beam of three levels asks: 4.25 from the head
        // nearest the strokes, or more.
        let (a_base, b_base) = if up { (a_low, b_low) } else { (a_high, b_high) };
        assert!(
            (a_end - a_base).abs() >= 4.25 - 0.01,
            "{a_end} from {a_base}"
        );
        assert!(
            (b_end - b_base).abs() >= 4.25 - 0.01,
            "{b_end} from {b_base}"
        );
        for (k, line) in lines.iter().enumerate().skip(1) {
            near(line.0 .0, x0, "every stroke starts as the first");
            near(line.1 .0, x1, "and ends as it");
            near(
                line.0 .1,
                y0 - sign * 0.75 * k as f32,
                "a step nearer the heads",
            );
            near(line.1 .1, y1 - sign * 0.75 * k as f32, "at both ends");
        }
    }

    // Two whole notes: no stems, the strokes standing where an up stem
    // would end, a space inside where each would stand.
    let lines = tremolo_lines(&layout, e[4], true);
    assert_eq!(lines.len(), 3, "three strokes");
    let (first, second) = (head(&loaded, &layout, e[4]), head(&loaded, &layout, e[5]));
    let ((x0, y0), (x1, y1)) = lines[0];
    near(
        x0,
        first[2] - 0.06 + 1.0,
        "a space inside the first head's right",
    );
    near(x1, second[2] + 0.06 - 1.0, "a space inside the second's");
    let first_y = (first[1] + first[3]) * 0.5;
    assert!(
        y0 + 0.25 >= first_y + 4.25 - 0.01 && y1 > y0 - 1.0,
        "{y0} over {first_y}"
    );
}

/// A mark past the end of a stem meeting a beam across two staves stands
/// past the beam, wherever the staves come to stand: an up-bow over a note
/// of the lower staff whose stem rises to the beam, in a system whose next
/// measure holds the staves further apart than the beam alone would. Like
/// the beam, it is the ink of neither staff: alone in its system, the
/// staves stand as far apart with it as without it.
#[test]
fn a_mark_past_a_beam_across_two_staves_stands_past_it() {
    let beamed = |pitch: &str, staff: u8, beam: &str, notations: &str| {
        note(pitch, 1, "eighth", 5, staff, notations).replacen(
            "</staff>",
            &format!("</staff><beam number=\"1\">{beam}</beam>"),
            1,
        )
    };
    let measure = |bow: &str| {
        [
            note("rest", 8, "whole", 1, 1, ""),
            backup(8),
            beamed("A2", 2, "begin", bow),
            beamed("C5", 1, "continue", ""),
            beamed("A2", 2, "continue", ""),
            beamed("C5", 1, "end", ""),
            note("rest", 4, "half", 5, 2, ""),
        ]
        .concat()
    };
    let gap = |bow: &str, name: &str| {
        let layout = engrave(&part(name, 2, &[measure(bow)]).reduced.score).layout;
        let lines = staff_lines(&layout);
        lines[0].0 - lines[1].1
    };
    let up_bow = "<technical><up-bow/></technical>";
    assert_eq!(
        gap(up_bow, "bow_between.musicxml"),
        gap("", "no_bow_between.musicxml"),
        "the staves' gap"
    );
    let first = measure(up_bow);
    let second = [
        note("C3", 8, "whole", 1, 1, ""),
        backup(8),
        note("C5", 8, "whole", 5, 2, ""),
    ]
    .concat();
    let loaded = part("mark_past_a_cross_beam.musicxml", 2, &[first, second]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let bow = the_glyph(&layout, e[1], "stringsUpBow");
    // Where its stem meets the beam.
    let x = stem(&layout, e[1]).0;
    let beam_top = layout
        .strokes
        .iter()
        .filter(|s| is_beam_stroke(s) && s.from.x.0 <= x && s.to.x.0 >= x)
        .map(|s| {
            let t = (x - s.from.x.0) / (s.to.x.0 - s.from.x.0);
            s.from.y.0 + (s.to.y.0 - s.from.y.0) * t + s.thickness.0 * 0.5
        })
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(beam_top.is_finite(), "a beam over the up-bow");
    near(bow[1], beam_top + 0.4, "the up-bow's bottom, past the beam");
}

/// A fermata over a note above the lower staff of two is its staff's ink:
/// the staves stand apart so that it clears the upper staff's.
#[test]
fn a_fermata_between_two_staves_holds_them_apart() {
    let measure = |fermata: &str| {
        [
            note("C4", 8, "whole", 1, 1, ""),
            backup(8),
            note("E4", 8, "whole", 5, 2, fermata),
        ]
        .concat()
    };
    let gap = |loaded: &Loaded| {
        let layout = engrave(&loaded.reduced.score).layout;
        let lines = staff_lines(&layout);
        (lines[0].0 - lines[1].1, layout)
    };
    let (plain, _) = gap(&part("no_fermata_between.musicxml", 2, &[measure("")]));
    let loaded = part("fermata_between.musicxml", 2, &[measure("<fermata/>")]);
    let (held, layout) = gap(&loaded);
    let e = events(&loaded);
    let fermata = named(&layout, "fermataAbove")[0];
    let upper = head(&loaded, &layout, e[0]);
    assert!(
        held > plain,
        "the staves move apart: {held} against {plain}"
    );
    assert!(
        fermata[3] < upper[1],
        "clear of the upper staff's C4: {fermata:?}, {upper:?}"
    );
}

/// A breath mark keeps a space from the heads before and after it, though
/// it stands over the staff and they in it, their stems turned down, and
/// from a ledger line beside it: at its measure's natural width and set
/// toward its floor alike, sixteen sixteenths and sixty-four sixty-fourths,
/// whose springs are shorter than that, in the staff or over it on ledger
/// lines, a breath mark after every other note, the file breaking the
/// system after them, on pages from one too narrow for their floor, where
/// they stand at their natural width, to one that holds them so.
#[test]
fn a_breath_mark_keeps_its_padding_at_the_floor() {
    for (count, kind, divisions, octave, pages) in [
        (16, "16th", 4, 5, 60..=80),
        (64, "64th", 16, 5, 80..=80),
        (64, "64th", 16, 6, 80..=80),
    ] {
        let first: String = (0..count)
            .map(|k| {
                let mark = if k % 2 == 0 {
                    "<notations><articulations><breath-mark/></articulations></notations>"
                } else {
                    ""
                };
                let step = ["C", "D"][k % 2];
                format!(
                    "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
                     <duration>1</duration><voice>1</voice><type>{kind}</type>{mark}</note>"
                )
            })
            .collect();
        let second = format!(
            "<print new-system=\"yes\"/><note><pitch><step>C</step><octave>5</octave>\
             </pitch><duration>{}</duration><voice>1</voice><type>whole</type></note>",
            divisions * 4
        );
        let mut spans = Vec::new();
        for width in pages.map(|w| w as f32) {
            let xml = format!(
                "<score-partwise version=\"4.0\"><defaults><scaling><millimeters>7\
                 </millimeters><tenths>40</tenths></scaling><page-layout><page-height>3000\
                 </page-height><page-width>{}</page-width><page-margins type=\"both\">\
                 <left-margin>75</left-margin><right-margin>75</right-margin><top-margin>75\
                 </top-margin><bottom-margin>75</bottom-margin></page-margins></page-layout>\
                 </defaults><part-list><score-part id=\"P1\"><part-name>A</part-name>\
                 </score-part></part-list><part id=\"P1\"><measure number=\"1\"><attributes>\
                 <divisions>{}</divisions><time><beats>4</beats><beat-type>4</beat-type></time>\
                 <clef><sign>G</sign><line>2</line></clef></attributes>{first}</measure>\
                 <measure number=\"2\">{second}</measure></part></score-partwise>",
                width * 10.0,
                divisions
            );
            let name = format!("breaths_{count}_{octave}_{width}.musicxml");
            let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
            std::fs::write(&path, xml).expect("written");
            let loaded = load(&path).expect("loads");
            let layout = epiphany_cli::engrave_loaded(&loaded).layout;
            let e = events(&loaded);
            let marks = named(&layout, "breathMarkComma");
            assert_eq!(marks.len(), count / 2);
            for (k, mark) in marks.iter().enumerate() {
                // The note before: its head, and its ledger lines at the
                // mark's height.
                let before = head(&loaded, &layout, e[2 * k]);
                let before_right = ledgers(&loaded, &layout, e[2 * k])
                    .into_iter()
                    .filter(|l| l[3] > mark[1] - 0.2 && l[1] < mark[3] + 0.2)
                    .map(|l| l[2])
                    .fold(before[2], f32::max);
                let after = head(&loaded, &layout, e[2 * k + 1]);
                let at = format!("{count} at octave {octave} on {width}, mark {k}");
                assert!(
                    mark[0] >= before_right + 1.0 - 1e-3,
                    "{at}: {before:?} to {before_right} then {mark:?}"
                );
                assert!(
                    after[0] >= mark[2] + 1.0 - 1e-3,
                    "{at}: {mark:?} then {after:?}"
                );
            }
            spans.push(head(&loaded, &layout, e[count - 1])[2] - head(&loaded, &layout, e[0])[0]);
        }
        // The narrowest page leaves the measure at its natural width; for the
        // sixteenths some wider one sets it tighter, toward its floor.
        if count == 16 {
            assert!(spans.iter().any(|&s| s < spans[0] - 1.0), "{spans:?}");
        }
    }
}

/// Set toward its floor too, a note's marks take no room: thirty-two
/// thirty-seconds, each with an accent and a staccato, the file breaking the
/// system after them, stand where the same notes without marks do, on every
/// page from one that compresses them hard to one that holds them whole.
#[test]
fn marks_take_no_room_at_the_floor() {
    let page = |marks: &str, width: f32| {
        let first: String = (0..32)
            .map(|k| {
                let (step, octave) = [("C", 5), ("G", 4)][k % 2];
                format!(
                    "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
                     <duration>1</duration><voice>1</voice><type>32nd</type>{marks}</note>"
                )
            })
            .collect();
        format!(
            "<score-partwise version=\"4.0\"><defaults><scaling><millimeters>7</millimeters>\
             <tenths>40</tenths></scaling><page-layout><page-height>3000</page-height>\
             <page-width>{}</page-width><page-margins type=\"both\"><left-margin>75\
             </left-margin><right-margin>75</right-margin><top-margin>75</top-margin>\
             <bottom-margin>75</bottom-margin></page-margins></page-layout></defaults>\
             <part-list><score-part id=\"P1\"><part-name>A</part-name></score-part>\
             </part-list><part id=\"P1\"><measure number=\"1\"><attributes><divisions>8\
             </divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G\
             </sign><line>2</line></clef></attributes>{first}</measure><measure number=\"2\">\
             <print new-system=\"yes\"/><note><pitch><step>C</step><octave>5</octave></pitch>\
             <duration>32</duration><voice>1</voice><type>whole</type></note></measure></part>\
             </score-partwise>",
            width * 10.0
        )
    };
    let marked = "<notations><articulations><accent/><staccato/></articulations></notations>";
    let mut spans = Vec::new();
    for width in (40..=100).step_by(4).map(|w| w as f32) {
        let heads_x = |marks: &str, name: &str| -> Vec<f32> {
            let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
            std::fs::write(&path, page(marks, width)).expect("written");
            let loaded = load(&path).expect("loads");
            let layout = epiphany_cli::engrave_loaded(&loaded).layout;
            events(&loaded)
                .iter()
                .filter_map(|e| heads(&loaded, &layout, *e).first().map(|h| h[0]))
                .collect()
        };
        let plain = heads_x("", &format!("plain_{width}.musicxml"));
        let with = heads_x(marked, &format!("marked_{width}.musicxml"));
        assert_eq!(plain, with, "on a page {width} wide");
        spans.push(plain[31] - plain[0]);
    }
    assert!(
        spans.iter().any(|&s| s < spans[0] - 1.0) || spans.iter().any(|&s| s > spans[0] + 1.0),
        "some page sets them otherwise than another: {spans:?}"
    );
}

/// A one-note tremolo and a chord line take room as their chord's ink, as
/// MuseScore's `ChordLayout::fillShape` adds them to its shape, and keep
/// its padding (0.35) from ink beside them at their height: set toward its
/// floor on pages from one too narrow for it to one that holds it, a note
/// after a doit stands clear of the doit, a rest after a tremolo clear of
/// the tremolo, and a scoop clear of the note before it, where on some such
/// page the same notes without them stand closer. At ease on a wide page, a
/// doit or a scoop moves no note: its column keeps its head's place.
#[test]
fn a_tremolo_and_a_chord_line_take_room() {
    let doit = "<notations><articulations><doit/></articulations></notations>";
    let tremolo =
        "<notations><ornaments><tremolo type=\"single\">3</tremolo></ornaments></notations>";
    let scoop = "<notations><articulations><scoop/></articulations></notations>";
    let page = |marks: [&str; 3], width: f32| {
        let [doit, tremolo, scoop] = marks;
        let quarter = |marks: &str| {
            format!(
                "<note><pitch><step>G</step><octave>4</octave></pitch><duration>2</duration>\
                 <voice>1</voice><type>quarter</type>{marks}</note>"
            )
        };
        let rest = "<note><rest/><duration>2</duration><voice>1</voice><type>quarter</type></note>";
        let first = [
            quarter(doit),
            quarter(""),
            quarter(tremolo),
            rest.to_owned(),
            quarter(""),
            quarter(scoop),
        ]
        .concat();
        format!(
            "<score-partwise version=\"4.0\"><defaults><scaling><millimeters>7</millimeters>\
             <tenths>40</tenths></scaling><page-layout><page-height>3000</page-height>\
             <page-width>{}</page-width><page-margins type=\"both\"><left-margin>75\
             </left-margin><right-margin>75</right-margin><top-margin>75</top-margin>\
             <bottom-margin>75</bottom-margin></page-margins></page-layout></defaults>\
             <part-list><score-part id=\"P1\"><part-name>A</part-name></score-part>\
             </part-list><part id=\"P1\"><measure number=\"1\"><attributes><divisions>2\
             </divisions><time><beats>6</beats><beat-type>4</beat-type></time><clef><sign>G\
             </sign><line>2</line></clef></attributes>{first}</measure><measure number=\"2\">\
             <print new-system=\"yes\"/><note><rest measure=\"yes\"/><duration>12</duration>\
             <voice>1</voice></note></measure></part></score-partwise>",
            width * 10.0
        )
    };
    let engraved = |marks: [&str; 3], width: f32| {
        let kind: String = marks.iter().map(|m| m.len().to_string()).collect();
        let name = format!("quarters_{kind}_{width}.musicxml");
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::write(&path, page(marks, width)).expect("written");
        let loaded = load(&path).expect("loads");
        let layout = epiphany_cli::engrave_loaded(&loaded).layout;
        (loaded, layout)
    };
    // How far each mark's next ink stands clear of it: the note after the
    // doit, the rest after the tremolo, the scoop after the note before
    // it; in the plain file, of where the mark would stand by its note.
    let clearances = |marks: [&str; 3], width: f32| -> [f32; 3] {
        let (loaded, layout) = engraved(marks, width);
        let e = events(&loaded);
        let head_of = |k: usize| head(&loaded, &layout, e[k]);
        let rest = named(&layout, "restQuarter");
        let (stem_x, _, _) = stem(&layout, e[2]);
        // Each mark where the file draws it; else where it would stand: a
        // doit's left edge 0.2 right of its head, a tremolo centred on its
        // stem, a scoop's right edge 0.2 left of its head, Bravura's doit
        // and scoop 1.28 wide and its three strokes 1.2.
        let drawn = |name: &str| named(&layout, name).first().copied();
        let doit_right = drawn("brassDoitShort").map_or(head_of(0)[2] + 0.2 + 1.28, |g| g[2]);
        let tremolo_right = drawn("tremolo3").map_or(stem_x + 0.6, |g| g[2]);
        let scoop_left = drawn("brassScoop").map_or(head_of(5)[0] - 0.2 - 1.28, |g| g[0]);
        [
            head_of(1)[0] - doit_right,
            rest[0][0] - tremolo_right,
            scoop_left - head_of(4)[2],
        ]
    };
    let mut plain_closer = [false; 3];
    for width in (24..=48).map(|w| w as f32) {
        for (k, marks) in [[doit, "", ""], ["", tremolo, ""], ["", "", scoop]]
            .into_iter()
            .enumerate()
        {
            let clear = clearances(marks, width)[k];
            assert!(
                clear >= 0.35 - 0.02,
                "{width}: mark {k} stands {clear} from the ink beside it"
            );
            if clearances(["", "", ""], width)[k] < 0.35 - 0.05 {
                plain_closer[k] = true;
            }
        }
    }
    assert_eq!(
        plain_closer, [true; 3],
        "some page sets the plain notes closer"
    );
    let heads_x = |marks: [&str; 3]| -> Vec<f32> {
        let (loaded, layout) = engraved(marks, 120.0);
        let e = events(&loaded);
        [0, 1, 2, 4, 5]
            .iter()
            .map(|&k| head(&loaded, &layout, e[k])[0])
            .collect()
    };
    let plain = heads_x(["", "", ""]);
    assert_eq!(plain, heads_x([doit, "", ""]), "a doit moves no note");
    assert_eq!(plain, heads_x(["", "", scoop]), "a scoop moves no note");
}

/// A fermata is placed once its system is set, after the slurs, as
/// MuseScore lays it out: over a note under a slur's arc it rises until it
/// clears the slur by its distance, though the note alone would hold it
/// lower.
#[test]
fn a_fermata_clears_the_slur_over_its_note() {
    let slurred = |notations: &str, slur: &str| {
        let both = format!("{notations}{slur}");
        note("C5", 2, "quarter", 1, 1, &both)
    };
    let measure = [
        slurred(
            "",
            "<slur type=\"start\" number=\"1\" placement=\"above\"/>",
        ),
        note("D5", 2, "quarter", 1, 1, "<fermata/>"),
        note("D5", 2, "quarter", 1, 1, ""),
        slurred("", "<slur type=\"stop\" number=\"1\"/>"),
    ]
    .concat();
    let loaded = part("fermata_under_a_slur.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let fermata = named(&layout, "fermataAbove")[0];
    let slur = layout
        .curves
        .iter()
        .find(|c| matches!(c.provenance.source, TypedObjectId::Slur(_)))
        .expect("the slur");
    let cp = slur.control_points();
    // The slur's highest point over the fermata's width.
    let mut top = f32::NEG_INFINITY;
    for k in 0..=64 {
        let t = k as f32 / 64.0;
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let x: f32 = (0..4).map(|i| w[i] * cp[i].x.0).sum();
        let y: f32 = (0..4).map(|i| w[i] * cp[i].y.0).sum();
        if x >= fermata[0] && x <= fermata[2] {
            top = top.max(y + slur.thickness.0 * 0.5);
        }
    }
    assert!(top.is_finite(), "the slur passes over the fermata's note");
    let (_, top_line) = staff_lines(&layout)[0];
    assert!(
        top > top_line + 0.5,
        "the slur stands where the fermata would: {top}"
    );
    assert!(
        fermata[1] >= top + 0.4 - 0.01,
        "{fermata:?} over the slur at {top}"
    );
}

/// A dynamic stands below its staff, its optical centre on its note's head,
/// its baseline two spaces under the staff, and further down where ink there
/// needs it, half a space clear: under a G with its stem up, and under a low
/// C on a ledger line.
#[test]
fn a_dynamic_stands_below_its_note_centred_on_its_head() {
    let measure = [
        dynamic("f", 1),
        note("G4", 2, "quarter", 1, 1, ""),
        dynamic("pp", 1),
        note("C4", 2, "quarter", 1, 1, ""),
        note("rest", 4, "half", 1, 1, ""),
    ]
    .concat();
    let loaded = part("dynamics.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let (bottom_line, _) = staff_lines(&layout)[0];
    let forte = only(&layout, "dynamicForte");
    near(
        forte.position.x.0 + 1.256,
        centre_x(head(&loaded, &layout, e[0])),
        "f's optical centre",
    );
    // Two spaces under the staff, or lower, its top half a space under the
    // bottom line's ink.
    let rise = named(&layout, "dynamicForte")[0][3] - forte.position.y.0;
    let clear = bottom_line - 0.065 - 0.5 - rise;
    near(
        forte.position.y.0,
        (bottom_line - 2.0).min(clear),
        "f's baseline",
    );
    let pp = only(&layout, "dynamicPP");
    near(
        pp.position.x.0 + 1.708,
        centre_x(head(&loaded, &layout, e[1])),
        "pp's optical centre",
    );
    let below = head(&loaded, &layout, e[1])[1];
    let pp_ink = named(&layout, "dynamicPP")[0];
    assert!(
        pp_ink[3] <= below - 0.5 + 0.01,
        "clear of the low C: {pp_ink:?} under {below}"
    );
    assert!(
        pp.position.y.0 < bottom_line - 2.0 - 0.1,
        "pushed down: {}",
        pp.position.y.0
    );
}

/// A staff most of whose notes carry a syllable takes its dynamics and
/// hairpins above; one with fewer, below (D79's Q10).
#[test]
fn a_voices_staff_takes_its_dynamics_above() {
    let measure = |syllables: usize| {
        let mut notes = vec![dynamic("p", 1), wedge("diminuendo", 1)];
        for k in 0..4 {
            let n = note("A4", 2, "quarter", 1, 1, "");
            notes.push(if k < syllables { sung(n) } else { n });
            if k == 1 {
                notes.push(wedge("stop", 1));
            }
        }
        notes.concat()
    };
    for (syllables, above) in [(3, true), (2, false)] {
        let loaded = part(
            &format!("vocal_{syllables}.musicxml"),
            1,
            &[measure(syllables)],
        );
        let layout = engrave(&loaded.reduced.score).layout;
        let (bottom_line, top_line) = staff_lines(&layout)[0];
        let piano = only(&layout, "dynamicPiano");
        let axis = hairpin_legs(&layout)[0].1 .1;
        if above {
            assert!(
                piano.position.y.0 >= top_line + 1.0 - 0.01,
                "{}",
                piano.position.y.0
            );
            assert!(axis >= top_line + 1.75 - 0.01, "{axis}");
        } else {
            assert!(
                piano.position.y.0 <= bottom_line - 2.0 + 0.01,
                "{}",
                piano.position.y.0
            );
            assert!(axis <= bottom_line - 1.75 + 0.01, "{axis}");
        }
    }
}

/// A dynamic no glyph names is its words, in italic, centred on its note.
#[test]
fn a_dynamic_in_words_is_an_italic_run() {
    let measure = [
        direction(
            1,
            "<dynamics><other-dynamics>sempre</other-dynamics></dynamics>",
        ),
        note("C5", 2, "quarter", 1, 1, ""),
        note("rest", 6, "half", 1, 1, "").replace("<type>half</type>", "<type>half</type><dot/>"),
    ]
    .concat();
    let loaded = part("dynamic_words.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let runs: Vec<&ResolvedText> = layout.texts.iter().filter(|t| t.text == "sempre").collect();
    assert_eq!(runs.len(), 1);
    let run = runs[0];
    assert!(
        run.shaping.faces[0].family.0.contains("Italic"),
        "{:?}",
        run.shaping.faces[0].family
    );
    let ink = text_box(run);
    assert!(
        (centre_x([
            run.origin.x.0 + run.reserved_box.left.0,
            0.0,
            run.origin.x.0 + run.reserved_box.right.0,
            0.0
        ]) - centre_x(head(&loaded, &layout, e[0])))
        .abs()
            < 0.01,
        "{ink:?}"
    );
}

/// A hairpin runs from its first note to a space before the note it ends
/// at, its legs opening from its axis, a crescendo to its end and a
/// diminuendo from its start, the axis 1.75 under the staff.
#[test]
fn a_hairpin_opens_from_its_axis_between_its_notes() {
    for (kind, crescendo) in [("crescendo", true), ("diminuendo", false)] {
        let measure = [
            wedge(kind, 1),
            note("G4", 2, "quarter", 1, 1, ""),
            note("A4", 2, "quarter", 1, 1, ""),
            wedge("stop", 1),
            note("G4", 2, "quarter", 1, 1, ""),
            note("rest", 2, "quarter", 1, 1, ""),
        ]
        .concat();
        let loaded = part(&format!("hairpin_{kind}.musicxml"), 1, &[measure]);
        let layout = engrave(&loaded.reduced.score).layout;
        let e = events(&loaded);
        let (bottom_line, _) = staff_lines(&layout)[0];
        let legs = hairpin_legs(&layout);
        assert_eq!(legs.len(), 2, "{legs:?}");
        let (upper, lower) = (legs[0], legs[1]);
        near(upper.0 .0, head(&loaded, &layout, e[0])[0], "its start");
        near(upper.1 .0, head(&loaded, &layout, e[2])[0] - 1.0, "its end");
        let (narrow, wide) = if crescendo { (0, 1) } else { (1, 0) };
        let at = |leg: Leg, end: usize| if end == 0 { leg.0 .1 } else { leg.1 .1 };
        near(at(upper, narrow), at(lower, narrow), "its tip");
        near(at(upper, wide) - at(lower, wide), 1.15, "its mouth");
        // Its stems up, nothing under the staff moves it.
        near(at(upper, narrow), bottom_line - 1.75, "its axis");
    }
}

/// A dynamic and the hairpins meeting it are one chain: each hairpin keeps
/// half a space from the dynamic at either end, and the chain stands in one
/// row, aligned on the optical centres, as low as its lowest member needs.
#[test]
fn a_chain_of_dynamics_and_hairpins_stands_in_one_row() {
    let measure = [
        dynamic("p", 1),
        wedge("crescendo", 1),
        note("C5", 2, "quarter", 1, 1, ""),
        note("C4", 2, "quarter", 1, 1, ""),
        wedge("stop", 1),
        dynamic("ff", 1),
        note("C5", 2, "quarter", 1, 1, ""),
        note("rest", 2, "quarter", 1, 1, ""),
    ]
    .concat();
    let loaded = part("chain.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let legs = hairpin_legs(&layout);
    assert_eq!(legs.len(), 2);
    let p = named(&layout, "dynamicPiano")[0];
    let ff = named(&layout, "dynamicFF")[0];
    near(legs[0].0 .0, p[2] + 0.5, "the hairpin after p");
    assert!(legs[0].1 .0 <= ff[0] - 0.5 + 0.01, "the hairpin before ff");
    let axis = legs[0].0 .1;
    let centre = |name: &str| only(&layout, name).position.y.0 + 0.46;
    near(centre("dynamicPiano"), axis, "p on the row");
    near(centre("dynamicFF"), axis, "ff on the row");
    // The low C under the hairpin set the row.
    assert!(axis < staff_lines(&layout)[0].0 - 1.75 - 0.1, "{axis}");
}

/// A hairpin over a system break is drawn a piece in each system, each
/// opening as MuseScore draws one: a crescendo from its tip to its mouth in
/// the first, and in the next from a half-space opening to its mouth; a
/// diminuendo from its mouth to a half-space opening, then from its mouth to
/// its tip. Its first piece stops a quarter space short of its system's end
/// and the next starts at its system's first note, as any line's
/// (`TLayout::layoutSystemSLine`).
#[test]
fn a_hairpin_over_a_system_break_opens_each_piece() {
    for (kind, crescendo) in [("crescendo", true), ("diminuendo", false)] {
        let first = [wedge(kind, 1), note("C5", 8, "whole", 1, 1, "")].concat();
        let second = [
            "<print new-system=\"yes\"/>".to_string(),
            note("C5", 4, "half", 1, 1, ""),
            wedge("stop", 1),
            note("C5", 4, "half", 1, 1, ""),
        ]
        .concat();
        let name = format!("hairpin_break_{kind}.musicxml");
        let loaded = part(&name, 1, &[first, second]);
        let layout = engrave(&loaded.reduced.score).layout;
        let legs = hairpin_legs(&layout);
        assert_eq!(legs.len(), 4, "{legs:?}");
        // Two systems: the pieces in page order, upper legs first in each.
        let mut pieces: Vec<_> = legs.clone();
        pieces.sort_by(|a, b| b.0 .1.total_cmp(&a.0 .1));
        let (one, two) = (&pieces[..2], &pieces[2..]);
        let opening = |piece: &[Leg], end: usize| {
            let y = |leg: &Leg| if end == 0 { leg.0 .1 } else { leg.1 .1 };
            (y(&piece[0]) - y(&piece[1])).abs()
        };
        let (a, b, c, d) = if crescendo {
            (0.0, 1.15, 0.5, 1.15)
        } else {
            (1.15, 0.5, 1.15, 0.0)
        };
        near(opening(one, 0), a, "the first piece's start");
        near(opening(one, 1), b, "the first piece's end");
        near(opening(two, 0), c, "the second piece's start");
        near(opening(two, 1), d, "the second piece's end");
        let end = system_end(&layout, one[0].0 .1);
        near(one[0].1 .0, end - 0.25, "short of the system's end");
        let e = events(&loaded);
        near(
            two[0].0 .0,
            head(&loaded, &layout, e[1])[0],
            "from the system's first note",
        );
    }
}

/// A hairpin over a break is placed by the ink it is drawn over: one ending
/// in a full system a space before a low sharpened note, its end over the
/// sharp, stands clear of the sharp.
#[test]
fn a_hairpin_over_a_break_clears_the_ink_at_its_end() {
    let first = [wedge("crescendo", 1), note("C5", 8, "whole", 1, 1, "")].concat();
    let second = [
        "<print new-system=\"yes\"/>".to_string(),
        note("C5", 4, "half", 1, 1, ""),
        wedge("stop", 1),
        note("G#3", 4, "half", 1, 1, ""),
    ]
    .concat();
    let third = [
        "<print new-system=\"yes\"/>".to_string(),
        note("C5", 8, "whole", 1, 1, ""),
    ]
    .concat();
    let loaded = part("hairpin_sharp.musicxml", 1, &[first, second, third]);
    let layout = engrave(&loaded.reduced.score).layout;
    let sharp = named(&layout, "accidentalSharp");
    assert_eq!(sharp.len(), 1);
    let sharp = sharp[0];
    let legs: Vec<Leg> = hairpin_legs(&layout)
        .into_iter()
        .filter(|leg| (leg.0 .1 - sharp[1]).abs() < 8.0)
        .collect();
    assert_eq!(legs.len(), 2, "{legs:?}");
    let (left, right) = (legs[0].0 .0, legs[0].1 .0);
    assert!(
        right > sharp[0] && left < sharp[2],
        "the piece reaches over the sharp: {legs:?} {sharp:?}"
    );
    let top = legs
        .iter()
        .flat_map(|leg| [leg.0 .1, leg.1 .1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(top <= sharp[1] - 0.5 + 1e-3, "{top} over {sharp:?}");
}

/// A hairpin is at least a space long: one over a sixty-fourth in a measure
/// of them too wide for its line, whose next stands less than two spaces
/// after it.
#[test]
fn a_hairpin_is_at_least_a_space_long() {
    // A measure of sixty-fourths, too many for its line.
    let mut notes = vec![wedge("crescendo", 1), note("C5", 1, "64th", 1, 1, "")];
    notes.push(wedge("stop", 1));
    notes.extend((1..64).map(|_| note("D5", 1, "64th", 1, 1, "")));
    let measure = notes.concat();
    let loaded = part_in("short_hairpin.musicxml", 1, 16, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let step = head(&loaded, &layout, e[1])[0] - head(&loaded, &layout, e[0])[0];
    assert!(step < 2.0, "the notes stand {step} apart");
    let legs = hairpin_legs(&layout);
    assert_eq!(legs.len(), 2);
    assert!(legs[0].1 .0 - legs[0].0 .0 >= 1.0 - 1e-3, "{legs:?}");
}

/// A dynamic where no note of its staff starts stands at its own time, in a
/// column made for it: one anchored half a note into its measure, over a
/// whole note.
#[test]
fn a_dynamic_where_no_note_starts_stands_at_its_time() {
    use epiphany_core::{
        AnchorOffset, MarkerKind, MeasurePosition, MusicalDuration, RationalTime, TimeAnchor,
    };
    let measure = [dynamic("mf", 1), note("C5", 8, "whole", 1, 1, "")].concat();
    let loaded = part("dynamic_between.musicxml", 1, &[measure]);
    let mut score = loaded.reduced.score.clone();
    let id = loaded.import.ids.measures[0][0][0];
    for marker in &mut score.cross_cutting.markers {
        if matches!(marker.kind, MarkerKind::Dynamic(_)) {
            marker.anchor = TimeAnchor::Measure {
                id,
                position: MeasurePosition::Start,
                offset: AnchorOffset::Musical(MusicalDuration(
                    RationalTime::new(1, 2).expect("a half"),
                )),
            };
        }
    }
    let layout = engrave(&score).layout;
    let e = events(&loaded);
    let mf = only(&layout, "dynamicMF");
    assert!(
        mf.position.x.0 > head(&loaded, &layout, e[0])[2],
        "after the whole note's head: {}",
        mf.position.x.0
    );
}

/// On a grand staff a dynamic of the upper staff stands centred between the
/// two staves, where the ink between them lets it.
#[test]
fn a_grand_staffs_dynamic_stands_between_its_staves() {
    let measure = [
        dynamic("mf", 1),
        note("C5", 8, "whole", 1, 1, ""),
        backup(8),
        note("C3", 8, "whole", 5, 2, ""),
    ]
    .concat();
    let loaded = part("grand_dynamic.musicxml", 2, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let lines = staff_lines(&layout);
    let middle = (lines[0].0 + lines[1].1) * 0.5;
    near(
        only(&layout, "dynamicMF").position.y.0 + 0.46,
        middle,
        "mf's centre",
    );
}

/// A syllable stands centred on its note's head but for its leading and
/// trailing marks, a line under the staff; one extending a melisma stands
/// with its letters from the head's left (`LyricsLayout::layout`).
#[test]
fn a_syllable_stands_centred_on_its_note_but_for_its_marks() {
    let measure = [
        syllable(
            note("A4", 2, "quarter", 1, 1, ""),
            1,
            "single",
            "word,",
            false,
        ),
        syllable(note("A4", 2, "quarter", 1, 1, ""), 1, "single", "la", true),
        note("A4", 2, "quarter", 1, 1, ""),
        note("rest", 2, "quarter", 1, 1, ""),
    ]
    .concat();
    let loaded = part("syllables.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let (bottom_line, _) = staff_lines(&layout)[0];
    let word = runs(&layout, "word,")[0];
    // The letters of "word" centred: the run's advance less the comma's.
    let letters = epiphany_layout_ir::shape_text(
        word.provenance.clone(),
        "word",
        epiphany_layout_ir::TextFace::Roman,
        2.016,
        epiphany_layout_ir::TextAlign::Left,
        epiphany_layout_ir::Point::new(0.0, 0.0),
    )
    .0;
    let width = letters.reserved_box.right.0 - letters.reserved_box.left.0;
    let left = word.origin.x.0 + word.reserved_box.left.0;
    near(
        left + width * 0.5,
        centre_x(head(&loaded, &layout, e[0])),
        "word's letters' centre",
    );
    near(
        word.origin.y.0,
        bottom_line - 3.0,
        "the first verse's baseline",
    );
    let la = runs(&layout, "la")[0];
    near(
        la.origin.x.0 + la.reserved_box.left.0,
        head(&loaded, &layout, e[1])[0],
        "la from the head's left",
    );

    // Marks around the letters are left out of the centring; a verse
    // number leads letters from the head's left; digits alone are the
    // syllable, centred whole.
    let measure = [
        syllable(
            note("A4", 2, "quarter", 1, 1, ""),
            1,
            "single",
            "(word)",
            false,
        ),
        syllable(
            note("A4", 2, "quarter", 1, 1, ""),
            1,
            "single",
            "1. Wo",
            false,
        ),
        syllable(note("A4", 2, "quarter", 1, 1, ""), 1, "single", "12", false),
        note("rest", 2, "quarter", 1, 1, ""),
    ]
    .concat();
    let loaded = part("syllable_marks.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    // A caret stop's x on the page.
    let caret = |t: &ResolvedText, offset: usize| {
        t.origin.x.0
            + t.clusters
                .clusters
                .iter()
                .flat_map(|c| c.carets.iter())
                .find(|stop| stop.offset as usize == offset)
                .expect("a caret stop")
                .x
                .0
    };
    let bracketed = runs(&layout, "(word)")[0];
    near(
        (caret(bracketed, 1) + caret(bracketed, 5)) * 0.5,
        centre_x(head(&loaded, &layout, e[0])),
        "the bracketed word's letters' centre",
    );
    near(
        caret(runs(&layout, "1. Wo")[0], 3),
        head(&loaded, &layout, e[1])[0],
        "the numbered syllable's letters from the head's left",
    );
    let digits = runs(&layout, "12")[0];
    near(
        digits.origin.x.0 + (digits.reserved_box.left.0 + digits.reserved_box.right.0) * 0.5,
        centre_x(head(&loaded, &layout, e[2])),
        "digits alone centred",
    );
}

/// The verses stand a line apart, Edwin's line at 10 points; ink under the
/// staff moves the first verse to stand a space clear of it, and the second
/// with it.
#[test]
fn verses_stand_a_line_apart_and_move_together() {
    let measure = |low: &str| {
        [
            syllable(
                syllable(
                    note("A4", 2, "quarter", 1, 1, ""),
                    1,
                    "single",
                    "one",
                    false,
                ),
                2,
                "single",
                "two",
                false,
            ),
            syllable(
                syllable(note(low, 2, "quarter", 1, 1, ""), 1, "single", "la", false),
                2,
                "single",
                "li",
                false,
            ),
            note("rest", 4, "half", 1, 1, ""),
        ]
        .concat()
    };
    let line = 1.2 * 2.016;
    for (low, moved) in [("G4", false), ("C3", true)] {
        let loaded = part(&format!("verses_{low}.musicxml"), 1, &[measure(low)]);
        let layout = engrave(&loaded.reduced.score).layout;
        let (bottom_line, _) = staff_lines(&layout)[0];
        let (one, two) = (runs(&layout, "one")[0], runs(&layout, "two")[0]);
        near(one.origin.y.0 - two.origin.y.0, line, "a line apart");
        if moved {
            let e = events(&loaded);
            let low_ink = head(&loaded, &layout, e[1])[1];
            let la = runs(&layout, "la")[0];
            near(
                la.origin.y.0 + la.bounds.top.0,
                low_ink - 1.0,
                "a space under the low note",
            );
        } else {
            near(
                one.origin.y.0,
                bottom_line - 3.0,
                "three spaces under the staff",
            );
        }
    }
}

/// A verse keeps its syllables apart, a quarter space, or room for a dash
/// where a hyphen joins them, at natural width and set toward its floor
/// alike: four measures of words on quarters, every other word joined to the
/// next by a hyphen, the file breaking the system after them, on pages from
/// one too narrow for their floor to one that holds them whole.
#[test]
fn a_verse_keeps_its_syllables_apart() {
    let words: String = (0..4)
        .map(|m| {
            let notes: String = (0..4)
                .map(|k| {
                    let (syllabic, text) = if k % 2 == 0 {
                        ("begin", "Hallo")
                    } else {
                        ("end", "wordy")
                    };
                    syllable(note("A4", 2, "quarter", 1, 1, ""), 1, syllabic, text, false)
                })
                .collect();
            let attributes = if m == 0 {
                "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4\
                 </beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>"
            } else {
                ""
            };
            format!(
                "<measure number=\"{}\">{attributes}{notes}</measure>",
                m + 1
            )
        })
        .collect();
    let mut spans = Vec::new();
    // The gaps a hyphen's dash stood in with room to spare over its least.
    let mut roomy = 0;
    for width in (40..=120).step_by(4).map(|w| w as f32) {
        let xml = format!(
            "<score-partwise version=\"4.0\"><defaults><scaling><millimeters>7</millimeters>\
             <tenths>40</tenths></scaling><page-layout><page-height>3000</page-height>\
             <page-width>{}</page-width><page-margins type=\"both\"><left-margin>75\
             </left-margin><right-margin>75</right-margin><top-margin>75</top-margin>\
             <bottom-margin>75</bottom-margin></page-margins></page-layout></defaults>\
             <part-list><score-part id=\"P1\"><part-name>A</part-name></score-part>\
             </part-list><part id=\"P1\">{words}<measure number=\"5\">\
             <print new-system=\"yes\"/><note><pitch><step>A</step><octave>4</octave>\
             </pitch><duration>8</duration><voice>1</voice><type>whole</type></note>\
             </measure></part></score-partwise>",
            width * 10.0
        );
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("verse_{width}.musicxml"));
        std::fs::write(&path, xml).expect("written");
        let loaded = load(&path).expect("loads");
        let layout = epiphany_cli::engrave_loaded(&loaded).layout;
        let firsts = runs(&layout, "Hallo");
        let seconds = runs(&layout, "wordy");
        assert_eq!((firsts.len(), seconds.len()), (8, 8));
        // In page order; a pair in two systems keeps nothing.
        let mut words: Vec<(&ResolvedText, bool)> = firsts
            .iter()
            .map(|t| (*t, true))
            .chain(seconds.iter().map(|t| (*t, false)))
            .collect();
        words.sort_by(|a, b| {
            let (a, b) = (a.0.origin, b.0.origin);
            b.y.0.total_cmp(&a.y.0).then(a.x.0.total_cmp(&b.x.0))
        });
        for pair in words.windows(2) {
            let ((a, hyphen), (b, _)) = (pair[0], pair[1]);
            if (a.origin.y.0 - b.origin.y.0).abs() > 0.01 {
                continue;
            }
            let gap = if hyphen { 0.5 } else { 0.25 };
            let (a, b) = (text_box(a), text_box(b));
            assert!(b[0] >= a[2] + gap - 1e-3, "{width}: {a:?} {b:?}");
            if hyphen {
                // Its dash a pad clear of each.
                let dashes: Vec<_> = lyric_lines(&layout)
                    .into_iter()
                    .filter(|d| d.0 > a[0] && d.1 < b[2] && (d.2 - a[1]).abs() < 1.0)
                    .collect();
                assert_eq!(dashes.len(), 1, "{width}: {a:?} {b:?}");
                // As long as the gap less its pads allows, between the least
                // and the longest dash, centred in it.
                let d = dashes[0];
                let room = b[0] - a[2] - 2.0 * 0.05;
                let near = |x: f32, y: f32| (x - y).abs() < 1e-3;
                assert!(
                    near(d.1 - d.0, room.clamp(0.4, 0.6))
                        && near((d.0 + d.1) * 0.5, (a[2] + b[0]) * 0.5),
                    "{width}: {d:?} {a:?} {b:?}"
                );
                if room > 0.4 + 1e-3 && room < 1.5 {
                    roomy += 1;
                }
            }
        }
        let y = firsts[0].origin.y.0;
        if words.iter().all(|(t, _)| (t.origin.y.0 - y).abs() < 0.01) {
            spans.push(text_box(seconds[7])[2] - text_box(firsts[0])[0]);
        }
    }
    // Some page holding them in one system sets them tighter than another,
    // toward their floor.
    let widest = spans.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let tightest = spans.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(tightest < widest - 1.0, "{spans:?}");
    // Some dash stood in a gap wider than its least's room and its pads but
    // narrower than a pad's growth would leave unchanged.
    assert!(roomy > 0, "no gap tested the pads");
}

/// A hyphen stands between its syllables as a dash 0.4 to 0.6 long, centred
/// between them, at half Edwin's x-height over their baseline; over a system
/// break, a dash after the first in its system and another before the next
/// in the next system.
#[test]
fn a_hyphen_stands_between_its_syllables() {
    let first = [
        syllable(note("A4", 4, "half", 1, 1, ""), 1, "begin", "no", false),
        syllable(note("A4", 4, "half", 1, 1, ""), 1, "end", "thing", false),
    ]
    .concat();
    let loaded = part("hyphen.musicxml", 1, &[first]);
    let layout = engrave(&loaded.reduced.score).layout;
    let (no, thing) = (
        text_box(runs(&layout, "no")[0]),
        text_box(runs(&layout, "thing")[0]),
    );
    let dashes = lyric_lines(&layout);
    assert_eq!(dashes.len(), 1, "{dashes:?}");
    let (l, r, y) = dashes[0];
    assert!((0.4 - 1e-3..=0.6 + 1e-3).contains(&(r - l)), "{dashes:?}");
    near(
        (l + r) * 0.5,
        (no[2] + 0.05 + thing[0] - 0.05) * 0.5,
        "the dash's centre",
    );
    near(
        y,
        runs(&layout, "no")[0].origin.y.0 + 0.5 * 0.466 * 2.016,
        "the dash's height",
    );

    // Over a break.
    let first = syllable(note("A4", 8, "whole", 1, 1, ""), 1, "begin", "no", false);
    let second = [
        "<print new-system=\"yes\"/>".to_string(),
        syllable(note("A4", 8, "whole", 1, 1, ""), 1, "end", "thing", false),
    ]
    .concat();
    let loaded = part("hyphen_break.musicxml", 1, &[first, second]);
    let layout = engrave(&loaded.reduced.score).layout;
    let (no, thing) = (
        text_box(runs(&layout, "no")[0]),
        text_box(runs(&layout, "thing")[0]),
    );
    let dashes = lyric_lines(&layout);
    // After "no", from 0.05 past it to a quarter space short of its system's
    // end, more than 16 spaces: a dash in each 16-space share, centred in it,
    // its longest.
    let end = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, TypedObjectId::Staff(_))
                && s.from.y.0 > no[3]
                && s.from.y.0 < no[3] + 8.0
        })
        .map(|s| s.to.x.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let (x0, x1) = (no[2] + 0.05, end - 0.25);
    let n = ((x1 - x0) / 16.0).ceil();
    let after: Vec<_> = dashes
        .iter()
        .filter(|d| (d.2 - no[1]).abs() < 2.0)
        .collect();
    assert!(n >= 2.0 && after.len() as f32 == n, "{n}: {dashes:?}");
    for (k, d) in after.iter().enumerate() {
        near(
            (d.0 + d.1) * 0.5,
            x0 + (x1 - x0) / n * (k as f32 + 0.5),
            "a dash centred in its share",
        );
        near(d.1 - d.0, 0.6, "the longest dash");
    }
    // Before "thing", opening its system: the least dash, ending 0.05 before
    // it.
    let before: Vec<_> = dashes
        .iter()
        .filter(|d| (d.2 - thing[1]).abs() < 2.0)
        .collect();
    assert_eq!(before.len(), 1, "{dashes:?}");
    near(before[0].1, thing[0] - 0.05, "the dash's end");
    near(before[0].1 - before[0].0, 0.4, "the least dash");

    // Into a full system, to its second note: a dash in each 16-space share
    // between the system's first column and the syllable, whose end rides
    // its note as the system stretches.
    let first = syllable(note("A4", 8, "whole", 1, 1, ""), 1, "begin", "no", false);
    let second = [
        "<print new-system=\"yes\"/>".to_string(),
        note("A4", 2, "quarter", 1, 1, ""),
        syllable(note("A4", 2, "quarter", 1, 1, ""), 1, "end", "thing", false),
        note("rest", 4, "half", 1, 1, ""),
    ]
    .concat();
    let third = [
        "<print new-system=\"yes\"/>".to_string(),
        note("A4", 8, "whole", 1, 1, ""),
    ]
    .concat();
    let loaded = part("hyphen_stretched.musicxml", 1, &[first, second, third]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let thing = text_box(runs(&layout, "thing")[0]);
    let before: Vec<_> = lyric_lines(&layout)
        .into_iter()
        .filter(|d| (d.2 - thing[1]).abs() < 2.0)
        .collect();
    let (x0, x1) = (head(&loaded, &layout, e[1])[0], thing[0] - 0.05);
    let n = ((x1 - x0) / 16.0).ceil();
    assert!(before.len() as f32 == n, "{n}: {before:?}");
    for (k, d) in before.iter().enumerate() {
        near(
            (d.0 + d.1) * 0.5,
            x0 + (x1 - x0) / n * (k as f32 + 0.5),
            "a dash centred in its share",
        );
    }

    // A middle syllable's word goes on too, and to the next syllable of its
    // own verse, past a note only the second verse sings.
    let both = |n: String, one: Option<(&str, &str)>, two: &str| {
        let n = match one {
            Some((syllabic, text)) => syllable(n, 1, syllabic, text, false),
            None => n,
        };
        syllable(n, 2, "single", two, false)
    };
    let notes = [
        both(
            note("A4", 2, "quarter", 1, 1, ""),
            Some(("begin", "Hal")),
            "a",
        ),
        both(
            note("A4", 2, "quarter", 1, 1, ""),
            Some(("middle", "le")),
            "b",
        ),
        both(note("A4", 2, "quarter", 1, 1, ""), None, "c"),
        syllable(note("A4", 2, "quarter", 1, 1, ""), 1, "end", "lu", false),
    ]
    .concat();
    let loaded = part("hyphen_middle.musicxml", 1, &[notes]);
    let layout = engrave(&loaded.reduced.score).layout;
    let ink = |words: &str| text_box(runs(&layout, words)[0]);
    let dashes = lyric_lines(&layout);
    let between = |a: [f32; 4], b: [f32; 4]| {
        dashes
            .iter()
            .filter(|d| d.0 >= a[2] && d.1 <= b[0] && (d.2 - a[1]).abs() < 1.0)
            .count()
    };
    assert!(between(ink("Hal"), ink("le")) >= 1, "{dashes:?}");
    assert!(between(ink("le"), ink("lu")) >= 1, "{dashes:?}");
}

/// A melisma line runs from just after its syllable to its last note's
/// head; one that would stand shorter than a space is not drawn.
#[test]
fn a_melisma_line_runs_to_its_last_note() {
    let measure = [
        syllable(note("A4", 2, "quarter", 1, 1, ""), 1, "single", "Ah", true),
        note("B4", 2, "quarter", 1, 1, ""),
        note("C5", 2, "quarter", 1, 1, ""),
        syllable(note("A4", 2, "quarter", 1, 1, ""), 1, "single", "la", false),
    ]
    .concat();
    let loaded = part("melisma.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let ah = text_box(runs(&layout, "Ah")[0]);
    let lines = lyric_lines(&layout);
    assert_eq!(lines.len(), 1, "{lines:?}");
    near(lines[0].0, ah[2] + 0.2, "from after Ah");
    near(
        lines[0].1,
        head(&loaded, &layout, e[2])[2],
        "to the last note's head",
    );
    near(
        lines[0].2,
        runs(&layout, "Ah")[0].origin.y.0,
        "on the baseline",
    );

    // Over one sixty-fourth, the next syllable on the one after: shorter
    // than a space, so not drawn.
    let measure: String = [
        syllable(note("A4", 1, "64th", 1, 1, ""), 1, "single", "la", true),
        note("B4", 1, "64th", 1, 1, ""),
        syllable(note("C5", 1, "64th", 1, 1, ""), 1, "single", "lu", false),
    ]
    .concat()
        + &(0..61)
            .map(|_| note("C5", 1, "64th", 1, 1, ""))
            .collect::<String>();
    let loaded = part_in("short_melisma.musicxml", 1, 16, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let span = head(&loaded, &layout, e[1])[2] - (text_box(runs(&layout, "la")[0])[2] + 0.2);
    assert!(span > 0.0 && span < 0.95, "{span}");
    assert!(
        lyric_lines(&layout).is_empty(),
        "{:?}",
        lyric_lines(&layout)
    );
}

/// A syllable takes room in its system: a long word on the last note of
/// each measure, past its barline, stands within the page's margin in every
/// system, the full ones as the last.
#[test]
fn a_system_holds_its_syllables() {
    let measure: String = (0..8)
        .map(|k| {
            let text = if k == 7 { "extraordinarily" } else { "la" };
            syllable(note("A4", 1, "eighth", 1, 1, ""), 1, "single", text, false)
        })
        .collect();
    let measures = vec![measure; 8];
    let loaded = part("long_word.musicxml", 1, &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let words = runs(&layout, "extraordinarily");
    assert_eq!(words.len(), 8);
    let systems: BTreeSet<i32> = words
        .iter()
        .map(|w| (w.origin.y.0 * 100.0).round() as i32)
        .collect();
    assert!(systems.len() >= 3, "{systems:?}");
    // The default page: A4 at an 8 mm staff, 7.5 margins, 105 wide.
    for word in words {
        let ink = text_box(word);
        assert!(ink[2] <= 105.0 - 7.5 + 1e-3, "{ink:?}");
    }
}

/// Words over `staff`, as MuseScore writes staff text, or, `italic`, as it
/// writes expression text, below.
fn words(text: &str, staff: u8, italic: bool) -> String {
    let (placement, style) = if italic {
        ("below", " font-style=\"italic\"")
    } else {
        ("above", "")
    };
    format!(
        "<direction placement=\"{placement}\"><direction-type><words{style}>{text}</words>\
         </direction-type><staff>{staff}</staff></direction>"
    )
}

/// A tempo mark over `staff`: its words in bold, if any, and its metronome
/// mark, if any, a beat of `beat` with `dots` at `per_minute`.
fn tempo(words: Option<&str>, beat: Option<(&str, u8, u32)>, staff: u8) -> String {
    let words = words.map_or(String::new(), |w| {
        format!("<direction-type><words font-weight=\"bold\">{w}</words></direction-type>")
    });
    let (metronome, sound) = beat.map_or((String::new(), 100), |(unit, dots, per)| {
        let dots = "<beat-unit-dot/>".repeat(usize::from(dots));
        (
            format!(
                "<direction-type><metronome><beat-unit>{unit}</beat-unit>{dots}\
                 <per-minute>{per}</per-minute></metronome></direction-type>"
            ),
            per,
        )
    });
    format!(
        "<direction placement=\"above\">{words}{metronome}<staff>{staff}</staff>\
         <sound tempo=\"{sound}\"/></direction>"
    )
}

/// A run's face's family, its first segment's.
fn face(run: &ResolvedText) -> &str {
    &run.shaping.faces[run.segments[0].face as usize].family.0
}

/// A run's size, its first segment's.
fn size(run: &ResolvedText) -> f32 {
    run.segments[0].size.0
}

/// A run's pen start and end across.
fn pen(run: &ResolvedText) -> (f32, f32) {
    (
        run.origin.x.0 + run.reserved_box.left.0,
        run.origin.x.0 + run.reserved_box.right.0,
    )
}

/// Staff text stands over its staff, upright, its baseline a space over
/// the staff, its pen from its note's head; words MuseScore writes as
/// expression text, italic and below, stand the same until the model holds
/// their kind (D79's Q9); and a note over the staff lifts the text a half
/// space clear of it.
#[test]
fn staff_text_stands_over_its_staff_from_its_note() {
    let measure = [
        words("dolce", 1, false),
        note("C5", 2, "quarter", 1, 1, ""),
        note("C5", 2, "quarter", 1, 1, ""),
        words("high", 1, false),
        note("A5", 4, "half", 1, 1, ""),
        backup(8),
        words("sotto voce", 2, true),
        note("E3", 8, "whole", 5, 2, ""),
    ]
    .concat();
    let loaded = part("staff_text.musicxml", 2, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let lines = staff_lines(&layout);
    let dolce = runs(&layout, "dolce")[0];
    assert_eq!(face(dolce), "Edwin Roman");
    near(size(dolce), 2.016, "ten points");
    near(dolce.origin.y.0, lines[0].1 + 1.0, "a space over the staff");
    near(
        pen(dolce).0,
        head(&loaded, &layout, e[0])[0],
        "from the head",
    );
    let sotto = runs(&layout, "sotto voce")[0];
    assert_eq!(face(sotto), "Edwin Roman");
    near(sotto.origin.y.0, lines[1].1 + 1.0, "over the lower staff");
    near(
        pen(sotto).0,
        head(&loaded, &layout, e[3])[0],
        "from its head",
    );
    let high = text_box(runs(&layout, "high")[0]);
    let a5 = head(&loaded, &layout, e[2]);
    near(high[1], a5[3] + 0.5, "a half space over the high note");
}

/// A text written in lines is drawn in lines (D81's Q2), as MuseScore sets a
/// text on its baseline (`TextLayout`, `BASELINE`): each line a run from its
/// note, the first on the text's baseline and each further one Edwin's line,
/// 1.2 of its size, lower, the whole moved clear of the staff as one; a
/// tempo mark's lines so too, its metronome after its last line.
#[test]
fn a_text_in_lines_is_drawn_in_lines() {
    let measure = [
        words("play softly\nand slowly", 1, false),
        note("C5", 4, "half", 1, 1, ""),
        tempo(Some("Allegro\nmolto"), Some(("quarter", 0, 120)), 1),
        note("C5", 4, "half", 1, 1, ""),
    ]
    .concat();
    let loaded = part("text_lines.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let (_, top) = staff_lines(&layout)[0];
    let (first, second) = (
        runs(&layout, "play softly")[0],
        runs(&layout, "and slowly")[0],
    );
    near(pen(first).0, pen(second).0, "both from the note");
    near(
        first.origin.y.0 - second.origin.y.0,
        1.2 * 2.016,
        "a line apart",
    );
    assert!(
        text_box(second)[1] >= top + 0.5 - 0.01,
        "the lower line clear of the staff"
    );
    let (allegro, molto) = (runs(&layout, "Allegro")[0], runs(&layout, "molto ")[0]);
    near(pen(allegro).0, pen(molto).0, "both from the tempo's note");
    near(
        allegro.origin.y.0 - molto.origin.y.0,
        1.2 * 2.419,
        "a line apart",
    );
    let figure = runs(&layout, " = 120")[0];
    near(
        figure.origin.y.0,
        molto.origin.y.0,
        "the metronome on the last line",
    );
}

/// A tempo mark stands over the system's top staff, whatever staff holds
/// it: its words in Edwin bold at 12 points, its metronome's beat in Bravura
/// Text at 20 and its figure in bold after it, pen to pen, on one baseline 2.0
/// over the staff, from its note's head, or, at a measure opening with a
/// time signature, from the signature; repeated from another staff at its
/// time, it is drawn once.
#[test]
fn a_tempo_mark_stands_over_the_top_staff() {
    let first = [
        tempo(Some("Lento"), None, 1),
        note("C5", 8, "whole", 1, 1, ""),
        backup(8),
        note("E3", 8, "whole", 5, 2, ""),
    ]
    .concat();
    let second = [
        note("C5", 8, "whole", 1, 1, ""),
        backup(8),
        tempo(Some("Allegro"), Some(("quarter", 0, 120)), 2),
        tempo(Some("Allegro"), Some(("quarter", 0, 120)), 1),
        note("E3", 8, "whole", 5, 2, ""),
    ]
    .concat();
    let third = [
        note("C5", 8, "whole", 1, 1, ""),
        backup(8),
        tempo(None, Some(("half", 1, 60)), 2),
        note("E3", 8, "whole", 5, 2, ""),
    ]
    .concat();
    let loaded = part("tempo.musicxml", 2, &[first, second, third]);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let top = staff_lines(&layout)[0].1;
    // Over the time signature the first measure opens with.
    let lento = runs(&layout, "Lento")[0];
    assert_eq!(face(lento), "Edwin Bold");
    near(size(lento), 2.419, "twelve points");
    near(lento.origin.y.0, top + 2.0, "2.0 over the top staff");
    let signature = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "timeSig4")
        .expect("a time signature");
    near(pen(lento).0, signature.position.x.0, "over the signature");
    // Words, beat and figure, pen to pen, drawn once.
    let allegro = runs(&layout, "Allegro ");
    assert_eq!(allegro.len(), 1, "drawn once");
    let beat = runs(&layout, "\u{ECA5}");
    let figure = runs(&layout, " = 120");
    assert_eq!((beat.len(), figure.len()), (1, 1));
    let (allegro, beat, figure) = (allegro[0], beat[0], figure[0]);
    assert_eq!(face(beat), "Bravura Text");
    near(size(beat), 4.032, "twenty points");
    assert_eq!(face(figure), "Edwin Bold");
    near(
        pen(allegro).0,
        head(&loaded, &layout, e[2])[0],
        "from its head",
    );
    near(pen(beat).0, pen(allegro).1, "the beat after the words");
    near(pen(figure).0, pen(beat).1, "the figure after the beat");
    for run in [allegro, beat, figure] {
        near(run.origin.y.0, top + 2.0, "one baseline");
    }
    // A dotted beat alone: the note, a space and its dot.
    let dotted = runs(&layout, "\u{ECA3} \u{ECB7}");
    assert_eq!(
        dotted.len(),
        1,
        "{:?}",
        layout.texts.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    assert!(dotted[0]
        .segments
        .iter()
        .all(|s| s.glyphs.iter().all(|g| g.glyph_id != 0)));
    near(
        pen(runs(&layout, " = 60")[0]).0,
        pen(dotted[0]).1,
        "its figure after it",
    );
}

/// The frame about a rehearsal mark: its strokes' lines, as
/// `(left, bottom, right, top)`.
fn frame(layout: &ResolvedLayoutIR, run: &ResolvedText) -> (f32, f32, f32, f32) {
    let strokes: Vec<_> = layout
        .strokes
        .iter()
        .filter(|s| s.provenance.source == run.provenance.source && s.from != s.to)
        .collect();
    assert_eq!(strokes.len(), 4, "{strokes:?}");
    for s in &strokes {
        near(s.thickness.0, 0.16, "the frame's width");
    }
    let xs = strokes
        .iter()
        .filter(|s| s.from.x.0 == s.to.x.0)
        .map(|s| s.from.x.0);
    let ys = strokes
        .iter()
        .filter(|s| s.from.y.0 == s.to.y.0)
        .map(|s| s.from.y.0);
    (
        xs.clone().fold(f32::INFINITY, f32::min),
        ys.clone().fold(f32::INFINITY, f32::min),
        xs.fold(f32::NEG_INFINITY, f32::max),
        ys.fold(f32::NEG_INFINITY, f32::max),
    )
}

/// A rehearsal mark stands in Edwin bold at 14 points in a frame half a
/// space off its ink, at least as wide as high, over the system's top staff:
/// centred on its measure's barline's right edge, where the measure starts;
/// at the piece's start, just after the first measure's signatures; after a
/// system break, just after the lead's clef, a letter taller than wide in a
/// square frame; and outside a tempo mark at its measure.
#[test]
fn a_rehearsal_mark_stands_framed_at_its_measures_start() {
    let rehearsal = |mark: &str| {
        format!(
            "<direction placement=\"above\"><direction-type><rehearsal>{mark}</rehearsal>\
             </direction-type><staff>1</staff></direction>"
        )
    };
    let measures = [
        [rehearsal("A"), note("C5", 8, "whole", 1, 1, "")].concat(),
        [
            rehearsal("B"),
            tempo(Some("Allegro"), None, 1),
            note("C5", 8, "whole", 1, 1, ""),
        ]
        .concat(),
        [
            "<print new-system=\"yes\"/>".to_string(),
            rehearsal("I"),
            note("C5", 8, "whole", 1, 1, ""),
        ]
        .concat(),
    ];
    let loaded = part("rehearsal.musicxml", 1, &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let centre = |run: &ResolvedText| centre_x(text_box(run));
    let (a, b, c) = (
        runs(&layout, "A")[0],
        runs(&layout, "B")[0],
        runs(&layout, "I")[0],
    );
    assert_eq!(face(a), "Edwin Bold");
    near(size(a), 2.822, "fourteen points");
    near(
        a.origin.y.0,
        staff_lines(&layout)[0].1 + 2.0,
        "2.0 over the top staff",
    );
    // Its frame, about its ink.
    let ink = text_box(b);
    let (l, bottom, r, top) = frame(&layout, b);
    let half = ((ink[3] - ink[1]) - (ink[2] - ink[0])).max(0.0) * 0.5;
    near(l, ink[0] - half - 0.58, "the frame's left");
    near(r, ink[2] + half + 0.58, "the frame's right");
    near(bottom, ink[1] - 0.58, "the frame's bottom");
    near(top, ink[3] + 0.58, "the frame's top");
    // On the barline's right edge.
    let barline = named(&layout, "barlineSingle")[0];
    near(centre(b), barline[2], "B on its barline's right edge");
    // After the first measure's time signature.
    let signature = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("timeSig"))
        .map(|g| glyph_box(g)[2])
        .fold(f32::NEG_INFINITY, f32::max);
    near(centre(a), signature, "A after the signature");
    // After the next system's clef.
    let clef = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "gClef")
        .map(glyph_box)
        .min_by(|p, q| p[1].total_cmp(&q[1]))
        .expect("a clef");
    near(centre(c), clef[2], "I after the lead's clef");
    // A letter taller than wide stands in a square frame.
    let square = frame(&layout, c);
    near(square.2 - square.0, square.3 - square.1, "a square frame");
    // Outside the tempo mark it overlaps.
    let allegro = text_box(runs(&layout, "Allegro")[0]);
    assert!(allegro[0] < r && allegro[2] > l, "{allegro:?} {l} {r}");
    assert!(
        bottom - 0.08 >= allegro[3] + 0.5 - 1e-3,
        "{bottom} {allegro:?}"
    );
}

/// A rehearsal mark takes no room across: a frame wider than its measure's
/// first beat, at the piece's start or on a barline, moves no note or
/// barline.
#[test]
fn a_rehearsal_mark_takes_no_room() {
    let measures = |label: Option<&str>| {
        let mark = label
            .map(|label| {
                format!(
                    "<direction placement=\"above\"><direction-type><rehearsal>{label}\
                     </rehearsal></direction-type><staff>1</staff></direction>"
                )
            })
            .unwrap_or_default();
        let beats = note("C5", 2, "quarter", 1, 1, "").repeat(4);
        [
            [mark.clone(), beats.clone()].concat(),
            [mark, beats].concat(),
        ]
    };
    let ink = |name: &str, label: Option<&str>| {
        let loaded = part(name, 1, &measures(label));
        let layout = engrave(&loaded.reduced.score).layout;
        let mut xs: Vec<f32> = ["noteheadBlack", "barlineSingle", "barlineFinal"]
            .into_iter()
            .flat_map(|glyph| named(&layout, glyph))
            .map(|ink| ink[0])
            .collect();
        xs.sort_by(f32::total_cmp);
        xs
    };
    let (bare, marked) = (
        ink("unmarked.musicxml", None),
        ink("wide-rehearsal.musicxml", Some("A Long Section Label")),
    );
    assert_eq!(bare.len(), 10, "{bare:?}");
    assert_eq!(bare.len(), marked.len());
    for (b, m) in bare.iter().zip(&marked) {
        near(*m, *b, "a note or barline beside a wide rehearsal mark");
    }
}

/// A mark beside or over its head gives its column neither its place nor
/// its share of a justified system's stretch: accents over a measure's
/// notes move no note, in a system set to the width; and a scoop left of a
/// note opening a measure, which takes room as its chord's ink does
/// (`ChordLayout::fillShape`), leaves the barline before its note, though
/// under the staff, left of the note's accidental, it reaches left past the
/// barline.
#[test]
fn a_mark_gives_its_column_no_place() {
    let measures = |accent: &str, scoop: &str| {
        [
            note("C5", 2, "quarter", 1, 1, accent).repeat(4),
            note("A#3", 8, "whole", 1, 1, scoop),
        ]
    };
    let accent = "<articulations><accent/></articulations>";
    let scoop = "<articulations><scoop/></articulations>";
    let ink = |name: &str, accent: &str, scoop: &str| {
        let loaded = part(name, 1, &measures(accent, scoop));
        let layout = engrave(&loaded.reduced.score).layout;
        let mut xs: Vec<f32> = [
            "noteheadBlack",
            "noteheadWhole",
            "barlineSingle",
            "barlineFinal",
        ]
        .into_iter()
        .flat_map(|glyph| named(&layout, glyph))
        .map(|ink| ink[0])
        .collect();
        xs.sort_by(f32::total_cmp);
        (xs, layout)
    };
    let (bare, _) = ink("unaccented.musicxml", "", "");
    let (accented, layout) = ink("accented.musicxml", accent, "");
    assert_eq!(named(&layout, "articAccentAbove").len(), 4);
    assert_eq!(bare.len(), 7, "{bare:?}");
    assert_eq!(bare.len(), accented.len());
    for (b, m) in bare.iter().zip(&accented) {
        near(*m, *b, "a note or barline beside its marks");
    }
    let (_, layout) = ink("scooped.musicxml", "", scoop);
    let scoop = named(&layout, "brassScoop");
    assert_eq!(scoop.len(), 1);
    let barline = named(&layout, "barlineSingle");
    assert_eq!(barline.len(), 1);
    let whole = named(&layout, "noteheadWhole");
    assert!(
        barline[0][2] < whole[0][0] && whole[0][0] > scoop[0][2],
        "the barline before the note, the scoop beside it: {barline:?} {whole:?} {scoop:?}"
    );
    assert!(
        scoop[0][0] < barline[0][0],
        "the scoop reaches past the barline"
    );
}

/// A segno or coda stands in Bravura Text at 18 points over the system's
/// top staff, its baseline 2.0 over it, centred on its measure's barline, or
/// at the piece's start on the system's left edge.
#[test]
fn a_segno_or_coda_stands_at_its_measures_start() {
    let sign = |name: &str| {
        format!(
            "<direction placement=\"above\"><direction-type><{name}/></direction-type>\
             <staff>1</staff></direction>"
        )
    };
    let measures = [
        [sign("coda"), note("C5", 8, "whole", 1, 1, "")].concat(),
        [sign("segno"), note("C5", 8, "whole", 1, 1, "")].concat(),
    ];
    let loaded = part("segno.musicxml", 1, &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let top = staff_lines(&layout)[0].1;
    let (segno, coda) = (runs(&layout, "\u{E047}")[0], runs(&layout, "\u{E048}")[0]);
    assert_eq!(face(segno), "Bravura Text");
    near(size(segno), 3.629, "eighteen points");
    near(segno.origin.y.0, top + 2.0, "2.0 over the staff");
    let barline = named(&layout, "barlineSingle")[0];
    near(
        centre_x(text_box(segno)),
        centre_x(barline),
        "the segno on its barline",
    );
    near(
        centre_x(text_box(coda)),
        7.5,
        "the coda on the system's edge",
    );
}

/// A mark where no note starts stands at its time, in a system set wide:
/// staff text and a dynamic in words anchored an eighth and a quarter into
/// their measure, and a tempo mark and a framed rehearsal mark anchored
/// three eighths and eleven sixteenths into their region, on no staff, each
/// after the first note and before the barline in the order of their times,
/// the frame about its mark.
#[test]
fn a_text_where_no_note_starts_stands_at_its_time() {
    use epiphany_core::{
        AnchorOffset, MarkerKind, MeasurePosition, MusicalDuration, RationalTime, RegionEdge,
        TimeAnchor,
    };
    let marks = [
        words("poco", 1, false),
        direction(
            1,
            "<dynamics><other-dynamics>sempre</other-dynamics></dynamics>",
        ),
        tempo(Some("Lento"), None, 1),
        direction(1, "<rehearsal>R</rehearsal>"),
        note("C5", 4, "half", 1, 1, ""),
        note("C5", 2, "quarter", 1, 1, ""),
        note("C5", 2, "quarter", 1, 1, ""),
    ]
    .concat();
    let measures = [
        marks,
        note("C5", 8, "whole", 1, 1, ""),
        [
            "<print new-system=\"yes\"/>".to_string(),
            note("C5", 8, "whole", 1, 1, ""),
        ]
        .concat(),
    ];
    let loaded = part("no_note.musicxml", 1, &measures);
    let mut score = loaded.reduced.score.clone();
    let id = loaded.import.ids.measures[0][0][0];
    let region = score.canvas.regions[0].id;
    let at = |n: i64, d: i64| {
        AnchorOffset::Musical(MusicalDuration(RationalTime::new(n, d).expect("a time")))
    };
    for marker in &mut score.cross_cutting.markers {
        let (measure, offset) = match marker.kind {
            MarkerKind::Text(_) => (true, at(1, 8)),
            MarkerKind::Dynamic(_) => (true, at(1, 4)),
            MarkerKind::Tempo(_) => (false, at(3, 8)),
            MarkerKind::Rehearsal(_) => (false, at(11, 16)),
            _ => continue,
        };
        marker.anchor = if measure {
            TimeAnchor::Measure {
                id,
                position: MeasurePosition::Start,
                offset,
            }
        } else {
            TimeAnchor::Region {
                id: region,
                edge: RegionEdge::Start,
                offset,
            }
        };
    }
    let layout = engrave(&score).layout;
    let e = events(&loaded);
    let first = head(&loaded, &layout, e[0]);
    let barline = named(&layout, "barlineSingle")[0];
    // Each at its time: staff text and a tempo mark from their pen, a
    // dynamic and a rehearsal mark by their centres.
    let mut last = first[2];
    for (words, centred) in [
        ("poco", false),
        ("sempre", true),
        ("Lento", false),
        ("R", true),
    ] {
        let run = runs(&layout, words);
        assert_eq!(run.len(), 1, "{words}");
        let x = if centred {
            centre_x(text_box(run[0]))
        } else {
            pen(run[0]).0
        };
        assert!(
            x > last && x < barline[0],
            "{words}: {x} after {last}, before {barline:?}"
        );
        last = x;
    }
    // The rehearsal mark's frame stands about its ink, though, as the
    // projection sets it, it reaches past the last note's column.
    let r = runs(&layout, "R")[0];
    let ink = text_box(r);
    let (l, _, right, _) = frame(&layout, r);
    let grow = ((ink[3] - ink[1]) - (ink[2] - ink[0])).max(0.0) * 0.5;
    near(l, ink[0] - grow - 0.58, "the frame's left");
    near(right, ink[2] + grow + 0.58, "the frame's right");
}

/// A mark at its measure's start is placed by its ink where it stands
/// there, not where its note is: a rehearsal mark and a segno at a barline
/// before a key and meter change, a note high over the staff after them,
/// stand at their own height beside the note; a segno with a rehearsal
/// mark stands outside it.
#[test]
fn a_mark_at_its_measures_start_clears_the_ink_where_it_stands() {
    // A key and a meter change, each held inside the staff.
    let key = |fifths: i8, beats: u8, beat: u8| {
        format!(
            "<attributes><key><fifths>{fifths}</fifths></key><time><beats>{beats}</beats>\
             <beat-type>{beat}</beat-type></time></attributes>"
        )
    };
    let measures = [
        note("C5", 8, "whole", 1, 1, ""),
        [
            key(-1, 2, 2),
            direction(1, "<rehearsal>B</rehearsal>"),
            note("C6", 8, "whole", 1, 1, ""),
        ]
        .concat(),
        note("C5", 8, "whole", 1, 1, ""),
        [
            key(0, 4, 4),
            direction(1, "<segno/>"),
            note("C6", 8, "whole", 1, 1, ""),
        ]
        .concat(),
        [
            direction(1, "<rehearsal>D</rehearsal>"),
            direction(1, "<segno/>"),
            note("C5", 8, "whole", 1, 1, ""),
        ]
        .concat(),
    ];
    let loaded = part("measure_start_ink.musicxml", 1, &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let top = staff_lines(&layout)[0].1;
    let (b, d) = (runs(&layout, "B")[0], runs(&layout, "D")[0]);
    let (_, _, r, _) = frame(&layout, b);
    let high = head(&loaded, &layout, e[1]);
    assert!(
        r < high[0],
        "the note stands beside the frame: {r} {high:?}"
    );
    near(b.origin.y.0, top + 2.0, "B at its own height");
    let mut segnos = runs(&layout, "\u{E047}");
    segnos.sort_by(|p, q| p.origin.x.0.total_cmp(&q.origin.x.0));
    assert_eq!(segnos.len(), 2);
    let high = head(&loaded, &layout, e[3]);
    assert!(text_box(segnos[0])[2] < high[0], "beside the note");
    near(
        segnos[0].origin.y.0,
        top + 2.0,
        "the segno at its own height",
    );
    let (_, _, _, frame_top) = frame(&layout, d);
    let outer = text_box(segnos[1]);
    assert!(
        outer[1] >= frame_top + 0.08 + 0.5 - 1e-3,
        "{outer:?} over {frame_top}"
    );
}

/// Where the staff lines nearest `y` end: their system's end.
fn system_end(layout: &ResolvedLayoutIR, y: f32) -> f32 {
    layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .min_by(|a, b| (a.from.y.0 - y).abs().total_cmp(&(b.from.y.0 - y).abs()))
        .map(|s| s.from.x.0.max(s.to.x.0))
        .expect("a staff line")
}

/// A note's notations: a trill mark and a wavy line, `kind` "start" or
/// "stop", or both, a trill mark with a sharp over it and its line's start.
fn trill(kind: &str) -> String {
    match kind {
        "sharp" => "<ornaments><trill-mark/><accidental-mark placement=\"above\">sharp\
                    </accidental-mark><wavy-line type=\"start\"/></ornaments>"
            .to_string(),
        "start" => "<ornaments><trill-mark/><wavy-line type=\"start\"/></ornaments>".to_string(),
        kind => format!("<ornaments><wavy-line type=\"{kind}\"/></ornaments>"),
    }
}

/// A trill line's glyphs, its sign and its wiggles, left to right.
fn trill_line(layout: &ResolvedLayoutIR) -> Vec<&ResolvedGlyph> {
    let mut found: Vec<&ResolvedGlyph> = layout
        .glyphs
        .iter()
        .filter(|g| matches!(g.provenance.source, TypedObjectId::Spanner(_)))
        .collect();
    found.sort_by(|a, b| {
        b.position
            .y
            .0
            .total_cmp(&a.position.y.0)
            .then(a.position.x.0.total_cmp(&b.position.x.0))
    });
    found
}

/// A trill line stands half a space over its staff, or clear of the ink
/// under it by half a space: from its note's column its sign, the trill it
/// stands for, the trill's accidental small over it, then wiggles, one each
/// advance, as many as reach to the nearest a space before the end of its
/// last note; the trill draws no sign of its own there. One over a system
/// break stops its wiggles a quarter space short of the system's end and
/// starts them again, with no sign, at the next system's first note
/// (`TLayout::layoutTrillSegment`, `LineSegment::symbolLine`).
#[test]
fn a_trill_line_is_its_sign_then_its_wiggles() {
    let (sign_advance, wiggle_advance) = (2134.0 / 1024.0, 971.0 / 1024.0);
    let measures = [
        [
            note("C5", 2, "quarter", 1, 1, &trill("sharp")),
            note("C5", 2, "quarter", 1, 1, ""),
            note("C5", 4, "half", 1, 1, &trill("stop")),
        ]
        .concat(),
        note("C5", 8, "whole", 1, 1, ""),
    ];
    let loaded = part("trill.musicxml", 1, &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let top = staff_lines(&layout)[0].1;
    let line = trill_line(&layout);
    let (sign, wiggles) = (line[0], &line[1..]);
    assert_eq!(sign.glyph.as_str(), "ornamentTrill");
    assert_eq!(
        named(&layout, "ornamentTrill").len(),
        1,
        "no sign of its own"
    );
    near(
        sign.position.x.0,
        head(&loaded, &layout, e[0])[0],
        "from its note",
    );
    // Half a space over the staff, or half a space clear of its top line.
    let half_line = layout
        .strokes
        .iter()
        .find(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .map(|s| s.thickness.0 * 0.5)
        .expect("a staff line");
    let at = (top + 0.5).max(top + half_line + 0.5 - sign.bounding_box.bottom.0);
    near(sign.position.y.0, at, "the sign's baseline");
    let end = named(&layout, "barlineSingle")[0][0] - 1.0;
    let from = sign.position.x.0 + sign_advance;
    let count = ((end - from) / wiggle_advance).round() as usize;
    assert!(count > 5);
    assert_eq!(wiggles.len(), count, "{wiggles:?}");
    for (i, w) in wiggles.iter().enumerate() {
        assert_eq!(w.glyph.as_str(), "wiggleTrill");
        near(
            w.position.x.0,
            from + i as f32 * wiggle_advance,
            "a wiggle's place",
        );
        near(w.position.y.0, sign.position.y.0, "on the sign's line");
    }
    // The sharp, small, centred over the sign 0.35 clear of it.
    let sharp = layout
        .texts
        .iter()
        .find(|t| t.text == "\u{E262}")
        .expect("the trill's sharp");
    let (ink, sign_ink) = (text_box(sharp), glyph_box(sign));
    near(ink[1], sign_ink[3] + 0.35, "0.35 over the sign");
    near(centre_x(ink), centre_x(sign_ink), "centred over the sign");

    // Over a system break.
    let measures = [
        [
            note("C5", 4, "half", 1, 1, &trill("start")),
            note("C5", 4, "half", 1, 1, ""),
        ]
        .concat(),
        [
            "<print new-system=\"yes\"/>".to_string(),
            note("C5", 4, "half", 1, 1, &trill("stop")),
            note("rest", 4, "half", 1, 1, ""),
        ]
        .concat(),
    ];
    let loaded = part("trill_break.musicxml", 1, &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    // The break's line, by its spanner, its pieces by their systems.
    let line: Vec<_> = trill_line(&layout);
    let source = line[0].provenance.source;
    let line: Vec<_> = line
        .into_iter()
        .filter(|g| g.provenance.source == source)
        .collect();
    let y0 = line[0].position.y.0;
    let first: Vec<_> = line
        .iter()
        .filter(|g| (g.position.y.0 - y0).abs() < 3.0)
        .collect();
    let second: Vec<_> = line
        .iter()
        .filter(|g| (g.position.y.0 - y0).abs() >= 3.0)
        .collect();
    // The first piece: its sign, then wiggles to a quarter space short of the
    // system's end.
    assert_eq!(first[0].glyph.as_str(), "ornamentTrill");
    let end = system_end(&layout, first[0].position.y.0) - 0.25;
    let from = first[0].position.x.0 + sign_advance;
    assert_eq!(
        first.len() - 1,
        ((end - from) / wiggle_advance).round() as usize
    );
    // The second: wiggles alone from the system's first note to a space
    // before the end of the line's last.
    assert!(second.iter().all(|g| g.glyph.as_str() == "wiggleTrill"));
    let from = head(&loaded, &layout, e[2])[0];
    near(second[0].position.x.0, from, "from the system's first note");
    let rest = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("rest"))
        .expect("the rest");
    let end = rest.position.x.0 - 1.0;
    assert_eq!(
        second.len(),
        ((end - from) / wiggle_advance).round() as usize
    );
}

/// A trill line of a second voice stands under its staff, clear of the ink
/// over it by half a space.
#[test]
fn a_second_voices_trill_line_stands_under_its_staff() {
    let measure = [
        note("C5", 8, "whole", 1, 1, ""),
        backup(8),
        note("D4", 4, "half", 2, 1, &trill("start")),
        note("D4", 4, "half", 2, 1, &trill("stop")),
    ]
    .concat();
    let loaded = part(
        "trill_below.musicxml",
        1,
        &[measure, note("C5", 8, "whole", 1, 1, "")],
    );
    let layout = engrave(&loaded.reduced.score).layout;
    let (bottom, _) = staff_lines(&layout)[0];
    let line = trill_line(&layout);
    let sign = glyph_box(line[0]);
    assert!(sign[3] < bottom, "under the staff: {sign:?} {bottom}");
    let e = events(&loaded);
    let low = head(&loaded, &layout, e[1]);
    let stem_bottom = stem(&layout, e[1]).1;
    let ink = low[1].min(stem_bottom);
    assert!(sign[3] <= ink - 0.5 + 1e-3, "{sign:?} under {ink}");

    // With nothing under the staff, half a space under its bottom line.
    let measure = [
        note("C5", 8, "whole", 1, 1, ""),
        backup(8),
        note("A4", 8, "whole", 2, 1, &trill("start")),
    ]
    .concat();
    let loaded = part(
        "trill_below_alone.musicxml",
        1,
        &[measure, note("C5", 8, "whole", 1, 1, &trill("stop"))],
    );
    let layout = engrave(&loaded.reduced.score).layout;
    let (bottom, _) = staff_lines(&layout)[0];
    let half_line = layout
        .strokes
        .iter()
        .find(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .map(|s| s.thickness.0 * 0.5)
        .expect("a staff line");
    let sign = glyph_box(trill_line(&layout)[0]);
    near(
        sign[3],
        bottom - half_line - 0.5,
        "half a space under the staff",
    );
}

/// A trill line is placed before the rest of the family, so a fermata at its
/// note stands outside it; its wiggles clear a high note under them by half
/// a space, by their own ink.
#[test]
fn a_trill_line_stands_nearest_its_staff() {
    let measure = [
        note(
            "C5",
            4,
            "half",
            1,
            1,
            &format!("{}<fermata type=\"upright\"/>", trill("start")),
        ),
        note("C6", 2, "quarter", 1, 1, ""),
        note("C5", 2, "quarter", 1, 1, &trill("stop")),
    ]
    .concat();
    let loaded = part(
        "trill_fermata.musicxml",
        1,
        &[measure, note("C5", 8, "whole", 1, 1, "")],
    );
    let layout = engrave(&loaded.reduced.score).layout;
    let e = events(&loaded);
    let line = trill_line(&layout);
    let sign = glyph_box(line[0]);
    let fermata = named(&layout, "fermataAbove")[0];
    assert!(
        fermata[1] >= sign[3] + 0.4 - 1e-3,
        "{fermata:?} over {sign:?}"
    );
    // The wiggles over the high note: their ink half a space over its head.
    let high = head(&loaded, &layout, e[1]);
    let over: Vec<_> = line
        .iter()
        .filter(|g| g.glyph.as_str() == "wiggleTrill")
        .map(|g| glyph_box(g))
        .filter(|w| w[0] < high[2] && w[2] > high[0])
        .collect();
    assert!(!over.is_empty());
    let lowest = over.iter().map(|w| w[1]).fold(f32::INFINITY, f32::min);
    near(
        lowest,
        high[3] + 0.5,
        "the wiggles half a space over the note",
    );
}

/// A line ending at the piece's end runs to a space before its final
/// barline, as one ending at any barline does: a hairpin, and a trill line's
/// wiggles.
#[test]
fn a_line_to_the_end_runs_to_a_space_before_the_final_barline() {
    let measure = [
        wedge("crescendo", 1),
        note("C5", 8, "whole", 1, 1, ""),
        wedge("stop", 1),
    ]
    .concat();
    let loaded = part("hairpin_to_end.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let end = named(&layout, "barlineFinal")[0][0] - 1.0;
    let legs = hairpin_legs(&layout);
    assert_eq!(legs.len(), 2, "{legs:?}");
    for leg in &legs {
        near(leg.1 .0, end, "the hairpin's end");
    }

    let measure = [
        note("C5", 4, "half", 1, 1, &trill("start")),
        note("C5", 4, "half", 1, 1, &trill("stop")),
    ]
    .concat();
    let loaded = part("trill_to_end.musicxml", 1, &[measure]);
    let layout = engrave(&loaded.reduced.score).layout;
    let end = named(&layout, "barlineFinal")[0][0] - 1.0;
    let line = trill_line(&layout);
    let from = line[0].position.x.0 + 2134.0 / 1024.0;
    let advance = 971.0 / 1024.0;
    assert_eq!(line.len() - 1, ((end - from) / advance).round() as usize);
}
