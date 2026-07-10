//! The document round trip the format could not do until Push 5 / P5: write a
//! bundle, close it, reopen it from bytes, and rebuild the operation set the
//! score's canonical state *is* (Chapter 6 §"Design Principles").
//!
//! Before `epiphany_ops::decode_envelope` existed, a bundle's operation blocks
//! decoded to opaque byte strings and stopped there. Everything below the
//! `decode_block` call was unreachable: no `OperationEnvelope`, no
//! `OperationSet`, no reduction, no score.

use epiphany_bundle::{
    decode_block, pack_operation_blocks, Bundle, DocumentId, FileUuid, Manifest, MemStore,
    StagedChunk,
};
use epiphany_determinism::fuzz::SplitMix64;
use epiphany_ops::{decode_envelope, OperationSet};

fn append_roots(ctx: &epiphany_bundle::CommitContext) -> Manifest {
    let mut m = ctx.previous_manifest.clone();
    m.operation_roots.extend(ctx.new_chunks.iter().copied());
    m
}

#[test]
fn a_bundle_round_trips_through_bytes_back_into_a_reduced_score() {
    // A real, varied operation set: every kind the generator reaches.
    let mut rng = SplitMix64::new(0x00DE_C0DE_0B00_C1E5);
    let envelopes = epiphany_ops::fuzz::gen_envelope_set(&mut rng, 400);
    assert!(envelopes.len() > 300, "a meaningful document");

    let expected = {
        let mut set = OperationSet::new();
        set.accept_all(envelopes.clone());
        set.reduce().canonical_bytes()
    };

    // Write it.
    let payloads: Vec<Vec<u8>> = envelopes
        .iter()
        .map(epiphany_determinism::CanonicalEncode::to_canonical_bytes)
        .collect();
    let staged: Vec<StagedChunk> = pack_operation_blocks(&payloads)
        .into_iter()
        .map(StagedChunk::operation_block)
        .collect();
    assert!(!staged.is_empty());

    let mut bundle = Bundle::create(
        MemStore::new(),
        FileUuid([3; 16]),
        Manifest::empty(DocumentId([9; 16])),
    )
    .expect("create");
    bundle.commit(&staged, append_roots).expect("commit");
    let image = bundle.into_store().into_bytes();

    // Close it, reopen it from nothing but the bytes.
    let reopened = Bundle::open(MemStore::from_bytes(image)).expect("reopen");

    let mut recovered = Vec::new();
    for chunk in reopened.manifest().operation_roots.clone() {
        let payload = reopened.read_chunk(&chunk).expect("chunk reads");
        for envelope_bytes in decode_block(&payload).expect("block frames") {
            // The inverse that did not exist.
            recovered.push(decode_envelope(&envelope_bytes).expect("envelope decodes"));
        }
    }

    assert_eq!(recovered.len(), envelopes.len(), "every envelope came back");

    // Same operations, and therefore the same canonical state. Reduction is
    // permutation-invariant, so this compares the document, not the file layout.
    let mut set = OperationSet::new();
    set.accept_all(recovered);
    assert_eq!(
        set.reduce().canonical_bytes(),
        expected,
        "the reopened bundle reduces to the same canonical state"
    );
}
