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

> **RATIFIED (Pass 11, 2026-06-21).** The spec-internal items below have been
> ratified into normative `core_spec.tex` text — see
> `spec/PASS11_RATIFICATION_LOG.md`. Disposition summary: P11-1/3/6 adopted as
> golden (TypedObjectId discriminants, promoted-voice + synthetic-pitch
> derivations); P11-2 fixed (count = 19; three construction-time MUSTs named, and
> `TupletRatio` now rejects degenerate ratios at construction); P11-4 adopted
> (RationalTime/scalar layouts + codec convention baseline the Binary Format
> companion inherits); P11-7 decided (tempo `Linear` interpolates speed). P11-5
> remains a scope boundary (Track C). The byte-layout golden tests now cite their
> ratified requirements.

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

**Locked (M3 follow-up).** The discriminant table and `Registered` layout are now
pinned by a golden-bytes test (`typed_object_id_byte_form_is_locked`): any
reorder, discriminant reassignment, or layout change breaks it deliberately,
since these bytes are normative (ordering/hashing/equality). The values remain
*this crate's proposal* until the spec adopts or overrides them.

### P11-2 — Graph-invariant count: spec body says 19, QUICKSTART says 18

`spec/QUICKSTART.md` (Agent B) refers to "the 18 graph invariants enumerated in
Chapter 5", but Chapter 5 §"Graph Invariants" actually enumerates **19** items
(1–19). This crate implements all 19 (see `GraphInvariant`). The discrepancy is
almost certainly a stale count in the QUICKSTART; the spec body is treated as
authoritative. Reconcile the two.

### P11-3 — resolved in M2: promoted voices retain the full derivation inputs

Invariant 18 requires a system-promoted voice's `VoiceId` to equal the
deterministic derivation of Chapter 5 §"System-Promoted Voices", whose inputs
are *(staff instance, original voice, winning op, losing op)* — four ids. But
The first-pass `VoiceOrigin::SystemPromoted` recorded only one operation id.
M2 resolves the inconsistency by storing `{ winning_operation,
losing_operation, original_voice }`; the staff instance remains recoverable from
containment.

Invariant 18 recomputes the exact derivation and rejects any
`SystemPromoted` voice whose id does not match it (not merely a wrong namespace)
— see `check_voice_origin_consistent` and the
`inv18_flags_fabricated_promoted_voice_id_and_accepts_the_derivation` test.

The core spec listing now carries both operation ids. The exact hash-domain
derivation remains provisional until the semantic-operations companion ratifies
`derive_promoted_voice_id`.

**Locked (M3 follow-up).** The 64-byte `MUSCSVCE` preimage — `staff_instance ||
original_voice || winning_op || losing_op`, each 16 big-endian bytes — and its
hash output are pinned by a golden-bytes test
(`promoted_voice_id_byte_form_is_locked`), so the layout cannot drift unnoticed;
the companion's ratification (or a different derivation) will update both the code
and that golden.

### P11-4 — A prototype canonical encoding precedes the Binary Format companion

Appendix D and Chapter 8 defer the canonical wire encoding of graph value types
to the *Binary Format companion specification* (Agent D), which does not yet
exist. To make round-trip serialization testable now (v0 acceptance criterion
4), this crate defines a concrete canonical byte form for its primitives —
notably `RationalTime` (sign + length-prefixed big-endian numerator and
denominator magnitudes, always reduced) and the wall-clock integers
(little-endian, matching `QuantizedCoord`).

**M3 follow-up — the whole-score codec (item 5).** `src/codec.rs` now composes
those primitives into a total, reversible canonical byte form for the *entire*
`Score` graph (`Score::canonical_bytes` / `Score::decode_canonical`), so the
materialized graph — not only the Chapter 6 `MaterializedState` bookkeeping —
round-trips byte-identically (Agent F's `criterion_4_full_score_byte_roundtrip`
drives it through a real bundle snapshot). The form is deliberately uniform:
little-endian integers, a single discriminant byte per tagged union, `u32`
counts/length-prefixes, every variable-width leaf length-prefixed, and raw
(non-NFC-folded) UTF-8 for free-text fields so `decode(encode(x)) == x` for every
valid score (catalog ids are already NFC at construction). The two private-field
accessors the codec needs (`EventOrderingDAG::edges_ref`,
`SpellingPrecedence::order_ref`) are `pub(crate)`.

These are deterministic and reversible but provisional: when the Binary Format
companion lands, reconcile this crate's `CanonicalEncode`/`CanonicalDecode` and
the whole-score `codec` with it (a failing cross-crate round-trip test would be
the trigger, per the QUICKSTART process notes).

