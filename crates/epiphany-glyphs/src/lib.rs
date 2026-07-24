#![forbid(unsafe_code)]
//! # epiphany-glyphs
//!
//! The shared typed glyph-asset seam (Editor T4-pre W2,
//! `spec/PLAN_EDITOR_APP.md` §3.7 / Ruling A: "a canvas tessellator needs
//! typed vector paths from a crate both the renderer and the app can depend
//! on"). This crate owns:
//!
//! * the bundled genuine Bravura SMuFL outline data ([`outline`],
//!   [`bundled_glyph_count`], [`smufl_codepoint`]) extracted by
//!   `tools/extract_bravura_outlines.py` into `src/outlines_generated.rs` —
//!   SVG path `d` strings, staff-space units, y-up, moved here from
//!   `epiphany-render-svg` (which previously kept the table private,
//!   `outline()` was `pub(crate)`);
//! * a parser from that `d`-string grammar into
//!   [`epiphany_layout_ir::PathCommand`] (`path` module, private — consumed
//!   only through [`BravuraGlyphCatalog`]), with a round-trip re-emitter
//!   that proves the typed form describes the same geometry (pin 5);
//! * [`BravuraGlyphCatalog`], a real [`epiphany_layout_ir::GlyphCatalog`]
//!   whose `render_data` returns genuine parsed outlines — unlike
//!   `epiphany_layout_ir::BravuraCatalog`, which bundles no render data and
//!   honestly returns `None` for every glyph;
//! * `tools/OFL.txt`, the SIL Open Font License 1.1 these redistributed
//!   outlines travel under (a redistribution condition, not decoration).
//!
//! ## What does NOT live here
//!
//! `BRAVURA_METRICS` stays in `epiphany_layout_ir` (pin 3) — this crate
//! depends on it and never edits or re-derives it; any edit to that table
//! churns every conformance byte in the repo, and it is out of this
//! packet's scope entirely. `epiphany-render-svg`'s embedded-font subset
//! (`font_subset_generated.rs`) also stays put: an embeddable font subset is
//! a renderer concern, not a shared asset (pin 2).
//!
//! ## No cycle, no third-party dependency
//!
//! This crate depends on `epiphany-layout-ir` only (for `BRAVURA_METRICS`,
//! `PathCommand`, and the `GlyphCatalog` vocabulary) — never the reverse —
//! and pulls in no third-party crate (`cargo tree -p epiphany-glyphs` proves
//! it) and uses no `build.rs`: it is in the MSRV closure, and the MSRV job
//! runs `--workspace --exclude epiphany-editor-gui`, so it must stay clean.
//!
//! ## Byte-neutrality for `render-svg`
//!
//! `epiphany-render-svg` depends on this crate for the outline lookup but
//! keeps emitting each glyph's *stored* `d` string, byte-for-byte, in its
//! SVG output — never a re-serialization of the typed outline. Every SVG
//! golden and every layout-conformance byte is unaffected by this crate's
//! existence.

mod catalog;
// `extent` computes an outline's tight bounding box purely to prove test g4
// (the drawn ink fits the declared metrics bbox); nothing in production code
// consumes it, so it is compiled only for `cargo test`.
#[cfg(test)]
mod extent;
mod outline;
mod outlines_generated;
mod path;

pub use catalog::BravuraGlyphCatalog;
pub use outline::{bundled_glyph_count, outline, smufl_codepoint, BravuraOutline};
