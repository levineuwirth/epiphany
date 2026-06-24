//! Canonical byte codec for the whole [`Score`] graph (item 5 / Agent B's
//! "whole-score codec").
//!
//! The determinism crate fixes the canonical byte form of the *primitives*
//! (identifiers, hashes, [`CanonicalF64`], [`RationalTime`]); this module
//! composes those into a total, reversible byte form for every value reachable
//! from a [`Score`], so the materialized graph itself — not just the Chapter 6
//! bookkeeping projection — has a canonical serialization
//! ([`Score::canonical_bytes`] / [`Score::decode_canonical`]).
//!
//! The form is deliberately boring and total, mirroring `epiphany-ops`'
//! [`MaterializedState`](crate) codec and Appendix D §"Canonical serialization
//! determinism":
//!
//! * Integers are little-endian. Booleans are a single `0`/`1` byte.
//! * Every variable-width *leaf* (an id, a [`RationalTime`], a string) is
//!   `u32`-length-prefixed, so the decoder never guesses a boundary and is
//!   width-agnostic.
//! * Composites encode their fields in declaration order; the decoder knows the
//!   structure, so fixed-shape composites are not length-prefixed.
//! * Sequences and maps carry a `u32` count then each element (maps/sets in
//!   their canonical key order). Maps and sets are rebuilt on decode.
//! * Tagged unions carry a single discriminant byte then the variant payload.
//! * Strings are length-prefixed UTF-8 and are **not** NFC-folded here: the
//!   graph's [`Score`] equality is byte-exact on its [`String`] fields, so the
//!   codec preserves them exactly (`decode(encode(x)) == x` for every valid
//!   score). Catalog ids are already NFC at construction.
//!
//! This is a concrete, reversible canonical form that predates the Binary Format
//! companion specification; when that lands, reconcile the two (see
//! `DECISIONS.md`, P11-4). The convention set above is RATIFIED by Pass 11
//! (item 1.8): core_spec §"Binary Format Companion",
//! Requirement `req:format:codec-conventions` blesses these conventions as the
//! companion's inherited baseline, so core/ops/bundle stay mutually consistent
//! until Agent J formalizes the full companion.

use core::num::NonZeroU16;
use std::collections::{BTreeMap, BTreeSet};

use epiphany_determinism::{CanonicalDecode, CanonicalEncode, CanonicalF64, ContentHash};

use crate::event::{
    ArticulationMark, CueEvent, CueRendering, DynamicMark, Event, EventArena, GraceKind,
    GraphicEvent, IndeterminacyHints, IndeterminacyKind, IndeterminateEvent, OrnamentMark,
    PitchedEvent, PlaybackBinding, Rest, StaffPosition, StemConfiguration, TrajectoryDisplay,
    TrajectoryEndpoint, TrajectoryEvent, TrajectoryShape, UnpitchedEvent, UnpitchedMemberId,
};
use crate::graph::{
    AleatoricAnchoringDiscipline, AleatoricTimeModel, AnalysisLayer, AnalyticalAnnotation,
    AnnotationAnchor, BarlineAlignmentGroup, BarlineAlignmentMember, Beam, BeatGroup, Canvas,
    ChordSymbol, ClefChange, Comment, CrossCuttingRegistry, DecompositionAttachment,
    DecompositionSource, EventOrderingDAG, GestureAnchoring, GraphicContent, GraphicGesture,
    GraphicObject, Instrument, KeySignatureChange, LyricLine, Marker, Measure,
    MeasureNumberVisibility, MeterChange, MetricGrid, MetricTimeModel, NotatedComponent, NoteValue,
    PartDefinition, PowerOfTwo, ProportionalTimeModel, Region, RegionContent, RegionTimeModel,
    RepeatStructure, Score, ScoreMetadata, ScoreTuningContext, Slur, Spanner, Staff,
    StaffBasedContent, StaffExtent, StaffGroup, StaffGroupKind, StaffInstance,
    StaffLineConfiguration, StemDirection, TempoMapReference, Tie, TieClass, TimeExtent,
    TimeSignature, TimeSignatureDisplay, Tuplet, TupletRatio, ViewDefinition, Voice, VoiceOrigin,
};
use crate::ids::{
    AnalysisLayerId, AnalyticalAnnotationId, BarlineAlignmentGroupId, BeamId, ChordSymbolId,
    CommentId, EventId, GraphicGestureId, GraphicObjectId, IdentityContext, InstrumentId,
    LyricLineId, MarkerId, MeasureId, OperationId, PartDefinitionId, PitchId, RegionId,
    RepeatStructureId, ReplicaId, SlurId, SpannerId, StaffGroupId, StaffId, StaffInstanceId, TieId,
    TimeSignatureId, TupletId, ViewId, VoiceId,
};
use crate::pitch::{
    AccidentalId, AcousticPitch, AcousticRealization, CmnNominal, ForeignFormatId, IdentifiedPitch,
    NominalRegistryId, Pitch, PitchSpaceId, PitchSpacePosition, PitchSpelling, PositionRegistryId,
    ReferencePitch, ScalePosition, SpellingAttachment, SpellingDirective, SpellingNominal,
    SpellingPrecedence, SpellingRenderHints, SpellingRule, SpellingRuleSetId, SpellingScope,
    SpellingSource, SpellingSourceKind, StaffGroupKindRegistryId, TieClassRegistryId,
    TuningReference, TuningSystemId, VoiceSelector,
};
use crate::tempo::{Tempo, TempoMap, TempoSegment, TempoShape};
use crate::time::{
    AnchorOffset, ConcreteDuration, DurationBounds, EventBounds, EventDuration, EventPosition,
    MeasurePosition, MusicalDuration, MusicalPosition, RationalTime, RegionEdge, TimeAnchor,
    TimeBounds, WallClockDuration, WallClockTime,
};

// ===========================================================================
// Errors and the reader cursor.
// ===========================================================================

/// Why decoding canonical [`Score`] bytes failed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ScoreDecodeError {
    /// A field ended before its declared or fixed width.
    UnexpectedEof,
    /// A length prefix or count cannot be represented as `usize` on this target.
    LengthOverflow,
    /// A tagged union carried an unknown discriminant.
    InvalidTag { kind: &'static str, tag: u8 },
    /// A primitive leaf failed its own canonical decoder.
    InvalidValue(&'static str),
    /// A canonical boolean was not `0` or `1`.
    InvalidBoolean(u8),
    /// A canonical text field was not UTF-8.
    InvalidUtf8,
    /// A value failed a type invariant on reconstruction (e.g. a time signature
    /// whose beat groups do not sum to its measure duration).
    Reconstruct(&'static str),
    /// Bytes remained after the complete score was decoded.
    TrailingBytes,
}

impl core::fmt::Display for ScoreDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of score bytes"),
            Self::LengthOverflow => f.write_str("score length does not fit usize"),
            Self::InvalidTag { kind, tag } => write!(f, "invalid {kind} tag {tag}"),
            Self::InvalidValue(kind) => write!(f, "invalid canonical {kind}"),
            Self::InvalidBoolean(v) => write!(f, "invalid canonical boolean {v}"),
            Self::InvalidUtf8 => f.write_str("invalid UTF-8 in canonical text"),
            Self::Reconstruct(kind) => write!(f, "{kind} failed its invariant on decode"),
            Self::TrailingBytes => f.write_str("trailing bytes after score"),
        }
    }
}

impl std::error::Error for ScoreDecodeError {}

type Result<T> = core::result::Result<T, ScoreDecodeError>;

