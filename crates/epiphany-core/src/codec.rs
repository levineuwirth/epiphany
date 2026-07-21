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
    AnnotationAnchor, BarlineAlignmentGroup, BarlineAlignmentMember, Beam, BeamGeometryOverride,
    BeatGroup, BracketKind, Canvas, CanvasLayoutDefaults, CanvasMargins, CanvasSize, ChordSymbol,
    Clef, ClefChange, ClefShape, Comment, CrossCuttingRegistry, CurvatureOverride, CurveDirection,
    DecompositionAttachment, DecompositionSource, EventOrderingDAG, GestureAnchoring,
    GraphicContent, GraphicGesture, GraphicObject, HairpinDirection, Instrument, KeySignature,
    KeySignatureChange, LineStyle, LyricLine, Marker, Measure, MeasureNumberVisibility,
    MetadataEntry, MetadataValue, MeterChange, MetricGrid, MetricTimeModel, NotatedComponent,
    NoteValue, OctaveOffset, PartDefinition, PedalKind, PowerOfTwo, ProportionalTimeModel, Region,
    RegionContent, RegionTimeModel, RepeatKind, RepeatStructure, Score, ScoreMetadata,
    ScoreTuningContext, Slur, SlurKind, SoundConfiguration, SpaceUnit, SpanStyle, Spanner,
    SpannerKind, Staff, StaffBasedContent, StaffBracketKind, StaffExtent, StaffGroup,
    StaffGroupKind, StaffInstance, StaffLineConfiguration, StemDirection, SubBeam,
    TempoMapReference, TextLineDefinition, Tie, TieClass, TimeExtent, TimeSignature,
    TimeSignatureDisplay, Timestamp, Tuplet, TupletRatio, UnpitchedMember, ViewDefinition, Voice,
    VoiceOrigin, Volta,
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
    NominalRegistryId, Pitch, PitchRange, PitchSpaceId, PitchSpacePosition, PitchSpelling,
    PositionRegistryId, ReferencePitch, ScalePosition, SpellingAttachment, SpellingDirective,
    SpellingNominal, SpellingPrecedence, SpellingRenderHints, SpellingRule, SpellingRuleSetId,
    SpellingScope, SpellingSource, SpellingSourceKind, StaffGroupKindRegistryId,
    TieClassRegistryId, TranspositionInterval, TuningReference, TuningSystemId, VoiceSelector,
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
        let n = usize::try_from(self.u32()?).map_err(|_| ScoreDecodeError::LengthOverflow)?;
        // A count/length prefix can never exceed the bytes remaining, in both of
        // its uses: a length-prefixed leaf needs exactly `n` further bytes
        // (`lp`), and a collection prefix counts elements, each of which is at
        // least one byte in this codec, so `n` elements need at least `n` bytes.
        // (An empty leaf or collection has `n == 0`, which passes.) A larger `n`
        // is corrupt or adversarial: reject it up front rather than looping
        // element-by-element toward EOF — that bounds decode time on hostile
        // input (a garbage `u32` count is otherwise a soft-DoS: e.g. misparsing
        // v1 bytes as v0 reads a huge count and iterates ~remaining times) and
        // caps the `Vec`/set allocation to a real size.
        if n > self.bytes.len() - self.pos {
            return Err(ScoreDecodeError::UnexpectedEof);
        }
        Ok(n)
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

impl Codec for i64 {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(r.u64()? as i64)
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

        impl crate::textvalue::TextValue for $ty {
            fn project(&self) -> crate::textvalue::Sexp {
                let $ty { $($field),* } = self;
                crate::textvalue::Sexp::List(vec![
                    crate::textvalue::Sexp::Symbol(crate::textvalue::kebab(stringify!($ty))),
                    $( crate::textvalue::TextValue::project($field), )*
                ])
            }
            fn parse(
                s: &crate::textvalue::Sexp,
            ) -> core::result::Result<Self, crate::textvalue::TextError> {
                const ARITY: usize = [$(stringify!($field)),*].len();
                let fields = s.expect_struct(&crate::textvalue::kebab(stringify!($ty)), ARITY)?;
                let mut next = fields.iter();
                $( let $field = crate::textvalue::TextValue::parse(
                    next.next().expect("arity checked by expect_struct"))?; )*
                Ok($ty { $($field),* })
            }
        }
    };
}

/// [`Codec`] for a zero-field unit struct: no bytes. And its [`TextValue`]: the
/// bare symbol, as a fieldless variant is. It encodes to no bytes and carries no
/// value in the text either (`req:textproj:value-projection` clause 1).
///
/// [`TextValue`]: crate::textvalue::TextValue
macro_rules! unit_codec {
    ($($ty:ident),* $(,)?) => {
        $(
            impl Codec for $ty {
                fn enc(&self, _out: &mut Vec<u8>) {}
                fn dec(_r: &mut Reader<'_>) -> Result<Self> {
                    Ok($ty)
                }
            }

            impl crate::textvalue::TextValue for $ty {
                fn project(&self) -> crate::textvalue::Sexp {
                    crate::textvalue_impls::project_unit(stringify!($ty))
                }
                fn parse(
                    s: &crate::textvalue::Sexp,
                ) -> core::result::Result<Self, crate::textvalue::TextError> {
                    crate::textvalue_impls::parse_unit(s, stringify!($ty)).map(|()| $ty)
                }
            }
        )*
    };
}

