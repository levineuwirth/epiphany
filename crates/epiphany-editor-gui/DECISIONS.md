# epiphany-editor-gui — Decisions

This crate's first `DECISIONS.md`. Records the calls made standing up the T1a
visual golden harness (`spec/CONTRACT_EDITOR_T1A_GOLDENS.md`; plan
`spec/PLAN_EDITOR_APP.md` §Ruling C, granted as amended 2026-07-23) — pixel-level
golden baselines of the resvg-rasterized score, as ordinary `#[test]`s in this
crate, comparing decoded pixels rather than encoded PNG files. The harness lives
in `src/goldens.rs` (`#[cfg(test)]`-only; never ships in the built binary) and
`goldens/*.png`.

## 1. The comparison contract: decoded pixels, never encoded PNG bytes

A golden test decodes the committed baseline PNG and compares **dimensions
first, then raw RGBA bytes** exactly — never the encoded file bytes. Plan
§Ruling C (granted as amended 2026-07-23) is explicit about why: comparing
encoded bytes would also lock the PNG encoder's own behavior (compression
level, filter choice, chunk layout, …), so the goldens would churn on an
encoder change even when every rendered pixel is identical — exactly the
defect the amendment exists to prevent. `goldens.rs`'s
`reencoding_with_different_settings_still_passes` test makes this guarantee
executable rather than merely asserted: it encodes one pixmap twice with
deliberately different `png` encoder settings (filter type, compression
level) via the `png` crate directly (decision 7), confirms the two encoded
byte strings differ, and confirms the comparator accepts either as a baseline
for the same pixels.

On a mismatch the comparator writes failure artifacts to
`target/golden-failures/<name>/` and names them in the panic message: a
dimension mismatch writes `actual.png` + `expected.png` (no `diff.png` — a
per-pixel map is not meaningful across differing dimensions, and the
comparator fails there *before* any pixel is compared); a pixel mismatch
writes all three, `diff.png` being a per-pixel highlight of exactly the
differing pixels. Decision 8 covers how that record survives past the
ephemeral CI runner.

## 2. The bless mechanism and its policy

`EPIPHANY_BLESS_GOLDENS=1` makes `assert_golden` write/overwrite the baseline
unconditionally instead of comparing (creating `goldens/` if needed). This is
a **reviewed decision**, stated at the function's definition: never a
mechanism for turning a red test green, only for accepting a new or
deliberately-changed raster after a human has looked at it. The three initial
baselines (`ten_measure_open.png`, `ten_measure_insert.png`,
`ten_measure_slurs_castoff.png`) are the first visual record of this editor's
output in the project's history — an unreviewed baseline is an unverified
claim wearing a checkmark (plan §Ruling C, user deep-dive point 2) — and were
**visually reviewed and approved by the user on 2026-07-23** before being
committed. Any future re-bless is the same kind of event: a diff is a finding,
reviewed before it is accepted, never a fix applied to make a failing test
pass.

## 3. G3 reuses G1's baseline and deliberately bypasses the bless path

