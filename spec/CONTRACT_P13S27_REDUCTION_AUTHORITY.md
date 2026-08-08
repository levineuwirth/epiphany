# Contract — P13-S27: the reduction version gets an outside witness

**Status:** **NOT RATIFIED. NOT DISPATCHABLE.** **Awaiting the next review round;
the pins remain open and no execution work may begin.** Which rounds have closed,
what each found, and the running tally are **the history table below** — this line
does not restate them, having gone stale in two consecutive rounds by doing so.

**Round 1's ratification is WITHDRAWN.** It was claimed on 2026-08-07 after a
single round; round 2 then found four more blocking defects against the
supposedly frozen text, two of them introduced by round 1's own amendments. A
ratification that a subsequent round falsifies that quickly was not a
ratification, and leaving the claim standing would make the status field mean
nothing.

**The pins are therefore NOT frozen.** They are open to **the current round's**
findings. Freezing follows ratification; it does not precede it, and it does not
survive a withdrawal.

### One narrow, explicit exception to "no execution work" — granted 2026-08-08

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
S27. **No execution work may begin** — not implementation, not staging,
not partial work against "the settled pins."

**History — the running tally, which has now gone stale three times and been
restructured twice to stop it.** Every amendment is a row. **There is no separate
amendment count, deliberately**: round 5 turned the review totals into a table
and left the amendment tally as prose immediately above it, which went stale in
the same edit that fixed its neighbour. The amendment count **is** the number of
rows — read it off, do not restate it.

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
| **Total** | **41** | **29** | one amendment per row |

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
| 1 | **Test 10b is not a "genuinely validated" reference.** Its write-side capability is `synthetic_for_fixture(0)`; only its *reopen* uses the real authority. M7 would have compared one synthetic fixture against another, with the validated half of the claim simply absent | M7 now **builds its own reference** in `epiphany-testkit`: commit a base under `caps` derived from the real constant, so pin 3a validates it on the way in |
| 2 | **The field enumeration could not support its conclusion.** It claimed "everything that could carry provenance" while omitting `FixedHeader.file_uuid` — **the field it required to match** — plus the superblock's `generation`, `manifest_offset`, `manifest_length`, `manifest_hash`, and the manifest outside `canonical_base` | Replaced with **whole-`image()` byte comparison**, with any difference enumerated and classified rather than assumed |

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

