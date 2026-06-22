//! Decoder for the canonical materialized-state snapshot format.
//!
//! The operation layer owns this inverse because only it understands the
//! semantic types embedded in [`MaterializedState`](crate::MaterializedState).
//! Bundle storage deliberately treats snapshot payloads as opaque bytes.

use std::collections::BTreeMap;

use epiphany_core::{MusicalPosition, OperationId, PitchId, RegionId, TypedObjectId};
use epiphany_determinism::{CanonicalDecode, ContentHash};

use crate::{
    ConflictId, ConflictKind, ConflictKindRegistryId, ConflictRecord, ConflictRegistry,
    ConflictResolutionState, ExtensionPreconditionId, IntegrityAnomaly, IntegrityAnomalyKind,
    IntegrityAnomalyRegistryId, MaterializedState, NoOpReason, ObjectKind, ObjectState,
    OperationEffect, PendingReason, PreconditionFailureReason, PreconditionFailureRegistryId,
    ReanchorReason, ReanchorReasonRegistryId, RepairKind, RepairKindRegistryId, RepairRecord,
    ResolutionAction, ResolutionRegistryId, SerializedCanonicalInputs, TupletCompensationKind,
};

/// Failure to decode a canonical [`MaterializedState`] snapshot.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MaterializedDecodeError {
    /// A field ended before its declared or fixed width.
    UnexpectedEof,
    /// A length prefix cannot be represented safely by this process.
    LengthOverflow,
    /// A tagged union carried an unknown discriminant.
    InvalidTag { kind: &'static str, tag: u8 },
    /// A primitive value failed its own canonical decoder.
    InvalidValue(&'static str),
    /// A canonical boolean was not encoded as zero or one.
    InvalidBoolean(u8),
    /// A canonical text field was not UTF-8.
    InvalidUtf8,
    /// Bytes remained after the complete state was decoded.
    TrailingBytes,
    /// The bytes decoded structurally but were not in canonical order/form.
    NonCanonical,
}

impl core::fmt::Display for MaterializedDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of materialized-state bytes"),
            Self::LengthOverflow => f.write_str("materialized-state length does not fit usize"),
            Self::InvalidTag { kind, tag } => write!(f, "invalid {kind} tag {tag}"),
            Self::InvalidValue(kind) => write!(f, "invalid canonical {kind}"),
            Self::InvalidBoolean(value) => write!(f, "invalid canonical boolean {value}"),
            Self::InvalidUtf8 => f.write_str("invalid UTF-8 in canonical text"),
            Self::TrailingBytes => f.write_str("trailing bytes after materialized state"),
            Self::NonCanonical => f.write_str("materialized-state bytes are not canonical"),
        }
    }
}

impl std::error::Error for MaterializedDecodeError {}

type Result<T> = core::result::Result<T, MaterializedDecodeError>;

struct Reader<'a> {
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
            .ok_or(MaterializedDecodeError::LengthOverflow)?;
        if end > self.bytes.len() {
            return Err(MaterializedDecodeError::UnexpectedEof);
        }
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32_le(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("fixed width"),
        ))
    }

    fn u64_le(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("fixed width"),
        ))
    }

    fn u128_be(&mut self) -> Result<u128> {
        Ok(u128::from_be_bytes(
            self.take(16)?.try_into().expect("fixed width"),
        ))
    }

    fn len(&mut self) -> Result<usize> {
        usize::try_from(self.u32_le()?).map_err(|_| MaterializedDecodeError::LengthOverflow)
    }

    fn lp_bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.len()?;
        self.take(len)
    }

    fn seq<T>(&mut self, mut decode: impl FnMut(&[u8]) -> Result<T>) -> Result<Vec<T>> {
        let count = self.len()?;
        let mut values = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            values.push(decode(self.lp_bytes()?)?);
        }
        Ok(values)
    }

    fn finish(self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(MaterializedDecodeError::TrailingBytes)
        }
    }
}

