# Contract: Push 4b tranche 3a — the accidental vocabulary, in memory

Repo root `/home/jeans/Repos/active/epiphany`. The plan is
`spec/PLAN_PUSH4B_TUNING.md`, Ruling D. Read this file in full before editing.

Tranche 3 completes `ScoreTuningContext` — its three unencoded spec fields
(`accidental_extensions`, `smufl`, `overrides`) go on the wire, which opens
schema major 3 and freezes the layout permanently. Because that freeze is
irreversible, the work is split: **3a (this tranche) builds the type surface in
memory with a real consumer, so the shapes are exercised while still free to
change; 3b freezes them on the wire.** This is the same reversible-first
discipline tranches 1 and 2 used, and it has caught real bugs twice.

So this tranche adds **no `Codec`, no wire movement** — canonical bytes stay
byte-identical. It is entirely `epiphany-core`, in memory.

## Blast radius

* `crates/epiphany-core/src/` — a new module for the accidental/glyph/engraving
  vocabulary; small changes to `graph.rs` (`ScoreTuningContext`), `codec.rs` (the
  hand codec's `dec` only — see below), `pitch.rs` (new id newtypes), `lib.rs`
  (re-exports); the compatibility consumer (likely `invariants.rs`).
* `crates/epiphany-core/DECISIONS.md`.
* Test files under `crates/epiphany-core/`.

You edit **no `.tex`**, add **no requirement** (counts stay 212/282/282), and
touch **no other crate**. In particular **do not touch `epiphany-layout-ir`** —
see the `SmuflVersion` note below — and stay clear of `crates/epiphany-editor-gui/`,
`spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_T1A_GOLDENS.md` (concurrent
parallel work).

## The central prohibition: no `Codec`, no wire movement

Same as tranches 1/2/2b. Do not write a `Codec` impl for any new type, do not put
anything new on the wire. Canonical bytes MUST be byte-identical before and after
— if any golden or fuzz digest moves, stop and report.

### The `ScoreTuningContext` codec, exactly as far as it must go

`ScoreTuningContext` (`graph.rs:1659`) currently has four Rust fields
(`default_pitch_space`, `default_tuning_system`, `reference`, `overrides`), and a
**hand-written** `Codec` (`codec.rs:1848`) that encodes the first three and
defaults `overrides` on decode. Add the two missing spec fields **in memory**:

```rust
pub accidental_extensions: Vec<ScoreAccidentalExtensions>,
pub smufl: SmuflVersionRequirement,
```

Then extend **only the hand codec's `dec`** to default these two as well
(`accidental_extensions: Vec::new()`, `smufl: <the default below>`). **`enc` is
unchanged** — it still writes exactly the three wire fields, in the same order.
The three new fields are in-memory-only this tranche; 3b encodes them.

Prove it: a round-trip test that a `ScoreTuningContext` (or a `Score`) with
non-empty `accidental_extensions`/`smufl`/`overrides` encodes to **byte-identical**
output as one with all three empty/default, and decoding either reconstructs the
three as empty/default. This is the direct analogue of tranche 2's
`overrides_do_not_reach_the_wire`, extended to all three. The hand codec's
rationale comment (`codec.rs:1835`) should be updated to say all three fields stay
off the wire until 3b.

`SmuflVersionRequirement` needs a `Default` for the decode path. Use
**`{ minimum: SmuflVersion(1.4), authored_against: SmuflVersion(1.4) }`** — 1.4 is
the SMuFL version the repo already targets (`layout-ir/glyph.rs:120`), so the
default aligns with what 3b will unify against.

## What to build

### 1. The types, transcribed from Chapter 4 with three ratified corrections

From `core_spec.tex` §"Accidental Registries" (`3054`–`3234`) and §"Glyph
References and SMuFL" (`3235`–`3277`). Transcribe field-for-field, in spec order,
**except** the three corrections below (P13-S10/S11/S12, ratified 2026-07-23):

* `ScoreAccidentalExtensions { base: AccidentalRegistryId, additions: Vec<AccidentalDefinition>, overrides: Vec<AccidentalDefinition> }` (`:3228`)
* `AccidentalDefinition { id: AccidentalId, name: String, glyph: GlyphReference, modification: PitchSpaceModification, engraving: AccidentalEngraving, combination: AccidentalCombination }` (`:3073`)
* `GlyphReference` (`:3244`) — `enum { Smufl(u32), Custom(CustomGlyphId), Composite(Vec<GlyphReference>) }`. **Recursive**, and **Chapter 4's own** — do NOT unify with or import layout-ir's `struct GlyphReference(Cow<str>)`, a different concept sharing the name (the homonym the plan's Ruling D §"correction" calls out).
* `PitchSpaceModification` (`:3097`) — `enum { CmnChromatic(i8), EdoSteps(i16), JiRatio { numerator: i32, denominator: NonZeroU32 }, Cents(CanonicalF64), Registered(ModificationRegistryId) }`. **S10 correction: `Cents(CanonicalF64)`, not `Cents(f64)`** — a raw `f64` is unencodable in canonical state (`serialize.rs:110` decodes floats only through `CanonicalF64::from_le_bytes → NonFiniteFloat`; there is no `Codec for f64`). Reuse `CanonicalF64`/`SpaceUnit`'s existing pattern.
* `AccidentalEngraving { bounding_box: EngravingBoundingBox, anchor: AnchorPoint, advance_width: SpaceUnit, stacking_order: i32, default_parenthesized: bool }` (`:3159`)
* `EngravingBoundingBox { left: SpaceUnit, right: SpaceUnit, top: SpaceUnit, bottom: SpaceUnit }` (`:3150`) — Ruling D's canonical-safe box; `SpaceUnit` already exists (`graph.rs:895`).
* `AnchorPoint { x: SpaceUnit, y: SpaceUnit }` — **S11 correction: it is undefined in the spec** (`:3166` references it, nothing defines it), and core cannot depend on layout-ir. Define it core-native over `SpaceUnit`, and give it a doc comment pinning the frame per the ratification: **x/y in canonical space units, y-up, relative to the glyph's coordinate origin** (the box at `:3160` is "relative to the glyph's anchor point", so the anchor needs an unambiguous origin).
* `AccidentalCombination` (`:3210`) — `enum { Solitary, Stacking { compatible_groups: Vec<AccidentalGroupId> } }`
* `SmuflVersion` — **S12 correction, read this carefully.** `{ major: u16, minor_centi: u16 }`, where `minor_centi` is the version's fractional part **normalized to hundredths**: SMuFL versions are decimal fractions, so 1.12→`(1,12)`, 1.18→`(1,18)`, 1.20→`(1,20)`, **1.3→`(1,30)`**, **1.4→`(1,40)`**. Derived `Ord` on `(major, minor_centi)` is then correct across SMuFL's real release order (1.12 < 1.18 < 1.20 < 1.3 < 1.4); literal-minor storage would sort 1.3 and 1.4 *before* 1.12. Give it a constructor or documented mapping that performs the normalization (interpret the fractional digits as a decimal, express in hundredths: one digit ×10, two digits as-is), so a caller cannot accidentally pass a literal minor. **Do NOT touch `layout-ir`'s existing `SmuflVersion`** (`glyph.rs:29`, literal-minor, load-bearing for `GlyphCatalogIdentity`): unifying the two and moving `GlyphCatalogIdentity` is 3b's job, done deliberately with golden regen. For this tranche `core::SmuflVersion` and layout-ir's are a bounded homonym — and core cannot import layout-ir's, so within core there is no ambiguity. Leave a comment saying 3b unifies them.
* `SmuflVersionRequirement { minimum: SmuflVersion, authored_against: SmuflVersion }` (`:3269`)

New id newtypes (`catalog_id!` in `pitch.rs`, beside the others): `CustomGlyphId`,
`ModificationRegistryId`, `AccidentalGroupId`. `AccidentalRegistryId` and
`AccidentalId` already exist.

**No `Codec` for any of these.** They are in-memory only.

### 2. The consumer — this is what keeps the surface from being NOTEHEAD_ANCHORS

A type surface with no reader is the trap this project has paid for. Build two
in-core consumers that read the new types:

**(a) Accidental resolution.** A function that resolves an `AccidentalId` against a
score's `accidental_extensions`, honoring the spec's precedence (`:3224`:
"Extensions are stored on the score and override or augment the base registry"):
an `overrides` entry wins over an `additions` entry wins over the base registry.
Returns the resolved `AccidentalDefinition` (so it reads the whole structure —
`base`, `additions`, `overrides`, and the definition's fields).

**(b) The modification-compatibility invariant** —
`req:tuning:accidental-modification-compatibility` (`core_spec.tex`, the
requirement label at `:3120`): an accidental's `PitchSpaceModification` MUST be
expressible in the interval algebra of every pitch space referencing its
registry, and an implementation MUST reject a score that violates this. The
requirement's rules: `CmnChromatic` is valid only in `DiatonicOverChromatic` (or
compatible) spaces; `EdoSteps` only in `Chromatic` (or `Registered`); extend the
same shape to `JiRatio`↔`JiLattice`. Add this as a `check_invariants` violation
(`invariants.rs:221`), reading `built_in_position_structure` (tranche 1) for each
referenced space. Fail closed on a genuinely unknown space, as tranche 1 did.

Glyph and engraving metadata (`GlyphReference`, `AccidentalEngraving`,
`AccidentalCombination`) are carried by these consumers but their deep consumer is
the engraver, out of core — a later tranche. Say so in your report; do not
fabricate an in-core engraving consumer to manufacture coverage.

## Proof of life — assert behaviour and the ratified corrections

1. **S12 ordering** — `SmuflVersion` constructed from the real release sequence
   orders correctly: 1.12 < 1.18 < 1.20 < 1.3 < 1.4 (i.e. `(1,12) < (1,18) <
   (1,20) < (1,30) < (1,40)`). A test that would FAIL under literal-minor storage
   (where 1.3 sorts before 1.12). This directly locks S12.
2. **S10** — a `PitchSpaceModification::Cents` round-trips a finite value and
   cannot be constructed with a non-finite one (the `CanonicalF64` guard).
3. **Compatibility invariant** — a `CmnChromatic` accidental in a `cmn-12`
   (`DiatonicOverChromatic`) space passes; the same accidental in an `edo-31`
   (`Chromatic`) space is rejected with the violation. A test that only checks the
   accept case would miss the reject — assert both.
4. **Resolution precedence** — an `overrides` entry for an id shadows an
   `additions` entry for the same id; a base-registry id with no extension
   resolves to the base.
5. **The wire invariant** — the byte-identity round-trip from §"the codec"
   (all three new fields off the wire).

## Verification

**Mutation-verify every new test.** Assert the anchor is present before
substituting. **Restore by reversing the exact substitution — never `git
checkout`, which discards uncommitted work.** Mutate meaningfully — e.g. store
`SmuflVersion`'s minor literally instead of centi-normalized and confirm the S12
ordering test dies; weaken the compatibility check to accept everything and
confirm the reject test dies. Report each mutation and the test that died.

Then the full gate, actual commands and output:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets` → 0
3. `cargo test --workspace` → 0 failed
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
5. `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8
6. `cargo test -p epiphany-testkit --test requirement_labels` → 6 passed, counts
   212/282/282.

Zero golden churn and zero canonical-byte movement expected.

## Citations

Read any `req:*` you cite and confirm it says what your sentence claims (P13-S9).
Labels here: `req:tuning:accidental-modification-compatibility`,
`req:tuning:smufl-version-fallback`, `req:determinism:canonical-floating-point`,
`req:pitch:alteration-unit`.

## Do not

* Write a `Codec` impl, or move anything onto the wire.
* Edit `layout-ir` (the `SmuflVersion` unification and `GlyphCatalogIdentity` move
  are 3b), any other crate, any `.tex`, or the parallel editor-app files.
* Use `Cents(f64)`, literal-minor `SmuflVersion`, or an `AnchorPoint` without the
  frame doc.
* Unify Chapter 4's `GlyphReference` with layout-ir's — they are homonyms.
* Restore a mutation with `git checkout`. Re-bless a golden or move a count.

## Report

State: the types added and where; the three ratified corrections as you
implemented them (Cents→CanonicalF64, AnchorPoint frame, SmuflVersion
minor_centi); the extended hand codec and the byte-identity proof that all three
fields stay off the wire; the two consumers and exactly which fields each reads
(and which fields are carried-but-not-consumed-in-core, honestly); the mutation
for every test and which died; the actual gate output; whether any golden moved;
and anything you chose not to do and why.
