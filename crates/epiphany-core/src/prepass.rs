//! The spelling and notational-decomposition **pre-passes** (Agent H; Chapter 2
//! §"The Spelling Pre-Pass", Chapter 3 §"Sounding Duration and Notational
//! Decomposition").
//!
//! ## Canonical model: derived annotations, not stored objects
//!
//! Pre-pass outputs are **canonical derived annotations**: deterministic
//! functions of `(materialized score graph, profile, [`SpellingAlgorithmId`],
//! [`DecompositionAlgorithmId`])`, recomputed on materialization. They are *not*
//! author-minted graph objects with operation envelopes, and they do **not**
//! enter the canonical `Score` bytes (there is deliberately no codec for
//! [`DerivedAnnotations`]). Three consequences fall out of this choice:
//!
//! * **Manual overrides layer above generated output via ordinary operations.**
//!   An authored spelling (the canonical product of a `RespellPitch` operation,
//!   carried as a [`SpellingAttachment`] whose [`crate::SpellingSource`] outranks
//!   [`SpellingSourceKind::Inferred`] in the score's [`SpellingPrecedence`])
//!   takes precedence over the algorithm's default. The default is *derived*;
//!   the override is *authored*. H formalizes the precedence rule, not the model
//!   (see [`resolve_spelling`]). The decomposition side is governed the same way
//!   (Chapter 3: "same sources, same precedence machinery, same pre-pass
//!   discipline"): an authored [`DecompositionAttachment`] in
//!   `Score::decomposition_attachments` whose source outranks
//!   [`DecompositionSource::Inferred`] outranks the derived decomposition (see
//!   [`resolve_decomposition`]).
//! * **Algorithm version is part of the derivation key.** The
//!   [`SpellingAlgorithmId`] / [`DecompositionAlgorithmId`] in [`PrePassProfile`]
//!   key the derivation; a profile-declared change deterministically
//!   re-derives. No stored state migrates, because annotations are not stored.
//! * **Caching is permitted; canonical identity is not.** Two replicas at the
//!   same `(graph, profile, algorithm version)` produce byte-identical
//!   annotations whether or not either cached the result.
//!
//! The reducer (Chapter 6) does **not** run these algorithms; materialization
//! does, after reduction completes, as a pure function over a fully-reduced
//! `Score`. [`derive_annotations`] is the entry point a materializer (or F's
//! integration harness) calls.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::event::Event;
use crate::graph::{
    DecompositionAttachment, DecompositionSource, NotatedComponent, NoteValue, RegionTimeModel,
    Score, TupletRatio,
};
use crate::ids::{EventId, PitchId, RegionId, TupletId};
use crate::pitch::{
    AccidentalId, CmnNominal, DecompositionAlgorithmId, IdentifiedPitch, Pitch, PitchSpacePosition,
    PitchSpelling, SpellingAlgorithmId, SpellingAttachment, SpellingDirective, SpellingNominal,
    SpellingPrecedence, SpellingScope, SpellingSourceKind,
};
use crate::time::{EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime};

// ===========================================================================
// Profile and result types
// ===========================================================================

/// The algorithm-version key for a pre-pass run (Chapter 2 §"The Spelling
/// Pre-Pass"; Chapter 3). The literal algorithm ids are part of the derivation
/// key: two derivations agree byte-for-byte iff their profiles agree.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PrePassProfile {
    pub spelling_algorithm: SpellingAlgorithmId,
    pub decomposition_algorithm: DecompositionAlgorithmId,
}

impl Default for PrePassProfile {
    fn default() -> Self {
        PrePassProfile {
            spelling_algorithm: SpellingAlgorithmId::default_id(),
            decomposition_algorithm: DecompositionAlgorithmId::default_id(),
        }
    }
}

/// Where a resolved spelling came from: the algorithm's inference, or an
/// authored override that outranked it (Chapter 2 §"Configurable Precedence").
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SpellingProvenance {
    /// Produced by the spelling pre-pass algorithm (lowest default precedence).
    Inferred,
    /// Selected from an authored [`SpellingAttachment`] of the given source kind,
    /// which outranks [`SpellingSourceKind::Inferred`] in the score's precedence.
    Authored(SpellingSourceKind),
}

/// One pitch's resolved spelling: the effective [`PitchSpelling`] plus its
/// provenance (authored override vs. inferred default).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResolvedSpelling {
    pub spelling: PitchSpelling,
    pub provenance: SpellingProvenance,
}

/// Per-event-kind classification of what the pre-passes produced or deliberately
/// skipped, so "ineligible" is **explicit and counted**, never silently absent
/// (the acceptance taxonomy of Chapter 2 / PHASE2_QUICKSTART §H). Every embedded
/// pitch and every event is accounted for in exactly the buckets that apply.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TaxonomyReport {
    // --- Event counts by kind. ---
    pub pitched_events: usize,
    pub unpitched_events: usize,
    pub rest_events: usize,
    pub trajectory_events: usize,
    pub graphic_events: usize,
    pub indeterminate_events: usize,
    pub cue_events: usize,

    // --- Spelling outcomes (over embedded `IdentifiedPitch`es). ---
    /// Eligible pitches that received a non-trivial inferred spelling.
    pub spellings_inferred: usize,
    /// Eligible pitches whose effective spelling came from an authored override.
    pub spellings_authored: usize,
    /// Pitches whose pitch space declares spelling unavailable (non-`cmn-12`-
    /// determinable positions: JI vectors, non-12 EDOs, registered grammars).
    pub spelling_unavailable: usize,

    // --- Decomposition outcomes (over events). ---
    /// Eligible events whose effective decomposition is the pre-pass's inferred
    /// one.
    pub decompositions_inferred: usize,
    /// Eligible events whose effective decomposition came from an authored
    /// [`DecompositionAttachment`] whose source outranks
    /// [`DecompositionSource::Inferred`] (mirroring `spellings_authored`).
    /// Counted distinctly from `decompositions_inferred`: the derived map holds
    /// the authored components for these events, not the pre-pass's.
    pub decompositions_authored: usize,
    /// Metric-region events whose duration is wall-clock or indeterminate (no
    /// determinate musical duration to decompose).
    pub decomposition_skipped_nonmusical: usize,
    /// Events in proportional or aleatoric regions: decomposition deferred
    /// (Chapter 3; PHASE2_QUICKSTART §H "proportional and aleatoric regions:
    /// explicitly deferred").
    pub decomposition_deferred_nonmetric: usize,
    /// Event kinds that never decompose (trajectory / graphic / indeterminate /
    /// cue), counted so their absence from the decomposition set is explicit.
    pub decomposition_inapplicable: usize,
    /// Events whose musical duration is not representable on the notation grid
    /// (finer than a sixty-fourth, or non-dyadic and not a tuplet member);
    /// skipped rather than mis-rendered. Expected to be zero for well-formed
    /// metric input.
    pub decomposition_ungriddable: usize,
}

