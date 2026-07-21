# Contract: the Text Projection document layer

Repo root `/home/jeans/Repos/active/epiphany`. Read this in full before writing a
line. The plan is `spec/PLAN_TEXTPROJ_DOCUMENT.md`; both its rulings are granted
and this contract states them as law.

The two layers below you are done and are your model: `epiphany-core`'s
`textvalue*.rs` (Chapter-5 values) and `epiphany-ops`'s `textproj_*.rs`
(Chapter-6 operations). Read the latter — the document layer is the same shape.

## Grammar-directed, and free functions

`req:textproj:operation-vocabulary` established that the grammar's productions
govern, not the mechanical value rule. The document lines are the same: find your
production in `spec/text_projection.tex`'s Grammar chapter and implement exactly
what it says.

**Do not implement `TextValue` for bundle types.** `epiphany-bundle` does not
depend on `epiphany-core`, so the trait is foreign and so is the type — the
orphan rule forbids it from `epiphany-textproj`. You do not need it: no
document-line production contains a `value` position; every one bottoms out in
`bytes`, `integer`, `bool`, `string`, `option`, or a closed vocabulary. Write
**free functions** — `fn project_profile(&ProfileDeclaration) -> Sexp`,
`fn parse_profile(&Sexp) -> Result<ProfileDeclaration, TextError>`. Use
`epiphany_core::textvalue::{Sexp, read_sexp, TextError}` for the machinery.

Do **not** add `epiphany-core` to `epiphany-bundle`'s dependencies. The bundle is
a container format and its independence from the music model is deliberate.

## Blobs: emit none, and reject them on parse

A blob is canonical **iff** referenced by a canonical operation or by canonical
reduced state (`req:textproj:canonical-blobs`; `core_spec` §"Canonical and
Non-Canonical Manifest Roots"). Nothing in `epiphany-core` or `epiphany-ops`
references a `BlobId` — verified by source scan — so **no blob is canonical
today** and the correct projection of every real bundle emits zero `(blob ...)`
lines.

Three obligations, and they are not the obvious ones:

1. **Emit side.** Implement the reachability predicate as a real function that
   today provably returns the empty set. **Never project
   `manifest.blob_roots`** — that is the plausible wrong answer: one line, looks
   right, emits non-canonical blobs in violation of the requirement. Even
   `manifest.rs`'s own doc comment says "canonical `blob_roots`" — the subset, not
   the field.

2. **Parse side: reject.** Document-level validation **MUST reject any
   `(blob ...)` line** as unreferenced, with a test asserting the rejection.
   A blob-bearing text at this companion version is necessarily non-canonical.
   Accepting one would stage a blob into the bundle that the next projection
   silently drops, which loses data *and* falsifies
   `project(serialize(parse(T))) == T` for that text. Forward compatibility is
   owned by header-version gating, not by leniency here.

   Line-level `project`/`parse` of the `(blob ...)` production must still be
   written and unit-tested in **both** directions against synthetic data, so that
   when the predicate becomes non-empty both sides are ready at once.

3. **A trip-wire.** A test that source-scans `crates/epiphany-core/src` and
   `crates/epiphany-ops/src` for the token `BlobId` and fails if it appears — same
   family as the existing drift locks. Its failure message must name the
   reachability predicate and tell the finder that both the emit side and the
   parse-side rejection now need their real implementation.

## The header carries exactly one version

The header names the version of the companion the text conforms to. The parser
**accepts exactly one**: the companion version this crate implements. Anything
else is a rejection at line one. Multi-version acceptance and migrate-on-read for
text are future spec decisions — do not improvise them.

Define the version once, as a constant, and lock it against the companion with a
test that reads `spec/text_projection.tex` and asserts the title version matches.
A constant that silently disagrees with the document it claims conformance to is
the whole failure mode this project keeps paying for.

## Section order is normative

`projection ::= header document lineage? profile* extension* canonical-base?
blob* envelope*` is a **sequence**, not a set. A repeated or out-of-order section
is a rejection. A parser that accepts lines in any order and sorts them is
normalizing, which `req:textproj:strict-parse` forbids.

## What must not appear (`req:textproj:derive-or-carry`)

* **Physical attributes** — `offset`, `compressed_length`, `compression`,
  `uncompressed_length`. A serializer chooses them freely.
* **Derivable identities** — `ChunkId`, `ContentHash`, `BlobId`. Re-derived from
  content.
* The **one** identity carried verbatim is `SnapshotId`, which has nothing to
  derive from.
* **Non-canonical accelerators are not projected**: `operation_index_root`,
  `acceleration_snapshots`, `text_projection_root`, `integrity_root`,
  `operation_block_summaries`. A bundle that round-trips through text comes back
  without them. That is correct; say so in a doc comment, because it reads as data
  loss and is not.

## `derived-ordering` sorts by projected form

`req:textproj:derived-ordering` applies to exactly two sequences: the `(blob ...)`
lines and an extension's preserved chunk roots. Order **and de-duplicate** them by
the **UTF-8 bytes of their rendered form** — not by any binary key, because the
binary key reads the offset, and a chunk's file position must not decide the
text's order. Every other sequence keeps the binary order.

## Traps

* **The extension line has six fields** in ratified declaration order: id,
  version, required, chunks, affected-kinds, barriers. `affected_object_kinds`
  and `edit_barriers` are **opaque byte strings**, never structured — the
  `Vec<u8>`-is-not-a-sequence trap that bit the operation layer lives here too.
  Each preserved chunk root projects as kind + schema_version + **uncompressed
  payload**, never as a `ChunkRef`.
* **`ProfileId::Custom`** carries a 16-byte registry id and is the one profile
  that is not a bare symbol.
* **Two `WallClockDuration` types exist**, one in `epiphany-core` and one in
  `epiphany-bundle`. The retention policy uses the bundle's.
* **`canonical-base` carries the root chunk's uncompressed payload inline**, as a
  single byte string, plus the `SnapshotId` verbatim.
* **Envelopes are emitted in `canonical_reduction_order`**
  (`epiphany_ops::canonical_reduction_order`, public and exported).

## Style

Match the surrounding code. Doc comments on every public item and every
non-obvious decision — especially *why* a parse rejects rather than normalizes.
`rustfmt` clean, no `clippy` warnings, no `#[allow(...)]`, no `unwrap`/`expect` on
a parse path. Touch only the file you are told to create; do not run
`cargo fmt --all`.

## Verification

The standing gate: `cargo fmt --all --check`; `cargo clippy --workspace
--all-targets` → 0; full workspace tests; `RUSTDOCFLAGS="-D warnings" cargo doc
--workspace --no-deps` → 0; `cargo run -q -p epiphany-testkit --example
conformance_suite`; zero golden churn.

Plus the three this project learned the hard way:

1. **Mutation-verify every check you write.** Assert the anchor is present, delete
   or invert the check, confirm a *named* test of yours fails, restore. Report per
   check, with the actual command output. A survivor is a finding — say so.
2. **Exercise every outbound normalization.** For every value you normalize on the
   way out, construct a non-normalized input and prove the normalization happens.
   `derived-ordering` is exactly this shape: build a document with duplicate blobs
   and out-of-order chunk roots *in memory* and prove the projection collapses and
   orders them. Five such normalizations shipped untested in the operation layer
   because every fixture was already sorted.
3. **Assert your suite's own reach.** A round-trip suite that never exercises an
   extension, a canonical base, or a multi-envelope document proves far less than
   its green tick suggests. Count what you covered and assert the counts.

Report the actual commands and their actual output. A previous agent reported
"verification passes" when errors did point into its own file.
