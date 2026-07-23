//! Chapter 4 accidental, glyph, and engraving vocabulary (`core_spec.tex`
//! §"Accidental Registries", `sec:tuning:accidentals`, `:3054`-`:3234`; §"Glyph
//! References and SMuFL", `sec:tuning:smufl`, `:3235`-`:3277`).
//!
//! **Scope**, per `spec/CONTRACT_PUSH4B_ACCIDENTALS.md` (Push 4b tranche 3a):
//! the type surface, in memory, with two real consumers —
//! [`resolve_accidental`] (accidental resolution, honoring the precedence
//! "an override wins over an addition wins over the base registry") and
//! [`accidental_modification_compatible_with_space`]
//! (`req:tuning:accidental-modification-compatibility`, wired into
//! [`crate::invariants::check_invariants`]). Glyph and engraving metadata
//! ([`GlyphReference`], [`AccidentalEngraving`], [`AccidentalCombination`]) are
//! *carried* by both consumers — resolution returns them as part of the
//! resolved [`AccidentalDefinition`], and the compatibility check reads past
//! them to `modification` — but neither consumer *reads* them for their own
//! sake. Their deep consumer is the engraver, out of `epiphany-core`, a later
//! tranche; this module does not fabricate one to manufacture coverage.
//!
//! **No `Codec` impl exists, or may be added, for anything in this module**
//! (same discipline as `pitch_space.rs` and `tuning.rs`, Ruling C,
//! `spec/PLAN_PUSH4B_TUNING.md`): these types stay in memory this tranche.
//! [`crate::graph::ScoreTuningContext::accidental_extensions`] and `::smufl`
//! reference them without putting them on the wire (schema major 3, Push 4b
//! tranche 3b).
//!
//! ## Three ratified corrections (P13-S10/S11/S12, ratified 2026-07-23)
//!
//! * **S10** — [`PitchSpaceModification::Cents`] carries a
//!   [`CanonicalF64`], not a raw `f64`: a raw `f64` is unencodable in
//!   canonical state (`serialize.rs:110` decodes floats only through
//!   `CanonicalF64::from_le_bytes`; there is no `Codec for f64`).
//! * **S11** — [`AnchorPoint`] is referenced by the specification (`:3166`)
//!   but defined nowhere in it, and this crate cannot depend on
//!   `epiphany-layout-ir`. It is core-native here, over [`SpaceUnit`], with a
//!   pinned frame — see its doc comment.
//! * **S12** — [`SmuflVersion`] stores its minor fraction-normalized to
//!   hundredths (`minor_centi`), not literally, so derived `Ord` agrees with
//!   SMuFL's real release order. **This is not**
//!   `epiphany_layout_ir`'s existing, differently-shaped `SmuflVersion`
//!   (`glyph.rs:29`, literal-minor) — see this type's doc comment for why the
//!   two are a deliberate, bounded homonym until Push 4b tranche 3b unifies
//!   them.

use core::num::NonZeroU32;

use epiphany_determinism::CanonicalF64;

use crate::graph::SpaceUnit;
use crate::pitch::{
    AccidentalGroupId, AccidentalId, AccidentalRegistryId, CustomGlyphId, ModificationRegistryId,
    PitchSpaceId,
};
use crate::pitch_space::{built_in_position_structure, PositionStructure};

// ===========================================================================
// Glyph references (Chapter 4 §"Glyph References and SMuFL", `:3244`).
// ===========================================================================

/// A glyph reference (Chapter 4 §"Glyph References and SMuFL", `:3244`).
/// Recursive, and **this chapter's own** — do not confuse with
/// `epiphany_layout_ir::GlyphReference` (`glyph.rs:50`), a glyph *name*
/// (`Cow<'static, str>`), a rendering/layout concern. Same name, unrelated
/// types (Push 4b Ruling D's correction: "they are homonyms, not shared
/// types"); `epiphany-core` cannot depend on `epiphany-layout-ir` in any
/// case, so within this crate there is no ambiguity.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum GlyphReference {
    /// SMuFL codepoint, resolved against the active SMuFL font.
    Smufl(u32),
    /// Custom glyph defined by the score or a plugin.
    Custom(CustomGlyphId),
    /// Composite glyph: multiple glyph references rendered as a single
    /// accidental. Used for compound HEJI symbols and similar.
    Composite(Vec<GlyphReference>),
}