**Phase 2 — Agent K (Operation Catalog): the `CanonicalValue` seam.** Track B's
Operation Catalog shifts `epiphany-ops` from identifier-only operation payloads
to *value-typed* ones (an `InsertEvent` carrying the real `Event`, a
`RespellPitch` carrying the real `PitchSpelling`). Those payloads must serialize
canonically (an envelope stays hashable across an implementation boundary), so
`epiphany-ops` needs to canonically encode/decode core value types. The internal
`Codec` trait and its `Reader` cursor stay `pub(crate)` (they are composition
machinery, not a stable surface); instead `src/codec.rs` exposes a thin **public
`CanonicalValue` trait** (`canonical_bytes` / `decode_canonical`) implemented —
via a macro that *delegates to the existing `Codec` impls* — for exactly the
value types operation payloads embed (`Event`, `Rest`, `PitchSpelling`, `Tie`,
`Slur`, `Beam`, `Spanner`, `RegionTimeModel`, `TimeAnchor`). This introduces **no
new byte layout**: a `value_codec` test asserts each value's `CanonicalValue`
bytes equal the bytes the whole-score codec already embeds for it, so all
existing goldens / criterion 4 stay byte-for-byte green. This is the **K↔J
coordination seam**: value-type wire encoding is nominally the Binary Format
companion's (Agent J) to formalize, but K needs the surface now and J inherits
core's ratified conventions (Pass 11 item 1.8, `req:format:codec-conventions`)
rather than reconciling a second codec. Rejected alternative: a parallel value
codec inside `epiphany-ops` (two sources of truth for one byte layout).

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

