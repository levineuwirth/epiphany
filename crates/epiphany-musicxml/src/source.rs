//! Reading partwise MusicXML into a [`SourceScore`]: the source as the file
//! states it, with every time an exact rational in whole notes.
//!
//! Every element the reader meets is either mapped into the source model or
//! recorded in [`Features`] by kind, so nothing is dropped without a trace.
//! The reader interprets MusicXML; it builds no Epiphany value and emits no
//! operation (that is [`crate::emit`]).

use std::collections::BTreeMap;

use epiphany_core::{
    AcousticPitch, AcousticRealization, Clef, ClefShape, CmnNominal, Pitch, PitchSpaceId,
    PitchSpacePosition, RationalTime, ScalePosition, TranspositionInterval, TuningReference,
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
}

/// What an event is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Content {
    /// A rest; `visible` is false for `print-object="no"`.
    Rest { visible: bool },
    /// One or more pitches sounding together (a note or a chord).
    Pitched(Vec<SourcePitch>),
    /// An unpitched percussion note: its staff step (bottom line 0, one per
    /// diatonic step, read against a treble clef as MusicXML prescribes) and
    /// the index of its member in [`SourcePart::members`].
    Unpitched { step: i16, member: usize },
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
    /// Chord notes recorded as unsupported and not imported: a cross-staff
    /// chord note, an unpitched chord note, a chord note joining a rest.
    pub dropped_notes: usize,
}

/// Raw counts taken straight from the `<note>` elements of a part, with no
/// timing logic, and the keys and clefs its `<attributes>` state, as an
/// independent check on the reader.
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
    /// Grace and cue notes, which are not imported.
    pub grace_or_cue: usize,
    /// Per staff, the `fifths` of each `<key>` the model can hold (at most
    /// seven accidentals) that applies to it: a numbered key to its staff,
    /// an unnumbered one to every staff the part's `<staves>` declare.
    pub keys: Vec<Vec<i8>>,
    /// Per staff, each `<clef>` of a shape the model holds that applies to
    /// it: a numbered clef to its staff, an unnumbered one to the first.
    pub clefs: Vec<Vec<Clef>>,
}

