//! [`TextValue`] for the leaves and the hand-written composites of Chapter 5.
//!
//! The 82 `struct_codec!` structs, the 8 `unit_codec!` unit structs, the 17
//! `cstyle_enum_codec!` enums and the 9 `catalog_id_codec!` ids get their impls
//! from the *same macro invocation* that gives them their binary codec, so their
//! field order cannot drift from the order the binary form writes. See
//! `codec.rs`. This module supplies what those macros cannot: the leaves, and the
//! composites whose `Codec` impl is hand-written.
//!
//! Every `parse` here obeys `req:textproj:strict-parse`. Where the only way to
//! construct a value is through a normalizing constructor — `RationalTime::new`
//! reduces, `PitchSpaceId::new` folds to NFC — the impl constructs and then
//! **compares against its input**, rejecting on difference. It never returns the
//! normalized value and calls that acceptance.

use epiphany_determinism::{CanonicalDecode, CanonicalEncode, CanonicalF64, ContentHash};
use num_rational::BigRational;
use num_traits::Zero;

use crate::textvalue::{kebab, Sexp, TextError, TextValue};
use crate::time::{
    MusicalDuration, MusicalPosition, RationalTime, WallClockDuration, WallClockTime,
};

// ===========================================================================
// Byte-string leaves.
// ===========================================================================

/// An identifier or hash is a byte string (`req:textproj:value-projection`
/// clause 6).
///
/// `decode_canonical` is the leaf's own validating decoder, but validating is not
/// the same as *rejecting non-canonical bytes*: a decoder that accepts a
/// denormalized encoding would let two byte strings — and so two texts — denote
/// one value. The re-encode comparison closes that, exactly as
/// `Score::decode_canonical` does for the binary form.
macro_rules! bytes_text_value {
    ($($ty:ty => $what:literal),* $(,)?) => {
        $(
            impl TextValue for $ty {
                fn project(&self) -> Sexp {
                    Sexp::Bytes(self.to_canonical_bytes())
                }
                fn parse(s: &Sexp) -> Result<Self, TextError> {
                    let Sexp::Bytes(bytes) = s else {
                        return Err(TextError::Expected {
                            expected: $what,
                            found: class_of(s),
                        });
                    };
                    let value = <$ty>::decode_canonical(bytes)
                        .map_err(|_| TextError::NotCanonical($what))?;
                    if value.to_canonical_bytes() != *bytes {
                        return Err(TextError::NotCanonical(
                            concat!($what, " is not canonically encoded")
                        ));
                    }
                    Ok(value)
                }
            }
        )*
    };
}

/// The lexical class of `s`, for error messages. Mirrors `Sexp::class`, which is
/// private to its module.
pub(crate) fn class_of(s: &Sexp) -> &'static str {
    match s {
        Sexp::List(_) => "list",
        Sexp::Symbol(_) => "symbol",
        Sexp::Int(_) => "integer",
        Sexp::Bytes(_) => "byte string",
        Sexp::Str(_) => "string",
    }
}

bytes_text_value! {
    crate::ids::ReplicaId => "ReplicaId",
    crate::ids::OperationId => "OperationId",
    crate::ids::EventId => "EventId",
    crate::ids::PitchId => "PitchId",
    crate::ids::VoiceId => "VoiceId",
    crate::ids::StaffId => "StaffId",
    crate::ids::StaffInstanceId => "StaffInstanceId",
    crate::ids::StaffGroupId => "StaffGroupId",
    crate::ids::RegionId => "RegionId",
    crate::ids::InstrumentId => "InstrumentId",
    crate::ids::PartDefinitionId => "PartDefinitionId",
    crate::ids::MeasureId => "MeasureId",
    crate::ids::BarlineAlignmentGroupId => "BarlineAlignmentGroupId",
    crate::ids::SlurId => "SlurId",
    crate::ids::TieId => "TieId",
    crate::ids::BeamId => "BeamId",
    crate::ids::SpannerId => "SpannerId",
    crate::ids::TupletId => "TupletId",
    crate::ids::MarkerId => "MarkerId",
    crate::ids::AnalyticalAnnotationId => "AnalyticalAnnotationId",
    crate::ids::CommentId => "CommentId",
    crate::ids::RepeatStructureId => "RepeatStructureId",
    crate::ids::LyricLineId => "LyricLineId",
    crate::ids::ChordSymbolId => "ChordSymbolId",
    crate::ids::GraphicObjectId => "GraphicObjectId",
    crate::ids::GraphicGestureId => "GraphicGestureId",
    crate::ids::TimeSignatureId => "TimeSignatureId",
    crate::ids::AnalysisLayerId => "AnalysisLayerId",
    crate::ids::ViewId => "ViewId",
    ContentHash => "ContentHash",
}