**Locked (M3 follow-up).** `canonical_pitch_bytes` now NFC-normalizes its string
fields *at the derivation boundary* (not merely relying on catalog ids being NFC
at construction), making the "NFC" guarantee explicit, and the `MUSCSPCH` input
layout + hash output are pinned by a golden-bytes test
(`system_pitch_id_byte_form_is_locked`). The exact field set ("intrinsic
identity") and layout are still this crate's proposal pending spec ratification.

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
- `TupletRatio::new` rejects degenerate ratios (either term zero, or
  `actual == notated`); its fields are private, so a degenerate `TupletRatio`
  is never representable, and codec decode re-validates through the same
  constructor. (Pass 11 item 3.5 moved this from a runtime invariant-16
  sub-check to a construction-time MUST.)

## Phase 2 — Agent H (spelling + decomposition pre-passes)

The two sanctioned v0 stubs (`pitch::spell` returning a trivial middle-C; no
decomposition algorithm) are now real, in `src/prepass.rs`. `spell` now takes
the full `&Pitch` (was `AcousticPitch` by value): spelling needs the scale
position, which `AcousticPitch` does not carry, so the old signature could not
have done real work — a breaking but necessary change (the only callers were
in-crate). The pre-passes are
**canonical derived annotations** (PHASE2_QUICKSTART §H): pure functions of
`(materialized Score, profile, SpellingAlgorithmId, DecompositionAlgorithmId)`,
recomputed on materialization, never stored. They do **not** enter the canonical
`Score` bytes — there is deliberately no codec for `DerivedAnnotations` — so the
Chapter-6 reducer and criteria 4/5 are untouched (the conformance suite stays
green). `derive_annotations(&Score, &PrePassProfile)` is the entry point a
materializer (F's integration harness) calls after reduction completes.

### Phase-2 decisions made (the five the dispatch asked H to make once)

1. **Spelling algorithm — Temperley line-of-fifths, registered as
   `SpellingAlgorithmId::default_id()` (`"default"`).** A per-voice
   centre-of-gravity preference rule over the line of fifths (each note picks the
   tonal-pitch-class spelling closest to a running window of recent spellings,
   broken by accidental simplicity, then melodic direction, then a total order on
   the lof value). It is deterministic, key-free (infers tonal context from the
   melody itself), and spells diatonic music in sharp/flat keys correctly
   (verified against C/D-major and B♭-major scales and sharp/flat contexts in the
   `prepass::tests` suite). Authored CMN scale positions are **preserved, not
   re-spelled** (an authored C♯ stays C♯); the algorithm only *decides* spelling
   for integer/chromatic (12-EDO) input, which is where spelling is genuinely
   undetermined. Chosen over Longuet-Higgins line-of-fifths because it is the
   best-documented and has the cleanest deterministic constraint formulation
   (PHASE2_QUICKSTART recommendation). **Awaits G ratification in Pass 12** (Pass
   11 is closed), per the dispatch's "or Pass 12 if the call slips."

2. **Decomposition algorithm — metric greedy-aligned splitting, registered as
   `DecompositionAlgorithmId::default_id()` (`"default"`).** All grid logic is
   integer arithmetic over a `1/4096`-of-a-whole-note grid, so every note value
   to a (single-)dotted sixty-fourth is exact and the derivation is deterministic.
   A duration is split at barlines (with ties), then within a measure each span
   is emitted as the single notated value it equals **unless** it would cross a
   dyadic boundary at least as strong as the one it starts on (the
   beat-clarity/syncopation rule), in which case it splits at the strongest
   interior boundary and ties across. Tuplet members convert sounding→notated in
   the exact rational domain *before* gridding (a triplet eighth's sounding `1/12`
   is non-dyadic; its notated `1/8` is), then decompose and carry tuplet
   membership. Components' sounding durations sum to the event's (invariant 15).

   The remaining three Phase-2 decisions (solver architecture, renderer SVG
   dialect, catalog/companion versioning) belong to Agents I/K/J, not H.

### Eligibility taxonomy

`derive_annotations` classifies and **counts** every event and embedded pitch
into explicit buckets (`TaxonomyReport`), so "ineligible" is never silently
absent (PHASE2_QUICKSTART §H): pitched events → spelling per pitch + decomposition
(if metric, determinate musical duration); rests/unpitched → decomposition, no
pitch spelling; trajectory pitches are spelled but the event is not decomposed;
graphic/indeterminate/cue → neither; non-`cmn-12`-determinable pitch spaces →
`spelling_unavailable`; proportional/aleatoric regions →
`decomposition_deferred_nonmetric`.

### Precedence rule (H formalizes the rule, not the model)

`resolve_spelling` layers authored overrides above the inferred default: an
engraved-layer, pitch-scoped, `Explicit` `SpellingAttachment` whose
`SpellingSource` kind outranks `Inferred` in the score's `SpellingPrecedence`
wins; otherwise the algorithm's spelling stands. This is the precedence a
`RespellPitch` override rides on. **Coordination with K:** the v0 `RespellPitchOp`
carries only a `ContentHash` *fingerprint* of the new spelling (P11-C1), so an
override's spelling *value* is not reconstructable from a v0 op alone; H's rule
operates on the authored `SpellingAttachment`s present on the materialized
`Score` (set by imports/analysis today, by K's real value-typed payloads in
Phase 2). The rule itself is value-independent and final.

### Pass 12 candidates (batched for F's Pass 12 tracker; ≥3, so the batch opens)

- **P12-H1 — Ratify `SpellingAlgorithmId::Default` = Temperley line-of-fifths
  v1.** The algorithm choice is H's call to propose and G's to ratify; Pass 11
  closed before H landed, so this is the first Pass 12 item. Until ratified the id
  `"default"` is this crate's proposal (it is *not* a byte-layout, so nothing
  golden-locks on it).
- **P12-H2 — `KeySignatureChange` / `ClefChange` are anchor-only placeholders**
  (Chapter 7 detail deferred). Context-aware spelling therefore infers tonal
  context from the melody (line-of-fifths centre of gravity) rather than a
  *declared* key. A real key-signature/clef content model would let spelling and
  decomposition honour declared keys/clefs and place natural signs to cancel a
  key; flagged as a graph-model gap, not improvised here.
- **P12-H3 — Chromatic-run convention** (ascending = sharps, descending = flats)
  is only a *tiebreak* in the centre-of-gravity rule, so an isolated chromatic run
  with no tonal context may pick the enharmonic the convention would not. A
  voice-leading refinement is a Pass-12 candidate (the dispatch sanctions deferring
  hard chromatic cases).
- **P12-H4 — Decomposition simplifications:** single governing meter per region
  (multi-meter / mid-region meter changes deferred); region origin assumed to be a
  barline (anacrusis/pickup deferred); compound-meter (6/8…) beat-group grouping
  beyond the dyadic default; tuplet nesting and cross-beat tuplet members; double
  (and higher) augmentation dots — `MAX_DOTS = 1` for v1, so a double-dotted value
  is written as tied single/dotted values (correct, if not the most compact).
- **P12-H5 — Automatic spelling under aleatoric regions** (the spec's open
  question). H spells pitches region-independently (pitch identity does not depend
  on the time model) but performs no region-specific aleatoric spelling; defer if
  the algorithm does not generalise cleanly.
