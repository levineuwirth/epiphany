# Evidence — P13-S29 execution

**Not part of the candidate's normative content.** The destination gates 6 and 7
require: every mutation transcript and boundary-gate output, recorded verbatim
rather than summarised.

Contract ratified at `cea21cd`. Executed 2026-08-12.

---

## §1. Pin 3's count movement, measured

Pin 3 requires the requirement to be added **first**, the counts measured, then
the temporary allowlist row removed. Observed on the first run after the
requirement landed and before any constant was touched:

```
thread 'requirement_labels_follow_the_grammar' panicked at
crates/epiphany-testkit/tests/requirement_labels.rs:299:5:
assertion `left == right` failed
  left: 287
 right: 286
```

`CORE_REQUIREMENT_COUNT` 215 → **216**; `SUITE_REQUIREMENT_COUNT` 286 → **287**;
`SUITE_LABEL_COUNT` 286 → **287**.

**The citation gate is green for the correct reason**, which gate 12 requires
distinguishing from green-because-still-allowlisted:

- the temporary `req:time:tempo-segment-shape` row is **absent** from
  `DISCUSSED_NOT_CITED`, whose surviving rows are exactly
  `req:layoutir:vertical-bands` and `req:graph:aleatoric-reference-locality`;
- `\label{req:time:tempo-segment-shape}` is **present** in `spec/core_spec.tex`,
  so the label is *defined*, not excused.

---

## §2. Pin 11's migration, and what failed before it

After the emission split and before pin 11's migration, the workspace stood at
**1582 passed / 4 failed**, and the four were exactly the migrated positives pin
11 names:

```
invariants::accidental_compatibility_tests::cmn_chromatic_accidental_in_edo_31_fires
invariants::review_fix_tests_4::f3_aleatoric_bounds_key_absent_and_reversed_window_fire
invariants::review_fix_tests_4::f3_aleatoric_dag_referencing_absent_event_fires
invariants::review_fix_tests_4::f4_tempo_segment_structural_defects_fire
```

**The two accidental negatives did not fail** — they stayed green and vacuous,
exactly as pin 11b predicts, which is why that pin replaces their *predicates*
rather than only their selector. A migration repairing only the loud failures
would have left them passing for a reason unrelated to what they test.

All nine observations were migrated: five in `review_fix_tests_4`, four in
`accidental_compatibility_tests`. The ghost-region anchor assertion in
`f4_tempo_segment_structural_defects_fire` and the `EventCoordinateModel`
assertion in `f3_aleatoric_bounds_key_absent_and_reversed_window_fire` were
**left as invariant assertions**, per pin 11a.

---

## §3. Five execution faults, recorded rather than left in the transcript

None is a contract defect; none reached a commit. All are recorded because they
were self-inflicted by *how* the edits were made, because the first is a repeat
of one this rung's family has already paid for, and because the fifth is the
only one no tool would have reported.

### 3.1 A span replacement deleted `struct DeferredCheck`

Rewriting the `Display` impl by splicing from
`impl core::fmt::Display for WellFormednessViolation` to the **next** `impl`
swallowed the `pub struct DeferredCheck` declaration, which sat between the two.
Observed as:

```
error[E0432]: unresolved imports `invariants::DeferredCheck`, `invariants::InvariantViolation`
error[E0425]: cannot find type `DeferredCheck` in this scope
```

**Restored** with its fields and doc comment intact, including the note that
§0.5 excludes it from this rung deliberately.

**This is the second occurrence of the same defect in this rung's family.** The
first removed mutation **M18** from the P13-S26 contract's §3 during revision M,
also by replacing a span whose endpoints straddled an unrelated neighbour. A
span replacement is silent when it takes something extra: nothing fails, the
text simply no longer exists.

### 3.2 Line-number targeting edited the wrong module

Six assertions were rewritten by line number after a grep. Two of them —
reported at `4243` and `4261` — lay in **`review_fix_tests_3`**, not
`review_fix_tests_4`, which begins at `4266`. Both were reverted to their
original `CrossCuttingRefsResolve` form, and the genuine C9 site was located
symbolically at `f3_aleatoric_bounds_key_absent_and_reversed_window_fire`.

**The rule the contracts already state for locators applies to edits too.**
P13-S16 §7 and P13-S26 §0.6 require symbolic anchors because line numbers drift;
these edits drifted *within a single session*, between one grep and the
replacement that consumed it. **All remaining edits in this execution use
symbolic anchors.**

### 3.3 Two fixture faults, corrected by reading rather than guessing

