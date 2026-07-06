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
    encode_block, envelope_offsets, pack_operation_blocks, BlockStore, Bundle, BundleError,
    CommitContext, CrashPoint, DocumentId, ExtensionDeclaration, ExtensionId, FaultStore, FileUuid,
    IndexedBlock, Manifest, MemStore, OperationIndex, SchemaVersion, SemVer, Slot, StagedChunk,
    Tear,
};
use epiphany_determinism::CanonicalEncode;
use epiphany_ops::{peek_operation_id, OperationEnvelope};

/// Stages real operation envelopes into a single op-envelope block, **deriving**
/// the block's schema version from its operations: the block major is the max
/// over `OperationEnvelope::schema_major` (a v1 `CreateRegion` → major 1),
/// mapped to a version by [`SchemaVersion::for_major`]. This is the writer-side
/// derivation every real-envelope staging path must use so a block carrying a
/// v1 payload is never mis-stamped major 0.
pub fn stage_operation_block(envelopes: &[OperationEnvelope]) -> StagedChunk {
    let payloads: Vec<Vec<u8>> = envelopes.iter().map(|e| e.to_canonical_bytes()).collect();
    let major = envelopes
        .iter()
        .map(|e| e.schema_major())
        .max()
        .unwrap_or(0);
    StagedChunk::operation_block_versioned(encode_block(&payloads), SchemaVersion::for_major(major))
}

use crate::generators;
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

// --- The operation-index harness (Chapter 8 §"The Operation Index") --------
//
// The C/D seam: `epiphany-ops` vouches that a canonical envelope *leads* with
// its 16 operation-id bytes (`peek_operation_id`); the bundle records those
// raw bytes against `(block ChunkRef, offset-in-decoded-payload)` coordinates
// (`envelope_offsets` + `OperationIndex`) without ever interpreting an
// envelope. The index is an acceleration structure, not canonical: absent →
// rebuild by scanning blocks; present but stale or corrupt → reject and
// rebuild, never bundle corruption.

/// Stages real canonical envelope encodings into operation blocks of at most
/// `per_block` envelopes each (forcing a multi-block layout regardless of the
/// 1 MiB soft target, so index ordinals are actually exercised).
fn staged_envelope_blocks(envelopes: &[OperationEnvelope], per_block: usize) -> Vec<StagedChunk> {
    // Each block derives its schema version from its own operations, so a group
    // containing a v1 CreateRegion is stamped major 1 (not V0).
    envelopes
        .chunks(per_block)
        .map(stage_operation_block)
        .collect()
}

/// The reader-side rebuild the spec mandates when no usable index exists:
/// scan every operation block the manifest references, recover each
/// envelope's `(offset, bytes)` with [`envelope_offsets`], peek the leading
/// operation-id bytes with ops' [`peek_operation_id`], and build a fresh
/// [`OperationIndex`]. The same procedure serves the writer at commit time
/// (the spec's SHOULD-rebuild-on-commit), from the committed block refs.
pub fn scan_rebuild_operation_index<S: BlockStore>(bundle: &Bundle<S>) -> OperationIndex {
    let blocks: Vec<IndexedBlock> = bundle
        .manifest()
        .operation_roots
        .iter()
        .map(|root| {
            let payload = bundle.read_chunk(root).expect("operation block reads");
            let entries = envelope_offsets(&payload)
                .expect("a committed block payload decodes")
                .into_iter()
                .map(|(offset, bytes)| {
                    let id = peek_operation_id(bytes)
                        .expect("a canonical envelope leads with its 16 id bytes");
                    (id.canonical_bytes(), offset)
                })
                .collect();
            (*root, entries)
        })
        .collect();
    OperationIndex::build(&blocks).expect("scanned blocks build a valid index")
}

/// Commits `index` as an operation-index chunk and wires it into the
/// manifest's `operation_index_root` — the writer's commit-time update.
fn commit_index<S: BlockStore>(bundle: &mut Bundle<S>, index: &OperationIndex) {
    bundle
        .commit(&[StagedChunk::operation_index(index.encode())], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            m.operation_index_root = Some(ctx.new_chunks[0]);
            m
        })
        .expect("commit operation index");
}

