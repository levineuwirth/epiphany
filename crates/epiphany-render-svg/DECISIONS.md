# epiphany-render-svg — decisions and Pass 12 candidates

This file records (a) the Phase-2 QUICKSTART decisions Agent I made for the
renderer, and (b) ambiguities batched as **Pass 12 candidates**
(`spec/PASS12_BATCH.md`) rather than improvised in code.

## Scope and phase status

`epiphany-render-svg` is one renderer behind the Chapter 7 `RenderIR` interface:
it turns a `ResolvedLayoutIR` into well-formed **SVG 1.1**, drawing each glyph as
a genuine Bravura SMuFL outline `<path>`. Per the QUICKSTART development pattern
it is built and **golden-locked against the stub solver's output first**, before
the real engraving solver and the score→real-notation engraving pass land. The
stub returns the constrained IR's geometry verbatim — a structural projection,
not yet real notation (each layout object becomes one arbitrary glyph in a row) —
so this phase proves the renderer is *correct and faithful* (genuine outlines,
provenance preserved, output XML-valid and deterministic), independent of
engraving quality. The renderer already consumes any solver's `ResolvedLayoutIR`,
so when the real `epiphany-engrave` solver lands, the visible result improves with
no renderer change (the demo binary's `--solver=stub|real` flag exercises both).

## The non-overreach rule (Chapter 7 / QUICKSTART, Agent I)

The renderer makes **SVG-encoding choices only** and **no engraving-semantic**
choices. Every emitted element traces to a `ResolvedGlyph` (and thus a score-graph
source, via `data-prov`/`data-source-kind`) or to a declared renderer wrapper
(the `<svg>` root, the metadata comment, the y-flip `<g>`, a per-layer `<g>`). A
glyph with no bundled outline is **surfaced as a diagnostic** and drawn as a
visible bounding-box fallback `<rect>` — never silently dropped and never
invented. The acceptance harness asserts one drawn element per glyph and one
provenance trace per drawn element.

## Implementation decisions (QUICKSTART "Decisions you'll need to make")

1. **Spelling / 2. Solver architecture** — N/A here (Agent H; `epiphany-engrave`).
3. **Renderer SVG dialect — SVG 1.1 + inline presentation attributes.** Maximum
   portability for what is effectively a viewer/print artifact; no CSS, no SVG 2
   features. (CSS styling can come later if needed.) Output validates under the
   system `xmllint` (libxml2) as well as the in-crate well-formedness checker.
4. **Catalog / 5. Binary Format versioning** — N/A (Agents K, J).

### Local decisions

- **Glyph rendering — inline genuine Bravura outline `<path>`s
  (`GlyphMode::PathOutline`), the default and only mode this phase.** The
  QUICKSTART's recommendation: path outlines make the SVG self-contained (it
  renders in any browser, image tool, or print pipeline with no font installed),
  at the cost of file size. An embedded-`@font-face` mode is a documented future
  option, intentionally **not stubbed** so the interface does not lie about a
  capability that is absent.
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

- **P12-I1** — the constrained IR is a structural placeholder, so the rendered
  stub output is *not yet recognizable notation*. The QUICKSTART's human-review
  visual-acceptance gate ("the SVG visually parses as standard music notation")
  is therefore a **next-phase** gate, met once real engraving lands; this phase's
  gate is renderer correctness/faithfulness. Recorded so the visual gate is not
  mistaken for already-met.
- **P12-I2** — stable layout-object id derivation (`MUSCLOID`, Pass-11 item 2.6,
  deferred to Agent I) is still unwired: the determinism crate exposes no
  `MUSCLOID` tag and is frozen. The renderer traces provenance by the existing
  provisional `stable_id`; wiring the ratified derivation is Track A work.