fn exact<T>(bytes: &[u8], decode: impl FnOnce(&mut Reader<'_>) -> Result<T>) -> Result<T> {
    let mut reader = Reader::new(bytes);
    let value = decode(&mut reader)?;
    reader.finish()?;
    Ok(value)
}

fn fixed<T: CanonicalDecode>(reader: &mut Reader<'_>, n: usize, name: &'static str) -> Result<T> {
    T::decode_canonical(reader.take(n)?).map_err(|_| MaterializedDecodeError::InvalidValue(name))
}

fn operation_id(reader: &mut Reader<'_>) -> Result<OperationId> {
    fixed(reader, 16, "OperationId")
}

fn typed_object_id(reader: &mut Reader<'_>) -> Result<TypedObjectId> {
    let tag_bytes = reader
        .bytes
        .get(reader.pos..reader.pos.saturating_add(2))
        .ok_or(MaterializedDecodeError::UnexpectedEof)?;
    let tag = u16::from_be_bytes(tag_bytes.try_into().expect("two bytes"));
    let width = if tag == 27 { 34 } else { 18 };
    fixed(reader, width, "TypedObjectId")
}

fn musical_position(reader: &mut Reader<'_>) -> Result<MusicalPosition> {
    let start = reader.pos;
    reader.byte()?;
    let numer_len = reader.len()?;
    reader.take(numer_len)?;
    let denom_len = reader.len()?;
    reader.take(denom_len)?;
    MusicalPosition::decode_canonical(&reader.bytes[start..reader.pos])
        .map_err(|_| MaterializedDecodeError::InvalidValue("MusicalPosition"))
}

fn registry_id<T>(reader: &mut Reader<'_>, ctor: impl FnOnce(u128) -> T) -> Result<T> {
    Ok(ctor(reader.u128_be()?))
}

fn object_state(reader: &mut Reader<'_>) -> Result<ObjectState> {
    match reader.byte()? {
        0 => Ok(ObjectState::Live),
        1 => Ok(ObjectState::Tombstoned {
            deleted_by: operation_id(reader)?,
            minted_by: operation_id(reader)?,
        }),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "ObjectState",
            tag,
        }),
    }
}

fn pending_reason(reader: &mut Reader<'_>) -> Result<PendingReason> {
    let tag = reader.byte()?;
    let blocker = operation_id(reader)?;
    match tag {
        0 => Ok(PendingReason::MissingCausalPredecessor { missing: blocker }),
        1 => Ok(PendingReason::DependsOnEquivocated { on: blocker }),
        2 => Ok(PendingReason::DependsOnExcluded { on: blocker }),
        3 => Ok(PendingReason::DependsOnPending { on: blocker }),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "PendingReason",
            tag,
        }),
    }
}

fn precondition_reason(reader: &mut Reader<'_>) -> Result<PreconditionFailureReason> {
    match reader.byte()? {
        0 => Ok(PreconditionFailureReason::TargetMissing),
        1 => Ok(PreconditionFailureReason::TargetTombstoned),
        2 => Ok(PreconditionFailureReason::WrongRegionTimeModel),
        3 => Ok(PreconditionFailureReason::TupletCompensationInvalid),
        4 => Ok(PreconditionFailureReason::EventDurationInvalid),
        5 => Ok(PreconditionFailureReason::PositionOutsideRegion),
        6 => Ok(PreconditionFailureReason::PitchSpaceMismatch),
        7 => Ok(PreconditionFailureReason::VoiceMissing),
        8 => Ok(PreconditionFailureReason::ExtensionPrecondition(
            registry_id(reader, ExtensionPreconditionId)?,
        )),
        9 => Ok(PreconditionFailureReason::Registered(registry_id(
            reader,
            PreconditionFailureRegistryId,
        )?)),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "PreconditionFailureReason",
            tag,
        }),
    }
}

fn no_op_reason(reader: &mut Reader<'_>) -> Result<NoOpReason> {
    match reader.byte()? {
        0 => Ok(NoOpReason::TargetTombstoned),
        1 => Ok(NoOpReason::AlreadyApplied),
        2 => Ok(NoOpReason::SupersededByLaterOperation {
            superseder: operation_id(reader)?,
        }),
        3 => Ok(NoOpReason::PreconditionFailedUnderReduction {
            reason: precondition_reason(reader)?,
        }),
        4 => Ok(NoOpReason::TransactionConflict),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "NoOpReason",
            tag,
        }),
    }
}

