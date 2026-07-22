# P13-S2 — `cmn-24` cannot exist: scope and plan

Status: **implemented and verified.**
Prepared against `master` @ `043c18c`. Every claim below was checked against the
source; where I ran a probe I give the file and line.

**Landing constraint:** this plan and the Chapter 2 amendment land atomically.
The implementation defines and cites `req:pitch:alteration-unit` in the same
change, so requirement-citation integrity remains a commit invariant rather
than follow-up cleanup.

---

## 1. The tracker's framing is wrong in one load-bearing way

P13-S2 is filed as: *"Either the space is not `Cmn`-representable (and needs
`Integer`/`Registered`), or `alteration` needs a finer unit — **a data-model
major**."*

The diagnosis is right. The parenthetical is wrong, and it is the reason this
item has sat parked.

`binary_format.tex` Requirement `req:binfmt:frozen-layout` (line 1168) names
exactly four **open** value-layer vocabularies, and `PitchSpacePosition` is one
of them:

> At the value layer, the open vocabularies are those with a `Registered` escape
> variant — `PitchSpacePosition`, `SpellingNominal`, `StaffGroupKind`,
> `TieClass` — through which extensions attach **without any wire change at
> all**.

and `sec:evolution:additive` (line 2378) repeats it: *"Extension through an
escape variant is **not** a schema change at all: the wire form is already
defined."* Appending a **new** variant to one of these is a schema-**minor**
change, not a major.

So of the four live options (§4), **three cost no major bump and move no
existing byte.** Only one — regrading `alteration` onto a finer fixed unit — is
the data-model major the tracker names, and it is also the only option that
rewrites the canonical bytes of every pitch ever authored. The item is
considerably cheaper than its parking notice claims, and the expensive option
is ruled out by Ruling A.

---

## 2. The defect is larger than `cmn-24`

`cmn-24` is one instance of a rule nobody wrote down: **nothing binds a `Cmn`
position to a space whose chromatic layer is twelve.**

| what I checked | result |
|---|---|
| `ScalePosition` carries the space beside the position | yes — `pitch.rs:344` |
| any code consults that space when interpreting `Cmn` | **none** |
| an invariant rejects `Cmn` in a non-diatonic space | **none** — `invariants.rs` never validates the space against the position variant |
| `PitchSpacePosition::Cmn` match sites in the workspace | **52**, across 15 files |

`ScalePosition { space: PitchSpaceId::new("edo-31"), position: Cmn { .. } }`
constructs, validates, encodes, and transposes today. It means nothing, and the
core will happily do twelve-semitone arithmetic on it.

Four core surfaces bake in "`Cmn` ⟹ 12":

* **`Pitch::transposed`** (`pitch.rs:270`) — `nominal.chromatic() + alteration +
  12 * octave`, never reading `self.scale_position.space`. This is the normative
  equation of `req:pitch:transposition`, so it is a *specified* assumption, not
  an implementation shortcut.
* **`Pitch::twelve_tet_semitone`** (`pitch.rs:490`) — same expression; feeds
  `twelve_tet_class`, `enharmonic_equivalent`, `PitchRange::contains`
  (`pitch.rs:612`), the spelling pre-pass (`prepass.rs:675`), and the transpose
  reducer (`reduce.rs:5899`).
* **`CmnNominal::chromatic()`** (`pitch.rs:172`) — returns the cmn-12 map
  `{C:0, D:2, E:4, F:5, G:7, A:9, B:11}` unconditionally, for every space.
* **The spelling pre-pass** (`prepass.rs:524-746`) — line-of-fifths, which is
  `7·lof mod 12`. Structurally 12-chromatic. `req:pitch:spelling-algorithm`
  specifies it that way and gates it on nothing.

Two further instances the tracker does not name:

* **`maqam-base`** is in the same built-in table (`core_spec.tex:3460`) as
  *"Skeletal maqam framework with quarter-flat and quarter-sharp accidentals."*
  Same quarter-tone requirement, and no ruling on `cmn-24` settles it unless the
  ruling is about the *unit* rather than about one catalog row.
