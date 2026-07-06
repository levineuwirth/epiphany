//! **F3 — the representative score corpus + kind-by-kind eligibility-taxonomy
//! harness** (Agent F, for Agent H; `spec/PHASE2_F_WEEK0_WORKLIST.md` item F3,
//! `spec/PHASE2_QUICKSTART.md` §H "What H produces, by event kind").
//!
//! H's whole acceptance rests on "F's representative corpus (>20 fixtures)
//! spanning common / edge / torture cases" exercising the event-kind
//! **eligibility taxonomy** — and H develops against it, not just is tested by
//! it. This module is that corpus.
//!
//! Every fixture is:
//! * **deterministic** — built from a fixed replica seed, no platform entropy
//!   (Appendix D §"Randomness"), so a failure reproduces exactly; and
//! * **`check_invariants`-clean** — asserted by `every_fixture_is_invariant_clean`
//!   in this module's tests, so a malformed fixture can never mask a real H
//!   regression.
//!
//! The harness does **not** re-derive the taxonomy: it runs Agent H's own
//! [`epiphany_core::derive_annotations`] and reads its
//! [`epiphany_core::TaxonomyReport`], then (a) **independently recounts** events
//! by kind and cross-checks H's counts (so a miscount is caught, not trusted),
//! and (b) aggregates per-bucket counts across the corpus and asserts every
//! taxonomy bucket is non-empty or explicitly deferred — "ineligible" is then
//! explicit and counted, never silently absent.

use std::collections::BTreeSet;

use epiphany_core::{
    check_invariants, derive_annotations, AleatoricAnchoringDiscipline, AleatoricTimeModel, Canvas,
    CmnNominal, CueEvent, CueRendering, DerivedAnnotations, Event, EventArena, EventDuration,
    EventPosition, GraceKind, GraphicEvent, IdentifiedPitch, IdentityContext, IndeterminacyHints,
    IndeterminacyKind, IndeterminateEvent, Instrument, MetricTimeModel, MusicalDuration,
    MusicalPosition, PitchSpelling, PowerOfTwo, PrePassProfile, ProportionalTimeModel, Region,
    RegionContent, RegionTimeModel, Score, SpellingAttachment, SpellingDirective, SpellingNominal,
    SpellingScope, SpellingSource, Staff, StaffBasedContent, StaffExtent, StaffInstance,
    StaffLineConfiguration, StaffPosition, StemConfiguration, TaxonomyReport, TimeAnchor,
    TimeExtent, TimeSignature, TimeSignatureDisplay, TrajectoryDisplay, TrajectoryEndpoint,
    TrajectoryEvent, TrajectoryShape, Tuplet, TupletRatio, UnpitchedEvent, UnpitchedMemberId,
    Voice, WallClockDuration, WallClockTime,
};
use epiphany_core::{
    AccidentalId, AcousticPitch, AcousticRealization, BeatGroup, EventId, InstrumentId, Pitch,
    PitchId, PitchSpaceId, PitchSpacePosition, RegionId, ReplicaId, ScalePosition, StaffId,
    StaffInstanceId, TimeSignatureId, TuningReference, TupletId, VoiceId,
};

// ===========================================================================
// The eligibility-taxonomy buckets
// ===========================================================================

/// One bucket of [`TaxonomyReport`]. Every event and every embedded pitch lands
/// in exactly the buckets that apply; the corpus must populate each (or list it
/// in [`DEFERRED_BUCKETS`] with a reason).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Bucket {
    PitchedEvents,
    UnpitchedEvents,
    RestEvents,
    TrajectoryEvents,
    GraphicEvents,
    IndeterminateEvents,
    CueEvents,
    SpellingsInferred,
    SpellingsAuthored,
    SpellingUnavailable,
    DecompositionsInferred,
    DecompositionSkippedNonmusical,
    DecompositionDeferredNonmetric,
    DecompositionInapplicable,
    DecompositionUngriddable,
}

impl Bucket {
    /// All buckets, in declaration order.
    pub fn all() -> [Bucket; 15] {
        use Bucket::*;
        [
            PitchedEvents,
            UnpitchedEvents,
            RestEvents,
            TrajectoryEvents,
            GraphicEvents,
            IndeterminateEvents,
            CueEvents,
            SpellingsInferred,
            SpellingsAuthored,
            SpellingUnavailable,
            DecompositionsInferred,
            DecompositionSkippedNonmusical,
            DecompositionDeferredNonmetric,
            DecompositionInapplicable,
            DecompositionUngriddable,
        ]
    }

    /// This bucket's count in a report.
    pub fn count(self, t: &TaxonomyReport) -> usize {
        use Bucket::*;
        match self {
            PitchedEvents => t.pitched_events,
            UnpitchedEvents => t.unpitched_events,
            RestEvents => t.rest_events,
            TrajectoryEvents => t.trajectory_events,
            GraphicEvents => t.graphic_events,
            IndeterminateEvents => t.indeterminate_events,
            CueEvents => t.cue_events,
            SpellingsInferred => t.spellings_inferred,
            SpellingsAuthored => t.spellings_authored,
            SpellingUnavailable => t.spelling_unavailable,
            DecompositionsInferred => t.decompositions_inferred,
            DecompositionSkippedNonmusical => t.decomposition_skipped_nonmusical,
            DecompositionDeferredNonmetric => t.decomposition_deferred_nonmetric,
            DecompositionInapplicable => t.decomposition_inapplicable,
            DecompositionUngriddable => t.decomposition_ungriddable,
        }
    }