/// Locates every envelope through the index and verifies each `(block,
/// offset)` coordinate addresses exactly that envelope's canonical bytes in
/// the block's decoded payload — plus a miss for an id no envelope uses.
fn assert_index_locates_all<S: BlockStore>(
    bundle: &Bundle<S>,
    index: &OperationIndex,
    envelopes: &[OperationEnvelope],
) {
    for env in envelopes {
        let id = env.id.canonical_bytes();
        let (block_ref, offset) = index
            .locate(&id)
            .unwrap_or_else(|| panic!("operation {:?} missing from the index", env.id));
        let payload = bundle.read_chunk(block_ref).expect("indexed block reads");
        let pairs = envelope_offsets(&payload).expect("indexed block payload decodes");
        let (_, bytes) = pairs
            .iter()
            .find(|(off, _)| *off == offset)
            .expect("the indexed offset lands on an envelope boundary");
        assert_eq!(
            *bytes,
            env.to_canonical_bytes().as_slice(),
            "the (block, offset) coordinate must address exactly this envelope's bytes"
        );
        // The bundle-opaque bytes really lead with this operation's id — the
        // ops-side half of the seam.
        assert_eq!(peek_operation_id(bytes), Some(env.id));
        // The kind-checked block read path sees the same envelope.
        let envs = bundle
            .read_operation_block(block_ref)
            .expect("kind-checked block read");
        assert!(envs.iter().any(|e| e.as_slice() == *bytes));
    }
    assert_eq!(
        index.locate(&[0xFF; 16]),
        None,
        "an id no envelope uses must miss"
    );
}

/// End-to-end: generate real envelopes, pack multi-block, commit, build the
/// index at commit time and wire it into `operation_index_root`, reopen, and
/// verify the index is usable and locates every operation at its exact bytes.
pub fn assert_operation_index_end_to_end(seed: u64) {
    let mut rng = Rng::new(seed);
    let envelopes = generators::operation_envelopes(&mut rng, 36, 3, 8, 8);
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    bundle
        .commit(&staged_envelope_blocks(&envelopes, 12), append_roots)
        .expect("commit operation blocks");

    // Writer side: rebuild the index at commit time from the committed block
    // refs (the spec's SHOULD) and reference it from the manifest.
    let index = scan_rebuild_operation_index(&bundle);
    assert!(
        index.blocks().len() >= 2,
        "the fixture must span multiple blocks to exercise ordinals"
    );
    assert_eq!(index.entries().len(), envelopes.len());
    commit_index(&mut bundle, &index);

    // Reader side: reopen from the durable image; the fresh index is usable
    // and locates every operation.
    let image = bundle.into_store().into_bytes();
    let reopened = Bundle::open(MemStore::from_bytes(image)).expect("reopen");
    let usable = reopened
        .usable_operation_index()
        .expect("a fresh, covering index is usable");
    assert_eq!(usable, index, "the index round-trips through storage");
    assert_index_locates_all(&reopened, &usable, &envelopes);
}

/// A commit that grows the operation set *without* updating the index leaves
/// a stale index: intact as a chunk, but no longer covering the operation
/// roots. Readers must reject it (`usable_operation_index` → `None`) and a
/// scan-rebuild must produce a fresh valid index (spec §"The Operation
/// Index": present but stale → reject and rebuild from blocks).
pub fn assert_stale_operation_index_rejected_and_rebuilt(seed: u64) {
    let mut rng = Rng::new(seed);
    // One authoring session, so ids are unique across both phases.
    let envelopes = generators::operation_envelopes(&mut rng, 48, 3, 8, 8);
    let (first, later) = envelopes.split_at(36);

    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    bundle
        .commit(&staged_envelope_blocks(first, 12), append_roots)
        .expect("commit first blocks");
    let index = scan_rebuild_operation_index(&bundle);
    commit_index(&mut bundle, &index);
    assert!(bundle.usable_operation_index().is_some());

    // Grow the operation set WITHOUT updating the index (the closure carries
    // the old `operation_index_root` forward).
    bundle
        .commit(&staged_envelope_blocks(later, 6), append_roots)
        .expect("commit later blocks");
    let root = bundle
        .manifest()
        .operation_index_root
        .expect("the stale index is still referenced");
    // The chunk itself is intact — readable and well-formed —
    let stale = bundle
        .read_operation_index(&root)
        .expect("the stale index chunk still reads and decodes");
    // — but STALE: its block set no longer equals the operation roots.
    assert!(!stale.covers(&bundle.manifest().operation_roots));
    assert!(
        bundle.usable_operation_index().is_none(),
        "a stale index must be rejected"
    );

    // The same verdict from a cold reopen; then rebuild from blocks.
    let image = bundle.into_store().into_bytes();
    let mut reopened = Bundle::open(MemStore::from_bytes(image)).expect("reopen");
    assert!(reopened.usable_operation_index().is_none());
    let rebuilt = scan_rebuild_operation_index(&reopened);
    commit_index(&mut reopened, &rebuilt);
    let usable = reopened
        .usable_operation_index()
        .expect("the rebuilt index is usable");
    assert_eq!(usable, rebuilt);
    assert_index_locates_all(&reopened, &usable, &envelopes);
}

