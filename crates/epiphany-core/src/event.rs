//! The event taxonomy and the event arena (Chapter 5 §"The Event Arena").
//!
//! Events are the rhythmic atoms of the score. All events of all kinds live in
//! a single flat [`EventArena`] owned by the score; voices, cross-cutting
//! structures, and analysis layers hold [`EventId`]s, not events (Chapter 5
//! §"Design Principles": "Arena storage"). Every event carries its own
//! identifier, its voice membership, and its position, stored on the event so
//! operations can resolve context without traversing parents.
//!
//! ## Storage backend (QUICKSTART decision 2)
//!
//! The arena is a [`slotmap::SlotMap`] of events plus a hash index from
//! [`EventId`] to the slot key. The slotmap gives generation-checked
//! stale-handle detection (matching the spec's identifier-stability
//! requirement) and the index gives the required `O(1)`-amortized lookup by
//! `EventId` (Chapter 5 §"The Event Arena"). Canonical iteration is by
//! ascending `EventId` (Appendix D §"Ordered Iteration").
//!
//! ## Forward-declared engraving types
//!
//! The decoration payloads on events (articulations, dynamics, ornaments, stem
//! configuration, staff positions) are *introduced informally here and fully
//! defined in Chapter 7* (the spec says so explicitly); they belong to Agent E
//! (`epiphany-layout-ir`). This module carries minimal placeholders for them so
//! the event records have their Chapter 5 shape without pre-empting Chapter 7.

use std::collections::HashMap;

use slotmap::{new_key_type, SlotMap};

use crate::ids::{EventId, GraphicObjectId, VoiceId};
use crate::pitch::{AccidentalId, IdentifiedPitch};
use crate::time::{DurationBounds, EventDuration, EventPosition};

// --- Forward-declared engraving placeholders (Chapter 7 / Agent E). ---------

/// A staff line/space position for unpitched events and explicitly-placed
/// rests (Chapter 5). Placeholder: the full vertical model is Chapter 7's.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct StaffPosition(pub i16);

/// Reference to an unpitched instrument member (snare, kick, …) for sound
/// mapping (Chapter 5). Placeholder for the audio-engine mapping.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct UnpitchedMemberId(pub u32);

/// A mark written on a note or an unpitched note (schema major 5): an
/// articulation, a string, brass or jazz technique, a tremolo, or an
/// arpeggio. The event's `marks` are a set, one per kind, in ascending order
/// of [`EventMark::tag`]; the codec refuses any other order or a repeat, so
/// two authors writing the same marks write equal values ([`canonical_marks`]).
/// Placement above or below the note is the engraver's, not stored.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum EventMark {
    Staccato,
    Staccatissimo,
    Spiccato,
    Tenuto,
    /// Louré: a tenuto line over a staccato dot.
    DetachedLegato,
    Accent,
    /// The strong accent (MusicXML's `strong-accent`).
    Marcato,
    Stress,
    Unstress,
    UpBow,
    DownBow,
    Harmonic,
    OpenString,
    /// The closed "+" (MusicXML's `stopped`, a brass mute closed).
    Stopped,
    SnapPizzicato,
    Scoop,
    Plop,
    Doit,
    Falloff,
    /// A single-note tremolo of `strokes` strokes through the stem.
    Tremolo {
        strokes: u8,
    },
    /// A two-note tremolo between this event and the next event of its voice,
    /// of `strokes` strokes. The pairing is positional, as a standard tie's
    /// is, so the mark names no other object.
    TremoloWithNext {
        strokes: u8,
    },
    /// An arpeggio sign before a chord.
    Arpeggio {
        direction: ArpeggioDirection,
    },
}

impl EventMark {
    /// The mark's kind as its wire tag: the set's order and its uniqueness
    /// key.
    pub fn tag(&self) -> u8 {
        match self {
            EventMark::Staccato => 0,
            EventMark::Staccatissimo => 1,
            EventMark::Spiccato => 2,
            EventMark::Tenuto => 3,
            EventMark::DetachedLegato => 4,
            EventMark::Accent => 5,
            EventMark::Marcato => 6,
            EventMark::Stress => 7,
            EventMark::Unstress => 8,
            EventMark::UpBow => 9,
            EventMark::DownBow => 10,
            EventMark::Harmonic => 11,
            EventMark::OpenString => 12,
            EventMark::Stopped => 13,
            EventMark::SnapPizzicato => 14,
            EventMark::Scoop => 15,
            EventMark::Plop => 16,
            EventMark::Doit => 17,
            EventMark::Falloff => 18,
            EventMark::Tremolo { .. } => 19,
            EventMark::TremoloWithNext { .. } => 20,
            EventMark::Arpeggio { .. } => 21,
        }
    }
}