/// The canonical derived annotations for a score under a profile: the effective
/// spelling per eligible pitch, the decomposition per eligible event, and the
/// eligibility taxonomy with per-kind counts. Recomputed on materialization;
/// never serialized into canonical `Score` state.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DerivedAnnotations {
    pub spellings: BTreeMap<PitchId, ResolvedSpelling>,
    pub decompositions: BTreeMap<EventId, DecompositionAttachment>,
    pub taxonomy: TaxonomyReport,
    pub profile: PrePassProfile,
}

impl DerivedAnnotations {
    /// A canonical byte fingerprint of the derivation. Deterministic and
    /// order-independent — the maps are `BTreeMap`s in canonical key order, the
    /// embedded graph values (`PitchSpelling`, `DecompositionAttachment`,
    /// `SpellingSourceKind`) use their ratified [`CanonicalValue`] bytes, and
    /// counts/ids are little-endian, length-framed where variable. Two
    /// byte-equal fingerprints imply byte-identical derivations. This is the
    /// normative serialization surface the determinism gate compares, rather
    /// than the (non-normative) `Debug` form.
    ///
    /// [`CanonicalValue`]: crate::CanonicalValue
    pub fn canonical_fingerprint(&self) -> Vec<u8> {
        use crate::CanonicalValue;
        let mut out = Vec::new();
        let lp = |out: &mut Vec<u8>, bytes: &[u8]| {
            out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            out.extend_from_slice(bytes);
        };

        out.extend_from_slice(&(self.spellings.len() as u64).to_le_bytes());
        for (pid, rs) in &self.spellings {
            out.extend_from_slice(&pid.canonical_bytes());
            lp(&mut out, &rs.spelling.canonical_bytes());
            match &rs.provenance {
                SpellingProvenance::Inferred => out.push(0),
                SpellingProvenance::Authored(kind) => {
                    out.push(1);
                    lp(&mut out, &kind.canonical_bytes());
                }
            }
        }

        out.extend_from_slice(&(self.decompositions.len() as u64).to_le_bytes());
        for (eid, dec) in &self.decompositions {
            out.extend_from_slice(&eid.canonical_bytes());
            lp(&mut out, &dec.canonical_bytes());
        }

        let t = &self.taxonomy;
        for count in [
            t.pitched_events,
            t.unpitched_events,
            t.rest_events,
            t.trajectory_events,
            t.graphic_events,
            t.indeterminate_events,
            t.cue_events,
            t.spellings_inferred,
            t.spellings_authored,
            t.spelling_unavailable,
            t.decompositions_inferred,
            t.decompositions_authored,
            t.decomposition_skipped_nonmusical,
            t.decomposition_deferred_nonmetric,
            t.decomposition_inapplicable,
            t.decomposition_ungriddable,
        ] {
            out.extend_from_slice(&(count as u64).to_le_bytes());
        }

        lp(
            &mut out,
            self.profile.spelling_algorithm.as_str().as_bytes(),
        );
        lp(
            &mut out,
            self.profile.decomposition_algorithm.as_str().as_bytes(),
        );
        out
    }
}

// ===========================================================================
// Entry point
// ===========================================================================

/// Computes the [`DerivedAnnotations`] for `score` under `profile`. Pure and
/// deterministic: byte-identical `(score, profile)` yields byte-identical
/// output, independent of iteration incidentals (the derivation walks the graph
/// in canonical id/position order).
pub fn derive_annotations(score: &Score, profile: &PrePassProfile) -> DerivedAnnotations {
    let mut taxonomy = TaxonomyReport::default();

    // Index regions: event -> (is the owning region metric?), and the event's
    // musical position when it has one. The graph keeps each voice's events
    // position-sorted (invariant 3), so the per-voice order is canonical.
    let layout = ScoreLayout::build(score);

    // The pre-passes implement exactly one algorithm each (the registered
    // `"default"` id). A profile requesting any other id is **not honored**:
    // rather than return default output labeled as the requested algorithm —
    // which would let an unimplemented/future algorithm silently alias the
    // default in a derivation cache — that pre-pass derives nothing. The
    // requested id stays in the result's `profile`, so the cache key is honest.
    let spelling_supported = profile.spelling_algorithm == SpellingAlgorithmId::default_id();
    let decomposition_supported =
        profile.decomposition_algorithm == DecompositionAlgorithmId::default_id();

    // --- Spelling pre-pass. ---
    let mut spellings = BTreeMap::new();
    if spelling_supported {
        let inferred = infer_spellings(score, &layout, &mut taxonomy);
        let precedence = &score.spelling_precedence;
        for (pid, inferred_spelling) in inferred {
            let resolved = resolve_spelling(score, pid, inferred_spelling, precedence);
            match resolved.provenance {
                SpellingProvenance::Inferred => taxonomy.spellings_inferred += 1,
                SpellingProvenance::Authored(_) => taxonomy.spellings_authored += 1,
            }
            spellings.insert(pid, resolved);
        }
    }

    // --- Decomposition pre-pass. ---
    let mut decompositions = BTreeMap::new();
    if decomposition_supported {
        let inferred = infer_decompositions(score, &layout, &mut taxonomy);
        for (eid, inferred_attachment) in inferred {
            let resolved = resolve_decomposition(score, eid, inferred_attachment);
            match resolved.source {
                DecompositionSource::Inferred => taxonomy.decompositions_inferred += 1,
                _ => taxonomy.decompositions_authored += 1,
            }
            decompositions.insert(eid, resolved);
        }
    }

    DerivedAnnotations {
        spellings,
        decompositions,
        taxonomy,
        profile: profile.clone(),
    }
}