/// [`Codec`] for a fieldless ("C-like") enum: a single discriminant byte. And its
/// [`TextValue`]: the variant name as a bare symbol.
///
/// Both `match`es are exhaustive over the enum, so a variant added to the type and
/// not to this invocation **fails to compile** — the guarantee
/// `operation_kind_tag_vocabulary!` gives the operation decoder, here for every
/// C-like enum in Chapter 5.
///
/// [`TextValue`]: crate::textvalue::TextValue
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

        impl crate::textvalue::TextValue for $ty {
            fn project(&self) -> crate::textvalue::Sexp {
                let variant = match self { $( $ty::$variant => stringify!($variant), )* };
                crate::textvalue::Sexp::Symbol(crate::textvalue::kebab(variant))
            }
            fn parse(
                s: &crate::textvalue::Sexp,
            ) -> core::result::Result<Self, crate::textvalue::TextError> {
                let name = s.as_symbol().ok_or(crate::textvalue::TextError::Expected {
                    expected: "symbol",
                    found: crate::textvalue_impls::class_of(s),
                })?;
                $(
                    if name == crate::textvalue::kebab(stringify!($variant)) {
                        return Ok($ty::$variant);
                    }
                )*
                Err(crate::textvalue::TextError::UnknownConstructor {
                    type_name: stringify!($ty),
                    found: name.to_owned(),
                })
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

            // A catalog id is canonical text, so it projects as a quoted string.
            // `new` folds to NFC, so `parse` interns and then *compares*: returning
            // the folded value would accept a non-NFC spelling and silently
            // normalize it (`req:textproj:strict-parse`).
            impl crate::textvalue::TextValue for $ty {
                fn project(&self) -> crate::textvalue::Sexp {
                    crate::textvalue::Sexp::Str(self.as_str().to_owned())
                }
                fn parse(
                    s: &crate::textvalue::Sexp,
                ) -> core::result::Result<Self, crate::textvalue::TextError> {
                    crate::textvalue_impls::parse_catalog_id(s, $ty::new, |v| v.as_str())
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
// Schema major 1: `Instrument.range` embeds this (appended after `name`).
struct_codec!(PitchRange { lowest, highest });
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

// --- Schema-major-2 leaf types (Binary Format §Schema Major 2). -------------
// Every layout below is the ratified v2 wire form: tag-only enums are one
// discriminant byte; `SpaceUnit` rides its `CanonicalF64` leaf (framed 12);
// `Timestamp` is one bare i64 LE; fixed-width primitives inside structs are
// bare; identifiers keep the leaf framing.

impl Codec for Timestamp {
    fn enc(&self, out: &mut Vec<u8>) {
        self.0.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Timestamp(i64::dec(r)?))
    }
}

impl Codec for SpaceUnit {
    fn enc(&self, out: &mut Vec<u8>) {
        self.0.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(SpaceUnit(Codec::dec(r)?))
    }
}

impl Codec for SoundConfiguration {
    fn enc(&self, out: &mut Vec<u8>) {
        self.0.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(SoundConfiguration(Codec::dec(r)?))
    }
}

impl Codec for OctaveOffset {
    fn enc(&self, out: &mut Vec<u8>) {
        self.0.enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        Ok(OctaveOffset(i8::dec(r)?))
    }
}

cstyle_enum_codec!(LineStyle { 0 => Solid, 1 => Dashed, 2 => Dotted });
cstyle_enum_codec!(SlurKind {
    0 => Legato,
    1 => Phrase,
    2 => Articulation,
    3 => Editorial,
});
cstyle_enum_codec!(CurveDirection { 0 => Above, 1 => Below });
cstyle_enum_codec!(HairpinDirection {
    0 => Crescendo,
    1 => Diminuendo,
});
cstyle_enum_codec!(PedalKind {
    0 => Sustain,
    1 => Sostenuto,
    2 => UnaCorda,
});
cstyle_enum_codec!(BracketKind { 0 => Square });
cstyle_enum_codec!(StaffBracketKind { 0 => Brace, 1 => Bracket });

impl Codec for SpannerKind {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            SpannerKind::Generic => out.push(0),
            SpannerKind::Hairpin(d) => {
                out.push(1);
                d.enc(out);
            }
            SpannerKind::OctaveLine(o) => {
                out.push(2);
                o.enc(out);
            }
            SpannerKind::PedalLine(p) => {
                out.push(3);
                p.enc(out);
            }
            SpannerKind::TrillExtension => out.push(4),
            SpannerKind::Glissando => out.push(5),
            SpannerKind::Portamento => out.push(6),
            SpannerKind::TextLine(t) => {
                out.push(7);
                t.enc(out);
            }
            SpannerKind::Bracket(b) => {
                out.push(8);
                b.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(SpannerKind::Generic),
            1 => Ok(SpannerKind::Hairpin(Codec::dec(r)?)),
            2 => Ok(SpannerKind::OctaveLine(Codec::dec(r)?)),
            3 => Ok(SpannerKind::PedalLine(Codec::dec(r)?)),
            4 => Ok(SpannerKind::TrillExtension),
            5 => Ok(SpannerKind::Glissando),
            6 => Ok(SpannerKind::Portamento),
            7 => Ok(SpannerKind::TextLine(Codec::dec(r)?)),
            8 => Ok(SpannerKind::Bracket(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "SpannerKind",
                tag,
            }),
        }
    }
}

impl Codec for RepeatKind {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            RepeatKind::SimpleRepeat { count } => {
                out.push(0);
                count.enc(out);
            }
            RepeatKind::DaCapo { end_target } => {
                out.push(1);
                end_target.enc(out);
            }
            RepeatKind::DalSegno { segno, end_target } => {
                out.push(2);
                segno.enc(out);
                end_target.enc(out);
            }
            RepeatKind::Volta => out.push(3),
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(RepeatKind::SimpleRepeat {
                count: Codec::dec(r)?,
            }),
            1 => Ok(RepeatKind::DaCapo {
                end_target: Codec::dec(r)?,
            }),
            2 => Ok(RepeatKind::DalSegno {
                segno: Codec::dec(r)?,
                end_target: Codec::dec(r)?,
            }),
            3 => Ok(RepeatKind::Volta),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "RepeatKind",
                tag,
            }),
        }
    }
}

impl Codec for MetadataValue {
    fn enc(&self, out: &mut Vec<u8>) {
        match self {
            MetadataValue::Text(s) => {
                out.push(0);
                s.enc(out);
            }
            MetadataValue::Integer(i) => {
                out.push(1);
                i.enc(out);
            }
            MetadataValue::Flag(b) => {
                out.push(2);
                b.enc(out);
            }
        }
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(MetadataValue::Text(Codec::dec(r)?)),
            1 => Ok(MetadataValue::Integer(Codec::dec(r)?)),
            2 => Ok(MetadataValue::Flag(Codec::dec(r)?)),
            tag => Err(ScoreDecodeError::InvalidTag {
                kind: "MetadataValue",
                tag,
            }),
        }
    }
}

struct_codec!(CurvatureOverride { direction, height });
struct_codec!(SpanStyle { line, thickness });
struct_codec!(SubBeam { level, events });
struct_codec!(BeamGeometryOverride { slope, offset });
struct_codec!(TextLineDefinition { text });
struct_codec!(Volta {
    endings,
    start,
    end
});
struct_codec!(MetadataEntry { key, value });
struct_codec!(TranspositionInterval {
    diatonic_steps,
    chromatic_steps
});
struct_codec!(UnpitchedMember {
    member,
    name,
    staff_position
});