* **`PitchSpaceModification::CmnChromatic(i8)`** (`core_spec.tex:3043`) — the
  accidental registry's modification type has the identical unit problem, and
  `req:tuning:accidental-modification-compatibility` (line 3065) already ties it
  to spaces with "`DiatonicChromatic` or compatible algebra". **There are two
  sites with one unit decision between them**, and a ruling that fixes only
  `PitchSpacePosition::Cmn.alteration` leaves the accidental half broken.

**The spec is already fully general where it counts.** `PositionStructure::
DiatonicOverChromatic { nominals_per_octave, chromatic_positions_per_octave,
nominal_to_chromatic }` (`core_spec.tex:2851`) expresses `cmn-24` exactly:
`{7, 24, [0,4,8,10,14,18,22]}`. `req:tuning:diatonic-chromatic-mapping`
constrains that mapping and **never fixes 24 or 12**. Chapter 4 was written for
an arbitrary chromatic layer; Chapter 2 was written for twelve. That is the
whole defect.

---

## 3. Two ratified MUSTs already contradict each other

This is not merely an implementation gap. Both of these are labelled,
implemented requirements:

* **`req:pitch:transposition`** (`core_spec.tex:1085`) fixes, for *every*
  `Cmn` position with no space qualifier, `s = nominal.chromatic() + alteration
  + 12·octave`, where `nominal.chromatic()` is the cmn-12 map made normative by
  `sec:pitch:nominal`.
* **`req:tuning:builtin-tuning-catalog`** (`core_spec.tex:3501`) makes every
  built-in identifier MUST-resolve with the specified semantics, and the table
  specifies `cmn-24` as *"CMN extended with 24-EDO quarter-tone accidentals."*

A conforming implementation is therefore required to admit a `cmn-24` `Cmn`
position and required to transpose it by semitone arithmetic. It is the same
family as P13-I1's two-listings drift: two normative statements, each correct in
its own chapter, jointly unsatisfiable. **Whatever else this pass does, one of
these two requirements changes.** That is what makes it a ratification exercise
rather than a bug fix.

---

## 4. The four options

### Option 1 — `cmn-24` is not `Cmn`-representable

Redefine the catalog row: `cmn-24` becomes `PositionStructure::Chromatic
{ positions_per_octave: 24 }`, positions are `Integer { space_size: 24, index }`.

*Cost:* one table row and a sentence. No code, no bytes, no schema event.
*What it buys:* the contradiction in §3 disappears immediately and honestly.
*What it costs:* the feature. An `Integer` position has no nominal, so a
half-flat E cannot be told to sit on the E line — the notehead has no staff
position and the spelling pre-pass has nothing to spell. `req:pitch:transposition`
refuses non-`Cmn` positions outright, so `cmn-24` scores become untransposable.
"CMN-compatible notation" is precisely what the catalog row promises and
precisely what this deletes.

### Option 2 — `alteration` is chromatic steps *of its space* — **ratified**

The `i8` keeps its type and its width. Its **unit** becomes one step of the
enclosing space's chromatic layer. In `cmn-12` that is a semitone; in `cmn-24` a
quarter-tone, so a flat is `-2` and a half-flat is `-1`.

*Migration: none.* Every score in existence is `cmn-12`, where the new unit and
the old unit are the same thing. Not one canonical byte moves — and I verified
the stronger property that makes this safe: `canonical_pitch_bytes`
(`pitch.rs:645`) writes the **space id first**, before the position, so a
`cmn-24` E-half-flat and a `cmn-12` E-flat derive different `PitchId`s despite
identical position bytes. The position is never interpreted without its space in
the byte stream either.

*What it requires:*

* `req:pitch:transposition` amended to `s = nominal.chromatic(space) +
  alteration + C·octave`, with `C = chromatic_positions_per_octave`. The
  diatonic half is unchanged — mod 7 stays mod 7, because the nominal count is
  still seven.
* `CmnNominal::chromatic()` takes the space. It currently *is* the cmn-12 map;
  in `cmn-24` the map is `[0,4,8,10,14,18,22]`. An API break in core, not a wire
  break.
* **The pitch-space registry must exist**, because the core must resolve a
  `PitchSpaceId` to its `chromatic_positions_per_octave`. That is Push 4b's
  *other* blocker. So Option 2 does not unblock 4b from outside — **it is part
  of 4b**, and the sequencing question in Ruling B follows from that.
* The `twelve_tet_*` family gets an honest gate: `None` unless `C == 12`.

