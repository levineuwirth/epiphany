//! The engraving-omission count on the importer's hand-written fixtures.
//! Each test changes a real layout and checks that the count moves with it,
//! so it holds whatever the engraver draws today.

use std::path::{Path, PathBuf};

use epiphany_cli::omissions::omissions;
use epiphany_cli::{engrave, load, Engraved, Loaded};
use epiphany_core::{Event, TypedObjectId};
use epiphany_layout_ir::GlyphReference;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../epiphany-musicxml/tests/fixtures")
        .join(name)
}

fn loaded(name: &str) -> (Loaded, Engraved) {
    let loaded = load(&fixture(name)).expect("the fixture loads");
    let engraved = engrave(&loaded.reduced.score);
    (loaded, engraved)
}

fn count(loaded: &Loaded, engraved: &Engraved, kind: &str) -> usize {
    omissions(
        &loaded.reduced.score,
        &engraved.layout,
        &engraved.diagnostics,
    )
    .kinds
    .get(kind)
    .copied()
    .unwrap_or(0)
}

#[test]
fn a_slur_with_no_ink_is_counted_and_an_anchor_is_not_ink() {
    let (loaded, mut engraved) = loaded("ties_and_slurs.musicxml");
    let slurs = loaded.reduced.score.cross_cutting.slurs.len();
    assert_eq!(slurs, 2);
    let is_slur = |source: &TypedObjectId| matches!(source, TypedObjectId::Slur(_));
    let anchor = engraved
        .layout
        .strokes
        .iter()
        .find(|s| s.from == s.to)
        .cloned()
        .expect("the layout carries a traced anchor");
    // Re-attributed rather than removed, so the systems' indices stay valid.
    let region = TypedObjectId::Region(loaded.reduced.score.canvas.regions[0].id);
    for g in &mut engraved.layout.glyphs {
        if is_slur(&g.provenance.source) {
            g.provenance.source = region;
        }
    }
    for s in &mut engraved.layout.strokes {
        if is_slur(&s.provenance.source) {
            s.provenance.source = region;
        }
    }
    for c in &mut engraved.layout.curves {
        if is_slur(&c.provenance.source) {
            c.provenance.source = region;
        }
    }
    assert_eq!(count(&loaded, &engraved, "slur not drawn"), slurs);
    // A zero-length stroke traced to a slur is still no ink for it.
    let mut traced = anchor;
    traced.provenance.source = TypedObjectId::Slur(loaded.reduced.score.cross_cutting.slurs[0].id);
    engraved.layout.strokes.push(traced);
    assert_eq!(count(&loaded, &engraved, "slur not drawn"), slurs);
}

/// A shown tuplet with no ink of its own is counted, whatever the engraver
/// draws for it today: the fixture's quarter-note triplet across two
/// staves, which no beam joins, as drawn; every tuplet once its ink is taken
/// away; and one whose bracket is drawn without its number.
#[test]
fn a_shown_tuplet_with_no_ink_is_counted_and_an_anchor_is_not_ink() {
    let (loaded, mut engraved) = loaded("cross_staff_tuplets.musicxml");
    let tuplets = loaded.reduced.score.cross_cutting.tuplets.len();
    assert_eq!(tuplets, 6);
    assert_eq!(count(&loaded, &engraved, "tuplet not drawn"), 1);
    assert_eq!(count(&loaded, &engraved, "tuplet number not drawn"), 0);
    let is_tuplet = |source: &TypedObjectId| matches!(source, TypedObjectId::Tuplet(_));
    let region = TypedObjectId::Region(loaded.reduced.score.canvas.regions[0].id);
    // A bracket drawn without its number.
    let bracketed = engraved
        .layout
        .strokes
        .iter()
        .find(|s| is_tuplet(&s.provenance.source) && s.from != s.to)
        .map(|s| s.provenance.source)
        .expect("a tuplet's bracket is drawn");
    for g in &mut engraved.layout.glyphs {
        if g.provenance.source == bracketed {
            g.provenance.source = region;
        }
    }
    assert_eq!(count(&loaded, &engraved, "tuplet number not drawn"), 1);
    // Re-attributed rather than removed, so the systems' indices stay valid.
    for g in &mut engraved.layout.glyphs {
        if is_tuplet(&g.provenance.source) {
            g.provenance.source = region;
        }
    }
    for s in &mut engraved.layout.strokes {
        if is_tuplet(&s.provenance.source) && s.from != s.to {
            s.provenance.source = region;
        }
    }
    assert_eq!(count(&loaded, &engraved, "tuplet not drawn"), tuplets);
    // Each tuplet keeps a traced anchor, which is no ink for it.
    assert!(engraved
        .layout
        .strokes
        .iter()
        .any(|s| is_tuplet(&s.provenance.source) && s.from == s.to));
}

