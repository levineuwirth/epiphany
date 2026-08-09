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

**This rung's first act is bumping the authority to `1`**, because it changes
`CreateStaffGroup`'s reduction verdict. That bump is the discipline S27 installed: no
mechanism can detect a semantics change, so the bump is the entire guarantee.

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

> **Line-number citations in this contract predate S27 and have NOT been re-derived.**
> S27 changed `bundle.rs` by 795 lines, so `bundle.rs:989`, `:396` and `ids.rs:288`
> above — and every other `bundle.rs` reference in this document — are stale as
> locators even where the claim about them is historical. **Re-deriving them is part of
> ratification**, not something this supersession did.

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
| `t6` | `reduce.rs:16154` | `CreateStaffGroup.members` naming a non-live target is `TargetMissing`, one of three referential loops asserted separately | that arm becomes `ContainerNotEmpty` and stops testing a *referential* loop at all; the `CreatePartDefinition.staves` and `CreateView.active_layers` arms are untouched and must stay |
| `t7` | `reduce.rs:16229` | those same preconditions are **not** enforced base-free | pin 1's refusal is **not** graph-gated — it is a property of the carried value — so this arm now refuses base-free too, inverting the claim for that arm only |
| `t9` | `reduce.rs:16454` | a from-empty score passes `check_invariants`, and **each skipped reducer check independently makes invariant 10 fire**, on a fixture that already attempts a dangling reference in each of the four ops | the dangling-member fixture can no longer enter the graph, so both the fixture and t9's own mutation set change |

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

**Pin 8 — the four G3a undo-repair tests are re-verified, not assumed.**
`:16838`, `:16992`, `:17138`, `:17345` each construct the missing-member form,
which pin 2 now makes an agreeing form. They assert on undo effects rather than
on `members`, so they are expected to survive — **expected, not known.** Each
must be run and its verdict reported; any that changes is a finding.

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

Regenerate the two PDFs **only after** their sources reach final form.

---

## §3. Mutation plan

Applied, **run**, output recorded verbatim, restored **by hand-editing back**.

**M1 — the refusal fires.** Remove pin 1's emptiness check; the rewritten `t8b`
spurious-order assertion must fail.

**M2 — the maintenance fires.** Remove pin 2's append; the rewritten `t8b`
missing-order assertion must fail.

**M3 — the re-carry stays idempotent.** Make pin 3 write maintained members into
`staff_group_values`; a byte-identical re-carry must degrade from
`AlreadyApplied` to `RecreateContentMismatch`. **This mutation must be observed,
not reasoned about** — it is the sub-pin disposition A named.

**M4 — the base-ingest hazard is real.** Restore `:1619` to `group.clone()`;
pin 4a's test must fail. **If it does NOT fail, pin 4 is unmotivated and that is
a finding** — report it rather than keeping a guard nothing needs.

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

**M6 — invariant 21 sees both directions.** Delete each arm in turn; each
deletion must leave a distinct disagreeing fixture unreported.

**M7 — the doc guards discriminate.** With pin 10's needles updated, revert one
doc comment to its disposition-B wording; the guard must fail. A guard that
passes against both wordings is weakened, not updated.

---

## §4. Gate

1. `cargo test --workspace` — full pass. **The count will move** (t8b renamed,
   new invariant tests). Report the new total and the delta with its cause.
2. `cargo clippy --workspace --all-targets -- -D warnings` → clean.
3. `cargo fmt -p epiphany-ops -p epiphany-core --check` → clean.
   **`cargo fmt --all` is forbidden.**
4. `git diff --cached --check` → clean, after staging; confirm the staged list
   is exactly §2.
5. `spec/vectors/decode_vectors.txt` **unmodified** (pin 9). Confirm by `git
   status`, not by inspection.
6. Invariant 21 is reached **through `check_invariants`** on a score violating
   only it, in the shape of `m40_check_invariants_dispatches_invariant_20`
   (`invariants.rs:6045`). `all().len() == 21` and the `core_spec.tex`
   enumeration ending at 21 are checked **in addition**, never instead.
7. The four pin-8 tests each run, each verdict reported.
8. The pin-1 structural gate: `create_staff_group`'s production body contains
   the empty-members refusal and no member-liveness/`TargetMissing` path.
9. `t8c` (pin 3a) and `t8d` (pin 4a) both present and passing, by name.

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

1. The nine mutations (M1–M9), each with verbatim failure output.
2. The nine gate results, each with its command.
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
4. The four pin-8 verdicts, and the `t6`/`t7`/`t9` revisions with what each
   asserted before and after.
5. Anything contradicting this contract. A contract defect reported is worth
   more than a contract satisfied.
