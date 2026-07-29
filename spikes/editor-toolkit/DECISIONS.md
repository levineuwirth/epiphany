# T4 toolkit spike — decisions and findings log

Governed by `spec/CONTRACT_EDITOR_T4_SPIKE.md`. This file records
implementation decisions and named deviations, per round.

## Round 0 — accessibility route + desk survey

**Environment prerequisite, not obvious from the contract:** on this
machine (sway, AT-SPI2 via `at-spi-bus-launcher` + `at-spi2-registryd`),
AT-SPI application registration is gated behind two settings that are
*off* by default even though the bus itself is always up:

```
gsettings set org.gnome.desktop.interface toolkit-accessibility true
gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus \
  --method org.freedesktop.DBus.Properties.Set org.a11y.Status \
  ScreenReaderEnabled "<true>"
```

Without both, `Atspi.get_desktop(0)` enumerates **zero** applications even
while a probe process is alive, rendering, and actually connected to the
AT-SPI D-Bus (confirmed separately via `busctl --address
unix:path=$XDG_RUNTIME_DIR/at-spi/bus list`). This is a real environment
absence, not a candidate defect, and any later round run in a fresh
sandbox/session must redo both steps before trusting a `NOT RUN` verdict on
accessibility.

**Verifier substitution.** The contract's Round 0 evidence rule allows "a
small verifier binary using the `atspi` crate, **or an equivalent AT-SPI
client**". `a11y-verifier/verify.py` uses `gi.repository.Atspi` (the
official AT-SPI2 GObject-introspection binding — the same library behind
Orca and Accerciser) instead of the Rust `atspi` crate. This was a
deliberate substitution: the Rust crate's async zbus proxy API would have
had to be learned from source rather than from any working example, and
`pyatspi` was already confirmed reachable on this machine. It is a
standalone process, external to every probe, and performs a real tree walk
from the AT-SPI registry — it satisfies "a real client query of the tree",
not "printing your own struct".

**C1 (egui).** First-party route, the full chain being
`eframe` → `egui-winit` → `accesskit_winit` → `accesskit_unix`: `egui-winit`'s
`accesskit` feature is literally `dep:accesskit_winit`
(`egui-winit-0.35.0/Cargo.toml:55`), on by default through `eframe` in 0.35, and
`accesskit_winit` delegates to the platform crate. So C1 gets the
window-lifecycle handling that C3's bypass would have forfeited. No manual
wiring was needed. `probe-egui` draws one button; readback: **PASS**. See
`round0-evidence/c1-egui-readback.txt`.

**Carry forward — C1's frame node is unnamed.** C1's readback path is
`application:'probe-egui' / frame:'' / button:'EpiphanyProbeButton'`, where C2
and C3 both name their frame. The window title does not reach the AT-SPI frame
node under `eframe` 0.35's default wiring. **Non-disqualifying** — round 0
requires one node with a role *and* a name, and the button carries both — but
it is a real gap: a screen-reader user hears an unnamed window. Round 3
(accessibility semantics) must check it, since window identity is part of
navigation, and it should not be rediscovered there as a surprise.

**C2 (vello + winit).** Manual `accesskit_winit` route, exactly as named by
the contract: `probe-vello` builds the accessibility tree by hand
(`accesskit::TreeUpdate`) and drives it through
`accesskit_winit::Adapter::with_event_loop_proxy`, wired into the same
`winit::application::ApplicationHandler` that owns the vello
`RenderContext`/`Renderer`/`Scene` (the vello render pass is real, not a
stub — it draws a filled rounded rect every frame, following vello's own
`examples/simple` pattern at `linebender/vello@main`). Readback: **PASS**.
See `round0-evidence/c2-vello-readback.txt`.

**C3 (iced) — ROUND-0 RESULT: FAIL. Eliminated at round 0, adjudicated
2026-07-28 by coordinator review; no waiver sought or granted.** The initial
report recorded this as "PASS with a flagged deviation". That adjudication was
wrong and is corrected here. Under pin 14(c) C3's disqualifying set is not
passed; keeping it would require an explicit recorded ruling amendment, which
was declined.

**Dual attribution, because the two failures are different in kind.**