    pub fn label(self) -> &'static str {
        use Bucket::*;
        match self {
            PitchedEvents => "pitched_events",
            UnpitchedEvents => "unpitched_events",
            RestEvents => "rest_events",
            TrajectoryEvents => "trajectory_events",
            GraphicEvents => "graphic_events",
            IndeterminateEvents => "indeterminate_events",
            CueEvents => "cue_events",
            SpellingsInferred => "spellings_inferred",
            SpellingsAuthored => "spellings_authored",
            SpellingUnavailable => "spelling_unavailable",
            DecompositionsInferred => "decompositions_inferred",
            DecompositionSkippedNonmusical => "decomposition_skipped_nonmusical",
            DecompositionDeferredNonmetric => "decomposition_deferred_nonmetric",
            DecompositionInapplicable => "decomposition_inapplicable",
            DecompositionUngriddable => "decomposition_ungriddable",
        }
    }
}

/// Buckets the clean corpus is *not* required to populate, each with a written
/// reason. Empty today: every bucket is reachable by invariant-clean input and
/// is exercised by a dedicated fixture (see `DECISIONS.md` F3). If a future
/// invariant change makes a bucket unreachable, list it here rather than
/// dropping the coverage assertion.
pub const DEFERRED_BUCKETS: &[Bucket] = &[];

/// The "unusual outcome" buckets: loss, deferral, inapplicability, or
/// unavailability. Unlike the success buckets (`*Events`, `Spellings*`,
/// `DecompositionsInferred`), which vary freely, a fixture may populate one of
/// these **only if it declares it** in [`Fixture::expect`] — the corpus harness
/// enforces that exactly (`classify_corpus` step 5b). This makes `expect` an
/// exact whitelist for precisely the buckets where a silent *under-emission*
/// regression would otherwise hide: an event that should have decomposed but
/// instead slips into `ungriddable`/`skipped` lights up an undeclared bucket,
/// rather than vanishing while the success count merely shrinks and the
/// event-total cross-check still balances.
const UNUSUAL_BUCKETS: &[Bucket] = &[
    Bucket::SpellingUnavailable,
    Bucket::DecompositionSkippedNonmusical,
    Bucket::DecompositionDeferredNonmetric,
    Bucket::DecompositionInapplicable,
    Bucket::DecompositionUngriddable,
];

/// The fixture tier: common notation, edge cases, and torture cases — the three
/// the acceptance criterion names.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Tier {
    Common,
    Edge,
    Torture,
}

/// One tagged corpus fixture: a deterministic builder plus the taxonomy buckets
/// it is asserted to populate.
pub struct Fixture {
    pub name: &'static str,
    pub tier: Tier,
    pub build: fn() -> Score,
    /// Buckets this fixture must drive non-zero (validated per-fixture, and
    /// aggregated for corpus coverage). For the crate-private `UNUSUAL_BUCKETS` this list is
    /// also an *exact whitelist*: a fixture that lands an event in a
    /// loss/deferral bucket it did not declare fails the harness.
    pub expect: &'static [Bucket],
}

// ===========================================================================
// The corpus
// ===========================================================================

