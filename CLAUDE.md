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
| **Spec / Pass 13** — wire format, bundle, ops, text projection, the `.tex` suite | `spec/`, `crates/epiphany-{core,ops,bundle,textproj,testkit}` | P13-S27 unblocked and dispatchable |
| **Editor / T4** — the editing seam, engraving, the toolkit spike | `spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_*`, `spikes/`, `crates/epiphany-{editor-core,editor-gui,engrave,layout-ir,glyphs,render-svg}` | T4 spike, round 2 built but not run |

They are currently independent. **They collide when T1b opens**, because T1b
and P13-S27 both land in `epiphany-bundle/src/bundle.rs`. Do not fly those two
together.

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
cargo test --workspace                                 # expect 1570 passing, 0 failed
cargo clippy --workspace --all-targets -- -D warnings   # clean
```

If the count differs on arrival, reconcile that **before** starting new work —
the mutation discipline above depends on a known-green baseline.

## One live constraint

Until **P13-S27** lands, **no bundle anywhere may carry a canonical base** — not
in production, tests, or the conformance suite. Base-bearing fixtures must be
hand-built images (see `craft_image` in `epiphany-bundle/src/bundle.rs`).
Refusals you hit there are the design, not a bug. See handoff §1.2.
