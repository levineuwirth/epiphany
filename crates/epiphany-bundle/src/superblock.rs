//! The dual superblock slots and the selection rule (Chapter 8 §"The
//! Superblock Slots", §"Superblock Selection").
//!
//! The superblocks are *the only mutable on-disk objects in the bundle*. A
//! commit flips the active superblock by writing the currently-inactive slot
//! and durably flushing it; that flush is the commit point. Recovery is just:
//! read both slots, validate, select the highest valid generation. The CRC over
//! each slot is what lets a reader reject a torn write and fall back.
//!
//! Layout of a 256-byte slot (little-endian; see `DECISIONS.md`):
//!
//! | range     | field                       |
//! |-----------|-----------------------------|
//! | `0..8`    | magic `"MUSCSUPR"`          |
//! | `8..16`   | `generation` (u64)          |
//! | `16..24`  | `manifest_offset` (u64)     |
//! | `24..32`  | `manifest_length` (u64)     |
//! | `32..64`  | `manifest_hash` (32 bytes)  |
//! | `64..68`  | `manifest_schema_version`   |
//! | `68..72`  | `reduction_algorithm_version` |
//! | `72..92`  | `profile_id`                |
//! | `92..100` | `commit_state`              |
//! | `100..108`| `commit_timestamp` (i64)    |
//! | `108..252`| reserved (zero)             |
//! | `252..256`| `superblock_crc` (CRC-32C of `0..252`) |

use crate::codec::{DecodeError, Reader, Writer};
use crate::crc::crc32c;
use crate::error::{BundleError, IntegrityAnomaly};
use crate::ids::{ProfileRegistryId, ReductionAlgorithmVersion, SchemaVersion, WallClockTime};
use epiphany_determinism::ContentHash;

/// The fixed length of each superblock slot, in bytes.
pub const SUPERBLOCK_LEN: u64 = 256;

/// Byte range covered by the superblock CRC: everything before the CRC field.
const SUPERBLOCK_CRC_RANGE: usize = 252;

/// Which physical slot a superblock occupies.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Slot {
    /// Slot A, at offset 64.
    A,
    /// Slot B, at offset 320.
    B,
}

impl Slot {
    /// The file offset of this slot.
    #[inline]
    pub const fn offset(self) -> u64 {
        match self {
            Slot::A => crate::header::SLOT_A_OFFSET,
            Slot::B => crate::header::SLOT_B_OFFSET,
        }
    }

    /// The other slot (the commit target when this one is active).
    #[inline]
    pub const fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }
}

/// The conformance profile a bundle (or chunk) declares (Chapter 8
/// §"Format Profiles").
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ProfileId {
    /// Full profile: all features, no restrictions.
    Full,
    /// Read-only profile: openable by readers that cannot edit.
    ReadOnly,
    /// Lite profile: reduced feature set for embedded/mobile readers.
    Lite,
    /// Custom profile, identified by a registry id.
    Custom(ProfileRegistryId),
}

impl ProfileId {
    #[inline]
    fn discriminant(self) -> u32 {
        match self {
            ProfileId::Full => 0,
            ProfileId::ReadOnly => 1,
            ProfileId::Lite => 2,
            ProfileId::Custom(_) => 3,
        }
    }

    /// Encodes as a fixed 20 bytes: a `u32` discriminant plus a 16-byte registry
    /// id (zero unless `Custom`). Fixed width so the superblock layout is fixed.
    pub(crate) fn encode(self, w: &mut Writer) {
        w.put_u32(self.discriminant());
        match self {
            ProfileId::Custom(reg) => reg.encode(w),
            _ => ProfileRegistryId::default().encode(w),
        }
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let disc = r.get_u32()?;
        let reg = ProfileRegistryId::decode(r)?;
        Ok(match disc {
            0 => ProfileId::Full,
            1 => ProfileId::ReadOnly,
            2 => ProfileId::Lite,
            3 => ProfileId::Custom(reg),
            other => {
                return Err(DecodeError::InvalidDiscriminant {
                    what: "ProfileId",
                    value: other as u64,
                })
            }
        })
    }
}