/// A forward-only cursor over canonical bytes.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(ScoreDecodeError::LengthOverflow)?;
        if end > self.bytes.len() {
            return Err(ScoreDecodeError::UnexpectedEof);
        }
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("8 bytes"),
        ))
    }

    fn count(&mut self) -> Result<usize> {
        usize::try_from(self.u32()?).map_err(|_| ScoreDecodeError::LengthOverflow)
    }

    /// A `u32`-length-prefixed byte slice.
    fn lp(&mut self) -> Result<&'a [u8]> {
        let n = self.count()?;
        self.take(n)
    }

    fn finish(self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(ScoreDecodeError::TrailingBytes)
        }
    }
}

// ===========================================================================
// The codec trait and combinators.
// ===========================================================================

/// A type with a canonical byte form within a [`Score`]. `enc`/`dec` are exact
/// inverses, kept adjacent per type so they cannot drift.
pub(crate) trait Codec: Sized {
    fn enc(&self, out: &mut Vec<u8>);
    fn dec(r: &mut Reader<'_>) -> Result<Self>;
}

/// Appends a `u32` little-endian length/count prefix.
fn put_len(out: &mut Vec<u8>, n: usize) {
    debug_assert!(n <= u32::MAX as usize, "canonical length exceeds u32");
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

/// A length-prefixed leaf: encode the determinism-canonical bytes behind a
/// `u32` length so the decoder is width-agnostic.
fn put_leaf<T: CanonicalEncode>(out: &mut Vec<u8>, v: &T) {
    let mut scratch = Vec::new();
    v.encode_canonical(&mut scratch);
    put_len(out, scratch.len());
    out.extend_from_slice(&scratch);
}

fn get_leaf<T: CanonicalDecode>(r: &mut Reader<'_>, name: &'static str) -> Result<T> {
    let slice = r.lp()?;
    T::decode_canonical(slice).map_err(|_| ScoreDecodeError::InvalidValue(name))
}

// --- Primitive impls. -------------------------------------------------------

impl Codec for bool {
    fn enc(&self, out: &mut Vec<u8>) {
        out.push(*self as u8);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(ScoreDecodeError::InvalidBoolean(v)),
        }
    }
}

impl Codec for u8 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.push(*self);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        r.u8()
    }
}

impl Codec for u16 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        r.u16()
    }
}

impl Codec for u32 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        r.u32()
    }
}

impl Codec for u64 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        r.u64()
    }
}

impl Codec for i8 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.push(*self as u8);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(r.u8()? as i8)
    }
}

impl Codec for i16 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(r.u16()? as i16)
    }
}

impl Codec for i32 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(r.u32()? as i32)
    }
}

impl Codec for u128 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(u128::from_le_bytes(
            r.take(16)?.try_into().expect("16 bytes"),
        ))
    }
}

impl Codec for String {
    fn enc(&self, out: &mut Vec<u8>) {
        put_len(out, self.len());
        out.extend_from_slice(self.as_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let bytes = r.lp()?;
        core::str::from_utf8(bytes)
            .map(|s| s.to_owned())
            .map_err(|_| ScoreDecodeError::InvalidUtf8)
    }
}

// --- Generic combinators. ---------------------------------------------------

impl<T: Codec> Codec for Option<T> {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            None => out.push(0),
            Some(v) => {
                out.push(1);
                v.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "Option",
                tag,
            }),
        }
    }
}

impl<T: Codec> Codec for Vec<T> {
    fn enc(&self, out: &mut Vec<u8>) {
        put_len(out, self.len());
        for item in self {
            item.enc(out);
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.count()?;
        let mut v = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            v.push(T::dec(r)?);
        }
        Ok(v)
    }
}