#[test]
fn a_notehead_removed_is_counted_and_a_dot_supplied_is_not() {
    let (loaded, mut engraved) = loaded("single_part.musicxml");
    let score = &loaded.reduced.score;
    let voice = &score.canvas.regions[0].staff_instances()[0].voices[0];
    let Some(Event::Pitched(dotted)) = score.events.get(voice.events[0]) else {
        panic!("the first event is the dotted D5")
    };
    let pitch = TypedObjectId::Pitch(dotted.pitches[0].id);

    // Re-attributed rather than removed, so the systems' indices stay valid.
    let region = TypedObjectId::Region(score.canvas.regions[0].id);
    let before = count(&loaded, &engraved, "notehead not drawn");
    let mut without = engraved.layout.clone();
    for g in &mut without.glyphs {
        if g.provenance.source == pitch && g.glyph.as_str().starts_with("notehead") {
            g.provenance.source = region;
        }
    }
    let spoiled = Engraved {
        layout: without,
        diagnostics: engraved.diagnostics.clone(),
        geometry: engraved.geometry,
        staff_space_mm: engraved.staff_space_mm,
        time: engraved.time,
    };
    assert_eq!(count(&loaded, &spoiled, "notehead not drawn"), before + 1);

    // Whatever dots the engraver draws for this note are taken away first.
    let event = TypedObjectId::Event(voice.events[0]);
    for g in &mut engraved.layout.glyphs {
        if (g.provenance.source == pitch || g.provenance.source == event)
            && g.glyph.as_str() == "augmentationDot"
        {
            g.provenance.source = region;
        }
    }
    let dots = count(&loaded, &engraved, "augmentation dot");
    let mut dot = engraved
        .layout
        .glyphs
        .iter()
        .find(|g| g.provenance.source == pitch)
        .cloned()
        .expect("the dotted note has ink");
    dot.glyph = GlyphReference::owned("augmentationDot");
    engraved.layout.glyphs.push(dot);
    let after = count(&loaded, &engraved, "augmentation dot");
    assert!(dots >= 1, "the dotted quarter without a dot is counted");
    assert_eq!(after, dots - 1, "a drawn dot satisfies the check");
}

#[test]
fn an_octave_clef_drawn_without_its_mark_is_counted() {
    // The bass clarinet and the double bass read 8vb clefs.
    let (loaded, mut engraved) = loaded("concert_transposing.musicxml");
    let score = &loaded.reduced.score;
    let octave: Vec<TypedObjectId> = score.canvas.regions[0]
        .staff_instances()
        .iter()
        .filter(|i| i.clef_sequence.iter().any(|c| c.clef.octave_shift != 0))
        .map(|i| TypedObjectId::StaffInstance(i.id))
        .collect();
    assert_eq!(octave.len(), 2);
    let set = |engraved: &mut Engraved, suffix: &str| {
        for g in &mut engraved.layout.glyphs {
            if octave.contains(&g.provenance.source) && g.glyph.as_str().contains("Clef") {
                let base = if g.glyph.as_str().starts_with('g') {
                    "gClef"
                } else {
                    "fClef"
                };
                g.glyph = GlyphReference::owned(format!("{base}{suffix}"));
            }
        }
    };
    set(&mut engraved, "");
    assert_eq!(count(&loaded, &engraved, "clef octave mark"), 2);
    set(&mut engraved, "8vb");
    assert_eq!(count(&loaded, &engraved, "clef octave mark"), 0);
    assert_eq!(
        count(
            &loaded,
            &engraved,
            "clef of another shape at a system start"
        ),
        0
    );
}

