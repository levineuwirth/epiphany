# epiphany-core

The Epiphany **score graph**: the in-memory representation of all musical
content in a score, implementing the normative requirements of **Chapters 2–5**
of the core specification (`spec/core_spec.pdf`). This is Agent B's crate per
`spec/QUICKSTART.md` — the largest scope — building only on Agent A's
`epiphany-determinism`.

> The graph is the canonical truth about the music; layout, serialization, and
> editing operations are downstream projections and consumers of it.
> — Chapter 5, Design Principles

## What's here

| Area | Items | Spec |
|------|-------|------|
| Identifiers | `ReplicaId` (+ `SYSTEM_DERIVED`), `OperationId`, the typed 128-bit family (`EventId`, `PitchId`, `VoiceId`, …), `TypedObjectId`, `IdentityContext`, `derive_system_id` | Ch. 5 §"Identifiers"; Ch. 6 §"Operation Identity" |
| Time | `RationalTime` (inline-or-promoted), `MusicalPosition`/`MusicalDuration` (typed algebra), `WallClockTime`/`WallClockDuration`, `TimeAnchor`/`AnchorOffset`, `EventPosition`/`EventDuration`/`ConcreteDuration`, `TimeSignature`/`BeatGroup`, `NotatedComponent`/`NoteValue` | Ch. 3 |
| Tempo | `TempoMap`, `Tempo`, `TempoSegment`, `TempoShape` with closed-form `musical_to_wallclock`/`wallclock_to_musical` over constant/linear/exponential segments (curve deferred → `TempoError`) | Ch. 3 §"Tempo Map" |
| Pitch | `Pitch`, `ScalePosition`, `IdentifiedPitch`, `PitchSpelling`, the spelling-attachment subsystem, `ReferencePitch`, `spell` stub, all three equivalences (`scale_position_equivalent`, `enharmonic_equivalent`, `sounding_equivalent`) | Ch. 2; Ch. 4 registry ids |
| Events | the `Event` taxonomy (7 variants) and the `slotmap`-backed `EventArena` | Ch. 5 §"The Event Arena" |
| Graph | `Canvas`, `Region`, `Staff` vs `StaffInstance`, `Voice`/`VoiceOrigin`, `Measure`, `BarlineAlignmentGroup`, aleatoric `EventOrderingDAG` (acyclic by construction), the full cross-cutting registry, the full top-level `Score` | Ch. 5 |
| Indexes | `ScoreIndexes`: the four mandatory indexes (event-time, cross-cutting-reference, measure, spelling-attachment) | Ch. 5 §"Indexes" |
| Invariants | `check_invariants` over all 19 enumerated graph invariants, with a typed `InvariantViolation` witness per check | Ch. 5 §"Graph Invariants" |
| Generators | `generators::valid_score`/`valid_score_rich` (positive), `violating_score` (negative, per invariant), `shrink` (witness minimizer) | QUICKSTART, Agent B hand-off |

## The identity discipline this crate enforces

1. **Replica + counter, big-endian canonical bytes.** Every typed identifier is
   `(replica << 64) | counter`; its canonical 16-byte form is `to_be_bytes()`
   (8-byte replica, 8-byte counter) and the numeric `Ord` *is* the
   Appendix-D lexicographic byte order. Identity is exact, never tolerant.
2. **A reserved system namespace.** `ReplicaId::SYSTEM_DERIVED` is rejected by
   `ReplicaId::generate`/`from_entropy`; system-derived ids (`derive_system_id`,
   `derive_promoted_voice_id`) live only in that namespace, with counters
   `trunc64(BLAKE3(domain_tag || canonical_inputs))` via
   `epiphany-determinism`.
3. **Cross-kind confusion is a compile error.** Each object kind has its own
   newtype; `TypedObjectId` tags them apart with a discriminant that is part of
   canonical content.

## Graph invariants as property tests

The Chapter 5 invariants are **property tests in CI, not runtime assertions in
release builds** (QUICKSTART). `check_invariants` returns every violation with a
small witness. For each invariant `generators` provides:

- a **positive generator** (`valid_score` / `arbitrary_graph_corpus`) whose
  output passes every check, and
- a **negative generator** (`violating_score`) plus a **shrinker** (`shrink`)
  that minimizes a violating graph to a small witness while retaining only the
  structure the violation needs.

Generation is deterministic (a vendored SplitMix64), so a failing case
reproduces from its seed — no platform entropy enters generation (Appendix D
§"Randomness").

## Implementation decisions

Per QUICKSTART "Decisions you'll need to make" (full rationale in `DECISIONS.md`):

- **Replica entropy: `getrandom`** (decision 1). `ReplicaId::generate` re-draws
  until the value is not the reserved namespace.
- **Event-arena storage: `slotmap`** (decision 2) plus a hash index for the
  required `O(1)` `EventId` lookup and generation-checked stale handles.
- **Sync only** (decision 4): no async anywhere.
- **Current stable Rust** (decision 5); MSRV pinned at the workspace's 1.77.
- `RationalTime`'s promoted arm uses `num-rational`'s `BigRational`, the spec's
  reference design (Ch. 3 §"Recommended Implementation").
- `unsafe` is forbidden crate-wide (`#![forbid(unsafe_code)]`).

