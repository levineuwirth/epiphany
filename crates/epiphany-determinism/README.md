# epiphany-determinism

The reproducibility-contract primitives for Epiphany, implementing the
normative requirements of **Appendix D (Determinism Contract)** of the core
specification (`spec/core_spec.pdf`). This is Agent A's crate per
`spec/QUICKSTART.md`: the smallest scope, landing first, with every other
crate depending on it.

> Canonical document state must be independent of platform, CPU, locale, thread
> scheduling, hash-map iteration order, floating-point environment, compression
> settings, and wall-clock timing. — Appendix D, Thesis

Everything in this crate exists to make that hold *by construction*. It is pure
value types and pure functions: **no async, no I/O, no platform calls.** It is
the strict single-threaded baseline that every parallel or accelerated
implementation must reproduce bit-for-bit.

## What's here

| Area | Items | Spec |
|------|-------|------|
| Spatial grid | `QuantizedCoord` (`1/1024` staff space, round-ties-to-even) | App. D §"Quantized Layout Coordinates"; Ch. 7 §7.2 |
| Content addressing | `ContentHash`, `ChunkId`, `blake3_256`, `trunc64`, `trunc128`, `Preimage`, `derive_system_counter` | Ch. 8 §"Content Hashing"; Ch. 5–6 derivations |
| Domain tags | `DomainTag` (closed vocabulary) + `MUSC*` constants, `SystemDomainTag`, bundle/superblock magic | Ch. 8 §"Domain-Separated Preimages"; Ch. 5 §"System-Derived" |
| Tolerances | `Tolerance`, `ToleranceClass` (the five classes), `ToleranceGovernance` | App. D §"Tolerance Classes" |
| Float hygiene | `CanonicalF64`, `canonical_f64_bytes`, `canonicalize_zero`, `debug_assert_canonical` | App. D §"Floating-Point Values in Canonical State" |
| Canonical order | `CanonicalEncode`/`CanonicalDecode`, `sort_canonical`, `CanonicalMap`/`CanonicalSet` | App. D §"Ordered Iteration over Sets and Maps" |
| Fuzz harness | `fuzz::run_round_trip_fuzz`, `examples/fuzz_roundtrip` | hand-off gate |

## The three determinism rules this crate enforces mechanically

1. **No NaN/inf/`-0.0` in canonical state.** `CanonicalF64` is the only way to
   put a float into canonical form; it rejects NaN/inf and maps `-0.0` to
   `+0.0`. Canonical equality is byte equality of the little-endian
   serialization. Decode rejects non-finite payloads as corruption.
2. **All hashing is domain-separated BLAKE3-256.** `Preimage` puts the 8-byte
   `DomainTag` first, every time. `DomainTag` is a closed vocabulary (built-ins
   plus `MUSCS` extension tags, printable ASCII) with a private field, so a
   foreign or non-ASCII tag can't be minted; `derive_system_counter` takes a
   `SystemDomainTag`, so only a system tag can seed a system identifier.
   `trunc64`/`trunc128` take the *leading* bytes **big-endian**, matching the
   spec's reference code exactly.
3. **Canonical iteration is a specified total order.** Reach for
   `CanonicalMap`/`CanonicalSet` (BTree) instead of `HashMap`/`HashSet`, or
   `sort_canonical` (gated on `CanonicalByteOrder`) before it affects canonical
   output.

## Implementation decisions

Per QUICKSTART "Decisions you'll need to make", the calls that touch this crate:

- **Sync only** (decision 4). No async traits anywhere; nothing here needs them.
- **`blake3` is the sole dependency.** One content-hash algorithm for this
  format version (Ch. 8); no second hash, no RNG dependency (the fuzz harness
  uses a vendored SplitMix64 so failures reproduce from a seed).
- **MSRV 1.77** for `f64::round_ties_even`. The spec uses no exotic Rust
  features; `overflow-checks` stay on in release so identifier/coordinate
  arithmetic faults loudly rather than wrapping.
- **`unsafe` is forbidden** crate-wide (`#![forbid(unsafe_code)]`).

## Building and testing

```sh
cargo test -p epiphany-determinism          # unit + integration + 1M fuzz gate
cargo clippy --all-targets -- -D warnings    # lint clean
cargo run --release --example fuzz_roundtrip -- 10000000   # extended soak
```

## Hand-off criteria (QUICKSTART, Agent A)

- [x] `cargo test` clean.
- [x] Round-trip canonical encode/decode fuzz harness runs 1M iterations
      without panic (`fuzz_round_trip_one_million_iterations`; extended soak via
      the example binary).

## Scope boundaries

`ChunkKind`, `SchemaVersion`, `ChunkRef`, and the full chunk `hash_preimage`
live in `epiphany-bundle` (Agent D); the typed graph identifiers and the full
`derive_conflict_id` live in `epiphany-core`/`epiphany-ops` (Agents B/C). This
crate provides only the shared primitives those derivations compose from
(`DomainTag`, `Preimage`, `trunc64`/`trunc128`, canonical float bytes), so
there is no dependency cycle and exactly one definition of each contract type.

Ambiguities encountered while building are *not* resolved in code — they are
batched as Pass 11 candidates against the spec (QUICKSTART, Process notes). None
were required for this crate.
