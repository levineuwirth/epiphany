# epiphany-testkit — Decisions

Agent F's crate. The v0 decisions live in `spec/QUICKSTART.md` and the per-crate
DECISIONS files; this file records the **Phase 2** calls F makes as its mandate
broadens (`spec/PHASE2_QUICKSTART.md`, `spec/PHASE2_F_WEEK0_WORKLIST.md`).

## F0 — Where the new harnesses, benches, and the integration runner live

*Worklist item F0: "a 1-day call that everything else lands inside of." Decide
before writing F1/F3/F4/F5.*

**Decision: keep the examples + library-module pattern; do not adopt `xtask`;
put `benches/` in this crate; per-agent harnesses are library modules asserted
by `tests/` integration tests; the end-to-end integration harness is a `tests/`
integration test over a library `integration` module.**

Concretely:

- **No `xtask`, no `tests/reference-suite/`.** The QUICKSTART topology named
  both (HANDOFF §7-F), but their function is already met by per-crate
  `examples/*` fuzzers and `epiphany-testkit/examples/conformance_suite.rs`,
  which is the orchestration layer and the soak entry point. Adding `xtask`
  would introduce a second, parallel orchestration surface for no present gain;
  the workspace has one cross-cutting crate (this one) and `cargo` already
  drives every gate. Revisit only if cross-crate orchestration outgrows a single
  `examples/` runner (e.g. multi-binary pipelines that need a build graph).

- **Per-agent harnesses are library modules in `src/`**, alongside the v0
  harnesses (`roundtrip`, `convergence`, `equivocation`, `bundle_harness`,
  `negative`, `layout_stub`). Each new agent gets one module exposing
  `run_all()`-style entry points plus the granular asserts:
  - **H** → [`crate::prepass_harness`] (spelling/decomposition determinism,
    eligibility-taxonomy coverage, `RespellPitch` precedence, non-vacuity),
    driving the corpus in [`crate::corpus`]. **Live now** (H landed).
  - **I** → `engrave_harness` (hard-constraint validation against *declared* IR
    constraints + class-specific collision rules, provenance survival, SVG
    XML-validity, golden machine-acceptance snapshot). *Skeleton when I starts.*
  - **K** → `migration_harness` (deterministic + equivalence-preserving v0→v1
    migration, payload-schema completeness). *Skeleton when K starts.*
  - **J** → `wire_harness` (cross-impl decoder, canonicalization tests) +
    the wire-format fuzzer as an `examples/` soak target. *Skeleton when J
    starts.*

- **Each harness is asserted by a `tests/` integration test** (the same shape as
  `tests/acceptance.rs`) so it is a discrete `cargo test` target and a discrete
  CI job, and is **also** exercised at scale by `examples/conformance_suite.rs`
  for the nightly soak. The H harness lands as `tests/prepass.rs` + a
  `[prepass-harness]` stage in the conformance suite.

- **`benches/` lives in this crate** (not per-crate). The Chapter-10 budgets are
  workspace-level, and the marquee bench (the reducer's — at the time `O(n²)` —
  `canonical_reduction_order` at 10K+ envelopes, worklist F1) drives
  `epiphany-ops` *through* the testkit's envelope generators — exactly what this
  crate already does. A bench that lived in `epiphany-ops` could not reuse the
  generators without a dev-dependency cycle. Uses `criterion`; thresholds are
  written in the bench, with known-pending scale points marked `xfail` per F1.

- **The end-to-end integration harness (F5) is a `tests/` integration test** over
  a library `integration` module with documented stub swap-points, so real H/I/K/J
  stages replace stubs in place as they land, and the byte-identity assertion is
  wired from day one (trivially true on stubs, meaningful once stages are real).

**Why a library module + integration test, not a bare integration test:** the
harness logic (asserts, fingerprinting, the corpus) must be callable from both
the unit-budget `tests/` target *and* the at-scale `examples/conformance_suite`.
Bare `tests/` code is not importable across targets; library modules are. This
mirrors how v0's `convergence`/`roundtrip` modules are shared between
`tests/acceptance.rs` and the conformance example.

**Unblocks:** F1 (benches), F3 (corpus + taxonomy harness — done), F4 (per-agent
harness skeletons — H done), F5 (integration skeleton).

## F1 — The Chapter 10 budget benches (`benches/reduction.rs`, `benches/bundle.rs`)

