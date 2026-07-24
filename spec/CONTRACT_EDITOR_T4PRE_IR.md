# Contract: Editor T4-pre — the layout-IR readiness tranche

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/PLAN_EDITOR_APP.md` §3.7 ("the layout-IR readiness tranche (T4
prerequisite)") and named as a prerequisite by Ruling A: *"IR per-system
primitive ownership; the shared typed glyph-asset seam; the text-run
primitive decision. These are IR/render tranches this ruling depends on, not
work it smuggles in."* T3 is complete at `85d8af6`.

Execution model as T1a/T2/T3: Sonnet subagents per packet, coordinator
line-level review with independent mutation re-runs, user deep-dives at
contract sign-off, any new golden baseline, and the final report. Mutation
discipline throughout: anchor-assert before substituting, restore by
reversing, never `git checkout`.

**Parallel safety.** The Push-4b track owns `crates/epiphany-core/**`, its
`DECISIONS.md`, and all `.tex` (`CONTRACT_PUSH4B_RESOLVER.md` blast radius);
it is live in the working tree. This tranche touches **no `epiphany-core`
file, no `.tex`, and adds no requirement label** — counts stay at whatever
the parallel track has them at; report observed actuals, never assert stale
numbers.

**The literal census, both structs.** W1 adds a field to `ResolvedSystem` as
well as to `ResolvedLayoutIR`, and Rust struct literals are exhaustive, so
every literal of both must be updated or the workspace does not compile.
Verified:

* `ResolvedLayoutIR {` — `layout-ir/src/{resolved.rs:405, solver.rs:518}`,
  `engrave/src/lib.rs:407`, `render-svg/src/svg.rs:1049`,
  `testkit/src/layout_stub.rs:436`.
* `ResolvedSystem {` — `layout-ir/src/solver.rs:501`,
  `engrave/src/casting.rs:1950`, `testkit/src/layout_stub.rs:461`, **and
  `editor-core/src/lib.rs:4718`** — the two-system hit-test fixture in
  editor-core's test module, a full four-field literal with no `..` spread,
  and `ResolvedSystem` cannot implement `Default` (it carries a
  `Provenance`).

So this tranche **does** reach one editor-crate file: that test literal, a
**mechanical field-add only**. `epiphany-editor-core` otherwise borrows
`&ResolvedLayoutIR` and mutates `.pages`; it **must not consume ownership** —
adoption stays at T4 per pin 8. Parallel safety is unaffected either way:
Push-4b owns `epiphany-core`, not the editor crate.

---

## W1 — per-system primitive ownership (dispatchable now)

### The verified starting point

Read these before designing; they change the shape of the work:

* **The partition already exists and is discarded.** Casting-off computes
  `system_of_slot`, `stroke_system`, `curve_system`, and `region_of_system`
  and returns them on `CastLayout` (`engrave/src/casting.rs:186-206`);
  `engrave/src/lib.rs:386-391` folds `cast.glyphs/strokes/curves/pages` into
  `ResolvedLayoutIR` and **drops every ownership vector on the floor**. W1 is
  "stop discarding it", not "infer it".
* **Glyph ownership is derivable exactly, and one consumer already derives
  it**: the quality census maps a glyph to a system through
  `system_of_slot[constrained_glyph.horizontal_slot]`
  (`engrave/src/quality.rs:232`), skipping slots no region claimed.
* **Cross-system primitives are already resolved at this stage.** A
  system-spanning stroke is replaced by its first segment plus synthesized
  continuation segments, and a system-spanning curve is split by de Casteljau
  subdivision, the continuations carrying `SYSTEM_CONTINUATION_SYNTHESIS`
  provenance (strokes `casting.rs:1051-1075`, curves
  `casting.rs:1128-1165`). So **every resolved primitive belongs
  to at most one system already**. Fact 2's warning that "cross-system curves
  and boundary-straddling primitives make inference ambiguous" is an argument
  against *spatial inference by the consumer*, not against publishing what
  casting-off knows.
* **`SystemId(pub u128)` (`layout-ir/src/cache.rs:10`) has no production
  constructor** — the single construction in the tree is testkit's random
  cache generator (`layout_stub.rs:630`, property-test fodder for the cache
  codec). Nothing maps a real system into
  `LayoutCache.resolved: BTreeMap<SystemId, …>`; it is inert scaffolding for
  T4b. W1 does **not** wire it; see the identity pin.

### Design pins

1. **Ownership lives on the system, not in a parallel table.**
   `ResolvedSystem` gains one field holding index lists into the layout's
   flat `glyphs`/`strokes`/`curves` arrays (`u32` indices — no layout nears
   4 G primitives, and it matches the crate's existing count width), and
   `ResolvedLayoutIR` gains one **unowned** bucket of the same shape.
   Structural ownership
   removes the "table order must match page order" invariant a top-level
   parallel table would create, and each system already carries its own
   identity in `provenance`.
2. **No primitive is split, merged, reordered, or renumbered.** The flat
   arrays are exactly what they are today, in exactly today's order. W1
   publishes indices into them and nothing else.
3. **Unowned is a first-class bucket, never coerced.** `None` — "claimed by
   no region" — goes to the unowned bucket. It has a real producer: the stub
   solver (`layout-ir/src/solver.rs:490-520`) builds a degenerate page tree
   of default-rect systems and resolves no per-system geometry, so **it
   publishes every primitive as unowned**. Fabricating an attribution there
   would be a lie about what that path computed.
4. **The partition is total and disjoint.** For each of the three arrays, the
   union of every system's indices plus the unowned bucket is exactly
   `0..len`, each index appearing exactly once. This is the load-bearing
   invariant; it is a test, not a comment.
5. **Identity is available, position is primary.** A consumer keys a system
   by its `provenance.stable_id` (`LayoutObjectId`); systems remain addressed
   positionally in page order. **Verified caution to record in DECISIONS.md,
   because it will bite T4b:** a region's *first* system reuses the region's
   own provenance verbatim (`casting.rs:1871`), and page 1 reuses the first
   region's provenance too (`casting.rs:1192`) — so a system's `stable_id`
   can equal a page's and a region's. On the **stub solver path the aliasing
   is total, not just first-system**: every stub system reuses its region's
   provenance (`solver.rs:501-502`), so the rule below has a producer on both
   solver paths. Uniqueness *among systems* is structural, not incidental —
   synthesized ids are domain-tagged hashes over (source, kind discriminant,
   namespaced instance key), and `KEY_NS_SYSTEM` and `KEY_NS_PAGE` occupy
   distinct namespaces — which is all this packet needs; **any future
   cross-kind map keyed on raw `LayoutObjectId`/`SystemId` u128s must
   disambiguate by kind.**
6. **Byte-neutral: ownership is NOT canonically encoded.** Precedent is in
   the same module: `vertical_band` is carried through but excluded, because
   "band ownership tells a vertical solver which staff owns a primitive; it
   draws nothing, so two layouts differing only in it are the same rendered
   layout and hash alike" (`resolved.rs:110-118`). System ownership draws
   nothing either. `encode_canonical` is therefore **unchanged**, and the
   module's canonical-serialization note gains ownership to its stated
   exclusions. If a future normative requirement wants ownership pinned on
   the wire, that is a spec-side schema-major decision — not this packet's
   budget to spend. Two consequences the **DECISIONS.md entry must state
   outright**, because they are what stop a later reader from "fixing" this:
   encoding ownership would make the fingerprint *more fragile than the
   rendering it fingerprints* — a casting-off refactor that re-partitions
   without moving a pixel would become a byte-level break and force a schema
   major for something no renderer and no conformance claim can observe; and
   because two conformant implementations may legitimately partition
   differently, **any future cross-implementation test of incremental
   relayout compares final bytes, never intermediate partitions.**
7. **One derivation, not two.** Casting-off publishes `glyph_system` on
   `CastLayout` (computed once, from `system_of_slot`) and the quality census
   **consumes it** instead of re-deriving. Two copies of an attribution rule
   drift; this is the packet that makes that impossible.
8. **One accessor.** `ResolvedLayoutIR` gains a `systems()` walk in page
   order (pages → systems), which the partition tests use. Editor-core
   hand-rolls this same flatten today (`editor-core/src/lib.rs:989`); it
   adopts the accessor at T4, not here.

### Tests + minimum mutations

Value-asserting throughout; the multi-system fixtures already exist
(`ten_measure_single_staff` casts to 2 systems at the demo's scale;
`ten_measure_with_slurs` is the casting fixture behind golden G4).

* **(m1) totality/disjointness** — the partition test of pin 4 on a real
  multi-system engrave. *Mutation:* drop the last glyph index from a system's
  list → dies.
* **(m2) no coercion** — the stub-solver path publishes everything unowned.
  *Mutation:* map `None` to system 0 → dies.
* **(m3) attribution correctness** — assert the *actual* per-system glyph /
  stroke / curve counts of a two-system fixture (real numbers, reported in
  the packet report). *Mutation:* off-by-one on the system index → dies.
* **(m4) the census really consumes the published vector** — *mutation:*
  publish `Some(0)` for every glyph while leaving `system_of_slot` correct →
  the existing quality-metric assertions die. If they survive, the census is
  still deriving its own answer and pin 7 is not done.
* **(m5) continuation segments follow the later system** — a slur or stroke
  crossing a system break: its continuation is owned by the system it was
  split *into*. *Mutation:* attribute continuations to the source segment's
  system → dies.
* **(m6) byte-neutrality is locked** — build a layout, clone it, perturb
  **only** the ownership lists, assert `canonical_bytes()` equal while
  `PartialEq` differs. *Mutation:* encode the ownership lists in
  `encode_canonical` → dies.

### Blast radius

`crates/epiphany-layout-ir/src/{resolved.rs, solver.rs, lib.rs}`,
`crates/epiphany-engrave/src/{casting.rs, lib.rs, quality.rs}`, the
test-only literals at `render-svg/src/svg.rs:1049`,
`testkit/src/layout_stub.rs:{436, 461}` and
`editor-core/src/lib.rs:4718` (mechanical field-add only — editor-core
consumes no ownership in this packet), and the `DECISIONS.md` of `layout-ir`
and `engrave`. Nothing else. No new crate, no new dependency, no public API
removed or renamed.

---

## W2 — the shared typed glyph-asset seam (contracted after W1 review)

Named now so the tranche's shape is visible; **not dispatched with W1**.

The Bravura table is private to `render-svg` (`outline()` is `pub(crate)`,
`outline.rs:7`) and stores SVG path `d` strings generated by
`tools/extract_bravura_outlines.py` (`outlines_generated.rs:1-20`). A canvas
tessellator needs typed vector paths from a crate both the renderer and the
app can depend on.

**The pin that makes it safe**, to be detailed when W1 lands: the generator
emits a **typed path representation alongside the existing `d` string**, and
`render-svg` keeps emitting the `d` string byte-for-byte — so every SVG
golden and every conformance byte stays identical while the canvas gets
typed geometry. The open decision W2's contract resolves is the home (a new
`epiphany-glyphs` crate vs. a `layout-ir` module), judged on dependency
weight and the MSRV/CI job structure, plus whether the typed form is
generated or parsed once at load.

## W3 — the text-run primitive decision (analysis packet, after W2)

Ruling A's criterion 3 makes the text pipeline a hard spike criterion:
shaping, fallback, bidi, and metrics consistent across canvas, SVG/PDF
export, hit testing, and the accessibility tree. W3 is a **decision
document**, not an implementation — what a text-run primitive is in the
resolved IR, and which of shaping/metrics the IR owns versus the renderer —
because getting this wrong forecloses the toolkit choice T4's spike is
supposed to make freely.

## Coordinator deliverable, alongside W1

`spec/ANALYSIS_GENESIS_PERSISTENCE.md` — the **field-by-field `Score` table**
Ruling B blocker (i) demands before it can be resolved, per the plan's T1b
runway. Analysis only: no code, no `.tex`, no core edits, therefore no
collision with the parallel track. Fable writes it while W1 runs; the user
deep-dives it as a coordination point.

---

## Gate (every packet, actual output)

The standard six; conformance **9/9 with `--features golden-gate`, 8/8
without** (both run); requirement labels reported as observed. Plus two locks
specific to this tranche, run as before/after comparisons and reported with
their actual digests:

* **Layout canonical bytes are byte-identical.** Capture
  `ResolvedLayoutIR::canonical_bytes()` for the reference-suite fixtures at
  the base commit, then after the change; they must be equal.
* **All five GUI goldens are byte-identical** (`ten_measure_open.png` 53638,
  `ten_measure_insert.png` 54590, `ten_measure_slurs_castoff.png` 57891,
  `ten_measure_caret_entry.png` 57379, plus G3's reuse of G1). No golden is
  re-blessed in this tranche — nothing here may change a pixel.

Blast radius per packet as stated. Do not commit.

## Report

Per packet, as every tranche: files + summary, exact asserted values
(including the real per-system primitive counts), every mutation with kill
evidence, gate output, deviations flagged explicitly.
