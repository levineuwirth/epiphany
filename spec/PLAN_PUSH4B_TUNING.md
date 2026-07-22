# Push 4b — the Chapter 4 tuning catalog: scope and plan

Status: **all four rulings granted; ready for dispatch.**
Prepared against `master` @ `b344925`. Every claim was checked against the
source; where I ran a probe I give the file and line.

---

## 1. The two facts that shape this pass

**4b is the first schema major 3.** `ScoreTuningContext` is a frozen positional
struct carrying **three** of its six specified fields:

| specified (`core_spec.tex:3380`) | on the wire (`codec.rs:1835`) |
|---|---|
| `default_pitch_space` | ✓ |
| `default_tuning_system` | ✓ |
| `reference` | ✓ |
| `accidental_extensions` | **absent** |
| `smufl` | **absent** |
| `overrides` | **absent** |

`req:binfmt:frozen-layout` is explicit that this is a major: *"Adding a field is
a MAJOR change regardless of its type: an `Option` field is not 'optional' in
the wire sense… There is no 'downgrade to minor' for an `Option` addition."*
Majors 0, 1 and 2 are defined; **3 is unopened**, and `epiphany-bundle/DECISIONS.md:413`
already names it as next ("beyond-accept-set tests moved to major 3"). Push 4a
deliberately avoided opening it (`binary_format.tex:3272`: "no major~3").

So 4b opens a major. That is not a cost to minimize — it is a **budget to
spend deliberately**, because the next one after it is major 4.

**And 4b is a spec pass before it is a build.** Zero of Chapter 4's types exist
in Rust — not `PitchSpace`, `PositionStructure`, `IntervalAlgebra`,
`NominalRegistry`, `AccidentalRegistry`, `AccidentalDefinition`,
`PitchSpaceModification`, `TranspositionBehavior`, `SpellingRuleSet`,
`TuningSystem`, `TuningResolution`, `TuningOverride`, nor `TuningScope`. Only
the `*Id` newtypes exist. There is nothing to extend; there is a chapter to
implement. And three defects stand in front of it.

---

## 2. Three spec defects block the build

Two were verified today from claims this repo had carried as **unverified**
through two passes (P13-S5, P13-S6). The third I found while scoping this plan.

### P13-S5 — the JI prime basis is specified at two lengths

`req:pitch:ji-vector-basis` says the built-in JI spaces order primes ascending
**starting with 2**, and `components.len()` MUST equal the basis size. The
built-in table says otherwise, each row exactly one short:

| space | requirement | table (`core_spec.tex:3496`) |
|---|---|---|
| `ji-5limit` | {2,3,5} → **3** | "Two-dimensional (prime axes 3, 5)" |
| `ji-7limit` | {2,3,5,7} → **4** | "Three-dimensional" |
| `ji-11limit` | {2,3,5,7,11} → **5** | "Four-dimensional" |

Both normative. A 5-limit vector is required to be both length 2 and length 3.
The octave-reduction clause does not reconcile them — it *normalizes* the first
component, it does not remove it.

### P13-S6 — no built-in tuning resolution is pinned; 14 of 20 are undefined

`req:tuning:builtin-tuning-catalog` makes all 20 identifiers MUST-resolve with
normative semantics. Only the six `tet-*` are specified, structurally, by
`TuningResolution::EqualTemperament`. The other 14 are names:
`pythagorean` (3:2 given; fifth-chain construction and wolf placement not),
three meantone variants, `werckmeister-iii`/`iv`, `vallotti`,
`kirnberger-ii`/`iii`, `young-ii`, three `ji-static-5limit-*`, and
`ji-adaptive-5limit`. `TuningResolution::Function` delegates them to a
`TuningFunctionId` that Chapter 10 lists as an **extension point**; no built-in
is mapped to one, and none is pinned.

Against `req:tuning:tuning-resolution-determinism` — determinism MUST hold
*across platforms* — two conforming implementations may each pick a different
published Werckmeister III and both pass. This is the requirement 4b exists to
satisfy, so it cannot be deferred past it.

