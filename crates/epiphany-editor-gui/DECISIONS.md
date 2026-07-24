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

## 9. Rubber-band selection (2026-07-23, T2-W2) — drag threshold and anchor accent

Dispatched under `spec/CONTRACT_EDITOR_T2_SELECTION.md` §W2, over W1's
selection-set API (`selections()`, `anchor()`, `toggle_at`, `select_within`).
`main.rs` only; no golden pixel moves — the overlay is drawn by `score_view`
after the score texture is painted, entirely outside `goldens.rs`'s headless
raster path, so none of it is exercised by a golden test.

**Sensing.** The score view's `Sense` widens from `click()` to
`click_and_drag()`. A plain click still resolves through
`Response::clicked()`, unchanged. A drag is tracked in a new pure `DragRect`
(origin + current screen `Pos2`, no `egui::Response`/`Context` dependency, so
it is unit-testable headlessly), updated across frames via
`drag_started`/`dragged`, and resolved on `drag_stopped`.

**The drag threshold, and why it exists even though egui has its own.**
`DragRect::RUBBER_BAND_THRESHOLD = 4.0` screen points: below it, a completed
drag resolves as a plain click/toggle at the release point, not a
rubber-band `select_within`. This exists on top of egui's own click-distance
tolerance (`InputOptions::max_click_dist`, 6.0 points) because a widget
sensing both click and drag reclassifies a **long, still press-and-hold**
(no meaningful movement, held past `max_click_duration`, 0.8s) as "dragging"
purely on elapsed time (`PointerState::is_decidedly_dragging`) — a gesture a
user experiences as a click would otherwise silently become a near-empty
rubber-band replace of the current selection. `DragRect::is_rubber_band`
(`distance >= threshold`) and the release dispatch
(`resolve_release(&DragRect, ctrl) -> ReleaseAction`) are pure and directly
unit-tested, including the exact boundary (`>=`, not `>`).

**World-space query stays unnormalized.** `select_within`'s `BoundingBox`
normalizes its own corners (its doc: "rect's corners are order-independent");
the release handler therefore maps `drag.origin`/`drag.current` straight
through `ViewMap::screen_to_world` with no pre-normalization of its own.
`DragRect::screen_rect`'s own min/max normalization is a separate concern
(painting a valid, non-inverted rectangle for the live overlay) and never
feeds the world-space query.

**Ctrl/Cmd-click and the below-threshold-drag release** both resolve through
one shared `EditorApp::resolve_click(world, grid, toggle)`: `toggle` calls
`toggle_at`; otherwise `click`, with the exact same "empty — pencil would
insert…" reporting the single-selection code already had. This keeps the
plain-click behavior — including its status-line wording — identical to
before this packet for both call sites.

**Anchor accent.** Every member in `session.selections()` gets the existing
2.0pt blue stroke (unchanged from the prior single-selection code, now
looped); `session.anchor()` additionally gets a second pass with a 3.0pt
orange stroke on top. A single-member selection's sole member is trivially
the anchor, so it now receives both passes (blue then orange) where it
previously received only the blue one — a deliberate, undocumented-by-any-
golden visual change (the overlay is outside the golden raster path per
above), not a behavior change: `click`'s selection-logic contract (single
member, replaces the set) is unchanged, only its paint style gained an
always-present anchor accent.

**Debug panel and help text.** Adds a `selected: N member(s)` line and
relabels the existing selection lines as the anchor's (`session.anchor()`
directly, replacing the `session.selection()` compatibility read — the two
are equal by construction, `selection()` is `anchor().copied()`). The help
text now documents drag-to-select and Ctrl/Cmd-click, and states which
toolbar/key intents act on the whole selection (`delete_selection`,
`alter_selection` — both batch over every/every-pitch member per W1's
`DECISIONS.md`) versus the anchor alone (`move_selection_staff_step`,
`add_note_to_selection`, `insert_note_after_selection`,
`set_selection_duration` — verified against each function's own
`self.selection.anchor()` read in `epiphany-editor-core/src/lib.rs`, not
assumed from the contract's own summary, which groups `alter` with the
anchor-only intents even though its implementation batches).

