# Pass 12 — G-Ratification Worklist (the batch pass)

*Purpose: retire the accumulated `PASS12_BATCH.md` backlog — 28 open rows spanning
six agents — in one deliberate G pass, before the data-model schema-major-2 push
starts consuming graph-model decisions (H2, K8) and the Standard-tier solver push
starts consuming solver-contract decisions (I4/I5/I6).*

*Scope: a **spec revision pass** plus a small, explicitly-listed code tranche.
The architecture stays frozen. Most rows bless an implemented, DECISIONS-recorded
reading; four rows carried genuine forks and were decided by the project lead on
2026-07-07 (see "Key decisions" below); three rows defer to named future tracks
with rationale.*

*Working rule (Pass-11 precedent): **adopt** = bless the implementation's existing
choice in normative spec text. **decide** = a real fork, resolved and recorded
here. **defer** = punt to a *named* landing site with rationale — never a silent
drop. Every disposition lands in `PASS12_RATIFICATION_LOG.md`; every retired row
is struck through in `PASS12_BATCH.md` citing its disposition.*

---

## Key decisions this pass (project lead, 2026-07-07)

| Row | Decision |
|---|---|
| **P12-K12** | Cross-region slur spanning permission = **AND** (both endpoint regions must permit). As implemented; advisory-only, never byte-affecting. |
| **P12-H7** | Authored spelling/decomposition attachments on inference-ineligible events **surface in derived annotations** (authored-only resolution path + new taxonomy buckets). Code work in both pre-passes. |
| **P12-K4** | ResolveConflict: **no supersede** — earliest applied resolve governs universally (concurrent *and* causally-later); later resolves and resolves against `Dismissed` read `AlreadyApplied`. Re-resolution is a future dedicated operation. No `TypedObjectId::Conflict` kind; the meta-conflict names both resolver op ids, as built. |
| **P12-K8** | **Genesis outside the operation set** — the document root and canvas are structural givens (`Score::empty` + bundle creation), never op-minted. The K1 create-score/create-canvas slots are retired as *deliberately outside the operation set*, not "unavailable". Revisit only if multi-canvas becomes a real major-2+ feature. |

---

## Bucket 1 — Adopt-and-pin (spec text only; 19 entries / 20 rows)

No code changes; each blesses a deterministic implemented reading. Golden/test
anchors already exist for the byte-adjacent ones.

### 1.1 — P12-H1 — `SpellingAlgorithmId::Default` ratified
- **Spec locus:** core_spec Ch 2 (spelling pre-pass) + Appendix D §Open Algorithm Hooks.
- **Ratify:** `"default"` = the Temperley-style line-of-fifths preference algorithm,
  v1 (`epiphany-core/src/prepass.rs`). The id is now the *spec's*, not the crate's
  proposal. `CONFORMANCE.md` already declares it; drop its "pending ratification" caveat.

### 1.2 — P12-H3 — chromatic-run convention is algorithm-defined
- **Spec locus:** core_spec Ch 2, same section as 1.1.
- **Ratify:** enharmonic choice in the absence of tonal context is a property of the
  *versioned* spelling algorithm, not spec-pinned; v1's centre-of-gravity rule with
  the ascending-sharps/descending-flats convention **as tiebreak only** is the
  ratified v1 behavior. A voice-leading refinement = a future algorithm version, not
  a spec hole.

### 1.3 — P12-H4 (+ P12-C5 folded) — decomposition v1 scope bounds
- **Spec locus:** core_spec Ch 3 (decomposition pre-pass) + Appendix D hooks;
  cross-ref operation_catalog §SetTimeSignature for C5.
- **Ratify:** the v1 `DecompositionAlgorithmId "default"` bounds become *declared
  normative bounds of the versioned algorithm*: single governing meter per region;
  region origin assumed a barline (no anacrusis); dyadic compound-meter grouping;
  no tuplet nesting / cross-beat members; `MAX_DOTS = 1`. A wider algorithm is a
  version bump that deterministically invalidates derived output (the derived-
  annotation model already guarantees this).
- **C5 disposition:** reduction semantics of a mid-region `SetTimeSignature` are
  already pinned (catalog §Meter and Tempo Overwrites); the derived-notation gap is
  *subsumed by the H4 single-meter bound* — one log entry covers both rows.

### 1.4 — P12-H5 — aleatoric spelling open question closed
- **Spec locus:** core_spec Ch 2 open-question box (aleatoric regions).
- **Ratify:** spelling is region-time-model-**independent** (pitch identity does not
  depend on the time model); there is no aleatoric-specific spelling pass. Close the
  open question with that answer; aleatoric-aware refinements are algorithm-version
  territory.