/// A `CanonicalF64` is its eight canonical little-endian IEEE 754 bytes, never a
/// decimal (`req:textproj:value-projection` clause 6). Decimal float text is not
/// canonically unique — shortest-round-trip and 17-significant-digit spellings both
/// round-trip, and `-0.0` has two spellings — so a decimal tempo would break
/// `req:textproj:canonical-text` at the first tempo mark.
impl TextValue for CanonicalF64 {
    fn project(&self) -> Sexp {
        Sexp::Bytes(self.to_le_bytes().to_vec())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let Sexp::Bytes(bytes) = s else {
            return Err(TextError::Expected {
                expected: "CanonicalF64",
                found: class_of(s),
            });
        };
        let bytes: [u8; 8] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| TextError::NotCanonical("a CanonicalF64 is exactly eight bytes"))?;
        CanonicalF64::from_le_bytes(bytes).ok_or(TextError::NotCanonical(
            "not a canonical f64 (NaN, or -0.0)",
        ))
    }
}

// ===========================================================================
// Rationals.
// ===========================================================================

/// `(ratio <numerator> <denominator>)`, in lowest terms with a positive
/// denominator and the sign on the numerator; zero is `(ratio 0 1)`.
///
/// `RationalTime::new` **reduces**, so parsing through it would silently accept
/// `(ratio 2 4)` as `1/2` — normalizing, which `req:textproj:strict-parse`
/// forbids. The canonical form is therefore checked *before* construction.
impl TextValue for RationalTime {
    fn project(&self) -> Sexp {
        let big = self.to_big();
        Sexp::ratio(big.numer().clone(), big.denom().clone())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "rational",
            found: class_of(s),
        })?;
        let [head, numerator, denominator] = items else {
            return Err(TextError::Syntax(
                "a rational is `(ratio <numerator> <denominator>)`",
            ));
        };
        if head.as_symbol() != Some("ratio") {
            return Err(TextError::Syntax("a rational is headed by `ratio`"));
        }
        let (Sexp::Int(numerator), Sexp::Int(denominator)) = (numerator, denominator) else {
            return Err(TextError::Expected {
                expected: "integer",
                found: "non-integer",
            });
        };
        if denominator.is_zero() {
            // Checked before `BigRational::new`, which panics on a zero
            // denominator rather than returning an error.
            return Err(TextError::NotCanonical("rational denominator is zero"));
        }

        // `BigRational::new` *is* the canonical form: it reduces, keeps the
        // denominator positive, and puts the sign on the numerator. So the
        // canonical spelling of this value is the one whose parts survive it
        // unchanged. Constructing first and returning the result would accept
        // `(ratio 2 4)` as `1/2` — normalizing, which `req:textproj:strict-parse`
        // forbids. Comparing instead rejects it.
        let reduced = BigRational::new(numerator.clone(), denominator.clone());
        if reduced.numer() != numerator || reduced.denom() != denominator {
            return Err(TextError::NotCanonical(
                "rational is not in lowest terms with a positive denominator",
            ));
        }
        Ok(RationalTime::from_big(reduced))
    }
}

// ===========================================================================
// Transparent newtypes.
// ===========================================================================

/// A newtype is projected as its field alone, with no wrapper
/// (`req:textproj:value-projection` clause 2), mirroring the binary form in which
/// a newtype delegates to its field and adds no bytes.
macro_rules! newtype_text_value {
    ($($ty:ty => $inner:ty),* $(,)?) => {
        $(
            impl TextValue for $ty {
                fn project(&self) -> Sexp {
                    TextValue::project(&self.0)
                }
                fn parse(s: &Sexp) -> Result<Self, TextError> {
                    <$inner as TextValue>::parse(s).map(Self)
                }
            }
        )*
    };
}

