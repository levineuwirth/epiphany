//! The `epiphany` command on the importer's hand-written fixtures, and the
//! page a render keeps.

use std::path::{Path, PathBuf};
use std::process::Command;

use epiphany_cli::omissions::omissions;
use epiphany_cli::{engrave, load, page};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../epiphany-musicxml/tests/fixtures")
        .join(name)
}

#[test]
fn render_writes_the_requested_page_and_refuses_one_past_the_end() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("single_part.svg");
    let _ = std::fs::remove_file(&out);
    let status = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("render")
        .arg(fixture("single_part.musicxml"))
        .args(["--page", "1", "-o"])
        .arg(&out)
        .output()
        .expect("runs");
    assert!(status.status.success(), "{status:?}");
    let svg = std::fs::read_to_string(&out).expect("an SVG was written");
    assert!(svg.starts_with("<?xml") && svg.contains("<svg"));
    assert!(svg.contains("noteheadBlack"), "the page carries the notes");

    let past = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("render")
        .arg(fixture("single_part.musicxml"))
        .args(["--page", "2"])
        .output()
        .expect("runs");
    assert_eq!(past.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&past.stderr).contains("page 2 of 1"));
}

#[test]
fn import_exits_nonzero_on_a_named_refusal() {
    let out = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("import")
        .arg(fixture("timewise.musicxml"))
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not <score-partwise>"));
}

/// A generated score long enough to break onto several pages.
fn long_score() -> String {
    let mut measures = String::new();
    for m in 1..=320 {
        measures.push_str(&format!("<measure number=\"{m}\">"));
        if m == 1 {
            measures.push_str(
                "<attributes><divisions>1</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>",
            );
        }
        for step in ["C", "D", "E", "F"] {
            measures.push_str(&format!(
                "<note><pitch><step>{step}</step><octave>5</octave></pitch>\
                 <duration>1</duration><voice>1</voice></note>"
            ));
        }
        measures.push_str("</measure>");
    }
    format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Flute</part-name></score-part></part-list>\
         <part id=\"P1\">{measures}</part></score-partwise>"
    )
}

#[test]
fn a_page_keeps_exactly_the_primitives_its_systems_own() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long_score.musicxml");
    std::fs::write(&path, long_score()).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    assert!(
        layout.pages.len() > 1,
        "the score breaks onto several pages"
    );
    let owned: usize = layout.systems().map(|s| s.primitives.glyphs.len()).sum();
    let mut total = 0;
    for number in 1..=layout.pages.len() {
        let one = page(&layout, number).expect("the page exists");
        let expected: usize = layout.pages[number - 1]
            .systems
            .iter()
            .map(|s| s.primitives.glyphs.len())
            .sum();
        assert_eq!(one.glyphs.len(), expected, "page {number}");
        assert_eq!(one.pages.len(), 1);
        total += one.glyphs.len();
    }
    assert_eq!(total, owned, "every owned glyph is on exactly one page");
    assert!(page(&layout, layout.pages.len() + 1).is_none());
    assert!(page(&layout, 0).is_none());
}

/// A primitive no system owns is kept on a layout of one page, and counted as
/// on no page when there are several.
#[test]
fn unowned_ink_is_kept_on_a_single_page_and_counted_on_several() {
    let single = load(&fixture("single_part.musicxml")).expect("loads");
    let mut layout = engrave(&single.reduced.score).layout;
    assert_eq!(layout.pages.len(), 1);
    let before = page(&layout, 1).expect("page 1").glyphs.len();
    let moved = layout.pages[0].systems[0]
        .primitives
        .glyphs
        .pop()
        .expect("a glyph");
    layout.unowned.glyphs.push(moved);
    assert_eq!(page(&layout, 1).expect("page 1").glyphs.len(), before);
    assert_eq!(
        omissions(&single.reduced.score, &layout, &[])
            .kinds
            .get("ink on no page"),
        None
    );

    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long_score_unowned.musicxml");
    std::fs::write(&path, long_score()).expect("written");
    let long = load(&path).expect("loads");
    let mut layout = engrave(&long.reduced.score).layout;
    assert!(layout.pages.len() > 1);
    let moved = layout.pages[0].systems[0]
        .primitives
        .glyphs
        .pop()
        .expect("a glyph");
    layout.unowned.glyphs.push(moved);
    let pages: usize = (1..=layout.pages.len())
        .map(|n| page(&layout, n).expect("a page").glyphs.len())
        .sum();
    assert_eq!(pages + 1, layout.glyphs.len());
    assert_eq!(
        omissions(&long.reduced.score, &layout, &[])
            .kinds
            .get("ink on no page"),
        Some(&1)
    );
}

