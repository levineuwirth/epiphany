//! Grace notes on the page (X4d.2), each on a hand-written file, against the
//! rules `epiphany-layout-ir`'s `graces.rs` cites: Gould's (*Behind Bars*,
//! pp. 84 and 125 to 130), MuseScore 4.7.5's where she gives no figure, and
//! the owner's ruling of her 2¼-space stem.

use std::path::Path;

use epiphany_cli::{engrave_loaded, load, Loaded};
use epiphany_core::{Event, EventId, TypedObjectId};
use epiphany_layout_ir::{ResolvedLayoutIR, ResolvedText, Stroke};

/// A part of one treble staff in 4/4, `divisions` to the quarter, its
/// measures as written.
fn part(name: &str, divisions: u8, measures: &[String]) -> Loaded {
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

/// A pitch as `<pitch>` and its `<accidental>`: `"C5"`, `"F#4"`, `"Bb4"`,
/// `"Fn4"` (a natural shown).
fn pitch(p: &str) -> (String, &'static str) {
    let (step, rest) = p.split_at(1);
    let (alter, accidental, octave) = match rest.chars().next() {
        Some('#') => (
            "<alter>1</alter>",
            "<accidental>sharp</accidental>",
            &rest[1..],
        ),
        Some('b') => (
            "<alter>-1</alter>",
            "<accidental>flat</accidental>",
            &rest[1..],
        ),
        Some('n') => ("", "<accidental>natural</accidental>", &rest[1..]),
        _ => ("", "", rest),
    };
    (
        format!("<pitch><step>{step}</step>{alter}<octave>{octave}</octave></pitch>"),
        accidental,
    )
}

/// A grace note: slashed (an acciaccatura) or not, its value, its beam
/// (`"begin"`, `"end"` or none), its voice.
fn grace(p: &str, slash: bool, kind: &str, beam: Option<&str>, voice: u8) -> String {
    let (pitch, accidental) = pitch(p);
    let slash = if slash { "yes" } else { "no" };
    let beam = beam.map_or(String::new(), |b| format!("<beam number=\"1\">{b}</beam>"));
    format!(
        "<note><grace slash=\"{slash}\"/>{pitch}<voice>{voice}</voice><type>{kind}</type>\
         {accidental}{beam}</note>"
    )
}

/// A measured note.
fn note(p: &str, duration: u8, kind: &str, voice: u8) -> String {
    let (pitch, accidental) = pitch(p);
    format!(
        "<note>{pitch}<duration>{duration}</duration><voice>{voice}</voice><type>{kind}</type>\
         {accidental}</note>"
    )
}

fn events(loaded: &Loaded) -> Vec<EventId> {
    loaded.import.ids.events[0].clone()
}

/// An ink box, `[left, bottom, right, top]`.
type Ink = [f32; 4];

fn text_box(text: &ResolvedText) -> Ink {
    let (x, y, b) = (text.origin.x.0, text.origin.y.0, &text.bounds);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

/// The pitches of an event, as sources.
fn sources(loaded: &Loaded, event: EventId) -> Vec<TypedObjectId> {
    let mut out = vec![TypedObjectId::Event(event)];
    if let Some(Event::Pitched(p)) = loaded.reduced.score.events.get(event) {
        out.extend(p.pitches.iter().map(|ip| TypedObjectId::Pitch(ip.id)));
    }
    out
}

/// A grace's small head: the run traced to its pitch.
fn grace_head(loaded: &Loaded, layout: &ResolvedLayoutIR, event: EventId) -> Ink {
    let Some(Event::Pitched(p)) = loaded.reduced.score.events.get(event) else {
        panic!("a pitched grace");
    };
    let found: Vec<Ink> = layout
        .texts
        .iter()
        .filter(|t| {
            p.pitches
                .iter()
                .any(|ip| t.provenance.source == TypedObjectId::Pitch(ip.id))
        })
        .map(text_box)
        .collect();
    assert_eq!(found.len(), 1, "one small head");
    found[0]
}

/// Every run an event draws (heads, accidentals, its flag), left to right.
fn grace_runs<'a>(
    loaded: &Loaded,
    layout: &'a ResolvedLayoutIR,
    event: EventId,
) -> Vec<&'a ResolvedText> {
    let sources = sources(loaded, event);
    let mut found: Vec<&ResolvedText> = layout
        .texts
        .iter()
        .filter(|t| sources.contains(&t.provenance.source))
        .collect();
    found.sort_by(|a, b| a.origin.x.0.total_cmp(&b.origin.x.0));
    found
}

