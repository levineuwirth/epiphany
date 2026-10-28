//! The two-mode fuzz: random concurrent histories, each reduced base-free
//! ([`OperationSet::reduce`]) and graph-aware onto an empty base
//! ([`OperationSet::reduce_onto`]), held to each other in every effect, every
//! object and the canonical bytes, with the graph's invariants checked in every
//! replica's view as the history grows.
//!
//! A history is what simulated editors write. Replica 1 authors a genesis (a
//! small score built by operations, the way the importer builds one: an
//! instrument and its staves, a metric region and its meter, staff instances,
//! voices, measures, notes, rests and chords, a quarter-tone with its spelling,
//! a tuplet, a slur, a tie and a beam), which every replica has seen. Then the
//! replicas take turns: one either merges another's history into its own, or
//! authors an operation against its own view, the graph-aware reduction of what
//! it has seen. Each operation names only what that view holds, so the history
//! is valid; concurrency makes some of its references stale by the time the
//! whole history is reduced, which is where the two modes can part. Every
//! [`OperationKind`] is drawn, the editing of notes and pitches most often, and
//! the three other payloads besides: undo of a transaction the view holds (half
//! the time one of its author's three latest, as an editor's undo history
//! offers), resolution of a conflict it records, and (rarely, since only a
//! faulty replica makes one) an equivocation with its resolution. Some turns
//! author an editor's gesture of several operations: a note replaced by a rest,
//! a tie entered with the note it continues into, a quarter-tone with its
//! spelling, a staff, voice or region added and taken away again, a tied pair
//! moved one end at a time in one transaction, a pitch entered with its
//! spelling, a spanner moved to another staff as a command of its own (onto a
//! staff added for it, onto one the view holds, or onto an added one and back
//! again as two commands), an undo and its redo (of a transaction that
//! overwrote values, minting and deleting nothing, so the redo has values to
//! restore, the author revaluing a pitch first where the view holds none; a
//! redo is an undo of the undo, never the command authored again),
//! the author's two latest commands reverted, the older first, an
//! unmeasured passage (a region, a staff, a voice and a note) set in free time
//! by any anchoring discipline, a syllable moved away in a transaction of its
//! own, the slot it left written again (by a new syllable or another moved
//! there) and the move undone, and a passage added and taken away again (a
//! region, an instance of a staff and a voice created in one transaction, a
//! note in it marked): the note entered after the transaction and deleted,
//! orphaning its mark, then the voice, instance and region deleted in turn or
//! the transaction undone; or one or two notes entered in the transaction,
//! the first marked, and the transaction undone; or one or two notes entered
//! in the transaction, which also moves an older mark onto the first of them
//! or the region's start, and the transaction undone, the mark's restoration
//! taking it back to where it stood; and a mark placed at a region's start
//! by a musical offset, moved off it, the region taken out of musical time and
//! the move undone, the migration superseding the mark's restoration. A
//! whole-event modify moves, resizes or revalues an event, writes a chord
//! without a pitch its author sees, writes an event as another kind in its
//! place, or mints a pitch, a chord written with a new one or another kind
//! written as a note of one.
//!
//! Every event kind is written: notes, rests and unpitched notes, and
//! indeterminate, graphic, cue and trajectory events, entered or written over
//! another, a trajectory holding pitches of its own; none names another object
//! (a cue's sources, an indeterminate event's alternatives, a graphic event's
//! objects, a trajectory endpoint naming another event's pitch). A pitch is
//! entered into an event of any kind. A spanner's staves are rewritten, and a
//! migration's aleatoric target takes every anchoring discipline. A spanner is
//! anchored to events and, by arm 67, to a region's start, which holds the
//! region against its delete and undo; a repeat structure is anchored to
//! events only, one anchored to a region a delete or undo removes being the
//! class the owner has parked (P13-D3; C5b review 1's L2). A tuplet is now and
//! then nested in one the view holds (arm 40), so its parent's undo is held and
//! a member's cascade carries it (X5b fix round 1, D87 Q3).
//!
//! Some shapes of those kinds are not written either, though none names
//! another object: a trajectory is linear or exponential, never curved or
//! stepwise, so a step is never drawn (the reducer reads and writes a step as
//! a note's pitch, held by the trajectory test in `epiphany-musicxml`'s
//! `reduction_modes`); an indeterminate event's kind is pitch, duration or
//! choice, never compound; and its hints are empty (no duration bounds, no
//! alternatives, no textual instruction).
//!
//! Of schema major 5's values (X4b), some are written only in part. A wavy
//! line is a spanner's alone, never a slur's, a tie's, or a staff's,
//! instrument's or staff instance's lines, though each stamps major 5; no
//! verdict reads a line style. A marker is anchored to an event or to a
//! region by a musical offset, and, by arm 67 (X5b.9), to a region, an event
//! or a measure by an offset of either kind; never to a wall-clock time. A
//! dynamic is one of four (`p`, `mf`,
//! `sfz`, `niente`), never another nor `Other` with its text; a fermata takes
//! two of its five shapes, a breath mark and a caesura one kind each, and a
//! tempo mark always a quarter's metronome. An ornament stands alone on its
//! note, with an accidental above and never below; a grace is an eighth, a
//! sixteenth or a quarter, of at most one dot.
//!
//! Of reduction version 5's verdicts (X5b), each is written. A region is
//! placed over another's span with an instance of a staff that region holds,
//! or starts at a region's end or at an event, so that a fill overlapping a
//! region holding its staff is refused (`RegionsOverlap`). A region's extent
//! takes an offset of either kind from a region's end, from its own start, or
//! from an event or a measure, refused `WrongRegionTimeModel` where its
//! target's time model does not admit it, and a region so anchored has the
//! region it names migrated to a model of any kind, which strands it where
//! the target does not admit its offset; a region anchored at a note or a
//! measure by a wall-clock offset, in a region first given a model admitting
//! either clock, has that region migrated so too (X5b.8). A user break is anchored to a
//! measure's start, an event or an offset from its region's start, now and
//! then in another region (`TargetMissing`). A constant tempo segment now and
//! then records its end tempo (`TempoMapMalformed`), built in memory since no
//! decoder reads it.
//! Expression text is one of a marker's kinds. Every other holder of a time
//! anchor is written by a wall-clock offset too (X5b.9): a marker, a user break,
//! a time signature, a metric grid's meter change, a tempo segment (at a
//! region's start, a note or a measure, each held against its removal since
//! X5b's fix round 1), a measure, a staff instance's carried clef, key or grid,
//! a region's own meter, break or tempo segment, a migration's own meter, a
//! spanner at a note or a region's start, and a repeat at a note (one at a
//! region is the parked P13-D3), each in a region the view holds,
//! given a model admitting wall-clock time one time in two, refused
//! `WrongRegionTimeModel` where its target admits none, and its region then
//! migrated to a model of any kind one time in two (arm 67); a marker, a
//! spanner, a break, a tempo segment, a time signature or a grid so written,
//! changed in a transaction, its region migrated and the change undone, the
//! migration superseding the restoration (arm 68); and a region placed with its
//! start at a note or a measure its author makes next, which the note's insert
//! or the measure's create then reads (arm 69). That extent names what its
//! author's view does not yet hold, the one name [`valid`] does not hold to
//! the view; since X5b's fix round 1 the extent's write is refused
//! `TargetMissing`, as every holder's naming a target that is not live is.
//! And a time signature set in a transaction, a region's grid then set naming
//! it outside the transaction, and the transaction undone (arm 70), which the
//! grid holds (D86 Q1); and a note, a measure, an empty region or a tuplet made
//! in a transaction, a holder or a nested tuplet then naming it, and the
//! transaction undone or the note or region deleted (arm 71), which the holder
//! holds (D86 Q2, D87 Q1 and Q3). And a region placed carrying a tempo map
//! with two segments on one key, one at a note the view holds and the other
//! at its own start, a measure's or another region's, in either order, and
//! one time in two the note then deleted (arm 72): the create is refused
//! `TempoMapMalformed` (D89). And a region placed in a metric model carrying a
//! tempo map no sequence of `SetTempoSegment` writes could leave, its segments
//! a bar or more into its start out of order, one ending past the next one's
//! start, one ending before its own start, or a linear one without its end
//! tempo (arm 73): the create is refused `TempoMapMalformed` (D90 Q3).
//!
//! Everything is seeded: [`generate`] is a function of its seed and length, so
//! a finding reproduces from both. [`minimize`] shrinks a failing history to
//! one that fails the same way, for a committed regression test.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    check_invariants, AleatoricAnchoringDiscipline, AleatoricTimeModel, AnalysisLayerId, Clef,
    CmnNominal, CueEvent, CueRendering, Event, EventDuration, EventId, EventOrderingDAG,
    EventPosition, GraphicEvent, IdentifiedPitch, IdentityContext, IndeterminacyHints,
    IndeterminacyKind, IndeterminateEvent, InstrumentId, KeySignature, MeasureId, MusicalDuration,
    MusicalPosition, OperationId, PartDefinitionId, Pitch, PitchId, PitchSpaceId,
    PitchSpacePosition, PitchSpelling, PitchedEvent, RationalTime, Region, RegionContent, RegionId,
    RegionTimeModel, RepeatStructureId, ReplicaId, Rest, Score, StaffGroupId, StaffId,
    StaffInstance, StaffInstanceId, StaffPosition, StemConfiguration, StemDirection,
    TimeSignatureId, TrajectoryDisplay, TrajectoryEndpoint, TrajectoryEvent, TrajectoryShape,
    TransactionId, TranspositionInterval, Tuplet, TupletDisplay, TupletId, TupletRatio,
    TypedObjectId, UnpitchedEvent, UnpitchedMemberId, ViewId, Voice, VoiceId, WallClockDuration,
    WallClockTime,
};
use epiphany_determinism::fuzz::SplitMix64;

use crate::causal::CausalContext;
use crate::conflict::ResolutionAction;
use crate::envelope::OperationEnvelope;
use crate::opset::OperationSet;
use crate::payload::{
    ChangeRegionTimeModelOp, CreateAnalysisLayerOp, CreateCrossCuttingOp, CreateInstrumentOp,
    CreateMeasureOp, CreatePartDefinitionOp, CreateRegionOp, CreateRepeatStructureOp,
    CreateStaffGroupOp, CreateStaffInstanceOp, CreateStaffOp, CreateTupletOp, CreateViewOp,
    CreateVoiceOp, CrossCuttingValue, DeleteCrossCuttingOp, DeleteEventOp, DeleteIdentifiedPitchOp,
    DeleteRegionOp, DeleteRepeatStructureOp, DeleteStaffInstanceOp, DeleteVoiceOp, InsertEventOp,
    InsertIdentifiedPitchOp, ModifyCrossCuttingOp, ModifyEventOp, ModifyIdentifiedPitchOp,
    OperationKind, OperationKindTag, OperationPayload, PositionRemapping, ResolveConflictPayload,
    ResolveEquivocationPayload, RespellPitchOp, SetCanvasLayoutDefaultsOp, SetClefOp,
    SetKeySignatureOp, SetMetadataOp, SetMetricGridOp, SetSpellingPrecedenceOp, SetStaffLayoutOp,
    SetTempoSegmentOp, SetTimeSignatureOp, SetTuningContextOp, SetUserPageBreakOp,
    SetUserSystemBreakOp, SetVoiceHomeOp, TransactionDescriptor, TransposeIntervalOp, TransposeOp,
    TupletCompensation,
};
use crate::stamp::{HybridLogicalClock, OperationStamp};
use crate::support::AuthorId;
use crate::undo::{UndoPolicy, UndoTransactionPayload};
use crate::valuegen;
use crate::{GraphMaterialization, MaterializedState, OperationEffect, OperationKindRegistryId};

/// The identity of the empty base both modes start from.
const BASE: ReplicaId = ReplicaId(0);
/// The replica that authors the genesis.
const GENESIS: u64 = 1;
/// How many replicas author, the genesis's included.
const REPLICAS: u64 = 3;

/// Bounded draws from the seeded generator.
struct Rng(SplitMix64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.0.next_u64() % n
        }
    }
    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in.max(1)) == 0
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len() as u64) as usize])
        }
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
}

fn rational(n: i64, d: i64) -> RationalTime {
    RationalTime::new(n, d).expect("a nonzero denominator")
}

fn position(n: i64, d: i64) -> MusicalPosition {
    MusicalPosition(rational(n, d))
}

fn duration(n: i64, d: i64) -> MusicalDuration {
    MusicalDuration(rational(n, d))
}

const NOMINALS: [CmnNominal; 7] = [
    CmnNominal::C,
    CmnNominal::D,
    CmnNominal::E,
    CmnNominal::F,
    CmnNominal::G,
    CmnNominal::A,
    CmnNominal::B,
];

/// A nominal's semitones above C.
fn semitones(nominal: CmnNominal) -> i32 {
    match nominal {
        CmnNominal::C => 0,
        CmnNominal::D => 2,
        CmnNominal::E => 4,
        CmnNominal::F => 5,
        CmnNominal::G => 7,
        CmnNominal::A => 9,
        CmnNominal::B => 11,
    }
}

fn cmn(space: &str, nominal: CmnNominal, alteration: i8, octave: i8) -> Pitch {
    let mut pitch = valuegen::pitch_value();
    pitch.scale_position.space = PitchSpaceId::new(space);
    pitch.scale_position.position = PitchSpacePosition::Cmn {
        nominal,
        alteration,
        octave,
    };
    pitch
}

/// A random pitch: twelve-tone, or one time in `quarter` a quarter-tone in
/// `cmn-24` (its alteration counted in quarter-tones, and odd).
fn random_pitch(rng: &mut Rng, quarter: u64) -> Pitch {
    let nominal = NOMINALS[rng.below(7) as usize];
    let octave = rng.range(3, 5) as i8;
    if rng.chance(quarter) {
        let alteration = [-3i8, -1, 1, 3][rng.below(4) as usize];
        cmn("cmn-24", nominal, alteration, octave)
    } else {
        cmn("cmn-12", nominal, rng.range(-1, 1) as i8, octave)
    }
}

/// The accidental naming `alteration` semitones (`cmn-12`) or quarter-tones
/// (`cmn-24`), as the importer names them.
fn accidental(space: &str, alteration: i8) -> Option<&'static str> {
    match (space, alteration) {
        (_, 0) => None,
        ("cmn-24", -3) => Some("three-quarters-flat"),
        ("cmn-24", -1) => Some("quarter-flat"),
        ("cmn-24", 1) => Some("quarter-sharp"),
        ("cmn-24", 3) => Some("three-quarters-sharp"),
        ("cmn-24", a) => accidental("cmn-12", a / 2),
        (_, -2) => Some("double-flat"),
        (_, -1) => Some("flat"),
        (_, 1) => Some("sharp"),
        (_, 2) => Some("double-sharp"),
        _ => None,
    }
}

fn spelling_of(space: &str, nominal: CmnNominal, alteration: i8, octave: i8) -> PitchSpelling {
    let mut spelling = PitchSpelling::cmn(nominal, octave);
    spelling.accidentals = accidental(space, alteration)
        .map(epiphany_core::AccidentalId::new)
        .into_iter()
        .collect();
    spelling
}

/// A spelling of `pitch`: its own, or a twelve-tone pitch's enharmonic on a
/// neighbouring nominal where one exists within a double sharp or flat.
fn spelling_for(rng: &mut Rng, pitch: &Pitch) -> Option<PitchSpelling> {
    let PitchSpacePosition::Cmn {
        nominal,
        alteration,
        octave,
    } = pitch.scale_position.position
    else {
        return None;
    };
    let space = pitch.scale_position.space.as_str().to_owned();
    if space == "cmn-12" && rng.chance(2) {
        let index = NOMINALS.iter().position(|n| *n == nominal)? as i32;
        let step = if rng.chance(2) { 1 } else { -1 };
        let other = (index + step).rem_euclid(7);
        let octave2 = octave as i32 + (index + step).div_euclid(7);
        let target = 12 * octave as i32 + semitones(nominal) + alteration as i32;
        let alteration2 = target - (12 * octave2 + semitones(NOMINALS[other as usize]));
        if alteration2.abs() <= 2 {
            return Some(spelling_of(
                "cmn-12",
                NOMINALS[other as usize],
                alteration2 as i8,
                octave2 as i8,
            ));
        }
    }
    Some(spelling_of(&space, nominal, alteration, octave))
}

/// What one replica has seen and how it mints.
struct Replica {
    id: ReplicaId,
    next_op: u64,
    identity: IdentityContext,
    /// Indices into the history, closed under each replica's prefix order.
    seen: BTreeSet<usize>,
    /// An open transaction and how many more of this replica's operations it
    /// takes.
    open: Option<(TransactionId, u64)>,
    /// A gesture's later transactions, each opened when its declaration is
    /// authored, with how many operations it takes.
    queued: Vec<(TransactionId, u64)>,
}

/// A replica's view: the graph-aware reduction of what it has seen.
struct View {
    envelopes: Vec<OperationEnvelope>,
    reduced: GraphMaterialization,
}

/// Why a history fails the fuzz: the class of the failure, for minimizing, and
/// what was seen, for the reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// What kind of failure, stable under shrinking: an effect split names the
    /// operation's kind and both effects' shapes; an invariant names the
    /// invariant and its witness's shape ([`witness_shape`]).
    pub class: String,
    /// The particulars.
    pub detail: String,
}

/// The kind of an effect, without its particulars: its variant and every
/// nested `reason` and `kind` it names (`NoOp PreconditionFailedUnderReduction
/// ContainerNotEmpty`), so two effects of one shape share a class.
fn effect_shape(effect: Option<&OperationEffect>) -> String {
    let Some(effect) = effect else {
        return String::from("none");
    };
    let full = format!("{effect:?}");
    let head = full
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(full.as_str(), |i| &full[..i]);
    let mut shape = head.to_owned();
    for marker in ["reason: ", "kind: "] {
        let mut rest = full.as_str();
        while let Some(i) = rest.find(marker) {
            rest = &rest[i + marker.len()..];
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            if end > 0 {
                shape.push(' ');
                shape.push_str(&rest[..end]);
            }
        }
    }
    shape
}

fn kind_name(payload: &OperationPayload) -> String {
    match payload {
        OperationPayload::Primitive(OperationKind::Registered(..)) => String::from("Registered"),
        OperationPayload::Primitive(kind) => format!("{:?}", kind.tag()),
        OperationPayload::ResolveConflict(_) => String::from("ResolveConflict"),
        OperationPayload::UndoTransaction(_) => String::from("UndoTransaction"),
        OperationPayload::ResolveEquivocation(_) => String::from("ResolveEquivocation"),
    }
}

fn empty_base() -> Score {
    Score::empty(IdentityContext::new(BASE))
}

/// Reduces `history` both ways and holds the two to each other: each
/// operation's effect, the objects and the canonical bytes; then the
/// graph-aware score's invariants. The first failure, or `None`.
pub fn check(history: &[OperationEnvelope]) -> Option<Finding> {
    findings(history).into_iter().next()
}

/// Every failure of `history`, in the order [`check`] reports them: each
/// operation whose effect differs, then the objects, the canonical bytes and
/// each invariant the graph-aware score breaks.
pub fn findings(history: &[OperationEnvelope]) -> Vec<Finding> {
    let mut set = OperationSet::new();
    set.accept_all(history.iter().cloned());
    let free = set.reduce();
    let aware = set.reduce_onto(&empty_base());
    compare(history, &free, &aware)
}

fn compare(
    history: &[OperationEnvelope],
    free: &MaterializedState,
    aware: &GraphMaterialization,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let free_effects: BTreeMap<_, _> = free.effects.iter().cloned().collect();
    let aware_effects: BTreeMap<_, _> = aware.state.effects.iter().cloned().collect();
    for envelope in history {
        let (f, a) = (
            free_effects.get(&envelope.id),
            aware_effects.get(&envelope.id),
        );
        if f != a {
            out.push(Finding {
                class: format!(
                    "effect {}: base-free {} | graph-aware {}",
                    kind_name(&envelope.payload),
                    effect_shape(f),
                    effect_shape(a)
                ),
                detail: format!(
                    "{:?}\n  base-free: {f:?}\n  graph-aware: {a:?}",
                    envelope.id
                ),
            });
        }
    }
    if free.objects != aware.state.objects {
        let differing: Vec<_> = free
            .objects
            .iter()
            .filter(|(k, v)| aware.state.objects.get(k) != Some(v))
            .map(|(k, v)| format!("{k:?} base-free {v:?}"))
            .chain(
                aware
                    .state
                    .objects
                    .iter()
                    .filter(|(k, _)| !free.objects.contains_key(k))
                    .map(|(k, v)| format!("{k:?} graph-aware only {v:?}")),
            )
            .collect();
        out.push(Finding {
            class: String::from("objects differ"),
            detail: differing.join("\n"),
        });
    }
    if free.canonical_bytes() != aware.state.canonical_bytes() {
        for (field, differs) in [
            ("conflicts", free.conflicts != aware.state.conflicts),
            ("anomalies", free.anomalies != aware.state.anomalies),
            ("spellings", free.spellings != aware.state.spellings),
            (
                "breaks",
                free.breaks != aware.state.breaks || free.page_breaks != aware.state.page_breaks,
            ),
            ("pending", free.pending != aware.state.pending),
        ] {
            if differs {
                out.push(Finding {
                    class: format!("canonical bytes differ in {field}"),
                    detail: String::new(),
                });
            }
        }
    }
    out.extend(invariant_findings(&aware.score));
    out
}

/// Every invariant the graph-aware score breaks, each class once: the
/// invariant and its witness's shape. Nothing is excepted: the region class
/// D48 and D50 to D52 deferred closed when reduction version 5 refused a fill
/// whose region overlaps another holding its staff (X5b), and its machinery
/// left the fuzz with it.
fn invariant_findings(score: &Score) -> Vec<Finding> {
    let mut seen = BTreeSet::new();
    check_invariants(score)
        .into_iter()
        .filter_map(|violation| {
            let full = format!("{:?}", violation.kind);
            let class = format!(
                "invariant {}: {}",
                full.trim_end_matches(')'),
                witness_shape(&violation.witness)
            );
            seen.insert(class.clone()).then(|| Finding {
                class,
                detail: format!("{violation:?}"),
            })
        })
        .collect()
}

/// The shape of an invariant's witness, which classes a finding with its
/// invariant so that a run lists each cause once rather than each invariant
/// once: the witness with each identifier reduced to its kind (`StaffId`),
/// each number to `#`, and each list's runs of equal items to one, so two
/// findings of one cause share a shape whatever objects and how many they
/// name.
pub fn witness_shape(witness: &str) -> String {
    let chars: Vec<char> = witness.chars().collect();
    let mut stripped = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            while i < chars.len() && chars[i].is_ascii_hexdigit() {
                i += 1;
            }
            stripped.push('#');
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if word.ends_with("Id") && chars.get(i) == Some(&'(') {
                if let Some(close) = chars[i..].iter().position(|c| *c == ')') {
                    i += close + 1;
                }
            }
            stripped.push_str(&word);
            continue;
        }
        stripped.push(c);
        i += 1;
    }
    let chars: Vec<char> = stripped.chars().collect();
    collapse_lists(&chars, &mut 0, None)
}

/// `chars` from `*at` to the bracket closing `close` (or the end), the runs of
/// equal comma-separated items in each bracket (a list, a set, a tuple; a
/// struct's fields are never equal) collapsed to one.
fn collapse_lists(chars: &[char], at: &mut usize, close: Option<char>) -> String {
    let list = close.is_some();
    let mut items: Vec<String> = vec![String::new()];
    while *at < chars.len() {
        let c = chars[*at];
        *at += 1;
        if Some(c) == close {
            break;
        }
        let item = items.last_mut().expect("never empty");
        match c {
            '[' | '(' | '{' => {
                let closing = match c {
                    '[' => ']',
                    '(' => ')',
                    _ => '}',
                };
                let inner = collapse_lists(chars, at, Some(closing));
                item.push(c);
                item.push_str(&inner);
                item.push(closing);
            }
            ',' if list && chars.get(*at) == Some(&' ') => {
                *at += 1;
                items.push(String::new());
            }
            _ => item.push(c),
        }
    }
    items.dedup();
    items.join(", ")
}

/// Counts per operation kind over a run: authored, and applied (with or
/// without a repair) in the final base-free reduction.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    pub authored: BTreeMap<String, u64>,
    pub applied: BTreeMap<String, u64>,
    /// Each kind's other effects, by shape.
    pub other: BTreeMap<(String, String), u64>,
}

impl Coverage {
    fn record(&mut self, authored: &[OperationEnvelope], state: &MaterializedState) {
        let effects: BTreeMap<_, _> = state.effects.iter().cloned().collect();
        let gestures = gesture_shapes(authored);
        for (envelope, gesture) in authored.iter().zip(gestures) {
            let effect = effects.get(&envelope.id);
            let applied = matches!(
                effect,
                Some(OperationEffect::Applied | OperationEffect::AppliedWithRepair { .. })
            );
            let names = std::iter::once(kind_name(&envelope.payload)).chain(
                shape_names(&envelope.payload)
                    .into_iter()
                    .chain(gesture)
                    .map(String::from),
            );
            for name in names {
                *self.authored.entry(name.clone()).or_default() += 1;
                if applied {
                    *self.applied.entry(name).or_default() += 1;
                } else {
                    *self.other.entry((name, effect_shape(effect))).or_default() += 1;
                }
            }
        }
    }