// ===========================================================================
// Score layout index (event -> region/voice/position membership)
// ===========================================================================

/// The containment facts the pre-passes need, gathered once: which region owns
/// each event (and whether that region is metric), each event's musical
/// position, and the per-(region, voice) ordered pitch sequences spelling runs
/// over. Tuplet membership is folded in for decomposition's notated domain.
struct ScoreLayout {
    /// Event -> (owning region, region is metric).
    event_region: BTreeMap<EventId, RegionId>,
    metric_regions: BTreeSet<RegionId>,
    /// Event -> its musical position, when it has one.
    event_position: BTreeMap<EventId, MusicalPosition>,
    /// Ordered melodic runs for spelling: `(region, voice) -> events in position
    /// order`. Keyed in a `BTreeMap` so iteration is canonical.
    voice_runs: BTreeMap<(RegionId, crate::ids::VoiceId), Vec<EventId>>,
    /// Event -> innermost tuplet membership (id + ratio), if any.
    event_tuplet: BTreeMap<EventId, (TupletId, TupletRatio)>,
    /// Measure duration (whole-note units on the notation grid) governing each
    /// region, resolved from the region's first determinable time signature.
    region_measure_units: BTreeMap<RegionId, i64>,
}

impl ScoreLayout {
    fn build(score: &Score) -> Self {
        let mut event_region = BTreeMap::new();
        let mut metric_regions = BTreeSet::new();
        let mut event_position = BTreeMap::new();
        let mut voice_runs: BTreeMap<(RegionId, crate::ids::VoiceId), Vec<EventId>> =
            BTreeMap::new();
        let mut region_measure_units = BTreeMap::new();

        for region in &score.canvas.regions {
            let is_metric = matches!(region.time_model, RegionTimeModel::Metric(_));
            if is_metric {
                metric_regions.insert(region.id);
            }
            region_measure_units.insert(region.id, resolve_measure_units(score, region));
            for si in region.staff_instances() {
                for v in &si.voices {
                    let run = voice_runs.entry((region.id, v.id)).or_default();
                    for &eid in &v.events {
                        event_region.insert(eid, region.id);
                        run.push(eid);
                        if let Some(ev) = score.events.get(eid) {
                            if let EventPosition::Musical(p) = ev.position() {
                                event_position.insert(eid, p.clone());
                            }
                        }
                    }
                }
            }
        }

        // Tuplet membership: innermost wins (a member listed by a child tuplet
        // shadows its parent). We resolve "innermost" as the tuplet with the
        // smallest `required_total`; ties keep the lowest id for determinism. An
        // id→tuplet index makes the "is the incumbent still the smallest?" check an
        // O(log n) lookup rather than a linear scan of every tuplet per member (a
        // quadratic blow-up on densely-tupletted input otherwise).
        let tuplet_by_id: BTreeMap<TupletId, &crate::graph::Tuplet> = score
            .cross_cutting
            .tuplets
            .iter()
            .map(|t| (t.id, t))
            .collect();
        let mut event_tuplet: BTreeMap<EventId, (TupletId, TupletRatio)> = BTreeMap::new();
        for t in &score.cross_cutting.tuplets {
            for &m in &t.members {
                let replace = match event_tuplet.get(&m) {
                    Some((cur_id, _)) => match tuplet_by_id.get(cur_id).copied() {
                        Some(cur) => {
                            (t.required_total.rational(), t.id)
                                < (cur.required_total.rational(), cur.id)
                        }
                        None => true,
                    },
                    None => true,
                };
                if replace {
                    event_tuplet.insert(m, (t.id, t.ratio));
                }
            }
        }

        ScoreLayout {
            event_region,
            metric_regions,
            event_position,
            voice_runs,
            event_tuplet,
            region_measure_units,
        }
    }

    fn is_metric(&self, event: EventId) -> bool {
        self.event_region
            .get(&event)
            .is_some_and(|r| self.metric_regions.contains(r))
    }
}

// ===========================================================================
// Line-of-fifths spelling model (Chapter 2)
// ===========================================================================
//
// A "tonal pitch class" is a point on the *line of fifths*: an integer `lof`
// where each +1 step is an ascending perfect fifth. `C = 0`, the naturals run
// `F C G D A E B = -1..=5`, sharps continue upward, flats downward. The
// chromatic (12-TET) pitch class of `lof` is `(7 * lof) mod 12`. Candidate
// spellings of a pitch class are the `lof` values realizing it; the Temperley
// preference rule chooses the candidate keeping the music *compact* on the line
// of fifths (close to a running centre of gravity), which yields tonally
// plausible spellings (sharps in sharp keys, flats in flat keys).

/// Lowest / highest line-of-fifths index the algorithm spells with — `Cb`
/// (`-7`) up to `B#` (`12`): naturals and single accidentals only. Double
/// accidentals are out of the default candidate set (Phase-2 scope; a
/// musicologically-perfect double-accidental refinement is a Pass-12 candidate).
const LOF_MIN: i32 = -7;
const LOF_MAX: i32 = 12;

/// Melodic memory (in spelled pitches) of the centre-of-gravity window. ~one
/// octave of context; large enough to stabilise a key, small enough to follow a
/// modulation.
const SPELLING_WINDOW: usize = 9;

/// The 12-TET pitch class of a line-of-fifths index.
#[inline]
fn lof_pitch_class(lof: i32) -> i32 {
    (7 * lof).rem_euclid(12)
}

/// The diatonic nominal and chromatic alteration (semitones) a line-of-fifths
/// index resolves to. Naturals sit at `lof -1..=5`; each whole step of 7 is one
/// alteration.
fn lof_to_nominal_alteration(lof: i32) -> (CmnNominal, i32) {
    let mut nat = lof;
    let mut alteration = 0;
    while nat > 5 {
        nat -= 7;
        alteration += 1;
    }
    while nat < -1 {
        nat += 7;
        alteration -= 1;
    }
    let nominal = match nat {
        -1 => CmnNominal::F,
        0 => CmnNominal::C,
        1 => CmnNominal::G,
        2 => CmnNominal::D,
        3 => CmnNominal::A,
        4 => CmnNominal::E,
        5 => CmnNominal::B,
        _ => unreachable!("nat reduced into -1..=5"),
    };
    (nominal, alteration)
}