Neither is a contract defect; both were invented shapes that the tree disproves.

**A C1 fixture assumed `valid_score` carries spanners.** It pushed a ghost staff
onto `cross_cutting.spanners[0]` and panicked: *"index out of bounds: the len is
0 but the index is 0"*. The generator produces no spanners. The fixture now uses
a **dangling staff instrument** — the same `check_cross_cutting_refs` surface,
and stable across generator changes in a way an index into an optional
collection is not.

**An `AleatoricTimeModel` literal named a field that does not exist**
(`discipline`), failing to compile. Rather than guess the shape, C8 and C9 were
rebuilt on the pattern
`f3_aleatoric_dag_referencing_absent_event_fires` already proves: locate the
aleatoric region by `matches!` on its time model, take its instance-0 voice-0
events, and install an `EventOrderingDAG` or a `bounds` key naming a ghost.

*Both faults share the shape of §3.1 and §3.2: a construct written from
expectation instead of from the tree. The difference is that the compiler and a
panic caught these two immediately, where a span replacement and a stale line
number are silent.*

### 3.4 Two source helpers, deliberately

`production_source()` now exists twice: privately inside `g3a_tests`, and again
inside `s29_violation_kind_tests`. **This is scoping, not duplication drift.**
Pin 1b requires its derives guard to live in `g3a_tests` **because** that
module's helper is private to it; pin 10's tests are a different module and
cannot reach it. Widening the original's visibility was declined — it would add
surface to satisfy a placement the contract already settled.

### 3.5 Shared kind bindings made a pinned mutation inapplicable

`check_tempo_maps` was first written with three shared bindings —

```rust
let anchor = ViolationKind::Invariant(GraphInvariant::CrossCuttingRefsResolve);
let shape = ViolationKind::Requirement("req:time:tempo-segment-shape");
let order = ViolationKind::Requirement("req:time:tempo-segment-order");
```

— passed to the `flag` closure at all six emission sites. **§3's M1 requires
"seven separate mutations, one per requirement condition", and §3's column
header requires each mutation be *applicable*.** Under shared bindings it is
not: re-tagging `shape` moves C4 **and** C5 together, and `order` moves C6 and
C7 together. The first two runs made that concrete — a single edit to `shape`
failed four tests, the union of C4's and C5's radii.

**The six kinds are now written inline at their emission sites and the three
bindings are gone.** Behaviour is identical — `cargo test --workspace` returned
the same `44 suites / 1604 passed / 0 failed / 0 ignored` before and after — and
each condition is now independently re-taggable, which is what M1 asks for.

*This is an execution fault of the same family as §3.1–§3.4: code written from
what reads well rather than from what the contract has to be able to do to it.
It is the only one of the five that no compiler, panic or assertion would ever
have caught — the mutation simply would have measured something coarser than the
cell it was compared against, and matched a union that looked like a radius.*

### 3.6 Restoration and baseline

After both faults were repaired and pin 11's migration completed:

```
suites=43 passed=1586 failed=0 ignored=0
```

The rung's structural baseline, unchanged from the ratified input, because
nothing in pins 1–11 adds or removes a test. The new tests of pins 1b and 10
move it; that movement is recorded in §4 when they land.

---

## §4. Structural baseline after the new tests

```
suites=44 passed=1604 failed=0 ignored=0
cargo +1.95.0 clippy --workspace --all-targets -- -D warnings: clean
cargo +1.95.0 fmt -p epiphany-core -p epiphany-testkit --check: clean
core_spec.pdf rebuilt, 0 undefined references
```

**1586 → 1604, and 43 → 44 suites.** Eighteen net-new tests: pin 10's sixteen,
pin 1b's derives guard, and pin 1b's integration test — the last being the new
suite, since `tests/public_surface.rs` is a new integration target.

**Every mutation radius below is measured against this surface**, not against
the ratified input's.

### 4.1 Pin 1b's guard found a real defect on its first run

`violation_types_declare_their_pinned_derives` failed immediately:

```
assertion `left == right` failed: ViolationKind's rustdoc is pinned
  left: "A violation of a graph invariant: which invariant, and a short witness naming
         the smallest offending objects (Chapter 5; QUICKSTART: …). What a
         [`WellFormednessViolation`] failed. `Invariant` names a numbered …"
 right: "What a [`WellFormednessViolation`] failed. `Invariant` names a numbered …"
```

