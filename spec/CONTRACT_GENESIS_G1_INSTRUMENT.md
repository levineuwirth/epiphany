# Contract: genesis tranche G1 — `CreateInstrument`, and the from-empty spine

Repo root `/home/jeans/Repos/active/epiphany`. Governed by
`spec/RULING_GENESIS_PERSISTENCE.md` (011c68a; `identity` sub-decision ruled at
ec17d06) and `spec/PLAN_GENESIS_OPS.md` §4 (439e1e2), whose G1/G2/G3 ladder the
user ratified 2026-07-24.

Execution model as every tranche on this track: Sonnet subagent, coordinator
line-level review with **independent mutation re-runs**, user deep-dive at
contract sign-off and final report. Mutation discipline throughout: anchor-assert
before substituting, restore by reversing, never `git checkout`.

**Parallel safety.** The editor track owns `epiphany-editor-core`,
`epiphany-editor-gui`, `epiphany-layout-ir`, `epiphany-render-svg`, the new
`epiphany-glyphs` crate, every `crates/epiphany-editor-gui/goldens/*.png`, and
`spec/PLAN_EDITOR_APP.md` + `spec/CONTRACT_EDITOR_*.md`. **This packet touches
none of them.** Do not stage them; do not `git add -A`.

---

## What this packet does, in one sentence

Adds one operation kind, `CreateInstrument`, so that a `Score::empty(identity)`
plus an operation log can materialize a note-bearing score — and makes the
from-empty path correct in the two ways the ruling requires.

## Why it is this small

`CreateInstrument` is **the single missing link** between an empty score and a
note. `Score::empty` (`core/src/graph.rs:1747`) yields `Canvas::default()` and
empty vectors; from there the chain is

```
Score::empty → CreateInstrument (MISSING) → CreateStaff ✓ → CreateRegion ✓
             → CreateStaffInstance ✓ → CreateVoice ✓ → InsertEvent ✓
```

Every later arrow exists today with graph-aware preconditions. `CreateStaff`
already requires a **live `Instrument`** (`reduce.rs:3824-3833`) — which nothing
can currently create. That is the whole gap.

`Instrument` (`core/src/graph.rs`, fields `id, name, range, abbreviation,
sound_config, transposition, default_clef, default_staff_lines,
unpitched_members`) holds **no outbound entity references**. It is a root, so
this operation needs **no referential preconditions** — only mint/idempotence.

## Design pins

1. **No wire layout is designed.** `Instrument` already has a `Codec` and
   already ships inside `Score`. Add `Instrument` to `canonical_value!`
   (`core/src/codec.rs:3484`) and encode the payload as
   `push_lp_bytes(out, &self.instrument.canonical_bytes())` — the
   `CreateStaffOp` template verbatim (`ops/src/payload.rs:1411`). The generated
   `decode_canonical` already rejects non-canonical encodings, so strict-form
   enforcement is inherited, not written.
2. **Discriminants: kind `31`, tag `31`.** Both spaces currently top out at 30
   (`TransposeInterval`). **They coincide here by accident, not by rule** — the
   two spaces are independent and misaligned elsewhere (`RespellPitch` is kind 2
   / tag 3; `CreateVoice` is kind 19 / tag 20). Append-only under
   `req:binfmt:kind-discriminants`.
3. **Name it `CreateInstrument` in both spaces**, catalog name
   `"create-instrument"`. The tag layer's older Create→Insert convention
   (`InsertStaff` for kind `CreateStaff`) is **not** followed: the two most
   recent additions, `CreateVoice` and `CreateRepeatStructure`, use `Create` in
   the tag, and a gratuitous homonym costs more than the convention buys.
4. **`schema_major()` is unconditionally `2`.** `Instrument`'s major-2 appends
   (`sound_config`, `default_clef`, `default_staff_lines`) are **mandatory, not
   `Option`-hidden**, so no lower-major layout for this payload exists. Put it in
   the existing `| OperationKind::CreateStaff(_) | OperationKind::SetMetadata(_)
   => 2` arm (`payload.rs:219`). **Do not** copy the value-dependent arm shape
   used by `CreateRegion`/`SetStaffLayout`.
5. **No accept-set change.** `max_supported_major(OperationEnvelopeBlock)` is
   already 2 (`bundle.rs:69`) and **stays 2**. Do not touch `epiphany-bundle`.
   The raise to 3 belongs to G2 (`SetTuningContext`) and to nothing else.
