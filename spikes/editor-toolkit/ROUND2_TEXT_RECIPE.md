# Round 2 text recipe — the precommitted stand-in, fixtures, and differential

Governed by `spec/CONTRACT_EDITOR_T4_SPIKE.md` pins 8, 9, 10, 13, 14 and
`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md` (W3) §3E and §5. This is **Packet 2A**:
everything candidate-neutral, committed and user-reviewed **before either
candidate consumes it**, under the same rule as the Round 1 oracle.

Nothing here is a recommendation to the core track. W3 §4 already took
disposition E; the spike is that shape's **first consumer**, and pin 8 makes
every place the shape proves awkward to consume a finding routed back to the
`.tex` amendment. Those findings are §12.

---

## 1. Why the faces are what they are

Pin 9: faces resolve **once at startup from an explicit path list, with their
bytes hashed**, committing no font binary. Absent face ⇒ `NOT RUN` (pin 14).

The declared chain, in order:

| # | Path | family (name id 1) | version (name id 5) | upem | sha256 | bytes |
|---|---|---|---|---|---|---|
| 0 | `/usr/share/fonts/tex-gyre/texgyrepagella-regular.otf` | TeX Gyre Pagella | `Version 2.501;PS 2.501;ffdkm 0.1` | 1000 | `44e64260716d8f2bbe412baa1ee99b7c995190ac4573177c24def0b9200438c7` | 218100 |
| 1 | `/usr/share/fonts/liberation-fonts/LiberationSerif-Regular.ttf` | Liberation Serif | `Version 2.1.5` | 2048 | `058ea80864aef09a23f45cbec2bb5400bc3dfbdea01c3f10538a21fcb497fb74` | 393576 |

The hashes above are recorded **as observed on this machine on 2026-07-29**.
The generator recomputes them and **fails loudly** on any mismatch rather than
regenerating: under pin 9 the content hash *is* the identity, so a changed file
is a changed fixture set, not a detail to absorb.

**This pair was chosen to make check 2 real rather than nominal.** Face 0
covers Latin and the combining acute but **not** Hebrew (measured, §4); face 1
covers both. So:

* a Latin-only run resolves entirely within face 0 — no fallback exercised;
* a Hebrew run **must** traverse to face 1, and doing so is observable in
  `ShapedSegment::face`;
* U+0627 ARABIC LETTER ALEF is covered by **neither** — verified `None` in both
  faces — so it is the uncovered codepoint check 2 requires, and it is
  uncovered *by fact*, not by a chain artificially truncated to manufacture the
  test.

**The two faces disagree on units-per-em (1000 vs 2048).** This is deliberate,
not incidental: a fallback chain whose faces share a upem would let a
consumer that forgets to normalize per face pass anyway. Every position this
recipe records is in staff-space, already divided by the *originating face's*
upem, so a consumer that reads `size` and multiplies without consulting the
face will land visibly wrong on the Hebrew segment and nowhere else.

### 1.1 Deviation, named: there is no Arabic-capable face on this machine

W3 §5 check 3 says "a mixed **Arabic**/Latin run". `fc-list :charset=0627`
returns nothing, and the 168 installed faces contain no Arabic coverage at all
(nor CJK). Under pin 9 that is an environment absence ⇒ `NOT RUN`.

Recording check 3 as `NOT RUN` would lose the only bidi evidence the round can
produce, so this recipe supplies **F-D, a Hebrew/Latin bidi fixture**, as an
explicitly named substitution with its coverage gap stated:

* **What it still tests:** the property check 3 actually names — that a mixed
  run *itemizes into multiple directional segments, each drawn in its resolved
  face at its resolved position*. Hebrew is strong RTL; the run itemizes into
  three visual runs at levels 0/1/0 (measured, §4), the RTL segment resolves to
  a different face than the LTR segments, and its glyph clusters run in
  descending source order.
* **What it does not test:** Arabic is *cursive-joining*. Its positional forms
  (initial/medial/final/isolated) come from contextual GSUB, so an Arabic
  fixture would additionally prove that a consumer draws the **resolved** glyph
  ids rather than re-deriving them. Hebrew has no joining behaviour, so F-D
  cannot catch a consumer that re-shapes and happens to agree.

  **That gap is covered elsewhere, but only partly:** F-A carries two real
  ligature clusters (`ff` and `fi` → one glyph each, measured), which a
  re-shaping consumer with different feature settings would get wrong. It is
  weaker than positional forms — a re-shaper using the same font and default
  features reproduces `liga` — but it is not nothing.

### 1.2 RULED (2026-07-29): check 3 is `NOT RUN`

The user's ruling, recorded verbatim in substance:

> Check 3 is **NOT RUN**: the contract explicitly requires Arabic/Latin, and
> pin 9 defines an absent required face as environmental `NOT RUN`. Record F-D
> separately as:
>
> > **Supplementary PASS** — Hebrew/Latin bidi itemization, fallback-face
> > selection, visual ordering, and resolved positioning.
>
> It must not upgrade check 3 to PASS; it cannot exercise contextual Arabic
> joining. If checks 1, 2, 4 and 5 pass, the Round 2 criterion cell is
> therefore `NOT RUN`, but **eligibility is unaffected** because check 3 is not
> disqualifying.