There is no `ten_measure_undo.png`. G3 (post-undo) asserts the raster equals
**G1's own baseline file** byte-for-byte in decoded RGBA — undo must return
the pixels, not just the model — by calling `assert_golden_at` directly
against `baseline_path("ten_measure_open")`, never `assert_golden`. Routing G3
through `assert_golden` would let `EPIPHANY_BLESS_GOLDENS=1` overwrite
`ten_measure_open.png`: an initial bless run performed before undo is known to
be correct would silently bless a broken undo's post-undo pixels as the new
"as opened" baseline, after which every future run would compare undo against
its own bug instead of against G1. Bypassing `assert_golden` removes that
failure mode entirely — this comparison always compares, and can never bless
itself green. (G3 replays G2's scripted insert on its own fresh session
rather than continuing G2's, so the two tests stay independent of each
other's mutations.)

## 4. The casting-off fixture, and what its system-count assertion actually guards

G4 uses `ten_measure_with_slurs(0)` (`fixtures.rs:777`) specifically for its
three slurs, one of which is forced across a system break — the cross-system
slur-split path (`casting.rs:2262`) — the layout path real documents take,
not exercised by any single-system fixture.

An empirical finding changed the contract's original framing: at
`px_per_staff_space: 12.0`, **`ten_measure_single_staff(0)` itself already
casts off into two systems** (ten measures of quarter notes don't fit one
line at this scale) — casting-off is not unique to the slurred fixture. All
three baselines (G1/G2/G3's shared raster and G4's) are therefore
multi-system layouts. Consequently G4's `system_count > 1` assertion does not
guard "this is the slurred fixture, not the plain one" — a mutation
substituting `ten_measure_single_staff(0)` for the slurs fixture leaves the
system count at 2 either way, so that assertion still passes under the
mutation. What it guards is the more durable claim it was written for:
*casting-off itself has not stopped triggering* at this geometry — if it ever
did, this would fail as a named system-count value error rather than a
mystery pixel diff. The mutation instead dies on the golden pixel comparison
(dimensions differ: G4's baseline is taller, carrying the slur curves and the
wider slurred content), which is where the fixture-identity guarantee
actually lives.

## 5. Known engraving gaps, locked knowingly

The goldens lock the **current, real** output of the Minimal-tier engraver —
bugs and rough edges included, by design (plan §Ruling C: "a golden locks
whatever it sees"). Two are worth naming explicitly, both blessed by the user
with this record in hand on 2026-07-23:

- **No clef restatement on second and subsequent systems.** A new system does
  not redraw the governing clef at its start, unlike conventional engraving
  practice.
- **The second system's spacing is noticeably denser than the first's.** The
  casting-off balance between systems is not yet even.

Both are engraving-track items (`epiphany-engrave`'s casting/spacing passes;
plan §3.7), not `epiphany-editor-gui` work — this crate only observes the
rendered score, it does not engrave it. When engraving improves, the fix will
change these three PNGs' pixels, and the golden harness will surface that as
a reviewable diff to bless deliberately. That is the harness doing its job,
not a defect of it.

## 6. The baselines pin the raster stack

Determinism basis: `GlyphMode::PathOutline` uses no fonts (inlined Bravura
outline paths), and `resvg`/`tiny-skia` are pure Rust with deterministic
rasterization; CI and dev are both Linux. Standing consequence: **the
baselines pin the raster stack**, so any `Cargo.lock` movement of
`resvg`, `tiny-skia`, or `png` is a golden-review event — the diff must be
inspected and deliberately re-blessed — never a silent re-bless folded into
an unrelated dependency bump. If cross-platform rasterization drift is ever
observed, the fallback is a bounded per-pixel tolerance, recorded as that
decision when it happens, not pre-engineered here.

## 7. The `png` dev-dependency

`tiny_skia::Pixmap::encode_png`/`decode_png` (used throughout the comparator
and the bless path) expose no encoder configuration — every call from a given
pixmap produces byte-identical output. Proving decision 1 executable (that
two *differently*-encoded PNGs of the same pixels both compare equal) needs
an encoder with configurable filter/compression settings, which only the
underlying `png` crate exposes directly. `png` is therefore a **dev-only**
dependency, used exclusively inside `goldens.rs`'s own test module — never in
the comparator or bless code paths, which stay on
`resvg::tiny_skia::Pixmap::{encode_png,decode_png}` exclusively, and never at
runtime.

Declared as a caret requirement (`png = "0.17.16"`), not an exact `=` pin (a
W1-review amendment): `tiny-skia` 0.11.4 already resolves `png 0.17.16` in
`Cargo.lock`, so the caret requirement is a dev-only edge onto that same
dependency-tree node today — and stays unified with it after any future
`tiny-skia` bump that moves its own `png` requirement forward, rather than
forking a second `png` version into the tree (which an exact pin would force
the day `tiny-skia` moves).

## 8. CI failure artifacts

A comparator panic's assertion message names `actual.png`/`expected.png`/
`diff.png` paths under `target/golden-failures/<name>/` — useful for local
reproduction, but those paths name the CI runner's own ephemeral filesystem,
gone the moment the job ends. The reviewable record is the `editor-gui` job's
one additive step (`.github/workflows/ci.yml`): an `if: failure()`
`actions/upload-artifact@v4` step uploading `target/golden-failures/` as the
`golden-failures` artifact (`if-no-files-found: ignore`, since it produces
nothing on a green run). This is the tranche's only CI change; nothing else
in `ci.yml` moves.
