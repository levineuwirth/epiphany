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
