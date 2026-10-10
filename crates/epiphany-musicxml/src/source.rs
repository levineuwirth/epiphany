//! Reading partwise MusicXML into a [`SourceScore`]: the source as the file
//! states it, with every time an exact rational in whole notes.
//!
//! Every element the reader meets is either mapped into the source model or
//! recorded in [`Features`] by kind, so nothing is dropped without a trace.
//! The reader interprets MusicXML; it builds no Epiphany value and emits no
//! operation (that is [`crate::emit`]).

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    AccidentalId, AcousticPitch, AcousticRealization, ArpeggioDirection, BracketKind, BreathMark,
    CaesuraMark, Clef, ClefShape, CmnNominal, Dynamic, EventMark, Fermata, FermataShape, Grace,
    GraceKind, HairpinDirection, LineStyle, MarkerKind, Metronome, NoteValue, OctaveOffset,
    Ornament, OrnamentKind, PedalKind, Pitch, PitchSpaceId, PitchSpacePosition, PitchSpelling,
    RationalTime, ScalePosition, SpannerKind, SpellingNominal, Syllabic, TempoMark, Text,
    TextLineDefinition, TranspositionInterval, TuningReference, TupletBracket, TupletDisplay,
    TupletNumber,
};
use roxmltree::{Document, Node, ParsingOptions};

/// A time or length in whole notes.
pub type Time = RationalTime;

/// Why a file could not be read at all. Anything short of this is read, with
/// what could not be mapped recorded in [`Features`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReadError {
    /// Not well-formed XML.
    Xml(String),
    /// The root is not `score-partwise` (a timewise score, or not MusicXML).
    NotPartwise(String),
    /// A value the reader needs is malformed: `(line, what)`.
    Malformed(u32, String),
    /// The file states something the model cannot hold and the importer
    /// can neither approximate nor leave out, such as a pitch that is not a
    /// whole number of quarter-tones: `(line, what)`.
    Unsupported(u32, String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Xml(e) => write!(f, "not well-formed XML: {e}"),
            ReadError::NotPartwise(root) => {
                write!(f, "root element is <{root}>, not <score-partwise>")
            }
            ReadError::Malformed(line, what) => write!(f, "line {line}: {what}"),
            ReadError::Unsupported(line, what) => write!(f, "line {line}: unsupported: {what}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// How a feature that is not imported stands (see [`Features`]).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum FeatureClass {
    /// Musical content the model cannot yet hold: tuplet grouping, lyrics,
    /// dynamics, articulations, grace notes, repeats, tempo, text, ….
    Content,
    /// Engraving information the importer does not map yet: beams, stems,
    /// note and rest placement, system and page breaks, barline styles, ….
    Notation,
    /// Page layout, fonts, credits and playback settings.
    Presentation,
}

/// Where a feature occurs: the part's name and the measure's number.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Place {
    pub part: String,
    pub measure: String,
}

/// One kind of feature that is not imported, with every place it occurs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Feature {
    pub class: FeatureClass,
    pub places: Vec<Place>,
}

/// The source features that are not imported, by kind. A kind is a short
/// phrase naming the element (`"tuplet 3:2"`, `"direction: dynamics"`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Features {
    pub kinds: BTreeMap<String, Feature>,
}

impl Features {
    pub(crate) fn record(&mut self, class: FeatureClass, kind: impl Into<String>, place: Place) {
        self.kinds
            .entry(kind.into())
            .or_insert_with(|| Feature {
                class,
                places: Vec::new(),
            })
            .places
            .push(place);
    }

    /// The kinds of one class, with their counts, in kind order.
    pub fn of_class(&self, class: FeatureClass) -> impl Iterator<Item = (&str, &Feature)> {
        self.kinds
            .iter()
            .filter(move |(_, f)| f.class == class)
            .map(|(k, f)| (k.as_str(), f))
    }
}

/// A measure across the whole score: the parts' measures share an index.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceMeasure {
    /// The measure's `number` attribute, as written.
    pub number: String,
    /// The start, in whole notes from the start of the score.
    pub onset: Time,
    /// The length: the furthest any part's content reaches in it.
    pub length: Time,
    /// `implicit="yes"`: a measure that does not count (a pickup).
    pub implicit: bool,
}

/// A meter: `numerators` over `denominator` (`3+2` over 8 is `[3, 2]`, 8).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Meter {
    pub numerators: Vec<u16>,
    pub denominator: u16,
}

impl Meter {
    /// The length of one measure under this meter, in whole notes.
    pub fn measure_length(&self) -> Time {
        let beats: i64 = self.numerators.iter().map(|&n| i64::from(n)).sum();
        RationalTime::new(beats, i64::from(self.denominator)).expect("a nonzero denominator")
    }
}

/// A meter taking effect at a point.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MeterChange {
    pub onset: Time,
    pub measure: usize,
    pub meter: Meter,
}

/// A clef taking effect on a staff.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ClefChange {
    pub onset: Time,
    pub measure: usize,
    pub clef: Clef,
}

/// A key signature taking effect on a staff, in fifths.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyChange {
    pub onset: Time,
    pub measure: usize,
    pub fifths: i8,
}

/// One staff of a part.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SourceStaff {
    pub clefs: Vec<ClefChange>,
    pub keys: Vec<KeyChange>,
    /// `staff-details/staff-lines`, if the file states it.
    pub lines: Option<u8>,
}

/// A pitch as it sounds, with the file's tie flags on it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourcePitch {
    pub pitch: Pitch,
    pub tie_start: bool,
    pub tie_stop: bool,
    /// For a quarter-tone, the spelling its notation gives it at its
    /// sounding pitch: its letter and octave, and the quarter-tone accidental
    /// the file writes on it or carries to it over a tie
    /// ([`quarter_tone_accidental`]). The importer authors it, since the
    /// spelling pre-pass spells no `cmn-24` pitch. `None` for every other
    /// pitch.
    pub spelling: Option<PitchSpelling>,
}

/// What an event is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Content {
    /// A rest; `visible` is false for `print-object="no"`.
    Rest { visible: bool },
    /// One or more pitches sounding together (a note or a chord).
    Pitched(Vec<SourcePitch>),
    /// An unpitched percussion note: its staff step (bottom line 0, one per
    /// diatonic step, read against a treble clef as MusicXML prescribes), the
    /// index of its member in [`SourcePart::members`], and the file's tie
    /// flags on it.
    Unpitched {
        step: i16,
        member: usize,
        tie_start: bool,
        tie_stop: bool,
    },
}

/// A note, chord or rest.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceEvent {
    pub measure: usize,
    /// 0-based staff within the part.
    pub staff: usize,
    /// The file's voice, as written (`"1"` when absent).
    pub voice: String,
    /// The start, in whole notes from the start of the score.
    pub onset: Time,
    pub duration: Time,
    pub content: Content,
    /// The byte offset of the event's first `<note>` in the file (a line
    /// number would cost a scan of the file per event).
    pub offset: usize,
    /// The marks its notes carry, one of each kind (schema major 5).
    pub marks: Vec<EventMark>,
    /// A note's ornaments, one of each kind.
    pub ornaments: Vec<Ornament>,
    /// A grace note's notation; its duration is then zero, and it stands at
    /// the position of the note it precedes.
    pub grace: Option<Grace>,
    /// Its lyric syllables, one per verse.
    pub lyrics: Vec<SourceLyric>,
}

/// A lyric syllable on an event.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceLyric {
    pub verse: u16,
    pub text: Text,
    pub syllabic: Syllabic,
    pub extension: bool,
}

/// Where a mark the file places stands: on an event of the part, or at a
/// staff's position, which the emitter puts on an event of the staff that
/// starts there or, where none does, at the position itself. A position is
/// read as an offset into its measure and placed once the measures are.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SourcePoint {
    Event(usize),
    At {
        staff: usize,
        measure: usize,
        onset: Time,
    },
    /// A barline's place, which stands at the position whatever event starts
    /// there: a fermata over a barline is not over the next measure's note.
    Barline {
        measure: usize,
        onset: Time,
    },
}

/// A point mark: a dynamic, a fermata, a breath mark, a caesura, staff text,
/// a tempo mark, a rehearsal mark, a segno or a coda.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceMarker {
    pub at: SourcePoint,
    pub kind: MarkerKind,
}

/// A line on one staff from one point to another: a hairpin, a pedal line,
/// an ottava, a trill line, a glissando, a text line or a bracket.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceSpanner {
    pub kind: SpannerKind,
    pub line: LineStyle,
    pub staff: usize,
    pub start: SourcePoint,
    pub end: SourcePoint,
}

/// A tempo the file sets (`<sound tempo>`), in quarter notes per minute, and
/// the tempo mark its direction shows, if it shows one.
#[derive(Clone, PartialEq, Debug)]
pub struct SourceTempo {
    pub at: SourcePoint,
    pub bpm: f64,
    pub mark: Option<TempoMark>,
}

// The reader admits only a finite positive tempo, so equality is total.
impl Eq for SourceTempo {}

/// A tuplet of a part's events, by index into [`SourcePart::events`]: the
/// notes and rests of one voice from a `<tuplet type="start">` to the stop of
/// the same number, at the ratio its first note's `<time-modification>`
/// gives (`actual` notes in the time of `normal`), shown as its start mark
/// says: hidden where the mark's `<notations>` is not printed, without a
/// number where it asks `show-number="none"`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceTuplet {
    pub actual: u32,
    pub normal: u32,
    pub events: Vec<usize>,
    pub display: TupletDisplay,
}

/// How a tuplet's start mark shows it: hidden, with no number and no
/// bracket, where the `<notations>` holding it is not printed
/// (`print-object="no"`, as MuseScore writes a tuplet it hides); with no
/// number where the mark says `show-number="none"`; otherwise as an engraver
/// draws any tuplet. A mark's `bracket` attribute is not read: MuseScore
/// writes `no` on a beamed tuplet it draws without one and `yes` on one it
/// brackets, which the engraver's own choice already follows.
fn tuplet_display(notations: Node, mark: Node) -> TupletDisplay {
    if notations.attribute("print-object") == Some("no") {
        return TupletDisplay::HIDDEN;
    }
    TupletDisplay {
        number: if mark.attribute("show-number") == Some("none") {
            TupletNumber::None
        } else {
            TupletNumber::Actual
        },
        bracket: TupletBracket::Auto,
    }
}

/// A beamed group of a part's events, by index into [`SourcePart::events`]:
/// the notes of one voice from a `<beam number="1">` begin to its end.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceBeam {
    pub events: Vec<usize>,
}

/// A slur between two events of a part, by index into [`SourcePart::events`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceSlur {
    pub start: usize,
    pub end: usize,
}

/// A percussion instrument member (a `score-instrument` of a part that has
/// unpitched notes).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceMember {
    pub id: String,
    pub name: String,
    /// The staff step of the member's first note, or 0 if it has none.
    pub step: i16,
}

/// A part: one instrument on one or more staves.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourcePart {
    pub id: String,
    pub name: String,
    pub abbreviation: Option<String>,
    pub staves: Vec<SourceStaff>,
    /// The written-versus-sounding interval: what is added to a written pitch
    /// to reach the sounding one (a B-flat clarinet is -1 diatonic, -2
    /// chromatic), octave changes folded in. `None` for a non-transposing part.
    pub transposition: Option<TranspositionInterval>,
    pub members: Vec<SourceMember>,
    pub events: Vec<SourceEvent>,
    pub slurs: Vec<SourceSlur>,
    pub beams: Vec<SourceBeam>,
    /// The beams begun in the file that the reader made none of, each
    /// recorded: one never ended, one begun again before its end, or one
    /// ended on the note it began.
    pub unmade_beams: usize,
    pub tuplets: Vec<SourceTuplet>,
    /// The tuplets begun in the file that the reader made none of, each
    /// recorded: one inside another (nesting is not yet read), one with no
    /// usable ratio, or one never stopped.
    pub unmade_tuplets: usize,
    /// Chord notes recorded as unsupported and not imported: a cross-staff
    /// chord note, an unpitched chord note, a chord note joining a rest.
    pub dropped_notes: usize,
    /// Of the dropped notes, those the reader makes quarter-tones.
    pub dropped_quarter_tones: Vec<QuarterTone>,
    /// Of the dropped notes, those carrying a tie start.
    pub dropped_tie_starts: usize,
    /// Its point marks, its lines and its tempos (schema major 5).
    pub markers: Vec<SourceMarker>,
    pub spanners: Vec<SourceSpanner>,
    pub tempos: Vec<SourceTempo>,
}

/// A quarter-tone: where it falls, and the pitch it sounds.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct QuarterTone {
    /// The measure's index.
    pub measure: usize,
    /// The start within the measure, in whole notes.
    pub offset: Time,
    /// 0-based staff within the part.
    pub staff: usize,
    /// The sounding pitch's nominal, C to B as 0 to 6.
    pub nominal: u8,
    /// Its alteration, in quarter-tones (odd, being a quarter-tone).
    pub quarter_tones: i16,
    pub octave: i8,
}

/// Counts taken straight from a part's elements by walks the reader does not
/// run, sharing none of its code, as an independent check on it: the
/// `<note>` elements by kind and their tie starts, with no timing logic; the
/// keys and clefs its `<attributes>` state; and, timed by a reading of their
/// own, the quarter-tones at their values, the chord notes the model cannot
/// hold, and the tie starts the file does not end or ends on a quarter-tone.
/// Each count the reader keeps of what it leaves out is held to one of these,
/// and the quarter-tone ties to the ones the score makes.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Census {
    /// `<note>` elements with a `<pitch>`, not grace or cue.
    pub pitched: usize,
    /// `<note>` elements with an `<unpitched>`, not grace or cue.
    pub unpitched: usize,
    /// `<note>` elements with a `<rest>`, not grace or cue.
    pub rests: usize,
    /// Of the counted notes, those carrying `<chord/>`.
    pub chord_members: usize,
    /// Of the counted pitched and unpitched notes, those with a
    /// `<tie type="start"/>` among their `<tie>` elements.
    pub tie_starts: usize,
    /// Of the counted notes, the chord notes the model cannot hold, by kind:
    /// a `<chord/>` note that is not pitched, or whose chord's first note is
    /// not pitched or is on another `<staff>`.
    pub dropped: Kinds,
    /// Of the tie starts, those on dropped chord notes.
    pub dropped_tie_starts: usize,
    /// Of the other tie starts, those the file gives no stop: no note of the
    /// part that is not dropped, on the same staff, at the same written step,
    /// octave and alteration (an unpitched note: the same display step,
    /// octave and instrument), carries a `<tie type="stop"/>` where the tied
    /// note ends, in its measure or at the start of the next.
    pub unended_ties: usize,
    /// Of the tie starts the file ends, those on a quarter-tone, each of
    /// which the score ties or a refused tie explains.
    pub quarter_tone_ties: usize,
    /// Grace and cue notes, which this walk does not count among the notes:
    /// a cue note is not imported, and the expression census counts graces.
    pub grace_or_cue: usize,
    /// Primary beams (`<beam number="1">`) the file begins and ends in one
    /// voice, paired by a walk of the notes not joining a chord.
    pub beams: usize,
    /// Primary beams the file begins and never ends: one begun again before
    /// its end, or still open when the part ends.
    pub unmade_beams: usize,
    /// The beams counted in `beams`, each where the census's own timed walk
    /// finds it: each member's staff, measure and offset, a member being
    /// each note of its voice, not joining a chord, that carries its
    /// primary beam from its begin to its end. A beam whose members sit on
    /// two staves is a cross-staff beam.
    pub beam_places: Vec<CensusBeam>,
    /// The expression and text the model can hold, counted by class by a walk
    /// of its own (schema major 5): each event's marks and ornaments, its
    /// grace notes, point marks, lyric syllables, lines begun and ended, and
    /// the places a tempo is set ([`expression_census`]).
    pub expression: BTreeMap<String, usize>,
    /// Where the part sets a tempo: the measure and the offset in it.
    pub tempo_places: BTreeSet<(usize, Time)>,
    /// Tuplets the file begins and stops in one voice, outside any other, by
    /// the ratio (`actual`, `normal`) their first note's
    /// `<time-modification>` gives, paired by number by a walk of the notes
    /// not joining a chord.
    pub tuplets: BTreeMap<(u32, u32), usize>,
    /// Tuplets the file begins and the reader cannot make: one begun inside
    /// another, one whose first note gives no usable ratio, or one never
    /// stopped.
    pub unmade_tuplets: usize,
    /// The tuplets counted in `tuplets`, each where the census's own timed
    /// walk finds it: the staff of its first note, its ratio, and the measure
    /// and offset of each member, a member being each note of its voice, not
    /// joining a chord, from its start to its stop.
    pub tuplet_places: Vec<CensusTuplet>,
    /// Per staff, the `fifths` of each `<key>` the model can hold (at most
    /// seven accidentals) that applies to it, where it is stated: a numbered
    /// key to its staff, an unnumbered one to every staff the part's
    /// `<staves>` declare.
    pub keys: Vec<Vec<Stated<i8>>>,
    /// Per staff, each `<clef>` of a shape the model holds that applies to
    /// it, where it is stated: a numbered clef to its staff, an unnumbered
    /// one to the first.
    pub clefs: Vec<Vec<Stated<Clef>>>,
    /// Pitched notes, not grace or cue, that the file makes quarter-tones:
    /// by a fractional `<alter>`, by a quarter-tone `<accidental>` with no
    /// `<alter>`, or by a tie from either to a note that writes neither.
    /// Each is read with its own timing, its own value for each name and its
    /// own transposition to the sounding pitch.
    pub quarter_tones: Vec<QuarterTone>,
}

/// A tuplet where the census finds it, apart from the reader.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CensusTuplet {
    /// The staff of its first note, from 0.
    pub staff: usize,
    /// Its `actual` and `normal` notes.
    pub ratio: (u32, u32),
    /// Each member's measure index and offset within the measure, in whole
    /// notes.
    pub members: Vec<(usize, Time)>,
    /// Whether its start mark's `<notations>` is not printed, and whether
    /// the mark asks for no number.
    pub hidden: bool,
    pub numberless: bool,
}