/// A defective operation index — a garbage payload staged as the index chunk,
/// or on-disk corruption of a valid index chunk's bytes — must be rejected
/// *without* being treated as bundle corruption (Chapter 8 §"Canonical and
/// Non-Canonical Manifest Roots"): the bundle still opens cleanly, canonical
/// chunks verify, canonical reads work, and only `usable_operation_index`
/// says `None` (rebuild from blocks).
pub fn assert_corrupt_operation_index_is_not_bundle_corruption(seed: u64) {
    let mut rng = Rng::new(seed);
    let envelopes = generators::operation_envelopes(&mut rng, 24, 3, 8, 8);
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    bundle
        .commit(&staged_envelope_blocks(&envelopes, 8), append_roots)
        .expect("commit operation blocks");

    // (a) A garbage index chunk, staged and referenced like a real one. The
    // commit succeeds (the bundle does not interpret non-canonical chunks);
    // readers must reject it and rebuild.
    bundle
        .commit(
            &[StagedChunk::operation_index(b"not an index".to_vec())],
            |ctx| {
                let mut m = ctx.previous_manifest.clone();
                m.operation_index_root = Some(ctx.new_chunks[0]);
                m
            },
        )
        .expect("commit garbage index chunk");
    let image = bundle.into_store().into_bytes();
    let mut reopened = Bundle::open(MemStore::from_bytes(image))
        .expect("a defective index must not prevent opening");
    assert!(reopened.anomalies().is_empty());
    assert!(!reopened.is_read_only());
    reopened
        .verify_canonical_chunks()
        .expect("canonical chunks are intact");
    assert!(
        reopened.usable_operation_index().is_none(),
        "a malformed index is rejected: rebuild, not corruption"
    );
    for root in &reopened.manifest().operation_roots.clone() {
        reopened
            .read_operation_block(root)
            .expect("canonical block reads are unaffected");
    }
    // Rebuild-from-blocks restores a usable index.
    let rebuilt = scan_rebuild_operation_index(&reopened);
    commit_index(&mut reopened, &rebuilt);
    let usable = reopened
        .usable_operation_index()
        .expect("the rebuilt index is usable");
    assert_index_locates_all(&reopened, &usable, &envelopes);

    // (b) On-disk corruption of the now-valid index chunk's payload region.
    let valid_image = reopened.into_store().into_bytes();
    let probe = Bundle::open(MemStore::from_bytes(valid_image.clone())).expect("reopen");
    let root = probe
        .manifest()
        .operation_index_root
        .expect("the index is referenced");
    let mut corrupt = valid_image;
    corrupt[(root.offset + 3) as usize] ^= 0xFF;
    let reopened = Bundle::open(MemStore::from_bytes(corrupt))
        .expect("index-region corruption must not prevent opening");
    assert!(reopened.anomalies().is_empty());
    reopened
        .verify_canonical_chunks()
        .expect("canonical chunks are intact");
    // The raw read surfaces the hash defect for diagnostics …
    assert!(
        matches!(
            reopened.read_operation_index(&root),
            Err(BundleError::ChunkHashMismatch { .. })
        ),
        "the raw index read reports the hash mismatch"
    );
    // … while the reject-and-rebuild gate simply declares it unusable.
    assert!(reopened.usable_operation_index().is_none());
    for root in &reopened.manifest().operation_roots {
        reopened
            .read_operation_block(root)
            .expect("canonical block reads are unaffected");
    }
}

// --- The edit-barrier declaration harness (Chapter 8 §"Forward Compatibility
// and Edit Barriers" / §"Behavior Under Unknown Extensions") ------------------

