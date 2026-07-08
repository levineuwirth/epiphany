# epiphany-render-svg — decisions and Pass 12 candidates

This file records (a) the QUICKSTART decisions Agent I made for the renderer,
and (b) ambiguities batched as **Pass 12 candidates** (`spec/PASS12_BATCH.md`)
rather than improvised in code.

## Scope and status

`epiphany-render-svg` is the SVG renderer behind the Chapter 7 `RenderIR`
interface: it turns a `ResolvedLayoutIR` into well-formed **SVG 1.1**, drawing
glyphs from genuine Bravura SMuFL data either as inline outline `<path>`s
(`GlyphMode::PathOutline`) or as `<text>` set in an embedded subset font
(`GlyphMode::EmbeddedFont`). Per the QUICKSTART development pattern it was
golden-locked against the stub solver's output first, then against the real
`epiphany-engrave` solver once the score→real-notation pass and re-spacing
landed. The renderer consumes any solver's `ResolvedLayoutIR`; it proves
renderer faithfulness — resolved geometry preserved, provenance traced,
output XML-valid and deterministic — independent of engraving quality.

## The non-overreach rule (Chapter 7 / QUICKSTART, Agent I)

The renderer makes **SVG-encoding choices only** and **no engraving-semantic**
choices. In the default **archival** mode every emitted element traces to a
`ResolvedGlyph` (and thus a score-graph source, via `data-prov`/`data-source-kind`)
or to a declared renderer wrapper (the `<svg>` root, the metadata comment, the
y-flip `<g>`, a per-layer `<g>`). Traces can be turned off
(`RenderOptions::emit_provenance = false`) for a smaller **display-only** SVG; that
is an explicit mode the **metadata comment declares** (archival → "carries a
data-prov trace", display-only → "provenance traces suppressed"), so a trace-free
SVG — including the empty canvas — announces itself rather than passing as
archival. A glyph with no bundled outline is **surfaced as a diagnostic** and drawn
as a visible bounding-box fallback `<rect>` — never silently dropped and never
invented. The acceptance harness (archival mode) asserts one drawn element per
glyph and one provenance trace per drawn element.

## Implementation decisions (QUICKSTART "Decisions you'll need to make")

1. **Spelling / 2. Solver architecture** — N/A here (Agent H; `epiphany-engrave`).
3. **Renderer SVG dialect — SVG 1.1 + inline presentation attributes.** Maximum
   portability for what is effectively a viewer/print artifact; no CSS, no SVG 2
   features. (CSS styling can come later if needed.) Output validates under the
   system `xmllint` (libxml2) as well as the in-crate well-formedness checker.
4. **Catalog / 5. Binary Format versioning** — N/A (Agents K, J).

### Local decisions

- **Glyph rendering — two self-contained modes; inline outlines
  (`GlyphMode::PathOutline`) is the default and the verified reference.** Path
  outlines make the SVG self-contained (it renders in any browser, image tool, or
  print pipeline with no font installed) and are byte-golden-locked, at the cost of
  file size — the QUICKSTART's recommendation. `GlyphMode::EmbeddedFont` instead
  references each glyph by SMuFL codepoint with a `<text>` element drawn from an
  `@font-face`-embedded Bravura *subset* (only the ~33 named glyphs), so the SVG is
  still self-contained (the font travels in it) and the text is selectable, at a
  larger file size. The two modes anchor glyphs at the same origin (em = 4 staff
  spaces), so placement is consistent by construction; the embedded mode is
  structurally tested rather than byte-golden-locked, and exact rasterisation is
  the consumer's font renderer's, so path mode remains the pixel-verified one.
- **Embedded-font subset — generated, not a vendored binary.** The subset is a
  deterministic base64 OTF emitted into `src/font_subset_generated.rs` by
  `tools/extract_bravura_outlines.py --font-out`, keeping the "only generated
  artifacts committed" rule (no font binary is vendored). It retains the font's
  OFL copyright/license name records (belt-and-suspenders with `tools/OFL.txt`).
  Caveat: unlike the geometry-only outlines, the binary subset's exact bytes
  depend on the fontTools version, which the generated header records.
- **Outline source — the official OFL `Bravura.otf`, extracted reproducibly.**
  `tools/extract_bravura_outlines.py` fetches the font + SMuFL `glyphnames.json`
  and emits `src/outlines_generated.rs`. The font is **not vendored**; only the
  generated Rust is committed (`tools/OFL.txt` carries the SIL Open Font License
  1.1 under which the outlines are redistributed). Exactly the glyph set the v0
  pipeline can name (`layout-ir`'s `BRAVURA_METRICS`) is bundled; a test asserts
  every pipeline glyph has an outline (so the table cannot silently fall behind).
- **Coordinate system — staff spaces, y-up, one global flip.** Outlines are
  extracted in staff-space units (SMuFL em = 4 staff spaces, Bravura
  unitsPerEm = 1000 ⇒ 1 staff space = 250 font units), **y-up** (musical
  convention). The whole document is wrapped in one
  `translate(-min_x, max_y) scale(1, -1)` group, so every glyph is placed with a
  plain `translate(x, y)` and the `viewBox` is `0 0 W H` in staff spaces;
  `width`/`height` carry the px display scale. The renderer never bakes the flip
  into per-glyph data.
- **Determinism — fixed 4-decimal number formatting, `-0` normalised to `0`.**
  Outline `d` data is fixed bundled text; every computed coordinate is formatted
  identically, so identical input yields byte-identical SVG. The acceptance
  harness golden-locks the full SVG and a machine snapshot (object/glyph/path/
  provenance/layer/per-class/hard-constraint counts + XML validity).
- **No external XML dependency.** The workspace is deliberately dependency-light;
  `xml::check_well_formed` is a hand-rolled validator for the subset the renderer
  emits, and the acceptance test additionally cross-checks with `xmllint` when
  present so the "XML-validates" claim rests on a real parser too.

## Pass 12 candidates

See `spec/PASS12_BATCH.md` (rows P12-I1, P12-I2, P12-I3). Most relevant here:

- **P12-I1 (resolved by I-1/I-3)** — the original stub-only renderer output was
  a structural placeholder, so the human-review visual-acceptance gate ("the SVG
  visually parses as standard music notation") was deferred until real engraving
  landed. The real notation pass and real-Engraver goldens now close that gate;
  the stub path remains locked as an interface/reference mode, not the visual
  deliverable.
- **P12-I2 (resolved)** — the stable layout-object id derivation (`MUSCLOID`,
  Pass-11 item 2.6) is wired: `epiphany-determinism` reserves the built-in
  `MUSCLOID` tag and `layout-ir` provenance routes through it. The renderer traces
  provenance by the (now `MUSCLOID`-tagged) `stable_id`; only the `data-prov` hex in
  the goldens changed (the ids are non-canonical).

## Repeat glyphs bundled + `repeat` glyph class (E1, 2026-07-07)

The E1 repeat tranche added `repeatLeft`, `repeatRight`, `repeatRightLeft`,
and `repeatDots` to the extractor's `NAMES` set; outlines, companion
layout-ir metrics, and the embedded font subset were regenerated from the
same SHA-pinned `bravura-1.392` (fontTools 4.63.0 preserved, so the subset
diff is content-only). `GlyphClass` gained a `Repeat` class (token
`repeat`) so snapshots and `data-class` attributes separate repeat signs
from plain barlines. Volta ending numerals arrive as `timeSig` digit glyphs
(there is still no free-text primitive — unchanged).
