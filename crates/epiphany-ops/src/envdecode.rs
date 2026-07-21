//! The inverse of [`OperationEnvelope::encode_canonical`] — bytes back to an
//! envelope (Binary Format companion §"Operation Envelope Encoding").
//!
//! Until this existed the format was **write-only for operations**: a bundle's
//! envelope blocks decoded to opaque byte strings ([`crate::fuzz`]'s corpus
//! notwithstanding), `OperationKind` had an encoder and no decoder, and nothing
//! in the workspace could turn a saved document's operations back into an
//! [`OperationSet`](crate::OperationSet). Chapter 6 holds that a score's
//! canonical state *is* the set of operations committed to it, so a bundle whose
//! operations cannot be read is a bundle whose score cannot be reopened.
//!
//! ## Strictness
//!
//! Decode is **strict-canonical**: an accepted byte string re-encodes to itself.
//! That is enforced complete-by-construction at the envelope boundary — the
//! decoded envelope is re-encoded and compared — which is sound *here* because
//! every sequence in the envelope encoding is normalized by its encoder
//! (`sorted_canonical`, `BTreeSet`, `BTreeMap`). Where an encoder writes a `Vec`
//! verbatim a guard is blind and a per-site order check is required; see
//! `DECISIONS.md` §"Push 5 / P2".
//!
//! Two rules are checked per-site anyway, because they deserve their own error
//! rather than a bare `NonCanonical`, and because a future encoder change must
//! not silently relax them:
//!
//! * `TransposeInterval.targets` is a **set**: its sequence must be strictly
//!   increasing. A duplicate is rejected, never normalized away
//!   (Binary Format `seq^⇑`; `req:opcat:transpose-interval-targets`).
//! * `Transpose.targets` is the frozen **multiset**: non-decreasing, duplicates
//!   preserved (`req:opcat:transpose-frozen`).

use std::collections::BTreeSet;

use epiphany_core::{
    Beam, Event, IdentifiedPitch, Pitch, Region, RepeatStructure, Rest, Slur, Spanner, Tie,
};
use epiphany_core::{CanonicalValue, TempoSegment};
use epiphany_core::{
    EventId, InstrumentId, MetricGrid, MusicalPosition, OperationId, PitchId, PitchSpelling,
    RegionId, RegionTimeModel, RepeatStructureId, ReplicaId, ScoreMetadata, Staff, StaffInstance,
    StaffInstanceId, StaffLineConfiguration, TimeAnchor, TimeSignature, TranspositionInterval,
    TupletId, TypedObjectId, Voice, VoiceId, WallClockTime,
};
use epiphany_determinism::{CanonicalDecode, CanonicalEncode};

use crate::causal::CausalContext;
use crate::conflict::{ConflictId, ResolutionAction};
use crate::envelope::{EnvelopeHash, OperationEnvelope};
use crate::payload::*;
use crate::stamp::{HybridLogicalClock, OperationStamp};
use crate::support::ResolutionRegistryId;
use crate::support::{AuthorId, OperationKindRegistryId};
use crate::undo::{UndoPolicy, UndoTransactionPayload};
use epiphany_core::TransactionId;

/// Why an operation envelope failed to decode.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EnvelopeDecodeError {
    /// A field ended before its declared or fixed width.
    UnexpectedEof,
    /// A `u32` length or count does not fit this process's `usize`.
    LengthOverflow,
    /// A declared count or length exceeds the bytes remaining. Rejected before
    /// any allocation: a garbage `u32` must not become a 4-billion-element
    /// reservation, nor a walk toward EOF.
    CountExceedsRemaining { declared: usize, remaining: usize },
    /// A tagged union carried an unknown discriminant.
    InvalidTag { kind: &'static str, tag: u8 },
    /// An embedded canonical value failed its own decoder.
    InvalidValue(&'static str),
    /// A canonical boolean was not `0` or `1`.
    InvalidBoolean(u8),
    /// A canonical text field was not UTF-8.
    InvalidUtf8,
    /// A canonical text field was not in Unicode NFC.
    NotNfc,
    /// Bytes remained after the envelope was decoded.
    TrailingBytes,
    /// `TransposeInterval.targets` is a set: strictly increasing, no duplicates.
    TargetsNotStrictlyIncreasing,
    /// `Transpose.targets` is a frozen multiset: non-decreasing.
    TargetsNotSorted,
    /// The bytes decoded structurally but are not the canonical encoding of the
    /// value they decode to.
    NonCanonical,
}

impl core::fmt::Display for EnvelopeDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of envelope bytes"),
            Self::LengthOverflow => f.write_str("envelope length does not fit usize"),
            Self::CountExceedsRemaining {
                declared,
                remaining,
            } => write!(
                f,
                "declared count {declared} exceeds {remaining} bytes remaining"
            ),
            Self::InvalidTag { kind, tag } => write!(f, "invalid {kind} tag {tag}"),
            Self::InvalidValue(kind) => write!(f, "invalid canonical {kind}"),
            Self::InvalidBoolean(v) => write!(f, "invalid canonical boolean {v}"),
            Self::InvalidUtf8 => f.write_str("invalid UTF-8 in canonical text"),
            Self::NotNfc => f.write_str("canonical text is not NFC"),
            Self::TrailingBytes => f.write_str("trailing bytes after envelope"),
            Self::TargetsNotStrictlyIncreasing => f.write_str(
                "TransposeInterval targets are a set: strictly increasing, no duplicates",
            ),
            Self::TargetsNotSorted => f.write_str("Transpose targets are not sorted"),
            Self::NonCanonical => f.write_str("envelope bytes are not canonical"),
        }
    }
}

