# Contract — P13-S29: the violation tag stops multiplexing

STATUS: RATIFIED; DISPATCHED.

**Ratified 2026-08-12 on the authority of the repository owner**, the final
whole-artifact review returning zero findings. Review-round records accumulate
above §0.

**THE PINS ARE FROZEN. They may be executed, not edited.** A defect found during
execution is **reported, not patched in place** — if it needs a pin change, that
is its own amendment with its own review round.

Owning candidate: **P13-S29**, filed by
`spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md` pin 7.

Rung class: **behaviour change.** A public type is renamed, a public field
changes type, a targeted selector deliberately returns less than it does today,
a public `Display` changes for one arm, and a normative requirement is minted.

---

### REVISION R — review round 18. Four blockers, one smaller, two cleanups. ALL ACCEPTED.

1. **[P1] Assertion 3's slice was ambiguous and destructive.** The module header
   is a **28-line `//!` run** with breaks at `:2`, `:9`, `:13`, `:22`; asserting
   the whole run equals a four-line paragraph would have **deleted 24 lines of
   retained header**. **Fixed** — insertion and boundaries are pinned. *Placed
   **after** the title paragraph rather than prepended: prepending would make its
   first sentence the module's rustdoc summary line, displacing "The Chapter 5
   graph invariants".*
2. **[P1] Assertions 4 and 5 did not have the boundaries they claimed** — the
   rustdocs are not "immediately preceding" their declarations; the **derive
   attribute sits between**. **Fixed** — a three-step algorithm: declaration, its
   derive line, then the `///` block above *that*.
3. **[P1] Pin 1a and the pinned prose disagreed.** Pin 1a promises all three
   blocks state the two-arm split *and* that a requirement failure is not an
   invariant failure; the struct rustdoc stated neither and the enum rustdoc only
   the first. **Fixed by strengthening the blocks**, so pin 1a's claim is true of
   each **individually**, not merely of the three collectively.
4. **[P1] M20h, M20j and M20k were not applicable mutations** — "append a
   sentence" names no sentence. **Fixed** — each pins its exact added line, as
   M20f, M20g and M20i already did.
5. **[P2] Revision Q corrected M20f's rationale but not the two live statements
   it contradicts.** **Fixed** — both now say a call whose result is **asserted**
   would widen M3/M9's radii; an ignored one would not.

**Two cleanups landed with them:**

- **`invariants.rs:8` carries `` [`InvariantViolation`] `` in retained header
  prose.** Gate 6 forbids that token in Rust code, so leaving it unpinned made
  **gate 6 unsatisfiable**. Pin 9 now owns its rename.
- **"Every assertion has an ADDITIVE discriminator" was false** — M20i is a
  **reordering**. Now "non-deletion".

*The round also noted my tally of 11 M20-rows was 12; the contract states no such
count, which is why it did not stale.*

---

### REVISION Q — review round 17. Four blockers, one smaller. ALL ACCEPTED.

1. **[P1] M20f did not compile and did not demonstrate its stated radius.** The
   file imports only the three pinned names, and `valid_score` lives at
   `epiphany_core::generators::valid_score` (`lib.rs:89`, `generators.rs:96`).
   **And its result was discarded**, so even qualified it would not have put the
   test in M3's or M9's radius. **Fixed** — the call is fully qualified, and the
   rationale is corrected: M20f signs the **file-content prohibition**; the radius
   concern is *why* that prohibition exists, not what M20f exhibits.
2. **[P1] Assertions 3–5 had no pinned expected text.** Pin 1a states a
   *requirement*; a requirement is not an expected value, and an executor would
   have written the production string and copied it into the test — asserting
   nothing. **Fixed** — the module paragraph and both rustdocs are pinned **as raw
   source**.
3. **[P1] Assertions 1–5 had no extraction boundaries**, and assertion 1 is
   **unsound without one**: `#[derive(Clone, PartialEq, Eq, Debug)]` occurs
   **twice** in `invariants.rs` — on the violation type (`:249`) and on
   **`DeferredCheck`** (`:286`), which §0.5 deliberately retains — so a file-wide
   search **survives M20**. **Fixed** — every slice is symbolic and anchored to
   the declaration it precedes.
4. **[P1] Equality was unsigned for assertions 2–4.** M20g and M20h discriminated
   only 1 and 5; the rest were deletion-only, which a needle or subset
   implementation satisfies. **Fixed** — **M20i, M20j, M20k**. *M20i **reorders**
   rather than adds: every trait this enum can usefully derive is already there,
   and a per-trait needle check passes on a reordering while equality fails.*
5. **[P2] Finding 5 of round 16 reached the touch row but not pin 1b's premise.**
   **Fixed** — the pin now claims absence of a **complete explicit three-name**
   observer, not absence of any: migrated `score_graph.rs` does resolve
   `ViolationKind` through the root.

---

### REVISION P — review round 16. Three blockers, two smaller. ALL ACCEPTED.

1. **[P1] Pin 1b's no-call constraint was unobserved.** Assertion 6 checked only
   the import line, so the body could start calling `check_requirement` while
   every mutation and gate behaved as expected — **silently widening M3's and
   M9's radii**, which is exactly what the type-level constraint exists to
   prevent. **Fixed** — assertion 6 is **whole-file equality** against the fenced
   source, and **M20f** adds a call while keeping the imports intact.
2. **[P1] `production_source()` is not reachable where pin 1b put its guard.** It
   is a **private `fn` of `mod g3a_tests`** (`invariants.rs:4679`), so a guard
   "beside pin 10's tests" cannot call it and the prescription was not literally
   implementable. **Fixed** — the guard is placed **inside `mod g3a_tests`**, and
   the two alternatives (a second slicing helper, widening that one's visibility)
   are named and declined as surface for no gain.
3. **[P1] Assertions 1–5 were not fully signed.** M20/M20a remove a trait;
   M20b–d delete whole prose units — all of which a guard that merely *searches*
   for `Eq`, `Hash` or one phrase satisfies, while failing every promise the pin
   makes. **Fixed** — all six assertions are **equalities**, and **M20g** (adds a
   trait) and **M20h** (appends a sentence) are the discriminators: a subset or
   needle check accepts both; only equality rejects them.
4. **[P2] Assertion 6 restated its import in the wrong register** — an inline
   Markdown copy wrapped across source lines, against the one-line Rust in the
   fenced block. **Fixed** — the fenced source is its sole authority, and
   assertion 6 no longer respells it.
5. **[P2] Touch row 4c overstated exclusivity.** `tests/score_graph.rs` is an
   integration test too, and once its `.invariant` comparisons migrate it must
   resolve `ViolationKind` from the root, `invariants` being private. **Fixed** —
   4c is the unique **explicit three-name** observer, not the only observer of the
   re-export.

---

### REVISION O — review round 15. Three blockers, three smaller. ALL ACCEPTED.

1. **[P1] M19 could not produce its promised failure.** `mod invariants` is
   **private** (`lib.rs:76`), so deleting a name from the root re-export is an
   **unresolved import** — a compile error, which observes nothing and which gate
   7 cannot record as a failing assertion. The mutation's claim that the function
   "remains public at its module path" was false for the same reason. **Fixed** —
   **M19 is withdrawn**; the re-export is **compiler-observed**, in the same class
   as pin 1's rename. What a mutation *can* reach is whether the test imports all
   three names, which is now assertion 6 of the source guard, signed by **M20e**.
2. **[P1] Pin 1b's source guard was one-fifth signed.** M20 exercised one derive
   line; a guard implementing only that passed while omitting `ViolationKind`'s
   derives, the module header, and both rustdocs. **Fixed** — **six** independent
   assertions, signed by **M20 and M20a–M20e**. *M20a removes `Hash` rather than
   `Ord`, because `Ord` would not compile — and a compile error observes
   nothing.*
3. **[P1] Pin 1b named no locations, and the touch table did not cover it.**
   **Fixed** — the integration test is
   `crates/epiphany-core/tests/public_surface.rs` (**new touch row 4c**), the
   source guard lands in `invariants.rs` (**row 1 updated**), and **the
   integration test is pinned type-level only**: a function-item coercion and a
   `matches!` arm, calling nothing. *Invoking `check_requirement` there would
   silently widen M3's and M9's radii to include it.*
4. **[P2] The README propagation fix stopped one hop short again.** Pin 9 still
   reproduced the cell inline while pin 9a was meant to be sole authority.
   **Fixed** — pin 9 defers.
5. **[P2] Pin 1b created new landing-time falsehoods** — *"no test or gate reads
   them"* and *"omitting the re-export leaves the suite green"* become false when
   pin 1b lands. **Fixed** — scoped to the **ratified input**, as pin 13 now is.
6. **[P2] §3's observer inventory was stale**, naming pins 3a, 10 and 11 only.
   **Fixed** — two classes: *behavioural/classification* (3a, 10, 11) and
   *structural*, reading declarations rather than running the checker (1b, 2, 8).

---

### REVISION N — review round 14. Four blockers, three smaller. ALL ACCEPTED.

1. **[P1] M18 was named in pin 2 but absent from §3 — and I deleted it.**
   Revision M replaced the span from `M17·C1` to `M8` to insert the measured
   radii, and **M18 sat inside that span**. Gate 7 covers §3's mutations, so a
   mutation named only in a pin is a mutation nothing runs. **Restored**, with
   that note in its cell. *A span replacement is how I edit; this is the first
   time it silently removed a neighbour, and it did so in the round that was
   correcting radii.*
2. **[P1] M7d was not compile-complete.** `violating_score` matches exhaustively
   on `GraphInvariant` (`generators.rs:502`), so a variant without an arm is a
   **compile error, which observes nothing**. **Fixed** — M7d now pins a
   `violating_score` arm returning an unchanged valid score, while still omitting
   the variant from `all()`.
3. **[P1] Parts of the public API had no observer.** Pin 1a's derives and
   rustdocs were prose, and **touch row 2's root re-export could be omitted with
   the suite green**, since every pinned caller lives inside `invariants.rs`.
   **Fixed** — **pin 1b**: an **integration** test importing the three names from
   `epiphany_core`'s root (a unit test cannot observe this — it reaches the
   module path regardless), plus a derives/rustdoc source guard. Signed by
   **M19** and **M20**.
4. **[P1] The invariant-side `Display` fixture was unpinned**, and the choice
   decides which cell the test lands in — C1's, C2/C3's, M11's, or none.
   **Fixed** — pinned to a **reversed aleatoric bound**, acquired from
   `check_invariants`; **M11's cell now names the test.**
5. **[P2] §3 still said only M2a was measured.** **Fixed** — four cells were:
   M2a and M17·C1–C3, and *every one differed from what static reading
   predicted.*
6. **[P2] The README oracle had a live duplicate.** Pin 9 quoted the cell inline
   while claiming exactness; pin 9a holds the fenced source. **Fixed** — pin 9
   now defers to pin 9a as sole authority.
7. **[P2] Pin 13 carried another landing-time falsehood** — *"the open S29 row
   exists today"* becomes false once execution resolves it. **Fixed** — scoped to
   the **ratified input**.

---

### REVISION M — review round 13. Five blockers. ALL ACCEPTED.

1. **[P1] Revision L's two propagation fixes never reached the live pins.** The
   README cell stayed an inline double-backtick construct, and pin 13 still said
   both additions are compared as *"the whole normalised added record"* —
   contradicting its own preceding paragraph. **Fixed** — **pin 9a** carries the
   cell as fenced raw source, and pin 13 states outright that the phrase
   described gate 15 only and was never true of gate 10.
2. **[P1] The C4–C10 cleanliness claim was unsatisfiable for C6/C7.** "Only its
   own requirement-arm violation" contradicts this contract's own finding that
   C6's fixture emits **both**. **Fixed** — the property is **"no invariant-arm
   violation"**, which is what the pinned assertion always said and what the
   structural exclusion needs.
3. **[P1] M17's radii were guesses, and all three were wrong. Measured with
   compiling proxies:** **C1 = 18 existing tests**, spanning four
   `review_fix_tests` modules, the g3b matrix tests and — unguessed — **four
   generator tests**, since `violating_score(CrossCuttingRefsResolve)` stops
   violating it. **`f4` is not among them**: its ghost-anchor is C2's, exactly as
   the round said. **C2 = 1** (`f4`). **C3 = 0** — nothing in the tree carries a
   dangling tempo *end* anchor. **C2 and C3 now pin their replacement label**,
   since a different one changes which `check_requirement` retrieves the
   violation.

   *Two measurement faults are recorded because each nearly became a result:* a
   first proxy hit `E0501` — the `flag` closure holds a unique borrow of `out` —
   and reported **0 failures for both C2 and C3**, *a compile error observing
   nothing*, the trap this repository names and that these contracts cite. A
   second attempt used `cargo test`'s `error: test failed` as a compile signal
   and misread a real 1-failure result as non-compiling. **The third used
   `cargo build --tests` and `error[`**, and only its numbers are pinned.
4. **[P1] M7b's radius was false.** `every_invariant_has_a_negative_generator`
   iterates `all()`, so replacing an entry means it visits the replacement twice
   and never asks for the removed variant. **A test driven by the mutated list
   cannot detect an omission from that list.** **Fixed** —
   `graph_invariant_all_is_unchanged` alone.
5. **[P1] Pin 8 froze `all()`, not the enum.** A fully implemented 22nd variant
   **omitted from `all()`** leaves the canonical sequence untouched and every gate
   green — the same hole M18 closes for `ViolationKind`, since a variant nothing
   enumerates is a variant nothing observes. **Fixed** — a second, independent
   **declaration inventory** read from the `enum GraphInvariant` source, signed by
   **M7d**.

---

### REVISION L — review round 12. Four behavioural blockers, two propagation defects. ALL ACCEPTED.

1. **[P1] C1–C3 were tested but never mutation-signed.** §3 opened at C4, so an
   implementation moving a tempo anchor — or the whole reference surface — to a
   requirement arm was **observed by tests nothing verified**. **Fixed** —
   **M17·C1, M17·C2, M17·C3**, each a complete reclassification with an
   exhaustive radius. As predicted, **C2 is the wide one**: it reaches the mixed
   fixture *and* the legacy ghost-anchor assertion in
   `f4_tempo_segment_structural_defects_fire`.
2. **[P1] Pin 8's freeze was only count-signed.** M7 adds a 22nd entry, so a test
   asserting `all().len() == 21` satisfies it while leaving unobserved: the same
   variants **reordered**, one **replaced or duplicated** at length 21, and two
   `number()` arms **swapped**. **Fixed** — the test compares a **canonical
   `(variant, number)` sequence**, pinned in full, and **M7a/M7b/M7c** sign
   order, membership and mapping separately.
3. **[P1] M2a's radius was unestablished for this rung's prospective fixtures.**
   The disposable run measured existing consumers; each new C4–C10 test also
   carries a negative `check_invariant` assertion, and any unrelated invariant
   tripped by a not-yet-built fixture would add it. **Fixed structurally, not by
   estimate** — a **fourth pinned assertion** on each of the seven requires the
   fixture's aggregate **invariant arm to be empty**, which removes the
   possibility rather than predicting it.
4. **[P1] Pin 2's "exactly two arms" had no observer.** A complete third arm with
   a `Display` arm and no emitter **compiles and leaves every gate green**.
   **Fixed** — `violation_kind_has_exactly_two_arms` compares the enum's
   declaration to pin 2's by equality, and **M18** is that third arm.
5. **[P2] The README's "verbatim" cell was still an inline Markdown construct**
   whose own backticks forced outer delimiters, leaving two readings. **Fixed** —
   fenced raw source, as revision K did for `APPEND`.
6. **[P2] Pin 13's final sentence still repeated the superseded method.**
   **Fixed** — deleted; gate 10 reconstructs from removed-plus-added.

---

### REVISION K — review round 11. Three blockers, two lifecycle/precision defects. ALL ACCEPTED.

1. **[P1] `APPEND` was pinned in the wrong source register.** A Markdown
   blockquote's `>` prefixes are part of the raw text and survive
   whitespace-collapsing, while the ledger row has none — so the reconstruction
   could never match. **Fixed** — `APPEND` is a **fenced raw-source block**.
