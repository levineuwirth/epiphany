# epiphany-ops — decisions and Pass 11 candidates

This file records (a) the implementation decisions the QUICKSTART asked each
agent to make once and document, and (b) the ambiguities discovered while
building `epiphany-ops`, batched as **Pass 11 candidates** for the spec rather
than improvised in code (QUICKSTART, Process notes: *"Ambiguities go into a
batch, not into code … Don't open Pass 11 until you have at least three such
items batched."*).

> **RATIFIED (Pass 11, 2026-06-21).** The ops-layer Pass 11 candidates have been
> ratified into `core_spec.tex` — see `spec/PASS11_RATIFICATION_LOG.md`.
> Highlights: C2 adopted (`IntegrityAnomalyId` = `derive_system_id(MUSCSANM,…)`,
> with `MUSCSANM` promoted to a reserved built-in tag); C3 decided
> (field-collision tags the *winner* `Conflicted`); C4 adopted + lifted to spec
> (the >2-way, partial-overlap promotion generalization); C7 fixed (zero-based
> DVV floor made normative); C9 decided (`TransactionCategory`/`ObjectKind` core
> vocabularies pinned); C10 decided (added `ResolutionAction::Dismiss` so
> `Dismissed` is reachable). C1/C5/C6/C8 stay deferred to their tracks.

## Implementation decisions (QUICKSTART "Decisions you'll need to make")

1. **Replica ID entropy / event-arena / chunk store** — N/A to this crate
   (Agents A, B, D). `epiphany-ops` consumes Agent B's identifier family and
   never mints graph identifiers itself, except the deterministic system-derived
   ones (promoted voices via Agent B's `derive_promoted_voice_id`, and the
   content-derived `ConflictId` / `IntegrityAnomalyId`).
2. **Async or sync — sync only.** No async traits anywhere (decision 4).
3. **MSRV — workspace 1.77** (decision 5). No exotic features. `unsafe` forbidden
   crate-wide (`#![forbid(unsafe_code)]`).
4. **Canonical iteration is structural.** Every collection that feeds canonical
   output is a `BTreeMap`/`BTreeSet` or a vector put into the normative order
   before encoding (Appendix D §"Ordered Iteration"). The determinism fuzzer is
   the tripwire: it reduces each random set in several acceptance orders and
   asserts byte-identical materialized state, which would fail the moment a
   `HashMap` iteration order leaked in.

## Scope boundary: framework in full, representative operations

Chapter 6 specifies the *operation framework* and a *representative selection* of
operations; the full catalog of ~60–80 primitives is an **explicit open
question** (§6.11, marked `\openquestion`) deferred to the Operation Catalog
companion. This crate mirrors that division exactly:

- **Implemented in full (the framework):** operation identity/stamps, the HLC and
  its monotonicity rule, DVV causal context, the order-independent
  `OperationSlot` model and acceptance pipeline, the canonical reduction order,
  the four-phase lifecycle, effects with the typed `PreconditionFailureReason`,
  conflict records with content-derived ids, the conflict registry, integrity
  anomalies kept separate from conflicts, transactions with the
  descriptor-precedence rule, re-anchoring, LWW discipline, validation modes, and
  forward undo.
- **Representative (the §6.10 set):** `InsertEvent`, `DeleteEvent`,
  `RespellPitch`, `CreateCrossCutting`, `ChangeRegionTimeModel`,
  `SetUserSystemBreak`, `DeclareTransaction`, plus `ResolveConflict` and
  `UndoTransaction`. Together they exercise every reduction *discipline* the
  chapter defines (position-keyed insert + voice promotion; delete-wins +
  tombstone + re-anchor; field-overwrite + conflict; set-union; structural
  migration; LWW; atomic transactions). The remaining catalog kinds are an
  additive future change behind the existing `OperationKind` enum.

`MaterializedState` is the canonical bookkeeping Chapter 6 owns — the effect
log, conflict registry, anomaly register, object existence/tombstones,
spellings, and LWW fields. M2 adds `OperationSet::reduce_onto(&Score)`, which
seeds those indices from a canonical base and returns the corresponding Agent B
graph. Insert/delete, voice promotion, supported reference-level cross-cutting
values, system breaks, migration checks, transaction rollback, and undo mutate
that graph. The base-free `reduce()` remains the operation-set convergence API.

## M2a (Group 1) — event & pitch leaf-field ops: graph materialization

The Group-1 leaf-field ops (`ModifyEvent`, `Transpose`, `InsertIdentifiedPitch`,
`DeleteIdentifiedPitch`, `ModifyIdentifiedPitch`) reuse M1's reduction
disciplines unchanged; the canonical `MaterializedState` records only their
effect-log entry and — for the field-overwrite ops — a `StructuralFieldCollision`
on a concurrent differing write. Their resolved values live in the graph
(`reduce_onto`), which is derived state, not a second canonical store, so the two
decisions below are **graph-materialization-only**: the bookkeeping projection,
and therefore convergence/determinism, is unaffected. Both exist because an
in-place `EventArena::get_mut` edit bypasses `insert`'s well-formedness guard, so
the reducer must keep the graph Chapter-5-valid itself (otherwise the malformed
state surfaces only later, in `check_invariants`). Both are exercised at scale by
`run_graph_convergence` (criterion 1, now emitting these kinds) and pinned by
targeted `reduce_onto` tests in `tests/graph_reduction.rs`.

- **A note and a rest are the same slot under pitch add/remove (note↔rest
  conversion).** `DeleteIdentifiedPitch` of a single-pitch note's *only* pitch
  degrades the event to a `Rest` of the same id/voice/position/duration rather
  than leaving an empty pitched event — Chapter 5 forbids the empty chord ("use
  `Rest` for the no-pitch case", `ArenaError::EmptyPitchedEvent`).
  `InsertIdentifiedPitch` into a rest is the dual: the rest becomes a one-pitch
  note. This preserves the ops' disciplines (delete-wins / mint) and keeps the
  graph consistent with the bookkeeping (which tombstones / mints the pitch
  object either way). *Rejected alternative:* a "would empty the event"
  precondition failure — it needs a new `PreconditionFailureReason` against a
  ratified set, breaks delete-wins, and is a worse fit than the editor-natural
  "deleting a note's last pitch leaves a rest". **For the spec:** the Operation
  Catalog §"Insert/Delete identified pitch" (M2e) ratifies this note↔rest
  equivalence normatively.

- **`ModifyEvent` materializes metric placement changes (trim/move).** A
  `ModifyEvent` whose payload moves a *metric* event (different `Musical` position
  or duration) is now applied to the graph and the owning voice is **re-sorted**
  by ascending position (id-tiebroken — the same order an insert maintains), so
  invariant 3 (`VoiceEventsSortedNonOverlap`) is preserved. This is the make-room
  enabler: a "pencil" insert trims the events it overlaps. To keep invariant 3,
  `modify_event` first checks a **placement precondition** read from
  `voice_occupancy` (the canonical, graph-independent placement index, so
  `reduce()` and `reduce_onto()` agree): a move with a non-positive span, or one
  that would overlap another live event in the voice, is **refused** as a clean
  `NoOp(EventDurationInvalid)` rather than skipped silently (which would log a
  clean op that never took effect). A materialized move updates `voice_occupancy`
  too, so a later insert sees the freed/changed span. A *non-metric* placement
  change is still deferred (re-sorting a non-metric voice is out of scope), and a
  malformed (empty) pitched replacement is still skipped; same-placement field
  edits apply as before, preserving voice membership. The LWW bookkeeping records
  the modify in every case. **For the spec:** the catalog §ModifyEvent (M2e)
  prototype boundary moves from "placement-change deferral" to "metric
  placement-change materialization with an invariant-3 precondition"; partial
  trimming of a tuplet member, and non-metric moves, remain later refinements.

## DeleteEvent re-anchoring: the graph follows the ledger

A `DeleteEvent` that tombstones an event runs the re-anchoring rule table
(Chapter 6 §6.5) over the cross-cutting structures that referenced it. The
*ledger* decision (`reanchor_for_tombstone`) and the *graph* mutation
(`materialize_graph_delete`) MUST agree on whether a structure survives — else an
object is `Live` in `MaterializedState` but gone from the graph (or vice versa),
a faithfulness gap the graph convergence gate (criterion 1) would surface.

- **Slurs and spanners re-anchor, not unconditionally drop.** The ledger keeps a
  slur/spanner `Live` while ≥1 endpoint survives (re-anchored) and cascade-deletes
  only when none does. `materialize_graph_delete` now mirrors that exactly:
  an endpoint-deleted slur/spanner re-anchors onto its surviving endpoint (the
  structure stays in the graph) and is removed only when no endpoint survives.
  Previously the graph removed *any* slur touching the deleted event regardless of
  the ledger's re-anchor — the divergence this entry fixes. Ties (cascade) and
  beams (truncate-while-≥2-members, else cascade) were already consistent and are
  unchanged.
- **A re-anchored two-endpoint structure collapses onto the survivor.** With only
  two endpoints, the sole survivor *is* the other endpoint, so re-anchoring sets
  both to it (a degenerate `(B, B)` slur / spanner). This is reference-clean — the
  cross-cutting invariant requires only that endpoints reference *live* events, not
  that they differ — but musically a stand-in. A proximity-aware target (the note
  that took the deleted one's place) needs resolved positions and is deferred
  (P11-C5; `nearest_survivor` is the lexicographic stand-in).
- **Base-score spanners are now indexed for re-anchoring.** `seed_from_graph`
  records each seeded spanner's event-anchored endpoints in `structures` (as it
  already did for slurs/ties/beams), so a base spanner re-anchors through the same
  rule as a created one rather than being left dangling.
- **A `CascadeDeleteTuplets` also drops decompositions that named the tuplet.** A
  notated decomposition component records its tuplet by id (`NotatedComponent.tuplet`),
  so once the cascade removes the tuplet structure, any attachment naming it would
  dangle — `check_invariants` flags it as a cross-cutting reference that no longer
  resolves (invariant 6). `materialize_graph_delete` therefore prunes, in the same
  step, every decomposition attachment whose components reference a removed tuplet.
  Those members are tombstoned in the same cascade, so the decomposition has nothing
  left to describe. This is what lets the editor's atomic tuplet overwrite (a pencil
  insert over a triplet member removes the whole triplet) leave an invariant-valid
  graph even when a member carried an in-tuplet decomposition.

This consistency is what lets `graph_edit_session` create cross-cutting structures
and delete their endpoints, giving the Group-2 CRUD ops (and slur re-anchoring)
real at-scale coverage under criterion 1 + `check_invariants`.

## M2c (Group 3) — structural containers: empty-only delete

The structural-container CRUD ops (`CreateRegion`/`DeleteRegion`,
`CreateStaffInstance`/`DeleteStaffInstance`, `CreateVoice`/`DeleteVoice`) mint an
**empty** container (set-union creation) and tombstone an **empty** one. Two
scoping calls the user made fixed the shape: the slice covers *structural
container* CRUD (not the document root / canvas / global staves — those stay K1),
and delete semantics are **empty-only (precondition)**.

- **A delete of a non-empty container is a precondition no-op
  (`ContainerNotEmpty`), not a cascade.** The caller deletes contents first.
  *Rejected alternative:* a cascading delete that tombstones the live children
  transitively — it conflates two intents (remove this container vs. remove its
  contents), and a cascade's re-anchoring interactions (a deleted voice's events
  feeding the cross-cutting re-anchoring table) are a strictly larger design than
  the slice needs. The empty-only gate is the conservative floor a cascade could
  later build on. Catalog §Structural Containers (M2e) states this normatively.