**The old `InvariantViolation` rustdoc had survived**, stranded above
`ViolationKind` when the enum was inserted before the struct. Nothing else in the
suite could see it: it is prose, and every other assertion is behavioural. It was
removed.

*This is the case pin 1b was written for — round 15 called the derives and
rustdocs "prose that no test or gate reads", and the first thing the new guard
read was a stale one.*

---

## §5. Expected-versus-observed matrix

**The phase is halted at M1.** Five of the seven M1 conditions matched their §3
cells exactly; **M1·C6 and M1·C7 did not**, and the difference is a defect in
§3, not in the tree. §7 states the finding; the remaining mutations (M2 onward)
are **not run** and this section stays open until the amendment lands.

Every row below was run as `cargo test --workspace --no-fail-fast` against the
§4 surface (`44 / 1604 / 0 / 0`), with a `cargo build --tests --workspace`
compile check first — **a mutation that does not compile observed nothing** —
and restored by hand write-back.

| M | §3 cell | Observed | |
|---|---|---|---|
| M1·C4 | 2 | 2 | ✅ |
| M1·C5 | 3 | 3 | ✅ |
| M1·C6 | 1 | **2** | ❌ |
| M1·C7 | 1 | **2** | ❌ |
| M1·C8 | 4 | 4 | ✅ |
| M1·C9 | 2 | 2 | ✅ |
| M1·C10 | 3 | 3 | ✅ |

### 5.1 M1·C4 — `tempo_constant_mismatch_reports_shape`

Cell: that test **plus** `f4_tempo_segment_structural_defects_fire`. Observed
`1602 passed / 2 failed`, exactly those two.

### 5.2 M1·C5 — `tempo_nonconstant_missing_end_reports_shape`

Cell: that test, `f4_tempo_segment_structural_defects_fire`, and
`requirement_selector_discriminates_its_payload` — pin 10 builds the requirement
discriminator from C5 and C8. Observed `1601 passed / 3 failed`, exactly those
three.

### 5.3 M1·C6 — MISMATCH

**Cell: `tempo_out_of_order_reports_order` alone. Observed two.**

```
passed=1602 failed=2
    invariants::s29_violation_kind_tests::tempo_out_of_order_reports_order
    invariants::s29_violation_kind_tests::tempo_overlap_reports_order

---- invariants::s29_violation_kind_tests::tempo_out_of_order_reports_order stdout ----
thread '...tempo_out_of_order_reports_order' panicked at
crates/epiphany-core/src/invariants.rs:6677:9:
aggregate must carry (Requirement("req:time:tempo-segment-order"), "tempo segments are
out of start order"); got [(Invariant(CrossCuttingRefsResolve), "tempo segments are out
of start order"), (Requirement("req:time:tempo-segment-order"), "tempo segments overlap
in musical time")]

---- invariants::s29_violation_kind_tests::tempo_overlap_reports_order stdout ----
thread '...tempo_overlap_reports_order' panicked at
crates/epiphany-core/src/invariants.rs:6681:9:
the rider must not answer to invariant 10; got [WellFormednessViolation { kind:
Invariant(CrossCuttingRefsResolve), witness: "tempo segments are out of start order" }]
```

**The second failure is pin 10's own fourth assertion, on the sibling test.**

### 5.4 M1·C7 — MISMATCH, symmetrically

**Cell: `tempo_overlap_reports_order` alone. Observed the same two, with the
roles exchanged.**

```
passed=1602 failed=2
    invariants::s29_violation_kind_tests::tempo_out_of_order_reports_order
    invariants::s29_violation_kind_tests::tempo_overlap_reports_order

---- invariants::s29_violation_kind_tests::tempo_out_of_order_reports_order stdout ----
thread '...tempo_out_of_order_reports_order' panicked at
crates/epiphany-core/src/invariants.rs:6681:9:
the rider must not answer to invariant 10; got [WellFormednessViolation { kind:
Invariant(CrossCuttingRefsResolve), witness: "tempo segments overlap in musical time" }]

---- invariants::s29_violation_kind_tests::tempo_overlap_reports_order stdout ----
thread '...tempo_overlap_reports_order' panicked at
crates/epiphany-core/src/invariants.rs:6677:9:
aggregate must carry (Requirement("req:time:tempo-segment-order"), "tempo segments
overlap in musical time"); got [(Requirement("req:time:tempo-segment-order"), "tempo
segments are out of start order"), (Invariant(CrossCuttingRefsResolve), "tempo segments
overlap in musical time")]
```

### 5.5 M1·C8 — `aleatoric_ordering_outside_region_reports_locality`

