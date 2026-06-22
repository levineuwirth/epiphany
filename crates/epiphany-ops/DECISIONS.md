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

## Pass 11 candidates (ambiguities for the spec, not resolved in code)

### P11-C1 — operation payload schemas are deferred; we carry identifiers + fingerprints

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

## Provisional canonical encoding (mirrors Agent B's P11-4)

The composite Chapter 6 types use a concrete, reversible canonical byte form
(little-endian integers; `u32` length prefixes on every variable-length part;
NFC + length-prefixed text; single-byte discriminants) so that envelope hashing,
conflict-id derivation, and the materialized-state bytes are testable now. This
is deterministic and unambiguous but **provisional**: when the Binary Format
companion lands, reconcile `encode.rs` and the per-type `CanonicalEncode` impls
with it. A failing cross-crate round-trip test is the trigger.
