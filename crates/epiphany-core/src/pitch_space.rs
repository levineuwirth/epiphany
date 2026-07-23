//! Chapter 4 pitch-space vocabulary (`core_spec.tex` §"Pitch Spaces",
//! `sec:tuning:space`, `:2850` onward): the shape of a pitch space's position
//! structure, plus the built-in catalog Push 4b tranche 1 resolves against.
//!
//! **Scope**, per `spec/CONTRACT_PUSH4B_PITCHSPACES.md`: only the types this
//! tranche's one consumer needs — [`Pitch::transposed`](crate::pitch::Pitch::transposed)
//! and [`Pitch::twelve_tet_semitone`](crate::pitch::Pitch::twelve_tet_semitone)'s
//! structural pitch-space resolution, which replaces the P13-S2 interim
//! `"cmn-12"` identifier guard — plus [`PitchSpace`], the type the
//! specification itself carries [`PositionStructure`] in. `TuningSystem`,
//! `TuningResolution`, the accidental/nominal registry *bodies* (their ids
//! already exist in [`crate::pitch`]), and Chapter 4's glyph and engraving
//! vocabulary are out of scope: they belong to the tranche that adds the
//! codec, and landing them here would create exactly the unconsumed type
//! surface `spec/CONTRACT_PUSH4B_PITCHSPACES.md` exists to avoid.
//!
//! **No `Codec` impl exists, or may be added, for anything in this module**
//! (Ruling C, `spec/PLAN_PUSH4B_TUNING.md`): these types stay in memory —
//! referenced only by id from canonical score state — so they remain free to
//! change once a later tranche discovers they are wrong. Adding one, or a
//! field to `Score`/`ScoreTuningContext`, would freeze this surface onto the
//! wire before it has ever had more than one consumer.
//!
//! ## The six built-in pitch spaces this module does not resolve
//!
//! `core_spec.tex:3598-3646` normatively names thirteen built-in pitch
//! spaces. Seven are fully determined by the table and are transcribed
//! below. **Six are not**, and [`built_in_position_structure`] returns
//! `None` for each rather than inventing a [`PositionStructure`] for it —
//! see that function's match arms for what the specification does and does
//! not fix for each one. A structure inferred and written down as though the
//! specification stated it is the `NOTEHEAD_ANCHORS` failure this project has
//! already paid for twice.

use core::num::NonZeroU32;

use crate::pitch::{
    AccidentalRegistryId, IntervalAlgebraRegistryId, NominalRegistryId, PitchSpaceId,
    PositionStructureRegistryId, SpellingAlgorithmId, SpellingRuleSetId, TranspositionRegistryId,
};

/// The shape of a pitch space's positions (Chapter 4 §"Position Structure",
/// `core_spec.tex:2896`, `sec:tuning:positions`). Three families cover the
/// vast majority of useful cases; a fourth is the escape hatch for grammars
/// defined entirely by plugin.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum PositionStructure {
    /// Chromatic: a fixed number of equally-numbered positions per octave,
    /// with no hierarchical diatonic substructure. Includes EDOs and serial
    /// 12-tone.
    Chromatic { positions_per_octave: u16 },

    /// Diatonic over chromatic: a smaller set of named nominals (e.g. A-G
    /// for CMN) plus accidental modifications reaching the full chromatic
    /// range. CMN's structure.
    ///
    /// Never construct this variant's literal directly outside this module;
    /// use [`PositionStructure::diatonic_over_chromatic`], the checked
    /// constructor that enforces every clause of
    /// `req:tuning:diatonic-chromatic-mapping`.
    DiatonicOverChromatic {
        nominals_per_octave: u16,
        chromatic_positions_per_octave: u16,
        /// Mapping from each nominal to its position in the chromatic layer
        /// (e.g. C=0, D=2, E=4, F=5, G=7, A=9, B=11 for CMN).
        nominal_to_chromatic: Vec<u16>,
    },

    /// Just intonation lattice: positions defined by exact rational ratios
    /// from a tonic, organized along prime-axis dimensions (3-limit,
    /// 5-limit, 7-limit, etc.).
    JiLattice {
        /// Prime limit.
        limit: u8,
        /// One generator ratio per prime dimension.
        generators: Vec<JiRatio>,
    },

    /// Grammar-defined: structure is opaque to the core and resolved by a
    /// grammar plugin. Used for maqam, gamelan, raga frameworks, and other
    /// systems whose position structure is not flat or hierarchical in the
    /// above senses.
    Registered(PositionStructureRegistryId),
}

