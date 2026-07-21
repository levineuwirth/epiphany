//! [`TextValue`] for the hand-written tagged unions of `time.rs`.
//!
//! These are the Chapter-3/Chapter-5 time enums whose [`Codec`] is written by
//! hand rather than by a `struct_codec!`/`cstyle_enum_codec!` macro, so their
//! projection has to be written by hand too: [`TimeAnchor`], [`EventPosition`],
//! [`ConcreteDuration`], [`EventDuration`], [`TimeBounds`], and [`AnchorOffset`].
//!
//! [`Codec`]: crate::codec::Codec
//!
//! `AnchorOffset` is included because [`TimeAnchor`] embeds it and nothing else
//! projects it: it is a hand-written-codec union with no macro to generate a
//! [`TextValue`], and the C-style/struct siblings a `TimeAnchor` also touches
//! ([`MeasurePosition`], [`RegionEdge`], [`DurationBounds`]) get theirs from their
//! codec macros. Its omission would leave `TimeAnchor::project` with no way to
//! project its `offset`.
//!
//! [`MeasurePosition`]: crate::time::MeasurePosition
//! [`RegionEdge`]: crate::time::RegionEdge
//! [`DurationBounds`]: crate::time::DurationBounds
//!
//! # Why none of these needs [`ensure_canonical`]
//!
//! `req:textproj:strict-parse` forbids a parse that normalizes. A whole-value
//! [`ensure_canonical`] guard is needed only where a value can *only* be built
//! through a constructor that validates or normalizes. None of these unions has
//! one: each `parse` selects a variant by its head symbol and builds it with a
//! plain enum expression, and every field it then reads parses strictly on its
//! own — a byte-string id re-encode-checks, a `RationalTime` rejects a non-reduced
//! ratio, an `i64` range-checks, a [`DurationBounds`] parses its fields
//! positionally. So the guard would have nothing left to catch, and there is no
//! order-constrained `Vec` here for it to be blind to either. The strictness these
//! impls own is narrower and explicit: a fieldless variant is the bare symbol, so
//! its one-element list spelling (`(zero)`, `(unbounded)`) is rejected, not
//! silently accepted (`req:textproj:value-projection` clause 3).
//!
//! [`ensure_canonical`]: crate::textvalue::ensure_canonical

use crate::textvalue::{kebab, Sexp, TextError, TextValue};
use crate::textvalue_impls::{class_of, project_unit};
use crate::time::{
    AnchorOffset, ConcreteDuration, EventDuration, EventPosition, TimeAnchor, TimeBounds,
};

// ===========================================================================
// Shared shape helpers.
// ===========================================================================

/// The projection of a variant that carries `fields`: a list headed by the
/// variant's kebab-case name (`req:textproj:value-projection` clause 3). A
/// fieldless variant is not built here; it is the bare symbol, via
/// [`project_unit`].
fn applied(variant: &str, fields: Vec<Sexp>) -> Sexp {
    let mut items = Vec::with_capacity(fields.len() + 1);
    items.push(Sexp::Symbol(kebab(variant)));
    items.extend(fields);
    Sexp::List(items)
}

/// A tagged-union projection split into its constructor and any fields, keeping
/// the distinction the binary discriminant keeps: a bare symbol names a fieldless
/// variant and can match *only* one; a list headed by a symbol names an applied
/// variant and can match *only* one. Collapsing the two would accept `(zero)` as
/// `zero`, which is exactly the normalization `req:textproj:strict-parse` forbids.
enum Variant<'a> {
    /// A bare symbol: a fieldless variant's canonical spelling.
    Nullary(&'a str),
    /// A list `(<name> <field>…)`: an applied variant's canonical spelling.
    Applied(&'a str, &'a [Sexp]),
}