6. **Reduction follows `create_staff` exactly** (`reduce.rs:3794-3855`): live +
   byte-identical value → `NoOp{AlreadyApplied}`; live + differing value →
   `NoOp{PreconditionFailedUnderReduction{RecreateContentMismatch}}`; tombstoned
   → `NoOp{TargetTombstoned}`; otherwise `mint_container` + `Applied`.
7. **No `DeleteInstrument`.** `CreateStaff` ships today with no `DeleteStaff`;
   the ruling states full CRUD is not automatically owed. Delete coverage is
   G3's design work.

## The two cross-cutting items

These are the reason G1 is not merely "one more kind".

8. **`instrument_values` must exist and be seeded.** `TypedObjectId::Instrument`
   liveness is already seeded from the base (`reduce.rs:1319-1321`), but there
   is **no value map**, so re-carry idempotence against a *base* instrument has
   nothing to compare. Add `instrument_values: BTreeMap<InstrumentId, Instrument>`
   mirroring `staff_values` at every one of its sites: the two struct decls
   (`:914`, `:1004`), the init (`:1282`), the base seed (beside `:1327`), and the
   snapshot/restore pair (`:7234`, `:7270`). **Missing the snapshot/restore pair
   is the silent failure here** — it would surface only under undo/replay.
9. **The identity cursor** (ruling §3). From-empty reduction sets
   `next_counter` to `1 + max(counter)` over ids authored by the reducing
   replica in the log, leaving the seed untouched when that replica authored
   none. This is **the first time reduction writes `identity` at all** —
   `epiphany-ops` has no `.identity` reference today. Note `Score::identity` is
   an authoring cursor reduction has never advanced; under from-empty it would
   otherwise sit at the seed while the log already holds that replica's ids at
   `0..N`, so minting from a reduced score re-issues used counters. Nothing
   catches that: invariant 11 (`core/src/invariants.rs:1746-1752`) checks only
   the reserved replica, never the counter.
10. **From-empty must reduce with a graph.** Use
    `reduce_operation_set_onto(&set, &Score::empty(identity))`
    (`reduce.rs:617`), **never** `reduce_operation_set(&set)` (`:612`).
    Graph-aware preconditions are skipped in the base-free mode *by design* —
    it has no universe to check against (`reduce.rs:3721`, `:3824`). A
    from-empty document through the base-free entry point silently loses
    referential enforcement. Document this at both entry points.

## Touch points

Derived by enumerating every `CreateStaff` site. Twelve non-test surfaces —
**two of which the plan under-counted** (`v0.rs`, `migrate.rs`):

| # | File | Site (CreateStaff analogue) |
|---|---|---|
| 1 | `core/src/codec.rs` | `canonical_value!` — add `Instrument` (`:3484`) |
| 2 | `ops/src/payload.rs` | `CreateInstrumentOp` struct + `CanonicalEncode` (`:1400`, `:1411`) |
| 3 | `ops/src/payload.rs` | `OperationKind` variant (`:177`) |
| 4 | `ops/src/payload.rs` | `schema_major()` → 2 arm (`:219`) |
| 5 | `ops/src/payload.rs` | `discriminant()` → 31 (`:280`) |
| 6 | `ops/src/payload.rs` | `tag()` (`:323`) + encode dispatch (`:367`) |
| 7 | `ops/src/payload.rs` | `operation_kind_tag_vocabulary!` entry (`:440`) — **compile-enforced** |
| 8 | `ops/src/envdecode.rs` | discriminant-31 decode (`:542`) + tag→kind (`:830`) |
| 9 | `ops/src/v0.rs` | `V0OperationKind` variant (`:96`) |
| 10 | `ops/src/migrate.rs` | both directions (`:171`, `:325`) |
| 11 | `ops/src/reduce.rs` | dispatch (`:2632`) + `create_instrument` + pin 8's map |
| 12 | `ops/src/textproj_kind.rs` | production (`:185`) + parse (`:470`) |
| 13 | `ops/src/fuzz.rs` | generator arm (`:214`) |
| 14 | `ops/src/vectors.rs` | a decode vector for the new payload |
| 15 | `spec/operation_catalog.tex` | §`CreateInstrument` |

