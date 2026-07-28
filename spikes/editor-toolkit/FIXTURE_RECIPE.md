# T4 spike fixture recipe (pin 7)

**Approved 2026-07-28** at the recipe level; moved here from
`spec/DRAFT_T4_FIXTURE_RECIPE.md`. Revision 1 asserted three things about the
pipeline that reading it disproves (ties as curves, two rest glyphs from
eighth-density, multi-subpath coverage from 4/4). Revision 2 fixed those and
introduced three of its own: a command census that skipped `H`/`V`, barlines
filed as strokes, and ledger quotas expressed in "positions" that can mean zero
ink. This revision corrects those and drops ties to zero.

`CONTRACT_EDITOR_T4_SPIKE.md` pin 7 requires this recipe to be committed and
**user-reviewed before any candidate-specific implementation begins**, because
staff × measure dimensions do not pin the workload and the workload is what
gets tessellated. That review is discharged; the generator's census (below) is
a separate, still-open gate.

**One recipe, scaled across rungs by dimension only.** F1 1 staff × 10
measures, F2 4 × 32, F3 12 × 100, F4 24 × 200. Every rate is an integral
per-measure quota under one fixed cadence, so scale is the only thing that
moves between rungs — up to bounded edge effects where a rung's measure count
does not divide the cadence, which the census reports rather than the recipe
glossing.

---

## What the pipeline actually draws

Verified in `epiphany-layout-ir` and `epiphany-engrave` before choosing any
rate, because a recipe that asks for ink the pipeline does not emit silently
under-delivers.

* **Curves are slurs, and only slurs.** `Curve`'s own doc: "Slurs engrave to
  one of these; **ties and other span curves will follow**"
  (`constrained.rs:109-111`) — future tense. `constrained.rs` has a
  `TypedObjectId::Slur` arm and **no `Tie` arm**; ties fall through with the
  other structures, which "are carried as zero-extent traced [anchors]"
  (`constrained.rs:734`). Revision 1 counted ties as curves and inflated every
  curve figure by 50%. The casting lines I cited only *transform* curves
  already produced upstream — they were evidence of nothing.
* **Rest glyph follows note value exactly** — `rest_glyph`
  (`engrave_theory.rs:112`): Whole→`restWhole`, Half→`restHalf`,
  Quarter→`restQuarter`, Eighth→`rest8th`, and **`None` for anything shorter**,
  deliberately, so missing coverage surfaces instead of misrendering. An
  all-eighths recipe therefore yields `rest8th` **only**; revision 1 claimed
  both quarter and eighth rests from a rhythm that cannot produce them.
* **`timeSig4` is single-subpath** (measured: 1 subpath, 34 commands). Revision
  1's claim that 4/4 brings multi-subpath coverage "free" was simply false.
  `timeSig8` is the multi-subpath digit (3 subpaths) and 4/4 does not use it.
* **Beams are NOT drawn.** `beam_slope_penalty` is "vacuous 0.0: no beam
  geometry is drawn yet (beams exist logically, not visually)"
  (`quality.rs:40-41`). Eighth density here is an *event-count* choice, not a
  beam-ink claim. When beams land, the stroke mix changes and this is revisited.
* **Strokes** — staff lines, stems, ledger lines, plus zero-extent traced
  anchors (`casting.rs:1081`, `lib.rs:119`). **Barlines are glyphs, not
  strokes** (`constrained.rs:1455`) — revision 2 listed them wrongly.
* **Ledger lines are emitted per staff step, not per off-staff note**
  (`ledger_steps`, `constrained.rs:2642-2659`): the even steps strictly outside
  the staff, from the staff out to the note. By absolute staff step:
  **−1 and +9 emit 0 ledgers; −2, −3, +10 and +11 emit 1; −4 and +12 emit 2.**
  The recipe therefore pins off-staff pitches by **exact staff step**; "±1 and
  ±2 ledger positions" (revision 2) would have produced roughly half the ledger
  ink it implied.
