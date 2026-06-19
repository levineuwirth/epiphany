#![forbid(unsafe_code)]
//! # epiphany-testkit
//!
//! The Epiphany conformance testkit: the cross-cutting harness that proves the
//! architecture works end to end. This is Agent F's crate per
//! `spec/QUICKSTART.md`:
//!
//! > Builds: property-test generators for every public type in A through E; the
//! > canonical round-trip harness; the CRDT convergence harness; the
//! > equivocation harness; the crash-recovery harness (Agent D's gate); the
//! > manifest selection harness. … This is the agent whose work prevents you
//! > from finding regressions in weeks 12+. … The harness is the architecture's
//! > tripwire.
//!
//! ## What is real and what is stubbed
//!
//! The QUICKSTART charters Agent F to *"build against A and stubs for the
//! others."* At the time of writing, Agents A ([`epiphany_determinism`]),
//! B ([`epiphany_core`]), C ([`epiphany_ops`]), and D ([`epiphany_bundle`]) have
//! shipped, so the harnesses that depend on them are **real**:
//!
//! * [`roundtrip`] — the canonical round-trip harness, driving every
//!   `CanonicalEncode`/`CanonicalDecode` type in A and B, the real
//!   [`epiphany_bundle::Manifest`]/header/superblock encode/decode, and a real
//!   [`epiphany_ops::MaterializedState`] serialize→load→deserialize→re-serialize
//!   cycle through the bundle (v0 acceptance criterion 4).
//! * [`bundle_harness`] — the crash-recovery gate (criterion 2) and the
//!   manifest-selection harness, driving the real [`epiphany_bundle`] through
//!   its public API and re-exporting its in-crate gates.
//! * [`convergence`] — the CRDT convergence and reduction-determinism harnesses
//!   (criteria 1 and 5), driving the real [`epiphany_ops::OperationSet`] /
//!   [`epiphany_ops::canonical_reduction_order`] / reduce, and re-exporting Agent
//!   C's own determinism gate.
//! * [`equivocation`] — the equivocation harness (criterion 3), driving the real
//!   [`epiphany_ops::OperationSlot`] model and re-exporting Agent C's gate.
//!
//! Agent E (`epiphany-layout-ir`, Chapters 7 & 9) has **not** landed, so the
//! layout round-trip runs against a **faithful in-tree stub** that implements
//! the spec's contract directly:
//!
//! * [`layout_stub`] — a minimal but spec-faithful Chapter 7 / Chapter 9 model:
//!   the four IR stages, the `TimeAxisModel` tagged enum, the provenance
//!   back-references, and the stub solver that returns
//!   [`layout_stub::SolveStatus::Solved`] with the input geometry verbatim.
//!   Drives the layout round-trip (criterion 6).
//!
//! The stub is documented as a stub and is written so that, when
//! `epiphany-layout-ir` lands, [`layout_stub::round_trip`] re-points at the real
//! IR types with minimal churn: the stub types mirror the spec's field names and
//! the provenance/solve contracts are the ones the real crate must also satisfy.
//!
//! ## Determinism of the harness itself
//!
//! Per Appendix D §"Randomness", platform entropy must never enter canonical
//! state — and the testkit goes further: **no platform entropy enters the
//! harness at all.** Every generator draws from the seeded [`rng::Rng`]
//! (a wrapper over Agent A's vendored SplitMix64), so any failing case
//! reproduces exactly from its seed. The harnesses are the strict
//! single-threaded baseline every accelerated implementation must reproduce
//! bit-for-bit.
//!
//! ## The six v0 acceptance criteria
//!
//! The QUICKSTART's six acceptance tests map onto this crate as follows; they
//! are asserted as integration tests in `tests/acceptance.rs` and runnable at
//! scale via `examples/conformance_suite.rs`.
//!
//! | # | Criterion | Entry point |
//! |---|-----------|-------------|
//! | 1 | Convergence | [`convergence::assert_convergence`] |
//! | 2 | Crash safety | [`bundle_harness::run_crash_recovery`] |
//! | 3 | Equivocation | [`equivocation::assert_equivocation_order_independent`] |
//! | 4 | Canonical serialization stability | [`roundtrip::run_roundtrip_corpus`] |
//! | 5 | Reduction determinism | [`convergence::assert_reduction_determinism`] |
//! | 6 | Layout round-trip | [`layout_stub::round_trip`] |

pub mod rng;

pub mod fixtures;
pub mod generators;
pub mod roundtrip;

pub mod convergence;
pub mod equivocation;

pub mod bundle_harness;

pub mod layout_stub;

pub use rng::Rng;