The tag vocabulary macro (#7) is a **single source of truth**: the decoder, fuzz
corpus, conformance vectors, and edit-barrier round-trip all read `PAYLOAD_FREE`,
and a variant missing an entry **fails to compile**. It exists because Push 4a
added `TransposeInterval` to a hand-written match and nothing else, and four
hand-maintained lists stayed green while the decoder rejected its own encoding.
`OperationKind::discriminant()` (#5) has **no such guard** — it is hand-written.

## Tests + minimum mutations

Every test must be mutation-verified: re-introduce the bug it exists to catch and
show it dies. A test that cannot see its own bug is not a test.

* **(i1) the spine reaches a note.** From `Score::empty(identity)`, operations
  only — `CreateInstrument` → `CreateStaff` → `CreateRegion` →
  `CreateStaffInstance` → `CreateVoice` → `InsertEvent` — materializes a
  note-bearing `Score`. No fixture, no base. *This is the packet's load-bearing
  test and the ruling's acceptance criterion 1.* **Mutation:** drop the
  `CreateInstrument` envelope → `CreateStaff` must `NoOp{TargetMissing}` and no
  note appears.
* **(i2) re-carry idempotence.** The same `CreateInstrument` twice →
  `AlreadyApplied`, and the state is byte-identical. **Mutation:** make the
  second carry differ in one field → `RecreateContentMismatch`.
* **(i3) re-carry against a *base* instrument** — the pin-8 case. Reduce onto a
  base that already contains the instrument. **Mutation:** skip seeding
  `instrument_values` → the byte-identical re-carry is misreported as a
  mismatch.
* **(i4) order independence.** Two replicas applying a concurrent genesis-era
  set in opposite delivery orders converge to byte-identical
  `MaterializedState` (ruling acceptance criterion 2; the existing pattern is
  `reduce.rs:10031`). **Mutation:** perturb one op's reduction order.
* **(i5) the identity cursor.** Mint from a reduced-from-empty score and assert
  the id collides with nothing in the log. **Mutation:** leave `next_counter` at
  the seed → collision.
* **(i6) base-free loses enforcement.** Assert explicitly that
  `reduce_operation_set` (base-free) does **not** enforce the instrument
  precondition while `reduce_operation_set_onto` does. This test documents a
  designed asymmetry rather than guarding a bug — state that in its doc comment
  so a later reader does not "fix" it.
* **(i7) minimal stamping.** `CreateInstrument.schema_major() == 2`
  *unconditionally* — including for an instrument whose optional fields are all
  `None`. Extend `schema_majors_follow_the_minimal_stamping_rule`
  (`reduce.rs:10386`). **Mutation:** make the arm value-dependent → dies.
* **(i8) decode vector pinned to literal bytes**, not round-trip. Round-trip
  locking cannot see a self-consistent encoder/decoder reorder — that is the
  3b-i lesson, where a swap applied to both halves passed 1283 tests and 8/8
  conformance.
* **(i9) text-projection round-trip** for the new kind, matching the existing
  per-kind coverage.

## Blast radius

`crates/epiphany-core/src/codec.rs` (one `canonical_value!` line) and its
`DECISIONS.md`; `crates/epiphany-ops/src/{payload,envdecode,v0,migrate,reduce,textproj_kind,fuzz,vectors}.rs`
and its `DECISIONS.md`; `spec/operation_catalog.tex` (+ its rebuilt PDF — every
other `.tex` commit carries one). **Nothing else.** No `epiphany-bundle`, no
`binary_format.tex`, no editor-track crate, no golden re-blessing.

## Gate (report actual output, never stale numbers)

`cargo fmt --check`; `cargo clippy --workspace --all-targets` at **0** warnings;
`cargo test --workspace` at **0** failed; `cargo test --doc` at 0 failed;
conformance **8/8**; `requirement_labels` **6/6** with its three observed counts
reported as seen (they were 212/282/282 at 439e1e2 and will move with the
catalog section). Plus:

* `max_supported_major(OperationEnvelopeBlock)` is **still 2** — assert it, and
  state it in the report.
* No `crates/epiphany-editor-gui/goldens/*.png` byte changes.

## What I will verify independently before committing

Build to survive this. I re-run every mutation myself, and I check specifically:
that #5's hand-written discriminant match and #7's macro entry agree; that pin
8's snapshot/restore pair (`:7234`/`:7270`) was not missed; that i1 fails for the
stated reason rather than incidentally; that the accept-set really did not move;
and that no claim in the report is copied forward from this contract rather than
observed — the recurring failure mode on this project is a plausible claim
propagating because nobody re-derived it.

## Report

Files + summary, exact asserted values, every mutation with kill evidence, gate
output verbatim, deviations flagged explicitly. If any pin here turns out to be
wrong, say so rather than working around it silently.
