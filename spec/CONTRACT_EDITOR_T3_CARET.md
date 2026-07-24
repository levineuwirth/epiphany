# Contract: Editor T3 — the note-entry caret

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/PLAN_EDITOR_APP.md`'s T3 ladder entry (the input-method-agnostic entry
seam). T2 is complete at `62117d3`. Parallel work is active in
`epiphany-core` (the decode-vector surface) — **no packet here touches
`epiphany-core`, any `.tex`, or `epiphany-testkit`**. Execution model as
T1a/T2: Sonnet subagents per packet, coordinator line-level review with
independent mutation re-runs, user deep-dives at contract sign-off (done),
any new golden baseline, and the final report. Mutation discipline
throughout: anchor-assert before substituting, restore by reversing, never
`git checkout`.

## Design pins (contract-level calls; no new plan ruling required)

* **The caret is session-local state, never in the op log**:
  `(voice, position, entry_duration)`. Undo does not move it (documented).
  If its voice ceases to exist, the caret **clears** (documented; v1 keeps
  the fallback trivial — the selection set's survivor rule does not apply to
  a point cursor).
* **Letters enter naturals only** (alteration is a follow-up gesture via the
  existing transpose intents, exactly MuseScore's model); the MIDI path
  enters explicit pitches, sharps-spelled by v1 policy.
* **Octave inference for letters**: nearest to the reference pitch — the
  nearest preceding note in the caret's voice (by position), else octave 4.
  Equidistant tie-break: **downward**. Table-tested, not vibes-tested.
* **Entry inserts with make-room overwrite** — the pencil's exact semantics
  and machinery (`make_room`), atomic; then the caret advances by the
  entered duration. Positions are global rationals, so advancing crosses
  barlines with no special casing.
* **`x_at_position` — the forward map** (position → world x over the same
  anchors `position_at` inverts) lands in W1 so W2 can draw the caret; it is
  seam API, round-trip-tested against `position_at`.

## W1 — the caret seam (`crates/epiphany-editor-core/**` only; dispatchable now)

API (indicative; crate idiom wins): `caret()`, `set_caret_at(point)` (region/
staff/voice via the existing `nearest_manifestation` path, primary voice,
position snapped via `default_grid_at`/`position_at`; entry duration
initialized from the grid step), `clear_caret()`,
`set_entry_duration(MusicalDuration)` (positive-validated),
`enter_nominal(CmnNominal)`, `enter_pitch(Pitch)`, `enter_rest()`,
`advance()`/`retreat()` (move by the entry duration, clamped at zero; no
insertion), and the pure `midi_note_to_pitch(u8) -> Pitch` (60 = C4,
69 = A4; black keys sharp-spelled — the v1 policy the spelling prepass may
later refine) feeding `enter_pitch`. `enter_nominal`/`enter_pitch`/
`enter_rest` funnel to ONE insertion core.

Tests (value-asserting) + minimum mutations: (t1) advance arithmetic across
a barline — mutation: skip the advance → dies; (t2) the octave table incl.
the equidistant tie-break — mutation: flip the tie-break → dies; (t3) entry
over occupied space reproduces the pencil's overwrite values — mutation:
bypass make-room → dies; (t4) the MIDI table (21/60/61/69/108) — mutation:
off-by-one octave → dies; (t5) `x_at_position`∘`position_at` round-trip —
mutation: anchor the wrong segment → dies; (t6) undo leaves the caret in
place AND a vanished voice clears it — mutations: move-on-undo /
keep-on-vanish → each dies.

DECISIONS.md entry records: the session-local/no-undo-move call, the octave
rule + tie-break, the naturals-only letter policy, the sharp-spelling MIDI
policy, and the `x_at_position` seam.

## W2 — GUI note-entry mode + the G5 golden (after W1 review)

`crates/epiphany-editor-gui/**` only. `N` toggles entry mode (precedence
documented against pencil — the two modes are mutually exclusive); letters
A–G → `enter_nominal`; the existing duration palette sets the entry
duration while in entry mode; ←/→ → `retreat`/`advance` (entry mode only —
they currently move selection pitch; the mode gates which); the caret draws
as a vertical line at `x_at_position` through `ViewMap`, full staff height.
**G5**: a new golden — `ten_measure_single_staff(0)`, entry mode, scripted
C-D-E-F quarters entered at the caret from the score's end — locking the
entry loop's visible result as G2 locks the pencil's. **A new baseline is a
user deep-dive: visually reviewed before blessing.** G1–G4 stay
byte-identical (53638/54590/57891 + G5's new file). Pure logic
unit-tested + mutated; egui dispatch review-only, stated honestly.

## Gate (every packet, actual output)

The standard six; conformance **9/9 with `--features golden-gate`, 8/8
without** (both run); requirement labels — report observed actuals (parallel
spec-side work may move them; never assert stale numbers); existing goldens
byte-identical. Blast radius per packet as stated. Do not commit.

## Report

Per packet, as every tranche: files + summary, exact asserted values, every
mutation with kill evidence, gate output, deviations flagged explicitly.