2. **[P1] Pin 13's closing paragraph never received the ledger fix**, still
   describing gate 10 as an added-lines check. **Fixed** — and the paragraph now
   says *why the two gates differ*: a new Revision History row genuinely is a run
   of added lines, while an append to a one-line table row rewrites that line, so
   only gate 15 can use the added-lines method.
3. **[P1] The README guard forbade digits, not restated counts.** *"all
   twenty-one variants returned by `GraphInvariant::all()`"* carries no digit and
   recreates the defect exactly. **Fixed** — the invariant-description cell is
   **pinned verbatim and compared by equality**, which is the only form that
   rejects a spelled-out count.
4. **[P2] "The real set is 20" conflicted with M2a's own cell**, which also
   carries `invariant_selector_discriminates_its_payload`. **Fixed** — 20 is the
   measured **pre-existing/legacy** radius; the cell is those 20 **plus** the new
   test.
5. **[P2] The `reduce.rs` contingency was temporally impossible** — re-verified
   "at execution", but a row added "before dispatch" cannot follow, since pin 12
   dispatches at ratification. **Fixed by doing the verification now**:
   `reduce.rs` contains **zero `.invariant` accesses** and both `GraphInvariant`
   uses pass a variant as an argument. Re-verified at revision K, and if it turns
   out wrong during execution, **execution stops and an amendment adds the row**.

---

### REVISION J — review round 10. Five blockers. ALL ACCEPTED.

1. **[P1] M2a's derivation equated object identity with state identity.** Tests
   mutate the score between assertions — `invariants.rs:3216` removes the overlap
   *before* its negative; `invariants.rs:4457` installs the tempo map only
   *after* one — so a positive assertion proves nothing about the state the
   negative selector sees. And **`m35` was not one binding group at all**: its
   negative uses `lone`, its positive a separately built score
   (`invariants.rs:4939`).
2. **[P1] M2a's candidate surface was incomplete.** `!fires(…)` is not the
   surface; the mutation changes **every** `check_invariant` consumer — direct
   zero-result assertions (`invariants.rs:5158`, the S2/S7/S8 matrix cells) and
   **behavioural** ones, notably **`generators.rs:953`, where `shrink` uses the
   selector to choose candidates**, plus test-scope consumers in
   `reduce.rs:16586`.

   **Both fixed by measuring, as ruled.** A disposable implementation —
   `check_invariant` with its payload filter removed — was run against the full
   workspace with `--no-fail-fast`. **The measured *pre-existing* set is 20
   tests, all in `invariants::g3b_measure20_tests`; M2a's cell is those 20 plus
   `invariant_selector_discriminates_its_payload`.**
   Against revision I's derived 13: **3 in common, 10 false positives, 17
   missed** — the two sets are nearly disjoint. *Static adjacency did not
   approximate this radius; it produced a different one.* The tree was restored
   and re-verified at 43 suites / 1586 before continuing.
3. **[P1] The pinned Revision History row had its delimiter backwards.** The
   existing final row already terminates with `\\` immediately before
   `\bottomrule` (`core_spec.tex:17031`), so a block **beginning** with `\\`
   produces two separators before the new row and none after. **Fixed** — no
   leading delimiter, a terminating one, and the insertion point pinned.
4. **[P1] The ledger reconstruction could not hold.** The S29 line **ends with
   the table's terminal `|`**, so appending after the whole line writes outside
   the row. **Fixed** — the comparison strips the trailing delimiter from both
   sides and requires it restored on the added line. **Gate 10's superseded
   added-lines-only instruction is replaced** by the reconstruction.
5. **[P1] The README digit gate was unsatisfiable at row scope.** The row's final
   cell necessarily reads `Ch. 5 §"Graph Invariants"`, so a digit scan over the
   line rejects the correct edit. **Fixed** — the numeral prohibition is scoped
   to the row's **invariant-description cell**.

---

### REVISION I — review round 9. Three blockers, two smaller. ALL ACCEPTED.

1. **[P1] M2a had no precommitted failure oracle.** A 19-test search surface with
   the set deferred left gate 7 nothing fixed to compare against. **Derived and
   pinned: 13 named tests.** Method, stated in §3: enumerate the 19 negative
   selector assertions, keep those where a **positive** assertion falls in the
   same *binding group* — the same score instance. **Two directions of error are
   named rather than hidden** (a rebinding through an unrecognised helper
   over-counts; a score violating an invariant no assertion names under-counts),
   and execution reconciles against the thirteen. *This is the one cell derived
   by reading pre-existing tests rather than constructed from pinned ones, and it
   is flagged as such — M2a mutates code this rung creates, so it cannot be
   measured beforehand, only precommitted.*
2. **[P1] Pin 13 promised exact comparisons and supplied semantic summaries.**
   **Fixed** — the Revision History row is pinned as **verbatim LaTeX**, and the
   ledger append as **verbatim text**. The extraction boundaries are pinned too,
   and the ledger's is the subtle one: **the S29 entry is a single-line table
   row, so an append shows as one removed and one added line and the added line
   is not the append.** The comparison is therefore
   **`added == removed + " " + APPEND`**, which checks the whole logical record
   without pinning the pre-existing cell.
3. **[P1] The README repair perpetuated the defect it found.** Writing `21` for
   `19` restates a count that has already staled twice, against the rule
   `invariants.rs`' own module header states — *"no prose here restates it"*.
   **Fixed** — the README becomes **symbolic**, *"all variants returned by
   `GraphInvariant::all()`"*, and **gate 16 fails on a digit there**.
4. **[P2] M3 was not a complete mutation.** **Fixed** — the predicate is
   **replaced** by `matches!(v.kind, ViolationKind::Invariant(_))`, matching every
   invariant variant and no label, which is what its radius assumed.
5. **[P2] Gate numbering ran 14, 16, 15.** **Fixed** — 1 through 16 in order,
   which matters because execution evidence is keyed by gate number.

---

### REVISION H — review round 8. Five blockers. ALL ACCEPTED.

1. **[P1] The new selector tests were not propagated through the radii.**
   **Fixed** — M1·C5 and M1·C8 gain
   `requirement_selector_discriminates_its_payload` (built from C5 and C8); M3
   and M9 gain it; **M11 gains `invariant_selector_discriminates_its_payload`,
   whose invariant-4 half is the very violation M11 retags.** **M2a was not
   "alone":** `m39_unresolvable_reference_is_invariant_10_only` asserts
   `fires(CrossCuttingRefsResolve)` **and** `!fires(MeasureMeterConsistency)`, so
   `Invariant(_)` breaks it, as it does
   `matrix_b2_governing_signature_unresolving_delegated`. **The candidate surface
   is enumerated — 19 tests carry a negative selector assertion — and the exact
   set is measured at execution**, since which of them fail depends on whether
   their score violates anything else. **M3a's radius was right and its reason
   wrong:** under M3a *alone* C6 and C7 still share a label, so M6 is irrelevant
   to it; what excludes every other test is that each presents one label.
2. **[P1] The equality transition stopped before its consumers.** Pin 3 still
   cited retired stems; M12pre–M13neutral named "positive 1–5"; M14a–c named stem
   assertions. **Fixed.** And **M15a/M15b could no longer sign the structural
   assertion**, since equality fails first on a mis-sliced block — **so that
   assertion is retired as redundant** and both are ordinary equality mutations.
3. **[P1] Pin 9's prose outcomes had no landing observer** — every gate could
   pass with all of them omitted. **Fixed** — **gate 16**, scoped per site. The
   sweep also gained two more: `review_fix_tests_4`'s module doc calls its tempo
   subject *"invariants"*, and **`check_invariants`' public rustdoc never says it
   returns both arms** — the one place a caller looks before relying on pin 4's
   comprehensiveness. **The "six" tally is removed**: the set grew in two
   consecutive rounds, and a count beside a table that can grow is the defect
   this family keeps finding.
4. **[P1] Pin 13's gates observed tokens, not records.** A truncated or misplaced
   addition carries the token and passes. **Fixed** — both gates slice the
   **added record** from the staged diff and compare it in **normalised
   whole-record form** against pin 13's text.
5. **[P1] The `Display` test's acquisition path was unpinned**, and the choice
   decides M3's radius. **Fixed** — acquisition is **from `check_invariants`**,
   pinned, which keeps the test out of M3's radius as the table assumes.

---

### REVISION G — review round 7. Five blockers. ALL ACCEPTED.

1. **[P1] S8 neutrality was bypassable by additive prose.** Five required
   sentences plus three forbidden stems all pass on
   `A \texttt{Constant} segment's \texttt{end\_tempo} \MUST{} be absent.` —
   stem-free, and it **resolves P13-S8**. **Fixed by taking the cleaner option:**
   pin 3a asserts **normalised exact equality** against pin 3's pinned source.
   Pin 3 declares that block complete, so equality is the assertion that matches
   the pin, and it needs to anticipate nothing — *no closed stem inventory can be
   trusted to anticipate the sentence someone will actually write.* **M14e** is
   that sentence, and the demonstration.
2. **[P1] Both selectors' payload discrimination was unsigned.** Every fixture
   presented one variant or one label, so implementations matching
   `Invariant(_)` or `Requirement(_)` — **ignoring the argument** — satisfied
   every assertion. **Fixed** — two fixtures carrying **two** invariant variants
   and **two** requirement labels, with exact projections, signed by **M2a** and
   **M3a**. M2/M3 signed arm selection only.
3. **[P1] The real-violation `Display` oracle was not independent.** An expected
   string built from `violation.witness` moves with the actual when M10 restores
   the suffix, so the test would not fail. **Fixed** — an **independent**
   assertion that the witness contains no `req:` comes **first**, and the wrapper
   equality is then built from the witness that assertion has already checked.
4. **[P1] The prose migration stopped at the `/// 10.` block.** Five more live
   statements become false — the tempo, aleatoric and accidental headers, the
   migrated negative's message, and `README.md:25`, which exposes the old type
   **and a count already stale at 19**. **Fixed** — pin 9 covers all six, with
   touch rows 4a and 4b. **`DECISIONS.md` is not rewritten**: it gains an explicit
   supersession note, because a decision record that is edited stops recording
   why the multiplexing was once defensible.
5. **[P1] Both permanent history additions were authorized without pinned
   content**, and their gates were satisfiable by baseline artifacts — the
   Revision History chapter and the open S29 row both exist today. **Fixed** —
   **pin 13** pins distinguishing content for each, and gates 10 and 15 are
   rescoped to **added lines in the staged diff**. *Third occurrence of the
   touch-row-without-a-pin class in this family, which is why it is pinned rather
   than tidied.*

---

### REVISION F — review round 6. Three blockers, one precision issue. ALL ACCEPTED.

1. **[P1] Pin 10's pair matrix was incomplete, and M10's radius contradicted
   it.** Only C6 and C7 had pinned witnesses; C4, C5, C8, C9 and C10 would have
   been derived at execution. **Fixed** — the matrix is closed, and it
   distinguishes two forms, because the checker does: **C4–C7's witnesses are
   fixed strings and are pinned as equalities**; **C8–C10's are `format!`-built
   and carry ids**, so what is pinned is the **discriminating substring** — enough
   to separate same-label siblings without pinning `Debug` output a fixture
   change would move. **M10's radius gains
   `accidental_incompatible_reports_tuning_requirement`**: restoring the suffix
   changes C10's witness, so every observer of that witness fails.
2. **[P1] M6 described an impossible selector outcome.** After C6 is mislabelled,
   C7 still emits under `req:time:tempo-segment-order` from the same fixture, so
   `check_requirement` for that label is **not** empty. **Fixed** — M6 pins the
   real outcome: the expected **C6 pair** is absent while **C7's pair remains**,
   and both assertions fail on the *pair*. *The old "returns nothing" rationale
   contradicted the very masking that motivated pairs in revision E.*
3. **[P1] M13post did not sign both closing sentences.** Positive 4 was one
   literal holding two independent sentences, and M13post deleted the whole
   paragraph — so a guard implementing only the first sentence would still fail
   it and satisfy the plan, leaving the **S8-neutrality sentence unobserved**.
   **Fixed** — positives 4 and 5 are separate, with **M13post** and
   **M13neutral** deleting one sentence each.
4. **[P2] Pin 11b's assertion shape moved the observation count.** Two predicates
   as two assertions would make the module 5 observations and the inventory 26,
   against gate 13's 25. **Fixed** — pinned as **one combined assertion**,
   inventory unchanged at 25, and described as **combined witness predicates**
   rather than an exact witness, since the full string carries `Debug` ids.

---

### REVISION E — review round 5. Four blockers, two smaller. ALL ACCEPTED.

