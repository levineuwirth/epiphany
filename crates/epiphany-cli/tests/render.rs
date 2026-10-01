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
