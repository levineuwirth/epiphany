# Text-run primitives in the resolved layout IR — analysis and recommendation

**Status:** analysis + recommendation, awaiting ruling.
**Charter:** `PLAN_EDITOR_APP.md` §3.7 (the layout-IR readiness tranche) and
§Ruling A criterion 3, the hard criterion — "shaping, font fallback,
bidi/complex scripts, and metrics consistent between interactive canvas,
SVG/PDF export, hit testing, and the accessibility tree. A stack with no
credible text story is disqualified regardless of vector performance."
**Companion to:** `CONTRACT_EDITOR_T4PRE_IR.md` (W1, landed `dd33b34`) and
`CONTRACT_EDITOR_T4PRE_W2_GLYPHS.md` (W2, landed `24f8c80`). This is W3, the
third and last T4 prerequisite — a decision document, not an implementation.
**Prepared against `main` @ `3b09595`.** Every claim below was checked against
the source; file and line are given for each.

> **Revision 2 (review response).** Draft 1 recommended disposition B —
> a text run carrying only its source string, shaped independently by each
> painter. Review found the argument for it rested on a layout guarantee the
> spec does not make, and that B permits precisely the inconsistency Ruling A's
> criterion 3 makes disqualifying. Both findings are correct and were verified
> against source. §2 is rewritten, **disposition E is added and recommended**,
> B is demoted with its defect stated, and F6's NFC claim is **withdrawn as
> false**. Draft 1's dispositions are retained rather than deleted so the
> rejected options stay on the record with their reasons.
>
> **Revision 3 (second review response).** Disposition E approved
> directionally; four specification-tightening findings incorporated before the
> `.tex` amendment. E's identity and segment types are now **fully specified**
> rather than opaque, with the invariants the amendment must state (§3E) — the
> governing test being that E's bytes must *actually* determine the ink, which
> revision 2's placeholder types did not achieve. The host-font rule is
> **narrowed** from "no host fonts" to "no ambient or unresolved lookup": an
> updated OS font is a changed *input* under
> `req:solver:within-implementation-byte-stability`, not a determinism
> violation, so revision 2's prohibition was broader than the spec supports and
> would have foreclosed imported faces (§2.1, §4.3). The reservation is
> **re-ordered** to derive from measured bounds rather than precede shaping
> (§3E cost 5). The cluster map gains its canonical invariants and a required
> Unicode segmentation version (§3E, §5.4). Two citation errors fixed.

---

## 0. What this decides, and what it does not

W3 decides **what shape a text run takes in the layout IR, and what part of it
enters the canonical bytes**. Those two answers are what a toolkit spike needs
in order to be conducted honestly, and they are what a wrong early convenience
would foreclose.

W3 does **not** decide the toolkit, the shaper library, the text-engraving
algorithms (title-block layout, lyric spacing, melisma extenders,
chord-symbol alignment), or the operation vocabulary that authors text. Those
are downstream and each is its own tranche.

---

## 1. The facts

### F1 — the model carries almost no text

§3.7 names five categories: "titles, lyrics, chord symbols, rehearsal marks,
instrument names." Three of the five carry **no text in the model at all**:

| category | model type | carries text? |
|---|---|---|
| titles / composer / subtitle / lyricist / arranger / copyright | `ScoreMetadata` (`graph.rs:1502`) | **yes** — six `Option<String>` |
| additional metadata | `MetadataEntry { key, MetadataValue::Text }` (`graph.rs:1494`, `1483`) | **yes** |
| instrument names | `Instrument { name, abbreviation }` (`graph.rs:1544`) | **yes** |
| staff names | `Staff { name, abbreviation }` (`graph.rs:813`) | **yes** |
| text-line spanners (`rit.`, `cresc.`) | `TextLineDefinition { text }` (`graph.rs:1034`) | **yes** |
| part / group / layer / view names | `PartDefinition`, `StaffGroup`, `AnalysisLayer`, `ViewDefinition` (`graph.rs:1612`–`1638`) | **yes** (non-score-ink) |
| **lyrics** | `LyricLine { id, events }` (`graph.rs:1326`) | **no** — no syllables |
| **chord symbols** | `ChordSymbol { id, anchor }` (`graph.rs:1334`) | **no** |
| **rehearsal marks** | `Marker { id, anchor }` (`graph.rs:1173`) | **no** label |
| analytical annotations | `AnalyticalAnnotation { id, anchor, layer }` (`graph.rs:1283`) | **no** |
| comments | `Comment { id, anchor, resolved }` (`graph.rs:1292`) | **no** body |

