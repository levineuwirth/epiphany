//! The crash-recovery acceptance gate (QUICKSTART, Agent D): *"the
//! crash-recovery fuzzer is the acceptance gate … the bundle must be valid in
//! 100% of runs, and must recover to the previous generation when the crash
//! precedes the durable flush. This is the most important single test in the
//! entire prototype."*
//!
//! These tests drive the gate three ways: the randomized 10,000-iteration
//! sweep, an exhaustive per-syscall sweep across several base configurations,
//! and a real-filesystem (`fsync`) round-trip to confirm the protocol works
//! against an actual durable-flush primitive, not only the simulator.

use epiphany_bundle::fuzz::{exhaustive_crash_check, run_crash_recovery_fuzz, SplitMix64};
use epiphany_bundle::{Bundle, DocumentId, FileUuid, Manifest, MemStore, StagedChunk};

/// The headline gate: 10,000 randomized crash scenarios. Every one must recover
/// to a valid bundle at the previous or new generation, with all canonical
/// chunks intact.
#[test]
fn crash_recovery_fuzz_ten_thousand_iterations() {
    run_crash_recovery_fuzz(10_000, 0x6D75_7363_6272_6E64);
}

/// A second seed, for independence from any single seed's coverage.
#[test]
fn crash_recovery_fuzz_alternate_seed() {
    run_crash_recovery_fuzz(10_000, 0x0123_4567_89AB_CDEF);
}

/// Exhaustively crash *every* syscall boundary, with every tear point, for a
/// range of base generations and commit shapes. This leaves no step of the
/// commit protocol untested.
#[test]
fn exhaustive_sweep_across_base_states_and_commit_shapes() {
    let mut rng = SplitMix64::new(0xE0A1_5707_C0FF_EE55);
    for base_commits in 0..4u64 {
        let (image, gen) = build_base(&mut rng, base_commits);
        // A few commit shapes: empty, single tiny block, multi-block, oversized.
        let shapes: Vec<Vec<Vec<u8>>> = vec![
            vec![],
            vec![b"x".to_vec()],
            vec![b"alpha".to_vec(), b"beta".to_vec(), b"gamma".to_vec()],
            vec![vec![9u8; 1024]],
        ];
        for shape in &shapes {
            exhaustive_crash_check(&image, gen, shape);
        }
    }
}

/// Helper: a fresh bundle advanced through `commits` clean commits, returning
/// its image and generation. (Mirrors the fuzzer's own base builder.)
fn build_base(rng: &mut SplitMix64, commits: u64) -> (Vec<u8>, u64) {
    let doc = DocumentId([(rng.next_u64() & 0xff) as u8; 16]);
    let mut bundle =
        Bundle::create(MemStore::new(), FileUuid([7; 16]), Manifest::empty(doc)).unwrap();
    for i in 0..commits {
        let payload = epiphany_bundle::encode_block(&[vec![i as u8; 16]]);
        bundle
            .commit(&[StagedChunk::operation_block(payload)], |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_roots.extend(ctx.new_chunks.iter().copied());
                m
            })
            .unwrap();
    }
    let gen = bundle.generation();
    (bundle.into_store().into_bytes(), gen)
}

/// A real-filesystem round-trip: create a bundle, commit twice with genuine
/// `fsync` flushes, reopen from disk, and verify the committed state. Confirms
/// the atomic-commit protocol is wired to an actual durable-flush primitive.
#[cfg(unix)]
#[test]
fn file_store_real_fsync_round_trip() {
    use epiphany_bundle::FileStore;

    let path = unique_temp_path("epiphany-bundle-roundtrip");

    {
        let store = FileStore::create(&path).unwrap();
        let mut bundle = Bundle::create(
            store,
            FileUuid([0xAB; 16]),
            Manifest::empty(DocumentId([1; 16])),
        )
        .unwrap();
        for i in 1..=2u64 {
            let payload = epiphany_bundle::encode_block(&[vec![i as u8; 32]]);
            bundle
                .commit(&[StagedChunk::operation_block(payload)], |ctx| {
                    let mut m = ctx.previous_manifest.clone();
                    m.operation_roots.extend(ctx.new_chunks.iter().copied());
                    m
                })
                .unwrap();
        }
        assert_eq!(bundle.generation(), 2);
    }

    // Reopen from disk in a fresh handle: the committed state is durable.
    let reopened = Bundle::open(FileStore::open(&path).unwrap()).unwrap();
    assert_eq!(reopened.generation(), 2);
    assert_eq!(reopened.manifest().operation_roots.len(), 2);
    reopened.verify_canonical_chunks().unwrap();
    assert_eq!(reopened.file_uuid(), FileUuid([0xAB; 16]));

    std::fs::remove_file(&path).ok();
}

/// A unique temp path without a `tempfile` dependency: temp dir + process id +
/// a monotonic counter.
#[cfg(unix)]
fn unique_temp_path(stem: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!("{stem}-{}-{}.musc", std::process::id(), n);
    std::env::temp_dir().join(name)
}
