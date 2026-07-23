//! Pitch primitives and the spelling subsystem (Chapter 2; Chapter 4 for the
//! tuning/pitch-space registry identifiers a pitch references).
//!
//! A [`Pitch`] is two independent intrinsic layers (Chapter 2 §"The Pitch
//! Type"): a [`ScalePosition`] (analytical identity within a pitch space) and
//! an [`AcousticPitch`] (sounding-frequency identity, relative to a tuning
//! system). Spelling — what the performer sees — is **not** a field on the
//! pitch (Chapter 2 §"Spelling"); it is attached externally, indexed by
//! [`PitchId`], through [`SpellingAttachment`].
//!
//! Pitches embedded in events are wrapped in [`IdentifiedPitch`], pairing the
//! pitch with its stable [`PitchId`] (Chapter 5 §"Identified Pitches").

use epiphany_determinism::{
    canonical_f64_bytes, CanonicalF64, SystemDomainTag, Tolerance, ToleranceClass,
};
use unicode_normalization::UnicodeNormalization;

use crate::ids::{derive_system_id, AnalysisLayerId, PitchId, VoiceId};
use crate::time::TimeAnchor;

/// Defines a catalog / registry identifier: a named entry in one of the
/// score's registries (pitch spaces, tuning systems, accidental registries,
/// …). The built-in catalog uses short ASCII names like `"cmn-12"` and
/// `"tet-12"` (Chapter 4 §"Built-in Catalog"), so the identifier is a string.
///
/// Appendix D §"Text and Unicode" makes canonical text identity byte comparison
/// of the UTF-8 **NFC** form: *"Canonical text fields MUST be encoded as UTF-8
/// with Unicode NFC applied … Comparisons of canonical text fields for identity
/// MUST be byte comparisons of NFC-encoded UTF-8."* [`$name::new`] therefore
/// **normalizes to NFC on construction**, so two canonically-equivalent names
/// (e.g. precomposed "é" U+00E9 vs decomposed "e"+U+0301) intern to the same
/// value and compare, hash, and order equal. The built-in catalog names are
/// ASCII (already NFC) and so are unaffected.
macro_rules! catalog_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Interns a catalog name, normalizing it to Unicode NFC so
            /// canonically-equivalent spellings compare equal (Appendix D
            /// §"Text and Unicode").
            #[inline]
            pub fn new(name: impl Into<String>) -> Self {
                $name(name.into().nfc().collect())
            }
            /// The catalog name.
            #[inline]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, concat!(stringify!($name), "({:?})"), self.0)
            }
        }
        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

catalog_id!(
    /// Identifies a pitch space (Chapter 4 §"Pitch Spaces"). Built-ins include
    /// `cmn-12`, `edo-31`, `ji-5limit`.
    PitchSpaceId
);
catalog_id!(
    /// Identifies a tuning system (Chapter 4 §"Tuning Systems"). Built-ins
    /// include `tet-12`, `werckmeister-iii`.
    TuningSystemId
);
catalog_id!(
    /// Identifies an accidental registry (Chapter 4 §"Accidental Registries").
    AccidentalRegistryId
);
catalog_id!(
    /// Identifies an accidental within a registry (Chapter 4).
    AccidentalId
);
catalog_id!(
    /// Identifies a nominal registry (Chapter 4).
    NominalRegistryId
);
catalog_id!(
    /// Identifies a registered (grammar-defined) scale position
    /// (Chapter 2 §"Scale Position").
    PositionRegistryId
);
catalog_id!(
    /// Identifies a registered tie class with custom validation behaviour
    /// (Chapter 5 §"Ties", `TieClass::Registered`).
    TieClassRegistryId
);
catalog_id!(
    /// Identifies a registered staff-group kind (Chapter 5
    /// §"Top-Level Score Structure", `StaffGroupKind::Registered`).
    StaffGroupKindRegistryId
);
catalog_id!(
    /// Identifies a spelling rule set (Chapter 4 §"Spelling Rule Sets").
    SpellingRuleSetId
);
catalog_id!(
    /// Identifies a spelling algorithm family (Chapter 2 §"The Spelling
    /// Pre-Pass"). The v0 stub registers [`SpellingAlgorithmId::default_id`].
    SpellingAlgorithmId
);
catalog_id!(
    /// Identifies a notational-decomposition algorithm family (Chapter 3
    /// §"Sounding Duration and Notational Decomposition"). Versioned the same
    /// way as [`SpellingAlgorithmId`]: the id is part of the derivation key for
    /// the decomposition pre-pass, so a profile-declared change deterministically
    /// invalidates derived decompositions. The Phase-2 default
    /// ([`DecompositionAlgorithmId::default_id`]) resolves to the metric
    /// greedy-aligned splitter in [`crate::prepass`].
    DecompositionAlgorithmId
);
catalog_id!(
    /// Identifies a foreign interchange format (e.g. MusicXML), used as a
    /// spelling/decomposition provenance tag.
    ForeignFormatId
);
catalog_id!(
    /// Identifies a registered (grammar-defined) position structure
    /// (Chapter 4 §"Position Structure", `PositionStructure::Registered`).
    PositionStructureRegistryId
);
catalog_id!(
    /// Identifies a registered (grammar-defined) interval algebra
    /// (Chapter 4 §"Interval Algebra", `IntervalAlgebra::Registered`).
    IntervalAlgebraRegistryId
);
catalog_id!(
    /// Identifies a registered (grammar-defined) transposition behavior
    /// (Chapter 4 §"Transposition Behavior", `TranspositionBehavior::Registered`).
    TranspositionRegistryId
);

impl SpellingAlgorithmId {
    /// The Phase-2 default spelling algorithm, registered under the id
    /// `"default"`. The id resolves to the deterministic Temperley-style
    /// line-of-fifths pre-pass implemented in [`crate::prepass`] (Chapter 2
    /// §"The Spelling Pre-Pass"). The literal id is part of the derivation key:
    /// changing the registered algorithm changes the id, so derived spellings
    /// computed under a different version never silently alias.
    pub fn default_id() -> Self {
        SpellingAlgorithmId::new("default")
    }
}

impl DecompositionAlgorithmId {
    /// The Phase-2 default decomposition algorithm, registered under the id
    /// `"default"`. The id resolves to the deterministic metric greedy-aligned
    /// splitter implemented in [`crate::prepass`] (Chapter 3 §"Sounding Duration
    /// and Notational Decomposition").
    pub fn default_id() -> Self {
        DecompositionAlgorithmId::new("default")
    }
}

/// The seven CMN diatonic nominals. The discriminants are **normative**: they
/// define the diatonic step ordering used by transposition (Chapter 2
/// §"The CmnNominal Type").
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CmnNominal {
    C = 0,
    D = 1,
    E = 2,
    F = 3,
    G = 4,
    A = 5,
    B = 6,
}

impl CmnNominal {
    /// The nominal's position in the 12-chromatic layer for `cmn-12`
    /// (`C=0, D=2, E=4, F=5, G=7, A=9, B=11`), per the
    /// `DiatonicOverChromatic` mapping in Chapter 4.
    pub fn chromatic(self) -> u8 {
        match self {
            CmnNominal::C => 0,
            CmnNominal::D => 2,
            CmnNominal::E => 4,
            CmnNominal::F => 5,
            CmnNominal::G => 7,
            CmnNominal::A => 9,
            CmnNominal::B => 11,
        }
    }

    /// The nominal for a diatonic index in `0..7`, the inverse of the type's
    /// normative discriminant (Chapter 2 §"The CmnNominal Type"). Panics
    /// outside the range; callers reduce with `rem_euclid(7)` first.
    fn from_index(index: i32) -> Self {
        match index {
            0 => CmnNominal::C,
            1 => CmnNominal::D,
            2 => CmnNominal::E,
            3 => CmnNominal::F,
            4 => CmnNominal::G,
            5 => CmnNominal::A,
            6 => CmnNominal::B,
            other => unreachable!("diatonic index out of 0..7: {other}"),
        }
    }
}

/// A signed interval: the transposition primitive (Chapter 2
/// §"Transposition and the Interval Type"; schema major 2). Both components
/// are load-bearing — the diatonic one fixes the spelling (which nominal,
/// hence which staff line), the chromatic one fixes the sound. Neither
/// determines the other: an augmented second `(1, 3)` and a minor third
/// `(2, 3)` sound alike and are written differently.
///
/// [`Instrument::transposition`](crate::Instrument) carries one as its
/// written-versus-sounding interval (a B-flat clarinet is `-1` diatonic, `-2`
/// chromatic). Its *action* is pinned by [`Pitch::transposed`]; what stays
/// advisory is its automatic application at the written/sounding boundary,
/// which nothing in the core performs yet.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TranspositionInterval {
    pub diatonic_steps: i32,
    pub chromatic_steps: i32,
}

impl TranspositionInterval {
    /// The interval that undoes this one, or `None` when it is not
    /// representable: `i32::MIN` has no `i32` negation. Exact where it exists,
    /// because [`Pitch::transposed`] never saturates — it refuses instead.
    ///
    /// The non-representable case is explicit rather than a panic or a wrap:
    /// an interval whose inverse cannot be written down is a fact about the
    /// type, and a caller composing undo out of inverses must see it.
    pub fn inverse(self) -> Option<Self> {
        Some(TranspositionInterval {
            diatonic_steps: self.diatonic_steps.checked_neg()?,
            chromatic_steps: self.chromatic_steps.checked_neg()?,
        })
    }
}