Cell: that test, `f3_aleatoric_dag_referencing_absent_event_fires`,
`mixed_fixture_splits_by_arm` (C8 is in the pinned mixed fixture), and
`requirement_selector_discriminates_its_payload`. Observed `1600 / 4`, exactly
those four.

```
thread '...f3_aleatoric_dag_referencing_absent_event_fires' panicked at invariants.rs:4325:9:
assertion failed: fires_req(&s, "req:time:aleatoric-reference-locality")

thread '...mixed_fixture_splits_by_arm' panicked at invariants.rs:7040:9:
aggregate must carry the rider as its requirement; got [ ... kind: Invariant(
CrossCuttingRefsResolve), witness: "tempo segment start anchor target ... dangling" },
... kind: Invariant(CrossCuttingRefsResolve), witness: "aleatoric region ... ordering
references event ..., absent from the region" }]
```

### 5.6 M1·C9 — `aleatoric_bounds_outside_region_reports_locality`

Cell: that test plus `f3_aleatoric_bounds_key_absent_and_reversed_window_fire`.
Observed `1602 / 2`, exactly those two.

### 5.7 M1·C10 — `accidental_incompatible_reports_tuning_requirement`

Cell: that test, the migrated `cmn_chromatic_accidental_in_edo_31_fires`, and
`display_renders_each_arm_exactly`, whose requirement side uses a real
accidental violation. Observed `1601 / 3`, exactly those three.

### 5.8 What the five matching rows establish about the two that did not

**The sibling-fixture hazard is specific to C6/C7 and measurably absent
elsewhere**, which is why the correction must not be generalized into a blanket
rule about shared labels:

- **C4/C5 share the label `req:time:tempo-segment-shape`** and did **not**
  cross-talk: C4's fixture is `Constant` with a mismatched `end_tempo`, C5's is
  non-constant with none, and neither shape can emit the other's condition.
- **C8/C9 share `req:time:aleatoric-reference-locality`** and did **not**
  cross-talk: each aggregate in the transcripts above carries exactly one
  violation, so the fixtures are genuinely disjoint.
- **C6/C7 share `req:time:tempo-segment-order` and cannot be made disjoint** —
  pin 10 says so and rules out the fixture that would separate them.

**Restoration after every row**, verified by full run:

```
suites=44 passed=1604 failed=0 ignored=0
```

---

## §6. Gate 6 — the identifier and field migration, verbatim

**6a. No `InvariantViolation` identifier in Rust code.** Gate 6 scopes this to
`crates/**/*.rs`; the contract, the ledger and this annex quote the old name
historically and are out of scope.

```
$ find crates -name '*.rs' -type f -exec grep -Hn 'InvariantViolation' {} +
$ echo $?
1
```

*Run with `find … -exec`, not a piped `grep | head`: a universal negative from a
truncated pipe is the failure mode `CLAUDE.md` names, and this gate is a
universal negative.*

**6b. No `.invariant` field access on a `WellFormednessViolation`.** Three hits
survive, and gate 6 requires each to be attributed rather than counted:

```
crates/epiphany-core/src/invariants.rs:320:            self.invariant.number(),
crates/epiphany-core/src/invariants.rs:321:            self.invariant,
crates/epiphany-core/src/invariants.rs:3326:        assert_eq!(deferred[0].invariant, GraphInvariant::RegionExtents);
```

**All three are `DeferredCheck`, not the violation type.** Lines 320–321 are
inside `impl core::fmt::Display for DeferredCheck` (opened at `:315`); line 3326
indexes the result of `deferred_checks(&s)`.

**6c. `DeferredCheck.invariant` retained**, as §0.5 requires:

```
crates/epiphany-core/src/invariants.rs:305:pub struct DeferredCheck {
crates/epiphany-core/src/invariants.rs-306-    /// The invariant whose decision was deferred.
crates/epiphany-core/src/invariants.rs-310-    pub invariant: GraphInvariant,
```

---

## §7. Finding against §3: M1·C6 and M1·C7's radii

**Reported, not patched.** The pins are frozen; this needs an amendment with its
own review round, and the mutation phase stays halted until one lands.

### 7.1 What §3 says

> **C6 and C7 fail their pin-10 test alone**: the legacy `f4` assertion is one
> fixture tripping *both*, so re-tagging one leaves the other still reporting the
> same label and the legacy assertion still passes

### 7.2 Why it is wrong