/// A primary beam where the census finds it, apart from the reader.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CensusBeam {
    /// Each member's staff (from 0), measure index and offset within the
    /// measure, in whole notes.
    pub members: Vec<(usize, usize, Time)>,
}

impl CensusBeam {
    /// Whether its members sit on more than one staff.
    pub fn crosses_staves(&self) -> bool {
        self.members
            .first()
            .is_some_and(|(first, _, _)| self.members.iter().any(|(s, _, _)| s != first))
    }
}

/// A key or clef where the file states it: the index of its measure, and
/// its offset within the measure in whole notes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Stated<T> {
    pub measure: usize,
    pub offset: Time,
    pub value: T,
}

/// Notes counted by kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Kinds {
    pub pitched: usize,
    pub unpitched: usize,
    pub rests: usize,
}

/// The counts of a part's `<note>` elements, read from each note's own
/// children with an exclusion of its own: grace and cue notes apart, what
/// every other note is, whether it joins a chord, and whether it starts a
/// tie, a rest never.
fn note_census(part: Node) -> Census {
    let mut census = Census::default();
    // Per voice, whether a primary beam is open.
    let mut open: BTreeSet<&str> = BTreeSet::new();
    // Per voice, the open tuplets by number, each with its ratio when the
    // reader could make it.
    type Open<'a> = Vec<(&'a str, Option<(u32, u32)>)>;
    let mut tuplets: BTreeMap<&str, Open> = BTreeMap::new();
    for note in children(part, "measure").flat_map(|m| children(m, "note")) {
        if child(note, "grace").is_some() || child(note, "cue").is_some() {
            census.grace_or_cue += 1;
            continue;
        }
        let rest = child(note, "rest").is_some();
        if child(note, "pitch").is_some() {
            census.pitched += 1;
        } else if child(note, "unpitched").is_some() {
            census.unpitched += 1;
        } else if rest {
            census.rests += 1;
        }
        census.chord_members += usize::from(child(note, "chord").is_some());
        let primary = children(note, "beam")
            .find(|b| b.attribute("number").is_none_or(|n| n == "1"))
            .map(text);
        if let (None, Some(beam)) = (child(note, "chord"), primary) {
            let voice = child_text(note, "voice").unwrap_or("1");
            match beam {
                "begin" if !open.insert(voice) => census.unmade_beams += 1,
                "end" if open.remove(voice) => census.beams += 1,
                _ => {}
            }
        }
        if child(note, "chord").is_none() {
            let voice = child_text(note, "voice").unwrap_or("1");
            let stack = tuplets.entry(voice).or_default();
            let marks: Vec<Node> = children(note, "notations")
                .flat_map(|n| children(n, "tuplet"))
                .collect();
            for mark in marks
                .iter()
                .filter(|t| t.attribute("type") == Some("start"))
            {
                let ratio = child(note, "time-modification").and_then(|m| {
                    let actual = child_text(m, "actual-notes")?.trim().parse::<u32>().ok()?;
                    let normal = child_text(m, "normal-notes")?.trim().parse::<u32>().ok()?;
                    (actual != 0 && normal != 0 && actual != normal).then_some((actual, normal))
                });
                let made = ratio.filter(|_| stack.is_empty());
                stack.push((mark.attribute("number").unwrap_or("1"), made));
            }
            for mark in marks.iter().filter(|t| t.attribute("type") == Some("stop")) {
                let number = mark.attribute("number").unwrap_or("1");
                if let Some(at) = stack.iter().rposition(|(n, _)| *n == number) {
                    match stack.remove(at).1 {
                        Some(ratio) => *census.tuplets.entry(ratio).or_default() += 1,
                        None => census.unmade_tuplets += 1,
                    }
                }
            }
        }
        let tied = children(note, "tie").any(|t| t.attribute("type") == Some("start"));
        census.tie_starts += usize::from(tied && !rest);
    }
    census.unmade_beams += open.len();
    census.unmade_tuplets += tuplets.values().map(Vec::len).sum::<usize>();
    census
}

