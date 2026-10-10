#![forbid(unsafe_code)]
//! MusicXML import for Epiphany.
//!
//! A partwise MusicXML file is read into a source model ([`source`]), the
//! operations that build it from an empty score are emitted through the
//! public operation API ([`emit`]), the operation set is reduced and every
//! operation's outcome read back ([`outcome`]), and the reduced score is
//! compared against the source ([`fidelity`]). What the source holds that has
//! no operation or mapping is recorded by kind in [`source::Features`], never
//! approximated.
//!
//! Mapping decisions a reader of the output needs:
//!
//! - Pitches are stored as they sound. A concert score's pitches already are;
//!   a transposed score's are moved by its `transpose` element. Each part's
//!   written-versus-sounding interval, octave changes folded in, goes on its
//!   instrument (`part-transpose` in a concert score), so the written pitch is
//!   the stored one moved by the inverse interval.
//! - Keys are stored at concert pitch too. A concert score's already are; a
//!   transposed score's are moved by the fifths of the part's first
//!   `transpose`. An open key (`<mode>none</mode>`) is no key signature,
//!   which no transposition moves: where no key is in force it is not
//!   stored, and after one it is stored as no accidentals and recorded,
//!   since the model holds no open key. Whether the file is a concert score
//!   is kept on the source (`SourceScore::concert`), and a score is drawn as
//!   its file is set.
//! - The whole score is one metric region. Meter changes are
//!   `SetTimeSignature` operations; each staff instance is created carrying
//!   every clef and key change the file makes on it, since no operation adds
//!   one afterward; measures are created on every staff at the onsets the
//!   parts' content reaches.
//! - A quarter-tone is held in `cmn-24`, whose chromatic step is the
//!   quarter-tone; every other pitch in `cmn-12`. It is stated by a
//!   fractional `<alter>` or, as MuseScore writes its arrowed and Stein
//!   accidentals, by an `<accidental>` name with no `<alter>`. Such an
//!   accidental applies to its own note, as MuseScore reads it, and over a
//!   tie to the note continuing it; a later note of its line in the measure
//!   that writes neither is unaltered. A pitch finer than a quarter-tone
//!   refuses the file by name.
//! - A quarter-tone is spelt as its notation writes it. The spelling
//!   pre-pass infers no `cmn-24` spelling, so the importer authors one with
//!   `RespellPitch`: the pitch's letter and octave, and the arrowed or Stein
//!   accidental the file writes on the note or carries to it over a tie, by
//!   its MusicXML name, moved to the sounding pitch with its kind kept.
//! - A tie pairs pitches equal in their space's chromatic layer, which the
//!   core decides in `cmn-24` as in `cmn-12`, so a tie between quarter-tones
//!   is made as any other.
//! - Voices are per staff: a MusicXML voice that crosses staves becomes a
//!   voice on each. Unpitched notes keep their staff step (read against a
//!   treble clef, bottom line 0) and their instrument member, and a tie
//!   between two of the same member and step pairs no pitch.
//! - Every operation comes from one replica in one causal chain, so the import
//!   is deterministic and reduces as one author's history.
//! - Expression and text import through the operations (schema major 5). A
//!   note's articulations, technical marks and arpeggio become its event's
//!   marks, one of each kind over its chord; its trills, mordents and turns,
//!   with an accidental mark above or below, its ornaments; a single-note
//!   tremolo its strokes, and a two-note tremolo's start a mark on the first
//!   note, the pairing being positional. MuseScore's closed "+" written as
//!   `other-ornament` is the stopped mark, once with `<stopped/>`.
//! - A grace note is an event of zero duration at the position of the note it
//!   precedes, its slash, notated value and dots in its payload and its place
//!   among the graces there its order. It beams with graces alone and is no
//!   tuplet's member; a tie on one is recorded, not read.
//! - Dynamics, fermatas (on a note, a rest or a barline), breath marks,
//!   caesuras, staff text, tempo and metronome marks, rehearsal marks, segno
//!   and coda are markers. One a direction places stands on the first note or
//!   rest of its staff starting where it stands (the note, not a grace before
//!   it), or where none starts, at the position in the region. A run of
//!   `<words>` is one text; the words of a text line are its text, and those
//!   of a tempo its mark's.
//! - A `<sound tempo>` sets a segment of the score's tempo map, in quarter
//!   notes per minute, once at each place any part sets one; the mark its
//!   direction shows is written in the same transaction, with no link
//!   stored (D58, Q7).
//! - Hairpins, pedal lines (without their sign where the file says so),
//!   ottavas (`down` sounds above), text lines, brackets, trill lines and
//!   glissandi (a slide straight, a glissando wavy unless the file says
//!   otherwise) are spanners from the point their start stands to their
//!   stop's, paired by kind and number; a start of a line already open, a
//!   stop with none open, or a line never stopped is recorded, since the
//!   file does not say which stop ends which.
//! - A lyric is one syllable an event and verse, its text in NFC; a syllable
//!   only continuing its verse's extender line is the earlier syllable's
//!   extension.
//! - A voice at home on one staff that writes on another is a voice on each,
//!   and the visiting one names its home staff (`SetVoiceHome`), which the
//!   layout reads to draw it on its home's side (D46).
//!
//! ```text
//! let import = epiphany_musicxml::import(&xml)?;
//! let reduced = epiphany_musicxml::outcome::reduce(&import);
//! let fidelity = epiphany_musicxml::fidelity::compare(&import, &reduced);
//! ```

pub mod emit;
pub mod fidelity;
pub mod outcome;
pub mod source;

pub use emit::{Import, DEFAULT_REPLICA};
pub use source::{ReadError, SourcePage, SourceScore};

/// Reads a partwise MusicXML document and emits the operations that build it,
/// authored from [`DEFAULT_REPLICA`].
pub fn import(xml: &str) -> Result<Import, ReadError> {
    import_as(xml, DEFAULT_REPLICA)
}

/// [`import`], authored from `replica`.
pub fn import_as(xml: &str, replica: epiphany_core::ReplicaId) -> Result<Import, ReadError> {
    Ok(emit::emit(SourceScore::read(xml)?, replica))
}