**The quoted sentence is true about `f4`, and `f4` is indeed not in either
radius** — measurement confirms it. The error is that the derivation *stopped at
the legacy observer*. It never asked the same question of pin 10's own C6 and C7
tests, which share the fixture **shape**.

Pin 10 pins **four** assertions per C4–C10 row. The fourth is:

> the aggregate's **invariant arm is empty**

and pin 10 states the fixture relationship outright:

> C6's natural fixture — `seg(2,3)` then `seg(1,2)` — is *also* overlapping, so
> it emits C6 **and** C7 under the same label.

Both tests are built by `two_segments(seed, (2, 3), (1, 2))`, differing only in
the seed. So re-tagging **either** condition puts an `Invariant` violation into
**both** fixtures, and the sibling's fourth assertion fails. The radius is two,
not one, in both directions.

### 7.3 The tree is correct; the cell is not

**No implementation change can reconcile them, and none should be attempted.**
The only fixture that isolates C6 needs `seg1.end < seg1.start` — an
end-before-start segment — and pin 10 **explicitly rejects it**:

> *The alternative to pairs — an isolated C6 fixture — would need an
> end-before-start segment, which is unnatural and tests a shape the checker
> should never see.*

Pin 10 chose shared fixtures deliberately and §3 then costed them as though it
had not. **Changing the fixtures to fit §3 would violate pin 10**; changing the
fourth assertion would discard the property §3's own M2a derivation leans on.

### 7.4 The defect class

**The one-hop correction** — the ledger's recurring shape, and named as such in
this rung's own review history. A fact is established in one place (pin 10: this
fixture emits both conditions) and not carried into the place that depends on it
(§3: what fails when one of them moves). §3 even *cites* the shared-fixture
property, in the same cell, about a different observer.

It is also the fifth instance of the pattern §3's preamble already documents:

> Every one of the four differed from what static reading predicted

Four cells were measured because they reached tests the contract did not write.
**C6 and C7 reach tests the contract *did* write — which is exactly why they
were derived instead of measured, and exactly why the derivation was trusted.**
Authorship of a test is not knowledge of its fixture's reach.

### 7.5 The measured replacement

| M | Radius |
|---|---|
| M1·C6 | `tempo_out_of_order_reports_order` **and** `tempo_overlap_reports_order` |
| M1·C7 | the same two |

Measured, not derived; transcripts in §5.3 and §5.4.

**This strengthens M1 rather than weakening it.** Under the pinned cell a
re-tagging that broke only the sibling would have read as a radius mismatch;
under the corrected cell each of C6 and C7 is observed by two independent
assertions — its own pair assertion and the sibling's invariant-arm assertion.

---

## §8. Amendment 1: resolution and resumption

Amendment 1 is ratified at `29ef3af` and landed by this commit. It
corrects two radius cells in the contract's §3 and changes no pin, test, fixture
or behaviour.

**§5 and §7 above are the dated execution record and are not edited.** They
state what was expected, what was observed, and why execution stopped. The
mismatch they record is the reason this amendment exists; reconciling them would
remove it.

### 8.1 The corrected radii

| M | Dispatched cell (§5) | Corrected cell, measured |
|---|---|---|
| M1·C6 | `tempo_out_of_order_reports_order` | that test **and** `tempo_overlap_reports_order` |
| M1·C7 | `tempo_overlap_reports_order` | that test **and** `tempo_out_of_order_reports_order` |

Both were observed before the halt. The transcripts in §5.3 and §5.4 stand as
the observation and **were not re-run**: the amendment corrects the expectation
they were compared against, not the observation.

### 8.2 Resumption

The mutation sequence resumes at **M2**. M1 is complete — C4, C5, C8, C9 and C10
matched their dispatched cells, and C6 and C7 match the corrected cells above.

---

## §9. The mutation phase, resumed at M2

**§5 is closed at M1 by the digest gate (contract §6.5c-bis); this section
continues the matrix.** Every row was run as
`cargo test --workspace --no-fail-fast`, preceded by
`cargo build --tests --workspace` checked for `error[` — **a mutation that does
not compile observed nothing** — and restored by hand write-back.

### 9.1 Five more execution faults, none a contract defect

**They are recorded here rather than appended to §3** because §3 is the
pre-amendment record committed at `29ef3af` as amendment 1's oracle. Its count of
five is true as of that commit; these four are later events.

#### 9.1.1 Pin 10's eleventh row was never built

Pin 10's matrix has **eleven** rows. The eleventh is marked `—` rather than a
C-number:

| — | reversed aleatoric bounds | **invariant 4, unchanged** | `reversed_aleatoric_bounds_stay_invariant_four` |

It was read as a note and no test was written. **M11's cell names that test**, so
the omission surfaced as a radius mismatch — expected 4, observed 2 — and not
before.

*A row whose identifier is a dash reads as commentary. The other ten rows carry
C-numbers; the eleventh carries the same three columns and a dash, and only the
columns matter.*

#### 9.1.2 `display_renders_each_arm_exactly`'s invariant side was the wrong fixture

Its comment read:

```rust
// Invariant side: a reversed aleatoric bound (invariant 4), acquired
// from the aggregate -- pinned, because acquisition decides M3's radius.
```

The code beneath it built a **dangling staff instrument** and searched for
`CrossCuttingRefsResolve` — invariant 10, a C1 fixture. **The comment recorded
the pin; the code did something else.**

Pin 10 forecloses exactly this:

> *The choice changes radii, so it cannot be the executor's: a C1 fixture would
> put this test in M17·C1's cell, a tempo-anchor fixture in M17·C2/C3's, and
> reversed bounds put it in **M11's**, which is where it now belongs and where
> M11's cell names it.*

Both halves of that prediction were then observed: with the fixture repaired,
`display_renders_each_arm_exactly` **entered M11's radius** (§9.3) and is
**absent from M17·C1's measured 20** (§9.4). Under the unrepaired fixture it
would have failed M17·C1 too — a second mismatch the first one pre-empted.

*This is the most dangerous of the five fault classes in this rung: a comment
that states the pin correctly next to code that does not. Nothing reads the
comment.*

#### 9.1.3 Pin 3a's guard did not exist

`tempo_segment_shape_requirement_states_its_clauses_and_stays_s8_neutral` was
absent from the tree. **Thirteen mutations target it** — M12pre, M12, M13,
M13post, M13neutral, M14a–e, M15a, M15b — so thirteen cells had no possible
observer. Found when the `.tex` batch went to look for it.

It is now implemented as pin 3a requires: the `.tex` slice from the
`\begin{requirement}` preceding the label to the first `\end{requirement}` at or
after it, whitespace-collapsed, **equal** to pin 3's pinned source.

**M14e is the vindication of equality over a stem inventory.** It appends
`A \texttt{Constant} segment's \texttt{end\_tempo} \MUST{} be absent.` — no
`canonic`, no `normaliz`, no `prefer`, and it settles P13-S8 inside a requirement
minted not to. Equality fails it; every phrase-and-stem form passes it.

#### 9.1.4 An interrupted run left a mutation applied

The mutation harness restores in a `finally`. A **killed** process runs no
`finally`, so an interrupted batch leaves the tree mutated. One did: M2a's
wildcard survived at `invariants.rs:425`.

It was found by re-reading the file, and `invariant_selector_discriminates_its_payload`
confirmed it:

```
invariant 10's selector must return only its own; got [
  WellFormednessViolation { kind: Invariant(CrossCuttingRefsResolve), witness: "staff ...
  WellFormednessViolation { kind: Invariant(EventCoordinateModel), witness: "aleatoric ...
```

**Restored by hand-editing**, never git, and the full suite returned to
`44 / 1606 / 0 / 0`.

*The lesson is not "be careful with interrupts": it is that a restore guaranteed
only by process exit is not a guarantee. **Verify the baseline after any
interrupted mutation run, before trusting the next measurement** — a stray
mutation makes every subsequent radius wrong in a way that looks like a
mismatch in the wrong place.*

### 9.2 The surface moved twice, and what that obliges

| Surface | Cause |
|---|---|
| 1604 | §4's surface — pin 10's 16 tests, pin 1b's two |
| **1605** | §9.1.1's missing test added |
| **1606** | §9.1.3's missing guard added |

**Radii are measured against the final surface.** The 17 mutations first measured
at 1604 were **re-run in full** at 1605 and all matched. The 38 measured at 1605
were **not** re-run in full at 1606; the justification is bounded and stated:

- **Pin 3a's guard has exactly two inputs** — `core_spec.tex` via `include_str!`
  and a string literal. Verified mechanically: no `production_source`, no
  `check_*` call, one `include_str!` target.
- **None of those 38 modifies `core_spec.tex`.** They edit `invariants.rs`,
  `generators.rs` or `public_surface.rs`.
- **Two representatives were re-run at 1606 anyway**, one per interaction class:
  **M2a** — the widest selector radius, 21 — and **M20b**, a prose edit inside
  `invariants.rs`. Both matched unchanged.

