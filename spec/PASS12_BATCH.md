# Pass 12 — Batch Tracker

*Maintained by Agent F (testkit & tripwire), per `spec/PHASE2_QUICKSTART.md`:
"Ambiguities discovered during Phase 2 implementation go into a Pass 12 batch,
not into code improvisations. … Don't open Pass 12 until at least 3 items
accumulate."*

**Status: OPEN.** Agent H's landing (spelling + decomposition pre-passes)
surfaced five candidates, crossing the ≥3 threshold. This file is the running
collection; G ratifies (or defers/rejects) the batch when Phase 2's open
questions are resolved. F collects, F does not resolve.

## How an item lands here

When implementation hits a behavior the ratified spec does not determine, the
responsible agent records it in their crate's `DECISIONS.md` with an ID
(`P12-<agent><n>`) and a one-line rationale, then adds a row below. Improvising in
code instead is the failure mode this batch exists to prevent.

## Items

| ID | Source | Summary | Disposition target |
|----|--------|---------|--------------------|
| P12-H1 | `epiphany-core` H | Ratify `SpellingAlgorithmId::Default` = Temperley line-of-fifths v1 (Pass 11 closed before H landed; the id `"default"` is the crate's proposal until ratified — not a byte layout, so nothing golden-locks on it). | G (algorithm-choice ratification) |
| P12-H2 | `epiphany-core` H | `KeySignatureChange` / `ClefChange` are anchor-only placeholders; context-aware spelling infers tonal context from the melody (line-of-fifths centre of gravity) rather than a *declared* key. A real key/clef content model would let spelling/decomposition honour declared keys and place cancelling naturals. Flagged as a graph-model gap. | G (graph model) |
| P12-H3 | `epiphany-core` H | Chromatic-run convention (ascending = sharps, descending = flats) is only a *tiebreak* in the centre-of-gravity rule, so an isolated chromatic run with no tonal context may pick the enharmonic the convention would not. Voice-leading refinement deferred. | G / Pass 12 (spelling) |
| P12-H4 | `epiphany-core` H | Decomposition simplifications: single governing meter per region (multi/mid-region meter changes deferred); region origin assumed a barline (anacrusis deferred); compound-meter beat grouping beyond the dyadic default; tuplet nesting and cross-beat tuplet members; double+ augmentation dots (`MAX_DOTS = 1`). | G (decomposition scope) |
| P12-H5 | `epiphany-core` H | Automatic spelling under aleatoric regions (the spec's open question). H spells pitches region-independently but performs no region-specific aleatoric spelling; defer if the algorithm does not generalise cleanly. | G / Pass 12 (open question) |
| ~~P12-I1~~ **RESOLVED (I-1)** | `epiphany-layout-ir` / `engrave` / `render-svg` I | The v0 pipeline was a **structural placeholder** (one *arbitrary* glyph per object at `y = 0`), not real notation. **Resolved by I-1 (Phase 2-3):** `to_constrained` now builds real notation — clef-relative noteheads (by `NoteValue`), spelling-derived accidentals, key/time signatures, rests, barlines, and the staff-line/stem strokes — and the Engraver re-spaces it; the human visual-acceptance gate is met (goldens locked against the stub *and* the real Engraver). The Ch 7 engraving-boundary question resolved to: notation construction lives in `to_constrained`, horizontal spacing in the Engraver. | ✅ done |
| ~~P12-I2~~ **RESOLVED (wired)** | `epiphany-determinism` / `epiphany-layout-ir` I | Stable layout-object id derivation (`MUSCLOID`, Pass-11 item 2.6). **Wired:** `epiphany-determinism` now reserves the built-in `DomainTag::LAYOUT_OBJECT_ID` (`MUSCLOID`), and `layout-ir`'s provenance derivations (single / multiply-manifested / synthesized) plus the engraving-decision id route through it (no longer borrowing `MUSCCONF`). Layout ids stay non-canonical, so only `data-prov` hex in the render goldens changed; no durable/interchanged artifact. See `layout-ir/DECISIONS.md` and `req:layoutir:object-id-derivation`. | ✅ done |
| ~~P12-I3~~ **RESOLVED (I-4a)** | `epiphany-layout-ir` I | The bundled `BRAVURA_METRICS` were *approximations* disagreeing with the renderer's genuine outlines. **Resolved by I-4a:** the metrics table is re-extracted from the **same** SHA-pinned `bravura-1.392` font the outlines come from, with bboxes rounded *outward* so each metric box contains the drawn ink (a `render-svg` test proves containment); `BRAVURA_VERSION = SemVer(1, 392, 0)`. A coupled barline-placement bug it surfaced (bottom-origin glyph floated) was fixed in the same increment. | ✅ done |
| P12-K1 | `epiphany-ops` K | A v0 `RespellPitch` carried a `ContentHash` *fingerprint* of the spelling, not the `PitchSpelling`. The v0→v1 migration (Operation Catalog, M1) cannot invert a fingerprint, so it recovers the spelling from the score-graph context (an explicit per-pitch spelling attachment whose canonical bytes hash to the fingerprint) and returns `MigrationError::Irreversible` (bundle opens read-only) when the context lacks it. Every other representative payload migrates self-contained; this is the lone exception. Confirm the read-only fallback is the intended disposition vs. requiring a v0 corpus that preserves spelling pre-images. | G / Pass 12 (migration) |
| P12-K2 | `epiphany-ops` K | The `Transpose` op (Operation Catalog, M2 Group 1) carries a minimal `chromatic_steps: i32` interval and `reduce_onto` applies it as a CMN *alteration* shift only. Faithful interval algebra (diatonic vs. chromatic intervals, octave/nominal renormalization, transposition in non-CMN pitch spaces) is the deferred Chapter 4 tuning-catalog territory. The prototype also clamps the shifted alteration to the `i8` range, so an extreme transpose silently saturates instead of renormalizing — another reason the representation needs pinning. Pin the interval representation and transposition semantics when the tuning catalog lands. | G / Pass 12 (tuning) |

## Not yet open elsewhere

Agent I (Track A) has contributed P12-I1..I3 above. Track B's Agent K has
contributed P12-K1 (Operation Catalog M1). Agent J (Binary Format companion) has
not yet contributed; when it does, append rows — the batch is already open, so it
joins directly (no new threshold).