1. **[P1] C6 could be masked by C7, so M6 was not guaranteed to fail.** C6's
   natural fixture also overlaps, so after mislabelling C6 alone, C7 keeps
   supplying `req:time:tempo-segment-order` and both label-only assertions still
   pass. **Fixed** — pin 10's requirement-arm template asserts **`(kind,
   witness)` pairs**, with the witnesses pinned verbatim from the checker:
   `tempo segments are out of start order` and `tempo segments overlap in musical
   time`. *The alternative isolated C6 fixture needs an end-before-start segment,
   a shape the checker should never see.*
2. **[P1] Pin 11b's inventory was still wrong.** **Three** observations use
   `witness.contains(…)`; the fourth is a `fires()` call observing selector
   non-emptiness. **Fixed** in §0.4, pin 11b and revision D's record. The
   "exact-witness assertion" also named no expected value — **now two exact
   predicates**: the witness **ends with** `interval algebra` and **contains no**
   `req:`. **M10's radius gains the legacy test**, which those predicates make it
   fail.
3. **[P1] M16b risked proving the wrong assertion.** The test asserts
   `accidental_extensions.is_empty()` *before* the selector call; inserting the
   extension above that line trips the precondition first. **Fixed** — the
   mutation is pinned complete and **ordered**: `let mut`, the emptiness
   assertion kept in place, then the `edo-31` space and the `CmnChromatic`
   extension inserted **after** it. Both edits are named so incompatibility is
   not inferred.
4. **[P1] Pin 3 pinned a complete source that the guard only half observed.**
   Deleting the opening `\MUST{}` sentence or the closing neutrality paragraph
   passed pin 3a and every gate — and the second is the sentence that makes the
   requirement S8-neutral in prose. **Fixed** — four exact positives covering
   every sentence, with **M12pre** and **M13post** signing the two that were
   unguarded.
5. **[P2] The stem guard did not specify case normalisation.** *"Canonically, …"*
   escapes a case-sensitive search while M14a–c all still pass. **Fixed** — the
   slice is lowercased before scanning, and **M14d** uses an uppercase stem to
   sign the fold.
6. **[P2] M15a/M15b named an assertion but no test.** **Fixed** — both name
   `tempo_segment_shape_requirement_states_its_clauses_and_stays_s8_neutral`.

---

### REVISION D — review round 4. Four blockers, three propagation errors. ALL ACCEPTED.

1. **[P1] Pin 11b contradicted pin 8a.** All four accidental observations
   identify the rule by `v.witness.contains("accidental-modification-compatibility")`
   — **the exact suffix pin 8a deletes**. *(Revision E: **three** of the four do;
   the fourth is a `fires()` call.)* A selector-only migration would make
   the positive fail and leave both negatives passing *vacuously*, since
   `.all(|v| !v.witness.contains(…))` is true of every violation once the suffix
   is gone. **Fixed** — pin 11b pins replacement **predicates**: negatives assert
   `check_requirement(…).is_empty()`; the positive asserts non-empty plus a
   label-free witness property.
2. **[P1] M9 and gate 13 had the negative-test logic backwards.** M9 makes
   `check_requirement` return nothing, so both negatives **pass vacuously** and
   are *not* in its radius — I had counted them as failures, asserting the very
   vacuity the migration removes as though it were detection. And M1·C10 cannot
   prove them non-vacuous: **their fixtures emit no violation to re-tag.**
   **Fixed** — **M16a** and **M16b** make each negative's own fixture violate, and
   M3/M9's legacy set is stated as **four test functions**, not nine
   observations.
3. **[P1] Pin 3a claimed exact literals and supplied none**, and pin 3 never
   pinned the requirement's source or placement, so M12/M13 were not literal
   mutations. **M15's premise was also false** — the following requirement does
   not carry the non-constant clause, and the later `end_tempo` occurrence is in
   a rationale. **Fixed** — pin 3 pins the complete source and its placement;
   pin 3a's positives are exact normalised literals from it; and the boundary is
   observed **structurally**: the slice contains exactly one `\label{` and no
   inner `\begin{requirement}`. **One assertion catches over-reach and
   under-reach alike**, so **M15a/M15b** are ordinary failing mutations and the
   start boundary is covered too.
4. **[P1] Gate 7 was unsatisfiable for M15**, which demanded a verbatim failing
   assertion for a mutation whose success was another mutation ceasing to fail.
   **Fixed by removing the passing control entirely** — §3 now states this rung
   has **no passing-outcome mutation**, so gate 7 applies uniformly.
5. **[P2] §0.4 omitted `accidental_compatibility_tests`** from the measured blast
   radius; revision C's correction reached pin 11 and gate 13 and stopped before
   the section that claims to inventory the radius. **Fixed.**
6. **[P2] The live status still read revision B.** **Fixed.**
7. **[P2] §3 still said radii derive from pin 10's tests** — repeating the exact
   omission revision C corrected. **Fixed** — pins 3a, 10 **and** 11.

---

### REVISION C — review round 3. Four blockers, three smaller. ALL ACCEPTED.

1. **[P1] Pin 11's table totalled 22.** The `f3_aleatoric_bounds` row already
   counted its invariant-4 assertion, so "nine further" double-counted it.
   **Measured and fixed** — eight further, in exactly the four tests the round
   named: `f4_…offset_kind` 1, `f5_…tempo_conversion` 2, `f6_…namespaces` 3,
   `f10_…member_notation` 2. Eight plus the `f3` invariant-4 gives the nine
   other-invariant assertions; 12 CCRR + 9 = 21.
2. **[P1] The migration surface omitted `accidental_compatibility_tests`** —
   four selector-based observations in three tests (`invariants.rs:4594`–`:4663`).
   **Fixed** — pin 11b. **The sharper half is the two negatives:** they assert the
   selector returns *empty*, so under pin 5 they stay green and go **vacuous**,
   remaining green through any future accidental-compatibility regression. A
   migration repairing only the loudly-failing test leaves two tests silently
   weakened.
3. **[P1] §3 derived radii from pin 10 alone, ignoring the legacy observers.**
   **Fixed** — M1 is derived per condition (including that **C6/C7 fail their
   pin-10 test alone**, since the legacy fixture trips both and re-tagging one
   leaves the assertion passing), M3 and M11 gain their legacy observers, and M9
   is re-derived. **M9 also still named the deleted
   `comprehensive_check_retains_both_arms` — a live one-hop error from revision
   B, now removed and recorded as such.** **Pin 6a** newly *requires* the
   architecture M9 assumed: both selectors are projections of `check_invariants`.
4. **[P1] Pin 3a was not a complete guard artifact.** **Fixed** — test name,
   exact slice endpoint (the **first** `\end{requirement}`, because `end_tempo`
   appears later in Chapter 3 and a loose slice would pass on another
   requirement's prose), normalisation rule, exact positive literals, and a
   **closed forbidden-stem inventory** — `canonic`, `normaliz`, `prefer` — with
   **M14a, M14b, M14c** exercising each separately. **M15** is a passing-outcome
   control that signs the slice endpoint itself.
5. **[P2] Touch row 1 omitted pin 3a.** **Fixed.**
6. **[P2] Revision B overclaimed M7's verification.** Six of the seven exist
   today; `graph_invariant_all_is_unchanged` is **prospective**, pinned by pin 10.
   **Fixed.**
7. **[P2] "No free-text arm" was literally inaccurate** — `Requirement` carries a
   `&'static str`. **Fixed** — the rule is *no unclassified fallback arm*: every
   violation names either a numbered invariant or a real requirement label.

---

### REVISION B — review round 2. Five blockers, two smaller. ALL ACCEPTED.

1. **[P1] Pin 10's assertion template was impossible for C1–C3.** It required
   every condition to be *absent* from the invariant selector and returned by
   `check_requirement` — but C1–C3 deliberately stay invariant 10. **Fixed** —
   the template splits by arm, two assertions for the invariant arm and three for
   the requirement arm.
2. **[P1] Pin 11's inventory was false.** Measured: `review_fix_tests_4` holds
   **21 `fires()` assertions**, of which **12** target `CrossCuttingRefsResolve`
   and 9 target other invariants; of the 12, **5 migrate and 7 stay**. My "21
   `CrossCuttingRefsResolve` assertions" counted identifier occurrences over the
   wrong line range. **Fixed** — pin 11 carries the per-test table, gate 13
   requires 21 preserved / 5 migrated / 16 unchanged, and §0.4 and revision A's
   record are swept.

   *Two details only reading the tests reveals:* `f4`'s fourth assertion is a
   **ghost region**, i.e. anchor existence, so it **stays**; and its out-of-order
   fixture — `seg(2,3)` then `seg(1,2)` — trips **both C6 and C7** in one
   assertion.
3. **[P1] M7 invented an implementation step and truncated its radius.** `shrink`
   has no per-invariant `match`, so no passthrough is needed to compile.
   **Fixed** — four pinned edits, and the radius is **seven** tests, because
   `Probe22` enters `all()` with a fixture that violates nothing. **Six exist
   today and were verified; `graph_invariant_all_is_unchanged` is prospective**,
   pinned by pin 10 of this contract.
4. **[P1] Pin 3a's clause guard was present but unsigned.** M8 deletes the whole
   requirement, proving only that the slice anchor exists. **Fixed** — M12, M13
   and M14, one per clause. M14 is the one that matters: it is the edit a
   well-meaning later reader would make, and it silently resolves P13-S8 inside a
   requirement minted specifically not to.
5. **[P1] M2's radius named a test its pin left insensitive.**
   `mixed_fixture_splits_by_arm` was specified against the aggregate only, which
   widening `check_invariant` does not change. **Fixed** — pin 10 requires it to
   exercise **all three surfaces**, M3's radius expands to match, and the
   duplicate `comprehensive_check_retains_both_arms` is dropped rather than kept
   as a second observation of the same assertion.
6. **[P2] M6 claimed all three assertions detect it.** Two do; the third is a
   **positive control**. **Fixed.**
7. **[P2] Pin 1a attached the `BTreeSet` rationale to the wrong type.** It
   constrains `ViolationKind: Ord`, not the struct's derives. **Fixed** — the
   struct's retention is justified as public-API preservation, separately.

---

### REVISION A — review round 1. Nine findings, eight blocking. ALL ACCEPTED.

1. **[P1] Coverage was by label, not by emitted condition.** One fixture per
   *label* can miss half of a shared label's surface — both tempo-shape
   conditions, both order conditions, both aleatoric conditions. **Fixed** —
   pin 10's matrix is **per condition**, and pin 11 pins the migration of
   `review_fix_tests_4`'s assertions, preserving reversed aleatoric bounds as
   **invariant 4**. *(This entry said "21 `CrossCuttingRefsResolve` assertions";
   revision B measured 21 `fires()` assertions of which 12 target that variant.)* Without that pin, execution
   could satisfy the suite by deleting or softening the tests that fail under
   the new selector.
2. **[P1] The mutation matrix was neither executable nor exhaustive.** Radii
   were guessed per mutation rather than derived against a named test set; M7
   would have stopped at a non-exhaustive-match **compile error**, which
   observes nothing; M8's radius was wrong. **Fixed** — §3 derives every radius
   from pin 10's named tests, by construction, and M7 pins a complete compilable
   variant.
3. **[P1] Gate 6 was unsatisfiable.** `DeferredCheck.invariant` is deliberately
   retained (§0.5), and this contract and the ledger quote `InvariantViolation`
   historically. **Fixed** — the identifier check is scoped to **Rust code**, and
   the field migration to **`WellFormednessViolation` consumers**, with
   `DeferredCheck.invariant` explicitly allowed.
4. **[P1] The contract's own lifecycle was unowned** — touch row 9 permitted a
   status edit no pin required. **Fixed** — pin 12 and gate 14.
5. **[P1] Gate 12 contradicted itself**, requiring the baseline to stay at
   `CLAUDE.md`'s figures while the rung adds tests. **Fixed** — gate 1 owns the
   new total; gate 12 owns the *targeted* `requirement_labels` run with the
   temporary row absent and the label defined.
6. **[P1] Pin 3's normative content was unobserved.** M8 proves only that a
   block and label exist. **Fixed** — pin 3a adds a prose guard over the
   requirement's clauses.
7. **[P1] The public API surface was incomplete**, and pin 4 overclaimed.
   **Fixed** — pin 1a pins derives and documentation; pin 4 now says it
   preserves rule **coverage**, not caller **meaning**, and lists what does
   change.
8. **[P1] The accidental witness already embeds its own label**
   (`invariants.rs:1594`), so the requirement `Display` form would render it
   twice. **Fixed** — pin 8a strips the suffix, and the `Display` test uses a
   **real** accidental violation, not a synthetic value.
9. **[P2] Touch row 1 carried a pre-change grep count.** "Five emission sites
   re-tagged" is false: one must stay invariant 10, and the tempo site splits by
   condition. **Fixed** — the row describes the transformation.

---

## §0. What was verified before drafting

### 0.1 The defect, and why it is not internal

`GraphInvariant::CrossCuttingRefsResolve` is emitted from **five sites in four
functions**. Only `check_cross_cutting_refs`, plus the tempo map's two
segment-anchor conditions, are reference resolution.

`check_invariant(score, which)` is `pub` and re-exported from `epiphany_core`'s
root, and `impl Display for InvariantViolation` renders:

```
invariant 10 (CrossCuttingRefsResolve) violated: non-constant tempo segment is missing its end_tempo
```

A Chapter 3 tempo rule, attributed in user-visible text to a Chapter 5 graph
invariant, through a public API.

### 0.2 The classification is per emitted condition, and it is P13-S26's

S26 derived and ruled it; this rung implements it. **Pin 10 carries the closed
matrix**; the summary is that anchor existence stays invariant 10 and the seven
rider conditions move to four labels.

### 0.3 `GraphInvariant` does not move

- `core_spec.tex` states **"This enumeration contains exactly 21 invariants"**.
- The enum is *"numbered as in §Graph Invariants"*, so a variant with no
  enumeration entry has no number to return.
- S26 ruled invariant 10 stays **normatively** reference resolution.

Minting variants would also owe a negative generator and a shrink path each —
`every_invariant_has_a_negative_generator` and
`every_invariant_shrinks_to_a_small_witness` iterate `all()`. **This rung adds no
variant, so neither obligation grows.**

### 0.4 The blast radius, measured

- **No wire reach.** `GraphInvariant`'s only uses outside `epiphany-core` are two
  **test-scope** sites in `reduce.rs` (nearest `#[cfg(test)]` at `:9663`), and
  both name a *variant*, not the field.
- **`.invariant` on the violation type: four consumers outside `invariants.rs`** —
  `tests/score_graph.rs` ×2, `src/generators.rs` ×2.
- **`review_fix_tests_4` (`invariants.rs:4217`–`:4591`) holds 21 `fires()`
  assertions**, all via `check_invariant`. **12 target `CrossCuttingRefsResolve`;
  9 target other invariants.** Of the 12, **5 are riders that migrate** and **7
  are genuine invariant 10 that stay** — the split is pin 11's table. Pin 11 owns
  all 21.
- **`accidental_compatibility_tests` (`invariants.rs:4594`–`:4663`) holds 4
  selector-based observations in 3 tests**: **three** identify the rule by the
  witness suffix pin 8a deletes, and **one** is a `fires()` call observing only
  selector non-emptiness — see pin 11b, which is why a selector-only migration is
  not enough.
- **`check_invariants` has ~10 consumer files.** Pin 4 keeps it comprehensive.
- **Current derives:** `#[derive(Clone, PartialEq, Eq, Debug)]`.

### 0.5 `DeferredCheck` is checked and excluded

It carries a `GraphInvariant` and renders *"invariant N (…) deferred"*, so it
resembles the defect. It is not: its only producer,
`deferred_region_overlaps`, tags `GraphInvariant::RegionExtents` — a **genuine**
invariant, correctly attributed. **Not renamed, not re-typed**, and its
`.invariant` field is explicitly exempt from gate 6.

### 0.6 The tempo-shape rule, and P13-S8

Enforced today, verbatim:

```rust
TempoShape::Constant => seg.end_tempo.as_ref().is_none_or(|et| et == &seg.start_tempo)
TempoShape::Linear | TempoShape::Exponential | TempoShape::Curve => seg.end_tempo.is_some()
```

**P13-S8 asks whether `Constant` + `Some(equal)` is canonical or normalizes to
`None`.** Pin 3 states the **compatibility** enforced today and takes no position
on canonical form, so the label survives whichever spelling S8 ratifies.

### 0.7 One consequence for P13-S26's landed text

S26's pin 5 put a note in the `/// 10.` doc block saying the riders *"are
reported under this same tag"*, naming P13-S29 as owner. **This rung makes that
false**, and pin 9 updates it. Not a violation of S26's frozen pin: pins govern
their own execution, not the tree forever.

---

## §1. Pins

### Pin 1 — the type becomes neutral

`InvariantViolation` → **`WellFormednessViolation`**; field `invariant` →
**`kind: ViolationKind`**. A type called `InvariantViolation` cannot honestly
carry a requirement failure, and the struct is public.

### Pin 1a — derives and documentation are pinned

- `WellFormednessViolation` **retains** `#[derive(Clone, PartialEq, Eq, Debug)]`
  exactly, as measured in §0.4. The rationale is **public-API preservation**: the
  struct is public, and a rung that renames a type has no business also narrowing
  what callers can do with it.
- `ViolationKind` derives **`Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
  Debug`** — matching `GraphInvariant`'s own derives. **`Ord` is load-bearing
  independently:** `generators.rs` collects violation kinds into a `BTreeSet`,
  which requires it. *That collection constrains this enum, not the struct's
  derives; an earlier draft attached the rationale to the wrong type.*
- The module header and both types' rustdoc state the two-arm split and that a
  requirement failure is **not** an invariant failure.

### Pin 2 — `ViolationKind` has exactly two arms

```rust
pub enum ViolationKind {
    Invariant(GraphInvariant),
    Requirement(&'static str),
}
```

**No third arm, and no unclassified fallback arm.** `Requirement` does carry
text — a `&'static str` label — so the rule is not "no text"; it is that **every
violation names either a numbered invariant or a real requirement label**, with
nothing in between. A condition with no label does not belong here, which is why
pin 3 mints one rather than adding an `Other`.

**The two-arm rule has an observer, because otherwise it has none.** A complete
third arm — variant, `Display` match arm, and no emitter — **compiles and leaves
every other gate green**, since nothing produces it. `violation_kind_has_exactly_two_arms`
therefore asserts the enum's shape: the source's `enum ViolationKind` block,
whitespace-collapsed, **equals** pin 2's declaration above. **M18** adds a
complete, compiling third arm with a `Display` arm and no emitter, and must fail
it.

### Pin 3 — `req:time:tempo-segment-shape` is minted, source and placement pinned

**Placement:** immediately after `req:time:tempo-segment-order`'s
`\end{requirement}` in `spec/core_spec.tex`, before whatever follows it.

**Source, verbatim — this is what execution writes:**

```latex
\begin{requirement}
  \label{req:time:tempo-segment-shape}
  A tempo segment's \texttt{shape} and its \texttt{end\_tempo} \MUST{} be
  compatible. If \texttt{shape} is \texttt{Constant} and \texttt{end\_tempo}
  is present, it \MUST{} equal \texttt{start\_tempo}. If \texttt{shape} is
  \texttt{Linear}, \texttt{Exponential} or \texttt{Curve}, \texttt{end\_tempo}
  \MUST{} be present.

  This requirement states the compatibility that is enforced. It does not
  determine whether a constant segment records an \texttt{end\_tempo} at all.
\end{requirement}
```

**It takes no position on canonical form.** P13-S8 decides whether `Constant` +
`Some(equal)` is canonical or normalizes to `None`; this text is true under
either, and the closing sentence says so without settling the question. *Pin 3a
observes this block by **exact equality**, so no phrase or stem inventory
governs its wording.*

Moves `requirement_labels.rs`' three counts. **Measured at execution, never
predicted.**

**Pin 3 also deletes the temporary allowlist row.** Naming the label broke the
citation gate immediately; one `DISCUSSED_NOT_CITED` row was added as
**prerequisite review scaffolding — not dispatch, licensing no other pin work** —
on the owner's authorization. Pin 3 removes it when it mints the requirement,
because the row's claim (*discussed, never cited*) becomes false then. Removed by
hand if S29 is abandoned or the label changes.

### Pin 3a — the requirement's clauses are guarded, and the guard is pinned

**Test:** `tempo_segment_shape_requirement_states_its_clauses_and_stays_s8_neutral`,
in `invariants.rs` beside pin 10's tests.

**Slice.** `spec/core_spec.tex`, whitespace-collapsed: from the
`\begin{requirement}` immediately preceding `\label{req:time:tempo-segment-shape}`
to the **first** `\end{requirement}` at or after that label.

**No separate structural boundary assertion.** Revision F added one — exactly one
`\label{`, no inner `\begin{requirement}` — to catch a mis-sliced block. **Under
equality it is redundant**: a slice extended past its `\end{requirement}` or
started at an earlier `\begin{requirement}` is *unequal* to pin 3's source and
fails on that. Retained as two assertions, M15a and M15b could no longer sign it
independently, since equality fails first. **It is retired, and M15a/M15b are
ordinary equality mutations.**

**Content assertion — normalised exact equality against pin 3's pinned source.**
The slice, whitespace-collapsed, **equals** pin 3's block, whitespace-collapsed.
Nothing weaker.

**Why equality rather than required phrases plus a forbidden-stem list.** Pin 3
declares the block *complete source*, so equality is the assertion that matches
the pin. Every weaker form leaves an **additive** hole: a guard checking five
required sentences and three forbidden stems passes on

```latex
  A \texttt{Constant} segment's \texttt{end\_tempo} \MUST{} be absent.
```

which contains no `canonic`, `normaliz` or `prefer` — and **resolves P13-S8
inside a requirement minted specifically not to.** No closed stem inventory can
be trusted to anticipate the sentence someone will actually write. Equality needs
to anticipate nothing.

**The cost is intended:** any later edit to this requirement breaks the guard and
forces deliberate review, which is what a frozen normative block should do.

*(This replaces revision E's five positive literals and revision D's stem
inventory; both are subsumed. M12, M13, M13post, M13neutral and M14a–M14d all
now fail this one assertion, each with its own `assert_eq!` diagnostic naming
what differs.)*

### Pin 4 — `check_invariants` stays comprehensive

It returns **both** arms, so every broad caller keeps exactly the **rule
coverage** it has today and no rule stops being checked anywhere.

**It does not preserve caller *meaning*, and this pin does not claim to.** What
changes for callers: the type name, the field name and type, the targeted
selector's results (pin 5), `Debug` output, and the rider arm's `Display`.

### Pin 5 — `check_invariant` filters the `Invariant` arm only

Matches only `ViolationKind::Invariant(which)`. **This is the deliberate
behaviour change:** `check_invariant(score, CrossCuttingRefsResolve)` returns
**only genuine invariant-10 failures**.

### Pin 6 — `check_requirement` is added, symmetric

`check_requirement(score, label: &str) -> Vec<WellFormednessViolation>`, matching
only `ViolationKind::Requirement(label)`. A rung that made invariant failures
selectable while leaving requirement failures reachable only by scanning the
comprehensive result would have moved the asymmetry, not removed it.

### Pin 6a — both selectors are projections of `check_invariants`

`check_invariant` and `check_requirement` are **filters over the comprehensive
result**, not independent traversals. One traversal produces every violation;
each selector projects one arm out of it.

**Pinned because §3's M9 depends on it.** If the selectors traversed the score
independently, narrowing `check_invariants` would not reach them, and M9's radius
— the widest in the rung, and the measure of what pin 4 protects — would be
wrong. An architecture assumed by a mutation must be required by a pin.

### Pin 7 — the two `Display` forms are exact

- Invariant arm: `invariant {n} ({variant:?}) violated: {witness}` — **unchanged**.
- Requirement arm: `requirement {label} violated: {witness}`.

**A requirement failure never renders the word `invariant` and never renders a
number.**

### Pin 8 — `GraphInvariant` is untouched, and the freeze is sequence-signed

No variant added or removed; `number()` unchanged; `core_spec.tex`'s count claim
unchanged.

**`graph_invariant_all_is_unchanged` compares `GraphInvariant::all()` to this
canonical `(variant, number)` sequence, in order, by equality:**

```
EventVoiceBacklink=1        VoiceEventBacklink=2         VoiceEventsSortedNonOverlap=3
EventCoordinateModel=4      ContainmentTree=5            StaffInstanceResolves=6
RegionExtents=7             MeasureSingleInstance=8      AnchorOffsetModel=9
CrossCuttingRefsResolve=10  UniqueIdentifiers=11         PitchIdUnique=12
SpellingScopeResolves=13    DecompositionTargetResolves=14  DecompositionSum=15
TupletSum=16                TiePairing=17                VoiceOriginConsistent=18
BarlineGroupSameRegion=19   MeasureMeterConsistency=20   StaffGroupMembershipAgreement=21
```

**A length check is not the freeze.** M7 adds a 22nd entry, and a test asserting
only `all().len() == 21` fails it — while leaving unobserved: the same variants
in a **different order**, one entry **replaced or duplicated** with the length
still 21, and two `number()` arms **swapped**. M7a, M7b and M7c sign those three
independently.

**And `all()` is not the enum.** A **fully implemented 22nd variant that is
omitted from `all()`** leaves the canonical sequence untouched and every gate
green — the same hole M18 closes for `ViolationKind`, since a variant nothing
enumerates is a variant nothing observes. The test therefore carries a **second,
independent inventory**: the `enum GraphInvariant` **declaration** is read from
source and its variant list, in order, must equal the canonical sequence's.
**M7d** declares a complete 22nd variant with a `number()` arm and **no `all()`
entry**, and must fail on that inventory and nothing else.

### Pin 8a — the accidental witness stops embedding its label

`check_accidental_modification_compatibility`'s witness ends
`"… (req:tuning:accidental-modification-compatibility)"`. Under pin 7 the label
becomes the `Display` prefix, so the suffix **is deleted** — otherwise a real
violation renders the label twice.

**Pin 10's `Display` test uses a real accidental violation**, produced by the
checker, not a hand-built `WellFormednessViolation`. A synthetic value cannot
observe this, because the duplication lives in the *witness the checker writes*.

### Pin 9 — consumers migrate, and **every** live statement made false is corrected

The four `.invariant` consumers migrate to `.kind`. **The prose migration does not
stop at the `/// 10.` block.** Every live statement made false is pinned below;
**no count is stated**, because the set grew in two consecutive rounds and a
tally beside a table that can grow is the defect this family keeps finding:

| Site | What becomes false |
|---|---|
| `invariants.rs:8`, the module header | *"a typed [`InvariantViolation`] witness"* — gate 6 forbids that token in Rust code, so pin 1b pins its replacement, `` [`WellFormednessViolation`] `` |
| `invariants.rs`, the `/// 10.` doc block | S26's rider note: the riders are no longer *"reported under this same tag"*, and P13-S29 is no longer their pending owner. Rewritten to name the four labels |
| `invariants.rs:1421` region | the tempo-map header says its conditions are *"surfaced here under invariant 10"* |
| `invariants.rs:1500` region | the aleatoric header says dangling references *"go under invariant 10"* |
| `invariants.rs:1557` region | the accidental header says it is *"surfaced under an existing `GraphInvariant` tag rather than minting a new one"* — **it now carries its own requirement label.** The same comment cites `core_spec.tex:3120`, **a line locator already wrong before this rung**; it is replaced by the symbolic `req:tuning:accidental-modification-compatibility`, which is what it meant |
| `invariants.rs:4624` | the migrated negative's message calls it *"the compatibility invariant"* |
| `crates/epiphany-core/README.md:25` | exposes **`InvariantViolation`** and *"all 19 enumerated graph invariants"* — **the count was already stale** (20 since G3b, 21 since S16). **Its invariant-description cell is replaced with the text pin 9a pins; pin 9a is the sole authority and this row does not restate it** |
| `review_fix_tests_4`'s module doc | describes its subject as *"tempo-map segment invariants"* — after this rung they are requirements, not invariants |
| `check_invariants`' public rustdoc | does not say it returns **both** arms. Pin 4 makes comprehensiveness the property callers depend on; the doc must state it, since that is the one place a caller looks before relying on it |

**`crates/epiphany-core/DECISIONS.md:1061` records the multiplexing as current
policy**, wiring the accidental check into `check_invariants` as *"the
`req:tuning:accidental-modification-compatibility` invariant"*. **It is not
rewritten.** A decision record is a record of what was decided; it gains an
explicit **supersession note** naming P13-S29 and the date, leaving the original
decision legible. *Rewriting it would destroy the reasoning that made the
multiplexing defensible at the time, which is exactly what a later reader needs
in order to understand why it was accepted and then reversed.*

### Pin 1b — the public surface has observers

**As of the ratified input**, pin 1a's derives and rustdocs are prose that no
test or gate reads, and touch row 2's root re-export has **no complete explicit
observer**: the migrated `tests/score_graph.rs` will resolve `ViolationKind`
through the root, so the re-export is not unobserved outright — but no test names
all three exported items, so dropping `check_requirement` alone would leave the
suite green. *Scoped to the ratified input: once
this pin lands, both statements are false of the tree.*

**`crates/epiphany-core/tests/public_surface.rs`** — a new integration test,
`public_violation_surface_is_reexported`, **type-level only**:

```rust
use epiphany_core::{check_requirement, ViolationKind, WellFormednessViolation};

#[test]
fn public_violation_surface_is_reexported() {
    let _: fn(&epiphany_core::Score, &str) -> Vec<WellFormednessViolation> = check_requirement;
    let _ = |k: &ViolationKind| matches!(k, ViolationKind::Requirement(_));
}
```

**It calls nothing.** *A call whose result were **asserted** would put this test
in M3's and M9's radii; an ignored call would not, as M20f shows. The file stays
type-level so the question never arises — a function-item coercion and a
`matches!` arm observe the surface without producing a violation.*

**The re-export is compiler-observed, not mutation-observed.** `mod invariants`
is **private** (`lib.rs:76`), so deleting a name from the root re-export is an
**unresolved import** — a compile error, which observes nothing and which gate 7
cannot record as a failing assertion. It is in the same class as pin 1's rename:
the compiler is the observer.

**`violation_types_declare_their_pinned_derives`**, placed **inside
`mod g3a_tests`** — *not merely "beside pin 10's tests": `production_source()` is
a private `fn` of that module (`invariants.rs:4679`), so a guard anywhere else
cannot call it, and the alternatives (a second slicing helper, or widening that
one's visibility) add surface for no gain.*

**Six assertions, each an independent promise, each an EQUALITY, and each with
a pinned extraction boundary.**

| # | Slice — symbolic, never a file-wide search | Assertion |
|---|---|---|
| 1 | locate `pub struct WellFormednessViolation {`; take the line **immediately preceding** it | **equals** `#[derive(Clone, PartialEq, Eq, Debug)]` |
| 2 | locate `pub enum ViolationKind {`; take the line **immediately preceding** it | **equals** `#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]` |
| 3 | in the module header, from the pinned paragraph's **first line** through the **next blank `//!`** | **equals** the module paragraph below |
| 4 | locate `pub struct WellFormednessViolation {`; require its preceding line to be assertion 1's derive; take the contiguous `///` block **immediately preceding that derive** | **equals** the struct rustdoc below |
| 5 | the same, for `pub enum ViolationKind {` and assertion 2's derive | **equals** the enum rustdoc below |
| 6 | the whole of `public_surface.rs` | **equals** the fenced source above |

**Assertions 4 and 5 cannot say "immediately preceding the declaration": the
derive attribute sits between.** The algorithm is therefore three steps —
declaration, then its derive line, then the `///` block above *that*.

**Assertion 1 is unsound without its boundary.**
`#[derive(Clone, PartialEq, Eq, Debug)]` occurs **twice** in `invariants.rs`
today: on the violation type (`:249`) and on **`DeferredCheck`** (`:286`), which
§0.5 deliberately retains. **A file-wide search survives M20**, because removing
`Eq` from one leaves the other matching.

**Assertion 3 replaces nothing.** The module header is a **28-line `//!` run**
with paragraph breaks at `:2`, `:9`, `:13` and `:22`; asserting the whole run
equals a four-line paragraph would **delete 24 lines of retained header**. The
pinned paragraph is **inserted after the title paragraph's blank `//!`** — *not
prepended: prepending would make its first sentence the module's rustdoc summary
line, displacing "The Chapter 5 graph invariants"* — and the slice is bounded by
that paragraph's own first line and the next blank `//!`.

**The three prose blocks are pinned here as raw source**, not described. *Pin 1a
states a requirement; a requirement is not an expected value, and an executor
would otherwise write the production string and copy it into the test, which
asserts nothing.* **Each block states both of pin 1a's promises** — the two-arm
split, and that a requirement failure is not an invariant failure — *so pin 1a's
claim is true of each block individually, not merely of the three collectively.*

Module paragraph:

```rust
//! Violations carry a two-armed [`ViolationKind`]: `Invariant` for the Chapter 5
//! graph invariants, `Requirement` for a normative requirement named by its
//! label. A requirement failure is not an invariant failure, and neither arm is
//! a fallback for the other.
```

`WellFormednessViolation` rustdoc:

```rust
/// A well-formedness failure: one [`ViolationKind`] — `Invariant` or
/// `Requirement` — and the witness identifying the smallest offending objects.
/// A requirement failure is not an invariant failure.
```

`ViolationKind` rustdoc:

```rust
/// What a [`WellFormednessViolation`] failed. `Invariant` names a numbered
/// Chapter 5 graph invariant; `Requirement` names a normative requirement by its
/// label. A requirement failure is not an invariant failure, and there is no
/// third arm and no unclassified fallback.
```

**One retained header line must also be renamed, and pin 9 owns it:**
`invariants.rs:8` reads *"a typed [`InvariantViolation`] witness"*. **Gate 6
forbids that token in Rust code**, so leaving it unpinned would make gate 6
unsatisfiable; it becomes `` [`WellFormednessViolation`] ``, the rest of that
sentence unchanged.

**Every assertion has a NON-DELETION discriminator**, because deletion-only
mutations are satisfied by needle and subset implementations: **M20g** (1),
**M20i** (2), **M20j** (3), **M20k** (4), **M20h** (5), **M20f** (6). *"Additive"
would be inaccurate — M20i is a **reordering**. Revision P signed only 1 and 5
this way and left 2, 3 and 4 deletion-only, so a subset implementation of any of
those three passed the whole plan.*

**Assertion 6 is the whole file, not its import line.** *An import-only check
leaves the body free to start calling `check_requirement` — and a call whose
result were asserted would widen M3's and M9's radii, which is the outcome the
type-level constraint exists to prevent. It is not restated here: the fenced Rust block above is its sole
authority, one line per line, and a second "exact" spelling wrapped into Markdown
prose would be a different string.* **M20f** adds a call while keeping the
imports intact.

### Pin 9a — the README's invariant-description cell, verbatim

Raw source, fenced so it has exactly one interpretation:

```
`check_invariants` over all variants returned by `GraphInvariant::all()`, with a typed `WellFormednessViolation` witness per check
```

Gate 16 compares that row's second cell to this, by equality after whitespace
collapse. **Equality, not a numeral scan:** a scan over the line rejects the
correct edit — the row's final cell necessarily reads `Ch. 5 §"Graph Invariants"`
— and a scan for digits passes on *"all twenty-one variants"*, recreating the
defect without one.

### Pin 10 — the closed per-condition matrix, and the tests

**Coverage is per emitted condition, not per label.** Each row gets a fixture and
a named test. **The assertion template differs by arm**, because C1–C3 stay
invariant 10 and the requirement template is impossible for them:

- **C1–C3 (invariant arm), two assertions:** the aggregate `check_invariants`
  result carries `ViolationKind::Invariant(CrossCuttingRefsResolve)`; and
  `check_invariant(score, CrossCuttingRefsResolve)` **returns** it.
**Every C4–C10 fixture violates no invariant at all** — its aggregate
`check_invariants` result carries **no invariant-arm violation**, and that is a
**pinned fourth assertion** on each of the seven.

*The property is "no invariant arm", not "only its own violation": C6's fixture
emits **both** C6 and C7, as this contract establishes and as pairs exist to
handle. A cleanliness claim of "only its own" would be unsatisfiable for that
pair and for C7's.*

*This is what keeps those seven out of M2a's radius **structurally rather than by
assumption**. Each carries a negative `check_invariant` assertion; under M2a's
wildcard that assertion fails if the fixture emits **any** invariant violation.
An unrelated invariant tripped incidentally by a not-yet-built fixture would put
the test in M2a's cell — which the measured legacy set cannot anticipate, because
these fixtures do not exist yet. Requiring an empty invariant arm removes the
possibility instead of estimating it.*

- **C4–C10 (requirement arm), three assertions, each on a `(kind, witness)`
  **pair**, never on the label alone:** the aggregate contains
  `(Requirement(label), witness)` for that row; the same fixture yields
  **nothing** from `check_invariant(score, CrossCuttingRefsResolve)`; and
  `check_requirement(score, label)` **returns that pair**; and — the fourth —
  the aggregate's **invariant arm is empty**.

**Pairs are required because three labels are shared by two conditions each, and
one fixture can trip both.** C6's natural fixture — `seg(2,3)` then `seg(1,2)` —
is *also* overlapping, so it emits C6 **and** C7 under the same label. A
label-only assertion would still pass after C6 alone was mislabelled, because C7
keeps supplying the label. The pinned witnesses, verbatim from the checker:

**The matrix is closed. C4–C7's witnesses are fixed strings in the checker and
are pinned exactly; C8–C10's are `format!`-built and carry ids, so what is pinned
is the discriminating substring** — enough to tell same-label siblings apart,
without pinning `Debug` output a fixture change would move.

| Condition | Assertion on the witness | Form |
|---|---|---|
| C4 | `constant tempo segment has end_tempo != start_tempo` | **equals**, fixed string |
| C5 | `non-constant tempo segment is missing its end_tempo` | **equals**, fixed string |
| C6 | `tempo segments are out of start order` | **equals**, fixed string |
| C7 | `tempo segments overlap in musical time` | **equals**, fixed string |
| C8 | contains `ordering references event` | **contains**, id-bearing |
| C9 | contains `bounds key event` | **contains**, id-bearing |
| C10 | ends with `interval algebra` **and** contains no `req:` — **one combined assertion** | **combined predicate**, id-bearing |

*The alternative to pairs — an isolated C6 fixture — would need an
end-before-start segment, which is unnatural and tests a shape the checker should
never see.*

| # | Emitted condition | Arm | Test |
|---|---|---|---|
| C1 | `check_cross_cutting_refs` — any condition | invariant 10 | `cross_cutting_refs_stay_invariant_ten` |
| C2 | tempo segment **`start`** anchor existence | invariant 10 | `tempo_start_anchor_stays_invariant_ten` |
| C3 | tempo segment **`end`** anchor existence | invariant 10 | `tempo_end_anchor_stays_invariant_ten` |
| C4 | `Constant` with `end_tempo` ≠ `start_tempo` | `req:time:tempo-segment-shape` | `tempo_constant_mismatch_reports_shape` |
| C5 | non-constant missing `end_tempo` | `req:time:tempo-segment-shape` | `tempo_nonconstant_missing_end_reports_shape` |
| C6 | segment start ordering | `req:time:tempo-segment-order` | `tempo_out_of_order_reports_order` |
| C7 | segment overlap | `req:time:tempo-segment-order` | `tempo_overlap_reports_order` |
| C8 | aleatoric **`ordering`** event outside region | `req:time:aleatoric-reference-locality` | `aleatoric_ordering_outside_region_reports_locality` |
| C9 | aleatoric **`bounds`** key outside region | `req:time:aleatoric-reference-locality` | `aleatoric_bounds_outside_region_reports_locality` |
| C10 | accidental modification expressibility | `req:tuning:accidental-modification-compatibility` | `accidental_incompatible_reports_tuning_requirement` |
| — | reversed aleatoric bounds | **invariant 4, unchanged** | `reversed_aleatoric_bounds_stay_invariant_four` |

**C4 and C5 share a label and are separate conditions; so do C6/C7 and C8/C9.
One fixture per label would leave one of each pair unexercised.**

**Whole-surface tests**, in addition:

- `mixed_fixture_splits_by_arm` — **the fixture is pinned**: one score carrying a
  dangling tempo **`start`** anchor (C2) **and** an out-of-region aleatoric
  **`ordering`** event (C8). **It exercises all three surfaces, not the aggregate
  alone**, or it is insensitive to the selectors it exists to separate:
  - the aggregate carries **both** kinds;
  - `check_invariant(score, CrossCuttingRefsResolve)` returns **the anchor only**;
  - `check_requirement(score, "req:time:aleatoric-reference-locality")` returns
    **the rider only**.

  *An aggregate-only form would leave M2 and M3 undetected here, which is why
  §3's radii name this test for both.*
- `invariant_selector_discriminates_its_payload` — **a fixture violating two
  different invariants**, C1's cross-cutting reference **and** a reversed
  aleatoric bound (invariant 4). `check_invariant(score, CrossCuttingRefsResolve)`
  returns **only** the invariant-10 violation; `check_invariant(score,
  EventCoordinateModel)` returns **only** the invariant-4 one.
- `requirement_selector_discriminates_its_payload` — **a fixture violating two
  different requirement labels**, C5's tempo shape **and** C8's aleatoric
  locality. `check_requirement` for each label returns **only** that label's
  violation.

  *Both exist because every other fixture in pin 10 presents its selector with a
  single variant or a single label. Against those, an implementation matching
  `ViolationKind::Invariant(_)` or `ViolationKind::Requirement(_)` — **ignoring
  the payload entirely** — satisfies every assertion. M2 and M3 sign arm
  selection; these two and M2a/M3a sign that the selector reads its argument.*

- `display_renders_each_arm_exactly` — **two assertions on the requirement side,
  in this order**, because the second alone is circular:

  1. **independent:** the violation's `witness` contains **no** `req:`;
  2. **then** `assert_eq!(violation.to_string(), format!("requirement {} violated: {}", label, witness))`,
     the wrapper built from the witness assertion 1 has already checked.

  *Assertion 2 alone cannot fail under M10: an expected string built from
  `violation.witness` moves with the actual when the suffix returns. Assertion 1
  is the independent oracle, and it is what makes M10 fail here.* **The invariant side's fixture is pinned: a reversed aleatoric bound**, which
  emits `EventCoordinateModel` — invariant 4 — acquired from `check_invariants`
  like the requirement side. *The choice changes radii, so it cannot be the
  executor's: a C1 fixture would put this test in M17·C1's cell, a tempo-anchor
  fixture in M17·C2/C3's, and reversed bounds put it in **M11's**, which is where
  it now belongs and where M11's cell names it.*

  **The violation is acquired from `check_invariants`, the aggregate — pinned,
  not incidental.** Acquiring it through `check_requirement` would put this test
  in M3's radius; through the aggregate it is not. *Leaving the path to the
  executor would leave M3's cell true or false depending on a choice no pin
  made.*
- `graph_invariant_all_is_unchanged` — the same 21, in order.
*`comprehensive_check_retains_both_arms` is **not** a separate test:* it would
duplicate the mixed fixture's first assertion exactly. Pin 4's coverage is
observed by that assertion and by M9's radius, which reaches every aggregate
assertion in the rung.

### Pin 11 — every legacy observer is migrated, not deleted

**Two modules observe the riders through the selector, and both are in scope.**

#### 11a — `review_fix_tests_4` (`invariants.rs:4217`–`:4591`), 21 assertions

| Test | Assertions | Disposition |
|---|---|---|
| `f3_aleatoric_dag_referencing_absent_event_fires` | 1 CCRR | **migrates** — C8 |
| `f3_aleatoric_bounds_key_absent_and_reversed_window_fire` | 1 CCRR + 1 `EventCoordinateModel` | the CCRR one **migrates** — C9; **the invariant-4 one is untouched** |
| `f4_tempo_segment_structural_defects_fire` | 4 CCRR | **3 migrate** — C5, C4, and one fixture tripping **both C6 and C7** (`seg(2,3)` then `seg(1,2)` is out of order **and** overlapping); **the 4th stays**, a ghost-region anchor — C2 |
| `f8_structural_reference_resolution` | 5 CCRR | **all stay** — genuine invariant 10 |
| `f9_dangling_decomposition_tuplet_reference_fires` | 1 CCRR | **stays** — genuine invariant 10 |
| `f4_tempo_segment_offset_kind_is_checked_by_invariant_9` | 1 `AnchorOffsetModel` | unchanged |
| `f5_overlapping_metric_regions_caught_via_tempo_conversion` | 2 `RegionExtents` | unchanged |
| `f6_time_signature_uniqueness_and_namespaces` | 3 `UniqueIdentifiers` | unchanged |
| `f10_tuplet_ratio_inconsistent_with_member_notation_fires` | 2 `TupletSum` | unchanged |

**21 preserved: 12 CCRR (5 migrate, 7 stay) + 9 other-invariant (1 in the `f3`
row above, 8 in the four rows beneath it).**

#### 11b — `accidental_compatibility_tests` (`invariants.rs:4594`–`:4663`), 4 observations in 3 tests

**A selector-only migration is insufficient here, and would leave the module
worse than it is now.** **Three** of the four observations identify the rule
through `v.witness.contains("accidental-modification-compatibility")` — **the
exact suffix pin 8a deletes**; the fourth is a `fires()` call, which observes
only selector non-emptiness.

| Test | Observations | Selector-only migration |
|---|---|---|
| `cmn_chromatic_accidental_in_edo_31_fires` | 2 — one `fires()`, one `witness.contains` | **Fails** — the witness no longer contains the label |
| `cmn_chromatic_accidental_in_cmn_12_does_not_fire` | 1 | **Passes vacuously** — `.all(|v| !v.witness.contains(…))` is true of *every* violation once the suffix is gone, including a real one |
| `a_score_with_no_accidental_extensions_never_fires_this_check` | 1 | **Passes vacuously**, same predicate |

**Pinned replacement predicates:**

- **Both negatives:** `check_requirement(&s, "req:tuning:accidental-modification-compatibility").is_empty()`.
  The label moves from the witness text to the selector, which is where pin 2
  put it.
- **The positive:** that same call is **non-empty**, and its witness is checked
  by **one combined assertion** — *ends with* `interval algebra` **and** *contains
  no* `req:`. **One assertion, not two**, so the module's observation count stays
  at **4** and pin 11c's inventory stays at 25; splitting it would make the
  inventory 26 and gate 13's figure wrong.

  *These are **combined witness predicates**, not an exact witness: the full
  string carries `Debug`-formatted ids that a fixture change would move. The
  label is now the `Display` prefix; asserting it inside the witness is what pin
  8a removed.*

**The two negatives are the sharper half.** A migration repairing only the loudly
failing test leaves two tests green, weaker than before, and reporting nothing.

#### 11c — the standing rule

**All 25 observations preserved. Exactly 9 migrated** (5 in 11a, 4 in 11b),
**and the 4 in 11b have their predicates replaced, not merely their selector**.
**Exactly 16 unchanged.** None deleted, none softened — *the failure mode this
pin prevents is an execution that makes the suite green by removing or hollowing
the tests that caught the change.*

**Reversed aleatoric bounds keep asserting invariant 4.** Correctly tagged before
this rung and not a rider; sweeping them into the requirement arm would introduce
the defect S29 removes.

### Pin 12 — this contract's own lifecycle

- **On ratification:** the status block reads exactly
  `STATUS: RATIFIED; DISPATCHED.`, and the **frozen-pins statement** is added,
  word for word as P13-S16, S27 and S26 carry it.
- **On landing:** exactly `STATUS: LANDED by this commit.` — **no hash**; a
  commit cannot carry its own id, and if one is wanted it arrives by a later
  administrative amendment.
- **On landing**, all review-round blocks above §0 are marked a **dated
  historical record**.

Touch row 9 authorizes these edits; **this pin is what requires them.**

### Pin 13 — the two permanent history additions, with pinned content

Touch rows 5 and 8 license a `core_spec.tex` Revision History row and an S29
ledger append. **Neither was required by any pin, and neither gate could observe
one**: in the **ratified input** — the tree as it stands at ratification — the
Revision History chapter already exists and the S29 row is already present and
open, so a gate asserting their presence passes on the baseline artifact.
*Scoped to the ratified input deliberately: after execution the S29 row is
resolved, so a clause saying it "exists today, open" would be false at landing.* **This is the
third occurrence of the touch-row-without-a-pin class in this rung's family**
(S26's Revision History row, S26's own status block, and now these two), which is
why it is pinned rather than tidied.

**Revision History row — pinned source, verbatim.** Appended immediately before
`\bottomrule` in `core_spec.tex`'s Revision History `longtable`:

```latex
  \today & \sectionsc{Graph Invariants}, \sectionsc{Time and Duration} &
  \textbf{P13-S29: the violation tag stops multiplexing.} Graph invariant~10
  reported Chapter~3 and Chapter~4 failures under its own number, through a
  public API. The violation type becomes \texttt{WellFormednessViolation}
  carrying a two-armed \texttt{ViolationKind}: an invariant arm and a
  requirement arm naming a \texttt{req:} label. Invariant~10 keeps only
  reference resolution; tempo segment shape, tempo segment order, aleatoric
  reference locality and accidental modification expressibility now report
  under their own requirements. \sectionsc{Time and Duration} gains
  Requirement~\ref{req:time:tempo-segment-shape}, stating the enforced
  shape/\texttt{end\_tempo} compatibility without determining whether a
  constant segment records an \texttt{end\_tempo} at all. The enumeration is
  unchanged.
  \\
```

**The delimiter placement is pinned and was wrong in revision I.** The existing
final row already terminates with `\\` immediately before `\bottomrule`
(`core_spec.tex:17031`), so a block **beginning** with `\\` yields two separators
before the new row and none after it. The new row carries **no leading `\\` and a
terminating one**, and is inserted between that existing `\\` and `\bottomrule`.

**Extraction:** the contiguous run of **added** lines in
`git diff --cached -- spec/core_spec.tex` that lies inside the Revision History
chapter. Whitespace-collapsed, it **equals** the block above, likewise collapsed.

**S29 ledger append — pinned text, and an exact whole-line reconstruction.** The
ledger's S29 entry is a **single-line markdown table row**, so an append shows in
the diff as one removed line and one added line; **the added line is not the
append**. `APPEND` is exactly the following, as **raw source** — a fenced block,
not a blockquote, because blockquote `>` prefixes are part of the raw text and
whitespace-collapsing does not remove them, while the ledger row contains none:

```
**RESOLVED 2026-08-11 by `spec/CONTRACT_P13S29_VIOLATION_KIND.md`, disposition (b) type-neutral.** `InvariantViolation` becomes `WellFormednessViolation` with `kind: ViolationKind`, whose two arms are `Invariant(GraphInvariant)` and `Requirement(&'static str)`. `check_invariants` stays **comprehensive** — every broad caller keeps its rule coverage — while `check_invariant` deliberately narrows to the invariant arm and a symmetric `check_requirement` is added. `req:time:tempo-segment-shape` is minted for the one rider that had no label, stating enforced compatibility without resolving P13-S8. **`GraphInvariant` did not move: 21 variants, unchanged.**
```

**Extraction and comparison, exact — and the terminal delimiter is the trap.**
The S29 line **ends with the table's terminal `|`**, so the append goes *inside*
the row, before that delimiter; concatenating after the whole line would write
outside the table. Let `removed` and `added` be the S29 row's removed and added
lines. Then, whitespace-collapsed, with `strip(x)` removing a trailing `|` and
surrounding space:

**`strip(added) == strip(removed) + " " + APPEND`**, **and** `added` ends with
`|`.

*Revision I compared `added == removed + " " + APPEND`, which no correct edit can
satisfy. This compares the whole logical record without pinning the pre-existing
cell, and still rejects a truncated append — which carries the distinguishing
token and fails the equality.*

**Both gates observe the record, not a token — by different means, because the
two additions differ in shape. Neither is "the whole normalised added record":
that phrase described only gate 15 and was never true of gate 10.** Gate 15 slices the **added Revision History
row** out of `git diff --cached`'s additions and compares it, normalised, against
pin 13's block: a new table row is genuinely a run of added lines. **Gate 10
cannot do that**, because the S29 entry is one line and an append rewrites it —
so it uses the **removed-plus-added reconstruction** above, not an added-lines
check. *Checking for `req:time:tempo-segment-shape` or
`WellFormednessViolation` anywhere in the additions passes on a truncated or
misplaced record — the token is present, the record is not. Both gates therefore
compare the **whole logical record** — gate 15 the added row, gate 10 the
reconstruction — and a shortened addition fails either.*


---

## §2. Touch table

| # | Path | Why |
|---|---|---|
| 1 | `crates/epiphany-core/src/invariants.rs` | pins 1, 1a, **1b (its source guard lands here)**, 2, 3a (its guard lands here), 4, 5, 6, 6a, 7, 8, 8a, 9, 10, 11. **The transformation, not a re-tag count:** `check_cross_cutting_refs`' emission is unchanged; `check_tempo_maps`' single `flag` closure **splits by condition** into an invariant arm (C2, C3) and a requirement arm (C4–C7); `check_aleatoric_models`' two `CrossCuttingRefsResolve` pushes become requirement-arm (C8, C9) while its `EventCoordinateModel` push is untouched; `check_accidental_modification_compatibility`'s becomes requirement-arm (C10) and loses its witness suffix |
| 2 | `crates/epiphany-core/src/lib.rs` | re-exports: renamed type, `ViolationKind`, `check_requirement` |
| 3 | `crates/epiphany-core/src/generators.rs` | `.invariant` → `.kind` (two sites) |
| 4 | `crates/epiphany-core/tests/score_graph.rs` | `.invariant` → `.kind` (two sites) |
| 4a | `crates/epiphany-core/README.md` | pin 9 — the exposed type name, and the already-stale invariant count |
| 4b | `crates/epiphany-core/DECISIONS.md` | pin 9 — a supersession note; **the original decision is not rewritten** |
| 4c | `crates/epiphany-core/tests/public_surface.rs` | pin 1b's integration test, **new file** — the only **explicit three-name** observer of touch row 2's root re-export. *Not the only observer of that re-export: `tests/score_graph.rs` is an integration test too, and once its `.invariant` comparisons migrate it must resolve `ViolationKind` from the root, `invariants` being private* |
| 5 | `spec/core_spec.tex` | pin 3's requirement; Revision History row |
| 6 | `spec/core_spec.pdf` | tracked build product of row 5 |
| 7 | `crates/epiphany-testkit/tests/requirement_labels.rs` | pin 3's counts; pin 3's deletion of the temporary allowlist row |
| 8 | `spec/PASS13_CANDIDATES.md` | S29 status append |
| 9 | `spec/CONTRACT_P13S29_VIOLATION_KIND.md` | pin 12's transitions and historical marking |
| 10 | `spec/EVIDENCE_P13S29_EXECUTION.md` | gate evidence destination |

**Deliberately absent:** `epiphany-ops/src/reduce.rs` — its two `GraphInvariant`
uses are test-scope and pass a *variant* as an argument, and **`reduce.rs`
contains zero `.invariant` field accesses**, so it compiles unchanged.
**Re-verified before ratification, at revision K** — *not "at execution": pin 12
dispatches at ratification, so a row added "before dispatch" after an
execution-time discovery is temporally impossible. If this turns out wrong
during execution, **execution stops and an amendment adds the row**, like any
other frozen-pin defect.*

---

## §3. Mutation plan

Every guard is verified by re-introducing the defect and **observing** it. **A
compile error observed nothing.** Restore by hand-editing, never with git.

**Radii are derived against every observer this rung pins, in two classes.**
*Behavioural and classification* observers are pins 3a, 10 and 11; *structural*
observers — reading declarations rather than running the checker — are pins 1b,
2 and 8. **Not guessed, and not against pin 10 alone.** The derivation is given per row so
review can check it rather than take it. *Deriving against pin 10 alone was
revision C's finding 3; repeating the phrase would repeat the omission.*

**This rung has no passing-outcome mutation.** Every mutation below must produce
a failure, so gate 7 applies uniformly.

**Four cells were measured, not derived, and the difference matters.** Most radii
here follow from tests **this contract pins**. **M2a's and M17·C1–C3's** do not:
they reach pre-existing tests the contract did not write and whose fixtures it
does not control, so each was run against a disposable implementation and its
observed set pinned. *Every one of the four differed from what static reading
predicted — M2a by 10 false positives and 17 omissions, M17·C1 by naming 7 where
18 fail and including a test (`f4`) that belongs to C2, M17·C2 and C3 by
predicting legacy observers that measurement put at 1 and 0.*

**Static derivation was attempted and failed.** Revision I enumerated the 19
`!fires(…)` assertions and grouped them by score binding, yielding 13 tests.
**Measured against a disposable implementation — `check_invariant` with its
payload filter removed, full workspace, `--no-fail-fast` — the **pre-existing**
set is 20, and against revision I's derived 13 the two are almost disjoint: 3 in
common, 10 false positives, 17 missed.** *M2a's full radius is those 20 **plus**
`invariant_selector_discriminates_its_payload`, the test this rung adds; 20 is
the legacy figure, not the cell.*

Three reasons the static method could not work, each confirmed by the run:

- **Binding identity is not state identity.** Tests mutate the score between
  assertions — `invariants.rs:3216` removes the overlap *before* its negative
  assertion; `invariants.rs:4457` installs the tempo map only *after* one. A
  positive assertion proves nothing about the state the negative selector sees.
- **`m35` was not one binding group at all** — its negative uses `lone` while its
  positive builds a separate score (`invariants.rs:4939`), contradicting the
  criterion outright.
- **`!fires(…)` was never the whole surface.** The mutation changes *every*
  `check_invariant` consumer: direct zero-result assertions (`invariants.rs:5158`,
  the S2/S7/S8 matrix cells), and behavioural consumers — **`generators.rs:953`,
  where `shrink` uses the selector to choose candidates** — plus test-scope
  consumers in `reduce.rs:16586`.

**The measured 20 are pinned in M2a's cell, alongside the new test.** *One caveat, stated because the
proxy is not the post-rung code: it was measured against today's single-arm type,
so it over-approximates for any test observing a **rider** violation through the
selector — after this rung those move to the requirement arm and `Invariant(_)`
will not return them. **None of the 20 is a rider test** (all are
`g3b_measure20_tests`, measure/meter territory), so the set is expected to hold;
execution confirms it and any difference is a finding.*

| M | Mutation (complete, applicable) | Must fail — exhaustively, and why |
|---|---|---|
| M1·C4…C10 | Re-tag **one** rider emission back to `ViolationKind::Invariant(CrossCuttingRefsResolve)` — **seven separate mutations, one per requirement condition** | That condition's pin-10 test, **plus its legacy observer, plus the requirement discriminator where it uses that condition**: `requirement_selector_discriminates_its_payload` is built from **C5 and C8**, so **M1·C5 and M1·C8 also fail it**. Derived per condition: **C4, C5** also fail `f4_tempo_segment_structural_defects_fire`; **C8** also fails `f3_aleatoric_dag_referencing_absent_event_fires` **and** `mixed_fixture_splits_by_arm` (C8 is in the pinned mixed fixture); **C9** also fails `f3_aleatoric_bounds_key_absent_and_reversed_window_fire`; **C10** also fails the migrated `cmn_chromatic_accidental_in_edo_31_fires` **and** `display_renders_each_arm_exactly`, whose requirement side uses a real accidental violation. **C6 and C7 fail their pin-10 test alone**: the legacy `f4` assertion is one fixture tripping *both*, so re-tagging one leaves the other still reporting the same label and the legacy assertion still passes |
| M2 | `check_invariant` matches both arms | **All seven** C4–C10 tests, via their absence assertion, **and** `mixed_fixture_splits_by_arm`, which pin 10 now requires to exercise **both selectors** and not only the aggregate. *Not* C1–C3, which assert presence and are unaffected by widening |
| M2a | `check_invariant` matches `ViolationKind::Invariant(_)`, ignoring `which` — **the wildcard replaces the payload match; the requirement arm is untouched** | `invariant_selector_discriminates_its_payload`, **and the 20 existing tests measured below**, all in `invariants::g3b_measure20_tests`: `agreement_and_boundary_hold_together`, `m35_pickup_successor_boundary_flags_wrong_distance`, `m37_incomparable_abstains`, `m38_pickup_first_measure_boundary_clause_not_flagged`, `m39_unresolvable_reference_is_invariant_10_only`, `matrix_a1_none_time_signature_inapplicable`, `matrix_a3_vacuous_agreement`, `matrix_b2_governing_signature_unresolving_delegated`, `matrix_b3_vacuous_boundary`, `matrix_s1_wallclock_measures_wallclock_meter_changes`, `matrix_s2_agreement_a4`, `matrix_s2_boundary_b4`, `matrix_s4_measure_same_id_end_end_decides`, `matrix_s5_measure_distinct_ids_start_zero`, `matrix_s6_event_same_id_live_event_decides`, `matrix_s7_agreement_a4`, `matrix_s7_boundary_b4`, `matrix_s8_agreement_a4`, `matrix_s8_boundary_b4`, `matrix_s9_heterogeneous_measure_anchors` |
| M3a | `check_requirement` matches `ViolationKind::Requirement(_)`, ignoring `label` | `requirement_selector_discriminates_its_payload` **alone**. Every other requirement-arm fixture presents its selector with **one** label, so a wildcard returns the same set as an exact match. *Not M6's test either — but not for the reason an earlier draft gave: M6 is a **separate** mutation, so under M3a alone C6 and C7 still carry the same label, and one fixture with one label cannot distinguish wildcard from exact* |
| M3 | `check_requirement`'s predicate is **replaced** — not supplemented — by `matches!(v.kind, ViolationKind::Invariant(_))`, matching **every** invariant variant and **no** requirement label | **All seven** C4–C10 tests via their retrieval assertion; `mixed_fixture_splits_by_arm` under pin 10's strengthened form; **and every migrated positive legacy observer.** Counted as **test functions, not observations**, the legacy set is exactly four: `f3_aleatoric_dag_referencing_absent_event_fires`, `f3_aleatoric_bounds_key_absent_and_reversed_window_fire`, `f4_tempo_segment_structural_defects_fire`, `cmn_chromatic_accidental_in_edo_31_fires`; **and `requirement_selector_discriminates_its_payload`**, whose exact projections both go empty. *The two migrated negatives assert emptiness and **pass** under M3 — they are not in this radius* |
| M4 | Render the requirement arm with the invariant form | `display_renders_each_arm_exactly` |
| M5 | Render the invariant arm with the requirement form | `display_renders_each_arm_exactly` |
| M6 | Give C6's emission the label `req:time:tempo-segment-shape` | `tempo_out_of_order_reports_order` **alone**, via **two** of its three assertions, and the outcome is pinned precisely because the masking makes it counter-intuitive: **`check_requirement(score, "req:time:tempo-segment-order")` is NOT empty** — C7 still emits under that label from the same fixture — but **the expected C6 pair `(Requirement(order), "tempo segments are out of start order")` is absent** while C7's pair remains, so both the aggregate and the selector assertions fail on the *pair*. **The third — targeted invariant-10 absence — still passes**, the kind remaining a `Requirement`; it is a positive control here, not a detector. *An earlier draft said the selector "returns nothing", contradicting the very masking that motivated pairs* |
| M7 | Add a 22nd variant **completely, and only these four edits**: the `GraphInvariant::Probe22` variant, a `number() => 22` arm, an `all()` entry **with its array length raised**, and a `violating_score` arm returning an unmodified valid score. **No `shrink` edit** — `shrink(score, inv)` has no per-invariant `match` and needs none to compile | **Seven tests**, because `Probe22` enters `all()` with a fixture that violates nothing: `graph_invariant_all_is_unchanged`; `m40_check_invariants_dispatches_invariant_20`; `every_invariant_has_a_negative_generator`; `every_invariant_shrinks_to_a_small_witness`; `shrink_is_idempotent`; `negative_generators_are_reasonably_targeted`; `full_invariant_sweep_via_public_api`. *An earlier draft named two and invented a `shrink` step* |
| M7a | **Order only:** swap the `all()` entries for `TupletSum` and `TiePairing`, leaving membership and `number()` untouched | `graph_invariant_all_is_unchanged` **alone** — length is 21 and membership identical, so nothing else notices |
| M7b | **Membership only:** replace `all()`'s `PitchIdUnique` entry with a second `TupletSum`, length still 21 | `graph_invariant_all_is_unchanged` **alone**. *Revision L also named `every_invariant_has_a_negative_generator`; that is false — it iterates `all()`, so it simply visits `TupletSum` twice and never asks for `PitchIdUnique`. A test driven by the mutated list cannot detect an omission from that list* |
| M7c | **`number()` only:** swap the arms for `BarlineGroupSameRegion` and `MeasureMeterConsistency` (19 ⇄ 20), leaving `all()` untouched | `graph_invariant_all_is_unchanged` **alone** — `all()` is identical and only the mapping moves |
| M7d | Declare a complete 22nd variant `GraphInvariant::Probe22` with a `number() => 22` arm **and a `violating_score` arm returning an unchanged valid score**, while **omitting it from `all()`** | `graph_invariant_all_is_unchanged`'s **declaration inventory alone** — `all()` is unchanged, so the sequence check passes and every generator test still enumerates 21. **The `violating_score` arm is required for the mutation to compile at all:** that function matches exhaustively on `GraphInvariant` (`generators.rs:502`), so a variant without an arm is a compile error, which observes nothing |
| M17·C1 | Re-tag `check_cross_cutting_refs`' emission to `ViolationKind::Requirement("req:time:tempo-segment-order")` | **Measured, 18 existing tests:** `full_invariant_sweep_via_public_api`, `every_invariant_has_a_negative_generator`, `every_invariant_shrinks_to_a_small_witness`, `negative_generators_are_reasonably_targeted`, `shrink_is_idempotent`, `m39_unresolvable_reference_is_invariant_10_only`, `matrix_b2_governing_signature_unresolving_delegated`, `inv10_resolves_annotation_layer_and_tuplet_parent`, `inv10_resolves_event_internal_references`, `inv10_resolves_graphic_object_references`, `inv10_flags_dangling_staff_instrument`, `inv10_flags_unresolved_time_signature_reference`, `f8_structural_reference_resolution`, `f9_dangling_decomposition_tuplet_reference_fires`, `inv10_flags_dangling_marker_lyric_and_gesture_refs`, `inv10_flags_dangling_repeat_kind_and_volta_anchors`, `inv10_flags_dangling_spanner_anchor`, `inv10_flags_dangling_sub_beam_event` — **plus** the new `cross_cutting_refs_stay_invariant_ten` and `invariant_selector_discriminates_its_payload`. *Revision L guessed 7, named `f4`, and missed the four generator tests entirely: `violating_score(CrossCuttingRefsResolve)` stops violating it, so the whole generator battery fails* |
| M17·C2 | Re-tag the tempo segment **`start`** anchor condition to `ViolationKind::Requirement("req:time:tempo-segment-order")` — **the label is pinned, since a different one changes which `check_requirement` call retrieves it** | **Measured: exactly one existing test**, `f4_tempo_segment_structural_defects_fire` — its ghost-region assertion, which is C2's and **not C1's** — **plus** the new `tempo_start_anchor_stays_invariant_ten` and `mixed_fixture_splits_by_arm` |
| M17·C3 | Re-tag the tempo segment **`end`** anchor condition to `ViolationKind::Requirement("req:time:tempo-segment-order")`, label pinned as above | **Measured: no existing test** — nothing in the tree carries a dangling tempo *end* anchor. Its radius is the new `tempo_end_anchor_stays_invariant_ten` **alone** |
| M20 | Remove `Eq` from `WellFormednessViolation`'s derives | `violation_types_declare_their_pinned_derives`, **assertion 1 alone**. *`Eq` is chosen because dropping it still compiles — `BTreeSet` needs `Ord` on `ViolationKind`, not `Eq` on the struct — so the guard is the only observer* |
| M20a | Remove `Hash` from `ViolationKind`'s derives | that test, **assertion 2 alone**. *`Hash` compiles without it; `Ord` would not, and a compile error observes nothing* |
| M20b | Delete the two-arm sentence from the module header | that test, **assertion 3 alone** |
| M20c | Delete `WellFormednessViolation`'s rustdoc | that test, **assertion 4 alone** |
| M20d | Delete `ViolationKind`'s rustdoc | that test, **assertion 5 alone** |
| M20e | Remove `ViolationKind` from `public_surface.rs`'s `use` line, dropping the `matches!` line with it so it still compiles | that test, **assertion 6 alone** |
| M20f | Add to `public_surface.rs`, keeping all three imports: `let _ = check_requirement(&epiphany_core::generators::valid_score(1), "req:time:tempo-segment-shape");` — **fully qualified, because the file imports only the three pinned names and `valid_score` lives at `epiphany_core::generators`** | that test, **assertion 6 alone**. **What it signs is the file-content prohibition, not a radius change:** this call discards its result, so it would *not* by itself put the test in M3's or M9's radius. The prohibition exists because a call whose result were **asserted** would, and assertion 6 is what keeps the file type-level so that never arises |
| M20g | Add `Hash` to `WellFormednessViolation`'s derives | that test, **assertion 1 alone** — *and it is the discriminator: a guard that searches for the pinned traits rather than comparing the line accepts an added one* |
| M20h | Append to `ViolationKind`'s rustdoc the line `/// Both arms are exhaustive over what the checker reports.` | that test, **assertion 5 alone** |
| M20i | **Reorder** `ViolationKind`'s derive list to `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]` | that test, **assertion 2 alone**. *Reordering rather than adding, because every trait a simple field-less-payload enum can usefully derive is already there; a needle check for each trait passes on a reordering, and only equality fails* |
| M20j | Append to the module header's pinned paragraph the line `//! The arms are disjoint by construction.` | that test, **assertion 3 alone** |
| M20k | Append to `WellFormednessViolation`'s rustdoc the line `/// The witness is human-readable and not parsed.` | that test, **assertion 4 alone** |
| M18 | Add a **complete, compiling** third arm to `ViolationKind`: `Deferred(GraphInvariant)`, a `Display` match arm rendering it, and **no emitter anywhere** | `violation_kind_has_exactly_two_arms` **alone** — nothing emits it, so no classification, selector or `Display` test can observe it. *That is precisely why the arm count needs its own guard, and why this row must exist in §3: gate 7 covers §3's mutations, so a mutation named only in a pin is a mutation nothing runs* |
| M8 | Delete pin 3's requirement from `core_spec.tex` | `every_requirement_block_has_one_label`, `requirement_labels_follow_the_grammar`, `requirement_labels_are_unique_across_the_suite` — the three count-bearing tests — **and** `every_requirement_citation_is_defined`, since this contract cites the label. **Also** `tempo_constant_mismatch_reports_shape` and `tempo_nonconstant_missing_end_reports_shape`? **No** — the Rust label string is independent of the `.tex` block. **And** pin 3a's prose guard, whose slice anchor disappears |
| M9 | Narrow `check_invariants` to the `Invariant` arm | **Every observer of a requirement-arm violation, because pin 6a makes both selectors projections of it:** all seven C4–C10 tests; `mixed_fixture_splits_by_arm`; `display_renders_each_arm_exactly`, whose requirement side needs a real violation to exist; and the **four migrated legacy test functions** — `f3_aleatoric_dag_referencing_absent_event_fires`, `f3_aleatoric_bounds_key_absent_and_reversed_window_fire`, `f4_tempo_segment_structural_defects_fire`, `cmn_chromatic_accidental_in_edo_31_fires`; **and `requirement_selector_discriminates_its_payload`**. **The two migrated negatives are NOT in this radius:** M9 makes `check_requirement` return nothing, so their emptiness assertions **pass vacuously**. *An earlier draft had this backwards and counted them as failures — the same vacuity the migration exists to remove, asserted as if it were detection.* *Widest radius in the rung, and the measure of what pin 4 protects.* **`comprehensive_check_retains_both_arms` is NOT in this radius: revision B deleted it, and naming it here was a live one-hop error, not a historical quotation** |
| M10 | Restore the witness suffix `" (req:tuning:accidental-modification-compatibility)"` | `display_renders_each_arm_exactly` — the requirement side, whose expected string is exact and whose fixture is a real violation — **the migrated `cmn_chromatic_accidental_in_edo_31_fires`**, whose pinned witness predicate forbids `req:`, **and `accidental_incompatible_reports_tuning_requirement`**, whose C10 pair assertion carries the same predicate. *Earlier drafts named one, then two; restoring the suffix changes C10's witness, so every observer of that witness fails* |
| M11 | Retag reversed aleatoric bounds from `EventCoordinateModel` to the locality requirement | `reversed_aleatoric_bounds_stay_invariant_four`; **`display_renders_each_arm_exactly`, whose invariant-side fixture is that violation**; the legacy `f3_aleatoric_bounds_key_absent_and_reversed_window_fire`, whose invariant-4 assertion pin 11a leaves untouched; **and `invariant_selector_discriminates_its_payload`, whose invariant-4 half is that very violation and disappears** |
| M12pre | Delete pin 3's opening sentence — `A tempo segment's \texttt{shape} and its \texttt{end\_tempo} \MUST{} be compatible.` | `tempo_segment_shape_requirement_states_its_clauses_and_stays_s8_neutral`, **its equality** |
| M12 | Replace clause 2 with `If \texttt{shape} is \texttt{Constant}, \texttt{end\_tempo} \MUST{} equal \texttt{start\_tempo}.` — dropping *is present* | that test, **its equality** |
| M13 | Delete clause 3 — `If \texttt{shape} is \texttt{Linear}, \texttt{Exponential} or \texttt{Curve}, \texttt{end\_tempo} \MUST{} be present.` | that test, **its equality** |
| M13post | Delete only `This requirement states the compatibility that is enforced.`, keeping the sentence after it | that test, **its equality** |
| M13neutral | Delete only `It does not determine whether a constant segment records an \texttt{end\_tempo} at all.`, keeping the sentence before it | that test, **its equality**. *This is the sentence that keeps the requirement S8-neutral in prose. A mutation deleting the whole paragraph cannot sign it, because a guard checking only the first sentence would fail that too* |
| M14a | Append to the block: `A \texttt{Constant} segment's \texttt{end\_tempo} is canonically absent.` | that test, **its equality** |
| M14b | Append: `A \texttt{Constant} segment's \texttt{end\_tempo} is normalized to absent.` | that test, **its equality** |
| M14c | Append: `The preferred spelling omits \texttt{end\_tempo}.` | that test, **its equality** |
| M14d | Append: `Canonically, a \texttt{Constant} segment omits \texttt{end\_tempo}.` | that test's equality |
| M14e | Append: `A \texttt{Constant} segment's \texttt{end\_tempo} \MUST{} be absent.` — **stem-free, and it resolves P13-S8** | that test's equality. **This is the mutation the stem inventory could not catch**, and the reason pin 3a asserts equality: it uses no forbidden stem, states no preference, and settles S8 outright |
| M15a | Extend pin 3a's slice end to the **second** `\end{requirement}` | `tempo_segment_shape_requirement_states_its_clauses_and_stays_s8_neutral`, **its equality** — the slice is longer than pin 3's source |
| M15b | Start pin 3a's slice at the **preceding** `\begin{requirement}` | the same test and assertion, from the other direction. *Equality catches over- and under-reach alike; the separate structural assertion revision F added is retired as redundant* |
| M16a | In `cmn_chromatic_accidental_in_cmn_12_does_not_fire`, change the fixture's space from `cmn-12` to `edo-31`, making the accidental **incompatible** | that test, migrated — its `check_requirement(...).is_empty()` becomes false. **This is what proves the negative is non-vacuous**; M1·C10 cannot, because this fixture emits no violation to re-tag |
| M16b | In `a_score_with_no_accidental_extensions_never_fires_this_check`, **complete and ordered**: change `let s = valid_score(301);` to `let mut s = valid_score(301);`; **keep the existing `assert!(s.tuning_context.accidental_extensions.is_empty());` where it is**; then, **after** it and **before** the selector call, insert `s.tuning_context.default_pitch_space = PitchSpaceId::new("edo-31");` and `s.tuning_context.accidental_extensions.push(fixture_extensions("cmn-accidentals", PitchSpaceModification::CmnChromatic(1)));` | that test, migrated — its `check_requirement(…).is_empty()` becomes false. **The ordering is pinned, not incidental:** inserting the extension *before* the emptiness assertion trips that precondition first and proves nothing about selector non-vacuity. Both edits are needed for incompatibility: the space must be `edo-31` *and* the accidental `CmnChromatic`, exactly as `cmn_chromatic_accidental_in_edo_31_fires` builds it |

---

## §4. Gate

1. `cargo test --workspace` — **the `CLAUDE.md` baseline plus this rung's
   net-new tests**, 0 failed, 0 ignored. **Gate 1 owns the new total.**
   `--no-fail-fast` whenever anything is failing.
2. `cargo +1.95.0 clippy --workspace --all-targets -- -D warnings` — clean.
3. `cargo +1.95.0 fmt -p epiphany-core -p epiphany-testkit --check` — clean.
   **Never `--all`.**
4. Every staged path is a §2 row; every §2 row staged or named unused.
5. `git diff --cached --check` clean.
6. **Identifier and field migration, scoped:**
   - no `InvariantViolation` identifier remains **in Rust code** (`crates/**/*.rs`).
     *This contract, the ledger and the annex quote it historically and are out of
     scope;*
   - no `.invariant` field access remains **on a `WellFormednessViolation`**.
     **`DeferredCheck.invariant` is explicitly retained** (§0.5) and must still be
     present.
   Both greps recorded verbatim.
7. Every mutation of §3 observed, with the failing assertion verbatim and the
   complete `--no-fail-fast` failure set, compared against its derived cell; any
   mismatch is a finding. Evidence into `spec/EVIDENCE_P13S29_EXECUTION.md`.
8. **`GraphInvariant::all()` re-derived and confirmed at 21**, and
   `core_spec.tex`'s count claim confirmed unchanged.
9. `cd spec && latexmk -xelatex -interaction=nonstopmode core_spec` — re-run
   until *"There were undefined references"* clears; `core_spec.pdf` rebuilt.
10. **S29's ledger append**, verified by pin 13's **removed-plus-added
    reconstruction**: `strip(added) == strip(removed) + " " + APPEND` with the
    terminal `|` restored on `added`. **Not an added-lines-only check** — that
    instruction was superseded, and an added line here is the *whole row*, not
    the append.
11. **The temporary allowlist row is ABSENT** from the staged
    `requirement_labels.rs`, and the two surviving rows are present and
    unchanged. *A stale temporary row is **inert** once the label exists —
    nothing but this gate would catch it.*
12. **`cargo test -p epiphany-testkit --test requirement_labels` passes with the
    temporary row absent and the label defined.** This, with gate 11, is what
    distinguishes *the citation gate passes because the label is now defined*
    from *because it is still allowlisted* — states indistinguishable from
    outside.
13. **Pin 11's inventory recorded and confirmed**, both modules: **25**
    observations preserved, exactly **9** migrated (5 in `review_fix_tests_4`, 4
    in `accidental_compatibility_tests`), exactly **16** unchanged, none deleted
    and none softened. **The two migrated negative accidental assertions are
    confirmed non-vacuous by M16a and M16b**, each of which makes its own fixture
    violate. *M1·C10 cannot do this: those fixtures emit no violation to re-tag.*
14. **Pin 12's lifecycle:** the status block reads exactly
    `STATUS: LANDED by this commit.` with **no hash**; the frozen-pins statement
    is present and shows **no hunk** in a zero-context staged diff, having been
    written at ratification; all review-round blocks above §0 are marked a dated
    historical record.
15. **Read-checked, no machine observer:** pin 3's requirement sits in Chapter 3
    immediately after `req:time:tempo-segment-order`'s `\end{requirement}`;
    **the added Revision History row is sliced from the staged diff and compared
    in normalised whole-record form** against pin 13's text — every fact it
    pins, not just its distinguishing names;
    the `/// 10.` rider note names the four labels and no longer names P13-S29 as
    a pending owner.
16. **Pin 9's prose outcomes, each read-checked against its own site** — the
    `/// 10.` rider note names the four labels and not P13-S29; the tempo,
    aleatoric and accidental headers no longer say their conditions are surfaced
    under invariant 10; the accidental header cites
    `req:tuning:accidental-modification-compatibility` symbolically rather than
    the stale `core_spec.tex:3120`; the migrated negative's message no longer
    says *"compatibility invariant"*; `review_fix_tests_4`'s module doc no longer
    calls its tempo subject an invariant; `check_invariants`' rustdoc states it
    returns both arms; `README.md`'s invariant row: **its description cell equals pin 9a's
    fenced source** after whitespace collapse; and `DECISIONS.md` carries a **supersession note** with the
    original decision **intact**. *Every one of these can be omitted with all
    other gates green.*


---

## §5. What ratification does NOT settle

- **P13-S8's canonical-form ruling.** Pin 3 states the enforced compatibility and
  nothing more; pin 3a guards that it says nothing more.
- **Whether the riders should eventually leave `check_invariants` entirely**
  (disposition (c)). Pin 4 keeps them for safety; a later rung may revisit it
  with a migration path for all ~10 callers.
- **`DeferredCheck`'s shape.** §0.5 verified it is correctly attributed today. A
  future deferred check for a non-invariant rule inherits this rung's problem.
- **The recurring citation-gate collision** — a contract proposing a label breaks
  the gate the moment it is written, now four times. That belongs in `CLAUDE.md`
  as a standing note, on its own terms, not here.

---

## §6. AMENDMENT 1 — §3's M1·C6 AND M1·C7 RADII, MID-EXECUTION

STATUS: RATIFIED; FROZEN. Execution of P13-S29 resumes at M2.

Ratified 2026-08-12 on the authority of the repository owner, the final review
round returning zero findings. The replacements are executed, not edited; a
further defect is its own amendment with its own review round.

**§3 was frozen and dispatched at `cea21cd`.** Ratification **authorizes and
freezes** this amendment; execution applies the replacements. §6.9 pins the
landed form and the order of the remaining steps.

**Scope — the complete write surface, all four parts.** A description of the
normative correction is not a description of what gets written, and an earlier
draft of this head said *"two radius cells and nothing else"* while prescribing
three further edits below it:

| | Edit | Where | Touch row |
|---|---|---|---|
| 1 | M1 row replacement | §3 (this file) | 9 |
| 2 | Preamble append | §3 (this file) | 9 |
| 3 | This amendment's own lifecycle transitions | §6 (this file) | 9 |
| 4 | Resolution/resumption append | `spec/EVIDENCE_P13S29_EXECUTION.md` | 10 |

**"Two edits, both in §3" describes edits 1 and 2 — the normative §3 correction
— and nothing else.** Edits 3 and 4 are lifecycle and record.

**One further act belongs to ratification rather than to execution:** the
ratification commit stages `spec/EVIDENCE_P13S29_EXECUTION.md` in its
**pre-amendment** state, under the same touch row 10, to serve as §6.5c-bis's
oracle. *It is listed here and not in the table because it is not an edit this
amendment makes — it is the act that gives the amendment something durable to be
checked against, and it must happen before edit 4.*

**What does not move:** no pin, no test, no fixture, no behaviour, no other §3
cell. It does not create, retire or renumber a mutation, and it **rewrites no
existing evidence** — §6.5d is the standing prohibition.

### 6.1 The defect

§3's M1 row closes:

> **C6 and C7 fail their pin-10 test alone**: the legacy `f4` assertion is one
> fixture tripping *both*, so re-tagging one leaves the other still reporting the
> same label and the legacy assertion still passes

**Measured, the radius of each is two, not one.** Transcripts: annex §5.3, §5.4.

| M | §3's cell | Measured |
|---|---|---|
| M1·C6 | `tempo_out_of_order_reports_order` | that test **and** `tempo_overlap_reports_order` |
| M1·C7 | `tempo_overlap_reports_order` | that test **and** `tempo_out_of_order_reports_order` |

### 6.2 Why — and what in the quoted sentence survives

**The sentence's claim about `f4` is correct and is retained.** `f4` is in
neither radius, for exactly the reason given: it holds one fixture tripping both
conditions and asserts on the label, which the surviving sibling keeps supplying.
Measurement confirms it.

**What the sentence omits is that pin 10's own C6 and C7 tests stand in the same
relation to each other.** Pin 10 pins four assertions per row; the fourth is
*the aggregate's invariant arm is empty*. Pin 10 also states, in terms:

> C6's natural fixture — `seg(2,3)` then `seg(1,2)` — is *also* overlapping, so
> it emits C6 **and** C7 under the same label.

Both tests are built by `two_segments(seed, (2, 3), (1, 2))`, differing only in
the seed. Re-tagging **either** condition therefore places an `Invariant`
violation in **both** fixtures, and the sibling's fourth assertion fails on it.

**The cell reasoned about the legacy observer and stopped there.** Having
established the shared-fixture property one clause earlier, it did not ask the
same question of the tests this contract itself writes.

### 6.3 What must NOT change in response

- **Not the fixtures.** The only shape isolating C6 is an end-before-start
  segment, which pin 10 **rejects** as *"unnatural and tests a shape the checker
  should never see."*
- **Not pin 10's fourth assertion.** §3's M2a derivation depends on it: it is
  what keeps the seven C4–C10 tests out of M2a's radius *structurally rather than
  by assumption*.
- **Not any other M1 cell.** C4, C5, C8, C9 and C10 were measured in the same
  pass and **matched exactly** — 2, 3, 4, 2 and 3 respectively.

### 6.4 The correction is a strengthening, not a relaxation

**The amendment adds no observer.** Pin 10's tests already supplied both — its
own `(kind, witness)` pair assertion and the sibling's invariant-arm assertion
have detected each re-tagging since the moment those tests were written. What
changes is the **accounted and required** radius: §3 credited the mutation with
one observer and would have read the second as a mismatch, so the amendment
**recognizes** an observation the contract was under-counting.

That is a strengthening because the required radius is now two: a future
implementation that silenced the sibling assertion would fail this gate, where
under the dispatched cell it would have passed.

### 6.5 The replacements — edits 1, 2 and 4 of the scope table

**Touch rows 9 and 10 already cover both files this amendment writes to**
(`CONTRACT_P13S29_VIOLATION_KIND.md`, `EVIDENCE_P13S29_EXECUTION.md`). No touch
row is added.

#### 6.5a — §3's M1 row

Replace the sentence quoted in §6.1 with:

> **C6 and C7 each fail *both* pin-10 order tests, and the legacy `f4` assertion
> fails for neither.** `f4` holds one fixture tripping both conditions and
> asserts on the label, so the surviving sibling keeps supplying it — but pin
> 10's C6 and C7 tests stand in that same relation to *each other*: both are
> `two_segments(seed, (2, 3), (1, 2))`, which pin 10 establishes emits C6 **and**
> C7. Re-tagging either puts an `Invariant` violation into both fixtures, so the
> sibling's **fourth** assertion — *the aggregate's invariant arm is empty* —
> fails alongside the mutated condition's own pair assertion. **Measured, not
> derived; the fixtures are shared by pin 10's deliberate choice and cannot be
> separated without the end-before-start segment pin 10 rejects.**

#### 6.5b — §3's preamble count

§3's preamble opens a paragraph with:

> **Four cells were measured, not derived, and the difference matters.**

**Six are now measured**, and a count left standing beside what it counts is
itself one of the ledger's recurring defects. **Do not edit that sentence** — it
is an accurate statement about what was measured *before ratification*, and
rewriting it would destroy a true historical claim to fix a staleness. Instead
**append to the end of that same paragraph**:

> **Execution measured a fifth and sixth cell, M1·C6 and M1·C7; amendment 1
> records them and authorizes their corrected radii.** They are not among the
> four because they reach only tests this contract writes — the criterion that
> sent the other four to measurement was *reaching tests the contract did not
> write*, and that criterion was wrong. **The operative property is whether a
> fixture is shared, not who authored the test.**

*This is deliberately an append, not a rewrite: the paragraph's subject is the
pre-ratification derivation and its failures, and the amendment is a later event
in the same record.*

#### 6.5c — the annex append, pinned as a source template

`spec/EVIDENCE_P13S29_EXECUTION.md` gains a new final section **§8**. **It is
pinned as source, not described by its contents** — a content description admits
a truncated or differently scoped record that still satisfies every "carries X"
clause, which is what an earlier draft of this subsection did. **Exactly one
slot, `{RATIFICATION_HASH}`**; everything else verbatim.

~~~
## §8. Amendment 1: resolution and resumption

Amendment 1 is ratified at `{RATIFICATION_HASH}` and landed by this commit. It
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
~~~

**Extraction endpoint — the same boundary rule as §6.5c-bis.** §8 runs from its
`## §8.` heading to the **last line before the next `## §` heading, or EOF if
none**. Normalize with §6.5c-bis's canonical serialization, then compare for
**equality**. Not "contains", not "mentions".

**Later evidence sections are expressly permitted and do not disturb A4.** M2
onward still needs its transcripts, and they land as **§9 and beyond**. *Had §8
been defined as "heading to EOF", the very next evidence section would have
invalidated A4 — a gate that the work it gates is guaranteed to break.*

**§5 is closed at M1 and must not be extended.** It is digest-frozen by
§6.5c-bis, so appending M2's rows to the expected-versus-observed matrix — the
obvious place for them — **would break A4(b).** §9 continues the matrix under
its own heading and carries M2 onward. *This is a consequence of freezing an
open section, and it is stated here because nothing else would warn the
executor before the gate failed.*

**`landed by this commit`, never a landing hash** — the annex lands *in* that
commit, and pin 12 already states why: *a commit cannot carry its own id.*

**§5 and §7 are NOT edited.** §5's table keeps its dispatched cells, its two ❌
marks and its *"the phase is halted at M1"* opening; §7 keeps the finding in the
tense it was found in. §6.5c-bis pins the oracle.

#### 6.5c-bis — the oracle for §5 and §7

**"Byte-identical to their pre-amendment state" needs something durable to
compare against, and the annex is untracked.** Once §8 is appended there is
nothing left to diff. Two measures, both required:

**1. The ratification act commits the pre-amendment annex.** Ratification stages
`spec/EVIDENCE_P13S29_EXECUTION.md` exactly as it stands at the halt — §1–§7,
**no §8** — under touch row 10. **That commit's blob is the named oracle**, and
A4 is checked with `git show <ratification-commit>:spec/EVIDENCE_P13S29_EXECUTION.md`.
*This is also what makes the amendment reviewable at all: an untracked evidence
file cannot be cited by a frozen document.*

**2. Digests, pinned here, as the oracle's oracle.**

**The canonical byte serialization, pinned exactly — a digest with an inferred
serialization is a digest nobody else can reproduce.** In order:

1. Read the file as bytes and **decode UTF-8**. The annex is **LF-only**; a CRLF
   copy is out of scope and will not reproduce these hashes.
2. Find section starts by `^## §\d+\.` (multiline). A slice runs from its own
   heading — **the heading line is included** — to the byte before the next such
   heading, or to EOF for the last.
3. Split the slice into lines and **right-strip each line** (trailing spaces and
   tabs).
4. **Repeat while the last retained line is empty *or* is exactly `---`: drop
   it.** *One loop, not three passes — a `---` and the blank lines around it are
   removed together however they interleave.*
5. **Join the retained lines with `\n`, and emit NO terminal newline.**
6. **SHA-256 over the UTF-8 bytes of that string.**

**Step 5 is the one that actually needs pinning.** An ordinary
`"\n".join(lines) + "\n"` — the natural way to write a file back out — yields
`53b32278…` for §5 and `eb704854…` for §7 instead of the pinned values. *The
digests reproduce only because there is no terminal newline, and nothing in an
earlier draft said so.*

*Steps 2–3 use `^## §\d+\.` and any line splitter that agrees with LF splitting;
verified that `str.splitlines()` and `split("\n")` produce identical digests for
both slices, the annex containing no form feed, vertical tab or U+2028.*

| Slice | SHA-256 | Lines |
|---|---|---|
| annex §5 | `40ce82a70339159024c69dd5e280d8bd6846efbeb1eecd5b7971a2737826f74b` | 134 |
| annex §7 | `cdd9bfd91da5091174201b1bc16aea6a5f162cc5f6d8553f6d58d7c56aacccd8` | 76 |

**The slice rule is what makes the §7 digest stable across the append.** §7
currently ends at EOF and will end at §8's heading; the rule ends it at the last
non-blank line either way, so **appending §8 must not move either digest.**
*A rule defined as "to EOF" would have made §7's digest change by construction
and the gate unfalsifiable.*

#### 6.5d — the prohibition on rewriting evidence

**An earlier draft's gate A4 would have rewritten §5's cells and removed the
halt notice. That is prohibited, and the prohibition is the reason this
subsection exists.**

§5 and §7 are the **dated record of what was expected, what was observed, and
why execution stopped.** Overwriting the expectation with the corrected one
would leave an annex in which the observed radius matches the recorded cell
everywhere — **an annex that no longer contains the reason this amendment
exists.** The mismatch *is* the evidence.

**The rule, stated generally:** an execution record is appended to, never
reconciled. A correction is a later event in the record, not a revision of an
earlier one. *This is the same discipline `PASS13_CANDIDATES.md` status cells
already carry — appended to, not rewritten — and the same reason §6.5b appends
to §3's preamble rather than editing its count.*

### 6.6 The generalization this amendment explicitly declines to make

**It would be wrong to conclude that same-label conditions always cross-talk.**
The same pass measured the other two shared-label pairs and found no cross-talk
in either:

- **C4/C5**, sharing `req:time:tempo-segment-shape`: `Constant`-with-mismatch and
  non-constant-missing-`end_tempo` are mutually exclusive shapes.
- **C8/C9**, sharing `req:time:aleatoric-reference-locality`: distinct fixtures,
  each aggregate carrying exactly one violation.

**The property that matters is fixture identity, not label identity**, and only
C6/C7 share a fixture shape.

### 6.7 The defect class, for the ledger

**The one-hop correction**, the recurring shape: a fact established in one pin
is not carried into the section that depends on it — here within a single table
cell, which cites the shared-fixture property about `f4` in one clause and
overlooks it about pin 10's own tests in the next.

§3's preamble already records that **all four** of its measured cells differed
from static reading, and measured them because they reached tests the contract
did not write. **C6 and C7 reach tests the contract *did* write — which is why
they were derived rather than measured.** Authorship of a test is not knowledge
of its fixture's reach.

### 6.8 Gate

- **A1.** §3's M1 row carries §6.5a's replacement text, and §3's preamble
  paragraph carries §6.5b's appended sentences with its opening
  *"Four cells were measured"* sentence **unedited**. The superseded sentence
  survives in the file **exactly once**, inside §6.1's blockquote, where it is a
  dated quotation of what was corrected — **not "absent from the file", which
  this amendment makes permanently unsatisfiable by quoting it.** Checked by
  stripping blockquote markers and counting: **one** occurrence, and it is
  §6.1's.
- **A2.** M1·C6 and M1·C7 each observed, each failing **exactly**
  `tempo_out_of_order_reports_order` and `tempo_overlap_reports_order`, with the
  failing assertions verbatim in the annex — and each restoring to
  `44 / 1604 / 0 / 0`.
- **A3.** **Measured against the halt checkpoint, not against `cea21cd`.** The
  ratification commit predates every pin-10 test, so "no test differs from
  `cea21cd`" is unsatisfiable by construction — the tests P13-S29 exists to add
  necessarily differ from it. The checkpoint is the working tree at the M1 halt:
  `44 suites / 1604 passed / 0 failed / 0 ignored`, annex §4. Against it:
  **no implementation, test or fixture change is attributable to amendment 1**,
  and **no staged hunk in `crates/` belongs to it.** Amendment 1's diff touches
  exactly two files, `spec/CONTRACT_P13S29_VIOLATION_KIND.md` and
  `spec/EVIDENCE_P13S29_EXECUTION.md`.
- **A4.** Two checks, both mechanical:

  **(a) The append.** The annex's §8, normalized per §6.5c, **equals** §6.5c's
  template with `{RATIFICATION_HASH}` filled — including the literal
  `landed by this commit`. Whole-section equality, not presence of topics.

  **(b) The preserved record.** Annex §5 and §7, sliced and normalized per
  §6.5c-bis, hash to `40ce82a7…f74b` and `cdd9bfd9…ccd8`, and are byte-identical
  to the same slices in `git show <ratification-commit>:spec/EVIDENCE_P13S29_EXECUTION.md`.
  Halt notice and ❌ marks intact. **This check is also what observes the
  ratification act**: if the ratification commit does not contain the
  pre-amendment annex, the `git show` does not resolve and A4(b) cannot pass.

  *A4 observes an append; it must never observe a reconciliation (§6.5d).*
- **A5.** §6's status block carries §6.9's exact ratified form, and — at landing
  — its exact landed form and the historical marking.

  **The contract's own top status has two permitted values, and A5 accepts
  whichever the landing commit makes true:**

  | Landing | Contract's top status must read |
  |---|---|
  | Amendment 1 lands **alone**, contract still mid-flight | `STATUS: RATIFIED; DISPATCHED.` — unchanged |
  | Amendment 1 lands **in the same commit as P13-S29** | pin 12's exact landed transition, `STATUS: LANDED by this commit.` |

  *An earlier draft required the first unconditionally, which **rejected
  compliance with pin 12** in the same-commit case §6.9 explicitly permits — a
  gate contradicting a lifecycle the same amendment allows.* **What A5 forbids
  in both branches is unchanged: no pin-12 transition attributable to amendment
  1 itself.** *Without A5 the scope table's edit 3 would be a prescribed edit no
  gate observes.*

### 6.9 This amendment's own lifecycle

**Mirrors pin 12's form for the contract, with the transitions an amendment
needs and a contract does not.**

- **On ratification:** §6's status block reads exactly

  `STATUS: RATIFIED; FROZEN. Execution of P13-S29 resumes at M2.`

  and the **frozen-amendment statement** is added:

  > The replacements are executed, not edited; a further defect is its own
  > amendment with its own review round.

  **This adapts P13-S26's formula and does not reproduce it word for word.**
  The precedent reads *"The amended pins are executed, not edited; a further
  defect is its own amendment with its own review round."*
  (`CONTRACT_P13S26_INVARIANT10_SURFACE.md:1656`). **Amendment 1 amends no pin**
  — it replaces two radius cells — so quoting the precedent exactly would assert
  something false about this amendment. The second clause is verbatim; the first
  substitutes *replacements* for *amended pins*. *An earlier draft claimed exact
  reuse while paraphrasing; the claim, not the wording, was the defect.*

- **Ratification resumes execution, and Amendment 1's own execution comes
  first.** The order is pinned:

  1. **Ratify** — §6's status block takes the form above; the pre-amendment
     annex is committed as §6.5c-bis's oracle.
  2. **Apply edits 1, 2 and 4** — the M1 row replacement, the §3 preamble
     append, and the annex §8 append.
  3. **Pass A1–A5.**
  4. **Resume the mutation sequence at M2.**

  *"Ratification resumes execution at M2" must not be read as skipping steps 2
  and 3: the amendment has its own execution, and M2 follows it.*

  The resumption point is **M2** — not M1.
  M1·C6 and M1·C7 were **already observed** (annex §5.3, §5.4); the amendment
  corrects the expectation they were compared against, not the observation. The
  tree is unchanged between those runs and this amendment — A3's checkpoint is
  what establishes that — so **A2 is discharged by the existing dated
  transcripts and requires no re-run.** *Re-running would produce a second,
  identical transcript and a record implying the first was doubted.*

- **On landing:** exactly

  `STATUS: LANDED by this commit.`

  — **no hash**; a commit cannot carry its own id.

- **On landing**, §6's review-round record is marked a **dated historical
  record**, in the form P13-S26 §9 uses: *§6's findings and dispositions are an
  account of what was found and decided, and state no current condition.*

- **P13-S29's own §0-and-above review blocks are NOT marked historical by this
  amendment.** Pin 12 marks them **on the contract's landing**, which has not
  happened — execution is mid-flight. *Marking them here would date a record
  whose subject is still live.*

**This amendment does not alter pin 12.** The contract's own status block still
reads `STATUS: RATIFIED; DISPATCHED.` and transitions to `LANDED` on the
contract's landing commit, which may be the same commit that lands this
amendment or a later one.