- **A create must carry an empty container, too — for *every* typed child
  object, not just the structural hierarchy.** `create_region` /
  `create_staff_instance` / `create_voice` reject (`ContainerNotEmpty`) a carried
  value bearing any nested object with a distinct `TypedObjectId`: a region's
  staff instances, barline-alignment groups, or graphic objects; a staff
  instance's voices or measures; a voice's events. Graph creation clones the full
  value into the score, so a carried child would materialize an object the reducer
  never mints in its `objects` bookkeeping — a graph/ledger faithfulness gap. The
  check reads the carried value only, so `reduce()` and `reduce_onto()` agree.
  *(Review-driven: the first cut checked only the hierarchy vectors;
  `barline_alignment_groups` / `graphic_objects` / `measures` were added after.)*
- **Live-child sets are tracked in the reducer, not re-derived from the graph.**
  `region_instances: RegionId → {StaffInstanceId}` and `instance_voices:
  StaffInstanceId → {VoiceId}` (a voice's live events are read from
  `voice_occupancy`) are maintained by the create/delete materializers and seeded
  by `seed_from_graph`, so the empty-only precondition is decided identically in
  the base-free `reduce()` and graph-aware `reduce_onto()`.
- **Graph creation maintains the region staff extent.** An empty staff-based
  region with a freshly-created staff instance would violate `RegionExtents`
  (the staff extent must list exactly the manifested staves) unless the extent is
  updated as instances are added/removed; the create/delete materializers do so,
  and `valuegen::region` carries a far-future time extent so a fresh region does
  not overlap an existing one once a staff instance lands.

## M2d (Group 4) — score settings: per-op discipline (review-hardened)

The score-settings ops (`SetMetadata`, `SetMetricGrid`, `SetUserPageBreak`) are
all field overwrites, but they do **not** share one discipline; the M2d review
(closed in commit `d93baac`) pinned each to the discipline its catalog
classification names. Recorded here because the review changed code/tests.

- **`SetMetadata` is an advisory LWW — no conflict.** The catalog
  (§set-user-system-break "LWW advisory") and core_spec already classified
  metadata this way; the first implementation wrongly raised a
  `StructuralFieldCollision` on a concurrent differing write, which could make a
  clean concurrent metadata edit turn `MaterializedState::is_clean()` false. It
  now silently last-writer-wins in canonical order (graph singleton overwrite,
  always `Applied`, no working slot). *Direction chosen:* fix the implementation
  to match the already-correct spec, not the reverse.
- **`SetMetricGrid` is a structural overwrite with two preconditions.** It keys a
  `StructuralFieldCollision` (field `metric_grid`) on concurrent differing grids
  (kept), *and* (a) preconditions the region live **and staff-based** — a
  FreeGraphic region has no metric-grid slot — and (b) rejects a grid whose meter
  sequence names a time signature that is not a live object (the Chapter-5
  invariant forbids installing such a grid). Both checks read only base-free
  indices.
- **`SetUserPageBreak` mirrors `SetUserSystemBreak` exactly, under the canonical
  LWW key.** Both now (i) share the live-and-staff-based precondition via a
  `staff_based_regions` index, so for any region *represented in reducer state*
  `reduce()` and `reduce_onto()` reach the same missing / tombstoned / FreeGraphic
  verdict — that is, regions an op stream creates or deletes. (`reduce_onto(base)`
  additionally seeds the base regions into that state, which a base-free `reduce()`
  does not see, so a layout op targeting a live *base* region can still apply under
  `reduce_onto` and no-op under `reduce`; the corpus exercises only op-created
  regions, where the two agree.) Both also (ii) materialize the graph break
  under the anchor's **resolved musical position** (`apply_break_lww` +
  `resolved_anchor_position`): any existing anchor resolving to the same position
  is dropped before the new one is added, so the graph break list stays in lockstep
  with the resolved-position-keyed ledger map. The system-break fix is a sibling
  parity change, not new M2d scope.
- **Performance.** The dedicated 10K-envelope reducer micro-bench (criterion 5)
  is Agent F's worklist F1; the M2 value-typed ops are already exercised at
  10K·scale by the conformance suite's reduction-determinism and convergence
  gates, which emit every M2 kind. No criterion bench is added under Agent K.

## Pass 11 candidates (ambiguities for the spec, not resolved in code)

### P11-C1 — operation payload schemas are deferred; we carry identifiers + fingerprints

> **RESOLVED (Phase 2, Agent K — Operation Catalog, M1).** The representative
> payloads are now **value-typed**: `InsertEventOp { staff_instance, event:
> Event }`, `RespellPitchOp { pitch, spelling: PitchSpelling }`,
> `CreateCrossCuttingOp { structure: CrossCuttingValue }`,
> `ChangeRegionTimeModelOp { …, new_time_model: RegionTimeModel }`,
> `SetUserSystemBreakOp { …, anchor: TimeAnchor }`, and
> `TupletCompensation::ReplaceWithRest { rest: Rest }`. They serialize by framing
> each value's `epiphany_core::CanonicalValue` bytes behind a `u32` length prefix
> — the ratified byte-convention baseline (Pass 11 item 1.8,
> `req:format:codec-conventions`), introducing no new layout (the K↔J seam; see
> `epiphany-core/DECISIONS.md`). Graph-aware reduction now materializes the
> **real** event/structure rather than the C4 placeholder described below.
> `reduce_onto`'s reduction rules are unchanged — only the field read-sites moved
> onto the value. The v0 identifier-only shapes are frozen in `src/v0.rs` as the
> migration regression guard, and `src/migrate.rs` lifts a v0 envelope to v1
> deterministically and equivalence-preservingly (`migrate_v0_envelope(v0,
> context: &Score)`; `epiphany-testkit::migration` is the merge gate). The full
> K0 set + the literal wire layout (Binary Format companion, Agent J) follow; the
> remainder of this entry is the historical v0 rationale. See `P12-K1` below and
> `spec/operation_catalog.tex`.

