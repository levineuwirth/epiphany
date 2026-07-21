# P13-S1 — naming the requirements: scope and plan

Status: **approved for dispatch. Requirements will be numbered; the Determinism prefix is `determinism`.**
Prepared against `master` @ `a9a5712`. Every claim was checked against the source;
where I ran a count I give it.

---

## 1. The task is not what the tracker says

P13-S1 is filed as *"169 of core_spec's 207 requirement blocks carry no `\label`,
so no conformance claim can cite them."* The count is right (**168** today) and
the diagnosis is right. But labelling alone does not deliver a citable
requirement, because **no document in the suite numbers its requirements.**

`\newtcolorbox{requirement}` has no counter, in all six documents. So a
`\label` inside one binds to the last incremented counter — the enclosing
sectioning unit — and `\ref` renders a *section* number:

> `...see Requirement 2.5.4` — 2.5.4 is a subsubsection.

This is already ambiguous today, in documents I bumped this session:
**`text_projection.pdf` renders five different requirements as "Requirement
3.1."** In `core_spec`, **25 sectioning units contain more than one requirement**,
so 25 sites where added labels would collide on their rendered number.

Adding 168 labels to that scheme produces 168 citable-but-ambiguously-rendered
references. Numbering is a **prerequisite**, not a follow-up.

### Ruling: give `requirement` a counter

Accepted: use `auto counter, number within=chapter`, with the box title showing
`Requirement~\thetcbcounter`, in all six documents. Every `req:*` label then
binds to its requirement's counter, `\ref{req:pitch:spelling-algorithm}` yields
a unique requirement number within its document, and the box header identifies
the requirement it contains.

Cost: every rendered "Requirement X.Y.Z" in all six PDFs changes to a requirement
number. That is churn in the PDFs and in the rendered text of the 63 existing
cross-references in `core_spec` alone — but the new text is *correct* where the
old text was misleading. No `req:*` label string changes, so **no code, test, or
conformance vector is affected**: those cite labels, not numbers.

---

## 2. What I verified

| fact | value |
|---|---|
| `core_spec` requirement blocks | **207** |
| labelled | 39 |
| **unlabelled** | **168** |
| every existing label matches `req:<area>:<slug>` | 39/39 |
| the five companions | **70/70 labelled** — `core_spec` is the sole offender |
| `req:*` strings cited anywhere but never defined | **1** |
| requirement blocks opening with a `\textbf{...}` lead | 18/207 |

**The one dangling citation is `req:layoutir:vertical-bands`**, cited twice in
`spec/PASS12_RATIFICATION_LOG.md`. It was never a requirement, and no labelled
requirement governs vertical-band *heights*, which is what both entries describe
— so the honest correction is to say so, not to point them at the two *ownership*
requirements (`req:layoutir:primitive-band-ownership`,
`req:layoutir:resolved-band-ownership`), which govern something else.
A historical log citing a name that does not exist is a wrong pointer, not a
record of what was decided; recommend correcting it to the real label.

**Naming is judgment work, not transcription.** Only 18 requirements open with a
bolded lead sentence a slug could be derived from mechanically. The other 189 must
be read and named for the *rule they state*. That is the whole cost of this pass,
and it is what parallelizes.

---

## 3. Per-chapter distribution and the area table

Area prefixes are **fixed here** so parallel agents cannot diverge. Seven are
already established by the 39 existing labels; five chapters have none and need
one. House style across the suite is short and lowercase (`binfmt`, `opcat`,
`qmc`, `refsuite`, `semops`, `layoutir`).

| chapter | blocks | todo | area prefix |
|---|---:|---:|---|
| Pitch | 13 | 10 | `pitch` (established) |
| Time and Duration | 20 | 16 | `time` (established) |
| Tuning Systems and Pitch Spaces | 9 | 9 | **`tuning`** (new) |
| The Score Graph | 28 | 22 | `graph` (established) |
| Semantic Operations and Concurrent Reduction | 27 | 24 | `semops` (established) |
| Layout Intermediate Representation | 22 | 11 | `layoutir` (established) |
| File Format | 34 | 24 | `format` (established) |
| Constraint-Solver Interface | 18 | 16 | `solver` (established) |
| Performance Requirements | 16 | 16 | **`perf`** (new) |
| Extension Points | 4 | 4 | **`ext`** (new) |
| Intentionally Deferred Types | 1 | 1 | **`deferred`** (new) |
| Determinism Contract | 15 | 15 | **`determinism`** (new; fixed by ruling) |