/// The direction an arpeggio rolls: a plain wavy line, or one with an arrow.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ArpeggioDirection {
    Plain,
    Up,
    Down,
}

/// `marks` as the canonical set an event holds: ascending by
/// [`EventMark::tag`], the first of each kind kept.
pub fn canonical_marks(marks: impl IntoIterator<Item = EventMark>) -> Vec<EventMark> {
    let mut out: Vec<EventMark> = Vec::new();
    for mark in marks {
        if !out.iter().any(|m| m.tag() == mark.tag()) {
            out.push(mark);
        }
    }
    out.sort_by_key(EventMark::tag);
    out
}

/// A single-event dynamic marking. Reserved since schema major 5: a dynamic is
/// a point mark (`MarkerKind::Dynamic`), anchored where it stands, so this
/// slot stays `None` (Chapter 7 placeholder).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct DynamicMark;

/// An ornament on a note (schema major 5), with the accidentals written above
/// or below it. A note's `ornaments` are a set, one per kind, ascending by
/// kind ([`canonical_ornaments`]); the codec refuses any other order or a
/// repeat. A trill's wavy extension is a spanner, not part of the ornament.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Ornament {
    pub kind: OrnamentKind,
    pub accidental_above: Option<AccidentalId>,
    pub accidental_below: Option<AccidentalId>,
}

/// An ornament's kind; its order is the order of an event's ornament set.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum OrnamentKind {
    Trill,
    /// The lower mordent.
    Mordent,
    /// The upper mordent (pralltriller).
    InvertedMordent,
    Turn,
    InvertedTurn,
}

/// `ornaments` as the canonical set a note holds: ascending by kind, the
/// first of each kind kept.
pub fn canonical_ornaments(ornaments: impl IntoIterator<Item = Ornament>) -> Vec<Ornament> {
    let mut out: Vec<Ornament> = Vec::new();
    for ornament in ornaments {
        if !out.iter().any(|o| o.kind == ornament.kind) {
            out.push(ornament);
        }
    }
    out.sort_by_key(|o| o.kind);
    out
}

/// Stem configuration (direction, length adjustment, hidden). Placeholder
/// (Chapter 7).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct StemConfiguration;

/// Hints constraining an indeterminate event. Placeholder for the full hint
/// model; carries the duration bounds and alternative-event references, which
/// are structural.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct IndeterminacyHints {
    pub duration_bounds: Option<DurationBounds>,
    pub alternatives: Vec<EventId>,
    pub textual_instruction: Option<String>,
}

/// Visual representation of a trajectory. Placeholder (Chapter 7).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct TrajectoryDisplay;

/// A playback parameter binding for graphic content. Placeholder (audio
/// engine).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct PlaybackBinding;

/// Rendering hints for a cue. Placeholder (Chapter 7).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct CueRendering;

// --- The event taxonomy (Chapter 5 §"The Event Type"). ----------------------

/// A grace note's kind (Chapter 5): slashed or not.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GraceKind {
    /// Slashed.
    Acciaccatura,
    Appoggiatura,
}

/// What makes an event a grace note (schema major 5). A grace note has zero
/// duration, so it occupies no time in its voice; it stands at the position
/// of the event it precedes, its principal, which is found by position, not
/// stored. Its notated value and dots are held here, since the duration is
/// zero, and `order` places the graces at one position, before the event of
/// positive duration there (then by id). Playback's steal-time is not modeled.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Grace {
    pub kind: GraceKind,
    pub value: crate::graph::NoteValue,
    pub dots: u8,
    pub order: u16,
}

/// A pitched event: one or more identified pitches sounding together
/// (Chapter 5 §"Pitched Events"). A single pitch is a note; multiple pitches
/// are a chord.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PitchedEvent {
    pub id: EventId,
    pub voice: VoiceId,
    pub position: EventPosition,
    pub duration: EventDuration,
    /// One or more identified pitches. Must be non-empty (use [`Rest`] for the
    /// no-pitch case); enforced by [`PitchedEvent::is_well_formed`].
    pub pitches: Vec<IdentifiedPitch>,
    /// The marks written on the note, a canonical set ([`EventMark`]).
    pub marks: Vec<EventMark>,
    /// Reserved: always `None` (see [`DynamicMark`]).
    pub dynamic: Option<DynamicMark>,
    /// The ornaments on the note, a canonical set ([`Ornament`]).
    pub ornaments: Vec<Ornament>,
    pub stem: StemConfiguration,
    /// `Some` for a grace note, which has zero duration ([`Grace`]).
    pub grace: Option<Grace>,
}