/// Why a pitch could not be faithfully transposed (Chapter 2
/// `req:pitch:transposition`). Each variant is a way a transposition would
/// otherwise have to lie.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TransposeRefusal {
    /// The position is not [`PitchSpacePosition::Cmn`], so it has no nominal
    /// for the interval's diatonic component to move.
    NonCmnPosition,
    /// The enclosing pitch space's chromatic layer or nominal mapping cannot
    /// be established, so applying the interval would require guessed
    /// arithmetic (`req:pitch:space-capability-refusal`). Structural: the
    /// space's [`PositionStructure`](crate::pitch_space::PositionStructure)
    /// is looked up in [`crate::pitch_space::built_in_position_structure`]
    /// and must resolve to
    /// [`DiatonicOverChromatic`](crate::pitch_space::PositionStructure::DiatonicOverChromatic);
    /// every other outcome — `Chromatic`, `JiLattice`, `Registered`, an
    /// unknown identifier, or one of the six built-in spaces the
    /// specification names but does not structurally determine — refuses.
    PitchSpaceUnavailable,
    /// The pitch's [`AcousticRealization::AbsoluteHz`] overrides the tuning
    /// system, so moving the scale position would move the notehead without
    /// moving the sound.
    AcousticPinned,
    /// The resulting `alteration` or `octave` does not fit its `i8` field.
    OutOfRange,
}

/// Resolves `space` to its [`DiatonicOverChromatic`](crate::pitch_space::PositionStructure::DiatonicOverChromatic)
/// chromatic cardinality and nominal mapping against the built-in catalog
/// ([`crate::pitch_space::built_in_position_structure`]), or `None` for
/// every other case — `Chromatic`, `JiLattice`, `Registered`, an unknown
/// identifier, or one of the six catalog spaces the specification does not
/// structurally determine. This is the structural replacement for the
/// retired P13-S2 `"cmn-12"` identifier check
/// (`req:pitch:space-capability-refusal`): [`Pitch::transposed`] and
/// [`Pitch::twelve_tet_semitone`] are its only two call sites, matching the
/// two places the interim guard used to live.
fn diatonic_over_chromatic_structure(space: &PitchSpaceId) -> Option<(u16, Vec<u16>)> {
    match crate::pitch_space::built_in_position_structure(space)? {
        crate::pitch_space::PositionStructure::DiatonicOverChromatic {
            chromatic_positions_per_octave,
            nominal_to_chromatic,
            ..
        } => Some((chromatic_positions_per_octave, nominal_to_chromatic)),
        _ => None,
    }
}

impl Pitch {
    /// Transposes this pitch by `interval`, per Chapter 2
    /// `req:pitch:transposition`.
    ///
    /// With `n` the nominal's normative discriminant, `C` the enclosing
    /// space's `chromatic_positions_per_octave`, and `m` its
    /// `nominal_to_chromatic` mapping (both resolved structurally — see
    /// below), the absolute chromatic coordinate is
    /// `s = m(nominal) + alteration + C*octave`, and transposing by
    /// `{ d, c }` yields
    ///
    /// ```text
    /// nominal'    = CmnNominal((n + d).rem_euclid(7))
    /// octave'     = octave + (n + d).div_euclid(7)
    /// alteration' = (s + c) - (m(nominal') + C*octave')
    /// ```
    ///
    /// `C` and `m` are resolved by looking `self.scale_position.space` up in
    /// [`crate::pitch_space::built_in_position_structure`]: when it resolves
    /// to [`DiatonicOverChromatic`](crate::pitch_space::PositionStructure::DiatonicOverChromatic),
    /// that structure's own `chromatic_positions_per_octave` and
    /// `nominal_to_chromatic` are used — which is what makes `cmn-24`
    /// transpose in quarter-tone steps rather than semitones. Every other
    /// resolution — `Chromatic`, `JiLattice`, `Registered`, an unknown
    /// identifier, or one of the six built-in spaces the specification does
    /// not structurally determine — refuses
    /// ([`TransposeRefusal::PitchSpaceUnavailable`],
    /// `req:pitch:space-capability-refusal`) rather than guessing a
    /// structure.
    ///
    /// The diatonic component alone selects the nominal and octave; the
    /// alteration absorbs exactly the residue. So in `cmn-12`, `C4 + (7, 12)`
    /// is `C5`, not "C with twelve sharps", and `C4 + (0, 1)` is `C#4`.
    ///
    /// Refuses rather than saturating, clamping, or approximating: a
    /// transposition that silently produces a pitch nobody asked for reports
    /// success and destroys the evidence needed to notice.
    pub fn transposed(&self, interval: TranspositionInterval) -> Result<Pitch, TransposeRefusal> {
        if matches!(
            self.acoustic.realization,
            AcousticRealization::AbsoluteHz(_)
        ) {
            return Err(TransposeRefusal::AcousticPinned);
        }
        let PitchSpacePosition::Cmn {
            nominal,
            alteration,
            octave,
        } = self.scale_position.position
        else {
            return Err(TransposeRefusal::NonCmnPosition);
        };
        let (chromatic_card, nominal_to_chromatic) =
            diatonic_over_chromatic_structure(&self.scale_position.space)
                .ok_or(TransposeRefusal::PitchSpaceUnavailable)?;

        // Widen to `i64` before any arithmetic. The `i8` bound is a *result*
        // constraint, not an intermediate one, so an octave that overflows on
        // the way to a value that fits would be a spurious refusal — but
        // `i32` is not wide enough to hold the intermediates for an `i32`
        // interval, and the previous version of this function panicked on
        // `diatonic_steps = i32::MAX` at `chromatic_card * new_octave`.
        // Refusing is the contract; panicking on a value the public type
        // admits is not.
        //
        // `i64` is amply wide: `step` is bounded by `6 + 2^31`, so
        // `new_octave` by `2^31/7 + 127`, and the largest intermediate
        // `chromatic_card * new_octave` by roughly `chromatic_card * 3e8`,
        // nowhere near `i64::MAX` for any `u16` cardinality.
        let chromatic_of = |nom: CmnNominal| i64::from(nominal_to_chromatic[nom as usize]);
        let c = i64::from(chromatic_card);
        let n = i64::from(nominal as u8);
        let semitone = chromatic_of(nominal) + i64::from(alteration) + c * i64::from(octave);
        let step = n + i64::from(interval.diatonic_steps);
        let new_nominal = CmnNominal::from_index(step.rem_euclid(7) as i32);
        let new_octave = i64::from(octave) + step.div_euclid(7);
        let new_alteration = (semitone + i64::from(interval.chromatic_steps))
            - (chromatic_of(new_nominal) + c * new_octave);

        let octave = i8::try_from(new_octave).map_err(|_| TransposeRefusal::OutOfRange)?;
        let alteration = i8::try_from(new_alteration).map_err(|_| TransposeRefusal::OutOfRange)?;

        let mut out = self.clone();
        out.scale_position.position = PitchSpacePosition::Cmn {
            nominal: new_nominal,
            alteration,
            octave,
        };
        Ok(out)
    }
}

/// A position within a pitch space (Chapter 2 §"Scale Position"). Tagged union
/// with a fast CMN path plus a registry escape hatch for arbitrary grammars.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum PitchSpacePosition {
    /// Common Music Notation: diatonic nominal + chromatic alteration + octave.
    Cmn {
        nominal: CmnNominal,
        /// Chromatic alteration in steps of the enclosing pitch space's
        /// chromatic layer (`req:pitch:alteration-unit`), conventionally
        /// `-2..=+2`. One step is a semitone in `cmn-12` and a quarter-tone in
        /// `cmn-24`, so a flat is respectively `-1` and `-2`.
        alteration: i8,
        /// Scientific Pitch Notation octave; middle C is C4.
        octave: i8,
    },
    /// N-tone integer position (serial, EDO).
    Integer { space_size: u16, index: i32 },
    /// Just-intonation lattice vector; one exponent per prime in the space's
    /// declared basis (Chapter 2 §"Scale Position", `JiVector`).
    JiVector { components: Vec<i32> },
    /// A registered position resolved by the pitch space's grammar plugin.
    Registered(PositionRegistryId),
}

/// A pitch's analytical identity (Chapter 2 §"Scale Position").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ScalePosition {
    /// The pitch space this position is defined within.
    pub space: PitchSpaceId,
    /// The position within that space.
    pub position: PitchSpacePosition,
}

/// A reference to the tuning system governing a pitch (Chapter 2
/// §"Tuning Reference and Inheritance").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TuningReference {
    /// Inherit the tuning system from the enclosing scope. Malformed at score
    /// level (the score's tuning system must be explicit).
    Inherit,
    /// Explicitly named tuning system.
    Explicit(TuningSystemId),
}

/// How the tuning system resolves to a frequency (Chapter 2 §"Acoustic
/// Realization"). The cents/Hz payloads are [`CanonicalF64`] so a NaN/inf/`-0.0`
/// can never enter canonical state (Appendix D §"Floating-Point Values").
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AcousticRealization {
    /// Resolve through the tuning system using the scale position alone (the
    /// default for ordinary CMN).
    Implicit,
    /// An explicit offset in cents from the tuning system's result.
    CentsOffset(CanonicalF64),
    /// An explicit absolute frequency in Hertz, overriding the tuning system.
    AbsoluteHz(CanonicalF64),
}

