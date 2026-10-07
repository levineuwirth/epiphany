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

/// The staff each event of a score stands on, and each pitch's event.
fn staves_of(
    score: &epiphany_core::Score,
) -> (
    std::collections::BTreeMap<epiphany_core::EventId, epiphany_core::StaffId>,
    std::collections::BTreeMap<epiphany_core::PitchId, epiphany_core::EventId>,
) {
    let mut staff_of = std::collections::BTreeMap::new();
    for region in &score.canvas.regions {
        for instance in region.staff_instances() {
            for voice in &instance.voices {
                for event in &voice.events {
                    staff_of.insert(*event, instance.staff);
                }
            }
        }
    }
    let mut event_of = std::collections::BTreeMap::new();
    for event in score.events.iter() {
        if let epiphany_core::Event::Pitched(p) = event {
            for pitch in &p.pitches {
                event_of.insert(pitch.id, p.id);
            }
        }
    }
    (staff_of, event_of)
}

/// Checks every beam of `score` that joins notes on two staves against its
/// engraving: its primary beam at least a space and a half long and rising
/// at most half a space per space, each note on its own staff, the upper
/// staff's stems turned down to the beam and the lower's up, each stem
/// meeting the outermost beam over it, and none shorter than
/// `CROSS_STEM_MIN` from its head to the nearest beam it meets. Returns how many such beams there are, the shortest such
/// stem, and the gap between the two staves' lines.
fn check_cross_staff_beams(
    score: &epiphany_core::Score,
    layout: &epiphany_layout_ir::ResolvedLayoutIR,
) -> (usize, f32, f32) {
    use epiphany_core::TypedObjectId;
    use epiphany_layout_ir::{is_beam_stroke, Stroke};

    let (staff_of, event_of) = staves_of(score);
    // Each staff's lines, bottom and top.
    let mut lines: std::collections::BTreeMap<epiphany_core::StaffId, (f32, f32)> =
        std::collections::BTreeMap::new();
    for stroke in &layout.strokes {
        if let TypedObjectId::Staff(staff) = stroke.provenance.source {
            if stroke.from.y == stroke.to.y {
                let y = stroke.from.y.0;
                let entry = lines.entry(staff).or_insert((y, y));
                entry.0 = entry.0.min(y);
                entry.1 = entry.1.max(y);
            }
        }
    }
    assert_eq!(lines.len(), 2, "one system of two staves");
    let mut order: Vec<_> = lines.iter().map(|(s, l)| (*s, *l)).collect();
    order.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
    let (upper, lower) = (order[0], order[1]);
    let middle = |(_, (bottom, top)): (epiphany_core::StaffId, (f32, f32))| (bottom + top) / 2.0;
    let heads_of = |event: epiphany_core::EventId| -> Vec<f32> {
        layout
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("notehead"))
            .filter(|g| match g.provenance.source {
                TypedObjectId::Pitch(p) => event_of.get(&p) == Some(&event),
                _ => false,
            })
            .map(|g| g.position.y.0)
            .collect()
    };
    let beam_y = |beam: &Stroke, x: f32| {
        let t = (x - beam.from.x.0) / (beam.to.x.0 - beam.from.x.0);
        beam.from.y.0 + t * (beam.to.y.0 - beam.from.y.0)
    };
    let mut crossing = 0;
    let mut shortest = f32::INFINITY;
    for beam in &score.cross_cutting.beams {
        let staves: std::collections::BTreeSet<_> =
            beam.events.iter().map(|e| staff_of[e]).collect();
        if staves.len() < 2 {
            continue;
        }
        crossing += 1;
        let strokes: Vec<&Stroke> = layout
            .strokes
            .iter()
            .filter(|s| {
                is_beam_stroke(s)
                    && beam.events.iter().all(|e| {
                        s.provenance
                            .dependencies
                            .contains(&TypedObjectId::Event(*e))
                    })
            })
            .collect();
        assert!(!strokes.is_empty(), "a beam across two staves is drawn");
        // Its primary beam, the longest, runs at least a space and a half,
        // rising by at most half a space per space.
        let primary = strokes
            .iter()
            .max_by(|a, b| (a.to.x.0 - a.from.x.0).total_cmp(&(b.to.x.0 - b.from.x.0)))
            .expect("a beam stroke");
        let run = primary.to.x.0 - primary.from.x.0;
        assert!(run >= 1.49, "a beam across two staves runs {run}");
        let slope = (primary.to.y.0 - primary.from.y.0) / run;
        assert!(
            slope.abs() <= 0.51,
            "a beam across two staves rises {slope}"
        );
        for event in &beam.events {
            let heads = heads_of(*event);
            assert!(!heads.is_empty());
            let on_upper = staff_of[event] == upper.0;
            for y in &heads {
                let own = if on_upper {
                    middle(upper)
                } else {
                    middle(lower)
                };
                let other = if on_upper {
                    middle(lower)
                } else {
                    middle(upper)
                };
                assert!((y - own).abs() < (y - other).abs(), "a head left its staff");
            }
            let stem = layout
                .strokes
                .iter()
                .find(|s| {
                    s.provenance.source == TypedObjectId::Event(*event)
                        && s.from.x == s.to.x
                        && s.from.y != s.to.y
                })
                .expect("a member's stem");
            let x = stem.from.x.0;
            let centres: Vec<f32> = strokes
                .iter()
                .filter(|b| b.from.x.0 - 0.01 <= x && x <= b.to.x.0 + 0.01)
                .map(|b| beam_y(b, x))
                .collect();
            assert!(!centres.is_empty(), "a beam over every member");
            let half = strokes[0].thickness.0 / 2.0;
            let top = centres.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let bottom = centres.iter().copied().fold(f32::INFINITY, f32::min);
            if on_upper {
                assert!(stem.to.y.0 < stem.from.y.0, "an upper stem turns down");
                assert!(
                    (stem.to.y.0 - bottom).abs() <= half + 0.17,
                    "a down stem meets its outermost beam"
                );
                let head = heads.iter().copied().fold(f32::INFINITY, f32::min);
                shortest = shortest.min(head - (top + half));
            } else {
                assert!(stem.to.y.0 > stem.from.y.0, "a lower stem turns up");
                assert!(
                    (stem.to.y.0 - top).abs() <= half + 0.17,
                    "an up stem meets its outermost beam"
                );
                let head = heads.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                shortest = shortest.min((bottom - half) - head);
            }
        }
    }
    (crossing, shortest, upper.1 .0 - lower.1 .1)
}

/// A beam joining one voice's notes on both staves of a part stands between
/// them: each note on its own staff, the upper staff's stems turned down to
/// the beam and the lower's up, every stem meeting it after the staves are
/// spaced, each at least `CROSS_STEM_MIN` from its head to the nearest beam
/// it meets, and no flag left. Where the staves' own ink would let them
/// close further, the stems reaching the beam from the lower staff hold them
/// at that least length.
#[test]
fn a_beam_across_two_staves_joins_its_notes_between_them() {
    use epiphany_layout_ir::CROSS_STEM_MIN;

    let loaded = load(&fixture("cross_staff.musicxml")).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    let (crossing, shortest, _) = check_cross_staff_beams(&loaded.reduced.score, &layout);
    assert_eq!(crossing, 7, "the fixture's beams across the staves");
    assert!(
        shortest >= CROSS_STEM_MIN - 0.05,
        "a stem too short: {shortest}"
    );
    assert!(layout
        .glyphs
        .iter()
        .all(|g| !g.glyph.as_str().starts_with("flag")));
    // The staves realize their band's preferred clearance, and the vertical
    // metric, which leaves the beams' ink out of both staves as the solve
    // does, measures no deviation.
    use epiphany_layout_ir::ConstraintSolver;
    let report = epiphany_engrave::Engraver::default().solve(
        &epiphany_layout_ir::to_constrained(&epiphany_layout_ir::to_logical(&loaded.reduced.score)),
        &epiphany_layout_ir::SolverConfig::default(),
    );
    let penalty = report.metric_vector.vertical_density_penalty.0;
    assert!(penalty < 1e-3, "vertical density penalty {penalty}");

    // Two bass staves, whose ink stays within their lines, each beam joining
    // the lower staff's top line to the upper's bottom line: the staves
    // would close to the band's preferred clearance, nearer than the beam
    // between them allows, so its stems from the lower staff hold them apart.
    let note = |step: &str, octave: u8, staff: u8, beam: &str| {
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>1</duration><voice>1</voice><type>eighth</type><staff>{staff}</staff>\
             <beam number=\"1\">{beam}</beam></note>"
        )
    };
    let mut notes = String::new();
    for _ in 0..4 {
        notes += &note("A", 3, 2, "begin");
        notes += &note("G", 2, 1, "end");
    }
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Piano</part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>2</divisions>\
         <time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves>\
         <clef number=\"1\"><sign>F</sign><line>4</line></clef>\
         <clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>\
         {notes}</measure></part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("close_cross_staff.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    let (crossing, shortest, gap) = check_cross_staff_beams(&loaded.reduced.score, &layout);
    assert_eq!(crossing, 4);
    assert!(
        (shortest - CROSS_STEM_MIN).abs() < 0.05,
        "the stems reaching the beam hold the staves at their least length: {shortest}, \
         the staves {gap} apart"
    );
}

/// A tuplet whose notes are one beam across two staves is drawn by the beam:
/// its number alone, no bracket, standing above the beam between the staves,
/// clear of every stem and beam, within the group's span.
#[test]
fn a_tuplet_on_a_beam_across_two_staves_takes_its_number_by_the_beam() {
    use epiphany_core::TypedObjectId;
    use epiphany_layout_ir::is_beam_stroke;

    let loaded = load(&fixture("cross_staff.musicxml")).expect("loads");
    let score = &loaded.reduced.score;
    let layout = engrave(score).layout;
    let (staff_of, _) = staves_of(score);
    let mut numbered = 0;
    for tuplet in &score.cross_cutting.tuplets {
        let staves: std::collections::BTreeSet<_> =
            tuplet.members.iter().map(|e| staff_of[e]).collect();
        if staves.len() < 2 {
            continue;
        }
        let beam = score
            .cross_cutting
            .beams
            .iter()
            .find(|b| {
                let mut a = b.events.clone();
                let mut m = tuplet.members.clone();
                a.sort();
                m.sort();
                a == m
            })
            .expect("the fixture's cross-staff tuplets are each one beam");
        let digits: Vec<_> = layout
            .glyphs
            .iter()
            .filter(|g| g.provenance.source == TypedObjectId::Tuplet(tuplet.id))
            .collect();
        assert!(
            layout
                .strokes
                .iter()
                .all(|s| s.provenance.source != TypedObjectId::Tuplet(tuplet.id) || s.from == s.to),
            "no bracket"
        );
        if tuplet.display == epiphany_core::TupletDisplay::HIDDEN {
            assert!(digits.is_empty(), "a hidden tuplet shows no number");
            continue;
        }
        numbered += 1;
        assert_eq!(digits.len(), 1);
        assert_eq!(digits[0].glyph.as_str(), "tuplet6");
        let number = glyph_box(digits[0]);
        let beams: Vec<_> = layout
            .strokes
            .iter()
            .filter(|s| {
                is_beam_stroke(s)
                    && s.provenance
                        .dependencies
                        .contains(&TypedObjectId::Event(beam.events[0]))
            })
            .collect();
        let stems: Vec<_> = layout
            .strokes
            .iter()
            .filter(|s| {
                beam.events
                    .iter()
                    .any(|e| s.provenance.source == TypedObjectId::Event(*e))
                    && s.from.x == s.to.x
            })
            .collect();
        for ink in &stems {
            assert!(
                !boxes_overlap(number, stroke_box(ink)),
                "the number touches its group's ink"
            );
        }
        let (first, last) = stems
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), s| {
                (a.min(s.from.x.0), b.max(s.from.x.0))
            });
        assert!(
            number[0] >= first && number[2] <= last,
            "the number stands within its group"
        );
        // Above the beam: higher than the beam's ink across the number.
        let top = beams
            .iter()
            .filter(|b| b.from.x.0 <= number[2] && number[0] <= b.to.x.0)
            .map(|b| stroke_box(b)[3])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(number[1] > top, "the number stands above the beam");
    }
    assert_eq!(numbered, 3, "each shown cross-staff tuplet numbered");
}