**Slug rules** (also fixed here):

* lowercase, hyphenated, `[a-z][a-z0-9-]*`;
* names the **rule**, not the section or the type — `spelling-algorithm`, not
  `chapter-2-para-4` and not `pitchspelling`;
* stable under rewording: a slug that describes the *constraint* survives an
  editorial pass, one that quotes the sentence does not;
* unique across the whole suite, not merely within the chapter.

---

## 4. Work breakdown — propose, then apply

**Every requirement to label lives in one file.** `core_spec.tex` is 15k+ lines and all 168
edits land in it, so the parallel fan-out that worked for the last three waves
would put eight agents in one file. It must not.

Split the judgment from the edit:

### Wave 1 — numbering (one agent; may run concurrently with Wave 2)

Add `auto counter, number within=chapter` to `requirement` and render
`Requirement~\thetcbcounter` in the box title in all six documents; rebuild all
six PDFs; confirm every existing `\ref{req:...}` still resolves and now renders
a requirement number. 63 cross-references in `core_spec` alone will change
their rendered text — that is the point.

### Wave 2 — naming (seven agents in parallel, **no `.tex` edits**)

Each agent takes a set of chapters and emits **one TSV file per chapter** under
`spec/labels/<chapter-slug>.tsv`, with one row per unlabelled requirement:

```
<line-number-of-\begin{requirement}>	<proposed-label>	<one-line summary of the rule>
```

No agent touches `core_spec.tex`. Conflicts become impossible, and the whole
proposal is reviewable in one place — 168 names side by side, which is the only
way to catch the near-duplicates and the inconsistent verb tenses that a
chapter-at-a-time review misses.

Suggested split, balanced by count: (File Format 24) · (Semantic Operations 24) ·
(The Score Graph 22) · (Time 16 + Pitch 10) · (Solver 16 + Layout IR 11) ·
(Performance 16 + Determinism 15) · (Tuning 9 + Extension 4 + Deferred 1).

### Wave 3 — review, apply, and lock (one agent)

Review all TSVs together for duplicate concepts, inconsistent phrasing, and
slug collisions; resolve those proposal defects before editing. Then apply all
168 accepted rows to `core_spec.tex`, correct both dangling `vertical-bands`
pseudo-label citations to the real ownership requirement each row describes,
and land the checker.

---

## 5. The checker — the durable half

A committed test, in the shape of `text_projection_grammar.rs`. It must enforce:

1. **Every** `\begin{requirement}` in **every** `spec/*.tex` carries a `\label`.
   Not "most", not core_spec only — the companions are at 70/70 and must stay.
2. Every label matches `req:<area>:<slug>` with the slug grammar above.
3. The area matches the chapter, per the table in §3, held as data in the test.
4. Labels are **unique across the suite**.
5. **No `req:*` string cited anywhere in the repo is undefined** — this is what
   kills the `vertical-bands` class of defect permanently, and it is the check
   with the most future value.

Assert the counts, so the test cannot pass by scanning nothing: 207 blocks in
`core_spec`, ≥277 across the suite, ≥110 distinct citations resolved.

---

## 6. Traps

* **Do not renumber or rename an existing label.** 39 in `core_spec` and 70 in the
  companions are cited by code, tests, DECISIONS records and conformance vectors.
  This pass is additive.
* **A `\label` must sit inside its `requirement` block**, after any `\textbf`
  lead. Placed before `\begin{requirement}` it binds to the wrong thing and the
  checker will not catch it — only the rendered PDF will.
* **`\ref` renders a number, `\nameref` renders a title.** Existing prose says
  "Requirement~\ref{...}"; keep that form.
* **Two requirements can state one rule in different chapters.** Where that
  happens, do not invent two names for it — flag it, because it is a spec defect
  (P13-I1's two-listings drift in another costume).
* Rebuild **all six** PDFs. `core_spec` is the one being edited, but Wave 1
  touches every document's box definition.
