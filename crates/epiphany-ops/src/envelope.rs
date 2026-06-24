//! Operation envelopes, their canonical hash, and the well-formedness contract
//! (Chapter 6 §"Operation Envelopes", §"Envelope Acceptance").
//!
//! An operation is transmitted, stored, and reduced as an *envelope*: the
//! payload together with its identity, ordering stamp, causal context, and
//! optional transaction grouping. The [`EnvelopeHash`] is the BLAKE3-256 of the
//! envelope's canonical serialization under the `MUSCENVH` domain tag; it is
//! what the [`OperationSlot`](crate::OperationSlot) compares to detect
//! equivocation (two distinct canonical envelopes under one
//! [`OperationId`](epiphany_core::OperationId)).
//!
//! [`well_formed`] is the reception gate (Chapter 6 §6.4): an envelope that is
//! not well-formed is rejected and never enters the canonical operation set.
//! Crucially, acceptance does **not** trust stamp *content* — a peer may emit
//! implausible future times — it only checks the structural invariants,
//! foremost `stamp.id == id`.

use epiphany_core::{OperationId, TransactionId};
use epiphany_determinism::{CanonicalEncode, DomainTag, Preimage};

use crate::causal::CausalContext;
use crate::encode::{push_canon, push_tag};
use crate::payload::OperationPayload;
use crate::stamp::OperationStamp;
use crate::support::AuthorId;

/// The BLAKE3-256 hash of a canonical envelope serialization under the
/// `MUSCENVH` domain tag (Chapter 6 §6.5). Ordered lexicographically on the 32
/// bytes — the order an `Equivocated` slot's `candidates` set enumerates in.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EnvelopeHash(pub [u8; 32]);

impl EnvelopeHash {
    /// The raw 32 bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex (64 chars), for diagnostics.
    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(64);
        for &b in &self.0 {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0x0f) as usize] as char);
        }
        s
    }
}

impl core::fmt::Debug for EnvelopeHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "EnvelopeHash({}…)", &self.to_hex()[..8])
    }
}

impl CanonicalEncode for EnvelopeHash {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}

/// An operation envelope (Chapter 6 §"Operation Envelopes").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OperationEnvelope {
    /// Stable identifier of this operation.
    pub id: OperationId,
    /// Author (may differ from the replica in shared authoring sessions).
    pub author: AuthorId,
    /// Ordering stamp. For canonical reduction order; never for identity.
    pub stamp: OperationStamp,
    /// Compact causal context (dotted version vector).
    pub causal_context: CausalContext,
    /// Optional transaction grouping; members sharing a `TransactionId` reduce
    /// atomically.
    pub transaction: Option<TransactionId>,
    /// The mutation itself.
    pub payload: OperationPayload,
}

impl OperationEnvelope {
    /// The canonical [`EnvelopeHash`] (`BLAKE3(MUSCENVH || canonical_bytes)`).
    pub fn envelope_hash(&self) -> EnvelopeHash {
        let mut p = Preimage::new(DomainTag::ENVELOPE);
        p.push_bytes(&self.to_canonical_bytes());
        EnvelopeHash(*p.finish().as_bytes())
    }
}

impl CanonicalEncode for OperationEnvelope {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.id);
        push_canon(out, &self.author);
        push_canon(out, &self.stamp);
        push_canon(out, &self.causal_context);
        match &self.transaction {
            None => push_tag(out, 0),
            Some(t) => {
                push_tag(out, 1);
                push_canon(out, t);
            }
        }
        push_canon(out, &self.payload);
    }
}

/// Why an envelope failed the well-formedness check (Chapter 6 §6.4). A
/// rejected envelope is recorded in the local diagnostic log but does not enter
/// the canonical operation set.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WellFormednessError {
    /// The operation id's replica is the reserved `SYSTEM_DERIVED` namespace;
    /// operations are not authored by the system namespace.
    SystemDerivedReplica,
    /// `stamp.id` is not byte-identical to the top-level `id`: the stamp
    /// addresses a different operation than the envelope's identity declares.
    StampIdMismatch,
    /// `stamp.hlc.physical_time` is negative (it must be finite and
    /// non-negative; the integer encoding is always finite).
    NegativePhysicalTime,
    /// The causal context's vector references the reserved `SYSTEM_DERIVED`
    /// replica namespace, which never authors operations.
    MalformedCausalReplica,
    /// A causal dot is not a well-formed `OperationId` (its replica is the
    /// reserved `SYSTEM_DERIVED` namespace).
    MalformedDot,
}