/// Every stroke an event draws.
fn strokes(layout: &ResolvedLayoutIR, event: EventId) -> Vec<&Stroke> {
    layout
        .strokes
        .iter()
        .filter(|s| s.provenance.source == TypedObjectId::Event(event) && s.from != s.to)
        .collect()
}

/// An event's stem: its x and its ends, lower first.
fn stem(layout: &ResolvedLayoutIR, event: EventId) -> (f32, f32, f32) {
    let found: Vec<(f32, f32, f32)> = strokes(layout, event)
        .into_iter()
        .filter(|s| s.from.x == s.to.x)
        .map(|s| {
            (
                s.from.x.0,
                s.from.y.0.min(s.to.y.0),
                s.from.y.0.max(s.to.y.0),
            )
        })
        .collect();
    assert_eq!(found.len(), 1, "one stem of {event:?}");
    found[0]
}

/// A full-size head of a measured note.
fn head(loaded: &Loaded, layout: &ResolvedLayoutIR, event: EventId) -> Ink {
    let sources = sources(loaded, event);
    let found: Vec<Ink> = layout
        .glyphs
        .iter()
        .filter(|g| {
            sources.contains(&g.provenance.source) && g.glyph.as_str().starts_with("notehead")
        })
        .map(|g| {
            let (x, y, b) = (g.position.x.0, g.position.y.0, &g.bounding_box);
            [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
        })
        .collect();
    assert_eq!(found.len(), 1, "one head");
    found[0]
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} against {b}");
}