*What it does not fix:* the spelling pre-pass. Line-of-fifths is 12-chromatic in
its bones; `cmn-24` pitches would land in `spelling_unavailable`, which
`prepass.rs:116` already has a bucket and a counter for. That is a defensible
first delivery — store, transpose, and engrave quarter-tones without inferring
their spelling — but it should be stated, not discovered.

### Option 3 — regrade `alteration` onto a finer fixed unit

Quarter-tones, cents, or a rational. **This is the data-model major the tracker
names, and the only option that is one.** Every existing `alteration` is
rescaled, so every `Pitch`'s canonical bytes move, so every `PitchId`,
`ContentHash` and `OperationId` derived from one moves, and every golden vector
regenerates. It also buys a single grid: pick 24 and `edo-31`, `edo-53`,
`edo-72` stay unrepresentable, so it pays a major and does not close the class.

Ruling A explicitly rules this out, so it cannot continue to make the selected
option look expensive.

### Option 4 — append a `Cmn`-microtonal variant

`CmnMicro { nominal, alteration_num, alteration_den, octave }` at discriminant
4. Schema-**minor** per §1; existing bytes decode unchanged.

*What it costs:* it forks the CMN path. All 52 `Cmn` match sites become
two-armed, and the ones that are not become sites that silently ignore
microtonal pitches — the exact failure mode this project keeps paying for, and
one no test would catch because no fixture would have a microtonal pitch in it.
It also still needs the space, to know what the denominator is denominated
against. Strictly more machinery than Option 2 for strictly less.

---

## 5. Rulings

### Ruling A — Option 2 governs both CMN chromatic units

**Ratified: Option 2.** A `Cmn` alteration and a `CmnChromatic`
modification are denominated in steps of the enclosing pitch space's chromatic
layer; `cmn-12`'s step is the semitone. This is one unit rule governing
`PitchSpacePosition::Cmn.alteration`,
`PitchSpaceModification::CmnChromatic`, `cmn-24`, and `maqam-base`—not a
special case for one catalog row.

The `i8` representation and wire layout stay unchanged. Existing `cmn-12`
scores require no migration because their unit remains the semitone. Option 3
is explicitly ruled out: rewriting every existing pitch and its derived
identifiers to obtain a fixed finer grid pays a data-model major without
closing the general class. Options 1 and 4 are not selected.

### Ruling B — specify now, generalize in Push 4b, fail closed meanwhile

**Ratified: land the normative unit and transposition changes under P13-S2
now.** This resolves the contradiction in §3 without waiting for a registry.
The generalized code path belongs to Push 4b: the registry,
space-relative nominal mapping, and structural `twelve_tet_*` gates must land
together because the core cannot derive them from an identifier alone.

**Also ratified: the interim guard is part of P13-S2.** Until Push 4b supplies
structural resolution, the core must fail closed:

* `Pitch::transposed` may use the current arithmetic only when the core can
  establish that the enclosing space has the built-in `cmn-12` structure;
  otherwise it returns a dedicated `TransposeRefusal`.
* `twelve_tet_semitone` returns `None` unless the core can establish a
  twelve-chromatic layer; callers must propagate that unavailability instead
  of deriving a 12-TET result.

This is a capability check, not a normative claim that the identifier
`cmn-12` defines the structure. In the pre-registry implementation, recognizing
that identifier is merely the only available proof of the capability. The
temporary consequence is explicit and accepted: a score-defined
diatonic-over-12 space refuses rather than silently receiving arithmetic the
core cannot validate. Push 4b must replace the identifier check with
`PositionStructure::DiatonicOverChromatic` resolution; it must not preserve the
name check as policy.

The refusal rule is the stable contract: unresolved structure must not produce
a guessed transposition or 12-TET value. Push 4b broadens what the core can
prove without changing that fail-closed behavior.

### Ruling C — distinct diagnostic, existing wire reason

**Ratified:** add a distinct
`TransposeRefusal::PitchSpaceUnavailable` diagnostic for the core's inability
to establish the pitch space structure required by the operation.
`epiphany-ops` must map it to the existing
`PreconditionFailureReason::PitchSpaceMismatch` discriminant 6, alongside
`TransposeRefusal::NonCmnPosition`.

