# Epiphany Phase 2 — Implementation Quickstart (Agents G–K)

This is the operational reference for the next phase. The foundation exists; the full architecture and contract is in `spec/core_spec.pdf` (Pass 10 / 244 pages, pending Pass 11 ratification). The v0 dispatch is in `spec/QUICKSTART.md` and remains the canonical context for *what's already built and why*. This document is the dispatch layer for what comes next.

## Project context

The v0 prototype baseline is complete: six Rust library crates that prove the architecture works end to end. All six v0 acceptance criteria pass; gates are honest; per-crate decisions and Pass-11 candidates are batched and golden-locked. A developer can build on the crates; a musician cannot yet see, hear, or save anything.

Phase 2 turns the foundation into something perceivable, durable, and ratified:

- **Pass 11 (G)** ratifies the durable byte layouts so storage is stable before anyone writes documents on top.
- **Track A (visible slice — H, I)** makes notes appear on a page: spelling, decomposition, real engraving, SVG output. This is what makes Epiphany demonstrable.
- **Track B (interchange core — K, J)** makes documents portable: real operation payloads and a finalized wire format. This is what makes Epiphany interoperable.
- **F continues** as the cross-cutting tripwire. Mandate broadens substantially; nothing else changes.

The two tracks run in parallel after Pass 11's Bucket 1 lands. They will not redesign each other, but they *will* coordinate through core value types, serialization conventions, algorithm IDs, and canonical derived annotations. F owns the **end-to-end integration harness** that proves the tracks compose, not just that each side passes its own gate. Without that harness, both tracks can be green while the visible serialized score is broken at the seam.

The architecture is frozen; Phase 2 is build-out and ratification, not redesign.

## Before you start

Every Phase 2 agent must:

1. Read `spec/QUICKSTART.md` (the v0 dispatch) for context on what already exists. The architectural decisions in it are not up for debate; they are the substrate Phase 2 builds on.
2. Read the spec chapters that map to your agent (see assignments below). Pass 11's revision will land in the same document; you are responsible for following its updates.
3. Read the relevant `crates/*/DECISIONS.md` files in your scope. Every implementation call made in v0 is recorded there. If a Phase 2 decision contradicts a v0 decision, that's a discussion to have explicitly, not an override to assume.
4. **Treat the spec as the contract.** Same rule as v0. If a behavior is ratified after Pass 11, implement it. If it's not — flag it for Pass 12, don't improvise.
5. **Treat F's harness for your scope as your merge gate.** Same rule as v0. The harness is the tripwire that catches what review and intuition miss.

## Crate topology (additions)

```
epiphany/
├── crates/
│   ├── epiphany-determinism/     # unchanged
│   ├── epiphany-core/            # grows under H (+ spelling, + decomposition)
│   ├── epiphany-ops/             # grows under K (payloads replace projections)
│   ├── epiphany-bundle/          # grows under J (codec replacements)
│   ├── epiphany-layout-ir/       # mostly stable; minor changes for I
│   ├── epiphany-engrave/         # NEW (I): real constraint solver
│   ├── epiphany-render-svg/      # NEW (I): SVG renderer behind RenderIR
│   └── epiphany-testkit/         # grows under F (more harnesses, benches)
└── spec/
    ├── core_spec.{tex,pdf}              # Pass 11 revision (G)
    ├── operation_catalog.{tex,pdf}      # NEW (K)
    └── binary_format.{tex,pdf}          # NEW (J)
```

Two new implementation crates (`epiphany-engrave`, `epiphany-render-svg`) and two new companion specs. The `engrave` crate is separate from `layout-ir` deliberately: the v0 quickstart established that `layout-ir` is the *interface* layer (the contract between graph and renderer), and the spec's core/product boundary puts the actual constraint-solving on the product side. Replacing the `StubSolver` *inside* `layout-ir` would blur that boundary; a new crate keeps it clean. Likewise `render-svg` is one renderer behind the existing `RenderIR` interface; other backends (PDF, MusicXML round-trip, MIDI) can follow the same pattern without disturbing it.

## Agent assignments

Six agents in active roles. Sequenced by dependency below; see Sequencing for parallelism.

### Agent G — Pass 11 Ratification

Owns: `spec/PASS11_WORKLIST.md` and the resulting spec revision.

Already running. The worklist is 25 items in three buckets (8 adopt-and-pin, 6 decide-then-pin, 5 fix, 6 no-spec-change). Bucket 1 in id-dependency order first, because `TypedObjectId` discriminants transitively gate most other byte layouts. Bucket 3 spec fixes early (blob hashing contradiction is a real spec bug). Bucket 2 judgment calls last.