Consequences, so no later packet has to re-derive them:

* The Round 2 criterion cell for check 3 reads `NOT RUN` for **every**
  candidate, on both adapters. It is an environment fact, not a candidate
  outcome, and it is identical for C1 and C2 — so it separates nothing and
  decides nothing.
* F-D is still built, still rendered, still diffed, and still reported — as
  **Supplementary PASS/FAIL**, on its own row, never merged into check 3's.
* A candidate that fails F-D has failed the supplementary row. That is
  reportable evidence for the ruling, and it is not a check-3 FAIL.
* Check 3 is **not** in the disqualifying set (checks 2 and 5 are), so a
  `NOT RUN` cell cannot end a candidacy.

The pre-ruling text above is kept as written because it is the reasoning the
ruling answered, not because it is still open.

---

## 2. The fixture set

Committed **verbatim**, as Rust string literals with every non-ASCII codepoint
escaped, so the file is unambiguous under any editor or normalization:

| id | scored as | literal |
|---|---|---|
| **F-A** | check 1 faithful consumption, check 5 accessibility | `"Allegro affettuoso \u{2014} al fine"` |
| **F-B** | check 2 fallback, forced | `"Coro \u{05D0}\u{05D1}\u{05D2}"` |
| **F-C** | check 2 uncovered codepoint | `"Coro \u{0627}"` |
| **F-D** | **Supplementary** bidi evidence (Hebrew/Latin) — **check 3 remains `NOT RUN`** (§1.2) | `"Allegro \u{05D0}\u{05D1}\u{05D2} con brio"` |
| **F-E** | check 4 hit testing / caret | `"Cafe\u{301} \u{2014} resume\u{301}"` |

Plain-text rendering, for reading: `Allegro affettuoso — al fine` ·
`Coro אבג` · `Coro ا` · `Allegro אבג con brio` · `Café — resumé`.

**The F-D row is a scoring classification, not a description, and it is
enforced.** The column header used to read "purpose (W3 §5 check)" and F-D's
cell "3 bidi", which contradicts the 2026-07-29 ruling wherever that label
is printed. The label lives in exactly one place in code
(`fixtures::FIXTURES[3].purpose`), flows from there into `fixtures.json`,
`FIXTURES_SUMMARY.md` and every generator's console output, and is restated as
a literal in `FixtureFile::validate` (`EXPECTED_PURPOSES`) so that restoring
`"check 3 (bidi)"` is a named validation failure rather than a stale string
someone reads in good faith. A ruling recorded only in prose, while the
machine-readable artifact still says "check 3", is a ruling that loses to
whichever record the reader happens to open.

**F-E is deliberately NFD, not NFC.** W6 withdrew the claim that score text is
NFC (`ANALYSIS_TEXT_RUN_PRIMITIVES.md` F6), so a fixture that quietly assumed
composed input would test a guarantee the model does not make. `e` + U+0301
also produces the case that matters: the shaper *composes* it to one glyph
(measured: gid 198), while segmentation reports **one grapheme spanning three
UTF-8 bytes**. One caret stop, two codepoints, one glyph — the exact place a
codepoint-indexed caret and a grapheme-indexed caret diverge.

**F-A carries two ligatures**, which is the same divergence from the other
direction: `ff` and `fi` each shape to a single glyph covering two codepoints,
so a caret between `f` and `f` has a grapheme boundary but **no glyph
boundary**, and its position must be interpolated within the ligature rather
than read off a glyph origin.

---

## 3. Render geometry, fixed for every fixture

Identical to Round 1's rule so the two rounds share one convention:

```
device = (staff.x * scale + tx, ty - staff.y * scale)
scale  = 100 device px per staff space
target = 1920 x 1080 (pin 4)
```

* Em size **1.28 staff spaces = 128 device px**, and the size is **derived
  from the differential's blind spot, not chosen for legibility** — see §10's
  D1 blind-spot rule. Measured on TeX Gyre Pagella (mid-height scanline,
  upem 1000): lowercase verticals `l`/`i`/`n` are 84 font units, round strokes
  `o`/`e` are 93–94. That is **10.8 and 12.0 device px** at this em size,
  comfortably above D1's 5 px floor.

  **Revision 1 pinned 64 px em, which was wrong**, and wrong in a way that
  would have quietly voided the round's strongest rule: at 64 px the same
  stems measure **5.4 device px**, and D1's 2 px band swallows any stroke
  narrower than 5 px whole. D1 would have been blind to every stem in the
  fixture set while still reporting `pass`. The em size and the band radius
  had been chosen independently, and were incompatible.

  All five fixtures fit the 1920 px target at this size. Measured from the
  generated `fixtures.json` (`bounds` is absolute staff space, so the device
  right edge is `bounds.right * 100`), longest first:

  | fixture | ink width (device px) | right edge (device px) |
  |---|---|---|
  | F-A | 1553.2 | 1715.1 |
  | F-D | 1128.8 | 1290.7 |
  | F-E |  860.3 | 1023.0 |
  | F-B |  486.3 |  809.1 |
  | F-C |  274.2 |  597.0 |

  F-A is the longest and clears the frame by ~205 px.
