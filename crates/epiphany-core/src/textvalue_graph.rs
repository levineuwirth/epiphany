//! [`TextValue`] for the Chapter-5 graph types whose binary [`Codec`] is
//! hand-written rather than macro-generated.
//!
//! The macro families in `codec.rs` (`struct_codec!`, `cstyle_enum_codec!`,
//! `unit_codec!`, `catalog_id_codec!`) emit a `TextValue` alongside every binary
//! codec, so those types cannot drift. This module is for the graph types whose
//! `impl Codec` is spelled out by hand — tagged unions, structs with private
//! fields and validating constructors, and two newtypes that need a byte string
//! rather than the transparent field. Each impl below **mirrors the field and
//! variant order of the matching `fn enc` in `codec.rs`** (that order is the
//! ratified declaration order), so the projection cannot diverge from the wire.
//!
//! Every `parse` obeys `req:textproj:strict-parse`: it rejects text that is not
//! the canonical projection of the value it denotes rather than normalizing it.
//! A fieldless variant is *only* its bare symbol, so its list spelling is
//! rejected; a validating constructor's rejection is surfaced, not swallowed; and
//! where a constructor could launder non-canonical input into a canonical value
//! ([`EventOrderingDAG::try_new`]), the result is re-projected and compared with
//! [`ensure_canonical`].
//!
//! [`Codec`]: crate::codec::Codec

use std::collections::BTreeMap;

use epiphany_determinism::CanonicalF64;

use crate::graph::{
    AnnotationAnchor, DecompositionSource, EventOrderingDAG, GestureAnchoring, KeySignature,
    MetadataValue, RegionContent, RegionTimeModel, RepeatKind, ScoreTuningContext,
    SoundConfiguration, SpaceUnit, SpannerKind, StaffGroupKind, TieClass, TimeSignature,
    TimeSignatureDisplay, Timestamp, TupletRatio, VoiceOrigin,
};
use crate::textvalue::{kebab, Sexp, TextError, TextValue};
use crate::textvalue_impls::class_of;

// ===========================================================================
// Tagged-union helpers.
// ===========================================================================
//
// `req:textproj:value-projection` clause 3: a tagged-union variant is
// `(<variant> <field>…)`, and a variant with no fields is the bare symbol
// `<variant>`. These helpers give every hand-written union one strict reading of
// that rule.

/// Projects a tagged-union variant: the bare symbol `<variant>` when it has no
/// fields, `(<variant> <field>…)` when it has.
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

/// Splits a tagged-union projection into its constructor name and, when it is a
/// list, the fields after the head.
///
/// A bare symbol yields `None` fields; a list yields `Some(fields)`. Keeping the
/// two forms apart is what makes the parse strict (`req:textproj:strict-parse`):
/// a fieldless variant projects to a bare symbol, so a caller can reject its list
/// spelling `(volta)` instead of silently reading it as `volta`, and a
/// field-bearing variant can reject a bare-symbol spelling.
fn split_variant(s: &Sexp) -> Result<(&str, Option<&[Sexp]>), TextError> {
    match s {
        Sexp::Symbol(name) => Ok((name.as_str(), None)),
        Sexp::List(items) => {
            let head = items
                .first()
                .and_then(Sexp::as_symbol)
                .ok_or(TextError::Syntax(
                    "a variant is a bare symbol or a list headed by its constructor",
                ))?;
            Ok((head, Some(&items[1..])))
        }
        _ => Err(TextError::Expected {
            expected: "variant",
            found: class_of(s),
        }),
    }
}

/// Confirms a fieldless variant was spelled as its bare symbol, not `(name)`.
/// Accepting the list spelling would fold two texts onto one value.
fn no_fields(fields: Option<&[Sexp]>) -> Result<(), TextError> {
    match fields {
        None => Ok(()),
        Some(_) => Err(TextError::NotCanonical(
            "a fieldless variant is a bare symbol, not a list",
        )),
    }
}

/// Confirms a variant with `arity` fields was spelled as a list of exactly that
/// many fields, and returns them. A bare-symbol spelling of a field-bearing
/// variant is rejected here.
fn fields_of<'a>(
    fields: Option<&'a [Sexp]>,
    type_name: &'static str,
    arity: usize,
) -> Result<&'a [Sexp], TextError> {
    let fields = fields.ok_or(TextError::Expected {
        expected: "variant with fields",
        found: "symbol",
    })?;
    if fields.len() != arity {
        return Err(TextError::Arity {
            type_name,
            expected: arity,
            found: fields.len(),
        });
    }
    Ok(fields)
}

// ===========================================================================
// Newtypes.
// ===========================================================================

/// A [`Timestamp`] is its wrapped `i64` alone (`req:textproj:value-projection`
/// clause 2): the newtype adds no bytes to the wire and no wrapper to the text.
impl TextValue for Timestamp {
    fn project(&self) -> Sexp {
        TextValue::project(&self.0)
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        <i64 as TextValue>::parse(s).map(Timestamp)
    }
}

/// A [`SpaceUnit`] is its wrapped [`CanonicalF64`] alone (clause 2). The float
/// leaf is a byte string of its eight canonical IEEE-754 bytes, never a decimal —
/// see the `CanonicalF64` impl in `textvalue_impls.rs`.
impl TextValue for SpaceUnit {
    fn project(&self) -> Sexp {
        TextValue::project(&self.0)
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        <CanonicalF64 as TextValue>::parse(s).map(SpaceUnit)
    }
}

