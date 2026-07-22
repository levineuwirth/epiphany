# Contract: Push 4b tranche 1 — pitch spaces become structure, and the interim guard dies

Repo root `/home/jeans/Repos/active/epiphany`. The plan is
`spec/PLAN_PUSH4B_TUNING.md`; its four rulings are **granted** and are law here.
Read this file in full before editing anything.

This is the first tranche of Push 4b's implementation. It is deliberately a
**vertical slice**, not a layer: types, the data that fills them, and the
consumer that reads them, landing together.

## Why it is a slice and not just "the type surface"

The plan lists the type surface, the built-in catalog, and retiring the P13-S2
guard as three steps. They ship as one here, because this repository has twice
been burned by the alternative. `Staff::default_clef` sat unread while a
bass-clef staff engraved as treble. `NOTEHEAD_ANCHORS` was hand-written,
unconsumed, wrong in two independent ways, and survived passes *because* nothing
read it. A Chapter 4 type surface with no consumer is that shape exactly.

So the acceptance test for this tranche is behavioural, not structural: **a
`cmn-24` pitch transposes end-to-end.** If that works, the types are real.

## Blast radius

* `crates/epiphany-core/src/` — a new module for the Chapter 4 pitch-space
  vocabulary, plus changes to `pitch.rs`.
* `crates/epiphany-core/DECISIONS.md` — record the tranche.
* Test files under `crates/epiphany-core/`.

You edit **no `.tex` file**. The specification is correct as written and is your
input, not your output. You add no requirement, so all three counts in
`crates/epiphany-testkit/tests/requirement_labels.rs` **must not move** — if one
does, stop and report rather than updating it.

## The central prohibition: no `Codec` impls

**Do not write a `Codec` impl for any type you add. Do not add any of them to a
`struct_codec!`. Do not add a field to `Score` or `ScoreTuningContext`.**

This is not a style preference, it is Ruling C, and it is the entire reason the
ruling was affordable. `req:binfmt:frozen-layout` says adding a field is a MAJOR
change *regardless of its type* — there is no "downgrade to minor" for an
`Option`. Anything that reaches the wire is frozen there permanently, sight
unseen, before it has ever had a consumer. These types stay in memory precisely
so they remain free to change when the second tranche discovers they are wrong.

An agent that adds a `Codec` impl to `PitchSpace` has quietly undone the ruling
that made this tranche cheap. Canonical bytes must be **byte-identical** before
and after your change; the conformance suite and the fuzz digest will say so.

## What to build

### 1. The pitch-space vocabulary, in memory

From `core_spec.tex` §"Pitch Spaces" (`:2850` onward). At minimum
`PositionStructure` (`:2889`, all four variants) and the `PitchSpace` type that
carries it. Take the shapes from the specification's own listings — field names,
types, and order — and do not improve them.

`req:tuning:diatonic-chromatic-mapping` (`:2932`) constrains
`DiatonicOverChromatic` with **three** clauses, all of which the constructor must
enforce: `nominal_to_chromatic` MUST have length `nominals_per_octave`, each
entry MUST be strictly less than `chromatic_positions_per_octave`, and the
mapping MUST be **strictly increasing**. Enforce them in a checked constructor,
the way `KeySignature::new` rejects out-of-range fifths — not in a comment.

(Both built-in mappings satisfy all three, which means a constructor that
enforces only two still passes every test built from the catalog. Test the
rejections directly.)

Only the types this tranche's consumer needs. `TuningSystem`,
`TuningResolution`, the accidental registries, and Chapter 4's glyph and
engraving vocabulary are **out of scope** — they belong to the tranche that adds
the codec, and landing them here would create exactly the unconsumed surface
this contract's opening section exists to avoid.

### 2. The thirteen built-in pitch spaces, as data

`core_spec.tex:3598-3646` is the normative table. Seven are fully determined by
it and are transcription:

* `cmn-12` — `DiatonicOverChromatic`, 7 nominals over 12, mapping
  `[0,2,4,5,7,9,11]` (given in `PositionStructure`'s own listing at `:2889`).
* `cmn-24` — `DiatonicOverChromatic`, 7 over 24, mapping
  `[0,4,8,10,14,18,22]` (given in the table).
* `edo-19`, `edo-22`, `edo-31`, `edo-53`, `edo-72` — `Chromatic`, with
  `positions_per_octave` from the identifier.

**Six are not fully determined, and this is the part that matters.** The three
`ji-*` spaces give a limit and a prime basis but `JiLattice.generators` is
`Vec<JiRatio>`, whose values the table does not state. `maqam-base`,
`gamelan-slendro`, and `gamelan-pelog` are described in prose that does not
determine a `PositionStructure` at all.

**Do not invent values for these.** Represent each honestly as unresolved —
whatever mechanism you choose, an unresolved space must **fail closed** at every
consumer, never fall back to a guess. Then **report each one** in your final
report, saying what the specification does and does not determine. A structure
you inferred and wrote down as though the spec stated it is the exact defect
this project has paid for twice; six half-plausible lattices would be a worse
outcome than six honest gaps.

### 3. Retire the P13-S2 interim guard — replace it, do not preserve it

`Pitch::transposed` (`pitch.rs`) currently refuses on a **string comparison**:

```rust
if self.scale_position.space.as_str() != "cmn-12" {
    return Err(TransposeRefusal::PitchSpaceUnavailable);
}
```

`twelve_tet_semitone` is gated the same way. Both were ratified as **explicitly
temporary** in `PLAN_P13S2_CMN24.md` Ruling B, which says the identifier check
"must be replaced—not preserved as policy". `req:pitch:space-capability-refusal`
is the requirement.

Replace with structural resolution: look the space's `PositionStructure` up in
the built-in catalog, and

* if it is `DiatonicOverChromatic`, transpose using **that structure's**
  `nominal_to_chromatic` and `chromatic_positions_per_octave` — which is what
  makes `cmn-24` work, and what `req:pitch:alteration-unit` and
  `req:pitch:transposition` already specify space-relatively;
* otherwise — `Chromatic`, `JiLattice`, `Registered`, unknown identifier, or one
  of the six unresolved entries — **refuse**, with the existing
  `TransposeRefusal::PitchSpaceUnavailable`.

**A version that adds structural lookup *beside* the name check has preserved
the temporary mechanism as policy.** The string `"cmn-12"` must not survive as a
control-flow condition anywhere in `pitch.rs`.

The fail-closed contract is unchanged. What the core can *prove* widens from one
space to the ones the specification actually determines.

### 4. Do not rename `twelve_tet_semitone`

The plan flags that this name "becomes a lie once non-12 spaces resolve", and
suggests renaming now. **Do not.** The name only becomes a lie if the function
starts answering for non-12-chromatic spaces. Keep it returning `None` unless
the resolved structure has exactly 12 chromatic positions — then the name
becomes *true* rather than false, structurally rather than by identifier, and no
breaking change lands in a public API with no consumer waiting for it.

Its six callers across three crates keep working unchanged. If you find a case
where the name genuinely misleads after your change, **report it** rather than
renaming.

## Proof of life

A test that transposes a `cmn-24` pitch and asserts the resulting scale position
— **not** merely that it returns `Ok`. This is the tranche's acceptance
criterion. `cmn-24`'s alteration steps are quarter-tones, so a flat is −2 and a
half-flat is −1 (`core_spec.tex:3606`); a transposition that silently applies
12-chromatic arithmetic to a 24-chromatic space is the defect this whole tranche
exists to prevent, and a test that only checks `is_ok()` cannot see it.

Also assert the refusals: a `JiLattice` space, an unknown identifier, and one of
the six unresolved spaces each refuse rather than guess.

## Verification

**Mutation-verify every new test.** For each, re-introduce the bug it exists to
catch, confirm the test fails, then restore. Report the mutation and the test
that died. A test asserting `is_ok()` on `cmn-24` will survive the mutation that
matters, which is how you will know it is the wrong test.

Before substituting, **assert the anchor is present** — a string replacement
that silently matches nothing produces a no-op mutation, and a no-op mutation is
indistinguishable from a passing test. This project has been caught by exactly
that, when rustfmt had rewrapped the line the replacement targeted.

Then the full gate, reporting actual commands and actual output:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets` → **0 warnings**
3. `cargo test --workspace` → 0 failed
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
5. `cargo run -q -p epiphany-testkit --example conformance_suite` → **8/8**
6. `cargo test -p epiphany-testkit --test requirement_labels` → 6 passed, and
   the three counts unchanged at 212 / 282 / 282.

**Zero golden churn and zero canonical-byte movement are expected.** You are
adding in-memory types; if any golden file or fuzz digest moves, something
reached the wire that should not have. Stop and report — do not re-bless a
golden.

CI pins clippy at 1.95.0 and the MSRV at 1.85, and lints run only on the pinned
stable. Local `cargo clippy` is the same toolchain CI uses, so a clean local run
is meaningful; `clippy::incompatible_msrv` will catch any API above the 1.85
floor.

## Citations

If you cite a requirement in a doc comment, **read the requirement first and
confirm it says what your sentence claims.** The checker proves only that a
`req:*` string resolves to a real label; a citation that resolves cleanly while
supporting nothing passes a fully green gate. This is filed as P13-S9 after
three instances in one day, one of them in a contract like this one. The
relevant labels here are `req:pitch:space-capability-refusal`,
`req:pitch:alteration-unit`, `req:pitch:transposition`, and
`req:tuning:diatonic-chromatic-mapping`.

## Do not

* Write a `Codec` impl, extend `struct_codec!`, or add a field to `Score` or
  `ScoreTuningContext`.
* Edit any `.tex` file, or any DECISIONS.md other than `epiphany-core`'s.
* Invent a `PositionStructure` for the six the specification underdetermines.
* Keep the `"cmn-12"` string comparison alongside the new lookup.
* Rename `twelve_tet_semitone`.
* Re-bless a golden file or update a requirement count to make a test pass.
* Run `cargo fmt --all` (use `--check`; format only what you wrote).

## Report

State: the types added and where; how you represented the six underdetermined
spaces and what the spec does and does not fix for each; the mutation for every
new test and which test died; the actual gate output; whether any golden or
digest moved; and anything you chose not to do and why.
