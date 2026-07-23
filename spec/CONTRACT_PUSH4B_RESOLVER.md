# Contract: Push 4b tranche 2 — tuning resolution, in memory

Repo root `/home/jeans/Repos/active/epiphany`. The plan is
`spec/PLAN_PUSH4B_TUNING.md`; its four rulings are granted. Tranche 1 (the
pitch-space vocabulary and the retired P13-S2 guard) has landed. Read this file
in full before editing anything.

This tranche makes tuning **resolvable**: given a pitch, a tuning system, and a
reference pitch, compute the frequency it sounds at, walking the five resolution
scopes. Like tranche 1 it is a **vertical slice that stays in memory** — no
`Codec`, no wire movement, nothing frozen. It is the reversible half of the
remaining work; the schema-major-3 wire bump is a separate later tranche, so that
the resolution logic is exercised and proven correct before anything is frozen.

## Blast radius

* `crates/epiphany-core/src/` — a new module for the tuning-resolution
  vocabulary and the resolver; small changes to `graph.rs` (the
  `ScoreTuningContext` struct) and `codec.rs` (see the macro trap below);
  `lib.rs` re-exports.
* `crates/epiphany-core/DECISIONS.md` — record the tranche.
* Test files under `crates/epiphany-core/`.

You edit **no `.tex` file** and add **no requirement**, so all three counts in
`crates/epiphany-testkit/tests/requirement_labels.rs` stay at 212 / 282 / 282 —
if one moves, stop and report. You touch no other crate's source. In particular
`crates/epiphany-editor-gui/**` is being worked in parallel — do not go near it.

## The central prohibition: no `Codec`, no wire movement

Same as tranche 1, and for the same reason (Ruling C, `req:binfmt:frozen-layout`):
**canonical bytes MUST be byte-identical before and after this tranche.** The
resolver and its types stay in memory so they remain free to change when the wire
tranche discovers something about them. Do not write a `Codec` impl for any new
type. Do not add anything to the wire.

### The `ScoreTuningContext` field, and the macro trap

The resolver's scope walk needs `overrides`. Add exactly one field to
`ScoreTuningContext` (`graph.rs:1646`):

```rust
pub overrides: Vec<TuningOverride>,
```

Add **only** this one. Do **not** add `accidental_extensions` or `smufl` — the
resolver does not consume them, they pull in a large unconsumed type subtree, and
they belong to the wire tranche where they are added, encoded, and consumed by
engraving together. An unconsumed type surface is the `NOTEHEAD_ANCHORS` trap;
tranche 1 held that line and so does this one.

**The trap:** `ScoreTuningContext` is encoded by `struct_codec!` (`codec.rs:1835`),
whose generated `dec` ends `Ok(ScoreTuningContext { default_pitch_space,
default_tuning_system, reference })` — a struct literal naming exactly the three
wire fields. Adding a fourth field makes that literal fail to compile, and the
macro's `TextValue` destructure fails too. So you **must replace the
`struct_codec!` line with a hand-written `Codec` (and `TextValue`) impl** that
encodes and decodes exactly the three existing fields in their existing order and
constructs `overrides` as `Vec::new()`. The wire stays three fields; `overrides`
is in-memory-only. Confirm with a round-trip test that a `ScoreTuningContext`
with non-empty `overrides` encodes to the same bytes as one with empty
`overrides` — the field must not reach the wire this tranche.

Field *order* is the wire tranche's problem, not yours: the spec order places
`overrides` last, after `accidental_extensions` and `smufl`. Where you put it in
the Rust struct is free (the manual codec names its own encode order); leave a
comment that the encoded order is fixed at the three wire fields and the rest is
the major-3 delta.

## What to build

### 1. Types (in memory, grown not stubbed)

From `core_spec.tex` §"Tuning Systems" (`:3279` onward): `TuningSystem`
(`:3287`), `TuningResolution` (`:3309`), `TuningOverride` (`:3527`), `TuningScope`
(`:3534`). `ReferencePitch` already exists (`pitch.rs:469`) and is encoded — reuse
it.

`TuningResolution` is a six-variant enum in the spec. **Define only the variants
this tranche's built-in catalog constructs** — `EqualTemperament { divisions_per_octave }`
and `PerPositionRatios(Vec<PositionRatio>)` — plus `PositionRatio` itself. The
other four (`Function`, `Overlay`, `Imported`, `Adaptive`) are added when a
built-in needs them: `Function` in the temperament tranche, `Adaptive` when
`HarmonicContext` exists. Growing an in-memory enum later is free; transcribing
four variants whose payload subtrees (`TuningParameters`, `ImportedTuningData`,
`AdaptiveTuningParameters`, …) nothing constructs is exactly the unconsumed
surface to avoid. Note in a comment that the enum is deliberately partial and
which tranche each remaining variant waits on.

