//! The crash-recovery fuzzer and the manifest-selection harness — Agent D's
//! acceptance gates (QUICKSTART):
//!
//! > Kill the process between any two syscalls in the commit protocol; reopen;
//! > the bundle must be valid in 100% of runs, and must recover to the previous
//! > generation when the crash precedes the durable flush. This is the most
//! > important single test in the entire prototype.
//!
//! Killing a real process between syscalls cannot be made deterministic, so the
//! fuzzer drives the commit against a [`FaultStore`], which separates *live*
//! (page-cache) bytes from *durable* (survives-a-crash) bytes and can crash
//! after any chosen syscall — optionally tearing the in-flight superblock write,
//! the case the slot CRC must catch. After the simulated crash the bundle is
//! reopened from the durable image and must, in **every** case:
//!
//! 1. open successfully (never corrupt — the gate's "valid in 100% of runs");
//! 2. be at the previous generation *or* the new one, never anything else;
//! 3. if the commit returned `Ok`, be at the new generation; and if the crash
//!    was *clean* (the in-flight flush persisted nothing) and the commit did
//!    not complete, be at the previous generation — the exact acceptance
//!    property "recover to the previous generation when the crash precedes the
//!    durable flush". (A torn final flush may at a full prefix legitimately
//!    persist the whole superblock, the genuine post-commit case, so the torn
//!    branch admits either generation.)
//! 4. report no integrity anomaly;
//! 5. have every canonical chunk present and hash-intact.
//!
//! Both a randomized driver ([`run_crash_recovery_fuzz`], the 10,000-iteration
//! gate) and an *exhaustive* per-commit sweep over every syscall boundary and
//! tear point ([`exhaustive_crash_check`]) are provided. The second is the
//! stronger guarantee; the first explores a wide space of base states and commit
//! shapes.
//!
//! The manifest-selection harness ([`run_manifest_selection_harness`]) builds
//! bundle images by hand and asserts the Chapter 8 §"Superblock Selection" rule
//! across every corruption scenario the QUICKSTART enumerates.

use crate::bundle::{Bundle, CommitContext, StagedChunk, BODY_START};
use crate::chunk::{ChunkKind, ChunkRef, CompressionAlgorithm};
use crate::error::IntegrityAnomaly;
use crate::header::FixedHeader;
use crate::ids::{DocumentId, FileUuid, ReductionAlgorithmVersion, SchemaVersion, WallClockTime};
use crate::manifest::Manifest;
use crate::opindex::OperationIndex;
use crate::store::{CrashPoint, FaultStore, MemStore, Tear};
use crate::superblock::{CommitState, ProfileId, Slot, Superblock, SUPERBLOCK_LEN};
use crate::{block, manifest_chunk_hash};
use epiphany_determinism::{ChunkId, ContentHash};

/// A tiny deterministic generator (SplitMix64), matching `epiphany-determinism`'s
/// fuzz harness: reproducible across platforms, no `rand` dependency, so a
/// failing iteration reproduces exactly from its seed.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seeds the generator.
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    /// Next 64-bit draw.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A draw in `0..n` (`n > 0`).
    #[inline]
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// The manifest builder used throughout the fuzzer: append the commit's new
/// chunks to the previous manifest's `operation_roots`. (Block boundaries are
/// storage artifacts; what matters is that the canonical chunks are reachable.)
fn append_roots(ctx: &CommitContext) -> Manifest {
    let mut m = ctx.previous_manifest.clone();
    m.operation_roots.extend(ctx.new_chunks.iter().copied());
    m
}

/// Builds the staged operation-envelope blocks for a commit from opaque envelope
/// payloads.
fn staged_blocks(envelope_payloads: &[Vec<u8>]) -> Vec<StagedChunk> {
    block::pack_operation_blocks(envelope_payloads)
        .into_iter()
        .map(StagedChunk::operation_block)
        .collect()
}

