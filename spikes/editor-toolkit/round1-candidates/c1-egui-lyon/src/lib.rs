//! Packet 2B-C1: Round 2 (text) shared modules for candidate C1
//! (egui + lyon).
//!
//! `src/main.rs` — the Round 1 binary — is frozen evidence and does not use
//! this library; it is untouched by this packet (confirmed by `git diff` in
//! the packet's own report). This crate gains a `[lib]` target purely so
//! `src/bin/c1_round2_text.rs` and `src/bin/c1_round2_a11y.rs` can share
//! candidate-owned logic.
//!
//! ## The F3 cost-schema mapping: one `ReportPart` per whole file
//!
//! Every module below maps to exactly one `round2_candidatekit::ReportPart`,
//! and no file contributes to two parts — the rule the F3 finding fixed
//! this packet to follow, so the per-part LOC comparison against C2 is
//! actually comparable rather than an artifact of how one candidate happened
//! to split its own files.
//!
//! | module | `ReportPart` |
//! |---|---|
//! | [`glyph_outline`] | `TextRendering` |
//! | [`render_target`] | `TextRendering` |
//! | [`hit_test`] | `HitTestResolution` |
//! | [`a11y_node`] | `AccessibilityTreeConstruction` |
//! | [`a11y_app`] | `AccessibilityIntegrationWiring` |
//! | [`a11y_subprocess`] | `AccessibilityIntegrationWiring` |
//!
//! `bin/c1_round2_text.rs` and `bin/c1_round2_a11y.rs` themselves are
//! `FixtureAndReportPlumbing` — fixture/font loading, diff invocation,
//! report assembly, CLI — the two are printed in the run's own output
//! (`c1_round2_text`'s `loc_by_part` section) rather than only asserted here.

pub mod a11y_app;
pub mod a11y_node;
pub mod a11y_subprocess;
pub mod glyph_outline;
pub mod hit_test;
pub mod render_target;
