//! The Chapter 10 file-format budgets: typical-edit bundle write and
//! manifest+bootstrap read (Phase 2 worklist F1).
//!
//! The normative budgets (`spec/core_spec.tex`, Chapter 10 §"File Format
//! Performance"):
//!
//! > Bundle write of a typical edit (one or more operation envelopes appended
//! > to the operation-envelope block stream, manifest rewrite, superblock flip)
//! > completes within 50 ms at p99 on the reference hardware profile.
//! >
//! > Bundle read of the manifest and bootstrap chunks (sufficient for first
//! > interactive frame) completes within 200 ms at p99 for a 100-page
//! > orchestral score.
//!
//! Both rows run against a real on-disk bundle (`FileStore`, whose flush is a
//! genuine `fsync`) in a scratch directory under the build's **target dir** —
//! deliberately not `std::env::temp_dir()`, which is commonly tmpfs on Linux,
//! where fsync is a near-no-op and the commit budget would be measured against
//! RAM. The corpus is moderate: 1,000 generated operation envelopes packed
//! into operation blocks plus a canonical `MaterializedState` snapshot wired
//! as the manifest's `canonical_base`. Both
//! are expected to **Pass** today, so a regression fails `cargo bench` loudly.
//! The read row's honesty note: the spec sizes its 200 ms against a 100-page
//! orchestral score; no such corpus generator exists yet, so this row is the
//! moderate-corpus stand-in (recorded in `DECISIONS.md` F1) and the budget is
//! asserted with margin to spare. The OS page cache is warm across iterations
//! (dropping it needs privileges); each iteration's *process-level* state is
//! fresh.
//!
//! The timed sections:
//!
//! * **typical_edit_commit** — on an opened bundle restored to the same base
//!   image (restore + open are un-timed setup): one `commit` staging a small
//!   envelope block, i.e. block append + manifest rewrite + superblock flip,
//!   every write fsync'd. This mirrors `bundle_harness`'s commit driver.
//! * **open_bootstrap_read** — `FileStore::open` + `Bundle::open` (superblock
//!   selection + header + manifest decode) + reading the `canonical_base`
//!   snapshot and every operation block — the bytes a first interactive frame
//!   needs.
//!
//! Criterion measures; the budget gate in `main` asserts (see
//! `epiphany_testkit::budget` for the Pass/Xfail semantics and the documented
//! deviation from Chapter 10's p99-over-1000-iterations conformance
//! methodology). Run: `cargo bench -p epiphany-testkit --bench bundle`;
//! `EPIPHANY_BENCH_QUICK=1` shrinks sampling for PR CI.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{BatchSize, Criterion};
use epiphany_bundle::{
    pack_operation_blocks, Bundle, ChunkKind, CommitContext, DocumentId, FileStore, FileUuid,
    FrontierBytes, Manifest, MemStore, ProfileId, ReductionAlgorithmVersion, SchemaVersion,
    SnapshotId, SnapshotRef, StagedChunk,
};
use epiphany_determinism::CanonicalEncode;
use epiphany_ops::{OperationEnvelope, OperationSet};
use epiphany_testkit::budget::{self, Expectation};
use epiphany_testkit::{generators, Rng};

/// Chapter 10: a typical-edit bundle write completes within 50 ms (p99).
const COMMIT_BUDGET: Duration = Duration::from_millis(50);
/// Chapter 10: manifest + bootstrap read completes within 200 ms (p99).
const READ_BUDGET: Duration = Duration::from_millis(200);

/// Base-corpus scale: the criterion-5 envelope count, packed into real blocks.
const BASE_ENVELOPES: usize = 1_000;
/// The typical edit: a handful of envelopes appended as one block.
const EDIT_ENVELOPES: usize = 4;

/// Everything the two rows measure against, built once from a fixed seed.
struct Fixture {
    /// The committed base image (blocks + canonical-base snapshot).
    base_image: Vec<u8>,
    /// The typical edit, staged (one small operation block).
    edit: Vec<StagedChunk>,
    /// Temp-dir file paths: one per row so commit growth never skews reads.
    commit_path: PathBuf,
    read_path: PathBuf,
}

/// The commit-context closure the harness uses: append the new chunks to the
/// previous manifest's `operation_roots`.
fn append_roots(ctx: &CommitContext) -> Manifest {
    let mut manifest = ctx.previous_manifest.clone();
    manifest
        .operation_roots
        .extend(ctx.new_chunks.iter().copied());
    manifest
}

fn staged_blocks(envelopes: &[OperationEnvelope]) -> Vec<StagedChunk> {
    let payloads: Vec<Vec<u8>> = envelopes.iter().map(|e| e.to_canonical_bytes()).collect();
    pack_operation_blocks(&payloads)
        .into_iter()
        .map(StagedChunk::operation_block)
        .collect()
}