    /// Every kind the generator draws.
    pub fn kinds() -> Vec<String> {
        use epiphany_determinism::CanonicalDecode;
        let mut kinds: Vec<String> = (0..=u8::MAX)
            .filter_map(|d| OperationKindTag::decode_canonical(&[d]).ok())
            .map(|t| format!("{t:?}"))
            .collect();
        kinds.push(String::from("Registered"));
        kinds.extend(
            ["ResolveConflict", "UndoTransaction", "ResolveEquivocation"]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        kinds.extend(SHAPES.iter().map(|s| (*s).to_owned()));
        kinds
    }
}

/// The payload shapes schema major 5 adds, and those reduction version 5
/// reads (X5b), each counted beside its kind; and arm 70's gesture, read
/// from the history ([`gesture_shapes`]).
const SHAPES: [&str; 40] = [
    "InsertEvent+marks",
    "InsertEvent+ornaments",
    "InsertEvent+grace",
    "ModifyEvent+marks",
    "ModifyEvent+ornaments",
    "ModifyEvent+grace",
    "CreateCrossCutting+Marker",
    "CreateCrossCutting+Lyric",
    "CreateCrossCutting+wavy line",
    "CreateCrossCutting+pedal bracket",
    "ModifyCrossCutting+Marker",
    "ModifyCrossCutting+Lyric",
    "DeleteCrossCutting+Marker",
    "DeleteCrossCutting+Lyric",
    "CreateCrossCutting+expression text",
    "SetUserSystemBreak+measure anchor",
    "SetUserSystemBreak+event anchor",
    "SetUserPageBreak+measure anchor",
    "SetUserPageBreak+event anchor",
    "CreateRegion+anchored start",
    "CreateRegion+musical offset",
    "CreateRegion+wall-clock offset",
    "CreateRegion+anchored start, placed end",
    "CreateRegion+start at a region's end, placed end",
    "CreateCrossCutting+wall-clock offset",
    "ModifyCrossCutting+wall-clock offset",
    "CreateRepeatStructure+wall-clock offset",
    "CreateMeasure+wall-clock offset",
    "SetUserSystemBreak+wall-clock offset",
    "SetUserPageBreak+wall-clock offset",
    "SetTimeSignature+wall-clock offset",
    "SetMetricGrid+wall-clock offset",
    "SetTempoSegment+wall-clock offset",
    "CreateStaffInstance+carried wall-clock offset",
    "CreateRegion+carried wall-clock offset",
    "SetTempoSegment+event anchor",
    "CreateCrossCutting+spanner at a region",
    "CreateTuplet+parent",
    "CreateTuplet+parent, several members",
    "UndoTransaction+a signature a later grid names",
];

/// The shapes reduction version 5 refuses wherever they are written, each
/// counted beside its kind: a constant tempo segment recording its end tempo,
/// and a region created carrying a tempo map with two segments on one key, or
/// one out of order, overlapping, ending before its start or without its end
/// data (each `TempoMapMalformed`), which version 4 applied; and arm 69's
/// region whose extent names what its author makes next, refused
/// `TargetMissing` since X5b's fix round 1, read from the history
/// (`gesture_shapes`). A user break
/// naming another region is counted too ("user break+another region's
/// anchor"), where its anchor shows it, but a view seldom holds two regions
/// to choose from.
pub const REFUSED: [&str; 7] = [
    "SetTempoSegment+constant with an end tempo",
    "CreateRegion+tempo map with two segments on one key",
    "CreateRegion+tempo map out of order",
    "CreateRegion+tempo map overlapping",
    "CreateRegion+tempo segment ending before its start",
    "CreateRegion+tempo segment without its end data",
    "CreateRegion+extent at what its author makes next",
];

/// The shapes of a gesture its payload alone does not show, read from the
/// authored history around each envelope, so that the CI budget counts the
/// arm's reach (D90 Q2): a region whose extent names a note or a measure an
/// envelope after it makes (arm 69, refused `TargetMissing` since X5b's fix
/// round 1), and an undo of a transaction that set a time signature a grid
/// set outside it names (arm 70, held while the grid names it, D86 Q1).
fn gesture_shapes(authored: &[OperationEnvelope]) -> Vec<Vec<&'static str>> {
    let mut minted_at: BTreeMap<TypedObjectId, usize> = BTreeMap::new();
    for (index, envelope) in authored.iter().enumerate() {
        for object in mints_and_refs(&envelope.payload).0 {
            minted_at.entry(object).or_insert(index);
        }
    }
    authored
        .iter()
        .enumerate()
        .map(|(index, envelope)| {
            let mut out = Vec::new();
            if names_ahead(&envelope.payload)
                .iter()
                .any(|object| minted_at.get(object).is_some_and(|at| *at > index))
            {
                out.push("CreateRegion+extent at what its author makes next");
            }
            if let OperationPayload::UndoTransaction(undo) = &envelope.payload {
                let before = &authored[..index];
                let signed: Vec<TimeSignatureId> = before
                    .iter()
                    .filter(|e| e.transaction == Some(undo.target))
                    .filter_map(|e| match &e.payload {
                        OperationPayload::Primitive(OperationKind::SetTimeSignature(op)) => {
                            op.time_signature.as_ref().map(|signature| signature.id)
                        }
                        _ => None,
                    })
                    .collect();
                let gridded = before.iter().any(|e| {
                    e.transaction != Some(undo.target)
                        && matches!(&e.payload,
                            OperationPayload::Primitive(OperationKind::SetMetricGrid(op))
                                if op.grid.iter().flat_map(|grid| &grid.meter_sequence)
                                    .any(|change| signed.contains(&change.time_signature)))
                });
                if gridded {
                    out.push("UndoTransaction+a signature a later grid names");
                }
            }
            out
        })
        .collect()
}

/// Which of [`SHAPES`] a payload has, with the holders' wall-clock offsets
/// ([`wall_clock_shape`]).
fn shape_names(payload: &OperationPayload) -> Vec<&'static str> {
    let mut names = kind_shapes(payload);
    names.extend(wall_clock_shape(payload));
    names
}

/// The shape of a holder of a time anchor written by a wall-clock offset
/// (X5b.9): a marker or a spanner created or modified, a repeat, a measure, a
/// user break, a time signature, a grid, a tempo segment, the changes a staff
/// instance's create carries, and the model, grid, breaks and tempo map a
/// region's create carries.
fn wall_clock_shape(payload: &OperationPayload) -> Option<&'static str> {
    use epiphany_core::TimeAnchor;
    let OperationPayload::Primitive(kind) = payload else {
        return None;
    };
    let cross = |value: &CrossCuttingValue| -> Vec<TimeAnchor> {
        match value {
            CrossCuttingValue::Marker(m) => vec![m.anchor.clone()],
            CrossCuttingValue::Spanner(s) => vec![s.start.clone(), s.end.clone()],
            _ => Vec::new(),
        }
    };
    let (anchors, name): (Vec<TimeAnchor>, &'static str) = match kind {
        OperationKind::CreateCrossCutting(op) => {
            (cross(&op.structure), "CreateCrossCutting+wall-clock offset")
        }
        OperationKind::ModifyCrossCutting(op) => {
            (cross(&op.structure), "ModifyCrossCutting+wall-clock offset")
        }
        OperationKind::CreateRepeatStructure(op) => (
            op.repeat.anchor_sites().into_iter().cloned().collect(),
            "CreateRepeatStructure+wall-clock offset",
        ),
        OperationKind::CreateMeasure(op) => (
            vec![op.measure.start.clone()],
            "CreateMeasure+wall-clock offset",
        ),
        OperationKind::SetUserSystemBreak(op) => (
            vec![op.anchor.clone()],
            "SetUserSystemBreak+wall-clock offset",
        ),
        OperationKind::SetUserPageBreak(op) => (
            vec![op.anchor.clone()],
            "SetUserPageBreak+wall-clock offset",
        ),
        OperationKind::SetTimeSignature(op) => (
            vec![op.anchor.clone()],
            "SetTimeSignature+wall-clock offset",
        ),
        OperationKind::SetMetricGrid(op) => (
            op.grid
                .iter()
                .flat_map(|g| g.meter_sequence.iter().map(|m| m.anchor.clone()))
                .collect(),
            "SetMetricGrid+wall-clock offset",
        ),
        OperationKind::SetTempoSegment(op) => (
            op.segment
                .iter()
                .flat_map(|s| std::iter::once(s.start.clone()).chain(s.end.clone()))
                .collect(),
            "SetTempoSegment+wall-clock offset",
        ),
        OperationKind::CreateStaffInstance(op) => (
            op.instance
                .clef_sequence
                .iter()
                .map(|c| c.anchor.clone())
                .chain(op.instance.key_sequence.iter().map(|k| k.anchor.clone()))
                .chain(
                    op.instance
                        .local_metric_grid
                        .iter()
                        .flat_map(|g| g.meter_sequence.iter().map(|m| m.anchor.clone())),
                )
                .collect(),
            "CreateStaffInstance+carried wall-clock offset",
        ),
        OperationKind::CreateRegion(op) => {
            let region = &op.region;
            let mut anchors = Vec::new();
            if let RegionTimeModel::Metric(m) = &region.time_model {
                anchors.extend(m.meters.iter().map(|m| m.anchor.clone()));
            }
            if let Some(c) = region.content.staff_based() {
                anchors.extend(
                    c.default_metric_grid
                        .iter()
                        .flat_map(|g| g.meter_sequence.iter().map(|m| m.anchor.clone())),
                );
                anchors.extend(c.user_system_breaks.iter().cloned());
                anchors.extend(c.user_page_breaks.iter().cloned());
            }
            for segment in region.local_tempo_map.iter().flat_map(|m| &m.segments) {
                anchors.push(segment.start.clone());
                anchors.extend(segment.end.clone());
            }
            (anchors, "CreateRegion+carried wall-clock offset")
        }
        _ => return None,
    };
    anchors
        .iter()
        .any(|a| matches!(a.offset(), Some(epiphany_core::AnchorOffset::WallClock(_))))
        .then_some(name)
}

/// Which of [`SHAPES`] a payload has, by its kind's own shapes.
fn kind_shapes(payload: &OperationPayload) -> Vec<&'static str> {
    let OperationPayload::Primitive(kind) = payload else {
        return Vec::new();
    };
    let event = |event: &Event, prefix: [&'static str; 3]| {
        let (marks, ornaments, grace) = match event {
            Event::Pitched(p) => (
                !p.marks.is_empty(),
                !p.ornaments.is_empty(),
                p.grace.is_some(),
            ),
            Event::Unpitched(u) => (!u.marks.is_empty(), false, u.grace.is_some()),
            _ => (false, false, false),
        };
        [marks, ornaments, grace]
            .iter()
            .zip(prefix)
            .filter(|(has, _)| **has)
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
    };
    let structure = |value: &CrossCuttingValue, marker: &'static str, lyric: &'static str| {
        let mut out = Vec::new();
        match value {
            CrossCuttingValue::Marker(m) => {
                out.push(marker);
                if matches!(m.kind, epiphany_core::MarkerKind::ExpressionText(_))
                    && marker.starts_with("Create")
                {
                    out.push("CreateCrossCutting+expression text");
                }
            }
            CrossCuttingValue::Lyric(_) => out.push(lyric),
            CrossCuttingValue::Spanner(s) => {
                if matches!(s.style.line, epiphany_core::LineStyle::Wavy)
                    && marker.starts_with("Create")
                {
                    out.push("CreateCrossCutting+wavy line");
                }
                if matches!(s.kind, epiphany_core::SpannerKind::PedalBracket(_))
                    && marker.starts_with("Create")
                {
                    out.push("CreateCrossCutting+pedal bracket");
                }
                if [&s.start, &s.end]
                    .into_iter()
                    .any(|anchor| matches!(anchor, epiphany_core::TimeAnchor::Region { .. }))
                    && marker.starts_with("Create")
                {
                    out.push("CreateCrossCutting+spanner at a region");
                }
            }
            _ => {}
        }
        out
    };
    match kind {
        OperationKind::InsertEvent(op) => event(
            &op.event,
            [
                "InsertEvent+marks",
                "InsertEvent+ornaments",
                "InsertEvent+grace",
            ],
        ),
        OperationKind::ModifyEvent(op) => event(
            &op.event,
            [
                "ModifyEvent+marks",
                "ModifyEvent+ornaments",
                "ModifyEvent+grace",
            ],
        ),
        OperationKind::CreateCrossCutting(op) => structure(
            &op.structure,
            "CreateCrossCutting+Marker",
            "CreateCrossCutting+Lyric",
        ),
        OperationKind::ModifyCrossCutting(op) => structure(
            &op.structure,
            "ModifyCrossCutting+Marker",
            "ModifyCrossCutting+Lyric",
        ),
        OperationKind::DeleteCrossCutting(op) => match op.structure {
            TypedObjectId::Marker(_) => vec!["DeleteCrossCutting+Marker"],
            TypedObjectId::LyricLine(_) => vec!["DeleteCrossCutting+Lyric"],
            _ => Vec::new(),
        },
        OperationKind::SetUserSystemBreak(op) => user_break(
            op.region,
            &op.anchor,
            [
                "SetUserSystemBreak+measure anchor",
                "SetUserSystemBreak+event anchor",
            ],
        ),
        OperationKind::SetUserPageBreak(op) => user_break(
            op.region,
            &op.anchor,
            [
                "SetUserPageBreak+measure anchor",
                "SetUserPageBreak+event anchor",
            ],
        ),
        OperationKind::CreateRegion(op) => {
            let extent = &op.region.time_extent;
            let offsets: Vec<_> = [&extent.start, &extent.end]
                .into_iter()
                .filter_map(|anchor| anchor.offset())
                .collect();
            let mut out = Vec::new();
            let mut keys = std::collections::BTreeSet::new();
            if !op
                .region
                .local_tempo_map
                .iter()
                .flat_map(|map| &map.segments)
                .all(|segment| {
                    keys.insert(crate::payload::resolved_anchor_position(&segment.start))
                })
            {
                out.push("CreateRegion+tempo map with two segments on one key");
            }
            // Arm 73's shapes, each a map no `SetTempoSegment` writes leave
            // (D90 Q3), read by its segments' resolved positions in order.
            let keyed: Vec<_> = op
                .region
                .local_tempo_map
                .iter()
                .flat_map(|map| &map.segments)
                .map(|segment| {
                    let at = crate::payload::resolved_anchor_position;
                    (at(&segment.start), segment.end.as_ref().map(at), segment)
                })
                .collect();
            if keyed.windows(2).any(|pair| pair[0].0 > pair[1].0) {
                out.push("CreateRegion+tempo map out of order");
            }
            if keyed
                .windows(2)
                .any(|pair| pair[0].1.as_ref().is_some_and(|end| *end > pair[1].0))
            {
                out.push("CreateRegion+tempo map overlapping");
            }
            if keyed
                .iter()
                .any(|(start, end, _)| end.as_ref().is_some_and(|end| end < start))
            {
                out.push("CreateRegion+tempo segment ending before its start");
            }
            if keyed.iter().any(|(_, _, segment)| {
                !matches!(segment.shape, epiphany_core::TempoShape::Constant)
                    && (segment.end_tempo.is_none() || segment.end.is_none())
            }) {
                out.push("CreateRegion+tempo segment without its end data");
            }
            if !matches!(extent.start, epiphany_core::TimeAnchor::WallClock { .. }) {
                out.push("CreateRegion+anchored start");
                // Arm 15's own gesture for the overlap refusal: a start with
                // no offset at a region's end or a note, the end where an
                // editor places one (D84 Q4).
                if matches!(
                    extent.start.offset(),
                    Some(epiphany_core::AnchorOffset::Zero)
                ) && matches!(extent.end, epiphany_core::TimeAnchor::WallClock { .. })
                {
                    out.push("CreateRegion+anchored start, placed end");
                    // Arm 15's start at a region's end alone: arm 71's
                    // holders start with no offset at a note, a measure or a
                    // region's start, and arm 66's at a region's end carry an
                    // anchored end (X5b fix round 1).
                    if matches!(
                        extent.start,
                        epiphany_core::TimeAnchor::Region {
                            edge: epiphany_core::RegionEdge::End,
                            ..
                        }
                    ) {
                        out.push("CreateRegion+start at a region's end, placed end");
                    }
                }
            }
            // An offset from a region's edge, which arms 15 and 66 draw of a
            // kind its region may not admit (D84 Q4).
            if [&extent.start, &extent.end].into_iter().any(|anchor| {
                matches!(anchor, epiphany_core::TimeAnchor::Region { offset, .. }
                    if !matches!(offset, epiphany_core::AnchorOffset::Zero))
            }) {
                out.push("CreateRegion+offset at a region's edge");
            }
            if offsets
                .iter()
                .any(|o| matches!(o, epiphany_core::AnchorOffset::Musical(_)))
            {
                out.push("CreateRegion+musical offset");
            }
            if offsets
                .iter()
                .any(|o| matches!(o, epiphany_core::AnchorOffset::WallClock(_)))
            {
                out.push("CreateRegion+wall-clock offset");
            }
            out
        }
        OperationKind::SetTempoSegment(op) => {
            let mut out: Vec<&'static str> = op
                .segment
                .as_ref()
                .filter(|s| matches!(s.shape, epiphany_core::TempoShape::Constant))
                .filter(|s| s.end_tempo.is_some())
                .map(|_| "SetTempoSegment+constant with an end tempo")
                .into_iter()
                .collect();
            if op.segment.is_some() && matches!(op.start, epiphany_core::TimeAnchor::Event { .. }) {
                out.push("SetTempoSegment+event anchor");
            }
            out
        }
        OperationKind::CreateTuplet(op) if op.tuplet.parent.is_some() => {
            let mut out = vec!["CreateTuplet+parent"];
            // Arm 40's nest alone: arm 71's nested tuplet holds one member
            // (X5b fix round 1).
            if op.tuplet.members.len() > 1 {
                out.push("CreateTuplet+parent, several members");
            }
            out
        }
        _ => Vec::new(),
    }
}

/// A user break's shape: its anchor's kind, and whether it names another
/// region. A measure or a note counts only by no offset, arms 5 and 23's own
/// gesture: the holders' arms write each by an offset (X5b.9), and counted
/// so they would hide that gesture's loss.
fn user_break(
    region: RegionId,
    anchor: &epiphany_core::TimeAnchor,
    names: [&'static str; 2],
) -> Vec<&'static str> {
    use epiphany_core::AnchorOffset::Zero;
    match anchor {
        epiphany_core::TimeAnchor::Measure { offset: Zero, .. } => vec![names[0]],
        epiphany_core::TimeAnchor::Event { offset: Zero, .. } => vec![names[1]],
        epiphany_core::TimeAnchor::Region { id, .. } if *id != region => {
            vec!["user break+another region's anchor"]
        }
        _ => Vec::new(),
    }
}

/// The history's simulation.
struct Simulation {
    rng: Rng,
    history: Vec<OperationEnvelope>,
    replicas: Vec<Replica>,
    clock: i64,
    /// Every class of invariant breach or mode split any view showed, each
    /// with the first view's history that showed it; every view is checked.
    view_findings: BTreeMap<String, (Vec<OperationEnvelope>, Finding)>,
}

impl Simulation {
    fn new(seed: u64) -> Self {
        let replicas = (1..=REPLICAS)
            .map(|r| Replica {
                id: ReplicaId(r),
                next_op: 0,
                identity: IdentityContext::new(ReplicaId(r)),
                seen: BTreeSet::new(),
                open: None,
                queued: Vec::new(),
            })
            .collect();
        Simulation {
            rng: Rng(SplitMix64::new(seed)),
            history: Vec::new(),
            replicas,
            clock: 0,
            view_findings: BTreeMap::new(),
        }
    }

    fn context(&self, r: usize) -> CausalContext {
        let mut high: BTreeMap<ReplicaId, u64> = BTreeMap::new();
        for &i in &self.replicas[r].seen {
            let id = self.history[i].id;
            let slot = high.entry(id.replica).or_insert(id.counter);
            *slot = (*slot).max(id.counter);
        }
        high.into_iter()
            .fold(CausalContext::new(), |c, (replica, counter)| {
                c.with_seen(replica, counter)
            })
    }

    /// Authors `payload` on replica `r`, in its open transaction if any; the
    /// declaration of a gesture's queued transaction opens it.
    fn author(&mut self, r: usize, payload: OperationPayload) -> OperationId {
        if let OperationPayload::Primitive(OperationKind::DeclareTransaction(d)) = &payload {
            let replica = &mut self.replicas[r];
            if replica.open.is_none() && replica.queued.first().is_some_and(|(tx, _)| *tx == d.id) {
                replica.open = Some(replica.queued.remove(0));
            }
        }
        let transaction = match &mut self.replicas[r].open {
            Some((tx, left)) => {
                let tx = *tx;
                *left = left.saturating_sub(1);
                if *left == 0 {
                    self.replicas[r].open = None;
                }
                Some(tx)
            }
            None => None,
        };
        self.author_in(r, payload, transaction)
    }

    fn author_in(
        &mut self,
        r: usize,
        payload: OperationPayload,
        transaction: Option<TransactionId>,
    ) -> OperationId {
        let causal_context = self.context(r);
        let replica = &mut self.replicas[r];
        let id = OperationId::new(replica.id, replica.next_op);
        replica.next_op += 1;
        self.clock += 1;
        self.history.push(OperationEnvelope {
            id,
            author: AuthorId(u128::from(replica.id.0)),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(self.clock), 0), id),
            causal_context,
            transaction,
            payload,
        });
        let index = self.history.len() - 1;
        self.replicas[r].seen.insert(index);
        id
    }

    fn merge(&mut self, into: usize, from: usize) {
        let theirs = self.replicas[from].seen.clone();
        self.replicas[into].seen.extend(theirs);
    }

    fn view(&mut self, r: usize) -> View {
        let envelopes: Vec<OperationEnvelope> = self.replicas[r]
            .seen
            .iter()
            .map(|&i| self.history[i].clone())
            .collect();
        let mut set = OperationSet::new();
        set.accept_all(envelopes.iter().cloned());
        let reduced = set.reduce_onto(&empty_base());
        let free = set.reduce();
        for finding in compare(&envelopes, &free, &reduced) {
            self.view_findings
                .entry(finding.class.clone())
                .or_insert_with(|| (envelopes.clone(), finding));
        }
        View { envelopes, reduced }
    }

    fn mint<T: epiphany_core::GraphId>(&mut self, r: usize) -> T {
        self.replicas[r].identity.mint()
    }
}

/// What a view holds, gathered once per authoring turn.
struct Holdings<'a> {
    score: &'a Score,
    state: &'a MaterializedState,
    envelopes: &'a [OperationEnvelope],
    /// Each staff-based region's id, its time model, and its instances.
    regions: Vec<(RegionId, &'a RegionTimeModel, Vec<&'a StaffInstance>)>,
    /// Each voice with its instance and region.
    voices: Vec<(VoiceId, StaffInstanceId, RegionId)>,
    events: Vec<&'a Event>,
    pitches: Vec<(EventId, &'a IdentifiedPitch)>,
    instance_of_voice: BTreeMap<VoiceId, StaffInstanceId>,
}

impl<'a> Holdings<'a> {
    fn of(view: &'a View) -> Self {
        let score = &view.reduced.score;
        let mut regions = Vec::new();
        let mut voices = Vec::new();
        let mut instance_of_voice = BTreeMap::new();
        for region in &score.canvas.regions {
            let content = match &region.content {
                RegionContent::StaffBased(c) => c,
                RegionContent::Hybrid { staves, .. } => staves,
                RegionContent::FreeGraphic(_) => continue,
            };
            let instances: Vec<&StaffInstance> = content.staff_instances.iter().collect();
            let metric = matches!(region.time_model, RegionTimeModel::Metric(_));
            for instance in &instances {
                for voice in &instance.voices {
                    if metric {
                        voices.push((voice.id, instance.id, region.id));
                    }
                    instance_of_voice.insert(voice.id, instance.id);
                }
            }
            regions.push((region.id, &region.time_model, instances));
        }
        let events: Vec<&Event> = score.events.iter_canonical().collect();
        let pitches = events
            .iter()
            .filter_map(|e| match e {
                Event::Pitched(p) => Some(p),
                _ => None,
            })
            .flat_map(|p| p.pitches.iter().map(move |ip| (p.id, ip)))
            .collect();
        Holdings {
            score,
            state: &view.reduced.state,
            envelopes: &view.envelopes,
            regions,
            voices,
            events,
            pitches,
            instance_of_voice,
        }
    }

    fn instances(&self) -> Vec<(RegionId, &'a StaffInstance)> {
        self.regions
            .iter()
            .flat_map(|(r, _, is)| is.iter().map(move |i| (*r, *i)))
            .collect()
    }

    /// The staff-based regions a musical anchor or position may name.
    fn metric(&self) -> Vec<(RegionId, &'a RegionTimeModel, Vec<&'a StaffInstance>)> {
        self.regions
            .iter()
            .filter(|(_, m, _)| matches!(m, RegionTimeModel::Metric(_)))
            .cloned()
            .collect()
    }

    /// The instances of metric regions.
    fn metric_instances(&self) -> Vec<(RegionId, &'a StaffInstance)> {
        self.metric()
            .into_iter()
            .flat_map(|(r, _, is)| is.into_iter().map(move |i| (r, i)))
            .collect()
    }

    /// Whether `voice` holds no event.
    fn empty_voice(&self, voice: VoiceId) -> bool {
        !self.events.iter().any(|e| e.voice() == voice)
    }

    /// The musical span of each event of `voice`.
    fn spans(&self, voice: VoiceId) -> Vec<(MusicalPosition, MusicalPosition)> {
        self.events
            .iter()
            .filter(|e| e.voice() == voice)
            .filter_map(|e| span(e))
            .collect()
    }

    fn measures_of(&self, instance: StaffInstanceId) -> usize {
        self.instances()
            .iter()
            .find(|(_, i)| i.id == instance)
            .map_or(0, |(_, i)| i.measures.len())
    }
}

fn span(event: &Event) -> Option<(MusicalPosition, MusicalPosition)> {
    match (event.position(), event.duration()) {
        (EventPosition::Musical(p), EventDuration::Musical(d)) => {
            Some((p.clone(), p.clone() + d.clone()))
        }
        _ => None,
    }
}

fn overlaps(
    spans: &[(MusicalPosition, MusicalPosition)],
    start: &MusicalPosition,
    end: &MusicalPosition,
) -> bool {
    spans.iter().any(|(s, e)| s < end && start < e)
}

/// A duration an author enters, in whole notes.
fn random_duration(rng: &mut Rng) -> MusicalDuration {
    match rng.below(6) {
        0 => duration(1, 8),
        1 => duration(1, 4),
        2 => duration(1, 2),
        3 => duration(3, 8),
        4 => duration(1, 16),
        _ => duration(1, 4),
    }
}

/// A few marks of any kinds, as a canonical set (schema major 5).
fn random_marks(rng: &mut Rng) -> Vec<epiphany_core::EventMark> {
    use epiphany_core::{ArpeggioDirection, EventMark as M};
    let count = 1 + rng.below(3);
    let marks: Vec<M> = (0..count)
        .map(|_| match rng.below(22) {
            0 => M::Staccato,
            1 => M::Staccatissimo,
            2 => M::Spiccato,
            3 => M::Tenuto,
            4 => M::DetachedLegato,
            5 => M::Accent,
            6 => M::Marcato,
            7 => M::Stress,
            8 => M::Unstress,
            9 => M::UpBow,
            10 => M::DownBow,
            11 => M::Harmonic,
            12 => M::OpenString,
            13 => M::Stopped,
            14 => M::SnapPizzicato,
            15 => M::Scoop,
            16 => M::Plop,
            17 => M::Doit,
            18 => M::Falloff,
            19 => M::Tremolo {
                strokes: 1 + rng.below(4) as u8,
            },
            20 => M::TremoloWithNext {
                strokes: 1 + rng.below(4) as u8,
            },
            _ => M::Arpeggio {
                direction: [
                    ArpeggioDirection::Plain,
                    ArpeggioDirection::Up,
                    ArpeggioDirection::Down,
                ][rng.below(3) as usize],
            },
        })
        .collect();
    epiphany_core::canonical_marks(marks)
}

/// An ornament, with an accidental above it a third of the time.
fn random_ornaments(rng: &mut Rng) -> Vec<epiphany_core::Ornament> {
    use epiphany_core::OrnamentKind as K;
    let kind = [
        K::Trill,
        K::Mordent,
        K::InvertedMordent,
        K::Turn,
        K::InvertedTurn,
    ][rng.below(5) as usize];
    let accidental_above = rng.chance(3).then(|| {
        epiphany_core::AccidentalId::new(["flat", "sharp", "natural"][rng.below(3) as usize])
    });
    vec![epiphany_core::Ornament {
        kind,
        accidental_above,
        accidental_below: None,
    }]
}

/// A grace note's notation, in an order among its position's graces.
fn random_grace(rng: &mut Rng) -> epiphany_core::Grace {
    use epiphany_core::{GraceKind, NoteValue};
    epiphany_core::Grace {
        kind: if rng.chance(2) {
            GraceKind::Acciaccatura
        } else {
            GraceKind::Appoggiatura
        },
        value: [NoteValue::Eighth, NoteValue::Sixteenth, NoteValue::Quarter][rng.below(3) as usize],
        dots: u8::from(rng.chance(6)),
        order: rng.below(3) as u16,
    }
}

/// A short text, one of them composed (an NFC `é`).
fn random_text(rng: &mut Rng) -> epiphany_core::Text {
    epiphany_core::Text::new(["la", "lo", "\u{e9}", "cresc."][rng.below(4) as usize])
}

/// A point mark of any kind.
fn random_marker_kind(rng: &mut Rng) -> epiphany_core::MarkerKind {
    use epiphany_core::{
        BreathMark, CaesuraMark, Dynamic, Fermata, FermataShape, MarkerKind, Metronome, NoteValue,
        TempoMark,
    };
    match rng.below(10) {
        0 => MarkerKind::Dynamic(
            [Dynamic::P, Dynamic::Mf, Dynamic::Sfz, Dynamic::Niente][rng.below(4) as usize].clone(),
        ),
        1 => MarkerKind::Fermata(Fermata {
            shape: [FermataShape::Normal, FermataShape::Long][rng.below(2) as usize],
            inverted: rng.chance(4),
        }),
        2 => MarkerKind::Breath(BreathMark::Comma),
        3 => MarkerKind::Caesura(CaesuraMark::Thick),
        4 => MarkerKind::Text(random_text(rng)),
        5 => MarkerKind::Tempo(TempoMark {
            text: rng.chance(2).then(|| random_text(rng)),
            metronome: Some(Metronome {
                beat: NoteValue::Quarter,
                dots: 0,
                per_minute: epiphany_core::Text::new("96"),
            }),
        }),
        6 => MarkerKind::Rehearsal(epiphany_core::Text::new("A")),
        7 => MarkerKind::Segno,
        8 => MarkerKind::Coda,
        _ => MarkerKind::ExpressionText(random_text(rng)),
    }
}

