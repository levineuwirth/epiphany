//! Decode conformance vectors for the bundle wire (P4 of the decode-hardening
//! track). See `epiphany_ops::vectors` for the corpus's purpose and columns.
//!
//! These live inside the crate because several rejection classes can only be
//! *constructed* with internal access: a manifest whose vectors are out of order
//! but whose `manifest_id` is correctly derived over the reordered body isolates
//! the ordering rule from the id rule, and `ManifestId::derive` is crate-private.

use crate::chunk::{ChunkKind, ChunkRef, CompressionAlgorithm};
use crate::ids::{DocumentId, ManifestId, SchemaVersion};
use crate::manifest::Manifest;
use crate::opindex::OperationIndex;
use crate::{block, DecodeError};
use epiphany_determinism::{ChunkId, ContentHash};

/// `(surface, verdict, class, name, bytes)` — see `epiphany_ops::vectors`.
pub type DecodeVector = (&'static str, &'static str, &'static str, String, Vec<u8>);

fn row(
    surface: &'static str,
    verdict: &'static str,
    class: &'static str,
    name: impl Into<String>,
    bytes: Vec<u8>,
) -> DecodeVector {
    (surface, verdict, class, name.into(), bytes)
}

/// A `ChunkRef` encodes as 95 bytes: id 32, kind 1, schema 4, offset 8,
/// compressed_length 8, uncompressed_length 8, compression 2, hash 32.
const CHUNK_REF_LEN: usize = 32 + 1 + 4 + 8 + 8 + 8 + 2 + 32;
/// Offset of the compression *parameter* byte within a `ChunkRef`.
const CHUNK_REF_COMPRESSION_PARAM: usize = 32 + 1 + 4 + 8 + 8 + 8 + 1;

fn block_ref(hash_byte: u8, offset: u64) -> ChunkRef {
    ChunkRef {
        id: ChunkId(ContentHash([hash_byte; 32])),
        kind: ChunkKind::OperationEnvelopeBlock,
        schema_version: SchemaVersion::V0,
        offset,
        compressed_length: 64,
        uncompressed_length: 64,
        compression: CompressionAlgorithm::None,
        hash: ContentHash([hash_byte; 32]),
    }
}

fn swap(bytes: &[u8], first: usize, width: usize) -> Vec<u8> {
    let second = first + width;
    let mut out = bytes.to_vec();
    out[first..second].copy_from_slice(&bytes[second..second + width]);
    out[second..second + width].copy_from_slice(&bytes[first..second]);
    out
}

/// Every bundle-wire vector.
pub fn decode_vectors() -> Vec<DecodeVector> {
    let mut v: Vec<DecodeVector> = Vec::new();

    // --- Manifest ----------------------------------------------------------
    const MAN: &str = "bundle.manifest";
    let doc = DocumentId([5; 16]);
    let empty = Manifest::empty(doc);
    v.push(row(MAN, "accept", "-", "empty_manifest", empty.encode()));

    let mut one = Manifest::empty(doc);
    one.operation_roots.push(block_ref(0x11, 576));
    let one_bytes = one.encode();
    v.push(row(
        MAN,
        "accept",
        "-",
        "one_operation_root",
        one_bytes.clone(),
    ));

    // Body edit: `manifest_id` is derived from the body, so it no longer matches.
    let mut id_mismatch = one_bytes.clone();
    let last = id_mismatch.len() - 1;
    id_mismatch[last] ^= 0xFF;
    v.push(row(
        MAN,
        "reject",
        "manifest-id-mismatch",
        "one_root_body_edited",
        id_mismatch,
    ));

    // Two roots, encoded in canonical (sorted) order. Swapping them and then
    // *re-deriving the id over the reordered body* isolates the ordering rule:
    // the id is right, the order is wrong, and only the re-encode guard rejects
    // it — `encode_body` sorts, so the round trip cannot reproduce these bytes.
    let mut two = Manifest::empty(doc);
    two.operation_roots.push(block_ref(0x11, 576));
    two.operation_roots.push(block_ref(0x22, 1024));
    let two_bytes = two.encode();
    v.push(row(
        MAN,
        "accept",
        "-",
        "two_operation_roots",
        two_bytes.clone(),
    ));

    // Locate the first root: id(16) + document_id(16) + lineage option tag(1)
    // + generation(8) + roots count(4).
    const ROOTS_AT: usize = 16 + 16 + 1 + 8 + 4;
    let mut reordered = swap(&two_bytes, ROOTS_AT, CHUNK_REF_LEN);
    let body = &reordered[16..];
    let redone = ManifestId::derive(doc, 0, body);
    reordered[0..16].copy_from_slice(&redone.0.to_be_bytes());
    v.push(row(
        MAN,
        "reject",
        "non-canonical-vec-order",
        "two_roots_out_of_order_valid_id",
        reordered,
    ));

    let mut trailing = one_bytes.clone();
    trailing.push(0);
    v.push(row(
        MAN,
        "reject",
        "trailing-bytes",
        "one_root_trailing",
        trailing,
    ));

    // --- OperationIndex ----------------------------------------------------
    const IDX: &str = "bundle.operation_index";
    let empty_index = OperationIndex::build(&[]).expect("empty").encode();
    v.push(row(IDX, "accept", "-", "empty_index", empty_index.clone()));

    let one_index = OperationIndex::build(&[(block_ref(0x11, 576), vec![([2; 16], 8)])])
        .expect("one block")
        .encode();
    v.push(row(
        IDX,
        "accept",
        "-",
        "one_block_one_entry",
        one_index.clone(),
    ));

    let two_index = OperationIndex::build(&[
        (block_ref(0x11, 576), vec![([1; 16], 8), ([3; 16], 40)]),
        (block_ref(0x22, 1024), vec![([2; 16], 8)]),
    ])
    .expect("two blocks")
    .encode();
    v.push(row(IDX, "accept", "-", "two_blocks", two_index.clone()));

    // The lenient-compression class. `CompressionAlgorithm::None` used to ignore
    // this byte, so `[0, 0xFF]` and `[0, 0]` decoded alike: two byte strings, one
    // value. The index has no whole-value re-encode guard, so it accepted both.
    // (Push 5 / P3; `req:binfmt:compression-none-parameter`.)
    let mut lenient = one_index.clone();
    lenient[4 + CHUNK_REF_COMPRESSION_PARAM] = 0xFF;
    v.push(row(
        IDX,
        "reject",
        "lenient-sub-codec",
        "compression_none_non_zero_parameter",
        lenient,
    ));

    // Strict ascent, checked per-site (there is no guard here to catch it).
    v.push(row(
        IDX,
        "reject",
        "non-canonical-vec-order",
        "blocks_out_of_order",
        swap(&two_index, 4, CHUNK_REF_LEN),
    ));

    // Entries are 24 bytes: id(16) + block(4) + offset(4). They begin after the
    // blocks and their count.
    let entries_at = 4 + 2 * CHUNK_REF_LEN + 4;
    v.push(row(
        IDX,
        "reject",
        "non-canonical-vec-order",
        "entries_out_of_order",
        swap(&two_index, entries_at, 24),
    ));

    // An entry naming a block ordinal that does not exist.
    let mut bad_ordinal = one_index.clone();
    let ordinal_at = 4 + CHUNK_REF_LEN + 4 + 16;
    bad_ordinal[ordinal_at..ordinal_at + 4].copy_from_slice(&7u32.to_le_bytes());
    v.push(row(
        IDX,
        "reject",
        "block-ordinal-out-of-range",
        "entry_names_missing_block",
        bad_ordinal,
    ));

    // A block reference whose kind is not an operation-envelope block.
    let mut wrong_kind = one_index.clone();
    wrong_kind[4 + 32] = ChunkKind::Snapshot.discriminant();
    v.push(row(
        IDX,
        "reject",
        "wrong-chunk-kind",
        "block_is_not_an_envelope_block",
        wrong_kind,
    ));

    let mut idx_trailing = empty_index.clone();
    idx_trailing.push(0);
    v.push(row(
        IDX,
        "reject",
        "trailing-bytes",
        "empty_index_trailing",
        idx_trailing,
    ));

    // --- Block payload framing ---------------------------------------------
    const BLK: &str = "bundle.block";
    let payloads: Vec<Vec<u8>> = vec![vec![0xAA; 8], vec![0xBB; 13]];
    let packed = block::pack_operation_blocks(&payloads);
    let good = packed.first().expect("one block").clone();
    v.push(row(BLK, "accept", "-", "two_envelopes", good.clone()));

    let mut blk_trailing = good.clone();
    blk_trailing.push(0);
    v.push(row(
        BLK,
        "reject",
        "trailing-bytes",
        "two_envelopes_trailing",
        blk_trailing,
    ));

    let mut blk_truncated = good.clone();
    blk_truncated.pop();
    v.push(row(
        BLK,
        "reject",
        "truncated",
        "two_envelopes_truncated",
        blk_truncated,
    ));

    // A declared envelope count far past the bytes remaining: the decoder must
    // reject on the count, not pre-allocate for it.
    let mut huge = good.clone();
    huge[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    v.push(row(
        BLK,
        "reject",
        "count-exceeds-remaining",
        "envelope_count_u32_max",
        huge,
    ));

    v
}

/// Applies `surface`'s decoder to `bytes`.
///
/// `Ok(injective)` means the decoder **accepted**, and `injective` says whether
/// the value re-encodes to exactly these bytes; `Err` means it **rejected**. See
/// `epiphany_ops::vectors::check` for why those must not be collapsed.
///
/// `None` for surfaces this crate does not own.
pub fn check(surface: &str, bytes: &[u8]) -> Option<Result<bool, String>> {
    fn report<T>(r: Result<T, DecodeError>) -> Result<T, String> {
        r.map_err(|e| format!("{e:?}"))
    }
    match surface {
        "bundle.manifest" => Some(report(Manifest::decode(bytes)).map(|m| m.encode() == bytes)),
        "bundle.operation_index" => {
            Some(report(OperationIndex::decode(bytes)).map(|i| i.encode() == bytes))
        }
        // A block payload is a framing, not a canonical value: `decode_block`
        // returns the envelopes, and re-framing them reproduces the payload.
        "bundle.block" => Some(report(block::decode_block(bytes)).map(|envelopes| {
            block::pack_operation_blocks(&envelopes)
                .first()
                .is_some_and(|packed| packed == bytes)
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vector_gets_its_declared_verdict() {
        for (surface, verdict, class, name, bytes) in decode_vectors() {
            let result = check(surface, &bytes).expect("a surface this crate owns");
            match (verdict, &result) {
                ("accept", Ok(true)) => {}
                ("accept", Ok(false)) => {
                    panic!("{surface}/{name}: accepted but does not re-encode to its bytes")
                }
                ("reject", Err(_)) => {}
                _ => panic!("{surface}/{name} ({class}): declared {verdict}, got {result:?}"),
            }
        }
    }

    #[test]
    fn every_surface_carries_both_verdicts() {
        for surface in ["bundle.manifest", "bundle.operation_index", "bundle.block"] {
            let rows: Vec<_> = decode_vectors()
                .into_iter()
                .filter(|(s, ..)| *s == surface)
                .collect();
            assert!(
                rows.iter().any(|(_, v, ..)| *v == "accept"),
                "{surface} has no accept vector"
            );
            assert!(
                rows.iter().any(|(_, v, ..)| *v == "reject"),
                "{surface} has no reject vector"
            );
        }
    }

    /// The reordered-manifest vector must isolate the *ordering* rule: its
    /// `manifest_id` is correctly derived over the reordered body, so it is not
    /// rejected merely for a stale id. If this stopped holding, the vector would
    /// be pinning the wrong rejection.
    #[test]
    fn the_reordered_manifest_vector_carries_a_valid_id() {
        let (.., bytes) = decode_vectors()
            .into_iter()
            .find(|(.., name, _)| *name == "two_roots_out_of_order_valid_id")
            .expect("the vector exists");
        let derived = ManifestId::derive(DocumentId([5; 16]), 0, &bytes[16..]);
        let stored = ManifestId(u128::from_be_bytes(
            bytes[0..16].try_into().expect("16 bytes"),
        ));
        assert_eq!(stored, derived, "the id matches its (reordered) body");
        assert!(Manifest::decode(&bytes).is_err(), "the order still rejects");
    }
}