impl<T: Codec + Ord> Codec for BTreeSet<T> {
    fn enc(&self, out: &mut Vec<u8>) {
        put_len(out, self.len());
        for item in self {
            item.enc(out);
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.count()?;
        let mut set = BTreeSet::new();
        for _ in 0..n {
            set.insert(T::dec(r)?);
        }
        Ok(set)
    }
}

impl<K: Codec + Ord, V: Codec> Codec for BTreeMap<K, V> {
    fn enc(&self, out: &mut Vec<u8>) {
        put_len(out, self.len());
        for (k, v) in self {
            k.enc(out);
            v.enc(out);
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.count()?;
        let mut map = BTreeMap::new();
        for _ in 0..n {
            let k = K::dec(r)?;
            let v = V::dec(r)?;
            map.insert(k, v);
        }
        Ok(map)
    }
}

impl<A: Codec, B: Codec> Codec for (A, B) {
    fn enc(&self, out: &mut Vec<u8>) {
        self.0.enc(out);
        self.1.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let a = A::dec(r)?;
        let b = B::dec(r)?;
        Ok((a, b))
    }
}

// --- Leaf impls (length-prefixed determinism-canonical bytes). --------------

/// Implements [`Codec`] for a determinism-canonical leaf by length-prefixing
/// its canonical bytes; decode runs the leaf's own validating decoder.
macro_rules! leaf_codec {
    ($($ty:ty => $name:literal),* $(,)?) => {
        $(
            impl Codec for $ty {
                fn enc(&self, out: &mut Vec<u8>) {
                    put_leaf(out, self);
                }
                fn dec(r: &mut Reader<'_>) -> Result<Self> {
                    get_leaf::<$ty>(r, $name)
                }
            }
        )*
    };
}

leaf_codec! {
    ReplicaId => "ReplicaId",
    OperationId => "OperationId",
    EventId => "EventId",
    PitchId => "PitchId",
    VoiceId => "VoiceId",
    StaffId => "StaffId",
    StaffInstanceId => "StaffInstanceId",
    StaffGroupId => "StaffGroupId",
    RegionId => "RegionId",
    InstrumentId => "InstrumentId",
    PartDefinitionId => "PartDefinitionId",
    MeasureId => "MeasureId",
    BarlineAlignmentGroupId => "BarlineAlignmentGroupId",
    SlurId => "SlurId",
    TieId => "TieId",
    BeamId => "BeamId",
    SpannerId => "SpannerId",
    TupletId => "TupletId",
    MarkerId => "MarkerId",
    AnalyticalAnnotationId => "AnalyticalAnnotationId",
    CommentId => "CommentId",
    RepeatStructureId => "RepeatStructureId",
    LyricLineId => "LyricLineId",
    ChordSymbolId => "ChordSymbolId",
    GraphicObjectId => "GraphicObjectId",
    GraphicGestureId => "GraphicGestureId",
    TimeSignatureId => "TimeSignatureId",
    AnalysisLayerId => "AnalysisLayerId",
    ViewId => "ViewId",
    ContentHash => "ContentHash",
    CanonicalF64 => "CanonicalF64",
    RationalTime => "RationalTime",
    MusicalPosition => "MusicalPosition",
    MusicalDuration => "MusicalDuration",
    WallClockTime => "WallClockTime",
    WallClockDuration => "WallClockDuration",
}

// ===========================================================================
// Boilerplate macros for the composite types.
// ===========================================================================

/// [`Codec`] for a plain struct whose fields all implement [`Codec`]: encode in
/// declaration order; decode the same and rebuild the literal.
macro_rules! struct_codec {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        impl Codec for $ty {
            fn enc(&self, out: &mut Vec<u8>) {
                $( self.$field.enc(out); )*
            }
            fn dec(r: &mut Reader<'_>) -> Result<Self> {
                $( let $field = Codec::dec(r)?; )*
                Ok($ty { $($field),* })
            }
        }
    };
}

/// [`Codec`] for a zero-field unit struct: no bytes.
macro_rules! unit_codec {
    ($($ty:ident),* $(,)?) => {
        $(
            impl Codec for $ty {
                fn enc(&self, _out: &mut Vec<u8>) {}
                fn dec(_r: &mut Reader<'_>) -> Result<Self> {
                    Ok($ty)
                }
            }
        )*
    };
}

/// [`Codec`] for a fieldless ("C-like") enum: a single discriminant byte.
macro_rules! cstyle_enum_codec {
    ($ty:ident { $($tag:literal => $variant:ident),* $(,)? }) => {
        impl Codec for $ty {
            fn enc(&self, out: &mut Vec<u8>) {
                let tag: u8 = match self { $( $ty::$variant => $tag, )* };
                out.push(tag);
            }
            fn dec(r: &mut Reader<'_>) -> Result<Self> {
                match r.u8()? {
                    $( $tag => Ok($ty::$variant), )*
                    tag => Err(ScoreDecodeError::InvalidTag { kind: stringify!($ty), tag }),
                }
            }
        }
    };
}

/// [`Codec`] for a catalog-id newtype over `String` (construct via `new`, read
/// via `as_str`; the stored form is already NFC).
macro_rules! catalog_id_codec {
    ($($ty:ident),* $(,)?) => {
        $(
            impl Codec for $ty {
                fn enc(&self, out: &mut Vec<u8>) {
                    self.as_str().to_owned().enc(out);
                }
                fn dec(r: &mut Reader<'_>) -> Result<Self> {
                    Ok($ty::new(String::dec(r)?))
                }
            }
        )*
    };
}

unit_codec!(
    ArticulationMark,
    DynamicMark,
    OrnamentMark,
    StemConfiguration,
    TrajectoryDisplay,
    PlaybackBinding,
    CueRendering,
    TempoMapReference,
);

catalog_id_codec!(
    PitchSpaceId,
    TuningSystemId,
    AccidentalId,
    NominalRegistryId,
    PositionRegistryId,
    TieClassRegistryId,
    StaffGroupKindRegistryId,
    SpellingRuleSetId,
    ForeignFormatId,
);

// --- Small newtypes / std types needing reconstruction. ---------------------

impl Codec for StaffPosition {
    fn enc(&self, out: &mut Vec<u8>) {
        self.0.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(StaffPosition(i16::dec(r)?))
    }
}

impl Codec for UnpitchedMemberId {
    fn enc(&self, out: &mut Vec<u8>) {
        self.0.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(UnpitchedMemberId(u32::dec(r)?))
    }
}

impl Codec for PowerOfTwo {
    fn enc(&self, out: &mut Vec<u8>) {
        self.get().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        PowerOfTwo::new(u16::dec(r)?).ok_or(ScoreDecodeError::Reconstruct("PowerOfTwo"))
    }
}

impl Codec for NonZeroU16 {
    fn enc(&self, out: &mut Vec<u8>) {
        self.get().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        NonZeroU16::new(u16::dec(r)?).ok_or(ScoreDecodeError::Reconstruct("NonZeroU16"))
    }
}

// ===========================================================================
// time.rs
// ===========================================================================

cstyle_enum_codec!(MeasurePosition { 0 => Start, 1 => End });
cstyle_enum_codec!(RegionEdge { 0 => Start, 1 => End });

impl Codec for AnchorOffset {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            AnchorOffset::Musical(d) => {
                out.push(0);
                d.enc(out);
            }
            AnchorOffset::WallClock(d) => {
                out.push(1);
                d.enc(out);
            }
            AnchorOffset::Zero => out.push(2),
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(AnchorOffset::Musical(Codec::dec(r)?)),
            1 => Ok(AnchorOffset::WallClock(Codec::dec(r)?)),
            2 => Ok(AnchorOffset::Zero),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "AnchorOffset",
                tag,
            }),
        }
    }
}

impl Codec for TimeAnchor {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            TimeAnchor::Event { id, offset } => {
                out.push(0);
                id.enc(out);
                offset.enc(out);
            }
            TimeAnchor::Measure {
                id,
                position,
                offset,
            } => {
                out.push(1);
                id.enc(out);
                position.enc(out);
                offset.enc(out);
            }
            TimeAnchor::Region { id, edge, offset } => {
                out.push(2);
                id.enc(out);
                edge.enc(out);
                offset.enc(out);
            }
            TimeAnchor::WallClock { time } => {
                out.push(3);
                time.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(TimeAnchor::Event {
                id: Codec::dec(r)?,
                offset: Codec::dec(r)?,
            }),
            1 => Ok(TimeAnchor::Measure {
                id: Codec::dec(r)?,
                position: Codec::dec(r)?,
                offset: Codec::dec(r)?,
            }),
            2 => Ok(TimeAnchor::Region {
                id: Codec::dec(r)?,
                edge: Codec::dec(r)?,
                offset: Codec::dec(r)?,
            }),
            3 => Ok(TimeAnchor::WallClock {
                time: Codec::dec(r)?,
            }),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "TimeAnchor",
                tag,
            }),
        }
    }
}

impl Codec for EventPosition {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            EventPosition::Musical(p) => {
                out.push(0);
                p.enc(out);
            }
            EventPosition::WallClock(t) => {
                out.push(1);
                t.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(EventPosition::Musical(Codec::dec(r)?)),
            1 => Ok(EventPosition::WallClock(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "EventPosition",
                tag,
            }),
        }
    }
}

impl Codec for ConcreteDuration {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            ConcreteDuration::Musical(d) => {
                out.push(0);
                d.enc(out);
            }
            ConcreteDuration::WallClock(d) => {
                out.push(1);
                d.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(ConcreteDuration::Musical(Codec::dec(r)?)),
            1 => Ok(ConcreteDuration::WallClock(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "ConcreteDuration",
                tag,
            }),
        }
    }
}

struct_codec!(DurationBounds { lower, upper });

impl Codec for EventDuration {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            EventDuration::Musical(d) => {
                out.push(0);
                d.enc(out);
            }
            EventDuration::WallClock(d) => {
                out.push(1);
                d.enc(out);
            }
            EventDuration::Indeterminate(b) => {
                out.push(2);
                b.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(EventDuration::Musical(Codec::dec(r)?)),
            1 => Ok(EventDuration::WallClock(Codec::dec(r)?)),
            2 => Ok(EventDuration::Indeterminate(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "EventDuration",
                tag,
            }),
        }
    }
}

impl Codec for TimeBounds {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            TimeBounds::MusicalRange { min, max } => {
                out.push(0);
                min.enc(out);
                max.enc(out);
            }
            TimeBounds::WallClockRange { min, max } => {
                out.push(1);
                min.enc(out);
                max.enc(out);
            }
            TimeBounds::Unbounded => out.push(2),
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(TimeBounds::MusicalRange {
                min: Codec::dec(r)?,
                max: Codec::dec(r)?,
            }),
            1 => Ok(TimeBounds::WallClockRange {
                min: Codec::dec(r)?,
                max: Codec::dec(r)?,
            }),
            2 => Ok(TimeBounds::Unbounded),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "TimeBounds",
                tag,
            }),
        }
    }
}

struct_codec!(EventBounds { start, end });

// ===========================================================================
// pitch.rs
// ===========================================================================

cstyle_enum_codec!(CmnNominal {
    0 => C, 1 => D, 2 => E, 3 => F, 4 => G, 5 => A, 6 => B,
});

