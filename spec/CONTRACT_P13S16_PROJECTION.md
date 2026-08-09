# Contract — P13-S16: the projection gets maintained

**Status:** **DRAFT — UNBLOCKED 2026-08-09. NOT RATIFIED, and therefore NOT
dispatchable.**

**P13-S27 landed and was accepted at `4df8e25`**, so pin 0's blocker is discharged: an
authority now defines the implementation's current reduction semantics
(`epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION`, currently **`0`**), and
`core_spec.tex:11614`'s requirement is **met** — a stale canonical base is refused with
`CanonicalBaseRequiresRebuild` on both the read and write paths.

**What remains before dispatch is this contract's own ratification.** It is complete and
ratifiable as a *plan* and has **not been through adversarial review rounds**.
*Unblocked* and *dispatchable* are different states, and conflating them is how the S27
contract came to be ratified after a single round and then have that ratification
withdrawn — see its status block.

**This rung's first act is bumping the authority to `1`** (**pin 12**), because it
changes `CreateStaffGroup`'s reduction verdict. That bump is the discipline S27
installed: no mechanism can detect a semantics change, so the bump is the entire
guarantee.

### Draft amendment 1 — 2026-08-09, on ratification reconnaissance. NOT a review round.

**Fourteen findings, thirteen blocking**, against the draft **before** its first
ratification round. Nine came from the reconnaissance pass, five from a second sweep of
the same defect classes. **The contract is a DRAFT, so these are edits, not amendments to
frozen pins** — the distinction S27's status block draws, applied here.

| # | Finding | Disposition |
|---|---|---|
| **1** | **§2 omitted `epiphany-ops/src/lib.rs`**, though the rung's defining act is bumping the authority there. The bump had **no pin, no touch row, no gate and no report item** — it lived only in pin 0's discharge note | **Pin 12** added, with **touch row 7**, **gate 10**, **report item 2b** |
| **2** | **Invariant 21 is a TWO-crate change, not one.** `violating_score` (`generators.rs:498`) matches `GraphInvariant` **exhaustively** — invariant 21 does not compile without an arm — and four `all()`-driven tests then require a **real** generator | **Touch row 8** added, with the consumer surface enumerated. **This is the amendment's root cause**; everything else is bookkeeping by comparison |
| **3** | **S27's two tripwires fail at gate 1 with no touch row.** `roundtrip.rs` test 10b `panic!`s by design once the authority moves; `serialize.rs` test 10a asserts the literal `0` and its own doc says it *is expected to fail when S16 bumps* | **Touch rows 9 and 10**, plus **gate 11** requiring they be updated and not silenced |
| **4** | **`requirement_labels.rs` — the escapee `CLAUDE.md` names by name — was absent**, and the contract never decided whether pin 6 or pin 10 mints a label. It touches **both** counted documents | **Pin 10a** forces the decision; **touch row 11** conditional; **report item 2c**. All three counters named, per S27's finding |
| **5** | **Gates 2–3 pinned no toolchain**, in a repo whose CI gates on **1.95.0** while this machine's default is **1.97.1** | Both pinned |
| **6** | **Gate 3 formatted two crates of four** — and rows 9–10 added the other two, so it would have reported clean over unformatted code | Expanded to ops, core, textproj, testkit |
| **7** | **Gate 1 had no baseline and no ignored requirement** | Baseline **1577 / 0 / 0**, three delta buckets, ignored MUST be 0 |
| **8** | **Gate 4 said "exactly §2"** — the formulation S27's round 17 found **unsatisfiable** with a conditional row, and row 11 is now conditional | Subset both ways, with the unused row named in the report |
| **9** | **M1, M2, M4 accepted "the test failed"** as their whole signature | Each now names the **behaviour** the mutation produces. M3, M5, M6, M9 already met the standard |
| **10** | **M7 covered two independently guarded doc blocks but reverted "one doc comment"** — it could pass while the other guard stayed weak | Split into **M7a** and **M7b**, each quoting its own needle's non-match |
| **11** | **Pin 8 identified four tests by INTERIOR line numbers** (`:16838` etc.), roughly a dozen lines into each body — anchored to nothing searchable | All four **named**, with `fn` lines given second |
| **12** | **`t6`/`t7`/`t9` locators stale** | Re-derived to `:16158`, `:16231`, `:16461`, with the **two-`t6`-families trap** recorded |
| **13** | **The bump falsifies live statements in `CLAUDE.md` and the handoff** | **§4a** added: explicitly **not staged** during execution — staging them would assert the rung had landed while it awaited acceptance — and required as a post-acceptance reconciliation the report must list as outstanding |
| 14 | **`shrink` also takes `GraphInvariant`; whether it matches exhaustively was not established** | **Report item 2d** requires execution to determine it. **Deliberately not guessed** — guessing about an exhaustive match is what produced finding 2 |

**Finding 2 is the one that changes the rung's shape.** The others make an existing plan
checkable; this one says the plan was scoped to the wrong number of files. **An enum
extension in a crate with a generated exhaustive consumer is never local**, and nothing
in the contract's own §0 inspection would have surfaced it — it appears at compile time,
after execution begins.

**Findings 5–8 are all defects S27 had already found and fixed in its own gate set.**
This contract predates those corrections and inherited none of them. **A ratified
contract's gate section is reusable knowledge**, and the next contract drafted here
should start from S27's §5 rather than from an older template.

### Draft amendment 1, revision A — independent review of `d06e2f7`

**Six findings, five blocking. All six are in draft amendment 1's own text**, and four
of them are the same mistake: **importing an S27 conclusion instead of re-deriving it
against this rung's facts.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **Report item 2d asked execution to decide a static fact the draft could read.** `shrink` (`generators.rs:932`) does **not** match `GraphInvariant` — it calls `check_invariant(score, inv)`. The claim was made twice, in item 2d **and** touch row 8 | Both corrected to state `shrink` is generic. Item 2d replaced with the **real** obligation: invariant 21's fixture must **survive shrinking** (`:1025`), since `shrink` asserts its input still violates the target |
| **2** | **Pin 10a's "all three counters move if either document mints a label" is FALSE.** `CORE_REQUIREMENT_COUNT` is asserted only against `core_spec.tex` (`:259`); a label in `operation_catalog.tex` moves the two suite counters only | Replaced with a per-document table. **S27's "name all three" was derived for a rung touching one document**; importing it here made a correction into a new error |
| **3** | **Gate 11 permitted the exact tautology it exists to prevent.** "Updated, not silenced" does not forbid replacing the literals with `CURRENT_REDUCTION_ALGORITHM_VERSION` — the tidiest-looking update, after which both operands move together and **M5a/M5b are vacuous** | Rewritten as **11a–e**: independent literal `1`, **never the constant**, each quoted. S27's round 3 caught this substitution and `roundtrip.rs:882` forbids it by name |
| **4** | **Gate 11 omitted `roundtrip.rs:947`**, test 10b's mutation-only `Err` arm. Left at `0`, **M5b aborts on the base comparison before reaching its two-field `panic!`** — failing at the wrong assertion and observing nothing | **11d** added, plus **11e** for the literal-preservation comments, whose reasoning is what stops the next rung making substitution 3 |
| **5** | **§6 demanded "the nine mutations (M1–M9)"** while M7's split makes ten executions — a report could not both enumerate them and obey the tally | Count removed; **§3 is the single origin** |
| 6 | **Pin 12 said no gate catches a missed bump except the tripwires** — written in the same amendment that added **gate 10**, which compares the value against `HEAD` directly | Split: gate 10 guards *this* bump, 11a–e guard the wiring, and only the **general future case** stays undetectable |

**The tightening that came with finding 1 is the sharpest of the set.** M6's two fixtures
must each violate **one** direction only. A fixture disagreeing in both is still reported
after either arm is deleted, so the mutation appears to fail correctly **while signing
nothing** — and the same trap applies to touch row 8's generator, whose `all()`-driven
consumers only ask whether 21 is reported.

**Findings 1, 2, 3 and 6 share one root cause: an S27 conclusion applied without
re-derivation.** S27's "name all three counters", its "no mechanism can detect a
semantics change", and its literal-independence rule are all *true statements about
S27*. Two of them are false or incomplete here, and one was dropped exactly where it was
needed. **A ratified contract is reusable as a source of questions, not as a source of
answers** — which sharpens the note above about starting from S27's §5.

### Draft amendment 1, revision B — independent review of `3096c54`

**Three findings, all blocking, all in revision A's text — and each is a rule revision A
had just corrected, surviving one step downstream of where it was fixed.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **§6 item 2c still said "all three counters and their new values"** — the rule pin 10a corrected in the same revision. **Third site of one false claim**: the pin, then touch row 11, then the report item that *reads* the pin | Points at pin 10a's table; names which document minted the label and which counters moved |
| **2** | **§6 still demanded "the nine gate results"** while §4 carries **eleven** gates. Revision A removed the identical tally from item 1 for mutations and **left its neighbour standing** | Count removed, §4 named as origin, with 11a–e identified as subchecks of one gate rather than five results |
| **3** | **Touch row 8 still required the generator's fixture to violate "both directions"** while M6 requires direction-**isolated** fixtures — **incompatible evidence models in one contract.** A both-direction generator stays reported after either M6 arm is deleted | Row 8 now specifies **one named direction** plus shrink survival; pin 6/M6 own two separate isolated fixtures. `violating_score` returns one `Score` per variant and could not have carried both anyway |

**A third tally was found by sweeping and fixed with them:** gate 7 and §6 item 4 both
said *"the four pin-8 tests"*. Correct today, and the same construction — a count
restated away from its origin. Removed, pin 8's table named instead.

### Draft amendment 1, revision C — independent review of `0a5b936`

**Two blocking findings and one factual error. The first two are one issue:** invariant
21 mandates **two** directions, and only one — unspecified — had durable evidence.

| # | Finding | Disposition |
|---|---|---|
| **1** | **Neither direction had permanent named coverage.** Pin 6 asked for *"a score violating only invariant 21"* (singular), gate 6 asked for one, the generator carries one, and **M6 observed both only while mutated.** A mutation is reverted, so the restored suite could ship with one branch untested | **Pin 6a** added: two permanent direction-isolated tests, `m41_..._staff_names_absent_group` (S→G) and `m41b_..._group_lists_unowned_staff` (G→S), each required to *satisfy* the direction it does not break. **Gate 6 requires both**; **M6 now breaks those exact tests, one each, and requires the sibling to still pass** |
| **2** | **"One named direction" delegated a design decision to execution.** Either choice changes the generated witness and the shrink evidence, so reporting it afterward is not specifying it | **Pinned to S→G** in touch row 8, with the reason: smallest corruption of `valid_score`, matching every other arm's doctrine, **and the exact shape pin 2's append failing produces** — what M2 observes |
| 3 | **§6's revision-B history said §4 has "twelve entries — 1–11 plus 4a"**, a false identity: `4a.` was a gate-numbered **scope note** with no command and no output, colliding with **§4a**, the landing-obligation section | The note is **demoted out of the gate numbering** into gate 4's body, so no gate carries an `a` suffix and `§4a` is unambiguous. *(This cell stated a gate count until revision J; **§4 is the origin and no count is restated anywhere**.)* |

**Revision C's finding 1 is the strongest evidence for a rule this contract already
states and did not apply to itself.** Pin 3a says *"a mutation demonstrates the hazard
once; only a test keeps it demonstrated."* M6 was carrying both directions on mutation
alone, three sections below that sentence. **Every branch a contract mandates needs a
permanent test, and a mutation is that test's signature — never its substitute.**

### Draft amendment 1, revision D — independent review of `25473a1`

**Two blocking findings, plus one the sweep escalated.** Both reported findings are the
same failure in different clothes: **a requirement stated with nothing able to fail it.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **Pin 10a still deferred the label decision to execution** — twice reworded, never decided. The facts were readable the whole time: `core_spec.tex:6529`–`:6648` is **one `requirement` box** carrying the single label `req:graph:score-graph-invariants`, with exactly **20 `\item`s**; invariant 21 is a 21st `\item` **inside** it. Pin 10 rewrites prose plus a Revision History row | **DECIDED: neither document mints a label. Row 11 is UNUSED and MUST NOT be staged. No counter moves.** If execution finds otherwise, that is a **finding against this contract**, not a decision to take at the keyboard. The counter table is retained for that case |
| **2** | **Pin 6a required each fixture to violate its direction only, and nothing could observe it.** The prescribed shape, `m40`, asserts `check_invariants(&s).iter().any(...)` — **`any()` cannot see a second unrelated defect** — and gate 6 checked the target verdict and the opposite direction, but never the absence of invariants 1–20 | Each `m41`/`m41b` must assert the **EXACT violation set**: one violation, `StaffGroupMembershipAgreement`, witness naming the direction's staff and group ids, opposite direction asserted satisfied. **Gate 6 reports the full return of `check_invariants` for both** |
| **3** | *(sweep)* **The same blind spot covers touch row 8's generator, and worse.** `negative_generators_are_reasonably_targeted` bounds `kinds: BTreeSet<GraphInvariant>` at `<= 3`, but **both directions of invariant 21 are the same variant** — they collapse to one element, so **no existing test can observe direction at all**; the other three `all()` loops assert only `!is_empty()` | Row 8 now requires a **dedicated permanent test** that the generator violates S→G and not G→S |