/// A timed walk of a part's notes, not grace or cue, that the reader does not
/// run: the quarter-tones its file makes, the chord notes the model cannot
/// hold, and the tie starts the file leaves without a stop or puts on a
/// quarter-tone.
///
/// A pitched note is a quarter-tone by a fractional `<alter>`, by a
/// quarter-tone `<accidental>` with no `<alter>` (roadmap D20), or by a tie
/// from such a note, when the note writes neither. A quarter-tone accidental
/// applies to its own note alone, as MuseScore reads it (roadmap D40), so a
/// later note of its line in the measure that writes neither is not one. The
/// walk reads `<alter>`, `<accidental>`, `<divisions>` and `<transpose>` and
/// times the notes itself, sharing none of the reader's code. A name's value
/// comes from the accidental it alters and its arrow, not from the reader's
/// table, and the sounding pitch from an arithmetic of its own, not from the
/// core's transposition, so a quarter-tone the reader values or places
/// wrongly shows as a difference, and not only one it misses.
///
/// A chord note is dropped by its own kind and staff and its chord's first
/// note's, in the file's order, and a tie start is ended by a stop the walk
/// finds itself: where the tied note ends in its measure, or at the start of
/// the next when it ends with the measure's furthest note. So a tie the
/// reader loses at its stop, a chord note it drops, or a note it takes for
/// the other kind differs from these counts, rather than passing as a
/// feature of the source.
/// The expression and text of a part the model can hold, by class, counted by
/// a walk of the part's elements that shares none of the reader's code, the
/// classes named as the score's values are (`mark staccato`, `marker
/// dynamic`, `spanner hairpin`): a note's marks and ornaments once per kind
/// over its chord, a chord note on another staff, or not pitched, not among
/// them; an ornament only on a pitched note, a mark not on a rest; each grace
/// note; each point mark of a known value; each lyric syllable, one per verse
/// of an event; each line a start and a stop of its kind and number make;
/// and, timed by its own walk, where a tempo is set.
pub fn expression_census(part: Node) -> (BTreeMap<String, usize>, BTreeSet<(usize, Time)>) {
    // The event a note starts and its chord notes join: its staff, whether it
    // is pitched, whether it is a rest, the mark and ornament kinds gathered,
    // and the verses its syllables take.
    type Gathered = (String, bool, bool, BTreeSet<String>, BTreeSet<u16>);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut add = |class: &str| *counts.entry(class.to_owned()).or_default() += 1;
    let mut tempos: BTreeSet<(usize, Time)> = BTreeSet::new();
    let mut open: BTreeMap<(String, String), ()> = BTreeMap::new();
    let mut lines: Vec<String> = Vec::new();
    let mut line = |key: (String, String), begins: bool, ends: bool, class: &str| {
        if ends && open.remove(&key).is_some() {
            lines.push(class.to_owned());
        }
        if begins {
            open.insert(key, ());
        }
    };
    let mut divisions: i64 = 1;
    for (m, measure) in children(part, "measure").enumerate() {
        let mut cursor: i64 = 0;
        let mut event: Option<Gathered> = None;
        let flush = |event: &mut Option<Gathered>, add: &mut dyn FnMut(&str)| {
            if let Some((_, _, _, kinds, _)) = event.take() {
                for kind in kinds {
                    add(&kind);
                }
            }
        };
        for item in elements(measure) {
            match name(item) {
                "attributes" => {
                    if let Some(d) = child_text(item, "divisions").and_then(|d| d.parse().ok()) {
                        divisions = d;
                    }
                }
                "backup" | "forward" => {
                    let d: i64 = child_text(item, "duration")
                        .and_then(|d| d.trim().parse().ok())
                        .unwrap_or(0);
                    cursor += if name(item) == "backup" { -d } else { d };
                }
                "sound"
                    if item
                        .attribute("tempo")
                        .and_then(|t| t.trim().parse::<f64>().ok())
                        .is_some_and(|t| t.is_finite() && t > 0.0) =>
                {
                    tempos.insert((
                        m,
                        RationalTime::new(cursor.max(0), 4 * divisions).unwrap_or_else(zero),
                    ));
                }
                "barline" => {
                    for f in children(item, "fermata") {
                        if matches!(
                            text(f).trim(),
                            "" | "normal" | "angled" | "square" | "double-angled" | "double-square"
                        ) {
                            add("marker fermata");
                        }
                    }
                }
                "direction" => {
                    let offset: i64 = child_text(item, "offset")
                        .and_then(|o| o.trim().parse().ok())
                        .unwrap_or(0);
                    let types: Vec<Node> = children(item, "direction-type")
                        .flat_map(elements)
                        .collect();
                    let sound = child(item, "sound")
                        .and_then(|s| s.attribute("tempo"))
                        .and_then(|t| t.trim().parse::<f64>().ok())
                        .is_some_and(|t| t.is_finite() && t > 0.0);
                    if sound {
                        tempos.insert((
                            m,
                            RationalTime::new((cursor + offset).max(0), 4 * divisions)
                                .unwrap_or_else(zero),
                        ));
                    }
                    let metronome =
                        types
                            .iter()
                            .find(|t| name(**t) == "metronome")
                            .is_some_and(|t| {
                                children(*t, "beat-unit").count() == 1
                                    && child(*t, "beat-unit-tied").is_none()
                                    && child_text(*t, "per-minute")
                                        .is_some_and(|p| !p.trim().is_empty())
                                    && child_text(*t, "beat-unit").is_some_and(|u| {
                                        matches!(
                                            u.trim(),
                                            "whole"
                                                | "half"
                                                | "quarter"
                                                | "eighth"
                                                | "16th"
                                                | "32nd"
                                                | "64th"
                                        )
                                    })
                            });
                    let words = types
                        .iter()
                        .filter(|t| name(**t) == "words")
                        .any(|t| !text(*t).trim().is_empty());
                    let dashes = types
                        .iter()
                        .any(|t| name(*t) == "dashes" && t.attribute("type") == Some("start"));
                    if metronome || (sound && words && !dashes) {
                        add("marker tempo");
                    } else if words && !dashes && !metronome {
                        add("marker text");
                    }
                    for t in &types {
                        let number = t.attribute("number").unwrap_or("1").to_owned();
                        let kind = t.attribute("type").unwrap_or("");
                        match name(*t) {
                            "dynamics" => {
                                for _ in elements(*t) {
                                    add("marker dynamic");
                                }
                            }
                            "rehearsal" => add("marker rehearsal"),
                            "segno" => add("marker segno"),
                            "coda" => add("marker coda"),
                            "wedge" => line(
                                ("wedge".into(), number),
                                kind == "crescendo" || kind == "diminuendo",
                                kind == "stop",
                                "spanner hairpin",
                            ),
                            "pedal" if t.attribute("line") != Some("no") => line(
                                ("pedal".into(), number),
                                kind == "start" || kind == "change",
                                kind == "stop" || kind == "change",
                                "spanner pedal",
                            ),
                            "octave-shift"
                                if matches!(
                                    t.attribute("size").unwrap_or("8"),
                                    "8" | "15" | "22"
                                ) || kind == "stop" =>
                            {
                                line(
                                    ("octave".into(), number),
                                    kind == "up" || kind == "down",
                                    kind == "stop",
                                    "spanner ottava",
                                )
                            }
                            "dashes" => line(
                                ("dashes".into(), number),
                                kind == "start",
                                kind == "stop",
                                "spanner text line",
                            ),
                            "bracket" => line(
                                ("bracket".into(), number),
                                kind == "start",
                                kind == "stop",
                                "spanner bracket",
                            ),
                            _ => {}
                        }
                    }
                }
                "note" => {
                    if child(item, "cue").is_some() {
                        if child(item, "chord").is_none() && child(item, "grace").is_none() {
                            cursor += child_text(item, "duration")
                                .and_then(|d| d.trim().parse::<i64>().ok())
                                .unwrap_or(0);
                        }
                        continue;
                    }
                    let staff = child_text(item, "staff").unwrap_or("1").to_owned();
                    let pitched = child(item, "pitch").is_some();
                    let rest = child(item, "rest").is_some();
                    let grace = child(item, "grace").is_some();
                    let graced = grace
                        && child_text(item, "type").is_some_and(|u| {
                            matches!(
                                u.trim(),
                                "whole" | "half" | "quarter" | "eighth" | "16th" | "32nd" | "64th"
                            )
                        })
                        && !rest;
                    if grace && !graced {
                        continue;
                    }
                    let chord = child(item, "chord").is_some();
                    let held = match (&event, chord) {
                        (Some((s, p, _, _, _)), true) => *s == staff && *p && pitched,
                        _ => true,
                    };
                    if !chord {
                        flush(&mut event, &mut add);
                        event = Some((
                            staff.clone(),
                            pitched,
                            rest,
                            BTreeSet::new(),
                            BTreeSet::new(),
                        ));
                        if graced {
                            add("grace");
                        }
                        if !grace {
                            cursor += child_text(item, "duration")
                                .and_then(|d| d.trim().parse::<i64>().ok())
                                .unwrap_or(0);
                        }
                    }
                    let Some((_, event_pitched, event_rest, kinds, verses)) = event.as_mut() else {
                        continue;
                    };
                    for lyric in children(item, "lyric") {
                        let numbered = lyric
                            .attribute("number")
                            .is_none_or(|n| n.trim().parse::<u16>().is_ok_and(|v| v >= 1));
                        let sung: String = children(lyric, "text").map(text).collect();
                        let verse = lyric
                            .attribute("number")
                            .and_then(|n| n.trim().parse::<u16>().ok())
                            .unwrap_or(1);
                        if held
                            && numbered
                            && child(lyric, "elision").is_none()
                            && !sung.is_empty()
                            && verses.insert(verse)
                        {
                            add("lyric");
                        }
                    }
                    for notations in children(item, "notations") {
                        for n in elements(notations) {
                            match name(n) {
                                "articulations" | "technical" if held && !*event_rest => {
                                    for a in elements(n) {
                                        let class = match name(a) {
                                            "accent" => "mark accent",
                                            "strong-accent" => "mark marcato",
                                            "staccato" => "mark staccato",
                                            "tenuto" => "mark tenuto",
                                            "detached-legato" => "mark detached legato",
                                            "staccatissimo" => "mark staccatissimo",
                                            "spiccato" => "mark spiccato",
                                            "scoop" => "mark scoop",
                                            "plop" => "mark plop",
                                            "doit" => "mark doit",
                                            "falloff" => "mark falloff",
                                            "stress" => "mark stress",
                                            "unstress" => "mark unstress",
                                            "up-bow" => "mark up-bow",
                                            "down-bow" => "mark down-bow",
                                            "harmonic" => "mark harmonic",
                                            "open-string" | "open" => "mark open",
                                            "stopped" => "mark stopped",
                                            "snap-pizzicato" => "mark snap pizzicato",
                                            _ => continue,
                                        };
                                        kinds.insert(class.to_owned());
                                    }
                                }
                                _ => {}
                            }
                            match name(n) {
                                "articulations" => {
                                    for a in elements(n) {
                                        match (name(a), text(a).trim()) {
                                            ("breath-mark", "" | "comma" | "tick") if held => {
                                                add("marker breath")
                                            }
                                            (
                                                "caesura",
                                                "" | "normal" | "single" | "thick" | "short"
                                                | "curved",
                                            ) if held => add("marker caesura"),
                                            _ => {}
                                        }
                                    }
                                }
                                "ornaments" if held => {
                                    for o in elements(n) {
                                        let number =
                                            o.attribute("number").unwrap_or("1").to_owned();
                                        match name(o) {
                                            "trill-mark" | "mordent" | "inverted-mordent"
                                            | "turn" | "inverted-turn"
                                                if *event_pitched =>
                                            {
                                                kinds.insert(format!("ornament {}", name(o)));
                                            }
                                            "tremolo" if !*event_rest => {
                                                let strokes = text(o)
                                                    .trim()
                                                    .parse::<u8>()
                                                    .ok()
                                                    .filter(|s| (1..=8).contains(s));
                                                match (
                                                    o.attribute("type").unwrap_or("single"),
                                                    strokes,
                                                ) {
                                                    ("single", Some(_)) => {
                                                        kinds.insert(String::from("mark tremolo"));
                                                    }
                                                    ("start", Some(_)) => {
                                                        kinds.insert(String::from(
                                                            "mark two-note tremolo",
                                                        ));
                                                    }
                                                    _ => {}
                                                }
                                            }
                                            "other-ornament"
                                                if o.attribute("smufl")
                                                    == Some("brassMuteClosed")
                                                    && !*event_rest =>
                                            {
                                                kinds.insert(String::from("mark stopped"));
                                            }
                                            "wavy-line" => line(
                                                ("wavy".into(), number),
                                                o.attribute("type") == Some("start"),
                                                o.attribute("type") == Some("stop"),
                                                "spanner trill line",
                                            ),
                                            _ => {}
                                        }
                                    }
                                }
                                "fermata" if held => {
                                    if matches!(
                                        text(n).trim(),
                                        "" | "normal"
                                            | "angled"
                                            | "square"
                                            | "double-angled"
                                            | "double-square"
                                    ) {
                                        add("marker fermata");
                                    }
                                }
                                "arpeggiate" if held && !*event_rest => {
                                    kinds.insert(String::from("mark arpeggio"));
                                }
                                "dynamics" if held => {
                                    for _ in elements(n) {
                                        add("marker dynamic");
                                    }
                                }
                                "slide" | "glissando" if held => {
                                    let kind = n.attribute("type").unwrap_or("");
                                    let known = matches!(
                                        n.attribute("line-type"),
                                        None | Some("solid" | "dashed" | "dotted" | "wavy")
                                    );
                                    line(
                                        (
                                            name(n).to_owned(),
                                            n.attribute("number").unwrap_or("1").to_owned(),
                                        ),
                                        kind == "start" && known,
                                        kind == "stop",
                                        "spanner glissando",
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        flush(&mut event, &mut add);
    }
    for class in lines {
        add(&class);
    }
    (counts, tempos)
}

fn timed_census(part: Node, census: &mut Census) {
    /// Semitones above C of the naturals, C to B.
    const NATURALS: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
    /// The alteration in quarter-tones an accidental name states with no
    /// `<alter>`: Stein's four by name, and an arrowed one as the accidental
    /// it names, raised or lowered a quarter-tone by its arrow. Any other
    /// name states no quarter-tone.
    fn named(name: &str) -> Option<i32> {
        match name {
            "quarter-flat" => return Some(-1),
            "quarter-sharp" => return Some(1),
            "three-quarters-flat" => return Some(-3),
            "three-quarters-sharp" => return Some(3),
            _ => {}
        }
        let (accidental, arrow) = match name.rsplit_once('-')? {
            (accidental, "up") => (accidental, 1),
            (accidental, "down") => (accidental, -1),
            _ => return None,
        };
        let semitones = match accidental {
            "flat-flat" => -2,
            "flat" => -1,
            "natural" => 0,
            "sharp" => 1,
            "double-sharp" => 2,
            _ => return None,
        };
        Some(2 * semitones + arrow)
    }
    // Keyed by the written staff, step and octave, as the file spells them;
    // for an unpitched note, its display step and octave.
    type Spot<'a> = (&'a str, &'a str, &'a str);
    // What a tie holds: a spot, the instrument of an unpitched note, and a
    // pitched note's alteration in quarter-tones.
    type Held<'a> = (Spot<'a>, Option<&'a str>, i32);
    struct Marked<'a> {
        onset: i64,
        end: i64,
        offset: Time,
        spot: Spot<'a>,
        voice: &'a str,
        /// The instrument of an unpitched note (`""` when it names none);
        /// `None` for a pitched one.
        unpitched: Option<&'a str>,
        /// A chord note the model cannot hold.
        dropped: bool,
        /// The alteration in quarter-tones its own `<alter>` or
        /// `<accidental>` states.
        own: Option<i32>,
        tie_start: bool,
        tie_stop: bool,
        /// The diatonic and chromatic steps from written to sounding.
        transpose: (i32, i32),
    }
    let mut found = Vec::new();
    // Per voice, the open tuplets as the untimed walk pairs them: each one's
    // number, its ratio when it is made, its first note's staff, and its
    // members so far.
    type OpenTuplet<'a> = (
        &'a str,
        Option<(u32, u32)>,
        usize,
        Vec<(usize, Time)>,
        (bool, bool),
    );
    let mut tuplets: BTreeMap<&str, Vec<OpenTuplet>> = BTreeMap::new();
    // Per voice, the members so far of its open primary beam, each its
    // staff, measure and offset; a beam begun again drops the open one, as
    // the untimed walk leaves it unmade.
    let mut beams: BTreeMap<&str, Vec<(usize, usize, Time)>> = BTreeMap::new();
    // Per spot, the voice and alteration of each tied pitch.
    type Starts<'a> = Vec<(&'a str, i32)>;
    let mut over: BTreeMap<Spot, Starts> = BTreeMap::new();
    let continued = |starts: Option<&Starts>, voice: &str| {
        let starts = starts?;
        let same = starts.iter().find(|(v, _)| *v == voice);
        same.or(starts.first()).map(|&(_, alteration)| alteration)
    };
    // The tie starts that end with their measure's furthest note, each with
    // whether it is a quarter-tone, waiting for the next measure's stops.
    let mut waiting: Vec<(Held, bool)> = Vec::new();
    // The kind and staff of the last note that is not a chord note: whether
    // it is pitched, and its `<staff>`.
    let mut head: Option<(bool, &str)> = None;
    let (mut divisions, mut transpose) = (1i64, (0i32, 0i32));
    for (index, measure) in children(part, "measure").enumerate() {
        let mut notes: Vec<Marked> = Vec::new();
        let (mut cursor, mut last, mut furthest) = (0i64, 0i64, 0i64);
        for item in elements(measure) {
            let duration = child_text(item, "duration")
                .and_then(|d| d.parse::<i64>().ok())
                .unwrap_or(0);
            match name(item) {
                "attributes" => {
                    if let Some(d) = child_text(item, "divisions")
                        .and_then(|d| d.parse::<i64>().ok())
                        .filter(|d| *d > 0)
                    {
                        divisions = d;
                    }
                    for interval in children(item, "transpose") {
                        let steps = |n: &str| {
                            child_text(interval, n)
                                .and_then(|v| v.parse::<i32>().ok())
                                .unwrap_or(0)
                        };
                        let octaves = steps("octave-change");
                        transpose = (
                            steps("diatonic") + 7 * octaves,
                            steps("chromatic") + 12 * octaves,
                        );
                    }
                }
                "backup" => cursor -= duration,
                "forward" => cursor += duration,
                "note" if child(item, "grace").is_none() => {
                    let chord = child(item, "chord").is_some();
                    if !chord {
                        last = cursor;
                        cursor += duration;
                        furthest = furthest.max(cursor);
                    }
                    if child(item, "cue").is_some() {
                        continue;
                    }
                    let (pitch, unpitched) = (child(item, "pitch"), child(item, "unpitched"));
                    let staff = child_text(item, "staff").unwrap_or("1");
                    if !chord {
                        let voice = child_text(item, "voice").unwrap_or("1");
                        let stack = tuplets.entry(voice).or_default();
                        let marks: Vec<Node> = children(item, "notations")
                            .flat_map(|n| children(n, "tuplet"))
                            .collect();
                        for mark in marks
                            .iter()
                            .filter(|t| t.attribute("type") == Some("start"))
                        {
                            let ratio = child(item, "time-modification").and_then(|m| {
                                let actual =
                                    child_text(m, "actual-notes")?.trim().parse::<u32>().ok()?;
                                let normal =
                                    child_text(m, "normal-notes")?.trim().parse::<u32>().ok()?;
                                (actual != 0 && normal != 0 && actual != normal)
                                    .then_some((actual, normal))
                            });
                            let made = ratio.filter(|_| stack.is_empty());
                            let first = staff.parse::<usize>().map_or(0, |s| s.saturating_sub(1));
                            let number = mark.attribute("number").unwrap_or("1");
                            let hidden = mark.parent().and_then(|n| n.attribute("print-object"))
                                == Some("no");
                            let numberless = mark.attribute("show-number") == Some("none");
                            stack.push((number, made, first, Vec::new(), (hidden, numberless)));
                        }
                        let at = RationalTime::new(last, 4 * divisions)
                            .unwrap_or_else(RationalTime::zero);
                        for open in stack.iter_mut() {
                            open.3.push((index, at.clone()));
                        }
                        for mark in marks.iter().filter(|t| t.attribute("type") == Some("stop")) {
                            let number = mark.attribute("number").unwrap_or("1");
                            if let Some(at) = stack.iter().rposition(|open| open.0 == number) {
                                if let (_, Some(ratio), staff, members, (hidden, numberless)) =
                                    stack.remove(at)
                                {
                                    census.tuplet_places.push(CensusTuplet {
                                        staff,
                                        ratio,
                                        members,
                                        hidden,
                                        numberless,
                                    });
                                }
                            }
                        }
                        let member = (
                            staff.parse::<usize>().map_or(0, |s| s.saturating_sub(1)),
                            index,
                            at,
                        );
                        let primary = children(item, "beam")
                            .find(|b| b.attribute("number").is_none_or(|n| n == "1"))
                            .map(text);
                        match primary {
                            Some("begin") => {
                                beams.insert(voice, vec![member]);
                            }
                            Some("continue") => {
                                if let Some(open) = beams.get_mut(voice) {
                                    open.push(member);
                                }
                            }
                            Some("end") => {
                                if let Some(mut members) = beams.remove(voice) {
                                    members.push(member);
                                    census.beam_places.push(CensusBeam { members });
                                }
                            }
                            _ => {}
                        }
                    }
                    let ties = |kind: &str| {
                        children(item, "tie").any(|t| t.attribute("type") == Some(kind))
                    };
                    let dropped = if chord {
                        !(pitch.is_some() && head == Some((true, staff)))
                    } else {
                        head = Some((pitch.is_some(), staff));
                        false
                    };
                    if dropped {
                        let rest = pitch.is_none() && unpitched.is_none();
                        if pitch.is_some() {
                            census.dropped.pitched += 1;
                        } else if unpitched.is_some() {
                            census.dropped.unpitched += 1;
                        } else if child(item, "rest").is_some() {
                            census.dropped.rests += 1;
                        }
                        census.dropped_tie_starts += usize::from(ties("start") && !rest);
                    }
                    let offset =
                        RationalTime::new(last, 4 * divisions).unwrap_or_else(RationalTime::zero);
                    if let Some(display) = unpitched.filter(|_| !dropped) {
                        notes.push(Marked {
                            onset: last,
                            end: last + duration,
                            offset,
                            spot: (
                                staff,
                                child_text(display, "display-step").unwrap_or(""),
                                child_text(display, "display-octave").unwrap_or(""),
                            ),
                            voice: child_text(item, "voice").unwrap_or("1"),
                            unpitched: Some(
                                child(item, "instrument")
                                    .and_then(|i| i.attribute("id"))
                                    .unwrap_or(""),
                            ),
                            dropped,
                            own: None,
                            tie_start: ties("start"),
                            tie_stop: ties("stop"),
                            transpose,
                        });
                        continue;
                    }
                    let Some(pitch) = pitch else {
                        continue;
                    };
                    let accidental = child_text(item, "accidental");
                    let own = match (child_text(pitch, "alter"), accidental) {
                        (Some(alter), _) => Some(
                            alter
                                .parse::<f64>()
                                .ok()
                                .map(|a| a * 2.0)
                                .filter(|q| q.fract() == 0.0 && q.abs() <= 24.0)
                                .map_or(0, |q| q as i32),
                        ),
                        (None, Some(name)) => Some(named(name).unwrap_or(0)),
                        (None, None) => None,
                    };
                    let spot = (
                        staff,
                        child_text(pitch, "step").unwrap_or(""),
                        child_text(pitch, "octave").unwrap_or(""),
                    );
                    notes.push(Marked {
                        onset: last,
                        end: last + duration,
                        offset,
                        spot,
                        voice: child_text(item, "voice").unwrap_or("1"),
                        unpitched: None,
                        dropped,
                        own,
                        tie_start: ties("start"),
                        tie_stop: ties("stop"),
                        transpose,
                    });
                }
                _ => {}
            }
            furthest = furthest.max(cursor);
        }
        notes.sort_by_key(|n| n.onset);
        let incoming = std::mem::take(&mut over);
        let mut ending: BTreeMap<(Spot, i64), Starts> = BTreeMap::new();
        // The stops of the notes kept, and the starts, each with its end and
        // whether it is a quarter-tone.
        let mut stops: BTreeSet<(Held, i64)> = BTreeSet::new();
        let mut starts: Vec<(Held, i64, bool)> = Vec::new();
        for note in &notes {
            let Marked { onset, spot, .. } = *note;
            if note.unpitched.is_some() {
                let held = (spot, note.unpitched, 0);
                if note.tie_stop {
                    stops.insert((held, onset));
                }
                if note.tie_start {
                    starts.push((held, note.end, false));
                }
                continue;
            }
            // A quarter-tone accidental applies to its own note, and over a
            // tie to the note continuing it, never to a later note of its
            // line (roadmap D40).
            let alteration = note.own.unwrap_or_else(|| {
                let tied = if !note.tie_stop {
                    None
                } else if onset == 0 {
                    continued(incoming.get(&spot), note.voice)
                } else {
                    continued(ending.get(&(spot, onset)), note.voice)
                };
                tied.filter(|alteration| alteration % 2 != 0).unwrap_or(0)
            });
            if note.tie_start {
                let starts = ending.entry((spot, note.end)).or_default();
                starts.push((note.voice, alteration));
                if note.end >= furthest {
                    over.entry(spot).or_default().push((note.voice, alteration));
                }
            }
            if !note.dropped {
                let held = (spot, None, alteration);
                if note.tie_stop {
                    stops.insert((held, onset));
                }
                if note.tie_start {
                    starts.push((held, note.end, alteration % 2 != 0));
                }
            }
            if alteration % 2 == 0 {
                continue;
            }
            // The written pitch, counted in diatonic steps and in quarter-tones
            // from C0, moved by the transposition to the sounding one.
            let step = ["C", "D", "E", "F", "G", "A", "B"]
                .iter()
                .position(|s| *s == spot.1);
            let (Some(step), Ok(octave)) = (step, spot.2.parse::<i32>()) else {
                continue;
            };
            let (diatonic, chromatic) = note.transpose;
            let degree = 7 * octave + step as i32 + diatonic;
            let height = 24 * octave + 2 * NATURALS[step] + alteration + 2 * chromatic;
            let (octave, nominal) = (degree.div_euclid(7), degree.rem_euclid(7));
            found.push(QuarterTone {
                measure: index,
                offset: note.offset.clone(),
                staff: spot.0.parse::<usize>().map_or(0, |s| s.saturating_sub(1)),
                nominal: nominal as u8,
                quarter_tones: (height - 24 * octave - 2 * NATURALS[nominal as usize]) as i16,
                octave: octave as i8,
            });
        }
        // A tie start is ended by a stop of the same holding where it ends:
        // the previous measure's last notes at this one's start.
        let mut ended = |held: &Held, at: i64, quarter: bool| {
            if stops.contains(&(*held, at)) {
                census.quarter_tone_ties += usize::from(quarter);
            } else {
                census.unended_ties += 1;
            }
        };
        for (held, quarter) in std::mem::take(&mut waiting) {
            ended(&held, 0, quarter);
        }
        for (held, end, quarter) in starts {
            if end >= furthest {
                waiting.push((held, quarter));
            } else {
                ended(&held, end, quarter);
            }
        }
    }
    census.unended_ties += waiting.len();
    census.quarter_tones = found;
}

/// The keys and clefs of a part's `<attributes>`, per staff, read straight
/// from the elements, each at its measure and at an offset the census times
/// itself from the notes, `<backup>` and `<forward>` before it. It shares
/// none of the reader's order of reading, so it holds the reader's placement
/// of them to account: on which staff, and when. A key is counted at concert
/// pitch, and an open one (`<mode>none</mode>`) only after a key signature,
/// as the model holds them.
#[allow(clippy::type_complexity)]
fn attribute_census(part: Node, concert: bool) -> (Vec<Vec<Stated<i8>>>, Vec<Vec<Stated<Clef>>>) {
    let shift = concert_key_shift(part, concert);
    let attributes = || children(part, "measure").flat_map(|m| children(m, "attributes"));
    let staves = attributes()
        .flat_map(|a| children(a, "staves"))
        .filter_map(|s| text(s).parse::<usize>().ok())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut keys = vec![Vec::new(); staves];
    let mut clefs = vec![Vec::new(); staves];
    fn stated<T>(measure: usize, offset: &Time, value: T) -> Stated<T> {
        Stated {
            measure,
            offset: offset.clone(),
            value,
        }
    }
    let mut divisions = 1i64;
    for (index, measure) in children(part, "measure").enumerate() {
        let mut cursor = 0i64;
        for a in elements(measure) {
            let duration = child_text(a, "duration")
                .and_then(|d| d.parse::<i64>().ok())
                .unwrap_or(0);
            match name(a) {
                "backup" => cursor -= duration,
                "forward" => cursor += duration,
                "note" if child(a, "grace").is_none() && child(a, "chord").is_none() => {
                    cursor += duration;
                }
                _ => {}
            }
            if name(a) != "attributes" {
                continue;
            }
            if let Some(d) = child_text(a, "divisions")
                .and_then(|d| d.parse::<i64>().ok())
                .filter(|d| *d > 0)
            {
                divisions = d;
            }
            let offset =
                RationalTime::new(cursor, 4 * divisions).unwrap_or_else(RationalTime::zero);
            for key in children(a, "key") {
                let Some(fifths) = child_text(key, "fifths")
                    .and_then(|f| f.parse::<i8>().ok())
                    .filter(|f| (-7..=7).contains(f))
                else {
                    continue;
                };
                let open = child_text(key, "mode") == Some("none");
                let fifths = if open {
                    0
                } else {
                    match i8::try_from(i32::from(fifths) + shift)
                        .ok()
                        .filter(|f| (-7..=7).contains(f))
                    {
                        Some(concert) => concert,
                        None => continue,
                    }
                };
                let push = |list: &mut Vec<Stated<i8>>| {
                    if !(open && list.is_empty()) {
                        list.push(stated(index, &offset, fifths));
                    }
                };
                match key.attribute("number") {
                    None => keys.iter_mut().for_each(push),
                    Some(n) => {
                        let staff = n.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
                        if let Some(list) = staff.and_then(|s| keys.get_mut(s)) {
                            push(list);
                        }
                    }
                }
            }
            for clef in children(a, "clef") {
                let line = child_text(clef, "line").and_then(|l| l.parse::<i8>().ok());
                let octave_shift = child_text(clef, "clef-octave-change")
                    .and_then(|o| o.parse::<i8>().ok())
                    .unwrap_or(0);
                let (shape, default_line) = match child_text(clef, "sign") {
                    Some("G") => (ClefShape::G, 2),
                    Some("F") => (ClefShape::F, 4),
                    Some("C") => (ClefShape::C, 3),
                    Some("percussion") => (ClefShape::Percussion, 3),
                    _ => continue,
                };
                let value = if shape == ClefShape::Percussion {
                    Clef {
                        shape,
                        line: 3,
                        octave_shift: 0,
                    }
                } else {
                    Clef {
                        shape,
                        line: line.unwrap_or(default_line),
                        octave_shift,
                    }
                };
                let staff = match clef.attribute("number") {
                    None => Some(0),
                    Some(n) => n.parse::<usize>().ok().and_then(|n| n.checked_sub(1)),
                };
                if let Some(list) = staff.and_then(|s| clefs.get_mut(s)) {
                    list.push(stated(index, &offset, value));
                }
            }
        }
    }
    (keys, clefs)
}

/// A part-list entry, in order: a part's id, or a part-group's number and,
/// for a start, its symbol.
type ListEntry<'a> = (&'a str, Option<(String, Option<String>)>);

/// What a staff group draws at its left: a brace (a grand staff), a bracket,
/// or a thin square sub-bracket.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum GroupKind {
    Brace,
    Bracket,
    SubBracket,
}

/// A staff group as read: its kind and its staves, each `(part, staff)`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceGroup {
    pub kind: GroupKind,
    pub staves: Vec<(usize, usize)>,
}

/// The staff groups a file's part-list and parts make, counted by a walk of
/// their elements the reader does not run: groups a staff can be held in, and
/// `<part-group>` starts that make none (the model holds a staff in at most
/// one group).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GroupCensus {
    /// Groups made, by kind: braces, brackets, sub-brackets.
    pub made: [usize; 3],
    pub unmade: usize,
}

