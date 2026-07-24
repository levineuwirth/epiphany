# Plan — the genesis operation tranche: scope, ladder, and open rulings

**Governed by** `spec/RULING_GENESIS_PERSISTENCE.md` (ratified 2026-07-24,
011c68a; `identity` sub-decision ruled 2026-07-24, ec17d06). That ruling
reverses Pass-12 K8 and makes every mutable field of `Score` operation-authored.
This plan is the execution scope: what the tranche touches, in what order, and
which questions must be answered before a dispatch contract can be written.

**Status:** scoped, not dispatched. §6 lists what needs ratification first.

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

### G2 — the three settings setters

`SetCanvasLayoutDefaults`, `SetSpellingPrecedence`, `SetTuningContext` on the
`SetMetadata` LWW pattern (`reduce.rs:2713`), seeded for value-restoring undo
(`:1357`). This is where the **one accept-set raise is spent**: `bundle.rs`'s
`max_supported_major(OperationEnvelopeBlock)` moves 2 → 3.

Note `bundle.rs:58` documents the current cap **with the tuning-context
rationale in prose** — "no operation payload embeds the tuning context, so no op
block is ever born at v3". That comment becomes false the moment this lands and
must move with the cap. Same for `DECISIONS.md`'s superseded prohibition, which
is already marked.

**P13-S13 closes here.**

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
5. **`Score::empty` seeds `tuning_context` with a default**, so
   `SetTuningContext`'s reduction must distinguish "never authored" from
   "authored to the default value" if undo is to restore correctly.

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
4. **Ladder shape.** G1/G2/G3 as above, or a different cut.

*Related: `spec/RULING_GENESIS_PERSISTENCE.md`, `spec/ANALYSIS_GENESIS_PERSISTENCE.md`,
`spec/PLAN_EDITOR_APP.md` §Ruling B / §3.7, `spec/PLAN_PUSH4B_TUNING.md` (the
tranche mold), `spec/PASS13_CANDIDATES.md` (S13).*