/// A tuplet whose notes stand on two staves but are not exactly one beam is
/// still drawn. Two sharing one beam each take their number by it, centred
/// on their own notes, with no bracket; one a rest opens takes a bracket from
/// the rest to its last note, above its notes and the beam and clear of
/// them, its number in the gap; one whose ink stands on one staff, a hidden
/// rest on the other, is drawn on that staff with its bracket, as a tuplet
/// opening on a hidden rest is. A quarter-note triplet across the staves,
/// which no beam joins, is not drawn, and the omission count names it.
#[test]
fn a_tuplet_across_two_staves_that_is_not_one_beam_is_drawn() {
    use epiphany_core::{Event, TypedObjectId};
    use epiphany_layout_ir::is_beam_stroke;

    let loaded = load(&fixture("cross_staff_tuplets.musicxml")).expect("loads");
    let score = &loaded.reduced.score;
    let engraved = engrave(score);
    let layout = &engraved.layout;
    let (staff_of, _) = staves_of(score);
    let digits = |id| -> Vec<[f32; 4]> {
        layout
            .glyphs
            .iter()
            .filter(|g| {
                g.provenance.source == TypedObjectId::Tuplet(id)
                    && g.glyph.as_str().starts_with("tuplet")
            })
            .map(glyph_box)
            .collect()
    };
    let bracket = |id| -> Vec<&epiphany_layout_ir::Stroke> {
        layout
            .strokes
            .iter()
            .filter(|s| s.provenance.source == TypedObjectId::Tuplet(id) && s.from != s.to)
            .collect()
    };
    // A member's ink: its heads and its stem.
    let ink_of = |e: epiphany_core::EventId| -> Vec<[f32; 4]> {
        let pitches: Vec<TypedObjectId> = match score.events.get(e) {
            Some(Event::Pitched(p)) => p
                .pitches
                .iter()
                .map(|ip| TypedObjectId::Pitch(ip.id))
                .collect(),
            _ => Vec::new(),
        };
        layout
            .glyphs
            .iter()
            .filter(|g| pitches.contains(&g.provenance.source))
            .map(glyph_box)
            .chain(
                layout
                    .strokes
                    .iter()
                    .filter(|s| {
                        s.provenance.source == TypedObjectId::Event(e) && s.from.x == s.to.x
                    })
                    .map(stroke_box),
            )
            .collect()
    };
    let stem_x = |e: epiphany_core::EventId| {
        layout
            .strokes
            .iter()
            .find(|s| s.provenance.source == TypedObjectId::Event(e) && s.from.x == s.to.x)
            .map(|s| s.from.x.0)
    };
    let beams: Vec<[f32; 4]> = layout
        .strokes
        .iter()
        .filter(|s| is_beam_stroke(s))
        .map(stroke_box)
        .collect();
    let rest_of = |e: epiphany_core::EventId| match score.events.get(e) {
        Some(Event::Rest(r)) => Some(r.visible),
        _ => None,
    };

    let (mut shared, mut opened, mut hidden_led, mut exact, mut unbeamed) = (0, 0, 0, 0, 0);
    for tuplet in &score.cross_cutting.tuplets {
        let staves: std::collections::BTreeSet<_> =
            tuplet.members.iter().map(|e| staff_of[e]).collect();
        assert_eq!(
            staves.len(),
            2,
            "each of the fixture's tuplets spans the staves"
        );
        let notes: Vec<_> = tuplet
            .members
            .iter()
            .copied()
            .filter(|e| rest_of(*e).is_none())
            .collect();
        let beam = score
            .cross_cutting
            .beams
            .iter()
            .find(|b| b.events.contains(&notes[0]));
        let number = digits(tuplet.id);
        match (rest_of(tuplet.members[0]), beam) {
            (None, None) => {
                unbeamed += 1;
                assert!(number.is_empty() && bracket(tuplet.id).is_empty());
                continue;
            }
            (Some(false), _) => {
                hidden_led += 1;
                assert_eq!(number.len(), 1, "numbered on its staff");
                assert_eq!(
                    bracket(tuplet.id).len(),
                    4,
                    "and bracketed, a hidden rest opening it"
                );
                continue;
            }
            _ => {}
        }
        let beam = beam.expect("its notes ride one beam");
        assert_eq!(number.len(), 1, "each shown tuplet numbered");
        let number = number[0];
        let xs: Vec<f32> = notes.iter().map(|e| stem_x(*e).expect("a stem")).collect();
        let (first, last) = (xs[0], *xs.last().expect("notes"));
        for e in &tuplet.members {
            for ink in ink_of(*e) {
                assert!(!boxes_overlap(number, ink), "the number clears its notes");
            }
        }
        let over = beams
            .iter()
            .filter(|b| b[0] <= number[2] && number[0] <= b[2])
            .map(|b| b[3])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(number[1] > over, "the number stands above the beam");
        if rest_of(tuplet.members[0]) == Some(true) {
            opened += 1;
            let strokes = bracket(tuplet.id);
            assert_eq!(strokes.len(), 4, "hooks and a line broken for the number");
            let boxes: Vec<[f32; 4]> = strokes.iter().map(|s| stroke_box(s)).collect();
            let left = boxes.iter().map(|b| b[0]).fold(f32::INFINITY, f32::min);
            let right = boxes.iter().map(|b| b[2]).fold(f32::NEG_INFINITY, f32::max);
            let rest = layout
                .glyphs
                .iter()
                .find(|g| g.provenance.source == TypedObjectId::Event(tuplet.members[0]))
                .map(glyph_box)
                .expect("the rest is drawn");
            assert!(
                left <= rest[0] + 0.5 && right >= last,
                "from the rest to its last note"
            );
            let line = strokes
                .iter()
                .filter(|s| s.from.y == s.to.y)
                .map(|s| s.from.y.0)
                .fold(f32::INFINITY, f32::min);
            let notes_top = notes
                .iter()
                .flat_map(|e| ink_of(*e))
                .map(|b| b[3])
                .fold(over, f32::max);
            assert!(
                line > notes_top,
                "the bracket stands above its notes and the beam"
            );
            for b in &boxes {
                for e in &notes {
                    for ink in ink_of(*e) {
                        assert!(!boxes_overlap(*b, ink), "the bracket clears its notes");
                    }
                }
            }
            continue;
        }
        assert!(
            bracket(tuplet.id).is_empty(),
            "a beam's tuplet takes no bracket"
        );
        assert!(
            number[0] >= first && number[2] <= last,
            "the number stands within its own notes"
        );
        if beam.events.len() > tuplet.members.len() {
            shared += 1;
        } else {
            exact += 1;
        }
    }
    assert_eq!(
        (shared, opened, hidden_led, exact, unbeamed),
        (2, 1, 1, 1, 1),
        "the fixture's shapes"
    );
    let count = omissions(score, layout, &engraved.diagnostics)
        .kinds
        .get("tuplet not drawn")
        .copied()
        .unwrap_or(0);
    assert_eq!(count, 1, "the unbeamed triplet alone is counted");
}

/// A tuplet's number across two staves stands clear of the notes' ink, not
/// only of their stems: a quarter space beside every head, ledger line,
/// accidental, dot and stem of the score, and half a space above or below
/// one. The falling groups set their upper staff's last heads close above the
/// beam, beside the middle where the number would stand, and some stand so
/// far below each staff that the beam falls below the lower staff's place
/// before the staves are solved; each number still stands above its beam,
/// within its own notes.
#[test]
fn a_tuplet_number_across_two_staves_stands_clear_of_heads() {
    use epiphany_core::{Event, TypedObjectId};
    use epiphany_layout_ir::is_beam_stroke;

    // A quarter and a half space, less the layout's rounding to its grid.
    const BESIDE: f32 = 0.25 - 0.005;
    const OVER: f32 = 0.5 - 0.005;
    let mut falling = 0;
    for name in [
        "cross_staff.musicxml",
        "cross_staff_tuplets.musicxml",
        "cross_staff_descending.musicxml",
    ] {
        let loaded = load(&fixture(name)).expect("loads");
        let score = &loaded.reduced.score;
        let layout = engrave(score).layout;
        let (staff_of, _) = staves_of(score);
        let ink: Vec<(String, [f32; 4])> = layout
            .glyphs
            .iter()
            .filter(|g| {
                let n = g.glyph.as_str();
                n.starts_with("notehead") || n.starts_with("accidental") || n == "augmentationDot"
            })
            .map(|g| (g.glyph.as_str().to_owned(), glyph_box(g)))
            .chain(
                layout
                    .strokes
                    .iter()
                    .filter(|s| !is_beam_stroke(s) && s.from != s.to)
                    .filter_map(|s| match s.provenance.source {
                        TypedObjectId::Pitch(_) if s.from.y == s.to.y => {
                            Some(("ledger line".to_owned(), stroke_box(s)))
                        }
                        TypedObjectId::Event(_) if s.from.x == s.to.x => {
                            Some(("stem".to_owned(), stroke_box(s)))
                        }
                        _ => None,
                    }),
            )
            .collect();
        assert!(
            ink.iter().any(|(n, _)| n == "ledger line"),
            "{name}: ledger lines found"
        );
        for tuplet in &score.cross_cutting.tuplets {
            let staves: std::collections::BTreeSet<_> =
                tuplet.members.iter().map(|e| staff_of[e]).collect();
            if staves.len() < 2 {
                continue;
            }
            for number in layout
                .glyphs
                .iter()
                .filter(|g| {
                    g.provenance.source == TypedObjectId::Tuplet(tuplet.id)
                        && g.glyph.as_str().starts_with("tuplet")
                })
                .map(glyph_box)
            {
                let grown = [
                    number[0] - BESIDE,
                    number[1] - OVER,
                    number[2] + BESIDE,
                    number[3] + OVER,
                ];
                for (what, b) in &ink {
                    assert!(
                        !boxes_overlap(grown, *b),
                        "{name}: a tuplet number {number:?} within reach of a {what} {b:?}"
                    );
                }
                // A falling group: its first note on the upper staff, so its
                // last upper heads stand nearest the beam at the knee.
                let first = staff_of[&tuplet.members[0]];
                let upper = staves.iter().copied().min_by_key(|s| {
                    score
                        .canvas
                        .regions
                        .iter()
                        .flat_map(|r| r.staff_instances())
                        .position(|i| i.staff == *s)
                });
                if name == "cross_staff_descending.musicxml"
                    && Some(first) == upper
                    && matches!(score.events.get(tuplet.members[0]), Some(Event::Pitched(_)))
                {
                    falling += 1;
                    // The top of the tuplet's own beam under the number, each
                    // stroke's edge taken along its slope at the number's ends.
                    let over = layout
                        .strokes
                        .iter()
                        .filter(|s| {
                            is_beam_stroke(s)
                                && tuplet.members.iter().any(|e| {
                                    s.provenance
                                        .dependencies
                                        .contains(&TypedObjectId::Event(*e))
                                })
                        })
                        .filter_map(|s| {
                            let (x1, y1, x2, y2) = (s.from.x.0, s.from.y.0, s.to.x.0, s.to.y.0);
                            let (lo, hi) = (x1.min(x2), x1.max(x2));
                            if hi < number[0] || lo > number[2] {
                                return None;
                            }
                            let at = |x: f32| {
                                if hi - lo < 1e-6 {
                                    y1
                                } else {
                                    y1 + (y2 - y1) * (x.clamp(lo, hi) - x1) / (x2 - x1)
                                }
                            };
                            Some(at(number[0]).max(at(number[2])) + s.thickness.0 / 2.0)
                        })
                        .fold(f32::NEG_INFINITY, f32::max);
                    assert!(over.is_finite(), "{name}: its beam stands under the number");
                    assert!(
                        number[1] > over,
                        "{name}: the number {number:?} stands above its beam ({over})"
                    );
                    let xs: Vec<f32> = layout
                        .strokes
                        .iter()
                        .filter(|s| {
                            s.from.x == s.to.x
                                && tuplet
                                    .members
                                    .iter()
                                    .any(|e| s.provenance.source == TypedObjectId::Event(*e))
                        })
                        .map(|s| s.from.x.0)
                        .collect();
                    let (a, z) = xs
                        .iter()
                        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, z), x| {
                            (a.min(*x), z.max(*x))
                        });
                    assert!(
                        number[0] >= a && number[2] <= z,
                        "{name}: the number stands within its own notes"
                    );
                }
            }
        }
    }
    assert_eq!(falling, 11, "the falling groups each numbered");
}

/// A tuplet across two staves that draws nothing keeps its traced anchor in
/// no staff's band, where it adds no extent to a staff it is not on: a
/// hidden tuplet that is exactly a beam, and a shown triplet no beam joins.
/// An anchor riding the upper staff's band at the frame's origin would
/// stretch that staff toward whatever stands there.
#[test]
fn a_tuplet_across_two_staves_that_draws_nothing_keeps_its_anchor_off_the_staves() {
    use epiphany_core::TypedObjectId;

    let mut anchors = 0;
    for name in ["cross_staff.musicxml", "cross_staff_tuplets.musicxml"] {
        let loaded = load(&fixture(name)).expect("loads");
        let score = &loaded.reduced.score;
        let layout = engrave(score).layout;
        let (staff_of, _) = staves_of(score);
        let staff_bands: std::collections::BTreeSet<_> = layout
            .strokes
            .iter()
            .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)))
            .map(|s| s.vertical_band)
            .collect();
        assert_eq!(staff_bands.len(), 2, "{name}: two staves' bands");
        for tuplet in &score.cross_cutting.tuplets {
            let staves: std::collections::BTreeSet<_> =
                tuplet.members.iter().map(|e| staff_of[e]).collect();
            let source = TypedObjectId::Tuplet(tuplet.id);
            let inked = layout.glyphs.iter().any(|g| g.provenance.source == source)
                || layout
                    .strokes
                    .iter()
                    .any(|s| s.provenance.source == source && s.from != s.to);
            if staves.len() < 2 || inked {
                continue;
            }
            anchors += 1;
            let anchor = layout
                .strokes
                .iter()
                .find(|s| s.provenance.source == source)
                .expect("an undrawn tuplet keeps a traced anchor");
            assert!(
                !staff_bands.contains(&anchor.vertical_band),
                "{name}: an undrawn tuplet's anchor rides a staff's band"
            );
        }
    }
    assert_eq!(
        anchors, 2,
        "the hidden beam's tuplet and the unbeamed triplet"
    );
}

/// A tuplet the file hides draws no number and no bracket; one whose number
/// is hidden draws its bracket unbroken; the rest as before: a beamed
/// triplet its number alone, an unbeamed one its number in its bracket.
#[test]
fn a_tuplet_the_file_hides_draws_no_number_or_bracket() {
    use epiphany_core::{TupletDisplay, TupletNumber, TypedObjectId};

    // Quarter-note triplets on one staff, unbeamed: hidden, numberless, and
    // plain; then the hand-written fixture's beamed eighth triplets.
    let triplet = |notations: &str| {
        (0..3)
            .map(|i| {
                let mark = match i {
                    0 => format!("{notations}<tuplet type=\"start\"/></notations>"),
                    2 => "<notations><tuplet type=\"stop\"/></notations>".to_string(),
                    _ => String::new(),
                };
                format!(
                    "<note><pitch><step>B</step><octave>4</octave></pitch><duration>4</duration>\
                     <voice>1</voice><type>quarter</type><time-modification>\
                     <actual-notes>3</actual-notes><normal-notes>2</normal-notes>\
                     </time-modification>{mark}</note>"
                )
            })
            .collect::<String>()
    };
    let measures = [
        triplet("<notations print-object=\"no\">"),
        triplet("<notations>").replacen(
            "<tuplet type=\"start\"/>",
            "<tuplet type=\"start\" show-number=\"none\"/>",
            1,
        ),
        triplet("<notations>"),
    ];
    let body: String = measures
        .iter()
        .enumerate()
        .map(|(m, notes)| {
            let attributes = if m == 0 {
                "<attributes><divisions>6</divisions><time><beats>2</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>"
            } else {
                ""
            };
            format!(
                "<measure number=\"{}\">{attributes}{notes}</measure>",
                m + 1
            )
        })
        .collect();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hidden_tuplets.musicxml");
    std::fs::write(&path, xml).expect("written");
    for loaded in [
        load(&path).expect("loads"),
        load(&fixture("cross_staff.musicxml")).expect("loads"),
    ] {
        assert_eq!(
            loaded.reduced.rejected().count(),
            0,
            "every operation applies"
        );
        let score = &loaded.reduced.score;
        let layout = engrave(score).layout;
        for tuplet in &score.cross_cutting.tuplets {
            let source = TypedObjectId::Tuplet(tuplet.id);
            let digits = layout
                .glyphs
                .iter()
                .filter(|g| g.provenance.source == source && g.glyph.as_str().starts_with("tuplet"))
                .count();
            let lines: Vec<_> = layout
                .strokes
                .iter()
                .filter(|s| s.provenance.source == source && s.from != s.to)
                .collect();
            if tuplet.display == TupletDisplay::HIDDEN {
                assert_eq!(
                    (digits, lines.len()),
                    (0, 0),
                    "a hidden tuplet shows nothing"
                );
            } else if tuplet.display.number == TupletNumber::None {
                assert_eq!(digits, 0, "a numberless tuplet shows no number");
                // Its two hooks and one unbroken line between them.
                let level = lines.iter().filter(|s| s.from.y == s.to.y).count();
                assert_eq!((lines.len(), level), (3, 1), "its bracket runs unbroken");
            } else {
                assert_eq!(digits, 1, "a shown tuplet shows its number");
            }
        }
    }
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
        // break starts before the first, after the system's lead, and one
        // breaking off ends after the last.
        let left = heads.iter().map(|h| h.0).fold(f32::INFINITY, f32::min);
        let lead = system
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .filter(|g| matches!(g.provenance.source, TypedObjectId::StaffInstance(_)))
            .map(|g| g.position.x.0 + g.bounding_box.right.0)
            .fold(f32::NEG_INFINITY, f32::max);
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
                after_head || (lead < x0 && x0 < left),
                "an arc starts just after a head, or between its system's lead and first: {x0}"
            );
            assert!(
                before_head || (right - x3).abs() < 1.0,
                "an arc ends just before a head, or at its system's end: {x3}"
            );
            assert!(after_head || before_head, "an arc meets a head it joins");
        }
    }
}