*Candidate limitation — this alone fails the round.* iced 0.14 ships **no
accessibility integration at all**: `accesskit` appears in no iced crate
manifest (verified across every `iced*` crate in the 0.14 tree). And its
**stock runner** exposes to application code neither a
`winit::event_loop::ActiveEventLoop` nor a pre-visibility
`winit::window::Window`; both appear only inside `iced_winit`'s own private
`ApplicationHandler` impl, with `create_window` at `iced_winit-0.14.0/src/lib.rs:350`
inside iced's runner. Every `accesskit_winit::Adapter` constructor requires
both and panics if the window is already visible.
**Scope this to the stock runner, deliberately:** `iced_winit`'s own docs offer
a `conversion` module "for users that decide to implement a custom event loop",
so a hand-built shell carrying a real route remains **conceivable but
unproven** — and it would mean owning the shell. Upstream iced #552 remains
open. "Provably closed" applies to the stock runner, not to iced in principle.

*Probe-design defect — why the first report read PASS.*
`accesskit_unix::Adapter::new()` takes **no window handle**, only handlers, and
registers with AT-SPI from process identity
(`accesskit_unix-0.22.1/src/context.rs`; `app_name()` reads
`std::env::current_exe()`). `probe-iced` therefore registered a **hand-built
static tree**, decoupled from iced's window, focus, and event lifecycle, with
every action discarded — while `view()` happened to label its button
identically, which is what made the transcript read as though iced produced it.
**Deleting iced from the probe would produce the identical readback.** That is
the disqualifying fact: round 0 asks whether the *candidate* exposes a route,
and a process-level side channel answers a different question. That
`accesskit_unix` sits one layer beneath `accesskit_winit` does not make it a
route *for the candidate* — that was the reasoning error, and it is recorded as
a probe defect rather than folded into the candidate's result.

Evidence is preserved rather than rewritten: `round0-evidence/c3-iced-readback.txt`
keeps the verifier's factual `READBACK: PASS` under a `ROUND-0 RESULT: FAIL`
annotation, so the false positive stays visible alongside its adjudication.

One consequence survives the corrected verdict and is worth carrying, because
it would apply to any future hand-built route: bypassing `accesskit_winit`
forfeits that crate's window-lifecycle handling — deactivation on window close,
multi-window disambiguation, focus-driven activation. Any real iced integration
would have to build and maintain that wiring itself rather than inheriting it,
which is a maintenance-surface fact, not merely a round-0 curiosity.

## Round 0 — desk survey

All version/date/MSRV figures were fetched live (crates.io API + GitHub
`Cargo.toml` at the released tag), not from memory or the contract's
2026-07-23 snapshot. See the Round 0 report for the full table.