/// A rest filling its measure is drawn as a whole rest in 3/4 and 2/4 alike,
/// with no dot, and a hidden rest keeps its time but draws nothing.
#[test]
fn a_measure_rest_is_a_whole_rest_in_any_meter_and_a_hidden_rest_draws_nothing() {
    let xml = "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Flute</part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>1</divisions><time><beats>3</beats>\
         <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
         </attributes><note><rest measure=\"yes\"/><duration>3</duration><voice>1</voice>\
         </note></measure>\
         <measure number=\"2\"><attributes><time><beats>2</beats><beat-type>4</beat-type>\
         </time></attributes><note><rest measure=\"yes\"/><duration>2</duration>\
         <voice>1</voice></note></measure>\
         <measure number=\"3\"><note print-object=\"no\"><rest/><duration>1</duration>\
         <voice>1</voice></note><note><pitch><step>C</step><octave>5</octave></pitch>\
         <duration>1</duration><voice>1</voice></note></measure>\
         </part></score-partwise>";
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("measure_rests.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    let rests: Vec<&str> = layout
        .glyphs
        .iter()
        .map(|g| g.glyph.as_str())
        .filter(|name| name.starts_with("rest"))
        .collect();
    assert_eq!(rests, ["restWhole", "restWhole"]);
    assert!(layout
        .glyphs
        .iter()
        .all(|g| g.glyph.as_str() != "augmentationDot"));
}

/// Every beam's ends sit on the stems it joins, and every beamed stem ends on
/// its beam, after the engraver re-spaces the columns and justifies the
/// systems: on the imported beams of the fixture, and on a long score of
/// eighths the meter beams in pairs across many justified systems.
#[test]
fn beams_stay_on_their_stems_through_spacing_and_justification() {
    use epiphany_core::TypedObjectId;
    use epiphany_layout_ir::{is_beam_stroke, ResolvedLayoutIR, Stroke};

    fn check(layout: &ResolvedLayoutIR) -> usize {
        let beams: Vec<&Stroke> = layout
            .strokes
            .iter()
            .filter(|s| is_beam_stroke(s))
            .collect();
        for beam in &beams {
            let stems: Vec<&Stroke> = layout
                .strokes
                .iter()
                .filter(|s| {
                    !is_beam_stroke(s)
                        && s.from.x == s.to.x
                        && s.from.y != s.to.y
                        && matches!(s.provenance.source, TypedObjectId::Event(_))
                        && beam.provenance.dependencies.contains(&s.provenance.source)
                })
                .collect();
            let half = beam.thickness.0 / 2.0;
            let on_stem = |x: f32| stems.iter().any(|s| (s.from.x.0 - x).abs() < 0.07);
            let (from, to) = (beam.from.x.0, beam.to.x.0);
            let hook = (to - from - 1.16).abs() < 0.02;
            assert!(
                if hook {
                    on_stem(from + 0.06) || on_stem(to - 0.06)
                } else {
                    on_stem(from + 0.06) && on_stem(to - 0.06)
                },
                "a beam's ends sit on its stems: {from}..{to}"
            );
            // A stem beneath a beam's span ends within the beam nearest its
            // tip, the outermost one.
            for stem in stems
                .iter()
                .filter(|s| s.from.x.0 >= from && s.from.x.0 <= to)
            {
                let reach = beams
                    .iter()
                    .filter(|b| b.from.x.0 <= stem.from.x.0 && stem.from.x.0 <= b.to.x.0)
                    .map(|b| {
                        let t = (stem.from.x.0 - b.from.x.0) / (b.to.x.0 - b.from.x.0);
                        let y = b.from.y.0 + t * (b.to.y.0 - b.from.y.0);
                        (stem.to.y.0 - y).abs()
                    })
                    .fold(f32::INFINITY, f32::min);
                assert!(reach <= half + 0.17, "a stem reaches its beam: {reach}");
            }
        }
        beams.len()
    }

    let loaded = load(&fixture("beams.musicxml")).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    // Five beamed groups, the four sixteenths' second beam, and the
    // sixteenth's hook beside its dotted eighth.
    assert_eq!(check(&layout), 7);

    let mut measures = String::new();
    for m in 1..=160 {
        measures.push_str(&format!("<measure number=\"{m}\">"));
        if m == 1 {
            measures.push_str(
                "<attributes><divisions>2</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>",
            );
        }
        for step in ["C", "E", "D", "F", "E", "G", "F", "A"] {
            measures.push_str(&format!(
                "<note><pitch><step>{step}</step><octave>5</octave></pitch>\
                 <duration>1</duration><voice>1</voice><type>eighth</type></note>"
            ));
        }
        measures.push_str("</measure>");
    }
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Flute</part-name></score-part></part-list>\
         <part id=\"P1\">{measures}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long_eighths.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    assert!(layout.systems().count() > 2, "the score wraps");
    // Pairs by beat: four beams a measure, and no beamed note keeps a flag.
    assert_eq!(check(&layout), 160 * 4);
    assert!(layout
        .glyphs
        .iter()
        .all(|g| !g.glyph.as_str().starts_with("flag")));
}

/// Ties are drawn across every barline and across every system break: a long
/// score of whole notes, each tied to the next, wraps onto many justified
/// systems; each tie is one arc, or two half-arcs where a system breaks
/// between its notes, and every arc's ends sit at a head it joins or at its
/// system's edge.
#[test]
fn ties_cross_barlines_and_system_breaks() {
    use epiphany_core::TypedObjectId;

    let mut measures = String::new();
    let count = 120;
    for m in 1..=count {
        measures.push_str(&format!("<measure number=\"{m}\">"));
        if m == 1 {
            measures.push_str(
                "<attributes><divisions>1</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>",
            );
        }
        let ties = match m {
            1 => "<tie type=\"start\"/>",
            m if m == count => "<tie type=\"stop\"/>",
            _ => "<tie type=\"stop\"/><tie type=\"start\"/>",
        };
        measures.push_str(&format!(
            "<note><pitch><step>A</step><octave>4</octave></pitch><duration>4</duration>\
             {ties}<voice>1</voice><type>whole</type></note></measure>"
        ));
    }
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Oboe</part-name></score-part></part-list>\
         <part id=\"P1\">{measures}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long_ties.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert_eq!(loaded.reduced.score.cross_cutting.ties.len(), count - 1);
    let layout = engrave(&loaded.reduced.score).layout;
    let systems: Vec<_> = layout.systems().collect();
    assert!(systems.len() > 2, "the score wraps");
    let arcs: Vec<_> = layout
        .curves
        .iter()
        .filter(|c| matches!(c.provenance.source, TypedObjectId::Tie(_)))
        .collect();
    assert_eq!(arcs.len(), count - 1 + systems.len() - 1);
    for system in &systems {
        let heads: Vec<(f32, f32)> = system
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .filter(|g| g.glyph.as_str().starts_with("notehead"))
            .map(|g| {
                let x = g.position.x.0;
                (x + g.bounding_box.left.0, x + g.bounding_box.right.0)
            })
            .collect();
        // The system's first and last heads: an arc continuing across the
        // break starts before the first (after the system's lead) and one
        // breaking off ends after the last.
        let left = heads.iter().map(|h| h.0).fold(f32::INFINITY, f32::min);
        let right = system.bounding_box.origin.x.0 + system.bounding_box.size.width.0;
        for &i in &system.primitives.curves {
            let arc = &layout.curves[i as usize];
            if !matches!(arc.provenance.source, TypedObjectId::Tie(_)) {
                continue;
            }
            let (x0, x3) = (arc.p0.x.0, arc.p3.x.0);
            let after_head = heads.iter().any(|(_, r)| (x0 - r - 0.15).abs() < 0.02);
            let before_head = heads.iter().any(|(l, _)| (l - x3 - 0.15).abs() < 0.02);
            assert!(
                after_head || (x0 < left && left - x0 < 1.5),
                "an arc starts just after a head, or just before its system's first: {x0}"
            );
            assert!(
                before_head || (right - x3).abs() < 1.0,
                "an arc ends just before a head, or at its system's end: {x3}"
            );
            assert!(after_head || before_head, "an arc meets a head it joins");
        }
    }
}

/// An accidental is drawn against the key and the measure: none where the key
/// or an earlier accidental already gives the pitch, a natural to cancel one,
/// held to the barline and only on its own octave, and none on a note a tie
/// carries over the barline.
#[test]
fn accidentals_are_drawn_against_the_key_and_the_measure() {
    use epiphany_core::TypedObjectId;

    let note = |step: &str, alter: i32, octave: u8, duration: u8, tie: &str| {
        format!(
            "<note><pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave>\
             </pitch><duration>{duration}</duration>{tie}<voice>1</voice></note>"
        )
    };
    // Two flats: B and E are flat by the key.
    let measures = [
        // E-flat by the key, E natural, E natural again, E-flat again.
        [
            note("E", -1, 4, 1, ""),
            note("E", 0, 4, 1, ""),
            note("E", 0, 4, 1, ""),
            note("E", -1, 4, 1, ""),
        ]
        .concat(),
        // The barline cancels the natural; another octave has its own; a B
        // natural, tied over the barline.
        [
            note("E", 0, 4, 1, ""),
            note("E", 0, 5, 1, ""),
            note("B", 0, 4, 2, "<tie type=\"start\"/>"),
        ]
        .concat(),
        // The tied B natural shows nothing and sets nothing, so the next B
        // natural shows its own; then B-flat again.
        [
            note("B", 0, 4, 2, "<tie type=\"stop\"/>"),
            note("B", 0, 4, 1, ""),
            note("B", -1, 4, 1, ""),
        ]
        .concat(),
        // F-sharp, carried; F-sharp an octave up needs its own; F natural.
        [
            note("F", 1, 4, 1, ""),
            note("F", 1, 4, 1, ""),
            note("F", 1, 5, 1, ""),
            note("F", 0, 4, 1, ""),
        ]
        .concat(),
    ];
    let mut body = String::new();
    for (m, notes) in measures.iter().enumerate() {
        body.push_str(&format!("<measure number=\"{}\">", m + 1));
        if m == 0 {
            body.push_str(
                "<attributes><divisions>1</divisions><key><fifths>-2</fifths></key>\
                 <time><beats>4</beats><beat-type>4</beat-type></time>\
                 <clef><sign>G</sign><line>2</line></clef></attributes>",
            );
        }
        body.push_str(notes);
        body.push_str("</measure>");
    }
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Violin</part-name></score-part></part-list>\
         <part id=\"P1\">{body}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("accidentals.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    // Each note's own accidental, in reading order: the accidental shares its
    // notehead's pitch source.
    let mut heads: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .collect();
    heads.sort_by(|a, b| a.position.x.0.total_cmp(&b.position.x.0));
    let shown: Vec<Option<&str>> = heads
        .iter()
        .map(|head| {
            layout
                .glyphs
                .iter()
                .find(|g| {
                    g.provenance.source == head.provenance.source
                        && g.glyph.as_str().starts_with("accidental")
                })
                .map(|g| g.glyph.as_str())
        })
        .collect();
    assert!(heads
        .iter()
        .all(|h| matches!(h.provenance.source, TypedObjectId::Pitch(_))));
    let (n, f, s) = (
        Some("accidentalNatural"),
        Some("accidentalFlat"),
        Some("accidentalSharp"),
    );
    assert_eq!(
        shown,
        [
            None, n, None, f, // measure 1
            n, n, n, // measure 2
            None, n, f, // measure 3, the first note tied over
            s, None, s, n, // measure 4
        ]
    );
}

/// Two voices on a staff turn apart wherever both show ink: the upper
/// voice's stems and beams up, its rests, ties and slurs above; the lower's
/// stems down, its rests below and a dot on a line under it. A voice alone
/// keeps the pitch's rule.
#[test]
fn voices_turn_their_stems_rests_ties_and_dots_apart() {
    use epiphany_core::{Event, TypedObjectId};

    let note = |step: &str, octave: u8, duration: u8, voice: u8, kind: &str, extra: &str| {
        // A tie goes before the voice; a dot and a slur's notation after the
        // type.
        let (before, after) = if extra.starts_with("<tie") {
            (extra, "")
        } else {
            ("", extra)
        };
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>{duration}</duration>{before}<voice>{voice}</voice><type>{kind}</type>\
             {after}</note>"
        )
    };
    let rest = |voice: u8| {
        format!(
            "<note><rest/><duration>2</duration><voice>{voice}</voice><type>quarter</type></note>"
        )
    };
    let backup = "<backup><duration>8</duration></backup>";
    let slur_start = "<notations><slur type=\"start\"/></notations>";
    let slur_stop = "<notations><slur type=\"stop\"/></notations>";
    let measures = [
        // The upper voice's C, its rest and its A tied over; the lower's
        // dotted G and rest.
        format!(
            "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
             </time><clef><sign>G</sign><line>2</line></clef></attributes>{}{}{}{backup}{}{}",
            note("C", 5, 2, 1, "quarter", ""),
            rest(1),
            note("A", 4, 4, 1, "half", "<tie type=\"start\"/>"),
            note("G", 4, 6, 2, "half", "<dot/>"),
            rest(2),
        ),
        // The upper voice alone.
        format!(
            "{}{}",
            note("A", 4, 4, 1, "half", "<tie type=\"stop\"/>"),
            note("C", 5, 4, 1, "half", ""),
        ),
        // Two eighths beamed, B and C slurred over the lower voice's dotted
        // E and rest.
        format!(
            "{}{}{}{}{backup}{}{}",
            note("C", 5, 1, 1, "eighth", ""),
            note("D", 5, 1, 1, "eighth", ""),
            note("B", 4, 2, 1, "quarter", slur_start),
            note("C", 5, 4, 1, "half", slur_stop),
            note("E", 4, 6, 2, "half", "<dot/>"),
            rest(2),
        ),
    ];
    let body: String = measures
        .iter()
        .enumerate()
        .map(|(m, content)| format!("<measure number=\"{}\">{content}</measure>", m + 1))
        .collect();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("voices.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    let systems: Vec<_> = layout.systems().collect();
    assert_eq!(systems.len(), 1);
    let staff = &systems[0].staves[0].bounding_box;
    let middle = staff.origin.y.0 + staff.size.height.0 / 2.0;
    // Staff steps from the bottom line.
    let step = |y: f32| ((y - middle) * 2.0).round() as i32 + 4;

    // Each head's staff step and whether its note's stem (its event's one
    // upright stroke) turns up, in time order and down a column.
    let event_of = |source: &TypedObjectId| {
        loaded
            .reduced
            .score
            .events
            .iter()
            .find_map(|e| match (e, source) {
                (Event::Pitched(n), TypedObjectId::Pitch(p))
                    if n.pitches.iter().any(|i| i.id == *p) =>
                {
                    Some(TypedObjectId::Event(n.id))
                }
                _ => None,
            })
    };
    let mut heads: Vec<(f32, i32, Option<bool>)> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .map(|g| {
            let (x, y) = (g.position.x.0, g.position.y.0);
            let event = event_of(&g.provenance.source).expect("a head's note");
            let stem = layout
                .strokes
                .iter()
                .find(|s| s.provenance.source == event && s.from.x == s.to.x);
            (x, step(y), stem.map(|s| s.from.y.0.max(s.to.y.0) > y + 1.0))
        })
        .collect();
    heads.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
    let up = Some(true);
    let down = Some(false);
    assert_eq!(
        heads.iter().map(|h| (h.1, h.2)).collect::<Vec<_>>(),
        [
            // The upper voice's C up, where alone it would turn down; the
            // lower voice's G down, where alone it would turn up; the A up.
            (5, up),
            (2, down),
            (3, up),
            // Alone: the A up and the C down, by their pitches.
            (3, up),
            (5, down),
            // The beamed eighths up over the lower voice's E, down; the B on
            // the middle line up, and the last C.
            (5, up),
            (0, down),
            (6, up),
            (4, up),
            (5, up),
        ]
    );
    // The upper voice's rest a space over the middle line; the lower
    // voice's a space under it.
    let mut rests: Vec<(f32, f32)> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "restQuarter")
        .map(|g| (g.position.x.0, g.position.y.0 - middle))
        .collect();
    rests.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(rests.len(), 3);
    for (rest, want) in rests.iter().zip([1.0, -1.0, -1.0]) {
        assert!(
            (rest.1 - want).abs() < 1e-3,
            "a rest {} from the middle",
            rest.1
        );
    }
    // The upper voice's tie arcs above, though alone its A's would arc below.
    let tie = layout
        .curves
        .iter()
        .find(|c| matches!(c.provenance.source, TypedObjectId::Tie(_)))
        .expect("a tie");
    assert!(tie.p1.y.0 > tie.p0.y.0, "the tie arcs above");
    // And its slur, over the B and C whose stems turn up while the lower
    // voice holds its E, where alone it would arc below.
    let slur = layout
        .curves
        .iter()
        .find(|c| matches!(c.provenance.source, TypedObjectId::Slur(_)))
        .expect("a slur");
    assert!(slur.p1.y.0 > slur.p0.y.0, "the slur arcs above");
    // The lower voice's dotted G and E, each on a line, dot the space below.
    let mut dots: Vec<(f32, i32)> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "augmentationDot")
        .map(|g| (g.position.x.0, step(g.position.y.0)))
        .collect();
    dots.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(dots.iter().map(|d| d.1).collect::<Vec<_>>(), [1, -1]);
}