/// A tie continued across a system break starts its second half clear of the
/// system's lead (the widest staff's clef and key signature) or of a time
/// signature opening the system, and arcs as a tie of its own length to just
/// before its note, at least a tie's length: the system's lead makes room for
/// it, and the break search reserves that room, so no system runs past the
/// right margin.
#[test]
fn a_tie_continued_into_a_system_starts_clear_of_its_lead() {
    use epiphany_core::TypedObjectId;
    use epiphany_engrave::PageGeometry;

    // Two staves of whole notes tied over every barline: a treble staff's C4
    // on its ledger line, and a bass staff in four flats with a chord whose
    // ties arc above and below. With `meters`, every measure changes meter,
    // so every system opens with a time signature.
    let score = |meters: bool| {
        let count = 60;
        let part = |id: &str, clef: &str, fifths: i8, notes: &str| {
            let mut measures = String::new();
            for m in 1..=count {
                let (beats, value, kind) = if meters && m % 2 == 0 {
                    (2, 2, "half")
                } else {
                    (4, 4, "whole")
                };
                measures.push_str(&format!("<measure number=\"{m}\">"));
                if m == 1 || meters {
                    let opening = if m == 1 {
                        format!(
                            "<divisions>1</divisions><key><fifths>{fifths}</fifths></key>\
                             <time><beats>{beats}</beats><beat-type>4</beat-type></time>{clef}"
                        )
                    } else {
                        format!("<time><beats>{beats}</beats><beat-type>4</beat-type></time>")
                    };
                    measures.push_str(&format!("<attributes>{opening}</attributes>"));
                }
                let ties = match m {
                    1 => "<tie type=\"start\"/>",
                    m if m == count => "<tie type=\"stop\"/>",
                    _ => "<tie type=\"stop\"/><tie type=\"start\"/>",
                };
                measures.push_str(
                    &notes
                        .replace("{ties}", ties)
                        .replace("{value}", &value.to_string())
                        .replace("{kind}", kind),
                );
                measures.push_str("</measure>");
            }
            format!("<part id=\"{id}\">{measures}</part>")
        };
        let note = |step: &str, alter: i8, octave: u8, chord: bool| {
            format!(
                "<note>{}<pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}\
                 </octave></pitch><duration>{{value}}</duration>{{ties}}<voice>1</voice>\
                 <type>{{kind}}</type></note>",
                if chord { "<chord/>" } else { "" }
            )
        };
        format!(
            "<score-partwise version=\"4.0\"><part-list>\
             <score-part id=\"P1\"><part-name>A</part-name></score-part>\
             <score-part id=\"P2\"><part-name>B</part-name></score-part></part-list>{}{}\
             </score-partwise>",
            part(
                "P1",
                "<clef><sign>G</sign><line>2</line></clef>",
                0,
                &note("C", 0, 4, false)
            ),
            part(
                "P2",
                "<clef><sign>F</sign><line>4</line></clef>",
                -4,
                &(note("F", 0, 3, false) + &note("A", -1, 3, true))
            ),
        )
    };
    let geometry = PageGeometry::default();
    let margin_right = geometry.size.width.0 - geometry.margins.right.0;
    for meters in [false, true] {
        let name = format!(
            "continued_ties_{}.musicxml",
            if meters { "meters" } else { "one_meter" }
        );
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::write(&path, score(meters)).expect("written");
        let loaded = load(&path).expect("loads");
        // A tie joins events: the chord's two arcs are one tie.
        assert_eq!(loaded.reduced.score.cross_cutting.ties.len(), 2 * 59);
        let layout = engrave(&loaded.reduced.score).layout;
        let systems: Vec<_> = layout.systems().collect();
        assert!(systems.len() > 2, "the score wraps");
        let mut continued = 0;
        for (k, system) in systems.iter().enumerate() {
            let glyphs: Vec<_> = system
                .primitives
                .glyphs
                .iter()
                .map(|&i| &layout.glyphs[i as usize])
                .collect();
            let right_of =
                |g: &&epiphany_layout_ir::ResolvedGlyph| g.position.x.0 + g.bounding_box.right.0;
            let heads: Vec<_> = glyphs
                .iter()
                .filter(|g| g.glyph.as_str().starts_with("notehead"))
                .map(|g| g.position.x.0 + g.bounding_box.left.0)
                .collect();
            let first = heads.iter().copied().fold(f32::INFINITY, f32::min);
            // What stands before the system's first note: its lead, and with
            // `meters` its time signature.
            let lead = glyphs
                .iter()
                .filter(|g| matches!(g.provenance.source, TypedObjectId::StaffInstance(_)))
                .map(right_of)
                .fold(f32::NEG_INFINITY, f32::max);
            let opening = glyphs
                .iter()
                .filter(|g| g.glyph.as_str().starts_with("timeSig") && right_of(g) < first)
                .map(right_of)
                .fold(lead, f32::max);
            if k > 0 {
                assert_eq!(opening > lead, meters, "a time signature opens system {k}");
                // The system's first measure holds its opening columns.
                let measure = system.measures.first().expect("a measure").bounding_box;
                let left = glyphs
                    .iter()
                    .filter(|g| right_of(g) <= opening && right_of(g) > lead)
                    .map(|g| g.position.x.0 + g.bounding_box.left.0)
                    .fold(first, f32::min);
                assert!(
                    measure.origin.x.0 < left + 0.01,
                    "system {k}'s first measure starts at {left}, not {:?}",
                    measure.origin.x
                );
            }
            let ink_right = glyphs
                .iter()
                .map(right_of)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                ink_right <= margin_right + 0.01,
                "system {k} ends at {ink_right}, past the margin at {margin_right}"
            );
            for &i in &system.primitives.curves {
                let arc = &layout.curves[i as usize];
                let (x0, x3) = (arc.p0.x.0, arc.p3.x.0);
                if !matches!(arc.provenance.source, TypedObjectId::Tie(_)) || x0 > first {
                    continue;
                }
                continued += 1;
                assert!(k > 0, "the first system continues no tie");
                assert!(
                    (x0 - opening - 0.4).abs() < 1e-3,
                    "system {k}: a continued tie starts 0.4 clear of {opening}, not at {x0}"
                );
                // 0.15 clear of its note, or of the down-stem at its left
                // that the tie's height meets.
                assert!(
                    heads
                        .iter()
                        .any(|l| [0.15, 0.21].iter().any(|d| (l - x3 - d).abs() < 1e-3)),
                    "system {k}: a continued tie ends just before its note, not at {x3}"
                );
                // A justified system stretches the gap after a time signature.
                let most = if meters { f32::INFINITY } else { 1.6 };
                assert!(
                    (1.5..most).contains(&(x3 - x0)),
                    "system {k}: a continued tie runs a tie's length, not {}",
                    x3 - x0
                );
                let above = arc.p1.y.0 > arc.p0.y.0;
                let rule = epiphany_layout_ir::tie_arc(x0, x3, arc.p0.y.0, above);
                for (point, expected) in [arc.p0, arc.p1, arc.p2, arc.p3].iter().zip(rule) {
                    assert!(
                        (point.x.0 - expected.x.0).abs() < 1e-3
                            && (point.y.0 - expected.y.0).abs() < 1e-3,
                        "system {k}: a continued tie arcs as a tie of its length: \
                         {point:?} against {expected:?}"
                    );
                }
            }
        }
        assert_eq!(
            continued,
            3 * (systems.len() - 1),
            "three ties cross each break"
        );
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
        // The tied B natural shows nothing, and the next B natural shows its
        // own; then B-flat again.
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
        // F by the key, G, and F-sharp tied over the barline.
        [
            note("F", 0, 4, 1, ""),
            note("G", 0, 4, 1, ""),
            note("F", 1, 4, 2, "<tie type=\"start\"/>"),
        ]
        .concat(),
        // The tied F-sharp shows nothing; the F natural after it a courtesy
        // natural, though the key gives it; F-sharp again its sharp; E-flat
        // by the key nothing.
        [
            note("F", 1, 4, 1, "<tie type=\"stop\"/>"),
            note("F", 0, 4, 1, ""),
            note("F", 1, 4, 1, ""),
            note("E", -1, 4, 1, ""),
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
            None, None, s, // measure 5
            None, n, s, None, // measure 6, the first note tied over
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

/// The lower staff of a grand staff holds two voices of its own, the file's
/// 5 and 6, and the upper staff's voice 1 writes one note on it a measure
/// later. The lower staff's voices keep their sides: voice 5's rests,
/// stems and sextuplet above, voice 6's stems below, as they would were the
/// visiting note not there.
#[test]
fn a_voice_visiting_a_staff_leaves_its_own_voices_their_sides() {
    use epiphany_core::{Event, TypedObjectId};

    let note =
        |what: &str, duration: u8, voice: u8, kind: &str, six: bool, staff: u8, tail: &str| {
            let modification = if six {
                "<time-modification><actual-notes>6</actual-notes><normal-notes>4</normal-notes>\
             </time-modification>"
            } else {
                ""
            };
            format!(
            "<note>{what}<duration>{duration}</duration><voice>{voice}</voice><type>{kind}</type>\
             {modification}<staff>{staff}</staff>{tail}</note>"
        )
        };
    let pitch = |step: &str, alter: i8, octave: u8| {
        format!("<pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave></pitch>")
    };
    let start = "<notations><tuplet type=\"start\" bracket=\"yes\"/></notations>";
    let beam = |kind: &str| format!("<beam number=\"1\">{kind}</beam>");
    let end = "<beam number=\"1\">end</beam><notations><tuplet type=\"stop\"/></notations>";
    let backup = "<backup><duration>12</duration></backup>";
    let six = |p: String, staff: u8, tail: &str| note(&p, 1, 5, "eighth", true, staff, tail);
    let first = [
        // The upper staff's voice: two half notes.
        note(&pitch("C", 0, 5), 6, 1, "half", false, 1, ""),
        note(&pitch("D", 0, 5), 6, 1, "half", false, 1, ""),
        backup.to_owned(),
        // Voice 5: a sextuplet a rest opens, on the lower staff alone, then
        // one a rest opens whose first note stands on the upper staff.
        note("<rest/>", 1, 5, "eighth", true, 2, start),
        six(pitch("E", -1, 3), 2, &beam("begin")),
        six(pitch("D", -1, 3), 2, &beam("continue")),
        six(pitch("B", -1, 3), 2, &beam("continue")),
        six(pitch("A", -1, 3), 2, &beam("continue")),
        six(pitch("D", 0, 4), 2, end),
        note("<rest/>", 1, 5, "eighth", true, 2, start),
        six(pitch("A", 0, 4), 1, &beam("begin")),
        six(pitch("E", -1, 4), 2, &beam("continue")),
        six(pitch("B", -1, 3), 2, &beam("continue")),
        six(pitch("F", 0, 3), 2, &beam("continue")),
        six(pitch("C", 0, 3), 2, end),
        backup.to_owned(),
        // Voice 6: two half notes under it.
        note(&pitch("G", -1, 2), 6, 6, "half", false, 2, ""),
        note(&pitch("A", 0, 1), 6, 6, "half", false, 2, ""),
    ];
    let second = [
        // The upper staff's voice writes its third quarter on the lower
        // staff, over voice 5's whole note.
        note(&pitch("C", 0, 5), 3, 1, "quarter", false, 1, ""),
        note(&pitch("D", 0, 5), 3, 1, "quarter", false, 1, ""),
        note(&pitch("G", 0, 3), 3, 1, "quarter", false, 2, ""),
        note(&pitch("E", 0, 5), 3, 1, "quarter", false, 1, ""),
        backup.to_owned(),
        note(&pitch("C", 0, 3), 12, 5, "whole", false, 2, ""),
    ];
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>Piano\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>3</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef>\
         <clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>{}</measure>\
         <measure number=\"2\">{}</measure></part></score-partwise>",
        first.concat(),
        second.concat()
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("visiting_voice.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert!(loaded.fidelity.passed(), "{:#?}", loaded.fidelity.failures);
    let layout = engrave(&loaded.reduced.score).layout;
    let systems: Vec<_> = layout.systems().collect();
    assert_eq!(systems.len(), 1);
    let lower_staff = loaded.import.ids.staves[0][1];
    let lower = &systems[0]
        .staves
        .iter()
        .find(|s| s.staff == lower_staff)
        .expect("the lower staff")
        .bounding_box;
    let middle = lower.origin.y.0 + lower.size.height.0 / 2.0;

    // Each event's source note: its staff (0 the upper) and voice, and its
    // index among the part's events.
    let source = &loaded.import.source.parts[0].events;
    let events = &loaded.import.ids.events[0];
    let of = |voice: &str, staff: usize| -> Vec<(usize, epiphany_core::EventId)> {
        source
            .iter()
            .zip(events)
            .enumerate()
            .filter(|(_, (e, _))| e.voice == voice && e.staff == staff && e.measure == 0)
            .map(|(i, (_, id))| (i, *id))
            .collect()
    };
    // A note's heads and whether its stem turns up, from its one upright
    // stroke against its heads.
    let heads = |id: epiphany_core::EventId| -> Vec<[f32; 4]> {
        let Some(Event::Pitched(n)) = loaded.reduced.score.events.get(id) else {
            return Vec::new();
        };
        layout
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("notehead"))
            .filter(|g| matches!(g.provenance.source, TypedObjectId::Pitch(p) if n.pitches.iter().any(|q| q.id == p)))
            .map(glyph_box)
            .collect()
    };
    let stem_up = |id: epiphany_core::EventId| -> bool {
        let head = heads(id)[0];
        let stem = layout
            .strokes
            .iter()
            .find(|s| s.provenance.source == TypedObjectId::Event(id) && s.from.x == s.to.x)
            .expect("a stem");
        stem.from.y.0.max(stem.to.y.0) > head[3] + 1.0
    };

    // Voice 5's rests above the middle line, and the stems of its notes
    // on the lower staff up.
    let upper_voice = of("5", 1);
    let rests: Vec<f32> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
        .filter(|g| {
            upper_voice
                .iter()
                .any(|(_, id)| g.provenance.source == TypedObjectId::Event(*id))
        })
        .map(|g| {
            let b = glyph_box(g);
            (b[1] + b[3]) / 2.0 - middle
        })
        .collect();
    assert_eq!(rests.len(), 2);
    for (k, rest) in rests.iter().enumerate() {
        assert!(
            *rest > 0.5,
            "voice 5's rest {k} stands {rest} from the middle line"
        );
    }
    let notes: Vec<_> = upper_voice
        .iter()
        .filter(|(i, _)| {
            !matches!(
                source[*i].content,
                epiphany_musicxml::source::Content::Rest { .. }
            )
        })
        .collect();
    assert_eq!(notes.len(), 9);
    for (i, id) in &notes {
        assert!(stem_up(*id), "voice 5's note {i} turns its stem down");
    }
    // Voice 6's stems down.
    let lower_voice = of("6", 1);
    assert_eq!(lower_voice.len(), 2);
    for (i, id) in &lower_voice {
        assert!(!stem_up(*id), "voice 6's note {i} turns its stem up");
    }
    // Voice 5's first sextuplet, on the lower staff alone, takes its number
    // and bracket above its notes.
    let first_rest = upper_voice[0].1;
    let tuplet = loaded
        .reduced
        .score
        .cross_cutting
        .tuplets
        .iter()
        .find(|t| t.members.first() == Some(&first_rest))
        .expect("the first sextuplet");
    let top = tuplet
        .members
        .iter()
        .flat_map(|id| heads(*id))
        .map(|h| h[3])
        .fold(f32::MIN, f32::max);
    let number: Vec<[f32; 4]> = layout
        .glyphs
        .iter()
        .filter(|g| g.provenance.source == TypedObjectId::Tuplet(tuplet.id))
        .map(glyph_box)
        .collect();
    assert!(!number.is_empty(), "the first sextuplet draws its number");
    let bracket: Vec<[f32; 4]> = layout
        .strokes
        .iter()
        .filter(|s| s.provenance.source == TypedObjectId::Tuplet(tuplet.id) && s.from.y == s.to.y)
        .map(stroke_box)
        .collect();
    assert!(!bracket.is_empty(), "the first sextuplet draws its bracket");
    for ink in number.iter().chain(&bracket) {
        assert!(
            ink[1] > top,
            "the first sextuplet's mark stands at {} under its notes' top {top}",
            ink[1]
        );
    }
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
            rest(2, 1),
            rest(4, 1),
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
        ("G", 2, 2, "gClef15ma"),
        ("F", 4, -2, "fClef15mb"),
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

/// A score of `measures` measures of mixed quarters and eighths, some with
/// ledger lines, on a page `width` staff spaces wide as its `<defaults>` set
/// it (in tenths, ten to a staff space), or on the default page.
fn paged_score(measures: usize, width: Option<f32>) -> String {
    let mut body = String::new();
    for m in 1..=measures {
        body.push_str(&format!("<measure number=\"{m}\">"));
        if m == 1 {
            body.push_str(
                "<attributes><divisions>2</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>",
            );
        }
        let notes: &[(&str, u8, u8)] = match m % 3 {
            0 => &[
                ("C", 4, 2),
                ("A", 5, 2),
                ("E", 4, 1),
                ("F", 4, 1),
                ("G", 4, 2),
            ],
            1 => &[
                ("D", 4, 1),
                ("E", 4, 1),
                ("F", 4, 2),
                ("C", 6, 2),
                ("B", 4, 2),
            ],
            _ => &[("G", 4, 4), ("C", 4, 1), ("D", 4, 1), ("A", 3, 2)],
        };
        for (step, octave, duration) in notes {
            body.push_str(&pitched(step, 0, *octave, *duration, 1, false));
        }
        body.push_str("</measure>");
    }
    let defaults = width.map_or(String::new(), |w| {
        format!(
            "<defaults><scaling><millimeters>7</millimeters><tenths>40</tenths></scaling>\
             <page-layout><page-width>{}</page-width><page-height>1500</page-height>\
             <page-margins type=\"both\"><left-margin>75</left-margin>\
             <right-margin>75</right-margin><top-margin>75</top-margin>\
             <bottom-margin>75</bottom-margin></page-margins></page-layout></defaults>",
            (w * 10.0).round()
        )
    });
    format!(
        "<score-partwise version=\"4.0\">{defaults}<part-list><score-part id=\"P1\">\
         <part-name>A</part-name></score-part></part-list><part id=\"P1\">{body}</part>\
         </score-partwise>"
    )
}

/// A score is set on the page its file gives, and no system's ink, a stroke's
/// half-thickness included, runs past the right margin, whatever the width.
#[test]
fn a_score_takes_its_files_page_and_keeps_within_its_margins() {
    use epiphany_cli::engrave_loaded;

    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("paged.musicxml");
    let default_page = epiphany_engrave::PageGeometry::default();
    std::fs::write(&path, paged_score(18, None)).expect("written");
    assert_eq!(
        epiphany_cli::geometry(&load(&path).expect("loads").import.source),
        default_page,
        "a file with no page layout takes the default page"
    );
    let mut overruns = Vec::new();
    let mut breaks = std::collections::BTreeSet::new();
    for k in 0..150 {
        let width = (400 + 3 * k) as f32 / 10.0;
        std::fs::write(&path, paged_score(18, Some(width))).expect("written");
        let loaded = load(&path).expect("loads");
        let geometry = epiphany_cli::geometry(&loaded.import.source);
        assert!((geometry.size.width.0 - width).abs() < 1e-4);
        assert_eq!(geometry.margins.right.0, 7.5);
        let layout = engrave_loaded(&loaded).layout;
        let right = width - 7.5;
        let mut systems = Vec::new();
        for (s, system) in layout.systems().enumerate() {
            let glyphs = system
                .primitives
                .glyphs
                .iter()
                .map(|&i| glyph_box(&layout.glyphs[i as usize])[2]);
            let strokes = system
                .primitives
                .strokes
                .iter()
                .map(|&i| &layout.strokes[i as usize])
                .map(|st| st.from.x.0.max(st.to.x.0) + st.thickness.0 / 2.0);
            let ink = glyphs.chain(strokes).fold(f32::NEG_INFINITY, f32::max);
            if ink > right + 1e-3 {
                overruns.push(format!(
                    "width {width}: system {s} ink to {ink}, margin {right}"
                ));
            }
            systems.push(system.primitives.glyphs.len());
        }
        breaks.insert(systems.len());
    }
    assert!(
        breaks.len() > 3,
        "the widths break the score differently: {breaks:?}"
    );
    assert!(overruns.is_empty(), "{overruns:#?}");
}

/// Every system's staff lines run to the right edge of the barline that
/// closes it, the last system's final barline's thick line included, and no
/// further.
#[test]
fn staff_lines_end_with_the_barline_that_closes_their_system() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("closing_barlines.musicxml");
    for (measures, at_least) in [(14, 3), (2, 1)] {
        std::fs::write(&path, paged_score(measures, Some(70.0))).expect("written");
        let loaded = load(&path).expect("loads");
        let layout = epiphany_cli::engrave_loaded(&loaded).layout;
        staff_lines_end_with_their_barlines(&layout, at_least);
    }
}

/// Every system's staff lines end at its closing barline's right edge.
fn staff_lines_end_with_their_barlines(
    layout: &epiphany_layout_ir::ResolvedLayoutIR,
    at_least: usize,
) {
    let systems: Vec<_> = layout.systems().collect();
    assert!(systems.len() >= at_least, "{} systems", systems.len());
    for (s, system) in systems.iter().enumerate() {
        let barline = system
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .filter(|g| g.glyph.as_str().starts_with("barline"))
            .map(glyph_box)
            .max_by(|a, b| a[2].total_cmp(&b[2]))
            .expect("a system closes on a barline");
        let last = s + 1 == systems.len();
        let name = system
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .find(|g| glyph_box(g) == barline)
            .map(|g| g.glyph.as_str().to_owned());
        assert_eq!(
            name.as_deref(),
            Some(if last {
                "barlineFinal"
            } else {
                "barlineSingle"
            })
        );
        let ends: Vec<f32> = system
            .primitives
            .strokes
            .iter()
            .map(|&i| &layout.strokes[i as usize])
            .filter(|st| matches!(st.provenance.source, epiphany_core::TypedObjectId::Staff(_)))
            .map(|st| st.from.x.0.max(st.to.x.0))
            .collect();
        assert_eq!(ends.len(), 5, "system {s} draws one staff");
        for end in ends {
            assert!(
                (end - barline[2]).abs() < 1e-3,
                "system {s}: a staff line ends at {end}, its barline at {}",
                barline[2]
            );
        }
    }
}

/// A note the pre-pass splits into tied parts ties them on its voice's side,
/// as a tie between two notes does: the upper voice's above and the lower's
/// below, though each stem would turn its tie toward the other voice.
#[test]
fn a_split_notes_tie_takes_its_voices_side() {
    use epiphany_core::TypedObjectId;

    let backup = "<backup><duration>8</duration></backup>";
    // An eighth, five eighths from the offbeat (which no one value is: an
    // eighth tied to two quarters), and a quarter.
    let voice = |step: &str, octave: u8, voice: u8| {
        format!(
            "{}{}{}",
            pitched(step, 0, octave, 1, voice, false),
            pitched(step, 0, octave, 5, voice, false),
            pitched(step, 0, octave, 2, voice, false)
        )
    };
    let loaded = treble_part(
        "split_ties.musicxml",
        &[format!("{}{backup}{}", voice("D", 5, 1), voice("F", 4, 2))],
    );
    let layout = engrave(&loaded.reduced.score).layout;
    // Each voice's long note is drawn as three tied parts.
    let mut ties: Vec<_> = layout
        .curves
        .iter()
        .filter(|c| matches!(c.provenance.source, TypedObjectId::Pitch(_)))
        .collect();
    assert_eq!(ties.len(), 4, "each voice's long note splits twice");
    ties.sort_by(|a, b| b.p0.y.0.total_cmp(&a.p0.y.0));
    for tie in &ties[..2] {
        assert!(tie.p1.y.0 > tie.p0.y.0, "the upper voice's ties arc above");
    }
    for tie in &ties[2..] {
        assert!(tie.p1.y.0 < tie.p0.y.0, "the lower voice's ties arc below");
    }
}

/// A tie leaving or meeting a head with other ink beside it at the tie's
/// height (a head set across the stem, another voice's head, a stem, a
/// ledger line, a dot) stands clear of it: no point of a tie's stroke lies
/// within any head, stem, ledger line, dot or accidental.
#[test]
fn a_tie_stands_clear_of_the_ink_beside_its_heads() {
    use epiphany_core::TypedObjectId;

    let (start, stop) = ("<tie type=\"start\"/>", "<tie type=\"stop\"/>");
    let note =
        |step: &str, alter: i8, octave: u8, duration: u8, voice: u8, chord: bool, tie: &str| {
            format!(
                "<note>{}<pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave>\
             </pitch><duration>{duration}</duration>{tie}<voice>{voice}</voice></note>",
                if chord { "<chord/>" } else { "" }
            )
        };
    let rest = |duration: u8, voice: u8| {
        format!("<note><rest/><duration>{duration}</duration><voice>{voice}</voice></note>")
    };
    let measures = [
        // A second tied from whole notes to halves, the upper head set
        // right of the halves' stem over the lower's ledger lines.
        [
            note("A", 0, 3, 8, 1, false, start),
            note("B", -1, 3, 8, 1, true, start),
        ]
        .concat(),
        [
            note("A", 0, 3, 4, 1, false, stop),
            note("B", -1, 3, 4, 1, true, stop),
            note("C", 0, 5, 4, 1, false, ""),
        ]
        .concat(),
        // An upper voice's half tied over beside the lower voice's quarter
        // on the same G, set to its right.
        [
            note("G", 0, 4, 4, 1, false, start),
            note("G", 0, 4, 4, 1, false, stop),
            "<backup><duration>8</duration></backup>".to_owned(),
            note("G", 0, 4, 2, 2, false, ""),
            rest(2, 2),
            rest(4, 2),
        ]
        .concat(),
        // A dotted B on the middle line, its dot in the space above, tied
        // over above.
        [
            note("B", 0, 4, 6, 1, false, start),
            note("B", 0, 4, 2, 1, false, stop),
        ]
        .concat(),
        // A second inside the staff tied to itself, the upper head set right
        // of the stem, which the tie meeting it passes.
        [
            note("F", 0, 4, 4, 1, false, start),
            note("G", 0, 4, 4, 1, true, start),
            note("F", 0, 4, 4, 1, false, stop),
            note("G", 0, 4, 4, 1, true, stop),
        ]
        .concat(),
        // An E tied into a chord whose F sharp's accidental stands at the
        // tie's height before the column.
        [
            note("E", 0, 5, 4, 1, false, start),
            note("E", 0, 5, 4, 1, false, stop),
            note("F", 1, 5, 4, 1, true, ""),
        ]
        .concat(),
    ];
    let loaded = treble_part("tie_clearance.musicxml", &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let mut ink: Vec<(String, [f32; 4])> = layout
        .glyphs
        .iter()
        .filter(|g| {
            let name = g.glyph.as_str();
            name.starts_with("notehead")
                || name.starts_with("accidental")
                || name == "augmentationDot"
        })
        .map(|g| (g.glyph.as_str().to_owned(), glyph_box(g)))
        .collect();
    for stroke in &layout.strokes {
        let stem = stroke.from.x == stroke.to.x && stroke.from.y != stroke.to.y;
        if stem && !matches!(stroke.provenance.source, TypedObjectId::Measure(_)) {
            ink.push(("stem".to_owned(), stroke_box(stroke)));
        } else if epiphany_layout_ir::is_rigid_width_stroke(stroke) {
            ink.push(("ledger line".to_owned(), stroke_box(stroke)));
        }
    }
    let ties: Vec<_> = layout
        .curves
        .iter()
        .filter(|c| {
            matches!(
                c.provenance.source,
                TypedObjectId::Tie(_) | TypedObjectId::Pitch(_)
            )
        })
        .collect();
    assert_eq!(
        ties.len(),
        7,
        "the seconds' four ties, the half's, the dotted B's, the E's"
    );
    for tie in ties {
        let [p0, p1, p2, p3] = [tie.p0, tie.p1, tie.p2, tie.p3].map(|p| (p.x.0, p.y.0));
        // The spacing gives every tie room to run a staff space.
        assert!(p3.0 - p0.0 > 1.0 - 1e-3, "a tie runs {}", p3.0 - p0.0);
        let half = tie.thickness.0 / 2.0;
        for k in 0..=100 {
            let t = k as f32 / 100.0;
            let u = 1.0 - t;
            let at = |a: f32, b: f32, c: f32, d: f32| {
                u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
            };
            let (x, y) = (at(p0.0, p1.0, p2.0, p3.0), at(p0.1, p1.1, p2.1, p3.1));
            let stroke = [x - half, y - half, x + half, y + half];
            for (name, b) in &ink {
                assert!(
                    !boxes_overlap(stroke, *b),
                    "a tie from ({:.2}, {:.2}) to ({:.2}, {:.2}) runs through a {name} at {b:?}",
                    p0.0,
                    p0.1,
                    p3.0,
                    p3.1
                );
            }
        }
    }
}

/// A rest of one voice moves off its place, away from the other voice, until
/// it stands clear of the other voice's notes that start with it; a rest with
/// none beside it keeps its place a space off the middle line.
#[test]
fn a_rest_stands_clear_of_another_voices_notes() {
    let backup = "<backup><duration>8</duration></backup>";
    let rest = |duration: u8, voice: u8| {
        format!("<note><rest/><duration>{duration}</duration><voice>{voice}</voice></note>")
    };
    let measures = [
        // The lower voice rests under the upper voice's low chord.
        format!(
            "{}{}{}{backup}{}{}",
            pitched("E", 0, 4, 2, 1, false),
            pitched("G", 0, 4, 2, 1, true),
            pitched("C", 0, 5, 6, 1, false),
            rest(2, 2),
            pitched("A", 0, 4, 6, 2, false),
        ),
        // The upper voice rests over the lower voice's high chord; the lower
        // voice rests last with no note starting beside it.
        format!(
            "{}{}{backup}{}{}{}{}",
            rest(2, 1),
            pitched("C", 0, 5, 6, 1, false),
            pitched("D", 0, 5, 2, 2, false),
            pitched("F", 0, 5, 2, 2, true),
            pitched("A", 0, 4, 4, 2, false),
            rest(2, 2),
        ),
    ];
    let loaded = treble_part("rest_clearance.musicxml", &measures);
    let layout = engrave(&loaded.reduced.score).layout;
    let staff = &layout.systems().next().expect("a system").staves[0].bounding_box;
    let middle = staff.origin.y.0 + staff.size.height.0 / 2.0;
    let heads: Vec<[f32; 4]> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .map(glyph_box)
        .collect();
    let mut rests: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
        .collect();
    rests.sort_by(|a, b| a.position.x.0.total_cmp(&b.position.x.0));
    assert_eq!(rests.len(), 3);
    for (k, rest) in rests.iter().take(2).enumerate() {
        let r = glyph_box(rest);
        // The heads of its column: those whose x range meets the rest's.
        let column: Vec<_> = heads
            .iter()
            .filter(|h| h[0] < r[2] && h[2] > r[0])
            .collect();
        assert_eq!(column.len(), 2, "rest {k} stands beside a chord");
        for h in column {
            let gap = (h[1] - r[3]).max(r[1] - h[3]);
            assert!(gap >= 0.25 - 1e-3, "rest {k} stands {gap} from a head");
        }
    }
    let (lower, upper, alone) = (rests[0], rests[1], rests[2]);
    assert!(
        lower.position.y.0 < middle - 1.5,
        "the lower voice's rest moved down"
    );
    assert!(
        upper.position.y.0 > middle + 1.5,
        "the upper voice's rest moved up"
    );
    assert!(
        (alone.position.y.0 - (middle - 1.0)).abs() < 1e-3,
        "the lower rest beside no note keeps its place, {} from the middle",
        alone.position.y.0 - middle
    );
}

/// A tuplet draws its number, the ratio's actual term, clear of its notes on
/// its stems' or voice's side: alone over a group beamed together, and in a
/// bracket hooked toward the notes when a rest is among its members.
#[test]
fn a_tuplet_draws_its_number_and_a_bracket_unless_beamed_alone() {
    use epiphany_core::TypedObjectId;

    let note = |step: &str, octave: u8, duration: u8, voice: u8, kind: &str, extra: &str| {
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>{duration}</duration><voice>{voice}</voice><type>{kind}</type>\
             {extra}</note>"
        )
    };
    let modification = |actual: u8, normal: u8| {
        format!(
            "<time-modification><actual-notes>{actual}</actual-notes>\
             <normal-notes>{normal}</normal-notes></time-modification>"
        )
    };
    let mark = |kind: &str| format!("<notations><tuplet type=\"{kind}\"/></notations>");
    let beam = |state: &str| format!("<beam number=\"1\">{state}</beam>");
    let triplet = modification(3, 2);
    let sextuplet = modification(6, 4);
    // Divisions 12: a triplet eighth is 4, a sextuplet sixteenth 2.
    let measure1 = [
        // A beamed triplet of low eighths, stems up.
        note(
            "E",
            4,
            4,
            1,
            "eighth",
            &format!("{}{triplet}{}", beam("begin"), mark("start")),
        ),
        note(
            "F",
            4,
            4,
            1,
            "eighth",
            &format!("{}{triplet}", beam("continue")),
        ),
        note(
            "G",
            4,
            4,
            1,
            "eighth",
            &format!("{}{triplet}{}", beam("end"), mark("stop")),
        ),
        // A triplet with a rest among its members.
        format!(
            "<note><rest/><duration>4</duration><voice>1</voice><type>eighth</type>\
             {triplet}{}</note>",
            mark("start")
        ),
        note(
            "A",
            4,
            4,
            1,
            "eighth",
            &format!("{}{triplet}", beam("begin")),
        ),
        note(
            "B",
            4,
            4,
            1,
            "eighth",
            &format!("{}{triplet}{}", beam("end"), mark("stop")),
        ),
    ]
    .concat();
    let measure2 = [
        // A beamed sextuplet of sixteenths, then a half.
        note(
            "C",
            5,
            2,
            1,
            "16th",
            &format!("{}{sextuplet}{}", beam("begin"), mark("start")),
        ),
        note(
            "D",
            5,
            2,
            1,
            "16th",
            &format!("{}{sextuplet}", beam("continue")),
        ),
        note(
            "E",
            5,
            2,
            1,
            "16th",
            &format!("{}{sextuplet}", beam("continue")),
        ),
        note(
            "F",
            5,
            2,
            1,
            "16th",
            &format!("{}{sextuplet}", beam("continue")),
        ),
        note(
            "G",
            5,
            2,
            1,
            "16th",
            &format!("{}{sextuplet}", beam("continue")),
        ),
        note(
            "A",
            5,
            2,
            1,
            "16th",
            &format!("{}{sextuplet}{}", beam("end"), mark("stop")),
        ),
        note("C", 5, 12, 1, "quarter", ""),
        // The lower voice: a quarter, then a beamed triplet below.
        "<backup><duration>24</duration></backup>".to_owned(),
        note("G", 4, 12, 2, "quarter", ""),
        note(
            "F",
            4,
            4,
            2,
            "eighth",
            &format!("{}{triplet}{}", beam("begin"), mark("start")),
        ),
        note(
            "E",
            4,
            4,
            2,
            "eighth",
            &format!("{}{triplet}", beam("continue")),
        ),
        note(
            "D",
            4,
            4,
            2,
            "eighth",
            &format!("{}{triplet}{}", beam("end"), mark("stop")),
        ),
    ]
    .concat();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>12</divisions><time><beats>2</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{measure1}</measure>\
         <measure number=\"2\">{measure2}</measure></part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tuplets.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert_eq!(loaded.reduced.score.cross_cutting.tuplets.len(), 4);
    let layout = engrave(&loaded.reduced.score).layout;

    // The numbers, left to right: the triplets' 3s and the sextuplet's 6.
    let mut numbers: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("tuplet"))
        .collect();
    numbers.sort_by(|a, b| a.position.x.0.total_cmp(&b.position.x.0));
    assert_eq!(
        numbers.iter().map(|g| g.glyph.as_str()).collect::<Vec<_>>(),
        ["tuplet3", "tuplet3", "tuplet6", "tuplet3"]
    );
    // Only the triplet with a rest is bracketed: two lines and two hooks.
    let brackets: Vec<_> = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, TypedObjectId::Tuplet(_))
                && (s.from.x.0 - s.to.x.0).abs() + (s.from.y.0 - s.to.y.0).abs() > 1e-3
        })
        .collect();
    assert_eq!(brackets.len(), 4, "one bracket of four strokes");
    let rest = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "rest8th")
        .expect("the bracketed triplet's rest");
    let bracketed = numbers[1];
    for stroke in &brackets {
        for end in [&stroke.from, &stroke.to] {
            assert!(
                end.x.0 >= glyph_box(rest)[0] - 1e-3,
                "the bracket starts at its first member"
            );
        }
    }
    let (bracket_line_y, hook_low) =
        brackets
            .iter()
            .fold((f32::NEG_INFINITY, f32::INFINITY), |(hi, lo), s| {
                (
                    hi.max(s.from.y.0.max(s.to.y.0)),
                    lo.min(s.from.y.0.min(s.to.y.0)),
                )
            });
    let b = glyph_box(bracketed);
    assert!(
        b[1] < bracket_line_y && b[3] > bracket_line_y,
        "the bracketed number sits on its line"
    );
    assert!(
        hook_low < bracket_line_y,
        "the hooks point down to the notes"
    );

    // Each number stands at least the clearance off every head and stem
    // under it: above for the first three, below for the lower voice's.
    let heads_and_stems: Vec<[f32; 4]> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .map(glyph_box)
        .chain(
            layout
                .strokes
                .iter()
                .filter(|s| s.from.x == s.to.x && s.from.y != s.to.y)
                .map(stroke_box),
        )
        .chain(
            layout
                .strokes
                .iter()
                .filter(|s| epiphany_layout_ir::is_beam_stroke(s))
                .map(stroke_box),
        )
        .collect();
    for (k, number) in numbers.iter().enumerate() {
        let n = glyph_box(number);
        let under: Vec<&[f32; 4]> = heads_and_stems
            .iter()
            .filter(|ink| ink[0] < n[2] && ink[2] > n[0])
            .collect();
        assert!(!under.is_empty(), "number {k} stands over ink");
        if k < 3 {
            let top = under.iter().map(|i| i[3]).fold(f32::NEG_INFINITY, f32::max);
            assert!(
                n[1] >= top + 0.5 - 0.05,
                "number {k} stands clear above, {} over {top}",
                n[1]
            );
        } else {
            let bottom = under.iter().map(|i| i[1]).fold(f32::INFINITY, f32::min);
            assert!(
                n[3] <= bottom - 0.5 + 0.05,
                "the lower voice's number stands clear below"
            );
        }
    }
}

