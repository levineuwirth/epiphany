# Plan — the genesis operation tranche: scope, ladder, and open rulings

**Governed by** `spec/RULING_GENESIS_PERSISTENCE.md` (ratified 2026-07-24,
011c68a; `identity` sub-decision ruled 2026-07-24, ec17d06). That ruling
reverses Pass-12 K8 and makes every mutable field of `Score` operation-authored.
This plan is the execution scope: what the tranche touches, in what order, and
which questions must be answered before a dispatch contract can be written.

**Status:** the ladder **G1 → G2a → G-minor → G2b** is **complete**; only **G3**
remains.

* **G1 landed** (3b09595) — `CreateInstrument`, kind/tag 31.
* **G2a landed** (7df5ca1 + 55eff00) — `SetCanvasLayoutDefaults` and
  `SetSpellingPrecedence`, kinds/tags 32/33, both major 0.
* **G-minor landed** (ff9bd0f) — the chunk schema minor became a derived
  record; epoch ladder ratified at minors 2–9 (`spec/PLAN_GMINOR_SCHEMA_MINOR.md`
  §4), governed by `spec/AUDIT_GMINOR_VOCABULARIES.md`. Closed **P13-S14**.
* **G2b landed** — `SetTuningContext`, kind/tag **34**, schema major **3**,
  minor epoch **10**. Carried the sole accept-set raise
  (`OperationEnvelopeBlock` 2→3) and rewrote the `bundle.rs` rationale it
  falsified. Payload is the five-field subset
  `epiphany_core::TuningContextSettings`, **not** the full graph type — §5
  trap 7's holdout, resolved in the contract as *subset over normalization*.
  Closed **P13-S13**.
* **G3** — the five remaining mint families, plus the delete/modify coverage
  design. Scoped, not contracted.

§6 lists what still needs ratification. **Standing constraint, now with real
teeth:** pruning MUST NOT be implemented until disposition C lands — after G2b
it would discard *authored* genesis state, not merely re-derivable state.

**G1 shipped documentation debt** — five normative falsehoods across
`binary_format.tex`, `core_spec.tex`, and `operation_catalog.tex`, because its
contract declared the normative wire surfaces out of scope. G2a repairs them.
The lesson generalises: **an operation-vocabulary append is a documented event
in four specification documents**, and any contract on this track that does not
name them all is wrong. See §4's split-cost accounting.

---

## 1. The mechanism, and why the tranche is tractable

Every one of the nine surfaces carries a `Score` field whose type already has a
`Codec` in `epiphany-core`. `Codec` is `pub(crate)`, so operations cannot use it
— but they do not need to. The established seam is `CanonicalValue`
(`codec.rs:3443`), and the template is one line:

```rust
impl CanonicalEncode for CreateStaffOp {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_lp_bytes(out, &self.staff.canonical_bytes());   // payload.rs:1411
    }
}
```

`canonical_value!` (`codec.rs:3455`) implements the trait by delegating to the
existing `Codec`, **introducing no new byte layout**, and its generated
`decode_canonical` already does decode → `finish()` → re-encode → reject on
mismatch. So every new operation payload gets strict canonical-form enforcement
for free, on the same seam the decode-vector corpus uses.

Concretely: the tranche adds the eight remaining carried types to
`canonical_value!` and writes one `push_lp_bytes` line per op. It does **not**
design wire layouts. Every layout it carries is already frozen and already
shipping inside `Score`.

## 2. Two facts that de-risk this, and one that does not

**The tag layer is now safe.** `operation_kind_tag_vocabulary!`
(`payload.rs:440`) is a single source of truth generating the discriminant map,
its inverse, and `PAYLOAD_FREE` — which the decoder, fuzz corpus, conformance
vectors, and edit-barrier round-trip all read. A variant added to
`OperationKindTag` without an entry **fails to compile**. This macro exists
because Push 4a added `TransposeInterval` to a hand-written match and nothing
else: the decoder rejected its own encoding, an edit barrier naming it could not
be reopened, and four hand-maintained lists — two asserting the tag was
*unknown* — stayed green.

**Canonical-form enforcement is inherited, not written.** Per §1.

**But `OperationKind::discriminant()` is still a hand-written match**
(`payload.rs:253`), and it is a *different* space from the tag space. They are
not aligned and must not be assumed so — `RespellPitch` is kind 2 and tag 3;
`InsertEvent` is 0 in both. The tag space reserves 16 for `Registered`; the kind
space does not. Both are append-only under `req:binfmt:kind-discriminants`.
Next free: **kind 31, tag 31.**