impl std::error::Error for EnvelopeDecodeError {}

type Result<T> = core::result::Result<T, EnvelopeDecodeError>;

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(EnvelopeDecodeError::UnexpectedEof);
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32_le(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }

    fn u64_le(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }

    /// A `u32` count or length, bounded by the bytes remaining. Every element of
    /// every sequence in this encoding costs at least one byte, so a count past
    /// the remainder is garbage — rejected before it can drive an allocation.
    fn count(&mut self) -> Result<usize> {
        let declared =
            usize::try_from(self.u32_le()?).map_err(|_| EnvelopeDecodeError::LengthOverflow)?;
        if declared > self.remaining() {
            return Err(EnvelopeDecodeError::CountExceedsRemaining {
                declared,
                remaining: self.remaining(),
            });
        }
        Ok(declared)
    }

    fn lp_bytes(&mut self) -> Result<&'a [u8]> {
        let n = self.count()?;
        self.take(n)
    }

    fn bool(&mut self) -> Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(EnvelopeDecodeError::InvalidBoolean(other)),
        }
    }

    fn string(&mut self) -> Result<String> {
        let raw = self.lp_bytes()?;
        let s = core::str::from_utf8(raw).map_err(|_| EnvelopeDecodeError::InvalidUtf8)?;
        // Appendix D: canonical text is NFC. A non-NFC string would be
        // normalized on re-encode, so the boundary guard would reject it — but
        // name the failure here rather than call it "non-canonical".
        use unicode_normalization::UnicodeNormalization;
        if s.nfc().collect::<String>() != s {
            return Err(EnvelopeDecodeError::NotNfc);
        }
        Ok(s.to_string())
    }

    /// A `count`-prefixed sequence of length-prefixed elements.
    fn seq<T>(&mut self, mut each: impl FnMut(&[u8]) -> Result<T>) -> Result<Vec<T>> {
        let n = self.count()?;
        let mut out = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            let elem = self.lp_bytes()?;
            out.push(each(elem)?);
        }
        Ok(out)
    }

    fn finish(self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(EnvelopeDecodeError::TrailingBytes)
        }
    }
}

/// A fixed-width canonical primitive read directly from the reader.
fn canon<T: CanonicalDecode>(r: &mut Reader<'_>, width: usize, kind: &'static str) -> Result<T> {
    let bytes = r.take(width)?;
    T::decode_canonical(bytes).map_err(|_| EnvelopeDecodeError::InvalidValue(kind))
}

/// An embedded [`CanonicalValue`] behind its `u32` length prefix.
fn value<T: CanonicalValue>(r: &mut Reader<'_>, kind: &'static str) -> Result<T> {
    let bytes = r.lp_bytes()?;
    T::decode_canonical(bytes).map_err(|_| EnvelopeDecodeError::InvalidValue(kind))
}

/// A whole `T` from exactly `bytes` (a sequence element).
fn exact<T: CanonicalDecode>(bytes: &[u8], kind: &'static str) -> Result<T> {
    T::decode_canonical(bytes).map_err(|_| EnvelopeDecodeError::InvalidValue(kind))
}