**Finding 2 names the failure mode precisely: a requirement no assertion can fail is not
a requirement.** Pin 6a demanded isolation and, in the same breath, pointed at a model
test that cannot check isolation. **Borrowing a test's shape imports its blind spots
along with its virtue** — `m40` was cited for its dispatch property, which is real and
still applies, and nothing about invariant 20 ever turned on exactness.

**Finding 1 closes the last conditional in the contract.** *"Decide and report"* reads
like rigour and is its opposite: it makes the staged set and the counter expectations
depend on a choice made at the keyboard, so **the touch table can be wrong in either
direction and the report will agree with whatever happened.** Where the facts are
readable — and these were, in the `.tex` source — the contract decides.

### Draft amendment 1, revision E — independent review of `cae1d32`

**Two blocking findings and one stale rationale. Both blocking findings are in
requirements revision D itself wrote**, and both are its own closing lesson turned back
on it.

| # | Finding | Disposition |
|---|---|---|
| **1** | **Row 8's new generator test had no name, so nothing consumed it.** Gate 6 named only `m41`/`m41b`; §6 item 2d asks for shrink evidence. **Omitting the test entirely would still compile, satisfy all four `all()` loops, and pass every named gate** | Named **`invariant_21_negative_generator_breaks_staff_to_group_only`**, with its three required assertions spelled out; **added to gate 6** and to **§6 item 2e** |
| **2** | **Gate 6 demanded runtime evidence the prescribed tests cannot emit.** It said to quote `check_invariants`' full return and witness ids — but these are `assert!` tests in `m40`'s shape, and `cargo test` prints `ok`, not local values. Satisfying it literally would need **unpinned `--nocapture` instrumentation** or source inference presented as observation | **Evidence model chosen explicitly: quote the SOURCE assertions plus the pass verdict.** A passing exact-set assertion *is* the observation. Follows S27's gate 6c — a quoted source construct is read, not inferred — and adds no code to `epiphany-core` written solely for a report |
| 3 | **Gate 4's rationale still said row 11 is "conditional"**, which revision D changed to decided-unused. *(A fourth site under pin 10a said "carrying it conditionally costs nothing" — found by sweep)* | Both updated. **The subset rule itself is unaffected**; only its rationale needed the current term |

**Finding 1 is revision D's own lesson, unapplied to revision D.** It closed by
distinguishing a rule with no *consumer* from a rule with no *observer* — and then wrote
a requirement with neither. **An unnamed artifact cannot be gated**, because every gate
in this contract names what it checks.

**Finding 2 is the more interesting failure: a gate that specified the right thing to
know and the wrong way to know it.** Exactness is the correct requirement; *"quote the
runtime return"* was a mechanism borrowed from gates that run commands and read stdout,
applied to a unit test that emits nothing on success. **A gate must name evidence the
prescribed artifact actually produces** — otherwise execution improvises, and improvised
instrumentation is unpinned scope arriving through the report.

### Draft amendment 1, revision F — independent review of `abe2c35`

**One blocking finding, and it is both prior lessons at once.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **The shrink leg had no observable direction or exactness guarantee.** Row 8 requires the S→G fixture to *survive* shrinking, but the named test asserted only on the **raw** `violating_score(...)`; the shrunk score was checked solely by `every_invariant_shrinks_to_a_small_witness` (`:1003`), whose `!check_invariant(&small, inv).is_empty()` is **membership in one variant** — and **both directions are the same variant**, so a shrunk witness that flipped to **G→S-only** passes it. Because it calls `check_invariant` (singular), a shrunk witness that **gained an unrelated second defect** passes too. And §6 item 2d still said *"quote the shrunk witness"* — the unproduced-runtime-evidence defect revision E fixed in gate 6 **and did not carry one hop to item 2d** | The named test now asserts **the same three properties twice — raw and shrunk**. Gate 6 requires **both legs**; item 2d rewritten to revision E's source-assertion-plus-verdict model; §6 item 2e extended to both legs |

**This finding is the two prior lessons colliding.** Revision E established that *a gate
must name evidence its artifact produces* and fixed gate 6 — **stopping one hop short of
item 2d**, which is the fix-propagation failure revisions A–D kept recording. And the
underlying gap is revision D's: **a requirement — "survives shrinking" — with nothing
able to fail it.**

**The generalisable rule: `shrink` is a TRANSFORMATION, and a transformation's output
needs the same guarantees asserted of its input.** Requiring a fixture to "survive" a
transformation establishes only that *something* survived. Every property the input was
pinned for must be re-asserted on the output, or the transformation is free to change
what the fixture proves.

### Draft amendment 1, revision G — independent review of `2818ced`

**One blocking finding: M6 was unexecutable against a conforming implementation.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **M6 assumed two independently removable arms; pin 6 required only behaviour.** A single shared comparison — one walk emitting a violation whichever way `Staff.group` and `StaffGroup.members` disagree — satisfies `m41`, `m41b`, the generator test and gate 6, **and leaves nothing for M6 to delete one at a time.** Deleting the shared check disables both directions, so M6's *"one fails, the sibling passes"* observation **cannot be produced**. M6 was executable only against one implementation style | **Pin 6b** pins the mutation surface: two `GraphIndex` methods, `check_staff_names_absent_group` and `check_group_lists_unowned_staff`, both dispatched from `check_invariants`. **M6a/M6b delete them by name**, and **gate 12** proves both exist and are both called *before* M6 is attempted |

**This is the unexecutable-mutation class, which S27 hit twice** — its M5 and M6 both had
to be rewritten after review found no runnable observation behind them. **The tell is
identical: a mutation phrased as an edit to a structure the pins never required.**
Behaviour pins constrain outcomes; a mutation deletes *code*. **Where a mutation is the
signature, the structure it deletes must itself be pinned** — otherwise the contract is
satisfiable in a shape that makes its own evidence unobtainable.

**Pin 6b is precedent, not invention:** `check_invariants` (`invariants.rs:257`–`:282`)
already dispatches **23** `check_*` methods for **20** invariants, so more than one
method per invariant is the crate's existing shape. And a shared helper the two methods
both call is explicitly permitted — **the deletable call site is what M6 needs, not a
duplicated walk.**

### Draft amendment 1, revision H — independent review of `09d8439`

**One blocking finding, and one of the same family found by sweeping.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **Gate 12's four-line aggregate could pass with one surface missing.** Both definitions can exist (2 lines) while `check_invariants` calls `check_staff_names_absent_group` **twice** and `check_group_lists_unowned_staff` **never** (2 lines) — **four lines, gate passes, and M6b has no call site to delete.** The count proved a population, never a pairing | Replaced by **four independent `grep -c` checks, each required to be exactly `1`** — a count **above** 1 now also fails — plus **quoted context**: each definition with its enclosing `impl GraphIndex<'_>` header, each dispatch with the `pub fn check_invariants` header |
| 2 | *(sweep)* **Gate 8 asserted an absence with no method.** *"contains the empty-members refusal and no member-liveness/`TargetMissing` path"* named no command, and a `TargetMissing` path can be spelled without either literal | **Method pinned: quote the production body in full and read it**, explicitly not a grep. S27's gate-6a lesson; **M8 signs exactly this**, so a vacuous gate 8 leaves M8's deletion unobserved |

**Finding 1 is the count-versus-mapping failure, and it is this contract's oldest defect
class wearing new clothes.** Revisions A–C removed counts that had gone *stale*; this one
was **never right** — an aggregate can be satisfied by the wrong distribution of the
same total. **Where a gate must establish a mapping, it cannot count.** It has to check
each element on its own, which is the structural sibling of the rule this document
already carries: *where a claim requires completeness, do not enumerate — derive.*

**Both findings are gates that report success without observing what they claim.** One
counted instead of pairing; the other asserted an absence with nothing able to establish
it. **A structural gate needs a method, and the method must distinguish the passing case
from every failing one** — not merely from the most obvious failing one.

### Draft amendment 1, revision I — independent review of `5d2db93`

**One blocking finding: gate 8's new method named a boundary that made the gate
impossible to pass or honestly fail.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **Gate 8 said "quote the production body to the `#[cfg(test)]` boundary".** `create_staff_group` begins at `reduce.rs:4458`; **`create_part_definition` begins at `:4515`** with its own `PreconditionFailureReason::TargetMissing` at **`:4554`**; the next `#[cfg(test)]` is at **`:9576`**. **The literal read spans ~5,000 lines and always contains the path the gate says must be absent** — while any shorter read violates the stated boundary | Gate 8 now **quotes exactly pin 1's slice**: `create_staff_group`'s body, **brace-matched from its `fn` line to its closing brace**, production source only |

**Pin 1 had the boundary right the whole time** — *"slice the `create_staff_group` body
(brace-matched from its `fn` line to its closing brace, production source only)"*.
**Revision H invented a second, looser boundary instead of citing the pin.** That is
revision A's failure in a new place: importing a plausible-sounding rule rather than
re-deriving from the source that owns it — there, an S27 conclusion; here, a boundary
from a different kind of check entirely. *(The `#[cfg(test)]` boundary is the right
instrument for "is this call site production or test?", which is what §0.4 used it for.
It is the wrong instrument for "where does this function end.")*

**The rule: where a pin already defines the artifact, the gate CITES the pin — it does
not redescribe it.** A redescription is a second definition, and two definitions of one
artifact are a contradiction waiting for someone to read the looser one.

**And note what kind of failure this was:** not a gate that passes when it should fail —
revision H's usual shape — but **a gate with no passing state at all.** It would have
been discovered at execution, by an agent forced to choose between obeying the boundary
and obeying the requirement, and whichever it chose would have been reported as a pass.

### Draft amendment 1, revision J — independent review of `9e43994`

**One blocking finding in three live consumers: adding gate 12 restored the tally defect
this contract had already removed twice.**

| # | Finding | Disposition |
|---|---|---|
| **1** | **Three sites still said §4 has "eleven gates, 1–11"** after revision G added gate 12: revision C's disposition for its finding 3, gate 4's scope note, and **§6 item 2 — immediately after the words "No count is stated here — §4 is the single origin."** | **All three counts REMOVED, not updated.** The revision-C record now says only that the `4a.` scope note was demoted; gate 4's note identifies `§4a` without counting gates; §6 item 2 keeps the pointer to §4 and adds only that lettered subchecks report under their gate |

**§6 item 2 is the one that indicts the method.** It declared §4 the single origin and
restated a count in the same breath — **the defect naming itself.** Revision B removed
*"the nine gate results"* from that very item; revision C's correction then wrote the
then-current number into the explanation, and revision G's new gate made it false again.

**The rule this makes explicit: a correction that EXPLAINS a removed count must not
restate the corrected value.** Say what changed, not what the number now is — otherwise
the record becomes a new instance of the defect it records, and the next addition to the
set falsifies the explanation instead of the original. **Every count this contract has
removed was re-created by the prose written to remove it.**

**What the sweep confirms is still sound**, so the rule is not "no numbers anywhere": a
count **at its origin, immediately above the table that enumerates the set** — pin 1a's
three revised tests, pin 8's four G3a tests — is read off, not restated, and a change to
the set edits the table and the adjacent word together. **The defect is a count living
away from the set it counts.**

**The pattern across revisions A–J is sharper than any individual finding: a correction
propagates one hop and stops.** Rev A fixed pin 10a and left touch row 11; the sweep
caught row 11 and stopped before §6's consumer; rev D found pin 10a's *decision* still
deferred after two reworders. Rev A removed item 1's mutation tally and left item 2's
gate tally on the next line. **The fix-every-site rule is not satisfied by fixing the
site *and* its obvious neighbour** — it requires asking who *reads* the corrected rule,
and correcting them too.

**And rev D adds its converse:** ask what *observes* each requirement. Findings 2 and 3
were both requirements with no assertion able to fail them — invisible to every sweep
that looks for restated text, because nothing was restated. **A rule with no consumer
goes stale; a rule with no observer was never enforced at all.**

