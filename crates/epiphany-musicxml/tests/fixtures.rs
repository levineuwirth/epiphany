//! The hand-written MusicXML fixtures, each taken through the outermost seam:
//! import, reduce, every operation's outcome, the invariants, the fidelity
//! comparison, and engraving. The expected values are written out by hand from
//! the files, so they check the reader's interpretation of MusicXML rather
//! than restating it.

use std::path::Path;

use epiphany_core::{
    check_invariants, AnchorOffset, Clef, ClefShape, Event, EventDuration, EventPosition,
    PitchSpacePosition, RationalTime, ScalePosition, Score, StaffId, TieClass, TimeAnchor,
    TimeSignatureDisplay, TranspositionInterval,
};
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{
    to_constrained, to_logical, written_view, ConstraintSolver, SolverConfig,
};
use epiphany_musicxml::fidelity::{compare, show, Fidelity};
use epiphany_musicxml::outcome::{reduce, Reduced, Verdict};
use epiphany_musicxml::source::{FeatureClass, QuarterTone};
use epiphany_musicxml::{import, Import, ReadError};

struct Run {
    import: Import,
    reduced: Reduced,
    fidelity: Fidelity,
}

fn xml(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Imports, reduces and compares a fixture, and holds it to the seam's
/// general promises: the invariants are clean, the comparison with the source
/// finds no unexplained difference, and the engraver lays it out.
fn run(name: &str) -> Run {
    let import = import(&xml(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let reduced = reduce(&import);
    let fidelity = compare(&import, &reduced);
    let violations = check_invariants(&reduced.score);
    assert!(violations.is_empty(), "{name}: {violations:?}");
    assert!(fidelity.passed(), "{name}: {:#?}", fidelity.failures);
    let constrained = to_constrained(&to_logical(&reduced.score));
    let report = Engraver::default().solve(&constrained, &SolverConfig::default());
    assert!(!report.layout.glyphs.is_empty(), "{name}: nothing engraved");
    Run {
        import,
        reduced,
        fidelity,
    }
}

fn all_applied(run: &Run) {
    for (i, verdict) in run.reduced.verdicts.iter().enumerate() {
        assert_eq!(
            verdict,
            &Verdict::Applied,
            "{} #{i} did not apply",
            run.import.labels[i].kind
        );
    }
}

fn rational(t: &RationalTime) -> String {
    show(t)
}

fn offset(anchor: &TimeAnchor) -> String {
    match anchor {
        TimeAnchor::Region {
            offset: AnchorOffset::Musical(d),
            ..
        } => rational(&d.0),
        TimeAnchor::Region {
            offset: AnchorOffset::Zero,
            ..
        } => String::from("0"),
        other => format!("{other:?}"),
    }
}

/// A pitch's name: `Bb4`, or for a quarter-tone in `cmn-24` its alteration
/// in signed quarter-tones, `G-1q4`.
fn pitch_name(scale: &ScalePosition) -> String {
    let PitchSpacePosition::Cmn {
        nominal,
        alteration,
        octave,
    } = &scale.position
    else {
        return format!("{scale:?}");
    };
    if scale.space.as_str() == "cmn-24" {
        return format!("{nominal:?}{alteration:+}q{octave}");
    }
    assert_eq!(scale.space.as_str(), "cmn-12");
    let accidental = match alteration {
        a if *a > 0 => "#".repeat(*a as usize),
        a => "b".repeat(a.unsigned_abs() as usize),
    };
    format!("{nominal:?}{accidental}{octave}")
}

fn clef_name(clef: &Clef) -> String {
    let shape = match clef.shape {
        ClefShape::G => "G",
        ClefShape::F => "F",
        ClefShape::C => "C",
        ClefShape::Percussion => "perc",
    };
    let shift = match clef.octave_shift {
        0 => String::new(),
        -1 => String::from(" 8vb"),
        1 => String::from(" 8va"),
        n => format!(" {n:+}"),
    };
    format!("{shape}{}{shift}", clef.line)
}

/// Every event, staff instance by staff instance and voice by voice, in the
/// score's own order: `"s<staff> v<voice> <onset> <duration> <content>"`.
fn events(score: &Score) -> Vec<String> {
    let mut out = Vec::new();
    for (s, instance) in score.canvas.regions[0].staff_instances().iter().enumerate() {
        for (v, voice) in instance.voices.iter().enumerate() {
            for id in &voice.events {
                let event = score.events.get(*id).expect("a listed event exists");
                let EventPosition::Musical(onset) = event.position() else {
                    panic!("a non-metric position")
                };
                let EventDuration::Musical(duration) = event.duration() else {
                    panic!("a non-metric duration")
                };
                let content = match event {
                    Event::Rest(r) if r.visible => String::from("rest"),
                    Event::Rest(_) => String::from("rest(hidden)"),
                    Event::Pitched(p) => p
                        .pitches
                        .iter()
                        .map(|ip| pitch_name(&ip.pitch.scale_position))
                        .collect::<Vec<_>>()
                        .join(" "),
                    Event::Unpitched(u) => {
                        format!("x{} m{}", u.staff_position.0, u.instrument_member.0)
                    }
                    other => format!("{other:?}"),
                };
                out.push(format!(
                    "s{s} v{v} {} {} {content}",
                    rational(&onset.0),
                    rational(&duration.0)
                ));
            }
        }
    }
    out
}

fn clefs(score: &Score) -> Vec<String> {
    let mut out = Vec::new();
    for (s, instance) in score.canvas.regions[0].staff_instances().iter().enumerate() {
        for change in &instance.clef_sequence {
            out.push(format!(
                "s{s} {} {}",
                offset(&change.anchor),
                clef_name(&change.clef)
            ));
        }
    }
    out
}

fn keys(score: &Score) -> Vec<String> {
    let mut out = Vec::new();
    for (s, instance) in score.canvas.regions[0].staff_instances().iter().enumerate() {
        for change in &instance.key_sequence {
            out.push(format!(
                "s{s} {} {}",
                offset(&change.anchor),
                change.key.fifths()
            ));
        }
    }
    out
}

fn meters(score: &Score) -> Vec<String> {
    let grid = score.canvas.regions[0]
        .content
        .staff_based()
        .and_then(|c| c.default_metric_grid.as_ref())
        .expect("a metric grid");
    grid.meter_sequence
        .iter()
        .map(|change| {
            let signature = score
                .time_signatures
                .iter()
                .find(|t| t.id == change.time_signature)
                .expect("the signature is live");
            let TimeSignatureDisplay::Standard {
                numerator,
                denominator,
            } = &signature.display
            else {
                panic!("a non-standard display")
            };
            format!(
                "{} {numerator}/{}",
                offset(&change.anchor),
                denominator.get()
            )
        })
        .collect()
}

fn measure_starts(score: &Score, staff: usize) -> Vec<String> {
    score.canvas.regions[0].staff_instances()[staff]
        .measures
        .iter()
        .map(|m| offset(&m.start))
        .collect()
}

fn ties(score: &Score) -> Vec<String> {
    let pitch_of = |event: &Event, id| match event {
        Event::Pitched(p) => p
            .pitches
            .iter()
            .find(|ip| ip.id == id)
            .map(|ip| pitch_name(&ip.pitch.scale_position))
            .unwrap_or_default(),
        _ => String::new(),
    };
    let onset = |event: &Event| match event.position() {
        EventPosition::Musical(p) => rational(&p.0),
        other => format!("{other:?}"),
    };
    score
        .cross_cutting
        .ties
        .iter()
        .flat_map(|tie| {
            let start = score.events.get(tie.start_event).expect("tie start");
            let end = score.events.get(tie.end_event).expect("tie end");
            assert_eq!(tie.class, TieClass::Standard);
            tie.pitch_pairing
                .clone()
                .expect("an explicit pairing")
                .into_iter()
                .map(move |(a, b)| {
                    format!(
                        "{} {} -> {} {}",
                        onset(start),
                        pitch_of(start, a),
                        onset(end),
                        pitch_of(end, b)
                    )
                })
        })
        .collect()
}

fn slurs(score: &Score) -> Vec<String> {
    let onset = |id| match score.events.get(id).map(Event::position) {
        Some(EventPosition::Musical(p)) => rational(&p.0),
        other => format!("{other:?}"),
    };
    score
        .cross_cutting
        .slurs
        .iter()
        .map(|slur| format!("{} -> {}", onset(slur.start_event), onset(slur.end_event)))
        .collect()
}

/// A quarter-tone as the census holds it: its measure, its offset there as
/// `(n, d)`, staff 0, and its sounding nominal (C is 0), alteration in
/// quarter-tones and octave.
fn quarter_tone(
    measure: usize,
    (n, d): (i64, i64),
    nominal: u8,
    quarter_tones: i16,
    octave: i8,
) -> QuarterTone {
    QuarterTone {
        measure,
        offset: RationalTime::new(n, d).expect("an offset"),
        staff: 0,
        nominal,
        quarter_tones,
        octave,
    }
}

fn interval(diatonic_steps: i32, chromatic_steps: i32) -> Option<TranspositionInterval> {
    Some(TranspositionInterval {
        diatonic_steps,
        chromatic_steps,
    })
}

#[test]
fn a_single_part_imports_its_notes_rests_key_meter_and_clef() {
    let run = run("single_part.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 3/8 D5",
            "s0 v0 3/8 1/8 C#5",
            "s0 v0 1/2 1/4 rest",
            "s0 v0 3/4 1/4 Bb4",
            "s0 v0 1 1/2 G4",
            "s0 v0 3/2 1/2 rest",
        ]
    );
    assert_eq!(score.instruments.len(), 1);
    assert_eq!(score.instruments[0].name, "Flute");
    assert_eq!(score.instruments[0].abbreviation.as_deref(), Some("Fl."));
    assert_eq!(score.staves.len(), 1);
    assert_eq!(score.staves[0].name, "Flute");
    assert_eq!(score.metadata.title.as_deref(), Some("Single part"));
    assert_eq!(score.metadata.composer.as_deref(), Some("Fixture"));
    assert_eq!(clefs(score), ["s0 0 G2"]);
    assert_eq!(keys(score), ["s0 0 1"]);
    assert_eq!(meters(score), ["0 4/4"]);
    assert_eq!(measure_starts(score, 0), ["0", "1"]);
    assert_eq!(run.fidelity.counts[0][0].notes, 3);
    assert_eq!(run.fidelity.counts[0][0].rests, 1);
    assert_eq!(run.fidelity.counts[0][1].notes, 1);
    assert_eq!(run.fidelity.counts[0][1].rests, 1);
}

#[test]
fn chords_voices_gaps_and_invisible_rests_import() {
    let run = run("chords_and_voices.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1/2 C4 E4 G4",
            "s0 v0 1/2 1/4 F4",
            "s0 v0 3/4 1/4 rest",
            "s0 v0 1 1 C5",
            "s0 v1 1/4 1/4 A3",
            "s0 v1 1/2 1/2 B3",
            "s0 v1 1 1 rest(hidden)",
        ]
    );
    let first = run.fidelity.counts[0][0];
    assert_eq!((first.notes, first.rests, first.chords), (6, 1, 1));
    assert_eq!((first.voices, first.staves), (2, 1));
}

#[test]
fn a_part_on_two_staves_shares_one_instrument() {
    let run = run("grand_staff.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 3/4 E5",
            "s1 v0 0 1/4 C3",
            "s1 v0 1/4 1/4 G3",
            "s1 v0 1/2 1/4 E3",
        ]
    );
    assert_eq!(score.instruments.len(), 1);
    assert_eq!(score.staves.len(), 2);
    assert!(score
        .staves
        .iter()
        .all(|s| s.instrument == score.instruments[0].id));
    assert_eq!(clefs(score), ["s0 0 G2", "s1 0 F4"]);
    assert_eq!(score.staves[1].default_clef, Clef::bass());
    assert_eq!(measure_starts(score, 1), ["0"]);
    assert_eq!(run.fidelity.counts[0][0].staves, 2);
}