/// A glyph's ink box on the page: left, bottom, right, top.
fn glyph_box(glyph: &epiphany_layout_ir::ResolvedGlyph) -> [f32; 4] {
    let (x, y, b) = (glyph.position.x.0, glyph.position.y.0, &glyph.bounding_box);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

/// A straight stroke's ink box on the page.
fn stroke_box(stroke: &epiphany_layout_ir::Stroke) -> [f32; 4] {
    let half = stroke.thickness.0 / 2.0;
    let (a, b) = (&stroke.from, &stroke.to);
    if (a.y.0 - b.y.0).abs() < 1e-4 {
        [
            a.x.0.min(b.x.0),
            a.y.0 - half,
            a.x.0.max(b.x.0),
            a.y.0 + half,
        ]
    } else {
        [
            a.x.0 - half,
            a.y.0.min(b.y.0),
            a.x.0 + half,
            a.y.0.max(b.y.0),
        ]
    }
}

/// Whether two boxes share ink; touching edges do not.
fn boxes_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[2] > b[0] && b[2] > a[0] && a[3] > b[1] && b[3] > a[1]
}

/// Writes a one-part treble-clef score in 4/4 of `measures` and loads it.
fn treble_part(name: &str, measures: &[String]) -> epiphany_cli::Loaded {
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
    load(&path).expect("loads")
}