cstyle_enum_codec!(SpellingSourceKind {
    0 => UserChosen, 1 => Imported, 2 => Propagated, 3 => Inferred, 4 => Analytical,
});

impl Codec for PitchSpacePosition {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            PitchSpacePosition::Cmn {
                nominal,
                alteration,
                octave,
            } => {
                out.push(0);
                nominal.enc(out);
                alteration.enc(out);
                octave.enc(out);
            }
            PitchSpacePosition::Integer { space_size, index } => {
                out.push(1);
                space_size.enc(out);
                index.enc(out);
            }
            PitchSpacePosition::JiVector { components } => {
                out.push(2);
                components.enc(out);
            }
            PitchSpacePosition::Registered(id) => {
                out.push(3);
                id.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(PitchSpacePosition::Cmn {
                nominal: Codec::dec(r)?,
                alteration: Codec::dec(r)?,
                octave: Codec::dec(r)?,
            }),
            1 => Ok(PitchSpacePosition::Integer {
                space_size: Codec::dec(r)?,
                index: Codec::dec(r)?,
            }),
            2 => Ok(PitchSpacePosition::JiVector {
                components: Codec::dec(r)?,
            }),
            3 => Ok(PitchSpacePosition::Registered(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "PitchSpacePosition",
                tag,
            }),
        }
    }
}

struct_codec!(ScalePosition { space, position });

impl Codec for TuningReference {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            TuningReference::Inherit => out.push(0),
            TuningReference::Explicit(id) => {
                out.push(1);
                id.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(TuningReference::Inherit),
            1 => Ok(TuningReference::Explicit(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "TuningReference",
                tag,
            }),
        }
    }
}

impl Codec for AcousticRealization {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            AcousticRealization::Implicit => out.push(0),
            AcousticRealization::CentsOffset(c) => {
                out.push(1);
                c.enc(out);
            }
            AcousticRealization::AbsoluteHz(c) => {
                out.push(2);
                c.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(AcousticRealization::Implicit),
            1 => Ok(AcousticRealization::CentsOffset(Codec::dec(r)?)),
            2 => Ok(AcousticRealization::AbsoluteHz(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "AcousticRealization",
                tag,
            }),
        }
    }
}

impl Codec for ReferencePitch {
    fn enc(&self, out: &mut Vec<u8>) {
        self.position.enc(out);
        let hz = CanonicalF64::new(self.frequency_hz()).expect("valid reference pitch is finite");
        hz.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let position = PitchSpacePosition::dec(r)?;
        let hz = CanonicalF64::dec(r)?;
        ReferencePitch::new(position, hz.get())
            .ok_or(ScoreDecodeError::Reconstruct("ReferencePitch"))
    }
}

struct_codec!(AcousticPitch {
    tuning,
    realization
});
struct_codec!(Pitch {
    scale_position,
    acoustic
});
struct_codec!(IdentifiedPitch { id, pitch });

impl Codec for SpellingNominal {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            SpellingNominal::Cmn(n) => {
                out.push(0);
                n.enc(out);
            }
            SpellingNominal::Integer(i) => {
                out.push(1);
                i.enc(out);
            }
            SpellingNominal::Registered(id) => {
                out.push(2);
                id.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(SpellingNominal::Cmn(Codec::dec(r)?)),
            1 => Ok(SpellingNominal::Integer(Codec::dec(r)?)),
            2 => Ok(SpellingNominal::Registered(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "SpellingNominal",
                tag,
            }),
        }
    }
}

struct_codec!(SpellingRenderHints {
    parenthesized,
    cautionary,
    editorial,
    small_print
});
struct_codec!(PitchSpelling {
    nominal,
    accidentals,
    octave,
    render_hints
});

impl Codec for SpellingSource {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            SpellingSource::UserChosen => out.push(0),
            SpellingSource::Inferred => out.push(1),
            SpellingSource::Imported { format } => {
                out.push(2);
                format.enc(out);
            }
            SpellingSource::Propagated { from } => {
                out.push(3);
                from.enc(out);
            }
            SpellingSource::Analytical => out.push(4),
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(SpellingSource::UserChosen),
            1 => Ok(SpellingSource::Inferred),
            2 => Ok(SpellingSource::Imported {
                format: Codec::dec(r)?,
            }),
            3 => Ok(SpellingSource::Propagated {
                from: Codec::dec(r)?,
            }),
            4 => Ok(SpellingSource::Analytical),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "SpellingSource",
                tag,
            }),
        }
    }
}

impl Codec for SpellingPrecedence {
    fn enc(&self, out: &mut Vec<u8>) {
        self.order_ref().to_vec().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let order: Vec<SpellingSourceKind> = Codec::dec(r)?;
        SpellingPrecedence::new(order).ok_or(ScoreDecodeError::Reconstruct("SpellingPrecedence"))
    }
}

impl Codec for VoiceSelector {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            VoiceSelector::All => out.push(0),
            VoiceSelector::Voices(v) => {
                out.push(1);
                v.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(VoiceSelector::All),
            1 => Ok(VoiceSelector::Voices(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "VoiceSelector",
                tag,
            }),
        }
    }
}

struct_codec!(SpellingRule { rule_set });

impl Codec for SpellingScope {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            SpellingScope::Pitch(id) => {
                out.push(0);
                id.enc(out);
            }
            SpellingScope::Range { start, end, voices } => {
                out.push(1);
                start.enc(out);
                end.enc(out);
                voices.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(SpellingScope::Pitch(Codec::dec(r)?)),
            1 => Ok(SpellingScope::Range {
                start: Codec::dec(r)?,
                end: Codec::dec(r)?,
                voices: Codec::dec(r)?,
            }),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "SpellingScope",
                tag,
            }),
        }
    }
}

impl Codec for SpellingDirective {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            SpellingDirective::Explicit(s) => {
                out.push(0);
                s.enc(out);
            }
            SpellingDirective::Rule(rule) => {
                out.push(1);
                rule.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(SpellingDirective::Explicit(Codec::dec(r)?)),
            1 => Ok(SpellingDirective::Rule(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "SpellingDirective",
                tag,
            }),
        }
    }
}

struct_codec!(SpellingAttachment {
    scope,
    directive,
    source,
    priority,
    layer
});

// ===========================================================================
// event.rs
// ===========================================================================

impl Codec for GraceKind {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            GraceKind::Acciaccatura => out.push(0),
            GraceKind::Appoggiatura => out.push(1),
            GraceKind::Unmeasured => out.push(2),
            GraceKind::MeasuredFraction(d) => {
                out.push(3);
                d.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(GraceKind::Acciaccatura),
            1 => Ok(GraceKind::Appoggiatura),
            2 => Ok(GraceKind::Unmeasured),
            3 => Ok(GraceKind::MeasuredFraction(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "GraceKind",
                tag,
            }),
        }
    }
}

impl Codec for IndeterminacyKind {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            IndeterminacyKind::Pitch => out.push(0),
            IndeterminacyKind::Duration => out.push(1),
            IndeterminacyKind::Choice => out.push(2),
            IndeterminacyKind::Compound(v) => {
                out.push(3);
                v.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(IndeterminacyKind::Pitch),
            1 => Ok(IndeterminacyKind::Duration),
            2 => Ok(IndeterminacyKind::Choice),
            3 => Ok(IndeterminacyKind::Compound(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "IndeterminacyKind",
                tag,
            }),
        }
    }
}

struct_codec!(IndeterminacyHints {
    duration_bounds,
    alternatives,
    textual_instruction
});