### P13-S7 — the pitch-space registry does not exist (new; file it)

`ScalePosition.space` is documented as *"References an entry in **the score's
pitch-space registry**"* (`core_spec.tex:934`). `req:pitch:default-pitch-space`
says *"Every score MUST **define** at least one pitch space… Scores MAY define
additional pitch spaces."*

**There is no such registry anywhere in the data model.** `Score` has 13 fields
(`core_spec.tex:3639`) and none is a pitch-space or tuning-system registry.
`ScoreTuningContext`'s six specified fields carry *ids* and accidental
*extensions* — no space or system definitions. So a score can **name** an
arbitrary `PitchSpaceId` and can never **define** one: the MAY is flatly
unsatisfiable and the MUST is satisfiable only by reading "define" as "select".

This is load-bearing for 4b in a way the other two are not: a registry-backed
resolver has nothing to resolve *against* except the built-in catalog until it
is answered. **Ruling C answers it by moving the requirement, not the data
model** — "define" becomes "select", and the resolver's target is the built-in
catalog by ratification rather than by omission.

---

## 3. What I verified

| claim | result |
|---|---|
| Chapter 4 types present in Rust | **0 of 13**; only `*Id` newtypes |
| `ScoreTuningContext` wire fields | 3 of 6 (`codec.rs:1835`) |
| schema majors defined | 0, 1, 2; **3 unopened** |
| Chapter 4 labelled requirements | 9, all citable since P13-S1 |
| built-in tuning systems specified | **6 of 20** |
| score-local pitch-space storage | **none exists** |
| deferred items awaiting a major elsewhere | 1 (`epiphany-core/DECISIONS.md:431`, a configurable-order `Score` field with no consumer — not worth riding along) |

**The P13-S2 interim guard is narrower than it looked.** It refuses any `Cmn`
position outside built-in `cmn-12`, and `PLAN_P13S2_CMN24.md`'s Ruling B
accepted "false refusal for score-defined 12-chromatic spaces" as its cost.
Given P13-S7, that cost is **zero**: no score can define a space at all, so no
false-refusal case exists. Ruling C keeps it at zero by ratification rather than
by omission — the accepted cost would only become real if score-local definition
later lands, and it is now a deliberate deferral with a stated reason.

---

## 4. Rulings

### Ruling A — P13-S5 — **RATIFIED: full register**

The built-in table gains prime 2. `ji-5limit` becomes **three**-dimensional
(basis {2,3,5}), `ji-7limit` **four**, `ji-11limit` **five**.
`req:pitch:ji-vector-basis` is correct as written and does not change; the three
catalog rows do.

The data model forces it: a `JiVector` is an absolute *position*, and without
the prime-2 exponent `ji-5limit` cannot distinguish C4 from C5. The requirement
already says "Without such a declaration, full register is preserved." The
table's "two-dimensional (prime axes 3, 5)" is a *theory* presentation, in which
octave equivalence is assumed; the data model cannot assume it. Octave-reduction
would additionally have required a separate register carrier on `JiVector` — a
new field, and a second major-3 change for no gain.

### Ruling B — P13-S6 — **RATIFIED: keep all 20, pinned by construction**

The first scoping priced this as "a musicological ratification, 14 times over."
That was wrong, and the error was in *what form a definition takes*.

These temperaments have standard **constructions** — Vallotti narrows six
consecutive fifths by 1/6 Pythagorean comma and leaves the rest pure;
quarter-comma meantone narrows every fifth by 1/4 syntonic comma; Werckmeister
III narrows four named fifths by 1/4 Pythagorean comma. What varies across
published sources is overwhelmingly the **rounding of cents tables**, not the
construction. Specifying by construction therefore *removes* the variance that
specifying by cents table would *introduce*.