/// A pitched note of `duration` eighths in `voice`, a chord member when
/// `chord`.
fn pitched(step: &str, alter: i8, octave: u8, duration: u8, voice: u8, chord: bool) -> String {
    format!(
        "<note>{}<pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave>\
         </pitch><duration>{duration}</duration><voice>{voice}</voice></note>",
        if chord { "<chord/>" } else { "" }
    )
}

/// The accidentals of a column stand clear of its heads, of the ledger lines
/// their height spans, of each other and of every stem, each as near its
/// head as that allows: the highest nearest, a column further out for each
/// that would touch one already placed, across voices.
#[test]
fn accidentals_stand_clear_of_their_column_and_close_to_it() {
    let measures = [
        [
            // A sharp on a ledger-line note.
            pitched("C", 1, 4, 2, 1, false),
            // A flat whose height spans its own ledger and a chord-mate's.
            pitched("G", 0, 3, 2, 1, false),
            pitched("B", -1, 3, 2, 1, true),
            // A third with two sharps.
            pitched("F", 1, 4, 2, 1, false),
            pitched("A", 1, 4, 2, 1, true),
            // Three flats a third apart.
            pitched("E", -1, 4, 2, 1, false),
            pitched("G", -1, 4, 2, 1, true),
            pitched("B", -1, 4, 2, 1, true),
        ]
        .concat(),
        [
            // A double flat; a sharp over another voice's flat; a natural.
            pitched("B", -2, 4, 2, 1, false),
            pitched("C", 1, 5, 2, 1, false),
            pitched("B", 0, 4, 4, 1, false),
            "<backup><duration>8</duration></backup>".to_owned(),
            pitched("E", 0, 4, 2, 2, false),
            pitched("A", -1, 4, 2, 2, false),
            pitched("D", 0, 4, 4, 2, false),
        ]
        .concat(),
    ];
    let loaded = treble_part("accidental_columns.musicxml", &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let staff = &layout.systems().next().expect("a system").staves[0].bounding_box;
    let middle = staff.origin.y.0 + staff.size.height.0 / 2.0;
    let step = |y: f32| ((y - middle) * 2.0).round() as i32 + 4;

    let heads: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .collect();
    let accidentals: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("accidental"))
        .collect();
    let ledgers: Vec<[f32; 4]> = layout
        .strokes
        .iter()
        .filter(|s| epiphany_layout_ir::is_rigid_width_stroke(s))
        .map(stroke_box)
        .collect();
    let stems: Vec<[f32; 4]> = layout
        .strokes
        .iter()
        .filter(|s| s.from.x == s.to.x && s.from.y != s.to.y)
        .map(stroke_box)
        .collect();
    assert!(ledgers.len() >= 4 && !stems.is_empty());

    // Clear of every head, accidental, ledger line and stem.
    for (i, a) in accidentals.iter().enumerate() {
        let ink = glyph_box(a);
        let name = (a.glyph.as_str(), step(a.position.y.0));
        for head in &heads {
            assert!(!boxes_overlap(ink, glyph_box(head)), "{name:?} on a head");
        }
        for (j, b) in accidentals.iter().enumerate() {
            assert!(
                i == j || !boxes_overlap(ink, glyph_box(b)),
                "{name:?} on another accidental"
            );
        }
        for ledger in &ledgers {
            assert!(!boxes_overlap(ink, *ledger), "{name:?} on a ledger line");
        }
        for stem in &stems {
            assert!(!boxes_overlap(ink, *stem), "{name:?} on a stem");
        }
    }

    // And as near its head as that allows: each accidental's ink stands this
    // far left of its own head's (0.2 from a head, 0.3 further past a ledger
    // line it spans, and a column further out by the width of the one it
    // clears and 0.15).
    let (sharp, flat) = (1020.0 / 1024.0, 926.0 / 1024.0);
    let placed: Vec<(&str, i32, f32)> = accidentals
        .iter()
        .map(|a| {
            let own = heads
                .iter()
                .find(|h| h.provenance.source == a.provenance.source)
                .expect("an accidental's head");
            let gap = glyph_box(own)[0] - glyph_box(a)[2];
            (a.glyph.as_str(), step(a.position.y.0), gap)
        })
        .collect();
    let expected = [
        ("accidentalSharp", -2, 0.5),               // C sharp, past its ledger
        ("accidentalFlat", -3, 0.5),                // B flat, past two ledgers
        ("accidentalSharp", 3, 0.2),                // A sharp, nearest
        ("accidentalSharp", 1, 0.2 + sharp + 0.15), // F sharp, a column out
        ("accidentalFlat", 4, 0.2),                 // B flat, nearest
        ("accidentalFlat", 0, 0.2 + flat + 0.15),   // E flat, a column out
        ("accidentalFlat", 2, 0.2 + 2.0 * (flat + 0.15)), // G flat, two out
        ("accidentalDoubleFlat", 4, 0.2),           // B double flat
        ("accidentalSharp", 5, 0.2),                // C sharp, nearest
        ("accidentalFlat", 3, 0.2 + sharp + 0.15),  // the other voice's A flat
        ("accidentalNatural", 4, 0.2),              // B natural
    ];
    assert_eq!(accidentals.len(), expected.len(), "{placed:?}");
    let mut remaining = placed.clone();
    for (name, at, gap) in expected {
        let found = remaining
            .iter()
            .position(|p| p.0 == name && p.1 == at && (p.2 - gap).abs() < 1e-3)
            .unwrap_or_else(|| panic!("no {name} at step {at}, {gap} off its head: {placed:?}"));
        remaining.remove(found);
    }
}