impl PitchedEvent {
    /// A pitched event must have at least one pitch (Chapter 5: "Empty pitch
    /// lists are forbidden").
    pub fn is_well_formed(&self) -> bool {
        !self.pitches.is_empty()
    }
}

/// An unpitched percussion event (Chapter 5 §"Unpitched Events").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UnpitchedEvent {
    pub id: EventId,
    pub voice: VoiceId,
    pub position: EventPosition,
    pub duration: EventDuration,
    pub staff_position: StaffPosition,
    pub instrument_member: UnpitchedMemberId,
    /// The marks written on the note, a canonical set ([`EventMark`]).
    pub marks: Vec<EventMark>,
    /// Reserved: always `None` (see [`DynamicMark`]).
    pub dynamic: Option<DynamicMark>,
    pub stem: StemConfiguration,
    /// `Some` for a grace note, which has zero duration ([`Grace`]).
    pub grace: Option<Grace>,
}

/// A rest: a duration without a sounding event (Chapter 5 §"Rests").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Rest {
    pub id: EventId,
    pub voice: VoiceId,
    pub position: EventPosition,
    pub duration: EventDuration,
    pub vertical_position: Option<StaffPosition>,
    pub visible: bool,
}

/// What aspect of an indeterminate event is indeterminate (Chapter 5).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum IndeterminacyKind {
    Pitch,
    Duration,
    Choice,
    Compound(Vec<IndeterminacyKind>),
}

/// An event whose pitch, duration, or choice is deliberately indeterminate
/// (Chapter 5 §"Indeterminate Events").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IndeterminateEvent {
    pub id: EventId,
    pub voice: VoiceId,
    pub position: EventPosition,
    pub duration: EventDuration,
    pub indeterminacy: IndeterminacyKind,
    pub hints: IndeterminacyHints,
}

/// An endpoint of a trajectory: a reference to another event's pitch, or an
/// explicit pitch local to the trajectory (Chapter 5 §"Trajectory Events").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TrajectoryEndpoint {
    /// A pitch belonging to another event.
    EventPitch(crate::ids::PitchId),
    /// An explicit identified pitch local to this trajectory; participates in
    /// spelling and identity like an embedded pitch.
    ExplicitPitch(IdentifiedPitch),
}

/// The shape of a trajectory between its endpoints (Chapter 5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TrajectoryShape {
    Linear,
    Exponential,
    /// Control points (placeholder geometry; full type in Chapter 7).
    Curve,
    /// A stepwise sequence of identified pitches.
    Stepwise(Vec<IdentifiedPitch>),
}

/// A continuous pitch motion: glissando, portamento, or pitch bend
/// (Chapter 5 §"Trajectory Events").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TrajectoryEvent {
    pub id: EventId,
    pub voice: VoiceId,
    pub position: EventPosition,
    pub duration: EventDuration,
    pub start: TrajectoryEndpoint,
    pub end: TrajectoryEndpoint,
    pub shape: TrajectoryShape,
    pub display: TrajectoryDisplay,
}

/// An event whose primary content is graphic (Chapter 5 §"Graphic Events").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GraphicEvent {
    pub id: EventId,
    pub voice: VoiceId,
    pub position: EventPosition,
    pub duration: EventDuration,
    pub graphics: Vec<GraphicObjectId>,
    pub playback_bindings: Vec<PlaybackBinding>,
}

/// A small-print rendering of music from another voice/instrument
/// (Chapter 5 §"Cue Events").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CueEvent {
    pub id: EventId,
    pub voice: VoiceId,
    pub position: EventPosition,
    pub duration: EventDuration,
    pub source: Vec<EventId>,
    pub rendering: CueRendering,
}

/// A rhythmic event of any kind (Chapter 5 §"The Event Type").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Event {
    Pitched(PitchedEvent),
    Unpitched(UnpitchedEvent),
    Rest(Rest),
    Indeterminate(IndeterminateEvent),
    Trajectory(TrajectoryEvent),
    Graphic(GraphicEvent),
    Cue(CueEvent),
}

