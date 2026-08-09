# Contract — P13-S27: the reduction version gets an outside witness

**Status:** **RATIFIED 2026-08-08, on the authority of the repository owner,
after nineteen adversarial review rounds — the last of which returned zero
findings.** **DISPATCHED for execution.** Which rounds closed, what each found,
and the running tally are **the history table below**; this line does not restate
them, having gone stale in two consecutive rounds by doing so.

**THE PINS ARE FROZEN. They may be executed, not edited.** A defect found during
execution is **reported, not patched in place** — if it needs a pin change, that
is its own amendment with its own review round.

**IMPLEMENTED AND ACCEPTED 2026-08-09, by the repository owner after execution review
8.** The implementation was staged for the required independent review and is committed
with this acceptance record.

**How many post-ratification reviews have closed, which amendment each produced, and
what each found are THE HISTORY TABLE'S ROWS. This block does not restate them —
amendment 5.** It said *"**Three** independent reviews … amendments 1, 2 and 3 …
Amendment 3 changed **pin 3** … needs its own independent pass"* — written while
amendment 4 was being added three screens below, and stale before the commit landed.
**That is count-staleness by restatement again — the exact defect the history table was
built to end** — and the second occurrence inside the status block the author edits
every round. The history table's own preamble already states the rule: *read it off,
do not restate it.* No ordinal is given for how many times this has now happened,
deliberately: a tally of tally-staleness would be one more thing to keep current.

> **The invariant, stated so this block cannot go stale again: the MOST RECENT
> amendment always needs the next independent pass before completion is accepted.**
> **Which one that is, mechanically — LOCATOR FIXED BY AMENDMENT 7.** Read the **last
> `execution review` row** of the history table: the row **immediately above `Total`**.
> The amendment it produced carries the **same number** — review *N* produced amendment
> *N*, one per row, by construction. *(It read "the last row", and the literal last row
> is `Total`; the last review row names a **review**, not an amendment. The rule could
> not be followed to an answer.)* No amendment number appears in this paragraph,
> deliberately.
>
> **The amendment blocks below carry NO live status, and none may be added — amendment
> 7.** Each once ended with its own *"this amendment needs another independent pass"*,
> which made **six** standing instructions for a question that has exactly one answer,
> five of them already false. They are now dated records of what closed. **This
> paragraph is the only place in the document that says which pass is outstanding.**
>
> **TERMINATING CASE — added with the record of execution review 8.** The rule above
> assumed every review produces an amendment. **A clean review produces none**, so if
> the last `execution review` row shows **0 findings**, there is no amendment of that
> number and **no pass is outstanding**: the chain this invariant tracks has closed.
> **That is the state as of execution review 8.** What remains is not another pass — it
> is the **owner's acceptance decision**, which this document does not make on their
> behalf. *(Without this case the locator sends a reader looking for an "amendment 8"
> that does not exist — the same class of defect as the `Total`-row locator amendment 7
> fixed, and reachable only by a clean round, which is why neither earlier version
> caught it.)*
>
> **And the rule for WHICH figures in this document are allowed to look stale, because
> execution reviews 5 and 6 each had to rediscover it — amendment 6.** A figure recording
> **what a given round found or did** is *historical*: correct as of its dated block,
> and **never updated**. A figure inside a **general claim about the document's present
> state** is *live*, and must not be written at all — **wherever it sits, including
> inside an amendment block.** *"Amendment 3 returned six findings"* is historical and
> stays. *"The five amendments share one diagnosis"* is live, and was the defect
> execution review 6 found three lines below this invariant. **Location does not decide
> it; tense and scope do.**

**Every post-ratification amendment after the first was prompted by findings in the
previous amendment's text, OR in earlier text that a previous correction failed to
sweep** — never in the implementation. The pattern this contract's history table has
shown since round 3: *the amendment written to fix a round is where the next round's
defects are.*

> **The narrower claim — "in the PREVIOUS amendment's own text" — was FALSE, and the
> amendment that wrote it disproved it in the same breath. Corrected by amendment 6.**
> Amendment 5's own findings were a status block **amendment 3** left, a gate
> instruction **amendment 2** wrote, and three pieces of ratification-time text
> predating execution entirely — **not one of them written by the amendment
> immediately before.** The corrected form covers both routes, and they are the same
> failure either way: *state duplicated in a second place, updated in one.* Stated
> without enumerating which amendments, so that adding a row cannot falsify it — the
> paragraph that stood here listed them, and would have gone stale on the next one,
> three lines below the invariant forbidding exactly that.

