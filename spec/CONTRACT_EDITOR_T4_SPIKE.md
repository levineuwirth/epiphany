# Contract: Editor T4 — the toolkit spike

**Revision 6** (2026-07-28), after five reviews. Revision 1 left discretion in
the measurement and elimination mechanics; revision 2 reordered the ladder and
pinned the deciding numbers, but its new structure carried its own defects —
a damage oracle that compared the wrong thing, correctness failures dressed as
environmental absences, and an escalation branch with no continuation.
Revisions 3 and 4 closed those and introduced smaller ones of their own — a
damage gate covering one rung, a censored median that flattered silence, and a
latency threshold with no timer origin. Revision 5 closed those. **Revision 6 amends Round 1** after building its
oracle exposed two defects in the round as written: its glyph set was chosen by
subpath count rather than by measured holes (`fClef` has none), and it framed
criterion 1 around the fill *rule* when Bravura's oppositely-wound contours make
even-odd and nonzero agree. Both are corrected below, and Ruling A criterion 1
is amended to match.

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/PLAN_EDITOR_APP.md` Ruling A ("What this ruling does **not** pin: the
toolkit/tessellation stack. The T4 spike decides it, bounded by these recorded
criteria" 1–6) and Ruling D (the app crate is created **at T4**, and the spike
opens the tranche). All three §3.7 prerequisites are discharged: W1 `dd33b34`,
W2 `24f8c80`, W3 `f639919` (`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`).

Execution model as T1a/T2/T3/W1/W2: Sonnet subagents implement per round,
coordinator line-level review with independent re-runs, **user deep-dives at
this contract's sign-off, at every precommitted oracle before candidates run
against it, at every measured table before it enters the ruling, and at the
final verdict**. Mutation discipline where there are assertions to mutate:
anchor-assert before substituting, restore by reversing, never `git checkout`,
never `git stash`.

**Parallel safety.** The genesis-operation tranche owns `epiphany-core`,
`epiphany-ops`, `epiphany-bundle` (at G2), all `.tex`, and testkit's
requirement-label constants, vectors, and generators
(`spec/PLAN_GENESIS_OPS.md`). **This packet touches none of them**, and by
pin 2 it modifies no existing crate at all. Requirement and conformance counts
are reported as observed, never asserted from memory; this packet moves
neither. Pin 12 additionally freezes the *sources* the spike measures against,
which file-level non-overlap does not by itself achieve.

---

## The verified starting point

Confirmed in the tree on 2026-07-28. Several of these make the spike smaller;
two make it possible at all.

* **The tessellator's input already exists.** W2's `epiphany-glyphs` exposes
  `BravuraGlyphCatalog` implementing `GlyphCatalog` with a real `render_data`,
  returning `GlyphRenderData { outline: Vec<PathCommand>, .. }` over
  `PathCommand::{MoveTo, LineTo, CurveTo, Close}` in staff-space units
  (`glyphs/src/catalog.rs:21,39`; `layout-ir/src/glyph.rs:283`). No candidate
  needs to parse SVG `d` strings, and no glyph work belongs in this packet.
* **Glyphs with holes are in the bundled set — but subpath count does not
  identify them.** Revisions 1–5 named `gClef`/`fClef`/`timeSig8`/
  `accidentalFlat` on the strength of "19 of the 37 bundled outlines have more
  than one subpath". That conflates *multi-subpath* with *has a bounded hole*,
  and `fClef` is the counter-example: its three subpaths are a bowl and **two
  solid, disjoint dots**, nested in nothing. Measured over all 37 by
  point-in-path (2026-07-28), exactly twelve carry a **bounded hole** —
  `gClef`, `timeSig8`, `accidentalFlat`, `accidentalSharp`,
  `accidentalNatural`, `noteheadHalf`, `noteheadWhole`, `noteheadDoubleWhole`,
  `timeSig0`, `timeSig6`, `timeSig9`, `dynamicPiano` — while `fClef`, `cClef`,
  `barlineFinal` and every repeat glyph carry none. Criterion 1 has real
  material, and Round 1 below names it by that measurement rather than by
  subpath count.
* **Bravura's contours are correctly oppositely wound**, so even-odd and
  nonzero *agree* on every bundled hole. Signed ring areas, as measured by the
  Round 1 oracle's adaptive flattening (tolerance 0.0005 staff-space, the
  authoritative figures — magnitudes are flattening-dependent, the **signs**
  are the claim): `gClef` `[8.702, −0.691, −1.803, −0.509]`, `timeSig8`
  `[2.674, −0.435, −0.515]`, `accidentalFlat` `[1.040, −0.257]`,
  `noteheadHalf` `[0.903, −0.368]`; and `fClef` `[2.534, 0.153, 0.148]` —
  **all positive**, which is the same fact from the other side: no counter,
  three filled components. The
  fill **rule** is therefore not the load-bearing property here; **preserving
  every filled contour and every bounded counter** is. Ruling A criterion 1 is
  amended to say so.
* **Per-system ownership is populated only by the real solver.** `Engraver`'s
  casting module builds one `PrimitiveIndices` per system
  (`engrave/src/casting.rs:1185`); the **stub solver publishes everything
  `unowned`** by deliberate honesty (`layout-ir/src/resolved.rs:82-87`).
  Damage-based repaint is therefore untestable under the stub — the spike
  uses `Engraver`, or it is not measuring Ruling A's mechanism.
* **`ResolvedLayoutIR::systems()` exists and is documented as adopted "at T4,
  not before"** (`resolved.rs`). This packet is its first consumer.
* **There is no large fixture.** The testkit's largest is **two staves ×
  twelve measures** (`two_staff_wrapping_pressure`, `fixtures.rs:346`); the
  three-staff fixture is **one measure** (`three_staff_close_content`,
  `fixtures.rs:225`), and the `ten_measure_*` family is single-staff. No
  fixture is both wide and long. The spike builds its own ladder — pin 7.
* **There is no `ResolvedText`.** W3 recommended disposition E and the `.tex`
  amendment is the core/layout track's work, unlanded. See pins 8–10.
* **Today's SVG exporter cannot serve as round 2's reference renderer.**
  `ShapedSegment` carries font-internal glyph ids and "an ordinary SVG
  `<text>` element cannot request one"
  (`ANALYSIS_TEXT_RUN_PRIMITIVES.md:496`). W3 says the real exporter will need
  explicit-glyph path output. Pin 10 supplies that for the spike rather than
  letting the check be unrunnable.
* **The historical baseline is context, not a comparand.**
  `benches/editor_pipeline.rs` (`986c9cc`, `4a4988c`) measured today's
  SVG-string path at 123–165 µs scene-build + 1.36–2.86 ms paint per edit —
  but **dev profile**, offscreen `resvg`, medians across score states whose
  ink differs by depth, and the bench documents these limits itself
  (`editor_pipeline.rs:154`). Pin 6 re-measures it inside the spike instead.
  The demo pins `eframe = "0.29"` and rasterizes through `resvg` into an egui
  texture (`editor-gui/src/main.rs:361`).
* **Two GPU adapters are present on the development machine** — a discrete
  Navi 31 and an integrated Granite Ridge — with a live display. Vulkan 1.4
  enumerated from an unsandboxed shell; **re-confirm adapter enumeration from
  the spike's own process before relying on it**, since a sandboxed run could
  not see it.
* **MSRV is 1.85**, and the MSRV CI job excludes exactly one crate,
  `epiphany-editor-gui` (`ci.yml:99`). Pin 1 keeps that list unchanged.

---

## What the spike is, and what it is not

**It is** an elimination experiment that produces one document: a ruling
naming the toolkit/tessellation stack for `epiphany-editor-app`, with the
measurements and disqualifications that chose it.

**It is not the app.** No command registry, no editing intents, no document
layer, no undo, no goldens, no persistence. Ruling D creates
`epiphany-editor-app` as a fresh crate **after** this verdict, built around
the §3.5 command registry from its first commit; spike code is not grown into
it, for the same reason the demo is not.

**It does not reopen Ruling A.** The architecture — direct vector canvas over
`ResolvedLayoutIR`, viewport-culled, per-system damage, SVG demoted to export
— is granted. The spike chooses the stack. A finding that *no* candidate can
implement it escalates to the user as a ruling-amendment request; it does not
silently promote the SVG path back.

---

## Design pins

### Placement and isolation

1. **The spike lives outside the workspace**, at `spikes/editor-toolkit/`, as
   its own Cargo workspace with its own committed `Cargo.lock`, path-depending
   on the epiphany crates it reads. The root `Cargo.toml` gains one additive
   line — `exclude = ["spikes"]` — and nothing else. Rationale: candidates
   bring `wgpu`, `vello`, modern `egui`, `lyon`, `rustybuzz`; as workspace
   members they would enter the shared lockfile and the MSRV closure for code
   designed to be thrown away. (Cargo's `exclude` places the directory outside
   root `--workspace` and lets it keep its own lockfile.) **Consequence,
   stated because it is a real cost:** root `cargo test --workspace` does not
   build the spike, so the spike's own gate is run explicitly and reported.
2. **No existing crate is modified.** The spike consumes public API only. If a
   candidate needs something `layout-ir`, `glyphs`, `engrave`, or
   `editor-core` does not expose, the spike **works around it locally and
   reports the gap** as a named finding for a later packet. An API that has to
   change to make a candidate work is itself evidence about that candidate.

### Measurement

3. **Builds are `--release --locked`.** Every timed figure, without exception.
   A dev-profile number decides nothing here.
4. **One common deciding configuration, identical across candidates:**
   offscreen render target (not a surface), fixed target size **1920 × 1080**,
   identical MSAA sample count (4×, or the highest all survivors support,
   stated), identical sRGB target format, `wgpu` Vulkan backend on both
   adapters. Candidates are compared only within this configuration.
   **Surface-present timing is measured separately as a capability figure and
   decides nothing** — it mixes in the compositor.
5. **Timed work must be completed GPU work, not submission.** Each timed
   iteration ends with an explicit device wait (`Device::poll` with
   `Maintain::Wait`, or the candidate stack's equivalent barrier) and, where
   the backend supports timestamp queries, a GPU-side timestamp span reported
   **alongside** the wall figure. A candidate that cannot be made to report
   completed work is reported as `NOT RUN` for that figure, never as fast.
   Protocol per figure: **≥50 warm-up iterations discarded, then ≥200 timed
   iterations**; report median, p95, min, max, and iteration count. **The
   deciding statistic is the median**, with a **predeclared practical
   equivalence band of 10%** — a band chosen in advance to stop small
   differences deciding architecture, *not* a claim of statistical
   indistinguishability, which this protocol does not test for. Vsync is off,
   or the figure is a full distribution — a 16.7 ms mean under vsync measures
   the display.
6. **The SVG comparison is re-measured inside the spike, or it is not made.**
   Under pins 3–5, on the exact same fixture, target, size, statistic, and
   machine: render the score to an SVG string, parse with `usvg`, rasterize
   with `resvg` to the same offscreen dimensions, and compare
   **scene-build + paint totals**, because the premise under test concerns the
   whole renderer-owned path, not paint alone. The historical 123–165 µs /
   1.36–2.86 ms figures appear in the report **labeled as dev-profile
   historical context only** and are never compared against a candidate.
   **What falsification means, defined in advance:** the SVG path has no
   damage mechanism — it re-serializes and re-rasterizes everything per edit —
   so the honest comparison is *candidate damage repaint* against *SVG full
   repaint*, at the deciding rung. A candidate losing on **full** paint at the
   smallest rung does **not** falsify Ruling A's demotion, since damage and
   culling are exactly what the demotion buys; a candidate losing on the
   damage-vs-full comparison at the deciding rung does, and escalates to the
   user as a finding rather than resolving itself.
7. **The fixture ladder is pinned now, before any candidate renders**, so the
   deciding workload cannot move. Rungs are **F1–F4** (named apart from the
   elimination rounds, which are 0–5): **F1** 1 staff × 10 measures (bench
   parity), **F2** 4 staves × 32 measures, **F3** 12 staves × 100 measures,
   **F4** 24 staves × 200 measures. Generated in the spike workspace (not
   testkit, whose generators the parallel track owns) and engraved up front
   with `Engraver::default()`.
   **Dimensions do not pin the workload, and the workload is what gets
   tessellated.** The generator's musical-content recipe — notes per measure,
   accidental cadence, slur and tie density, voice count, articulations, and
   the **fixed seed** — is committed to the spike repo and **user-reviewed
   before any candidate-specific implementation begins**, under the same rule
   as the oracles (pin 13). One recipe, scaled across rungs by dimension only.
   **Reporting:** per rung, actual page and system counts, **per-type
   primitive counts** (glyphs / strokes / curves) and **total path-command
   count** — not an aggregate primitive number. Path commands are what a
   tessellator actually consumes; two layouts with equal primitive counts can
   differ severalfold in curve work.
   **Rung validity and the deciding rung are decided independently of any
   candidate.** A rung is **dropped** only by the shared engraving preflight —
   engrave exceeding 10 minutes wall, or memory exhaustion — reported with its
   reason. **The deciding rung is the largest engraving-valid rung**, full
   stop. A candidate that cannot render there records its own `NOT RUN` and
   the eligibility consequence in pin 16; **one candidate's inability does not
   shrink the contest for the others**, which revision 2's "largest completed
   by every survivor" wording allowed.

### Text

8. **Text uses a local, explicitly non-canonical stand-in.**
   `SpikeResolvedText` in the spike workspace, mirroring W3 §3E **completely**:
   `provenance`; source string; shaping identity (pin 9);
   `Vec<ShapedSegment>` of positioned glyphs — each with face index, source
   range, direction, script, language, size, **and per-glyph transform**;
   cluster map; measured `bounds`; `reserved_box`; `origin`; `align`; `style`;
   `layer`. Mirroring a subset and calling it §3E would test a shape the
   amendment is not going to have. It is **not** the `.tex` amendment and does
   not pre-empt it — the
   reverse is the point. The spike is that shape's first consumer, so **every
   place it proves awkward to consume is a finding routed back to the
   amendment**, which is the cheapest available review of a design otherwise
   reviewed only on paper.
9. **The stand-in identity records every field of W3's `TextShapingIdentity`**
   (`ANALYSIS_TEXT_RUN_PRIMITIVES.md:407`), not a file hash: the **ordered
   fallback chain**, and per face its **family** and **version** (W3 carries
   both as diagnostics even though the content hash is the identity that
   binds), its file content hash, face index, variations, and synthesis flags;
   **the shaper implementation id and
   version** ("it moves glyphs, so it is an input on exactly the footing of
   the font version"); **the OpenType feature set applied, in canonical
   order**; and the **Unicode version** governing both the bidi algorithm and
   grapheme segmentation. The exact fixture strings are committed verbatim in
   the spike repo. A partial identity would let two runs agree on pixels and
   disagree on clusters — the divergence W3 §5 check 4 exists to catch.
   **Shaping is fixture generation, not a recommendation:** `rustybuzz` shapes
   and `unicode-bidi` itemizes. **`unicode-bidi` does not do grapheme
   segmentation** — the segmentation implementation is separate
   (`unicode-segmentation` or equivalent) and **it, and its Unicode-data
   version, are named in the identity and the report**, because caret stops
   come from it and not from the shaper. Whether the *engraver* adopts any of
   these is the core/layout track's call; the spike reports on fitness only.
   **Faces** resolve **once at startup from an explicit path list, with their
   bytes hashed** — W3's narrowed rule exactly ("a host face may participate
   only once resolved to an exact content-hashed asset"), committing no font
   binary. A required face absent on the machine ⇒ `NOT RUN` (pin 14).
10. **The spike builds its own SVG reference emitter.** Today's exporter
    cannot draw a `SpikeResolvedText` — `<text>` cannot request font-internal
    glyph ids (`ANALYSIS_TEXT_RUN_PRIMITIVES.md:496`) — so without this,
    round 2 check 1 would be `NOT RUN` for every candidate and the round would
    decide nothing. The spike emits **explicit glyph outlines as `<path>`**
    from the same hashed face and the same glyph ids (`ttf-parser`, already in
    `rustybuzz`'s tree), rasterized with `resvg` under pin 4's configuration.
    This is a prototype of the explicit-glyph output W3 says the real exporter
    needs, and its findings are reported as such. **Rejected alternative:**
    sequencing round 2 after the real exporter lands — that would block T4 on
    the core track's amendment, which is precisely what pin 8 exists to avoid.

### Damage

11. **Damage repaint is defined as a state transition, with an oracle.** For
    the fixture at each rung, build a deterministic **A/B pair**: score `S`
    and `S′`, where `S′` is `S` with **one** operation applied to a single
    note chosen to lie inside one system. Engrave both up front.
    **The damage set is computed from rendered content, never from raw index
    vectors.** `PrimitiveIndices` are positions in flat arrays
    (`resolved.rs:77`), so comparing the vectors is doubly wrong: a pitch
    change that preserves primitive count alters geometry while every index
    vector stays identical, and an inserted accidental renumbers global
    indices so that later systems' vectors differ although nothing they draw
    moved. Instead, each system is fingerprinted over its **dereferenced**
    primitives — glyph reference, quantized position, transform, bounding box,
    style, layer for glyphs, and the analogous fields for strokes and curves —
    together with the system's own geometry, under a fingerprint function
    committed in the spike repo. **Assert before timing** that exactly one
    system's fingerprint differs between IR_A and IR_B. **If the count is
    anything other than one — zero included** — advance to the next note in a
    **finite, committed candidate order** (notes in canonical layout order,
    first 20 tried) and record how many were tried. Zero matters as much as
    many: an operation whose layout effect the solver absorbs entirely would
    otherwise yield a "damage repaint" that repaints nothing and times as
    instant. **Exhausting the candidate order is a harness/preflight failure**
    — reported against the spike, not against any candidate, and not recorded
    as candidate evidence, since every candidate shares the same A/B pair. **`unowned` is fingerprinted the same way, not merely counted** —
    size alone misses changed geometry at unchanged indices. If its
    fingerprint differs, its primitives join the damage set and the report
    says so; its size is reported at every rung regardless, because a large
    unowned bucket caps what damage repaint can ever save, and hiding that
    would flatter every candidate equally but the architecture not at all.
    **Timed:** retessellation of the damaged system + cache replacement +
    completed paint (pin 5), starting from a warm scene built from IR_A.
    **Oracle:** the damage-updated target must be **byte-identical** to a
    from-scratch full paint of IR_B in the same process and configuration.
    (Byte equality is legitimate here — same renderer, same settings — unlike
    the cross-renderer comparison in pin 6, which is a bounded visual
    differential.)
    **Failing the oracle is a FAIL, not a `NOT RUN`.** Under pin 14's split:
    the damage **capability** cell is `FAIL` — the check ran and the output
    was wrong — while the damage **timing** is `NOT RUN`, because no valid
    timing exists for an incorrect result. Revision 2 recorded both as
    `NOT RUN`, which filed a broken implementation as an environmental
    absence. A damage-capability `FAIL` **escalates with a required root-cause
    attribution**: candidate limitation, or spike implementation defect. That
    distinction decides whether it eliminates — Ruling A's granted
    architecture *is* per-system damage, so a genuine candidate limitation is
    disqualifying, while a spike defect is a bug to fix and re-run — and it is
    not a judgement the report author makes silently.

### Inputs and outcomes

12. **The measured source tree is frozen and named.** All rounds run inside a
    dedicated `git worktree` checked out at a **pinned baseline commit**,
    recorded by SHA in the report, so the spike's path dependencies resolve to
    frozen sources rather than the working tree — which currently carries the
    parallel track's in-flight edits. The spike's own tree is placed into that
    worktree (its `../../crates/...` path deps then resolve inside it), so the
    only unfrozen inputs are the spike's own files. Builds are `--locked`. If the baseline
    moves, **the affected round is re-run in full**; partially re-run rounds
    are not reported. Blast radius's "only modification to pre-existing
    workspace configuration or production files" is scoped against that
    baseline, not against the globally dirty tree.
13. **Every check has a precommitted oracle.** Oracles are committed to the
    spike repo, with their commit recorded, **before** any candidate renders
    against them. This is a hard sequencing rule, not a preference: "points
    that must be ink" chosen after seeing output is not a test.
14. **The outcome model, in three parts.**
    **(a) Capability results are `PASS` / `FAIL` / `NOT RUN`.** `NOT RUN`
    means *could not execute* — missing font, missing backend, unavailable
    API, environment absent. **A check that ran and produced the wrong answer
    is `FAIL`**, never `NOT RUN`; conflating them lets a broken implementation
    buy an escalation instead of recording negative evidence.
    **(b) Timings are `measured` or `NOT RUN`, separately from (a).** A
    timing is `NOT RUN` whenever no valid figure exists — including when the
    corresponding capability `FAIL`ed, since timing a wrong result measures
    nothing.
    **(c) Eligibility is tracked as "disqualifying checks passed", separately
    from criterion cells.** The **disqualifying set** is fixed here: round 0's
    accessibility route; round 1's fill correctness; round 2 checks 2 and 5;
    round 3's semantics; and a damage-capability `FAIL` attributed to
    candidate limitation (pin 11). A criterion cell is the **worst** of its
    checks and is reported for the record — but **eligibility for the
    tie-break is the disqualifying set alone**, so a `FAIL` on a
    non-disqualifying check does not silently strand a candidate the user
    chose to keep, which revision 2's "passing every hard criterion" wording
    did. **Criterion 2 has no PASS/FAIL cell at all** — it is a ranking
    criterion, represented in the matrix by its measured table plus the
    damage-capability cell, and a fabricated pass for it would be a fake row.
    The spike is not complete while a disqualifying check is `NOT RUN` for a
    surviving candidate, and a `NOT RUN` that would decide the verdict
    escalates (pin 16) — **with one exemption, named here so this pin and
    pin 16 cannot drift apart: a `NOT RUN` timing at the deciding rung is the
    predeclared ranking loss of pin 16 step 1 and eliminates without pausing.**
15. **Current releases, not Ruling A's snapshot.** Ruling A criterion 6
    records "egui 0.29 / 0.35" as a 2026-07-23 observation. The spike
    evaluates each candidate's actual release at spike time and records
    version + release date as a matrix row. **Criterion 6 is a question the
    ruling answers, not a test a candidate passes** — Ruling A defers it "to
    the spike's call under criteria 1–5" — so it gets no PASS/FAIL cell and
    is answered in prose, with the version row as its evidence.
16. **The tie-break, fixed before measurement, with escalation as a real
    outcome.** Among candidates whose **disqualifying set** (pin 14c) is fully
    passed, in order:
    1. **Damage-repaint median at the deciding rung** (pin 7), integrated
       adapter, pin 4's configuration. **Anchored to the best median, not
       pairwise:** retain every candidate within the 10% band *of the best*,
       discard the rest. Pairwise comparison is non-transitive with three
       candidates and could order them inconsistently. One survivor ⇒ it wins.
       A candidate with `NOT RUN` at the deciding rung is **not** retained
       here and its inability is reported as such (pin 7). **This is a
       predeclared ranking loss and is explicitly exempt from the escalation
       rule below** — the deciding rung is the largest engraving-valid rung,
       so failing to render it is failing on the merits at the scale the
       product targets, decided by a rule fixed before any measurement.
       **If step 1 retains nobody** — every eligible candidate `NOT RUN` at
       the deciding rung, so there is no best median to anchor to — that is
       the **no-winner branch**, not an empty selection: the spike stops and
       the ruling records a ranking-loss wipeout with the recommended
       widening.
    2. **Foreclosure coverage** — count of round-5 probes passed (0–3), each
       binary. **Retain every candidate at the maximum count and discard the
       rest**; one survivor ⇒ it wins, multiple survivors ⇒ go to 3. (Stated
       because "equal counts ⇒ go to 3" left 3/3/2 undefined.)
    3. **Maintenance surface, by Pareto rule over three operationally defined
       axes.** **The measured upstreams are pinned here, before any figure is
       fetched**, because every candidate is a composite and "the primary
       crate" would let the favourable upstream be chosen after seeing the
       data: **C1 = `egui` (emilk/egui) + `lyon` (nical/lyon); C2 = `vello`
       (linebender/vello) + `winit` (rust-windowing/winit); C3 = `iced`
       (iced-rs/iced)**. Where a candidate names more than one upstream, **it
       takes the worse value on each axis** — conservative, and it removes the
       selection entirely. Each figure is recorded with its measurement date:
       - **Transitive crate count** — unique packages from `cargo tree
         --edges normal --target <host triple>` for the candidate's spike
         crate under exactly the features the spike enables, excluding the
         epiphany path crates. Lower is better.
       - **Release recency** — days since the most recent crates.io release
         that is neither **yanked** nor a **prerelease**. Lower is better.
       - **Issue responsiveness** — measured on the pinned repositories above,
         at a **recorded snapshot date**, from an **archived issue/comment
         snapshot committed to the spike repo** so the figure is recomputable
         after the tracker moves on. A *qualifying issue* is one opened in the
         180 days before the snapshot, in the issue tracker (**pull requests
         excluded**), **not authored by a maintainer or a bot**. *Maintainer*
         is **machine-observable, not inferred**: a comment whose recorded
         GitHub `author_association` is `OWNER`, `MEMBER`, or `COLLABORATOR`,
         or whose author is listed in a named owners file **at a pinned
         commit**. Triage/write permission is not reliably public and is not
         used. **Bot comments never count as a response.**
         Take the twenty most recent qualifying issues and the time from open
         to first maintainer response. **An issue with no maintainer response
         contributes `+∞`, not its current age.** Revision 4 used
         opening-to-snapshot age, which is only a *lower bound* on the
         response time and would have given twenty issues opened yesterday and
         ignored an excellent median. With `+∞`, the median stays computable
         while fewer than half are unanswered and becomes `+∞` — the worst
         possible value, which is the right answer — once half or more are.
         Lower median is better; fewer than five qualifying issues ⇒ this axis
         is `NOT RUN` and step 3 escalates.
       A candidate wins step 3 only if it **Pareto-dominates every other
       remaining candidate** — no worse on all three axes and better on at
       least one, against each of them individually. No aggregate score, no
       weighting, no author discretion.
    A decisive `NOT RUN`, a tie surviving all three steps, or any
    Pareto-incomparable pair **escalates rather than selects** — **except**
    the deciding-rung `NOT RUN` at step 1, which is the predeclared ranking
    loss above and eliminates without pausing.

---

## The candidate set

Three entrants — **all three named by Ruling A criterion 6**
(`PLAN_EDITOR_APP.md:596`: "modern egui, iced, or a Vello surface"). The spike
may add a fourth **with written justification in the report**; it may not drop
one silently. Exact crate names, versions, features, and backends are recorded
in the report and frozen by the committed `Cargo.lock`.

* **C1 — modern `egui` + `lyon`-tessellated meshes.** Closest to the demo, so
  the migration story is cheapest; the open questions are whether pushing
  tessellated meshes through `epaint` holds up at F3/F4, and whether egui's
  text stack can be bypassed cleanly for pin 8's pre-shaped runs.
* **C2 — `vello` behind a `winit` shell.** GPU-compute path rendering, so
  criterion 1 is likely free; the open questions are windowing and UI chrome
  (there is no widget toolkit), accessibility wiring, and maturity.
* **C3 — `iced`.** Retained-mode where C1 is immediate-mode, which bears
  directly on per-system damage, and it ships its own wgpu renderer.
  **Accessibility is its live risk, and round 0 must prove the route rather
  than assume it:** AccessKit lists egui among integrated projects and does
  not list iced, and iced's upstream accessibility issue (#552) remains open.
  A manual `accesskit_winit` route may exist; round 0 demonstrates it or C3
  fails there.

**Named exclusions, so their absence is a decision.** `skia-safe`: a C++ build
chain against a project whose MSRV and reproducibility posture rests on a
pure-Rust closure, plus a vendoring and licensing surface out of proportion to
the gain. GTK/Qt drawing surfaces: excluded on **deployment, ABI stability,
dependency weight, and integration cost** — *not* on any claim that they
foreclose overlays, freehand input, or touch, which they do not. Either may be
reconsidered if all three entrants fail.

---

## The elimination ladder

Cheapest hard disqualifier first, genuinely: every disqualifying check
(pin 14c) is settled before the expensive ranking round runs. All candidates complete round *n*
before any enters round *n+1*.

**Round 0 — accessibility route + desk survey (HARD).** For each candidate
record: current release and date, transitive dependency count, MSRV, and
whether the fill tessellator documents nonzero/even-odd support. Then the hard
part: **demonstrate an accessibility route** — a minimal window exposing one
accessible node whose role and name are **read back through the platform
adapter**, for **every** candidate without exception. Naming an integration
crate does **not** satisfy this round; it is at most a hint about how hard the
demonstration will be, and revision 2's wording that let a crate name
substitute for a readback was a false-pass path. A first-party integration and
a manual `accesskit_winit` wiring are equally acceptable *routes*; only the
readback is the evidence. **Timebox: two working days per candidate**, fixed
here rather than left open — exceeding it is a `FAIL` with the attempt and its
blocking point documented, not an indefinite extension. Rationale: criterion 4
is a hard
criterion, and discovering its absence in round 3 would waste three rounds —
which is the whole claim of "cheapest disqualifier first".

**Round 1 — criterion 1, compound-path fill correctness (HARD).** Each
candidate draws five glyphs from `BravuraGlyphCatalog`'s typed outlines under
pin 4's configuration, in **two check classes testing two different
properties**. All sample points are **derived programmatically** by
point-in-path over the `PathCommand` outline — never chosen by eye — and every
point must lie **≥8 device pixels from any outline edge** at the pinned render
transform, so antialiasing cannot explain any result.

* **Hole checks — `gClef`, `timeSig8`, `accidentalFlat`, `noteheadHalf`.**
  ≥3 must-be-ink and **≥3 must-be-background points, each inside a bounded
  hole** — enclosed by the glyph's outer silhouette yet unfilled — asserted by
  the derivation, not assumed. A background point merely outside the silhouette
  tests nothing, since a renderer that fills holes solid passes it trivially.
  `noteheadHalf` is in this set deliberately: it is a frequently repeated and
  semantically consequential glyph — a filled counter renders half notes as
  quarter notes, a notation error rather than a cosmetic artifact. (It is not
  the single most-drawn glyph; `noteheadBlack` is, and it has no counter.)
* **Disjoint-component check — `fClef`.** It has **no bounded hole** (bowl plus
  two solid dots), so it carries **no background requirement** and tests the
  other half of compound-path correctness: **≥1 ink point inside each of its
  three filled subpaths** — body and both dots — at the same clearance floor,
  each tagged with its `subpath_index` so the oracle proves coverage of every
  component rather than three generic ink points that could all land in the
  bowl. A tessellator that keeps only the largest contour fails here and would
  pass every hole check.

**The oracle's status model must be explicit, not inferred from an absence.**
Each glyph carries a requirement enum (or at minimum `background_required:
bool`) *plus* an overall satisfied status. `fClef` passing with zero background
points is a **satisfied** result under its own requirement class; recording it
only as `background_satisfied = false` would make a correct outcome
indistinguishable from a failed one.

The oracle file, the render transform, and the expected class per point are
committed before any candidate renders. *Fails hard:* **any bounded hole
painted as ink, or any required filled subpath omitted.**

**Round 2 — criterion 3, text (HARD).** W3 §5's five checks against pin 8's
stand-in and pin 10's reference emitter: (1) **faithful consumption** — draws
positioned segments without re-shaping, matching the reference emitter's
rendering of the same run under Ruling A's bounded visual differential;
(2) **fallback, forced** — resolves through the declared chain only, and
reports rather than substitutes an uncovered codepoint; (3) **bidi** — a mixed
Arabic/Latin run itemizes into multiple directional segments, each drawn in
its resolved face at its resolved position; (4) **hit testing at character
granularity** — UTF-8 byte offsets as base index, caret stops at
grapheme-cluster boundaries with bidi affinity; (5) **accessibility** — the run
appears in the tree as its **source string**, not a graphic.
*Outcome rule (pin 14):* the criterion cell is the worst of the five.
**Checks 2 and 5 are in the disqualifying set** — W3 names those two as
disqualifying "regardless of tessellation throughput" — so failing either
eliminates. Failing 1, 3, or 4 marks the cell `FAIL` and is **not**
disqualifying by default, since W3 did not make them so and this contract does
not silently promote them. **What that escalation is, concretely:** the spike
*pauses* and the user issues a recorded waiver or ruling amendment that states
explicitly whether the failed check joins the disqualifying set for this
candidate. Eligibility then follows pin 14(c) from that record. Revision 2
left the user a decision with no effect — the candidate stayed `FAIL` on the
criterion and pin 16 admitted only candidates passing every hard criterion, so
a "keep it" ruling could not reach the tie-break.

**Round 3 — criterion 4, accessibility semantics (HARD).** One score fragment
exposed with **meaningful** semantics. **Precommitted oracle (pin 13):** a
fixed numbered interaction script — key and pointer actions in order — and for
each step the required evidence: accessibility-tree node **role, name, state,
and supported actions**, plus **before/after focus and selection state**, plus
the transcribed spoken output. Orca version, AT-SPI version, and desktop
session are recorded. Transcription alone does not satisfy this round;
navigation, focus movement, selection updates, and command activation must
each be evidenced by tree state, because a stack can narrate while remaining
unusable. *Fails hard:* a tree that exists but conveys nothing — Ruling A
names that outcome as failure, so carrying AccessKit earns no credit.

**Round 3b — damage correctness (HARD, untimed).** Pin 14(c) makes a
candidate-attributed damage failure disqualifying, so it must be settled
*before* the ranking round, not discovered inside it — otherwise the ladder's
"every disqualifier before round 4" claim is false. Each survivor runs pin
11's A/B transition **at every engraving-valid rung** with the byte-identity
oracle, and **nothing is timed**: this round asks only whether correct
per-system damage is implementable in the stack. Every rung, not F1 alone —
revision 4 checked F1 and left an F2–F4 oracle failure to surface *inside* the
ranking round, which is exactly the late hard disqualification this round
exists to prevent, and larger rungs are where multi-system and unowned
handling actually breaks.
**Byte equality is necessary but not sufficient**, so it is paired with a
**reuse assertion**: the candidate's scene cache is instrumented to record
which system representations were rebuilt during the transition, and the
rebuilt set must equal the damage set. Without it a candidate that silently
redraws everything produces a byte-identical target and passes as if it
implemented damage — it would then be caught only by round 4's timings, i.e.
by being slow, which is evidence about performance rather than about
architecture.
Outcome is the damage-**capability** cell (pin 14a), with root-cause
attribution on failure. Still cheap: no measurement protocol applies, so each
rung is one transition rather than 250 timed iterations. Round 4 then times
damage for candidates that already proved it correct.

**Round 4 — criterion 2, staged latency (RANKING, no hard pass).** Survivors
only. Per rung (pin 7), per adapter, under pins 3–5: (a) scene build — IR to
the candidate's draw representation; (b) full paint; (c) **damage repaint**
per pin 11, with its oracle; (d) viewport-culled paint at 1920 × 1080 on the
deciding rung — **with the viewport transform and location committed in
advance** alongside the oracles (pin 13) and the **visible primitive count
reported**, since a size without a position leaves the culled workload
unpinned. Plus the SVG re-measurement of pin 6. **The integrated adapter's
figures decide; the discrete adapter's are reported as headroom** — a budget
met on a Navi 31 says nothing about the machine most users have.
**Every candidate's figures within a round come from the same spike commit**,
recorded in the report, in addition to pin 12's frozen root baseline and the
oracle commits — otherwise the harness itself is an uncontrolled variable
between candidates.

**Round 5 — criterion 5, foreclosure probes (RANKING).** Prototyped, not
assumed, each with a binary pass condition fixed here: **overlay** — a moving
presence cursor drawn in a separate layer while the score's damage set stays
empty (asserted, not observed); **freehand** — a live stroke rendered from a
**committed synthetic pointer trace** (≥1,000 events, replayed at a stated
fixed rate, committed with the oracles under pin 13), on the **integrated
adapter at the deciding rung** under pin 4's configuration.
**Latency is measured from the trace's scheduled injection timestamp to
completion of the first frame containing that event — backlog included.**
Starting the timer when the renderer picks the event up would let a candidate
buffer the whole trace, render it late, drop nothing, and report fast update
work while lagging visibly; that is the failure mode this probe exists to
catch, so the timer origin is the schedule, not the pickup.
**Coalescing is permitted and must be declared:** where a frame covers several
injected events, each of those events is charged that frame's completion time.
**Every sample in the trace must appear in the resulting stroke geometry** —
asserted against the committed trace, not assumed.
Passes if the **p95 of that latency is ≤ 16.7 ms** — the spec's own frame
figure, not an invented one — **and no injected event is dropped or missing
from the stroke**; a single one fails the probe. (Revision 3 pointed at "round
4(c)'s budget", but 4(c) defines a measurement, not a threshold, and this
probe feeds the deciding tie-break.) **touch** — multi-touch events received with
distinguishable pointer ids, and a two-finger pan/zoom driven by them.
Coverage is the count passed (0–3) and feeds tie-break step 2 only.

---

## Blast radius

New `spikes/editor-toolkit/**` (its own workspace, committed `Cargo.lock`,
candidate sub-crates, fixture generator, SVG reference emitter, precommitted
oracle files, and a `DECISIONS.md`); one additive `exclude = ["spikes"]` line
in the root `Cargo.toml`; `spec/RULING_EDITOR_TOOLKIT.md` and this contract.
Nothing else — no `epiphany-core`, no `epiphany-ops`, no `epiphany-bundle`, no
`epiphany-layout-ir`, no `epiphany-glyphs`, no `epiphany-engrave`, no
`epiphany-editor-core`, no `epiphany-editor-gui`, no testkit, no `.tex`, no CI
change, no new workspace member, no golden touched or blessed. Scoped against
pin 12's baseline commit.

## Gate (actual output)

The root workspace must be **unchanged in behavior**: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
--workspace`, conformance suite and requirement counts **reported as observed**
and unmoved, all GUI goldens byte-identical, layout canonical bytes
byte-identical. The one-line `exclude` is the **only modification to
pre-existing workspace configuration or production files** against the
baseline — the contract, the ruling, and the spike tree are additions at the
repository root too, so "the only root diff" would have been wrong — and the
gate proves it changed nothing.

The spike workspace is gated **separately and explicitly**, since root
`--workspace` does not reach it: `cargo fmt --check` and `cargo test --locked`
inside `spikes/editor-toolkit/`, reported with its own output. Clippy is
advisory there, not `-D warnings`: throwaway code held to the production bar
wastes the reviewer's attention, and pin 1 guarantees none of it ships.

## Deliverable

`spec/RULING_EDITOR_TOOLKIT.md` — the verdict. A candidate × check matrix with
every capability result carrying `PASS` / `FAIL` / `NOT RUN` and its evidence,
its timing carrying `measured` / `NOT RUN` **separately** (pin 14a–b),
criterion cells derived from it, an explicit **disqualifying-set column**
recording eligibility (pin 14c), **no PASS/FAIL cell for criterion 2**, and a
version row answering Ruling A criterion 6 in prose (pin 15); round 4's tables
in full with adapter, configuration, rung, per-type primitive and path-command
counts, statistic, and iteration counts on every figure; the pin-12 baseline
SHA, the spike commit per round, and the oracle commits; the tie-break applied
on the record per pin 16, including any waiver or amendment that changed
eligibility; the named foreclosures the choice accepts; and round 2's findings
routed back to the `ResolvedText` amendment. Plus the **no-winner branch** if
it fires, in either of its two forms: **disqualification** — which
disqualifying check killed which candidate — or **ranking-loss wipeout**,
where no candidate rendered the deciding rung so step 1 retained nobody
(pin 16).
Both end in a recommended widening; the second is not a lesser outcome and is
not reported as an inconclusive run.

## Report

Per round, as every tranche: files + summary, exact measured values with their
full labels, every `NOT RUN` with the reason it could not run, gate output for
both workspaces, deviations flagged explicitly. **The user reviews each
precommitted oracle before candidates run against it, and each measured table
before it enters the ruling.**