// Schema major 2: `ScoreMetadata` appended six fields after the major-0/1
// order. The frozen prior layout (`title`, `composer`, `copyright`) is read
// by `dec_metadata_v1`.
struct_codec!(ScoreMetadata {
    title,
    composer,
    copyright,
    subtitle,
    lyricist,
    arranger,
    creation_timestamp,
    modification_timestamp,
    additional
});
// Schema major 1: `Instrument` gained `range` (appended after `name`); the
// frozen major-0 layout (`id`, `name`) is read by `dec_instruments_v0`.
// Schema major 2 appended six more fields after `range`; the frozen major-1
// layout (`id`, `name`, `range`) is read by `dec_instruments_v1`.
struct_codec!(Instrument {
    id,
    name,
    range,
    abbreviation,
    sound_config,
    transposition,
    default_clef,
    default_staff_lines,
    unpitched_members
});
// Schema major 2: three fields appended after `line_count`; the frozen prior
// layout (`line_count` only) is read by `dec_staff_lines_v1`.
struct_codec!(StaffLineConfiguration {
    line_count,
    line_spacing,
    line_style,
    bracket
});
struct_codec!(GraphicObject { id });
struct_codec!(GraphicContent { objects });
struct_codec!(TimeExtent { start, end });
struct_codec!(StaffExtent { staves });
// Schema major 2: `default_clef` appended last; the frozen prior layout is
// read by `dec_staff_v1`.
struct_codec!(Staff {
    id,
    name,
    abbreviation,
    instrument,
    default_staff_lines,
    group,
    default_clef
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
cstyle_enum_codec!(ClefShape {
    0 => G,
    1 => F,
    2 => C,
    3 => Percussion,
});
struct_codec!(Clef {
    shape,
    line,
    octave_shift
});
impl Codec for KeySignature {
    fn enc(&self, out: &mut Vec<u8>) {
        self.fifths().enc(out);
    }
    fn dec(r: &mut Reader<'_>) -> Result<Self> {
        KeySignature::new(i8::dec(r)?).ok_or(ScoreDecodeError::Reconstruct("KeySignature"))
    }
}
struct_codec!(ClefChange { anchor, clef });
struct_codec!(KeySignatureChange { anchor, key });
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
// Schema major 2: the cross-cutting bodies filled (appended fields); the
// frozen prior layouts are read by the `dec_*_v1` sub-decoders.
struct_codec!(Slur {
    id,
    start_event,
    end_event,
    kind,
    curvature_override,
    style
});
struct_codec!(Beam {
    id,
    events,
    level,
    sub_beams,
    geometry_override
});
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
    staves,
    kind,
    style
});
struct_codec!(Marker { id, anchor });
struct_codec!(RepeatStructure {
    id,
    start,
    end,
    kind,
    voltas
});
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
// Schema major 1: `Region` gained `permits_spanning_slurs` (appended after
// `local_tempo_map`). The frozen major-0 layout (the six fields before it) is
// read by `dec_region_v0`. `Region` is also a `CanonicalValue` embedded in the
// canonical `CreateRegion` op payload, so this byte change is canonical — the
// op-block migration for it is a separate change (schema-major track, D2).
struct_codec!(Region {
    id,
    time_model,
    content,
    time_extent,
    staff_extent,
    local_tempo_map,
    permits_spanning_slurs
});
struct_codec!(CanvasSize { width, height });
struct_codec!(CanvasMargins {
    top,
    right,
    bottom,
    left
});
struct_codec!(CanvasLayoutDefaults { page_size, margins });
// Schema major 1: `Canvas` gained `layout_defaults` (appended after `regions`).
// The frozen major-0 layout (`regions` only) is read by `decode_v0_score`.
struct_codec!(Canvas {
    regions,
    layout_defaults
});
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
    class,
    style
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
    ///
    /// This is the **current (schema major 2)** layout. To decode bytes whose
    /// schema major is not known to be current, use
    /// [`Score::decode_canonical_versioned`].
    ///
    /// Decoding is **strictly canonical**: an accepted byte string is its own
    /// canonical form (`req:format:codec-conventions`). Several leaf and
    /// collection decoders are individually *lenient* — they normalize on decode
    /// (an unreduced [`RationalTime`](crate::RationalTime) reduces to lowest
    /// terms; a `BTreeSet`/`BTreeMap` re-sorts and de-duplicates; a
    /// [`CanonicalF64`] / `ReferencePitch` / `Tempo` normalizes via its
    /// constructor) — so decode alone is not injective. This entry point closes
    /// that gap uniformly: it re-encodes the decoded score and rejects any input
    /// that is not already canonical, so two byte strings can never map to one
    /// score (which would break content-addressing). A valid encoder never emits
    /// a non-canonical score, so this only rejects corrupted or adversarial bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Score> {
        let mut r = Reader::new(bytes);
        let score = Score::dec(&mut r)?;
        r.finish()?;
        if score.canonical_bytes() != bytes {
            return Err(ScoreDecodeError::InvalidValue(
                "non-canonical Score encoding",
            ));
        }
        Ok(score)
    }

    /// The **schema-version dispatch seam** (Binary Format companion
    /// §"Schema Major 1" / §"Schema Major 2"): decodes a full-`Score` snapshot
    /// whose bytes were written under the given schema `major`, migrating a
    /// lower-major encoding up to the current in-memory form on read. Major 2
    /// is the current layout ([`Score::decode_canonical`]); majors 1 and 0 are
    /// decoded through their frozen wire forms (`decode_v1_score`,
    /// `decode_v0_score`), each a total default-filling migration — the
    /// composed v0→v1→v2 translation happens in the one v0 read.
    ///
    /// The caller (the bundle read path) only reaches this after the chunk gate
    /// has admitted the major into its accept-set, so a major outside
    /// `{0, 1, 2}` is a defensive error, not an expected path.
    pub fn decode_canonical_versioned(bytes: &[u8], major: u16) -> Result<Score> {
        match major {
            2 => Score::decode_canonical(bytes),
            1 => decode_v1_score(bytes),
            0 => decode_v0_score(bytes),
            _ => Err(ScoreDecodeError::InvalidValue("unsupported schema major")),
        }
    }
}

/// Decodes **schema-major-0** `Score` bytes into the current-layout `Score`,
/// migrating on read.
///
/// This is the **frozen v0 wire form, decoded by value** — a hand-written walk
/// of the 19 `Score` fields in declaration order, using the current [`Codec`]
/// for every field whose layout is unchanged, the frozen **v1** sub-decoders
/// (v0 == v1) for the types schema major 2 filled, and a frozen v0
/// sub-decoder for the three that grew a field in schema major 1:
///
/// * `Canvas` (field 2) — v0 was `regions` only; v1 appended `layout_defaults`.
///   [`dec_canvas_v0`] reads the region vector and default-fills the defaults.
/// * `Instrument` (inside field 3, `instruments: Vec<Instrument>`) — v0 was
///   `{ id, name }`; v1 appended `range`. [`dec_instruments_v0`] default-fills
///   `range: None`.
/// * `Region` (inside `Canvas.regions`) — v0 was the six fields before
///   `permits_spanning_slurs`; v1 appended it. [`dec_region_v0`] default-fills
///   `false`.
///
/// A byte-splice sufficed while only `Canvas` (a single top-level field) had
/// changed, but two of the three grown structs are nested inside `Vec`s, so
/// there is no single splice point — the walk must reconstruct each element.
/// The migration is **total and default-filling**: no score context is needed,
/// and every new field takes its canonical default. It is frozen by value — it
/// depends only on the v0 field lists above and the current codec for unchanged
/// fields; the `v0_score_migrates_*` golden tests guard it (they synthesize real
/// v0 bytes via a mirror v0 encoder and check the migration reconstructs the
/// original score with the new fields at their defaults).
fn decode_v0_score(bytes: &[u8]) -> Result<Score> {
    let mut r = Reader::new(bytes);
    // The 19 Score fields in declaration order (codec.rs `struct_codec!(Score)`).
    // `canvas` (2) and `instruments` (3) carry v0-specific sub-forms; the
    // schema-major-2 fills route `metadata`, `staves`, `cross_cutting`, and
    // (transitively, inside regions) the staff instances through the frozen
    // v1 sub-decoders — v0 == v1 for every type major 2 changed.
    let metadata = dec_metadata_v1(&mut r)?;
    let canvas = dec_canvas_v0(&mut r)?;
    let instruments = dec_instruments_v0(&mut r)?;
    let staves = dec_staves_v1(&mut r)?;
    let staff_groups = Codec::dec(&mut r)?;
    let parts = Codec::dec(&mut r)?;
    let cross_cutting = dec_ccr_v1(&mut r)?;
    let time_signatures = Codec::dec(&mut r)?;
    let tuning_context = Codec::dec(&mut r)?;
    let tempo_map = Codec::dec(&mut r)?;
    let events = Codec::dec(&mut r)?;
    let spelling_attachments = Codec::dec(&mut r)?;
    let decomposition_attachments = Codec::dec(&mut r)?;
    let spelling_precedence = Codec::dec(&mut r)?;
    let analysis_layers = Codec::dec(&mut r)?;
    let views = Codec::dec(&mut r)?;
    let identity = Codec::dec(&mut r)?;
    let tombstoned_pitches = Codec::dec(&mut r)?;
    let tombstoned_events = Codec::dec(&mut r)?;
    r.finish()?;
    let score = Score {
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
        tombstoned_events,
    };
    // Strictly canonical on the **v0 wire form**, exactly as
    // `Score::decode_canonical` is for v1: the frozen field walk above decodes
    // leniently (the unchanged fields normalize on decode — an unreduced
    // RationalTime, an unsorted set, …), so re-encode the migrated score to the
    // frozen v0 form and reject any input that is not already its canonical v0
    // encoding. This keeps the migration injective over v0 snapshots too (a
    // major-0 chunk is content-addressed just like a major-1 one). A valid v0
    // writer never emitted a non-canonical snapshot.
    if encode_v0_score(&score) != bytes {
        return Err(ScoreDecodeError::InvalidValue(
            "non-canonical v0 Score encoding",
        ));
    }
    Ok(score)
}

