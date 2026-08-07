# Contract — P13-S27: the reduction version gets an outside witness

**Status:** DRAFT — **BLOCKED on the format-epoch rung**
(`spec/CONTRACT_FORMAT_EPOCH_MAJOR1.md`), which is **ratified and in
implementation** but has not yet landed. Pins 1 and 3–10 are settled, internally
consistent, and ratifiable as a plan.

**Pin 2a is RESOLVED as of 2026-08-07** — from outside this rung, by that
contract's pin 8, exactly as its prohibition required. Legacy bases are refused
by container epoch, never by version arithmetic; see the resolution block under
pin 2a. This contract additionally **inherits three obligations** from that rung
(the two interim refusals it must convert to validation, M8's deferred laundering
demonstration, and pin 3c's two suspended conformance assertions) — recorded in
the same place.

**It becomes dispatchable when the format rung lands, not before.** Until then
this remains bounded analysis. Pin 2a's original prohibition stands for the
record: it was never amended into a disposition from inside this contract.

**Rung type:** **capability + API change.** No wire bytes move and no schema
major or minor changes — `BundleError` has no discriminant and no encoder
(`bundle/src/error.rs`), so a new variant is a pure Rust API change. What does
change is `Bundle::open`'s **and `Bundle::create`'s** signatures, at 57 and 32
call sites.

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
carried value via `reduction_version_for` (`bundle.rs:798`). `project.rs:936` is
a **second** production write path of the same shape.

**So production code mints a self-consistent stale document without ever calling
`open`.** An authority enforced only at read time would leave that entirely
open, which is the gap this revision closes (pin 3a, pin 4a).

> **Method note, recorded because it is this rung's own subject matter.** The
> false claim came from `grep … canonical_base | head -14`. The `textproj` hits
> were below the cut. A universal negative — "no production code *anywhere*" —
> was asserted from deliberately truncated evidence. This is the second
> instrument failure in this rung: the first searched for
> `ReductionAlgorithmVersion(` constructor calls and so could not see a path
> that *propagates* a value without constructing one. Both are the defect S27
> exists to fix, committed while scoping it: **an observation that cannot
> support the claim drawn from it.**

**Reader surface — 57 `Bundle::open(` sites across 10 files:**

| Crate | Sites |
|---|---|
| `epiphany-bundle` (`bundle.rs` 20, `fuzz.rs` 15) | 35 |
| `epiphany-testkit` (`bundle_harness.rs` 11, `roundtrip.rs` 4, `benches/bundle.rs` 2, `tests/bundle_reopen.rs` 1) | 18 |
| `epiphany-textproj` (`serialize.rs`, `project.rs`) | 2 |
| `epiphany-bundle/tests/` | 2 |

**Writer surface — 32 `Bundle::create(` sites:** `bundle.rs` 11,
`testkit/roundtrip.rs` 6, `testkit/bundle_harness.rs` 6, `textproj/project.rs` 2,
`bundle/tests/crash_recovery.rs` 2, `textproj/serialize.rs` 1,
`testkit/tests/bundle_reopen.rs` 1, `testkit/benches/bundle.rs` 1,
`bundle/tests/manifest_selection.rs` 1, `bundle/fuzz.rs` 1.

**`commit` has 57 sites** (including 2 in `epiphany-editor-core`) — pin 3's
design keeps every one of them unchanged.

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

**Pin 3a — the writer is validated, not only the reader.** §0.4 shows production
code minting a stale document without ever calling `open`. Therefore
`commit`/`commit_versioned` MUST reject a manifest whose **newly emitted or
replaced** canonical base carries a version differing from
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

**Pin 10 — the ledger.**
`spec/PASS13_CANDIDATES.md`: P13-S27 → RESOLVED, recording both rulings, the
baseline-0 decision, the writer-path correction of §0.4, and **whichever
legacy-base disposition pin 2a settles on**.

**P13-S16's row does NOT become dispatchable on this rung alone.** It moves from
"blocked on P13-S27" to "blocked on the pin-2a legacy disposition" unless 2a is
ratified and tested within this rung, in which case it opens and its row records
that its first act is bumping the authority past the baseline.

---

## §2. Touch table

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-ops/src/lib.rs` (or a new `reduction.rs`) | pins 1, 2 |
| 2 | `crates/epiphany-bundle/src/bundle.rs` | pins 3, 3a, 3b, 5, 6, 7 + 35 in-crate `open` sites + 11 in-crate `create` sites |
| 3 | `crates/epiphany-bundle/src/error.rs` | pin 4 |
| 4 | `crates/epiphany-bundle/src/ids.rs` | pin 8 |
| 5 | `crates/epiphany-bundle/src/fuzz.rs` | call sites |
| 6 | `crates/epiphany-bundle/tests/{crash_recovery,manifest_selection}.rs` | call sites |
| 7 | `crates/epiphany-testkit/src/{bundle_harness,roundtrip,generators}.rs` | call sites, real authority |
| 8 | `crates/epiphany-testkit/tests/bundle_reopen.rs`, `benches/bundle.rs` | call sites |
| 9 | `crates/epiphany-textproj/src/{serialize,project}.rs` | call sites, real authority |
| 10 | `spec/core_spec.tex` (+ `.pdf`) | pin 9 |
| 11 | `spec/PASS13_CANDIDATES.md` | pin 10 |

---

## §3. Required tests (pin 4's ruling names all four)

Named, permanent, in `epiphany-bundle`:

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

**Added 2026-08-07 with pin 2a's resolution — the inherited obligations, stated
as tests so they cannot be discharged by prose:**

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

Tests 2 and 3 must be **paired in review**: each asserts the other's error is
*not* produced. A test that only checks its own variant cannot show the two
paths are distinguishable, which is the whole point of pin 6.

Tests 6 and 2 stand in the same relation to each other.

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

**M5 — the authority is the one consulted, and by a production path.** Change
`CURRENT_REDUCTION_ALGORITHM_VERSION` to a different value. The failing test
MUST be one exercising a **production composition path** — `textproj`'s
`serialize_document` / `project` round trip — **not** a fixture using
`synthetic_for_fixture`, which by design would not move. Report which test
failed; if only fixture tests fail, pin 3b has been applied backwards and that
is a finding.

**M6 — the writer check fires.** Remove pin 3a's commit-side validation; test 5
must fail. Then narrow it to refuse *any* stale inherited base rather than only a
newly emitted or replaced one, and confirm an unrelated commit on an
already-open bundle starts failing — signing that pin 3a's scope is deliberate.

---

## §5. Gate

1. `cargo test --workspace` — full pass; report the new total and the delta with
   its cause (four tests added).
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
6a. **No production composition path uses the fixture constructor:**
   `grep -rn "synthetic_for_fixture" crates/epiphany-textproj/src/` shows matches
   **only** inside `#[cfg(test)]` modules. Report each match with its enclosing
   item.
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

---

## §7. Report requirements

1. The five mutations, each with verbatim output (M4 as a recorded prohibition).
2. The seven gate results, each with its command.
3. The staged file list and the test-count delta with its cause.
4. The four required tests by name, each passing, with tests 2 and 3 shown to
   produce *different* errors.
5. A count of call sites updated per crate, against §0.4's table — any
   discrepancy is a finding.
6. Anything contradicting this contract.
