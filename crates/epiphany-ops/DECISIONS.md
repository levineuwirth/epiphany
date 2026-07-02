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