Chapter 6's payload structs embed rich graph values (`InsertEventOp { event:
Event }`, `RespellPitchOp { new_spelling: PitchSpelling }`, …), but the
*canonical wire encoding* of those graph value types is itself deferred to the
Binary Format companion (Agent B's P11-4: `epiphany-core` canonically encodes
only identifiers and the scalar time types). An `OperationEnvelope` must be
hashable **today** — the `EnvelopeHash` and slot equivocation both need canonical
bytes — so this crate's payloads carry the reduction-relevant *identifiers and
canonical scalar coordinates*, plus a `ContentHash` fingerprint where the
reduction needs only equality (a respelling). Graph-aware reduction materializes
this projection as deterministic C4 pitches (or a rest when no pitch ids are
present); those placeholders do not claim to recover musical values absent from
the payload. **For the spec:** pin the payload
schemas (the Operation Catalog companion) and the canonical encoding (the Binary
Format companion); when they land, the structs regain their full value fields
without changing the reduction. The trigger will be a failing cross-crate
round-trip test, per the QUICKSTART process notes.

### P11-C2 — `IntegrityAnomalyId` derivation is unspecified

Chapter 5 gives `IntegrityAnomaly` an `IntegrityAnomalyId` but does not pin how
it is derived. Because anomalies are deterministic facts of reduction, the id
must be content-derived (two replicas must agree). This crate derives it as
`derive_system_id(MUSCSANM, kind.canonical_bytes())` in the `SYSTEM_DERIVED`
namespace — the same discipline Chapter 5 uses for system identifiers, with a new
`MUSCSANM` extension system tag. **For the spec:** confirm the derivation (and
whether a built-in `MUSCS…` tag should be reserved for anomalies, as `MUSCSVCE`/
`MUSCSPCH` are for voices/pitches).

### P11-C3 — which participant's *effect* carries `Conflicted` in a field collision

For concurrent differing `RespellPitch`es, the spec pins the **conflict record**
(kind `StructuralFieldCollision`, with `winner`/`loser` and the loser's spelling)
and says the later-in-canonical-order operation wins and materializes. It does
not pin which participant's `OperationEffect` is tagged `Conflicted`. This crate
tags the **winner** (the later op, which materializes and whose processing
created the record) `Conflicted`, and leaves the earlier op's already-recorded
`Applied` effect in place. The outcome is order-independent because canonical
order is fixed. **For the spec:** pin the per-operation effect tag for a field
collision (and whether the superseded loser should retroactively read
`NoOp{SupersededByLaterOperation}`).

### P11-C4 — voice-promotion derivation inputs and the >2-collision generalization

Invariant 18's promoted-voice derivation takes *(staff instance, original voice,
winning op, losing op)*. M2 expanded `VoiceOrigin::SystemPromoted` to retain both
operation ids, so Agent B verifies the exact Agent C derivation. This crate resolves promotion
in an **order-independent pre-pass**: bucket inserts by voice, walk them by
`OperationId`, keep a non-overlapping set in the original voice, and promote
each concurrent overlapping loser to `derive_promoted_voice_id(staff_instance,
voice, winner, loser)`. This applies the InsertEvent invariant to partial
interval overlaps as well as identical start positions. The op carries its
`staff_instance` explicitly (a full reducer recovers it from the voice's
container). One open point remains for the spec: define the >2-way collision
case — the spec describes a pairwise rule; this crate uses the first lower-id overlapping
operation retained in the original voice as the winner for each promotion.

### P11-C5 — "nearest surviving anchor" needs resolved positions

The re-anchoring total order (Chapter 6 §6.5) ranks surviving candidates by
containment proximity, then absolute time distance, then direction, then id. The
prototype does not yet track resolved positions/time per object, so
`nearest_survivor` uses a deterministic **stand-in**: the lexicographically-
smallest surviving endpoint. The *structure* of the rule table (Tie →
cascade-delete, Comment → orphan, Beam → truncate, Slur/Spanner → reanchor-or-
cascade) is implemented faithfully; only the metric "nearest" is approximated.
**For the spec:** no change needed — this resolves once the graph mutation phase
tracks positions; recorded so the approximation is explicit.

*Push-3 update (2026-07): the four-key "nearest" metric is now implemented
(`nearest_live_event` + `containment_rank`, over the canonical ledger indices)
and drives the marker and graphic-gesture rows plus the slur/spanner
`Reanchored` reason; see "Re-anchoring rule-table completion" below. For
slurs/spanners the table itself prescribes surviving-endpoint collapse, so
`nearest_survivor` legitimately remains the endpoint-set minimum (proximity-
aware re-targeting beyond the endpoints stays the table's own "deferred
refinement"). Wall-clock distance in proportional regions remains
unimplemented — the occupancy index is metric-only — so a wall-clock referent
falls to the kind's declared failure action.*

### P11-C6 — time-model compatibility is computed when a graph is available

`ChangeRegionTimeModel` retains a `declared_incompatible` list for base-free
reduction. Graph-aware reduction additionally derives incompatibilities from
every event's actual coordinate variants and mapping coverage, refusing any
migration that would violate Agent B's coordinate discipline. Concurrent
same-region migrations conflict; causally-later migrations are reevaluated
against the first migration's graph. **For the spec:** the rich migration
payload still belongs to the Operation Catalog.

### P11-C7 — DVV contiguous ranges use the zero-based operation-counter floor

The DVV's contiguous `vector[r] = n` asserts predecessors `(r, 0..=n)` exist,
matching the operation-id and causal-context documentation. Reduction finds the
first absent id in every asserted range and holds the dependent pending; dots
and vector coverage of known equivocated/excluded ids remain direct blocking
signals, with transitive propagation to dependents. The range check walks known
ids rather than expanding `0..=n`, so a sparse context with a very high counter
does not cause proportional work. **For the spec:** explicitly retain the
zero-based per-replica counter floor in the normative DVV definition.

### P11-C8 — forward undo is modeled via minted-object compensation

The spec defines undo as a forward compensating edit (StrictInverse / BestEffort
/ Cascade) computed against the current materialized state. Without the full
graph-mutation phase, this crate models the compensation as tombstoning the
objects the target transaction *minted*: StrictInverse conflicts if any such
object was already tombstoned/modified; BestEffort tombstones the survivors;
Cascade is treated as StrictInverse over the same set (dependent-closure undo is
deferred with the rest of the catalog). Graph-aware reduction also removes those
event, pitch, promoted-voice, and supported cross-cutting mints from the live
graph and records graph tombstones. **For the spec:** this is faithful to the
"content-equivalence to pre-target state" definition for insert-shaped
transactions; the inverse of every catalog primitive is the Operation Catalog's
job.

### P11-C9 — local minimal enums for open vocabularies

`TransactionCategory` (spec: open, "used by UIs and analytics") and `ObjectKind`
(used by `SystemIdentifierCollision`) are given **minimal core enums with a
`Registered(…)` escape**: `TransactionCategory ∈ {NoteEntry, Structural, Layout,
Import, Registered}`, and `ObjectKind ∈ {Voice, Pitch, Registered}` (only the
kinds the spec actually derives into the system namespace). **For the spec:**
ratify or extend these sets.

### P11-C10 — `ResolveConflict` Dismissed has no distinct payload

The spec distinguishes `Resolved` from `Dismissed` resolution states but provides
a single `ResolveConflictPayload { target, action }`. This crate maps every
applied resolve to `Resolved { action }`; `Dismissed` is reachable as a state but
is not authored by a representative op. **For the spec:** define how a
`ResolveConflict` selects Dismissed (a distinct action, or a separate payload).
*Phase 2 update: the Operation Catalog (Ch. ResolveConflict) records that
`ResolutionAction::Dismiss` is the action that selects `Dismissed`; resolved.*

## Pass 12 candidates (Agent K — Operation Catalog)

### P12-K1 — a v0 `RespellPitch` fingerprint is not invertible to a spelling

The v0 `RespellPitchOp` carried a `ContentHash` *fingerprint* of the new
spelling, not the `PitchSpelling`. The v0→v1 migration (`src/migrate.rs`) must
reconstruct the value, but a fingerprint cannot be inverted without a side table.
The migration recovers the spelling from the score-graph **context** — an
explicit per-pitch spelling attachment (`SpellingScope::Pitch` +
`SpellingDirective::Explicit`) whose canonical bytes hash to the fingerprint —
and, when the context lacks it, returns `MigrationError::Irreversible` (the
bundle opens read-only, per the QUICKSTART migration contract). This is the one
representative payload that is not self-contained under migration. **For Pass
12:** confirm the read-only fallback is the intended long-term disposition (vs.
requiring a richer v0 corpus that preserves spelling pre-images). Recorded in
`spec/PASS12_BATCH.md` and `spec/operation_catalog.tex`
(§RespellPitch, §Migration).

## Provisional canonical encoding (mirrors Agent B's P11-4)

The composite Chapter 6 types use a concrete, reversible canonical byte form
(little-endian integers; `u32` length prefixes on every variable-length part;
NFC + length-prefixed text; single-byte discriminants) so that envelope hashing,
conflict-id derivation, and the materialized-state bytes are testable now. This
is deterministic and unambiguous but **provisional**: when the Binary Format
companion lands, reconcile `encode.rs` and the per-type `CanonicalEncode` impls
with it. A failing cross-crate round-trip test is the trigger.

> **Ratified (2026-07-02):** `spec/binary_format.tex` v0.1.0 Chapter 6 pins
> this crate's wire forms exactly as implemented — the envelope field order and
> its normative id-leads property, the `OperationPayload` (0..=3) and
> `OperationKind` (0..=23, append-only) discriminant tables, per-payload
> framing, `OperationKindTag`'s separate space, the 28-byte stamp, the DVV
> layout, and the full effects/conflict/anomaly/`MaterializedState` vocabulary.
> The reconciliation trigger never fired: the companion was transcribed from
> this crate and its golden anchors.

## Spec-compliance audit follow-up (2026-07, Push 1)

Four reduction-semantics fixes closing MUST-level gaps the six-agent spec audit
confirmed, plus one parity check. Each changes canonical effect bytes only in
the scenario it fixes; all determinism gates (permutation invariance, 10k fuzz,
migration equivalence) stay green.

- **Transpose skips tombstoned targets.** The catalog (§Transpose,
  re-anchoring) says "tombstoned targets are skipped (the transpose applies
  only to live pitches)"; the code refused the whole operation on any dead
  target. Now: missing target → whole-op `TargetMissing` refusal (a dangling
  reference is malformed authorship, unchanged); tombstoned targets are
  skipped and the live remainder shifts (`Applied`); all-tombstoned
  degenerates to `NoOp { TargetTombstoned }` (byte-identical to the old
  behavior for that sub-case). The skip is not recorded as a repair: no
  compensating change is performed on the skipped target.

- **Marker re-anchoring is a recorded repair.** Chapter 6 §Re-Anchoring:
  "Re-anchoring actions MUST be recorded as RepairRecord entries in the
  triggering operation's effect." The graph materializer re-pointed
  event-anchored markers to their region start silently;
  `materialize_graph_delete` now returns those re-anchors as
  `RepairKind::Reanchored { …, reason: ExplicitFallback }` records
  (`ExplicitFallback` because region-start is the documented P11-C5 stand-in
  for the table's "nearest event in same staff instance" metric, which stays
  deferred). Delete and undo effects carry them. Bookkeeping-only `reduce()`
  cannot see markers (they are graph state), so the records appear only under
  graph-aware reduction — same documented asymmetry class as base-only
  regions.

- **System-derived counter collision check** (Chapter 5 §"System-Derived
  Counter Collisions" — the audit's top reduction MUST gap). The reducer now
  keeps a mint registry `(ObjectKind, counter) → (canonical inputs, minting
  op)` seeded from the base graph (`SystemPromoted` voices via the `MUSCSVCE`
  preimage, `SYSTEM_DERIVED` pitches via `canonical_pitch_bytes`, now public
  in epiphany-core for exactly this) and checked by a pre-walk over the
  canonical order before any effect is applied. Prospective mints are the
  promotion pre-pass assignments plus `SYSTEM_DERIVED` pitches carried by
  minting payloads (InsertEvent, InsertIdentifiedPitch). On the first
  differing-inputs collision: a `SystemIdentifierCollision` anomaly is
  recorded (both input sets retained), reduction does not continue past the
  collision point, and the earlier occupant op is held too, so *neither*
  input set occupies the collided counter. Held ops surface in `pending` under
  the new `PendingReason::HaltedBySystemCollision { at }` (discriminant 4,
  additive); transactions with a held member are wholly held, and causal
  dependents hold behind them (`DependsOnPending`). Scope decisions, made
  deliberately: (a) the pre-walk is conservative — a claimed mint participates
  even if the op would fail an unrelated apply-time precondition, since two
  input sets contending for one counter is a structural identity failure
  regardless of which contender materializes; (b) a base occupant (owner =
  None) cannot be evicted by this reduction and is left to diagnostic
  recovery; (c) in-place content rewrites of a live system pitch (ModifyEvent /
  ModifyIdentifiedPitch) are *not* treated as mints — that is Invariant-11
  territory. **For Pass 12:** should reduction refuse content modification of
  a `SYSTEM_DERIVED` pitch outright (it silently invalidates the id's
  content-derivation)? Reader-side diagnostic-recovery gating on a bundle
  whose state carries this anomaly is bundle/editor work, tracked with the
  edit-barrier seam.

- **ResolveConflict meta-conflict names both resolvers.** The spec requires a
  true conflict's `caused_by` to name "at least two" operations; the
  meta-conflict for a differing later resolve had `winner == loser == self`
  and one cause. Now: winner = the earlier resolver (its action stands),
  loser = the later differing resolver, `caused_by` = both. `affected_objects`
  stays empty — a conflict record has no `TypedObjectId` kind, so the
  contested conflict cannot be named there (spec-side gap, Pass-12-adjacent).
  The related asymmetries (a causally-later resolve cannot supersede; a
  differing resolve against `Dismissed` reads `AlreadyApplied`) are spec-gray
  and deliberately unchanged.

- **Base-free pitch-id freshness.** `insert_event` now refuses a carried pitch
  id that already exists in canonical state under bookkeeping-only `reduce()`
  too (the graph-aware precondition already did, with the same
  `TargetTombstoned` reason), closing one base-free/graph-aware divergence
  from the audit. Placed after the graph precondition so graph-aware effect
  bytes are unchanged.

Reserved effect vocabulary (`OperationEffect::TombstonedTarget`,
`NoOpReason::SupersededByLaterOperation`, `PositionOutsideRegion`,
`PitchSpaceMismatch`, `ReanchorResult`) is now annotated as reserved at the
type definitions with the condition under which each becomes load-bearing.

## Re-anchoring rule-table completion (2026-07, Push 3)

The remaining referent rows of the re-anchoring rule table (core_spec §"The
Re-Anchoring Rule Table") are now implemented: **marker**, **cue event**,
**comment**, **analytical annotation**, and **graphic gesture** — together with
the four-key "nearest" total ordering (§"Total Ordering for Nearest") they
depend on. No discriminant was added or renumbered anywhere; every record uses
the ratified `RepairKind`/`ReanchorReason` vocabulary.

- **The four-key "nearest" is computed from the canonical ledger indices.**
  `nearest_live_event` takes the strict lexicographic minimum of (containment
  proximity, absolute rational time distance from the referent's resolved
  position, forward-before-backward, ascending `EventId` — whose numeric order
  *is* its canonical 16-byte order) over live events within the kind's declared
  proximity bound. Proximity (`containment_rank`: same voice 0, staff instance
  1, staff 2, region 3, canvas 4) reads `instance_voices`, the new
  `instance_staff` map (seeded + maintained by `CreateStaffInstance`), and
  `region_instances`; distance/direction read `voice_occupancy`. All are
  base-free indices, so `reduce()` and `reduce_onto()` rank identically
  wherever both can represent a scenario. Occupancy is metric-only, so
  wall-clock distance (proportional regions) is a deferred refinement: a
  wall-clock referent finds no candidate and falls to the kind's declared
  failure action.
- **Where the rows run: the graph arm, one decision per row.** None of the five
  kinds is creatable by an operation — they exist only in seeded base graphs —
  so their rows run in `reanchor_event_referents`, called from
  `materialize_graph_delete` (hence from both the `DeleteEvent` path and undo's
  `materialize_graph_tombstones`). Each row's ledger record and graph mutation
  are decided together, in canonical id order, keeping "the graph follows the
  ledger" (above) intact; `reanchor_for_tombstone` explicitly skips these kinds
  so no row is double-recorded. The referents are indexed in the same
  `structures` map as slurs/ties/beams/spanners (markers by event anchor,
  comments/annotations by event-anchored annotation anchors, gestures by
  `Events`/`Range` event references, cues — keyed by their own `EventId` — by
  their source lists); the four creatable kinds' create/modify/delete paths are
  unchanged.
- **Marker: nearest event in the same staff instance replaces the Push-1
  region-start fallback.** The re-anchored anchor keeps its offset (the
  survivor shares the staff instance, hence the region and its offset
  discipline), and the recorded reason names the *achieved* proximity rank
  (`SameVoiceNearer` when the survivor shares the voice). On failure the marker
  **orphans**: kept live in ledger and graph with `RepairKind::Orphaned`; the
  graph anchor degrades to the containing region's start purely as reference
  hygiene (invariant 10 rejects a dangling event anchor) — that form is no
  longer presented as a re-anchor choice.
- **Cue event: plain-text cascade on any source deletion.** Deleting *any*
  event in a live cue's `source` cascade-deletes the cue — ledger tombstone,
  graph removal, `CascadeDeleted` repair — in the same reduction step. The
  cascaded cue is itself a tombstoned event, so the full pass runs over its own
  referents transitively (a cue-of-a-cue cascades along; ties/slurs on the cue
  re-anchor through the ordinary ledger arm). The rationale-vs-action tension
  for multi-source cues ("no source is meaningless" suggests truncate-while-
  any-survives) is deliberately *not* resolved in code — proposed Pass-12 row.
- **Comment: orphan, with deterministic anchor hygiene.** The ledger records
  `Orphaned` and the comment survives everywhere; because invariant 10 rejects
  dangling anchors, the graph anchor degrades deterministically — an `Event`
  anchor to `AnnotationAnchor::Region(containing region)`, a dead `Range`
  endpoint to the region edge on its side (start → `Start`, end → `End`).
- **Analytical annotation: extent-preserving range reconstruction, else
  orphan.** An event-anchored annotation whose event dies re-anchors to
  `AnnotationAnchor::Range` with both endpoints as region-start `Musical`
  offsets covering the event's exact span (positions are region-relative, so
  the resolved extent is preserved); recorded as `Reanchored { to:
  Region(region), reason: ExplicitFallback }` (the row is a declared fallback,
  not a proximity choice). Range-anchored annotations get the same treatment
  per dead endpoint (event position plus any musical anchor offset). A
  wall-clock or indeterminate span is not expressible as a stored
  region-relative range — the annotation orphans with the comment's anchor
  hygiene; the expressibility gap is a proposed Pass-12 row.
- **Graphic gesture: nearest re-target / truncate / orphan.** `Events`
  references to the dead event re-target to the nearest survivor in the same
  staff instance (`Reanchored`, reason per achieved rank); with no candidate
  the reference drops — `SpannerTruncated { removed_members: [event] }` while
  references remain (the vocabulary's own definition, "lost members but enough
  remained", fits), `Orphaned` when the list empties (gesture kept, user
  content). `Range` anchoring "truncates": a dead endpoint moves to its region
  edge (start → `Start`, end → `End`, offset zero) with `Reanchored { to:
  Region, reason: ExplicitFallback }` — the least-surprising deterministic
  reading of the table's one-word action; proposed Pass-12 row. `Free` is never
  indexed (table: "no action").
- **Slur/spanner `Reanchored` reason is computed, not hardcoded.** The
  surviving-endpoint collapse itself is unchanged, but the reason now names the
  survivor's actual containment rank relative to the tombstoned endpoint
  (`containment_rank` over the same ledger indices; `SameVoiceNearer` remains
  the default when neither side has an indexed metric placement). Rank 4 (same
  canvas) has no ratified `ReanchorReason` variant, so it is recorded as
  `ExplicitFallback` rather than appending a discriminant — spec-vocabulary
  question, proposed Pass-12 row.
- **Known asymmetries, kept deliberately.** (a) The five rows fire only under
  graph-aware reduction — the same documented asymmetry class as the Push-1
  marker fix (base-free `reduce()` cannot see these kinds at all). (b) The
  tie/beam/slur/spanner *ledger* arm still does not run in the undo path
  (pre-existing gap, unchanged by this slice; the graph-only rows *do* run
  there via `materialize_graph_delete`).

## ResolveEquivocation + validation modes (2026-07, Push 3)

Two Push-3 items land together: the **ResolveEquivocation** meta-operation
(operation_catalog §"ResolveEquivocation", ratified this pass) and the
**validation-mode** seam (core_spec §"Validation Modes"). One payload
discriminant was appended (`OperationPayload::ResolveEquivocation` = 3); the
ratified 0..=2 and every other discriminant table are untouched. Canonical
reduction changes **only** for scenarios containing a valid resolve of an
equivocated slot — everything else reduces byte-identically (the extended
equivocation fuzz plus the unchanged `run_equivocation_fuzz` gate this).

- **Promotion is a set-level pre-pass, not a walk step.** The catalog's rule
  ("when the operation set holds an Equivocated slot for `target` and `chosen`
  names one of its candidates, the slot reduces as if it had always been
  Single") conditions on the *operation set*, so `Reducer::run` resolves it in
  step 1b, before pending computation and ordering: among Single-slot,
  non-quarantined resolves whose `(target, chosen)` is valid, the smallest
  reduction tuple — the same total HLC order `canonical_reduction_order`
  selects ready ops by — governs. The chosen candidate envelope then joins the
  reducible set *at its own canonical position*: it flows through
  `compute_pending` (its own causal gaps still hold it), transactions, voice
  promotion, and the walk exactly like a native Single, and dependents that
  were `DependsOnEquivocated` unblock. A resolved slot records no
  `OperationSlotEquivocated` anomaly; losing candidates stay only in the
  opset's diagnostic candidate store.
- **No `OperationSet` API extension was needed.** The promotion needs the
  candidate *envelope*, and `OperationSet::candidate(hash)` already exposes
  the retained diagnostic store (the CRDT property forbids dropping
  candidates, so every hash in an `Equivocated` slot resolves). `accept`
  transitions are untouched — the opset still holds the slot as `Equivocated`;
  the promoted view lives only in the reducer (`promoted_singles`, consulted
  by `env_of` so concurrency checks see the slot "as if always Single").
- **Resolve effects mirror `resolve_conflict`.** Governing resolve →
  `Applied`; later resolve naming the same candidate → `NoOp(AlreadyApplied)`;
  later *valid* resolve naming a differing candidate →
  `StructuralFieldCollision` on `FieldPath("equivocation_resolution")` with
  winner = governing, loser = later, both in `caused_by`, `affected_objects`
  empty (a slot is not a `TypedObjectId`) → `Conflicted`. A resolve whose
  target is absent, holds a Single slot, or whose `chosen` names no candidate
  is a precondition no-op reusing `TargetMissing` — the named target/candidate
  pair does not exist — rather than a new appended reason.
- **Deliberate single-pass simplifications (proposed Pass-12 rows).** (a) A
  promoted candidate that is itself a `ResolveEquivocation` does not govern a
  further promotion (no cascade/fixpoint; the catalog names none). (b) The
  promoted candidate is not re-subjected to HLC-monotonicity segmentation
  (quarantine detection runs over native singles first; "as if it had always
  been Single" vs. the quarantine pass ordering is a spec question). (c) A
  resolve held pending by its own causal gaps still governs promotion — the
  set-level rule needs no walk position — while its recorded effect stays
  subject to the ordinary pending rules.
- **v0 migration.** v0 predates the entry, so `V0OperationPayload` gains a
  `ResolveEquivocation` variant carried **verbatim** — the same "v1-native,
  round-trip by identity" treatment as the Group 1–4 kinds. `project_v1_to_v0`
  stays total; `migrate_v0_envelope` maps it back by identity (deterministic,
  trivially equivalence-preserving).
- **Golden locks added.** `operation_kind_wire_discriminants_are_golden` pins
  all 24 `OperationKind` literals (0..=23),
  `operation_payload_discriminants_are_golden` pins the payload union
  (0..=3 incl. the appended variant), and
  `resolve_equivocation_payload_encodes_target_then_hash` pins the 16+32-byte
  layout — the append-only discipline is now enforced by test, not convention.
- **Validation modes: the reducer *is* replay mode.** `src/validate.rs` adds
  `ValidationMode { Authoring, Replay }` and
  `advisory_violations(kind, score)`. `reduce`/`reduce_onto` enforce exactly
  the invariant preconditions in every context; authoring enforcement happens
  in epiphany-editor-core, which runs `advisory_violations` against its
  current materialized score *before minting* and refuses with a new
  `EditorError::AdvisoryViolation` (no envelope enters the log). Canonical
  reduction behavior and bytes are untouched by the mode machinery;
  `AdvisoryViolation` is deliberately **non-canonical** (no encoding, no
  discriminants — it never enters effects or state).
- **Advisory inventory (core_spec §6.10).** Implemented: InsertEvent /
  ModifyEvent *duration-not-crossing-region-boundary* (the span straddles the
  region's musical end bound — resolvable when the extent's end anchor is
  region-start-anchored with a `Musical` offset, the same sound-but-incomplete
  discipline as `Region::overlaps_in_time`), and CreateCrossCutting(Slur)
  *not-spanning-a-region-boundary* (endpoint events resolve to different
  regions). Blocked on the truncated data model, documented in the module
  docs: InsertEvent *pitch-within-instrument-range* (`Instrument` carries no
  range field; staged to the Binary Format companion) and the Slur rule's
  "unless explicitly permitted by region configuration" (no such flag on
  `Region`; spanning is treated as never permitted until it lands). A
  wall-clock or symbolic region extent yields no musical bound, so the
  boundary check passes vacuously there (deferred tempo/measure resolution,
  P11-C5).

## Edit-barrier bridge: `OperationKindTag` decode (2026-07, Push 3)

- **`OperationKindTag` gains its decode mirror.** The tag had `CanonicalEncode`
  only (the discriminant byte, plus the registry id's 16 big-endian bytes for
  `Registered`); the edit-barrier blob codec (epiphany-layout-ir, the barrier
  owner) needs the inverse to read `prohibited_operation_kinds` back out of a
  manifest declaration. `CanonicalDecode` now decodes exactly the encoder's
  image: variable width (1 byte, or 17 for `Registered`), length mismatches and
  trailing bytes rejected, and an **unknown discriminant rejected** — via
  `DecodeError::MalformedDomainTag`, the same unknown-discriminant rejection
  `TypedObjectId::decode_canonical` already uses, so no new error variant and
  no change to the determinism crate. No encoding changed and no discriminant
  was appended; `operation_kind_tag_decode_mirrors_encode_exactly` /
  `operation_kind_tag_decode_rejects_malformed_bytes` pin the contract.

## Subquadratic `canonical_reduction_order` (2026-07, the F1 → K fix)

- **The defect (F surfaces, K fixes).** The testkit's F1 bench
  (`crates/epiphany-testkit/benches/reduction.rs`, Chapter 10: > 10,000
  envelopes/s cold) documented `canonical_reduction_order` as O(n²) twice
  over: a literal double loop over all (predecessor, successor) pairs to
  build indegrees, and a full ready-scan per emission. Measured pre-fix:
  ~155K / ~12.5K / ~1.7K env/s at 1K / 10K / 50K envelopes (50K ≈ 29 s per
  cold reduce — the documented xfail row).
- **The algorithm: term decomposition + monotone thresholds, never pairs.**
  Materializing edges is inherently quadratic for the common chain shape
  (every DVV floor covers the whole replica prefix, so covered *pairs* are
  Θ(n²)); the rewrite therefore never enumerates pairs. Each causal-context
  entry becomes one *requirement term* over the present set:
  - *Vector floor `(r, n)`* — covers exactly the present envelopes of replica
    `r` with counter `<= n` (zero-based floor, P11-C7): a **prefix** of the
    replica's lane sorted by `(counter, slice index)`. The term is satisfied
    when the lane's emission **frontier** (first unemitted slot, monotone)
    passes the prefix; if the floor covers the envelope's own id, only the
    self-pair is exempt, so that term instead reads "frontier at own slot and
    **second frontier** past the prefix" (both monotone). Terms park in
    per-lane `BTreeMap`s keyed by the threshold slot and are drained exactly
    once as the frontiers advance.
  - *Explicit dot* — covers exactly the present envelopes bearing that id;
    satisfied when the id's unemitted multiplicity reaches 0 (1 for a
    self-dot, which covers only duplicate-id twins).
  An envelope is ready when its unsatisfied-term count reaches zero; ready
  envelopes sit in a `BinaryHeap` keyed by `(reduction tuple, slice index)`.
  A pre-sorted `(tuple, index)` list with a cursor supplies the malformed-
  cycle fallback (heap empty, envelopes remain). Total work is
  `O((n + Σ|context|) log n)`.
- **Why the order is byte-identical (the consensus argument).** The order is
  *defined* by: edge `p → s` iff `p != s` and `s.context.covers(p.id)`;
  ready iff every present covered predecessor emitted; emit the minimum
  `(reduction tuple, slice index)` among ready (slice index reproduces
  `min_by_key`'s first-minimum rule — reachable only under duplicate-id
  tuple ties); if none is ready, the same minimum over all unemitted (cycle
  break). The rewrite computes the *same readiness predicate*: each term is
  satisfied iff all envelopes it covers are emitted, every covered
  predecessor is covered by at least one term, and terms cover only covered
  predecessors — so "all terms satisfied" ⇔ "all covered predecessors
  emitted", including the exempt self-pair (a floor/dot naming the
  envelope's own id) and envelopes covered by both a dot and a floor (the
  conjunction needs no per-pair dedup; the redundant term is harmless).
  Absent context entries (unknown replicas, floors below every present
  counter, absent dots) yield no term, exactly as they yield no edge. Same
  edge relation, same ready set, same total order on ready ⇒ the same
  emission sequence, element for element.
- **The retained oracle + regression gate.** The pre-fix implementation is
  kept verbatim as `canonical_reduction_order_reference` (`#[cfg(test)]`),
  and three property tests assert element-for-element *pointer* equality of
  both orders (the strictest check — it distinguishes byte-identical twins):
  `canonical_order_matches_reference_on_fuzz_sets` (the crate's well-formed
  fuzz generator, 250 seeds × 2 slice orders),
  `..._on_adversarial_sets` (400 randomized hostile sets: self-covering
  floors, dots to present/absent/own ids, duplicate ids with tied stamps,
  `SYSTEM_DERIVED` replicas, stamps contradicting the causal edges, empty
  contexts), and `..._on_directed_shapes` (a 2,000-envelope full-coverage
  chain with descending stamps, self-covering and dot-only chains, dot
  2-/3-cycles, mutual-floor cycles, twin permutations). Mutation-checked:
  breaking the self-exemption or the self-dot threshold fails the suite.