fn opt<T>(
    r: &mut Reader<'_>,
    mut some: impl FnMut(&mut Reader<'_>) -> Result<T>,
) -> Result<Option<T>> {
    match r.byte()? {
        0 => Ok(None),
        1 => Ok(Some(some(r)?)),
        tag => Err(EnvelopeDecodeError::InvalidTag {
            kind: "Option",
            tag,
        }),
    }
}

// --- Identifiers (all 16 big-endian bytes) ---------------------------------

macro_rules! id_reader {
    ($fn_name:ident, $ty:ty, $kind:literal) => {
        fn $fn_name(r: &mut Reader<'_>) -> Result<$ty> {
            canon::<$ty>(r, 16, $kind)
        }
    };
}
id_reader!(operation_id, OperationId, "OperationId");
id_reader!(event_id, EventId, "EventId");
id_reader!(pitch_id, PitchId, "PitchId");
id_reader!(region_id, RegionId, "RegionId");
id_reader!(staff_instance_id, StaffInstanceId, "StaffInstanceId");
id_reader!(voice_id, VoiceId, "VoiceId");
id_reader!(instrument_id, InstrumentId, "InstrumentId");
id_reader!(repeat_id, RepeatStructureId, "RepeatStructureId");
id_reader!(transaction_id, TransactionId, "TransactionId");

/// `TypedObjectId` is variable-width: a 2-byte big-endian discriminant, then a
/// 16-byte payload — or, for `Registered` (discriminant 27), a 16-byte registry
/// id followed by 16 more extension bytes. The discriminant must be read before
/// the width is known.
const TYPED_OBJECT_ID_REGISTERED: u16 = 27;
fn typed_object_id(r: &mut Reader<'_>) -> Result<TypedObjectId> {
    if r.remaining() < 2 {
        return Err(EnvelopeDecodeError::UnexpectedEof);
    }
    let discriminant = u16::from_be_bytes(r.bytes[r.pos..r.pos + 2].try_into().expect("2"));
    let width = if discriminant == TYPED_OBJECT_ID_REGISTERED {
        2 + 16 + 16
    } else {
        2 + 16
    };
    canon::<TypedObjectId>(r, width, "TypedObjectId")
}

fn u128_id(r: &mut Reader<'_>) -> Result<u128> {
    Ok(u128::from_be_bytes(r.take(16)?.try_into().expect("16")))
}

// --- Envelope scaffolding ---------------------------------------------------

fn hlc(r: &mut Reader<'_>) -> Result<HybridLogicalClock> {
    let physical = WallClockTime(i64::from_le_bytes(r.take(8)?.try_into().expect("8")));
    let logical_counter = r.u32_le()?;
    Ok(HybridLogicalClock::new(physical, logical_counter))
}

fn stamp(r: &mut Reader<'_>) -> Result<OperationStamp> {
    let clock = hlc(r)?;
    let id = operation_id(r)?;
    Ok(OperationStamp::new(clock, id))
}

fn causal_context(r: &mut Reader<'_>) -> Result<CausalContext> {
    let mut ctx = CausalContext::new();
    let vector_len = r.count()?;
    for _ in 0..vector_len {
        let replica = ReplicaId(u64::from_be_bytes(r.take(8)?.try_into().expect("8")));
        let counter = r.u64_le()?;
        ctx = ctx.with_seen(replica, counter);
    }
    let dots = r.count()?;
    for _ in 0..dots {
        ctx = ctx.with_dot(operation_id(r)?);
    }
    Ok(ctx)
}

// --- Sub-values -------------------------------------------------------------

fn tuplet_compensation(r: &mut Reader<'_>) -> Result<TupletCompensation> {
    Ok(match r.byte()? {
        0 => TupletCompensation::NotInTuplet,
        1 => TupletCompensation::ReplaceWithRest {
            rest: value::<Rest>(r, "Rest")?,
        },
        2 => TupletCompensation::RewriteTuplets {
            tuplets: r.seq(|b| exact::<TupletId>(b, "TupletId"))?,
        },
        3 => TupletCompensation::CascadeDeleteTuplets {
            tuplets: r.seq(|b| exact::<TupletId>(b, "TupletId"))?,
        },
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "TupletCompensation",
                tag,
            })
        }
    })
}

fn cross_cutting(r: &mut Reader<'_>) -> Result<CrossCuttingValue> {
    Ok(match r.byte()? {
        0 => CrossCuttingValue::Tie(value::<Tie>(r, "Tie")?),
        1 => CrossCuttingValue::Slur(value::<Slur>(r, "Slur")?),
        2 => CrossCuttingValue::Beam(value::<Beam>(r, "Beam")?),
        3 => CrossCuttingValue::Spanner(value::<Spanner>(r, "Spanner")?),
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "CrossCuttingValue",
                tag,
            })
        }
    })
}

fn position_remapping(r: &mut Reader<'_>) -> Result<PositionRemapping> {
    Ok(match r.byte()? {
        0 => PositionRemapping::PreserveTime,
        1 => {
            let n = r.count()?;
            let mut entries = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                let event = event_id(r)?;
                let bytes = r.lp_bytes()?;
                entries.push((event, exact::<MusicalPosition>(bytes, "MusicalPosition")?));
            }
            PositionRemapping::Reassign(entries)
        }
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "PositionRemapping",
                tag,
            })
        }
    })
}

fn transaction_category(r: &mut Reader<'_>) -> Result<TransactionCategory> {
    Ok(match r.byte()? {
        0 => TransactionCategory::NoteEntry,
        1 => TransactionCategory::Structural,
        2 => TransactionCategory::Layout,
        3 => TransactionCategory::Import,
        4 => TransactionCategory::Registered(OperationKindRegistryId(u128_id(r)?)),
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "TransactionCategory",
                tag,
            })
        }
    })
}

fn transaction_descriptor(r: &mut Reader<'_>) -> Result<TransactionDescriptor> {
    let id = transaction_id(r)?;
    let label = r.string()?;
    let category = opt(r, transaction_category)?;
    Ok(TransactionDescriptor {
        id,
        label,
        category,
    })
}

fn resolution_action(r: &mut Reader<'_>) -> Result<ResolutionAction> {
    Ok(match r.byte()? {
        0 => ResolutionAction::AcceptLoser,
        1 => ResolutionAction::KeepWinner,
        2 => ResolutionAction::Override {
            override_operation: operation_id(r)?,
        },
        3 => ResolutionAction::Reanchor {
            new_target: typed_object_id(r)?,
        },
        4 => ResolutionAction::Dismiss,
        5 => ResolutionAction::Registered(ResolutionRegistryId(u128_id(r)?)),
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "ResolutionAction",
                tag,
            })
        }
    })
}

fn undo_policy(r: &mut Reader<'_>) -> Result<UndoPolicy> {
    Ok(match r.byte()? {
        0 => UndoPolicy::StrictInverse,
        1 => UndoPolicy::BestEffort,
        2 => UndoPolicy::Cascade,
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "UndoPolicy",
                tag,
            })
        }
    })
}

// --- Operation kinds --------------------------------------------------------

