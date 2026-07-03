//! Small builders for the **value-typed** graph values that v1 operation
//! payloads now embed (Operation Catalog).
//!
//! Before the catalog, payloads carried only identifiers, so a test or fuzz
//! harness could mint an `InsertEvent` from a bare `EventId`. v1 payloads carry
//! the real [`Event`], [`PitchSpelling`], cross-cutting structure, and
//! [`TimeAnchor`], so harnesses need a deterministic way to build a faithful
//! value from a handful of ids. These builders are that single source — used by
//! the reduction fuzzer, the migration regression guard, the in-crate tests, and
//! (re-exported) by `epiphany-testkit`'s generators — so the value shapes stay
//! consistent everywhere. They are intentionally simple (a default C-octave-4
//! pitch space, whole-note durations unless given) — the catalog defines the
//! schema, not these helpers.

use std::collections::BTreeMap;

use epiphany_core::{
    AcousticPitch, AcousticRealization, AleatoricAnchoringDiscipline, AleatoricTimeModel,
    AnchorOffset, Beam, BeamId, CmnNominal, Event, EventId, EventOrderingDAG, EventPosition,
    IdentifiedPitch, MetricTimeModel, MusicalDuration, MusicalPosition, Pitch, PitchId,
    PitchSpaceId, PitchSpacePosition, PitchSpelling, PitchedEvent, ProportionalTimeModel, Region,
    RegionContent, RegionEdge, RegionId, RegionTimeModel, Rest, ScalePosition, Slur, SlurId,
    SpellingAttachment, SpellingDirective, SpellingScope, SpellingSource, StaffBasedContent,
    StaffExtent, StaffId, StaffInstance, StaffInstanceId, StemConfiguration, Tie, TieClass, TieId,
    TimeAnchor, TimeExtent, Voice, VoiceId, VoiceOrigin, WallClockDuration, WallClockTime,
};

/// A deterministic, fully-specified C4 pitch in the cmn-12 space — the neutral
/// pitch value an identified pitch wraps when a harness only has the id.
pub fn pitch_value() -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: 0,
                octave: 4,
            },
        },
        acoustic: AcousticPitch {
            tuning: epiphany_core::TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

/// An [`IdentifiedPitch`] with the given id and the neutral [`pitch_value`].
pub fn identified_pitch(id: PitchId) -> IdentifiedPitch {
    IdentifiedPitch {
        id,
        pitch: pitch_value(),
    }
}

/// A distinct CMN [`Pitch`] per `nth`: nominal = `nth % 7`, octave = `nth / 7`.
/// Unlike [`spelling`] (which fixes the octave, so distinct `nth` can collide),
/// this is injective over the whole `u8`, letting a harness make concurrent
/// `ModifyIdentifiedPitch`es agree or conflict deterministically.
pub fn pitch_value_nth(nth: u8) -> Pitch {
    let nominal = match nth % 7 {
        0 => CmnNominal::C,
        1 => CmnNominal::D,
        2 => CmnNominal::E,
        3 => CmnNominal::F,
        4 => CmnNominal::G,
        5 => CmnNominal::A,
        _ => CmnNominal::B,
    };
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Cmn {
                nominal,
                alteration: 0,
                octave: (nth / 7) as i8,
            },
        },
        acoustic: AcousticPitch {
            tuning: epiphany_core::TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

/// The event an InsertEvent inserts: a pitched event when `pitch_ids` is
/// non-empty, otherwise a visible rest. Mirrors the prototype's
/// pitched-or-rest split, now as a real value.
pub fn insert_event_value(
    id: EventId,
    voice: VoiceId,
    position: MusicalPosition,
    duration: MusicalDuration,
    pitch_ids: &[PitchId],
) -> Event {
    let position = EventPosition::Musical(position);
    let duration = epiphany_core::EventDuration::Musical(duration);
    if pitch_ids.is_empty() {
        Event::Rest(Rest {
            id,
            voice,
            position,
            duration,
            vertical_position: None,
            visible: true,
        })
    } else {
        Event::Pitched(PitchedEvent {
            id,
            voice,
            position,
            duration,
            pitches: pitch_ids.iter().copied().map(identified_pitch).collect(),
            articulations: Vec::new(),
            dynamic: None,
            ornaments: Vec::new(),
            stem: StemConfiguration,
            grace: None,
        })
    }
}

/// A bare replacement [`Rest`] value (tuplet compensation) of the given duration.
pub fn rest_value(id: EventId, voice: VoiceId, duration: MusicalDuration) -> Rest {
    Rest {
        id,
        voice,
        position: EventPosition::Musical(MusicalPosition::origin()),
        duration: epiphany_core::EventDuration::Musical(duration),
        vertical_position: None,
        visible: true,
    }
}

/// A distinct CMN spelling per `nth`, **injective over the full `u8`**: the
/// nominal is `nth mod 7` and the octave is `nth / 7`, so two distinct `nth`
/// always yield distinct [`PitchSpelling`] values. This lets a harness make
/// concurrent respellings agree or conflict deterministically without having to
/// keep its selector constants within any small range (the earlier `mod 7`-only
/// form silently collapsed congruent selectors). The octave is a test token, not
/// a musically meaningful register.
pub fn spelling(nth: u8) -> PitchSpelling {
    let nominal = match nth % 7 {
        0 => CmnNominal::C,
        1 => CmnNominal::D,
        2 => CmnNominal::E,
        3 => CmnNominal::F,
        4 => CmnNominal::G,
        5 => CmnNominal::A,
        _ => CmnNominal::B,
    };
    PitchSpelling::cmn(nominal, (nth / 7) as i8)
}

/// A [`Slur`] over two event endpoints.
pub fn slur(id: SlurId, start: EventId, end: EventId) -> Slur {
    Slur {
        id,
        start_event: start,
        end_event: end,
    }
}

/// A [`Tie`] over two event endpoints (laissez-vibrer class, no pitch pairing).
pub fn tie(id: TieId, start: EventId, end: EventId) -> Tie {
    Tie {
        id,
        start_event: start,
        end_event: end,
        pitch_pairing: None,
        class: TieClass::LaissezVibrer,
    }
}

/// A level-1 [`Beam`] over a run of events.
pub fn beam(id: BeamId, events: Vec<EventId>) -> Beam {
    Beam {
        id,
        events,
        level: 1,
    }
}

/// A region-start [`TimeAnchor`] at the given musical offset — the anchor a
/// system-break advisory uses; its resolved position is `offset`.
pub fn region_start_anchor(region: RegionId, offset: MusicalPosition) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge: RegionEdge::Start,
        offset: AnchorOffset::Musical(MusicalDuration(offset.0)),
    }
}