// ===========================================================================
// Pitch space modifications (Chapter 4 §"Pitch Space Modifications", `:3097`).
// ===========================================================================

/// A position modification an accidental applies (Chapter 4 §"Pitch Space
/// Modifications", `:3097`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum PitchSpaceModification {
    /// CMN-style integer offset in chromatic steps of the enclosing pitch
    /// space (`req:pitch:alteration-unit`).
    CmnChromatic(i8),
    /// Integer step offset in an EDO position space.
    EdoSteps(i16),
    /// Modification expressed as an exact rational ratio. Used for JI
    /// accidentals such as the HEJI syntonic-comma symbols (81/80) and
    /// septimal-comma symbols (64/63).
    JiRatio {
        numerator: i32,
        denominator: NonZeroU32,
    },
    /// Modification expressed in cents. Used for Sagittal-style precise
    /// microtonal accidentals.
    ///
    /// **S10 correction**: `CanonicalF64`, not a raw `f64`, per
    /// `req:determinism:canonical-floating-point` ("finite IEEE 754
    /// binary64") — a raw `f64` cannot be canonical state at all, since there
    /// is no `Codec for f64` and the byte layer decodes floats only through
    /// `CanonicalF64::from_le_bytes`.
    Cents(CanonicalF64),
    /// Modification defined by a grammar plugin.
    Registered(ModificationRegistryId),
}

// ===========================================================================
// Accidental engraving metadata (Chapter 4 §"Accidental Engraving Metadata",
// `:3150`, `:3159`).
// ===========================================================================

/// A bounding box over canonical space units, used for engraving metadata
/// that lives in canonical score state (Chapter 4 §"Accidental Engraving
/// Metadata", `:3150`). Distinct from Chapter 7's `BoundingBox` (built on
/// `StaffSpace`, single precision, for the non-canonical resolved-layout
/// cache, `epiphany_layout_ir::spatial::BoundingBox`): this type's edges are
/// [`SpaceUnit`] (`CanonicalF64`), per
/// `req:determinism:canonical-floating-point`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct EngravingBoundingBox {
    pub left: SpaceUnit,
    pub right: SpaceUnit,
    pub top: SpaceUnit,
    pub bottom: SpaceUnit,
}

/// Where a glyph attaches to the note: typically the geometric center or a
/// custom anchor for compound glyphs (Chapter 4 §"Accidental Engraving
/// Metadata", `:3166`).
///
/// **S11 correction**: the specification *references* this type (`:3166`,
/// `AccidentalEngraving::anchor`) but never defines it, and `epiphany-core`
/// cannot depend on `epiphany-layout-ir`. Defined core-native here, over
/// [`SpaceUnit`].
///
/// **Frame** (ratified alongside S11 — `EngravingBoundingBox` is "relative to
/// the glyph's anchor point" (`:3160`), so the anchor itself needs an
/// unambiguous origin): `x`/`y` are in **canonical space units**, **y-up**,
/// relative to **the glyph's coordinate origin**.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AnchorPoint {
    pub x: SpaceUnit,
    pub y: SpaceUnit,
}

/// Engraving metadata for an accidental (Chapter 4 §"Accidental Engraving
/// Metadata", `:3159`). Reachable from canonical score state — via
/// [`AccidentalDefinition`] inside [`ScoreAccidentalExtensions`], which hangs
/// off [`crate::graph::ScoreTuningContext`] — so every field is
/// canonical-safe: [`EngravingBoundingBox`], [`AnchorPoint`], and
/// `advance_width` are all [`SpaceUnit`] (`CanonicalF64`)-based, never
/// Chapter 7's single-precision `StaffSpace`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AccidentalEngraving {
    /// Bounding box in canonical space units, relative to the glyph's anchor
    /// point.
    pub bounding_box: EngravingBoundingBox,
    /// Where the glyph attaches to the note.
    pub anchor: AnchorPoint,
    /// Advance width for horizontal spacing computations.
    pub advance_width: SpaceUnit,
    /// Stacking order when multiple accidentals attach to one note. Lower
    /// values are placed closer to the notehead.
    pub stacking_order: i32,
    /// Whether this glyph should be drawn with parentheses by default (e.g.,
    /// editorial or cautionary accidentals).
    pub default_parenthesized: bool,
}