## Clipboard wiring (2026-07-24, T2-W4b)

Dispatched as the W4b packet's Half 2, over `epiphany-editor-core`'s
`copy_selection`/`paste_over_selection`/`CopyOutcome`/`PasteOutcome`
(fragment.rs's clipboard fragment projection, Ruling E). `main.rs` only; no
golden pixel moves for the same structural reason W2's overlay didn't
(decision 9): `goldens.rs` builds its own session and calls
`render`/`rasterize_pixmap` directly, never through `EditorApp`, so nothing
this packet touches (toolbar, keys, status line, a new `last_paste_text`
field) is reachable from a golden test. Verified, not assumed: the three
baseline PNGs (`ten_measure_open.png` 53638B, `ten_measure_insert.png`
54590B, `ten_measure_slurs_castoff.png` 57891B) are byte-identical
(`md5sum` before/after) and untouched by `git status` throughout this
packet.

**The clipboard mechanism egui 0.29 actually offers — verified against the
vendored source (`~/.cargo/registry/…/egui-0.29.1`,
`~/.cargo/registry/…/eframe-0.29.1`), not assumed from memory of a newer
egui:**

* **Write (copy): `egui::Context::output_mut`/`egui::Ui::output_mut`,
  setting `PlatformOutput::copied_text`.** This is the *only* clipboard-write
  surface in 0.29 (`egui/src/data/output.rs:107`, `egui/src/context.rs:1423`)
  — the backend (`eframe`'s `egui-winit` integration) reads it after the
  frame and pushes it to the OS clipboard. `do_copy` sets
  `ctx.output_mut(|o| o.copied_text = outcome.fragment.clone())` on a
  successful `copy_selection`.
* **Read (paste): there is no synchronous "read the OS clipboard now" call
  anywhere in `eframe::Frame`'s or `egui::Context`'s public API** — checked
  directly (`grep -rn "pub fn" eframe-0.29.1/src/epi.rs`; the only clipboard
  reference in `eframe`'s own source is `egui_winit.clipboard_text()`
  inside the native integration's *internal* event-translation code,
  `native/glow_integration.rs:681`, never exposed to app code). The **only**
  way an app learns paste content is by consuming
  **`egui::Event::Paste(String)`** from the input event queue
  (`egui/src/data/input.rs:388`) — the backend detects an OS paste gesture
  (Ctrl/Cmd+V, or an OS-level paste menu action), reads the clipboard
  itself, and injects the text as one event, once, for that frame only.
  `EditorApp::handle_clipboard_events` reads `ctx.input(|i| &i.events)`
  every frame via the new pure helper `paste_event_text(&[egui::Event]) ->
  Option<&str>` and, on a hit, both **acts immediately** (pastes over the
  current selection) and **caches the text** (`last_paste_text`) for the
  toolbar "Paste" button — which has no other way to act between paste
  gestures, since a button click generates no `Event::Paste` of its own and
  there is nothing else to read it from.

**Why Copy is a plain keyboard edge but Paste cannot be.** `handle_keys`'s
existing pattern is a boolean edge-read (`i.modifiers.command &&
i.key_pressed(egui::Key::...)`), used for every other shortcut in this file.
Ctrl/Cmd+C fits it exactly: `do_copy` needs nothing from egui *except* that
the chord fired — the text it writes out comes from `copy_selection()`, not
from any event payload — so `Keys` gained one more field (`copy`) the same
way. Ctrl/Cmd+V structurally cannot fit that pattern: a bare `key_pressed`
edge tells you *that* a paste gesture happened, never *what* was pasted, and
only `Event::Paste`'s own `String` payload carries that. So paste has no
`Keys` field at all — `handle_clipboard_events` (called once per frame from
`update`, alongside `handle_keys`) is the whole mechanism, and it is what
makes Ctrl/Cmd+V "just work": egui's own native integration already turns
that chord into the event, this app only has to consume it.

**No-selection paste policy.** `paste_over_selection` itself returns
`Err(EditorError::NoSelection)` when nothing is selected — Display: "no
selection", accurate but not actionable in a clipboard context (a user
who never selected anything doesn't know that's what "no selection" means
here). `do_paste` pre-checks `session.anchor().is_none()` and reports
**"select a destination first"** instead, without ever calling
`paste_over_selection` — a GUI-level policy decision, not a change to the
core session's error semantics. Every *other* outcome — a successful paste
(reports events/slur/tie counts from `PasteOutcome`) or any other
`EditorError` — surfaces `{err}` verbatim, per the packet's brief ("the
fragment error Display strings are user-grade"); `FragmentError`'s and
`EditorError`'s `Display` impls (`epiphany-editor-core/src/fragment.rs`,
`lib.rs`) are already written for a human reader, so this crate adds no
translation layer over them.

