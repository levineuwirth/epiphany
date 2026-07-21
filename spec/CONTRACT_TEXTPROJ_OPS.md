# Contract: the Text Projection operation layer

Repo root `/home/jeans/Repos/active/epiphany`. Read this in full before writing a
line. The plan it implements is `spec/PLAN_TEXTPROJ_OPS.md`.

## The ruling: this layer is grammar-directed

`req:textproj:value-projection` — the mechanical struct/enum/newtype rule — does
**not** govern the operation vocabulary. It governs exactly the `value` positions
*inside* the grammar's productions.

The grammar is in `spec/text_projection.tex`, Chapter "Grammar", in the single
`lstlisting` block. **It is the specification for your work.** Find your
production, implement exactly what it says. Where it says `value`, call
`TextValue`; where it says anything else, follow the production.

Three consequences you will meet immediately:

* An operation kind **inlines** its `*Op` payload record. `(insert-event #x0a
  <event>)` — *not* `(insert-event (insert-event-op #x0a <event>))`. The record
  exists so each enum variant can name a type; the binary form adds no bytes for
  it (`OperationKind`'s encoding writes the tag then delegates), so the text adds
  no wrapper either.
* Envelope-level productions use the grammar's names, not kebabbed Rust names:
  `envelope`, `stamp`, `causal`, `undo`.
* `stamp` flattens `HybridLogicalClock` into three arguments. That is the
  grammar's shape and it is deliberate.

## Where field order comes from

Every operation payload type has an explicit
`impl CanonicalEncode for T { fn encode_canonical(&self, …) }` in
`crates/epiphany-ops/src/payload.rs` (or `undo.rs` / `conflict.rs`). It reads its
fields in order. **That order is the ratified declaration order — mirror it
exactly.** Never infer an order from the struct declaration, a doc comment, or
the grammar's argument names; read `encode_canonical`.

`crates/epiphany-ops/src/envdecode.rs` is the binary *inverse* and is the best
cross-reference in the repo: it already decodes every one of these in wire order.

## Strict parsing is the point

`req:textproj:strict-parse`: a parser MUST reject text that is not the canonical
projection of the value it denotes. It MUST NOT normalize — not whitespace, not
case, not an out-of-order sequence, not a duplicate in a set-typed field.

The rules that follow from that, learned expensively on the core layer:

* **If a constructor normalizes, check before constructing.** Returning the
  normalized value *is* accepting the bad input.
* **If a constructor only validates** (rejects rather than adjusts), the `None`
  it returns is the whole of the strictness. Do **not** add a
  re-project-and-compare guard as a backstop: on the core layer four such guards
  were written, mutation-tested, found unable to fire, and removed. A check that
  cannot fail is worse than no check — it invites weakening the real one.
* **Order-preserving `Vec`s need per-site checks.** A guard that re-projects
  cannot see an order it faithfully preserves.
* **Exercise every outbound normalization.** For every value normalized on the
  way out, construct a non-normalized input and prove that normalization happens.
  Five such normalizations were written across two agents and all five were
  untestable by omission because every fixture was already sorted. A projection
  that emits text its own parser rejects is a defect ordinary round-trip tests
  cannot see.

### The six order-constrained sequences

Each one's encoder normalizes, so `project` must apply the same normalization and
`parse` must reject text that is not already in that order.

| field | rule | note |
|---|---|---|
| `TransposeOp.targets` | **non-decreasing multiset** | duplicates are **legal**; the wire form is frozen. Reject only a strict *decrease*. |
| `TransposeIntervalOp.targets` | strictly increasing | a `CanonicalSet<PitchId>`, which is a type alias for `BTreeSet<PitchId>`; the existing core impl already does this check |
| `ChangeRegionTimeModelOp.declared_incompatible` | non-decreasing | encoder applies `sorted_canonical` |
| `TupletCompensation::RewriteTuplets.tuplets` | non-decreasing | encoder applies `sorted_canonical` |
| `TupletCompensation::CascadeDeleteTuplets.tuplets` | non-decreasing | shares one encoder arm with `RewriteTuplets`, but is a **separate** arm in the projector |
| `PositionRemapping::Reassign` entries | non-decreasing by `EventId` | encoder does `sort_by_key(|(e, _)| e.canonical_bytes())` |

**This table said "three" until an agent found the other three.** The scoping
script behind it searched `pub struct` bodies for sequence fields and never
looked inside enum variants, so every constrained sequence living in an enum was
invisible to it. If you are looking for sites of some kind, say out loud what
shapes your search can and cannot see before you trust its answer.

Getting `TransposeOp` backwards — rejecting a duplicate — silently breaks a
frozen operation's replay. It is the single highest-risk line in this work.

## Names are generated, never spelled

`OperationKindTag::catalog_name()` is production code, generated by
`operation_kind_tag_vocabulary!` from the same list that generates the wire
discriminant and the decoder. **Use it.** Writing `"insert-event"` as a literal
creates a list parallel to the enum — the exact shape that has cost this project
four bugs, most recently a kind that encoded to tag 30 and whose own decoder
rejected it.

For the same reason, `parse` must dispatch through an **exhaustive `match` over
`OperationKindTag`** (no `_` arm), so a kind added to the vocabulary and not to
the projector fails to compile. Every `match` in a `project` is likewise
exhaustive with no `_` arm.

## Style

* Match the surrounding code. Doc comments on every `impl` and every non-obvious
  decision — especially *why* a parse rejects rather than normalizes. No comment
  that merely restates the next line.
* `rustfmt` clean. No `clippy` warnings. No `#[allow(...)]`.
* No `unwrap`/`expect` on a parse path, except where an invariant was just
  checked — and then name the check in the message.
* Touch **only** the file you are told to create. Do not edit `payload.rs`,
  `envelope.rs`, `textvalue*.rs`, or another agent's file.
* Do **not** run `cargo fmt --all` — it rewrites files other agents hold. Run
  `rustfmt --edition 2021 crates/epiphany-ops/src/<your file>`.

## How to verify, and how not to mislead yourself

Sibling files may be empty stubs while you work, so the crate may not link. That
is expected. Your file is done when **no compiler error points into it**:

```
cargo check -p epiphany-ops --all-targets 2>&1 | grep -n '<your file>'
```

must print nothing. Errors located in other files, or in `payload.rs` from a
macro expansion, are not yours.

A previous agent on this project reported "verification passes" when errors did
in fact point into its own file. **Paste the actual command and its actual
output in your report.** If the crate does not link, say so plainly and say your
tests were therefore not executed.

### Tests you must write

Under `#[cfg(test)] mod tests` in your own file:

1. **Round-trip** for every production you implement: build a value, `project()`,
   `render()`, `read_sexp()`, `parse()`, assert equal to the original, and assert
   the re-rendered text is byte-identical.
2. **Rejection** for every strictness rule you implement — a text that is
   well-formed but not canonical must be rejected, not normalized. Assert the
   rejection, not the message.
3. **Mutation-verify every order check and every guard you write.** For each:
   assert the anchor text is present, delete or invert the check, confirm a named
   test of yours fails, restore. A `str.replace` that matches nothing looks
   exactly like a passing test. **Report the result per check, with output.** If
   a check survives deletion, say so — that is a finding, not a failure, and it
   means the check is dead or the test is blind.

Use `epiphany_core::textvalue::read_sexp` to parse test text.