*This is an argument from a test's complete input set, not from reasoning about
fixture reach — the thing this rung has repeatedly got wrong. It is recorded as
an argument, not presented as a measurement.*

### 9.3 Expected versus observed — all 54 mutations

| M | Cell | Observed | |
|---|---|---|---|
| M1·C4 | 2 | 2 | ✅ |
| M1·C5 | 3 | 3 | ✅ |
| M1·C6 | 2 *(amendment 1)* | 2 | ✅ |
| M1·C7 | 2 *(amendment 1)* | 2 | ✅ |
| M1·C8 | 4 | 4 | ✅ |
| M1·C9 | 2 | 2 | ✅ |
| M1·C10 | 3 | 3 | ✅ |
| M2 | 8 | 8 | ✅ |
| M2a | 21 *(measured pre-ratification)* | 21 | ✅ |
| M3 | 13 | 13 | ✅ |
| M3a | 1 | 1 | ✅ |
| M4 | 1 | 1 | ✅ |
| M5 | 1 | 1 | ✅ |
| M6 | 1 | 1 | ✅ |
| M7 | 7 | 7 | ✅ |
| M7a | 1 | 1 | ✅ |
| M7b | 1 | 1 | ✅ |
| M7c | 1 | 1 | ✅ |
| M7d | 1 | 1 | ✅ |
| M8 | 5 | 5 | ✅ |
| M9 | 14 | 14 | ✅ |
| M10 | 3 | 3 | ✅ |
| M11 | 4 | 4 | ✅ |
| M12pre | 1 | 1 | ✅ |
| M12 | 1 | 1 | ✅ |
| M13 | 1 | 1 | ✅ |
| M13post | 1 | 1 | ✅ |
| M13neutral | 1 | 1 | ✅ |
| M14a–M14e | 1 each | 1 each | ✅ |
| M15a | 1 | 1 | ✅ |
| M15b | 1 | 1 | ✅ |
| M16a | 1 | 1 | ✅ |
| M16b | 1 | 1 | ✅ |
| M17·C1 | 20 *(18 measured + 2 new)* | 20 | ✅ |
| M17·C2 | 3 *(measured)* | 3 | ✅ |
| M17·C3 | 1 *(measured)* | 1 | ✅ |
| M18 | 1 | 1 | ✅ |
| M20 | 1 | 1 | ✅ |
| M20a–M20k | 1 each | 1 each | ✅ |

**No compile-only result. No passing-outcome mutation** — §3 requires every row
to fail, and every row did.

### 9.4 Three cells worth their own note

**M2a, 21, unchanged from its pre-ratification measurement.** §3 warned its proxy
over-approximated for any test observing a rider through the selector, and that
none of the 20 legacy tests was a rider test. Confirmed: the legacy 20 are exactly
the `g3b_measure20_tests` set measured, plus the new discriminator.

**M17·C1, 20, and `display_renders_each_arm_exactly` is not among them.** That
absence is the receipt for §9.1.2: under the unrepaired C1 fixture it would have
been.

**M7b, 1.** §3 records that revision L wrongly named
`every_invariant_has_a_negative_generator` here, since that test iterates `all()`
and cannot detect an omission from `all()`. Observed: `graph_invariant_all_is_unchanged`
alone.

### 9.5 Restoration

After every row, and after the interrupted-run repair:

```
suites=44 passed=1606 failed=0 ignored=0
cargo +1.95.0 clippy --workspace --all-targets -- -D warnings: clean
```

**1586 → 1606, twenty net-new tests**, and 43 → 44 suites:

| Count | Where |
|---|---|
| 17 | pin 10's eleven-row matrix and its whole-surface tests |
| 1 | pin 3a's `.tex` prose guard |
| 1 | pin 1b's derives guard, in `g3a_tests` |
| 1 | pin 1b's integration test — the new suite |

---

## §10. A sixth fault, found by gate 16 at the last moment

**Pin 9's `/// 10.` rider note was never migrated.** It is one row of pin 9's
table, and it was the only row left undone — the other eight were verified
individually rather than assumed, which is how this one surfaced.

The note still read:

```rust
///     Beyond that surface, further checks are reported under this same tag
///     and are NOT part of the normative invariant 10: tempo-map segment
///     shape, ordering and non-overlap (Chapter 3,
///     `req:time:tempo-segment-order`); ...
///     multiplexing is filed as P13-S29 — the public `check_invariant`
///     filter and this violation's `Display` attribute those failures to
///     invariant 10. Repairing it is a behaviour change, out of scope here.
```

