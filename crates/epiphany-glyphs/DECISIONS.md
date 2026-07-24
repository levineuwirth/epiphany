# epiphany-glyphs — decisions

Editor T4-pre W2 (`spec/CONTRACT_EDITOR_T4PRE_W2_GLYPHS.md`): the shared
typed glyph-asset seam a canvas tessellator needs, populated on top of the
already-designed `layout-ir` interface (`PathCommand`, `GlyphRenderData`,
`GlyphCatalog::render_data`) rather than designing a new one.

## Scope and status

New crate. Owns the bundled Bravura outline data (`src/outlines_generated.rs`,
moved from `epiphany-render-svg`, which held it `pub(crate)` and private to
itself), the extractor tool and its OFL license (`tools/`), an in-crate
grammar-specific parser from the bundled SVG path `d` strings into
`epiphany_layout_ir::PathCommand` (`src/path.rs`, private — reachable only
through the catalog), and `BravuraGlyphCatalog` (`src/catalog.rs`): a real
`GlyphCatalog` whose `render_data` returns genuine outlines, unlike
`epiphany_layout_ir::BravuraCatalog`, which is metrics-only and honestly
returns `None`.

`BRAVURA_METRICS` does not move here and is not edited (contract pin 3): it
stays in `epiphany-layout-ir`, and this crate depends on it read-only. No
cycle: `epiphany-glyphs` → `epiphany-layout-ir`, never the reverse.

## The grammar the contract described vs. the grammar the generator emits

The contract's "verified starting point" states the bundled `d` strings use
"absolute `M`/`L`/`C`/`Z`, decimal coordinates, 4 decimal places". Verified
against all 37 bundled glyphs before writing the parser (not assumed), the
real grammar is wider on both counts, traced to
`tools/extract_bravura_outlines.py`'s use of
`fontTools.pens.svgPathPen.SVGPathPen`, whose default `optimizeCommands`
behavior the extractor never disables:

* **`V` (vertical-only) and `H` (horizontal-only) lineto shorthand** are also
  emitted, absolute, whenever a lineto's target shares an axis with the
  current point — 23 of the 37 bundled glyphs use at least one. `PathCommand`
  (the already-designed shared type) has no shorthand variant, so `parse_d`
  lowers `V`/`H` to `PathCommand::LineTo`; `emit_d` reconstructs the
  shorthand byte-for-byte purely from geometry — comparing the lineto's
  target to the current point (`V` when only *x* is unchanged, `H` when only
  *y* is, `L` otherwise) — which is exactly what the round-trip test proves
  for every bundled glyph. No information is lost: the typed form and the
  stored string are geometrically and (after this reconstruction)
  byte-for-byte equivalent.
* **Coordinates are rounded to *at most* 4 decimals**, with trailing zeros
  and a bare `-0` stripped by the generator's own `round_d`
  (`tools/extract_bravura_outlines.py:180-185`: `f"{v:.4f}".rstrip('0').rstrip('.')`,
  normalising `""`/`"-0"` to `"0"`). Printed precision therefore varies per
  number (0-3 fractional digits observed in the bundled data; 4 is the
  ceiling, not the width). `emit_d`'s number formatter reproduces this exact
  rule.
* Every command in the observed grammar carries exactly one point/coordinate
  group (`M`, `L`, `V`, `H` one; `C` three points; `Z` none) — the generator
  never merges consecutive same-type commands into a multi-coordinate group
  (SVG allows this generally; the fontTools pen never emits it), so the
  parser does not implement that generality either.

This is a correction to the contract's stated facts, not a deviation from its
design pins: pin 4 ("an in-crate parser … covers exactly the grammar the
generator emits") is satisfied by parsing the *real* grammar, which is what
"exactly the grammar the generator emits" requires once the grammar is
actually read from the data rather than assumed from the contract's summary.

## Round-trip proof, not the sanctioned fallback (pin 5)

Pin 5 sanctions a weaker fallback — comparing parsed coordinate sequences as
exact `f32` — if the generator's number formatting cannot be reproduced
exactly. It can: `emit_d`'s formatter is a direct transcription of
`round_d`'s Python (round to 4, strip trailing zeros/dot, normalise `-0`),
verified by an actual byte-for-byte `assert_eq!` against every bundled
glyph's stored `d` string (`path.rs`'s
`every_bundled_glyph_round_trips_byte_for_byte`, the load-bearing test). The
fallback was not needed and was not taken.

## `render_data` caching (pin 6)

`BravuraGlyphCatalog::render_data` looks up a `std::sync::OnceLock`-cached
`BTreeMap<&'static str, Vec<PathCommand>>`, built once by parsing every
bundled outline's `d` string on first use. `parse_d` is pure, so the cached
value a lookup clones is exactly what a fresh parse of the same bundled
string would produce on every call — caching changes only *when* the parse
work happens, never *what* it returns. No interior mutability leaks into the
result; two calls for the same name compare equal (`GlyphRenderData:
PartialEq`), asserted directly.

## `epiphany-render-svg`'s byte-neutrality (pin 2, pin 5)

`render-svg`'s `outline()`/`bundled_glyph_count()`/`smufl_codepoint()` now
delegate to this crate (`src/outline.rs` in each crate — `render-svg`'s copy
is a thin re-export/delegation, this crate's is the real lookup). Critically,
`render-svg`'s SVG emission (`svg.rs`'s `GlyphMode::PathOutline` arm) still
reads `outline(name).path` — the *stored* string — directly into the `<path
d="…">` attribute. It never routes through `parse_d`/`emit_d`; those exist
only for the round-trip proof and are not reachable from `render-svg` at all
(`path` is a private module here, and its functions are `pub(crate)`, not
exported). This is why the byte-neutrality probe (every SVG golden, every
`ResolvedLayoutIR::canonical_bytes()` reference-suite fixture) is unaffected
by this packet's existence — verified before and after against the base
commit, `diff -r` empty.

## Why a new crate and not a `layout-ir` module

The contract left this open (§CONTRACT_EDITOR_T4PRE_IR.md W2 framing: "the
open decision W2's contract resolves is the home … judged on dependency
weight and the MSRV/CI job structure"), and `CONTRACT_EDITOR_T4PRE_W2_GLYPHS.md`
pin 1 settles it: a new crate. Reasons that held once the data was in hand:

* `layout-ir` is deliberately data-light — it defines the `GlyphCatalog`
  *interface* and a small in-tree metrics slice, not asset payloads; the
  bundled outline table is ~37 KB of generated Rust literal that has nothing
  to do with the constraint-solver interface `layout-ir` exists to define.
* A separate crate keeps the "moved verbatim, byte-for-byte identical output"
  claim easy to audit: the outline table's home changed, its content and the
  bytes it produces did not.
* It matches the existing pattern (`epiphany-render-svg` already held this
  exact data privately) — moving it sideways into a shared crate is a
  smaller, more reviewable diff than folding it into `layout-ir` and then
  re-exporting it back out for `render-svg`'s and a future canvas renderer's
  use.