**Toolbar buttons.** "Copy" is always clickable (unguarded), matching this
toolbar's existing convention for every other selection-dependent action
(Delete, Transpose, Move, …) — none of them are `add_enabled`-gated on
selection state; a `NoSelection`/`WrongSelection` error is left to surface
normally through the status line. "Paste" **is** gated
(`add_enabled(self.last_paste_text.is_some(), …)`), because unlike a
domain error, "nothing has ever arrived to paste" is a structural
precondition with no session-side error to report at all — the same class
of gate `can_undo`/`can_redo` already use for Undo/Redo.

**Pencil mode is untouched.** Clipboard actions (buttons, keys, and
`handle_clipboard_events`) run unconditionally regardless of `self.pencil`
— no new interaction with the pencil click-to-insert path, no new branch in
`score_view`. Pencil's own behavior, and every existing test/golden that
exercises it, is unchanged.

**Testing scope, stated honestly.** `paste_event_text` is pure (no
`egui::Context`/`InputState`/native-backend dependency) and gets four real
unit tests, each proven live by a mutation (dropping `.rev()` — breaks
"last `Paste` in the frame wins" — and matching the wrong `Event` variant
— both confirmed to fail the expected tests, then reverted). Everything
else this packet added — `do_copy`/`do_paste`/`do_paste_from_cache`/
`handle_clipboard_events`'s own dispatch, the toolbar buttons, the
`Keys::copy` edge, the help text — is **egui-side dispatch, reviewed but
not unit-tested**: there is no headless way in this crate to synthesize an
`egui::Context` frame, drive `ctx.input`/`output_mut`, or simulate an OS
clipboard/paste gesture (the same limitation the rest of `main.rs`
already lives with — `goldens.rs` exists precisely because rendering has
no headless story either, and `resolve_release`/`DragRect` were pulled out
pure for the same reason W2 needed to unit-test *something* about
release-time dispatch). `cargo test -p epiphany-editor-gui` green (20/20,
including all four golden tests) is this packet's regression gate for that
untested surface, per the brief.

## Note-entry mode + G5 (2026-07-24, T3-W2)

Dispatched under `spec/CONTRACT_EDITOR_T3_CARET.md` §W2, over W1's caret seam
(`EditorSession::{caret, set_caret_at, set_entry_duration, advance, retreat,
enter_nominal, enter_pitch, enter_rest, x_at_position}`, `Caret`). `main.rs`
(mode, keys, palette gating, click dispatch, caret overlay, help/debug text)
and `goldens.rs` (the new G5 test) only — **no editor-core change was
needed**, confirmed by grep-checking the packet's own suggestion of a "tiny
pub accessor": every piece the caret overlay needs (`score().voices()`,
`score().staff_instances()`, `resolved().strokes`, `x_at_position()`) was
already public.

