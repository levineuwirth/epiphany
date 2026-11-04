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

/// A two-note tremolo's strokes are its two notes' beam, as MuseScore draws
/// the pair: two eighths written with a 2:1 time modification and no tuplet,
/// as MuseScore exports them, sound as sixteenths, and need no flag of their
/// own while the strokes are drawn; with the strokes taken away, each is
/// counted. The score names its beams (a beamed pair of eighths after the
/// tremolo), so the tremolo's notes are beamed by none.
#[test]
fn a_two_note_tremolos_strokes_are_its_notes_beam() {
    let member = |step: &str, kind: &str| {
        format!(
            "<note><pitch><step>{step}</step><octave>5</octave></pitch>\
             <duration>1</duration><voice>1</voice><type>eighth</type>\
             <time-modification><actual-notes>2</actual-notes>\
             <normal-notes>1</normal-notes></time-modification><notations><ornaments>\
             <tremolo type=\"{kind}\">3</tremolo></ornaments></notations></note>"
        )
    };
    let eighth = |step: &str, beam: &str| {
        format!(
            "<note><pitch><step>{step}</step><octave>5</octave></pitch>\
             <duration>2</duration><voice>1</voice><type>eighth</type>\
             <beam number=\"1\">{beam}</beam></note>"
        )
    };
    let rest = "<note><rest/><duration>2</duration><voice>1</voice><type>eighth</type></note>\
                <note><rest/><duration>8</duration><voice>1</voice><type>half</type></note>";
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>4</divisions><time><beats>4</beats>\
         <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
         </attributes>{}{}{}{}{rest}</measure></part></score-partwise>",
        member("C", "start"),
        member("E", "stop"),
        eighth("G", "begin"),
        eighth("A", "end"),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tremolo_pair.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let score = &loaded.reduced.score;
    let first = score.canvas.regions[0].staff_instances()[0].voices[0].events[0];
    let mut engraved = engrave(score);
    let strokes = |engraved: &Engraved| {
        engraved
            .layout
            .strokes
            .iter()
            .filter(|s| s.provenance.source == TypedObjectId::Event(first) && s.from.x != s.to.x)
            .count()
    };
    assert_eq!(
        strokes(&engraved),
        3,
        "three strokes between the two, no beam"
    );
    assert_eq!(count(&loaded, &engraved, "flag or beam"), 0);

    // Each stroke made a zero-length anchor, which is no ink, rather than
    // removed, so the systems' indices stay valid.
    for s in &mut engraved.layout.strokes {
        if s.provenance.source == TypedObjectId::Event(first) && s.from.x != s.to.x {
            s.to = s.from;
        }
    }
    assert_eq!(strokes(&engraved), 0);
    assert_eq!(count(&loaded, &engraved, "flag or beam"), 2);
}