/// The decomposition takes each measure's own meter: after a change from 4/4
/// to 3/4 a dotted half fills each 3/4 bar as one value, where a single
/// meter for the region would put a phantom barline inside the third bar and
/// tie its dotted half across it; and the new meter's signature is drawn.
#[test]
fn notes_take_their_values_from_every_meter() {
    use epiphany_core::TypedObjectId;

    let note = |step: &str, duration: u8, kind: &str, dot: bool| {
        format!(
            "<note><pitch><step>{step}</step><octave>4</octave></pitch>\
             <duration>{duration}</duration><voice>1</voice><type>{kind}</type>{}</note>",
            if dot { "<dot/>" } else { "" }
        )
    };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>2</divisions><time><beats>4</beats>\
         <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
         </attributes>{}</measure>\
         <measure number=\"2\"><attributes><time><beats>3</beats><beat-type>4</beat-type>\
         </time></attributes>{}</measure>\
         <measure number=\"3\">{}</measure>\
         <measure number=\"4\">{}</measure></part></score-partwise>",
        note("C", 8, "whole", false),
        note("D", 6, "half", true),
        note("E", 6, "half", true),
        note("F", 6, "half", true),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("meters.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    let heads = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .count();
    assert_eq!(heads, 4, "one head a measure");
    let dots = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "augmentationDot")
        .count();
    assert_eq!(dots, 3, "each dotted half keeps its dot");
    assert!(
        !layout
            .curves
            .iter()
            .any(|c| matches!(c.provenance.source, TypedObjectId::Pitch(_))),
        "no note is split into tied parts"
    );
    // The 3/4 signature drawn where it begins.
    assert!(layout.glyphs.iter().any(|g| g.glyph.as_str() == "timeSig3"));
}

