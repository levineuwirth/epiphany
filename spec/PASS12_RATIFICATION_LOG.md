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