/// Commit state of a superblock (Chapter 8). Distinguishes a fully-committed
/// superblock from a written-but-not-completed one. Writers in this format
/// version MUST produce only `Committed`; a non-`Committed` slot is invalid for
/// ordinary selection.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CommitState {
    /// Fully committed: the referenced manifest is canonical for this generation.
    Committed,
    /// Reserved for future use; never produced by this version's writers.
    Reserved(u32),
}

impl CommitState {
    /// Whether this state is admissible for ordinary superblock selection.
    #[inline]
    pub fn is_committed(self) -> bool {
        matches!(self, CommitState::Committed)
    }

    fn encode(self, w: &mut Writer) {
        match self {
            CommitState::Committed => {
                w.put_u32(0).put_u32(0);
            }
            CommitState::Reserved(v) => {
                w.put_u32(1).put_u32(v);
            }
        }
    }

    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let tag = r.get_u32()?;
        let value = r.get_u32()?;
        Ok(match tag {
            0 => CommitState::Committed,
            _ => CommitState::Reserved(value),
        })
    }
}

/// A parsed superblock (the 256-byte slot minus magic and CRC framing).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Superblock {
    /// Generation counter; the active superblock has the highest valid value.
    pub generation: u64,
    /// Offset of this generation's manifest chunk.
    pub manifest_offset: u64,
    /// Length of the manifest chunk (also its uncompressed length — the
    /// manifest is mandatory-uncompressed in this version).
    pub manifest_length: u64,
    /// BLAKE3 content hash of the manifest chunk.
    pub manifest_hash: ContentHash,
    /// Schema version of the manifest at this generation.
    pub manifest_schema_version: SchemaVersion,
    /// Reduction-algorithm version of any canonical base in this manifest.
    pub reduction_algorithm_version: ReductionAlgorithmVersion,
    /// Profile under which this superblock is valid.
    pub profile_id: ProfileId,
    /// Commit state.
    pub commit_state: CommitState,
    /// Advisory commit timestamp (selection never consults it).
    pub commit_timestamp: WallClockTime,
}

/// Why a slot was rejected for ordinary selection.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SlotReject {
    /// Magic bytes were not `"MUSCSUPR"` (empty/garbage/foreign slot).
    BadMagic,
    /// The slot CRC did not match (a torn write — the central recovery case).
    CrcMismatch,
    /// The slot parsed but its commit state was not `Committed`.
    NotCommitted,
    /// The slot's bytes did not decode into a superblock.
    Malformed(DecodeError),
}

/// The result of parsing one slot for *ordinary* selection.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SlotParse {
    /// A valid, committed superblock.
    Valid(Superblock),
    /// Rejected; not usable for ordinary selection.
    Rejected(SlotReject),
}

impl Superblock {
    /// Serializes to the fixed 256-byte slot form, appending the CRC.
    pub fn encode(&self) -> [u8; SUPERBLOCK_LEN as usize] {
        let mut w = Writer::with_capacity(SUPERBLOCK_LEN as usize);
        w.put_bytes(&epiphany_determinism::SUPERBLOCK_MAGIC);
        w.put_u64(self.generation);
        w.put_u64(self.manifest_offset);
        w.put_u64(self.manifest_length);
        w.put_bytes(self.manifest_hash.as_bytes());
        self.manifest_schema_version.encode(&mut w);
        self.reduction_algorithm_version.encode(&mut w);
        self.profile_id.encode(&mut w);
        self.commit_state.encode(&mut w);
        self.commit_timestamp.encode(&mut w);

        let mut buf = [0u8; SUPERBLOCK_LEN as usize];
        let body = w.as_bytes();
        debug_assert!(body.len() <= SUPERBLOCK_CRC_RANGE);
        buf[..body.len()].copy_from_slice(body);
        // bytes body.len()..252 stay zero (reserved padding).
        let crc = crc32c(&buf[0..SUPERBLOCK_CRC_RANGE]);
        buf[SUPERBLOCK_CRC_RANGE..].copy_from_slice(&crc.to_le_bytes());
        buf
    }