/// Runs one commit against a [`FaultStore`] crashing at `crash`, recovers from
/// the durable image, and asserts the five crash-safety properties. Panics with
/// context on any violation (the gate's failure condition).
fn check_recovery(base_image: &[u8], base_gen: u64, chunks: &[StagedChunk], crash: CrashPoint) {
    // Open the bundle over a fault store; `open` only reads, so it never
    // consumes crash budget and always succeeds on a valid base image.
    let mut bundle = Bundle::open(FaultStore::new(base_image.to_vec(), crash))
        .expect("base image must open before the commit");
    let committed = bundle.commit(chunks, append_roots).is_ok();
    let store = bundle.into_store();

    // Recover: reopen from exactly the bytes that survived the crash.
    let durable = store.durable_image();
    let recovered = Bundle::open(MemStore::from_bytes(durable.clone())).unwrap_or_else(|e| {
        panic!(
            "crash at {crash:?} left an UNOPENABLE bundle (base gen {base_gen}): {e}\n\
             durable image length {}",
            durable.len()
        )
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
    // The acceptance criterion's exact property: a crash that *precedes* the
    // durable flush recovers to the previous generation. A `Clean` crash on a
    // flush persists nothing from that flush, so the new superblock — which
    // becomes durable only via its own flush — cannot be present unless the
    // commit completed. (A `TornLastWrite` may, at a full prefix, legitimately
    // persist the entire superblock; that is the genuine post-commit case, so it
    // is allowed to recover to either generation.)
    if !committed && crash.tear == Tear::Clean {
        assert_eq!(
            g, base_gen,
            "clean crash at {crash:?}: a commit aborted before its durable flush \
             must recover to the previous generation {base_gen}, got {g}"
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
    recovered.verify_canonical_chunks().unwrap_or_else(|e| {
        panic!("crash at {crash:?}: a canonical chunk is corrupt after recovery: {e}")
    });
}

/// Creates a fresh bundle image and advances it through `commits` clean commits,
/// returning `(image, generation)`. Drives the base state through which slot is
/// active and what the *inactive* slot holds (the previous-previous generation),
/// so the fuzzer exercises commits from many starting configurations.
fn build_base(rng: &mut SplitMix64, commits: u64) -> (Vec<u8>, u64) {
    let doc = DocumentId([(rng.next_u64() & 0xff) as u8; 16]);
    let uuid = FileUuid([(rng.next_u64() & 0xff) as u8; 16]);
    let mut bundle = Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).unwrap();
    for _ in 0..commits {
        let n = rng.below(3) as usize; // 0..2 envelopes
        let envelopes: Vec<Vec<u8>> = (0..n)
            .map(|_| vec![(rng.next_u64() & 0xff) as u8; 1 + rng.below(40) as usize])
            .collect();
        bundle
            .commit(&staged_blocks(&envelopes), append_roots)
            .unwrap();
    }
    let gen = bundle.generation();
    (bundle.into_store().into_bytes(), gen)
}

// ---------------------------------------------------------------------------
// Wire-decode fuzzing (P3 of the decode-hardening track).
//
// The crash-recovery fuzzer above corrupts the image the way a *crash* does:
// torn writes at syscall boundaries. This one corrupts it the way an *attacker*
// or a bit-rotted disk does — arbitrary bytes, anywhere — and drives every
// decode surface the bundle exposes:
//
//   Bundle::open  ·  Manifest::decode  ·  OperationIndex::decode
//   block::decode_block  ·  block::envelope_offsets
//
// Two properties. A mutated image must never panic a decoder (a bundle is
// attacker-controlled input the moment it is emailed), and an *accepted* byte
// string must re-encode to itself where the type has a canonical encoding.
//
// P2 (`epiphany-ops`) established that a re-encode guard is complete only for
// fields the encoder NORMALIZES. `Manifest::encode_body` sorts and deduplicates
// every vector, so its guard genuinely is complete; `OperationIndex::decode`
// has no guard and instead checks its two `Vec`s for strict ascent per-site,
// which is the other correct answer. Both are asserted here.
// ---------------------------------------------------------------------------

/// What a decode-fuzz run actually reached. A harness that never gets a decoder
/// to say `Ok` proves only the absence of a panic.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct WireFuzzCoverage {
    pub opens_ok: u64,
    pub opens_rejected: u64,
    pub manifests_ok: u64,
    pub manifests_rejected: u64,
    pub blocks_ok: u64,
    pub blocks_rejected: u64,
    pub indices_ok: u64,
    pub indices_rejected: u64,
}

fn wire_random_bytes(rng: &mut SplitMix64, n: usize) -> Vec<u8> {
    (0..n).map(|_| rng.next_u64() as u8).collect()
}

fn wire_substitute(rng: &mut SplitMix64, bytes: &mut [u8], k: usize) {
    if bytes.is_empty() {
        return;
    }
    for _ in 0..k {
        let i = (rng.next_u64() as usize) % bytes.len();
        bytes[i] = rng.next_u64() as u8;
    }
}

/// Overwrites a random 4- or 8-byte window with an extreme integer: the
/// count/length/offset attack. A bundle's manifest carries file offsets and
/// lengths, so this is the mutation that matters most here.
fn wire_corrupt_int(rng: &mut SplitMix64, bytes: &mut [u8]) {
    if bytes.len() < 8 {
        return;
    }
    let i = (rng.next_u64() as usize) % (bytes.len() - 7);
    match rng.next_u64() % 4 {
        0 => bytes[i..i + 4].copy_from_slice(&u32::MAX.to_le_bytes()),
        1 => bytes[i..i + 8].copy_from_slice(&u64::MAX.to_le_bytes()),
        2 => {
            bytes[i..i + 8].copy_from_slice(&(rng.next_u64() | 0x8000_0000_0000_0000).to_le_bytes())
        }
        _ => bytes[i..i + 4].copy_from_slice(&(rng.next_u64() as u32).to_le_bytes()),
    }
}

fn mutate_image(rng: &mut SplitMix64, base: &[u8]) -> Vec<u8> {
    let mut b = base.to_vec();
    match rng.next_u64() % 6 {
        0 => return b, // unmutated: a live check that the corpus opens
        1 => {
            let k = 1 + (rng.next_u64() % 6) as usize;
            wire_substitute(rng, &mut b, k);
        }
        2 => {
            let t = (rng.next_u64() as usize) % (b.len() + 1);
            b.truncate(t);
        }
        3 => {
            let n = 1 + (rng.next_u64() % 32) as usize;
            let tail = wire_random_bytes(rng, n);
            b.extend_from_slice(&tail);
        }
        4 => wire_corrupt_int(rng, &mut b),
        _ => {
            // Corrupt the superblock region specifically: the two 256-byte slots
            // whose CRCs gate `open`.
            let n = b.len().min(512);
            if n > 0 {
                let i = (rng.next_u64() as usize) % n;
                b[i] = rng.next_u64() as u8;
            }
        }
    }
    b
}

/// Runs `iters` adversarial wire-decode iterations from `seed` over the bundle's
/// decode surfaces. A panic, or an accepted byte string that does not re-encode
/// to itself, fails the run; `seed` reproduces it exactly.
pub fn run_wire_decode_fuzz(iters: u64, seed: u64) -> WireFuzzCoverage {
    let mut rng = SplitMix64::new(seed);
    let mut cov = WireFuzzCoverage::default();

    // A small pool of valid, populated images and valid manifest payloads.
    let mut images: Vec<Vec<u8>> = Vec::new();
    let mut manifests: Vec<Vec<u8>> = Vec::new();
    for commits in 0..4u64 {
        let (image, _) = build_base(&mut rng, commits);
        let bundle = Bundle::open(MemStore::from_bytes(image.clone())).expect("valid image opens");
        manifests.push(bundle.manifest().encode());
        images.push(image);
    }
    let valid_blocks: Vec<Vec<u8>> = {
        let payloads: Vec<Vec<u8>> = (0..3).map(|i| vec![i as u8 + 1; 8 + i * 5]).collect();
        block::pack_operation_blocks(&payloads)
    };

    // Valid operation-index payloads. Random bytes never decode, so an index
    // corpus of noise leaves the decoder's accept path — and therefore every
    // assertion below it — unreached (measured: `indices_ok` was 0).
    let valid_indices: Vec<Vec<u8>> = {
        let block_ref = |hash_byte: u8, offset: u64, len: u64| ChunkRef {
            id: ChunkId(ContentHash([hash_byte; 32])),
            kind: ChunkKind::OperationEnvelopeBlock,
            schema_version: SchemaVersion::V0,
            offset,
            compressed_length: len,
            uncompressed_length: len,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([hash_byte; 32]),
        };
        [
            OperationIndex::build(&[]).expect("empty index"),
            OperationIndex::build(&[(block_ref(0x11, 576, 64), vec![([2; 16], 8)])])
                .expect("one block"),
            OperationIndex::build(&[
                (block_ref(0x22, 1000, 64), vec![([3; 16], 8), ([1; 16], 40)]),
                (block_ref(0x11, 576, 32), vec![([2; 16], 8)]),
            ])
            .expect("two blocks"),
        ]
        .iter()
        .map(|i| i.encode())
        .collect()
    };

    // The corpus must decode. An *unmutated* valid byte string that a decoder
    // rejects would otherwise be tallied as a rejection, like any garbage input
    // — which is exactly how a broken `OperationKindTag` decoder hid inside the
    // ops fuzzer for two commits (Push 5 / P4).
    for bytes in &manifests {
        let m = Manifest::decode(bytes).expect("a valid manifest must decode");
        assert_eq!(m.encode(), *bytes);
    }
    for bytes in &valid_indices {
        let i = OperationIndex::decode(bytes).expect("a valid operation index must decode");
        assert_eq!(i.encode(), *bytes);
    }
    for bytes in &valid_blocks {
        block::decode_block(bytes).expect("a valid block payload must decode");
    }

    for _ in 0..iters {
        // 1. Whole-image open. Must never panic; an Ok manifest must re-encode.
        let pick = (rng.next_u64() as usize) % images.len();
        let image = mutate_image(&mut rng, &images[pick]);
        match Bundle::open(MemStore::from_bytes(image)) {
            Ok(bundle) => {
                cov.opens_ok += 1;
                let encoded = bundle.manifest().encode();
                assert_eq!(
                    Manifest::decode(&encoded).as_ref(),
                    Ok(bundle.manifest()),
                    "an opened bundle's manifest does not round-trip"
                );
                // Reading every chunk the manifest names must be total.
                for r in bundle.manifest().canonical_chunk_refs() {
                    let _ = bundle.read_chunk(&r);
                }
            }
            Err(_) => cov.opens_rejected += 1,
        }

        // 2. Manifest payload decode: strict-canonical, guard-backed.
        let mut m = manifests[(rng.next_u64() as usize) % manifests.len()].clone();
        match rng.next_u64() % 4 {
            0 => {}
            1 => wire_substitute(&mut rng, &mut m, 1),
            2 => wire_corrupt_int(&mut rng, &mut m),
            _ => {
                let t = (rng.next_u64() as usize) % (m.len() + 1);
                m.truncate(t);
            }
        }
        match Manifest::decode(&m) {
            Ok(manifest) => {
                cov.manifests_ok += 1;
                assert_eq!(
                    manifest.encode(),
                    m,
                    "the manifest decoder accepted a non-canonical byte string"
                );
            }
            Err(_) => cov.manifests_rejected += 1,
        }

        // 3. Block payload framing.
        let mut b = valid_blocks[(rng.next_u64() as usize) % valid_blocks.len()].clone();
        match rng.next_u64() % 4 {
            0 => {}
            1 => wire_substitute(&mut rng, &mut b, 1),
            2 => wire_corrupt_int(&mut rng, &mut b),
            _ => {
                let t = (rng.next_u64() as usize) % (b.len() + 1);
                b.truncate(t);
            }
        }
        match block::decode_block(&b) {
            Ok(envelopes) => {
                cov.blocks_ok += 1;
                // `envelope_offsets` shares the code path: the two must agree,
                // and each recorded offset must actually address its envelope.
                let offsets = block::envelope_offsets(&b).expect("same validation");
                assert_eq!(offsets.len(), envelopes.len());
                for ((offset, slice), env) in offsets.iter().zip(envelopes.iter()) {
                    assert_eq!(
                        *slice,
                        &env[..],
                        "envelope_offsets disagrees with decode_block"
                    );
                    let at = *offset as usize;
                    assert_eq!(
                        &b[at..at + env.len()],
                        &env[..],
                        "offset does not address the envelope"
                    );
                }
            }
            Err(_) => cov.blocks_rejected += 1,
        }

        // 4. Operation index: no re-encode guard; per-site strict-ascent checks.
        let mut idx = valid_indices[(rng.next_u64() as usize) % valid_indices.len()].clone();
        match rng.next_u64() % 5 {
            0 => {}
            1 => wire_substitute(&mut rng, &mut idx, 1),
            2 => wire_corrupt_int(&mut rng, &mut idx),
            3 => {
                let t = (rng.next_u64() as usize) % (idx.len() + 1);
                idx.truncate(t);
            }
            _ => {
                let n = (rng.next_u64() % 96) as usize;
                idx = wire_random_bytes(&mut rng, n);
            }
        }
        match OperationIndex::decode(&idx) {
            Ok(index) => {
                cov.indices_ok += 1;
                // No re-encode guard here; the decoder's per-site checks are the
                // contract, so assert them directly — and assert injectivity,
                // which the `Vec`-order preservation makes non-trivial.
                assert_eq!(
                    index.encode(),
                    idx,
                    "the operation-index decoder accepted a non-canonical byte string"
                );
                assert!(
                    index.blocks().windows(2).all(|w| w[0] < w[1]),
                    "the index decoder accepted unsorted blocks"
                );
                assert!(
                    index.entries().windows(2).all(|w| w[0].id < w[1].id),
                    "the index decoder accepted unsorted entries"
                );
            }
            Err(_) => cov.indices_rejected += 1,
        }
    }
    cov
}

/// The crash-recovery fuzzer: `iters` randomized scenarios from `seed`. Each
/// iteration builds a base bundle at a random generation, then commits a random
/// set of operation blocks while crashing at a random syscall with a random
/// tear, and asserts full recovery. Deterministic: a failure reproduces from its
/// seed.
pub fn run_crash_recovery_fuzz(iters: u64, seed: u64) {
    let mut rng = SplitMix64::new(seed);
    for _ in 0..iters {
        // A base bundle at generation 0..3 (so the inactive slot variously holds
        // nothing, an older generation, etc.).
        let base_commits = rng.below(4);
        let (base_image, base_gen) = build_base(&mut rng, base_commits);

        // A commit of 0..3 operation blocks with small random payloads.
        let n = rng.below(4) as usize;
        let envelopes: Vec<Vec<u8>> = (0..n)
            .map(|_| vec![(rng.next_u64() & 0xff) as u8; 1 + rng.below(64) as usize])
            .collect();
        let chunks = staged_blocks(&envelopes);

        // A crash after 0..15 syscalls (covers every step of a small commit plus
        // the no-crash case), with a random tear over the 256-byte slot.
        let crash = CrashPoint {
            after_syscalls: rng.below(16) as u32,
            tear: random_tear(&mut rng),
        };
        check_recovery(&base_image, base_gen, &chunks, crash);
    }
}

/// A random tear mode: half the time a clean crash, half a torn write to a
/// random prefix of a 256-byte slot (including the boundary values 0, the
/// 252-byte CRC offset, and a full 256).
fn random_tear(rng: &mut SplitMix64) -> Tear {
    if rng.next_u64() & 1 == 0 {
        Tear::Clean
    } else {
        let prefix = match rng.below(6) {
            0 => 0,
            1 => 1,
            2 => 252,
            3 => 255,
            4 => SUPERBLOCK_LEN as usize, // 256 (full)
            _ => rng.below(SUPERBLOCK_LEN) as usize,
        };
        Tear::TornLastWrite { prefix }
    }
}

/// The tear points the exhaustive sweep tries at every syscall boundary: a clean
/// crash plus torn writes around the CRC offset (252) and slot size (256).
fn exhaustive_tears() -> Vec<Tear> {
    let mut tears = vec![Tear::Clean];
    for prefix in [0usize, 1, 100, 128, 251, 252, 253, 255, 256] {
        tears.push(Tear::TornLastWrite { prefix });
    }
    tears
}

/// Exhaustively checks crash recovery for one commit: every crash budget from 0
/// through the commit's total syscall count, crossed with every tear point. This
/// is the strongest crash-safety guarantee — it leaves no syscall boundary
/// untested. `make_chunks` is called per run so each gets fresh payloads.
pub fn exhaustive_crash_check(base_image: &[u8], base_gen: u64, envelope_payloads: &[Vec<u8>]) {
    let chunks = staged_blocks(envelope_payloads);

    // Learn the commit's total syscall count (and confirm the clean commit
    // recovers to G+1) via a no-fault run.
    let total = {
        let mut bundle = Bundle::open(FaultStore::no_fault(base_image.to_vec())).unwrap();
        bundle.commit(&chunks, append_roots).unwrap();
        let store = bundle.into_store();
        let recovered = Bundle::open(store.recover()).unwrap();
        assert_eq!(recovered.generation(), base_gen + 1);
        store.syscalls_issued()
    };

    // Crash after every k in 0..=total, with every tear point.
    for k in 0..=total {
        for tear in exhaustive_tears() {
            check_recovery(
                base_image,
                base_gen,
                &chunks,
                CrashPoint {
                    after_syscalls: k,
                    tear,
                },
            );
        }
    }
}

// ===========================================================================
// Manifest selection harness
// ===========================================================================

/// Builds a bundle image by hand, slot by slot, for the selection harness. This
/// is the only place that assembles a prelude directly (rather than through the
/// commit protocol), so every corruption scenario can be expressed precisely.
struct ImageBuilder {
    bytes: Vec<u8>,
    cursor: u64,
}

impl ImageBuilder {
    /// A fresh image: header at 0, both slots zeroed (invalid), body cursor at
    /// [`BODY_START`].
    fn new() -> Self {
        let mut bytes = vec![0u8; BODY_START as usize];
        let header = FixedHeader::new(FileUuid([42; 16])).encode();
        bytes[0..header.len()].copy_from_slice(&header);
        ImageBuilder {
            bytes,
            cursor: BODY_START,
        }
    }

    /// Appends a manifest chunk, returning a superblock that points at it (with
    /// the given generation). The superblock is valid and committed; the caller
    /// places it in a slot.
    fn add_manifest(&mut self, generation: u64, manifest: &Manifest) -> Superblock {
        // The manifest's own generation must match the superblock that points at
        // it (a conforming writer always keeps them in lockstep), so set it here.
        let mut manifest = manifest.clone();
        manifest.generation = generation;
        let payload = manifest.encode();
        let offset = self.cursor;
        let end = offset as usize + payload.len();
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
        }
        self.bytes[offset as usize..end].copy_from_slice(&payload);
        self.cursor = end as u64;
        Superblock {
            generation,
            manifest_offset: offset,
            manifest_length: payload.len() as u64,
            manifest_hash: manifest_chunk_hash(&payload),
            manifest_schema_version: SchemaVersion::V0,
            reduction_algorithm_version: ReductionAlgorithmVersion(0),
            profile_id: ProfileId::Full,
            commit_state: CommitState::Committed,
            commit_timestamp: WallClockTime(0),
        }
    }

    /// Writes a superblock into the given slot.
    fn set_slot(&mut self, slot: Slot, sb: &Superblock) {
        let off = slot.offset() as usize;
        self.bytes[off..off + SUPERBLOCK_LEN as usize].copy_from_slice(&sb.encode());
    }

    /// Corrupts a slot by flipping a byte inside its CRC-covered region (so the
    /// slot fails CRC and is invalid for ordinary selection — a "torn write").
    fn corrupt_slot(&mut self, slot: Slot) {
        let off = slot.offset() as usize + 64; // inside the field region
        self.bytes[off] ^= 0xFF;
    }

    fn store(&self) -> MemStore {
        MemStore::from_bytes(self.bytes.clone())
    }
}

fn manifest_with_marker(marker: u8) -> Manifest {
    Manifest::empty(DocumentId([marker; 16]))
}

/// Runs every manifest-selection scenario the QUICKSTART enumerates, panicking
/// on any deviation from the Chapter 8 §"Superblock Selection" rule. This is the
/// second Agent D acceptance gate.
pub fn run_manifest_selection_harness() {
    // Scenario: slot A corrupt + slot B valid -> select B.
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(1);
        let sb_a = b.add_manifest(0, &m);
        let mut sb_b = b.add_manifest(1, &m);
        sb_b.generation = 1;
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        b.corrupt_slot(Slot::A);
        let bundle = Bundle::open(b.store()).expect("slot B is valid; bundle must open");
        assert_eq!(bundle.active_slot(), Slot::B);
        assert_eq!(bundle.generation(), 1);
        assert!(bundle.anomalies().is_empty());
    }

    // Scenario: slot A valid + slot B corrupt -> select A.
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(2);
        let sb_a = b.add_manifest(7, &m);
        let sb_b = b.add_manifest(8, &m);
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        b.corrupt_slot(Slot::B);
        let bundle = Bundle::open(b.store()).expect("slot A is valid; bundle must open");
        assert_eq!(bundle.active_slot(), Slot::A);
        assert_eq!(bundle.generation(), 7);
        assert!(bundle.anomalies().is_empty());
    }

    // Scenario: both valid, generations differ by one -> select the higher.
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(3);
        let sb_a = b.add_manifest(4, &m);
        let sb_b = b.add_manifest(5, &m);
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        let bundle = Bundle::open(b.store()).unwrap();
        assert_eq!(bundle.generation(), 5);
        assert_eq!(bundle.active_slot(), Slot::B);
        assert!(bundle.anomalies().is_empty());
    }

    // Scenario: both valid, same generation, same manifest -> equivalent, pick A.
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(4);
        let sb = b.add_manifest(9, &m);
        b.set_slot(Slot::A, &sb);
        b.set_slot(Slot::B, &sb);
        let bundle = Bundle::open(b.store()).unwrap();
        assert_eq!(bundle.active_slot(), Slot::A);
        assert_eq!(bundle.generation(), 9);
        assert!(bundle.anomalies().is_empty());
        assert!(!bundle.is_read_only());
    }

    // Scenario: both valid, same generation, divergent manifests -> anomaly,
    // read-only recovery.
    {
        let mut b = ImageBuilder::new();
        let m_a = manifest_with_marker(5);
        let m_b = manifest_with_marker(6);
        let mut sb_a = b.add_manifest(9, &m_a);
        let mut sb_b = b.add_manifest(9, &m_b);
        sb_a.generation = 9;
        sb_b.generation = 9;
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        let bundle = Bundle::open(b.store()).unwrap();
        assert_eq!(
            bundle.anomalies(),
            &[IntegrityAnomaly::DivergentSameGeneration { generation: 9 }]
        );
        assert!(bundle.is_read_only());
    }

    // Scenario: both valid, generations differ by more than one -> anomaly, but
    // opens (read-only) at the higher generation.
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(7);
        let mut sb_a = b.add_manifest(2, &m);
        let mut sb_b = b.add_manifest(9, &m);
        sb_a.generation = 2;
        sb_b.generation = 9;
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        let bundle = Bundle::open(b.store()).unwrap();
        assert_eq!(bundle.generation(), 9);
        assert_eq!(
            bundle.anomalies(),
            &[IntegrityAnomaly::GenerationGap {
                active: 9,
                other: 2
            }]
        );
        assert!(bundle.is_read_only());
    }

    // Scenario: neither slot valid -> hard error (corrupt).
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(8);
        let sb_a = b.add_manifest(1, &m);
        let sb_b = b.add_manifest(2, &m);
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        b.corrupt_slot(Slot::A);
        b.corrupt_slot(Slot::B);
        assert!(matches!(
            Bundle::open(b.store()),
            Err(crate::BundleError::NoValidSuperblock)
        ));
    }

    // Scenario: a slot whose commit_state is not Committed is excluded, and its
    // presence is reported as an anomaly.
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(9);
        let sb_a = b.add_manifest(3, &m);
        let mut sb_b = b.add_manifest(4, &m);
        sb_b.commit_state = CommitState::Reserved(1);
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        let bundle = Bundle::open(b.store()).unwrap();
        // Slot B is not committed -> excluded; A (gen 3) is selected. The
        // non-committed slot is surfaced as an anomaly, but this is ordinary
        // fallback (the next commit overwrites the bad slot), so the bundle is
        // NOT forced read-only.
        assert_eq!(bundle.generation(), 3);
        assert!(bundle
            .anomalies()
            .contains(&IntegrityAnomaly::NonCommittedSlot));
        assert!(!bundle.is_read_only());
    }

    // Scenario: a slot pointing at a manifest whose hash does not verify is
    // excluded (Chapter 8 §"Superblock Selection", step 2).
    {
        let mut b = ImageBuilder::new();
        let m = manifest_with_marker(10);
        let sb_a = b.add_manifest(5, &m);
        let mut sb_b = b.add_manifest(6, &m);
        // Point slot B's manifest_hash at the wrong value.
        sb_b.manifest_hash = manifest_chunk_hash(b"not the manifest");
        b.set_slot(Slot::A, &sb_a);
        b.set_slot(Slot::B, &sb_b);
        let bundle = Bundle::open(b.store()).expect("slot A is valid");
        assert_eq!(bundle.active_slot(), Slot::A);
        assert_eq!(bundle.generation(), 5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_recovery_fuzz_smoke() {
        // A quick run under the unit-test timeout; the heavy 10k-iteration gate
        // lives in tests/crash_recovery.rs and the example binary.
        run_crash_recovery_fuzz(500, 0xD00D_F00D_1234_5678);
    }

    #[test]
    fn exhaustive_sweep_for_a_representative_commit() {
        // Build a base at generation 2, then exhaustively crash a 2-block commit.
        let mut rng = SplitMix64::new(0x00A1_1CE5);
        let (image, gen) = build_base(&mut rng, 2);
        let envelopes = vec![b"alpha".to_vec(), b"beta".to_vec(), vec![7u8; 300]];
        exhaustive_crash_check(&image, gen, &envelopes);
    }

    #[test]
    fn exhaustive_sweep_from_a_fresh_bundle() {
        // The first commit (generation 0 -> 1): the inactive slot starts zeroed.
        let mut rng = SplitMix64::new(1);
        let (image, gen) = build_base(&mut rng, 0);
        assert_eq!(gen, 0);
        exhaustive_crash_check(&image, gen, &[b"only-envelope".to_vec()]);
    }

    #[test]
    fn manifest_selection_harness_passes() {
        run_manifest_selection_harness();
    }

    /// Two deterministic smoke seeds over every bundle decode surface.
    ///
    /// The coverage assertions are load-bearing. A wire fuzzer that never gets a
    /// decoder to say `Ok` proves only the absence of a panic — and this one
    /// initially reached the operation index's accept path *zero* times, because
    /// random bytes never decode as an index. It found the lenient
    /// `CompressionAlgorithm::None` parameter byte only once its index corpus
    /// was real.
    #[test]
    fn wire_decode_fuzz_smoke_seed_a() {
        let cov = run_wire_decode_fuzz(20_000, 0x0DEC_0DE0_F022_1234);
        assert!(cov.opens_ok > 1_000, "{cov:?}");
        assert!(cov.opens_rejected > 1_000, "{cov:?}");
        assert!(cov.manifests_ok > 1_000, "{cov:?}");
        assert!(cov.manifests_rejected > 1_000, "{cov:?}");
        assert!(cov.blocks_ok > 1_000, "{cov:?}");
        assert!(cov.blocks_rejected > 1_000, "{cov:?}");
        assert!(cov.indices_ok > 1_000, "{cov:?}");
        assert!(cov.indices_rejected > 1_000, "{cov:?}");
    }

    #[test]
    fn wire_decode_fuzz_smoke_seed_b() {
        let cov = run_wire_decode_fuzz(20_000, 0xF0FA_11BA_C0DE_5EED);
        assert!(cov.opens_ok > 1_000, "{cov:?}");
        assert!(cov.manifests_ok > 1_000, "{cov:?}");
        assert!(cov.blocks_ok > 1_000, "{cov:?}");
        assert!(cov.indices_ok > 1_000, "{cov:?}");
    }
}