/// The line-of-fifths index of a CMN nominal + alteration (the inverse of
/// [`lof_to_nominal_alteration`] on the nominal band).
fn cmn_to_lof(nominal: CmnNominal, alteration: i32) -> i32 {
    let base = match nominal {
        CmnNominal::F => -1,
        CmnNominal::C => 0,
        CmnNominal::G => 1,
        CmnNominal::D => 2,
        CmnNominal::A => 3,
        CmnNominal::E => 4,
        CmnNominal::B => 5,
    };
    base + 7 * alteration
}

/// Accidental "complexity" of a spelling: `|alteration|`. Naturals (0) are
/// simplest, single accidentals (1) next; used as a tiebreak so a white-key
/// pitch class defaults to its natural spelling absent contrary context.
#[inline]
fn lof_complexity(lof: i32) -> i32 {
    lof_to_nominal_alteration(lof).1.abs()
}

/// The candidate line-of-fifths spellings of a pitch class, within the
/// single-accidental band `[LOF_MIN, LOF_MAX]`, in ascending order.
fn lof_candidates(pitch_class: i32) -> Vec<i32> {
    (LOF_MIN..=LOF_MAX)
        .filter(|&lof| lof_pitch_class(lof) == pitch_class)
        .collect()
}

/// The accidental glyph stack for a chromatic alteration. An empty stack means
/// *no glyph is drawn* (a natural with no key signature to cancel), distinct
/// from a natural sign (Chapter 2 §"Absent Accidentals").
fn accidental_ids(alteration: i32) -> Vec<AccidentalId> {
    // The inferred candidate band ([`LOF_MIN`, `LOF_MAX`]) only ever yields single
    // accidentals, but the *authored* path preserves any `alteration` an author
    // wrote (`alteration` is an `i8`), so a triple-sharp can reach here. Emit a
    // glyph stack whose magnitudes sum to `alteration` — `⌊|a|/2⌋` double-glyphs
    // plus a single-glyph for the odd remainder — so the spelling reconstructs the
    // authored pitch exactly rather than being clamped to a double accidental
    // (which would silently drop a semitone). An alteration of 0 draws no glyph.
    let (double, single, magnitude) = if alteration >= 0 {
        ("double-sharp", "sharp", alteration)
    } else {
        ("double-flat", "flat", -alteration)
    };
    let mut out = Vec::with_capacity((magnitude as usize).div_ceil(2));
    for _ in 0..(magnitude / 2) {
        out.push(AccidentalId::new(double));
    }
    if magnitude % 2 == 1 {
        out.push(AccidentalId::new(single));
    }
    out
}

/// Builds a [`PitchSpelling`] for a chosen line-of-fifths index realizing the
/// absolute 12-TET `semitone` (octave included). The spelling octave is solved
/// so `nominal.chromatic() + alteration + 12*octave == semitone`, which places
/// e.g. `B#` an octave below the `C` it sounds as.
fn spelling_from_lof(lof: i32, semitone: i32) -> PitchSpelling {
    let (nominal, alteration) = lof_to_nominal_alteration(lof);
    // Exact: (semitone - chromatic - alteration) is divisible by 12 because the
    // candidate realizes the same pitch class as `semitone`.
    let octave = (semitone - nominal.chromatic() as i32 - alteration).div_euclid(12);
    PitchSpelling {
        nominal: SpellingNominal::Cmn(nominal),
        accidentals: accidental_ids(alteration),
        octave: octave as i8,
        render_hints: Default::default(),
    }
}

/// The context-free simplest spelling of a single pitch: its authored CMN
/// letter if it has one, else the fewest-accidental enharmonic spelling of its
/// 12-TET pitch class. Returns `None` if the pitch space declares spelling
/// unavailable. The score-level [`derive_annotations`] is the context-aware
/// entry; this is the isolated fallback behind [`crate::spell`].
pub fn simplest_spelling(pitch: &Pitch) -> Option<PitchSpelling> {
    let (semitone, authored) = spell_entry_for(pitch)?;
    if let Some((nominal, alteration, octave)) = authored {
        return Some(PitchSpelling {
            nominal: SpellingNominal::Cmn(nominal),
            accidentals: accidental_ids(alteration),
            octave,
            render_hints: Default::default(),
        });
    }
    let pc = semitone.rem_euclid(12);
    let lof = choose_lof(pc, 0, 0, std::cmp::Ordering::Equal);
    Some(spelling_from_lof(lof, semitone))
}

/// An authored CMN letter the algorithm must preserve: `(nominal, alteration,
/// octave)`.
type AuthoredCmn = (CmnNominal, i32, i8);

/// A pitch's role in a spelling run: its id, its absolute 12-TET semitone, and —
/// when its scale position is already CMN — the authored letter the algorithm
/// must preserve rather than re-spell.
struct SpellEntry {
    pid: PitchId,
    semitone: i32,
    authored_cmn: Option<AuthoredCmn>,
}

/// The absolute 12-TET semitone of a pitch, and its authored CMN letter if it
/// has one. Returns `None` when the pitch space declares spelling unavailable
/// (the 12-TET class is not determinable from the position alone).
fn spell_entry_for(pitch: &Pitch) -> Option<(i32, Option<AuthoredCmn>)> {
    let semitone = pitch.twelve_tet_semitone()?;
    let authored = match &pitch.scale_position.position {
        PitchSpacePosition::Cmn {
            nominal,
            alteration,
            octave,
        } => Some((*nominal, *alteration as i32, *octave)),
        _ => None,
    };
    Some((semitone, authored))
}