**The arithmetic-determinism worry is already answered by existing machinery.**
`ToleranceClass::AcousticCents` exists in `epiphany-determinism`, and its doc
comment names "Chapter 4 reference-pitch frequency resolution" outright.
`req:determinism:canonical-floating-point` lists **tuning** among the
floating-point contexts while barring floating point "where exact identity is
needed (object identifiers, operation ordering, graph membership, hash
identity)" — so a resolved frequency never enters canonical bytes. Bit-identity
was never achievable in any case: `pow`/`exp` are not correctly-rounded in
IEEE 754, so two libms may differ in the last ulp *including* for `tet-12`'s
$2^{n/12}$. `req:tuning:tuning-resolution-determinism` forbids dependence on
floating-point **rounding modes** — the FPU mode — not libm variance, and the
residual is on the order of $10^{-13}$ cents.

So the split is not 6 specified / 14 unspecified. It is **16 transcription and
4 decisions**:

* **16 pinned by construction** in Chapter 4 — one line each, with a source
  citation. Transcription work; a decision only where sources genuinely differ.
* **3 × `ji-static-5limit-*`** — which 12-note 5-limit scale (the comma choices
  for the chromatic degrees) is genuinely unsettled and needs one ratification.
* **`ji-adaptive-5limit`** — needs a real algorithm under
  `req:tuning:adaptive-tuning-purity`. Apply the in-house pattern:
  `req:pitch:spelling-algorithm` pins `SpellingAlgorithmId "default"` at
  version 1 to a named algorithm and errors on any other identifier. Do the same
  with a versioned `AdaptiveTuningFunctionId "default"`.

### Ruling C — P13-S7 — **RATIFIED: the catalog is closed, for now**

A score **selects** a pitch space and a tuning system from the built-in catalog;
it does not define one. Concretely:

* Amend `req:pitch:default-pitch-space`: "Every score MUST **define** at least
  one pitch space… Scores MAY **define** additional pitch spaces" becomes
  *select*, which is the only reading the data model has ever supported.
* Correct `ScalePosition`'s comment (`core_spec.tex:934`) — "References an entry
  in the score's pitch-space registry" — to name the **built-in** catalog. The
  registry it points at does not exist and, under this ruling, will not.
* **No `pitch_spaces` or `tuning_systems` field is added.** Score-local
  definition is recorded as a deferred major with its reason stated, not left
  as an unwritten intention.

This resolves P13-S7 rather than deferring it: the contradiction was between a
requirement and a data model, and the requirement moves.

**Implement all of it; freeze almost none of it.** The ruling does not shrink
the *implementation* surface — the 22 Chapter 4 types still need Rust
definitions, because the built-in catalog's 13 pitch spaces and 20 tuning
systems have to be expressed *somewhere*, and the P13-S2 guard's structural
replacement is a lookup from `PitchSpaceId` to `PositionStructure` over exactly
that data. What the ruling removes is the **wire** surface: only
`accidental_extensions`, `smufl` and `overrides` are encoded (Ruling D). Every
other Chapter 4 type stays in-memory and therefore stays **free to change**,
instead of being frozen by `req:binfmt:frozen-layout` before it has ever had a
consumer.

That asymmetry is the whole argument. Twenty never-constructed types — including
`SpellingRuleSet` and `TranspositionBehavior`, themselves unimplemented — would
otherwise be frozen permanently, sight unseen. `NOTEHEAD_ANCHORS` was
hand-written, unconsumed, and wrong in two independent ways; it survived
precisely because nothing read it. If deferral proves wrong, a later major adds
the registries; if committing proves wrong, we live with the frozen structs
*and* still need the migration.

**Consequence for P13-S2.** The interim guard's accepted cost — "false refusal
for score-defined 12-chromatic spaces" — stays **zero**, because no score can
define a space. Push 4b still replaces the identifier check with structural
resolution over the built-in table, as `req:pitch:space-capability-refusal`
requires; the fail-closed contract is unchanged and what the core can *prove*
widens from one space to thirteen.