impl Codec for TrajectoryEndpoint {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            TrajectoryEndpoint::EventPitch(id) => {
                out.push(0);
                id.enc(out);
            }
            TrajectoryEndpoint::ExplicitPitch(p) => {
                out.push(1);
                p.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(TrajectoryEndpoint::EventPitch(Codec::dec(r)?)),
            1 => Ok(TrajectoryEndpoint::ExplicitPitch(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "TrajectoryEndpoint",
                tag,
            }),
        }
    }
}

impl Codec for TrajectoryShape {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            TrajectoryShape::Linear => out.push(0),
            TrajectoryShape::Exponential => out.push(1),
            TrajectoryShape::Curve => out.push(2),
            TrajectoryShape::Stepwise(v) => {
                out.push(3);
                v.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(TrajectoryShape::Linear),
            1 => Ok(TrajectoryShape::Exponential),
            2 => Ok(TrajectoryShape::Curve),
            3 => Ok(TrajectoryShape::Stepwise(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "TrajectoryShape",
                tag,
            }),
        }
    }
}

struct_codec!(PitchedEvent {
    id,
    voice,
    position,
    duration,
    pitches,
    articulations,
    dynamic,
    ornaments,
    stem,
    grace
});
struct_codec!(UnpitchedEvent {
    id,
    voice,
    position,
    duration,
    staff_position,
    instrument_member,
    articulations,
    dynamic,
    stem,
    grace
});
struct_codec!(Rest {
    id,
    voice,
    position,
    duration,
    vertical_position,
    visible
});
struct_codec!(IndeterminateEvent {
    id,
    voice,
    position,
    duration,
    indeterminacy,
    hints
});
struct_codec!(TrajectoryEvent {
    id,
    voice,
    position,
    duration,
    start,
    end,
    shape,
    display
});
struct_codec!(GraphicEvent {
    id,
    voice,
    position,
    duration,
    graphics,
    playback_bindings
});
struct_codec!(CueEvent {
    id,
    voice,
    position,
    duration,
    source,
    rendering
});

impl Codec for Event {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            Event::Pitched(e) => {
                out.push(0);
                e.enc(out);
            }
            Event::Unpitched(e) => {
                out.push(1);
                e.enc(out);
            }
            Event::Rest(e) => {
                out.push(2);
                e.enc(out);
            }
            Event::Indeterminate(e) => {
                out.push(3);
                e.enc(out);
            }
            Event::Trajectory(e) => {
                out.push(4);
                e.enc(out);
            }
            Event::Graphic(e) => {
                out.push(5);
                e.enc(out);
            }
            Event::Cue(e) => {
                out.push(6);
                e.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(Event::Pitched(Codec::dec(r)?)),
            1 => Ok(Event::Unpitched(Codec::dec(r)?)),
            2 => Ok(Event::Rest(Codec::dec(r)?)),
            3 => Ok(Event::Indeterminate(Codec::dec(r)?)),
            4 => Ok(Event::Trajectory(Codec::dec(r)?)),
            5 => Ok(Event::Graphic(Codec::dec(r)?)),
            6 => Ok(Event::Cue(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag { kind: "Event", tag }),
        }
    }
}

impl Codec for EventArena {
    fn enc(&self, out: &mut Vec<u8>) {
        // Canonical (ascending EventId) order; identity travels inside each event.
        let events: Vec<&Event> = self.iter_canonical().collect();
        put_len(out, events.len());
        for event in events {
            event.enc(out);
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.count()?;
        let mut arena = EventArena::new();
        for _ in 0..n {
            let event = Event::dec(r)?;
            arena
                .insert(event)
                .map_err(|_| ScoreDecodeError::Reconstruct("EventArena"))?;
        }
        Ok(arena)
    }
}

// ===========================================================================
// tempo.rs
// ===========================================================================

cstyle_enum_codec!(TempoShape {
    0 => Constant, 1 => Linear, 2 => Exponential, 3 => Curve,
});

impl Codec for Tempo {
    fn enc(&self, out: &mut Vec<u8>) {
        let bpm = CanonicalF64::new(self.bpm()).expect("valid tempo bpm is finite");
        bpm.enc(out);
        self.beat_unit().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let bpm = CanonicalF64::dec(r)?;
        let beat_unit = MusicalDuration::dec(r)?;
        Tempo::new(bpm.get(), beat_unit).ok_or(ScoreDecodeError::Reconstruct("Tempo"))
    }
}

struct_codec!(TempoSegment {
    start,
    end,
    start_tempo,
    end_tempo,
    shape
});
struct_codec!(TempoMap { initial, segments });

// ===========================================================================
// graph.rs
// ===========================================================================

cstyle_enum_codec!(StemDirection { 0 => Up, 1 => Down });
cstyle_enum_codec!(MeasureNumberVisibility {
    0 => Auto, 1 => Always, 2 => Never,
});
cstyle_enum_codec!(AleatoricAnchoringDiscipline {
    0 => Musical, 1 => WallClock, 2 => EitherPerEvent, 3 => FreelyMixed,
});
cstyle_enum_codec!(NoteValue {
    0 => Whole, 1 => Half, 2 => Quarter, 3 => Eighth,
    4 => Sixteenth, 5 => ThirtySecond, 6 => SixtyFourth,
});

struct_codec!(ScoreMetadata {
    title,
    composer,
    copyright
});
struct_codec!(Instrument { id, name });
struct_codec!(StaffLineConfiguration { line_count });
struct_codec!(GraphicObject { id });
struct_codec!(GraphicContent { objects });
struct_codec!(TimeExtent { start, end });
struct_codec!(StaffExtent { staves });
struct_codec!(Staff {
    id,
    name,
    abbreviation,
    instrument,
    default_staff_lines,
    group
});
struct_codec!(PartDefinition { id, name, staves });
struct_codec!(AnalysisLayer { id, name });
struct_codec!(ViewDefinition {
    id,
    name,
    active_layers
});
struct_codec!(MeterChange {
    anchor,
    time_signature
});
struct_codec!(MetricGrid { meter_sequence });
struct_codec!(ProportionalTimeModel { duration });
struct_codec!(MetricTimeModel { meters, tempo });
struct_codec!(ClefChange { anchor });
struct_codec!(KeySignatureChange { anchor });
struct_codec!(Measure {
    id,
    start,
    time_signature,
    explicit_number,
    number_visibility
});
struct_codec!(BeatGroup {
    duration,
    subdivision,
    accent
});
struct_codec!(ScoreTuningContext {
    default_pitch_space,
    default_tuning_system,
    reference
});
struct_codec!(Slur {
    id,
    start_event,
    end_event
});
struct_codec!(Beam { id, events, level });
// TupletRatio is not a plain struct_codec!: it has private fields and a checked
// constructor that rejects degenerate ratios, so decode must validate too
// (a malformed bundle cannot inject a degenerate ratio).
impl Codec for TupletRatio {
    fn enc(&self, out: &mut Vec<u8>) {
        self.actual().enc(out);
        self.notated().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let actual = Codec::dec(r)?;
        let notated = Codec::dec(r)?;
        TupletRatio::new(actual, notated).ok_or(ScoreDecodeError::Reconstruct(
            "TupletRatio: degenerate ratio",
        ))
    }
}
struct_codec!(Tuplet {
    id,
    ratio,
    members,
    parent,
    required_total
});
struct_codec!(Spanner {
    id,
    start,
    end,
    staves
});
struct_codec!(Marker { id, anchor });
struct_codec!(RepeatStructure { id, start, end });
struct_codec!(Comment {
    id,
    anchor,
    resolved
});
struct_codec!(LyricLine { id, events });
struct_codec!(ChordSymbol { id, anchor });
struct_codec!(AnalyticalAnnotation { id, anchor, layer });
struct_codec!(GraphicGesture {
    id,
    objects,
    anchoring
});
struct_codec!(BarlineAlignmentMember {
    staff_instance,
    measure,
    position
});
struct_codec!(BarlineAlignmentGroup { id, members });
struct_codec!(NotatedComponent {
    base_value,
    dots,
    tuplet,
    tied_to_next
});
struct_codec!(Voice {
    id,
    events,
    default_stem_direction,
    is_primary,
    origin
});
struct_codec!(StaffInstance {
    id,
    staff,
    voices,
    clef_sequence,
    key_sequence,
    local_metric_grid,
    measures,
    instrument_override,
    staff_lines_override,
    visible
});
struct_codec!(StaffBasedContent {
    staff_instances,
    default_metric_grid,
    barline_alignment_groups,
    user_system_breaks,
    user_page_breaks
});
struct_codec!(Region {
    id,
    time_model,
    content,
    time_extent,
    staff_extent,
    local_tempo_map
});
struct_codec!(Canvas { regions });
struct_codec!(CrossCuttingRegistry {
    slurs,
    ties,
    beams,
    tuplets,
    spanners,
    markers,
    repeats,
    analytical,
    comments,
    graphic_gestures,
    lyrics,
    chord_symbols
});
struct_codec!(IdentityContext {
    replica_id,
    next_counter
});

impl Codec for EventOrderingDAG {
    fn enc(&self, out: &mut Vec<u8>) {
        self.edges_ref().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let edges: BTreeMap<EventId, Vec<EventId>> = Codec::dec(r)?;
        EventOrderingDAG::try_new(edges).ok_or(ScoreDecodeError::Reconstruct("EventOrderingDAG"))
    }
}

struct_codec!(AleatoricTimeModel {
    ordering,
    anchoring,
    bounds,
    duration_hint
});

impl Codec for RegionTimeModel {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            RegionTimeModel::Metric(m) => {
                out.push(0);
                m.enc(out);
            }
            RegionTimeModel::Proportional(p) => {
                out.push(1);
                p.enc(out);
            }
            RegionTimeModel::Aleatoric(a) => {
                out.push(2);
                a.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(RegionTimeModel::Metric(Codec::dec(r)?)),
            1 => Ok(RegionTimeModel::Proportional(Codec::dec(r)?)),
            2 => Ok(RegionTimeModel::Aleatoric(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "RegionTimeModel",
                tag,
            }),
        }
    }
}