/// The **frozen schema-major-0** encoding of a score — the byte-exact inverse of
/// [`decode_v0_score`]'s field walk: `Canvas` without `layout_defaults`,
/// `Instrument` without `range`, `Region` without `permits_spanning_slurs`;
/// every other field through the current [`Codec`]. Used by `decode_v0_score`
/// to enforce strict v0 canonicality (and by the migration tests to synthesize
/// genuine v0 bytes). A migrated score's schema-major-1 fields hold their
/// defaults, so this simply omits them.
pub(crate) fn encode_v0_score(s: &Score) -> Vec<u8> {
    fn enc_region_v0(reg: &Region, out: &mut Vec<u8>) {
        reg.id.enc(out);
        reg.time_model.enc(out);
        // Content through the frozen v1 (== v0) sub-form: its staff
        // instances embed the pre-major-2 StaffLineConfiguration.
        enc_content_v1(&reg.content, out);
        reg.time_extent.enc(out);
        reg.staff_extent.enc(out);
        reg.local_tempo_map.enc(out);
        // v0: no permits_spanning_slurs.
    }
    let mut out = Vec::new();
    enc_metadata_v1(&s.metadata, &mut out);
    // Canvas v0: `regions` only (no `layout_defaults`).
    put_len(&mut out, s.canvas.regions.len());
    for reg in &s.canvas.regions {
        enc_region_v0(reg, &mut out);
    }
    // Instruments v0: `{ id, name }` only (no `range`).
    put_len(&mut out, s.instruments.len());
    for inst in &s.instruments {
        inst.id.enc(&mut out);
        inst.name.enc(&mut out);
    }
    // Fields 4..19: the schema-major-2-changed ones go through the frozen
    // v1 (== v0) sub-encoders; the rest are unchanged since v0.
    enc_staves_v1(&s.staves, &mut out);
    s.staff_groups.enc(&mut out);
    s.parts.enc(&mut out);
    enc_ccr_v1(&s.cross_cutting, &mut out);
    s.time_signatures.enc(&mut out);
    s.tuning_context.enc(&mut out);
    s.tempo_map.enc(&mut out);
    s.events.enc(&mut out);
    s.spelling_attachments.enc(&mut out);
    s.decomposition_attachments.enc(&mut out);
    s.spelling_precedence.enc(&mut out);
    s.analysis_layers.enc(&mut out);
    s.views.enc(&mut out);
    s.identity.enc(&mut out);
    s.tombstoned_pitches.enc(&mut out);
    s.tombstoned_events.enc(&mut out);
    out
}

/// Frozen v0 decoder for `Canvas`: the v0 layout was **just `regions`** (a
/// `Vec<Region>` in v0 element form), with no `layout_defaults`. Reads the
/// region vector via [`dec_region_v0`] and default-fills the new field.
fn dec_canvas_v0(r: &mut Reader<'_>) -> Result<Canvas> {
    let n = r.count()?;
    let mut regions = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        regions.push(dec_region_v0(r)?);
    }
    Ok(Canvas {
        regions,
        layout_defaults: CanvasLayoutDefaults::default(),
    })
}

/// Frozen v0 decoder for a `Region` element: the six fields before
/// `permits_spanning_slurs`, which is default-filled `false`. (The `Vec`
/// framing is the caller's; this reads one element.)
fn dec_region_v0(r: &mut Reader<'_>) -> Result<Region> {
    let id = Codec::dec(r)?;
    let time_model = Codec::dec(r)?;
    let content = dec_content_v1(r)?;
    let time_extent = Codec::dec(r)?;
    let staff_extent = Codec::dec(r)?;
    let local_tempo_map = Codec::dec(r)?;
    Ok(Region {
        id,
        time_model,
        content,
        time_extent,
        staff_extent,
        local_tempo_map,
        permits_spanning_slurs: false,
    })
}

/// Frozen v0 decoder for `Score.instruments`: v0 `Instrument` was `{ id, name }`
/// (no `range`), which is default-filled `None`. Mirrors the `Vec` framing (a
/// `u32` count then each element).
fn dec_instruments_v0(r: &mut Reader<'_>) -> Result<Vec<Instrument>> {
    let n = r.count()?;
    let mut instruments = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        let id = Codec::dec(r)?;
        let name = Codec::dec(r)?;
        instruments.push(Instrument {
            id,
            name,
            range: None,
            abbreviation: None,
            sound_config: SoundConfiguration::default(),
            transposition: None,
            default_clef: Clef::treble(),
            default_staff_lines: StaffLineConfiguration::default(),
            unpitched_members: Vec::new(),
        });
    }
    Ok(instruments)
}

// ===========================================================================
// Frozen schema-major-1 wire form (Binary Format §Schema Major 2).
// ===========================================================================
//
// Schema major 2 filled nine type bodies (Slur/Tie/Beam/Spanner,
// RepeatStructure, Staff, StaffLineConfiguration, Instrument, ScoreMetadata).
// The v1 (= v0, for all of these — major 1 touched none of them) layouts are
// frozen here as `enc_*_v1`/`dec_*_v1` sub-codecs, shared by
// [`decode_v1_score`]/[`encode_v1_score`] and the v0 pair (whose walk routes
// the transitively-changed fields through these). Each `dec_*_v1`
// default-fills the appended v2 fields per the companion's total migration
// table. The chain is transitive where an Option/Vec hides the embedding:
// Region → RegionContent → StaffBasedContent → StaffInstance →
// `staff_lines_override`.

fn enc_vec_v1<T>(items: &[T], out: &mut Vec<u8>, enc: impl Fn(&T, &mut Vec<u8>)) {
    put_len(out, items.len());
    for item in items {
        enc(item, out);
    }
}

fn dec_vec_v1<T>(r: &mut Reader<'_>, dec: impl Fn(&mut Reader<'_>) -> Result<T>) -> Result<Vec<T>> {
    let n = r.count()?;
    let mut items = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        items.push(dec(r)?);
    }
    Ok(items)
}

fn enc_metadata_v1(m: &ScoreMetadata, out: &mut Vec<u8>) {
    m.title.enc(out);
    m.composer.enc(out);
    m.copyright.enc(out);
}

fn dec_metadata_v1(r: &mut Reader<'_>) -> Result<ScoreMetadata> {
    Ok(ScoreMetadata {
        title: Codec::dec(r)?,
        composer: Codec::dec(r)?,
        copyright: Codec::dec(r)?,
        ..Default::default()
    })
}

fn enc_staff_lines_v1(c: &StaffLineConfiguration, out: &mut Vec<u8>) {
    c.line_count.enc(out);
}

fn dec_staff_lines_v1(r: &mut Reader<'_>) -> Result<StaffLineConfiguration> {
    Ok(StaffLineConfiguration {
        line_count: Codec::dec(r)?,
        ..Default::default()
    })
}

fn enc_staff_v1(s: &Staff, out: &mut Vec<u8>) {
    s.id.enc(out);
    s.name.enc(out);
    s.abbreviation.enc(out);
    s.instrument.enc(out);
    enc_staff_lines_v1(&s.default_staff_lines, out);
    s.group.enc(out);
}

fn dec_staff_v1(r: &mut Reader<'_>) -> Result<Staff> {
    Ok(Staff {
        id: Codec::dec(r)?,
        name: Codec::dec(r)?,
        abbreviation: Codec::dec(r)?,
        instrument: Codec::dec(r)?,
        default_staff_lines: dec_staff_lines_v1(r)?,
        group: Codec::dec(r)?,
        default_clef: Clef::treble(),
    })
}

fn enc_staves_v1(staves: &[Staff], out: &mut Vec<u8>) {
    enc_vec_v1(staves, out, enc_staff_v1);
}

fn dec_staves_v1(r: &mut Reader<'_>) -> Result<Vec<Staff>> {
    dec_vec_v1(r, dec_staff_v1)
}

fn enc_slur_v1(s: &Slur, out: &mut Vec<u8>) {
    s.id.enc(out);
    s.start_event.enc(out);
    s.end_event.enc(out);
}

fn dec_slur_v1(r: &mut Reader<'_>) -> Result<Slur> {
    Ok(Slur {
        id: Codec::dec(r)?,
        start_event: Codec::dec(r)?,
        end_event: Codec::dec(r)?,
        kind: SlurKind::Legato,
        curvature_override: None,
        style: SpanStyle::default(),
    })
}

fn enc_tie_v1(t: &Tie, out: &mut Vec<u8>) {
    t.id.enc(out);
    t.start_event.enc(out);
    t.end_event.enc(out);
    t.pitch_pairing.enc(out);
    t.class.enc(out);
}