/// The representative corpus: ≥20 invariant-clean fixtures spanning the
/// eligibility taxonomy across common / edge / torture tiers, plus the existing
/// positive generators so F's taxonomy harness runs over the same graphs Agents
/// H and I develop and render against.
pub fn corpus() -> Vec<Fixture> {
    use Bucket::*;
    use Tier::*;
    vec![
        // ---- Common: ordinary tonal notation. -----------------------------
        Fixture {
            name: "c_major_scale",
            tier: Common,
            build: fx_c_major_scale,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "d_major_scale",
            tier: Common,
            build: fx_d_major_scale,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "b_flat_major_scale",
            tier: Common,
            build: fx_b_flat_major_scale,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "c_triad_chord",
            tier: Common,
            build: fx_c_triad_chord,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "notes_and_rests",
            tier: Common,
            build: fx_notes_and_rests,
            expect: &[
                PitchedEvents,
                RestEvents,
                SpellingsInferred,
                DecompositionsInferred,
            ],
        },
        Fixture {
            name: "dotted_half_and_quarter",
            tier: Common,
            build: fx_dotted_half_and_quarter,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "two_voice_counterpoint",
            tier: Common,
            build: fx_two_voice_counterpoint,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "meter_three_four",
            tier: Common,
            build: fx_meter_three_four,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        // ---- Edge: rhythmic and spelling difficulty. ----------------------
        Fixture {
            name: "syncopation_offbeat",
            tier: Edge,
            build: fx_syncopation_offbeat,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "mixed_rhythm",
            tier: Edge,
            build: fx_mixed_rhythm,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "tie_across_barline",
            tier: Edge,
            build: fx_tie_across_barline,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "triplet_eighths",
            tier: Edge,
            build: fx_triplet_eighths,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "quintuplet_sixteenths",
            tier: Edge,
            build: fx_quintuplet_sixteenths,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "unpitched_percussion",
            tier: Edge,
            build: fx_unpitched_percussion,
            expect: &[UnpitchedEvents, DecompositionsInferred],
        },
        Fixture {
            name: "respell_override",
            tier: Edge,
            build: fx_respell_override,
            expect: &[PitchedEvents, SpellingsAuthored, DecompositionsInferred],
        },
        Fixture {
            name: "ascending_chromatic_run",
            tier: Edge,
            build: fx_ascending_chromatic_run,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        // ---- Torture: non-pitched kinds, deferred regions, ungriddable. ----
        Fixture {
            name: "ji_spelling_unavailable",
            tier: Torture,
            build: fx_ji_spelling_unavailable,
            expect: &[PitchedEvents, SpellingUnavailable, DecompositionsInferred],
        },
        Fixture {
            name: "proportional_region",
            tier: Torture,
            build: fx_proportional_region,
            expect: &[
                PitchedEvents,
                SpellingsInferred,
                DecompositionDeferredNonmetric,
            ],
        },
        Fixture {
            name: "aleatoric_region",
            tier: Torture,
            build: fx_aleatoric_region,
            expect: &[
                PitchedEvents,
                SpellingsInferred,
                DecompositionDeferredNonmetric,
            ],
        },
        Fixture {
            name: "trajectory_glissando",
            tier: Torture,
            build: fx_trajectory_glissando,
            expect: &[
                TrajectoryEvents,
                SpellingsInferred,
                DecompositionInapplicable,
            ],
        },
        Fixture {
            name: "graphic_event",
            tier: Torture,
            build: fx_graphic_event,
            expect: &[GraphicEvents, DecompositionInapplicable],
        },
        Fixture {
            name: "indeterminate_event",
            tier: Torture,
            build: fx_indeterminate_event,
            expect: &[IndeterminateEvents, DecompositionInapplicable],
        },
        Fixture {
            name: "cue_event",
            tier: Torture,
            build: fx_cue_event,
            expect: &[CueEvents, PitchedEvents, DecompositionInapplicable],
        },
        Fixture {
            name: "off_grid_ungriddable",
            tier: Torture,
            build: fx_off_grid_ungriddable,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionUngriddable],
        },
        Fixture {
            name: "sub_sixtyfourth_ungriddable",
            tier: Torture,
            build: fx_sub_sixtyfourth_ungriddable,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionUngriddable],
        },
        Fixture {
            name: "grace_zero_duration",
            tier: Torture,
            build: fx_grace_zero_duration,
            expect: &[
                PitchedEvents,
                SpellingsInferred,
                DecompositionSkippedNonmusical,
            ],
        },
        // ---- Existing positive generators, re-used so the taxonomy harness
        //      runs over the exact graphs H and I build/render against. -------
        Fixture {
            name: "gen_valid_score",
            tier: Common,
            build: fx_gen_valid_score,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
        Fixture {
            name: "gen_valid_score_rich",
            tier: Torture,
            build: fx_gen_valid_score_rich,
            expect: &[PitchedEvents, DecompositionDeferredNonmetric],
        },
        Fixture {
            name: "gen_ten_measure_single_staff",
            tier: Common,
            build: fx_gen_ten_measure_single_staff,
            expect: &[PitchedEvents, SpellingsInferred, DecompositionsInferred],
        },
    ]
}

// ===========================================================================
// Construction helpers
// ===========================================================================

/// A one-region, one-staff score under construction. Single-region builders
/// cover the whole taxonomy; the few multi-region cases come from the wrapped
/// generators.
struct OneStaff {
    idc: IdentityContext,
    staff: StaffId,
    instrument: InstrumentId,
    region: RegionId,
    instance: StaffInstanceId,
    arena: EventArena,
    voices: Vec<Voice>,
    cross_cutting: epiphany_core::CrossCuttingRegistry,
    spelling_attachments: Vec<SpellingAttachment>,
    time_signatures: Vec<TimeSignature>,
}

impl OneStaff {
    fn new(replica: u64) -> Self {
        let mut idc = IdentityContext::new(ReplicaId(replica));
        let staff: StaffId = idc.mint();
        let instrument: InstrumentId = idc.mint();
        let region: RegionId = idc.mint();
        let instance: StaffInstanceId = idc.mint();
        OneStaff {
            idc,
            staff,
            instrument,
            region,
            instance,
            arena: EventArena::new(),
            voices: Vec::new(),
            cross_cutting: epiphany_core::CrossCuttingRegistry::default(),
            spelling_attachments: Vec::new(),
            time_signatures: Vec::new(),
        }
    }

    /// Mints a fresh voice and returns its id.
    fn voice(&mut self) -> VoiceId {
        let id: VoiceId = self.idc.mint();
        self.voices.push(Voice::user(id));
        id
    }

    /// Inserts an event into the arena and appends it to `voice`'s ordered list.
    /// Callers add events in ascending position order (invariant 3).
    fn add(&mut self, voice: VoiceId, e: Event) {
        let eid = e.id();
        self.arena.insert(e).expect("fresh event id");
        self.voices
            .iter_mut()
            .find(|v| v.id == voice)
            .expect("voice exists")
            .events
            .push(eid);
    }

    /// Assembles the score under `time_model`.
    fn finish(self, time_model: RegionTimeModel) -> Score {
        let OneStaff {
            idc,
            staff,
            instrument,
            region,
            instance,
            arena,
            voices,
            cross_cutting,
            spelling_attachments,
            time_signatures,
        } = self;

        let mut si = StaffInstance::new(instance, staff);
        si.voices = voices;
        let region_obj = Region {
            id: region,
            time_model,
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![si],
                ..Default::default()
            }),
            time_extent: TimeExtent {
                start: TimeAnchor::WallClock {
                    time: WallClockTime(0),
                },
                end: TimeAnchor::WallClock {
                    time: WallClockTime(10_000_000),
                },
            },
            staff_extent: StaffExtent {
                staves: vec![staff],
            },
            local_tempo_map: None,
        };

        let mut score = Score::empty(idc.clone());
        score.identity = idc;
        score.staves = vec![Staff {
            id: staff,
            name: String::from("F-corpus"),
            abbreviation: None,
            instrument,
            default_staff_lines: StaffLineConfiguration::default(),
            group: None,
        }];
        score.instruments = vec![Instrument {
            id: instrument,
            name: String::from("F-corpus"),
        }];
        score.events = arena;
        score.cross_cutting = cross_cutting;
        score.spelling_attachments = spelling_attachments;
        score.time_signatures = time_signatures;
        score.canvas = Canvas {
            regions: vec![region_obj],
            ..Default::default()
        };
        score
    }
}