fn operation_kind(r: &mut Reader<'_>) -> Result<OperationKind> {
    let tag = r.byte()?;
    Ok(match tag {
        0 => OperationKind::InsertEvent(InsertEventOp {
            staff_instance: staff_instance_id(r)?,
            event: value::<Event>(r, "Event")?,
        }),
        1 => OperationKind::DeleteEvent(DeleteEventOp {
            event: event_id(r)?,
            tuplet_compensation: tuplet_compensation(r)?,
        }),
        2 => OperationKind::RespellPitch(RespellPitchOp {
            pitch: pitch_id(r)?,
            spelling: value::<PitchSpelling>(r, "PitchSpelling")?,
        }),
        3 => OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: cross_cutting(r)?,
        }),
        4 => OperationKind::ChangeRegionTimeModel(ChangeRegionTimeModelOp {
            region: region_id(r)?,
            new_time_model: value::<RegionTimeModel>(r, "RegionTimeModel")?,
            declared_incompatible: r.seq(|b| exact::<EventId>(b, "EventId"))?,
            remapping: position_remapping(r)?,
        }),
        5 => OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
            region: region_id(r)?,
            anchor: value::<TimeAnchor>(r, "TimeAnchor")?,
            present: r.bool()?,
        }),
        6 => OperationKind::DeclareTransaction(transaction_descriptor(r)?),
        7 => {
            let id = OperationKindRegistryId(u128_id(r)?);
            let bytes = r.lp_bytes()?.to_vec();
            OperationKind::Registered(id, bytes)
        }
        8 => OperationKind::ModifyEvent(ModifyEventOp {
            event: value::<Event>(r, "Event")?,
        }),
        9 => {
            let targets = r.seq(|b| exact::<PitchId>(b, "PitchId"))?;
            // The frozen multiset: non-decreasing, duplicates preserved.
            if targets
                .windows(2)
                .any(|w| w[0].to_canonical_bytes() > w[1].to_canonical_bytes())
            {
                return Err(EnvelopeDecodeError::TargetsNotSorted);
            }
            let chromatic_steps = i32::from_le_bytes(r.take(4)?.try_into().expect("4"));
            OperationKind::Transpose(TransposeOp {
                targets,
                chromatic_steps,
            })
        }
        10 => OperationKind::InsertIdentifiedPitch(InsertIdentifiedPitchOp {
            event: event_id(r)?,
            pitch: value::<IdentifiedPitch>(r, "IdentifiedPitch")?,
        }),
        11 => OperationKind::DeleteIdentifiedPitch(DeleteIdentifiedPitchOp {
            pitch: pitch_id(r)?,
        }),
        12 => OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
            pitch: pitch_id(r)?,
            value: value::<Pitch>(r, "Pitch")?,
        }),
        13 => OperationKind::DeleteCrossCutting(DeleteCrossCuttingOp {
            structure: typed_object_id(r)?,
        }),
        14 => OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
            structure: cross_cutting(r)?,
        }),
        15 => OperationKind::CreateRegion(CreateRegionOp {
            region: value::<Region>(r, "Region")?,
        }),
        16 => OperationKind::DeleteRegion(DeleteRegionOp {
            region: region_id(r)?,
        }),
        17 => OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: region_id(r)?,
            instance: value::<StaffInstance>(r, "StaffInstance")?,
        }),
        18 => OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
            staff_instance: staff_instance_id(r)?,
        }),
        19 => OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: staff_instance_id(r)?,
            voice: value::<Voice>(r, "Voice")?,
        }),
        20 => OperationKind::DeleteVoice(DeleteVoiceOp {
            voice: voice_id(r)?,
        }),
        21 => OperationKind::SetMetadata(SetMetadataOp {
            metadata: value::<ScoreMetadata>(r, "ScoreMetadata")?,
        }),
        22 => OperationKind::SetMetricGrid(SetMetricGridOp {
            region: region_id(r)?,
            grid: opt(r, |r| value::<MetricGrid>(r, "MetricGrid"))?,
        }),
        23 => OperationKind::SetUserPageBreak(SetUserPageBreakOp {
            region: region_id(r)?,
            anchor: value::<TimeAnchor>(r, "TimeAnchor")?,
            present: r.bool()?,
        }),
        24 => OperationKind::CreateStaff(CreateStaffOp {
            staff: value::<Staff>(r, "Staff")?,
        }),
        25 => OperationKind::SetTimeSignature(SetTimeSignatureOp {
            region: region_id(r)?,
            anchor: value::<TimeAnchor>(r, "TimeAnchor")?,
            time_signature: opt(r, |r| value::<TimeSignature>(r, "TimeSignature"))?,
        }),
        26 => OperationKind::SetTempoSegment(SetTempoSegmentOp {
            region: opt(r, region_id)?,
            start: value::<TimeAnchor>(r, "TimeAnchor")?,
            segment: opt(r, |r| value::<TempoSegment>(r, "TempoSegment"))?,
        }),
        27 => OperationKind::SetStaffLayout(SetStaffLayoutOp {
            staff_instance: staff_instance_id(r)?,
            instrument_override: opt(r, instrument_id)?,
            staff_lines_override: opt(r, |r| {
                value::<StaffLineConfiguration>(r, "StaffLineConfiguration")
            })?,
            visible: r.bool()?,
        }),
        28 => OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
            repeat: value::<RepeatStructure>(r, "RepeatStructure")?,
        }),
        29 => OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp {
            repeat: repeat_id(r)?,
        }),
        30 => {
            let targets = r.seq(|b| exact::<PitchId>(b, "PitchId"))?;
            // `seq^⇑`: a set. Strictly increasing, and a duplicate is REJECTED,
            // never normalized away by the `BTreeSet` this collects into.
            if targets
                .windows(2)
                .any(|w| w[0].to_canonical_bytes() >= w[1].to_canonical_bytes())
            {
                return Err(EnvelopeDecodeError::TargetsNotStrictlyIncreasing);
            }
            let diatonic_steps = i32::from_le_bytes(r.take(4)?.try_into().expect("4"));
            let chromatic_steps = i32::from_le_bytes(r.take(4)?.try_into().expect("4"));
            OperationKind::TransposeInterval(TransposeIntervalOp {
                targets: targets.into_iter().collect::<BTreeSet<PitchId>>(),
                interval: TranspositionInterval {
                    diatonic_steps,
                    chromatic_steps,
                },
            })
        }
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "OperationKind",
                tag,
            })
        }
    })
}