### 2. The built-in tuning catalog, as data (partial, honestly)

A lookup from `TuningSystemId` to a `TuningSystem`. This tranche resolves **nine**
of the twenty:

* **Six `tet-*`** (`tet-12`, `-19`, `-22`, `-31`, `-53`, `-72`) —
  `EqualTemperament { divisions_per_octave: N }` from the identifier.
* **Three `ji-static-5limit-*`** (`-C`, `-G`, `-D`) — `PerPositionRatios` whose
  twelve ratios are **computed from the lattice block**
  $\{3^a 5^b \mid a \in [-1,2],\ b \in [-1,1]\}$ (ratified,
  `req:tuning:ji-static-construction`, `core_spec.tex:4015`), octave-reduced and
  assigned in ascending order from the anchor. Compute them from the block in
  code — do **not** paste the twelve ratios as a literal table. The construction
  belongs in the code, exactly as it now belongs in the spec, so the two state
  the same rule in the same form. `-C`/`-G`/`-D` are the one construction at three
  anchors.

**Deferred, fail-closed, and reported (do not fake):**

* **The ten historical temperaments** (`pythagorean`, the three `meantone-*`,
  `werckmeister-iii`/`-iv`, `vallotti`, `kirnberger-ii`/`-iii`, `young-ii`) are
  **tranche 2b**, not this one — see the closing section. Their catalog entries
  resolve to a not-yet-supported result here (fail closed), not to a guess.
* **`ji-adaptive-5limit`** needs `HarmonicContext`, which does not exist in Rust
  and whose completion the spec puts out of scope. Fail closed; a later tranche
  builds the context and the adaptive resolver together.

An unresolved or deferred system MUST fail closed at the resolver — a clear
"not yet supported" error, never a fallback frequency. Report which of the twenty
resolve and which do not.

### 3. The frequency resolver

A function from (pitch-space position, `TuningSystem`, `ReferencePitch`) to a
frequency in **Hz as `f64`**. Frequency is non-canonical:
`req:determinism:canonical-floating-point` lists tuning among the floating-point
contexts and bars floating point only "where exact identity is needed", which a
sounding frequency is not. So `f64` is correct and nothing here goes near
canonical bytes.

The one subtlety worth stating: the construction's ratios are relative to the
tuning's own 1/1 (the anchor), but the **reference pitch** fixes a *different*
position's absolute frequency (A4 = 440 Hz by default). Anchor correctly: derive
the 1/1 frequency from the reference, then every position's frequency is the 1/1
frequency times that position's ratio (and octave). For `EqualTemperament`,
frequency is `ref_freq · 2^((position − ref_position)/N)`. Getting this anchoring
right is the point of building the resolver in memory first, where it is free to
be wrong and fix.

### 4. The five-scope resolution walk

`req:tuning:tuning-resolution-order` (`core_spec.tex:3549`): for a pitch, resolve
each of the three components (pitch space, tuning system, reference)
**independently**, walking from most specific to most general and halting at the
first scope that supplies a non-inherited value:

1. the pitch's own `AcousticPitch` — an explicit `TuningReference::Explicit` or
   `AcousticRealization::AbsoluteHz` short-circuits;
2. the voice containing the pitch;
3. the staff containing the voice;
4. each region enclosing the pitch, innermost to outermost;
5. the score's default tuning context.

