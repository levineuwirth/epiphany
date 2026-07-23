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

> **Ratified (2026-07-02):** the Binary Format companion now exists
> (`spec/binary_format.tex`, v0.1.0). Its Chapter 5 ratifies this crate's
> whole-`Score` positional codec, the convention macros, every discriminant
> table, and the `CanonicalValue` seam as the schema-major-0 wire form, with
> the frozen-layout rule (a field-set change is a schema-major change). No
> reconciliation was needed: the companion was transcribed from this codec and
> its golden anchors, so the trigger never fired.

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

## Audit follow-up (2026-07-01): decomposition precedence + typed inversion tolerance

### Authored decomposition attachments outrank the pre-pass (`resolve_decomposition`)

Audit finding: `infer_decompositions` never consulted
`Score.decomposition_attachments`, so an authored decomposition was silently
shadowed by the derived one — violating Chapter 3 §"Sounding Duration and
Notational Decomposition": the pre-pass "produces inferred decompositions for
events that **lack a higher-precedence attachment**", with the "same sources,
same precedence machinery, same pre-pass discipline" as spelling.

Fixed by `resolve_decomposition`, the decomposition analogue of
`resolve_spelling`: for each event the pre-pass inferred a decomposition for,
an authored `DecompositionAttachment` targeting that event whose source
**outranks `Inferred`** replaces the derived one in
`DerivedAnnotations.decompositions` (the effective attachment keeps its
authored source, so provenance is visible and enters the derivation
fingerprint). Precedence is the spec's default source order — `UserChosen >
Imported > Propagated > Inferred` — as a fixed rank, because the graph model
carries **no** `DecompositionPrecedence` configuration and the attachment has
no `priority`/`layer` axes (unlike `SpellingAttachment`); among competing
authored attachments the lowest rank wins, and a full rank tie keeps the first
in the score's canonical (codec-fixed) `decomposition_attachments` order, so
resolution is deterministic across replicas.

**Taxonomy decision:** authored-override events are counted **distinctly** in a
new `TaxonomyReport::decompositions_authored` bucket (mirroring
`spellings_authored`); `decompositions_inferred` now counts only events whose
*effective* decomposition is the pre-pass's own. The effective map size equals
`decompositions_inferred + decompositions_authored` (the H harness's accounting
check was updated accordingly). The new bucket is serialized in
`DerivedAnnotations::canonical_fingerprint` with the other counts.

**Scope, mirroring spelling:** resolution layers overrides above the pre-pass's
*inferred* output only. An authored attachment on an event the pre-pass emits
nothing for (ungriddable / non-metric / inapplicable kind) does not surface as
a derived annotation — exactly as a spelling attachment on a
spelling-unavailable pitch does not (the attachment still lives in canonical
`Score` state either way). Two genuine ambiguities are batched, not improvised:

- **P12-H6 — Decomposition precedence configurability.** Chapter 3 says "same
  precedence machinery" as spelling, and Chapter 2 makes the spelling order
  *configurable* per score (`SpellingPrecedence`, plus `priority`/timestamp
  tie-breaks); but the graph model has no `DecompositionPrecedence` field and
  `DecompositionAttachment` has no `priority`. Whether decomposition precedence
  should be configurable (a new canonical `Score` field — a codec/ratification
  change), share `SpellingPrecedence`, or stay the fixed spec default needs a
  spec disposition. Until then the fixed default order is implemented.
- **P12-H7 — Authored decompositions for events the pre-pass cannot infer
  for.** An authored attachment is precisely how a user would notate an event
  the algorithm reports ungriddable, yet the derived-annotation surface only
  resolves overrides where an inferred output exists (the spelling mirror).
  Whether authored attachments should surface in `DerivedAnnotations` for
  inference-ineligible events (and how the taxonomy should count them) is a
  spec question for both pre-passes.

### Typed inversion tolerance (`tempo::inversion_tolerance`)

`INVERSION_TOLERANCE_WHOLE_NOTES` was a bare public `f64` documented as
belonging to tolerance class `TempoIntegration` but never constructed as a
`Tolerance` (Appendix D §"Tolerance Classes": no ad-hoc epsilons). It is now
the private raw magnitude behind the public `inversion_tolerance()` — a
`Tolerance { class: TempoIntegration, absolute: 1e-6, relative: None,
governance: Validation }` (the same construction pattern as the existing
`speed_degeneracy_tolerance`). Behavior is numerically identical: the inverse
conversion passes `inversion_tolerance().absolute.get()` (exactly `1e-6`) to
the continued-fraction approximation. Note the class's *non-normative* unit
label is "wallclock seconds" while this residual is measured in whole notes;
the class identity (`TempoIntegration`: conversion residual in either
direction) is what is normative.

## Pass 12 G-pass (2026-07-07): the H rows are ratified

All seven H rows are retired (dispositions in
`spec/PASS12_RATIFICATION_LOG.md`, "G-pass tranche"; worklist
`spec/PASS12_WORKLIST.md`). Summary: **H1** `"default"` = Temperley
line-of-fifths v1 is ratified normative (`req:pitch:spelling-algorithm`);
**H3** convention-as-tiebreak and **H5** region-time-model-independence are
pinned as properties of that versioned algorithm; **H4** the five
decomposition bounds are the declared normative bounds of `"default"` v1
(`req:time:decomposition-algorithm`; C5's derived-notation gap is subsumed);
**H6** decomposition precedence is ratified FIXED (not configurable — a
configurable order would be a schema-major `Score` field with no consumer);
**H2** narrowed (the content model landed in I-0; key-aware spelling is
algorithm-v2 territory, cancelling naturals a notation refinement); **H7**
decided the other way from the implementation: authored attachments on
inference-ineligible targets MUST surface in derived annotations
(`req:pitch:authored-uninferred`) — the code change lands with the G-pass
code tranche (authored-only resolution paths + distinct taxonomy buckets in
both pre-passes). Also ratified here: system-derived intrinsic content is
immutable under reduction (P12-K3; core Ch5 states it, the catalog pins the
precondition, `epiphany-ops` implements).

### G-pass follow-up (2026-07-07): unsupported algorithm ids now ERROR

A post-commit review found the ratified requirement and the implementation
disagreeing: `req:pitch:spelling-algorithm` / `req:time:decomposition-algorithm`
say a profile requesting an unregistered id MUST **error**, but
`derive_annotations` still implemented the pre-ratification behavior (that
pre-pass "derives nothing" while the requested id stays in the result profile
— the honest-cache-key rationale), and a test locked it. The MUST-error
contract is the right one and stands: a silently-empty derivation is
indistinguishable from a legitimately empty score, and an implementation that
*does* support the requested algorithm would produce real annotations while
this one quietly produced none — two "successful" results that disagree.
Fixed: `derive_annotations` returns `Result<DerivedAnnotations, PrePassError>`
(`UnsupportedSpellingAlgorithm` / `UnsupportedDecompositionAlgorithm`,
rejected up front); every production caller uses the default profile and
`.expect`s; the stale test is rewritten as `unknown_algorithm_ids_error`.
CONFORMANCE.md's long-standing "errors" claim is now true rather than
aspirational.

## Schema major 2, Phase B: the snapshot side (data-model fills + frozen v1)

The nine type bodies fill to the ratified Ch5 shapes (Binary Format §Schema
Major 2): Slur/Tie/Beam/Spanner (kind/curvature/sub-beams/geometry/style —
one shared `SpanStyle`), RepeatStructure (kind/voltas), Staff (default_clef),
StaffLineConfiguration (spacing/style/bracket), Instrument (six fields),
ScoreMetadata (six fields incl. the strictly-authored timestamps). All new
leaf types live in `graph.rs` with the ratified wire discriminants in
`codec.rs` (`tag_only_codec!` for the tag-only enums).

**The frozen-form architecture generalized:** major 2's fills reach types the
v0 walk had treated as "unchanged" (`metadata`, `staves`, `cross_cutting`,
and — transitively through `Region.content` — the staff instances), so the
frozen wire forms are now a *shared sub-codec layer*: `enc_/dec_*_v1`
functions (v0 == v1 for every type major 2 changed) used by BOTH
`decode_v1_score`/`encode_v1_score` (new) and the v0 pair (updated to route
through them). Each versioned decoder stays strictly canonical over its own
wire form (re-encode-and-compare, the fuzzer-P1 discipline) and the fuzzer
corpus gained genuine-v1 forms + the major-2 seam. The
`v1_score_migrates_default_filling_the_major_2_fields` size anchor pins that
v1 omits exactly the appended default bytes (so the frozen encoder cannot
silently drift), and
`current_major_round_trips_non_default_values_for_every_major_2_field`
exercises every new field (and every payload-carrying SpannerKind variant)
as real wire content.

**Deliberate scope choices:** generator fixtures were NOT given non-default
v2 content — that would churn the render goldens and reference-suite metrics
for zero coverage the codec tests don't already provide; ops-level coverage
arrives when Phase D's valuegen builders emit v2 values. `Score::empty` keeps
its signature (Timestamp(0) is the ratified unset convention; a
creation-time builder waits for a producer, e.g. import).

## Schema major 2, Phase D — repeat coverage lands; the anchor-site walk gets one home

The Phase-B promise above ("ops-level coverage arrives when Phase D's
valuegen builders emit v2 values") is discharged: `epiphany-ops::valuegen`
gained `event_anchor`/`repeat_structure`/`volta_repeat`, the op generators
emit the pair, and the decode fuzzer's corpus gains
`valid_score_rich_with_repeats` (DalSegno + voltas) — **corpus-local**, not
in the shared `valid_score_rich`: the shared fixture feeds the render
goldens, and repeat rendering is deliberately E1's churn, not D's (the
zero-golden-churn discipline).

`RepeatStructure::anchor_sites()`/`anchor_sites_mut()` (graph.rs) are now
THE site-set walk (start/end, kind jump targets, volta spans). Review found
the set hand-rolled in five places across three crates — and a sixth,
`indexes.rs`, silently stale since Phase B (it indexed only start/end,
missing every kind/volta anchor, contradicting its own doc). All flat walks
now consume the method (the classified per-site invariant check keeps its
exhaustive match for message attribution); the index gap is
regression-locked in `indexes_build_and_answer_queries`.

## The interval algebra, and the type that already existed (Push 4a, 2026-07-09)

`req:pitch:transposition` (core Ch2, §"Transposition and the Interval Type")
pins the action of a `TranspositionInterval { d, c }` on a
`PitchSpacePosition::Cmn`. With `n` the nominal's normative discriminant and
`s = nominal.chromatic() + alteration + 12*octave`:

    nominal'    = CmnNominal((n + d).rem_euclid(7))
    octave'     = octave + (n + d).div_euclid(7)
    alteration' = (s + c) - (nominal'.chromatic() + 12*octave')

The diatonic component alone picks the nominal and octave; the alteration
absorbs the residue. C4 + (7, 12) = C5, not "C with twelve sharps". C4 + (0, 1)
= C#4, so the editor's sharpen keeps its exact current behaviour.

**The type was already here.** `TranspositionInterval` has lived in `graph.rs`
since schema major 2, carrying `Instrument.transposition` (a B-flat clarinet is
`-1` diatonic, `-2` chromatic), already codec'd, already exported. It is
byte-for-byte the pair transposition needs, so Push 4a reuses it rather than
minting an `Interval` beside it. The spec now lists it once, in Chapter 2; the
Chapter 5 `Instrument` block references that listing instead of repeating it.
Two normative listings of one struct is the drift the P13-I1 fix just closed.

Its doc claimed it was "ADVISORY until the Chapter 4 tuning catalog pins
interval algebra". That was the P12-K2 false coupling, repeated. Transposition
acts on `scale_position`; tuning acts on `acoustic`; the two never touch. The
algebra is pinned here with no tuning catalog anywhere in sight. What remains
genuinely unimplemented is the *automatic application* of an instrument's
interval at the written/sounding boundary — nothing respells a written part
into a sounding one — so `Instrument.transposition` stays advisory, for that
reason and not the stated one.

**Refusal, not saturation, and never a panic.** All arithmetic widens to `i64`
first. `i32` is not wide enough to hold the intermediates of an `i32` interval:
the first version of `transposed` panicked on `diatonic_steps = i32::MAX` at
`12 * new_octave`, and `inverse()` panicked on `i32::MIN`. Refusing is the
contract; panicking on a value the public type admits is not. (Under
`overflow-checks = false` — any downstream release build — those expressions
wrapped instead. A 10.5M-case sweep of wrapping-vs-exact found *no* input where
wrapping produced a wrong `Ok` rather than a refusal, so the defect was a panic,
not silent corruption. `inverse()` now returns `Option`, because an interval
whose inverse is not representable is a fact about the type.)

`alteration` and `octave` are `i8`. A transposition whose result does not fit
refuses; so does one against a non-`Cmn` position (no nominal to move), a
`Cmn` position whose enclosing chromatic structure cannot be established, or
an `AcousticRealization::AbsoluteHz` pitch (which overrides the tuning system,
so moving the scale position moves the notehead without moving the sound).
Saturation or guessed pitch-space arithmetic is the worst possible failure
because it is invisible: it produces a pitch nobody asked for, reports
success, and destroys the evidence. See `epiphany-ops/DECISIONS.md` §"Push 4a"
for the operation-level consequences and the frozen `Transpose`.

## Push 4b (the Chapter 4 tuning catalog) — remaining implementation blockers

Push 4a proved that built-in `cmn-12` transposition needs no tuning catalog.
P13-S2 subsequently made the general algebra space-relative: interpreting a
`Cmn` alteration requires the enclosing space's chromatic cardinality and
nominal map. The specification contradiction is resolved, but the registry
implementation is now an explicit part of Push 4b. Remaining blockers:

- **Resolve pitch-space structure, then remove the interim name gate.** Until
  the registry exists, `Pitch::transposed` and the `twelve_tet_*` helpers fail
  closed for `Cmn` positions outside built-in `cmn-12`. Push 4b must resolve
  `PitchSpaceId` to `PositionStructure::DiatonicOverChromatic`, use its
  `chromatic_positions_per_octave` and `nominal_to_chromatic` mapping, and
  replace identifier recognition rather than preserving it as policy
  (P13-S2; `req:pitch:alteration-unit`,
  `req:pitch:space-capability-refusal`).
- **The core stores only the default space, tuning, and reference.** The
  overrides, accidental extensions, and SMuFL target Chapter 4 requires are
  absent, and none of the catalog/resolver types exist
  (`ScoreTuningContext` in `graph.rs` is the whole surface today).

P13-S1 removed the former requirement-label blocker; Chapter 4's requirements
are now independently citable.

The two remaining Push-4a audit claims, carried here as **unverified** through
two passes, were checked on 2026-07-22. **Both are real**, and both are Chapter 4
defects standing in front of 4b rather than inside it. Filed as P13-S5 and
P13-S6:

- **The JI prime basis is specified at two lengths** (P13-S5).
  `req:pitch:ji-vector-basis` says the built-in JI spaces order primes ascending
  *starting with 2* and that `components.len()` must equal the basis size; the
  built-in table calls `ji-5limit` "Two-dimensional (prime axes 3, 5)",
  `ji-7limit` three-dimensional, `ji-11limit` four-dimensional — each exactly
  one short, consistently, the table being octave-reduced and the requirement
  full-register. Both are normative, so a 5-limit vector must be both length 2
  and length 3. The requirement's octave-reduction clause normalizes the first
  component; it does not remove it.
- **No built-in tuning system's resolution is pinned** (P13-S6), and 14 of the
  20 have no definition at all — only the six `tet-*` are specified, by
  `EqualTemperament`'s structural rule. `TuningResolution::Function` delegates
  the historical temperaments to a `TuningFunctionId` that Chapter 10 lists as
  an *extension point*; no built-in is mapped to one and none is pinned. Against
  `req:tuning:tuning-resolution-determinism`, which requires determinism across
  platforms, two conforming implementations may choose different published
  variants of Werckmeister III and both pass.

The original phrasing understated the second: it is not missing ratio data but
an unpinned resolution contract, and `req:pitch:spelling-algorithm`'s versioned
`SpellingAlgorithmId "default"` is the in-house pattern for fixing it.

## The Text Projection value layer (`textvalue*.rs`)

The Chapter-5 half of the Text Projection companion: `project` and `parse` for
every value an operation payload can embed.

**One field list drives both forms.** `struct_codec!`, `unit_codec!`,
`cstyle_enum_codec!` and `catalog_id_codec!` now emit a `TextValue` impl beside
the `Codec` impl, from the *same* invocation — 116 types whose field order cannot
disagree between the binary form and the text, at zero call-site churn. This is
the companion's own rationale applied to code: *a rule cannot drift from the
listing it reads*, and two listings of one struct is the drift this project has
already been bitten by (P13-I1). The `struct_codec!` expansion rebuilds through a
struct literal and `cstyle_enum_codec!` matches exhaustively, so a field or
variant added later **fails to compile** rather than silently vanishing from the
text.

The remaining 44 types have hand-written `Codec` impls and so need hand-written
projections. Their field order was verified by a **mechanical diff** of the
identifier sequence in each `fn enc` against the one in each `project`; all 44
agree. Six apparent mismatches were regex artifacts — single-field variants whose
binding is named differently on each side, and `.iter().map(…)` forms the pattern
missed — each checked by hand.

**Strictness is per-site, and the whole-value layer turned out to be dead.**
The binary decoders enforce `req:binfmt`-style canonicality in two layers: a
re-encode-and-compare guard, plus per-site checks for the order-preserving fields
that guard is blind to. The text layer was built the same way, and then
mutation-tested. The result:

| check | verdict |
|---|---|
| set / map strictly-increasing walk | **live** |
| `RationalTime` lowest-terms compare before construction | **live** |
| catalog-id NFC intern-and-compare | **live** |
| `EventArena` ascending-`EventId` walk | **live** |
| `ensure_canonical` on `Tempo` | dead — removed |
| `ensure_canonical` on `ReferencePitch` | dead — removed |
| `ensure_canonical` on `SpellingPrecedence` | dead — removed |
| `ensure_canonical` on `EventOrderingDAG` | dead — removed |

A whole-value guard can only fire when a parse **normalizes**. Every Chapter-5
constructor that normalizes (`RationalTime::new` reduces, `X::new` folds to NFC,
`EventArena::insert` re-sorts, `BTreeSet`/`BTreeMap` re-sort) needed a check that
*names the fault* anyway. The four constructors left — `Tempo::new`,
`ReferencePitch::new`, `SpellingPrecedence::new`, `EventOrderingDAG::try_new` —
**reject rather than adjust**, so an accepted value re-projects to exactly its
input and the guard could never fire. A probe confirmed `try_new` returns its
input map unchanged. The helper and all four call sites were removed: *a check
that cannot fail is worse than no check, because it invites weakening the real
one* — the same finding as the reader's two diagnostic-only branches.

**What no round-trip test can see.** Two blind spots, both closed elsewhere:

1. *Field order.* A `project`/`parse` pair that agrees with itself on a wrong
   order round-trips perfectly, and two adjacent same-typed fields swapped in both
   directions are invisible to the compiler too. Closed by construction for the
   116 macro types and by the mechanical diff for the 44 hand-written ones.
2. *Constructor names.* `Sexp::sym("measured-fracton")` round-trips, because
   `parse` reads back the same wrong symbol `project` wrote. Closed by
   `tests/textvalue_names.rs`, which recovers each type's Rust name from its
   derived `Debug` and compares it to the symbol actually emitted.

**One method error worth recording.** The work list came from `cargo check`
errors, but the compiler reports only the *frontier* — `AnchorOffset`,
`VoiceSelector`, `PowerOfTwo`, `OctaveOffset` and `NonZeroU16` were each hidden
behind a type that had not compiled yet. The list has to be iterated to a
fixpoint, never taken once.

## Push 4b tranche 1: the pitch-space vocabulary lands, in memory, with a real consumer

`spec/CONTRACT_PUSH4B_PITCHSPACES.md`, dispatched as one vertical slice: types,
the built-in catalog data that fills them, and the consumer that reads them,
landing together rather than as three separate steps. The acceptance test is
behavioural — a `cmn-24` pitch transposes end-to-end, with the resulting scale
position asserted, not merely `is_ok()` — because a Chapter 4 type surface
with no consumer is the `Staff::default_clef` / `NOTEHEAD_ANCHORS` shape this
project has already paid for twice.

**New module `src/pitch_space.rs`.** `PositionStructure` (all four variants:
`Chromatic`, `DiatonicOverChromatic`, `JiLattice`, `Registered`), the checked
constructor `PositionStructure::diatonic_over_chromatic` (enforces all three
clauses of `req:tuning:diatonic-chromatic-mapping` — length, range, strictly
increasing — the way `KeySignature::new` rejects out-of-range fifths), `JiRatio`,
`IntervalAlgebra`, `TranspositionBehavior`, `SpellingRuleSet`, and `PitchSpace`
itself, transcribed field-for-field from the specification's own listings. Plus
`built_in_position_structure(&PitchSpaceId) -> Option<PositionStructure>`, the
built-in catalog: the seven fully-determined spaces (`cmn-12`, `cmn-24`,
`edo-19/22/31/53/72`) resolve; the six the specification names but does not
structurally determine (the three `ji-*` lattice generators, `maqam-base`,
`gamelan-slendro`, `gamelan-pelog`) return `None`, with a per-space comment
recording exactly what the table does and does not fix, rather than a value
this project would later discover was invented. `PositionStructureRegistryId`,
`IntervalAlgebraRegistryId`, and `TranspositionRegistryId` (new `catalog_id!`
entries in `pitch.rs`) back the three `Registered` variants.

**`SpellingParameters` is a deliberate zero-field marker, not a transcription.**
`SpellingRuleSet.parameters: SpellingParameters` is in the specification's own
listing, but `SpellingParameters`' shape is never given anywhere in
`core_spec.tex` — Chapter 4 calls the parameter schema of registered spelling
algorithms an open question outright ("the catalog of *additional* registered
spelling algorithms ... and their parameter schemas, which are normative once
registered"), and the one currently-registered algorithm (`"default"`,
`req:pitch:spelling-algorithm`) is a fixed rule with none. The type exists only
so `SpellingRuleSet`'s field list matches the listing; it carries no state and
nothing constructs one with content. This is the same "do not invent" discipline
the contract applies to the six pitch spaces, applied one level down to a type
rather than a data row.

**No `Codec` impl exists for anything in `pitch_space.rs`, and none was added
to `Score` or `ScoreTuningContext`** (Ruling C, `spec/PLAN_PUSH4B_TUNING.md`).
These types are referenced only by id from canonical state; they stay in memory
so a later tranche remains free to discover they are wrong.

**The P13-S2 interim guard is retired, not widened.** `Pitch::transposed` and
`Pitch::twelve_tet_semitone` no longer compare `scale_position.space.as_str()`
against the literal `"cmn-12"` anywhere; both call a new private helper,
`diatonic_over_chromatic_structure`, that looks the space up in
`built_in_position_structure` and proceeds only when it resolves to
`DiatonicOverChromatic` — `Chromatic`, `JiLattice`, `Registered`, an unknown
identifier, and all six unresolved catalog spaces refuse identically via the
existing `TransposeRefusal::PitchSpaceUnavailable`. `Pitch::transposed`'s
arithmetic is now genuinely space-relative (`chromatic_positions_per_octave`
and `nominal_to_chromatic` come from the resolved structure, not a hardcoded
`12`/`CmnNominal::chromatic()`), which is what makes `cmn-24` transpose in
quarter-tone steps rather than silently applying semitone arithmetic to a
24-chromatic space. `twelve_tet_semitone` keeps its own, stricter gate
(`chromatic_positions_per_octave == 12`) per the contract's instruction not to
rename it: the name stays true because the function still only answers for a
genuinely twelve-chromatic structure, proven structurally now rather than by
identifier. Its six callers across three crates are unchanged.

**One downstream test needed a fixture change, not a behavior change.**
`epiphany-ops::reduce::tests::unresolved_cmn_space_maps_to_canonical_pitch_space_mismatch`
used `"cmn-24"` as a stand-in for "a `Cmn` position in a space the core cannot
resolve." `cmn-24` is now resolved — that is this tranche's entire point — so
the fixture no longer witnesses that case; it now runs deep enough to hit a
second, pre-existing, unrelated refusal (`resolve_transposed_spellings`'s
`twelve_tet_semitone()?` gate, `TranspositionOutOfRange`) instead of the one the
test names. Retargeted to `"edo-31"` (resolved, but to `Chromatic`, not
`DiatonicOverChromatic` — still exactly the case the test is about), with a
comment recording why `cmn-24` stopped serving as the witness. No assertion
weakened, no wire byte or discriminant touched.

**Requirement counts did not move.** No new requirement was added or cited that
did not already exist; `crates/epiphany-testkit/tests/requirement_labels.rs`'s
212/282/282 are unchanged.

## Push 4b tranche 2: the tuning resolver lands, in memory, resolving a pitch to a frequency

`spec/CONTRACT_PUSH4B_RESOLVER.md`. Same vertical-slice discipline as tranche
1: `TuningSystem`, `TuningResolution`, `TuningOverride`, `TuningScope`, a
partial built-in catalog, and the five-scope resolver land together, proven
with real frequencies (`tet-12` C5 ≈ 523.2511 Hz off A4 = 440; a JI major
third measurably distinct from the equal-tempered one), not `is_ok()`.

**New module `src/tuning.rs`.** `TuningResolution` is deliberately a
**two**-of-six-variant enum: `EqualTemperament` and `PerPositionRatios` (plus
`PositionRatio`, which the specification's own listing never spells out the
fields of — defined here as a chromatic position plus a
`crate::pitch_space::JiRatio`, reusing rather than inventing a second
rational type). The other four variants (`Function`, `Overlay`, `Imported`,
`Adaptive`) are not transcribed: nothing in this tranche's catalog
constructs them, and their payload subtrees are exactly the unconsumed
surface tranche 1 already declined twice over.

**The built-in catalog resolves nine of twenty, honestly.** The six `tet-*`
equal temperaments (`tet-12` pairs with `cmn-12`, the default pairing;
`tet-19/22/31/53/72` pair with the matching `edo-*` pitch spaces — forced by
the built-in catalog's cardinalities, not chosen) and the three
`ji-static-5limit-{C,G,D}` just-intonation systems. The latter's twelve
ratios are *computed* — `ji_static_5limit_ratios`, exact integer arithmetic
over the lattice block $\{3^a5^b \mid a\in[-1,2], b\in[-1,1]\}$, octave-reduced
by doubling/halving (never a float comparison) and sorted by cross-
multiplication (never a float division) — not pasted from
`core_spec.tex:4034-4046`'s table. A dedicated test
(`ji_static_5limit_lattice_matches_the_published_construction`) spot-checks
the code's output against that table at all three anchors, proving the two
state the same construction rather than merely agreeing to look similar.

The remaining eleven (the ten historical temperaments and
`ji-adaptive-5limit`) are real catalog entries whose resolution this tranche
**defers**, distinguished from a genuinely unknown identifier by
`TuningCatalogEntry::{Resolved, Deferred}` — so `resolve_pitch_frequency`
can report "not yet supported, here's why" for a known-but-deferred system
and "not a built-in tuning system" for an unknown one, never the same error
for both, and never a guessed frequency for either. Tranche 2b re-derives the
ten temperaments from their ratified constructions (`core_spec.tex`
§"Temperament Constructions"); `ji-adaptive-5limit` waits on `HarmonicContext`,
out of scope per the spec itself.

**Anchoring, done as a ratio-of-ratios so the arbitrary anchor cancels.**
`frequency_for_position` places both the target position and the reference
position on one absolute integer coordinate (generalizing
`Pitch::twelve_tet_semitone`'s idea from a fixed 12 to any tuning's own
divisions, and from `Cmn` positions to `Integer` ones for the EDO spaces),
computes each one's frequency ratio relative to coordinate 0 under the
tuning's resolution, and takes `reference.frequency_hz() * ratio(position) /
ratio(reference.position)`. Which position a construction calls "1/1" cancels
out of that quotient — proven the hard way: the first draft of the JI-major-
third test anchored the comparison at A4 = 440 Hz (the score's own default
reference) and asserted the just third would be flatter than tet-12's; it
failed, because JI-static-5limit-C retunes A relative to C differently than
tet-12 does, so comparing frequencies referenced through A silently mixes
"how A retunes" into "how E retunes." The fix anchors the reference at C4
itself for both systems, isolating the C-to-E interval the test is actually
about — the resolver's arithmetic was correct throughout; the first test
design wasn't.

**The five-scope walk resolves each of pitch space, tuning system, and
reference independently** (`req:tuning:tuning-resolution-order`), voice then
staff then region then the score default, with an explicit
`TuningReference::Explicit` short-circuiting the tuning-system component at
step 1 (pitch space and reference have no step-1 concept of their own — an
`AcousticPitch` carries no field for either) and `AcousticRealization::AbsoluteHz`
short-circuiting the whole frequency, bypassing the walk and the catalog
entirely. "Each region enclosing the pitch, innermost to outermost" turns out
to be exactly **one** region in this data model: a `Voice` is owned by exactly
one `StaffInstance`, owned by exactly one `Region` (containment, not a
derived time-range query), so there is no nested-region multiplicity to walk.
`TuningScope::Range` is defined (Chapter 4's fourth scope variant) but the
walk never matches it — `req:tuning:tuning-resolution-order` enumerates
exactly five steps and does not mention it, so inventing a sixth would be
exactly the kind of unratified addition this project's process exists to
catch; documented as a scope note, not silently dropped.

**The compatibility check accepts only exact `pitch_space` equality**
(`req:tuning:tuning-system-compatibility`): no compatibility-mapping registry
exists, matching how tranche 1 left the pitch-space registry unbuilt. A
mismatch (e.g. `tet-19`'s declared `edo-19` against an unchanged `cmn-12`
default) is rejected, not silently resolved — proof-of-life item 4.

**`ScoreTuningContext` gains `overrides: Vec<TuningOverride>`, in memory
only.** This is the one wire-adjacent change, and it isn't a wire change: the
type's canonical encoding stays exactly the three fields it always had
(`default_pitch_space`, `default_tuning_system`, `reference`), because adding
a fourth field to a `struct_codec!`-generated type breaks the macro outright
— its generated `dec` ends in a struct literal naming every field it was
given, so a fourth field cannot compile against it. The `struct_codec!` line
is replaced with a hand-written `impl Codec` (`codec.rs`) and `impl TextValue`
(`textvalue_graph.rs`) that encode/project exactly the three wire fields, in
their original order, and construct `overrides: Vec::new()` unconditionally
on decode/parse. Two round-trip tests prove the field never reaches either
canonical surface: `codec::tests::score_tuning_context_overrides_do_not_reach_the_wire`
(a context with a non-empty `overrides` encodes to byte-identical output as
one with empty `overrides`, and decoding either reconstructs `overrides` as
empty) and `textvalue_graph::tests::score_tuning_context_round_trips_and_overrides_do_not_project`
(the same, for the text projection). Field order in the Rust struct is free
(the manual codec fixes the wire order independently); the specification's
eventual major-3 field order puts `overrides` last, after
`accidental_extensions` and `smufl` — that pairing is the wire tranche's
problem, not this one's.

**No `Codec` impl exists for anything new in `tuning.rs`.** These types are
referenced only by id and by the one in-memory `ScoreTuningContext` field;
they stay free to change once the wire tranche (schema major 3) discovers
something about them.

**Requirement counts did not move again.** No `.tex` file was touched, no
requirement added; the 212/282/282 counts stay put.
