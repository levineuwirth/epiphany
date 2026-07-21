//! [`TextValue`] for the hand-written `Codec` composites of Chapter 2's pitch and
//! spelling subsystem (`pitch.rs`).
//!
//! The macro-generated types of `pitch.rs` — the `struct_codec!` structs
//! ([`ScalePosition`](crate::pitch::ScalePosition), [`Pitch`](crate::pitch::Pitch),
//! [`PitchSpelling`](crate::pitch::PitchSpelling), …), the `cstyle_enum_codec!`
//! [`CmnNominal`](crate::pitch::CmnNominal) /
//! [`SpellingSourceKind`](crate::pitch::SpellingSourceKind), and the
//! `catalog_id_codec!` ids — get their [`TextValue`] from the same macro that
//! writes their bytes, so their projection cannot drift from the binary form.
//! This module supplies the rest: the tagged unions and the two structs whose
//! `Codec` is written out by hand. Each projection mirrors the field / variant
//! order of the corresponding `impl Codec` in `codec.rs` exactly.
//!
//! Two of these parse through a **validating** constructor, so they obey
//! `req:textproj:strict-parse` by re-projecting and comparing rather than
//! returning a laundered value:
//!
//! * [`ReferencePitch`](crate::pitch::ReferencePitch) — the frequency field is
//!   private and reachable only through `ReferencePitch::new`, which rejects a
//!   non-positive frequency.
//! * [`SpellingPrecedence`](crate::pitch::SpellingPrecedence) — the order is
//!   private and reachable only through `SpellingPrecedence::new`, which rejects
//!   any order that is not a total ranking (a kind missing or duplicated). A
//!   `Vec` parse preserves order and so cannot see a duplicate itself; `new` is
//!   what refuses it.
//!
//! Tagged-union parsing is strict about *shape* too: a fieldless variant projects
//! as a bare symbol, so its one-element list spelling (`(inherit)`) is rejected,
//! not accepted.

use epiphany_determinism::CanonicalF64;

use crate::pitch::{
    AcousticRealization, PitchSpacePosition, ReferencePitch, SpellingDirective, SpellingNominal,
    SpellingPrecedence, SpellingScope, SpellingSource, SpellingSourceKind, TuningReference,
    VoiceSelector,
};
use crate::textvalue::{kebab, Sexp, TextError, TextValue};
use crate::textvalue_impls::class_of;

// ===========================================================================
// Tagged-union helpers.
// ===========================================================================

/// Projects a tagged-union variant (`req:textproj:value-projection` clause 3): a
/// fieldless variant is its bare kebab name; a variant with fields is a list of
/// that name followed by the fields' projections.
fn variant(name: &str, fields: Vec<Sexp>) -> Sexp {
    if fields.is_empty() {
        Sexp::Symbol(kebab(name))
    } else {
        let mut items = Vec::with_capacity(fields.len() + 1);
        items.push(Sexp::Symbol(kebab(name)));
        items.extend(fields);
        Sexp::List(items)
    }
}

/// The constructor name of a tagged-union projection, and its field list when the
/// projection is *applied* (a list). A bare symbol yields `None` for the fields —
/// it is a fieldless variant — which is what lets a caller reject a fieldless
/// variant miswritten as a one-element list, a spelling `project` never emits
/// (`req:textproj:strict-parse`).
fn constructor(s: &Sexp) -> Result<(&str, Option<&[Sexp]>), TextError> {
    match s {
        Sexp::Symbol(name) => Ok((name.as_str(), None)),
        Sexp::List(items) => {
            let (head, rest) = items.split_first().ok_or(TextError::Syntax(
                "a tagged-union variant is a symbol or a non-empty list",
            ))?;
            let name = head
                .as_symbol()
                .ok_or(TextError::Syntax("a variant constructor is a symbol"))?;
            Ok((name, Some(rest)))
        }
        _ => Err(TextError::Expected {
            expected: "tagged-union variant",
            found: class_of(s),
        }),
    }
}

