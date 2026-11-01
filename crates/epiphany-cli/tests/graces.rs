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
