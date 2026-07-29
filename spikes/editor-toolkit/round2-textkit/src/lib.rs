//! # round2-textkit — Packet 2A-i: the candidate-neutral text fixture set.
//!
//! **Nothing in this crate is canonical, and nothing here pre-empts the
//! `.tex` amendment.** `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md` (W3) §3E and §4
//! propose `ResolvedText` as a fourth resolved-layout primitive; that
//! amendment has not landed. Every type in [`types`] is a **non-canonical
//! spike mirror**, prefixed `Spike*`, that exists only so this crate — the
//! shape's first consumer (`ROUND2_TEXT_RECIPE.md` intro) — can exercise it
//! before the real thing exists. A type here proving awkward to populate is
//! itself the deliverable: pin 8 routes that awkwardness back to the
//! amendment as a finding (see [`findings`]).
//!
//! ## What this crate does
//!
//! 1. Resolves the two declared faces from an explicit path list, hashing
//!    their bytes and failing loudly on any mismatch against the recipe's
//!    recorded hashes ([`faces`]).
//! 2. Records every pin-9 shaping-identity field, including the two
//!    separately-versioned Unicode components W3-F2 names ([`identity`]).
//! 3. Itemizes, shapes, and clusters the five committed fixture strings —
//!    `unicode-bidi` for bidi itemization, `rustybuzz` for shaping in the
//!    resolved face, `unicode-segmentation` for grapheme caret stops
//!    ([`shape`], [`fixtures`]).
//! 4. Asserts every W3 §5 invariant on the result, in code, naming the
//!    fixture and the invariant on failure ([`invariants`]).
//! 5. Writes `fixtures.json` and `FIXTURES_SUMMARY.md`, and exposes
//!    [`output::FixtureFile::validate`] so a later packet can check the
//!    loaded fixtures against literals restated here rather than against the
//!    file's own other fields — the same discipline
//!    `round1-candidates/harness`'s `OracleFile::validate` uses.
//! 6. Builds and validates the recipe §7 closing-paragraph hit-test *probe
//!    table* (`(device point) -> (byte offset, affinity)`), from an
//!    already-loaded, already-valid `fixtures.json` — [`hittest`],
//!    `bin/generate_hittest.rs`, `hittest_probes.json`. This is
//!    candidate-testing apparatus, not part of the candidate-neutral §3E
//!    mirror, which is why it is a sibling file rather than extra fields on
//!    `fixtures.json`'s own records.
//!
//! ## What this crate does not do
//!
//! It renders nothing. The SVG reference emitter (recipe §9) and the bounded
//! visual differential (recipe §10/§11) are separate packets (`round2-diff`
//! is the differential, built in parallel).

pub mod a11y;
pub mod faces;
pub mod fixtures;
pub mod hittest;
pub mod identity;
pub mod invariants;
pub mod output;
pub mod quantize;
pub mod shape;
pub mod types;

/// Em size, fixed for every fixture (recipe §3, **amended from the
/// original `0.64`**): `1.28` staff spaces = 128 device px at `scale = 100`.
///
/// The recipe originally pinned `0.64` (64 device px). That was wrong:
/// measured against TeX Gyre Pagella at 64 px em, the mid-height stem width
/// of a lowercase vertical (`l`/`i`/`n`, 84 font units at `upem = 1000`) is
/// 5.4 device px — inside `round2-diff`'s `EDGE_BAND_PX = 2` on *each* side
/// of an edge, i.e. a stem narrow enough that the whole stroke sits in the
/// antialiased band D1 is defined to be blind to. Doubling to `1.28` (128 px
/// em) doubles every stem to ~10.8 px (round strokes like `o`/`e` to ~12 px),
/// which leaves interior pixels D1 can actually decide. All five fixtures
/// still clear the 1920 px target at this size — measured device right edges
/// from the generated `fixtures.json`: F-A 1715.1, F-D 1290.7, F-E 1023.0,
/// F-B 809.1, F-C 597.0. F-A is the longest and clears the frame by ~205 px.
pub const EM_SIZE_STAFF_SPACE: f64 = 1.28;

/// The run's baseline origin in staff space (recipe §3): `1638/1024 =
/// 1.599609375`, i.e. device `(159.9609375, 540)` under `scale = 100` device
/// px per staff space, `target = 1920x1080`.
///
/// **Stated on the grid, not rounded onto it.** Invariant 5 requires every
/// position to sit exactly on the `1/1024` staff-space grid, and the recipe's
/// original `1.6` is not representable there (`1.6 × 1024 = 1638.4`). The
/// value used to be `1.6` and [`quantize::quantize_component`] moved it onto
/// the grid on the way past — which worked, and left the *stated* constant in
/// violation of the *stated* invariant. `1638.0 / 1024.0` is exact in `f32`
/// (1638 needs 11 mantissa bits) and in `f64`, so quantization is now a
/// no-op for it and the two agree at the source. Every fixture position is
/// numerically unchanged by this; it is the constant that was wrong, not the
/// output.
pub const RUN_ORIGIN_STAFF: (f32, f32) = (1638.0 / 1024.0, 0.0);