/// The keys and clefs of a part's `<attributes>`, per staff, read straight
/// from the elements. It shares none of the reader's order of reading, so it
/// holds the reader's placement of them to account.
fn attribute_census(part: Node) -> (Vec<Vec<i8>>, Vec<Vec<Clef>>) {
    let attributes = || children(part, "measure").flat_map(|m| children(m, "attributes"));
    let staves = attributes()
        .flat_map(|a| children(a, "staves"))
        .filter_map(|s| text(s).parse::<usize>().ok())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut keys = vec![Vec::new(); staves];
    let mut clefs = vec![Vec::new(); staves];
    for a in attributes() {
        for key in children(a, "key") {
            let Some(fifths) = child_text(key, "fifths")
                .and_then(|f| f.parse::<i8>().ok())
                .filter(|f| (-7..=7).contains(f))
            else {
                continue;
            };
            match key.attribute("number") {
                None => keys.iter_mut().for_each(|k| k.push(fifths)),
                Some(n) => {
                    let staff = n.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
                    if let Some(list) = staff.and_then(|s| keys.get_mut(s)) {
                        list.push(fifths);
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
                list.push(value);
            }
        }
    }
    (keys, clefs)
}

/// A partwise MusicXML score as the file states it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceScore {
    pub title: Option<String>,
    pub composer: Option<String>,
    /// `<defaults><concert-score/>`: the file writes transposing parts at
    /// concert pitch.
    pub concert: bool,
    pub parts: Vec<SourcePart>,
    pub measures: Vec<SourceMeasure>,
    /// The meter changes of the first part, which govern the score.
    pub meters: Vec<MeterChange>,
    pub features: Features,
    /// Per part, in part order.
    pub census: Vec<Census>,
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

struct PartState {
    divisions: i64,
    /// What is added to a pitch in the file to reach the sounding pitch.
    file_transpose: Option<TranspositionInterval>,
    /// Open slurs by number: the index of their start event.
    open_slurs: BTreeMap<String, usize>,
    /// The last event a `<chord/>` note would join.
    last_event: Option<usize>,
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
        let mut score_parts: BTreeMap<String, Node> = BTreeMap::new();
        let mut part_nodes = Vec::new();
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
                    for entry in elements(node) {
                        match name(entry) {
                            "score-part" => {
                                let id = entry.attribute("id").unwrap_or("").to_owned();
                                score_parts.insert(id, entry);
                            }
                            "part-group" => {
                                if entry.attribute("type") == Some("start") {
                                    let symbol =
                                        child_text(entry, "group-symbol").unwrap_or("none");
                                    self.features.record(
                                        FeatureClass::Content,
                                        format!("part group ({symbol})"),
                                        score_place(),
                                    );
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
        for node in part_nodes {
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

        Ok(SourceScore {
            title,
            composer,
            concert,
            parts,
            measures,
            meters,
            features: self.features,
            census,
        })
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
            open_slurs: BTreeMap::new(),
            last_event: None,
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
            dropped_notes: 0,
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
                    "direction" => self.read_direction(item, &place),
                    "barline" => self.read_barline(item, &place),
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
                    "sound" => {
                        let class = if item.attribute("tempo").is_some() {
                            FeatureClass::Content
                        } else {
                            FeatureClass::Presentation
                        };
                        let kind = if class == FeatureClass::Content {
                            "sound: tempo"
                        } else {
                            "sound: playback"
                        };
                        self.features.record(class, kind, place.clone());
                    }
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
        (read.census.keys, read.census.clefs) = attribute_census(node);
        Ok(read)
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
        let grace = child(note, "grace").is_some();
        let cue = child(note, "cue").is_some();
        if grace || cue {
            read.census.grace_or_cue += 1;
            let kind = if grace { "grace note" } else { "cue note" };
            self.features
                .record(FeatureClass::Content, kind, place.clone());
            // A cue note occupies time in its voice; a grace note does not.
            if cue && !is_chord {
                *last_onset = *cursor;
                *cursor += self.duration(note)?;
            }
            return Ok(());
        }
        let has_pitch = child(note, "pitch");
        let has_unpitched = child(note, "unpitched");
        let has_rest = child(note, "rest");
        match (
            has_pitch.is_some(),
            has_unpitched.is_some(),
            has_rest.is_some(),
        ) {
            (true, false, false) => read.census.pitched += 1,
            (false, true, false) => read.census.unpitched += 1,
            (false, false, true) => read.census.rests += 1,
            _ => {
                return Err(
                    self.malformed(note, "a note needs exactly one of pitch, unpitched or rest")
                )
            }
        }
        if is_chord {
            read.census.chord_members += 1;
        }

        let duration_div = self.duration(note)?;
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

        let onset_div = if is_chord { *last_onset } else { *cursor };
        let visible = note.attribute("print-object") != Some("no");

        // The notes' own children, mapped or recorded.
        for item in elements(note) {
            match name(item) {
                "chord" | "pitch" | "unpitched" | "rest" | "duration" | "voice" | "staff"
                | "tie" | "type" | "dot" | "accidental" | "time-modification" | "instrument" => {}
                "notations" => {}
                "stem" => {
                    self.features
                        .record(FeatureClass::Notation, "stem direction", place.clone())
                }
                "beam" => {
                    if item.attribute("number").unwrap_or("1") == "1" && text(item) == "begin" {
                        self.features
                            .record(FeatureClass::Notation, "beam", place.clone());
                    }
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
                "lyric" => self
                    .features
                    .record(FeatureClass::Content, "lyric", place.clone()),
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
        let explicit_accidental = child(note, "accidental").is_some_and(|a| {
            a.attribute("cautionary") == Some("yes")
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
        if let Some(modification) = child(note, "time-modification") {
            let actual = child_text(modification, "actual-notes").unwrap_or("?");
            let normal = child_text(modification, "normal-notes").unwrap_or("?");
            for notations in children(note, "notations") {
                for tuplet in children(notations, "tuplet") {
                    if tuplet.attribute("type") == Some("start") {
                        self.features.record(
                            FeatureClass::Content,
                            format!("tuplet {actual}:{normal}"),
                            place.clone(),
                        );
                    }
                }
            }
        }
        let mut slur_marks: Vec<(String, String)> = Vec::new();
        for notations in children(note, "notations") {
            for item in elements(notations) {
                match name(item) {
                    "tied" | "tuplet" => {}
                    "slur" => slur_marks.push((
                        item.attribute("type").unwrap_or("").to_owned(),
                        item.attribute("number").unwrap_or("1").to_owned(),
                    )),
                    "articulations" | "ornaments" | "technical" => {
                        for mark in elements(item) {
                            let spanning = matches!(name(mark), "wavy-line")
                                && mark.attribute("type") != Some("start");
                            if !spanning && name(mark) != "accidental-mark" {
                                self.features.record(
                                    FeatureClass::Content,
                                    format!("{}: {}", name(item), name(mark)),
                                    place.clone(),
                                );
                            } else if name(mark) == "accidental-mark" {
                                self.features.record(
                                    FeatureClass::Content,
                                    "ornament accidental",
                                    place.clone(),
                                );
                            }
                        }
                    }
                    "dynamics" => self.features.record(
                        FeatureClass::Content,
                        "dynamics on a note",
                        place.clone(),
                    ),
                    "slide" | "glissando" => {
                        if item.attribute("type") == Some("start") {
                            self.features.record(
                                FeatureClass::Content,
                                name(item).to_owned(),
                                place.clone(),
                            );
                        }
                    }
                    other => {
                        self.features
                            .record(FeatureClass::Content, other.to_owned(), place.clone())
                    }
                }
            }
        }

        let content = if let Some(pitch) = has_pitch {
            let step = child_text(pitch, "step").unwrap_or("");
            let nominal =
                nominal_of(step).ok_or_else(|| self.malformed(pitch, format!("step {step:?}")))?;
            let octave: i8 = child_text(pitch, "octave")
                .and_then(|o| o.parse().ok())
                .ok_or_else(|| self.malformed(pitch, "a pitch without an octave"))?;
            // `<alter>` counts semitones; a quarter-tone is half of one, and
            // the model holds it in `cmn-24`. Anything finer is refused,
            // never rounded.
            let alter_text = child_text(pitch, "alter").unwrap_or("0");
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
            let written = quarter_tone_pitch(nominal, doubled as i8, octave);
            let sounding = sounding(written, state.file_transpose).map_err(|refusal| {
                self.malformed(pitch, format!("cannot transpose to sounding: {refusal:?}"))
            })?;
            Some(SourcePitch {
                pitch: sounding,
                tie_start,
                tie_stop,
            })
        } else {
            None
        };

        let event_index = if is_chord {
            let Some(last) = state.last_event else {
                return Err(self.malformed(note, "a chord note with no note before it"));
            };
            let event = &mut part.events[last];
            match (&mut event.content, content) {
                (Content::Pitched(pitches), Some(pitch)) if event.staff == staff => {
                    pitches.push(pitch);
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
                    self.features.record(
                        FeatureClass::Content,
                        "cross-staff chord note",
                        place.clone(),
                    );
                }
                (Content::Unpitched { .. }, _) if has_unpitched.is_some() => {
                    part.dropped_notes += 1;
                    self.features.record(
                        FeatureClass::Content,
                        "unpitched chord note",
                        place.clone(),
                    );
                }
                _ => {
                    part.dropped_notes += 1;
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
                Content::Unpitched { step, member }
            } else {
                Content::Rest { visible }
            };
            let duration = RationalTime::new(duration_div, 4 * state.divisions)
                .ok_or_else(|| self.malformed(note, "a duration out of range"))?;
            part.events.push(SourceEvent {
                measure,
                staff,
                voice,
                onset: zero(),
                duration,
                content,
                offset: note.range().start,
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
        Ok(())
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

    fn read_direction(&mut self, direction: Node, place: &Place) {
        for item in elements(direction) {
            match name(item) {
                "direction-type" => {
                    for kind in elements(item) {
                        let starts = match name(kind) {
                            "wedge" => kind
                                .attribute("type")
                                .is_some_and(|t| t == "crescendo" || t == "diminuendo"),
                            "octave-shift" => kind
                                .attribute("type")
                                .is_some_and(|t| t == "up" || t == "down"),
                            "pedal" | "dashes" | "bracket" => {
                                kind.attribute("type") == Some("start")
                            }
                            _ => true,
                        };
                        if starts {
                            self.features.record(
                                FeatureClass::Content,
                                format!("direction: {}", name(kind)),
                                place.clone(),
                            );
                        }
                    }
                }
                "sound" => {
                    let class = if item.attribute("tempo").is_some() {
                        FeatureClass::Content
                    } else {
                        FeatureClass::Presentation
                    };
                    let kind = if class == FeatureClass::Content {
                        "sound: tempo"
                    } else {
                        "sound: playback"
                    };
                    self.features.record(class, kind, place.clone());
                }
                "offset" | "staff" | "voice" | "listening" => {}
                other => self.features.record(
                    FeatureClass::Presentation,
                    format!("direction: {other}"),
                    place.clone(),
                ),
            }
        }
    }

    fn read_barline(&mut self, barline: Node, place: &Place) {
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
                other => (FeatureClass::Content, format!("barline: {other}")),
            };
            self.features.record(class, kind, place.clone());
        }
    }
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