/// Chooses the line-of-fifths spelling of a pitch class given the running centre
/// of gravity (`sum`/`count` of recent spelled `lof`s) and the melodic step
/// direction into this note. Lower key wins:
///   1. distance to the centre of gravity (`|lof*count - sum|`, exact integer);
///   2. accidental simplicity;
///   3. melodic direction (ascending prefers sharps, descending flats) on ties;
///   4. the lof value itself, for a total deterministic order.
fn choose_lof(pitch_class: i32, sum: i64, count: i64, direction: std::cmp::Ordering) -> i32 {
    use std::cmp::Ordering;
    let candidates = lof_candidates(pitch_class);
    candidates
        .into_iter()
        .min_by_key(|&lof| {
            let distance = (lof as i64 * count - sum).abs();
            let complexity = lof_complexity(lof);
            let directional = match direction {
                Ordering::Greater => -lof, // ascending: prefer higher lof (sharps)
                Ordering::Less => lof,     // descending: prefer lower lof (flats)
                Ordering::Equal => lof.abs(),
            };
            (distance, complexity, directional, lof)
        })
        .expect("every pitch class has at least one candidate in the single-accidental band")
}

/// Assigns inferred spellings to one ordered melodic run (a voice), folding
/// chords (multiple pitches at one position) into a single centre-of-gravity
/// step.
fn spell_run(slots: &[Vec<SpellEntry>], out: &mut BTreeMap<PitchId, PitchSpelling>) {
    let mut window: VecDeque<i32> = VecDeque::with_capacity(SPELLING_WINDOW);
    let mut prev_ref: Option<i32> = None;
    for slot in slots {
        if slot.is_empty() {
            continue;
        }
        // The slot's representative semitone (mean), for melodic direction.
        let ref_sem: i32 =
            (slot.iter().map(|e| e.semitone as i64).sum::<i64>() / slot.len() as i64) as i32;
        let direction = match prev_ref {
            Some(p) => ref_sem.cmp(&p),
            None => std::cmp::Ordering::Equal,
        };
        let count = window.len() as i64;
        let sum: i64 = window.iter().map(|&l| l as i64).sum();

        let mut chosen: Vec<i32> = Vec::with_capacity(slot.len());
        for entry in slot {
            let (lof, spelling) = match entry.authored_cmn {
                Some((nominal, alteration, octave)) => {
                    let lof = cmn_to_lof(nominal, alteration);
                    let spelling = PitchSpelling {
                        nominal: SpellingNominal::Cmn(nominal),
                        accidentals: accidental_ids(alteration),
                        octave,
                        render_hints: Default::default(),
                    };
                    (lof, spelling)
                }
                None => {
                    let pc = entry.semitone.rem_euclid(12);
                    let lof = choose_lof(pc, sum, count, direction);
                    (lof, spelling_from_lof(lof, entry.semitone))
                }
            };
            out.insert(entry.pid, spelling);
            chosen.push(lof);
        }
        for l in chosen {
            window.push_back(l);
            while window.len() > SPELLING_WINDOW {
                window.pop_front();
            }
        }
        prev_ref = Some(ref_sem);
    }
}

/// Runs the spelling pre-pass over every eligible embedded pitch, returning the
/// inferred (pre-precedence) spelling per pitch. Counts unavailable pitches into
/// the taxonomy.
fn infer_spellings(
    score: &Score,
    layout: &ScoreLayout,
    taxonomy: &mut TaxonomyReport,
) -> BTreeMap<PitchId, PitchSpelling> {
    let mut out = BTreeMap::new();

    // Count events by kind once (independent of spelling eligibility).
    for ev in score.events.iter_canonical() {
        match ev {
            Event::Pitched(_) => taxonomy.pitched_events += 1,
            Event::Unpitched(_) => taxonomy.unpitched_events += 1,
            Event::Rest(_) => taxonomy.rest_events += 1,
            Event::Trajectory(_) => taxonomy.trajectory_events += 1,
            Event::Graphic(_) => taxonomy.graphic_events += 1,
            Event::Indeterminate(_) => taxonomy.indeterminate_events += 1,
            Event::Cue(_) => taxonomy.cue_events += 1,
        }
    }

    // Spell each voice run in melodic order. Pitches that decline a 12-TET class
    // (spelling-unavailable spaces) are counted, not spelled.
    for events in layout.voice_runs.values() {
        let mut slots: Vec<Vec<SpellEntry>> = Vec::new();
        for &eid in events {
            let Some(ev) = score.events.get(eid) else {
                continue;
            };
            let mut pitches: Vec<&IdentifiedPitch> = Vec::new();
            ev.collect_identified_pitches(&mut pitches);
            if pitches.is_empty() {
                continue;
            }
            // Deterministic chord order: ascending semitone, then pitch id.
            let mut entries: Vec<SpellEntry> = Vec::new();
            for ip in pitches {
                match spell_entry_for(&ip.pitch) {
                    Some((semitone, authored_cmn)) => entries.push(SpellEntry {
                        pid: ip.id,
                        semitone,
                        authored_cmn,
                    }),
                    None => taxonomy.spelling_unavailable += 1,
                }
            }
            entries.sort_by_key(|e| (e.semitone, e.pid));
            if !entries.is_empty() {
                slots.push(entries);
            }
        }
        spell_run(&slots, &mut out);
    }

    out
}