## 3. The nine surfaces, and what each costs

Only one of the nine forces the accept-set raise. Verified by reading each
carried type's fields for mandatory (non-`Option`) appends above major 0:

| Surface | Op | Carried type | `schema_major()` | Raise? |
|---|---|---|---|---|
| `instruments` | `CreateInstrument` | `Instrument` | **2** (mandatory `sound_config`, `default_clef`) | no |
| `canvas.layout_defaults` | `SetCanvasLayoutDefaults` | `CanvasLayoutDefaults` | 0 | no |
| `spelling_precedence` | `SetSpellingPrecedence` | `SpellingPrecedence` | 0 | no |
| `staff_groups` | `CreateStaffGroup` | `StaffGroup` | 0 | no |
| `parts` | `CreatePartDefinition` | `PartDefinition` | 0 | no |
| `analysis_layers` | `CreateAnalysisLayer` | `AnalysisLayer` | 0 | no |
| `views` | `CreateView` | `ViewDefinition` | 0 | no |
| `StaffInstance.measures` | `CreateMeasure` | `Measure` | 0 | no |
| `tuning_context` | `SetTuningContext` | `ScoreTuningContext` | **3** | **YES** |

The op-block accept-set is already 2 (`bundle.rs:69`), so **`CreateInstrument`
at major 2 costs nothing** — it sits exactly where `CreateStaff` already sits.
`SetTuningContext` is the sole surface that drags `OperationEnvelopeBlock` to 3.

Each op touches roughly thirteen places: the op struct, the `OperationKind`
variant, `discriminant()`, `tag()`, `schema_major()`, `CanonicalEncode`, the tag
vocabulary entry, the `envdecode.rs` decode arm and its validation, the
`reduce.rs` arm, both `textproj_kind.rs` arms (production `:217`, parse `:523`),
`validate.rs`, `vectors.rs`, the fuzz/`valuegen.rs` generators, plus
`operation_catalog.tex`. Nine surfaces at thirteen touch points is not one
dispatch.

## 4. The ladder

### G1 — the spine (`CreateInstrument` only)

**`CreateInstrument` is the single missing link between an empty score and a
note.** `Score::empty` (`graph.rs:1747`) gives `Canvas::default()` and empty
vectors; from there the chain is

```
Score::empty → CreateInstrument (MISSING) → CreateStaff ✓ → CreateRegion ✓
             → CreateStaffInstance ✓ → CreateVoice ✓ → InsertEvent ✓
```

Every arrow but the first already exists and already has graph-aware
preconditions. So one operation satisfies the ruling's acceptance criteria 1 and
3 — a document created empty and given only operations materializes a
note-bearing `Score`, and opening it from a bundle reaches a note, which is what
unblocks T1b.

G1 also carries the two constraints that are not per-surface:

* **The identity cursor.** From-empty reduction sets `next_counter` to
  `1 + max(counter)` over ids authored by the reducing replica in the log,
  leaving the seed untouched when that replica authored none (ruling §3). This
  is the first time reduction writes `identity` at all — `epiphany-ops` has no
  `.identity` reference today. Wants a test that mints from a reduced score and
  asserts no collision with the log.
* **From-empty must reduce with a graph.** `new_onto` with an empty score
  enforces preconditions from the first operation; the base-free mode skips them
  by design, because it has no universe to check against (`reduce.rs:3721`,
  `:3824`). A from-empty document through the wrong entry point silently loses
  referential enforcement. Name it and test it.

No accept-set raise. No wire change. Highest value per unit of risk in the whole
tranche.

### G2 — the settings setters, split in two

All three ride the `SetMetadata` LWW pattern (`reduce.rs:2814`), seeded for
value-restoring undo (`:1385`). But they do **not** sit at the same major, and
only one of them is a compatibility event. Verified 2026-07-28 against the
working tree:

* `SetSpellingPrecedence` → `SpellingPrecedence`. Major **0**: the frozen
  `decode_v0_score`/`decode_v1_score`/`decode_v2_score` walks and the live
  `Codec` all read this field through plain `Codec::dec` (`codec.rs:2673`,
  `:3249`, `:3372`) — it has never been versioned.
* `SetCanvasLayoutDefaults` → `CanvasLayoutDefaults`. Major **0**. The type is
  labelled "schema major 1" (`graph.rs:836`), but the versioning lives in the
  *containing* `Canvas` walk, not the leaf: `dec_canvas_v0` (`codec.rs:2775`)
  default-fills the whole field while `enc_canvas_v1` (`:3158`) writes it
  through the live `Codec`. As a standalone payload it has exactly one layout.