/// The staff groups a part-list and its parts make, by a walk of their
/// elements of its own: a part whose `<staves>` exceed one is a brace; then
/// each brace and bracket `<part-group>` makes a group if any of its parts'
/// staves is not yet held, and a square one if none is; anything else, or a
/// group restarted or never stopped, makes none.
fn group_census(part_list: Node, parts: &[Node]) -> GroupCensus {
    let staves: BTreeMap<&str, usize> = parts
        .iter()
        .map(|part| {
            let count = children(*part, "measure")
                .flat_map(|m| children(m, "attributes"))
                .flat_map(|a| children(a, "staves"))
                .filter_map(|s| text(s).parse::<usize>().ok())
                .max()
                .unwrap_or(1);
            (part.attribute("id").unwrap_or(""), count)
        })
        .collect();
    let mut census = GroupCensus::default();
    let mut held: BTreeSet<(&str, usize)> = BTreeSet::new();
    for (&id, &count) in &staves {
        if count >= 2 {
            census.made[0] += 1;
            held.extend((0..count).map(|s| (id, s)));
        }
    }
    let mut open: BTreeMap<&str, (&str, Vec<&str>)> = BTreeMap::new();
    let mut closed: Vec<(&str, Vec<&str>)> = Vec::new();
    for entry in elements(part_list) {
        match (name(entry), entry.attribute("type")) {
            ("score-part", _) => {
                let id = entry.attribute("id").unwrap_or("");
                if staves.contains_key(id) {
                    open.values_mut().for_each(|(_, ids)| ids.push(id));
                }
            }
            ("part-group", Some("start")) => {
                let symbol = child_text(entry, "group-symbol").unwrap_or("none");
                let number = entry.attribute("number").unwrap_or("1");
                if open.insert(number, (symbol, Vec::new())).is_some() {
                    census.unmade += 1;
                }
            }
            ("part-group", Some("stop")) => {
                let number = entry.attribute("number").unwrap_or("1");
                if let Some(group) = open.remove(number) {
                    closed.push(group);
                }
            }
            _ => {}
        }
    }
    census.unmade += open.len();
    for (kind, wanted) in ["brace", "bracket", "square"].into_iter().enumerate() {
        for (_, ids) in closed.iter().filter(|(symbol, _)| *symbol == wanted) {
            let all: Vec<(&str, usize)> = ids
                .iter()
                .flat_map(|id| (0..staves[id]).map(move |s| (*id, s)))
                .collect();
            let free: Vec<(&str, usize)> =
                all.iter().copied().filter(|s| !held.contains(s)).collect();
            let makes = !free.is_empty() && (wanted != "square" || free.len() == all.len());
            if makes {
                census.made[kind] += 1;
                held.extend(free);
            } else {
                census.unmade += 1;
            }
        }
    }
    census.unmade += closed
        .iter()
        .filter(|(symbol, _)| !matches!(*symbol, "brace" | "bracket" | "square"))
        .count();
    census
}

/// A partwise MusicXML score as the file states it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceScore {
    pub title: Option<String>,
    pub composer: Option<String>,
    /// `<defaults><concert-score/>`: the file writes transposing parts at
    /// concert pitch.
    pub concert: bool,
    /// `<defaults><page-layout>`: the page the file sets the score on, when
    /// it gives one whole.
    pub page: Option<SourcePage>,
    pub parts: Vec<SourcePart>,
    pub measures: Vec<SourceMeasure>,
    /// The meter changes of the first part, which govern the score.
    pub meters: Vec<MeterChange>,
    pub features: Features,
    /// Per part, in part order.
    pub census: Vec<Census>,
    /// The staff groups: a brace for each part of two or more staves, then
    /// the part-list's brackets, braces and square sub-brackets over the
    /// staves no group yet holds.
    pub groups: Vec<SourceGroup>,
    /// The part-list's `<part-group>`s that make no group, each recorded: a
    /// symbol the model has no group for, a square sub-bracket inside another
    /// group, a group with no staff left to hold, or one never stopped.
    pub unmade_groups: usize,
    /// The census's own count of the same.
    pub group_census: GroupCensus,
}

/// A page as `<defaults><page-layout>` sets it, in staff spaces: a tenth is a
/// tenth of a staff space whatever `<scaling>` makes it in millimeters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourcePage {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

// Every field is finite (`read` admits no other), so equality is total.
impl Eq for SourcePage {}

impl SourcePage {
    /// The page of a `<page-layout>`: its size and its first margins (the
    /// odd pages' where odd and even differ), or `None` when any is missing
    /// or the content area would be empty.
    fn read(layout: Node) -> Option<SourcePage> {
        let tenths = |node: Node, name: &str| -> Option<f32> {
            let v: f32 = child_text(node, name)?.trim().parse().ok()?;
            (v.is_finite() && v >= 0.0).then_some(v / 10.0)
        };
        let margins = elements(layout)
            .filter(|n| name(*n) == "page-margins")
            .find(|n| n.attribute("type") != Some("even"))?;
        let page = SourcePage {
            width: tenths(layout, "page-width")?,
            height: tenths(layout, "page-height")?,
            left: tenths(margins, "left-margin")?,
            right: tenths(margins, "right-margin")?,
            top: tenths(margins, "top-margin")?,
            bottom: tenths(margins, "bottom-margin")?,
        };
        (page.width > page.left + page.right && page.height > page.top + page.bottom)
            .then_some(page)
    }
}

impl SourceScore {
    /// Reads a partwise MusicXML document.
    pub fn read(xml: &str) -> Result<SourceScore, ReadError> {
        let options = ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        };
        let doc = Document::parse_with_options(xml, options)
            .map_err(|e| ReadError::Xml(e.to_string()))?;
        let root = doc.root_element();
        if root.tag_name().name() != "score-partwise" {
            return Err(ReadError::NotPartwise(root.tag_name().name().to_owned()));
        }
        Reader::new(&doc).read(root)
    }
}

// --- Small readers. ---------------------------------------------------------

fn line_of(doc: &Document, node: Node) -> u32 {
    doc.text_pos_at(node.range().start).row
}

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a, 'i: 'a>(
    node: Node<'a, 'i>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn elements<'a, 'i: 'a>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children().filter(|c| c.is_element())
}

fn text<'a>(node: Node<'a, '_>) -> &'a str {
    node.text().unwrap_or("").trim()
}

fn child_text<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).map(text)
}

fn name<'a>(node: Node<'a, '_>) -> &'a str {
    node.tag_name().name()
}

fn nominal_of(step: &str) -> Option<CmnNominal> {
    Some(match step {
        "C" => CmnNominal::C,
        "D" => CmnNominal::D,
        "E" => CmnNominal::E,
        "F" => CmnNominal::F,
        "G" => CmnNominal::G,
        "A" => CmnNominal::A,
        "B" => CmnNominal::B,
        _ => return None,
    })
}