impl AcousticRealization {
    /// Builds a cents offset, rejecting a non-finite value (Appendix D).
    pub fn cents_offset(cents: f64) -> Option<Self> {
        CanonicalF64::new(cents).map(AcousticRealization::CentsOffset)
    }
    /// Builds an absolute-frequency realization, rejecting a non-finite or
    /// non-positive frequency (Chapter 4 §"Reference Pitch": "positive and
    /// finite").
    pub fn absolute_hz(hz: f64) -> Option<Self> {
        if hz > 0.0 {
            CanonicalF64::new(hz).map(AcousticRealization::AbsoluteHz)
        } else {
            None
        }
    }
}

/// The reference pitch anchoring a tuning system's ratios to absolute Hertz
/// (Chapter 4 §"Reference Pitch"). A score-level property, not a tuning-system
/// one. The frequency is a finite, positive [`CanonicalF64`].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ReferencePitch {
    /// The pitch-space position chosen as the reference (conventionally A4).
    pub position: PitchSpacePosition,
    /// The frequency in Hertz; **positive** and finite. Private so it cannot be
    /// mutated into an invalid value after construction (Chapter 4: "positive
    /// and finite") — the only way to set it is through [`ReferencePitch::new`].
    frequency_hz: CanonicalF64,
}

impl ReferencePitch {
    /// Builds a reference pitch, rejecting a non-finite or non-positive
    /// frequency (Chapter 4: "positive and finite").
    pub fn new(position: PitchSpacePosition, frequency_hz: f64) -> Option<Self> {
        if frequency_hz > 0.0 {
            CanonicalF64::new(frequency_hz).map(|frequency_hz| ReferencePitch {
                position,
                frequency_hz,
            })
        } else {
            None
        }
    }

    /// The reference frequency in Hertz (always positive and finite).
    #[inline]
    pub fn frequency_hz(&self) -> f64 {
        self.frequency_hz.get()
    }

    /// The conventional default: A4 = 440 Hz in `cmn-12` (Chapter 4 §"Default
    /// Score Configuration").
    pub fn a440() -> Self {
        ReferencePitch::new(
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::A,
                alteration: 0,
                octave: 4,
            },
            440.0,
        )
        .expect("A4=440 is a valid reference pitch")
    }
}

/// A pitch's acoustic identity (Chapter 2 §"Acoustic Realization").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AcousticPitch {
    /// The tuning system governing this pitch's frequency.
    pub tuning: TuningReference,
    /// How the tuning system resolves to a frequency.
    pub realization: AcousticRealization,
}

/// A pitch's intrinsic identity: scale position plus acoustic realization
/// (Chapter 2 §"The Pitch Type"). Spellings are attached externally.
///
/// Derived `Eq` is **structural equality** (Chapter 2 §"Equality and
/// Comparison"). The computed equivalences — scale-position and enharmonic —
/// are separate methods and must never be conflated with structural equality.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Pitch {
    /// The analytical identity within a pitch space.
    pub scale_position: ScalePosition,
    /// The acoustic identity.
    pub acoustic: AcousticPitch,
}

impl Pitch {
    /// Scale-position equivalence (Chapter 2): equal `ScalePosition` fields,
    /// ignoring acoustic realization. Exact; never tolerant.
    pub fn scale_position_equivalent(&self, other: &Pitch) -> bool {
        self.scale_position == other.scale_position
    }

    /// The 12-TET pitch class (`0..=11`) of this pitch's *scale position*, when
    /// its 12-chromatic structure is established: `Cmn` positions whose space
    /// resolves structurally to a `DiatonicOverChromatic` structure with
    /// exactly 12 chromatic positions (built-in `cmn-12`, at this tranche's
    /// catalog), and 12-EDO integer positions. Returns `None` for CMN
    /// positions in spaces that do not resolve that way (e.g. `cmn-24`,
    /// whose 24 quarter-tone positions are not a 12-TET pitch class at all)
    /// and positions whose 12-TET class is not determinable without a tuning
    /// resolver (JI vectors, non-12 EDOs, registered grammars). Octave-blind
    /// — for sounding comparison use [`Pitch::twelve_tet_semitone`].
    pub fn twelve_tet_class(&self) -> Option<u8> {
        self.twelve_tet_semitone().map(|s| s.rem_euclid(12) as u8)
    }

    /// The *absolute* 12-TET semitone of this pitch's scale position, octave
    /// included, when its 12-chromatic structure is established. For a `Cmn`
    /// position this is `octave*C + m(nominal) + alteration`, where `C` and
    /// `m` come from the space's structural resolution
    /// ([`crate::pitch_space::built_in_position_structure`]) — but only when
    /// `C` is exactly `12`; a space resolving to `DiatonicOverChromatic` with
    /// any other chromatic cardinality (`cmn-24`, for instance) is **not** a
    /// 12-TET semitone and this deliberately does not answer for it (the
    /// name `twelve_tet_semitone` stays honest per
    /// `spec/CONTRACT_PUSH4B_PITCHSPACES.md` item 4: it answers only when the
    /// resolved structure truly has twelve chromatic positions, never by
    /// identifier). For 12-EDO integer positions it is the absolute `index`.
    /// `None` for CMN positions whose space does not resolve to a
    /// `DiatonicOverChromatic` structure with exactly 12 chromatic positions
    /// (`req:pitch:space-capability-refusal`) and positions not determinable
    /// without a tuning resolver.
    ///
    /// The CMN and 12-EDO frames use different zero references, so the absolute
    /// value is only meaningful *within* a frame;
    /// [`Pitch::enharmonic_equivalent`] compares only same-frame positions for
    /// that reason.
    pub fn twelve_tet_semitone(&self) -> Option<i32> {
        match &self.scale_position.position {
            PitchSpacePosition::Cmn {
                nominal,
                alteration,
                octave,
            } => {
                let (chromatic_card, nominal_to_chromatic) =
                    diatonic_over_chromatic_structure(&self.scale_position.space)?;
                if chromatic_card != 12 {
                    return None;
                }
                let m = nominal_to_chromatic[*nominal as usize] as i32;
                Some(*octave as i32 * 12 + m + *alteration as i32)
            }
            PitchSpacePosition::Integer { space_size, index } if *space_size == 12 => Some(*index),
            _ => None,
        }
    }

