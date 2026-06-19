# epiphany-core — decisions and Pass 11 candidates

This file records (a) the implementation decisions the QUICKSTART asked each
agent to make once and document, and (b) the ambiguities discovered while
building `epiphany-core`, batched as **Pass 11 candidates** for the spec rather
than improvised in code (QUICKSTART, Process notes: *"Ambiguities go into a
batch, not into code … Don't open Pass 11 until you have at least three such
items batched."*).

## Implementation decisions (QUICKSTART "Decisions you'll need to make")

1. **Replica ID entropy source — `getrandom`.** `ReplicaId::generate` fills 8
   bytes from the platform CSPRNG and re-draws until the value is not the
   reserved `SYSTEM_DERIVED` namespace (Chapter 5: "MUST reject this value …
   and MUST regenerate"). `ReplicaId::from_entropy` is the deterministic,
   testable entry point. This is the spec's only sanctioned use of platform
   randomness (Appendix D §"Randomness").

2. **Event-arena storage — `slotmap::SlotMap` + `HashMap<EventId, EventKey>`.**
   The slotmap provides generation-checked stale-handle detection (matching the
   identifier-stability requirement); the hash index provides the required
   `O(1)`-amortized lookup by `EventId`. Canonical iteration
   (`iter_canonical` / `ids_canonical`) sorts by `EventId`.

3. **Chunk store backend** — N/A to this crate (Agent D).

4. **Async or sync — sync only.** No async traits anywhere.

5. **MSRV — workspace 1.77** (current stable is used in practice). No exotic
   features.

Additional local decision: **`RationalTime`'s promoted arm uses
`num-rational::BigRational`** — the spec's own reference design (Chapter 3
§"Recommended Implementation: Inline-or-Promoted"). Arithmetic takes a fast
`i128` path for the inline `Small ⊕ Small` case and promotes only on overflow,
re-establishing the "Small iff fits" canonical-form invariant after every
operation so demotion is never observable.

## Pass 11 candidates (ambiguities for the spec, not resolved in code)

### P11-1 — `TypedObjectId` discriminant values are unspecified

Chapter 5 fixes the *shape* of `TypedObjectId::canonical_bytes` ("a 16-bit
big-endian discriminant followed by the variant payload's canonical bytes") but
does **not** assign a numeric discriminant to each variant — and its variant
list ends with "`// … and so on for every named object kind`", so the set is
explicitly open. Because these bytes enter canonical state (ordering, hashing,
equality), the values are normative and must be pinned.

This crate assigns discriminants by declaration order starting at 0
(`Event = 0` … `AnalysisLayer = 21`), and — since `TypedObjectId` must name
*every* object kind the graph exposes — adds the kinds beyond the spec's
explicit list: `Tuplet = 22`, `RepeatStructure = 23`, `LyricLine = 24`,
`ChordSymbol = 25`, `View = 26`, then `Registered = 27`. The spec should adopt
or override this table and confirm the full kind set.

A sub-point: the canonical bytes of `TypedObjectId::Registered(reg, raw)` need a
defined layout for the registry id. This crate encodes it as
`discriminant(2) || reg.canonical_bytes(16) || raw_be(16)`; the spec should
confirm the registry-id encoding (and whether `ObjectKindRegistryId` is a
128-bit value, as assumed here).

### P11-2 — Graph-invariant count: spec body says 19, QUICKSTART says 18

`spec/QUICKSTART.md` (Agent B) refers to "the 18 graph invariants enumerated in
Chapter 5", but Chapter 5 §"Graph Invariants" actually enumerates **19** items
(1–19). This crate implements all 19 (see `GraphInvariant`). The discrepancy is
almost certainly a stale count in the QUICKSTART; the spec body is treated as
authoritative. Reconcile the two.

### P11-3 — `VoiceOrigin::SystemPromoted` does not carry the spec's full derivation inputs

Invariant 18 requires a system-promoted voice's `VoiceId` to equal the
deterministic derivation of Chapter 5 §"System-Promoted Voices", whose inputs
are *(staff instance, original voice, winning op, losing op)* — four ids. But
`VoiceOrigin::SystemPromoted` records only `{ cause: OperationId, original_voice:
VoiceId }` (one op, not two).

**Prototype convention (enforced):** the staff instance is recovered from the
containment walk, and `cause` is fed into *both* the winning- and losing-op
slots of `derive_promoted_voice_id`. Invariant 18 now recomputes that derivation
and rejects any `SystemPromoted` voice whose id does not match it (not merely a
wrong namespace) — see `check_voice_origin_consistent` and the
`inv18_flags_fabricated_promoted_voice_id_and_accepts_the_derivation` test.

**Open question for the spec:** either `VoiceOrigin::SystemPromoted` should carry
both the winning and losing op ids (and the spec confirm the derivation over
those), or the derivation should be defined over the inputs the origin actually
stores. The derivation function itself is also still deferred ("specified in the
semantic-operations companion document"); this crate's `derive_promoted_voice_id`
and the cause-as-both-slots convention are placeholders for that companion to
ratify.

### P11-4 — A prototype canonical encoding precedes the Binary Format companion

Appendix D and Chapter 8 defer the canonical wire encoding of graph value types
to the *Binary Format companion specification* (Agent D), which does not yet
exist. To make round-trip serialization testable now (v0 acceptance criterion
4), this crate defines a concrete canonical byte form for its primitives —
notably `RationalTime` (sign + length-prefixed big-endian numerator and
denominator magnitudes, always reduced) and the wall-clock integers
(little-endian, matching `QuantizedCoord`). These are deterministic and
reversible but provisional: when the Binary Format companion lands, reconcile
this crate's `CanonicalEncode`/`CanonicalDecode` with it (a failing cross-crate
round-trip test would be the trigger, per the QUICKSTART process notes).

### P11-5 — Scope boundary: the Chapter 4 tuning catalog is referenced, not defined here

`epiphany-core` (Agent B) owns the score graph and the pitch/time primitives. It
models the *identifiers* of pitch spaces, tuning systems, and accidental
registries (`PitchSpaceId`, `TuningSystemId`, …) and the score-level
`ScoreTuningContext`, but **not** the Chapter 4 catalog itself: the
`PitchSpace`/`TuningSystem`/`AccidentalRegistry` definitions, the normative
built-in catalog (`cmn-12`, `tet-12`, …), the hierarchical resolver, the
compatibility mappings, and the deterministic position→frequency resolution
function. The QUICKSTART's Agent B deliverable list references those by id; they
are a separate subsystem (closer to the acoustic engine, Chapter 1, which is
explicitly out of core scope). Consequences:

- `Pitch::sounding_equivalent` (the third Chapter 2 equivalence) takes a
  caller-supplied frequency resolver and handles the `AbsoluteHz` fast path; the
  other two equivalences are fully computed here. Its tolerance is the named
  `ToleranceClass::AcousticCents` (Appendix D forbids ad-hoc epsilons), not a raw
  `f64`; a wrong-class or non-finite comparison never matches.
- Tempo conversion integrates the piecewise map in closed form for
  `Constant`/`Linear`/`Exponential` segments (Chapter 3 §"Conversion"); only
  `TempoShape::Curve` is deferred (`TempoError::CurveIntegrationUnsupported`),
  per QUICKSTART. The inverse uses a deterministic continued-fraction rational
  approximation with documented bounds (`INVERSION_*`). See P11-7.

If a later phase decides the catalog belongs in `epiphany-core`, it is additive
(new modules behind the existing ids); nothing here needs to change. Recorded so
the boundary is explicit rather than a silent omission.

### P11-6 — System-derived (synthetic) pitch derivation inputs are unspecified

Chapter 5 reserves the `SYSTEM_DERIVED` replica namespace for
deterministically-derived identifiers (system-promoted voices *and*
content-derived synthetic pitches via the `MUSCSPCH` domain tag) but, as with
promoted voices (P11-3), defers the exact derivation *function* for synthetic
pitches. Invariant 11 now requires a `SYSTEM_DERIVED` embedded `PitchId` to
*prove* its namespace: its counter must equal the deterministic derivation of
its own pitch content, rather than being accepted unconditionally.

**Prototype convention (enforced):** `derive_system_pitch_id` content-addresses
the pitch from a fixed canonical byte form of its intrinsic identity (scale
position + acoustic realization; strings length-prefixed and NFC). The spec
should pin the canonical input layout (or define a different derivation).

### P11-7 — Tempo "Linear" interpolation parameter

Chapter 3 says a `Linear` segment is "linear interpolation from `start_tempo` to
`end_tempo`" without pinning *what* interpolates linearly (bpm, period, or
speed). This crate interpolates **speed** (whole notes per second) linearly,
which is beat-unit-agnostic and coincides with linear-bpm when the two tempos
share a beat unit. `Exponential` interpolates speed geometrically. The spec
should confirm the parameter (it affects the derived wall-clock schedule).

## Enforced-at-construction invariants (Chapter 3 "reject at construction")

Three Chapter-3 well-formedness rules are not in the Chapter 5 §"Graph
Invariants" enumeration but are MUSTs the spec says to "reject at construction".
This crate enforces them in the constructors (so a malformed value cannot exist),
which is both faithful and removes the need for a runtime pass:

- `TimeSignature::new` rejects beat groups that do not sum to the measure
  duration.
- `EventOrderingDAG::try_new` rejects a cyclic aleatoric ordering.
- `Tuplet` degenerate ratios (`0:n`/`n:0`) are caught by invariant 16
  (`check_invariants`), since a `Tuplet` is a plain struct.