fn metric() -> RegionTimeModel {
    RegionTimeModel::Metric(MetricTimeModel::default())
}

fn proportional() -> RegionTimeModel {
    RegionTimeModel::Proportional(ProportionalTimeModel {
        duration: WallClockDuration(1_000_000),
    })
}

fn aleatoric_musical() -> RegionTimeModel {
    RegionTimeModel::Aleatoric(AleatoricTimeModel {
        ordering: Default::default(),
        anchoring: AleatoricAnchoringDiscipline::Musical,
        bounds: Default::default(),
        duration_hint: WallClockDuration(1_000_000),
    })
}

/// A `cmn-12` integer (chromatic / 12-EDO) pitch at absolute 12-TET `semitone`
/// (C4 = 48). The spelling algorithm must *decide* its spelling — there is no
/// authored CMN letter to preserve.
fn integer_pitch(semitone: i32) -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Integer {
                space_size: 12,
                index: semitone,
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

/// A just-intonation pitch whose 12-TET class is not determinable from the
/// position alone: spelling unavailable.
fn ji_pitch() -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("ji-5limit"),
            position: PitchSpacePosition::JiVector {
                components: vec![0, 1, -1],
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::Implicit,
        },
    }
}

fn ipitch(id: PitchId, pitch: Pitch) -> IdentifiedPitch {
    IdentifiedPitch { id, pitch }
}

fn mpos(n: i64, d: i64) -> EventPosition {
    EventPosition::Musical(MusicalPosition(rt(n, d)))
}
fn mdur(n: i64, d: i64) -> EventDuration {
    EventDuration::Musical(MusicalDuration(rt(n, d)))
}
fn rt(n: i64, d: i64) -> epiphany_core::RationalTime {
    epiphany_core::RationalTime::new(n, d).unwrap()
}

fn pitched(
    id: EventId,
    voice: VoiceId,
    position: EventPosition,
    duration: EventDuration,
    pitches: Vec<IdentifiedPitch>,
) -> Event {
    Event::Pitched(epiphany_core::PitchedEvent {
        id,
        voice,
        position,
        duration,
        pitches,
        articulations: vec![],
        dynamic: None,
        ornaments: vec![],
        stem: StemConfiguration,
        grace: None,
    })
}

/// A single integer-pitched note: mints the event + pitch and adds it.
fn add_note(
    b: &mut OneStaff,
    voice: VoiceId,
    pos: EventPosition,
    dur: EventDuration,
    semitone: i32,
) {
    let eid: EventId = b.idc.mint();
    let pid: PitchId = b.idc.mint();
    b.add(
        voice,
        pitched(
            eid,
            voice,
            pos,
            dur,
            vec![ipitch(pid, integer_pitch(semitone))],
        ),
    );
}

// ===========================================================================
// Common-tier fixtures
// ===========================================================================

fn metric_line(replica: u64, semitones: &[i32]) -> Score {
    let mut b = OneStaff::new(replica);
    let v = b.voice();
    for (i, &s) in semitones.iter().enumerate() {
        add_note(&mut b, v, mpos(i as i64, 4), mdur(1, 4), s);
    }
    b.finish(metric())
}

fn fx_c_major_scale() -> Score {
    // C4 D4 E4 F4 G4 A4 B4 C5 — all naturals (line-of-fifths stays diatonic).
    metric_line(0xF001, &[48, 50, 52, 53, 55, 57, 59, 60])
}

fn fx_d_major_scale() -> Score {
    // D major: D E F# G A B C# D — sharps in a sharp context.
    metric_line(0xF002, &[50, 52, 54, 55, 57, 59, 61, 62])
}

fn fx_b_flat_major_scale() -> Score {
    // Bb major: Bb C D Eb F G A — flats in a flat context.
    metric_line(0xF003, &[46, 48, 50, 51, 53, 55, 57])
}

fn fx_c_triad_chord() -> Score {
    // A C-E-G triad as one event (chord), plus a following G — all spelled.
    let mut b = OneStaff::new(0xF004);
    let v = b.voice();
    let eid: EventId = b.idc.mint();
    let p1: PitchId = b.idc.mint();
    let p2: PitchId = b.idc.mint();
    let p3: PitchId = b.idc.mint();
    b.add(
        v,
        pitched(
            eid,
            v,
            mpos(0, 4),
            mdur(1, 4),
            vec![
                ipitch(p1, integer_pitch(48)),
                ipitch(p2, integer_pitch(52)),
                ipitch(p3, integer_pitch(55)),
            ],
        ),
    );
    add_note(&mut b, v, mpos(1, 4), mdur(1, 4), 55);
    b.finish(metric())
}

fn fx_notes_and_rests() -> Score {
    // Note, rest, note, rest — rests decompose but are not spelled.
    let mut b = OneStaff::new(0xF005);
    let v = b.voice();
    add_note(&mut b, v, mpos(0, 4), mdur(1, 4), 48);
    let r1: EventId = b.idc.mint();
    b.add(v, rest_ev(r1, v, mpos(1, 4), mdur(1, 4)));
    add_note(&mut b, v, mpos(2, 4), mdur(1, 4), 52);
    let r2: EventId = b.idc.mint();
    b.add(v, rest_ev(r2, v, mpos(3, 4), mdur(1, 4)));
    b.finish(metric())
}

fn fx_dotted_half_and_quarter() -> Score {
    // Dotted half (3/4 on the downbeat) then a quarter — exercises dots.
    let mut b = OneStaff::new(0xF006);
    let v = b.voice();
    add_note(&mut b, v, mpos(0, 4), mdur(3, 4), 48);
    add_note(&mut b, v, mpos(3, 4), mdur(1, 4), 50);
    b.finish(metric())
}

