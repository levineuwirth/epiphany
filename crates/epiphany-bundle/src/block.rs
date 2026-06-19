//! Operation-envelope block packing (Chapter 8 §"Operation Envelope Blocks").
//!
//! The canonical document is stored as operation-envelope blocks — each a chunk
//! of kind [`crate::ChunkKind::OperationEnvelopeBlock`]. From the bundle's
//! vantage an envelope is *opaque encoded bytes* (the `OperationEnvelope` type
//! and its canonical encoding belong to `epiphany-ops`, Agent C). The bundle's
//! contribution is purely physical: pack a sequence of opaque envelope byte
//! strings into block payloads at the spec's size targets, and split a block
//! payload back into its envelopes.
//!
//! > Writers SHOULD begin a new operation-envelope block when adding another
//! > envelope would cause the uncompressed block payload to exceed 1 MiB, except
//! > when an individual envelope's encoded size exceeds 1 MiB, in which case the
//! > envelope occupies its own block. — Chapter 8
//!
//! Block boundaries are storage artifacts, not semantic structure: *"The set of
//! envelopes is the union of all envelopes across all referenced blocks."* So
//! `unpack` ∘ `pack` preserves the multiset and order of envelopes but says
//! nothing about how they were grouped.

use crate::codec::{DecodeError, Reader, Writer};

/// Soft target for an uncompressed operation-envelope block payload: 1 MiB
/// (Chapter 8). A writer starts a new block rather than exceed it.
pub const BLOCK_SOFT_LIMIT: u64 = 1 << 20;

/// Default reader bound on an uncompressed block: 64 MiB (Chapter 8). Profiles
/// may raise or lower it; blocks exceeding the active bound are malformed.
pub const MAX_BLOCK_DEFAULT: u64 = 64 << 20;

/// Per-envelope framing overhead in a block payload: a `u32` length prefix.
const ENVELOPE_FRAMING: u64 = 4;

/// The fixed framing overhead of a block payload: a `u32` envelope count.
const BLOCK_HEADER: u64 = 4;

/// Encodes one block payload from a slice of opaque envelope byte strings:
/// a `u32` count, then each envelope length-prefixed.
pub fn encode_block(envelopes: &[Vec<u8>]) -> Vec<u8> {
    let mut w = Writer::new();
    w.put_seq(envelopes, |w, env| {
        w.put_var_bytes(env);
    });
    w.into_bytes()
}

/// Splits a block payload back into its opaque envelope byte strings. Total and
/// bounds-checked: a corrupt payload yields a [`DecodeError`], never a panic.
pub fn decode_block(payload: &[u8]) -> Result<Vec<Vec<u8>>, DecodeError> {
    let mut r = Reader::new(payload);
    let envelopes = r.get_seq(|r| r.get_var_bytes())?;
    r.finish()?;
    Ok(envelopes)
}

/// Packs opaque envelope byte strings into block payloads at the 1 MiB soft
/// target. An envelope whose framed size alone exceeds the soft limit gets its
/// own block (so individual oversized envelopes are never dropped or split).
/// The flattened envelope order across the returned blocks equals the input
/// order, so `decode_block` over the blocks in order reproduces the input.
pub fn pack_operation_blocks(envelopes: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<Vec<u8>> = Vec::new();
    let mut current_size = BLOCK_HEADER;

    for env in envelopes {
        let entry_size = ENVELOPE_FRAMING + env.len() as u64;

        // An individual oversized envelope occupies its own block.
        if entry_size + BLOCK_HEADER > BLOCK_SOFT_LIMIT {
            if !current.is_empty() {
                blocks.push(encode_block(&current));
                current.clear();
                current_size = BLOCK_HEADER;
            }
            blocks.push(encode_block(std::slice::from_ref(env)));
            continue;
        }

        // Otherwise, start a new block before exceeding the soft target.
        if !current.is_empty() && current_size + entry_size > BLOCK_SOFT_LIMIT {
            blocks.push(encode_block(&current));
            current.clear();
            current_size = BLOCK_HEADER;
        }
        current.push(env.clone());
        current_size += entry_size;
    }

    if !current.is_empty() {
        blocks.push(encode_block(&current));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_packs_to_no_blocks() {
        assert!(pack_operation_blocks(&[]).is_empty());
    }

    #[test]
    fn block_round_trips() {
        let envs = vec![b"a".to_vec(), b"bb".to_vec(), b"ccc".to_vec()];
        let payload = encode_block(&envs);
        assert_eq!(decode_block(&payload).unwrap(), envs);
    }

    #[test]
    fn small_envelopes_share_one_block_and_preserve_order() {
        let envs: Vec<Vec<u8>> = (0..100).map(|i| vec![i as u8; 8]).collect();
        let blocks = pack_operation_blocks(&envs);
        assert_eq!(blocks.len(), 1, "100 tiny envelopes fit in one 1 MiB block");
        assert_eq!(decode_block(&blocks[0]).unwrap(), envs);
    }

    #[test]
    fn block_payloads_stay_under_the_soft_limit() {
        // ~300 KiB envelopes: each block holds a few, none exceeds 1 MiB.
        let envs: Vec<Vec<u8>> = (0..10).map(|i| vec![i as u8; 300 * 1024]).collect();
        let blocks = pack_operation_blocks(&envs);
        assert!(blocks.len() > 1);
        for b in &blocks {
            assert!(
                b.len() as u64 <= BLOCK_SOFT_LIMIT,
                "block exceeded soft limit"
            );
        }
        // The flattened order is preserved across blocks.
        let recovered: Vec<Vec<u8>> = blocks
            .iter()
            .flat_map(|b| decode_block(b).unwrap())
            .collect();
        assert_eq!(recovered, envs);
    }

    #[test]
    fn oversized_envelope_gets_its_own_block() {
        let big = vec![7u8; (BLOCK_SOFT_LIMIT as usize) + 10];
        let envs = vec![b"small".to_vec(), big.clone(), b"tail".to_vec()];
        let blocks = pack_operation_blocks(&envs);
        // small | big-alone | tail
        assert_eq!(blocks.len(), 3);
        assert_eq!(decode_block(&blocks[1]).unwrap(), vec![big]);
        let recovered: Vec<Vec<u8>> = blocks
            .iter()
            .flat_map(|b| decode_block(b).unwrap())
            .collect();
        assert_eq!(recovered, envs);
    }
}