## Hand-off criteria (QUICKSTART, Agent B)

- [x] Every invariant has both a generator and a shrinker
      (`generators::{valid_score, violating_score, shrink}`; one per invariant,
      property-tested), plus targeted tests for the cross-cutting/anchor/tie
      sub-rules.
- [x] The arbitrary-graph corpus runs clean
      (`generators::tests::positive_corpus_runs_clean`, 500 graphs), and a
      breadth corpus (`valid_score_rich`: concurrent metric/proportional/
      aleatoric regions, measures, triplet, tie, spanner, marker, chord symbol,
      decomposition, tombstones) runs clean over 200 seeds.
- [x] `cargo test -p epiphany-core` clean (69 unit + 5 integration).
- [x] `cargo clippy --all-targets -- -D warnings` clean; `cargo doc` clean under
      `RUSTDOCFLAGS="-D warnings"`.

### Depth of the invariant checks

The checks are not surface-level. In particular: invariant 3 computes per-clock
event intervals and detects both disorder and overlap; invariant 7 resolves
region extents to absolute wall-clock coordinates (wall-clock leaves, plus
event/region/measure-start anchors — an event anchor is its region origin plus
its region-relative position) and only skips pairs that can't be placed without
the deferred tempo map; invariant 9 sweeps *every* reachable anchor (region
extents, meter changes, measure starts, clef/key changes, user breaks, spanners,
spelling ranges); invariant 10 resolves *all* graph references — cross-cutting
anchor targets, annotation layers, tuplet parents, graphic objects, and
event-internal references (indeterminate alternatives, trajectory event-pitches,
graphic-event objects, cue sources); invariant 11 covers every id kind, plus
tombstone/live collisions, `SYSTEM_DERIVED` misuse (including the score's own
identity context), and arena index/well-formedness integrity (catching
post-`get_mut` corruption); invariant 17 validates explicit *and* implicit
(pitch-id-ascending) tie pairings, per-class adjacency, and the cross-voice
position rule; invariant 18 recomputes the deterministic promoted-voice
derivation. Enharmonic equivalence is a sounding notion (octave matters:
C4 ≠ C5). Empty pitched events are rejected at the arena boundary and re-checked;
`IdentityContext::try_new` rejects the reserved replica and counters use
`checked_add` so a counter is never silently reused.

### Known bounded limitations (deferred dependencies)

- **Tempo conversion** integrates the piecewise map in closed form for
  `Constant`/`Linear`/`Exponential` segments (Chapter 3 §"Conversion"); only
  `TempoShape::Curve` is deferred to the open numerical algorithm
  (`TempoError::CurveIntegrationUnsupported`). Segment boundaries that cannot be
  placed without the score graph, and malformed segment sequences, return a
  `TempoError`, never a wrong answer. The inverse round-trips ordinary rhythms
  via a documented continued-fraction approximation (DECISIONS P11-7).
- **Region time-overlap (invariant 7)** resolves extents to absolute wall-clock,
  now including musical event positions placed through the region's effective
  tempo map (its `local_tempo_map`, else the score map). Extents that still
  cannot be placed (no tempo defined, or a deferred curve) are skipped rather
  than rejected. Sound (no false positives), incomplete (DECISIONS P11-4).
- **System-promoted voice derivation (invariant 18)** retains the winning and
  losing operation ids on `VoiceOrigin::SystemPromoted`; the checker recomputes
  the exact four-input derivation used by `epiphany-ops`.
- **The Chapter 4 tuning *catalog*** — `PitchSpace`/`TuningSystem`/
  `AccidentalRegistry` *definitions*, the built-in catalog, the hierarchical
  resolver, and the position→frequency resolution function — is **not** an
  Agent B deliverable (the QUICKSTART lists those as referenced-by-id). This
  crate models the identifiers and the score-level `ScoreTuningContext`;
  `Pitch::sounding_equivalent` takes a caller-supplied frequency resolver. See
  DECISIONS P11-5.

## Scope boundaries

The full Chapter 5 top-level `Score` shape is modeled (metadata, instruments,
staff groups, parts, tuning context, tempo map, analysis layers, views) along
with the complete `CrossCuttingRegistry` (slurs, ties, beams, tuplets, spanners,
markers, repeats, analytical annotations, comments, graphic gestures, lyrics,
chord symbols). The reference- and identity-bearing fields are modeled in depth;
deeper *bodies* (tuning resolution, tempo-curve integration, part layout, view
recipes, glyph/engraving detail) are Chapters 3/4/7 and later companions.

Engraving-display detail (`StemConfiguration`, `ClefChange`,
`KeySignatureChange`, articulations, dynamics, line styles, spanner/marker
visual kinds) is *introduced informally here and fully defined in Chapter 7* —
it belongs to Agent E (`epiphany-layout-ir`), and this crate carries minimal,
clearly-marked placeholders for it. Operation envelopes, stamps (HLC), causal
contexts, the canonical reduction, tombstone *tracking*, and conflict records
are Chapter 6 / Agent C (`epiphany-ops`); `epiphany-core` defines only the
`OperationId` they hang off and the tombstone-aware invariants.

Ambiguities discovered while building are **not** resolved in code — they are
batched as Pass 11 candidates in `DECISIONS.md` (QUICKSTART, Process notes).
