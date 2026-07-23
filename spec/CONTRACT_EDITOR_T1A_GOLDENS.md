# Contract: Editor T1a — the visual golden harness

Repo root `/home/jeans/Repos/active/epiphany`. The plan is
`spec/PLAN_EDITOR_APP.md`; read its §Ruling C (granted, as amended), §Ruling A
(granted as amended 2026-07-23), and §5 in full before editing anything.
Ruling C independently authorizes this tranche. Ruling B remains blocked and
Ruling D remains conditional — **nothing in this contract touches a document,
a bundle, or `editor-core`**.

This tranche closes the repo's last unverified surface: the GUI's rendered
output has never been seen by any gate (plan Fact 1). It adds **pixel-level
golden baselines of the resvg-rasterized score** — the exact surface the demo
GUI displays — as ordinary `#[test]`s in `epiphany-editor-gui`, compared as
**decoded pixels, never encoded files**, with reviewable failure artifacts in
CI. The resolver tranche (`CONTRACT_PUSH4B_RESOLVER.md`) is in flight; plan
§5's parallel-safety rules are binding on every commit.

## Execution model — who does what

Implementation is dispatched to **subagents (Sonnet-tier), one per work
packet** (§Work packets), each with a file-scoped brief quoting this
contract's relevant section verbatim. The **coordinator (Fable)** reviews
every packet at line level before anything lands: re-runs the gate itself,
verifies each mutation's evidence (a worker's own report is never sufficient),
checks that every cited file:line says what the code claims, and rejects any
packet that drifts outside its blast radius. The **user's deep-dive points**,
named in advance:

1. **This contract's sign-off** (precedes any dispatch).
2. **Baseline review.** The four PNGs are the first visual record of this
   editor's output in the project's history, and a golden locks whatever it
   sees — **bugs included**. Before the blessing commit, the user visually
   inspects each baseline: clef and staff lines present, ledger lines correct,
   noteheads on the intended positions, slur curves plausible, system breaks
   where casting-off put them. An unreviewed baseline is an unverified claim
   wearing a checkmark.
3. **The final report** (§Report).

## Blast radius

* `crates/epiphany-editor-gui/src/main.rs` — minimal: a `#[cfg(test)] mod
  goldens;` declaration and, if needed, `pub(crate)` visibility on the
  existing `rasterize` helper (`main.rs:160`). No behavior change to the
  running binary.
* `crates/epiphany-editor-gui/src/goldens.rs` — NEW, `#[cfg(test)]`-only: the
  comparator, bless machinery, and the golden tests.