impl PositionStructure {
    /// Builds a [`PositionStructure::DiatonicOverChromatic`], enforcing all
    /// three clauses of `req:tuning:diatonic-chromatic-mapping`:
    /// `nominal_to_chromatic` MUST have length `nominals_per_octave`, each
    /// entry MUST be strictly less than `chromatic_positions_per_octave`,
    /// and the mapping MUST be strictly increasing. Returns `None` if any
    /// clause is violated, so a malformed mapping is never representable
    /// (the `KeySignature::new`/`TupletRatio::new` "reject at construction"
    /// pattern, `epiphany-core/DECISIONS.md` §"Enforced-at-construction
    /// invariants").
    pub fn diatonic_over_chromatic(
        nominals_per_octave: u16,
        chromatic_positions_per_octave: u16,
        nominal_to_chromatic: Vec<u16>,
    ) -> Option<Self> {
        if nominal_to_chromatic.len() != nominals_per_octave as usize {
            return None;
        }
        if !nominal_to_chromatic
            .iter()
            .all(|&entry| entry < chromatic_positions_per_octave)
        {
            return None;
        }
        if !nominal_to_chromatic
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        {
            return None;
        }
        Some(PositionStructure::DiatonicOverChromatic {
            nominals_per_octave,
            chromatic_positions_per_octave,
            nominal_to_chromatic,
        })
    }
}

/// An exact just-intonation ratio: one generator or modification offset
/// within a [`PositionStructure::JiLattice`] (Chapter 4 §"Position
/// Structure", `core_spec.tex:3108`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct JiRatio {
    pub numerator: i32,
    pub denominator: NonZeroU32,
}

/// How distances between positions are computed and transposition is
/// realized (Chapter 4 §"Interval Algebra", `core_spec.tex:2949`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum IntervalAlgebra {
    /// Single-axis integer arithmetic. Two positions are subtracted to
    /// yield a single integer interval.
    Chromatic,
    /// Two-axis arithmetic distinguishing diatonic and chromatic
    /// components. CMN's algebra: a major third is (3 diatonic steps, 4
    /// chromatic steps); a diminished fourth is (3 diatonic steps, 4
    /// chromatic steps) — distinguishable from the major third only by
    /// spelling intent.
    DiatonicChromatic,
    /// Multi-axis JI lattice arithmetic.
    JiVector { dimensions: u8 },
    /// Grammar-defined algebra.
    Registered(IntervalAlgebraRegistryId),
}

/// How scale positions move under interval operations and how spellings
/// follow (Chapter 4 §"Transposition Behavior", `core_spec.tex:3004`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TranspositionBehavior {
    /// Diatonic transposition: shift by N diatonic steps within a chosen
    /// key. Accidentals adjust to fit the destination key.
    Diatonic,
    /// Chromatic transposition: shift by N chromatic steps. Spellings
    /// follow rules of the [`SpellingRuleSet`].
    Chromatic,
    /// Compound: support both, parameterized at the operation site.
    Compound,
    /// Grammar-defined.
    Registered(TranspositionRegistryId),
}

/// Placeholder for the spelling-algorithm parameter schema (Chapter 4
/// §"Spelling Rule Sets", `core_spec.tex:3029`). The specification leaves
/// this type's shape an open question: "the catalog of *additional*
/// registered spelling algorithms ... and their parameter schemas, which are
/// normative once registered" (`core_spec.tex:3043`). The only
/// currently-registered algorithm, [`SpellingAlgorithmId::default_id`]
/// (`req:pitch:spelling-algorithm`), is a fixed rule with no declared
/// parameters of its own. This tranche does not invent a shape for the open
/// question — the same discipline `built_in_position_structure` applies to
/// the six underdetermined pitch spaces, applied to a type rather than a
/// data row: this marker exists only so [`SpellingRuleSet`]'s field list
/// matches the specification's listing, and carries no state.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct SpellingParameters;

/// A pitch space's default spelling rule set, consulted by the spelling
/// pre-pass when that pitch space is active (Chapter 4 §"Spelling Rule
/// Sets", `core_spec.tex:3028`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SpellingRuleSet {
    pub id: SpellingRuleSetId,
    pub name: String,
    /// Algorithmic family this rule set belongs to. The specific algorithm
    /// is referenced by id and resolved against the implementation's
    /// registered spelling algorithms.
    pub algorithm: SpellingAlgorithmId,
    /// Algorithm-specific parameters.
    pub parameters: SpellingParameters,
}

