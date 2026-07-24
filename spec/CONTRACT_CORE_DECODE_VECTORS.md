# CONTRACT — the core decode-vector surface

**Status:** dispatch-ready. Delivers the future revision the Binary Format
companion already prescribes (`binary_format.tex:3294-3296`) and closes the gap
found empirically in Push 4b tranche 3b-i.

**Ratified by the user 2026-07-23:**
1. **Coverage** — the spec's five representative layouts, **plus** the
   schema-major-3 tuning-context types, **plus** one whole-`Score` vector at each
   schema major 0/1/2/3.
2. **Strictness** — `check()` calls **genuine public core API**, not a
   harness-side wrapper, so a leaf reject vector exercises production code.

---

## Why this exists

`spec/vectors/decode_vectors.txt` is normative under `req:binfmt:decode-vectors`:
it is what a *second implementation* is checked against. Today it holds 65
vectors across 5 surfaces (`ops.operation_kind_tag`, `ops.materialized_state`,
`bundle.block`, `bundle.manifest`, `bundle.operation_index`) and **nothing from
`epiphany-core`** — the score wire, the oldest and most load-bearing format in
the repo, has no cross-implementation vectors at all.

The companion already asked for this (`binary_format.tex:3294`): "A future
revision should extend the corpus to the representative struct layouts of
Section~\ref{sec:values:representative}, which remain **round-trip locked rather
than literal-byte locked**."

That is not theoretical. In tranche 3b-i the reviewer swapped `smufl` and
`overrides` in **both** halves of `ScoreTuningContext`'s codec — a
self-consistent reordering of a layout `req:binfmt:frozen-layout` freezes
permanently — and **the entire workspace suite (1283 tests) and all 8/8
conformance rows passed.** Round-trip locking cannot see that. Only literal bytes
can.

## The mechanism already exists — do not invent a new one

- **`CanonicalValue`** (`codec.rs:3435`) is **public** and already implements
  exactly the required discipline: decode, `finish()` (reject trailing bytes),
  re-encode, and reject on mismatch. It already covers **four of the spec's
  five** representative layouts — `Pitch`, `TimeAnchor`, `Event`, `Slur`.
- `Codec` and `Reader` are `pub(crate)` and **MUST stay that way.** A module
  inside `epiphany-core` can use them directly; nothing new goes public except
  the `CanonicalValue` impls below and the two `vectors` functions.
- `encode_v0_score` / `encode_v1_score` / `encode_v2_score` are `pub(crate)` —
  an in-core `vectors` module can call them to synthesize genuine old-major bytes.

## 1. Extend `CanonicalValue`

Add to the `canonical_value!` list (`codec.rs:3476`), each because the corpus
pins it:

- **`RationalTime`** — the fifth representative layout, and the only one
  missing. It **normalizes on decode** (an unreduced fraction reduces to lowest
  terms, `codec.rs:2570`), which makes it the exemplar of the spec's own warning
  that "a guard on an outer value can *mask* a lenient inner codec".
- **`ScoreTuningContext`** — the schema-major-3 container whose silent
  reorderability is the reason this tranche exists.
- **`TuningOverride`**, **`TuningScope`**, **`SmuflVersionRequirement`**,
  **`SmuflVersion`** — the four leaf layouts frozen in 3b-i.

Update the macro's doc rationale (`codec.rs:3433`, "Implemented only for the
value types operation payloads embed... to keep the public surface intentional")
to record the second reason: **and the types the decode-vector corpus pins**.
The surface stays intentional; the intent widened.

If any of these lacks a `Codec` impl, stop and report rather than inventing a
byte layout — every one of them is already encoded inside `Score`, so the layout
exists and `CanonicalValue` must introduce **no new bytes** (that is the trait's
stated contract).

## 2. New `crates/epiphany-core/src/vectors.rs`

Mirror `epiphany-ops/src/vectors.rs` in shape and doc style:

```rust
pub type DecodeVector = (&'static str, &'static str, &'static str, String, Vec<u8>);
pub fn decode_vectors() -> Vec<DecodeVector>;
pub fn check(surface: &str, bytes: &[u8]) -> Option<Result<bool, String>>;
```

`check` semantics, exactly as the other two crates: `Err` = rejected,
`Ok(injective)` = accepted with whether it re-encodes to its own bytes. Route
every leaf surface through **`CanonicalValue::decode_canonical`** — the public
production API — never a locally re-implemented strictness check.

### Surfaces

Leaves (one per `CanonicalValue` type above): `core.rational_time`,
`core.time_anchor`, `core.pitch`, `core.event`, `core.slur`,
`core.score_tuning_context`, `core.tuning_override`, `core.tuning_scope`,
`core.smufl_version_requirement`, `core.smufl_version`.

Whole `Score`, one per schema major: `core.score_v0`, `core.score_v1`,
`core.score_v2`, `core.score_v3`.

**Every surface MUST carry both an `accept` and a `reject` vector** —
`every_surface_carries_both_verdicts` enforces it. If a surface cannot be given a
meaningful reject vector, fold it into its container rather than inventing a
contrived one, and say so in your report.