newtype_text_value! {
    MusicalPosition => RationalTime,
    MusicalDuration => RationalTime,
    WallClockTime => i64,
    WallClockDuration => i64,
    crate::graph::OctaveOffset => i8,
}

/// A transparent newtype over an integer whose constructor **validates** — it
/// rejects a value outside the type's domain rather than adjusting one into it.
///
/// So no `ensure_canonical` is wanted here, and none would fire: an accepted value
/// re-projects to exactly its input. What rejects `(power-of-two 3)` is `new`
/// returning `None`, and that is the whole of the strictness.
macro_rules! validated_int_newtype_text_value {
    ($($ty:ty => $inner:ty, $new:path, $get:ident, $what:literal);* $(;)?) => {
        $(
            impl TextValue for $ty {
                fn project(&self) -> Sexp {
                    TextValue::project(&self.$get())
                }
                fn parse(s: &Sexp) -> Result<Self, TextError> {
                    let inner = <$inner as TextValue>::parse(s)?;
                    $new(inner).ok_or(TextError::NotCanonical($what))
                }
            }
        )*
    };
}

validated_int_newtype_text_value! {
    crate::graph::PowerOfTwo => u16, crate::graph::PowerOfTwo::new, get,
        "a PowerOfTwo denominator must be a power of two";
    core::num::NonZeroU16 => u16, core::num::NonZeroU16::new, get,
        "a NonZeroU16 must not be zero";
}

// ===========================================================================
// Tempo.
// ===========================================================================

