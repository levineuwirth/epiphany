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

## Push 4b tranche 2b: the ten historical temperaments resolve, built from their constructions

`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md`. The ten `TuningCatalogEntry::Deferred`
entries tranche 2 left behind now resolve, via the specification's own third
`TuningResolution` variant, `Function`. Same in-memory discipline as
tranches 1 and 2: no `Codec`, no wire movement, canonical bytes untouched —
only `tuning.rs`, `pitch.rs` (one new `catalog_id!`), and `lib.rs`
(re-exports) changed.

**`TuningResolution::Function { function: TuningFunctionId, parameters:
TuningParameters }` lands, plus the `TuningFunctionId` catalog newtype**
(`pitch.rs`, beside `TuningSystemId`). The ten temperaments are reserved
built-in ids spelled identically to their `TuningSystemId` (`"pythagorean"`,
`"werckmeister-iii"`, …); an id with no reserved built-in has no registry
to fall back on, so `coordinate_ratio`'s `Function` arm returns `None` for
it — the extension point fails closed, never a guessed frequency.
`frequency_for_position`'s `divisions` match gains a `Function` arm too:
since the variant carries no division count of its own, divisions comes
from the *pitch space*'s chromatic cardinality instead (a new private
`chromatic_cardinality(&PositionStructure) -> Option<u32>`, 12 for
`cmn-12`), never from the resolution.

**`TuningParameters` is a deliberate zero-field marker, exactly like
`SpellingParameters`.** No built-in parameterizes a `Function` resolution —
each of the ten temperaments is fixed entirely by its `TuningFunctionId`
alone — and `core_spec.tex` never gives this type's shape (it calls the
sibling `AdaptiveTuningParameters` "likewise undefined" for the identical
reason, `:4083`). The type carries no state and exists only so
`TuningResolution::Function`'s field list matches the specification's own
listing.

**The construction, not the cents table, is what's built.** Each temperament
is represented as a `Construction = [FifthTempering; 12]`: one
`FifthTempering` tag per fifth of the fixed circle-of-fifths chain
`C–G–D–A–E–B–F♯–C♯–G♯–E♭–B♭–F–(C)` (`Pure`, `NarrowPythagorean(fraction)`,
`WidePythagorean(fraction)`, `NarrowSyntonic(fraction)`, `NarrowSchisma`, or
— for the four non-circulating temperaments' one closing wolf — `Residual`,
whose cents are computed as whatever value brings the other eleven arcs'
sum to exactly seven octaves, never given a fraction of its own). One shared
`walk_temperament` function walks any construction forward from C,
accumulating raw (unreduced) cents, then reduces each of the twelve chain
notes' cumulative cents mod 1200 into its `cmn-12` chromatic degree's ratio.
The same walk produces the wolf (for the four non-circulating constructions)
and the full twelve-note closure (for the six circulating ones) without
separate code paths — reducing mod 1200 is what lets the wolf simply fall
out where the chain doesn't close, exactly as `core_spec.tex`'s own framing
puts it ("any assignment of twelve distinct pitch classes must sum to seven
octaves by construction"). The four comma sizes (`pure_fifth_cents`,
`pythagorean_comma_cents`, `syntonic_comma_cents`, `schisma_cents`) are each
`1200·log2(exact ratio)` in `f64` — never a hardcoded rounded cents
constant — and the schisma is computed independently from `32805/32768`
rather than derived as `pythagorean − syntonic`, so the closure tests prove
that identity rather than assume it.