impl Codec for RegionContent {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            RegionContent::StaffBased(s) => {
                out.push(0);
                s.enc(out);
            }
            RegionContent::FreeGraphic(g) => {
                out.push(1);
                g.enc(out);
            }
            RegionContent::Hybrid {
                staves,
                overlay,
                overlay_below_staves,
            } => {
                out.push(2);
                staves.enc(out);
                overlay.enc(out);
                overlay_below_staves.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(RegionContent::StaffBased(Codec::dec(r)?)),
            1 => Ok(RegionContent::FreeGraphic(Codec::dec(r)?)),
            2 => Ok(RegionContent::Hybrid {
                staves: Codec::dec(r)?,
                overlay: Codec::dec(r)?,
                overlay_below_staves: Codec::dec(r)?,
            }),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "RegionContent",
                tag,
            }),
        }
    }
}

impl Codec for VoiceOrigin {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            VoiceOrigin::UserDeclared => out.push(0),
            VoiceOrigin::Imported { format } => {
                out.push(1);
                format.enc(out);
            }
            VoiceOrigin::SystemPromoted {
                winning_operation,
                losing_operation,
                original_voice,
            } => {
                out.push(2);
                winning_operation.enc(out);
                losing_operation.enc(out);
                original_voice.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(VoiceOrigin::UserDeclared),
            1 => Ok(VoiceOrigin::Imported {
                format: Codec::dec(r)?,
            }),
            2 => Ok(VoiceOrigin::SystemPromoted {
                winning_operation: Codec::dec(r)?,
                losing_operation: Codec::dec(r)?,
                original_voice: Codec::dec(r)?,
            }),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "VoiceOrigin",
                tag,
            }),
        }
    }
}

impl Codec for StaffGroupKind {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            StaffGroupKind::GrandStaff => out.push(0),
            StaffGroupKind::Bracket => out.push(1),
            StaffGroupKind::SubBracket => out.push(2),
            StaffGroupKind::Choral => out.push(3),
            StaffGroupKind::Registered(id) => {
                out.push(4);
                id.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(StaffGroupKind::GrandStaff),
            1 => Ok(StaffGroupKind::Bracket),
            2 => Ok(StaffGroupKind::SubBracket),
            3 => Ok(StaffGroupKind::Choral),
            4 => Ok(StaffGroupKind::Registered(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "StaffGroupKind",
                tag,
            }),
        }
    }
}

struct_codec!(StaffGroup {
    id,
    name,
    kind,
    members
});

impl Codec for TieClass {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            TieClass::Standard => out.push(0),
            TieClass::Editorial => out.push(1),
            TieClass::CrossVoice => out.push(2),
            TieClass::LaissezVibrer => out.push(3),
            TieClass::Registered(id) => {
                out.push(4);
                id.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(TieClass::Standard),
            1 => Ok(TieClass::Editorial),
            2 => Ok(TieClass::CrossVoice),
            3 => Ok(TieClass::LaissezVibrer),
            4 => Ok(TieClass::Registered(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "TieClass",
                tag,
            }),
        }
    }
}

struct_codec!(Tie {
    id,
    start_event,
    end_event,
    pitch_pairing,
    class
});

impl Codec for AnnotationAnchor {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            AnnotationAnchor::Event(id) => {
                out.push(0);
                id.enc(out);
            }
            AnnotationAnchor::Range { start, end } => {
                out.push(1);
                start.enc(out);
                end.enc(out);
            }
            AnnotationAnchor::Region(id) => {
                out.push(2);
                id.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(AnnotationAnchor::Event(Codec::dec(r)?)),
            1 => Ok(AnnotationAnchor::Range {
                start: Codec::dec(r)?,
                end: Codec::dec(r)?,
            }),
            2 => Ok(AnnotationAnchor::Region(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "AnnotationAnchor",
                tag,
            }),
        }
    }
}

impl Codec for GestureAnchoring {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            GestureAnchoring::Events(v) => {
                out.push(0);
                v.enc(out);
            }
            GestureAnchoring::Range { start, end, staves } => {
                out.push(1);
                start.enc(out);
                end.enc(out);
                staves.enc(out);
            }
            GestureAnchoring::Free => out.push(2),
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(GestureAnchoring::Events(Codec::dec(r)?)),
            1 => Ok(GestureAnchoring::Range {
                start: Codec::dec(r)?,
                end: Codec::dec(r)?,
                staves: Codec::dec(r)?,
            }),
            2 => Ok(GestureAnchoring::Free),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "GestureAnchoring",
                tag,
            }),
        }
    }
}

impl Codec for DecompositionSource {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            DecompositionSource::UserChosen => out.push(0),
            DecompositionSource::Inferred => out.push(1),
            DecompositionSource::Imported { format } => {
                out.push(2);
                format.enc(out);
            }
            DecompositionSource::Propagated { from } => {
                out.push(3);
                from.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(DecompositionSource::UserChosen),
            1 => Ok(DecompositionSource::Inferred),
            2 => Ok(DecompositionSource::Imported {
                format: Codec::dec(r)?,
            }),
            3 => Ok(DecompositionSource::Propagated {
                from: Codec::dec(r)?,
            }),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "DecompositionSource",
                tag,
            }),
        }
    }
}

