# Contract — P13-S26: invariant 10's reference surface, derived

STATUS: LANDED by this commit.

**Lifecycle record.** RATIFIED 2026-08-11 on the authority of the repository
owner, after the independent whole-artifact passes recorded below — the last
returning zero findings. DISPATCHED 2026-08-11; amendment 1 ratified at
`f9170b0`, amendment 2 at `86bf7c6`. Which passes closed and what each found
are those records; this does not restate them, and states no count of them, per
the rule this contract adopted after its own tallies went stale twice. **No hash
of the landing commit appears above**: a commit cannot carry its own id, so if
one is wanted it arrives by a later administrative amendment.

**THE PINS ARE FROZEN. They may be executed, not edited.** A defect found during
execution is **reported, not patched in place** — if it needs a pin change, that
is its own amendment with its own review round.

Owning candidate: **P13-S26** (`spec/PASS13_CANDIDATES.md`, Batch 3).
Rung class: **documentation-and-guard. No tag change, no behaviour change.**

---

### REVIEW ROUND 18 — independent. One blocking finding. ACCEPTED.

1. **[P1] Gate 10 observed only part of pin 7.** It required both row ids and
   expanded S30's four consequences and M20 evidence, but did not require
   S29's rendered-example citation, S29's behaviour-change classification,
   S30's test-scope/no-production-consumer scoping result, or either row's
   unresolved/out-of-scope disposition. A stub S29 row, or a row incorrectly
   marked resolved, satisfied the gate. **Fixed** — gate 10 now checks the
   complete filing contract for each row, including evidence, scope and status.

**Review-process correction:** this finding came from reading pin 7 and gate 10
side by side, not from another search for likely omissions. Future contract
reviews should begin with that complete cross-reference pass: for every pin,
identify its changed artifact, touch row, machine test or explicit read-check,
and signing mutation/evidence where one is claimed. A cell is either populated
or marked **N/A with a reason**; an unexplained empty cell is a finding in the
same round. This is a review method, not another execution gate for S26.

---

> **DATED HISTORICAL RECORD — every review block below, through to §0, is an
> account of what a pass found on the date it ran. None of it states current
> state. The rung landed; the pins were executed, not edited.**

---

### RATIFICATION CHECK 2 — independent, whole-artifact. **NO-GO: one completeness correction.** ACCEPTED.

1. **[P1] S30's "exhaustive" inventory covered only the opening delimiter.**
   Both parsers match the **closing** form exactly too —
   `requirement_labels.rs:167` (`let end = r"\end{requirement}"`) and
   `text_projection_grammar.rs:649`
   (`split_once("\\end{requirement}").expect(…)`) — and the row listed neither.
   **Fixed**, and it is not symmetry for its own sake: the failure differs by
   which delimiter is spaced.

   **Exact opener with spaced closer** was traced, not assumed. The scan finds
   the opener and hunts an exact close: with **no later exact close** both
   parsers **panic**, with two distinct messages; with **a later exact close**
   the scan **consumes through it**, swallowing the following block's opener so
   that block is never recorded. Usually loud — the consuming block inherits ≠1
   label — **but a second silent case exists**: an additive, label-free block
   with exact opener and spaced closer, placed immediately before an existing
   single-label block **in the same chapter**, inherits that one label, replaces
   the swallowed block one-for-one in the tally, leaves the whole-text label
   scan untouched, and keeps the same chapter. Every count holds.

   So the requirement-block consequence has **two** silent cases, not one, and
   the silent-cases paragraph now says so.

2. **M20 is qualified as opener-only evidence.** Its probe spaces **both**
   delimiters, so no parser reaches the opener and none ever hunts a close — it
   cannot exhibit consequence (ii). Pin 7 and gate 10 both say this explicitly;
   a rung wanting (ii) exhibited owes its own probe. *An unqualified citation
   would have made the row claim evidence it does not have — the same class as
   round 13's premature "M20 is the evidence".*

3. **Gate 10's inventory requirement now spans both delimiters** of every
   requirement-block scanner, so the omission cannot recur by transcription.

---

### RATIFICATION CHECK — independent, whole-artifact. **NO-GO: three corrections.** ALL ACCEPTED.

1. **[P1] Pin 1a contradicted pin 6.** It said an out-of-vocabulary term "fails
   on the same assertion" as the ordering check — **the exact error round 3
   caught in pin 6 and fixed there only.** A bad term sorts perfectly well, so
   order cannot see it; that is why pin 6 step 4 splits into (a) order and (b)
   vocabulary. **Fixed** — pin 1a now states the split and points at the two
   signing mutations.
2. **[P1] P13-S30's consumer sweep was incomplete**, and worse than unswept:
   `binary_format_history.rs` **appeared in the round 12 grep** and was never
   carried into the row. Also omitted were the exact `\label{…}` guards in
   `text_projection_grammar.rs`. **Fixed by re-deriving the inventory from an
   exhaustive grep rather than from recall**, and the result is larger than
   either list: `text_projection_grammar.rs` at `:73`, `:341`, `:363`, `:508`,
   `:538`, `:591`, `:596`, `:647`; `binary_format_history.rs:107`–`:112`, whose
   `.expect` **panics** on a spaced heading; `parse.rs:726`. **Gate 10 now
   requires the row to preserve the inventory in full**, so the omission class
   is observed rather than trusted.
3. **[P2] §0.6's locator claim was too absolute.** Pin 10 retains a verbatim
   `requirement_labels.rs:486:5` diagnostic. **Fixed** — the claim is narrowed
   to *operative* locators: nothing is found, sliced or asserted by coordinate,
   while coordinates inside frozen diagnostics are quoted evidence that
   reproducing exactly is the whole point of.

*Finding 2 is the sharper lesson: a sweep whose result is not written down is
not a sweep. I ran that grep, read the hit, and let it die in the transcript.*

---

### REVIEW ROUND 17 — independent. One blocking finding. ACCEPTED.

1. **[P1] Round 15's block still carried the superseded conclusion**, one
   paragraph below its own correction: *"the consequence reads 'silent when
   additive and label-free'"*, against the live pin's *"additive and free of
   `req:` labels"*. **Fixed** — struck and corrected. Round 16's closing claim
   that the record was *"struck in place with both corrections"* is struck too:
   **two of three instances were struck, not all of them**, so that sentence was
   false as written.

**This is the one-hop failure at its narrowest** — not a correction that reached
one document and stopped, but one that reached two sentences of a paragraph and
stopped at the third. Rounds 13, 14, 16 and 17 have each found a variant of it.
The standing remedy in P13-S16 §7 is to **grep the superseded phrase rather than
re-read for it**; that is what produced this fix, and it is what round 16 should
have done before claiming the record was fully struck.

---

### REVIEW ROUND 16 — independent. One blocking finding. ACCEPTED.

1. **[P1] M20's isolation conditions were conflated with the parser defect's
   silence conditions**, and one supporting fact was wrong. Verified in the
   source:
   - `CORE_REQUIREMENT_COUNT` and `SUITE_REQUIREMENT_COUNT` count **blocks**
     (`document.requirements.len()`); `SUITE_LABEL_COUNT` counts **labels from a
     whole-document scan**. So re-spacing an existing labelled block drops the
     two **block** counts — **not** `SUITE_LABEL_COUNT`, as round 15 claimed;
     the label remains visible precisely because collection is whole-text. That
     is the same fact round 15 used correctly one sentence later and misapplied
     here.
   - **Parser silence** therefore requires: *additive*, and *no globally
     recognized `req:` label*. A `tmp:`-namespaced label is invisible to
     `labels()` and does not break silence.
   - **Label-freedom is M20's own isolation condition**, not a silence
     condition: any label inside item 10's slice would additionally trip test
     1's independent no-`\label` assertion, widening M20's radius to two
     assertions and destroying the separation M18/M20 exist to maintain.

   **Fixed** in pin 7's consequence, the silent-cases paragraph and M20's
   rationale, which now labels each of its three properties by which purpose it
   serves. ~~Round 15's record is struck in place with both corrections.~~
   **— corrected by round 17: two of three instances were struck. The third,
   in the same paragraph, still stated the superseded conclusion.**

---

### REVIEW ROUND 15 — independent. Two blocking findings. ALL ACCEPTED.

1. **[P1] The requirement-block branch is not unconditionally silent, and M20's
   clean radius rested on an unstated condition.** Traced: ~~re-spacing an
   *existing labelled* block drops `CORE_REQUIREMENT_COUNT` **and**
   `SUITE_LABEL_COUNT`~~ **— corrected by round 16: it drops
   `CORE_REQUIREMENT_COUNT` and `SUITE_REQUIREMENT_COUNT`, both *block* counts;
   the label stays visible, so the label counts hold** — and a hidden block that
   carries a `req:` label still
   moves `SUITE_LABEL_COUNT`, because `all_defined_labels` calls `labels()` on
   the **whole document text**, not per block. ~~Only an **additive, label-free**
   probe is silent.~~ **— corrected by round 16: parser silence needs only
   *additive* and *no `req:` label*; label-freedom is M20's isolation
   condition.** **Fixed** — M20's content is pinned exactly, with each of
   its three properties justified by the failure it avoids; the consequence
   reads ~~*"silent when additive and label-free"*~~ **— corrected by round 17,
   the third and last instance of this round's superseded conclusion: the live
   pin reads *"silent when additive and free of `req:` labels"*** ; and gate 10 now cites the
   **requirement-block silent case**, one of two, rather than "the" silent one.
2. **[P1] The chapter partition was incomplete.** Four outcomes, by what the
   predecessor is: **(a)** no prior recognized chapter → `load_spec` panics;
   **(b)** predecessor absent from `CHAPTER_AREAS` → the area test's
   `unwrap_or_else` panics with "missing chapter-area data"; **(c)** predecessor
   in a different area → loud mismatch; **(d)** predecessor in the same area →
   silent. **Fixed**, all four recorded. Also corrected: `binary_format.tex` has
   **twelve** chapter headings, of which **seven** appear in `CHAPTER_AREAS` — I
   had written "all seven chapters", conflating the table's coverage with the
   file's contents. The `Operation Wire Forms` example survives, and is now
   anchored: `Graph Value Layouts` (`:814`) immediately precedes it (`:1196`).

*Both findings are one class: a consequence recorded without the condition that
produces it. Pin 7 now says why that matters — an unconditioned "silent" invites
a later reader to reproduce it the loud way and conclude the candidate is wrong.*

---

### REVIEW ROUND 14 — independent. Two blocking findings. ALL ACCEPTED.

1. **[P1] The temporal sweep stopped one hop early.** Round 13 made S30's
   evidence claims prospective and left *"two defects are filed"*, §0.4's
   filing sentence, both `Filed with…` clauses and §5's S29 bullet asserting a
   state execution has not yet produced. **Fixed** — every live occurrence is
   prospective, and **the ledger row remains the definition of filed**, not this
   contract's description of it. *This is the correction-propagates-one-hop
   defect, in the round whose own subject was premature state.*
2. **[P1] The chapter branch is not always loud.** `load_spec` binds a
   requirement to the previous *recognized* chapter while the area test compares
   only the coarse area — so a lost `\chapter {…}` between chapters that share
   an area is **silent**. Verified: ~~all **seven** `binary_format.tex` chapters
   map to `binfmt`~~ **— corrected by round 15: the file has twelve chapter
   headings, seven of which appear in `CHAPTER_AREAS`** — so losing
   `\chapter {Operation Wire Forms}` rebinds its
   requirements to `Graph Value Layouts` and the suite stays green. **Fixed** —
   the consequence is now "silent or loud, by neighbourhood", with the loud
   cases named (differing areas; the first chapter, which trips the
   "requirement before first chapter" panic).

   **This changes the candidate's weight**, so pin 7 now says it: **two** of the
   four consequences can pass silently, not one. A defect that fails loudly is
   at worst misdiagnosed; one that passes is not seen at all.

---

### REVIEW ROUND 13 — independent. Two blocking findings. ALL ACCEPTED.

1. **[P1] The fourth consequence was overstated twice.** `parse.rs:726` sits
   inside `#[cfg(test)] mod tests` (opened at `:639`) — **it is not production
   code**, and my round 12 sweep claimed otherwise by treating a path under
   `src/` as production without checking reachability. Separately, the read
   proves **duplicated exact-string scanners**, not *present divergence*.
   **Fixed** — the row now says duplication replicates the blind spot and makes
   divergence a **latent risk**, all sites are marked test-scope, and S30's
   filing states explicitly that **no production consumer was found during
   scoping**, so a later one extends the row rather than contradicting it. The
   round 12 claim is retained **struck through** with its correction, on the
   precedent of P13-S16 §7's withdrawn finding.
2. **[P1] M20 was written as existing evidence.** It has not run, the annex does
   not exist, and S30's row is created *during execution* — so "M20 is the
   evidence", "the defect is demonstrated" and "the analysis lives in the
   ledger" all asserted a future state as current. **Fixed** — M20 *will
   furnish* the evidence, the **landed** row *must* cite its transcript, and §5
   says the analysis *will live* there. What the code read establishes now, and
   what execution will exhibit, are separated.

*Both findings are the same class the last several rounds have turned on: a
claim about a state that does not hold yet, or a scope inferred rather than
traced.*

---

### REVIEW ROUND 12 — independent. One blocking finding. ACCEPTED, with the owner's ruling.

1. **[P1] §5 scoped the checker defect too narrowly, and a §5 note is not
   ownership.** Round 11 framed it around spaced `\label`, but **M20 proves the
   worse branch**: a legal `\begin {requirement}` is silently missed by the
   block scanner with nothing else reacting — this contract's own mutation plan
   depends on that property. Exact parsing also reaches `\chapter`, and a second
   requirement scanner exists in `text_projection_grammar.rs`.

   **Ruling: file it as P13-S30**, through a pin, a touch-row consumer and a
   gate assertion. **Done** — pin 7 now files two rows, gate 10 asserts both,
   touch row 6 names both, and §5 defers to the ledger instead of carrying the
   analysis into a document that becomes historical at landing.

   **Verified all four consequences rather than transcribing them.**

   > ~~And one reaches further than the finding states: the duplicated-scanner
   > branch is not confined to tests. `crates/epiphany-textproj/src/parse.rs:726`
   > splits on an exact `\chapter{A Worked Example}` in **`src/`** — the
   > exact-parsing assumption is in production code, not only in the checkers.~~
   >
   > **WITHDRAWN by round 13, finding 1.** `#[cfg(test)] mod tests` opens at
   > `parse.rs:639`, so line 726 is **test scope**. The claim was reached by
   > treating a path under `src/` as production without checking reachability —
   > the repository's own standing rule is to enumerate the reachable paths, not
   > to infer them from where a file sits. Retained struck rather than deleted:
   > the correction is the point.

---

### REVIEW ROUND 11 — independent. Three findings, one blocking. ALL ACCEPTED.

1. **[P1] The new structural guard was incomplete in syntax and in mutation
   coverage.** It matched literal `\label{` and `\begin{requirement}`, which TeX
   spellings like `\label {x}` bypass — and **the repository's own parser has
   the same hole**: `command_arguments` builds an exact `\{command}{` needle
   (`requirement_labels.rs:117`). A guard shaped like the checker it
   complements inherits that checker's blind spot. Separately, M18 signed only
   the label assertion, leaving the independent requirement-block assertion
   unexercised. **Fixed** — recognition is whitespace-tolerant in test 1 *and*
   in gate 11's file-wide check; M18 becomes the **spaced** label form; and new
   **M20** adds a spaced `\begin {requirement}`, which the existing scanner
   misses entirely, making test 1 the clean discriminator with no other suite
   reacting.
2. **[P2] The round 10 audit tally was wrong** — "seven" against six
   enumerated. **Fixed by removing the numeral**, not by repairing it, per the
   rule this contract adopted in round 6 after the review-round count went
   stale twice.
3. **[P2] M18 did not exercise count-remeasurement blindness.** `labels()`
   keeps only `req:`-prefixed labels, so `tmp:m18` never reaches a count
   constant and M18 is identical before and after remeasurement. The safe
   namespace stays; the **claim** goes. **Fixed** — M18 is described as a
   surrogate that signs test 1's assertion, with the count-blindness it guards
   against explicitly marked as *argued, not exhibited*.

**Raised, not filed** — and narrowed after tracing it. Finding 1 exposes a gap
in `requirement_labels.rs` itself, but *"invisible to the entire checker"* was
my first, overstated reading. Traced: a spaced label outside `req:` is indeed
invisible to every test — which is what makes M18 a sound surrogate — while a
spaced label **inside** `req:` desynchronizes the defining and citing sides and
**fails loudly with a misleading diagnosis** rather than passing. Still a defect
worth filing, but a different one. §5 carries it.

---

### REVIEW ROUND 10 — independent. One blocking finding, three parts. ALL ACCEPTED.