Nineteen lines mention `String` in the entire graph module, and none of them is
in a cross-cutting structure. `LyricLine`'s own doc comment says it plainly:
"Baseline: the event references it carries."

**Consequence.** The text-run primitive is not the binding constraint for
lyrics, chord symbols, or rehearsal marks — *their model is empty*, and filling
it is a schema major on the core track, not an IR tranche. What the primitive
*is* the binding constraint for is the text the model already holds: a title
block and instrument names down the left of the first system. That is a
smaller, more honest v1 than §3.7's list implies, and it is a real one.

### F2 — nothing engraves text today, and the spec says so

`epiphany-engrave` contains **zero** references to title, metadata, instrument
name, or lyric (verified by grep over the whole crate). The gap is admitted
normatively in three places:

* `req:layoutir:repeat-render` (`core_spec.tex:10256`): the jump kinds
  "draw no Minimal-tier marks — their segno / coda / instruction text **awaits
  a text primitive**".
* `logical.rs:179-180`: markers "need a text primitive and belong to a later
  tranche."
* `constrained.rs:1543`: volta ending numbers are set in time-signature digit
  glyphs because "the Minimal tier has no text primitive."

So this decision is *owed*; three landed tranches have deferred to it by name.

### F3 — the primitive vocabulary is normatively closed at three

`core_spec.tex:10111` lists `ResolvedLayoutIR` with exactly `glyphs`,
`strokes`, `curves`, and `req:layoutir:resolved-primitives` ratifies that
vocabulary ("Beyond glyphs … line-stroke primitives … and cubic-Bézier curve
primitives"). A fourth array is a **spec change**, on the parallel track's
`.tex`, not a code-only change. Whatever this document recommends, the `.tex`
edit is not mine to make.

### F4 — the exclusion precedent, and its exact wording

Two things are already excluded from the resolved layout's canonical bytes, on
one stated ground:

* `vertical_band` on strokes and curves — `req:layoutir:resolved-band-ownership`:
  "**it draws nothing**, so two layouts differing only in it are the same
  rendered layout."
* `PrimitiveIndices` (W1) — the same argument, recorded on
  `resolved.rs:91-94`.

Note the ground precisely. It is *not* "derived data may be excluded." It is
"non-drawing data may be excluded." That distinction is load-bearing in §3.

The positive form of the same rule is the one that decides this document.
`ResolvedLayoutIR::canonical_bytes` is defined as **the layout's rendering
fingerprint**: it "encodes what a conformant renderer draws and what a
conformance claim compares — every primitive's provenance, geometry, style, and
layer" (`resolved.rs:147-153`). Anything that determines ink belongs inside it.

### F5 — the identity precedent, which is the useful one

`GlyphCatalogIdentity` (`glyph.rs:93`) carries the SMuFL version, font id, font
version, and a BLAKE3 `metrics_hash` over every consulted glyph's metrics,
"required for any layout-conformance claim that depends on byte-equal output
across runs" (`glyph.rs:90`). The layout does not pretend its bytes are
font-independent; it **names the font inputs inside the bytes**, so the
byte-equality claim is conditional and honest.

The same discipline appears at the toolchain level in W2's neighbour:
`font_subset_generated.rs:13-15` records that "the binary subset's exact bytes
depend on the fontTools version recorded here, so regeneration is reproducible
per version."

This is the model a text-shaping identity follows — **and must exceed in one
respect**. `metrics_hash` covers "every consulted glyph's metrics (bounding box,
advance width, and named anchors)" (`glyph.rs:9-12`): it pins *spacing*, not
*shape*. That is sufficient for music glyphs, whose outlines are separately
locked by W2's byte-exact round trip and the golden PNGs. It is **not**
sufficient for a text face, whose outlines are what the run's ink is. A text
face identity therefore hashes the **font file's bytes**, not its metrics; and
the pinned source SHA recorded in a generated file's header
(`font_subset_generated.rs:12`) does not substitute, because it does not enter
the layout bytes. `req:solver:within-implementation-byte-stability` anticipates
this precisely — its identical-inputs list says font metrics are "referenced by
version **and content hash**" (`core_spec.tex:13115-13116`).