### 1.5 — P12-H6 — decomposition precedence is FIXED (decided: not configurable)
- **Spec locus:** core_spec Ch 3 §decomposition precedence.
- **Ratify:** decomposition precedence is the fixed default source order
  (`UserChosen > Imported > Propagated > Inferred`, canonical attachment order as
  tie-break) — **not** configurable. Rationale: configurability requires a new
  canonical `Score` field (a schema-major) with no consumer; `DecompositionAttachment`
  deliberately carries no `priority`. Revisit at a future major only if a use case
  appears. The "same precedence machinery as spelling" sentence is reworded to "same
  source-rank discipline" (spelling's *configurability* is spelling-specific).

### 1.6 — P12-K1 — v0 RespellPitch migration fallback
- **Spec locus:** operation_catalog §RespellPitch, §Migration.
- **Ratify:** context-recovery (explicit per-pitch attachment whose canonical bytes
  hash to the fingerprint) + `MigrationError::Irreversible` → bundle read-only when
  absent is the **long-term** disposition. No richer v0 corpus will be required
  (there is no production v0 corpus).

### 1.7 — P12-K4 — ResolveConflict beyond the concurrent case *(decided above)*
- **Spec locus:** operation_catalog §ResolveConflict; core_spec Ch 6 §Conflict Records.
- **Ratify:** earliest-applied-resolve-governs applies to causally-later resolves
  (they read `AlreadyApplied`); resolves against `Dismissed` read `AlreadyApplied`;
  intentional re-resolution is out of the v1 operation set (a future `ReopenConflict`-
  class op is the sanctioned path). Conflict records get **no** addressable
  `TypedObjectId` kind; the meta-conflict names both resolver operation ids.

### 1.8 — P12-K6 — ResolveEquivocation edge semantics
- **Spec locus:** operation_catalog §ResolveEquivocation.
- **Ratify as implemented:** single-pass promotion (a promoted candidate that is
  itself a resolve does not govern a further promotion — no fixpoint); the set-level
  rule (a resolve held pending by its own causal gaps still *governs promotion* while
  its own effect stays pending); invalid-target/chosen no-ops reuse `TargetMissing`
  (consistent with the K9 disposition below, which scopes its new reason to
  differing-value *re-creates* only). **Execution note:** read
  `epiphany-ops` and write down the implemented quarantine interaction verbatim
  (may a quarantined resolve govern?) — the ratification text records what the code
  does; if the code turns out to have no defined behavior there, the text says
  "quarantined resolves are excluded from governing" only if a test proves it.

### 1.9 — P12-K8 — genesis outside the operation set *(decided above)*
- **Spec locus:** operation_catalog K1 chapter (retire the two slots) + core_spec
  Ch 5 (root/canvas genesis note) + Ch 6 (operation-set completeness statement).

### 1.10 — P12-K10 — undo strand-blocks keep `TransactionConflict`
- **Spec locus:** operation_catalog §UndoTransaction.
- **Ratify:** a StrictInverse strand-block (refusing to tombstone a minted object
  still referenced by a live non-member) **is** a transaction-scoped conflict;
  `ConflictKind::TransactionConflict` reuse is blessed — no new conflict kind. The
  refusal detail lives in the conflict record's affected-objects/description, as built.

### 1.11 — P12-K11 — undo idempotence asymmetry ratified
- **Spec locus:** operation_catalog §UndoTransaction.
- **Ratify:** an undo's value restorations are ordinary chain writes by the undo op
  (no distinguished provenance); a second undo of the same transaction sees the first
  as superseding (`Conflicted` under StrictInverse, skipped under BestEffort) while
  absence-restorations repeat idempotently. The asymmetry is documented normative
  behavior. Revisit only under the deferred undo-as-operation (streaming-consistent
  undo) track, which subsumes it.

### 1.12 — P12-K12 — cross-region slur permission = AND *(decided above)*
- **Spec locus:** core_spec validation/advisory section that defines the
  `permits_spanning_slurs` advisory (schema-major-1 Phase A text) + catalog
  §CreateCrossCutting advisory note.
- **Ratify:** a region boundary is permeable to a spanning slur only when **both**
  endpoint regions permit. Advisory-only; never alters reduction.

### 1.13 — P12-C1 — multi-source cue re-anchoring = cascade
- **Spec locus:** core_spec §Re-Anchoring Rule Table (Cue row rationale).
- **Ratify:** the table's action column is normative as written — **any** source
  deletion cascade-deletes the cue (like Tie). Fix the rationale prose to match
  ("a cue that loses any source loses its quotation integrity"); truncate-while-any-
  survives is a rejected alternative, recorded in the log.

### 1.14 — P12-C2 — graphic-gesture Range "truncate" defined
- **Spec locus:** core_spec §Re-Anchoring Rule Table (graphic gesture row).
- **Ratify:** a dead event-anchored range endpoint moves to its containing region's
  edge — start endpoint → region Start, end endpoint → region End, zero offset. As
  implemented.