// ===========================================================================
// Combination behavior (Chapter 4 §"Combination Behavior", `:3210`).
// ===========================================================================

/// Combination behavior with other accidentals on the same note (Chapter 4
/// §"Combination Behavior", `:3210`). Most accidentals do not combine; some
/// systems (HEJI, certain microtonal notations) permit stacking multiple
/// glyphs to express compound modifications.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum AccidentalCombination {
    /// Stands alone; replaces any prior accidental on the same note.
    Solitary,
    /// May stack with members of the listed compatibility groups. Stacking
    /// order is determined by the engraving metadata.
    Stacking {
        compatible_groups: Vec<AccidentalGroupId>,
    },
}

// ===========================================================================
// Accidental definitions and score-local extensions (Chapter 4
// §"Accidental Registries" / "Score-Local Extensions", `:3073`, `:3228`).
// ===========================================================================

/// An accidental: a glyph, a position modification, and engraving metadata
/// (Chapter 4 §"Accidental Registries", `:3073`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AccidentalDefinition {
    pub id: AccidentalId,
    /// Canonical name. Used for serialization, accessibility, and
    /// foreign-format export.
    pub name: String,
    /// Glyph reference, typically SMuFL.
    pub glyph: GlyphReference,
    /// The position modification this accidental applies.
    pub modification: PitchSpaceModification,
    /// Engraving metadata.
    pub engraving: AccidentalEngraving,
    /// Combination behavior with other accidentals.
    pub combination: AccidentalCombination,
}

/// A score-local extension of a referenced accidental registry (Chapter 4
/// §"Score-Local Extensions", `:3228`). A score MAY extend a referenced
/// accidental registry with additional accidental definitions, provided the
/// registry's `extensible` flag is true (out of this tranche's scope: the
/// registry *body*, `extensible` flag included, is not built here — see
/// [`resolve_accidental`]'s doc comment). "Extensions are stored on the score
/// and override or augment the base registry during resolution" (`:3224`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScoreAccidentalExtensions {
    pub base: AccidentalRegistryId,
    pub additions: Vec<AccidentalDefinition>,
    pub overrides: Vec<AccidentalDefinition>,
}

// ===========================================================================
// SMuFL versioning (Chapter 4 §"SMuFL Versioning", `:3260`-`:3277`).
// ===========================================================================

/// A SMuFL version, stored so derived `Ord` agrees with SMuFL's real release
/// history (Chapter 4 §"SMuFL Versioning").
///
/// **S12 correction.** SMuFL versions are decimal fractions — 1.12, 1.18,
/// 1.20, 1.3, 1.4, released in that order — so storing the minor *literally*
/// and deriving `Ord` over `(major, minor)` sorts wrong: 1.3 as `(1, 3)` would
/// sort *before* 1.12 as `(1, 12)`, even though 1.12 shipped first.
/// `minor_centi` instead stores the fractional part normalized to hundredths
/// — 1.12 -> `12`, 1.18 -> `18`, 1.20 -> `20`, 1.3 -> `30`, 1.4 -> `40` — so
/// derived `Ord` on `(major, minor_centi)` is correct.
///
/// Construct through [`SmuflVersion::from_decimal`] rather than building the
/// literal fields directly, so a caller cannot accidentally pass a literal
/// minor digit where a normalized one is required (`from_decimal(1, "3")` and
/// a mistaken direct `SmuflVersion { major: 1, minor_centi: 3 }` would
/// otherwise look interchangeable and are not: the former is 1.3, the latter
/// is nonsensical).
///
/// **This is not** `epiphany_layout_ir::SmuflVersion` (`glyph.rs:29`,
/// `{ major: u16, minor: u16 }`, literal-minor, load-bearing for
/// `GlyphCatalogIdentity`). That type is Chapter 7's own and stays untouched:
/// unifying the two, and moving `GlyphCatalogIdentity` onto the normalized
/// shape, is Push 4b tranche 3b's job, done deliberately with golden regen.
/// `epiphany-core` cannot depend on `epiphany-layout-ir` in any case, so
/// within this crate there is no ambiguity — the two are a deliberate,
/// bounded homonym until then.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SmuflVersion {
    pub major: u16,
    /// The fractional part, normalized to hundredths. **Do not construct
    /// this field directly with a literal minor digit** — use
    /// [`SmuflVersion::from_decimal`].
    pub minor_centi: u16,
}