/// A lyric syllable on `event`, in one of two verses.
fn random_lyric(
    id: epiphany_core::LyricLineId,
    event: EventId,
    rng: &mut Rng,
) -> epiphany_core::Lyric {
    use epiphany_core::Syllabic;
    epiphany_core::Lyric {
        id,
        event,
        verse: 1 + rng.below(2) as u16,
        text: random_text(rng),
        syllabic: [
            Syllabic::Single,
            Syllabic::Begin,
            Syllabic::Middle,
            Syllabic::End,
        ][rng.below(4) as usize],
        extension: rng.chance(4),
    }
}

/// A spanner's kind and line, the wavy line and the pedal bracket among them.
fn random_spanner_look(rng: &mut Rng) -> (epiphany_core::SpannerKind, epiphany_core::SpanStyle) {
    use epiphany_core::{HairpinDirection, LineStyle, OctaveOffset, PedalKind, SpannerKind};
    let kind = match rng.below(8) {
        0 => SpannerKind::Generic,
        1 => SpannerKind::Hairpin(HairpinDirection::Crescendo),
        2 => SpannerKind::PedalLine(PedalKind::Sustain),
        3 => SpannerKind::PedalBracket(PedalKind::Sostenuto),
        4 => SpannerKind::TrillExtension,
        5 => SpannerKind::Glissando,
        6 => SpannerKind::OctaveLine(OctaveOffset(1)),
        _ => SpannerKind::TextLine(epiphany_core::TextLineDefinition {
            text: random_text(rng),
        }),
    };
    let line = [
        LineStyle::Solid,
        LineStyle::Dashed,
        LineStyle::Dotted,
        LineStyle::Wavy,
    ][rng.below(4) as usize];
    (
        kind,
        epiphany_core::SpanStyle {
            line,
            thickness: None,
        },
    )
}

/// An event value of a random kind in `voice`.
fn event_value(
    sim: &mut Simulation,
    r: usize,
    id: EventId,
    voice: VoiceId,
    at: MusicalPosition,
    length: MusicalDuration,
    quarter_tones: &mut Vec<(PitchId, PitchSpelling)>,
) -> Event {
    let position = EventPosition::Musical(at);
    let duration = EventDuration::Musical(length);
    match sim.rng.below(10) {
        0..=5 => {
            let count = 1 + sim.rng.below(3) as usize;
            let mut pitches = Vec::with_capacity(count);
            for _ in 0..count {
                let pid: PitchId = sim.mint(r);
                let pitch = random_pitch(&mut sim.rng, 5);
                if pitch.scale_position.space.as_str() == "cmn-24" {
                    if let Some(spelling) = spelling_for(&mut sim.rng, &pitch) {
                        quarter_tones.push((pid, spelling));
                    }
                }
                pitches.push(IdentifiedPitch { id: pid, pitch });
            }
            Event::Pitched(PitchedEvent {
                id,
                voice,
                position,
                duration,
                pitches,
                marks: if sim.rng.chance(3) {
                    random_marks(&mut sim.rng)
                } else {
                    Vec::new()
                },
                dynamic: None,
                ornaments: if sim.rng.chance(6) {
                    random_ornaments(&mut sim.rng)
                } else {
                    Vec::new()
                },
                stem: StemConfiguration,
                grace: None,
            })
        }
        6..=8 => Event::Rest(Rest {
            id,
            voice,
            position,
            duration,
            vertical_position: None,
            visible: !sim.rng.chance(5),
        }),
        _ => Event::Unpitched(UnpitchedEvent {
            id,
            voice,
            position,
            duration,
            staff_position: StaffPosition(sim.rng.range(-4, 4) as i16),
            instrument_member: UnpitchedMemberId(0),
            marks: if sim.rng.chance(3) {
                random_marks(&mut sim.rng)
            } else {
                Vec::new()
            },
            dynamic: None,
            stem: StemConfiguration,
            grace: None,
        }),
    }
}

/// An event of a kind without a pitch list other than a rest or an unpitched
/// note, at `id`'s place in `voice`: an indeterminate, graphic or cue event,
/// or, given two pitches or more, a trajectory between the first and the
/// last, which it holds as its own. None names another object: a cue's
/// sources, an indeterminate event's alternatives, a graphic event's objects
/// and a trajectory endpoint naming another event's pitch are not written.
fn other_kind(
    sim: &mut Simulation,
    id: EventId,
    voice: VoiceId,
    position: EventPosition,
    duration: EventDuration,
    pitches: &[IdentifiedPitch],
) -> Event {
    let kinds = if pitches.len() >= 2 { 4 } else { 3 };
    match sim.rng.below(kinds) {
        0 => Event::Indeterminate(IndeterminateEvent {
            id,
            voice,
            position,
            duration,
            indeterminacy: match sim.rng.below(3) {
                0 => IndeterminacyKind::Pitch,
                1 => IndeterminacyKind::Duration,
                _ => IndeterminacyKind::Choice,
            },
            hints: IndeterminacyHints::default(),
        }),
        1 => Event::Graphic(GraphicEvent {
            id,
            voice,
            position,
            duration,
            graphics: Vec::new(),
            playback_bindings: Vec::new(),
        }),
        2 => Event::Cue(CueEvent {
            id,
            voice,
            position,
            duration,
            source: Vec::new(),
            rendering: CueRendering,
        }),
        _ => Event::Trajectory(TrajectoryEvent {
            id,
            voice,
            position,
            duration,
            start: TrajectoryEndpoint::ExplicitPitch(pitches[0].clone()),
            end: TrajectoryEndpoint::ExplicitPitch(pitches[pitches.len() - 1].clone()),
            shape: if sim.rng.chance(2) {
                TrajectoryShape::Linear
            } else {
                TrajectoryShape::Exponential
            },
            display: TrajectoryDisplay,
        }),
    }
}

/// Builds the genesis: a small score of one or two instruments, each with one
/// or two staves, in one metric region of two to three 4/4 measures, every
/// voice holding a few notes, rests and chords; then a tuplet, slurs, a tie
/// and a beam among them.
fn genesis(sim: &mut Simulation) {
    let r = (GENESIS - 1) as usize;
    if sim.rng.chance(2) {
        sim.author(
            r,
            OperationPayload::Primitive(OperationKind::SetMetadata(SetMetadataOp {
                metadata: valuegen::score_metadata(0),
            })),
        );
    }
    let group: Option<StaffGroupId> = sim.rng.chance(2).then(|| sim.mint(r));
    if let Some(g) = group {
        sim.author(
            r,
            OperationPayload::Primitive(OperationKind::CreateStaffGroup(CreateStaffGroupOp {
                group: valuegen::staff_group(g, Vec::new()),
            })),
        );
    }
    let mut staves = Vec::new();
    for _ in 0..1 + sim.rng.below(2) {
        let instrument: InstrumentId = sim.mint(r);
        sim.author(
            r,
            OperationPayload::Primitive(OperationKind::CreateInstrument(CreateInstrumentOp {
                instrument: valuegen::instrument(instrument),
            })),
        );
        for _ in 0..1 + sim.rng.below(2) {
            let id: StaffId = sim.mint(r);
            let mut staff = valuegen::staff(id, instrument);
            staff.group = group.filter(|_| sim.rng.chance(2));
            sim.author(
                r,
                OperationPayload::Primitive(OperationKind::CreateStaff(CreateStaffOp { staff })),
            );
            staves.push(id);
        }
    }
    let region: RegionId = sim.mint(r);
    let mut value = valuegen::region(region);
    value.time_extent = epiphany_core::TimeExtent {
        start: epiphany_core::TimeAnchor::WallClock {
            time: WallClockTime(0),
        },
        end: epiphany_core::TimeAnchor::WallClock {
            time: WallClockTime(1),
        },
    };
    sim.author(
        r,
        OperationPayload::Primitive(OperationKind::CreateRegion(CreateRegionOp {
            region: value,
        })),
    );
    let signature: TimeSignatureId = sim.mint(r);
    sim.author(
        r,
        OperationPayload::Primitive(OperationKind::SetTimeSignature(SetTimeSignatureOp {
            region,
            anchor: valuegen::region_start_anchor(region, MusicalPosition::origin()),
            time_signature: Some(valuegen::time_signature(signature, 4)),
        })),
    );
    let bars = 2 + sim.rng.below(2) as i64;
    let mut voices = Vec::new();
    let mut instances = Vec::new();
    for staff in &staves {
        let id: StaffInstanceId = sim.mint(r);
        let mut instance = valuegen::staff_instance(id, *staff);
        if sim.rng.chance(2) {
            instance.clef_sequence = vec![epiphany_core::ClefChange {
                anchor: valuegen::region_start_anchor(region, MusicalPosition::origin()),
                clef: if sim.rng.chance(2) {
                    Clef::treble()
                } else {
                    Clef::bass()
                },
            }];
        }
        if sim.rng.chance(2) {
            instance.key_sequence = vec![epiphany_core::KeySignatureChange {
                anchor: valuegen::region_start_anchor(region, MusicalPosition::origin()),
                key: KeySignature::new(sim.rng.range(-3, 3) as i8).expect("a key in range"),
            }];
        }
        sim.author(
            r,
            OperationPayload::Primitive(OperationKind::CreateStaffInstance(
                CreateStaffInstanceOp { region, instance },
            )),
        );
        instances.push(id);
        for v in 0..1 + sim.rng.below(2) {
            let voice: VoiceId = sim.mint(r);
            let mut value = valuegen::voice(voice);
            value.is_primary = v == 0;
            sim.author(
                r,
                OperationPayload::Primitive(OperationKind::CreateVoice(CreateVoiceOp {
                    staff_instance: id,
                    voice: value,
                })),
            );
            voices.push((voice, id));
        }
    }
    for &instance in &instances {
        for bar in 0..bars {
            let id: MeasureId = sim.mint(r);
            sim.author(
                r,
                OperationPayload::Primitive(OperationKind::CreateMeasure(CreateMeasureOp {
                    instance,
                    measure: epiphany_core::Measure {
                        id,
                        start: valuegen::region_start_anchor(region, position(bar, 1)),
                        time_signature: (bar == 0).then_some(signature),
                        explicit_number: Some(bar as u32 + 1),
                        number_visibility: epiphany_core::MeasureNumberVisibility::Auto,
                    },
                })),
            );
        }
    }
    // Events: each voice fills runs of its bars from the start of a beat, in
    // eighths, quarters and halves, and one voice may take a triplet.
    let mut events: Vec<(EventId, VoiceId, MusicalPosition)> = Vec::new();
    let mut triplet: Option<Vec<EventId>> = None;
    for &(voice, instance) in &voices {
        let mut at = rational(sim.rng.below(2) as i64, 4);
        let end = rational(bars, 1);
        while at < end {
            if sim.rng.chance(6) {
                at = at.add(&rational(1, 4));
                continue;
            }
            if triplet.is_none() && sim.rng.chance(4) && at.add(&rational(1, 4)) <= end {
                let mut members = Vec::new();
                for k in 0..3 {
                    let id: EventId = sim.mint(r);
                    let pid: PitchId = sim.mint(r);
                    let event = Event::Pitched(PitchedEvent {
                        id,
                        voice,
                        position: EventPosition::Musical(MusicalPosition(at.add(&rational(k, 12)))),
                        duration: EventDuration::Musical(duration(1, 12)),
                        pitches: vec![IdentifiedPitch {
                            id: pid,
                            pitch: random_pitch(&mut sim.rng, 1000),
                        }],
                        marks: Vec::new(),
                        dynamic: None,
                        ornaments: Vec::new(),
                        stem: StemConfiguration,
                        grace: None,
                    });
                    sim.author(
                        r,
                        OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
                            staff_instance: instance,
                            event,
                        })),
                    );
                    events.push((id, voice, MusicalPosition(at.add(&rational(k, 12)))));
                    members.push(id);
                }
                triplet = Some(members);
                at = at.add(&rational(1, 4));
                continue;
            }
            let length = match sim.rng.below(4) {
                0 => duration(1, 8),
                1 | 2 => duration(1, 4),
                _ => duration(1, 2),
            };
            if at.add(&length.0) > end {
                break;
            }
            let id: EventId = sim.mint(r);
            let mut quarter_tones = Vec::new();
            let event = event_value(
                sim,
                r,
                id,
                voice,
                MusicalPosition(at.clone()),
                length.clone(),
                &mut quarter_tones,
            );
            sim.author(
                r,
                OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: instance,
                    event,
                })),
            );
            for (pitch, spelling) in quarter_tones {
                sim.author(
                    r,
                    OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                        pitch,
                        spelling,
                    })),
                );
            }
            events.push((id, voice, MusicalPosition(at.clone())));
            at = at.add(&length.0);
        }
    }
    if let Some(members) = triplet {
        let id: TupletId = sim.mint(r);
        sim.author(
            r,
            OperationPayload::Primitive(OperationKind::CreateTuplet(CreateTupletOp {
                tuplet: Tuplet {
                    id,
                    ratio: TupletRatio::new(3, 2).expect("not degenerate"),
                    members,
                    parent: None,
                    required_total: duration(1, 4),
                    display: TupletDisplay::default(),
                },
            })),
        );
    }
    // A slur, a tie and a beam between neighbours in one voice.
    for _ in 0..3 {
        if events.len() < 2 {
            break;
        }
        let i = sim.rng.below(events.len() as u64 - 1) as usize;
        let (a, va, _) = events[i].clone();
        let (b, vb, _) = events[i + 1].clone();
        if va != vb {
            continue;
        }
        let structure = if sim.rng.chance(2) {
            CrossCuttingValue::Slur(valuegen::slur(sim.mint(r), a, b))
        } else {
            CrossCuttingValue::Beam(valuegen::beam(sim.mint(r), vec![a, b]))
        };
        sim.author(
            r,
            OperationPayload::Primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                structure,
            })),
        );
    }
    // Every replica has seen the genesis.
    let all = sim.replicas[r].seen.clone();
    for replica in &mut sim.replicas {
        replica.seen.extend(all.iter().copied());
    }
}

