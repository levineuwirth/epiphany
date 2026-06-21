# epiphany-testkit

Agent F's crate per [`spec/QUICKSTART.md`](../../spec/QUICKSTART.md): the
cross-cutting conformance testkit. It is the architecture's tripwire — the suite
that proves the other crates work end to end and that runs in CI (see
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)).

It provides:

- **Deterministic property-test generators** for the public types of A
  (`epiphany-determinism`), B (`epiphany-core`), C (`epiphany-ops`), D
  (`epiphany-bundle`), and E (`epiphany-layout-ir`). Agent B's score-graph
  generators/shrinkers are re-exported as `generators::graph`.
- **The canonical round-trip harness** (`roundtrip`) — v0 acceptance criterion 4
  (typed values + bundle container; the bookkeeping `MaterializedState` round-trip
  is retained as `assert_reduction_serialization_stable`).
- **The CRDT convergence harness** (`convergence`) — criteria 1 and 5. Criterion
  1 proper is **real-Score** convergence through `reduce_onto`
  (`run_graph_convergence`); the byte-canonical bookkeeping-projection
  convergence (`assert_convergence`) backs criterion 5.
- **The equivocation harness** (`equivocation`) — criterion 3.
- **The crash-recovery harness** (`bundle_harness`) — Agent D's gate, criterion 2.
- **The manifest-selection harness** (`bundle_harness`).
- **The layout round-trip harness** (`layout_stub`) — criterion 6.
- **The audit regression guards** (`negative`) — one guard per defect the Agent C
  framework audit surfaced (the M1 fixes), so a regression trips this suite
  directly.

## All harnesses are real

The QUICKSTART charters Agent F to *"build against A and stubs for the others."*
All five implementation crates — A, B, C, D, and now E (`epiphany-layout-ir`) —
have shipped, so every harness drives the **real** crate.

| Harness | Backend | Status |
|---------|---------|--------|
| `roundtrip` (criterion 4) | A + B + C + D, real | **real** |
| `bundle_harness` (criterion 2, manifest selection) | D, real | **real** |
| `convergence` (criteria 1, 5) | C (`epiphany-ops`), real | **real** |
| `equivocation` (criterion 3) | C (`epiphany-ops`), real | **real** |
| `layout_stub` (criterion 6) | E (`epiphany-layout-ir`), real | **real** |

For criteria 1, 3, and 5 the testkit drives the real
`epiphany_ops::OperationSet` / `canonical_reduction_order` / reduce and also
re-exports Agent C's own authoritative gates
(`convergence::ops_reduction_determinism_fuzz`,
`equivocation::ops_equivocation_fuzz`). The `layout_stub` module — once a
faithful in-tree stub of Chapters 7 & 9 — now re-exports the real
`epiphany-layout-ir` IR types and stub solver behind the same `round_trip`
signature; the provenance-preservation contract is implemented and tested inside
that crate. (The "stub" in the module name now refers to the spec-sanctioned
*stub constraint solver*, not to a stubbed crate.)

## Criterion 1: real-Score vs. reducer-bookkeeping convergence

Criterion 1 proper (`convergence::run_graph_convergence`, the acceptance
`criterion_1_convergence` test) is **real-Score** convergence: a real ~50-bar,
two-voice base `epiphany_core::Score` is edited by two replicas through
`OperationSet::reduce_onto`, and the entire materialized graph — arena, voices,
tombstones, cross-cutting, *and* the bookkeeping state — must be **identical**
under every delivery order, pass `check_invariants`, and genuinely grow both
edited voices (non-vacuity). The session targets the base's actual voice ids
(`generators::graph_edit_session`), so it exercises the integration point, not a
synthetic id space.

The earlier, narrower gate is retained and honestly renamed
(`reducer_bookkeeping_convergence`): it converges the byte-canonical
**bookkeeping projection** (`OperationSet::reduce` →
`MaterializedState::canonical_bytes`) — the Chapter 6 §6.3 ledger (effects,
conflicts, anomalies, tombstones, spellings, pending), not the full musical
graph. It still backs criterion 5 and proves causal-first ordering
(`convergence::assert_causal_order_respected`,
`run_authoritative_reduction_gate`). The bookkeeping two-staff scenario remains
*instantiated* — a real ~50-bar (`TWO_STAFF_BARS`) session whose staves are
asserted populated by `generators::assert_two_staff_populated`, not just modeled.

## Criterion 4: what is and isn't tested

Criterion 4 has three tiers — two asserted now, one pending item 5:

- **Real decode round-trips** (these catch decoder / canonicalization defects):
  the generic `CanonicalEncode`/`CanonicalDecode` property swept across every
  typed identifier, both `RationalTime` arms, and every `TypedObjectId`
  discriminant; the bundle `Manifest` (`encode → decode → encode` fixpoint, with
  a *rich* generator exercising snapshots, blobs, extensions, varied profiles,
  retention, and the optional roots); the `FixedHeader`; and the `Superblock`
  slot encoding. Crucially, the **decoders are shown to validate**: corrupting a
  manifest or header makes `decode` *reject* it
  (`assert_manifest_decode_rejects_corruption`,
  `assert_header_decode_rejects_corruption`).

- **A reducer-bookkeeping serialization tier** (`reducer_bookkeeping_serialization`,
  via `assert_reduction_serialization_stable`): a real `OperationSet` is reduced
  to its `MaterializedState::canonical_bytes()` — the canonical *bookkeeping*
  state, **not** the whole musical `Score` — which is stored as a `Snapshot`
  chunk referenced by the manifest's `canonical_base`, survives the bundle's
  content-addressed store (hash-verified on reopen), decodes through
  `MaterializedState::decode_canonical`, compares structurally with the original
  reduction, and re-serializes byte-identically. The decoder validates nested
  tags, lengths, primitive values, canonical form, and trailing bytes. Musical
  sensitivity is proven two ways:
  `assert_content_mutation_changes_serialization` (a cloned operation set with
  **identical** ids/stamps/causal contexts but one changed payload reduces to
  *different* bytes — the rebuttal to an id-only serializer) and
  `assert_distinct_scores_serialize_differently`. The materialized real `Score`
  itself is shown reproducible today (`full_score_materialization_is_reproducible`,
  structural equality across delivery orders) — the determinism precondition a
  byte codec depends on.

- **The full-`Score` byte round-trip is pending item 5 (Agent B).** No
  whole-score canonical codec (`CanonicalEncode`/`CanonicalDecode for Score`)
  exists yet, so `criterion_4_full_score_byte_roundtrip` is marked `#[ignore]`
  (visible as *ignored*, never falsely green) rather than asserted on the
  bookkeeping projection and passed off as a whole-Score gate. When item 5 lands
  the codec, drop the attribute and assert the real byte cycle through a bundle
  snapshot.

## Decisions (per QUICKSTART "Make each one once and document it")

1. **No platform entropy in the harness.** Appendix D §"Randomness" forbids
   platform entropy in canonical state; the testkit holds itself to the stronger
   rule that *no* platform entropy enters the harness at all. Everything draws
   from `rng::Rng` (a wrapper over Agent A's vendored SplitMix64, with unbiased
   bounded draws and an overflow-safe full-range `range`), so every failure
   reproduces from its seed.
2. **Drive the real crate once it ships; stub only what hasn't landed.** Earlier
   in development `epiphany-ops` (C) and `epiphany-layout-ir` (E) were in-flight
   and their harnesses ran against faithful in-tree stubs; now that both have
   shipped, every harness drives the real crate and re-exports its gates.

## Flagged for a future spec pass (Pass 11 candidates)

Per the QUICKSTART, implementation-discovered gaps are batched, not improvised:

- **Whole-graph (`epiphany_core::Score`) wire format — pending item 5 (Agent B).**
  Criterion 4 is a real decode round-trip at the canonical Chapter-6
  `MaterializedState` layer; the materialized `Score` is shown *reproducible*
  today. A direct canonical byte codec for the richer core `Score` does not exist
  yet (it is item 5's "whole-score codec", to be reconciled with the Binary
  Format companion), so the whole-`Score` byte round-trip
  (`criterion_4_full_score_byte_roundtrip`) is an explicit `#[ignore]`'d gate
  rather than a falsely-green assertion.
- **Layout harness re-pointed.** `epiphany-layout-ir` has landed, so `layout_stub`
  now drives the real IR types behind the same `round_trip` signature (done). IR
  coordinates are f32 staff spaces, quantized only when serializing canonical
  `ResolvedLayoutIR` (Appendix D); see that crate's `DECISIONS.md` for the
  remaining layout-specific Pass 11 candidates (the `OperationKindTag` variant set
  and the layout-object id derivation).

## Running

```sh
# Unit + acceptance tests (a meaningful slice, under the cargo test timeout):
cargo test -p epiphany-testkit

# The full conformance suite at scale, outside the test timeout (includes Agent
# A's 1,000,000-iteration determinism gate and Agent C's reduction/equivocation
# gates):
cargo run --release -p epiphany-testkit --example conformance_suite        # scale 1
cargo run --release -p epiphany-testkit --example conformance_suite 10     # soak
cargo run --release -p epiphany-testkit --example conformance_suite 0      # smoke
```

The six v0 acceptance criteria are asserted in `tests/acceptance.rs`, one test
per architecture layer.