impl core::fmt::Display for WellFormednessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            WellFormednessError::SystemDerivedReplica => {
                "operation id uses the reserved SYSTEM_DERIVED replica namespace"
            }
            WellFormednessError::StampIdMismatch => "stamp.id does not equal the envelope id",
            WellFormednessError::NegativePhysicalTime => "stamp physical_time is negative",
            WellFormednessError::MalformedCausalReplica => {
                "causal context references the SYSTEM_DERIVED replica namespace"
            }
            WellFormednessError::MalformedDot => {
                "a causal dot uses the SYSTEM_DERIVED replica namespace"
            }
        };
        f.write_str(msg)
    }
}

impl std::error::Error for WellFormednessError {}

/// Checks an incoming envelope for well-formedness (Chapter 6 §6.4). Returns
/// `Ok(())` if it may enter the operation set, or the first structural failure.
///
/// The check is *structural only*: it never validates the plausibility of the
/// HLC content (a peer may legitimately emit implausible future times), and it
/// never depends on arrival order — two replicas observing the same envelope
/// reach the same verdict.
pub fn well_formed(env: &OperationEnvelope) -> Result<(), WellFormednessError> {
    // OperationId replica is not the system namespace.
    if env.id.replica.is_system_derived() {
        return Err(WellFormednessError::SystemDerivedReplica);
    }
    // stamp.id is byte-identical to the top-level id.
    if env.stamp.id != env.id {
        return Err(WellFormednessError::StampIdMismatch);
    }
    // physical_time is finite (always, for the integer encoding) and non-negative.
    if env.stamp.hlc.physical_time.0 < 0 {
        return Err(WellFormednessError::NegativePhysicalTime);
    }
    // Causal context references only well-formed replica identifiers …
    if env
        .causal_context
        .vector
        .keys()
        .any(|r| r.is_system_derived())
    {
        return Err(WellFormednessError::MalformedCausalReplica);
    }
    // … and its dots are well-formed OperationIds.
    if env
        .causal_context
        .dots()
        .any(|d| d.replica.is_system_derived())
    {
        return Err(WellFormednessError::MalformedDot);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::{OperationKind, RespellPitchOp};
    use crate::stamp::HybridLogicalClock;
    use epiphany_core::{PitchId, ReplicaId, WallClockTime};

    fn env(id: OperationId, stamp_id: OperationId) -> OperationEnvelope {
        OperationEnvelope {
            id,
            author: AuthorId(1),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(10), 0), stamp_id),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                pitch: PitchId::new(ReplicaId(1), 1),
                spelling: crate::valuegen::spelling(7),
            })),
        }
    }

    #[test]
    fn well_formed_requires_stamp_id_equals_id() {
        let good = env(
            OperationId::new(ReplicaId(1), 5),
            OperationId::new(ReplicaId(1), 5),
        );
        assert_eq!(well_formed(&good), Ok(()));
        let bad = env(
            OperationId::new(ReplicaId(1), 5),
            OperationId::new(ReplicaId(1), 6),
        );
        assert_eq!(well_formed(&bad), Err(WellFormednessError::StampIdMismatch));
    }

    #[test]
    fn well_formed_rejects_system_derived_author_replica() {
        let id = OperationId::new(ReplicaId::SYSTEM_DERIVED, 1);
        let e = env(id, id);
        assert_eq!(
            well_formed(&e),
            Err(WellFormednessError::SystemDerivedReplica)
        );
    }

    #[test]
    fn well_formed_rejects_negative_physical_time() {
        let mut e = env(
            OperationId::new(ReplicaId(1), 5),
            OperationId::new(ReplicaId(1), 5),
        );
        e.stamp.hlc.physical_time = WallClockTime(-1);
        assert_eq!(
            well_formed(&e),
            Err(WellFormednessError::NegativePhysicalTime)
        );
    }

    #[test]
    fn envelope_hash_changes_with_payload_bytes() {
        let mut a = env(
            OperationId::new(ReplicaId(1), 5),
            OperationId::new(ReplicaId(1), 5),
        );
        let ha = a.envelope_hash();
        a.payload = OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
            pitch: PitchId::new(ReplicaId(1), 1),
            spelling: crate::valuegen::spelling(9), // different spelling
        }));
        assert_ne!(ha, a.envelope_hash());
    }
}