/// A clef change is drawn smaller than a leading clef, where it takes effect:
/// mid-measure just before its note, which then reads in the new clef; at a
/// measure's start before that barline, so a change opening a system ends the
/// system before as a courtesy while the new system's lead shows it. A clef
/// restated draws nothing, and an octave clef's numeral stands over its clef.
#[test]
fn clef_changes_are_drawn_where_they_take_effect() {
    use epiphany_cli::omissions::omissions;
    use epiphany_core::TypedObjectId;

    let clef = |sign: &str, line: u8, change: i8| {
        format!(
            "<attributes><clef><sign>{sign}</sign><line>{line}</line>\
             <clef-octave-change>{change}</clef-octave-change></clef></attributes>"
        )
    };
    let note = |step: &str, octave: u8| {
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>1</duration><voice>1</voice><type>quarter</type></note>"
        )
    };
    let b4 = note("B", 4);
    let d3 = note("D", 3);
    let mut measures = vec![
        format!(
            "<attributes><divisions>1</divisions><time><beats>4</beats>\
             <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
             </attributes>{b4}{b4}{b4}{b4}"
        ),
        // Mid-measure: D3 below the treble staff, then on the bass staff's
        // middle line.
        format!("{b4}{d3}{}{d3}{d3}", clef("F", 4, 0)),
        format!("{}{b4}{b4}{b4}{b4}", clef("G", 2, 0)),
        // Restated: nothing to draw.
        format!("{}{b4}{b4}{b4}{b4}", clef("G", 2, 0)),
        format!("{b4}{}{b4}{b4}{b4}", clef("G", 2, 1)),
    ];
    // A change at every measure's start from here, so some open systems.
    for m in 0..55 {
        let (sign, line) = if m % 2 == 0 { ("F", 4) } else { ("G", 2) };
        measures.push(format!("{}{d3}{d3}{d3}{d3}", clef(sign, line, 0)));
    }
    let body: String = measures
        .iter()
        .enumerate()
        .map(|(m, content)| format!("<measure number=\"{}\">{content}</measure>", m + 1))
        .collect();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("clef_changes.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let engraved = engrave(&loaded.reduced.score);
    let layout = &engraved.layout;
    let found = omissions(&loaded.reduced.score, layout, &engraved.diagnostics);
    assert_eq!(found.kinds.get("clef change"), None, "{found:?}");
    assert_eq!(
        found.kinds.get("clef of another shape at a system start"),
        None
    );

    let changes: Vec<_> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().ends_with("ClefChange"))
        .collect();
    assert_eq!(changes.len(), 3 + 55, "every change but the restatement");

    let systems: Vec<_> = layout.systems().collect();
    assert!(systems.len() > 2, "the score wraps");
    let in_system = |s: usize, keep: &dyn Fn(&epiphany_layout_ir::ResolvedGlyph) -> bool| {
        let mut glyphs: Vec<&epiphany_layout_ir::ResolvedGlyph> = systems[s]
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .filter(|g| keep(g))
            .collect();
        glyphs.sort_by(|a, b| a.position.x.0.total_cmp(&b.position.x.0));
        glyphs
    };
    let head = |g: &epiphany_layout_ir::ResolvedGlyph| g.glyph.as_str().starts_with("notehead");
    let barline = |g: &epiphany_layout_ir::ResolvedGlyph| g.glyph.as_str() == "barlineSingle";
    let left = |g: &epiphany_layout_ir::ResolvedGlyph| g.position.x.0 + g.bounding_box.left.0;
    let right = |g: &epiphany_layout_ir::ResolvedGlyph| g.position.x.0 + g.bounding_box.right.0;

    // The first system: measures 1 to 5, in order.
    let heads = in_system(0, &head);
    let bars = in_system(0, &barline);
    let first_changes = in_system(0, &|g| g.glyph.as_str().ends_with("ClefChange"));
    // Mid-measure, between the second and third notes of measure 2, which
    // then reads in the bass clef: six staff spaces higher.
    let bass = first_changes[0];
    assert_eq!(bass.glyph.as_str(), "fClefChange");
    let (before, after) = (heads[5], heads[6]);
    assert!(right(before) <= left(bass) && right(bass) + 0.5 <= left(after) + 1e-3);
    assert!(
        (after.position.y.0 - before.position.y.0 - 6.0).abs() < 1e-3,
        "{} then {}",
        before.position.y.0,
        after.position.y.0
    );
    // At measure 3's start, before the barline closing measure 2 and after
    // its last note.
    let treble = first_changes[1];
    assert_eq!(treble.glyph.as_str(), "gClefChange");
    assert!(right(heads[7]) <= left(treble));
    assert!(right(treble) + 0.5 <= left(bars[1]) + 1e-3 && left(bars[1]) <= left(heads[8]));
    // The octave clef in measure 5: its numeral centred over it.
    let octave = first_changes[2];
    let numeral = in_system(0, &|g| g.glyph.as_str() == "clef8")
        .into_iter()
        .next()
        .expect("an octave clef draws its numeral");
    let centre = |g: &epiphany_layout_ir::ResolvedGlyph| (left(g) + right(g)) / 2.0;
    assert!((centre(numeral) - centre(octave)).abs() < 1e-3);
    assert!(
        numeral.position.y.0 + numeral.bounding_box.bottom.0
            >= octave.position.y.0 + octave.bounding_box.top.0 - 0.1 - 1e-3
    );

    // Before the engraver re-spaces, the constrained layout's own geometry
    // keeps every change clear of what follows it.
    let constrained =
        epiphany_layout_ir::to_constrained(&epiphany_layout_ir::to_logical(&loaded.reduced.score));
    let source_box = |g: &epiphany_layout_ir::GlyphObject| {
        [
            g.baseline.x.0 + g.bounding_box.left.0,
            g.baseline.y.0 + g.bounding_box.bottom.0,
            g.baseline.x.0 + g.bounding_box.right.0,
            g.baseline.y.0 + g.bounding_box.top.0,
        ]
    };
    let source_changes: Vec<_> = constrained
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().ends_with("ClefChange"))
        .collect();
    assert_eq!(source_changes.len(), changes.len());
    for change in &source_changes {
        for other in &constrained.glyphs {
            if std::ptr::eq(*change, other) || other.glyph.as_str().starts_with("clef") {
                continue;
            }
            assert!(
                !boxes_overlap(source_box(change), source_box(other)),
                "{} overlaps {} before spacing",
                change.glyph.as_str(),
                other.glyph.as_str()
            );
        }
    }

    // Every later system opens on a measure whose change ends the system
    // before, after its last note and before its closing barline, and its
    // lead shows the clef that change makes.
    for s in 1..systems.len() {
        let before = in_system(s - 1, &|g| {
            g.glyph.as_str().contains("Clef") || head(g) || barline(g)
        });
        let n = before.len();
        let (courtesy, closing) = (before[n - 2], before[n - 1]);
        assert!(
            courtesy.glyph.as_str().ends_with("ClefChange") && barline(closing),
            "system {s}: {} then {}",
            courtesy.glyph.as_str(),
            closing.glyph.as_str()
        );
        let lead = in_system(s, &|g| {
            matches!(g.provenance.source, TypedObjectId::StaffInstance(_))
                && g.glyph.as_str().contains("Clef")
        })[0];
        assert_eq!(
            lead.glyph.as_str().replace("Change", ""),
            courtesy.glyph.as_str().replace("Change", ""),
            "system {s}"
        );
    }
}

