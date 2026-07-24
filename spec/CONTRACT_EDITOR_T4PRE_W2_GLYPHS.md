# Contract: Editor T4-pre W2 — the shared typed glyph-asset seam

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/CONTRACT_EDITOR_T4PRE_IR.md` §W2 and `spec/PLAN_EDITOR_APP.md` §3.7 /
Ruling A, which names "a shared typed glyph-asset seam" as a T4 prerequisite:
a canvas tessellator needs typed vector paths from a crate both the renderer
and the app can depend on. W1 landed at `dd33b34`.

Execution model as T1a/T2/T3/W1: Sonnet subagent, coordinator line-level
review with independent mutation re-runs, user deep-dives at contract
sign-off, any new golden baseline, and the final report. Mutation discipline
throughout: anchor-assert before substituting, restore by reversing, never
`git checkout`.

**Parallel safety.** The genesis-operation tranche is in flight on the other
track, laddered as G1/G2/G3 in `spec/PLAN_GENESIS_OPS.md` under
`spec/RULING_GENESIS_PERSISTENCE.md`. Across its rungs it owns
`epiphany-core`, `epiphany-ops`, all `.tex` (so the requirement counts move),
testkit's requirement-label constants, vectors and fuzz generators, and — at
G2 — `epiphany-bundle` for the `OperationEnvelopeBlock` accept-set raise.
**This packet touches none of them**, at any rung. Report observed requirement
actuals; never assert stale numbers.

---

## The verified starting point

Confirm these as you go; they make the packet far smaller than §3.7's wording
suggests:

* **The typed path type already exists, in `layout-ir`.** `PathCommand`,
  `GlyphRenderData { outline: Vec<PathCommand>, bitmap: Option<GlyphBitmap> }`,
  and `GlyphCatalog::render_data(&self, name) -> Option<GlyphRenderData>` are
  all defined (`layout-ir/src/glyph.rs:~285-315`). **W2 populates a seam that
  is already designed; it does not design one.** `BravuraCatalog::render_data`
  returns `None` today by deliberate, documented honesty — "reporting `Some`
  would claim render data that does not exist."
* **The outlines exist, as SVG `d` strings**, in
  `render-svg/src/outlines_generated.rs`: `BravuraOutline { name, codepoint,
  path: &'static str, bbox: [f32; 4] }`, staff-space units, y-up, 4 decimals,
  sorted by name. `outline()` is `pub(crate)`; `bundled_glyph_count()` and
  `smufl_codepoint()` are `pub` but have **no callers outside `render-svg`**
  (verified workspace-wide).
* **`Bravura.otf` is NOT in the tree.** `render-svg/tools/` holds only
  `extract_bravura_outlines.py` and `OFL.txt`; the generated header pins
  source SHA-256s verified *at extraction time*. Re-running the extractor
  therefore requires fetching the font and **cannot be assumed**. This
  determines pin 4.
* **The metrics table is conformance identity.** `BRAVURA_METRICS` lives in
  `layout-ir/src/glyph.rs`; `metrics_hash_for` hashes `(name, metrics)` pairs
  with the *values* participating (the crate's own note at `glyph.rs:372-375`
  exists precisely to prove a field participates), and `GlyphCatalogIdentity`
  is encoded into `ResolvedLayoutIR`'s canonical bytes.

## Design pins

1. **A new crate, `epiphany-glyphs`**, owns the glyph assets: the moved
   `outlines_generated.rs`, the extractor tool, and **`OFL.txt`, which travels
   with the redistributed outlines** (the generated file's license header
   stays intact — Bravura is SIL OFL 1.1 and the notice is a redistribution
   condition, not decoration). It depends on `epiphany-layout-ir` and exposes
   a catalog type implementing `GlyphCatalog` with a **real `render_data`**.
   No cycle: glyphs → layout-ir, never the reverse.
2. **`render-svg` keeps its public API and its exact output.** It depends on
   `epiphany-glyphs`, and `bundled_glyph_count`/`smufl_codepoint` remain
   exported from `render-svg` (delegating or re-exporting) even though nothing
   outside calls them — an API that costs one `pub use` is not worth breaking.
   `font_subset_generated.rs` stays in `render-svg`: an embeddable font subset
   is a renderer concern, not a shared asset.
3. **The metrics table does not move and does not change.** Out of bounds
   entirely. Any edit to it churns every conformance byte in the repo.
4. **Typed paths are derived from the `d` strings by an in-crate parser, not
   by re-running the extractor.** The font is not in the tree, so regeneration
   is not reproducible here. The parser covers exactly the grammar the
   generator emits — absolute `M`/`L`/`C`/`Z`, decimal coordinates — and the
   crate stays **dependency-free** (it is in the MSRV closure; no new
   dependency, no `build.rs`).
5. **Equivalence is proven by round-trip, not asserted.** Parse every bundled
   glyph's `d`, re-emit it in the generator's exact formatting, and compare to
   the original string **byte-for-byte**. If all bundled glyphs round-trip,
   the typed form provably describes the same geometry and no geometric
   spot-checking is needed. *Sanctioned fallback if the generator's number
   formatting cannot be reproduced exactly:* parse both the original and the
   re-emitted string and compare the full coordinate sequences as exact `f32`.
   **Report which was used and why** — do not silently take the weaker one.
6. **`render_data` is deterministic and side-effect-free.** Caching (a
   one-time lazily built table) is permitted; it must not change results, and
   two calls must return equal data.

## Tests + minimum mutations

* **(g1) parser round-trip** over every bundled glyph (pin 5) — the packet's
  load-bearing test. *Mutation:* perturb one parsed coordinate → dies.
* **(g2) `Close` survives parsing** — *mutation:* drop the trailing `Z` →
  dies.
* **(g3) every pipeline glyph has render data** — the `BRAVURA_METRICS` name
  set all resolve through the new catalog's `render_data`, mirroring
  `render-svg`'s existing `every_pipeline_glyph_has_a_bundled_outline`.
  *Mutation:* return `None` for one bundled glyph → dies.
* **(g4) outline ink fits the declared metrics bbox** — the cross-table
  consistency the tree asserts in prose and tests nowhere: `glyph.rs:107-109`
  claims the metrics and outlines come from the same release "so reserved
  advances/bboxes and the drawn ink agree." Compare each glyph's outline
  extent (staff spaces) against its `BRAVURA_METRICS` bbox (font units, 250
  per staff space), with a stated tolerance for the outlines' 4-decimal
  rounding. **Report the actual worst-case deviation.** *Mutation:* inflate
  one parsed coordinate past the declared box → dies. **If a real glyph fails,
  that is a finding to report — not a reason to widen the tolerance.** It
  would mean engraving reserves the wrong space for that glyph, the same bug
  class that twice bit the vertical metric.
* **(g5) `render-svg`'s bytes do not move** — *mutation:* emit the `d` from
  the typed form instead of the stored string → the goldens die.
* **(g6) absolute, not relative** — *mutation:* treat a `C` as relative →
  round-trip dies.

## Blast radius

New `crates/epiphany-glyphs/**` (including the moved
`outlines_generated.rs`, `tools/extract_bravura_outlines.py`, `tools/OFL.txt`,
and a `DECISIONS.md`); `crates/epiphany-render-svg/src/{lib.rs, outline.rs,
svg.rs}` and its `DECISIONS.md`; the workspace `Cargo.toml` member list.
Nothing else — no `layout-ir` edit (the seam is already there), no
`epiphany-core`, no `epiphany-ops`, no `epiphany-bundle`, no `.tex`, no CI
change (the MSRV job runs `--workspace --exclude epiphany-editor-gui`, so a
new crate joins automatically — which is exactly why it must be MSRV-clean and
dependency-free).

## Gate (actual output)

The standard six; conformance **9/9 with `--features golden-gate`, 8/8
without** (both run); requirement labels reported as observed. Plus the two
W1 byte locks, run as before/after comparisons against the base commit and
reported with their actual sizes:

* **Layout canonical bytes byte-identical** for the reference-suite fixtures
  plus `ten_measure_single_staff(0)` and `ten_measure_with_slurs(0)`. (W1's
  numbers, for reference: 21615 / 6948 / 3839 / 5602 / 2299 / 2299 / 21615 /
  22119.)
* **All five GUI goldens byte-identical.** Nothing here may change a pixel;
  no golden is re-blessed in this packet.
* **`cargo tree -p epiphany-glyphs` shows no third-party dependency.**

## Coordinator deliverable, alongside

`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md` — W3, the text-run primitive decision
(Ruling A criterion 3: shaping, fallback, bidi, and metrics consistent across
canvas, SVG/PDF export, hit testing, and the accessibility tree). A decision
document, not an implementation, written by Fable while W2 runs, because
getting it wrong forecloses the toolkit choice T4's spike exists to make
freely.

## Report

Per packet, as every tranche: files + summary, exact asserted values
(including g4's worst-case deviation), every mutation with kill evidence, gate
output, deviations flagged explicitly.
