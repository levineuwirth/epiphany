# Contract: Push 4b tranche 2b — the ten historical temperaments

Repo root `/home/jeans/Repos/active/epiphany`. The plan is
`spec/PLAN_PUSH4B_TUNING.md`. Tranche 2 (`6fa14c7`) landed the in-memory tuning
resolver: nine of twenty systems resolve, the ten historical temperaments return
`TuningCatalogEntry::Deferred`. This tranche makes those ten **resolve**. Read
this file in full before editing anything.

Like tranches 1 and 2 it stays **in memory** — no `Codec`, no wire movement,
canonical bytes byte-identical. It is deliberately its own pass, separate from
the resolver plumbing, for one reason: **this is where the arithmetic must be
re-verified.** The S6 draft that produced these constructions first shipped two
temperaments that were arithmetically impossible and one false ambiguity — every
one properly cited to a real source — and only the closure invariant caught them.
The code re-derivation gets the same scrutiny. A temperament transcribed from its
cents table without re-deriving it from the construction is the same defect
wearing the same clothes.

## Blast radius

* `crates/epiphany-core/src/tuning.rs` — the extension points below, plus the ten
  construction descriptors and their tests.
* `crates/epiphany-core/src/pitch.rs` — only if you add a `TuningFunctionId`
  catalog-id newtype there beside the others (`pitch.rs:69` onward); otherwise
  leave it.
* `crates/epiphany-core/src/lib.rs` — re-export any new public type.
* `crates/epiphany-core/DECISIONS.md` — record the tranche.

You edit **no `.tex` file** and add **no requirement**, so the three counts in
`requirement_labels.rs` stay 212 / 282 / 282. You touch no other crate. In
particular `crates/epiphany-editor-gui/**` and `spec/PLAN_EDITOR_APP.md` are the
user's parallel work — do not go near them.

## The central prohibition, unchanged: no `Codec`, no wire movement

Canonical bytes MUST be byte-identical before and after. These types and this
data stay in memory. Do not write a `Codec` impl for any new type, do not add a
field to any encoded struct. If any golden file or fuzz digest moves, stop and
report — nothing here should reach the wire.

## The source of truth

`core_spec.tex` §"Temperament Constructions" (`3696`–`4011`), landed normatively
at `5e465a1`, is the authority. Each `\subsubsection` gives one construction: which
fifths are tempered, by what fraction of **which comma**, and — for the
non-circulating ones — where the wolf falls. It also gives derived cents tables.

**The fifth-tempering rules are the construction; the cents tables are derived.**
Build each temperament by walking the circle of fifths and applying the tempering
rules — do **not** paste the cents tables in as data. The tables are there for you
to *check your walk against*, exactly as tranche 2 computed the `ji-static` ratios
from the lattice block rather than pasting them. A walk that reproduces the spec's
derived cents *and* closes correctly is right; a pasted table proves nothing and
re-derives nothing.

## What to build

### 1. The `Function` variant and its plumbing

`TuningResolution` (`tuning.rs:95`) currently has two variants. Add the spec's
third, `Function { function: TuningFunctionId, parameters: TuningParameters }`
(`core_spec.tex:3320`). This is where historical temperaments live, per the
variant's own doc.

* `TuningFunctionId` — a `catalog_id!` string newtype like the others
  (`pitch.rs:69`+). The ten temperaments use reserved built-in ids (their catalog
  identifiers: `"pythagorean"`, `"werckmeister-iii"`, …).
* `TuningParameters` — its shape is not specified anywhere in `core_spec.tex` and
  no built-in parameterizes (each construction is fixed by its id). Define it as a
  documented zero-field / marker type, exactly as tranche 2 handled
  `SpellingParameters` and the unconstructed payloads. **Report it.** Do not
  invent a parameter schema.

Then teach the two resolver functions the new variant:

* `coordinate_ratio(resolution, s)` (`tuning.rs`, the `match` on `resolution`):
  a `Function` arm that, for a reserved built-in id, returns the temperament's
  ratio at coordinate `s` — `degree = s.rem_euclid(12)`, `octave = s.div_euclid(12)`,
  `ratio = temperament_ratios[degree] · 2^octave`. An **unknown**
  `TuningFunctionId` returns `None` (fails closed — `Function` is an extension
  point and no registry exists).