fn reanchor_reason(reader: &mut Reader<'_>) -> Result<ReanchorReason> {
    match reader.byte()? {
        0 => Ok(ReanchorReason::SameVoiceNearer),
        1 => Ok(ReanchorReason::SameStaffInstanceNearer),
        2 => Ok(ReanchorReason::SameStaffNearer),
        3 => Ok(ReanchorReason::SameRegionNearer),
        4 => Ok(ReanchorReason::ExplicitFallback),
        5 => Ok(ReanchorReason::DeclaredByExtension(registry_id(
            reader,
            ReanchorReasonRegistryId,
        )?)),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "ReanchorReason",
            tag,
        }),
    }
}

fn repair_kind(reader: &mut Reader<'_>) -> Result<RepairKind> {
    match reader.byte()? {
        0 => Ok(RepairKind::Reanchored {
            from: typed_object_id(reader)?,
            to: typed_object_id(reader)?,
            reason: reanchor_reason(reader)?,
        }),
        1 => Ok(RepairKind::SpannerTruncated {
            removed_members: reader.seq(|bytes| exact(bytes, typed_object_id))?,
        }),
        2 => Ok(RepairKind::Orphaned),
        3 => Ok(RepairKind::CascadeDeleted),
        4 => Ok(RepairKind::AttachmentTombstoned),
        5 => Ok(RepairKind::VoicePromoted {
            from: fixed(reader, 16, "VoiceId")?,
            to: fixed(reader, 16, "VoiceId")?,
        }),
        6 => {
            let compensation_kind = match reader.byte()? {
                0 => TupletCompensationKind::ReplaceWithRest,
                1 => TupletCompensationKind::RewriteTuplets,
                2 => TupletCompensationKind::CascadeDeleteTuplets,
                tag => {
                    return Err(MaterializedDecodeError::InvalidTag {
                        kind: "TupletCompensationKind",
                        tag,
                    })
                }
            };
            Ok(RepairKind::TupletCompensated { compensation_kind })
        }
        7 => Ok(RepairKind::Registered(registry_id(
            reader,
            RepairKindRegistryId,
        )?)),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "RepairKind",
            tag,
        }),
    }
}

fn repair_record(reader: &mut Reader<'_>) -> Result<RepairRecord> {
    Ok(RepairRecord {
        kind: repair_kind(reader)?,
        target: typed_object_id(reader)?,
    })
}

fn operation_effect(reader: &mut Reader<'_>) -> Result<OperationEffect> {
    match reader.byte()? {
        0 => Ok(OperationEffect::Applied),
        1 => Ok(OperationEffect::AppliedWithRepair {
            repairs: reader.seq(|bytes| exact(bytes, repair_record))?,
        }),
        2 => Ok(OperationEffect::Conflicted {
            conflict: ConflictId(reader.u128_be()?),
        }),
        3 => Ok(OperationEffect::TombstonedTarget {
            target: typed_object_id(reader)?,
        }),
        4 => Ok(OperationEffect::NoOp {
            reason: no_op_reason(reader)?,
        }),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "OperationEffect",
            tag,
        }),
    }
}

fn resolution_action(reader: &mut Reader<'_>) -> Result<ResolutionAction> {
    match reader.byte()? {
        0 => Ok(ResolutionAction::AcceptLoser),
        1 => Ok(ResolutionAction::KeepWinner),
        2 => Ok(ResolutionAction::Override {
            override_operation: operation_id(reader)?,
        }),
        3 => Ok(ResolutionAction::Reanchor {
            new_target: typed_object_id(reader)?,
        }),
        4 => Ok(ResolutionAction::Dismiss),
        5 => Ok(ResolutionAction::Registered(registry_id(
            reader,
            ResolutionRegistryId,
        )?)),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "ResolutionAction",
            tag,
        }),
    }
}

fn conflict_resolution(reader: &mut Reader<'_>) -> Result<ConflictResolutionState> {
    match reader.byte()? {
        0 => Ok(ConflictResolutionState::Unresolved),
        1 => Ok(ConflictResolutionState::Resolved {
            by: operation_id(reader)?,
            action: resolution_action(reader)?,
        }),
        2 => Ok(ConflictResolutionState::Dismissed {
            by: operation_id(reader)?,
        }),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "ConflictResolutionState",
            tag,
        }),
    }
}