* `SetTuningContext` → `ScoreTuningContext`. Major **3**, and
  **unconditionally** so. `enc_tuning_context_v2` (`codec.rs:3293`) writes three
  fields; the live `Codec` writes five. A default `smufl` and an empty
  `overrides` still append bytes, so there is no value for which a lower-major
  layout exists — this is a `CreateInstrument`-shaped arm, not a
  `CreateRegion`-shaped one.

So the split is not tidiness. Two of the three move no wire bound at all, and
the accept-set raise is a **one-way door**: once a block can be born at v3, an
older reader meeting one preserves the bundle read-only (`bundle/src/error.rs:257`).
Spending that in the same packet as two major-0 leaf setters buries it.

**G2a — `SetCanvasLayoutDefaults` + `SetSpellingPrecedence`.** Both land in
`schema_major()`'s catch-all `_ => 0` arm (`payload.rs:262`) with **no arm
added**; adding them to the `=> 2` arm would be the bug. No `epiphany-bundle`
change of any kind.

**G2b — `SetTuningContext` alone**, carrying the raise, the `bundle.rs` prose,
and the S13 close. **Sequenced after G-minor, not immediately after G2a** — see
that section: G2b appends kind 34, and the sweep is scoped to 24–33. `bundle.rs:58` documents the current cap **with the
tuning-context rationale in prose** — "no operation payload embeds the tuning
context, so no op block is ever born at v3". G2b is precisely what falsifies
that sentence, so the comment must move with the number. Same for
`DECISIONS.md`'s superseded prohibition, which is already marked.

**The cost of splitting, stated honestly — and it is larger than first
written.** Each packet appends to the operation vocabulary, and that is a
documented event in **three** companions, not one:

* `text_projection.tex` — a new *kind* production is a document-surface change
  (the G1 precedent), so `COMPANION_VERSION` bumps per packet: 0.8.0 → 0.9.0 →
  0.10.0, each re-sweeping five live version sites plus a changelog row and
  re-flipping the negative `superseded_companion_version` vector.
* `binary_format.tex` — payload-layout and tag rows per kind, plus a version
  bump and a Revision History row (the 0.2.0 entry is the precedent).
* `operation_catalog.tex` — a `\section` per kind, plus a version bump and a
  changelog paragraph.

Plus `core_spec.tex`'s normative `OperationKind`/`OperationKindTag` listings
and its spelled-out payload counts, and a regenerated vector corpus, per
packet. So the split roughly **doubles the documentation work**, and G2b pays
it again in full. Still the cheaper of the two risks — burying a one-way
accept-set door in a packet of routine work is worse than repeating a
mechanical sweep — but it is not the small tax the first draft implied.

**P13-S13 closes at G2b — and on the metadata precedent, not on the canonical
base.** The base cannot carry a v3 tuning context: it is role-bound to major 0
(`mis_stamped_canonical_base`, `bundle.rs:866`). It does not need to. The
canonical base is a `MaterializedState` (`reduce.rs:504`) — effects, conflicts,
anomalies, objects, spellings, breaks, page-breaks, pending — which embeds **no
graph values for any field**, including `metadata`, op-authored since M2d and
durable purely through its operations. S13's claim was "no canonical carrier
embeds it at all: no operation authors it". G2b makes an operation author it,
and the op log is canonical.

**What that sharpens.** The standing prohibition on pruning (blocked on
disposition C) stops being a performance concern the moment G2b lands: pruning
would then discard authored genesis state, not merely re-derivable state. G2b's
contract must state this as an explicit non-goal.

**G2b holdout — `accidental_extensions`, and why a naïve full-value
`SetTuningContext` would be wrong.** `ScoreTuningContext`'s `Codec`
**deliberately drops** `accidental_extensions` on encode and default-fills it
to `Vec::new()` on decode (`core/src/codec.rs:1939`) — the field is staged out
of schema major 3 and lands at a later one. Meanwhile `OperationSet::accept`
stores the authored envelope **as an object**, not as bytes
(`ops/src/opset.rs:70`). So a `SetTuningContext` carrying a non-empty
`accidental_extensions` would reduce with those extensions present on the
authoring replica, and reduce *without* them on any replica that received the
document through serialization — a silent divergence between a live session and
the same document reloaded.