* `frequency_for_position`'s `divisions` match: for `Function`, divisions is the
  pitch space's chromatic cardinality (12 for `cmn-12`), taken from `structure`,
  not from the resolution — the `Function` variant carries no division count.

### 2. The ten construction descriptors, and the walk

Represent each temperament by its construction — the twelve fifths of the circle
`C–G–D–A–E–B–F♯–C♯–G♯–E♭–B♭–F–(C)`, each tagged with its tempering — and one walk
that places the twelve notes and derives their ratios. Compute the comma sizes
**from their exact ratios**, not from hardcoded rounded cents:

* pure fifth = `3/2`; Pythagorean comma = `531441/524288`; syntonic comma =
  `81/80`; schisma = `32805/32768` (= Pythagorean − syntonic — and note
  `syntonic + schisma = Pythagorean` exactly, which is why the Kirnberger closure
  works out; see below). Take each as `1200·log2(ratio)` in `f64`. Frequencies are
  non-canonical (`req:determinism:canonical-floating-point`), so `f64` throughout
  is correct — there is no exact-rational requirement here, and several fifths
  (meantone 1/5-, 1/6-comma) are irrational by construction.

The ten, from the spec section (verify each against `core_spec.tex`, do not take
this summary as the source):

**Non-circulating — eleven fifths specified, the twelfth is the closing wolf
(the residue that brings the circle to seven octaves):**

* `pythagorean` — eleven pure fifths; conventional cut E♭–G♯, wolf G♯→E♭.
* `meantone-1/4-comma`, `-1/5-comma`, `-1/6-comma` — eleven fifths each narrowed
  by 1/4, 1/5, 1/6 of the **syntonic** comma; same E♭–G♯ cut; the twelfth is the
  wide wolf.

**Circulating — all twelve fifths specified, temperings summing to exactly one
Pythagorean comma, no wolf:**

* `werckmeister-iii` — C–G, G–D, D–A, B–F♯ narrowed 1/4 **Pythagorean**; eight
  pure.
* `werckmeister-iv` — C–G, D–A, E–B, F♯–C♯, B♭–F narrowed 1/3 **Pythagorean**;
  G♯–E♭ and E♭–B♭ **widened** 1/3 Pythagorean; five pure.
* `vallotti` — F–C, C–G, G–D, D–A, A–E, E–B narrowed 1/6 **Pythagorean**; six
  pure.
* `kirnberger-ii` — D–A, A–E narrowed 1/2 **syntonic**; F♯–D♭ (the closing fifth)
  narrowed **1 schisma**; nine pure.
* `kirnberger-iii` — C–G, G–D, D–A, A–E narrowed 1/4 **syntonic**; F♯–D♭ narrowed
  **1 schisma**; seven pure.
* `young-ii` — C–G, G–D, D–A, A–E, E–B, B–F♯ narrowed 1/6 **Pythagorean**; six
  pure (the same construction as `vallotti`, rotated to start at C).

**Which comma is the load-bearing distinction, and no test outside the closure
check can see it.** Werckmeister/Vallotti/Young temper by the *Pythagorean* comma;
meantone and the Kirnberger fifths by the *syntonic*; Kirnberger's closing fifth
by the *schisma*. Confusing syntonic and Pythagorean is the classic error, and it
was ratified (via closure) that Werckmeister uses Pythagorean. The closure check
below is what catches a wrong choice.

**The schisma-fifth trap.** `kirnberger-ii` and `-iii` each temper their closing
F♯–D♭ fifth by a schisma. It is the single most commonly omitted element of these
constructions — its omission is exactly what made the S6 draft's first Kirnbergers
impossible. Without it the closure lands one schisma (1.9537 c) short of the
Pythagorean comma. Include it; the closure check will catch you if you don't.

### 3. Resolve them in the catalog

`built_in_tuning_system`'s temperament arm (`tuning.rs:245-247`) currently returns
`Deferred(DEFERRED_TEMPERAMENT)`. Return `Resolved(TuningSystem { … resolution:
Function { function: TuningFunctionId::new(id), parameters: … } … })`, `pitch_space`
= `cmn-12`. `ji-adaptive-5limit` stays `Deferred` (still needs `HarmonicContext`).

## Proof of life — the closure invariant, recomputed in code

This is the tranche's reason to exist, so its tests are the deliverable. For each
of the ten, a test that **recomputes the closure from the walk** and asserts it —
not a hardcoded constant:

* **Six circulating** (`werckmeister-iii`/`-iv`, `vallotti`, `kirnberger-ii`/`-iii`,
  `young-ii`): the sum of the twelve fifths' deviations from pure equals **one
  Pythagorean comma** (`1200·log2(531441/524288)`, ≈ 23.4600 c) within a tight
  tolerance, and no fifth is a wolf (every fifth within a small bound of pure).
* **Four non-circulating** (`pythagorean`, the three `meantone-*`): the residual
  twelfth fifth (the wolf) equals the spec's stated value —
  `pythagorean` 678.495 c, `meantone-1/4` 737.637 c, `-1/5` 725.809 c, `-1/6`
  717.923 c — within tolerance. "Does not close" is the positive claim here.

Then a handful of **discriminating derived-value checks**, each reproducing a
spec-table value from the walk (so the walk is validated against the spec's
independently-derived cents, not just against itself):

* `pythagorean` E = 407.820 c, F♯ = 611.730 c.
* `meantone-1/4-comma`'s major third C→E = 386.31 c (the just `5/4`, meantone's
  defining property).
* `kirnberger-ii` D = 203.910 c **vs** `kirnberger-iii` D = 193.157 c — same chain
  skeleton, different tempering; a test that passes both proves the two are not
  the same construction.

And the resolver-level checks:

* All ten resolve (no longer `Deferred`); a frequency comes out. E.g.
  `werckmeister-iii`'s C♯ resolves distinctly from `tet-12`'s C♯.
* An **unknown** `TuningFunctionId` fails closed (the extension-point path returns
  the resolver's not-supported error, never a frequency).

## Verification

**Mutation-verify every new test**, and let the closure tests earn their keep by
mutating the *construction*, not just the assertion:

* Drop the schisma fifth from `kirnberger-ii` → its closure test must fail (lands
  one schisma short). This is the S6 trap, reproduced as a mutation.
* Change `werckmeister-iii`'s comma from Pythagorean to syntonic → its closure
  test must fail (4 × ¼ syntonic = 21.506 ≠ 23.460). This proves the comma-type
  distinction is actually checked.
* Plus the ordinary per-test mutation for the discriminators and the fail-closed
  path.

Assert the anchor text is present before each substitution (a no-op mutation
looks like a passing test). **Restore by reversing the exact substitution — never
`git checkout`, which discards all uncommitted work in the file.** Report each
mutation and the test that died.

Then the full gate, reporting actual commands and output:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets` → 0 warnings
3. `cargo test --workspace` → 0 failed
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
5. `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8
6. `cargo test -p epiphany-testkit --test requirement_labels` → 6 passed, counts
   unchanged at 212 / 282 / 282.

**Zero golden churn and zero canonical-byte movement expected** — in-memory only.

CI pins clippy at 1.95.0 and the MSRV at 1.85; local `cargo clippy` is CI's
toolchain.

## Citations

If you cite a `req:*` label in a doc comment, read the requirement first and
confirm it says what your sentence claims (P13-S9: the checker proves the string
resolves, not that it is relevant). Labels in play:
`req:tuning:builtin-tuning-catalog`, `req:tuning:tuning-resolution-determinism`,
`req:determinism:canonical-floating-point`.

## Do not

* Write a `Codec` impl, or let any new type or datum reach the wire.
* Paste the spec's cents tables as data — build from the fifth-tempering
  construction and check against the tables.
* Hardcode a rounded comma value — derive each comma's cents from its exact ratio.
* Invent a `TuningParameters` schema, or a frequency for a system you cannot build.
* Omit the Kirnberger schisma fifth, or use the wrong comma for any temperament.
* Edit any other crate, any `.tex`, `spec/PLAN_EDITOR_APP.md`, or any `DECISIONS.md`
  but `epiphany-core`'s.
* Restore a mutation with `git checkout`. Reverse the substitution.
* Re-bless a golden or move a requirement count to make a test pass.

## Report

State: the extension points touched; how each of the ten is represented (the
descriptor shape and the one walk); `TuningParameters` as you defined it; the
closure value your code computed for each of the ten (the six Pythagorean-comma
sums and the four wolves), against the ratified spec values; the two
construction-level mutations (dropped schisma, wrong comma) and that they failed;
the ordinary mutations; the actual gate output; whether any golden or digest
moved; and anything you chose not to do and why.