### 1.15 — P12-C3 — inexpressible reconstructed ranges orphan
- **Spec locus:** core_spec §Re-Anchoring Rule Table (analytical annotation row).
- **Ratify:** a wall-clock (region-relative) or indeterminate event span that cannot
  be expressed as a stored `Range` anchor **orphans** (recorded as such); an
  expressible form is future model work, not required.

### 1.16 — P12-E4 — conservative barrier matching
- **Spec locus:** core_spec Ch 8 (edit barriers / extension declarations).
- **Ratify:** operations with no graph target (`SetMetadata`, `DeclareTransaction`)
  are matched by **score-wide barriers only**; opaque `Registered` operations match
  **fully conservatively**. As implemented in editor-core's barrier gate.

### 1.17 — P12-I4 — constraint strength is kind-determined
- **Spec locus:** core_spec Ch 9 (normalized constraint form) + Ch 7 `LayoutConstraint`.
- **Ratify:** no per-instance strength field. Strength is determined by constraint
  kind: break constraints by `BreakKind` (Hard → Required, Soft → Preferred{1.0});
  the other core families Required; `Registered` conservatively Required. Future
  (Standard-tier) constraint families **declare their strength in their normative
  definition** — the channel is the kind, not the instance. This is a deliberate
  Push-3 design input.

### 1.18 — P12-I5 — sub-conformant solver report shape
- **Spec locus:** core_spec Ch 9 (SolveStatus semantics; next to the Pass-12-tranche-1
  `SolverTier::Stub` text).
- **Ratify:** a below-conformance passthrough solver reports constraints-present-but-
  not-evaluated as `SolvedWithWarnings` + `satisfied_hard_constraints == false` + a
  dedicated warning. Renderable, honest, makes no conformance claim.

### 1.19 — P12-I6 — the Minimal-tier constraint-emission floor
- **Spec locus:** core_spec Ch 9 (spacing-pass requirements).
- **Ratify:** the normative Minimal floor = successive-notehead-column no-collision
  chains + per-glyph region containment + user-break constraints, exactly the
  implemented emission set. Standard's floor is defined when the Standard tier lands
  (Push 3). Makes the Minimal acceptance surface testable.

---

## Bucket 2 — Decide-then-build (spec text + code; 5 rows)

Spec-first discipline: the normative text lands in the same pass, the code follows
in a separate commit, each change regression-tested.

### 2.1 — P12-H7 — surface authored annotations for ineligible events *(decided above)*
- **Spec:** core_spec Ch 2 + Ch 3 pre-pass sections — derived annotations include, for
  events/pitches the algorithm produces no output for, the winning **authored**
  attachment (same source-rank discipline); taxonomy counts them in dedicated
  authored-only buckets.
- **Code:** `epiphany-core/src/prepass.rs` — authored-only resolution path in both
  `resolve_spelling`-adjacent and `resolve_decomposition` surfaces; new
  `TaxonomyReport` buckets (e.g. `spellings_authored_uninferred`,
  `decompositions_authored_uninferred`), serialized into the derivation fingerprint
  (derived annotations only — **no canonical-byte impact**; the fingerprint is not
  canonical state). Regression tests: an authored attachment on an ungriddable event
  surfaces; an outranked one does not; fingerprint changes deterministically.

### 2.2 — P12-K3 — refuse content rewrites of SYSTEM_DERIVED pitches
- **Spec:** operation_catalog §ModifyEvent + §Identified-Pitch Operations gain the
  precondition; core_spec Ch 5 Invariant 11 rationale cross-ref.
- **Code:** reduction-time precondition — a `ModifyEvent`/`ModifyIdentifiedPitch`
  that would rewrite the *intrinsic content* of a `SYSTEM_DERIVED` pitch refuses as a
  clean no-op with a **new appended** `PreconditionFailureReason` (proposed:
  `SystemDerivedContentImmutable`). Append-only vocabulary = sanctioned minor
  evolution (precedent: `TempoMapMalformed = 11`). Tests: direct refusal, reduce ==
  reduce_onto agreement, wire golden for the new discriminant.

### 2.3 — P12-K9 — dedicated reason for differing-value re-creates
- **Spec:** operation_catalog (CreateStaff, carried TimeSignature, container creates).
- **Code:** appended `PreconditionFailureReason` (proposed: `RecreateContentMismatch`)
  replacing the misnamed `TargetMissing` on live-id-re-carried-with-different-content
  refusals. Same test discipline as 2.2.

