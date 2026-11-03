//! The family's lines on the page (X4d.1): pedal lines, ottavas and the notes
//! under them, text lines and verbal crescendos, brackets and glissandi, each
//! on a hand-written file, against the rules `epiphany-layout-ir`'s
//! `lines.rs` cites (MuseScore 4.7.5's defaults where Gould's *Behind Bars*
//! agrees or gives no figure).

use std::path::Path;

use epiphany_cli::{engrave_loaded, load, Loaded};
use epiphany_core::{Event, EventId, SpannerKind, TypedObjectId};
use epiphany_layout_ir::{ResolvedGlyph, ResolvedLayoutIR, ResolvedText, Stroke};

/// A part of one treble staff in 4/4, an eighth per division, its measures
/// as written; a measure opening with `<print new-system="yes"/>` starts a
/// system.
fn part(name: &str, measures: &[String]) -> Loaded {
    part_in(name, 2, measures)
}

/// [`part`] with `divisions` to the quarter.
fn part_in(name: &str, divisions: u8, measures: &[String]) -> Loaded {
    let body: String = measures
        .iter()
        .enumerate()
        .map(|(m, content)| {
            let attributes = if m == 0 {
                format!(
                    "<attributes><divisions>{divisions}</divisions><time><beats>4</beats>\
                     <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                     </attributes>"
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

/// A note: its pitch as a step and octave (`"C5"`, `"F#4"`, `"Bb4"`), its
/// duration in eighths and type, and what its `<notations>` hold.
fn note(pitch: &str, duration: u8, kind: &str, notations: &str) -> String {
    let (step, rest) = pitch.split_at(1);
    let (alter, octave) = match (rest.strip_prefix('#'), rest.strip_prefix('b')) {
        (Some(octave), _) => ("<alter>1</alter>", octave),
        (_, Some(octave)) => ("<alter>-1</alter>", octave),
        _ => ("", rest),
    };
    let accidental = match alter {
        "<alter>1</alter>" => "<accidental>sharp</accidental>",
        "<alter>-1</alter>" => "<accidental>flat</accidental>",
        _ => "",
    };
    let notations = if notations.is_empty() {
        String::new()
    } else {
        format!("<notations>{notations}</notations>")
    };
    format!(
        "<note><pitch><step>{step}</step>{alter}<octave>{octave}</octave></pitch>\
         <duration>{duration}</duration><voice>1</voice><type>{kind}</type>{accidental}\
         {notations}</note>"
    )
}

fn q(pitch: &str) -> String {
    note(pitch, 2, "quarter", "")
}

/// A direction holding each of `types` as a `<direction-type>`, standing at
/// the next note.
fn direction(types: &[&str]) -> String {
    let types: String = types
        .iter()
        .map(|t| format!("<direction-type>{t}</direction-type>"))
        .collect();
    format!("<direction>{types}</direction>")
}

fn pedal(kind: &str, sign: bool) -> String {
    let sign = if sign { "yes" } else { "no" };
    direction(&[&format!(
        "<pedal type=\"{kind}\" line=\"yes\" sign=\"{sign}\"/>"
    )])
}

/// An ottava's start, `"down"` an 8va, `"up"` an 8vb, of `size` 8 or 15, or
/// its stop.
fn octave_shift(kind: &str, size: u8) -> String {
    direction(&[&format!("<octave-shift type=\"{kind}\" size=\"{size}\"/>")])
}

/// A text line's start with its words, italic when `italic`, or its stop.
fn text_line(words: &str, italic: bool) -> String {
    let style = if italic { " font-style=\"italic\"" } else { "" };
    direction(&[
        &format!("<words{style}>{words}</words>"),
        "<dashes type=\"start\"/>",
    ])
}

fn text_line_stop() -> String {
    direction(&["<dashes type=\"stop\"/>"])
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

/// The page's runs whose string is `words`, left to right.
fn runs<'a>(layout: &'a ResolvedLayoutIR, words: &str) -> Vec<&'a ResolvedText> {
    let mut found: Vec<&ResolvedText> = layout.texts.iter().filter(|t| t.text == words).collect();
    found.sort_by(|a, b| a.origin.x.0.total_cmp(&b.origin.x.0));
    found
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

/// Every drawn stroke of the score's spanners of a kind `pick` takes, left to
/// right: its line, its dashes, its hooks.
fn line_strokes<'a>(
    loaded: &Loaded,
    layout: &'a ResolvedLayoutIR,
    pick: impl Fn(&SpannerKind) -> bool,
) -> Vec<&'a Stroke> {
    let ids: Vec<_> = loaded
        .reduced
        .score
        .cross_cutting
        .spanners
        .iter()
        .filter(|s| pick(&s.kind))
        .map(|s| TypedObjectId::Spanner(s.id))
        .collect();
    let mut found: Vec<&Stroke> = layout
        .strokes
        .iter()
        .filter(|s| ids.contains(&s.provenance.source) && s.from != s.to)
        .collect();
    found.sort_by(|a, b| {
        a.from
            .x
            .0
            .min(a.to.x.0)
            .total_cmp(&b.from.x.0.min(b.to.x.0))
    });
    found
}

/// A horizontal stroke's left, right and height, or a hook's x, bottom and
/// top.
type Piece = (f32, f32, f32);

/// The horizontal strokes and the vertical ones (the hooks) among `strokes`.
fn split(strokes: &[&Stroke]) -> (Vec<Piece>, Vec<Piece>) {
    let mut across = Vec::new();
    let mut hooks = Vec::new();
    for s in strokes {
        if (s.from.y.0 - s.to.y.0).abs() < 1e-4 {
            across.push((
                s.from.x.0.min(s.to.x.0),
                s.from.x.0.max(s.to.x.0),
                s.from.y.0,
            ));
        } else if (s.from.x.0 - s.to.x.0).abs() < 1e-4 {
            hooks.push((
                s.from.x.0,
                s.from.y.0.min(s.to.y.0),
                s.from.y.0.max(s.to.y.0),
            ));
        }
    }
    (across, hooks)
}

/// The bottom and top lines of the first staff on the page, and of each
/// system's staff in order, top system first.
fn staves(layout: &ResolvedLayoutIR) -> Vec<(f32, f32)> {
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

/// Dashes `(left, right, y)` of one piece, left to right, laid as MuseScore
/// distributes a dash pattern (`distributedDashPattern`): one more dash than
/// the pairs of `dash` and `gap` the piece's length holds whole, the gaps
/// equal.
fn spread(dashes: &[(f32, f32, f32)], dash: f32, gap: f32, what: &str) {
    let (Some(first), Some(last)) = (dashes.first(), dashes.last()) else {
        panic!("{what}: no dashes");
    };
    let length = last.1 - first.0;
    let pairs = (length / (dash + gap)).floor().max(1.0) as usize;
    assert_eq!(dashes.len(), pairs + 1, "{what}: {dashes:?}");
    let even = (length - dash * (pairs as f32 + 1.0)) / pairs as f32;
    for pair in dashes.windows(2) {
        near(pair[1].0 - pair[0].1, even, &format!("{what}: an even gap"));
    }
}

const PED: &str = "\u{E650}";
const PED_CONTINUED: &str = "\u{E676}\u{E650}\u{E677}";
const OTTAVA: &str = "\u{E510}";

fn is_pedal(kind: &SpannerKind) -> bool {
    matches!(
        kind,
        SpannerKind::PedalLine(_) | SpannerKind::PedalBracket(_)
    )
}

/// A pedal line under its staff (Gould, *Behind Bars*, pp. 332 to 335;
/// MuseScore's `Pedal`): the pedal sign half a space before its note, at
/// least 2.5 under the staff and clear of the ink over it by 0.7, here the
/// staff's bottom line, as MuseScore 4.7.5's own page of a probe sets it; a
/// solid line from half a space after the sign, 0.11 wide, its foot on the
/// sign's baseline, to 0.75 before the note its release falls at, where it
/// rises 1.2 to its release.
#[test]
fn a_pedal_line_stands_under_its_staff_from_its_sign_to_its_release() {
    let loaded = part(
        "pedal.musicxml",
        &[format!(
            "{}{}{}{}{}{}",
            pedal("start", true),
            q("G4"),
            q("A4"),
            pedal("stop", false),
            q("G4"),
            q("A4"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let (bottom, _) = staves(&layout)[0];
    let sign = runs(&layout, PED);
    assert_eq!(sign.len(), 1);
    let sign = sign[0];
    let first = head(&loaded, &layout, e[0]);
    near(
        sign.origin.x.0,
        first[0] - 0.5,
        "the sign half a space before its note",
    );
    assert!(
        sign.origin.y.0 <= bottom - 2.5 + 1e-3,
        "at least 2.5 under the staff"
    );
    near(
        text_box(sign)[3],
        bottom - 0.065 - 0.7,
        "0.7 clear of the bottom line's ink",
    );
    let strokes = line_strokes(&loaded, &layout, is_pedal);
    let (across, hooks) = split(&strokes);
    assert_eq!((across.len(), hooks.len()), (1, 1), "{strokes:?}");
    let (x0, x1, y) = across[0];
    let release = head(&loaded, &layout, e[2])[0] - 0.75;
    near(
        x0,
        text_box(sign)[2] + 0.5,
        "the line half a space after the sign",
    );
    near(
        y,
        sign.origin.y.0 + 0.055,
        "the line's foot on the baseline",
    );
    near(x1, release + 0.055, "the line to the hook's outer edge");
    let (hx, h0, h1) = hooks[0];
    near(hx, release, "the hook 0.75 before the release");
    near(h0, y - 0.055, "the hook from the line's foot");
    near(h1, y + 1.2, "the hook 1.2 up");
    assert_eq!(strokes[0].thickness.0, 0.11);
}

/// A pedal bracket, drawn with no sign, opens with a hook rising 1.2 where
/// its line starts, on its note.
#[test]
fn a_pedal_bracket_opens_with_a_hook() {
    let loaded = part(
        "pedal_bracket.musicxml",
        &[format!(
            "{}{}{}{}{}{}",
            pedal("start", false),
            q("G4"),
            q("A4"),
            pedal("stop", false),
            q("G4"),
            q("A4"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    assert!(runs(&layout, PED).is_empty());
    let (across, hooks) = split(&line_strokes(&loaded, &layout, is_pedal));
    assert_eq!((across.len(), hooks.len()), (1, 2));
    let start = head(&loaded, &layout, e[0])[0] + 0.055;
    near(hooks[0].0, start, "the opening hook on the note");
    near(across[0].0, start, "the line from the hook");
    near(
        hooks[0].2 - hooks[0].1,
        1.2 + 0.055,
        "the opening hook rising 1.2",
    );
}

/// A pedal released where the staff's next pedal starts at a note ends 1.25
/// before it; released where none starts, 0.75 before (MuseScore's
/// `Pedal::linePos`).
#[test]
fn a_pedal_retaken_at_a_note_ends_further_before_it() {
    let retaken = part(
        "pedal_retaken.musicxml",
        &[format!(
            "{}{}{}{}{}{}{}{}",
            pedal("start", true),
            q("G4"),
            q("A4"),
            pedal("stop", false),
            pedal("start", true),
            q("G4"),
            q("A4"),
            pedal("stop", false),
        )],
    );
    let apart = part(
        "pedal_apart.musicxml",
        &[format!(
            "{}{}{}{}{}{}{}{}",
            pedal("start", true),
            q("G4"),
            q("A4"),
            pedal("stop", false),
            q("G4"),
            pedal("start", true),
            q("A4"),
            pedal("stop", false),
        )],
    );
    for (loaded, inset) in [(&retaken, 1.25), (&apart, 0.75)] {
        let layout = engrave_loaded(loaded).layout;
        let e = events(loaded);
        let (_, hooks) = split(&line_strokes(loaded, &layout, is_pedal));
        near(
            hooks[0].0,
            head(loaded, &layout, e[2])[0] - inset,
            &format!("the first release {inset} before E5"),
        );
    }
}

/// Every pedal line of a staff in a system stands at one height, the
/// lowest any of them needs (`SystemLayout::processLines`, `align`; Gould
/// lets consecutive signs align where there is room, p. 333): a pedal over a
/// low note takes the next one, over high notes, down with it.
#[test]
fn a_staffs_pedal_lines_in_a_system_stand_at_one_height() {
    let loaded = part(
        "pedal_aligned.musicxml",
        &[format!(
            "{}{}{}{}{}{}{}{}{}",
            pedal("start", true),
            q("C3"),
            q("D3"),
            pedal("stop", false),
            q("G4"),
            pedal("start", true),
            q("A4"),
            q("G4"),
            pedal("stop", false),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let signs = runs(&layout, PED);
    assert_eq!(signs.len(), 2);
    let (bottom, _) = staves(&layout)[0];
    assert!(
        signs[0].origin.y.0 < bottom - 2.5 - 2.0,
        "the first pedal pushed down by its low notes: {}",
        signs[0].origin.y.0
    );
    near(
        signs[1].origin.y.0,
        signs[0].origin.y.0,
        "both at one height",
    );
}

/// A pedal held over a system break goes on to the system's end with no
/// hook, and its piece in the next system opens with "(Ped.)" (Gould's
/// reminder at a new system, p. 332; `pedalContinueText`), its line half a
/// space after it, ending at its release with the hook.
#[test]
fn a_pedal_over_a_system_break_opens_the_next_with_a_reminder() {
    let loaded = part(
        "pedal_break.musicxml",
        &[
            format!(
                "{}{}{}{}{}",
                pedal("start", true),
                q("C5"),
                q("D5"),
                q("E5"),
                q("F5")
            ),
            format!(
                "<print new-system=\"yes\"/>{}{}{}{}{}",
                q("G5"),
                q("A5"),
                pedal("stop", false),
                q("G5"),
                q("F5"),
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let staves = staves(&layout);
    assert_eq!(staves.len(), 2, "two systems");
    assert_eq!(runs(&layout, PED).len(), 1);
    let reminder = runs(&layout, PED_CONTINUED);
    assert_eq!(reminder.len(), 1, "the reminder in the second system");
    assert!(
        reminder[0].origin.y.0 < staves[1].0,
        "under the second system's staff"
    );
    let (across, hooks) = split(&line_strokes(&loaded, &layout, is_pedal));
    assert_eq!((across.len(), hooks.len()), (2, 1), "{across:?} {hooks:?}");
    let second = across
        .iter()
        .find(|l| l.2 < staves[0].0 - 10.0)
        .expect("a second piece");
    near(
        second.0,
        text_box(reminder[0])[2] + 0.5,
        "the line half a space after the reminder",
    );
    assert!(
        hooks[0].1 < staves[1].0,
        "the release hook in the second system"
    );
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

/// An ottava over the staff (Gould, pp. 28 to 30; MuseScore's `Ottava`): its
/// numeral at its first note, its top at least 2.0 over the staff and its
/// ink 0.7 clear of the staff's top line, as on MuseScore 4.7.5's page of a
/// probe; a dashed line 0.11 wide hanging from the numeral's top, from half a
/// space after the numeral to a space past its last note's ink, where its
/// hook falls 1.0 toward the staff; its dashes 0.66 long and at least 0.66
/// apart, the first and last at its ends. Under the staff, an 8vb's numeral
/// 0.7 clear of the bottom line, the line on its baseline, its hook rising.
#[test]
fn an_ottava_draws_its_numeral_and_dashed_line_to_a_corner() {
    for (name, kind, sounding, after) in [
        ("ottava_line_above.musicxml", "down", "D6", "B4"),
        ("ottava_line_below.musicxml", "up", "A3", "A4"),
    ] {
        let above = kind == "down";
        let loaded = part(
            name,
            &[format!(
                "{}{}{}{}{}{}",
                octave_shift(kind, 8),
                note(sounding, 4, "half", ""),
                note(sounding, 2, "quarter", ""),
                octave_shift("stop", 8),
                note(after, 1, "eighth", ""),
                note(after, 1, "eighth", ""),
            )],
        );
        let layout = engrave_loaded(&loaded).layout;
        let e = events(&loaded);
        let (bottom, top) = staves(&layout)[0];
        let numeral = runs(&layout, OTTAVA);
        assert_eq!(numeral.len(), 1, "{name}");
        let numeral = text_box(numeral[0]);
        let first = head(&loaded, &layout, e[0]);
        near(
            numeral[0],
            first[0],
            &format!("{name}: the numeral at its note"),
        );
        let strokes = line_strokes(&loaded, &layout, |k| {
            matches!(k, SpannerKind::OctaveLine(_))
        });
        let (dashes, hooks) = split(&strokes);
        assert_eq!(hooks.len(), 1, "{name}");
        let y = if above {
            assert!(numeral[3] >= top + 2.0 - 1e-3, "{name}: at least 2.0 over");
            near(
                numeral[1],
                top + 0.065 + 0.7,
                &format!("{name}: 0.7 clear of the top line"),
            );
            numeral[3] - 0.055
        } else {
            assert!(
                numeral_baseline(&layout) <= bottom - 2.0 + 1e-3,
                "{name}: at least 2.0 under"
            );
            near(
                numeral[3],
                bottom - 0.065 - 0.7,
                &format!("{name}: 0.7 clear of the bottom line"),
            );
            numeral_baseline(&layout) + 0.055
        };
        for d in &dashes {
            near(d.2, y, &format!("{name}: the dashes' height"));
            near(d.1 - d.0, 0.66, &format!("{name}: a dash's length"));
        }
        spread(&dashes, 0.66, 0.66, name);
        near(
            dashes[0].0,
            numeral[2] + 0.5,
            &format!("{name}: the line after the numeral"),
        );
        // Its last note's ink: its head, and its stem where it turns up.
        let stem_right = layout
            .strokes
            .iter()
            .filter(|s| s.provenance.source == TypedObjectId::Event(e[1]) && s.from.x == s.to.x)
            .map(|s| s.from.x.0 + s.thickness.0 * 0.5)
            .fold(f32::NEG_INFINITY, f32::max);
        let last = head(&loaded, &layout, e[1])[2].max(stem_right);
        let next = head(&loaded, &layout, e[2])[0];
        let end = (last + 1.0).min(next) - 0.055;
        near(
            hooks[0].0,
            end,
            &format!("{name}: the corner a space past the note"),
        );
        near(
            dashes.last().expect("dashes").1,
            end + 0.055,
            &format!("{name}: the last dash at the corner"),
        );
        let (h0, h1) = (hooks[0].1, hooks[0].2);
        if above {
            near(h0, y - 1.0, &format!("{name}: the hook falling 1.0"));
        } else {
            near(h1, y + 1.0, &format!("{name}: the hook rising 1.0"));
        }
    }
}

/// An ottava's corner never passes the next note (`Ottava::linePos`): over
/// thirty-seconds a system holds at its floor, where a space past its last
/// note's ink reaches past the next note's head, its corner stands at that
/// head, half its width short.
#[test]
fn an_ottavas_corner_stops_at_the_next_note() {
    let notes: String = (0..32)
        .map(|k| {
            let (step, octave) = if (4..8).contains(&k) {
                ("C", 7)
            } else {
                [("C", 5), ("G", 4)][k % 2]
            };
            let shift = match k {
                4 => {
                    "<direction><direction-type><octave-shift type=\"down\" size=\"8\"/>\
                      </direction-type></direction>"
                }
                8 => {
                    "<direction><direction-type><octave-shift type=\"stop\" size=\"8\"/>\
                      </direction-type></direction>"
                }
                _ => "",
            };
            format!(
                "{shift}<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
                 <duration>1</duration><voice>1</voice><type>32nd</type></note>"
            )
        })
        .collect();
    let xml = format!(
        "<score-partwise version=\"4.0\"><defaults><scaling><millimeters>7</millimeters>\
         <tenths>40</tenths></scaling><page-layout><page-height>3000</page-height>\
         <page-width>600</page-width><page-margins type=\"both\"><left-margin>75\
         </left-margin><right-margin>75</right-margin><top-margin>75</top-margin>\
         <bottom-margin>75</bottom-margin></page-margins></page-layout></defaults>\
         <part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list>\
         <part id=\"P1\"><measure number=\"1\"><attributes><divisions>8</divisions><time>\
         <beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line>\
         </clef></attributes>{notes}</measure><measure number=\"2\"><print new-system=\"yes\"/>\
         <note><pitch><step>C</step><octave>5</octave></pitch><duration>32</duration>\
         <voice>1</voice><type>whole</type></note></measure></part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ottava_close.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let next = head(&loaded, &layout, e[8])[0];
    let last = head(&loaded, &layout, e[7])[2];
    assert!(
        last + 1.0 > next + 0.1,
        "the next note nearer than a space: {last} {next}"
    );
    let (across, hooks) = split(&line_strokes(&loaded, &layout, |k| {
        matches!(k, SpannerKind::OctaveLine(_))
    }));
    near(hooks[0].0, next - 0.055, "the corner at the next head");
    near(
        across.last().expect("the line").1,
        next,
        "the line to the next head",
    );
}

/// The baseline of the page's only ottava numeral.
fn numeral_baseline(layout: &ResolvedLayoutIR) -> f32 {
    runs(layout, OTTAVA)[0].origin.y.0
}

/// An ottava held over a system break goes on to the system's end with no
/// corner, and its numeral stands again at the start of its piece in the
/// next system (Gould, p. 30).
#[test]
fn an_ottava_over_a_system_break_repeats_its_numeral() {
    let loaded = part(
        "ottava_break.musicxml",
        &[
            format!(
                "{}{}{}{}{}",
                octave_shift("down", 8),
                q("C7"),
                q("D7"),
                q("E7"),
                q("F7")
            ),
            format!(
                "<print new-system=\"yes\"/>{}{}{}{}{}",
                q("G7"),
                q("A7"),
                octave_shift("stop", 8),
                q("G5"),
                q("F5"),
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let staves = staves(&layout);
    assert_eq!(staves.len(), 2);
    let numerals = runs(&layout, OTTAVA);
    assert_eq!(numerals.len(), 2, "the numeral in each system");
    let (_, hooks) = split(&line_strokes(&loaded, &layout, |k| {
        matches!(k, SpannerKind::OctaveLine(_))
    }));
    assert_eq!(hooks.len(), 1, "one corner, at its end");
    assert!(
        hooks[0].1 < staves[0].0 - 5.0,
        "the corner in the second system"
    );
}

/// A text line over its staff (MuseScore's `TextLine`): its words upright,
/// their middle on the line, at least 1.0 over the staff and the words 0.7
/// clear of its top line, as on MuseScore 4.7.5's page of a probe; the line
/// 0.15 wide in dashes 0.6 long from half a space after the words to a space
/// before the note it ends at.
#[test]
fn a_text_line_stands_over_its_staff_through_its_words() {
    let loaded = part(
        "text_line.musicxml",
        &[format!(
            "{}{}{}{}{}{}",
            text_line("sul pont.", false),
            q("C5"),
            q("D5"),
            text_line_stop(),
            q("E5"),
            q("F5"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let (_, top) = staves(&layout)[0];
    let words = runs(&layout, "sul pont.");
    assert_eq!(words.len(), 1);
    let words = text_box(words[0]);
    let y = (words[1] + words[3]) * 0.5;
    assert!(y >= top + 1.0 - 1e-3, "at least 1.0 over the staff");
    near(
        words[1],
        top + 0.065 + 0.7,
        "the words 0.7 clear of the top line",
    );
    let strokes = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::TextLine(_)));
    let (dashes, hooks) = split(&strokes);
    assert!(hooks.is_empty());
    near(
        dashes[0].0,
        words[2] + 0.5,
        "the line half a space after the words",
    );
    near(
        dashes.last().expect("dashes").1,
        head(&loaded, &layout, e[2])[0] - 1.0,
        "a space before E5",
    );
    for d in &dashes {
        near(d.2, y, "the dashes on the line");
        near(d.1 - d.0, 0.6, "a dash 0.6 long");
    }
    spread(&dashes, 0.6, 0.6, "the text line");
    assert_eq!(strokes[0].thickness.0, 0.15);
}

/// A text line opening with a term of gradual tempo change is drawn as
/// MuseScore draws its gradual tempo change and Gould a tempo indication
/// (p. 182): its words in Edwin bold at 12 points, at least 2.0 over the
/// staff, on the line's baseline; the line 0.15 wide in dashes 0.9 long
/// spread with gaps of 1.35 (p. 185), to a space before the note it ends at.
#[test]
fn a_gradual_tempo_change_stands_bold_over_the_staff() {
    let loaded = part(
        "tempo_change.musicxml",
        &[format!(
            "{}{}{}{}{}{}{}{}",
            text_line("rit.", false),
            q("C5"),
            q("D5"),
            q("C5"),
            q("D5"),
            text_line_stop(),
            q("E5"),
            q("F5"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let (_, top) = staves(&layout)[0];
    let words = runs(&layout, "rit.");
    assert_eq!(words.len(), 1);
    let face = format!("{:?}", words[0].shaping.faces[0].family);
    assert!(face.contains("Bold"), "bold: {face}");
    assert!(
        (words[0].segments[0].size.0 - 2.419).abs() < 1e-3,
        "at 12 points"
    );
    let baseline = words[0].origin.y.0;
    assert!(baseline >= top + 2.0 - 1e-3, "at least 2.0 over the staff");
    let strokes = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::TextLine(_)));
    let (dashes, hooks) = split(&strokes);
    assert!(hooks.is_empty());
    for d in &dashes {
        near(d.2, baseline + 0.075, "the line on the baseline");
        near(d.1 - d.0, 0.9, "a dash 0.9 long");
    }
    spread(&dashes, 0.9, 1.35, "the tempo change");
    near(
        dashes[0].0,
        text_box(words[0])[2] + 0.5,
        "the line half a space after the words",
    );
    near(
        dashes.last().expect("dashes").1,
        head(&loaded, &layout, e[4])[0] - 1.0,
        "a space before E5",
    );
}

/// A gradual tempo change is a system object: two parts holding the same one
/// at one time draw it once, over the system's top staff.
#[test]
fn a_gradual_tempo_change_in_two_parts_is_drawn_once_over_the_system() {
    let measure = format!(
        "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{}{}{}{}{}{}",
        text_line("rit.", false),
        q("C5"),
        q("D5"),
        text_line_stop(),
        q("E5"),
        q("F5"),
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part><score-part id=\"P2\"><part-name>B</part-name>\
         </score-part></part-list><part id=\"P1\"><measure number=\"1\">{measure}</measure>\
         </part><part id=\"P2\"><measure number=\"1\">{measure}</measure></part>\
         </score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tempo_change_twice.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave_loaded(&loaded).layout;
    let staves = staves(&layout);
    let words = runs(&layout, "rit.");
    assert_eq!(words.len(), 1, "drawn once");
    assert!(words[0].origin.y.0 > staves[0].1, "over the top staff");
}

/// A gradual tempo change held by a lower part alone is drawn over the
/// system's top staff all the same.
#[test]
fn a_gradual_tempo_change_in_a_lower_part_stands_over_the_system() {
    let measure =
        |with: bool| {
            format!(
            "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
             </time><clef><sign>G</sign><line>2</line></clef></attributes>{}{}{}{}{}{}",
            if with { text_line("rit.", false) } else { String::new() },
            q("C5"),
            q("D5"),
            if with { text_line_stop() } else { String::new() },
            q("E5"),
            q("F5"),
        )
        };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part><score-part id=\"P2\"><part-name>B</part-name>\
         </score-part></part-list><part id=\"P1\"><measure number=\"1\">{}</measure>\
         </part><part id=\"P2\"><measure number=\"1\">{}</measure></part>\
         </score-partwise>",
        measure(false),
        measure(true),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tempo_change_lower.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave_loaded(&loaded).layout;
    let staves = staves(&layout);
    assert!(staves.len() >= 2, "two staves");
    let words = runs(&layout, "rit.");
    assert_eq!(words.len(), 1, "drawn once");
    assert!(words[0].origin.y.0 > staves[0].1, "over the top staff");
}

/// A text line opening with a dynamic term is a verbal crescendo, as
/// MuseScore's hairpin of the line type and Gould's verbal crescendo (pp. 101
/// and 106): its words italic, 2.0 under the staff on the line's baseline,
/// half a space after a dynamic at its note; its line 0.12 wide in dashes
/// 0.72 long at least 1.08 apart; over a system break, "(cresc.)" opening
/// its next piece.
#[test]
fn a_verbal_crescendo_stands_as_a_hairpin_with_its_reminder() {
    let loaded = part(
        "cresc_line.musicxml",
        &[
            format!(
                "{}{}{}{}{}{}",
                direction(&["<dynamics><p/></dynamics>"]),
                text_line("cresc.", true),
                q("C5"),
                q("D5"),
                q("E5"),
                q("F5"),
            ),
            format!(
                "<print new-system=\"yes\"/>{}{}{}{}{}",
                q("G5"),
                q("A5"),
                text_line_stop(),
                q("G5"),
                q("F5"),
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let staves = staves(&layout);
    let words = runs(&layout, "cresc.");
    assert_eq!(words.len(), 1);
    let dynamic = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "dynamicPiano")
        .map(glyph_box)
        .expect("the dynamic");
    let words_box = text_box(words[0]);
    let _ = words_box;
    near(
        words[0].origin.x.0,
        dynamic[2] + 0.5,
        "the words half a space after the dynamic",
    );
    assert!(
        words[0].origin.y.0 <= staves[0].0 - 2.0 + 1e-3,
        "under the staff"
    );
    let face = &words[0].shaping.faces[0].family;
    assert!(format!("{face:?}").contains("Italic"), "italic: {face:?}");
    let strokes = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::TextLine(_)));
    let (dashes, _) = split(&strokes);
    for d in &dashes {
        near(d.1 - d.0, 0.72, "a dash 0.72 long");
    }
    let first: Vec<_> = dashes
        .iter()
        .copied()
        .filter(|d| d.2 > staves[0].0 - 10.0)
        .collect();
    spread(&first, 0.72, 1.08, "the first piece");
    let reminder = runs(&layout, "(cresc.)");
    assert_eq!(reminder.len(), 1, "the reminder at the next system");
    assert!(reminder[0].origin.y.0 < staves[1].0, "in the second system");
}

/// A bracket draws its line alone, hooked 1.5 toward the staff at its end.
#[test]
fn a_bracket_ends_in_a_hook_toward_its_staff() {
    let loaded = part(
        "bracket.musicxml",
        &[format!(
            "{}{}{}{}{}{}",
            direction(&["<bracket type=\"start\" line-end=\"none\" line-type=\"solid\"/>"]),
            q("C5"),
            q("D5"),
            direction(&["<bracket type=\"stop\" line-end=\"down\"/>"]),
            q("E5"),
            q("F5"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let (_, top) = staves(&layout)[0];
    let (across, hooks) = split(&line_strokes(&loaded, &layout, |k| {
        matches!(k, SpannerKind::Bracket(_))
    }));
    assert_eq!((across.len(), hooks.len()), (1, 1));
    let y = across[0].2;
    assert!(y >= top + 1.0 - 1e-3, "at least 1.0 over the staff");
    near(hooks[0].2, y + 0.075, "the hook from the line's top");
    near(hooks[0].1, y - 1.5, "the hook 1.5 down toward the staff");
    near(
        hooks[0].0,
        across[0].1 - 0.075,
        "the hook at the line's end",
    );
}

/// A glissando's line (Gould, pp. 140 to 142; MuseScore's
/// `TLayout::layoutNoteAnchoredLine`): on the line joining its heads'
/// centres, from a quarter space past its first note's ink at its head to a
/// quarter space before its last note's, short of the last note's
/// accidental; "gliss." along it where it fits.
#[test]
fn a_glissando_runs_between_its_heads_clear_of_their_ink() {
    let loaded = part(
        "glissando.musicxml",
        &[format!(
            "{}{}{}{}",
            note(
                "C5",
                4,
                "half",
                "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>"
            ),
            note("G#5", 2, "quarter", "<slide type=\"stop\" number=\"1\"/>"),
            q("A5"),
            q("B5"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let strokes = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(strokes.len(), 1);
    let s = strokes[0];
    let a = head(&loaded, &layout, e[0]);
    let b = head(&loaded, &layout, e[1]);
    let sharp = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "accidentalSharp")
        .map(glyph_box)
        .expect("the sharp");
    near(s.from.x.0, a[2] + 0.25, "a quarter past the first head");
    near(s.to.x.0, sharp[0] - 0.25, "a quarter short of the sharp");
    let (ca, cb) = (
        ((a[0] + a[2]) * 0.5, (a[1] + a[3]) * 0.5),
        ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5),
    );
    let on_line = |x: f32| ca.1 + (x - ca.0) * (cb.1 - ca.1) / (cb.0 - ca.0);
    near(
        s.from.y.0,
        on_line(s.from.x.0),
        "the start on the heads' line",
    );
    near(s.to.y.0, on_line(s.to.x.0), "the end on the heads' line");
    assert_eq!(s.thickness.0, 0.15);
    assert_eq!(runs(&layout, "gliss.").len(), 1, "its words along it");
}

/// Between heads on one line a glissando rises or falls a quarter space at
/// each end the way it slides (Gould, p. 142; MuseScore), and its words stand
/// along it only where they are shorter than it.
#[test]
fn a_glissando_on_one_line_slants_toward_its_slide() {
    let loaded = part(
        "glissando_line.musicxml",
        &[format!(
            "{}{}{}{}{}",
            note(
                "C5",
                1,
                "eighth",
                "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>"
            ),
            note("C#5", 1, "eighth", "<slide type=\"stop\" number=\"1\"/>"),
            q("A5"),
            q("B5"),
            note("C5", 4, "half", ""),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let s = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando))[0];
    assert!(
        s.to.y.0 > s.from.y.0 + 0.1,
        "rising toward the sharp: {s:?}"
    );
    // On the line from a quarter space under its first head's centre to a
    // quarter space over its last's.
    let centre = |ink: Ink| ((ink[0] + ink[2]) * 0.5, (ink[1] + ink[3]) * 0.5);
    let (a, b) = (
        centre(head(&loaded, &layout, e[0])),
        centre(head(&loaded, &layout, e[1])),
    );
    let (a, b) = ((a.0, a.1 - 0.25), (b.0, b.1 + 0.25));
    let on = |x: f32| a.1 + (x - a.0) * (b.1 - a.1) / (b.0 - a.0);
    near(s.from.y.0, on(s.from.x.0), "its start on that line");
    near(s.to.y.0, on(s.to.x.0), "its end on that line");
    let length = ((s.to.x.0 - s.from.x.0).powi(2) + (s.to.y.0 - s.from.y.0).powi(2)).sqrt();
    let words = runs(&layout, "gliss.");
    let fits = words
        .first()
        .map(|w| w.bounds.right.0 - w.bounds.left.0 < length);
    assert!(fits != Some(false), "words longer than the line: {length}");
}

/// A wavy glissando is drawn as wiggles along its line, the line itself
/// drawing nothing.
#[test]
fn a_wavy_glissando_draws_wiggles() {
    let loaded = part(
        "glissando_wavy.musicxml",
        &[format!(
            "{}{}{}{}",
            note(
                "C5",
                4,
                "half",
                "<glissando type=\"start\" line-type=\"wavy\" number=\"1\">gliss.</glissando>"
            ),
            note(
                "G5",
                2,
                "quarter",
                "<glissando type=\"stop\" number=\"1\"/>"
            ),
            q("A5"),
            q("B5"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    assert!(line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando)).is_empty());
    let wiggles: Vec<_> = layout
        .texts
        .iter()
        .filter(|t| !t.text.is_empty() && t.text.chars().all(|c| c == '\u{EAAF}'))
        .collect();
    assert_eq!(wiggles.len(), 1);
    assert!(wiggles[0].text.chars().count() >= 3, "{}", wiggles[0].text);
}

/// A glissando over a system break keeps its gradient across its pieces
/// (Gould, p. 142; MuseScore interpolates across the break).
#[test]
fn a_glissando_over_a_system_break_keeps_its_gradient() {
    let loaded = part(
        "glissando_break.musicxml",
        &[
            format!(
                "{}{}{}{}",
                q("C5"),
                q("D5"),
                q("E5"),
                note(
                    "C4",
                    2,
                    "quarter",
                    "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>"
                ),
            ),
            format!(
                "<print new-system=\"yes\"/>{}{}{}{}",
                note("C6", 2, "quarter", "<slide type=\"stop\" number=\"1\"/>"),
                q("A5"),
                q("G5"),
                q("F5"),
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let pieces = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    let slope = |s: &Stroke| (s.to.y.0 - s.from.y.0) / (s.to.x.0 - s.from.x.0);
    near(slope(pieces[0]), slope(pieces[1]), "one gradient");
    assert!(slope(pieces[0]) > 0.0, "rising");
}

/// A glissando's piece in the system its end note opens starts a quarter
/// space after the ink of the system's lead and ends a quarter space before
/// its chord's ink, an accidental's included, as MuseScore opens a later
/// segment before the first column's leading ink; it had started at the
/// column itself, running back over the accidental.
#[test]
fn a_glissando_continued_into_a_system_stands_between_its_lead_and_its_chord() {
    let loaded = part(
        "glissando_break_flat.musicxml",
        &[
            note(
                "C5",
                8,
                "whole",
                "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>",
            ),
            format!(
                "<print new-system=\"yes\"/>{}",
                note("Bb4", 8, "whole", "<slide type=\"stop\" number=\"1\"/>"),
            ),
            note("C5", 8, "whole", ""),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let pieces = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    let later = pieces
        .iter()
        .max_by(|a, b| a.from.y.0.total_cmp(&b.from.y.0).reverse())
        .expect("the later piece, in the lower system");
    let flat = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "accidentalFlat")
        .map(glyph_box)
        .next()
        .expect("the flat");
    let clef = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("gClef"))
        .map(glyph_box)
        .find(|c| c[1] < flat[3] && c[3] > flat[1])
        .expect("the lower system's clef");
    let (from, to) = (
        later.from.x.0.min(later.to.x.0),
        later.from.x.0.max(later.to.x.0),
    );
    assert!(
        from >= clef[2] + 0.25 - 0.01,
        "after the lead: {from} {clef:?}"
    );
    near(to, flat[0] - 0.25, "a quarter space before the flat");
}

/// Two glissandi between two-note chords join heads of one rank, the upper
/// line the upper heads and the lower the lower, as the `k`th of them takes
/// the `k`th head of each chord from the top (the model keeps no head): they
/// run side by side and never cross.
#[test]
fn parallel_glissandi_between_chords_join_heads_of_one_rank() {
    let chord = |low: &str, high: &str, start: bool| {
        let slide = |n: u8| {
            if start {
                format!("<slide type=\"start\" line-type=\"solid\" number=\"{n}\">gliss.</slide>")
            } else {
                format!("<slide type=\"stop\" number=\"{n}\"/>")
            }
        };
        format!(
            "{}{}",
            note(low, 4, "half", &slide(1)),
            note(high, 4, "half", &slide(2)).replace("<note>", "<note><chord/>"),
        )
    };
    let loaded = part(
        "glissandi_parallel.musicxml",
        &[
            format!("{}{}", chord("C4", "E4", true), chord("G5", "B5", false)),
            format!(
                "{}{}",
                q("C5"),
                note("C5", 6, "half", "").replace("<type>half</type>", "<type>half</type><dot/>")
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let mut lines = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(lines.len(), 2, "{lines:?}");
    lines.sort_by(|a, b| a.from.y.0.total_cmp(&b.from.y.0));
    let (lower, upper) = (lines[0], lines[1]);
    assert!(
        upper.to.y.0 > lower.to.y.0,
        "the upper line ends on the upper head: {upper:?} {lower:?}"
    );
    let rise = |s: &Stroke| (s.to.y.0 - s.from.y.0) / (s.to.x.0 - s.from.x.0);
    near(rise(upper), rise(lower), "side by side");
}

/// A pedal released at a measure's start ends 0.75 before the barline there
/// (`Pedal::linePos`), even where a clef change stands before the barline at
/// that time: its line does not stop short at the clef.
#[test]
fn a_pedal_released_at_a_clef_change_ends_before_the_barline() {
    let loaded = part(
        "pedal_clef_change.musicxml",
        &[
            format!(
                "{}{}{}{}{}",
                pedal("start", true),
                q("G4"),
                q("A4"),
                q("B4"),
                q("C5")
            ),
            format!(
                "<attributes><clef><sign>F</sign><line>4</line></clef></attributes>{}{}{}{}{}",
                pedal("stop", false),
                q("C3"),
                q("D3"),
                q("E3"),
                q("F3"),
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let (_, hooks) = split(&line_strokes(&loaded, &layout, is_pedal));
    assert_eq!(hooks.len(), 1, "{hooks:?}");
    let barline = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("barline"))
        .map(|g| g.position.x.0)
        .fold(f32::INFINITY, f32::min);
    let clef = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("fClef"))
        .map(glyph_box)
        .next()
        .expect("the clef change");
    assert!(
        clef[2] <= barline + 1e-3,
        "the clef before the barline: {clef:?} {barline}"
    );
    near(
        hooks[0].0,
        barline - 0.75,
        "the hook 0.75 before the barline",
    );
}

/// A glissando's piece in a later system never runs back: on a system
/// whose lead holds nothing, as one would once a clef marked hidden is
/// honoured (the importer draws every clef today, so the state is built by
/// hand), the piece's chord leaves it no room after the lead, and it draws
/// nothing rather than a line from its end back to its start.
#[test]
fn a_glissando_piece_after_an_empty_lead_never_runs_back() {
    use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};
    let loaded = part(
        "glissando_empty_lead.musicxml",
        &[
            note(
                "C5",
                8,
                "whole",
                "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>",
            ),
            format!(
                "<print new-system=\"yes\"/>{}",
                note("Bb4", 8, "whole", "<slide type=\"stop\" number=\"1\"/>"),
            ),
            note("C5", 8, "whole", ""),
        ],
    );
    let score = &loaded.reduced.score;
    let mut constrained = to_constrained(&to_logical(score));
    for lead in &mut constrained.system_leads {
        for (_, glyphs) in &mut lead.entries {
            glyphs.clear();
        }
    }
    let layout = epiphany_engrave::Engraver::with_geometry(epiphany_cli::page_of(score))
        .solve(&constrained, &SolverConfig::default())
        .layout;
    let clefs = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("gClef"))
        .count();
    assert_eq!(clefs, 1, "the later system's lead holds nothing");
    let pieces = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert!(!pieces.is_empty(), "the first piece");
    for piece in pieces {
        assert!(
            piece.to.x.0 >= piece.from.x.0,
            "a piece running back: {piece:?}"
        );
    }
}

/// A glissando whose notes stand on two staves is left undrawn: each end's
/// ink is read on the line's own staff, which holds no head of the other
/// staff's note.
#[test]
fn a_glissando_between_two_staves_is_left_undrawn() {
    let xml = "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>\
         A</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef>\
         <clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes><note><pitch>\
         <step>C</step><octave>5</octave></pitch><duration>4</duration><voice>1</voice>\
         <type>half</type><staff>1</staff><notations><slide type=\"start\" line-type=\"solid\" \
         number=\"1\">gliss.</slide></notations></note><note><rest/><duration>4</duration>\
         <voice>1</voice><type>half</type><staff>1</staff></note><backup><duration>8\
         </duration></backup><note><rest/><duration>4</duration><voice>5</voice><type>half\
         </type><staff>2</staff></note><note><pitch><step>C</step><octave>3</octave></pitch>\
         <duration>4</duration><voice>5</voice><type>half</type><staff>2</staff><notations>\
         <slide type=\"stop\" number=\"1\"/></notations></note></measure></part>\
         </score-partwise>";
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("glissando_two_staves.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert_eq!(
        loaded
            .reduced
            .score
            .cross_cutting
            .spanners
            .iter()
            .filter(|s| matches!(s.kind, SpannerKind::Glissando))
            .count(),
        1,
        "the glissando imported"
    );
    let layout = engrave_loaded(&loaded).layout;
    let drawn = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert!(drawn.is_empty(), "{drawn:?}");
}

/// An ottava starting at a grace note sets its numeral just left of the
/// grace, the first note it applies to (Gould, *Behind Bars*, p. 28, as
/// LilyPond 2.26.0 starts it), and runs its line from there, where MuseScore
/// starts it at the principal: the numeral's ink at the grace's head, before
/// the principal's, the line from half a space after the numeral, and the
/// grace drawn an octave nearer the staff with the notes it opens.
#[test]
fn an_ottava_starting_at_a_grace_sets_its_numeral_at_the_grace() {
    let loaded = part(
        "ottava_grace.musicxml",
        &[format!(
            "{}{}{}{}{}{}",
            octave_shift("down", 8),
            "<note><grace slash=\"yes\"/><pitch><step>D</step><octave>6</octave></pitch>\
             <voice>1</voice><type>eighth</type></note>",
            note("C6", 2, "quarter", ""),
            note("E6", 2, "quarter", ""),
            note("G6", 4, "half", ""),
            octave_shift("stop", 8),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let Some(Event::Pitched(g)) = loaded.reduced.score.events.get(e[0]) else {
        panic!("the grace");
    };
    let grace_head = layout
        .texts
        .iter()
        .filter(|t| matches!(t.provenance.source, TypedObjectId::Pitch(p) if g.pitches.iter().any(|q| q.id == p)))
        .map(text_box)
        .next()
        .expect("the grace's head");
    let (_, top) = staves(&layout)[0];
    // D6 under an 8va, written D5: on the staff's fourth line.
    near(
        (grace_head[1] + grace_head[3]) * 0.5,
        top - 1.0,
        "the grace at its written place",
    );
    let numeral = runs(&layout, OTTAVA);
    assert_eq!(numeral.len(), 1);
    let numeral = text_box(numeral[0]);
    let principal = head(&loaded, &layout, e[1]);
    near(numeral[0], grace_head[0], "the numeral at the grace's head");
    assert!(
        numeral[0] < principal[0] - 0.5,
        "before the principal: {numeral:?} {principal:?}"
    );
    let strokes = line_strokes(&loaded, &layout, |k| {
        matches!(k, SpannerKind::OctaveLine(_))
    });
    let (dashes, _) = split(&strokes);
    let start = dashes.iter().map(|d| d.0).fold(f32::INFINITY, f32::min);
    near(
        start,
        numeral[2] + 0.5,
        "the line from half a space after the numeral",
    );
}

/// Only a grace that opens an ottava on its own staff moves its numeral: an
/// ottava whose first note has no grace, though a later note under it has,
/// and one whose start holds a grace on the other staff of a grand staff,
/// each set the numeral at their first note's head.
#[test]
fn only_a_grace_opening_an_ottava_on_its_staff_moves_its_numeral() {
    let grace = "<note><grace slash=\"yes\"/><pitch><step>D</step><octave>6</octave></pitch>\
                 <voice>1</voice><type>eighth</type></note>";
    let later = part(
        "ottava_later_grace.musicxml",
        &[format!(
            "{}{}{}{}{}{}",
            octave_shift("down", 8),
            note("C6", 2, "quarter", ""),
            grace,
            note("E6", 2, "quarter", ""),
            note("G6", 4, "half", ""),
            octave_shift("stop", 8),
        )],
    );
    let layout = engrave_loaded(&later).layout;
    let numeral = text_box(runs(&layout, OTTAVA)[0]);
    let first = head(&later, &layout, events(&later)[0]);
    near(
        numeral[0],
        first[0],
        "a later grace leaves the numeral at its note",
    );

    let xml = "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef>\
         <clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>\
         <direction placement=\"above\"><direction-type><octave-shift type=\"down\" size=\"8\"/>\
         </direction-type><staff>1</staff></direction>\
         <note><pitch><step>C</step><octave>6</octave></pitch><duration>8</duration>\
         <voice>1</voice><type>whole</type><staff>1</staff></note>\
         <direction><direction-type><octave-shift type=\"stop\" size=\"8\"/></direction-type>\
         <staff>1</staff></direction><backup><duration>8</duration></backup>\
         <note><grace slash=\"yes\"/><pitch><step>D</step><octave>3</octave></pitch>\
         <voice>5</voice><type>eighth</type><staff>2</staff></note>\
         <note><pitch><step>C</step><octave>3</octave></pitch><duration>8</duration>\
         <voice>5</voice><type>whole</type><staff>2</staff></note></measure></part>\
         </score-partwise>";
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ottava_other_staff_grace.musicxml");
    std::fs::write(&path, xml).expect("written");
    let grand = load(&path).expect("loads");
    let layout = engrave_loaded(&grand).layout;
    let numeral = text_box(runs(&layout, OTTAVA)[0]);
    let first = head(&grand, &layout, events(&grand)[0]);
    near(
        numeral[0],
        first[0],
        "the other staff's grace leaves the numeral at its note",
    );
}

/// Glissandi between the same two chords take one "gliss.", as Gould gives a
/// set of parallel lines one instruction whatever the number of notes
/// (*Behind Bars*, p. 143), where MuseScore draws one on each: two lines,
/// their words along the upper alone.
#[test]
fn parallel_glissandi_take_one_gliss() {
    let chord = |low: &str, high: &str, start: bool| {
        let slide = |n: u8| {
            if start {
                format!("<slide type=\"start\" line-type=\"solid\" number=\"{n}\">gliss.</slide>")
            } else {
                format!("<slide type=\"stop\" number=\"{n}\"/>")
            }
        };
        format!(
            "{}{}",
            note(low, 4, "half", &slide(1)),
            note(high, 4, "half", &slide(2)).replace("<note>", "<note><chord/>"),
        )
    };
    let loaded = part(
        "glissandi_one_word.musicxml",
        &[
            format!("{}{}", chord("C4", "E4", true), chord("G5", "B5", false)),
            note("C5", 8, "whole", ""),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let mut lines = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(lines.len(), 2);
    lines.sort_by(|a, b| a.from.y.0.total_cmp(&b.from.y.0));
    let words = runs(&layout, "gliss.");
    assert_eq!(words.len(), 1, "one instruction");
    let w = text_box(words[0]);
    let upper = lines[1];
    let at = |x: f32| {
        upper.from.y.0
            + (upper.to.y.0 - upper.from.y.0) * (x - upper.from.x.0)
                / (upper.to.x.0 - upper.from.x.0)
    };
    assert!(
        w[1] >= at(w[0]) - 0.3,
        "along the upper line: {w:?} {upper:?}"
    );
}

/// A parallel set's one "gliss." stands on its topmost line long enough for
/// it (X4d fix round 2; Gould, *Behind Bars*, pp. 140 and 143): pairs from
/// E5 over C5 to an altered B4 over G4, quarters in a system of four
/// measures (C4d review 2's `q_sets2`). A double sharp ends the top line
/// short of the words, which then stand on the lower line, where the set had
/// carried none; a double flat, a flat or a sharp ends both lines short of
/// them, and those sets carry none. Where both lines hold the words they
/// stand on the top one (`parallel_glissandi_take_one_gliss`).
#[test]
fn a_parallel_sets_gliss_stands_on_its_topmost_line_long_enough() {
    let gliss = |n: u8, kind: &str| {
        let words = if kind == "start" { "gliss." } else { "" };
        format!("<glissando line-type=\"solid\" number=\"{n}\" type=\"{kind}\">{words}</glissando>")
    };
    let pitched = |step: &str, alter: i8, octave: u8, chord: bool, slide: &str| {
        let alter_xml = if alter == 0 {
            String::new()
        } else {
            format!("<alter>{alter}</alter>")
        };
        let accidental = match alter {
            -2 => "<accidental>flat-flat</accidental>",
            -1 => "<accidental>flat</accidental>",
            1 => "<accidental>sharp</accidental>",
            2 => "<accidental>double-sharp</accidental>",
            _ => "",
        };
        format!(
            "<note>{}<pitch><step>{step}</step>{alter_xml}<octave>{octave}</octave></pitch>\
             <duration>4</duration><voice>1</voice><type>quarter</type>{accidental}\
             <notations>{slide}</notations></note>",
            if chord { "<chord/>" } else { "" }
        )
    };
    let set = |alter: i8| {
        format!(
            "{}{}{}{}{}",
            pitched("E", 0, 5, false, &gliss(1, "start")),
            pitched("C", 0, 5, true, &gliss(2, "start")),
            pitched("B", alter, 4, false, &gliss(1, "stop")),
            pitched("G", 0, 4, true, &gliss(2, "stop")),
            note("C5", 8, "half", ""),
        )
    };
    let whole = note("C5", 16, "whole", "");
    let loaded = part_in(
        "parallel_glissandi_topmost_long_enough.musicxml",
        4,
        &[
            whole.clone(),
            format!("<print new-system=\"yes\"/>{}", set(-2)),
            set(-1),
            set(1),
            set(2),
            format!("<print new-system=\"yes\"/>{whole}"),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let mut lines = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(lines.len(), 8, "four sets of two lines");
    lines.sort_by(|a, b| a.from.x.0.total_cmp(&b.from.x.0));
    // Each set's two lines, the top one first.
    let sets: Vec<[&Stroke; 2]> = lines
        .chunks(2)
        .map(|pair| {
            assert!(
                (pair[0].from.x.0 - pair[1].from.x.0).abs() < 1.5,
                "a set's lines start together: {pair:?}"
            );
            if pair[0].from.y.0 >= pair[1].from.y.0 {
                [pair[0], pair[1]]
            } else {
                [pair[1], pair[0]]
            }
        })
        .collect();
    let words = runs(&layout, "gliss.");
    for (k, [top, lower]) in sets.iter().enumerate() {
        let on: Vec<&str> = words
            .iter()
            .filter_map(|w| {
                if w.provenance.source == top.provenance.source {
                    Some("top")
                } else if w.provenance.source == lower.provenance.source {
                    Some("lower")
                } else {
                    None
                }
            })
            .collect();
        let want: &[&str] = if k == 3 { &["lower"] } else { &[] };
        assert_eq!(on, want, "set {k}: its gliss. on {want:?}");
    }
}

/// A glissando's words are kept off barlines and other ink (X4d fix round
/// 1): along a line nearly level from a measure's last note over the
/// barline and a new time signature to the next measure's first, "gliss.",
/// which centred along it stands on the barline, slides along the line or
/// off it until it meets no ink, as Gould sets the words close to the line
/// and legible (*Behind Bars*, p. 141).
#[test]
fn a_glissandos_words_stand_off_the_barline_it_crosses() {
    let sixteenths =
        |pitches: &[&str]| -> String { pitches.iter().map(|p| note(p, 1, "16th", "")).collect() };
    let loaded = part_in(
        "glissando_words_barline.musicxml",
        4,
        &[
            format!(
                "{}{}",
                sixteenths(&["C5", "D5", "E5", "F5", "G5", "A5", "B5", "C6", "B5", "A5", "G5", "F5"]),
                note(
                    "C5",
                    4,
                    "quarter",
                    "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>"
                ),
            ),
            format!(
                "<attributes><time><beats>3</beats><beat-type>4</beat-type></time></attributes>{}{}",
                note("D5", 4, "quarter", "<slide type=\"stop\" number=\"1\"/>"),
                sixteenths(&["F5", "G5", "A5", "B5", "C6", "B5", "A5", "G5"]),
            ),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let words = runs(&layout, "gliss.");
    assert_eq!(words.len(), 1, "its words");
    let w = text_box(words[0]);
    for g in layout
        .glyphs
        .iter()
        .filter(|g| !g.glyph.as_str().starts_with("noteheadNull"))
    {
        let b = glyph_box(g);
        let meets = w[0] < b[2] && b[0] < w[2] && w[1] < b[3] && b[1] < w[3];
        assert!(!meets, "the words {w:?} on {} {b:?}", g.glyph.as_str());
    }
    let pieces = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(pieces.len(), 1);
    let line = pieces[0];
    let (cx, cy) = ((w[0] + w[2]) * 0.5, (w[1] + w[3]) * 0.5);
    assert!(
        cx > line.from.x.0 && cx < line.to.x.0,
        "the words along their line: {w:?} {line:?}"
    );
    let at = line.from.y.0
        + (line.to.y.0 - line.from.y.0) * (cx - line.from.x.0) / (line.to.x.0 - line.from.x.0);
    assert!(cy > at, "over it: {w:?} {line:?}");
}

/// A glissando's start is read again where its line would cross its first
/// column's other ink, any voice's (X4d fix round 1): from the upper of two
/// voices a second apart, the lower voice's head set beside it, the line
/// falling from it starts a quarter space past that head rather than running
/// through it; it had read its own note's ink alone.
#[test]
fn a_glissando_starts_past_a_head_its_line_would_cross() {
    let voiced = |pitch: &str, voice: u8, stem: &str, notations: &str| {
        note(pitch, 4, "half", notations)
            .replace("<voice>1</voice>", &format!("<voice>{voice}</voice>"))
            .replace("</type>", &format!("</type><stem>{stem}</stem>"))
    };
    let loaded = part(
        "glissando_beside_a_voice.musicxml",
        &[
            format!(
                "{}{}<backup><duration>8</duration></backup>{}{}",
                voiced(
                    "G5",
                    1,
                    "up",
                    "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>"
                ),
                voiced("C5", 1, "up", "<slide type=\"stop\" number=\"1\"/>"),
                voiced("F5", 2, "down", ""),
                voiced("D5", 2, "down", ""),
            ),
            note("C5", 8, "whole", ""),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let own = head(&loaded, &layout, e[0]);
    let other = head(&loaded, &layout, e[2]);
    assert!(
        (other[0] - own[0]).abs() > 0.5,
        "the voices' heads side by side: {other:?} {own:?}"
    );
    let lines = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
    assert_eq!(lines.len(), 1);
    let line = lines[0];
    let y = |x: f32| {
        line.from.y.0
            + (line.to.y.0 - line.from.y.0) * (x - line.from.x.0) / (line.to.x.0 - line.from.x.0)
    };
    let (l, r) = (other[0].max(line.from.x.0), other[2].min(line.to.x.0));
    let crosses = l < r && y(l).min(y(r)) < other[3] && y(l).max(y(r)) > other[1];
    assert!(
        !crosses,
        "the line through the other voice's head: {line:?} {other:?}"
    );
}

/// A glissando's start keeps the quarter space it keeps from its chords' ink
/// from a head beside its own too (X4d fix round 1), on either side of the
/// line: from the lower of two voices a second apart, the upper's dotted head
/// set right of it, the line falling from just under that head; and from the
/// upper, the lower's head set right of it, the line rising from just over
/// that head; and a line falling further, which would begin nearer that head
/// than the quarter space though clear of it by its width. Each starts past
/// the head beside its own rather than grazing it.
#[test]
fn a_glissando_starts_past_a_head_it_would_graze() {
    let voiced = |pitch: &str, duration: u8, kind: &str, voice: u8, stem: &str, notations: &str| {
        note(pitch, duration, kind, notations)
            .replace("<voice>1</voice>", &format!("<voice>{voice}</voice>"))
            .replace("</type>", &format!("</type><stem>{stem}</stem>"))
    };
    let start = "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>";
    let stop = "<slide type=\"stop\" number=\"1\"/>";
    // (the measure, the index of the glissando's first note, of the head
    // beside it)
    let cases = [
        (
            format!(
                "{}{}<backup><duration>8</duration></backup>{}{}",
                voiced("A5", 6, "half", 1, "up", "").replace("</type>", "</type><dot/>"),
                voiced("C6", 2, "quarter", 1, "up", ""),
                voiced("G5", 4, "half", 2, "down", start),
                voiced("F5", 4, "half", 2, "down", stop),
            ),
            2,
            0,
        ),
        // Falling further, the line stands a little under that head where it
        // would begin, nearer than the quarter space but clear of its width.
        (
            format!(
                "{}{}<backup><duration>8</duration></backup>{}{}",
                voiced("A5", 6, "half", 1, "up", "").replace("</type>", "</type><dot/>"),
                voiced("C6", 2, "quarter", 1, "up", ""),
                voiced("G5", 4, "half", 2, "down", start),
                voiced("D5", 4, "half", 2, "down", stop),
            ),
            2,
            0,
        ),
        (
            format!(
                "{}{}<backup><duration>8</duration></backup>{}{}",
                voiced("G5", 4, "half", 1, "up", start),
                voiced("A5", 4, "half", 1, "up", stop),
                voiced("F5", 4, "half", 2, "down", ""),
                voiced("D5", 4, "half", 2, "down", ""),
            ),
            0,
            2,
        ),
    ];
    for (measure, own, other) in cases {
        let loaded = part(
            "glissando_grazing_a_voice.musicxml",
            &[measure, note("C5", 8, "whole", "")],
        );
        let layout = engrave_loaded(&loaded).layout;
        let e = events(&loaded);
        let other = head(&loaded, &layout, e[other]);
        let own = head(&loaded, &layout, e[own]);
        assert!(
            other[0] > own[0] + 0.5,
            "the other voice's head set right of the glissando's: {other:?} {own:?}"
        );
        let lines = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
        assert_eq!(lines.len(), 1);
        let line = lines[0];
        assert!(
            line.from.x.0 >= other[2] + 0.2,
            "the line starts past the head beside its own: {line:?} {other:?}"
        );
    }
}

/// A glissando's end keeps the quarter space it keeps from its chords' ink
/// from another voice's head beside its own too (X4d fix round 2), as its
/// start does: a line from a quarter to a dotted half in the upper voice,
/// the lower voice's undotted half a second under it set left of it (version
/// 89), rising or falling to it (C4d review 2's `q_gliss_end2`). Each ends
/// before the other voice's head rather than in it or grazing it; Gould keeps
/// the line between the notes it joins (*Behind Bars*, p. 141).
#[test]
fn a_glissando_ends_before_a_head_beside_its_own() {
    let voiced = |pitch: &str, duration: u8, kind: &str, voice: u8, stem: &str, notations: &str| {
        note(pitch, duration, kind, notations)
            .replace("<voice>1</voice>", &format!("<voice>{voice}</voice>"))
            .replace("</type>", &format!("</type><stem>{stem}</stem>"))
    };
    let rest = "<note><rest/><duration>2</duration><voice>2</voice><type>quarter</type></note>";
    let start = "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>";
    let stop = "<slide type=\"stop\" number=\"1\"/>";
    for (from, to, low) in [
        ("C5", "F5", "E5"),
        ("A4", "E5", "D5"),
        ("D4", "A4", "G4"),
        ("G5", "C5", "B4"),
        ("B5", "F5", "E5"),
    ] {
        let measure = format!(
            "{}{}<backup><duration>8</duration></backup>{rest}{}{rest}",
            voiced(from, 2, "quarter", 1, "up", start),
            voiced(to, 6, "half", 1, "up", stop).replace("</type>", "</type><dot/>"),
            voiced(low, 4, "half", 2, "down", ""),
        );
        let loaded = part(
            "glissando_end_beside_a_voice.musicxml",
            &[measure, note("C5", 8, "whole", "")],
        );
        let layout = engrave_loaded(&loaded).layout;
        let e = events(&loaded);
        let own = head(&loaded, &layout, e[1]);
        let other = head(&loaded, &layout, e[3]);
        assert!(
            other[2] < own[0] + 0.5 && other[0] < own[0] - 0.5,
            "{from} to {to}: the other voice's head set left of the end's: {other:?} {own:?}"
        );
        let lines = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando));
        assert_eq!(lines.len(), 1, "{from} to {to}");
        let line = lines[0];
        assert!(
            line.to.x.0 <= other[0] - 0.2,
            "{from} to {to}: the line ends before the head beside its own: {line:?} {other:?}"
        );
    }
}

/// A glissando's words with no other ink in their way stay centred along
/// their line, their own line never one they keep off.
#[test]
fn a_glissandos_words_clear_of_ink_stay_centred_on_it() {
    let loaded = part(
        "glissando_words_free.musicxml",
        &[format!(
            "{}{}{}",
            note(
                "C4",
                4,
                "half",
                "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>"
            ),
            note("C6", 2, "quarter", "<slide type=\"stop\" number=\"1\"/>"),
            q("B5"),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let w = text_box(runs(&layout, "gliss.")[0]);
    let line = line_strokes(&loaded, &layout, |k| matches!(k, SpannerKind::Glissando))[0];
    // Their middle, read along the line, at its middle.
    let (dx, dy) = (line.to.x.0 - line.from.x.0, line.to.y.0 - line.from.y.0);
    let length = (dx * dx + dy * dy).sqrt();
    let (cx, cy) = ((w[0] + w[2]) * 0.5, (w[1] + w[3]) * 0.5);
    let along = ((cx - line.from.x.0) * dx + (cy - line.from.y.0) * dy) / length;
    near(along, length * 0.5, "centred along it");
}