*Worklist item F1: a criterion bench asserting Chapter-10 budgets at documented
scale points, known-pending points marked xfail with the numeric budget written
in the bench. Home per F0: this crate's `benches/`.*

**Criterion version: `0.5` (locks to 0.5.1), default features off plus
`cargo_bench_support`.** The 0.5 line's MSRV (1.70) fits the workspace's pinned
`rust-version = "1.77"`; criterion 0.6+ requires 1.80. Disabling default
features drops the plotting stack (`plotters`) and `rayon` — dead weight for
budget gates — while `cargo_bench_support` keeps plain `cargo bench` as the
entry point. Two transitive pins in `Cargo.lock` keep the tree MSRV-clean,
because the resolver (v2) is not MSRV-aware: `clap 4.5.53` (MSRV 1.74; 4.6
requires 1.85) and `half 2.4.1` (MSRV 1.70; 2.5+ requires 1.81). Re-check those
two if `cargo update` touches the criterion tree.

**The gate mechanism.** Criterion measures but never asserts, so each bench is
`harness = false` and its `main()` runs criterion's measurements first, then
the budget gate in [`crate::budget`] — a library module (the F0 pattern: shared
logic lives in `src/`, and it deliberately uses no criterion types, so the
library itself takes no new dependency). Every budget row carries an
`Expectation` written in the bench source next to its numeric threshold:

- `Pass` — must hold today; a miss exits nonzero (the CI tripwire).
- `Xfail(reason)` — documented known-pending miss; the reason names the defect
  and owner. A miss prints an expected-failure line; a **pass** prints an XPASS
  promotion notice, so a stale marking is loud in both directions.

Skipped rows (the 50K point in quick mode) print an explicit `skip` line, never
silence. Under `cargo test --benches` (criterion test mode, detected by the
absence of the `--bench` flag `cargo bench` passes) the measurements run once
and the gate is skipped — the gate belongs to `cargo bench`.

**Sampling calibration (a documented deviation from Chapter 10).** The
conformance methodology — p99 over ≥1000 iterations on the reference hardware
profile — is the reference suite's job, not this gate's: 1000 iterations of the
(pre-fix ~29 s) 50K cold reduction was not a CI shape. The gate takes the
**median of a small per-row iteration count** (reduction: 9/3/1 iterations at
1K/10K/50K; bundle: 40 commit / 25 read; cut to 5/2/skip and 12/8 under
`EPIPHANY_BENCH_QUICK=1`),
cold (no in-gate warm-up — the marquee budget is an explicitly *cold* rate).
Criterion uses flat sampling with `sample_size(10)` for the reduction group;
the 50K point is gate-only (calibrated when a single iteration took ~29 s and
criterion's ≥10-sample loop would have blown the wall-clock budget; ~0.58 s
post-fix). Full `cargo bench -p epiphany-testkit` stays under ~2 minutes of
measurement; quick mode under ~15 seconds.

**Measured on the dev profile (2026-07, post-K-fix), the table's ground
truth:**

| row | budget | measured | verdict |
|-----|--------|----------|---------|
| `reduction/1000` | > 10,000 env/s cold | ~674,000 env/s (~1.5 ms) | Pass |
| `reduction/10000` | > 10,000 env/s cold | ~257,000 env/s (~39 ms) | Pass |
| `reduction/50000` | > 10,000 env/s cold | ~87,000 env/s (~0.58 s) | Pass (promoted from Xfail) |
| `bundle/typical_edit_commit` | ≤ 50 ms | ~14.7 ms (real fsync) | Pass |
| `bundle/open_bootstrap_read` | ≤ 200 ms | ~63 µs (moderate corpus) | Pass |

As F1 first measured it (2026-07, pre-fix), the O(n²)
`canonical_reduction_order` indegree construction was unambiguous in the curve
(~155K / ~12.5K / ~1.7K env/s at 1K/10K/50K — 10x envelopes cost ~120x time),
sinking the 50K row (~29 s per cold reduce, the documented xfail) and leaving
10K only ~25% over budget. Agent K's subquadratic rewrite
(threshold/frontier readiness, byte-identical order — see
`epiphany-ops/DECISIONS.md`) triggered the gate's XPASS promotion notice, and
the 50K row was flipped to Pass in the same change (**F surfaces, K fixes**,
round-tripped). Any future red row is a fresh regression: fix the reducer, do
not re-mark rows xfail without a written decision here.