/// Resolves the *effective* spelling for a pitch: an authored
/// [`SpellingAttachment`] on the engraved layer whose [`SpellingSourceKind`]
/// outranks [`SpellingSourceKind::Inferred`] in `precedence` wins; otherwise the
/// algorithm's inferred spelling stands. This is the precedence rule a
/// `RespellPitch` override rides on (Chapter 2 §"Configurable Precedence").
pub fn resolve_spelling(
    score: &Score,
    pitch: PitchId,
    inferred: PitchSpelling,
    precedence: &SpellingPrecedence,
) -> ResolvedSpelling {
    let inferred_rank = precedence.rank(SpellingSourceKind::Inferred);
    // The best authored override: engraved layer, pitch-scoped, explicit, source
    // outranking Inferred. Among candidates, lowest precedence rank wins, then
    // highest priority; a remaining tie keeps the first candidate in the score's
    // `spelling_attachments` order, which is canonical (codec-fixed), so the
    // resolution is deterministic across replicas.
    let mut best: Option<(usize, i32, &SpellingAttachment)> = None;
    for att in &score.spelling_attachments {
        if att.layer.is_some() {
            continue; // engraved layer only
        }
        if !matches!(&att.scope, SpellingScope::Pitch(p) if *p == pitch) {
            continue;
        }
        let SpellingDirective::Explicit(_) = &att.directive else {
            continue;
        };
        let rank = precedence.rank(att.source.kind());
        if rank >= inferred_rank {
            continue; // does not outrank the inferred default
        }
        let candidate = (rank, att.priority, att);
        best = match best {
            None => Some(candidate),
            Some(cur) => {
                // Lower rank wins; then higher priority; a full tie keeps `cur`
                // (the earlier attachment in canonical order). `Reverse` orders by
                // descending priority without negating, which would overflow for a
                // `priority` of `i32::MIN`.
                if (rank, Reverse(att.priority)) < (cur.0, Reverse(cur.2.priority)) {
                    Some(candidate)
                } else {
                    Some(cur)
                }
            }
        };
    }

    match best {
        Some((_, _, att)) => {
            if let SpellingDirective::Explicit(spelling) = &att.directive {
                ResolvedSpelling {
                    spelling: spelling.clone(),
                    provenance: SpellingProvenance::Authored(att.source.kind()),
                }
            } else {
                unreachable!("filtered to Explicit directives")
            }
        }
        None => ResolvedSpelling {
            spelling: inferred,
            provenance: SpellingProvenance::Inferred,
        },
    }
}

// ===========================================================================
// Notational decomposition (Chapter 3)
// ===========================================================================
//
// The notation grid is whole-note units over `GRID_DEN` (1/4096), so every note
// value down to a (dotted) sixty-fourth is an exact integer. All metric grid
// logic — measure offset, dyadic levels, barline and beat splitting — is then
// integer arithmetic, which keeps the derivation exact and deterministic.

/// Subdivisions of a whole note on the notation grid: `2^12`, so a sixty-fourth
/// is `64` units and a (single-)dotted sixty-fourth is `96`.
const GRID_DEN: i64 = 1 << 12;
/// Phase-2 default decomposition uses single dots only (Chapter 3 / §H "simple
/// augmentation dots; double-dotted and beyond may defer").
const MAX_DOTS: u8 = 1;

/// Grid units of a base note value (undotted). This is the integer-grid mirror of
/// [`NoteValue::whole_note_fraction`] (the rational source of truth); the two are
/// pinned together by the `grid_units_agree_with_canonical_note_value_math` test
/// so they cannot silently diverge.
fn note_value_units(v: NoteValue) -> i64 {
    match v {
        NoteValue::Whole => GRID_DEN,
        NoteValue::Half => GRID_DEN / 2,
        NoteValue::Quarter => GRID_DEN / 4,
        NoteValue::Eighth => GRID_DEN / 8,
        NoteValue::Sixteenth => GRID_DEN / 16,
        NoteValue::ThirtySecond => GRID_DEN / 32,
        NoteValue::SixtyFourth => GRID_DEN / 64,
    }
}

const ALL_NOTE_VALUES: [NoteValue; 7] = [
    NoteValue::Whole,
    NoteValue::Half,
    NoteValue::Quarter,
    NoteValue::Eighth,
    NoteValue::Sixteenth,
    NoteValue::ThirtySecond,
    NoteValue::SixtyFourth,
];

/// Total grid units of `value` with `dots` augmentation dots
/// (`base * (2 - 2^-dots)`). The integer-grid mirror of
/// [`crate::graph::NotatedComponent::notated_duration`]; pinned to it by the
/// `grid_units_agree_with_canonical_note_value_math` test so a change to dot
/// semantics in either domain cannot silently diverge from the other.
fn dotted_units(value: NoteValue, dots: u8) -> i64 {
    let base = note_value_units(value);
    let mut total = base;
    let mut increment = base;
    for _ in 0..dots {
        increment /= 2;
        total += increment;
    }
    total
}

/// The `(value, dots)` whose dotted length is exactly `units`, if any (dots
/// capped at [`MAX_DOTS`]). Larger base values are tried first so a length is
/// expressed by the fewest noteheads.
fn note_for_units(units: i64) -> Option<(NoteValue, u8)> {
    for &v in &ALL_NOTE_VALUES {
        for dots in 0..=MAX_DOTS {
            if dotted_units(v, dots) == units {
                return Some((v, dots));
            }
        }
    }
    None
}

/// Total grid units of a component-length list. Used to verify a decomposition
/// reconstructs its input exactly: every component's `dotted_units` equals the
/// span it represents, so the sum equals the input length unless a residue finer
/// than the grid's smallest value was dropped.
fn sum_component_units(lengths: &[(NoteValue, u8)]) -> i64 {
    lengths.iter().map(|&(v, d)| dotted_units(v, d)).sum()
}

/// The dyadic *level* (metric strength) of a within-measure grid position:
/// `0` is a barline-strength multiple of a whole note, larger is weaker. A
/// boundary at level `L` is at least as strong as one at level `> L`.
fn dyadic_level(units: i64) -> i32 {
    if units == 0 {
        return 0;
    }
    // GRID_DEN = 2^12; level = max(0, 12 - v2(units)).
    let v2 = units.trailing_zeros() as i32;
    (12 - v2).max(0)
}

/// Whether `[start, start+len)` may be written as the single notated value of
/// `len`, starting at `start`: the length is a notatable value, and it crosses
/// no interior grid boundary at least as strong as `start` (so it never obscures
/// a beat stronger than the one it began on — the metric well-formedness rule of
/// Chapter 3). Positions are within-measure grid units.
fn single_value_acceptable(start: i64, len: i64) -> Option<(NoteValue, u8)> {
    let (value, dots) = note_for_units(len)?;
    let start_level = dyadic_level(start);
    // The strongest interior dyadic boundary in (start, start+len). The interior
    // boundary nearest `start` at each level governs; it suffices to check the
    // single strongest interior boundary.
    if let Some(b) = strongest_interior_boundary(start, start + len) {
        if dyadic_level(b) <= start_level {
            return None;
        }
    }
    Some((value, dots))
}

