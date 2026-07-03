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
//! ## What is real
//!
//! The QUICKSTART charters Agent F to *"build against A and stubs for the
//! others."* All five implementation crates have now shipped — Agents A
//! ([`epiphany_determinism`]), B ([`epiphany_core`]), C ([`epiphany_ops`]),
//! D ([`epiphany_bundle`]), and E ([`epiphany_layout_ir`]) — so **every**
//! harness drives the real crate:
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
//!   (criteria 1 and 5). Criterion 1 proper is **real-Score** convergence
//!   ([`convergence::run_graph_convergence`]): an edit session reduced onto a
//!   real [`epiphany_core::Score`] via [`epiphany_ops::OperationSet::reduce_onto`]
//!   must materialize an identical graph under every delivery order. The
//!   byte-canonical **bookkeeping projection** convergence
//!   ([`convergence::assert_convergence`], over
//!   [`epiphany_ops::OperationSet::reduce`] →
//!   [`epiphany_ops::MaterializedState`]) is retained as a determinism gate and
//!   the basis of criterion 5. Re-exports Agent C's own determinism gate.
//! * [`equivocation`] — the equivocation harness (criterion 3), driving the real
//!   [`epiphany_ops::OperationSlot`] model and re-exporting Agent C's gate.
//! * [`negative`] — regression guards for every defect the Agent C framework
//!   audit surfaced (the M1 fixes), so a regression in `epiphany-ops` trips this
//!   suite directly rather than slipping past a generic convergence gate.
//!
//! * [`layout_stub`] — the layout round-trip harness (criterion 6). Agent E
//!   (`epiphany-layout-ir`, Chapters 7 & 9) has landed, so this module — once a
//!   faithful in-tree stub — now re-exports the **real** IR types (the four IR
//!   stages, the `TimeAxisModel` tagged enum, the provenance back-references,
//!   the engraving-decision and vertical-band models, the glyph-catalog
//!   identity, and the real stub solver) behind the same
//!   [`layout_stub::round_trip`] signature. The provenance-preservation contract
//!   it asserts is implemented and tested inside `epiphany-layout-ir`; the
//!   testkit retains deterministic generators for E's public types and exercises
//!   the real round-trip on its hand-off fixtures. (The module name is kept so
//!   the harness entry point stays `layout_stub::round_trip`.)
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
//! | 1 | Convergence (real Score) | [`convergence::run_graph_convergence`] |
//! | 2 | Crash safety | [`bundle_harness::run_crash_recovery`] |
//! | 3 | Equivocation | [`equivocation::assert_equivocation_order_independent`] |
//! | 4 | Canonical serialization stability (typed + container) | [`roundtrip::run_roundtrip_corpus`] |
//! | 5 | Reduction determinism | [`convergence::assert_reduction_determinism`] |
//! | 6 | Layout round-trip | [`layout_stub::round_trip`] |
//!
//! Criterion 1's reducer-bookkeeping counterpart ([`convergence::assert_convergence`])
//! and criterion 4's bookkeeping-projection serialization
//! ([`roundtrip::assert_reduction_serialization_stable`]) are retained under
//! honest names. The full-`Score` **byte** round-trip
//! ([`roundtrip::assert_score_serialization_stable`]) is now live, driving Agent
//! B's whole-score codec ([`epiphany_core::Score::canonical_bytes`] /
//! [`epiphany_core::Score::decode_canonical`]) on a real `reduce_onto`
//! materialization through a bundle snapshot.

pub mod rng;

// Phase 2, Agent F (worklist F1): the Chapter 10 performance-budget gate the
// `benches/` targets assert through (Pass / Xfail rows with thresholds written
// in the bench source; see `DECISIONS.md` F0/F1).
pub mod budget;

pub mod fixtures;
pub mod generators;
pub mod roundtrip;

// Phase 2, Agent F (for Agent H): the representative score corpus + eligibility
// taxonomy harness (`corpus`, worklist F3) and H's spelling/decomposition merge
// gate (`prepass_harness`, worklist F4). See `DECISIONS.md` F0 for why per-agent
// harnesses are library modules asserted by `tests/` integration tests.
pub mod corpus;
pub mod prepass_harness;

// Phase 3: the Reference Suite companion's executable binding — the six-entry
// v0.1 set solved by the real engraver under the declared configuration, with
// the four-condition Minimal pass rule (`tests/reference_suite.rs` asserts it
// per entry). Library-module-per-harness per DECISIONS F0.
pub mod reference_suite;

pub mod convergence;
pub mod equivocation;
pub mod migration;
pub mod negative;

pub mod bundle_harness;

pub mod layout_stub;

// The editing-loop vertical slice: hit-test → score object → operation → reduce →
// re-layout → re-render → re-resolve selection, across the ops/layout/render seams.
pub mod editloop;

pub use rng::Rng;