    /// Enharmonic equivalence (Chapter 2): sounding-equivalent under 12-tone
    /// equal temperament, regardless of the actual tuning system. This is a
    /// *sounding* notion, so octave matters — C4 and C5 are **not**
    /// enharmonically equivalent (they sound an octave apart); C-sharp4 and
    /// D-flat4 are. Computed from the absolute 12-TET semitone
    /// ([`Pitch::twelve_tet_semitone`]).
    ///
    /// Pitches in **different pitch spaces** are not directly comparable
    /// (Chapter 2 §"Scale Position": "pitches in different spaces cannot be
    /// directly compared and operations between them MUST go through an explicit
    /// space-conversion mechanism"), so this returns `false` unless both share a
    /// [`ScalePosition::space`]. The two computable frames (CMN, 12-EDO integer)
    /// also use different zero references, so a cross-frame pair returns `false`
    /// too. `false` for non-determinable positions.
    ///
    /// General *sounding* equivalence across arbitrary tuning systems is
    /// [`Pitch::sounding_equivalent`].
    pub fn enharmonic_equivalent(&self, other: &Pitch) -> bool {
        if self.scale_position.space != other.scale_position.space {
            return false;
        }
        match (self.twelve_tet_semitone(), other.twelve_tet_semitone()) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Sounding equivalence (Chapter 2's third computed relation): two pitches
    /// are sounding-equivalent if they resolve to the same frequency under their
    /// respective tuning systems, within a [`Tolerance`] of the named
    /// [`ToleranceClass::AcousticCents`] class.
    ///
    /// The tolerance is the named class, not a raw `f64`: Appendix D §"Tolerance
    /// Classes" forbids ad-hoc epsilons, and a [`Tolerance`] cannot carry
    /// infinity or NaN ([`epiphany_determinism::CanonicalF64`] bounds). A
    /// tolerance of any *other* class is a category error and never matches.
    ///
    /// Frequency resolution in general depends on the full tuning-system catalog
    /// and reference pitch — a separate subsystem (the acoustic engine,
    /// Chapter 1; see `DECISIONS.md`), not modeled in this crate. Callers at that
    /// layer pass a `resolve` closure mapping a pitch to its frequency in Hertz
    /// (`None` if it cannot resolve it). An [`AcousticRealization::AbsoluteHz`]
    /// pitch resolves to its own stated frequency without the closure. Returns
    /// `false` if either frequency is unavailable.
    pub fn sounding_equivalent(
        &self,
        other: &Pitch,
        tolerance: Tolerance,
        mut resolve: impl FnMut(&Pitch) -> Option<f64>,
    ) -> bool {
        // The comparison is in cents, so it MUST use the AcousticCents class
        // (Appendix D); a tolerance of any other class never matches.
        if tolerance.class != ToleranceClass::AcousticCents {
            return false;
        }
        let freq = |p: &Pitch, resolve: &mut dyn FnMut(&Pitch) -> Option<f64>| -> Option<f64> {
            match p.acoustic.realization {
                AcousticRealization::AbsoluteHz(hz) => Some(hz.get()),
                _ => resolve(p),
            }
        };
        let mut r = &mut resolve;
        match (freq(self, &mut r), freq(other, &mut r)) {
            (Some(a), Some(b)) if a > 0.0 && b > 0.0 => {
                // Compare in cents against zero: |1200·log2(a/b)| within
                // tolerance. `Tolerance::within` rejects a non-finite cents
                // value (e.g. an infinite resolved frequency), so the comparison
                // can never be spuriously satisfied. This is a derived, tolerant
                // comparison (never canonical state), so the transcendental is
                // admissible (Appendix D applies to canonical numeric output).
                let cents = 1200.0 * (a / b).log2().abs();
                tolerance.within(cents, 0.0)
            }
            _ => false,
        }
    }
}

/// A closed pitch range, `lowest..=highest` (Chapter 2; the type
/// `core_spec` references for [`Instrument`](crate::Instrument)'s declared
/// range). Both endpoints are full [`Pitch`] values, so a range is expressed in
/// a specific pitch space; membership is only decidable when a candidate shares
/// a comparison frame with the endpoints (the same "sound but incomplete"
/// discipline as [`Pitch::enharmonic_equivalent`]). Derived `Eq` is structural.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PitchRange {
    /// The lowest sounding pitch admitted (inclusive).
    pub lowest: Pitch,
    /// The highest sounding pitch admitted (inclusive).
    pub highest: Pitch,
}

impl PitchRange {
    /// Whether `pitch` lies within `lowest..=highest`, decided by absolute
    /// 12-TET semitone ([`Pitch::twelve_tet_semitone`]). Returns `None` — the
    /// indeterminate case its advisory caller treats as a pass — when:
    ///
    /// * the three pitches do not all share a [`PitchSpaceId`] frame (absolute
    ///   semitones across frames use different zero references and are not
    ///   comparable);
    /// * any pitch's semitone is not determinable without a tuning resolver; or
    /// * the range is **malformed** in the comparable frame — `lowest` sorts
    ///   strictly above `highest`. A range is well-formed only when `lowest`
    ///   does not sort above `highest` (core spec §"Instrument"); a reversed
    ///   range is undecidable, not "empty", so it must not reject every pitch.
    pub fn contains(&self, pitch: &Pitch) -> Option<bool> {
        let frame = &self.lowest.scale_position.space;
        if &self.highest.scale_position.space != frame || &pitch.scale_position.space != frame {
            return None;
        }
        let lo = self.lowest.twelve_tet_semitone()?;
        let hi = self.highest.twelve_tet_semitone()?;
        if lo > hi {
            return None; // malformed (reversed) range: undecidable, not empty
        }
        let p = pitch.twelve_tet_semitone()?;
        Some(lo <= p && p <= hi)
    }
}

/// Canonical, deterministic bytes for a [`Pitch`]'s intrinsic content (scale
/// position plus acoustic realization), used to derive a content-addressed
/// system pitch identifier. Strings are length-prefixed and already NFC (the
/// catalog ids normalize on construction); the layout is fixed-shape so equal
/// pitches encode to equal bytes (Appendix D §"Canonical serialization").
///
/// Public because these bytes are the normative "canonical inputs" of the
/// `MUSCSPCH` derivation (`req:graph:system-derived-pitch-id`): the reduction's
/// system-derived counter-collision check (Chapter 5 §"System-Derived Counter
/// Collisions") compares exactly these input bytes to distinguish two pitches
/// contending for one derived counter.
pub fn canonical_pitch_bytes(p: &Pitch) -> Vec<u8> {
    // Length-prefixed UTF-8, normalized to NFC at the derivation boundary so the
    // canonical input is NFC regardless of how the string was obtained (Appendix
    // D §"Text and Unicode"). Catalog ids are already NFC at construction, so this
    // is a no-op for them; normalizing here makes the NFC guarantee explicit and
    // robust rather than relying on every caller.
    fn push_str(out: &mut Vec<u8>, s: &str) {
        let nfc: String = s.nfc().collect();
        out.extend_from_slice(&(nfc.len() as u32).to_le_bytes());
        out.extend_from_slice(nfc.as_bytes());
    }
    let mut out = Vec::new();
    push_str(&mut out, p.scale_position.space.as_str());
    match &p.scale_position.position {
        PitchSpacePosition::Cmn {
            nominal,
            alteration,
            octave,
        } => {
            out.push(0);
            out.push(*nominal as u8);
            out.extend_from_slice(&alteration.to_le_bytes());
            out.extend_from_slice(&octave.to_le_bytes());
        }
        PitchSpacePosition::Integer { space_size, index } => {
            out.push(1);
            out.extend_from_slice(&space_size.to_le_bytes());
            out.extend_from_slice(&index.to_le_bytes());
        }
        PitchSpacePosition::JiVector { components } => {
            out.push(2);
            out.extend_from_slice(&(components.len() as u32).to_le_bytes());
            for c in components {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        PitchSpacePosition::Registered(id) => {
            out.push(3);
            push_str(&mut out, id.as_str());
        }
    }
    match &p.acoustic.tuning {
        TuningReference::Inherit => out.push(0),
        TuningReference::Explicit(t) => {
            out.push(1);
            push_str(&mut out, t.as_str());
        }
    }
    let f64_bytes = |x: f64| canonical_f64_bytes(x).expect("CanonicalF64 is finite");
    match &p.acoustic.realization {
        AcousticRealization::Implicit => out.push(0),
        AcousticRealization::CentsOffset(c) => {
            out.push(1);
            out.extend_from_slice(&f64_bytes(c.get()));
        }
        AcousticRealization::AbsoluteHz(h) => {
            out.push(2);
            out.extend_from_slice(&f64_bytes(h.get()));
        }
    }
    out
}

/// Derives the deterministic [`PitchId`] of a *system-derived* (synthetic)
/// pitch in the [`crate::ReplicaId::SYSTEM_DERIVED`] namespace, content-addressed
/// from the pitch's intrinsic identity via the `MUSCSPCH` domain tag (Chapter 5
/// §"System-Derived Identifiers").
///
/// The spec defers the exact derivation function for synthetic pitches; this is
/// the prototype's concrete realization — the canonical inputs are the pitch's
/// content bytes (`canonical_pitch_bytes`) — so two replicas synthesizing the same pitch
/// derive a byte-identical id. Recorded as a Pass 11 candidate in `DECISIONS.md`
/// (mirrors [`crate::derive_promoted_voice_id`]). The graph-invariant checker
/// uses it to prove a `SYSTEM_DERIVED` embedded pitch is a legitimate
/// derivation rather than an arbitrary counter.
pub fn derive_system_pitch_id(p: &Pitch) -> PitchId {
    derive_system_id::<PitchId>(SystemDomainTag::PITCH, &canonical_pitch_bytes(p))
}

/// A pitch embedded in an event, paired with its stable identifier (Chapter 5
/// §"Identified Pitches"). The identifier enables spelling attachments,
/// respelling, pitch-level reduction, and stable tie pairing through chord
/// reordering.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct IdentifiedPitch {
    pub id: PitchId,
    pub pitch: Pitch,
}

/// The staff position (nominal) a spelling draws on (Chapter 2 §"The Spelling
/// Attachment").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SpellingNominal {
    /// CMN nominal (the fast path).
    Cmn(CmnNominal),
    /// Integer nominal for N-tone systems.
    Integer(i32),
    /// Registered nominal for grammar-specific systems.
    Registered(NominalRegistryId),
}

/// Optional rendering hints on a spelling (Chapter 2). Non-normative for
/// sounding pitch.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct SpellingRenderHints {
    pub parenthesized: bool,
    pub cautionary: bool,
    pub editorial: bool,
    pub small_print: bool,
}

/// An explicit spelling for a pitch: staff position, accidental stack, octave,
/// and render hints (Chapter 2 §"The Spelling Attachment").
///
/// The `accidentals` stack is ordered innermost-first for engraving; an empty
/// stack means *no glyph is drawn*, which is distinct from a stack containing
/// only a natural sign (Chapter 2 §"Absent Accidentals"). The accidental-stack
/// well-formedness rule (no repeated [`AccidentalId`] unless the accidental's
/// registered combination permits it) is checked by
/// [`PitchSpelling::accidental_stack_is_well_formed`].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PitchSpelling {
    pub nominal: SpellingNominal,
    /// Accidental stack, innermost (closest to the notehead) first.
    pub accidentals: Vec<AccidentalId>,
    pub octave: i8,
    pub render_hints: SpellingRenderHints,
}