fn dec_tie_v1(r: &mut Reader<'_>) -> Result<Tie> {
    Ok(Tie {
        id: Codec::dec(r)?,
        start_event: Codec::dec(r)?,
        end_event: Codec::dec(r)?,
        pitch_pairing: Codec::dec(r)?,
        class: Codec::dec(r)?,
        style: SpanStyle::default(),
    })
}

fn enc_beam_v1(b: &Beam, out: &mut Vec<u8>) {
    b.id.enc(out);
    b.events.enc(out);
    b.level.enc(out);
}

fn dec_beam_v1(r: &mut Reader<'_>) -> Result<Beam> {
    Ok(Beam {
        id: Codec::dec(r)?,
        events: Codec::dec(r)?,
        level: Codec::dec(r)?,
        sub_beams: Vec::new(),
        geometry_override: None,
    })
}

fn enc_spanner_v1(s: &Spanner, out: &mut Vec<u8>) {
    s.id.enc(out);
    s.start.enc(out);
    s.end.enc(out);
    s.staves.enc(out);
}

fn dec_spanner_v1(r: &mut Reader<'_>) -> Result<Spanner> {
    Ok(Spanner {
        id: Codec::dec(r)?,
        start: Codec::dec(r)?,
        end: Codec::dec(r)?,
        staves: Codec::dec(r)?,
        kind: SpannerKind::Generic,
        style: SpanStyle::default(),
    })
}

fn enc_repeat_v1(rep: &RepeatStructure, out: &mut Vec<u8>) {
    rep.id.enc(out);
    rep.start.enc(out);
    rep.end.enc(out);
}

fn dec_repeat_v1(r: &mut Reader<'_>) -> Result<RepeatStructure> {
    Ok(RepeatStructure {
        id: Codec::dec(r)?,
        start: Codec::dec(r)?,
        end: Codec::dec(r)?,
        kind: RepeatKind::migration_default(),
        voltas: Vec::new(),
    })
}

fn enc_ccr_v1(c: &CrossCuttingRegistry, out: &mut Vec<u8>) {
    enc_vec_v1(&c.slurs, out, enc_slur_v1);
    enc_vec_v1(&c.ties, out, enc_tie_v1);
    enc_vec_v1(&c.beams, out, enc_beam_v1);
    c.tuplets.enc(out);
    enc_vec_v1(&c.spanners, out, enc_spanner_v1);
    c.markers.enc(out);
    enc_vec_v1(&c.repeats, out, enc_repeat_v1);
    c.analytical.enc(out);
    c.comments.enc(out);
    c.graphic_gestures.enc(out);
    c.lyrics.enc(out);
    c.chord_symbols.enc(out);
}

fn dec_ccr_v1(r: &mut Reader<'_>) -> Result<CrossCuttingRegistry> {
    Ok(CrossCuttingRegistry {
        slurs: dec_vec_v1(r, dec_slur_v1)?,
        ties: dec_vec_v1(r, dec_tie_v1)?,
        beams: dec_vec_v1(r, dec_beam_v1)?,
        tuplets: Codec::dec(r)?,
        spanners: dec_vec_v1(r, dec_spanner_v1)?,
        markers: Codec::dec(r)?,
        repeats: dec_vec_v1(r, dec_repeat_v1)?,
        analytical: Codec::dec(r)?,
        comments: Codec::dec(r)?,
        graphic_gestures: Codec::dec(r)?,
        lyrics: Codec::dec(r)?,
        chord_symbols: Codec::dec(r)?,
    })
}

fn enc_staff_instance_v1(si: &StaffInstance, out: &mut Vec<u8>) {
    si.id.enc(out);
    si.staff.enc(out);
    si.voices.enc(out);
    si.clef_sequence.enc(out);
    si.key_sequence.enc(out);
    si.local_metric_grid.enc(out);
    si.measures.enc(out);
    si.instrument_override.enc(out);
    match &si.staff_lines_override {
        None => out.push(0),
        Some(c) => {
            out.push(1);
            enc_staff_lines_v1(c, out);
        }
    }
    si.visible.enc(out);
}

fn dec_staff_instance_v1(r: &mut Reader<'_>) -> Result<StaffInstance> {
    Ok(StaffInstance {
        id: Codec::dec(r)?,
        staff: Codec::dec(r)?,
        voices: Codec::dec(r)?,
        clef_sequence: Codec::dec(r)?,
        key_sequence: Codec::dec(r)?,
        local_metric_grid: Codec::dec(r)?,
        measures: Codec::dec(r)?,
        instrument_override: Codec::dec(r)?,
        staff_lines_override: match r.u8()? {
            0 => None,
            1 => Some(dec_staff_lines_v1(r)?),
            tag => {
                return Err(ScoreDecodeError::InvalidTag {
                    kind: "Option",
                    tag,
                })
            }
        },
        visible: Codec::dec(r)?,
    })
}

fn enc_sbc_v1(sbc: &StaffBasedContent, out: &mut Vec<u8>) {
    enc_vec_v1(&sbc.staff_instances, out, enc_staff_instance_v1);
    sbc.default_metric_grid.enc(out);
    sbc.barline_alignment_groups.enc(out);
    sbc.user_system_breaks.enc(out);
    sbc.user_page_breaks.enc(out);
}

fn dec_sbc_v1(r: &mut Reader<'_>) -> Result<StaffBasedContent> {
    Ok(StaffBasedContent {
        staff_instances: dec_vec_v1(r, dec_staff_instance_v1)?,
        default_metric_grid: Codec::dec(r)?,
        barline_alignment_groups: Codec::dec(r)?,
        user_system_breaks: Codec::dec(r)?,
        user_page_breaks: Codec::dec(r)?,
    })
}

fn enc_content_v1(content: &RegionContent, out: &mut Vec<u8>) {
    match content {
        RegionContent::StaffBased(s) => {
            out.push(0);
            enc_sbc_v1(s, out);
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
            enc_sbc_v1(staves, out);
            overlay.enc(out);
            overlay_below_staves.enc(out);
        }
    }
}

fn dec_content_v1(r: &mut Reader<'_>) -> Result<RegionContent> {
    match r.u8()? {
        0 => Ok(RegionContent::StaffBased(dec_sbc_v1(r)?)),
        1 => Ok(RegionContent::FreeGraphic(Codec::dec(r)?)),
        2 => Ok(RegionContent::Hybrid {
            staves: dec_sbc_v1(r)?,
            overlay: Codec::dec(r)?,
            overlay_below_staves: Codec::dec(r)?,
        }),
        tag => Err(ScoreDecodeError::InvalidTag {
            kind: "RegionContent",
            tag,
        }),
    }
}

fn enc_region_v1(reg: &Region, out: &mut Vec<u8>) {
    reg.id.enc(out);
    reg.time_model.enc(out);
    enc_content_v1(&reg.content, out);
    reg.time_extent.enc(out);
    reg.staff_extent.enc(out);
    reg.local_tempo_map.enc(out);
    reg.permits_spanning_slurs.enc(out);
}

fn dec_region_v1(r: &mut Reader<'_>) -> Result<Region> {
    Ok(Region {
        id: Codec::dec(r)?,
        time_model: Codec::dec(r)?,
        content: dec_content_v1(r)?,
        time_extent: Codec::dec(r)?,
        staff_extent: Codec::dec(r)?,
        local_tempo_map: Codec::dec(r)?,
        permits_spanning_slurs: Codec::dec(r)?,
    })
}

fn enc_canvas_v1(c: &Canvas, out: &mut Vec<u8>) {
    enc_vec_v1(&c.regions, out, enc_region_v1);
    c.layout_defaults.enc(out);
}

fn dec_canvas_v1(r: &mut Reader<'_>) -> Result<Canvas> {
    Ok(Canvas {
        regions: dec_vec_v1(r, dec_region_v1)?,
        layout_defaults: Codec::dec(r)?,
    })
}

fn enc_instruments_v1(instruments: &[Instrument], out: &mut Vec<u8>) {
    put_len(out, instruments.len());
    for inst in instruments {
        inst.id.enc(out);
        inst.name.enc(out);
        inst.range.enc(out);
    }
}

