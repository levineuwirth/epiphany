# Epiphany v0 — Implementation Quickstart

This is the operational reference for the v0 prototype. The full architecture and contract is in `core_spec.pdf` (244 pages). This document is the dispatch layer: who owns what, in what order, with what acceptance criteria.

## Project context

Epiphany is a FOSS music notation platform. The core specification has been through ten architectural review passes and is now structurally stable. Full conformance still depends on companion specifications and four open canonical algorithms (spelling pre-pass, notational decomposition, tempo integration, `wallclock_to_musical` root-finding), but **none of those are prototype blockers**. They have stub paths sanctioned by Appendix D of the spec.

We are building a **prototype baseline**, not the product. No UI, no audio engine, no renderer beyond the layout-IR interface, no plugin runtime. Six Rust crates that together demonstrate the core architecture works end to end.

## Before you start

Every agent must:

1. Read the spec, with particular attention to the chapter that maps to their crate (see Agent Assignments below).
2. Read Appendix D (Determinism Contract) regardless of which crate you own. It applies everywhere.
3. Read the Reading Guide chapter at the front of the spec. The summary tables (identity hierarchy, canonical vs non-canonical, operation lifecycle, determinism layers, solver tiers) are the fastest way to orient.
4. Treat the spec as the contract. If a behavior is in the spec, implement it as specified. If you encounter an ambiguity, **do not improvise** — flag it. We batch ambiguities and address them in spec revisions, not in code.

## Crate topology

```
epiphany/
├── crates/
│   ├── epiphany-determinism/    # tolerance classes, QuantizedCoord, hashing
│   ├── epiphany-core/           # identifiers, primitives, score graph
│   ├── epiphany-ops/            # envelopes, DVV/HLC, canonical reduction
│   ├── epiphany-bundle/         # file format, manifest, chunks
│   ├── epiphany-layout-ir/      # IR types, TimeAxisModel, stubbed solver
│   └── epiphany-testkit/        # property generators, conformance harness
├── tests/
│   └── reference-suite/         # cross-crate scaffolding
└── xtask/                       # build, fuzz, bench orchestration
```

`epiphany-determinism` is small but separate so every other crate can depend on it without circular dependencies. It contains only contract types — no crate-local logic. `epiphany-bundle` deliberately does **not** depend on `epiphany-ops`: bundles handle bytes, ops handles semantics. A canonical-base snapshot from the bundle's perspective is opaque bytes plus a frontier DVV; only `epiphany-ops` interprets it.

## Agent assignments

Six agents, sequenced by dependency. Each handoff is gated on a tagged release of the previous crate plus the testkit's harness passing for that scope.

### Agent A — `epiphany-determinism`

Smallest scope. Must land first.

Owns: `QuantizedCoord` at 1/1024 staff space, `ContentHash` and `ChunkId` newtypes, the `"MUSC*"` domain-tag constants, the five tolerance classes (`AcousticCents`, `LayoutCoordinate`, `QualityMetric`, `TempoIntegration`, `SolverResidual`), canonical-iteration-order helpers (BTreeMap wrappers, sorted-slice utilities), and floating-point hygiene (debug assertions on NaN/inf/-0.0, canonical f64 little-endian serialization).

Output: pure types, no async, no I/O, roughly 600 lines.

Hand off when: `cargo test` clean and the fuzz harness for round-trip canonical encode/decode runs 1M iterations without panic.

Spec sections: Appendix D in full, Chapter 4 §4.6 (frequency units), Chapter 7 §7.2 (spatial primitives).

### Agent B — `epiphany-core`

