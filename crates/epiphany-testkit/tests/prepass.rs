//! **Agent H's merge gate** (Phase 2, Agent F): the spelling + notational-
//! decomposition pre-passes (`epiphany_core::derive_annotations`) asserted
//! against H's `PHASE2_QUICKSTART` acceptance criterion, driven only through the
//! testkit's public surface. A discrete `cargo test` target so a failure is
//! attributable to H (its own CI job, `spec/PHASE2_F_WEEK0_WORKLIST.md` F4); the
//! heavy soak version lives in `examples/conformance_suite.rs`.
//!
//! If any of these fail, H's pre-pass stage is not done — and the non-vacuity
//! guard ([`epiphany_testkit::prepass_harness::assert_non_vacuity`]) would fail
//! if H's work were stubbed or vacuous (the F discipline).

use epiphany_testkit::{corpus, prepass_harness};

/// **F3 — representative corpus + eligibility taxonomy.** ≥20 invariant-clean
/// fixtures across common / edge / torture tiers; every taxonomy bucket is
/// non-empty (or explicitly deferred), H's per-kind counts agree with an
/// independent walk, and every event is bucketed (nothing silently absent).
#[test]
fn h_taxonomy_corpus_coverage() {
    corpus::run_all();
}

/// **F4(H) — the merge gate.** Determinism across runs, eligible-pitch spelling
/// correctness (by pitch class), decomposition reconstruction (invariant 15),
/// `RespellPitch` precedence, spelling vs. published tonal cases, and the
/// non-vacuity tripwire.
#[test]
fn h_merge_gate() {
    prepass_harness::run_all(1);
}

/// The derivation stays deterministic and non-vacuous when run on real reduced
/// scores from the criterion-5 convergence path (so criterion 5 keeps passing
/// with non-trivial pre-pass outputs downstream of materialization).
#[test]
fn h_pre_pass_in_materialization_pipeline() {
    prepass_harness::assert_deterministic_in_materialization_pipeline(1);
}