1. **[P1] Gate 11's claimed complement still missed two pin 3 outcomes**, and
   its preamble overclaimed.
   - *Opening sentence.* Pin 3 requires it to survive; pin 6 mentioned it only
     as "the sentence the list follows", never pinning it as a locator.
     **Fixed by the stronger of the two offered remedies:** test 1's outer
     anchor is now **the complete opening sentence matched as an exact
     literal**, so the requirement and its observation are the same string. A
     prefix anchor would have left the remainder unguarded. M19 signs it.
   - *No new label, no requirement block.* Pin 3 forbids both and nothing
     enforced it — an accidental well-formed label is **absorbed the moment pin
     4's count constants are remeasured**, which pin 4 instructs execution to
     do. **Fixed with a machine assertion rather than a read check**, since test
     1 already holds the outer slice: it must contain no `\label{` and no
     `\begin{requirement}`. M18 signs it, run in the remeasured state — the
     condition under which the label suite goes blind.
   - *Preamble.* Narrowed from "the tests" to **tests 1 and 2**, since test 3
     deliberately observes requirement prose and is not part of the
     machine-observed complement.

**Sweep — the blindness is not confined to item 10.** The count-remeasure
argument holds for a label added *anywhere* in `core_spec.tex`, which test 1's
slice cannot see. Gate 11 gains a file-wide read-check: the staged diff adds
**exactly one** `\label{`, pin 4's.

**And one defect of my own, caught by running the suite rather than by
reading.** M18 was first written with a `req:graph:`-shaped label literal (the
string is deliberately not reproduced here — see below), which **immediately
broke the baseline to 1582/1**, undefined-citation,
`spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md`. That is round 4's finding
reproduced while writing round 10's fix, in a document whose pin 10 exists to
prevent it.

**Then the repair's own record reproduced it a second time**, because naming the
offending literal in this block put it straight back into the scan — the checker
cannot distinguish quoting from citing. Hence the shape, not the string: a
`req:`-form label. Nothing is lost, since the string was arbitrary and only its
form mattered. *(This is not the euphemism the allowlist's doc comment warns
against — that concerns a dangling label a document genuinely needs to name.)*

Repaired at the root — M18 now pins
`\label{tmp:m18}` and states *why* the non-`req:` form is mandatory — rather
than by adding a third allowlist row, which would have grown permanent
scaffolding to accommodate a throwaway mutation. **Every `req:*` literal in this
contract was then audited**, and each is accounted for: pin 10's two allowlisted
labels, the genuinely-defined requirements it cites
(`req:time:tempo-segment-order`, `req:time:aleatoric-anchoring-discipline`,
`req:tuning:accidental-modification-compatibility`), and
`req:layoutir:vertical-bands`, which the existing allowlist already covers.

---

### REVIEW ROUND 9 — independent. Two findings, one blocking. ALL ACCEPTED.

1. **[P1] Gate 11's inventory of unobserved prose was incomplete.** Three pin
   3/5 outcomes passed every test and gate if simply omitted: item 10's
   re-anchoring exception and its `Chapter~\ref{ch:semops}` reference (deletable
   with the nested inventory intact); pin 5's rider separation and its P13-S29
   attribution (test 2 reads only pin-shaped token rows); and **the G3a aside's
   removal** — §0.1 rules out a mutation restoring it, but *declining to mutate
   something is not observing it*, and the aside is not pin-shaped, so test 2
   would pass with it still in place. **Fixed** — all three added to gate 11,
   which now opens by stating that the tests observe the token inventories and
   nothing else, so every other prose outcome belongs there by default.
2. **[P2] Gate 12 did not observe pin 10's traceability clause.** The permanent
   row's reason must name this contract and the evidence annex; gate 12 checked
   existence, wording and marker absence, all of which a shortened reason
   satisfies while stranding the tuple. **Fixed** — both provenance paths are
   named in the read-check.

**Sweep for the same class — one further instance.** Pin 6 requires its tests to
carry the comment explaining why the slice is mandatory; nothing observed it.
Added to gate 11.

---

### REVIEW ROUND 8 — independent. Two findings, one blocking. ALL ACCEPTED.

1. **[P1] The frozen-line observer was misattributed and underspecified.** Pin
   11 credited gate 6, which reads only `invariants.rs` and never opens this
   file — leaving the invariant unobserved while appearing guarded. Worse, gate
   13 and touch row 7 forbade any "hunk" on the line, which **a correct
   execution would fail**: pin 11's STATUS edit sits a few lines above, so at
   default context the unchanged frozen line appears as ordinary diff context.
   **Fixed** — the observer is gate 13, and the check is a comparison:
   `git diff --cached -U0` must contain no added and no removed line matching
   the statement. Zero context is what makes it satisfiable, and the pin says so.
2. **[P2] Pin 10's deletion rationale confused provenance with current
   applicability.** *"Authorized as prerequisite scaffolding, NOT as dispatch"*
   is a claim about how the rows were **introduced**; landing cannot
   retroactively re-authorize the past. *"No other pin work is licensed"* stays
   true, and the abandonment instruction becomes **inapplicable** rather than
   false. The claim that "the rows become dispatched work" was also wrong on its
   own terms — pin 4 deletes the temporary row, so only one survives. **Fixed** —
   the rationale now separates **falsified** (two claims) from **obsolete but
   true** (the rest), and deletion is justified on both counts rather than by
   overstating the first.

---

### REVIEW ROUND 7 — independent. Two findings, both blocking. ALL ACCEPTED.

1. **[P1] Pin 10 described the deleted scaffold, not the live one.** It quoted
   `(pre-ratification, unstaged)` and reasoned from claims round 6 had already
   removed, with the stale terminology surviving in execution item 1 and touch
   row 5. **Fixed** — every live consumer now names the `(pre-execution)` banner,
   and the deletion rationale is re-derived from the claims the *current* banner
   actually makes. *Round 6 fixed the scaffold and left the pin describing it —
   the one-hop failure again, two rounds running.*
2. **[P1] Pin 11 assigned a ratification transition to landing.** The
   frozen-pins statement is written in the **ratification** commit — verified in
   both precedents, which carry it word for word — so by execution the editable
   line is already gone. Pin 11's second "edit" could produce no hunk, touch row
   7 overstated, and gate 13 would have observed a pre-existing state as though
   it were a landing action. **Fixed** — pin 11 now separates **two landing
   edits** from **one ratified-input invariant**, and gate 13 checks the
   invariant as an invariant: a diff hunk on that line is a finding, not a
   discharge.

**Sweep for both classes — no further instance.** Every remaining
`pre-ratification` mention is inside a review-round record, which pin 11 marks
as historical at landing; no live pin, gate or touch row carries the old
terminology. Every pin was re-read for actions that belong to ratification
rather than landing, and pin 11 was the only one.

---

### REVIEW ROUND 6 — independent. Five findings, four blocking. ALL ACCEPTED.

1. **[P1] The scaffold became false at ratification, not landing.** Its banner
   claimed "pre-ratification, unstaged" and called the contract "untracked" —
   claims falsified when both are committed *for* ratification, while pin 10
   deletes the banner only at *execution*. **Fixed with lifecycle-neutral
   wording** rather than a second transition pin: the banner now says nothing
   about its own staging or tracking state, and says why.
2. **[P1] Pin 11 required an impossible self-reference.** A commit cannot contain
   its own hash. **Fixed** — the STATUS line is pinned as the symbolic
   `LANDED by this commit.` with **no hash**, on P13-S16's precedent (`aee4ff9`
   carried pre-landing wording; a later amendment recorded the id).
3. **[P1] Pin 10 pinned the wording it went on to forbid.** Its illustrative
   repair block still showed "never exists" while its landed-form paragraph and
   gate 12 required "not defined in the restored tree". **Fixed** — the block
   carries the final wording, and item 2 now says the row is written once,
   correctly, with nothing to edit later. *This is the one-hop correction
   failure — the live scaffold and the prose were fixed, the illustration was
   not — which is the defect P13-S16 §7 exists to name.*
4. **[P1] Round 5 was missing from the record, and the tally could not survive
   this round.** **Fixed** — rounds 5 and 6 are recorded, and every count of
   rounds is replaced by *all review-round blocks*, which cannot go stale. The
   same rule the contract already applies to `GraphInvariant::all` and to pin 1's
   inventory, applied to itself.
5. **[P2] Touch row 7 described one of pin 11's three edits.** **Fixed** — all
   three named in the row.

---

### REVIEW ROUND 5 — independent. Three findings, one blocking. ALL ACCEPTED.

1. **[P1] The scaffolding had no transition to a truthful landed form.** Pin 4
   removed the temporary tuple but nothing removed the surrounding banner, so
   the staged file would still have called itself pre-ratification scaffolding
   while shipping the permanent row. **Fixed** — pin 10 requires deleting the
   entire banner, gate 12 asserts the marker string is absent, and the row's
   "never exists" reason was corrected in the live scaffold as well as pinned.
2. **[P2] Touch row 5 named only the count changes**, not pin 10's three
   obligations. **Fixed** in the row.
3. **[P2] Pin 4a still claimed three signing mutations** after M16 was added.
   **Fixed** — four (M11–M13, M16), with normative force listed among what test
   3 protects.

**Ruling adopted:** no ledger row for the active scaffold. It would spread the
pre-ratification exception into another tracked file without creating a stronger
observer; the code-local instruction plus the visible dirty diff suffices while
S26 is active, and if S26 is abandoned, scaffold removal and the ledger
disposition happen together.

**Sweep beyond the round — three uncited instances:** touch row 7 authorized an
edit no pin mandated (→ pin 11); touch row 4 omitted pin 4a's test 3; and pin 6
said "both tests" in three places, written when there were two.

---

### REVIEW ROUND 4 — independent. Six findings, four blocking. ALL ACCEPTED.

1. **[P1] The evidence annex would have poisoned the final citation gate.**
   M6's verbatim diagnostic contains `req:graph:aleatoric-reference-locality`;
   after restoration that label is undefined, and
   `every_requirement_citation_is_defined` scans the annex like any other
   repository text. Verified: `repository_text_files` recurses everything but
   `.git` and `target`, excluding only generated *extensions* — `.md` under
   `spec/` is scanned — and `DISCUSSED_NOT_CITED` is exactly the hatch, with no
   anti-rot assertion, so an entry is safe both while the label exists (during
   M6) and after. **Fixed** by pin 10, which also fixes the **ordering**: the
   entry must land *before* M6's transcript is written, or every later
   mutation's radius silently gains a spurious failure. Redacting the
   diagnostic was rejected — verbatim evidence is the point.
2. **[P1] Test 3 invalidated M5 and M6's exhaustive radii.** Both mutations
   remove test 3's pinned label anchor. **Fixed** — both rows now include it.
   *This is the cost of an exhaustive-radius rule, working as intended: adding a
   guard obliges re-deriving every radius that guard can reach.*
3. **[P1] M14 changed membership as well as order.** No inventory row has the
   target set `{live event, anchor target}`, so M14 failed at pair equality and
   signed nothing about ordering. **Fixed** — it now permutes a *complete*
   existing target (`AnalyticalAnnotation.anchor`), so normalization alone would
   recover the expected pair and only the order assertion can catch it.
4. **[P1] M7 did not discriminate the nested extractor from a plausible wrong
   one.** Ordinary prose is ignored by both the correct first-environment
   extractor and a faulty scan-every-`\item`-in-the-outer-slice one. **Fixed** —
   the row moves into a **second nested `itemize`** inside item 10, which the
   correct extractor misses and the faulty one still collects.
5. **[P2] Test 3's `\MUST{}` assertion was unsigned.** **Fixed** — M16 weakens
   it to `\SHOULD{}`.
6. **[P2] Oracle validation's vocabulary branch was unsigned.** M15 exercised
   only uniqueness. **Fixed** — M17 puts an out-of-vocabulary target in the
   oracle itself, requiring step 0's vocabulary diagnostic specifically.

---

### REVIEW ROUND 3 — independent. Seven findings, four blocking. ALL ACCEPTED.

1. **[P1] Pin 4's substantive requirement had no observer.** Label, count,
   grammar, area and citation were all watched; the *wording* was not, so "same
   region" → "any region" passed every gate. **Fixed** — pin 4a adds test 3,
   slicing the requirement by its label, with three content mutations
   (M11/M12/M13). Read-checking was available and rejected for the same reason
   as round 2 finding 3.
2. **[P1] Only the LaTeX side's target extraction was mutation-signed.** Every
   M4 row would still fail if test 2 compared tokens alone, so the Rust doc
   could have read `Slur.start_event — declared staff` unchallenged. **Fixed** —
   M4d and M4e are the Rust-side equivalents of M1c and M1d. The closed
   vocabulary is intended on both sides, so M4e is included rather than
   omitted.
3. **[P1] M7 was still not the mutation it claimed to be.** An `\item` moved out
   of the nested `itemize` joins the surrounding `enumerate` and *becomes the
   next top-level invariant* — leaving item 10 entirely, rather than staying in
   it. **Fixed** — M7 now moves the token-target **text** outside the inner
   environment as ordinary prose, with no `\item`, so it genuinely remains
   inside item 10 while vanishing only from the nested extraction.
4. **[P1] The required evidence had no artifact or destination.** Gates 6 and 7
   demanded verbatim transcripts that the staging allowlist gave nowhere to
   live. **Fixed** — `spec/EVIDENCE_P13S26_EXECUTION.md` is pinned as touch row
   8, up front, rather than discovered at acceptance as P13-S16's annex was.
5. **[P2] Eleven code anchors did not resolve to the checker's expressions.**
   Verified: `cc` is bound at `invariants.rs:1075`, *after* the slur, tie, beam,
   tuplet and spanner loops at `:999`–`:1050`, which use
   `self.score.cross_cutting.*`. Exactly eleven rows were wrong. **Fixed** — all
   eleven now name the actual expression.
6. **[P2] Alphabetical target order was declared normative but normalized
   away.** **Fixed** by keeping the order normative and *enforcing* it: pin 6
   asserts the raw target already equals its normalized form before comparing.
   M14 writes an unsorted target and must fail.
7. **[P2] Duplicate validation excluded the oracle itself.** A duplicate inside
   `INVARIANT_TEN_SURFACE` vanished on conversion to `BTreeSet`, leaving both
   documents and M9/M10 passing. **Fixed** — pin 6 step 0 validates the constant
   before it is used as an oracle: unique tokens **and** every target term drawn
   from pin 1a's vocabulary. M15 duplicates an inventory entry.

---

### REVIEW ROUND 2 — independent. Seven findings, five blocking. ALL ACCEPTED, and the sweep found two more.

1. **[P1] The inventory contained invented field paths and had lost the promised
   code-anchor column.** Verified against `graph.rs`, `event.rs` and `tempo.rs`:
   `DecompositionComponent` **does not exist**, and four more tokens named the
   wrong owner. **Fixed** — pin 1's tokens are now actual schema paths, each
   carrying a **Code anchor** column back to the emitting control flow.

   **Sweep for the same class, beyond the five cited — two more found:**
   - `Repeat.start` / `.end` / `.kind` / `.voltas` → the struct is
     **`RepeatStructure`** (`graph.rs@1261`–`@1267`). Four tokens.
   - `IndeterminateEvent.alternatives` → the check reads `ie.hints.alternatives`,
     so the owner is **`IndeterminacyHints`** (`event.rs@70`).

   Every remaining token was checked the same way and is correct as written.
2. **[P1] `BTreeSet` equality erased duplicates.** **Fixed** — both tests now
   extract to a `Vec`, assert the duplicate list is **empty** (naming the
   repeats, hard-coding no total), and only then compare sets. New mutations
   M9/M10 duplicate a token in each document.
3. **[P1] The guard observed class names but not what they resolve to.** Pin 1's
   target column was normative while pins 3 and 5 declared trailing prose free —
   so swapping `live event` for `declared staff` passed every gate. **Fixed by
   the stronger of the two offered options:** the comparison is now over
   **(token, target) pairs**, with targets drawn from a closed canonical
   vocabulary (pin 1a). Read-checking was rejected — a rung arguing that
   unguarded documentation drifts should not leave half its own repair unguarded.
4. **[P1] M7 could not produce its required failure.** Extending the *outer*
   slice past invariant 11 does not change the *nested* `itemize`'s contents, so
   a correct extractor returns the same set. **Fixed** — M7 is replaced by a
   mutation that targets the nested-environment boundary specifically: move one
   `\item` out of the `itemize` into item 10's prose, still inside the outer
   slice.
5. **[P1] The mutation table left failure radii for execution to discover.**
   **Fixed** — every "Must fail" cell is now exhaustive, and §3 states that any
   mismatch with the observed set is itself a finding. Both cited rows verified
   in `requirement_labels.rs` and adopted: the count constants are asserted at
   `:259`/`:265` (`every_requirement_block_has_one_label`), `:294`
   (`requirement_labels_follow_the_grammar`), `:364`
   (`requirement_labels_are_unique_across_the_suite`) and `:445`
   (`every_requirement_citation_is_defined`) — so M5 trips all four, and M6
   trips `requirement_label_areas_match_their_chapters` plus
   `every_requirement_citation_is_defined`, because this contract cites the
   original `req:time:` label and the test scans repository text.
