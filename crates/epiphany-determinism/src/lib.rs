#![forbid(unsafe_code)]
//! # epiphany-determinism
//!
//! The reproducibility contract types for Epiphany, implementing the
//! normative requirements of **Appendix D (Determinism Contract)** of the
//! core specification. This crate is deliberately the smallest in the
//! workspace and depends on nothing but [`blake3`]: every other crate
//! depends on it, so it must stay free of crate-local semantics and of any
//! dependency cycle.
//!
//! The spec's thesis (Appendix D §"Thesis"): *canonical document state must
//! be independent of platform, CPU, locale, thread scheduling, hash-map
//! iteration order, floating-point environment, compression settings, and
//! wall-clock timing.* Everything here exists to make that hold by
//! construction.
//!
//! ## What lives here
//!
//! * [`QuantizedCoord`] — the `1/1024` staff-space canonical spatial grid
//!   (Appendix D §"Quantized Layout Coordinates", Chapter 7 §7.2).
//! * [`ContentHash`] / [`ChunkId`] — the BLAKE3-256 content-address newtypes
//!   (Chapter 8 §"Content Hashing"), plus [`blake3_256`], [`trunc64`],
//!   [`trunc128`], and the [`Preimage`] domain-separated hash builder.
//! * [`DomainTag`] and the `"MUSC*"` domain-tag constants
//!   (Chapter 8 §"Domain-Separated Preimages").
//! * [`Tolerance`], [`ToleranceClass`], [`ToleranceGovernance`] — the five
//!   named tolerance classes (Appendix D §"Tolerance Classes"). Ad-hoc
//!   epsilons are forbidden; every normative tolerance is one of these.
//! * [`CanonicalF64`] and the float-hygiene helpers — finite-only canonical
//!   `f64`, `-0.0 -> +0.0` canonicalization, little-endian canonical bytes
//!   (Appendix D §"Floating-Point Values in Canonical State").
//! * [`CanonicalEncode`] / [`CanonicalDecode`] and the canonical-iteration
//!   helpers ([`sort_canonical`], [`CanonicalMap`], [`CanonicalSet`])
//!   (Appendix D §"Ordered Iteration over Sets and Maps").
//!
//! ## What deliberately does *not* live here
//!
//! No async, no I/O, no platform calls. The crate is pure value types and
//! pure functions; this is the "strict single-threaded baseline" every
//! parallel or accelerated implementation must reproduce bit-for-bit.
//!
//! ## Implementation decisions (per QUICKSTART "Decisions you'll need to make")
//!
//! * **Sync only.** No async traits anywhere in this crate (decision 4).
//! * **MSRV 1.77** for [`f64::round_ties_even`]; the spec uses no exotic
//!   Rust features (decision 5).
//! * **`blake3` is the sole dependency.** Identifier/conflict derivation and
//!   content addressing all route through it; no second hash ever appears in
//!   this format version (Chapter 8 §"Content Hashing").

mod coord;
mod domain;
mod float;
mod hash;
mod order;
mod serialize;
mod tolerance;

pub mod fuzz;

pub use coord::{QuantizedCoord, STAFF_SPACE_GRID};
pub use domain::{DomainTag, SystemDomainTag, BUNDLE_MAGIC, SUPERBLOCK_MAGIC};
pub use float::{canonical_f64_bytes, canonicalize_zero, debug_assert_canonical, CanonicalF64};
pub use hash::{
    blake3_256, derive_system_counter, trunc128, trunc64, ChunkId, ContentHash, Preimage,
};
pub use order::{sort_canonical, sorted_canonical, CanonicalByteOrder, CanonicalMap, CanonicalSet};
pub use serialize::{CanonicalDecode, CanonicalEncode, DecodeError};
pub use tolerance::{Tolerance, ToleranceClass, ToleranceGovernance};