### Ruling D — **RATIFIED: one major, and `accidental_extensions` is in it**

`ScoreTuningContext` completes to all three missing fields in major 3:
`accidental_extensions`, `smufl`, `overrides`. One migration, not two.

**`accidental_extensions` is a subtree, not a field**, and that is the price of
this ruling stated honestly. `ScoreAccidentalExtensions { base, additions,
overrides }` carries `Vec<AccidentalDefinition>`, which pulls in
`AccidentalEngraving`, `AccidentalCombination`, `PitchSpaceModification`,
`GlyphReference`, a bounding box, and `AnchorPoint` — **none of which exists in
`epiphany-core`**, which depends only on `epiphany-determinism`.

### The correction: they are homonyms, not shared types

The first scoping said `GlyphReference` and `BoundingBox` "exist but live in
`epiphany-layout-ir`" and recommended moving them down. **That was wrong**, and
a move would have relocated the wrong types:

* **`GlyphReference`.** Chapter 4's is
  `enum { Smufl(u32), Custom(CustomGlyphId), Composite(Vec<GlyphReference>) }`
  (`core_spec.tex:3181`) — a recursive codepoint reference. layout-ir's is
  `struct GlyphReference(pub Cow<'static, str>)` (`glyph.rs:50`) — a glyph
  *name*, a rendering concern. Same name, unrelated types.
* **`BoundingBox`.** The one layout-ir holds is **Chapter 7's**
  (`core_spec.tex:8650`), built on `StaffSpace(pub f32)`. It belongs where it is.

### And a defect: `AccidentalEngraving` cannot be canonical as written

`AccidentalEngraving` mixes `bounding_box: BoundingBox` — Chapter 7's
`StaffSpace`, **single precision** by deliberate choice (`spatial.rs:3`) — with
`advance_width: SpaceUnit`, which is `CanonicalF64`. Resolved layout is
**non-canonical** (`binary_format.tex:2521`, `:3211`), so f32 is correct there.
But this ruling puts `accidental_extensions` inside `ScoreTuningContext`, which
**is** canonical state, and `req:determinism:canonical-floating-point` requires
canonical floats to be "finite IEEE 754 **binary64**". As specified, the field
would put single-precision floats into canonical bytes.

**RATIFIED: Chapter 4 gets its own `SpaceUnit`-based engraving box.** All four
edges become `SpaceUnit`, the type `advance_width` already uses. This makes
`AccidentalEngraving` wholly core-typed, removes the backwards Chapter 4 →
Chapter 7 dependency, and satisfies Appendix D. Chapter 7's `BoundingBox` is
untouched and stays in layout-ir.

So there is **no crate move**. Step 0 is *define Chapter 4's glyph and engraving
vocabulary as new core types*, and layout-ir's homonyms are left alone. The two
`GlyphReference`s must not be unified — they are different concepts that happen
to share a word.

Nothing else rides along. The survey found one other deferred field addition
(`epiphany-core/DECISIONS.md:431`, a configurable-order `Score` field with **no
consumer**), deliberately excluded — a field with no consumer is how the
`Staff::default_clef` and `NOTEHEAD_ANCHORS` findings started.

Ruling C's registries, if granted, must land in **this** major or wait for a
later one; they must not split `ScoreTuningContext`'s completion across two
migrations.

---

## 5. Work breakdown

Genuinely large — the first pass in a while that wants a real fan-out, but not
until §4 is answered. Sketch, in dependency order:

1a. **Spec corrections (mechanical).** S5's three catalog rows to full register;
   S7's "define" → "select" and the `ScalePosition` comment; Chapter 4's
   `AccidentalEngraving` onto a `SpaceUnit`-based box (Ruling D). No new
   requirements, so the `requirement_labels.rs` counts **must not move** — if
   they do, something unintended happened.