fn fx_two_voice_counterpoint() -> Score {
    // Two voices in one staff instance: independent melodic runs, each spelled.
    let mut b = OneStaff::new(0xF007);
    let upper = b.voice();
    let lower = b.voice();
    for (i, &s) in [48, 50, 52, 53].iter().enumerate() {
        add_note(&mut b, upper, mpos(i as i64, 4), mdur(1, 4), s);
    }
    for (i, &s) in [36, 38, 40, 41].iter().enumerate() {
        add_note(&mut b, lower, mpos(i as i64, 4), mdur(1, 4), s);
    }
    b.finish(metric())
}

fn fx_meter_three_four() -> Score {
    // 3/4: a declared time signature drives the measure length (exercises H's
    // meter-resolution path past the whole-note default).
    let mut b = OneStaff::new(0xF008);
    let ts_id: TimeSignatureId = b.idc.mint();
    let ts = TimeSignature::new(
        ts_id,
        TimeSignatureDisplay::Standard {
            numerator: 3,
            denominator: PowerOfTwo::new(4).unwrap(),
        },
        MusicalDuration(rt(3, 4)),
        vec![beat_quarter(), beat_quarter(), beat_quarter()],
    )
    .expect("3/4 beat groups sum to 3/4");
    b.time_signatures.push(ts);
    let v = b.voice();
    for (i, &s) in [48, 50, 52].iter().enumerate() {
        add_note(&mut b, v, mpos(i as i64, 4), mdur(1, 4), s);
    }
    b.finish(metric())
}

// ===========================================================================
// Edge-tier fixtures
// ===========================================================================

fn fx_syncopation_offbeat() -> Score {
    // [1/8, 1/2): an eighth tied to a quarter — a multi-component decomposition.
    let mut b = OneStaff::new(0xF101);
    let v = b.voice();
    add_note(&mut b, v, mpos(1, 8), mdur(3, 8), 48);
    b.finish(metric())
}

fn fx_mixed_rhythm() -> Score {
    // One bar mixing three note values (eighth, quarter, half) with a syncopated
    // eighth-tied-to-quarter. A single fixture that both spans ≥2 note values *and*
    // produces a tied multi-component split, so the non-vacuity spread checks keep
    // margin beyond the corpus's other rhythmic fixtures.
    let mut b = OneStaff::new(0xF205);
    let v = b.voice();
    add_note(&mut b, v, mpos(0, 8), mdur(1, 8), 48); // eighth on the downbeat
    add_note(&mut b, v, mpos(1, 8), mdur(3, 8), 50); // syncopation: eighth + quarter
    add_note(&mut b, v, mpos(1, 2), mdur(1, 2), 52); // half on beat 3
    b.finish(metric())
}

fn fx_tie_across_barline() -> Score {
    // A half note on beat 4 of 4/4 crosses the barline: quarter tied to quarter.
    let mut b = OneStaff::new(0xF102);
    let v = b.voice();
    add_note(&mut b, v, mpos(3, 4), mdur(1, 2), 48);
    b.finish(metric())
}

fn fx_triplet_eighths() -> Score {
    // A 3:2 eighth-note triplet (each member sounds 1/12, notates as an eighth).
    let mut b = OneStaff::new(0xF103);
    let v = b.voice();
    let tid: TupletId = b.idc.mint();
    let mut members = Vec::new();
    for k in 0..3i64 {
        let eid: EventId = b.idc.mint();
        let pid: PitchId = b.idc.mint();
        members.push(eid);
        b.add(
            v,
            pitched(
                eid,
                v,
                mpos(k, 12),
                mdur(1, 12),
                vec![ipitch(pid, integer_pitch(48 + k as i32))],
            ),
        );
    }
    b.cross_cutting.tuplets.push(Tuplet {
        id: tid,
        ratio: TupletRatio::new(3, 2).unwrap(),
        members,
        parent: None,
        required_total: MusicalDuration(rt(1, 4)),
    });
    b.finish(metric())
}

fn fx_quintuplet_sixteenths() -> Score {
    // A 5:4 quintuplet of sixteenths (each sounds 1/20, notates as a sixteenth).
    let mut b = OneStaff::new(0xF104);
    let v = b.voice();
    let tid: TupletId = b.idc.mint();
    let mut members = Vec::new();
    for k in 0..5i64 {
        let eid: EventId = b.idc.mint();
        let pid: PitchId = b.idc.mint();
        members.push(eid);
        b.add(
            v,
            pitched(
                eid,
                v,
                mpos(k, 20),
                mdur(1, 20),
                vec![ipitch(pid, integer_pitch(48 + k as i32))],
            ),
        );
    }
    b.cross_cutting.tuplets.push(Tuplet {
        id: tid,
        ratio: TupletRatio::new(5, 4).unwrap(),
        members,
        parent: None,
        required_total: MusicalDuration(rt(1, 4)),
    });
    b.finish(metric())
}

fn fx_unpitched_percussion() -> Score {
    // Snare-line unpitched events: decomposed, but never pitch-spelled.
    let mut b = OneStaff::new(0xF105);
    let v = b.voice();
    for i in 0..4i64 {
        let eid: EventId = b.idc.mint();
        b.add(
            v,
            Event::Unpitched(UnpitchedEvent {
                id: eid,
                voice: v,
                position: mpos(i, 4),
                duration: mdur(1, 4),
                staff_position: StaffPosition(2),
                instrument_member: UnpitchedMemberId(1),
                articulations: vec![],
                dynamic: None,
                stem: StemConfiguration,
                grace: None,
            }),
        );
    }
    b.finish(metric())
}