/// A [`SoundConfiguration`] projects as a **byte string**, not as a list of
/// integers.
///
/// It wraps a `Vec<u8>`, and the generic `Vec<u8>` impl would render each byte as
/// a decimal integer inside a list. But the binary form writes this as one
/// length-prefixed opaque run, and `spec/text_projection.tex`
/// (`req:textproj:value-projection`) names `SoundConfiguration` explicitly: "the
/// projection writes a byte string, not a list of integers." The core never
/// interprets the bytes, so any `Vec<u8>` is a canonical value and the strict
/// byte-string reader already rejects a non-canonical spelling; there is nothing
/// to normalize, so `parse` is a direct read.
impl TextValue for SoundConfiguration {
    fn project(&self) -> Sexp {
        Sexp::Bytes(self.0.clone())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s {
            Sexp::Bytes(bytes) => Ok(SoundConfiguration(bytes.clone())),
            _ => Err(TextError::Expected {
                expected: "SoundConfiguration byte string",
                found: class_of(s),
            }),
        }
    }
}

// ===========================================================================
// Structs with private fields / validating constructors.
// ===========================================================================

/// `(key-signature <fifths>)` — a struct with one named `i8` field
/// (`req:textproj:value-projection` clause 1).
///
/// The field is private and the only constructor, [`KeySignature::new`],
/// *validates* the `-7..=7` circle-of-fifths range. It validates by **rejecting**,
/// never by normalizing, so a `None` from `new` is surfaced as a rejection and an
/// accepted value re-projects to exactly its input, so a whole-value guard here
/// could never fire.
impl TextValue for KeySignature {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::Symbol(kebab("KeySignature")),
            TextValue::project(&self.fifths()),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("KeySignature"), 1)?;
        let fifths = <i8 as TextValue>::parse(&fields[0])?;
        KeySignature::new(fifths).ok_or(TextError::NotCanonical(
            "key-signature fifths outside the -7..=7 range",
        ))
    }
}

/// `(tuplet-ratio <actual> <notated>)` — a struct with two private `u32` fields,
/// in `fn enc` order (`actual` then `notated`).
///
/// [`TupletRatio::new`] rejects a *degenerate* ratio (a zero term, or
/// `actual == notated`); it does not reduce (`6:4` stays `6:4`, a value distinct
/// from `3:2` on the wire), so it never normalizes. The rejection is surfaced; an
/// accepted ratio re-projects to its input, so no guard is needed.
impl TextValue for TupletRatio {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::Symbol(kebab("TupletRatio")),
            TextValue::project(&self.actual()),
            TextValue::project(&self.notated()),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("TupletRatio"), 2)?;
        let actual = <u32 as TextValue>::parse(&fields[0])?;
        let notated = <u32 as TextValue>::parse(&fields[1])?;
        TupletRatio::new(actual, notated).ok_or(TextError::NotCanonical(
            "a tuplet ratio needs nonzero terms and actual != notated",
        ))
    }
}

/// `(event-ordering-dag <adjacency-map>)` — a struct with one private field, the
/// `BTreeMap<EventId, Vec<EventId>>` the codec writes via `edges_ref()`.
///
/// The only constructor that can build a non-empty DAG,
/// [`EventOrderingDAG::try_new`], **validates acyclicity** and stores the map as
/// given. It rejects rather than adjusts, so an accepted value re-projects to
/// exactly its input.
///
/// Note what that means this does *not* reject: a repeated successor in an
/// adjacency list. `try_new` admits it, and so does the binary form — the
/// successor list is an order-preserving `Vec`, so `[a, a]` and `[a]` are distinct
/// values there too. The projection is faithful to that, not lenient.
impl TextValue for EventOrderingDAG {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::Symbol(kebab("EventOrderingDAG")),
            TextValue::project(self.edges_ref()),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("EventOrderingDAG"), 1)?;
        let edges: BTreeMap<crate::ids::EventId, Vec<crate::ids::EventId>> =
            TextValue::parse(&fields[0])?;
        // `try_new` checks acyclicity and stores the map as given — validation,
        // not normalization — so an accepted value re-projects to its input.
        // Note what this therefore does *not* reject: a repeated successor in an
        // adjacency list. That is a distinct value in the binary form too (the
        // successor list is an order-preserving `Vec`), so the projection is
        // faithful, not lenient.
        EventOrderingDAG::try_new(edges)
            .ok_or(TextError::NotCanonical("event ordering contains a cycle"))
    }
}

/// `(time-signature <id> <display> <measure-duration> <beat-groups>)` — a struct
/// in `fn enc` order: `id`, `display`, then the two private fields
/// `measure_duration` and the `beat_groups` vector.
///
/// [`TimeSignature::new`] enforces the Chapter-3 MUST that the beat-group
/// durations sum to `measure_duration`, rejecting on mismatch. That is validation
/// by rejection, not normalization — an accepted signature stores its fields
/// verbatim and re-projects to its input — so the `None` is surfaced and no guard
/// is needed.
impl TextValue for TimeSignature {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::Symbol(kebab("TimeSignature")),
            TextValue::project(&self.id),
            TextValue::project(&self.display),
            TextValue::project(self.measure_duration()),
            Sexp::List(self.beat_groups().iter().map(TextValue::project).collect()),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("TimeSignature"), 4)?;
        let id = TextValue::parse(&fields[0])?;
        let display = TextValue::parse(&fields[1])?;
        let measure_duration = TextValue::parse(&fields[2])?;
        let beat_groups = TextValue::parse(&fields[3])?;
        TimeSignature::new(id, display, measure_duration, beat_groups).ok_or(
            TextError::NotCanonical("beat groups do not sum to the measure duration"),
        )
    }
}

