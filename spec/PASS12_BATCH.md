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

## Not yet open elsewhere

Track A's other agents (I) and Track B (K, J) have not yet contributed items.
When they do, append rows; the batch is already open, so they join directly (no
new threshold).
