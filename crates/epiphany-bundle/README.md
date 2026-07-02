# epiphany-bundle

The Epiphany `.musc` file format, implementing the normative requirements of
**Chapter 8 (File Format)** of the core specification (`spec/core_spec.pdf`).
This is Agent D's crate per `spec/QUICKSTART.md`. It depends on
`epiphany-determinism` (Agent A) and on **nothing else** — not on `epiphany-core`
(Agent B) or `epiphany-ops` (Agent C):

> bundles handle bytes, ops handles semantics. A canonical-base snapshot from the
> bundle's perspective is opaque bytes plus a frontier DVV; only `epiphany-ops`
> interprets it. — QUICKSTART

A bundle is a single file: a fixed 64-byte header at offset 0, two 256-byte
superblock slots, then a body of immutable, content-addressed chunks. The
superblocks are the only mutable on-disk objects. A commit appends new chunks,
writes a new manifest chunk, then flips the active superblock by writing the
*inactive* slot and durably flushing it — that flush is the commit point. Because
commits only ever **append** and touch the **inactive** slot, a crash can never
corrupt the active state.

## What's here

| Area | Items | Spec |
|------|-------|------|
| Prelude | `FixedHeader` (64 B, CRC-32C), `Superblock`/`CommitState` (256 B, CRC-32C), `select_active` | Ch. 8 §"The Bundle Layout", §"Superblock Selection" |
| Atomic commit | `Bundle::create`/`open`/`commit`, the 7-step protocol, cold-open path | Ch. 8 §"The Atomic Write Protocol", §"Streaming Reads" |
| Content addressing | `chunk_content_hash`/`chunk_id`, `ChunkRef`, `ChunkKind`, `CompressionAlgorithm`, domain separation | Ch. 8 §"Content Hashing", §"Chunks" |
| Manifest | `Manifest` (canonical_base ≠ acceleration_snapshots), `SnapshotRef`, `BlobRef`, `ProfileDeclaration`, `ExtensionDeclaration` | Ch. 8 §"The Manifest" |
| Retention | `RetentionPolicy` (first-class), `ProfileConstraints` | Ch. 8 §"Garbage Collection and Retention" |
| Op blocks | `pack_operation_blocks` (1 MiB soft target), `encode_block`/`decode_block` | Ch. 8 §"Operation Envelope Blocks" |
| Storage | `BlockStore`, `MemStore`, `FileStore` (real `fsync`), `FaultStore` (crash sim) | Ch. 8 §"Durable Writes" |
| Gates | `fuzz::run_crash_recovery_fuzz`, `fuzz::exhaustive_crash_check`, `fuzz::run_manifest_selection_harness` | QUICKSTART acceptance |

## The crash-recovery contract (the acceptance gate)

> Kill the process between any two syscalls in the commit protocol; reopen; the
> bundle must be valid in 100% of runs, and must recover to the previous
> generation when the crash precedes the durable flush. This is the most
> important single test in the entire prototype. — QUICKSTART, Agent D

Killing a real process between syscalls cannot be made deterministic, so the
fuzzer drives the commit against a `FaultStore` that distinguishes **live**
(page-cache) bytes from **durable** (survives-a-crash) bytes and can crash after
any chosen syscall — optionally *tearing* the in-flight superblock write, the
case the slot CRC must catch. After every simulated crash the bundle is reopened
from the durable image and must:

1. open successfully (never corrupt);
2. be at the previous generation **or** the new one, never anything else;
3. if the commit returned `Ok`, be at the new generation; and if the crash was
   *clean* (the in-flight flush persisted nothing) and the commit did not
   complete, be at the previous generation — the exact "recover to the previous
   generation when the crash precedes the durable flush" property. (A torn final
   flush may at a full prefix legitimately persist the whole superblock — the
   genuine post-commit case — so the torn branch admits either generation.)
4. report no integrity anomaly;
5. have every canonical chunk present and hash-intact.

Two drivers exercise this: a randomized 10,000-iteration sweep, and an
*exhaustive* per-commit sweep that tests **every** syscall boundary crossed with
**every** tear point (clean, and torn at prefixes around the 252-byte CRC offset
and the 256-byte slot size). The second leaves no step of the protocol untested.

The companion `manifest_selection` gate asserts the Chapter 8 superblock-
selection rule across every corruption scenario the QUICKSTART enumerates: slot A
corrupt + B valid (and vice versa), both valid at generation+1, both valid at the
same generation (equivalent, and divergent), a generation gap > 1, a
non-committed slot, a manifest-hash mismatch, and neither valid.

## Building and testing

```sh
cargo test -p epiphany-bundle                              # unit + the two gates
cargo clippy -p epiphany-bundle --all-targets -- -D warnings
cargo run --release --example fuzz_crash -- 1000000        # extended crash soak
```

## Hand-off criteria (QUICKSTART, Agent D)

- [x] `cargo test` clean.
- [x] Crash-recovery fuzzer passes 10,000 iterations
      (`crash_recovery_fuzz_ten_thousand_iterations`, two seeds; extended soak
      via the example binary; exhaustive per-syscall sweep in
      `exhaustive_sweep_across_base_states_and_commit_shapes`).
- [x] Manifest-selection harness handles every corruption scenario
      (`every_selection_scenario_holds`).
- [x] Real-filesystem `fsync` round-trip (`file_store_real_fsync_round_trip`).

## Scope boundaries (per QUICKSTART "Don't do these")

v0 writes only uncompressed chunks (compression on the *write* path is deferred),
but *reading* zstd-compressed chunks and blobs is supported, per the spec's
§Compression MUST (the manifest is mandatory-uncompressed regardless, and a
compressed manifest is rejected). It carries the text-projection *root* but does
not implement the s-expression projection content, and it *preserves* extension
declarations and chunks but does not *evaluate* edit barriers — barrier operands
(`OperationKindTag`, `ObjectKind`, `EditBarrier`) are owned by Agents C and E.
Operation envelopes, snapshots, and causal frontiers are opaque bytes here.

See `DECISIONS.md` for the prototype byte-layout choices that anticipate the
deferred Binary Format companion, and the batched Pass 11 candidates.