impl PitchSpelling {
    /// Moves this spelling by `interval`, keeping its enharmonic choice.
    ///
    /// The authored **nominal** is what carries that choice — an author who
    /// wrote B♯3 rather than C4 chose the letter B — so the diatonic component
    /// moves the nominal and octave, and the accidental stack is then whatever
    /// the transposed pitch requires on that new staff line.
    /// `sounding_semitone` is the transposed pitch's
    /// [`Pitch::twelve_tet_semitone`].
    ///
    /// So B♯3 (sounding C4) transposed by a perfect fifth `(4, 7)` becomes
    /// F×4 (sounding G4): the letter moved four diatonic steps — B→F carries
    /// the octave — and the
    /// double-sharp is what F needs to sound a G. The chromatic component never
    /// touches the spelling directly; it reaches it only through the pitch.
    ///
    /// `None` when the spelling is not CMN (no nominal to move), or when the
    /// resulting octave or alteration is not representable.
    pub fn transposed(
        &self,
        interval: TranspositionInterval,
        sounding_semitone: i32,
    ) -> Option<PitchSpelling> {
        let SpellingNominal::Cmn(nominal) = self.nominal else {
            return None;
        };
        // `i64` throughout, for the same reason `Pitch::transposed` does.
        let step = i64::from(nominal as u8) + i64::from(interval.diatonic_steps);
        let new_nominal = CmnNominal::from_index(step.rem_euclid(7) as i32);
        let new_octave = i64::from(self.octave) + step.div_euclid(7);
        let alteration =
            i64::from(sounding_semitone) - (i64::from(new_nominal.chromatic()) + 12 * new_octave);

        let octave = i8::try_from(new_octave).ok()?;
        let alteration = i32::try_from(alteration).ok()?;
        // Guard the accidental stack: `accidental_ids` allocates |alteration|/2
        // glyphs, so an unbounded alteration is an unbounded allocation.
        i8::try_from(alteration).ok()?;
        Some(PitchSpelling {
            nominal: SpellingNominal::Cmn(new_nominal),
            accidentals: crate::prepass::accidental_ids(alteration),
            octave,
            render_hints: self.render_hints,
        })
    }

    /// A bare CMN spelling with no accidental glyph at the given octave.
    pub fn cmn(nominal: CmnNominal, octave: i8) -> Self {
        PitchSpelling {
            nominal: SpellingNominal::Cmn(nominal),
            accidentals: Vec::new(),
            octave,
            render_hints: SpellingRenderHints::default(),
        }
    }

    /// Whether the accidental stack is free of repeated identifiers. The
    /// conventional double-sharp/double-flat are single accidental
    /// definitions, never repeated singles (Chapter 2 §"Accidental Stack
    /// Semantics"). Registered accidentals that permit repetition are out of
    /// scope for this baseline check.
    pub fn accidental_stack_is_well_formed(&self) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        self.accidentals.iter().all(|a| seen.insert(a.clone()))
    }
}

/// Provenance of a spelling attachment (Chapter 2 §"Spelling Sources").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SpellingSource {
    /// The user explicitly chose this spelling.
    UserChosen,
    /// Inferred by the spelling pre-pass. Lowest default precedence.
    Inferred,
    /// Imported from a foreign format.
    Imported { format: ForeignFormatId },
    /// Propagated from a transposition or other edit.
    Propagated { from: PitchId },
    /// An analytical spelling on a non-engraved layer.
    Analytical,
}

/// The provenance *kind* of a spelling source, for precedence ordering
/// (Chapter 2 §"Configurable Precedence").
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SpellingSourceKind {
    UserChosen,
    Imported,
    Propagated,
    Inferred,
    Analytical,
}

impl SpellingSource {
    /// The precedence kind of this source.
    pub fn kind(&self) -> SpellingSourceKind {
        match self {
            SpellingSource::UserChosen => SpellingSourceKind::UserChosen,
            SpellingSource::Imported { .. } => SpellingSourceKind::Imported,
            SpellingSource::Propagated { .. } => SpellingSourceKind::Propagated,
            SpellingSource::Inferred => SpellingSourceKind::Inferred,
            SpellingSource::Analytical => SpellingSourceKind::Analytical,
        }
    }
}

/// A score's spelling-precedence configuration: a total ordering over the
/// spelling-source kinds (Chapter 2 §"Configurable Precedence"). Earlier in the
/// `order` vector wins. Every score must carry one; the default ranks
/// `UserChosen > Imported > Propagated > Inferred`, with `Analytical` last
/// (analytical spellings live on their own layers).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SpellingPrecedence {
    order: Vec<SpellingSourceKind>,
}

impl Default for SpellingPrecedence {
    fn default() -> Self {
        SpellingPrecedence {
            order: vec![
                SpellingSourceKind::UserChosen,
                SpellingSourceKind::Imported,
                SpellingSourceKind::Propagated,
                SpellingSourceKind::Inferred,
                SpellingSourceKind::Analytical,
            ],
        }
    }
}

impl SpellingPrecedence {
    /// Builds a precedence from a total order. Returns `None` unless every
    /// source kind appears exactly once (the spec requires a *total* ordering).
    pub fn new(order: Vec<SpellingSourceKind>) -> Option<Self> {
        let mut seen = std::collections::BTreeSet::new();
        if order.len() != 5 || !order.iter().all(|k| seen.insert(*k)) {
            return None;
        }
        Some(SpellingPrecedence { order })
    }

    /// The precedence order, for the canonical codec.
    pub(crate) fn order_ref(&self) -> &[SpellingSourceKind] {
        &self.order
    }

    /// The rank of a source kind: lower wins. `0` is the highest precedence.
    pub fn rank(&self, kind: SpellingSourceKind) -> usize {
        self.order
            .iter()
            .position(|k| *k == kind)
            .expect("precedence is total over all source kinds")
    }
}

/// A voice selector for scope-level directives (Chapter 2 §"Spelling
/// Attachment"; completed in Chapter 6). Minimal baseline: all voices, or an
/// explicit set.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum VoiceSelector {
    /// Every voice in scope.
    #[default]
    All,
    /// An explicit set of voices.
    Voices(Vec<VoiceId>),
}

/// A rule for inferring spellings within a scope (Chapter 2 §"Spelling
/// Attachment"). Baseline placeholder referencing a rule set; the rule
/// parameters are the spelling-pre-pass open question (Appendix D).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SpellingRule {
    pub rule_set: SpellingRuleSetId,
}

/// What a spelling attachment applies to (Chapter 2).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SpellingScope {
    /// Applies to one specific pitch.
    Pitch(PitchId),
    /// Applies to all matching pitches in a time range.
    Range {
        start: TimeAnchor,
        end: TimeAnchor,
        voices: VoiceSelector,
    },
}

/// A spelling directive: an explicit spelling or an inference rule (Chapter 2).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SpellingDirective {
    /// An explicit spelling for a single pitch. Only valid with
    /// [`SpellingScope::Pitch`].
    Explicit(PitchSpelling),
    /// A rule for inferring spellings within a scope.
    Rule(SpellingRule),
}

/// An externally-stored spelling, indexed by pitch identifier (Chapter 2
/// §"The Spelling Attachment").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SpellingAttachment {
    pub scope: SpellingScope,
    pub directive: SpellingDirective,
    pub source: SpellingSource,
    /// Tie-break priority among attachments; higher wins (after precedence).
    pub priority: i32,
    /// Analysis layer; `None` is the engraved layer.
    pub layer: Option<AnalysisLayerId>,
}

impl SpellingAttachment {
    /// Whether this attachment is internally well-formed: an
    /// [`SpellingDirective::Explicit`] directive is valid only with a
    /// [`SpellingScope::Pitch`] scope (Chapter 2: "Only valid with
    /// SpellingScope::Pitch").
    pub fn is_well_formed(&self) -> bool {
        !matches!(
            (&self.scope, &self.directive),
            (SpellingScope::Range { .. }, SpellingDirective::Explicit(_))
        )
    }
}

/// Context consumed by the spelling pre-pass (Chapter 2 §"The Spelling
/// Pre-Pass"). Baseline placeholder; the real context (key signature,
/// in-measure accidental state, melodic/harmonic context) is filled in once the
/// pre-pass algorithm receives an Appendix D disposition.
#[derive(Clone, Debug, Default)]
pub struct SpellingContext {
    /// The pitch space active for the pitch being spelled.
    pub space: Option<PitchSpaceId>,
}