/// The default metric region time model.
pub fn metric_model() -> RegionTimeModel {
    RegionTimeModel::Metric(MetricTimeModel::default())
}

/// A minimal proportional region time model.
pub fn proportional_model() -> RegionTimeModel {
    RegionTimeModel::Proportional(ProportionalTimeModel {
        duration: WallClockDuration(1),
    })
}

/// A minimal aleatoric region time model (freely-mixed, empty bounds).
pub fn aleatoric_model() -> RegionTimeModel {
    RegionTimeModel::Aleatoric(AleatoricTimeModel {
        ordering: EventOrderingDAG::default(),
        anchoring: AleatoricAnchoringDiscipline::FreelyMixed,
        bounds: BTreeMap::new(),
        duration_hint: WallClockDuration(1),
    })
}

/// An empty, user-declared [`Voice`] (M2c) — the container a `CreateVoice` mints
/// before any event is inserted into it.
pub fn voice(id: VoiceId) -> Voice {
    Voice {
        id,
        events: Vec::new(),
        default_stem_direction: None,
        is_primary: false,
        origin: VoiceOrigin::UserDeclared,
    }
}

/// An empty [`StaffInstance`] (M2c) over the given global `staff` — the container
/// a `CreateStaffInstance` mints before any voice is created in it.
pub fn staff_instance(id: StaffInstanceId, staff: StaffId) -> StaffInstance {
    StaffInstance {
        id,
        staff,
        voices: Vec::new(),
        clef_sequence: Vec::new(),
        key_sequence: Vec::new(),
        local_metric_grid: None,
        measures: Vec::new(),
        instrument_override: None,
        staff_lines_override: None,
        visible: true,
    }
}

/// An empty metric [`Region`] (M2c) — the container a `CreateRegion` mints before
/// any staff instance is added to it. Carries no staff instances and an empty
/// staff extent (a region with no instances is reference-clean, Chapter 5).
pub fn region(id: RegionId) -> Region {
    Region {
        id,
        time_model: metric_model(),
        content: RegionContent::StaffBased(StaffBasedContent {
            staff_instances: Vec::new(),
            ..Default::default()
        }),
        // A far-future wall-clock extent so a freshly-created (initially empty)
        // region does not overlap an existing region in both time and staff once
        // a staff instance is added (Chapter 5 RegionExtents).
        time_extent: TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(1_000_000_000),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(1_000_001_000),
            },
        },
        staff_extent: StaffExtent { staves: Vec::new() },
        local_tempo_map: None,
    }
}

