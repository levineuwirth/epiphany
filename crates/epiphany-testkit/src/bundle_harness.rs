//! The crash-recovery harness (Agent D's gate; v0 acceptance criterion 2) and
//! the manifest-selection harness (QUICKSTART, Agent F).
//!
//! Agent D shipped [`epiphany_bundle`] with its own authoritative gates in
//! [`epiphany_bundle::fuzz`]; the testkit's job is to be the cross-cutting CI
//! entry point that drives them, *and* to exercise the same contract through the
//! bundle's fully public API as an external consumer would — which is what
//! [`assert_recovers`], [`exhaustive_crash_sweep`], and [`run_crash_recovery`]
//! do here. The crash-recovery property (Chapter 8, the most important single
//! test in the prototype):
//!
//! > Kill the process between any two syscalls in the commit protocol; reopen;
//! > the bundle must be valid in 100% of runs, and must recover to the previous
//! > generation when the crash precedes the durable flush.

use epiphany_bundle::{
    pack_operation_blocks, Bundle, CommitContext, CrashPoint, DocumentId, FaultStore, FileUuid,
    Manifest, MemStore, Slot, StagedChunk, Tear,
};

use crate::rng::Rng;

// --- Re-exported authoritative gates from Agent D --------------------------

/// Agent D's exhaustive per-syscall crash sweep for one commit.
pub use epiphany_bundle::fuzz::exhaustive_crash_check as bundle_exhaustive_crash_check;
/// Agent D's randomized crash-recovery fuzzer (the 10,000-iteration gate).
pub use epiphany_bundle::fuzz::run_crash_recovery_fuzz as bundle_crash_recovery_fuzz;
/// Agent D's manifest-selection harness over every corruption scenario.
pub use epiphany_bundle::fuzz::run_manifest_selection_harness as bundle_manifest_selection;

// --- Testkit-authored drivers through the public API -----------------------

/// The commit-context closure: append the commit's new chunks to the previous
/// manifest's `operation_roots`.
fn append_roots(ctx: &CommitContext) -> Manifest {
    let mut m = ctx.previous_manifest.clone();
    m.operation_roots.extend(ctx.new_chunks.iter().copied());
    m
}

fn staged(payloads: &[Vec<u8>]) -> Vec<StagedChunk> {
    pack_operation_blocks(payloads)
        .into_iter()
        .map(StagedChunk::operation_block)
        .collect()
}

/// Builds a fresh bundle image and advances it through `commits` clean commits,
/// returning `(image_bytes, generation)`. Pure public API.
pub fn build_base(rng: &mut Rng, commits: u64) -> (Vec<u8>, u64) {
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    for _ in 0..commits {
        let n = rng.range_usize(0, 2);
        let payloads: Vec<Vec<u8>> = (0..n).map(|_| rng.byte_vec(1, 40)).collect();
        bundle
            .commit(&staged(&payloads), append_roots)
            .expect("commit");
    }
    let generation = bundle.generation();
    (bundle.into_store().into_bytes(), generation)
}

/// Drives one commit over a [`FaultStore`] crashing at `crash`, recovers from
/// the durable image, and asserts the five crash-safety properties (Chapter 8).
/// Panics with context on any violation — the gate's failure condition.
pub fn assert_recovers(
    base_image: &[u8],
    base_gen: u64,
    envelope_payloads: &[Vec<u8>],
    crash: CrashPoint,
) {
    let blocks = staged(envelope_payloads);
    let mut bundle = Bundle::open(FaultStore::new(base_image.to_vec(), crash))
        .expect("the base image must open before the commit");
    let committed = bundle.commit(&blocks, append_roots).is_ok();
    let store = bundle.into_store();

    // Recover from exactly the bytes that survived the crash.
    let durable = store.durable_image();
    let recovered = Bundle::open(MemStore::from_bytes(durable)).unwrap_or_else(|e| {
        panic!("crash at {crash:?} left an UNOPENABLE bundle (base gen {base_gen}): {e}")
    });

    let g = recovered.generation();
    assert!(
        g == base_gen || g == base_gen + 1,
        "crash at {crash:?}: recovered to generation {g}, expected {base_gen} or {}",
        base_gen + 1
    );
    if committed {
        assert_eq!(
            g,
            base_gen + 1,
            "a commit that returned Ok must be durable at the new generation"
        );
    }
    // A clean crash before the durable flush recovers to the previous generation.
    if !committed && crash.tear == Tear::Clean {
        assert_eq!(
            g, base_gen,
            "clean crash at {crash:?}: an aborted commit must recover to the previous generation"
        );
    }
    assert!(
        recovered.anomalies().is_empty(),
        "crash at {crash:?}: recovery reported anomalies {:?}",
        recovered.anomalies()
    );
    assert!(
        !recovered.is_read_only(),
        "crash at {crash:?}: recovery opened read-only"
    );
    recovered
        .verify_canonical_chunks()
        .unwrap_or_else(|e| panic!("crash at {crash:?}: a canonical chunk is corrupt: {e}"));
}