/// The strongest (lowest-level, i.e. metrically heaviest) dyadic grid boundary
/// strictly inside `(start, end)`, or `None` if the open interval contains none.
/// Ties at the same level resolve to the leftmost.
fn strongest_interior_boundary(start: i64, end: i64) -> Option<i64> {
    // Walk levels from strongest (0) to finest; the first level with an interior
    // multiple gives the answer.
    for level in 0..=12i32 {
        let step = GRID_DEN >> level; // spacing of level-`level` boundaries
        if step == 0 {
            break;
        }
        // First multiple of `step` strictly greater than `start`.
        let first = (start / step + 1) * step;
        if first < end {
            return Some(first);
        }
    }
    None
}

/// Decomposes one within-measure span `[start, end)` (grid units) into notated
/// component lengths, splitting at the strongest interior boundary it crosses so
/// syncopations tie across stronger beats. Appends `(value, dots)` in order.
fn decompose_segment(start: i64, end: i64, out: &mut Vec<(NoteValue, u8)>) {
    if start >= end {
        return;
    }
    if let Some(component) = single_value_acceptable(start, end - start) {
        out.push(component);
        return;
    }
    match strongest_interior_boundary(start, end) {
        Some(b) => {
            decompose_segment(start, b, out);
            decompose_segment(b, end, out);
        }
        None => {
            // No interior boundary and not a clean note value: the span is finer
            // than the grid's smallest representable value (a sixty-fourth), so it
            // cannot be notated. Leave it unrepresented — `decompose_metric` /
            // `decompose_tuplet_member` verify the components reconstruct the input
            // and report such a residue as ungriddable rather than emitting a short
            // decomposition that would violate invariant 15.
        }
    }
}

/// Decomposes a determinate musical duration starting at `position` (region-
/// relative) under a measure of `measure_units`, splitting at barlines with
/// ties. Returns the notated component lengths in order, or `None` if the
/// duration is not representable on the grid.
fn decompose_metric(
    position_units: i64,
    duration_units: i64,
    measure_units: i64,
) -> Option<Vec<(NoteValue, u8)>> {
    if duration_units <= 0 {
        return Some(Vec::new());
    }
    // A degenerate (zero or negative) measure length has no barline grid to align
    // against; report it ungriddable rather than dividing by zero in the offset
    // reduction below.
    if measure_units <= 0 {
        return None;
    }
    let mut out = Vec::new();
    // Position within the current measure; barlines fall every `measure_units`.
    let mut offset = position_units.rem_euclid(measure_units);
    let mut remaining = duration_units;
    let mut guard = 0;
    while remaining > 0 {
        guard += 1;
        if guard > 4096 {
            return None; // runaway guard; should be unreachable for griddable input
        }
        let room = measure_units - offset; // until the next barline
        let seg = remaining.min(room);
        decompose_segment(offset, offset + seg, &mut out);
        remaining -= seg;
        offset += seg;
        if offset >= measure_units {
            offset = 0;
        }
    }
    // The components must reconstruct the input exactly. They will not when the
    // duration is a grid multiple but not a note-value multiple — i.e. finer than
    // the grid's smallest representable value (a sixty-fourth) — because
    // `decompose_segment` drops that residue. Report it as ungriddable rather than
    // emit a decomposition whose components fall short of the duration (invariant
    // 15). For well-formed metric input the sum always matches.
    if sum_component_units(&out) != duration_units {
        return None;
    }
    Some(out)
}

