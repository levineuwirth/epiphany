# epiphany-ops

The Epiphany **concurrent semantics**: the operations through which the score
graph becomes a *live* model, and the deterministic reduction by which a set of
operations becomes a materialized score state. Implements the normative
requirements of **Chapter 6 (Semantic Operations and Concurrent Reduction)** of
the core specification (`spec/core_spec.pdf`). This is Agent C's crate per
`spec/QUICKSTART.md`, building on Agent A's `epiphany-determinism` and Agent B's
`epiphany-core`.

> A score's state is defined by the set of operations committed to it. Any
> materialized graph is a deterministic reduction of that set; caches,
> snapshots, and partial reductions are acceleration structures, never the
> source of truth.
> — Chapter 6, Design Principles

## The thesis in one paragraph

The replicated operation set is a grow-only **CRDT**: replicas accumulate
[envelopes](src/envelope.rs) and converge on the same set. The materialized
graph is **not** a CRDT — it is the deterministic reduction of that set in a
single **canonical order** (causal-first, then the HLC tuple). Any permutation
of the same envelopes reduces to byte-identical materialized state. That
property is the determinism heart of the architecture, and the
[reduction fuzzer](src/fuzz.rs) is its tripwire.

## What's here

| Area | Items | Spec |
|------|-------|------|
| Stamps | `HybridLogicalClock`, `OperationStamp`, the reduction & monotonicity tuples | Ch. 6 §"Operation Identity and Stamps" |
| Causal context | `CausalContext` (dotted version vector), `covers`, the missing-predecessor signal | Ch. 6 §6.2 |
| Payloads | `OperationKind`, the discriminator-only `OperationKindTag`, `OperationPayload`, the §6.10 representative ops | Ch. 6 §"Operation Envelopes", §6.10 |
| Envelopes | `OperationEnvelope`, `EnvelopeHash` (`MUSCENVH`), `well_formed` (incl. `stamp.id == id`) | Ch. 6 §6.4 |
| Slots | `OperationSlot::{Single, Equivocated}`, the order-independent (Pass-10) transitions | Ch. 6 §6.5 |
| Anomalies | `AnomalousReplicaSegment`, `IntegrityAnomaly`/`Kind`, the HLC-monotonicity detector | Ch. 6 §6.6; Ch. 5 §"System-Derived Counter Collisions" |
| Effects | `OperationEffect`, `NoOpReason`, the typed `PreconditionFailureReason`, `RepairRecord`/`RepairKind` | Ch. 6 §6.3.2, §6.5 |
| Conflicts | `ConflictRecord`, `ConflictKind`, content-derived `ConflictId` (`derive_conflict_id`), the registry, resolution | Ch. 6 §6.4 |
| Transactions / undo | `TransactionDescriptor` with the causal-prior-descriptor rule, `UndoTransactionPayload` / `UndoPolicy` | Ch. 6 §6.6, §6.8 |
| Operation set | `OperationSet`: accept pipeline (well-formedness → slot → causal), grow-only | Ch. 6 §"Envelope Acceptance" |
| Reduction | `canonical_reduction_order` (single function), `MaterializedState`, the reduction driver | Ch. 6 §6.3 |

## The determinism this crate enforces

1. **A single reduction-order function.** `canonical_reduction_order` sorts by
   the intrinsic stamp tuple `(physical, logical, replica, counter)`. The
   authoring HLC rule guarantees a causal predecessor's tuple is strictly less,
   so the sort respects causal order without a topological pass — and, being a
   sort by intrinsic keys, it is trivially independent of arrival order.
2. **Order-independent equivocation.** A duplicate `OperationId` with different
   canonical bytes transitions its slot to `Equivocated` regardless of which
   envelope arrived first (Pass 10). Equivocated slots contribute nothing to
   reduction; dependents are held pending.
3. **Content-derived facts.** `ConflictId` and `IntegrityAnomalyId` are derived
   from content, so two replicas reducing the same set agree on every conflict
   and anomaly id — the conflict registry and anomaly register are deterministic
   materialized facts, not local bookkeeping.
4. **Byte-identical materialized state.** `MaterializedState::canonical_bytes`
   serializes the effect log, conflict registry, anomaly register, object
   existence, spellings, and LWW fields in their normative orders.

## Hand-off gates

Run the gate harnesses (QUICKSTART, Agent C):

```
cargo test -p epiphany-ops
cargo run --release -p epiphany-ops --example fuzz_reduction          # 10k iters, seed 0
cargo run --release -p epiphany-ops --example fuzz_reduction 100000 7 # soak, seed 7
```

- **Reduction determinism** — every randomized envelope set reduces to
  byte-identical materialized state under any acceptance order (v0 acceptance
  criteria 1 and 5).
- **Equivocation order-independence** — every duplicate-id-with-different-bytes
  scenario equivocates regardless of arrival order (v0 acceptance criterion 3).

The integration tests (`tests/concurrent_reduction.rs`) exercise these plus
transaction atomicity, descriptor precedence, anomaly exclusion, and forward
undo through the public API.

## Scope and decisions

Chapter 6 specifies the framework and a *representative* selection of
operations; the full ~60–80-primitive catalog is an explicit open question
(§6.11) deferred to the Operation Catalog companion. This crate implements the
framework in full and the representative operations, which is sufficient to
exercise every reduction discipline. The full musical-graph mutation against
`epiphany_core::Score` is the next phase. See `DECISIONS.md` for the scope
boundary, the prototype conventions (payloads carry identifiers + fingerprints,
voice promotion via an order-independent pre-pass, undo via minted-object
compensation), and the batched Pass 11 candidates.

Per QUICKSTART "Don't do these": undo is the spec's **forward** compensating
operation, never inverse-based; `unsafe` is forbidden; everything is sync.