### F6 — NFC is **not** guaranteed for score text (draft 1's claim withdrawn)

Draft 1 asserted that all score text arriving through operations inherits an
NFC guarantee. **That is false**, and the correction matters because the
primitive's `text` field cannot be documented as "NFC, verbatim from the graph."

* `req:determinism:unicode-canonicalization` (`core_spec.tex:15164`): canonical
  text fields MUST be UTF-8 in NFC; identity comparison MUST be byte comparison
  of NFC UTF-8; locale MUST NOT affect parsing, serialization, or key sorting.
* The envelope's own string reader does enforce it — `encode.rs:21` normalizes
  before length-prefixing and `envdecode.rs:185-190` rejects with
  `EnvelopeDecodeError::NotNfc` — **but that path covers only strings the
  envelope encodes directly**, and `encode.rs:21` names the one it means: "The
  single text field (a transaction label)".
* Every score-bearing payload instead embeds the **core codec's** bytes
  wholesale: `SetMetadataOp` pushes `self.metadata.canonical_bytes()`
  (`payload.rs:1360`), `CreateStaffOp` the staff's (`payload.rs:1438`),
  `CreateInstrumentOp` the instrument's (`payload.rs:1467`), and the
  cross-cutting values the spanner's (`payload.rs:831`).
* The core codec **deliberately preserves non-NFC strings**: "Strings are
  length-prefixed UTF-8 and are **not** NFC-folded here: the graph's `Score`
  equality is byte-exact on its `String` fields, so the codec preserves them
  exactly" (`codec.rs:24-27`).

So operation-authored score text can be non-NFC today, and the graph codec will
faithfully round-trip it. This is a **live compliance gap against
`req:determinism:unicode-canonicalization`, independent of this tranche** — it
exists whether or not a text primitive lands. The text primitive merely makes
it visible, because a run's string would flow from the graph into the rendering
fingerprint.

**W3's requirement:** the text primitive MUST NOT document its `text` field as
NFC unless something enforces it. Either the graph/payload boundary validates
(rejecting non-NFC score text at authoring, matching the envelope's own
discipline) or the projection into `ResolvedText` rejects. This is a finding
for the core track, and it is named here so its absence is a decision.

### F7 — blast radius, measured

| surface | sites | note |
|---|---|---|
| `.curves` references (proxy for "handles all three arrays") | layout-ir 26, engrave 21, render-svg 6, testkit 3 | a fourth array touches each |
| `PrimitiveRef::` matches | 19, across 3 files (`hittest.rs`, `engrave/lib.rs`, `testkit/editloop.rs`) | a fourth variant is exhaustive-match work |
| `epiphany-editor-core` | **0** `.curves`, 4 `.glyphs`, 1 `hit_test_map` | editor-core consumes the hit-test map, not the arrays |

The last row is the good news and should be stated carefully: editor-core is
nearly insulated, because W1's seam and the hit-test map already stand between
it and the primitive arrays. The genesis track's own lesson cuts the other way
and is worth quoting — adding an `OperationKind` variant proved *not*
containable to core+ops, with four downstream literal sites and editor-core
blocking the whole gate. A fourth primitive array should be assumed to behave
the same until a census says otherwise.

### F8 — SVG has a font-**embedding** precedent, not a text pipeline

Draft 1 overstated this. What exists: `render-svg` emits music glyphs either as
inline outline `<path>` or as `<text>` referencing an `@font-face`-embedded
Bravura **subset**, base64 in a data URI (`svg.rs:311-317`, `svg.rs:438`).

What that machinery actually does is narrower than "text export works":

* the subset is **static and build-time**, generated once by
  `tools/extract_bravura_outlines.py` (`font_subset_generated.rs:1`), not
  produced per document from the glyphs a score uses;
* each `<text>` element carries **one already-positioned SMuFL codepoint**
  (`svg.rs:438`) — the position comes from the resolved layout, not from
  shaping a string.

So the `@font-face` embedding mechanism is genuinely reusable, and the license
handling (Reserved Font Name, pinned source SHA, per-version reproducibility) is
a template worth copying exactly. **Shaping and document-specific subsetting do
not exist and are new work.**

### F9 — what the spec actually guarantees about layout determinism