* **Clefs are emitted per staff instance**, not per system (no restatement on
  later systems — `editor-gui/DECISIONS.md`), which holds given the topology
  pinned below.

### Measured glyph cost, since accidental *rate* does not control complexity

**The census definition is `BravuraGlyphCatalog::render_data(name).outline.len()`**
— the typed `Vec<PathCommand>` candidates actually consume. Revision 2 counted
`M`/`L`/`C`/`Z` tokens in the source `d` string and was wrong for it: the
parser converts each `H` and `V` to a `PathCommand::LineTo`
(`glyphs/src/path.rs:92`), so those are real commands in the tessellator's
input, not serialization shorthand. Recounted with them:

| glyph | subpaths | commands |
|---|---|---|
| `accidentalSharp` | 2 | **71** |
| `accidentalDoubleSharp` | 1 | 37 |
| `accidentalNatural` | 2 | 29 |
| `accidentalFlat` | 2 | 24 |
| `noteheadBlack` | 1 | 6 |
| `restQuarter` | 1 | 38 |
| `rest8th` | 1 | 19 |
| `gClef` | 4 | 59 |
| `fClef` | 3 | 28 |
| `timeSig4` | 1 | 34 |

A sharp costs **2.4×** a flat. So the accidental *mix* must be pinned, not just
the rate.

---

## The recipe

**Nothing is sampled.** Every quantity below is an integral per-measure quota
placed by a fixed rule, so the content is identical run to run, machine to
machine, with no rounding policy needed. The generator still takes a seed, but
its only role is replica/identity minting as in the testkit fixtures — **it
does not influence content**. That is strictly stronger than revision 1's
"fixed seed", which left the selection algorithm free.

### Topology

* **One region**, containing **one staff instance per staff**. No nested or
  parallel regions.
* Staves take **`gClef` on even indices, `fClef` on odd** — both complex
  multi-subpath clefs, in a mix a real keyboard/orchestral score would have.
* **4/4** throughout: `timeSig4` twice per staff (numerator + denominator).
* **Single-pitch note events only — no chords.** Chord cardinality would
  change notehead-per-slot density and stem sharing, and is not a variable this
  comparison needs.
* **One voice per staff instance** (see the limitation below).

### Per measure, per staff — a fixed 7-event bar

Rhythm: **six eighths + one quarter** = 6/8 + 2/8 = 4/4 exactly. Seven events,
metrically valid, and it makes both rest glyphs reachable.