/// A staff ranks its own voices first, in the file's numbering, and a voice
/// visiting from another staff after them, so that the engraver, which sets
/// a staff's first voice above and the next below, places the staff's own
/// voices as it would without the visitor. A number sounding on two staves
/// at once, as where a file numbers each staff's voices afresh, names a
/// voice on each and stays first on both; voice 10 follows voice 9.
#[test]
fn each_staff_ranks_its_own_voices_before_a_visiting_one() {
    let note = |step: &str, octave: u8, duration: u8, voice: &str, staff: u8| {
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>{duration}</duration><voice>{voice}</voice><staff>{staff}</staff></note>"
        )
    };
    let backup = "<backup><duration>4</duration></backup>";
    // Per staff, its voices by the file's number in the order the score
    // holds them, the primary starred.
    let ranked = |staves: u8, measures: &[String]| -> Vec<String> {
        let body: String = measures
            .iter()
            .enumerate()
            .map(|(m, content)| {
                let attributes = if m == 0 {
                    format!(
                        "<attributes><divisions>1</divisions><time><beats>4</beats>\
                         <beat-type>4</beat-type></time><staves>{staves}</staves></attributes>"
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
        let import = import(&xml).expect("imports");
        let reduced = reduce(&import);
        assert!(compare(&import, &reduced).passed());
        let names: std::collections::BTreeMap<_, _> = import
            .ids
            .voices
            .iter()
            .map(|((_, _, name), id)| (*id, name.clone()))
            .collect();
        let staves = &import.ids.staves[0];
        let mut lines = Vec::new();
        for region in &reduced.score.canvas.regions {
            for instance in region.staff_instances() {
                let staff = staves
                    .iter()
                    .position(|s| *s == instance.staff)
                    .expect("a staff");
                let voices: Vec<String> = instance
                    .voices
                    .iter()
                    .map(|v| format!("{}{}", names[&v.id], if v.is_primary { "*" } else { "" }))
                    .collect();
                lines.push(format!("s{}: {}", staff + 1, voices.join(" ")));
            }
        }
        lines.sort();
        lines
    };

    // The upper staff's voice 1 writes its third quarter on the lower staff,
    // whose own voices are 5 and 6; voice 5 writes one note on the upper.
    let visiting = [
        format!(
            "{}{backup}{}{}{}{}{backup}{}",
            note("C", 5, 4, "1", 1),
            note("C", 3, 1, "5", 2),
            note("D", 3, 1, "5", 2),
            note("E", 4, 1, "5", 1),
            note("F", 3, 1, "5", 2),
            note("C", 2, 4, "6", 2),
        ),
        format!(
            "{}{}{}{}{backup}{}",
            note("C", 5, 1, "1", 1),
            note("D", 5, 1, "1", 1),
            note("G", 3, 1, "1", 2),
            note("E", 5, 1, "1", 1),
            note("C", 3, 4, "5", 2),
        ),
    ];
    assert_eq!(ranked(2, &visiting), ["s1: 1* 5", "s2: 5* 6 1"]);

    // Each staff numbers its voices from 1: voice 1 sounds on both staves at
    // once, as many on one as on the other.
    let afresh = [
        format!(
            "{}{backup}{}{backup}{}",
            note("C", 5, 4, "1", 1),
            note("C", 3, 4, "1", 2),
            note("C", 2, 4, "2", 2),
        ),
        format!(
            "{}{backup}{}{backup}{}",
            note("D", 5, 4, "1", 1),
            note("D", 3, 4, "1", 2),
            note("D", 2, 4, "2", 2),
        ),
    ];
    assert_eq!(ranked(2, &afresh), ["s1: 1*", "s2: 1* 2"]);

    // Voice 2 writes as many notes on each staff, one after another: it is
    // at home on the upper.
    let even = [
        format!(
            "{}{}{}{backup}{}{backup}{}",
            note("C", 5, 2, "1", 1),
            note("E", 5, 1, "1", 1),
            note("D", 5, 1, "1", 1),
            note("C", 3, 4, "5", 2),
            note("A", 4, 2, "2", 1),
        ),
        format!(
            "{}{backup}{}{}",
            note("C", 5, 4, "1", 1),
            note("C", 3, 2, "5", 2),
            note("G", 3, 2, "2", 2),
        ),
    ];
    assert_eq!(ranked(2, &even), ["s1: 1* 2", "s2: 5* 2"]);

    // Voices 9 and 10 on one staff.
    let numbered = [format!(
        "{}{backup}{}",
        note("C", 5, 4, "9", 1),
        note("C", 4, 4, "10", 1),
    )];
    assert_eq!(ranked(1, &numbered), ["s1: 9* 10"]);
}

/// A voice visiting a staff from its home on another names that home
/// (schema major 5) and is drawn on its home's side, upper when visiting from
/// above; the staff's own voice beside it is drawn alone, as MuseScore draws
/// it (D46). The upper staff's voice 1 writes a quarter on the lower staff in
/// each measure: in the first where the lower staff's own voice 5 writes
/// nothing, in the second beside its whole note.
#[test]
fn a_visiting_voice_names_its_home_and_draws_on_its_side() {
    use epiphany_layout_ir::{LayoutContent, LayoutObject, VoicePlace};
    let note = |step: &str, octave: u8, duration: u8, voice: &str, staff: u8| {
        format!(
            "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch>\
             <duration>{duration}</duration><voice>{voice}</voice><staff>{staff}</staff></note>"
        )
    };
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
         </part-name></score-part></part-list><part id=\"P1\">\
         <measure number=\"1\"><attributes><divisions>1</divisions><time><beats>4</beats>\
         <beat-type>4</beat-type></time><staves>2</staves></attributes>\
         {}{}{}{}<backup><duration>4</duration></backup>{}<forward><duration>2</duration></forward>\
         </measure><measure number=\"2\">{}{}{}{}<backup><duration>4</duration></backup>{}</measure>\
         </part></score-partwise>",
        note("C", 5, 1, "1", 1),
        note("D", 5, 1, "1", 1),
        note("G", 3, 1, "1", 2),
        note("E", 5, 1, "1", 1),
        note("C", 3, 2, "5", 2),
        note("G", 3, 1, "1", 2),
        note("C", 5, 1, "1", 1),
        note("D", 5, 1, "1", 1),
        note("E", 5, 1, "1", 1),
        note("C", 3, 4, "5", 2),
    );
    let import = import(&xml).expect("imports");
    let reduced = reduce(&import);
    assert!(compare(&import, &reduced).passed());
    assert!(reduced.verdicts.iter().all(Verdict::applied));
    let score = &reduced.score;
    let staves = &import.ids.staves[0];
    let visitor = import.ids.voices[&(0, 1, String::from("1"))];
    let own = import.ids.voices[&(0, 1, String::from("5"))];
    assert_eq!(
        score.voice_homes.iter().collect::<Vec<_>>(),
        [(&visitor, &staves[0])],
        "only the visiting voice names a home"
    );
    let logical = to_logical(score);
    let place = |voice: epiphany_core::VoiceId| -> Vec<VoicePlace> {
        let events: Vec<epiphany_core::EventId> = score
            .voices()
            .find(|(_, _, v)| v.id == voice)
            .map(|(_, _, v)| v.events.clone())
            .expect("the voice");
        events
            .iter()
            .map(|id| {
                logical
                    .regions
                    .iter()
                    .flat_map(|r| &r.objects)
                    .find_map(|object| match object {
                        LayoutObject::Note(c) | LayoutObject::Rest(c)
                            if c.provenance.source == epiphany_core::TypedObjectId::Event(*id) =>
                        {
                            match &c.content {
                                LayoutContent::Note(n) => Some(n.voice),
                                LayoutContent::Rest(r) => Some(r.voice),
                                _ => None,
                            }
                        }
                        _ => None,
                    })
                    .expect("laid out")
            })
            .collect()
    };
    // The visitor stands above, alone or beside the staff's own voice.
    assert_eq!(place(visitor), [VoicePlace::Upper, VoicePlace::Upper]);
    // The staff's own voice is alone beside it.
    assert_eq!(place(own), [VoicePlace::Alone, VoicePlace::Alone]);
}

#[test]
fn a_key_written_before_the_staves_reaches_each_staff_it_names() {
    let run = run("keyed_grand_staves.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1 D5",
            "s0 v0 1 1 G4",
            "s1 v0 0 1 Bb2",
            "s1 v0 1 1 G2",
            "s2 v0 0 1 E5",
            "s2 v0 1 1 rest",
            "s3 v0 0 1 F3",
            "s3 v0 1 1 rest",
        ]
    );
    assert_eq!(
        keys(score),
        ["s0 0 -2", "s0 1 1", "s1 0 -2", "s1 1 1", "s2 0 1", "s3 0 -1"]
    );
    assert_eq!(clefs(score), ["s0 0 G2", "s1 0 F4", "s2 0 G2", "s3 0 F4"]);
    // The census's own reading: each key by measure index and offset.
    let census = &run.import.source.census;
    let stated = |part: usize| -> Vec<Vec<String>> {
        census[part]
            .keys
            .iter()
            .map(|staff| {
                staff
                    .iter()
                    .map(|k| format!("m{} +{} {}", k.measure, show(&k.offset), k.value))
                    .collect()
            })
            .collect()
    };
    assert_eq!(
        stated(0),
        [["m0 +0 -2", "m1 +0 1"], ["m0 +0 -2", "m1 +0 1"]]
    );
    assert_eq!(stated(1), [["m0 +0 1"], ["m0 +0 -1"]]);
    let lacking = &run.import.source.features.kinds["key for a staff the part lacks"];
    assert_eq!(lacking.places.len(), 1);
    assert_eq!(lacking.class, FeatureClass::Content);
}

#[test]
fn ties_pair_their_pitches_and_slurs_their_events() {
    let run = run("ties_and_slurs.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1/4 C5",
            "s0 v0 1/4 1/4 D5",
            "s0 v0 1/2 1/4 E5",
            "s0 v0 3/4 1/4 G4 B4",
            "s0 v0 1 1/4 G4 C5",
            "s0 v0 5/4 1/4 F5",
            "s0 v0 3/2 1/4 F5",
            "s0 v0 7/4 1/4 rest",
            "s0 v0 2 1/2 E5",
            "s0 v0 5/2 1/4 E5",
            "s0 v0 11/4 1/4 E5",
        ]
    );
    assert_eq!(
        ties(score),
        [
            "3/4 G4 -> 1 G4",
            "5/4 F5 -> 3/2 F5",
            "2 E5 -> 5/2 E5",
            "5/2 E5 -> 11/4 E5"
        ]
    );
    assert_eq!(slurs(score), ["0 -> 1/2", "1 -> 3/2"]);
    // The middle of the chain writes `<tie type="stop"/>` before its start.
    assert_eq!(run.import.source.census[0].tie_starts, 4);
}

