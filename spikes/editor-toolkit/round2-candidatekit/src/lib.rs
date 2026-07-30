//! # round2-candidatekit — Packet 2B-0: the candidate-neutral apparatus, and
//! **nothing else**.
//!
//! `spec/CONTRACT_EDITOR_T4_SPIKE.md` Round 2 scores criterion 3 (text) via
//! the five checks `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md` (W3) §5 names.
//! Packet 2A built every piece of candidate-neutral apparatus those checks
//! are measured against (fixtures, the hit-test probe table, the reference
//! rasters and D4 regions, the accessibility oracle). This crate is Packet
//! 2B-0: it is what the two Round 2 candidates — **C1** (egui + lyon) and
//! **C2** (vello) — both depend on, so that neither one re-derives fixture
//! loading, and neither one gets to define the scoring rule for itself.
//!
//! ## The neutrality boundary — this is the point of the crate
//!
//! The user's ruling, verbatim: **"Share only neutral fixture/oracle
//! loading. Rendering, hit testing, and accessibility integration remain
//! candidate-owned."**
//!
//! This crate **MAY** contain:
//!
//! - Loading and validating fixtures, the probe table, the reference
//!   rasters and region files, and the a11y expectations
//!   ([`inputs::load_all`]).
//! - The shared *report* data shape both candidates emit, and its
//!   serialization ([`report::CandidateReport`] and its constituent types).
//! - The scoring rule that turns per-check outcomes into the criterion cell
//!   ([`scoring::criterion_cell`], [`scoring::is_eligible`]).
//!
//! This crate **MUST NOT** contain:
//!
//! - Any rendering, rasterization, path/outline conversion, or
//!   tessellation.
//! - Any hit-test *resolution* — i.e. nothing that answers "which byte
//!   offset does this device point select". Loading the expected answers
//!   ([`round2_textkit::hittest::HitTestProbeFile`]) is neutral; computing
//!   them is the candidate's job and the thing check 4 measures. This crate
//!   only carries the *shape* of a recorded comparison
//!   ([`report::HitTestProbeResult`]) — it never resolves one.
//! - Any accessibility node construction or platform-adapter code. This
//!   crate only carries the *shape* of observed evidence
//!   ([`report::A11yEvidence`]) against the precommitted oracle
//!   ([`round2_textkit::a11y`]) — it never builds a tree.
//!
//! `tests/dependency_deny_list.rs` enforces what code review can miss: it
//! reads this crate's own `Cargo.toml` at test time and fails if `egui`,
//! `eframe`, `egui-wgpu`, `lyon`, `lyon_path`, `lyon_tessellation`, `vello`,
//! `wgpu`, `winit`, `accesskit`, `accesskit_winit`, `tiny-skia`, `resvg`, or
//! `usvg` is ever named in `[dependencies]`.
//!
//! ## What this crate does not decide
//!
//! [`scoring::criterion_cell`] implements the contract's outcome rule; it
//! does not implement W3 §5 itself, and it is not the place check 3's
//! `NOT RUN` ruling was *made* — that ruling is `ROUND2_TEXT_RECIPE.md`
//! §1.2, and this crate only encodes and enforces its consequences.

pub mod inputs;
pub mod outcome;
pub mod report;
pub mod scoring;

pub use inputs::{load_all, NeutralInputs, ReferenceFixture};
pub use outcome::CheckOutcome;
pub use report::{
    A11yEvidence, AdapterStatus, BusUnreachableEvidence, CandidateReport, CostRecord,
    DependencyDelta, DiffReportRecord, HitTestProbeResult, IntegrationOwnership, LocByPart,
    RegionMassRecord, ReportPart,
};
pub use scoring::{
    criterion_cell, is_eligible, CellOutcome, CHECK_3_RULING, DISQUALIFYING_CHECKS,
    ROUND0_READBACK_EVIDENCE, ROUND_PLATFORM,
};