> **The original status, retained:** *DRAFT — BLOCKED on P13-S27. Not executable as
> written. Pin 0 exposes that no authority defines the implementation's current
> reduction semantics, and prose saying old canonical bases "must be rebuilt" does not
> make them unusable. `core_spec.tex:11614`'s requirement stays unmet until S27 supplies
> a version authority and a rejection-or-rebuild path.*

**Rung type:** **canonical reduction-semantics change.** This is stronger than
"behaviour change" and the first draft of this contract understated it. The same
operation set now reduces to a different canonical `Score`: a graph that reduces
cleanly today is refused, and a field that was never written is written.
Disposition **A** of `spec/CONTRACT_GENESIS_G3A_ENTITIES.md` §1.1, ratified
there as "the later maintenance/enforcement fix" and sequenced after G3b — which
has landed.

**No schema major or minor moves and no wire byte changes** — but the Operation
Catalog's version *does* move (pin 10), and the reduction-semantics question is
**separate from the schema/wire version question**. See pin 0.

**No policy ruling is owed.** Disposition A is already ratified. What follows is
its implementation.

---

## §0. What was verified before drafting

Read out of the working tree at `f876836`, not recalled. Every line number below
was confirmed by reading the line.

### 0.1 The refusal needs no new machinery — and this is the rung's biggest saving

Disposition A says `CreateStaffGroup` "MUST carry `members: []`; a non-empty
`members` is refused or normalized away at construction." An earlier reading of
this contract assumed refusal required a new `PreconditionFailureReason` — which
would have made S16 a schema-minor epoch event with a footprint in
`binary_format.tex`, `PLAN_GMINOR_SCHEMA_MINOR.md`, `decode.rs`, and the history
test. **It does not.**

`reduce.rs:1236` already defines the shared helper, and its own doc comment
already covers this case:

> The precondition no-op a structural create or delete returns when a container
> is non-empty where the operation requires it empty (**a create carrying
> children**, or a delete of a container with live children).

Three creates already call it for exactly this reason — `create_region`
(`:4174`), `create_staff_instance` (`:4246`), `create_voice` (`:4310`) — each
refusing a carried value that bears a separately-minted typed child.
`create_staff_group` is **the sole outlier**: it accepts carried `members` and
merely checks each is live (`:4489`–`:4502`).

So the refusal is the **fourth instance of an established pattern**, not a new
one. `PreconditionFailureReason::ContainerNotEmpty` stays at discriminant 10.
**No new reason, no epoch, no wire change, no accept-set move.**

### 0.2 Normalizing at construction or decode is excluded, on ratified grounds

Disposition A's "or normalized away" alternative must **not** be taken. The
committed decode vector at `ops/src/vectors.rs:829` pins 130 literal bytes of a
`CreateStaffGroup` envelope whose payload carries `members = [StaffId(…)]`, and
its contract is decode-then-re-encode injectivity. The text-projection golden at
`textproj/src/vectors.rs:198`–`:201` carries the same value. Folding `members`
away at decode would break both, and violates `req:binfmt:decode-vectors`
(`binary_format.tex:3404`ff): *"Canonical decode is injective: distinct byte
strings denote distinct values."*

**Refusal happens at reduction. Decode is untouched, and both artifacts survive
unchanged.** This is the same fork P13-S8 faces, resolved the same way, for the
same ratified reason.

### 0.3 The authorship cycle makes empty-at-mint the only self-consistent rule

`create_staff` requires a carried `group` to be live (`:4372`–`:4383`);
`create_staff_group` requires each carried member to be live (`:4489`–`:4502`).
There is **no `ModifyStaff`, `ModifyStaffGroup`, `DeleteStaff`, or
`DeleteStaffGroup`** anywhere in `OperationKind` (`payload.rs:170`–`:307`;
confirmed normatively at `operation_catalog.tex:1288` and `:1553`). With mints
only, no authoring order produces an agreeing pair.

Therefore the *only* authorable agreeing sequence is: mint the group empty, then
mint staves naming it. Requiring `members: []` is not an arbitrary restriction —
it is the sole rule consistent with the operation surface that exists.

### 0.4 The re-carry hazard, and the base-ingest hazard hiding behind it

`create_staff_group`'s idempotence check compares `op.group` against
`self.staff_group_values` (`:4466`–`:4469`). If maintenance wrote the appended
members into that map, a byte-identical re-carry of `CreateStaffGroup(g, [])`
would compare `[]` against `[s]` and return `RecreateContentMismatch` instead of
`AlreadyApplied`. Disposition A names this sub-pin explicitly.

**The non-obvious half:** base ingest (`:1611`–`:1620`) reseeds
`staff_group_values` from `score.staff_groups` — the *maintained* value. So even
if reduction keeps carried and derived apart in one session, a snapshot round
trip launders the derived value into the carried slot, and the same misverdict
returns **after a reload**. A test that never reloads cannot see it.

The resolution falls out of §0.1: once a non-empty carried `members` is refused,
**the carried value is by construction always empty**, so the base seed can
reconstruct it exactly rather than approximately (pin 4).

### 0.5 `t8b` is the obstacle, and it inverts rather than dies

`t8b_both_permitted_stale_forms_hold` (`reduce.rs:16339`, doc `:16317`–`:16337`)
asserts both stale forms **hold**, and its doc block names disposition A's two
maintenance rules as **mutations that must make those assertions fail**. The
test is a correctly-built detector for precisely this change.

It is therefore rewritten, not deleted: the same two authoring orders, with the
verdicts inverted — the missing order now yields agreement, the spurious order
now yields `ContainerNotEmpty`. Its doc block's mutation notes become the
rung's own mutation evidence, pointing the other way.

### 0.6 Invariant 21 has real work left after maintenance

Maintenance plus refusal does not make disagreement impossible:

- **Undo.** `reduce.rs:2967` removes a `Staff` from `score.staves` on undo but
  leaves its id in any live group's `members`. The reverse direction *is*
  guarded (`:6736`–`:6744`, which blocks undoing a group still named by a live
  staff); **this direction is not.**
- **Base ingest.** A blob authored before this rung, or by another
  implementation, can carry a disagreeing pair straight in.

There are 20 invariants (`invariants.rs:149`, count guard `:6064`). 21 is free.

---

## §1. Pins

> ### PIN 0 IS DISCHARGED — read this before the pin. P13-S27 landed 2026-08-09.
>
> **Everything in pin 0 below is a dated record of the pre-S27 tree**, and its three
> numbered requirements are superseded. Its claims that no constant names the current
> reduction semantics, that a conformingly-propagated stale base is accepted, that
> `ids.rs:288`'s catalog claim is false, and that **this rung therefore cannot execute**
> are **now false**.
>
> **But one of pin 0's claims is STILL TRUE and S27 did not touch it: there is no
> mechanism to detect a reduction-semantics change.** S27 enforces *declared-version*
> mismatches; it cannot tell that the semantics behind a version number changed. **Do
> not read the discharge as closing that gap** — see *Enforcement is not detection* at
> the end of this pin, which is why this rung's bump to `1` is mandatory.
>
> **The discharge, with each claim answered individually and the replacement
> requirements, is at the end of this pin.** Read the pin as history; do not execute it.

**Pin 0 — declare the reduction-semantics break; do not pretend it is
containable.**

`core_spec.tex:14369`–`:14372` is normative: *"Two replicas with different
`ReductionAlgorithmVersion` may produce different canonical states from the same
operation set; the active superblock declares the version under which the
bundle's canonical base was materialized."* And `:11614`–`:11617`: *"Snapshots
produced under an earlier algorithm version cannot be used as canonical bases
under a later one without rebuilding."* This rung is exactly such a change.

**The obvious disposition — bump the version — is not available, and the reason
is subtler than an absence of machinery.** The machinery exists; it is
**self-referential**, so it cannot detect what it appears to guard.

`ReductionAlgorithmVersion` (`bundle/src/ids.rs:291`) is a wire field of the
bundle superblock (bytes `68..72`, `superblock.rs:20`), and there **is** a
production writer path:

- `reduction_version_for` (`bundle.rs:989`) sets a new superblock's version from
  **the canonical base's own reported version**, or `default()` (zero) when
  there is no base — its doc: *"only a base records a reduction."*
- `open` (`bundle.rs:396`–`:399`) rejects a bundle whose base's version
  **disagrees with the superblock's**.

So a document's declared reduction version is sourced from the base and then
checked against itself. **Nothing anywhere compares either value against the
semantics the running implementation actually implements.** A base reduced under
old semantics carries its own version forward, agrees with the superblock it
seeded, and is accepted. **The check is not vacuous** — it catches a corrupt or
tampered base whose version disagrees with its superblock. What it cannot catch
is a *valid* stale base: one whose version was conformingly propagated. So the
check **necessarily passes for a conformingly propagated stale base**, which is
exactly the case `core_spec.tex:11614` exists to prevent.

*(An earlier draft of this pin claimed "every construction site is an unrelated
hardcoded literal." That was false, and the error is instructive: the search
behind it looked for `ReductionAlgorithmVersion(` constructor calls, which by
construction cannot find a path that propagates an existing value without
constructing one. The instrument could not observe the thing it was used to
rule out.)*

Two supporting facts stand: there is **no constant or accessor naming the
implementation's current reduction semantics**, and **`ids.rs:288`–`:289` states
that "the algorithm catalog itself lives in `epiphany-ops`" while nothing of the
kind exists there** — a doc comment asserting a false fact about another crate,
which is **P13-S26's pattern, second instance.**

**Scope of the claim, deliberately narrowed:** this inspection establishes that
**the current implementation has no mechanism** to detect a reduction-semantics
change. It does **not** establish that no such change in the project's history
was ever detectable; that would need a history audit this rung has not done, and
the stronger sentence must not be written into the ledger.

**Therefore this rung does NOT invent the missing machinery**, which would be a
scope explosion and a separate design — **and, because it does not, this rung
cannot execute.** Pin 0 records the break; it does not discharge it. S16 waits on
S27's disposition. What pin 0 still requires of the eventual rung:

1. **State the break** in `spec/PASS13_CANDIDATES.md`'s P13-S16 row — which
   stays **blocked on P13-S27**, not resolved — and in `operation_catalog.tex`'s
   Revision History entry: canonical bases materialized before this rung must be
   **rebuilt**, not reused, because `create_staff` now writes
   `StaffGroup.members` and `create_staff_group` now refuses inputs it
   previously accepted.
2. **File P13-S27** — that the reduction-version machinery is
   **self-referential**: `reduction_version_for` (`bundle.rs:989`) sources a new
   superblock's version from the canonical base's own self-report, and `open`
   (`bundle.rs:396`) checks only that the two agree, so **the current
   implementation has no mechanism comparing either against the semantics it
   actually implements.** Supporting: no constant or accessor names the current
   semantics, and `ids.rs:288`'s catalog claim is false. **The stronger
   historical claim — that no such change has ever been detectable — is NOT
   established by this inspection and must not be written.** This rung is the
   occasion, not the cause.
3. **Assert nothing it cannot enforce.** No test may claim stale bases are
   rejected; nothing rejects them. The break is recorded, not guarded, and the
   contract says so plainly rather than implying coverage.

---

### PIN 0 IS DISCHARGED — P13-S27 LANDED 2026-08-09 (`4df8e25`)

**Everything above in pin 0 is a dated record of the pre-S27 tree and MUST NOT be
executed as written.** S27 built the machinery whose absence pin 0 documented, so the
pin's premises, its conclusion, and all three of its numbered requirements are
superseded. They are retained because the *reasoning* is what motivated S27, and because
`operation_catalog.tex`'s rebuild note still has to be written.

**Which of pin 0's factual claims are now FALSE**, stated individually so none is left
standing by implication:

| Pin 0 said | Now |
|---|---|
| "no constant or accessor naming the implementation's current reduction semantics" | **`epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION`** (currently `0`), plus `Bundle::capabilities()` as the accessor |
| "`ids.rs:288`–`:289` … asserts a false fact about another crate" | **Made true** by S27's pin 8 — the same doc comment now names the real location and mechanism |
| "a *valid* stale base is accepted — one whose version was conformingly propagated" | **False now.** A base whose **declared** version differs from the authority is refused with `CanonicalBaseRequiresRebuild { base, current }` on **both** the read path (`open`) and the write path (`commit`/`commit_versioned`) |
| "the current implementation has **no mechanism** to detect a reduction-semantics change" | **STILL TRUE, and S27 did not change it.** See the split immediately below — this row is the one an earlier draft of this supersession got wrong |
| "**this rung cannot execute**" | **It can.** This contract is UNBLOCKED — see the status block. It is still a DRAFT and needs ratification, which is a different bar |

