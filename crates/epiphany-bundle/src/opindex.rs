//! The operation index (Chapter 8 §"The Operation Index").
//!
//! An optional chunk of kind [`ChunkKind::OperationIndex`] mapping each
//! operation id to the [`ChunkRef`] of its enclosing operation-envelope block
//! plus a byte offset within the block, for O(log n) lookup of an operation
//! without scanning every block. It is **an acceleration structure, not
//! canonical**: *"If absent, readers rebuild it by scanning all blocks. If
//! present but corrupt or stale, readers MUST reject the index and rebuild
//! from blocks"* — and failed verification of a non-canonical chunk is *not*
//! bundle corruption (Chapter 8 §"Canonical and Non-Canonical Manifest
//! Roots"). [`crate::Bundle::usable_operation_index`] packages that
//! reject-and-rebuild discipline.
//!
//! ## Layering
//!
//! The bundle stays semantics-free: entries key on the **raw 16 canonical
//! bytes** of an operation id, which the bundle never interprets. That the
//! leading 16 bytes of a canonically encoded envelope *are* its operation id
//! is an `epiphany-ops` invariant, vouched for by ops' `peek_operation_id`;
//! index builders pair that helper with [`crate::envelope_offsets`] to produce
//! this module's `(id bytes, offset)` inputs.
//!
//! ## Provisional payload layout (see `DECISIONS.md`)
//!
//! The spec defers the index's byte format to the Binary Format companion; the
//! encoding here is a deterministic, golden-locked prototype under the crate's
//! codec conventions:
//!
//! ```text
//! u32 block_count
//!   block_count × ChunkRef            — strictly ascending canonical order
//! u32 entry_count
//!   entry_count × { id: [u8;16], block: u32, offset: u32 }
//!                                     — strictly ascending by id bytes
//! ```
//!
//! `block` is an ordinal into the block vector; `offset` is the byte offset of
//! the envelope's first content byte within the block's *decoded*
//! (uncompressed) payload, exactly as [`crate::envelope_offsets`] reports it.
//! The decoder **rejects** non-canonical bytes (unsorted or duplicated blocks
//! or ids, out-of-range ordinals, wrong-kind block references, trailing
//! bytes) rather than normalizing — the manifest decoder's discipline.

use crate::chunk::{ChunkKind, ChunkRef};
use crate::codec::{DecodeError, Reader, Writer};

/// The raw 16 canonical bytes of an operation id. Opaque to the bundle (the
/// semantics-free layering split): ops' `peek_operation_id` is what vouches
/// that an envelope's leading 16 bytes are its id.
pub type OperationIdBytes = [u8; 16];

/// One [`OperationIndex::build`] input: an operation-envelope block's
/// [`ChunkRef`] plus, for each envelope it contains, the envelope's id bytes
/// and its offset within the decoded block payload (the
/// [`crate::envelope_offsets`] coordinate).
pub type IndexedBlock = (ChunkRef, Vec<(OperationIdBytes, u32)>);

/// One index entry: an operation id (raw canonical bytes — the bundle never
/// interprets them), the ordinal of its enclosing block in the index's block
/// vector, and the byte offset of the envelope's first content byte within
/// that block's decoded payload.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct OperationIndexEntry {
    /// The operation's 16 canonical id bytes (opaque to the bundle).
    pub id: [u8; 16],
    /// Ordinal into [`OperationIndex::blocks`] of the enclosing block.
    pub block: u32,
    /// Byte offset of the envelope's first content byte within the block's
    /// decoded payload (`crate::envelope_offsets` coordinates).
    pub offset: u32,
}

/// Why [`OperationIndex::build`] refused its inputs. Building is the writer's
/// act, so these are writer bugs — unlike a decode failure, which is just an
/// unusable (discard-and-rebuild) index.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OperationIndexBuildError {
    /// An operation id was supplied under two blocks (or twice in one block).
    /// An `OperationId` names exactly one slot, so it lives in exactly one
    /// enclosing block; two coordinates for one id is a builder bug.
    DuplicateOperationId([u8; 16]),
    /// The same block reference was supplied twice: the block set is a set.
    DuplicateBlock,
    /// A supplied block reference is not an operation-envelope block.
    NotAnOperationBlock,
}