/// A CMN pitch in `cmn-12`, as the importer writes every pitch but a
/// quarter-tone.
pub fn cmn_pitch(nominal: CmnNominal, alteration: i8, octave: i8) -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Cmn {
                nominal,
                alteration,
                octave,
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

/// A CMN pitch whose alteration is counted in quarter-tones: in `cmn-12` when
/// it is a whole number of semitones, and otherwise in `cmn-24`, whose
/// chromatic step is the quarter-tone (a quarter-flat is `-1` there, a flat
/// `-2`).
pub fn quarter_tone_pitch(nominal: CmnNominal, quarter_tones: i8, octave: i8) -> Pitch {
    if quarter_tones % 2 == 0 {
        return cmn_pitch(nominal, quarter_tones / 2, octave);
    }
    let mut pitch = cmn_pitch(nominal, quarter_tones, octave);
    pitch.scale_position.space = PitchSpaceId::new("cmn-24");
    pitch
}

/// The accidental a quarter-tone is spelt with at its sounding pitch, from
/// the one the file writes or carries to it (`epiphany_core`'s, which the
/// reducer shares to move an authored quarter-tone spelling with a
/// transposition).
pub use epiphany_core::quarter_tone_accidental;
use epiphany_core::{quarter_tone_name, quarter_tones_named};

/// The spelling of a sounding quarter-tone, in `cmn-24` with an odd
/// alteration, from the accidental the file writes or carries to it.
fn quarter_tone_spelling(pitch: &Pitch, written: Option<&str>) -> Option<PitchSpelling> {
    if pitch.scale_position.space.as_str() != "cmn-24" {
        return None;
    }
    let PitchSpacePosition::Cmn {
        nominal,
        alteration,
        octave,
    } = pitch.scale_position.position
    else {
        return None;
    };
    if alteration % 2 == 0 {
        return None;
    }
    Some(PitchSpelling {
        nominal: SpellingNominal::Cmn(nominal),
        accidentals: vec![AccidentalId::new(quarter_tone_accidental(
            written, alteration,
        )?)],
        octave,
        render_hints: Default::default(),
    })
}

/// An accidental of whole semitones, whose alteration a file states in
/// `<alter>`.
fn semitone_accidental(name: &str) -> bool {
    matches!(
        name,
        "natural"
            | "sharp"
            | "flat"
            | "double-sharp"
            | "sharp-sharp"
            | "flat-flat"
            | "natural-sharp"
            | "natural-flat"
            | "triple-sharp"
            | "triple-flat"
    )
}

/// `written` moved by `transpose`, a MusicXML interval counted in semitones,
/// which a `cmn-24` pitch moves by in quarter-tones.
fn sounding(
    written: Pitch,
    transpose: Option<TranspositionInterval>,
) -> Result<Pitch, epiphany_core::TransposeRefusal> {
    let Some(mut interval) = transpose else {
        return Ok(written);
    };
    if written.scale_position.space.as_str() == "cmn-24" {
        interval.chromatic_steps *= 2;
    }
    written.transposed(interval)
}

/// The staff step of a written position read against a treble clef: E4, the
/// bottom line, is 0, and each diatonic step is 1 (the engraver's convention).
fn treble_step(nominal: CmnNominal, octave: i8) -> i16 {
    let index = i16::from(octave) * 7 + nominal as i16;
    index - (4 * 7 + CmnNominal::E as i16)
}

fn zero() -> Time {
    RationalTime::zero()
}

// --- The reader. --------------------------------------------------------------

struct OpenTuplet {
    number: String,
    ratio: Option<(u32, u32)>,
    events: Vec<usize>,
    display: TupletDisplay,
}

struct PartState {
    divisions: i64,
    /// What is added to a pitch in the file to reach the sounding pitch.
    file_transpose: Option<TranspositionInterval>,
    /// The fifths added to a key the file writes to reach the concert key:
    /// zero in a concert score, and in a transposed one the part's first
    /// `<transpose>`'s, read before any key since `<transpose>` follows
    /// `<key>` within an `<attributes>`.
    key_shift: i32,
    /// Open slurs by number: the index of their start event.
    open_slurs: BTreeMap<String, usize>,
    /// Open beams by voice: the indices of their events so far.
    open_beams: BTreeMap<String, Vec<usize>>,
    /// Open tuplets by voice, innermost last: each one's number, its ratio
    /// when it will be made (`None` for one recorded unmade), and the
    /// indices of its events so far.
    open_tuplets: BTreeMap<String, Vec<OpenTuplet>>,
    /// The last event a `<chord/>` note would join.
    last_event: Option<usize>,
    /// The measure's pitches as written, for [`Reader::carry`].
    written: Vec<WrittenPitch>,
    /// The pitches tied over the last barline, by staff, nominal and octave.
    tied_over: BTreeMap<(usize, u8, i8), Tied>,
    /// The next grace note's order at each place: measure, staff, voice and
    /// position in divisions.
    grace_orders: BTreeMap<(usize, usize, String, i64), u16>,
    /// Lines begun and not yet ended, by kind and number.
    open_lines: BTreeMap<(&'static str, String), OpenLine>,
}

/// A line begun: where, on which staff, and what it is.
struct OpenLine {
    start: SourcePoint,
    staff: usize,
    kind: SpannerKind,
    line: LineStyle,
}

/// Tied pitches at one place: each one's voice, its alteration in
/// quarter-tones and the quarter-tone accidental it is written with.
type Tied = Vec<(String, i8, Option<&'static str>)>;

/// A pitch as the file writes it, kept until its measure is read.
struct WrittenPitch {
    /// Its event and its place among the event's pitches; `None` for a
    /// chord note the reader drops.
    at: Option<(usize, usize)>,
    staff: usize,
    voice: String,
    /// The start and end within the measure.
    onset: Time,
    end: Time,
    nominal: CmnNominal,
    octave: i8,
    /// The quarter-tones its own `<alter>` or `<accidental>` states.
    stated: Option<i8>,
    /// Its own `<accidental>`, when that names a quarter-tone accidental.
    named: Option<&'static str>,
    tie_start: bool,
    tie_stop: bool,
    transpose: Option<TranspositionInterval>,
    offset: usize,
}

struct Reader<'d, 'i> {
    doc: &'d Document<'i>,
    features: Features,
}

/// A part read with measure-relative times; onsets are made absolute once
/// every part's measure lengths are known.
struct PartRead {
    part: SourcePart,
    /// Per measure: the furthest the content reaches, in whole notes.
    lengths: Vec<Time>,
    /// Per measure: the `number` and `implicit` attributes.
    headers: Vec<(String, bool)>,
    meters: Vec<(usize, Time, Meter)>,
    /// The offsets of events, clefs and keys within their measure.
    event_offsets: Vec<Time>,
    clef_offsets: Vec<Vec<Time>>,
    key_offsets: Vec<Vec<Time>>,
    census: Census,
}

impl<'d, 'i> Reader<'d, 'i> {
    fn new(doc: &'d Document<'i>) -> Self {
        Reader {
            doc,
            features: Features::default(),
        }
    }

    fn line(&self, node: Node) -> u32 {
        line_of(self.doc, node)
    }

    fn malformed(&self, node: Node, what: impl Into<String>) -> ReadError {
        ReadError::Malformed(self.line(node), what.into())
    }

    fn unsupported(&self, node: Node, what: impl Into<String>) -> ReadError {
        ReadError::Unsupported(self.line(node), what.into())
    }

    fn read(mut self, root: Node) -> Result<SourceScore, ReadError> {
        let score_place = || Place {
            part: String::from("(score)"),
            measure: String::new(),
        };
        let mut title = None;
        let mut composer = None;
        let mut concert = false;
        let mut page = None;
        let mut score_parts: BTreeMap<String, Node> = BTreeMap::new();
        let mut part_nodes = Vec::new();
        // The part-list in order: each part's id, and each group start
        // (number, symbol) and stop (number).
        let mut list: Vec<ListEntry> = Vec::new();
        let mut part_list = None;
        for node in elements(root) {
            match name(node) {
                "work" => {
                    title = child_text(node, "work-title").map(str::to_owned);
                    for other in elements(node).filter(|n| name(*n) != "work-title") {
                        self.features.record(
                            FeatureClass::Presentation,
                            format!("work: {}", name(other)),
                            score_place(),
                        );
                    }
                }
                "movement-title" => {
                    if title.is_none() {
                        title = Some(text(node).to_owned());
                    } else {
                        self.features.record(
                            FeatureClass::Presentation,
                            "movement-title",
                            score_place(),
                        );
                    }
                }
                "identification" => {
                    for part in elements(node) {
                        if name(part) == "creator" && part.attribute("type") == Some("composer") {
                            composer = Some(text(part).to_owned());
                        } else {
                            self.features.record(
                                FeatureClass::Presentation,
                                format!("identification: {}", name(part)),
                                score_place(),
                            );
                        }
                    }
                }
                "defaults" => {
                    for part in elements(node) {
                        if name(part) == "concert-score" {
                            concert = true;
                        } else if name(part) == "page-layout" && page.is_none() {
                            // The page is the renderer's to cast off against;
                            // the score graph does not hold it.
                            page = SourcePage::read(part);
                            self.features.record(
                                FeatureClass::Presentation,
                                "defaults: page-layout",
                                score_place(),
                            );
                        } else {
                            self.features.record(
                                FeatureClass::Presentation,
                                format!("defaults: {}", name(part)),
                                score_place(),
                            );
                        }
                    }
                }
                "credit" => {
                    self.features
                        .record(FeatureClass::Presentation, "credit", score_place());
                }
                "part-list" => {
                    part_list = Some(node);
                    for entry in elements(node) {
                        match name(entry) {
                            "score-part" => {
                                let id = entry.attribute("id").unwrap_or("");
                                list.push((id, None));
                                score_parts.insert(id.to_owned(), entry);
                            }
                            "part-group" => {
                                let number = entry.attribute("number").unwrap_or("1").to_owned();
                                let kind = entry.attribute("type").unwrap_or("");
                                let symbol = (kind == "start").then(|| {
                                    child_text(entry, "group-symbol")
                                        .unwrap_or("none")
                                        .to_owned()
                                });
                                if kind == "start" || kind == "stop" {
                                    list.push(("", Some((number, symbol))));
                                }
                            }
                            other => self.features.record(
                                FeatureClass::Presentation,
                                format!("part-list: {other}"),
                                score_place(),
                            ),
                        }
                    }
                }
                "part" => part_nodes.push(node),
                other => self.features.record(
                    FeatureClass::Presentation,
                    format!("score: {other}"),
                    score_place(),
                ),
            }
        }

        let mut reads = Vec::new();
        for &node in &part_nodes {
            let id = node.attribute("id").unwrap_or("").to_owned();
            let Some(declaration) = score_parts.get(&id).copied() else {
                return Err(self.malformed(node, format!("part {id:?} is not in the part-list")));
            };
            reads.push(self.read_part(node, declaration, concert)?);
        }

        // Measures are shared across parts by index: each measure's length is
        // the furthest any part reaches in it, and onsets accumulate from them.
        let count = reads.iter().map(|r| r.lengths.len()).max().unwrap_or(0);
        let mut measures = Vec::with_capacity(count);
        let mut onset = zero();
        for index in 0..count {
            let length = reads
                .iter()
                .filter_map(|r| r.lengths.get(index))
                .max()
                .cloned()
                .unwrap_or_else(zero);
            let (number, implicit) = reads
                .iter()
                .find_map(|r| r.headers.get(index).cloned())
                .unwrap_or_default();
            measures.push(SourceMeasure {
                number,
                onset: onset.clone(),
                length: length.clone(),
                implicit,
            });
            onset = onset.add(&length);
        }
        for read in &reads {
            if read.lengths.len() != count {
                self.features.record(
                    FeatureClass::Content,
                    "part with a different measure count",
                    Place {
                        part: read.part.name.clone(),
                        measure: String::new(),
                    },
                );
            }
        }

        let mut meters = Vec::new();
        if let Some(first) = reads.first() {
            for (measure, offset, meter) in &first.meters {
                meters.push(MeterChange {
                    onset: measures[*measure].onset.add(offset),
                    measure: *measure,
                    meter: meter.clone(),
                });
            }
            for read in &reads[1..] {
                let theirs: Vec<(usize, &Meter)> = read
                    .meters
                    .iter()
                    .map(|(m, _, meter)| (*m, meter))
                    .collect();
                let ours: Vec<(usize, &Meter)> = first
                    .meters
                    .iter()
                    .map(|(m, _, meter)| (*m, meter))
                    .collect();
                if theirs != ours {
                    self.features.record(
                        FeatureClass::Content,
                        "meter differing from the first part's (polymeter)",
                        Place {
                            part: read.part.name.clone(),
                            measure: String::new(),
                        },
                    );
                }
            }
        }

        let mut parts = Vec::with_capacity(reads.len());
        let mut census = Vec::with_capacity(reads.len());
        for read in reads {
            let mut part = read.part;
            for (event, offset) in part.events.iter_mut().zip(&read.event_offsets) {
                event.onset = measures[event.measure].onset.add(offset);
            }
            // A mark placed by time was read as an offset into its measure.
            let place = |point: &mut SourcePoint| {
                if let SourcePoint::At { measure, onset, .. }
                | SourcePoint::Barline { measure, onset } = point
                {
                    *onset = measures[*measure].onset.add(onset);
                }
            };
            for marker in &mut part.markers {
                place(&mut marker.at);
            }
            for spanner in &mut part.spanners {
                place(&mut spanner.start);
                place(&mut spanner.end);
            }
            for tempo in &mut part.tempos {
                place(&mut tempo.at);
            }
            for (staff, offsets) in part.staves.iter_mut().zip(&read.clef_offsets) {
                for (clef, offset) in staff.clefs.iter_mut().zip(offsets) {
                    clef.onset = measures[clef.measure].onset.add(offset);
                }
            }
            for (staff, offsets) in part.staves.iter_mut().zip(&read.key_offsets) {
                for (key, offset) in staff.keys.iter_mut().zip(offsets) {
                    key.onset = measures[key.measure].onset.add(offset);
                }
            }
            parts.push(part);
            census.push(read.census);
        }

        let (groups, unmade_groups) = self.groups(&list, &parts);
        let group_census =
            part_list.map_or_else(GroupCensus::default, |node| group_census(node, &part_nodes));
        Ok(SourceScore {
            title,
            composer,
            concert,
            page,
            parts,
            measures,
            meters,
            features: self.features,
            census,
            groups,
            unmade_groups,
            group_census,
        })
    }

    /// The staff groups of the part-list `list` over `parts`, and how many of
    /// its `<part-group>`s made none, each recorded. A staff is held in at most
    /// one group: a part of two or more staves is a brace; then each brace and
    /// bracket of the part-list holds those of its staves no group yet holds,
    /// and a square sub-bracket all of its staves, if no group holds any.
    fn groups(&mut self, list: &[ListEntry], parts: &[SourcePart]) -> (Vec<SourceGroup>, usize) {
        let place = || Place {
            part: String::new(),
            measure: String::new(),
        };
        let index: BTreeMap<&str, usize> = parts
            .iter()
            .enumerate()
            .map(|(p, part)| (part.id.as_str(), p))
            .collect();
        // Each part-group's symbol and the parts between its start and stop.
        let mut open: BTreeMap<&str, (String, Vec<usize>)> = BTreeMap::new();
        let mut spans: Vec<(String, Vec<usize>)> = Vec::new();
        let mut unmade = 0;
        for (id, group) in list {
            match group {
                None => {
                    if let Some(&p) = index.get(id) {
                        open.values_mut().for_each(|(_, parts)| parts.push(p));
                    }
                }
                Some((number, Some(symbol))) => {
                    if open
                        .insert(number.as_str(), (symbol.clone(), Vec::new()))
                        .is_some()
                    {
                        unmade += 1;
                        self.features.record(
                            FeatureClass::Notation,
                            "part group begun again before its stop",
                            place(),
                        );
                    }
                }
                Some((number, None)) => {
                    if let Some(span) = open.remove(number.as_str()) {
                        spans.push(span);
                    }
                }
            }
        }
        for _ in open {
            unmade += 1;
            self.features
                .record(FeatureClass::Notation, "part group without a stop", place());
        }
        let mut held: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut groups = Vec::new();
        for (p, part) in parts.iter().enumerate() {
            if part.staves.len() >= 2 {
                let staves: Vec<(usize, usize)> = (0..part.staves.len()).map(|s| (p, s)).collect();
                held.extend(staves.iter().copied());
                groups.push(SourceGroup {
                    kind: GroupKind::Brace,
                    staves,
                });
            }
        }
        let staves_of = |parts_in: &[usize]| -> Vec<(usize, usize)> {
            parts_in
                .iter()
                .flat_map(|&p| (0..parts[p].staves.len()).map(move |s| (p, s)))
                .collect()
        };
        for wanted in ["brace", "bracket", "square"] {
            for (symbol, parts_in) in spans.iter().filter(|(symbol, _)| symbol == wanted) {
                let staves = staves_of(parts_in);
                let free: Vec<(usize, usize)> = staves
                    .iter()
                    .copied()
                    .filter(|s| !held.contains(s))
                    .collect();
                let (kind, holds) = match symbol.as_str() {
                    "brace" => (GroupKind::Brace, !free.is_empty()),
                    "bracket" => (GroupKind::Bracket, !free.is_empty()),
                    _ => (
                        GroupKind::SubBracket,
                        !free.is_empty() && free.len() == staves.len(),
                    ),
                };
                if holds {
                    held.extend(free.iter().copied());
                    groups.push(SourceGroup { kind, staves: free });
                } else {
                    unmade += 1;
                    self.features.record(
                        FeatureClass::Notation,
                        format!("part group ({symbol}) within another group"),
                        place(),
                    );
                }
            }
        }
        for (symbol, _) in spans
            .iter()
            .filter(|(symbol, _)| !matches!(symbol.as_str(), "brace" | "bracket" | "square"))
        {
            unmade += 1;
            self.features.record(
                FeatureClass::Notation,
                format!("part group ({symbol})"),
                place(),
            );
        }
        (groups, unmade)
    }

    fn read_part(
        &mut self,
        node: Node,
        declaration: Node,
        concert: bool,
    ) -> Result<PartRead, ReadError> {
        let id = node.attribute("id").unwrap_or("").to_owned();
        let part_name = child_text(declaration, "part-name")
            .filter(|s| !s.is_empty())
            .unwrap_or(&id)
            .to_owned();
        let abbreviation = child_text(declaration, "part-abbreviation")
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let mut members: Vec<SourceMember> = children(declaration, "score-instrument")
            .map(|inst| SourceMember {
                id: inst.attribute("id").unwrap_or("").to_owned(),
                name: child_text(inst, "instrument-name").unwrap_or("").to_owned(),
                step: 0,
            })
            .collect();
        for other in elements(declaration).filter(|n| {
            !matches!(
                name(*n),
                "part-name" | "part-abbreviation" | "score-instrument" | "midi-instrument"
            )
        }) {
            self.features.record(
                FeatureClass::Presentation,
                format!("score-part: {}", name(other)),
                Place {
                    part: part_name.clone(),
                    measure: String::new(),
                },
            );
        }
        if children(declaration, "midi-instrument").next().is_some() {
            self.features.record(
                FeatureClass::Presentation,
                "score-part: midi-instrument",
                Place {
                    part: part_name.clone(),
                    measure: String::new(),
                },
            );
        }

        let mut state = PartState {
            divisions: 1,
            file_transpose: None,
            key_shift: concert_key_shift(node, concert),
            open_slurs: BTreeMap::new(),
            open_beams: BTreeMap::new(),
            open_tuplets: BTreeMap::new(),
            last_event: None,
            written: Vec::new(),
            tied_over: BTreeMap::new(),
            grace_orders: BTreeMap::new(),
            open_lines: BTreeMap::new(),
        };
        let mut part = SourcePart {
            id,
            name: part_name.clone(),
            abbreviation,
            staves: vec![SourceStaff::default()],
            transposition: None,
            members: Vec::new(),
            events: Vec::new(),
            slurs: Vec::new(),
            beams: Vec::new(),
            unmade_beams: 0,
            tuplets: Vec::new(),
            unmade_tuplets: 0,
            dropped_notes: 0,
            dropped_quarter_tones: Vec::new(),
            dropped_tie_starts: 0,
            markers: Vec::new(),
            spanners: Vec::new(),
            tempos: Vec::new(),
        };
        let mut read = PartRead {
            part: part.clone(),
            lengths: Vec::new(),
            headers: Vec::new(),
            meters: Vec::new(),
            event_offsets: Vec::new(),
            clef_offsets: vec![Vec::new()],
            key_offsets: vec![Vec::new()],
            census: Census::default(),
        };
        let mut part_transpose: Option<TranspositionInterval> = None;
        let mut member_steps: BTreeMap<usize, i16> = BTreeMap::new();

        for (index, measure) in children(node, "measure").enumerate() {
            let number = measure.attribute("number").unwrap_or("").to_owned();
            let implicit = measure.attribute("implicit") == Some("yes");
            read.headers.push((number.clone(), implicit));
            let place = Place {
                part: part_name.clone(),
                measure: number.clone(),
            };
            let mut cursor: i64 = 0;
            let mut furthest: i64 = 0;
            let mut last_onset: i64 = 0;
            for item in elements(measure) {
                match name(item) {
                    "note" => {
                        self.read_note(
                            item,
                            index,
                            &place,
                            &mut state,
                            &mut part,
                            &mut read,
                            &mut members,
                            &mut member_steps,
                            &mut cursor,
                            &mut last_onset,
                        )?;
                    }
                    "backup" => {
                        let d = self.duration(item)?;
                        cursor -= d;
                        if cursor < 0 {
                            return Err(self.malformed(item, "backup before the measure's start"));
                        }
                    }
                    "forward" => {
                        cursor += self.duration(item)?;
                    }
                    "attributes" => {
                        self.read_attributes(
                            item,
                            index,
                            cursor,
                            &place,
                            &mut state,
                            &mut part,
                            &mut read,
                            &mut part_transpose,
                            concert,
                        )?;
                    }
                    "direction" => {
                        self.read_direction(item, &place, index, cursor, &mut state, &mut part)
                    }
                    "barline" => {
                        self.read_barline(item, &place, index, cursor, state.divisions, &mut part)
                    }
                    "print" => {
                        for attr in ["new-system", "new-page"] {
                            if item.attribute(attr) == Some("yes") {
                                self.features.record(
                                    FeatureClass::Notation,
                                    format!("print: {attr}"),
                                    place.clone(),
                                );
                            }
                        }
                        if elements(item).next().is_some() {
                            self.features.record(
                                FeatureClass::Presentation,
                                "print: layout",
                                place.clone(),
                            );
                        }
                    }
                    "sound" => match sound_tempo(item) {
                        Some(bpm) => part.tempos.push(SourceTempo {
                            at: SourcePoint::At {
                                staff: 0,
                                measure: index,
                                onset: RationalTime::new(cursor, 4 * state.divisions)
                                    .unwrap_or_else(zero),
                            },
                            bpm,
                            mark: None,
                        }),
                        None if item.attribute("tempo").is_some() => self.features.record(
                            FeatureClass::Content,
                            "sound: tempo not a positive number",
                            place.clone(),
                        ),
                        None => self.features.record(
                            FeatureClass::Presentation,
                            "sound: playback",
                            place.clone(),
                        ),
                    },
                    "harmony" => {
                        self.features
                            .record(FeatureClass::Content, "chord symbol", place.clone())
                    }
                    "figured-bass" => {
                        self.features
                            .record(FeatureClass::Content, "figured bass", place.clone())
                    }
                    other => self.features.record(
                        FeatureClass::Presentation,
                        format!("measure: {other}"),
                        place.clone(),
                    ),
                }
                furthest = furthest.max(cursor);
            }
            let length = RationalTime::new(furthest, 4 * state.divisions)
                .ok_or_else(|| self.malformed(measure, "a measure length out of range"))?;
            self.carry(&mut state, &mut part, index, &length)?;
            read.lengths.push(length);
        }

        for (slot, member) in members.iter_mut().enumerate() {
            if let Some(step) = member_steps.get(&slot) {
                member.step = *step;
            }
        }
        part.transposition = if concert {
            part_transpose.or(state.file_transpose)
        } else {
            state.file_transpose.or(part_transpose)
        };
        if part
            .events
            .iter()
            .any(|e| matches!(e.content, Content::Unpitched { .. }))
        {
            part.members = members;
        }
        for lane in state.open_beams.into_keys() {
            let grace = lane.ends_with("\u{1}grace");
            part.unmade_beams += usize::from(!grace);
            self.features.record(
                FeatureClass::Notation,
                if grace {
                    "grace beam without an end"
                } else {
                    "beam without an end"
                },
                Place {
                    part: part_name.clone(),
                    measure: String::new(),
                },
            );
        }
        for open in state.open_tuplets.into_values().flatten() {
            part.unmade_tuplets += 1;
            self.features.record(
                FeatureClass::Content,
                match open.ratio {
                    Some(_) => "tuplet without a stop",
                    None => "tuplet not made, never stopped",
                },
                Place {
                    part: part_name.clone(),
                    measure: String::new(),
                },
            );
        }
        for (kind, _) in std::mem::take(&mut state.open_lines).into_keys() {
            self.features.record(
                FeatureClass::Content,
                format!("{kind} without an end"),
                Place {
                    part: part_name.clone(),
                    measure: String::new(),
                },
            );
        }
        for (number, _) in state.open_slurs {
            self.features.record(
                FeatureClass::Content,
                format!("slur {number} without an end"),
                Place {
                    part: part_name.clone(),
                    measure: String::new(),
                },
            );
        }
        read.part = part;
        read.census = note_census(node);
        (read.census.expression, read.census.tempo_places) = expression_census(node);
        (read.census.keys, read.census.clefs) = attribute_census(node, concert);
        timed_census(node, &mut read.census);
        Ok(read)
    }

    /// Carries a quarter-tone accidental over a tie to the note continuing
    /// it, across a barline too, found as the emitter pairs a tie: in the
    /// tie's own voice first. It carries no further. MuseScore writes no
    /// `<alter>` on an arrowed note, and its quarter-tone accidental applies
    /// to its own note alone (roadmap D40), so a later note on the same staff,
    /// step and octave in the measure that writes neither an accidental nor an
    /// `<alter>` is unaltered, and the engraver shows the natural it needs.
    /// Notes are taken in the order of their onsets, not the file's, so a tie
    /// is found before the note it continues into.
    fn carry(
        &self,
        state: &mut PartState,
        part: &mut SourcePart,
        measure: usize,
        length: &Time,
    ) -> Result<(), ReadError> {
        let mut notes = std::mem::take(&mut state.written);
        notes.sort_by(|a, b| a.onset.cmp(&b.onset));
        let tied_over = std::mem::take(&mut state.tied_over);
        let mut tied: BTreeMap<(usize, u8, i8, Time), Tied> = BTreeMap::new();
        // The tied pitch a note continues: one in its own voice, else another.
        let continued = |starts: Option<&Tied>, voice: &str| {
            let starts = starts?;
            starts
                .iter()
                .find(|(v, _, _)| v == voice)
                .or(starts.first())
                .map(|(_, quarter_tones, named)| (*quarter_tones, *named))
        };
        let mut resolved = Vec::with_capacity(notes.len());
        for note in &notes {
            let key = (note.staff, note.nominal as u8, note.octave);
            // A note that states no alteration (an `<accidental>` always
            // does) takes a quarter-tone over a tie alone.
            let over_tie = if note.stated.is_some() || !note.tie_stop {
                None
            } else if note.onset == zero() {
                continued(tied_over.get(&key), &note.voice)
            } else {
                continued(
                    tied.get(&(key.0, key.1, key.2, note.onset.clone())),
                    &note.voice,
                )
            };
            let carried = over_tie.filter(|(quarter_tones, _)| quarter_tones % 2 != 0);
            let quarter_tones = note
                .stated
                .or(carried.map(|(quarter_tones, _)| quarter_tones))
                .unwrap_or(0);
            // The quarter-tone accidental the note is written with: its own,
            // else the one a tie carries to it.
            let named = match carried {
                Some((_, named)) => named,
                None => note.named,
            };
            let pitch = || {
                sounding(
                    quarter_tone_pitch(note.nominal, quarter_tones, note.octave),
                    note.transpose,
                )
                .map_err(|refusal| {
                    ReadError::Malformed(
                        self.doc.text_pos_at(note.offset).row,
                        format!("cannot transpose to sounding: {refusal:?}"),
                    )
                })
            };
            match (carried, note.at) {
                (Some(_), Some((event, index))) => {
                    let pitch = pitch()?;
                    if let Content::Pitched(pitches) = &mut part.events[event].content {
                        pitches[index].pitch = pitch;
                    }
                }
                (_, None) if quarter_tones % 2 != 0 => {
                    if let PitchSpacePosition::Cmn {
                        nominal,
                        alteration,
                        octave,
                    } = pitch()?.scale_position.position
                    {
                        part.dropped_quarter_tones.push(QuarterTone {
                            measure,
                            offset: note.onset.clone(),
                            staff: note.staff,
                            nominal: nominal as u8,
                            quarter_tones: i16::from(alteration),
                            octave,
                        });
                    }
                }
                _ => {}
            }
            // A quarter-tone is spelt with its accidental at its sounding
            // pitch, its own or carried over a tie.
            if let Some((event, index)) = note.at {
                if let Content::Pitched(pitches) = &mut part.events[event].content {
                    let sounding = &mut pitches[index];
                    sounding.spelling = quarter_tone_spelling(&sounding.pitch, named);
                }
            }
            if note.tie_start {
                tied.entry((key.0, key.1, key.2, note.end.clone()))
                    .or_default()
                    .push((note.voice.clone(), quarter_tones, named));
            }
            resolved.push((quarter_tones, named));
        }
        for (note, (quarter_tones, named)) in notes.iter().zip(resolved) {
            if note.tie_start && note.end >= *length {
                state
                    .tied_over
                    .entry((note.staff, note.nominal as u8, note.octave))
                    .or_default()
                    .push((note.voice.clone(), quarter_tones, named));
            }
        }
        Ok(())
    }

    fn duration(&self, node: Node) -> Result<i64, ReadError> {
        let Some(d) = child_text(node, "duration") else {
            return Err(self.malformed(node, format!("<{}> without a duration", name(node))));
        };
        d.parse::<i64>()
            .ok()
            .filter(|d| *d >= 0)
            .ok_or_else(|| self.malformed(node, format!("duration {d:?} is not a whole number")))
    }

    #[allow(clippy::too_many_arguments)]
    fn read_note(
        &mut self,
        note: Node,
        measure: usize,
        place: &Place,
        state: &mut PartState,
        part: &mut SourcePart,
        read: &mut PartRead,
        members: &mut Vec<SourceMember>,
        member_steps: &mut BTreeMap<usize, i16>,
        cursor: &mut i64,
        last_onset: &mut i64,
    ) -> Result<(), ReadError> {
        let is_chord = child(note, "chord").is_some();
        let grace_mark = child(note, "grace");
        if child(note, "cue").is_some() {
            self.features
                .record(FeatureClass::Content, "cue note", place.clone());
            // A cue note occupies time in its voice.
            if !is_chord && grace_mark.is_none() {
                *last_onset = *cursor;
                *cursor += self.duration(note)?;
            }
            return Ok(());
        }
        let has_pitch = child(note, "pitch");
        let has_unpitched = child(note, "unpitched");
        let has_rest = child(note, "rest");
        if !matches!(
            (
                has_pitch.is_some(),
                has_unpitched.is_some(),
                has_rest.is_some()
            ),
            (true, false, false) | (false, true, false) | (false, false, true)
        ) {
            return Err(
                self.malformed(note, "a note needs exactly one of pitch, unpitched or rest")
            );
        }
        // A grace note is an event of zero duration at the position of the
        // note it precedes, its notated value and its place among the graces
        // there its payload's (schema major 5).
        let grace = match grace_mark {
            None => None,
            Some(mark) => {
                let value = child_text(note, "type").and_then(note_value);
                match value {
                    Some(value) if has_rest.is_none() => Some(Grace {
                        kind: if mark.attribute("slash") == Some("yes") {
                            GraceKind::Acciaccatura
                        } else {
                            GraceKind::Appoggiatura
                        },
                        value,
                        dots: children(note, "dot").count().min(usize::from(u8::MAX)) as u8,
                        order: 0,
                    }),
                    _ => {
                        self.features.record(
                            FeatureClass::Content,
                            "grace note with no value the model holds",
                            place.clone(),
                        );
                        return Ok(());
                    }
                }
            }
        };

        let duration_div = if grace.is_some() {
            0
        } else {
            self.duration(note)?
        };
        let staff = match child_text(note, "staff") {
            None => 0,
            Some(s) => {
                s.parse::<usize>()
                    .ok()
                    .filter(|s| *s >= 1)
                    .ok_or_else(|| self.malformed(note, format!("staff {s:?}")))?
                    - 1
            }
        };
        if staff >= part.staves.len() {
            return Err(self.malformed(
                note,
                format!(
                    "staff {} beyond the part's {} staves",
                    staff + 1,
                    part.staves.len()
                ),
            ));
        }
        let voice = child_text(note, "voice").unwrap_or("1").to_owned();
        let tie_start = children(note, "tie").any(|t| t.attribute("type") == Some("start"));
        let tie_stop = children(note, "tie").any(|t| t.attribute("type") == Some("stop"));
        // A tie on a grace note would pair it with its own principal, at the
        // same position: not read.
        let (tie_start, tie_stop) = if grace.is_some() && (tie_start || tie_stop) {
            self.features
                .record(FeatureClass::Content, "tie on a grace note", place.clone());
            (false, false)
        } else {
            (tie_start, tie_stop)
        };

        let onset_div = if is_chord { *last_onset } else { *cursor };
        let visible = note.attribute("print-object") != Some("no");

        // The notes' own children, mapped or recorded.
        for item in elements(note) {
            match name(item) {
                "chord" | "pitch" | "unpitched" | "rest" | "duration" | "voice" | "staff"
                | "tie" | "type" | "dot" | "accidental" | "time-modification" | "instrument"
                | "beam" | "grace" => {}
                "notations" | "lyric" => {}
                "stem" => {
                    self.features
                        .record(FeatureClass::Notation, "stem direction", place.clone())
                }
                "notehead" => {
                    if text(item) != "normal" {
                        self.features.record(
                            FeatureClass::Notation,
                            format!("notehead: {}", text(item)),
                            place.clone(),
                        );
                    }
                }
                "play" | "listen" => self.features.record(
                    FeatureClass::Presentation,
                    format!("note: {}", name(item)),
                    place.clone(),
                ),
                other => self.features.record(
                    FeatureClass::Notation,
                    format!("note: {other}"),
                    place.clone(),
                ),
            }
        }
        if !visible && has_rest.is_none() {
            self.features
                .record(FeatureClass::Notation, "invisible note", place.clone());
        }
        // MuseScore marks each accidental its user sets `cautionary`, and
        // every quarter-tone accidental is one. The note is spelt with a
        // quarter-tone accidental, so on one the mark alone leaves nothing
        // unimported; parentheses or an editorial mark still do.
        let explicit_accidental = child(note, "accidental").is_some_and(|a| {
            (a.attribute("cautionary") == Some("yes") && quarter_tone_name(text(a)).is_none())
                || a.attribute("editorial") == Some("yes")
                || a.attribute("parentheses") == Some("yes")
        });
        if explicit_accidental {
            self.features.record(
                FeatureClass::Notation,
                "cautionary accidental",
                place.clone(),
            );
        }
        let mut slur_marks: Vec<(String, String)> = Vec::new();
        let mut marks = NoteMarks::default();
        for notations in children(note, "notations") {
            for item in elements(notations) {
                match name(item) {
                    "tied" | "tuplet" => {}
                    "slur" => slur_marks.push((
                        item.attribute("type").unwrap_or("").to_owned(),
                        item.attribute("number").unwrap_or("1").to_owned(),
                    )),
                    "articulations" | "technical" | "ornaments" | "fermata" | "arpeggiate"
                    | "dynamics" | "slide" | "glissando" => {
                        if let Err(kind) = marks.read(item) {
                            for kind in kind {
                                self.features
                                    .record(FeatureClass::Content, kind, place.clone());
                            }
                        }
                    }
                    other => {
                        self.features
                            .record(FeatureClass::Content, other.to_owned(), place.clone())
                    }
                }
            }
        }
        let lyrics: Vec<Result<SourceLyric, String>> = children(note, "lyric")
            .filter_map(|lyric| read_lyric(lyric).transpose())
            .collect();

        let mut written = None;
        let content = if let Some(pitch) = has_pitch {
            let step = child_text(pitch, "step").unwrap_or("");
            let nominal =
                nominal_of(step).ok_or_else(|| self.malformed(pitch, format!("step {step:?}")))?;
            let octave: i8 = child_text(pitch, "octave")
                .and_then(|o| o.parse().ok())
                .ok_or_else(|| self.malformed(pitch, "a pitch without an octave"))?;
            let accidental = child(note, "accidental").map(text);
            let stated = match child_text(pitch, "alter") {
                // `<alter>` counts semitones; a quarter-tone is half of one,
                // and the model holds it in `cmn-24`. Anything finer is
                // refused, never rounded.
                Some(alter_text) => {
                    let alter: f64 = alter_text
                        .parse()
                        .ok()
                        .filter(|a: &f64| a.abs() <= 12.0)
                        .ok_or_else(|| self.malformed(pitch, format!("alter {alter_text:?}")))?;
                    let doubled = alter * 2.0;
                    if doubled.fract() != 0.0 {
                        return Err(self.unsupported(
                            pitch,
                            format!("alter {alter_text}, not a whole number of quarter-tones"),
                        ));
                    }
                    Some(doubled as i8)
                }
                // MuseScore writes a quarter-tone accidental with no
                // `<alter>`, so its name gives the alteration (roadmap D20).
                None => match accidental {
                    None => None,
                    Some(name) => match quarter_tones_named(name) {
                        Some(quarter_tones) => Some(quarter_tones),
                        None if semitone_accidental(name) => Some(0),
                        None => {
                            return Err(self
                                .unsupported(note, format!("accidental {name:?} with no alter")))
                        }
                    },
                },
            };
            let sounding = sounding(
                quarter_tone_pitch(nominal, stated.unwrap_or(0), octave),
                state.file_transpose,
            )
            .map_err(|refusal| {
                self.malformed(pitch, format!("cannot transpose to sounding: {refusal:?}"))
            })?;
            let onset = RationalTime::new(onset_div, 4 * state.divisions)
                .ok_or_else(|| self.malformed(note, "an onset out of range"))?;
            let end = RationalTime::new(onset_div + duration_div, 4 * state.divisions)
                .ok_or_else(|| self.malformed(note, "an end out of range"))?;
            written = Some(WrittenPitch {
                at: None,
                staff,
                voice: voice.clone(),
                onset,
                end,
                nominal,
                octave,
                stated,
                named: accidental.and_then(quarter_tone_name),
                tie_start,
                tie_stop,
                transpose: state.file_transpose,
                offset: note.range().start,
            });
            Some(SourcePitch {
                pitch: sounding,
                tie_start,
                tie_stop,
                spelling: None,
            })
        } else {
            None
        };
        let mut kept_at = None;
        let mut joined = !is_chord;

        let event_index = if is_chord {
            let Some(last) = state.last_event else {
                return Err(self.malformed(note, "a chord note with no note before it"));
            };
            let event = &mut part.events[last];
            match (&mut event.content, content) {
                (Content::Pitched(pitches), Some(pitch)) if event.staff == staff => {
                    pitches.push(pitch);
                    kept_at = Some((last, pitches.len() - 1));
                    joined = true;
                    if RationalTime::new(duration_div, 4 * state.divisions).as_ref()
                        != Some(&event.duration)
                    {
                        self.features.record(
                            FeatureClass::Content,
                            "chord note of a different duration",
                            place.clone(),
                        );
                    }
                }
                (Content::Pitched(_), Some(_)) => {
                    part.dropped_notes += 1;
                    part.dropped_tie_starts += usize::from(tie_start && has_rest.is_none());
                    self.features.record(
                        FeatureClass::Content,
                        "cross-staff chord note",
                        place.clone(),
                    );
                }
                (Content::Unpitched { .. }, _) if has_unpitched.is_some() => {
                    part.dropped_notes += 1;
                    part.dropped_tie_starts += usize::from(tie_start && has_rest.is_none());
                    self.features.record(
                        FeatureClass::Content,
                        "unpitched chord note",
                        place.clone(),
                    );
                }
                _ => {
                    part.dropped_notes += 1;
                    part.dropped_tie_starts += usize::from(tie_start && has_rest.is_none());
                    self.features.record(
                        FeatureClass::Content,
                        "chord note joining a rest",
                        place.clone(),
                    );
                }
            }
            last
        } else {
            let content = if let Some(pitch) = content {
                kept_at = Some((part.events.len(), 0));
                Content::Pitched(vec![pitch])
            } else if let Some(unpitched) = has_unpitched {
                let step = child_text(unpitched, "display-step").and_then(nominal_of);
                let octave = child_text(unpitched, "display-octave").and_then(|o| o.parse().ok());
                let step = match (step, octave) {
                    (Some(s), Some(o)) => treble_step(s, o),
                    _ => 0,
                };
                let member = child(note, "instrument")
                    .and_then(|i| i.attribute("id"))
                    .and_then(|id| members.iter().position(|m| m.id == id));
                let member = match member {
                    Some(m) => m,
                    None if !members.is_empty() => 0,
                    None => {
                        members.push(SourceMember {
                            id: String::new(),
                            name: part.name.clone(),
                            step,
                        });
                        0
                    }
                };
                member_steps.entry(member).or_insert(step);
                Content::Unpitched {
                    step,
                    member,
                    tie_start,
                    tie_stop,
                }
            } else {
                Content::Rest { visible }
            };
            let duration = RationalTime::new(duration_div, 4 * state.divisions)
                .ok_or_else(|| self.malformed(note, "a duration out of range"))?;
            let grace = grace.map(|grace| {
                let next = state
                    .grace_orders
                    .entry((measure, staff, voice.clone(), *cursor))
                    .or_insert(0);
                let order = *next;
                *next = next.saturating_add(1);
                Grace { order, ..grace }
            });
            part.events.push(SourceEvent {
                measure,
                staff,
                voice,
                onset: zero(),
                duration,
                content,
                offset: note.range().start,
                marks: Vec::new(),
                ornaments: Vec::new(),
                grace,
                lyrics: Vec::new(),
            });
            read.event_offsets.push(
                RationalTime::new(onset_div, 4 * state.divisions)
                    .ok_or_else(|| self.malformed(note, "an onset out of range"))?,
            );
            *last_onset = *cursor;
            *cursor += duration_div;
            let index = part.events.len() - 1;
            state.last_event = Some(index);
            index
        };
        if let Some(mut written) = written {
            written.at = kept_at;
            state.written.push(written);
        }

        // The primary beam joins the notes of one voice, a chord by its first
        // note; the shorter values' further beams follow from the notes.
        let beam = children(note, "beam")
            .find(|b| b.attribute("number").unwrap_or("1") == "1")
            .map(text);
        if let (false, Some(beam)) = (is_chord, beam) {
            // Graces beam among themselves, apart from their voice's notes.
            let voice = match grace_mark {
                Some(_) => format!("{}\u{1}grace", child_text(note, "voice").unwrap_or("1")),
                None => child_text(note, "voice").unwrap_or("1").to_owned(),
            };
            match beam {
                "begin"
                    if state
                        .open_beams
                        .insert(voice.clone(), vec![event_index])
                        .is_some() =>
                {
                    let grace = grace_mark.is_some();
                    part.unmade_beams += usize::from(!grace);
                    self.features.record(
                        FeatureClass::Notation,
                        if grace {
                            "grace beam begun again before its end"
                        } else {
                            "beam begun again before its end"
                        },
                        place.clone(),
                    );
                }
                "continue" | "end" => match state.open_beams.get_mut(&voice) {
                    Some(events) => {
                        events.push(event_index);
                        if beam == "end" {
                            let events = state.open_beams.remove(&voice).unwrap_or_default();
                            part.beams.push(SourceBeam { events });
                        }
                    }
                    None => self.features.record(
                        FeatureClass::Notation,
                        format!("beam {beam} without a begin"),
                        place.clone(),
                    ),
                },
                _ => {}
            }
        }

        // A tuplet joins the notes and rests of one voice from its start to
        // the stop of its number, a chord by its first note. One begun inside
        // another is recorded and not made: nesting is not yet read.
        if !is_chord {
            let voice = child_text(note, "voice").unwrap_or("1").to_owned();
            // Each mark with the `<notations>` that holds it.
            let held: Vec<(Node, Node)> = children(note, "notations")
                .flat_map(|n| children(n, "tuplet").map(move |t| (n, t)))
                .collect();
            let marks: Vec<Node> = held.iter().map(|(_, mark)| *mark).collect();
            let ratio = child(note, "time-modification").and_then(|m| {
                let actual = child_text(m, "actual-notes")?.trim().parse::<u32>().ok()?;
                let normal = child_text(m, "normal-notes")?.trim().parse::<u32>().ok()?;
                (actual != 0 && normal != 0 && actual != normal).then_some((actual, normal))
            });
            let stack = state.open_tuplets.entry(voice).or_default();
            for (notations, mark) in held
                .iter()
                .filter(|(_, t)| t.attribute("type") == Some("start"))
            {
                let made = match ratio {
                    Some(r) if stack.is_empty() => Some(r),
                    Some((actual, normal)) => {
                        self.features.record(
                            FeatureClass::Content,
                            format!("tuplet {actual}:{normal} inside another"),
                            place.clone(),
                        );
                        None
                    }
                    None => {
                        self.features.record(
                            FeatureClass::Content,
                            "tuplet with no ratio",
                            place.clone(),
                        );
                        None
                    }
                };
                stack.push(OpenTuplet {
                    number: mark.attribute("number").unwrap_or("1").to_owned(),
                    ratio: made,
                    events: Vec::new(),
                    display: tuplet_display(*notations, *mark),
                });
            }
            // A grace note occupies no time, so it is no tuplet's member.
            if grace_mark.is_none() {
                for open in stack.iter_mut().filter(|open| open.ratio.is_some()) {
                    open.events.push(event_index);
                }
            }
            for mark in marks.iter().filter(|t| t.attribute("type") == Some("stop")) {
                let number = mark.attribute("number").unwrap_or("1");
                match stack.iter().rposition(|open| open.number == number) {
                    Some(at) => {
                        let open = stack.remove(at);
                        match open.ratio {
                            Some((actual, normal)) => part.tuplets.push(SourceTuplet {
                                actual,
                                normal,
                                events: open.events,
                                display: open.display,
                            }),
                            None => part.unmade_tuplets += 1,
                        }
                    }
                    None => self.features.record(
                        FeatureClass::Content,
                        "tuplet stop without a start",
                        place.clone(),
                    ),
                }
            }
        }

        for (kind, number) in slur_marks {
            match kind.as_str() {
                "start"
                    if state
                        .open_slurs
                        .insert(number.clone(), event_index)
                        .is_some() =>
                {
                    self.features.record(
                        FeatureClass::Content,
                        format!("slur {number} restarted before its end"),
                        place.clone(),
                    );
                }
                "stop" => match state.open_slurs.remove(&number) {
                    Some(start) => part.slurs.push(SourceSlur {
                        start,
                        end: event_index,
                    }),
                    None => self.features.record(
                        FeatureClass::Content,
                        format!("slur {number} ending without a start"),
                        place.clone(),
                    ),
                },
                _ => {}
            }
        }

        // The marks, lyrics and lines the note carries, on its event; a note
        // the reader drops carries them nowhere, and each is recorded.
        if !joined {
            for kind in marks
                .kinds()
                .chain(lyrics.iter().map(|_| String::from("lyric")))
            {
                self.features.record(
                    FeatureClass::Content,
                    format!("{kind} on a dropped chord note"),
                    place.clone(),
                );
            }
            return Ok(());
        }
        let event = &mut part.events[event_index];
        let rest = matches!(event.content, Content::Rest { .. });
        if rest && !marks.event.is_empty() {
            for mark in &marks.event {
                self.features.record(
                    FeatureClass::Content,
                    format!("{} on a rest", mark_name(mark)),
                    place.clone(),
                );
            }
        } else {
            event.marks = epiphany_core::canonical_marks(event.marks.drain(..).chain(marks.event));
        }
        if !marks.ornaments.is_empty() {
            if matches!(event.content, Content::Pitched(_)) {
                event.ornaments = epiphany_core::canonical_ornaments(
                    event.ornaments.drain(..).chain(marks.ornaments),
                );
            } else {
                for _ in &marks.ornaments {
                    self.features.record(
                        FeatureClass::Content,
                        "ornament on an unpitched note or a rest",
                        place.clone(),
                    );
                }
            }
        }
        for lyric in lyrics {
            match lyric {
                Ok(lyric) if event.lyrics.iter().all(|l| l.verse != lyric.verse) => {
                    event.lyrics.push(lyric)
                }
                Ok(_) => self.features.record(
                    FeatureClass::Content,
                    "second lyric syllable in one verse",
                    place.clone(),
                ),
                Err(kind) => self
                    .features
                    .record(FeatureClass::Content, kind, place.clone()),
            }
        }
        for kind in marks.points {
            part.markers.push(SourceMarker {
                at: SourcePoint::Event(event_index),
                kind,
            });
        }
        for (begins, key, kind, line) in marks.lines {
            let point = SourcePoint::Event(event_index);
            if begins {
                let begun_again = state
                    .open_lines
                    .insert(
                        key.clone(),
                        OpenLine {
                            start: point,
                            staff,
                            kind,
                            line,
                        },
                    )
                    .is_some();
                if begun_again {
                    self.features.record(
                        FeatureClass::Content,
                        format!("{} begun again before its end", key.0),
                        place.clone(),
                    );
                }
            } else {
                self.end_line(state, part, key, point, place);
            }
        }
        Ok(())
    }

    /// Ends the line open under `key` at `end`, or records a stop with no
    /// start.
    fn end_line(
        &mut self,
        state: &mut PartState,
        part: &mut SourcePart,
        key: (&'static str, String),
        end: SourcePoint,
        place: &Place,
    ) {
        match state.open_lines.remove(&key) {
            Some(open) => part.spanners.push(SourceSpanner {
                kind: open.kind,
                line: open.line,
                staff: open.staff,
                start: open.start,
                end,
            }),
            None => self.features.record(
                FeatureClass::Content,
                format!("{} stop without a start", key.0),
                place.clone(),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn read_attributes(
        &mut self,
        attributes: Node,
        measure: usize,
        cursor: i64,
        place: &Place,
        state: &mut PartState,
        part: &mut SourcePart,
        read: &mut PartRead,
        part_transpose: &mut Option<TranspositionInterval>,
        concert: bool,
    ) -> Result<(), ReadError> {
        let offset = |divisions: i64| {
            RationalTime::new(cursor, 4 * divisions).expect("divisions are positive")
        };
        // The schema puts `<staves>` after `<key>`, but an unnumbered key
        // applies to every staff and a numbered one names a staff, so the
        // part's staves are known before any key or clef is placed.
        if let Some(item) = child(attributes, "staves") {
            let count = text(item)
                .parse::<usize>()
                .ok()
                .filter(|c| (1..=16).contains(c))
                .ok_or_else(|| self.malformed(item, format!("staves {:?}", text(item))))?;
            while part.staves.len() < count {
                part.staves.push(SourceStaff::default());
                read.clef_offsets.push(Vec::new());
                read.key_offsets.push(Vec::new());
            }
        }
        for item in elements(attributes) {
            match name(item) {
                "divisions" => {
                    state.divisions = text(item)
                        .parse::<i64>()
                        .ok()
                        .filter(|d| *d > 0)
                        .ok_or_else(|| {
                            self.malformed(item, format!("divisions {:?}", text(item)))
                        })?;
                }
                "staves" => {}
                "key" => {
                    let Some(fifths) =
                        child_text(item, "fifths").and_then(|f| f.parse::<i8>().ok())
                    else {
                        self.features.record(
                            FeatureClass::Content,
                            "non-traditional key signature",
                            place.clone(),
                        );
                        continue;
                    };
                    if !(-7..=7).contains(&fifths) {
                        self.features.record(
                            FeatureClass::Content,
                            "key signature beyond seven accidentals",
                            place.clone(),
                        );
                        continue;
                    }
                    // An open key (`<mode>none</mode>`) is no key signature,
                    // which no transposition moves; the model holds it as
                    // the absence of one where none is in force, and as no
                    // accidentals after one.
                    let open = child_text(item, "mode") == Some("none");
                    let fifths = if open {
                        0
                    } else {
                        // At concert pitch, as the model holds every pitch.
                        match i8::try_from(i32::from(fifths) + state.key_shift)
                            .ok()
                            .filter(|f| (-7..=7).contains(f))
                        {
                            Some(concert) => concert,
                            None => {
                                self.features.record(
                                    FeatureClass::Content,
                                    "key signature beyond seven accidentals at concert pitch",
                                    place.clone(),
                                );
                                continue;
                            }
                        }
                    };
                    let staves: Vec<usize> = match item.attribute("number") {
                        Some(n) => match n.parse::<usize>() {
                            Ok(n) if n >= 1 && n <= part.staves.len() => vec![n - 1],
                            _ => {
                                self.features.record(
                                    FeatureClass::Content,
                                    "key for a staff the part lacks",
                                    place.clone(),
                                );
                                continue;
                            }
                        },
                        None => (0..part.staves.len()).collect(),
                    };
                    for s in staves {
                        if open {
                            if part.staves[s].keys.is_empty() {
                                continue;
                            }
                            self.features.record(
                                FeatureClass::Content,
                                "an open key after a key signature",
                                place.clone(),
                            );
                        }
                        part.staves[s].keys.push(KeyChange {
                            onset: zero(),
                            measure,
                            fifths,
                        });
                        read.key_offsets[s].push(offset(state.divisions));
                    }
                }
                "time" => {
                    if child(item, "senza-misura").is_some() {
                        self.features
                            .record(FeatureClass::Content, "senza misura", place.clone());
                        continue;
                    }
                    let beats: Vec<&str> = children(item, "beats").map(text).collect();
                    let types: Vec<&str> = children(item, "beat-type").map(text).collect();
                    if beats.len() != 1 || types.len() != 1 {
                        self.features.record(
                            FeatureClass::Content,
                            "composite meter",
                            place.clone(),
                        );
                        continue;
                    }
                    let numerators: Option<Vec<u16>> =
                        beats[0].split('+').map(|b| b.trim().parse().ok()).collect();
                    let denominator: Option<u16> = types[0].parse().ok();
                    match (numerators, denominator) {
                        (Some(numerators), Some(denominator))
                            if denominator > 0 && numerators.iter().all(|n| *n > 0) =>
                        {
                            if cursor != 0 {
                                self.features.record(
                                    FeatureClass::Content,
                                    "meter change inside a measure",
                                    place.clone(),
                                );
                            }
                            if item.attribute("symbol").is_some_and(|s| s != "normal") {
                                self.features.record(
                                    FeatureClass::Notation,
                                    format!(
                                        "time symbol {}",
                                        item.attribute("symbol").unwrap_or("")
                                    ),
                                    place.clone(),
                                );
                            }
                            read.meters.push((
                                measure,
                                offset(state.divisions),
                                Meter {
                                    numerators,
                                    denominator,
                                },
                            ));
                        }
                        _ => self.features.record(
                            FeatureClass::Content,
                            "unreadable meter",
                            place.clone(),
                        ),
                    }
                }
                "clef" => {
                    let s = match item.attribute("number") {
                        Some(n) => match n.parse::<usize>() {
                            Ok(n) if n >= 1 && n <= part.staves.len() => n - 1,
                            _ => return Err(self.malformed(item, format!("clef number {n:?}"))),
                        },
                        None => 0,
                    };
                    let sign = child_text(item, "sign").unwrap_or("");
                    let line: Option<i8> = child_text(item, "line").and_then(|l| l.parse().ok());
                    let octave_shift: i8 = child_text(item, "clef-octave-change")
                        .and_then(|o| o.parse().ok())
                        .unwrap_or(0);
                    let clef = match sign {
                        "G" => Some(Clef {
                            shape: ClefShape::G,
                            line: line.unwrap_or(2),
                            octave_shift,
                        }),
                        "F" => Some(Clef {
                            shape: ClefShape::F,
                            line: line.unwrap_or(4),
                            octave_shift,
                        }),
                        "C" => Some(Clef {
                            shape: ClefShape::C,
                            line: line.unwrap_or(3),
                            octave_shift,
                        }),
                        "percussion" => Some(Clef {
                            shape: ClefShape::Percussion,
                            line: 3,
                            octave_shift: 0,
                        }),
                        _ => None,
                    };
                    match clef {
                        Some(clef) => {
                            part.staves[s].clefs.push(ClefChange {
                                onset: zero(),
                                measure,
                                clef,
                            });
                            read.clef_offsets[s].push(offset(state.divisions));
                        }
                        None => self.features.record(
                            FeatureClass::Content,
                            format!("clef {sign:?}"),
                            place.clone(),
                        ),
                    }
                }
                "staff-details" => {
                    let s = item
                        .attribute("number")
                        .and_then(|n| n.parse::<usize>().ok())
                        .filter(|n| *n >= 1 && *n <= part.staves.len())
                        .map_or(0, |n| n - 1);
                    if let Some(lines) =
                        child_text(item, "staff-lines").and_then(|l| l.parse().ok())
                    {
                        part.staves[s].lines = Some(lines);
                    }
                    if elements(item).any(|c| name(c) != "staff-lines") {
                        self.features.record(
                            FeatureClass::Presentation,
                            "staff-details",
                            place.clone(),
                        );
                    }
                }
                "transpose" => {
                    let interval = read_interval(item);
                    if child(item, "double").is_some() {
                        self.features.record(
                            FeatureClass::Content,
                            "transposition doubled at the octave",
                            place.clone(),
                        );
                    }
                    if measure > 0 || cursor > 0 {
                        self.features.record(
                            FeatureClass::Content,
                            "transposition change",
                            place.clone(),
                        );
                    }
                    if concert
                        && (interval.diatonic_steps % 7 != 0 || interval.chromatic_steps % 12 != 0)
                    {
                        self.features.record(
                            FeatureClass::Content,
                            "non-octave transposition in a concert score",
                            place.clone(),
                        );
                    }
                    state.file_transpose = (interval
                        != TranspositionInterval {
                            diatonic_steps: 0,
                            chromatic_steps: 0,
                        })
                    .then_some(interval);
                }
                "for-part" => {
                    if let Some(pt) = child(item, "part-transpose") {
                        let interval = read_interval(pt);
                        *part_transpose = (interval
                            != TranspositionInterval {
                                diatonic_steps: 0,
                                chromatic_steps: 0,
                            })
                        .then_some(interval);
                    }
                }
                "measure-style" => {
                    for style in elements(item) {
                        let class = if name(style) == "multiple-rest" {
                            FeatureClass::Presentation
                        } else {
                            FeatureClass::Content
                        };
                        self.features.record(
                            class,
                            format!("measure-style: {}", name(style)),
                            place.clone(),
                        );
                    }
                }
                "part-symbol" | "instruments" => self.features.record(
                    FeatureClass::Presentation,
                    format!("attributes: {}", name(item)),
                    place.clone(),
                ),
                "directive" => {
                    self.features
                        .record(FeatureClass::Content, "directive", place.clone())
                }
                other => self.features.record(
                    FeatureClass::Content,
                    format!("attributes: {other}"),
                    place.clone(),
                ),
            }
        }
        Ok(())
    }

    /// A direction's marks, at its staff and its position in the measure
    /// (schema major 5): dynamics, staff text, tempo and metronome marks,
    /// rehearsal marks, segno and coda as point marks; hairpins, pedal lines,
    /// ottavas, text lines and brackets as lines from their start to their
    /// stop; a tempo it sets, with the mark it shows.
    #[allow(clippy::too_many_arguments)]
    fn read_direction(
        &mut self,
        direction: Node,
        place: &Place,
        measure: usize,
        cursor: i64,
        state: &mut PartState,
        part: &mut SourcePart,
    ) {
        let staff = child_text(direction, "staff")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|s| (1..=part.staves.len()).contains(s))
            .map_or(0, |s| s - 1);
        let offset = child_text(direction, "offset")
            .and_then(|o| o.trim().parse::<i64>().ok())
            .unwrap_or(0);
        let at = SourcePoint::At {
            staff,
            measure,
            onset: RationalTime::new((cursor + offset).max(0), 4 * state.divisions)
                .unwrap_or_else(zero),
        };
        let kinds: Vec<Node> = children(direction, "direction-type")
            .flat_map(elements)
            .collect();
        // A run of `<words>` is one text, as MuseScore splits a text by font.
        let words: String = kinds
            .iter()
            .filter(|k| name(**k) == "words")
            .map(|k| text(*k))
            .collect();
        let words = (!words.trim().is_empty()).then(|| Text::new(words.trim()));
        let metronome = match kinds.iter().find(|k| name(**k) == "metronome") {
            Some(node) => match metronome(*node) {
                Some(m) => Some(m),
                None => {
                    self.features.record(
                        FeatureClass::Content,
                        "metronome the model cannot hold",
                        place.clone(),
                    );
                    None
                }
            },
            None => None,
        };
        let tempo = child(direction, "sound").and_then(sound_tempo);
        let text_line = kinds
            .iter()
            .any(|k| name(*k) == "dashes" && k.attribute("type") == Some("start"));
        // The words a tempo or a text line takes are its own, not staff text.
        let mut words_taken = false;
        let mark = (tempo.is_some() || metronome.is_some())
            .then(|| {
                words_taken = !text_line;
                TempoMark {
                    text: if text_line { None } else { words.clone() },
                    metronome,
                }
            })
            .filter(|m| m.text.is_some() || m.metronome.is_some());
        match (tempo, mark) {
            (Some(bpm), mark) => part.tempos.push(SourceTempo {
                at: at.clone(),
                bpm,
                mark,
            }),
            (None, Some(mark)) => part.markers.push(SourceMarker {
                at: at.clone(),
                kind: MarkerKind::Tempo(mark),
            }),
            (None, None) => {}
        }
        for kind in &kinds {
            let number = kind.attribute("number").unwrap_or("1").to_owned();
            match name(*kind) {
                "words" | "metronome" => {}
                "dynamics" => {
                    for mark in elements(*kind) {
                        part.markers.push(SourceMarker {
                            at: at.clone(),
                            kind: MarkerKind::Dynamic(dynamic(mark)),
                        });
                    }
                }
                "rehearsal" => part.markers.push(SourceMarker {
                    at: at.clone(),
                    kind: MarkerKind::Rehearsal(Text::new(text(*kind).trim())),
                }),
                "segno" => part.markers.push(SourceMarker {
                    at: at.clone(),
                    kind: MarkerKind::Segno,
                }),
                "coda" => part.markers.push(SourceMarker {
                    at: at.clone(),
                    kind: MarkerKind::Coda,
                }),
                "wedge" | "pedal" | "octave-shift" | "dashes" | "bracket" => {
                    let begins = match (name(*kind), kind.attribute("type")) {
                        ("wedge", Some("crescendo")) => {
                            Some(SpannerKind::Hairpin(HairpinDirection::Crescendo))
                        }
                        ("wedge", Some("diminuendo")) => {
                            Some(SpannerKind::Hairpin(HairpinDirection::Diminuendo))
                        }
                        ("pedal", Some("start" | "change")) => {
                            if kind.attribute("line") == Some("no") {
                                self.features.record(
                                    FeatureClass::Content,
                                    "pedal sign with no line",
                                    place.clone(),
                                );
                                continue;
                            }
                            Some(if kind.attribute("sign") == Some("no") {
                                SpannerKind::PedalBracket(PedalKind::Sustain)
                            } else {
                                SpannerKind::PedalLine(PedalKind::Sustain)
                            })
                        }
                        // An ottava is written an octave or two from where it
                        // sounds: shifted down, it sounds above (8va).
                        ("octave-shift", Some(shift @ ("up" | "down"))) => {
                            let octaves = match kind.attribute("size").unwrap_or("8") {
                                "8" => 1,
                                "15" => 2,
                                "22" => 3,
                                _ => {
                                    self.features.record(
                                        FeatureClass::Content,
                                        "octave shift of another size",
                                        place.clone(),
                                    );
                                    continue;
                                }
                            };
                            Some(SpannerKind::OctaveLine(OctaveOffset(if shift == "down" {
                                octaves
                            } else {
                                -octaves
                            })))
                        }
                        ("dashes", Some("start")) => {
                            words_taken = true;
                            Some(SpannerKind::TextLine(TextLineDefinition {
                                text: words.clone().unwrap_or_default(),
                            }))
                        }
                        ("bracket", Some("start")) => {
                            Some(SpannerKind::Bracket(BracketKind::Square))
                        }
                        _ => None,
                    };
                    let key = (line_kind(name(*kind)), number);
                    let ends = matches!(kind.attribute("type"), Some("stop" | "change"));
                    if ends {
                        self.end_line(state, part, key.clone(), at.clone(), place);
                    }
                    if let Some(spanner) = begins {
                        let line = match name(*kind) {
                            "dashes" => LineStyle::Dashed,
                            "bracket" => match kind.attribute("line-type") {
                                Some("dashed") => LineStyle::Dashed,
                                Some("dotted") => LineStyle::Dotted,
                                Some("wavy") => LineStyle::Wavy,
                                _ => LineStyle::Solid,
                            },
                            _ => LineStyle::Solid,
                        };
                        let begun_again = state
                            .open_lines
                            .insert(
                                key.clone(),
                                OpenLine {
                                    start: at.clone(),
                                    staff,
                                    kind: spanner,
                                    line,
                                },
                            )
                            .is_some();
                        if begun_again {
                            self.features.record(
                                FeatureClass::Content,
                                format!("{} begun again before its end", key.0),
                                place.clone(),
                            );
                        }
                    }
                }
                other => self.features.record(
                    FeatureClass::Content,
                    format!("direction: {other}"),
                    place.clone(),
                ),
            }
        }
        if let (Some(words), false) = (words, words_taken) {
            part.markers.push(SourceMarker {
                at,
                kind: MarkerKind::Text(words),
            });
        }
        for item in elements(direction) {
            match name(item) {
                "direction-type" | "offset" | "staff" | "voice" | "listening" => {}
                "sound" => {
                    if sound_tempo(item).is_none() {
                        let (class, kind) = if item.attribute("tempo").is_some() {
                            (FeatureClass::Content, "sound: tempo not a positive number")
                        } else {
                            (FeatureClass::Presentation, "sound: playback")
                        };
                        self.features.record(class, kind, place.clone());
                    }
                }
                other => self.features.record(
                    FeatureClass::Presentation,
                    format!("direction: {other}"),
                    place.clone(),
                ),
            }
        }
    }

    /// A barline's marks: a fermata, at the barline on the part's first staff
    /// (schema major 5); its style, repeat or ending, recorded.
    fn read_barline(
        &mut self,
        barline: Node,
        place: &Place,
        measure: usize,
        cursor: i64,
        divisions: i64,
        part: &mut SourcePart,
    ) {
        for item in elements(barline) {
            let (class, kind) = match name(item) {
                "bar-style" => (FeatureClass::Notation, format!("barline {}", text(item))),
                "repeat" => (FeatureClass::Content, String::from("repeat")),
                "ending" => {
                    if item.attribute("type") != Some("start") {
                        continue;
                    }
                    (FeatureClass::Content, String::from("ending (volta)"))
                }
                "fermata" => match fermata_shape(text(item)) {
                    Some(shape) => {
                        let at_start = barline.attribute("location") == Some("left");
                        part.markers.push(SourceMarker {
                            at: SourcePoint::Barline {
                                measure,
                                onset: if at_start {
                                    zero()
                                } else {
                                    RationalTime::new(cursor, 4 * divisions).unwrap_or_else(zero)
                                },
                            },
                            kind: MarkerKind::Fermata(Fermata {
                                shape,
                                inverted: item.attribute("type") == Some("inverted"),
                            }),
                        });
                        continue;
                    }
                    None => (FeatureClass::Content, format!("fermata {:?}", text(item))),
                },
                other => (FeatureClass::Content, format!("barline: {other}")),
            };
            self.features.record(class, kind, place.clone());
        }
    }
}

/// A line's kind, keyed with its number while it is open.
fn line_kind(element: &str) -> &'static str {
    match element {
        "wedge" => "wedge",
        "pedal" => "pedal",
        "octave-shift" => "octave shift",
        "dashes" => "text line",
        _ => "bracket",
    }
}

/// A `<sound tempo>`, where it is a finite positive number of quarter notes
/// per minute.
fn sound_tempo(sound: Node) -> Option<f64> {
    sound
        .attribute("tempo")?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|t| t.is_finite() && *t > 0.0)
}

/// A `<metronome>` the model holds: one beat unit, dotted or not, at a
/// number per minute kept as its text.
fn metronome(node: Node) -> Option<Metronome> {
    let units: Vec<Node> = children(node, "beat-unit").collect();
    let [unit] = units.as_slice() else {
        return None;
    };
    let per_minute = child_text(node, "per-minute")?.trim();
    if per_minute.is_empty() || child(node, "beat-unit-tied").is_some() {
        return None;
    }
    Some(Metronome {
        beat: note_value(text(*unit))?,
        dots: children(node, "beat-unit-dot")
            .count()
            .min(usize::from(u8::MAX)) as u8,
        per_minute: Text::new(per_minute),
    })
}

/// What a note's `<notations>` carry, mapped (schema major 5): its event's
/// marks and ornaments, its point marks (a fermata, a breath mark, a caesura,
/// dynamics), and the lines it begins or ends (a trill line, a glissando).
#[derive(Default)]
struct NoteMarks {
    event: Vec<EventMark>,
    ornaments: Vec<Ornament>,
    points: Vec<MarkerKind>,
    /// Whether it begins the line, the line's kind and number, and the line.
    lines: Vec<(bool, (&'static str, String), SpannerKind, LineStyle)>,
}

impl NoteMarks {
    /// Reads one child of `<notations>`, returning the kinds it holds that
    /// the model cannot, for the caller to record.
    fn read(&mut self, item: Node) -> Result<(), Vec<String>> {
        let mut unmapped = Vec::new();
        match name(item) {
            "articulations" => {
                for mark in elements(item) {
                    let mapped = match name(mark) {
                        "accent" => EventMark::Accent,
                        "strong-accent" => EventMark::Marcato,
                        "staccato" => EventMark::Staccato,
                        "tenuto" => EventMark::Tenuto,
                        "detached-legato" => EventMark::DetachedLegato,
                        "staccatissimo" => EventMark::Staccatissimo,
                        "spiccato" => EventMark::Spiccato,
                        "scoop" => EventMark::Scoop,
                        "plop" => EventMark::Plop,
                        "doit" => EventMark::Doit,
                        "falloff" => EventMark::Falloff,
                        "stress" => EventMark::Stress,
                        "unstress" => EventMark::Unstress,
                        "breath-mark" => {
                            match breath_mark(text(mark)) {
                                Some(b) => self.points.push(MarkerKind::Breath(b)),
                                None => unmapped.push(format!("breath mark {:?}", text(mark))),
                            }
                            continue;
                        }
                        "caesura" => {
                            match caesura(text(mark)) {
                                Some(c) => self.points.push(MarkerKind::Caesura(c)),
                                None => unmapped.push(format!("caesura {:?}", text(mark))),
                            }
                            continue;
                        }
                        other => {
                            unmapped.push(format!("articulations: {other}"));
                            continue;
                        }
                    };
                    self.event.push(mapped);
                }
            }
            "technical" => {
                for mark in elements(item) {
                    let mapped = match name(mark) {
                        "up-bow" => EventMark::UpBow,
                        "down-bow" => EventMark::DownBow,
                        "harmonic" => EventMark::Harmonic,
                        "open-string" | "open" => EventMark::OpenString,
                        "stopped" => EventMark::Stopped,
                        "snap-pizzicato" => EventMark::SnapPizzicato,
                        other => {
                            unmapped.push(format!("technical: {other}"));
                            continue;
                        }
                    };
                    self.event.push(mapped);
                }
            }
            "ornaments" => {
                for mark in elements(item) {
                    let kind = match name(mark) {
                        "trill-mark" => OrnamentKind::Trill,
                        "mordent" => OrnamentKind::Mordent,
                        "inverted-mordent" => OrnamentKind::InvertedMordent,
                        "turn" => OrnamentKind::Turn,
                        "inverted-turn" => OrnamentKind::InvertedTurn,
                        "accidental-mark" => {
                            let id = accidental_mark(text(mark));
                            match (self.ornaments.last_mut(), id) {
                                (Some(ornament), Some(id)) => {
                                    if mark.attribute("placement") == Some("below") {
                                        ornament.accidental_below = Some(id);
                                    } else {
                                        ornament.accidental_above = Some(id);
                                    }
                                }
                                _ => unmapped.push(String::from("ornament accidental")),
                            }
                            continue;
                        }
                        "tremolo" => {
                            let strokes = text(mark)
                                .trim()
                                .parse::<u8>()
                                .ok()
                                .filter(|s| (1..=8).contains(s));
                            match (mark.attribute("type").unwrap_or("single"), strokes) {
                                ("single", Some(strokes)) => {
                                    self.event.push(EventMark::Tremolo { strokes })
                                }
                                // The pairing is positional: the mark is on the
                                // first note, with the next of its voice.
                                ("start", Some(strokes)) => {
                                    self.event.push(EventMark::TremoloWithNext { strokes })
                                }
                                ("stop", _) => {}
                                (kind, _) => unmapped.push(format!("tremolo {kind}")),
                            }
                            continue;
                        }
                        "wavy-line" => {
                            let number = mark.attribute("number").unwrap_or("1").to_owned();
                            let key = ("trill line", number);
                            match mark.attribute("type") {
                                Some("start") => self.lines.push((
                                    true,
                                    key,
                                    SpannerKind::TrillExtension,
                                    LineStyle::Solid,
                                )),
                                Some("stop") => self.lines.push((
                                    false,
                                    key,
                                    SpannerKind::TrillExtension,
                                    LineStyle::Solid,
                                )),
                                _ => {}
                            }
                            continue;
                        }
                        // The closed "+" MuseScore writes as an ornament.
                        "other-ornament" if mark.attribute("smufl") == Some("brassMuteClosed") => {
                            self.event.push(EventMark::Stopped);
                            continue;
                        }
                        other => {
                            unmapped.push(format!("ornaments: {other}"));
                            continue;
                        }
                    };
                    self.ornaments.push(Ornament {
                        kind,
                        accidental_above: None,
                        accidental_below: None,
                    });
                }
            }
            "fermata" => match fermata_shape(text(item)) {
                Some(shape) => self.points.push(MarkerKind::Fermata(Fermata {
                    shape,
                    inverted: item.attribute("type") == Some("inverted"),
                })),
                None => unmapped.push(format!("fermata {:?}", text(item))),
            },
            "arpeggiate" => self.event.push(EventMark::Arpeggio {
                direction: match item.attribute("direction") {
                    Some("up") => ArpeggioDirection::Up,
                    Some("down") => ArpeggioDirection::Down,
                    _ => ArpeggioDirection::Plain,
                },
            }),
            "dynamics" => {
                for mark in elements(item) {
                    self.points.push(MarkerKind::Dynamic(dynamic(mark)));
                }
            }
            "slide" | "glissando" => {
                let kind = if name(item) == "slide" {
                    "slide"
                } else {
                    "glissando"
                };
                let line = match item.attribute("line-type") {
                    Some("solid") => Some(LineStyle::Solid),
                    Some("dashed") => Some(LineStyle::Dashed),
                    Some("dotted") => Some(LineStyle::Dotted),
                    Some("wavy") => Some(LineStyle::Wavy),
                    None if kind == "slide" => Some(LineStyle::Solid),
                    None => Some(LineStyle::Wavy),
                    Some(_) => None,
                };
                let key = (kind, item.attribute("number").unwrap_or("1").to_owned());
                match (item.attribute("type"), line) {
                    (Some("start"), Some(line)) => {
                        self.lines.push((true, key, SpannerKind::Glissando, line))
                    }
                    (Some("stop"), _) => {
                        self.lines
                            .push((false, key, SpannerKind::Glissando, LineStyle::Solid))
                    }
                    (Some("start"), None) => unmapped.push(format!("{kind} line")),
                    _ => {}
                }
            }
            other => unmapped.push(other.to_owned()),
        }
        if unmapped.is_empty() {
            Ok(())
        } else {
            Err(unmapped)
        }
    }

    /// A name for each thing it holds, for a note that carries them nowhere.
    fn kinds(&self) -> impl Iterator<Item = String> + '_ {
        self.event
            .iter()
            .map(mark_name)
            .chain(self.ornaments.iter().map(|_| String::from("ornament")))
            .chain(self.points.iter().map(|_| String::from("point mark")))
            .chain(self.lines.iter().map(|(_, key, ..)| key.0.to_owned()))
    }
}

/// An event mark's name in a recorded kind.
fn mark_name(mark: &EventMark) -> String {
    format!("mark {mark:?}")
}

/// A grace note's or a metronome's `<type>` or `<beat-unit>`, where the model
/// holds it.
fn note_value(value: &str) -> Option<NoteValue> {
    Some(match value.trim() {
        "whole" => NoteValue::Whole,
        "half" => NoteValue::Half,
        "quarter" => NoteValue::Quarter,
        "eighth" => NoteValue::Eighth,
        "16th" => NoteValue::Sixteenth,
        "32nd" => NoteValue::ThirtySecond,
        "64th" => NoteValue::SixtyFourth,
        _ => return None,
    })
}

fn breath_mark(value: &str) -> Option<BreathMark> {
    match value.trim() {
        "" | "comma" => Some(BreathMark::Comma),
        "tick" => Some(BreathMark::Tick),
        _ => None,
    }
}

fn caesura(value: &str) -> Option<CaesuraMark> {
    match value.trim() {
        "" | "normal" | "single" => Some(CaesuraMark::Normal),
        "thick" => Some(CaesuraMark::Thick),
        "short" => Some(CaesuraMark::Short),
        "curved" => Some(CaesuraMark::Curved),
        _ => None,
    }
}

fn fermata_shape(value: &str) -> Option<FermataShape> {
    match value.trim() {
        "" | "normal" => Some(FermataShape::Normal),
        "angled" => Some(FermataShape::Short),
        "square" => Some(FermataShape::Long),
        "double-angled" => Some(FermataShape::VeryShort),
        "double-square" => Some(FermataShape::VeryLong),
        _ => None,
    }
}

/// An ornament's accidental, by the name a spelling's accidental uses.
fn accidental_mark(value: &str) -> Option<AccidentalId> {
    match value.trim() {
        name @ ("sharp" | "flat" | "natural" | "double-sharp" | "sharp-sharp" | "flat-flat") => {
            Some(AccidentalId::new(name))
        }
        _ => None,
    }
}

/// One mark of a `<dynamics>`: a standard dynamic, or its text.
fn dynamic(mark: Node) -> Dynamic {
    match name(mark) {
        "pppppp" => Dynamic::Pppppp,
        "ppppp" => Dynamic::Ppppp,
        "pppp" => Dynamic::Pppp,
        "ppp" => Dynamic::Ppp,
        "pp" => Dynamic::Pp,
        "p" => Dynamic::P,
        "mp" => Dynamic::Mp,
        "mf" => Dynamic::Mf,
        "f" => Dynamic::F,
        "ff" => Dynamic::Ff,
        "fff" => Dynamic::Fff,
        "ffff" => Dynamic::Ffff,
        "fffff" => Dynamic::Fffff,
        "ffffff" => Dynamic::Ffffff,
        "fp" => Dynamic::Fp,
        "sf" => Dynamic::Sf,
        "sfz" => Dynamic::Sfz,
        "sffz" => Dynamic::Sffz,
        "sfp" => Dynamic::Sfp,
        "sfpp" => Dynamic::Sfpp,
        "rf" => Dynamic::Rf,
        "rfz" => Dynamic::Rfz,
        "fz" => Dynamic::Fz,
        "n" => Dynamic::Niente,
        "other-dynamics" => Dynamic::Other(Text::new(text(mark))),
        other => Dynamic::Other(Text::new(other)),
    }
}

/// A `<lyric>`: its syllable, nothing where it only continues an extender
/// line its verse's earlier syllable began, or the kind it holds that the
/// model cannot.
fn read_lyric(lyric: Node) -> Result<Option<SourceLyric>, String> {
    let verse = match lyric.attribute("number") {
        None => 1,
        Some(n) => n
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|v| *v >= 1)
            .ok_or_else(|| String::from("lyric verse not numbered"))?,
    };
    if child(lyric, "elision").is_some() {
        return Err(String::from("lyric elision"));
    }
    let syllable: String = children(lyric, "text").map(text).collect();
    let extend = child(lyric, "extend");
    if syllable.is_empty() {
        return match extend {
            Some(_) => Ok(None),
            None => Err(String::from("lyric with no syllable")),
        };
    }
    Ok(Some(SourceLyric {
        verse,
        text: Text::new(&syllable),
        syllabic: match child_text(lyric, "syllabic") {
            Some("begin") => Syllabic::Begin,
            Some("middle") => Syllabic::Middle,
            Some("end") => Syllabic::End,
            _ => Syllabic::Single,
        },
        extension: extend
            .is_some_and(|e| !matches!(e.attribute("type"), Some("stop") | Some("continue"))),
    }))
}

/// The fifths a transposition adds to a key: `7` per semitone less `12` per
/// diatonic step, so a B-flat instrument's `(-1, -2)` takes two flats and an
/// octave none.
pub(crate) fn key_shift(interval: TranspositionInterval) -> i32 {
    7 * interval.chromatic_steps - 12 * interval.diatonic_steps
}

/// The fifths added to a key a part writes to reach its concert key: none in
/// a concert score, whose keys are at concert pitch; in a transposed one, the
/// shift of the part's first `<transpose>`, its later changes being recorded
/// as unsupported.
fn concert_key_shift(part: Node, concert: bool) -> i32 {
    if concert {
        return 0;
    }
    children(part, "measure")
        .flat_map(|m| children(m, "attributes"))
        .flat_map(|a| children(a, "transpose"))
        .next()
        .map_or(0, |t| key_shift(read_interval(t)))
}

fn read_interval(node: Node) -> TranspositionInterval {
    let get = |n: &str| -> i32 {
        child_text(node, n)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    let octave = get("octave-change");
    TranspositionInterval {
        diatonic_steps: get("diatonic") + 7 * octave,
        chromatic_steps: get("chromatic") + 12 * octave,
    }
}