/// A two-note tremolo's notes are read at their written value, one longer
/// than they sound, as MuseScore keeps them and the engraver draws them: a
/// pair of halves and a pair of dotted halves, written with a 2:1 time
/// modification and no tuplet, count no notehead of another value, nor a
/// missing dot, while their hollow heads are drawn; drawn at their sounding
/// value instead, black, each head is counted. A pair a tuplet holds is read
/// at its written value too, the importer reading the tuplet's ratio with
/// the pair's 2:1 divided out: quarters in a tuplet the pair opens, halves
/// in a triplet another note opens; a note marked to start a tremolo with a
/// rest after it in its voice, and the rest, at their own.
#[test]
fn a_two_note_tremolos_notes_are_read_at_their_written_value() {
    let member = |step: &str, duration: u8, kind: &str, ratio: (u8, u8), notations: &str| {
        format!(
            "<note><pitch><step>{step}</step><octave>4</octave></pitch>\
             <duration>{duration}</duration><voice>1</voice><type>{kind}</type>\
             <time-modification><actual-notes>{}</actual-notes>\
             <normal-notes>{}</normal-notes></time-modification><notations>{notations}\
             </notations></note>",
            ratio.0, ratio.1
        )
    };
    let start = "<ornaments><tremolo type=\"start\">3</tremolo></ornaments>";
    let stop = "<ornaments><tremolo type=\"stop\">3</tremolo></ornaments>";
    let rest = |duration: u8, kind: &str| {
        format!("<note><rest/><duration>{duration}</duration><voice>1</voice><type>{kind}</type></note>")
    };
    let dotted = |xml: String| xml.replacen("</type>", "</type><dot/>", 1);
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>4</divisions><time><beats>4</beats>\
         <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
         </attributes>{}{}{}{}{}</measure><measure number=\"2\">{}{}{}</measure>\
         <measure number=\"3\">{}{}{}</measure><measure number=\"4\">{}{}{}{}\
         </measure></part></score-partwise>",
        member("G", 4, "half", (2, 1), start),
        member("B", 4, "half", (2, 1), stop),
        member(
            "D",
            3,
            "quarter",
            (4, 3),
            &format!("<tuplet type=\"start\"/>{start}")
        ),
        member(
            "F",
            3,
            "quarter",
            (4, 3),
            &format!("<tuplet type=\"stop\"/>{stop}")
        ),
        rest(2, "eighth"),
        dotted(member("A", 6, "half", (2, 1), start)),
        dotted(member("C", 6, "half", (2, 1), stop)),
        rest(4, "quarter"),
        member("E", 4, "quarter", (1, 1), start),
        rest(4, "quarter"),
        rest(8, "half"),
        // At twelve divisions to the quarter, a triplet of quarters whose last
        // two are a pair of written halves, 6:2, the triplet's 3:2 and the
        // pair's 2:1.
        String::from("<attributes><divisions>12</divisions></attributes>")
            + &member("G", 8, "quarter", (3, 2), "<tuplet type=\"start\"/>"),
        member("B", 8, "half", (6, 2), start),
        member(
            "D",
            8,
            "half",
            (6, 2),
            &format!("<tuplet type=\"stop\"/>{stop}")
        )
        .replacen("<octave>4</octave>", "<octave>5</octave>", 1),
        rest(24, "half"),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tremolo_values.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let mut engraved = engrave(&loaded.reduced.score);
    assert_eq!(count(&loaded, &engraved, "notehead of another value"), 0);
    assert_eq!(count(&loaded, &engraved, "augmentation dot"), 0);
    assert_eq!(count(&loaded, &engraved, "rest drawn at another value"), 0);

    // Drawn at their sounding value: four black heads where hollow ones are
    // written.
    let mut blackened = 0;
    for g in &mut engraved.layout.glyphs {
        if g.glyph.as_str() == "noteheadHalf" {
            g.glyph = GlyphReference::owned("noteheadBlack");
            blackened += 1;
        }
    }
    assert_eq!(blackened, 6);
    assert_eq!(count(&loaded, &engraved, "notehead of another value"), 6);
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

/// Italic words import as expression text, which the engraver draws italic
/// under its staff (X4d.6): drawn, it is no omission, and with its ink taken
/// away it is counted by its own class, the upright words beside it staff
/// text drawn and not counted.
#[test]
fn expression_text_is_drawn_and_counted_undrawn() {
    use epiphany_core::MarkerKind;

    let direction = |content: &str| {
        format!(
            "<direction placement=\"above\"><direction-type>{content}</direction-type>\
             </direction>"
        )
    };
    let measure = [
        direction("<words>dolce</words>"),
        direction("<words font-style=\"italic\">espressivo</words>"),
        "<note><pitch><step>C</step><octave>5</octave></pitch><duration>8</duration>\
         <voice>1</voice><type>whole</type></note>"
            .to_string(),
    ]
    .concat();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{measure}</measure>\
         </part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("expression_text_omission.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let score = &loaded.reduced.score;
    let marker = |words: &str| {
        score
            .cross_cutting
            .markers
            .iter()
            .find(|m| match &m.kind {
                MarkerKind::Text(t) | MarkerKind::ExpressionText(t) => t.as_str() == words,
                _ => false,
            })
            .expect("a marker")
    };
    assert!(matches!(marker("dolce").kind, MarkerKind::Text(_)));
    assert!(matches!(
        marker("espressivo").kind,
        MarkerKind::ExpressionText(_)
    ));
    let mut engraved = engrave(score);
    let drawn = |engraved: &Engraved, words: &str| {
        engraved
            .layout
            .texts
            .iter()
            .filter(|t| t.text == words)
            .count()
    };
    assert_eq!(drawn(&engraved, "dolce"), 1, "the staff text is drawn");
    assert_eq!(
        drawn(&engraved, "espressivo"),
        1,
        "the expression text is drawn"
    );
    let counted = |engraved: &Engraved| {
        omissions(score, &engraved.layout, &engraved.diagnostics)
            .kinds
            .into_iter()
            .filter(|(kind, _)| kind.ends_with(" text"))
            .collect::<Vec<_>>()
    };
    assert_eq!(counted(&engraved), vec![], "both drawn");

    // The expression text's ink taken away.
    let region = TypedObjectId::Region(score.canvas.regions[0].id);
    let expression = TypedObjectId::Marker(marker("espressivo").id);
    for t in &mut engraved.layout.texts {
        if t.provenance.source == expression {
            t.provenance.source = region;
        }
    }
    assert_eq!(counted(&engraved), vec![("expression text".to_owned(), 1)]);
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

/// X4d.1's lines the engraver draws (a pedal line, an ottava, a text line, a
/// bracket and a glissando) are no omission; their ink taken away, each is
/// counted by its class.
#[test]
fn the_lines_undrawn_are_counted() {
    let direction =
        |t: &str| format!("<direction><direction-type>{t}</direction-type></direction>");
    let note = |pitch: &str, notations: &str| {
        let (step, octave) = pitch.split_at(1);
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>2</duration><voice>1</voice><type>quarter</type>\
             <notations>{notations}</notations></note>"
        )
    };
    let measure = [
        direction("<pedal type=\"start\" line=\"yes\" sign=\"yes\"/>"),
        direction("<octave-shift type=\"down\" size=\"8\"/>"),
        "<direction><direction-type><words>rit.</words></direction-type><direction-type>\
         <dashes type=\"start\"/></direction-type></direction>"
            .to_owned(),
        direction("<bracket type=\"start\" line-end=\"none\" line-type=\"solid\"/>"),
        note(
            "C6",
            "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>",
        ),
        note("G6", "<slide type=\"stop\" number=\"1\"/>"),
        direction("<pedal type=\"stop\" line=\"yes\" sign=\"no\"/>"),
        direction("<octave-shift type=\"stop\" size=\"8\"/>"),
        direction("<dashes type=\"stop\"/>"),
        direction("<bracket type=\"stop\" line-end=\"down\"/>"),
        note("C5", ""),
        note("D5", ""),
    ]
    .concat();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{measure}</measure>\
         </part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lines_omissions.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let score = &loaded.reduced.score;
    let classes = ["pedal line", "ottava", "text line", "bracket", "glissando"];
    let counted = |engraved: &Engraved| -> Vec<(String, usize)> {
        let mut found: Vec<(String, usize)> =
            omissions(score, &engraved.layout, &engraved.diagnostics)
                .kinds
                .into_iter()
                .filter(|(kind, _)| classes.contains(&kind.as_str()))
                .collect();
        found.sort();
        found
    };
    let mut engraved = engrave(score);
    assert_eq!(counted(&engraved), vec![], "drawn");
    let region = TypedObjectId::Region(score.canvas.regions[0].id);
    for s in &mut engraved.layout.strokes {
        if matches!(s.provenance.source, TypedObjectId::Spanner(_)) {
            s.provenance.source = region;
        }
    }
    for t in &mut engraved.layout.texts {
        if matches!(t.provenance.source, TypedObjectId::Spanner(_)) {
            t.provenance.source = region;
        }
    }
    let mut want: Vec<(String, usize)> = classes.iter().map(|c| (c.to_string(), 1)).collect();
    want.sort();
    assert_eq!(counted(&engraved), want);
}

/// A gradual tempo change two parts hold at one time is drawn once over the
/// system, and the other part's is no omission.
#[test]
fn a_gradual_tempo_change_drawn_once_for_two_parts_is_no_omission() {
    let measure = "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4\
         </beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>\
         <direction><direction-type><words>rit.</words></direction-type><direction-type>\
         <dashes type=\"start\"/></direction-type></direction>\
         <note><pitch><step>C</step><octave>5</octave></pitch><duration>4</duration>\
         <voice>1</voice><type>half</type></note><direction><direction-type>\
         <dashes type=\"stop\"/></direction-type></direction><note><pitch><step>D</step>\
         <octave>5</octave></pitch><duration>4</duration><voice>1</voice><type>half</type>\
         </note>";
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part><score-part id=\"P2\"><part-name>B</part-name>\
         </score-part></part-list><part id=\"P1\"><measure number=\"1\">{measure}</measure>\
         </part><part id=\"P2\"><measure number=\"1\">{measure}</measure></part>\
         </score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tempo_change_omissions.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let score = &loaded.reduced.score;
    let engraved = engrave(score);
    assert_eq!(
        engraved
            .layout
            .texts
            .iter()
            .filter(|t| t.text == "rit.")
            .count(),
        1,
        "drawn once"
    );
    let found: Vec<_> = omissions(score, &engraved.layout, &engraved.diagnostics)
        .kinds
        .into_iter()
        .filter(|(kind, _)| kind == "text line")
        .collect();
    assert_eq!(found, vec![]);
}

/// A mark on a grace note is checked as a note's is (X4d fix round 1): the
/// staccato and the accent on two graces are drawn, so the report names
/// neither; with their runs drawn as other ink, it names each, where a check
/// that skipped graces named nothing.
#[test]
fn a_graces_marks_are_drawn_and_counted_undrawn() {
    use epiphany_core::EventMark;
    use epiphany_layout_ir::NoteMarkPart;

    let grace = |step: &str, octave: u8, mark: &str| {
        format!(
            "<note><grace slash=\"yes\"/><pitch><step>{step}</step><octave>{octave}</octave>\
             </pitch><voice>1</voice><type>eighth</type><notations><articulations>{mark}\
             </articulations></notations></note>"
        )
    };
    let note = |step: &str, octave: u8| {
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>4</duration><voice>1</voice><type>half</type></note>"
        )
    };
    let measure = [
        grace("G", 4, "<staccato/>"),
        note("A", 4),
        grace("D", 5, "<accent/>"),
        note("C", 5),
    ]
    .concat();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{measure}</measure>\
         </part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("grace_mark_omissions.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let mut engraved = engrave(&loaded.reduced.score);
    let events = &loaded.import.ids.events[0];
    assert!(loaded
        .reduced
        .score
        .events
        .get(events[0])
        .is_some_and(|e| e.grace().is_some()));
    assert_eq!(count(&loaded, &engraved, "note mark: staccato"), 0);
    assert_eq!(count(&loaded, &engraved, "note mark: accent"), 0);
    let ids = [
        NoteMarkPart::Glyph(EventMark::Staccato.tag()).id(events[0]),
        NoteMarkPart::Glyph(EventMark::Accent.tag()).id(events[2]),
    ];
    // Re-attributed rather than removed, so the systems' indices stay valid.
    let elsewhere = NoteMarkPart::Arpeggio.id(events[1]);
    let mut taken = 0;
    for t in &mut engraved.layout.texts {
        if ids.contains(&t.provenance.stable_id) {
            t.provenance.stable_id = elsewhere;
            taken += 1;
        }
    }
    assert_eq!(taken, 2, "both marks were drawn");
    assert_eq!(count(&loaded, &engraved, "note mark: staccato"), 1);
    assert_eq!(count(&loaded, &engraved, "note mark: accent"), 1);
}

/// A glissando drawn from or to a chord is named as drawn with its head
/// guessed (X4d fix round 1): the model keeps no head for it, so the line
/// takes its chord's head by its rank among the glissandi between the same
/// notes. One between single notes is not named.
#[test]
fn a_glissando_at_a_chord_is_named_as_drawn_with_its_head_guessed() {
    let note = |pitch: &str, chord: bool, notations: &str| {
        let (step, octave) = pitch.split_at(1);
        format!(
            "<note>{}<pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>4</duration><voice>1</voice><type>half</type>\
             <notations>{notations}</notations></note>",
            if chord { "<chord/>" } else { "" }
        )
    };
    let start = "<slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.</slide>";
    let stop = "<slide type=\"stop\" number=\"1\"/>";
    let measure = [
        note("C4", false, start),
        note("E4", true, ""),
        note("G5", false, stop),
        note("C5", false, start),
        note("G5", false, stop),
    ]
    .concat();
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{}</measure>\
         <measure number=\"2\">{}</measure></part></score-partwise>",
        &measure[..measure
            .find("<note><pitch><step>C</step><octave>5")
            .expect("the second")],
        &measure[measure
            .find("<note><pitch><step>C</step><octave>5")
            .expect("the second")..],
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("glissando_guessed.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let engraved = engrave(&loaded.reduced.score);
    assert_eq!(count(&loaded, &engraved, "glissando"), 0, "both drawn");
    assert_eq!(
        count(&loaded, &engraved, "glissando drawn with its head guessed"),
        1
    );
}

/// An undrawn glissando is named by why it is undrawn (X4d fix round 1):
/// between the two staves of a grand staff, between unpitched notes, and
/// between two thirty-seconds whose ink leaves its line no room, where
/// Gould sets such a line above or below the notes (*Behind Bars*,
/// p. 141); one with room is drawn and named nothing.
#[test]
fn an_undrawn_glissando_is_named_by_why() {
    let start = "<notations><slide type=\"start\" line-type=\"solid\" number=\"1\">gliss.\
                 </slide></notations>";
    let stop = "<notations><slide type=\"stop\" number=\"1\"/></notations>";
    let pitched = |step: &str, alter: i8, octave: u8, d: u8, kind: &str, staff: u8, n: &str| {
        let alter = if alter == 0 {
            String::new()
        } else {
            format!("<alter>{alter}</alter>")
        };
        let accidental = if alter.is_empty() {
            ""
        } else {
            "<accidental>flat</accidental>"
        };
        format!(
            "<note><pitch><step>{step}</step>{alter}<octave>{octave}</octave></pitch>\
             <duration>{d}</duration><voice>{staff}</voice><type>{kind}</type>{accidental}\
             <staff>{staff}</staff>{n}</note>"
        )
    };
    let upper = [
        pitched("D", 0, 5, 1, "32nd", 1, start),
        pitched("E", -1, 5, 1, "32nd", 1, stop),
        pitched("C", 0, 5, 2, "16th", 1, ""),
        pitched("C", 0, 5, 4, "eighth", 1, start),
        pitched("G", 0, 5, 8, "quarter", 1, stop),
        pitched("C", 0, 5, 16, "half", 1, start),
    ]
    .concat();
    let lower = [
        "<note><rest/><duration>16</duration><voice>2</voice><type>half</type><staff>2</staff>\
         </note>"
            .to_string(),
        pitched("C", 0, 3, 16, "half", 2, stop),
    ]
    .concat();
    let drum = |n: &str| {
        format!(
            "<note><unpitched><display-step>C</display-step><display-octave>5</display-octave>\
             </unpitched><duration>16</duration><voice>1</voice><type>half</type>{n}</note>"
        )
    };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part><score-part id=\"P2\"><part-name>B</part-name></score-part>\
         </part-list><part id=\"P1\"><measure number=\"1\"><attributes><divisions>8\
         </divisions><time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves>\
         <clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F\
         </sign><line>4</line></clef></attributes>{upper}<backup><duration>32</duration>\
         </backup>{lower}</measure></part><part id=\"P2\"><measure number=\"1\"><attributes>\
         <divisions>8</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef>\
         <sign>percussion</sign></clef></attributes>{}{}</measure></part></score-partwise>",
        drum(start),
        drum(stop),
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("glissandi_undrawn.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    assert!(loaded.fidelity.passed(), "{:#?}", loaded.fidelity.failures);
    let glissandi = loaded
        .reduced
        .score
        .cross_cutting
        .spanners
        .iter()
        .filter(|s| s.kind == epiphany_core::SpannerKind::Glissando)
        .count();
    assert_eq!(glissandi, 4, "four glissandi imported");
    let engraved = engrave(&loaded.reduced.score);
    let named = |kind: &str| count(&loaded, &engraved, kind);
    assert_eq!(named("glissando between staves"), 1);
    assert_eq!(named("glissando of unpitched notes"), 1);
    assert_eq!(named("glissando too short for its notes"), 1);
    assert_eq!(named("glissando"), 0, "the one with room drawn");
}

/// A whole or half rest outside its staff, drawn with its ledger line
/// (`restWholeLegerLine`, `restHalfLegerLine`, X5d.5), is drawn at its
/// value: rests over a second voice's F5 stand above the staff, ledgered,
/// and no rest is counted at another value.
#[test]
fn a_ledgered_rest_is_drawn_at_its_value() {
    let voices =
        |upper: &str, lower: &str| format!("{upper}<backup><duration>4</duration></backup>{lower}");
    let rest = |kind: &str, d: u8| {
        format!("<note><rest/><duration>{d}</duration><voice>1</voice><type>{kind}</type></note>")
    };
    let note = |kind: &str, d: u8| {
        format!(
            "<note><pitch><step>F</step><octave>5</octave></pitch>\
             <duration>{d}</duration><voice>2</voice><type>{kind}</type></note>"
        )
    };
    // F5 at the bar's middle, where the whole rest stands.
    let m1 = voices(
        &rest("whole", 4),
        &format!("{}{}", note("half", 2), note("half", 2)),
    );
    let m2 = voices(
        &format!("{}{}", rest("half", 2), rest("half", 2)),
        &format!("{}{}", note("half", 2), note("half", 2)),
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">\
         <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type>\
         </time><clef><sign>G</sign><line>2</line></clef></attributes>{m1}</measure>\
         <measure number=\"2\">{m2}</measure></part></score-partwise>"
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ledgered_rests_counted.musicxml");
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    let engraved = engrave(&loaded.reduced.score);
    let named = |name: &str| {
        engraved
            .layout
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == name)
            .count()
    };
    assert_eq!(named("restWholeLegerLine"), 1, "the whole rest, ledgered");
    assert_eq!(named("restHalfLegerLine"), 2, "the half rests, ledgered");
    assert_eq!(count(&loaded, &engraved, "rest drawn at another value"), 0);
    assert_eq!(count(&loaded, &engraved, "rest not drawn"), 0);
}