* Baseline origin at staff-space **`1638/1024 = 1.599609375`**, `y = 0.0` —
  i.e. device **`(159.9609375, 540.0)`** — left-aligned, vertically centred,
  with room for the longest fixture.

  **This is the normative value, and it is not `1.6`.** Invariant 5 puts every
  position on the `1/1024` staff-space grid, and `1.6 × 1024 = 1638.4` is not
  an integer, so `1.6` is not a representable origin. Revision 1 stated `1.6` /
  device `(160, 540)` here and the code quantized it silently on the way past;
  §12 recorded the discrepancy as a note, which does not repair a normative
  section that still states an invalid constant. `RUN_ORIGIN_STAFF` is now
  `1638.0/1024.0` exactly, so the constant and the grid agree at the source
  rather than at the first rounding.
* Ink is opaque black on opaque white, as in Round 1, so the same luminance
  classification applies.
* `align` is `Start`; per W3, `PositionedGlyph::offset` has alignment **already
  applied**, so a consumer places the run by `origin` alone. The field is
  retained as a record of the decision, and the fixtures assert that
  re-deriving from `align` is never necessary.

---

## 4. Measured shaping facts, recorded before anything consumes them

Produced by `rustybuzz` 0.20.1 / `unicode-bidi` 0.3.18 /
`unicode-segmentation` 1.13.3 / `ttf-parser` 0.25.1 on the faces in §1. These
are **precommitted expectations**: the generator asserts them, and a mismatch
is a reported failure, never a silent re-record.

Total glyph counts are stated for **every** fixture below. Revision 1 stated
them only for F-A, F-B and F-E, which left F-C and F-D uncovered by the one
validator check that catches shaped-output drift; the fix is to state the
measured numbers, not to let a validator invent them.

**F-A** — 28 codepoints, 30 bytes, **26 glyphs**, all face 0. Two ligature
clusters: cluster at byte 9 spans `ff` → gid 234; cluster at byte 26 spans
`fi` → gid 97. Em dash → gid 119.

**F-B** — Latin head `"Coro "` (5 bytes) → 5 glyphs on **face 0**; Hebrew tail
(6 bytes) → 3 glyphs on **face 1**, RTL, clusters descending 4/2/0. Two
segments, two faces, one string.

**F-C** — 6 codepoints, 7 bytes, **5 glyphs** in 2 segments. U+0627 resolves
in **neither** face (`glyph_index` → `None` in both).
Recorded as an **explicit unresolved cluster**, never dropped and never
substituted from the host: W3's invariant is that a cluster shaping could not
resolve is represented diagnostically, because a dropped cluster is a silent
divergence between the string and the ink.

**F-D** — 20 codepoints, 23 bytes, **20 glyphs** in 3 segments. Base level 0;
visual runs `0..8` level 0 (`"Allegro "`), `8..14` level 1 (Hebrew), `14..23`
level 0 (`" con brio"`). The middle segment is on face 1, the outer two on
face 0.

**F-E** — 15 codepoints, 19 bytes, **13 glyphs**. `e`+U+0301 composes to gid
198 at byte 3 and again at byte 16. Graphemes: `["C","a","f","e\u{301}"," ",
"—"," ","r","e","s","u","m","e\u{301}"]` — 13 graphemes, 15 codepoints.

---

## 5. `SpikeResolvedText` — the complete §3E mirror

Pin 8: mirroring a subset and calling it §3E would test a shape the amendment
is not going to have. Every field of W3 §3E is present, with W3's own names:

`provenance` · `text` · `shaping: SpikeTextShapingIdentity` ·
`segments: Vec<ShapedSegment>` · `clusters: ClusterMap` · `bounds` ·
`reserved_box` · `origin` · `align` · `style` · `layer`.

`ShapedSegment`: `face` (index into the chain) · `glyphs: Vec<PositionedGlyph>`
· `source: Range<u32>` (UTF-8 byte offsets) · `direction` · `script` ·
`language` · `size: StaffSpace`.

`PositionedGlyph`: `glyph_id` · `offset: Point` · `transform: Option<Transform2D>`.

**Every W3 invariant is asserted by the generator, not merely honoured:**

1. every `ClusterMap` offset and every `source` bound is a **valid UTF-8
   boundary** in `text`;
2. segment source ranges **partition the whole string** — total, non-
   overlapping in logical order, whatever the visual order;
3. every cluster carries its source range, its glyph indices, and its caret
   stops, each stop with a **geometric position and a bidi affinity**;
4. an unresolved cluster is present with an explicit marker (F-C), never
   dropped;
5. positions are staff-space, y-up, quantized on the **1/1024 grid**, the same
   convention as glyph positions — text quantization is not a second
   convention.

The type is `SpikeResolvedText`, in the spike workspace, marked
non-canonical in its own doc comment. It is **not** the `.tex` amendment and
does not pre-empt it.

---