**Two honesty notes on the bundle rows.** (1) They run on the build target's
filesystem, *not* `std::env::temp_dir()`: `/tmp` is commonly tmpfs, where fsync
is a near-no-op — measured, the commit row is ~54 µs on tmpfs vs ~14.7 ms on a
real NVMe filesystem, a 270x difference that would have made the 50 ms budget
vacuous. (2) The read budget is spec'd against "a 100-page orchestral score";
no such corpus generator exists yet, so the row is a moderate-corpus (1K
envelopes + canonical-base snapshot) stand-in with margin to spare, and says so
in its docs.

**Scope, and the budget rows deliberately not built yet.** F1 covers the three
budgets that are measurable against shipped subsystems: the reduction rate and
the two file-format budgets. The remaining Chapter 10 rows are blocked on
unimplemented or in-flight subsystems and become future benches in this same
`benches/` + gate shape: the interactive keystroke→frame budgets (blocked on
Agent I's engrave/render pipeline maturing past the visible slice), cold
solving of a 100-page score within 2 s p99 and the incremental-propagation
bound (blocked on the real Chapter 9 solver and a 100-page corpus generator —
the latter also unblocks the honest read-row corpus), and the memory ceilings
(blocked on the same corpus). CI runs the gates in the `conformance` job with
`EPIPHANY_BENCH_QUICK=1` (50K skipped) and in the nightly `soak` job in full.

## F3 — The representative score corpus + eligibility-taxonomy harness (Agent H)

*Worklist item F3 — "the most underbuilt dependency; unblocks H entirely."*

The corpus lives in [`crate::corpus`] as ≥20 deterministic, `check_invariants`-clean
fixtures tagged by tier (common / edge / torture) and by the event-kind
eligibility taxonomy of `PHASE2_QUICKSTART §H`. Rather than re-deriving the
taxonomy, the harness runs Agent H's own `epiphany_core::derive_annotations` and
reads its [`epiphany_core::TaxonomyReport`], then (a) independently recounts
events by kind and **cross-checks** H's counts (so a miscount is caught, not
trusted), and (b) aggregates per-bucket counts across the corpus and asserts every
taxonomy bucket is non-empty or explicitly deferred. The corpus also re-uses the
existing positive generators (`valid_score`, `valid_score_rich`,
`ten_measure_single_staff`) as fixtures so F's taxonomy harness runs over the same
graphs Agents H and I develop and render against.

**Deferred buckets** (documented, not required non-empty by the clean corpus):
none — every bucket including `decomposition_skipped_nonmusical` (zero-duration
grace) and `decomposition_ungriddable` (off-grid / sub-sixty-fourth torture
cases) is exercised by a dedicated fixture, so the honest-classification paths are
all proven reachable. If a future invariant change makes a bucket unreachable by
clean input, move it to `corpus::DEFERRED_BUCKETS` with a written reason rather
than dropping the assertion.

## F4 (H) — The H merge gate

[`crate::prepass_harness`] is H's merge gate (worklist F4). It asserts H's stated
`PHASE2_QUICKSTART` acceptance criterion over the corpus: every *eligible*
`IdentifiedPitch` carries a **non-trivial** spelling (verified by pitch-class
correctness, which the old constant-`C4` stub fails); every *eligible* determinate
metric duration carries a `Decomposition` whose components reconstruct the
duration (invariant 15); ineligible cases are classified and counted; derivation
is deterministic across runs (asserted by structural equality **and** a canonical
textual fingerprint, since `DerivedAnnotations` deliberately has no codec);
`RespellPitch`-style authored overrides take precedence; and the derivation stays
deterministic when run on materialized scores in the criterion-5 pipeline.

**Non-vacuity guard** (the F discipline — the gate must go red if H were stubbed):
across the corpus the harness requires multiple distinct spelled nominals and at
least one accidental (a constant-`C4` stub yields one nominal, zero accidentals),
at least one multi-component (tied) decomposition and multiple distinct note
values (an empty/no-op decomposition map yields neither), and per-pitch
pitch-class correctness (the stub mis-spells the first non-`C` pitch).

## Pass 12 batch tracker