No new `PreconditionFailureReason` variant or discriminant is permitted. That
vocabulary is canonical operation-effect bytes and append-only; permanently
reserving a wire value for the pre-registry identifier guard would preserve the
temporary mechanism in exactly the artifact Ruling B is trying to protect.
Discriminant 6 already covers a pitch-space or tuning-context precondition that
does not admit the operation. The operation-catalog description of that
existing case must be broadened accordingly, but the binary-format table and
wire bytes do not change.

### Ruling D — a reference pitch uses the score's default space

**Ratified:** when `PitchSpacePosition::Cmn` occurs bare as
`ScoreTuningContext.reference.position`, its alteration unit is the chromatic
step of `ScoreTuningContext.default_pitch_space`. This makes explicit the
relationship already required by `req:tuning:reference-pitch`: the reference
position must be valid within that default space.

`ReferencePitch::a440()` happens to be unit-independent because its alteration
is zero; that does not settle the type's admitted nonzero values. The normative
alteration-unit requirement must state the default-space rule rather than
letting implementations infer `cmn-12`. Any future context that embeds a bare
`ReferencePitch` must likewise identify the pitch space that denominates it.

---

## 6. What I verified, and what I did not

**Verified** (file:line given above): the open-vocabulary rule and the four named
vocabularies; the 52 `Cmn` sites in 15 files; that no invariant validates the
space against the position variant; that `canonical_pitch_bytes` writes the
space first; that `Pitch::transposed` and `twelve_tet_semitone` never read the
space; that `CmnNominal::chromatic()` is the unqualified cmn-12 map; that
`DiatonicOverChromatic` parameterizes the chromatic layer and
`req:tuning:diatonic-chromatic-mapping` does not fix it; that `maqam-base` and
`PitchSpaceModification::CmnChromatic` carry the same defect; that
`PitchSpace`, `PositionStructure`, `IntervalAlgebra`, `AccidentalRegistry`,
`AccidentalDefinition`, `PitchSpaceModification`, `TranspositionBehavior` and
`SpellingRuleSet` **exist only in the spec** — no Rust type in the workspace
corresponds to any of them (only the `*Id` catalog newtypes exist).

**Not verified, and inherited rather than checked:** the two Push-4a audit claims
already flagged as unconfirmed in `epiphany-core/DECISIONS.md` — that the JI
dimension convention conflicts with its own prime-2 requirement, and that the
named historical tunings lack exact deterministic ratio data. Neither bears on
this ruling. I also did not survey `epiphany-editor-core` or
`epiphany-layout-ir` exhaustively; I confirmed `editor-core:5319` performs
`alteration + 1` as a raise gesture (which becomes a quarter-tone raise under
Option 2 — arguably right, arguably a surprise) and that `constrained.rs` reads
`nominal`/`octave` for staff position, which is space-agnostic. A full
downstream survey belongs to the implementation wave, not to this scoping.

---

## 7. Work breakdown

Small enough that it does not fan out. One agent, or my own hands.

The implementation and this plan must land atomically. The Chapter 2 definition
of `req:pitch:alteration-unit` and every citation below are part of the same
change; `DISCUSSED_NOT_CITED` is not used because this is a real normative
dependency.

1. **Chapter 2.** Amend `req:pitch:transposition` to the space-relative
   equation. Amend the `PitchSpacePosition::Cmn` listing comment
   (`core_spec.tex:954`) from semitones to the space-relative unit. Add and
   label `req:pitch:alteration-unit` to govern
   `PitchSpacePosition::Cmn.alteration` and
   `PitchSpaceModification::CmnChromatic`; this plan cites that requirement as
   part of the same atomic change.
2. **Chapter 4.** Amend `PitchSpaceModification::CmnChromatic`'s comment to cite
   the new requirement. Amend the existing reference-pitch requirement to make
   `ScoreTuningContext.default_pitch_space` the unit source for its bare
   position; this is not a third new requirement. Confirm the `cmn-24` and
   `maqam-base` catalog rows now have a satisfiable reading, and state
   `cmn-24`'s `nominal_to_chromatic` explicitly so readers cannot derive
   different maps.
3. **Conformance note.** Record that `cmn-24` positions are
   `spelling_unavailable` at this revision, and why.