impl core::fmt::Display for OperationIndexBuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            OperationIndexBuildError::DuplicateOperationId(id) => {
                write!(f, "operation id {id:02x?} indexed more than once")
            }
            OperationIndexBuildError::DuplicateBlock => {
                f.write_str("the same block reference was supplied twice")
            }
            OperationIndexBuildError::NotAnOperationBlock => {
                f.write_str("an index block reference is not an operation-envelope block")
            }
        }
    }
}

impl std::error::Error for OperationIndexBuildError {}

/// The decoded operation index. Construct via [`OperationIndex::build`] (the
/// writer path) or [`OperationIndex::decode`] (the reader path); both uphold
/// the canonical-order invariants [`OperationIndex::locate`]'s binary search
/// relies on.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct OperationIndex {
    /// The indexed operation-envelope blocks, in canonical [`ChunkRef`] order.
    blocks: Vec<ChunkRef>,
    /// The entries, strictly ascending by id bytes.
    entries: Vec<OperationIndexEntry>,
}

impl OperationIndex {
    /// Builds an index from per-block entry lists: for each operation-envelope
    /// block's [`ChunkRef`], the `(id bytes, offset)` of every envelope it
    /// contains (as `peek_operation_id` over [`crate::envelope_offsets`]
    /// yields them). Usable at commit time from the [`ChunkRef`]s a commit's
    /// builder closure receives. Blocks are put into canonical order and the
    /// entry ordinals remapped accordingly; duplicate ids, duplicate blocks,
    /// and wrong-kind block references are rejected.
    pub fn build(blocks: &[IndexedBlock]) -> Result<OperationIndex, OperationIndexBuildError> {
        assert!(
            blocks.len() <= u32::MAX as usize,
            "block count {} overflows the u32 ordinal space",
            blocks.len()
        );
        if blocks
            .iter()
            .any(|(r, _)| r.kind != ChunkKind::OperationEnvelopeBlock)
        {
            return Err(OperationIndexBuildError::NotAnOperationBlock);
        }

        // Canonical block order (ChunkRef's total Ord), remembering where each
        // input block landed so entry ordinals can be remapped.
        let mut order: Vec<usize> = (0..blocks.len()).collect();
        order.sort_by(|&a, &b| blocks[a].0.cmp(&blocks[b].0));
        if order.windows(2).any(|w| blocks[w[0]].0 == blocks[w[1]].0) {
            return Err(OperationIndexBuildError::DuplicateBlock);
        }
        let mut ordinal_of = vec![0u32; blocks.len()];
        for (ordinal, &input) in order.iter().enumerate() {
            ordinal_of[input] = ordinal as u32;
        }
        let sorted_blocks: Vec<ChunkRef> = order.iter().map(|&i| blocks[i].0).collect();

        let mut entries: Vec<OperationIndexEntry> = Vec::new();
        for (i, (_, ids)) in blocks.iter().enumerate() {
            entries.extend(ids.iter().map(|&(id, offset)| OperationIndexEntry {
                id,
                block: ordinal_of[i],
                offset,
            }));
        }
        entries.sort_by_key(|e| e.id);
        if let Some(w) = entries.windows(2).find(|w| w[0].id == w[1].id) {
            return Err(OperationIndexBuildError::DuplicateOperationId(w[0].id));
        }

        Ok(OperationIndex {
            blocks: sorted_blocks,
            entries,
        })
    }