struct_codec!(DecompositionAttachment {
    target,
    components,
    source
});

impl Codec for TimeSignatureDisplay {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            TimeSignatureDisplay::Standard {
                numerator,
                denominator,
            } => {
                out.push(0);
                numerator.enc(out);
                denominator.enc(out);
            }
            TimeSignatureDisplay::Compound {
                numerators,
                denominator,
            } => {
                out.push(1);
                numerators.enc(out);
                denominator.enc(out);
            }
            TimeSignatureDisplay::Irrational {
                numerator,
                denominator,
            } => {
                out.push(2);
                numerator.enc(out);
                denominator.enc(out);
            }
            TimeSignatureDisplay::MixedDenominators { components } => {
                out.push(3);
                components.enc(out);
            }
            TimeSignatureDisplay::None => out.push(4),
            TimeSignatureDisplay::Symbolic(v) => {
                out.push(5);
                v.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(TimeSignatureDisplay::Standard {
                numerator: Codec::dec(r)?,
                denominator: Codec::dec(r)?,
            }),
            1 => Ok(TimeSignatureDisplay::Compound {
                numerators: Codec::dec(r)?,
                denominator: Codec::dec(r)?,
            }),
            2 => Ok(TimeSignatureDisplay::Irrational {
                numerator: Codec::dec(r)?,
                denominator: Codec::dec(r)?,
            }),
            3 => Ok(TimeSignatureDisplay::MixedDenominators {
                components: Codec::dec(r)?,
            }),
            4 => Ok(TimeSignatureDisplay::None),
            5 => Ok(TimeSignatureDisplay::Symbolic(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "TimeSignatureDisplay",
                tag,
            }),
        }
    }
}

impl Codec for TimeSignature {
    fn enc(&self, out: &mut Vec<u8>) {
        self.id.enc(out);
        self.display.enc(out);
        self.measure_duration().enc(out);
        self.beat_groups().to_vec().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        let id = Codec::dec(r)?;
        let display = TimeSignatureDisplay::dec(r)?;
        let measure_duration = MusicalDuration::dec(r)?;
        let beat_groups: Vec<BeatGroup> = Codec::dec(r)?;
        TimeSignature::new(id, display, measure_duration, beat_groups)
            .ok_or(ScoreDecodeError::Reconstruct("TimeSignature"))
    }
}

struct_codec!(Score {
    metadata,
    canvas,
    instruments,
    staves,
    staff_groups,
    parts,
    cross_cutting,
    time_signatures,
    tuning_context,
    tempo_map,
    events,
    spelling_attachments,
    decomposition_attachments,
    spelling_precedence,
    analysis_layers,
    views,
    identity,
    tombstoned_pitches,
    tombstoned_events
});

// ===========================================================================
// Public entry points on Score.
// ===========================================================================

impl Score {
    /// The canonical byte serialization of the whole score graph. Two equal
    /// scores produce identical bytes; the bytes round-trip through
    /// [`Score::decode_canonical`].
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.enc(&mut out);
        out
    }

    /// Decodes the exact inverse of [`Score::canonical_bytes`], validating every
    /// tag, length, primitive, and type invariant. Trailing bytes are rejected.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Score> {
        let mut r = Reader::new(bytes);
        let score = Score::dec(&mut r)?;
        r.finish()?;
        Ok(score)
    }
}

// ===========================================================================
// Public per-value canonical codec (the K↔J seam).
// ===========================================================================

/// A public, whole-buffer canonical byte form for an *individual* value
/// reachable from a [`Score`] — the per-type analogue of
/// [`Score::canonical_bytes`].
///
/// ## Why this exists
///
/// The internal `Codec` trait (and its `Reader`) are crate-private: they are
/// the composition machinery for the whole-score codec, and exposing them would
/// leak the cursor and the combinator surface. But Track B's Operation Catalog
/// (Agent K) needs *value-typed* operation payloads — an `InsertEvent` that
/// carries the real [`Event`], a `RespellPitch` that carries the real
/// [`PitchSpelling`] — and those payloads must serialize canonically so an
/// operation envelope stays hashable across an implementation boundary.
///
/// `CanonicalValue` is that agreed surface: a thin, public, per-type
/// `canonical_bytes`/`decode_canonical` pair that **delegates to the existing,
/// ratified `Codec` byte layout** (Pass 11 item 1.8,
/// `req:format:codec-conventions`). It introduces *no new byte layout* — the
/// bytes are byte-for-byte the same ones the whole-score codec already emits for
/// these values, merely made reachable for an individual value. The Binary
/// Format companion (Agent J) documents this surface as normative; until then it
/// inherits core's conventions exactly, so core/ops stay consistent. See
/// `DECISIONS.md` (P11-4 / the K↔J seam).
///
/// Implemented only for the value types operation payloads embed, not for every
/// `Codec` type, to keep the public surface intentional.
pub trait CanonicalValue: Sized {
    /// The canonical bytes of this single value, using the same layout the
    /// whole-score codec uses for it.
    fn canonical_bytes(&self) -> Vec<u8>;

    /// The exact inverse of [`CanonicalValue::canonical_bytes`]. Validates every
    /// tag, length, primitive, and invariant; rejects trailing bytes.
    fn decode_canonical(bytes: &[u8]) -> core::result::Result<Self, ScoreDecodeError>;
}

/// Implements [`CanonicalValue`] for value types that already have a [`Codec`],
/// by delegating to it. No new byte layout is introduced.
macro_rules! canonical_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl CanonicalValue for $ty {
                fn canonical_bytes(&self) -> Vec<u8> {
                    let mut out = Vec::new();
                    Codec::enc(self, &mut out);
                    out
                }
                fn decode_canonical(bytes: &[u8]) -> Result<Self> {
                    let mut r = Reader::new(bytes);
                    let v = <$ty as Codec>::dec(&mut r)?;
                    r.finish()?;
                    Ok(v)
                }
            }
        )*
    };
}

canonical_value! {
    Event,
    Rest,
    Pitch,
    IdentifiedPitch,
    PitchSpelling,
    Tie,
    Slur,
    Beam,
    Spanner,
    RegionTimeModel,
    TimeAnchor,
}

#[cfg(test)]
mod value_codec_tests {
    use super::*;
    use crate::generators::{valid_score, valid_score_rich};

    /// Every `CanonicalValue` round-trips, and its per-value bytes are exactly
    /// the bytes the whole-score codec embeds for that value (no new layout).
    fn assert_value_round_trips<T>(v: &T)
    where
        T: CanonicalValue + Codec + PartialEq + core::fmt::Debug,
    {
        let bytes = v.canonical_bytes();
        // Per-value bytes equal what the internal Codec emits (the embedded form).
        let mut embedded = Vec::new();
        Codec::enc(v, &mut embedded);
        assert_eq!(bytes, embedded, "CanonicalValue diverged from Codec layout");
        let decoded = T::decode_canonical(&bytes).expect("value decodes");
        assert_eq!(&decoded, v, "value round-trip changed the value");
        assert_eq!(
            decoded.canonical_bytes(),
            bytes,
            "value re-encode not byte-identical"
        );
    }