* `crates/epiphany-editor-gui/goldens/*.png` — NEW baseline images.
* `crates/epiphany-editor-gui/DECISIONS.md` — NEW (this crate's first):
  records this tranche's calls (§What to build, item 4).
* `crates/epiphany-editor-gui/Cargo.toml` + `Cargo.lock` — ONLY if the `png`
  dev-dependency fallback proves necessary (§prohibitions).
* `.github/workflows/ci.yml` — exactly **one additive step** in the existing
  `editor-gui` job (`ci.yml:109`): upload `target/golden-failures/**` with
  `if: failure()`. Nothing else in the file moves.

You touch no other crate and no other file. In particular
`crates/epiphany-core/**` is being worked in parallel by the resolver tranche
— do not go near it — and `epiphany-testkit` is frozen for this tranche
(gate promotion is T2, deliberately).

## Central prohibitions

* **Counts do not move.** Requirement labels stay **212 / 282 / 282**
  (`requirement_labels.rs:12-14`); the conformance suite stays **8/8**
  (`conformance_suite.rs:202`). If either moves, stop and report.
* **No existing golden or fuzz digest is re-blessed.** This tranche *adds*
  new PNG baselines; it modifies nothing that exists. The resolver's
  zero-churn tripwire must stay unambiguous.
* **Comparison is decoded pixels only** — dimensions + raw RGBA. Comparing
  encoded PNG bytes is the exact defect Ruling C's amendment exists to
  prevent (it locks encoder behavior and churns while pixels are identical).
* **No new runtime dependencies.** PNG encode/decode goes through the
  resvg-bundled tiny-skia (`png` is already in `Cargo.lock` via that tree).
  If the feature is not exposed through resvg's re-export, the fallback is a
  `png` **dev-dependency** (the version already in the lock) — never `image`,
  never a runtime dep.
* **Never bless to make a test pass.** A diff is a finding: it is reviewed
  (user deep-dive point 2 for new baselines; ordinary review for changes)
  before any re-bless. The bless mechanism exists for *deliberate* baseline
  updates only.
* Land only green (`cargo test --workspace` clean); whoever lands second
  rebases.

## What to build

### 1. The comparator and bless machinery (`src/goldens.rs`)

* **Rasterize** through the binary's existing `rasterize` fn (`main.rs:160`)
  — the same code path the GUI displays, `GlyphMode::PathOutline` (the
  default; no fonts), `RenderOptions { px_per_staff_space: 12.0, .. }` (the
  demo's default).
* **Compare**: decode the committed baseline PNG; assert dimensions equal,
  then raw RGBA bytes equal. On mismatch, write `actual.png`,
  `expected.png`, and `diff.png` (per-pixel highlight) to
  `target/golden-failures/<test_name>/` and name all three paths in the
  panic message.
* **Bless**: `EPIPHANY_BLESS_GOLDENS=1` writes/overwrites the baseline
  instead of comparing. Policy stated in a comment at the definition: a
  bless is a reviewed decision, never a fix.
* **The comparator's own unit tests** (value-asserting):
  * identical pixmaps pass;
  * a single altered pixel fails AND produces all three artifacts at the
    named paths;
  * **re-encoding the baseline PNG with different encoder settings still
    passes** — the decoded-pixel contract made executable;
  * mismatched dimensions fail before any pixel comparison.

### 2. The four golden states

Each test builds its session headlessly — `EditorSession::open(fixture,
Box::new(Engraver::default()))`, exactly the demo's open path (`main.rs:197`)
— asserts the *session-level values* first, then locks the pixels. Each
state renders **twice**, asserting the two SVG strings and the two pixmaps
are identical (the determinism double) before comparing to the baseline.

* **G1 — as opened**: `ten_measure_single_staff(0)`, rasterized, compared to
  `goldens/ten_measure_open.png`.
* **G2 — after a scripted pencil insert**: the target is derived through the
  session's own inverses, never a magic constant — pick the world point via
  `staff_pitch_at` / `position_at` / `default_grid_at`, **assert the resolved
  pitch and snapped beat values** (e.g., the intended nominal/octave and
  position as exact rationals), then `insert_note_at`, re-render, compare to
  `goldens/ten_measure_insert.png`.
* **G3 — after undo of that insert**: **no third baseline.** Assert the
  post-undo raster equals **G1's baseline** byte-for-byte in decoded RGBA —
  undo returns the pixels, not just the model. This is the strongest test in
  the tranche; it must reuse G1's file, not a copy.
* **G4 — casting-off**: `ten_measure_with_slurs(0)`
  (`fixtures.rs:777`) — multi-system layout **and** the
  cross-system slur-split path (`casting.rs:2262`), the layout path real
  documents take. Compared to `goldens/ten_measure_slurs_castoff.png`. The
  test additionally asserts the resolved layout actually produced **more
  than one system** (value assertion — if casting-off stops triggering, this
  must fail as a value error, not as a mysterious pixel diff).

### 3. The CI step

One additive step in the `editor-gui` job, after the test step:

```yaml
- name: Upload golden failure artifacts
  if: failure()
  uses: actions/upload-artifact@v4
  with:
    name: golden-failures
    path: target/golden-failures/
    if-no-files-found: ignore
```

Nothing else in `ci.yml` changes. (Match the `actions/upload-artifact`
major version already used elsewhere in the file, if any; otherwise v4.)

### 4. `DECISIONS.md` (the crate's first)

Record: the decoded-pixel comparison contract and why (Ruling C amendment);
the bless mechanism and its policy; the G3-reuses-G1 design; the casting-off
fixture choice (`ten_measure_with_slurs` for the slur-split coverage); the
determinism basis (PathOutline = no fonts; pure-Rust resvg/tiny-skia; Linux
CI and dev) — and the standing consequence: **baselines pin the raster
stack**, so any `Cargo.lock` movement of `resvg`/`tiny-skia`/`png` is a
golden-review event, never a silent re-bless.

## Work packets (subagent dispatch)

Sequential where stated; each packet's brief includes this contract's
matching section verbatim, the blast-radius list, and the prohibitions.

* **W1 — comparator + bless + its unit tests** (`goldens.rs` scaffolding;
  no baselines yet). Sonnet. Exit: comparator tests green, mutations
  verified (§Verification), no other file touched.
* **W2 — the four golden states + determinism doubles** (depends on W1).
  Sonnet. Exit: tests written and value-asserting; baselines generated via
  the bless mechanism but **committed only after user review** (deep-dive
  point 2); mutations verified.
* **W3 — CI step + DECISIONS.md** (after W2's baselines are blessed).
  Sonnet. Exit: the single-step `ci.yml` diff; DECISIONS.md complete.

The coordinator reviews each packet's diff line-by-line, independently
re-runs its tests and mutations, and only then stages it. Baselines go to
the user between W2 and W3.

## Verification

**Mutation-verify every new test.** Assert the anchor text is present before
substituting (a no-match substitution is a no-op mutation indistinguishable
from a passing test); restore by reversing the exact substitution — **never
`git checkout`**. Report each mutation and the test that died. The named
mutations, minimum set:

1. Comparator → compare encoded PNG bytes instead of decoded RGBA: the
   re-encoding unit test must die.
2. Comparator → skip the dimension check: the dimension-mismatch test must
   die.
3. Comparator → suppress artifact writing on mismatch: the
   artifacts-produced test must die.
4. G2 → skip the `insert_note_at` call (render the unedited session): G2
   must die on pixels AND its pitch/beat value assertions must be shown to
   still pass (proving the pixel lock catches what value assertions cannot).
5. G3 → skip the `undo()` call: G3 must die.
6. G4 → substitute `ten_measure_single_staff(0)` for the slurs fixture: G4
   must die (on the system-count value assertion or pixels — report which).
7. Determinism double → perturb the second render (a different
   `px_per_staff_space`, or append a byte to its SVG string before
   rasterizing): the double's equality assertions must die. (An earlier
   draft said "make the second render reuse the first's output" — that
   mutation passes vacuously and proves nothing; corrected at W2 dispatch.)

Then the full gate, reporting actual commands and actual output:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets` → 0 warnings
3. `cargo test --workspace` → 0 failed
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
5. `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8
6. `cargo test -p epiphany-testkit --test requirement_labels` → counts
   unchanged at 212 / 282 / 282.

**Expected churn: three NEW PNG files and nothing else** (four golden states,
but G3 reuses G1's baseline by design). If any existing
golden, digest, count, or gate number moves, stop and report rather than
re-blessing.

CI pins clippy at 1.95.0 and the MSRV at 1.85; `epiphany-editor-gui` is
excluded from the MSRV jobs and checked on pinned stable (`ci.yml:99,109`) —
this tranche keeps that structure untouched.

## Citations

This is product-side work; it adds no requirement labels and should cite no
`req:*` string. If a doc comment nonetheless cites one, read the requirement
first and confirm it says what the sentence claims (the P13-S9 rule). Plan
citations (`PLAN_EDITOR_APP.md` §Ruling C, §5, Fact 1, Fact 7) are the
governing references.

## Do not

* Touch `epiphany-core`, `epiphany-testkit`, any `.tex`, any other crate, or
  any `DECISIONS.md` but the new `epiphany-editor-gui/DECISIONS.md`.
* Promote a conformance gate, move 8/8 or 212/282/282, or re-bless anything
  existing.
* Compare encoded PNG bytes; add `image` or any runtime dependency.
* Commit a baseline the user has not visually reviewed.
* Change the demo binary's behavior, its egui pin, or its rendering path —
  this tranche observes; it does not refactor.
* Restore a mutation with `git checkout`. Reverse the substitution.

## Report

State: the comparator contract and its unit tests; each golden state's value
assertions and file; the G3=G1 equality design; every mutation and the test
that died (including mutation 4's two-part evidence); the baseline images'
human-review record; the exact `ci.yml` diff; whether the `png` dev-dep
fallback was needed; actual gate output; confirmation that no existing
golden, digest, or count moved; and anything you chose not to do and why.

## Next: T2 promotion, and what this tranche feeds

Promotion of this harness to a numbered `[9/9]` conformance gate is **T2**,
after the resolver lands (the 8/8 count is frozen until then). The baselines
also become the Ruling-A cross-check for the T4 canvas — as geometry/scene
equivalence plus a bounded visual differential, NOT pixel equality (plan
§Ruling A). T1b remains blocked per the plan; nothing here may anticipate it.