/// A tuplet opening on a hidden rest still engraves: its bracket rides the
/// columns its members draw in, since a column nothing draws in has no slot
/// to anchor it.
#[test]
fn a_tuplet_opening_on_a_hidden_rest_still_engraves() {
    let triplet = |inner: &str, edge: &str| {
        format!(
            "<note{inner}><duration>2</duration><voice>1</voice><type>eighth</type>\
             <time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes>\
             </time-modification>{edge}</note>"
        )
    };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>6</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{}{}{}{}{}{}</measure>\
         </part></score-partwise>",
        triplet(
            " print-object=\"no\"><rest/",
            "<notations><tuplet type=\"start\" bracket=\"yes\"/></notations>"
        ),
        triplet("><pitch><step>C</step><octave>5</octave></pitch", ""),
        triplet(
            "><pitch><step>D</step><octave>5</octave></pitch",
            "<notations><tuplet type=\"stop\"/></notations>"
        ),
        quarter("E"),
        quarter("F"),
        quarter("G"),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hidden_rest_tuplet.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert_eq!(loaded.reduced.score.cross_cutting.tuplets.len(), 1);
    let layout = engrave(&loaded.reduced.score).layout;
    assert_eq!(layout.pages.len(), 1, "the score engraves");
    assert!(layout.glyphs.iter().any(|g| g.glyph.as_str() == "tuplet3"));
    let bracket = layout
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, epiphany_core::TypedObjectId::Tuplet(_)))
        .count();
    assert!(bracket >= 2, "the bracket is drawn: {bracket} strokes");
}

fn quarter(step: &str) -> String {
    format!(
        "<note><pitch><step>{step}</step><octave>5</octave></pitch><duration>6</duration>\
         <voice>1</voice><type>quarter</type></note>"
    )
}

/// Every stem stands on a head of its own note, and every head that takes a
/// stem has its note's stem, after spacing and justification: told apart by
/// provenance, not by nearness. The glyph nearest a stem can belong to
/// another column: a beamed sextuplet's number stands between its third and
/// fourth notes, in the fourth's column, just left of the third's up-stem;
/// and a whole-note chord's displaced head on the lower staff stands just
/// right of the next column's head on the upper staff.
#[test]
fn a_stem_stands_on_its_own_heads() {
    use std::collections::BTreeMap;

    use epiphany_core::{Event, TypedObjectId};

    let note = |step: &str, alter: i8, octave: u8, duration: u8, kind: &str, extra: &str| {
        format!(
            "<note><pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave>\
             </pitch><duration>{duration}</duration><voice>1</voice><type>{kind}</type>\
             {extra}<staff>1</staff></note>"
        )
    };
    let beams = |state: &str, levels: u8| -> String {
        (1..=levels)
            .map(|n| format!("<beam number=\"{n}\">{state}</beam>"))
            .collect()
    };
    let tuplet = |actual: u8, normal: u8, mark: &str| {
        let modification = format!(
            "<time-modification><actual-notes>{actual}</actual-notes>\
             <normal-notes>{normal}</normal-notes></time-modification>"
        );
        match mark {
            "" => modification,
            _ => format!("{modification}<notations><tuplet type=\"{mark}\"/></notations>"),
        }
    };
    let rest = "<note><rest/><duration>12</duration><voice>1</voice><type>quarter</type>\
                <staff>1</staff></note>";
    // Divisions 12: a sextuplet sixteenth is 2, a quarter 12.
    let sextuplets: String = (0..2)
        .map(|_| {
            ["D", "E", "F", "G", "A", "G"]
                .iter()
                .enumerate()
                .map(|(i, step)| {
                    let (state, mark) = match i {
                        0 => ("begin", "start"),
                        5 => ("end", "stop"),
                        _ => ("continue", ""),
                    };
                    let extra = format!("{}{}", beams(state, 2), tuplet(6, 4, mark));
                    note(step, 0, 4, 2, "16th", &extra)
                })
                .collect::<String>()
        })
        .collect::<String>()
        + &note("G", 0, 4, 12, "quarter", "")
        + &note("B", 0, 4, 12, "quarter", "");
    let after_rests = [
        rest.to_string(),
        note("A", 0, 4, 12, "quarter", ""),
        rest.to_string(),
        note("F", 1, 4, 12, "quarter", ""),
    ]
    .concat();
    let whole_second = "<note><pitch><step>C</step><octave>3</octave></pitch>\
         <duration>48</duration><voice>5</voice><type>whole</type><staff>2</staff></note>\
         <note><chord/><pitch><step>D</step><octave>3</octave></pitch><duration>48</duration>\
         <voice>5</voice><type>whole</type><staff>2</staff></note>";
    let measure_rest = "<note><rest measure=\"yes\"/><duration>48</duration><voice>5</voice>\
         <staff>2</staff></note>";
    let body: String = (1..=16)
        .map(|m| {
            let attributes = if m == 1 {
                "<attributes><divisions>12</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><staves>2</staves><clef number=\"1\">\
                 <sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign>\
                 <line>4</line></clef></attributes>"
            } else {
                ""
            };
            let (upper, lower) = if m % 2 == 0 {
                (&after_rests, whole_second)
            } else {
                (&sextuplets, measure_rest)
            };
            format!(
                "<measure number=\"{m}\">{attributes}{upper}<backup><duration>48</duration>\
                 </backup>{lower}</measure>"
            )
        })
        .collect();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stems_on_heads.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let score = &loaded.reduced.score;
    let layout = epiphany_cli::engrave_loaded(&loaded).layout;
    assert!(
        layout.systems().count() > 1,
        "the score wraps, so its systems are justified"
    );

    let mut event_of = BTreeMap::new();
    for event in score.events.iter() {
        if let Event::Pitched(pitched) = event {
            for pitch in &pitched.pitches {
                event_of.insert(pitch.id, pitched.id);
            }
        }
    }
    // Each note's heads, (left, right, y, takes a stem), and its stems,
    // (x, low, high).
    let mut heads: BTreeMap<_, Vec<(f32, f32, f32, bool)>> = BTreeMap::new();
    for glyph in &layout.glyphs {
        let name = glyph.glyph.as_str();
        let (true, TypedObjectId::Pitch(pitch)) =
            (name.starts_with("notehead"), glyph.provenance.source)
        else {
            continue;
        };
        let [left, _, right, _] = glyph_box(glyph);
        heads.entry(event_of[&pitch]).or_default().push((
            left,
            right,
            glyph.position.y.0,
            name != "noteheadWhole",
        ));
    }
    let mut stems: BTreeMap<_, Vec<(f32, f32, f32)>> = BTreeMap::new();
    for stroke in &layout.strokes {
        let TypedObjectId::Event(event) = stroke.provenance.source else {
            continue;
        };
        if stroke.from.x.0 != stroke.to.x.0 || stroke.from.y.0 == stroke.to.y.0 {
            continue;
        }
        stems.entry(event).or_default().push((
            stroke.from.x.0,
            stroke.from.y.0.min(stroke.to.y.0),
            stroke.from.y.0.max(stroke.to.y.0),
        ));
    }
    // A stem touches a head it stands at the side of, reaching its height.
    let touches = |(x, low, high): (f32, f32, f32), (left, right, y, _): (f32, f32, f32, bool)| {
        left - 0.08 <= x && x <= right + 0.08 && low - 0.1 <= y && y <= high + 0.1
    };
    let mut apart = Vec::new();
    for (event, own) in &heads {
        let theirs = stems.get(event).map(Vec::as_slice).unwrap_or(&[]);
        for &stem in theirs {
            if !own.iter().any(|&head| touches(stem, head)) {
                apart.push(format!("{event:?}: a stem at {stem:?} on none of {own:?}"));
            }
        }
        for &head in own.iter().filter(|head| head.3) {
            if !theirs.iter().any(|&stem| touches(stem, head)) {
                apart.push(format!("{event:?}: a head at {head:?} has no stem"));
            }
        }
    }
    let stemmed: usize = stems.values().map(Vec::len).sum();
    assert_eq!(stemmed, 128, "every quarter and sixteenth has its stem");
    assert!(apart.is_empty(), "{apart:#?}");
}

/// A rest filling a pickup keeps the value the file writes, as a rest filling
/// a full bar is a measure rest: a pickup is not a measure's worth of
/// silence. The omission census reads both alike.
#[test]
fn a_rest_filling_a_pickup_keeps_its_value() {
    let xml = "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"0\" implicit=\"yes\"><attributes><divisions>1</divisions>\
         <time><beats>3</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line>\
         </clef></attributes><note><rest/><duration>1</duration><voice>1</voice>\
         <type>quarter</type></note></measure>\
         <measure number=\"1\"><note><rest measure=\"yes\"/><duration>3</duration>\
         <voice>1</voice></note></measure>\
         <measure number=\"2\"><note><rest/><duration>3</duration><voice>1</voice>\
         <type>half</type><dot/></note></measure></part></score-partwise>";
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pickup_rest.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let engraved = engrave(&loaded.reduced.score);
    let rests: Vec<&str> = engraved
        .layout
        .glyphs
        .iter()
        .map(|g| g.glyph.as_str())
        .filter(|g| g.starts_with("rest"))
        .collect();
    assert_eq!(rests, vec!["restQuarter", "restWhole", "restWhole"]);
    let found = omissions(
        &loaded.reduced.score,
        &engraved.layout,
        &engraved.diagnostics,
    );
    assert_eq!(
        found.kinds.get("rest drawn at another value"),
        None,
        "{found:?}"
    );
}