    #[test]
    fn value_types_round_trip_over_generator_corpus() {
        for seed in 0..64u64 {
            let s = valid_score_rich(seed.wrapping_mul(0x0100_0193).wrapping_add(7));
            for region in &s.canvas.regions {
                assert_value_round_trips(&region.time_model);
            }
            for tie in &s.cross_cutting.ties {
                assert_value_round_trips(tie);
            }
            for slur in &s.cross_cutting.slurs {
                assert_value_round_trips(slur);
            }
            for beam in &s.cross_cutting.beams {
                assert_value_round_trips(beam);
            }
            for spanner in &s.cross_cutting.spanners {
                assert_value_round_trips(spanner);
            }
        }
        for seed in 0..200u64 {
            let s = valid_score(seed.wrapping_mul(0x9E37_79B9).wrapping_add(1));
            for ev in s.events.iter() {
                assert_value_round_trips(ev);
                let mut ips = Vec::new();
                ev.collect_identified_pitches(&mut ips);
                for ip in ips {
                    assert_value_round_trips(ip);
                    assert_value_round_trips(&ip.pitch);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{IndeterminateEvent, Rest, UnpitchedEvent};
    use crate::generators::{valid_score, valid_score_rich};
    use crate::ids::EventId;
    use crate::time::{EventDuration, EventPosition};

    /// encode → decode → equal, and re-encode byte-identical.
    fn assert_round_trips(score: &Score) {
        let bytes = score.canonical_bytes();
        let decoded = Score::decode_canonical(&bytes).expect("score decodes");
        assert_eq!(&decoded, score, "round-trip changed the score");
        assert_eq!(
            decoded.canonical_bytes(),
            bytes,
            "re-encode not byte-identical"
        );
    }

    #[test]
    fn generator_scores_round_trip() {
        for seed in 0..200u64 {
            assert_round_trips(&valid_score(seed.wrapping_mul(0x9E37_79B9).wrapping_add(1)));
        }
        for seed in 0..64u64 {
            assert_round_trips(&valid_score_rich(
                seed.wrapping_mul(0x0100_0193).wrapping_add(7),
            ));
        }
    }

    #[test]
    fn distinct_scores_serialize_differently() {
        assert_ne!(
            valid_score(1).canonical_bytes(),
            valid_score(2).canonical_bytes(),
            "distinct scores must produce distinct canonical bytes"
        );
        // The rich generator differs from the simple one, too.
        assert_ne!(
            valid_score(3).canonical_bytes(),
            valid_score_rich(3).canonical_bytes()
        );
    }

    #[test]
    fn degenerate_tuplet_ratio_is_rejected_on_decode() {
        // Guards the TupletRatio::dec re-validation (Pass 11 item 3.5): a
        // hand-crafted byte stream must not be able to inject a degenerate ratio
        // that TupletRatio::new would reject at construction. Without the
        // `.ok_or(Reconstruct)` in dec, these would decode into an
        // unconstructible-by-API value.
        let decode = |actual: u32, notated: u32| {
            let mut bytes = Vec::new();
            actual.enc(&mut bytes);
            notated.enc(&mut bytes);
            TupletRatio::dec(&mut Reader::new(&bytes))
        };
        for (a, n) in [(0u32, 0u32), (2, 0), (0, 2), (4, 4)] {
            assert!(
                matches!(decode(a, n), Err(ScoreDecodeError::Reconstruct(_))),
                "degenerate ratio {a}:{n} must be rejected on decode"
            );
        }
        // A well-formed ratio still decodes.
        let ok = decode(3, 2).expect("non-degenerate ratio decodes");
        assert_eq!((ok.actual(), ok.notated()), (3, 2));
    }

    #[test]
    fn exotic_event_and_pitch_variants_round_trip() {
        // Round-trip is structural, so this need not satisfy graph invariants —
        // it exists to exercise every event/pitch variant the generators omit.
        let mut score = valid_score(0xE0E0);
        let voice = score.canvas.regions[0].staff_instances()[0].voices[0].id;
        let replica = score.identity.replica_id;
        let mut next = 9_000u64;
        let mut mint = || {
            let id = EventId::new(replica, next);
            next += 1;
            id
        };
        let pos = EventPosition::Musical(crate::time::MusicalPosition(
            crate::time::RationalTime::from_int(40),
        ));
        let dur = EventDuration::Musical(crate::time::MusicalDuration(
            crate::time::RationalTime::from_int(1),
        ));

        score
            .events
            .insert(Event::Rest(Rest {
                id: mint(),
                voice,
                position: pos.clone(),
                duration: dur.clone(),
                vertical_position: Some(crate::event::StaffPosition(-3)),
                visible: false,
            }))
            .expect("rest inserts");
        score
            .events
            .insert(Event::Unpitched(UnpitchedEvent {
                id: mint(),
                voice,
                position: pos.clone(),
                duration: dur.clone(),
                staff_position: crate::event::StaffPosition(2),
                instrument_member: crate::event::UnpitchedMemberId(7),
                articulations: vec![crate::event::ArticulationMark],
                dynamic: Some(crate::event::DynamicMark),
                stem: crate::event::StemConfiguration,
                grace: Some(GraceKind::MeasuredFraction(crate::time::MusicalDuration(
                    crate::time::RationalTime::new(1, 8).unwrap(),
                ))),
            }))
            .expect("unpitched inserts");
        score
            .events
            .insert(Event::Indeterminate(IndeterminateEvent {
                id: mint(),
                voice,
                position: pos,
                duration: EventDuration::Indeterminate(crate::time::DurationBounds {
                    lower: Some(crate::time::ConcreteDuration::Musical(
                        crate::time::MusicalDuration(crate::time::RationalTime::from_int(1)),
                    )),
                    upper: None,
                }),
                indeterminacy: IndeterminacyKind::Compound(vec![
                    IndeterminacyKind::Pitch,
                    IndeterminacyKind::Duration,
                ]),
                hints: IndeterminacyHints {
                    duration_bounds: None,
                    alternatives: vec![EventId::new(replica, 1)],
                    textual_instruction: Some("ad lib.".to_owned()),
                },
            }))
            .expect("indeterminate inserts");

        // A spelling attachment exercising the pitch-spelling tree.
        score.spelling_attachments.push(SpellingAttachment {
            scope: SpellingScope::Pitch(crate::ids::PitchId::new(replica, 1)),
            directive: SpellingDirective::Explicit(PitchSpelling {
                nominal: SpellingNominal::Cmn(CmnNominal::F),
                accidentals: vec![AccidentalId::new("sharp")],
                octave: 4,
                render_hints: SpellingRenderHints {
                    parenthesized: true,
                    cautionary: false,
                    editorial: true,
                    small_print: false,
                },
            }),
            source: SpellingSource::Imported {
                format: ForeignFormatId::new("musicxml"),
            },
            priority: -2,
            layer: None,
        });

        // Exotic metadata strings (preserved byte-exactly).
        score.metadata.title = Some("Étude café".to_owned());
        score.metadata.composer = Some("composer".to_owned());

        assert_round_trips(&score);
    }

    #[test]
    fn trailing_and_truncated_bytes_are_rejected() {
        let bytes = valid_score(7).canonical_bytes();

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            Score::decode_canonical(&trailing),
            Err(ScoreDecodeError::TrailingBytes)
        );

        let truncated = &bytes[..bytes.len() - 1];
        assert!(
            Score::decode_canonical(truncated).is_err(),
            "a truncated score must be rejected"
        );

        // An empty buffer is not a score.
        assert!(Score::decode_canonical(&[]).is_err());
    }
}