/// Pin 4's offscreen target, restated here (not read from any file) so a
/// validator checks it against a literal, exactly as
/// `round1-candidates/harness`'s `TARGET_WIDTH`/`TARGET_HEIGHT` do.
pub const TARGET_WIDTH: f64 = 1920.0;
pub const TARGET_HEIGHT: f64 = 1080.0;

/// Device pixels per staff space (recipe §3): `scale = 100`. Used by
/// [`hittest`] to convert caret-stop positions to device space, and (for the
/// same reason every other geometric constant here is a literal) restated
/// rather than derived.
pub const DEVICE_SCALE: f64 = 100.0;

/// The canonical-layout quantization grid this text mirror reuses (W3 §3E
/// invariant 5): `1/1024` staff space per unit, identical to
/// `epiphany_determinism::QuantizedCoord`.
pub const QUANTIZE_GRID: f64 = 1024.0;

/// Findings routed back to the W3 `.tex` amendment (recipe §12), discovered
/// during this packet's implementation. `W3_F1`/`W3_F2` are the two the
/// recipe already named before implementation began; anything past those is
/// new to this packet.
pub mod findings {
    /// `TextFaceIdentity::version: Option<SemVer>` is the wrong type — real
    /// font versions are not semver (recipe §12, precommitted). This crate's
    /// [`crate::identity::SpikeTextFaceIdentity::version`] carries the raw
    /// name-table string instead, `Option<String>`.
    pub const W3_F1: &str = "TextFaceIdentity::version must be Option<String> (raw name-table \
        text), not Option<SemVer> — real font versions are not semver.";

    /// One `unicode_version` field cannot honestly name two independently
    /// versioned components (bidi itemization, grapheme segmentation) (recipe
    /// §12, precommitted). This crate's
    /// [`crate::identity::SpikeTextShapingIdentity`] carries
    /// `unicode_bidi` and `unicode_segmentation` as two separate fields.
    pub const W3_F2: &str = "unicode_version must be two fields (bidi, segmentation), each \
        naming its own implementation, crate version, and Unicode-data version — a single \
        field cannot honestly name two independently-versioned components.";

    /// **New in this packet.** `ShapedSegment::face: u32` has no value to
    /// record when a run's codepoints are covered by *no* declared face
    /// (F-C, U+0627). Invariant 2 requires segment source ranges to
    /// *totally* partition the string — visual order may differ from
    /// logical order, but the partition must be total — so an unresolved
    /// span cannot simply have no segment; it needs a segment whose `face`
    /// field can say "none of the chain." This crate's
    /// [`crate::types::SpikeShapedSegment::face`] is `Option<u32>`, not
    /// `u32`, for exactly this reason: `None` names "resolution walked the
    /// whole declared chain and nothing covered this span," carrying the
    /// same information invariant 4's unresolved-cluster marker carries, at
    /// the segment granularity invariant 2 needs it at.
    pub const W3_F3: &str = "ShapedSegment::face must be Option<u32>, not u32 — an unresolved \
        span (no face in the declared chain covers it) still needs a segment for invariant 2's \
        total partition, and that segment has no face index to report.";

    /// **New in this packet, found by writing the wire format §3E does not
    /// have.** `epiphany-layout-ir` has no `serde` dependency, so nothing in
    /// §3E — `ResolvedText` included — can be serialized. Every consumer that
    /// needs to persist, cache, dump, or send one across a process boundary
    /// therefore has to hand-write a mirror, and the very first consumer to do
    /// it (this crate) wrote one that was quietly lossy for two `Provenance`
    /// fields until it was caught in review: a `Debug` rendering in place of
    /// `source`, and a length in place of `dependencies`.
    ///
    /// The mirror is fixed here (see [`crate::types::SpikeProvenance`]), but
    /// the *shape of the mistake* is what routes back: an incremental-layout
    /// cache and an out-of-process renderer are both plainly in W3's future,
    /// each needs this exact conversion, and each will write it independently.
    /// The amendment should say what `ResolvedText`'s serialized form is —
    /// derive it, or specify a canonical byte form as Chapter 5 does for
    /// `TypedObjectId` — rather than leave one per consumer.
    pub const W3_F6: &str = "§3E defines no serialized form for ResolvedText, and \
        epiphany-layout-ir carries no serde at all, so every consumer that must persist or send \
        one hand-writes its own mirror — the first one written (this spike's) was lossy for two \
        Provenance fields. The amendment should specify the serialized form once.";
}