#[test]
fn accidentals_and_unpitched_values_are_counted_as_not_checked() {
    let unchecked = |name: &str| {
        let (loaded, engraved) = loaded(name);
        omissions(
            &loaded.reduced.score,
            &engraved.layout,
            &engraved.diagnostics,
        )
        .unchecked
    };
    let pitched = unchecked("chords_and_voices.musicxml");
    assert_eq!(
        pitched.get("accidental: not checked against the key and the measure"),
        Some(&7),
        "one per pitch"
    );
    // Each unpitched note is drawn, and its value is counted as not checked.
    let (loaded, engraved) = loaded("percussion.musicxml");
    let percussion = omissions(
        &loaded.reduced.score,
        &engraved.layout,
        &engraved.diagnostics,
    );
    assert_eq!(
        percussion
            .unchecked
            .get("unpitched note: value not checked"),
        Some(&2),
        "{percussion:?}"
    );
    assert_eq!(percussion.kinds.get("unpitched note not drawn"), None);
}

/// A tuplet member is checked against its written value, its sounding span
/// scaled by its tuplet's ratio: four quarters in the time of three sound
/// as dotted eighths and two in the time of three as dotted quarters, and
/// neither is a dotted note missing its dot or flag.
#[test]
fn a_tuplet_member_is_read_at_its_written_value() {
    let member = |step: &str, duration: u8, actual: u8, normal: u8| {
        format!(
            "<note><pitch><step>{step}</step><octave>4</octave></pitch>\
             <duration>{duration}</duration><voice>1</voice><type>quarter</type>\
             <time-modification><actual-notes>{actual}</actual-notes>\
             <normal-notes>{normal}</normal-notes></time-modification></note>"
        )
    };
    let rest = "<note><rest/><duration>4</duration><voice>1</voice><type>quarter</type></note>";
    let quadruplet: String = ["C", "D", "E", "F"]
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let note = member(step, 3, 4, 3);
            match i {
                0 => note.replace(
                    "</time-modification>",
                    "</time-modification><notations><tuplet type=\"start\"/></notations>",
                ),
                3 => note.replace(
                    "</time-modification>",
                    "</time-modification><notations><tuplet type=\"stop\"/></notations>",
                ),
                _ => note,
            }
        })
        .collect();
    let duplet = format!(
        "{}{}",
        member("G", 6, 2, 3).replace(
            "</time-modification>",
            "</time-modification><notations><tuplet type=\"start\"/></notations>"
        ),
        member("A", 6, 2, 3).replace(
            "</time-modification>",
            "</time-modification><notations><tuplet type=\"stop\"/></notations>"
        ),
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>4</divisions><time><beats>4</beats>\
         <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
         </attributes>{quadruplet}{rest}</measure>\
         <measure number=\"2\">{duplet}{rest}</measure></part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tuplet_values.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert_eq!(loaded.reduced.score.cross_cutting.tuplets.len(), 2);
    let engraved = engrave(&loaded.reduced.score);
    for kind in [
        "augmentation dot",
        "flag or beam",
        "notehead of another value",
    ] {
        assert_eq!(count(&loaded, &engraved, kind), 0, "{kind}");
    }
    let heads = engraved
        .layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "noteheadBlack")
        .count();
    assert_eq!(heads, 6, "six quarter-note heads, none dotted or split");
}