**The closure invariant, recomputed in code, is what the tests assert.**
For the six circulating temperaments (`werckmeister-iii`/`-iv`, `vallotti`,
`kirnberger-ii`/`-iii`, `young-ii`), a test recomputes `12·pure_fifth_cents −
raw_closure_cents` (the sum of the twelve fifths' deviations from pure) and
asserts it equals `pythagorean_comma_cents()` (≈23.4600 c) within `1e-9`,
plus a per-fifth bound (`<15 c` deviation from pure) proving none of the
twelve is secretly a wolf. Computed values: werckmeister-iii, -iv, vallotti,
kirnberger-ii, kirnberger-iii, and young-ii each summed to `23.460010…` c
against the ratified `23.4600` c. For the four non-circulating ones
(`pythagorean`, the three `meantone-*`), the residual wolf (chain arc 8,
`G♯–E♭`) is asserted against the spec's ratified value: pythagorean
678.495 c, computed 678.495 c; meantone-1/4 737.637 c, computed 737.637 c;
meantone-1/5 725.809 c, computed 725.809 c; meantone-1/6 717.923 c,
computed 717.923 c — all within `0.001` c.

**The Kirnberger schisma-fifth trap and the Pythagorean-vs-syntonic comma
trap were both reproduced as mutations, and both caught.** Dropping
`kirnberger-ii`'s closing schisma fifth (changing its `F♯–C♯` arc from
`NarrowSchisma` to `Pure`) landed the closure at 21.506 c — one schisma
(1.954 c) short of 23.460 c — and only the closure test died. Changing
`werckmeister-iii`'s comma from Pythagorean to syntonic (`NarrowPythagorean`
→ `NarrowSyntonic` at the same fraction) landed the closure at 21.506 c
(`4 × ¼ syntonic`) instead of 23.460 c, exactly the defect the contract
predicted, and again only the closure test died. Both were reversed by
undoing the exact substitution, never `git checkout`.

**Discriminator and resolver-level tests were each mutation-verified too**
(construction- or dispatch-level mutations, run to red, then reversed):
swapping the `E`/`B` chromatic-degree slots in the shared
`CHAIN_CHROMATIC_DEGREE` table killed both `pythagorean`'s E/F♯ test and the
`meantone-1/4-comma` just-third test; doubling `kirnberger-iii`'s comma
fraction to match `kirnberger-ii`'s killed the D-value discriminator (and,
incidentally, the closure test, since the doubled fraction no longer sums to
one Pythagorean comma either); moving `pythagorean`'s residual arc from
index 8 to index 7 killed only the non-circulating wolf test; making
`temperament_ratios`'s wildcard arm return `pythagorean`'s construction
instead of `None` killed only the fail-closed test; typo-ing the
`"vallotti"` match arm killed only the "all ten resolve" test; and making
the resolver's `Function` arm ignore `function` and fall back to 12-TET
killed both the `werckmeister-iii` C♯-distinctness test and the fail-closed
test (the latter because an unknown id now also produced a ratio instead of
`None`). Every mutation was reversed by undoing its exact substitution.

**Zero golden or digest movement**, confirmed by the full gate: `cargo fmt
--all --check`, `cargo clippy --workspace --all-targets` (0 warnings),
`cargo test --workspace` (0 failed across every crate), `RUSTDOCFLAGS="-D
warnings" cargo doc --workspace --no-deps` (0 warnings, after two
intra-doc links to the private `temperament_ratios`/`coordinate_ratio`
were de-linked to plain code spans), `conformance_suite` (8/8), and
`requirement_labels` (6 passed, counts unchanged at 212/282/282) all pass.
No `.tex` file was touched and no requirement was added.

## Push 4b tranche 3a: the accidental/glyph/engraving vocabulary lands, in memory, with two real consumers

`spec/CONTRACT_PUSH4B_ACCIDENTALS.md`. Same reversible-first discipline as
tranches 1/2/2b: the accidental-registry, glyph-reference, and engraving type
surface from Chapter 4 §"Accidental Registries" / "Glyph References and
SMuFL" lands in `epiphany-core`, in memory, with two real consumers, and
**no `Codec`, no wire movement** — canonical bytes stay byte-identical. This
splits tranche 3's full `ScoreTuningContext` completion in two: 3a builds and
exercises the shapes while they are still free to change; 3b (a later
tranche) puts `accidental_extensions`, `smufl`, and `overrides` on the wire
together, opening schema major 3 — an irreversible freeze, so it is not done
until the shapes have had a consumer.

**New module `src/accidental.rs`.** Transcribed field-for-field from
`core_spec.tex:3054`-`3277`, in spec order, with three ratified corrections
(P13-S10/S11/S12, filed and ratified before dispatch):