Owns the score graph: the identifier family (`EventId`, `PitchId`, `VoiceId`, ..., `TypedObjectId`), the `ReplicaId::SYSTEM_DERIVED` reserved value and counter derivation (`trunc64(BLAKE3(domain_tag || canonical_inputs))`), `IdentifiedPitch`, `PitchSpelling`, `ScalePosition`, the duration union (`EventDuration` / `ConcreteDuration`), `MusicalPosition`, `WallClockTime`, `TimeAnchor`, `AnchorOffset`, the event arena, `Voice`, `Staff` and `StaffInstance` (these are distinct types — the spec is explicit), `Region`, `BarlineAlignmentGroup`, and the 19 graph invariants enumerated in Chapter 5.

**Critical**: graph invariants are property tests in CI, not runtime assertions in release builds. The testkit provides arbitrary-instance generators. For each invariant, you must have a positive generator that produces valid graphs and a negative shrinker that minimizes invariant violations to a small witness for debugging.

Hand off when: every invariant has both a generator and a shrinker, and the testkit's `arbitrary-graph` corpus runs clean.

Budget: this is the largest agent's scope. Plan accordingly.

Spec sections: Chapters 2, 3, 5 in full; Chapter 4 for tuning system integration.

### Agent C — `epiphany-ops`

Depends on A and B. Owns concurrent semantics.