### ENFORCEMENT IS NOT DETECTION — the split S27 did not close

**An earlier draft of this supersession declared pin 0's "no mechanism to detect a
reduction-semantics change" false because mismatches are now rejected. That was wrong,
and it contradicted requirement 3 thirty lines below it.** The two are different claims
and only one of them moved:

| | Declared-version mismatch | Semantics change |
|---|---|---|
| **What it is** | a base whose recorded `reduction_algorithm_version` differs from the running authority | the implementation's canonical reduction verdicts changing, with or without a bump |
| **After S27** | **ENFORCED** — refused with `CanonicalBaseRequiresRebuild` on read (`open`) and write (`commit`/`commit_versioned`) | **STILL UNDETECTABLE.** Nothing compares the semantics the code implements against the number it declares |
| **What guarantees it** | the check, mechanically | **a human remembering to bump.** There is no backstop |

**So S27's enforcement is conditional on the discipline, not a substitute for it.** If
this rung changes `CreateStaffGroup`'s verdict and the bump is missed, every base it
produces declares `0`, matches an authority still reading `0`, and **passes every check
S27 installed** — the enforcement fires correctly on a number that is itself wrong.
`epiphany-ops`'s authority doc states this outright: *no mechanism can detect a
semantics change; the discipline is the guarantee; there is no backstop.*

**This is precisely why pin 0's requirement 3 inverts into a mandatory bump rather than
dissolving.** S16 is the first rung to change a canonical reduction verdict, so **S16's
bump to `1` is the human-enforced half of the guarantee**, and the only half that
applies to itself.

**Pin 0's narrowing survives and still binds.** *This inspection does not establish that
no reduction-semantics change in the project's history was ever detectable* — S27 did
not perform that history audit either, and the stronger sentence still must not be
written into the ledger.

**What replaces the three requirements:**

1. **State the break** in `operation_catalog.tex`'s Revision History exactly as written
   above — canonical bases materialized before this rung must be **rebuilt**, not
   reused. **Unchanged.** But the ledger half is superseded: the P13-S16 row is
   **UNBLOCKED, not blocked on P13-S27** — see pin 11, amended the same day.
2. ~~**File P13-S27**~~ — **DISCHARGED.** S27 has its own row, its own ratified
   contract, and its own `RESOLVED — IMPLEMENTED 2026-08-09 (pin 10)` marker. Nothing
   here is left to file.
3. **Assert nothing it cannot enforce** — **the principle stands; its application
   inverts.** Stale bases *are* now rejected, and S27 owns the tests that prove it. So
   this rung must not re-assert S27's guarantee, and must not add a second detection
   path. **What it MUST do instead is bump `CURRENT_REDUCTION_ALGORITHM_VERSION` to
   `1`**, because it changes `CreateStaffGroup`'s reduction verdict. **No mechanism can
   detect a missed bump** — S27's authority doc is explicit that the discipline is the
   entire guarantee — so the bump is this rung's obligation and nothing will catch its
   absence.

> **Line-number citations in this contract predate S27 and are only PARTLY
> re-derived.** S27 changed `bundle.rs` by 795 lines, so `bundle.rs:989`, `:396` and
> `ids.rs:288` above — and every other `bundle.rs` reference in this document — remain
> stale as locators even where the claim about them is historical.
>
> **Draft amendment 1 re-derived the ones that name executable targets:** `t6`
> `:16154`→**`:16158`**, `t7` `:16229`→**`:16231`**, `t9` `:16454`→**`:16461`**, and
> pin 8's four interior line numbers replaced by test **names**. **The rest are not
> done** and finishing them is ratification work.
>
> **A trap the re-derivation found, recorded so the next pass does not fall into it:**
> `reduce.rs` contains **two `t6`/`t7` families** — `t6_undo_restores_the_chain_...`
> (`:15577`) and `t6_g3a_referential_loops_...` (`:16158`). **A re-derivation that greps
> `fn t6` and takes the first hit lands on the wrong test**, and both hits are plausible
> in context. Match on the full `_g3a_` name, never the prefix.

**Pin 1 — refuse a non-empty carried `members`, using the existing helper.**
In `create_staff_group` (`reduce.rs:4458`), before the liveness loop, refuse a
carried non-empty `members` with `container_not_empty()`. Match the idiom and
comment style of `:4174`, `:4246`, `:4310`. **Do not introduce a new
`PreconditionFailureReason`**; `ContainerNotEmpty` stays at 10 and no schema
document moves.

The liveness loop (`:4489`–`:4502`) becomes unreachable for non-empty members
and **must be deleted, not left dead** — a precondition that cannot fire is the
kind of residue this pass keeps finding.

**No behavioural test can prove that deletion.** A retained dead loop passes M1
and every other assertion here, because refusal short-circuits before it. Pin 1
therefore requires a **structural gate**: read
`crates/epiphany-ops/src/reduce.rs`, slice the `create_staff_group` body
(brace-matched from its `fn` line to its closing brace, **production source
only** — never the test module), whitespace-normalize, and assert the slice
**contains** the empty-members refusal and **does not contain** any
`TypedObjectId::Staff` liveness check or `TargetMissing` construction.

**Pin 1a — three existing tests assert what pin 1 removes; all three must be
revised.** Each currently *passes* by asserting the outgoing rule:

| Test | Line | Currently asserts | Under pin 1 |
|---|---|---|---|
| `t6` | `reduce.rs:16158` | `CreateStaffGroup.members` naming a non-live target is `TargetMissing`, one of three referential loops asserted separately | that arm becomes `ContainerNotEmpty` and stops testing a *referential* loop at all; the `CreatePartDefinition.staves` and `CreateView.active_layers` arms are untouched and must stay |
| `t7` | `reduce.rs:16231` | those same preconditions are **not** enforced base-free | pin 1's refusal is **not** graph-gated — it is a property of the carried value — so this arm now refuses base-free too, inverting the claim for that arm only |
| `t9` | `reduce.rs:16461` | a from-empty score passes `check_invariants`, and **each skipped reducer check independently makes invariant 10 fire**, on a fixture that already attempts a dangling reference in each of the four ops | the dangling-member fixture can no longer enter the graph, so both the fixture and t9's own mutation set change |

**`t7`'s inversion is the subtle one and must be stated in its doc block:** a
graph-aware precondition asks about the *universe* and cannot run base-free; an
empty-container precondition asks only about the *carried value* and therefore
runs everywhere. Different classes — conflating them is how a later reader would
wrongly "restore" the graph gate.

Each keeps its id and gains a doc note recording what it asserted before this
rung and why the assertion inverted.

**Pin 2 — maintain the projection in `create_staff`.**
In `create_staff` (`:4330`), after the graph push at `:4386` and only when
`op.staff.group == Some(g)`, append the new staff's id to `g`'s `members` **in
`self.graph`'s `score.staff_groups`**. Under base-free reduction
(`self.graph.is_none()`) there is no graph to maintain and nothing is written —
matching how `:4385` already guards the staff push itself.

Appending is set-union and order-independent per staff id; the append must be
idempotent (never add an id already present), since convergence replays.

**Pin 3 — carried and derived are kept apart, and `staff_group_values` holds the
carried value.**
`self.staff_group_values` (`:4507`) MUST continue to store the value **as
authored** — which pin 1 guarantees is always empty-membered. Pin 2 writes the
maintained members **only** into `self.graph`. The re-carry comparator at
`:4466` is not changed and must keep comparing against the carried value.

**Pin 3a — a permanent named regression test, not only mutation M3.** Add
`t8c_recarry_compares_against_the_carried_members_not_the_derived`: in one
session, `CreateStaffGroup(g, [])` → `CreateStaff(s, group: Some(g))` →
re-carry `CreateStaffGroup(g, [])`, asserting `AlreadyApplied` **and** that the
graph's `g.members == [s]` at that moment. A mutation demonstrates the hazard
once; only a test keeps it demonstrated.

**Pin 4 — base ingest reseeds the carried value, not the derived one.**
At `:1619`, seed `staff_group_values` with the group's value **with `members`
emptied**, not `group.clone()`. This is exact rather than lossy: pin 1 makes
empty the only authorable carried value, so the reconstruction is the carried
value. The comment at `:1614`–`:1618` must be extended to say why, naming the
reload hazard of §0.4 — otherwise a later reader "fixes" it back to
`group.clone()` and reintroduces a defect that only appears after a snapshot.

**Pin 4a — a permanent named regression test for the reload path.** Add
`t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent`: reduce
the pin-3a sequence, materialize the score, **re-reduce onto that score as a
base**, then re-carry `CreateStaffGroup(g, [])` and assert `AlreadyApplied`.
This is the only test that would exercise the base-seed path; pin 4 is
unguarded without it.

**Pin 5 — close the undo hole in the unguarded direction.**
The `Staff` undo `retain` arm (`:2967`) MUST also strip the removed staff's id
from every group's `members`, in both `self.graph` and — if any derived copy is
held — wherever else the projection lives. The `StaffGroup` arm (`:2977`) needs
no change: `:6736`'s reference guard already blocks undoing a group a live staff
names.

**Pin 6 — graph invariant 21, `StaffGroupMembershipAgreement`.**
Flags both directions: a staff whose `group` names a group whose `members` omit
it, and a group listing a staff whose own `group` is not that group. Witnesses
must name both ids and the direction.