6. **[P2] Pin 8 required a rename without pinning the name.** **Fixed** — the
   replacement is named exactly.
7. **[P2] M3 did not specify the weakening or which M1 it consumes.** **Fixed** —
   the predicate is pinned as `actual.is_subset(&expected)` (the direction that
   passes after a deletion; `expected.is_subset(&actual)` still fails), and M1 is
   now six individually named executions, with M3 consuming **M1-B**.

---

### REVIEW ROUND 1 — independent, against the untracked draft. Seven findings, five blocking. ALL ACCEPTED.

1. **[P1] The widened guards did not establish completeness.** Pin 8 required
   one needle per group A–F; a guard could pass while group A lost sub-beam
   members, volta anchors and repeat-kind anchors together. **Fixed** by
   replacing needle spot-checks with **exact set equality over a canonical token
   inventory** (pin 1, pin 6). Deletion *and* addition now fail, per token.
2. **[P1] Pin 6's uniqueness claim made M3 impossible.** Pin 6 asserted every
   needle occurred only in the item-10 slice; M3 required that an unsliced guard
   *pass* because listings contain the needles. Both could not hold. **Fixed**
   by dissolving the premise: there are no needles. With set equality, an
   unsliced extractor over-collects and fails loudly, so the slice is required
   for the guard to *function*, not to have teeth.
   *Measured while checking this: `instrument_override` occurs exactly once in
   `core_spec.tex` (the `StaffInstance` listing), so the original collision
   control could not have worked as written either.*
3. **[P1] Pin 9 had no observer.** **Fixed** — gate 6 is now a staged-diff
   boundary gate with explicit forbidden-token assertions.
4. **[P1] The mutation evidence gate was unsatisfiable.** Gate 6 demanded a
   failing assertion for every mutation while M3's required outcome is success.
   **Fixed** — §3 splits failing-evidence mutations from the single
   passing-evidence one, and requires a full workspace run with
   `--no-fail-fast` and the complete failure set for each failing mutation.
5. **[P1] The new guard was not pinned as an artifact.** **Fixed** — pin 6 names
   the path, both test names, both slice anchors, the normalization rule, and
   the complete inventory.
6. **[P2] The final gate dropped repository mechanics.** **Fixed** — clippy and
   fmt pinned to `+1.95.0`, the baseline sourced from `CLAUDE.md` rather than
   restated, `git diff --cached --check` added.
7. **[P2] Two status statements were false.** **Fixed** in §0.2 and §0.3.

**Both open questions closed by round 1, and adopted:**

- **No companion `.tex` restates item 10's surface.** `operation_catalog.tex`
  mentions individual referential preconditions but earns no touch row.
- **`core_spec.tex` carries no document-version literal** — already recorded at
  `spec/CONTRACT_P13S19_PARTIAL.md` §0 item 7, verified. The version question is
  **not applicable**; the Revision History edit stands.

---

## §0. What was verified before drafting

Everything in this section was measured against the working tree at `fcc3fb6`,
not reasoned from the ledger row. Two of the row's own claims did not survive.

### 0.1 The row's headline reading is FALSE, and the contract records it as false

The row states that `invariants.rs`' invariant-10 doc comment *"claims a
specification repair that never landed"*, quoting its aside:

> genesis tranche G3a repairs this prose to name what the check body already
> enforced

**That reading is not supported.** `spec/CONTRACT_GENESIS_G3A_ENTITIES.md`'s
pin 6 is titled, verbatim:

> **Pin 6 — the invariant-10 prose reconciliation (§6.3), and it is doc-only**

G3a uses "prose" to mean *the doc comment*, and scoped out any `.tex` change
explicitly. `git log -L 75,76:crates/epiphany-core/src/invariants.rs` attributes
the aside to `6c5e69f` — **G3a's own commit**. So "this prose" is
self-referential and the sentence is *true*: a comment announcing its own
repair. It is uselessly self-referential, and it misleads any reader who does
not have G3a pin 6 open — which is how the row came to be filed — but it is not
a false claim about another document.

**Consequences, and they are binding on the pins below:**

1. **No mutation may be built around restoring an alleged lie.** There is no lie
   to restore. A mutation asserting one would sign for a defect that does not
   exist.
2. **The row's "MUST NOT be corrected on its own" rationale does not hold as
   written.** It rests on the comment being the tree's only pointer to a
   falsehood. It is not.
3. **The two-sided repair survives on a different rationale**, stated in §0.2.
4. The ledger row is corrected by appending, never rewriting (house rule).

### 0.2 The replacement rationale: both summaries are incomplete mirrors

Neither the normative enumeration nor the doc comment is a faithful summary of
what the checker enforces, and **the two are incomplete in different places**.

- `core_spec.tex`' item 10 (the `\item` beginning *"Every cross-cutting
  structure's references resolve to extant objects"*, in Chapter 5's graph
  invariant `enumerate`) names **no** individual reference class.
- The Rust doc comment (the `/// 10.` block above `CrossCuttingRefsResolve`)
  names many, but not all. **Two strengths of omission, kept apart because they
  carry different weight:**

  **Definite** — enforced classes the comment's own structure excludes:

  1. `StaffInstance.instrument_override`. The comment's structural group is a
     closed list — *"a staff's declared instrument, a staff's group, a staff
     group's members, a part's staves, a view's active layers"* — with no
     hedge, and the override is not in it.
  2. `NotatedComponent.tuplet`. Pin 1 group **D** has no counterpart in the
     comment at all.
  3. Tempo segment anchor targets. Pin 1 group **F** likewise: the comment does
     not mention the tempo map.

  **Hedged** — covered only by a non-exhaustive parenthetical, *"cross-cutting
  structures (incl. anchor targets, annotation layers, tuplet parents, graphic
  objects)"*: `SubBeam.events`, and `RepeatStructure.kind` / `.voltas`. An
  `incl.` list is not wrong about these; it is simply not a summary anyone can
  check a specification against, which is what pin 3 needs it to be.

  The comment's meter group and event-internal group are, by contrast,
  **complete** — measured against pin 1 groups C and E respectively.

**Therefore: neither side may be repaired from the other.** Copying the doc
comment into the specification — the obvious repair, and the one the row's
framing invites — would promote an incomplete list to normative status. Pin 1
makes the derivation from the check bodies the sole origin for both.

This is the defect class P13-S16 §7 names: **enumerating, or copying an
enumeration that already exists, where completeness requires deriving.**

### 0.3 The tag multiplexes, and classification is per emitted condition

`GraphInvariant::CrossCuttingRefsResolve` is emitted from **five sites in four
functions**, not one (enumerated exhaustively, not by a truncated search):
`check_cross_cutting_refs`, `check_tempo_maps`, `check_aleatoric_models`, and
`check_accidental_modification_compatibility`.

The last of those says so itself, in its own comment: *"Not one of the
spec-enumerated Chapter 5 graph invariants … surfaced under an existing
`GraphInvariant` tag rather than minting a new one."*

**Directed classification (ruling 2) — per emitted condition, not per function.
Proposed by this DRAFT; ratified only when the contract is:**

| Emitted condition | Owner |
|---|---|
| Tempo segment `start` / `end` **anchor target existence** | **invariant 10** |
| Tempo segment shape ↔ `end_tempo` consistency | Chapter 3 (see 0.5) |
| Tempo segment start ordering; segment overlap | Chapter 3, `req:time:tempo-segment-order` |
| Aleatoric `ordering` referenced events in the **owning region** | Chapter 3 (new, pin 4) |
| Aleatoric `bounds` key events in the **owning region** | Chapter 3 (new, pin 4) |
| Accidental modification expressibility in a pitch space | Chapter 4, `req:tuning:accidental-modification-compatibility` |

Invariant 10 stays **reference resolution**. A Chapter 5 invariant is not
expanded to absorb Chapter 3/4 rules because the implementation multiplexes them
through one tag.

Note the aleatoric conditions are **stronger than resolution**: `in_region`
returns false both for an absent event and for one present in a *different*
region. "Extant object" cannot be stretched to cover that, which is why pin 4
states it in Chapter 3 rather than folding it into item 10.

The reversed-bounds condition in the same function already emits
`EventCoordinateModel` (invariant 4) and is **correctly tagged** — it is not part
of the multiplexing defect and this rung does not touch it.

### 0.4 The multiplexing is a separate live defect. It is recorded, not fixed

`check_invariant(score, CrossCuttingRefsResolve)` returns Chapter 3/4 failures,
and `impl Display for InvariantViolation` renders them as:

```
invariant 10 (CrossCuttingRefsResolve) violated: non-constant tempo segment is missing its end_tempo
```

A Chapter 3 tempo rule, attributed in user-visible text to a Chapter 5 graph
invariant, through a **public** filter API.

**This rung does not repair it, and does not rewrite the specification to
legitimize it.** Pin 7 **will file** it as its own candidate, **P13-S29** (id
verified free). Until execution writes that row it is scoped here and filed
nowhere — the ledger, not this contract, is what "filed" means. Repairing it means either minting tags or re-tagging emissions;
both are behaviour changes, and this rung is documentation-and-guard.

### 0.5 Two normative homes already exist; one does not

Measured, so the pins neither duplicate nor invent:

- **`req:time:tempo-segment-order` already covers ordering *and* non-overlap.**
  Pin 3 cross-references it; it needs no amendment.
- **Tempo shape ↔ `end_tempo` consistency is stated only in a listing doc
  comment**, not in a labelled requirement. That is a P13-S1-class gap, and it
  is also exactly P13-S8's site (the `is_none_or` spelling). **Out of scope
  here** — noted so a later rung does not read this contract's silence as
  coverage.
- **The aleatoric owning-region rule has no normative statement at all.**
  `req:time:aleatoric-anchoring-discipline` governs coordinate *kinds*, a
  different subject. Pin 4 adds a new labelled requirement.

### 0.6 Locators drift; this contract uses symbolic anchors only

The row cites `core_spec.tex:6570`–`:6572` for item 10 and `invariants.rs:59`–
`:62` (via G3a pin 6) for the doc block. Both have moved — P13-S16 inserted
above each. **No pin, test, or mutation in this contract uses a line number as
an *operative locator* in a file it also changes** — nothing is found, sliced or
asserted by coordinate. Line numbers do appear inside **frozen verbatim
diagnostics**, notably pin 10's `requirement_labels.rs:486:5` panic transcript;
those are quoted evidence, consumed by nothing, and reproducing them exactly is
the point. The claim is about what drifts, and a coordinate nobody follows
cannot. Pin 1's Code-anchor column names functions, loops and
match arms, never lines. The `graph.rs@NNNN` / `event.rs@NN` references in the
round 2 block are *provenance for a one-time verification* against files this
rung does not touch, and are not consumed by any pin.

### 0.7 One near-miss, recorded as method

`instrument_override` was first measured as **absent** from `core_spec.tex` by a
grep for the LaTeX-escaped `instrument\_override`. It is present, in a
`lstlisting` where the underscore is bare. The false absence was caught before
it reached a pin. **Any `.tex` search in this rung must be run in both
spellings** — escaped for prose, bare for listings — and pin 6's normalization
rule exists for the same reason.

---

## §1. Pins

### Pin 1 — the derived reference surface is the sole origin

The contract carries **one** table of invariant 10's normative reference
surface, derived by reading every emitted condition in the four functions of
§0.3 and keeping those classified to invariant 10. Both repaired documents are
written **from this table**; neither is written from the other.

**Token** is the actual schema path of the checked field — verified against
`graph.rs`, `event.rs` and `tempo.rs`, never paraphrased. **Target** is drawn
from pin 1a's closed vocabulary. **Code anchor** preserves the derivation back
to control flow, symbolically. One row is one (token, target) pair.

**A — cross-cutting structures** (`check_cross_cutting_refs`)

| Token | Target | Code anchor |
|---|---|---|
| `Slur.start_event` | live event | `self.score.cross_cutting.slurs` loop |
| `Slur.end_event` | live event | `self.score.cross_cutting.slurs` loop |
| `Tie.start_event` | live event | `self.score.cross_cutting.ties` loop |
| `Tie.end_event` | live event | `self.score.cross_cutting.ties` loop |
| `Beam.events` | live event | `self.score.cross_cutting.beams` loop, member arm |
| `SubBeam.events` | live event | `self.score.cross_cutting.beams` loop, `sub_beams` arm |
| `Tuplet.members` | live event | `self.score.cross_cutting.tuplets` loop |
| `Tuplet.parent` | extant tuplet | `self.score.cross_cutting.tuplets` loop, `parent` arm |
| `Spanner.staves` | declared staff | `self.score.cross_cutting.spanners` loop |
| `Spanner.start` | anchor target | `self.score.cross_cutting.spanners` loop |
| `Spanner.end` | anchor target | `self.score.cross_cutting.spanners` loop |
| `Marker.anchor` | anchor target | `cc.markers` loop |
| `RepeatStructure.start` | anchor target | `cc.repeats` loop |
| `RepeatStructure.end` | anchor target | `cc.repeats` loop |
| `RepeatStructure.kind` | anchor target | `cc.repeats` loop, `kind_ok` match |
| `RepeatStructure.voltas` | anchor target | `cc.repeats` loop, `voltas` arm |
| `ChordSymbol.anchor` | anchor target | `cc.chord_symbols` loop |
| `AnalyticalAnnotation.anchor` | anchor target, extant region, live event | `cc.analytical` loop, via `annotation_anchor_ok` |
| `AnalyticalAnnotation.layer` | declared analysis layer | `cc.analytical` loop, `layer` arm |
| `Comment.anchor` | anchor target, extant region, live event | `cc.comments` loop, via `annotation_anchor_ok` |
| `GraphicGesture.objects` | stored graphic object | `cc.graphic_gestures` loop |
| `GraphicGesture.anchoring` | anchor target, declared staff, live event | `cc.graphic_gestures` loop, `anchoring` match |
| `LyricLine.events` | live event | `cc.lyrics` loop |

**B — structural top-level references**

| Token | Target | Code anchor |
|---|---|---|
| `Staff.instrument` | declared instrument | `score.staves` loop |
| `StaffInstance.instrument_override` | declared instrument | `staff_instances()` loop |
| `Staff.group` | declared staff group | `score.staves` loop, `group` arm |
| `StaffGroup.members` | declared staff | `score.staff_groups` loop |
| `PartDefinition.staves` | declared staff | `score.parts` loop |
| `ViewDefinition.active_layers` | declared analysis layer | `score.views` loop |

**C — meter / time-signature references, at every level a `MeterChange` appears**

| Token | Target | Code anchor |
|---|---|---|
| `MetricTimeModel.meters` | declared time signature | region loop, `RegionTimeModel::Metric` arm |
| `StaffBasedContent.default_metric_grid` | declared time signature | region loop, `staff_based()` arm |
| `Measure.time_signature` | declared time signature | region loop, `si.measures` arm |
| `StaffInstance.local_metric_grid` | declared time signature | region loop, `si.local_metric_grid` arm |

**D — attachment-internal references**

| Token | Target | Code anchor |
|---|---|---|
| `NotatedComponent.tuplet` | extant tuplet | `score.decomposition_attachments` loop |

**E — event-internal references**

| Token | Target | Code anchor |
|---|---|---|
| `IndeterminacyHints.alternatives` | live event | events loop, `Event::Indeterminate` arm |
| `TrajectoryEvent.start` | live pitch | events loop, `Event::Trajectory` arm |
| `TrajectoryEvent.end` | live pitch | events loop, `Event::Trajectory` arm |
| `GraphicEvent.graphics` | stored graphic object | events loop, `Event::Graphic` arm |
| `CueEvent.source` | live event | events loop, `Event::Cue` arm |

**F — tempo map, invariant-10 conditions only** (`check_tempo_maps`)

| Token | Target | Code anchor |
|---|---|---|
| `TempoSegment.start` | anchor target | `tm.segments` loop |
| `TempoSegment.end` | anchor target | `tm.segments` loop |

**No total is stated anywhere in this contract or in either repaired document.**
A count restated beside the structure it counts goes stale silently — the rule
this module's own header already states for `GraphInvariant::all`. Pin 6
compares *pairs*, never lengths.

### Pin 1a — the target vocabulary is closed

A target is a **comma-separated, alphabetically sorted** list of terms from
exactly this vocabulary:

`anchor target`, `declared analysis layer`, `declared instrument`,
`declared staff`, `declared staff group`, `declared time signature`,
`extant region`, `extant tuplet`, `live event`, `live pitch`,
`stored graphic object`