/// `(tempo <bpm> <beat-unit>)`, in the order `impl Codec for Tempo` writes them.
///
/// Both fields are private and `Tempo::new` rejects a non-finite or non-positive
/// BPM, so the value can only be rebuilt through a validating constructor.
/// `ensure_canonical` closes what that leaves open.
impl TextValue for crate::tempo::Tempo {
    fn project(&self) -> Sexp {
        let bpm = CanonicalF64::new(self.bpm()).expect("a valid tempo has a canonical BPM");
        Sexp::List(vec![
            Sexp::Symbol(kebab("Tempo")),
            bpm.project(),
            self.beat_unit().project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("Tempo"), 2)?;
        let bpm = CanonicalF64::parse(&fields[0])?;
        let beat_unit = MusicalDuration::parse(&fields[1])?;
        // `new` rejects a non-positive or non-finite BPM rather than adjusting
        // one, and `CanonicalF64::parse` has already refused a non-canonical float,
        // so there is nothing left for a whole-value guard to catch.
        crate::tempo::Tempo::new(bpm.get(), beat_unit)
            .ok_or(TextError::NotCanonical("tempo BPM or beat unit is invalid"))
    }
}

// ===========================================================================
// Helpers the codec macros expand into.
// ===========================================================================

/// A zero-field struct is the bare symbol `<type-name>`, as a fieldless variant is
/// (`req:textproj:value-projection` clause 1).
pub(crate) fn project_unit(type_name: &str) -> Sexp {
    Sexp::Symbol(kebab(type_name))
}

/// Reads a zero-field struct or fieldless variant.
pub(crate) fn parse_unit(s: &Sexp, type_name: &'static str) -> Result<(), TextError> {
    match s.as_symbol() {
        Some(name) if name == kebab(type_name) => Ok(()),
        Some(found) => Err(TextError::UnknownConstructor {
            type_name,
            found: found.to_owned(),
        }),
        None => Err(TextError::Expected {
            expected: "symbol",
            found: class_of(s),
        }),
    }
}

/// Reads a catalog id: a string, checked to be exactly what interning it produces.
///
/// `PitchSpaceId::new` and its siblings **fold to Unicode NFC**. Constructing
/// through them and returning the result would accept a non-NFC spelling and
/// silently normalize it — two texts denoting one value, which
/// `req:textproj:strict-parse` forbids and which the binary form's whole-value
/// re-encode guard catches only because it re-encodes. Here the comparison is
/// explicit.
pub(crate) fn parse_catalog_id<T, F>(
    s: &Sexp,
    make: F,
    as_str: impl Fn(&T) -> &str,
) -> Result<T, TextError>
where
    F: Fn(String) -> T,
{
    let Sexp::Str(text) = s else {
        return Err(TextError::Expected {
            expected: "catalog id",
            found: class_of(s),
        });
    };
    let value = make(text.clone());
    if as_str(&value) != text {
        return Err(TextError::NotCanonical("catalog id is not in Unicode NFC"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textvalue::read_sexp;

    #[test]
    fn a_rational_projects_in_lowest_terms_with_the_sign_on_the_numerator() {
        let r = RationalTime::new(-2, 4).unwrap();
        assert_eq!(r.project().render(), "(ratio -1 2)");
        assert_eq!(RationalTime::zero().project().render(), "(ratio 0 1)");
        assert_eq!(RationalTime::one().project().render(), "(ratio 1 1)");
    }

    /// Parsing through `RationalTime::new` would reduce these and call it success.
    #[test]
    fn a_non_canonical_rational_is_rejected_not_reduced() {
        for bad in [
            "(ratio 2 4)",   // not in lowest terms
            "(ratio 1 -2)",  // sign on the denominator
            "(ratio -1 -2)", // both negative
            "(ratio 0 2)",   // zero is (ratio 0 1)
            "(ratio 1 0)",   // zero denominator
        ] {
            let s = read_sexp(bad).unwrap();
            assert!(
                RationalTime::parse(&s).is_err(),
                "{bad} must be rejected, not reduced"
            );
        }
    }

    #[test]
    fn a_rational_round_trips_through_its_projection() {
        for (n, d) in [(0, 1), (1, 2), (-3, 4), (7, 1), (-1, 1)] {
            let r = RationalTime::new(n, d).unwrap();
            let text = r.project().render();
            let back = RationalTime::parse(&read_sexp(&text).unwrap()).unwrap();
            assert_eq!(r, back, "{text} did not round-trip");
        }
    }

    /// A `MusicalPosition` *is* a rational; the wrapper adds no bytes and no text.
    #[test]
    fn a_newtype_is_transparent() {
        let p = MusicalPosition(RationalTime::new(3, 4).unwrap());
        assert_eq!(p.project().render(), "(ratio 3 4)");
        assert_eq!(WallClockTime(-5).project().render(), "-5");
    }

    #[test]
    fn the_codec_macros_project_units_c_style_enums_and_catalog_ids() {
        use crate::event::ArticulationMark;
        use crate::pitch::{AccidentalId, SpellingSourceKind};
        use crate::textvalue::read_sexp;
        // cstyle enum -> bare symbol
        assert_eq!(
            SpellingSourceKind::UserChosen.project().render(),
            "user-chosen"
        );
        assert_eq!(
            SpellingSourceKind::parse(&read_sexp("propagated").unwrap()).unwrap(),
            SpellingSourceKind::Propagated
        );
        assert!(SpellingSourceKind::parse(&read_sexp("nope").unwrap()).is_err());
        // unit struct -> bare symbol
        assert_eq!(ArticulationMark.project().render(), "articulation-mark");
        // catalog id -> string, NFC-checked
        let id = AccidentalId::new("sharp");
        assert_eq!(id.project().render(), "\"sharp\"");
        assert_eq!(
            AccidentalId::parse(&read_sexp("\"sharp\"").unwrap()).unwrap(),
            id
        );
        // A non-NFC spelling must be rejected, not folded: "e" + combining acute.
        let decomposed = read_sexp("\"e\u{0301}\"").unwrap();
        assert!(
            AccidentalId::parse(&decomposed).is_err(),
            "non-NFC catalog id must be rejected, not normalized"
        );
    }

    #[test]
    fn a_canonical_f64_is_eight_bytes_never_a_decimal() {
        let f = CanonicalF64::new(1.5).unwrap();
        assert_eq!(f.project().render(), "#x000000000000f83f");
        let back = CanonicalF64::parse(&read_sexp("#x000000000000f83f").unwrap()).unwrap();
        assert_eq!(f, back);
        assert!(CanonicalF64::parse(&read_sexp("#x00").unwrap()).is_err());
    }
}
