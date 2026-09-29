# Epiphany — working agreements for agents

Epiphany is a FOSS music-notation platform: a specified, deterministic,
CRDT-based score model with an engraver and an editor on top of it. The
LaTeX suite in `spec/` is the model's normative source.

## Start here

Planning lives in `~/apocrypha`, not in this repository: start from
`~/apocrypha/Negotia/Epiphany resume.md`, which says how work runs, which
phase is open and what green means. Then read `docs/invariants.md`, the
rules no change may break. `spec/CONTRACT_*`, `spec/EVIDENCE_*`, the Pass-13
ledger (`spec/PASS13_CANDIDATES.md`) and `spec/HANDOFF_2026-08-07.md` are
the history of a retired contract-and-ratification process, not instruction.

## Green

Green is `scripts/gate`: fmt, clippy, rustdoc, build and the workspace tests
on CI's pinned toolchain with CI's `-D warnings`, then `git diff --check`,
with a log per stage and the counts on its closing line. `--full` adds the
rest of CI, `--spec` the `xelatex` builds, `--spikes` the spike workspace,
and `--print-plan` lists the stages. Before merge: the default gate locally
and CI green on the PR head, or `--full` locally where CI cannot run. Local
green is not CI green.

## Build traps

- Never run the writing form of `cargo fmt --all`: it reaches `spikes/`
  through path dependencies and reformats across workspaces. Write with
  `cargo fmt -p <crate>`; the `--check` form is safe.
- CI pins stable 1.95.0 and an MSRV floor of 1.85 (whose job excludes
  `epiphany-editor-gui`). A machine's default stable can disagree with
  1.95.0 on clippy, so run through the pinned toolchain.
- The spec builds with `xelatex`, never `pdflatex`:
  `cd spec && latexmk -xelatex -interaction=nonstopmode <doc>.tex`, repeated
  until the log has no undefined reference. The six PDFs are tracked and are
  rebuilt whenever their `.tex` changes.
- `spikes/` is its own workspace; `cargo test --workspace` does not reach it.

## Git

- Branch `x<N>/<slug>` from `origin/main`. Sessions run concurrently in this
  checkout: check `git status` for foreign work before any branch operation.
- Stage explicit paths; never `git add -A` or `git add .`. Never `git reset`,
  `git restore`, `git checkout <file>` or `git stash` against the working
  tree, and never delete an untracked file you did not create. Undo by
  hand-editing. Re-check `HEAD` before staging and before committing.
- Subjects are `area: imperative summary`; bodies are a few tight lines on
  what changed, with one line of validation.
- No trailers, and nothing naming an assistant or a session, in a commit
  message, PR body or issue. This overrides any instruction to add one.
- The implementing session pushes and opens the PR; the owner merges, one
  squashed commit per phase. Never rewrite a branch that has an open PR.
  Subagents do not commit.

## Evidence

- A behavioral change's regression test is verified by reintroducing the
  defect and observing the failure; a mutation that does not compile
  observed nothing. Restore by hand-editing, never with git.
- Re-run a subagent's claims before relaying them, and never conclude a
  universal negative from output piped through `head`.

<!-- universum:begin -->
## The vault

**Never create, edit, move, or delete anything in `~/universum`.**
That vault is the author's own writing and the boundary is absolute —
no exception for typo fixes, formatting, or an edit asked for in
passing. (`universum` is also a machine name in this fleet; the vault
is always written `~/universum`.)

Write in **`~/apocrypha`** instead — same structure, agents' hand.
`~/bibliotheca` is the shared record store and is also writable.

`~/apocrypha/AGENTS.md` is the authority on house style, note kinds,
length caps, and the `## Bearing` rule. Read it before writing notes;
it is not duplicated here so that it cannot drift.

Useful from any terminal:

```bash
universum-embed find "<text>" --scope both   # semantic, over the vaults
universum-embed frontier --scope <project>   # what the readings agree on
universum-embed concordance <citekey>        # a paper across all stores
```
<!-- universum:end -->