This is the fact draft 1 got wrong, and it inverts the argument.

The determinism summary table (`core_spec.tex:613-631`) distinguishes tiers:

| tier | guarantee |
|---|---|
| Canonical score determinism | "Identical operation sets produce identical materialized score states **across all conforming implementations**" |
| **Layout determinism** | "**Within one solver implementation at a fixed version: byte-equal output** for identical input. **Across implementations: reference-suite thresholds, not byte equal.**" |
| Conformance determinism | "Reference-suite-based. **Cross-implementation byte equality is not required for layout.**" |
| Non-canonical caches | "May vary freely across runs, platforms, implementations." |

`req:solver:cross-implementation-conformance` (`core_spec.tex:13142`) says it
normatively: different conforming solvers MAY produce different layouts;
conformance is hard-constraint satisfaction, internal determinism,
well-formed `SolveReport`, and per-tier quality thresholds on the reference
suite.

**Therefore shaping before the canonical boundary is not inherently
non-conformant.** A fixed solver at a fixed version, with pinned font inputs
and a declared shaping identity, is exactly what "within one implementation at
a fixed version" contemplates. Draft 1 imported the *score* layer's
cross-implementation guarantee into the *layout* layer, where the spec
deliberately weakens it, and then designed around a constraint that does not
exist.

---

## 2. The actual problem

With F9 corrected, the problem is not "shaping poisons a cross-implementation
byte-equality claim." It is three narrower things, and they pull in the
opposite direction from draft 1:

1. **Within-implementation determinism is a real requirement, and *ambient*
   font lookup defeats it.** `req:solver:within-implementation-byte-stability`
   (`core_spec.tex:13106-13124`) obliges a solver at a fixed version to produce
   byte-identical output for identical inputs — and it *defines* identical
   inputs to include "identical font and glyph metrics (**referenced by version
   and content hash**)" (`core_spec.tex:13115-13116`).

   Note what that does and does not say, because revision 2 also overstated it.
   An OS font update is a **changed input**, not a determinism violation — the
   obligation is conditioned on identical font inputs, so a solver whose font
   changed is not obliged to reproduce its old output. Host fonts are therefore
   not inherently disqualifying. What *is* disqualifying is **ambient or
   unresolved lookup**: a chain that names "whatever the system calls Helvetica"
   has no content hash, so the input is unnamed, the layout is unportable, and
   no consumer other than the one machine can reproduce the ink. The rule to
   write is narrower than "no host fonts" (§4.3).
2. **Criterion 3 requires *consistency across the four consumers*, and only a
   shared shaping result delivers it.** Canvas, SVG/PDF export, hit testing,
   and the accessibility tree must agree on metrics. If each consumer shapes
   independently they agree only by coincidence of library and version.
3. **The fingerprint must mean what it says.** `canonical_bytes` "encodes what
   a conformant renderer draws" (`resolved.rs:147-153`, F4). Any design where
   two layouts with identical bytes legitimately produce different ink breaks
   that definition — and the W1/`vertical_band` exclusion rule, whose whole
   justification is the contrapositive, loses its footing.

Accessibility adds a fourth, orthogonal requirement: the **source string** must
survive into the IR, because a screen reader needs text, not outlines.

The design question is therefore not "shaped or unshaped" but **"how do we
carry both the shaped result and the source, with the shaping inputs declared."**

---

## 3. Dispositions

### A — no text primitive; text is anonymous glyph runs

Reuse `ResolvedGlyph`, one per shaped glyph, with `GlyphReference` naming a
text font's glyph. The engraver shapes; the IR carries only the result.

* **Cost 0 (structural):** nothing new — no fourth array, no `PrimitiveRef`
  variant, and `GlyphReference` is already an open `Cow<'static, str>`
  (`glyph.rs:55`), so it can name non-SMuFL glyphs today.
* **Cost 1 (fails criterion 4, fatal):** the source string is gone, so the
  accessibility tree has no text to expose. A title rendered as eleven
  anonymous outlines is invisible to a screen reader, and criterion 4
  explicitly rejects "toolkit carries a tree while the canvas is unusable."
* **Cost 2:** no caret, no character selection, no find-in-score — hit testing
  yields glyph boxes with no relation to source offsets.
* **Cost 3:** one `Provenance` per glyph; a 40-character title costs 40
  provenance records where one run would cost one.