- **Measured post-fix (same bench, dev profile 2026-07):** ~674K / ~257K /
  ~87K env/s at 1K / 10K / 50K — the 50K row cleared its budget by ~8.7x,
  the gate printed its XPASS promotion notice, and the row was flipped to
  `Pass` in the same change (see `epiphany-testkit/DECISIONS.md` F1).

## Phase-3 first tranche: staff/meter/tempo/layout ops + value-restoring undo (2026-07)

The ratified catalog text (operation_catalog §CreateStaff, §"Meter and Tempo
Overwrites", §SetStaffLayout, and the **rewritten** §UndoTransaction) is the
contract for this tranche. Wire facts are strictly additive:
`OperationKind` discriminants `CreateStaff = 24`, `SetTimeSignature = 25`,
`SetTempoSegment = 26`, `SetStaffLayout = 27`; `OperationKindTag`
discriminants `InsertStaff = 24` (Create→Insert tag naming, cf.
`InsertRegion`), `SetTimeSignature = 25`, `SetTempoSegment = 26`,
`SetStaffLayout = 27`; `PreconditionFailureReason::TempoMapMalformed = 11`.
No other discriminant table changed. Decisions the catalog left to the
implementation:

- **The differing-value re-create / re-carry precondition reason is
  `TargetMissing`.** §CreateStaff makes a byte-identical re-create idempotent
  (`AlreadyApplied`) and a differing value under a live id "a precondition
  no-op", without naming a reason; the reducer reuses `TargetMissing` (the
  generic dangling/unusable-target reason `ModifyCrossCutting`'s malformed
  branch already uses) rather than minting a new discriminant. The same
  discipline (and reason) applies to the `TimeSignature` value a
  `SetTimeSignature` carries. A tombstoned id refuses as `TargetTombstoned`.