Per F's mandate, the Pass 12 batch is tracked in `spec/PASS12_BATCH.md`. It opens
once ≥3 ambiguities accumulate (same rule as v0 → Pass 11). Agent H's landing
contributed five candidates (P12-H1…P12-H5, recorded in
`crates/epiphany-core/DECISIONS.md`), which crosses the threshold, so the batch is
open. F does not resolve these; F collects them.

## F5 — The Reference Suite harness (`src/reference_suite.rs`, 2026-07)

The Reference Suite companion (v0.1.0) charters the v0.1 entry set — six
scores named by reference-implementation **builder and seed** — and its
non-normative Harness Binding chapter says the executable binding "is
delivered with the reference implementation." This module is that binding,
in the F0 shape: a library module holding the machinery
(`entries`/`evaluate_minimal`/`table`), asserted by
`tests/reference_suite.rs` with **one test per entry** so a failure names its
entry, plus pins for the entry-set shape, the declared A4 default geometry
(the companion's solve-configuration requirement — asserted against
`Engraver::default().geometry()`), and RS-2's builder identity
(`corpus gen_valid_score_rich` ≡ `generators::valid_score_rich(0xF302)`,
byte-for-byte).

**Solver-parametric on purpose.** The library module takes
`&dyn ConstraintSolver`; the integration test supplies the real `Engraver`.
This keeps `epiphany-engrave` a dev-only dependency (the library dependency
graph is unchanged, mirroring the multi-system click test) while the harness
itself stays reusable against any solver claiming Minimal.

**The four-condition pass rule, with the F1 Pass/Xfail discipline on
condition 4.** `evaluate_minimal` asserts the companion's per-entry rule
exactly: hard-constraint satisfaction (renderable, non-partial,
`satisfied_hard_constraints`, nothing unsatisfied); internal determinism
(byte-identical `ResolvedLayoutIR` canonical bytes *and* bitwise-identical
metric vectors across repeated solves); a well-formed, accurate report
(Minimal tier claim, every axis a finite `[0,1]` value, never the unmeasured
placeholder); and every axis at or below the Quality Metric Catalog's
Minimal-column threshold. The first real measurement (2026-07) found exactly
one miss: **RS-1's `casting_off_quality` = 1.0** — greedy first-fit leaves a
two-measure stub last system (glyph spans ~78.6/18.8 staff spaces, width CV
0.61 ≥ the 0.5 anchor), the exact failure the axis exists to catch, on a
layout that is byte-locked by the render goldens. Waiving it silently would
fake conformance; failing the workspace would misreport a ratified-spec
tension as a code bug. So condition 4 carries the budget harness's (F1)
discipline: the miss is a **documented `minimal_xfail` row asserted to still
miss** — if the layout or the catalog changes and RS-1 comes within
threshold, the harness fails demanding promotion (remove the row), exactly
like an F1 `XPASS`. The row's resolution is spec-side and tracked in
`epiphany-engrave/DECISIONS.md`'s Pass-12 candidates (casting-off balance
pass with golden regeneration, or a QMC anchor/threshold minor revision —
the catalog's own threshold-tuning open question anticipated this).

**Eligibility tiers vs. solver tiers.** The corpus `Tier`
(Common/Edge/Torture) is Agent H's eligibility taxonomy; the suite's tiers
(Minimal/Standard) are Chapter 9 conformance tiers. The companion carries the
same caution; the harness resolves corpus entries by `name` string only and
never reads the corpus tier.

## The Text Projection grammar checker (`tests/text_projection_grammar.rs`)

Text Projection 0.3.0 claimed a "machine-checked" grammar. It was checked by a
throwaway script, so the claim was **true of one run and of nothing durable**.
This test file is the durable form; 0.4.0 retracts the earlier claim in its own
revision history. It is the same evidence gap the P2–P4 decode work kept
finding: a green gate proves nothing about coverage until the harness states its
own reach.

Six tests, and every one was **mutation-verified** — the mutation was applied to
the `.tex`, the anchor asserted present before substitution (a `str.replace`
that matches nothing looks exactly like a passing test), and the named test
observed to fail:

| Mutation | Killed by |
|---|---|
| delete the `transpose-interval` production | `the_kind_productions_are_the_operation_vocabulary` |
| reference an undefined nonterminal | `every_nonterminal_is_defined_and_reachable` |
| let `unescaped` admit U+005C (the 0.3.0 bug) | `the_escape_grammar_agrees_with_the_escape_requirement` |
| drop the `derived-ordering` citations | `the_derived_ordering_requirement_is_cited_where_it_applies` |
| spell the escape introducer `"\\"` | `the_escape_grammar_agrees_with_the_escape_requirement` |
| drop the `\t` escape alternative | `the_escape_grammar_agrees_with_the_escape_requirement` |
| restore `Ligatures={TeX}` on the mono font | `the_mono_font_does_not_substitute_glyphs_in_the_grammar` |

**The vocabularies are derived, never transcribed.** The 31 operation-kind
productions come from `OperationKindTag::PAYLOAD_FREE` plus `Registered`, mapped
to Catalog section names through an **exhaustive `match`** — so a kind added to
the enum and not to the grammar fails to compile, then fails the test. The chunk
productions come from `ChunkKind::from_discriminant`. This is the
`operation_kind_tag_vocabulary!` lesson applied to prose: a hand-maintained list
parallel to an enum is a latent false lock.

**Every locator finds its production by name, not by column.** Writing the
checker exposed four bugs *in the checker*, three of them of this kind: a
column-anchored needle (`"projection   ::="`) that a reflow broke — a checker
that cannot find the grammar silently checks nothing; a `defined()` that missed
three productions whose left-hand side sat on the line above their `::=`, which
would have **hidden** undefined nonterminals; and a per-line `<...>` stripper
that leaked `0022` and `005` out of multi-line prose spans, where they read as
nonterminals. Nonterminal tokens must now begin `[a-z]`, per `symbol` itself.

**Two rendering rules are load-bearing, not cosmetic.** A document that
specifies a text syntax must not misprint that syntax.

- The escape introducer and the string delimiter are written as **codepoints**
  (`U+005C`, `U+0022`). A quoted terminal `"\\"` reads as *two* backslashes,
  making every escape three characters where the requirement says two. This is
  the audited 0.3.0 defect in a second disguise; it was written, caught by
  review, and is now asserted against.
- The mono font must not enable **TeX ligatures**. `tlig` rewrites `"` as a
  right curly quote and `--` as an en dash, so `string ::= '"' schar* '"'`
  rendered as `’”’ schar* ’”’`. `core_spec.tex` already omitted `Ligatures={TeX}`
  from its `\setmonofont`, which makes this the house convention rather than a
  new one; the three companions that still enable it have no listing content the
  feature would touch.

### 0.5.0: what starting the implementation found

The grammar could not derive an ordinary pitched note. `value` had **no
alternative for a sequence**, though `req:textproj:value-projection` clause 5
required one. Adding it exposed why it had been missing: a sequence whose first
element is a fieldless variant is *shape-identical* to a struct, and `()` is both
the empty sequence and the absent option. Both collisions are reachable from the
first pitched note in any score — `PitchedEvent` carries `articulations` and
`ornaments` (sequences over the zero-field `ArticulationMark` / `OrnamentMark`)
beside an optional `DynamicMark`.

**The collision is irreducible without new syntax, and new syntax buys nothing.**
`req:textproj:strict-parse` already obliges a parser to reject "a duplicate in a
set-typed field" — which it cannot do without knowing the field is set-typed. A
parser consults the schema either way. Spending `(seq …)` or `[…]` to remove a
*shape* ambiguity, while leaving the schema dependency that syntax was meant to
remove, lengthens every line and simplifies no tool. **User ratified
schema-directed** (`req:textproj:schema-directed`), so `value` collapses to
`"(" value* ")"` or a leaf — all that shape can honestly say — and the
requirement assigns meaning by expected type.

Three consequences, now stated: a struct with no fields is the bare symbol, as a
fieldless variant is; a **byte string is not a sequence** (where the binary form
writes a length-prefixed run of bytes — an opaque extension payload, a
`SoundConfiguration` — the projection writes a byte string, never a list of
integers); and the grammar's repetitions carry a notation rule, adjacent elements
separated by exactly one space, without which `"(transpose (" bytes* ") "` spelled
two targets as one undelimited run of hex.

The checker gained `value_admits_a_bare_list_and_claims_no_shape_it_cannot_distinguish`,
mutation-verified two ways: restoring the 0.4.0 `value` production, and stripping
the `schema-directed` citations. A grammar that claims to distinguish a struct
from a sequence would be lying, so the test asserts the symbol-headed alternative
is **absent**.