1b. **S6 temperament constructions, drafted for review.** A non-normative
   artifact with a cited source per construction, *not* an edit to
   `core_spec.tex`. An agent writing Werckmeister III from memory into a
   normative document is the `NOTEHEAD_ANCHORS` failure exactly: hand-written,
   unverifiable in-tree, and load-bearing. Review gates promotion.
2. **Chapter 4 vocabulary in core.** The glyph and engraving types as *new* core
   types (Ruling D). Do **not** unify with layout-ir's homonyms.
3. **Major-3 delta.** Written against the real types once they exist, with the
   byte-for-byte migration `sec:evolution:migration` requires
   (`sec:evolution:major1`/`major2` are the models — a major MUST ship a
   documented byte-for-byte migration per `sec:evolution:migration`).
2. **Types (in-memory).** The Chapter 4 type surface, unfrozen: `PitchSpace`,
   `PositionStructure`, `IntervalAlgebra`, the registries, `TuningSystem`,
   `TuningResolution` and their kin. **No `Codec` impls** for these — Ruling C
   keeps them off the wire, which is what keeps them free to change.
3. **The built-in catalog as data.** The 13 pitch spaces and 20 tuning systems
   as constants, with the 16 constructible temperaments expressed *by their
   construction* (Ruling B) rather than as cent tables, so the spec and the code
   state the same rule in the same form.
4. **Codec — the narrow part.** `ScoreTuningContext`'s three added fields only,
   with `dec_*_v2` sub-decoders preserving the frozen prior layout exactly as
   major 2 did for the cross-cutting bodies. This is the entire wire delta.
5. **The resolver.** `req:tuning:tuning-resolution-order`'s five-scope walk,
   per-component and independent; `req:tuning:tuning-system-compatibility`'s
   rejection rule.
6. **Retire the P13-S2 interim guard.** Replace the `cmn-12` identifier check in
   `Pitch::transposed` and `twelve_tet_semitone` with
   `PositionStructure::DiatonicOverChromatic` resolution over the built-in
   table — **replace, not preserve** (`req:pitch:space-capability-refusal`, and
   Ruling B of `PLAN_P13S2_CMN24.md` says so explicitly). The fail-closed
   contract stays; what the core can *prove* widens from one space to thirteen.
   `cmn-24` transposition working end-to-end is this pass's proof of life.
7. **Accidental registries and SMuFL.** `req:tuning:smufl-version-fallback`,
   `req:tuning:accidental-modification-compatibility`, and the engrave-side
   consumers.
8. **Conformance.** A new step gating the built-in catalog: every promised
   identifier resolves, resolution is reproducible across runs, and each pinned
   temperament is checked against its construction rather than against a table
   copied from the implementation.

---

## 6. Traps

* **A major MUST ship a documented migration.** `sec:evolution:migration` is
  explicit, and the v0→v1 and v1→v2 entries are the shape to match. A major
  that lands without one is the defect this project would notice last.
* **`ScoreTuningContext` is positional and frozen.** Field *order* is as
  load-bearing as the field set, and the compiler cannot see it — the same
  invisibility that made the Text Projection layer verify field order by
  mechanical `encode_canonical`-vs-`project` diff.
* **The interim guard must be deleted, not widened.** Ratified in P13-S2. A 4b
  that adds registry resolution *beside* the name check has preserved the
  temporary mechanism as policy.
* **`twelve_tet_semitone` becomes a lie once non-12 spaces resolve.** It is
  public, has six callers across three crates, and renaming it is a breaking
  change in `epiphany-core` — cheaper now than later.
* **The spelling pre-pass stays 12-chromatic.** Line-of-fifths is `7·lof mod 12`
  in its bones. `cmn-24` positions land in `spelling_unavailable`, which the
  P13-S2 conformance note already records. 4b should not quietly acquire a
  microtonal spelling algorithm; that is its own ratification.
* **Built-in catalogs are implementation-provided, not serialized.** The 13
  pitch spaces and 20 tuning systems are referenced by id and do not go on the
  wire. Only score-*local* definitions and overrides would — which is precisely
  what Ruling C decides.