Deliverables:
- A Pass 11 revision of `spec/core_spec.pdf` (annotated in the revision history table the same way Passes 1–10 are).
- A **single byte-convention table** consolidated from Bucket 1 — every ratified discriminant, derivation preimage, and encoding layout in one place, in a format J can import directly into the Binary Format companion. Living in an appendix of the core spec, referenced by section in the companion. Avoids three reference points becoming four.
- A **ratification log** with one line per worklist item, classifying each as:
  - *adopted as-is* (code's choice ratified verbatim),
  - *modified before ratification* (code's choice changed; specify what and why),
  - *deferred to companion* (handled in Operation Catalog or Binary Format),
  - *deferred to Pass 12* (not ready for ratification this round),
  - *rejected* (decided against the implementation's call; specify alternative).
  
  This makes later archaeology — "why does the spec say X?" — straightforward.
- Annotations on the golden-bytes tests in the affected crates citing the ratified spec section.

**Boundary discipline:** Pass 11 ratifies what exists, fixes what contradicts, and blesses the convention baseline that Track B's companions will inherit. It does **not** write the Binary Format companion or the Operation Catalog (those are J and K). If the worklist tempts you to write companion text, that's scope creep — stop and bring it back to the agent topology.

Coordinates with H on item 2.1 (tempo-linear semantic call) since H may have a preference based on the spelling algorithm's needs; with K on items 1.2, 1.3, 1.4 (the derivations K's reducer consumes); with J on item 1.8 (the convention baseline J inherits, and the byte-convention table J imports); with I on item 2.6 (the layout-object id derivation; this one defers to I if it's not blocking).

Hand off when: spec rebuilds clean, byte-convention table is complete and self-contained, ratification log complete with dispositions, golden tests annotated. Pass 11 is timeboxed — target 2–4 weeks. If it's running longer, the bucketing was wrong; renegotiate scope rather than letting it drag.

### Agent H — Spelling + Decomposition pre-passes

Depends on G's Bucket 2 item 2.1 (tempo-linear semantic) and the Pass 11 ratification of system-derived pitch id (Bucket 1 item 1.3). Lives in `epiphany-core`.

Owns: the spelling pre-pass (currently `SpellingAlgorithmId("default")` returning a trivial spelling) and the notational decomposition pre-pass (the data model and invariants exist; the algorithm doesn't).

#### Canonical model: derived annotations, not stored objects

Pre-pass outputs are **canonical derived annotations**: deterministic functions of `(materialized score graph, profile, SpellingAlgorithmId, DecompositionAlgorithmId)`, recomputed on materialization. They are *not* author-minted graph objects with operation envelopes. Three consequences fall out of this choice:

- **Manual overrides layer above generated output via ordinary operations.** A `RespellPitch` operation produces a canonical user spelling that takes precedence over the algorithm's default. The default is derived; the override is authored. This is why `RespellPitch` exists as a distinct operation — H formalizes the precedence rule, not the model.
- **Algorithm version is part of the derivation key.** `SpellingAlgorithmId::Default` and `DecompositionAlgorithmId::Default` are versioned. A profile-declared change to either invalidates derived annotations for all replicas observing the new profile, deterministically. No migration of stored state is needed because annotations are not stored canonical state.
- **Caching is permitted; canonical identity is not.** Implementations may cache derived annotations in acceleration snapshots or non-canonical chunks; the cache must invalidate when the derivation key changes. Two replicas at the same `(graph, profile, algorithm version)` produce byte-identical annotations whether or not either cached.

This model is what makes the pre-passes safe to add to the architecture without reopening Chapter 6. The reducer doesn't run H's algorithms; materialization does, deterministically, after reduction completes. The algorithms are pure functions over a fully-reduced score.

#### Spelling

Implement a real algorithm. Recommend a Temperley-style preference-rule system over `cmn-12` scale positions, because it's the best-documented choice with the cleanest constraint formulation. Longuet-Higgins line-of-fifths is the other option in the spec; pick one and have G ratify the choice as `SpellingAlgorithmId::Default` v1 in Pass 11 (or Pass 12 if the call slips).

Scope discipline for the Phase 2 implementation:
- target `cmn-12` only;
- monophonic and basic polyphonic cases;
- key-signature/context-aware spelling;
- deterministic output;
- manual `RespellPitch` overrides take precedence over generated spellings (precedence rule formalized in the spec via the existing `SpellingPrecedence` machinery).

Do **not** require the first implementation to solve every chromatic tonal edge case beautifully. The Phase 2 bar is *canonical and plausible*, not *musicologically perfect*. Hard cases (modulating sequences, chromatic mediants, enharmonic respellings under pitch-class-set analysis) can be Pass-12 candidates.

#### Decomposition

Split sounding durations into notehead values + augmentation dots + ties. Phase 2 scope, in priority order:
- metric regions (proportional and aleatoric defer);
- non-tuplet durations first;
- tuplets second;
- barline crossing;
- ties across the above;
- simple augmentation dots (double-dotted and beyond may defer).

The algorithm is mostly mechanical (largest-power-of-two-that-fits with dot extensions, recursing across barlines and tuplet boundaries), but the edge cases — syncopated patterns, ties across tempo changes, tuplet nesting — are where the difficulty lives.

#### What H produces, by event kind

The acceptance criterion needs to respect the actual taxonomy, not blanket every event:

- every `IdentifiedPitch` inside every pitched event gets a resolved `PitchSpelling`, *unless* its pitch space declares spelling unavailable;
- every metric-region event with a determinate musical duration gets a `Decomposition`;
- rests get decomposition but no pitch spelling;
- unpitched events get percussion/staff-position spelling per their pitch space's rules, not pitch spelling;
- trajectory, graphic, and indeterminate events: no decomposition or a region-specific decomposition per the spec;
- proportional and aleatoric regions: explicitly deferred unless the algorithm version claims support, in which case H states the bound.

The harness must classify and count these cases; "every event has a spelling" was wrong shorthand on my part.

#### What H does not do

Does not implement the Chapter 4 tuning catalog. Spelling works in scale-position terms, not frequency terms. The catalog is genuinely separate work and stays deferred. If spelling needs *anything* tuning-related, write to the existing `PitchSpaceId` / `TuningSystemId` interfaces and let the catalog's eventual implementation honor them.

Hand off when: F's representative score corpus exercises the kind-by-kind eligibility taxonomy with documented counts per kind; every *eligible* `IdentifiedPitch` carries a non-trivial `PitchSpelling`; every *eligible* determinate metric duration carries a `Decomposition`; ineligible cases are explicitly classified and counted in the harness output (not silently absent); both pre-passes are deterministic across runs given the same `(graph, profile, algorithm version)`; manual `RespellPitch` overrides take precedence over generated output; criterion 5's reducer-determinism gate continues to pass with non-trivial pre-pass outputs in the materialization pipeline.

Spec sections: Chapter 2 (pitch + spelling), Chapter 3 §"Decomposition." Cross-checks: §6.5 re-anchoring (spelling changes via `RespellPitch` must not break re-anchoring); the spec's open question on automatic spelling under aleatoric regions (defer to Pass 12 if H finds the algorithm doesn't generalize cleanly).

### Agent I — Visible Engraving (Solver + Renderer)

Depends on G's Bucket 1 items 1.7 and 1.8 (the codec conventions, because the resolved layout's canonical bytes inherit them) and on H's spelling output (because notes don't render without spellings). Lives in two new crates: `epiphany-engrave` (the real constraint solver) and `epiphany-render-svg` (the SVG output backend).

Owns: turning a `ConstrainedLayoutIR` into a `ResolvedLayoutIR` with real geometry (positions, spacing, beam angles, accidental placement) and then into SVG that visually resembles standard music notation. This is the visible-slice deliverable: from a `Score` graph, produce an image a musician would recognize.

#### `epiphany-engrave`

Implements `LayoutSolver` returning `SolverTier::Minimal` (the lowest real conformance tier — not `Stub`). Must satisfy every hard constraint emitted in the `ConstrainedLayoutIR`; should honor quality metrics enough to produce readable output (consistent spacing, sensible beam grouping, no egregious whitespace). Quality is not yet `Standard` tier — that needs the full Quality Metric Catalog (a Phase 3 companion). `Minimal` means: hard constraints satisfied, no claim about optimality.

Recommended approach: a two-pass spring layout (horizontal then vertical), with the constraint graph derived from the existing `ConstrainedLayoutIR`. Don't attempt a global optimization solver in v0; cast-off and line-breaking are separately hard problems. Start with single-line layouts and add line breaks later.

**Constraint-validation rule (matters for the harness):** validation runs against the *declared hard constraints in the IR*, not against generic geometric heuristics. Some notehead arrangements are intentionally close or horizontally displaced inside chords; the relevant rule is "no constraint declared by `ConstrainedLayoutIR` is violated," not "no bounding boxes touch." F's harness can add class-specific collision rules on top (accidental-vs-notehead, stem-vs-beam, staff-line-vs-glyph) but the agent's responsibility is constraint satisfaction, not collision intuition.

#### `epiphany-render-svg`

Takes a `ResolvedLayoutIR` and emits SVG. Uses the SMuFL glyphs from the bundled Bravura metrics (the catalog is already wired in v0).

**The non-overreach rule:** the renderer must not make *engraving-semantic* decisions. It will necessarily make rendering decisions (SVG grouping, path vs. text encoding, transforms, viewBox, layering, style representation, font fallback strategy) — those are SVG-encoding choices, not engraving choices. The line:

- The renderer **may** choose SVG encoding details: how to group elements, when to use `<path>` vs `<text>`, transform decomposition, namespace handling, style placement, viewBox bounds, layer ordering for stacking.
- The renderer **must not** choose: stem direction, spacing, beam slope, accidental placement, semantic glyph selection (e.g., which notehead shape for a duration), articulation positioning, clef choice.

Every rendered SVG element must trace to a `RenderIR`/`ResolvedLayoutIR` object or a declared renderer wrapper (e.g., an `<svg>` root, a `<defs>` block, a layer `<g>` grouping elements from the same IR layer). If the renderer finds itself making an engraving decision, that's a layout-IR bug — surface it via a diagnostic, don't paper over it.

**Font availability — decide deliberately.** SVG that references SMuFL glyphs only works if the viewer has the font. Three options:
1. Embed Bravura via `@font-face` with the font payload base64-encoded in the SVG.
2. Convert all glyph references to inline `<path>` outlines (no font dependency in the viewer).
3. Require local Bravura installation and document it.

Recommendation: **inline path outlines for golden fixtures and the demonstrable deliverable**, embedded font as an optional rendering mode. Path outlines maximize portability and make the SVG self-contained (it renders in any browser, in any image-processing tool, in print pipelines), at the cost of larger file size. The font-embedded mode is a configuration option that ships but isn't the default.

#### Demo binary discipline

The visible slice deliverable is a library. But you almost certainly want a small `examples/render_fixture.rs` (or similar) in `epiphany-render-svg` that takes a fixture name on the command line and emits SVG to stdout. This is not an application; it's a demo harness. It will be invaluable for showing the work, for visual regression review, and for triage. Ship it.

#### Development pattern

Develop the renderer against stub-solver output first (so it's testable before the solver lands), then switch to the real solver. Keep the renderer working against *both* the stub and the real solver — that lets you bisect "is this a renderer bug or a solver bug" with a one-line change. The demo binary's CLI can take a `--solver=stub|real` flag.

#### What I doesn't do

Does not implement the Chapter 9 Quality Metric Catalog. The solver reports `SolverTier::Minimal`, which means it satisfies hard constraints but makes no normalized-metric claims. Quality metrics are Phase 3.

Hand off when: F's `ten_measure_single_staff` and `valid_score_rich` fixtures both render to SVG that a music reader recognizes as standard notation (human review gate, performed by you personally — see Acceptance criteria); the machine-readable acceptance snapshot for each fixture (object count, glyph count, bounding-box classes, provenance count, hard-constraint count, XML validity) is golden-locked; resolved layouts satisfy every declared hard constraint plus F's class-specific collision rules; criterion 6's layout round-trip continues to pass with the real solver replacing the stub; renderer output is well-formed SVG (XML-validates); the demo binary works end-to-end from a fixture name.

Spec sections: Chapter 7 (Layout IR) — already implemented at the interface level; Chapter 9 §"Solver Interface" — implement the `Minimal` tier; Chapter 7 §"Glyph Catalog" for the SMuFL integration.

### Agent K — Operation Catalog + Real Payloads

Depends on G's Bucket 1 items 1.1, 1.2, 1.4, 1.7, 1.8 (the byte conventions K's payloads encode under). Lives in `epiphany-ops` (real payloads replacing identifier projections) and `spec/operation_catalog.{tex,pdf}` (the new companion spec).

Owns: the Operation Catalog companion specification *and* the corresponding shift in `epiphany-ops` from identifier-only payload projections (today's prototype) to value-typed payloads. The catalog is the schema; the ops crate consumes it.

The current state (P11-C1): `InsertEventOp` carries `event: EventId` plus reduction-relevant scalars, not the full `Event`. `RespellPitchOp` carries the pitch id and a `ContentHash` fingerprint of the new spelling, not the spelling itself. This was the right call for v0 (it made envelopes hashable without a value-codec dependency) but it's not durable: any operation re-played in a fresh context (a backup restore, a cross-tool round-trip) needs the full value.

#### K0 / K1 split

Writing all 60–80 primitives as a Phase 2 close condition would turn Phase 2 into a catalog-writing marathon. Split deliberately:

**K0 — Minimum portable catalog (required for Phase 2 close):**
- create score / canvas / region / staff / staff instance / voice;
- insert / delete / modify event;
- insert / delete / modify identified pitch;
- respell pitch;
- set metadata (title, composer, lyricist, copyright);
- set metric grid / time signature / tempo segment;
- create / delete / update tie / slur / beam / spanner;
- set layout / system break advisory;
- resolve conflict;
- undo transaction descriptor payload.

These are the primitives the visible slice and the binary format actually need. K0's schemas must be complete and implemented in `epiphany-ops`.

**K1 — Full catalog expansion (drafted in Phase 2, completed in Phase 3):**
Everything else from the spec's 60–80 estimate. K1's *framework* must exist in Phase 2: schema template, undo-rule template, conflict-case template, re-anchoring-behavior template. Adding a K1 primitive in Phase 3 should be schema-fill, not design. But the K1 primitives themselves need only be drafted (one-paragraph descriptions and slot-in-the-framework) in Phase 2, not fully specified.

This isn't about cutting scope arbitrarily; it's about staging. The full catalog is genuinely Phase 3-sized work that doesn't gate Phase 2's deliverables.

#### Per-primitive schema content (K0)

For each K0 primitive, define:
- The complete payload schema (what value-typed fields it carries).
- The canonical byte encoding (consuming Pass 11's convention baseline from item 1.8).
- The reduction rule (the existing `epiphany-ops` reducer logic ratified against the schema).
- The conflict cases and how the rule resolves each.
- The undo semantics under `StrictInverse` / `BestEffort` / `Cascade`. K0 primitives must have undo semantics specified and implemented; K1 primitives need undo only drafted.
- The re-anchoring behavior on tombstoned referents.

Pass-11 item 2.5's `ResolveConflict::Dismiss` action lands here in the `ResolveConflict` primitive's schema.

#### v0 → v1 payload migration

The backward-compatibility requirement is correct but needs a mechanism. v0 envelopes carry identifier-only payloads; v1 envelopes carry value-typed payloads. Two options:

1. *Parallel variants forever.* `OperationPayload::V0Projection(...)` and `OperationPayload::V1ValueTyped(...)` coexist permanently. Reducer handles both. Pro: no migration. Con: doubles reducer surface area forever; v0's identifier-only shape becomes a permanent dialect.
2. *One-time migration.* K ships a migration function that converts v0 envelopes to v1-shaped envelopes using the score graph as context to reconstruct value payloads. The migration runs once on read; v0 envelopes are absent from production code afterward.

Adopt option 2. v0 envelopes live only in the test corpus as a regression guard (proving the migration is correct). Production code carries only v1 payloads.

Migration mechanism:
```rust
// In epiphany-ops, applied on cold open of a v0 bundle
fn migrate_v0_envelope(
    v0: V0OperationEnvelope,
    context: &Score,  // the base or partially-materialized graph
) -> Result<OperationEnvelope, MigrationError>;
```

The migration must be *deterministic* (two implementations migrating the same v0 envelope against the same context produce byte-identical v1 envelopes) and *equivalence-preserving* (a v0 envelope and its v1 migration reduce to identical canonical state). The criterion-1 convergence harness in F's testkit validates this: it runs the v0 corpus through migration and asserts byte-identical reduction outcome.

If migration is impossible for some v0 envelope (the context lacks information to reconstruct the value), K declares the v0 envelope incompatible and the bundle opens read-only. Document the cases where this happens; ideally there are none, but Phase 2's actual v0 corpus will tell.

#### Boundary discipline

The catalog defines schemas and reduction rules. It does **not** redefine the architecture of `epiphany-ops` — the reducer, the `OperationSlot`, the canonical reduction order, the equivocation handling, the integrity-anomaly model are all v0 deliverables that stay frozen. K consumes them by giving them real payloads to operate on, not by rewriting them.

#### Coordination with J

K's payload schemas are J's input for the operation-payload sections of the Binary Format companion. They design jointly (joint discussions, joint reviews) but K0's schemas land before J implements those sections. J's other surface area (identifiers, scalars, manifest, header, chunk preludes, schema-evolution rules, content-hash preimages, canonical container encodings) is K-independent and J starts on those immediately after G's Bucket 1.

Hand off when: every K0 primitive has a complete schema; `epiphany-ops` payload types match K0's schemas; v0 envelope corpus migrates to v1 deterministically and the migration is equivalence-preserving (F harness validates); v0 envelopes reduce to byte-identical canonical state through the migration path; criterion 5 continues to pass at 10K-envelope scale; F's performance bench passes the documented budget for that scale; K1 catalog framework exists with placeholder entries for each Phase-3 primitive.

Spec sections: Chapter 6 (existing), Chapter 5 (the typed-object family the payloads reference), the new Operation Catalog companion.

### Agent J — Binary Format Companion + Codec Replacement

Depends on G's Bucket 1 items 1.5–1.8 (the discriminant tables and convention baseline J formalizes). **Most of J's surface area does not depend on K** — start immediately after G's Bucket 1 lands. Only the operation-payload sections of the companion wait for K0's schemas. Lives in `spec/binary_format.{tex,pdf}` (the new companion) and across `epiphany-core`, `epiphany-ops`, and `epiphany-bundle` (codec replacements).

Owns: the Binary Format companion specification *and* the corresponding replacement of the three crates' prototype codecs with companion-conforming implementations. The companion is the schema; the three crates consume it.

The current state (P11-4, P11-D2, P11-C provisional): each of `epiphany-core`, `epiphany-ops`, `epiphany-bundle` has its own codec module (`codec.rs`, `encode.rs`/`decode.rs`, `manifest.rs`'s encoders) that produces and consumes canonical bytes for its types. These codecs share conventions (little-endian, u32 length prefixes, single-byte discriminants, etc.) but each was independently authored, and the conventions are documented in DECISIONS files rather than a single normative source.

#### What J can start on immediately (K-independent)

- Identifier encodings (all the typed-id family ratified by Pass 11 item 1.1).
- Primitive value encodings: `RationalTime`, scalar wall-clock, `QuantizedCoord`, `ContentHash`, `ChunkId`, string encoding (NFC rules), boolean, integer endianness.
- Composite graph value encodings: `Event`, `Voice`, `Region`, `StaffInstance`, `Pitch`, `IdentifiedPitch`, `PitchSpelling` and (once H lands) `Decomposition`. These are core's value types, not ops' payload types.
- Manifest encoding (header / superblock / chunk preludes / manifest body).
- Schema evolution rules: how minor versions add fields, how major versions are gated, how unknown fields are handled.
- Canonical container encodings: `CanonicalMap`, `CanonicalSet`, `CanonicalVec`. The ordering rules already exist in `epiphany-determinism`; J formalizes the wire layout.
- Content-hash preimage rules (mostly in Chapter 8 §"Domain-Separated Preimages" already; J makes it complete and unambiguous).

This is the bulk of the companion. Get it ratified while K is still drafting K0 schemas.

#### What J waits for K on

- Operation payload encoding for K0 primitives — J encodes them after K0's schemas exist.
- Operation payload encoding for K1 — drafted in Phase 2, completed in Phase 3 alongside K1.

These are the only sections gated on K.

#### Codec replacement

- Replace each crate's bespoke encoder/decoder with implementations that read from a single shared spec.
- Preserve byte-for-byte compatibility with v0's outputs *for the conventions Pass 11 ratifies* — this is the cleanest way to validate the companion: every existing golden test must continue to pass.
- Migrate any encoding that Pass 11 decided to *change* in the same commit as updating the goldens; document each migration in the companion's revision history.

#### Required harnesses (J-specific, beyond F's standard ones)

The v0 testkit could not test these because everything was one implementation talking to itself. J's work crosses an implementation boundary (the spec is now the contract, not the code), so the harness surface broadens:

- **Cross-implementation decoder test.** An isolated decoder, implemented from the companion spec text *without referencing the codec code*, reads the encoder's output and reproduces input. This is the test that proves the spec is self-sufficient. Live in F's testkit, written by F based on the companion text.

- **Wire-format fuzzer (early).** Random valid values round-trip; random invalid bytes fail cleanly with typed errors; no panics; malformed lengths/discriminants handled safely; bounded memory consumption on adversarial input. Adopt `cargo-fuzz` or equivalent. Run the fuzzer in CI's nightly soak. This is a cheap, high-value addition that catches the kinds of format bugs that don't surface in property tests.

- **Canonicalization tests.** Map iteration ordering matches Pass 11's canonical-container rules; UTF-8 NFC enforced at boundaries (not silently accepted as raw bytes); float `-0.0` normalization to `+0.0`; rational always reduced (no `2/4` round-tripping as `2/4`); unknown fields under minor schema evolution preserved opaquely; bytes outside the canonical alphabet rejected.

#### Boundary discipline

J writes the canonical wire format. J does **not** invent new payload schemas (those are K's) or new graph types (those are core's), and does **not** redesign the bundle's structural layer (the fixed header, superblocks, chunk graph, manifest are v0 deliverables). J's job is to formalize how the existing types serialize.

Hand off when: the Binary Format companion exists as a versioned document covering all K-independent sections plus all K0 operation payloads; the three crates' codec modules cite the companion as their normative source; no codec module contains private byte-layout decisions — every layout is in the companion; criterion 4 (canonical serialization stability) continues to pass byte-for-byte on the v0 corpus, the Pass 11 corpus, and the K0 envelope corpus; the cross-implementation decoder test passes; the wire-format fuzzer runs in CI without panics on 1M iterations.

Spec sections: Appendix D §"Canonical Serialization," Chapter 8 §"Domain-Separated Preimages," and the new Binary Format companion as a whole.

### Agent F — Testkit & Tripwire (continues, mandate broadens)

Continues v0 ownership and adds:

- **Per-agent harnesses.** Each new agent (H, I, K, J) gets a harness that's their merge gate. Same rule as v0. Design each harness for its agent's specific failure modes:
  - H: spelling and decomposition stability across runs at the kind-by-kind eligibility taxonomy; manual `RespellPitch` precedence over generated output; non-vacuity (the pre-passes actually change the score in expected ways).
  - I: hard-constraint validation on resolved layouts (against declared constraints, not generic bounding-box rules) plus class-specific collision rules (accidental-vs-notehead, stem-vs-beam, staff-line-vs-glyph); provenance survival through solver and renderer; SVG well-formedness (XML-validates); machine-readable acceptance snapshot per fixture (object count, glyph count, bounding-box classes, provenance count, hard-constraint count, XML validity) golden-locked.
  - K: deterministic and equivalence-preserving v0→v1 migration (v0 envelopes migrated to v1 reduce to byte-identical canonical state as v0 envelopes did); payload-schema completeness (every K0 catalog primitive has a payload type in `epiphany-ops`); K1 framework present.
  - J: cross-implementation decoder test (isolated decoder reads encoder output and reproduces input); criterion 4 continues to pass byte-for-byte; wire-format fuzzer (1M iterations no panics, no unbounded memory); canonicalization tests (map ordering, NFC, `-0.0` normalization, rational normalization, minor-schema-evolution unknown-field preservation).

- **End-to-end integration harness.** This is the harness that proves the two tracks actually compose. A single fixture run:
  ```
  Score fixture
    → canonical reduction
    → H pre-passes (spelling + decomposition)
    → layout IR
    → I solver (real, not stub)
    → SVG render
    → J bundle write
    → J bundle read
    → canonical reduction again
    → H pre-passes again
    → layout again
    → SVG render again
  ```
  Expected: canonical state and SVG are byte-identical between the first and second pass, modulo explicitly allowed non-canonical caches. Without this harness, H and I and J and K can all be green while the composed result silently breaks at the seams. F owns this harness; it lands before either track declares done.

- **Performance benchmark suite with documented thresholds.** Phase 2 creates the first scores with realistic event counts (Track A's visible-slice work). The v0 reducer's `canonical_reduction_order` is `O(n²)` in its indegree construction — fine for criterion 5's 1000 envelopes (~1M coverage checks), painful at 10K+. F adds a `benches/` directory (using `criterion`) that asserts performance budgets from Chapter 10 of the spec. **Set the threshold first, in the bench itself**, as an expected-failing (`#[xfail]`-equivalent) gate for known scale points. When the bench fails at a documented score size, the responsible agent (likely K, working in `epiphany-ops`) fixes it with a regression test. F surfaces; doesn't fix.

- **Pass 12 batch tracker.** Same batching rule from v0: ambiguities discovered during Phase 2 implementation go into a Pass 12 batch, not into code improvisations. F maintains the tracking list. Don't open Pass 12 until at least 3 items accumulate.

- **CI broadening.** The existing CI (fmt, clippy -D warnings, workspace tests, doc tests, conformance suite, nightly soak) gets per-agent jobs added as their harnesses land, plus the integration harness, plus the performance bench job, plus the wire-format fuzzer in the nightly soak. The conformance suite stays single-job to keep it the architecture's headline tripwire.

What F **doesn't** do: write any of the new crates' production code. F's role is to gate quality, not to implement features. If a harness reveals a problem, F files it against the responsible agent; F doesn't fix it.

Hand off (per agent's merge): F's harness for that agent runs in CI and passes; the harness asserts the agent's stated acceptance criterion (see "Acceptance criteria per agent" below); the harness has a non-vacuity guard (it would fail if the agent's work were stubbed or vacuous). For Phase 2 close: the integration harness runs end-to-end clean on the full fixture corpus.

## Sequencing

```
Week 0 (now):
  G starts Pass 11 (Bucket 1 first, ~2–3 weeks for the byte items).
  F broadens mandate; drafts per-agent harness templates;
    sketches the integration harness skeleton against stub stages.
  H designs algorithm and fixtures.
  I starts renderer against stub-solver RenderIR immediately.
  K starts K0 catalog schema design.
  J starts reading; cannot begin codec implementation until G's Bucket 1.

Week ~3 (G's Bucket 1 lands):
  H starts implementation (algorithm-choice ratification from G's Bucket 2).
  I starts engrave-solver implementation (renderer already in progress).
  K starts K0 payload implementation.
  J starts codec replacement for K-independent surface:
    identifiers, scalars, composites, manifest, headers, schema evolution,
    canonical container encodings, content-hash preimages. This is the
    majority of J's work and it begins now.

Week ~6 (G done):
  Pass 11 closed; byte-convention table delivered; all goldens annotated.
  H, I, K running in parallel.
  J continues K-independent codec work in parallel.
  F's per-agent harnesses live in CI; integration harness skeleton wired
    with real H+I+K stages as they land.

Week ~10:
  K0 catalog draft circulates.
  J starts operation-payload sections of the companion, paired with K.
  H lands (spelling + decomposition complete and harnessed).
  I's solver lands; I's renderer can now use real solver output.
  Integration harness exercises real H + real I + stub-K + stub-J path.

Week ~12:
  K0 lands. Operation Catalog companion ratified for K0 primitives;
    K1 framework drafted.
  I lands (visible slice demonstrable internally).
  J's K-independent codec work complete; only operation-payload encoding
    remains.
  Integration harness exercises full real H + real I + real K + stub-J.

Week ~15:
  J lands. Binary Format companion ratified for K0 surface.
  Integration harness fully real end-to-end.
  Phase 2 closed; foundation is durable + perceivable + ratified.

Week ~17:
  Pass 12 batch (if accumulated). Phase 3 planning.
```

This is calendar pacing assuming roughly one engineer per agent and that you're not running into the kinds of cross-track contention I'm not seeing from here. Adjust accordingly.

The critical path is G → (K0 schemas) → J operation-payload encoding. Track A (H, I) is parallelizable with both, and most of J's work (the K-independent surface) is parallelizable with K. The earlier sequencing had J fully blocked on K, which would have lost 4–6 weeks unnecessarily.

The biggest schedule risk is K0 turning out larger than the bullet list (some "K0 primitive" reveals subcategories that need their own schema work). If K0 swells, consider whether items belong in K1 instead — the principle is *what does the visible slice and binary format actually need*, not *what's spec-complete*.

## Decisions you'll need to make

Five next-phase calls, analogous to v0's five. Make each one once and document it.

1. **Spelling algorithm choice.** Temperley vs. Longuet-Higgins vs. a hybrid. H proposes; G ratifies via Pass 11 (or Pass 12). Recommendation: Temperley preference rules — best-documented, cleanest constraint formulation, easiest to test against published examples.

2. **Solver architecture for engraving.** Two-pass spring layout (recommended; matches the existing `ConstrainedLayoutIR` shape) vs. global optimization (cleaner output, much harder to make deterministic) vs. rule-based fallback (fast, brittle). Recommendation: two-pass spring. The spec's deterministic-output requirement makes global optimization expensive to validate. Document the choice in `epiphany-engrave`'s README.

3. **Renderer SVG dialect.** Pure SVG 1.1 (broadest compatibility) vs. SVG 2 (richer features) vs. SVG + CSS for styling (cleaner separation, harder to embed). Recommendation: SVG 1.1 + inline styles. Maximum portability for what's effectively a viewer artifact; CSS comes later if needed.

4. **Catalog versioning shape.** K writes the Operation Catalog companion. Recommendation: independent semver, separately versioned from `core_spec.pdf`. Operation kinds get added over time; the catalog should evolve without bumping the core spec.

5. **Binary Format companion versioning shape.** Same question for J. Recommendation: independent semver again, but tied tighter to `core_spec.pdf` than the Operation Catalog — the binary format is fundamentally about *how the spec's types serialize,* so it travels with the spec more closely. A reasonable rule: binary format major version matches core spec major version; minor versions diverge.

## Don't do these

Updated from v0 in light of Phase 2:

- **Don't reopen v0 design decisions.** The architecture is frozen — same rule as v0, restated because Phase 2 agents will be tempted to "improve" things they touch. If you have a real reason to revisit a Pass 1–10 decision, raise it as a Pass 12 candidate; do not unilaterally change it.
- **Don't implement the Chapter 4 tuning catalog.** It's deferred again. Spelling works in scale-position terms; rendering works in SMuFL glyph metrics; nothing in Phase 2 needs frequency resolution. Phase 3.
- **Don't implement quality metrics (Chapter 9 Catalog).** The solver reports `SolverTier::Minimal`; that's the right tier for the visible slice. Quality metrics need a Quality Metric Catalog companion and a Reference Suite — Phase 3.
- **Don't preemptively optimize performance.** F's benches set thresholds for known scale points up front; the fix happens when the bench fails at a documented size with a regression test attached. Don't rewrite the `O(n²)` reducer because someone is uncomfortable with it. Bench first, optimize only when the bench says so.
- **Don't implement bundle compression (zstd) yet.** The manifest must remain uncompressed (a Pass 11 ratification); other chunks could be compressed but the prototype baseline doesn't, and the visible slice doesn't need it. Phase 3.
- **Don't implement async wrappers, plugin runtime, UI, audio engine.** Same as v0; these remain explicitly out of scope.
- **Don't ship an application.** Phase 2 produces libraries plus *demo harnesses*. A `cargo run -p epiphany-render-svg --example render_fixture ten_measure_single_staff > out.svg` is a demo harness, not an application — and you almost certainly want it. It's invaluable for showing the work, for visual regression review, and for triage. Ship demo binaries; don't ship a viewer process or a GUI host.
- **Don't write K1 catalog primitives in Phase 2.** Draft them in the framework K provides; full implementation is Phase 3. Trying to ship the full 60–80 primitives is what turns Phase 2 into a marathon.

## Acceptance criteria per agent

You'll know each agent's Phase 2 work is done when these tests pass on F's harness. Each is the analog of the v0 quickstart's six criteria, scaled to its agent.

- **G — Pass 11 ratification:** spec rebuilds clean (XeLaTeX three-pass, no warnings, no undefined references); byte-convention table delivered as a self-contained spec section that J can cite verbatim; ratification log present with one line per worklist item classified as adopted-as-is / modified-before-ratification / deferred-to-companion / deferred-to-Pass-12 / rejected; all golden-bytes tests in `epiphany-core`, `epiphany-ops`, `epiphany-bundle` annotated to cite ratified spec sections; no test fails as a result of ratification.

- **H — Spelling + Decomposition:** on F's representative score corpus (>20 fixtures spanning common cases, edge cases, and torture cases), every *eligible* `IdentifiedPitch` carries a non-trivial `PitchSpelling` (eligibility per the kind-by-kind taxonomy above); every *eligible* determinate metric duration carries a `Decomposition`; ineligible cases are explicitly classified and counted in the harness output, not silently absent; the same score reduced twice produces byte-identical pre-pass annotations (deterministic-derivation property); manual `RespellPitch` overrides take precedence over generated spellings; criterion 5's reducer-determinism gate continues to pass with non-trivial pre-pass outputs in the materialization pipeline; spelling matches published Temperley/Longuet-Higgins expectations on a curated set of standard test cases (Bach chorale phrases, Beethoven motifs).

- **I — Visible engraving:** F's `ten_measure_single_staff` and `valid_score_rich` fixtures both render to SVG; **human review gate** (performed personally by the project lead — first-time-seeing-Epiphany-render-real-music is a meaningful moment and the visual bar matters): the SVG visually parses as standard music notation; **machine acceptance snapshot per fixture** (object count, glyph count, bounding-box class counts, provenance count, hard-constraint count, XML validity) golden-locked, changes require explicit golden update; resolved layouts satisfy every declared hard constraint plus F's class-specific collision rules (accidental-vs-notehead, stem-vs-beam, staff-line-vs-glyph); criterion 6's layout round-trip continues to pass with the real solver replacing the stub; renderer output is well-formed SVG (XML-validates); the demo binary works end-to-end from a fixture name on the command line.

- **K — Operation Catalog:** K0 primitives complete and implemented — every K0 primitive has a complete schema in `operation_catalog.pdf`, a matching payload type in `epiphany-ops`, complete undo semantics, and a reduction rule consistent with the v0 reducer; K1 catalog *framework* exists with one-paragraph descriptions and template slots for each Phase-3 primitive (unimplemented K1 primitives explicitly marked unavailable under the Phase 2 profile); v0→v1 migration is deterministic and equivalence-preserving (v0 envelopes migrated to v1 reduce to byte-identical canonical state as v0 envelopes did under v0 payloads); criterion 5 continues to pass at 10K-envelope scale; F's performance bench passes the documented budget for that scale.

- **J — Binary Format companion:** companion spec rebuilds clean; covers all K-independent surface plus K0 operation payloads; the three crates' codec modules each cite the companion as normative; no codec module contains private byte-layout decisions; criterion 4 (canonical serialization stability) continues to pass byte-for-byte on the v0 corpus and the Pass 11 corpus and the K0 envelope corpus; cross-implementation decoder test passes; **wire-format fuzzer runs in CI nightly soak without panics or unbounded memory consumption on 1M iterations**; canonicalization tests pass (map ordering, NFC enforcement, `-0.0` normalization, rational normalization, minor-schema-evolution unknown-field preservation).

- **F — Tripwire continues:** every agent's harness lands in CI before the agent merges; **integration harness exists and runs end-to-end clean** (Score → reduction → H → layout → I → SVG → J write → J read → reduction → H → layout → SVG, with byte-identical results between passes modulo allowed non-canonical caches); performance bench job runs in CI with documented budgets and `xfail` thresholds for known-pending scale points; Pass 12 batch tracker exists; conformance suite continues to pass on every push; nightly soak continues clean and includes J's wire-format fuzzer.

## Process notes

**The spec is the contract.** Same rule as v0. The Pass 11 revision joins the canonical contract once G lands; the Operation Catalog and Binary Format companions become part of the contract once K and J land. Reading the contract is the first step before implementing anything.

**Ambiguities go into a Pass 12 batch, not into code.** Same rule as v0. F maintains the batch. Don't open Pass 12 until at least 3 items accumulate. The discipline from v0 → Pass 11 worked; trust it for Phase 2 → Pass 12.

**Architecture is frozen.** Restating because Phase 2 will tempt revisits. The v0 architecture survived nine review passes and one full prototype build. Implementation pressure has *not* revealed a structural problem with it. If Phase 2 implementation reveals a structural problem, that's news worth treating as news — but it should clear the same review-pass bar that Passes 1–10 cleared, not slip in as a sidecar revision.

**Treat F's harness as the merge gate.** Not the reviewer's intuition. Not the test you wrote yourself. F's harness, which exists specifically to fail when something subtle has gone wrong. If F's harness is green and you have a bad feeling, write a regression test for the bad feeling and watch the harness go red; that's how the harness gets sharper. Improvising past a green harness is how the architecture gets eroded.

**The two tracks coordinate through interfaces, but don't redesign each other.** Track A (H, I) and Track B (K, J) work on largely disjoint parts of the codebase. They *do* coordinate through shared types (core value types, canonical derived annotations, algorithm IDs) and shared conventions (Pass 11's byte-convention table). If you find yourself redesigning each other's surface — H proposing payload-schema changes, J telling I how to lay out IR objects, K reshaping the layout model — that's scope creep. Coordinate through the harnesses (especially F's integration harness, which is the contract that proves the tracks compose) and through the spec, not through ad-hoc changes to each other's code.

**Phase 2 closes when all five agents land + Pass 12 batch is reviewed + the integration harness runs end-to-end clean.** Not when "everything feels done." The acceptance criteria above are the close condition; meet them, then Phase 3 planning begins.