/// Score metadata with a `nth`-distinct title (M2d) — distinct `nth` give
/// distinct `ScoreMetadata` values so a harness can drive concurrent
/// `SetMetadata`s, an advisory LWW field that resolves by canonical order with
/// no conflict.
pub fn score_metadata(nth: u8) -> epiphany_core::ScoreMetadata {
    epiphany_core::ScoreMetadata {
        title: Some(format!("title-{nth}")),
        composer: Some("composer".to_string()),
        copyright: None,
    }
}

/// An empty metric grid (M2d) — no meter changes, hence anchor-free and
/// reference-clean (Chapter 5). The container a `SetMetricGrid` sets on a region.
pub fn metric_grid() -> epiphany_core::MetricGrid {
    epiphany_core::MetricGrid {
        meter_sequence: Vec::new(),
    }
}

/// A minimal global [`Staff`](epiphany_core::Staff) (Phase-3 tranche) — the
/// value a `CreateStaff` mints: a five-line staff named for its counter,
/// referencing `instrument`, with no abbreviation or group.
pub fn staff(id: StaffId, instrument: epiphany_core::InstrumentId) -> epiphany_core::Staff {
    epiphany_core::Staff {
        id,
        name: format!("staff-{}", id.counter()),
        abbreviation: None,
        instrument,
        default_staff_lines: epiphany_core::StaffLineConfiguration::default(),
        group: None,
    }
}

/// A well-formed `numerator`/4 [`TimeSignature`](epiphany_core::TimeSignature)
/// (Phase-3 tranche): `numerator` quarter-note beat groups summing exactly to
/// the measure duration, so [`epiphany_core::TimeSignature::new`]'s beat-group
/// invariant holds by construction. Distinct numerators give distinct values,
/// letting a harness drive the set-union mint's identical/differing re-carry
/// branches deterministically.
pub fn time_signature(
    id: epiphany_core::TimeSignatureId,
    numerator: u16,
) -> epiphany_core::TimeSignature {
    let numerator = numerator.max(1);
    let quarter = MusicalDuration(epiphany_core::RationalTime::new(1, 4).expect("1/4 is valid"));
    let measure = MusicalDuration(
        epiphany_core::RationalTime::new(numerator as i64, 4).expect("n/4 is valid"),
    );
    let beat_groups = (0..numerator)
        .map(|i| epiphany_core::BeatGroup {
            duration: quarter.clone(),
            subdivision: None,
            accent: u8::from(i == 0),
        })
        .collect();
    epiphany_core::TimeSignature::new(
        id,
        epiphany_core::TimeSignatureDisplay::Standard {
            numerator,
            denominator: epiphany_core::PowerOfTwo::new(4).expect("4 is a power of two"),
        },
        measure,
        beat_groups,
    )
    .expect("beat groups sum to the measure duration by construction")
}

/// A constant [`TempoSegment`](epiphany_core::TempoSegment) (Phase-3 tranche)
/// starting at the given region-relative musical position, open-ended, at
/// `bpm` quarter-note beats per minute. Its start anchor resolves (under the
/// operation layer's coarse anchor resolution) to exactly `start`, so it
/// satisfies `SetTempoSegment`'s start-key agreement precondition when keyed
/// by the same anchor.
pub fn tempo_segment(
    region: RegionId,
    start: MusicalPosition,
    bpm: f64,
) -> epiphany_core::TempoSegment {
    epiphany_core::TempoSegment {
        start: region_start_anchor(region, start),
        end: None,
        start_tempo: epiphany_core::Tempo::quarter(bpm).expect("positive finite bpm"),
        end_tempo: None,
        shape: epiphany_core::TempoShape::Constant,
    }
}

/// An explicit, user-chosen per-pitch [`SpellingAttachment`] — the engraved-layer
/// spelling a materialized score carries after a `RespellPitch`. The v0 → v1
/// migration recovers a respell's spelling from exactly these attachments
/// ([`crate::migrate_v0_envelope`]).
pub fn explicit_spelling_attachment(pitch: PitchId, spelling: PitchSpelling) -> SpellingAttachment {
    SpellingAttachment {
        scope: SpellingScope::Pitch(pitch),
        directive: SpellingDirective::Explicit(spelling),
        source: SpellingSource::UserChosen,
        priority: 0,
        layer: None,
    }
}