#[test]
fn meter_key_and_clef_changes_land_where_the_file_puts_them() {
    let run = run("changes.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 3/4 D3",
            "s0 v0 3/4 1/4 G3",
            "s0 v0 1 1/4 A4",
            "s0 v0 5/4 1/2 E5",
        ]
    );
    assert_eq!(meters(score), ["0 3/4", "3/4 2/4"]);
    assert_eq!(keys(score), ["s0 0 -2", "s0 3/4 1"]);
    assert_eq!(clefs(score), ["s0 0 F4", "s0 1 C4", "s0 5/4 G2"]);
    assert_eq!(measure_starts(score, 0), ["0", "3/4", "5/4"]);
    // The measures where a meter takes effect name it; the rest inherit.
    let measures = &score.canvas.regions[0].staff_instances()[0].measures;
    assert!(measures[0].time_signature.is_some());
    assert!(measures[1].time_signature.is_some());
    assert_eq!(measures[2].time_signature, None);
    assert_ne!(measures[0].time_signature, measures[1].time_signature);
}

#[test]
fn a_concert_score_keeps_sounding_pitches_and_each_part_its_interval() {
    let run = run("concert_transposing.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        ["s0 v0 0 1 C5", "s1 v0 0 1 G3", "s2 v0 0 1 E2"]
    );
    let transpositions: Vec<_> = score.instruments.iter().map(|i| i.transposition).collect();
    assert_eq!(
        transpositions,
        [interval(-1, -2), interval(-8, -14), interval(-7, -12)]
    );
    assert_eq!(clefs(score), ["s0 0 G2", "s1 0 G2 8vb", "s2 0 F4 8vb"]);
    // The written pitch follows from the sounding one and the interval: the
    // clarinet's concert C5 is written D5, the bass clarinet's G3 is A4.
    let written = |staff: usize, part: usize| {
        let instance = &score.canvas.regions[0].staff_instances()[staff];
        let Some(Event::Pitched(p)) = score.events.get(instance.voices[0].events[0]) else {
            panic!("a pitched event")
        };
        let inverse = score.instruments[part]
            .transposition
            .and_then(TranspositionInterval::inverse)
            .expect("an invertible interval");
        pitch_name(
            &p.pitches[0]
                .pitch
                .transposed(inverse)
                .expect("a CMN pitch transposes")
                .scale_position,
        )
    };
    assert_eq!(written(0, 0), "D5");
    assert_eq!(written(1, 1), "A4");
    assert_eq!(written(2, 2), "E3");
}

#[test]
fn a_transposed_score_imports_the_sounding_pitch() {
    let run = run("written_transposing.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        ["s0 v0 0 1/4 C5", "s0 v0 1/4 1/4 E5", "s0 v0 1/2 1/2 Bb4"]
    );
    assert_eq!(score.instruments[0].transposition, interval(-1, -2));
}

/// A transposed score's keys are written, as its pitches are, and both are
/// held at concert pitch: the clarinet's two sharps and three are the keys of
/// C and G. The horn's open key is no key signature, which no transposition
/// moves, and the flute's are as written. The written view gives back the
/// keys and pitches the file writes.
#[test]
fn a_transposed_scores_keys_are_held_at_concert_pitch_and_drawn_written() {
    let run = run("written_keys.musicxml");
    all_applied(&run);
    assert!(!run.import.source.concert);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1 D5",
            "s0 v0 1 1 F#5",
            "s1 v0 0 1 C4",
            "s1 v0 1 1 F4",
            "s2 v0 0 1 D5",
            "s2 v0 1 1 F#5"
        ]
    );
    assert_eq!(keys(score), ["s0 0 0", "s0 1 1", "s2 0 0", "s2 1 1"]);
    let view = written_view(score);
    assert_eq!(keys(&view), ["s0 0 2", "s0 1 3", "s2 0 0", "s2 1 1"]);
    assert_eq!(
        events(&view),
        [
            "s0 v0 0 1 E5",
            "s0 v0 1 1 G#5",
            "s1 v0 0 1 G4",
            "s1 v0 1 1 C5",
            "s2 v0 0 1 D5",
            "s2 v0 1 1 F#5"
        ]
    );
}

/// A quarter-tone of a transposing part is drawn at its written pitch with
/// its written spelling: the clarinet's sounding C quarter-sharp, spelt so,
/// is written D quarter-sharp, as the file writes it; the flute's are
/// unchanged.
#[test]
fn a_transposed_parts_quarter_tone_is_drawn_with_its_written_spelling() {
    let run = run("quarter_tones.musicxml");
    assert!(!run.import.source.concert);
    let view = written_view(&run.reduced.score);
    assert_eq!(
        spelt(&view),
        [
            "0 G-1q4 Cmn(G) quarter-flat 4",
            "1/4 G-1q4 Cmn(G) quarter-flat 4",
            "1/2 C+3q5 Cmn(C) three-quarters-sharp 5",
            "2 E-1q5 Cmn(E) quarter-flat 5",
            "2 G+1q5 Cmn(G) quarter-sharp 5",
            "0 D+1q5 Cmn(D) quarter-sharp 5",
        ]
    );
}

#[test]
fn unpitched_notes_keep_their_staff_step_and_instrument_member() {
    let run = run("percussion.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1/4 x5 m0",
            "s0 v0 1/4 1/4 x1 m1",
            "s0 v0 1/2 1/2 rest"
        ]
    );
    let members: Vec<(String, i16, u32)> = score.instruments[0]
        .unpitched_members
        .iter()
        .map(|m| (m.name.clone(), m.staff_position.0, m.member.0))
        .collect();
    assert_eq!(
        members,
        [
            (String::from("Snare Drum"), 5, 0),
            (String::from("Bass Drum"), 1, 1)
        ]
    );
    assert_eq!(clefs(score), ["s0 0 perc3"]);
    assert_eq!(score.staves[0].default_staff_lines.line_count, 1);
}

#[test]
fn a_quarter_tone_imports_at_its_pitch_in_cmn_24() {
    let run = run("quarter_tones.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1/4 G-1q4",
            "s0 v0 1/4 1/4 G-1q4",
            "s0 v0 1/2 1/4 C+3q5",
            "s0 v0 3/4 1/4 A4",
            "s0 v0 1 1/2 A4",
            "s0 v0 3/2 1/2 rest",
            "s0 v0 2 1 C5 E-1q5 G+1q5",
            "s1 v0 0 1 C+1q5",
            "s1 v0 1 1 rest",
            "s1 v0 2 1 rest",
        ]
    );
    assert_eq!(score.instruments[1].transposition, interval(-1, -2));
    // The census reads each quarter-tone and transposes it to sounding pitch
    // apart from the reader.
    let census = &run.import.source.census;
    assert_eq!(
        census[0].quarter_tones,
        [
            quarter_tone(0, (0, 1), 4, -1, 4),
            quarter_tone(0, (1, 4), 4, -1, 4),
            quarter_tone(0, (1, 2), 0, 3, 5),
            quarter_tone(1, (1, 2), 2, -1, 5),
            quarter_tone(2, (0, 1), 2, -1, 5),
            quarter_tone(2, (0, 1), 4, 1, 5),
        ]
    );
    assert_eq!(census[1].quarter_tones, [quarter_tone(0, (0, 1), 0, 1, 5)]);
    // The E joining a rest is dropped, and its quarter-tone is accounted for
    // where it falls, at the rest's onset.
    let flute = &run.import.source.parts[0];
    assert_eq!(flute.dropped_notes, 1);
    assert_eq!(census[0].dropped.pitched, 1);
    assert_eq!(
        flute.dropped_quarter_tones,
        [quarter_tone(1, (1, 2), 2, -1, 5)]
    );
    // A tie's pitches must be equal in their space's chromatic layer, which
    // the core decides in `cmn-24` as in `cmn-12`: the quarter-tones' tie is
    // made, as is the ordinary tie beside it, and none is recorded.
    assert_eq!(ties(score), ["0 G-1q4 -> 1/4 G-1q4", "3/4 A4 -> 1 A4"]);
    let kinds = &run.import.source.features.kinds;
    assert!(!kinds.keys().any(|k| k.starts_with("tie")), "{kinds:?}");
}

