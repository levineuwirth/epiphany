//! The manifest-selection acceptance gate (QUICKSTART, Agent D): *"the manifest
//! selection harness handles every corruption scenario (slot A corrupt + slot B
//! valid, vice versa, both valid same generation, both valid generation+1,
//! neither valid)."*
//!
//! The harness itself lives in [`epiphany_bundle::fuzz::run_manifest_selection_harness`]
//! (it needs crate-private image construction); this integration test drives it
//! through the public surface and adds a couple of end-to-end checks against the
//! real commit path.

use epiphany_bundle::fuzz::run_manifest_selection_harness;
use epiphany_bundle::{Bundle, DocumentId, FileUuid, Manifest, MemStore, Slot, StagedChunk};

#[test]
fn every_selection_scenario_holds() {
    run_manifest_selection_harness();
}

/// After a real commit, both slots are valid and differ by exactly one
/// generation; selection picks the higher (the just-committed state), and
/// corrupting it falls back cleanly to the previous generation.
#[test]
fn commit_then_corrupt_active_slot_falls_back() {
    let mut bundle = Bundle::create(
        MemStore::new(),
        FileUuid([3; 16]),
        Manifest::empty(DocumentId([4; 16])),
    )
    .unwrap();
    // Commit once: slot A holds gen 0, slot B holds gen 1 (active).
    let payload = epiphany_bundle::encode_block(&[b"env".to_vec()]);
    bundle
        .commit(&[StagedChunk::operation_block(payload)], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            m.operation_roots.extend(ctx.new_chunks.iter().copied());
            m
        })
        .unwrap();
    assert_eq!(bundle.active_slot(), Slot::B);
    assert_eq!(bundle.generation(), 1);

    let mut image = bundle.into_store().into_bytes();
    // Corrupt the active slot B (offset 320) inside its CRC-covered region.
    image[320 + 80] ^= 0xFF;

    // Recovery falls back to slot A (the previous generation), cleanly.
    let recovered = Bundle::open(MemStore::from_bytes(image)).unwrap();
    assert_eq!(recovered.active_slot(), Slot::A);
    assert_eq!(recovered.generation(), 0);
    assert!(recovered.anomalies().is_empty());
    recovered.verify_canonical_chunks().unwrap();
}