/// A hand-written score through the whole pipeline, import to page, locked
/// to a golden. Its features are counted first, so the golden cannot lock a
/// page that lost one: a pickup whose rests keep their values, a key
/// signature, a meter change, beams, dots, accidentals and a second,
/// triplets beamed and bracketed, two voices on a staff, a tie, a slur, and
/// a grand staff that changes clef mid-measure and back. Regenerate
/// deliberately, with renders beside it, with `UPDATE_GOLDEN=1`.
#[test]
fn a_hand_written_score_engraves_to_its_golden() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("notation.svg");
    let _ = std::fs::remove_file(&out);
    let status = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("render")
        .arg(fixture("notation.musicxml"))
        .args(["--page", "1", "-o"])
        .arg(&out)
        .output()
        .expect("runs");
    assert!(status.status.success(), "{status:?}");
    let svg = std::fs::read_to_string(&out).expect("an SVG was written");

    let count = |needle: &str| svg.matches(needle).count();
    for (glyph, expected) in [
        ("restQuarter", 6),
        ("restHalf", 1),
        ("restWhole", 1),
        ("augmentationDot", 5),
        ("accidentalSharp", 3),
        ("accidentalNatural", 1),
        ("accidentalFlat", 6),
        ("timeSig3", 3),
        ("tuplet3", 2),
        ("gClefChange", 1),
        ("fClefChange", 1),
        ("brace", 1),
    ] {
        assert_eq!(
            count(&format!("data-glyph=\"{glyph}\"")),
            expected,
            "{glyph}"
        );
    }
    assert_eq!(count("data-kind=\"curve\""), 2, "a tie and a slur");
    assert!(count("stroke-width=\"0.5\"") >= 2, "the two beams");
    // The omission census agrees: nothing the file holds is drawn otherwise.
    let loaded = load(&fixture("notation.musicxml")).expect("loads");
    let engraved = epiphany_cli::engrave_loaded(&loaded);
    let found = omissions(
        &loaded.reduced.score,
        &engraved.layout,
        &engraved.diagnostics,
    );
    assert!(found.kinds.is_empty(), "{found:?}");

    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/notation.page-1.svg");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden.parent().expect("a directory")).expect("created");
        std::fs::write(&golden, &svg).expect("golden written");
    }
    let expected = std::fs::read_to_string(&golden)
        .unwrap_or_else(|e| panic!("{}: {e}; regenerate with UPDATE_GOLDEN=1", golden.display()));
    assert!(
        expected == svg,
        "the page differs from {}; if intended, regenerate with UPDATE_GOLDEN=1",
        golden.display()
    );
}

/// A time signature of several digits sets them side by side at their own
/// widths, no digit's ink meeting the next, and centres its shorter line
/// under its longer: 12 over 8, then 4 over 16.
#[test]
fn a_time_signatures_digits_stand_apart_and_centred() {
    let attributes = |beats: u8, beat_type: u8, first: bool| {
        format!(
            "<attributes>{}<time><beats>{beats}</beats><beat-type>{beat_type}</beat-type>\
             </time>{}</attributes>",
            if first {
                "<divisions>4</divisions>"
            } else {
                ""
            },
            if first {
                "<clef><sign>G</sign><line>2</line></clef>"
            } else {
                ""
            },
        )
    };
    // Divisions 4: the 12/8 bar is a dotted whole, the 4/16 bar a quarter.
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Flute</part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\">{}<note><pitch><step>C</step><octave>5</octave></pitch>\
         <duration>24</duration><voice>1</voice><type>whole</type><dot/></note></measure>\
         <measure number=\"2\">{}<note><pitch><step>C</step><octave>5</octave></pitch>\
         <duration>4</duration><voice>1</voice><type>quarter</type></note></measure>\
         </part></score-partwise>",
        attributes(12, 8, true),
        attributes(4, 16, false),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long_meters.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = epiphany_cli::engrave_loaded(&loaded).layout;
    // Each signature's digits (a name and its ink), by the measure they are
    // drawn for.
    type Digits<'a> = Vec<(&'a str, [f32; 4])>;
    let mut signatures: Vec<(epiphany_core::TypedObjectId, Digits)> = Vec::new();
    for glyph in &layout.glyphs {
        if !glyph.glyph.as_str().starts_with("timeSig") {
            continue;
        }
        let digit = (glyph.glyph.as_str(), glyph_box(glyph));
        match signatures
            .iter_mut()
            .find(|(source, _)| *source == glyph.provenance.source)
        {
            Some((_, digits)) => digits.push(digit),
            None => signatures.push((glyph.provenance.source, vec![digit])),
        }
    }
    signatures.sort_by(|a, b| a.1[0].1[0].total_cmp(&b.1[0].1[0]));
    let read: Vec<(Vec<&str>, Vec<&str>)> = signatures
        .iter()
        .map(|(_, digits)| {
            let top = digits
                .iter()
                .map(|(_, ink)| ink[3])
                .fold(f32::MIN, f32::max);
            let line = |upper: bool| {
                let mut line: Vec<&(&str, [f32; 4])> = digits
                    .iter()
                    .filter(|(_, ink)| (ink[3] > top - 1.0) == upper)
                    .collect();
                line.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]));
                for pair in line.windows(2) {
                    assert!(
                        pair[0].1[2] <= pair[1].1[0],
                        "{} ends at {} but {} starts at {}",
                        pair[0].0,
                        pair[0].1[2],
                        pair[1].0,
                        pair[1].1[0]
                    );
                }
                line
            };
            let (upper, lower) = (line(true), line(false));
            let centre =
                |line: &[&(&str, [f32; 4])]| (line[0].1[0] + line[line.len() - 1].1[2]) / 2.0;
            assert!(
                (centre(&upper) - centre(&lower)).abs() < 0.2,
                "the lines are centred on one axis: {} and {}",
                centre(&upper),
                centre(&lower)
            );
            (
                upper.iter().map(|(n, _)| *n).collect(),
                lower.iter().map(|(n, _)| *n).collect(),
            )
        })
        .collect();
    assert_eq!(
        read,
        [
            (vec!["timeSig1", "timeSig2"], vec!["timeSig8"]),
            (vec!["timeSig4"], vec!["timeSig1", "timeSig6"]),
        ]
    );
}

/// A quarter-tone's notehead stands at the step of its letter and octave, as
/// its authored spelling gives it, and draws no missing-spelling fallback:
/// each of the hand-written fixture's quarter-tones, measured from its
/// staff's bottom line on the page.
#[test]
fn a_quarter_tone_stands_at_its_spelled_step() {
    use epiphany_core::{Event, PitchSpacePosition, TypedObjectId};
    use epiphany_layout_ir::constrained::LayoutDiagnosticKind;

    let loaded = load(&fixture("arrow_accidentals.musicxml")).expect("loads");
    let score = &loaded.reduced.score;
    let engraved = epiphany_cli::engrave_loaded(&loaded);
    let layout = &engraved.layout;
    assert!(
        !engraved
            .diagnostics
            .iter()
            .any(|d| d.kind == LayoutDiagnosticKind::MissingSpelling),
        "a pitch drew at its clef's reference line"
    );
    let treble = epiphany_core::Clef {
        shape: epiphany_core::ClefShape::G,
        line: 2,
        octave_shift: 0,
    };
    let staff = score.staves[0].id;
    let mut checked = 0;
    for system in layout.systems() {
        let bottom = system
            .primitives
            .strokes
            .iter()
            .map(|&i| &layout.strokes[i as usize])
            .filter(|s| s.provenance.source == TypedObjectId::Staff(staff))
            .map(|s| s.from.y.0)
            .fold(f32::INFINITY, f32::min);
        for glyph in system
            .primitives
            .glyphs
            .iter()
            .map(|&i| &layout.glyphs[i as usize])
            .filter(|g| g.glyph.as_str().starts_with("notehead"))
        {
            let TypedObjectId::Pitch(id) = glyph.provenance.source else {
                continue;
            };
            let pitch = score
                .events
                .iter()
                .find_map(|e| match e {
                    Event::Pitched(p) => p.pitches.iter().find(|ip| ip.id == id),
                    _ => None,
                })
                .expect("the head's pitch is in the score");
            if pitch.pitch.scale_position.space.as_str() != "cmn-24" {
                continue;
            }
            let PitchSpacePosition::Cmn {
                nominal, octave, ..
            } = pitch.pitch.scale_position.position
            else {
                panic!("a CMN pitch")
            };
            let step = epiphany_layout_ir::staff_position(nominal, octave, &treble);
            let expected = bottom + step as f32 * 0.5;
            assert!(
                (glyph.position.y.0 - expected).abs() < 1e-3,
                "{nominal:?}{octave} stands at {}, its step {step} at {expected}",
                glyph.position.y.0
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 24, "every quarter-tone of the fixture is checked");
}

/// Each quarter-tone accidental draws its SMuFL glyph beside its own head:
/// the hand-written fixture's first fourteen quarter-tones hold each of the
/// fourteen names once, ten arrowed and Stein's four, and the page draws
/// each one's glyph traced to its pitch, with no glyph left unbundled.
#[test]
fn each_quarter_tone_accidental_draws_its_smufl_glyph() {
    use epiphany_core::{Event, EventPosition, TypedObjectId};
    use epiphany_layout_ir::constrained::LayoutDiagnosticKind;

    let loaded = load(&fixture("arrow_accidentals.musicxml")).expect("loads");
    let score = &loaded.reduced.score;
    let engraved = epiphany_cli::engrave_loaded(&loaded);
    assert!(
        !engraved
            .diagnostics
            .iter()
            .any(|d| matches!(d.kind, LayoutDiagnosticKind::UnbundledGlyph(_))),
        "{:?}",
        engraved.diagnostics
    );
    // The first staff's quarter-tones in time order, the first fourteen.
    let mut quarter_tones: Vec<(epiphany_core::RationalTime, epiphany_core::PitchId)> = score
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Pitched(p) => match &p.position {
                EventPosition::Musical(at) => Some((at.0.clone(), p)),
                _ => None,
            },
            _ => None,
        })
        .flat_map(|(at, p)| {
            p.pitches
                .iter()
                .filter(|ip| ip.pitch.scale_position.space.as_str() == "cmn-24")
                .map(move |ip| (at.clone(), ip.id))
        })
        .collect();
    quarter_tones.sort();
    let drawn: Vec<Vec<&str>> = quarter_tones[..14]
        .iter()
        .map(|(_, id)| {
            engraved
                .layout
                .glyphs
                .iter()
                .filter(|g| g.provenance.source == TypedObjectId::Pitch(*id))
                .map(|g| g.glyph.as_str())
                .filter(|name| name.starts_with("accidental"))
                .collect()
        })
        .collect();
    let expected = [
        "accidentalQuarterToneFlatArrowUp",
        "accidentalThreeQuarterTonesFlatArrowDown",
        "accidentalQuarterToneSharpNaturalArrowUp",
        "accidentalQuarterToneFlatNaturalArrowDown",
        "accidentalThreeQuarterTonesSharpArrowUp",
        "accidentalQuarterToneSharpArrowDown",
        "accidentalFiveQuarterTonesSharpArrowUp",
        "accidentalThreeQuarterTonesSharpArrowDown",
        "accidentalThreeQuarterTonesFlatArrowUp",
        "accidentalFiveQuarterTonesFlatArrowDown",
        "accidentalQuarterToneFlatStein",
        "accidentalQuarterToneSharpStein",
        "accidentalThreeQuarterTonesFlatZimmermann",
        "accidentalThreeQuarterTonesSharpStein",
    ];
    assert_eq!(drawn, expected.iter().map(|g| vec![*g]).collect::<Vec<_>>());
}

/// Quarter-tone accidentals join the measure's accidental state: one holds to
/// the barline on its letter and octave, across voices; a natural, a flat or
/// another quarter-tone after it on its letter is shown; the same alteration
/// stated again, by either notation, is not; a new measure states it again;
/// and against a key a quarter-tone is shown, and the key's own flat after it
/// is restated. A note after it that writes no accidental is natural, since a
/// quarter-tone accidental applies to its own note alone, and shows the
/// natural that cancels it, in its own voice or another.
#[test]
fn a_quarter_tone_accidental_holds_to_the_barline_and_yields_to_a_change() {
    use epiphany_core::{Event, EventPosition, TypedObjectId};

    // In 2/4, one flat in the key, quarters: (step, octave, accidental,
    // voice) in time order. A whole-semitone alteration is the `<alter>` a
    // file writes with it (the key's B-flat included); a quarter-tone is
    // written by name alone, as MuseScore writes its arrows; and a note that
    // writes neither (`plain`) is natural.
    let note = |step: &str, octave: u8, accidental: &str, voice: u8| {
        let alter = match (step, accidental) {
            (_, "flat") | ("B", "") => "<alter>-1</alter>",
            _ => "",
        };
        let accidental = match accidental {
            "" | "plain" => String::new(),
            name => format!("<accidental>{name}</accidental>"),
        };
        format!(
            "<note><pitch><step>{step}</step>{alter}<octave>{octave}</octave></pitch>\
             <duration>1</duration><voice>{voice}</voice><type>quarter</type>{accidental}</note>"
        )
    };
    let measures = [
        // A quarter-flat B holds: stated again, in its voice or another, it
        // is not shown again.
        [
            note("B", 4, "quarter-flat", 1),
            note("B", 4, "quarter-flat", 1),
        ]
        .concat()
            + "<backup><duration>2</duration></backup>"
            + &[note("D", 4, "", 2), note("B", 4, "quarter-flat", 2)].concat(),
        // A plain B after the first voice's B quarter-sharp is natural, and
        // shows its natural in the second voice; so does a plain E after an E
        // quarter-flat in its own.
        [note("B", 4, "natural-up", 1), note("D", 5, "", 1)].concat()
            + "<backup><duration>2</duration></backup>"
            + &[note("D", 4, "", 2), note("B", 4, "plain", 2)].concat(),
        [note("E", 5, "flat-up", 1), note("E", 5, "plain", 1)].concat(),
        // The key's flat after a quarter-flat is restated; a natural is shown.
        [note("B", 4, "quarter-flat", 1), note("B", 4, "flat", 1)].concat(),
        [note("B", 4, "flat-up", 1), note("B", 4, "natural", 1)].concat(),
        // The same alteration by the other notation is not shown again; at
        // another octave it is.
        [
            note("E", 5, "sharp-down", 1),
            note("E", 5, "quarter-sharp", 1),
        ]
        .concat(),
        [note("E", 5, "sharp-down", 1), note("E", 4, "sharp-down", 1)].concat(),
        // A quarter-tone after a quarter-tone of another value is shown, and
        // the next measure states each again.
        [note("A", 4, "flat-down", 1), note("A", 4, "natural-up", 1)].concat(),
        [note("A", 4, "natural-up", 1), note("B", 4, "", 1)].concat(),
        // Two voices on one D, a quarter-tone flat and a quarter-tone sharp,
        // share no notehead.
        [note("D", 5, "flat-up", 1), note("A", 4, "", 1)].concat()
            + "<backup><duration>2</duration></backup>"
            + &[note("D", 5, "natural-up", 2), note("F", 4, "", 2)].concat(),
    ];
    let mut body = String::new();
    for (m, content) in measures.iter().enumerate() {
        body.push_str(&format!("<measure number=\"{}\">", m + 1));
        if m == 0 {
            body.push_str(
                "<attributes><divisions>1</divisions><key><fifths>-1</fifths></key>\
                 <time><beats>2</beats><beat-type>4</beat-type></time>\
                 <clef><sign>G</sign><line>2</line></clef></attributes>",
            );
        }
        body.push_str(content);
        body.push_str("</measure>");
    }
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Flute</part-name></score-part></part-list><part id=\"P1\">{body}</part>\
         </score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("quarter_tone_carry.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let score = &loaded.reduced.score;
    let layout = epiphany_cli::engrave_loaded(&loaded).layout;
    // Each pitch in time order, the first voice before the second at one time,
    // with the accidentals drawn for it.
    let mut pitches: Vec<(epiphany_core::RationalTime, u64, epiphany_core::PitchId)> = score
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Pitched(p) => match &p.position {
                EventPosition::Musical(at) => Some((at.0.clone(), p)),
                _ => None,
            },
            _ => None,
        })
        .flat_map(|(at, p)| {
            let voice = score
                .canvas
                .regions
                .iter()
                .flat_map(|r| r.staff_instances())
                .flat_map(|i| i.voices.iter().enumerate())
                .find(|(_, v)| v.events.contains(&p.id))
                .map_or(9, |(k, _)| k as u64);
            p.pitches.iter().map(move |ip| (at.clone(), voice, ip.id))
        })
        .collect();
    pitches.sort();
    let drawn: Vec<String> = pitches
        .iter()
        .map(|(_, _, id)| {
            let names: Vec<&str> = layout
                .glyphs
                .iter()
                .filter(|g| g.provenance.source == TypedObjectId::Pitch(*id))
                .map(|g| g.glyph.as_str())
                .filter(|n| n.starts_with("accidental"))
                .collect();
            names.join("+")
        })
        .collect();
    assert_eq!(
        drawn,
        [
            "accidentalQuarterToneFlatStein",
            "",
            "",
            "",
            "accidentalQuarterToneSharpNaturalArrowUp",
            "",
            "",
            "accidentalNatural",
            "accidentalQuarterToneFlatArrowUp",
            "accidentalNatural",
            "accidentalQuarterToneFlatStein",
            "accidentalFlat",
            "accidentalQuarterToneFlatArrowUp",
            "accidentalNatural",
            "accidentalQuarterToneSharpArrowDown",
            "",
            "accidentalQuarterToneSharpArrowDown",
            "accidentalQuarterToneSharpArrowDown",
            "accidentalThreeQuarterTonesFlatArrowDown",
            "accidentalQuarterToneSharpNaturalArrowUp",
            "accidentalQuarterToneSharpNaturalArrowUp",
            "",
            "accidentalQuarterToneFlatArrowUp",
            "accidentalQuarterToneSharpNaturalArrowUp",
            "",
            "",
        ]
    );
    // The last measure's two D's stand apart, a quarter-tone flat and a
    // quarter-tone sharp.
    let x_of = |id: &epiphany_core::PitchId| {
        layout
            .glyphs
            .iter()
            .find(|g| {
                g.provenance.source == TypedObjectId::Pitch(*id)
                    && g.glyph.as_str().starts_with("notehead")
            })
            .map(|g| g.position.x.0)
            .expect("a head")
    };
    let pair = &pitches[pitches.len() - 4..pitches.len() - 2];
    assert_eq!(pair[0].0, pair[1].0, "both voices at one time");
    assert!(
        (x_of(&pair[0].2) - x_of(&pair[1].2)).abs() > 0.5,
        "two D quarter-tones share one notehead"
    );
}