/// Heads a second apart stand either side of their stem: in a chord the head
/// across the stem from the one the stem leaves, a cluster alternating, a
/// ledger line under a head set left, ties leaving and dots clearing the head
/// set right; a lower voice a second under an upper one stands to its right,
/// their stems in one line, while voices a third apart, and a unison they
/// share, stand together. The engraver holds each pair set apart to it.
#[test]
fn seconds_stand_either_side_of_the_stem() {
    use epiphany_core::{Event, TypedObjectId};
    use epiphany_layout_ir::{
        to_constrained, to_logical, ConstraintSolver, LayoutConstraint, SolverConfig,
    };

    // A pitch as `G4` or `D#5`, of `duration` eighths in `voice`; `tie`
    // precedes the voice and `after` follows it.
    let note = |pitch: &str, duration: u8, voice: u8, chord: bool, tie: &str, after: &str| {
        let (step, rest) = pitch.split_at(1);
        let (alter, octave) = match rest.strip_prefix('#') {
            Some(octave) => (1, octave),
            None => (0, rest),
        };
        format!(
            "<note>{}<pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave>\
             </pitch><duration>{duration}</duration>{tie}<voice>{voice}</voice>{after}</note>",
            if chord { "<chord/>" } else { "" }
        )
    };
    let (start, stop) = ("<tie type=\"start\"/>", "<tie type=\"stop\"/>");
    let dotted = "<type>half</type><dot/>";
    let (begin, end) = (
        "<type>eighth</type><beam number=\"1\">begin</beam>",
        "<type>eighth</type><beam number=\"1\">end</beam>",
    );
    let rest = |duration: u8, voice: u8| {
        format!("<note><rest/><duration>{duration}</duration><voice>{voice}</voice></note>")
    };
    let measures = [
        [
            note("F4", 2, 1, false, "", ""),
            note("G4", 2, 1, true, "", ""),
            note("C5", 2, 1, false, "", ""),
            note("D#5", 2, 1, true, "", ""),
            note("E5", 2, 1, true, "", ""),
            note("A5", 2, 1, false, "", ""),
            note("B5", 2, 1, true, "", ""),
            note("G4", 2, 1, false, start, ""),
            note("A4", 2, 1, true, start, ""),
        ]
        .concat(),
        [
            note("G4", 6, 1, false, stop, dotted),
            note("A4", 6, 1, true, stop, dotted),
            note("C5", 2, 1, false, "", ""),
        ]
        .concat(),
        [
            note("A4", 4, 1, false, "", ""),
            note("C5", 2, 1, false, "", ""),
            note("E5", 2, 1, false, "", ""),
            "<backup><duration>8</duration></backup>".to_owned(),
            note("G4", 1, 2, false, "", begin),
            note("B4", 1, 2, false, "", end),
            rest(2, 2),
            note("A4", 2, 2, false, "", ""),
            note("E5", 2, 2, false, "", ""),
        ]
        .concat(),
        [
            // A second whose beam turns its stem down.
            note("G4", 1, 1, false, "", begin),
            note("A4", 1, 1, true, "", "<type>eighth</type>"),
            note("A5", 1, 1, false, "", end),
            rest(6, 1),
        ]
        .concat(),
    ];
    let loaded = treble_part("seconds.musicxml", &measures);
    let score = &loaded.reduced.score;
    let layout = engrave(score).layout;
    let staff = &layout.systems().next().expect("a system").staves[0].bounding_box;
    let middle = staff.origin.y.0 + staff.size.height.0 / 2.0;
    let step = |y: f32| ((y - middle) * 2.0).round() as i32 + 4;
    // The width a head is set across by: its own, and the clearance.
    let across = 1209.0 / 1024.0 + 0.02;
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;

    let event_of = |source: &TypedObjectId| {
        score.events.iter().find_map(|e| match (e, source) {
            (Event::Pitched(n), TypedObjectId::Pitch(p))
                if n.pitches.iter().any(|i| i.id == *p) =>
            {
                Some(n.id)
            }
            _ => None,
        })
    };
    let heads: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .collect();
    // Each note's heads, by staff step: (step, left edge, box).
    type ChordHead = (i32, f32, [f32; 4]);
    let mut chords: Vec<(epiphany_core::EventId, Vec<ChordHead>)> = Vec::new();
    for head in &heads {
        let event = event_of(&head.provenance.source).expect("a head's note");
        let entry = (step(head.position.y.0), glyph_box(head)[0], glyph_box(head));
        match chords.iter_mut().find(|(e, _)| *e == event) {
            Some((_, members)) => members.push(entry),
            None => chords.push((event, vec![entry])),
        }
    }
    for (_, members) in &mut chords {
        members.sort_by_key(|m| m.0);
    }
    chords.sort_by(|a, b| {
        let left = |c: &[ChordHead]| c.iter().map(|m| m.1).fold(f32::INFINITY, f32::min);
        left(&a.1).total_cmp(&left(&b.1))
    });
    let with = |steps: &[i32]| -> Vec<&Vec<ChordHead>> {
        chords
            .iter()
            .map(|(_, members)| members)
            .filter(|members| members.iter().map(|m| m.0).collect::<Vec<_>>() == steps)
            .collect()
    };

    // No two heads share ink, but a unison two voices share.
    for (i, a) in heads.iter().enumerate() {
        for b in &heads[i + 1..] {
            let shared = a.glyph == b.glyph && a.position == b.position;
            assert!(
                shared || !boxes_overlap(glyph_box(a), glyph_box(b)),
                "heads at steps {} and {} share ink",
                step(a.position.y.0),
                step(b.position.y.0)
            );
        }
    }

    // The upper head of each second right of the lower, whichever way the
    // stem turns.
    assert_eq!(
        with(&[2, 3]).len(),
        3,
        "the tied, dotted and beamed seconds"
    );
    for chord in [with(&[1, 2]), with(&[2, 3])].concat() {
        assert!(near(chord[1].1 - chord[0].1, across), "{chord:?}");
    }
    // Down-stems: the lower head left of the stem, a cluster alternating.
    let cluster = with(&[5, 6, 7])[0];
    assert!(near(cluster[0].1, cluster[2].1), "{cluster:?}");
    assert!(near(cluster[2].1 - cluster[1].1, across), "{cluster:?}");
    let ledgered = with(&[10, 11])[0];
    assert!(near(ledgered[1].1 - ledgered[0].1, across), "{ledgered:?}");
    // The stem runs between the heads of a second, up or, where its beam
    // turns it, down.
    let stem_x: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| s.from.x == s.to.x && s.from.y != s.to.y)
        .map(|s| s.from.x.0)
        .collect();
    for chord in [with(&[1, 2])[0], with(&[2, 3])[2]] {
        assert!(
            stem_x
                .iter()
                .any(|x| *x > chord[0].2[2] - 0.1 && *x < chord[1].2[0] + 0.1),
            "no stem between {chord:?}"
        );
    }

    // A ledger line runs under the head set left of the stem.
    let a5 = ledgered[0].2;
    assert!(
        layout
            .strokes
            .iter()
            .filter(|s| epiphany_layout_ir::is_rigid_width_stroke(s))
            .map(stroke_box)
            .any(|l| near((l[1] + l[3]) / 2.0, (a5[1] + a5[3]) / 2.0)
                && l[0] <= a5[0]
                && l[2] >= a5[2]),
        "no ledger line under the head set left"
    );
    // The sharp clears the cluster's head set left.
    let sharp = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "accidentalSharp")
        .expect("the cluster's sharp");
    assert!(near(cluster[1].1 - glyph_box(sharp)[2], 0.2));
    for head in &heads {
        assert!(!boxes_overlap(glyph_box(sharp), glyph_box(head)));
    }

    // The dotted second's dots stand in one column right of its head set
    // right; the ties leave and meet heads, never start inside one.
    let dotted = with(&[2, 3])[1];
    let dots: Vec<f32> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "augmentationDot")
        .map(|g| glyph_box(g)[0])
        .collect();
    assert_eq!(dots.len(), 2);
    for dot in &dots {
        assert!(near(*dot, dotted[1].2[2] + 0.25), "a dot at {dot}");
    }
    let ties: Vec<_> = layout
        .curves
        .iter()
        .filter(|c| matches!(c.provenance.source, TypedObjectId::Tie(_)))
        .collect();
    assert_eq!(ties.len(), 2);
    for tie in ties {
        for end in [&tie.p0, &tie.p3] {
            for head in &heads {
                let b = glyph_box(head);
                assert!(
                    !(end.x.0 > b[0] && end.x.0 < b[2] && end.y.0 > b[1] && end.y.0 < b[3]),
                    "a tie ends inside a head"
                );
            }
        }
    }

    // Two voices a second apart: the lower to the right, the stems in one
    // line; a third apart and a shared unison, together.
    let (upper, lower) = (with(&[3])[0][0], with(&[2])[0][0]);
    assert!(near(lower.1 - upper.1, across));
    let voice_stems: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| s.from.x == s.to.x && s.from.y != s.to.y)
        .map(|s| s.from.x.0)
        .filter(|x| *x > upper.1 && *x < lower.2[2])
        .collect();
    assert_eq!(voice_stems.len(), 2, "{voice_stems:?}");
    assert!((voice_stems[0] - voice_stems[1]).abs() <= 0.02 + 1e-3);
    // The lower voice's beam starts on its stem, moved with it.
    assert!(
        layout
            .strokes
            .iter()
            .filter(|s| epiphany_layout_ir::is_beam_stroke(s))
            .any(|s| near(s.from.x.0.min(s.to.x.0), lower.1 - 0.06)),
        "no beam on the moved stem"
    );
    let third = with(&[3])[1];
    let c5: Vec<_> = with(&[5]);
    assert_eq!(c5.len(), 2);
    assert!(near(third[0].1, c5[1][0].1), "a third apart, together");
    let unison = with(&[7]);
    assert_eq!(unison.len(), 2);
    assert!(near(unison[0][0].1, unison[1][0].1), "a shared unison");

    // The constrained stage obliges each pair set apart within a column, and
    // the engraver's solve holds every obligation.
    let constrained = to_constrained(&to_logical(score));
    let by_id: std::collections::BTreeMap<_, _> =
        constrained.glyphs.iter().map(|g| (g.id(), g)).collect();
    let within = constrained
        .constraints
        .iter()
        .filter(|c| match c {
            LayoutConstraint::NoCollision { a, b } => {
                by_id[a].horizontal_slot == by_id[b].horizontal_slot
            }
            _ => false,
        })
        .count();
    assert_eq!(within, 8, "one obligation per second set apart");
    let report =
        epiphany_engrave::Engraver::default().solve(&constrained, &SolverConfig::default());
    assert!(report.unsatisfied_constraints.is_empty());
}