**The implementation has changed ONCE since execution, and only once: amendment 1
corrected a false comment in `bundle.rs`.** *(Amendment 3 claimed it had "survived
all three reviews unchanged"; that was false, and amendment 4 corrects it.)* Every
other defect found since execution has been in this contract rather than in the 21
staged files — worth stating, because sustained amendment traffic can read as
instability in the work when it is instability in the document describing it. But
**"only a comment" is not a licence to infer a gate result**, which is what amendment
3 did.

**Gates 1–3 were therefore RE-RUN against the final staged tree, cold, on
2026-08-09** — `cargo clean -p` on all four touched crates first, so the result is
the artifact and not a cache. See amendment 4.

**Gate 4's earlier "clean" result is WITHDRAWN**, and the reason belongs at the
top: it was measured against §2's touch table alone, while §2 also carried an
**absence rule** that the same staged tree violated — a rule amendment 1
contradicted without noticing. **A file can satisfy the touch table and violate
the contract in the same commit**, because a touch table and an absence rule are
the same allowlist read from two directions and only one direction was checked.

**Gates 4 and 4a WERE re-run and re-reported against the contract as amended** —
under amendment 2 when 4a was created, and again under amendment 4. *(This read
"**must** be re-run" until amendment 5: an instruction left standing after it had been
carried out, so the top of the document demanded work the record below showed was
done. Instructions go stale the same way counts do.)*

**Round 1's ratification was WITHDRAWN**, and the distinction matters. It was
claimed on 2026-08-07 after a single round; round 2 then found four more blocking
defects against the supposedly frozen text, two introduced by round 1's own
amendments. **A ratification that a subsequent round falsifies that quickly was
not a ratification.** This one rests on a different footing: **19 rounds, 65
findings, 47 blocking, and a clean independent round at the end.**

**What ratification did NOT settle — the RATIFICATION-TIME record of 2026-08-08.
SUPERSEDED BY EXECUTION; marked, not deleted, by amendment 5.** Every bullet below was
true when written and **two of the three are now false**. They read as current until
amendment 5, in the block placed *"at the top so it is not missed"* — so the first
thing a reader met was a claim that nothing had run.

- ~~**M7's authority/base leg is unverifiable until this rung is implemented**~~ —
  `BundleCapabilities` and `CURRENT_REDUCTION_ALGORITHM_VERSION` are its own
  deliverables, so this was an **execution requirement**. **DISCHARGED at execution:**
  M7 ran, returning byte-equal images with the mismatched-version control rejected;
  three independent reviews have since examined it and returned nothing against it.
- ~~**Every gate, test and mutation is specified and none has been run**~~ — nineteen
  rounds went into the claim that they *can* be run and their results *would* be
  evidential. **They have now been run and reported.** Gates 1–3 were additionally
  **re-run cold** under amendment 4; gates 4, 4a and §2's rules A/B under amendments 2
  and 4.
- **SATISFIED as of execution review 8** — the last of the three to close. The execution
  report **and every amendment to it** were **subject to independent review before
  completion is accepted**, required by the owner at dispatch and specifically covering
  **M7's three observations and its control**. The document's quality came from the
  independent rounds; the report got the same treatment. **Which reviews closed is the
  history table below**, not a figure stated here, and whether any pass remains
  outstanding is **the invariant above**. **Acceptance itself is the owner's decision
  and is not recorded as satisfied by this bullet.**

### One narrow, explicit exception to "no execution work" — granted 2026-08-08

> **SUPERSEDED BY RATIFICATION.** This exception was granted while execution was
> prohibited outright; full execution is now authorised under §6. It is retained
> as the record of what the scratch probe was permitted to do and what it
> produced — **its result falsified round 10 and is cited throughout M7** — not as
> a live constraint. **The probe's standing is unchanged: evidence for M7's
> prerequisite, and not a demonstration of laundering, because it carried no base.**

**Granted before use, because the prohibition below is otherwise absolute.**
Authorised: **a bounded mechanical probe of M7's experiment**, on a **disposable
branch or worktree**, for the sole purpose of producing evidence for the next
review round.

**Scope — what is authorised:**

- **Only M7's mechanics**, and only the part runnable against the tree as it
  stands: **whether a base-free `Bundle` → text → `Bundle` round trip reproduces
  the original `image()` bytes.** That is M7's load-bearing assumption after
  round 10, and it is the thing four paper designs never established.
- The exact result and the **complete diff** are recorded, then the branch is
  **discarded**. Nothing merges.

**Scope — what is NOT authorised, and is not a judgement call:**

- **No S27 implementation.** No `BundleCapabilities`, no `capabilities()`, no pin
  3a validation, no `CanonicalBaseRequiresRebuild`, none of tests 1–10b.
- **No staging or commit on `main`** beyond this contract's own amendment rows.
- **No pin, test, gate, mutation or touch-table change** arising from the probe
  without its own review round. **The probe produces evidence, not amendments.**
- **No canonical base anywhere** — the live constraint (§1.2) is untouched, which
  is exactly why the probe is base-free.

**Why the probe cannot cover M7 in full, which is itself evidence for round 11.**
`BundleCapabilities` and `CURRENT_REDUCTION_ALGORITHM_VERSION` **do not exist in
the tree** — they are S27's own deliverables. M7 step 1 requires a base committed
**under the real authority**, so **M7 as written cannot be executed until S27 has
landed.** M7 is a mutation *of this rung's own implementation*, and mutations run
after the rung, not before it. **The probe therefore tests the round-trip
machinery M7 depends on, not M7.**

**A base-free probe removes no refusals at all** — both `project_text_document`
(`project.rs:580`) and `serialize_document` (`serialize.rs:151`) gate on
`canonical_base.is_some()`. So the probe touches none of pin 3b's guards and
cannot leave one unrestored.

**What the probe decides.** If base-free round-tripping is **not**
byte-identical, M7's whole-image comparison is unsound **regardless of bases**,
and round 10's fix is wrong too. If it **is**, the comparison design survives its
first contact with the code and only the base leg remains unverified — pending
S27.

### Probe RESULT — run 2026-08-08 on a discarded branch. It falsified round 10.

**Verdict: the round trip is byte-preserving, but ONLY from a fixed point — and
round 10's comparison did not compare from one.**

| Case | `original` already a fixed point? | Round 10's comparison | From the fixed point |
|---|---|---|---|
| minimal, seed 42 | **yes** | **equal** (1641 B) | equal |
| minimal, seed 99 | no | *not run* | equal (1800 B) |
| with one extension | no | **295 differing bytes from offset 352** | equal (1894 B) |

**Round 10's design compared `A` against a `B` built from the *input* document,
which is only valid when that document is already a fixed point of
`document_from_bundle ∘ serialize_document`.** `minimal_document(42)` happens to
be one — which is why the first probe passed and would have been reported as
success. **Two of three documents were not, and the extension case diverged by
295 bytes.**

**The non-idempotent field is `envelopes`, not extensions.** Diagnosed
field-by-field: `document_id`, `manifest_schema_version`, `lineage_id`,
`profiles`, `canonical_base`, `blobs` and **`extensions` — including every
`TextChunk` payload — all survive exactly.** `document_from_bundle` applies a
**canonical envelope ordering** (its own test is named
`document_from_bundle_reads_every_section_and_orders_envelopes_canonically`), so a
document whose envelopes arrive in any other order is not a fixed point, and its
operation-block bytes differ.

**`project_text_document` → `parse_document` is LOSSLESS**: `b_doc == d` held in
every case. The text leg was never the problem. **The defect was entirely in which
artifact round 10 chose as the reference.**

**What M7 must therefore add — for round 11 to ratify, not for this probe to
assume:** an explicit **fixed-point normalisation and assertion** before any byte
comparison — build `B` from `document_from_bundle(serialize_document(doc))`, and
**assert that document is a fixed point** — because otherwise a mismatch is round
10's own "third category", a deterministic setup difference, and unclassifiable.

**Probe hygiene, recorded because the discipline demands it:** the comparison was
**mutation-verified** — giving `A` a different `FileUuid` produced **20 differing
bytes at offsets 32–47 and 60–63**, observed, then restored by hand-editing.
(Incidentally confirming round 9's finding: `FixedHeader.file_uuid` is
byte-visible at offset 32, and round 8's enumeration had omitted it.) The probe
touched **one file, 142 insertions, all inside `#[cfg(test)]`**; removed **no**
refusal (`CanonicalBaseUnsupported` occurrences unchanged at 7 and 8); carried
**no** canonical base, so §1.2 was never engaged; and the branch was deleted. **The
diff was captured before deletion.**

> **The methodological point, stated plainly.** Four paper rounds refined this
> comparison and none found that it silently depended on an unstated
> precondition. **One execution found it in minutes, and found it via the case a
> reviewer would least likely hand-pick — a document with an extension.** M7's
> first probe *passed*; had the probe stopped at the case round 10 implied, the
> contract would have been ratified on a comparison that fails for most
> documents.

**What the probe settled, and what it structurally cannot — the division stands
as a standing prerequisite, not a to-do.**

| Leg of M7 | Status |
|---|---|
| **Round-trip machinery** — is the text path byte-preserving, and under what precondition? | **SETTLED by the probe.** Byte-preserving from a fixed point; the precondition is now steps 1a–1c |
| **Authority/base leg** — does a base validated under the real authority produce a container indistinguishable from a laundered one? | **NOT settled, and NOT pre-verifiable.** It requires `BundleCapabilities`, `capabilities()` and pin 3a's validation — **S27's own deliverables** |

**No review round and no further probe can close the second row.** M7 is a
mutation *of this rung's implementation*; mutations run after the rung. **It is
therefore an execution requirement, to be discharged after S27 is implemented and
before the rung reports** — carried in §7 as the observation M7 owes.

**The probe's standing is evidence for the prerequisite, not partial completion of
it.** It establishes that when the authority leg becomes runnable, the comparison
it runs will be sound — *provided* steps 1a–1c hold. **Do not cite the probe as
having demonstrated laundering. It demonstrated nothing about bases; it carried
none.**

> **The sentence that stood here — *"No execution work may begin — not
> implementation, not staging, not partial work against 'the settled pins'"* — was
> LIFTED at ratification and dispatch on 2026-08-08, and is retained as the record of
> the probe's authorisation boundary. Marked by amendment 5.** It sat in the *Probe
> RESULT* subsection, **three subsections past** the `SUPERSEDED BY RATIFICATION`
> marker that covers the exception itself, and was therefore the last thing a reader
> met before the history table: an absolute prohibition on the work the rest of the
> document reports as done. **A supersession marker governs the section it heads, not
> every later restatement of what it superseded** — which is the same fix-every-site
> rule this contract has recorded since round 1, applied to markers instead of counts.

**History — the running tally, which has now gone stale three times and been
restructured twice to stop it.** **Each row is an EVENT** — an amendment, a review
round, or the scratch probe. **There is no separate amendment count, deliberately**:
round 5 turned the review totals into a table and left the amendment tally as prose
immediately above it, which went stale in the same edit that fixed its neighbour. Read
the rows; do not restate a count.

> **"Every amendment is a row … the amendment count IS the number of rows" — CORRECTED
> with the record of execution review 8, and it was already false when written.** The
> converse does not hold: a row is not always an amendment. **Round 19 returned zero
> findings and produced no amendment**, so the identity was broken from the moment that
> row was added, and execution review 8 is the second such row. **A row showing 0
> findings produced no amendment**; `pin 10`'s row is an amendment whose findings
> predate the tally and shows `—`. Neither is an exception to be remembered — they are
> why the count is not stated.

| Amendment | Findings | Blocking | Independent review? |
|---|---|---|---|
| pin 10 — the unsatisfiable escape clause | — | — | no |
| round 1 | 9 | 4 | no — same agent as the author |
| round 2 | 6 | 4 | no — same agent as the author |
| round 3 | 6 | 4 | **yes** |
| round 4 | 5 | 4 | **yes** |
| round 5 | 4 | 2 | **yes** |
| round 6 | 3 | 3 | **yes** |
| round 7 | 3 | 3 | **yes** |
| round 8 | 2 | 2 | **yes** |
| round 9 | 2 | 2 | **yes** |
| round 10 | 1 | 1 | **yes** |
| *scratch probe* | *1 falsification* | — | *execution, not review* |
| round 11 | 3 | 2 | **yes** |
| round 12 | 2 | 2 | **yes** |
| round 13 | 1 | 1 | **yes** |
| round 14 | 1 | 1 | **yes** |
| round 15 | 1 | 1 | **yes** |
| round 16 | 4 | 4 | **yes** |
| round 17 | 10 | 5 | authored-side scan |
| round 18 | 2 | 2 | **yes** |
| **round 19** | **0** | **0** | **yes — first clean round** |
| execution review 1 | 5 | 3 | **yes — against the staged tree** |
| execution review 2 | 5 | 4 | **yes — 4 of 5; the fifth from the fix-every-site sweep** |
| execution review 3 | 6 | 5 | **yes — 3 of 6; three from the sweep, two of those in a pin** |
| execution review 4 | 3 | 3 | **yes — 2 of 3; the third from sweeping the adjacent row** |
| execution review 5 | 5 | 4 | **yes — 3 of 5; two from the sweep** |
| execution review 6 | 3 | 1 | **yes — 2 of 3; the third from the sweep** |
| execution review 7 | 3 | 3 | **yes — 2 of 3; the third from the sweep** |
| **execution review 8** | **0** | **0** | **yes — CLEAN; no amendment followed** |
| **Total** | **95** | **70** | one **event** per row; `0`-finding rows produced no amendment |

**This block previously read "amended three times … fifteen findings so far,
eight of them blocking"** — the round-2 figures, left standing through rounds 3
and 4 while the very tables recording those rounds sat below it. **That is the
count-staleness defect for the fifth time**, and this time in the status block
the author edited in every single round. It is now a table, so a new round adds
a row rather than requiring a number to be found and re-derived.

**Which rounds were independent is the table's "Independent review?" column, and
is not restated in prose.** A sentence here read "rounds 3, 4 and 5 were
independent" from round 5 until round 7, going stale the moment round 6 closed —
**the third consecutive round to find a claim duplicated in prose beside the table
that owns it.** Every independent round so far has found blocking defects in the
amendments written to fix its predecessor; that fact is read off the table, not
maintained separately.

**Review round 3 — 2026-08-07, independent, against `b842975`.** Confirmed
`b741e48` as status prose only, then returned **six findings, four blocking**.
**Every blocking finding was a defect in text rounds 1 and 2 wrote.**

| # | Finding | Disposition |
|---|---|---|
| 1 | **Pin 3a still carried the rationale round 2 retracted** — §0.4 says there is no in-tree production base writer, pin 3a said "§0.4 shows production code minting a stale document." The contract asserted a claim and its negation | Pin 3a's rationale rewritten onto the public-API footing. **Third occurrence of fix-one-site-leave-the-others** |
| 2 | **M5a had no observation mechanism.** Pin 3 required the capability be *stored*, nothing exposed it; `Bundle` has 17 public accessors and none for capabilities | **`Bundle::capabilities()` pinned** — new scope, flagged for round 4 |
| 3 | **M5b could not fail.** If the supplied capability and the base version both derive from the constant — the natural implementation — both operands move together. **This is §0.1's tautology reproduced inside the mutation built to detect it** | Base version must come from a source that does not track the authority: persisted artifact or deliberate literal, with both operands' provenance reported |
| 4 | **M6's replacement named a scenario with no test.** Test 6 stops at opening; nothing asserted that an unrelated commit succeeds, so an implementation rejecting every post-base commit passed tests 2/5/6/8 and the broadening had nothing to break | **Test 9 added** |
| 5 | Touch row 7 listed `generators.rs` as "call sites, real authority" — it has **zero** `Bundle::open`/`create` calls, and its `rng.range(0, 8)` versions are exactly the arbitrary wire values pin 3b assigns to *synthetic* capabilities | Split to **row 7a**, with its actual (conditional) change stated |
| 6 | §7 credited the call-site correction to round 1; rounds 1 **and** 2 are both load-bearing | Attribution fixed |

**The pattern across three rounds is now legible, and it is not about counts.**
Round 1 found stale text. Round 2 found unexecutable mutations. Round 3 found
that **three separate mutations were unrunnable in three different ways** — M5a
could not observe, M5b could not fail, M6 had nothing to break. Writing a
mutation is easy; establishing that it *can run* requires deriving its
observation, its failure condition, and the test it breaks, and none of the three
was done. §7 item 4a now demands all three.

**Review round 4 — 2026-08-07, independent, against `53292f6`.** Five findings,
four blocking. It judged pin 3's accessor **bounded** — the first piece of new
text any round has accepted — and the M5 pair defective again.

| # | Finding | Disposition |
|---|---|---|
| 1 | **M5b cited the wrong value.** `roundtrip.rs:367` is in `assert_score_serialization_stable` (`:332`) and versions an **acceleration snapshot**, not the canonical base. `assert_reduction_serialization_stable` has **no base at all** — pin 3c suspended it | Evidence corrected. **The tautology diagnosis stands; only its evidence was wrong** |
| 2 | **M5b left the instrument unchosen** — it said "the rung picks one" and named two, one of which does not exist for the nominated crate: `craft_image_with_base` is private to `epiphany-bundle`'s test module (`:1648`) | **Chosen**: commit-then-reopen through public API only |
| 3 | **M5b had no test that could assert the error fields.** `assert_reduction_serialization_stable` returns `()` and reopens with `.expect` (`:292`) — a mismatch panics and cannot match `CanonicalBaseRequiresRebuild { base, current }` | **Test 10b** added, named and returning a matchable `Result` |
| 4 | **M5a violated §7 item 4a — the rule round 3 added in the same edit.** It named no test, and its natural assertion (`capabilities() == CURRENT_REDUCTION_ALGORITHM_VERSION`) compares the constant with itself and cannot fail | **Test 10a** added; the comparison is now against a deliberate **literal** |
| — | Status prose said the pins were open to *round 3's* findings after round 3 closed | Now "the current round's" |

**Round 3's error is the one to carry.** It grepped `ReductionAlgorithmVersion`
across `testkit/src/`, saw a `roundtrip.rs` hit, and attributed it to the function
it was already thinking about **without resolving the enclosing item** — the same
shape as §0.4's `.commit(` miscount, which round 1 recorded as a lesson and round
3 then repeated. **Recording a defect is not the same as not committing it.**

**And both M5 halves failed the same way twice.** Round 3 diagnosed M5b's
tautology and wrote M5a with an identical tautology *in the same edit*, then
added §7 item 4a and immediately violated it. A rule written and broken in one
sitting is evidence the author is pattern-matching the finding rather than
applying it.

**Review round 5 — 2026-08-08, independent, against `df9e528`.** Four findings,
**two** blocking — the first round where blocking findings fell below four.

| # | Finding | Disposition |
|---|---|---|
| 1 | **The status history was numerically stale again** — "amended three times … fifteen findings so far, eight blocking" were round-2 figures, left standing through rounds 3 and 4 while the tables recording those rounds sat directly below | Replaced with a **table**, so a round adds a row instead of requiring a number to be re-derived. **Fifth occurrence of count-staleness**, this time in the block edited every round |
| 2 | **Test 10b could not make M5b's two-field assertion.** §3 said only "assert it opens"; under mutation that yields a bare `Err` or a panic, and a `#[test] -> Result` returning `Err` asserts nothing about that error's fields | Both `Result` arms pinned, plus a third for the wrong-error case |
| 3 | **M5b's "cannot be tidied" claim was false.** Keeping `synthetic_for_fixture` while passing the constant as both its argument and the base version preserves the fixture and fully restores the tautology | Retracted. The protection is **§7 item 4b**, not the structure |
| 4 | §3's preamble still said the tests were "in `epiphany-bundle`" after round 4 added two that cannot be | Corrected, with each test's touch-table home named |

**Finding 3 is the one to carry.** Round 4 asserted a *structural* guarantee that
did not hold, and in doing so undercut the *procedural* check actually doing the
work. That is the same error as reasoning that a mutation would fail instead of
running it: **a guarantee that has not been tested against the edit it is supposed
to prevent is a hope.**

**Review round 6 — 2026-08-08, independent, against `03c85dd`.** Three findings,
**all three blocking**, and **all three were in text round 5 wrote**.

| # | Finding | Disposition |
|---|---|---|
| 1 | **The amendment tally went stale in the block round 5 restructured to stop exactly that.** Round 5 turned the review totals into a table and left "amended five times … rounds 1–4" as prose immediately above it | The amendment count is now **the number of rows**. There is no separate figure to go stale |
| 2 | **§3's test-home correction was itself false.** Round 5 wrote "tests 1–9 in `epiphany-bundle`"; test 7 *is* `assert_reduction_serialization_stable`, which the same section names as `testkit/src/roundtrip.rs` | Replaced with a per-crate table. Two wrong versions of this sentence, both written while fixing it |
| 3 | **§7 item 4b protected one operand where test 10b has two.** Replacing **both** `synthetic_for_fixture(0)` and the base's `ReductionAlgorithmVersion(0)` with the constant preserves the synthetic call and restores the tautology — and **the `Err` arm never runs in the unmutated case, so its literal cannot detect it** | Item 4b now enumerates all **three** operands individually and requires each quoted verbatim |

**All three are the same defect wearing different clothes: a fix applied to the
site named rather than to every site the claim covers.** Round 5 fixed the review
tally and not the amendment tally beside it; corrected a test-home range without
checking each member; and retracted a structural guarantee while writing its
procedural replacement too narrowly to deliver what the retraction promised.
**This is now the sixth count-staleness defect in six rounds and the third
range-correction that did not check its own range.**

**The mechanism that keeps working is structural, not vigilant.** The review
totals stopped going stale when they became a table; the amendment count did not,
because it stayed prose. Item 4b stopped being under-specified when it became an
enumerated table. **Where a round is tempted to write a number or a range, it
should write a table instead** — that has now been demonstrated three times.

> **Demonstrated a fourth time, inside this very amendment.** Round 6's first
> draft of the table above ended its Total row with "**6 amendments**" — a
> free-standing count, written **three paragraphs after** the sentence declaring
> that the count is the number of rows and that no separate figure exists to go
> stale. It was already wrong: seven rows, not six. It was caught before commit
> and replaced with "one amendment per row", which is not a number at all.
>
> **This is the seventh instance, and it is the most instructive**, because the
> author had just written the rule, in the same file, in the same edit, and
> broke it anyway. The lesson is not "be careful with counts." It is that
> **prose invites a number and a table does not** — so the defence has to be the
> shape of the artifact, never the attention of whoever is editing it.

**Review round 7 — 2026-08-08, independent, against `c0d896c`.** Three findings,
all blocking. **All three were the same defect: a claim living in two places and
fixed in one.**

| # | Finding | Disposition |
|---|---|---|
| 1 | **§7 item 4a was unsatisfiable.** It required every mutation to name "the test it breaks", while item 1 four paragraphs above says M4 is observed to *compile* and M7's expected outcome is *success*. **A report obeying 4a literally could not be written** | 4a is now a table of what each mutation owes, with M4 and M7 carved out explicitly |
| 2 | **Round 6's three-literal correction reached §7 and not §3.** §3 still said "**both** literals … tidying **either**", so the contract carried the fixed and the broken version of the same claim | §3 no longer states the count. It points at item 4b, the single home |
| 3 | **"Rounds 3, 4 and 5 were independent"** went stale the moment round 6 closed, sitting in prose beside the table whose column already records it | Deleted. Read the table's column |

**Three rounds, one lesson, finally applied.** Round 5 fixed the review totals and
not the amendment tally beside them. Round 6 fixed item 4b and not §3's copy of
the same rule. Round 7 found the classification sentence duplicating the table's
own column. **The defect is duplication, and every previous remedy was vigilance —
"check the other sites too" — which has now failed three rounds running.**

**The remedy adopted here is deletion, not diligence:** where a claim had two
homes, one is removed and replaced with a pointer. §3 no longer counts the
literals; the history block no longer classifies the rounds. **A copy that cannot
drift is one that does not exist.**

**Review round 8 — 2026-08-08, independent, against `9829ae3`.** Two findings,
both blocking. **The first round to reach into a mutation's mechanics rather than
its bookkeeping.**

| # | Finding | Disposition |
|---|---|---|
| 1 | **M7 did not describe a runnable observation.** It said to *construct* a `TextDocument`, which **bypasses `parse_document` entirely** — so the parser refusal it ordered removed was irrelevant, and the demonstration was not the *import* laundering it is named for. `project_text_document` is the **export** direction (`&TextDocument -> Result<String,_>`) and is not on the path at all, so "all three sides, since removing one leaves the others refusing" was false for it. And "byte-indistinguishable from one whose base was genuinely validated" named **no comparison artifact and no comparison method** | Input must be **text, parsed**. *(Everything else round 8 specified here — which refusals are removed, the comparison artifact, the comparison method — was **superseded by rounds 9 and 10**. Read M7; this cell records what round 8 decided, not what the contract now says.)* |
| 2 | **The round-7 deduplication was incomplete** — the status block still carried "rounds 3 and 4 are closed" while declaring the history table the sole authority | Deleted |

**Finding 1 is the most substantive of any round**, because every earlier one was
about text agreeing with other text. This one is about whether the *experiment*
runs at all — and it did not. M7 has been in the contract since round 1 and
survived seven reviews, including three that specifically re-derived mutations,
because reading it never required tracing what calls what. **An observation stated
in the right register can look complete for a long time.**

**"Indistinguishable" was a conclusion, not an observation** — the exact failure
mode this rung exists to eliminate, sitting inside its own demonstration since
round 1.

**Review round 9 — 2026-08-08, independent, against `01e76d1`.** Two findings,
both blocking, **both in M7's comparator — the text round 8 had just rewritten.**

| # | Finding | Disposition |
|---|---|---|
| 1 | **Test 10b is not a "genuinely validated" reference.** Its write-side capability is `synthetic_for_fixture(0)`; only its *reopen* uses the real authority. M7 would have compared one synthetic fixture against another, with the validated half of the claim simply absent | M7 now **builds its own reference**: commit a base under `caps` derived from the real constant, so pin 3a validates it on the way in. **The crate this cell named — `epiphany-testkit` — was SUPERSEDED IN ROUND 12**, which found `render_text_document` is `pub(crate)` to `epiphany-textproj` and pinned the harness **there**. Read M7 |
| 2 | **The field enumeration could not support its conclusion.** It claimed "everything that could carry provenance" while omitting `FixedHeader.file_uuid` — **the field it required to match** — plus the superblock's `generation`, `manifest_offset`, `manifest_length`, `manifest_hash`, and the manifest outside `canonical_base` | Replaced with **whole-`image()` byte comparison**. **The clause "with any difference enumerated and classified rather than assumed" was SUPERSEDED IN ROUND 10**, which found a byte difference would have a *third* possible cause outside both permitted classifications — making the classification unsound and the comparison meaningless. Round 10 replaced it with an inherited-alignment round trip; rounds 11, 13 and 14 then added `B_fixed`, the control, and necessary-but-insufficient equality. Read M7 |

> **Both cells above carried superseded dispositions with no marker — found by
> amendment 4, one from the review and one from sweeping the adjacent row.** The
> convention exists: round 8's cell says outright that it *"records what round 8
> decided, not what the contract now says."* Round 9's two cells never got it, and
> read as current for ten rounds. **A disposition column is a history, and history
> without supersession markers reads as instruction** — which is how a reader arrives
> at M7 expecting a `epiphany-testkit` harness that two rounds ruled cannot exist
> there.

**Finding 1 is a collision between two of this contract's own designs, not a
typo.** Round 4 made test 10b synthetic-on-write **deliberately**, so M5b's two
operands would be provably independent — and that is precisely what disqualifies
it as a validated reference. **One artifact cannot be both independent of the real
authority and committed under it.** Round 8 reused a fixture by name without
re-reading what it had been built to be, which is a failure mode no amount of
care about *wording* would have caught.

**Finding 2 retires a technique, not just an instance.** A hand-written list of
"every field" is a claim about a struct's contents that is wrong the moment the
struct changes — and this one was wrong the day it was written, omitting the very
field it depended on. **Comparing the whole artifact cannot be incomplete.** That
is the tables-over-numbers lesson applied to the experiment rather than the prose:
*let the artifact defend itself instead of enumerating it correctly.*

**Three further sites were found by the author while amending** — §7 item 6 still
said "M7's three text refusals" (surviving round 8's correction of that exact
count in two other places), §7 item 4a's M7 row still named the superseded method,
and round 8's own disposition cell stated it as current. All now point at M7
rather than restating it.

**Review round 10 — 2026-08-08, independent, against `0efd543`.** **One finding,
blocking** — the smallest round yet, and again in M7.

| # | Finding | Disposition |
|---|---|---|
| 1 | **The whole-image comparison had no complete construction alignment.** Round 9 listed four things to align; `serialize_document` also fixes `document_id`, `lineage_id`, `profile_declarations`, every extension field and preserved chunk, envelope payloads, **staging order**, manifest `major` and `epoch_max`, and every chunk ref/hash/offset derived from them. **A byte difference would therefore have had a third possible cause — "the reference was built differently" — which is neither permitted classification, making the result unclassifiable and the comparison meaningless** | M7 is now a **round trip**: `B` validated under the real authority → exported to text → parsed → re-serialized as `A` → images compared. **Alignment is inherited, not enumerated** |

**This is the third hand-enumerated "complete set" in this contract, and the
third to be wrong on the day it was written** — "every field that could carry
provenance" (round 8), "every field to align" (round 9), and now round 9's
alignment list again. **The rule earned across rounds 5–10 is one rule:** where a
claim requires completeness, **do not enumerate — derive.** Tables instead of
counts, whole artifacts instead of field lists, and now a single shared origin
instead of an alignment list.

**Deriving `A` from `B` eliminates the setup-mismatch category by construction
rather than by care**, which is the only reason whole-image equality can mean
anything. It also makes M7 the *realistic* threat: export a validated document to
text, re-import it, and observe that the re-imported container is
indistinguishable from the original — having validated only the base's number,
never its provenance.

**Two further sites were caught by the author while amending:** the "restore the
two removed refusals" instruction, whose count round 10's restructure invalidated
for the third time (hence no count is stated now), and round 8's disposition cell
still reading as current.

**Review round 11 — 2026-08-08, independent, against `39f2617` (post-probe).**
Three findings, **two blocking**. It confirmed the probe contained and its
fixed-point result decisive, and **kept M7 blocked.**

| # | Finding | Disposition |
|---|---|---|
| 1 | **M7 still lacked a distinct normalised reference.** Round 10 named one artifact; the comparison needs `B_raw` **and** `B_fixed`, with an explicit convergence loop and a hard fixed-point assertion, and the comparand must never be `B_raw` — otherwise an envelope-order normalisation difference stays **indistinguishable from a provenance result** | Steps **1a–1c** added; step 5 forbids `B_raw` as a comparand |
| 2 | **The claim was stated more broadly than any observation supports.** M7 read as though every direct bundle is byte-identical to its re-imported form; it is not, and the probe measured 295 differing bytes proving so | New scope section: it proves the text path carries **no provenance marker after normalisation**, and explicitly not the pre-normalisation claim. **Both sentences must appear in the report** |
| 3 | *(clarification, not a defect)* The probe cannot pre-verify M7's authority/base leg; that stays an execution requirement after S27, with the probe as evidence for the prerequisite | Recorded as a **standing prerequisite table**, with the explicit instruction not to cite the probe as having demonstrated laundering |

**Finding 2 is the one with consequences beyond M7.** M7's conclusion is the sole
evidence for a **permanent** capability loss — the text refusal that moved
`COMPANION_VERSION` to 0.14.0 and took the corpus's `canonical_bases` from 2 to 0.
**Justifying a permanent refusal from a claim broader than the result obtained is
the same error as concluding instead of observing**, one level up: not a false
observation, but a true one asked to carry more than it can.

**Review round 12 — 2026-08-08, independent, against `74dc994`.** Two findings,
both blocking. **Both are the same defect: a requirement stated without the
decision it requires**, leaving execution to make a design choice silently.

| # | Finding | Disposition |
|---|---|---|
| 1 | **The convergence loop was not actually bounded.** It said "bound the loop and fail if it does not converge" and named **no limit**, so execution would choose when non-convergence becomes failure — changing what the experiment means | Bound **pinned at one normalising step** (`n = 1`, computing at most `B₁` and `B₂`), with an outcome table and hard failure on `B₂ != B₁` |
| 2 | **M7's location was unchosen.** "In a crate that can reach the real constant" is true of two crates and decisive for neither — and `render_text_document` is `pub(crate)` to `epiphany-textproj`, so `epiphany-testkit` could only host M7 via **an unpinned visibility change to another crate's public API** | Harness **pinned to `epiphany-textproj`**, under existing touch row 9. `render_text_document` stays `pub(crate)` |

**The bound is one step because that is a property, not a tolerance.**
`document_from_bundle` canonicalises, so `serialize_document ∘
document_from_bundle` must reach its canonical form in one application. **If it
does not, there is no canonical form and M7 is invalid as a whole** — so `B₂ !=
B₁` is a reportable finding about the projection, not a signal to iterate again.
A loop that runs until it happens to settle tests nothing; it reports how long it
took.

**Finding 2 would have been discovered by execution as a wall, not a decision** —
and the natural improvisation is to widen `render_text_document`. Handoff §1.3
records that function as *the one intentional hole* in the text refusal, existing
solely so a negative vector can carry the spelling it asserts is refused.
**Widening it to host a mutation that gets reverted would leave a permanently
widened public surface behind**, which is how a temporary harness becomes an API
change nobody ratified.

**Review round 13 — 2026-08-08, independent, against `bff9c9a`.** **One finding,
blocking** — and it inverted M7's result.

| # | Finding | Disposition |
|---|---|---|
| 1 | **M7 claimed the capability check "does not fire".** Pin 3a validates a **newly emitted** canonical base, which is exactly what **both** `B_raw` and `A` commit. The check fires on both paths and **accepts**, because the raw version equals the real authority. **As written, M7 was satisfiable by deleting pin 3a's writer check entirely** — a passing M7 demonstrating the opposite of its purpose | Claim corrected to *fires and accepts*; **three required observations**; and a **control** added — the same path with a mismatched version must be **rejected** with `CanonicalBaseRequiresRebuild` |

**This is a new failure shape, and worth naming: an observation satisfiable by the
absence of the thing it observes.** M7's earlier defects were about being
unrunnable, or comparing the wrong artifacts. This one would have *run*, *passed*,
and *reported success* — on a tree where the writer check had been removed.
**"The check does not fire" cannot distinguish a check that accepts from a check
that is not there**, and only one of those is the finding.

**The control is what makes the positive result mean anything.** The matching case
succeeding is evidence only once the mismatching case is seen to fail on the same
path, in the same run, under the same removals. **M7's removals are now explicitly
limited to the text refusals** — pin 3a is not among them and may not be weakened,
because it is the thing under observation rather than an obstacle to it.

**Review round 14 — 2026-08-08, independent, against `f579172`.** **One finding,
blocking** — a contradiction round 13 created.

| # | Finding | Disposition |
|---|---|---|
| 1 | **The comparison method still said equal images "complete the observation and require nothing further"** — written in round 9 when byte equality *was* all of M7, and not swept when round 13 added the control. The contract simultaneously **required** the control and **licensed omitting it**, with the permissive sentence sitting **earlier**, reading as the summary | Equality is now **necessary but not sufficient** — observation 1 of three, with the control still required. The paragraph specifies *how to compare*, never *what suffices* |

**A second instance was found while amending, and round 14 reported none.** The
"informative in both directions" note read *"if **every field matches**, the
refusal is justified"* — the **same sufficiency claim in different words**, and
still carrying round 8's *"every field"* vocabulary that round 9 had replaced with
whole-image comparison. **A search for "nothing further" or "sufficient" cannot
reach a sentence that says "matches".** That is exactly the defect `CLAUDE.md`
names — *searching one spelling and concluding about all sites* — encountered
inside the fix for a sweep failure. **Neither the reviewer's search nor the
author's first search found it; a third pass on different terms did.**

**The round-13 lesson generalises further than round 13 stated.** It is not only
that a requirement must be swept to every site — it is that **the permissive
statement usually reads earlier than the restrictive one**, because requirements
accumulate downward as a document is amended. **A reader following the document in
order stops at the first sentence that says "done".** Where a later round narrows
what suffices, the earlier summary is the site most likely to contradict it and
least likely to be searched.

**Review round 15 — 2026-08-08, independent, against `fa483cf`.** **One finding,
blocking** — and it ran the scan rounds 13 and 14 left outstanding.

| # | Finding | Disposition |
|---|---|---|
| 1 | **M6 accepted "test 5 fails" and "test 9 fails" as its observations.** A test fails for **every** reason, not only the one under test — an unrelated writer rejection satisfies both exactly as well as the intended cause, so M6 could report success while demonstrating nothing about pin 3a's scope | Both halves now require the **mutated outcome itself**: test 5's stale commit observed to **succeed** and reopen at the new generation; test 9's unchanged inherited-base commit observed **rejected specifically by the broadened rule** |

**Historical disposition — FALSIFIED IN ROUND 16.** Round 15 reported that the
scan was complete and that M1–M5b survived it while M6 did not. Round 16
re-derived those entries and found M1–M3 and M5a still accepting a broken
assertion as evidence. The text is retained as the round-15 disposition, not as a
current conclusion.

**The principle, stated once so it need not be rediscovered:** *the evidence a
mutation owes is the behaviour it changed, not the assertion it broke.* A broken
assertion is a symptom with many possible causes; the changed behaviour has one.
**Every mutation in §4 now names an outcome, not a failure.**

**Review round 16 — 2026-08-08, independent, against `a230c6f`.** **Four
findings, all blocking.** Round 15 applied its rule to M6 only, while its own
new rule claimed to cover every mutation.

| # | Finding | Disposition |
|---|---|---|
| 1 | **M1 still accepted test 2 failing.** An unrelated open error could break the assertion after the pin-5 comparison was removed | Require the stale, self-consistent base to be observed **opening successfully** under the mismatching capability |
| 2 | **M2 still accepted test 3 failing.** Any non-malformed result could do that, without showing that corruption was reclassified as staleness | Require the corrupt fixture to be observed returning `CanonicalBaseRequiresRebuild`, with both fields reported |
| 3 | **M3 still accepted test 4 failing.** Any error on either no-base open could satisfy it | Require the base-free fixture to be observed rejected by the wrongly widened check under the deliberately mismatching capability, with the error fields reported |
| 4 | **M5a still accepted test 10a failing.** A serialization failure would satisfy it without showing the production path read the changed authority | Require the returned bundle's stored capability to be observed equal to the deliberately changed authority |

**M4 and M5b survive this correction.** M4's changed behaviour is compilation;
M5b already requires its specific error and both fields. M6 and M7 already name
their changed outcomes. The round-15 claim that the scan was complete is retained
above as the review's historical disposition, not silently rewritten.

**The rule now has its actual scope:** for each mutation, report the intended
changed behaviour itself; the named test's broken assertion is supporting
evidence, never sufficient evidence. **The contract remains NOT RATIFIED, NOT
DISPATCHABLE, pins open, and no execution work is authorised.**

**Convergence assessment, stated against interest.** The history table is the
authority for the sequence and totals. **No review round has returned zero.**
Round 16 also falsified round 15's claim that its scan was complete, so no
ratification inference may be drawn from the recent smaller rounds alone.

**Round 17 — 2026-08-08, the §3/§5 sweep. Ten findings, five blocking.** Round
13's question had been asked of §4 only. This applied it to the two sections that
had never had it, and **both yielded on first contact.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **Gate 6's derive alternative can never match.** Verified by running the exact regex: `grep` is line-oriented, so `[[:space:]]*` cannot cross the newline rustfmt puts between `#[derive(…, Default)]` and `pub struct BundleCapabilities`. **The likelier violation returns 0 matches and the gate passes** — and gate 6 is the *sole* mechanical guard on pin 3's prohibition | Replaced with three checks, one of which (**quote the definition verbatim**) cannot pass vacuously |
| **2** | **Gate 6a is vacuous under a rename.** Pin 3b offered `synthetic_for_fixture` as an example ("e.g."); the gate greps for that exact literal | **Name pinned** in pin 3b; gate 6a must confirm the actual name before trusting its zero |
| **3** | **Gates 2 and 3 named no toolchain**, in a repo whose CI comment records 1.95/1.97 lint divergence and whose default `stable` is 1.97.1 | Both pinned to `cargo +1.95.0`; toolchain reported with the result |
| **4** | **Gate 4's "staged list exactly §2" is unsatisfiable** — row 12 is conditional and 7a may not change | Subset both ways: every staged path in §2, every §2 row staged **or** named unused with its reason |
| **5** | **Tests 1, 6, 7, 8, 9 can all pass on a base-free bundle.** Pin 5 makes base-free the permissive case, so a fixture that silently loses its base makes each pass trivially — **test 1 degenerates into test 4** | One rule stated once: each MUST assert `canonical_base.is_some()` on the bundle under test |
| 6 | Tests 1 and 6 asserted the same outcome with no stated distinction | Kept distinct, with construction routes pinned — hand-built vs commit-path. **The probe showed construction changes bytes**, so two routes is coverage, not redundancy. If they collapse in practice, that is a finding |
| 7 | Test 4's "two different `caps` values" never asserted distinct | Inequality asserted, and the base-free precondition made explicit |
| 8 | Gate 1 required "full pass" but not **0 ignored** — an `#[ignore]`d test satisfies it while never running | Ignored count reported, must be 0 against the measured 1570/0/0 baseline |
| 9 | Gate 7 gave **no method** for "no schema major/minor moved" | Method pinned: quote the constants and show the diff empty |
| 10 | Gate 5 named no dependency tables | All three quoted — a dev-dependency would create the cycle pin 1 forbids |

**The unifying defect is that a gate proving *absence* is only as strong as the
string it searches for.** Findings 1, 2 and 9 are three faces of it: a regex that
cannot match, a name that was an example, and a clause with no method at all.
**Every one reported success while checking nothing.** The remedy applied
throughout is the same as §4's: **require an artifact to be quoted and read, not
a pattern to be matched** — a definition, a table, a command's output.

**Findings 5–7 are §3's version of the same thing:** a test asserting a
*permissive* outcome passes when its fixture degenerates into the permissive
case. That is not hypothetical — base-bearing fixtures have been the awkward ones
to build for this whole interval, so degenerating is the path of least resistance.

**This round was authored-side, not independent**, and it is recorded as such in
the table. **It found more than any round since the first**, in the two sections
nobody had scanned. **There is now no unscanned section: §0 through §7 have all
been swept or amended.** That is the first time that has been true — and it is a
statement about coverage, **not** a claim that the sections are clean. **Treat
"dispatchable" as a claim requiring evidence of convergence, not a status reached
by running out of findings.**

**Review round 18 — 2026-08-08, independent, against the uncommitted round-17
sweep.** **Two findings, both blocking, both in round 17's new §3 text.**

| # | Finding | Disposition |
|---|---|---|
| 1 | **The base-presence rule demanded the opposite of what test 8 is for.** It required `is_some()` before the commit for tests 8 and 9 alike — but **test 8 introduces the base**, so it must start `is_none()`. Requiring otherwise makes it unsatisfiable, or satisfiable by a fixture that already has a base, in which case the commit introduces nothing and the test asserts nothing | Rule split into a per-test table: 1/6/7 `is_some()` before; **8 `is_none()` before, `is_some()` after**; 9 `is_some()` both, base unchanged |
| 2 | **Test 6's construction was self-contradictory.** Round 17 assigned it the **commit** path while also requiring its fixture to "arrive the way" its ancestor's did — and that ancestor, `opening_a_major_1_bundle_that_already_carries_a_base_is_refused` (`bundle.rs:1866`), **hand-builds** via `craft_image_with_base` at `:1869` | **Routes swapped**: test 6 hand-built, test 1 commit-path. That makes the attribution **true** instead of deleting it |

**Finding 1 came from grouping by mechanism instead of by purpose.** Tests 8 and
9 were bracketed together as "the ones that commit" — which is true and
irrelevant. **Test 8 commits a base into a bundle that has none; test 9 commits
something unrelated to a base already there.** They are opposites that share a
verb, and a rule written from the verb inverted one of them.

**Finding 2 is the round-17 sweep's own version of the citation defect this
contract keeps producing:** an attribution asserted without opening the file it
attributes to. Reading `:1866` takes one command, and it says
`craft_image_with_base` in plain sight.

**Both were introduced by round 17 and neither pre-existed it** — the authored-side
sweep bought coverage of two unscanned sections at the cost of two new defects in
what it wrote. That is the trade the history table now shows for every large
amendment.

**Post-ratification amendment 1 — 2026-08-09, on the first independent review of
the STAGED EXECUTION.** Five findings, **three blocking**. The pins were frozen at
ratification, so these are an amendment with its own review round rather than
patches — which is the discipline working as designed: **execution reported, and
review of the report found what nineteen paper rounds had not.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **§0.4 never counted `Bundle::create_versioned`** — a third public constructor whose signature this rung changes, 3 sites, **zero mentions across nineteen rounds**. §0.4 searched `open(` and `create(` and concluded about "the writer surface" | Surface added to §0.4 and recorded as its **fourth instrument failure** — a count taken from one spelling, the same shape as the first three |
| **2** | **Gate 4 correctly failed: four required files were in no touch row**, and the staged patch alone cannot build. `bundle/src/lib.rs` (the re-export pin 3 makes unavoidable), `gminor.rs`, and the two crate-root helpers | **Touch rows 13–16** added |
| **3** | **`production_caps()` was unpinned design scope**, not merely a missed path — and in `epiphany-testkit` it landed as new **public** API | **Ruled** under pin 3b: sanctioned, name pinned, visibility pinned per crate — `pub(crate)` in the production crate, `pub` in the test-support crate whose tests and benches are external consumers. The direct-construction alternative is recorded as considered and rejected |
| 4 | **Row 12 named one counter; a label addition necessarily moves three** — core requirements, suite requirements, suite labels | Row 12 amended to name all three, with the line numbers |
| 5 | **The new writer comment was false.** It said `self.manifest.canonical_base` "is always `None` here", but **test 9 deliberately performs an unrelated second commit on an inherited `Some` base** | Corrected in code. The contract itself never made this claim — §7 item 10 says "stale *inherited* base", which is exactly right |

**Finding 5 is the one to carry, because it is a false rationale attached to
correct behaviour.** The check does the right thing; the comment justified it with
a reachability claim that a test in the same rung disproves. **What actually holds
is narrower:** an inherited base *can* be present, but never *stale* — `open`
refuses a stale one and `create` refuses a base-bearing manifest. So **narrowing**
pin 3a to "any stale inherited base" is unobservable, while **broadening** it to
"any base-bearing commit" is very observable and wrong. The scope is forced on one
axis and a real choice on the other, and the comment collapsed the two.

**Finding 1 is the fourth instrument failure in one section**, and its cost was
finding 2: `gminor.rs` calls *only* `create_versioned`, so no surface count ever
reached it, so no touch row existed, so gate 4 failed. **The allowlist caught what
the count missed** — which is what an allowlist is for.

**Review confirmed M7 internally coherent against the final tree**: `B_raw`
requiring one normalisation step matches the probe history, and the mismatched-base
control reaches the staged writer check and produces the required error. **No
additional M7 contradiction found.**

**Status when written (CLOSED):** the staged implementation was not accepted, and this
amendment needed another independent pass. **That pass closed as execution review 2.**

**Post-ratification amendment 2 — 2026-08-09, on the independent review of
amendment 1.** Five findings, **four blocking**. **Four came from the review; the
fifth from sweeping every site that restated the first finding** — and every one of
the five is a defect in *amendment 1's own text*, not in the implementation
amendment 1 ruled on.

| # | Finding | Disposition |
|---|---|---|
| **1** | **Touch row 15 contradicted the inherited-obligation boundary.** Row 15 stages `textproj/src/lib.rs`; the ruling under inherited obligation 2 and §2's absence sentence both say that exact file is deliberately absent and that staging it **makes gate 4 fail**. So the staged tree failed the contract as amended — while the execution report called gate 4 clean, having checked staging against §2's **table** and not against the absence rule four lines below it | The absence sentence **split into the two distinct rules it conflated**; `lib.rs` removed from the list with the reason it was on it preserved; and **gate 4a added** to replace what its absence was actually guarding |
| **2** | **The rejected alternative's site count was false** — "23 sites" where there are **39** production-authority invocations. The figure is load-bearing: it *is* the argument that direct construction repeats itself intolerably | Corrected to 39, and restated as a **derivation from §0.4's surface** rather than an independent count, with the likeliest origin of "23" recorded |
| **3** | **Touch row 13 reproduced the omission amendment 1 was written to close.** Its "92 converted sites" is 60 `open` + 32 `create` — the two-constructor count, missing `create_versioned`'s 3. The complete pre-S27 surface is **95** | The count **removed from the row's rationale entirely**, which needs only "the two crates outside `epiphany-bundle`"; 95 recorded with §0.4 as its single origin |
| **4** | **§7 item 5 still had the report reconcile against `open` 60 + `create` 32.** *(Found by the sweep, not by the review.)* Amendment 1 added the third constructor to §0.4 and to touch rows 13–16 and left this item behind, so **a report obeying item 5 literally would re-derive the incomplete surface the amendment existed to complete, and call it a match** | `create_versioned` **3** and the total **95** added; and the figures marked **pre-S27**, since S27's own new tests move the tree to 66 / 33 |
| 5 | **The helper touch-row reference was off by one** — "rows 13 and 14", where row 13 is the bundle re-export and the two helper roots are **14 and 15** | Corrected, with the superseded reading noted |

**Finding 1 has a lesson past its own fix, and it is a gate-design lesson.** Gate 4
already requires the staged list to be a subset of §2 **in both directions** — every
staged path in §2, every §2 row staged or named unused. But *"appears in §2"* and
*"is not forbidden by §2"* are different questions, and only the first was ever
asked. §2 carries prohibitions as well as permissions, and nothing checked the
prohibitions. **The allowlist was read as a whitelist and §2 is not only a
whitelist.**

**Finding 1's third site is the one worth carrying.** The review cited two of the
three places the contradiction lived; the third — §7 item 6, which required the
report to *confirm* that file's absence — surfaced only by grepping every mention of
the filename. **This contract has recorded "a correction that does not sweep every
site restating the thing corrected has not been made" since round 1** (see pin 3a's
note, which counted three occurrences before round 7 added another). Amendment 2 is
the first time that discipline caught something an *independent* round missed, which
is the argument for keeping it even after a clean round.

**Findings 2, 3 and 4 are count defects inside an amendment whose own subject was a
count defect**, and 3 and 4 reproduce the *exact* omission it was written to close —
twice, in two different sections. That is this document's strongest evidence for its
own doctrine: **where a claim requires completeness, do not enumerate — derive.**
Amendment 2 therefore **deletes** one count outright and converts the others into
derivations with their inputs named, rather than restating them more carefully.
Restating carefully is what rounds 1 through 7 tried.

**What this review did NOT find, recorded so the next pass knows what remains
untested by it:** it confirmed the **code-side** corrections sound — `production_caps`
`pub(crate)` in `epiphany-textproj` and `pub` in `epiphany-testkit`, the writer
comment now distinguishing an inherited base from an inherited *stale* base — and
staging clean across all 21 modified paths. **It returned nothing against M7, nor
against any test or mutation observation.** Two consecutive independent reviews have
now examined M7 and found it internally coherent.

**Status when written (CLOSED):** amendment 2 changed pins, so it needed its own
independent pass — **that pass closed as execution review 3** — and it ordered gates 4
and 4a re-run and re-reported, the prior gate-4 result having been withdrawn. **Those
re-runs were carried out**, under this amendment and again under amendment 4.

**Post-ratification amendment 3 — 2026-08-09, on the independent review of
amendment 2.** Six findings, **five blocking**. **Three came from the review, three
from sweeping each finding's other sites — and two of the three sweep findings were
in a PIN.** Again every finding is a defect in the previous amendment's text or in
text it should have swept, and none is in the implementation.

| # | Finding | Disposition |
|---|---|---|
| **1** | **The 39-site derivation did not derive from its cited source.** It claimed §0.4's surface and used `open` 19 + 2 / `create` 15 + 3 — the **post-S27 staged** counts. §0.4's table gives 18 + 2 and 14 + 3, and **§0.4 alone cannot yield 39**, because two of the sites are S27's own | Rewritten as a three-leg table naming the **pre/post-S27 transition** and attributing both added sites to **test 10b** (`roundtrip.rs:897`, `:928`). The result 39 was already correct; the derivation was not |
| **2** | **The "Rung type" paragraph still named two constructors at 60 and 32 sites** — the **fourth** live site of the third-constructor omission, outside the three amendments 1 and 2 corrected | `create_versioned` named, and **the figures removed from that paragraph rather than corrected** — §0.4 is their single origin. The paragraph has now been left behind by a §0.4 correction **twice** |
| **3** | **Pin 3 said the capability is "required at both constructors"** and named only `open` and `create`. *(Sweep.)* **Fifth site of the same omission, and the first inside a pin** — pins are what execution reads | Corrected to **all three**, with the five recorded sites listed so the next amendment cannot declare the omission closed a third time |
| **4** | **Pin 3 carried "their 57 call sites are unchanged"** — a figure **review round 1 withdrew from §0.4** on finding its basis was a `.commit(` token grep counting `epiphany-editor-core`'s same-named method. §0.4 states no number; this pin kept the withdrawn one through nineteen rounds and two amendments. *(Sweep.)* It is also unreconstructable: Bundle-typed commit sites are **58** at `381c498`, **60** at `96b40b2`, **60** pre-S27, **65** staged — **57 is none of them** | The number **removed, not replaced**, matching what round 1 did to §0.4 |
| **5** | **The synthetic site was attributed to M7.** It is **test 10b's** fixture, and round 9's finding is precisely that test 10b is *not* a validated reference M7 can use — which is why round 12 pinned M7's harness to `epiphany-textproj`. *(Sweep.)* **No M7 artifact exists in the staged tree at all** | Re-attributed, with both rulings named at the site |
| 6 | **Status said amendment 2 changed "two touch rows"**; its diff changes **one** table row — row 13. Rows 14–15 were amendment 1's and appear only as context | Corrected |

**Findings 3 and 4 are the ones that change how this contract should be read.** Both
are in **pin 3** — the pin execution reads to build the type — and both were found
by sweeping, not by any of three independent reviews. **Findings 2, 3 and 4 also
share one property: each was a site that a *previous* correction should have swept
and did not.** Round 1 corrected §0.4's commit figure and left pin 3's copy; round 1
and round 2 corrected the rung-type paragraph's count and amendment 1 left it naming
two constructors; amendment 1 added the third constructor to §0.4 and left it out of
pin 3, the rung-type paragraph, row 13 and §7 item 5.

**So the count is now the record: one omission, five sites, three amendments, and
two amendments that each declared it closed while it was live in three more
places.** No amount of care applied to a restatement fixes this; only removing the
restatement does. Amendment 3 therefore **deletes** two figures and one whole
count-bearing clause rather than correcting them, continuing what amendment 2 began.

**What the review confirmed, and what it did not reach.** It independently
reproduced **gate 4a's value comparison** — both sides `(0, 14, 0)` despite the
constant moving from line 70 to 72 — which is the first time a gate this document
added has been verified by someone other than its author. It confirmed the five
amendment-1 findings closed and the staged implementation **structurally clean**. It
returned nothing against M7, any test, or any mutation observation; **three
consecutive reviews have now left those untouched.**

**Status when written (CLOSED):** amendment 3 changed **pin 3**, a stronger claim than
amendment 2's changes, and needed its own independent pass — **that pass closed as
execution review 4.** It changed no staging rule, no gate mechanic and no touch row, so
gates 4, 4a and §2's rules A/B stood on their amendment-2 runs.

> **This paragraph also said "gates 1–3 stand on an unchanged staged tree", and that
> was FALSE — corrected by amendment 4.** The staged tree was **not** unchanged:
> amendment 1's finding 5 was *"corrected in code"* in `bundle.rs`, after the original
> gate run. **Gates 1–3 have been re-run cold; see amendment 4 for the results.**

**Post-ratification amendment 4 — 2026-08-09, on the independent review of
amendment 3.** Three findings, **all three blocking**. Two from the review, the third
from sweeping the row adjacent to the second.

| # | Finding | Disposition |
|---|---|---|
| **1** | **Gates 1–3 were no longer evidenced against the final staged tree, and two claims asserted otherwise.** Amendment 1 corrected the false writer comment **in code**, after the gates had run; amendment 3 then wrote both *"the implementation has survived all three reviews unchanged"* and *"gates 1–3 stand on an unchanged staged tree"*. **Both false.** A comment cannot change behaviour, but the gate is specified over the staged artifact, not over an artifact a reader is invited to infer is equivalent | Both claims corrected, and **gates 1–3 re-run cold** with `cargo clean -p` on all four touched crates first. Results below |
| **2** | **Round 9's finding-1 disposition still gave M7's reference home as `epiphany-testkit`** — superseded in round 12, which found `render_text_document` is `pub(crate)` to `epiphany-textproj` and pinned the harness there. The cell read as current | Supersession pointer added at the round-9 site, following the convention round 8's cell already uses |
| **3** | **Round 9's finding-2 disposition had the same defect.** *(Sweep.)* Its clause *"with any difference enumerated and classified rather than assumed"* was superseded in **round 10**, which found a byte difference would have a third possible cause outside both permitted classifications — making the classification unsound | Supersession pointer added, naming rounds 10, 11, 13 and 14 as the chain that replaced it |

**Finding 1 is the one that matters, and it is an instance of this repository's
central rule.** `CLAUDE.md` requires that a mutation be *observed* to fail, because
**"reasoning that a mutation would fail signs nothing."** Amendment 3 reasoned that a
comment-only change could not move a gate result and reported the gate as standing.
**That is the same substitution — an inference where a measurement is specified** —
committed in the document whose subject is a check that was a tautology because
nobody had measured it. The gates are cheap; the inference cost four rounds of
credibility.

**Gates 1–3, re-run cold against the final staged tree, 2026-08-09:**

| Gate | Toolchain | Result |
|---|---|---|
| 1 `cargo test --workspace` | default (1.97.1) | **1577 passed / 0 failed / 0 ignored, 42 suites**; `cargo` exit 0 |
| 2 `clippy --workspace --all-targets -- -D warnings` | **1.95.0** | exit 0, **0** warning/error lines; all four touched crates observed re-`Checking`ed after `clean -p`, so the result is not a cache replay |
| 3 `fmt -p …×4 --check` | **1.95.0** | exit 0 |

The 1577 figure is unchanged from execution, which is the *expected* outcome for a
comment correction — **but it is now measured rather than predicted**, and that
distinction is the whole of finding 1.

> **Method note, recorded because gate evidence is what this amendment is about.**
> Both gate re-runs were first captured through `tail`, which **truncated the
> aggregate test counts and the `Checking` lines** — the same
> truncated-evidence failure §0.4 records as its second instrument failure, and one
> `CLAUDE.md` names by name. Caught and re-run with full capture before anything was
> reported. **A gate result read through `tail` is not a gate result.**

**Findings 2 and 3 share the shape amendments 2 and 3 kept finding:** a statement that
was true when written, left standing without a marker once a later round overruled it.
The disposition column is a *history*; unmarked, it reads as *instruction*.

**Status when written (CLOSED):** amendment 4 changed no pin, no gate mechanic, no
touch row and no staging rule — it corrected two false status claims, re-ran gates 1–3,
and marked two superseded dispositions. It was the first amendment whose changes were
confined to the historical record and the gate evidence. **Its pass closed as execution
review 5.**

**Post-ratification amendment 5 — 2026-08-09, on the independent review of amendment
4.** Five findings, **four blocking**. Three from the review, two from sweeping.
**Every one is stale current-status prose** — no pin, gate, test, mutation or staged
file is touched.

| # | Finding | Disposition |
|---|---|---|
| **1** | **The top status block said "Three independent reviews … amendments 1, 2 and 3 … Amendment 3 … needs its own independent pass"** — written while amendment 4 was being added three screens below, and stale before that commit landed. **Sixth count-staleness in this document, second inside the status block itself** | **The count and the amendment number are REMOVED from the block**, replaced by an invariant that cannot go stale — *the most recent amendment always needs the next pass; read the last table row for which one that is.* No amendment number appears there now |
| **2** | **"Gates 4 and 4a must be re-run and re-reported"** stood at the top after both had been re-run — twice. **An instruction outliving its execution**, so the document's opening demanded work its own record showed complete | Changed to past tense with the two runs named. *Instructions go stale the same way counts do* — recorded, because every prior staleness finding in this document was about a count or a claim, never an imperative |
| **3** | **"What remains open after ratification" was still pre-execution text**, asserting M7 unverifiable and that no gate, test or mutation had run. Both false | Marked as the **ratification-time record**, bullets struck through with their execution outcomes, and the one bullet that *is* still open identified as the only one |
| **4** | **A SECOND copy of that block sat higher in the document** — *"What ratification does NOT settle, stated at the top so it is not missed"* — with the identical staleness. *(Sweep.)* Being higher, it was read **first**: the opening of the contract told a reader nothing had run | Same treatment; and the duplicate third bullet the two blocks shared is now **merged into one** rather than corrected twice |
| 5 | **"No execution work may begin — not implementation, not staging"** read as live. *(Sweep.)* It sits in the *Probe RESULT* subsection, **three subsections past** the `SUPERSEDED BY RATIFICATION` marker covering the exception, so it was the last thing before the history table | Marked lifted-at-dispatch and retained as the probe's authorisation boundary |

**Findings 4 and 5 give the rule this document had not yet stated: a supersession
marker governs the section it heads, not every later restatement of what it
superseded.** Both blocks and the prohibition were *downstream* of markers that
already existed, and each still read as current. This is the fix-every-site rule —
recorded here since round 1 for counts — applied to **markers**.

**The amendments share one diagnosis, and it is not carelessness about
wording.** The first fixed a missing surface; **every one since has fixed duplicated
state** — a count, a figure, a disposition, a status claim, an instruction — restated
in a second place and updated in only one. **Every structural fix that has held was a
deletion**: the history table replacing prose tallies, row 13's count removed, pin 3's
57 removed, the rung-type figures removed, and now the status block's count and
amendment number removed. **Every fix that was a more careful restatement has failed.**

**Status when written (CLOSED):** amendment 5 touched **no pin, no gate, no test, no
mutation, no touch row and no staged file** — so gates 1–3 (re-run cold under amendment
4) and gates 4, 4a and §2's rules A/B (re-run under amendments 2 and 4) stood on
evidence it did not disturb. **A scope claim, not an inferred gate result** — the
distinction amendment 4 exists to enforce. **Its pass closed as execution review 6.**

**Post-ratification amendment 6 — 2026-08-09, on the independent review of amendment
5.** Three findings, **one blocking**. Two from the review, one from the sweep. **All
three are defects amendment 5 introduced while fixing staleness**, which is the finding
that matters more than any of them individually.

| # | Finding | Disposition |
|---|---|---|
| **1** | **Amendment 5's new meta-claim was false, and its own findings disproved it.** It said every post-ratification amendment after the first was prompted by defects in **the previous amendment's** text. Amendment 5's findings were a status block **amendment 3** left, a gate instruction **amendment 2** wrote, and three pieces of **ratification-time** text — none written by amendment 4 | Widened to *"the previous amendment's text, **or earlier text a previous correction failed to sweep**"*, which covers both routes. They are the same failure either way |
| 2 | **Live tallies reintroduced immediately below the no-restatement invariant** — *"five reviews … have now closed"* and *"The five amendments now have one diagnosis"*, both stale at the next pass, three lines under the rule forbidding them | Replaced with a table pointer and non-numeric phrasing |
| 3 | **A third live tally in the same block.** *(Sweep.)* *"four rounds of amendment traffic can read as instability"* — same class, same paragraph group, missed by the review | Made non-numeric |

**Finding 1 is the sharper one, because the false claim was load-bearing.** It was the
document's own account of *why* it keeps needing amendments, and the narrow version
pointed the next reader at the wrong place to look — at the last amendment, rather than
at every prior correction that stopped short of a full sweep. **Amendment 5 fixed five
staleness defects and, in the same edit, three of them were in text that no recent
amendment had touched.** The corrected claim is the one that would have predicted that.

**Finding 2 is the pattern arriving at its limit case: a rule violated three lines
below its own statement.** Amendment 5 removed a count from the status block, wrote the
invariant forbidding counts there, and then wrote three more counts underneath it. That
is not inattention — it is that *"the amendments now share one diagnosis"* does not
**feel** like a count while being one.

**So amendment 6 states the rule execution reviews 5 and 6 each had to rediscover:**
**a figure recording what a round found or did is historical and never updated; a
figure inside a general claim about the document's present state is live and must not
be written at all — wherever it sits.** Location does not decide it; tense and scope
do. That rule is now in the invariant block, because both reviews found the defect by
applying it implicitly and no statement of it existed to apply.

**Status when written (CLOSED):** amendment 6 touched **no pin, gate, test, mutation,
touch row or staged file** — a scope claim, not an inferred gate result. **Its pass
closed as execution review 7.**

**Post-ratification amendment 7 — 2026-08-09, on the independent review of amendment
6.** Three findings, **all three blocking**. Two from the review, one from the sweep.
**All three are failures of the invariant amendment 5 introduced and amendment 6
refined** — the single-source rule did not survive contact with the text it was meant
to govern.

| # | Finding | Disposition |
|---|---|---|
| **1** | **The invariant contradicted five older standing instructions.** It says only the most recent amendment needs a pass, while amendments 1–5 each still ended *"this amendment needs another independent pass"* — **five live commands, all false**, plus amendment 2's order to re-run gates 4 and 4a, already carried out twice. **Creating a single source did not remove the other five** | All six closers rewritten as **dated records** — *"Status when written (CLOSED) … its pass closed as execution review N."* The invariant now states outright that the amendment blocks carry no live status and none may be added |
| **2** | **The invariant's locator could not identify the pending amendment.** It said read *"the last row"* of the history table — but the literal last row is **`Total`**, and the last review row names *"execution review 6"*, an event, not an amendment. **The rule could not be followed to an answer** | Locator fixed: the **last `execution review` row, immediately above `Total`**, and the amendment it produced carries the **same number** — review *N* → amendment *N*, one per row, by construction |
| **3** | **Amendment 6's own closer had the same defect.** *(Sweep.)* The review named amendments 1–5; amendment 6's said *"amendment 6 needs the next independent pass"* — **true when written, false the moment amendment 7 existed.** Fixing only the five would have rebuilt the contradiction on the next commit | Included in finding 1's treatment. **Six closers, not five** |

**Finding 1 is the failure mode this document has now demonstrated at every scale.**
Amendment 5 removed a count and wrote an invariant to own it; amendment 6 refined the
invariant; **neither swept the five places already answering the question the invariant
claimed to own.** *A single source of truth is not created by declaring one — it is
created by deleting the others.* That sentence is the whole of the last three
amendments, and it had to be learned three times: for counts (amendment 2), for figures
in general claims (amendment 6), and now for **instructions**.

**Finding 3 is why the review's own scope was insufficient**, and it generalises: a
correction that fixes every *currently false* instance of a claim, but leaves the one
instance that is *currently true*, has scheduled its own recurrence. The true one goes
false on the next event. **Correct by class, not by current truth value.**

**Status when written (CLOSED):** amendment 7 touched **no pin, gate, test, mutation,
touch row or staged file** — a scope claim, not an inferred gate result. **Its pass
closed as execution review 8, CLEAN.**

**Record of execution review 8 — 2026-08-09, independent, against `480a299`. ZERO
FINDINGS.** It confirmed the top block is the sole live pass-status authority, that the
historical blocks are explicitly closed, that the locator resolves execution review 7 to
amendment 7, and that the totals reconcile at **95 findings / 70 blocking**.

**This is a RECORD, not an amendment.** It appends a history row, marks the
ratification-time bullet satisfied, and adds the invariant's **terminating case** — which
no earlier version could have specified, because only a clean round reaches it. **It
changes no pin, gate, test, mutation, touch row or staged file.** Its own text is
unreviewed, and it is deliberately this short for that reason.

**What the clean round establishes, and what it does not — the same distinction round 19
drew, and it has not weakened.** It is the criterion named at round 11: *treat
dispatchable as a claim requiring evidence of convergence, not a status reached by
running out of findings.* Two clean rounds now exist in this document — round 19 before
execution, and this one after — and **the second is the stronger of the two**, because
what it reviewed had been executed, gated and measured rather than only argued.

**It is still not proof of correctness.** Execution review 8 reviewed amendment 7; it did
not re-derive the whole document, and **no round ever has**. The defect record is the
history table, and it shows that every independent round before this one found something.

**What is now established, stated plainly so acceptance is not read as inference:**

- **The implementation is unchanged since amendment 1's comment correction**, and gates
  1–3 were **re-run cold against that exact tree** under amendment 4 — 1577 passed / 0
  failed / 0 ignored across 42 suites; clippy and fmt clean on pinned **1.95.0**.
- **Gates 4, 4a and §2's rules A and B** were re-run under amendments 2 and 4.
- **M7 ran**, returning its three observations and its control, and **three independent
  reviews examined it and returned nothing against it.**
- **Every finding since execution has been in this contract, not in the 21 staged
  files** — across all eight reviews.

**The repository owner accepted the implementation on 2026-08-09, after execution
review 8.** That decision is recorded here; it does not recast the evidence as proof of
correctness. Round 1's author-claimed ratification was withdrawn, which is why the
evidence and the owner's decision remain distinct.

**Review round 19 — 2026-08-08, independent, against the round-17/18 working
tree. ZERO FINDINGS. The first clean round in nineteen.**

It confirmed the per-test state table distinguishes test 8's base *introduction*
from test 9's unrelated commit, that the route swap is consistent with
`craft_image_with_base`'s actual use in test 6's ancestor, and that the revised
gate mechanics are internally consistent.

**What a clean round does and does not establish.** It is the criterion named at
round 11 — *"treat dispatchable as a claim requiring evidence of convergence, not
a status reached by running out of findings"* — and it is the first evidence of
that kind this contract has produced. **It is not proof of correctness.** Round 19
reviewed the amendment rounds 17 and 18 produced; it did not re-derive the whole
document, and no round has.

**What remains open after ratification, and is not closed by it:**

- **M7's authority/base leg is unverifiable until S27 is implemented**, by
  construction — `BundleCapabilities` and `CURRENT_REDUCTION_ALGORITHM_VERSION`
  are S27's own deliverables. It is an **execution requirement**, not a document
  gap, and the scratch probe is evidence for its prerequisite only. **Do not cite
  the probe as having demonstrated laundering; it carried no base.**
- **Every gate, test and mutation is specified but none has been run.** The
  contract's claim is that they *can* be run and that their results *would* be
  evidential — nineteen rounds went into that claim, and execution is what tests
  it.

**The defect record, stated plainly so ratification is not read as vindication:**
65 findings across 19 rounds, 47 blocking. Rounds 1 and 2 were authored-side and
their ratification was withdrawn. Round 17 was authored-side and cost two defects
round 18 caught. **The document's quality comes from the rounds that were
independent, and that is the argument for the execution report being reviewed the
same way.**

(Was: DRAFT, BLOCKED on the format-epoch rung,
`spec/CONTRACT_FORMAT_EPOCH_MAJOR1.md`, which at the time was ratified and in
implementation but had not yet landed. **That rung landed at `bc06706`**, with
its pin-3b follow-up at `be244df`.)

**Review round 1 — 2026-08-07, at `96b40b2`.** Run because this contract had
reached "dispatchable" with **zero** ratification rounds on record, against a
standing rule that contracts go through adversarial review *before* dispatch —
the format-epoch rung had four, and its fourth is what produced pin 3c. What
round 1 returned:

| # | Finding | Disposition |
|---|---|---|
| 1 | Inherited obligation 2 was in **neither** §3 nor §4, though §3's preamble claimed all obligations were stated as tests | **M7** added; ruled a mutation, not a capability restoration |
| 2 | §0.4's *"`commit` has 57 sites (including 2 in `epiphany-editor-core`)"* — that crate has no `epiphany-bundle` dependency and the word `Bundle` appears in its `lib.rs` **zero** times | Corrected; recorded as the **third** instrument failure in §0.4 |
| 3 | §3's header, gate 1 and §7 items 1/2/4 each carried a stale count of the same lists | All replaced with "every item in §N"; gate 1 now reports three buckets |
| 4 | `requirement_labels.rs` absent from §2 while pin 9 may move `CORE_REQUIREMENT_COUNT` | Pin 9 must decide explicitly; **touch row 12** added, conditional |
| 5 | Locators verified at `381c498`; `bc06706` grew `bundle.rs` by 338 lines | Correction table in §0; pin 5's own `:396`–`:399` confirmed **unmoved** |
| 6 | Pin 2a cites `vectors.rs:353`/`:363`; the corpus was rebuilt to `canonical_bases` 2 → 0 | Evidence updated; disposition unchanged |
| 7 | `Bundle::open(` is **60**, not 57 | §0.4 table corrected; `create` confirmed still 32 |
| 8 | Gate 6a checked `textproj` only, while touch row 7 gives `testkit` the real authority | Scope widened to both |
| 9 | No **commit-side positive** test, though obligation 1 warns that converting one branch leaves a hole | **Test 8** added |

**Review round 2 — 2026-08-07, at `39287f8`.** Run against the *ratified and
frozen* contract, and it returned **six more findings, four of them blocking**.
Round 1's ratification was premature; this is the round that should have followed
it before dispatch.

| # | Finding | Disposition |
|---|---|---|
| 1 | The call-site count was corrected in §0.4 only. The "Rung type" paragraph still said **57**, and touch row 2 still said `bundle.rs` has **35** opens — a figure that was never `bundle.rs` alone (it was `bundle.rs` + `fuzz.rs`, which has its own row) and is now stale on top of that. Reconciliation was impossible | Both corrected to **60** / **23**; row 5 now states `fuzz.rs`'s 15 + 1 explicitly |
| 2 | §0.4 called `project.rs:936` a **production** bundle writer. `#[cfg(test)]` starts at `:630`; every `Bundle` call in the file is below it | Struck. §0.4's correction stands on `serialize.rs` alone. Recorded as the **fourth** instrument failure in that section |
| 3 | **M5 unexecutable.** `serialize_document` refuses bases at `serialize.rs:151`, so its output is base-free, and pin 5 + test 4 require base-free bundles to open at *any* authority — the mutation cannot fail | Split into **M5a** (production wires the constant) and **M5b** (the authority is load-bearing where a base exists) |
| 4 | **M6's second half unexecutable.** `open` rejects a stale base, `create` rejects a base-bearing manifest (`bundle.rs:234`), `commit` validates what it emits — no caller can hold an open `Bundle` with a stale *inherited* base | Replaced: **broaden** pin 3a rather than narrow it. The unreachability is itself reported |
| 5 | Pin 3a's justification — *"production code mints a self-consistent stale document"* — is false in-tree. **Zero** production paths stage a base | Restated: pin 3a guards the **public `commit_versioned` API**, not an in-tree path |
| 6 | `serialize.rs:157` is dead code, orphaned by the `:151` guard | Recorded as a finding; explicitly **not** this rung's to repair |

**Two of round 2's findings were introduced by round 1, and that is the lesson
worth carrying.** Ruling M7's refusal permanent is what made M5 unexecutable, and
adding test 8 on the write side did not come with a re-derivation of M6 against
the same reachability. **An amendment is a change to the system, not a patch to a
line**; the next round must re-derive every mutation against every ruling the
previous round made, not only inspect the text it edited. Round 1's own §0.4
correction has the same shape: it verified one claim in a list and inherited its
neighbours.

**Pins 1 and 3–10 are settled and internally consistent.** Pin 2a is resolved
from outside (below); pin 2 is unchanged.

**The pin 10 amendment**, recorded here so it is not read as the original: pin 10
made P13-S16's opening conditional on pin 2a being *"ratified and tested **within
this rung**"* — a condition that the only route pin 2a permitted, resolution from
outside, could never satisfy. Ratification is now sourced correctly. The
*tested* half is **not** waived: it is carried by this rung's inherited
obligations rather than dropped. See pin 10.

**Pin 2a is RESOLVED as of 2026-08-07** — from outside this rung, by that
contract's pin 8, exactly as its prohibition required. Legacy bases are refused
by container epoch, never by version arithmetic; see the resolution block under
pin 2a. This contract additionally **inherits three obligations** from that rung
(the two interim refusals it must convert to validation, M8's deferred laundering
demonstration, and pin 3c's two suspended conformance assertions) — recorded in
the same place.

**Its dependency cleared when the format rung landed, not before — and that has
now happened.** This is no longer bounded analysis. Pin 2a's original prohibition
stands for the record: it was never amended into a disposition from inside this
contract, and the resolution above came from outside, which is exactly what the
prohibition required.

> **Two senses of "dispatchable", disambiguated 2026-08-07 — this sentence
> previously used the wrong one.** *Unblocked* means the dependency chain has
> cleared: true since `bc06706`. *Dispatchable* means ratified and frozen and
> therefore ready to execute: **false**, and see the status block. This rung is
> **unblocked but not dispatchable**. Round 1 read the first as the second, which
> is how it came to be ratified after a single round.

**Rung type:** **capability + API change.** No wire bytes move and no schema
major or minor changes — `BundleError` has no discriminant and no encoder
(`bundle/src/error.rs`), so a new variant is a pure Rust API change. What does
change is the signature of **all three** public constructors — `Bundle::open`,
`Bundle::create` **and `Bundle::create_versioned`**.

> **`create_versioned` ADDED HERE BY AMENDMENT 3, and the call-site figures are
> REMOVED from this paragraph rather than corrected.** It read *"`Bundle::open`'s
> and `Bundle::create`'s signatures, at **60** and 32 call sites"*. §0.4 owns the
> surface and is its single origin; this paragraph only needs to say *which*
> constructors change to characterise the rung type, and every figure it has ever
> carried went stale.
>
> **This paragraph has now been left behind by a §0.4 correction TWICE, and the
> parenthetical below records the first time.** Round 1 fixed §0.4's table and left
> this sentence saying "57"; round 2 fixed it to "60". Amendment 1 then added the
> third constructor to §0.4 and left this sentence naming two, where the independent
> review of amendment 2 found it. **Same paragraph, same failure mode, two different
> counts, two different rounds** — so the fix is to stop restating the surface here,
> not to restate it more accurately a third time.

*(Was "57 and 32". Corrected in review round 2 — round 1 corrected
§0.4's table and left this spelling and touch row 2's untouched, which is the
same one-path-of-several defect §1.7 of the handoff names.)*

**Now DOES unblock P13-S16, once it lands.** This rung installs the authority and
validates both read and write paths; S16's remaining precondition was pin 2a's
legacy-base disposition, and that is resolved. The chain is therefore
format-epoch rung → **P13-S27** → **P13-S16**, with no open question left in it.

**Rulings already made (2026-07-31), not re-opened here:**

- **Fork 1 → B, typed injected capability.** A required capability carrying the
  current reduction version, with **no bundle-local default**: every caller
  consciously supplies the semantics it implements. Broadened from
  `OpenCapabilities` to **`BundleCapabilities`** in this revision, because it now
  governs creation and commit as well as opening.
- **Fork 2 → outright rejection.** A dedicated error, not read-only and not an
  integrity anomaly.

---

## §0. What was verified before drafting

Read out of the working tree at `381c498`. Every line number confirmed by
reading the line.

> **Locators re-verified 2026-08-07 in review round 1, at `96b40b2`.** The
> format-epoch rung (`bc06706`) landed *after* `381c498` and grew `bundle.rs` by
> **338 lines**, so the inline citations throughout this contract are as-of
> `381c498`. **This table is authoritative where the two disagree.**
>
> | Cited | Actual at `96b40b2` | Symbol |
> |---|---|---|
> | `bundle.rs:989` | **`:1024`** | `fn reduction_version_for` |
> | `bundle.rs:798` | **`:833`** | `commit_versioned`'s superblock stamp |
> | `bundle.rs:613`–`:621` | **`:622`ff** | `fn verify_canonical_chunks` |
> | `bundle.rs:939` | **`:974`** | `fn profile_is_understood` |
> | `bundle.rs:233`–`:240` | **`:205`ff** | `fn create` (base-bearing refusal) |
> | `error.rs:253` | **`:306`** | `UnsupportedCanonicalChunkMajor` |
> | `serialize.rs:119` | **`:143`** | `fn serialize_document` |
> | `serialize.rs:212`, `:219`–`:222` | **`:239`ff** | `fn build_manifest` |
> | `serialize.rs:347` | **`:379`** | `fn serialize_and_reopen` |
> | `roundtrip.rs:241` | **`:255`** | `assert_reduction_serialization_stable` |
> | `parse.rs:591` | **`:603`** | unbounded `u32` parse |
>
> **Confirmed still exact, and not to be "corrected":** `bundle.rs:396`–`:399`
> (**pin 5's own insertion point**), `:301` (`open`), `:87`
> (`SUPPORTED_PROFILE_MAJOR`), `:389` (`UnsupportedProfile`), `ids.rs:289`,
> `generators.rs:1628`/`:1651`.

### 0.1 The defect is a tautology, not an absence

`reduction_version_for` (`bundle.rs:989`) sets a new superblock's version from
**the canonical base's own self-report**, `unwrap_or_default()` when there is no
base. `open` (`bundle.rs:396`–`:399`) then rejects a bundle whose base's version
**disagrees with that superblock's**. Both operands descend from the same
source, so for any conformingly-written document the comparison is a tautology.

**It is not vacuous** — it catches a corrupt or tampered base whose version
disagrees with its superblock, and that behaviour is preserved by pin 6. What it
cannot catch is a *valid stale base*: one whose version was conformingly
propagated from an earlier implementation. That is exactly the case
`core_spec.tex:11614`–`:11617` exists to prevent.

### 0.2 The project has this pattern twice, and both instances are the wrong shape here

- `SUPPORTED_PROFILE_MAJOR` (`bundle.rs:87`) → `profile_is_understood`
  (`:939`) → read-only + `IntegrityAnomaly::UnsupportedProfile` (`:389`).
- `IntegrityAnomaly::UnsupportedCanonicalChunkMajor { schema_major }`
  (`error.rs:253`ff).

Both are **bundle-crate constants**, and `open(store: S)` (`bundle.rs:301`)
takes no capability parameter. Fork 1's ruling deliberately departs from this
precedent, because a profile major and a schema major are *visible in the
container* while reduction semantics are not: a bundle-crate constant would be a
number the container crate cannot verify and a rung must remember to bump —
the hand-maintained-parallel-table shape that produced P13-S15 and the four
Push-4a literal sites.

### 0.3 The layering forces the split, and makes `ids.rs:288` repairable

`epiphany-bundle`'s only workspace dependency is `epiphany-determinism`.
It cannot read a constant in `epiphany-ops`. So `ids.rs:288`–`:289`'s claim that
*"the algorithm catalog itself lives in `epiphany-ops`"* is not merely false —
as written it is **unimplementable from where the check must run**.

Fork 1's ruling makes it true: the authoritative number lives in
`epiphany-ops`, and the composing layer wraps it. **`epiphany-ops` must NOT gain
a dependency on `epiphany-bundle`** merely to use the wrapper type — hence a
plain `u32` in `epiphany-ops` and the `ReductionAlgorithmVersion` wrapper
constructed at the composition boundary.

### 0.4 The surface, and a correction to this contract's own first draft

**The writer path is production, not test-only. An earlier draft of this
section asserted the opposite and was wrong.**

`epiphany-textproj` serializes a `TextDocument` into a bundle in production
code: `serialize_document` (`serialize.rs:119`) creates a bundle and commits a
manifest built by `build_manifest` (`serialize.rs:212`), which **copies
`base.reduction_algorithm_version` verbatim** into a fresh `SnapshotRef`
(`:219`–`:222`). `commit_versioned` then stamps the superblock from that same
carried value via `reduction_version_for` (`bundle.rs:798`). ~~`project.rs:936` is
a **second** production write path of the same shape.~~

> **CORRECTED 2026-08-07 in review round 2 — `project.rs` is NOT a production
> write path, and never was.** `#[cfg(test)]` begins at `project.rs:630`. Every
> `Bundle` call in the file sits below it: `Bundle::create` at `:983` and
> `:1122`, `Bundle::open` at `:1147`. `:936` is an assertion inside a test. The
> `serialize.rs` half of this paragraph **is** correct — its `Bundle::create`
> (`:155`) and `commit_versioned` (`:183`) are above that file's `#[cfg(test)]`
> at `:284` — so §0.4's correction stands on **one** example, not two.
>
> **The same slip reaches the open table below:** both textproj entries there
> (`serialize.rs:383`, `project.rs:1147`) are also below their files'
> `#[cfg(test)]`, so **`epiphany-textproj` has zero production `Bundle::open`
> sites.** The crate still counts 2 for signature-change purposes; it counts 0
> for any argument about what production does.
>
> **Fourth instrument failure in this section.** Round 1 checked the
> `epiphany-editor-core` claim in this paragraph and did not check its
> neighbours — verifying one claim in a list and inheriting the rest.

**Pin 3a's justification, restated in review round 2.** This paragraph used to
conclude *"production code mints a self-consistent stale document without ever
calling `open`."* **That is no longer true in-tree.** Checking every
`canonical_base:` assignment above the `#[cfg(test)]` boundary in `serialize.rs`,
`project.rs` and `bundle.rs` returns **zero**: the format rung's pin 3b closed
the only in-tree path when it made `serialize_document` refuse a base-bearing
document (`serialize.rs:151`), and `create` already rejected a base-bearing
manifest (`bundle.rs:234`).

**Pin 3a survives on a different and narrower footing, which it must now state:**
`commit`/`commit_versioned` are **public API**, and an out-of-tree caller can
stage a canonical base directly without going through `epiphany-textproj` at all.
The writer check guards that surface. It is no longer guarding an in-tree
production path, because there is not one.

> **Consequence for M5, which round 1 did not follow through.** If no production
> path stages a base, no production path is authority-load-bearing, and a
> mutation of the authority cannot break a production round trip. M5 was written
> against the old reading and is corrected in §4.

> **Method note, recorded because it is this rung's own subject matter.** The
> false claim came from `grep … canonical_base | head -14`. The `textproj` hits
> were below the cut. A universal negative — "no production code *anywhere*" —
> was asserted from deliberately truncated evidence. This is the second
> instrument failure in this rung: the first searched for
> `ReductionAlgorithmVersion(` constructor calls and so could not see a path
> that *propagates* a value without constructing one. Both are the defect S27
> exists to fix, committed while scoping it: **an observation that cannot
> support the claim drawn from it.**

**Reader surface — 60 `Bundle::open(` sites across 10 files.** *(Was 57 at
`381c498`; the format-epoch rung added 3 in `bundle.rs`. Re-counted 2026-08-07 at
`96b40b2`.)*

| Crate | Sites |
|---|---|
| `epiphany-bundle` (`bundle.rs` **23**, `fuzz.rs` 15) | **38** |
| `epiphany-testkit` (`bundle_harness.rs` 11, `roundtrip.rs` 4, `benches/bundle.rs` 2, `tests/bundle_reopen.rs` 1) | 18 |
| `epiphany-textproj` (`serialize.rs`, `project.rs`) | 2 |
| `epiphany-bundle/tests/` | 2 |

**Writer surface — 32 `Bundle::create(` sites:** `bundle.rs` 11,
`testkit/roundtrip.rs` 6, `testkit/bundle_harness.rs` 6, `textproj/project.rs` 2,
`bundle/tests/crash_recovery.rs` 2, `textproj/serialize.rs` 1,
`testkit/tests/bundle_reopen.rs` 1, `testkit/benches/bundle.rs` 1,
`bundle/tests/manifest_selection.rs` 1, `bundle/fuzz.rs` 1.
*Re-counted 2026-08-07 at `96b40b2`: still 32, and every per-file figure above
still holds.*

**Writer surface, part two — 3 `Bundle::create_versioned(` sites. ADDED BY THE
POST-RATIFICATION AMENDMENT, 2026-08-09, on a finding from execution.**

`bundle.rs` **2** (`:2326`, `:2367`), `testkit/gminor.rs` **1** (`:85`).

> **This surface was missing from the contract entirely — zero mentions across
> nineteen review rounds.** `create_versioned` is a **third public constructor**
> whose signature this rung changes, distinct from `create` (which delegates to
> it). §0.4 searched `Bundle::open(` and `Bundle::create(` and concluded about
> "the writer surface".
>
> **This is the fourth instrument failure recorded in this section, and the same
> shape as the first three:** a count taken from one spelling and generalised. The
> first could not see a propagating path; the second asserted a universal negative
> from `head`-truncated output; the third resolved a method name without resolving
> its type; **this one enumerated two of three constructors.**
>
> **It is also the root cause of a touch-table gap.** `testkit/gminor.rs` calls
> *only* `create_versioned`, so it never appeared in any surface count and
> therefore never got a touch row — found by gate 4 failing during execution,
> exactly as an allowlist should.

**`commit` sites** — pin 3's design keeps every one of them unchanged.

> **CORRECTED 2026-08-07 in review round 1. The claim this paragraph made was
> *"`commit` has 57 sites (including 2 in `epiphany-editor-core`)"*, and the
> parenthetical is false.** `epiphany-editor-core` depends on `epiphany-core`,
> `epiphany-ops` and `epiphany-layout-ir` — **not** on `epiphany-bundle` — and
> the string `Bundle` does not appear in its `lib.rs` at all. Its two hits are
> `self.commit(...)` (`editor-core/src/lib.rs:1593`, `:1709`), resolving to its
> own `fn commit(&mut self, new: Vec<OperationEnvelope>) -> Result<EditOutcome,
> EditorError>` (`:1404`). A textual `.commit(` grep counted a same-named method
> in a crate that cannot reach `Bundle`.
>
> **This is the third instrument failure recorded in this section, and it was
> committed in the same paragraph as the method note above.** The first could not
> see a propagating path; the second asserted a universal negative from `head`-
> truncated output; this one resolved a method name without resolving the type it
> belongs to. Same defect, three shapes: **an observation that cannot support the
> claim drawn from it.** The executing agent MUST count `Bundle`-typed receivers,
> not the token `.commit(`.

**Which capability each site supplies is NOT decided by crate dependency.**
`epiphany-testkit` and `epiphany-textproj` depend on `epiphany-ops`, but that
does not mean every call site there should pass the real constant. Format and
container fixtures that deliberately exercise arbitrary wire values — e.g.
`generators.rs:1628`/`:1651`'s `rng.range(0, 8)`, and the committed
text-projection vectors — MUST pass an **explicitly named synthetic capability
matching the fixture**, so the test keeps testing what it was written to test.
Only **production composition paths** wrap
`CURRENT_REDUCTION_ALGORITHM_VERSION`. Pin 3b makes this a naming rule rather
than a judgement call.

---

## §1. Pins

**Pin 1 — the authority is a plain `u32` in `epiphany-ops`, with its discipline
beside it.**

```rust
pub const CURRENT_REDUCTION_ALGORITHM_VERSION: u32 = 0;
```

**`epiphany-ops` MUST NOT gain a dependency on `epiphany-bundle`.** The constant
is a bare `u32`; the `ReductionAlgorithmVersion` wrapper is constructed by the
composing layer (pin 3).

Its doc comment carries the **bump discipline**, in the shape of
`PLAN_GMINOR_SCHEMA_MINOR.md`'s epoch rules: *any change to a canonical
reduction verdict or to canonical reduced state MUST bump this constant and
record the change here*, with a dated list of bumps. State plainly that **no
mechanism can detect a semantics change** — a golden test over reduction outputs
can prompt the question, never answer it — so the discipline is the guarantee.

**Pin 2 — the initial value is 0, and this is a decision, not a placeholder.**
Bundles written to date carry `0` when they have no base, and bases self-report
whatever they were stamped with. Starting the constant at anything but `0` would
make every existing base-bearing document fail to open **without any semantics
having changed** — the rung would manufacture the breakage it exists to detect.
The first real bump is **P13-S16's**, to `1`.

The doc comment must say this, so `0` is not later read as "unset."

**Pin 2a — the legacy-base disposition is OPEN, and S16 is blocked on it.**
Baseline `0` does **not** preserve every current base-bearing document, and this
contract's first draft claimed it would. Verified: `serialize.rs:327`–`:331`
builds a canonical base stamped `ReductionAlgorithmVersion(1)` and
`serialize_and_reopen` (`:347`) round-trips it; `vectors.rs:353`/`:363` are
committed text-projection vectors carrying the same; `generators.rs:1628`/`:1651`
emit `rng.range(0, 8)`. Injecting the real authority `0` at every site would
reject part of the present corpus.

> **Evidence updated 2026-08-07 in review round 1 — the disposition is unchanged,
> the corpus is not.** `vectors.rs:353`/`:363` no longer exist. The format-epoch
> rung rebuilt the text-projection corpus to 20 vectors with `canonical_bases`
> reach **2 → 0**, so the two base-carrying vectors this pin cites are gone; the
> single surviving occurrence is `vectors.rs:326`, inside the
> `canonical_base_present` **reject** vector. Pin 2a's conclusion stands (it was
> settled from outside by the container epoch), but **pin 3b's fixture-surface
> reasoning must be re-derived against the current corpus** rather than against
> the two vectors named here. `generators.rs:1628`/`:1651` are unaffected and
> still emit `rng.range(0, 8)`.

Pin 3b handles the *fixtures* — they take named synthetic capabilities. It does
**not** handle the real problem:

> **After S16 moves the authority to `1`, a pre-S27 base that happens to carry
> `1` is indistinguishable from a legitimately rebuilt S16 base. A raw `u32`
> carries no provenance, so no check can tell a conforming new base from a
> coincidental legacy one.**

`0` may remain the baseline, but this rung MUST settle **one** of:

- **(i) Normalize before S16** — a migration that rebuilds or re-stamps every
  pre-authority nonzero base, so the space below the first real bump is empty by
  construction. Requires locating every such artifact, including committed
  vectors.
- **(ii) Choose a non-colliding epoch** — start the authority at a value no
  pre-authority artifact can hold (the observed range is `0..8`, so e.g. `1000`),
  making legacy values structurally distinguishable, and define what an
  implementation does on encountering one.
- **(iii) Carry provenance** — a wider or tagged authority type. The largest
  change; it makes `ReductionAlgorithmVersion`'s wire meaning richer and is
  likely a schema question, which the other two are not.

- **(iv) `FORMAT_MINOR` as a provenance carrier — PROPOSED AND REJECTED
  2026-07-31.** Recorded here so it is not re-proposed. The idea was to bump
  `FORMAT_MINOR` (`header.rs:42`) so pre-S27 bundles are structurally
  identifiable, since `decode` gates on **major** only (`:119`). Two independent
  objections, either one fatal:

  1. **The header is immutable and commit cannot touch it.**
     `core_spec.tex:10799`–`:10800`: *"The header never changes after the file is
     created."* `commit_versioned` (`bundle.rs:791`) computes and publishes only
     a new **superblock**. So: open a legacy minor-1 bundle with no base; under
     S27 commit a base that writer validation just accepted; the header is still
     minor 1. If minor ≤ 1 bases are rejected, a base S27 itself validated
     becomes permanently unusable. If they are accepted, S16 still cannot tell an
     old arbitrary `1` from a real S16 `1`. **Both branches fail**, which is the
     whole question 2a exists to settle.
  2. **A minor bump may not mean this.** `core_spec.tex:12258`–`:12262`: minor
     changes are backward-compatible and *"MUST only append discriminants to the
     append-safe"* set. Making a previously-valid canonical base newly rejectable
     is a **semantic acceptance change**, not an append. And because current
     readers ignore minor entirely, they would open minor-2 bundles and skip the
     check — an enforcement boundary that binds only the readers that already
     comply.

  The proposal also mis-stated its own corpus result: after the bump,
  `serialize_document` writes a **minor-2** header, so its base-`1` fixture must
  fail writer validation under real authority `0`. A synthetic capability
  (pin 3b) can preserve that as a *format* test, but it cannot make the fixture
  evidence about a legacy minor-1 bundle — the two are different documents.

**The insight worth keeping from (iv), stated so a later rung can use it:**

> Provenance MUST be carried by a container property that **old readers cannot
> silently accept** and that **cannot be inherited unchanged by a later commit**.
> `FORMAT_MINOR` fails both halves; any candidate carrier must be checked against
> both before it is proposed.

That points at a real format-epoch design — most plausibly a **major**-version
boundary with explicit legacy-read and rebuild handling, or a
generation-scoped attestation paired with an incompatibility boundary for old
readers. Either is a **format-design rung of its own**, with an honest
rebuild/repack policy for legacy bases; neither is a clause this contract can
absorb.

**Until a disposition is ratified AND tested, P13-S16 does not become
dispatchable merely because S27 lands.** This pin deliberately remains an open
question. It is not to be amended into a disposition without its own ratification
round.

### Pin 2a — **RESOLVED 2026-08-07 by the format-epoch rung.**

The ratification round this pin demanded is the one the format-epoch contract
had: `spec/CONTRACT_FORMAT_EPOCH_MAJOR1.md`, ratified after four adversarial
review rounds, whose **pin 8** exists to resolve this pin from outside it. The
open analysis above is retained verbatim as the reasoning that produced the
answer, not superseded prose.

**The disposition is none of (i), (ii) or (iii): it is the container epoch.**

> **Reduction-version authority is meaningful only in major-1 containers.
> Legacy bases are refused by container epoch, never by version arithmetic.**

That is why the collision this pin identified never has to be adjudicated. A
pre-S27 base carrying `1` and a legitimately rebuilt S16 base carrying `1` are
indeed indistinguishable **as numbers** — and they never meet, because the
pre-S27 base can only exist in a major-0 container, which is refused at the
epoch boundary before any version is compared. The `u32` never has to carry
provenance, because the container already does.

Each rejected option, and why the epoch beats it: **(i)** normalizing the corpus
would have to find every artifact, and a missed one is silently wrong forever;
**(ii)** a non-colliding epoch value is a convention a hand-authored document can
simply declare — `parse.rs:591` accepts an unbounded `u32`; **(iii)** widening
the type makes the wire meaning richer and buys nothing the container property
does not already give. All three try to make a number carry provenance. The
epoch makes the *file* carry it.

**S27's own baseline stays `0`** (pin 2 is unchanged). What changes is that the
question "what about a base older than the authority?" is no longer S27's to
answer.

### Inherited from the format-epoch rung — obligations S27 must discharge

The format rung lands **before** this one and closes two things temporarily,
naming S27 as what reopens them. Both are owed work here, not optional:

1. **The interim refusals become real validation.** The format rung's pin 3a
   temporarily refuses **both** major-1 base boundaries — opening a major-1
   container that already carries a base, and committing a base into one —
   through a third, temporary error (`ReductionAuthorityUnavailable`) that is
   distinct from its two legacy/repack errors. S27 replaces **both** branches
   with capability validation. Replacing only one leaves a hole exactly where
   the format rung's own review found one.

2. **The deferred laundering demonstration** (format-rung M8). That rung could
   not demonstrate the text-import laundering path end to end, because pin 3a
   refuses every major-1 base commit categorically, so the "a base-bearing text
   document really does serialize into a major-1 container" observation is
   unreachable there. Under S27 a base commit succeeds or fails **on its
   version**, so the demonstration becomes performable and is owed: with pin 3b's
   text refusal removed, show that a base-bearing document whose raw version
   happens to match the current authority serializes into a major-1 container
   indistinguishable from a validated one. That is the false provenance the text
   refusal exists to prevent, and it has never been observed — only reasoned
   about.

   > **RULED 2026-08-07 in review round 1: the removal is a MUTATION, not a
   > capability restoration.** As written, *"with pin 3b's text refusal removed"*
   > was ambiguous between temporarily lifting the refusal to observe what it
   > prevents, and permanently restoring base-bearing text round-trip. The
   > sentence's own next clause settles it — *"that is the false provenance the
   > text refusal exists to prevent"* — and a guard is not permanently deleted in
   > order to demonstrate why it is needed. **The format rung's text refusal is
   > permanent and S27 does not lift it.** The demonstration is therefore
   > **M7** in §4, removed and restored by hand-editing, and this obligation is
   > discharged there rather than by a test.
   >
   > **Consequences of the ruling, stated so they are not re-litigated:**
   > `COMPANION_VERSION` stays **0.14.0**; `parse.rs`, `vectors.rs` and
   > `spec/text_projection.tex` are **NOT** touched by this rung and are
   > deliberately absent from §2; and the corpus keeps `canonical_bases` reach
   > **0**. A future rung may restore the capability — that is its own contract,
   > with the four touch rows and the companion-version bump this one declines.
   >
   > **`crates/epiphany-textproj/src/lib.rs` was struck from that absent list by
   > amendment 2, and the reason it was ON it must not be lost.** That file is
   > where `COMPANION_VERSION` lives (`:72`) — so its absence from staging *was*
   > the mechanical guard on this ruling's first clause. Touch row 15 stages it for
   > `production_caps`, an unrelated and separately pinned reason, which
   > **silently removed that guard**. The guard is replaced by **gate 4a**, which
   > compares the constant's **value** instead of relying on the file's absence.
   >
   > **"The four touch rows" above still counts `lib.rs`, and that remains
   > correct.** A future restoration rung must touch it *in order to bump*
   > `COMPANION_VERSION`. **A file may be absent from one rung's table and required
   > by another's for an entirely different reason: why a file appears in a table is
   > part of the claim, not incidental to it.** Collapsing this rung's
   > `production_caps` reason into the restoration reason is what produced the
   > contradiction amendment 2 fixes, and re-merging them would reproduce it.

3. **Two conformance assertions come back** (format-rung pin 3c). Criterion 4's
   bookkeeping-projection counterpart, `assert_reduction_serialization_stable`
   (`testkit/src/roundtrip.rs:241`), keeps its serialize → load → decode →
   reserialize cycle through the interval but loses exactly two
   canonical-base-specific assertions: `verify_canonical_chunks`'s base branch
   (`bundle.rs:613`–`:621`, including the `base.hash != base.root.hash`
   cross-check), and the reopened manifest actually carrying the base
   (`roundtrip.rs:293`–`:297`). S27 restores both, since a base-bearing container
   becomes constructible again the moment validation replaces refusal. The
   harness carries a marker naming this contract at the suspension point.

**Pin 3 — `BundleCapabilities`, required at all three constructors, carried on the
`Bundle`. "BOTH" CORRECTED TO "ALL THREE" BY AMENDMENT 3.**

```rust
pub struct BundleCapabilities {
    pub current_reduction_version: ReductionAlgorithmVersion,
}
```

**It MUST NOT implement `Default`, and MUST NOT expose a bundle-local
constant.** Both would let a caller use a bundle without stating what semantics
it implements, which is the defect this rung exists to remove.

`Bundle::open(store, caps)`, `Bundle::create(store, …, caps)` **and
`Bundle::create_versioned(store, …, caps)`** all take it; the `Bundle` **stores
it**. `commit` and `commit_versioned` then validate against `self`'s copy and
**every one of their call sites is unchanged** — the capability is a property of the
session, not of each call. Naming it `BundleCapabilities` rather than
`OpenCapabilities` follows from its now governing creation and commit.

> **TWO CORRECTIONS TO THIS PIN BY AMENDMENT 3, both found by sweeping rather than
> by the round that prompted the amendment. This is the first time the
> third-constructor omission has been found inside a PIN** rather than in prose,
> a gate or a report item — pins are what execution reads, so this one had the
> shortest path to a wrong implementation.
>
> **1. It named two constructors and said "both".** `create_versioned` is a third
> public constructor whose signature this rung changes; `create` delegates to it.
> Five sites of this one omission are now recorded: §0.4 (amendment 1), touch row 13
> and §7 item 5 (amendment 2), the "Rung type" paragraph and **this pin**
> (amendment 3). **Two amendments each declared the omission closed while it was
> live in three more places**, which is the strongest case this document makes for
> its own single-origin rule.
>
> **2. It carried "their 57 call sites are unchanged".** That figure was withdrawn
> from §0.4 by **review round 1** — which found its basis was a `.commit(` token
> grep counting a same-named method in `epiphany-editor-core` — and §0.4 now
> deliberately states **no number at all**, saying only that pin 3's design keeps
> every one of them unchanged. **This pin kept the withdrawn figure for nineteen
> rounds and two amendments.** It is also unreconstructable: the only non-`Bundle`
> `commit` receiver in the workspace is `epiphany-editor-core`'s own method
> (`lib.rs:1404`, 2 sites), so the Bundle-typed count is exact at every tree this
> contract cites — **58** at `381c498`, **60** at `96b40b2`, **60** pre-S27 at
> `HEAD`, **65** in the staged tree. **57 is not any of them.** Amendment 3 removes
> the number rather than replacing it, matching what round 1 did to §0.4.

The struct is a struct rather than a bare parameter so later capabilities append
without another signature break — say so in its doc, and do not add speculative
fields now.

**The stored capability MUST be readable. ADDED IN REVIEW ROUND 3 — this is new
scope, and round 4 should scrutinise it as such.**

```rust
pub fn capabilities(&self) -> &BundleCapabilities
```

Round 3 found that M5a asked for an observation the API cannot make: this pin
said the `Bundle` *stores* the capability and never said anything could *see* it.
`Bundle`'s public surface is `manifest`, `generation`, `header`, `superblock`,
`active_slot`, `file_uuid`, `is_read_only`, `anomalies`, `store`, `into_store`
and the readers — **seventeen accessors, none for capabilities** — so a
`epiphany-textproj` test cannot inspect a private field of a bundle another crate
constructed.

The accessor is justified on its own merits, not only to make a mutation runnable:
the capability **governs rejection behaviour**, and a value that decides whether
`open` and `commit` fail should be inspectable by the caller diagnosing that
failure. It joins the same family as `header()` and `superblock()`.

**Read-only, borrowing, no setter.** A setter would let a caller change the
semantics it claims to implement *after* `open` validated against them, which
reintroduces exactly what pin 3 removes.

**Pin 3a — the writer is validated, not only the reader. RATIONALE CORRECTED IN
REVIEW ROUND 3.**

> **This pin previously read "§0.4 shows production code minting a stale document
> without ever calling `open`" — a sentence §0.4 itself retracts.** Round 2
> restated the justification in §0.4 and left the pin's own copy of it standing,
> so the contract asserted a claim and its negation in two places. **Third
> occurrence of the same meta-defect**: round 1 corrected one spelling of a count
> and left two, round 2 corrected §0.4's table and left the "Rung type"
> paragraph and touch row 2, and round 3 found this. A restatement that does not
> sweep every site restating it has not been made.

**The justification, as it actually stands:** `commit` and `commit_versioned` are
**public API**. An out-of-tree caller can stage a canonical base directly, never
touching `epiphany-textproj` and never calling `open`. There is **no in-tree
production path** that does so — §0.4 verifies zero — so this pin guards an
external surface, not an internal one. That is a narrower claim than the original
and it is the one that survives scrutiny.

Therefore `commit`/`commit_versioned` MUST reject a manifest whose **newly
emitted or replaced** canonical base carries a version differing from
`self.caps.current_reduction_version`, with the same error as pin 4.

"Newly emitted or replaced" is the operative scope: a commit that does not touch
`canonical_base` MUST NOT be refused merely because an inherited base is stale —
that document could not have been opened in the first place, and refusing here
would make an unrelated commit the site of the diagnosis.

**Pin 3b — synthetic capabilities are named, so the choice is not a judgement
call.** Provide a constructor for fixture use named **exactly**

```rust
BundleCapabilities::synthetic_for_fixture(v: u32)
```

> **The name is PINNED, not illustrative. ROUND 17.** This read *"a clearly-named
> constructor … **e.g.** `synthetic_for_fixture`"*, leaving the name to
> execution — while **gate 6a greps for that exact literal**. Any other name and
> the gate returns **0 matches and reports a pass**, having checked nothing.
> **A gate that proves absence is only as good as the name it searches for**, so
> the name it searches for cannot be an example. Renaming it requires amending
> this pin and gate 6a together.

whose doc states it is for
format and container fixtures deliberately exercising arbitrary wire values and
**must never appear in a production composition path**. Production paths wrap
`epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION`.

Every call site converted by this rung uses one or the other **explicitly**;
none may take a value that merely happens to be in scope.

### The real-authority side gets a named constructor. RULED BY THE POST-RATIFICATION AMENDMENT, 2026-08-09.

**The question this settles**, raised by execution and pressed by review: pin 3b
named the *synthetic* constructor and left the **real-authority** side as "wrap
`epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION`", which is a value, not a
name. Execution introduced a `production_caps()` helper per crate to satisfy
"explicitly" without repeating the wrap at every production-authority site —
**unpinned scope**, and in `epiphany-testkit` it landed as new **public** API.

**Ruled: the helpers are SANCTIONED, with visibility pinned per crate.**

| Crate | Visibility | Why |
|---|---|---|
| `epiphany-textproj` | **`pub(crate)`** | A **production** crate. Its five uses are all in-crate, so this rung adds **no public API** to it. `pub` here would be unpinned surface on a shipping crate |
| `epiphany-testkit` | **`pub`** | A test-support crate that exists to be consumed. Its **integration tests and benches are external consumers** (`tests/bundle_reopen.rs` 2 uses, `benches/bundle.rs` 3) and **cannot reach `pub(crate)`**. A capability constructor is precisely what this crate exists to provide |

**Rejected alternative — direct construction at every production-authority site,
of which there are 39. COUNT CORRECTED FROM "23" BY AMENDMENT 2.** It adds no API,
but repeats the same wrap 39 times, and "explicit" was never the same thing as
"repeated". The named helper *is* explicit: it says which side of pin 3b's split
the site is on, and gate 6a can distinguish the two by name.

> **The 39 is DERIVED — and amendment 2's stated derivation was FALSE. CORRECTED
> BY AMENDMENT 3.** It read *"Take §0.4's constructor surface restricted to
> `epiphany-testkit` and `epiphany-textproj` — `open` 19 + 2, `create` 15 + 3"*.
> Those are the **post-S27 staged** counts, presented under §0.4's name: **§0.4's
> table gives `open` 18 + 2 and `create` 14 + 3.** The derivation therefore did not
> reproduce from its cited source, and **§0.4 alone cannot yield 39 at all**,
> because two of the sites are S27's own deliverables. The *result* was right and
> the *derivation* was false, which is the worse of the two failures: a wrong number
> gets caught by recounting, a wrong derivation teaches the next reader to recount
> wrongly.
>
> **The derivation, with the pre/post-S27 transition named and attributed:**
>
> | Leg | `open` | `create` | `create_versioned` | running |
> |---|---|---|---|---|
> | §0.4's **pre-S27** table, `epiphany-testkit` + `epiphany-textproj` | 18 + 2 | 14 + 3 | 1 + 0 | **38** |
> | added by **S27 itself** — both sites in **test 10b**, `roundtrip.rs` | +1 (`:928`) | +1 (`:897`) | — | **40** |
> | less the one site deliberately taking a synthetic capability (`:901`) | — | −1 | — | **39** |
>
> All 38 pre-existing sites convert to `production_caps()`. Of test 10b's two new
> sites, the `create` takes `synthetic_for_fixture(0)` and the `open` takes
> `production_caps()` — so the rung's single synthetic site and one of its two new
> sites are the same site. **Cross-check, independent of §0.4's table:** 42 textual
> `production_caps()` occurrences, less the two definitions and one doc-comment
> mention = **39**.
>
> **The synthetic site is TEST 10b's fixture, not M7's — amendment 2 said "M7's own
> fixture" and that reverses two rulings.** Round 9's finding is *precisely* that
> test 10b is **not** a genuinely validated reference M7 could compare against —
> its write side is synthetic and only its reopen is real — which is why M7 builds
> its own reference, and why round 12 pinned M7's harness to **`epiphany-textproj`**
> rather than letting it reach into `epiphany-testkit`. **No M7 artifact survives in
> the staged tree at all**; M7 is removed and restored by hand. Attributing a
> testkit fixture to M7 would have a reader looking for M7's reference in the crate
> two rounds ruled it cannot live in.
>
> **The likeliest origin of "23", recorded because the shape recurs:** it is
> exactly touch row 2's count of in-crate `open` sites **in `bundle.rs`** — a
> figure about one file and one constructor, restated as a cross-crate total over
> three constructors. **Same shape as §0.4's four instrument failures, and the
> first recorded outside that section:** those are counts taken from one *spelling*
> and generalised; this is a count taken from one *scope* and generalised.

**Consequences, listed rather than left implicit:**

- The helper is named **`production_caps`** in both crates. Like
  `synthetic_for_fixture`, the name is **pinned**, because it is the token that
  distinguishes a real-authority site from a fixture one under review.
- **`epiphany-testkit`'s external consumers use it by path**
  (`epiphany_testkit::production_caps()`); the crate-internal ones use
  `crate::production_caps()`.
- **Touch rows 14 and 15** carry the two crate roots. *(Read "13 and 14" until
  amendment 2 — row 13 is `epiphany-bundle`'s re-export, not a helper root.)*
- **A `pub(crate)` helper in a production crate is not public API**, so gate 5's
  spirit — this rung adds no dependency and no surface `epiphany-ops` must
  honour — is preserved for `epiphany-textproj`.

**Pin 4 — the mismatch is a hard error.**

```rust
BundleError::CanonicalBaseRequiresRebuild {
    base: ReductionAlgorithmVersion,
    current: ReductionAlgorithmVersion,
}
```

A `BundleError`, **not** an `IntegrityAnomaly`, and **not** a read-only
degradation. Read-only is wrong on the merits: a stale base is not a
restricted-but-correct view, it is **the wrong materialization**, and exposing it
read-only would serve incorrect canonical state confidently.

Its doc must record why `open` cannot recover: **`open` cannot rebuild**, and
drop-and-replay is unsound once pruning exists (`core_spec.tex:12207`, `:14701`
— pruning is specified and **not implemented**; no `prune` appears anywhere in
`epiphany-bundle`). A higher-level rebuild path may be authorized later **only
where full pre-base history is demonstrably available**; this rung authorizes
none.

**Pin 5 — the check runs where the current check runs, and only for a base.**
In `open`, at `bundle.rs:396`. A bundle with **no canonical base is openable
regardless** of `caps.current_reduction_version` — there is no reduced state to
be stale. Order the two checks so the corrupt case (pin 6) is distinguishable.

**Pin 6 — the existing corrupt-disagreement failure is preserved, distinctly.**
The current base-vs-superblock disagreement check must keep returning its
existing malformed-bundle `DecodeError`. It is **not** replaced by pin 4 and
**not** merged with it: they detect different things — tampering versus valid
staleness — and collapsing them would lose the distinction §0.1 rests on.

**Pin 7 — `reduction_version_for` does not change, and the reasoning is
recorded.**
It keeps sourcing from the base's self-report — but its justification now rests
on **both** validation points, and the first draft's single-sided reasoning was
insufficient:

- **read-time (pin 5):** an opened bundle has proved `base == current`;
- **write-time (pin 3a):** a newly emitted or replaced base has proved the same.

Only together do these make propagation equal to propagating the current
version. A doc line MUST state both, naming pin 5 and pin 3a — otherwise a later
reader sees an unguarded self-report and either "fixes" it or trusts it in a
context where neither check has run. §0.4's correction is exactly what happens
when that reasoning is done one-sided.

**Pin 8 — `ids.rs:288`–`:289` becomes true.**
Repair the catalog claim to name the real location and the real mechanism: the
authoritative version lives in `epiphany-ops` as a plain `u32`; the wrapper type
is constructed at the composition boundary; `epiphany-bundle` deliberately does
not depend on `epiphany-ops`. This is the rung that earns the sentence.

**Pin 9 — normative specification text.**
`core_spec.tex` Chapter 8 §"Canonical Document Identity" currently states the
rebuild requirement (`:11614`) with no error surface. Add the rejection
behaviour normatively, and a Revision History row. There is **no** existing
"requires rebuild" error language in either document — verified — so this is new
prose, not an amendment.

**Whether this mints a new `\label{req:...}` MUST be decided explicitly, and
stated in the report. AMENDED 2026-08-07 in review round 1.** The pin said "add
the rejection behaviour normatively" without saying whether the behaviour gets
its own requirement label, and the two readings have different touch tables:

- **If it mints a label**, `core_spec.tex`'s requirement count moves 213 → 214
  and `crates/epiphany-testkit/tests/requirement_labels.rs` **must** change —
  `CORE_REQUIREMENT_COUNT = 213` (`:15`) is hardcoded and currently matches the
  tree exactly. **Touch row 12 carries it.**
- **If it does not** — the prose lands under an existing requirement — row 12 is
  unused and the report says so.

**This is the escapee `CLAUDE.md` names by name**, and it escaped the
format-epoch rung's touch table too. A file that must change but is not listed
**silently drops out of the commit**, and the resulting failure surfaces on
someone else's branch.

**Pin 10 — the ledger. AMENDED 2026-08-07, before dispatch, on a finding from
the machine-move review.**
`spec/PASS13_CANDIDATES.md`: P13-S27 → RESOLVED, recording both rulings, the
baseline-0 decision, the writer-path correction of §0.4, and **whichever
legacy-base disposition pin 2a settles on** — which, as of 2026-08-07, is the
container epoch, settled from outside this rung.

**The original clause, retained for the record:** *"P13-S16's row does NOT become
dispatchable on this rung alone. It moves from 'blocked on P13-S27' to 'blocked
on the pin-2a legacy disposition' unless 2a is ratified and tested within this
rung, in which case it opens and its row records that its first act is bumping
the authority past the baseline."*

**Why it is amended.** Pin 2a *was* ratified — through the format-epoch
contract's four adversarial rounds — but **outside** this rung, which is exactly
what pin 2a's own prohibition demanded: it forbade being amended into a
disposition from inside this contract. So the clause's literal condition
("within this rung") could never be satisfied by the only route pin 2a permitted,
and read literally pin 10 would have the ledger record S16 as still blocked on a
disposition that is settled. The condition was written when an inside resolution
still looked possible.

**As amended:** P13-S16's row becomes **dispatchable when this rung lands**. Pin
2a's disposition is settled, so the row moves from "blocked on P13-S27" straight
to open, and records that its first act is bumping the authority past the
baseline.

**The "tested" half is not waived by this amendment.** Ratification came from
outside; testing did not, and cannot — no other rung exercises this authority.
It is discharged by this rung's own inherited obligations, in particular
converting **both** interim refusals to real capability validation. **The ledger
row may not record S16 as open until those land with this rung**; ratification of
2a alone does not open it.

---

## §2. Touch table

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-ops/src/lib.rs` (or a new `reduction.rs`) | pins 1, 2 |
| 2 | `crates/epiphany-bundle/src/bundle.rs` | pins 3, 3a, 3b, 5, 6, 7 + **23** in-crate `open` sites + 11 in-crate `create` sites. *(Was "35 opens" — corrected in review round 2. The 35 was never `bundle.rs` alone: it was `bundle.rs` 20 + `fuzz.rs` 15 at `381c498`, and `fuzz.rs` has its own row 5. Now 23 after the format rung added 3.)* |
| 3 | `crates/epiphany-bundle/src/error.rs` | pin 4 |
| 4 | `crates/epiphany-bundle/src/ids.rs` | pin 8 |
| 5 | `crates/epiphany-bundle/src/fuzz.rs` | **15** `open` sites + **1** `create` site. *(Figures stated in review round 2; row 2's old "35" silently included these, so the two rows together could not be reconciled against §0.4.)* |
| 6 | `crates/epiphany-bundle/tests/{crash_recovery,manifest_selection}.rs` | call sites |
| 7 | `crates/epiphany-testkit/src/{bundle_harness,roundtrip}.rs` | call sites (11 + 4 `open`, 6 + 6 `create`), real authority |
| 7a | `crates/epiphany-testkit/src/generators.rs` | **NOT a call site — corrected in review round 3.** It has **zero** `Bundle::open`/`Bundle::create` calls; row 7 previously swept it in as "call sites, real authority" and was wrong on both counts. Its actual role is `:1628`/`:1651`, which mint manifests carrying `ReductionAlgorithmVersion(rng.range(0, 8))` — **arbitrary wire values, so per pin 3b their consumers take `synthetic_for_fixture`, never the real authority.** It changes **only** if the generated version must be surfaced so a caller can build a matching synthetic capability. **State in the report whether it changed and why; if it did not, it must not be staged.** |
| 8 | `crates/epiphany-testkit/tests/bundle_reopen.rs`, `benches/bundle.rs` | call sites |
| 9 | `crates/epiphany-textproj/src/{serialize,project}.rs` | call sites, real authority |
| 10 | `spec/core_spec.tex` (+ `.pdf`) | pin 9 |
| 11 | `spec/PASS13_CANDIDATES.md` | pin 10 |
| 12 | `crates/epiphany-testkit/tests/requirement_labels.rs` | **conditional** — pin 9, *only if* it mints a new `\label{req:...}`. **ALL THREE counters move, not one** (amended 2026-08-09): `CORE_REQUIREMENT_COUNT` (`:15`) 213 → 214, **and** `SUITE_REQUIREMENT_COUNT` (`:18`) and `SUITE_LABEL_COUNT` (`:19`) 284 → 285, because the suite totals include `core_spec`'s requirements and its labels. The original instruction named only the first; execution found the other two through **four** failing tests in this file. If pin 9 mints no label, leave unmodified and say so in the report |
| 13 | `crates/epiphany-bundle/src/lib.rs` | **ADDED 2026-08-09.** Pin 3's `BundleCapabilities` is required at `open`, `create` **and `create_versioned`**, so **the two crates outside `epiphany-bundle` that call them need it re-exported** — `epiphany-testkit` and `epiphany-textproj` cannot name a type this crate does not export, and each needs it for its `production_caps` signature alone. Unavoidable, and carried by no row through nineteen rounds. **This row carries NO site count, deliberately — amendment 2.** It read *"92 converted sites across three crates"*, which is 60 `open` + 32 `create`: **the two-constructor figure, missing `create_versioned`'s 3, inside the very amendment added to close that omission.** The complete pre-S27 surface is **95**, and §0.4 is its single origin. The quantity this row actually needs is **two crates**; any figure ≥ 1 makes the re-export unavoidable, so a count here could only rot |
| 14 | `crates/epiphany-testkit/src/lib.rs` | **ADDED 2026-08-09.** `pub fn production_caps()` — pin 3b's real-authority constructor, `pub` because this crate's integration tests and benches are external consumers |
| 15 | `crates/epiphany-textproj/src/lib.rs` | **ADDED 2026-08-09.** `pub(crate) fn production_caps()` — same constructor, **crate-private** because this is a production crate and all its uses are in-crate |
| 16 | `crates/epiphany-testkit/src/gminor.rs` | **ADDED 2026-08-09.** One `Bundle::create_versioned` site (`:85`). Missing because §0.4 never counted that constructor — see the amendment note there. This file calls **only** `create_versioned`, so no surface count reached it |

**Row 12 is conditional, and that is deliberate.** `CLAUDE.md` names this file as
a recurring escapee, and it escaped the format-epoch rung's table. Carrying it
conditionally costs nothing if unused; omitting it costs a silent drop-out.

### Deliberately ABSENT — TWO DISTINCT RULES. SEPARATED BY AMENDMENT 2.

This was **one sentence conflating two unrelated rules**, and it listed
`crates/epiphany-textproj/src/lib.rs` under both — putting it in direct
contradiction with **touch row 15, added by amendment 1 seven lines above.** The
staged tree satisfied the table and violated the sentence, and the execution report
checked only the table.

**Rule A — the M7 restoration tripwire.** M7 removes refusals temporarily and
restores them by hand-editing. Of the files it edits, `parse.rs` (`:138`–`:147`) is
in **no** touch row, so **if `crates/epiphany-textproj/src/parse.rs` appears in
`git diff --cached`, M7 was not restored** and gate 4 must fail.

> **This tripwire covers ONE of M7's three edit sites, and saying so is the whole
> point of stating it separately.** M7 also edits `serialize.rs:151` and the
> export-leg refusal in `document_from_bundle` (`project.rs:480`) — and **both those
> files are legitimately staged under touch row 9.** *Absence cannot test a file that
> is supposed to be present.* For those two, restoration is confirmed by **reading
> the staged diff** (§7 item 6), never by absence. A tripwire that covers one site in
> three while reading as though it covers all three is the `head`-truncated universal
> negative in another costume: it returns a clean result for the two cases it cannot
> see.

**Rule B — the declined capability restoration.** This rung does **not** restore
base-bearing text round-trip. So `spec/text_projection.tex` and
`crates/epiphany-textproj/src/vectors.rs` are absent from §2, and `COMPANION_VERSION`
does not move.

> **`crates/epiphany-textproj/src/lib.rs` is NOT in rule B's list, though it was
> until amendment 2** — row 15 stages it for `production_caps`. But
> `COMPANION_VERSION` **lives in that file** (`:72`), so its absence *was* rule B's
> only mechanical guard. **Gate 4a replaces that guard by value comparison**, and
> rule B is unenforced without it.

---

## §3. Required tests

> **Header corrected 2026-08-07 in review round 1.** It read *"(pin 4's ruling
> names all four)"* while the list below carried seven items. The count went
> stale twice — test 5 was added when the first draft's writer-path omission was
> found, then tests 6 and 7 with pin 2a's resolution — and gate 1 inherited the
> stale figure. **Do not reconcile against a fixed number**; see gate 1.

Named and permanent. **Where each test lives — corrected twice, in rounds 5 and
6:**

| Tests | Crate | Why there |
|---|---|---|
| 1–6, 8, 9 | `epiphany-bundle` | they exercise `open`/`commit` with synthetic capabilities and need no real authority |
| **7**, 10b | `epiphany-testkit` | 7 *is* `assert_reduction_serialization_stable`; 10b needs the real constant |
| 10a | `epiphany-textproj` | it asserts what the production writer supplied |

> **Two wrong versions of this sentence, both introduced while fixing it.** It
> originally read "in `epiphany-bundle`" for everything. Round 5 corrected it to
> "tests 1–9 in `epiphany-bundle`" — **still false, because test 7 is
> `assert_reduction_serialization_stable`, which the same round's own §3 entry
> names as `testkit/src/roundtrip.rs`.** Round 6 caught it. A correction that
> restates a range without checking each member of it is a guess with a citation
> attached.

**Why 10a and 10b cannot live in `epiphany-bundle`:** that crate must not depend
on `epiphany-ops` (pin 1, §0.3), so no test in it can reach
`CURRENT_REDUCTION_ALGORITHM_VERSION` — and reaching the real authority is their
entire purpose.

**Touch-table homes:** 10a lands in row 9's `textproj/src/serialize.rs`; 7 and
10b in row 7's `testkit/src/roundtrip.rs`. All rows already exist and none needs
widening — **state in the report which file each landed in**, since a test placed
outside its row silently drops out of the commit.


### Every base-bearing test MUST assert the base is present. ADDED IN ROUND 17

**Tests 1, 6, 7, 8 and 9 all describe base-bearing scenarios, and none of them
required checking that a base was there.** Pin 5 makes base-free the **permissive**
case — a bundle with no canonical base **opens at any authority** — so a fixture
that silently ends up base-free makes every one of those tests pass **trivially**,
asserting nothing. **Test 1 degenerates into test 4.**

**That is not a hypothetical fixture slip.** `create` rejects a base-bearing
manifest outright (`bundle.rs:234`), and for the whole S28 → S27 interval a
base-bearing `Bundle` has been **unconstructible** through the public API (§1.2).
Base-bearing fixtures are the awkward ones to build, so **the failure mode is the
path of least resistance**.

**Each of tests 1, 6, 7, 8 and 9 MUST assert the base's presence explicitly, on
the bundle under test.** The required assertion is **not the same for all of
them** — **CORRECTED IN ROUND 18**, which caught this rule demanding the opposite
of what test 8 exists to do:

| Test | Before | After |
|---|---|---|
| 1, 6, 7 | `canonical_base.is_some()` | — (no commit) |
| **8** | **`is_none()`** — it **introduces** the base; that is the whole test | `is_some()` |
| 9 | `is_some()` — it opens a base-bearing bundle | `is_some()`, and the base **unchanged** |

> **Round 17 wrote "before *and* after for tests 8 and 9, which commit",
> collapsing two opposite fixtures because both happen to commit.** Test 8
> commits a base **into a bundle that has none**; test 9 commits something
> **unrelated to a base that is already there**. Requiring `is_some()` before test
> 8's commit would make it **unsatisfiable** — or, worse, satisfiable by a fixture
> that already had a base, in which case the commit introduces nothing and the
> test asserts nothing. **Grouping by mechanism (both commit) rather than by
> what each is for is what produced the error.**

**A test that cannot state which bundle it asserted this on has not made the
assertion.**

*(Stated once here rather than repeated per test — round 7's rule. The tests below
do not restate it.)*

1. **`open_succeeds_when_base_and_current_reduction_versions_match`** — **the
   commit construction route. ROUTES SWAPPED IN ROUND 18.** Create a bundle with
   synthetic `caps(N)`, commit a canonical base carrying `N`, take the bytes, and
   reopen under `caps(N)`. It shares test 8's setup and asserts a different thing:
   test 8 asserts the **commit** succeeds, test 1 asserts the **reopen** does.
2. **`open_rejects_a_valid_stale_canonical_base`** — base and superblock agree
   with each other, `caps.current` differs. Must return
   `CanonicalBaseRequiresRebuild { base, current }` with **both fields
   asserted**, not merely the variant.
3. **`a_corrupt_base_superblock_disagreement_still_fails_as_malformed`** — the
   pin-6 path, asserted to be the *existing* malformed error and **not**
   `CanonicalBaseRequiresRebuild`.
4. **`a_bundle_with_no_canonical_base_opens_at_any_reduction_version`** — run at
   two `caps` values that the test **asserts are unequal**. **ROUND 17:** it
   previously said only "two different values"; if they coincide the test proves
   nothing about *any*, and nothing would say so. Assert the inequality, and
   assert the fixture is base-free — this is the one test whose scenario requires
   `canonical_base.is_none()`, the mirror of the rule above.
5. **`committing_a_stale_canonical_base_fails_and_leaves_the_prior_generation_reopenable`**
   — the pin-3a writer test, and the one this contract's first draft omitted
   entirely. Create a bundle, commit a good generation, then attempt a commit
   whose newly emitted canonical base carries a version differing from `caps`.
   Assert **both**: the commit fails with `CanonicalBaseRequiresRebuild`, **and**
   the bundle reopens at the prior active generation with its earlier content
   intact. A writer check that corrupts the document while refusing is worse than
   no check.

**Added 2026-08-07 with pin 2a's resolution — inherited obligations 1 and 3,
stated as tests so they cannot be discharged by prose.** *(Corrected in review
round 1: this preamble previously claimed **all** the inherited obligations were
stated as tests. Obligation 2 was in neither §3 nor §4 — it is now **M7**, by
the ruling recorded beside it.)*

6. **`a_major_1_bundle_carrying_a_base_opens_when_the_authority_matches`** — the
   read-side half of the format rung's pin 3a, converted from temporary refusal
   to real validation. Its sibling is test 2, which is the same path when the
   authority *disagrees*. Both branches must exist; the format rung's own review
   found a draft that closed only one.

   **How this differs from test 1, which asserts the same outcome. ROUND 17
   flagged them as possible duplicates; they are kept distinct, and here is the
   distinction they must actually carry. ROUTES SWAPPED IN ROUND 18.**

   **Test 6 builds its image by hand**, with
   `craft_image_with_base(1, 0, ReductionAlgorithmVersion(N), ReductionAlgorithmVersion(N))`,
   and exercises `open` in isolation from any commit path. **Test 1 reaches the
   same state by the commit path.**

   > **Why this way round, and not the way round 17 had it.** Test 6 is the
   > *conversion* of `opening_a_major_1_bundle_that_already_carries_a_base_is_refused`
   > (`bundle.rs:1866`), and **that test hand-builds its image** — it calls
   > `craft_image_with_base(1, 0, ReductionAlgorithmVersion(0), ReductionAlgorithmVersion(0))`
   > at `:1869`. Round 17 assigned test 6 the **commit** path while also requiring
   > its fixture to "arrive the way that test's did", **which are contradictory**:
   > the test it converts is hand-built.
   >
   > Swapping the routes makes the attribution **true** rather than deleting it.
   > Test 6 keeps its ancestor's construction and changes only its *assertion* —
   > refusal becomes validation, which is exactly what "conversion" should mean.
   > `craft_image_with_base` takes `format_major` as its first argument, so it
   > builds the major-1 image test 6's name requires.

   **The two construction routes are the point, and the probe is why.** The
   scratch probe established that **how an artifact is constructed changes its
   bytes** — a hand-built and a committed bundle are not interchangeable, and one
   can be a fixed point where the other is not. Two routes into the same
   guarantee is real coverage, not redundancy.

   **If execution finds the routes collapsing into the same construction, that is
   a finding**: say so rather than quietly writing one test twice.
7. **The two restored conformance assertions** (format-rung pin 3c), in
   `assert_reduction_serialization_stable` (`testkit/src/roundtrip.rs:241`):
   `verify_canonical_chunks` covering the base again, and the reopened manifest
   carrying it. **Restoring them means deleting the suspension marker** that
   names this contract — if the marker is still in the tree when this rung
   reports, the restoration did not happen.

8. **`committing_a_canonical_base_succeeds_when_the_authority_matches`** —
   **ADDED 2026-08-07 in review round 1.** Obligation 1 warns that converting
   only one branch "leaves a hole exactly where the format rung's own review
   found one", and the write-side **positive** branch had no test: test 1 is
   read-positive, test 2 read-negative, test 5 write-negative. The missing branch
   is precisely where the format rung's temporary refusal sits
   (`bundle.rs:795` → `ReductionAuthorityUnavailable`, asserted by
   `a_major_1_bundle_round_trips_and_refuses_to_introduce_a_base` at `:1787`).
   Without this test, an implementation that converts the read side and leaves
   `commit` refusing categorically passes every other test in this section.

9. **`an_unrelated_commit_on_a_base_bearing_bundle_succeeds`** — **ADDED IN
   REVIEW ROUND 3.** Open a bundle carrying a base whose version matches `caps`
   (the state test 6 establishes), then commit something that **does not touch
   `canonical_base`**, and assert it **succeeds** and the bundle reopens with
   both the new content and the untouched base.

   **Why it is required, and why M6 alone was not enough.** M6's replacement
   broadens pin 3a and says an unrelated commit "starts failing" — but a mutation
   only demonstrates; it does not *pin*. Test 6 stops at opening. So with tests
   2, 5, 6 and 8 alone, **an implementation that rejects every post-base
   unrelated commit passes all of them**, and M6's broadening would have nothing
   to break because the unbroadened behaviour was never asserted. This test is
   the permanent statement of pin 3a's "newly emitted or replaced" scope; M6 is
   only its mutation.

   **This is the third distinct way a §4 mutation has been found unrunnable** —
   M5a had no observation mechanism, M5b could not fail, and M6 had nothing
   asserting the behaviour it breaks. **A mutation is only as good as the test it
   breaks**, and §4 must name that test for every entry.

**Added in review round 4 — the two tests M5a and M5b break. Round 3 wrote both
mutations without them, in the same edit that added §7 item 4a requiring them.**

10a. **`serialize_document_supplies_the_real_reduction_authority`** — in
   `epiphany-textproj`. Serialize a base-free document and assert
   `bundle.capabilities().current_reduction_version == ReductionAlgorithmVersion(0)`.

   **The `0` MUST be a literal, and the test MUST carry a comment saying why.**
   Comparing against `CURRENT_REDUCTION_ALGORITHM_VERSION` would compare the
   constant with itself and hold for every value — the assertion would be
   unfalsifiable and M5a could not break it. **This test is expected to fail when
   S16 bumps the authority**, and that is correct: the literal is a tripwire on
   the production wiring, and S16 updating it is S16 stating that it moved.

10b. **`a_base_bearing_bundle_reopened_under_the_real_authority_validates`** — in
   `epiphany-testkit`, which may use the real constant. Build the fixture with
   `synthetic_for_fixture(0)`, commit a base carrying the literal
   `ReductionAlgorithmVersion(0)`, take the bytes, and reopen them with `caps`
   built from `CURRENT_REDUCTION_ALGORITHM_VERSION`.

   **It MUST match on the reopen's `Result` explicitly, with both arms written.
   PINNED IN ROUND 5 — "assert it opens" was not enough.**

   ```text
   match reopen {
       Ok(bundle)  => // the unmutated path: assert the base survived
       Err(BundleError::CanonicalBaseRequiresRebuild { base, current }) =>
           // assert base == ReductionAlgorithmVersion(0)
           // assert current == the mutated authority
           // THEN fail, quoting both fields
       Err(other) => // fail: the wrong error, quoting it
   }
   ```

   > **Why the `Err` arm is required even though the test asserts success.**
   > M5b must observe `CanonicalBaseRequiresRebuild { base, current }` **with both
   > fields asserted**. Round 4 specified only "assert it opens", which under
   > mutation yields a bare `Err` or a panic — **a `#[test] -> Result` that
   > returns `Err` is not an assertion about that error's fields.** The two-field
   > observation M5b demands had no home in the test M5b names. The `Err` arm is
   > that home: it runs only under mutation, and it is what makes M5b's required
   > output a *verified* observation rather than a stack trace.
   >
   > The third arm is not padding. Without it, an implementation returning a
   > *different* error under mutation still fails the test, and the mutation
   > report would read as success while observing nothing.

   **This is the only place in the rung where the real authority meets a
   canonical base**, which is why M5b needs it and why no existing test could
   serve. It must be a named test returning a matchable `Result`, **not** an
   assertion inside a void conformance helper.

**Every fixture literal in 10a and 10b is load-bearing as a literal.** A later
reader "tidying" any of them into `CURRENT_REDUCTION_ALGORITHM_VERSION` makes the
corresponding mutation vacuous while leaving every test green. **Give each one a
doc comment saying so.**

> **How many there are, and where that is recorded: §7 item 4b — which
> enumerates them, and is the only place they are counted.** This paragraph said
> "**both** literals … tidying **either**" from round 4 until round 7, a
> two-literal framing that round 6 corrected **in §7 and not here**, leaving the
> contract carrying both the fixed and the broken version of the same claim.
> **It is not restated here on purpose.** The recurring defect across rounds 5, 6
> and 7 is a claim living in two places and being fixed in one; the remedy is a
> single home and a pointer, not a second copy kept in step by vigilance.

Tests 2 and 3 must be **paired in review**: each asserts the other's error is
*not* produced. A test that only checks its own variant cannot show the two
paths are distinguishable, which is the whole point of pin 6.

Tests 6 and 2 stand in the same relation to each other. **So do tests 8 and 5**,
on the write side.

**Several of these CONVERT existing tests rather than adding new ones — found in
review round 1, and the reason gate 1 no longer names a number.** The format rung
left two tests asserting the interim refusal, and S27 turns each into a matched
pair:

| Existing test | Becomes |
|---|---|
| `opening_a_major_1_bundle_that_already_carries_a_base_is_refused` (`bundle.rs:1866`) | tests **6** and **2** |
| `a_major_1_bundle_round_trips_and_refuses_to_introduce_a_base` (`:1787`) | tests **8** and **5** |
| `a_corrupt_base_fails_as_malformed_before_any_epoch_error` (`:1840`) | extended into test **3** (adds the "*not* `CanonicalBaseRequiresRebuild`" assertion) |

**And `ReductionAuthorityUnavailable` is deleted by this rung**, so every site
naming it must move or the crate will not compile: `error.rs:152` (variant),
`:232` (Display arm), `bundle.rs:422` and `:795` (construction), the doc comments
at `:1538` and `:1844`, and **five test assertions** at `:1740`, `:1774`,
`:1834`, `:1861`, `:1879`. The three negative assertions (`:1740`, `:1774`,
`:1861`) exist to prove the legacy and corrupt paths do **not** produce it — they
must be re-pointed at the error that replaces it, **not** deleted, or the
distinction they were written to hold is lost.

---

## §4. Mutation plan

Applied, **run**, output recorded verbatim, restored **by hand-editing back**.

**M1 — the new check fires. OBSERVATION TIGHTENED IN ROUND 16.** Delete pin
5's comparison. **Test 2 failing is necessary and NOT sufficient**: an unrelated
open error can break its assertion without showing the capability check was the
thing removed.

**Observe the mutated outcome itself:** the structurally valid, self-consistent
base whose version differs from `caps.current_reduction_version` must be seen to
**OPEN SUCCESSFULLY**. Report that open result and the deliberately mismatching
base/current values.

**M2 — the paths are distinguishable. OBSERVATION TIGHTENED IN ROUND 16.** Make
pin 4's error subsume pin 6's (return `CanonicalBaseRequiresRebuild` for the
corrupt case too). **Test 3 failing is necessary and NOT sufficient**: any result
other than the expected malformed error breaks it, without proving corruption was
misclassified as staleness.

**Observe the mutated outcome itself:** the corrupt base/superblock-disagreement
fixture must be seen returning **`CanonicalBaseRequiresRebuild { base, current }`**.
Report both fields. That is the reclassification this mutation is meant to make,
not merely a broken test assertion.

**M3 — the no-base exemption is deliberate. OBSERVATION TIGHTENED IN ROUND 16.**
Make the check run when `canonical_base` is `None`. **Test 4 failing is necessary
and NOT sufficient**: an error on either attempt can break it without showing the
no-base exemption was removed.

**Observe the mutated outcome itself:** the base-free fixture, opened under the
deliberately mismatching capability from test 4, must be seen **rejected by the
wrongly widened check with `CanonicalBaseRequiresRebuild { base, current }`**.
For this mutation only, the `base` field is the base-free superblock's
`reduction_algorithm_version` — **`ReductionAlgorithmVersion(0)`**, obtained from
`reduction_version_for`'s no-base default. The `current` field is the deliberately
mismatching capability. Report both values and the otherwise successful
matching-capability open. **This synthetic source exists only to make the wrong
mutation observable; it MUST NOT enter shipped no-base validation.**

**M4 — the capability is genuinely required.** Add `impl Default for
BundleCapabilities` and a call site using it. **This must be observed to
compile**, then reverted — it demonstrates what pin 3 forbids and why the
prohibition needs to be a review rule, since no test can catch a `Default` that
callers then use. Report it as a *prohibition recorded*, not a guard.

**M5 — SPLIT IN REVIEW ROUND 2. It was unexecutable as written.**

> **Why.** It required changing `CURRENT_REDUCTION_ALGORITHM_VERSION` and
> observing a **production composition path** test fail — naming `textproj`'s
> round trip. But `serialize_document` refuses base-bearing documents at
> `serialize.rs:151`, *before* `Bundle::create`, so its output is **necessarily
> base-free**; and pin 5 with test 4 require a base-free bundle to open at **any**
> authority. The mutation therefore cannot make that test fail — not because the
> wiring is absent, but because the path carries nothing for the authority to
> check. Round 1 introduced this by ruling M7's refusal permanent and not
> re-deriving M5 against it.

The original intent had two halves. They are now separate mutations, because no
single path carries both any more.

**M5a — the constant is wired into production. OBSERVATION MECHANISM PINNED IN
REVIEW ROUND 3.**

> **As written in round 2 this could not be run.** It said "observe that the
> `BundleCapabilities` `serialize_document` constructs changes with it" without
> specifying *how* anything observes a capability stored on a bundle and exposed
> by nothing. **Pin 3 now requires `Bundle::capabilities()`**, and that accessor —
> not a temporary instrument — is the observation mechanism. A mutation whose
> observation depends on scaffolding that is not in the shipped tree observes the
> scaffolding, not the tree.

**The test it breaks is test 10a** (§3), and the comparison **MUST be against a
literal, not against the constant. CORRECTED IN ROUND 4.**

> **Round 3 wrote "assert that the value moved with the constant", which is
> unfalsifiable.** Asserting `capabilities() == CURRENT_REDUCTION_ALGORITHM_VERSION`
> compares the constant with itself laundered through one function call: mutate
> the constant and **both sides move**, so the assertion holds for every value.
> That is the same tautology round 3 caught in M5b, in the mutation *next to it*,
> written in the same edit. Round 3 also violated its own new §7 item 4a by
> naming no test at all.

Change `CURRENT_REDUCTION_ALGORITHM_VERSION` from `0` to any other value. **Test
10a failing is necessary and NOT sufficient**: a serialization failure would
break its assertion without showing the production path reads the authority.

**Observe the mutated outcome itself:** `serialize_document` must return a bundle
whose stored `capabilities().current_reduction_version` equals the deliberately
changed authority. Report that value beside the test's deliberate literal
`ReductionAlgorithmVersion(0)`; test 10a then fails for that specific mismatch.

Assert on the capability **only** — not on an open or commit outcome. There is no
base, so no check fires and none should. **If an open or commit outcome moves
here, the production path is doing something this contract does not authorize,
and that is a finding.**

**M5b — the authority is load-bearing where a base exists. REWRITTEN IN REVIEW
ROUND 3; as written it could not fail.**

> **Round 3's diagnosis was right and its evidence was wrong. CORRECTED IN ROUND
> 4.** Round 3 said the base version was stamped at `roundtrip.rs:367`. **It is
> not.** `:367` sits inside `assert_score_serialization_stable` (`:332`) and
> pushes to **`acceleration_snapshots`** — a different harness and a different
> field. `assert_reduction_serialization_stable` (`:255`) has **no canonical base
> at all** today; pin 3c suspended it, and the harness reads its snapshot chunk
> directly by `ChunkRef`. So the value round 3 told the implementer not to touch
> has nothing to do with the authority check, and mutating it could not have
> failed anything.
>
> **How the error was made, since it is this rung's own subject:** round 3 grepped
> `ReductionAlgorithmVersion` across `testkit/src/`, saw a `roundtrip.rs` hit, and
> attributed it to the function it was already thinking about **without resolving
> the enclosing item**. That is the fourth instrument failure in this contract and
> the second of exactly this shape — §0.4's `.commit(` miscount was the first.
>
> **The underlying tautology finding stands.** If the supplied capability and the
> base version both descend from `CURRENT_REDUCTION_ALGORITHM_VERSION`, both
> operands move together and the comparison passes for every value — §0.1's
> defect inside the mutation built to detect it. Only the cited evidence was
> wrong.

**The instrument is CHOSEN, not offered. ROUND 4.** Round 3 said "the rung picks
one" and named two routes, which is not a decision — and one of them does not
exist: **`craft_image_with_base` is a private `fn` inside `epiphany-bundle`'s
`#[cfg(test)]` module (`bundle.rs:1648`, module opens at `:1407`)**, so
`epiphany-testkit` cannot call it. The routes also carry different fixture and
touch-table consequences, so leaving the choice to execution would have put a
design decision in the implementer's hands.

**The chosen instrument — commit-then-reopen, entirely through public API:**

1. Build a bundle with `caps = synthetic_for_fixture(0)` and commit a canonical
   base whose version is the **deliberate literal** `ReductionAlgorithmVersion(0)`.
   This succeeds by test 8's path.
2. Take the bytes.
3. **Reopen them with `caps` built from the real
   `CURRENT_REDUCTION_ALGORITHM_VERSION`.**

Unmutated, the real constant is `0`, the operands agree, and the bundle opens.
Mutated, the constant is not `0`, and the reopen fails with
**`CanonicalBaseRequiresRebuild { base: 0, current: <mutated> }`**.

**Why this shape and not another:** the two operands are independent **by
construction at the time of writing** — one is a synthetic literal written into a
fixture, the other is the real constant read at the reopen. It needs no private
helper, no new fixture file, and no touch-table row. It isolates the real constant
on the **read** side only, so pin 3a's writer check cannot fire first and mask
the result.

> **What this does NOT give you, corrected in round 5.** Round 4 claimed the
> literals "cannot be tidied into the other without deleting the synthetic
> capability the fixture is built on." **That is false.** A later edit can keep
> `synthetic_for_fixture` exactly where it is and pass
> `CURRENT_REDUCTION_ALGORITHM_VERSION` as *both* its argument and the base
> version — the fixture still looks synthetic, every test still passes, and the
> tautology is fully restored. **The structure does not protect itself.**
>
> The only real protection is **§7 item 4b**, which requires positively
> confirming the literals are still literals. Round 4 overstated a structural
> guarantee and thereby weakened the case for the procedural check that is
> actually doing the work — the same error as trusting a mutation because it
> looks like it should fail.

**The test it breaks is test 10b** (§3) — a **named** test, not the void
conformance helper. **ROUND 4:** round 3 nominated
`assert_reduction_serialization_stable`, which returns `()` and whose reopen is
`.expect("reopen bundle")` (`roundtrip.rs:292`). A mismatch there **panics**; it
cannot match on `CanonicalBaseRequiresRebuild { base, current }`, so the required
two-field assertion was impossible in the nominated site.

**Report the provenance of both operands.** Naming where each came from is the
only way to show they are independent, and that independence is the whole content
of this mutation.

**Preserved from the original M5:** if only `synthetic_for_fixture` tests move,
pin 3b has been applied backwards, and that is a finding.

**Both halves are required.** M5a alone shows the constant is read but never that
it matters; M5b alone shows it matters but never that production reads it. The
original mutation conflated the two because, at `381c498`, one path did both.

**M6 — the writer check fires. OBSERVATIONS TIGHTENED IN ROUND 15: a failing test
is not the evidence; the changed behaviour is.**

Remove pin 3a's commit-side validation. **Test 5 failing is necessary and NOT
sufficient** — it fails for any error at all, including an unrelated writer
rejection that has nothing to do with pin 3a.

**Observe the mutated outcome itself:** test 5's stale commit must be seen to
**SUCCEED**, and the bundle must **reopen at the new generation with the stale
base present**. That — not the absence of the old error — is what shows the check
was the only thing refusing it.

**Second half REPLACED IN REVIEW ROUND 2. It was unexecutable as written.**

> **Why.** It asked for pin 3a to be narrowed to refuse *any* stale **inherited**
> base, then for an unrelated commit on an already-open bundle to start failing.
> That state cannot be constructed: `open` rejects a stale base (pin 5, test 2),
> `create` rejects a base-bearing manifest outright (`bundle.rs:234`), and a
> successful `commit` validates the base it emits. **No caller can hold an open
> `Bundle` whose inherited base is stale**, so the mutation has nothing to
> observe. Round 1 added test 8 on the write side without re-deriving M6 against
> the same reachability.

**As replaced — broaden rather than narrow.** Widen pin 3a to refuse a commit on
a **base-bearing bundle regardless of whether the version matches**, then observe
**test 9** — `an_unrelated_commit_on_a_base_bearing_bundle_succeeds`. That state
**is** reachable (test 6 establishes it), so the mutation runs, and it signs the
same thing the original was reaching for: that *"newly emitted or replaced"* is a
deliberate scope and not an accident of where the check was placed.

**Test 9 failing is necessary and NOT sufficient — ROUND 15.** Observe the
mutated outcome itself: test 9's commit, **otherwise unchanged and not touching
`canonical_base`**, must be seen **rejected specifically by the broadened writer
rule** — the error the broadening introduces, on the inherited-base path, named in
the report. **Not merely "test 9 now errors."**

> **Why both halves needed this. ROUND 15 applied round 13's question to M6 —
> "what else, besides the intended defect, would make this pass?"** Both halves
> accepted *a failing test* as the observation, and **a test fails for every
> reason, not only the one under test.** An unrelated writer rejection satisfies
> "test 5 must fail" and "test 9 starts failing" exactly as well as the intended
> cause does, so the mutation could report success while demonstrating nothing
> about pin 3a's scope. **The evidence a mutation owes is the behaviour it
> changed, not the assertion it broke.**
>
> This is the same shape as round 13's M7 finding — *an observation satisfiable
> by something other than the thing observed* — and it was found by the scan
> round 13's finding implied and round 14 did not run. **M1–M5b survive the same
> scan.**

> **Test 9 was added in round 3 for exactly this reason.** Round 2 wrote this
> replacement naming a *scenario* and no *test*, so nothing asserted the
> unbroadened behaviour and the broadening had nothing to break.

**Record alongside it** that the original formulation was unreachable. That pin
3a's scope is *forced* rather than *chosen* is a stronger result than the
mutation was written to obtain, and it belongs in the report.

**M7 — the laundering the text refusal prevents, finally observed. ADDED
2026-08-07 in review round 1; this discharges inherited obligation 2.**

The format rung reasoned about this path and could never run it: its own pin 3a
refused every major-1 base commit categorically, so the observation was
unreachable. Under S27 a base commit succeeds or fails on its version, so it
becomes reachable for the first time.

**The path, corrected in round 8 and restructured in round 10.**

The **import leg** is `text` → **`parse_document`** (`parse.rs:83`) →
`TextDocument` → **`serialize_document`** (`serialize.rs:143`) → `Bundle`; round 10
prepends an **export leg** to derive the text from `B` (steps 1–2 below). **Remove
whatever refusals your actual path crosses** — the import leg's are
`parse.rs:138`–`:147` and `serialize.rs:151`; the export leg's depend on how you
reach text, and `document_from_bundle` carries one from `be244df`.

> **No count is given, and that is deliberate.** This heading read "two of the
> three refusals matter" until round 10, when adding the export leg changed which
> are crossed. Every stated count here has been falsified by the next round —
> "all three" (round 1, wrong at round 8), "the two" (round 8, wrong at round 10).

**Do NOT remove `project_text_document`'s refusal (`project.rs:579`), and do not
count it.** It has the signature `&TextDocument -> Result<String, _>` — it is the
**export** direction, and **nothing on the import path calls it**. The previous
wording said "all three sides, since removing one leaves the others refusing and
the document never reaches the writer", which is false for this one: it is not
between the document and the writer, it points the other way.

**The input MUST be text, and MUST be parsed.** Round 1 wrote "construct a
base-bearing `TextDocument`", which **bypasses `parse_document` entirely** — so
the parser refusal was irrelevant to what was being demonstrated, and the
demonstration was not the *import* laundering it is named for. **An in-memory
`TextDocument` proves nothing about what an external document can do**, and what
an external document can do is the entire threat.

**And the text MUST be derived from the reference bundle, not hand-written.
RESTRUCTURED IN ROUND 10 — see the alignment note below.** The demonstration is a
**round trip**, which is also the realistic form of the threat:

**Steps 1a–1c added in round 11, on the probe's evidence.** Round 10's version
compared against the artifact built in step 1, and the probe proved that only
works when its input document happens already to be a fixed point.

**M7's harness lives in `epiphany-textproj`. PINNED IN ROUND 12 — it was
previously unchosen, and only one crate can host it.**

M7 needs two things at once: the **real constant** (`epiphany-ops`) and
**`render_text_document`**, which is **`pub(crate)` to `epiphany-textproj`**
(`project.rs:595`). `epiphany-textproj` depends on `epiphany-ops`, so it has
both. **`epiphany-testkit` has the constant but cannot call the renderer**, and
making it able to would be a **visibility change to `epiphany-textproj`'s public
API that no pin authorises** — an unpinned API change smuggled in as test
scaffolding.

**So M7's temporary harness is written in `epiphany-textproj`, under touch row 9,
which already carries `textproj/src/{serialize,project}.rs`. No new touch row.**
*(Round 11 said "in a crate that can reach the real constant", which is true of
two crates and decisive for neither — leaving execution to discover the
visibility wall and improvise past it.)*

> **`render_text_document` stays `pub(crate)`.** Handoff §1.3 records it as *the
> one intentional hole* in the text refusal, existing solely so a negative vector
> can carry the spelling it asserts is refused. **Widening it to host M7 would
> turn a deliberately narrow exemption into public API** — and M7 is a mutation
> that gets reverted, so it must not leave a widened surface behind.

1. **Build `B_raw`, the validated reference** — in `epiphany-textproj`, per the
   pin above: create a bundle with `caps` derived from
   **`CURRENT_REDUCTION_ALGORITHM_VERSION`** and commit a canonical base carrying
   that same version, so **pin 3a's writer check validates it on the way in**.
   That — and only that — is a genuinely validated base.

1a. **Normalise to a byte-level fixed point. The bound is PINNED AT ONE
   NORMALISING STEP — round 12.**

   Define, with `uuid` fixed throughout:

   - `B₀ = B_raw`
   - `B₍ₙ₊₁₎ = serialize_document(document_from_bundle(Bₙ), uuid)`
   - **`B_fixed = Bₙ` for the smallest `n` with `B₍ₙ₊₁₎.image() == Bₙ.image()`.**

   **Compute at most `B₁` and `B₂`. The permitted maximum is `n = 1`.** So:

   | Outcome | Meaning | Required action |
   |---|---|---|
   | `B₁ == B₀` | `B_raw` was already a fixed point | `B_fixed = B₀`; **report "already fixed"** |
   | `B₁ != B₀` and `B₂ == B₁` | one normalising step, the expected case | `B_fixed = B₁`; **report `n = 1`** |
   | `B₂ != B₁` | **the normalisation is not idempotent** | **HARD FAILURE.** Report all three image lengths and the first differing offset between `B₁` and `B₂` |

   **The bound is one step because that is a property, not a tolerance.**
   `document_from_bundle` **canonicalises**, so `serialize_document ∘
   document_from_bundle` must reach its canonical form in a single application.
   **If `B₂ != B₁`, there is no canonical form** — and then no choice of reference
   artifact is principled and **M7 is invalid as a whole**, not merely failing on
   this input. That outcome is a finding about the projection, and it must be
   reported as one rather than worked around by iterating further.

   **Raising the bound requires an amendment with its own review round.** It was
   left unstated until round 12, which meant **execution would have chosen when
   non-convergence becomes failure — silently changing what the experiment
   means.** A loop that iterates until it happens to settle tests nothing; it
   merely reports how long it took.

   *(The probe observed one step sufficing for three documents. That motivated
   this bound; it does not prove it, which is exactly why `B₂ != B₁` is a
   reportable finding rather than an assertion nobody expects to fire.)*

1b. **Assert the fixed-point property explicitly** — that
   `serialize_document(document_from_bundle(B_fixed), uuid).image()` equals
   `B_fixed.image()`. **This assertion is the precondition of the whole
   comparison and must be a hard failure, not a comment.**

1c. **Report the iteration count** step 1a needed, and **whether `B_raw` was
   already fixed.** If it was, say so — that is the coincidence that made the
   probe's first case pass, and a reader must be able to tell the lucky case from
   the general one.

2. **Export `B_fixed` to text.** `document_from_bundle` to a `TextDocument`, then
   the crate-private `render_text_document` — which **does not refuse**, and
   exists precisely so the base spelling can be produced for a negative vector.
3. **Parse that text back** with `parse_document`, giving `D`.
4. **`A = serialize_document(D)`**, with **`B_fixed`'s `FileUuid`**.
5. **Compare `A.image()` with `B_fixed.image()` — and NEVER with `B_raw`.**

> **Why `B_raw` is not a permitted comparand.** `document_from_bundle` applies a
> **canonical envelope ordering**, so if `B_raw`'s envelopes arrived in any other
> order, `A` and `B_raw` differ **by that normalisation alone**. The probe measured
> it: **295 differing bytes from offset 352** on a one-extension document. That
> difference is **indistinguishable from a provenance result** — and a comparison
> whose failure mode cannot be told apart from its success condition decides
> nothing. `B_raw` exists only to be normalised; it is never compared.

**The comparison artifact — built by M7 itself. CORRECTED IN ROUND 9.**

> **Round 8 nominated test 10b's construction, and 10b cannot serve.** Its
> write-side capability is `synthetic_for_fixture(0)`; **only its reopen uses the
> real authority.** So its base was never committed under the real constant, and
> comparing against it compares one synthetic fixture with another — the
> "genuinely validated" half of the claim would simply be absent.
>
> **This is a collision between two of this contract's own designs.** Round 4
> made 10b synthetic-on-write *deliberately*, so that M5b's two operands would be
> provably independent. That is exactly what disqualifies it here. **One artifact
> cannot be both "independent of the real authority" and "committed under the
> real authority."** Round 8 reused a fixture by name without re-reading what it
> was built to be.

M7 therefore builds its **own** reference — step 1 above.

**Alignment is INHERITED, not enumerated. ROUND 10.**

> **Round 9 listed four things to align — `FileUuid`, base payload, schema
> versions, generation — and the list was nowhere near complete.**
> `serialize_document` additionally fixes `document_id`, `lineage_id`,
> `profile_declarations`, every extension's `extension_id` / `version` /
> `required` / `affected_object_kinds` / `edit_barriers` and preserved chunks,
> the envelope payloads, the **staging order** (base root → extension chunks →
> operation-envelope block), the manifest schema `major`, and `epoch_max` — and
> from those, every chunk ref, hash and offset in the result.
>
> **So a byte difference would have had a third possible cause: "the reference was
> built differently."** That is neither permitted classification — not
> nondeterminism, not a provenance signal — and its existence makes the whole
> comparison uninterpretable. **A result that cannot be classified is not an
> observation.**
>
> **This is the same defect round 9 fixed one level up**, and it is the third time
> this contract has tried to enumerate a complete set by hand and failed: "every
> field that could carry provenance" (round 8), "every field to align" (round 9),
> both wrong on the day they were written. **Stop enumerating.**

Because `D` is *derived from* `B` by steps 2–3, **every input
`serialize_document` reads is already `B`'s own.** Nothing is aligned by hand and
no list can be incomplete. The only free variable is `FileUuid`, which is an
explicit argument, set to `B`'s in step 4.

**The setup-mismatch category is therefore eliminated by construction, not by
care** — there is no independent second construction to mismatch. That is the only
reason whole-image equality means anything here.

**Which refusals the path crosses: name them from the path, do not take a count
from this contract.** Steps 2–4 cross the projector-side, parser and serializer
refusals in whatever combination the code actually presents — `render_text_document`
is expected not to refuse, and `document_from_bundle` carries a refusal of its own
from `be244df`. **Enumerate what you actually had to remove, and restore each by
hand-editing back.** Every previously stated count here has been wrong (round 8
and round 9 each corrected one), which is why none is stated now.

**The comparison method — whole artifacts, not a field list. CORRECTED IN ROUND
9.**

**Compare the complete `image()` bytes of both bundles.** Equality is
**necessary but NOT sufficient** — it is observation 1 of three, and **the control
must still run and reject the mismatched base.** See the observation requirements
below; they are the authority on what M7 owes, and this paragraph specifies only
*how to compare*, never *what suffices*.

> **CORRECTED IN ROUND 14.** This read *"if they are equal, the observation is
> made and nothing further is required"* — written in round 9, when byte equality
> **was** the whole of M7. Round 13 added the writer-check control and did not
> sweep back to this sentence, so the contract simultaneously **required** the
> control and **licensed omitting it**, with the permissive sentence sitting
> earlier and reading as the summary. **A reader following the document in order
> would have stopped here.**

> **Round 8 specified a field enumeration and got it wrong**, which is why this
> is no longer a list. It claimed to cover "everything that could carry
> provenance" while omitting `FixedHeader.file_uuid` — **the very field it
> required to match** — along with the superblock's `generation`,
> `manifest_offset`, `manifest_length` and `manifest_hash`, and the whole of the
> manifest outside `canonical_base`. **A hand-written list of "every field" is a
> claim about a struct's contents that goes stale when the struct changes**, and
> this one was wrong on the day it was written.
>
> **Comparing the whole artifact cannot be incomplete.** Same lesson as tables
> over numbers, applied to the experiment instead of the prose: **let the artifact
> defend itself rather than enumerating it correctly.**

**If the images differ, do not stop and do not conclude.** Enumerate every
differing byte range, resolve each to its field, and classify it:

- **justified nondeterminism** — state the cause and why it cannot carry
  provenance, then normalize it and re-compare; or
- **a provenance signal** — a field that does distinguish a laundered base from a
  validated one. **That is a finding, and a significant one**: the text refusal
  may be stronger than it needs to be, and a future rung could use that field
  instead. Report it; do not normalize it away.

**Report the comparison, not a verdict.** "Byte-indistinguishable" asserted as a
conclusion is precisely the reasoning-instead-of-observing this rung exists to
stamp out — round 1 wrote the conclusion and specified no way to reach it, and
round 8 specified a way that could not support it.

**What the observation must show, or it has not been made. CORRECTED IN ROUND 13
— the previous wording inverted the result.**

It read *"the capability check does **not** fire."* **That is backwards.** Pin 3a
requires `commit`/`commit_versioned` to validate a **newly emitted** canonical
base — and **both** `B_raw` and `A` commit exactly that. **The check fires on both
paths. It fires and it ACCEPTS**, because the raw version equals the real
authority.

**That acceptance is the whole laundering result:** the base is not slipped past
an absent check, it is **admitted by a check that is working correctly and cannot
tell a coincidence from a rebuild.**

Therefore M7 must show **all three** of:

1. **`A.image()` equals `B_fixed.image()`** — the container records nothing about
   how the base arrived.
2. **Pin 3a's writer validation ran and accepted** on both commits.
3. **The control below.**

### The control — REQUIRED, added in round 13

**In the same run, with the same harness, repeat the import with a base version
deliberately NOT equal to the real authority, and observe that the commit is
REJECTED with `CanonicalBaseRequiresRebuild`.**

**Without this control, M7 is satisfiable by deleting pin 3a's writer check
entirely** — which would produce a *passing* M7 while demonstrating the exact
opposite of what M7 exists to show. An observation that the check "does not fire"
cannot distinguish **a check that accepts** from **a check that is not there**,
and only one of those is the finding.

**The control is what proves the check was live.** The matching case succeeding
means something only once the mismatching case is seen to fail on the same path,
in the same run, under the same removals.

> **M7's removals are limited to the text refusals — pin 3a is NOT among them,
> and MUST NOT be removed or weakened.** M7 mutates the *text* boundary to make a
> path reachable; it does not mutate the *authority* boundary, which is the thing
> under observation. Removing both would not be a stronger mutation, it would be
> a different and empty experiment.

**Report all three observations, and the control's error variant by name.** A
report giving only observation 1 has not made the demonstration — it has measured
two byte strings.

### The claim M7 establishes, and the one it does NOT. SCOPED IN ROUND 11.

**What it proves:** **the text path carries no provenance marker — after
normalisation.** A document exported to text and re-imported yields a container
byte-identical to the normalised validated one, so nothing in the container
records *how* the base arrived.

**What it does NOT prove, and must not be written as though it does:** that
**every** direct bundle is byte-identical to its re-imported form **before**
normalisation. **It is not** — the probe measured 295 differing bytes on a
one-extension document, and two of three documents tested were not fixed points.
Those differences are `document_from_bundle`'s canonical envelope ordering, and
they have **nothing to do with provenance**.

**Why the distinction is load-bearing rather than pedantic.** M7's conclusion is
the sole evidence for a **permanent capability loss** — the text refusal that cost
`COMPANION_VERSION` 0.14.0 and took the corpus's `canonical_bases` from 2 to 0. A
justification stated more broadly than the observation supports would be arguing
for a permanent refusal from a result that was never obtained. **State the
normalised claim, and state the exclusion beside it.**

**Report both sentences in the rung's report**, not just the first. An
unqualified "the text path is indistinguishable from the validated path" is
**false as written**, and it is the sentence a reader will otherwise carry
forward.

> **This mutation is informative in both directions, which is why it is worth
> running.** If the images are equal **and the other two observations hold**, the
> refusal is justified and the format rung's reasoning is confirmed by
> observation for the first time. **If the images differ, resolve the difference
> to a field**: it is either justified nondeterminism, or **a provenance signal
> nobody knew existed** — in which case the text refusal may be stronger than it
> needs to be, and that is a finding for a future rung, not something to suppress
> because it contradicts the expected result.
>
> **CORRECTED IN ROUND 14, as a second instance of that round's finding.** This
> read *"if every field matches, the refusal is justified"* — the same sufficiency
> claim as the sentence above, in different words, and still carrying round 8's
> **"every field"** vocabulary that round 9 replaced with whole-image comparison.
> **Round 14 reported no second instance, and there was one**: a search for
> "nothing further" or "sufficient" could not reach a sentence that says
> "matches" instead. **Same defect, different spelling** — the failure mode
> `CLAUDE.md` names, met inside the fix for it.

**Restore every refusal you removed by hand-editing back**, never with git —
working from the enumeration the path requirement above demands, **not from a
count stated here.** *(This said "all three" until round 8, then "the two … `parse.rs`
and `serialize.rs`" until round 10, when the round-trip path changed which
refusals are crossed. Three wordings, three wrong counts; there is now no count
to be wrong.)* Record the result as a **demonstration**, not a guard: nothing in
the shipped tree changes, and the refusal is permanent (see the ruling under
inherited obligation 2).

> **You will meet dead code here. Do NOT fix it — report it.** Found in review
> round 2: `serialize.rs:157`'s `if let Some(base) = &document.canonical_base`
> is **unreachable**, orphaned by the `:151` guard that returns
> `CanonicalBaseUnsupported` before it. Removing the guard for M7 makes it live
> again, which is what lets the demonstration run at all — and restoring the
> guard makes it dead again. It is a pre-existing defect from the format-epoch
> rung, **not this rung's to repair**, and touching it would put an unpinned
> change in a staged file. Record it in the report; it is a Pass-13 candidate.

**This is a mutation whose expected outcome is SUCCESS, not failure.** Every
other mutation here breaks a test; this one makes a refused path succeed, and
the finding is that it succeeds *silently*. Do not report it as a passing gate.

---

## §5. Gate

1. `cargo test --workspace` — full pass. **Report the new total and account for
   the delta by category — do NOT reconcile against a fixed number.** *(Corrected
   in review round 1: this item read "(four tests added)", a figure already stale
   twice over, and §3's own header carried the same wrong count. The delta is not
   a simple addition: three of §3's tests **convert or extend** existing
   format-rung tests, which nets zero, while others are new.)* The baseline is
   **1570**. Give the count in three buckets — net-new, converted-from-existing,
   restored-assertions — and if they do not sum to the observed delta, **that is
   a finding, not an arithmetic error to be papered over**.
   **Also report the ignored count, which MUST be 0. ROUND 17:** this required a
   "full pass" and said nothing about ignored tests, so an `#[ignore]`d new test
   satisfies it **while never running** — and could still be counted as net-new.
   The measured baseline is **1570 passed / 0 failed / 0 ignored across 42
   suites**; any nonzero ignored count is a finding.
2. `cargo +1.95.0 clippy --workspace --all-targets -- -D warnings` → clean.
   **The toolchain is part of the gate. ROUND 17.** This named none, in a repo
   whose CI comment records that *"1.97 rejects a bare `2.0` that 1.95 accepts"*
   — and whose default `stable` is **1.97.1**, not the pinned **1.95.0** that CI
   gates on. **A clippy result that does not say which toolchain produced it says
   nothing.** Report the toolchain with the result.
3. `cargo +1.95.0 fmt -p epiphany-ops -p epiphany-bundle -p epiphany-testkit -p
   epiphany-textproj --check` → clean, **on the same pinned toolchain and for the
   same reason**. **`cargo fmt --all` is forbidden** (it crosses into `spikes/`
   through path dependencies).
4. `git diff --cached --check` clean after staging. **The staged list is a SUBSET
   of §2, not an equality. ROUND 17.** It read "staged list exactly §2", which is
   **unsatisfiable**: row 12 is explicitly conditional and row 7a may legitimately
   not change. Read literally it fails whenever a conditional row is correctly
   unused — or invites staging an unchanged file to satisfy it. **Instead:**
   every staged path must appear in §2, **and** every §2 row must be either
   staged or **named in the report as unused, with its reason.** Neither
   direction may be silent.

   > **BOTH DIRECTIONS ARE ABOUT §2's ROWS, AND §2 ALSO CARRIES PROHIBITIONS.
   > AMENDMENT 2.** *"Appears in §2"* and *"is not forbidden by §2"* are different
   > questions; this item asks only the first, in both of its directions. §2's
   > rules A and B forbid paths, and **a staged file can satisfy this gate and
   > violate §2 in the same commit** — which is exactly what happened at execution
   > and was reported as clean. **This gate MUST therefore be run together with §2's
   > rule A and rule B**, and the report must state all three results, not a single
   > staged-list verdict.

4a. **The declined capability restoration is still declined — `COMPANION_VERSION`
   has not moved. ADDED BY AMENDMENT 2, replacing a guard amendment 1 removed
   without noticing it was a guard.**

   Until amendment 1, `crates/epiphany-textproj/src/lib.rs` was absent from §2, and
   that absence was the only thing standing between this rung and a silent
   companion-version bump. Touch row 15 now stages the file, so **the absence check
   is gone and this replaces it. Compare values, not diffs** — gate 7's ruling, for
   gate 7's reason:

   ```
   grep -n "pub const COMPANION_VERSION" crates/epiphany-textproj/src/lib.rs
   git show HEAD:crates/epiphany-textproj/src/lib.rs | grep -n "pub const COMPANION_VERSION"
   ```

   → the two values **identical**, and both **`(0, 14, 0)`**. Report both outputs,
   not the conclusion.

   > **A diff-based check fails here for a reason this rung can demonstrate rather
   > than predict:** `production_caps` is appended to that file, which **moves
   > `COMPANION_VERSION` from line 70 to line 72.** Any check keyed to the diff
   > touching those lines, or to the constant's line number, reports a change that
   > did not happen. The value comparison is unaffected.

   **And the staged diff of that file MUST be quoted in full in the report, and
   read.** It is permitted to contain `production_caps`, its doc comment, and the
   imports that function needs — **and nothing else.** Anything further is unpinned
   scope in a production crate's root, which is the precise thing row 15's
   `pub(crate)` ruling was narrowed to prevent. **A quoted diff is read, not
   matched**, for the same reason gate 6c quotes the struct: this file is now
   reachable by staging, so the only remaining guard on it is a human reading what
   went in.
5. **`epiphany-ops` has NOT gained an `epiphany-bundle` dependency** — read
   `crates/epiphany-ops/Cargo.toml` directly and **quote all three dependency
   tables** (`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`).
   **ROUND 17:** it said only "check directly", naming no tables — and a
   dev-dependency would satisfy a reader checking only the first while still
   creating the cycle pin 1 exists to prevent.
6. **`BundleCapabilities` implements no `Default`. REWRITTEN IN ROUND 17 — the
   previous query could not detect the likelier violation.**

   > **What was wrong, verified by running it.** The old regex was
   > `impl +Default +for +BundleCapabilities|derive\([^)]*\bDefault\b[^)]*\)[[:space:]]*(pub )?struct BundleCapabilities`.
   > `grep` is **line-oriented**, and `[[:space:]]*` cannot cross the newline
   > rustfmt puts between an attribute and its item. Run against a real
   > `#[derive(Debug, Clone, Default)]` above `pub struct BundleCapabilities`, it
   > returns **0 matches** — **the gate passes.** Only the explicit `impl` form
   > was ever caught. **`#[derive(Default)]` is the likelier way someone adds it**,
   > and gate 6 is the *sole* mechanical guard on pin 3's prohibition — M4 exists
   > precisely because **no test can catch a `Default` that callers then use.**

   **Three checks, all required. Report all three outputs, not a verdict.**

   a. `grep -rn "impl Default for BundleCapabilities" crates/epiphany-bundle/src/`
      → **0 matches.**
   b. `grep -rn -B4 "struct BundleCapabilities" crates/epiphany-bundle/src/ | grep -i derive`
      → **report every line**, and confirm none names `Default`. The `-B4` is what
      crosses the newline the old regex could not.
   c. **Quote the complete definition of `BundleCapabilities` verbatim in the
      report** — the struct, its attributes, and every `impl` block on it.

   **(c) is the one that cannot pass vacuously.** (a) and (b) are greps for
   absence, and **a grep for absence is defeated by a rename** — the same defect
   that made gate 6a vacuous (see pin 3b). A quoted definition is read, not
   matched, so it cannot be satisfied by searching for the wrong string.

   **If (c) cannot be produced because the type does not exist under that name,
   that is a pin 3 violation and a finding — not a gate that passed.** Zero
   output from (a) or (b) means "no `Default`" **only when (c) shows the type is
   there to have one.** Report (c) first, so (a) and (b) are read against a type
   known to exist.
6a. **No production composition path uses the fixture constructor.** *(Scope
   widened in review round 1: this checked `epiphany-textproj` only, while touch
   row 7 gives **`epiphany-testkit`** the real authority too — so a
   `synthetic_for_fixture` leak there was ungated.)* Run over **both**:
   `grep -rn "synthetic_for_fixture" crates/epiphany-textproj/src/ crates/epiphany-testkit/src/`
   Report each match with its enclosing item. In `epiphany-textproj` every match
   must be inside a `#[cfg(test)]` module. In `epiphany-testkit`, which is a
   fixture crate whose non-test code legitimately builds fixtures, each match
   must instead be justified against §0.4's rule: **only production composition
   paths wrap the real constant**, and `roundtrip.rs` / `bundle_harness.rs` carry
   both kinds. Name which kind each site is; an unclassified site is a finding.

   **This gate is vacuous if the constructor is not named `synthetic_for_fixture`.
   ROUND 17.** It greps for that exact literal, and pin 3b offered the name as an
   example until round 17 pinned it. **Any other name returns 0 matches and this
   gate reports a pass having checked nothing.** Before trusting the count,
   **confirm the constructor's actual name against pin 3b and quote its
   signature** — a zero here means "no leaks" only if the string searched for is
   the string that exists.
7. `spec/vectors/decode_vectors.txt` unmodified — `git diff --cached --stat --
   spec/vectors/decode_vectors.txt` is **empty**; and **no schema major or minor
   moved**, by the method below. **ROUND 17: the second clause named no method,
   so it could be satisfied by not being checked.**

   **Compare values, not diffs.** Quote the working-tree value of each of
   `FORMAT_MAJOR` (`header.rs:47`), `FORMAT_MINOR` (`header.rs:53`) and
   `Manifest::SCHEMA` (`manifest.rs:601`) beside its value at `HEAD`:

   ```
   grep -n "pub const FORMAT_MAJOR\|pub const FORMAT_MINOR" crates/epiphany-bundle/src/header.rs
   grep -n "const SCHEMA" crates/epiphany-bundle/src/manifest.rs
   git show HEAD:crates/epiphany-bundle/src/header.rs   | grep -n "pub const FORMAT_M"
   git show HEAD:crates/epiphany-bundle/src/manifest.rs | grep -n "const SCHEMA"
   ```

   → the two sides **identical**. Report all four outputs, not the conclusion.

   > **A diff-based check was written here first and withdrawn in the same round.**
   > Grepping `git diff` for the constant names goes non-empty when the constants
   > merely *move* — a comment added above them is enough — so it **fails on a
   > change that did not happen**. A value comparison cannot. **Pin 3's rung type
   > asserts no schema major or minor changes; this is the only item that checks
   > that assertion, and until round 17 it named no method at all.**

---

## §6. Staging and boundary

Stage only §2's files, by explicit path. **Never `git add -A`.**

**A concurrent session commits here.** Re-check `HEAD` before staging and before
commit. **Never** `git reset`, `git restore --staged`, `git checkout`, `git
stash`.

**Out of bounds — MUST NOT be read, written, or staged:** the entire `spikes/`
tree, `spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_*.md`,
`spec/ANALYSIS_GENESIS_PERSISTENCE.md`, `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`,
`spec/DRAFT_T4_FIXTURE_RECIPE.md`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`, `crates/epiphany-editor-gui/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the root `Cargo.toml`,
`.claude/worktrees/`.

**Do not implement any part of P13-S16.** This rung unblocks it; it does not
begin it. No `create_staff`, `create_staff_group`, or invariant change.

**Do not bump `CURRENT_REDUCTION_ALGORITHM_VERSION` past 0** — pin 2. The bump
to 1 belongs to S16.

**Execution boundary (SATISFIED):** the executing agent MUST NOT commit and left the
work staged for independent review.

**Execution is AUTHORISED as of ratification, 2026-08-08.** *(This read "no
execution work may begin at all until this contract is ratified"; ratification has
happened.)* The boundaries above are unchanged and remain binding — **stage only
§2's files by explicit path, never `git add -A`, re-check `HEAD` before staging
and before committing, and never `git reset`/`restore`/`checkout`/`stash`.**

**The executing agent left the work STAGED and did not commit.** The execution report
was then subject to **independent review before completion was accepted**, covering in
particular **M7's three observations and its control**. The repository owner accepted
the reviewed staged implementation on 2026-08-09.

---

## §7. Report requirements

*Counts corrected 2026-08-07 in review round 1 — items 1, 2 and 4 each named a
figure the document had outgrown. This is the same drift as §3's header and gate
1: **three independent stale counts of the same three lists.** Prefer "every item
in §N" to a number.*

1. **Every mutation in §4** — currently **eight** (M1, M2, M3, M4, **M5a**,
   **M5b**, M6, M7), each with verbatim output. *(M5 split in review round 2.)*
   **M4 is a recorded prohibition** (observed to compile, then reverted) and
   **M7 is a demonstration whose expected outcome is success — except its
   control (round 13), whose expected outcome is a REJECTION.** A report that
   treats the control's rejection as a problem has misread it: that rejection is
   the evidence pin 3a's writer check was live, without which M7's success proves
   nothing. Neither M4 nor M7 is a
   passing guard; do not report them as one.
2. **Every gate item in §5** — currently **eight** (1, 2, 3, 4, 5, 6, 6a, 7),
   each with the command that produced it.
3. The staged file list, and the test-count delta in gate 1's three buckets.
4. **Every required test in §3** — currently **eleven** (1–9 plus **10a** and
   **10b**, added in round 4) — by name, each passing, with tests 2 and 3 shown
   to produce *different* errors, and the same for tests 6/2 and 8/5.
4a. **For every mutation in §4, the observation it owes.** For most that is the
   named test it breaks; **for two it cannot be, and this item said otherwise
   until round 7.**

   | Mutation | Owes |
   |---|---|
   | M1 | its stale, self-consistent fixture observed to **open successfully** under mismatching `caps`, plus test **2** failing |
   | M2 | its corrupt fixture observed returning **`CanonicalBaseRequiresRebuild`** with both fields, plus test **3** failing |
   | M3 | its base-free fixture observed rejected by the wrongly widened check with **`CanonicalBaseRequiresRebuild`** and both fields, plus test **4** failing |
   | **M4** | **no test — none is possible.** It owes the observation that the `Default` impl **compiles**, and the explicit statement that no test can catch it. That is why pin 3's prohibition is a review rule |
   | M5a | `serialize_document` observed supplying the deliberately changed authority, plus test **10a** failing and the provenance of the asserted operand |
   | M5b | test **10b** fails with both error fields asserted, **plus** the provenance of *both* operands |
   | M6 | **the mutated outcomes M6 specifies for both halves** — test 5's stale commit observed to *succeed* and reopen at the new generation, and test 9's unchanged inherited-base commit observed *rejected by the broadened rule*. **A failing test is necessary and not sufficient**; counts and conditions live in M6, not here |
   | **M7** | **no test — its expected outcome is success.** It owes **every observation M7 specifies, including its control**, and confirmation that the refusals M7 names as removed were restored. **Counts, methods and file names live in M7, not here** — this cell said "all three refusals" until round 8 and "field-by-field enumeration" until round 9, each time describing a method M7 no longer used |

   **M4 and M7 were unsatisfiable under the previous wording.** Item 1 already
   said both are "not a passing guard", yet this item demanded a test each
   breaks — M4 is observed to *compile* and M7 is expected to *succeed*. **A
   report obeying 4a literally could not be written**, and the honest response
   would have been to invent a test for one of them.

   Three mutations were found unrunnable across rounds 2 and 3 (no observation
   mechanism, cannot fail, nothing asserting the broken behaviour). This item
   exists so a fourth is caught here rather than in review — **which required
   admitting that "breaks a test" is not the only shape an observation takes.**

4b. **Quote, verbatim, every fixture-construction operand in tests 10a and 10b,
   and confirm each is still a literal.** Not "the literal `0`" — **there are
   three**, and they must be listed individually:

   | # | Test | Operand | Must be |
   |---|---|---|---|
   | i | 10a | the value asserted against `capabilities()` | literal `0` |
   | ii | 10b | the argument to `synthetic_for_fixture(_)` | literal `0` |
   | iii | 10b | the committed base's `ReductionAlgorithmVersion(_)` | literal `0` |

   **Widened in round 6, which found the previous wording protected one operand
   where test 10b has two.** Item 4b said "the literal `0` in tests 10a and 10b"
   and "both literals", counting one per test. But 10b constructs its fixture
   from **two independent literals**, and replacing **both** (ii) and (iii) with
   `CURRENT_REDUCTION_ALGORITHM_VERSION` **keeps the `synthetic_for_fixture` call
   exactly where it is**, restores the tautology in full, and leaves every test
   green.

   > **And the error arm cannot catch it.** Test 10b's `Err` branch asserts
   > `base == 0`, but in the unmutated run the reopen returns `Ok` and that branch
   > never executes. The literal guarding the tautology sits on a path taken only
   > under mutation — so a tidy-up of (ii) and (iii) is invisible to the suite,
   > invisible to the error arm, and visible **only here**. This item is the sole
   > protection, which is exactly what round 5 concluded when it retracted M5b's
   > claim of a structural guarantee — and round 5 then wrote the check too
   > narrowly to deliver it.
5. A count of call sites updated per crate, against §0.4's table **as corrected in
   review rounds 1 and 2 and extended by amendment 1** — `open` **60**, `create`
   **32**, `create_versioned` **3**, total **95** — any discrepancy is a finding.
   Count `Bundle`-typed receivers, not the token `.commit(`.

   > **`create_versioned` ADDED HERE BY AMENDMENT 2 — the third site of that
   > omission, and the second one amendment 1 left behind.** Amendment 1 added the
   > third constructor to §0.4 and to touch rows 13–16, and left **this item** and
   > **touch row 13** still reconciling against `open` + `create` only. **A report
   > obeying this item literally would have re-derived the incomplete surface the
   > amendment existed to complete — and reported it as a match.** *A correction that
   > does not sweep every site restating the thing corrected has not been made*: the
   > meta-defect pin 3a's note counts, recurring now inside a post-ratification
   > amendment.
   >
   > **These figures are PRE-S27 and must be read as such — amendment 2.** S27's own
   > new tests add constructor call sites, so the tree at execution reads `open`
   > **66** and `create` **33**. That delta is a **deliverable, not a discrepancy**;
   > report it as one, with the added sites attributed to the tests that introduced
   > them. Reconciling the post-implementation tree against a pre-implementation
   > surface and calling the difference a finding would be its own error.

   *(Attribution fixed in round 3: round 1 moved §0.4's table 57 → 60; round 2
   fixed the "Rung type" paragraph, touch rows 2 and 5, and struck the
   `project.rs` production claim that made the table's textproj entries
   misleading. Both rounds are load-bearing here.)*
6. **Confirmation that every refusal M7 removed was restored** — M7 names which,
   and the count is not repeated here; it said "three" from round 1 until round 9
   caught it, having survived round 8's correction of that same number in two
   other places — and that none of
   `text_projection.tex`, `textproj/src/parse.rs` or `textproj/src/vectors.rs`
   appears in the staged diff.

   > **`textproj/src/lib.rs` STRUCK from that list by amendment 2 — the third site
   > of the same contradiction, and the one the independent review did not cite.**
   > Touch row 15 stages that file for `production_caps`, so requiring its absence
   > here made this item's confirmation **unobtainable**: the report could not both
   > obey row 15 and satisfy item 6. Its guard is now **gate 4a**, which compares
   > `COMPANION_VERSION`'s value rather than the file's absence. **The review that
   > found this defect cited two of its three sites** — the ruling under inherited
   > obligation 2 and §2's absence sentence; the third surfaced only by grepping
   > every mention of the filename, which is the fix-every-site discipline earning
   > its keep against an independent round rather than against the author.
   >
   > **`serialize.rs` and `project.rs` are not in that list and MUST NOT be added.**
   > M7 also edits `serialize.rs:151` and `document_from_bundle`
   > (`project.rs:480`), but both are legitimately staged under touch row 9, so
   > **their restoration is confirmed by reading the staged diff, not by absence** —
   > see §2's rule A, which states outright that the absence tripwire covers one of
   > M7's three edit sites. **This item is where the other two are discharged**, so
   > quote the relevant hunks; a bare "M7 restored" here confirms nothing for them.
7. **Whether pin 9 minted a new requirement label**, and therefore whether touch
   row 12 was used.
8. **Confirmation that the pin-3c suspension marker naming this contract is gone
   from `testkit/src/roundtrip.rs`.** If it is still in the tree, obligation 3's
   restoration did not happen, whatever the prose says.
9. **The `serialize.rs:157` dead branch**, recorded as a finding and **not
   repaired** — see the note under M7.
10. **M6's replaced second half**, with the reachability result stated: that no
   caller can hold an open `Bundle` with a stale *inherited* base, so pin 3a's
   scope is forced rather than chosen.
11. Anything contradicting this contract.