4. **Interim guard and operation effect.** Add
   `TransposeRefusal::PitchSpaceUnavailable`, the `Pitch::transposed` guard,
   the `twelve_tet_*` gate, and the second new requirement: unresolved pitch
   space structure must refuse rather than guess. Map the new core diagnostic
   to `PreconditionFailureReason::PitchSpaceMismatch` (6); append no wire
   vocabulary. Broaden the existing operation-catalog case for discriminant 6
   to include it. State in the core doc comment that Push 4b replaces
   identifier recognition with structural resolution.
   This normative broadening is an independent Operation Catalog **0.9.0**
   event: change the title-page version from 0.8.0 to 0.9.0 and add a 0.9.0
   revision-history entry. That entry must state both that the unresolved-space
   condition now maps to discriminant 6 and that this pass appends **no**
   `PreconditionFailureReason`; assignments 10 through 15 remain exactly as
   ratified.

   Also narrow the 0.8.0 history sentence that currently generalizes from its
   original non-`Cmn` case. Preserve the historical fact that detecting a
   non-`Cmn` position needs only its discriminant, but stop claiming that every
   use of `PitchSpaceMismatch` is registry-independent: the new capability
   refusal exists precisely because the pitch-space structure cannot yet be
   resolved. The 0.9.0 entry and amended 0.8.0 rationale must agree when read
   together.
5. **Tests that preserve the contract.**
   * Add purpose-built core tests proving that a non-`cmn-12` `Cmn` position
     refuses `transposed` and makes `twelve_tet_semitone` unavailable.
   * Add an ops test proving the new refusal becomes
     `PitchSpaceMismatch` (6) in the canonical operation effect.
   * Rework `pitch_range_contains_is_advisory_and_frame_aware` and
     `enharmonic_requires_the_same_pitch_space` to use the computable
     CMN-versus-`Integer { space_size: 12 }` pair. Assert both fixtures have a
     `Some` 12-TET value before asserting frame/space rejection, so the tests
     cannot pass through the new unavailability gate. Mutation-check the
     intended frame/space branches.
   * Do not add handling to `resolve_transposed_spellings`: its existing
     `transposed.twelve_tet_semitone()?` correctly refuses the whole operation
     when the gate returns `None`. Preserve that propagation.
   * Expect zero golden churn. Existing generators and fixtures only emit
     `cmn-12`, so unchanged goldens are not coverage; any churn is unexpected,
     and the purpose-built tests above are mandatory.
6. **Requirement counts.** The two new requirements take
   `CORE_REQUIREMENT_COUNT` from 207 to **209** and both suite/label counts from
   277 to **279**. Set `requirement_labels.rs` to **209/279/279**, then verify
   those values by counting; do not increment constants iteratively until the
   test passes.
7. **Tracker.** Mark P13-S2 resolved spec-side; move the registry work to Push
   4b's blocker list with the contradiction struck off.

Rebuild both independently versioned companions twice:
`core_spec.pdf` because the two inserted requirements renumber later
requirements and cross-references, and `operation_catalog.pdf` because the
normative case and catalog semver changed. No code check locks the Operation
Catalog's title-page version, so manually verify that its title page says
**0.9.0** and that the revision history contains the matching 0.9.0 entry.

---

## 8. Traps

* **A ruling about `cmn-24` alone does not settle `maqam-base` or
  `CmnChromatic`.** Rule on the unit, not the row.
* **`nominal.chromatic()` is not a scale factor.** cmn-24's nominal map is
  `[0,4,8,10,14,18,22]`, not `2 ×` the cmn-12 map — `E→F` is one chromatic step
  in cmn-12 and two in cmn-24, not one and two respectively scaled. Anyone who
  implements this as "multiply by `C/12`" gets F and B wrong.
* **The diatonic axis does not move.** Seven nominals, mod 7, in both spaces.
  Only the chromatic axis is parameterized.
* **`twelve_tet_semitone` is not private.** It is public API with six callers
  across three crates, and its name becomes a lie for `C != 12`. Renaming it is
  the honest move and it is a breaking change in `epiphany-core`.
* **Nothing today rejects `Cmn` in a JI or EDO space.** Whatever is ratified,
  the absence of that check is a live defect independent of `cmn-24`, and the
  most likely way for this pass to declare victory while leaving the hole open.