    /// Parses a 256-byte slot for *ordinary* selection: checks magic, then CRC,
    /// then decodes the fields, then requires `commit_state == Committed`
    /// (Chapter 8 §"Superblock Selection", steps 2–3). Each failure is reported
    /// as a [`SlotReject`], never a panic.
    pub fn parse_slot(bytes: &[u8]) -> SlotParse {
        if bytes.len() < SUPERBLOCK_LEN as usize {
            return SlotParse::Rejected(SlotReject::BadMagic);
        }
        let buf = &bytes[..SUPERBLOCK_LEN as usize];
        if buf[0..8] != epiphany_determinism::SUPERBLOCK_MAGIC {
            return SlotParse::Rejected(SlotReject::BadMagic);
        }
        let stored_crc = u32::from_le_bytes([
            buf[SUPERBLOCK_CRC_RANGE],
            buf[SUPERBLOCK_CRC_RANGE + 1],
            buf[SUPERBLOCK_CRC_RANGE + 2],
            buf[SUPERBLOCK_CRC_RANGE + 3],
        ]);
        if stored_crc != crc32c(&buf[0..SUPERBLOCK_CRC_RANGE]) {
            return SlotParse::Rejected(SlotReject::CrcMismatch);
        }
        match Self::decode_fields(&buf[0..SUPERBLOCK_CRC_RANGE]) {
            Ok(sb) if sb.commit_state.is_committed() => SlotParse::Valid(sb),
            Ok(_) => SlotParse::Rejected(SlotReject::NotCommitted),
            Err(e) => SlotParse::Rejected(SlotReject::Malformed(e)),
        }
    }

    /// Decodes the field region (bytes `0..252`), ignoring magic and the
    /// reserved padding. Used after magic/CRC have already been verified.
    fn decode_fields(body: &[u8]) -> Result<Superblock, DecodeError> {
        let mut r = Reader::new(body);
        let _magic = r.take_array::<8>()?;
        let generation = r.get_u64()?;
        let manifest_offset = r.get_u64()?;
        let manifest_length = r.get_u64()?;
        let manifest_hash = ContentHash(r.take_array::<32>()?);
        let manifest_schema_version = SchemaVersion::decode(&mut r)?;
        let reduction_algorithm_version = ReductionAlgorithmVersion::decode(&mut r)?;
        let profile_id = ProfileId::decode(&mut r)?;
        let commit_state = CommitState::decode(&mut r)?;
        let commit_timestamp = WallClockTime::decode(&mut r)?;
        Ok(Superblock {
            generation,
            manifest_offset,
            manifest_length,
            manifest_hash,
            manifest_schema_version,
            reduction_algorithm_version,
            profile_id,
            commit_state,
            commit_timestamp,
        })
    }
}

/// Whether two equal-generation superblocks describe the same committed state
/// for selection purposes: same manifest, same schema, same reduction-algorithm
/// version, same profile. The advisory `commit_timestamp` (and the physical
/// manifest offset/length, which a matching `manifest_hash` already pins) are
/// deliberately excluded — a difference in those does not make the states
/// divergent. RATIFIED by Pass 11 (item 3.2, P11-D1, a spec-gap fix): core_spec
/// §"Superblock Selection" now states the equal-generation rule (this
/// load-bearing field set → equivalent, pick A; otherwise
/// `DivergentSameGeneration`, read-only).
fn selection_equivalent(a: &Superblock, b: &Superblock) -> bool {
    a.manifest_hash == b.manifest_hash
        && a.manifest_schema_version == b.manifest_schema_version
        && a.reduction_algorithm_version == b.reduction_algorithm_version
        && a.profile_id == b.profile_id
}

/// The outcome of selecting the active superblock.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Selection {
    /// The selected (active) slot.
    pub slot: Slot,
    /// The selected superblock.
    pub superblock: Superblock,
    /// A structural anomaly that forces read-only recovery, if any.
    pub anomaly: Option<IntegrityAnomaly>,
}

