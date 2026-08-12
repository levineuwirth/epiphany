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

## §6. Gate 6 — pin 9's boundary check, verbatim

*(pending)*

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