- **Tempo-map well-formedness is checked against the chain state, under the
  coarse resolved-position key.** The resulting-map precondition
  (`TempoMapMalformed`) evaluates the scope's prospective segment list built
  from the tempo write chains (base-seeded under `reduce_onto`), ordered by
  the same `resolved_anchor_position` key the LWW slot uses: carried-start /
  key agreement, per-shape end data (a non-constant shape needs `end` +
  `end_tempo`; a constant `end_tempo` must equal `start_tempo` — the graph
  invariant checker's rules), resolved ends ≥ their start and ≤ the next
  key. Removals cannot malform a map (the gap rule holds the prior tempo) and
  always apply.
- **Graph normalization on removal.** A meter removal that empties a grid the
  meter writes themselves created normalizes the region's
  `default_metric_grid` back to `None` (unless a whole-grid write or the base
  holds a grid independently); a tempo-segment removal that leaves a region's
  local map empty and `initial`-less normalizes `local_tempo_map` to `None`
  (an empty local map would *shadow* the score map rather than fall back to
  it). Both keep "create on set" and "remove last" inverses of each other.
- **`CreateStaffInstance` now preconditions a live staff — graph-aware only.**
  §CreateStaff makes the check normative now that staves are mintable; like
  the insert preconditions, it is enforced when a graph is present (base-free
  reduction has no staff universe), which keeps every existing seeded-base
  scenario vacuously green. `CreateStaff`'s own instrument/group resolution
  preconditions are graph-aware for the same reason.

### Value-restoring undo: the write-chain design

Every LWW overwrite family now maintains, per key, the **canonical-order write
chain** `WriteChain<V> { base: Option<V>, writes: Vec<(op, tx, value)> }`
(replacing the former `last_*` last-writer maps; each chain's last write is
still the LWW concurrent-differing comparison point). Families: event modify,
identified-pitch modify, respell, cross-cutting modify, metadata, metric grid,
meter change (new), tempo segment (new), staff layout (new), and the user
system/page breaks. The reducer applies operations in canonical order, so
appends are inherently canonical and every verdict below is
permutation-invariant by construction (pinned by
`undo_restoration_is_permutation_invariant` and the convergence gates).

- **Seeding.** `seed_from_graph` seeds each chain's `base` with the base-graph
  value (events, pitch values, cross-cutting values, metadata, grids, meter
  changes, tempo segments, staff-instance layout fields, break anchors, and
  explicit user-chosen per-pitch spelling attachments). Mints seed the chains
  they create state for (`InsertEvent` → event + pitch chains,
  `InsertIdentifiedPitch` → pitch chain, `CreateCrossCutting` → structure
  chain, `CreateRegion` → grid/meter/break/tempo chains from the carried
  content, `CreateStaffInstance` → layout chain), so a chain-predecessor is
  defined identically under `reduce()` and `reduce_onto()` for op-minted
  objects. Only base objects in base-free reduction lack seeds — the
  established API-divergence caveat (see M2d) applies unchanged.
- **Undo verdict per written key** (`WriteChain::undo_verdict`): if the target
  transaction's write is still the key's last, restore the chain-predecessor —
  the latest non-member write, else the base value, else *absence*; if a later
  writer superseded it, `StrictInverse` refuses the **whole** undo with a
  `TransactionConflict` (`caused_by` = undo + the canonically-first
  superseding writer) while `BestEffort` restores the still-last keys and
  skips the superseded. Keys whose owning object is tombstoned — or is itself
  one of the transaction's mints, about to be tombstoned by the same undo —
  are skipped entirely (no live slot to restore; not a conflict).
- **Absence semantics per key.** Optional slots restore literal absence: the
  grid clears, the meter change / tempo segment / break is removed (with the
  normalizations above). The canonical bookkeeping maps (`spellings`,
  `breaks`, `page_breaks`) return to **key-absence** whenever the predecessor
  is the *base* value — base state lives in the graph, not the operational
  ledger — while the graph restores the base value (spelling attachment,
  break anchor). Always-valued families (event, pitch, cross-cutting,
  metadata, staff layout) with no known predecessor (reachable only base-free,
  for pre-horizon objects) restore nothing — a bookkeeping-only outcome.
- **Effects.** A fully clean compensation is `Applied`; `AppliedWithRepair`
  carries **only** the minted-object tombstone repairs (`CascadeDeleted`), per
  the catalog — restorations add no repair vocabulary. A transaction that
  minted nothing and wrote nothing this reduction knows of stays the
  `TargetMissing` no-op (the pre-tranche behavior); a transaction that *did*
  write chains is now genuinely undoable — the sanctioned semantic change to
  previously-TargetMissing overwrite-transaction undos.
- **Mixed transactions** compose both passes; `StrictInverse` refuses the
  whole undo if *either* part fails (a tombstoned mint keeps the pre-existing
  `TombstonedTarget` conflict; a superseded key raises the
  `TransactionConflict` above), and the refusal applies nothing. Two new
  strand guards protect the mint pass: a minted `Staff` still manifested by a
  live non-member instance, and a minted `TimeSignature` still referenced by
  a meter change that survives the restoration pass, refuse under
  `StrictInverse` (a `TransactionConflict` naming the blocked object and its
  referencer — reusing ratified conflict vocabulary rather than minting a new
  kind; a dedicated "stranded reference" kind is a Pass-12 question) and are
  skipped (left live) under `BestEffort`.
- **Undo-of-undo (pinned).** A restoration that restores a *value* enters the
  key's chain as a new write by the undo operation. Consequences, both
  pinned by tests: (i) undoing the first undo's own enclosing transaction
  restores the value the first undo removed
  (`undo_of_undo_restores_the_pre_undo_value`); (ii) a *second* undo of the
  same target transaction finds the key superseded by the first undo —
  `Conflicted` under `StrictInverse`, skipped under `BestEffort`
  (`a_second_undo_of_the_same_transaction_sees_the_first_as_superseding`).
  An *absence* restoration is not representable as a chain write and leaves
  the chain unchanged, so repeating it is idempotent — the one asymmetry,
  accepted until a chain-native absence entry is worth its weight.
- **Deferred residue** (unchanged from the catalog's "Still deferred"):
  delete resurrection (P11-C8), `Transpose` inversion (P12-K2) — so a
  transpose between a write and its undo is *not* re-applied on top of the
  restored value (the chain predecessor wins; transpose composition is not a
  chain write) — and `Cascade`'s dependent closure (`Cascade` remains
  `StrictInverse` over the same set).

Proposed Pass-12 rows from this tranche: (1) the `TargetMissing` reuse for
differing-value re-creates/re-carries (vs. a dedicated reason discriminant);
(2) a conflict-kind vocabulary for undo strand-blocks (live-reference
tombstone refusal) instead of `TransactionConflict` reuse; (3) whether an
undo's chain write should carry a distinguished provenance so a second undo
of the same transaction could be defined as idempotent rather than
conflicting; (4) P12-C5 stands as filed (the decomposition pre-pass still
honors only the first governing meter — the reduction semantics are pinned
here and tested under `a_mid_region_meter_change_reduces_cleanly_p12_c5`).

## Pass 12 G-pass (2026-07-07): the K and C rows are ratified

All open K/C rows are retired (dispositions in
`spec/PASS12_RATIFICATION_LOG.md`, "G-pass tranche"). Adopted as implemented:
**K1** (migration read-only fallback is long-term), **K4** (earliest resolve
governs universally — no supersede; re-resolution is a future dedicated op;
no Conflict object kind), **K6** (single-pass promotion, quarantined resolves
never govern, pending resolves govern set-level, `TargetMissing` reuse kept),
**K10** (`TransactionConflict` reuse blessed for strand-blocks), **K11**
(undo idempotence asymmetry is normative), **K12** (cross-region slur = AND),
**C1** (cue cascades on any source death), **C2** (Range truncate = region
edge, zero offset), **C3** (annotation orphaning sanctioned), **C5** (folded
into H4's single-meter bound). Deferred with named sites: **K2** (tuning
catalog; prototype pinned as declared v1 behavior, replacement = payload
schema-major), **K5** (Profile Conformance companion; v1 profiles declare no
selection function). Decided with code to land in the G-pass code tranche:
**K3** refuse SYSTEM_DERIVED intrinsic-content rewrites
(`PreconditionFailureReason::SystemDerivedContentImmutable`, discriminant 12),
**K9** differing-value re-creates get `RecreateContentMismatch` (13),
**C4** rank-4 re-anchors get `ReanchorReason::SameCanvasNearer` (6; 5 was
already `DeclaredByExtension`). **K8** retired: genesis is outside the
operation set (catalog K1 slots removed, core Ch5 states it). Catalog
0.5.0 → 0.6.0; Binary Format 0.3.0 → 0.4.0.

### G-pass code-tranche review findings (2026-07-07, all fixed before commit)

A high-effort review of the code tranche confirmed three correctness findings,
each fixed with a regression test:

1. **Transpose bypassed P12-K3** — it shifted a `SYSTEM_DERIVED` pitch's
   alteration in place, desynchronizing content from the id's derivation
   inputs and making the K3 verdict depend on where a snapshot was cut
   (a full replay's registry holds the mint content; a post-transpose
   snapshot's holds the shifted content). Fixed: system-derived targets are
   **skipped** like tombstoned ones (id-namespace filter, so base-free and
   graph-aware reduction agree); an all-system-derived transpose reduces as
   `SystemDerivedContentImmutable`. Catalog §Transpose updated.
2. **Unestablished rank-4 re-anchors were relabeled `SameCanvasNearer`** —
   `containment_rank` returns 4 both for the established same-canvas case and
   as the fallthrough when a voice's placement is unresolvable; the P12-C4
   append must not launder the latter into a positive proximity claim. Fixed:
   recording routes through `rank_reason`, which downgrades an unestablished
   4 to the honest `ExplicitFallback`; **selection order is unchanged** (the
   rank still compares as 4).
3. **Vocabulary generators lagged the appends** — testkit
   `precondition_failure_reason`/`reanchor_reason` never emitted discriminants
   12/13/6, so fuzz gates could not catch a renumbering regression. Fixed.

**Pass-13 candidate (P13-K1) — resolved (Pass 13, 2026-07-08; user: "reject
the introduction").** The K3 verdict for a system pitch *introduced by a
ModifyEvent replacement value* (never minted — the collision pre-walk excludes
ModifyEvent) differed across a snapshot cut: in-session the pitch was not Live
(slipped through — `system_mints` had no entry, so the identity check saw
nothing); after a snapshot re-seeded objects + registry from the base graph,
the same modify read `SystemDerivedContentImmutable`.

Resolved by rejecting the introduction: `modify_event` now refuses a
replacement that carries a **never-minted system-derived pitch id** (replica
`SYSTEM_DERIVED`, absent-or-not-Live in `objects`) with `TargetMissing`,
*before* the P12-K3 identity check. The verdict no longer depends on the
registry — the pitch is not live in `objects` in either frame — so both frames
refuse identically, closing the asymmetry. Scoped to the system namespace
(where the asymmetry lives: only system pitches are re-seeded as system mints):
a user-replica pitch carries no namespace claim and has no snapshot asymmetry,
so `ModifyEvent` may still introduce user pitch content (the concurrent-modify
tests rely on it). Locked by
`a_modify_event_introducing_a_never_minted_system_pitch_is_refused_p13_k1`.

## Schema major 2: minimal stamping (landed with core Phase B, deliberately)

The live codec change makes CrossCutting/Staff/Metadata payload bytes v2
immediately, so the honest stamps land in the SAME commit rather than a
later phase (the major-1 D1/D2 byte-shim approach would have needed v1
shims for nine transitively-embedded types, all throwaway).
`OperationKind::schema_major` implements the ratified **minimal-stamping**
rule (Binary Format §Schema Major 2): Create/ModifyCrossCutting, CreateStaff,
SetMetadata → always 2 (mandatory appends); CreateRegion → 2 iff a carried
staff instance bears Some(staff_lines_override) else 1; CreateStaffInstance/
SetStaffLayout → 2 iff Some(override) else 0 (None encodes byte-identically
to the prior major). Locked by `schema_majors_follow_the_minimal_stamping_rule`.
`the_canonical_base_is_byte_identical_across_data_model_majors` golden-locks
a seeded reduction's MaterializedState bytes — the companion's SHOULD that
the canonical base never moves across data-model majors.

**Review sharpening — the op-payload migrate-on-read deferral, stated
precisely:** with the live codecs at v2, op-payload BYTES from a pre-major-2
build (a persisted bundle whose CrossCutting/Staff/Metadata blocks were
stamped major 0 under the old per-kind rule) have no in-build decoder — the
frozen v1 layer covers whole-`Score` snapshots only, while binary_format's
migration table ratifies "a v0/v1 op block migrates on read". This is
ACCEPTABLE TODAY because (a) no production corpus exists (local repo, test
bundles only), and (b) no code path decodes op-envelope bytes back to values
(the bundle treats block bytes opaquely; reduction runs on in-memory
envelopes; the opindex reads only the leading id). The moment a consumer
byte-reconstructs op payloads (bundle replay of foreign documents, the P2
ops-decoder fuzzer, MusicXML round-trip tooling), it MUST bring the
op-payload migrate-on-read primitive with it — per-type frozen v1 payload
decoders keyed by the block's stamped major. Tracked as the standing Phase-C
remainder in the Push-2 plan.

## Schema major 2, Phase D — the repeat-authoring pair (code tranche)

**Stamping.** The minimal-stamping per-payload table (see "Schema major 2:
minimal stamping" above) gains the pair: `CreateRepeatStructure` ⇒ always
**2** — born at v2; the carried `RepeatStructure`'s `kind`/`voltas` are
unconditional fields, so even the migration-default value has no
lower-major layout — and `DeleteRepeatStructure` ⇒ **0** (a bare
identifier is a major-0 layout; the kind discriminant itself is a
schema-minor vocabulary append, the Phase-3 mechanism — the stamp is
always minimal stamping over the payload, never a property of the append).

**Set-union without value comparison.** A repeat create of a live id reads
`AlreadyApplied` — the cross-cutting discipline. `RecreateContentMismatch`
stays scoped to CreateStaff + the carried TimeSignature (the two sites
that retain the carried value for comparison); extending it here was
considered and declined: repeats mirror `create_cross_cutting`, whose
family the catalog places them beside.

**The all-anchors-live precondition** covers every event-referencing
anchor site — start/end, DaCapo/DalSegno jump targets, volta spans — via
`RepeatStructure::anchor_sites()` (core), THE single site-set walk.
Reduction (ledger + graph), the editor barrier seam, the invariant anchor
walk, and core's cross-reference index all consume it, after review found
five hand-rolled copies plus a SIXTH, stale one: `indexes.rs` had never
learned the Phase-B kind/volta anchors (start/end only, contradicting its
own every-referenced-object doc). Fixed with a regression test. Plain
field walks get no exhaustive-match protection; the shared method is the
guard.

**Survivor selection.** "Nearest surviving anchor" is realized as the
deterministic identifier-order minimum over the structure's surviving
endpoints — for two-endpoint slurs/spanners that was the vacuous
tie-break; for multi-site repeats it is load-bearing, so the rule-table
row and the catalog now say so explicitly, with proximity-aware (four-key)
selection deferred exactly as the spanner row defers per-kind proximity
bounds.

**Spanner ghost fix (pre-existing).** `materialize_graph_tombstones` never
removed an undone `Spanner` mint from the graph (Slur/Tie/Beam were
handled); the walk gains the Spanner and RepeatStructure arms,
regression-locked in
`undoing_a_repeat_or_spanner_create_removes_it_from_the_graph`.

**Canonical-base pin, honestly.** The 200-envelope blake3 was re-pinned
(the gen_payload modulus shift moved the whole seeded stream); review
found the seeded corpus's repeat creates all no-op, so that pin cannot
detect a repeat-value leak into the base. The dedicated
`the_canonical_base_embeds_no_repeat_values` test covers the property
surgically: two reductions differing only in the created repeat's v2
content must produce byte-identical bases.

**Pass-13 candidates filed (batch now open, see spec/PASS13_CANDIDATES.md):**
P13-D1 — undo-driven event tombstones run the graph-side re-anchor/cascade
(`materialize_graph_tombstones` → `materialize_graph_delete`) but never the
ledger-side `reanchor_for_tombstone`: a structure whose anchors are all
undo-tombstoned leaves the graph while staying `Live` in `objects`, with no
`RepairRecord` — a pre-existing class (slurs/spanners identically) that the
repeat row now also exhibits; contradicts Ch6's same-step RepairRecord MUST
under an undo-driven tombstone. P13-D2 — the cue-cascade recursion re-anchors
against the triggering event before that event's own tombstone lands in
`objects`, so a structure anchored on {X, cue-of-X} can record
`Reanchored{to: X}` and `CascadeDeleted` in one effect (plausible by code
trace, unexecuted). Neither is repeat-specific; neither is fixed here.

**P13-D1 resolved (Pass 13, 2026-07-08).** `tombstone_undo_targets` now runs
the ledger half after the graph half: it captures each event target's voice
(before `materialize_graph_tombstones` clears `voice_occupancy`), then calls
`reanchor_for_tombstone` per event target. A structure orphaned by an
undo-tombstoned anchor now cascades or re-anchors in `objects` with a
same-step `RepairRecord`, so the ledger and the already-updated graph agree —
both use the same `min`-survivor rule, so they converge on existence and
target. `reanchor_for_tombstone` gained a **liveness guard** (skip a
non-`Live` structure) so the undo's own tombstoned mints — whose stale
`structures` index entries linger — are not re-processed into duplicate
repairs; the direct-delete path drops a tombstoned structure from `structures`,
so the guard is a no-op there. Because `canonical_bytes` embeds both `objects`
and the effect log, this corrects the reduced state (an inconsistency that was
simply never exercised — no existing test broke). Locked by
`undo_orphaning_a_pre_existing_slur_cascades_it_in_the_ledger_p13_d1`
(cascade + recorded repair + order-independent convergence).

**P13-D2 resolved (Pass 13, 2026-07-08).** `delete_event` tombstoned the event
in `objects` *after* `materialize_graph_delete` — but that graph pass cascades
a cue among the event's referents, running `reanchor_for_tombstone` over the
cue's own referents while the source event is still `Live`. A slur bridging
{X, cue-of-X} therefore re-anchored onto X (recording `Reanchored{to: X}`) and
then cascade-deleted when X's tombstone landed a line later — a contradictory
same-effect trail (candidate: "plausible by code trace, unexecuted"; now
executed — reverting the fix reproduces exactly that two-record trail). Fixed
by tombstoning the event in `objects` **before** the graph delete, matching the
conventions `cascade_cue` and `tombstone_undo_targets` already follow (both
tombstone before their graph delete — which is why the undo path never had this
bug). The bridging slur now sees X already dead during the cue cascade and
cascades once. Locked by
`deleting_a_cue_source_does_not_leave_a_contradictory_repair_for_a_bridging_slur_p13_d2`.

**Noted, not implemented:** no writer path derives a chunk schema *minor*
from appended kind discriminants (a major-0 block carrying discriminant 29
stamps the same fixed minor as always) — pre-existing for the Phase-3
kinds, now also true of the delete; a reader hitting the unknown
discriminant cannot attribute the failure to version skew from the stamp
alone. A bundle-writer design item for the next bundle tranche.

### Follow-up (user review): the anchor-site set, consumed WHOLLY

Post-commit review found the Phase-D unification incomplete in exactly two
places that still collapsed `anchor_sites()` to events: (1) the mint
precondition validated only `TimeAnchor::Event` targets, so a create whose
start named a missing REGION (or a volta span a missing MEASURE) minted a
dangling repeat straight past `CrossCuttingRefsResolve` — fixed with
`anchor_object_refs` (events + measures + regions; wall-clock references
nothing), deterministic across both reduction modes since the base seed
registers regions and measures in `objects`; and (2) the editor barrier
containment derived only from event locations, so a repeat anchored solely
to a protected region carried a default context and bypassed a
region-scoped barrier — fixed with `repeat_context` (first
object-referencing site in anchor order binds: event/measure →
(region, instance); bare region anchor → the region). Both
regression-locked. The referent INDEX stays event-only by design (the rule
table repairs event tombstones; the spanner discipline).

**P13-D3 filed:** `CreateCrossCutting` has the same mint-time shape for
SPANNERS — `CrossCuttingValue::endpoints()` filters to events, so a spanner
anchored to a missing region/measure mints dangling past the invariant
(`anchor_target_exists` checks spanner anchors at all three kinds), and
non-event referent tombstones (a `DeleteRegion` under a region-anchored
spanner or repeat) re-anchor nothing.

**P13-D3 resolved (Pass 13, 2026-07-08; user: "fix the mint only").** The MINT
is fixed: `CrossCuttingValue::anchor_object_refs()` returns the full anchor
object set (events + a spanner's measure/region anchors; wall-clock references
nothing), and `create_cross_cutting`'s liveness precondition now checks it —
so a spanner anchored to a missing region/measure is refused
(`TargetMissing`) rather than minted dangling, exactly as the repeat mint's
`anchor_object_refs` fix does. Deterministic across both reduction modes (the
base seed registers regions and measures in `objects`). `endpoints()` stays
event-only — it feeds the re-anchoring referent index, and **non-event
referent re-anchoring stays deferred, ratified events-only** (the spanner
discipline; extending the referent index to region/measure tombstones is the
larger change the user parked). Regression:
`create_cross_cutting_spanner_preconditions_region_measure_anchors`.

## Push 4a — the transpose algebra (design gate, 2026-07-09)

`TransposeOp { targets: Vec<PitchId>, chromatic_steps: i32 }` was pinned in
Pass 12 (P12-K2) as a prototype whose repair would be "a payload schema-major
landing with the Chapter 4 tuning catalog". An audit reopened it. Both halves
of that pin were wrong, and the operation had more defects than the pin
admitted.

**What is actually broken.** Measured, not inferred — a probe through
`EditorSession` on a C4:

| `chromatic_steps` | result |
|---|---|
| +1 | C#4 |
| +12 | C4 with `alteration: 12` — six double-sharps, not C5 |
| +128 | `alteration: 127`, silently clamped, reports `Applied` |
| `targets: [p, p]`, +1 | `alteration: 2` |

Also: a non-`Cmn` position is left untouched while the operation reports
`Applied`. And `transpose(1000)` then `transpose(-1000)` lands on
`alteration: -128`, so the operation is not invertible.

Nothing downstream is at fault. `prepass::accidental_ids` faithfully renders
`alteration: 12` as six double-sharps; the engraver draws what it is handed.
The whole defect is in what `Transpose` *means*.

**The false coupling (the reason this looked big).** `Pitch` has two
orthogonal fields: `scale_position` and `acoustic`. Transposition adds an
interval to a *scale position*. Tuning decides what frequency a scale position
sounds at. Adding a fifth to C4 needs no tuning catalog. P12-K2's deferral
welded together two things that do not touch, and the weld propagated: it is
also why `PreconditionFailureReason::PitchSpaceMismatch` was documented as
"Reserved: requires the Chapter 4 tuning catalog" (detecting a non-`Cmn`
position reads a discriminant), and why `TranspositionInterval` was marked
"ADVISORY until the Chapter 4 tuning catalog pins interval algebra". Push 4
therefore splits: **4a is the transpose algebra, and needs no tuning catalog;
4b is the Chapter 4 catalog**, which has its own unrelated blockers (`cmn-24`
is in the spec's pitch-space table but cannot exist while `Cmn.alteration` is
`i8` semitones and a quarter-tone is half of one).

**Four ratified calls (user, 2026-07-09).**

1. **Split 4a / 4b.** Above.

2. **A new operation kind; freeze the old.** An operation is history. A replica
   replaying a stored `Transpose` must reconstruct the state its author saw, so
   a "corrected" reduction rule would silently rewrite every score that used
   one. `Transpose` (wire disc 9) keeps its exact semantics — saturation,
   nominal-blindness, duplicate-sensitivity — now written down as *normative
   replay semantics* rather than as apologies. `TransposeInterval` takes wire
   disc 30 and is what authoring emits.

   This turned out to be **cheap**: appending an `OperationKind` discriminant
   at ≥ 30 is a schema **minor** (`req:binfmt:kind-discriminants`, the Phase-3
   precedent), and every constituent of the new payload — a sequence of ids and
   two `i32`s — is a major-0 layout, so it stamps major 0 under minimal
   stamping. No schema major 3, no migration, no stored operation changes
   meaning. (I told the user it would cost a major before checking. It does
   not.)

3. **Diatonic + chromatic interval.** Reuses `TranspositionInterval`, which
   **already existed** in `graph.rs` at schema major 2 for `Instrument`'s
   written-versus-sounding interval, is already codec'd, and is byte-for-byte
   the pair required. Minting a new `Interval` struct would have been a second
   normative listing of one type — the drift hazard P13-I1 just closed. The
   spec now declares it once, in Chapter 2 where transposition lives, and
   Chapter 5 references it.

4. **Atomic refusal.** A target that cannot be faithfully transposed — non-`Cmn`
   position, `AbsoluteHz` realization, or an `alteration'`/`octave'` that does
   not fit — refuses the *whole* operation, changing nothing. Never saturate,
   never partially apply. A chord transposed except for one note is not a
   transposed chord.

   **Refusal is distinguished from skipping.** Tombstoned and `SYSTEM_DERIVED`
   targets are still *skipped*, exactly as for `Transpose`. A deleted pitch is
   not an untransposable pitch; it is a pitch the operation has nothing to say
   about. An `AbsoluteHz` pitch, by contrast, declares its own frequency: it
   can be respelled or resounded, but a transposition claiming both does one
   and lies about the other.

**`targets` becomes a set at the type level.** `CanonicalSet<PitchId>` (a
`BTreeSet`), not a `Vec` plus a `dedup()` someone can forget. `PitchId: Ord`
**is** its canonical byte order (locked by `ids.rs`), so the set iterates in
canonical order for free. This is not a convergence bug in the old op — every
replica replaying `[p, p]` double-transposes identically — it is a
canonicalization bug: the catalog said *set*, `sorted_canonical` sorts without
deduplicating, and the reduction is multiset-sensitive.

**Timing matters here.** No operation-payload decoder exists yet (only
`OperationKindTag` implements `CanonicalDecode`), so making `targets` a set is
free *today*. Once the decoder lands — Push 5, the decode-fuzz P2 tranche — any
dedup normalization would silently change the meaning of stored operations.
**Push 4a blocks Push 5**, and that is why.

**Spelling propagation.** Core Chapter 2 already requires that operations which
transpose a pitch produce `SpellingSource::Propagated` attachments. `Transpose`
produces none, and `RespellPitch` can only materialize `UserChosen` — so an
authored spelling survives a transposition still pinned to the notehead it was
authored against. `TransposeInterval` MUST emit the propagated attachment; the
interval's diatonic component already determines the nominal, and the
attachment is how that determination reaches the engraved layer.

**Spec:** `req:pitch:transposition` (core Ch2, the algebra and the three
refusals), `req:opcat:transpose-frozen`, `req:opcat:transpose-interval-targets`,
`req:opcat:transpose-interval-atomic`,
`req:opcat:transpose-interval-spelling`. Operation Catalog 0.7.0 → 0.8.0,
Binary Format 0.6.0 → 0.7.0 (disc 30; the new `seq^⇑` strictly-increasing
notation). New `PreconditionFailureReason`: `AcousticRealizationPinned` (14),
`TranspositionOutOfRange` (15); `PitchSpaceMismatch` (6) un-reserved.

**The two existing transpose tests are false locks.** Verified by mutation:
gutting `graph_transpose_pitch` entirely leaves
`transpose_skips_tombstoned_targets_and_shifts_the_live_ones` and
`transpose_skips_system_derived_targets_p12_k3` both green. They call base-free
`reduce()`, where `graph` is `None` and the function never runs, and they
assert only `OperationEffect`. Only `editor-core`'s `undo_and_redo_a_transpose`
— three crates away — catches it.

The fix was *not* to rewrite them, which the design gate wrongly promised. What
they assert — skip-tombstoned, skip-system-derived, refuse-missing — are
**effect-log** properties, and base-free reduction is the right place to assert
them. The defect was the first one's *name*: it claimed to check that the live
target "shifts" and checked no such thing. So:

- renamed to `transpose_effect_log_skips_tombstoned_targets_and_refuses_a_missing_one`,
  with its scope written into the test;
- `the_frozen_transpose_skips_a_tombstoned_target_and_shifts_the_live_sibling`
  now makes the original claim, against `reduce_onto` and an asserted pitch;
- `the_frozen_transpose_keeps_its_saturating_alteration_semantics` locks the
  three pinned defects (octave-blind, saturating, multiset) as replay semantics.
  Mutation-verified by "helpfully" repairing `graph_transpose_pitch` to use the
  real algebra: the test fails, which is the point — it is a guard against
  rewriting history, not against a bug.

**Consequences elsewhere.**

- `EditorSession::transpose_selection` now takes a `TranspositionInterval`. A
  scalar cannot distinguish "up an octave" `(7, 12)` from "C with twelve
  sharps" `(0, 12)`, which *is* P12-K2. The "+1 semitone" key became
  `alter_selection(±1)` — a chromatic alteration with no diatonic motion, which
  is what that key always meant and what the old operation always did.
  `TransposeOp` is consequently unused in `editor-core`'s lib: the compiler now
  enforces "never authored".
- `fuzz::gen_payload` gained arm 27 and `below(27)` became `below(28)`, which
  reshuffles the seeded stream, so `the_canonical_base_is_byte_identical_...`
  was re-pinned. Consciously, per that test's own instruction, and for the same
  reason as the Phase-D re-pin. Nothing leaked: `canonical_bytes` embeds
  effects, conflicts, and anomalies, never payload values.
- The frozen `Transpose` keeps its fuzz coverage (arm 6) and its corpus
  authoring in `testkit`. It must reduce correctly forever, and a generator is
  now the only thing that will ever produce one.

**Base-free reduction.** The refusal reads pitch *values*, which exist only
under `reduce_onto`. It is therefore a graph-aware-only precondition that
passes base-free — the convention `modify_identified_pitch`'s system-derived
rewrite check already established and documents ("unverifiable and passes, like
the other graph-aware-only preconditions"). It also *writes* nothing base-free,
so the two modes agree on `objects`, and on the effect log for every operation
whose targets are all transposable — the only case base-free reduction can
distinguish.

## Push 4a follow-up — the three gaps a green gate did not catch (2026-07-09)

An audit reopened P12-K2 after Push 4a landed. All three findings reproduced.
The focused suites passed throughout, which is the point: they were coverage
gaps, not regressions.

**1. Extreme intervals panicked instead of refusing.** `Pitch::transposed` did
its arithmetic in `i32` while `TranspositionInterval`'s components *are* `i32`,
so `diatonic_steps = i32::MAX` overflowed `12 * new_octave`. The comment above
it said "widen before arithmetic" — it widened `i8` to `i32`, which is exactly
not wide enough. `inverse()` negated `i32::MIN`. Now `i64` throughout, and
`inverse()` returns `Option`.

I checked whether this was worse than a panic, since `epiphany-core` is a
library and a downstream release profile has `overflow-checks` off. A 10.5M-case
sweep of wrapping-vs-exact arithmetic found **zero** inputs where wrapping
produced a wrong `Ok` rather than a refusal. So: a panic, not silent corruption.
The audit's characterisation was right and my instinct was wrong.

**2. The Propagated attachment met the requirement and missed its purpose.**
Default precedence ranks `UserChosen` and `Imported` *above* `Propagated`
(`SpellingPrecedence::default`), so the attachment a transpose writes is
outranked exactly when it matters. Reproduced: a C4 the author spelled "C",
sharpened to C♯4, resolves to `Authored(UserChosen)` with `accidentals: []`. The
notehead draws a C natural for a pitch sounding C♯ — the accidental disappears.
The Push-4a test could not see it because it deliberately started with no
attachment.

Ratified fix: authored spellings are **moved**, not left and not discarded, by
transposing their **nominal** — the nominal is what carries the enharmonic
decision. B♯3 (sounding C4) up a fifth is F𝄪4 (sounding G4), not G. Source,
priority and layer are preserved: a transposed `UserChosen` spelling is still
the user's choice. An authored spelling that cannot be written at the transposed
position refuses the whole operation, resolved before any write.

**3. Value-restoring undo did not restore either transpose.** Neither kind
recorded into `pitch_modify_chain`; `UndoTransaction(StrictInverse)` reduced to
`NoOp(TargetMissing)` and left the pitch shifted. `EditorSession::undo` works
because it re-materializes from a truncated log, an entirely separate mechanism.

The *behaviour* gap was pre-existing — the frozen `Transpose` behaves the same,
and the pre-Push-4a catalog said so honestly ("an inverse-interval undo is a
Phase-3 refinement, P11-C8"). What was new was **my false claim** that the write
chain handled it, written into the catalog for both kinds.

Ratified fix, and the interesting part: **`TransposeInterval` becomes undoable;
the frozen `Transpose` stays un-undoable, permanently.** Making `Transpose`
record into the chain would not change its own reduction rule, but it *would*
change what a stored `{Transpose, UndoTransaction}` history replays to — from
"the pitch stays shifted" to "the pitch returns". That is a change in what an
existing document means, which is the one thing the freeze forbids. So the
freeze bites, and gives another reason never to author the old kind.
`the_frozen_transpose_is_not_undoable_and_that_is_frozen_too` guards it, verified
by the inverse mutation (making it undoable fails the test).

**The new chain.** `transposed_spelling_chain: BTreeMap<PitchId,
WriteChain<Vec<SpellingAttachment>>>` holds the engraved-layer, pitch-scoped,
explicit attachment *set* per pitch, so undo restores the moved authored
attachments and removes the propagated one together. Deliberately **not**
`respell_chain`: `RespellPitch` owns that, its last write is the LWW working
state its conflict detection reads, and folding transposes in would make a
concurrent respell conflict with a transpose *and* move the canonical bytes of
every existing history.

**Two false locks, found by mutation, not by the gate.**
- The enharmonic test spelled a C♯4 pitch as "C♯", so the authored nominal
  coincided with the pitch's own and re-inference gave the same answer.
  Replacing `spelling.transposed(..)` with `simplest_spelling(..)` **passed**.
  Rewritten around B♯3-sounding-C4, it fails as it must.
- My own mutation harness restored the file between two tests, so the second
  ran unmutated and "passed". A mutation that no-ops looks exactly like a test
  that passes — the trap recorded after Push 4a, hit again in a new form.

## P13-S3 — a shared undo key with one writer (2026-07-09)

The `engraved_spelling_chain` added to make `TransposeInterval` undoable was
written only by the transpose. But `RespellPitch` mutates the same graph
attachments, and a chain with one writer is wrong in **both** directions:

- **Prior respell erased.** `respell → [tx: transpose] → StrictInverse undo`
  returned `Applied`, restored the pitch to C4, and removed *every* attachment.
  The respell was an operation, not part of the base, so it existed only as a
  chain write — and the transpose's chain had never seen it, so its predecessor
  was absence.
- **Later respell wiped.** `[tx: transpose] → respell → StrictInverse undo`
  returned `Applied` and erased the newer authoring, because the respell was
  invisible to the chain and so did not register as a superseding writer. The
  catalog's general rule is that a later canonical writer supersedes a strict
  undo.
- **Best-effort split the unit.** With the spelling set superseded but the pitch
  value not, `BestEffort` restored the pre-transpose pitch and left a spelling
  authored against the transposed one attached to it.

**Fix.** Both operations record on the shared key (`record_engraved_spellings`),
and the pitch value and its spelling set undo as one unit: if either half is
superseded, neither is restored. `StrictInverse` already refuses on any
supersession, so the coupling only bites for `BestEffort`.

**The physical separation stands, and my earlier note was only half right.** I
kept `engraved_spelling_chain` distinct from `respell_chain` because the latter
is `RespellPitch`'s LWW working state, read by its concurrent-differing conflict
detection — folding transposes in would make a concurrent respell *conflict* with
a transpose and would move the canonical bytes of every existing history. That
reasoning holds. What it did **not** license was letting one operation own the
key. Two physical chains, two responsibilities (`respell_chain`: the ledger
spelling and the LWW verdict; `engraved_spelling_chain`: the graph attachments),
but *every* writer of the attachments records on the attachment chain.

Recording is gated on `self.graph.is_some()`, so base-free reduction is
byte-unchanged and the seeded fuzz corpus's canonical-base digest does not move.

Spec: `req:opcat:spelling-set-chain`. Mutation-verified: removing the respell's
record fails all three new tests; removing the coupling fails the best-effort
one with the pitch back at C4 and the spelling still at C-sharp. Also covered:
both permutations of a concurrent respell/transpose reduce to identical bytes.

### P13-S3 follow-up: the coupling is broader than the transpose, on purpose

A review of the S3 fix caught the reducer's comment overclaiming. It said the
coupling "cannot mis-fire on an unrelated operation" because
`ModifyIdentifiedPitch` never writes the spelling set. True of the *operation*,
irrelevant to the *unit*: the coupling is keyed on the **pitch**, and on which
keys the **transaction** wrote. A transaction whose members write the two halves
separately is coupled exactly the same way.

The editor's "move note" is precisely that — `ModifyIdentifiedPitch` (the value)
plus `RespellPitch` (the spelling set), in one transaction
(`editor-core::apply_transaction`). Measured:

| later respell? | policy | undo effect | pitch | spelling |
|---|---|---|---|---|
| no | BestEffort | `Applied` | restored to C4 | attachment gone |
| yes | BestEffort | `Applied` | **stays D4** | the later E stands |
| yes | StrictInverse | `Conflicted` | stays D4 | the later E stands |

The middle row is the coupling firing on a non-transpose pair, and it is
**correct**: restoring the pitch to C4 while the engraved spelling reads E —
authored against the moved pitch — is exactly the stale-notehead defect the
coupling exists to prevent. So the breadth is intentional and now stated in
`req:opcat:spelling-set-chain` rather than left to be inferred.

Three regressions: full undo when nothing supersedes (guards against
*over*-coupling), best-effort skipping the pair when a later respell supersedes,
and strict undo conflicting. Mutation-verified: removing the coupling fails only
the middle one, restoring the pitch to C4 with the spelling still reading E.

## Push 5 / P2 — the ops decode surface (2026-07-09)

The operation layer exposes exactly two byte-decode surfaces:
`MaterializedState::decode_canonical` and `OperationKindTag::decode_canonical`.
Operation *payloads* have no decoder — `OperationKind` is encode-only — so
nothing here can yet accept a duplicate `TransposeInterval` target. When such a
decoder lands it inherits the wire table's `seq^⇑` rule: reject a duplicate,
never normalize it away.

**No defect found in the decoder.** It already carries the P1 hardening:
a whole-state re-encode-and-compare guard, and `Vec::with_capacity(n.min(1024))`
at every count site, so the unbounded-allocation and soft-DoS classes P1 fixed in
core do not apply. 2M adversarial inputs across four seeds, ~2s each.

**But the P1 design note — "the guard is complete-by-construction, it cannot miss
a lenient codec" — is only half true, and this is the finding worth keeping.**
Decode has two layers, and they catch disjoint things:

- The **whole-state guard** catches every field the decoder *normalizes*. The
  `BTreeMap`s (`objects`, `spellings`, `breaks`, `page_breaks`) re-sort and
  de-duplicate, so a non-canonical encoding of them cannot survive a round trip.
- It is **blind to order-preserving `Vec` fields**. A reordered `anomalies` or
  `pending` list re-encodes to exactly the bytes it came from, so the guard sees
  identity and accepts. Only the per-site `windows(2).all(<)` checks reject them.
  Same for a conflict record's `caused_by` / `affected_objects`, which
  `ConflictRecord::encode_canonical` writes verbatim.

Measured, not reasoned: removing both per-site `Vec` order checks leaves a 40K
input injectivity sweep **green**. An injectivity fuzzer structurally cannot see
this class — it asserts `bytes → value → bytes` identity, which is exactly what a
missing order check preserves. The per-site checks are load-bearing and were
locked by *nothing*.

(`effects` is a `Vec` with no order check, correctly: its canonical order is
*reduction* order, which a decoder cannot recompute. Two orderings are two
different states, so injectivity is not at stake.)

**Delivered.** `fuzz::run_decode_fuzz` (no-panic + injectivity over both
surfaces), returning a `DecodeFuzzCoverage` the smoke tests assert on — a decode
fuzzer that never reaches a decoder's accept path proves only the absence of a
panic. Plus deterministic tests for each layer:
`an_out_of_order_objects_map_is_rejected_by_the_whole_state_guard` (guard),
`an_out_of_order_anomaly_list_is_rejected` and
`a_reordered_pending_list_is_rejected` (per-site, invisible to the fuzzer).
Each was mutation-verified against the exact check it locks.

**Corpus depth is a property, not luck.** A fixed list of envelope-set sizes
reduces to states with no conflicts, anomalies, pending, or spellings — the very
branches that hold every canonical-order check. Measured: 6 of 12 seeds failed to
produce all four. `build_decode_corpus` now draws until covered and asserts it.