/// A score's opening time signature stands a clear gap after the clef and
/// key, and its first note a clear gap after the time signature, with or
/// without a key signature.
#[test]
fn the_opening_time_signature_clears_the_lead_and_the_music() {
    for (name, fifths) in [("opening_c.musicxml", 0), ("opening_f.musicxml", -1)] {
        let xml = format!(
            "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
             <part-name>A</part-name></score-part></part-list><part id=\"P1\">\
             <measure number=\"1\"><attributes><divisions>1</divisions>\
             <key><fifths>{fifths}</fifths></key><time><beats>2</beats>\
             <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
             </attributes><note><pitch><step>A</step><octave>4</octave></pitch>\
             <duration>2</duration><voice>1</voice><type>half</type></note></measure>\
             </part></score-partwise>"
        );
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::write(&path, xml).expect("written");
        let loaded = load(&path).expect("loads");
        let layout = engrave(&loaded.reduced.score).layout;
        let ink = |pick: &dyn Fn(&str) -> bool| -> (f32, f32) {
            layout
                .glyphs
                .iter()
                .filter(|g| pick(g.glyph.as_str()))
                .map(|g| {
                    let x = g.position.x.0;
                    (x + g.bounding_box.left.0, x + g.bounding_box.right.0)
                })
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), (a, b)| {
                    (l.min(a), r.max(b))
                })
        };
        let (_, lead) = ink(&|g| g == "gClef" || g.starts_with("accidental"));
        let (digits_left, digits_right) = ink(&|g| g.starts_with("timeSig"));
        let (head, _) = ink(&|g| g.starts_with("notehead"));
        assert!(
            digits_left - lead >= 0.8 - 1e-3,
            "{name}: the time signature stands {} after the lead",
            digits_left - lead
        );
        assert!(
            head - digits_right >= 1.0 - 1e-3,
            "{name}: the first note stands {} after the time signature",
            head - digits_right
        );
    }
}