Fill-rule documentation, quoted verbatim from source (not inferred from
behavior — that is round 1's job):

- **C1 (`lyon_tessellation` 1.0.20, via `lyon_path` 1.0.19):**
  `pub enum FillRule { EvenOdd, NonZero }`, with
  `DEFAULT_FILL_RULE: FillRule = FillRule::EvenOdd` and an explicit
  `is_in(winding_number)` implementation for both.
- **C2 (`peniko` 0.6.1, vello's fill-style type):** `pub enum Fill { NonZero, EvenOdd }`,
  each with a full doc comment ("All regions where the winding number of
  the path is not zero will be filled" / "... is odd will be filled").
- **C3 (`iced_graphics` 0.14.0, `geometry::fill`):**
  `pub enum Rule { NonZero, EvenOdd }`, doc pointing at the SVG
  `fill-rule` spec, default `NonZero`.

All three document both rules explicitly. No candidate is eliminated on
this desk-survey item; round 1 is where it is actually tested.

## Round 1 — the precommitted oracle: SUPERSEDED FIRST PASS

> **SUPERSEDED DISCOVERY — NEVER COMMITTED.** This section records the
> four-glyph oracle built against the pre-Revision-6 contract, kept because its
> two findings are what *caused* the amendment. It was never committed and is
> not the oracle any candidate renders against; **the authoritative account is
> the Revision 6 amendment section below** (five glyphs, two check classes,
> explicit status model). In particular, `fClef`'s `background_satisfied =
> false` below is the superseded encoding — under Revision 6 `fClef` is a
> *satisfied* disjoint-component result — and every "committed" in this section
> describes an intent that this pass never reached.

**This is the oracle only — no candidate has rendered against it yet.**
`round1-oracle/` (new spike-workspace member) derives, for `gClef`, `fClef`,
`timeSig8`, `accidentalFlat` from `BravuraGlyphCatalog::render_data`: a
pinned staff-space -> device-pixel transform, Bezier-flattened outlines
(recursive de Casteljau, 0.0005 staff-space flatness tolerance), an even-odd
/ nonzero point-in-path classifier, and a grid search (0.01 staff-space
step) that derives >=3 ink and >=3 bounded-hole background sample points per
glyph, each with >=8 device-px clearance from any outline edge, entirely
programmatically — no coordinate here was chosen by eye. Output:
`round1-oracle/oracle.json` (committed, machine-readable) and
`round1-oracle/ORACLE_SUMMARY.md` (human-readable). **No rendering,
tessellation, or windowing crate is a dependency of this crate** — see its
Cargo.toml and module doc comment.

**Transform (pinned):** `device = (staff.x * scale + tx, ty - staff.y *
scale)`, `scale = 100` device px per staff space, `tx`/`ty` center each
glyph's own flattened bounding box in the pin-4 1920x1080 target — one
uniform rule applied identically to all four glyphs, not a per-glyph fudge.

**Subpath counts.** All four match the contract's recorded starting point
exactly: `gClef` 4, `fClef` 3, `timeSig8` 3, `accidentalFlat` 2 (asserted in
`subpath_counts_match_the_recorded_starting_point`).

**Finding — `fClef` has no bounded hole at all.** Its three subpaths (the
solid clef bowl, area 2.534, plus two disjoint dot subpaths, areas 0.153 and
0.148) are not nested — the search finds **zero** grid hits inside the outer
contour and outside the even-odd fill, at both the main search's resolution
and an independent finer corroboration (0.005 staff-space step, no
clearance floor at all;
`finding_corroboration::fclef_has_no_bounded_hole_at_any_resolution_or_clearance`).
Per Round 1's hole-check clause ("asserted by the derivation, not assumed"), no fallback/relaxed point was substituted:
`fClef`'s oracle carries 3 ink points and **zero** background points, and
`background_satisfied = false` is recorded in `oracle.json` alongside the
diagnostic. The other three glyphs each got 3 ink + 3 background points with
no relaxation needed (thousands of qualifying grid candidates for each).

**Finding — even-odd and nonzero agree on every hole point found.** Mutation
(b) (flip even-odd to nonzero on the selected hole points) was run for
`gClef`, `timeSig8`, `accidentalFlat`'s hole points and **none reclassify**:
Bravura's outer contour and its hole wind in *opposite* directions (the
well-formed-font convention), so nonzero winding cancels to 0 at exactly the
points where the even-odd crossing count is 2 — both rules correctly exclude
the hole. This is real information about the font data (recorded, not
smoothed over), and it means the literal "flip the rule" mutation does not,
for this data, demonstrate that fill-*rule* choice is load-bearing. A
supplementary mutation
(`mutation_b_supplement_hole_points_reclassify_under_naive_outer_only_fill`)
demonstrates the property the check actually needed — that respecting inner
subpaths at all is load-bearing — by showing every selected hole point *is*
inside a naive outer-contour-only fill (what a renderer ignoring holes
entirely would wrongly paint as ink) while being outside the real
whole-outline fill.

Full mutation evidence (a/b/b-supplement/c) is in
`round1-oracle/src/lib.rs`'s `tests` and `finding_corroboration` modules, run
via `cargo test -p round1-oracle -- --nocapture`.

## Round 1 — Revision 6 amendment (five glyphs, two check classes, status model)

**Both findings above changed the contract, not just this oracle.**
`fClef` having no bounded hole and criterion 1's fill-*rule* framing being
moot on Bravura's correctly (oppositely) wound contours were reported as
findings against the pre-Revision-6 contract; the coordinator's response was
to amend `spec/CONTRACT_EDITOR_T4_SPIKE.md` to Revision 6 rather than treat
`fClef` as a shortfall. Round 1 is now **five glyphs in two check classes**,
and this oracle is amended to match (uncommitted diff on top of the original
oracle; the mechanics below are unchanged — same flattening, same
point-in-path classifier, same transform, same clearance floor — only the
requirement model and the glyph set changed):

- **Bounded-hole check** (unchanged mechanics): `gClef`, `timeSig8`,
  `accidentalFlat`, and now **`noteheadHalf`** (new: two subpaths, measured
  `ring_signed_areas = [0.903, -0.368]` — the authoritative figure, now also
  what the contract records; an earlier `[0.902, -0.367]` came from a coarser
  fixed-step flattening). Still >=3 ink + >=3 background points per glyph, every
  background point inside a bounded hole.
- **Disjoint-component check** (new): `fClef` alone, no longer folded into
  the bounded-hole class it never actually belonged to. It carries **no
  background requirement** — that is its design (bowl plus two solid,
  disjoint dots) — and instead requires >=1 ink point inside **each** of its
  three filled subpaths, each point tagged with `subpath_index` so the
  oracle proves coverage of every component rather than three ink points
  that could all land in the bowl. Verified two ways: a finer (0.005
  staff-space step) grid corroboration (renamed
  `fclef_has_zero_bounded_hole_grid_hits_at_a_finer_0_005_grid`, since the
  old name overclaimed "any resolution" for what was actually one grid) and
  a genuinely resolution-independent topological check,
  `subpaths_are_mutually_non_nested` /
  `fclef_subpaths_are_topologically_non_nested`, which asserts no subpath's
  vertices lie inside another subpath — a property of the flattened polygon
  itself, not of any sampling step, and what actually justifies the
  stronger claim.

**Status model.** `GlyphOracle` now carries an explicit `Requirement` enum
(`BoundedHole` / `DisjointComponents`), per-requirement booleans
(`background_required`/`background_satisfied`,
`subpath_coverage_required`/`subpath_coverage_satisfied`), and one overall
`satisfied: bool` — the field to read. `fClef` is `satisfied = true` with
`background_required = false` and zero background points; a bounded-hole
glyph that failed to find enough background points would be
`satisfied = false`. The two are no longer distinguishable only by an absent
field, which was the defect the amendment closes.

**Fill-rule equivalence, now asserted, not merely reported.** The original
oracle's `mutation_b_...` test printed `reclassifies_under_nonzero=false` for
every hole point without asserting on it. `derive_bounded_hole_oracle` now
asserts `!ev.nonzero_filled` for every selected background point at
derivation time (fails loudly if the opposite-winding assumption is ever
false for a glyph's real data), and
`mutation_b_nonzero_rule_equivalence_on_hole_points_is_asserted` asserts the
same independently at the test level. `ring_signed_areas` — the measured
signed area of every subpath, in ring order — is now recorded on every
glyph's oracle output (`oracle.json` and `ORACLE_SUMMARY.md`), not just
observed in this log: `gClef [8.702, -0.691, -1.803, -0.509]`, `timeSig8
[2.674, -0.435, -0.515]`, `accidentalFlat [1.040, -0.257]`, `noteheadHalf
[0.903, -0.368]`, `fClef [2.534, 0.153, 0.148]` — all within measurement
tolerance of the contract's recorded values.

**New kill test.** `mutation_d_largest_contour_only_fill_misses_the_dot_points`
is the disjoint-component analogue of the outer-contour-only mutation kept
from the original oracle: it asserts that filling only `fClef`'s largest
subpath (the bowl) fails to contain either dot's ink point — the literal
"tessellator keeps only the largest contour" scenario Round 1 names as the
reason this check exists. The original outer-contour-only mutation
(`mutation_b_supplement_...`) is unchanged and still runs, now over all four
bounded-hole glyphs including `noteheadHalf`.

**Citations.** Every `pin 13 item N` citation in `round1-oracle/src/lib.rs`,
`src/main.rs`, `ORACLE_SUMMARY.md`, and this file has been replaced with a
direct citation to the Round 1 clause's own wording — pin 13 has no numbered
items in Revision 6 (or any prior revision), so those citations pointed at
nothing.

`oracle.json` and `ORACLE_SUMMARY.md` are regenerated from the amended code
(`cargo run -p round1-oracle` from `spikes/editor-toolkit/round1-oracle/`).
