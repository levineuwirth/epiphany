# epiphany-render-svg

Agent I's **SVG renderer** behind the Epiphany `RenderIR` interface (spec
Chapter 7): turns a `ResolvedLayoutIR` into well-formed **SVG 1.1**, drawing each
glyph from **genuine Bravura SMuFL** data — inline outline `<path>`s by default
(`GlyphMode::PathOutline`), or `<text>` set in an `@font-face`-embedded Bravura
subset (`GlyphMode::EmbeddedFont`). It is the visible end of the v0
`Score → layout IR` pipeline.

## Status

The `Score → layout IR → SVG` pipeline renders **recognizable notation** — clefs,
noteheads at clef-relative staff positions, accidentals, key/time signatures,
rests, barlines, and the staff lines and stems that connect them. Output is
golden-locked against **both** the interface-only stub solver and Agent I's real
`epiphany-engrave` solver (whose horizontal spacing pass re-spaces the glyphs),
and the layout round-trip (criterion 6) runs through both. What the renderer
itself guarantees, independent of engraving quality: real Bravura glyphs,
provenance preserved to the score graph, output XML-valid and deterministic. The
renderer consumes any solver's `ResolvedLayoutIR`.

## Demo

```sh
# Render a fixture to SVG (stub solver by default):
cargo run -p epiphany-render-svg --example render_fixture -- \
    ten_measure_single_staff > out.svg

# Drive Agent I's engrave solver instead, to bisect renderer-vs-solver:
cargo run -p epiphany-render-svg --example render_fixture -- \
    ten_measure_single_staff --solver=real > out.svg

# Use the embedded-font glyph mode (<text> + @font-face) instead of inline paths:
cargo run -p epiphany-render-svg --example render_fixture -- \
    ten_measure_single_staff --glyph-mode=embedded > out.svg
```

Fixtures: `ten_measure_single_staff`, `valid_score_rich`, `valid_score`. Stats and
diagnostics go to stderr; the SVG goes to stdout.

## Library

```rust
use epiphany_render_svg::{render, RenderOptions};

let out = render(&resolved_layout_ir, &RenderOptions::default());
assert!(out.is_well_formed());
println!("{}", out.svg);
```

`render` is pure and deterministic. `RenderOptions` controls SVG-encoding choices
only (display scale, margin, provenance attributes, and `glyph_mode` — inline
`PathOutline` vs `EmbeddedFont`) — nothing that changes engraving.

## Bundled Bravura data

Two generated artifacts come from the official OFL `Bravura.otf` via
`tools/extract_bravura_outlines.py` — the font is **not vendored**, only the
generated Rust is committed:

- `src/outlines_generated.rs` — the inline glyph outlines (geometry-only, so
  byte-stable across fontTools versions);
- `src/font_subset_generated.rs` — a base64 OTF **subset** (just the pipeline's
  glyphs) for `GlyphMode::EmbeddedFont`. As a Modified Version, its primary font
  name is renamed off the Reserved Font Name "Bravura" per the OFL; a content
  BLAKE3 + decoded length are committed alongside as an integrity lock.

Bravura is © Steinberg Media Technologies GmbH under the SIL Open Font License 1.1
(`tools/OFL.txt`); both artifacts are redistributed under the same license. To
regenerate both (the subset step also needs the `blake3` package):

```sh
cd crates/epiphany-render-svg/tools
python3 -m venv .venv && . .venv/bin/activate && pip install fonttools blake3
python3 extract_bravura_outlines.py --font-out ../src/font_subset_generated.rs \
    > ../src/outlines_generated.rs
```