/// One authoring turn's payloads, drawn for `kind` from replica `r`'s view;
/// `None` where the view holds nothing it applies to.
/// A staff-based region as an editor adds one: placed after every region the
/// view holds (two authors unaware of each other may still choose one place),
/// proportional one time in three.
fn placed_region(sim: &mut Simulation, r: usize, h: &Holdings<'_>) -> Region {
    let id: RegionId = sim.mint(r);
    let mut region: Region = valuegen::region(id);
    let after = h
        .score
        .canvas
        .regions
        .iter()
        .filter_map(|other| match other.time_extent.end {
            epiphany_core::TimeAnchor::WallClock { time } => Some(time.0),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    region.time_extent = epiphany_core::TimeExtent {
        start: epiphany_core::TimeAnchor::WallClock {
            time: WallClockTime(after),
        },
        end: epiphany_core::TimeAnchor::WallClock {
            time: WallClockTime(after + 1000),
        },
    };
    if sim.rng.chance(3) {
        region.time_model = valuegen::proportional_model();
    }
    region
}

fn make(
    sim: &mut Simulation,
    r: usize,
    kind: u64,
    h: &Holdings<'_>,
) -> Option<Vec<OperationPayload>> {
    let prim = |k: OperationKind| OperationPayload::Primitive(k);
    let rng_event = |sim: &mut Simulation| sim.rng.pick(&h.events).copied();
    Some(match kind {
        0 => {
            // InsertEvent: into a voice, mostly where its own events leave room.
            let &(voice, instance, _) = sim.rng.pick(&h.voices)?;
            let bars = h.measures_of(instance).max(1) as i64;
            let spans = h.spans(voice);
            let mut chosen = None;
            for _ in 0..8 {
                let at = position(sim.rng.below(8 * bars as u64) as i64, 8);
                let length = random_duration(&mut sim.rng);
                let end = at.clone() + length.clone();
                if !overlaps(&spans, &at, &end) || sim.rng.chance(6) {
                    chosen = Some((at, length));
                    break;
                }
            }
            let (at, length) = chosen?;
            let id: EventId = sim.mint(r);
            let mut quarter_tones = Vec::new();
            let event = if sim.rng.chance(6) {
                // An event of another kind, a trajectory between two pitches
                // the insert mints.
                let pitches: Vec<IdentifiedPitch> = (0..2)
                    .map(|_| IdentifiedPitch {
                        id: sim.mint(r),
                        pitch: random_pitch(&mut sim.rng, 1000),
                    })
                    .collect();
                other_kind(
                    sim,
                    id,
                    voice,
                    EventPosition::Musical(at),
                    EventDuration::Musical(length),
                    &pitches,
                )
            } else {
                event_value(sim, r, id, voice, at, length, &mut quarter_tones)
            };
            let mut out = vec![prim(OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event,
            }))];
            out.extend(quarter_tones.into_iter().map(|(pitch, spelling)| {
                prim(OperationKind::RespellPitch(RespellPitchOp {
                    pitch,
                    spelling,
                }))
            }));
            out
        }
        1 => {
            // DeleteEvent, with the compensation its tuplets ask.
            let event = rng_event(sim)?;
            let tuplets: Vec<TupletId> = h
                .score
                .cross_cutting
                .tuplets
                .iter()
                .filter(|t| t.members.contains(&event.id()))
                .map(|t| t.id)
                .collect();
            let compensation = if tuplets.is_empty() {
                TupletCompensation::NotInTuplet
            } else {
                match sim.rng.below(3) {
                    0 => match (event.position(), event.duration()) {
                        (EventPosition::Musical(p), d) => TupletCompensation::ReplaceWithRest {
                            rest: Rest {
                                id: sim.mint(r),
                                voice: event.voice(),
                                position: EventPosition::Musical(p.clone()),
                                duration: d.clone(),
                                vertical_position: None,
                                visible: true,
                            },
                        },
                        _ => TupletCompensation::CascadeDeleteTuplets { tuplets },
                    },
                    1 => TupletCompensation::CascadeDeleteTuplets { tuplets },
                    _ => TupletCompensation::RewriteTuplets { tuplets },
                }
            };
            vec![prim(OperationKind::DeleteEvent(DeleteEventOp {
                event: event.id(),
                tuplet_compensation: compensation,
            }))]
        }
        2 => {
            // RespellPitch: its own spelling or an enharmonic.
            let &(_, ip) = sim.rng.pick(&h.pitches)?;
            let spelling = spelling_for(&mut sim.rng, &ip.pitch)?;
            vec![prim(OperationKind::RespellPitch(RespellPitchOp {
                pitch: ip.id,
                spelling,
            }))]
        }
        3 => {
            // CreateCrossCutting: a slur, tie, beam or spanner over one voice's
            // events in time order.
            let &(voice, _, _) = sim.rng.pick(&h.voices)?;
            let mut own: Vec<&Event> = h
                .events
                .iter()
                .copied()
                .filter(|e| e.voice() == voice)
                .collect();
            own.sort_by(|a, b| span(a).map(|s| s.0).cmp(&span(b).map(|s| s.0)));
            if own.len() < 2 {
                return None;
            }
            let i = sim.rng.below(own.len() as u64 - 1) as usize;
            let j = i + 1 + sim.rng.below((own.len() - i - 1).min(3) as u64) as usize;
            let (a, b) = (own[i].id(), own[j].id());
            let structure = match sim.rng.below(4) {
                0 => CrossCuttingValue::Slur(valuegen::slur(sim.mint(r), a, b)),
                1 => return tie_entry(sim, r, h, own[i]).map(|(payloads, _)| payloads),
                2 => CrossCuttingValue::Beam(valuegen::beam(
                    sim.mint(r),
                    own[i..=j].iter().map(|e| e.id()).collect(),
                )),
                _ => {
                    let staff = h
                        .instances()
                        .iter()
                        .find(|(_, inst)| Some(inst.id) == h.instance_of_voice.get(&voice).copied())
                        .map(|(_, inst)| inst.staff)?;
                    let (kind, style) = random_spanner_look(&mut sim.rng);
                    CrossCuttingValue::Spanner(epiphany_core::Spanner {
                        id: sim.mint(r),
                        start: valuegen::event_anchor(a),
                        end: valuegen::event_anchor(b),
                        staves: vec![staff],
                        kind,
                        style,
                    })
                }
            };
            vec![prim(OperationKind::CreateCrossCutting(
                CreateCrossCuttingOp { structure },
            ))]
        }
        4 => {
            // ChangeRegionTimeModel: to another model, or metric again with a
            // remapping of some of the region's events; half the time in a
            // region that holds none, which any target admits.
            let quiet: Vec<_> = h
                .regions
                .iter()
                .filter(|(_, _, is)| {
                    is.iter()
                        .all(|i| i.voices.iter().all(|v| h.empty_voice(v.id)))
                })
                .cloned()
                .collect();
            let (region, _, instances) = match sim.rng.pick(&quiet) {
                Some(q) if sim.rng.chance(2) => q.clone(),
                _ => sim.rng.pick(&h.regions)?.clone(),
            };
            let (region, instances) = (&region, &instances);
            let in_region: Vec<&Event> = h
                .events
                .iter()
                .copied()
                .filter(|e| {
                    instances
                        .iter()
                        .any(|i| i.voices.iter().any(|v| v.id == e.voice()))
                })
                .collect();
            let (model, remapping) = match sim.rng.below(4) {
                0 => (
                    valuegen::proportional_model(),
                    PositionRemapping::PreserveTime,
                ),
                1 => (
                    RegionTimeModel::Aleatoric(AleatoricTimeModel {
                        ordering: EventOrderingDAG::default(),
                        anchoring: match sim.rng.below(4) {
                            0 => AleatoricAnchoringDiscipline::Musical,
                            1 => AleatoricAnchoringDiscipline::WallClock,
                            2 => AleatoricAnchoringDiscipline::EitherPerEvent,
                            _ => AleatoricAnchoringDiscipline::FreelyMixed,
                        },
                        bounds: BTreeMap::new(),
                        duration_hint: WallClockDuration(1),
                    }),
                    PositionRemapping::PreserveTime,
                ),
                2 => (valuegen::metric_model(), PositionRemapping::PreserveTime),
                _ => {
                    let mut pairs: Vec<(EventId, MusicalPosition)> = Vec::new();
                    for e in &in_region {
                        if sim.rng.chance(2) {
                            pairs.push((e.id(), position(sim.rng.below(16) as i64, 8)));
                        }
                    }
                    pairs.sort_by_key(|(e, _)| *e);
                    pairs.dedup_by_key(|(e, _)| *e);
                    (valuegen::metric_model(), PositionRemapping::Reassign(pairs))
                }
            };
            let declared_incompatible = in_region
                .iter()
                .filter(|_| sim.rng.chance(4))
                .map(|e| e.id())
                .collect();
            vec![prim(OperationKind::ChangeRegionTimeModel(
                ChangeRegionTimeModelOp {
                    region: *region,
                    new_time_model: model,
                    declared_incompatible,
                    remapping,
                },
            ))]
        }
        5 | 23 => {
            // A user break at a measure's start, as the importer and the
            // editor write one, at an event, or at an offset from its
            // region's start, as version 4's writers wrote one; one time in
            // eight in another region the view holds, which the break's
            // validity refuses `TargetMissing` (reduction version 5), or
            // `WrongRegionTimeModel` where that region is out of musical time
            // and the anchor in it.
            let metric = h.metric();
            let (region, _, instances) = sim.rng.pick(&metric)?;
            let bars = instances.first().map_or(1, |i| i.measures.len().max(1)) as i64;
            let at = position(sim.rng.below(bars as u64) as i64, 1);
            let anchor = match sim.rng.below(3) {
                0 => {
                    let measures: Vec<MeasureId> = instances
                        .iter()
                        .flat_map(|i| i.measures.iter().map(|m| m.id))
                        .collect();
                    epiphany_core::TimeAnchor::Measure {
                        id: *sim.rng.pick(&measures)?,
                        position: epiphany_core::MeasurePosition::Start,
                        offset: epiphany_core::AnchorOffset::Zero,
                    }
                }
                1 => {
                    let events: Vec<EventId> = h
                        .events
                        .iter()
                        .filter(|e| {
                            instances
                                .iter()
                                .any(|i| i.voices.iter().any(|v| v.id == e.voice()))
                        })
                        .map(|e| e.id())
                        .collect();
                    epiphany_core::TimeAnchor::Event {
                        id: *sim.rng.pick(&events)?,
                        offset: epiphany_core::AnchorOffset::Zero,
                    }
                }
                _ => valuegen::region_start_anchor(*region, at),
            };
            let others: Vec<RegionId> = h
                .regions
                .iter()
                .map(|(other, _, _)| *other)
                .filter(|other| other != region)
                .collect();
            let region = match sim.rng.pick(&others) {
                Some(other) if sim.rng.chance(8) => other,
                _ => region,
            };
            let present = !sim.rng.chance(3);
            if kind == 5 {
                vec![prim(OperationKind::SetUserSystemBreak(
                    SetUserSystemBreakOp {
                        region: *region,
                        anchor,
                        present,
                    },
                ))]
            } else {
                vec![prim(OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                    region: *region,
                    anchor,
                    present,
                }))]
            }
        }
        6 => {
            // DeclareTransaction: opens one that this replica's next one to
            // three operations join.
            let tx: TransactionId = sim.mint(r);
            let left = 1 + sim.rng.below(3);
            sim.replicas[r].open = Some((tx, left + 1));
            vec![prim(OperationKind::DeclareTransaction(
                TransactionDescriptor {
                    id: tx,
                    label: String::from("edit"),
                    category: None,
                },
            ))]
        }
        7 => vec![prim(OperationKind::Registered(
            OperationKindRegistryId(u128::from(sim.rng.below(3) as u8)),
            vec![sim.rng.below(256) as u8],
        ))],
        8 => {
            // ModifyEvent: the same event moved, lengthened, shortened or
            // given other pitches under the same ids.
            let event = rng_event(sim)?;
            let mut value = event.clone();
            match sim.rng.below(3) {
                0 => {
                    if let Some((start, _)) = span(event) {
                        let step = if sim.rng.chance(2) {
                            rational(1, 8)
                        } else {
                            rational(-1, 8)
                        };
                        let moved = start.0.add(&step);
                        if moved < RationalTime::zero() {
                            return None;
                        }
                        set_position(&mut value, MusicalPosition(moved));
                    }
                }
                1 => set_duration(&mut value, random_duration(&mut sim.rng)),
                _ => match &mut value {
                    Event::Pitched(p) => {
                        for ip in &mut p.pitches {
                            if sim.rng.chance(2) {
                                ip.pitch = random_pitch(&mut sim.rng, 6);
                            }
                        }
                    }
                    _ => set_duration(&mut value, random_duration(&mut sim.rng)),
                },
            }
            vec![prim(OperationKind::ModifyEvent(ModifyEventOp {
                event: value,
            }))]
        }
        9 | 30 => {
            let count = 1 + sim.rng.below(2) as usize;
            let mut targets: Vec<PitchId> = (0..count)
                .filter_map(|_| sim.rng.pick(&h.pitches).map(|(_, ip)| ip.id))
                .collect();
            targets.sort();
            targets.dedup();
            if targets.is_empty() {
                return None;
            }
            if kind == 9 {
                vec![prim(OperationKind::Transpose(TransposeOp {
                    targets,
                    chromatic_steps: sim.rng.range(-3, 3) as i32,
                }))]
            } else {
                let diatonic = sim.rng.range(-2, 2) as i32;
                let chromatic = [0, 2, 4, 5, 7][diatonic.unsigned_abs() as usize]
                    * diatonic.signum()
                    + sim.rng.range(-1, 1) as i32;
                vec![prim(OperationKind::TransposeInterval(
                    TransposeIntervalOp {
                        targets: targets.into_iter().collect(),
                        interval: TranspositionInterval {
                            diatonic_steps: diatonic,
                            chromatic_steps: chromatic,
                        },
                    },
                ))]
            }
        }
        10 => {
            // InsertIdentifiedPitch: into an event of any kind, mostly a
            // note, and spelt half the time, as an editor enters a pitch.
            let notes = h
                .events
                .iter()
                .copied()
                .filter(|e| matches!(e, Event::Pitched(_)))
                .collect::<Vec<_>>();
            let event = match sim.rng.pick(&notes) {
                Some(note) if !sim.rng.chance(3) => *note,
                _ => rng_event(sim)?,
            };
            let id: PitchId = sim.mint(r);
            let pitch = random_pitch(&mut sim.rng, 6);
            let spelling = sim
                .rng
                .chance(2)
                .then(|| spelling_for(&mut sim.rng, &pitch))
                .flatten();
            let mut out = vec![prim(OperationKind::InsertIdentifiedPitch(
                InsertIdentifiedPitchOp {
                    event: event.id(),
                    pitch: IdentifiedPitch { id, pitch },
                },
            ))];
            out.extend(spelling.map(|spelling| {
                prim(OperationKind::RespellPitch(RespellPitchOp {
                    pitch: id,
                    spelling,
                }))
            }));
            out
        }
        11 => {
            let &(_, ip) = sim.rng.pick(&h.pitches)?;
            vec![prim(OperationKind::DeleteIdentifiedPitch(
                DeleteIdentifiedPitchOp { pitch: ip.id },
            ))]
        }
        12 => {
            let &(_, ip) = sim.rng.pick(&h.pitches)?;
            vec![prim(OperationKind::ModifyIdentifiedPitch(
                ModifyIdentifiedPitchOp {
                    pitch: ip.id,
                    value: random_pitch(&mut sim.rng, 5),
                },
            ))]
        }
        13 | 14 => {
            // DeleteCrossCutting or ModifyCrossCutting over a live structure.
            let cc = &h.score.cross_cutting;
            let mut live: Vec<CrossCuttingValue> = Vec::new();
            live.extend(cc.slurs.iter().cloned().map(CrossCuttingValue::Slur));
            live.extend(cc.ties.iter().cloned().map(CrossCuttingValue::Tie));
            live.extend(cc.beams.iter().cloned().map(CrossCuttingValue::Beam));
            live.extend(cc.spanners.iter().cloned().map(CrossCuttingValue::Spanner));
            live.extend(cc.markers.iter().cloned().map(CrossCuttingValue::Marker));
            live.extend(cc.lyrics.iter().cloned().map(CrossCuttingValue::Lyric));
            let chosen = sim.rng.pick(&live)?.clone();
            if kind == 13 {
                let structure = chosen.id();
                vec![prim(OperationKind::DeleteCrossCutting(
                    DeleteCrossCuttingOp { structure },
                ))]
            } else {
                let other = rng_event(sim)?.id();
                let structure = match chosen {
                    CrossCuttingValue::Slur(mut s) => {
                        let start = h.events.iter().find(|e| e.id() == s.start_event);
                        let later: Vec<EventId> = h
                            .events
                            .iter()
                            .filter(|e| {
                                start.is_some_and(|st| {
                                    e.voice() == st.voice()
                                        && span(e).map(|x| x.0) > span(st).map(|x| x.0)
                                })
                            })
                            .map(|e| e.id())
                            .collect();
                        if let (Some(end), true) = (sim.rng.pick(&later), sim.rng.chance(2)) {
                            s.end_event = *end;
                        } else {
                            s.kind = Default::default();
                            s.curvature_override = Some(epiphany_core::CurvatureOverride {
                                direction: Some(epiphany_core::CurveDirection::Below),
                                height: None,
                            });
                        }
                        CrossCuttingValue::Slur(s)
                    }
                    CrossCuttingValue::Tie(mut t) => {
                        t.class = epiphany_core::TieClass::LaissezVibrer;
                        CrossCuttingValue::Tie(t)
                    }
                    CrossCuttingValue::Beam(mut b) => {
                        if !b.events.contains(&other) {
                            b.events.push(other);
                        }
                        CrossCuttingValue::Beam(b)
                    }
                    CrossCuttingValue::Spanner(mut s) => {
                        if sim.rng.chance(2) {
                            s.end = valuegen::event_anchor(other);
                        } else {
                            // Its staves rewritten: one or two the view holds.
                            let mut staves: Vec<StaffId> = (0..1 + sim.rng.below(2))
                                .filter_map(|_| sim.rng.pick(&h.score.staves).map(|s| s.id))
                                .collect();
                            staves.sort();
                            staves.dedup();
                            if staves.is_empty() {
                                return None;
                            }
                            s.staves = staves;
                        }
                        CrossCuttingValue::Spanner(s)
                    }
                    // A point mark of another kind, or moved to another event.
                    CrossCuttingValue::Marker(mut m) => {
                        if sim.rng.chance(2) {
                            m.kind = random_marker_kind(&mut sim.rng);
                        } else {
                            m.anchor = valuegen::event_anchor(other);
                        }
                        CrossCuttingValue::Marker(m)
                    }
                    // A syllable in another verse, of another text, or moved
                    // to another event, where one may stand already.
                    CrossCuttingValue::Lyric(mut l) => {
                        match sim.rng.below(3) {
                            0 => l.verse = 1 + sim.rng.below(2) as u16,
                            1 => l.text = random_text(&mut sim.rng),
                            _ => l.event = other,
                        }
                        CrossCuttingValue::Lyric(l)
                    }
                };
                vec![prim(OperationKind::ModifyCrossCutting(
                    ModifyCrossCuttingOp { structure },
                ))]
            }
        }
        15 => {
            // CreateRegion: one an editor places ([`placed_region`]), or one
            // in two times one no editor writes (reduction version 5): over
            // the span of a region the view holds, with an instance of a staff
            // that region holds, the fill refused `RegionsOverlap`; or
            // starting, with no offset, at a region's end, which the ledger
            // places through its index of extents, or at an event, which it
            // cannot place, so that a fill of a staff another live region
            // holds is refused; or with an offset of either kind from a
            // region's end, from its own start, or from an event or a
            // measure, refused `WrongRegionTimeModel` where its target's time
            // model does not admit the offset (X5b.8).
            let mut region = placed_region(sim, r, h);
            let mut fill = None;
            match sim.rng.below(12) {
                0 => {
                    let (other, _, instances) = sim.rng.pick(&h.regions)?;
                    region.time_extent = h
                        .score
                        .canvas
                        .regions
                        .iter()
                        .find(|held| held.id == *other)?
                        .time_extent
                        .clone();
                    fill = sim.rng.pick(instances).map(|instance| instance.staff);
                }
                1 => {
                    let other = sim.rng.pick(&h.score.canvas.regions)?.id;
                    region.time_extent.start = epiphany_core::TimeAnchor::Region {
                        id: other,
                        edge: epiphany_core::RegionEdge::End,
                        offset: epiphany_core::AnchorOffset::Zero,
                    };
                }
                2 => {
                    let event = rng_event(sim)?.id();
                    region.time_extent.start = epiphany_core::TimeAnchor::Event {
                        id: event,
                        offset: epiphany_core::AnchorOffset::Zero,
                    };
                }
                3 => {
                    let other = sim.rng.pick(&h.score.canvas.regions)?.id;
                    let offset = random_offset(&mut sim.rng);
                    let at = |offset| epiphany_core::TimeAnchor::Region {
                        id: other,
                        edge: epiphany_core::RegionEdge::End,
                        offset,
                    };
                    region.time_extent.start = at(epiphany_core::AnchorOffset::Zero);
                    region.time_extent.end = at(offset);
                }
                4 => {
                    region.time_extent.end = epiphany_core::TimeAnchor::Region {
                        id: region.id,
                        edge: epiphany_core::RegionEdge::Start,
                        offset: random_offset(&mut sim.rng),
                    };
                }
                5 => {
                    let offset = random_offset(&mut sim.rng);
                    region.time_extent.start = if sim.rng.chance(2) {
                        epiphany_core::TimeAnchor::Event {
                            id: rng_event(sim)?.id(),
                            offset,
                        }
                    } else {
                        let measures: Vec<MeasureId> = h
                            .instances()
                            .iter()
                            .flat_map(|(_, instance)| instance.measures.iter().map(|m| m.id))
                            .collect();
                        epiphany_core::TimeAnchor::Measure {
                            id: *sim.rng.pick(&measures)?,
                            position: epiphany_core::MeasurePosition::Start,
                            offset,
                        }
                    };
                }
                _ => {}
            }
            let id = region.id;
            let mut out = vec![prim(OperationKind::CreateRegion(CreateRegionOp { region }))];
            if let Some(staff) = fill {
                let instance: StaffInstanceId = sim.mint(r);
                out.push(prim(OperationKind::CreateStaffInstance(
                    CreateStaffInstanceOp {
                        region: id,
                        instance: valuegen::staff_instance(instance, staff),
                    },
                )));
            }
            out
        }
        16 => {
            let empty: Vec<RegionId> = h
                .regions
                .iter()
                .filter(|(_, _, is)| is.is_empty())
                .map(|(r, _, _)| *r)
                .collect();
            let region = match sim.rng.pick(&empty) {
                Some(r) if sim.rng.chance(2) => *r,
                None if sim.rng.chance(2) => {
                    // A region added and taken away again.
                    let mut out = vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                        region: placed_region(sim, r, h),
                    }))];
                    let Some(OperationPayload::Primitive(OperationKind::CreateRegion(op))) =
                        out.first()
                    else {
                        return None;
                    };
                    let region = op.region.id;
                    out.push(prim(OperationKind::DeleteRegion(DeleteRegionOp { region })));
                    return Some(out);
                }
                _ => sim.rng.pick(&h.regions)?.0,
            };
            vec![prim(OperationKind::DeleteRegion(DeleteRegionOp { region }))]
        }
        17 => {
            // A staff a staff-based region does not yet manifest.
            let open: Vec<(RegionId, StaffId)> = h
                .regions
                .iter()
                .flat_map(|(region, _, instances)| {
                    h.score
                        .staves
                        .iter()
                        .map(|s| s.id)
                        .filter(|s| !instances.iter().any(|i| i.staff == *s))
                        .map(move |s| (*region, s))
                })
                .collect();
            let &(region, staff) = sim.rng.pick(&open)?;
            let region = &region;
            let id: StaffInstanceId = sim.mint(r);
            vec![prim(OperationKind::CreateStaffInstance(
                CreateStaffInstanceOp {
                    region: *region,
                    instance: valuegen::staff_instance(id, staff),
                },
            ))]
        }
        18 => {
            let instances = h.instances();
            let empty: Vec<StaffInstanceId> = instances
                .iter()
                .filter(|(_, i)| i.voices.is_empty())
                .map(|(_, i)| i.id)
                .collect();
            let staff_instance = match sim.rng.pick(&empty) {
                Some(i) if !sim.rng.chance(4) => *i,
                None if sim.rng.chance(2) => {
                    // A staff added and taken away again.
                    let mut out = make(sim, r, 17, h)?;
                    let Some(OperationPayload::Primitive(OperationKind::CreateStaffInstance(op))) =
                        out.first()
                    else {
                        return None;
                    };
                    let staff_instance = op.instance.id;
                    out.push(prim(OperationKind::DeleteStaffInstance(
                        DeleteStaffInstanceOp { staff_instance },
                    )));
                    return Some(out);
                }
                _ => sim.rng.pick(&instances)?.1.id,
            };
            vec![prim(OperationKind::DeleteStaffInstance(
                DeleteStaffInstanceOp { staff_instance },
            ))]
        }
        19 => {
            let instances = h.instances();
            let (_, instance) = sim.rng.pick(&instances)?;
            let id: VoiceId = sim.mint(r);
            let mut voice: Voice = valuegen::voice(id);
            voice.default_stem_direction = match sim.rng.below(3) {
                0 => Some(StemDirection::Up),
                1 => Some(StemDirection::Down),
                _ => None,
            };
            vec![prim(OperationKind::CreateVoice(CreateVoiceOp {
                staff_instance: instance.id,
                voice,
            }))]
        }
        20 => {
            let all: Vec<VoiceId> = h.instance_of_voice.keys().copied().collect();
            let empty: Vec<VoiceId> = all.iter().copied().filter(|v| h.empty_voice(*v)).collect();
            let voice = match sim.rng.pick(&empty) {
                Some(v) if sim.rng.chance(2) => *v,
                None if sim.rng.chance(2) => {
                    // A voice added and taken away again.
                    let mut out = make(sim, r, 19, h)?;
                    let Some(OperationPayload::Primitive(OperationKind::CreateVoice(op))) =
                        out.first()
                    else {
                        return None;
                    };
                    let voice = op.voice.id;
                    out.push(prim(OperationKind::DeleteVoice(DeleteVoiceOp { voice })));
                    return Some(out);
                }
                _ => *sim.rng.pick(&all)?,
            };
            vec![prim(OperationKind::DeleteVoice(DeleteVoiceOp { voice }))]
        }
        21 => vec![prim(OperationKind::SetMetadata(SetMetadataOp {
            metadata: valuegen::score_metadata(sim.rng.below(3) as u8),
        }))],
        22 => {
            let metric = h.metric();
            let (region, _, _) = sim.rng.pick(&metric)?;
            vec![prim(OperationKind::SetMetricGrid(SetMetricGridOp {
                region: *region,
                grid: sim.rng.chance(2).then(valuegen::metric_grid),
            }))]
        }
        24 => {
            let instrument = sim.rng.pick(&h.score.instruments)?.id;
            let id: StaffId = sim.mint(r);
            let mut staff = valuegen::staff(id, instrument);
            if sim.rng.chance(3) {
                staff.group = sim.rng.pick(&h.score.staff_groups).map(|g| g.id);
            }
            vec![prim(OperationKind::CreateStaff(CreateStaffOp { staff }))]
        }
        25 => {
            // SetTimeSignature at a bar of a region: set or removed.
            let metric = h.metric();
            let (region, _, instances) = sim.rng.pick(&metric)?;
            let bars = instances.first().map_or(1, |i| i.measures.len().max(1)) as i64;
            let at = sim.rng.below(bars as u64) as i64;
            let signature = (!sim.rng.chance(4)).then(|| {
                let id: TimeSignatureId = sim.mint(r);
                valuegen::time_signature(id, [2u16, 3, 4][sim.rng.below(3) as usize])
            });
            vec![prim(OperationKind::SetTimeSignature(SetTimeSignatureOp {
                region: *region,
                anchor: valuegen::region_start_anchor(*region, position(at, 1)),
                time_signature: signature,
            }))]
        }
        26 => {
            let metric = h.metric();
            let (region, _, _) = sim.rng.pick(&metric)?;
            let at = sim.rng.below(2) as i64;
            let scoped = sim.rng.chance(2);
            let mut segment = (!sim.rng.chance(4)).then(|| {
                valuegen::tempo_segment(
                    *region,
                    position(at, 1),
                    60.0 + 30.0 * sim.rng.below(3) as f64,
                )
            });
            // One time in eight a constant segment records its end tempo, as
            // version 4 applied one: no decoder reads it now (P13-S8), so only
            // an operation built in memory carries it, refused
            // `TempoMapMalformed` (reduction version 5). A history holding it
            // renders but does not parse.
            if let Some(segment) = segment.as_mut().filter(|_| sim.rng.chance(8)) {
                segment.end_tempo = Some(segment.start_tempo.clone());
            }
            vec![prim(OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: scoped.then_some(*region),
                start: valuegen::region_start_anchor(*region, position(at, 1)),
                segment,
            }))]
        }
        27 => {
            let instances = h.instances();
            let (_, instance) = sim.rng.pick(&instances)?;
            vec![prim(OperationKind::SetStaffLayout(SetStaffLayoutOp {
                staff_instance: instance.id,
                instrument_override: sim
                    .rng
                    .chance(3)
                    .then(|| sim.rng.pick(&h.score.instruments).map(|i| i.id))
                    .flatten(),
                staff_lines_override: sim
                    .rng
                    .chance(2)
                    .then(epiphany_core::StaffLineConfiguration::default),
                visible: !sim.rng.chance(4),
            }))]
        }
        28 => {
            let a = rng_event(sim)?.id();
            let b = rng_event(sim)?.id();
            let id: RepeatStructureId = sim.mint(r);
            let repeat = if sim.rng.chance(2) {
                valuegen::repeat_structure(id, a, b)
            } else {
                valuegen::volta_repeat(id, a, b)
            };
            vec![prim(OperationKind::CreateRepeatStructure(
                CreateRepeatStructureOp { repeat },
            ))]
        }
        29 => {
            let repeat = sim.rng.pick(&h.score.cross_cutting.repeats)?.id;
            vec![prim(OperationKind::DeleteRepeatStructure(
                DeleteRepeatStructureOp { repeat },
            ))]
        }
        31 => {
            let id: InstrumentId = sim.mint(r);
            vec![prim(OperationKind::CreateInstrument(CreateInstrumentOp {
                instrument: valuegen::instrument(id),
            }))]
        }
        32 => vec![prim(OperationKind::SetCanvasLayoutDefaults(
            SetCanvasLayoutDefaultsOp {
                layout_defaults: valuegen::canvas_layout_defaults(sim.rng.below(3) as u8),
            },
        ))],
        33 => vec![prim(OperationKind::SetSpellingPrecedence(
            SetSpellingPrecedenceOp {
                precedence: valuegen::spelling_precedence(sim.rng.below(2) as u8),
            },
        ))],
        34 => vec![prim(OperationKind::SetTuningContext(SetTuningContextOp {
            settings: valuegen::tuning_context_settings(sim.rng.below(3) as u8),
        }))],
        35 => {
            let id: StaffGroupId = sim.mint(r);
            vec![prim(OperationKind::CreateStaffGroup(CreateStaffGroupOp {
                group: valuegen::staff_group(id, Vec::new()),
            }))]
        }
        36 => {
            let mut staves: Vec<StaffId> = h
                .score
                .staves
                .iter()
                .filter(|_| sim.rng.chance(2))
                .map(|s| s.id)
                .collect();
            staves.sort();
            let id: PartDefinitionId = sim.mint(r);
            vec![prim(OperationKind::CreatePartDefinition(
                CreatePartDefinitionOp {
                    part: valuegen::part_definition(id, staves),
                },
            ))]
        }
        37 => {
            let id: AnalysisLayerId = sim.mint(r);
            vec![prim(OperationKind::CreateAnalysisLayer(
                CreateAnalysisLayerOp {
                    layer: valuegen::analysis_layer(id),
                },
            ))]
        }
        38 => {
            let mut layers: Vec<AnalysisLayerId> = h
                .score
                .analysis_layers
                .iter()
                .filter(|_| sim.rng.chance(2))
                .map(|l| l.id)
                .collect();
            layers.sort();
            let id: ViewId = sim.mint(r);
            vec![prim(OperationKind::CreateView(CreateViewOp {
                view: valuegen::view(id, layers),
            }))]
        }
        39 => {
            // CreateMeasure: the next bar of an instance.
            let instances = h.metric_instances();
            let (region, instance) = *sim.rng.pick(&instances)?;
            let next = instance.measures.len() as i64;
            let id: MeasureId = sim.mint(r);
            vec![prim(OperationKind::CreateMeasure(CreateMeasureOp {
                instance: instance.id,
                measure: epiphany_core::Measure {
                    id,
                    start: valuegen::region_start_anchor(region, position(next, 1)),
                    time_signature: None,
                    explicit_number: Some(next as u32 + 1),
                    number_visibility: epiphany_core::MeasureNumberVisibility::Auto,
                },
            }))]
        }
        40 => {
            // CreateTuplet over two or three consecutive events of a voice,
            // their total required as they stand; one time in four over two
            // consecutive members of a tuplet the view holds, nested in it
            // (D87 Q3).
            if sim.rng.chance(4) {
                let parents: Vec<&Tuplet> = h
                    .score
                    .cross_cutting
                    .tuplets
                    .iter()
                    .filter(|tuplet| tuplet.members.len() >= 2)
                    .collect();
                if let Some(parent) = sim.rng.pick(&parents) {
                    let i = sim.rng.below(parent.members.len() as u64 - 1) as usize;
                    let members = parent.members[i..i + 2].to_vec();
                    let total = members
                        .iter()
                        .filter_map(|id| h.events.iter().find(|e| e.id() == *id))
                        .filter_map(|e| match e.duration() {
                            EventDuration::Musical(d) => Some(d.clone()),
                            _ => None,
                        })
                        .fold(MusicalDuration::zero(), |a, b| a + b);
                    let id: TupletId = sim.mint(r);
                    return Some(vec![prim(OperationKind::CreateTuplet(CreateTupletOp {
                        tuplet: Tuplet {
                            id,
                            ratio: TupletRatio::new(2, 3).expect("not degenerate"),
                            members,
                            parent: Some(parent.id),
                            required_total: total,
                            display: TupletDisplay::default(),
                        },
                    }))]);
                }
            }
            let &(voice, _, _) = sim.rng.pick(&h.voices)?;
            let mut own: Vec<&Event> = h
                .events
                .iter()
                .copied()
                .filter(|e| e.voice() == voice && span(e).is_some())
                .collect();
            own.sort_by(|a, b| span(a).map(|s| s.0).cmp(&span(b).map(|s| s.0)));
            let n = 2 + sim.rng.below(2) as usize;
            if own.len() < n {
                return None;
            }
            let i = sim.rng.below((own.len() - n + 1) as u64) as usize;
            let members: Vec<EventId> = own[i..i + n].iter().map(|e| e.id()).collect();
            let total = own[i..i + n]
                .iter()
                .filter_map(|e| match e.duration() {
                    EventDuration::Musical(d) => Some(d.clone()),
                    _ => None,
                })
                .fold(MusicalDuration::zero(), |a, b| a + b);
            let id: TupletId = sim.mint(r);
            let ratio = if n == 3 { (3, 2) } else { (2, 3) };
            vec![prim(OperationKind::CreateTuplet(CreateTupletOp {
                tuplet: Tuplet {
                    id,
                    ratio: TupletRatio::new(ratio.0, ratio.1).expect("not degenerate"),
                    members,
                    parent: None,
                    required_total: total,
                    display: if sim.rng.chance(3) {
                        TupletDisplay::HIDDEN
                    } else {
                        TupletDisplay::default()
                    },
                },
            }))]
        }
        41 | 42 => {
            let instances = h.metric_instances();
            let (_, instance) = sim.rng.pick(&instances)?;
            let bars = instance.measures.len().max(1) as i64;
            let offset = position(sim.rng.below(2 * bars as u64) as i64, 2).0;
            if kind == 41 {
                vec![prim(OperationKind::SetClef(SetClefOp {
                    instance: instance.id,
                    offset,
                    clef: (!sim.rng.chance(3)).then(|| match sim.rng.below(3) {
                        0 => Clef::treble(),
                        1 => Clef::bass(),
                        _ => Clef::alto(),
                    }),
                }))]
            } else {
                vec![prim(OperationKind::SetKeySignature(SetKeySignatureOp {
                    instance: instance.id,
                    offset,
                    key: (!sim.rng.chance(3)).then(|| {
                        KeySignature::new(sim.rng.range(-4, 4) as i8).expect("a key in range")
                    }),
                }))]
            }
        }
        43 | 45 => {
            // UndoTransaction of a transaction the view declares.
            let target = undo_target(sim, r, h)?;
            let policy = match sim.rng.below(3) {
                0 => UndoPolicy::StrictInverse,
                1 => UndoPolicy::BestEffort,
                _ => UndoPolicy::Cascade,
            };
            vec![OperationPayload::UndoTransaction(UndoTransactionPayload {
                target,
                policy,
            })]
        }
        44 | 46 => {
            // ResolveConflict of a conflict the view records.
            let records = h.state.conflicts.records();
            let record = sim.rng.pick(records)?;
            let action = match sim.rng.below(4) {
                0 => ResolutionAction::KeepWinner,
                1 => ResolutionAction::AcceptLoser,
                2 => ResolutionAction::Dismiss,
                _ => match record.caused_by.first() {
                    Some(op) => ResolutionAction::Override {
                        override_operation: *op,
                    },
                    None => ResolutionAction::Dismiss,
                },
            };
            vec![OperationPayload::ResolveConflict(ResolveConflictPayload {
                target: record.id,
                action,
            })]
        }
        47 => {
            // Replace a note with a rest: the event deleted and a rest of its
            // span entered in its voice, the editor's gesture.
            let event = rng_event(sim)?;
            let (start, _) = span(event)?;
            let instance = *h.instance_of_voice.get(&event.voice())?;
            if h.score
                .cross_cutting
                .tuplets
                .iter()
                .any(|t| t.members.contains(&event.id()))
            {
                return None;
            }
            let rest = Event::Rest(Rest {
                id: sim.mint(r),
                voice: event.voice(),
                position: EventPosition::Musical(start),
                duration: event.duration().clone(),
                vertical_position: None,
                visible: true,
            });
            let mut out = Vec::new();
            if sim.replicas[r].open.is_none() && sim.rng.chance(2) {
                let tx: TransactionId = sim.mint(r);
                sim.replicas[r].open = Some((tx, 3));
                out.push(prim(OperationKind::DeclareTransaction(
                    TransactionDescriptor {
                        id: tx,
                        label: String::from("replace with a rest"),
                        category: None,
                    },
                )));
            }
            out.push(prim(OperationKind::DeleteEvent(DeleteEventOp {
                event: event.id(),
                tuplet_compensation: TupletCompensation::NotInTuplet,
            })));
            out.push(prim(OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event: rest,
            })));
            out
        }
        48 => {
            // A tied pair moved in one transaction, one end at a time: the
            // first move breaks the tie and the second mends it. The pair is
            // a tie the view holds, or one the gesture enters first.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let pitches_of = |event: &Event| -> Option<Vec<PitchId>> {
                match event {
                    Event::Pitched(p) => {
                        let mut ids: Vec<PitchId> = p.pitches.iter().map(|ip| ip.id).collect();
                        ids.sort();
                        Some(ids)
                    }
                    _ => None,
                }
            };
            let event_of = |id: EventId| h.events.iter().copied().find(|e| e.id() == id);
            let existing = sim.rng.pick(&h.score.cross_cutting.ties).cloned();
            let (mut out, mut first, mut second) = match existing {
                Some(tie) if sim.rng.chance(2) => (
                    Vec::new(),
                    pitches_of(event_of(tie.start_event)?)?,
                    pitches_of(event_of(tie.end_event)?)?,
                ),
                _ => {
                    let pitched: Vec<&Event> = h
                        .events
                        .iter()
                        .copied()
                        .filter(|e| matches!(e, Event::Pitched(_)) && span(e).is_some())
                        .collect();
                    let start = *sim.rng.pick(&pitched)?;
                    let (entry, continuation) = tie_entry(sim, r, h, start)?;
                    (entry, pitches_of(start)?, continuation)
                }
            };
            if sim.rng.chance(2) {
                std::mem::swap(&mut first, &mut second);
            }
            let chromatic_steps = [-2, -1, 1, 2][sim.rng.below(4) as usize];
            let tx: TransactionId = sim.mint(r);
            out.insert(
                0,
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("move the tied notes"),
                    category: None,
                })),
            );
            out.push(prim(OperationKind::Transpose(TransposeOp {
                targets: first,
                chromatic_steps,
            })));
            out.push(prim(OperationKind::Transpose(TransposeOp {
                targets: second,
                chromatic_steps,
            })));
            sim.replicas[r].open = Some((tx, out.len() as u64));
            out
        }
        49 => {
            // ModifyEvent: a chord written without one of its pitches, which
            // its author sees, so observed-remove takes it out.
            let chords: Vec<&Event> = h
                .events
                .iter()
                .copied()
                .filter(|e| matches!(e, Event::Pitched(p) if p.pitches.len() >= 2))
                .collect();
            let mut value = (*sim.rng.pick(&chords)?).clone();
            if let Event::Pitched(p) = &mut value {
                let dropped = sim.rng.below(p.pitches.len() as u64) as usize;
                p.pitches.remove(dropped);
            }
            vec![prim(OperationKind::ModifyEvent(ModifyEventOp {
                event: value,
            }))]
        }
        50 => {
            // ModifyEvent: an event written as another kind in its place: a
            // rest, an unpitched note, an indeterminate, graphic or cue
            // event, or, over a chord, a trajectory between its first and
            // last pitches, which it carries.
            let event = rng_event(sim)?;
            let (id, voice) = (event.id(), event.voice());
            let (position, duration) = (event.position().clone(), event.duration().clone());
            let mut own = Vec::new();
            event.collect_identified_pitches(&mut own);
            let own: Vec<IdentifiedPitch> = own.into_iter().cloned().collect();
            let mut written = None;
            for _ in 0..8 {
                let value = match sim.rng.below(3) {
                    0 => Event::Rest(Rest {
                        id,
                        voice,
                        position: position.clone(),
                        duration: duration.clone(),
                        vertical_position: None,
                        visible: !sim.rng.chance(5),
                    }),
                    1 => Event::Unpitched(UnpitchedEvent {
                        id,
                        voice,
                        position: position.clone(),
                        duration: duration.clone(),
                        staff_position: StaffPosition(sim.rng.range(-4, 4) as i16),
                        instrument_member: UnpitchedMemberId(0),
                        marks: Vec::new(),
                        dynamic: None,
                        stem: StemConfiguration,
                        grace: None,
                    }),
                    _ => other_kind(sim, id, voice, position.clone(), duration.clone(), &own),
                };
                if std::mem::discriminant(&value) != std::mem::discriminant(event) {
                    written = Some(value);
                    break;
                }
            }
            vec![prim(OperationKind::ModifyEvent(ModifyEventOp {
                event: written?,
            }))]
        }
        51 => {
            // ModifyEvent minting a pitch: a chord written with a new pitch
            // added, or an event of another kind written as a note of a new
            // pitch in its place; the pitch spelt half the time.
            let event = rng_event(sim)?;
            let pitch = IdentifiedPitch {
                id: sim.mint(r),
                pitch: random_pitch(&mut sim.rng, 6),
            };
            let value = match event.clone() {
                Event::Pitched(mut note) => {
                    note.pitches.push(pitch.clone());
                    Event::Pitched(note)
                }
                other => Event::Pitched(PitchedEvent {
                    id: other.id(),
                    voice: other.voice(),
                    position: other.position().clone(),
                    duration: other.duration().clone(),
                    pitches: vec![pitch.clone()],
                    marks: Vec::new(),
                    dynamic: None,
                    ornaments: Vec::new(),
                    stem: StemConfiguration,
                    grace: None,
                }),
            };
            let spelling = sim
                .rng
                .chance(2)
                .then(|| spelling_for(&mut sim.rng, &pitch.pitch))
                .flatten();
            let mut out = vec![prim(OperationKind::ModifyEvent(ModifyEventOp {
                event: value,
            }))];
            out.extend(spelling.map(|spelling| {
                prim(OperationKind::RespellPitch(RespellPitchOp {
                    pitch: pitch.id,
                    spelling,
                }))
            }));
            out
        }
        52 => {
            // A spanner moved to another staff as one command, in its own
            // transaction: onto a staff added for it (the staff created and
            // the spanner's staves rewritten to it), or onto a staff the view
            // holds.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let spanner = sim.rng.pick(&h.score.cross_cutting.spanners)?.clone();
            let tx: TransactionId = sim.mint(r);
            let declare = |id: TransactionId| {
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id,
                    label: String::from("move the line to another staff"),
                    category: None,
                }))
            };
            let mut out = vec![declare(tx)];
            if sim.rng.chance(3) {
                // Onto a staff added for it, and back again: two commands.
                let instrument = sim.rng.pick(&h.score.instruments)?.id;
                let staff: StaffId = sim.mint(r);
                let back: TransactionId = sim.mint(r);
                out.push(prim(OperationKind::CreateStaff(CreateStaffOp {
                    staff: valuegen::staff(staff, instrument),
                })));
                out.push(prim(OperationKind::ModifyCrossCutting(
                    ModifyCrossCuttingOp {
                        structure: CrossCuttingValue::Spanner(epiphany_core::Spanner {
                            staves: vec![staff],
                            ..spanner.clone()
                        }),
                    },
                )));
                sim.replicas[r].open = Some((tx, out.len() as u64));
                sim.replicas[r].queued.push((back, 2));
                out.push(declare(back));
                out.push(prim(OperationKind::ModifyCrossCutting(
                    ModifyCrossCuttingOp {
                        structure: CrossCuttingValue::Spanner(spanner),
                    },
                )));
                return Some(out);
            }
            let staff = if sim.rng.chance(2) {
                let instrument = sim.rng.pick(&h.score.instruments)?.id;
                let staff: StaffId = sim.mint(r);
                out.push(prim(OperationKind::CreateStaff(CreateStaffOp {
                    staff: valuegen::staff(staff, instrument),
                })));
                staff
            } else {
                sim.rng.pick(&h.score.staves)?.id
            };
            out.push(prim(OperationKind::ModifyCrossCutting(
                ModifyCrossCuttingOp {
                    structure: CrossCuttingValue::Spanner(epiphany_core::Spanner {
                        staves: vec![staff],
                        ..spanner
                    }),
                },
            )));
            sim.replicas[r].open = Some((tx, out.len() as u64));
            out
        }
        53 => {
            // An undo and its redo: a transaction that overwrote values
            // (`overwrite_target`) undone in a transaction of its own, which
            // is then undone in turn. Where the view holds no such
            // transaction, its author makes one first, a pitch revalued in a
            // transaction of its own, as an editor edits, undoes and redoes.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let mut policy = || match sim.rng.below(3) {
                0 => UndoPolicy::StrictInverse,
                1 => UndoPolicy::BestEffort,
                _ => UndoPolicy::Cascade,
            };
            let (undo_policy, redo_policy) = (policy(), policy());
            let declare = |id: TransactionId, label: &str| {
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id,
                    label: String::from(label),
                    category: None,
                }))
            };
            let tx: TransactionId = sim.mint(r);
            let mut out = Vec::new();
            let target = match overwrite_target(sim, r, h) {
                Some(target) => {
                    sim.replicas[r].open = Some((tx, 2));
                    target
                }
                None => {
                    // A pitch given a new value, which an undo restores.
                    let edit = make(sim, r, 12, h)?;
                    let edit_tx: TransactionId = sim.mint(r);
                    sim.replicas[r].open = Some((edit_tx, 1 + edit.len() as u64));
                    sim.replicas[r].queued.push((tx, 2));
                    out.push(declare(edit_tx, "transpose"));
                    out.extend(edit);
                    edit_tx
                }
            };
            out.push(declare(tx, "undo"));
            out.push(OperationPayload::UndoTransaction(UndoTransactionPayload {
                target,
                policy: undo_policy,
            }));
            out.push(OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: redo_policy,
            }));
            out
        }
        54 => {
            // Its author's two latest commands reverted from the undo history,
            // the older first.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let own: Vec<TransactionId> = h
                .envelopes
                .iter()
                .filter(|e| e.id.replica == sim.replicas[r].id)
                .filter_map(|e| match &e.payload {
                    OperationPayload::Primitive(OperationKind::DeclareTransaction(d)) => Some(d.id),
                    _ => None,
                })
                .collect();
            let [older, newer] = own[own.len().checked_sub(2)?..] else {
                return None;
            };
            let mut policy = || match sim.rng.below(3) {
                0 => UndoPolicy::StrictInverse,
                1 => UndoPolicy::BestEffort,
                _ => UndoPolicy::Cascade,
            };
            vec![
                OperationPayload::UndoTransaction(UndoTransactionPayload {
                    target: older,
                    policy: policy(),
                }),
                OperationPayload::UndoTransaction(UndoTransactionPayload {
                    target: newer,
                    policy: policy(),
                }),
            ]
        }
        55 => {
            // An unmeasured passage set in free time: a region added after
            // every region the view holds, a staff in it with a voice and a
            // note or rest, and the region migrated to an aleatoric model of
            // any anchoring discipline, its notes kept where they stand.
            let mut out = vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                region: placed_region(sim, r, h),
            }))];
            let Some(OperationPayload::Primitive(OperationKind::CreateRegion(op))) =
                out.first_mut()
            else {
                return None;
            };
            op.region.time_model = valuegen::metric_model();
            let region = op.region.id;
            let staff = sim.rng.pick(&h.score.staves)?.id;
            let instance: StaffInstanceId = sim.mint(r);
            let voice: VoiceId = sim.mint(r);
            let event: EventId = sim.mint(r);
            let mut quarter_tones = Vec::new();
            let length = random_duration(&mut sim.rng);
            let value = event_value(
                sim,
                r,
                event,
                voice,
                MusicalPosition::origin(),
                length,
                &mut quarter_tones,
            );
            out.push(prim(OperationKind::CreateStaffInstance(
                CreateStaffInstanceOp {
                    region,
                    instance: valuegen::staff_instance(instance, staff),
                },
            )));
            out.push(prim(OperationKind::CreateVoice(CreateVoiceOp {
                staff_instance: instance,
                voice: valuegen::voice(voice),
            })));
            out.push(prim(OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event: value,
            })));
            out.push(prim(OperationKind::ChangeRegionTimeModel(
                ChangeRegionTimeModelOp {
                    region,
                    new_time_model: RegionTimeModel::Aleatoric(AleatoricTimeModel {
                        ordering: EventOrderingDAG::default(),
                        anchoring: match sim.rng.below(4) {
                            0 => AleatoricAnchoringDiscipline::Musical,
                            1 => AleatoricAnchoringDiscipline::WallClock,
                            2 => AleatoricAnchoringDiscipline::EitherPerEvent,
                            _ => AleatoricAnchoringDiscipline::FreelyMixed,
                        },
                        bounds: BTreeMap::new(),
                        duration_hint: WallClockDuration(1),
                    }),
                    declared_incompatible: Vec::new(),
                    remapping: PositionRemapping::PreserveTime,
                },
            )));
            out
        }
        56 => {
            // SetVoiceHome: a voice's home set to a staff the view holds, or
            // cleared.
            let voices: Vec<VoiceId> = h.score.voices().map(|(_, _, v)| v.id).collect();
            let voice = *sim.rng.pick(&voices)?;
            let home = if sim.rng.chance(4) {
                None
            } else {
                Some(sim.rng.pick(&h.score.staves)?.id)
            };
            vec![prim(OperationKind::SetVoiceHome(SetVoiceHomeOp {
                voice,
                home,
            }))]
        }
        57 => {
            // A grace note in an event's voice: at the event's onset, before
            // it, or alone where it ends; marked a quarter of the time.
            let event = rng_event(sim)?;
            let (start, end) = span(event)?;
            let voice = event.voice();
            let instance = *h.instance_of_voice.get(&voice)?;
            let at = if sim.rng.chance(4) { end } else { start };
            let id: EventId = sim.mint(r);
            let pitch = IdentifiedPitch {
                id: sim.mint(r),
                pitch: random_pitch(&mut sim.rng, 6),
            };
            let value = Event::Pitched(PitchedEvent {
                id,
                voice,
                position: EventPosition::Musical(at),
                duration: EventDuration::Musical(MusicalDuration::zero()),
                pitches: vec![pitch],
                marks: if sim.rng.chance(4) {
                    random_marks(&mut sim.rng)
                } else {
                    Vec::new()
                },
                dynamic: None,
                ornaments: Vec::new(),
                stem: StemConfiguration,
                grace: Some(random_grace(&mut sim.rng)),
            });
            vec![prim(OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event: value,
            }))]
        }
        58 => {
            // ModifyEvent: a grace's order, or an event's marks or ornaments,
            // rewritten.
            let mut value = rng_event(sim)?.clone();
            match &mut value {
                Event::Pitched(p) => match p.grace.as_mut() {
                    Some(grace) if sim.rng.chance(2) => grace.order = sim.rng.below(3) as u16,
                    _ if sim.rng.chance(2) => p.marks = random_marks(&mut sim.rng),
                    _ if sim.rng.chance(3) => p.ornaments = Vec::new(),
                    _ => p.ornaments = random_ornaments(&mut sim.rng),
                },
                Event::Unpitched(u) => {
                    u.marks = if sim.rng.chance(3) {
                        Vec::new()
                    } else {
                        random_marks(&mut sim.rng)
                    }
                }
                _ => return None,
            }
            vec![prim(OperationKind::ModifyEvent(ModifyEventOp {
                event: value,
            }))]
        }
        59 => {
            // A point mark on an event, or at a position of a region.
            let anchor = if sim.rng.chance(3) {
                let regions = h.metric();
                let (region, _, _) = sim.rng.pick(&regions)?;
                valuegen::region_start_anchor(*region, position(sim.rng.below(16) as i64, 8))
            } else {
                valuegen::event_anchor(rng_event(sim)?.id())
            };
            let marker = epiphany_core::Marker {
                id: sim.mint(r),
                anchor,
                kind: random_marker_kind(&mut sim.rng),
            };
            vec![prim(OperationKind::CreateCrossCutting(
                CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Marker(marker),
                },
            ))]
        }
        60 => {
            // A lyric syllable on an event, in one of two verses, where
            // another may stand already.
            let event = rng_event(sim)?.id();
            let id = sim.mint(r);
            vec![prim(OperationKind::CreateCrossCutting(
                CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Lyric(random_lyric(id, event, &mut sim.rng)),
                },
            ))]
        }
        61 => {
            // A syllable moved away in a transaction of its own, to another
            // verse of its event or a free slot of another event; the slot it
            // left written again, by a new syllable or another moved there;
            // and the move undone, its restoration meeting the taken slot.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let lyrics = &h.score.cross_cutting.lyrics;
            let moved = sim.rng.pick(lyrics)?.clone();
            let taken: BTreeSet<(EventId, u16)> =
                lyrics.iter().map(|l| (l.event, l.verse)).collect();
            let free: Vec<(EventId, u16)> = h
                .events
                .iter()
                .flat_map(|e| [(e.id(), 1), (e.id(), 2)])
                .filter(|slot| !taken.contains(slot))
                .collect();
            let (event, verse) = *sim.rng.pick(&free)?;
            let tx: TransactionId = sim.mint(r);
            sim.replicas[r].open = Some((tx, 2));
            let mut out = vec![
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("move the syllable"),
                    category: None,
                })),
                prim(OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                    structure: CrossCuttingValue::Lyric(epiphany_core::Lyric {
                        event,
                        verse,
                        ..moved.clone()
                    }),
                })),
            ];
            let others: Vec<&epiphany_core::Lyric> =
                lyrics.iter().filter(|l| l.id != moved.id).collect();
            let fill = match sim.rng.pick(&others) {
                Some(other) if sim.rng.chance(3) => {
                    OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                        structure: CrossCuttingValue::Lyric(epiphany_core::Lyric {
                            event: moved.event,
                            verse: moved.verse,
                            ..(**other).clone()
                        }),
                    })
                }
                _ => {
                    let id = sim.mint(r);
                    OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                        structure: CrossCuttingValue::Lyric(epiphany_core::Lyric {
                            verse: moved.verse,
                            ..random_lyric(id, moved.event, &mut sim.rng)
                        }),
                    })
                }
            };
            out.push(prim(fill));
            out.push(OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: random_policy(&mut sim.rng),
            }));
            out
        }
        62 | 63 => {
            // A passage an editor adds and then takes away, a point mark on its
            // note: a metric region after every region the view holds, an
            // instance of a staff the view holds in it, and a voice, created in
            // one transaction. In 62 a note is entered after the transaction,
            // marked, and deleted, which orphans the mark to the region's
            // start; then the region is taken away, its voice, instance and
            // region deleted in turn or its transaction undone, the orphan
            // holding it. In 63 one or two notes are entered in the
            // transaction, the first marked, and the transaction undone, which
            // would orphan the mark to the region it removes.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let mut out = vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                region: placed_region(sim, r, h),
            }))];
            let Some(OperationPayload::Primitive(OperationKind::CreateRegion(op))) =
                out.first_mut()
            else {
                return None;
            };
            op.region.time_model = valuegen::metric_model();
            let region = op.region.id;
            let staff = sim.rng.pick(&h.score.staves)?.id;
            let instance: StaffInstanceId = sim.mint(r);
            let voice: VoiceId = sim.mint(r);
            let tx: TransactionId = sim.mint(r);
            out.insert(
                0,
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("add a passage"),
                    category: None,
                })),
            );
            out.push(prim(OperationKind::CreateStaffInstance(
                CreateStaffInstanceOp {
                    region,
                    instance: valuegen::staff_instance(instance, staff),
                },
            )));
            out.push(prim(OperationKind::CreateVoice(CreateVoiceOp {
                staff_instance: instance,
                voice: valuegen::voice(voice),
            })));
            let notes = if kind == 63 { 1 + sim.rng.below(2) } else { 1 };
            let mut quarter_tones = Vec::new();
            let mut at = MusicalPosition::origin();
            let mut marked = None;
            let mut entered = Vec::new();
            for _ in 0..notes {
                let event: EventId = sim.mint(r);
                let length = random_duration(&mut sim.rng);
                let value = event_value(
                    sim,
                    r,
                    event,
                    voice,
                    at.clone(),
                    length.clone(),
                    &mut quarter_tones,
                );
                at = MusicalPosition(at.0.add(&length.0));
                marked.get_or_insert(event);
                entered.push(prim(OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: instance,
                    event: value,
                })));
            }
            let in_transaction = out.len() + if kind == 63 { entered.len() } else { 0 };
            sim.replicas[r].open = Some((tx, in_transaction as u64));
            out.extend(entered);
            let note = marked?;
            out.push(prim(OperationKind::CreateCrossCutting(
                CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Marker(epiphany_core::Marker {
                        id: sim.mint(r),
                        anchor: valuegen::event_anchor(note),
                        kind: random_marker_kind(&mut sim.rng),
                    }),
                },
            )));
            if kind == 62 {
                out.push(prim(OperationKind::DeleteEvent(DeleteEventOp {
                    event: note,
                    tuplet_compensation: TupletCompensation::NotInTuplet,
                })));
                if sim.rng.chance(2) {
                    out.push(prim(OperationKind::DeleteVoice(DeleteVoiceOp { voice })));
                    out.push(prim(OperationKind::DeleteStaffInstance(
                        DeleteStaffInstanceOp {
                            staff_instance: instance,
                        },
                    )));
                    out.push(prim(OperationKind::DeleteRegion(DeleteRegionOp { region })));
                    return Some(out);
                }
            }
            out.push(OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: random_policy(&mut sim.rng),
            }));
            out
        }
        64 => {
            // An older point mark moved into a passage an editor adds and then
            // takes away: a metric region after every region the view holds,
            // an instance of a staff the view holds in it, a voice and one or
            // two notes, created in one transaction, which also moves a mark
            // the view holds (one its author places on a note first, where it
            // holds none) onto the passage's first note or its start; then the
            // transaction undone, its restoration taking the mark back to
            // where it stood, so the undo strands nothing in the region it
            // removes.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let mut out = Vec::new();
            let older = match sim.rng.pick(&h.score.cross_cutting.markers) {
                Some(marker) => marker.clone(),
                None => {
                    let event = rng_event(sim)?.id();
                    let marker = epiphany_core::Marker {
                        id: sim.mint(r),
                        anchor: valuegen::event_anchor(event),
                        kind: random_marker_kind(&mut sim.rng),
                    };
                    out.push(prim(OperationKind::CreateCrossCutting(
                        CreateCrossCuttingOp {
                            structure: CrossCuttingValue::Marker(marker.clone()),
                        },
                    )));
                    marker
                }
            };
            let mut passage = vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                region: placed_region(sim, r, h),
            }))];
            let Some(OperationPayload::Primitive(OperationKind::CreateRegion(op))) =
                passage.first_mut()
            else {
                return None;
            };
            op.region.time_model = valuegen::metric_model();
            let region = op.region.id;
            let staff = sim.rng.pick(&h.score.staves)?.id;
            let instance: StaffInstanceId = sim.mint(r);
            let voice: VoiceId = sim.mint(r);
            let tx: TransactionId = sim.mint(r);
            passage.insert(
                0,
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("add a passage"),
                    category: None,
                })),
            );
            passage.push(prim(OperationKind::CreateStaffInstance(
                CreateStaffInstanceOp {
                    region,
                    instance: valuegen::staff_instance(instance, staff),
                },
            )));
            passage.push(prim(OperationKind::CreateVoice(CreateVoiceOp {
                staff_instance: instance,
                voice: valuegen::voice(voice),
            })));
            let mut quarter_tones = Vec::new();
            let mut at = MusicalPosition::origin();
            let mut first = None;
            for _ in 0..1 + sim.rng.below(2) {
                let event: EventId = sim.mint(r);
                let length = random_duration(&mut sim.rng);
                let value = event_value(
                    sim,
                    r,
                    event,
                    voice,
                    at.clone(),
                    length.clone(),
                    &mut quarter_tones,
                );
                at = MusicalPosition(at.0.add(&length.0));
                first.get_or_insert(event);
                passage.push(prim(OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: instance,
                    event: value,
                })));
            }
            let anchor = if sim.rng.chance(2) {
                valuegen::region_start_anchor(region, MusicalPosition::origin())
            } else {
                valuegen::event_anchor(first?)
            };
            passage.push(prim(OperationKind::ModifyCrossCutting(
                ModifyCrossCuttingOp {
                    structure: CrossCuttingValue::Marker(epiphany_core::Marker { anchor, ..older }),
                },
            )));
            let members = passage.len() as u64;
            if out.is_empty() {
                sim.replicas[r].open = Some((tx, members));
            } else {
                sim.replicas[r].queued.push((tx, members));
            }
            out.extend(passage);
            out.push(OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: random_policy(&mut sim.rng),
            }));
            out
        }
        65 => {
            // A mark moved off a region's start and the region taken out of
            // musical time, then the move undone: a metric region after every
            // region the view holds, holding no staff, and a mark placed at
            // its start by a musical offset; the mark moved in a transaction
            // of its own onto an event the view holds; the region migrated to
            // a proportional model or an aleatoric one anchored in wall-clock
            // time; and the move undone, whose restoration would anchor the
            // mark by an offset the region no longer admits, so the migration
            // supersedes it (reduction version 5, P13-Q17).
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let mut region = placed_region(sim, r, h);
            region.time_model = valuegen::metric_model();
            let id = region.id;
            let marker = epiphany_core::Marker {
                id: sim.mint(r),
                anchor: valuegen::region_start_anchor(id, position(sim.rng.below(4) as i64, 4)),
                kind: random_marker_kind(&mut sim.rng),
            };
            let event = rng_event(sim)?.id();
            let model = if sim.rng.chance(2) {
                valuegen::proportional_model()
            } else {
                RegionTimeModel::Aleatoric(AleatoricTimeModel {
                    ordering: EventOrderingDAG::default(),
                    anchoring: AleatoricAnchoringDiscipline::WallClock,
                    bounds: BTreeMap::new(),
                    duration_hint: WallClockDuration(1),
                })
            };
            let tx: TransactionId = sim.mint(r);
            sim.replicas[r].queued.push((tx, 2));
            vec![
                prim(OperationKind::CreateRegion(CreateRegionOp { region })),
                prim(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Marker(marker.clone()),
                })),
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("move a mark"),
                    category: None,
                })),
                prim(OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                    structure: CrossCuttingValue::Marker(epiphany_core::Marker {
                        anchor: valuegen::event_anchor(event),
                        ..marker
                    }),
                })),
                prim(OperationKind::ChangeRegionTimeModel(
                    ChangeRegionTimeModelOp {
                        region: id,
                        new_time_model: model,
                        declared_incompatible: Vec::new(),
                        remapping: PositionRemapping::PreserveTime,
                    },
                )),
                OperationPayload::UndoTransaction(UndoTransactionPayload {
                    target: tx,
                    policy: random_policy(&mut sim.rng),
                }),
            ]
        }
        66 => {
            // A region anchored by an offset, and the region it names
            // migrated (X5b.8): a region placed as an editor places one, its
            // end at a region's end the view holds, or at its own start, by
            // an offset of a kind the named region's model admits (of either
            // kind one time in four, refused `WrongRegionTimeModel` where its
            // model does not admit it); then the named region migrated to a
            // model of any kind, which strands the anchored region where it
            // does not admit the offset. Or, one time in three, its start at
            // a note or a measure the view holds, whose region is first given
            // a model admitting either clock, so that a wall-clock offset
            // there applies, before that region is migrated to a model of any
            // kind, which strands the anchored region where it does not admit
            // the offset as it reads the note or the measure.
            let mut region = placed_region(sim, r, h);
            let gesture = sim.rng.below(3);
            if gesture == 2 {
                let offset = if sim.rng.chance(4) {
                    random_offset(&mut sim.rng)
                } else {
                    epiphany_core::AnchorOffset::WallClock(WallClockDuration(
                        250 * (1 + sim.rng.below(4) as i64),
                    ))
                };
                let (held_in, anchoring, start) = if sim.rng.chance(2) {
                    let event = rng_event(sim)?;
                    let held_in = h.voices.iter().find(|(v, _, _)| *v == event.voice())?.2;
                    let start = epiphany_core::TimeAnchor::Event {
                        id: event.id(),
                        offset,
                    };
                    (held_in, AleatoricAnchoringDiscipline::FreelyMixed, start)
                } else {
                    let measures: Vec<(RegionId, MeasureId)> = h
                        .instances()
                        .iter()
                        .flat_map(|(held_in, instance)| {
                            instance.measures.iter().map(move |m| (*held_in, m.id))
                        })
                        .collect();
                    let &(held_in, measure) = sim.rng.pick(&measures)?;
                    let anchoring = if sim.rng.chance(2) {
                        AleatoricAnchoringDiscipline::FreelyMixed
                    } else {
                        AleatoricAnchoringDiscipline::EitherPerEvent
                    };
                    let start = epiphany_core::TimeAnchor::Measure {
                        id: measure,
                        position: epiphany_core::MeasurePosition::Start,
                        offset,
                    };
                    (held_in, anchoring, start)
                };
                region.time_extent.start = start;
                let migrate = |model| {
                    prim(OperationKind::ChangeRegionTimeModel(
                        ChangeRegionTimeModelOp {
                            region: held_in,
                            new_time_model: model,
                            declared_incompatible: Vec::new(),
                            remapping: PositionRemapping::PreserveTime,
                        },
                    ))
                };
                return Some(vec![
                    migrate(aleatoric_model(anchoring)),
                    prim(OperationKind::CreateRegion(CreateRegionOp { region })),
                    migrate(random_migration_target(&mut sim.rng)),
                ]);
            }
            let (named, edge, model) = if gesture == 0 {
                let (other, model, _) = sim.rng.pick(&h.regions)?;
                (*other, epiphany_core::RegionEdge::End, (*model).clone())
            } else {
                let own = region.time_model.clone();
                (region.id, epiphany_core::RegionEdge::Start, own)
            };
            let offset = if sim.rng.chance(4) {
                random_offset(&mut sim.rng)
            } else if model.coordinate_discipline().admits_musical_offsets() {
                epiphany_core::AnchorOffset::Musical(duration(1 + sim.rng.below(4) as i64, 4))
            } else {
                epiphany_core::AnchorOffset::WallClock(WallClockDuration(
                    250 * (1 + sim.rng.below(4) as i64),
                ))
            };
            region.time_extent.end = epiphany_core::TimeAnchor::Region {
                id: named,
                edge,
                offset,
            };
            if named != region.id {
                region.time_extent.start = epiphany_core::TimeAnchor::Region {
                    id: named,
                    edge,
                    offset: epiphany_core::AnchorOffset::Zero,
                };
            }
            vec![
                prim(OperationKind::CreateRegion(CreateRegionOp { region })),
                prim(OperationKind::ChangeRegionTimeModel(
                    ChangeRegionTimeModelOp {
                        region: named,
                        new_time_model: random_migration_target(&mut sim.rng),
                        declared_incompatible: Vec::new(),
                        remapping: PositionRemapping::PreserveTime,
                    },
                )),
            ]
        }
        67 => {
            // A holder of a time anchor written by an offset, and the region
            // holding it migrated (X5b.9): a region the view holds, one time in
            // two first given a model admitting wall-clock time (freely mixed
            // where it holds notes or measures, else proportional or freely
            // mixed); then a marker, a user break, a time signature, a metric
            // grid's meter change, a tempo segment, a measure, a staff
            // instance carrying a clef, a key or its own grid, a migration to
            // a model with its own meter, or a marker moved, anchored at the
            // region's start, a note or a measure of it, and a spanner or a
            // repeat, created or (a spanner) moved, at a note of it (a spanner
            // one time in three at its start instead), by a
            // wall-clock offset three times in four (else of either kind),
            // refused `WrongRegionTimeModel` where the region does not admit
            // it; or a region placed as an editor places one carrying, at its
            // own start so, a meter of its own model, a break or a tempo
            // segment; and one time in two the holding region then migrated
            // to a model of any kind, which strands the holder where it does
            // not admit the offset. A repeat anchored to a region is the class
            // the owner parked (P13-D3), so none is written.
            let (target, model, instances) = sim.rng.pick(&h.regions)?;
            let holds_music = instances
                .iter()
                .any(|i| !i.measures.is_empty() || !i.voices.is_empty());
            let (target, model) = (*target, (*model).clone());
            let mut out = Vec::new();
            let admits_wall = model
                .coordinate_discipline()
                .admits_offset(epiphany_core::OffsetKind::WallClock);
            if !admits_wall && sim.rng.chance(2) {
                let to = if holds_music || sim.rng.chance(2) {
                    aleatoric_model(AleatoricAnchoringDiscipline::FreelyMixed)
                } else {
                    valuegen::proportional_model()
                };
                out.push(migration(target, to));
            }
            let offset = holder_offset(&mut sim.rng);
            let anchor = anchor_in(sim, h, target, offset.clone())?;
            let (held, holding) = held_by(sim, r, h, target, anchor, offset, None)?;
            out.extend(held);
            if sim.rng.chance(2) {
                out.push(migration(holding, random_migration_target(&mut sim.rng)));
            }
            out
        }
        68 => {
            // A holder's change undone after its region's migration (X5b.9,
            // arm 65's mirror for every holder an undo restores): a region the
            // view holds, given a model admitting wall-clock time first where
            // it admits none; a marker, a system or a page break, a tempo
            // segment, a time signature or a metric grid written at its start
            // by a wall-clock offset, or a spanner at a note of it so
            // (P13-D3); the holder moved off or taken away in a transaction of
            // its own; the region migrated to a model of any kind, metric one
            // time in two; and the transaction undone, whose restoration the
            // migration supersedes where the region admits the offset no
            // longer.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let (target, model, _) = sim.rng.pick(&h.regions)?;
            let (target, model) = (*target, (*model).clone());
            let gesture = sim.rng.below(6);
            let mut out = Vec::new();
            // A grid stands only where musical time is admitted too.
            let wall = epiphany_core::OffsetKind::WallClock;
            let discipline = model.coordinate_discipline();
            if !discipline.admits_offset(wall)
                || (gesture == 5 && !discipline.admits_musical_offsets())
            {
                out.push(migration(
                    target,
                    aleatoric_model(AleatoricAnchoringDiscipline::FreelyMixed),
                ));
            }
            let at = epiphany_core::TimeAnchor::Region {
                id: target,
                edge: epiphany_core::RegionEdge::Start,
                offset: epiphany_core::AnchorOffset::WallClock(WallClockDuration(
                    250 * (1 + sim.rng.below(4) as i64),
                )),
            };
            let zero = valuegen::region_start_anchor(target, MusicalPosition::origin());
            let staff = sim.rng.pick(&h.score.staves)?.id;
            let signature = sim.rng.pick(&h.score.time_signatures).map(|t| t.id);
            let (set, change) = match gesture {
                0 => {
                    let marker = epiphany_core::Marker {
                        id: sim.mint(r),
                        anchor: at,
                        kind: random_marker_kind(&mut sim.rng),
                    };
                    let moved = epiphany_core::Marker {
                        anchor: zero,
                        ..marker.clone()
                    };
                    (
                        OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                            structure: CrossCuttingValue::Marker(marker),
                        }),
                        OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                            structure: CrossCuttingValue::Marker(moved),
                        }),
                    )
                }
                1 => {
                    let epiphany_core::TimeAnchor::Region { offset, .. } = &at else {
                        return None;
                    };
                    let note = match anchor_in(sim, h, target, offset.clone())? {
                        note @ epiphany_core::TimeAnchor::Event { .. } => note,
                        _ => return None,
                    };
                    let epiphany_core::TimeAnchor::Event { id: event, .. } = note else {
                        return None;
                    };
                    let (kind, style) = random_spanner_look(&mut sim.rng);
                    let spanner = epiphany_core::Spanner {
                        id: sim.mint(r),
                        start: note,
                        end: valuegen::event_anchor(event),
                        staves: vec![staff],
                        kind,
                        style,
                    };
                    let moved = epiphany_core::Spanner {
                        start: valuegen::event_anchor(event),
                        ..spanner.clone()
                    };
                    (
                        OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                            structure: CrossCuttingValue::Spanner(spanner),
                        }),
                        OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                            structure: CrossCuttingValue::Spanner(moved),
                        }),
                    )
                }
                2 => {
                    // A system break or a page break.
                    let page = sim.rng.chance(2);
                    let user_break = |present| {
                        if page {
                            OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                                region: target,
                                anchor: at.clone(),
                                present,
                            })
                        } else {
                            OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
                                region: target,
                                anchor: at.clone(),
                                present,
                            })
                        }
                    };
                    (user_break(true), user_break(false))
                }
                3 => {
                    let scope = sim.rng.chance(2).then_some(target);
                    let segment = |present: bool| {
                        let mut segment =
                            valuegen::tempo_segment(target, MusicalPosition::origin(), 90.0);
                        segment.start = at.clone();
                        OperationKind::SetTempoSegment(SetTempoSegmentOp {
                            region: scope,
                            start: at.clone(),
                            segment: present.then_some(segment),
                        })
                    };
                    (segment(true), segment(false))
                }
                4 => {
                    let id: TimeSignatureId = sim.mint(r);
                    let signature =
                        valuegen::time_signature(id, [2u16, 3, 4][sim.rng.below(3) as usize]);
                    let meter = |signature| {
                        OperationKind::SetTimeSignature(SetTimeSignatureOp {
                            region: target,
                            anchor: at.clone(),
                            time_signature: signature,
                        })
                    };
                    (meter(Some(signature)), meter(None))
                }
                _ => {
                    let grid = epiphany_core::MetricGrid {
                        meter_sequence: vec![epiphany_core::MeterChange {
                            anchor: at,
                            time_signature: signature?,
                        }],
                    };
                    let set = |grid| {
                        OperationKind::SetMetricGrid(SetMetricGridOp {
                            region: target,
                            grid,
                        })
                    };
                    (set(Some(grid)), set(None))
                }
            };
            let tx: TransactionId = sim.mint(r);
            sim.replicas[r].queued.push((tx, 2));
            let to = if sim.rng.chance(2) {
                valuegen::metric_model()
            } else {
                random_migration_target(&mut sim.rng)
            };
            out.extend([
                prim(set),
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("change a holder"),
                    category: None,
                })),
                prim(change),
                migration(target, to),
                OperationPayload::UndoTransaction(UndoTransactionPayload {
                    target: tx,
                    policy: random_policy(&mut sim.rng),
                }),
            ]);
            out
        }
        69 => {
            // A note or a measure named before it is made (X5b.9, D84 Q1): a
            // region placed as an editor places one, its start at the id of a
            // note or a measure its author makes next, by a wall-clock offset
            // three times in four (else of either kind); then the note entered
            // into a voice of a metric region, or the measure made the next bar
            // of a metric instance, refused `WrongRegionTimeModel` where the
            // name's offset is one its region does not admit.
            let mut region = placed_region(sim, r, h);
            let offset = holder_offset(&mut sim.rng);
            let made = if sim.rng.chance(2) {
                let &(voice, instance, _) = sim.rng.pick(&h.voices)?;
                let bars = h.measures_of(instance).max(1) as u64;
                let at = position(sim.rng.below(8 * bars) as i64, 8);
                let id: EventId = sim.mint(r);
                let mut quarter_tones = Vec::new();
                let event = event_value(sim, r, id, voice, at, duration(1, 8), &mut quarter_tones);
                region.time_extent.start = epiphany_core::TimeAnchor::Event { id, offset };
                OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: instance,
                    event,
                })
            } else {
                let instances = h.metric_instances();
                let (held_in, instance) = *sim.rng.pick(&instances)?;
                let next = instance.measures.len() as i64;
                let id: MeasureId = sim.mint(r);
                region.time_extent.start = epiphany_core::TimeAnchor::Measure {
                    id,
                    position: epiphany_core::MeasurePosition::Start,
                    offset,
                };
                OperationKind::CreateMeasure(CreateMeasureOp {
                    instance: instance.id,
                    measure: epiphany_core::Measure {
                        id,
                        start: valuegen::region_start_anchor(held_in, position(next, 1)),
                        time_signature: None,
                        explicit_number: Some(next as u32 + 1),
                        number_visibility: epiphany_core::MeasureNumberVisibility::Auto,
                    },
                })
            };
            vec![
                prim(OperationKind::CreateRegion(CreateRegionOp { region })),
                prim(made),
            ]
        }
        70 => {
            // A time signature set in a transaction, a region's grid then set
            // by a `SetMetricGrid` naming it outside the transaction, at the
            // region's start or a later bar, and the transaction undone by any
            // policy (D86 Q1): a strict or cascading undo conflicts while the
            // grid names the signature, and a best-effort one keeps it.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let metric: Vec<RegionId> = h
                .regions
                .iter()
                .filter(|(_, model, _)| model.coordinate_discipline().admits_musical_offsets())
                .map(|(id, _, _)| *id)
                .collect();
            let signed = *sim.rng.pick(&metric)?;
            let gridded = *sim.rng.pick(&metric)?;
            let tx: TransactionId = sim.mint(r);
            sim.replicas[r].queued.push((tx, 2));
            let signature: TimeSignatureId = sim.mint(r);
            let bar = MusicalPosition(RationalTime::from_int(sim.rng.below(3) as i32));
            vec![
                prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: tx,
                    label: String::from("set a meter"),
                    category: None,
                })),
                prim(OperationKind::SetTimeSignature(SetTimeSignatureOp {
                    region: signed,
                    anchor: valuegen::region_start_anchor(signed, MusicalPosition::origin()),
                    time_signature: Some(valuegen::time_signature(
                        signature,
                        [2u16, 3, 4][sim.rng.below(3) as usize],
                    )),
                })),
                prim(OperationKind::SetMetricGrid(SetMetricGridOp {
                    region: gridded,
                    grid: Some(epiphany_core::MetricGrid {
                        meter_sequence: vec![epiphany_core::MeterChange {
                            anchor: valuegen::region_start_anchor(gridded, bar),
                            time_signature: signature,
                        }],
                    }),
                })),
                OperationPayload::UndoTransaction(UndoTransactionPayload {
                    target: tx,
                    policy: random_policy(&mut sim.rng),
                }),
            ]
        }
        71 => {
            // A removal a holder of what it removes holds (X5b fix round 1,
            // D86 Q2, D87 Q1 and Q3): a note entered into a voice of a metric
            // region, the next bar of a metric instance made, an empty region
            // placed as an editor places one, or a tuplet created over two
            // consecutive events of a voice, in a transaction of its own; then,
            // outside it, a holder that holds what it names written at what the
            // gesture made with no offset (a time signature, a grid's meter
            // change, a tempo segment, a measure's start, a staff instance's
            // carried clef, key or grid, or a region's extent or own meter), or
            // for a tuplet one nested in it; and the transaction undone by any
            // policy or, one time in two for a note or a region, the object
            // deleted.
            if sim.replicas[r].open.is_some() {
                return None;
            }
            let zero = epiphany_core::AnchorOffset::Zero;
            let tx: TransactionId = sim.mint(r);
            let declare = prim(OperationKind::DeclareTransaction(TransactionDescriptor {
                id: tx,
                label: String::from("make and name"),
                category: None,
            }));
            let undo = OperationPayload::UndoTransaction(UndoTransactionPayload {
                target: tx,
                policy: random_policy(&mut sim.rng),
            });
            let delete = sim.rng.chance(2);
            // One of the holders that hold what they name: a time signature,
            // a grid's meter change, a tempo segment, a measure's start or a
            // staff instance's carried clef, key or grid (`held_by`'s kinds 5
            // to 9), or a region placed with its extent or its own model's
            // meter at what the gesture made.
            let holder = sim.rng.below(7);
            let held_at = |sim: &mut Simulation,
                           region: RegionId,
                           anchor: epiphany_core::TimeAnchor|
             -> Option<Vec<OperationPayload>> {
                if holder < 5 {
                    return held_by(sim, r, h, region, anchor, zero.clone(), Some(5 + holder))
                        .map(|(held, _)| held);
                }
                let mut placed = placed_region(sim, r, h);
                if holder == 5 {
                    placed.time_extent.start = anchor;
                } else {
                    placed.time_model = RegionTimeModel::Metric(epiphany_core::MetricTimeModel {
                        meters: vec![epiphany_core::MeterChange {
                            anchor,
                            time_signature: sim.rng.pick(&h.score.time_signatures)?.id,
                        }],
                        ..Default::default()
                    });
                }
                Some(vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                    region: placed,
                }))])
            };
            let mut out = vec![declare];
            match sim.rng.below(4) {
                0 => {
                    let &(voice, instance, region) = sim.rng.pick(&h.voices)?;
                    let bars = h.measures_of(instance).max(1) as u64;
                    let at = position(sim.rng.below(8 * bars) as i64, 8);
                    let id: EventId = sim.mint(r);
                    let mut quarter_tones = Vec::new();
                    let event =
                        event_value(sim, r, id, voice, at, duration(1, 8), &mut quarter_tones);
                    let anchor = epiphany_core::TimeAnchor::Event {
                        id,
                        offset: zero.clone(),
                    };
                    let held = held_at(sim, region, anchor)?;
                    out.push(prim(OperationKind::InsertEvent(InsertEventOp {
                        staff_instance: instance,
                        event,
                    })));
                    out.extend(held);
                    out.push(if delete {
                        prim(OperationKind::DeleteEvent(DeleteEventOp {
                            event: id,
                            tuplet_compensation: TupletCompensation::NotInTuplet,
                        }))
                    } else {
                        undo
                    });
                }
                1 => {
                    let instances = h.metric_instances();
                    let (held_in, instance) = *sim.rng.pick(&instances)?;
                    let next = instance.measures.len() as i64;
                    let id: MeasureId = sim.mint(r);
                    let anchor = epiphany_core::TimeAnchor::Measure {
                        id,
                        position: epiphany_core::MeasurePosition::Start,
                        offset: zero.clone(),
                    };
                    let held = held_at(sim, held_in, anchor)?;
                    out.push(prim(OperationKind::CreateMeasure(CreateMeasureOp {
                        instance: instance.id,
                        measure: epiphany_core::Measure {
                            id,
                            start: valuegen::region_start_anchor(held_in, position(next, 1)),
                            time_signature: None,
                            explicit_number: Some(next as u32 + 1),
                            number_visibility: epiphany_core::MeasureNumberVisibility::Auto,
                        },
                    })));
                    out.extend(held);
                    out.push(undo);
                }
                2 => {
                    let region = placed_region(sim, r, h);
                    let id = region.id;
                    let (other, _, _) = sim.rng.pick(&h.regions)?;
                    let anchor = epiphany_core::TimeAnchor::Region {
                        id,
                        edge: epiphany_core::RegionEdge::Start,
                        offset: zero.clone(),
                    };
                    let held = held_at(sim, *other, anchor)?;
                    out.push(prim(OperationKind::CreateRegion(CreateRegionOp { region })));
                    out.extend(held);
                    out.push(if delete {
                        prim(OperationKind::DeleteRegion(DeleteRegionOp { region: id }))
                    } else {
                        undo
                    });
                }
                _ => {
                    let &(voice, _, _) = sim.rng.pick(&h.voices)?;
                    let mut own: Vec<&Event> = h
                        .events
                        .iter()
                        .copied()
                        .filter(|e| e.voice() == voice && span(e).is_some())
                        .collect();
                    own.sort_by(|a, b| span(a).map(|s| s.0).cmp(&span(b).map(|s| s.0)));
                    if own.len() < 2 {
                        return None;
                    }
                    let i = sim.rng.below(own.len() as u64 - 1) as usize;
                    let length = |e: &Event| match e.duration() {
                        EventDuration::Musical(d) => d.clone(),
                        _ => MusicalDuration::zero(),
                    };
                    let parent: TupletId = sim.mint(r);
                    let child: TupletId = sim.mint(r);
                    let tuplet = |id, members: Vec<EventId>, parent, total| Tuplet {
                        id,
                        ratio: TupletRatio::new(3, 2).expect("not degenerate"),
                        members,
                        parent,
                        required_total: total,
                        display: TupletDisplay::default(),
                    };
                    out.push(prim(OperationKind::CreateTuplet(CreateTupletOp {
                        tuplet: tuplet(
                            parent,
                            vec![own[i].id(), own[i + 1].id()],
                            None,
                            length(own[i]) + length(own[i + 1]),
                        ),
                    })));
                    out.push(prim(OperationKind::CreateTuplet(CreateTupletOp {
                        tuplet: tuplet(child, vec![own[i].id()], Some(parent), length(own[i])),
                    })));
                    out.push(undo);
                }
            }
            sim.replicas[r].queued.push((tx, 2));
            out
        }
        72 => {
            // A region carrying a tempo map with two segments on one key, one
            // at a note (X5b fix round 2, D89): placed as an editor places
            // one, its map a segment at a note the view holds and one at its
            // own start, a measure's start or another region's, all with no
            // offset, in either order, refused `TempoMapMalformed`; one time
            // in two the note then deleted, which, were the create applied
            // with the other segment first, would leave the note's naming
            // nothing.
            let note = sim.rng.pick(&h.events)?.id();
            let mut carrier = placed_region(sim, r, h);
            let zero = epiphany_core::AnchorOffset::Zero;
            let other = match sim.rng.below(3) {
                0 => epiphany_core::TimeAnchor::Region {
                    id: carrier.id,
                    edge: epiphany_core::RegionEdge::Start,
                    offset: zero.clone(),
                },
                1 => {
                    let instances = h.metric_instances();
                    let (_, instance) = *sim.rng.pick(&instances)?;
                    epiphany_core::TimeAnchor::Measure {
                        id: sim.rng.pick(&instance.measures)?.id,
                        position: epiphany_core::MeasurePosition::Start,
                        offset: zero.clone(),
                    }
                }
                _ => epiphany_core::TimeAnchor::Region {
                    id: sim.rng.pick(&h.regions)?.0,
                    edge: epiphany_core::RegionEdge::Start,
                    offset: zero.clone(),
                },
            };
            let mut starts = vec![
                other,
                epiphany_core::TimeAnchor::Event {
                    id: note,
                    offset: zero,
                },
            ];
            if sim.rng.chance(2) {
                starts.reverse();
            }
            let id = carrier.id;
            carrier.local_tempo_map = Some(epiphany_core::TempoMap {
                initial: None,
                segments: starts
                    .into_iter()
                    .map(|start| {
                        let mut segment =
                            valuegen::tempo_segment(id, MusicalPosition::origin(), 90.0);
                        segment.start = start;
                        segment
                    })
                    .collect(),
            });
            let mut out = vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                region: carrier,
            }))];
            if sim.rng.chance(2) {
                out.push(prim(OperationKind::DeleteEvent(DeleteEventOp {
                    event: note,
                    tuplet_compensation: TupletCompensation::NotInTuplet,
                })));
            }
            out
        }
        73 => {
            // A region carrying a tempo map no sequence of `SetTempoSegment`
            // writes could leave (X5b fix round 2's addendum, D90 Q3): placed
            // as an editor places one, in a metric model, its segments a bar
            // or more into its own start out of order, one ending past the
            // next one's start, one ending before its own start, or a linear
            // one without its end tempo; refused `TempoMapMalformed`.
            let mut carrier = placed_region(sim, r, h);
            carrier.time_model = valuegen::metric_model();
            let id = carrier.id;
            let at = |bars: i64| epiphany_core::TimeAnchor::Region {
                id,
                edge: epiphany_core::RegionEdge::Start,
                offset: epiphany_core::AnchorOffset::Musical(duration(bars, 1)),
            };
            let segment = |from: i64, to: Option<i64>| {
                let mut segment = valuegen::tempo_segment(id, MusicalPosition::origin(), 90.0);
                segment.start = at(from);
                segment.end = to.map(at);
                segment
            };
            let b = 1 + sim.rng.below(3) as i64;
            let segments = match sim.rng.below(4) {
                0 => vec![segment(b + 1, None), segment(b, None)],
                1 => vec![segment(b, Some(b + 2)), segment(b + 1, None)],
                2 => vec![segment(b + 1, Some(b))],
                _ => {
                    let mut linear = segment(b, Some(b + 1));
                    linear.shape = epiphany_core::TempoShape::Linear;
                    vec![linear]
                }
            };
            carrier.local_tempo_map = Some(epiphany_core::TempoMap {
                initial: None,
                segments,
            });
            vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                region: carrier,
            }))]
        }
        _ => return None,
    })
}