**What round 11 should weigh, stated against interest:** findings by round are
**9, 6, 6, 5, 4, 3, 3, 2, 2, 1**; blocking **4, 4, 4, 4, 2, 3, 3, 2, 2, 1**. Ten
rounds, **none clean**, and **three consecutive rounds have found exactly one
paragraph — M7 — defective in a new way each time**: unrunnable, wrong artifact,
wrong method, incomplete alignment. Findings are now falling steadily and every
one of the last three has been narrower than the last, which is the first
sustained convergence signal in the contract's history. **Against that: M7 has
never been executed, and each of its four designs looked correct when written.**
The honest question for round 11 is whether the next defect is findable by reading
at all, or whether M7 now needs to be *run* — against a scratch branch, reporting
what the code actually does — before another round of paper review can add
anything. **Treat "dispatchable" as a claim requiring evidence of convergence, not
a status reached by running out of findings.**

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
change is `Bundle::open`'s **and `Bundle::create`'s** signatures, at **60** and 32
call sites. *(Was "57 and 32". Corrected in review round 2 — round 1 corrected
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
   > `COMPANION_VERSION` stays **0.14.0**; `parse.rs`, `vectors.rs`,
   > `textproj/src/lib.rs` and `spec/text_projection.tex` are **NOT** touched by
   > this rung and are deliberately absent from §2; and the corpus keeps
   > `canonical_bases` reach **0**. A future rung may restore the capability —
   > that is its own contract, with the four touch rows and the companion-version
   > bump this one declines.

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

**Pin 3 — `BundleCapabilities`, required at both constructors, carried on the
`Bundle`.**

```rust
pub struct BundleCapabilities {
    pub current_reduction_version: ReductionAlgorithmVersion,
}
```

**It MUST NOT implement `Default`, and MUST NOT expose a bundle-local
constant.** Both would let a caller use a bundle without stating what semantics
it implements, which is the defect this rung exists to remove.

`Bundle::open(store, caps)` **and `Bundle::create(store, …, caps)`** both take
it; the `Bundle` **stores it**. `commit` and `commit_versioned` then validate
against `self`'s copy and **their 57 call sites are unchanged** — the capability
is a property of the session, not of each call. Naming it
`BundleCapabilities` rather than `OpenCapabilities` follows from its now
governing creation and commit.

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
call.** Provide a clearly-named constructor for fixture use, e.g.
`BundleCapabilities::synthetic_for_fixture(v: u32)`, whose doc states it is for
format and container fixtures deliberately exercising arbitrary wire values and
**must never appear in a production composition path**. Production paths wrap
`epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION`.

Every call site converted by this rung uses one or the other **explicitly**;
none may take a value that merely happens to be in scope.

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
| 12 | `crates/epiphany-testkit/tests/requirement_labels.rs` | **conditional** — pin 9, *only if* it mints a new `\label{req:...}`; `CORE_REQUIREMENT_COUNT` (`:15`) then moves 213 → 214. Added in review round 1. If pin 9 mints no label, leave unmodified and say so in the report |

**Row 12 is conditional, and that is deliberate.** `CLAUDE.md` names this file as
a recurring escapee, and it escaped the format-epoch rung's table. Carrying it
conditionally costs nothing if unused; omitting it costs a silent drop-out.

**`spec/text_projection.tex`, `crates/epiphany-textproj/src/{parse,vectors}.rs`
and `crates/epiphany-textproj/src/lib.rs` are deliberately ABSENT** — see the
ruling under inherited obligation 2. M7 edits the refusals temporarily and
restores them by hand; nothing there is staged. **If any of those files shows up
in `git diff --cached`, M7 was not restored** and gate 4 must fail.

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


1. **`open_succeeds_when_base_and_current_reduction_versions_match`**
2. **`open_rejects_a_valid_stale_canonical_base`** — base and superblock agree
   with each other, `caps.current` differs. Must return
   `CanonicalBaseRequiresRebuild { base, current }` with **both fields
   asserted**, not merely the variant.
3. **`a_corrupt_base_superblock_disagreement_still_fails_as_malformed`** — the
   pin-6 path, asserted to be the *existing* malformed error and **not**
   `CanonicalBaseRequiresRebuild`.
4. **`a_bundle_with_no_canonical_base_opens_at_any_reduction_version`** — run at
   two different `caps` values.
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

**M1 — the new check fires.** Delete pin 5's comparison; test 2 must fail.

**M2 — the paths are distinguishable.** Make pin 4's error subsume pin 6's
(return `CanonicalBaseRequiresRebuild` for the corrupt case too); test 3 must
fail. Signs that §0.1's distinction is real in the code, not only in prose.

**M3 — the no-base exemption is deliberate.** Make the check run when
`canonical_base` is `None`; test 4 must fail.

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

Change `CURRENT_REDUCTION_ALGORITHM_VERSION` from `0` to any other value. Test
10a must **fail**, because it asserts
`bundle.capabilities().current_reduction_version == ReductionAlgorithmVersion(0)`
with `0` written as a **deliberate literal** — the independent operand.

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

**M6 — the writer check fires.** Remove pin 3a's commit-side validation; test 5
must fail.

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
a **base-bearing bundle regardless of whether the version matches**, then confirm
that **test 9** — `an_unrelated_commit_on_a_base_bearing_bundle_succeeds` — starts
failing. That state **is** reachable (test 6 establishes it), so the mutation
runs, and it signs the same thing the original was reaching for: that *"newly
emitted or replaced"* is a deliberate scope and not an accident of where the
check was placed.

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

1. **Build `B`, the validated reference**, in a crate that can reach the real
   constant: create a bundle with `caps` derived from
   **`CURRENT_REDUCTION_ALGORITHM_VERSION`** and commit a canonical base carrying
   that same version, so **pin 3a's writer check validates it on the way in**.
   That — and only that — is a genuinely validated base.
2. **Export `B` to text.** `document_from_bundle` to a `TextDocument`, then the
   crate-private `render_text_document` — which **does not refuse**, and exists
   precisely so the base spelling can be produced for a negative vector.
3. **Parse that text back** with `parse_document`, giving `D`.
4. **`A = serialize_document(D)`**, with **`B`'s `FileUuid`**.
5. **Compare `A.image()` with `B.image()`.**

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

**Compare the complete `image()` bytes of both bundles.** If they are equal, the
observation is made and nothing further is required.

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

**What the observation must show, or it has not been made:** that the capability
check does **not** fire, and that **no enumerated field differs** — the document
launders precisely *because* its number matches, and no check can tell a
coincidence from a rebuild.

> **This mutation is informative in both directions, which is why it is worth
> running.** If every field matches, the refusal is justified and the format
> rung's reasoning is confirmed by observation for the first time. **If any field
> differs, that field is a provenance signal nobody knew existed** — the text
> refusal may be stronger than it needs to be, and that is a finding for a future
> rung, not something to suppress because it contradicts the expected result.

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
2. `cargo clippy --workspace --all-targets -- -D warnings` → clean.
3. `cargo fmt -p epiphany-ops -p epiphany-bundle -p epiphany-testkit -p
   epiphany-textproj --check` → clean. **`cargo fmt --all` is forbidden.**
4. `git diff --cached --check` clean after staging; staged list exactly §2.
5. **`epiphany-ops` has NOT gained an `epiphany-bundle` dependency** — check
   `crates/epiphany-ops/Cargo.toml` directly.
6. **`BundleCapabilities` implements no `Default`**, by exact query with an
   expected count of **zero**:
   `grep -rnE "impl +Default +for +BundleCapabilities|derive\\([^)]*\\bDefault\\b[^)]*\\)[[:space:]]*(pub )?struct BundleCapabilities" crates/epiphany-bundle/src/`
   → **0 matches.** Run it against production source only; report the count, not
   a verdict.
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
7. `spec/vectors/decode_vectors.txt` unmodified; no schema major/minor moved.

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

**The executing agent MUST NOT commit.** Leave the work staged.

**No execution work may begin at all** until this contract is ratified — **with the
single exception granted in the status block above**, which authorises a
base-free, discard-only mechanical probe of M7's round-trip machinery on a
disposable branch, and nothing else.

---

## §7. Report requirements

*Counts corrected 2026-08-07 in review round 1 — items 1, 2 and 4 each named a
figure the document had outgrown. This is the same drift as §3's header and gate
1: **three independent stale counts of the same three lists.** Prefer "every item
in §N" to a number.*

1. **Every mutation in §4** — currently **eight** (M1, M2, M3, M4, **M5a**,
   **M5b**, M6, M7), each with verbatim output. *(M5 split in review round 2.)*
   **M4 is a recorded prohibition** (observed to compile, then reverted) and
   **M7 is a demonstration whose expected outcome is success**. Neither is a
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
   | M1 | test **2** fails |
   | M2 | test **3** fails |
   | M3 | test **4** fails |
   | **M4** | **no test — none is possible.** It owes the observation that the `Default` impl **compiles**, and the explicit statement that no test can catch it. That is why pin 3's prohibition is a review rule |
   | M5a | test **10a** fails, **plus** the provenance of the asserted operand |
   | M5b | test **10b** fails with both error fields asserted, **plus** the provenance of *both* operands |
   | M6 | test **5** fails (first half); test **9** fails (second half) |
   | **M7** | **no test — its expected outcome is success.** It owes the comparison M7 specifies, and confirmation that the refusals M7 names as removed were restored. **Counts, methods and file names live in M7, not here** — this cell said "all three refusals" until round 8 and "field-by-field enumeration" until round 9, each time describing a method M7 no longer used |

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
5. A count of call sites updated per crate, against §0.4's table **as corrected
   in review rounds 1 and 2** (open **60**, create **32**) — any discrepancy is a
   finding. Count `Bundle`-typed receivers, not the token `.commit(`.
   *(Attribution fixed in round 3: round 1 moved §0.4's table 57 → 60; round 2
   fixed the "Rung type" paragraph, touch rows 2 and 5, and struck the
   `project.rs` production claim that made the table's textproj entries
   misleading. Both rounds are load-bearing here.)*
6. **Confirmation that every refusal M7 removed was restored** — M7 names which,
   and the count is not repeated here; it said "three" from round 1 until round 9
   caught it, having survived round 8's correction of that same number in two
   other places — and that none of
   `text_projection.tex`, `textproj/src/parse.rs`, `textproj/src/vectors.rs` or
   `textproj/src/lib.rs` appears in the staged diff.
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
