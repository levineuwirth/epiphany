# Pass 12 — Ratification Log

## Tranche 1 (2026-07-02): audit spec-alignment

The 2026-07 six-agent spec-compliance audit sorted its findings into code-side
MUST fixes (landed separately; see the crate `DECISIONS.md` files and
`CONFORMANCE.md`) and places where the *spec text* trailed a disposition the
project had already made — a documented crate decision, the ratified Operation
Catalog, or the PHASE2_QUICKSTART model the implementation follows. This
tranche ratifies that second set. **No byte-layout changes; architecture
unchanged.** The main Pass 12 batch (`PASS12_BATCH.md`) stays open: the
P12-H/K rows await G's ratification alongside Phase 2's open questions and are
*not* resolved here.

`adopt` = blessed the implementation/catalog behavior as normative spec text.
`fixed` = the spec text was contradictory or stale and changed.

| Item | Disposition | Spec locus | Authority followed |
|---|---|---|---|
| ModifyEvent metric placement | **adopt** — the catalog's §ModifyEvent reduction rule now states: a *metric* move (different `Musical` position/duration) is materialised, re-sorting the owning voice (id-tiebroken), behind a **placement precondition** read from the reducer's canonical voice-occupancy index — a non-positive span or an overlap with another live event refuses as a clean `EventDurationInvalid` no-op; a materialised move updates the occupancy index; non-metric moves and tuplet-member trimming stay deferred; same-placement field edits apply in place | operation_catalog §ModifyEvent | ops DECISIONS "ModifyEvent materializes metric placement changes (trim/move)"; `reduce.rs::modify_event` |
| Slur/Spanner re-anchoring | **fixed (core-spec↔catalog contradiction)** — the rule-table rows now read: re-anchor to the nearest surviving endpoint/anchor while ≥ 1 survives, cascade-delete only when none does; a two-endpoint slur collapses onto its sole survivor (reference-clean degenerate form); proximity-aware re-targeting and per-spanner-kind bounds are deferred refinements (P11-C5 stand-in). Previously the Slur row said "nearest forward event in same voice; cascade if fewer than two members survive" | core_spec §The Re-Anchoring Rule Table | operation_catalog §DeleteEvent Re-anchoring; `reduce.rs` ledger + `materialize_graph_delete` (kept consistent per ops DECISIONS "the graph follows the ledger") |
| RewriteTuplets payload | **adopt (id-only v1)** — core spec drops `TupletRewrite { tuplet, new_ratio, new_members }` for the catalog's `RewriteTuplets { tuplets: Vec<TupletId> }`, and states normatively that graph-aware reduction MUST refuse the variant as an ill-formed compensation rather than fabricate rewritten values; a future value-carrying payload revision reopens it (`ReplaceWithRest`/`CascadeDeleteTuplets` are the applicable compensations until then). Keeps v1 bytes stable; the functionality hole is documented instead of contradictory | core_spec §DeleteEvent (payload, preconditions, effect) | operation_catalog §DeleteEvent; `reduce.rs` refusal (`TupletCompensationInvalid`) |
| Operation-block summaries | **adopt (bundle M4)** — an `OperationEnvelopeBlock` chunk payload is the envelope vector only (the spec'd `block_id` field was unrealizable: a content-addressed chunk cannot embed its own id); `OperationBlockSummary { dvv_summary, min_stamp, max_stamp }` lives in the **manifest**, keyed by the block's `ChunkId`, as opaque ops-supplied, non-canonical, optional metadata; the "writers SHOULD order blocks by min_stamp" line is dropped — the canonical manifest encoding sorts chunk references ascending by encoded form (Appendix D), so stamp-ordered scanning goes through the summaries | core_spec Ch8 §Operation Envelope Blocks + §Manifest | bundle DECISIONS "Operation-envelope block summary metadata is carried (M4 follow-up)"; `manifest.rs::OperationBlockSummary` |
| Pre-pass output model | **fixed (stale Ch2/Ch3 text)** — the spelling and decomposition pre-passes produce **canonical derived annotations**: deterministic functions of (materialized graph, profile, versioned algorithm id), recomputed on materialization, never stored as graph state or serialized into canonical chunks — replacing "the pre-pass MUST produce / write `Inferred`-source attachments" and "stored as an attachment on the event". The pre-pass now MUST NOT mint attachments; promotion to a stored `UserChosen` attachment happens only via explicit editing operations. The incremental-re-run MUST is demoted to MAY (cache/incremental recompute permitted; neither observable in output; cache invalidates on derivation-key change). Ch1 design-principle retitled "notational rhythm is *derived*" | core_spec Ch2 §Spelling Source/Precedence/Pre-Pass + rationale; Ch3 §Notational Decomposition + §Decomposition Pre-Pass; Ch1 design principles | PHASE2_QUICKSTART "Canonical model: derived annotations, not stored objects"; `prepass.rs::derive_annotations` |
| Spelling tie-break | **adopt** — conflicts break by precedence, then attachment `priority` (higher wins), then **canonical attachment order** (earliest in the score's canonical serialization wins); the "attachment creation timestamp" tie-break is deleted — attachments carry no timestamp, and one would hang resolution on non-canonical state | core_spec Ch2 §Configurable Precedence | `prepass.rs::resolve_spelling` |
| Conflict-registry location | **fixed** — `Score.conflicts: ConflictRegistry` removed from the graph root; the registry is a component of canonical **materialized state** (which Ch6's requirement already said), i.e. a reduction product, not authored content; the hierarchy table, chapter overview, and §Conflict Records prose updated to match | core_spec Ch5 §Score root; Ch6 §Conflict Records / §The Conflict Registry | `reduce.rs::MaterializedState.conflicts` |
| SolverTier::Stub | **adopt** — the spec enum gains the code's `Stub` variant: not a conformance tier, an interface-only/passthrough solver ordered below every conformant tier; declaring it makes no conformance claim and satisfies no minimum-tier requirement. Never canonically serialized, so no byte impact | core_spec Ch7 §Conformance Tiers | `layout-ir/solver.rs::SolverTier` |

**Version movements.** Operation Catalog 0.2.0 → 0.3.0 (ModifyEvent
reduction-rule behavior text). Core spec: revision-history row "Pass 12
tranche 1 (audit spec-alignment)" appended.

## Tranche 1 addendum (2026-07-02, Push-3 enablers)

Two further ratifications landed the same day as spec-side *enablers* for the
Push-3 wiring work — each defines normative text the implementation then built
against (spec-first, per the batch discipline):

| Item | Disposition | Spec locus | Consumer |
|---|---|---|---|
| `ResolveEquivocation` meta-operation | **fixed (dangling prose reference)** — the core spec named the operation only in prose (§Equivocation, resolution path 2) with no payload schema, no catalog entry, no K1 slot. Ratified: catalog §ResolveEquivocation pins `ResolveEquivocationPayload { target: OperationId, chosen: EnvelopeHash }`, order-independent earliest-resolve-governs promotion, `AlreadyApplied` idempotence, `StructuralFieldCollision` on `equivocation_resolution` for differing resolves, precondition no-ops, and the equivocated-resolve exclusion; core spec `OperationPayload` gains the variant. Catalog 0.3.0 → 0.4.0. The profile-policy path stays open (P12-K5) | operation_catalog §ResolveEquivocation; core_spec §Operation Envelope | `epiphany-ops` (payload discriminant 3 appended; set-level promotion pre-pass) |
| Anchored break overrides | **fixed (representational gap)** — `OverrideKind::SystemBreak`/`PageBreak` were nullary, so a break's *position* was unrepresentable and the logical-stage projection required by §Engraving Overrides could not exist. Ratified: both kinds carry `anchor: TimeAnchor`; the `ScoreGraph` target names the owning region; projected break overrides carry `Internal` origin (authorship lives in the op log, P11-C8) | core_spec Ch7 §Engraving Overrides listing + note | `epiphany-layout-ir` (`to_logical` projection + paired `UserOverride` decisions) |

**Deliberately not touched here** (still open in `PASS12_BATCH.md`): every
P12-H/K row (algorithm-id ratification, decomposition precedence
configurability, authored decompositions for ineligible events, system-pitch
content modification, ResolveConflict beyond the concurrent case, RespellPitch
v0 migration fallback, Transpose interval algebra), plus the audit's Push-3
wiring tracks (constraints, overrides, validation modes, edit barriers,
operation index) — those are code work, not spec alignment.

## P12-I11 resolution (2026-07-03) — no spec change

The batch's one measured-and-tracked conformance miss, **P12-I11** (RS-1
honestly failed the Minimal `casting_off_quality` threshold — greedy first-fit
left a two-measure stub last system, width CV 0.6145 clamped to 1.0 > 0.90),
is resolved **entirely on the implementation side**, so it appears here only to
record that the Quality Metric Catalog and the core spec were **not** changed.

| Item | Disposition | Spec locus | Consumer |
|---|---|---|---|
| P12-I11 casting-off stub last system | **no spec change (engrave fix)** — casting-off gained a second **widow-rebalance** phase (`epiphany-engrave` v3): it moves whole trailing measures from a region's penultimate system into its final one, choosing the shift that minimizes the larger of the two distribution penalties the catalog already defines for the break family (`casting_off_quality` width imbalance vs `system_break_penalty` non-final underfill; both share the 0.5 anchor). RS-1 casts six/four instead of eight/two — `casting_off` 1.0 → 0.4463, every axis ≤ 0.90 — so the suite's asserted Xfail row is promoted to a plain Pass. This is the deliberately-chosen *honest* resolution: the `casting_off` 0.5 anchor and the Minimal 0.90 column were **vindicated, not relaxed** (the engraver improved, no anchor rescale / threshold loosening / RS-1 per-entry override). Core spec Chapter 9's "Minimal makes no optimality claim" already permits the heuristic; nothing normative changed | — (no spec locus; core spec + Quality Metric Catalog unchanged) | `epiphany-engrave` (`casting::rebalance_widows`, `ENGRAVER_VERSION` 3); `epiphany-testkit` (RS-1 xfail row removed); render goldens regenerated |

## P12-I12 resolution (2026-07-03) — Quality Metric Catalog 0.1.0 → 0.2.0

Unlike P12-I11, the second measured finding's honest fix is a **catalog
change**: the defect was in the metric's own definition, not the engraver.

| Item | Disposition | Spec locus | Consumer |
|---|---|---|---|
| P12-I12 spacing floor on short scores | **fixed (metric measurement domain)** — `spacing_distortion`'s raw measurement is scoped to the system's **rhythmic columns** (spring slots bearing a notehead or rest); the clef / key-signature / time-signature lead and barlines contribute no column, so a note-to-note advance spans them. Rationale: the leading clef-to-first-note gap is furniture width, not note spacing, and folding it into the CV flagged perfectly-spaced short lines as distorted purely for carrying a clef. The three warning entries drop from 0.36–0.41 to 0.2188 / 0.0819 / 0.0856 (below the Standard 0.32 floor), and the axis stays live on real irregularity. The `1.0` anchor, orientation, range, tier thresholds, and the eight other axes are **unchanged**; this is measurement-only — no layout, canonical bytes, render golden, or `ENGRAVER_VERSION` change. Catalog **0.1.0 → 0.2.0** (a normative refinement of one axis's measurement domain). The duration-aware optical-spacing open question stays open | quality\_metric\_catalog §`spacing_distortion` (`req:qmc:spacing`); revision-history row 0.2.0 | `epiphany-engrave` (`quality::census` rhythmic-column filter, `is_rhythmic`); `epiphany-testkit` (RS-3/5/6 measured values drop, still Pass) |

**Version movements.** Quality Metric Catalog 0.1.0 → 0.2.0
(`spacing_distortion` scoped to rhythmic columns). Core spec unchanged (the
axis's formal definition is delegated to the catalog; Chapter 9 carries only
the field, which is unchanged).

## Schema-major-1 tranche (2026-07-06) — data-model growth + engrave ratify-only

The first binary-format schema-major bump (v0 → v1, `binary_format` 0.3.0),
built machinery-first on a small payload, landed the two data-model rows the
batch parked behind the frozen-layout rule (**P12-I7**, **P12-K7**) and ratifies
the three engrave/layout-ir dispositions the implementation already made
(**P12-I8/I9/I10**). Delivered as the schema-major-1 Phase A–F commit series
(from `f4a2f1f`); this tranche records Phase F.

| Item | Disposition | Spec locus | Authority followed |
|---|---|---|---|
| P12-I7 page-geometry graph home | **landed (schema-major-1)** — `Canvas` gained `layout_defaults: CanvasLayoutDefaults { page_size: CanvasSize, margins: CanvasMargins }` (staff spaces, A4/8 mm default), defined in core spec (Phase A) and added to the graph + canonical wire form at major 1 (Phase C, byte-for-byte v0→v1 migrate-on-read, zero golden churn). The engraver still defaults to the same geometry; wiring it to *read* the graph field (C′) is a byte-neutral follow-up deferred until a custom-geometry producer exists | core_spec Ch5 §The Canvas (`CanvasLayoutDefaults`); binary_format §Schema Major 1 | `epiphany-core` (`graph.rs`, `codec.rs::decode_v0_score`) |
| P12-K7 advisory-precondition catalog | **landed (schema-major-1) + adopt** — the two graph-model gaps the advisory catalog named are filled: `Instrument.range: Option<PitchRange>` and `Region.permits_spanning_slurs: bool` (major 1, Phases D1/D2), so `epiphany-ops` now enforces the InsertEvent pitch-in-range advisory (frame-guarded; "if any"/indeterminate → vacuous pass) and the CreateCrossCutting(Slur) region-spanning advisory. ModifyEvent's duration-boundary bucket is stated explicitly (it carries the full replacement value). Cross-region slur permission is read as **both** endpoint regions must permit (conservative AND) — see the open item below | core_spec Ch5 §Instruments / §Regions; Ch6 §6.10 advisory buckets; `PitchRange` | `epiphany-core` (`pitch.rs::PitchRange`, `graph.rs`); `epiphany-ops` (`validate.rs`) |
| P12-I8 break-constraint satisfaction | **adopt** — Ch7 gains a normative predicate (Requirement `req:layoutir:break-satisfaction`): a `SystemBreakAt`/`PageBreakAt` at `slot` is satisfied iff the final `ResolvedLayoutIR` starts a system/page at that slot; a region-first slot is trivially satisfied; satisfaction is a predicate on the output, not the solver's spring state | core_spec Ch7 §ConstrainedLayoutIR (`req:layoutir:break-satisfaction`) | `epiphany-engrave` casting-off (evaluates hard break constraints for its tier claim) |
| P12-I9 break-override attribution | **adopt (decline widening)** — Ch7 gains Requirement `req:layoutir:break-origin-attribution`: honouring a user break carries `DecisionSource::UserOverride(id)`, threaded through a `ConstrainedLayoutIR.break_origins` sidecar populated by the logical-stage projection. The normalized constraint record is deliberately **not** widened to carry override identity — attribution is a projection concern, not a solver input | core_spec Ch7 §Engraving Overrides (`req:layoutir:break-origin-attribution`) | `epiphany-layout-ir` (`constrained.rs::break_origins`, `to_constrained`) |
| P12-I10 continuation synthesis | **adopt (bless registered id)** — Ch7 gains Requirement `req:layoutir:continuation-synthesis`: a system-spanning stroke's post-first segments are synthesized under `SynthesisKind::Registered(SYSTEM_CONTINUATION_SYNTHESIS)`, with `stable_semantic_instance_key = (original, ordinal)`; because `LayoutObjectId`s are non-canonical and re-derived per layout, the key need only be stable within a layout | core_spec Ch7 §Provenance (`req:layoutir:continuation-synthesis`) | `epiphany-layout-ir` (`provenance.rs`; `SYSTEM_CONTINUATION_SYNTHESIS`) |

**Version movements.** Binary Format companion 0.2.0 → 0.3.0 (schema major 1
wire form + migration; Phase A). Core spec: `CanvasLayoutDefaults` / `PitchRange`
/ `Region.permits_spanning_slurs` data-model additions (Phase A) and the three
engrave requirements above (Phase F); revision-history row appended.

**Open item (flagged, not resolved).** Which region governs a *cross-region*
slur's spanning permission is under-specified. The implementation chose the
conservative **AND** (a boundary is permeable only when both endpoint regions set
`permits_spanning_slurs`); a "the start region governs" or "either side" reading
is equally defensible. Tracked for ratification; the advisory is authoring-only
and never alters reduction, so the choice is not byte-affecting.

## G-pass tranche (2026-07-07) — the batch pass

The full G-ratification of the accumulated batch: all 28 open rows retired in
one deliberate pass (worklist: `PASS12_WORKLIST.md`). Four rows carried genuine
forks and were decided by the project lead; three rows defer to *named* landing
sites; the rest bless implemented, DECISIONS-recorded readings. Spec-first: the
normative text landed with this tranche; the small code tranche (H7 surfacing,
K3/K9 reasons, C4 variant) follows in its own commit.

**Key decisions (project lead, 2026-07-07):** P12-K12 cross-region slur
permission = **AND** (both endpoint regions); P12-H7 authored annotations for
inference-ineligible events **surface** in derived annotations; P12-K4
ResolveConflict = **no supersede** (earliest applied resolve governs
universally; re-resolution is a future dedicated op; no `TypedObjectId`
Conflict kind); P12-K8 **genesis outside the operation set** (create-score/
canvas slots retired, not "unavailable").

| Item | Disposition | Spec locus | Authority followed |
|---|---|---|---|
| P12-H1 spelling algorithm id | **adopt** — `"default"` = Temperley-style line-of-fifths preference v1, ratified normative (`req:pitch:spelling-algorithm`); profile-declared disposition; no silent substitution. CONFORMANCE.md caveat dropped | core_spec Ch2 §Spelling Pre-Pass; Ch4 open question narrowed; Ch1 + App. D open-hooks lists updated | `epiphany-core/src/prepass.rs` |
| P12-H2 key/clef model | **defer (premise stale, verified)** — the content model *exists* (I-0: `Clef`, `KeySignature`, content-bearing changes) and layout consumes it; what remains is algorithmic. Key-aware spelling → a spelling-algorithm **v2** (versioned rev, deterministically invalidates derived output); key-aware accidental display / cancelling naturals → the notation-refinement backlog (major-2 / Standard-tier neighborhood) | `req:pitch:spelling-algorithm` states v1 does not consult declared keys | verified: 0 `KeySignature` refs in `prepass.rs`; `PlacedKeySignature` in layout-ir |
| P12-H3 chromatic-run convention | **adopt** — enharmonic choice absent tonal context is a property of the *versioned* algorithm; v1 = convention-as-tiebreak only; voice-leading refinement = future version, not a spec hole | `req:pitch:spelling-algorithm` | `prepass.rs` centre-of-gravity rule |
| P12-H4 decomposition scope bounds (+P12-C5 folded) | **adopt** — the five v1 bounds (single governing meter; barline origin; dyadic compound grouping; no nested/cross-beat tuplets; `MAX_DOTS = 1`) become *declared normative bounds* of `DecompositionAlgorithmId "default"` v1 (`req:time:decomposition-algorithm`); a wider algorithm is a version bump. C5: `SetTimeSignature` reduction semantics already pinned (catalog §Meter and Tempo Overwrites); the derived-notation gap is subsumed by the single-meter bound | core_spec Ch3 §Decomposition Pre-Pass | `prepass.rs` integer-grid splitter; ops `reduce.rs` meter LWW |
| P12-H5 aleatoric spelling | **adopt (open question closed)** — spelling is region-time-model-independent; no aleatoric-specific pass exists or is required; stated in the v1 algorithm definition | `req:pitch:spelling-algorithm` | `prepass.rs` (region-independent by construction) |
| P12-H6 decomposition precedence | **decide: FIXED** — the fixed default order (`UserChosen > Imported > Propagated > Inferred`, canonical attachment order tie-break) is ratified; *not* configurable (a configurable order = new canonical `Score` field = schema-major with no consumer). "Same precedence machinery" reworded to "same source-rank discipline" | core_spec Ch3 §Notational Decomposition | `prepass.rs::resolve_decomposition` |
| P12-H7 authored-uninferred surfacing | **decide: SURFACE (code follows)** — derived annotations MUST report the winning authored attachment for inference-ineligible targets, both pre-passes; taxonomy counts them distinctly (`req:pitch:authored-uninferred`). Derived-annotation-only: no canonical-byte impact | core_spec Ch2 (new requirement) + Ch3 cross-ref | decision reverses the implemented override-only mirror; code tranche implements |
| P12-K1 RespellPitch migration | **adopt** — context-recovery + `Irreversible` → read-only is the *long-term* disposition; no richer v0 corpus required (none exists). Open-question box → ratified migration note | operation_catalog §RespellPitch | `epiphany-ops/src/migrate.rs` |
| P12-K2 Transpose algebra | **defer (named site) + pin** — prototype semantics (CMN alteration shift, documented `i8` saturation) declared v1 behavior; faithful interval representation = *payload schema-major* landing with the Ch4 tuning catalog (Push 4) | operation_catalog §Transpose | `reduce.rs` transpose arm |
| P12-K3 system-derived content rewrite | **decide: REFUSE (code follows)** — reduction MUST refuse intrinsic-content rewrites of `SYSTEM_DERIVED`-namespace pitches; appended `PreconditionFailureReason::SystemDerivedContentImmutable` (12). Core Ch5 states immutability; catalog pins the precondition for ModifyEvent + ModifyIdentifiedPitch | core_spec Ch5 §System-Derived Identifiers; operation_catalog §ModifyEvent; binary_format vocab (12) | protects Invariant 11; code tranche implements |
| P12-K4 ResolveConflict beyond concurrent | **decide: NO SUPERSEDE** — earliest-applied-resolve governs universally; causally-later differing resolves + any resolve against `Dismissed` read `AlreadyApplied`; re-resolution = future `ReopenConflict`-class op; no `TypedObjectId` Conflict kind (meta-conflict names both resolvers) | operation_catalog §ResolveConflict | `reduce.rs` resolve arm (as implemented) |
| P12-K5 equivocation selection policy | **defer (named site)** — v1 profiles declare *no* selection function (now stated); the hook's definition belongs to the Profile Conformance companion; no reducer policy hook until then | operation_catalog §ResolveEquivocation rationale | deliberate absence in `reduce.rs` |
| P12-K6 equivocation edge semantics | **adopt** — single-pass promotion (no fixpoint); quarantined resolves never govern (verified in `reduce.rs` pre-pass comment + code); pending-by-causal-gaps resolves still govern (set-level); invalid-target/chosen no-op keeps `TargetMissing` (dedicated reason rejected — verdict does not change caller behavior) | operation_catalog §ResolveEquivocation (new Edge semantics block) | `reduce.rs` promotion pre-pass |
| P12-K8 create score/canvas | **decide: GENESIS OUTSIDE OPS** — root + canvas are structural givens; genesis = empty-document constructor + bundle creation, normative; K1 slots *retired* (no kind will be assigned); revisit only under an addressable multi-canvas major | core_spec Ch5 §The Canvas; operation_catalog K1 chapter + Conformance Profiles | `Score::empty` + bundle creation path |
| P12-K9 differing-value re-creates | **decide: DEDICATED REASON (code follows)** — appended `PreconditionFailureReason::RecreateContentMismatch` (13) replaces the misnaming `TargetMissing` reuse at every differing-value re-create site (CreateStaff, carried TimeSignature, container creates) | operation_catalog §CreateStaff + §Meter and Tempo Overwrites; binary_format vocab (13) | code tranche implements |
| P12-K10 undo strand-blocks | **adopt (bless reuse)** — a StrictInverse strand-block *is* a transaction-scoped conflict; `TransactionConflict` reuse ratified; detail lives in the conflict record | operation_catalog §UndoTransaction | `undo.rs` |
| P12-K11 undo idempotence asymmetry | **adopt** — restorations are ordinary chain writes (no distinguished undo provenance); second-undo conflict + idempotent absence-restores are normative; revisit only under undo-as-operation | operation_catalog §UndoTransaction | `undo.rs` write chains |
| P12-K12 cross-region slur governance | **decide: AND** — boundary permeable only when both endpoint regions permit; advisory-only, never byte-affecting | core_spec Ch5 (after `Region` listing); operation_catalog §CreateCrossCutting | `validate.rs` conservative AND (as implemented) |
| P12-C1 multi-source cue | **adopt** — cascade on *any* source deletion is normative; rationale prose fixed to match ("losing any source breaks quotation integrity"); truncate-while-any-survives recorded as rejected | core_spec §Re-Anchoring Rule Table (Cue row) | `reduce.rs` cue cascade |
| P12-C2 Range truncate | **adopt** — truncate = dead event-anchored endpoint moves to its containing region's edge (start→Start, end→End, zero offset) | core_spec §Re-Anchoring Rule Table (graphic-gesture row) | `reduce.rs` re-anchor ledger |
| P12-C3 annotation orphaning | **adopt** — orphaning is the sanctioned outcome for wall-clock/indeterminate spans inexpressible as stored `Range` anchors; an expressible form is future model work | core_spec §Re-Anchoring Rule Table (annotation row) | `reduce.rs` |
| P12-C4 same-canvas reason | **decide: APPEND (code follows)** — `ReanchorReason::SameCanvasNearer` appended at discriminant **6** (5 was already owned by `DeclaredByExtension`); rank-4 survivors record it instead of `ExplicitFallback` | core_spec `ReanchorReason` listing + note; binary_format vocab (6) | code tranche implements |
| P12-E4 barrier matching | **adopt** — target-free ops (`SetMetadata`, `DeclareTransaction`) match score-wide barriers only; opaque `Registered` ops match fully conservatively (`req:format:barrier-matching`) | core_spec Ch8 | `editor-core` barrier gate |
| P12-E5 unsafe-edit tombstone | **adopt semantics + defer encoding (named site)** — `req:format:unsafe-tombstone`: immediate deactivation of the crossed extension's remaining barriers; durable record MUST land at next commit; `required = true` → read-only for dependents. The manifest-side *byte encoding* is a new binary_format open question (manifest frozen at major 0 → blob-layer or new chunk kind; next bundle-format tranche) | core_spec Ch8; binary_format §extension blobs (open question) | `editor-core::extensions_requiring_tombstone()` (producer exists) |
| P12-I4 constraint strength | **adopt** — strength is *kind-determined*, no instance field (`req:solver:kind-strength`): breaks by `BreakKind` (Hard→Required, Soft→Preferred{1.0}), core families Required, `Registered` conservative Required; future families declare strength in their definitions. Deliberate Standard-tier design input | core_spec Ch9 §Strength Levels | `layout-ir` normalization |
| P12-I5 sub-conformant report | **adopt** — `SolvedWithWarnings` + `satisfied_hard_constraints == false` + warning is the sanctioned constraints-present-but-unevaluated report; the one renderable status with unsatisfied-hard, because the field reports *evaluated* satisfaction (`req:solver:subconformant-report`) | core_spec Ch9 §SolveReport | `layout-ir` stub solver |
| P12-I6 Minimal constraint floor | **adopt** — the implemented emission set (successive-notehead no-collision chains + per-glyph containment + user-break constraints) is the normative Minimal floor (`req:layoutir:constraint-floor`); higher-tier floors defined when those tiers land | core_spec Ch7 §ConstrainedLayoutIR | `constrained.rs` emission |

**Ride-along staleness fix:** the Ch8 `OperationKindTag` listing gained the
eleven appended tags the code has carried since M2/Phase-3 (append-only
vocabulary; the listing had drifted).

**Version movements.** Operation Catalog 0.5.0 → 0.6.0. Binary Format
0.3.0 → 0.4.0 (vocab appends 12/13/6 + the E5 encoding open question). Core
spec: revision-history row "Pass 12 G-ratification (the batch pass)"; two
open-question boxes replaced by ratified requirements (spelling,
decomposition), one narrowed (Ch4 spelling-catalog), one added
(binary_format E5 encoding). All three PDFs rebuilt clean, zero undefined
references.

**Code tranche (follows this commit):** H7 authored-only surfacing + taxonomy
buckets (`prepass.rs`); K3 `SystemDerivedContentImmutable` (12); K9
`RecreateContentMismatch` (13); C4 `SameCanvasNearer` (6) — each with
regression tests and wire goldens.

### G-pass addendum (2026-07-07): pre-pass error contract aligned

A post-commit review caught a spec↔code disagreement introduced by the H1/H4
ratifications: the requirement text (MUST error on an unregistered algorithm
id) had been adopted from CONFORMANCE.md's claim, but the implementation still
returned a successful empty derivation under the requested profile. Resolution:
the **spec text stands**; the code moved — `derive_annotations` now returns
`Result` and rejects unregistered ids up front (`PrePassError`). No spec
change; core DECISIONS records the reasoning.

## Schema-major-2 tranche (2026-07-08) — rendering (E1/E2), engrave ratify-only

The schema-major-2 push's *rendering* consumers, ratified into core spec
Chapter 7. The data model (Phases A–C) and the repeat-authoring op pair (Phase
D) already landed with their own revision-history rows and companion-version
bumps (Operation Catalog 0.7.0, Binary Format 0.6.0); this tranche records
**Phase F**, which is layout-IR (Chapter 7) only. Delivered as the E1
(`7a9bf42` + `6651ae5`), E2 (`81b7f42` + `28f210e`) commit series. Layout
geometry is non-canonical (no content hash, no wire form — see
`req:layoutir:object-id-derivation`), so this is a ratify-as-implemented with
**no byte-layout change and no companion bump**, mirroring the schema-major-1
Phase-F precedent.

| Item | Disposition | Spec locus | Consumer |
|---|---|---|---|
| Non-glyph resolved primitives | **adopt (ratify pre-existing + E2 reality)** — Ch7 gains Requirement `req:layoutir:resolved-primitives`: `ResolvedLayoutIR` carries, beyond glyphs, non-glyph line `Stroke`s (staff lines, stems, barlines, volta brackets — present since staff lines, never previously ratified) and cubic-Bézier `Curve`s (slurs, E2). Each carries provenance (the hit-testing basis) and is re-spaced by the solver like a glyph; both are non-canonical. The struct listing and the `Stroke`/`Curve` shapes are added; the RenderIR provenance requirement widened from "originating `ResolvedGlyph`" to "…`ResolvedGlyph`, `Stroke`, or `Curve`" | core_spec Ch7 §ResolvedLayoutIR (`req:layoutir:resolved-primitives`) | `epiphany-layout-ir` (`constrained.rs::{Stroke,Curve}`, `resolved.rs`); `epiphany-render-svg` |
| E1 repeat / volta rendering | **adopt** — Ch7 gains Requirement `req:layoutir:repeat-render`: a barline-drawing `RepeatStructure` renders a repeat barline at each resolved boundary (the precomposed sign, replacing a coinciding measure barline or standing alone; the dot pair beside a never-replaced final barline), each `Volta` a bracket with ending numbers; an unresolvable boundary draws no ink (traced anchor, honest placement); jump-kind (`DaCapo`/`DalSegno`) marks and cross-region repeats deferred. The glyph vocabulary/spacing stay engraving-algorithm concerns | core_spec Ch7 §ResolvedLayoutIR (`req:layoutir:repeat-render`); references Ch5 §Repeat Structures | `epiphany-layout-ir` (`logical.rs`, `constrained.rs`); `epiphany-engrave` (casting) |
| E2 slur rendering | **adopt** — Ch7 gains Requirement `req:layoutir:slur-curve`: a `Slur` renders as a cubic Bézier between its endpoint columns, honoring `CurvatureOverride` (direction, height); an endpoint that does not resolve to a column on a single staff of one region draws no curve (traced anchor); a non-`Solid` `SpanStyle` line MAY be deferred but MUST be surfaced (a layout diagnostic), never silently rendered solid. The curvature algorithm and dash rendering stay forward-referenced out | core_spec Ch7 §ResolvedLayoutIR (`req:layoutir:slur-curve`); references Ch5 §Slurs and Phrase Marks | `epiphany-layout-ir` (`logical.rs`, `constrained.rs`, `hittest.rs`); `epiphany-editor-core` |

**Version movements.** None on the wire: core spec Chapter 7 gains three
`req:layoutir:*` requirements + the `Stroke`/`Curve` struct listings + a
revision-history row (Phase F). Operation Catalog stays 0.7.0, Binary Format
stays 0.6.0 — layout geometry is non-canonical.

**Deferred (documented, not open candidates).** Dashed/dotted slur *rendering*
(surfaced as a diagnostic today), kind-differentiated slur appearance
(Phrase/Editorial), jump-kind repeat marks, cross-region and cross-system-break
curve splitting (de Casteljau), and the `slur_shape` quality metric moving off
`0.0`-by-construction — all Standard-tier / Push-3 work, named in the crate
DECISIONS, none a Pass-13 ambiguity.

## Push-3 tranche (2026-07-08) — slur rendering completion

The three slur refinements the schema-major-2 Phase-F ratification deferred,
landed and ratified. Layout-IR (Chapter 7) only, non-canonical — a
ratify-as-implemented with **no wire form and no companion version move**
(mirrors the schema-major-2 rendering tranche). Delivered as commits `c5173d2`
(dashed rendering), `5869691` (curve splitting), `7d61271` (slur_shape
measured).

| Item | Disposition | Spec locus | Consumer |
|---|---|---|---|
| Dashed/dotted slur rendering | **adopt** — `req:layoutir:slur-curve` extended: an authored non-`Solid` `SpanStyle` line renders faithfully (the `LineStyle` rides the `Curve`, whose Ch7 listing gains the field); an implementation that defers the pattern must surface it, never silently render solid. The E2 `SlurLineStyleNotRendered` diagnostic is retired (the style is rendered, not deferred) | core_spec Ch7 §ResolvedLayoutIR (`req:layoutir:slur-curve`, `Curve` listing) | `epiphany-layout-ir` (`Curve.line`); `epiphany-render-svg` (`stroke-dasharray`) |
| Curve splitting across systems | **adopt** — `req:layoutir:slur-curve` extended: a slur spanning a system break splits into per-system sub-curves by de Casteljau (first segment keeps the slur's provenance, the rest synthesized continuations under `req:layoutir:continuation-synthesis`), replacing E2's draw-whole-in-start-system floating end | core_spec Ch7 §ResolvedLayoutIR (`req:layoutir:slur-curve`) | `epiphany-engrave` (casting `curve_fate`, `sub_cubic`) |
| slur_shape measured | **adopt (implement pinned formula)** — `slur_shape_penalty` moves off its `0.0` placeholder to the catalog's pinned `req:qmc:slur` formula (arc ratio ρ = apex/chord against the `[0.08, 0.25]` band). The Quality Metric Catalog's §`slur_shape` rationale and the notated-but-unrendered open question are refreshed to record slurs now render and are measured; the formula is unchanged, so no catalog version move | quality_metric_catalog §`slur_shape_penalty` (`req:qmc:slur`, rationale) | `epiphany-engrave` (`quality.rs::slur_shape_raw`) |

**Version movements.** None: core spec Ch7 `req:layoutir:slur-curve` extended +
the `Curve` listing gains `line`; a revision-history row (Push 3); the Quality
Metric Catalog rationale refreshed (no formula change). Operation Catalog stays
0.7.0, Binary Format 0.6.0, Quality Metric Catalog 0.2.0 — all non-canonical /
formula-stable.

**Deferred (documented, not open candidates).** Kind-differentiated slur
appearance (Phrase/Editorial's own line, distinct from the SpanStyle dash),
duration-aware slur height (would pull the clamped short/long slurs back into
the shallow-arc band), collision-aware slur reshaping (the Standard-tier
`slur_shape` driver) — all Standard-tier work.

## Push-3 tranche (2026-07-09) — primitive band ownership

The Standard-tier inter-staff vertical solve needed to know which staff owns each
primitive. `GlyphObject` had always declared its `vertical_band`; `Stroke` and
`Curve` had not, so `epiphany-engrave` reconstructed ownership from geometry —
and got it wrong twice, tearing stems (`4132a7a`) and then slurs (`b1bfe04`) off
their own notes, both reaching a committed golden. A slur is the proof the
inference cannot be made safe: its endpoints are lifted clear of its own staff by
construction, into the zone where the nearest notehead belongs to the neighbouring
staff. Ratified as implemented in `efaebb9` (code) and `fc411ea` (Ch7 listings).
Layout-IR (Chapter 7) only, non-canonical — **no wire form, no companion version
move**, and no golden churn (the declared owner agrees with the inferred one on
the whole corpus, which is what licensed the swap).

| Item | Disposition | Spec locus | Consumer |
|---|---|---|---|
| Primitive band ownership | **adopt (new requirement)** — Ch7 gains `req:layoutir:primitive-band-ownership`: every primitive presented to the solver in `ConstrainedLayoutIR` (`GlyphObject`, `Stroke`, `Curve`) MUST declare the `VerticalBandId` that owns it and that band MUST exist; a vertical solver MUST take ownership from the declaration and MUST NOT infer it from geometry. Staff-less content names a non-`Staff` band. A band MUST therefore exist for every staff of a region and for its margin, members or not; only glyphs are band *members* (a stroke's/curve's reference is one-way) | core_spec Ch7 §ConstrainedLayoutIR §Vertical Bands (`req:layoutir:primitive-band-ownership`) | `epiphany-layout-ir` (`constrained.rs::{Stroke,Curve}.vertical_band`, band emission); `epiphany-engrave` (`casting.rs` attribution) |
| Ownership through the resolved stage | **adopt (new requirement)** — Ch7 gains `req:layoutir:resolved-band-ownership`: a resolved `Stroke`/`Curve` MUST retain its `vertical_band` (casting-off and the inter-staff solve both relocate them and must attribute them); a `ResolvedGlyph` carries none, its ownership already baked into its resolved position. Band ownership is non-canonical attribution metadata — it enters no content hash and no canonical encoding, so `ResolvedLayoutIR::canonical_bytes` omits it even from the primitives that carry it | core_spec Ch7 §ResolvedLayoutIR (`req:layoutir:resolved-band-ownership`) | `epiphany-layout-ir` (`resolved.rs::canonical_bytes`) |
| Struct listings | **adopt (ratify reality)** — the `ConstrainedLayoutIR` listing gains `strokes: Vec<Stroke>` / `curves: Vec<Curve>` (present in code since staff lines, never listed — without them Chapter 7 cannot describe supplying non-glyph ownership to the solver); the `Stroke`/`Curve` listings gain `vertical_band`; `Stroke`'s stale "the vertical-band model does not contain" gloss is dropped | core_spec Ch7 §ConstrainedLayoutIR, §ResolvedLayoutIR | `epiphany-layout-ir` |

**Version movements.** None on the wire: core spec Chapter 7 gains two
`req:layoutir:*` requirements, three struct-listing corrections, and a
revision-history row. Operation Catalog stays 0.7.0, Binary Format stays 0.6.0 —
layout geometry is non-canonical.

**Known listing gap (pre-existing, not introduced here).** The `ConstrainedLayoutIR`
listing still elides two fields the code carries: `break_origins: Vec<BreakOrigin>`
and `catalog: GlyphCatalogIdentity`. Both *types* are specified — the former's
semantics by `req:layoutir:break-origin-attribution`, the latter by Ch7 §Glyph
Catalog Identity — so only the struct listing is incomplete, and neither gap
blocks an implementation the way a missing `strokes`/`curves` would have. Filed in
`epiphany-layout-ir/DECISIONS.md` and **parked**: the Pass-13 batch is closed and
the house rule opens a pass at ≥3 candidates, so this waits for company rather
than reopening one. Deliberately not fixed here — the listing correction in this
tranche is scoped to exactly what `req:layoutir:primitive-band-ownership` depends
on.

**Deferred (documented, not open candidates).** A height model for the
inter-staff gap band — the missing piece behind both staff-less content placed
*between* two staves (it holds still while the lower staff descends away) and
`vertical_density_penalty` saturating at 1.0 when the solve targets content
extents while the metric scores the realized gap against the band's *preferred*
height. Both are named in `epiphany-engrave/DECISIONS.md`; neither is a Pass-13
ambiguity.

## Push-3 tranche (2026-07-09) — the inter-staff gap band as a height model

Residue of the Standard-tier inter-staff solve, closed. **No spec requirement
changes and no version moves**: the Quality Metric Catalog was right and the
engraver was non-conforming. Delivered alongside `efaebb9`/`ced4b72` (primitive
band ownership), which is what made the fix expressible.

`req:qmc:vertical` has always defined the realized inter-staff gap as the
separation "between the adjacent **content extents** the band separates".
`engrave::quality::vertical_raw` measured the separation between the two bands'
glyph `members` — because until `req:layoutir:primitive-band-ownership` landed, a
band listed no strokes or curves to own. A staff's outermost ink is usually not a
glyph. On `two_staff_close_content` the solve cleared the declared 2.0 gap exactly
while the glyph-ink gap was 5.06, so the axis reported `|5.06 − 2|/2 = 1.53` →
saturated **1.0**, and a Standard-tier `QualityFloorApproached` warning fired on a
correct layout. Per §Conformance ("a solver that reports a `QualityMetricVector`
computed by any function other than the ones defined here is non-conforming"),
that was an implementation defect, not a definition defect.

| Item | Disposition | Spec locus | Consumer |
|---|---|---|---|
| `vertical_density_penalty` measured over glyph members | **fix (conformance), no catalog change** — `vertical_raw` now measures each staff band's full content (glyphs, strokes, curves), attributed by declared `vertical_band`, read back from the BAKED output rather than the solve's own extents (so a shift the bake drops surfaces as a real deviation instead of hiding behind solver intent). Axis on the pressure fixture: 1.0 → 2.7e-7; the floor warning is gone. Formula, contributing units, anchor, normalization all unchanged | quality_metric_catalog §`vertical_density_penalty` (`req:qmc:vertical`) — **clarification only** | `epiphany-engrave` (`quality.rs::vertical_raw`, `casting.rs::CastLayout::{stroke_system,curve_system}`) |
| "Content extent" was ambiguous | **clarify (editorial)** — `req:qmc:vertical` now spells out that content extent means every primitive the band owns, each attributed by its declared `vertical_band`, and not the band's glyph `members`. Before primitive band ownership this reading was arguably unimplementable, which is why the defect survived. The stale rationale (claiming the vertical spring solve is deferred) is refreshed, and the inter-system half of the axis is recorded as a genuine trade-off against `page_fill_efficiency`, not a defect | quality_metric_catalog §`vertical_density_penalty` rationale | — |
| Solve read the gap from a constructor | **fix** — the inter-staff solve now targets the `preferred_height` of the `InterStaffGap` band `to_constrained` emitted for that staff pair, not `VerticalBand::inter_staff_gap`'s default. The band is now a height model: a region declaring a wider gap gets one, and the solve and the metric agree by construction rather than by both calling the same constructor | — (behavioural, within `req:layoutir:vertical-bands`) | `epiphany-engrave` (`casting.rs`) |

**Version movements.** None. Quality Metric Catalog stays **0.2.0** (formula,
units, anchor, normalization unchanged — a clarification and a rationale refresh
are not a definition change; contrast P12-I12, which redefined
`spacing_distortion`'s unit and did move the version). Core spec unchanged.
Operation Catalog 0.7.0, Binary Format 0.6.0 unchanged. **No layout change and no
render-golden churn** — this is measurement-only, as the ENGRAVER_VERSION staying
at 11 records.

**Deferred (documented, not open candidates).** Staff-less content placed
*between* two staves would hold still while the lower staff descends away from
it. No primitive can name an `InterStaffGap` band today (`band_of` yields a staff
band or the region's margin band), so this is unreachable rather than latent;
building the machinery now would be speculative. Named in
`epiphany-engrave/DECISIONS.md`.