**Refused.** Not because shaping is early — F9 says early shaping is legitimate
— but because discarding the source is unrecoverable.

### B — a text run carrying only the source string; each painter shapes

`ResolvedText { provenance, text, font, size, origin, align, reserved_box,
shaping_context, layer }`, with no shaped output. Canvas, exporter, and a11y
tree each shape independently.

**This was draft 1's recommendation. It is withdrawn.** Its defect is §2.2 and
§2.3 together: it *knowingly* permits canvas and `resvg` to place glyphs
differently, which is not what Ruling A's tolerance grants. That tolerance
covers **rasterization** differences downstream of equivalent scene geometry —
antialiasing, curve flattening — not different advances, different clusters,
different fallback faces, or different line geometry. Criterion 3 separately and
explicitly demands consistent *metrics*. And under B, two layouts with identical
canonical bytes can legitimately produce different ink, which contradicts the
fingerprint's own definition.

Two further defects surfaced in review:

* **A `TextShapingIdentity` is required under B too**, not only under C.
  Reporting a host fallback substitution tells a user something changed; it does
  not make the output reproducible or make two consumers agree.
* **One font and one `(direction, script, language)` context cannot represent
  the spike's own cases** (§5). Mixed Arabic/Latin requires itemization into
  multiple directional and script runs; fallback produces segments in multiple
  faces; feature selection is named in §2 as a shaping input but appears in no
  field; and automatic itemization needs a declared Unicode/bidi algorithm
  version. "The painter must not re-guess" and "one mixed-direction run" cannot
  both hold with that shape.

### C — B plus a shaped cache excluded from the canonical bytes

Retained for the record; superseded by E. Its distinctive move was to exclude
derived-but-drawing data from the fingerprint, which F4's precedent does not
license ("non-drawing", not "derived"). E resolves this by **including** the
shaped result instead, which needs no new exclusion doctrine at all.

### D — defer; the spike uses the toolkit's own text and the IR gets nothing

* Criterion 3 is a *hard* criterion. A spike that never renders score text
  cannot disqualify a stack for having no text story — the one thing criterion
  3 exists to do.
* It is the foreclosure Ruling A was written to prevent: pick a stack on vector
  performance, discover its text pipeline cannot serve export and the a11y
  tree, and the ruling has failed silently.

### E — the shaped run: source string **and** canonical positioned glyphs

The disposition draft 1 omitted. One primitive carrying both halves:

```rust
pub struct ResolvedText {
    pub provenance: Provenance,

    /// The source string, carried verbatim for accessibility, search,
    /// and editing. NOT documented as NFC until F6's gap is closed.
    pub text: String,

    /// Every shaping input, declared — the `GlyphCatalogIdentity`
    /// discipline (F5) applied to text.
    pub shaping: TextShapingIdentity,

    /// The shaped result: the definitive geometry every consumer draws.
    pub segments: Vec<ShapedSegment>,

    /// Source offsets to glyph clusters and caret positions, so hit
    /// testing and a caret index into the string rather than the ink.
    pub clusters: ClusterMap,

    /// What shaping actually produced, and what the solver allocated.
    /// `reserved_box` is a solver *policy* over `bounds` (padding, a
    /// minimum allocation) — never an unshaped estimate. See below.
    pub bounds: BoundingBox,
    pub reserved_box: BoundingBox,

    pub origin: Point,
    pub align: TextAlign,
    pub style: GlyphStyle,
    pub layer: i32,
}

/// Every input that determines the ink, named so the fingerprint means
/// what it says. Modelled on `GlyphCatalogIdentity` (F5) but *stronger*:
/// that type hashes metrics, and metrics do not determine outlines.
pub struct TextShapingIdentity {
    /// The ordered chain, tried in order. Resolution is closed over this
    /// list: a codepoint no listed face covers is a reported failure,
    /// never an ambient host lookup (§4.3).
    pub faces: Vec<TextFaceIdentity>,
    /// Shaper implementation and version — it moves glyphs, so it is an
    /// input on exactly the footing of the font version.
    pub shaper: ShaperId,
    pub shaper_version: SemVer,
    /// The OpenType feature set applied, in canonical order.
    pub features: Vec<FeatureSetting>,
    /// The Unicode version whose bidi algorithm itemized the run, and
    /// whose grapheme segmentation defines the caret stops. Present
    /// whether or not itemization was automatic: segmentation is
    /// version-dependent even when direction is declared by hand.
    pub unicode_version: UnicodeVersion,
}

pub struct TextFaceIdentity {
    /// Human-facing name — diagnostic only, never an input.
    pub family: FontId,
    pub version: Option<SemVer>,
    /// **The identity that matters**: a content hash over the exact font
    /// file's bytes. A family/version pair does not pin outlines, and a
    /// SHA recorded in a build script does not enter these bytes.
    pub file_hash: [u8; 32],
    /// Which face within a collection (`.ttc`/`.otc`).
    pub face_index: u32,
    /// Variable-font axis coordinates, canonical-ordered by axis tag;
    /// empty for a static face.
    pub variations: Vec<(AxisTag, CanonicalF64)>,
    /// Synthetic weight/slant applied when the face lacks the style —
    /// it changes the ink, so it is not a renderer's private choice.
    pub synthesis: FaceSynthesis,
}

pub struct ShapedSegment {
    /// Which face in `TextShapingIdentity::faces` this segment resolved
    /// to — an index, so the identity is stated once.
    pub face: u32,
    /// The glyph ids are **font-internal ids in that face's namespace**,
    /// meaningless without `TextFaceIdentity::file_hash`.
    pub glyphs: Vec<PositionedGlyph>,
    /// The half-open source range this segment covers, in UTF-8 byte
    /// offsets into `ResolvedText::text`.
    pub source: Range<u32>,
    pub direction: TextDirection,
    pub script: ScriptTag,
    pub language: LanguageTag,
    /// Em size in staff spaces. Positions below are in the same
    /// staff-space, y-up frame as every other resolved primitive, and
    /// quantize to the 1/1024 grid identically.
    pub size: StaffSpace,
}

pub struct PositionedGlyph {
    pub glyph_id: u32,
    /// Offset from the run's `origin`, **with alignment already applied**
    /// — a consumer places the run by `origin` alone and never re-derives
    /// from `align`, which is retained as a record of the decision.
    pub offset: Point,
    pub transform: Option<Transform2D>,
}
```

**Invariants the `.tex` amendment must state, not merely imply:**

* Every offset in `ClusterMap` and every `ShapedSegment::source` bound is a
  **valid UTF-8 boundary** in `text`.
* Segment source ranges **cover the whole string** and do not overlap
  invalidly — visual order may differ from logical order under bidi, but the
  logical partition is total.
* `ClusterMap` carries, per cluster: its source range, its glyph indices, and
  its caret stops — each stop with a **geometric position and a bidi affinity**,
  so a caret at a direction boundary is unambiguous.
* A cluster that shaping could not resolve is represented **diagnostically**
  (an explicit unresolved marker), never dropped — a dropped cluster is a
  silent divergence between the string and the ink.
* Positions are staff-space y-up, quantized on the same 1/1024 grid as glyph
  positions (`resolved.rs:11-13`), so text quantization is not a second
  convention.

**The positioned result enters the canonical bytes.** Canvas, SVG/PDF export,
and hit testing consume that one result; accessibility consumes the source
string. Tessellated meshes remain the genuinely discardable paint cache — the
thing that legitimately varies per platform under the determinism table's
"non-canonical caches … may vary freely across runs, platforms,
implementations" (`core_spec.tex:629-631`).

**No consumer reshapes — and SVG export cannot honour that with `<text>`.**
This is a concrete consequence worth stating before the `.tex` amendment,
because it constrains the exporter. `ShapedSegment` carries font-internal glyph
ids, and an ordinary SVG `<text>` element cannot request one: it carries
*characters*, and the viewer's own shaper picks the glyphs. For anything
contextual — a ligature, a positional Arabic form, a `locl` substitution —
`<text>` will silently draw a different glyph than the one the layout resolved.
So conformant text export must use **explicit-glyph representation**: outlines
looked up from the same face (the `epiphany-glyphs` seam W2 built, extended to
text faces) and emitted as `<path>`, exactly as `GlyphMode::PathOutline` already
does for music glyphs. The existing `@font-face` mode remains available only
where the run is provably non-contextual, and that is an optimization, not the
contract. PDF is easier — it addresses glyphs by id natively — but the rule is
the same: draw the resolved glyphs, never the string.