impl Event {
    /// The event's identifier.
    pub fn id(&self) -> EventId {
        match self {
            Event::Pitched(e) => e.id,
            Event::Unpitched(e) => e.id,
            Event::Rest(e) => e.id,
            Event::Indeterminate(e) => e.id,
            Event::Trajectory(e) => e.id,
            Event::Graphic(e) => e.id,
            Event::Cue(e) => e.id,
        }
    }

    /// The voice that owns this event.
    pub fn voice(&self) -> VoiceId {
        match self {
            Event::Pitched(e) => e.voice,
            Event::Unpitched(e) => e.voice,
            Event::Rest(e) => e.voice,
            Event::Indeterminate(e) => e.voice,
            Event::Trajectory(e) => e.voice,
            Event::Graphic(e) => e.voice,
            Event::Cue(e) => e.voice,
        }
    }

    /// The event's position within its voice and region.
    pub fn position(&self) -> &EventPosition {
        match self {
            Event::Pitched(e) => &e.position,
            Event::Unpitched(e) => &e.position,
            Event::Rest(e) => &e.position,
            Event::Indeterminate(e) => &e.position,
            Event::Trajectory(e) => &e.position,
            Event::Graphic(e) => &e.position,
            Event::Cue(e) => &e.position,
        }
    }

    /// The event's duration.
    pub fn duration(&self) -> &EventDuration {
        match self {
            Event::Pitched(e) => &e.duration,
            Event::Unpitched(e) => &e.duration,
            Event::Rest(e) => &e.duration,
            Event::Indeterminate(e) => &e.duration,
            Event::Trajectory(e) => &e.duration,
            Event::Graphic(e) => &e.duration,
            Event::Cue(e) => &e.duration,
        }
    }

    /// The event's grace payload, if it is a grace note (schema major 5).
    pub fn grace(&self) -> Option<&Grace> {
        match self {
            Event::Pitched(e) => e.grace.as_ref(),
            Event::Unpitched(e) => e.grace.as_ref(),
            _ => None,
        }
    }

    /// Whether the event's duration is zero, as a grace note's is.
    pub fn has_zero_duration(&self) -> bool {
        matches!(self.duration(), EventDuration::Musical(d) if d.0.is_zero())
    }

    /// Sets the voice membership (used by generators and edit operations).
    pub fn set_voice(&mut self, voice: VoiceId) {
        match self {
            Event::Pitched(e) => e.voice = voice,
            Event::Unpitched(e) => e.voice = voice,
            Event::Rest(e) => e.voice = voice,
            Event::Indeterminate(e) => e.voice = voice,
            Event::Trajectory(e) => e.voice = voice,
            Event::Graphic(e) => e.voice = voice,
            Event::Cue(e) => e.voice = voice,
        }
    }

    /// Sets the event's region-local position (used by time-model migration
    /// during canonical operation reduction).
    pub fn set_position(&mut self, position: EventPosition) {
        match self {
            Event::Pitched(e) => e.position = position,
            Event::Unpitched(e) => e.position = position,
            Event::Rest(e) => e.position = position,
            Event::Indeterminate(e) => e.position = position,
            Event::Trajectory(e) => e.position = position,
            Event::Graphic(e) => e.position = position,
            Event::Cue(e) => e.position = position,
        }
    }

    /// Appends references to every [`IdentifiedPitch`] this event embeds:
    /// chord pitches for [`Event::Pitched`], and explicit/stepwise pitches for
    /// [`Event::Trajectory`]. Used by the pitch-uniqueness invariant.
    pub fn collect_identified_pitches<'a>(&'a self, out: &mut Vec<&'a IdentifiedPitch>) {
        match self {
            Event::Pitched(e) => out.extend(e.pitches.iter()),
            Event::Trajectory(e) => {
                for ep in [&e.start, &e.end] {
                    if let TrajectoryEndpoint::ExplicitPitch(ip) = ep {
                        out.push(ip);
                    }
                }
                if let TrajectoryShape::Stepwise(ps) = &e.shape {
                    out.extend(ps.iter());
                }
            }
            _ => {}
        }
    }
}

new_key_type! {
    /// A generation-checked handle into the [`EventArena`]'s slot storage.
    /// Stale handles are detected by the slotmap rather than silently aliasing
    /// a recycled slot (the identifier-stability discipline of Chapter 5).
    pub struct EventKey;
}