Scopes 2–4 come from `ScoreTuningContext.overrides`: each `TuningOverride` carries
a `TuningScope` (`Voice`/`Staff`/`Region`/`Range`) and optional per-component
values. The walk finds the applicable overrides for the pitch (matching scope to
the pitch's voice/staff/region via the score graph) and takes the most specific
value per component. Independent per-component resolution is normative — a passage
may override only the reference while inheriting space and system.

### 5. The compatibility check

`req:tuning:tuning-system-compatibility` (`core_spec.tex:3581`): the resolved
tuning system's declared `pitch_space` MUST equal the resolved pitch space, or be
declared compatible via a registered mapping; otherwise the configuration MUST be
rejected. **No mapping
registry exists.** This tranche accepts **only same-`pitch_space`** and fails
closed on any mismatch — the registry is deferred, matching how tranche 1 handled
the underdetermined spaces. Report it as a deliberate deferral.

## Proof of life

Tests that assert *values*, not `is_ok()`:

1. **`tet-12`, A4 = 440 Hz → C5 ≈ 523.2511 Hz.** Assert within a tight tolerance
   (`ToleranceClass::AcousticCents` in `epiphany-determinism` is the project's
   acoustic tolerance; a cents-level check is right). This pins the reference
   anchoring.
2. **`ji-5limit` resolves a major third distinctly from `tet-12`.** The just
   major third is 5/4 (386.31 ¢); the equal-tempered one is 400 ¢. Assert the two
   resolved frequencies differ by the expected ~13.7 ¢, in the right direction.
   This is the "tuning actually does something" test.
3. **Scope precedence.** A voice-scoped `TuningOverride` changes the resolution
   for a pitch in that voice, and a pitch *outside* it still resolves to the score
   default. Assert both — a walk that ignores scope passes the first and fails the
   second.
4. **Compatibility rejects a mismatch.** A tuning system whose `pitch_space`
   differs from the resolved pitch space is rejected, not silently resolved.
5. **A deferred system fails closed.** `ji-adaptive-5limit` (and a temperament)
   return the not-supported error, never a frequency.

## Verification

**Mutation-verify every new test.** Re-introduce the bug each exists to catch,
confirm it fails, restore — and **assert the anchor text is present before
substituting**, because a replacement that matches nothing is a no-op mutation
indistinguishable from a passing test (this project has been bitten by exactly
that when rustfmt rewrapped the target line). **Restore by reversing the exact
substitution — never with `git checkout`, which discards all uncommitted work in
the file.** Report each mutation and the test that died.

Then the full gate, reporting actual commands and actual output:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets` → 0 warnings
3. `cargo test --workspace` → 0 failed
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
5. `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8
6. `cargo test -p epiphany-testkit --test requirement_labels` → 6 passed, counts
   unchanged at 212 / 282 / 282.

**Zero golden churn and zero canonical-byte movement are expected** — this tranche
is in-memory only. If any golden file or fuzz digest moves, something reached the
wire; stop and report rather than re-blessing.

CI pins clippy at 1.95.0 and the MSRV at 1.85; local `cargo clippy` is CI's
toolchain, and `clippy::incompatible_msrv` catches any API above the 1.85 floor.

## Citations

If you cite a requirement in a doc comment, **read it first and confirm it says
what your sentence claims** — the checker proves only that a `req:*` string
resolves, not that it is relevant (filed as P13-S9 after three
resolve-but-irrelevant citations in one day). The labels in play here:
`req:tuning:tuning-resolution-order`, `req:tuning:tuning-system-compatibility`,
`req:tuning:ji-static-construction`, `req:determinism:canonical-floating-point`.

## Do not

* Write a `Codec` impl for any new type, or let any new type reach the wire.
* Add `accidental_extensions` or `smufl` to `ScoreTuningContext`, or any field
  beyond the single `overrides`.
* Transcribe `TuningResolution` variants no built-in constructs this tranche.
* Paste the twelve `ji-static` ratios as a literal table — compute them from the
  lattice block.
* Invent a frequency for a deferred system, or a compatibility mapping for a
  mismatched pitch space. Fail closed and report.
* Edit any other crate (especially `epiphany-editor-gui`, worked in parallel),
  any `.tex`, or any `DECISIONS.md` but `epiphany-core`'s.
* Restore a mutation with `git checkout`. Reverse the substitution.
* Re-bless a golden or move a requirement count to make a test pass.

## Report

State: the types added and where; the manual `ScoreTuningContext` codec and the
round-trip proof that `overrides` stays off the wire; which of the twenty tuning
systems resolve and which fail closed, with the reason for each deferral; the
reference-anchoring computation and the `tet-12` C5 value you got; the mutation
for every new test and which test died; the actual gate output; whether any
golden or digest moved; and anything you chose not to do and why.

## Next: tranche 2b — the ten historical temperaments

Not this tranche. Named here so the deferral above is a plan, not a gap.

The ten temperaments resolve via `TuningResolution::Function` with reserved
built-in identifiers, each computing its ratios **from the ratified construction**
(which fifths are tempered, by what fraction of which comma) rather than from a
transcribed cents table — the constructions are normative in `core_spec.tex`
§"Temperament Constructions" as of `5e465a1`. They are their own pass because that
is where the arithmetic must be re-verified: the S6 draft produced two
arithmetically impossible temperaments and one false ambiguity, every one properly
cited, caught only by the closure invariant. The code re-derivation deserves the
same scrutiny — each construction's twelve-fifth closure sum recomputed in code
and checked against the ratified 23.4600 ¢ (or the stated wolf residue for the
non-circulating meantones and Pythagorean). Bundling that verify-heavy arithmetic
into the resolver plumbing would make a plumbing bug and a temperament-arithmetic
bug hard to tell apart.