fn dec_instruments_v1(r: &mut Reader<'_>) -> Result<Vec<Instrument>> {
    let n = r.count()?;
    let mut instruments = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        instruments.push(Instrument {
            id: Codec::dec(r)?,
            name: Codec::dec(r)?,
            range: Codec::dec(r)?,
            abbreviation: None,
            sound_config: SoundConfiguration::default(),
            transposition: None,
            default_clef: Clef::treble(),
            default_staff_lines: StaffLineConfiguration::default(),
            unpitched_members: Vec::new(),
        });
    }
    Ok(instruments)
}

/// The **frozen schema-major-1** encoding of a score — the byte-exact inverse
/// of [`decode_v1_score`]'s field walk. Used by `decode_v1_score` to enforce
/// strict v1 canonicality, and by migration tests and the decode fuzzer to
/// synthesize genuine v1 bytes. A migrated score's schema-major-2 fields hold
/// their defaults, so this simply omits them.
pub(crate) fn encode_v1_score(s: &Score) -> Vec<u8> {
    let mut out = Vec::new();
    enc_metadata_v1(&s.metadata, &mut out);
    enc_canvas_v1(&s.canvas, &mut out);
    enc_instruments_v1(&s.instruments, &mut out);
    enc_staves_v1(&s.staves, &mut out);
    s.staff_groups.enc(&mut out);
    s.parts.enc(&mut out);
    enc_ccr_v1(&s.cross_cutting, &mut out);
    s.time_signatures.enc(&mut out);
    s.tuning_context.enc(&mut out);
    s.tempo_map.enc(&mut out);
    s.events.enc(&mut out);
    s.spelling_attachments.enc(&mut out);
    s.decomposition_attachments.enc(&mut out);
    s.spelling_precedence.enc(&mut out);
    s.analysis_layers.enc(&mut out);
    s.views.enc(&mut out);
    s.identity.enc(&mut out);
    s.tombstoned_pitches.enc(&mut out);
    s.tombstoned_events.enc(&mut out);
    out
}