/// The end-to-end edit-barrier round-trip: a manifest [`ExtensionDeclaration`]
/// carrying *really-encoded* barrier blobs (the provisional canonical byte
/// form owned by `epiphany-layout-ir`) commits, reopens byte-verbatim — the
/// bundle preserves the blobs opaquely, exactly as it preserves unknown
/// extension chunks — decodes through the owning layer's codec, and
/// *evaluates*: the reopened barrier prohibits exactly the edits the authored
/// one did.
pub fn run_barrier_declaration_roundtrip(seed: u64) {
    use epiphany_core::{EventId, TypedObjectId};
    use epiphany_layout_ir::{
        decode_affected_object_kinds, decode_edit_barriers, encode_affected_object_kinds,
        encode_edit_barriers, AlwaysLiveOracle, BarrierCondition, BarrierScope, EditBarrier,
        EditContext, ObjectKind, OperationKindTag,
    };

    let mut rng = Rng::new(seed);
    let protected = TypedObjectId::Event(EventId::from_raw(rng.next_u64() as u128));
    // One deterministic barrier the evaluation half asserts against, plus one
    // generated barrier so arbitrary shapes ride the same declaration.
    let locked_barrier = EditBarrier {
        scope: BarrierScope::ObjectSet(vec![protected]),
        affected_object_kinds: vec![ObjectKind::of(&protected)],
        prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
        condition: BarrierCondition::Always,
    };
    let authored = vec![
        locked_barrier.clone(),
        crate::layout_stub::gen_edit_barrier(&mut rng),
    ];
    let kinds = vec![ObjectKind::of(&protected)];

    let declaration = ExtensionDeclaration {
        extension_id: ExtensionId(rng.array16()),
        version: SemVer {
            major: 1,
            minor: 0,
            patch: 0,
        },
        required: false,
        preserved_chunk_roots: Vec::new(),
        affected_object_kinds: encode_affected_object_kinds(&kinds),
        edit_barriers: encode_edit_barriers(&authored),
    };
    let extension_id = declaration.extension_id;
    let barrier_bytes = declaration.edit_barriers.clone();
    let kind_bytes = declaration.affected_object_kinds.clone();

    // Commit a manifest carrying the declaration; reopen from the raw image.
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    bundle
        .commit(&staged(&[rng.byte_vec(1, 40)]), |ctx| {
            let mut m = append_roots(ctx);
            m.extension_declarations.push(declaration.clone());
            m
        })
        .expect("commit the declaration");
    let image = bundle.into_store().into_bytes();
    let reopened = Bundle::open(MemStore::from_bytes(image)).expect("reopen");

    // The bundle preserved the opaque blobs verbatim ...
    let decl = reopened
        .manifest()
        .extension_declarations
        .iter()
        .find(|d| d.extension_id == extension_id)
        .expect("the declaration survives the commit");
    assert_eq!(
        decl.edit_barriers, barrier_bytes,
        "barrier blob is verbatim"
    );
    assert_eq!(
        decl.affected_object_kinds, kind_bytes,
        "object-kind blob is verbatim"
    );

    // ... the owning layer's codec decodes them, re-encoding byte-identically ...
    let decoded = decode_edit_barriers(&decl.edit_barriers).expect("the canonical blob decodes");
    assert_eq!(
        encode_edit_barriers(&decoded),
        barrier_bytes,
        "decode → re-encode is byte-identical"
    );
    assert_eq!(
        decode_affected_object_kinds(&decl.affected_object_kinds).expect("kinds decode"),
        kinds
    );

    // ... and the reopened barrier evaluates as authored: deleting the
    // protected event is prohibited; a different operation class and a
    // different object are not.
    let reopened_barrier = decoded
        .iter()
        .find(|b| **b == locked_barrier)
        .expect("the locked barrier survives the round-trip");
    let ctx = EditContext::default();
    assert!(reopened_barrier.prohibits_edit(
        OperationKindTag::DeleteEvent,
        &protected,
        &ctx,
        &AlwaysLiveOracle
    ));
    assert!(!reopened_barrier.prohibits_edit(
        OperationKindTag::InsertEvent,
        &protected,
        &ctx,
        &AlwaysLiveOracle
    ));
    let other = TypedObjectId::Event(EventId::from_raw(u128::MAX));
    assert!(!reopened_barrier.prohibits_edit(
        OperationKindTag::DeleteEvent,
        &other,
        &ctx,
        &AlwaysLiveOracle
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barrier_declaration_roundtrip() {
        for seed in 0..8u64 {
            run_barrier_declaration_roundtrip(0xBA22_1E20_0000 + seed);
        }
    }

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

    #[test]
    fn operation_index_end_to_end() {
        assert_operation_index_end_to_end(0x0091_D0EC_5EED_0001);
    }

    #[test]
    fn stale_operation_index_is_rejected_and_rebuilt() {
        assert_stale_operation_index_rejected_and_rebuilt(0x0091_D0EC_5EED_0002);
    }

    #[test]
    fn corrupt_operation_index_is_not_bundle_corruption() {
        assert_corrupt_operation_index_is_not_bundle_corruption(0x0091_D0EC_5EED_0003);
    }
}