/// An anchor offset of either kind, as an extent written by hand may take
/// one: a musical one of a quarter to a whole, or a wall-clock one.
fn random_offset(rng: &mut Rng) -> epiphany_core::AnchorOffset {
    if rng.chance(2) {
        epiphany_core::AnchorOffset::Musical(duration(1 + rng.below(4) as i64, 4))
    } else {
        epiphany_core::AnchorOffset::WallClock(WallClockDuration(250 * (1 + rng.below(4) as i64)))
    }
}

/// A holder's offset (X5b.9): wall-clock three times in four, of either kind
/// otherwise.
fn holder_offset(rng: &mut Rng) -> epiphany_core::AnchorOffset {
    if rng.chance(4) {
        random_offset(rng)
    } else {
        epiphany_core::AnchorOffset::WallClock(WallClockDuration(250 * (1 + rng.below(4) as i64)))
    }
}

/// A migration of `region` to `model`, its events kept where they stand.
fn migration(region: RegionId, model: RegionTimeModel) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::ChangeRegionTimeModel(
        ChangeRegionTimeModelOp {
            region,
            new_time_model: model,
            declared_incompatible: Vec::new(),
            remapping: PositionRemapping::PreserveTime,
        },
    ))
}

/// A time anchor in `region` by `offset` (X5b.9): its start, or a note or a
/// measure the view holds in it.
fn anchor_in(
    sim: &mut Simulation,
    h: &Holdings<'_>,
    region: RegionId,
    offset: epiphany_core::AnchorOffset,
) -> Option<epiphany_core::TimeAnchor> {
    let instances = h
        .regions
        .iter()
        .find(|(id, _, _)| *id == region)
        .map(|(_, _, instances)| instances.clone())?;
    let events: Vec<EventId> = h
        .events
        .iter()
        .filter(|e| {
            instances
                .iter()
                .any(|i| i.voices.iter().any(|v| v.id == e.voice()))
        })
        .map(|e| e.id())
        .collect();
    let measures: Vec<MeasureId> = instances
        .iter()
        .flat_map(|i| i.measures.iter().map(|m| m.id))
        .collect();
    Some(match sim.rng.below(3) {
        0 if !events.is_empty() => epiphany_core::TimeAnchor::Event {
            id: *sim.rng.pick(&events)?,
            offset,
        },
        1 if !measures.is_empty() => epiphany_core::TimeAnchor::Measure {
            id: *sim.rng.pick(&measures)?,
            position: epiphany_core::MeasurePosition::Start,
            offset,
        },
        _ => epiphany_core::TimeAnchor::Region {
            id: region,
            edge: epiphany_core::RegionEdge::Start,
            offset,
        },
    })
}