/// Every mark of a note and every fermata, breath mark and caesura the
/// engraver draws is no omission; each whose ink is taken away is counted
/// by its class; and a fermata anchored at its region's offset, on no
/// staff, is named as drawn nowhere.
#[test]
fn the_expression_familys_undrawn_members_are_counted_by_class() {
    use epiphany_core::{MarkerKind, TimeAnchor};
    use epiphany_layout_ir::NoteMarkPart;

    let note = |pitch: &str, notations: &str| {
        let (step, octave) = pitch.split_at(1);
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>2</duration><voice>1</voice><type>quarter</type>\
             <notations>{notations}</notations></note>"
        )
    };
    let measure = [
        note(
            "C5",
            "<articulations><accent/><breath-mark/></articulations>",
        ),
        note(
            "E5",
            "<ornaments><trill-mark/><accidental-mark placement=\"above\">sharp\
             </accidental-mark></ornaments><fermata/>",
        ),
        note(
            "G4",
            "<arpeggiate/><articulations><caesura/></articulations>",
        ),
        note(
            "D5",
            "<ornaments><tremolo type=\"single\">2</tremolo></ornaments>",
        ),
    ]
    .concat();
    // A syllable on the last note.
    let at = measure.rfind("</note>").expect("a note");
    let measure = format!(
        "{}<lyric number=\"1\"><syllabic>single</syllabic><text>la</text></lyric>{}",
        &measure[..at],
        &measure[at..]
    );
    // A dynamic and a hairpin at the first note, the hairpin stopping at the
    // third.
    let measure = format!(
        "<direction><direction-type><dynamics><p/></dynamics></direction-type></direction>\
         <direction><direction-type><wedge type=\"crescendo\"/></direction-type></direction>\
         {}",
        measure.replacen(
            "<note><pitch><step>G</step>",
            "<direction><direction-type><wedge type=\"stop\"/></direction-type></direction>\
             <note><pitch><step>G</step>",
            1
        )
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{measure}</measure>\
         </part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("family_omissions.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let mut engraved = engrave(&loaded.reduced.score);
    let family = |loaded: &Loaded, engraved: &Engraved| -> Vec<(String, usize)> {
        omissions(
            &loaded.reduced.score,
            &engraved.layout,
            &engraved.diagnostics,
        )
        .kinds
        .into_iter()
        .filter(|(kind, _)| {
            [
                "note mark",
                "ornament",
                "fermata",
                "breath",
                "caesura",
                "dynamic",
                "hairpin",
                "lyric",
            ]
            .iter()
            .any(|class| kind.starts_with(class))
        })
        .collect()
    };
    assert_eq!(family(&loaded, &engraved), vec![], "every class drawn");

    // The accent's glyph, the ornament's accidental and the arpeggio's run
    // taken away, and the fermata's glyph re-attributed.
    let score = &loaded.reduced.score;
    let events: Vec<epiphany_core::EventId> = score.events.iter().map(Event::id).collect();
    let accent = NoteMarkPart::Glyph(epiphany_core::EventMark::Accent.tag()).id(events[0]);
    let accidental = NoteMarkPart::Accidental {
        tag: NoteMarkPart::ornament_tag(epiphany_core::OrnamentKind::Trill),
        above: true,
    }
    .id(events[1]);
    let arpeggio = NoteMarkPart::Arpeggio.id(events[2]);
    let region = TypedObjectId::Region(score.canvas.regions[0].id);
    for g in &mut engraved.layout.glyphs {
        if g.provenance.stable_id == accent {
            g.provenance.stable_id = epiphany_layout_ir::LayoutObjectId(1);
        }
        if matches!(g.provenance.source, TypedObjectId::Marker(_))
            && (g.glyph.as_str().starts_with("fermata") || g.glyph.as_str().starts_with("dynamic"))
        {
            g.provenance.source = region;
        }
    }
    for s in &mut engraved.layout.strokes {
        if matches!(s.provenance.source, TypedObjectId::Spanner(_)) {
            s.provenance.source = region;
        }
    }
    for t in &mut engraved.layout.texts {
        if matches!(t.provenance.source, TypedObjectId::LyricLine(_)) {
            t.provenance.source = region;
        }
        if [accidental, arpeggio].contains(&t.provenance.stable_id) {
            t.provenance.stable_id = epiphany_layout_ir::LayoutObjectId(2);
        }
    }
    let counted = family(&loaded, &engraved);
    for kind in [
        "note mark: accent",
        "ornament accidental: trill",
        "note mark: arpeggio",
        "fermata",
        "dynamic",
        "hairpin",
        "lyric",
    ] {
        assert!(
            counted.contains(&(kind.to_owned(), 1)),
            "{kind}: {counted:?}"
        );
    }
    assert_eq!(counted.len(), 7, "{counted:?}");

    // A fermata at its region's start: on no staff, drawn nowhere.
    let mut moved = loaded.reduced.score.clone();
    for marker in &mut moved.cross_cutting.markers {
        if matches!(marker.kind, MarkerKind::Fermata(_)) {
            marker.anchor = TimeAnchor::Region {
                id: moved.canvas.regions[0].id,
                edge: epiphany_core::RegionEdge::Start,
                offset: epiphany_core::AnchorOffset::Zero,
            };
        }
    }
    let engraved = engrave(&moved);
    let kinds = omissions(&moved, &engraved.layout, &engraved.diagnostics).kinds;
    assert_eq!(
        kinds.get("fermata at a region offset: drawn nowhere"),
        Some(&1)
    );
    assert_eq!(kinds.get("fermata"), None);
}

/// Staff text, tempo and rehearsal marks, segnos and codas the engraver
/// draws are no omission, a tempo mark repeated from another staff at its
/// time drawn with the one it repeats; each whose ink is taken away is
/// counted by its class; staff text anchored at its region's offset, on no
/// staff, is named as drawn nowhere, and a tempo mark there stands over the
/// system.
#[test]
fn text_marks_undrawn_are_counted_by_class() {
    use epiphany_core::{MarkerKind, TimeAnchor};

    let direction = |staff: u8, content: &str, sound: &str| {
        format!(
            "<direction placement=\"above\"><direction-type>{content}</direction-type>\
             <staff>{staff}</staff>{sound}</direction>"
        )
    };
    let note = |staff: u8, voice: u8, octave: u8| {
        format!(
            "<note><pitch><step>C</step><octave>{octave}</octave></pitch><duration>8\
             </duration><voice>{voice}</voice><type>whole</type><staff>{staff}</staff></note>"
        )
    };
    let tempo = |staff: u8| {
        direction(
            staff,
            "<words font-weight=\"bold\">Allegro</words>",
            "<sound tempo=\"120\"/>",
        )
    };
    let first = [
        direction(1, "<words>dolce</words>", ""),
        tempo(1),
        direction(1, "<rehearsal>A</rehearsal>", ""),
        note(1, 1, 5),
        "<backup><duration>8</duration></backup>".to_string(),
        tempo(2),
        note(2, 5, 3),
    ]
    .concat();
    let second = [
        direction(1, "<segno/>", ""),
        direction(1, "<coda/>", ""),
        note(1, 1, 5),
        "<backup><duration>8</duration></backup>".to_string(),
        note(2, 5, 3),
    ]
    .concat();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef>\
         <clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>{first}\
         </measure><measure number=\"2\">{second}</measure></part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("text_mark_omissions.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let classes = [
        "staff text",
        "tempo mark",
        "rehearsal mark",
        "segno",
        "coda",
    ];
    let counted = |score: &epiphany_core::Score, engraved: &Engraved| -> Vec<(String, usize)> {
        omissions(score, &engraved.layout, &engraved.diagnostics)
            .kinds
            .into_iter()
            .filter(|(kind, _)| classes.iter().any(|class| kind.starts_with(class)))
            .collect()
    };
    let score = &loaded.reduced.score;
    let tempos = score
        .cross_cutting
        .markers
        .iter()
        .filter(|m| matches!(m.kind, MarkerKind::Tempo(_)))
        .count();
    assert_eq!(tempos, 2, "a tempo mark on each staff");
    let mut engraved = engrave(score);
    assert_eq!(counted(score, &engraved), vec![], "every class drawn");

    // Every text mark's ink taken away.
    let region = TypedObjectId::Region(score.canvas.regions[0].id);
    for t in &mut engraved.layout.texts {
        if matches!(t.provenance.source, TypedObjectId::Marker(_)) {
            t.provenance.source = region;
        }
    }
    for s in &mut engraved.layout.strokes {
        if matches!(s.provenance.source, TypedObjectId::Marker(_)) {
            s.provenance.source = region;
        }
    }
    let mut found = counted(score, &engraved);
    found.sort();
    assert_eq!(
        found,
        vec![
            ("coda".to_owned(), 1),
            ("rehearsal mark".to_owned(), 1),
            ("segno".to_owned(), 1),
            ("staff text".to_owned(), 1),
            ("tempo mark".to_owned(), 2),
        ]
    );

    // Staff text and both tempo marks at the region's start.
    let mut moved = score.clone();
    for marker in &mut moved.cross_cutting.markers {
        if matches!(marker.kind, MarkerKind::Text(_) | MarkerKind::Tempo(_)) {
            marker.anchor = TimeAnchor::Region {
                id: moved.canvas.regions[0].id,
                edge: epiphany_core::RegionEdge::Start,
                offset: epiphany_core::AnchorOffset::Zero,
            };
        }
    }
    let engraved = engrave(&moved);
    assert_eq!(
        counted(&moved, &engraved),
        vec![("staff text at a region offset: drawn nowhere".to_owned(), 1)]
    );
    assert_eq!(
        engraved
            .layout
            .texts
            .iter()
            .filter(|t| t.text == "Allegro")
            .count(),
        1
    );
}

/// A trill line the engraver draws is no omission, nor the trill at its
/// note, which its sign draws; taken away, each is counted.
#[test]
fn a_trill_line_and_its_trill_undrawn_are_counted() {
    let note = |ornaments: &str| {
        format!(
            "<note><pitch><step>C</step><octave>5</octave></pitch><duration>4</duration>\
             <voice>1</voice><type>half</type><notations><ornaments>{ornaments}\
             </ornaments></notations></note>"
        )
    };
    let measure = [
        note("<trill-mark/><wavy-line type=\"start\"/>"),
        note("<wavy-line type=\"stop\"/>"),
    ]
    .concat();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{measure}</measure>\
         </part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("trill_line_omissions.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let score = &loaded.reduced.score;
    let counted = |engraved: &Engraved| -> Vec<(String, usize)> {
        omissions(score, &engraved.layout, &engraved.diagnostics)
            .kinds
            .into_iter()
            .filter(|(kind, _)| kind.starts_with("trill") || kind.starts_with("ornament"))
            .collect()
    };
    let mut engraved = engrave(score);
    assert_eq!(counted(&engraved), vec![], "drawn");
    let region = TypedObjectId::Region(score.canvas.regions[0].id);
    for g in &mut engraved.layout.glyphs {
        if matches!(g.provenance.source, TypedObjectId::Spanner(_)) {
            g.provenance.source = region;
        }
    }
    let mut found = counted(&engraved);
    found.sort();
    assert_eq!(
        found,
        vec![
            ("ornament: trill".to_owned(), 1),
            ("trill line".to_owned(), 1)
        ]
    );
}
