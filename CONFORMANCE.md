# Determinism Conformance Statement

This is the published conformance statement required by the core
specification's Determinism Contract (`core_spec` Appendix D,
§"Conformance Statement", `sec:det:conformance`). It covers the reference
implementation in this repository — the `epiphany-*` workspace crates — as of
the tree that carries this file. Every claim below is anchored by a test named
in the Canonical Byte-Layout Reference (Appendix E) or cited inline.

## Platform and floating-point library

The reference build and CI platform is Linux on x86-64, compiled with stable
Rust (MSRV pinned in `Cargo.toml`, `rust-version = "1.77"`), default target
options — no `fast-math`-class flags anywhere in the workspace.

Canonical state contains no *computed* floating-point values. Every float that
enters canonical state is a stored `CanonicalF64` (finite by construction,
`-0.0` canonicalized) serialized as its 8 little-endian IEEE 754 bytes
(`epiphany-determinism/src/float.rs`); no canonical algorithm derives new
float values into canonical state.

**Transcendental functions.** The only transcendentals in the workspace are
`f64::ln`/`f64::exp` from the platform's `std`/libm, used exclusively by tempo
integration (`epiphany-core/src/tempo.rs`, speed-linear and exponential
segments). Per the contract's required disposition, those conversion outputs
are declared **advisory and non-canonical**: musical time is the exact
rational, wall-clock time is exact integer nanoseconds, and no tempo-derived
float is stored in canonical chunks or hashed into canonical identity. If a
future feature promotes tempo-derived values into canonical state, adopting a
documented portable math library (or quantizing at the canonical boundary) is
a precondition, not an afterthought.

Content hashing is BLAKE3 (the `blake3` crate, workspace-pinned), which is
bit-exact by specification on all platforms.

## Parallel execution strategy

None. Every canonical algorithm — reduction, materialization, pre-passes,
encoding, hashing, layout solving — is single-threaded by construction; the
workspace contains no `rayon`, `std::thread::spawn`, or other concurrency in
any canonical path. The implementation *is* the single-threaded baseline, so
equivalence to that baseline holds by identity. Any future parallel execution
must demonstrate byte-identical output against this baseline before it ships.

## `-0.0` canonicalization and NaN/infinity rejection

`CanonicalF64::new` rejects NaN and ±infinity at runtime in **all build
profiles** (not just debug assertions) and canonicalizes `-0.0` to `+0.0`
before storage, so both are unrepresentable in canonical state. Decoding
treats non-finite float bytes as corruption (typed error, never silent
acceptance), and hash preimages accept only `CanonicalF64` values. Locked by
the unit tests in `epiphany-determinism/src/float.rs` and
`src/serialize.rs`.

## Rounding

Round-to-nearest-ties-to-even is used at every canonical quantization
boundary: `QuantizedCoord` (1/1024 staff space) quantizes via
`round_ties_even` and rejects NaN/infinity/overflow rather than saturating
(`epiphany-determinism/src/coord.rs`), and `ResolvedLayoutIR` quantizes its
f32 working coordinates through the same type at canonical emission.

Declared deviation, inside the advisory surface only: the non-canonical
wall-clock conversion in `tempo.rs` rounds nanoseconds with `f64::round`
(half-away-from-zero). It shares the tempo-integration surface declared
non-canonical above; it must be converted to ties-to-even if that surface is
ever promoted.

## Canonical iteration orders

All canonically-serialized collections iterate in the contract's orders
(§`sec:det:ordering`):

- Generic containers are `BTreeMap`/`BTreeSet` or explicitly sorted through
  `CanonicalMap`/`CanonicalSet`/`sort_canonical`, whose element types must
  implement the `CanonicalByteOrder` marker — a type whose `Ord` does not
  match its canonical byte order is rejected at compile time
  (`epiphany-determinism/src/order.rs`).
- Operation envelopes reduce in the canonical order: causal (Kahn topological
  over DVV coverage), then the HLC tuple
  (`epiphany-ops/src/reduce.rs::canonical_reduction_order`), property-tested
  for permutation invariance at 1,000 envelopes × 10 orders plus a
  10,000-set fuzz gate.
- Conflicts serialize ascending by `ConflictId`; integrity anomalies ascending
  by `IntegrityAnomalyId`; chunk references by `(kind, hash, offset)`;
  extension declarations by `(ExtensionId, SemVer)` as numeric tuples —
  each rejected (not normalized) on decode when out of order.

`HashMap`/`HashSet` appear only in non-canonical lookup indexes, caches, and
diagnostics, or are projected through a sort before any canonical output.

## NFC normalization

Unicode normalization uses the `unicode-normalization` crate
(workspace-pinned, `0.1.x`). Catalog identifiers NFC-normalize at
construction (`epiphany-core/src/pitch.rs`); envelope string fields
NFC-normalize before hashing (`epiphany-ops/src/encode.rs`, test
`nfc_normalizes_before_hashing`); system-derived pitch identity NFC-normalizes
at the derivation boundary. Free-text fields are raw UTF-8 by the ratified
codec convention (`req:format:codec-conventions`) and are not folded.

## Declared open-question algorithms

Per §"Open Algorithm Hooks", every open-question area is either
profile-declared with a versioned identifier or errors rather than
substituting a vendor heuristic:

- **Spelling**: `SpellingAlgorithmId` `"default"` — a Temperley-style
  line-of-fifths preference algorithm, v1 (`epiphany-core/src/prepass.rs`).
  The identifier is the crate's proposal pending ratification (P12-H1). A
  profile requesting any other id errors; nothing is silently substituted.
- **Notational decomposition**: `DecompositionAlgorithmId` `"default"` — the
  integer-grid metric splitter, v1, with its scope bounds recorded as
  P12-H4 (single governing meter, `MAX_DOTS = 1`, tuplet-nesting deferred).
  Same no-substitution rule.
- Both pre-pass outputs are **canonical derived annotations**: deterministic
  functions of `(materialized graph, profile, algorithm id)` recomputed on
  materialization, never stored canonical state — so an algorithm version
  change deterministically invalidates derived output without state
  migration.
- **Tempo curves**: `TempoShape::Curve` integration is unimplemented and
  declared as such — conversion returns `CurveIntegrationUnsupported`
  rather than a wrong or vendor-specific answer. The linear/exponential
  segment integration and the `wallclock_to_musical` inverse (deterministic
  continued-fraction with documented iteration/denominator bounds and a
  typed `TempoIntegration`-class tolerance) are advisory, per the
  floating-point declaration above.