**Three statements, all false as of this rung**, and the gate names all three:
the riders are no longer *"reported under this same tag"*; P13-S29 is no longer
their *pending owner* — it is this commit; and the note listed **three** labels
where pin 9 requires **four**, `req:time:tempo-segment-shape` being the one this
rung minted.

*The missing fourth label is the same defect shape as §9.1.1: a set that grew by
one, and a list that did not.* It is now rewritten to name all four and to say
that `check_invariants` still returns them, so a caller asking *"is this graph
well-formed"* keeps its coverage.

**Why no test caught it.** Pin 9's prose outcomes have **no machine observer** —
gate 16 says so outright: *"Every one of these can be omitted with all other
gates green."* Eight rows had landed; the ninth had not; nothing in 1606 tests
could tell the difference.

**Every other pin 9 row was re-verified by its own stale phrase**, not by
assumption:

| Stale phrase | Found in |
|---|---|
| `surfaced here under invariant 10` | clean |
| `go under invariant 10` | clean |
| `surfaced under an existing` | clean |
| `the compatibility invariant` | clean |
| `tempo-map segment invariants` | clean |
| `all 19 enumerated graph invariants` | clean |
| `reported under this same tag` | **`invariants.rs`** → repaired |
| `filed as P13-S29` | **`invariants.rs`** → repaired |

*`core_spec.tex:3120` still occurs in `DECISIONS.md` and `accidental.rs`.
Neither is a pin 9 row: pin 9 pins that locator's replacement in the **accidental
header comment** in `invariants.rs`, which is clean, and it explicitly does
**not** rewrite `DECISIONS.md`, which gains a supersession note instead.*

---

## §11. Gate results, 1–16

| # | Gate | Result |
|---|---|---|
| 1 | `cargo test --workspace` | **44 suites / 1606 passed / 0 failed / 0 ignored** |
| 2 | clippy `-D warnings` | clean, 0 warnings and 0 errors |
| 3 | `fmt -p epiphany-core -p epiphany-testkit --check` | clean (never `--all`) |
| 4 | staged paths ⊆ §2 rows, all rows staged | 13 paths, 13 rows |
| 5 | `git diff --cached --check` | clean |
| 6 | identifier and field migration | §6 above, verbatim |
| 7 | every §3 mutation observed | §9.3 — 54 rows, all matched |
| 8 | `all()` re-derived at 21; `.tex` count claim unchanged | 21 entries, 21 unique, 21 `number()` arms 1..21, order identical; `core_spec.tex:6746` still reads *"exactly \textbf{21} invariants"* and is absent from the diff |
| 9 | `latexmk -xelatex core_spec` | undefined references cleared on pass 1; `core_spec.pdf` rebuilt |
| 10 | ledger append, removed-plus-added reconstruction | 1 removed / 1 added; `added` ends with `\|`; `strip(added) == strip(removed) + " " + APPEND` **true** |
| 11 | temporary allowlist row absent, two survivors present | `req:time:tempo-segment-shape` absent; `req:layoutir:vertical-bands` and `req:graph:aleatoric-reference-locality` present |
| 12 | `requirement_labels` passes with the row absent | 6 passed, 0 failed |
| 13 | pin 11's inventory | **25 observations: 9 migrated (5 + 4), 16 unchanged**, none deleted, none softened; M16a and M16b prove both negatives non-vacuous |
| 14 | pin 12's lifecycle | status block exactly `STATUS: LANDED by this commit.`, no hash; frozen-pins statement shows **0 hunks** in a zero-context staged diff; revisions A–R marked a dated historical record |
| 15 | placement and Revision History row | shape follows order's `\end{requirement}` with only a `\begin{requirement}` between; the added run equals pin 13's block, whitespace-collapsed |
| 16 | pin 9's prose outcomes | §10 — eight rows verified clean by their own stale phrase, one repaired |

### 11.1 Gate 10's one-character finding

The staged ledger append first read `RESOLVED 2026-08-12`; pin 13's `APPEND` is
pinned verbatim as `2026-08-11`. The reconstruction failed on that character
alone, and **the artifact was corrected to the pin, not the pin to the artifact.**

*Flagged for the owner rather than silently reconciled: the contract's own
ratification line reads 2026-08-12, so the pinned append carries the date the
row was drafted rather than the date the rung resolved. Changing it is an
administrative amendment to pin 13, not an execution decision — gate 10 exists
to make exactly this deviation visible.*