**`canonical_value!` does not catch this.** Its generated `decode_canonical`
compares *bytes* (decode → `finish()` → re-encode → reject on mismatch); it
never compares against the originating value, so a field that never reached the
bytes is invisible to it. **G2b needs a normalization-or-subset pin before
dispatch** — either the payload carries a wire-complete subset type, or the
operation normalizes the field away at construction and refuses a non-empty
one. Decide that in the G2b contract, not in its implementation. Does not block
G2a.

### G-minor — the schema-minor sweep (ruled 2026-07-28)

**Ladder order is G2a → G-minor → G2b, and the order is load-bearing.** The
sweep is scoped to kinds 24–33, which is exactly what exists once G2a lands. Run
G2b first and it appends kind 34 into a vocabulary the sweep has already been
scoped against, so either the sweep grows mid-flight or 34 ships with the same
defect the rung exists to retire. G2b therefore sequences *after* G-minor and
inherits working machinery — the same reasoning that put the accept-set raise in
its own packet.

`binary_format.tex:2330` requires a writer to raise the chunk schema **minor**
when it emits a discriminant appended after the minor it declares — a MUST with
a stated rationale, so that an unknown-discriminant decode failure is
attributable to version skew rather than corruption. No writer has ever done
it: `SchemaVersion::for_major` (`bundle/src/ids.rs:204`) maps a major to a fixed
constant — `V0` is `{0, 1}`, not `{0, 0}` (`ids.rs:173`); `V1`/`V2`/`V3` carry
minor 0 — and, decisively, **accepts only a major**, so no per-kind additive
minor can reach it. Both staging paths likewise derive only the major
(`testkit/src/bundle_harness.rs:25`, `textproj/src/serialize.rs:183`).

So kinds **24–27**, **28–29**, **30**, and **31** already carry no additive
record, and G2a takes that to **32–33** knowingly. Filed as **P13-S14**. Ruled:
one retroactive sweep over 24–33 after G2a, rather than blocking G2a on a debt
already eight kinds deep or paying for two partial sweeps.