/// Every system opens with the clef and key signature in force, the key a
/// clear gap after the clef; a treble clef an octave down and a percussion
/// clef draw as themselves.
#[test]
fn every_system_starts_with_its_clef_and_key() {
    use epiphany_cli::omissions::omissions;
    use epiphany_core::TypedObjectId;

    let staff = |sign: &str, line: u8, change: i8, step: &str, octave: u8, count: usize| {
        let mut measures = String::new();
        for m in 1..=count {
            measures.push_str(&format!("<measure number=\"{m}\">"));
            if m == 1 {
                measures.push_str(&format!(
                    "<attributes><divisions>1</divisions><key><fifths>-2</fifths></key>\
                     <time><beats>4</beats><beat-type>4</beat-type></time>\
                     <clef><sign>{sign}</sign><line>{line}</line>\
                     <clef-octave-change>{change}</clef-octave-change></clef></attributes>"
                ));
            }
            for _ in 0..4 {
                measures.push_str(&format!(
                    "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
                     <duration>1</duration><voice>1</voice></note>"
                ));
            }
            measures.push_str("</measure>");
        }
        measures
    };
    let score = |measures: String| {
        format!(
            "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
             <part-name>Cello</part-name></score-part></part-list>\
             <part id=\"P1\">{measures}</part></score-partwise>"
        )
    };
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("leads.musicxml");
    std::fs::write(&path, score(staff("F", 4, 0, "D", 3, 160))).expect("written");
    let loaded = load(&path).expect("loads");
    let engraved = engrave(&loaded.reduced.score);
    let layout = &engraved.layout;
    assert!(layout.systems().count() > 2, "the score wraps");
    let found = omissions(&loaded.reduced.score, layout, &engraved.diagnostics);
    assert_eq!(found.kinds.get("clef at a system start"), None, "{found:?}");
    assert_eq!(found.kinds.get("key signature at a system start"), None);
    for system in layout.systems() {
        let lead: Vec<_> = system
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .filter(|g| matches!(g.provenance.source, TypedObjectId::StaffInstance(_)))
            .collect();
        let clef = lead
            .iter()
            .find(|g| g.glyph.as_str() == "fClef")
            .expect("the system opens with its clef");
        let flats: Vec<_> = lead
            .iter()
            .filter(|g| g.glyph.as_str() == "accidentalFlat")
            .collect();
        assert_eq!(flats.len(), 2, "and its key");
        let clef_right = clef.position.x.0 + clef.bounding_box.right.0;
        for flat in &flats {
            assert!(flat.position.x.0 + flat.bounding_box.left.0 >= clef_right + 0.5);
        }
        // The system's music starts clear of its lead.
        let lead_right = lead
            .iter()
            .map(|g| g.position.x.0 + g.bounding_box.right.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let music_left = system
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .filter(|g| matches!(g.provenance.source, TypedObjectId::Pitch(_)))
            .map(|g| g.position.x.0 + g.bounding_box.left.0)
            .fold(f32::INFINITY, f32::min);
        assert!(music_left > lead_right, "{music_left} after {lead_right}");
    }

    for (sign, line, change, name) in [
        ("G", 2, -1, "gClef8vb"),
        ("percussion", 3, 0, "unpitchedPercussionClef1"),
    ] {
        std::fs::write(&path, score(staff(sign, line, change, "B", 3, 2))).expect("written");
        let loaded = load(&path).expect("loads");
        let layout = engrave(&loaded.reduced.score).layout;
        assert!(
            layout.glyphs.iter().any(|g| g.glyph.as_str() == name),
            "{name} is drawn"
        );
    }
}

/// A vertical band's height means what its kind says, and the solver realizes
/// it on a full orchestral system: twenty-seven staves, their notes reaching
/// above and below their staves by turns, each pair of adjacent staves set to
/// the inter-staff band's ink clearance, which the quality census measures
/// as realized everywhere.
#[test]
fn a_27_staff_system_realizes_every_inter_staff_clearance() {
    use epiphany_engrave::Engraver;
    use epiphany_layout_ir::{
        to_constrained, to_logical, ConstraintSolver, SolveStatus, SolverConfig,
    };

    let mut list = String::new();
    let mut parts = String::new();
    for p in 1..=27 {
        list.push_str(&format!(
            "<score-part id=\"P{p}\"><part-name>Part {p}</part-name></score-part>"
        ));
        // Every third part climbs above its staff, every third dives below,
        // the rest stay inside it.
        let octave = [6, 3, 5][p % 3];
        let mut measures = String::new();
        for m in 1..=3 {
            measures.push_str(&format!("<measure number=\"{m}\">"));
            if m == 1 {
                measures.push_str(
                    "<attributes><divisions>1</divisions><time><beats>4</beats>\
                     <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                     </attributes>",
                );
            }
            for step in ["C", "E", "G", "B"] {
                measures.push_str(&format!(
                    "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
                     <duration>1</duration><voice>1</voice></note>"
                ));
            }
            measures.push_str("</measure>");
        }
        parts.push_str(&format!("<part id=\"P{p}\">{measures}</part>"));
    }
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list>{list}</part-list>{parts}</score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("twenty_seven.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let report = Engraver::default().solve(
        &to_constrained(&to_logical(&loaded.reduced.score)),
        &SolverConfig::default(),
    );
    assert_eq!(report.status, SolveStatus::Solved, "{:?}", report.warnings);
    let systems: Vec<_> = report.layout.systems().collect();
    assert_eq!(systems.len(), 1, "three measures make one system");
    assert_eq!(systems[0].staves.len(), 27);
    // The census's realized clearances against the bands' declared heights.
    assert!(
        report.metric_vector.vertical_density_penalty.0 < 1e-4,
        "every inter-staff clearance is realized: {}",
        report.metric_vector.vertical_density_penalty.0
    );
    // The staves stand top to bottom in part order, the climbing and diving
    // ones further apart than the plain ones.
    let tops: Vec<f32> = systems[0]
        .staves
        .iter()
        .map(|s| s.bounding_box.origin.y.0)
        .collect();
    assert!(tops.windows(2).all(|w| w[0] > w[1]));
    let pitches: Vec<f32> = tops.windows(2).map(|w| w[0] - w[1]).collect();
    let (least, most) = pitches
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &p| {
            (a.min(p), b.max(p))
        });
    assert!(most > least + 2.0, "pitches {least}..{most}");
}