This is an **append with a documentation footprint** — `invariants.rs:149`,
`all()`, the count guard at `:6064`, and `core_spec.tex`'s normative enumeration
(which currently ends at 20 and which **P13-S26 has just established is already
under-describing invariant 10** — do not attempt to repair invariant 10's prose
here; that is S26's rung).

**The count is not the test.** `all().len() == 21` passes with the dispatch arm
deleted — the project already knows this, which is why
`m40_check_invariants_dispatches_invariant_20` exists
(`invariants.rs:6045`–`:6066`, whose own doc says *"`all().len() == 20` passes
even with the dispatch arm deleted, so this row must instead show a score
violating ONLY invariant 20 is actually flagged by the top-level
`check_invariants` entry point"*). Pin 6 requires the **same behavioural shape**:
a score violating *only* invariant 21, flagged through `check_invariants`, with
the dispatch-arm deletion as its signing mutation. A count assertion may
accompany it and may not replace it.

**Pin 6a — TWO permanent, direction-isolated tests. ADDED IN REVISION C, which found
both directions mandated and only one durably covered.**

Pin 6 required *"a score violating only invariant 21"* — **singular** — and gate 6 asked
for one score. The generator carries one direction. **M6 observes both directions, but
only while mutated**, and a mutation is reverted: after restoration nothing in the
permanent suite need exercise the second branch. **A mutation demonstrates once; only a
test keeps it demonstrated** — this contract's own words under pin 3a, applied to itself.

Add **both**, named, in the shape of `m40_check_invariants_dispatches_invariant_20`:

| Test | Fixture violates | Must satisfy |
|---|---|---|
| `m41_check_invariants_dispatches_invariant_21_staff_names_absent_group` | **S→G**: a staff whose `group` names a group whose `members` omit it | the G→S direction, and every other invariant |
| `m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff` | **G→S**: a group listing a staff whose own `group` is not that group | the S→G direction, and every other invariant |

**Each fixture MUST violate its own direction only.** A fixture disagreeing both ways is
reported after either arm is deleted, so it signs neither — the same isolation M6
requires, made permanent. **Each test must state in its doc which direction it holds
and which it breaks**, so a later reader cannot "simplify" the two into one.

**Each test MUST assert the EXACT violation set, not membership. ADDED IN REVISION D.**

`m40_check_invariants_dispatches_invariant_20` — the shape this pin points at — asserts
only that the target **appears**:
`check_invariants(&s).iter().any(|v| v.invariant == …)`. **`any()` cannot detect a
second, unrelated defect**, so a fixture carrying one satisfies m40's shape, satisfies
gate 6, and satisfies M6's deletion outcome — while violating pin 6a's own
"only its own direction" requirement, which nothing then observes. **A requirement no
assertion can fail is not a requirement.**

So each of `m41` / `m41b` MUST assert:

1. **`check_invariants(&s)` returns EXACTLY one violation**, and its `invariant` is
   `StaffGroupMembershipAgreement` — an exact-set assertion, not `any()`. This is what
   proves invariants 1–20 are satisfied, which `any()` never touches.
2. **The witness names the specific staff and group ids** for that direction, so the two
   tests cannot pass on each other's fixture.
3. **The opposite direction is satisfied** on the same score, asserted directly.

> **Pin 6a required isolation and then prescribed a test shape that cannot check it.**
> The model test was cited for its *dispatch* property — that `all().len()` alone passes
> with the arm deleted — which is real and still applies. **Borrowing a test's shape
> imports its blind spots along with its virtue**, and m40 never needed exactness because
> nothing about invariant 20 turned on it.

**Pin 6b — the MUTATION SURFACE is pinned, not just the behaviour. ADDED IN REVISION G.**

Pin 6 and pin 6a specify **behaviour only**, and **a conforming implementation can
satisfy every one of them with a single shared comparison** — one walk that compares
`Staff.group` against `StaffGroup.members` and emits a violation whichever way they
disagree. That implementation passes `m41`, `m41b`, the generator test and gate 6, **and
leaves M6 with no two arms to delete.** Deleting the shared check disables both
directions at once, so M6's required *"one test fails, the sibling passes"* observation
**cannot be produced at all** — M6 would be executable only against one implementation
style, which pin 6 never required.

**So the two directions MUST be two independently removable checks**, following this
crate's existing idiom — `GraphIndex` methods writing into `out`:

```rust
fn check_staff_names_absent_group(&self, out: &mut Vec<InvariantViolation>)   // S→G
fn check_group_lists_unowned_staff(&self, out: &mut Vec<InvariantViolation>)  // G→S
```

Both emit `GraphInvariant::StaffGroupMembershipAgreement` violations, and
**`check_invariants` calls both**, in sequence with the existing `idx.check_*(&mut v)`
calls.

> **This is the crate's shape, not an invention for the mutation's convenience.**
> `check_invariants` (`invariants.rs:257`–`:282`) already calls **23** `check_*` methods
> for **20** invariants, so **more than one method per invariant is existing precedent**.
> The names are **PINNED**, because gate 12 greps for them and M6 deletes them by name —
> the same reason S27 had to pin `synthetic_for_fixture` after discovering its gate
> searched for a name the contract had offered only as an example.
>
> **A shared helper the two methods both call is permitted** — deduplicating the walk is
> fine. What is forbidden is a single call site for both directions, because the
> *deletable unit* is what M6 needs.

**Pin 6 is coupled to pin 5 and they split only together.** M5 signs the undo
hole by requiring invariant 21 to *observe* the residue. If invariant 21 is
deferred to a later rung, then either pin 5 and M5 defer with it, or M5 must be
restated to assert the leftover member **directly** on the materialized score
rather than through `check_invariants`. Splitting pin 6 out while leaving M5 as
written would leave the undo repair unsigned.

**Pin 7 — `t8b` inverts.**
Rewrite `t8b_both_permitted_stale_forms_hold` as
`t8b_the_projection_is_maintained_and_the_spurious_form_is_refused`: same two
authoring orders, verdicts inverted. Its doc block must record that it
previously pinned the opposite, and why the change is the ratified disposition
rather than a regression. **Deleting it is forbidden** — the pairing of the two
orders is the coverage.

**Pin 8 — the four G3a undo-repair tests are re-verified, not assumed. NAMED by draft
amendment 1.** All four are in `crates/epiphany-ops/src/reduce.rs`:

| Test | `fn` at |
|---|---|
| `u2a_a_live_staff_naming_the_group_blocks_its_undo` | `:16826` |
| `u2bf_a_the_staff_group_guard_holds_base_free` | `:16983` |
| `u2tomb_a_a_tombstoned_referencing_staff_does_not_block_the_groups_undo` | `:17125` |
| `u3a_minting_the_group_and_its_referencing_staff_in_one_transaction_undoes_whole` | `:17333` |

> **This pin previously identified them ONLY as `:16838`, `:16992`, `:17138`,
> `:17345`** — and those are **interior** line numbers, roughly a dozen lines into each
> body, not the `fn` lines. An interior line number is worse than a stale one: it
> anchors to nothing a reader can search for, and it silently drifts with every edit to
> the function above it. **Names are stable; line numbers are a convenience.** The `fn`
> line numbers above are given second and are already subject to the locator warning
> under pin 0.

They each construct the missing-member form,
which pin 2 now makes an agreeing form. They assert on undo effects rather than
on `members`, so they are expected to survive — **expected, not known.** Each
must be run and its verdict reported; any that changes is a finding.

**Pin 12 — the authority bump. ADDED BY DRAFT AMENDMENT 1; it had no pin, no touch row
and no gate.**

Bump `epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION` **`0` → `1`** (touch row 7),
**and add this rung's entry to the constant's own `Bumps` list** in the same doc
comment. The list is the only record of *why* a version exists; a bump without its
entry leaves a number nobody can account for.

**This is not optional and not a consequence — it is the obligation.** This rung changes
`CreateStaffGroup`'s reduction verdict, and `core_spec.tex` §"Canonical Document
Identity" makes such a change require a new version.

**What guards it, stated precisely — corrected on review, which found this paragraph
contradicting the gate added beside it:**

- **Gate 10 is the direct guard on THIS rung's bump.** It compares the constant's value
  against `HEAD` and requires the `Bumps` entry. A bump omitted *here* is caught.
- **Gates 11a–e are the independent wiring guards.** They confirm the production path
  actually reads the moved authority, using operands that do not descend from it.
- **What remains undetectable is the general case: a FUTURE semantics change whose bump
  is forgotten.** No gate in any contract can catch that — see *Enforcement is not
  detection* under pin 0. The discipline is the guarantee.

> **The sentence that stood here said "nothing in the gate set will catch its absence
> except the two tripwires" — written in the same amendment that added gate 10, which
> catches exactly that.** It imported S27's true statement about *semantics changes in
> general* and applied it to *this rung's bump in particular*, where it is false. **The
> same over-generalisation the enforcement/detection split under pin 0 exists to
> prevent**, committed one section away from it.

> **The bump is stated in pin 0's discharge as requirement 3, which is a discharge note
> and not a pin.** So it had no numbered pin to execute, no touch row to stage, no gate
> to check and no report item to confirm — **the four places this repo's discipline
> requires a mandated change to appear.** It now has all four: pin 12, touch row 7,
> gate 10, report item 2b.

**Pin 10a — whether pin 6 or pin 10 mints a `\label{req:...}` MUST be decided
explicitly, and stated in the report. ADDED BY DRAFT AMENDMENT 1.**

This rung edits **`core_spec.tex`** (row 5, invariant 21's enumeration) and
**`operation_catalog.tex`** (row 4), and `crates/epiphany-testkit/tests/requirement_labels.rs`
counts requirements and labels in **both**. The two readings have different touch tables:

### DECIDED IN REVISION D: NEITHER document mints a label. Row 11 is UNUSED. No counter moves.

**This pin twice said "decide and report", which is not a decision.** It left the staged
set and the counter expectations conditional on a choice execution would make
arbitrarily — and a conditional touch row is a row that can be wrong in either
direction. **The facts settle it, and they were readable while the pin was being
written:**

- **Pin 6 adds invariant 21 to an enumeration that already sits inside ONE requirement
  box.** `core_spec.tex:6529` opens `\begin{requirement}`, `:6530` carries the single
  label **`req:graph:score-graph-invariants`**, `:6533` opens the `enumerate`, and the
  box closes at **`:6648`** — with exactly **20 `\item`s** inside. Item 21 is an `\item`
  **within** that box. **It mints no requirement and no label.**
- **Pin 10 rewrites existing prose** in `operation_catalog.tex` §CreateStaff and
  §CreateStaffGroup, plus a Revision History row and a version bump. **No new
  requirement box, no new label.**

**Therefore:** `CORE_REQUIREMENT_COUNT`, `SUITE_REQUIREMENT_COUNT` and
`SUITE_LABEL_COUNT` all stay **unchanged**; **touch row 11 is unused and MUST NOT be
staged**; and the report states that it was unused for this reason rather than
re-deriving the question.

**If execution finds this wrong** — if either edit turns out to require a new
`\begin{requirement}` — **that is a finding and a contract defect**, reported under §6
item 5, not a decision to be made at the keyboard. The counters would then move per the
table below, which is retained for that case and for the next rung.

| Label minted in | `CORE_REQUIREMENT_COUNT` | `SUITE_REQUIREMENT_COUNT` | `SUITE_LABEL_COUNT` |
|---|---|---|---|
| `core_spec.tex` | **moves** | **moves** | **moves** |
| `operation_catalog.tex` | **unchanged** | **moves** | **moves** |
| both | moves by 1 | moves by 2 | moves by 2 |
| **neither — THIS RUNG** | **—** | **—** | **—** |

> **S27's lesson was "name all three, not one" — for a rung that touched only
> `core_spec.tex`.** Carrying that conclusion across to a rung touching two documents
> turned a correction into a different error, and then into a deferred decision. **A fix
> imported from another contract must be re-derived against this one's facts** — and
> where the facts are readable, re-derived *now*, not delegated to execution as a
> "decide and report".

> **`CLAUDE.md` names this file by name as a recurring escapee**, it escaped the
> format-epoch rung's table, and S27 had to add it mid-execution. **A file that must
> change but is not listed silently drops out of the commit**, and the failure surfaces
> on someone else's branch. **Carrying it as a decided-unused row costs nothing and
> documents the decision** — revision E; it read "carrying it conditionally costs nothing
> if unused" until revision D removed the conditionality.

**Pin 9 — no byte artifact moves.**
`spec/vectors/decode_vectors.txt`, `ops/src/vectors.rs:829`'s literal-byte
vector, and `textproj/src/vectors.rs:198`'s golden are all **unchanged**,
because refusal is at reduction and decode is untouched (§0.2). Confirm, do not
assume. No schema major or minor, no accept-set move, and **no schema/binary-format
companion version bump**. This is narrower than "no version bump": pin 10 *does*
require an Operation Catalog version bump and Revision History row, because the
catalog's normative text about these two operations changes.

**Pin 10 — specification surfaces.**
`operation_catalog.tex` §CreateStaff and §CreateStaffGroup currently state the
disposition-B stale-form semantics normatively (`:1265`–`:1278`, `:1531`–`:1540`)
and must be rewritten to the maintained rule, with a Revision History row and a
version bump. `core_spec.tex`'s `Staff.group` / `StaffGroup.members` docs
(`:5585`–`:5591`, `:4235`–`:4241`) and the two Rust doc comments
(`graph.rs:842`–`:847`, `:1643`–`:1649`) likewise.

**The two Rust doc comments are grep-asserted** by
`graph.rs:2136` and `:2160` (needles at `:2139`, `:2142`, `:2163`, `:2166`,
`:2148`, `:2172`, `:2176`, `:2180`). Those guards must be updated in step, and
the updated needles must assert the *new* rule — a guard left asserting the old
words would fail loudly, but a guard weakened to a substring both rules share
would pass silently, which is the worse outcome.

**Each new needle MUST be wording the disposition-B comments cannot satisfy.**
Retaining only "sole authority" and "non-authoritative" is insufficient: both
phrases are true under B and under A, so a guard built from them alone passes
against the text it is meant to have replaced. Require phrases that are false
under B — for example **"maintained from `Staff.group`"** and **"must agree"** —
so that reverting either comment to its B wording fails the guard. M7 signs
exactly this.

**Pin 11 — the ledger. AMENDED 2026-08-09: the mandated state changed when S27 landed.**

> **This pin required the P13-S16 row to read "blocked on P13-S27".** S27 landed and was
> accepted at `4df8e25`, so that state is now **false**, and a pin mandating a false
> ledger state would put this contract in contradiction with the ledger it governs.
> **As amended, the row must read: UNBLOCKED — S27 landed; this contract is DRAFT and
> needs ratification before dispatch.** Everything else the pin requires recorded is
> unchanged.
>
> The second paragraph's instruction to **file P13-S27 in the same edit** is
> **discharged** — S27 has its own row, its own ratified contract, and its own
> `RESOLVED — IMPLEMENTED 2026-08-09 (pin 10)` marker. It is no longer this rung's to
> file.

`spec/PASS13_CANDIDATES.md`'s P13-S16 row → **UNBLOCKED, S27 landed** *(was: blocked on
P13-S27)*, recording the
`ContainerNotEmpty` reuse (and that a new reason was considered and proved
unnecessary), the base-ingest hazard, the `t8b` inversion, the `t6`/`t7`/`t9`
revisions, and — per pin 0 — that canonical bases materialized before this rung
must be rebuilt rather than reused.

~~**File P13-S27 in the same `spec/PASS13_CANDIDATES.md` edit**~~ — **DISCHARGED
2026-08-09; do not execute.** S27 was filed, contracted, ratified, implemented and
landed at `4df8e25`, and its row carries `RESOLVED — IMPLEMENTED 2026-08-09 (pin 10)`.
The blocking relation this instruction existed to make visible from both ends is now a
*resolved* relation recorded at both ends. **The reasoning below is retained as the
record of why S27 was filed, not as work to do.**

> It is a
> prerequisite discovered by this rung, not independent ledger cleanup, and the
> two rows must land together so the blocking relation is visible from either end.
> Its claim, at the scope §0's inspection supports: the reduction-version
> machinery is **self-referential** — `reduction_version_for` (`bundle.rs:989`)
> sources a new superblock's version from the canonical base's own self-report,
> and `open` (`bundle.rs:396`) checks only that the two agree — so **the current
> implementation has no mechanism comparing either against the semantics it
> actually implements**, and `core_spec.tex:11614`'s rebuild requirement is
> unenforced. Supporting: no constant or accessor names the current semantics, and
> `ids.rs:288`'s claim that the catalog lives in `epiphany-ops` is false — a
> second instance of **P13-S26**'s pattern.
>
> *(**Kept verbatim as the filing that produced S27, not as a description of the tree.
> MOST of it is now false — but NOT all, and an earlier draft of this annotation said
> "every claim" and was itself wrong in the way this contract keeps having to correct.**
> **False now:** the machinery is no longer self-referential — `open` compares the base's
> version against the injected authority, not only against the superblock it seeded;
> `ids.rs:288` was made true by S27's pin 8; and `core_spec.tex:11614` is enforced for
> declared-version mismatches. **STILL TRUE:** *"no mechanism comparing either against
> the semantics it actually implements"* — S27 compares a **declared number** against a
> **declared number**. Nothing anywhere compares either against the semantics the code
> actually implements, and nothing can. See* **Enforcement is not detection** *under pin
> 0.)*

**Do not write the stronger historical claim** ("no reduction-semantics change
has ever been detectable"); §0 does not establish it.

**The P13-S16 row does NOT move to RESOLVED in this edit.** It records the
disposition-A plan, names this contract, and is marked ~~**blocked on P13-S27**~~
**UNBLOCKED — S27 landed `4df8e25`** *(corrected 2026-08-09)*. **"Not RESOLVED" still
holds** and is the part that matters here: this rung has not been implemented, and
unblocked, dispatchable and resolved are three different states.

---

## §2. Touch table

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-ops/src/reduce.rs` | pins 1, 1a, 2, 3, 3a, 4, 4a, 5, 7, 8; and `create_staff_group`'s own doc comment, which states the disposition-B rule |
| 1b | `crates/epiphany-ops/src/payload.rs` | `CreateStaffGroupOp`'s doc (`:1789`ff) states that graph-aware reduction preconditions every carried member resolves to a live `Staff` and that "the mint stores `members` exactly as given and neither maintains nor trusts it" — **both clauses become false** |
| 1c | `crates/epiphany-ops/src/valuegen.rs` | `staff_group`'s doc (`:372`–`:375`). **Precision:** the helper still carries and preserves supplied `members` exactly, and "**never normalizes**" stays true and MUST be preserved — the helper is unchanged. What becomes false is the framing that such a non-empty value is "the value a `CreateStaffGroup` mints" (it can no longer mint one), and the disposition-B attribution. Rewrite those two clauses only |
| 2 | `crates/epiphany-core/src/invariants.rs` | pin 6 |
| 3 | `crates/epiphany-core/src/graph.rs` | pin 10 (two doc comments + their two grep guards) |
| 4 | `spec/operation_catalog.tex` (+ `.pdf`) | pin 10 |
| 5 | `spec/core_spec.tex` (+ `.pdf`) | pins 6, 10 |
| 6 | `spec/PASS13_CANDIDATES.md` | pin 11 |
| **7** | `crates/epiphany-ops/src/lib.rs` | **ADDED by draft amendment 1.** **Pin 12** — bump `CURRENT_REDUCTION_ALGORITHM_VERSION` `0 → 1` **and add its entry to the constant's own `Bumps` list**, which is in the same doc comment. The rung mandated by pin 0's discharge had no touch row, no pin and no gate |
| **8** | `crates/epiphany-core/src/generators.rs` | **ADDED by draft amendment 1 — the root cause below.** `violating_score` (`:498`) matches `GraphInvariant` **exhaustively**, so invariant 21 **does not compile** without a new arm. Four `all()`-driven tests (`:991`, `:1004`, `:1025`, `:1042`) then consume it, so the arm must be a **real generator**, not a stub |
| **9** | `crates/epiphany-testkit/src/roundtrip.rs` | **ADDED by draft amendment 1.** S27 test 10b (`:894`) reopens a literal-`0` base under `production_caps()`; pin 12's bump makes that path return `Err` and hit an arm that **`panic!`s by design** |
| **10** | `crates/epiphany-textproj/src/serialize.rs` | **ADDED by draft amendment 1.** S27 test 10a (`:659`) asserts the production writer supplies `ReductionAlgorithmVersion(0)`. Its own doc (`:655`) says it **is expected to fail when S16 bumps** and that updating it *is* S16 stating the authority moved |
| **11** | `crates/epiphany-testkit/tests/requirement_labels.rs` | **UNUSED — DECIDED in revision D, no longer conditional.** Pin 10a establishes that **neither** pin 6 nor pin 10 mints a `\label{req:...}`: invariant 21 becomes an `\item` inside the existing `req:graph:score-graph-invariants` box (`core_spec.tex:6529`–`:6648`), and pin 10 rewrites prose. **No counter moves; this file MUST NOT be staged**, and the report says so citing pin 10a. **The row is retained rather than deleted** because `CLAUDE.md` names this file as a recurring escapee — a row reading *"deliberately unused, and why"* survives review, while an absent row looks like an oversight. *(Read as "CONDITIONAL — decide and report" until revision D, and as "all three counters move" until revision A.)* |

Regenerate the two PDFs **only after** their sources reach final form.

### Rows 9 and 10 are S27's tripwires firing as designed, not collateral damage

S27 wrote two assertions against the **literal** `0` precisely so they would move when
the authority moved, and `serialize.rs:655` says so in as many words. **Their failure is
this rung's signal that the bump took effect** — the only signal it gets, since no
mechanism can detect a semantics change. They must be **updated, not deleted or
`#[ignore]`d**, and the report must quote both before and after.

> **Neither file was in any touch row, and both fail at gate 1.** That is the exact
> shape of S27's own `gminor.rs` failure: a file the rung must change that no surface
> count reached, found by a gate rather than by the table. **The allowlist catching it
> is the allowlist working** — but only if the row exists before execution starts.

### Row 8 is the rung's real shape change: an enum extension is a TWO-crate change

**This contract treated invariant 21 as local to `invariants.rs`.** It is not.
`GraphInvariant` has an **exhaustive generated-consumer surface in `epiphany-core`**:

- `violating_score` (`generators.rs:498`) matches every variant — **adding one is a
  compile error until its arm exists.**
- Four `all()`-driven tests then call it and `shrink` (`:991`, `:1004`, `:1025`,
  `:1042`), so **a `todo!()` or trivial arm fails them.** Invariant 21 needs a fixture
  that violates the **S→G direction — a staff whose `group` names a group whose
  `members` omit it — and NOT the G→S direction**, and survives shrinking.
  **The direction is PINNED here, in revision C.**
- **The direction needs its own permanent, NAMED test — REVISION D, name pinned in
  REVISION E.** **No existing `all()`-driven test can observe direction.**
  `negative_generators_are_reasonably_targeted` (`:1037`) collects
  `kinds: BTreeSet<GraphInvariant>` and allows `kinds.len() <= 3`, but **both directions
  of invariant 21 are the same `GraphInvariant` variant**, so they collapse to one
  element and the bound is blind to the distinction; the other three loops assert only
  `!is_empty()`.

  Add to `generators.rs`'s test module, named **exactly**:

  ```
  invariant_21_negative_generator_breaks_staff_to_group_only
  ```

  It MUST assert **the same three properties twice — before and after shrinking. THE
  SHRUNK LEG IS ADDED IN REVISION F.** On `violating_score(StaffGroupMembershipAgreement,
  seed)` **and again on `shrink(&that, StaffGroupMembershipAgreement)`**:

  **(i)** `check_invariants` returns **exactly one** violation and it is
  `StaffGroupMembershipAgreement`; **(ii)** that violation's witness names the **S→G**
  staff and group ids; **(iii)** the **G→S** direction is satisfied, asserted directly.

  > **The shrink leg was required by row 8 and observed by nothing.** The raw fixture was
  > checked by this test; the shrunk one only by
  > `every_invariant_shrinks_to_a_small_witness` (`:1003`), which asserts
  > `!check_invariant(&small, inv).is_empty()`. **That is membership in a single
  > `GraphInvariant` variant**, so a shrunk witness that flipped to **G→S-only** passes
  > it — both directions are the same variant — and because it calls `check_invariant`
  > (singular) rather than `check_invariants`, **a shrunk witness that gained an
  > unrelated second defect passes too.**
  >
  > **`shrink` is a transformation, so its output needs the same guarantees as its
  > input.** Requiring a fixture to "survive shrinking" without asserting *what survives*
  > only establishes that something still fires.

  > **Revision D required this test and gave it no name — so nothing consumed it.**
  > Gate 6 named only `m41`/`m41b`, and §6 item 2d asks for shrink evidence. **Omitting
  > the test entirely would still compile, satisfy all four `all()` loops, and pass every
  > named gate.** An unnamed obligation has no consumer, which is revision D's own
  > closing lesson applied to the requirement revision D wrote.

  > **"Both directions" was wrong here and incompatible with M6 — corrected in revision
  > B.** `violating_score` returns **one** `Score` per variant, so it cannot carry two
  > fixtures; and a fixture disagreeing in both directions is **still reported after
  > either M6 arm is deleted**, which is precisely the evidence failure M6's own
  > isolation rule forbids.
  >
  > **Revision B then said "one named direction" and left WHICH to execution — a design
  > decision disguised as a reporting requirement.** Either choice changes the generated
  > witness and the shrink evidence, so *reporting* it afterward does not substitute for
  > *specifying* it. **Pinned above to S→G**, because that is the smallest corruption of
  > `valid_score` (drop the staff id from `group.members`, leave `staff.group` intact),
  > matching the doctrine every other arm follows, **and it is the exact shape this
  > rung's own failure produces** — pin 2's append not firing, which is what M2 observes.
  >
  > **The division of labour, stated once:** row 8's generator covers **S→G** and must
  > survive `shrink`; **pin 6a owns two permanent direction-isolated tests**, one per
  > direction; **M6 breaks those two tests, one each.** Three fixtures, three purposes,
  > none standing in for another.
- **`shrink` (`:932`) takes `GraphInvariant` but does NOT match on it** — it calls
  `check_invariant(score, inv)` and `shrink_candidates`, so it is **generic over the
  invariant and needs no new arm.** *(Draft amendment 1 first said this "MUST be checked
  at execution", deferring a **static fact readable from the function body**. Corrected
  on review: a draft that can decide something must decide it, or it exports its own
  unfinished reading as execution work.)*
- **What the EXISTING `shrink` tests impose is weak, and revision F is why the named test
  carries a shrunk leg.** `generators.rs:1025`–`:1026` runs
  `shrink(&violating_score(inv, 7), inv)` for every variant, and `shrink` asserts on
  entry that its input violates the target; `:1003` then checks the shrunk score with
  `!check_invariant(&small, inv).is_empty()`. **Together these establish only that
  *something* still fires** — a fixture whose violation depends on incidental structure
  `shrink_candidates` removes fails there, which is real but is the *weakest* of the
  three properties. **Direction and exactness after shrinking are guaranteed only by the
  named test's shrunk leg.**

**Consequence for the mutation plan and gate 6:** invariant 21's generator is itself
load-bearing, so M6 (deleting each arm) now has a second signature — the negative
generator must still produce a violating score for the arm under test.

---

## §3. Mutation plan

Applied, **run**, output recorded verbatim, restored **by hand-editing back**.

> ### A FAILING TEST IS NOT AN OBSERVATION — draft amendment 1
>
> **M1, M2 and M4 accepted "the named test fails" as their whole signature.** That
> signs nothing: a test fails for the mutation, for a typo, for an unrelated panic, or
> because the harness changed — and a **compile error is not a test failure at all.**
> **The evidence a mutation owes is the BEHAVIOUR it changed, not the assertion it
> broke.** Each mutation below must now name the wrong verdict, value or state the
> mutated build produces, and the report must quote it. *(This is S27's round-15/16
> lesson, which cost four defects there; M3, M5, M6 and M9 in this contract already
> met the standard, so the fix is uneven-by-design rather than uniform.)*

**M1 — the refusal fires. OBSERVATION TIGHTENED, draft amendment 1.** Remove pin 1's
emptiness check. **Required observation:** a `CreateStaffGroup` carrying a **non-empty
`members`** is now **applied instead of refused** — report the resulting
`OperationEffect` and the minted `StaffGroup.members` value, showing the spurious
membership that reached the graph. The rewritten `t8b` spurious-order assertion failing
is the *symptom*; the applied mint is the observation.

**M2 — the maintenance fires. OBSERVATION TIGHTENED, draft amendment 1.** Remove pin 2's
append. **Required observation:** after a `CreateStaff` naming a live group,
`StaffGroup.members` is **still empty** — quote it, and quote invariant 21's verdict on
that state. The rewritten `t8b` missing-order assertion failing is the symptom.

**M3 — the re-carry stays idempotent.** Make pin 3 write maintained members into
`staff_group_values`; a byte-identical re-carry must degrade from
`AlreadyApplied` to `RecreateContentMismatch`. **This mutation must be observed,
not reasoned about** — it is the sub-pin disposition A named.

**M4 — the base-ingest hazard is real. OBSERVATION TIGHTENED, draft amendment 1.**
Restore `:1619` to `group.clone()`. **Required observation: the re-carry misverdict
itself** — name what an ingested base's `CreateStaffGroup` re-carry now returns and what
it should have returned, in the shape M3 already uses (`AlreadyApplied` →
`RecreateContentMismatch`). "Pin 4a's test must fail" was the entire signature and does
not distinguish the hazard from any other breakage.
**If the misverdict does NOT appear, pin 4 is unmotivated and that is a finding** —
report it rather than keeping a guard nothing needs.

**M3 and M4 are signed by pins 3a and 4a, not by manual demonstration.** Run each
mutation against its named test so the guard, not the transcript, is what
survives the rung.

**M8 — the dead loop is actually gone.** Reinstate the member-liveness loop in
`create_staff_group` **after** pin 1's refusal, where it cannot fire. Every
behavioural assertion in this contract must still pass, and the pin-1 structural
gate must fail. This is the only signature available for a deletion that no
behaviour observes.

**M9 — `t7`'s inversion is real, not assumed.** With pin 1 in place, run the
`CreateStaffGroup` arm of `t7` base-free and confirm it refuses; then graph-gate
pin 1's refusal behind `self.graph.is_some()` and confirm that arm reverts to
applying. Signs that the empty-container precondition is deliberately
universe-independent.

**M5 — the undo hole is closed.** Remove pin 5's strip; undoing a `CreateStaff`
must leave a disagreeing pair that invariant 21 flags.

**M6 — invariant 21 sees both directions. FIXTURES MUST ISOLATE, tightened on review.**
Delete each arm in turn; each deletion must leave a distinct disagreeing fixture
**unreported**.

**M6 deletes pin 6b's two NAMED surfaces and breaks pin 6a's two named tests, one each —
bound in revision C, surface pinned in REVISION G:**

- **M6a** — delete `idx.check_staff_names_absent_group(&mut v);` from `check_invariants`
  → `m41_check_invariants_dispatches_invariant_21_staff_names_absent_group` must fail,
  **and `m41b` must still pass.**
- **M6b** — delete `idx.check_group_lists_unowned_staff(&mut v);`
  → `m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff` must fail,
  **and `m41` must still pass.**

**Each deletion is a single named call site**, which is what makes the two independently
removable. *(M6 previously said "delete each arm in turn" while pin 6 required only
behaviour — so against a single-shared-check implementation there were no two arms and
**M6 was unexecutable**. Pin 6b fixes that at the source rather than restating the
mutation.)*

**The surviving test passing is half the observation**, and the half that proves the
arms are independent rather than one arm catching everything.

> **Each fixture must violate ONE direction only**, and the report must show that it
> satisfies the other. A fixture disagreeing in **both** directions still gets reported
> after either arm is deleted — the surviving arm catches it — so the invariant looks
> intact and **the deletion is signed by nothing.**
>
> **Revision C made this permanent rather than mutation-only.** M6 previously named no
> tests, so it demonstrated both directions *while mutated* and left the restored suite
> free of durable coverage for either. Pin 6a's tests are the coverage; M6 is now their
> signature. **The generator's fixture (row 8, S→G) is a third artifact and stands in
> for neither** — its `all()`-driven consumers only ask *"is 21 reported?"*.

**M7 — the doc guards discriminate. SPLIT IN TWO by draft amendment 1.** Pin 10 covers
**two independently guarded doc blocks** — `Staff.group` and `StaffGroup.members`, each
with its own grep guard (touch row 3). Reverting *"one doc comment"* leaves the other
guard untested, and a run that reverts the stronger one **passes while the weaker guard
is still weak.** So:

- **M7a — `Staff.group`.** Revert that block to its disposition-B wording. **Required
  observation:** quote the guard's own output showing **its specific needle no longer
  matches**, not merely that a test failed.
- **M7b — `StaffGroup.members`.** The same, independently, with its own output.

**Both must be run and both outputs reported.** A guard that passes against both
wordings is weakened, not updated — and with one mutation covering two guards, that
weakening is invisible.

---

## §4. Gate

1. `cargo test --workspace` — full pass. **Baseline is 1577 passed / 0 failed / 0
   ignored across 42 suites** *(the post-S27 figure; `CLAUDE.md`'s Green baseline is the
   single origin — if it disagrees with this line, `CLAUDE.md` wins and that is a
   finding)*. **The count will move** (t8b renamed, new invariant tests, and rows 9–10's
   tripwire updates). Give the delta in buckets — net-new, converted-from-existing,
   tripwire-updated — and **if they do not sum to the observed delta, that is a finding,
   not an arithmetic error to be papered over.**
   **Report the ignored count, which MUST be 0** — a "full pass" is satisfied by an
   `#[ignore]`d test that never runs.
2. `cargo +1.95.0 clippy --workspace --all-targets -- -D warnings` → clean.
   **The toolchain is part of the gate**: CI pins **1.95.0**, this machine's default
   `stable` is **1.97.1**, and the repo's CI comment records that *1.97 rejects a bare
   `2.0` that 1.95 accepts*. **A clippy result that does not say which toolchain
   produced it says nothing.** Report the toolchain with the result.
3. `cargo +1.95.0 fmt -p epiphany-ops -p epiphany-core -p epiphany-textproj -p
   epiphany-testkit --check` → clean, on the same pinned toolchain.
   **All four changed crates — corrected by draft amendment 1**, which added rows 9 and
   10 in `epiphany-testkit` and `epiphany-textproj`; the command named only two and
   would have left both unformatted while reporting clean.
   **`cargo fmt --all` is forbidden** — it reaches `spikes/` through path dependencies.
4. `git diff --cached --check` → clean after staging. **The staged list is a SUBSET of
   §2, not an equality — corrected by draft amendment 1.** "Exactly §2" is
   **unsatisfiable** here because **row 11 is deliberately unused** (pin 10a decided in
   revision D that no label is minted): read literally, "exactly §2" fails whenever a row
   is correctly unstaged, or invites staging an unchanged file to satisfy it.
   *(This said "row 11 is conditional" until revision E — true when written, and revision
   D made the row **decided-unused** rather than conditional. **The subset rule is
   unaffected**; only its rationale needed the current term.)* **Instead:** every staged path must appear in §2, **and** every §2 row must be
   either staged or **named in the report as unused, with its reason.** Neither
   direction may be silent. *(This is S27's round-17 correction; S16 carried the
   formulation S27 had already found unsatisfiable.)*
   > **Scope note, not a separate gate — demoted in revision C.** §2 carries no
   > prohibitions, unlike S27's §2, so gate 4 is the whole staging check here. **If a
   > later amendment adds an absence rule, gate 4 does not cover it**: "appears in §2"
   > and "is not forbidden by §2" are different questions. *(This was numbered `4a.`,
   > which produced no command and no output — so it could not be "a gate result" — and
   > collided with **§4a**, the landing-obligation section. Demoted here in revision C,
   > so **no gate carries an `a` suffix** and `§4a` names only the landing obligation.
   > **The gate count that stood in this note until revision J is gone, not
   > corrected.**)*
5. `spec/vectors/decode_vectors.txt` **unmodified** (pin 9). Confirm by `git
   status`, not by inspection.
6. Invariant 21 is reached **through `check_invariants`** on a score violating
   only it, in the shape of `m40_check_invariants_dispatches_invariant_20`
   (`invariants.rs:6045`) — **for BOTH directions, by pin 6a's two named tests**:
   `m41_check_invariants_dispatches_invariant_21_staff_names_absent_group` **and**
   `m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff`. **Both run,
   both verdicts reported, and each confirmed to satisfy the direction it does not
   break.** *(Revision C: this asked for one score, which left one branch with no durable
   coverage once M6 was reverted.)*
   **And `invariant_21_negative_generator_breaks_staff_to_group_only`** (touch row 8),
   which is the only consumer of the generator's pinned direction. **Three tests, all
   run, all verdicts reported — revision E**; revision D named two and left the third
   with no gate.
   **The generator test's evidence covers BOTH its legs — revision F:** the raw fixture
   **and the shrunk one**, each with the exact-set, witness-direction and
   opposite-direction assertions. **A gate that accepts only the raw leg leaves `shrink`
   free to change what the fixture proves.**

   **EVIDENCE MODEL — CHOSEN EXPLICITLY IN REVISION E. Quote the SOURCE assertions plus
   the pass verdict; do NOT claim a runtime return.**

   Revision D said *"quote `check_invariants`' full return … and its witness ids"*, which
   **the prescribed tests cannot emit**: they are `assert!`-style tests in `m40`'s shape,
   and `cargo test` prints `ok` for a passing test, not local values. A report obeying
   that literally would need either **unpinned `--nocapture` instrumentation added purely
   to produce it**, or inference from source dressed up as observed output. **Neither is
   evidence.**

   So for each of the three tests, the report gives:
   1. **The exact-set assertion, quoted verbatim from source** — the `len() == 1` and
      invariant-identity assertions, and the witness-id assertion.
   2. **The test's pass verdict** from `cargo test`.

   **A passing exact-set assertion IS the observation**; the assertion text says what was
   checked and the verdict says it held. *(This follows S27's gate 6c, which quotes a
   struct definition rather than grepping for it: **a quoted source construct is read,
   not inferred**. Adding print instrumentation to satisfy a report would be new,
   unpinned code in `epiphany-core` written for no other purpose.)*

   *(Revision D added the exactness requirement because this gate checked the target
   verdict and the opposite direction but **not the absence of invariants 1–20**, so a
   fixture carrying an unrelated second defect passed every stated check.)*
   `all().len() == 21` and the `core_spec.tex` enumeration ending at 21 are checked
   **in addition**, never instead.
7. **Every test named in pin 8** runs, each verdict reported. *(Read "the four pin-8
   tests" until revision B — a third live tally, left standing while the mutation and
   gate tallies beside it were removed. Pin 8's table is the origin.)*
8. The pin-1 structural gate: `create_staff_group`'s production body contains
   the empty-members refusal and no member-liveness/`TargetMissing` path.
   **METHOD PINNED IN REVISION H, BOUNDARY CORRECTED IN REVISION I.**

   **Quote exactly pin 1's slice: `create_staff_group`'s body, brace-matched from its
   `fn` line to its closing brace, production source only** — and read it. Report the
   refusal's lines and state that no `TypedObjectId::Staff` liveness check and no
   `TargetMissing` construction remain **within that slice**.

   > **Revision H said "to the `#[cfg(test)]` boundary", which made this gate
   > self-defeating.** `create_staff_group` begins at `reduce.rs:4458`;
   > **`create_part_definition` begins at `:4515`** and carries its own
   > `PreconditionFailureReason::TargetMissing` at **`:4554`**; the next `#[cfg(test)]`
   > is at **`:9576`**. So the literal read spans five thousand lines and **always
   > contains the very path the gate says must be absent**, while any shorter read
   > violates the stated boundary. **The gate could not be passed and could not be
   > honestly failed.**
   >
   > **Pin 1 had the boundary right all along** (`slice the create_staff_group body,
   > brace-matched from its fn line to its closing brace`). Revision H invented a second,
   > looser one instead of citing it — **the same "imported a plausible-sounding rule
   > rather than re-deriving from the pin" failure revision A recorded**, applied to a
   > boundary instead of a conclusion. **Where a pin already defines the artifact, the
   > gate cites the pin; it does not redescribe it.**

   **Do not establish the absence by grep**: a `TargetMissing` path can be spelled
   without either literal, so **a grep for absence proves only that a chosen string is
   gone** — S27's gate-6a lesson, and the reason its gate 6c quotes a definition rather
   than searching for it. **This is also what M8 signs**, so a vacuous gate 8 makes M8's
   deletion unobserved.
9. `t8c` (pin 3a) and `t8d` (pin 4a) both present and passing, by name.
10. **Pin 12's bump landed, by value comparison — ADDED BY DRAFT AMENDMENT 1.**
    ```
    grep -n "pub const CURRENT_REDUCTION_ALGORITHM_VERSION" crates/epiphany-ops/src/lib.rs
    git show HEAD:crates/epiphany-ops/src/lib.rs | grep -n "pub const CURRENT_REDUCTION_ALGORITHM_VERSION"
    ```
    → working tree **`= 1`**, `HEAD` **`= 0`**. **Report both outputs, not the
    conclusion**, and **quote the new `Bumps` list entry verbatim.** A bump without its
    entry fails this gate.
11. **Rows 9 and 10's tripwires were UPDATED, not silenced, and remain INDEPENDENT of the
    authority. STRENGTHENED ON REVIEW — the first version permitted the exact tautology
    it exists to prevent.**

    It said only *"updated, not silenced … not weakened to accept any version"*, which
    **does not forbid replacing the literals with `CURRENT_REDUCTION_ALGORITHM_VERSION`.**
    That reads as the tidiest possible update and is the one thing that must not happen:
    both operands would then move together with every future bump, the comparison would
    pass for all values, and **M5a and M5b become vacuous — §0.1's tautology rebuilt
    inside the tests written to detect it.** S27's round 3 caught this exact substitution
    and `roundtrip.rs:882` says *"do not tidy either literal into the constant"*.

    **Required, and each quoted verbatim in the report:**

    a. `serialize.rs:663` asserts against an **independent literal `ReductionAlgorithmVersion(1)`** — never the constant, never a value derived from it.
    b. `roundtrip.rs:901` fixture capability → `synthetic_for_fixture(1)`, and `:918`'s staged base → **literal `ReductionAlgorithmVersion(1)`**, so the reopen under `production_caps()` matches and the `Ok` arm still runs.
    c. `roundtrip.rs:941`'s success-arm assertion → **literal `1`**.
    d. **`roundtrip.rs:947`'s mutation-only `Err` arm — `assert_eq!(base, ReductionAlgorithmVersion(0))` → literal `1`.** *Omitted from the first version of this gate.* Left at `0`, **M5b fails at the wrong assertion**: the arm is reached only under mutation, and it would abort on the base comparison before reaching the two-field `panic!` that is M5b's required observation. The mutation would appear to fail correctly while observing nothing.
    e. The **literal-preservation doc comments** (`roundtrip.rs:872`–`:883`, `serialize.rs:648`–`:657`) updated to name `1`, **with their "do not tidy into the constant" reasoning intact.** The reasoning is what stops the next rung making the substitution this gate forbids.

    **None of a–e may be deleted or `#[ignore]`d.** A tripwire that accepts both values,
    or that derives either operand from the authority, is not a weakened guard — it is no
    guard at all.
12. **Pin 6b's two direction checks both exist and are both dispatched — STRUCTURAL,
    ADDED IN REVISION G.** M6 is unexecutable without them, so this gate proves the
    mutation surface exists **before** M6 is attempted rather than discovering its
    absence mid-run.

    **FOUR INDEPENDENT CHECKS, each required to be EXACTLY ONE. REWRITTEN IN REVISION H
    — the aggregate count it replaced could pass with one surface missing.**

    ```
    grep -c "fn check_staff_names_absent_group"          crates/epiphany-core/src/invariants.rs   # a → 1
    grep -c "fn check_group_lists_unowned_staff"         crates/epiphany-core/src/invariants.rs   # b → 1
    grep -c "idx.check_staff_names_absent_group(&mut v)" crates/epiphany-core/src/invariants.rs   # c → 1
    grep -c "idx.check_group_lists_unowned_staff(&mut v)" crates/epiphany-core/src/invariants.rs  # d → 1
    ```

    **Report all four counts separately. Any count other than exactly `1` is a pin 6b
    violation and a finding** — including a count **greater** than 1, which means a
    duplicated definition or a doubled dispatch.

    **And quote the context, because a count is not a mapping:**

    - **each definition with its enclosing `impl GraphIndex<'_>` header**, proving the
      method belongs to the type `check_invariants` builds — not a free function, not a
      method on some other type that merely shares the name;
    - **each dispatch with the `pub fn check_invariants` header above it**, proving the
      call is in the dispatcher M6 will edit — not in a test, a helper, or a second
      dispatcher.

    > **What the previous version permitted, stated so it is not reintroduced.** It ran
    > two alternation greps and required *"four lines total"*. **Both definitions can
    > exist (2 lines) while `check_invariants` calls `check_staff_names_absent_group`
    > **twice** and `check_group_lists_unowned_staff` **never** (2 lines) — four lines,
    > gate passes, and M6b has no call site to delete.** The aggregate proved a
    > population, never a pairing. **Where a gate must establish a MAPPING, it cannot
    > count** — it has to check each element on its own, which is the same shape as this
    > contract's rule against enumerating where completeness is required.
    >
    > **A grep for presence is also defeated by a rename**, so pin 6b's names are pinned
    > and this gate and M6 both use them. If the implementation chose other names, **that
    > is the finding** — the gate has not "passed with zero matches", it has failed.

---

## §4a. Landing obligation — files that must NOT be staged, and must be fixed after

**ADDED BY DRAFT AMENDMENT 1.** Pin 12's bump makes two live statements false, and both
are in documents **outside this rung's touch table on purpose**:

| File | Statement the bump falsifies |
|---|---|
| `CLAUDE.md:106` | *"`CURRENT_REDUCTION_ALGORITHM_VERSION` — **currently `0`**"*, and *"P13-S16 is the first rung that must move it to `1`"* |
| `spec/HANDOFF_2026-08-07.md:25` | the POST-S27 block's *"currently `0`"* and the same "first rung" sentence |

**DO NOT STAGE EITHER DURING EXECUTION.** Both describe the state of the repository
**after a rung has landed**, and this rung is not landed while it is staged and awaiting
acceptance. Staging them would have the contract assert S16 had landed at the moment it
was submitted for review — **a claim about the future written as present fact**, which is
the defect class S27's amendments 4 through 7 were spent removing.

**They are a POST-ACCEPTANCE obligation.** Once the owner accepts and commits this rung,
reconcile both in a separate commit, together with anything else the bump falsifies.
**The report MUST list them as outstanding**, so acceptance is not mistaken for
completion.

> **Pin 11 already covers `spec/PASS13_CANDIDATES.md`, which is different** and *is*
> staged: a ledger row records what a rung *did*, and is written by the rung. `CLAUDE.md`
> and the handoff state what is *true now*, and only become true at acceptance. **The
> distinction is whether the file's claim is dated or live** — the rule amendment 6
> settled for S27, applied here to decide staging rather than wording.

---

## §5. Staging and boundary

Stage only §2's files, by explicit path. **Never `git add -A`.**

**A concurrent session commits here.** Re-check `HEAD` before staging and before
commit. **Never** `git reset`, `git restore --staged`, `git checkout`, or `git
stash`.

**Out of bounds — MUST NOT be read, written, or staged:** the entire `spikes/`
tree, `spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_*.md`,
`spec/ANALYSIS_GENESIS_PERSISTENCE.md`, `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`,
`spec/DRAFT_T4_FIXTURE_RECIPE.md`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`, `crates/epiphany-editor-gui/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the root `Cargo.toml`,
`.claude/worktrees/`.

**Ratified contracts MUST NOT be edited**, including
`spec/CONTRACT_GENESIS_G3A_ENTITIES.md` and `…_G3A_UNDO_REPAIR.md`.

**Do not repair `core_spec.tex`'s invariant-10 prose** — that is P13-S26, and
its evidence at `invariants.rs:69`–`:71` must stay intact.

**The executing agent MUST NOT commit.** Leave the work staged.

---

## §6. Report requirements

1. **Every mutation listed in §3**, each with its verbatim output and, where §3 names
   one, the **behaviour** it observed rather than the assertion it broke.
   **The count is NOT stated here — corrected on review.** It read *"the nine mutations
   (M1–M9)"*, and M7's split into **M7a and M7b** makes ten executions, so a report could
   not both enumerate them and obey the tally. **§3 is the single origin**; a count here
   goes stale the next time a mutation splits, exactly as this one did.
2. **Every gate listed in §4**, each with its command and output. **No count is stated
   here — §4 is the single origin.** Where a gate has lettered subchecks, they are
   reported **under that gate**, not as separate results.
   *(This item read "the nine gate results", then carried a corrected count, and the
   corrected count went stale the moment a gate was added. **Revision J removed it rather
   than updating it again** — restating a count beside the words "no count is stated
   here" is the defect naming itself.)*
2b. **Pin 12's bump and its `Bumps` entry**, with gate 10's two outputs; **and rows 9
   and 10's tripwire updates**, each quoted before and after, with gate 11's confirmation
   that neither was silenced. **ADDED BY DRAFT AMENDMENT 1.**
2c. **Confirmation that neither pin 6 nor pin 10 minted a `\label{req:...}`** — as pin
   10a decided in revision D — that **touch row 11 was not staged**, and that all three
   counters are **unchanged**. **If either edit did mint one, that is a FINDING against
   this contract** (§6 item 5), reported with pin 10a's table applied; it is not a
   decision to be taken during execution.
   *(This item read "decide and report" and, before that, "all three counters and their
   new values" — the rule pin 10a had just corrected, surviving in its own report
   consumer. **Third site of one false claim**: the pin, touch row 11, and here.)*
2d. **Invariant 21's negative fixture survives `shrink` with its properties intact** —
   evidenced by `invariant_21_negative_generator_breaks_staff_to_group_only`'s **shrunk
   leg**, per gate 6's model: **the shrunk-leg assertions quoted from source, plus the
   test's pass verdict.**
   **REWRITTEN IN REVISION F — it said "quote the shrunk witness", which the test cannot
   emit.** That is the same unproduced-runtime-evidence defect revision E fixed in gate 6
   **and did not carry one hop to this item**; and "still violates 21" was the weak
   membership check that let a shrunk witness flip direction or gain a second defect.
   *(Earlier still, this item asked whether `shrink` matches `GraphInvariant`
   exhaustively — a **static fact the draft could read**: it does not, it calls
   `check_invariant(score, inv)`.)*
2e. **The three invariant-21 tests' verdicts** — `m41`, `m41b`, and
   `invariant_21_negative_generator_breaks_staff_to_group_only` — **each with its
   exact-set and direction assertions quoted from source**, per gate 6's evidence model,
   **and for the generator test BOTH legs, raw and shrunk (revision F).**
   **No runtime return or witness dump is claimed**; a passing exact-set assertion is the
   observation. **ADDED IN REVISION E**, which found the generator test required by
   revision D but consumed by nothing.
2a. **REWRITTEN 2026-08-09 — it required the opposite of what is now correct.** It read:
   *"For pin 0: confirmation that **nothing** was added claiming to reject or detect
   stale canonical bases, and that the break is recorded only in prose."* That was right
   while nothing rejected them. **S27 now does**, on both the read and write paths, so a
   report obeying the old text would confirm the absence of a guarantee that exists.
   **As rewritten, the report must state:**
   - that **`CURRENT_REDUCTION_ALGORITHM_VERSION` was bumped to `1`**, with the bump's
     entry added to the constant's own "Bumps" list — this rung changes
     `CreateStaffGroup`'s reduction verdict, and **nothing can detect a missed bump**;
   - that **no second detection path was added.** S27 owns the check and its tests; a
     rung that re-implements the guarantee it depends on has built a duplicate that can
     disagree with the original;
   - that the rebuild break is recorded in `operation_catalog.tex`'s Revision History,
     **which is unchanged from the original requirement.**
3. The staged file list, and the test-count delta with its cause.
4. **The pin-8 verdicts** — one per test named there, count not restated — and the
   `t6`/`t7`/`t9` revisions with what each
   asserted before and after.
5. Anything contradicting this contract. A contract defect reported is worth
   more than a contract satisfied.