fn conflict_kind(reader: &mut Reader<'_>) -> Result<ConflictKind> {
    match reader.byte()? {
        0 => Ok(ConflictKind::StructuralFieldCollision {
            winner: operation_id(reader)?,
            loser: operation_id(reader)?,
            field: crate::FieldPath(
                std::str::from_utf8(reader.lp_bytes()?)
                    .map_err(|_| MaterializedDecodeError::InvalidUtf8)?
                    .to_owned(),
            ),
        }),
        1 => Ok(ConflictKind::TransactionConflict {
            transaction: fixed(reader, 16, "TransactionId")?,
            failed_members: reader.seq(|bytes| exact(bytes, operation_id))?,
        }),
        2 => Ok(ConflictKind::TombstonedTarget {
            target: typed_object_id(reader)?,
            operation: operation_id(reader)?,
        }),
        3 => Ok(ConflictKind::ReanchorFailure {
            original_referent: typed_object_id(reader)?,
            referencing_object: typed_object_id(reader)?,
        }),
        4 => Ok(ConflictKind::TimeModelMigrationFailure {
            region: fixed(reader, 16, "RegionId")?,
            incompatible_events: reader.seq(|bytes| exact(bytes, typed_object_id))?,
        }),
        5 => Ok(ConflictKind::ExtensionConflict {
            kind_id: registry_id(reader, ConflictKindRegistryId)?,
            details: reader.lp_bytes()?.to_vec(),
        }),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "ConflictKind",
            tag,
        }),
    }
}

fn conflict_record(reader: &mut Reader<'_>) -> Result<ConflictRecord> {
    let record = ConflictRecord {
        id: ConflictId(reader.u128_be()?),
        caused_by: reader.seq(|bytes| exact(bytes, operation_id))?,
        kind: conflict_kind(reader)?,
        affected_objects: reader.seq(|bytes| exact(bytes, typed_object_id))?,
        resolution_state: conflict_resolution(reader)?,
    };
    let ordered_ops = record.caused_by.windows(2).all(|pair| pair[0] < pair[1]);
    let ordered_objects = record
        .affected_objects
        .windows(2)
        .all(|pair| pair[0] < pair[1]);
    let expected_id =
        crate::derive_conflict_id(&record.kind, &record.caused_by, &record.affected_objects);
    if !ordered_ops || !ordered_objects || record.id != expected_id {
        return Err(MaterializedDecodeError::NonCanonical);
    }
    Ok(record)
}

fn object_kind(reader: &mut Reader<'_>) -> Result<ObjectKind> {
    match reader.byte()? {
        0 => Ok(ObjectKind::Voice),
        1 => Ok(ObjectKind::Pitch),
        2 => Ok(ObjectKind::Registered(registry_id(
            reader,
            crate::OperationKindRegistryId,
        )?)),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "ObjectKind",
            tag,
        }),
    }
}

fn integrity_anomaly_kind(reader: &mut Reader<'_>) -> Result<IntegrityAnomalyKind> {
    match reader.byte()? {
        0 => Ok(IntegrityAnomalyKind::SystemIdentifierCollision {
            kind: object_kind(reader)?,
            colliding_counter: reader.u64_le()?,
            input_set_a: SerializedCanonicalInputs(reader.lp_bytes()?.to_vec()),
            input_set_b: SerializedCanonicalInputs(reader.lp_bytes()?.to_vec()),
        }),
        1 => Ok(IntegrityAnomalyKind::OperationSlotEquivocated {
            operation_id: operation_id(reader)?,
        }),
        2 => Ok(IntegrityAnomalyKind::ReplicaStreamQuarantined {
            replica: fixed(reader, 8, "ReplicaId")?,
            first_bad_counter: reader.u64_le()?,
        }),
        3 => Ok(IntegrityAnomalyKind::Registered(registry_id(
            reader,
            IntegrityAnomalyRegistryId,
        )?)),
        tag => Err(MaterializedDecodeError::InvalidTag {
            kind: "IntegrityAnomalyKind",
            tag,
        }),
    }
}

