# Contract: P13-S6 — drafting the built-in tuning constructions for review

Repo root `/home/jeans/Repos/active/epiphany`. The plan is
`spec/PLAN_PUSH4B_TUNING.md`, Ruling B. Read it before starting.

## What this is, and why it is a draft

`req:tuning:builtin-tuning-catalog` makes all **20** built-in tuning
identifiers MUST-resolve with normative semantics. Only the six `tet-*` are
actually specified — structurally, by `TuningResolution::EqualTemperament`. The
other 14 are bare names (P13-S6). Ruling B keeps all 20 and pins them **by
construction** rather than by cents table, because published sources agree on
the constructions and differ mainly in how they round cents.

**You are producing a reviewable draft, not a specification edit.** Write
`spec/DRAFT_P13S6_TEMPERAMENTS.md`. **Do not edit any `.tex` file.** Promotion
into `core_spec.tex` happens only after human review.

The reason is specific and this project has paid for it: `NOTEHEAD_ANCHORS` was
a table of hand-written glyph metrics that looked authoritative, was wrong in two
independent ways, and survived for passes because nothing consumed it. A tuning
construction written from memory into a normative document is the same artifact.
**Confidence is not a source.**

## The deliverable

For each of the 14 unspecified systems, one entry:

* **Identifier** exactly as the catalog spells it.
* **Construction** — the generative rule, stated so an implementer can compute
  it without further research. For a circulating temperament that means: which
  fifths are tempered, by what fraction of *which* comma (syntonic vs
  Pythagorean — say which; they are different and confusing them is the classic
  error), and which remain pure. For a JI scale it means the ratio for each of
  the twelve degrees.
* **Wolf / chain placement** where the construction does not determine it —
  `pythagorean` in particular is a pure 3:2 chain whose twelve-note selection is
  a *choice*, and the choice must be stated, not assumed.
* **Source** — a specific, checkable citation: author, work, date, and where the
  construction appears. "Common knowledge" is not a source.
* **Confidence**, explicitly: `verified` (you can cite it precisely),
  `recalled` (you believe it but cannot cite it precisely), or `unknown`.

**A `recalled` or `unknown` entry is a useful result and an honest one.** A
fabricated citation is the worst possible output of this task — worse than
leaving the row blank — because it defeats the review that exists to catch it.
If you cannot source something, say so in that entry and move on.

The 14: `pythagorean`; `meantone-1/4-comma`, `meantone-1/6-comma`,
`meantone-1/5-comma`; `werckmeister-iii`, `werckmeister-iv`; `vallotti`;
`kirnberger-ii`, `kirnberger-iii`; `young-ii`; `ji-static-5limit-C`,
`ji-static-5limit-G`, `ji-static-5limit-D`; `ji-adaptive-5limit`.

## Four are open ratifications — surface, do not decide

The plan marks these as needing a human decision. **Present the options and
their trade-offs; do not pick.**

* **`ji-static-5limit-C` / `-G` / `-D`.** Which 12-note 5-limit scale — the comma
  choices for the chromatic degrees are genuinely unsettled and more than one
  selection is defensible. Lay out the main candidates and what each optimizes.
  Note whether the three differ only by transposition of one scale or are
  independently chosen.
* **`ji-adaptive-5limit`.** Needs a real algorithm, constrained by
  `req:tuning:adaptive-tuning-purity` — **read that requirement first** and state
  what it permits. The in-house pattern for this shape is
  `req:pitch:spelling-algorithm`, which pins `SpellingAlgorithmId "default"` at
  version 1 to a named algorithm and errors on any other identifier; recommend
  the analogous versioned `AdaptiveTuningFunctionId`, but do not invent the
  algorithm's content as though it were settled.

## Cents tables are derived, never primary

If you include cents, mark them **derived from the construction** and give the
derivation. Do not copy a cents table from a source and present it as the
definition — that reintroduces exactly the rounding variance Ruling B exists to
avoid. Exact-ratio form is preferred wherever a construction yields rationals;
say plainly where it does not (quarter-comma meantone's fifth is $5^{1/4}$, and
several others are irrational by construction).

Note for context, so you do not over-engineer precision: resolved frequencies
never enter canonical bytes (`req:determinism:canonical-floating-point` lists
tuning among the floating-point contexts and bars floating point from anything
needing exact identity), and acoustic comparison is governed by
`ToleranceClass::AcousticCents` in `epiphany-determinism`.

## Check the arithmetic, not just the source

**A citation proves the source said it. It does not prove it is right, that you
read it correctly, or that you transcribed it completely.** Sourcing catches
fabrication; only an invariant catches a real source misapplied. This section
was added after a first draft produced two impossible temperaments and one false
ambiguity, every one of them properly cited.

Where the data has an internal invariant, **compute it and show the computation
in the entry**. For tuning constructions the invariants are:

1. **Closure.** A circulating (wolf-free, twelve-note) temperament's twelve
   fifths MUST absorb **exactly one Pythagorean comma** — $531441/524288$,
   23.4600 cents. Narrowings count positive, widenings negative. State the sum.
   A construction that does not close is not a well temperament, and if your
   sum lands 1.9537 cents short you have dropped a **schisma**-tempered fifth —
   the difference between the Pythagorean and syntonic commas, and the single
   most commonly omitted element of these constructions.
2. **Closure is also a decision procedure.** Where a source is vague about
   *which* comma a fraction refers to, closure usually settles it: try both and
   report which one closes. Do not carry an ambiguity forward that arithmetic
   resolves — a normative hedge on an answerable question is worse than either
   answer.
3. **Non-circulating temperaments must NOT close.** Meantone and Pythagorean
   have a wolf by construction. Compute the wolf's size — it is the residue —
   and state it. "Does not close" is a positive claim here, not a gap.
4. **Twelve fifths, each exactly once.** Enumerate the full circle and classify
   every fifth as narrowed, widened, or pure. A construction naming eleven or
   thirteen is wrong on its face; adjacent fifths sharing a note are two
   distinct fifths.
5. **Recompute every derived claim.** If a source says a third is pure, derive
   it from your own chain and confirm. If your derivation disagrees with the
   source, that is a finding — report it, do not quietly follow either one.

If an invariant fails, **say so in the entry and mark it unresolved**. Do not
adjust the construction to make the arithmetic work and present the result as
sourced — that manufactures a temperament nobody published.

## Do not

* Edit any `.tex`, any Rust file, or any other agent's file. **Your entire output
  is one new Markdown file.**
* Take values from anywhere in this repository — there are none, and anything you
  think you found is something else.
* Present a construction you cannot source as though you could.
* Decide any of the four open ratifications.

## Report

State how many of the 14 you marked `verified`, how many `recalled`, how many
`unknown`. That ratio is the single most useful number in your report, and a low
`verified` count is a finding about the task, not a failure of it.