| Quota | Value | Placement rule |
|---|---|---|
| Events | **7** | positions 1–6 eighths, position 7 quarter |
| Rests | **1** | **even measures: the quarter (→ `restQuarter`); odd measures: eighth position 3 (→ `rest8th`)** — an exact 50/50 split of the two bundled rest glyphs |
| Accidentals | **1** (of 6 pitched events = **16.7%**, the integral replacement for revision 1's 15%) | on pitched event position 2; glyph cycles **sharp → flat → natural** by `measure index mod 3`. `accidentalDoubleSharp` excluded: rare in real music and single-subpath, so it adds cost without adding hole coverage |
| Ledger-bearing notes | **1** | on pitched event position 5; staff step cycles **−2 → −4 → +10 → +12** by `measure index mod 4`, giving ledger-stroke counts **1, 2, 1, 2** — an average of **1.5 ledger strokes per measure**, pinned by step rather than by a "ledger position" that can mean zero ink |
| Slurs | **1** | spanning positions 1→4, **within the measure** — never crossing a barline or system break, since a slur whose endpoints do not resolve to one staff traces instead of producing a curve (`constrained.rs:1605`) |
| Ties | **0** | see below |

**Ties are zero for the primary ladder**, on the review's reasoning, which is
better than my revision-2 argument for keeping them. Two independent grounds:
the rule I wrote tied positions 6→7 while position 7 *is* the rest on even
measures, which is an **invalid tie pairing**
(`epiphany-core/src/invariants.rs:2101`); and — decisively — every structural
tie anchor is emitted at the **default x, which is the clef column**
(`constrained.rs:1155`, `default_x = region_x + CLEF_X`), so casting assigns
them all to the **first system** regardless of their musical measure. At F4
that would pile 2,400 anchors into the most likely damage target and distort
per-system rebuild timing — corrupting the measurement the whole spike turns
on. Revisit when ties draw real geometry.

**Pitch placement** is deterministic and quota-respecting: two ordered pitch
lists — *on-staff* (the staff's own span) and *off-staff* (the four steps
listed above) — each advanced cyclically. The designated ledger event draws from the
off-staff list; every other pitched event draws from the on-staff list. Pitch
never depends on the seed.

**One fixed cadence, with bounded edge imbalance — not exact thirds.** 10, 32,
100, and 200 are none of them divisible by 3, so the sharp/flat/natural cycle
leaves at most one extra of one or two glyph types per staff; and **F1 has a
single staff, so it is `gClef` only** while every larger rung alternates
clefs. The cadence is identical and deterministic at every rung; the resulting
*counts* differ slightly at the edges, and the census reports what they are
rather than the recipe claiming they are equal.

---

## What that produces

Events and quotas are exact. Primitive counts are **estimates to be replaced by
the generator's measured census** (below) — pin 7 requires glyphs / strokes /
curves separately plus total path-command count.

| Rung | staves × measures | events | pitched | **curves (slurs)** | accidentals | ledger strokes |
|---|---|---|---|---|---|---|
| F1 | 1 × 10 | 70 | 60 | **10** | 10 | 15 |
| F2 | 4 × 32 | 896 | 768 | **128** | 128 | 192 |
| F3 | 12 × 100 | 8,400 | 7,200 | **1,200** | 1,200 | 1,800 |
| F4 | 24 × 200 | 33,600 | 28,800 | **4,800** | 4,800 | 7,200 |

Plus stems (≈ one per notehead), staff lines (5 × staves × systems), and
barline **glyphs**.

---

## Approve the generator, not just the recipe

Per the review, and I agree it is the right gate: the generator lands **before
any candidate code**, and emits a **per-rung census** — per-type primitive
counts, total path commands, and a glyph-name histogram — plus a **`SHA-256`
hash of each rung's `ResolvedLayoutIR` canonical bytes**, the algorithm named
here rather than left to the implementation. That census is what you approve;
the numbers above are my arithmetic, and the census is the measurement. The
hash then makes every later round's fixture provably the one that was approved.

**This is the standing gate: no candidate code exists until that output is
approved.** Recipe approval is not generator approval.

**F4's engrave time is measured in that same preflight**, against the corrected
generator — not now, and not against an underspecified recipe, where a timing
could not settle anything. If F4 exceeds pin 7's ten-minute wall it is dropped
and the deciding rung becomes **the largest remaining engraving-valid rung** —
expected to be F3, but not unconditionally F3.

---

## Stated limitation: one voice

**Corrected from revision 1, which overstated the gap.** Two-voice engraving is
*not* unproven: `rs4_two_voice_counterpoint_passes_minimal`
(`testkit/tests/reference_suite.rs:89`) exercises it against the real engraver
and passes, and logical projection iterates every voice. Revision 1 inferred a
gap from casting tests that happen to read `voices[0]` — evidence about those
tests, not about the engraver, which is the same reading-*a*-path-for-*the*-path
error that has bitten this track before.

What is genuinely unverified is narrower: **collision handling between voices,
rest displacement, and deliberate stem-direction separation**. The deciding
ladder stays at one voice as an isolation choice — a spike measuring
tessellation should not also be exercising the engraver's least-proven
geometry — and a multi-voice engraving check belongs on its own, later.

## What this recipe deliberately does not vary

Dynamics, articulations, text, tuplets, grace notes, repeats. Several are
unengraved today (§3.7's registry), and the rest add model surface without
adding a distinct *primitive type* — the tessellator sees glyphs, strokes, and
curves, and this recipe's job is to load those three in a realistic ratio.
Text is the exception and is covered by round 2's synthetic `SpikeResolvedText`
fixtures, where it belongs.