/// Why an arena insertion failed.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ArenaError {
    /// An event with this identifier already exists. Identifiers are unique
    /// within a score (Chapter 5 invariant 11) and never reused.
    DuplicateId(EventId),
    /// A [`PitchedEvent`] was inserted with no pitches. "A `PitchedEvent` MUST
    /// have at least one pitch. Empty pitch lists are forbidden; use `Rest` for
    /// the no-pitch case." (Chapter 5 §"Pitched Events".) Enforced at the
    /// construction boundary so a malformed pitched event never enters the
    /// arena in the first place.
    EmptyPitchedEvent(EventId),
}

impl core::fmt::Display for ArenaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ArenaError::DuplicateId(id) => write!(f, "duplicate EventId {id:?} in arena"),
            ArenaError::EmptyPitchedEvent(id) => {
                write!(f, "pitched event {id:?} has no pitches (Chapter 5)")
            }
        }
    }
}
impl std::error::Error for ArenaError {}

/// The flat arena of all events in a score (Chapter 5 §"The Event Arena").
///
/// Observable contract: `O(1)`-amortized lookup by [`EventId`] and stable
/// handles for the lifetime of the event. Canonical iteration
/// ([`EventArena::iter_canonical`]) is by ascending `EventId`.
#[derive(Clone, Debug, Default)]
pub struct EventArena {
    slots: SlotMap<EventKey, Event>,
    by_id: HashMap<EventId, EventKey>,
}

impl PartialEq for EventArena {
    fn eq(&self, other: &Self) -> bool {
        let ids = self.ids_canonical();
        ids == other.ids_canonical() && ids.into_iter().all(|id| self.get(id) == other.get(id))
    }
}

impl Eq for EventArena {}

impl EventArena {
    /// An empty arena.
    pub fn new() -> Self {
        EventArena {
            slots: SlotMap::with_key(),
            by_id: HashMap::new(),
        }
    }

    /// Inserts an event, rejecting a duplicate identifier (invariant 11) and a
    /// pitched event with no pitches (Chapter 5 §"Pitched Events"). Enforcing
    /// the latter here keeps `PitchedEvent::is_well_formed` from being merely
    /// advisory: an empty pitched event cannot enter the arena.
    pub fn insert(&mut self, event: Event) -> Result<EventKey, ArenaError> {
        let id = event.id();
        if let Event::Pitched(p) = &event {
            if !p.is_well_formed() {
                return Err(ArenaError::EmptyPitchedEvent(id));
            }
        }
        if self.by_id.contains_key(&id) {
            return Err(ArenaError::DuplicateId(id));
        }
        let key = self.slots.insert(event);
        self.by_id.insert(id, key);
        Ok(key)
    }

    /// `O(1)`-amortized lookup by identifier.
    pub fn get(&self, id: EventId) -> Option<&Event> {
        self.by_id.get(&id).and_then(|k| self.slots.get(*k))
    }

    /// Mutable lookup by identifier.
    pub fn get_mut(&mut self, id: EventId) -> Option<&mut Event> {
        match self.by_id.get(&id) {
            Some(k) => self.slots.get_mut(*k),
            None => None,
        }
    }

    /// Whether an event with this identifier is live in the arena.
    pub fn contains(&self, id: EventId) -> bool {
        self.by_id.contains_key(&id)
    }

    /// Removes an event by identifier (the live-storage side of a tombstone;
    /// tombstone *tracking* is `epiphany-ops`, Chapter 6). The identifier is
    /// never re-minted, so re-inserting the same id is a caller error.
    pub fn remove(&mut self, id: EventId) -> Option<Event> {
        let key = self.by_id.remove(&id)?;
        self.slots.remove(key)
    }

    /// Identifiers whose index entry is inconsistent: the `by_id` key no longer
    /// resolves, or resolves to an event whose own `id()` differs from the key.
    /// This catches post-insertion corruption via [`EventArena::get_mut`] (e.g.
    /// mutating an event's id), which the safe [`EventArena::insert`] path
    /// cannot produce. Empty for a well-formed arena.
    pub fn index_inconsistencies(&self) -> Vec<EventId> {
        self.by_id
            .iter()
            .filter(|(id, key)| self.slots.get(**key).map(|e| e.id()) != Some(**id))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Live pitched events that are malformed (no pitches). [`EventArena::insert`]
    /// rejects these, but [`EventArena::get_mut`] could clear a chord's pitches
    /// after insertion; this re-checks (Chapter 5 §"Pitched Events").
    pub fn malformed_pitched_events(&self) -> Vec<EventId> {
        self.slots
            .values()
            .filter_map(|e| match e {
                Event::Pitched(p) if !p.is_well_formed() => Some(p.id),
                _ => None,
            })
            .collect()
    }

    /// Number of live events.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the arena holds no live events.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Iterates live events in unspecified (storage) order. Use
    /// [`EventArena::iter_canonical`] where canonical output depends on order.
    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.slots.values()
    }