/// The context-free spelling of a single pitch: its authored CMN letter if it
/// has one, else the simplest (fewest-accidental) enharmonic spelling of its
/// 12-TET pitch class (Chapter 2 §"The Spelling Pre-Pass").
///
/// This is the *isolated* entry point. Real, context-aware spelling — the
/// Temperley line-of-fifths pre-pass that resolves a pitch by its melodic
/// neighbours — is a function of the whole score and lives in
/// [`crate::prepass::derive_annotations`]; the `_ctx` argument is retained for
/// source compatibility. A pitch whose space declares spelling unavailable
/// (no determinable 12-TET class) falls back to a middle-C advisory default.
pub fn spell(p: &Pitch, _ctx: &SpellingContext) -> PitchSpelling {
    crate::prepass::simplest_spelling(p).unwrap_or_else(|| PitchSpelling::cmn(CmnNominal::C, 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmn(nominal: CmnNominal, alteration: i8, octave: i8) -> Pitch {
        cmn_in("cmn-12", nominal, alteration, octave)
    }

    fn cmn_in(space: &str, nominal: CmnNominal, alteration: i8, octave: i8) -> Pitch {
        Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new(space),
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

    #[test]
    fn cmn_nominal_discriminants_are_normative() {
        assert_eq!(CmnNominal::C as u8, 0);
        assert_eq!(CmnNominal::B as u8, 6);
        // The cmn-12 chromatic mapping.
        assert_eq!(CmnNominal::C.chromatic(), 0);
        assert_eq!(CmnNominal::E.chromatic(), 4);
        assert_eq!(CmnNominal::B.chromatic(), 11);
    }

    fn iv(diatonic_steps: i32, chromatic_steps: i32) -> TranspositionInterval {
        TranspositionInterval {
            diatonic_steps,
            chromatic_steps,
        }
    }

    fn position(p: &Pitch) -> (CmnNominal, i8, i8) {
        match p.scale_position.position {
            PitchSpacePosition::Cmn {
                nominal,
                alteration,
                octave,
            } => (nominal, alteration, octave),
            ref other => panic!("expected a Cmn position, got {other:?}"),
        }
    }

    #[test]
    fn transposing_an_octave_moves_the_octave_not_the_alteration() {
        // The P12-K2 defect, locked: the old alteration-shift produced
        // `C4 alteration 12` — a C with six double-sharps — for this input.
        let up = cmn(CmnNominal::C, 0, 4).transposed(iv(7, 12)).unwrap();
        assert_eq!(position(&up), (CmnNominal::C, 0, 5));

        let down = cmn(CmnNominal::C, 0, 4).transposed(iv(-7, -12)).unwrap();
        assert_eq!(position(&down), (CmnNominal::C, 0, 3));
    }

    #[test]
    fn the_diatonic_component_picks_the_nominal_and_the_alteration_absorbs_the_rest() {
        // A perfect fifth up: C4 -> G4, no accidental.
        let fifth = cmn(CmnNominal::C, 0, 4).transposed(iv(4, 7)).unwrap();
        assert_eq!(position(&fifth), (CmnNominal::G, 0, 4));

        // A *diminished* sixth up sounds the same and is spelled a step away.
        let dim6 = cmn(CmnNominal::C, 0, 4).transposed(iv(5, 7)).unwrap();
        assert_eq!(position(&dim6), (CmnNominal::A, -2, 4));

        // Sharpening: the editor's `transpose_selection(1)` equivalent. This
        // is the one case the frozen operation already got right.
        let sharp = cmn(CmnNominal::C, 0, 4).transposed(iv(0, 1)).unwrap();
        assert_eq!(position(&sharp), (CmnNominal::C, 1, 4));
    }

    #[test]
    fn transposition_crosses_the_octave_boundary_in_both_directions() {
        // B4 up a semitone spelled as a diatonic step: C5, not B#4.
        let up = cmn(CmnNominal::B, 0, 4).transposed(iv(1, 1)).unwrap();
        assert_eq!(position(&up), (CmnNominal::C, 0, 5));

        // C4 down a diatonic step: B3. Euclidean, so the negative index wraps
        // to 6 and the octave carries -1.
        let down = cmn(CmnNominal::C, 0, 4).transposed(iv(-1, -1)).unwrap();
        assert_eq!(position(&down), (CmnNominal::B, 0, 3));

        // B#4 exists and sounds as C5, an octave *below* the C it spells past.
        let bsharp = cmn(CmnNominal::B, 1, 4).transposed(iv(0, 0)).unwrap();
        assert_eq!(position(&bsharp), (CmnNominal::B, 1, 4));
    }

    #[test]
    fn every_transposition_is_undone_by_its_inverse() {
        let start = cmn(CmnNominal::E, -1, 3);
        for interval in [iv(4, 7), iv(-2, -3), iv(7, 12), iv(0, 1), iv(5, 7)] {
            let there = start.transposed(interval).unwrap();
            let back = there
                .transposed(interval.inverse().expect("these inverses exist"))
                .unwrap();
            assert_eq!(
                position(&back),
                position(&start),
                "{interval:?} did not round-trip"
            );
        }
    }

    #[test]
    fn a_transposition_refuses_rather_than_saturating() {
        // The old operation clamped `alteration` at the i8 bound and reported
        // success. An octave past the i8 bound must refuse instead.
        let high = cmn(CmnNominal::C, 0, 127);
        assert_eq!(
            high.transposed(iv(7, 12)),
            Err(TransposeRefusal::OutOfRange)
        );
        let low = cmn(CmnNominal::C, 0, -128);
        assert_eq!(
            low.transposed(iv(-7, -12)),
            Err(TransposeRefusal::OutOfRange)
        );

        // An alteration that overflows while the octave is fine.
        let alt = cmn(CmnNominal::C, 120, 4);
        assert_eq!(alt.transposed(iv(0, 20)), Err(TransposeRefusal::OutOfRange));

        // Nothing is mutated on refusal — `transposed` is by-value.
        assert_eq!(position(&high), (CmnNominal::C, 0, 127));
    }

    #[test]
    fn an_extreme_interval_refuses_instead_of_panicking() {
        // The public type admits any `i32`. Every one of these overflowed an
        // `i32` intermediate and panicked (or, with overflow-checks off, wrapped
        // through unspecified arithmetic) before the widening to `i64`.
        let p = cmn(CmnNominal::C, 0, 4);
        for (d, c) in [
            (i32::MAX, 0),
            (i32::MIN, 0),
            (0, i32::MAX),
            (0, i32::MIN),
            (i32::MAX, i32::MAX),
            (i32::MIN, i32::MIN),
            (i32::MAX, i32::MIN),
        ] {
            assert_eq!(
                p.transposed(iv(d, c)),
                Err(TransposeRefusal::OutOfRange),
                "({d}, {c}) must refuse, not panic"
            );
        }
        // The boundary either side of a representable octave shift.
        assert!(cmn(CmnNominal::C, 0, 126).transposed(iv(7, 12)).is_ok());
        assert_eq!(
            cmn(CmnNominal::C, 0, 127).transposed(iv(7, 12)),
            Err(TransposeRefusal::OutOfRange)
        );
    }

    #[test]
    fn the_inverse_of_the_unrepresentable_interval_is_none() {
        // `-i32::MIN` is not an `i32`. Negating it panicked.
        assert_eq!(iv(i32::MIN, 0).inverse(), None);
        assert_eq!(iv(0, i32::MIN).inverse(), None);
        assert_eq!(iv(i32::MIN, i32::MIN).inverse(), None);
        assert_eq!(iv(i32::MIN + 1, 0).inverse(), Some(iv(i32::MAX, 0)));
        assert_eq!(iv(4, 7).inverse(), Some(iv(-4, -7)));
    }

    #[test]
    fn a_spelling_moves_by_its_nominal_and_keeps_the_authors_enharmonic_choice() {
        let semitone = |p: &Pitch| p.twelve_tet_semitone().unwrap();

        // The author wrote C-sharp, not D-flat. Up a perfect fifth that must be
        // G-sharp, not A-flat: the pre-pass, left to itself, may prefer either.
        let cs4 = cmn(CmnNominal::C, 1, 4);
        let spelled = PitchSpelling {
            nominal: SpellingNominal::Cmn(CmnNominal::C),
            accidentals: crate::prepass::accidental_ids(1),
            octave: 4,
            render_hints: SpellingRenderHints::default(),
        };
        let up = cs4.transposed(iv(4, 7)).unwrap();
        let moved = spelled.transposed(iv(4, 7), semitone(&up)).unwrap();
        assert_eq!(moved.nominal, SpellingNominal::Cmn(CmnNominal::G));
        assert_eq!(moved.octave, 4);
        assert_eq!(moved.accidentals, crate::prepass::accidental_ids(1));

        // B-sharp 3 sounds as C4. Up a fifth it must stay a *letter F*, spelled
        // F-double-sharp 4, sounding G4 — the nominal carries the choice.
        let bs3 = PitchSpelling {
            nominal: SpellingNominal::Cmn(CmnNominal::B),
            accidentals: crate::prepass::accidental_ids(1),
            octave: 3,
            render_hints: SpellingRenderHints::default(),
        };
        let c4 = cmn(CmnNominal::C, 0, 4);
        let g4 = c4.transposed(iv(4, 7)).unwrap();
        let moved = bs3.transposed(iv(4, 7), semitone(&g4)).unwrap();
        assert_eq!(moved.nominal, SpellingNominal::Cmn(CmnNominal::F));
        assert_eq!(moved.octave, 4);
        assert_eq!(moved.accidentals, crate::prepass::accidental_ids(2));

        // A plain sharpen: the staff line never moves.
        let c = PitchSpelling::cmn(CmnNominal::C, 4);
        let sharp = c4.transposed(iv(0, 1)).unwrap();
        let moved = c.transposed(iv(0, 1), semitone(&sharp)).unwrap();
        assert_eq!(moved.nominal, SpellingNominal::Cmn(CmnNominal::C));
        assert_eq!(moved.octave, 4);
        assert_eq!(moved.accidentals, crate::prepass::accidental_ids(1));
    }

    #[test]
    fn a_spelling_refuses_to_move_when_it_cannot_be_written() {
        let c = PitchSpelling::cmn(CmnNominal::C, 4);
        // Octave out of range, and an extreme interval that must not panic.
        assert_eq!(c.transposed(iv(7 * 200, 12 * 200), 0), None);
        assert_eq!(c.transposed(iv(i32::MAX, 0), 0), None);
        assert_eq!(c.transposed(iv(i32::MIN, 0), 0), None);
        // An alteration that will not fit an `i8`.
        assert_eq!(c.transposed(iv(0, 0), 4000), None);
        // A non-CMN nominal has no letter to move.
        let integer = PitchSpelling {
            nominal: SpellingNominal::Integer(7),
            accidentals: Vec::new(),
            octave: 4,
            render_hints: SpellingRenderHints::default(),
        };
        assert_eq!(integer.transposed(iv(4, 7), 55), None);
    }

    #[test]
    fn a_transposition_refuses_a_non_cmn_position() {
        let mut p = cmn(CmnNominal::C, 0, 4);
        p.scale_position.position = PitchSpacePosition::Integer {
            space_size: 31,
            index: 7,
        };
        assert_eq!(
            p.transposed(iv(4, 7)),
            Err(TransposeRefusal::NonCmnPosition)
        );

        p.scale_position.position = PitchSpacePosition::JiVector {
            components: vec![1, 0, -1],
        };
        assert_eq!(
            p.transposed(iv(0, 0)),
            Err(TransposeRefusal::NonCmnPosition)
        );
    }

    #[test]
    fn unresolved_cmn_space_refuses_transposition_and_twelve_tet_conversion() {
        // "edo-31" resolves structurally (the built-in catalog fully
        // determines it — `PositionStructure::Chromatic { 31 }`), but a
        // *resolved* structure in the wrong family is exactly as unusable to
        // a `Cmn` position as an unresolved one: `Cmn` only has a defined
        // action under `DiatonicOverChromatic`.
        let mut p = cmn(CmnNominal::E, -1, 4);
        p.scale_position.space = PitchSpaceId::new("edo-31");

        assert_eq!(
            p.transposed(iv(4, 7)),
            Err(TransposeRefusal::PitchSpaceUnavailable)
        );
        assert_eq!(p.twelve_tet_semitone(), None);
        assert_eq!(p.twelve_tet_class(), None);
    }

    #[test]
    fn transposition_refuses_a_ji_space_an_unknown_identifier_and_an_unresolved_catalog_space() {
        // Three distinct ways `req:pitch:space-capability-refusal` is
        // satisfied without guessing: a real catalog identifier whose family
        // is JI (`ji-5limit`), an identifier the catalog has never heard of,
        // and one of the six built-in identifiers the catalog names but does
        // not structurally determine (`maqam-base`).
        for space in ["ji-5limit", "not-a-built-in-pitch-space", "maqam-base"] {
            let p = cmn_in(space, CmnNominal::C, 0, 4);
            assert_eq!(
                p.transposed(iv(4, 7)),
                Err(TransposeRefusal::PitchSpaceUnavailable),
                "{space} must refuse a transposition rather than guess a pitch-space structure"
            );
        }
    }

    #[test]
    fn twelve_tet_semitone_stays_none_for_a_resolved_non_twelve_chromatic_structure() {
        // `cmn-24` resolves structurally — it is one of the seven fully
        // determined catalog entries — but its chromatic cardinality is 24,
        // not 12. `twelve_tet_semitone`/`twelve_tet_class` must still refuse:
        // resolving *a* structure is not the same as resolving a
        // *twelve*-chromatic one (`spec/CONTRACT_PUSH4B_PITCHSPACES.md` item
        // 4 — the name stays honest only if it keeps this gate).
        let p = cmn_in("cmn-24", CmnNominal::E, -1, 4);
        assert_eq!(p.twelve_tet_semitone(), None);
        assert_eq!(p.twelve_tet_class(), None);
    }

    #[test]
    fn cmn_24_transposes_using_its_own_quarter_tone_structure() {
        // The tranche's acceptance criterion (`spec/CONTRACT_PUSH4B_PITCHSPACES.md`
        // "Proof of life"): a `cmn-24` pitch transposes end-to-end, with the
        // *resulting scale position* asserted — not merely `is_ok()`.

        // First, the quarter-tone alteration unit itself
        // (`core_spec.tex:3606`: "A flat is -2 and a half-flat is -1"), via a
        // same-nominal chromatic-only shift.
        let c4 = cmn_in("cmn-24", CmnNominal::C, 0, 4);
        let half_flat = c4.transposed(iv(0, -1)).unwrap();
        assert_eq!(
            half_flat.scale_position.position,
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: -1,
                octave: 4,
            }
        );
        let flat = c4.transposed(iv(0, -2)).unwrap();
        assert_eq!(
            flat.scale_position.position,
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: -2,
                octave: 4,
            }
        );

        // Second, the case that only a genuinely 24-chromatic computation
        // gets right: B4 up a semitone (2 quarter-tone steps) spelled as a
        // diatonic step lands on a *natural* C5. `cmn-24`'s nominal map is
        // `[0,4,8,10,14,18,22]` (`core_spec.tex:3609`) and its chromatic
        // cardinality is 24 — not "2x the cmn-12 map applied to a 12-space".
        // Arithmetic that silently fell back to `nominal.chromatic()` (the
        // cmn-12 map, `B=11`) and a chromatic cardinality of 12 would instead
        // compute `alteration' = (11+0+48+2) - (0+60) = 1`: a spurious C5♯.
        let b4 = cmn_in("cmn-24", CmnNominal::B, 0, 4);
        let up_a_semitone = b4.transposed(iv(1, 2)).unwrap();
        assert_eq!(
            up_a_semitone.scale_position.position,
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: 0,
                octave: 5,
            }
        );
    }

    #[test]
    fn a_transposition_refuses_a_pitch_pinned_to_a_frequency() {
        // AbsoluteHz overrides the tuning system: moving the scale position
        // would move the notehead and leave the sound where it was.
        let mut p = cmn(CmnNominal::C, 0, 4);
        p.acoustic.realization = AcousticRealization::absolute_hz(261.6).unwrap();
        assert_eq!(
            p.transposed(iv(4, 7)),
            Err(TransposeRefusal::AcousticPinned)
        );

        // A cents offset is relative to whatever the tuning produces for the
        // *new* position, so it transposes and survives.
        let mut q = cmn(CmnNominal::C, 0, 4);
        q.acoustic.realization = AcousticRealization::cents_offset(-13.7).unwrap();
        let moved = q.transposed(iv(4, 7)).unwrap();
        assert_eq!(position(&moved), (CmnNominal::G, 0, 4));
        assert_eq!(moved.acoustic.realization, q.acoustic.realization);
    }

    #[test]
    fn pitch_range_contains_is_advisory_and_frame_aware() {
        let range = PitchRange {
            lowest: cmn(CmnNominal::C, 0, 2),
            highest: cmn(CmnNominal::C, 0, 6),
        };
        // Interior and inclusive endpoints are in range.
        assert_eq!(range.contains(&cmn(CmnNominal::C, 0, 4)), Some(true));
        assert_eq!(range.contains(&cmn(CmnNominal::C, 0, 2)), Some(true));
        assert_eq!(range.contains(&cmn(CmnNominal::C, 0, 6)), Some(true));
        // Below and above are out of range.
        assert_eq!(range.contains(&cmn(CmnNominal::C, 0, 1)), Some(false));
        assert_eq!(range.contains(&cmn(CmnNominal::C, 0, 7)), Some(false));

        // A malformed (reversed) range is *undecidable*, not "everything out of
        // range" — it must not reject every comparable pitch.
        let reversed = PitchRange {
            lowest: cmn(CmnNominal::C, 0, 6),
            highest: cmn(CmnNominal::C, 0, 2),
        };
        assert_eq!(reversed.contains(&cmn(CmnNominal::C, 0, 4)), None);

        // A candidate in a different pitch-space frame is undecidable (absolute
        // semitones across frames are not comparable).
        let other_frame = Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new("cmn-19"),
                position: PitchSpacePosition::Integer {
                    space_size: 12,
                    index: 48,
                },
            },
            acoustic: AcousticPitch {
                tuning: TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        };
        assert_eq!(other_frame.twelve_tet_semitone(), Some(48));
        assert_eq!(range.contains(&other_frame), None);
    }

    #[test]
    fn enharmonic_equivalence_is_twelve_tet_pitch_class() {
        // C-sharp4 and D-flat4 are enharmonic but not structurally equal nor
        // scale-position equivalent.
        let cis = cmn(CmnNominal::C, 1, 4);
        let des = cmn(CmnNominal::D, -1, 4);
        assert_ne!(cis, des);
        assert!(!cis.scale_position_equivalent(&des));
        assert!(cis.enharmonic_equivalent(&des));
        // B-sharp 3 wraps up to C4 (same sounding semitone across the octave
        // boundary): octave-aware arithmetic handles this.
        let bis = cmn(CmnNominal::B, 1, 3);
        let c = cmn(CmnNominal::C, 0, 4);
        assert!(bis.enharmonic_equivalent(&c));
        // A different pitch class is not enharmonic.
        assert!(!cmn(CmnNominal::C, 0, 4).enharmonic_equivalent(&cmn(CmnNominal::D, 0, 4)));
        // Enharmonic equivalence is a *sounding* notion: the same nominal an
        // octave apart is NOT equivalent (it sounds an octave higher).
        assert!(!cmn(CmnNominal::C, 0, 4).enharmonic_equivalent(&cmn(CmnNominal::C, 0, 5)));
        assert!(cmn(CmnNominal::C, 0, 4).enharmonic_equivalent(&cmn(CmnNominal::C, 0, 4)));
    }

    #[test]
    fn structural_equality_is_not_equivalence() {
        let a = cmn(CmnNominal::C, 0, 4);
        let b = cmn(CmnNominal::C, 0, 4);
        assert_eq!(a, b);
        assert!(a.scale_position_equivalent(&b));
    }

    #[test]
    fn enharmonic_requires_the_same_pitch_space() {
        // Both operands are independently computable as the same absolute
        // 12-TET semitone. The rejection therefore proves the space-frame
        // branch rather than passing through conversion unavailability.
        let cis = cmn(CmnNominal::C, 1, 4);
        let other_space = Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new("edo-31"),
                position: PitchSpacePosition::Integer {
                    space_size: 12,
                    index: 49,
                },
            },
            acoustic: AcousticPitch {
                tuning: TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        };
        assert_eq!(cis.twelve_tet_semitone(), Some(49));
        assert_eq!(other_space.twelve_tet_semitone(), Some(49));
        assert!(cis.enharmonic_equivalent(&cmn(CmnNominal::D, -1, 4)));
        assert!(!cis.enharmonic_equivalent(&other_space));
    }

    #[test]
    fn sounding_equivalence_uses_acoustic_cents_tolerance_class() {
        use epiphany_determinism::{Tolerance, ToleranceClass, ToleranceGovernance};
        let cents = |c| {
            Tolerance::absolute(
                ToleranceClass::AcousticCents,
                c,
                ToleranceGovernance::Validation,
            )
            .unwrap()
        };
        let a = cmn(CmnNominal::A, 0, 4);
        let b = cmn(CmnNominal::A, 0, 4);
        // A resolver placing both at 440 Hz -> equivalent; 440 vs 466 (~100c)
        // is not within a 5-cent tolerance.
        assert!(a.sounding_equivalent(&b, cents(5.0), |_| Some(440.0)));
        let resolve = |p: &Pitch| match &p.scale_position.position {
            PitchSpacePosition::Cmn { nominal, .. } if *nominal == CmnNominal::A => Some(440.0),
            _ => Some(466.16),
        };
        assert!(!a.sounding_equivalent(&cmn(CmnNominal::B, -1, 4), cents(5.0), resolve));
        // An AbsoluteHz pitch resolves without the closure.
        let mut abs = cmn(CmnNominal::A, 0, 4);
        abs.acoustic.realization = AcousticRealization::absolute_hz(440.0).unwrap();
        assert!(abs.sounding_equivalent(&b, cents(1.0), |_| Some(440.0)));

        // A tolerance of the wrong class is a category error and never matches,
        // even for identical pitches.
        let wrong = Tolerance::absolute(
            ToleranceClass::LayoutCoordinate,
            5.0,
            ToleranceGovernance::Validation,
        )
        .unwrap();
        assert!(!a.sounding_equivalent(&b, wrong, |_| Some(440.0)));
        // A resolver returning an infinite frequency never spuriously matches
        // (the named tolerance rejects non-finite operands).
        assert!(!a.sounding_equivalent(&b, cents(5.0), |_| Some(f64::INFINITY)));
    }

    #[test]
    fn catalog_ids_normalize_to_nfc() {
        // Precomposed "é" (U+00E9) vs decomposed "e" + combining acute (U+0301)
        // are canonically equivalent and MUST intern equal (Appendix D).
        let precomposed = PitchSpaceId::new("caf\u{00e9}");
        let decomposed = PitchSpaceId::new("cafe\u{0301}");
        assert_eq!(precomposed, decomposed);
        assert_eq!(precomposed.as_str(), decomposed.as_str());
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let h = |x: &PitchSpaceId| {
            let mut s = DefaultHasher::new();
            x.hash(&mut s);
            s.finish()
        };
        assert_eq!(h(&precomposed), h(&decomposed));
    }

    #[test]
    fn system_pitch_derivation_is_deterministic_and_content_addressed() {
        let p = cmn(CmnNominal::C, 0, 4);
        let id = derive_system_pitch_id(&p);
        // Deterministic and in the reserved namespace.
        assert_eq!(id, derive_system_pitch_id(&p));
        assert_eq!(id.replica(), crate::ids::ReplicaId::SYSTEM_DERIVED);
        // Different content derives a different id.
        assert_ne!(id, derive_system_pitch_id(&cmn(CmnNominal::D, 0, 4)));
    }

    #[test]
    fn system_pitch_id_byte_form_is_locked() {
        // Golden: locks the MUSCSPCH canonical-input layout (space name, scale
        // position discriminant + payload, tuning, acoustic realization; strings
        // length-prefixed NFC) and the hash. RATIFIED by Pass 11 (item 1.3,
        // P11-6): this is the spec's golden, normative in core_spec
        // §"System-Derived Pitch Identity",
        // Requirement `req:graph:system-derived-pitch-id` — note the tuning
        // reference (incl. the Inherit marker) is always part of intrinsic
        // identity. A change to the byte form breaks this deliberately.
        let id = derive_system_pitch_id(&cmn(CmnNominal::C, 0, 4));
        assert_eq!(id.replica(), crate::ids::ReplicaId::SYSTEM_DERIVED);
        const GOLDEN: [u8; 16] = [
            255, 255, 255, 255, 255, 255, 255, 255, 164, 31, 138, 24, 68, 38, 241, 168,
        ];
        assert_eq!(id.canonical_bytes(), GOLDEN);
    }

    #[test]
    fn reference_pitch_rejects_non_positive_frequency() {
        let pos = PitchSpacePosition::Cmn {
            nominal: CmnNominal::A,
            alteration: 0,
            octave: 4,
        };
        assert!(ReferencePitch::new(pos.clone(), -440.0).is_none());
        assert!(ReferencePitch::new(pos.clone(), 0.0).is_none());
        assert_eq!(
            ReferencePitch::new(pos, 440.0).unwrap().frequency_hz(),
            440.0
        );
        assert_eq!(ReferencePitch::a440().frequency_hz(), 440.0);
    }

    #[test]
    fn acoustic_realization_rejects_bad_floats() {
        assert!(AcousticRealization::cents_offset(f64::NAN).is_none());
        assert!(AcousticRealization::absolute_hz(0.0).is_none());
        assert!(AcousticRealization::absolute_hz(-440.0).is_none());
        assert!(AcousticRealization::absolute_hz(440.0).is_some());
    }

    #[test]
    fn accidental_stack_rejects_repeats() {
        let mut s = PitchSpelling::cmn(CmnNominal::F, 4);
        s.accidentals = vec![AccidentalId::new("sharp")];
        assert!(s.accidental_stack_is_well_formed());
        s.accidentals = vec![AccidentalId::new("sharp"), AccidentalId::new("sharp")];
        assert!(!s.accidental_stack_is_well_formed());
    }

    #[test]
    fn spelling_precedence_default_and_totality() {
        let p = SpellingPrecedence::default();
        assert!(p.rank(SpellingSourceKind::UserChosen) < p.rank(SpellingSourceKind::Inferred));
        assert!(p.rank(SpellingSourceKind::Imported) < p.rank(SpellingSourceKind::Propagated));
        // Non-total configurations are rejected.
        assert!(SpellingPrecedence::new(vec![SpellingSourceKind::UserChosen]).is_none());
        assert!(SpellingPrecedence::new(vec![
            SpellingSourceKind::UserChosen,
            SpellingSourceKind::UserChosen,
            SpellingSourceKind::Imported,
            SpellingSourceKind::Propagated,
            SpellingSourceKind::Inferred,
        ])
        .is_none());
    }

    #[test]
    fn explicit_spelling_requires_pitch_scope() {
        let pid = PitchId::new(crate::ReplicaId(1), 0);
        let ok = SpellingAttachment {
            scope: SpellingScope::Pitch(pid),
            directive: SpellingDirective::Explicit(PitchSpelling::cmn(CmnNominal::C, 4)),
            source: SpellingSource::UserChosen,
            priority: 0,
            layer: None,
        };
        assert!(ok.is_well_formed());
    }

    #[test]
    fn spell_preserves_authored_cmn_letter() {
        // An authored C-sharp keeps its letter (spelling follows the scale
        // position, not the trivial old C-default stub).
        let p = Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new("cmn-12"),
                position: PitchSpacePosition::Cmn {
                    nominal: CmnNominal::C,
                    alteration: 1,
                    octave: 5,
                },
            },
            acoustic: AcousticPitch {
                tuning: TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        };
        let s = spell(&p, &SpellingContext::default());
        assert_eq!(s.nominal, SpellingNominal::Cmn(CmnNominal::C));
        assert_eq!(s.accidentals, vec![AccidentalId::new("sharp")]);
        assert_eq!(s.octave, 5);
        assert_eq!(SpellingAlgorithmId::default_id().as_str(), "default");
    }

    #[test]
    fn spell_chromatic_integer_pitch_is_nontrivial() {
        // A 12-EDO integer position (chromatic input) gets a real spelling of
        // its pitch class, not the old constant middle-C.
        let p = Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new("cmn-12"),
                position: PitchSpacePosition::Integer {
                    space_size: 12,
                    index: 54, // pitch class 6 (F#/Gb)
                },
            },
            acoustic: AcousticPitch {
                tuning: TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        };
        let s = spell(&p, &SpellingContext::default());
        // Simplest single-accidental spelling of pitch class 6 (either F# or Gb);
        // it is a real, non-default spelling.
        assert!(matches!(
            s.nominal,
            SpellingNominal::Cmn(CmnNominal::F) | SpellingNominal::Cmn(CmnNominal::G)
        ));
        assert_eq!(s.accidentals.len(), 1);
    }
}