// ===========================================================================
// A struct whose `Codec` is hand-written for a macro-incompatibility reason,
// not a validating constructor.
// ===========================================================================

/// `(score-tuning-context <default-pitch-space> <default-tuning-system>
/// <reference>)` — exactly the three wire fields, in `fn enc` order
/// (Push 4b tranche 2, `spec/CONTRACT_PUSH4B_RESOLVER.md`).
///
/// `ScoreTuningContext` gained a fourth field, `overrides`, that is
/// deliberately **not** part of this projection: it is in-memory only (no
/// schema major 3 has been opened), so it must never reach the wire, and the
/// text projection is the same canonical surface the binary codec is — a
/// value that omits it here would otherwise silently launder a
/// non-empty-`overrides` context into one indistinguishable from an
/// empty-`overrides` context, which is exactly the intended behavior, not an
/// oversight. `parse` always constructs `overrides: Vec::new()`, mirroring
/// `Codec::dec`.
impl TextValue for ScoreTuningContext {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::Symbol(kebab("ScoreTuningContext")),
            self.default_pitch_space.project(),
            self.default_tuning_system.project(),
            self.reference.project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct(&kebab("ScoreTuningContext"), 3)?;
        let default_pitch_space = TextValue::parse(&fields[0])?;
        let default_tuning_system = TextValue::parse(&fields[1])?;
        let reference = TextValue::parse(&fields[2])?;
        Ok(ScoreTuningContext {
            default_pitch_space,
            default_tuning_system,
            reference,
            overrides: Vec::new(),
        })
    }
}

// ===========================================================================
// Tagged unions.
// ===========================================================================

