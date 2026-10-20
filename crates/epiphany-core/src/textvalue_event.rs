//! [`TextValue`] for the event taxonomy and the event arena (Chapter 5).
//!
//! The seven payload records — [`PitchedEvent`], [`UnpitchedEvent`], [`Rest`],
//! [`IndeterminateEvent`], [`TrajectoryEvent`], [`GraphicEvent`], [`CueEvent`] —
//! are `struct_codec!` types and get their projection from the same macro that
//! gives them their binary codec (see `codec.rs`), so their field order cannot
//! drift from the bytes. This module supplies what those macros cannot: the two
//! placeholder newtypes, the four hand-written tagged unions, the [`Event`] union
//! that dispatches over the seven records, and the [`EventArena`].
//!
//! Every field order below mirrors the matching `impl Codec for …` `fn enc` in
//! `codec.rs` exactly (`req:textproj:value-projection` clause 1).
//!
//! The arena is the one place a `parse` must add a **per-site order check**:
//! the binary form writes it in ascending `EventId` order but rebuilds it through
//! [`EventArena::insert`], which accepts any order and re-sorts on the way out.
//! Returning such an arena would launder a mis-ordered text into a canonical value
//! — the normalization `req:textproj:strict-parse` forbids.

use crate::event::{
    ArenaError, ArpeggioDirection, CueEvent, Event, EventArena, EventMark, GraphicEvent,
    IndeterminacyKind, IndeterminateEvent, PitchedEvent, Rest, StaffPosition, TrajectoryEndpoint,
    TrajectoryEvent, TrajectoryShape, UnpitchedEvent, UnpitchedMemberId,
};
use crate::ids::{EventId, PitchId};
use crate::pitch::IdentifiedPitch;
use crate::textvalue::{Sexp, TextError, TextValue};
use crate::textvalue_impls::class_of;

// ===========================================================================
// Helpers shared by the tagged unions.
// ===========================================================================

/// The single field of a one-field variant `(<name> <field>)`, after checking
/// the head symbol and the arity. `expect_struct` guarantees exactly one field
/// remains, so indexing it cannot be out of range.
fn one_field<'a>(s: &'a Sexp, name: &str) -> Result<&'a Sexp, TextError> {
    let fields = s.expect_struct(name, 1)?;
    Ok(&fields[0])
}

/// The head symbol of a list `(<head> …)`, for a union whose every variant
/// carries fields. Rejects a non-list and a list not headed by a symbol.
fn head_symbol<'a>(s: &'a Sexp, type_name: &'static str) -> Result<&'a str, TextError> {
    let items = s.as_list().ok_or(TextError::Expected {
        expected: type_name,
        found: class_of(s),
    })?;
    items
        .first()
        .and_then(Sexp::as_symbol)
        .ok_or(TextError::Syntax(
            "a tagged union is a list headed by its variant name",
        ))
}

// ===========================================================================
// Placeholder newtypes.
// ===========================================================================

/// A staff position is its inner `i16` alone, with no wrapper
/// (`req:textproj:value-projection` clause 2): the binary form writes the field
/// and adds no bytes for the newtype, and the text adds no wrapper either.
impl TextValue for StaffPosition {
    fn project(&self) -> Sexp {
        TextValue::project(&self.0)
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        <i16 as TextValue>::parse(s).map(StaffPosition)
    }
}

/// An unpitched member id is its inner `u32` alone; a transparent newtype, as
/// above.
impl TextValue for UnpitchedMemberId {
    fn project(&self) -> Sexp {
        TextValue::project(&self.0)
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        <u32 as TextValue>::parse(s).map(UnpitchedMemberId)
    }
}

// ===========================================================================
// Tagged unions.
// ===========================================================================