## 6. `SpikeTextShapingIdentity` — every pin-9 field, and where its value comes from

| field | value on these fixtures | source |
|---|---|---|
| `faces[i].family` | `TeX Gyre Pagella` / `Liberation Serif` | name id 1, **diagnostic only** |
| `faces[i].version` | `Version 2.501;PS 2.501;ffdkm 0.1` / `Version 2.1.5` | name id 5, **diagnostic only** |
| `faces[i].file_hash` | §1 table | SHA-256 over the exact file bytes — **the identity** |
| `faces[i].face_index` | 0 / 0 | neither file is a collection |
| `faces[i].variations` | empty | both faces measured non-variable |
| `faces[i].synthesis` | none | no synthetic weight or slant is applied |
| `shaper` / `shaper_version` | `rustybuzz` / `0.20.1` | it moves glyphs, so it is an input on the footing of the font version |
| `features` | the empty set, canonically ordered | see below |
| `unicode_version` | the version backing `unicode-bidi` **and** `unicode-segmentation`, both recorded | see below |

**Features.** The fixtures apply **no explicit feature settings**; rustybuzz's
default horizontal feature set governs, which is what produced the measured
ligatures in §4. The identity records the empty explicit set **plus the shaper
id and version that define the defaults** — an identity recording "empty" with
no shaper version would be exactly the partial identity pin 9 forbids, since
the defaults are the shaper's, not the document's.

**Unicode version, recorded twice on purpose.** Pin 9 is explicit that
`unicode-bidi` does **not** do grapheme segmentation, that the segmentation
implementation is separate, and that it and its Unicode-data version are named
in the identity and the report — because caret stops come from it and not from
the shaper. The identity therefore carries **both** `(bidi_impl, bidi_version,
bidi_unicode_version)` and `(segmentation_impl, segmentation_version,
segmentation_unicode_version)`, each read from the crate rather than asserted
here, and the report prints both. If the two disagree on Unicode version, that
disagreement is **reported as a finding**, not reconciled: two components
defining one `unicode_version` field is a shape problem, and it belongs in §12.

---

## 7. Cluster map, caret stops, and the hit-test contract

Pinned by W3 §5 check 4 and restated here as the thing the generator builds:

* **Base index is UTF-8 byte offsets** into `text`, addressing the stored
  `String` directly.
* **Caret stops are grapheme-cluster boundaries**, from
  `unicode-segmentation`, **not** codepoint boundaries and **not** glyph
  boundaries. F-E has 13 stops for 15 codepoints; F-A has a stop between the
  two `f`s of `affettuoso` although the ligature is one glyph.
* **Each stop carries a bidi affinity**, so a caret at a direction boundary is
  unambiguous. F-D's byte 8 and byte 14 each carry two stops — one per
  affinity — at **different geometric positions**, which is the whole reason
  affinity exists.
* **A stop inside a ligature is interpolated across the ligature's advance**
  in proportion to the cluster's grapheme count, and the recipe records that
  rule explicitly so a candidate cannot pass by rounding to the glyph origin.

The expected hit-test answers are committed per fixture as a table of
`(device point) -> (byte offset, affinity)` probes: for every caret stop, one
probe at the midpoint of each adjacent grapheme, plus probes before the first
and after the last stop. Points are placed at least **4 device px** from any
stop position, so a correct implementation cannot fail on a rounding tie and an
incorrect one cannot pass on one. A probe that cannot meet the 4 px separation
is **dropped and recorded as dropped** — never placed closer. Measured: 80
probes across the five fixtures (F-A 29, F-B 9, F-C 7, F-D 21, F-E 14), **none
dropped**, smallest interior gap 31.9 device px.

**What this table cannot test, stated rather than left implicit.** The probes
carry an affinity, but they do not *test* affinity, and no point-based table
could: a device point selects one answer, while affinity is precisely the
distinction between two answers **at the same point**. F-D's two `Upstream`
stops both sit at staff-space x = 4.609375 — the same position as a
`Downstream` stop belonging to another grapheme — so a probe placed there would
be ambiguous by construction, which is what the 4 px rule exists to forbid.
Affinity is therefore validated structurally, by the direction-boundary
distinctness check (§5 invariant 3 and its F-D specialization), and hit testing
is validated by point → byte offset. Both halves of check 4 are covered; they
are covered by different instruments, and the reason is geometric, not a
convenience.

---

## 8. Accessibility oracle (check 5) — pinned, not described

Check 5 is **disqualifying**. Revision 2 of this section said "committed per
fixture: the expected node role, the expected name as exact bytes…" and then
named no role and encoded nothing. That is not an oracle; it is a place where a
judgement would have been made *after* seeing a candidate's tree, which is the
one thing pin 13 exists to prevent. Revision 3 pins it, in this section and in
`fixtures.json`'s own `accessibility` record per fixture (`round2-textkit`
`src/a11y.rs`), validated against literals by `FixtureFile::validate`.

### 8.1 The name

The run reaches assistive technology **as its source string** — the exact bytes
of `text`, not the shaped glyphs, not a graphic, and not a normalization of it.
Each fixture's record carries the name three ways — the string, its **lowercase
hex**, and its byte length — because a normalization can look identical in a
string field and never does in hex.