A conditional target (one whose resolution depends on the value's form) lists
every term it can require, sorted — which is why `AnalyticalAnnotation.anchor`
reads *"anchor target, extant region, live event"*.

**The sort order is normative AND enforced.** Pin 6 asserts each raw target
already equals its own normalized form *before* comparing pairs, so an unsorted
document target fails rather than being silently repaired. (Normalizing without
that assertion would have made the ordering rule unobservable — a rule stated in
prose and erased by the comparison.)

**Vocabulary membership is a *separate* assertion, not a consequence of that
one.** An out-of-vocabulary term sorts perfectly well, so the order check cannot
see it; pin 6 step 4 therefore splits into (a) order and (b) vocabulary, signed
by M14 and by M1d/M4e respectively. *An earlier draft of this pin said a bad
term "fails on the same assertion" — the very error round 3 caught in pin 6,
left uncorrected here.*

### Pin 2 — invariant 10 stays reference resolution

Item 10 describes **reference resolution and nothing else**. The three rider
classes of §0.3 are named in neither repaired document as invariant-10 content.
The specification is not rewritten around the implementation's tag reuse.

### Pin 3 — `core_spec.tex` item 10 is repaired from pin 1

Chapter 5's graph-invariant `\item` for invariant 10 keeps its opening sentence
and its re-anchoring exception clause with the `Chapter~\ref{ch:semops}`
cross-reference, and gains a nested `itemize` carrying **every pin-1 row**, one
per `\item`, in exactly this form:

```latex
\item \texttt{Slur.start\_event} --- live event.
```

The **token** is the first `\texttt{}` argument of the `\item`; the **target**
is the text between `---` and the terminating period. Nothing else on the line
is free: pin 1a's vocabulary is closed and pin 6 compares the pair. Group
headings A–F may be rendered as prose lead-ins **outside** the nested `itemize`
and carry no tokens.

The item cross-references `req:time:tempo-segment-order` for the tempo
conditions that are *not* invariant 10's, so a reader is not left inferring that
ordering is unowned.

**That cross-reference has no machine observer**, and is named here as such
rather than left to look guarded: pin 6 compares pairs, and a dropped `\ref` is
prose loss the pair set cannot see. It is **gate item 11**, checked by reading.
(Precedent: P13-S16's pin 4b recorded its `.tex` halves as gate items for the
same reason.) A `\ref` to a *deleted* label would still be caught —
`requirement_labels.rs` enforces cited→defined — so the unguarded failure mode
is narrow: silent deletion of the sentence.

**No new label, no requirement block.** Item 10 is an `\item` in an enumeration,
not a `requirement`; this pin therefore moves no requirement count.

**`core_spec.tex`'s Revision History gains a row** for this rung, naming pin 3's
item-10 repair and pin 4's new requirement. Touch row 1 authorizes it and **this
pin is what requires it** — a table row authorizing an edit no pin mandates is a
change nobody signed for. It has no machine observer and is **gate item 11**.

### Pin 4 — Chapter 3 gains the aleatoric reference-locality rule

A new labelled requirement in Chapter 3's Aleatoric Time subsection —
`req:time:aleatoric-reference-locality` — stating that an aleatoric region's
`ordering` referenced events and `bounds` key events **MUST** be events of that
same region.

This is the "state it explicitly rather than stretch *extant object*" half of
ruling 2. It is a **normative addition**, so:

- `requirement_labels.rs`' three counts move. They are **measured at execution,
  never predicted** — the contract carries no target number.
- The `CHAPTER_AREAS` assignment must accept it under `time`. Verified: the
  Aleatoric Time subsection sits inside `\chapter{Time and Duration}`, which
  `CHAPTER_AREAS` maps to area `time`, and the label is `req:time:…`.

### Pin 4a — the requirement's *wording* is observed, not just its label

`requirement_labels.rs` watches labels, counts, grammar, chapter area and
citations. **None of that sees the sentence.** Replacing "same region" with "any
region", or dropping either referent, would satisfy every other gate in this
contract.

**Test 3 — `aleatoric_reference_locality_states_both_referents_and_locality`**,
in pin 6's file, sharing its root helper.

- *Slice:* `spec/core_spec.tex`, the `requirement` environment containing
  `\label{req:time:aleatoric-reference-locality}` — from its `\begin{requirement}`
  to the matching `\end{requirement}`.
- *Assert:* the slice names **both** referents (`ordering`, `bounds`), the
  locality phrase, and the normative keyword `\MUST{}`.

This is phrase presence, not exact comparison — **weaker than tests 1 and 2, and
stated as such** rather than presented as equivalent coverage. **Four mutations
sign it: M11 (locality), M12 and M13 (the two referents), and M16 (normative
force).** What it buys is that neither referent, nor the locality claim, nor the
requirement's normative force can silently leave — the last of those being a
weakening that reads as an editorial softening rather than a deletion.

### Pin 5 — the doc comment is repaired from pin 1, and separated

The `/// 10.` block is rewritten from pin 1 and carries **every pin-1 row**, one
per line, in exactly this form:

```rust
///   - Slur.start_event — live event.
```

The **token** is the first whitespace-delimited word after `- `; the **target**
is the text between `—` and the terminating period.

The block then gains an explicitly-marked separation: the normative invariant-10
surface above, and below it a note that **additional checks are currently
reported under this tag** (§0.3's riders), naming P13-S29 as their owner. The
rider note carries **no** pin-5-shaped lines, so it cannot pollute extraction —
and if it ever did, test 2 would fail on a spurious token, which makes that
constraint self-observing.

**The G3a aside is removed.** Ruling 1(a): it is ambiguous, not false; its
historical content is preserved in §0.1 and in the ledger append, which is where
a historical aside belongs. Removing it is not a correction of a lie and must
not be described as one.

### Pin 6 — the guard, pinned as an artifact

**Path:** `crates/epiphany-testkit/tests/invariant_ten_surface.rs` — testkit,
because it is the crate that already reads both the `.tex` suite and repository
sources, via the `Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")` root
helper that `requirement_labels.rs` uses.

**The inventory constant.** One
`const INVARIANT_TEN_SURFACE: &[(&str, &str)]` holding pin 1's (Token, Target)
pairs verbatim. This is the ratified origin in machine-readable form, not a
second list: the derivation source is Rust control flow, which is not parseable,
so gate 8 re-derives it by hand. Tests 1 and 2 compare against it; test 3
does not use it.

**Tests 1 and 2 share this shape**, and the order matters:

0. **Validate the oracle, before using it as one.** Assert
   `INVARIANT_TEN_SURFACE` has no repeated token, and that every term in every
   target is drawn from pin 1a's vocabulary. A duplicate inside the constant
   would otherwise vanish when the expected side becomes a `BTreeSet`, leaving
   both documents and M9/M10 passing against a silently-collapsed oracle. This
   step is shared by tests 1 and 2, so neither can run against an unvalidated
   inventory.
1. Slice.
2. Extract to a **`Vec<(String, String)>`**, preserving order and repeats.
3. **Duplicate check:** assert the list of tokens appearing more than once is
   **empty**, naming them. This is what set comparison cannot see, and it
   hard-codes no total.
4. **Canonical-form check, in two independent parts** — they catch different
   mutations and neither implies the other:
   - **(a) Order.** Assert each raw target already equals its own normalized
     form (split on `,`, trim, sort, rejoin with `, `). Pin 1a's order is
     normative, so this is where it is enforced; normalizing without asserting
     would erase the rule. *Signed by M14.*
   - **(b) Vocabulary.** Assert every term is in pin 1a's closed vocabulary.
     **An out-of-vocabulary term sorts perfectly well**, so part (a) cannot
     catch it — a single combined check would have left M1d and M4e unable to
     produce their required failures. *Signed by M1d and M4e.*
5. `assert_eq!` the resulting `BTreeSet<(String, String)>` against
   `INVARIANT_TEN_SURFACE`. **Set equality, never `contains`, never
   `is_subset`** — the `left`/`right` diagnostic *is* the observation, naming
   missing pairs, spurious pairs, and any pair whose target drifted.

**Tests 1 and 2 both extract and compare targets, not tokens alone.** M1c/M1d sign this
for the specification; M4d/M4e sign it for the doc comment. A token-only
comparison on either side would let that document's targets drift freely.

**Test 1 — `specification_item_ten_names_exactly_the_derived_surface`**

- *Outer anchor — and this is what makes pin 3's opening sentence machine-
  observed:* the outer item-10 slice begins at the **complete opening sentence,
  matched as an exact literal**, not at a short prefix of it. Pin 3 requires that
  sentence to survive verbatim; using the whole of it as the anchor makes the
  requirement and its observation the same string, so any deletion or edit fails
  the slice rather than passing unnoticed. A prefix anchor would have left the
  remainder unguarded.
- *Slice:* `spec/core_spec.tex`, item 10's **nested `itemize` environment** —
  from the `\begin{itemize}` that follows that sentence to its matching
  `\end{itemize}`. The outer slice bounds the search; extraction reads only
  inside the environment.
- *Extract:* per `\item`, the first `\texttt{}` argument as token, the text
  between `---` and the period as target.
- *Normalize:* `\_` → `_`; trim; discard empties.
- *Also assert, on the **outer** slice, as **two independent assertions**:* it
  contains **no `\label`** and **no `\begin{requirement}`**. Pin 3 forbids both,
  and nothing else would catch them — an accidental well-formed label is
  absorbed the moment pin 4's count constants are remeasured, which pin 4
  instructs execution to do.

  **Recognition is whitespace-tolerant** (`\label` followed by optional
  whitespace then `{`, and likewise `\begin`), because TeX accepts `\label {x}`
  and `\begin {requirement}` and **the repository's own parser does not**:
  `command_arguments` builds an exact `\{command}{` needle. A guard written to
  the same exact-string shape would inherit that blind spot from the checker it
  is meant to complement. Signed by M18 (label) and M20 (requirement block),
  each spelled in the spaced form for that reason.

**Test 2 — `implementation_doc_names_exactly_the_derived_surface`**

- *Slice:* `crates/epiphany-core/src/invariants.rs`, from `/// 10. ` to the
  `CrossCuttingRefsResolve,` variant line — G3a's `t12` anchors, already proven
  to bound this block.
- *Extract:* per line matching pin 5's form, the token after `- ` and the target
  between `—` and the period.

**Why the slice is mandatory, stated in the tests' own comments:** unsliced, the
extractor collects every `\texttt{}` in `core_spec.tex` and every `- ` line in
`invariants.rs`, and equality fails on a flood of spurious pairs. The slice is
required for the guard to *function*. Its exactness is what M7/M8 control for,
and the strength of the comparison is what M3 controls for.

### Pin 7 — two defects are filed by execution. Neither is fixed

`spec/PASS13_CANDIDATES.md` gains **two rows**. Filing is this rung's whole
obligation for both; repairing either is out of scope.

**P13-S29** — the public `check_invariant` filter and
`Display for InvariantViolation` attribute Chapter 3/4 failures to invariant 10.
Its row **will cite** §0.4's rendered example as its evidence, and **must**
record that repairing it is a behaviour change.

**P13-S30** — **the repository's ad hoc TeX parsers assume a spelling TeX does
not require.** `\label {x}`, `\begin {requirement}` and `\chapter {X}` are all
legal and all missed. Its row **must carry** the four consequences below, each
traced rather than inferred:

| Consequence | Site | Severity |
|---|---|---|
| **Requirement blocks are mis-scanned — and both delimiters are matched exactly, so the failure differs by which one is spaced.**<br><br>**(i) Spaced opening.** The block is never pushed into `requirements`; no per-block check sees it. **Silent when additive and carrying no `req:` label.** The counts are of different things, which is the subtlety: `CORE_REQUIREMENT_COUNT`/`SUITE_REQUIREMENT_COUNT` count **blocks**, `SUITE_LABEL_COUNT` counts **labels from a whole-text scan**. Re-spacing an *existing* block is loud on the two **block** counts while its label stays visible; adding a hidden block that carries a `req:` label is loud on the **label** counts while the block counts hold. A non-`req:` label is invisible either way.<br><br>**(ii) Exact opening, spaced closing.** The scanner finds the opener, then hunts an exact close. **Two outcomes:** *no later exact close in the document* → both parsers **panic** (`"unterminated requirement in …"`; `"every requirement block is closed"`); *a later exact close exists* → the scan **consumes through it**, swallowing the intervening text — including any following block's opener, which is then never recorded separately. Usually loud, because the consuming block inherits ≠1 label. **But there is a second silent case:** an **additive, label-free** block with exact opener and spaced closer, inserted immediately before an existing single-label block **in the same chapter**, inherits exactly that one label, replaces it one-for-one in the block tally, leaves the whole-text label scan untouched, and keeps the same chapter attribution. Every count holds | **Openers:** `load_spec`'s `let begin = r"\begin{requirement}"` (`requirement_labels.rs:166`); `split_once("\\begin{requirement}")` (`text_projection_grammar.rs:647`). **Closers, matched just as exactly:** `let end = r"\end{requirement}"` (`requirement_labels.rs:167`); `split_once("\\end{requirement}").expect(…)` (`text_projection_grammar.rs:649`). Label collection: `all_defined_labels` → `labels()` on `document.text` | **two conditional silent cases; otherwise loud, including two distinct panics** |
| A `req:` label yields a false **"cited but undefined"** diagnosis — missed on the defining side, still found on the citing side | `labels()` via `command_arguments` vs `requirement_strings` | loud, misleading |
| **Chapter association missed or shifted** — `load_spec` binds the requirement to the previous *recognized* chapter. **Four outcomes, by what that predecessor is:** (a) *no* prior recognized chapter → `load_spec` panics, "requirement before first chapter"; (b) predecessor absent from `CHAPTER_AREAS` → the area test panics, "missing chapter-area data"; (c) predecessor mapped to a **different** area → loud mismatch listing the requirement; (d) predecessor mapped to the **same** area → **silent**. Case (d) is reachable: `Graph Value Layouts` (`binary_format.tex:814`) immediately precedes `Operation Wire Forms` (`:1196`) and both map to `binfmt`, so losing the latter's heading leaves the suite green. Of that file's **twelve** chapter headings, **seven** are represented in `CHAPTER_AREAS` | `command_arguments(&text, "chapter")`; `CHAPTER_AREAS`; the area test's `unwrap_or_else` panic | **(a)–(c) loud, (d) silent** |
| **The blind spot is replicated across independently written scanners**, so a fix in one leaves the others unrepaired and the parsers may drift apart. **Divergence is a latent risk, not an observed fact** — the read proves duplication only | **The full known inventory, from an exhaustive grep of exact TeX-form literals across `crates/`, all test-scoped:** `text_projection_grammar.rs` — `\chapter{…}` splits (`:73`, `:363`), exact `\label{…}` guards (`:341`, `:508`, `:538`, `:591`), `\begin{requirement}` scans (`:596`, `:647`) **and the matching exact `\end{requirement}` at `:649`**; `binary_format_history.rs:107`–`:112` — `\chapter{Revision History}` and a `\chapter{` delimiter, whose `.expect` **panics** on a spaced heading; `epiphany-textproj/src/parse.rs:726` — inside `#[cfg(test)] mod tests` | latent |

**Every site above is test-scope.** No production consumer of the exact-string
assumption was found during scoping; if one is later found, S30's row gains it.
Saying so is part of the filing — an unqualified "no production impact" would be
a claim this scoping did not make.

**Two of the four consequences have silent cases, and the requirement-block one
has two of them** — spaced-opener (additive, free of `req:` labels) and
exact-opener-with-spaced-closer (additive, label-free, immediately before a
single-label block in the same chapter) — plus the chapter branch in case
(d). That
is what makes the candidate worth filing rather than folding into an existing
misleading-diagnosis row. **Neither is silent unconditionally**, and the row must
carry the conditions: a consequence recorded as "silent" without them invites a
later reader to reproduce it the loud way and conclude the candidate is wrong.
A defect that fails loudly is at worst misdiagnosed; one that passes is not seen
at all.

**M20 will furnish evidence for consequence (i) only.** Its probe spaces **both**
delimiters, so no parser reaches the opener and none ever hunts a close — it
exhibits the **spaced-opening** blind spot and says nothing about the
exact-open/spaced-close behaviour of consequence (ii). S30's row must not cite
it as evidence for the whole consequence; (ii) is established here by code read,
and a rung that wants it exhibited owes its own probe.

**M20's evidence, for the row it does cover**, concerns the serious case: a
spaced `\begin {requirement}` is missed by the block scanner with **nothing else
reacting**. That property is established here by code read, and this rung relies
on it to make M20 a clean discriminator — so once M20 runs, the defect is
**exhibited** by the contract's own mutation plan rather than only argued.
**The landed S30 row must cite M20's transcript**; until execution, neither the
transcript nor the row exists.

**P13-S22 and P13-S25 do not own this**, and neither does a §5 note: a section
of a contract that becomes a historical record at landing is not durable
ownership. The ledger row will be.

### Pin 8 — `t12` is narrowed to a local non-vacuity check, and renamed exactly

G3a's `t12_invariant_10_doc_comment_names_the_four_reference_classes` asserted
four needles, all inside the structural class — which is how §0.2's omissions
went unseen. Pin 6 test 2 strictly supersedes it.

`t12` is **not deleted**: `cargo test -p epiphany-core` must still fail when the
doc block is destroyed, and testkit is a different crate. It is narrowed to
assert only that the block slices cleanly and yields a **non-empty** token list
in pin 5's form.

**The replacement name is pinned exactly:**
`t12_invariant_10_doc_block_slices_and_is_non_empty`.
M4c and the evidence both reference it by that name.

**Renaming is permitted here and pinned explicitly**, because this contract owns
the name. (P13-S16's execution renamed a *contract-pinned* test name and had to
revert; the distinction is that the pin is doing the renaming, not the keyboard.)

### Pin 10 — the evidence annex must not poison the citation gate

`every_requirement_citation_is_defined` scans **every** repository text file
outside `.git` and `target`, excluding only generated extensions. It therefore
scans `spec/EVIDENCE_P13S26_EXECUTION.md`.

M6 relabels pin 4's requirement to `req:graph:aleatoric-reference-locality`. Its
diagnostic, recorded verbatim per gate 7, puts that string in the annex — and
**after restoration the label does not exist**, so the citation test fails on
the evidence itself, permanently.

**The same defect already has a live instance, found sweeping this finding:**
`repository_text_files` reads the **filesystem, not git**, so the untracked
draft of this contract is scanned like any other file. It names both
`req:time:aleatoric-reference-locality` (pin 4) and — since round 4 —
`req:graph:aleatoric-reference-locality` (this pin). **Neither exists.** The
citation gate has therefore been failing since the draft was created, through
three review rounds in which both parties reported a clean worktree. Measured:

```
thread 'every_requirement_citation_is_defined' panicked at
crates/epiphany-testkit/tests/requirement_labels.rs:486:5:
req:graph:aleatoric-reference-locality: spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md
req:time:aleatoric-reference-locality: spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md
```

**Obfuscating the labels is not available**, and the checker says so itself:
*"Without this escape the check forces prose to become vaguer than the finding
it records: it already rewrote a scoping plan's `req:layoutir:vertical-bands`
into a euphemism to make itself pass."* Naming a label that does not exist is a
legitimate thing for a scoping document to do; the allowlist is the sanctioned
mechanism.

**The repair, in two rows with different lifetimes:**

```
("req:graph:aleatoric-reference-locality",
 "not defined in the restored tree; P13-S26's M6 mutation defines it only while
  that mutation is applied. Named by
  spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md and recorded in
  spec/EVIDENCE_P13S26_EXECUTION.md as M6's verbatim diagnostic.")  // PERMANENT

("req:time:aleatoric-reference-locality",
 "proposed by spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md; the requirement does
  not exist until pin 4 lands.")        // TEMPORARY — pin 4 REMOVES this row
```

**Pin 4 must delete the second row when it creates the requirement.** Once the
label is defined, the row's own claim — *"discussed, never cited"* — is false,
and this rung does not leave a false claim in a test file to fix a different
one. The row is inert rather than harmful at that point, which is exactly why it
would be easy to forget; gate 12 checks it.

**The scaffolding must also transition to a truthful landed form, and that is
this pin's obligation, not the keyboard's.** The rows sit under a banner marked
`P13-S26 REVIEW SCAFFOLDING (pre-execution)`. It deliberately claims nothing
about its own staging or tracking state — those change at ratification, and a
banner deleted only at execution would spend that interval false. Of what
remains, **two claims are falsified by landing and the rest merely go
obsolete** — a distinction worth keeping, because only the first kind would be a
lie in the tree:

**Falsified by landing:**

- `(pre-execution)` itself;
- *"neither of which exists yet"* — pin 4 creates `req:time:…`.

**Obsolete, but still true:** *"Authorized as prerequisite review scaffolding,
NOT as dispatch of P13-S26"* is a claim about how the rows were **introduced**,
and landing cannot retroactively re-authorize the past; *"no other pin work is
licensed by it"* remains true; and the abandonment instruction becomes
**inapplicable** rather than false. Note also that only the **permanent** row
survives — pin 4 deletes the temporary one — so it is not the case that "the
rows become dispatched work".

Deletion is right on either count: two falsified sentences and a set of spent
instructions do not belong in a landed tree, and shipping the surviving row
beneath them would leave the staged tree describing a state it is not in — the
§7 defect P13-S16 closed on.

Execution therefore:

1. **Deletes the entire `(pre-execution)` banner**, not merely the temporary
   tuple beneath it.
2. **Retains the `req:graph:` row unchanged.** Its wording above is already the
   landed form — *not defined in the restored tree* — chosen because the
   shorter "never exists" is falsified by M6 for as long as that mutation is
   applied. Nothing to edit here; the row is written once, correctly, and
   survives the banner's deletion.
3. Leaves the row's reason naming this contract and the annex, so the tuple
   remains traceable without the banner.

Gate 12 asserts the scaffolding marker string is **absent** from the staged
file.

Touch row 5 already covers `requirement_labels.rs` for pin 4's counts; this pin
extends *why* it is listed. Verified safe in both directions: the allowlist only
removes entries from the undefined set, and nothing asserts its entries *are*
undefined, so a row is inert while its label exists and effective while it
does not.

**Ordering is part of this pin, not an execution detail.** The entry lands
**before** M6's transcript is written. Otherwise every full-workspace run after
that write gains a spurious `every_requirement_citation_is_defined` failure, and
the exhaustive radii of every later mutation are wrong — turning §3's
mismatch-is-a-finding rule into a generator of false findings.

**Redaction was rejected.** Paraphrasing the diagnostic would satisfy the
checker and forfeit the verbatim evidence that gate 7 exists to produce.

### Pin 9 — no tag change, no behaviour change, and gate 6 observes it

`GraphInvariant` gains no variant. `all()` does not change length. No emission
site changes its tag. No check body changes. `number()` is untouched.

**Gate 6 is this pin's observer.** Re-deriving the surface cannot detect a
retag; a staged-diff boundary check can.

### Pin 11 — this contract's own landed form

Touch row 7 authorizes an edit to this file's status block; **this pin is what
requires it**, on the same rule as pin 3's Revision History row — a table row
licensing a change no pin mandates is a change nobody signed for.

**Two of these belong to landing; one belongs to ratification and is only
verified at landing.** Conflating them would have made pin 11 demand a hunk that
cannot exist.

**Landing edits — two:**

1. The **STATUS** block reads exactly **`STATUS: LANDED by this commit.`**
   **It must not contain a hash.** A commit cannot carry its own id — inserting
   it changes the tree and therefore the id. P13-S16 shows the shape: `aee4ff9`
   itself still read as pre-landing wording, and both hashes in its status line
   were written by later commits. If a hash is wanted here, it arrives the same
   way, as its own administrative amendment; it is **not** a gate item of this
   rung.
2. **All review-round blocks above §0** are marked as a dated historical
   record, not current state.

**Ratified-input invariant — not a landing edit:**

3. The header line *"Pins are editable until ratification"* is replaced by the
   frozen-pins statement **in the ratification commit**, which is where both
   precedents do it — P13-S27 (`RATIFIED … DISPATCHED`) and P13-S16 already
   carry it, word for word:

   > **THE PINS ARE FROZEN. They may be executed, not edited.** A defect found
   > during execution is **reported, not patched in place** — if it needs a pin
   > change, that is its own amendment with its own review round.

   By the time execution begins, that line **already exists**. Pin 11 therefore
   requires it as an **unchanged input**, verified present and untouched at
   landing.

   **Its observer is gate 13, not gate 6.** Gate 6 reads only
   `crates/epiphany-core/src/invariants.rs` and never opens this file; naming it
   here would have left the invariant unobserved while appearing guarded.

   **And the check is a comparison, not a claim about hunks:**
   `git diff --cached -U0 -- spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md` must
   contain **no added and no removed line** matching the frozen statement.
   *Zero context is what makes this satisfiable.* Pin 11's STATUS edit sits a
   few lines above, so at default context the unchanged frozen line appears in
   the diff as ordinary context — and a naive "no hunk touches this line"
   reading would fail a correct execution.

Landing edit 2 is not bookkeeping. P13-S16 shipped exactly this defect twice — a
document whose top was updated while its body went on describing the
pre-execution world — and it is the reason that contract has a §7 at all. This
contract accumulates review round after review round of *"Fixed —"* prose that
reads as present-tense work-in-progress; left unmarked, it would describe a
state the landed tree is not in.

**No count of the rounds appears in this pin or in gate 13.** Round 6 found the
previous wording claiming five rounds while four were recorded — and observed
that repairing the numeral would stale it again the moment another round landed.
The rule is *all of them*, which cannot go stale.

**Gate 13** checks the two landing edits as edits, and the frozen-pins line as an
invariant.

---

## §2. Touch table

The staging allowlist. A file that must change and is not listed here silently
drops out of the commit.

| # | Path | Why |
|---|---|---|
| 1 | `spec/core_spec.tex` | pin 3 (item 10), pin 4 (new requirement), Revision History row |
| 2 | `spec/core_spec.pdf` | tracked build product of row 1 |
| 3 | `crates/epiphany-core/src/invariants.rs` | pin 5 (doc block), pin 8 (`t12` narrowed + renamed) |
| 4 | `crates/epiphany-testkit/tests/invariant_ten_surface.rs` | pin 6 (tests 1 and 2) **and pin 4a (test 3)**, new file |
| 5 | `crates/epiphany-testkit/tests/requirement_labels.rs` | pin 4 moves its three counts — the recurring escapee, listed deliberately. **Also pin 10:** retain the permanent `req:graph:` exception under its landed wording, delete the temporary `req:time:` exception, and delete the `(pre-execution)` scaffolding banner |
| 6 | `spec/PASS13_CANDIDATES.md` | S26 status append; **two new rows, P13-S29 and P13-S30** (pin 7) |
| 7 | `spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md` | **pin 11's two landing edits:** the STATUS block, and marking all review-round blocks above §0 as a dated historical record. *The frozen-pins line is a ratified input, not an edit — written at ratification, and gate 13 verifies it appears as neither an addition nor a deletion in a zero-context staged diff* |
| 8 | `spec/EVIDENCE_P13S26_EXECUTION.md` | **the destination for gates 6 and 7** — every mutation transcript and boundary-gate output, verbatim |

**Row 8 is pinned before dispatch, deliberately.** P13-S16's evidence annex was
written with no touch row, had to be left untracked at acceptance, and gained
its row only by amendment. Gates 6 and 7 require verbatim transcripts; a
contract that demands them without naming a destination cannot be executed
inside its own allowlist.

**Rows deliberately absent, with reasons resolved in review round 1:**

- **No other `.tex`.** No companion document restates item 10's surface.
  `operation_catalog.tex` mentions individual referential preconditions and
  earns no row.
- **No version-literal row. Not applicable:** `core_spec.tex` carries no
  title-page version and never has (`spec/CONTRACT_P13S19_PARTIAL.md` §0 item
  7, verified). The Revision History edit is row 1.
- **No `epiphany-ops` / `epiphany-bundle` row.** Pin 9 forbids behaviour change,
  and invariant 10's tag does not reach the wire.

---

## §3. Mutation plan

Every guard is verified by re-introducing the defect and **observing** the
failure. A compile error observed nothing. Restore by hand-editing, never with
git.

**Every "Must fail" cell below is exhaustive.** Execution records the observed
set for each mutation, and **any mismatch — a test that fails and is not listed,
or a listed test that passes — is itself a finding**, reported, not absorbed.

**Failing-evidence mutations.** Each runs against the full workspace with
`--no-fail-fast`, recording the complete failure set.

| M | Mutation | Must fail — exhaustively |
|---|---|---|
| M1-A | Delete `RepeatStructure.voltas` from item 10's nested `itemize` | test 1 |
| M1-B | Delete `StaffInstance.instrument_override` | test 1 |
| M1-C | Delete `StaffBasedContent.default_metric_grid` | test 1 |
| M1-D | Delete `NotatedComponent.tuplet` | test 1 |
| M1-E | Delete `IndeterminacyHints.alternatives` | test 1 |
| M1-F | Delete `TempoSegment.end` | test 1 |
| M1b | Add a token to item 10 that is not in `INVARIANT_TEN_SURFACE` | test 1 |
| M1c | Change one item-10 target to a different vocabulary term (`live event` → `declared staff` on `Slur.start_event`) | test 1 — **the finding-3 control: token sets are identical, only the pair differs** |
| M1d | Change one item-10 target to a term outside pin 1a's vocabulary | test 1 |
| M2 | Restore item 10 to its pre-rung sentence in full | test 1 |
| M7 | Move one row into a **second nested `itemize`** inside item 10, after the first one closes | test 1 — **the nested-environment boundary control, and it discriminates.** The correct first-environment extractor misses the row and fails; a faulty "scan every `\item` in the outer item-10 slice" extractor still collects it and would pass. Two weaker forms were rejected: an `\item` moved *outside* the `itemize` joins the enclosing `enumerate` and becomes the next top-level invariant, leaving item 10 entirely; and the same text demoted to ordinary prose is ignored by the correct and the faulty extractor alike, so it separates nothing |
| M8 | Narrow test 1's slice to drop the final nested `\item` | test 1 |
| M9 | Duplicate one `\item` inside item 10's nested `itemize` | test 1's **duplicate assertion**, naming the repeated token |
| M4-A…F | Delete the same six tokens, one at a time, from the `/// 10.` block | test 2 |
| M4b | Add a token to the `/// 10.` block that is not in the inventory | test 2 |
| M4c | Destroy the `/// 10.` block entirely | test 2 **and** `t12_invariant_10_doc_block_slices_and_is_non_empty` — the two-crate locality pin 8 exists for |
| M4d | Change one `/// 10.` target to a different vocabulary term (`Slur.start_event — declared staff`) | test 2 — **the Rust-side twin of M1c; without it every M4 row passes on a token-only comparison** |
| M4e | Change one `/// 10.` target to a term outside pin 1a's vocabulary | test 2's canonical-form check — the closed vocabulary is intended on both sides |
| M10 | Duplicate one line in the `/// 10.` block | test 2's **duplicate assertion** |
| M11 | Replace "same region" with "any region" in pin 4's requirement | test 3 |
| M12 | Delete the `ordering` referent from pin 4's requirement | test 3 |
| M13 | Delete the `bounds` referent from pin 4's requirement | test 3 |
| M14 | Permute a **complete** existing target: write `AnalyticalAnnotation.anchor` as `live event, extant region, anchor target` | test 1's canonical-form check **part (a)** — the set is unchanged, so normalization alone would recover the expected pair and only the order assertion can catch it. *An earlier form used a target set no row has, which failed at pair equality and signed nothing about ordering* |
| M15 | Duplicate one entry inside `INVARIANT_TEN_SURFACE` | pin 6 **step 0**, uniqueness branch, in tests 1 and 2 both |
| M16 | Weaken pin 4's requirement from `\MUST{}` to `\SHOULD{}` | test 3's normative-keyword assertion |
| M17 | Put an out-of-vocabulary target on one `INVARIANT_TEN_SURFACE` entry | pin 6 **step 0**, vocabulary branch, in tests 1 and 2 both — without it that branch is asserted and never observed |
| M18 | Add **`\label {tmp:m18}`** — spaced — inside item 10's outer slice. Two pinned choices: **the spaced form**, because it is what discriminates a whitespace-tolerant assertion from an exact-string one that would inherit `command_arguments`' blind spot; and **the non-`req:` namespace**, because a `req:`-shaped literal enters the citation scan, lands in the annex via this mutation's own transcript, and poisons `every_requirement_citation_is_defined` after restoration — pin 10's hazard exactly | test 1's **no-`\label`** assertion, alone. **It is a surrogate, not a demonstration of the remeasured-count condition:** `labels()` keeps only `req:`-prefixed labels, so `tmp:m18` never reaches a count constant and M18 behaves identically before and after remeasurement. It signs the assertion; the count-blindness it guards against is argued, not exhibited |
| M20 | Add, inside item 10's outer slice, exactly:<br>`\begin {requirement}` / `Probe.` / `\end {requirement}`<br>**Spaced, additive, label-free — all three pinned. Two serve parser silence, one serves M20's own isolation, and the contract keeps them apart.** *Spaced* (silence): an exact-spelled block is found by `load_spec` and moves the block counts. *Additive* (silence): re-spacing an existing block drops `CORE_REQUIREMENT_COUNT` and `SUITE_REQUIREMENT_COUNT`. *Label-free* (**isolation, not silence**): parser silence needs only the absence of a `req:` label — a `tmp:` one is invisible to `labels()` — but **any** label here would additionally trip test 1's independent no-`\label` assertion and widen M20's radius to two assertions | test 1's **no-`\begin{requirement}`** assertion, **alone** — the independent second assertion M18 does not reach. Spaced + additive keep every other suite quiet; label-free keeps M18's assertion quiet, so test 1's two assertions stay separately signed |
| M19 | Delete one clause from item 10's opening sentence | test 1's **outer anchor** — proves the anchor is the whole sentence, not a prefix |
| M5 | Delete pin 4's requirement from `core_spec.tex` | `every_requirement_block_has_one_label`, `requirement_labels_follow_the_grammar`, `requirement_labels_are_unique_across_the_suite`, `every_requirement_citation_is_defined` — all four assert a count constant this deletion moves — **and `aleatoric_reference_locality_states_both_referents_and_locality`**, whose label anchor the deletion removes |
| M6 | Re-label pin 4's requirement as `req:graph:…` | `requirement_label_areas_match_their_chapters`, `every_requirement_citation_is_defined` (this contract cites the original `req:time:` label, and that test scans repository text), **and `aleatoric_reference_locality_states_both_referents_and_locality`**, whose label anchor the relabel removes. Run **after** pin 10's allowlist entry lands |

**Passing-evidence mutation — exactly one, and its required outcome is success.**

| M | Mutation | Required outcome |
|---|---|---|
| M3 | In test 1, replace `assert_eq!(actual, expected)` with `assert!(actual.is_subset(&expected))` — **that direction specifically**, since `expected.is_subset(&actual)` still fails after a deletion — then re-run **M1-B** alone | **M1-B stops failing.** Evidence is the passing guard and a green full-suite run, not an assertion diagnostic. This is the positive control proving equality is load-bearing; without it, nothing distinguishes an exact inventory from a spot-check. |

**No mutation restores the G3a aside.** Per ruling 1(a) and §0.1, there is no
falsehood to reinstate, and a mutation asserting one would sign for a defect
that does not exist.

---

## §4. Gate

1. `cargo test --workspace` — **the baseline in `CLAUDE.md` plus this rung's net
   new tests**, 0 failed, 0 ignored. The count is sourced there, never restated
   here. Use `--no-fail-fast` whenever anything is failing.
2. `cargo +1.95.0 clippy --workspace --all-targets -- -D warnings` — clean.
3. `cargo +1.95.0 fmt -p epiphany-core -p epiphany-testkit --check` — clean.
   **Never `--all`.**
4. Every staged path is a §2 row; every §2 row is staged or named unused.
5. `git diff --cached --check` — clean (no whitespace errors).
6. **Pin 9's boundary gate.** On the staged tree:
   - `git diff --cached -U0 -- crates/epiphany-core/src/invariants.rs` — every
     hunk falls **either** inside the `/// 10.` doc block **or** inside the test
     module. No hunk touches a check body.
   - The same diff contains **no** added or removed line matching
     `InvariantViolation::new` or `fn check_` anywhere, and none matching
     `GraphInvariant::` **outside the `/// 10.` doc block and the test module**
     — pin 5's rider note may legitimately name the tag it is about, and a gate
     that forbade it could not be satisfied.
   - Both outputs are recorded verbatim in **`spec/EVIDENCE_P13S26_EXECUTION.md`**
     (touch row 8), not summarized.
7. Every failing-evidence mutation of §3 observed, each with the failing
   assertion quoted verbatim **and** the complete `--no-fail-fast` failure set,
   **compared against the exhaustive "Must fail" cell**; any mismatch reported
   as a finding. M3 recorded separately, as a passing guard and a green suite.
   All of it lands in `spec/EVIDENCE_P13S26_EXECUTION.md`, which is a **tracked
   deliverable of this rung**, not a scratch file.
8. Pin 1's table re-derived against the check bodies **after** all edits, and
   confirmed unchanged — the derivation is the origin, so it is the thing that
   must still be true at the end. `INVARIANT_TEN_SURFACE` is compared to it pair
   by pair, and every Token re-checked against its declaring struct.
9. `cd spec && latexmk -xelatex -interaction=nonstopmode core_spec` — re-run
   until *"There were undefined references"* clears. `core_spec.pdf` is tracked
   and rebuilt. (This machine needs the `~/.config/fontconfig/fonts.conf` entry
   exposing the TeX tree; without it `fontspec` fails with ~46 errors that look
   like a broken source.)
10. **Pin 7's complete ledger filing, read against the staged
    `spec/PASS13_CANDIDATES.md`, not inferred from row presence:**
    - the P13-S26 row has §0.1's correction appended;
    - **P13-S29 is present and unresolved**, cites §0.4's rendered example,
      states that repairing the attribution requires a **behaviour change**, and
      leaves that repair outside S26;
    - **P13-S30 is present and unresolved**, names all four consequences **with
      the conditions under which each is silent or loud**, and cites M20's
      transcript in the annex as evidence for the **spaced-opener** case only,
      explicitly **not** for the exact-open/spaced-close case, which M20's
      doubly-spaced probe cannot reach;
    - P13-S30 also records the scoping boundary exactly: every consumer found
      during scoping is **test-scope**, no production consumer was found, and a
      later production-site discovery extends the row rather than contradicting
      it;
    - **P13-S30 preserves the consumer inventory in full** — every site named in
      the duplicated-scanner consequence, each with its file and symbolic
      position, **including both delimiters of every requirement-block scanner,
      opener and closer alike**, none dropped in transcription. *This gate item exists because
      two sites surfaced by a scoping grep were then omitted from the row; an
      inventory that loses entries between contract and ledger is the defect
      this rung is about;* and
    - repairing either P13-S29 or P13-S30 remains outside this rung.

    A row id alone is insufficient. A stub row, a row with the required
    evidence or scoping result shortened away, or either row marked resolved is
    a gate failure.
11. **Read-checked, no machine observer** — each declared as such at its pin,
    never left looking guarded. **Tests 1 and 2 observe the token inventories,
    item 10's opening sentence and its label-freedom, and nothing else** — test
    3 deliberately observes requirement prose and is not part of this
    complement. Every other prose outcome pin 3 and pin 5 require lives here:
    - item 10's `req:time:tempo-segment-order` cross-reference is present (pin 3);
    - item 10's **re-anchoring exception clause** and its
      `Chapter~\ref{ch:semops}` cross-reference are present (pin 3) — both are
      deletable with the nested inventory left perfectly intact;
    - `core_spec.tex`'s Revision History row is present (pin 3);
    - the `/// 10.` block's **rider separation** is present and **names
      P13-S29** as the riders' owner (pin 5) — test 2 reads only pin-shaped
      token rows and cannot see its absence;
    - the **G3a aside is absent** from the `/// 10.` block (pin 5). *§0.1 rules
      out a mutation that restores it — there is no falsehood to reinstate — but
      declining to mutate it is not the same as observing its removal, and the
      aside is not pin-shaped, so test 2 would pass with it still there;*
    - pin 6's tests carry the comment stating **why the slice is mandatory**;
    - the staged `core_spec.tex` diff adds **exactly one** `\label` — pin 4's —
      counted **whitespace-tolerantly**, so `\label {…}` counts too. *Test 1's
      assertion covers item 10 only; the count-remeasure blindness round 10
      identified is general, so a label added anywhere else in the file is
      equally absorbed. This is the file-wide read-check for it, and it must not
      inherit `command_arguments`' exact-needle blind spot;*
    - the rider classes of §0.3 appear in neither repaired document as
      invariant-10 content (pin 2).
12. **Pin 10's landed form**, in the staged `requirement_labels.rs`:
    - the **temporary** `req:time:aleatoric-reference-locality` row is **absent**;
    - the **permanent** `req:graph:` row is **present**, under final wording —
      *not defined in the restored tree*, never "never exists";
    - that row's reason still names **both provenance paths** —
      `spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md` and
      `spec/EVIDENCE_P13S26_EXECUTION.md`. A shortened reason satisfies every
      other predicate here while stranding the tuple with no route back to why
      it exists, which is the whole reason it survives the banner;
    - the string `REVIEW SCAFFOLDING` is **absent** from the file.

    Every item here is inert if wrong — a stale row, a stale banner and a stale
    reason all pass every other check — which is why they are gated by reading
    rather than left to the suite.
13. **Pin 11's landed form of this contract.** Two edits and one invariant,
    checked as such:
    - *edit* — STATUS reads exactly `STATUS: LANDED by this commit.`, with
      **no hash**;
    - *edit* — **all** review-round blocks above §0 are marked as a dated
      historical record;
    - *invariant* — the frozen-pins statement is **present**, and
      `git diff --cached -U0 -- spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md`
      contains **no added and no removed line** matching it. It was written at
      ratification; an execution that touches it is a finding, not a discharge.
      Zero context is required — the STATUS edit is close enough that the
      unchanged line would otherwise appear as diff context.

    Same inertness argument as gate 12 — nothing in the suite reads this file's
    prose.

---

## §5. What ratification does NOT settle

- **Whether the tag multiplexing gets repaired.** Pin 7 files P13-S29 **at
  execution**; its disposition is then its own rung.
- **The tempo shape/`end_tempo` labelling gap** (§0.5). Out of scope, and
  entangled with P13-S8's open ruling.
- **Whether item 10's exception clause is itself accurate.** This rung preserves
  it verbatim and does not audit the re-anchoring rules it defers to.
- **Whether `INVARIANT_TEN_SURFACE` should eventually be derived mechanically.**
  It cannot be today; gate 8's hand re-derivation is the standing compensation.
- **The TeX-syntax / ad-hoc-parser mismatch, which pin 7 files as P13-S30.**
  Repairing it is not this rung's work. The analysis **will live** in that
  ledger row once execution writes it — deliberately not here, since a §5
  bullet becomes a historical record at landing and that is not where a live
  defect should be owned. Two facts belong to *this* contract and stay: its own
  guards are whitespace-tolerant and therefore unaffected, and M18's surrogate
  status depends on the non-`req:` branch of the defect (a spaced `tmp:` label
  reaches no other assertion, leaving test 1 the sole discriminator).

---

## §6. AMENDMENT 1 — PIN 3, PIN 6 AND PIN 11, PRE-FIRST-EDIT

STATUS: LANDED by this commit.

Ratified 2026-08-11 on the authority of the repository owner, review round 5
returning zero findings. The amended pins are executed, not edited; a further
defect is its own amendment with its own review round.

**DATED HISTORICAL RECORD — this amendment is ratified, its pin changes are
executed, and the hold it describes ENDED at `f9170b0`.** Everything in §6,
including its revision records, its lifecycle statements and §6.8's disposition,
is an account of what was found and decided on 2026-08-11. None of it states
current state.

On that date the contract was dispatched for implementation, execution began,
and it stopped on reaching pin 3; the top status paragraph still read NOT YET
DISPATCHED, and 6.7 prescribed its replacement. Throughout the hold no pin 3 or
pin 6 implementation target was modified — `core_spec.tex`, `invariants.rs`, the
ledger and the guard file were untouched at `01c621d`, and the only change was
this amendment's own draft.

**Both of 6.7's ratification-act edits were applied at `f9170b0` and the hold was
lifted.** Execution resumes at pin 3.

### REVISION E — review round 5. One exactness correction. ACCEPTED.

1. **[P2] The prescribed clause replacement was not verbatim.** 6.7 quoted the
   text to remove as rendered prose — *"NOT YET DISPATCHED; ratification and…"* —
   while the source carries `**` emphasis markers and an internal line break.
   Applied literally it would have matched nothing, or left unbalanced emphasis.
   **Fixed** — the remove and insert texts are now pinned as **Markdown source
   blocks**, and because a clause replacement inside a wrapped paragraph
   necessarily moves line breaks, the **whole resulting paragraph** is pinned
   too. *A prescription meant to be applied literally must be quoted in the
   register it will be applied to; this one was quoted in the register it is
   read in.*

### REVISION D — review round 4. Two blocking findings, one smaller. ALL ACCEPTED.

1. **[P1] Two residual phrases overstated what was established.** 6.4 said
   *"both halves were wrong"* of revision A's rationale — but only the checker
   argument is demonstrably a category error; the **no-permission** half is
   **unproved**, and nothing here shows it false either. And 6.8 still called
   revision A the *"bare coverage form"* after revision C had reclassified it as
   **ambiguous**. **Fixed** — the two halves are now distinguished by *how* they
   fail, and 6.8 uses the settled classifications. *The correction that
   distinguishes unproved from wrong had itself not reached two sentences that
   depend on it — the same one-hop shape, now inside the subsection whose
   subject is that distinction.*
2. **[P1] The prescribed top-status edit would have erased the ratification
   state.** The status is a multi-line paragraph carrying the ratification date,
   the owner's authority and the no-count rule; replacing "the top status line"
   wholesale discards all three to repair one false clause. **Fixed** — 6.7 now
   prescribes an exact **clause** replacement, quoting the text to remove and
   the text to insert, with everything around it standing.
3. **[P2] The execution-target statement contradicted itself**, claiming no
   execution target had changed and then naming this contract as pin 11's.
   **Fixed** — scoped to *pin 3 and pin 6 implementation targets*, with the
   amendment draft named as a change to the future pin-11 target that is not an
   execution edit.

**Also settled by the round, and recorded so it is not re-litigated:** M19's
forward-slice assumption (revision B's open question) **holds** — pin 6 already
requires the nested list to follow the anchor, so a backward search would
violate the pin rather than expose an unresolved choice.

### REVISION C — review round 3. Two blocking findings, two smaller. ALL ACCEPTED.

1. **[P1] Coverage still implied permission in the *rejection* reasoning.** 6.4's
   claim was fixed in revision B; the two sentences explaining *why* the
   cross-cutting form was rejected still argued from reach. **Fixed** — the
   cross-cutting form is now recorded as **unproved, not wrong**: proving it
   either way needs the surface derivation 6.4 declines. Revision A's bare form
   is recorded as **ambiguous**, not demonstrably coverage-based. *The
   conflation survived one round inside the paragraph that corrects it.*
2. **[P1] The dispatch reconciliation belonged to this amendment's ratification
   act, not to pin 11.** The contract is already dispatched and its top status
   is already untrue; routing the fix through pin 11 would leave the document
   lying about its lifecycle for the whole of execution. **Fixed** — 6.7 now
   prescribes **two ratification-act edits** with exact literals (§6's status,
   and the top status), and pin 11 keeps only the landed forms.
3. **[P2] "Filed" claimed an authority that does not exist.** This contract
   establishes that filing means a ledger row, and none is added for the surface
   question. **Fixed** — *presented*, not filed.
4. **[P2] The live review heading was stale**, naming round 2 while introducing
   round 3. **Fixed** — *What the next review must decide*, which cannot stale.

Also adopted: the section heading reads **PRE-FIRST-EDIT** rather than
PRE-EXECUTION, since execution began and made no edit.

### REVISION B — review round 2. Three blocking findings. ALL ACCEPTED.

1. **[P1] Rule coverage was still being mistaken for permission.** The Cue and
   Trajectory rows prescribe **cascade deletion** and **immediate replacement**;
   neither leaves a reference dangling. They prove re-anchoring **reaches**
   group E — they do not prove dangling is **permitted** there. **Fixed** — 6.4
   now says exactly that, and 6.3's universal defers to *explicit permission*
   without claiming where it exists.
2. **[P1] The uniqueness assertion masked M19.** Truncating the real opening
   drops the complete-anchor count to zero, so uniqueness fails **before** the
   slice's complete-versus-prefix behaviour is reached — leaving M19 and M21
   both signing uniqueness and nothing signing the anchor's completeness.
   **Fixed** in 6.6 by the construction review round 2 supplied.
3. **[P1] The lifecycle gate was underspecified and unsatisfiable.** Ratification
   and landing needed **separate exact status literals**, or §6 sits at DRAFT
   between them; and gate 13 could not demand that §6 contain no `DRAFT`, since
   §6 uses the word historically. **Fixed** in 6.7 — two literals, and the gate
   reads the **status line**, not the section.

### REVISION A — review round 1. Five findings, four blocking. ALL ACCEPTED.

1. **[P1] The proposed universal contradicted the retained exception.** *"Every
   graph reference resolves…"* covers group A too, so it forbade exactly the
   transient states the next sentence permits. **Fixed** — the universal is
   explicitly subordinated (6.3), and ordering alone is no longer relied on.
2. **[P1] The exception-scope rationale was not derived and rested on a false
   premise. Withdrawn in full** — see 6.4, which replaces it.
3. **[P1] M19 no longer signed its property**, and the short anchor was
   unguarded against duplication. **Fixed** in 6.6.
4. **[P1] The amendment's own lifecycle reached no pin.** **Fixed** in 6.7.
5. **[P2] Totals appeared in the amendment**, against pin 1's rule that no total
   is stated anywhere in this contract. **Fixed throughout** — the surface is
   referred to as *every pin-1 row*, and the out-of-scope portion as *every row
   in groups B–F*.

### 6.1 The defect

Pin 3 requires item 10 to **keep its opening sentence** and to gain a nested
`itemize` carrying **every pin-1 row**. Those are not jointly satisfiable.

The retained sentence is:

> Every **cross-cutting structure's** references resolve to extant objects in
> the graph, except where explicit re-anchoring rules permit transient dangling
> states during edits (see `Chapter~\ref{ch:semops}`).

**"Cross-cutting structure" is a defined term in the same chapter** (the
Hybrid-topology principle): *"The graph is a tree of containment overlaid with
cross-cutting structures that hold references."* It denotes pin 1 **group A**.
**Every row in groups B–F falls outside it**, and the checker says so at the
group-E loop: *"These are **not** cross-cutting structures but they bear graph
references that can dangle."*

So the sentence scopes to one group while the list beneath it spans all of them.

**Why the contract did not catch this.** §0.2 measured that the `.tex` names *no
individual reference class* and stopped; it never asked whether the sentence's
**scope** was also wrong.

### 6.2 Why this is a pin change, not an execution judgment

Pin 3 permits group headings "rendered as prose lead-ins outside the nested
`itemize`". A scope-broadening opening is not a group heading, so executing it
that way would be editing a frozen pin by interpretation.

### 6.3 Replacement for pin 3's item-10 shape — RATIFIED at `f9170b0`

Item 10 becomes, in this order:

1. **A subordinated universal, deferring to explicit permission only:**
   *"Except where the re-anchoring rules of `Chapter~\ref{ch:semops}` explicitly
   permit transient dangling states during edits, every graph reference resolves
   to an extant object."*
2. **The ratified sentence, retained verbatim**, including its exception clause
   and its `Chapter~\ref{ch:semops}` cross-reference.
3. The `req:time:tempo-segment-order` cross-reference pin 3 already requires.
4. **Exactly one** nested `itemize`, carrying **every pin-1 row** in pin 3's
   pinned form.

**On the form of the subordination.** Two candidate forms were rejected before
this one, and **neither was rejected as demonstrably wrong** — that distinction
is the point of 6.4. A **cross-cutting-scoped** exception (review round 1's
example) may well be complete; **its completeness is unproved**, and proving it
means deriving the permission surface, which 6.4 declines to do. A bare *"except
as the re-anchoring rules permit"* (revision A) is **ambiguous**: it can be read
as deferring to whatever those rules cover, and coverage is not permission. The
adopted form defers only to where those rules **explicitly permit transient
dangling states** — it names the normative source, matches the retained
sentence's own phrasing, and asserts nothing about which references enjoy that
permission, so it needs no surface derivation to be sound.

### 6.4 The exception's surface is NOT determined here — the earlier rationale is withdrawn

Revision A's §6.3 argued that the exception clause must stay bound to
cross-cutting references because broadening it *"would licence transient
dangling for structural, meter, event-internal and tempo references, which no
rule permits and the checker does not tolerate."* **Neither half stands, but
they fail differently, and the difference is the whole lesson of this
subsection:** the *"no rule permits"* half is **unproved** — nothing here shows
it false either — while the *"checker does not tolerate"* half is
**demonstrably a category error**.

- **Re-anchoring reaches beyond group A.**
  `req:semops:tombstoned-reference-resolution` says it governs *"cross-cutting
  structures **and attachments**"*, and its table carries **Cue event / Source
  event** and **Trajectory event / Endpoint pitch** — `CueEvent.source` and
  `TrajectoryEvent.start`/`.end`, **group E**.

  **Reach is not permission, and revision A conflated them.** Those two rows
  prescribe **cascade deletion** and **immediate replacement** of the reference;
  neither leaves anything dangling, transiently or otherwise. They establish
  that the re-anchoring rules *govern* group-E references, and **nothing** about
  whether a group-E reference may transiently dangle.

  **So this does not show a cross-cutting-scoped exception to be wrong** — only
  that it is **unproved**. Deciding it either way requires the surface
  derivation 6.4 declines. **No claim either way is made here**, which is why
  6.3's universal defers to explicit permission rather than to coverage.
- **The checker cannot settle it.** `check_invariants` observes a *final* state
  and says nothing about which transitional states the operation rules permit.
  Citing it was a category error.

**What this amendment therefore claims:** the retained sentence keeps its own
grammatical scope, unchanged, and the universal is subordinated to the
re-anchoring rules **as a whole** rather than to any surface this amendment
asserts. **The exception's true surface is left open**, because deriving it
means reading the whole re-anchoring table against pin 1's inventory — work with
its own evidence obligations, which is a rung, not a clause.

**Presented for the owner's disposition, not filed:** whether that derivation
should be a candidate in its own right. *"Filed" would be false — this contract
establishes that filing means a ledger row, and no row is added for it;* S26
needs no derivation, so the question is left, not lodged. It is adjacent to P13-S26 but not owned
by it, and P13-S30's precedent is that a live gap belongs in the ledger rather
than in a contract section that becomes historical.

### 6.5 Consequential change to pin 6 — the anchor

Test 1's outer anchor is pinned as "the complete opening sentence, matched as an
exact literal". Under 6.3 that sentence changes, so the anchor becomes the
sentence in 6.3 item 1.

### 6.6 Consequential change to pin 6 — anchor uniqueness, and M19

**Uniqueness, newly required.** The anchor is short, so pin 6 additionally
requires it to occur **exactly once within the graph-invariants requirement** —
the `requirement` environment labelled `req:graph:score-graph-invariants` — and
test 1 asserts that count. A second occurrence inside that block makes the outer
slice ambiguous, and no other assertion would notice.

**New mutation M21** — duplicate the anchor sentence elsewhere inside
`req:graph:score-graph-invariants`; test 1's **uniqueness assertion** must fail,
and it alone.

**M19 is rewritten, in two parts, and the second part is what makes it work.**
The ratified M19 deletes "one clause from item 10's opening sentence"; revision
A truncated the trailing portion. **Both fail against the uniqueness assertion
first** — truncating the real opening drops the complete-anchor count to zero,
so uniqueness fires before the slice's complete-versus-prefix behaviour is ever
exercised, and M19 degenerates into a second signature for M21.

**New M19, per review round 2's construction:** truncate the real opening
sentence **and** place one **complete** anchor sentence *after* item 10's nested
list. Then:

- the complete-anchor count is **one**, so the uniqueness assertion passes and
  does not mask anything;
- a **prefix**-matching slice finds the real, truncated item, brackets the
  nested list correctly, and **passes**;
- the **pinned complete-literal** slice starts at the anchor *after* the list,
  so the nested `itemize` falls outside it, extraction comes back empty and it
  **fails**.

That is the discrimination M19 is for: the two anchor kinds are driven to
opposite verdicts by one mutation. **M21 remains the duplicate-count
signature**, and the two no longer overlap.

### 6.7 Consequential change to pin 11, touch row 7 and gate 13 — RATIFIED at `f9170b0`; the ratification-act edits are DISCHARGED

Pin 11 and gate 13 speak only of "review-round blocks above §0", so **this
amendment and its review record could land still reading DRAFT and HELD** —
the exact defect pin 11 exists to prevent, one section further down the
document than pin 11 looks.

**Proposed:**

**Ratification and landing are different moments, and they are owned by
different acts.** With one literal, §6 sits at `DRAFT` through the whole of
execution.

**Two edits belonged to this amendment's own ratification act — not to pin 11**,
because the contract was *already dispatched* and its top status *already*
untrue; waiting for landing would have left the document lying about its
lifecycle for the entire execution. **Both were applied at `f9170b0` and are
DISCHARGED:**

- **§6's status line** becomes exactly:
  `STATUS: RATIFIED. Pins 3, 6 and 11 amended. Execution resumes.`
- **The contract's top status paragraph** has exactly one **clause** replaced.
  It is **not** rewritten: the paragraph carries the ratification date, the
  owner's authority and the no-count rule, and replacing it wholesale would
  discard all three to fix one false clause.

  **These are Markdown *source* strings, not rendered text.** An earlier
  revision quoted the clause without its `**` emphasis markers and without its
  internal line break; applied literally it would have matched nothing, or left
  unbalanced emphasis behind.

  **Remove — exactly these two source lines, emphasis markers and line break
  included:**

  ```
  **NOT YET DISPATCHED**; ratification and dispatch are
  separate acts and no execution has begun.
  ```

  **Insert — exactly this source text:**

  ```
  **DISPATCHED 2026-08-11.** Amendment 1 ratified;
  execution resumes.
  ```

  **The paragraph is then re-wrapped**, which is presentational only. Since a
  clause replacement inside a wrapped paragraph necessarily moves line breaks,
  the **whole resulting paragraph** is pinned here so application is
  unambiguous:

  ```
  **Status:** **RATIFIED 2026-08-11, on the authority of the repository owner**,
  after the independent whole-artifact passes recorded above §0 — the last
  returning zero findings. **DISPATCHED 2026-08-11.** Amendment 1 ratified;
  execution resumes. Which passes closed and what each found are those records;
  this line does not restate them, and states no count of them, per the rule
  this contract adopted after its own tallies went stale twice.
  ```

  The ratification date, the owner's authority and the no-count rule all
  survive, which is the point of replacing a clause rather than the paragraph.

**Pin 11 continues to own only the later landed forms**, and gains one — this
part remains a change to pin 11, executed at landing, not at ratification:

- **On landing**, §6's status line becomes exactly
  `STATUS: LANDED by this commit.` — the same no-hash rule as pin 11's own
  STATUS, for the same reason.

**Amended into pin 11, touch row 7 and gate 13:**

- **Pin 11** gains a third landing edit: **§6's status line** takes its landed
  literal, and **§6's revision records, preamble and lifecycle records are
  marked a dated historical record**, on the same rule as the review-round
  blocks. *Widened by amendment 2 from "revision records" alone, which left the
  preamble and the section's own lifecycle statements unowned — the defect
  amendment 2 exists to close.*
- **Touch row 7** names that third edit alongside the other two.
- **Gate 13** asserts **the status line specifically** — that §6's status line
  reads exactly `STATUS: LANDED by this commit.`, and that §6's revision
  records, **its preamble and its lifecycle records** are marked historical. **It must not search §6 for the word `DRAFT`**: the
  revision records explain what the drafts said and legitimately contain it, so
  a section-wide search would be unsatisfiable by a correct execution.

**Why the top status is reconciled at ratification rather than at landing.**
It read **NOT YET DISPATCHED** while §6 recorded **DISPATCHED; HELD** — the
document contradicting itself about its own lifecycle, the class this rung has
caught repeatedly. Deferring the fix to pin 11 would have kept that
contradiction live for the whole of execution, which is precisely the interval
in which a reader most needs the status to be true. It was therefore an
**amendment-ratification** edit, and it was **applied at `f9170b0`**.

### 6.8 Review disposition — CLOSED at `f9170b0`

Every question this section put to review was answered before ratification.
None is open.

1. **The explicit-permission subordination is the right instrument.** Review
   round 5 accepted it, the cross-cutting form standing **unproved** and
   revision A's bare form **ambiguous**. Closed.
2. **M19's two-part construction holds.** Review round 4: pin 6 already requires
   the nested list to follow the anchor, so a backward-searching slice would
   violate the pin rather than expose an unresolved choice. Closed.
3. **No further coverage-for-text substitution remains in §6.** Review rounds 3
   and 4 found the class twice more — in 6.4's claim, then in its own rejection
   reasoning — and round 5 returned zero findings against the whole section.
   Closed.
4. **The two ratification-act edits were correctly scoped**, and are
   **discharged** at `f9170b0`: §6's status literal, and the clause-level
   replacement in the top status paragraph. Closed.

---

## §7. AMENDMENT 2 — AMENDMENT 1'S LIFECYCLE STATEMENTS

**STATUS: RATIFIED 2026-08-11 on the owner's instruction. Applied by the same
commit that records it.** Amendment 1 stands as ratified at `f9170b0`; **this
amendment does not rewrite that commit**, and changes no pin 3, pin 4, pin 5,
pin 6, pin 7 or pin 10 content.

### 7.1 The defect

Ratifying amendment 1 discharged its *prescriptions* and left its *prose*
describing the state it was written in. §6 went on calling itself a draft, its
replacement "proposed", its ratification act pending, and its review questions
open — while §6's own status line read RATIFIED and the hold had been lifted.

**Pin 11 did not cover this.** Its landing edit reached §6's *revision records*
only, so the preamble, the section and subsection lead-ins, and the review
disposition were owned by nothing. That is the same shape as the defect pin 11
was written to prevent, one layer further in: **a document describing a state it
is not in**, this time about its own amendment rather than about the rung.

Recorded plainly because the pattern is the rung's whole subject: **ratification
is not self-applying.** A frozen prescription discharges; the prose around it
does not, unless something owns it.

### 7.2 What this amendment applied

1. **§6's preamble** becomes a **dated record** ending the hold at `f9170b0`,
   in past tense throughout, stating that execution resumes.
2. **§6.3** is marked **RATIFIED at `f9170b0`** rather than "Proposed
   replacement".
3. **§6.7** is marked **RATIFIED**, its two ratification-act edits
   **DISCHARGED**, and its closing paragraph put in past tense — it previously
   asserted that "this amendment is not yet ratified".
4. **§6.8** becomes a **closed review disposition**, each question answered and
   attributed to the round that answered it, replacing "What the next review
   must decide".

### 7.3 Pin 11 and gate 13 are widened

**Pin 11's third landing edit** now covers **§6's revision records, preamble and
lifecycle records**, not revision records alone. **Gate 13** asserts the same
widened set, still reading §6's **status line** specifically and never searching
the section for `DRAFT` — the revision records legitimately contain it.

**Why widening rather than another enumeration:** listing the sites that were
stale would leave the next lifecycle sentence unowned in exactly the way this
amendment is repairing. The category — §6's lifecycle statements — is what pin
11 must own.

### 7.4 Scope

No implementation target is touched. `core_spec.tex`, `invariants.rs`, the
ledger and the guard file remain untouched at `01c621d`. Execution resumes at
pin 3 with amendment 1's shape unchanged.

---

## §8. AMENDMENT 3 — TEST 3'S CLAUSE SCOPE, POST-EXECUTION

STATUS: LANDED by this commit.

> **DATED HISTORICAL RECORD — amendment 3 is ratified and executed. Its
> revision records, its finding statement and §8.6's disposition are an
> account of what was found and decided on 2026-08-11. None of it states
> current state.**

**Ratified 2026-08-11 at `7c8a30d` on the authority of the repository owner**,
review round 4 returning zero findings. The rung landed at `eddf6e9`; **this amendment does not
reopen it.** The amended surface is executed, not edited; a further defect is its
own amendment with its own review round.

**§8 is an ADDITIVE OVERRIDE.** Pin 4a and §3 are **not rewritten in place** —
they are frozen and were executed. Where §8 conflicts with pin 4a's claim about
what test 3 buys, **§8 governs from its ratification**; where it adds mutations,
they are §8's, listed here, not inserted into §3's table.
`spec/EVIDENCE_P13S26_EXECUTION.md`'s 38/38 matrix is a historical record of
what execution measured and **is preserved unchanged**; M22–M27 evidence is
*appended* as its own section.

### REVISION D — review round 4. One blocking finding. ACCEPTED.

1. **[P1] The exactly-one-`\MUST{}` assertion had no signature.** It is step 1
   of 8.3 and every earlier mutation varies the clause's *contents*, so none of
   them reaches it. **Fixed** — **M27** leaves the normative sentence unchanged
   and adds a second `\MUST{}` to the recap: a correct implementation fails the
   count before selection begins, while one that silently takes the *first*
   occurrence selects the original sentence and passes every clause assertion.
   *This closes the last item §8.6 carried open, and it was an open item because
   I had specified an assertion without asking what would exercise it.*

### REVISION C — review round 3. Four blocking findings, one smaller. ALL ACCEPTED.

1. **[P1] §8.3 and M25 contradicted each other.** Revision B made the clause
   start *always* fall after the label; under M25 that produces one slice
   running from the label through the recap, containing all four needles, so
   **M25 would have passed**. **Fixed** — the selector is restored to *last
   `". "` before the occurrence, falling back after the label only when there is
   none*, which is what makes M25 discriminate. *Revision B fixed the fallback
   and broke the rule it was a fallback for.*
2. **[P1] The fallback had no signature.** Nothing failed if an implementation
   anchored the start at the block instead of after the label. **Fixed** —
   **M26**, the only mutation in the set that can tell those two
   implementations apart.
3. **[P1] M24 and M25 were not verbatim**, using prose and ellipses for their
   recap edits while §8.4 claimed source blocks. **Fixed** — every mutation now
   carries complete fenced before/after blocks for the whole requirement.
4. **[P1] Revision A's fourth finding fell below revision B's heading**, leaving
   revision A reading as three findings and a stray second item 4. **Fixed** —
   both historical records are accurate again.
5. **[P2] M24 and M25 did not propagate into the live summaries.** The preamble
   said "M22/M23 evidence", 8.4 opened with "Both", and its closing credited
   only M22/M23. **Swept to M22–M26**, with no numeric total stated; row 8 and
   gate 7 likewise.

### REVISION B — review round 2. Three blocking findings, one smaller. ALL ACCEPTED.

1. **[P1] The clause start did not select a sentence.** The normative sentence
   is *first* in the requirement, so no preceding `". "` exists and the fallback
   silently started the slice at `\begin{requirement}`, swallowing the label.
   **Fixed** — the fallback is anchored **after the `\label{…}` argument**.
   *(Revision B over-applied this to every case; revision C item 1 corrects it.)*
2. **[P1] Locality and normative force lacked symmetric scope mutations.** A
   faulty implementation could clause-scope `ordering` and `bounds` while
   searching the whole block for `same region` and `\MUST{}`, and M11, M16, M22
   and M23 would all behave as pinned. **Fixed** — **M24** and **M25**.
3. **[P1] The ratification literal claimed implementation too early.**
   `RATIFIED. Test 3 clause-scoped; DISPATCHED.` is false for the whole interval
   between ratification and execution. **Fixed** — prospective wording.
4. **[P2] M22 was described as the exact mutation previously run.** The annex
   records a *partial referent deletion*, not this sentence replacement.
   **Fixed** — M22 *reproduces* the measured escape.

### REVISION A — review round 1. Four blocking findings. ALL ACCEPTED.

1. **[P1] M22 was not literally executable** — it wrote `\n` as two Markdown
   characters where a LaTeX source line break was meant. **Fixed**: fenced
   source blocks and grammatical sentence replacements.
2. **[P1] M22 alone did not sign both referents' clause scope.** An
   implementation could scope `ordering` to the normative sentence and keep
   checking `bounds` over the whole block: M13 would still fail, M22 would still
   pass, and the asymmetry would ship. **Fixed** — **M23**.
3. **[P1] The execution and lifecycle surface was incomplete.** **Fixed** in
   8.5: rows 4, 6, 7 and 8, with §8's own status transitions pinned.
4. **[P1] "Sentence containing `\MUST{}`" was under-specified**, leaving an
   implementation free to take the first match or widen back to the block.
   **Fixed** in 8.3 with an exactly-one assertion and explicit boundary rules.

**Rulings adopted:** the same-sentence rule stands and is deliberate — it guards
the *current* normative construction, and a semantically valid multi-sentence
rewrite **should** fail and force deliberate guard review. M23 added.

**Round 1 also corrected a false premise of mine:** §8.6 asked whether all three
tests should share normalisation by construction. **Test 2 does not normalise at
all** — it parses raw Rust lines with `strip_prefix("/// ")`. Only **tests 1 and
3** share `normalise`.

### 8.1 The finding — measured, not reasoned

Pin 4a says test 3 buys that *"neither referent, nor the locality claim, nor the
requirement's normative force can silently leave"*.

**M12's first application disproved the first clause.** The `ordering` referent
was deleted from the requirement's **normative clause** — the sentence carrying
the `\MUST{}` — and left standing in the closing recap. **Test 3 stayed green.**
Transcript: `spec/EVIDENCE_P13S26_EXECUTION.md` §6(b).

The cause is structural: test 3 asserts phrase presence over the **whole
requirement block**, so a referent that leaves the sentence where it does
normative work survives anywhere else in the block.

### 8.2 Disposition: strengthen the guard, do not soften the claim

Softening pin 4a to promise only what block-scoped presence delivers would make
the contract honest and the guard no better, leaving a measured escape
undetected in a rung whose subject is documentation drifting because nothing
watches it. **§8 strengthens test 3 instead**, so pin 4a's claim becomes true
rather than smaller.

### 8.3 The strengthened test 3 — clause selection, specified

Test 3 slices the requirement block as now, in text normalised by the existing
`normalise` helper (whitespace runs collapsed to one space), then:

1. **Assert exactly one `\MUST{}`** occurs in the normalised block. More than
   one and the clause is ambiguous; none and the requirement has no normative
   force. *This assertion exists so no implementation may silently take the
   first match.*
2. **Select the clause** around that single occurrence, at position `p`:
   - **start** — the character after the **last occurrence of `". "` at or
     before `p`**; **only if there is none**, the character after the block's
     `\label{…}` argument. Both halves are load-bearing. The fallback is needed
     because the normative sentence is *first* in this requirement, so no
     preceding `". "` exists and a block-start default would swallow the label,
     which is not a sentence. The last-period rule is needed because M25 moves
     `\MUST{}` into a later sentence, and an unconditional fallback-after-label
     would then return one slice spanning label to recap — containing every
     needle, and passing.
   - **end** — the character after the first `". "` at or after `p`, taking the
     period; if there is none, the block end.
3. **Run all four assertions on that slice, and on nothing else:** it names
   `ordering`, names `bounds`, states `same region`, and carries `\MUST{}`.

The recap sentence is out of scope, which is the whole of the measured gap.

**Test 3 remains phrase presence**, and weaker than tests 1 and 2, which compare
exact sets. What changes is *where* the phrases must appear.

**Normalisation is shared with test 1 only.** Test 2 reads raw Rust source line
by line and never normalises; the `.tex` side needs it because that source is
hard-wrapped and a clause spans lines.

### 8.4 The signing mutations

Each is a replacement of the **whole requirement block**, given verbatim so it
can be applied literally. `spec/core_spec.tex` is otherwise untouched.

**M22 — the `ordering` escape, made into a signature.** `ordering` leaves the normative clause and survives in the recap.

*Before* — the requirement as landed at `eddf6e9`:

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

*After:*

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event used as a key in an aleatoric region's \texttt{bounds}
  map \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

**This reproduces the measured escape** — the annex records a *partial referent deletion* that left the recap intact and passed, not this sentence replacement, so M22 is that escape's faithful reconstruction rather than a re-run of it. Its exact verdict is established during amendment 3's execution, not claimed here.

*Must fail:* test 3, **alone**.

**M23 — the symmetric `bounds` escape.** `bounds` leaves the clause and survives in the recap.

*Before* — the requirement as landed at `eddf6e9`:

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

*After:*

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

**Without M23 the narrowing is only half signed:** an implementation could scope `ordering` to the clause and keep `bounds` checked over the block, and M13 plus M22 would both still behave as pinned.

*Must fail:* test 3, **alone**.

**M24 — the `same region` escape.** the locality phrase leaves the clause and appears in the recap instead.

*Before* — the requirement as landed at `eddf6e9`:

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

*After:*

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the same region whose time model declares them.
\end{requirement}
```

Catches an implementation that clause-scopes the referents while still searching the whole block for the locality phrase.

*Must fail:* test 3, **alone**.

**M25 — the `\MUST{}` escape.** the normative force moves to the recap.

*Before* — the requirement as landed at `eddf6e9`:

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

*After:*

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  is an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map \MUST{} reach outside
  the region whose time model declares them.
\end{requirement}
```

**Its discrimination is not M24's, and the mechanism is worth stating.** The clause is *selected by* the `\MUST{}` position, so moving `\MUST{}` moves the clause onto the recap — which is exactly why 8.3's start rule takes the **last `". "` before** the occurrence, falling back after the label only when there is none. A correct implementation reads the recap alone as its clause, finds `ordering` and `bounds` there, and **fails on `same region`**, which the recap does not contain. An implementation that always started after the label would produce one slice running from label to recap, containing all four needles, and would pass.

*Must fail:* test 3, **alone**.

**M26 — the fallback anchor's own signature.** M22's clause replacement **plus** a period-free `ordering` decoy inserted before the unchanged label.

*Before* — the requirement as landed at `eddf6e9`:

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

*After:*

```latex
\begin{requirement}
  The ordering DAG is discussed below
  \label{req:time:aleatoric-reference-locality}
  Every event used as a key in an aleatoric region's \texttt{bounds}
  map \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

**This is the only mutation that tests the fallback itself.** The decoy carries no period, so it creates no `". "` boundary and the selector still falls back. A **correct** fallback starts after the label, excludes the decoy, finds no `ordering` in the clause and **fails**. A fallback anchored at the block start includes the decoy and **passes**. Nothing else in this set can tell those two implementations apart.

*Must fail:* test 3, **alone**.

**M27 — the exactly-one assertion's own signature.** The normative sentence is
left **unchanged**; the recap gains a second `\MUST{}`.

*Before* — the requirement as landed at `eddf6e9`:

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map may reach outside
  the region whose time model declares them.
\end{requirement}
```

*After:*

```latex
\begin{requirement}
  \label{req:time:aleatoric-reference-locality}
  Every event referenced by an aleatoric region's \texttt{ordering}
  DAG, and every event used as a key in its \texttt{bounds} map,
  \MUST{} be an event of that same region. Naming an event that does
  not exist is a dangling reference, governed by graph
  invariant~10; naming an event that exists in a \emph{different}
  region is a distinct defect, and this requirement is what forbids
  it. Neither the ordering DAG nor the bounds map \MUST{} reach outside
  the region whose time model declares them.
\end{requirement}
```

**This is the only mutation that reaches step 1 of 8.3.** A **correct**
implementation asserts exactly one `\MUST{}` in the block, finds two, and
**fails** before clause selection begins. An implementation that silently takes
the **first** occurrence selects the original normative sentence as its clause —
which still contains `ordering`, `bounds`, `same region` and `\MUST{}` — and
**passes every clause assertion**. Nothing in M22–M26 separates those two: each
of them presupposes that selection succeeded, and this one attacks whether
selection is permitted at all.

*Must fail:* test 3, **alone**.

**M11, M12, M13 and M16 are unchanged** and remain §3's. They delete or weaken
across the whole block; M22–M27 cover the narrower escapes they cannot see.

### 8.5 Execution surface and lifecycle

| Row | Path | Why |
|---|---|---|
| 4 | `crates/epiphany-testkit/tests/invariant_ten_surface.rs` | test 3's clause selection and four assertions |
| 6 | `spec/PASS13_CANDIDATES.md` | **append closes the finding recorded as open by `eddf6e9`** |
| 7 | `spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md` | §8's status transitions and historical marking |
| 8 | `spec/EVIDENCE_P13S26_EXECUTION.md` | M22–M27 transcripts, **appended**; the 38/38 matrix is preserved |

`spec/core_spec.tex` is **not** touched: the requirement's text is already
correct, and only the guard was weak.

**§8's status transitions, pinned** — otherwise this amendment lands still
calling itself a draft, and the ledger finding stays open, which is the defect
amendment 2 was written to close:

- **On ratification:** `STATUS: RATIFIED; DISPATCHED to clause-scope test 3.`
  *Prospective, because between ratification and execution the test is not yet
  clause-scoped and a perfect-tense literal would be false for that whole
  interval.*
- **On landing:** `STATUS: LANDED by this commit.` — no hash, same rule as pin 11.
- **On landing**, §8's revision records and this finding statement are marked a
  **dated historical record**, and the S26 ledger row is appended to record the
  finding **closed**.

**Gate 7 covers M22–M27** like any failing-evidence mutation: full workspace with
`--no-fail-fast`, radius compared against 8.4, transcripts appended to the annex.

### 8.6 Settled, and what the next review must decide

**Settled by review round 1 — the delimiter.** `". "` is sufficient for the
current pinned prose once the start is anchored as 8.3 requires. A future
abbreviation inside the clause would be a deliberate prose change; if it affects
a guarded phrase the test fails and forces guard review, which is the intended
behaviour. **No general TeX sentence parser is warranted here**, and building
one would add a component with its own failure modes to guard against a change
that announces itself.

**Settled by review round 1 — the same-sentence rule stands.** It deliberately
guards the *current* normative construction. A semantically valid multi-sentence
rewrite **should** fail and force deliberate guard review rather than pass
silently.

**Settled by review round 4 — every step of 8.3 is signed.** M22 and M23 sign
the referents' clause scope, M24 the locality phrase, M25 the last-period rule,
M26 the fallback anchor, and M27 the exactly-one-`\MUST{}` assertion — which was
step 1 and, until this round, the only step no mutation reached.

**Open: none.**

---

## §9. AMENDMENT 4 — §8'S CHARACTERIZATION OF M27

STATUS: LANDED by this commit.

> **DATED HISTORICAL RECORD — amendment 4 is ratified and executed. §9's
> findings, its taxonomy and §9.5's dispositions are an account of what was
> found and decided on 2026-08-11, and state no current condition.**

**Ratified 2026-08-11 at `a347a03` on the authority of the repository owner**,
the last review pass returning zero findings. Amendment 3 is ratified at
`7c8a30d` and executed at `bff7be0`; **this amendment does not reopen it**, changes no
selector rule, no mutation, no radius and no lifecycle transition. It corrects
**one explanatory sentence**, and nothing else. The replacement is executed, not
edited; a further defect is its own amendment with its own review round.

**§8 is untouched by this draft.** Its text is frozen and was executed.
Ratification **authorizes and freezes** this amendment; **execution applies the
replacement, and landing carries it.** The evidence annex already carries the
corrected characterization — that annex is amendment 3's own execution product
rather than ratified text — and needs no further edit, because its statement
that the correction was *open as of that commit* remains historically true.

### 9.1 The defect

§8.4's M27 entry closes:

> Nothing in M22–M26 separates those two: they all vary the *contents* of the
> clause, and this one varies which clause is chosen.

**That single sentence is false in both directions.**

**§8.6 is NOT affected and must not be touched.** It says only that M27 was
*"the only step no mutation reached"*, which is **true**. An earlier draft of
this amendment proposed rewriting it — that would have damaged an accurate
historical statement in frozen text to repair a different sentence's error.

- **M25 already varies which clause is chosen.** §8.4's own M25 entry says so:
  *"moving `\MUST{}` moves the clause onto the recap"*. That is exactly how M25
  discriminates the last-period rule from an unconditional fallback.
- **M27 varies no clause at all.** A correct implementation rejects the block on
  the exactly-one assertion **before selection begins**; there is no selected
  clause to differ about. The faulty implementation it catches is the one that
  proceeds to select anyway.

### 9.2 The correct characterization

M27 uniquely varies **whether clause selection is unambiguous, and therefore
permitted at all.** The full taxonomy, ruled at review:

| Mutation | What it varies |
|---|---|
| M22, M23, M24 | what the selected clause **contains** |
| M25 | **which sentence** is selected |
| M26 | the **fallback boundary** — starting after the label versus incorrectly widening before it |
| M27 | whether selection is **unambiguous, and therefore permitted at all** |

That distinction is the reason M27 exists: an assertion guarding the
*precondition* of selection cannot be exercised by any mutation that presupposes
selection succeeded.

### 9.3 The proposed replacement

In §8.4's M27 entry, replace the closing sentence with:

> Nothing in M22–M26 separates those two: each of them presupposes that
> selection succeeded, and this one attacks whether selection is permitted at
> all.

### 9.4 Execution surface

**Touch row 7 only — this contract. Amendment 4 is contract-only.**

No test, no `.tex`, no ledger row, and **no annex edit**: the annex's record
that the correction was open as of `bff7be0` **remains true**, and an append
would only add redundant lifecycle prose. *Rewriting* it would be the worse
error — replacing an accurate historical statement with a newer one — but that
is not what is being declined here; appending simply buys nothing.

**No behaviour, test, or normative semantics changes** — not "nothing
observable": the prose diff is itself observable, and 9.6 is what observes it.
This is P2 for that reason, and an amendment rather than a silent edit because
the sentence is frozen, and freezing is not conditional on a claim mattering.

**Status transitions**, on the pattern amendment 3 established:

- **On ratification:** `STATUS: RATIFIED; DISPATCHED to correct §8's M27 characterization.`
- **On landing:** `STATUS: LANDED by this commit.`, with §9's records marked a
  dated historical record **naming amendment 4**, so gate 9.6 item 4 can
  distinguish it from the contract's, §6's and §8's markers.

### 9.5 Settled

- **Completeness:** the single §8.4 replacement in 9.3 is the whole of it.
  **§8.6 must remain untouched** — it is accurate, and an earlier draft of this
  amendment proposed rewriting it, which would have damaged a true historical
  statement in frozen text to repair a different sentence's error.
- **Severity:** a P2 prose defect in ratified text **does** warrant an
  amendment. Frozen text is frozen regardless of the defect's severity.
- **Taxonomy:** 9.2's table is the ruled one, M26 signing the fallback
  *boundary* rather than an anchor.

**Open: none.**

### 9.6 Landing gate

A prose-only replacement has no test to sign it, so the observer is a gate. On
the staged tree, **all of the following**:

1. **Exactly one path staged**, `spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md`.
2. **Within §8.4's M27 entry** — the slice from `**M27 — the exactly-one
   assertion's own signature.**` to the next `**M` at that level — the old
   sentence is **absent** and the replacement is present **exactly once**.
   **Both assertions are scoped to that slice, and must not be run file-wide:**
   §9.1 quotes the old sentence and §9.3 quotes the replacement, so file-wide
   the old text still occurs once and the replacement occurs twice. A file-wide
   gate here is not merely loose — it is unsatisfiable by a correct execution.
3. **§8.6's sentence is unchanged**, verified with a **zero-context** diff —
   `git diff --cached -U0` must show it as neither an addition nor a deletion.
   *Zero context is required for the same reason pin 11's frozen-line check
   needs it: §8.4's replacement is close enough that at default context the
   untouched §8.6 line appears as ordinary context and a naive check would read
   it as touched.*
4. **Within §9's slice** — from `## §9. AMENDMENT 4` to end of file — the landed
   status literal `STATUS: LANDED by this commit.` is present, **and** a marker
   naming **amendment 4** specifically. **Scoping and specificity are both
   required:** the file already carries that same status literal for the
   contract itself, for §6 and for §8, and carries historical markers for each,
   so an unscoped check passes on lifecycle text §9 never wrote.
5. `git diff --cached --check` clean.