**Mode exclusivity.** `pencil: bool` and the new `entry_mode: bool` are
mutually exclusive: turning either on always clears the other. Implemented
once, as a pure `toggle_exclusive(this, other) -> (bool, bool)` (mirrors
`resolve_release`'s role from T2-W2 — pulled out so the invariant itself is
unit-tested, not eyeballed at each of its four call sites: the P/N keys and
the two toolbar `toggle_value`s). `toggle_exclusive`'s contract is
intentionally asymmetric: turning `this` **on** forces `other` off; turning
`this` **off** leaves `other` exactly as it was (never assumes the
invariant already holds coming in, only guarantees it holds going out).

**Letter-key interpretation is mode-gated**, pulled out as a second pure
function `resolve_letter(entry_mode, nominal) -> LetterAction` (`Enter`,
`AddChordNote`, or `None`) — the packet's own named candidate for "the
obvious testable pure fn". In entry mode every letter A–G enters that
natural; outside it, only `A` keeps its pre-existing "add chord note to the
anchor" meaning (B–G stay unbound outside entry mode, as they always were).
`handle_keys` reads whichever of the seven letter keys fired this frame (at
most one in practice — one key event — but the lookup takes the first match
either way) into `Keys::letter: Option<CmnNominal>`, then dispatches through
`resolve_letter`.

**Key map, verified against the code rather than assumed from the
contract's own summary** (the contract said "←/→ ... they currently move
selection pitch", which does not match `handle_keys` as it stood — ↑/↓ move
pitch, ←/→ were unbound; the coordinator's dispatch caught this and it is
recorded here as the corrected, verified fact):

* **N** — toggles note-entry mode (mutually exclusive with pencil).
* **A–G**, entry mode on — `enter_nominal`. **A**, entry mode off — the
  existing "add chord note" (unchanged). **B–G**, entry mode off — no
  binding (unchanged: they never had one).
* **R** — `enter_rest`, entry mode only. Verified free: `handle_keys` never
  read `egui::Key::R` before this packet.
* **←/→** — `retreat`/`advance`, entry mode only. Both new bindings (neither
  was bound before); outside entry mode they remain inert, same as before
  this packet.
* **↑/↓** — unchanged in every mode: `move_selection_staff_step`.
* Duration palette (toolbar buttons `1`/`1/2`/`1/4`/`1/8`/`1/16`) — while
  entry mode is on, `set_entry_duration`; otherwise unchanged
  (`set_selection_duration`). Gated per-click on `self.entry_mode`, not
  a separate widget.

**Click dispatch.** `score_view`'s three-way branch is now `pencil` →
`insert_note_at` (unchanged) → else `entry_mode` → `set_caret_at` (new) →
else the existing select/toggle path. Pencil's branch keeps its early
`return` after `request_repaint()` (needed because `insert_note_at` mutates
the score, staling the hit-test map the overlay code below would otherwise
paint against this same frame). `set_caret_at` needs no such guard — it
mutates no score/layout state at all, so the overlay code safely runs in
the same frame using the just-updated caret. Rubber-band drag tracking
(`DragRect`) is now gated on `!self.pencil && !self.entry_mode` (widened
from `!self.pencil` alone) — an extension beyond the contract's literal
text (which only names "a click in entry mode"), decided for the same
reason pencil already excludes dragging: a click in either mode does
something other than select, so a drag gesture in either mode has no
selection to build either. Flagged as this packet's own call, not
contract-mandated.

**The caret overlay's region/staff derivation** (`caret_segment`, next to
`selection_rect`): `caret.voice` → `(region, staff_instance)` via
`session.score().voices()` (matches `epiphany-editor-core`'s own internal
use of `Score::voices()`) → the staff's global `StaffId` via
`session.score().staff_instances()` → that staff's *own* rendered line
strokes, filtered from `session.resolved().strokes` by
`TypedObjectId::Staff(staff_id)` (not just any staff — a multi-staff score
needs the caret drawn on its own staff, not the topmost one) — min/max `y`
across those strokes is the "full staff height" span, no hardcoded staff-
space constant needed (unlike duplicating editor-core's private
`STAFF_SPAN`, this reads the *actual* rendered geometry). `x` comes
straight from `x_at_position(region, None, &caret.position)`. Drawn as a
2.0pt line in a distinct green (`rgb(0, 170, 90)`), separate from the
selection's blue and the anchor's orange.