/// Decomposes a tuplet member: its *sounding* duration (which is non-dyadic,
/// e.g. `1/12` for a triplet eighth) is converted to the *notated* domain in the
/// exact rational domain (`sounding * actual / notated`, which a well-formed
/// tuplet makes dyadic), gridded, decomposed there, and tagged with the tuplet.
/// The notated value is decomposed as a standalone span (Phase-2 simplification:
/// tuplet-internal metric alignment beyond a clean member, and tuplet nesting,
/// are Pass-12 refinements).
fn decompose_tuplet_member(
    sounding: &RationalTime,
    ratio: TupletRatio,
) -> Option<Vec<(NoteValue, u8)>> {
    // notated = sounding * actual / notated, computed exactly before gridding.
    let scale = RationalTime::new(ratio.actual() as i64, ratio.notated() as i64)?;
    let notated = sounding.mul(&scale);
    let notated_units = to_grid_units(&notated)?;
    if notated_units <= 0 {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    decompose_segment(0, notated_units, &mut out);
    // Same reconstruction guarantee as `decompose_metric`: reject a notated value
    // finer than the grid's smallest representable value rather than emit
    // components that fall short of it (invariant 15).
    if sum_component_units(&out) != notated_units {
        return None;
    }
    Some(out)
}

/// Converts a musical duration/position to grid units, or `None` if it is not an
/// exact multiple of `1/GRID_DEN` (finer than a sixty-fourth, or non-dyadic).
fn to_grid_units(r: &RationalTime) -> Option<i64> {
    match r {
        RationalTime::Small(s) => {
            let n = s.numerator() as i64;
            let d = s.denominator() as i64;
            if d == 0 || GRID_DEN % d != 0 {
                return None;
            }
            Some(n * (GRID_DEN / d))
        }
        // Real notation durations never promote; treat a promoted value as
        // ungriddable rather than guessing.
        RationalTime::Large(_) => None,
    }
}

/// Resolves the measure length (grid units) governing a region: the first
/// determinable time signature referenced by one of the region's measures, else
/// the score's first time signature, else a whole note (a 4/4-equivalent
/// default). Multi-meter and mid-region meter changes are a Phase-2
/// simplification (a single governing meter per region).
fn resolve_measure_units(score: &Score, region: &crate::graph::Region) -> i64 {
    let lookup = |tsid: &crate::ids::TimeSignatureId| -> Option<i64> {
        score
            .time_signatures
            .iter()
            .find(|ts| &ts.id == tsid)
            .and_then(|ts| to_grid_units(ts.measure_duration().rational()))
    };
    for si in region.staff_instances() {
        for m in &si.measures {
            if let Some(tsid) = &m.time_signature {
                if let Some(units) = lookup(tsid) {
                    return units;
                }
            }
        }
    }
    if let Some(ts) = score.time_signatures.first() {
        if let Some(units) = to_grid_units(ts.measure_duration().rational()) {
            return units;
        }
    }
    GRID_DEN
}

/// Runs the decomposition pre-pass over every eligible event, returning the
/// inferred (pre-precedence) attachment per event. Counts ineligible / deferred
/// / skipped cases into the taxonomy; the inferred/authored split is counted by
/// [`derive_annotations`] after [`resolve_decomposition`], mirroring
/// [`infer_spellings`].
fn infer_decompositions(
    score: &Score,
    layout: &ScoreLayout,
    taxonomy: &mut TaxonomyReport,
) -> BTreeMap<EventId, DecompositionAttachment> {
    let mut out = BTreeMap::new();

    for ev in score.events.iter_canonical() {
        let eid = ev.id();
        // Kinds that never decompose: explicit, counted absence.
        match ev {
            Event::Trajectory(_) | Event::Graphic(_) | Event::Indeterminate(_) | Event::Cue(_) => {
                taxonomy.decomposition_inapplicable += 1;
                continue;
            }
            Event::Pitched(_) | Event::Unpitched(_) | Event::Rest(_) => {}
        }

        // Only metric regions decompose; proportional/aleatoric deferred.
        if !layout.is_metric(eid) {
            taxonomy.decomposition_deferred_nonmetric += 1;
            continue;
        }

        // Need a determinate musical duration.
        let EventDuration::Musical(MusicalDuration(dur)) = ev.duration() else {
            taxonomy.decomposition_skipped_nonmusical += 1;
            continue;
        };

        // Tuplet members convert sounding -> notated *before* gridding (the
        // sounding duration is non-dyadic); plain metric events grid directly.
        let components_units = if let Some((tuplet_id, ratio)) = layout.event_tuplet.get(&eid) {
            decompose_tuplet_member(dur, *ratio).map(|l| (l, Some(*tuplet_id)))
        } else {
            match to_grid_units(dur) {
                Some(duration_units) => {
                    let measure_units = layout
                        .region_measure_units
                        .get(layout.event_region.get(&eid).unwrap())
                        .copied()
                        .unwrap_or(GRID_DEN);
                    // The position must land on the grid too. A recorded but
                    // off-grid position (e.g. an event sitting mid-tuplet) is
                    // reported as ungriddable rather than silently decomposed as
                    // if it began on the barline; an absent position — a metric
                    // event must have one — defaults to the barline.
                    let position_units = match layout.event_position.get(&eid) {
                        Some(p) => to_grid_units(p.rational()),
                        None => Some(0),
                    };
                    position_units.and_then(|pos| {
                        decompose_metric(pos, duration_units, measure_units).map(|l| (l, None))
                    })
                }
                None => None,
            }
        };

        let Some((lengths, tuplet)) = components_units else {
            taxonomy.decomposition_ungriddable += 1;
            continue;
        };
        if lengths.is_empty() {
            // Zero-duration determinate event: nothing to decompose, but it is a
            // determinate musical duration, so do not mis-count it.
            taxonomy.decomposition_skipped_nonmusical += 1;
            continue;
        }

        let n = lengths.len();
        let components: Vec<NotatedComponent> = lengths
            .into_iter()
            .enumerate()
            .map(|(i, (base_value, dots))| NotatedComponent {
                base_value,
                dots,
                tuplet,
                tied_to_next: i + 1 < n,
            })
            .collect();
        out.insert(
            eid,
            DecompositionAttachment {
                target: eid,
                components,
                source: DecompositionSource::Inferred,
            },
        );
    }

    out
}

/// The precedence rank of a decomposition source: the spec's default order
/// `UserChosen > Imported > Propagated > Inferred` (Chapter 3 §"Sounding
/// Duration and Notational Decomposition" mirrors Chapter 2 §"Configurable
/// Precedence": "same sources, same precedence machinery"). Lower rank wins.
/// The score carries no decomposition-specific precedence configuration (there
/// is no `DecompositionPrecedence` analogue of [`SpellingPrecedence`] in the
/// graph model), so the spec's default order is the fixed rank here; a
/// configurable decomposition precedence is a Pass-12 candidate.
fn decomposition_source_rank(source: &DecompositionSource) -> usize {
    match source {
        DecompositionSource::UserChosen => 0,
        DecompositionSource::Imported { .. } => 1,
        DecompositionSource::Propagated { .. } => 2,
        DecompositionSource::Inferred => 3,
    }
}

/// Resolves the *effective* decomposition for an event: an authored
/// [`DecompositionAttachment`] in `Score::decomposition_attachments` whose
/// source outranks [`DecompositionSource::Inferred`] wins; otherwise the
/// algorithm's inferred decomposition stands. This is the decomposition
/// analogue of [`resolve_spelling`] — Chapter 3: the pre-pass produces inferred
/// decompositions only "for events that lack a higher-precedence attachment",
/// with the "same precedence machinery" as spelling, minus the axes the
/// attachment does not carry (no analysis layers, no `priority` field). Among
/// competing authored attachments the lowest [`decomposition_source_rank`]
/// wins; a remaining tie keeps the first candidate in the score's
/// `decomposition_attachments` order, which is canonical (codec-fixed), so the
/// resolution is deterministic across replicas.
pub fn resolve_decomposition(
    score: &Score,
    event: EventId,
    inferred: DecompositionAttachment,
) -> DecompositionAttachment {
    let inferred_rank = decomposition_source_rank(&DecompositionSource::Inferred);
    let mut best: Option<(usize, &DecompositionAttachment)> = None;
    for att in &score.decomposition_attachments {
        if att.target != event {
            continue;
        }
        let rank = decomposition_source_rank(&att.source);
        if rank >= inferred_rank {
            continue; // does not outrank the inferred default
        }
        best = match best {
            None => Some((rank, att)),
            // Lower rank wins; a full tie keeps `cur` (the earlier attachment
            // in canonical order).
            Some(cur) => {
                if rank < cur.0 {
                    Some((rank, att))
                } else {
                    Some(cur)
                }
            }
        };
    }
    match best {
        Some((_, att)) => att.clone(),
        None => inferred,
    }
}

#[cfg(test)]
mod tests;