    /// Encodes the index to its (provisional, golden-locked) canonical chunk
    /// payload. Deterministic: [`OperationIndex::build`]/[`OperationIndex::decode`]
    /// established the canonical orders, so re-encoding is byte-stable.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.put_seq(&self.blocks, |w, b| b.encode(w));
        w.put_seq(&self.entries, |w, e| {
            w.put_bytes(&e.id);
            w.put_u32(e.block);
            w.put_u32(e.offset);
        });
        w.into_bytes()
    }

    /// Decodes an index payload, **rejecting** (never normalizing) any
    /// non-canonical form: unsorted or duplicated blocks or entry ids, a
    /// non-operation-block reference, an out-of-range block ordinal, or
    /// trailing bytes. A rejection means the index is unusable and must be
    /// rebuilt from blocks — it is *not* bundle corruption (the index is
    /// non-canonical).
    pub fn decode(bytes: &[u8]) -> Result<OperationIndex, DecodeError> {
        let mut r = Reader::new(bytes);
        let blocks = r.get_seq(ChunkRef::decode)?;
        let entries = r.get_seq(|r| {
            Ok(OperationIndexEntry {
                id: r.take_array::<16>()?,
                block: r.get_u32()?,
                offset: r.get_u32()?,
            })
        })?;
        r.finish()?;

        if blocks
            .iter()
            .any(|b| b.kind != ChunkKind::OperationEnvelopeBlock)
        {
            return Err(DecodeError::Malformed(
                "operation index references a non-operation-block chunk",
            ));
        }
        // Strictly ascending (so also duplicate-free) in both vectors.
        if blocks.windows(2).any(|w| w[0] >= w[1]) {
            return Err(DecodeError::Malformed(
                "operation index blocks are not strictly canonically ordered",
            ));
        }
        if entries.windows(2).any(|w| w[0].id >= w[1].id) {
            return Err(DecodeError::Malformed(
                "operation index entries are not strictly ascending by id",
            ));
        }
        if entries.iter().any(|e| e.block as usize >= blocks.len()) {
            return Err(DecodeError::Malformed(
                "operation index entry names an out-of-range block ordinal",
            ));
        }
        Ok(OperationIndex { blocks, entries })
    }

    /// Locates an operation by its 16 canonical id bytes: the [`ChunkRef`] of
    /// its enclosing block and the byte offset of the envelope's first content
    /// byte within that block's decoded payload. Binary search — the O(log n)
    /// lookup the spec names as the index's purpose.
    pub fn locate(&self, id: &[u8; 16]) -> Option<(&ChunkRef, u32)> {
        let i = self.entries.binary_search_by(|e| e.id.cmp(id)).ok()?;
        let e = &self.entries[i];
        Some((&self.blocks[e.block as usize], e.offset))
    }

    /// Whether this index covers exactly the given operation roots — the
    /// staleness gate (Chapter 8 §"The Operation Index": a stale index MUST be
    /// rejected and rebuilt). True iff the index's block set equals the root
    /// set as **full [`ChunkRef`]s** (not just chunk ids): `locate` hands out
    /// the index's *stored* references for reading, so a reference agreeing in
    /// content hash but differing in any locator field (offset, lengths,
    /// compression) is not the manifest's block and must count as stale rather
    /// than silently steering reads elsewhere. `false` = stale: reject and
    /// rebuild from blocks; it is *not* bundle corruption.
    pub fn covers(&self, operation_roots: &[ChunkRef]) -> bool {
        // The index's own blocks are strictly sorted; canonicalize the roots
        // the same way (a decoded manifest's roots already are — this only
        // shields against hand-assembled inputs).
        let mut roots = operation_roots.to_vec();
        roots.sort();
        roots.dedup();
        roots == self.blocks
    }

    /// The indexed blocks, in canonical order.
    pub fn blocks(&self) -> &[ChunkRef] {
        &self.blocks
    }

    /// The entries, strictly ascending by id bytes.
    pub fn entries(&self) -> &[OperationIndexEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::CompressionAlgorithm;
    use crate::ids::SchemaVersion;
    use epiphany_determinism::{ChunkId, ContentHash};

    fn block_ref(hash_byte: u8, offset: u64, len: u64) -> ChunkRef {
        ChunkRef {
            id: ChunkId(ContentHash([hash_byte; 32])),
            kind: ChunkKind::OperationEnvelopeBlock,
            schema_version: SchemaVersion::V0,
            offset,
            compressed_length: len,
            uncompressed_length: len,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([hash_byte; 32]),
        }
    }

    fn sample() -> OperationIndex {
        OperationIndex::build(&[
            (block_ref(0x22, 1000, 64), vec![([3; 16], 8), ([1; 16], 40)]),
            (block_ref(0x11, 576, 32), vec![([2; 16], 8)]),
        ])
        .expect("valid build inputs")
    }

    #[test]
    fn payload_encoding_is_golden() {
        // PROVISIONAL golden lock (DECISIONS.md "Operation index"): the byte
        // form awaits Binary Format companion ratification, but until then it
        // is deterministic and locked — a layout change must break here
        // deliberately. The expected bytes are spelled out literally.
        let index =
            OperationIndex::build(&[(block_ref(0x11, 576, 64), vec![([2; 16], 8), ([1; 16], 44)])])
                .expect("valid build inputs");

        let mut expected: Vec<u8> = Vec::new();
        // u32 block count = 1.
        expected.extend_from_slice(&[1, 0, 0, 0]);
        // ChunkRef: id (32 raw bytes) …
        expected.extend_from_slice(&[0x11; 32]);
        // … kind discriminant (OperationEnvelopeBlock = 0) …
        expected.push(0);
        // … schema version 0.1 (u16 major LE, u16 minor LE) …
        expected.extend_from_slice(&[0, 0, 1, 0]);
        // … offset 576 (u64 LE), compressed and uncompressed length 64 …
        expected.extend_from_slice(&[0x40, 0x02, 0, 0, 0, 0, 0, 0]);
        expected.extend_from_slice(&[64, 0, 0, 0, 0, 0, 0, 0]);
        expected.extend_from_slice(&[64, 0, 0, 0, 0, 0, 0, 0]);
        // … compression None (discriminant 0, parameter 0) …
        expected.extend_from_slice(&[0, 0]);
        // … restated hash (32 raw bytes).
        expected.extend_from_slice(&[0x11; 32]);
        // u32 entry count = 2; entries ascend by id bytes.
        expected.extend_from_slice(&[2, 0, 0, 0]);
        // Entry { id [1;16], block ordinal 0 (u32 LE), offset 44 (u32 LE) }.
        expected.extend_from_slice(&[1; 16]);
        expected.extend_from_slice(&[0, 0, 0, 0]);
        expected.extend_from_slice(&[44, 0, 0, 0]);
        // Entry { id [2;16], block ordinal 0, offset 8 }.
        expected.extend_from_slice(&[2; 16]);
        expected.extend_from_slice(&[0, 0, 0, 0]);
        expected.extend_from_slice(&[8, 0, 0, 0]);

        assert_eq!(index.encode(), expected);
    }

    #[test]
    fn empty_index_round_trips() {
        let empty = OperationIndex::build(&[]).expect("empty build");
        let bytes = empty.encode();
        // Two zero u32 counts.
        assert_eq!(bytes, vec![0, 0, 0, 0, 0, 0, 0, 0]);
        let decoded = OperationIndex::decode(&bytes).expect("decodes");
        assert_eq!(decoded, empty);
        assert!(decoded.covers(&[]));
        assert_eq!(decoded.locate(&[0; 16]), None);
    }

    #[test]
    fn round_trips_and_reencodes_byte_stably() {
        let index = sample();
        let bytes = index.encode();
        let decoded = OperationIndex::decode(&bytes).expect("decodes");
        assert_eq!(decoded, index);
        assert_eq!(decoded.encode(), bytes, "re-encode must be byte-identical");
    }

    #[test]
    fn build_sorts_blocks_canonically_and_remaps_ordinals() {
        // Inputs arrive with the higher-hash block first; build must order by
        // the canonical ChunkRef key and keep each entry pointing at its own
        // block through the remapped ordinal.
        let index = sample();
        assert_eq!(index.blocks()[0].hash.as_bytes()[0], 0x11);
        assert_eq!(index.blocks()[1].hash.as_bytes()[0], 0x22);
        let (b, off) = index.locate(&[2; 16]).expect("hit");
        assert_eq!((b.hash.as_bytes()[0], off), (0x11, 8));
        let (b, off) = index.locate(&[1; 16]).expect("hit");
        assert_eq!((b.hash.as_bytes()[0], off), (0x22, 40));
        let (b, off) = index.locate(&[3; 16]).expect("hit");
        assert_eq!((b.hash.as_bytes()[0], off), (0x22, 8));
    }

    #[test]
    fn locate_misses_an_unknown_id() {
        assert_eq!(sample().locate(&[9; 16]), None);
    }

    #[test]
    fn build_rejects_duplicate_ids_blocks_and_wrong_kinds() {
        // The same id under two blocks: one operation, one slot.
        assert_eq!(
            OperationIndex::build(&[
                (block_ref(0x11, 576, 32), vec![([1; 16], 8)]),
                (block_ref(0x22, 1000, 32), vec![([1; 16], 8)]),
            ]),
            Err(OperationIndexBuildError::DuplicateOperationId([1; 16]))
        );
        // The same block twice: the block set is a set.
        assert_eq!(
            OperationIndex::build(&[
                (block_ref(0x11, 576, 32), vec![([1; 16], 8)]),
                (block_ref(0x11, 576, 32), vec![([2; 16], 8)]),
            ]),
            Err(OperationIndexBuildError::DuplicateBlock)
        );
        // A non-operation-block reference.
        let mut wrong = block_ref(0x11, 576, 32);
        wrong.kind = ChunkKind::Snapshot;
        assert_eq!(
            OperationIndex::build(&[(wrong, vec![])]),
            Err(OperationIndexBuildError::NotAnOperationBlock)
        );
    }

    #[test]
    fn decode_rejects_unsorted_blocks() {
        // Hand-assemble an index whose blocks are out of canonical order;
        // encode trusts the construction, so decode must be the gate.
        let bad = OperationIndex {
            blocks: vec![block_ref(0x22, 1000, 32), block_ref(0x11, 576, 32)],
            entries: vec![],
        };
        assert_eq!(
            OperationIndex::decode(&bad.encode()),
            Err(DecodeError::Malformed(
                "operation index blocks are not strictly canonically ordered"
            ))
        );
        // Duplicates violate *strict* ascent too.
        let dup = OperationIndex {
            blocks: vec![block_ref(0x11, 576, 32), block_ref(0x11, 576, 32)],
            entries: vec![],
        };
        assert!(OperationIndex::decode(&dup.encode()).is_err());
    }

    #[test]
    fn decode_rejects_unsorted_or_duplicate_entries() {
        let entry = |id: u8, block: u32| OperationIndexEntry {
            id: [id; 16],
            block,
            offset: 8,
        };
        let unsorted = OperationIndex {
            blocks: vec![block_ref(0x11, 576, 32)],
            entries: vec![entry(2, 0), entry(1, 0)],
        };
        assert!(OperationIndex::decode(&unsorted.encode()).is_err());
        let duplicated = OperationIndex {
            blocks: vec![block_ref(0x11, 576, 32)],
            entries: vec![entry(1, 0), entry(1, 0)],
        };
        assert!(OperationIndex::decode(&duplicated.encode()).is_err());
    }

    #[test]
    fn decode_rejects_out_of_range_ordinals_wrong_kinds_and_trailing_bytes() {
        let out_of_range = OperationIndex {
            blocks: vec![block_ref(0x11, 576, 32)],
            entries: vec![OperationIndexEntry {
                id: [1; 16],
                block: 1, // only ordinal 0 exists
                offset: 8,
            }],
        };
        assert_eq!(
            OperationIndex::decode(&out_of_range.encode()),
            Err(DecodeError::Malformed(
                "operation index entry names an out-of-range block ordinal"
            ))
        );

        let mut wrong = block_ref(0x11, 576, 32);
        wrong.kind = ChunkKind::Snapshot;
        let wrong_kind = OperationIndex {
            blocks: vec![wrong],
            entries: vec![],
        };
        assert_eq!(
            OperationIndex::decode(&wrong_kind.encode()),
            Err(DecodeError::Malformed(
                "operation index references a non-operation-block chunk"
            ))
        );

        let mut trailing = sample().encode();
        trailing.push(0);
        assert_eq!(
            OperationIndex::decode(&trailing),
            Err(DecodeError::TrailingBytes { remaining: 1 })
        );
    }

    #[test]
    fn covers_is_exact_set_equality_over_full_refs() {
        let index = sample();
        let a = block_ref(0x11, 576, 32);
        let b = block_ref(0x22, 1000, 64);
        // Equal set (any input order; duplicates collapse).
        assert!(index.covers(&[b, a]));
        assert!(index.covers(&[a, b, a]));
        // Subset / superset are stale.
        assert!(!index.covers(&[a]));
        assert!(!index.covers(&[a, b, block_ref(0x33, 2000, 8)]));
        // Same content hash at a different locator is stale: the index would
        // hand out a reference that is not the manifest's block.
        let mut moved = b;
        moved.offset = 4096;
        assert!(!index.covers(&[a, moved]));
    }
}