impl SmuflVersion {
    /// Builds a `SmuflVersion` from its conventional decimal notation,
    /// normalizing the minor digits to hundredths: one digit is scaled by 10
    /// (`from_decimal(1, "4")` -> 1.4 -> `minor_centi: 40`), two digits are
    /// taken as-is (`from_decimal(1, "12")` -> 1.12 -> `minor_centi: 12`).
    ///
    /// `minor_digits` is the literal digit string that would follow the
    /// decimal point (so `"4"` for 1.4, `"12"` for 1.12, `"3"` for 1.3) —
    /// *not* a numeric value to be stored as-is; `from_decimal(1, "3")` and
    /// `from_decimal(1, "30")` both denote 1.3 and produce the same
    /// `minor_centi: 30`. Returns `None` if `minor_digits` is empty, longer
    /// than two characters, or contains anything but ASCII digits — SMuFL
    /// versions in the wild (1.12, 1.18, 1.20, 1.3, 1.4, ...) never need a
    /// third fractional digit.
    pub fn from_decimal(major: u16, minor_digits: &str) -> Option<Self> {
        if !(1..=2).contains(&minor_digits.len())
            || !minor_digits.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let value: u16 = minor_digits.parse().ok()?;
        let minor_centi = if minor_digits.len() == 1 {
            value * 10
        } else {
            value
        };
        Some(SmuflVersion { major, minor_centi })
    }
}

/// A score's declared SMuFL version requirements (Chapter 4 §"SMuFL
/// Versioning", `:3269`).
///
/// `req:tuning:smufl-version-fallback`: "Every score MUST declare the SMuFL
/// version it targets" — this struct is that declaration. The requirement's
/// other clause (resolving an absent codepoint MUST produce a deterministic
/// fallback, never a silent failure) is a rendering-time behavior with no
/// consumer in `epiphany-core`; implementing it is the engraver's job, out of
/// this crate, a later tranche — this struct only carries the declaration
/// the engraver will need to consult.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SmuflVersionRequirement {
    /// Minimum SMuFL version required by this score.
    pub minimum: SmuflVersion,
    /// SMuFL version this score was authored against. Used for detecting
    /// whether newer glyphs are in use.
    pub authored_against: SmuflVersion,
}

impl Default for SmuflVersionRequirement {
    /// `1.4` for both fields — the SMuFL version this repository already
    /// targets (`epiphany_layout_ir::glyph::GlyphCatalogIdentity`'s default,
    /// `glyph.rs:120`), so this default aligns with what Push 4b tranche 3b
    /// will unify against. Used as
    /// [`crate::graph::ScoreTuningContext`]'s `smufl` default when decoding a
    /// wire stream that predates this field (schema major <3).
    fn default() -> Self {
        let v1_4 = SmuflVersion::from_decimal(1, "4").expect("1.4 is a valid SMuFL version");
        SmuflVersionRequirement {
            minimum: v1_4,
            authored_against: v1_4,
        }
    }
}

// ===========================================================================
// Consumer (a): accidental resolution (Chapter 4 §"Score-Local Extensions",
// `:3224`).
// ===========================================================================

/// Resolves an [`AccidentalId`] against a score's accidental extensions,
/// honoring the precedence Chapter 4 states outright (`:3224`): "Extensions
/// are stored on the score and override or augment the base registry during
/// resolution" — an `overrides` entry wins over an `additions` entry wins
/// over the base registry.
///
/// `base_registry` is the resolved contents of `extensions.base`'s catalog
/// body. `epiphany-core` has no built-in catalog of accidental-registry
/// bodies this tranche (the same deferred-data-catalog discipline as
/// `crate::pitch_space`'s six underdetermined pitch spaces and
/// `crate::tuning`'s partial tuning catalog: inventing registry contents the
/// specification does not pin would be exactly the `NOTEHEAD_ANCHORS`
/// failure this project exists to avoid), so callers supply the base
/// registry's contents directly rather than this function reaching into a
/// catalog that does not exist.
///
/// Returns `None` when `id` is not found in `overrides`, `additions`, or
/// `base_registry`.
pub fn resolve_accidental<'a>(
    base_registry: &'a [AccidentalDefinition],
    extensions: &'a ScoreAccidentalExtensions,
    id: &AccidentalId,
) -> Option<&'a AccidentalDefinition> {
    extensions
        .overrides
        .iter()
        .find(|d| &d.id == id)
        .or_else(|| extensions.additions.iter().find(|d| &d.id == id))
        .or_else(|| base_registry.iter().find(|d| &d.id == id))
}

