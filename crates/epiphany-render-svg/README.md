# epiphany-render-svg

Agent I's **SVG renderer** behind the Epiphany `RenderIR` interface (spec
Chapter 7): turns a `ResolvedLayoutIR` into well-formed **SVG 1.1**, drawing each
glyph as a **genuine Bravura SMuFL outline** `<path>`. It is the visible end of
the v0 `Score → layout IR` pipeline.

## Status: renderer against the stub solver

This phase builds and golden-locks the renderer against the **stub solver's**
output (the QUICKSTART development pattern), before the real engraving solver and
the score→real-notation engraving pass land. The stub returns the IR geometry
verbatim — a structural projection (each object becomes one arbitrary glyph in a
row), not yet recognizable notation — so what is proven here is **renderer
correctness and faithfulness**: real outlines, provenance preserved, output
XML-valid and deterministic. The renderer consumes any solver's output, so the
picture improves with no renderer change once `epiphany-engrave` lands.

## Demo

```sh
# Render a fixture to SVG (stub solver by default):
cargo run -p epiphany-render-svg --example render_fixture -- \
    ten_measure_single_staff > out.svg

# Drive Agent I's engrave solver instead, to bisect renderer-vs-solver:
cargo run -p epiphany-render-svg --example render_fixture -- \
    ten_measure_single_staff --solver=real > out.svg
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
only (display scale, margin, provenance attributes) — nothing that changes
engraving.

## Bundled outlines

The glyph outlines in `src/outlines_generated.rs` are extracted from the official
OFL `Bravura.otf` by `tools/extract_bravura_outlines.py`. The font is not
vendored; only the generated Rust is committed. Bravura is © Steinberg Media
Technologies GmbH under the SIL Open Font License 1.1 (`tools/OFL.txt`); the
extracted outlines are redistributed under the same license. To regenerate:

```sh
cd crates/epiphany-render-svg/tools
python3 -m venv .venv && . .venv/bin/activate && pip install fonttools
python3 extract_bravura_outlines.py > ../src/outlines_generated.rs
```