/// Classifies `s` as a tagged-union projection, or reports why it is not one.
fn classify<'a>(s: &'a Sexp, type_name: &'static str) -> Result<Variant<'a>, TextError> {
    match s {
        Sexp::Symbol(name) => Ok(Variant::Nullary(name)),
        Sexp::List(items) => {
            let (head, fields) = items.split_first().ok_or(TextError::Syntax(
                "a tagged-union list is headed by its variant name",
            ))?;
            let head = head.as_symbol().ok_or(TextError::Syntax(
                "a tagged-union list is headed by its variant name",
            ))?;
            Ok(Variant::Applied(head, fields))
        }
        _ => Err(TextError::Expected {
            expected: type_name,
            found: class_of(s),
        }),
    }
}

/// The error for an applied variant read with the wrong number of fields.
fn arity(type_name: &'static str, expected: usize, found: usize) -> TextError {
    TextError::Arity {
        type_name,
        expected,
        found,
    }
}

/// The error for a constructor symbol naming no variant of `type_name`.
fn unknown(type_name: &'static str, found: &str) -> TextError {
    TextError::UnknownConstructor {
        type_name,
        found: found.to_owned(),
    }
}

// ===========================================================================
// TimeAnchor.
// ===========================================================================

/// A stored time reference. The variant order and every struct-variant's field
/// order mirror `impl Codec for TimeAnchor` exactly: `Event { id, offset }`,
/// `Measure { id, position, offset }`, `Region { id, edge, offset }`,
/// `WallClock { time }`.
///
/// The match in `project` is exhaustive with no `_` arm, so a new variant will
/// not compile until it is projected here. `parse` builds each variant directly
/// and reads its fields with their own strict parses, so it needs no
/// whole-value guard.
impl TextValue for TimeAnchor {
    fn project(&self) -> Sexp {
        match self {
            TimeAnchor::Event { id, offset } => applied(
                "Event",
                vec![TextValue::project(id), TextValue::project(offset)],
            ),
            TimeAnchor::Measure {
                id,
                position,
                offset,
            } => applied(
                "Measure",
                vec![
                    TextValue::project(id),
                    TextValue::project(position),
                    TextValue::project(offset),
                ],
            ),
            TimeAnchor::Region { id, edge, offset } => applied(
                "Region",
                vec![
                    TextValue::project(id),
                    TextValue::project(edge),
                    TextValue::project(offset),
                ],
            ),
            TimeAnchor::WallClock { time } => applied("WallClock", vec![TextValue::project(time)]),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match classify(s, "TimeAnchor")? {
            Variant::Applied(head, fields) if head == kebab("Event") => {
                let [id, offset] = fields else {
                    return Err(arity("TimeAnchor", 2, fields.len()));
                };
                Ok(TimeAnchor::Event {
                    id: TextValue::parse(id)?,
                    offset: TextValue::parse(offset)?,
                })
            }
            Variant::Applied(head, fields) if head == kebab("Measure") => {
                let [id, position, offset] = fields else {
                    return Err(arity("TimeAnchor", 3, fields.len()));
                };
                Ok(TimeAnchor::Measure {
                    id: TextValue::parse(id)?,
                    position: TextValue::parse(position)?,
                    offset: TextValue::parse(offset)?,
                })
            }
            Variant::Applied(head, fields) if head == kebab("Region") => {
                let [id, edge, offset] = fields else {
                    return Err(arity("TimeAnchor", 3, fields.len()));
                };
                Ok(TimeAnchor::Region {
                    id: TextValue::parse(id)?,
                    edge: TextValue::parse(edge)?,
                    offset: TextValue::parse(offset)?,
                })
            }
            Variant::Applied(head, fields) if head == kebab("WallClock") => {
                let [time] = fields else {
                    return Err(arity("TimeAnchor", 1, fields.len()));
                };
                Ok(TimeAnchor::WallClock {
                    time: TextValue::parse(time)?,
                })
            }
            Variant::Applied(head, _) | Variant::Nullary(head) => Err(unknown("TimeAnchor", head)),
        }
    }
}

// ===========================================================================
// EventPosition.
// ===========================================================================

/// An event's position, unioned over the two clocks. Variant order mirrors
/// `impl Codec for EventPosition`: `Musical(_)` then `WallClock(_)`.
impl TextValue for EventPosition {
    fn project(&self) -> Sexp {
        match self {
            EventPosition::Musical(p) => applied("Musical", vec![TextValue::project(p)]),
            EventPosition::WallClock(t) => applied("WallClock", vec![TextValue::project(t)]),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match classify(s, "EventPosition")? {
            Variant::Applied(head, fields) if head == kebab("Musical") => {
                let [p] = fields else {
                    return Err(arity("EventPosition", 1, fields.len()));
                };
                Ok(EventPosition::Musical(TextValue::parse(p)?))
            }
            Variant::Applied(head, fields) if head == kebab("WallClock") => {
                let [t] = fields else {
                    return Err(arity("EventPosition", 1, fields.len()));
                };
                Ok(EventPosition::WallClock(TextValue::parse(t)?))
            }
            Variant::Applied(head, _) | Variant::Nullary(head) => {
                Err(unknown("EventPosition", head))
            }
        }
    }
}

// ===========================================================================
// ConcreteDuration.
// ===========================================================================

/// A determinate duration in one clock. Variant order mirrors
/// `impl Codec for ConcreteDuration`: `Musical(_)` then `WallClock(_)`.
impl TextValue for ConcreteDuration {
    fn project(&self) -> Sexp {
        match self {
            ConcreteDuration::Musical(d) => applied("Musical", vec![TextValue::project(d)]),
            ConcreteDuration::WallClock(d) => applied("WallClock", vec![TextValue::project(d)]),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match classify(s, "ConcreteDuration")? {
            Variant::Applied(head, fields) if head == kebab("Musical") => {
                let [d] = fields else {
                    return Err(arity("ConcreteDuration", 1, fields.len()));
                };
                Ok(ConcreteDuration::Musical(TextValue::parse(d)?))
            }
            Variant::Applied(head, fields) if head == kebab("WallClock") => {
                let [d] = fields else {
                    return Err(arity("ConcreteDuration", 1, fields.len()));
                };
                Ok(ConcreteDuration::WallClock(TextValue::parse(d)?))
            }
            Variant::Applied(head, _) | Variant::Nullary(head) => {
                Err(unknown("ConcreteDuration", head))
            }
        }
    }
}

// ===========================================================================
// EventDuration.
// ===========================================================================

/// An event's duration, unioned over musical, wall-clock, and indeterminate
/// forms. Variant order mirrors `impl Codec for EventDuration`: `Musical(_)`,
/// `WallClock(_)`, then `Indeterminate(_)` whose payload is a
/// [`DurationBounds`](crate::time::DurationBounds) — itself a `struct_codec!`
/// struct whose projection and strict parse the macro supplies.
impl TextValue for EventDuration {
    fn project(&self) -> Sexp {
        match self {
            EventDuration::Musical(d) => applied("Musical", vec![TextValue::project(d)]),
            EventDuration::WallClock(d) => applied("WallClock", vec![TextValue::project(d)]),
            EventDuration::Indeterminate(b) => {
                applied("Indeterminate", vec![TextValue::project(b)])
            }
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match classify(s, "EventDuration")? {
            Variant::Applied(head, fields) if head == kebab("Musical") => {
                let [d] = fields else {
                    return Err(arity("EventDuration", 1, fields.len()));
                };
                Ok(EventDuration::Musical(TextValue::parse(d)?))
            }
            Variant::Applied(head, fields) if head == kebab("WallClock") => {
                let [d] = fields else {
                    return Err(arity("EventDuration", 1, fields.len()));
                };
                Ok(EventDuration::WallClock(TextValue::parse(d)?))
            }
            Variant::Applied(head, fields) if head == kebab("Indeterminate") => {
                let [b] = fields else {
                    return Err(arity("EventDuration", 1, fields.len()));
                };
                Ok(EventDuration::Indeterminate(TextValue::parse(b)?))
            }
            Variant::Applied(head, _) | Variant::Nullary(head) => {
                Err(unknown("EventDuration", head))
            }
        }
    }
}

// ===========================================================================
// TimeBounds.
// ===========================================================================

/// An aleatoric interval bound. Variant order and struct-variant field order
/// mirror `impl Codec for TimeBounds`: `MusicalRange { min, max }`,
/// `WallClockRange { min, max }`, then the fieldless `Unbounded`.
///
/// `Unbounded` is the bare symbol `unbounded`; the list `(unbounded)` is a
/// different spelling and is rejected below as an unknown *applied* constructor,
/// never accepted as `Unbounded`. The binary form imposes no `min <= max`
/// ordering and there is no validating constructor, so any `min`/`max` pair is
/// canonical and no per-site order check is owed.
impl TextValue for TimeBounds {
    fn project(&self) -> Sexp {
        match self {
            TimeBounds::MusicalRange { min, max } => applied(
                "MusicalRange",
                vec![TextValue::project(min), TextValue::project(max)],
            ),
            TimeBounds::WallClockRange { min, max } => applied(
                "WallClockRange",
                vec![TextValue::project(min), TextValue::project(max)],
            ),
            TimeBounds::Unbounded => project_unit("Unbounded"),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match classify(s, "TimeBounds")? {
            Variant::Nullary(head) if head == kebab("Unbounded") => Ok(TimeBounds::Unbounded),
            Variant::Applied(head, fields) if head == kebab("MusicalRange") => {
                let [min, max] = fields else {
                    return Err(arity("TimeBounds", 2, fields.len()));
                };
                Ok(TimeBounds::MusicalRange {
                    min: TextValue::parse(min)?,
                    max: TextValue::parse(max)?,
                })
            }
            Variant::Applied(head, fields) if head == kebab("WallClockRange") => {
                let [min, max] = fields else {
                    return Err(arity("TimeBounds", 2, fields.len()));
                };
                Ok(TimeBounds::WallClockRange {
                    min: TextValue::parse(min)?,
                    max: TextValue::parse(max)?,
                })
            }
            Variant::Applied(head, _) | Variant::Nullary(head) => Err(unknown("TimeBounds", head)),
        }
    }
}

// ===========================================================================
// AnchorOffset.
// ===========================================================================

/// An offset applied to an anchor target, embedded in [`TimeAnchor`]. Variant
/// order mirrors `impl Codec for AnchorOffset`: `Musical(_)`, `WallClock(_)`,
/// then the fieldless `Zero`.
///
/// As with [`TimeBounds::Unbounded`], `Zero` is the bare symbol `zero` and its
/// list spelling `(zero)` is rejected rather than normalized.
impl TextValue for AnchorOffset {
    fn project(&self) -> Sexp {
        match self {
            AnchorOffset::Musical(d) => applied("Musical", vec![TextValue::project(d)]),
            AnchorOffset::WallClock(d) => applied("WallClock", vec![TextValue::project(d)]),
            AnchorOffset::Zero => project_unit("Zero"),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match classify(s, "AnchorOffset")? {
            Variant::Nullary(head) if head == kebab("Zero") => Ok(AnchorOffset::Zero),
            Variant::Applied(head, fields) if head == kebab("Musical") => {
                let [d] = fields else {
                    return Err(arity("AnchorOffset", 1, fields.len()));
                };
                Ok(AnchorOffset::Musical(TextValue::parse(d)?))
            }
            Variant::Applied(head, fields) if head == kebab("WallClock") => {
                let [d] = fields else {
                    return Err(arity("AnchorOffset", 1, fields.len()));
                };
                Ok(AnchorOffset::WallClock(TextValue::parse(d)?))
            }
            Variant::Applied(head, _) | Variant::Nullary(head) => {
                Err(unknown("AnchorOffset", head))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{EventId, MeasureId, RegionId, ReplicaId};
    use crate::textvalue::read_sexp;
    use crate::time::{
        DurationBounds, MeasurePosition, MusicalDuration, MusicalPosition, RationalTime,
        RegionEdge, WallClockDuration, WallClockTime,
    };

    fn rt(n: i64, d: i64) -> RationalTime {
        RationalTime::new(n, d).unwrap()
    }

    /// Build a value, project it, render it, read it back with the strict reader,
    /// parse it, and require the result to equal the original.
    #[track_caller]
    fn round_trip<T: TextValue + PartialEq + std::fmt::Debug>(value: T) {
        let text = value.project().render();
        let read = read_sexp(&text)
            .unwrap_or_else(|e| panic!("projection {text:?} was rejected by read_sexp: {e}"));
        let back =
            T::parse(&read).unwrap_or_else(|e| panic!("projection {text:?} did not parse: {e}"));
        assert_eq!(value, back, "{text:?} did not round-trip");
    }

    /// The strict reader must accept `text`, and the typed parse must then reject
    /// it — the projection is well-formed lexically but is not the canonical
    /// projection of any value of `T`.
    #[track_caller]
    fn rejects<T: TextValue + std::fmt::Debug>(text: &str) {
        let s = read_sexp(text).unwrap_or_else(|e| panic!("{text:?} was not lexable: {e}"));
        assert!(
            T::parse(&s).is_err(),
            "{text:?} was accepted; strict parsing forbids it"
        );
    }

    // -------------------------------------------------------------------
    // TimeAnchor.
    // -------------------------------------------------------------------

    #[test]
    fn time_anchor_round_trips_every_variant() {
        round_trip(TimeAnchor::WallClock {
            time: WallClockTime(42),
        });
        round_trip(TimeAnchor::Event {
            id: EventId::new(ReplicaId(1), 2),
            offset: AnchorOffset::Zero,
        });
        round_trip(TimeAnchor::Measure {
            id: MeasureId::new(ReplicaId(3), 4),
            position: MeasurePosition::Start,
            offset: AnchorOffset::Musical(MusicalDuration(rt(1, 4))),
        });
        round_trip(TimeAnchor::Region {
            id: RegionId::new(ReplicaId(5), 6),
            edge: RegionEdge::End,
            offset: AnchorOffset::WallClock(WallClockDuration(10)),
        });
    }

    #[test]
    fn time_anchor_projects_fields_in_codec_order() {
        let anchor = TimeAnchor::Measure {
            id: MeasureId::new(ReplicaId(0), 1),
            position: MeasurePosition::End,
            offset: AnchorOffset::Zero,
        };
        // id, then position, then offset — the order `impl Codec` writes.
        assert_eq!(
            anchor.project().render(),
            "(measure #x00000000000000000000000000000001 end zero)"
        );
    }

    #[test]
    fn time_anchor_rejects_wrong_arity_and_unknown_constructor() {
        // A one-field `WallClock` written with none, and a `Measure` with too few.
        rejects::<TimeAnchor>("(wall-clock)");
        rejects::<TimeAnchor>("(measure #x00000000000000000000000000000001 start)");
        // No such variant.
        rejects::<TimeAnchor>("(sometime 1)");
        // A field-bearing union is never a bare symbol.
        rejects::<TimeAnchor>("wall-clock");
    }

    // -------------------------------------------------------------------
    // EventPosition.
    // -------------------------------------------------------------------

    #[test]
    fn event_position_round_trips() {
        round_trip(EventPosition::Musical(MusicalPosition(rt(3, 4))));
        round_trip(EventPosition::WallClock(WallClockTime(-7)));
    }

    /// The inner rational is parsed strictly, so a non-reduced ratio is rejected
    /// through the projection rather than reduced inside it.
    #[test]
    fn event_position_rejects_non_canonical_field_and_shape() {
        rejects::<EventPosition>("(musical (ratio 2 4))");
        rejects::<EventPosition>("(musical)");
        rejects::<EventPosition>("(musical (ratio 1 2) (ratio 1 2))");
        rejects::<EventPosition>("(bogus (ratio 1 2))");
    }

    // -------------------------------------------------------------------
    // ConcreteDuration.
    // -------------------------------------------------------------------

    #[test]
    fn concrete_duration_round_trips() {
        round_trip(ConcreteDuration::Musical(MusicalDuration(rt(1, 4))));
        round_trip(ConcreteDuration::WallClock(WallClockDuration(500)));
    }

    #[test]
    fn concrete_duration_rejects_non_canonical_field_and_shape() {
        rejects::<ConcreteDuration>("(musical (ratio 2 4))");
        rejects::<ConcreteDuration>("(wall-clock 1 2)");
        rejects::<ConcreteDuration>("(indeterminate 1)");
    }

    // -------------------------------------------------------------------
    // EventDuration.
    // -------------------------------------------------------------------

    #[test]
    fn event_duration_round_trips_including_indeterminate() {
        round_trip(EventDuration::Musical(MusicalDuration(rt(1, 8))));
        round_trip(EventDuration::WallClock(WallClockDuration(250)));
        round_trip(EventDuration::Indeterminate(DurationBounds {
            lower: Some(ConcreteDuration::Musical(MusicalDuration(rt(1, 4)))),
            upper: Some(ConcreteDuration::WallClock(WallClockDuration(1000))),
        }));
        round_trip(EventDuration::Indeterminate(DurationBounds {
            lower: None,
            upper: None,
        }));
    }

    #[test]
    fn event_duration_rejects_wrong_arity_and_unknown_constructor() {
        rejects::<EventDuration>("(indeterminate)");
        rejects::<EventDuration>("(musical (ratio 2 4))");
        rejects::<EventDuration>("(nope 1)");
    }

    // -------------------------------------------------------------------
    // TimeBounds.
    // -------------------------------------------------------------------

    #[test]
    fn time_bounds_round_trips_every_variant() {
        round_trip(TimeBounds::MusicalRange {
            min: MusicalPosition(rt(0, 1)),
            max: MusicalPosition(rt(4, 1)),
        });
        round_trip(TimeBounds::WallClockRange {
            min: WallClockTime(0),
            max: WallClockTime(1000),
        });
        round_trip(TimeBounds::Unbounded);
    }

    /// `Unbounded` projects to the bare symbol, so the list spelling `(unbounded)`
    /// is not its canonical projection and must be rejected — not normalized into
    /// `Unbounded`. Symmetrically, a range variant is never a bare symbol.
    #[test]
    fn time_bounds_rejects_the_list_form_of_the_fieldless_variant() {
        rejects::<TimeBounds>("(unbounded)");
        rejects::<TimeBounds>("musical-range");
        rejects::<TimeBounds>("(musical-range (ratio 1 2))");
        rejects::<TimeBounds>("(musical-range (ratio 2 4) (ratio 1 2))");
    }

    // -------------------------------------------------------------------
    // AnchorOffset (the dependency TimeAnchor embeds).
    // -------------------------------------------------------------------

    #[test]
    fn anchor_offset_round_trips_every_variant() {
        round_trip(AnchorOffset::Musical(MusicalDuration(rt(1, 16))));
        round_trip(AnchorOffset::WallClock(WallClockDuration(-3)));
        round_trip(AnchorOffset::Zero);
    }

    #[test]
    fn anchor_offset_rejects_the_list_form_of_zero() {
        rejects::<AnchorOffset>("(zero)");
        rejects::<AnchorOffset>("musical");
        rejects::<AnchorOffset>("(musical (ratio 2 4))");
    }
}