    /// Live event identifiers, ascending (Appendix D §"Ordered Iteration":
    /// lexicographic on the identifier's canonical byte form, which for these
    /// identifiers is numeric order).
    pub fn ids_canonical(&self) -> Vec<EventId> {
        let mut ids: Vec<EventId> = self.by_id.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Iterates live events in canonical [`EventId`] order.
    pub fn iter_canonical(&self) -> impl Iterator<Item = &Event> + '_ {
        self.ids_canonical()
            .into_iter()
            .map(move |id| self.get(id).expect("indexed id resolves"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ReplicaId;
    use crate::pitch::{
        AcousticPitch, AcousticRealization, CmnNominal, Pitch, PitchSpaceId, PitchSpacePosition,
        ScalePosition, TuningReference,
    };
    use crate::time::MusicalDuration;
    use crate::time::{MusicalPosition, RationalTime};

    fn rest(id: EventId, voice: VoiceId, beat: i64) -> Event {
        Event::Rest(Rest {
            id,
            voice,
            position: EventPosition::Musical(MusicalPosition(RationalTime::new(beat, 4).unwrap())),
            duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
            vertical_position: None,
            visible: true,
        })
    }

    #[test]
    fn arena_lookup_and_duplicate_rejection() {
        let mut arena = EventArena::new();
        let r = ReplicaId(1);
        let v = VoiceId::new(r, 100);
        let e0 = EventId::new(r, 0);
        let e1 = EventId::new(r, 1);
        arena.insert(rest(e0, v, 0)).unwrap();
        arena.insert(rest(e1, v, 1)).unwrap();
        assert_eq!(arena.len(), 2);
        assert!(arena.contains(e0));
        assert_eq!(arena.get(e0).unwrap().id(), e0);
        // Duplicate id rejected.
        assert_eq!(
            arena.insert(rest(e0, v, 2)),
            Err(ArenaError::DuplicateId(e0))
        );
    }

    #[test]
    fn empty_pitched_event_is_rejected_at_insertion() {
        let mut arena = EventArena::new();
        let r = ReplicaId(1);
        let v = VoiceId::new(r, 1);
        let eid = EventId::new(r, 0);
        let empty = Event::Pitched(PitchedEvent {
            id: eid,
            voice: v,
            position: EventPosition::Musical(MusicalPosition::origin()),
            duration: EventDuration::Musical(MusicalDuration::whole()),
            pitches: vec![],
            marks: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: StemConfiguration,
            grace: None,
        });
        assert_eq!(arena.insert(empty), Err(ArenaError::EmptyPitchedEvent(eid)));
        assert!(arena.is_empty());
    }

    #[test]
    fn canonical_iteration_is_by_ascending_id() {
        let mut arena = EventArena::new();
        let r = ReplicaId(1);
        let v = VoiceId::new(r, 100);
        // Insert out of id order.
        for c in [5u64, 1, 9, 3] {
            arena.insert(rest(EventId::new(r, c), v, c as i64)).unwrap();
        }
        let ids: Vec<u64> = arena.iter_canonical().map(|e| e.id().counter()).collect();
        assert_eq!(ids, vec![1, 3, 5, 9]);
    }

    #[test]
    fn collects_embedded_and_trajectory_pitches() {
        let r = ReplicaId(1);
        let v = VoiceId::new(r, 1);
        let mk_ip = |c: u64| IdentifiedPitch {
            id: crate::ids::PitchId::new(r, c),
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
        };
        let chord = Event::Pitched(PitchedEvent {
            id: EventId::new(r, 0),
            voice: v,
            position: EventPosition::Musical(MusicalPosition::origin()),
            duration: EventDuration::Musical(MusicalDuration::whole()),
            pitches: vec![mk_ip(10), mk_ip(11)],
            marks: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: StemConfiguration,
            grace: None,
        });
        let mut out = Vec::new();
        chord.collect_identified_pitches(&mut out);
        assert_eq!(out.len(), 2);
    }
}