/// A holder of a time anchor written at `anchor` in `region`, of any kind an
/// operation writes (X5b.9): arm 67's draw, with the region holding the
/// anchor, which the arm may then migrate. `kind` chooses the holder, drawn
/// here where it is `None` (arm 71 chooses among those that hold their
/// target against its removal).
fn held_by(
    sim: &mut Simulation,
    r: usize,
    h: &Holdings<'_>,
    region: RegionId,
    anchor: epiphany_core::TimeAnchor,
    offset: epiphany_core::AnchorOffset,
    kind: Option<u64>,
) -> Option<(Vec<OperationPayload>, RegionId)> {
    let prim = |k: OperationKind| OperationPayload::Primitive(k);
    // A repeat stands at notes alone (P13-D3), and a spanner there too but
    // one time in three, which stands at the region's start (C5b review 1's
    // L2): the note `anchor` names, or one of the region's by its offset, and
    // the note itself.
    let note = match &anchor {
        epiphany_core::TimeAnchor::Event { .. } => Some(anchor.clone()),
        _ => match anchor_in(sim, h, region, offset.clone()) {
            Some(at @ epiphany_core::TimeAnchor::Event { .. }) => Some(at),
            _ => None,
        },
    };
    let note = note.and_then(|at| match at {
        epiphany_core::TimeAnchor::Event { id, .. } => Some((at, valuegen::event_anchor(id))),
        _ => None,
    });
    let own_start = epiphany_core::TimeAnchor::Region {
        id: region,
        edge: epiphany_core::RegionEdge::Start,
        offset: offset.clone(),
    };
    let signature = sim.rng.pick(&h.score.time_signatures).map(|t| t.id);
    let meter = |anchor: epiphany_core::TimeAnchor, signature: TimeSignatureId| {
        epiphany_core::MeterChange {
            anchor,
            time_signature: signature,
        }
    };
    let held = match kind.unwrap_or_else(|| sim.rng.below(13)) {
        0 => vec![prim(OperationKind::CreateCrossCutting(
            CreateCrossCuttingOp {
                structure: CrossCuttingValue::Marker(epiphany_core::Marker {
                    id: sim.mint(r),
                    anchor,
                    kind: random_marker_kind(&mut sim.rng),
                }),
            },
        ))],
        1 => {
            let (start, end) = match note {
                Some(at) if !sim.rng.chance(3) => at,
                _ => (own_start.clone(), own_start.clone()),
            };
            let (kind, style) = random_spanner_look(&mut sim.rng);
            vec![prim(OperationKind::CreateCrossCutting(
                CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Spanner(epiphany_core::Spanner {
                        id: sim.mint(r),
                        start,
                        end,
                        staves: vec![sim.rng.pick(&h.score.staves)?.id],
                        kind,
                        style,
                    }),
                },
            ))]
        }
        2 => {
            let (start, end) = note?;
            let id: RepeatStructureId = sim.mint(r);
            vec![prim(OperationKind::CreateRepeatStructure(
                CreateRepeatStructureOp {
                    repeat: epiphany_core::RepeatStructure {
                        id,
                        start,
                        end,
                        kind: epiphany_core::RepeatKind::migration_default(),
                        voltas: Vec::new(),
                    },
                },
            ))]
        }
        3 => vec![prim(OperationKind::SetUserSystemBreak(
            SetUserSystemBreakOp {
                region,
                anchor,
                present: true,
            },
        ))],
        4 => vec![prim(OperationKind::SetUserPageBreak(SetUserPageBreakOp {
            region,
            anchor,
            present: true,
        }))],
        5 => {
            let id: TimeSignatureId = sim.mint(r);
            vec![prim(OperationKind::SetTimeSignature(SetTimeSignatureOp {
                region,
                anchor,
                time_signature: Some(valuegen::time_signature(
                    id,
                    [2u16, 3, 4][sim.rng.below(3) as usize],
                )),
            }))]
        }
        6 => vec![prim(OperationKind::SetMetricGrid(SetMetricGridOp {
            region,
            grid: Some(epiphany_core::MetricGrid {
                meter_sequence: vec![meter(anchor, signature?)],
            }),
        }))],
        7 => {
            // At the region's start, a note or a measure of it: a note's or a
            // measure's removal is held while a segment names it (D86 Q2).
            let mut segment = valuegen::tempo_segment(region, MusicalPosition::origin(), 90.0);
            segment.start = anchor.clone();
            vec![prim(OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: sim.rng.chance(2).then_some(region),
                start: anchor,
                segment: Some(segment),
            }))]
        }
        8 => {
            // The first measure of an instance of the region: one it holds
            // without a measure, or one made for it of a staff it lacks.
            let instances = h
                .regions
                .iter()
                .find(|(id, _, _)| *id == region)
                .map(|(_, _, instances)| instances.clone())?;
            let bare: Vec<StaffInstanceId> = instances
                .iter()
                .filter(|i| i.measures.is_empty())
                .map(|i| i.id)
                .collect();
            let mut out = Vec::new();
            let instance = match sim.rng.pick(&bare) {
                Some(instance) => *instance,
                None => {
                    let staff = h
                        .score
                        .staves
                        .iter()
                        .map(|s| s.id)
                        .find(|s| !instances.iter().any(|i| i.staff == *s))?;
                    let id: StaffInstanceId = sim.mint(r);
                    out.push(prim(OperationKind::CreateStaffInstance(
                        CreateStaffInstanceOp {
                            region,
                            instance: valuegen::staff_instance(id, staff),
                        },
                    )));
                    id
                }
            };
            out.push(prim(OperationKind::CreateMeasure(CreateMeasureOp {
                instance,
                measure: epiphany_core::Measure {
                    id: sim.mint(r),
                    start: anchor,
                    time_signature: None,
                    explicit_number: None,
                    number_visibility: epiphany_core::MeasureNumberVisibility::Auto,
                },
            })));
            out
        }
        9 => {
            let held = h
                .regions
                .iter()
                .find(|(id, _, _)| *id == region)
                .map(|(_, _, instances)| instances.clone())?;
            let staff = h
                .score
                .staves
                .iter()
                .map(|s| s.id)
                .find(|s| !held.iter().any(|i| i.staff == *s))?;
            let id: StaffInstanceId = sim.mint(r);
            let mut instance = valuegen::staff_instance(id, staff);
            match sim.rng.below(3) {
                0 => instance.clef_sequence.push(epiphany_core::ClefChange {
                    anchor,
                    clef: Clef::treble(),
                }),
                1 => instance
                    .key_sequence
                    .push(epiphany_core::KeySignatureChange {
                        anchor,
                        key: KeySignature::new(sim.rng.below(5) as i8 - 2)?,
                    }),
                _ => {
                    instance.local_metric_grid = Some(epiphany_core::MetricGrid {
                        meter_sequence: vec![meter(anchor, signature?)],
                    })
                }
            }
            vec![prim(OperationKind::CreateStaffInstance(
                CreateStaffInstanceOp { region, instance },
            ))]
        }
        10 => {
            // A region carrying, at its own start, a meter of its own model,
            // a break or a tempo segment: the region the arm migrates.
            let mut carrier = placed_region(sim, r, h);
            let at = epiphany_core::TimeAnchor::Region {
                id: carrier.id,
                edge: epiphany_core::RegionEdge::Start,
                offset,
            };
            match sim.rng.below(3) {
                0 => {
                    carrier.time_model = RegionTimeModel::Metric(epiphany_core::MetricTimeModel {
                        meters: vec![meter(at, signature?)],
                        ..Default::default()
                    })
                }
                1 => carrier
                    .content
                    .staff_based_mut()?
                    .user_system_breaks
                    .push(at),
                _ => {
                    let mut segment =
                        valuegen::tempo_segment(carrier.id, MusicalPosition::origin(), 90.0);
                    segment.start = at;
                    carrier.local_tempo_map = Some(epiphany_core::TempoMap {
                        initial: None,
                        segments: vec![segment],
                    });
                }
            }
            let holding = carrier.id;
            return Some((
                vec![prim(OperationKind::CreateRegion(CreateRegionOp {
                    region: carrier,
                }))],
                holding,
            ));
        }
        11 => vec![migration(
            region,
            RegionTimeModel::Metric(epiphany_core::MetricTimeModel {
                meters: vec![meter(own_start, signature?)],
                ..Default::default()
            }),
        )],
        _ => {
            // A marker the view holds moved onto the anchor, or a spanner's
            // start onto a note.
            if sim.rng.chance(2) {
                let mut marker = sim.rng.pick(&h.score.cross_cutting.markers)?.clone();
                marker.anchor = anchor;
                vec![prim(OperationKind::ModifyCrossCutting(
                    ModifyCrossCuttingOp {
                        structure: CrossCuttingValue::Marker(marker),
                    },
                ))]
            } else {
                let (start, _) = note?;
                let mut spanner = sim.rng.pick(&h.score.cross_cutting.spanners)?.clone();
                spanner.start = start;
                vec![prim(OperationKind::ModifyCrossCutting(
                    ModifyCrossCuttingOp {
                        structure: CrossCuttingValue::Spanner(spanner),
                    },
                ))]
            }
        }
    };
    Some((held, region))
}

