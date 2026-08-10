# Epiphany — working agreements for agents

FOSS music-notation platform: a specified, deterministic, CRDT-based score
model with a LaTeX specification suite as its source of truth.

## Read this first

1. **`spec/HANDOFF_2026-08-07.md`** — current state of both tracks, what
   transfers between machines, environment setup, and what to do next. Start
   there; everything below is the standing rules that document assumes.
2. **`spec/PASS13_CANDIDATES.md`** — the spec-track ledger. Status cells are
   **appended to, not rewritten**, so an opener can lag the truth by several
   rungs. Read the whole cell.
3. The contract for whatever you are about to touch: `spec/CONTRACT_*.md`.

## The two tracks

| Track | Lives in | Current head |
|---|---|---|
| **Spec / Pass 13** — wire format, bundle, ops, text projection, the `.tex` suite | `spec/`, `crates/epiphany-{core,ops,bundle,textproj,testkit}` | **P13-S27 LANDED** (`4df8e25`); **P13-S16 LANDED** (`aee4ff9`) — six findings against its own contract are unamended, see its `PASS13_CANDIDATES.md` row |
| **Editor / T4** — the editing seam, engraving, the toolkit spike | `spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_*`, `spikes/`, `crates/epiphany-{editor-core,editor-gui,engrave,layout-ir,glyphs,render-svg}` | T4 spike, round 2 built but not run |

They are currently independent. The T1b/P13-S27 collision in
`epiphany-bundle/src/bundle.rs` is **resolved — S27 landed 2026-08-09.** **This does
not make T1b free:** it stays blocked on Ruling B blocker (ii), versioned decode
(handoff §2.3). Any future `epiphany-bundle` rung re-creates a collision with T1b on
its own terms — that is a property of the crate, not of S27.

## How work is done here

**Contracts are ratified, then frozen.** Substantive work is scoped into a
`spec/CONTRACT_*.md` with numbered pins, tests, mutations, a touch table and a
gate; it goes through adversarial review rounds before dispatch. **After
ratification the pins are executed, not edited.** A defect found during
execution is **reported, not patched in place** — if it needs a pin change,
that is its own amendment with its own review round.

**The touch table is the staging allowlist.** A file that must change but is
not listed silently drops out of the commit. Two recurring escapees, in no
contract's table: `crates/epiphany-testkit/tests/requirement_labels.rs` (its
hardcoded requirement counts move whenever a `.tex` gains a requirement), and
version literals in `.tex` prose.

**Mutation-first.** Every regression test is verified by re-introducing the bug
and **observing** the failure. Reasoning that a mutation *would* fail signs
nothing. A **compile error is not a test failure** — a mutation that does not
compile observed nothing. Restore by hand-editing back, never with git.

**Verify subagent claims before committing them.** Re-run the tests, re-read
the diff. Agents in this repo have misattributed failures, missed failing
suites, and guarded the wrong code path — all found only by re-running.

**Before believing a zero-result search, ask what it would miss if the claim
were false.** Different encoding, different spelling, a propagating rather than
constructing path, output truncated by `head`. Never conclude a universal
negative from a piped `head`.

**Guard every reachable path, not the one the spec sentence names.** Enumerate
the public entry points into an invariant and ask which a caller can actually
reach today.

## Git

- **Stage explicit paths. Never `git add -A`.**
- **Never `git reset`, `git restore --staged`, `git checkout`, or `git stash`**
  against the working tree — sessions have run concurrently here, and these
  destroy work that is not yours. Undo by hand-editing.
- Re-check `HEAD` before staging and before committing.
- Subagents do not commit. They leave work for review.

## Build and environment traps

- **Never run `cargo fmt --all`.** It reaches the `spikes/` workspace through
  path dependencies and reformats across workspaces. Use `cargo fmt -p <crate>`.
  The `--check` form CI runs is safe; the *writing* form is not.
- **The spec builds with `xelatex`, not `pdflatex`** — `fontspec` refuses
  pdfTeX outright: `cd spec && latexmk -xelatex -interaction=nonstopmode <doc>.tex`.
  Re-run until `undefined references` clears; one pass is often not enough. The
  six PDFs are tracked and must be rebuilt when their `.tex` changes.
- Toolchains: pinned stable **1.95.0**, MSRV floor **1.85**. The MSRV CI job
  excludes `epiphany-editor-gui`.
- `spikes/` is **its own workspace** — root `cargo test --workspace` does not
  reach it. Gate it separately.

## Green baseline

```
cargo test --workspace                                 # expect 1583 passing, 0 failed, 0 ignored
cargo clippy --workspace --all-targets -- -D warnings   # clean
```

**This is the single origin for the count** — `spec/HANDOFF_2026-08-07.md` used to
repeat it in three places and now points here. It moved 1570 → 1577 when P13-S27 landed,
and 1577 → 1583 when P13-S16 landed (six net-new tests).

**Use `--no-fail-fast` whenever anything is failing.** The bare command stops at the
first failing suite, so a partial failure set reads as the whole one — P13-S16's M6a
reported six failures over four suites bare, and seven over all forty-two with the flag.

If the count differs on arrival, reconcile that **before** starting new work —
the mutation discipline above depends on a known-green baseline.

## The canonical-base rule — the "one live constraint" is LIFTED

**P13-S27 landed 2026-08-09 (`4df8e25`), so the blanket prohibition is gone.** It read:
*until P13-S27 lands, no bundle anywhere may carry a canonical base.* It no longer
applies, and base-bearing bundles are constructible again through the ordinary API.

**What replaced it, and it is not "anything goes":** a canonical base is accepted only
when its `reduction_algorithm_version` equals the running authority,
`epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION` — **currently `1`**. A mismatch is
`CanonicalBaseRequiresRebuild { base, current }`, refused on **both** the read side
(`open`) and the write side (`commit`/`commit_versioned`). Legacy-epoch bundles still
refuse a base outright.

**Any base materialized before P13-S16 must be rebuilt, not reused** — it declares `0`
and holds state the current semantics would not have computed.

**The bump discipline is the whole guarantee.** Any change to a canonical reduction
verdict **or to canonical reduced state** MUST bump that constant — **no mechanism can
detect a semantics change**, so nothing will catch a missed bump. Both classes are named
because a change leaving every verdict intact while altering the reduced graph is the
easier one to overlook, and it invalidates a base just as completely.

P13-S16 made the first bump, `0` → `1`, and carried one change of each kind:
`CreateStaffGroup`'s verdict (applied → `ContainerNotEmpty` no-op) and `CreateStaff`'s
reduced state (it still applies, but now maintains `StaffGroup.members`). The constant's
own `Bumps` list is the record of why each version exists; a bump without its entry
leaves a number nobody can account for.

Fixtures deliberately exercising arbitrary wire versions take
`BundleCapabilities::synthetic_for_fixture(v)`; production paths take the crate-local
`production_caps()`. Never the former on a production path. `craft_image` /
`craft_image_with_base` still exist for hand-built images, but are no longer the *only*
way to get a base. See handoff §1.2, marked as a dated record.