/// Staff groups mark their staves where each system stands: a line opens the
/// system across all its staves, a brace marks the piano and a bracket the
/// two parts that share it, and barlines run unbroken through a group's
/// staves but not from one group to the next.
#[test]
fn groups_mark_their_staves_and_join_their_barlines() {
    use epiphany_core::TypedObjectId;
    use epiphany_engrave::casting::{GROUP_SIGN_SYNTHESIS, JOINED_BARLINE_SYNTHESIS};
    use epiphany_layout_ir::SynthesisKind;

    let loaded = load(&fixture("groups.musicxml")).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    let registered = |k| Some(SynthesisKind::Registered(k));
    let signs: Vec<_> = layout
        .strokes
        .iter()
        .filter(|s| s.provenance.synthesis == registered(GROUP_SIGN_SYNTHESIS))
        .collect();
    let system = layout.systems().next().expect("a system");
    let (top, bottom) =
        system
            .staves
            .iter()
            .fold((f32::NEG_INFINITY, f32::INFINITY), |(t, b), s| {
                let y = s.bounding_box.origin.y.0;
                (t.max(y + s.bounding_box.size.height.0), b.min(y))
            });
    // The opening line spans the whole system, from the region.
    let opening = signs
        .iter()
        .find(|s| {
            matches!(s.provenance.source, TypedObjectId::Region(_))
                && (s.from.y.0 - bottom).abs() < 1e-3
                && (s.to.y.0 - top).abs() < 1e-3
        })
        .expect("an opening line");
    let line_x = opening.from.x.0;
    // The staves top to bottom, each as (bottom, top): the flute, the oboe,
    // the piano's two, the two violins, the cello.
    let mut staves: Vec<(f32, f32)> = system
        .staves
        .iter()
        .map(|s| {
            let y = s.bounding_box.origin.y.0;
            (y, y + s.bounding_box.size.height.0)
        })
        .collect();
    staves.sort_by(|a, b| b.0.total_cmp(&a.0));
    assert_eq!(staves.len(), 7);
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
    // Each group's upright sign stands left of the opening line, over its
    // own staves: the bracket's line over the flute and oboe, the brace over
    // the piano, the sub-bracket's line over the violins.
    let upright: Vec<(f32, f32, f32)> = signs
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, TypedObjectId::StaffGroup(_)) && s.from.x == s.to.x
        })
        .map(|s| (s.from.x.0 + s.thickness.0 / 2.0, s.from.y.0, s.to.y.0))
        .collect();
    assert_eq!(upright.len(), 2, "{upright:?}");
    for (right, from, to) in &upright {
        assert!(
            *right < line_x,
            "a sign's line at {right} reaches the opening line"
        );
        assert!(
            (near(*from, staves[1].0) && near(*to, staves[0].1))
                || (near(*from, staves[5].0) && near(*to, staves[4].1)),
            "a sign's line spans {from}..{to}"
        );
    }
    let brace = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "brace")
        .expect("a brace");
    assert!(matches!(
        brace.provenance.source,
        TypedObjectId::StaffGroup(_)
    ));
    let [[sx, ..], [_, sy, _], _] = brace.transform.expect("a scaled brace").matrix;
    let b = &brace.bounding_box;
    assert!(brace.position.x.0 + b.right.0 * sx < line_x);
    assert!((brace.position.y.0 + b.bottom.0 * sy - staves[3].0).abs() < 0.05);
    assert!((brace.position.y.0 + b.top.0 * sy - staves[2].1).abs() < 0.05);
    let group_glyphs: Vec<&str> = layout
        .glyphs
        .iter()
        .filter(|g| matches!(g.provenance.source, TypedObjectId::StaffGroup(_)))
        .map(|g| g.glyph.as_str())
        .collect();
    assert!(group_glyphs.contains(&"bracketTop") && group_glyphs.contains(&"bracketBottom"));
    // Joined barlines: one measure, so each of the seven staves has one final
    // barline of two lines; the bracket joins its two staves, the brace the
    // piano's two, the square sub-bracket its two; the cello stands alone.
    // Each join closes one gap exactly, from the lower staff's top to the
    // upper's bottom: the gaps under the flute, the piano's upper staff and
    // the first violins, two lines each, and none between groups.
    let mut gaps: Vec<usize> = layout
        .strokes
        .iter()
        .filter(|s| s.provenance.synthesis == registered(JOINED_BARLINE_SYNTHESIS))
        .map(|s| {
            (0..staves.len() - 1)
                .find(|&k| near(s.from.y.0, staves[k + 1].1) && near(s.to.y.0, staves[k].0))
                .unwrap_or_else(|| panic!("a join spans {}..{}", s.from.y.0, s.to.y.0))
        })
        .collect();
    gaps.sort();
    assert_eq!(gaps, [0, 0, 2, 2, 4, 4]);
}