/// A single acciaccatura (Gould, pp. 125 and 126; MuseScore's
/// `graceNoteMag`, `stemSlashAngle`, `stemSlashPosition`): its head and flag
/// at 0.7 of their size, its stem up 2¼ spaces from its head's centre, its
/// slash rising at 40° across its stem from 1.4 under the tip to the flag's
/// right edge; an appoggiatura has no slash; a grace of a lower part turns
/// its stem down (p. 126).
#[test]
fn a_single_grace_is_small_stemmed_and_slashed() {
    let loaded = part(
        "grace_single.musicxml",
        2,
        &[format!(
            "{}{}{}{}{}{}",
            grace("G4", true, "eighth", None, 1),
            note("A4", 2, "quarter", 1),
            grace("D5", false, "eighth", None, 1),
            note("C5", 2, "quarter", 1),
            note("B4", 2, "quarter", 1),
            note("A4", 2, "quarter", 1),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let h = grace_head(&loaded, &layout, e[0]);
    near(h[3] - h[1], 0.7, "a head 0.7 high");
    let (x, low, high) = stem(&layout, e[0]);
    let centre = (h[1] + h[3]) * 0.5;
    near(low, centre, "the stem from the head");
    near(high - centre, 2.25, "a stem 2¼ long");
    near(x, h[2] - 0.042, "on the head's right edge");
    let slashes: Vec<&Stroke> = strokes(&layout, e[0])
        .into_iter()
        .filter(|s| s.from.x != s.to.x && s.from.y != s.to.y)
        .collect();
    assert_eq!(slashes.len(), 1, "one slash");
    let s = slashes[0];
    near(s.from.y.0, high - 1.4, "1.4 under the tip");
    let rise = (s.to.y.0 - s.from.y.0) / (s.to.x.0 - s.from.x.0);
    near(rise, 40f32.to_radians().tan(), "at 40°");
    assert!(s.from.x.0 < x && s.to.x.0 > x, "across the stem");
    let flag = grace_runs(&loaded, &layout, e[0])
        .into_iter()
        .find(|t| t.text == "\u{E240}")
        .expect("a small flag");
    near(s.to.x.0, text_box(flag)[2], "to the flag's right edge");
    // The appoggiatura: no slash.
    let plain: Vec<&Stroke> = strokes(&layout, e[2])
        .into_iter()
        .filter(|s| s.from.x != s.to.x && s.from.y != s.to.y)
        .collect();
    assert!(plain.is_empty(), "an appoggiatura has no slash");
}

#[test]
fn a_lower_parts_grace_turns_its_stem_down() {
    let loaded = part(
        "grace_lower.musicxml",
        2,
        &[format!(
            "{}{}<backup><duration>8</duration></backup>{}{}",
            note("C5", 8, "whole", 1),
            "",
            grace("E4", true, "eighth", None, 2),
            note("F4", 8, "whole", 2),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let g = e[1];
    let h = grace_head(&loaded, &layout, g);
    let (_, low, high) = stem(&layout, g);
    let centre = (h[1] + h[3]) * 0.5;
    near(high, centre, "the stem from the head");
    near(centre - low, 2.25, "down 2¼");
}

/// A group stands right to left before its principal (MuseScore's
/// `graceToGraceNoteDist`, `graceToMainNoteDist`): its last grace's ink 0.45
/// clear of the principal's, its accidental included, each before it 0.3
/// clear of the next; a run one beam joins is beamed, its stems reaching the
/// beam, the slash on its first stem alone (Gould, p. 126).
#[test]
fn a_group_is_beamed_and_stands_right_to_left_before_its_principal() {
    let loaded = part(
        "grace_group.musicxml",
        2,
        &[format!(
            "{}{}{}{}{}{}",
            grace("C5", true, "16th", Some("begin"), 1),
            grace("D5", true, "16th", Some("end"), 1),
            note("F#5", 2, "quarter", 1),
            note("C5", 2, "quarter", 1),
            note("B4", 2, "quarter", 1),
            note("A4", 2, "quarter", 1),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let sharp = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "accidentalSharp")
        .map(|g| g.position.x.0 + g.bounding_box.left.0)
        .expect("the principal's sharp");
    let ink_right = |event: EventId| {
        grace_runs(&loaded, &layout, event)
            .iter()
            .map(|t| text_box(t)[2])
            .chain(
                strokes(&layout, event)
                    .iter()
                    .map(|s| s.from.x.0.max(s.to.x.0) + s.thickness.0 * 0.5),
            )
            .fold(f32::NEG_INFINITY, f32::max)
    };
    let ink_left = |event: EventId| {
        grace_runs(&loaded, &layout, event)
            .iter()
            .map(|t| text_box(t)[0])
            .chain(
                strokes(&layout, event)
                    .iter()
                    .map(|s| s.from.x.0.min(s.to.x.0) - s.thickness.0 * 0.5),
            )
            .fold(f32::INFINITY, f32::min)
    };
    // The beam runs from the first grace's stem to the second's: its ink is
    // the first's, so the second's alone is read for its right edge.
    let second_right = grace_runs(&loaded, &layout, e[1])
        .iter()
        .map(|t| text_box(t)[2])
        .chain(std::iter::once(stem(&layout, e[1]).0 + 0.042))
        .fold(f32::NEG_INFINITY, f32::max);
    near(
        sharp - second_right,
        0.45,
        "0.45 before the principal's sharp",
    );
    let first_right = grace_runs(&loaded, &layout, e[0])
        .iter()
        .map(|t| text_box(t)[2])
        .chain(std::iter::once(stem(&layout, e[0]).0 + 0.042))
        .fold(f32::NEG_INFINITY, f32::max);
    near(ink_left(e[1]) - first_right, 0.3, "0.3 between the graces");
    let _ = ink_right;
    // The beams: two, a sixteenth's, from the first stem to the second.
    let beams: Vec<&Stroke> = strokes(&layout, e[0])
        .into_iter()
        .filter(|s| s.thickness.0 > 0.3)
        .collect();
    assert_eq!(beams.len(), 2, "two beams");
    let (x0, _, top0) = stem(&layout, e[0]);
    let (x1, _, top1) = stem(&layout, e[1]);
    let beam = beams
        .iter()
        .max_by(|a, b| a.from.y.0.total_cmp(&b.from.y.0))
        .expect("the outer beam");
    let at = |x: f32| {
        beam.from.y.0
            + (beam.to.y.0 - beam.from.y.0) * (x - beam.from.x.0) / (beam.to.x.0 - beam.from.x.0)
    };
    near(top0, at(x0) + 0.175, "the first stem to the beam's top");
    near(top1, at(x1) + 0.175, "the second stem to the beam's top");
    let slashed = |event: EventId| {
        strokes(&layout, event)
            .into_iter()
            .filter(|s| s.from.x != s.to.x && s.from.y != s.to.y && s.thickness.0 < 0.3)
            .count()
    };
    assert_eq!(
        (slashed(e[0]), slashed(e[1])),
        (1, 0),
        "a slash on the first stem alone"
    );
}

/// Accidentals hold for grace and measured notes alike (Gould, p. 84): a
/// grace's sharp holds for the measured note after it on its line, and a
/// grace's natural cancels a measured note's earlier flat, drawn small.
#[test]
fn a_graces_accidental_joins_its_measure() {
    let loaded = part(
        "grace_accidentals.musicxml",
        2,
        &[format!(
            "{}{}{}{}{}{}",
            grace("F#5", true, "eighth", None, 1),
            note("F#5", 2, "quarter", 1),
            note("Bb4", 2, "quarter", 1),
            grace("Bn4", true, "eighth", None, 1),
            note("C5", 2, "quarter", 1),
            note("C5", 2, "quarter", 1),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let full: Vec<&str> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("accidental"))
        .map(|g| g.glyph.as_str())
        .collect();
    assert_eq!(
        full,
        vec!["accidentalFlat"],
        "the F♯ after its grace shows none"
    );
    let small: Vec<&str> = layout
        .texts
        .iter()
        .filter(|t| t.text == "\u{E262}" || t.text == "\u{E261}")
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(
        small,
        vec!["\u{E262}", "\u{E261}"],
        "the graces' sharp and natural, small"
    );
}

/// An unpitched grace (a flam) is drawn small at its staff position.
#[test]
fn an_unpitched_grace_is_drawn() {
    let xml = "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>\
         Snare</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>percussion</sign></clef></attributes><note><grace slash=\"no\"/>\
         <unpitched><display-step>C</display-step><display-octave>5</display-octave>\
         </unpitched><voice>1</voice><type>eighth</type></note><note><unpitched>\
         <display-step>C</display-step><display-octave>5</display-octave></unpitched>\
         <duration>4</duration><voice>1</voice><type>whole</type></note></measure></part>\
         </score-partwise>";
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("grace_unpitched.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let runs: Vec<&ResolvedText> = layout
        .texts
        .iter()
        .filter(|t| t.provenance.source == TypedObjectId::Event(e[0]) && t.text == "\u{E0A4}")
        .collect();
    assert_eq!(runs.len(), 1, "its head, small");
    let _ = stem(&layout, e[0]);
}

/// A grace group takes room in its principal's column (D79's Q13): before a
/// principal that follows sixteenths closely, the column stands further right
/// than without its graces, and the graces clear the note before.
#[test]
fn a_grace_group_takes_room_before_its_principal() {
    let sixteenths: String = (0..4).map(|_| note("C5", 1, "16th", 1)).collect();
    let rest = format!(
        "{}{}{}",
        note("A4", 4, "quarter", 1),
        note("B4", 4, "quarter", 1),
        note("C5", 4, "quarter", 1)
    );
    let graced = part(
        "grace_room.musicxml",
        4,
        &[format!(
            "{sixteenths}{}{}{}{}{rest}",
            grace("C5", true, "16th", Some("begin"), 1),
            grace("D5", true, "16th", None, 1)
                .replace("</note>", "<beam number=\"1\">continue</beam></note>"),
            grace("E5", true, "16th", Some("end"), 1),
            ""
        )],
    );
    let plain = part(
        "grace_room_plain.musicxml",
        4,
        &[format!("{sixteenths}{rest}")],
    );
    let x =
        |loaded: &Loaded, event: EventId| head(loaded, &engrave_loaded(loaded).layout, event)[0];
    let (eg, ep) = (events(&graced), events(&plain));
    assert!(
        x(&graced, eg[7]) > x(&plain, ep[4]) + 1.0,
        "the principal further right with its graces"
    );
    let layout = engrave_loaded(&graced).layout;
    let before = head(&graced, &layout, eg[3])[2];
    let left = grace_runs(&graced, &layout, eg[4])
        .iter()
        .map(|t| text_box(t)[0])
        .fold(f32::INFINITY, f32::min);
    assert!(left > before, "the graces clear the note before");
}

/// A grace's slash is kept off its ledger lines (Gould, p. 126): on a note
/// three ledger lines under the staff its stem is lengthened until the
/// slash's foot stands a quarter space above the highest of them.
#[test]
fn a_graces_slash_clears_its_ledger_lines() {
    let loaded = part(
        "grace_ledger.musicxml",
        2,
        &[format!(
            "{}{}{}{}{}",
            grace("E3", true, "eighth", None, 1),
            note("F3", 2, "quarter", 1),
            note("A4", 2, "quarter", 1),
            note("A4", 2, "quarter", 1),
            note("A4", 2, "quarter", 1),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let all = strokes(&layout, e[0]);
    let ledger_top = all
        .iter()
        .filter(|s| s.from.y == s.to.y)
        .map(|s| s.from.y.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let slash = all
        .iter()
        .find(|s| s.from.x != s.to.x && s.from.y != s.to.y)
        .expect("a slash");
    near(
        slash.from.y.0.min(slash.to.y.0),
        ledger_top + 0.25,
        "a quarter above its ledger lines",
    );
}

/// The leftmost ink of a group of graces: their runs, their stems and
/// ledger lines, their slashes and beams.
fn group_left(loaded: &Loaded, layout: &ResolvedLayoutIR, group: &[EventId]) -> f32 {
    group
        .iter()
        .flat_map(|&event| {
            grace_runs(loaded, layout, event)
                .into_iter()
                .map(|t| text_box(t)[0])
                .chain(
                    strokes(layout, event)
                        .into_iter()
                        .map(|s| s.from.x.0.min(s.to.x.0) - s.thickness.0 * 0.5),
                )
                .collect::<Vec<f32>>()
        })
        .fold(f32::INFINITY, f32::min)
}

/// The right edge of the barline glyph nearest before `x` whose staff holds
/// the height `y`.
fn barline_before(layout: &ResolvedLayoutIR, x: f32, y: f32) -> f32 {
    layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("barline"))
        .map(|g| {
            let (gx, gy, b) = (g.position.x.0, g.position.y.0, &g.bounding_box);
            [gx + b.left.0, gy + b.bottom.0, gx + b.right.0, gy + b.top.0]
        })
        .filter(|bar| bar[2] <= x + 0.01 && bar[1] - 3.0 < y && y < bar[3] + 3.0)
        .map(|bar| bar[2])
        .fold(f32::NEG_INFINITY, f32::max)
}

/// A grace at a bar's first beat stands after the barline before it at any
/// height, as Gould sets graces before the first beat of a bar (*Behind
/// Bars*, p. 127) and as MuseScore and LilyPond do: a slashed grace with
/// its flat on ledger lines over the staff, two under it, and a beamed pair
/// high over it, each group's leftmost ink (its accidental, ledger lines and
/// beam included) the barline's clearance, 0.65, right of the barline. Their ink, meeting
/// no barline ink at its height, had passed the barline into the bar before.
#[test]
fn a_grace_off_the_staff_at_a_bars_first_beat_stands_after_its_barline() {
    let eighths =
        |pitches: &[&str]| -> String { pitches.iter().map(|p| note(p, 1, "eighth", 1)).collect() };
    let loaded = part(
        "grace_after_barline.musicxml",
        2,
        &[
            eighths(&["C5", "D5", "E5", "F5", "G5", "A5", "B5", "C6"]),
            format!(
                "{}{}{}{}{}",
                grace("Eb7", true, "eighth", None, 1),
                note("C6", 2, "quarter", 1),
                note("B5", 2, "quarter", 1),
                note("A5", 2, "quarter", 1),
                note("G5", 2, "quarter", 1),
            ),
            eighths(&["C6", "B5", "A5", "G5", "F5", "E5", "D5", "C5"]),
            format!(
                "{}{}{}{}{}{}",
                grace("F3", true, "eighth", None, 1),
                grace("A3", true, "eighth", None, 1),
                note("C4", 2, "quarter", 1),
                note("D4", 2, "quarter", 1),
                note("E4", 2, "quarter", 1),
                note("F4", 2, "quarter", 1),
            ),
            format!(
                "{}{}{}",
                grace("A7", true, "16th", Some("begin"), 1),
                grace("B7", true, "16th", Some("end"), 1),
                note("C6", 8, "whole", 1),
            ),
            note("C5", 8, "whole", 1),
        ],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    for (group, principal) in [(&e[8..9], e[9]), (&e[21..23], e[23]), (&e[27..29], e[29])] {
        let h = head(&loaded, &layout, principal);
        let bar = barline_before(&layout, h[0], (h[1] + h[3]) * 0.5);
        assert!(bar.is_finite(), "a barline before {principal:?}");
        let left = group_left(&loaded, &layout, group);
        assert!(
            left >= bar + 0.65 - 0.005,
            "graces {group:?} at {left}, the barline's right edge at {bar}"
        );
        assert!(left < h[0], "the graces before their principal");
    }
}

/// A grace's mark run, by the id its part of the event's marks carries.
fn mark_run(
    layout: &ResolvedLayoutIR,
    event: EventId,
    mark: epiphany_core::EventMark,
) -> Option<&ResolvedText> {
    let id = epiphany_layout_ir::NoteMarkPart::Glyph(mark.tag()).id(event);
    let found: Vec<&ResolvedText> = layout
        .texts
        .iter()
        .filter(|t| t.provenance.stable_id == id)
        .collect();
    assert!(found.len() <= 1, "one run of {mark:?}");
    found.first().copied()
}

/// The staff's top and bottom lines.
fn staff_lines(layout: &ResolvedLayoutIR) -> (f32, f32) {
    let ys: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
        .map(|s| s.from.y.0)
        .collect();
    let top = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    (top, top - 4.0)
}

/// Marks on grace notes are drawn, at the grace's size and outside the
/// staff, where Gould sets them as the most conspicuous place (*Behind
/// Bars*, p. 130), on the side nearer the grace: a staccato on a grace high
/// on the staff, its stem up, over the staff beyond its stem's end; one on
/// a grace low on it under the staff; an accent on a grace under the staff,
/// under its head; a staccato on each of a beamed pair over the staff, over
/// its beam. Each run carries its part's id, as a note's mark does.
#[test]
fn a_graces_marks_stand_small_outside_the_staff() {
    use epiphany_core::EventMark;
    let marked = |g: String, notation: &str| {
        g.replace(
            "</note>",
            &format!("<notations><articulations>{notation}</articulations></notations></note>"),
        )
    };
    let loaded = part(
        "grace_marks.musicxml",
        2,
        &[format!(
            "{}{}{}{}{}{}{}{}{}{}{}{}{}",
            marked(grace("E5", true, "eighth", None, 1), "<staccato/>"),
            note("F5", 1, "eighth", 1),
            marked(grace("G4", true, "eighth", None, 1), "<staccato/>"),
            note("A4", 1, "eighth", 1),
            marked(grace("A3", true, "eighth", None, 1), "<accent/>"),
            note("C4", 2, "quarter", 1),
            marked(grace("A5", false, "16th", Some("begin"), 1), "<staccato/>"),
            marked(grace("C6", false, "16th", Some("end"), 1), "<staccato/>"),
            note("B5", 2, "quarter", 1),
            marked(grace("D5", false, "16th", Some("begin"), 1), "<staccato/>"),
            marked(
                grace("F4", false, "16th", Some("continue"), 1),
                "<staccato/>"
            ),
            marked(grace("D4", false, "16th", Some("end"), 1), "<staccato/>"),
            note("C4", 2, "quarter", 1),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let (top, bottom) = staff_lines(&layout);
    let full = |name: &str| {
        let m = epiphany_layout_ir::glyph::metrics(name).expect(name);
        let b = m.bounding_box();
        b.top.0 - b.bottom.0
    };
    // The staccato over the staff, beyond its grace's stem.
    let dot = text_box(mark_run(&layout, e[0], EventMark::Staccato).expect("the staccato drawn"));
    let (_, _, tip) = stem(&layout, e[0]);
    near(
        dot[3] - dot[1],
        0.7 * full("articStaccatoAbove"),
        "a staccato at 0.7",
    );
    assert!(
        dot[1] >= top + 0.4 - 0.01,
        "over the staff: {dot:?}, top line {top}"
    );
    assert!(
        dot[1] >= tip + 0.4 - 0.01,
        "beyond its stem: {dot:?}, the tip {tip}"
    );
    // On the stem's side, midway between the head's centre and the stem.
    let h0 = grace_head(&loaded, &layout, e[0]);
    let (x0, _, _) = stem(&layout, e[0]);
    near(
        (dot[0] + dot[2]) * 0.5,
        ((h0[0] + h0[2]) * 0.5 + x0) * 0.5,
        "midway between head and stem",
    );
    // The low grace's staccato under the staff, nearer its head.
    let low = text_box(mark_run(&layout, e[2], EventMark::Staccato).expect("drawn"));
    assert!(low[3] <= bottom - 0.4 + 0.01, "under the staff: {low:?}");
    // The accent under the low grace's head and ledger lines.
    let accent = text_box(mark_run(&layout, e[4], EventMark::Accent).expect("the accent drawn"));
    let h = grace_head(&loaded, &layout, e[4]);
    near(
        accent[3] - accent[1],
        0.7 * full("articAccentBelow"),
        "an accent at 0.7",
    );
    assert!(
        accent[3] <= bottom - 0.4 + 0.01,
        "under the staff: {accent:?}"
    );
    assert!(
        accent[3] <= h[1] - 0.4 + 0.01,
        "under its head: {accent:?}, {h:?}"
    );
    let centre = (h[0] + h[2]) * 0.5;
    near((accent[0] + accent[2]) * 0.5, centre, "centred on its head");
    // The beamed pair's staccatos over the beam, where each stands.
    let beam = strokes(&layout, e[6])
        .into_iter()
        .filter(|s| s.thickness.0 > 0.3)
        .max_by(|a, b| a.from.y.0.total_cmp(&b.from.y.0))
        .expect("the pair's beam");
    let beam_top = |x: f32| {
        let t = (x - beam.from.x.0) / (beam.to.x.0 - beam.from.x.0);
        beam.from.y.0 + t.clamp(0.0, 1.0) * (beam.to.y.0 - beam.from.y.0) + beam.thickness.0 * 0.5
    };
    for g in [e[6], e[7]] {
        let dot = text_box(mark_run(&layout, g, EventMark::Staccato).expect("drawn"));
        let at = beam_top((dot[0] + dot[2]) * 0.5);
        assert!(
            dot[1] >= at + 0.4 - 0.01,
            "over the beam: {dot:?}, the beam's top {at}"
        );
        assert!(dot[1] >= top + 0.4 - 0.01, "over the staff: {dot:?}");
    }
    // A run one beam joins takes its marks on one side, the run's: falling
    // from high on the staff to under it, under the staff.
    for g in [e[9], e[10], e[11]] {
        let dot = text_box(mark_run(&layout, g, EventMark::Staccato).expect("drawn"));
        assert!(dot[3] <= bottom - 0.4 + 0.01, "under the staff: {dot:?}");
    }
}

/// A grace group's reach widens its principal's column in the projection's
/// frame, as an accidental's does (D79's Q13), so that frame, in which the
/// curves and beams are first shaped before the spacing sets the page,
/// holds no grace over the note before: before a principal that follows
/// sixteenths closely, the group's leftmost ink in it stands right of the
/// previous note's head.
#[test]
fn a_grace_groups_reach_widens_its_columns_place_in_the_projection() {
    use epiphany_layout_ir::{to_constrained, to_logical};
    let sixteenths: String = (0..4).map(|_| note("C5", 1, "16th", 1)).collect();
    let graced = part(
        "grace_overhang.musicxml",
        4,
        &[format!(
            "{sixteenths}{}{}{}{}{}{}",
            grace("C5", true, "16th", Some("begin"), 1),
            grace("D5", true, "16th", Some("continue"), 1),
            grace("E5", true, "16th", Some("end"), 1),
            note("A4", 4, "quarter", 1),
            note("B4", 4, "quarter", 1),
            note("C5", 4, "quarter", 1),
        )],
    );
    let e = events(&graced);
    let constrained = to_constrained(&to_logical(&graced.reduced.score));
    let before = sources(&graced, e[3]);
    let previous = constrained
        .glyphs
        .iter()
        .filter(|g| {
            before.contains(&g.provenance.source) && g.glyph.as_str().starts_with("notehead")
        })
        .map(|g| g.baseline.x.0 + g.bounding_box.right.0)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(previous.is_finite(), "the previous note's head");
    let group: Vec<TypedObjectId> = e[4..7].iter().flat_map(|&g| sources(&graced, g)).collect();
    let left = constrained
        .texts
        .iter()
        .filter(|t| group.contains(&t.text.provenance.source))
        .map(|t| t.text.origin.x.0 + t.text.bounds.left.0)
        .chain(
            constrained
                .strokes
                .iter()
                .filter(|s| group.contains(&s.provenance.source) && s.from != s.to)
                .map(|s| s.from.x.0.min(s.to.x.0) - s.thickness.0 * 0.5),
        )
        .fold(f32::INFINITY, f32::min);
    assert!(
        left >= previous,
        "the group at {left}, the previous head's right edge at {previous}"
    );
}

/// A grace group stands clear of what its principal's column places that
/// takes room as its chords' ink, by the 0.45 it keeps from the principal
/// (MuseScore's `graceToMainNoteDist`): before a rolled chord, the grace's
/// ink ends 0.45 left of the arpeggio's, where it had been set against the
/// chord's heads alone and stood on the arpeggio.
#[test]
fn a_grace_before_a_rolled_chord_stands_clear_of_its_arpeggio() {
    let rolled = |p: &str, chord: bool| {
        note(p, 4, "half", 1)
            .replace("</note>", "<notations><arpeggiate/></notations></note>")
            .replacen("<note>", if chord { "<note><chord/>" } else { "<note>" }, 1)
    };
    let loaded = part(
        "grace_arpeggio.musicxml",
        2,
        &[format!(
            "{}{}{}{}{}",
            note("C5", 4, "half", 1),
            grace("D4", true, "eighth", None, 1),
            rolled("E4", false),
            rolled("G4", true),
            rolled("C5", true),
        )],
    );
    let layout = engrave_loaded(&loaded).layout;
    let e = events(&loaded);
    let arpeggio = layout
        .texts
        .iter()
        .find(|t| t.provenance.stable_id == epiphany_layout_ir::NoteMarkPart::Arpeggio.id(e[2]))
        .map(text_box)
        .expect("the arpeggio");
    let right = grace_runs(&loaded, &layout, e[1])
        .iter()
        .map(|t| text_box(t)[2])
        .chain(
            strokes(&layout, e[1])
                .iter()
                .map(|s| s.from.x.0.max(s.to.x.0) + s.thickness.0 * 0.5),
        )
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        right <= arpeggio[0] - 0.45 + 0.01,
        "the grace's ink to {right}, the arpeggio from {}",
        arpeggio[0]
    );
}