/// Decodes **schema-major-1** `Score` bytes into the current-layout `Score`,
/// migrating on read (total, default-filling — Binary Format §Schema
/// Major 2's migration table). Strictly canonical on the v1 wire form, like
/// its v0 sibling: re-encodes through [`encode_v1_score`] and rejects any
/// input that is not already its canonical v1 encoding.
fn decode_v1_score(bytes: &[u8]) -> Result<Score> {
    let mut r = Reader::new(bytes);
    let metadata = dec_metadata_v1(&mut r)?;
    let canvas = dec_canvas_v1(&mut r)?;
    let instruments = dec_instruments_v1(&mut r)?;
    let staves = dec_staves_v1(&mut r)?;
    let staff_groups = Codec::dec(&mut r)?;
    let parts = Codec::dec(&mut r)?;
    let cross_cutting = dec_ccr_v1(&mut r)?;
    let time_signatures = Codec::dec(&mut r)?;
    let tuning_context = Codec::dec(&mut r)?;
    let tempo_map = Codec::dec(&mut r)?;
    let events = Codec::dec(&mut r)?;
    let spelling_attachments = Codec::dec(&mut r)?;
    let decomposition_attachments = Codec::dec(&mut r)?;
    let spelling_precedence = Codec::dec(&mut r)?;
    let analysis_layers = Codec::dec(&mut r)?;
    let views = Codec::dec(&mut r)?;
    let identity = Codec::dec(&mut r)?;
    let tombstoned_pitches = Codec::dec(&mut r)?;
    let tombstoned_events = Codec::dec(&mut r)?;
    r.finish()?;
    let score = Score {
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
        tombstoned_events,
    };
    if encode_v1_score(&score) != bytes {
        return Err(ScoreDecodeError::InvalidValue(
            "non-canonical v1 Score encoding",
        ));
    }
    Ok(score)
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
                    // Strictly canonical, exactly as `Score::decode_canonical`:
                    // reject a non-canonical encoding so per-value decode is
                    // injective (these bytes are content-addressed inside
                    // operation payloads).
                    if v.canonical_bytes() != bytes {
                        return Err(ScoreDecodeError::InvalidValue(
                            "non-canonical value encoding",
                        ));
                    }
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
    // Structural containers (M2c) — value-typed create payloads embed these.
    Region,
    StaffInstance,
    Voice,
    // Pre-pass annotation leaves — give DerivedAnnotations a canonical byte
    // fingerprint (a normative surface, vs. its Debug form).
    DecompositionAttachment,
    SpellingSourceKind,
    // Score settings (M2d) — value-typed set-* payloads embed these.
    ScoreMetadata,
    MetricGrid,
    // Phase-3 ops tranche — value-typed create/set payloads embed these.
    // `TimeSignature::dec` re-validates the beat-group sum through
    // `TimeSignature::new` (the "reject at construction *and* at decode"
    // discipline the operation catalog's §"Meter and Tempo Overwrites"
    // requires), so a malformed value never round-trips.
    Staff,
    TimeSignature,
    TempoSegment,
    StaffLineConfiguration,
    // Repeat authoring (schema-major-2 revision) — CreateRepeatStructure
    // embeds the full value.
    RepeatStructure,
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
                assert_value_round_trips(region);
                for instance in region.staff_instances() {
                    assert_value_round_trips(instance);
                    for voice in &instance.voices {
                        assert_value_round_trips(voice);
                    }
                }
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
    fn clef_and_key_signature_codecs_round_trip() {
        use crate::graph::{Clef, ClefChange, ClefShape, KeySignature, KeySignatureChange};
        use crate::time::{TimeAnchor, WallClockTime};

        fn rt<T: Codec + PartialEq + core::fmt::Debug>(v: &T) {
            let mut out = Vec::new();
            v.enc(&mut out);
            let mut r = Reader::new(&out);
            let decoded = T::dec(&mut r).expect("decodes");
            assert_eq!(&decoded, v, "round-trip changed the value");
        }

        for shape in [
            ClefShape::G,
            ClefShape::F,
            ClefShape::C,
            ClefShape::Percussion,
        ] {
            rt(&shape);
            for line in [-3i8, 1, 2, 3, 4, 5] {
                for octave_shift in [-1i8, 0, 1] {
                    rt(&Clef {
                        shape,
                        line,
                        octave_shift,
                    });
                }
            }
        }
        for shape_clef in [Clef::treble(), Clef::bass(), Clef::alto(), Clef::tenor()] {
            rt(&shape_clef);
        }
        for fifths in -7i8..=7 {
            rt(&KeySignature::new(fifths).expect("fifths is in range"));
        }
        let anchor = TimeAnchor::WallClock {
            time: WallClockTime(0),
        };
        rt(&ClefChange {
            anchor: anchor.clone(),
            clef: Clef::bass(),
        });
        rt(&KeySignatureChange {
            anchor,
            key: KeySignature::new(-3).expect("fifths is in range"),
        });
    }

    #[test]
    fn key_signature_rejects_out_of_range_fifths_on_decode() {
        let decode = |fifths: i8| {
            let mut bytes = Vec::new();
            fifths.enc(&mut bytes);
            KeySignature::dec(&mut Reader::new(&bytes))
        };
        assert!(decode(-7).is_ok());
        assert!(decode(7).is_ok());
        for fifths in [-8i8, 8] {
            assert!(
                matches!(decode(fifths), Err(ScoreDecodeError::Reconstruct(_))),
                "out-of-range fifth count {fifths} must be rejected"
            );
        }
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

    /// A C-in-`cmn-12` pitch at the given octave, for a non-default
    /// [`PitchRange`] (the generators never populate `Instrument.range`).
    fn cmn_c(octave: i8) -> Pitch {
        Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new("cmn-12"),
                position: PitchSpacePosition::Cmn {
                    nominal: CmnNominal::C,
                    alteration: 0,
                    octave,
                },
            },
            acoustic: AcousticPitch {
                tuning: crate::pitch::TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        }
    }

    #[test]
    fn v1_round_trips_non_default_values_for_every_new_field() {
        // The three schema-major-1 fields must survive a v1 round-trip at
        // *non-default* values — not just vacuously when they equal the default
        // (which is all the generators produce). This is what makes them real
        // wire content rather than always-default padding.
        let mut score = valid_score(5);
        assert!(!score.instruments.is_empty() && !score.canvas.regions.is_empty());

        let ss = |v: f64| CanonicalF64::new(v).expect("finite");
        let custom = CanvasLayoutDefaults {
            // US Letter (216 × 279.4 mm) in staff spaces, distinct margins.
            page_size: CanvasSize {
                width: ss(216.0),
                height: ss(279.4),
            },
            margins: CanvasMargins {
                top: ss(9.0),
                right: ss(10.0),
                bottom: ss(11.0),
                left: ss(12.0),
            },
        };
        assert_ne!(custom, CanvasLayoutDefaults::default());
        score.canvas.layout_defaults = custom;
        score.canvas.regions[0].permits_spanning_slurs = true;
        score.instruments[0].range = Some(PitchRange {
            lowest: cmn_c(2),
            highest: cmn_c(6),
        });

        let bytes = score.canonical_bytes();
        assert_eq!(Score::decode_canonical(&bytes).unwrap(), score);
        assert_eq!(Score::decode_canonical_versioned(&bytes, 2).unwrap(), score);
        // The non-default major-1 values are representable at the frozen v1
        // wire form too: the v1 encoding migrates back to the same score
        // (its major-2 fields are all defaults here).
        let v1 = encode_v1_score(&score);
        assert_eq!(Score::decode_canonical_versioned(&v1, 1).unwrap(), score);
    }

    #[test]
    fn v0_score_migrates_default_filling_all_three_new_fields() {
        // Schema major 1 grew three fields — Canvas.layout_defaults,
        // Instrument.range, Region.permits_spanning_slurs — so a major-0 score
        // has none of them, and migrate-on-read must reconstruct each with its
        // canonical default. We synthesize *genuine* v0 bytes via a mirror v0
        // encoder (the byte-exact inverse of decode_v0_score) and check the
        // versioned decoder rebuilds the original score, whose three fields ARE
        // the defaults.
        let ld_len = {
            let mut b = Vec::new();
            CanvasLayoutDefaults::default().enc(&mut b);
            b.len()
        };
        for seed in 0..64u64 {
            let score = valid_score(seed.wrapping_mul(0x9E37_79B9).wrapping_add(1));
            // Precondition: the generator produces the v0-equivalent defaults, so
            // the original IS what a v0→v1 migration should reconstruct.
            assert_eq!(
                score.canvas.layout_defaults,
                CanvasLayoutDefaults::default()
            );
            assert!(score.instruments.iter().all(|i| i.range.is_none()));
            assert!(score
                .canvas
                .regions
                .iter()
                .all(|r| !r.permits_spanning_slurs));

            let current = score.canonical_bytes();
            let v1 = encode_v1_score(&score);
            let v0 = encode_v0_score(&score);
            // Size anchor (independent of the v0 encoder's field order): v0 is
            // exactly v1 minus the appended default bytes — layout_defaults once,
            // plus one byte each for every instrument's range=None (Option tag)
            // and every region's permits_spanning_slurs=false (bool).
            let expected_removed = ld_len + score.instruments.len() + score.canvas.regions.len();
            assert_eq!(
                v1.len() - v0.len(),
                expected_removed,
                "v0 omits exactly the three new fields' default bytes"
            );

            // Major 0: the shorter v0 bytes migrate up, default-filling all three
            // (and, composed, every schema-major-2 default).
            let migrated = Score::decode_canonical_versioned(&v0, 0).unwrap();
            assert_eq!(migrated, score);
            // The migrated score re-encodes to the production (major-2) bytes.
            assert_eq!(migrated.canonical_bytes(), current);
            // Major 1: the frozen v1 bytes migrate to the same score.
            assert_eq!(Score::decode_canonical_versioned(&v1, 1).unwrap(), score);
            // Major 2: the current bytes decode unchanged.
            assert_eq!(
                Score::decode_canonical_versioned(&current, 2).unwrap(),
                score
            );
        }
        // A major outside {0, 1, 2} is a defensive decode error (the gate
        // rejects it upstream in practice).
        assert!(Score::decode_canonical_versioned(&valid_score(1).canonical_bytes(), 3).is_err());
    }

    #[test]
    fn v1_score_migrates_default_filling_the_major_2_fields() {
        // Schema major 2 filled nine type bodies. A major-1 score carries none
        // of the appended fields; migrate-on-read reconstructs each at its
        // canonical default (Binary Format §Schema Major 2 migration table).
        // Genuine v1 bytes come from the mirror v1 encoder; the size anchor
        // pins that v1 omits exactly the appended default bytes, so the frozen
        // encoder cannot silently drift from the pre-major-2 wire form.
        for seed in 0..64u64 {
            let score = valid_score(seed.wrapping_mul(0x9E37_79B9).wrapping_add(3));
            let current = score.canonical_bytes();
            let v1 = encode_v1_score(&score);

            let c = &score.cross_cutting;
            let override_sites: usize = score
                .canvas
                .regions
                .iter()
                .flat_map(|r| r.content.staff_instances())
                .filter(|si| si.staff_lines_override.is_some())
                .count();
            // Appended default bytes per component: slur 4 (kind 1 +
            // curvature presence 1 + style line 1 + thickness presence 1);
            // tie 2 (style); beam 5 (sub_beams count 4 + geometry presence 1);
            // spanner 3 (kind tag 1 + style 2); repeat 9 (SimpleRepeat tag 1 +
            // count 4 + voltas count 4); staff 17 (clef 3 + config appends
            // 14 = spacing 12 + style 1 + bracket 1); instrument 28
            // (abbreviation 1 + sound count 4 + transposition 1 + clef 3 +
            // full config 15 + members count 4); an overridden staff-instance
            // config 14; metadata 23 (three presence bytes + two i64
            // timestamps + additional count 4).
            let expected_removed = c.slurs.len() * 4
                + c.ties.len() * 2
                + c.beams.len() * 5
                + c.spanners.len() * 3
                + c.repeats.len() * 9
                + score.staves.len() * 17
                + score.instruments.len() * 28
                + override_sites * 14
                + 23;
            assert_eq!(
                current.len() - v1.len(),
                expected_removed,
                "v1 omits exactly the appended major-2 default bytes"
            );

            let migrated = Score::decode_canonical_versioned(&v1, 1).unwrap();
            assert_eq!(migrated, score);
            assert_eq!(migrated.canonical_bytes(), current);
        }
    }

    #[test]
    fn current_major_round_trips_non_default_values_for_every_major_2_field() {
        // One score carrying a non-default value for EVERY schema-major-2
        // field (and every payload-carrying SpannerKind variant), so the live
        // codec arms are exercised as real wire content, not always-default
        // padding. Byte round-trip only — the standalone cross-cutting values
        // reference fixture events for hygiene but invariants are not the
        // subject here.
        use crate::ids::{BeamId, RepeatStructureId, SlurId, SpannerId, TieId};
        let mut score = valid_score(11);
        let events: Vec<crate::ids::EventId> = score
            .voices()
            .flat_map(|(_, _, v)| v.events.clone())
            .collect();
        assert!(events.len() >= 2);
        let ss = |v: f64| SpaceUnit(CanonicalF64::new(v).expect("finite"));
        let anchor = |n: i64| crate::time::TimeAnchor::WallClock {
            time: crate::time::WallClockTime(n),
        };

        score.cross_cutting.slurs.push(Slur {
            id: SlurId::new(ReplicaId(9), 1),
            start_event: events[0],
            end_event: events[1],
            kind: SlurKind::Phrase,
            curvature_override: Some(CurvatureOverride {
                direction: Some(CurveDirection::Below),
                height: Some(ss(2.5)),
            }),
            style: SpanStyle {
                line: LineStyle::Dashed,
                thickness: Some(ss(0.25)),
            },
        });
        score.cross_cutting.ties.push(Tie {
            id: TieId::new(ReplicaId(9), 2),
            start_event: events[0],
            end_event: events[1],
            pitch_pairing: None,
            class: TieClass::Editorial,
            style: SpanStyle {
                line: LineStyle::Dotted,
                thickness: None,
            },
        });
        score.cross_cutting.beams.push(Beam {
            id: BeamId::new(ReplicaId(9), 3),
            events: events.clone(),
            level: 1,
            sub_beams: vec![SubBeam {
                level: 2,
                events: vec![events[0]],
            }],
            geometry_override: Some(BeamGeometryOverride {
                slope: Some(CanonicalF64::new(0.5).expect("finite")),
                offset: Some(ss(-1.0)),
            }),
        });
        for (n, kind) in [
            SpannerKind::Hairpin(HairpinDirection::Crescendo),
            SpannerKind::OctaveLine(OctaveOffset(-1)),
            SpannerKind::PedalLine(PedalKind::Sostenuto),
            SpannerKind::TextLine(TextLineDefinition {
                text: String::from("rit."),
            }),
            SpannerKind::Bracket(BracketKind::Square),
            SpannerKind::TrillExtension,
        ]
        .into_iter()
        .enumerate()
        {
            score.cross_cutting.spanners.push(Spanner {
                id: SpannerId::new(ReplicaId(9), 10 + n as u64),
                start: anchor(0),
                end: anchor(1000),
                staves: score.staves.iter().map(|st| st.id).collect(),
                kind,
                style: SpanStyle {
                    line: LineStyle::Dashed,
                    thickness: Some(ss(0.1)),
                },
            });
        }
        score.cross_cutting.repeats.push(RepeatStructure {
            id: RepeatStructureId::new(ReplicaId(9), 20),
            start: anchor(0),
            end: anchor(500),
            kind: RepeatKind::DalSegno {
                segno: anchor(10),
                end_target: anchor(400),
            },
            voltas: vec![Volta {
                endings: vec![1, 2],
                start: anchor(300),
                end: anchor(500),
            }],
        });
        // Every RepeatKind wire arm as real content: DaCapo (tag 1) and the
        // payload-less Volta (tag 3); SimpleRepeat is the migration default
        // exercised everywhere else.
        score.cross_cutting.repeats.push(RepeatStructure {
            id: RepeatStructureId::new(ReplicaId(9), 21),
            start: anchor(500),
            end: anchor(700),
            kind: RepeatKind::DaCapo {
                end_target: anchor(600),
            },
            voltas: vec![],
        });
        score.cross_cutting.repeats.push(RepeatStructure {
            id: RepeatStructureId::new(ReplicaId(9), 22),
            start: anchor(700),
            end: anchor(900),
            kind: RepeatKind::Volta,
            voltas: vec![Volta {
                endings: vec![3],
                start: anchor(800),
                end: anchor(900),
            }],
        });

        score.staves[0].default_clef = Clef::bass();
        score.staves[0].default_staff_lines = StaffLineConfiguration {
            line_count: 1,
            line_spacing: ss(0.6),
            line_style: LineStyle::Dotted,
            bracket: Some(StaffBracketKind::Brace),
        };
        let si = score.canvas.regions[0]
            .content
            .staff_instances_mut()
            .expect("staff-based region")
            .first_mut()
            .expect("an instance");
        si.staff_lines_override = Some(StaffLineConfiguration {
            line_count: 4,
            line_spacing: ss(0.8),
            line_style: LineStyle::Dashed,
            bracket: Some(StaffBracketKind::Bracket),
        });

        score.instruments[0].abbreviation = Some(String::from("Vln."));
        score.instruments[0].sound_config = SoundConfiguration(vec![1, 2, 3]);
        score.instruments[0].transposition = Some(TranspositionInterval {
            diatonic_steps: -1,
            chromatic_steps: -2,
        });
        score.instruments[0].default_clef = Clef::alto();
        score.instruments[0].default_staff_lines = StaffLineConfiguration {
            line_count: 5,
            line_spacing: ss(1.2),
            line_style: LineStyle::Solid,
            bracket: None,
        };
        score.instruments[0].unpitched_members = vec![UnpitchedMember {
            member: UnpitchedMemberId(3),
            name: String::from("snare"),
            staff_position: StaffPosition(2),
        }];

        score.metadata.subtitle = Some(String::from("a subtitle"));
        score.metadata.lyricist = Some(String::from("a lyricist"));
        score.metadata.arranger = Some(String::from("an arranger"));
        score.metadata.creation_timestamp = Timestamp(1_700_000_000_000_000_000);
        score.metadata.modification_timestamp = Timestamp(1_700_000_100_000_000_000);
        score.metadata.additional = vec![
            MetadataEntry {
                key: String::from("opus"),
                value: MetadataValue::Integer(27),
            },
            MetadataEntry {
                key: String::from("dedication"),
                value: MetadataValue::Text(String::from("f\u{fc}r Elise")),
            },
            MetadataEntry {
                key: String::from("urtext"),
                value: MetadataValue::Flag(true),
            },
        ];

        let bytes = score.canonical_bytes();
        assert_eq!(Score::decode_canonical(&bytes).unwrap(), score);
        assert_eq!(Score::decode_canonical_versioned(&bytes, 2).unwrap(), score);
        // The per-value seam carries the filled bodies too.
        let slur = &score.cross_cutting.slurs[0];
        assert_eq!(
            Slur::decode_canonical(&slur.canonical_bytes()).unwrap(),
            *slur
        );
    }

    #[test]
    fn v0_regions_inside_canvas_decode_after_region_grew() {
        // The nested-Vec case the struct decoder exists for: multiple v0 Region
        // elements inside Canvas.regions must each decode and default-fill
        // permits_spanning_slurs. A wrong per-element size would desync the Vec
        // after the first element (every later region would misparse), so a
        // multi-region canvas is the discriminating fixture — valid_score_rich
        // carries three regions.
        let score = valid_score_rich(9);
        assert!(
            score.canvas.regions.len() >= 2,
            "need a multi-region canvas to exercise the Vec walk"
        );
        let v0 = encode_v0_score(&score);
        let migrated = Score::decode_canonical_versioned(&v0, 0).unwrap();
        assert_eq!(migrated, score);
        assert_eq!(migrated.canvas.regions.len(), score.canvas.regions.len());
        assert!(migrated
            .canvas
            .regions
            .iter()
            .all(|r| !r.permits_spanning_slurs));
    }

    #[test]
    fn v0_decode_is_strictly_canonical_over_the_v0_wire_form() {
        // The major-0 migration is strict, like the major-1 decoder: a canonical
        // v0 encoding is accepted and re-encodes to itself in the frozen v0 form
        // (`decode_v0_score` compares against `encode_v0_score`), while trailing
        // or truncated bytes are rejected. This closes the injectivity gap for
        // major-0 snapshots (content-addressed like major-1 ones).
        for seed in 0..32u64 {
            let score = valid_score(seed.wrapping_mul(0x9E37_79B9).wrapping_add(1));
            let v0 = encode_v0_score(&score);
            // Canonical v0 is accepted and re-encodes to the same v0 bytes.
            let migrated = Score::decode_canonical_versioned(&v0, 0).unwrap();
            assert_eq!(encode_v0_score(&migrated), v0);
            // Trailing garbage is rejected.
            let mut trailed = v0.clone();
            trailed.push(0);
            assert!(Score::decode_canonical_versioned(&trailed, 0).is_err());
            // A truncation is rejected.
            if !v0.is_empty() {
                assert!(Score::decode_canonical_versioned(&v0[..v0.len() - 1], 0).is_err());
            }
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