/// Two voices sounding one letter and octave at once with two alterations,
/// a quarter-tone or a flat beside a natural, each show their own
/// accidental, the natural included, in either voice's order and on a tie
/// continuation beside a fresh one too, and stand clear of each other; the
/// next note there states its own. A unison of one alteration shows it once,
/// and one of two tie continuations none, each read by its tie.
#[test]
fn a_unison_of_two_alterations_shows_both_accidentals() {
    use epiphany_core::{Event, EventPosition, TypedObjectId};

    // In 4/4, quarters: (step, octave, alter, quarter-tone name) per voice. A
    // quarter-tone is written by name alone, as MuseScore writes its arrows.
    let tied = |step: &str, octave: u8, alter: i8, tie: &str, voice: u8| {
        format!(
            "<note><pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave>\
             </pitch><duration>2</duration><tie type=\"{tie}\"/><voice>{voice}</voice>\
             <type>quarter</type></note>"
        )
    };
    let note = |step: &str, octave: u8, alter: i8, name: &str, voice: u8| {
        let accidental = match name {
            "" => String::new(),
            name => format!("<accidental>{name}</accidental>"),
        };
        let alter = if name.is_empty() {
            format!("<alter>{alter}</alter>")
        } else {
            String::new()
        };
        format!(
            "<note><pitch><step>{step}</step>{alter}<octave>{octave}</octave></pitch>\
             <duration>2</duration><voice>{voice}</voice><type>quarter</type>{accidental}</note>"
        )
    };
    let backup = "<backup><duration>8</duration></backup>";
    let measures = [
        // B natural over B flat-up; then B natural again, in the first voice.
        [
            note("B", 4, 0, "", 1),
            note("B", 4, 0, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
        ]
        .concat()
            + backup
            + &[
                note("B", 4, 0, "flat-up", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
            ]
            .concat(),
        // B flat over B natural, the voices the other way about; then B
        // natural again, in the second voice.
        [
            note("B", 4, -1, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
        ]
        .concat()
            + backup
            + &[
                note("B", 4, 0, "", 2),
                note("B", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
            ]
            .concat(),
        // F sharp in both voices at once: one alteration, shown once.
        [
            note("F", 5, 1, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
        ]
        .concat()
            + backup
            + &[
                note("F", 5, 1, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
            ]
            .concat(),
        // B natural tied over the barline, met by B flat in the other voice:
        // the tie continuation shows its natural, the flat its flat.
        [
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            tied("B", 4, 0, "start", 1),
        ]
        .concat()
            + backup
            + &[
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
            ]
            .concat(),
        [
            tied("B", 4, 0, "stop", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
        ]
        .concat()
            + backup
            + &[
                note("B", 4, -1, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
            ]
            .concat(),
        // B natural and B flat at once, each tied over the barline: in the
        // next measure both are tie continuations, each read by its own tie.
        [
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            tied("B", 4, 0, "start", 1),
        ]
        .concat()
            + backup
            + &[
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                tied("B", 4, -1, "start", 2),
            ]
            .concat(),
        [
            tied("B", 4, 0, "stop", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
            note("D", 5, 0, "", 1),
        ]
        .concat()
            + backup
            + &[
                tied("B", 4, -1, "stop", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
                note("G", 4, 0, "", 2),
            ]
            .concat(),
    ];
    let loaded = treble_part("unison_alterations.musicxml", &measures);
    let score = &loaded.reduced.score;
    let layout = epiphany_cli::engrave_loaded(&loaded).layout;
    // Each pitch in time order, the first voice before the second at one time.
    let mut pitches: Vec<(epiphany_core::RationalTime, u64, epiphany_core::PitchId)> = score
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Pitched(p) => match &p.position {
                EventPosition::Musical(at) => Some((at.0.clone(), p)),
                _ => None,
            },
            _ => None,
        })
        .flat_map(|(at, p)| {
            let voice = score
                .canvas
                .regions
                .iter()
                .flat_map(|r| r.staff_instances())
                .flat_map(|i| i.voices.iter().enumerate())
                .find(|(_, v)| v.events.contains(&p.id))
                .map_or(9, |(k, _)| k as u64);
            p.pitches.iter().map(move |ip| (at.clone(), voice, ip.id))
        })
        .collect();
    pitches.sort();
    let accidentals = |id: &epiphany_core::PitchId| -> Vec<&epiphany_layout_ir::ResolvedGlyph> {
        layout
            .glyphs
            .iter()
            .filter(|g| {
                g.provenance.source == TypedObjectId::Pitch(*id)
                    && g.glyph.as_str().starts_with("accidental")
            })
            .collect()
    };
    let drawn: Vec<String> = pitches
        .iter()
        .map(|(_, _, id)| {
            accidentals(id)
                .iter()
                .map(|g| g.glyph.as_str())
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect();
    let (n, f, s, q) = (
        "accidentalNatural",
        "accidentalFlat",
        "accidentalSharp",
        "accidentalQuarterToneFlatArrowUp",
    );
    assert_eq!(
        drawn,
        [
            n, q, n, "", "", "", "", "", // B natural and B flat-up, then B natural
            f, n, "", n, "", "", "", "", // B flat and B natural, then B natural
            s, "", "", "", "", "", "", "", // F sharp in both voices, shown once
            "", "", "", "", "", "", "", "", // B natural tied over the barline
            n, f, "", "", "", "", "", "", // the tie continuation beside B flat
            "", "", "", "", "", "", n, f, // B natural and B flat, each tied on
            "", "", "", "", "", "", "", "", // two tie continuations, shown by their ties
        ]
    );
    // The two accidentals of each unison stand clear of each other.
    for first in [0, 8, 32, 46] {
        let (a, b) = (
            accidentals(&pitches[first].2),
            accidentals(&pitches[first + 1].2),
        );
        assert!(
            !boxes_overlap(glyph_box(a[0]), glyph_box(b[0])),
            "a unison's two accidentals overlap"
        );
    }
}

/// The hand-written quarter-tone fixture through the whole pipeline, locked
/// to a golden: each of the fourteen quarter-tone accidentals once; the
/// natural a note after a quarter-tone shows when it writes no accidental, in
/// its own voice or another, since the accidental applies to its own note
/// alone; a natural after a tie; and quarter-tones tied over a barline, along
/// a chain and in a second voice, their continuations unmarked. Its features are counted
/// first, so the golden cannot lock a page that lost one. Regenerate
/// deliberately, with renders beside it, with `UPDATE_GOLDEN=1`.
#[test]
fn the_quarter_tone_fixture_engraves_to_its_golden() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("arrow_accidentals.svg");
    let _ = std::fs::remove_file(&out);
    let status = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("render")
        .arg(fixture("arrow_accidentals.musicxml"))
        .args(["--page", "1", "-o"])
        .arg(&out)
        .output()
        .expect("runs");
    assert!(status.status.success(), "{status:?}");
    let svg = std::fs::read_to_string(&out).expect("an SVG was written");

    let count = |needle: &str| svg.matches(needle).count();
    for (glyph, expected) in [
        ("accidentalQuarterToneFlatArrowUp", 3),
        ("accidentalThreeQuarterTonesFlatArrowDown", 2),
        ("accidentalQuarterToneSharpNaturalArrowUp", 3),
        ("accidentalQuarterToneFlatNaturalArrowDown", 1),
        ("accidentalThreeQuarterTonesSharpArrowUp", 2),
        ("accidentalQuarterToneSharpArrowDown", 1),
        ("accidentalFiveQuarterTonesSharpArrowUp", 1),
        ("accidentalThreeQuarterTonesSharpArrowDown", 1),
        ("accidentalThreeQuarterTonesFlatArrowUp", 1),
        ("accidentalFiveQuarterTonesFlatArrowDown", 1),
        ("accidentalQuarterToneFlatStein", 1),
        ("accidentalQuarterToneSharpStein", 1),
        ("accidentalThreeQuarterTonesFlatZimmermann", 1),
        ("accidentalThreeQuarterTonesSharpStein", 1),
        ("accidentalNatural", 3),
        ("accidentalFlat", 0),
        ("accidentalSharp", 0),
    ] {
        assert_eq!(
            count(&format!("data-glyph=\"{glyph}\"")),
            expected,
            "{glyph}"
        );
    }
    assert_eq!(count("data-kind=\"curve\""), 5, "five ties, no slur");
    let loaded = load(&fixture("arrow_accidentals.musicxml")).expect("loads");
    let engraved = epiphany_cli::engrave_loaded(&loaded);
    let found = omissions(
        &loaded.reduced.score,
        &engraved.layout,
        &engraved.diagnostics,
    );
    assert!(found.kinds.is_empty(), "{found:?}");

    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/arrow_accidentals.page-1.svg");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden.parent().expect("a directory")).expect("created");
        std::fs::write(&golden, &svg).expect("golden written");
    }
    let expected = std::fs::read_to_string(&golden)
        .unwrap_or_else(|e| panic!("{}: {e}; regenerate with UPDATE_GOLDEN=1", golden.display()));
    assert!(
        expected == svg,
        "the page differs from {}; if intended, regenerate with UPDATE_GOLDEN=1",
        golden.display()
    );
}

/// A file set transposed is drawn as it is set: `engrave_loaded` lays out
/// the written view of a transposed score, its transposing parts at written
/// pitch, and a concert score as the model holds it.
#[test]
fn a_transposed_file_is_drawn_at_written_pitch_and_a_concert_one_at_concert_pitch() {
    use epiphany_cli::{engrave_loaded, engrave_on, geometry};
    use epiphany_layout_ir::written_view;
    let transposed = load(&fixture("written_keys.musicxml")).expect("loads");
    assert!(!transposed.import.source.concert);
    let page = geometry(&transposed.import.source);
    let drawn = engrave_loaded(&transposed).layout;
    assert_eq!(
        drawn,
        engrave_on(&written_view(&transposed.reduced.score), page).layout
    );
    assert_ne!(drawn, engrave_on(&transposed.reduced.score, page).layout);

    let concert = load(&fixture("concert_transposing.musicxml")).expect("loads");
    assert!(concert.import.source.concert);
    let page = geometry(&concert.import.source);
    assert_eq!(
        engrave_loaded(&concert).layout,
        engrave_on(&concert.reduced.score, page).layout
    );
}

/// An open key is no key signature until the staff's first: the lead shows
/// none and a B flat before the change states its flat, which the four-flat
/// key then carries. Before, a staff whose first key came later was read in
/// that key from its start.
#[test]
fn a_staff_has_no_key_before_its_first() {
    use epiphany_core::TypedObjectId;
    let note = "<note><pitch><step>B</step><alter>-1</alter><octave>4</octave></pitch>\
                <duration>4</duration><voice>1</voice><type>whole</type></note>";
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>1</divisions>\
         <key><fifths>0</fifths><mode>none</mode></key><time><beats>4</beats>\
         <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
         </attributes>{note}</measure>\
         <measure number=\"2\"><attributes><key><fifths>-4</fifths></key></attributes>\
         {note}</measure></part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("open_key.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    let first_head = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("notehead"))
        .map(|g| g.position.x.0)
        .fold(f32::INFINITY, f32::min);
    let flats: Vec<&epiphany_layout_ir::ResolvedGlyph> = layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "accidentalFlat")
        .collect();
    let of_key = |g: &&&epiphany_layout_ir::ResolvedGlyph| {
        matches!(g.provenance.source, TypedObjectId::StaffInstance(_))
    };
    assert_eq!(
        flats
            .iter()
            .filter(of_key)
            .filter(|g| g.position.x.0 < first_head)
            .count(),
        0,
        "no key in the lead"
    );
    assert_eq!(
        flats.iter().filter(|g| !of_key(g)).count(),
        1,
        "the first B flat states its flat, the key carries the second"
    );
}