fn fx_respell_override() -> Score {
    // An integer pc-6 note (which the algorithm spells one way) carries an
    // authored UserChosen override that must win (precedence): F#4.
    let mut b = OneStaff::new(0xF106);
    let v = b.voice();
    let eid: EventId = b.idc.mint();
    let pid: PitchId = b.idc.mint();
    b.add(
        v,
        pitched(
            eid,
            v,
            mpos(0, 4),
            mdur(1, 4),
            vec![ipitch(pid, integer_pitch(54))],
        ),
    );
    b.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(PitchSpelling {
            nominal: SpellingNominal::Cmn(CmnNominal::F),
            accidentals: vec![AccidentalId::new("sharp")],
            octave: 4,
            render_hints: Default::default(),
        }),
        source: SpellingSource::UserChosen,
        priority: 0,
        layer: None,
    });
    b.finish(metric())
}

fn fx_ascending_chromatic_run() -> Score {
    // A rising chromatic line: the direction tiebreak prefers sharps.
    metric_line(0xF107, &[48, 49, 50, 51, 52])
}

// ===========================================================================
// Torture-tier fixtures
// ===========================================================================

fn fx_ji_spelling_unavailable() -> Score {
    // A JI-vector pitch (spelling unavailable) alongside a cmn-12 pitch (spelled).
    let mut b = OneStaff::new(0xF201);
    let v = b.voice();
    add_note(&mut b, v, mpos(0, 4), mdur(1, 4), 48);
    let eid: EventId = b.idc.mint();
    let pid: PitchId = b.idc.mint();
    b.add(
        v,
        pitched(
            eid,
            v,
            mpos(1, 4),
            mdur(1, 4),
            vec![ipitch(pid, ji_pitch())],
        ),
    );
    b.finish(metric())
}

fn fx_proportional_region() -> Score {
    // Wall-clock events in a proportional region: pitches still spell (identity
    // is region-independent); decomposition is deferred (non-metric).
    let mut b = OneStaff::new(0xF202);
    let v = b.voice();
    for k in 0..3i64 {
        let eid: EventId = b.idc.mint();
        let pid: PitchId = b.idc.mint();
        b.add(
            v,
            Event::Pitched(epiphany_core::PitchedEvent {
                id: eid,
                voice: v,
                position: EventPosition::WallClock(WallClockTime(k * 1000)),
                duration: EventDuration::WallClock(WallClockDuration(1000)),
                pitches: vec![ipitch(pid, integer_pitch(48 + 2 * k as i32))],
                articulations: vec![],
                dynamic: None,
                ornaments: vec![],
                stem: StemConfiguration,
                grace: None,
            }),
        );
    }
    b.finish(proportional())
}

fn fx_aleatoric_region() -> Score {
    // A musical-discipline aleatoric region: decomposition deferred, pitches
    // still spelled.
    let mut b = OneStaff::new(0xF203);
    let v = b.voice();
    for k in 0..3i64 {
        add_note(&mut b, v, mpos(k, 4), mdur(1, 4), 48 + 2 * k as i32);
    }
    b.finish(aleatoric_musical())
}

fn fx_trajectory_glissando() -> Score {
    // A glissando whose endpoints are explicit local pitches: those pitches are
    // spelled; the trajectory event itself is not decomposed (inapplicable).
    let mut b = OneStaff::new(0xF204);
    let v = b.voice();
    let eid: EventId = b.idc.mint();
    let p_start: PitchId = b.idc.mint();
    let p_end: PitchId = b.idc.mint();
    b.add(
        v,
        Event::Trajectory(TrajectoryEvent {
            id: eid,
            voice: v,
            position: mpos(0, 4),
            duration: mdur(1, 4),
            start: TrajectoryEndpoint::ExplicitPitch(ipitch(p_start, integer_pitch(48))),
            end: TrajectoryEndpoint::ExplicitPitch(ipitch(p_end, integer_pitch(55))),
            shape: TrajectoryShape::Linear,
            display: TrajectoryDisplay,
        }),
    );
    b.finish(metric())
}

fn fx_graphic_event() -> Score {
    // A graphic event with no referenced objects: a kind that never decomposes
    // or spells (counted as inapplicable).
    let mut b = OneStaff::new(0xF205);
    let v = b.voice();
    let eid: EventId = b.idc.mint();
    b.add(
        v,
        Event::Graphic(GraphicEvent {
            id: eid,
            voice: v,
            position: mpos(0, 4),
            duration: mdur(1, 4),
            graphics: vec![],
            playback_bindings: vec![],
        }),
    );
    b.finish(metric())
}

fn fx_indeterminate_event() -> Score {
    // An indeterminate-pitch event (determinate duration): not spelled, not
    // decomposed (inapplicable).
    let mut b = OneStaff::new(0xF206);
    let v = b.voice();
    let eid: EventId = b.idc.mint();
    b.add(
        v,
        Event::Indeterminate(IndeterminateEvent {
            id: eid,
            voice: v,
            position: mpos(0, 4),
            duration: mdur(1, 4),
            indeterminacy: IndeterminacyKind::Pitch,
            hints: IndeterminacyHints::default(),
        }),
    );
    b.finish(metric())
}

fn fx_cue_event() -> Score {
    // A cue referencing a real (live) source event: the cue itself is
    // inapplicable for spelling/decomposition; the sourced pitched note spells.
    let mut b = OneStaff::new(0xF207);
    let v = b.voice();
    let src: EventId = b.idc.mint();
    let pid: PitchId = b.idc.mint();
    b.add(
        v,
        pitched(
            src,
            v,
            mpos(0, 4),
            mdur(1, 4),
            vec![ipitch(pid, integer_pitch(48))],
        ),
    );
    let cue: EventId = b.idc.mint();
    b.add(
        v,
        Event::Cue(CueEvent {
            id: cue,
            voice: v,
            position: mpos(1, 4),
            duration: mdur(1, 4),
            source: vec![src],
            rendering: CueRendering,
        }),
    );
    b.finish(metric())
}

