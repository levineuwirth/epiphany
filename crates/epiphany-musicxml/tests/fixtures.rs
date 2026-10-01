//! The hand-written MusicXML fixtures, each taken through the outermost seam:
//! import, reduce, every operation's outcome, the invariants, the fidelity
//! comparison, and engraving. The expected values are written out by hand from
//! the files, so they check the reader's interpretation of MusicXML rather
//! than restating it.

use std::path::Path;

use epiphany_core::{
    check_invariants, AnchorOffset, Clef, ClefShape, Event, EventDuration, EventPosition,
    PitchSpacePosition, RationalTime, ScalePosition, Score, TieClass, TimeAnchor,
    TimeSignatureDisplay, TranspositionInterval,
};
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{to_constrained, to_logical, ConstraintSolver, SolverConfig};
use epiphany_musicxml::emit::Subject;
use epiphany_musicxml::fidelity::{compare, show, Fidelity};
use epiphany_musicxml::outcome::{reduce, Reduced, Verdict};
use epiphany_musicxml::source::FeatureClass;
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
    let census = &run.import.source.census;
    assert_eq!(census[0].keys, [vec![-2, 1], vec![-2, 1]]);
    assert_eq!(census[1].keys, [vec![1], vec![-1]]);
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
            "s1 v0 0 1 C+1q5",
            "s1 v0 1 1 rest",
        ]
    );
    assert_eq!(score.instruments[1].transposition, interval(-1, -2));
    // A tie's pitches must be enharmonically equivalent, which the core
    // answers only in a twelve-chromatic space: the quarter-tones' tie is
    // recorded, and the ordinary tie beside it is made.
    assert_eq!(ties(score), ["3/4 A4 -> 1 A4"]);
    let kinds = &run.import.source.features.kinds;
    let tie = &kinds["tie on a quarter-tone pitch"];
    assert_eq!((tie.class, tie.places.len()), (FeatureClass::Content, 1));
    assert!(!kinds.contains_key("tie without a matching end"));
}

#[test]
fn a_quarter_tone_accidental_named_without_an_alter_gives_the_pitch_and_carries() {
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
            // The second voice's B three-quarter-flat at 17/4 reaches this B.
            "s0 v0 4 1/2 rest",
            "s0 v0 9/2 1/4 B-3q4",
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
        ]
    );
    assert_eq!(run.import.source.census[0].quarter_tones, 23);
    assert_eq!(ties(score), ["9 D5 -> 10 D5"]);
    let kinds = &run.import.source.features.kinds;
    assert_eq!(kinds["tie on a quarter-tone pitch"].places.len(), 4);
    assert!(!kinds.contains_key("tie without a matching end"));
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
    assert_eq!(run.import.recorded_ties, [1]);
    assert_eq!(source.parts[0].dropped_tie_starts, 1);
    assert_eq!(
        source.features.kinds["tie without a matching end"]
            .places
            .len(),
        1
    );
}

#[test]
fn tuplet_notes_sit_at_exact_positions_and_the_grouping_is_recorded() {
    let run = run("tuplet.musicxml");
    all_applied(&run);
    assert_eq!(
        events(&run.reduced.score),
        [
            "s0 v0 0 1/12 A4",
            "s0 v0 1/12 1/12 B4",
            "s0 v0 1/6 1/12 C5",
            "s0 v0 1/4 1/4 D5",
        ]
    );
    let feature = &run.import.source.features.kinds["tuplet 3:2"];
    assert_eq!(feature.class, FeatureClass::Content);
    assert_eq!(feature.places.len(), 1);
    assert!(run.reduced.score.cross_cutting.tuplets.is_empty());
}

#[test]
fn a_pickup_reports_the_measures_the_reducer_refuses() {
    let run = run("pickup.musicxml");
    let refused: Vec<(Subject, &Verdict)> = run
        .reduced
        .rejected()
        .map(|i| {
            (
                run.import.labels[i].subject.clone(),
                &run.reduced.verdicts[i],
            )
        })
        .collect();
    let mismatch = Verdict::Refused(String::from("MeasureMeterMismatch"));
    assert_eq!(
        refused,
        [
            (Subject::Measure(0, 0, 1), &mismatch),
            (Subject::Measure(0, 0, 2), &mismatch),
        ]
    );
    let score = &run.reduced.score;
    assert_eq!(measure_starts(score, 0), ["0"]);
    assert_eq!(
        events(score),
        ["s0 v0 0 1/4 G4", "s0 v0 1/4 3/4 C5", "s0 v0 1 3/4 E5"]
    );
    // The absent measures are explained by the refusals, not silently passed.
    assert_eq!(run.fidelity.explained.len(), 2);
    assert!(run.fidelity.explained[0].contains("measure 1 absent"));
    assert!(run.fidelity.explained[0].contains("MeasureMeterMismatch"));
}

#[test]
fn features_without_an_operation_are_recorded_by_kind_and_not_imported() {
    let run = run("unsupported.musicxml");
    all_applied(&run);
    assert_eq!(
        events(&run.reduced.score),
        ["s0 v0 0 1/4 C5", "s0 v0 1/4 1/4 D5"]
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
        [
            ("articulations: staccato", 1),
            ("direction: dynamics", 1),
            ("direction: words", 1),
            ("fermata", 1),
            ("grace note", 1),
            ("lyric", 1),
        ]
    );
    assert_eq!(run.import.source.census[0].grace_or_cue, 1);
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