### 2.4 — P12-C4 — appended `ReanchorReason::SameCanvasNearer`
- **Spec:** core_spec §Re-Anchoring Rule Table (rank-4 survivor recording).
- **Code:** append the discriminant (after `ExplicitFallback = 4`; before
  `DeclaredByExtension`'s registered space — confirm the discriminant table allows a
  clean append; if `DeclaredByExtension` already owns 5, take 6 and record why).
  Rank-4 (same-canvas) survivors record it instead of `ExplicitFallback`. Wire golden
  + one re-anchoring test updated.

### 2.5 — P12-E5 — unsafe-edit tombstone: semantics now, encoding deferred
- **Spec (this pass):** core_spec Ch 8 ratifies the *semantics*: crossing a barrier
  via `apply_unsafe` immediately deactivates the extension's remaining barriers for
  the session; the crossing MUST be durably recorded at the next bundle commit; a
  tombstoned `required = true` extension makes the bundle open **read-only** for
  writers that honor the extension (they can no longer trust its invariants).
- **Deferred (named site):** the manifest-side *encoding* of the tombstone record —
  the manifest is major-0-forever (schema-major-1 design decision), so the record
  must ride the extension-declaration blob layer or a new chunk kind; that design
  belongs to the next bundle-format tranche. Add an `openquestion` box to
  `binary_format.tex` naming the constraint. Editor-core's
  `extensions_requiring_tombstone()` is the implemented producer awaiting that
  consumer.

---

## Bucket 3 — Defer-with-rationale (3 rows, named landing sites)

### 3.1 — P12-H2 — key/clef model: premise stale, remainder deferred
- **Finding (verified 2026-07-07):** the row's premise is stale — the content model
  *exists* (I-0: `Clef`, `KeySignature`, content-bearing `ClefChange`/
  `KeySignatureChange`) and layout consumes it (`PlacedKeySignature`, `active_clef`).
  What remains is algorithmic: the spelling pre-pass never consults declared keys
  (0 references in `prepass.rs`), and notation does no key-aware accidental
  suppression / cancelling naturals.
- **Defer:** key-aware spelling → a **spelling algorithm v2** (versioned-algorithm
  rev, invalidates derived output deterministically — no spec hole); key-aware
  accidental display + cancelling naturals → the **notation/engraving refinement
  backlog** (major-2 / Standard-tier neighborhood). Batch row struck with the
  narrowed statement.

### 3.2 — P12-K2 — Transpose interval algebra → Chapter-4 tuning track
- **Defer:** the faithful interval representation (diatonic/chromatic, octave/nominal
  renormalization, non-CMN pitch spaces) lands with the tuning catalog (Push 4).
  **Pin now (one sentence each):** the v1 payload's `chromatic_steps: i32` semantics
  = CMN alteration shift with documented `i8` saturation — a declared prototype whose
  replacement is a *payload schema-major* under the companion's evolution rule; and
  Transpose stays excluded from undo inversion (already documented).

### 3.3 — P12-K5 — profile-declared equivocation selection → profile track
- **Defer:** the third resolution path (profile-declared deterministic selection
  function) stays unpinned; v1 profiles declare **none** (ratify that sentence in the
  catalog §ResolveEquivocation rationale). The hook's definition belongs to the
  Profile Conformance companion, which does not exist yet — named landing site.

---

## Execution plan

Order of work (each tranche gated; spec-first before code):

1. **Tranche A — core_spec pre-pass text** (H1, H3, H4+C5, H5, H6, H7-normative,
   H2-narrowing): Ch 2/Ch 3 edits, two open-question boxes closed.
2. **Tranche B — operation_catalog text** (K1, K4, K6, K8, K10, K11, K12, K2-pin,
   K5-pin, K3/K9-normative): catalog version bump 0.5.0 → 0.6.0.
3. **Tranche C — core_spec re-anchoring + barriers + solver** (C1, C2, C3, C4-normative,
   E4, E5-semantics, I4, I5, I6) + binary_format openquestion (E5 encoding).
4. **Both PDFs rebuild clean** (lualatex core_spec; xelatex catalog; check
   binary_format too), zero undefined refs. → **Commit 1 (spec).**
5. **Tranche D — code** (H7 surfacing + taxonomy; K3 + K9 reasons; C4 variant), full
   gate (fmt, clippy -D warnings, workspace tests, conformance scale 1), review pass,
   → **Commit 2 (code).**
6. **Process trail:** PASS12_RATIFICATION_LOG "G-pass tranche" section (dispositions
   table, version movements, key decisions); PASS12_BATCH rows struck (28 → 0 open);
   crate DECISIONS cross-refs (core, ops, layout-ir, editor-core); CONFORMANCE.md
   H1 caveat dropped. Rides Commit 1/2 as appropriate.

After this pass the batch is **empty** and Push 2 (data-model major 2) starts with
no pending graph-model questions: H2 narrowed, K8 decided, H6 decided.