* **S10** — `PitchSpaceModification::Cents(CanonicalF64)`, not `Cents(f64)`:
  a raw `f64` is unencodable in canonical state (`serialize.rs` decodes
  floats only through `CanonicalF64::from_le_bytes`; there is no `Codec for
  f64`). Locked by `accidental::tests::cents_round_trips_a_finite_value_and_guards_non_finite`,
  which round-trips a finite cents value and shows `CanonicalF64::new` rejects
  NaN/±infinity outright, so a `Cents` payload can never be non-finite.
* **S11** — `AnchorPoint { x: SpaceUnit, y: SpaceUnit }`, defined core-native.
  The specification references it (`:3166`, `AccidentalEngraving::anchor`)
  but never defines it, and `epiphany-core` cannot depend on
  `epiphany-layout-ir`. Doc comment pins the frame ratified alongside S11:
  canonical space units, y-up, relative to the glyph's coordinate origin
  (needed because `EngravingBoundingBox` is itself "relative to the glyph's
  anchor point", `:3160`, so the anchor needs an unambiguous origin of its
  own).
* **S12** — `SmuflVersion { major: u16, minor_centi: u16 }`, the minor stored
  fraction-normalized to hundredths (1.4 -> 40, 1.3 -> 30, 1.12 -> 12), built
  only through the checked `SmuflVersion::from_decimal(major, minor_digits)`
  constructor so a caller cannot pass a literal minor digit by mistake.
  Locked by `accidental::tests::smufl_version_orders_the_real_release_sequence`,
  which asserts SMuFL's actual release order (1.12 < 1.18 < 1.20 < 1.3 < 1.4)
  — a test that would pass under literal-minor storage (where 1.3 and 1.4
  sort before 1.12) would not lock S12 at all; see the mutation below, which
  reproduces exactly that failure and confirms this test catches it. **Not**
  `epiphany_layout_ir::SmuflVersion` (`glyph.rs:29`, literal-minor,
  load-bearing for `GlyphCatalogIdentity`) — at this tranche that type was
  untouched, the two a deliberate, bounded homonym (`epiphany-core` cannot
  depend on `epiphany-layout-ir` in any case), pending a later unification.

  **Superseded by tranche 3b-ii (2026-07-23), which corrects two forward-looking
  claims made here.** The homonym is gone: layout-ir's type is deleted and
  re-exports this one, so `GlyphCatalogIdentity` now carries the normalized
  shape and its backwards ordering (live at the time this was written) is
  fixed. And the anticipated **"golden regen" never happened — there was
  nothing to regenerate.** No golden, baseline, or vector anywhere in the
  workspace is pinned to the catalog identity: every assertion on
  `ResolvedLayoutIR::canonical_bytes()` is *relative* (stability, determinism,
  and a sensitivity check that mutates `metrics_hash`, never `smufl_version`),
  and the committed SVG/PNG goldens do not embed it. The encoded minor moved
  `0x04` → `0x28` with no committed bytes pinning it. See
  `epiphany-layout-ir/DECISIONS.md` for the unification itself.

Also new: `CustomGlyphId`, `ModificationRegistryId`, `AccidentalGroupId`
(`catalog_id!` entries in `pitch.rs`, beside `AccidentalRegistryId`/
`AccidentalId`, which already existed). **No `Codec` impl exists for
anything in `accidental.rs`.**

**Two real consumers, so this does not become the `NOTEHEAD_ANCHORS` trap.**