Implements: `OperationEnvelope` with the `stamp.id == envelope.id` well-formedness invariant; `OperationSlot::{Single, Equivocated}` (this is order-independent — see Pass 10 in the spec's revision history); `CausalContext` as DVV; HLC stamp with per-replica monotonicity check; `AnomalousReplicaSegment` with the exclusion-from-canonical-reduction rule (envelopes from the first violating counter onward are retained but not reduced); `OperationKind` enum and the `OperationKindTag` discriminator-only variant; envelope-acceptance pipeline (well-formedness → slot transition → causal validation); the four-phase lifecycle (Prepare → Commit → Reduce → Report); `OperationEffect` including `RepairKind`; the typed `PreconditionFailureReason` enum (not free-form String); `ConflictRecord` with content-derived `ConflictId` via `trunc128(BLAKE3("MUSCCONF" || canonical_kind || sorted_ops || sorted_objs))`; `IntegrityAnomaly` kept separate from `ConflictKind` (the distinction matters — conflicts are ordinary canonical-state facts; anomalies are structural failures); transactions with the causal-prior-descriptor rule (every member MUST causally depend on its `DeclareTransaction`); `UndoTransaction` with `StrictInverse` / `BestEffort` / `Cascade` policies.

**Critical**: the canonical reduction order (causal → HLC physical → HLC logical → replica → counter) must be a single function with a property test asserting that any permutation of input envelopes yields **byte-identical materialized state**. This is the determinism heart of the architecture. If it doesn't hold, nothing else matters.

Hand off when: the determinism property holds across 10,000 randomized envelope sets in CI, and the equivocation harness produces `OperationSlot::Equivocated` for any duplicate-id-with-different-bytes scenario regardless of arrival order.

Spec sections: Chapter 6 in full, especially the equivocation rules (§6.5), HLC monotonicity (§6.6), transaction causal ordering (§6.9), and the canonical reduction order (§6.7).

### Agent D — `epiphany-bundle`

Depends on A. Does **not** depend on B or C.

Implements: the 64-byte fixed header (CRC at bytes 60–63), two 256-byte superblock slots, `CommitState` validation in superblock selection (non-`Committed` is invalid for ordinary selection, available only in diagnostic recovery), atomic-commit protocol (write inactive slot → durable flush → generation+1; commit point is durable flush), uncompressed manifest chunk (mandatory in this format version), `Manifest` with `canonical_base: Option<SnapshotRef>` and `acceleration_snapshots: Vec<SnapshotRef>` (these are distinct; do not merge them), `ChunkRef` and BLAKE3-256 hashes with domain separation, operation-envelope blocks at 1 MiB soft target, `RetentionPolicy` as a first-class type, the cold-open path.

**Critical**: the crash-recovery fuzzer is the acceptance gate. Kill the process between any two syscalls in the commit protocol; reopen; the bundle must be valid in 100% of runs, and must recover to the previous generation when the crash precedes the durable flush. This is the most important single test in the entire prototype.

Hand off when: crash-recovery fuzzer passes 10,000 iterations and the manifest selection harness handles every corruption scenario (slot A corrupt + slot B valid, vice versa, both valid same generation, both valid generation+1, neither valid).

Spec sections: Chapter 8 in full.

### Agent E — `epiphany-layout-ir`

Depends on A and B. Lands last among the implementation crates.

Implements: `LogicalLayoutIR`, `ConstrainedLayoutIR`, `ResolvedLayoutIR`, `RenderIR` (interface only — no actual rendering), `TimeAxisModel` as a tagged enum over `Metric` / `Proportional` / `Aleatoric` / `Registered` (not `Box<dyn TimeAxis>`), `GlyphCatalogIdentity` with Bravura metrics bundled in-tree for testing, the provenance back-references (this is what makes incremental layout possible later), engraving-decision records, the vertical-band model, edit-barrier types with `OperationKindTag`-based `prohibited_operation_kinds`.

**Stub the constraint solver**. The stub returns `SolveStatus::Solved` with the input geometry verbatim. The real solver comes later; v0 just needs to round-trip IR through the solver interface to prove the contracts hold.

Hand off when: a 10-measure single-staff score round-trips graph → LogicalLayoutIR → ConstrainedLayoutIR → stub-solved ResolvedLayoutIR → RenderIR interface call → back to graph identity with all provenance preserved.

Spec sections: Chapter 7 in full; Chapter 9 for the solver interface (but only the interface — don't implement quality metrics).

### Agent F — `epiphany-testkit`

Cross-cutting. Starts in week 3, builds against A and stubs for the others.

Builds: property-test generators for every public type in A through E; the canonical round-trip harness (serialize → bytes → deserialize → assert byte-identical re-serialization); the CRDT convergence harness (apply same envelope set in N random orders, assert byte-identical materialized state); the equivocation harness; the crash-recovery harness (Agent D's gate); the manifest selection harness.

This is the agent whose work prevents you from finding regressions in weeks 12+. Run their suite in CI from the day Agent A lands.

Don't let any other agent merge work without F's harness for their scope passing. The harness is the architecture's tripwire.

## Sequencing

```
Week 1–2     Agent A lands.
Week 2–6     Agents B and D in parallel (no dependency between them).
             Agent F starts week 3, builds against A and stubs for B/D.
Week 6–12    Agent C builds on B. F adds C-specific harnesses.
Week 10–14   Agent E builds on B with stub solver.
Week 14+     Integration: round-trip a real score through all five crates.
```

This is calendar pacing assuming roughly one engineer per agent. Scale up by parallelizing within each crate (each crate has natural module boundaries), not by skipping the dependency order.

## Decisions you'll need to make

Five small but non-obvious calls. The spec doesn't fix these because they're implementation choices, not architecture. Make each one once and document it in the repo.

1. **Replica ID entropy source.** Spec requires ≥64 bits of CSPRNG entropy. Recommendation: the `getrandom` crate. Document the source in `epiphany-core`'s README.
2. **Storage backend for the event arena.** Options: `slab::Slab`, `slotmap::SlotMap`, or hand-rolled. Recommendation: slotmap, because it gives you generation-checking for stale-handle detection for free, which matches the spec's identifier-stability requirement.
3. **Chunk store backend for v0.** Recommendation: in-memory `BTreeMap<ChunkId, Bytes>` for v0. Defer mmap'd file backend until Agent D's crash fuzzer is green.
4. **Async or sync?** Recommendation: keep `epiphany-determinism`, `epiphany-core`, and `epiphany-ops` fully sync. Make `epiphany-bundle` sync with a thin async wrapper crate (separate, optional) later. Don't poison the type system with async traits this early.
5. **MSRV.** Pin to a recent stable Rust. The spec uses no exotic Rust features; current stable is fine.

## Don't do these

Each of these is explicitly out of scope for v0. Stub them as noted; the spec sanctions stubs behind versioned interfaces.

- **Don't implement the spelling pre-pass.** Stub: `fn spell(p: AcousticPitch, ctx: &SpellingContext) -> PitchSpelling` returning a trivial default. Register as `SpellingAlgorithmId::Default`.
- **Don't implement notational decomposition.** Same pattern — stub returns a single notehead matching the event's sounding duration in the most basic possible way.
- **Don't implement tempo curve integration.** Stub `wallclock_to_musical` with linear segments only. For curve segments, return `SolveStatus::PartialBudgetExhausted` with a diagnostic noting that curve integration is not yet implemented.
- **Don't implement compression in the bundle.** The spec permits it (except for the manifest, which is mandatory uncompressed in v0); skip it for v0 entirely. Add zstd later as a non-breaking minor version.
- **Don't implement extension registries.** Hardcode the core registry-id constants used by the spec's own examples; load no external registries.
- **Don't implement the plugin runtime, the UI, the audio engine, or the renderer.** All four are explicitly out of core scope per Chapter 1 of the spec.
- **Don't implement undo as inverse-based.** The spec is explicit: undo is a forward compensating `UndoTransaction` operation. It feels heavier than the obvious inverse-op approach, but the obvious approach breaks under concurrent edits. Implement the spec's path.

## v0 acceptance criteria

You'll know v0 is done when these six tests pass from a Rust test runner. Each one corresponds to a layer of the architecture; if any fails, that layer is not done.

1. **Convergence.** Two replicas authoring overlapping edits to the same 50-bar two-staff score converge to byte-identical materialized state regardless of envelope delivery order. (Tests Chapter 6's canonical reduction.)
2. **Crash safety.** A `kill -9` between any two syscalls in the commit path leaves the bundle openable — possibly at the previous generation, never corrupt. (Tests Chapter 8's atomic commit.)
3. **Equivocation.** An injected duplicate `OperationId` with different canonical bytes produces an `OperationSlot::Equivocated` at both replicas, regardless of which envelope arrived first. (Tests Pass 10's order-independence fix.)
4. **Canonical serialization stability.** The same score serialized → loaded → re-serialized produces byte-identical bytes. (Tests Appendix D's canonical-serialization layer.)
5. **Reduction determinism.** A randomized 1,000-envelope set, reduced 10 times in 10 different orders, produces byte-identical materialized states. (Tests Appendix D's canonical-reduction layer.)
6. **Layout round-trip.** A score graph → LogicalLayoutIR → stub-solved ResolvedLayoutIR → RenderIR interface call completes without panic and without losing provenance back-references. (Tests Chapter 7's IR contract.)

If all six pass on the testkit's CI run, you have an implementation baseline. From there, every further spec revision should be triggered by a failing test or an impossible API in this codebase — not by abstract review.

## Process notes

**The spec is the contract.** Keep `core_spec.pdf` on the build server next to the test binaries. When an agent has a question about behavior, the answer is in the document. If the document doesn't answer it, that's a real bug in the spec, not a license to improvise.

**Ambiguities go into a batch, not into code.** Every implementation-discovered ambiguity is a Pass 11 candidate. Open a tracking issue against the spec, not against the code. Don't open Pass 11 until you have at least three such items batched — otherwise the spec churns under the implementation and the agents lose their footing.

**Architecture is frozen.** Further revisions are driven by failing tests, impossible APIs, or genuine ambiguity encountered while building. They are not driven by abstract review. If someone wants to revisit a Pass-1-through-Pass-10 decision, ask first: "do you have a failing test or a stuck implementation that motivates this?" If not, defer.

**Treat the testkit as a first-class deliverable.** It is not infrastructure overhead. It is the artifact that proves the architecture works. Agent F's work is on the critical path even though their crate doesn't ship in v0.