Why this is the right shape:

* **It matches the existing resolved-stage contract.** The resolved IR is where
  "every glyph has a definitive position" (`resolved.rs:3`). Text with
  undetermined positions would be the one primitive that isn't resolved at the
  resolved stage.
* **It satisfies criterion 3 by construction**, rather than by hoping two
  shapers agree.
* **It keeps the fingerprint honest** in the direction the fingerprint is
  actually defined: bytes determine ink.
* **It preserves accessibility**, which is exactly what disposition A threw
  away — so the apparent dilemma between deterministic geometry and
  accessibility was false, and it was false because of §2's corrected framing.
* **It needs no new exclusion doctrine**, unlike C.

Costs, honestly:

* **Cost 1:** the engraver gains a shaper dependency. That is real weight in a
  crate that currently has none, and it must be a pinned, vendored-or-locked
  version because `req:solver:within-implementation-byte-stability` makes the
  solver's determinism an obligation at fixed version.
* **Cost 2:** F7's blast radius in full — fourth array, fourth `PrimitiveRef`
  variant, 19 exhaustive matches, plus the `.tex` amendment (F3).
* **Cost 3:** the canonical encoding grows a substantially richer primitive
  than a glyph, and every field of it is fingerprint-visible. A shaper upgrade
  moves the bytes — which is correct behaviour (it moves the ink) but means the
  shaper version is now a conformance-relevant input, exactly as the font
  version already is (F5).
* **Cost 4:** a text-font asset decision, with the full Bravura treatment
  (pinned source, SHA, license, RFN handling, subsetting-tool version).
* **Cost 5 — and a pipeline-ordering consequence revision 2 got wrong.**
  Draft 1 and revision 2 both had the solver *reserve space before shaping* and
  report overflow afterwards. That was a workaround for B's missing shaper, and
  it does not survive E: once a canonical shaper lives inside the engraving
  pipeline, **shaping and itemization run before the spacing constraints that
  depend on text extents are solved**. `reserved_box` therefore stops being an
  unshaped estimate and becomes a **solver policy over `bounds`** — padding, a
  minimum allocation, a column quantum — derived from measurement, not guessing
  at it. Both are kept because the policy is a real decision worth recording and
  worth changing independently. Paint-time re-spacing remains forbidden either
  way: a painter draws what `segments` says.

  The residual cost is ordering pressure on the engraver: text extents now
  participate in horizontal spacing, so the pipeline gains a shaping pass ahead
  of the constraint solve. That is the honest price of consistent metrics, and
  it is smaller than it looks because v1's text (titles, instrument names) sits
  outside the note-spacing problem.

---

## 4. Recommendation

**Take E. Refuse A explicitly. B, C, and D are recorded as considered and
rejected for the reasons above.**

Concretely:

1. **`ResolvedText` as in §3E** — source string, shaping identity, itemized
   positioned segments, cluster/caret map, measured bounds and reservation — as
   a fourth primitive array and a fourth `PrimitiveRef` variant, with the
   `.tex` amendment to `req:layoutir:resolved-primitives` and the
   §"ResolvedLayoutIR" listing (parallel-track work; this document is the
   request).

2. **The canonical rule:** *a text run's shaped result is canonical layout
   geometry and enters the rendering fingerprint, together with the complete
   shaping identity that produced it. The source string is carried alongside it,
   never in place of it.* Draft 1's proposed rule — that any pre-canonical
   shaping is non-conformant — is **withdrawn**; F9 shows the spec does not
   support it.

3. **No ambient or unresolved host-font lookup.** The rule, in the form the
   `.tex` should carry it:

   > A host font MAY participate in a conformant fallback chain **only after
   > resolution to an exact, content-hashed asset that every consumer can
   > obtain**, named in the run's `TextShapingIdentity`. Ambient lookup — any
   > chain entry naming a face the layout does not pin by content hash — is
   > non-conformant. A codepoint no resolved face covers is a **reported**
   > failure, never a silent substitution.

   This supersedes both draft 1's blanket prohibition on pre-canonical shaping
   *and* revision 2's blanket prohibition on host fonts. It protects
   portability, export consistency, and the goldens, without foreclosing
   imported or user-supplied faces — which a notation product that opens other
   people's documents will need. What it costs: a document that resolves a
   user's local face is only portable if that asset travels with it, which
   makes font embedding in the bundle a real downstream question. Named here,
   not answered.