/// Applies the Chapter 8 selection rule to the two slots' *manifest-verified*
/// validity (a `None` slot is invalid: it failed magic, CRC, commit-state, or
/// manifest-hash verification — all checked by the caller, which has store
/// access for the hash check).
///
/// This is the pure decision core, separated so every branch is unit-testable
/// without a backing store:
///
/// * neither valid → corrupt, hard error;
/// * exactly one valid → it is active;
/// * both valid, generations differ by ≤ 1, unequal → higher is active;
/// * both valid, equal generation, same manifest → equivalent, pick A;
/// * both valid, equal generation, divergent manifest → anomaly, pick A;
/// * both valid, generations differ by > 1 → anomaly, pick the higher.
pub fn select_active(
    a: Option<Superblock>,
    b: Option<Superblock>,
) -> Result<Selection, BundleError> {
    match (a, b) {
        (None, None) => Err(BundleError::NoValidSuperblock),
        (Some(sb), None) => Ok(Selection {
            slot: Slot::A,
            superblock: sb,
            anomaly: None,
        }),
        (None, Some(sb)) => Ok(Selection {
            slot: Slot::B,
            superblock: sb,
            anomaly: None,
        }),
        (Some(sa), Some(sb)) => {
            let ga = sa.generation;
            let gb = sb.generation;
            let gap = ga.abs_diff(gb);
            // Pre-resolve the higher-generation slot for the unequal cases.
            let higher = if ga >= gb {
                (Slot::A, sa)
            } else {
                (Slot::B, sb)
            };
            if ga == gb {
                if selection_equivalent(&sa, &sb) {
                    // Equivalent committed states: deterministically pick A.
                    Ok(Selection {
                        slot: Slot::A,
                        superblock: sa,
                        anomaly: None,
                    })
                } else {
                    Ok(Selection {
                        slot: Slot::A,
                        superblock: sa,
                        anomaly: Some(IntegrityAnomaly::DivergentSameGeneration { generation: ga }),
                    })
                }
            } else if gap > 1 {
                Ok(Selection {
                    slot: higher.0,
                    superblock: higher.1,
                    anomaly: Some(IntegrityAnomaly::GenerationGap {
                        active: ga.max(gb),
                        other: ga.min(gb),
                    }),
                })
            } else {
                // Normal steady state: generations differ by exactly one.
                Ok(Selection {
                    slot: higher.0,
                    superblock: higher.1,
                    anomaly: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sb(generation: u64, manifest_hash_byte: u8) -> Superblock {
        Superblock {
            generation,
            manifest_offset: 576,
            manifest_length: 10,
            manifest_hash: ContentHash([manifest_hash_byte; 32]),
            manifest_schema_version: SchemaVersion::V0,
            reduction_algorithm_version: ReductionAlgorithmVersion(0),
            profile_id: ProfileId::Full,
            commit_state: CommitState::Committed,
            commit_timestamp: WallClockTime(0),
        }
    }

    #[test]
    fn profile_id_discriminants_are_golden() {
        // RATIFIED by Pass 11 (item 1.5, req:format:profileid-discriminants):
        // ProfileId is a load-bearing superblock-selection field, so the literal
        // u32 discriminants are normative. Lock the values, not just round-trip.
        assert_eq!(ProfileId::Full.discriminant(), 0);
        assert_eq!(ProfileId::ReadOnly.discriminant(), 1);
        assert_eq!(ProfileId::Lite.discriminant(), 2);
        assert_eq!(
            ProfileId::Custom(ProfileRegistryId([0; 16])).discriminant(),
            3
        );
        // And the fixed-width encoding: a u32-LE discriminant + 16-byte registry
        // id (zero unless Custom) = 20 bytes; Full is twenty zero bytes.
        let mut w = Writer::new();
        ProfileId::Full.encode(&mut w);
        assert_eq!(w.into_bytes(), vec![0u8; 20]);
    }

    #[test]
    fn superblock_round_trips_through_256_bytes() {
        let original = Superblock {
            commit_timestamp: WallClockTime(1_700_000_000_000_000_000),
            profile_id: ProfileId::Custom(ProfileRegistryId([5; 16])),
            reduction_algorithm_version: ReductionAlgorithmVersion(3),
            ..sb(7, 9)
        };
        let bytes = original.encode();
        assert_eq!(bytes.len(), 256);
        assert_eq!(Superblock::parse_slot(&bytes), SlotParse::Valid(original));
    }

    #[test]
    fn torn_slot_fails_crc() {
        let mut bytes = sb(3, 1).encode();
        bytes[200] ^= 0xFF; // corrupt a byte inside the CRC-covered region
        assert_eq!(
            Superblock::parse_slot(&bytes),
            SlotParse::Rejected(SlotReject::CrcMismatch)
        );
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = sb(3, 1).encode();
        bytes[0] = 0;
        assert_eq!(
            Superblock::parse_slot(&bytes),
            SlotParse::Rejected(SlotReject::BadMagic)
        );
        // An all-zero slot (never written) is rejected too.
        assert_eq!(
            Superblock::parse_slot(&[0u8; 256]),
            SlotParse::Rejected(SlotReject::BadMagic)
        );
    }

    #[test]
    fn non_committed_slot_is_invalid_for_ordinary_selection() {
        let mut s = sb(4, 1);
        s.commit_state = CommitState::Reserved(99);
        let bytes = s.encode();
        assert_eq!(
            Superblock::parse_slot(&bytes),
            SlotParse::Rejected(SlotReject::NotCommitted)
        );
    }

    #[test]
    fn selection_picks_higher_generation() {
        let s = select_active(Some(sb(5, 1)), Some(sb(6, 2))).unwrap();
        assert_eq!(s.slot, Slot::B);
        assert_eq!(s.superblock.generation, 6);
        assert!(s.anomaly.is_none());

        let s = select_active(Some(sb(6, 1)), Some(sb(5, 2))).unwrap();
        assert_eq!(s.slot, Slot::A);
        assert_eq!(s.superblock.generation, 6);
    }

    #[test]
    fn selection_handles_single_valid_slot() {
        assert_eq!(select_active(Some(sb(2, 1)), None).unwrap().slot, Slot::A);
        assert_eq!(select_active(None, Some(sb(2, 1))).unwrap().slot, Slot::B);
    }

    #[test]
    fn selection_errors_when_neither_valid() {
        assert!(matches!(
            select_active(None, None),
            Err(BundleError::NoValidSuperblock)
        ));
    }

    #[test]
    fn equal_generation_same_manifest_is_equivalent() {
        let s = select_active(Some(sb(8, 7)), Some(sb(8, 7))).unwrap();
        assert_eq!(s.slot, Slot::A);
        assert!(s.anomaly.is_none());
    }

    #[test]
    fn equal_generation_divergent_manifest_is_an_anomaly() {
        let s = select_active(Some(sb(8, 1)), Some(sb(8, 2))).unwrap();
        assert_eq!(
            s.anomaly,
            Some(IntegrityAnomaly::DivergentSameGeneration { generation: 8 })
        );
    }

    #[test]
    fn generation_gap_over_one_is_an_anomaly_but_opens() {
        let s = select_active(Some(sb(2, 1)), Some(sb(9, 2))).unwrap();
        assert_eq!(s.superblock.generation, 9);
        assert_eq!(
            s.anomaly,
            Some(IntegrityAnomaly::GenerationGap {
                active: 9,
                other: 2
            })
        );
    }

    #[test]
    fn equal_generation_same_manifest_but_divergent_profile_is_an_anomaly() {
        // Same generation and manifest hash, but a different profile: the slots
        // are not interchangeable, so this is divergence, not equivalence.
        let mut a = sb(8, 7);
        let mut b = sb(8, 7);
        a.profile_id = ProfileId::Full;
        b.profile_id = ProfileId::Lite;
        let s = select_active(Some(a), Some(b)).unwrap();
        assert_eq!(
            s.anomaly,
            Some(IntegrityAnomaly::DivergentSameGeneration { generation: 8 })
        );
    }
}