/// The field count of a (possibly bare) variant, for arity diagnostics.
fn field_count(fields: Option<&[Sexp]>) -> usize {
    fields.map_or(0, <[Sexp]>::len)
}

fn arity(type_name: &'static str, expected: usize, found: usize) -> TextError {
    TextError::Arity {
        type_name,
        expected,
        found,
    }
}

/// Rejects a fieldless variant spelled as a list. A fieldless variant projects as
/// a bare symbol, so any field list — even the empty `(inherit)` — is not its
/// canonical text and must be refused rather than absorbed
/// (`req:textproj:strict-parse`).
fn expect_fieldless(fields: Option<&[Sexp]>) -> Result<(), TextError> {
    match fields {
        None => Ok(()),
        Some(_) => Err(TextError::Syntax(
            "a fieldless variant projects as a bare symbol, not a list",
        )),
    }
}

// ===========================================================================
// Tagged unions.
// ===========================================================================

/// A position within a pitch space (`req:textproj:value-projection` clause 3),
/// mirroring `PitchSpacePosition`'s `Codec::enc` variant order: `Cmn`, `Integer`,
/// `JiVector`, `Registered`. The `Cmn` fields project positionally in their
/// declared order — nominal, alteration, octave — never by name.
impl TextValue for PitchSpacePosition {
    fn project(&self) -> Sexp {
        match self {
            PitchSpacePosition::Cmn {
                nominal,
                alteration,
                octave,
            } => variant(
                "Cmn",
                vec![nominal.project(), alteration.project(), octave.project()],
            ),
            PitchSpacePosition::Integer { space_size, index } => {
                variant("Integer", vec![space_size.project(), index.project()])
            }
            PitchSpacePosition::JiVector { components } => {
                variant("JiVector", vec![components.project()])
            }
            PitchSpacePosition::Registered(id) => variant("Registered", vec![id.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("Cmn") {
            let Some([nominal, alteration, octave]) = fields else {
                return Err(arity("PitchSpacePosition", 3, field_count(fields)));
            };
            return Ok(PitchSpacePosition::Cmn {
                nominal: TextValue::parse(nominal)?,
                alteration: TextValue::parse(alteration)?,
                octave: TextValue::parse(octave)?,
            });
        }
        if head == kebab("Integer") {
            let Some([space_size, index]) = fields else {
                return Err(arity("PitchSpacePosition", 2, field_count(fields)));
            };
            return Ok(PitchSpacePosition::Integer {
                space_size: TextValue::parse(space_size)?,
                index: TextValue::parse(index)?,
            });
        }
        if head == kebab("JiVector") {
            let Some([components]) = fields else {
                return Err(arity("PitchSpacePosition", 1, field_count(fields)));
            };
            return Ok(PitchSpacePosition::JiVector {
                components: TextValue::parse(components)?,
            });
        }
        if head == kebab("Registered") {
            let Some([id]) = fields else {
                return Err(arity("PitchSpacePosition", 1, field_count(fields)));
            };
            return Ok(PitchSpacePosition::Registered(TextValue::parse(id)?));
        }
        Err(TextError::UnknownConstructor {
            type_name: "PitchSpacePosition",
            found: head.to_owned(),
        })
    }
}

/// The tuning reference governing a pitch: `inherit`, or `(explicit <id>)`.
/// Mirrors `TuningReference`'s `Codec::enc`.
impl TextValue for TuningReference {
    fn project(&self) -> Sexp {
        match self {
            TuningReference::Inherit => variant("Inherit", vec![]),
            TuningReference::Explicit(id) => variant("Explicit", vec![id.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("Inherit") {
            expect_fieldless(fields)?;
            return Ok(TuningReference::Inherit);
        }
        if head == kebab("Explicit") {
            let Some([id]) = fields else {
                return Err(arity("TuningReference", 1, field_count(fields)));
            };
            return Ok(TuningReference::Explicit(TextValue::parse(id)?));
        }
        Err(TextError::UnknownConstructor {
            type_name: "TuningReference",
            found: head.to_owned(),
        })
    }
}

/// How the tuning system resolves to a frequency: `implicit`, `(cents-offset
/// <f64>)`, or `(absolute-hz <f64>)`. Mirrors `AcousticRealization`'s
/// `Codec::enc`. Each payload is a [`CanonicalF64`], so it projects as its eight
/// canonical little-endian bytes — never a decimal, which would not be uniquely
/// spellable (Appendix D §"Floating-Point Values").
impl TextValue for AcousticRealization {
    fn project(&self) -> Sexp {
        match self {
            AcousticRealization::Implicit => variant("Implicit", vec![]),
            AcousticRealization::CentsOffset(c) => variant("CentsOffset", vec![c.project()]),
            AcousticRealization::AbsoluteHz(c) => variant("AbsoluteHz", vec![c.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("Implicit") {
            expect_fieldless(fields)?;
            return Ok(AcousticRealization::Implicit);
        }
        if head == kebab("CentsOffset") {
            let Some([c]) = fields else {
                return Err(arity("AcousticRealization", 1, field_count(fields)));
            };
            return Ok(AcousticRealization::CentsOffset(TextValue::parse(c)?));
        }
        if head == kebab("AbsoluteHz") {
            let Some([c]) = fields else {
                return Err(arity("AcousticRealization", 1, field_count(fields)));
            };
            return Ok(AcousticRealization::AbsoluteHz(TextValue::parse(c)?));
        }
        Err(TextError::UnknownConstructor {
            type_name: "AcousticRealization",
            found: head.to_owned(),
        })
    }
}

/// The staff position a spelling draws on: `(cmn <nominal>)`, `(integer <i>)`, or
/// `(registered <id>)`. Mirrors `SpellingNominal`'s `Codec::enc`.
impl TextValue for SpellingNominal {
    fn project(&self) -> Sexp {
        match self {
            SpellingNominal::Cmn(n) => variant("Cmn", vec![n.project()]),
            SpellingNominal::Integer(i) => variant("Integer", vec![i.project()]),
            SpellingNominal::Registered(id) => variant("Registered", vec![id.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("Cmn") {
            let Some([n]) = fields else {
                return Err(arity("SpellingNominal", 1, field_count(fields)));
            };
            return Ok(SpellingNominal::Cmn(TextValue::parse(n)?));
        }
        if head == kebab("Integer") {
            let Some([i]) = fields else {
                return Err(arity("SpellingNominal", 1, field_count(fields)));
            };
            return Ok(SpellingNominal::Integer(TextValue::parse(i)?));
        }
        if head == kebab("Registered") {
            let Some([id]) = fields else {
                return Err(arity("SpellingNominal", 1, field_count(fields)));
            };
            return Ok(SpellingNominal::Registered(TextValue::parse(id)?));
        }
        Err(TextError::UnknownConstructor {
            type_name: "SpellingNominal",
            found: head.to_owned(),
        })
    }
}

/// The provenance of a spelling attachment. Mirrors `SpellingSource`'s
/// `Codec::enc` variant order: `UserChosen`, `Inferred`, `Imported`,
/// `Propagated`, `Analytical`. (That order — with `Inferred` before `Imported` —
/// is the type's declaration order and differs from
/// [`SpellingSourceKind`](crate::pitch::SpellingSourceKind)'s discriminant order;
/// the text carries variant *names*, not tags, so only the names matter here.)
/// The single-field variants carry their named field positionally.
impl TextValue for SpellingSource {
    fn project(&self) -> Sexp {
        match self {
            SpellingSource::UserChosen => variant("UserChosen", vec![]),
            SpellingSource::Inferred => variant("Inferred", vec![]),
            SpellingSource::Imported { format } => variant("Imported", vec![format.project()]),
            SpellingSource::Propagated { from } => variant("Propagated", vec![from.project()]),
            SpellingSource::Analytical => variant("Analytical", vec![]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("UserChosen") {
            expect_fieldless(fields)?;
            return Ok(SpellingSource::UserChosen);
        }
        if head == kebab("Inferred") {
            expect_fieldless(fields)?;
            return Ok(SpellingSource::Inferred);
        }
        if head == kebab("Imported") {
            let Some([format]) = fields else {
                return Err(arity("SpellingSource", 1, field_count(fields)));
            };
            return Ok(SpellingSource::Imported {
                format: TextValue::parse(format)?,
            });
        }
        if head == kebab("Propagated") {
            let Some([from]) = fields else {
                return Err(arity("SpellingSource", 1, field_count(fields)));
            };
            return Ok(SpellingSource::Propagated {
                from: TextValue::parse(from)?,
            });
        }
        if head == kebab("Analytical") {
            expect_fieldless(fields)?;
            return Ok(SpellingSource::Analytical);
        }
        Err(TextError::UnknownConstructor {
            type_name: "SpellingSource",
            found: head.to_owned(),
        })
    }
}

/// A voice selector: `all`, or `(voices <voice-id>…)`. Mirrors `VoiceSelector`'s
/// `Codec::enc`. Implemented here — though it is not one of the "spelling" types —
/// because it is a `pitch.rs` composite with a hand-written `Codec` (the macros
/// generate no [`TextValue`] for it) and [`SpellingScope::Range`] embeds it.
impl TextValue for VoiceSelector {
    fn project(&self) -> Sexp {
        match self {
            VoiceSelector::All => variant("All", vec![]),
            VoiceSelector::Voices(voices) => variant("Voices", vec![voices.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("All") {
            expect_fieldless(fields)?;
            return Ok(VoiceSelector::All);
        }
        if head == kebab("Voices") {
            let Some([voices]) = fields else {
                return Err(arity("VoiceSelector", 1, field_count(fields)));
            };
            return Ok(VoiceSelector::Voices(TextValue::parse(voices)?));
        }
        Err(TextError::UnknownConstructor {
            type_name: "VoiceSelector",
            found: head.to_owned(),
        })
    }
}

/// What a spelling attachment applies to: `(pitch <id>)`, or `(range <start>
/// <end> <voices>)`. Mirrors `SpellingScope`'s `Codec::enc`, whose `Range` fields
/// are start, end, voices in that order.
impl TextValue for SpellingScope {
    fn project(&self) -> Sexp {
        match self {
            SpellingScope::Pitch(id) => variant("Pitch", vec![id.project()]),
            SpellingScope::Range { start, end, voices } => variant(
                "Range",
                vec![start.project(), end.project(), voices.project()],
            ),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("Pitch") {
            let Some([id]) = fields else {
                return Err(arity("SpellingScope", 1, field_count(fields)));
            };
            return Ok(SpellingScope::Pitch(TextValue::parse(id)?));
        }
        if head == kebab("Range") {
            let Some([start, end, voices]) = fields else {
                return Err(arity("SpellingScope", 3, field_count(fields)));
            };
            return Ok(SpellingScope::Range {
                start: TextValue::parse(start)?,
                end: TextValue::parse(end)?,
                voices: TextValue::parse(voices)?,
            });
        }
        Err(TextError::UnknownConstructor {
            type_name: "SpellingScope",
            found: head.to_owned(),
        })
    }
}

/// A spelling directive: `(explicit <pitch-spelling>)`, or `(rule <spelling-rule>)`.
/// Mirrors `SpellingDirective`'s `Codec::enc`. Both payloads are `struct_codec!`
/// composites whose own [`TextValue`] is macro-generated.
impl TextValue for SpellingDirective {
    fn project(&self) -> Sexp {
        match self {
            SpellingDirective::Explicit(spelling) => variant("Explicit", vec![spelling.project()]),
            SpellingDirective::Rule(rule) => variant("Rule", vec![rule.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (head, fields) = constructor(s)?;
        if head == kebab("Explicit") {
            let Some([spelling]) = fields else {
                return Err(arity("SpellingDirective", 1, field_count(fields)));
            };
            return Ok(SpellingDirective::Explicit(TextValue::parse(spelling)?));
        }
        if head == kebab("Rule") {
            let Some([rule]) = fields else {
                return Err(arity("SpellingDirective", 1, field_count(fields)));
            };
            return Ok(SpellingDirective::Rule(TextValue::parse(rule)?));
        }
        Err(TextError::UnknownConstructor {
            type_name: "SpellingDirective",
            found: head.to_owned(),
        })
    }
}

// ===========================================================================
// Structs with a private, validated field.
// ===========================================================================

/// `(reference-pitch <position> <frequency>)`, the frequency a [`CanonicalF64`]'s
/// eight bytes, mirroring `ReferencePitch`'s `Codec::enc` (position then
/// frequency).
///
/// The frequency field is private and reachable only through
/// [`ReferencePitch::new`], which *validates*: it rejects a non-positive or
/// non-finite frequency (Chapter 4: "positive and finite"). Parsing routes
/// through it — there is no other constructor — so a byte-legal frequency that is
/// negative or zero is refused rather than stored. `new` does not normalize, so a
/// value it accepts already projects back verbatim and a whole-value guard could
/// never fire — the `None` is the whole of the strictness.
impl TextValue for ReferencePitch {
    fn project(&self) -> Sexp {
        // `new` guaranteed a finite frequency, so re-wrapping cannot fail — the
        // same invariant `ReferencePitch`'s `Codec::enc` asserts.
        let hz = CanonicalF64::new(self.frequency_hz())
            .expect("a constructed ReferencePitch has a finite frequency");
        Sexp::List(vec![
            Sexp::Symbol(kebab("ReferencePitch")),
            self.position.project(),
            hz.project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("ReferencePitch"), 2)?;
        let [position, frequency] = fields else {
            return Err(arity("ReferencePitch", 2, fields.len()));
        };
        let position = PitchSpacePosition::parse(position)?;
        let frequency: CanonicalF64 = TextValue::parse(frequency)?;
        // `new` *validates* — it refuses a non-positive or non-finite frequency —
        // and never adjusts one, so an accepted value re-projects to exactly its
        // input. A whole-value guard here could not fire; the `None` is the whole
        // of the strictness.
        ReferencePitch::new(position, frequency.get()).ok_or(TextError::NotCanonical(
            "a reference pitch frequency must be positive and finite",
        ))
    }
}

/// `(spelling-precedence <order>)`, `order` the total ranking of source kinds,
/// highest precedence first. Mirrors `SpellingPrecedence`'s `Codec::enc`, which
/// writes the single `order` vector.
///
/// The order is private and reachable only through [`SpellingPrecedence::new`],
/// which *validates*: it rejects any order that is not a total ranking — the
/// wrong length, or a source kind missing or duplicated. A `Vec` parse preserves
/// order and cannot see a duplicate itself, so `new` is what refuses it, and
/// parsing routes through `new` rather than laundering a malformed order into a
/// value (`req:textproj:strict-parse`). Any *permutation* of the five kinds is a
/// distinct, legitimate value, so `new` never reorders — it only accepts or
/// rejects, and there is nothing left for a whole-value guard to catch.
impl TextValue for SpellingPrecedence {
    fn project(&self) -> Sexp {
        let order = Sexp::List(self.order_ref().iter().map(TextValue::project).collect());
        Sexp::List(vec![Sexp::Symbol(kebab("SpellingPrecedence")), order])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("SpellingPrecedence"), 1)?;
        let [order] = fields else {
            return Err(arity("SpellingPrecedence", 1, fields.len()));
        };
        let order: Vec<SpellingSourceKind> = TextValue::parse(order)?;
        // `new` accepts only a permutation of the five source kinds and stores it
        // unchanged: every permutation is a distinct legitimate value, so it never
        // reorders. Validation, not normalization — the `None` rejects a duplicate
        // or a short list, and nothing downstream could catch what it misses.
        SpellingPrecedence::new(order).ok_or(TextError::NotCanonical(
            "spelling precedence must rank every source kind exactly once",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{PitchId, ReplicaId, VoiceId};
    use crate::pitch::{
        CmnNominal, ForeignFormatId, NominalRegistryId, PitchSpelling, PositionRegistryId,
        SpellingRule, SpellingRuleSetId, TuningSystemId,
    };
    use crate::textvalue::read_sexp;
    use crate::time::{TimeAnchor, WallClockTime};

    /// Build a value, project it, render, read the text back, parse, and require
    /// equality — the full `project → render → read_sexp → parse` loop.
    #[track_caller]
    fn round_trip<T: TextValue + PartialEq + std::fmt::Debug>(value: T) {
        let text = value.project().render();
        let sexp = read_sexp(&text).unwrap_or_else(|e| panic!("read_sexp rejected {text:?}: {e}"));
        let back = T::parse(&sexp).unwrap_or_else(|e| panic!("parse rejected {text:?}: {e}"));
        assert_eq!(value, back, "{text:?} did not round-trip");
    }

    #[test]
    fn pitch_space_position_round_trips_every_variant() {
        round_trip(PitchSpacePosition::Cmn {
            nominal: CmnNominal::A,
            alteration: -1,
            octave: 4,
        });
        round_trip(PitchSpacePosition::Integer {
            space_size: 31,
            index: -5,
        });
        round_trip(PitchSpacePosition::JiVector {
            components: vec![1, -2, 3],
        });
        round_trip(PitchSpacePosition::Registered(PositionRegistryId::new(
            "my-pos",
        )));
        // The Cmn fields project positionally in declaration order.
        assert_eq!(
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: 0,
                octave: 4,
            }
            .project()
            .render(),
            "(cmn c 0 4)"
        );
    }

    #[test]
    fn tuning_reference_round_trips() {
        round_trip(TuningReference::Inherit);
        round_trip(TuningReference::Explicit(TuningSystemId::new("tet-12")));
        assert_eq!(TuningReference::Inherit.project().render(), "inherit");
    }

    #[test]
    fn acoustic_realization_round_trips() {
        round_trip(AcousticRealization::Implicit);
        round_trip(AcousticRealization::cents_offset(3.5).unwrap());
        round_trip(AcousticRealization::absolute_hz(440.0).unwrap());
    }

    #[test]
    fn reference_pitch_round_trips() {
        round_trip(ReferencePitch::a440());
        round_trip(
            ReferencePitch::new(
                PitchSpacePosition::Cmn {
                    nominal: CmnNominal::A,
                    alteration: 0,
                    octave: 4,
                },
                442.0,
            )
            .unwrap(),
        );
    }

    /// A negative or zero frequency is a valid `CanonicalF64` byte string, so the
    /// lexer and leaf parse accept it; only `ReferencePitch::new`'s validation
    /// rejects it — which is the point, the text is the projection of no reference
    /// pitch, and must not be laundered into one.
    #[test]
    fn a_non_positive_reference_frequency_is_rejected_not_accepted() {
        let position = PitchSpacePosition::Cmn {
            nominal: CmnNominal::A,
            alteration: 0,
            octave: 4,
        };
        for bad_hz in [-440.0, 0.0] {
            let sexp = Sexp::List(vec![
                Sexp::Symbol(kebab("ReferencePitch")),
                position.project(),
                CanonicalF64::new(bad_hz).unwrap().project(),
            ]);
            let text = sexp.render();
            let read = read_sexp(&text).unwrap();
            assert!(
                ReferencePitch::parse(&read).is_err(),
                "{text} must be rejected"
            );
        }
    }

    #[test]
    fn spelling_nominal_round_trips() {
        round_trip(SpellingNominal::Cmn(CmnNominal::G));
        round_trip(SpellingNominal::Integer(7));
        round_trip(SpellingNominal::Registered(NominalRegistryId::new("nom")));
    }

    #[test]
    fn spelling_source_round_trips() {
        let r = ReplicaId::SYSTEM_DERIVED;
        round_trip(SpellingSource::UserChosen);
        round_trip(SpellingSource::Inferred);
        round_trip(SpellingSource::Imported {
            format: ForeignFormatId::new("musicxml"),
        });
        round_trip(SpellingSource::Propagated {
            from: PitchId::new(r, 7),
        });
        round_trip(SpellingSource::Analytical);
    }

    /// A fieldless variant projects as a bare symbol, and a variant with fields as
    /// a list; the wrong shape is refused rather than accepted.
    #[test]
    fn a_source_of_the_wrong_shape_is_rejected() {
        // `user-chosen` is fieldless; `(user-chosen)` is a different text.
        let listed = read_sexp("(user-chosen)").unwrap();
        assert!(SpellingSource::parse(&listed).is_err());
        // `imported` carries a field; the bare symbol is missing it.
        let bare = read_sexp("imported").unwrap();
        assert!(SpellingSource::parse(&bare).is_err());
    }

    #[test]
    fn voice_selector_round_trips() {
        let r = ReplicaId::SYSTEM_DERIVED;
        round_trip(VoiceSelector::All);
        round_trip(VoiceSelector::Voices(vec![
            VoiceId::new(r, 1),
            VoiceId::new(r, 2),
        ]));
    }

    #[test]
    fn spelling_scope_round_trips() {
        let r = ReplicaId::SYSTEM_DERIVED;
        round_trip(SpellingScope::Pitch(PitchId::new(r, 3)));
        round_trip(SpellingScope::Range {
            start: TimeAnchor::WallClock {
                time: WallClockTime(0),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(480),
            },
            voices: VoiceSelector::All,
        });
    }

    #[test]
    fn spelling_directive_round_trips() {
        round_trip(SpellingDirective::Explicit(PitchSpelling::cmn(
            CmnNominal::C,
            4,
        )));
        round_trip(SpellingDirective::Rule(SpellingRule {
            rule_set: SpellingRuleSetId::new("rs"),
        }));
    }

    #[test]
    fn spelling_precedence_round_trips() {
        round_trip(SpellingPrecedence::default());
        // Any permutation is a distinct, legitimate value.
        round_trip(
            SpellingPrecedence::new(vec![
                SpellingSourceKind::Analytical,
                SpellingSourceKind::Inferred,
                SpellingSourceKind::Propagated,
                SpellingSourceKind::Imported,
                SpellingSourceKind::UserChosen,
            ])
            .unwrap(),
        );
        assert_eq!(
            SpellingPrecedence::default().project().render(),
            "(spelling-precedence (user-chosen imported propagated inferred analytical))"
        );
    }

    /// A `Vec` parse preserves order, so a duplicated or missing source kind is
    /// invisible to it; only `SpellingPrecedence::new` catches it. Parsing must
    /// reject, never silently repair, such an order.
    #[test]
    fn a_precedence_missing_or_duplicating_a_source_kind_is_rejected() {
        let duplicated = read_sexp(
            "(spelling-precedence (user-chosen user-chosen propagated inferred analytical))",
        )
        .unwrap();
        assert!(SpellingPrecedence::parse(&duplicated).is_err());

        let missing =
            read_sexp("(spelling-precedence (user-chosen imported propagated inferred))").unwrap();
        assert!(SpellingPrecedence::parse(&missing).is_err());
    }
}