**Composition.** F-B and F-D are multi-segment runs, and a tree that exposes one
text node per direction run is not wrong; requiring exactly one node would
manufacture a failure for a legitimate implementation. So the requirement is on
the concatenation:

> the run's own accessible name, **or** the names of its text descendants
> concatenated in **logical** (not visual) order, must equal the source string
> byte for byte.

F-D is the fixture that makes that distinction bite: a tree assembled by walking
the visual runs left to right produces a different string, and only there.

### 8.2 The role

Stated **per platform**, not in one toolkit's vocabulary. Naming only
AccessKit's `Role` enum would have quietly favoured C1 (egui ships AccessKit)
over C2 (vello is a rendering crate with no accessibility layer at all), and a
criterion that encodes one candidate's stack is not a criterion. A candidate
satisfies this half by matching **one row** — the platform it actually exposes a
tree on — and it is not required to expose trees on platforms it does not
target.

| platform | accepted | prohibited |
|---|---|---|
| accesskit-0.24 | `Label`, `TextRun`, `Paragraph` | `Image`, `GraphicsObject`, `GraphicsSymbol`, `GenericContainer`, `Unknown`, `Pane` |
| at-spi2 | `label`, `static`, `text`, `paragraph` | `image`, `canvas`, `filler`, `panel`, `unknown` |
| aria | *(no role)*, `text`, `paragraph` | `img`, `presentation`, `none`, `graphics-object`, `graphics-symbol` |
| macos-nsaccessibility | `AXStaticText` | `AXImage`, `AXUnknown`, `AXGroup` |
| windows-uia | `Text` | `Image`, `Pane`, `Custom` |

The accesskit row was read from the `accesskit` 0.24.1 `Role` enum in this
workspace's own lockfile, not from memory; every name in that row exists there.
The prohibited column is named rather than left as "anything not accepted", so a
candidate's result reads as *this specific* divergence.

### 8.3 Outcomes that fail whatever the role says

* `absent-from-tree` — **the one this check will most likely actually catch.**
  It is the default outcome for a toolkit that draws to a canvas and stops.
* `name-empty` — absence wearing a role.
* `name-normalized` — F-E's case. A tree exposing `Café` (NFC) for a fixture
  whose `text` is `Cafe\u{301}` has silently normalized, a divergence between
  the string and the record exactly as damaging as a dropped cluster.
* `name-is-shaped-glyphs` — the tree exposes what was drawn rather than what was
  said: glyph names, glyph ids, or the ligated text. F-A is the fixture.
* `name-drops-unresolved-codepoints` — F-C's case. Its U+0627 is covered by
  **neither** declared face and draws no ink at all, and it must appear in the
  name regardless: the accessibility tree carries the text, not the ink.

### 8.4 What is deliberately not pinned

Nothing here says *how* a candidate builds the tree, on which thread, or through
which crate. A candidate that has to write its own accessibility layer to pass
is free to; what it may not do is expose the run as a picture, or not expose it
at all.

---

## 9. The SVG reference emitter (pin 10)

Today's exporter cannot draw a `SpikeResolvedText` — `<text>` carries
characters, and the viewer's shaper picks the glyphs, so anything contextual
(the measured `ff`/`fi` ligatures, the composed `é`) would silently draw
different glyphs than the layout resolved. Without this emitter, check 1 is
`NOT RUN` for every candidate and the round decides nothing.