**G5 — the caret-entry golden.** `ten_measure_single_staff(0)`, the *same*
fixture and the *same* `scripted_insert_target` click point G2/G3 already
use (the contract's own ask: "the same extrapolation territory G2 used") —
reused verbatim rather than deriving a second target function. Drives
`set_caret_at` then four `enter_nominal(C/D/E/F)` calls at the caret's
default (quarter, from the 4/4 meter) entry duration. **Value assertions
before pixels**, per the contract: the grid step, the caret's post-
`set_caret_at` position (independently re-derived, not just asserted equal
to G2's own — though it does land on the same whole-note-10 slot G2 proves,
since it is the identical target on the identical fixture) and entry
duration, then each of the four entries' resulting caret position as an
exact `MusicalPosition`/`RationalTime` before any raster is taken. Baseline:
`goldens/ten_measure_caret_entry.png`, 57379 bytes — generated via
`EPIPHANY_BLESS_GOLDENS=1`, **not judged correct here**: per plan §Ruling
C's user deep-dive point, a new baseline is visually reviewed and approved
by the user before being committed. G1–G4's three baselines
(`ten_measure_open.png` 53638B, `ten_measure_insert.png` 54590B,
`ten_measure_slurs_castoff.png` 57891B) are confirmed byte-identical
(`git status` reports them untouched throughout this packet).

**Mutations, each substituted, observed failing, then reversed** (never
`git checkout`): (1) `resolve_letter`'s mode gate (`if entry_mode` →
`if !entry_mode`) — kills both of its own tests (letters interpreted
backwards in and out of entry mode). (2) `toggle_exclusive`'s clear branch
(`other` always passed through unchanged) — kills
`toggle_exclusive_turning_on_clears_the_other` (`(true, true)` vs expected
`(true, false)`). (3) G5: skipping the D entry (substituting a hand-built
`EditOutcome{graph_changed: true, ..}` in place of ever calling
`enter_nominal`) — **the position assertion fires, not the pixel one**:
the test panics at `session.caret().map(|c| c.position) == Some(after)`
(`Some(MusicalPosition(41/4))` vs the expected `Some(MusicalPosition(21/2))`)
long before reaching `render_pixmap`/`assert_golden` at all — confirming
the contract's own ordering claim ("value assertions before pixels") is
real, not just documented. (4) G5's determinism double: perturbing the
second `render_pixmap` call's `px_per_staff_space` (12.0 → 13.0) — kills
the `assert_eq!(svg1, svg2, ...)` check (a large SVG-string diff), proving
the double is a real, load-bearing check and not vestigial boilerplate
copied from G1–G4.

**Testing scope, stated honestly.** `resolve_letter` and `toggle_exclusive`
are pure and unit-tested directly, same discipline as `resolve_release`/
`DragRect`. Everything else this packet added — the toolbar's two
`toggle_value` calls and their `.changed()` mutual-exclusion glue, the
`Keys` struct's new fields and their `ctx.input` reads, `do_set_caret_at`/
`do_caret_step`/`set_entry_duration`'s own bodies, `score_view`'s click/drag
branching, the caret overlay's paint call, the debug panel and help text —
is **egui-side dispatch, reviewed but not unit-tested**, for the same
structural reason T2-W2/W4b's equivalent surfaces are not: there is no
headless way in this crate to synthesize an `egui::Context` frame or drive
`ctx.input`/painter calls. `cargo test -p epiphany-editor-gui` green
(25/25, including all five golden tests) is this packet's regression gate
for that untested surface.