### The trap in the per-major Score vectors

For `core.score_v3`, injectivity is `decoded.canonical_bytes() == bytes`.

For `core.score_v0/v1/v2` it is **NOT**. Migration deliberately produces a value
whose *current* encoding differs from the input bytes — that is the whole point
of a default-filling migration. `decode_vN_score` is already strictly canonical
**over its own wire form** (it re-encodes through `encode_vN_score` and rejects a
mismatch), so a successful `Score::decode_canonical_versioned(bytes, N)` already
proves the input was canonical at major N. Return `Ok(true)` there. **Do not
compare against `canonical_bytes()` for majors 0–2** — it will fail, and
"fixing" it by relaxing the vector would destroy the vector's meaning.

### Reject classes — principled, not arbitrary

The ops module's rule applies (`vectors.rs:10`): each class is one this
repository actually shipped a bug in, or one an injectivity fuzzer cannot see.
Draw from:

- **unreduced `RationalTime`** — the lenient-leaf normalization case;
- **non-finite float** (NaN / ±inf bits) in a `CanonicalF64` leaf
  (`NonFiniteFloat`);
- **unsorted or duplicated** `BTreeSet`/`BTreeMap` entries (order-preserving
  sequence checks are invisible to a whole-value re-encode guard);
- **out-of-range discriminant** on a tagged union (`TimeAnchor`, `TuningScope`,
  `PitchSpacePosition`);
- **trailing bytes** and **truncation**;
- **the swapped major-3 field order** — bytes encoding `ScoreTuningContext` with
  `overrides` before `smufl`. This is the direct regression vector for the 3b-i
  defect and MUST be present.

## 3. Wire it into the testkit

`crates/epiphany-testkit/src/vectors.rs`:
- `all()` (:78) — chain `epiphany_core::vectors::decode_vectors()`.
- the `check` dispatch (:165) — add `.or_else(|| epiphany_core::vectors::check(..))`.

**Append core LAST**, after ops and bundle. This is deliberate: it keeps the
regenerated corpus diff **purely additive**, which *proves* no existing vector's
bytes moved — i.e. that this tranche changed no existing wire form. A
nicer-looking dependency order would move all 65 existing lines and destroy that
property. Update `all()`'s doc comment ("the operation layer, then the bundle
wire") accordingly.

## 4. Regenerate

`cargo run -q -p epiphany-testkit --example generate_vectors`

`the_committed_corpus_matches_the_generator` fails on drift, so the regenerated
file must be committed. Confirm in your report that **the first 65 vectors are
byte-identical** to before.

## 5. Spec

Update the rationale at `binary_format.tex:3294-3296`: the "future revision"
it anticipates is now delivered for the representative layouts, and record that
the corpus additionally pins the schema-major-3 tuning-context layouts and the
per-major whole-`Score` forms. If §"The Decode Vector Corpus" enumerates
surfaces, extend that list.

**No new `\label{req:...}`** — `req:binfmt:decode-vectors` already governs this
and needs no change. Requirement counts MUST stay **212 / 282 / 282**.

## Do NOT

- Make `Codec` or `Reader` public.
- Introduce any new byte layout. `CanonicalValue` must emit exactly the bytes the
  whole-score codec already emits (its stated contract). If a value's per-type
  bytes differ from its embedded bytes, that is a bug — stop and report it.
- Change any existing vector, or any wire form.
- Touch the editor track (`spec/PLAN_EDITOR_APP.md`, `CONTRACT_EDITOR_*`,
  `crates/epiphany-editor-gui/goldens/`) or `.claude/worktrees/`.

## The gate (report exact numbers)

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets` → 0 warnings
- `cargo test --workspace` → 0 failed (HEAD is at 1318; report the count)
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
- `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8, and
  report the vector count gate [7d] prints (65 today)
- `cargo test -p epiphany-testkit --test requirement_labels` → 6/6, counts
  **212 / 282 / 282**

## Prove the hole is closed — the deliverable that matters

A corpus that does not catch the original defect is decoration. **Re-apply the
3b-i mutation** — swap `smufl` and `overrides` in *both* the `enc` and `dec`
halves of `impl Codec for ScoreTuningContext` (`codec.rs:1960`) — and confirm the
**corpus check now fails** (`every_vector_gets_its_declared_verdict` /
`the_committed_corpus_matches_the_generator`). Restore, and assert the anchor
text is back. Report the exact failure message.

That mutation previously passed 1283 tests and 8/8 conformance. If it still
passes after this tranche, the tranche did not do its job.

## What the reviewer will verify independently

- The first 65 corpus vectors are byte-identical; the diff is purely additive.
- The 3b-i swap mutation is caught by the corpus.
- The per-major `Score` vectors genuinely decode at their stamped major, and
  `core.score_v0/v1/v2` are *not* asserting `canonical_bytes()` equality.
- Every new `accept` vector's bytes equal the bytes the whole-score codec embeds
  for that value (no new layout).
- No new public API beyond the `CanonicalValue` impls and the two `vectors`
  functions; `Codec`/`Reader` still `pub(crate)`.