fn fx_off_grid_ungriddable() -> Score {
    // A metric note whose position is off the dyadic grid (1/3) — its duration
    // is a clean quarter, but it cannot be barline-aligned, so decomposition is
    // honestly ungriddable while the pitch still spells.
    let mut b = OneStaff::new(0xF208);
    let v = b.voice();
    add_note(&mut b, v, mpos(1, 3), mdur(1, 4), 48);
    b.finish(metric())
}

fn fx_sub_sixtyfourth_ungriddable() -> Score {
    // A 1/128 duration — an exact grid multiple but below the sixty-fourth
    // floor: ungriddable (never silently dropped), pitch still spells.
    let mut b = OneStaff::new(0xF209);
    let v = b.voice();
    add_note(&mut b, v, mpos(0, 4), mdur(1, 128), 48);
    b.finish(metric())
}

fn fx_grace_zero_duration() -> Score {
    // A zero-duration grace note: a determinate musical duration with nothing to
    // decompose (skipped as non-musical), but the pitch still spells.
    let mut b = OneStaff::new(0xF20A);
    let v = b.voice();
    let eid: EventId = b.idc.mint();
    let pid: PitchId = b.idc.mint();
    b.add(
        v,
        Event::Pitched(epiphany_core::PitchedEvent {
            id: eid,
            voice: v,
            position: mpos(0, 4),
            duration: mdur(0, 1),
            pitches: vec![ipitch(pid, integer_pitch(49))],
            articulations: vec![],
            dynamic: None,
            ornaments: vec![],
            stem: StemConfiguration,
            grace: Some(GraceKind::Acciaccatura),
        }),
    );
    b.finish(metric())
}

// ---- Wrapped positive generators (fixed seeds). ----------------------------

fn fx_gen_valid_score() -> Score {
    epiphany_core::generators::valid_score(0xF301)
}
fn fx_gen_valid_score_rich() -> Score {
    epiphany_core::generators::valid_score_rich(0xF302)
}
fn fx_gen_ten_measure_single_staff() -> Score {
    crate::fixtures::ten_measure_single_staff(0xF303)
}

/// A one-note metric score (a `cmn-12` integer pitch, pc 6) together with the id
/// of its single pitch — for the merge gate's precedence harness to attach
/// `RespellPitch`-style overrides to.
pub fn override_probe() -> (Score, PitchId) {
    let mut b = OneStaff::new(0xF401);
    let v = b.voice();
    let eid: EventId = b.idc.mint();
    let pid: PitchId = b.idc.mint();
    b.add(
        v,
        pitched(
            eid,
            v,
            mpos(0, 4),
            mdur(1, 4),
            vec![ipitch(pid, integer_pitch(54))],
        ),
    );
    (b.finish(metric()), pid)
}

fn rest_ev(id: EventId, voice: VoiceId, position: EventPosition, duration: EventDuration) -> Event {
    Event::Rest(epiphany_core::Rest {
        id,
        voice,
        position,
        duration,
        vertical_position: None,
        visible: true,
    })
}

fn beat_quarter() -> BeatGroup {
    BeatGroup {
        duration: MusicalDuration(rt(1, 4)),
        subdivision: None,
        accent: 1,
    }
}

// ===========================================================================
// The classification + counting harness (F3)
// ===========================================================================

/// Independently counts events by kind from the score graph (F's own walk), so
/// H's [`TaxonomyReport`] event counts can be cross-checked rather than trusted.
fn event_kind_counts(score: &Score) -> [usize; 7] {
    // [pitched, unpitched, rest, trajectory, graphic, indeterminate, cue]
    let mut c = [0usize; 7];
    for e in score.events.iter() {
        let i = match e {
            Event::Pitched(_) => 0,
            Event::Unpitched(_) => 1,
            Event::Rest(_) => 2,
            Event::Trajectory(_) => 3,
            Event::Graphic(_) => 4,
            Event::Indeterminate(_) => 5,
            Event::Cue(_) => 6,
        };
        c[i] += 1;
    }
    c
}

/// Per-fixture taxonomy outcome.
pub struct FixtureReport {
    pub name: &'static str,
    pub tier: Tier,
    pub annotations: DerivedAnnotations,
}

/// Runs H's derivation over the whole corpus, validating each fixture and
/// returning the per-fixture reports and the corpus-wide bucket totals.
pub struct CorpusReport {
    pub fixtures: Vec<FixtureReport>,
    /// Aggregated bucket totals across the corpus.
    pub totals: Vec<(Bucket, usize)>,
}

impl CorpusReport {
    pub fn total(&self, b: Bucket) -> usize {
        self.totals
            .iter()
            .find(|(x, _)| *x == b)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }
}