*(a) `resolve_accidental(base_registry, extensions, id)`* — resolution
precedence per `core_spec.tex:3224` ("Extensions are stored on the score and
override or augment the base registry during resolution"): an `overrides`
entry wins over an `additions` entry wins over the base registry, checked by
`accidental::tests::resolution_precedence_overrides_beats_additions_beats_base`
across all three tiers plus the not-found case. `base_registry` is supplied
by the caller rather than looked up from an in-core catalog: `epiphany-core`
has no built-in catalog of accidental-registry *bodies* this tranche (the
same deferred-data-catalog discipline tranche 1 applied to the six
underdetermined pitch spaces) — inventing one would itself be the
`NOTEHEAD_ANCHORS` failure this consumer exists to avoid.

*(b) `accidental_modification_compatible_with_space(modification, space)`*,
wired into `check_invariants` as
`GraphIndex::check_accidental_modification_compatibility` — the
`req:tuning:accidental-modification-compatibility` invariant
(`core_spec.tex:3120`). `space` resolves structurally against
`built_in_position_structure` (Push 4b tranche 1), the same catalog
`Pitch::transposed` uses. The requirement's two named rules (`CmnChromatic`
only in `DiatonicOverChromatic`-shaped spaces; `EdoSteps` only in
`Chromatic` or `Registered`) are matched directly against
`PositionStructure`; the contract's instruction to "extend the same shape"
gives `JiRatio` the identical `JiLattice`-or-`Registered` rule. `Cents` and
`Registered` modifications have no requirement-stated constraint, so they
are accepted whenever the space itself resolves — inventing a constraint the
requirement does not state would be the same failure as inventing a JI
generator ratio. An unresolvable space (outside the built-in catalog, or one
of the six catalog-named-but-underdetermined ones) fails closed for *every*
modification kind, `Cents`/`Registered` included, mirroring tranche 1's
`Pitch::transposed` discipline. `check_invariants` folds the result into the
existing `GraphInvariant::CrossCuttingRefsResolve` tag rather than inventing
a 20th spec-enumerated invariant — the same choice already made for the
tempo-map and aleatoric-model checks, since this is a Chapter 4 requirement,
not one of the 19 spec-enumerated Chapter 5 graph invariants.