fn integrity_anomaly(reader: &mut Reader<'_>) -> Result<IntegrityAnomaly> {
    let anomaly = IntegrityAnomaly {
        id: fixed(reader, 16, "IntegrityAnomalyId")?,
        kind: integrity_anomaly_kind(reader)?,
    };
    if IntegrityAnomaly::new(anomaly.kind.clone()).id != anomaly.id {
        return Err(MaterializedDecodeError::NonCanonical);
    }
    Ok(anomaly)
}

pub(crate) fn decode_materialized_state(bytes: &[u8]) -> Result<MaterializedState> {
    let mut reader = Reader::new(bytes);

    let effect_count = reader.len()?;
    let mut effects = Vec::with_capacity(effect_count.min(1024));
    for _ in 0..effect_count {
        let id = operation_id(&mut reader)?;
        let effect = exact(reader.lp_bytes()?, operation_effect)?;
        effects.push((id, effect));
    }

    let records = reader.seq(|bytes| exact(bytes, conflict_record))?;
    let mut conflicts = ConflictRegistry::new();
    for record in records {
        conflicts.insert(record);
    }

    let anomaly_count = reader.len()?;
    let mut anomalies = Vec::with_capacity(anomaly_count.min(1024));
    for _ in 0..anomaly_count {
        anomalies.push(exact(reader.lp_bytes()?, integrity_anomaly)?);
    }
    if !anomalies.windows(2).all(|pair| pair[0].id < pair[1].id) {
        return Err(MaterializedDecodeError::NonCanonical);
    }

    let object_count = reader.len()?;
    let mut objects = BTreeMap::new();
    for _ in 0..object_count {
        objects.insert(typed_object_id(&mut reader)?, object_state(&mut reader)?);
    }

    let spelling_count = reader.len()?;
    let mut spellings = BTreeMap::new();
    for _ in 0..spelling_count {
        let pitch = fixed::<PitchId>(&mut reader, 16, "PitchId")?;
        let hash = fixed::<ContentHash>(&mut reader, 32, "ContentHash")?;
        spellings.insert(pitch, hash);
    }

    let break_count = reader.len()?;
    let mut breaks = BTreeMap::new();
    for _ in 0..break_count {
        let region = fixed::<RegionId>(&mut reader, 16, "RegionId")?;
        let anchor = musical_position(&mut reader)?;
        let present = match reader.byte()? {
            0 => false,
            1 => true,
            value => return Err(MaterializedDecodeError::InvalidBoolean(value)),
        };
        breaks.insert((region, anchor), present);
    }

    let pending_count = reader.len()?;
    let mut pending = Vec::with_capacity(pending_count.min(1024));
    for _ in 0..pending_count {
        pending.push((operation_id(&mut reader)?, pending_reason(&mut reader)?));
    }
    if !pending.windows(2).all(|pair| pair[0].0 < pair[1].0) {
        return Err(MaterializedDecodeError::NonCanonical);
    }
    reader.finish()?;

    let state = MaterializedState {
        effects,
        conflicts,
        anomalies,
        objects,
        spellings,
        breaks,
        pending,
    };
    if state.canonical_bytes() != bytes {
        return Err(MaterializedDecodeError::NonCanonical);
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_determinism::fuzz::SplitMix64;

    #[test]
    fn reduced_states_decode_and_reencode() {
        let mut rng = SplitMix64::new(0xDEC0_DED5);
        for _ in 0..500 {
            let envelopes = crate::fuzz::gen_envelope_set(&mut rng, 20);
            let mut set = crate::OperationSet::new();
            set.accept_all(envelopes);
            let state = set.reduce();
            let bytes = state.canonical_bytes();
            let decoded = MaterializedState::decode_canonical(&bytes).unwrap();
            assert_eq!(decoded.canonical_bytes(), bytes);
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn decoder_rejects_truncation_and_trailing_bytes() {
        let bytes = MaterializedState::default().canonical_bytes();
        assert!(MaterializedState::decode_canonical(&bytes[..bytes.len() - 1]).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            MaterializedState::decode_canonical(&trailing),
            Err(MaterializedDecodeError::TrailingBytes)
        );
    }
}