/// The `SpannerKind` variants, in `fn enc` tag order 0..=8. Each carries its
/// payload positionally.
impl TextValue for SpannerKind {
    fn project(&self) -> Sexp {
        match self {
            SpannerKind::Generic => variant("Generic", vec![]),
            SpannerKind::Hairpin(d) => variant("Hairpin", vec![d.project()]),
            SpannerKind::OctaveLine(o) => variant("OctaveLine", vec![o.project()]),
            SpannerKind::PedalLine(p) => variant("PedalLine", vec![p.project()]),
            SpannerKind::TrillExtension => variant("TrillExtension", vec![]),
            SpannerKind::Glissando => variant("Glissando", vec![]),
            SpannerKind::Portamento => variant("Portamento", vec![]),
            SpannerKind::TextLine(t) => variant("TextLine", vec![t.project()]),
            SpannerKind::Bracket(b) => variant("Bracket", vec![b.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("Generic") {
            no_fields(fields)?;
            Ok(SpannerKind::Generic)
        } else if ctor == kebab("Hairpin") {
            let f = fields_of(fields, "SpannerKind", 1)?;
            Ok(SpannerKind::Hairpin(TextValue::parse(&f[0])?))
        } else if ctor == kebab("OctaveLine") {
            let f = fields_of(fields, "SpannerKind", 1)?;
            Ok(SpannerKind::OctaveLine(TextValue::parse(&f[0])?))
        } else if ctor == kebab("PedalLine") {
            let f = fields_of(fields, "SpannerKind", 1)?;
            Ok(SpannerKind::PedalLine(TextValue::parse(&f[0])?))
        } else if ctor == kebab("TrillExtension") {
            no_fields(fields)?;
            Ok(SpannerKind::TrillExtension)
        } else if ctor == kebab("Glissando") {
            no_fields(fields)?;
            Ok(SpannerKind::Glissando)
        } else if ctor == kebab("Portamento") {
            no_fields(fields)?;
            Ok(SpannerKind::Portamento)
        } else if ctor == kebab("TextLine") {
            let f = fields_of(fields, "SpannerKind", 1)?;
            Ok(SpannerKind::TextLine(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Bracket") {
            let f = fields_of(fields, "SpannerKind", 1)?;
            Ok(SpannerKind::Bracket(TextValue::parse(&f[0])?))
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "SpannerKind",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `RepeatKind` variants, in `fn enc` tag order 0..=3. `DalSegno` carries
/// `segno` then `end_target`, matching the declaration order the codec writes.
impl TextValue for RepeatKind {
    fn project(&self) -> Sexp {
        match self {
            RepeatKind::SimpleRepeat { count } => variant("SimpleRepeat", vec![count.project()]),
            RepeatKind::DaCapo { end_target } => variant("DaCapo", vec![end_target.project()]),
            RepeatKind::DalSegno { segno, end_target } => {
                variant("DalSegno", vec![segno.project(), end_target.project()])
            }
            RepeatKind::Volta => variant("Volta", vec![]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("SimpleRepeat") {
            let f = fields_of(fields, "RepeatKind", 1)?;
            Ok(RepeatKind::SimpleRepeat {
                count: TextValue::parse(&f[0])?,
            })
        } else if ctor == kebab("DaCapo") {
            let f = fields_of(fields, "RepeatKind", 1)?;
            Ok(RepeatKind::DaCapo {
                end_target: TextValue::parse(&f[0])?,
            })
        } else if ctor == kebab("DalSegno") {
            let f = fields_of(fields, "RepeatKind", 2)?;
            Ok(RepeatKind::DalSegno {
                segno: TextValue::parse(&f[0])?,
                end_target: TextValue::parse(&f[1])?,
            })
        } else if ctor == kebab("Volta") {
            no_fields(fields)?;
            Ok(RepeatKind::Volta)
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "RepeatKind",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `MetadataValue` variants, in `fn enc` tag order: `Text` (0), `Integer`
/// (1), `Flag` (2).
impl TextValue for MetadataValue {
    fn project(&self) -> Sexp {
        match self {
            MetadataValue::Text(s) => variant("Text", vec![s.project()]),
            MetadataValue::Integer(i) => variant("Integer", vec![i.project()]),
            MetadataValue::Flag(b) => variant("Flag", vec![b.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("Text") {
            let f = fields_of(fields, "MetadataValue", 1)?;
            Ok(MetadataValue::Text(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Integer") {
            let f = fields_of(fields, "MetadataValue", 1)?;
            Ok(MetadataValue::Integer(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Flag") {
            let f = fields_of(fields, "MetadataValue", 1)?;
            Ok(MetadataValue::Flag(TextValue::parse(&f[0])?))
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "MetadataValue",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `RegionTimeModel` variants, in `fn enc` tag order: `Metric` (0),
/// `Proportional` (1), `Aleatoric` (2).
impl TextValue for RegionTimeModel {
    fn project(&self) -> Sexp {
        match self {
            RegionTimeModel::Metric(m) => variant("Metric", vec![m.project()]),
            RegionTimeModel::Proportional(p) => variant("Proportional", vec![p.project()]),
            RegionTimeModel::Aleatoric(a) => variant("Aleatoric", vec![a.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("Metric") {
            let f = fields_of(fields, "RegionTimeModel", 1)?;
            Ok(RegionTimeModel::Metric(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Proportional") {
            let f = fields_of(fields, "RegionTimeModel", 1)?;
            Ok(RegionTimeModel::Proportional(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Aleatoric") {
            let f = fields_of(fields, "RegionTimeModel", 1)?;
            Ok(RegionTimeModel::Aleatoric(TextValue::parse(&f[0])?))
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "RegionTimeModel",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `RegionContent` variants, in `fn enc` tag order: `StaffBased` (0),
/// `FreeGraphic` (1), `Hybrid` (2). `Hybrid` carries `staves`, `overlay`,
/// `overlay_below_staves` in that order.
impl TextValue for RegionContent {
    fn project(&self) -> Sexp {
        match self {
            RegionContent::StaffBased(c) => variant("StaffBased", vec![c.project()]),
            RegionContent::FreeGraphic(g) => variant("FreeGraphic", vec![g.project()]),
            RegionContent::Hybrid {
                staves,
                overlay,
                overlay_below_staves,
            } => variant(
                "Hybrid",
                vec![
                    staves.project(),
                    overlay.project(),
                    overlay_below_staves.project(),
                ],
            ),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("StaffBased") {
            let f = fields_of(fields, "RegionContent", 1)?;
            Ok(RegionContent::StaffBased(TextValue::parse(&f[0])?))
        } else if ctor == kebab("FreeGraphic") {
            let f = fields_of(fields, "RegionContent", 1)?;
            Ok(RegionContent::FreeGraphic(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Hybrid") {
            let f = fields_of(fields, "RegionContent", 3)?;
            Ok(RegionContent::Hybrid {
                staves: TextValue::parse(&f[0])?,
                overlay: TextValue::parse(&f[1])?,
                overlay_below_staves: TextValue::parse(&f[2])?,
            })
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "RegionContent",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `VoiceOrigin` variants, in `fn enc` tag order: `UserDeclared` (0),
/// `Imported` (1), `SystemPromoted` (2). `SystemPromoted` carries
/// `winning_operation`, `losing_operation`, `original_voice` in that order.
impl TextValue for VoiceOrigin {
    fn project(&self) -> Sexp {
        match self {
            VoiceOrigin::UserDeclared => variant("UserDeclared", vec![]),
            VoiceOrigin::Imported { format } => variant("Imported", vec![format.project()]),
            VoiceOrigin::SystemPromoted {
                winning_operation,
                losing_operation,
                original_voice,
            } => variant(
                "SystemPromoted",
                vec![
                    winning_operation.project(),
                    losing_operation.project(),
                    original_voice.project(),
                ],
            ),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("UserDeclared") {
            no_fields(fields)?;
            Ok(VoiceOrigin::UserDeclared)
        } else if ctor == kebab("Imported") {
            let f = fields_of(fields, "VoiceOrigin", 1)?;
            Ok(VoiceOrigin::Imported {
                format: TextValue::parse(&f[0])?,
            })
        } else if ctor == kebab("SystemPromoted") {
            let f = fields_of(fields, "VoiceOrigin", 3)?;
            Ok(VoiceOrigin::SystemPromoted {
                winning_operation: TextValue::parse(&f[0])?,
                losing_operation: TextValue::parse(&f[1])?,
                original_voice: TextValue::parse(&f[2])?,
            })
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "VoiceOrigin",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `StaffGroupKind` variants, in `fn enc` tag order 0..=4. The first four are
/// fieldless; `Registered` (4) carries a registry id.
impl TextValue for StaffGroupKind {
    fn project(&self) -> Sexp {
        match self {
            StaffGroupKind::GrandStaff => variant("GrandStaff", vec![]),
            StaffGroupKind::Bracket => variant("Bracket", vec![]),
            StaffGroupKind::SubBracket => variant("SubBracket", vec![]),
            StaffGroupKind::Choral => variant("Choral", vec![]),
            StaffGroupKind::Registered(id) => variant("Registered", vec![id.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("GrandStaff") {
            no_fields(fields)?;
            Ok(StaffGroupKind::GrandStaff)
        } else if ctor == kebab("Bracket") {
            no_fields(fields)?;
            Ok(StaffGroupKind::Bracket)
        } else if ctor == kebab("SubBracket") {
            no_fields(fields)?;
            Ok(StaffGroupKind::SubBracket)
        } else if ctor == kebab("Choral") {
            no_fields(fields)?;
            Ok(StaffGroupKind::Choral)
        } else if ctor == kebab("Registered") {
            let f = fields_of(fields, "StaffGroupKind", 1)?;
            Ok(StaffGroupKind::Registered(TextValue::parse(&f[0])?))
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "StaffGroupKind",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `TieClass` variants, in `fn enc` tag order 0..=4. The first four are
/// fieldless; `Registered` (4) carries a registry id.
impl TextValue for TieClass {
    fn project(&self) -> Sexp {
        match self {
            TieClass::Standard => variant("Standard", vec![]),
            TieClass::Editorial => variant("Editorial", vec![]),
            TieClass::CrossVoice => variant("CrossVoice", vec![]),
            TieClass::LaissezVibrer => variant("LaissezVibrer", vec![]),
            TieClass::Registered(id) => variant("Registered", vec![id.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("Standard") {
            no_fields(fields)?;
            Ok(TieClass::Standard)
        } else if ctor == kebab("Editorial") {
            no_fields(fields)?;
            Ok(TieClass::Editorial)
        } else if ctor == kebab("CrossVoice") {
            no_fields(fields)?;
            Ok(TieClass::CrossVoice)
        } else if ctor == kebab("LaissezVibrer") {
            no_fields(fields)?;
            Ok(TieClass::LaissezVibrer)
        } else if ctor == kebab("Registered") {
            let f = fields_of(fields, "TieClass", 1)?;
            Ok(TieClass::Registered(TextValue::parse(&f[0])?))
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "TieClass",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `AnnotationAnchor` variants, in `fn enc` tag order: `Event` (0), `Range`
/// (1), `Region` (2). `Range` carries `start` then `end`.
impl TextValue for AnnotationAnchor {
    fn project(&self) -> Sexp {
        match self {
            AnnotationAnchor::Event(id) => variant("Event", vec![id.project()]),
            AnnotationAnchor::Range { start, end } => {
                variant("Range", vec![start.project(), end.project()])
            }
            AnnotationAnchor::Region(id) => variant("Region", vec![id.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("Event") {
            let f = fields_of(fields, "AnnotationAnchor", 1)?;
            Ok(AnnotationAnchor::Event(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Range") {
            let f = fields_of(fields, "AnnotationAnchor", 2)?;
            Ok(AnnotationAnchor::Range {
                start: TextValue::parse(&f[0])?,
                end: TextValue::parse(&f[1])?,
            })
        } else if ctor == kebab("Region") {
            let f = fields_of(fields, "AnnotationAnchor", 1)?;
            Ok(AnnotationAnchor::Region(TextValue::parse(&f[0])?))
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "AnnotationAnchor",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `GestureAnchoring` variants, in `fn enc` tag order: `Events` (0), `Range`
/// (1), `Free` (2). `Range` carries `start`, `end`, `staves` in that order.
impl TextValue for GestureAnchoring {
    fn project(&self) -> Sexp {
        match self {
            GestureAnchoring::Events(v) => variant("Events", vec![v.project()]),
            GestureAnchoring::Range { start, end, staves } => variant(
                "Range",
                vec![start.project(), end.project(), staves.project()],
            ),
            GestureAnchoring::Free => variant("Free", vec![]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("Events") {
            let f = fields_of(fields, "GestureAnchoring", 1)?;
            Ok(GestureAnchoring::Events(TextValue::parse(&f[0])?))
        } else if ctor == kebab("Range") {
            let f = fields_of(fields, "GestureAnchoring", 3)?;
            Ok(GestureAnchoring::Range {
                start: TextValue::parse(&f[0])?,
                end: TextValue::parse(&f[1])?,
                staves: TextValue::parse(&f[2])?,
            })
        } else if ctor == kebab("Free") {
            no_fields(fields)?;
            Ok(GestureAnchoring::Free)
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "GestureAnchoring",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `DecompositionSource` variants, in `fn enc` tag order: `UserChosen` (0),
/// `Inferred` (1), `Imported` (2), `Propagated` (3).
impl TextValue for DecompositionSource {
    fn project(&self) -> Sexp {
        match self {
            DecompositionSource::UserChosen => variant("UserChosen", vec![]),
            DecompositionSource::Inferred => variant("Inferred", vec![]),
            DecompositionSource::Imported { format } => variant("Imported", vec![format.project()]),
            DecompositionSource::Propagated { from } => variant("Propagated", vec![from.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("UserChosen") {
            no_fields(fields)?;
            Ok(DecompositionSource::UserChosen)
        } else if ctor == kebab("Inferred") {
            no_fields(fields)?;
            Ok(DecompositionSource::Inferred)
        } else if ctor == kebab("Imported") {
            let f = fields_of(fields, "DecompositionSource", 1)?;
            Ok(DecompositionSource::Imported {
                format: TextValue::parse(&f[0])?,
            })
        } else if ctor == kebab("Propagated") {
            let f = fields_of(fields, "DecompositionSource", 1)?;
            Ok(DecompositionSource::Propagated {
                from: TextValue::parse(&f[0])?,
            })
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "DecompositionSource",
                found: ctor.to_owned(),
            })
        }
    }
}

/// The `TimeSignatureDisplay` variants, in `fn enc` tag order 0..=5. `Standard`,
/// `Compound`, and `Irrational` each carry `numerator(s)` then `denominator`;
/// `MixedDenominators` carries its component list; `None` is fieldless;
/// `Symbolic` carries a `u32` id.
impl TextValue for TimeSignatureDisplay {
    fn project(&self) -> Sexp {
        match self {
            TimeSignatureDisplay::Standard {
                numerator,
                denominator,
            } => variant("Standard", vec![numerator.project(), denominator.project()]),
            TimeSignatureDisplay::Compound {
                numerators,
                denominator,
            } => variant(
                "Compound",
                vec![numerators.project(), denominator.project()],
            ),
            TimeSignatureDisplay::Irrational {
                numerator,
                denominator,
            } => variant(
                "Irrational",
                vec![numerator.project(), denominator.project()],
            ),
            TimeSignatureDisplay::MixedDenominators { components } => {
                variant("MixedDenominators", vec![components.project()])
            }
            TimeSignatureDisplay::None => variant("None", vec![]),
            TimeSignatureDisplay::Symbolic(v) => variant("Symbolic", vec![v.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (ctor, fields) = split_variant(s)?;
        if ctor == kebab("Standard") {
            let f = fields_of(fields, "TimeSignatureDisplay", 2)?;
            Ok(TimeSignatureDisplay::Standard {
                numerator: TextValue::parse(&f[0])?,
                denominator: TextValue::parse(&f[1])?,
            })
        } else if ctor == kebab("Compound") {
            let f = fields_of(fields, "TimeSignatureDisplay", 2)?;
            Ok(TimeSignatureDisplay::Compound {
                numerators: TextValue::parse(&f[0])?,
                denominator: TextValue::parse(&f[1])?,
            })
        } else if ctor == kebab("Irrational") {
            let f = fields_of(fields, "TimeSignatureDisplay", 2)?;
            Ok(TimeSignatureDisplay::Irrational {
                numerator: TextValue::parse(&f[0])?,
                denominator: TextValue::parse(&f[1])?,
            })
        } else if ctor == kebab("MixedDenominators") {
            let f = fields_of(fields, "TimeSignatureDisplay", 1)?;
            Ok(TimeSignatureDisplay::MixedDenominators {
                components: TextValue::parse(&f[0])?,
            })
        } else if ctor == kebab("None") {
            no_fields(fields)?;
            Ok(TimeSignatureDisplay::None)
        } else if ctor == kebab("Symbolic") {
            let f = fields_of(fields, "TimeSignatureDisplay", 1)?;
            Ok(TimeSignatureDisplay::Symbolic(TextValue::parse(&f[0])?))
        } else {
            Err(TextError::UnknownConstructor {
                type_name: "TimeSignatureDisplay",
                found: ctor.to_owned(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroU16;

    use crate::graph::{
        BeatGroup, GraphicContent, HairpinDirection, MetricTimeModel, OctaveOffset, PowerOfTwo,
        ProportionalTimeModel, StaffBasedContent, TextLineDefinition,
    };
    use crate::ids::{
        EventId, OperationId, RegionId, ReplicaId, StaffId, TimeSignatureId, VoiceId,
    };
    use crate::pitch::{ForeignFormatId, StaffGroupKindRegistryId, TieClassRegistryId};
    use crate::textvalue::read_sexp;
    use crate::time::{AnchorOffset, MusicalDuration, RationalTime, TimeAnchor, WallClockDuration};

    // --- builders -----------------------------------------------------------

    fn event(n: u64) -> EventId {
        EventId::new(ReplicaId(1), n)
    }

    fn anchor(n: u64) -> TimeAnchor {
        TimeAnchor::Event {
            id: event(n),
            offset: AnchorOffset::Zero,
        }
    }

    fn dur(n: i64, d: i64) -> MusicalDuration {
        MusicalDuration(RationalTime::new(n, d).expect("valid rational"))
    }

    /// Everything a canonical value writes, a read must return unchanged:
    /// `project` -> `render` -> `read_sexp` -> `parse` is the identity.
    #[track_caller]
    fn round_trip<T: TextValue + PartialEq + core::fmt::Debug>(value: T) {
        let text = value.project().render();
        let sexp = read_sexp(&text).unwrap_or_else(|e| panic!("{text:?} is not valid syntax: {e}"));
        let parsed = T::parse(&sexp).unwrap_or_else(|e| panic!("{text:?} did not parse: {e}"));
        assert_eq!(value, parsed, "{text:?} did not round-trip");
    }

    /// A valid 4/4 signature whose four quarter-note beat groups sum to a whole.
    fn four_four() -> TimeSignature {
        let bg = || BeatGroup {
            duration: dur(1, 4),
            subdivision: None,
            accent: 1,
        };
        TimeSignature::new(
            TimeSignatureId::new(ReplicaId(1), 1),
            TimeSignatureDisplay::Standard {
                numerator: 4,
                denominator: PowerOfTwo::new(4).expect("4 is a power of two"),
            },
            dur(1, 1),
            vec![bg(), bg(), bg(), bg()],
        )
        .expect("beat groups sum to the measure duration")
    }

    // --- round trips --------------------------------------------------------

    #[test]
    fn newtypes_round_trip() {
        round_trip(Timestamp(1_700_000_000_000));
        round_trip(Timestamp(0));
        round_trip(SpaceUnit(CanonicalF64::new(1.5).unwrap()));
        round_trip(SoundConfiguration(vec![0xde, 0xad, 0xbe, 0xef]));
        round_trip(SoundConfiguration(vec![]));
    }

    #[test]
    fn private_field_structs_round_trip() {
        round_trip(KeySignature::new(-3).unwrap());
        round_trip(KeySignature::new(0).unwrap());
        round_trip(KeySignature::new(7).unwrap());
        round_trip(TupletRatio::new(3, 2).unwrap());
        round_trip(TupletRatio::new(6, 4).unwrap());
        round_trip(four_four());

        let mut edges = BTreeMap::new();
        edges.insert(event(1), vec![event(2), event(3)]);
        edges.insert(event(2), vec![event(3)]);
        round_trip(EventOrderingDAG::try_new(edges).expect("acyclic"));
        round_trip(EventOrderingDAG::default());
    }

    #[test]
    fn score_tuning_context_round_trips_and_overrides_do_not_project() {
        // Empty overrides: ordinary identity round-trip.
        round_trip(ScoreTuningContext::default());

        // Non-empty overrides: the text projection is identical to the
        // empty-overrides projection (the field is in-memory only, Push 4b
        // tranche 2 / Ruling C), and parsing always reconstructs `overrides`
        // as empty.
        let mut with_overrides = ScoreTuningContext::default();
        with_overrides
            .overrides
            .push(crate::tuning::TuningOverride {
                scope: crate::tuning::TuningScope::Staff(StaffId::new(ReplicaId(1), 1)),
                pitch_space: None,
                tuning_system: None,
                reference: None,
            });
        assert_eq!(
            with_overrides.project().render(),
            ScoreTuningContext::default().project().render(),
            "overrides must not appear in the text projection"
        );
        let parsed = ScoreTuningContext::parse(&with_overrides.project()).unwrap();
        assert!(parsed.overrides.is_empty());
    }

    #[test]
    fn tagged_unions_round_trip() {
        round_trip(SpannerKind::Generic);
        round_trip(SpannerKind::Hairpin(HairpinDirection::Crescendo));
        round_trip(SpannerKind::OctaveLine(OctaveOffset(2)));
        round_trip(SpannerKind::TextLine(TextLineDefinition {
            text: "cresc.".to_owned(),
        }));

        round_trip(RepeatKind::SimpleRepeat { count: 2 });
        round_trip(RepeatKind::DalSegno {
            segno: anchor(1),
            end_target: anchor(2),
        });
        round_trip(RepeatKind::Volta);

        round_trip(MetadataValue::Text("a\"b".to_owned()));
        round_trip(MetadataValue::Integer(-5));
        round_trip(MetadataValue::Flag(true));

        round_trip(RegionTimeModel::Metric(MetricTimeModel::default()));
        round_trip(RegionTimeModel::Proportional(ProportionalTimeModel {
            duration: WallClockDuration(1000),
        }));

        round_trip(RegionContent::Hybrid {
            staves: StaffBasedContent::default(),
            overlay: GraphicContent::default(),
            overlay_below_staves: true,
        });
        round_trip(RegionContent::FreeGraphic(GraphicContent::default()));

        round_trip(VoiceOrigin::UserDeclared);
        round_trip(VoiceOrigin::Imported {
            format: ForeignFormatId::new("musicxml"),
        });
        round_trip(VoiceOrigin::SystemPromoted {
            winning_operation: OperationId::new(ReplicaId(1), 1),
            losing_operation: OperationId::new(ReplicaId(1), 2),
            original_voice: VoiceId::new(ReplicaId(1), 3),
        });

        round_trip(StaffGroupKind::GrandStaff);
        round_trip(StaffGroupKind::Registered(StaffGroupKindRegistryId::new(
            "custom",
        )));

        round_trip(TieClass::Standard);
        round_trip(TieClass::LaissezVibrer);
        round_trip(TieClass::Registered(TieClassRegistryId::new("x")));

        round_trip(AnnotationAnchor::Event(event(4)));
        round_trip(AnnotationAnchor::Range {
            start: anchor(1),
            end: anchor(2),
        });
        round_trip(AnnotationAnchor::Region(RegionId::new(ReplicaId(1), 5)));

        round_trip(GestureAnchoring::Events(vec![event(1), event(2)]));
        round_trip(GestureAnchoring::Range {
            start: anchor(1),
            end: anchor(2),
            staves: vec![StaffId::new(ReplicaId(1), 1)],
        });
        round_trip(GestureAnchoring::Free);

        round_trip(DecompositionSource::UserChosen);
        round_trip(DecompositionSource::Propagated { from: event(9) });

        round_trip(TimeSignatureDisplay::Standard {
            numerator: 3,
            denominator: PowerOfTwo::new(4).unwrap(),
        });
        round_trip(TimeSignatureDisplay::MixedDenominators {
            components: vec![
                (3, NonZeroU16::new(8).unwrap()),
                (2, NonZeroU16::new(4).unwrap()),
            ],
        });
        round_trip(TimeSignatureDisplay::None);
        round_trip(TimeSignatureDisplay::Symbolic(7));
    }

    // --- strict rejection (validation must not be laundered) -----------------

    #[test]
    fn a_cyclic_event_ordering_is_rejected_not_accepted() {
        // A self-loop `a -> a` is a cycle: `try_new` returns `None`, and the
        // parse surfaces that rather than accepting an ill-formed ordering.
        let a = event(1);
        let cyclic = Sexp::List(vec![
            Sexp::Symbol(kebab("EventOrderingDAG")),
            Sexp::List(vec![Sexp::List(vec![
                a.project(),
                Sexp::List(vec![a.project()]),
            ])]),
        ]);
        assert!(
            EventOrderingDAG::parse(&cyclic).is_err(),
            "a cyclic ordering must be rejected"
        );
    }

    #[test]
    fn an_out_of_range_key_signature_is_rejected() {
        for bad in [
            "(key-signature 8)",
            "(key-signature -8)",
            "(key-signature 100)",
        ] {
            let s = read_sexp(bad).unwrap();
            assert!(
                KeySignature::parse(&s).is_err(),
                "{bad} is outside -7..=7 and must be rejected"
            );
        }
        assert!(KeySignature::parse(&read_sexp("(key-signature -3)").unwrap()).is_ok());
    }

    #[test]
    fn a_degenerate_tuplet_ratio_is_rejected() {
        for bad in [
            "(tuplet-ratio 3 3)",
            "(tuplet-ratio 0 2)",
            "(tuplet-ratio 2 0)",
        ] {
            let s = read_sexp(bad).unwrap();
            assert!(
                TupletRatio::parse(&s).is_err(),
                "{bad} is degenerate and must be rejected"
            );
        }
    }

    #[test]
    fn a_time_signature_whose_beat_groups_do_not_sum_is_rejected() {
        // Take a valid projection and corrupt the measure-duration field
        // (index 3: `[time-signature id display measure-duration beat-groups]`),
        // so the beat groups no longer sum to it. `new` must reject.
        let mut sexp = four_four().project();
        if let Sexp::List(items) = &mut sexp {
            items[3] = dur(1, 2).project();
        }
        assert!(
            TimeSignature::parse(&sexp).is_err(),
            "a mismatched beat-group sum must be rejected, not normalized"
        );
    }

    // --- strict rejection (variant shape) -----------------------------------

    #[test]
    fn a_fieldless_variant_rejects_its_list_spelling() {
        // `Volta`, `Free`, and `TimeSignatureDisplay::None` project to bare
        // symbols; their list spellings denote the same value a second way,
        // which strict parsing forbids.
        assert!(RepeatKind::parse(&read_sexp("(volta)").unwrap()).is_err());
        assert!(GestureAnchoring::parse(&read_sexp("(free)").unwrap()).is_err());
        assert!(TimeSignatureDisplay::parse(&read_sexp("(none)").unwrap()).is_err());
        // And the bare symbols are accepted.
        assert!(RepeatKind::parse(&read_sexp("volta").unwrap()).is_ok());
        assert!(GestureAnchoring::parse(&read_sexp("free").unwrap()).is_ok());
        assert!(TimeSignatureDisplay::parse(&read_sexp("none").unwrap()).is_ok());
    }

    #[test]
    fn an_unknown_constructor_is_rejected() {
        assert!(matches!(
            RepeatKind::parse(&read_sexp("nope").unwrap()),
            Err(TextError::UnknownConstructor { .. })
        ));
        assert!(matches!(
            RegionContent::parse(&read_sexp("(mystery x)").unwrap()),
            Err(TextError::UnknownConstructor { .. })
        ));
    }

    // --- SoundConfiguration: a byte string, never a list of integers --------

    #[test]
    fn sound_configuration_projects_as_a_byte_string_not_a_list() {
        let sc = SoundConfiguration(vec![0x00, 0x0a, 0xff]);
        let projected = sc.project();

        // Rendered `#x…`, exactly as any other opaque byte run.
        assert_eq!(projected.render(), "#x000aff");
        assert!(matches!(projected, Sexp::Bytes(_)));

        // NOT the list of integers the generic `Vec<u8>` impl would produce.
        let as_integer_list: Sexp = vec![0x00u8, 0x0a, 0xff].project();
        assert!(matches!(as_integer_list, Sexp::List(_)));
        assert_eq!(as_integer_list.render(), "(0 10 255)");
        assert_ne!(sc.project(), as_integer_list);

        // A list of integers is rejected where a SoundConfiguration is expected.
        assert!(SoundConfiguration::parse(&read_sexp("(0 10 255)").unwrap()).is_err());
    }
}