// ===========================================================================
// Consumer (b): the modification-compatibility invariant
// (`req:tuning:accidental-modification-compatibility`, `core_spec.tex:3120`).
// ===========================================================================

/// Whether `modification` is expressible in the interval algebra of `space`
/// (`req:tuning:accidental-modification-compatibility`, `core_spec.tex:3120`:
/// "An accidental's modification MUST be expressible in the interval algebra
/// of every pitch space that references its registry ... Implementations
/// MUST reject scores referencing an accidental in a space whose algebra does
/// not admit the modification").
///
/// `space` is resolved structurally against the built-in catalog
/// ([`built_in_position_structure`], Push 4b tranche 1) — the same lookup
/// `Pitch::transposed` uses, so this consumer and that one agree on what
/// "the algebra of a pitch space" means.
///
/// The requirement states two rules by name — `CmnChromatic` only in spaces
/// "with `DiatonicChromatic` or compatible algebra" (matched here against
/// [`PositionStructure::DiatonicOverChromatic`], the position-structure
/// family that algebra describes); `EdoSteps` only in `Chromatic` or
/// `Registered` spaces — and this tranche's contract directs extending "the
/// same shape" to `JiRatio` <-> [`PositionStructure::JiLattice`] (also
/// admitting `Registered`, matching `EdoSteps`'s explicit inclusion of it).
/// `Cents` and `Registered` modifications have no requirement-stated
/// constraint of their own — inventing one *would be* the `NOTEHEAD_ANCHORS`
/// failure this project has already paid for twice — so they are accepted
/// whenever `space` itself resolves.
///
/// An unresolvable `space` (an identifier outside the built-in catalog, or
/// one of the six catalog-named-but-underdetermined spaces
/// `built_in_position_structure` deliberately returns `None` for) fails
/// closed for *every* modification kind, `Cents` and `Registered` included:
/// nothing can be shown expressible in an algebra that cannot itself be
/// established (`req:pitch:space-capability-refusal`'s discipline, applied
/// here as tranche 1 applied it to `Pitch::transposed`).
pub fn accidental_modification_compatible_with_space(
    modification: &PitchSpaceModification,
    space: &PitchSpaceId,
) -> bool {
    let Some(structure) = built_in_position_structure(space) else {
        return false;
    };
    match modification {
        PitchSpaceModification::CmnChromatic(_) => {
            matches!(structure, PositionStructure::DiatonicOverChromatic { .. })
        }
        PitchSpaceModification::EdoSteps(_) => matches!(
            structure,
            PositionStructure::Chromatic { .. } | PositionStructure::Registered(_)
        ),
        PitchSpaceModification::JiRatio { .. } => matches!(
            structure,
            PositionStructure::JiLattice { .. } | PositionStructure::Registered(_)
        ),
        PitchSpaceModification::Cents(_) | PitchSpaceModification::Registered(_) => true,
    }
}

// ===========================================================================
// Test-only fixtures, shared with `codec.rs`, `textvalue_graph.rs`, and
// `invariants.rs`'s test modules so each does not hand-roll its own minimal
// `AccidentalDefinition`.
// ===========================================================================

#[cfg(test)]
pub(crate) fn fixture_engraving() -> AccidentalEngraving {
    let su = |x: f64| SpaceUnit(CanonicalF64::new(x).expect("test fixture value is finite"));
    AccidentalEngraving {
        bounding_box: EngravingBoundingBox {
            left: su(-0.5),
            right: su(0.5),
            top: su(1.0),
            bottom: su(-1.0),
        },
        anchor: AnchorPoint {
            x: su(0.0),
            y: su(0.0),
        },
        advance_width: su(1.0),
        stacking_order: 0,
        default_parenthesized: false,
    }
}

#[cfg(test)]
pub(crate) fn fixture_definition(
    id: &str,
    modification: PitchSpaceModification,
) -> AccidentalDefinition {
    AccidentalDefinition {
        id: AccidentalId::new(id),
        name: id.to_string(),
        glyph: GlyphReference::Smufl(0xE262),
        modification,
        engraving: fixture_engraving(),
        combination: AccidentalCombination::Solitary,
    }
}