The check determines "every pitch space that references \[a\] registry"
(the requirement's phrase) as every pitch space the score's tuning context
concretely names: `default_pitch_space`, plus any per-scope override's
`pitch_space` (`crate::tuning::TuningOverride`). `epiphany-core` has no
built-in catalog linking an `AccidentalRegistryId` to the pitch space(s)
that declare it their `accidental_registry` — tranche 1 built only the
id -> `PositionStructure` map, not a populated `PitchSpace` catalog — so
this is the referencing relation the score can actually attest to, stated
honestly rather than invented. Because every existing generator leaves
`accidental_extensions` empty (this tranche adds no test data to any
generator), the new check is silent across the entire pre-existing
test/property-test corpus — proven directly by
`invariants::accidental_compatibility_tests::a_score_with_no_accidental_extensions_never_fires_this_check`.

**Glyph and engraving metadata are carried, not consumed, in core.**
`GlyphReference`, `AccidentalEngraving` (and its `EngravingBoundingBox`/
`AnchorPoint`), and `AccidentalCombination` are read by both consumers only
incidentally — resolution returns the whole `AccidentalDefinition`, and the
compatibility check reads past `engraving`/`glyph`/`combination` straight to
`modification`. Their deep consumer is the engraver, out of
`epiphany-core`, a later tranche; no in-core consumer was fabricated for
them to manufacture coverage.

**`GlyphReference` is Chapter 4's own, deliberately not unified with
`epiphany_layout_ir::GlyphReference`** (`glyph.rs:50`, a glyph *name*,
`Cow<'static, str>`, a rendering concern): same name, unrelated types
(Ruling D's "correction"). `epiphany-core` cannot depend on
`epiphany-layout-ir` in any case, so within this crate there is no
ambiguity.

**`ScoreTuningContext` gains its second and third in-memory-only fields.**
`accidental_extensions: Vec<ScoreAccidentalExtensions>` and
`smufl: SmuflVersionRequirement` join `overrides` (Push 4b tranche 2) as
Rust fields with **no wire presence**: the hand-written `Codec::enc` is
byte-for-byte unchanged (still exactly `default_pitch_space`,
`default_tuning_system`, `reference`, in that order); only `dec` grows two
more defaults (`accidental_extensions: Vec::new()`, `smufl:
SmuflVersionRequirement::default()`), alongside the pre-existing `overrides:
Vec::new()`. `SmuflVersionRequirement::default()` is `{ minimum:
SmuflVersion(1.4), authored_against: SmuflVersion(1.4) }` — the SMuFL
version this repository already targets
(`epiphany_layout_ir::glyph::GlyphCatalogIdentity`'s default), so the
default aligns with what the layout-ir unification will target. The
matching `impl TextValue` (`textvalue_graph.rs`) gets the identical
treatment: `project` still emits exactly three fields, `parse` defaults all
three in-memory fields.

Proved with a new test extending tranche 2's
`score_tuning_context_overrides_do_not_reach_the_wire` pattern to all three
fields at once:
`codec::tests::score_tuning_context_accidental_extensions_smufl_and_overrides_do_not_reach_the_wire`
(binary) and
`textvalue_graph::tests::score_tuning_context_accidental_extensions_smufl_and_overrides_do_not_project`
(text) — a fixture with non-empty `accidental_extensions`, a non-default
`smufl`, and a non-empty `overrides` encodes/projects byte-for-byte
identically to the all-default fixture, and decoding/parsing either
reconstructs all three as empty/default. The original tranche-2 tests are
untouched, preserving their historical narrative.

**Every new test was mutation-verified** (substitution made, test run to
red, then reversed by undoing the exact substitution — never `git
checkout`):

* **S12**: `SmuflVersion::from_decimal` mutated to store the minor literally
  (`minor_centi = value` unconditionally, dropping the ×10 for one-digit
  input). Killed both `smufl_version_orders_the_real_release_sequence` (the
  release-order lock: `(1, 20)` no longer sorted before `(1, 3)`) and
  `smufl_version_from_decimal_normalizes_one_and_two_digit_minors`.
* **S10**: `PitchSpaceModification::Cents` reverted to `Cents(f64)`. This is
  a type-level correction, so the "test" is the type system itself: nine
  call sites across `accidental.rs`'s own tests and the codec byte-identity
  fixture failed to *compile* against the reverted shape (`Option<CanonicalF64>::map(Cents)`
  no longer type-checks; direct `Cents(CanonicalF64::new(...).unwrap())`
  construction no longer type-checks) — the strongest possible test failure.
* **Resolution precedence**: `resolve_accidental` mutated to check
  `base_registry` first, `additions` second, `overrides` last (precedence
  reversed). Killed `resolution_precedence_overrides_beats_additions_beats_base`
  (resolved to the base-registry entry, `-1`, instead of the overrides
  entry, `-3`).
* **Compatibility check**: `accidental_modification_compatible_with_space`
  mutated to `true` unconditionally. Killed five tests at once:
  `cmn_chromatic_is_compatible_only_with_diatonic_over_chromatic`,
  `edo_steps_is_compatible_with_chromatic_and_registered_not_diatonic`,
  `ji_ratio_is_compatible_only_with_ji_lattice`,
  `every_modification_kind_fails_closed_on_an_unresolvable_space` (all four
  in `accidental.rs`), and — proving the graph-level wiring is load-bearing,
  not just the pure predicate —
  `invariants::accidental_compatibility_tests::cmn_chromatic_accidental_in_edo_31_fires`.
* **Wire invisibility**: `ScoreTuningContext::enc` mutated to push
  `accidental_extensions.len() as u8`, and separately `impl TextValue::project`
  mutated to append the same length as a projected field. The binary
  mutation killed the new byte-identity test (last byte `1` vs `0`); the
  text mutation killed *both* text-projection tests, including the
  pre-existing tranche-2 one (`parse` still expects exactly 3 fields, so
  even `ScoreTuningContext::default()`'s own round-trip failed to parse a
  4-field list) — confirming the frozen field arity is what both tests
  actually enforce.

**Zero golden or digest movement**, confirmed by the full gate after every
mutation was reverted: `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets` (0 warnings), `cargo test --workspace` (0 failed across every
crate), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` (0
warnings, after one intra-doc link — `` [`:3160`] `` — was de-linked to a
plain parenthetical citation), `conformance_suite` (8/8), and
`requirement_labels` (6 passed, counts unchanged at 212/282/282) all pass.
No `.tex` file was touched and no requirement was added.

## Push 4b tranche 3b-i: schema major 3 opens — `smufl` and `overrides` reach the wire

`spec/CONTRACT_PUSH4B_3BI_WIRE.md`. The user split the original 3b sketch in
two and staged it down: 3b-i (this) freezes only `smufl` and `overrides` onto
the score wire; `accidental_extensions` stays in memory (no consumer yet —
the engraver, out of `epiphany-core`); the `SmuflVersion` unification with
`epiphany-layout-ir` and the `GlyphCatalogIdentity` move are **3b-ii**, a
separate dispatch, untouched here. This is the first **irreversible** byte
layout of Push 4b: schema major 3 is now open, and every layout below is
frozen forever under `req:binfmt:frozen-layout`.

**The frozen wire**, exactly as the contract specifies:
`ScoreTuningContext(v3) = (default_pitch_space, default_tuning_system,
reference)` (the untouched v0..v2 prefix) `⌢ smufl ⌢ overrides`. Four new
leaf `Codec` impls, all hand-written (not `struct_codec!`/
`cstyle_enum_codec!`, since those macros also generate a `TextValue` impl,
and none of these four types has one — text projection is a separate
surface this tranche does not touch, so the macro would either fail to
compile, missing `TextValue for u16`/`TuningScope`, or silently open a new
projection surface nobody asked for):

* `SmuflVersion = major(u16 LE) ⌢ minor_centi(u16 LE)`.
* `SmuflVersionRequirement = minimum(SmuflVersion) ⌢ authored_against(SmuflVersion)`.
* `TuningScope`: one discriminant byte ⌢ body — `0` Voice(VoiceId), `1`
  Staff(StaffId), `2` Region(RegionId), `3` Range { start, end, voices }
  (`TimeAnchor`/`VoiceSelector` already encode).
* `TuningOverride = scope(TuningScope) ⌢ pitch_space(Option<PitchSpaceId>) ⌢
  tuning_system(Option<TuningSystemId>) ⌢ reference(Option<ReferencePitch>)`.

**The reroute — the highest-risk edit, and it reaches further than the
contract's own two named call sites.** The contract named
`decode_v0_score`/`decode_v1_score` as the must-not-miss reroute onto a new
frozen `dec_tuning_context_v2` (the pre-v3, 3-field form — "v2" because it is
the form majors 0/1/2 all share, exactly the naming convention
`dec_ccr_v1`/`dec_metadata_v1` already use for "the frozen form as of the
prior major"). Auditing the two named decoders surfaced two more sites the
contract's prose did not call out but the same bug applies to: **their
byte-exact-inverse encoders**, `encode_v0_score` and `encode_v1_score`, both
of which called `s.tuning_context.enc(&mut out)` — the *live* codec. Once the
live codec became 5-field, both would have silently started emitting 5-field
tuning-context bytes inside a nominally-frozen v0/v1 form, which
`decode_v0_score`/`decode_v1_score`'s own strict-canonicality re-encode check
(`encode_v0_score(&score) != bytes`) would then reject on *every* input —
not a subtle bug, a total breakage of the v0/v1 migration paths, caught only
because the goldens exercise real synthesized v0/v1 bytes rather than
hand-written literals. Fixed by routing both through a new
`enc_tuning_context_v2`, symmetric with `dec_tuning_context_v2`. Also added
(named by the contract): `encode_v2_score`/`decode_v2_score`, the newly-frozen
schema-major-2 score form — the live walk for the other 18 fields (unchanged
between major 2 and 3) plus `enc_tuning_context_v2`/`dec_tuning_context_v2`
for `tuning_context`. `decode_canonical_versioned` now dispatches
`3 => decode_canonical, 2 => decode_v2_score, 1 => decode_v1_score, 0 =>
decode_v0_score`.

**Consequences the contract didn't spell out, found by re-running every
existing test after the bump:**

* Two pre-existing tests asserted `decode_canonical_versioned(bytes, 2)` where
  `2` meant "the current major" (`v1_round_trips_non_default_values_for_every_new_field`,
  `current_major_round_trips_non_default_values_for_every_major_2_field`) —
  both bumped to `3`.
* `v0_score_migrates_default_filling_all_three_new_fields` asserted major `3`
  was *unsupported* (`decode_canonical_versioned(.., 3).is_err()`) — true
  before this tranche, false after (3 is now current). Bumped the probe to
  `4`, the new first-unsupported major.
* `v1_score_migrates_default_filling_the_major_2_fields`'s exact byte-count
  size anchor (`current.len() - v1.len() == expected_removed`) silently grew
  by a flat 12 bytes — `smufl`'s 8 (two bare `u16` pairs) plus `overrides`'
  empty-count 4 — present in `current` (now v3) but absent from `v1` (frozen
  pre-v3). Added `+ 12` to `expected_removed`, documented why.
* Added the contract's asked-for `v2_score_migrates_default_filling_smufl_and_overrides`,
  mirroring the v0/v1 migration goldens: synthesizes real v2 bytes via
  `encode_v2_score`, asserts the flat 12-byte size anchor, and checks the v2
  bytes migrate to the same score `decode_canonical` reaches.

**The two off-the-wire tests fold into one staging-boundary test**, per the
contract: `score_tuning_context_overrides_do_not_reach_the_wire` (tranche 2)
is deleted outright (fully superseded); `..._accidental_extensions_smufl_and_overrides_do_not_reach_the_wire`
(tranche 3a) is renamed to
`score_tuning_context_smufl_and_overrides_reach_the_wire_accidental_extensions_do_not`
and inverted: the same three-field-loaded fixture now asserts `smufl`/`overrides`
survive `enc`→`dec` equal, while `accidental_extensions` still decodes empty.
Mutation-verified (weakened the test to also expect `accidental_extensions`
survival — it failed, confirming the drop assertion is real, not vacuous).

**`bundle.rs:1356`'s `UnsupportedCanonicalChunkMajor { schema_major: 3 }`
case — inspected, not guessed, per the contract's own instruction.** It is
`committing_an_unsupported_major_op_root_makes_the_live_bundle_read_only`,
which stages an `OperationEnvelopeBlock` (not a canonical base) at schema
major 3 to prove "beyond the op-block accept-set." Since no operation
payload embeds the tuning context, `max_supported_major(OperationEnvelopeBlock)`
stays at 2 this tranche (verified: a full search of `epiphany-ops` for
`ScoreTuningContext`/`TuningOverride`/`tuning_context`/
`SmuflVersionRequirement` finds nothing) — so major 3 is *still* beyond that
role's accept-set. **Left at 3, unchanged**; bumping to 4 would have been
wrong (it would stop testing the boundary this test actually exercises). The
sibling canonical-base test (`a_canonical_base_stamped_above_major_0_opens_read_only`,
`schema_major: 1`) is a different test entirely and was never in scope.

**Version/accept-set**: `SchemaVersion::V3 = {3, 0}` (`epiphany-bundle/src/ids.rs`);
`max_supported_major(Snapshot) = 3`, `max_supported_major(OperationEnvelopeBlock) = 2`
(unchanged — the first data-model major where a chunk role's max does not
move in lockstep with the others); `testkit::roundtrip::assert_score_serialization_stable`
flips both `for_major(2)` sites to `for_major(3)`.

**Spec**: new `spec/binary_format.tex` §"Schema Major 3" (mirrors §Schema
Major 2's structure: where the fields reach, cross-major reader behavior,
changed/new layouts, the v2→v3 migration table), the chunk-level-gate section
updated to state the accept-set is per-role as of this bump (Snapshot 3,
OperationEnvelopeBlock 2, everything else 0), a revision-history entry
(0.10.0), and the title-page version line. **No `req:` label added** —
requirement counts stay 212/282/282 (asserted unchanged by
`requirement_labels`, which passed 6/6).

**Full gate green** after the bump: `cargo fmt --all --check`; `cargo clippy
--workspace --all-targets` (0 warnings); `cargo test --workspace` (1271
passed, 0 failed); `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
--no-deps` (0 warnings); `conformance_suite` (8/8); `requirement_labels`
(6/6, counts unchanged at 212/282/282 — there is no `--example
requirement_labels`; it is the integration test at
`epiphany-testkit/tests/requirement_labels.rs`, run via `cargo test -p
epiphany-testkit --test requirement_labels`). Mutation-verified the reroute
itself: reverting all three `dec_tuning_context_v2` call sites back to the
live `Codec::dec` killed six tests at once (`v0_score_migrates_*`,
`v1_score_migrates_*`, `v2_score_migrates_*`, `v0_regions_inside_canvas_decode_after_region_grew`,
`v0_decode_is_strictly_canonical_over_the_v0_wire_form`,
`v1_round_trips_non_default_values_for_every_new_field`) — confirming the
reroute is load-bearing across the whole frozen-decoder family, not just the
two sites the contract named.