4. **The shaped result is drawn, never re-derived.** No consumer reshapes, and
   SVG export therefore emits explicit glyphs (outlines from the same face),
   not `<text>` — see §3E. This is the rule that makes criterion 3's
   "consistent metrics" mean something operational.

5. **v1 scope: the text that exists.** Instrument/staff names at system left,
   the title block from `ScoreMetadata`, and `TextLineDefinition.text` on
   spanners — model-backed, Latin, one pinned face. Lyrics, chord symbols,
   rehearsal marks, analytical annotations, and comments are **blocked on the
   model, not on this decision** (F1). Bidi and fallback are exercised in the
   spike through **synthetic `ResolvedText` fixtures**, which need no model work
   and no schema major.

6. **Design for itemization from day one even though v1 populates one segment.**
   `segments` is a `Vec` and `TextShapingIdentity` carries an ordered chain and
   a feature set from the first commit. This is the foreclosure guard for this
   tranche: retrofitting itemization means touching every producer and consumer
   simultaneously, and it is nearly free to honour now.

7. **Name F6's NFC gap as core-track work.** The text primitive must not
   document its `text` field as NFC until the graph/payload boundary validates
   or projection rejects.

---

## 5. What the T4 spike must then demonstrate (criterion 3, made testable)

Under E the spike's job changes: the shaping happens in the engraver, so the
spike is testing whether a candidate stack can **faithfully consume a shaped
result** — a materially easier and more decisive test than asking each stack to
shape correctly.

1. **Faithful consumption:** the candidate canvas draws a `ResolvedText`'s
   positioned segments at the given positions without re-shaping, and matches
   the SVG exporter's rendering of the same run under Ruling A's bounded visual
   differential — which is now a *rasterization* comparison of identical
   geometry, which is what that tolerance was always for.
2. **Fallback, forced:** a run whose string needs a face beyond the first in the
   declared chain resolves through the **declared chain only**, and a codepoint
   no declared face covers is reported, not silently substituted from the host.
3. **Bidi:** a mixed Arabic/Latin run itemizes into multiple directional
   segments, each drawn in its resolved face at its resolved position. Synthetic
   fixture; no model work required.
4. **Hit testing at character granularity, against the stated index contract.**
   Click inside a run and recover a source index. The contract, now pinned:
   **UTF-8 byte offsets** are the base index (they address the stored `String`
   directly), **caret stops are grapheme-cluster boundaries carrying bidi
   affinity**, and the **Unicode version that defines that segmentation is part
   of `TextShapingIdentity`** — always, not only when itemization is automatic,
   because grapheme boundaries move between Unicode versions even for a run
   whose direction was declared by hand. Without that, two implementations could
   agree on every pixel of ink and still produce different canonical
   `ClusterMap`s, which would put a divergence inside the fingerprint that no
   visual test can see. The current map has one geometric region per primitive
   and no cluster data (`hittest.rs:303-310`), so this is new surface either
   way.
5. **Accessibility:** the run appears in the accessibility tree as its **source
   string**, not as a graphic — the check disposition A fails outright.

A stack that cannot do 2 and 5 is disqualified regardless of tessellation
throughput.

---

## 6. The four open rulings — answered

Recorded from review; no longer open.

1. **The `.tex` amendment proceeds now**, as a non-document-canonical layout-IR
   addition. It changes the layout fingerprint but requires no bundle/wire
   schema-major move — it follows strokes and curves, which
   `req:layoutir:resolved-primitives` already added on those terms.
2. **Every bundled text face gets the full treatment** — pinned source, hash,
   license, Reserved Font Name handling, version, subsetting-tool version. **Do
   not select a face until the ordered fallback-chain representation is
   settled**, since the chain is what the face must slot into.
3. **Accept text-bearing golden exposure, and include at least one controlled
   text golden.** Keeping all text out of the goldens would remove precisely the
   regression tripwire the goldens exist to provide. The house rule stands: a
   moved text golden is re-reviewed visually, never re-blessed to pass.
4. **Lyrics do not jump the queue.** Title and instrument text give real
   model-backed coverage; bidi and fallback are covered by synthetic
   `ResolvedText` fixtures during the spike.
