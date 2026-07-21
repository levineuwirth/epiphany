# Contract: naming core_spec's requirements (P13-S1)

Repo root `/home/jeans/Repos/active/epiphany`. Read this in full before writing
anything. The plan is `spec/PLAN_P13S1_LABELS.md`; both its rulings are granted.

## What this pass is

`core_spec.tex` has **207** requirement blocks. **39** carry a `\label`; **168**
do not, so no conformance claim can cite them. The five companions are 70/70
labelled — `core_spec` is the sole offender. This pass names the 168.

It is **additive**. Never rename, renumber, or move an existing label: 39 in
`core_spec` and 70 in the companions are cited by code, tests, DECISIONS records
and conformance vectors.

## Naming agents write no `.tex`

All 168 edits land in one file, so the naming work is split from the editing
work. **You produce a TSV proposal. You do not touch `core_spec.tex`.** One
later agent applies every proposal after reviewing them together.

### The TSV

One file per chapter at `spec/labels/<chapter-slug>.tsv`. One row per
**unlabelled** requirement, tab-separated, no header:

```
<ordinal>	<proposed-label>	<one-line summary of the rule>
```

`<ordinal>` is the **1-based index of the `\begin{requirement}` occurrence in
`core_spec.tex`, counting every requirement block in file order — labelled ones
included.**

**Do not use line numbers.** The concurrent numbering wave edits the tcolorbox
definition at line 227, upstream of every requirement (the first is at line 966),
so a one-line change there shifts every line number in the file and would apply
168 labels to the wrong blocks. Ordinals cannot move.

Compute them exactly this way, so every agent agrees:

```python
import re
s = open('spec/core_spec.tex').read()
blocks = list(re.finditer(r'\\begin\{requirement\}(.*?)\\end\{requirement\}', s, re.S))
# ordinal is i+1; blocks[i].group(1) is the body; unlabelled iff '\\label{' not in it
```

`<one-line summary>` is for the human reviewing 168 proposals side by side. Say
what the requirement *obliges*, in one clause. It is not a slug and not a quote.

## The label grammar

`req:<area>:<slug>`

`<area>` is fixed **per chapter** by this table. Do not invent one.

| chapter | area |
|---|---|
| Pitch | `pitch` |
| Time and Duration | `time` |
| Tuning Systems and Pitch Spaces | `tuning` |
| The Score Graph | `graph` |
| Semantic Operations and Concurrent Reduction | `semops` |
| Layout Intermediate Representation | `layoutir` |
| File Format | `format` |
| Constraint-Solver Interface | `solver` |
| Performance Requirements | `perf` |
| Extension Points | `ext` |
| Intentionally Deferred Types and Specifications | `deferred` |
| Determinism Contract | `determinism` |

`<slug>` matches `[a-z][a-z0-9-]*` and must:

* **name the rule, not the location or the type.** `spelling-algorithm`, not
  `chapter-2-para-4`, not `pitchspelling`.
* **survive rewording.** A slug describing the *constraint* outlives an editorial
  pass; one quoting the sentence does not. Ask: if someone rewrote this
  requirement's prose next year without changing what it obliges, would the name
  still be right?
* **be unique across the whole suite**, not merely within your chapter. Check
  against every existing label first:
  `grep -rhoE 'req:[a-z0-9]+:[a-z0-9-]+' spec/*.tex | sort -u`

Study the 39 existing `core_spec` labels before naming anything. They are the
house voice and your names must sit beside them without looking foreign.

## Judgment calls you must surface rather than resolve

Put these in your report, not silently in the TSV:

* **Two requirements stating one rule.** Do not invent two names for it. Flag it
  — it is a spec defect of the same family as P13-I1's two-listings drift, and it
  is worth more than a label.
* **A block that is not really a requirement** — a definition, an example, a
  restatement of something ratified elsewhere. Name it anyway so the checker
  passes, but say so.
* **A requirement whose rule you cannot state in one clause.** That usually means
  it obliges more than one thing, which is itself worth reporting.

## Verify your own output

Before reporting:

1. Row count equals the number of unlabelled requirements in your chapters. State
   both numbers.
2. Every ordinal you emit points at a block that is currently **unlabelled** —
   re-derive them with the snippet above, do not trust an earlier scroll.
3. Every label matches `req:<area>:<slug>` with your chapter's area.
4. No slug collides with an existing label or with another row of yours.
5. Tabs, not spaces, between fields. No header line. No trailing blank line.

Report the actual commands and their actual output. On this project an agent once
reported "verification passes" when errors did in fact point into its own file.

## Do not

* Edit `core_spec.tex`, any other `.tex`, or another agent's TSV.
* Run `cargo fmt --all`.
* Rename or move an existing label.
* Guess at a chapter's area prefix — the table above is complete.