The spike emits **explicit glyph outlines as `<path>`**, from the same hashed
face and the same glyph ids, via `ttf-parser` (already in `rustybuzz`'s tree),
then rasterizes with `resvg` 0.45 under pin 4's configuration — 1920×1080,
opaque white ground, opaque black ink.

**It never emits `<text>`, and the generator asserts that** — a `<text>`
element anywhere in the output is a hard failure of the emitter, because it
would reintroduce exactly the re-shaping this round exists to forbid.

This is a prototype of the explicit-glyph output W3 says the real exporter
needs, and its findings are reported as such.

---

## 10. The bounded visual differential — defined before anything is compared

Ruling A demoted SVG to export and permitted "geometry/scene equivalence plus
a **bounded visual differential** under a controlled backend, NOT pixel
equality", because a GPU tessellator legitimately differs from `resvg` in
antialiasing and curve flattening while being geometrically correct. That
phrase has never been given a number. It is given one here, in advance,
because a tolerance chosen after seeing a candidate's output is not a
tolerance.

Both rasters are 1920×1080, opaque, black-on-white. Both are reduced to 8-bit
luminance with the same Rec. 601 weights Round 1 used.

**Edge band.** A reference pixel is an *edge pixel* if its 3×3 neighbourhood
contains both a pixel with luma < 128 and one with luma ≥ 128. The **band** is
every pixel within Chebyshev distance `EDGE_BAND_PX = 2` of an edge pixel.
This is the same device Round 1 used for its 8 px clearance floor: confine the
comparison to where the answer is geometric and not a coin flip about
antialiasing.

**Four rules decide. All four are hard.**

| | rule | rationale |
|---|---|---|
| **D1** | **Outside the band, zero pixels may differ in class** (ink = luma < 128). Not "few". Zero. | Away from an edge, both renderers are painting solid ink or solid ground. Any disagreement there is geometry, not antialiasing. |
| **D2** | **Whole-image ink mass** — Σ(255 − luma)/255 — agrees within **2%** relative. | Catches "drew nothing" and "drew everything" outright. |
| **D3** | **Whole-image ink centroid** agrees within **0.5 device px** per axis. | Catches gross misplacement of the run as a whole. Its floor is declared below; it is not a sub-pixel registration test. |
| **D4** | **Per-glyph ink mass**, over each shaped glyph's device bounding box dilated by 3 px, agrees within **2%** relative, **for every glyph**. | This is the rule that actually catches a wrong, dropped, or re-shaped glyph. D1 cannot (blind spot below) and whole-image D2 cannot (one glyph is a small fraction of the total). |

**D1's blind spot, measured and declared.** D1 can only see an error that
reaches a pixel outside the band, so it is **structurally blind to any error
confined to a stroke narrower than `2 * EDGE_BAND_PX + 1 = 5 device px`** —
such a stroke is entirely within 2 px of its own edges, so deleting it outright
changes no unbanded pixel. This is not a defect to be fixed by tuning; it is
what confining the comparison to non-edge pixels *means*. It is handled by
choosing the em size so the fixtures have no stroke that thin (§3: thinnest
measured stroke 10.8 px) **and** by D4, which does not depend on band geometry
at all. Verified empirically: deleting a 4 px stem from a synthetic reference
produced `d1 = 0` differing pixels.

**D3's detection floor, declared rather than discovered.** A whole-image
centroid is one number over two million pixels. Measured on the synthetic
reference: a legitimate antialiasing-only variant moved it **0.346 px**, while
a true 0.5 px translation moved it **0.486 px**. Those are not separable, so
**D3 does not detect uniform drift below roughly 1 device px, and this recipe
does not claim it does.** D3 is retained for gross misplacement, where it is
decisive (deleting one stem moved it 40.7 px; a 1% scale moved it 2.7 px).
Sub-pixel registration is **out of scope for this round**, stated here in
advance rather than inferred later from a candidate's numbers.

**Reported, never deciding:** inside the band, the max |Δluma| and the count of
pixels differing by more than 16. Those numbers are antialiasing, which is
precisely what the differential is bounded *against* measuring.

**These thresholds are claims, and §11 is how they are tested.** If a mutation
in §11 fails to kill, the threshold is wrong and is reported as wrong — it is
never loosened to make the comparison pass, under the same rule as goldens.
Revision 1's rules D1–D3 were tested exactly that way and **two of its
mutations did not kill**; the finding produced D4, the declared D1 blind spot,
and the declared D3 floor above, rather than a relaxed threshold.

---

## 11. The mutation set the differential must kill

**Every row below is executed, and the executable that runs it exits non-zero
when a required kill does not happen.** Two harnesses, split by what the
mutation needs:

* **M1, M2, M3, M3B, M7, M8, M10** — geometric, no fonts required:
  `cargo run --release -p round2-diff --bin selftest`, against synthetic
  geometry (§10).
* **M4, M5, M6** — *text* mutations, meaningless without shaped glyphs from the
  declared faces: `cargo run --release -p round2-reference --bin
  text_mutations`, against the real frozen fixtures. Every substituted glyph id
  and advance is measured from the faces through `round2_textkit::shape` and
  anchor-asserted before use, so a mutation that silently became a no-op fails
  loudly rather than passing as "did not kill".
* **M9** is structural (§9's `<text>` assertion) and fires before any raster.

Recipe revision 2 stated M4/M5/M6 and executed none of them. **Executing them
corrected the recipe on the first run** — see M4.

| # | mutation | must fail | measured |
|---|---|---|---|
| M1 | translate the whole run by **1 device px** in x | D3 (and D1 where strokes exceed the band) | kills |
| M2 | translate by **0.5 device px** | **nothing required — boundary probe.** §10 declares D3's floor at ~1 px; a mutation set at exactly the tolerance tests arithmetic, not the rule. | recorded |
| M3 | drop a stem **below** D1's 5 px floor | **D4** and D2; D1 expected silent | kills D4; D1 silent, as declared |
| M3B | drop a stem **above** D1's 5 px floor | D1 **and** D4 | kills both |
| M4 | replace the `ff` ligature with the two unligated glyphs (a re-shaping consumer's output) | **D1** — *not* D4; see below | D1 = 221 px outside band; D4 worst region 1.40%, ligature's own region 1.20% (tolerance 2%) |
| M5 | draw the composed `é` as `e` with the acute **omitted** | **D4** | D4 13.28% and 13.19% on the two `é` regions; D1, D2 (2.55%) and D3 (2.41 px) also fire |
| M6 | render the Hebrew segment with face **0** substituted (host substitution, the thing check 2 forbids) | emitter refuses; if forced, D4 | refusal fires on U+05D0 before any raster; forced, D4 = 95.37% / 71.26% / 34.69% on the three Hebrew regions |
| M7 | scale the run by **1%** about its origin | D1, D3, D4 | kills |
| M8 | blank the target entirely | D2 | kills |
| M9 | render the source string as `<text>` instead of explicit glyphs | emitter assertion (§9), before any raster | kills |

### M4 is assigned to D1, and the measurement is why

Revision 2 assigned M4 to D4 by analogy with M3, a dropped glyph. Executing it
showed the analogy is false. An `ff` ligature and two `f` glyphs carry very
nearly the **same ink**: 0.07% of whole-image mass, and 1.20% inside the
ligature's own region. D4 is a *mass* rule, and this is a *shape* substitution
— the wrong instrument. D1, which asks where the ink is rather than how much,
sees it immediately at 221 differing pixels outside the edge band.

D4 is not *structurally* blind here the way D1 is blind below 5 px — it came
within 1.7× of firing. But tightening D4 to catch 1.20% would leave barely
1.3× of margin over M10's measured 0.785%, the legitimate antialiasing-only
variant that **must** pass. A threshold that close to a known-good variant is
not a tolerance. So D4 keeps its 2%, D1 owns M4, and the margin is written down
here instead of being discovered by whoever tightens it later.

The general shape, worth carrying into the ruling: **D1 and D4 are
complementary, and neither is sufficient.** D1 is blind to error confined
inside a stroke narrower than 5 device px; D4 is blind to error that rearranges
ink without changing its mass. M3 is caught only by D4; M4 is caught only by
D1.

**M10 — the mutation that is not a mutation.** A legitimate
antialiasing-only variant of the reference — identical geometry, different
edge coverage — **must PASS all four rules.** A differential that rejects
everything is exactly as useless as one that accepts everything, and this is
the only test that shows the tolerance is a tolerance. Measured on the
synthetic reference: D1 0 differing px, D2 0.25%, D3 0.35 px, in-band
max |Δluma| 3.

M9 is not a differential test; it is listed here because it is the mutation
that would make the differential *meaningless*, and the emitter must refuse it
structurally rather than be caught by a threshold.

**Why M3 and M5 name D4 and not D1.** Revision 1 required D1 to catch them. It
cannot: each is an error confined to strokes inside the band, and deleting a
4 px stem from the synthetic reference measured `d1 = 0`. Revision 1 would have
shipped a rule that reported `pass` on a dropped glyph. (M4 went the other way
on measurement — see above.)

A mutation that does **not** kill is reported as a finding against this recipe,
and the recipe changes — as it has now done twice: revision 2 gained D4, the
declared D1 blind spot and the declared D3 floor; revision 3 moved M4 from D4
to D1 and recorded D4's mass-preserving blind spot alongside D1's stroke-width
one.

---

## 12. Findings routed back to the W3 `.tex` amendment (pin 8)

The spike is §3E's first consumer, and pin 8 makes each awkwardness a finding.
Recorded as they are discovered; these are already known before implementation:

**W3-F1 — `TextFaceIdentity::version: Option<SemVer>` is the wrong type.**
Real font versions are not semver. The two faces here report
`Version 2.501;PS 2.501;ffdkm 0.1` and `Version 2.1.5`; only the second parses
as semver, and only after stripping a prefix. Since the field is explicitly
diagnostic — `file_hash` is the identity — the honest type is the **raw name-
table string**, `Option<String>`, or the field should be dropped. Typing it as
`SemVer` forces either a lossy parse or an empty field on a face that plainly
has a version.

**W3-F2 — one `unicode_version` field, two components define it.** Pin 9
requires that the segmentation implementation and its Unicode-data version be
named, and that they are separate from the bidi implementation. §3E carries a
single `unicode_version: UnicodeVersion`. Either the field means "the bidi
algorithm's" and segmentation's is unrecorded — the exact gap pin 9 says must
not exist — or it means both and the type is silently asserting the two agree.
The spike records both, and the amendment should carry both.

**W3-F3 — `ShapedSegment::face: u32` has no value for a wholly-uncovered
span.** F-C's Arabic letter resolves in no declared face, so its segment has no
face index to carry — but W3's invariant that segment source ranges *partition
the whole string* means the span cannot simply be omitted. The two requirements
are in direct conflict as §3E is written. The spike's stand-in uses
`face: Option<u32>`; the amendment needs that, or an explicit unresolved
segment variant.

**W3-F4 — at a boundary into an unresolved span, affinity carries no
geometry.** W3 requires each caret stop to have "a geometric position and a
bidi affinity, so a caret at a direction boundary is unambiguous". F-C's byte 5
is a direction boundary (Latin LTR → Arabic RTL) whose downstream side is an
unresolved, zero-advance cluster, so both affinities land on the **identical**
position — measured, staff-space x = 3.130859375 for both. The distinctness
that makes affinity useful is unavailable by construction there. The spike
therefore enforces boundary distinctness on F-D (where both sides have ink) and
**deliberately exempts** an unresolved-side boundary; the exemption is recorded
here because an unstated exemption is indistinguishable from an oversight. The
amendment should say which of the two it wants: a stop pair that is allowed to
coincide, or a single stop where no distinction exists.

**W3-F5 — a `u128` identity does not survive a JSON round-trip, and this is
not only a spike problem.** `Provenance`'s stable id renders as up to 39
decimal digits. Round-tripping the fixture file through `serde_json::Value` —
or Python's `json`, or any JavaScript consumer — silently converts it to an
`f64`: measured, `82875741697311382809239399464544864365` came back as
`8.287574169731139e+37`. A provenance id that changes when a tool merely reads
and rewrites a file is not an identity. The canonical wire format is binary and
is unaffected, so this is a constraint on **JSON artifacts** — this file, and
any debug or fixture dump carrying an id of that width. The spike serializes it
as a decimal string. Any project tooling that dumps IR to JSON needs the same
treatment.

**W3-F6 — §3E defines no serialized form, and the first consumer to need one
wrote a lossy mirror.** `epiphany-layout-ir` carries no `serde` dependency at
all, so nothing in §3E can be serialized as written. Every consumer that has to
persist, cache, dump, or send a `ResolvedText` must hand-write a mirror — and
the very first one (this spike's `fixtures.json`) was quietly lossy for two
`Provenance` fields until review caught it: a `Debug` rendering in place of
`source`, and a length in place of `dependencies`. `Debug` output has no
stability contract and cannot be parsed back, and a dependency *count* discards
the invalidation set that is the field's whole purpose. It lost nothing
measurable here only because these fixtures' dependency lists are empty, which
is an accident of the fixtures.

The mirror is fixed (`source` and `dependencies` now carry
`TypedObjectId::canonical_bytes()`, under W3's field names). What routes back
is the shape of the mistake: an incremental-layout cache and an out-of-process
renderer are both plainly in W3's future, each needs this same conversion, and
each will write it independently. The amendment should specify `ResolvedText`'s
serialized form once — a derive, or a canonical byte form as Chapter 5 fixes
for `TypedObjectId` — rather than leave one per consumer.

**Not a W3 finding, but recorded — the origin was not on the quantization
grid.** Invariant 5 requires positions on the 1/1024 grid, and this recipe's
own stated origin of `1.6` staff spaces is not representable there
(1.6 × 1024 = 1638.4). The invariant caught it during implementation rather
than after, which is the whole reason it is asserted instead of assumed.

Revision 2 recorded that here and left §3 saying `1.6` / device `(160, 540)`,
on the reasoning that the note explained the discrepancy. It does not: a
findings section does not repair a normative section, and a reader taking §3 at
its word would have taken an unrepresentable origin. **Revision 3 states the
quantized value in §3 itself** — `1638/1024 = 1.599609375`, device x
`159.9609375` — and `RUN_ORIGIN_STAFF` is now that exact ratio, so nothing is
silently rounded on the way past. Worth keeping on the record because the
number came from this recipe, not from the code: a stated constant can violate
a stated invariant, only one of the two was executable, and the executable one
was right.

**Also not a W3 finding, and worse than the one above — the spike had its own
quantizer.** W3 §5 says positions are "quantized on the same 1/1024 grid as
glyph positions … **so text quantization is not a second convention**". The
spike's `quantize_component` implemented the grid arithmetic locally as
`(v * 1024.0).round() / 1024.0`, which is round-half-**away-from-zero**, while
`epiphany_determinism::QuantizedCoord::from_staff_spaces` — the project's own
quantizer, Appendix D — is round-half-to-**even**. The divergence was *named*
in a doc comment, with the reasoning that this spike's values never land on a
tie. That is not a defence: nothing checked it, a font metric or a padding
constant could land on a tie at any time, and W3's requirement is about the
convention rather than about whether two conventions agree on today's inputs.
Naming a divergence is not the same as being allowed to take it.

`quantize_component` now routes through `QuantizedCoord`, and `is_on_grid` —
which claimed exactness while accepting anything within `1e-6` — is now an
exact round-trip through the same type. Four tests pin ties-to-even at
`±0.5` and `±2.5` grid units, each chosen because the two conventions
**disagree** there; a fifth records a tie where they agree, so the four are
understood as testing the disagreement and not merely "ties round somewhere".
Regenerating changed nothing: `fixtures.json` is byte-identical and the
artifact digest is unchanged at `c808d6eb…`, so no fixture value did in fact
land on a tie — which is what the old comment claimed, and is still not what
made it acceptable.

Further findings are appended as implementation reaches them.

---

## 13. Rulings

1. **Check 3 scoring — RULED 2026-07-29: `NOT RUN`.** Full text and its
   consequences in §1.2. F-D is recorded separately as *Supplementary PASS —
   Hebrew/Latin bidi itemization, fallback-face selection, visual ordering, and
   resolved positioning*, and must not upgrade check 3 to PASS. Eligibility is
   unaffected: check 3 is not disqualifying.
2. **Nothing else is open.** Every other choice here is either measured, taken
   from a pin verbatim, or recorded as a finding against the amendment rather
   than decided by the spike.