/// Classifies and counts the whole corpus. This is the F3 entry point: it builds
/// every fixture, asserts each is invariant-clean, derives H's annotations,
/// cross-checks H's event-kind counts against an independent walk, validates each
/// fixture's declared expected buckets, and aggregates per-bucket totals.
pub fn classify_corpus() -> CorpusReport {
    let profile = PrePassProfile::default();
    let mut fixtures = Vec::new();
    let mut totals: Vec<(Bucket, usize)> = Bucket::all().iter().map(|b| (*b, 0usize)).collect();

    for f in corpus() {
        let score = (f.build)();

        // 1. Invariant-clean: a malformed fixture must never mask an H regression.
        let v = check_invariants(&score);
        assert!(
            v.is_empty(),
            "corpus fixture `{}` has invariant violations: {v:?}",
            f.name
        );

        // 2. Derive H's annotations + taxonomy.
        let ann = derive_annotations(&score, &profile);
        let t = &ann.taxonomy;

        // 3. Cross-check H's event-kind counts against an independent walk.
        let mine = event_kind_counts(&score);
        let his = [
            t.pitched_events,
            t.unpitched_events,
            t.rest_events,
            t.trajectory_events,
            t.graphic_events,
            t.indeterminate_events,
            t.cue_events,
        ];
        assert_eq!(
            mine, his,
            "fixture `{}`: H's event-kind taxonomy {his:?} disagrees with an \
             independent walk {mine:?}",
            f.name
        );

        // 4. Every embedded pitch and every event is accounted for in some bucket
        //    (nothing silently absent): the spelling buckets cover every
        //    determinable embedded pitch, and the decomposition buckets cover
        //    every event.
        let event_total: usize = his.iter().sum();
        let decomp_total = t.decompositions_inferred
            + t.decomposition_skipped_nonmusical
            + t.decomposition_deferred_nonmetric
            + t.decomposition_inapplicable
            + t.decomposition_ungriddable;
        assert_eq!(
            event_total, decomp_total,
            "fixture `{}`: {decomp_total} events bucketed for decomposition but \
             {event_total} events exist — a kind is silently absent",
            f.name
        );

        // 5. Each fixture drives its declared buckets non-zero.
        for &b in f.expect {
            assert!(
                b.count(t) > 0,
                "fixture `{}` was tagged to populate `{}` but its count is 0",
                f.name,
                b.label()
            );
        }

        // 5b. The unusual-outcome buckets are an exact whitelist: a fixture may
        //     only land events in a loss/deferral bucket it declared. This closes
        //     the under-emission gap — an event that should decompose but silently
        //     slips into `ungriddable`/`skipped` lights up an undeclared bucket
        //     here, instead of hiding (step 4 stays balanced because the event
        //     just moved buckets, and the success count merely shrinks).
        for &b in UNUSUAL_BUCKETS {
            if !f.expect.contains(&b) {
                assert_eq!(
                    b.count(t),
                    0,
                    "fixture `{}` landed {} event(s) in `{}` without declaring it in \
                     `expect` — an event slipped into a loss/deferral bucket it should not",
                    f.name,
                    b.count(t),
                    b.label()
                );
            }
        }

        // 6. Aggregate.
        for entry in totals.iter_mut() {
            entry.1 += entry.0.count(t);
        }

        fixtures.push(FixtureReport {
            name: f.name,
            tier: f.tier,
            annotations: ann,
        });
    }

    CorpusReport { fixtures, totals }
}

/// The F3 acceptance assertion: every taxonomy bucket is non-empty across the
/// corpus (so "ineligible" is explicit and counted), or explicitly listed in
/// [`DEFERRED_BUCKETS`] with a reason. Emits per-kind counts to stderr.
pub fn assert_taxonomy_coverage() {
    let report = classify_corpus();
    assert!(
        report.fixtures.len() >= 20,
        "the representative corpus must have ≥20 fixtures (has {})",
        report.fixtures.len()
    );

    eprintln!("[F3] corpus taxonomy ({} fixtures):", report.fixtures.len());
    for (b, n) in &report.totals {
        eprintln!("    {:<34} {n}", b.label());
    }

    let deferred: BTreeSet<Bucket> = DEFERRED_BUCKETS.iter().copied().collect();
    let mut missing = Vec::new();
    for b in Bucket::all() {
        if report.total(b) == 0 && !deferred.contains(&b) {
            missing.push(b.label());
        }
    }
    assert!(
        missing.is_empty(),
        "taxonomy buckets never exercised by the corpus (and not deferred): {missing:?}"
    );

    // Each *broad* success bucket must be populated by several fixtures, not
    // carried by a single one: "non-empty corpus-wide" alone would stay green if a
    // bucket regressed everywhere except its one dedicated fixture. (The
    // unusual-outcome buckets are intentionally single-fixture and are pinned
    // exactly, per fixture, by `classify_corpus` step 5b instead.)
    const BROAD: &[Bucket] = &[
        Bucket::PitchedEvents,
        Bucket::SpellingsInferred,
        Bucket::DecompositionsInferred,
    ];
    for &b in BROAD {
        let populating = report
            .fixtures
            .iter()
            .filter(|f| b.count(&f.annotations.taxonomy) > 0)
            .count();
        assert!(
            populating >= 3,
            "broad bucket `{}` is populated by only {populating} fixture(s); a \
             success bucket carried by so few is fragile — a regression elsewhere \
             would not register in corpus coverage",
            b.label()
        );
    }
}

/// Runs the whole F3 harness (the conformance-suite entry point).
pub fn run_all() {
    assert_taxonomy_coverage();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_is_invariant_clean() {
        for f in corpus() {
            let s = (f.build)();
            let v = check_invariants(&s);
            assert!(
                v.is_empty(),
                "fixture `{}` not invariant-clean: {v:?}",
                f.name
            );
        }
    }

    #[test]
    fn corpus_has_at_least_twenty_fixtures_across_three_tiers() {
        let c = corpus();
        assert!(c.len() >= 20, "corpus too small: {}", c.len());
        for tier in [Tier::Common, Tier::Edge, Tier::Torture] {
            assert!(
                c.iter().any(|f| f.tier == tier),
                "no fixtures in tier {tier:?}"
            );
        }
    }

    #[test]
    fn taxonomy_coverage_is_complete() {
        assert_taxonomy_coverage();
    }

    #[test]
    fn fixture_names_are_unique() {
        let mut seen = BTreeSet::new();
        for f in corpus() {
            assert!(seen.insert(f.name), "duplicate fixture name `{}`", f.name);
        }
    }
}