/// A migration's target model of any kind: proportional, aleatoric by any
/// anchoring discipline, or metric.
fn random_migration_target(rng: &mut Rng) -> RegionTimeModel {
    match rng.below(3) {
        0 => valuegen::proportional_model(),
        1 => aleatoric_model(match rng.below(4) {
            0 => AleatoricAnchoringDiscipline::Musical,
            1 => AleatoricAnchoringDiscipline::WallClock,
            2 => AleatoricAnchoringDiscipline::EitherPerEvent,
            _ => AleatoricAnchoringDiscipline::FreelyMixed,
        }),
        _ => valuegen::metric_model(),
    }
}

/// An aleatoric model anchored by `anchoring`, with no ordering or bounds.
fn aleatoric_model(anchoring: AleatoricAnchoringDiscipline) -> RegionTimeModel {
    RegionTimeModel::Aleatoric(AleatoricTimeModel {
        ordering: EventOrderingDAG::default(),
        anchoring,
        bounds: BTreeMap::new(),
        duration_hint: WallClockDuration(1),
    })
}

/// An undo's policy, each of the three alike.
fn random_policy(rng: &mut Rng) -> UndoPolicy {
    match rng.below(3) {
        0 => UndoPolicy::StrictInverse,
        1 => UndoPolicy::BestEffort,
        _ => UndoPolicy::Cascade,
    }
}

/// The transaction an undo names: half the time one of its author's three
/// latest the view holds, as an editor's undo history offers, and otherwise
/// any the view declares.
fn undo_target(sim: &mut Simulation, r: usize, h: &Holdings<'_>) -> Option<TransactionId> {
    let declared: Vec<(ReplicaId, TransactionId)> = h
        .envelopes
        .iter()
        .filter_map(|e| match &e.payload {
            OperationPayload::Primitive(OperationKind::DeclareTransaction(d)) => {
                Some((e.id.replica, d.id))
            }
            _ => None,
        })
        .collect();
    let own: Vec<TransactionId> = declared
        .iter()
        .filter(|(replica, _)| *replica == sim.replicas[r].id)
        .map(|(_, tx)| *tx)
        .collect();
    let latest = &own[own.len().saturating_sub(3)..];
    match sim.rng.pick(latest) {
        Some(tx) if sim.rng.chance(2) => Some(*tx),
        _ => sim.rng.pick(&declared).map(|(_, tx)| *tx),
    }
}

/// A transaction the view holds that overwrote values, drawn as
/// [`undo_target`] draws (half the time one of its author's three latest):
/// each member writes a value over another that an undo restores (minting and
/// deleting nothing, and not the frozen `Transpose`, which records no write),
/// and applied in the view, so an undo of it restores the earlier values and a
/// redo, an undo of that undo, restores the transaction's own. (An undo of a
/// transaction that rolled back has nothing to restore, and its redo less.) A redo of an undo that removed what
/// its transaction minted has nothing to restore, a tombstone being final, so
/// drawn over any transaction most redos were refused `TargetMissing` (X4a
/// review 3's L4: 991 of 1,196 per 3,000 histories of 64 authored). `None`
/// where the view holds no such transaction.
fn overwrite_target(sim: &mut Simulation, r: usize, h: &Holdings<'_>) -> Option<TransactionId> {
    let effects: BTreeMap<OperationId, &OperationEffect> =
        h.state.effects.iter().map(|(id, e)| (*id, e)).collect();
    let overwrites = |tx: TransactionId| {
        let mut members = h
            .envelopes
            .iter()
            .filter(|e| e.transaction == Some(tx))
            .filter(|e| {
                !matches!(
                    &e.payload,
                    OperationPayload::Primitive(OperationKind::DeclareTransaction(_))
                )
            })
            .peekable();
        members.peek().is_some()
            && members.all(|e| match &e.payload {
                OperationPayload::Primitive(kind) => {
                    !matches!(
                        kind,
                        OperationKind::DeleteEvent(_)
                            | OperationKind::DeleteIdentifiedPitch(_)
                            | OperationKind::DeleteCrossCutting(_)
                            | OperationKind::DeleteRegion(_)
                            | OperationKind::DeleteStaffInstance(_)
                            | OperationKind::DeleteVoice(_)
                            | OperationKind::DeleteRepeatStructure(_)
                            // The frozen transposition records no write, so
                            // its undo restores nothing (operation catalog).
                            | OperationKind::Transpose(_)
                    ) && mints_and_refs(&e.payload).0.is_empty()
                        && matches!(
                            effects.get(&e.id),
                            Some(
                                OperationEffect::Applied
                                    | OperationEffect::AppliedWithRepair { .. }
                            )
                        )
                }
                _ => false,
            })
    };
    let declared: Vec<(ReplicaId, TransactionId)> = h
        .envelopes
        .iter()
        .filter_map(|e| match &e.payload {
            OperationPayload::Primitive(OperationKind::DeclareTransaction(d)) => {
                Some((e.id.replica, d.id))
            }
            _ => None,
        })
        .filter(|(_, tx)| overwrites(*tx))
        .collect();
    let own: Vec<TransactionId> = declared
        .iter()
        .filter(|(replica, _)| *replica == sim.replicas[r].id)
        .map(|(_, tx)| *tx)
        .collect();
    let latest = &own[own.len().saturating_sub(3)..];
    match sim.rng.pick(latest) {
        Some(tx) if sim.rng.chance(2) => Some(*tx),
        _ => sim.rng.pick(&declared).map(|(_, tx)| *tx),
    }
}

/// An editor's tie entry from `start`, a note: a continuation of its pitches
/// entered after it, where its voice leaves room, and the tie, whose end must
/// hold its start's pitches. The payloads, and the continuation's pitch ids
/// in order.
fn tie_entry(
    sim: &mut Simulation,
    r: usize,
    h: &Holdings<'_>,
    start: &Event,
) -> Option<(Vec<OperationPayload>, Vec<PitchId>)> {
    let Event::Pitched(pitched) = start else {
        return None;
    };
    let voice = start.voice();
    let (_, end) = span(start)?;
    let length = duration(1, 8);
    let after = end.clone() + length.clone();
    if overlaps(&h.spans(voice), &end, &after) {
        return None;
    }
    let instance = *h.instance_of_voice.get(&voice)?;
    let id: EventId = sim.mint(r);
    let pitches: Vec<IdentifiedPitch> = pitched
        .pitches
        .iter()
        .map(|ip| IdentifiedPitch {
            id: sim.mint(r),
            pitch: ip.pitch.clone(),
        })
        .collect();
    let mut ids: Vec<PitchId> = pitches.iter().map(|ip| ip.id).collect();
    ids.sort();
    let continuation = Event::Pitched(PitchedEvent {
        id,
        voice,
        position: EventPosition::Musical(end),
        duration: EventDuration::Musical(length),
        pitches,
        marks: Vec::new(),
        dynamic: None,
        ornaments: Vec::new(),
        stem: StemConfiguration,
        grace: None,
    });
    let mut tie = valuegen::tie(sim.mint(r), start.id(), id);
    tie.class = epiphany_core::TieClass::Standard;
    Some((
        vec![
            OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
                staff_instance: instance,
                event: continuation,
            })),
            OperationPayload::Primitive(OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                structure: CrossCuttingValue::Tie(tie),
            })),
        ],
        ids,
    ))
}

/// Each arm of [`make`] and how often it is drawn: the editing of notes,
/// pitches and their marks three times as often as the score's structure and
/// settings, as an editor's history runs.
const ARMS: [(u64, u64); 74] = [
    (0, 4),
    (1, 3),
    (2, 3),
    (3, 3),
    (4, 2),
    (5, 1),
    (6, 2),
    (7, 1),
    (8, 3),
    (9, 2),
    (10, 2),
    (11, 2),
    (12, 3),
    (13, 2),
    (14, 2),
    (15, 1),
    (16, 1),
    (17, 3),
    (18, 2),
    (19, 1),
    (20, 1),
    (21, 1),
    (22, 1),
    (23, 1),
    (24, 1),
    (25, 1),
    (26, 1),
    (27, 1),
    (28, 1),
    (29, 2),
    (30, 2),
    (31, 1),
    (32, 1),
    (33, 1),
    (34, 1),
    (35, 1),
    (36, 1),
    (37, 1),
    (38, 1),
    (39, 1),
    (40, 2),
    (41, 1),
    (42, 1),
    (43, 2),
    (44, 2),
    (45, 1),
    (46, 1),
    (47, 3),
    (48, 2),
    (49, 2),
    (50, 2),
    (51, 2),
    (52, 2),
    (53, 1),
    (54, 1),
    (55, 1),
    (56, 1),
    (57, 2),
    (58, 2),
    (59, 2),
    (60, 2),
    (61, 1),
    (62, 1),
    (63, 1),
    (64, 1),
    (65, 1),
    (66, 1),
    (67, 6),
    (68, 2),
    (69, 1),
    (70, 1),
    (71, 4),
    (72, 2),
    (73, 1),
];

fn draw_arm(rng: &mut Rng) -> u64 {
    let total: u64 = ARMS.iter().map(|(_, w)| w).sum();
    let mut x = rng.below(total);
    for (arm, weight) in ARMS {
        if x < weight {
            return arm;
        }
        x -= weight;
    }
    unreachable!("the draw is below the total weight")
}

fn set_position(event: &mut Event, at: MusicalPosition) {
    let p = EventPosition::Musical(at);
    match event {
        Event::Pitched(e) => e.position = p,
        Event::Unpitched(e) => e.position = p,
        Event::Rest(e) => e.position = p,
        Event::Indeterminate(e) => e.position = p,
        Event::Trajectory(e) => e.position = p,
        Event::Graphic(e) => e.position = p,
        Event::Cue(e) => e.position = p,
    }
}

fn set_duration(event: &mut Event, length: MusicalDuration) {
    let d = EventDuration::Musical(length);
    match event {
        Event::Pitched(e) => e.duration = d,
        Event::Unpitched(e) => e.duration = d,
        Event::Rest(e) => e.duration = d,
        Event::Indeterminate(e) => e.duration = d,
        Event::Trajectory(e) => e.duration = d,
        Event::Graphic(e) => e.duration = d,
        Event::Cue(e) => e.duration = d,
    }
}

/// A faulty replica's equivocation: a second envelope under an authored
/// operation's id, and a resolution choosing one of the two.
fn equivocate(sim: &mut Simulation) {
    let authored: Vec<usize> = (0..sim.history.len())
        .filter(|&i| sim.history[i].id.replica != ReplicaId(GENESIS))
        .collect();
    let Some(&victim) = sim.rng.pick(&authored) else {
        return;
    };
    let mut twin = sim.history[victim].clone();
    twin.payload = OperationPayload::Primitive(OperationKind::SetMetadata(SetMetadataOp {
        metadata: valuegen::score_metadata(9),
    }));
    if twin.envelope_hash() == sim.history[victim].envelope_hash() {
        return;
    }
    let chosen = if sim.rng.chance(2) {
        twin.envelope_hash()
    } else {
        sim.history[victim].envelope_hash()
    };
    let target = twin.id;
    sim.history.push(twin);
    let r = sim.rng.below(REPLICAS) as usize;
    let index = sim.history.len() - 1;
    sim.replicas[r].seen.insert(index);
    sim.author_in(
        r,
        OperationPayload::ResolveEquivocation(ResolveEquivocationPayload { target, chosen }),
        None,
    );
}

/// A generated history.
pub struct Generated {
    /// Every envelope, in authoring order.
    pub history: Vec<OperationEnvelope>,
    /// How many of them, from the start, are the genesis.
    pub genesis: usize,
    /// Every class of failure seen in a replica's view as the history grew,
    /// each with the first view's history that showed it.
    pub in_view: BTreeMap<String, (Vec<OperationEnvelope>, Finding)>,
}

/// The history seeded by `seed`: a genesis and then `authored` operations of
/// the replicas merging and authoring.
pub fn generate(seed: u64, authored: usize) -> Generated {
    let mut sim = Simulation::new(seed);
    genesis(&mut sim);
    let genesis_len = sim.history.len();
    let mut turns = 0;
    while turns < authored {
        let r = sim.rng.below(REPLICAS) as usize;
        if sim.replicas[r].open.is_none() && sim.rng.chance(4) {
            let from = sim.rng.below(REPLICAS) as usize;
            if from != r {
                sim.merge(r, from);
            }
            continue;
        }
        let view = sim.view(r);
        let holdings = Holdings::of(&view);
        let mut payloads = None;
        for _ in 0..20 {
            let kind = draw_arm(&mut sim.rng);
            // A transaction is not opened inside another.
            if kind == 6 && sim.replicas[r].open.is_some() {
                continue;
            }
            if let Some(p) = make(&mut sim, r, kind, &holdings) {
                payloads = Some(p);
                break;
            }
        }
        drop(holdings);
        for payload in payloads.into_iter().flatten() {
            sim.author(r, payload);
            turns += 1;
        }
    }
    if sim.rng.chance(8) {
        equivocate(&mut sim);
    }
    Generated {
        history: sim.history,
        genesis: genesis_len,
        in_view: sim.view_findings,
    }
}

/// What a run found: each class of failure once, with the first seed that
/// showed it and its history; and what it covered.
#[derive(Debug, Default)]
pub struct Report {
    pub iterations: u64,
    pub findings: BTreeMap<String, (u64, Vec<OperationEnvelope>, Finding)>,
    pub coverage: Coverage,
}

/// Runs `iterations` histories from `seed`, each of `authored` operations
/// after its genesis. The seed of iteration `i` is `seed + i`.
pub fn run(seed: u64, iterations: u64, authored: usize) -> Report {
    let mut report = Report::default();
    for i in 0..iterations {
        let s = seed.wrapping_add(i);
        let generated = generate(s, authored);
        let history = &generated.history;
        let mut set = OperationSet::new();
        set.accept_all(history.iter().cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&empty_base());
        report.coverage.record(&history[generated.genesis..], &free);
        for finding in compare(history, &free, &aware) {
            report
                .findings
                .entry(finding.class.clone())
                .or_insert((s, history.clone(), finding));
        }
        for (class, (sub, finding)) in generated.in_view {
            report.findings.entry(class).or_insert((s, sub, finding));
        }
        report.iterations += 1;
    }
    report
}

/// Shrinks `history` to a smaller one that still fails with `class`: removes
/// runs of envelopes, halving the run length, while the failure keeps its
/// class, until no single envelope can go. After each removal the history is
/// renumbered ([`compact`]), so a removed operation leaves no gap that every
/// later causal context would wait on.
pub fn minimize(history: &[OperationEnvelope], class: &str) -> Vec<OperationEnvelope> {
    let fails = |h: &[OperationEnvelope]| findings(h).iter().any(|f| f.class == class);
    let mut current = history.to_vec();
    let mut chunk = (current.len() / 2).max(1);
    loop {
        let mut removed = false;
        let mut start = 0;
        while start < current.len() {
            let end = (start + chunk).min(current.len());
            let kept: Vec<OperationEnvelope> = current[..start]
                .iter()
                .chain(&current[end..])
                .cloned()
                .collect();
            let candidate = compact(&kept);
            if !candidate.is_empty() && valid(&candidate) && fails(&candidate) {
                current = candidate;
                removed = true;
            } else {
                start = end;
            }
        }
        if chunk == 1 && !removed {
            return current;
        }
        if !removed {
            chunk = (chunk / 2).max(1);
        }
    }
}

/// The objects an operation mints and the objects it names, as far as this
/// generator writes them: enough to keep a shrunk history valid.
fn mints_and_refs(payload: &OperationPayload) -> (Vec<TypedObjectId>, Vec<TypedObjectId>) {
    use TypedObjectId as T;
    fn anchor(a: &epiphany_core::TimeAnchor, refs: &mut Vec<TypedObjectId>) {
        match a {
            epiphany_core::TimeAnchor::Event { id, .. } => refs.push(T::Event(*id)),
            epiphany_core::TimeAnchor::Measure { id, .. } => refs.push(T::Measure(*id)),
            epiphany_core::TimeAnchor::Region { id, .. } => refs.push(T::Region(*id)),
            epiphany_core::TimeAnchor::WallClock { .. } => {}
        }
    }
    fn event_refs(e: &Event, mints: &mut Vec<TypedObjectId>, refs: &mut Vec<TypedObjectId>) {
        mints.push(T::Event(e.id()));
        refs.push(T::Voice(e.voice()));
        let mut own = Vec::new();
        e.collect_identified_pitches(&mut own);
        mints.extend(own.iter().map(|ip| T::Pitch(ip.id)));
    }
    let (mut mints, mut refs) = (Vec::new(), Vec::new());
    let OperationPayload::Primitive(kind) = payload else {
        return (mints, refs);
    };
    match kind {
        OperationKind::InsertEvent(op) => {
            refs.push(T::StaffInstance(op.staff_instance));
            event_refs(&op.event, &mut mints, &mut refs);
        }
        OperationKind::DeleteEvent(op) => {
            refs.push(T::Event(op.event));
            match &op.tuplet_compensation {
                TupletCompensation::ReplaceWithRest { rest } => {
                    mints.push(T::Event(rest.id));
                    refs.push(T::Voice(rest.voice));
                }
                TupletCompensation::RewriteTuplets { tuplets }
                | TupletCompensation::CascadeDeleteTuplets { tuplets } => {
                    refs.extend(tuplets.iter().map(|t| T::Tuplet(*t)));
                }
                TupletCompensation::NotInTuplet => {}
            }
        }
        OperationKind::RespellPitch(op) => refs.push(T::Pitch(op.pitch)),
        OperationKind::CreateCrossCutting(CreateCrossCuttingOp { structure })
        | OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp { structure }) => {
            let creating = matches!(kind, OperationKind::CreateCrossCutting(_));
            let (own, named): (TypedObjectId, Vec<TypedObjectId>) = match structure {
                CrossCuttingValue::Slur(x) => (
                    T::Slur(x.id),
                    vec![T::Event(x.start_event), T::Event(x.end_event)],
                ),
                CrossCuttingValue::Tie(x) => (
                    T::Tie(x.id),
                    vec![T::Event(x.start_event), T::Event(x.end_event)],
                ),
                CrossCuttingValue::Beam(x) => (
                    T::Beam(x.id),
                    x.events.iter().map(|e| T::Event(*e)).collect(),
                ),
                CrossCuttingValue::Spanner(x) => {
                    let mut named: Vec<TypedObjectId> =
                        x.staves.iter().map(|s| T::Staff(*s)).collect();
                    anchor(&x.start, &mut named);
                    anchor(&x.end, &mut named);
                    (T::Spanner(x.id), named)
                }
                CrossCuttingValue::Marker(x) => {
                    let mut named = Vec::new();
                    anchor(&x.anchor, &mut named);
                    (T::Marker(x.id), named)
                }
                CrossCuttingValue::Lyric(x) => (T::LyricLine(x.id), vec![T::Event(x.event)]),
            };
            if creating {
                mints.push(own);
            } else {
                refs.push(own);
            }
            refs.extend(named);
        }
        OperationKind::ChangeRegionTimeModel(op) => {
            refs.push(T::Region(op.region));
            if let epiphany_core::RegionTimeModel::Metric(model) = &op.new_time_model {
                for meter in &model.meters {
                    anchor(&meter.anchor, &mut refs);
                }
            }
            refs.extend(op.declared_incompatible.iter().map(|e| T::Event(*e)));
            if let PositionRemapping::Reassign(pairs) = &op.remapping {
                refs.extend(pairs.iter().map(|(e, _)| T::Event(*e)));
            }
        }
        OperationKind::SetUserSystemBreak(op) => {
            refs.push(T::Region(op.region));
            anchor(&op.anchor, &mut refs);
        }
        OperationKind::SetUserPageBreak(op) => {
            refs.push(T::Region(op.region));
            anchor(&op.anchor, &mut refs);
        }
        OperationKind::ModifyEvent(op) => {
            refs.push(T::Event(op.event.id()));
            refs.push(T::Voice(op.event.voice()));
            refs.extend(modify_pitches(op));
        }
        OperationKind::Transpose(op) => refs.extend(op.targets.iter().map(|p| T::Pitch(*p))),
        OperationKind::TransposeInterval(op) => {
            refs.extend(op.targets.iter().map(|p| T::Pitch(*p)))
        }
        OperationKind::InsertIdentifiedPitch(op) => {
            refs.push(T::Event(op.event));
            mints.push(T::Pitch(op.pitch.id));
        }
        OperationKind::DeleteIdentifiedPitch(op) => refs.push(T::Pitch(op.pitch)),
        OperationKind::ModifyIdentifiedPitch(op) => refs.push(T::Pitch(op.pitch)),
        OperationKind::DeleteCrossCutting(op) => refs.push(op.structure),
        OperationKind::CreateRegion(op) => {
            mints.push(T::Region(op.region.id));
            anchor(&op.region.time_extent.start, &mut refs);
            anchor(&op.region.time_extent.end, &mut refs);
            if let epiphany_core::RegionTimeModel::Metric(model) = &op.region.time_model {
                for meter in &model.meters {
                    anchor(&meter.anchor, &mut refs);
                }
            }
            if let Some(content) = op.region.content.staff_based() {
                for meter in content
                    .default_metric_grid
                    .iter()
                    .flat_map(|grid| &grid.meter_sequence)
                {
                    anchor(&meter.anchor, &mut refs);
                }
                for at in content
                    .user_system_breaks
                    .iter()
                    .chain(&content.user_page_breaks)
                {
                    anchor(at, &mut refs);
                }
            }
            for segment in op
                .region
                .local_tempo_map
                .iter()
                .flat_map(|map| &map.segments)
            {
                anchor(&segment.start, &mut refs);
                if let Some(end) = &segment.end {
                    anchor(end, &mut refs);
                }
            }
        }
        OperationKind::DeleteRegion(op) => refs.push(T::Region(op.region)),
        OperationKind::CreateStaffInstance(op) => {
            refs.push(T::Region(op.region));
            refs.push(T::Staff(op.instance.staff));
            mints.push(T::StaffInstance(op.instance.id));
            for change in &op.instance.clef_sequence {
                anchor(&change.anchor, &mut refs);
            }
            for change in &op.instance.key_sequence {
                anchor(&change.anchor, &mut refs);
            }
            for meter in op
                .instance
                .local_metric_grid
                .iter()
                .flat_map(|grid| &grid.meter_sequence)
            {
                anchor(&meter.anchor, &mut refs);
            }
        }
        OperationKind::DeleteStaffInstance(op) => refs.push(T::StaffInstance(op.staff_instance)),
        OperationKind::CreateVoice(op) => {
            refs.push(T::StaffInstance(op.staff_instance));
            mints.push(T::Voice(op.voice.id));
        }
        OperationKind::DeleteVoice(op) => refs.push(T::Voice(op.voice)),
        OperationKind::SetMetricGrid(op) => {
            refs.push(T::Region(op.region));
            for meter in op.grid.iter().flat_map(|grid| &grid.meter_sequence) {
                anchor(&meter.anchor, &mut refs);
            }
        }
        OperationKind::CreateStaff(op) => {
            mints.push(T::Staff(op.staff.id));
            refs.push(T::Instrument(op.staff.instrument));
            refs.extend(op.staff.group.map(T::StaffGroup));
        }
        OperationKind::SetTimeSignature(op) => {
            refs.push(T::Region(op.region));
            anchor(&op.anchor, &mut refs);
            mints.extend(op.time_signature.as_ref().map(|t| T::TimeSignature(t.id)));
        }
        OperationKind::SetTempoSegment(op) => {
            refs.extend(op.region.map(T::Region));
            anchor(&op.start, &mut refs);
            if let Some(end) = op.segment.as_ref().and_then(|segment| segment.end.as_ref()) {
                anchor(end, &mut refs);
            }
        }
        OperationKind::SetStaffLayout(op) => {
            refs.push(T::StaffInstance(op.staff_instance));
            refs.extend(op.instrument_override.map(T::Instrument));
        }
        OperationKind::CreateRepeatStructure(op) => {
            mints.push(T::RepeatStructure(op.repeat.id));
            anchor(&op.repeat.start, &mut refs);
            anchor(&op.repeat.end, &mut refs);
            for volta in &op.repeat.voltas {
                anchor(&volta.start, &mut refs);
                anchor(&volta.end, &mut refs);
            }
        }
        OperationKind::DeleteRepeatStructure(op) => refs.push(T::RepeatStructure(op.repeat)),
        OperationKind::CreateInstrument(op) => mints.push(T::Instrument(op.instrument.id)),
        OperationKind::CreateStaffGroup(op) => {
            mints.push(T::StaffGroup(op.group.id));
            refs.extend(op.group.members.iter().map(|s| T::Staff(*s)));
        }
        OperationKind::CreatePartDefinition(op) => {
            mints.push(T::PartDefinition(op.part.id));
            refs.extend(op.part.staves.iter().map(|s| T::Staff(*s)));
        }
        OperationKind::CreateAnalysisLayer(op) => mints.push(T::AnalysisLayer(op.layer.id)),
        OperationKind::CreateView(op) => {
            mints.push(T::View(op.view.id));
            refs.extend(op.view.active_layers.iter().map(|l| T::AnalysisLayer(*l)));
        }
        OperationKind::CreateMeasure(op) => {
            refs.push(T::StaffInstance(op.instance));
            mints.push(T::Measure(op.measure.id));
            anchor(&op.measure.start, &mut refs);
            refs.extend(op.measure.time_signature.map(T::TimeSignature));
        }
        OperationKind::CreateTuplet(op) => {
            mints.push(T::Tuplet(op.tuplet.id));
            refs.extend(op.tuplet.members.iter().map(|e| T::Event(*e)));
            refs.extend(op.tuplet.parent.map(T::Tuplet));
        }
        OperationKind::SetClef(op) => refs.push(T::StaffInstance(op.instance)),
        OperationKind::SetKeySignature(op) => refs.push(T::StaffInstance(op.instance)),
        OperationKind::SetVoiceHome(op) => {
            refs.push(T::Voice(op.voice));
            refs.extend(op.home.map(T::Staff));
        }
        _ => {}
    }
    (mints, refs)
}