What the rung owes — **policy and epoch ladder both ratified 2026-07-28**; see
`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4 and the governing inventory
`spec/AUDIT_GMINOR_VOCABULARIES.md`:

* a **global additive epoch** (minors 2–9, one per additive event), **not** the
  per-kind minor this paragraph originally proposed — that was rejected,
  because it cannot represent multiple independent vocabularies inside one
  block, and its obvious generalisation is worse: an old `OperationKind` 23
  would numerically mask a newly appended `OperationPayload` 3;
* `introduced_minor` assigned **per variant, per vocabulary**, co-located with
  the discriminant in an exhaustive macro/match with **no wildcard arm**, so a
  new variant cannot compile without an epoch;
* an **envelope's** required minor = the max over **every discriminant it
  actually emits** (outer payload variant, primitive kind, every nested
  additive variant), and a **block's** = the max over its envelopes;
* **content-minimal derivation for the other payload roles** — including the
  manifest, which takes a barrier tag's epoch when it names one;
* a `for_major` replacement that accepts a minor, and both staging paths.

**The scope is not "kinds 24–33."** The audit found appends in three further
vocabularies (`OperationPayload` 3, `ReanchorReason` 6,
`PreconditionFailureReason` 10–15) and a reachability path to
`OperationKindTag` through the *manifest* with no operation envelope involved.
Orthogonal to the *major* accept-set, which stays 2 through G2a and rises to 3
only at G2b.

### G3 — the remaining entity families

`CreateStaffGroup`, `CreatePartDefinition`, `CreateAnalysisLayer`, `CreateView`,
`CreateMeasure`, on the `CreateStaff` set-union mint pattern (`reduce.rs:3850`)
with byte-identical re-carry idempotence, plus whatever delete/modify coverage
§6.1 rules owed. Graph-aware referential preconditions per the ruling §2:

* `CreateStaffGroup.members`, `CreatePartDefinition.staves` → live `Staff`s
* `CreateView.active_layers` → live `AnalysisLayer`s
* `CreateMeasure` → a live `StaffInstance`
* deleting an entity with live dependents → refuse (container-not-empty)

`CreateMeasure` is shaped differently from its five siblings: `measures` is
nested on `StaffInstance` (`graph.rs:611`), not a `Score`-level vector, so its
precondition reaches three levels down through `canvas.regions[].staff_instances()`.
Consider splitting it out if G3 runs long.

## 5. Traps

1. **Two unaligned discriminant spaces**, one macro-guarded and one not (§2).
2. **`bundle.rs`'s cap comment is prose that encodes a rationale**, not just a
   number. Moving the number without the prose leaves a confident falsehood in
   the file that most directly governs accept-sets.
3. **Minimal stamping is a pure function of the value.** `CreateInstrument` is
   *unconditionally* 2 (its major-2 appends are not `Option`s), unlike
   `CreateRegion`/`SetStaffLayout`, which are value-dependent. Do not copy the
   value-dependent arm shape.
4. **Round-trip locking cannot see a self-consistent reorder.** The lesson of
   3b-i: a swap applied to both codec halves passed 1283 tests and 8/8
   conformance. New payloads want decode-vector entries pinned to literal bytes,
   not just round-trip tests.
5. ~~**`Score::empty` seeds `tuning_context` with a default**, so
   `SetTuningContext`'s reduction must distinguish "never authored" from
   "authored to the default value" if undo is to restore correctly.~~
   **Withdrawn 2026-07-28 — this is not a trap, and `SetMetadata` already
   proves it.** `Score::empty` seeds `metadata` with a default exactly as it
   seeds `tuning_context` (`graph.rs:1749`, `:1757`), and the base ingest then
   runs `metadata_chain.seed(score.metadata.clone())` (`reduce.rs:1385`) under
   a comment stating the purpose outright: the score-level LWW chains seed with
   the base values so a value-restoring undo of the *first* operational write
   restores the pre-operational state. Since from-empty reduces **onto**
   `Score::empty` (trap-free only through `reduce_operation_set_onto` — G1 pin
   10), the seed runs, and undoing the first write yields
   `Restore(Some(Predecessor::Base(default)))`. Restoring the default is
   correct in both the never-authored and the authored-to-default case, so the
   distinction is unobservable **and must stay so**. The `Predecessor::Base`
   vs `::Write` distinction earns its keep only for the canonical bookkeeping
   families (`spellings`, `breaks`, `page_breaks`, `reduce.rs:707`), where a
   base predecessor returns a *map key* to absence. `ScoreTuningContext` is an
   always-valued `Score` field, like metadata: there is no absent state to
   return to. All three G2 setters copy `set_metadata` structurally.
6. **Adding an `OperationKind` variant is NOT containable to core + ops** —
   the G1 lesson, and the one claim this plan previously got wrong. Rust
   exhaustiveness forces an arm in `epiphany-editor-core`'s `subjects_of`
   (`barriers.rs:437`), and because `epiphany-testkit` depends on editor-core,
   a missing arm blocks conformance *and* `requirement_labels` — the gate
   cannot run at all. Three further sites bake in a literal that only surfaces
   once the workspace compiles: `layout-ir/src/barrier.rs:1105` (a tag
   "one past the vocabulary"), `testkit/tests/text_projection_grammar.rs:307`
   (a hardcoded kind *count*), and `textproj/src/vectors.rs` (a negative vector
   whose "wrong version" is the one each bump moves to). Every G2/G3 contract
   MUST enumerate these and budget the boundary crossing up front.

## 6. Open rulings — needed before a dispatch contract

1. **Delete/modify coverage per family.** The ruling leaves this as the
   contract's design work and explicitly declines to assume full CRUD:
   `CreateStaff` ships today with no `DeleteStaff`. Group 3's precedent is
   "mint + empty-only delete" for containers. Which families get deletes in G3?
2. **`decomposition_attachments`.** The ruling calls it derived, not authored —
   the prepass creates it (`prepass.rs:382`) and reduction only ever *retains*
   (`reduce.rs:2342`, its sole mention). It leaves the eight-field table rather
   than gaining operations. **Flagged for ratification with the tranche.**
   Verified: the citation is accurate.
3. **The measure/meter invariant.** Measures are authored, not derived (ruling
   §2), so measure/meter consistency becomes an authoring obligation backed by a
   graph invariant. That invariant needs specifying — it belongs in G3.
4. ~~**Ladder shape.** G1/G2/G3 as above, or a different cut.~~ **Ratified
   2026-07-24**, and amended 2026-07-28: G2 splits into **G2a** (the two
   major-0 setters) and **G2b** (`SetTuningContext` alone, carrying the
   accept-set raise and the S13 close). See §4.

*Related: `spec/RULING_GENESIS_PERSISTENCE.md`, `spec/ANALYSIS_GENESIS_PERSISTENCE.md`,
`spec/PLAN_EDITOR_APP.md` §Ruling B / §3.7, `spec/PLAN_PUSH4B_TUNING.md` (the
tranche mold), `spec/PASS13_CANDIDATES.md` (S13).*
