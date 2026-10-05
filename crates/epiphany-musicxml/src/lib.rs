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
//! - The whole score is one metric region. Meter changes are
//!   `SetTimeSignature` operations; each staff instance is created carrying
//!   every clef and key change the file makes on it, since no operation adds
//!   one afterward; measures are created on every staff at the onsets the
//!   parts' content reaches.
//! - A quarter-tone is held in `cmn-24`, whose chromatic step is the
//!   quarter-tone; every other pitch in `cmn-12`. It is stated by a
//!   fractional `<alter>` or, as MuseScore writes its arrowed and Stein
//!   accidentals, by an `<accidental>` name with no `<alter>`, carried through
//!   the measure and over a tie as notation carries it. A pitch finer than a
//!   quarter-tone refuses the file by name.
//! - A quarter-tone is spelt as its notation writes it. The spelling
//!   pre-pass infers no `cmn-24` spelling, so the importer authors one with
//!   `RespellPitch`: the pitch's letter and octave, and the arrowed or Stein
//!   accidental the file writes or carries to the note, by its MusicXML name,
//!   moved to the sounding pitch with its kind kept.
//! - The core ties only pitches it can call enharmonic, which it answers in
//!   twelve-chromatic spaces alone, so a tie between quarter-tones is
//!   recorded, not made.
//! - Voices are per staff: a MusicXML voice that crosses staves becomes a
//!   voice on each. Unpitched notes keep their staff step (read against a
//!   treble clef, bottom line 0) and their instrument member, and a tie
//!   between two of the same member and step pairs no pitch.
//! - Every operation comes from one replica in one causal chain, so the import
//!   is deterministic and reduces as one author's history.
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