/// The pitches a whole-event modify's value carries, its own or a
/// trajectory's.
fn modify_pitches(op: &ModifyEventOp) -> Vec<TypedObjectId> {
    let mut carried = Vec::new();
    op.event.collect_identified_pitches(&mut carried);
    carried
        .iter()
        .map(|ip| TypedObjectId::Pitch(ip.id))
        .collect()
}

/// Whether every object an operation of `history` names was minted by an
/// operation its author had seen, or by itself. A system-derived id (a
/// promoted voice) is minted by a promotion, not a payload, and an author
/// names one only from a view whose reduction made it, so it is not checked.
/// A whole-event modify mints a pitch it carries that no other operation
/// mints. A region's extent may name a note or a measure before it is made
/// (arm 69, D84 Q1): such a name needs only an operation of the history that
/// mints it, so the minimizer keeps the note or the measure it names.
pub fn valid(history: &[OperationEnvelope]) -> bool {
    let mut minted_by: BTreeMap<TypedObjectId, OperationId> = BTreeMap::new();
    for envelope in history {
        for object in mints_and_refs(&envelope.payload).0 {
            minted_by.entry(object).or_insert(envelope.id);
        }
    }
    for envelope in history {
        if let OperationPayload::Primitive(OperationKind::ModifyEvent(op)) = &envelope.payload {
            for object in modify_pitches(op) {
                minted_by.entry(object).or_insert(envelope.id);
            }
        }
    }
    history.iter().all(|envelope| {
        let (mints, refs) = mints_and_refs(&envelope.payload);
        let ahead = names_ahead(&envelope.payload);
        refs.iter().all(|object| {
            matches!(object, TypedObjectId::Voice(v) if v.replica() == ReplicaId::SYSTEM_DERIVED)
                || mints.contains(object)
                || minted_by
                    .get(object)
                    .is_some_and(|op| *op == envelope.id || envelope.causal_context.covers(*op))
                || (ahead.contains(object) && minted_by.contains_key(object))
        })
    })
}

/// The events and measures a region's extent names, which it may name before
/// they are made (X5b.9, D84 Q1).
fn names_ahead(payload: &OperationPayload) -> Vec<TypedObjectId> {
    let OperationPayload::Primitive(OperationKind::CreateRegion(op)) = payload else {
        return Vec::new();
    };
    [&op.region.time_extent.start, &op.region.time_extent.end]
        .into_iter()
        .filter_map(|anchor| match anchor {
            epiphany_core::TimeAnchor::Event { id, .. } => Some(TypedObjectId::Event(*id)),
            epiphany_core::TimeAnchor::Measure { id, .. } => Some(TypedObjectId::Measure(*id)),
            _ => None,
        })
        .collect()
}

/// Renumbers each replica's operations to run from `0` without a gap, in
/// their order, and rewrites every causal context, stamp and operation
/// reference to match: a context that had seen a replica up to a counter has
/// seen it up to the last operation still present at or below it. An
/// equivocation's resolution chooses the renumbered candidate it chose, by
/// that candidate's new hash (review 2's L7: it had kept the old hash, which
/// no candidate then had, so a shrunk history lost its resolution).
pub fn compact(history: &[OperationEnvelope]) -> Vec<OperationEnvelope> {
    let mut counters: BTreeMap<ReplicaId, Vec<u64>> = BTreeMap::new();
    for envelope in history {
        counters
            .entry(envelope.id.replica)
            .or_default()
            .push(envelope.id.counter);
    }
    for list in counters.values_mut() {
        list.sort_unstable();
        list.dedup();
    }
    let renumber = |op: OperationId| -> Option<OperationId> {
        let list = counters.get(&op.replica)?;
        let index = list.binary_search(&op.counter).ok()?;
        Some(OperationId::new(op.replica, index as u64))
    };
    let seen_up_to = |replica: ReplicaId, counter: u64| -> Option<u64> {
        let list = counters.get(&replica)?;
        let below = list.partition_point(|c| *c <= counter);
        below.checked_sub(1).map(|i| i as u64)
    };
    let renumbered: Vec<OperationEnvelope> = history
        .iter()
        .map(|envelope| {
            let id = renumber(envelope.id).expect("every envelope's own id is counted");
            let mut context = CausalContext::new();
            for (replica, counter) in &envelope.causal_context.vector {
                if let Some(high) = seen_up_to(*replica, *counter) {
                    context = context.with_seen(*replica, high);
                }
            }
            for dot in envelope.causal_context.dots() {
                if let Some(dot) = renumber(dot) {
                    context = context.with_dot(dot);
                }
            }
            let payload = match &envelope.payload {
                OperationPayload::ResolveConflict(ResolveConflictPayload {
                    target,
                    action: ResolutionAction::Override { override_operation },
                }) => OperationPayload::ResolveConflict(ResolveConflictPayload {
                    target: *target,
                    action: ResolutionAction::Override {
                        override_operation: renumber(*override_operation)
                            .unwrap_or(*override_operation),
                    },
                }),
                OperationPayload::ResolveEquivocation(p) => {
                    OperationPayload::ResolveEquivocation(ResolveEquivocationPayload {
                        target: renumber(p.target).unwrap_or(p.target),
                        chosen: p.chosen,
                    })
                }
                other => other.clone(),
            };
            OperationEnvelope {
                id,
                author: envelope.author,
                stamp: OperationStamp::new(envelope.stamp.hlc, id),
                causal_context: context,
                transaction: envelope.transaction,
                payload,
            }
        })
        .collect();
    // Each envelope's hash, before and after, for the resolutions.
    let rehashed: BTreeMap<_, _> = history
        .iter()
        .zip(&renumbered)
        .map(|(before, after)| (before.envelope_hash(), after.envelope_hash()))
        .collect();
    renumbered
        .into_iter()
        .map(|mut envelope| {
            if let OperationPayload::ResolveEquivocation(p) = &mut envelope.payload {
                if let Some(chosen) = rehashed.get(&p.chosen) {
                    p.chosen = *chosen;
                }
            }
            envelope
        })
        .collect()
}

/// A history as text: one envelope per line, in the envelope text form, after
/// `#` lines naming what it shows.
pub fn render(history: &[OperationEnvelope], comments: &[String]) -> String {
    let mut out = String::new();
    for c in comments {
        for line in c.lines() {
            out.push_str("# ");
            out.push_str(line);
            out.push('\n');
        }
    }
    for envelope in history {
        out.push_str(&crate::project_envelope(envelope));
        out.push('\n');
    }
    out
}

/// Parses [`render`]'s form: blank and `#` lines are skipped.
pub fn parse(text: &str) -> Result<Vec<OperationEnvelope>, epiphany_core::textvalue::TextError> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(crate::parse_envelope)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::witness_shape;
    use super::{
        BTreeMap, BTreeSet, DeleteEventOp, ModifyEventOp, OperationEffect, OperationEnvelope,
        OperationKind, OperationPayload, TupletCompensation,
    };

    /// A region's extent names what it is anchored to, which its author
    /// must have seen: a history whose extent names an event no envelope
    /// mints is invalid, so the minimizer keeps whatever an extent names
    /// (X5b).
    #[test]
    fn a_region_s_extent_names_what_its_author_has_seen() {
        use super::{genesis, valid, valuegen, CreateRegionOp, Simulation};
        use epiphany_core::{AnchorOffset, EventId, RegionId, ReplicaId, TimeAnchor};
        let mut sim = Simulation::new(0x5B06);
        genesis(&mut sim);
        let event = sim
            .view(0)
            .reduced
            .score
            .events
            .iter_canonical()
            .next()
            .expect("a genesis event")
            .id();
        let anchored = |id: EventId| {
            let mut region = valuegen::region(RegionId::new(ReplicaId(1), 9_000));
            region.time_extent.start = TimeAnchor::Event {
                id,
                offset: AnchorOffset::Zero,
            };
            OperationPayload::Primitive(OperationKind::CreateRegion(CreateRegionOp { region }))
        };
        let mut seen = Simulation::new(0x5B06);
        genesis(&mut seen);
        seen.author(0, anchored(event));
        assert!(valid(&seen.history));
        let mut unseen = Simulation::new(0x5B06);
        genesis(&mut unseen);
        unseen.author(0, anchored(EventId::new(ReplicaId(9), 9)));
        assert!(!valid(&unseen.history));
        // Named before it is made (X5b.9): valid where the history makes it.
        let mut ahead = Simulation::new(0x5B06);
        genesis(&mut ahead);
        let insert = seen
            .history
            .iter()
            .find(|e| {
                matches!(
                    e.payload,
                    OperationPayload::Primitive(OperationKind::InsertEvent(_))
                )
            })
            .expect("a genesis insert")
            .payload
            .clone();
        let OperationPayload::Primitive(OperationKind::InsertEvent(op)) = insert else {
            unreachable!("an insert");
        };
        let later = EventId::new(ReplicaId(1), 8_999);
        let made = super::InsertEventOp {
            staff_instance: op.staff_instance,
            event: valuegen::insert_event_value(
                later,
                op.event.voice(),
                epiphany_core::MusicalPosition::origin(),
                epiphany_core::MusicalDuration(
                    epiphany_core::RationalTime::new(1, 8).expect("an eighth"),
                ),
                &[epiphany_core::PitchId::new(ReplicaId(1), 8_998)],
            ),
        };
        ahead.author(0, anchored(later));
        assert!(!valid(&ahead.history), "not yet made");
        ahead.author(
            0,
            OperationPayload::Primitive(OperationKind::InsertEvent(made)),
        );
        assert!(valid(&ahead.history), "made after");
    }

    /// Every holder of a time anchor names what its author has seen, as an
    /// extent does: a history naming an event no envelope mints in a grid's
    /// meter change, a region's own model, carried grid, break or tempo map,
    /// a staff instance's carried clef, key or grid, a migration's target
    /// model or a tempo segment's end is invalid, so the minimizer keeps what
    /// each names (X5b fix round 1: each such write is refused naming a
    /// target that is not live, and base-free reduction takes an object no
    /// envelope mints as live).
    #[test]
    fn every_holder_names_what_its_author_has_seen() {
        use super::{
            genesis, valid, valuegen, ChangeRegionTimeModelOp, CreateRegionOp,
            CreateStaffInstanceOp, PositionRemapping, SetMetricGridOp, SetTempoSegmentOp,
            Simulation,
        };
        use epiphany_core::{
            AnchorOffset, Clef, ClefChange, EventId, KeySignature, KeySignatureChange, MeterChange,
            MetricGrid, MetricTimeModel, MusicalPosition, RegionEdge, RegionId, RegionTimeModel,
            ReplicaId, StaffInstanceId, TempoMap, TimeAnchor,
        };
        let mut sim = Simulation::new(0x5B06);
        genesis(&mut sim);
        let score = &sim.view(0).reduced.score;
        let event = score
            .events
            .iter_canonical()
            .next()
            .expect("a genesis event")
            .id();
        let region = score.canvas.regions[0].id;
        let staff = score.staves[0].id;
        let signature = score.time_signatures[0].id;
        let fresh = RegionId::new(ReplicaId(1), 9_000);
        let instance = StaffInstanceId::new(ReplicaId(1), 9_001);
        let holder = |n: usize, at: TimeAnchor| -> OperationKind {
            let meter = MeterChange {
                anchor: at.clone(),
                time_signature: signature,
            };
            let model = RegionTimeModel::Metric(MetricTimeModel {
                meters: vec![meter.clone()],
                ..Default::default()
            });
            let grid = MetricGrid {
                meter_sequence: vec![meter],
            };
            let mut r = valuegen::region(fresh);
            let mut i = valuegen::staff_instance(instance, staff);
            let mut segment = valuegen::tempo_segment(region, MusicalPosition::origin(), 90.0);
            match n {
                0 => {
                    return OperationKind::SetMetricGrid(SetMetricGridOp {
                        region,
                        grid: Some(grid),
                    })
                }
                1 => r.time_model = model,
                2 => {
                    r.content
                        .staff_based_mut()
                        .expect("staff-based")
                        .default_metric_grid = Some(grid)
                }
                3 => r
                    .content
                    .staff_based_mut()
                    .expect("staff-based")
                    .user_system_breaks
                    .push(at),
                4 => {
                    segment.start = at;
                    r.local_tempo_map = Some(TempoMap {
                        initial: None,
                        segments: vec![segment],
                    });
                }
                5 => i.clef_sequence.push(ClefChange {
                    anchor: at,
                    clef: Clef::treble(),
                }),
                6 => i.key_sequence.push(KeySignatureChange {
                    anchor: at,
                    key: KeySignature::new(0).expect("C"),
                }),
                7 => i.local_metric_grid = Some(grid),
                8 => {
                    return OperationKind::ChangeRegionTimeModel(ChangeRegionTimeModelOp {
                        region,
                        new_time_model: model,
                        declared_incompatible: Vec::new(),
                        remapping: PositionRemapping::PreserveTime,
                    })
                }
                _ => {
                    segment.end = Some(at);
                    return OperationKind::SetTempoSegment(SetTempoSegmentOp {
                        region: None,
                        start: TimeAnchor::Region {
                            id: region,
                            edge: RegionEdge::Start,
                            offset: AnchorOffset::Zero,
                        },
                        segment: Some(segment),
                    });
                }
            }
            if n < 5 {
                OperationKind::CreateRegion(CreateRegionOp { region: r })
            } else {
                OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                    region,
                    instance: i,
                })
            }
        };
        let at = |id: EventId| TimeAnchor::Event {
            id,
            offset: AnchorOffset::Zero,
        };
        for n in 0..10 {
            let mut seen = Simulation::new(0x5B06);
            genesis(&mut seen);
            seen.author(0, OperationPayload::Primitive(holder(n, at(event))));
            assert!(valid(&seen.history), "holder {n}, a seen event");
            let mut unseen = Simulation::new(0x5B06);
            genesis(&mut unseen);
            let unmade = at(EventId::new(ReplicaId(9), 9));
            unseen.author(0, OperationPayload::Primitive(holder(n, unmade)));
            assert!(
                !valid(&unseen.history),
                "holder {n}, an event no envelope mints"
            );
        }
    }

    #[test]
    fn a_witness_shape_names_its_cause_not_its_objects() {
        let staff = "staff StaffId(0000000000000001:0000000000000066) instrument \
                     InstrumentId(0000000000000001:0000000000000065) is not declared";
        let other_staff = "staff StaffId(0000000000000003:0000000000000002) instrument \
                           InstrumentId(0000000000000002:0000000000000a07) is not declared";
        let meter = "region RegionId(0000000000000001:0000000000000002) default-grid meter \
                     change time signature TimeSignatureId(0000000000000003:0000000000000001) \
                     is not declared";
        assert_eq!(witness_shape(staff), witness_shape(other_staff));
        assert_eq!(
            witness_shape(staff),
            "staff StaffId instrument InstrumentId is not declared"
        );
        assert_ne!(witness_shape(staff), witness_shape(meter));
        // A list of one and of three objects, at other offsets, share a shape.
        let one = "events [EventId(0000000000000001:0000000000000004)] overlap at 3/8";
        let three = "events [EventId(0000000000000001:0000000000000004), \
                     EventId(0000000000000002:0000000000000001), \
                     EventId(0000000000000003:000000000000000c)] overlap at 1/2";
        assert_eq!(witness_shape(one), witness_shape(three));
        assert_eq!(witness_shape(one), "events [EventId] overlap at #/#");
        // So do sets of any size.
        let small = "region RegionId(0000000000000001:0000000000000002) staff_extent \
                     {StaffId(0000000000000001:0000000000000003)} != manifested staves {}";
        let large = "region RegionId(0000000000000001:0000000000000002) staff_extent \
                     {StaffId(0000000000000001:0000000000000003), \
                     StaffId(0000000000000001:0000000000000004)} != manifested staves {}";
        assert_eq!(witness_shape(small), witness_shape(large));
        // A struct keeps its fields.
        assert_eq!(
            witness_shape("anchor Region { id: RegionId(0000000000000001:0000000000000002), edge: Start } offset"),
            "anchor Region { id: RegionId, edge: Start } offset"
        );
    }

    /// `compact` carries an equivocation's choice to the renumbered candidate
    /// (review 2's L7): the ledger history with its first envelope taken out,
    /// so that every candidate's context and hash change, resolves to its
    /// twin's new hash, as it chose the twin before.
    #[test]
    fn compact_carries_an_equivocations_choice() {
        use super::{compact, OperationPayload};
        let history = super::parse(include_str!(
            "../../tests/two_modes/160-invariant-region-extents.txt"
        ))
        .expect("parses");
        let resolution = |history: &[super::OperationEnvelope]| {
            history
                .iter()
                .find_map(|env| match &env.payload {
                    OperationPayload::ResolveEquivocation(p) => Some((p.target, p.chosen)),
                    _ => None,
                })
                .expect("a resolution")
        };
        let (target, chosen) = resolution(&history);
        let twin = history
            .iter()
            .position(|env| env.id == target && env.envelope_hash() == chosen)
            .expect("the chosen candidate");
        let compacted = compact(&history[1..]);
        let (_, now) = resolution(&compacted);
        assert_ne!(
            compacted[twin - 1].envelope_hash(),
            chosen,
            "the candidate is rehashed"
        );
        assert_eq!(now, compacted[twin - 1].envelope_hash());
    }

    /// A region overlap in a graph is the invariant's plain class, which
    /// nothing excepts: the never-seen history (`110`) reduces whole, its
    /// second fill refused, and its graph with that fill written in by hand,
    /// as reduction version 4 applied it, breaks `RegionExtents` under the
    /// invariant's own name.
    #[test]
    fn a_region_overlap_is_the_invariant_s_plain_class() {
        use super::{empty_base, invariant_findings, OperationSet, RegionContent};
        use crate::{NoOpReason, PreconditionFailureReason};
        let history = super::parse(include_str!(
            "../../tests/two_modes/110-invariant-region-extents.txt"
        ))
        .expect("parses");
        let mut set = OperationSet::new();
        set.accept_all(history.iter().cloned());
        let aware = set.reduce_onto(&empty_base());
        assert!(invariant_findings(&aware.score).is_empty());
        let effects: BTreeMap<_, _> = aware.state.effects.iter().cloned().collect();
        let refused = history
            .iter()
            .find_map(|env| match (&env.payload, effects.get(&env.id)) {
                (
                    OperationPayload::Primitive(OperationKind::CreateStaffInstance(op)),
                    Some(OperationEffect::NoOp {
                        reason:
                            NoOpReason::PreconditionFailedUnderReduction {
                                reason: PreconditionFailureReason::RegionsOverlap,
                            },
                    }),
                ) => Some(op),
                _ => None,
            })
            .expect("a fill refused for the overlap");
        let mut score = aware.score.clone();
        let region = score
            .canvas
            .regions
            .iter_mut()
            .find(|region| region.id == refused.region)
            .expect("the refused fill's region");
        match &mut region.content {
            RegionContent::StaffBased(content) => {
                content.staff_instances.push(refused.instance.clone())
            }
            other => panic!("a staff-based region, not {other:?}"),
        }
        region.staff_extent.staves.push(refused.instance.staff);
        let classes: Vec<String> = invariant_findings(&score)
            .into_iter()
            .map(|f| f.class)
            .collect();
        assert!(!classes.is_empty());
        assert!(
            classes
                .iter()
                .all(|c| c.starts_with("invariant Invariant(RegionExtents: ")),
            "{classes:?}"
        );
    }

    /// Every view is checked and every class it shows kept (review 1's L2).
    /// The classes are references the owner parked (D54), which the fuzz
    /// writes none of: a cue's source, an indeterminate event's alternative
    /// and a trajectory endpoint naming another event's pitch, each left
    /// dangling by a delete of what it names. Three views, each of one, all
    /// reach the simulation's findings; one view of all three keeps them all.
    #[test]
    fn every_view_is_checked_and_every_class_kept() {
        use super::{genesis, render, Simulation};
        use epiphany_core::{
            CueEvent, CueRendering, Event, IndeterminacyHints, IndeterminacyKind,
            IndeterminateEvent, TrajectoryDisplay, TrajectoryEndpoint, TrajectoryEvent,
            TrajectoryShape,
        };
        // A genesis, its first note rewritten to name the voice's next note
        // (as a cue, an indeterminate event or a trajectory, by `which`), and
        // that next note deleted.
        let dangling = |which: u8| -> Vec<OperationEnvelope> {
            let mut sim = Simulation::new(0x5B03);
            genesis(&mut sim);
            let view = sim.view(0);
            let score = &view.reduced.score;
            let (a, b) = score
                .voices()
                .find_map(|(_, _, voice)| {
                    let notes: Vec<_> = voice
                        .events
                        .iter()
                        .filter_map(|id| match score.events.get(*id) {
                            Some(Event::Pitched(note)) => Some(note.clone()),
                            _ => None,
                        })
                        .collect();
                    (notes.len() >= 2).then(|| (notes[0].clone(), notes[1].clone()))
                })
                .expect("a voice of two notes");
            let (id, voice, position, duration) =
                (a.id, a.voice, a.position.clone(), a.duration.clone());
            let event = match which {
                0 => Event::Cue(CueEvent {
                    id,
                    voice,
                    position,
                    duration,
                    source: vec![b.id],
                    rendering: CueRendering,
                }),
                1 => Event::Indeterminate(IndeterminateEvent {
                    id,
                    voice,
                    position,
                    duration,
                    indeterminacy: IndeterminacyKind::Choice,
                    hints: IndeterminacyHints {
                        alternatives: vec![b.id],
                        ..IndeterminacyHints::default()
                    },
                }),
                _ => Event::Trajectory(TrajectoryEvent {
                    id,
                    voice,
                    position,
                    duration,
                    start: TrajectoryEndpoint::ExplicitPitch(a.pitches[0].clone()),
                    end: TrajectoryEndpoint::EventPitch(b.pitches[0].id),
                    shape: TrajectoryShape::Linear,
                    display: TrajectoryDisplay,
                }),
            };
            for kind in [
                OperationKind::ModifyEvent(ModifyEventOp { event }),
                OperationKind::DeleteEvent(DeleteEventOp {
                    event: b.id,
                    tuplet_compensation: TupletCompensation::NotInTuplet,
                }),
            ] {
                sim.author(0, OperationPayload::Primitive(kind));
            }
            sim.history
        };
        let histories = [dangling(0), dangling(1), dangling(2)];
        let mut sim = Simulation::new(0);
        let mut each = Vec::new();
        for history in &histories {
            let before: BTreeSet<String> = sim.view_findings.keys().cloned().collect();
            sim.replicas[0].seen = (0..history.len()).collect();
            sim.history = history.clone();
            sim.view(0);
            let now: BTreeSet<String> = sim.view_findings.keys().cloned().collect();
            let new: Vec<String> = now.difference(&before).cloned().collect();
            assert!(!new.is_empty(), "the view shows a class of its own");
            each.extend(new);
        }
        let all: Vec<&String> = sim.view_findings.keys().collect();
        assert_eq!(all.len(), each.len(), "three views: {all:?}");
        // One view of all three: the later histories' replicas renamed apart.
        let renamed = |history: &[OperationEnvelope], to: [&str; 3]| {
            let mut text = render(history, &[]);
            for (from, to) in ["1", "2", "3"].iter().zip(to) {
                text = text.replace(
                    &format!("#x000000000000000{from}"),
                    &format!("#x000000000000000{to}"),
                );
            }
            super::parse(&text).expect("parses")
        };
        let mut one_view = histories[0].clone();
        one_view.extend(renamed(&histories[1], ["4", "5", "6"]));
        one_view.extend(renamed(&histories[2], ["7", "8", "9"]));
        let mut sim = Simulation::new(0);
        sim.replicas[0].seen = (0..one_view.len()).collect();
        sim.history = one_view;
        sim.view(0);
        let one: Vec<&String> = sim.view_findings.keys().collect();
        assert_eq!(one.len(), all.len(), "one view: {one:?}, three: {all:?}");
    }
}