/// A pitch space: the analytical universe in which scale-degree and interval
/// relationships are defined (Chapter 4 §"Pitch Spaces", `core_spec.tex:2857`,
/// `sec:tuning:space`). It is the algebra; the tuning system is the physics.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PitchSpace {
    pub id: PitchSpaceId,
    /// Human-readable name (e.g., "CMN 12-tone chromatic").
    pub name: String,
    /// Optional description and provenance notes.
    pub description: Option<String>,
    /// What positions exist and how they are structured.
    pub positions: PositionStructure,
    /// How intervals between positions are defined.
    pub interval_algebra: IntervalAlgebra,
    /// The accidental registry valid in this pitch space.
    pub accidental_registry: AccidentalRegistryId,
    /// The nominal registry: named position-letters (e.g., A-G for CMN).
    pub nominal_registry: NominalRegistryId,
    /// Rules governing how pitches transpose within this space.
    pub transposition: TranspositionBehavior,
    /// Default rules for the spelling pre-pass when this pitch space is
    /// active.
    pub spelling_rules: SpellingRuleSet,
}

/// Looks up the [`PositionStructure`] of a built-in pitch space (Chapter 4
/// §"Built-in Catalog", `core_spec.tex:3598-3646`), the structural
/// replacement for the retired P13-S2 interim `"cmn-12"` identifier check
/// (`req:pitch:space-capability-refusal`).
///
/// `None` covers two cases alike, and deliberately does not distinguish
/// them: an identifier outside the thirteen built-in pitch spaces, and one
/// of the **six built-in spaces the specification names but does not
/// structurally determine**. Both must fail closed identically at every
/// consumer (`req:pitch:space-capability-refusal`: "The `Cmn` position
/// discriminant alone MUST NOT be treated as proof of ... capability" —
/// naming a real catalog entry is not proof of its structure either), so one
/// `Option` return is enough; the per-arm comments below record *why* each
/// of the six is `None`, for anyone auditing which gap is which.
pub fn built_in_position_structure(id: &PitchSpaceId) -> Option<PositionStructure> {
    match id.as_str() {
        // -- Fully determined by the table (`core_spec.tex:3598-3646`); --
        // -- transcription, not decision.                               --
        "cmn-12" => PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 5, 7, 9, 11]),
        "cmn-24" => {
            PositionStructure::diatonic_over_chromatic(7, 24, vec![0, 4, 8, 10, 14, 18, 22])
        }
        "edo-19" => Some(PositionStructure::Chromatic {
            positions_per_octave: 19,
        }),
        "edo-22" => Some(PositionStructure::Chromatic {
            positions_per_octave: 22,
        }),
        "edo-31" => Some(PositionStructure::Chromatic {
            positions_per_octave: 31,
        }),
        "edo-53" => Some(PositionStructure::Chromatic {
            positions_per_octave: 53,
        }),
        "edo-72" => Some(PositionStructure::Chromatic {
            positions_per_octave: 72,
        }),

        // -- Named by the catalog; not structurally determined by it.   --
        //
        // `ji-5limit` / `ji-7limit` / `ji-11limit`: the table fixes `limit`
        // (5, 7, 11) and the ascending-from-2 prime basis ({2,3,5},
        // {2,3,5,7}, {2,3,5,7,11} — `req:pitch:ji-vector-basis`), so the
        // *dimension count* of `JiLattice.generators` is known (3, 4, 5).
        // What the table never states is the generator *ratios themselves*:
        // `JiLattice.generators: Vec<JiRatio>` (`core_spec.tex:2920`) needs
        // one exact rational per prime dimension, and the specification
        // gives the axes, not the ratios that generate them (unison?
        // 3-limit uses `3/2`, but nothing pins that choice normatively for
        // this catalog, and 5-, 7-, and 11-limit have no stated generator
        // set at all). Choosing rationals here — even "the obvious" ones —
        // would be exactly the `NOTEHEAD_ANCHORS` failure this contract
        // exists to avoid: hand-written data that looks authoritative and
        // is wrong. These three refuse rather than guess.
        "ji-5limit" | "ji-7limit" | "ji-11limit" => None,

        // `maqam-base`, `gamelan-slendro`, `gamelan-pelog`: described
        // entirely in prose — "skeletal maqam framework with quarter-flat
        // and quarter-sharp ... accidentals", "five-tone slendro
        // framework", "seven-tone pelog framework"
        // (`core_spec.tex:3630-3635`). None of the three prose
        // descriptions fixes a nominal count, a chromatic cardinality, or
        // even which `PositionStructure` family applies.
        // `PositionStructure::Registered` is the family the specification
        // itself names for exactly this case ("used for maqam, gamelan,
        // raga frameworks ... whose position structure is not flat or
        // hierarchical", `core_spec.tex:2923-2926`), but committing any of
        // these three to a specific `PositionStructureRegistryId` would
        // invent the grammar plugin the prose explicitly defers to grammar
        // plugins ("deep coverage of specific maqamat is the province of
        // grammar plugins", `core_spec.tex:3633`). `maqam-base`'s
        // quarter-tone accidentals additionally presuppose a chromatic
        // cardinality (`req:pitch:alteration-unit` denominates them in
        // "the space's chromatic layer") that nothing in the table states.
        // These three refuse rather than guess.
        "maqam-base" | "gamelan-slendro" | "gamelan-pelog" => None,

        // Not one of the thirteen built-in identifiers.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diatonic_over_chromatic_rejects_a_mapping_of_the_wrong_length() {
        // Clause 1 of `req:tuning:diatonic-chromatic-mapping`: length MUST
        // equal `nominals_per_octave`. Six entries for seven nominals.
        assert!(
            PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 5, 7, 9]).is_none()
        );
        // Eight entries for seven nominals.
        assert!(
            PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 5, 7, 9, 11, 11])
                .is_none()
        );
    }

    #[test]
    fn diatonic_over_chromatic_rejects_an_entry_not_strictly_below_the_chromatic_card() {
        // Clause 2: every entry MUST be strictly less than
        // `chromatic_positions_per_octave`. `12` is not `< 12`.
        assert!(
            PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 5, 7, 9, 12]).is_none()
        );
        // Comfortably out of range too.
        assert!(
            PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 5, 7, 9, 99]).is_none()
        );
    }

    #[test]
    fn diatonic_over_chromatic_rejects_a_non_increasing_mapping() {
        // Clause 3: the mapping MUST be strictly increasing. A repeat...
        assert!(
            PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 4, 7, 9, 11]).is_none()
        );
        // ...and a descent, both rejected.
        assert!(
            PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 3, 7, 9, 11]).is_none()
        );
    }

    #[test]
    fn diatonic_over_chromatic_accepts_both_built_in_mappings() {
        // The contract's own warning: both built-ins satisfy all three
        // clauses, so this alone cannot substitute for the rejection tests
        // above — a constructor enforcing only two of the three clauses
        // still passes this.
        let cmn12 =
            PositionStructure::diatonic_over_chromatic(7, 12, vec![0, 2, 4, 5, 7, 9, 11]).unwrap();
        assert_eq!(
            cmn12,
            PositionStructure::DiatonicOverChromatic {
                nominals_per_octave: 7,
                chromatic_positions_per_octave: 12,
                nominal_to_chromatic: vec![0, 2, 4, 5, 7, 9, 11],
            }
        );
        let cmn24 =
            PositionStructure::diatonic_over_chromatic(7, 24, vec![0, 4, 8, 10, 14, 18, 22])
                .unwrap();
        assert_eq!(
            cmn24,
            PositionStructure::DiatonicOverChromatic {
                nominals_per_octave: 7,
                chromatic_positions_per_octave: 24,
                nominal_to_chromatic: vec![0, 4, 8, 10, 14, 18, 22],
            }
        );
    }

    #[test]
    fn built_in_position_structure_resolves_the_seven_determined_spaces() {
        let cmn12 = built_in_position_structure(&PitchSpaceId::new("cmn-12")).unwrap();
        assert_eq!(
            cmn12,
            PositionStructure::DiatonicOverChromatic {
                nominals_per_octave: 7,
                chromatic_positions_per_octave: 12,
                nominal_to_chromatic: vec![0, 2, 4, 5, 7, 9, 11],
            }
        );
        let cmn24 = built_in_position_structure(&PitchSpaceId::new("cmn-24")).unwrap();
        assert_eq!(
            cmn24,
            PositionStructure::DiatonicOverChromatic {
                nominals_per_octave: 7,
                chromatic_positions_per_octave: 24,
                nominal_to_chromatic: vec![0, 4, 8, 10, 14, 18, 22],
            }
        );
        for (id, expected) in [
            ("edo-19", 19u16),
            ("edo-22", 22),
            ("edo-31", 31),
            ("edo-53", 53),
            ("edo-72", 72),
        ] {
            assert_eq!(
                built_in_position_structure(&PitchSpaceId::new(id)),
                Some(PositionStructure::Chromatic {
                    positions_per_octave: expected
                }),
                "{id} did not resolve to its EDO cardinality"
            );
        }
    }

    #[test]
    fn built_in_position_structure_refuses_the_six_underdetermined_spaces() {
        for id in [
            "ji-5limit",
            "ji-7limit",
            "ji-11limit",
            "maqam-base",
            "gamelan-slendro",
            "gamelan-pelog",
        ] {
            assert_eq!(
                built_in_position_structure(&PitchSpaceId::new(id)),
                None,
                "{id} is named by the catalog but its structure is not spec-determined; \
                 it must not resolve"
            );
        }
    }

    #[test]
    fn built_in_position_structure_refuses_an_identifier_outside_the_catalog() {
        assert_eq!(
            built_in_position_structure(&PitchSpaceId::new("not-a-built-in-space")),
            None
        );
    }
}