#[test]
fn a_quarter_tone_accidental_named_without_an_alter_applies_to_its_note_and_its_ties() {
    let run = run("arrow_accidentals.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            // Each name once.
            "s0 v0 0 1/4 G-1q4",
            "s0 v0 1/4 1/4 A-3q4",
            "s0 v0 1/2 1/4 B+1q4",
            "s0 v0 3/4 1/4 C-1q5",
            "s0 v0 1 1/4 D+3q5",
            "s0 v0 5/4 1/4 E+1q5",
            "s0 v0 3/2 1/4 F+5q5",
            "s0 v0 7/4 1/4 G+3q5",
            "s0 v0 2 1/4 A-3q5",
            "s0 v0 9/4 1/4 B-5q5",
            "s0 v0 5/2 1/4 C-1q4",
            "s0 v0 11/4 1/4 D+1q4",
            "s0 v0 3 1/4 E-3q4",
            "s0 v0 13/4 1/4 F+3q4",
            "s0 v0 7/2 1/2 rest",
            // The second voice's B three-quarter-flat at 17/4 does not reach
            // this B.
            "s0 v0 4 1/2 rest",
            "s0 v0 9/2 1/4 B4",
            "s0 v0 19/4 1/4 D5",
            // Over a tie, to the tied note alone, and along a chain.
            "s0 v0 5 1/4 C5",
            "s0 v0 21/4 1/2 rest",
            "s0 v0 23/4 1/4 B-1q4",
            "s0 v0 6 1/4 B-1q4",
            "s0 v0 25/4 1/4 B4",
            "s0 v0 13/2 1/4 rest",
            "s0 v0 27/4 1/4 A+1q4",
            "s0 v0 7 1 A+1q4",
            "s0 v0 8 1/2 A+1q4",
            "s0 v0 17/2 1/2 rest",
            // Two voices tie the same written D: each keeps its own.
            "s0 v0 9 1 D5",
            "s0 v0 10 1/2 D5",
            "s0 v0 21/2 1/2 rest",
            // Neither of two accidentals before it reaches a note that writes
            // none.
            "s0 v0 11 1/4 B-1q4",
            "s0 v0 45/4 1/4 B+3q4",
            "s0 v0 23/2 1/4 B4",
            "s0 v0 47/4 1/4 rest",
            // Not before the accidental, not at another octave, not past a
            // natural.
            "s0 v1 4 1/4 B4",
            "s0 v1 17/4 1/4 B-3q4",
            "s0 v1 9/2 1/4 B3",
            "s0 v1 19/4 1/4 B4",
            "s0 v1 9 1/2 rest",
            "s0 v1 19/2 1/2 D+1q5",
            "s0 v1 10 1/2 D+1q5",
            "s0 v1 21/2 1/2 rest",
            "s1 v0 0 1 rest",
            "s1 v0 1 1 rest",
            "s1 v0 2 1 rest",
            "s1 v0 3 1 rest",
            // Not on the other staff.
            "s1 v0 4 1/2 rest",
            "s1 v0 9/2 1/4 B4",
            "s1 v0 19/4 1/4 rest",
            "s1 v0 5 1 rest",
            "s1 v0 6 1 rest",
            "s1 v0 7 1 rest",
            "s1 v0 8 1 rest",
            "s1 v0 9 1 rest",
            "s1 v0 10 1 rest",
            "s1 v0 11 1 rest",
        ]
    );
    // The census values each name and tie apart from the reader, and in
    // measures 5 and 12 finds only the notes that write an accidental.
    let census = &run.import.source.census[0].quarter_tones;
    assert_eq!(census.len(), 24);
    let in_measure = |m: usize| {
        census
            .iter()
            .filter(|q| q.measure == m)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(in_measure(4), [quarter_tone(4, (1, 4), 6, -3, 4)]);
    assert_eq!(
        in_measure(11),
        [
            quarter_tone(11, (0, 1), 6, -1, 4),
            quarter_tone(11, (1, 4), 6, 3, 4),
        ]
    );
    // Every tie is made, the quarter-tones' among them: over a barline, along
    // a chain, and the second voice's beside the first's natural D.
    assert_eq!(
        ties(score),
        [
            "23/4 B-1q4 -> 6 B-1q4",
            "27/4 A+1q4 -> 7 A+1q4",
            "7 A+1q4 -> 8 A+1q4",
            "9 D5 -> 10 D5",
            "19/2 D+1q5 -> 10 D+1q5",
        ]
    );
    let kinds = &run.import.source.features.kinds;
    assert!(!kinds.keys().any(|k| k.starts_with("tie")), "{kinds:?}");
}