/// Exhaustively crashes a single commit: every syscall boundary from 0 through
/// the commit's total syscall count, crossed with a set of tear points. This is
/// the strongest crash-safety guarantee — no syscall boundary untested. Pure
/// public API.
pub fn exhaustive_crash_sweep(base_image: &[u8], base_gen: u64, envelope_payloads: &[Vec<u8>]) {
    let blocks = staged(envelope_payloads);

    // Learn the commit's total syscall count via a no-fault run (and confirm the
    // clean commit reaches G+1).
    let total = {
        let mut bundle = Bundle::open(FaultStore::no_fault(base_image.to_vec())).unwrap();
        bundle.commit(&blocks, append_roots).unwrap();
        let store = bundle.into_store();
        let recovered = Bundle::open(store.recover()).unwrap();
        assert_eq!(recovered.generation(), base_gen + 1);
        store.syscalls_issued()
    };

    let tears = || {
        let mut v = vec![Tear::Clean];
        for prefix in [0usize, 1, 100, 128, 251, 252, 253, 255, 256] {
            v.push(Tear::TornLastWrite { prefix });
        }
        v
    };
    for k in 0..=total {
        for tear in tears() {
            assert_recovers(
                base_image,
                base_gen,
                envelope_payloads,
                CrashPoint {
                    after_syscalls: k,
                    tear,
                },
            );
        }
    }
}

/// The testkit's randomized crash-recovery driver (acceptance criterion 2):
/// `iters` scenarios from `seed`, each a random base generation, random commit
/// shape, and random crash point + tear. Deterministic; a failure reproduces
/// from its seed.
pub fn run_crash_recovery(iters: u64, seed: u64) {
    let mut rng = Rng::new(seed);
    for _ in 0..iters {
        let base_commits = rng.below(4);
        let (image, generation) = build_base(&mut rng, base_commits);

        let n = rng.range_usize(0, 3);
        let payloads: Vec<Vec<u8>> = (0..n).map(|_| rng.byte_vec(1, 64)).collect();

        let tear = if rng.boolean() {
            Tear::Clean
        } else {
            let prefix = match rng.below(6) {
                0 => 0,
                1 => 1,
                2 => 252,
                3 => 255,
                4 => 256,
                _ => rng.below(256) as usize,
            };
            Tear::TornLastWrite { prefix }
        };
        let crash = CrashPoint {
            after_syscalls: rng.below(16) as u32,
            tear,
        };
        assert_recovers(&image, generation, &payloads, crash);
    }
}

/// A testkit-level manifest-selection check through the *normal commit
/// protocol*: after several commits the bundle reopens at the highest generation
/// and the active slot alternates A/B per commit (Chapter 8 §"Superblock
/// Selection"). Complements [`bundle_manifest_selection`], which assembles
/// corrupt images directly.
pub fn assert_selection_through_commits(seed: u64) {
    let mut rng = Rng::new(seed);
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    // Generation 0 lives in slot A; each commit flips the active slot.
    assert_eq!(bundle.active_slot(), Slot::A);
    let commits = 5u64;
    for i in 1..=commits {
        let payloads = vec![rng.byte_vec(1, 50)];
        bundle
            .commit(&staged(&payloads), append_roots)
            .expect("commit");
        assert_eq!(bundle.generation(), i);
        let expected = if i % 2 == 1 { Slot::B } else { Slot::A };
        assert_eq!(bundle.active_slot(), expected, "active slot must alternate");
    }
    let image = bundle.into_store().into_bytes();

    // Reopen: selection picks the highest committed generation, no anomaly.
    let reopened = Bundle::open(MemStore::from_bytes(image)).expect("reopen");
    assert_eq!(reopened.generation(), commits);
    assert!(reopened.anomalies().is_empty());
    assert!(!reopened.is_read_only());
    reopened.verify_canonical_chunks().expect("chunks intact");
}

/// Runs both the authoritative bundle manifest-selection harness and the
/// testkit's commit-protocol selection check.
pub fn run_manifest_selection(seed: u64) {
    bundle_manifest_selection();
    assert_selection_through_commits(seed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_recovery_smoke() {
        run_crash_recovery(400, 0xF00D_BEEF_1234_5678);
    }

    #[test]
    fn exhaustive_sweep_representative_commit() {
        let mut rng = Rng::new(0x00A1_1CE5);
        let (image, generation) = build_base(&mut rng, 2);
        exhaustive_crash_sweep(&image, generation, &[b"alpha".to_vec(), b"beta".to_vec()]);
    }

    #[test]
    fn manifest_selection_passes() {
        run_manifest_selection(7);
    }
}