fn payload(r: &mut Reader<'_>) -> Result<OperationPayload> {
    Ok(match r.byte()? {
        0 => OperationPayload::Primitive(operation_kind(r)?),
        1 => OperationPayload::ResolveConflict(ResolveConflictPayload {
            target: ConflictId(u128_id(r)?),
            action: resolution_action(r)?,
        }),
        2 => OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: transaction_id(r)?,
            policy: undo_policy(r)?,
        }),
        3 => OperationPayload::ResolveEquivocation(ResolveEquivocationPayload {
            target: operation_id(r)?,
            chosen: EnvelopeHash(r.take(32)?.try_into().expect("32")),
        }),
        tag => {
            return Err(EnvelopeDecodeError::InvalidTag {
                kind: "OperationPayload",
                tag,
            })
        }
    })
}

/// Decodes an [`OperationEnvelope`] from its canonical bytes.
///
/// Strict: an accepted byte string re-encodes to exactly itself. This is the
/// inverse the format was missing — with it, a bundle's operation blocks become
/// an [`OperationSet`](crate::OperationSet) again.
pub fn decode_envelope(bytes: &[u8]) -> Result<OperationEnvelope> {
    let mut r = Reader::new(bytes);
    let id = operation_id(&mut r)?;
    let author = AuthorId(u128_id(&mut r)?);
    let stamp = stamp(&mut r)?;
    let causal_context = causal_context(&mut r)?;
    let transaction = opt(&mut r, transaction_id)?;
    let payload = payload(&mut r)?;
    r.finish()?;

    let envelope = OperationEnvelope {
        id,
        author,
        stamp,
        causal_context,
        transaction,
        payload,
    };
    // Complete-by-construction strictness: every sequence in this encoding is
    // normalized by its encoder, so a non-canonical input cannot survive a round
    // trip. (Where an encoder writes a `Vec` verbatim this guard is blind; those
    // fields carry per-site checks above.)
    if envelope.to_canonical_bytes() != bytes {
        return Err(EnvelopeDecodeError::NonCanonical);
    }
    Ok(envelope)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::valuegen;
    use epiphany_core::{BeamId, MusicalDuration, RationalTime, SlurId, StaffId, TimeSignatureId};
    use epiphany_determinism::fuzz::SplitMix64;

    fn ev(n: u64) -> EventId {
        EventId::new(ReplicaId(7), n)
    }
    fn pi(n: u64) -> PitchId {
        PitchId::new(ReplicaId(7), n)
    }
    fn rg() -> RegionId {
        RegionId::new(ReplicaId(7), 1)
    }
    fn si() -> StaffInstanceId {
        StaffInstanceId::new(ReplicaId(7), 1)
    }
    fn pos(n: i32) -> MusicalPosition {
        MusicalPosition(RationalTime::from_int(n))
    }

    /// One representative operation per tag. The `match` is **exhaustive over
    /// `OperationKindTag`**, so a new kind cannot be added without giving the
    /// round-trip a sample of it — which is the compile-time half of the same
    /// guarantee `operation_kind_tag_vocabulary!` gives the decoder.
    pub(crate) fn sample_kind(tag: OperationKindTag) -> OperationKind {
        match tag {
            OperationKindTag::InsertEvent => OperationKind::InsertEvent(InsertEventOp {
                staff_instance: si(),
                event: valuegen::insert_event_value(
                    ev(1),
                    VoiceId::new(ReplicaId(7), 1),
                    pos(0),
                    MusicalDuration::whole(),
                    &[pi(1)],
                ),
            }),
            OperationKindTag::DeleteEvent => OperationKind::DeleteEvent(DeleteEventOp {
                event: ev(1),
                tuplet_compensation: TupletCompensation::RewriteTuplets {
                    tuplets: vec![TupletId::new(ReplicaId(7), 1)],
                },
            }),
            OperationKindTag::RespellPitch => OperationKind::RespellPitch(RespellPitchOp {
                pitch: pi(1),
                spelling: valuegen::spelling(2),
            }),
            OperationKindTag::CreateCrossCutting => {
                OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: CrossCuttingValue::Beam(valuegen::beam(
                        BeamId::new(ReplicaId(7), 1),
                        vec![ev(1), ev(2)],
                    )),
                })
            }
            // The generator never emits this one, so its `PositionRemapping`
            // decoder was reached by nothing until this sample existed.
            OperationKindTag::ChangeRegionTimeModel => {
                OperationKind::ChangeRegionTimeModel(ChangeRegionTimeModelOp {
                    region: rg(),
                    new_time_model: valuegen::proportional_model(),
                    declared_incompatible: vec![ev(1), ev(2)],
                    remapping: PositionRemapping::Reassign(vec![(ev(1), pos(0)), (ev(2), pos(4))]),
                })
            }
            OperationKindTag::SetUserSystemBreak => {
                OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
                    region: rg(),
                    anchor: valuegen::region_start_anchor(rg(), pos(4)),
                    present: true,
                })
            }
            // Nor this one: its NFC string and optional category were untested.
            OperationKindTag::DeclareTransaction => {
                OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: TransactionId::new(ReplicaId(7), 1),
                    label: "a transaction".to_string(),
                    category: Some(TransactionCategory::NoteEntry),
                })
            }
            OperationKindTag::Registered(_) => OperationKind::Registered(
                OperationKindRegistryId(0x0123_4567_89AB_CDEF),
                vec![1, 2, 3, 4],
            ),
            OperationKindTag::ModifyEvent => OperationKind::ModifyEvent(ModifyEventOp {
                event: valuegen::insert_event_value(
                    ev(1),
                    VoiceId::new(ReplicaId(7), 1),
                    pos(0),
                    MusicalDuration::whole(),
                    &[],
                ),
            }),
            OperationKindTag::Transpose => OperationKind::Transpose(TransposeOp {
                targets: vec![pi(1), pi(2)],
                chromatic_steps: -3,
            }),
            OperationKindTag::TransposeInterval => {
                OperationKind::TransposeInterval(TransposeIntervalOp {
                    targets: [pi(1), pi(2)].into_iter().collect(),
                    interval: TranspositionInterval {
                        diatonic_steps: 4,
                        chromatic_steps: 7,
                    },
                })
            }
            OperationKindTag::InsertIdentifiedPitch => {
                OperationKind::InsertIdentifiedPitch(InsertIdentifiedPitchOp {
                    event: ev(1),
                    pitch: valuegen::identified_pitch(pi(1)),
                })
            }
            OperationKindTag::DeleteIdentifiedPitch => {
                OperationKind::DeleteIdentifiedPitch(DeleteIdentifiedPitchOp { pitch: pi(1) })
            }
            OperationKindTag::ModifyIdentifiedPitch => {
                OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                    pitch: pi(1),
                    value: valuegen::pitch_value(),
                })
            }
            OperationKindTag::DeleteCrossCutting => {
                OperationKind::DeleteCrossCutting(DeleteCrossCuttingOp {
                    structure: TypedObjectId::Slur(SlurId::new(ReplicaId(7), 1)),
                })
            }
            OperationKindTag::ModifyCrossCutting => {
                OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                    structure: CrossCuttingValue::Slur(valuegen::slur(
                        SlurId::new(ReplicaId(7), 1),
                        ev(1),
                        ev(2),
                    )),
                })
            }
            OperationKindTag::InsertRegion => OperationKind::CreateRegion(CreateRegionOp {
                region: valuegen::region(rg()),
            }),
            OperationKindTag::DeleteRegion => {
                OperationKind::DeleteRegion(DeleteRegionOp { region: rg() })
            }
            OperationKindTag::InsertStaffInstance => {
                OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                    region: rg(),
                    instance: valuegen::staff_instance(si(), StaffId::new(ReplicaId(7), 1)),
                })
            }
            OperationKindTag::DeleteStaffInstance => {
                OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
                    staff_instance: si(),
                })
            }
            OperationKindTag::CreateVoice => OperationKind::CreateVoice(CreateVoiceOp {
                staff_instance: si(),
                voice: valuegen::voice(VoiceId::new(ReplicaId(7), 1)),
            }),
            OperationKindTag::DeleteVoice => OperationKind::DeleteVoice(DeleteVoiceOp {
                voice: VoiceId::new(ReplicaId(7), 1),
            }),
            OperationKindTag::SetMetadata => OperationKind::SetMetadata(SetMetadataOp {
                metadata: valuegen::score_metadata(1),
            }),
            OperationKindTag::SetMetricGrid => OperationKind::SetMetricGrid(SetMetricGridOp {
                region: rg(),
                grid: Some(valuegen::metric_grid()),
            }),
            OperationKindTag::SetUserPageBreak => {
                OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                    region: rg(),
                    anchor: valuegen::region_start_anchor(rg(), pos(8)),
                    present: false,
                })
            }
            OperationKindTag::InsertStaff => OperationKind::CreateStaff(CreateStaffOp {
                staff: valuegen::staff(
                    StaffId::new(ReplicaId(7), 1),
                    InstrumentId::new(ReplicaId(7), 1),
                ),
            }),
            OperationKindTag::SetTimeSignature => {
                OperationKind::SetTimeSignature(SetTimeSignatureOp {
                    region: rg(),
                    anchor: valuegen::region_start_anchor(rg(), pos(0)),
                    time_signature: Some(valuegen::time_signature(
                        TimeSignatureId::new(ReplicaId(7), 1),
                        3,
                    )),
                })
            }
            OperationKindTag::SetTempoSegment => {
                OperationKind::SetTempoSegment(SetTempoSegmentOp {
                    region: Some(rg()),
                    start: valuegen::region_start_anchor(rg(), pos(0)),
                    segment: Some(valuegen::tempo_segment(rg(), pos(0), 90.0)),
                })
            }
            OperationKindTag::SetStaffLayout => OperationKind::SetStaffLayout(SetStaffLayoutOp {
                staff_instance: si(),
                instrument_override: Some(InstrumentId::new(ReplicaId(7), 1)),
                staff_lines_override: Some(StaffLineConfiguration::default()),
                visible: true,
            }),
            OperationKindTag::CreateRepeatStructure => {
                OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
                    repeat: valuegen::volta_repeat(
                        RepeatStructureId::new(ReplicaId(7), 1),
                        ev(1),
                        ev(2),
                    ),
                })
            }
            OperationKindTag::DeleteRepeatStructure => {
                OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp {
                    repeat: RepeatStructureId::new(ReplicaId(7), 1),
                })
            }
        }
    }

    fn envelope(payload: OperationPayload) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(7), 1);
        OperationEnvelope {
            id,
            author: AuthorId(0x1122_3344),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(42), 7), id),
            causal_context: CausalContext::new()
                .with_seen(ReplicaId(1), 3)
                .with_dot(OperationId::new(ReplicaId(2), 9)),
            transaction: Some(TransactionId::new(ReplicaId(7), 5)),
            payload,
        }
    }

    fn assert_round_trips(env: &OperationEnvelope) {
        let bytes = env.to_canonical_bytes();
        let decoded = decode_envelope(&bytes)
            .unwrap_or_else(|e| panic!("{:?} must decode: {e}", env.payload));
        assert_eq!(&decoded, env, "value round-trip");
        assert_eq!(decoded.to_canonical_bytes(), bytes, "byte round-trip");
    }

    /// Every operation kind, and every payload variant, round-trips.
    ///
    /// Driven from `OperationKindTag::PAYLOAD_FREE` plus `Registered`, so it
    /// cannot silently stop covering the vocabulary.
    #[test]
    fn every_operation_kind_and_payload_round_trips() {
        for tag in OperationKindTag::PAYLOAD_FREE {
            assert_round_trips(&envelope(OperationPayload::Primitive(sample_kind(*tag))));
        }
        assert_round_trips(&envelope(OperationPayload::Primitive(sample_kind(
            OperationKindTag::Registered(OperationKindRegistryId(1)),
        ))));

        // The three meta payloads. `gen_envelope_set` emits none of them, so
        // their decoders were reached by nothing.
        assert_round_trips(&envelope(OperationPayload::ResolveConflict(
            ResolveConflictPayload {
                target: ConflictId(0xDEAD_BEEF),
                action: ResolutionAction::Reanchor {
                    new_target: TypedObjectId::Event(ev(3)),
                },
            },
        )));
        assert_round_trips(&envelope(OperationPayload::UndoTransaction(
            UndoTransactionPayload {
                target: TransactionId::new(ReplicaId(7), 5),
                policy: UndoPolicy::BestEffort,
            },
        )));
        assert_round_trips(&envelope(OperationPayload::ResolveEquivocation(
            ResolveEquivocationPayload {
                target: OperationId::new(ReplicaId(7), 2),
                chosen: EnvelopeHash([9; 32]),
            },
        )));
    }

    /// Replaces the **last** occurrence of `find` with `with` (equal lengths).
    ///
    /// Last, not first: a `PitchId` and an `OperationId` with the same replica
    /// and counter have identical canonical bytes, so `pi(1)` also matches the
    /// envelope's own leading id. The targets are the last thing in these
    /// payloads.
    fn patch(bytes: &[u8], find: &[u8], with: &[u8]) -> Vec<u8> {
        assert_eq!(find.len(), with.len());
        let at = bytes
            .windows(find.len())
            .rposition(|w| w == find)
            .expect("the needle occurs");
        let mut out = bytes.to_vec();
        out[at..at + find.len()].copy_from_slice(with);
        out
    }

    /// `TransposeInterval.targets` is a **set**: `seq^⇑`, strictly increasing.
    /// A duplicate is rejected, never silently absorbed by the `BTreeSet` it
    /// collects into — that is the rule Push 4a wrote into the wire table and
    /// left for whoever built this decoder.
    #[test]
    fn transpose_interval_targets_reject_duplicates_and_disorder() {
        let env = envelope(OperationPayload::Primitive(sample_kind(
            OperationKindTag::TransposeInterval,
        )));
        let bytes = env.to_canonical_bytes();
        assert!(decode_envelope(&bytes).is_ok());

        let (a, b) = (pi(1).to_canonical_bytes(), pi(2).to_canonical_bytes());

        // [p1, p1]: a duplicate. Collecting into a `BTreeSet` would swallow it.
        let duplicated = patch(&bytes, &b, &a);
        assert_eq!(
            decode_envelope(&duplicated),
            Err(EnvelopeDecodeError::TargetsNotStrictlyIncreasing)
        );

        // [p2, p1]: out of order. A `BTreeSet` would re-sort it.
        let swapped = patch(&patch(&bytes, &a, &[0xEE; 16]), &b, &a);
        let swapped = patch(&swapped, &[0xEE; 16], &b);
        assert_eq!(
            decode_envelope(&swapped),
            Err(EnvelopeDecodeError::TargetsNotStrictlyIncreasing)
        );
    }

    /// The frozen `Transpose` is a **multiset**: non-decreasing, duplicates
    /// preserved. Its wire form must keep decoding exactly as it always has.
    #[test]
    fn the_frozen_transpose_accepts_duplicate_targets_but_not_disorder() {
        let env = envelope(OperationPayload::Primitive(sample_kind(
            OperationKindTag::Transpose,
        )));
        let bytes = env.to_canonical_bytes();
        let (a, b) = (pi(1).to_canonical_bytes(), pi(2).to_canonical_bytes());

        // [p1, p1]: a duplicate is legal here, and must survive the round trip.
        let duplicated = patch(&bytes, &b, &a);
        let decoded = decode_envelope(&duplicated).expect("a multiset accepts duplicates");
        match &decoded.payload {
            OperationPayload::Primitive(OperationKind::Transpose(op)) => {
                assert_eq!(op.targets, vec![pi(1), pi(1)], "both targets survive");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(decoded.to_canonical_bytes(), duplicated);

        // [p2, p1]: still not canonical.
        let swapped = patch(&patch(&bytes, &a, &[0xEE; 16]), &b, &a);
        let swapped = patch(&swapped, &[0xEE; 16], &b);
        assert_eq!(
            decode_envelope(&swapped),
            Err(EnvelopeDecodeError::TargetsNotSorted)
        );
    }

    /// `ChangeRegionTimeModel.declared_incompatible` is written **sorted** by its
    /// encoder and read back verbatim, so an unsorted encoding decodes fine and
    /// only the whole-envelope re-encode guard rejects it.
    ///
    /// Nothing else can: the round-trip tests feed canonical bytes by
    /// construction, so removing the guard leaves them green (verified). This is
    /// the test that locks it.
    #[test]
    fn an_unsorted_sequence_is_rejected_by_the_whole_envelope_guard() {
        let env = envelope(OperationPayload::Primitive(sample_kind(
            OperationKindTag::ChangeRegionTimeModel,
        )));
        let bytes = env.to_canonical_bytes();
        assert!(decode_envelope(&bytes).is_ok());

        // `declared_incompatible` is a `push_seq`: count, then each element as
        // `u32` length + 16 bytes. Locate the two framed elements together —
        // a bare id search would hit the envelope's own leading id, which has
        // the same canonical bytes as `ev(1)`.
        let (a, b) = (ev(1).to_canonical_bytes(), ev(2).to_canonical_bytes());
        let mut framed = Vec::new();
        framed.extend_from_slice(&16u32.to_le_bytes());
        framed.extend_from_slice(&a);
        framed.extend_from_slice(&16u32.to_le_bytes());
        framed.extend_from_slice(&b);
        let at = bytes
            .windows(framed.len())
            .position(|w| w == framed)
            .expect("the framed pair occurs exactly here");

        let mut swapped = bytes.clone();
        swapped[at + 4..at + 20].copy_from_slice(&b);
        swapped[at + 24..at + 40].copy_from_slice(&a);
        assert_ne!(swapped, bytes);

        assert_eq!(
            decode_envelope(&swapped),
            Err(EnvelopeDecodeError::NonCanonical),
            "an unsorted sequence must be rejected, never silently re-sorted"
        );
    }

    #[test]
    fn the_decoder_rejects_trailing_truncated_and_garbage_counts() {
        let env = envelope(OperationPayload::Primitive(sample_kind(
            OperationKindTag::DeleteRegion,
        )));
        let bytes = env.to_canonical_bytes();
        assert!(decode_envelope(&bytes).is_ok());

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            decode_envelope(&trailing),
            Err(EnvelopeDecodeError::TrailingBytes)
        );

        let truncated = &bytes[..bytes.len() - 1];
        assert!(decode_envelope(truncated).is_err());

        // The causal context's vector count sits at a known offset: id(16) +
        // author(16) + stamp(12+16). A garbage count must be rejected against
        // the bytes remaining, never pre-allocated for.
        let mut huge = bytes.clone();
        huge[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            decode_envelope(&huge),
            Err(EnvelopeDecodeError::CountExceedsRemaining { .. })
        ));
    }

    /// Every envelope the generator can produce round-trips, at scale.
    ///
    /// This is 4,000 envelopes and it reaches only 28 of 31 kinds and one of
    /// four payload variants — which is why the exhaustive test above exists.
    /// The coverage is asserted so the gap cannot widen unnoticed.
    #[test]
    fn every_generated_envelope_round_trips() {
        let mut rng = SplitMix64::new(0x0DEC_0DE0_E4E1_0001);
        let envelopes = crate::fuzz::gen_envelope_set(&mut rng, 4_000);
        let mut kinds = std::collections::BTreeSet::new();
        for env in &envelopes {
            if let OperationPayload::Primitive(k) = &env.payload {
                kinds.insert(k.tag());
            }
            assert_round_trips(env);
        }
        assert!(
            kinds.len() >= 28,
            "the generator reached only {} kinds",
            kinds.len()
        );
    }
}