/// Each quarter-tone of `score` with the spelling authored for it, in the
/// order [`events`] lists them: its onset, its pitch, and its spelling's
/// letter, accidentals and octave, or `unspelt`.
fn spelt(score: &Score) -> Vec<String> {
    let spellings: std::collections::BTreeMap<_, _> = score
        .spelling_attachments
        .iter()
        .filter_map(|a| match (&a.scope, &a.directive) {
            (
                epiphany_core::SpellingScope::Pitch(pitch),
                epiphany_core::SpellingDirective::Explicit(spelling),
            ) => Some((*pitch, spelling)),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for instance in score.canvas.regions[0].staff_instances() {
        for voice in &instance.voices {
            for id in &voice.events {
                let Some(Event::Pitched(event)) = score.events.get(*id) else {
                    continue;
                };
                let EventPosition::Musical(onset) = &event.position else {
                    panic!("a non-metric position")
                };
                for ip in &event.pitches {
                    if ip.pitch.scale_position.space.as_str() != "cmn-24" {
                        continue;
                    }
                    let spelling = match spellings.get(&ip.id) {
                        Some(sp) => format!(
                            "{:?} {} {}",
                            sp.nominal,
                            sp.accidentals
                                .iter()
                                .map(|a| a.as_str())
                                .collect::<Vec<_>>()
                                .join("+"),
                            sp.octave
                        ),
                        None => String::from("unspelt"),
                    };
                    out.push(format!(
                        "{} {} {spelling}",
                        rational(&onset.0),
                        pitch_name(&ip.pitch.scale_position)
                    ));
                }
            }
        }
    }
    out
}

/// A quarter-tone is spelt with the accidental its notation gives it, its
/// own or the one a tie carries to it, at its letter and octave, since the
/// spelling pre-pass spells no `cmn-24` pitch: the fourteen names each as
/// written, one carried over a tie and along a chain, and in a transposed
/// part the sounding pitch's accidental of the written one's kind.
#[test]
fn a_quarter_tone_is_spelt_with_the_accidental_its_notation_gives_it() {
    let arrows = run("arrow_accidentals.musicxml");
    assert_eq!(
        spelt(&arrows.reduced.score),
        [
            "0 G-1q4 Cmn(G) flat-up 4",
            "1/4 A-3q4 Cmn(A) flat-down 4",
            "1/2 B+1q4 Cmn(B) natural-up 4",
            "3/4 C-1q5 Cmn(C) natural-down 5",
            "1 D+3q5 Cmn(D) sharp-up 5",
            "5/4 E+1q5 Cmn(E) sharp-down 5",
            "3/2 F+5q5 Cmn(F) double-sharp-up 5",
            "7/4 G+3q5 Cmn(G) double-sharp-down 5",
            "2 A-3q5 Cmn(A) flat-flat-up 5",
            "9/4 B-5q5 Cmn(B) flat-flat-down 5",
            "5/2 C-1q4 Cmn(C) quarter-flat 4",
            "11/4 D+1q4 Cmn(D) quarter-sharp 4",
            "3 E-3q4 Cmn(E) three-quarters-flat 4",
            "13/4 F+3q4 Cmn(F) three-quarters-sharp 4",
            // Over a tie, and along a chain.
            "23/4 B-1q4 Cmn(B) flat-up 4",
            "6 B-1q4 Cmn(B) flat-up 4",
            "27/4 A+1q4 Cmn(A) natural-up 4",
            "7 A+1q4 Cmn(A) natural-up 4",
            "8 A+1q4 Cmn(A) natural-up 4",
            // Each on its own note.
            "11 B-1q4 Cmn(B) flat-up 4",
            "45/4 B+3q4 Cmn(B) sharp-up 4",
            "17/4 B-3q4 Cmn(B) flat-down 4",
            "19/2 D+1q5 Cmn(D) natural-up 5",
            "10 D+1q5 Cmn(D) natural-up 5",
        ]
    );
    // A fractional `<alter>` with no accidental of its own, tied from one
    // with Stein's, takes Stein's; each quarter-tone of a chord is spelt,
    // not only its first note; the clarinet's written D quarter-sharp sounds
    // C quarter-sharp, spelt so.
    let stein = run("quarter_tones.musicxml");
    assert_eq!(
        spelt(&stein.reduced.score),
        [
            "0 G-1q4 Cmn(G) quarter-flat 4",
            "1/4 G-1q4 Cmn(G) quarter-flat 4",
            "1/2 C+3q5 Cmn(C) three-quarters-sharp 5",
            // The chord's second and third notes, above a natural C.
            "2 E-1q5 Cmn(E) quarter-flat 5",
            "2 G+1q5 Cmn(G) quarter-sharp 5",
            "0 C+1q5 Cmn(C) quarter-sharp 5",
        ]
    );
}

/// The comparison holds every quarter-tone's spelling to its pitch: one
/// spelt with an accidental of another value, at another octave, or not at
/// all, fails it.
#[test]
fn the_comparison_finds_a_quarter_tone_spelt_other_than_it_sounds() {
    let run = run("arrow_accidentals.musicxml");
    let fails = |change: &dyn Fn(&mut epiphany_core::PitchSpelling)| {
        let mut reduced = run.reduced.clone();
        let attachment = reduced
            .score
            .spelling_attachments
            .first_mut()
            .expect("a quarter-tone's spelling");
        let epiphany_core::SpellingDirective::Explicit(spelling) = &mut attachment.directive else {
            panic!("an explicit spelling")
        };
        change(spelling);
        compare(&run.import, &reduced)
            .failures
            .iter()
            .any(|f| f.contains("not spelt as they sound"))
    };
    assert!(!fails(&|_| {}), "the import's own spellings agree");
    // The first is G flat-up; natural-down is the same pitch, flat-down not.
    assert!(!fails(&|s| {
        s.accidentals = vec![epiphany_core::AccidentalId::new("natural-down")]
    }));
    assert!(fails(&|s| {
        s.accidentals = vec![epiphany_core::AccidentalId::new("flat-down")]
    }));
    assert!(fails(&|s| s.octave += 1));
    assert!(fails(&|s| s.accidentals.clear()));
    let mut unspelt = run.reduced.clone();
    unspelt.score.spelling_attachments.clear();
    assert!(compare(&run.import, &unspelt)
        .failures
        .iter()
        .any(|f| f.starts_with("24 of 24 quarter-tones")));
}

/// The accidental of a sounding quarter-tone keeps the written one's kind:
/// an arrow its direction, moving the accidental under it with a
/// transposition, and Stein's its family, taking an arrow where Stein has
/// none for the alteration.
#[test]
fn a_quarter_tones_accidental_keeps_its_kind_at_the_sounding_pitch() {
    use epiphany_musicxml::source::quarter_tone_accidental as accidental;
    let arrowed = [
        ("flat-flat-down", -5),
        ("flat-flat-up", -3),
        ("flat-down", -3),
        ("flat-up", -1),
        ("natural-down", -1),
        ("natural-up", 1),
        ("sharp-down", 1),
        ("sharp-up", 3),
        ("double-sharp-down", 3),
        ("double-sharp-up", 5),
    ];
    let stein = [
        ("three-quarters-flat", -3),
        ("quarter-flat", -1),
        ("quarter-sharp", 1),
        ("three-quarters-sharp", 3),
    ];
    for (name, quarter_tones) in arrowed.iter().chain(&stein) {
        assert_eq!(accidental(Some(name), *quarter_tones), Some(*name));
    }
    // A written F natural-up for a B-flat instrument sounds E flat-up.
    assert_eq!(accidental(Some("natural-up"), -1), Some("flat-up"));
    assert_eq!(accidental(Some("sharp-down"), -1), Some("natural-down"));
    assert_eq!(accidental(Some("flat-down"), 1), Some("sharp-down"));
    // Past a double accidental, the other arrow; past any, none.
    assert_eq!(accidental(Some("flat-up"), -5), Some("flat-flat-down"));
    assert_eq!(accidental(Some("sharp-down"), 5), Some("double-sharp-up"));
    assert_eq!(accidental(Some("flat-flat-down"), -7), None);
    // Stein past three quarter-tones takes an arrow, up first.
    assert_eq!(
        accidental(Some("quarter-sharp"), 5),
        Some("double-sharp-up")
    );
    assert_eq!(accidental(Some("quarter-flat"), -5), Some("flat-flat-down"));
    // No accidental of its own: Stein's.
    assert_eq!(accidental(None, -1), Some("quarter-flat"));
    assert_eq!(accidental(None, 3), Some("three-quarters-sharp"));
}

/// MuseScore marks every accidental its user sets `cautionary`, every
/// quarter-tone accidental among them. On a quarter-tone accidental, which
/// spells its note, the mark alone records no feature; on a standard
/// accidental it records a cautionary accidental, and parentheses record one
/// on either. The marks change no pitch.
#[test]
fn the_cautionary_mark_on_a_quarter_tone_accidental_records_nothing() {
    let plain = xml("arrow_accidentals.musicxml");
    let marked = plain
        .replace(
            "<accidental>flat-up</accidental>",
            r#"<accidental cautionary="yes" parentheses="no">flat-up</accidental>"#,
        )
        .replace(
            "<accidental>natural</accidental>",
            r#"<accidental cautionary="yes" parentheses="no">natural</accidental>"#,
        )
        .replace(
            "<accidental>sharp-up</accidental>",
            r#"<accidental cautionary="yes" parentheses="yes">sharp-up</accidental>"#,
        );
    assert_eq!(marked.matches("cautionary=").count(), 6, "six marked");
    let imported = import(&marked).expect("imports");
    // The three flat-ups record nothing; the natural (measure 5) and the two
    // parenthesised sharp-ups (measures 2 and 12) each record one.
    let places: Vec<&str> = imported
        .source
        .features
        .kinds
        .get("cautionary accidental")
        .map(|f| f.places.iter().map(|p| p.measure.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(places, ["2", "5", "12"]);
    let plain = import(&plain).expect("imports");
    assert_eq!(
        events(&reduce(&imported).score),
        events(&reduce(&plain).score)
    );
}

#[test]
fn an_accidental_with_no_alter_and_no_known_alteration_is_refused_by_name() {
    let text = xml("eighth_tone.musicxml")
        .replace("<alter>0.25</alter>", "")
        .replace(
            "<type>whole</type>",
            "<type>whole</type><accidental>koron</accidental>",
        );
    match import(&text) {
        Err(ReadError::Unsupported(_, what)) => assert!(what.contains("koron"), "{what}"),
        other => panic!(
            "expected a named refusal, got {:?}",
            other.map(|i| i.envelopes.len())
        ),
    }
}

#[test]
fn a_pitch_finer_than_a_quarter_tone_is_refused_by_name() {
    match import(&xml("eighth_tone.musicxml")) {
        Err(ReadError::Unsupported(_, what)) => assert!(what.contains("alter 0.25"), "{what}"),
        other => panic!(
            "expected a named refusal, got {:?}",
            other.map(|i| i.envelopes.len())
        ),
    }
}

#[test]
fn an_unpitched_note_ties_to_the_next_of_its_member_at_its_step() {
    let run = run("percussion_ties.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1/2 x5 m0",
            "s0 v0 1/2 1/2 x5 m0",
            "s0 v0 1 1 x10 m1",
            "s0 v0 2 1/2 x10 m1",
            "s0 v0 5/2 1/2 x5 m0",
            "s0 v0 3 1 x10 m1",
        ]
    );
    let onset = |id| match score.events.get(id).map(Event::position) {
        Some(EventPosition::Musical(p)) => rational(&p.0),
        other => format!("{other:?}"),
    };
    let ties: Vec<String> = score
        .cross_cutting
        .ties
        .iter()
        .map(|t| {
            format!(
                "{} -> {} {:?} {:?}",
                onset(t.start_event),
                onset(t.end_event),
                t.class,
                t.pitch_pairing
            )
        })
        .collect();
    assert_eq!(ties, ["0 -> 1/2 Standard None", "1 -> 2 Standard None"]);
    // Four tie starts in the file: two tied, one recorded for want of an
    // end, one on the snare drum's chord note, which is dropped.
    let source = &run.import.source;
    assert_eq!(source.census[0].tie_starts, 4);
    assert_eq!(run.import.unended_ties, [1]);
    assert_eq!(source.parts[0].dropped_tie_starts, 1);
    assert_eq!(
        source.features.kinds["tie without a matching end"]
            .places
            .len(),
        1
    );
}

#[test]
fn tuplets_import_with_their_ratios_and_members() {
    let run = run("tuplet.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    let onsets: std::collections::BTreeMap<_, _> = score
        .events
        .iter()
        .map(|event| match event.position() {
            EventPosition::Musical(at) => (event.id(), rational(&at.0)),
            other => panic!("a metric event, not {other:?}"),
        })
        .collect();
    let mut tuplets: Vec<(u32, u32, Vec<String>, String)> = score
        .cross_cutting
        .tuplets
        .iter()
        .map(|tuplet| {
            (
                tuplet.ratio.actual(),
                tuplet.ratio.notated(),
                tuplet.members.iter().map(|m| onsets[m].clone()).collect(),
                rational(&tuplet.required_total.0),
            )
        })
        .collect();
    tuplets.sort();
    let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        tuplets,
        [
            // The triplet, the one with another begun inside it, and the
            // second voice's beside a quarter.
            (3, 2, strings(&["0", "1/12", "1/6"]), String::from("1/4")),
            (
                3,
                2,
                strings(&["1", "7/6", "4/3", "11/8", "17/12"]),
                String::from("1/2")
            ),
            (3, 2, strings(&["3/4", "5/6", "11/12"]), String::from("1/4")),
            // The sextuplet: its rest a member, its chord one.
            (
                6,
                4,
                strings(&["1/4", "7/24", "1/3", "3/8", "5/12", "11/24"]),
                String::from("1/4")
            ),
        ]
    );
    for tuplet in &score.cross_cutting.tuplets {
        assert!(tuplet.parent.is_none());
    }
    // The triplet with another begun inside it is made, its members its own
    // and the inner one's; the inner one, and the one never stopped, are
    // recorded and not made.
    let part = &run.import.source.parts[0];
    assert_eq!(part.tuplets.len(), 4, "{:?}", part.tuplets);
    assert_eq!(part.unmade_tuplets, 2);
    assert_eq!(part.tuplets[3].events.len(), 5);
    let kinds = &run.import.source.features.kinds;
    assert_eq!(kinds["tuplet 12:8 inside another"].places.len(), 1);
    assert_eq!(kinds["tuplet without a stop"].places.len(), 1);
    assert!(!kinds
        .keys()
        .any(|k| k.starts_with("tuplet 3:2") || k.starts_with("tuplet 6:4")));
    let census = &run.import.source.census[0];
    assert_eq!(census.tuplets.values().sum::<usize>(), 4);
    assert_eq!(census.unmade_tuplets, 2);
}

/// The comparison holds the reader's tuplets to the census's own timed walk of
/// the file, member by member: where the census places one member elsewhere,
/// or one fewer, the comparison fails, though every count and ratio agrees
/// and the score holds exactly what the reader made.
#[test]
fn the_comparison_holds_each_tuplets_members_to_the_census() {
    let run = run("tuplet.musicxml");
    let fails = |census: &dyn Fn(&mut epiphany_musicxml::source::CensusTuplet)| {
        let mut import = run.import.clone();
        let tuplet = import.source.census[0]
            .tuplet_places
            .first_mut()
            .expect("the census places the file's tuplets");
        census(tuplet);
        compare(&import, &run.reduced)
            .failures
            .iter()
            .any(|f| f.contains("the reader's tuplets are not the file's"))
    };
    assert!(!fails(&|_| {}), "the file's own census agrees");
    assert!(fails(&|t| {
        t.members.pop();
    }));
    assert!(fails(&|t| {
        let (measure, offset) = t.members[1].clone();
        t.members[1] = (
            measure,
            offset.add(&epiphany_core::RationalTime::new(1, 64).unwrap()),
        );
    }));
    assert!(fails(&|t| t.staff += 1));
}

/// A tuplet the file hides (its start mark's `<notations>` not printed, as
/// MuseScore writes a tuplet it hides) imports hidden, with no number and no
/// bracket; one whose mark asks `show-number="none"` imports without a
/// number; every other as any tuplet is drawn. The census reads each mark
/// apart from the reader, and the comparison holds them together.
#[test]
fn a_tuplet_the_file_hides_imports_hidden() {
    use epiphany_core::{TupletBracket, TupletDisplay, TupletNumber};
    let run = run("cross_staff.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    let onset = |id| match score.events.get(id).map(Event::position) {
        Some(EventPosition::Musical(p)) => rational(&p.0),
        other => format!("{other:?}"),
    };
    let mut displays: Vec<(String, TupletDisplay)> = score
        .cross_cutting
        .tuplets
        .iter()
        .map(|t| (onset(t.members[0]), t.display))
        .collect();
    displays.sort_by_key(|(at, _)| (at.len(), at.clone()));
    let shown = TupletDisplay::default();
    let hidden = TupletDisplay::HIDDEN;
    assert_eq!(
        displays,
        [
            (String::from("0"), shown),
            (String::from("2"), hidden),
            (String::from("1/2"), shown),
            (String::from("1/4"), hidden),
            (String::from("3/4"), shown),
            (String::from("9/4"), shown),
        ]
    );
    let census = &run.import.source.census[0];
    assert_eq!(census.tuplet_places.iter().filter(|t| t.hidden).count(), 2);

    // A mark asking for no number, its notations printed.
    let xml = xml("tuplet.musicxml").replacen(
        r#"<tuplet type="start""#,
        r#"<tuplet show-number="none" type="start""#,
        1,
    );
    assert_ne!(xml, self::xml("tuplet.musicxml"), "the fixture has a mark");
    let numberless = import(&xml).expect("imports");
    let reduced = reduce(&numberless);
    assert!(compare(&numberless, &reduced).passed());
    let marked: Vec<TupletDisplay> = reduced
        .score
        .cross_cutting
        .tuplets
        .iter()
        .map(|t| t.display)
        .filter(|d| *d != shown)
        .collect();
    assert_eq!(
        marked,
        [TupletDisplay {
            number: TupletNumber::None,
            bracket: TupletBracket::Auto,
        }]
    );
}

/// The comparison holds each tuplet's display to the census's own reading
/// of its start mark: where the census reads a hidden tuplet as shown, or a
/// shown one as numberless, the comparison fails.
#[test]
fn the_comparison_holds_each_tuplets_display_to_the_census() {
    let run = run("cross_staff.musicxml");
    let fails = |census: &dyn Fn(&mut Vec<epiphany_musicxml::source::CensusTuplet>)| {
        let mut import = run.import.clone();
        census(&mut import.source.census[0].tuplet_places);
        compare(&import, &run.reduced)
            .failures
            .iter()
            .any(|f| f.contains("the reader's tuplets are not the file's"))
    };
    assert!(!fails(&|_| {}), "the file's own census agrees");
    assert!(fails(&|t| {
        let hidden = t.iter_mut().find(|t| t.hidden).expect("a hidden tuplet");
        hidden.hidden = false;
    }));
    assert!(fails(&|t| {
        let shown = t.iter_mut().find(|t| !t.hidden).expect("a shown tuplet");
        shown.numberless = true;
    }));
}

#[test]
fn a_pickup_imports_in_full() {
    let run = run("pickup.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    // The measure after the pickup starts a beat in, the next a bar later.
    assert_eq!(measure_starts(score, 0), ["0", "1/4", "1"]);
    assert_eq!(
        events(score),
        ["s0 v0 0 1/4 G4", "s0 v0 1/4 3/4 C5", "s0 v0 1 3/4 E5"]
    );
    assert!(run.fidelity.explained.is_empty());
}

#[test]
fn features_without_an_operation_are_recorded_by_kind_and_not_imported() {
    let run = run("unsupported.musicxml");
    all_applied(&run);
    // The cue note keeps its quarter, so F5 starts a quarter into the
    // second measure, its A5 at F5's duration. The grace note, a zero-length
    // event since schema major 5, stands before C5.
    assert_eq!(
        events(&run.reduced.score),
        [
            "s0 v0 0 0 B4",
            "s0 v0 0 1/4 C5",
            "s0 v0 1/4 1/4 D5",
            "s0 v0 3/4 1/4 F5 A5"
        ]
    );
    let content: Vec<(&str, usize)> = run
        .import
        .source
        .features
        .of_class(FeatureClass::Content)
        .map(|(kind, f)| (kind, f.places.len()))
        .collect();
    assert_eq!(
        content,
        [("chord note of a different duration", 1), ("cue note", 1)]
    );
    assert_eq!(run.import.source.census[0].grace_or_cue, 2);
}

/// A fermata over a barline stands at the barline, not on the note the next
/// measure starts with there.
#[test]
fn a_barline_fermata_stands_at_the_barline() {
    use epiphany_core::{MarkerKind, TimeAnchor};
    let xml = "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A\
               </part-name></score-part></part-list><part id=\"P1\">\
               <measure number=\"1\"><attributes><divisions>1</divisions><time><beats>1</beats>\
               <beat-type>4</beat-type></time></attributes>\
               <note><pitch><step>C</step><octave>5</octave></pitch><duration>1</duration>\
               <voice>1</voice><type>quarter</type></note>\
               <barline location=\"right\"><fermata type=\"upright\"/></barline></measure>\
               <measure number=\"2\"><note><pitch><step>D</step><octave>5</octave></pitch>\
               <duration>1</duration><voice>1</voice><type>quarter</type></note></measure>\
               </part></score-partwise>";
    let import = import(xml).expect("imports");
    let reduced = reduce(&import);
    assert!(compare(&import, &reduced).passed());
    let markers = &reduced.score.cross_cutting.markers;
    assert_eq!(markers.len(), 1);
    assert!(matches!(markers[0].kind, MarkerKind::Fermata(_)));
    assert_eq!(
        offset(&markers[0].anchor),
        "1/4",
        "at the barline, not on the second measure's note"
    );
    assert!(!matches!(markers[0].anchor, TimeAnchor::Event { .. }));
}

/// A grace note inside a tuplet's span occupies no time, so it is no member:
/// the tuplet holds its three eighths and applies.
#[test]
fn a_grace_note_inside_a_tuplet_is_no_member() {
    let run = run("grace_in_tuplet.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        [
            "s0 v0 0 1/12 C5",
            "s0 v0 1/12 0 E5",
            "s0 v0 1/12 1/12 D5",
            "s0 v0 1/6 1/12 E5"
        ]
    );
    let ids = &run.import.ids.events[0];
    assert_eq!(
        score.cross_cutting.tuplets[0].members,
        vec![ids[0], ids[2], ids[3]]
    );
}

/// Each kind of expression and text the model holds (schema major 5),
/// imported through the operations with none recorded as unsupported: the
/// values written out by hand from the file.
#[test]
fn expression_and_text_import_through_their_operations() {
    use epiphany_core::{
        ArpeggioDirection, BracketKind, BreathMark, Dynamic, EventMark, Fermata, FermataShape,
        Grace, GraceKind, HairpinDirection, LineStyle, MarkerKind, Metronome, NoteValue,
        OctaveOffset, Ornament, OrnamentKind, PedalKind, SpannerKind, Syllabic, TempoMark, Text,
        TextLineDefinition,
    };
    use epiphany_musicxml::fidelity::expression_counts;
    use epiphany_ops::{OperationKind, OperationPayload};
    let run = run("expression.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        run.import
            .source
            .features
            .of_class(FeatureClass::Content)
            .count(),
        0,
        "nothing of the family is recorded as unsupported"
    );
    let counts: Vec<(String, usize)> = expression_counts(score).into_iter().collect();
    let expected: Vec<(String, usize)> = [
        ("grace", 2),
        ("lyric", 3),
        ("mark accent", 1),
        ("mark arpeggio", 1),
        ("mark harmonic", 1),
        ("mark marcato", 1),
        ("mark staccato", 2),
        ("mark tenuto", 1),
        ("mark tremolo", 1),
        ("mark two-note tremolo", 1),
        ("mark up-bow", 1),
        ("marker breath", 1),
        ("marker coda", 1),
        ("marker dynamic", 2),
        ("marker fermata", 2),
        ("marker rehearsal", 1),
        ("marker segno", 1),
        ("marker tempo", 1),
        ("marker text", 1),
        ("ornament trill-mark", 1),
        ("spanner bracket", 1),
        ("spanner glissando", 1),
        ("spanner hairpin", 1),
        ("spanner ottava", 1),
        ("spanner pedal", 1),
        ("spanner text line", 1),
        ("spanner trill line", 1),
        ("tempo", 2),
    ]
    .into_iter()
    .map(|(class, n)| (class.to_owned(), n))
    .collect();
    assert_eq!(counts, expected);

    // The graces stand at their note's position, of no length, in order,
    // before it.
    assert_eq!(
        events(score)[..3],
        ["s0 v0 0 0 B4", "s0 v0 0 0 D5", "s0 v0 0 1/4 C5"]
    );
    let ids = &run.import.ids.events[0];
    let event = |i: usize| score.events.get(ids[i]).expect("imported");
    let note = |i: usize| match event(i) {
        Event::Pitched(p) => p.clone(),
        other => panic!("a note, not {other:?}"),
    };
    for (i, order) in [(0, 0), (1, 1)] {
        assert_eq!(
            note(i).grace,
            Some(Grace {
                kind: GraceKind::Acciaccatura,
                value: NoteValue::Eighth,
                dots: 0,
                order
            })
        );
    }
    assert_eq!(
        run.reduced.score.cross_cutting.beams[0].events,
        vec![ids[0], ids[1]],
        "the graces beam together"
    );
    // Marks as a canonical set, ascending by kind; a trill with its flat.
    assert_eq!(
        note(2).marks,
        [EventMark::Staccato, EventMark::Tenuto, EventMark::Accent]
    );
    assert_eq!(
        note(2).ornaments,
        [Ornament {
            kind: OrnamentKind::Trill,
            accidental_above: Some(epiphany_core::AccidentalId::new("flat")),
            accidental_below: None,
        }]
    );
    assert_eq!(note(3).marks, [EventMark::Tremolo { strokes: 3 }]);
    assert_eq!(
        note(4).marks,
        [
            EventMark::Marcato,
            EventMark::Arpeggio {
                direction: ArpeggioDirection::Up
            }
        ]
    );
    assert_eq!(note(6).marks, [EventMark::UpBow, EventMark::Harmonic]);
    assert_eq!(note(9).marks, [EventMark::TremoloWithNext { strokes: 2 }]);

    // Lyrics: one syllable an event and verse.
    let lyrics: Vec<_> = score
        .cross_cutting
        .lyrics
        .iter()
        .map(|l| {
            (
                ids.iter().position(|e| *e == l.event).expect("an event"),
                l.verse,
                l.text.as_str().to_owned(),
                l.syllabic,
                l.extension,
            )
        })
        .collect();
    assert_eq!(
        lyrics,
        [
            (2, 1, String::from("la"), Syllabic::Begin, false),
            (2, 2, String::from("lo"), Syllabic::Single, false),
            (3, 1, String::from("la"), Syllabic::End, true),
        ]
    );

    // Point marks: on the note or rest starting where the file puts them,
    // the note before its graces; where none of the staff starts, at the
    // position.
    let on = |i: usize| TimeAnchor::Event {
        id: ids[i],
        offset: AnchorOffset::Zero,
    };
    let at = |numerator: i64, denominator: i64| TimeAnchor::Region {
        id: score.canvas.regions[0].id,
        edge: epiphany_core::RegionEdge::Start,
        offset: AnchorOffset::Musical(epiphany_core::MusicalDuration(
            RationalTime::new(numerator, denominator).expect("a time"),
        )),
    };
    let markers: Vec<(TimeAnchor, MarkerKind)> = score
        .cross_cutting
        .markers
        .iter()
        .map(|m| (m.anchor.clone(), m.kind.clone()))
        .collect();
    for expected in [
        (on(2), MarkerKind::Dynamic(Dynamic::Mf)),
        (on(2), MarkerKind::Text(Text::new("dolce"))),
        (on(2), MarkerKind::Rehearsal(Text::new("A"))),
        (
            on(5),
            MarkerKind::Fermata(Fermata {
                shape: FermataShape::Long,
                inverted: false,
            }),
        ),
        (at(1, 8), MarkerKind::Dynamic(Dynamic::P)),
        (on(6), MarkerKind::Breath(BreathMark::Comma)),
        (on(7), MarkerKind::Segno),
        (on(11), MarkerKind::Coda),
        (
            at(2, 1),
            MarkerKind::Fermata(Fermata {
                shape: FermataShape::Normal,
                inverted: false,
            }),
        ),
        (
            on(2),
            MarkerKind::Tempo(TempoMark {
                text: Some(Text::new("Allegro")),
                metronome: Some(Metronome {
                    beat: NoteValue::Quarter,
                    dots: 0,
                    per_minute: Text::new("96"),
                }),
            }),
        ),
    ] {
        assert!(markers.contains(&expected), "{expected:?} in {markers:?}");
    }
    assert_eq!(markers.len(), 10);

    // Lines from start to stop.
    let spanners: Vec<(TimeAnchor, TimeAnchor, SpannerKind, LineStyle)> = score
        .cross_cutting
        .spanners
        .iter()
        .map(|s| (s.start.clone(), s.end.clone(), s.kind.clone(), s.style.line))
        .collect();
    for expected in [
        (
            on(2),
            on(4),
            SpannerKind::Hairpin(HairpinDirection::Crescendo),
            LineStyle::Solid,
        ),
        (on(2), on(3), SpannerKind::TrillExtension, LineStyle::Solid),
        (
            on(7),
            on(11),
            SpannerKind::PedalBracket(PedalKind::Sustain),
            LineStyle::Solid,
        ),
        (
            on(7),
            on(11),
            SpannerKind::OctaveLine(OctaveOffset(1)),
            LineStyle::Solid,
        ),
        (
            on(7),
            on(11),
            SpannerKind::TextLine(TextLineDefinition {
                text: Text::new("cresc."),
            }),
            LineStyle::Dashed,
        ),
        (
            on(7),
            on(11),
            SpannerKind::Bracket(BracketKind::Square),
            LineStyle::Solid,
        ),
        (on(7), on(8), SpannerKind::Glissando, LineStyle::Wavy),
    ] {
        assert!(spanners.contains(&expected), "{expected:?} in {spanners:?}");
    }
    assert_eq!(spanners.len(), 7);

    // The tempo map: one segment at each tempo the file sets; the first's
    // mark in the same transaction as its segment.
    let bpm: Vec<(TimeAnchor, String)> = score
        .tempo_map
        .segments
        .iter()
        .map(|s| (s.start.clone(), format!("{}", s.start_tempo.bpm())))
        .collect();
    assert_eq!(
        bpm,
        [
            (at(0, 1), String::from("96")),
            (at(1, 1), String::from("120"))
        ]
    );
    let transaction_of = |pick: &dyn Fn(&OperationKind) -> bool| {
        run.import
            .envelopes
            .iter()
            .find(|e| matches!(&e.payload, OperationPayload::Primitive(kind) if pick(kind)))
            .and_then(|e| e.transaction)
    };
    let segment = transaction_of(
        &|k| matches!(k, OperationKind::SetTempoSegment(op) if op.start == at(0, 1)),
    );
    let mark = transaction_of(&|k| {
        matches!(k, OperationKind::CreateCrossCutting(op)
            if matches!(&op.structure, epiphany_ops::CrossCuttingValue::Marker(m)
                if matches!(m.kind, MarkerKind::Tempo(_))))
    });
    assert!(segment.is_some(), "the tempo is set in a transaction");
    assert_eq!(segment, mark, "its mark is in the same transaction");
}

#[test]
fn a_chord_tied_into_two_voices_keeps_both_ties() {
    let run = run("split_tie.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    assert_eq!(
        events(score),
        ["s0 v0 0 1 E4 C5", "s0 v0 1 1 C5", "s0 v1 1 1 E4"]
    );
    let mut ties: Vec<(TieClass, usize)> = score
        .cross_cutting
        .ties
        .iter()
        .map(|t| {
            (
                t.class.clone(),
                t.pitch_pairing.as_ref().map_or(0, Vec::len),
            )
        })
        .collect();
    ties.sort_by_key(|(class, _)| format!("{class:?}"));
    assert_eq!(ties, [(TieClass::CrossVoice, 1), (TieClass::Standard, 1)]);
    assert!(!run
        .import
        .source
        .features
        .kinds
        .contains_key("tie without a matching end"));
}

#[test]
fn parts_sharing_a_name_keep_their_own_accounts() {
    let run = run("same_names.musicxml");
    all_applied(&run);
    assert_eq!(
        events(&run.reduced.score),
        [
            "s0 v0 0 1/4 x5 m0",
            "s0 v0 1/4 3/4 rest",
            "s1 v0 0 1/4 x9 m0",
            "s1 v0 1/4 3/4 rest",
        ]
    );
    let parts = &run.import.source.parts;
    assert_eq!((parts[0].dropped_notes, parts[1].dropped_notes), (1, 0));
    assert_eq!(
        run.import.source.features.kinds["unpitched chord note"]
            .places
            .len(),
        1
    );
}

#[test]
fn beams_join_the_notes_of_one_voice_from_begin_to_end() {
    let run = run("beams.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    let onset = |id| match score.events.get(id).map(Event::position) {
        Some(EventPosition::Musical(p)) => rational(&p.0),
        other => format!("{other:?}"),
    };
    let mut beams: Vec<String> = score
        .cross_cutting
        .beams
        .iter()
        .map(|b| {
            b.events
                .iter()
                .map(|e| onset(*e))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    beams.sort();
    // Onsets in whole notes: the pairs of measure 1, the sixteenths and the
    // dotted pair of measure 2, and the second voice's pair there.
    assert_eq!(
        beams,
        [
            "0 1/8",
            "1/2 5/8",
            "1/2 9/16 5/8 11/16",
            "1/4 3/8",
            "3/4 15/16",
        ]
    );
    let source = &run.import.source;
    assert_eq!(
        (source.census[0].beams, source.census[0].unmade_beams),
        (5, 1)
    );
    assert_eq!(source.parts[0].unmade_beams, 1);
    assert_eq!(source.features.kinds["beam without an end"].places.len(), 1);
}

/// A beam joining the notes of one voice on both staves of a part is made
/// as the file writes it, each member on the staff its `<staff>` names, and
/// the census finds it by its own walk: every beam of the fixture's piano
/// part, seven of them across the staves.
#[test]
fn a_beam_across_two_staves_keeps_each_member_on_its_staff() {
    let run = run("cross_staff.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    let staves = &run.import.ids.staves[0];
    let mut staff_of = std::collections::BTreeMap::new();
    let mut onset_of = std::collections::BTreeMap::new();
    for region in &score.canvas.regions {
        for instance in region.staff_instances() {
            let staff = staves
                .iter()
                .position(|s| *s == instance.staff)
                .expect("a part staff");
            for voice in &instance.voices {
                for event in &voice.events {
                    staff_of.insert(*event, staff + 1);
                    if let Some(EventPosition::Musical(at)) =
                        score.events.get(*event).map(Event::position)
                    {
                        onset_of.insert(*event, rational(&at.0));
                    }
                }
            }
        }
    }
    let mut beams: Vec<String> = score
        .cross_cutting
        .beams
        .iter()
        .map(|b| {
            b.events
                .iter()
                .map(|e| format!("{}@{}", onset_of[e], staff_of[e]))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    beams.sort();
    // Onset in whole notes @ staff: the four 6:4 groups rising from the
    // lower staff, the two eighth groups falling from the upper, the two
    // triplets on the upper staff alone, and the sixteenths below ending on
    // the eighth chord above.
    assert_eq!(
        beams,
        [
            "0@2 1/24@2 1/12@2 1/8@1 1/6@1 5/24@1",
            "1/2@2 13/24@2 7/12@2 5/8@1 2/3@1 17/24@1",
            "1/4@2 7/24@2 1/3@2 3/8@1 5/12@1 11/24@1",
            "1@1 9/8@1 5/4@2 11/8@2",
            "2@1 25/12@1 13/6@1",
            "3/2@1 13/8@1 7/4@2 15/8@2",
            "3/4@2 19/24@2 5/6@2 7/8@1 11/12@1 23/24@1",
            "3@2 49/16@2 25/8@2 51/16@2 13/4@1",
            "9/4@1 7/3@1 29/12@1",
        ]
    );
    let census = &run.import.source.census[0];
    assert_eq!((census.beams, census.unmade_beams), (9, 0));
    assert_eq!(census.beam_places.len(), 9);
    assert_eq!(
        census
            .beam_places
            .iter()
            .filter(|b| b.crosses_staves())
            .count(),
        7
    );
}

/// The comparison holds the reader's beams to the census's own timed walk,
/// member by member and staff by staff: where the census places a member on
/// the other staff, at another time, or one fewer, the comparison fails,
/// though the counts agree and the score holds what the reader made.
#[test]
fn the_comparison_holds_each_beams_members_to_the_census() {
    let run = run("cross_staff.musicxml");
    let fails = |census: &dyn Fn(&mut epiphany_musicxml::source::CensusBeam)| {
        let mut import = run.import.clone();
        let beam = import.source.census[0]
            .beam_places
            .first_mut()
            .expect("the census places the file's beams");
        census(beam);
        compare(&import, &run.reduced)
            .failures
            .iter()
            .any(|f| f.contains("the reader's beams are not the file's"))
    };
    assert!(!fails(&|_| {}), "the file's own census agrees");
    assert!(fails(&|b| {
        b.members.pop();
    }));
    assert!(fails(&|b| b.members[4].0 = 1 - b.members[4].0));
    assert!(fails(&|b| {
        let (staff, measure, offset) = b.members[1].clone();
        b.members[1] = (
            staff,
            measure,
            offset.add(&epiphany_core::RationalTime::new(1, 64).unwrap()),
        );
    }));
}

#[test]
fn part_groups_and_grand_staves_become_staff_groups() {
    let run = run("groups.musicxml");
    all_applied(&run);
    let score = &run.reduced.score;
    let ids = &run.import.ids;
    let place = |staff: StaffId| -> String {
        ids.staves
            .iter()
            .enumerate()
            .find_map(|(p, staves)| {
                staves
                    .iter()
                    .position(|s| *s == staff)
                    .map(|s| format!("P{}.{}", p + 1, s + 1))
            })
            .unwrap_or_default()
    };
    let mut groups: Vec<String> = score
        .staff_groups
        .iter()
        .map(|g| {
            let mut members: Vec<String> = g.members.iter().map(|s| place(*s)).collect();
            members.sort();
            format!("{:?} {}", g.kind, members.join(" "))
        })
        .collect();
    groups.sort();
    assert_eq!(
        groups,
        [
            "Bracket P1.1 P2.1",
            "GrandStaff P3.1 P3.2",
            "SubBracket P4.1 P5.1",
        ]
    );
    let source = &run.import.source;
    assert_eq!(source.unmade_groups, 3);
    assert_eq!(
        (source.group_census.made, source.group_census.unmade),
        ([1, 1, 1], 3)
    );
    let kinds = &source.features.kinds;
    assert_eq!(
        kinds["part group (square) within another group"]
            .places
            .len(),
        1
    );
    assert_eq!(kinds["part group (line)"].places.len(), 1);
    assert_eq!(kinds["part group ()"].places.len(), 1);
}

#[test]
fn a_timewise_score_is_refused_by_name() {
    match import(&xml("timewise.musicxml")) {
        Err(ReadError::NotPartwise(root)) => assert_eq!(root, "score-timewise"),
        other => panic!(
            "expected a named refusal, got {:?}",
            other.map(|i| i.envelopes.len())
        ),
    }
}

#[test]
fn an_import_is_deterministic() {
    for name in ["chords_and_voices.musicxml", "ties_and_slurs.musicxml"] {
        let a = import(&xml(name)).expect("imports");
        let b = import(&xml(name)).expect("imports");
        assert_eq!(a.envelopes, b.envelopes, "{name}");
        assert_eq!(
            reduce(&a).state.canonical_bytes(),
            reduce(&b).state.canonical_bytes(),
            "{name}"
        );
    }
}

/// The comparison is not vacuous: each way of spoiling a reduced score is
/// caught as an unexplained difference.
#[test]
fn the_fidelity_comparison_catches_a_spoiled_score() {
    let base = run("ties_and_slurs.musicxml");
    type Spoil = fn(&mut Score);
    let spoils: [(&str, Spoil); 7] = [
        ("an event moved", |score| {
            let id = score.canvas.regions[0].staff_instances()[0].voices[0].events[1];
            if let Some(Event::Pitched(p)) = score.events.get_mut(id) {
                p.duration = EventDuration::Musical(epiphany_core::MusicalDuration(
                    RationalTime::new(1, 8).expect("1/8"),
                ));
            }
        }),
        ("a pitch respelled", |score| {
            let id = score.canvas.regions[0].staff_instances()[0].voices[0].events[0];
            if let Some(Event::Pitched(p)) = score.events.get_mut(id) {
                if let PitchSpacePosition::Cmn { alteration, .. } =
                    &mut p.pitches[0].pitch.scale_position.position
                {
                    *alteration = 1;
                }
            }
        }),
        ("a tie dropped", |score| {
            score.cross_cutting.ties.pop();
        }),
        ("a slur dropped", |score| {
            score.cross_cutting.slurs.pop();
        }),
        ("a clef changed", |score| {
            let instance = &mut score.canvas.regions[0]
                .content
                .staff_instances_mut()
                .expect("staff-based")[0];
            instance.clef_sequence[0].clef = Clef::alto();
        }),
        ("a transposition changed", |score| {
            score.instruments[0].transposition = interval(-1, -2);
        }),
        ("an event given to another voice", |score| {
            let instance = &mut score.canvas.regions[0]
                .content
                .staff_instances_mut()
                .expect("staff-based")[0];
            let moved = instance.voices[0].events.pop().expect("an event");
            let mut voice = instance.voices[0].clone();
            voice.id = epiphany_core::VoiceId::new(epiphany_core::ReplicaId(7), 7);
            voice.events = vec![moved];
            instance.voices.push(voice);
        }),
    ];
    for (what, spoil) in spoils {
        let mut reduced = base.reduced.clone();
        spoil(&mut reduced.score);
        let fidelity = compare(&base.import, &reduced);
        assert!(!fidelity.passed(), "{what} was not caught");
    }
    // And the reader is held to a raw count of the file's notes: a source
    // model that lost a rest disagrees with the count, whatever the score says.
    let mut import = base.import.clone();
    import.source.census[0].rests += 1;
    assert!(
        !compare(&import, &base.reduced).passed(),
        "a census mismatch was not caught"
    );
    // A quarter-tone is not the semitone of the same alteration count.
    let quarter = run("quarter_tones.musicxml");
    let mut reduced = quarter.reduced.clone();
    let id = reduced.score.canvas.regions[0].staff_instances()[0].voices[0].events[0];
    if let Some(Event::Pitched(p)) = reduced.score.events.get_mut(id) {
        p.pitches[0].pitch.scale_position.space = epiphany_core::PitchSpaceId::new("cmn-12");
    }
    assert!(
        !compare(&quarter.import, &reduced).passed(),
        "a quarter-tone read as a semitone was not caught"
    );
    // The quarter-tones are held to the file's `<alter>` and `<accidental>`
    // too: one the reader lost is caught where the score agrees with it, as
    // when an accidental's name was ignored.
    let arrows = run("arrow_accidentals.musicxml");
    let mut import = arrows.import.clone();
    let mut reduced = arrows.reduced.clone();
    let natural = epiphany_musicxml::source::cmn_pitch(epiphany_core::CmnNominal::G, 0, 4);
    let id = reduced.score.canvas.regions[0].staff_instances()[0].voices[0].events[0];
    if let Some(Event::Pitched(p)) = reduced.score.events.get_mut(id) {
        p.pitches[0].pitch = natural.clone();
    }
    if let epiphany_musicxml::source::Content::Pitched(pitches) =
        &mut import.source.parts[0].events[0].content
    {
        pitches[0].pitch = natural;
    }
    assert!(
        !compare(&import, &reduced).passed(),
        "a quarter-tone the reader lost was not caught"
    );
    // And at its value: a quarter-flat that the reader and the score both
    // hold as a three-quarter-flat keeps the number of quarter-tones.
    let mut import = arrows.import.clone();
    let mut reduced = arrows.reduced.clone();
    let lower = epiphany_musicxml::source::quarter_tone_pitch(epiphany_core::CmnNominal::G, -3, 4);
    let id = reduced.score.canvas.regions[0].staff_instances()[0].voices[0].events[0];
    if let Some(Event::Pitched(p)) = reduced.score.events.get_mut(id) {
        p.pitches[0].pitch = lower.clone();
    }
    if let epiphany_musicxml::source::Content::Pitched(pitches) =
        &mut import.source.parts[0].events[0].content
    {
        pitches[0].pitch = lower;
    }
    assert!(
        !compare(&import, &reduced).passed(),
        "a quarter-tone valued wrongly was not caught"
    );
    // And the ties to the file's count of tie starts: one the reader lost,
    // as it lost every tie on an unpitched note, is caught where the score
    // agrees with the reader.
    let tied = run("percussion_ties.musicxml");
    let mut import = tied.import.clone();
    let mut reduced = tied.reduced.clone();
    reduced.score.cross_cutting.ties.remove(0);
    let events = &mut import.source.parts[0].events;
    for (i, flag) in [(0, true), (1, false)] {
        if let epiphany_musicxml::source::Content::Unpitched {
            tie_start,
            tie_stop,
            ..
        } = &mut events[i].content
        {
            *if flag { tie_start } else { tie_stop } = false;
        }
    }
    assert!(
        !compare(&import, &reduced).passed(),
        "a tie the reader lost was not caught"
    );
    // So are the keys and clefs: one the reader lost is caught even where the
    // score agrees with the reader, as when a key before `<staves>` was lost.
    let keyed = run("keyed_grand_staves.musicxml");
    for what in ["key", "clef"] {
        let mut import = keyed.import.clone();
        let mut reduced = keyed.reduced.clone();
        let instance = &mut reduced.score.canvas.regions[0]
            .content
            .staff_instances_mut()
            .expect("staff-based")[1];
        let staff = &mut import.source.parts[0].staves[1];
        if what == "key" {
            instance.key_sequence.remove(0);
            staff.keys.remove(0);
        } else {
            instance.clef_sequence.remove(0);
            staff.clefs.remove(0);
        }
        assert!(
            !compare(&import, &reduced).passed(),
            "a {what} the reader lost was not caught"
        );
    }
}