/// `impl Codec for EventMark` writes the mark's tag, then a field for the three
/// kinds that carry one, so a fieldless mark is a bare symbol and the others are
/// `(tremolo <strokes>)`, `(tremolo-with-next <strokes>)` and
/// `(arpeggio <direction>)` (`req:textproj:value-projection` clause 3).
impl TextValue for EventMark {
    fn project(&self) -> Sexp {
        match self {
            EventMark::Tremolo { strokes } => {
                Sexp::List(vec![Sexp::sym("tremolo"), strokes.project()])
            }
            EventMark::TremoloWithNext { strokes } => {
                Sexp::List(vec![Sexp::sym("tremolo-with-next"), strokes.project()])
            }
            EventMark::Arpeggio { direction } => {
                Sexp::List(vec![Sexp::sym("arpeggio"), direction.project()])
            }
            fieldless => Sexp::sym(EVENT_MARK_SYMBOLS[fieldless.tag() as usize]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s {
            Sexp::Symbol(name) => FIELDLESS_EVENT_MARKS
                .iter()
                .find(|m| EVENT_MARK_SYMBOLS[m.tag() as usize] == name.as_str())
                .copied()
                .ok_or_else(|| TextError::UnknownConstructor {
                    type_name: "EventMark",
                    found: name.clone(),
                }),
            Sexp::List(_) => match head_symbol(s, "EventMark")? {
                "tremolo" => Ok(EventMark::Tremolo {
                    strokes: u8::parse(one_field(s, "tremolo")?)?,
                }),
                "tremolo-with-next" => Ok(EventMark::TremoloWithNext {
                    strokes: u8::parse(one_field(s, "tremolo-with-next")?)?,
                }),
                "arpeggio" => Ok(EventMark::Arpeggio {
                    direction: ArpeggioDirection::parse(one_field(s, "arpeggio")?)?,
                }),
                found => Err(TextError::UnknownConstructor {
                    type_name: "EventMark",
                    found: found.to_owned(),
                }),
            },
            _ => Err(TextError::Expected {
                expected: "EventMark",
                found: class_of(s),
            }),
        }
    }
}

/// Each mark's symbol, indexed by its tag.
const EVENT_MARK_SYMBOLS: [&str; 22] = [
    "staccato",
    "staccatissimo",
    "spiccato",
    "tenuto",
    "detached-legato",
    "accent",
    "marcato",
    "stress",
    "unstress",
    "up-bow",
    "down-bow",
    "harmonic",
    "open-string",
    "stopped",
    "snap-pizzicato",
    "scoop",
    "plop",
    "doit",
    "falloff",
    "tremolo",
    "tremolo-with-next",
    "arpeggio",
];

/// The marks that carry no field, in tag order.
const FIELDLESS_EVENT_MARKS: [EventMark; 19] = [
    EventMark::Staccato,
    EventMark::Staccatissimo,
    EventMark::Spiccato,
    EventMark::Tenuto,
    EventMark::DetachedLegato,
    EventMark::Accent,
    EventMark::Marcato,
    EventMark::Stress,
    EventMark::Unstress,
    EventMark::UpBow,
    EventMark::DownBow,
    EventMark::Harmonic,
    EventMark::OpenString,
    EventMark::Stopped,
    EventMark::SnapPizzicato,
    EventMark::Scoop,
    EventMark::Plop,
    EventMark::Doit,
    EventMark::Falloff,
];

/// An event's marks and ornaments are canonical sets, so the text parses one
/// only in that order (`req:textproj:strict-parse`), as the codec decodes it.
fn canonical_sets(
    marks: &[EventMark],
    ornaments: &[crate::event::Ornament],
) -> Result<(), TextError> {
    if marks.windows(2).all(|w| w[0].tag() < w[1].tag())
        && ornaments.windows(2).all(|w| w[0].kind < w[1].kind)
    {
        Ok(())
    } else {
        Err(TextError::NotCanonical(
            "an event's marks and ornaments are sets, ascending by kind, one per kind",
        ))
    }
}

/// `impl Codec for IndeterminacyKind`: three fieldless
/// kinds as bare symbols and `Compound` carrying a nested sequence, so the text
/// is `pitch` / `duration` / `choice` / `(compound (<kind>…))`. The `Compound`
/// sequence is itself an [`IndeterminacyKind`] list, and the generic `Vec` impl
/// carries it recursively.
impl TextValue for IndeterminacyKind {
    fn project(&self) -> Sexp {
        match self {
            IndeterminacyKind::Pitch => Sexp::sym("pitch"),
            IndeterminacyKind::Duration => Sexp::sym("duration"),
            IndeterminacyKind::Choice => Sexp::sym("choice"),
            IndeterminacyKind::Compound(v) => Sexp::List(vec![Sexp::sym("compound"), v.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s {
            Sexp::Symbol(name) => match name.as_str() {
                "pitch" => Ok(IndeterminacyKind::Pitch),
                "duration" => Ok(IndeterminacyKind::Duration),
                "choice" => Ok(IndeterminacyKind::Choice),
                found => Err(TextError::UnknownConstructor {
                    type_name: "IndeterminacyKind",
                    found: found.to_owned(),
                }),
            },
            Sexp::List(_) => Ok(IndeterminacyKind::Compound(
                Vec::<IndeterminacyKind>::parse(one_field(s, "compound")?)?,
            )),
            _ => Err(TextError::Expected {
                expected: "IndeterminacyKind",
                found: class_of(s),
            }),
        }
    }
}

/// `impl Codec for TrajectoryEndpoint` writes a discriminant then a field for
/// both variants — a [`PitchId`] for `EventPitch`, an [`IdentifiedPitch`] for
/// `ExplicitPitch` — so every spelling is a list and the head symbol selects the
/// variant.
impl TextValue for TrajectoryEndpoint {
    fn project(&self) -> Sexp {
        match self {
            TrajectoryEndpoint::EventPitch(id) => {
                Sexp::List(vec![Sexp::sym("event-pitch"), id.project()])
            }
            TrajectoryEndpoint::ExplicitPitch(p) => {
                Sexp::List(vec![Sexp::sym("explicit-pitch"), p.project()])
            }
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match head_symbol(s, "TrajectoryEndpoint")? {
            "event-pitch" => Ok(TrajectoryEndpoint::EventPitch(PitchId::parse(one_field(
                s,
                "event-pitch",
            )?)?)),
            "explicit-pitch" => Ok(TrajectoryEndpoint::ExplicitPitch(IdentifiedPitch::parse(
                one_field(s, "explicit-pitch")?,
            )?)),
            found => Err(TextError::UnknownConstructor {
                type_name: "TrajectoryEndpoint",
                found: found.to_owned(),
            }),
        }
    }
}

/// `impl Codec for TrajectoryShape` has three fieldless shapes and `Stepwise`
/// carrying a pitch sequence, so the text is `linear` / `exponential` / `curve`
/// / `(stepwise (<pitch>…))`.
impl TextValue for TrajectoryShape {
    fn project(&self) -> Sexp {
        match self {
            TrajectoryShape::Linear => Sexp::sym("linear"),
            TrajectoryShape::Exponential => Sexp::sym("exponential"),
            TrajectoryShape::Curve => Sexp::sym("curve"),
            TrajectoryShape::Stepwise(v) => Sexp::List(vec![Sexp::sym("stepwise"), v.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s {
            Sexp::Symbol(name) => match name.as_str() {
                "linear" => Ok(TrajectoryShape::Linear),
                "exponential" => Ok(TrajectoryShape::Exponential),
                "curve" => Ok(TrajectoryShape::Curve),
                found => Err(TextError::UnknownConstructor {
                    type_name: "TrajectoryShape",
                    found: found.to_owned(),
                }),
            },
            Sexp::List(_) => Ok(TrajectoryShape::Stepwise(Vec::<IdentifiedPitch>::parse(
                one_field(s, "stepwise")?,
            )?)),
            _ => Err(TextError::Expected {
                expected: "TrajectoryShape",
                found: class_of(s),
            }),
        }
    }
}

// ===========================================================================
// The event union.
// ===========================================================================

/// `impl Codec for Event` writes a discriminant byte and then delegates to the
/// payload's own `enc`, so its projection is **not** transparent: it is
/// `(<variant> <payload>)`, the kebab of the *variant* name wrapping the payload
/// record's own struct projection — e.g. `(rest (rest <id> … <visible>))` and
/// `(pitched (pitched-event <id> … <grace>))`. A single-field variant projects as
/// `(<variant> <field>)`, never as the field alone; only an unnamed-field newtype
/// is transparent (`req:textproj:value-projection` clauses 2 and 3), and `Event`
/// is a union, not a newtype.
impl TextValue for Event {
    fn project(&self) -> Sexp {
        match self {
            Event::Pitched(e) => Sexp::List(vec![Sexp::sym("pitched"), e.project()]),
            Event::Unpitched(e) => Sexp::List(vec![Sexp::sym("unpitched"), e.project()]),
            Event::Rest(e) => Sexp::List(vec![Sexp::sym("rest"), e.project()]),
            Event::Indeterminate(e) => Sexp::List(vec![Sexp::sym("indeterminate"), e.project()]),
            Event::Trajectory(e) => Sexp::List(vec![Sexp::sym("trajectory"), e.project()]),
            Event::Graphic(e) => Sexp::List(vec![Sexp::sym("graphic"), e.project()]),
            Event::Cue(e) => Sexp::List(vec![Sexp::sym("cue"), e.project()]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match head_symbol(s, "Event")? {
            "pitched" => {
                let e = PitchedEvent::parse(one_field(s, "pitched")?)?;
                canonical_sets(&e.marks, &e.ornaments)?;
                Ok(Event::Pitched(e))
            }
            "unpitched" => {
                let e = UnpitchedEvent::parse(one_field(s, "unpitched")?)?;
                canonical_sets(&e.marks, &[])?;
                Ok(Event::Unpitched(e))
            }
            "rest" => Ok(Event::Rest(Rest::parse(one_field(s, "rest")?)?)),
            "indeterminate" => Ok(Event::Indeterminate(IndeterminateEvent::parse(one_field(
                s,
                "indeterminate",
            )?)?)),
            "trajectory" => Ok(Event::Trajectory(TrajectoryEvent::parse(one_field(
                s,
                "trajectory",
            )?)?)),
            "graphic" => Ok(Event::Graphic(GraphicEvent::parse(one_field(
                s, "graphic",
            )?)?)),
            "cue" => Ok(Event::Cue(CueEvent::parse(one_field(s, "cue")?)?)),
            found => Err(TextError::UnknownConstructor {
                type_name: "Event",
                found: found.to_owned(),
            }),
        }
    }
}

// ===========================================================================
// The event arena.
// ===========================================================================

/// The arena projects as the sequence of its events in ascending `EventId`
/// order, matching `impl Codec for EventArena`, whose `enc` iterates
/// [`EventArena::iter_canonical`]. Identity travels inside each event, so no
/// key is written beside it.
///
/// `parse` enforces that ascending order **per site**. The binary decoder rebuilds
/// the arena through [`EventArena::insert`], which accepts events in any order and
/// lets [`EventArena::iter_canonical`] silently re-sort them; returning such an
/// arena would launder a mis-ordered or duplicate-id text into a canonical value,
/// the normalization `req:textproj:strict-parse` forbids. Equal ids fail the same
/// strict-`<` test, so a duplicate is rejected here and never reaches `insert`.
///
/// A whole-value re-project-and-compare guard *would* also catch this, since
/// `project` re-sorts and so a mis-ordered input re-projects differently. The
/// explicit check is what the crate keeps: it names the fault
/// (`NotStrictlyIncreasing`), points at the first offending event, and avoids a
/// redundant second projection of the whole arena. It is mutation-verified live.
impl TextValue for EventArena {
    fn project(&self) -> Sexp {
        Sexp::List(self.iter_canonical().map(Event::project).collect())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "EventArena",
            found: class_of(s),
        })?;
        let mut arena = EventArena::new();
        let mut previous: Option<EventId> = None;
        for item in items {
            let event = Event::parse(item)?;
            let id = event.id();
            if previous.is_some_and(|prev| prev >= id) {
                return Err(TextError::NotStrictlyIncreasing(
                    "EventArena events must be in ascending EventId order",
                ));
            }
            previous = Some(id);
            // `insert` still enforces the Chapter 5 invariant that a pitched event
            // has at least one pitch. A duplicate id cannot reach it — the strict
            // order check above rejects equal ids — but the arm is kept exhaustive
            // (no `_`) so a new `ArenaError` variant forces a decision here.
            arena.insert(event).map_err(|err| match err {
                ArenaError::EmptyPitchedEvent(_) => {
                    TextError::NotCanonical("a pitched event must have at least one pitch")
                }
                ArenaError::DuplicateId(_) => TextError::NotStrictlyIncreasing(
                    "EventArena events must be in ascending EventId order",
                ),
            })?;
        }
        Ok(arena)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::StemConfiguration;
    use crate::ids::{EventId, PitchId, ReplicaId, VoiceId};
    use crate::pitch::{
        AcousticPitch, AcousticRealization, CmnNominal, IdentifiedPitch, Pitch, PitchSpaceId,
        PitchSpacePosition, ScalePosition, TuningReference,
    };
    use crate::textvalue::read_sexp;
    use crate::time::{
        EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime,
    };

    fn replica() -> ReplicaId {
        ReplicaId(1)
    }

    fn voice() -> VoiceId {
        VoiceId::new(replica(), 100)
    }

    /// project → render → read_sexp → parse must return the original value.
    #[track_caller]
    fn round_trip<T>(value: T)
    where
        T: TextValue + PartialEq + std::fmt::Debug,
    {
        let text = value.project().render();
        let read = read_sexp(&text).unwrap_or_else(|e| panic!("{text:?} did not lex: {e}"));
        let back = T::parse(&read).unwrap_or_else(|e| panic!("{text:?} did not parse: {e}"));
        assert_eq!(back, value, "round trip changed {text:?}");
    }

    fn identified_pitch(counter: u64) -> IdentifiedPitch {
        IdentifiedPitch {
            id: PitchId::new(replica(), counter),
            pitch: Pitch {
                scale_position: ScalePosition {
                    space: PitchSpaceId::new("cmn-12"),
                    position: PitchSpacePosition::Cmn {
                        nominal: CmnNominal::C,
                        alteration: 0,
                        octave: 4,
                    },
                },
                acoustic: AcousticPitch {
                    tuning: TuningReference::Inherit,
                    realization: AcousticRealization::Implicit,
                },
            },
        }
    }

    fn rest_event(counter: u64) -> Event {
        Event::Rest(Rest {
            id: EventId::new(replica(), counter),
            voice: voice(),
            position: EventPosition::Musical(MusicalPosition(
                RationalTime::new(counter as i64, 4).unwrap(),
            )),
            duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
            vertical_position: Some(StaffPosition(-2)),
            visible: true,
        })
    }

    fn pitched_event(counter: u64) -> Event {
        Event::Pitched(PitchedEvent {
            id: EventId::new(replica(), counter),
            voice: voice(),
            position: EventPosition::Musical(MusicalPosition(RationalTime::new(1, 2).unwrap())),
            duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
            pitches: vec![identified_pitch(counter * 10)],
            marks: vec![
                EventMark::Staccato,
                EventMark::Accent,
                EventMark::Tremolo { strokes: 3 },
                EventMark::Arpeggio {
                    direction: ArpeggioDirection::Up,
                },
            ],
            dynamic: None,
            ornaments: vec![crate::event::Ornament {
                kind: crate::event::OrnamentKind::Trill,
                accidental_above: Some(crate::pitch::AccidentalId::new("flat")),
                accidental_below: None,
            }],
            stem: StemConfiguration,
            grace: Some(crate::event::Grace {
                kind: crate::event::GraceKind::Acciaccatura,
                value: crate::graph::NoteValue::Eighth,
                dots: 0,
                order: 1,
            }),
        })
    }

    #[test]
    fn staff_position_and_member_id_are_transparent_newtypes() {
        assert_eq!(StaffPosition(-3).project().render(), "-3");
        assert_eq!(UnpitchedMemberId(42).project().render(), "42");
        round_trip(StaffPosition(-3));
        round_trip(StaffPosition(0));
        round_trip(UnpitchedMemberId(42));
    }

    #[test]
    fn a_grace_and_its_kind_round_trip() {
        use crate::event::{Grace, GraceKind};
        assert_eq!(GraceKind::Acciaccatura.project().render(), "acciaccatura");
        round_trip(GraceKind::Appoggiatura);
        let grace = Grace {
            kind: GraceKind::Appoggiatura,
            value: crate::graph::NoteValue::Sixteenth,
            dots: 1,
            order: 2,
        };
        assert_eq!(
            grace.project().render(),
            "(grace appoggiatura sixteenth 1 2)"
        );
        round_trip(grace);
        assert!(GraceKind::parse(&read_sexp("(acciaccatura)").unwrap()).is_err());
        assert!(GraceKind::parse(&read_sexp("unmeasured").unwrap()).is_err());
    }

    /// Every event mark round-trips; a fieldless mark spelled as a list, or a
    /// field mark spelled bare, is refused, and so is an event whose marks are
    /// out of order or repeat a kind.
    #[test]
    fn event_marks_round_trip_and_an_event_holds_them_as_a_set() {
        let mut all: Vec<EventMark> = FIELDLESS_EVENT_MARKS.to_vec();
        all.extend([
            EventMark::Tremolo { strokes: 2 },
            EventMark::TremoloWithNext { strokes: 3 },
            EventMark::Arpeggio {
                direction: ArpeggioDirection::Plain,
            },
        ]);
        for (tag, mark) in all.iter().enumerate() {
            assert_eq!(mark.tag() as usize, tag);
            round_trip(*mark);
        }
        assert_eq!(EventMark::Marcato.project().render(), "marcato");
        assert_eq!(
            EventMark::Tremolo { strokes: 3 }.project().render(),
            "(tremolo 3)"
        );
        assert!(EventMark::parse(&read_sexp("(staccato)").unwrap()).is_err());
        assert!(EventMark::parse(&read_sexp("tremolo").unwrap()).is_err());
        let Event::Pitched(mut e) = pitched_event(3) else {
            unreachable!()
        };
        e.marks = vec![EventMark::Accent, EventMark::Staccato];
        let text = Event::Pitched(e.clone()).project();
        assert!(Event::parse(&text).is_err(), "out of order");
        e.marks = vec![EventMark::Accent, EventMark::Accent];
        let text = Event::Pitched(e).project();
        assert!(Event::parse(&text).is_err(), "repeated");
    }

    #[test]
    fn indeterminacy_kind_round_trips_including_nested_compound() {
        assert_eq!(IndeterminacyKind::Choice.project().render(), "choice");
        assert_eq!(
            IndeterminacyKind::Compound(vec![
                IndeterminacyKind::Pitch,
                IndeterminacyKind::Compound(vec![IndeterminacyKind::Duration]),
            ])
            .project()
            .render(),
            "(compound (pitch (compound (duration))))"
        );
        for k in [
            IndeterminacyKind::Pitch,
            IndeterminacyKind::Duration,
            IndeterminacyKind::Choice,
            IndeterminacyKind::Compound(vec![IndeterminacyKind::Pitch, IndeterminacyKind::Choice]),
        ] {
            round_trip(k);
        }
    }

    #[test]
    fn trajectory_endpoint_round_trips_both_variants() {
        round_trip(TrajectoryEndpoint::EventPitch(PitchId::new(replica(), 7)));
        round_trip(TrajectoryEndpoint::ExplicitPitch(identified_pitch(3)));
    }

    #[test]
    fn trajectory_shape_round_trips_every_variant() {
        assert_eq!(TrajectoryShape::Curve.project().render(), "curve");
        for shape in [
            TrajectoryShape::Linear,
            TrajectoryShape::Exponential,
            TrajectoryShape::Curve,
            TrajectoryShape::Stepwise(vec![identified_pitch(1), identified_pitch(2)]),
        ] {
            round_trip(shape);
        }
    }

    /// `Event` is `(<variant> <payload>)`, not the payload alone.
    #[test]
    fn event_projection_wraps_the_payload_in_the_variant_name() {
        let text = rest_event(1).project().render();
        assert!(
            text.starts_with("(rest (rest "),
            "expected `(rest (rest …`, got {text}"
        );
        let pitched = pitched_event(1).project().render();
        assert!(
            pitched.starts_with("(pitched (pitched-event "),
            "expected `(pitched (pitched-event …`, got {pitched}"
        );
    }

    #[test]
    fn event_round_trips_a_rest_and_a_pitched_event() {
        round_trip(rest_event(4));
        round_trip(pitched_event(9));
    }

    #[test]
    fn event_arena_round_trips_in_ascending_id_order() {
        let mut arena = EventArena::new();
        // Inserted out of id order; projection must still be ascending.
        for c in [5u64, 1, 9, 3] {
            arena.insert(rest_event(c)).unwrap();
        }
        round_trip(arena);
    }

    /// A text whose events are not in ascending `EventId` order must be rejected,
    /// not silently re-sorted the way [`EventArena::insert`] would.
    #[test]
    fn event_arena_rejects_events_out_of_id_order() {
        let descending = Sexp::List(vec![rest_event(3).project(), rest_event(1).project()]);
        assert_eq!(
            EventArena::parse(&descending),
            Err(TextError::NotStrictlyIncreasing(
                "EventArena events must be in ascending EventId order"
            ))
        );
    }

    /// A duplicate `EventId` breaks strict ascension and is rejected there, before
    /// it can reach `insert`.
    #[test]
    fn event_arena_rejects_a_duplicate_id() {
        let duplicated = Sexp::List(vec![rest_event(2).project(), rest_event(2).project()]);
        assert_eq!(
            EventArena::parse(&duplicated),
            Err(TextError::NotStrictlyIncreasing(
                "EventArena events must be in ascending EventId order"
            ))
        );
    }

    /// A pitched event with no pitches is a well-formed projection of an invalid
    /// value; the arena rejects it rather than admitting it.
    #[test]
    fn event_arena_rejects_an_empty_pitched_event() {
        let empty = Event::Pitched(PitchedEvent {
            id: EventId::new(replica(), 1),
            voice: voice(),
            position: EventPosition::Musical(MusicalPosition::origin()),
            duration: EventDuration::Musical(MusicalDuration::whole()),
            pitches: vec![],
            marks: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: StemConfiguration,
            grace: None,
        });
        let s = Sexp::List(vec![empty.project()]);
        assert_eq!(
            EventArena::parse(&s),
            Err(TextError::NotCanonical(
                "a pitched event must have at least one pitch"
            ))
        );
    }
}