/// Builds the moderate base corpus: 1,000 envelopes committed as operation
/// blocks, then their cold reduction committed as a `Snapshot` chunk wired to
/// the manifest's `canonical_base` (the roundtrip harness's snapshot shape).
fn build_fixture(dir: &Path) -> Fixture {
    let mut rng = Rng::new(0x00F1_B0DE_0001);
    let envelopes = generators::operation_envelopes(&mut rng, BASE_ENVELOPES, 3, 40, 40);
    let edit_envelopes = generators::operation_envelopes(&mut rng, EDIT_ENVELOPES, 3, 8, 8);

    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create base bundle");
    bundle
        .commit(&staged_blocks(&envelopes), append_roots)
        .expect("commit base operation blocks");

    // The canonical base: the corpus's cold reduction, stored as a snapshot.
    let mut set = OperationSet::new();
    set.accept_all(envelopes.iter().cloned());
    let canonical = set.reduce().canonical_bytes();
    let snapshot = StagedChunk {
        kind: ChunkKind::Snapshot,
        schema_version: SchemaVersion::V0,
        payload: canonical,
    };
    let frontier = generators::frontier_bytes(&envelopes);
    bundle
        .commit(&[snapshot], |ctx| {
            let mut manifest = ctx.previous_manifest.clone();
            let root = ctx.new_chunks[0];
            let mut sid = [0u8; 16];
            sid.copy_from_slice(&root.hash.as_bytes()[..16]);
            manifest.canonical_base = Some(SnapshotRef {
                snapshot_id: SnapshotId(sid),
                covers_causal_frontier: FrontierBytes::from_bytes(frontier.clone()),
                reduction_algorithm_version: ReductionAlgorithmVersion(0),
                profile_id: ProfileId::Full,
                hash: root.hash,
                root,
            });
            manifest
        })
        .expect("commit canonical-base snapshot");

    Fixture {
        base_image: bundle.into_store().into_bytes(),
        edit: staged_blocks(&edit_envelopes),
        commit_path: dir.join("commit.epb"),
        read_path: dir.join("read.epb"),
    }
}

/// Un-timed setup for the commit row: restore the base image and open it.
fn restore_and_open(path: &Path, image: &[u8]) -> Bundle<FileStore> {
    fs::write(path, image).expect("restore base image");
    Bundle::open(FileStore::open(path).expect("open store")).expect("open bundle")
}

/// The timed commit: block append + manifest rewrite + superblock flip, fsync'd.
fn typical_edit_commit(mut bundle: Bundle<FileStore>, edit: &[StagedChunk]) -> u64 {
    bundle
        .commit(edit, append_roots)
        .expect("typical-edit commit");
    bundle.generation()
}

/// The timed read: open (superblock selection + manifest decode) + the
/// bootstrap chunks — canonical-base snapshot and every operation block.
fn open_bootstrap_read(path: &Path) -> usize {
    let bundle = Bundle::open(FileStore::open(path).expect("open store")).expect("open bundle");
    let manifest = bundle.manifest();
    let mut bytes = 0usize;
    let base = manifest
        .canonical_base
        .as_ref()
        .expect("the fixture wires a canonical base");
    bytes += bundle
        .read_chunk(&base.root)
        .expect("snapshot chunk reads")
        .len();
    for root in &manifest.operation_roots {
        for envelope in bundle
            .read_operation_block(root)
            .expect("operation block reads")
        {
            bytes += envelope.len();
        }
    }
    bytes
}

/// The criterion measurement side.
fn criterion_measurements(criterion: &mut Criterion, fixture: &Fixture, quick: bool) {
    let mut group = criterion.benchmark_group("bundle");
    group.sample_size(if quick { 10 } else { 30 });
    group.measurement_time(Duration::from_secs(if quick { 1 } else { 4 }));
    group.warm_up_time(Duration::from_millis(if quick { 300 } else { 1000 }));

    group.bench_function("typical_edit_commit", |b| {
        b.iter_batched(
            || restore_and_open(&fixture.commit_path, &fixture.base_image),
            |bundle| typical_edit_commit(bundle, &fixture.edit),
            BatchSize::PerIteration,
        )
    });

    fs::write(&fixture.read_path, &fixture.base_image).expect("write read-row image");
    group.bench_function("open_bootstrap_read", |b| {
        b.iter(|| open_bootstrap_read(&fixture.read_path))
    });

    group.finish();
}

/// The budget-gate side: both rows are expected to **Pass** today.
fn budget_gate(fixture: &Fixture, quick: bool) -> Vec<budget::GateReport> {
    let commit_median = budget::median_time(
        if quick { 12 } else { 40 },
        || restore_and_open(&fixture.commit_path, &fixture.base_image),
        |bundle| typical_edit_commit(bundle, &fixture.edit),
    );
    fs::write(&fixture.read_path, &fixture.base_image).expect("write read-row image");
    let read_median = budget::median_time(
        if quick { 8 } else { 25 },
        || (),
        |()| open_bootstrap_read(&fixture.read_path),
    );
    vec![
        budget::latency_gate(
            "bundle/typical_edit_commit",
            commit_median,
            COMMIT_BUDGET,
            Expectation::Pass,
        ),
        budget::latency_gate(
            "bundle/open_bootstrap_read",
            read_median,
            READ_BUDGET,
            Expectation::Pass,
        ),
    ]
}

/// A scratch directory on the **build target's filesystem** (real disk), not
/// `temp_dir()`: `/tmp` is commonly tmpfs, where the commit row's fsyncs would
/// be free and the 50 ms budget vacuous. Honors `CARGO_TARGET_DIR`.
fn scratch_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target")
        });
    target.join(format!("f1-bundle-bench-{}", std::process::id()))
}

fn main() {
    // `cargo bench` passes `--bench`; its absence means criterion's test mode
    // (`cargo test --benches` / `--all-targets`): run each measurement once,
    // skip the gate.
    let bench_mode = std::env::args().any(|arg| arg == "--bench");
    let quick = budget::quick_mode();

    let dir = scratch_dir();
    fs::create_dir_all(&dir).expect("create bench scratch dir");
    let fixture = build_fixture(&dir);

    let mut criterion = Criterion::default().configure_from_args();
    criterion_measurements(&mut criterion, &fixture, quick);
    criterion.final_summary();

    let holds = if bench_mode {
        budget::verdict(&budget_gate(&fixture, quick))
    } else {
        true
    };

    let _ = fs::remove_dir_all(&dir);
    if !holds {
        std::process::exit(1);
    }
}