#[cfg(test)]
pub(crate) fn fixture_extensions(
    base: &str,
    modification: PitchSpaceModification,
) -> ScoreAccidentalExtensions {
    ScoreAccidentalExtensions {
        base: AccidentalRegistryId::new(base),
        additions: vec![fixture_definition("test-accidental", modification)],
        overrides: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- S12: SmuflVersion ordering. -----------------------------------

    #[test]
    fn smufl_version_from_decimal_normalizes_one_and_two_digit_minors() {
        assert_eq!(
            SmuflVersion::from_decimal(1, "4"),
            Some(SmuflVersion {
                major: 1,
                minor_centi: 40
            })
        );
        assert_eq!(
            SmuflVersion::from_decimal(1, "3"),
            Some(SmuflVersion {
                major: 1,
                minor_centi: 30
            })
        );
        assert_eq!(
            SmuflVersion::from_decimal(1, "12"),
            Some(SmuflVersion {
                major: 1,
                minor_centi: 12
            })
        );
        // "3" and "30" both denote 1.3.
        assert_eq!(
            SmuflVersion::from_decimal(1, "3"),
            SmuflVersion::from_decimal(1, "30")
        );
    }

    #[test]
    fn smufl_version_from_decimal_rejects_malformed_input() {
        assert_eq!(SmuflVersion::from_decimal(1, ""), None);
        assert_eq!(SmuflVersion::from_decimal(1, "123"), None);
        assert_eq!(SmuflVersion::from_decimal(1, "x"), None);
    }

    #[test]
    fn smufl_version_orders_the_real_release_sequence() {
        // The anchor: this is SMuFL's actual release order. A test that
        // would PASS under literal-minor storage (where 1.3 and 1.4 sort
        // before 1.12) would not lock S12 at all — see the mutation in
        // DECISIONS.md, which reproduces exactly that failure mode and
        // confirms this test dies under it.
        let v = |minor: &str| SmuflVersion::from_decimal(1, minor).unwrap();
        let sequence = [v("12"), v("18"), v("20"), v("3"), v("4")];
        for pair in sequence.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{:?} did not sort before {:?} in SMuFL's real release order",
                pair[0],
                pair[1]
            );
        }
    }

    // --- S10: PitchSpaceModification::Cents. ---------------------------

    #[test]
    fn cents_round_trips_a_finite_value_and_guards_non_finite() {
        let cents = CanonicalF64::new(23.46)
            .map(PitchSpaceModification::Cents)
            .expect("23.46 is finite");
        match cents {
            PitchSpaceModification::Cents(c) => assert_eq!(c.get(), 23.46),
            other => panic!("expected Cents, got {other:?}"),
        }
        // The S10 guard: a non-finite value can never become a `Cents`
        // payload, because `CanonicalF64::new` is the only way to produce
        // one and it rejects NaN/infinity outright.
        assert!(CanonicalF64::new(f64::NAN)
            .map(PitchSpaceModification::Cents)
            .is_none());
        assert!(CanonicalF64::new(f64::INFINITY)
            .map(PitchSpaceModification::Cents)
            .is_none());
        assert!(CanonicalF64::new(f64::NEG_INFINITY)
            .map(PitchSpaceModification::Cents)
            .is_none());
    }

    // --- Consumer (a): resolution precedence. ---------------------------

    #[test]
    fn resolution_precedence_overrides_beats_additions_beats_base() {
        let base = vec![fixture_definition(
            "flat",
            PitchSpaceModification::CmnChromatic(-1),
        )];
        let mut extensions =
            fixture_extensions("cmn-accidentals", PitchSpaceModification::CmnChromatic(1));
        extensions.additions = vec![fixture_definition(
            "flat",
            PitchSpaceModification::CmnChromatic(-2),
        )];
        extensions.overrides = vec![fixture_definition(
            "flat",
            PitchSpaceModification::CmnChromatic(-3),
        )];

        let id = AccidentalId::new("flat");
        // Overrides wins over additions wins over base.
        let resolved = resolve_accidental(&base, &extensions, &id).expect("resolves");
        assert_eq!(
            resolved.modification,
            PitchSpaceModification::CmnChromatic(-3),
            "an overrides entry must shadow an additions entry for the same id"
        );

        // Drop the overrides entry: additions should now win.
        extensions.overrides.clear();
        let resolved = resolve_accidental(&base, &extensions, &id).expect("resolves");
        assert_eq!(
            resolved.modification,
            PitchSpaceModification::CmnChromatic(-2),
            "an additions entry must shadow the base registry for the same id"
        );

        // Drop the additions entry too: an id with no extension resolves to
        // the base registry.
        extensions.additions.clear();
        let resolved = resolve_accidental(&base, &extensions, &id).expect("resolves");
        assert_eq!(
            resolved.modification,
            PitchSpaceModification::CmnChromatic(-1),
            "a base-registry id with no extension must resolve to the base"
        );

        // An id present nowhere resolves to nothing.
        assert!(
            resolve_accidental(&base, &extensions, &AccidentalId::new("nonexistent")).is_none()
        );
    }

    // --- Consumer (b): the modification-compatibility predicate. -------

    #[test]
    fn cmn_chromatic_is_compatible_only_with_diatonic_over_chromatic() {
        let cmn_chromatic = PitchSpaceModification::CmnChromatic(1);
        assert!(accidental_modification_compatible_with_space(
            &cmn_chromatic,
            &PitchSpaceId::new("cmn-12")
        ));
        // A test that only checked the accept case would miss the reject.
        assert!(!accidental_modification_compatible_with_space(
            &cmn_chromatic,
            &PitchSpaceId::new("edo-31")
        ));
    }

    #[test]
    fn edo_steps_is_compatible_with_chromatic_and_registered_not_diatonic() {
        let edo_steps = PitchSpaceModification::EdoSteps(1);
        assert!(accidental_modification_compatible_with_space(
            &edo_steps,
            &PitchSpaceId::new("edo-31")
        ));
        assert!(!accidental_modification_compatible_with_space(
            &edo_steps,
            &PitchSpaceId::new("cmn-12")
        ));
    }

    #[test]
    fn ji_ratio_is_compatible_only_with_ji_lattice() {
        // None of the built-in `ji-*limit` spaces structurally resolve this
        // tranche (`built_in_position_structure` returns `None` for all
        // three, tranche 1's honest gap), so both sides of this rule are
        // exercised against an unresolvable space and a resolvable
        // non-`JiLattice` one.
        let ji_ratio = PitchSpaceModification::JiRatio {
            numerator: 81,
            denominator: NonZeroU32::new(80).unwrap(),
        };
        assert!(!accidental_modification_compatible_with_space(
            &ji_ratio,
            &PitchSpaceId::new("cmn-12")
        ));
        assert!(!accidental_modification_compatible_with_space(
            &ji_ratio,
            &PitchSpaceId::new("ji-5limit")
        ));
    }

    #[test]
    fn cents_and_registered_are_unconstrained_on_a_resolved_space() {
        let cents = PitchSpaceModification::Cents(CanonicalF64::new(23.46).unwrap());
        let registered =
            PitchSpaceModification::Registered(ModificationRegistryId::new("sagittal"));
        for space in ["cmn-12", "edo-31"] {
            assert!(accidental_modification_compatible_with_space(
                &cents,
                &PitchSpaceId::new(space)
            ));
            assert!(accidental_modification_compatible_with_space(
                &registered,
                &PitchSpaceId::new(space)
            ));
        }
    }

    #[test]
    fn every_modification_kind_fails_closed_on_an_unresolvable_space() {
        let unknown = PitchSpaceId::new("not-a-built-in-space");
        assert!(!accidental_modification_compatible_with_space(
            &PitchSpaceModification::CmnChromatic(1),
            &unknown
        ));
        assert!(!accidental_modification_compatible_with_space(
            &PitchSpaceModification::EdoSteps(1),
            &unknown
        ));
        assert!(!accidental_modification_compatible_with_space(
            &PitchSpaceModification::JiRatio {
                numerator: 3,
                denominator: NonZeroU32::new(2).unwrap()
            },
            &unknown
        ));
        assert!(!accidental_modification_compatible_with_space(
            &PitchSpaceModification::Cents(CanonicalF64::new(1.0).unwrap()),
            &unknown
        ));
        assert!(!accidental_modification_compatible_with_space(
            &PitchSpaceModification::Registered(ModificationRegistryId::new("x")),
            &unknown
        ));
    }
}
