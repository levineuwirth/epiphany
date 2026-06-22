# Pass 11 — Spec Ratification Worklist

*Purpose: convert the v0 implementation's provisional choices into ratified spec text, so the durable byte layouts are fixed before the next-phase build-outs (visible slice, interchange core) start writing real documents on top of them.*

*Scope: this is a **spec revision pass**, not an architecture pass. The architecture stays frozen. Every item below is either (a) a value the spec must pin, (b) a place the spec must show a field/derivation it implied but never wrote, or (c) a genuine spec-internal contradiction to fix. No item requires reopening a Pass 1–10 design decision.*

*Working rule (unchanged from QUICKSTART): the spec is the contract. Where this worklist says "adopt," the disposition is to bless the implementation's existing choice in normative spec text. Where it says "decide," there is a real fork the implementer should resolve and record. Where it says "fix," the spec text is self-contradictory or missing and must change regardless of the code.*

---

## How to use this document

The 25 items are grouped into three buckets by disposition difficulty:

- **Bucket 1 — Adopt-and-pin (mechanical).** The implementation made a deterministic, golden-locked choice; the spec just needs to write it down. Low judgment, high volume. An agent can draft all of these from the code directly. **Start here** — it unblocks the most downstream work for the least deliberation.
- **Bucket 2 — Decide-then-pin (judgment).** There is a real fork the spec left open; the implementation picked one branch but the choice has semantic consequences worth a deliberate call.
- **Bucket 3 — Fix (spec is wrong or silent).** The spec text contradicts itself or omits a required field. These change the spec independent of the code.

Each item carries: the source P11 id, the spec location to edit, the implementation's current choice (with the code anchor), a recommended disposition, and the **blast radius** (what downstream work is unblocked by ratifying it).

A golden-bytes test already locks every byte-layout item. **Ratification protocol per item:** (1) write the normative spec text; (2) if the spec adopts the code's layout, confirm the existing golden test is now the *spec's* golden, not just the crate's proposal — annotate the test to cite the ratified spec section; (3) if the spec overrides the code, update both the code and the golden in the same commit and add a migration note. Either way, no provisional byte layout survives this pass unannotated.

---

## Bucket 1 — Adopt-and-pin (mechanical, do first)

These are the items gating durable bytes. Every one is a discriminant table, derivation preimage, or encoding layout that already exists in code, is golden-locked, and just needs blessing. Ratifying this whole bucket is what converts "provisional encoding" to "stable format" — the precondition for the interchange track (J/K) and for any document a user saves and reopens later.

### 1.1 — `TypedObjectId` discriminant table `[P11-1]`
- **Spec edit:** Chapter 5 §"Identifier Family" / wherever `TypedObjectId::canonical_bytes` is defined. The spec fixes the *shape* (16-bit big-endian discriminant + variant payload) but the variant list ends "…and so on for every named object kind."
- **Code choice:** declaration-order discriminants `Event = 0 … AnalysisLayer = 21`, then the kinds beyond the spec's explicit list: `Tuplet = 22, RepeatStructure = 23, LyricLine = 24, ChordSymbol = 25, View = 26, Registered = 27`. `Registered(reg, raw)` encodes `discriminant(2) || reg.canonical_bytes(16) || raw_be(16)`. Locked by `typed_object_id_byte_form_is_locked`.
- **Disposition:** **Adopt.** The table is reasonable and complete against the current graph. Two confirmations needed: (a) the full object-kind set is closed at these 27 — confirm no graph object kind is missing; (b) `ObjectKindRegistryId` is a 128-bit value.
- **Blast radius:** every content hash, every canonical ordering, every equality over objects. This is the single highest-leverage ratification in the pass — most other byte layouts transitively reference object ids.

### 1.2 — Promoted-voice id derivation `[P11-3]`
- **Spec edit:** Chapter 5 §"System-Promoted Voices" / the semantic-operations companion's `derive_promoted_voice_id`.
- **Code choice:** 64-byte `MUSCSVCE` preimage = `staff_instance || original_voice || winning_op || losing_op`, each 16 big-endian bytes. Locked by `promoted_voice_id_byte_form_is_locked`. (Note the cross-crate dependency: this is **also** ops `P11-C4` — Agent C's reducer computes the same derivation Agent B's Invariant 18 verifies. Ratify once; both crates consume it.)
- **Disposition:** **Adopt.** Confirm field order and that staff-instance-from-containment is an acceptable recovery (the op does not store it redundantly).
- **Blast radius:** unblocks Invariant 18 verification and the ops promotion pre-pass from "provisional." Required before the Operation Catalog (K) fills in the real promotion payload.

### 1.3 — System-derived (synthetic) pitch id derivation `[P11-6]`
- **Spec edit:** Chapter 5 §"System-Derived Identifiers" (the `MUSCSPCH` tag is reserved but the derivation function is deferred).
- **Code choice:** `derive_system_pitch_id` content-addresses the pitch from a fixed canonical byte form of intrinsic identity (scale position + acoustic realization; strings length-prefixed and NFC-normalized at the derivation boundary). Locked by `system_pitch_id_byte_form_is_locked`.
- **Disposition:** **Adopt**, but **decide one sub-point**: the exact field set that constitutes "intrinsic identity." The code uses scale position + acoustic realization. Confirm that's the complete identity (e.g., does a synthetic pitch's identity include or exclude its tuning reference when tuning is `Inherit`?). This touches Invariant 11.
- **Blast radius:** Invariant 11 enforcement; any future synthetic-pitch minting.

### 1.4 — `IntegrityAnomalyId` derivation + anomaly domain tag `[P11-C2]`
- **Spec edit:** Chapter 5 (`IntegrityAnomaly` id) + the system-tag registry.
- **Code choice:** `derive_system_id(MUSCSANM, kind.canonical_bytes())` in the `SYSTEM_DERIVED` namespace, introducing a new `MUSCSANM` extension system tag.
- **Disposition:** **Adopt + decide:** confirm `MUSCSANM` should be a *built-in* reserved system tag alongside `MUSCSVCE`/`MUSCSPCH` (recommended — anomalies are core, not an extension), rather than an extension tag. If built-in, add it to the spec's closed tag vocabulary.
- **Blast radius:** cross-replica agreement on anomaly identity. Needed before equivocation/anomaly handling is part of any conformance claim.

### 1.5 — `ChunkKind` / `ProfileId` / `CompressionAlgorithm` discriminants `[P11-D4]`
- **Spec edit:** Chapter 8 §"Domain-Separated Preimages" — `ChunkKind::canonical_bytes()` is in the chunk hash preimage, so its discriminants are normative.
- **Code choice:** declaration-order single-byte discriminants: `ChunkKind` `OperationEnvelopeBlock = 0 … Manifest = 8`; `ProfileId` `0–3`; `CompressionAlgorithm` `0–2`.
- **Disposition:** **Adopt.** Same situation as 1.1 for the bundle layer.
- **Blast radius:** every chunk hash. `ChunkKind` in particular changes content addresses, so this gates the bundle format's stability.

### 1.6 — `ManifestId` derivation preimage `[P11-D5]`
- **Spec edit:** Chapter 8 §"The Manifest" (says "each commit produces a new `ManifestId`" + assigns `MUSCMNIF`, but gives no preimage).
- **Code choice:** `trunc128(BLAKE3("MUSCMNIF" || document_id || generation || manifest_body))`, where `manifest_body` is the canonical manifest encoding with the `manifest_id` field excluded (avoids self-reference).
- **Disposition:** **Adopt.** Confirm the self-reference exclusion is stated normatively (a conforming writer must zero/omit `manifest_id` when computing it).
- **Blast radius:** two conforming writers must derive identical manifest ids; gates multi-writer interop.

### 1.7 — `RationalTime` + scalar canonical encodings `[P11-4]`
- **Spec edit:** Appendix D / Chapter 8 — explicitly deferred to the Binary Format companion, but the primitive layouts can be ratified now independent of the full companion.
- **Code choice:** `RationalTime` = sign + length-prefixed big-endian numerator and denominator magnitudes, always reduced; wall-clock integers little-endian (matching `QuantizedCoord`).
- **Disposition:** **Adopt the primitive layouts now**, leave the *composite* whole-Score codec (1.8) flagged for the companion. The primitives are stable, small, and referenced everywhere; pinning them de-risks the companion.
- **Blast radius:** the foundation for every higher composite encoding.

### 1.8 — Whole-`Score` canonical codec conventions `[P11-4 / P11-C "Provisional encoding" / P11-D2]`
- **Spec edit:** This is the Binary Format companion's job in full, but the *conventions* the three crates already share should be ratified as the companion's baseline so they don't drift apart before it's written.
- **Code choice (uniform across core/ops/bundle):** little-endian integers; single discriminant byte per tagged union; `u32` counts/length-prefixes; every variable-width leaf length-prefixed; raw (non-NFC) UTF-8 for free-text fields, NFC only for catalog ids at construction.
- **Disposition:** **Adopt as the companion's baseline convention set.** Do not attempt to write the full companion in Pass 11 — that's Track B/Agent J. Pass 11's job is to bless the conventions so core/ops/bundle stay mutually consistent until J formalizes them. Record explicitly that the companion *inherits* these conventions rather than re-deriving them.
- **Blast radius:** this is the seam between Pass 11 and the interchange track. Getting the convention baseline ratified means Agent J writes a companion that matches three crates instead of reconciling three divergent codecs.

---

## Bucket 2 — Decide-then-pin (judgment calls)

Each has a real fork. The implementation picked a branch; the choice has semantic weight. Resolve deliberately and record the rationale in the spec, not just the verdict.

### 2.1 — Tempo "Linear" interpolation parameter `[P11-7]`
- **Fork:** Chapter 3 says a `Linear` segment is "linear interpolation from `start_tempo` to `end_tempo`" without saying *what* interpolates linearly: bpm, period, or speed.
- **Code choice:** interpolates **speed** (whole notes per second) linearly; `Exponential` interpolates speed geometrically. Speed is beat-unit-agnostic and coincides with linear-bpm when both tempos share a beat unit.
- **Recommendation:** **adopt speed-linear**, and state the rationale in the spec (beat-unit-agnosticism is the right invariant for a format that supports tempo changes across beat-unit changes). But this is a genuine musical-semantics call — a conductor's "accelerando" intuition is arguably linear-in-bpm. Worth one deliberate confirmation because it changes the derived wall-clock schedule and therefore playback timing forever.
- **Blast radius:** every wall-clock schedule derived from a tempo map; playback, and any `wallclock_to_musical` inverse.

### 2.2 — Per-operation effect tag in a field collision `[P11-C3]`
- **Fork:** for concurrent differing `RespellPitch`es the spec pins the *conflict record* (kind `StructuralFieldCollision`, winner/loser) and says the later-in-canonical-order op wins and materializes. It does not pin which participant's `OperationEffect` reads `Conflicted`.
- **Code choice:** tags the **winner** (later op, which materializes and whose processing created the record) `Conflicted`; leaves the earlier op's `Applied` in place.
- **Recommendation:** **decide deliberately.** There's a defensible alternative: tag the *loser* `Conflicted` (or `NoOp{SupersededByLaterOperation}`) since it's the one whose intent didn't survive, and leave the winner `Applied`. The code's choice (winner carries the flag because it's the one that *noticed* the collision) is order-independent and defensible, but the loser-tagged alternative is arguably more intuitive for a UI surfacing "your edit was overridden." Pick one, state why. Sub-point to resolve in the same breath: whether the superseded loser retroactively reads `NoOp{SupersededByLaterOperation}`.
- **Blast radius:** any UI or analytics consuming per-op effects; conflict-resolution UX.

### 2.3 — `>2`-way voice-promotion collision `[P11-C4]`
- **Fork:** the spec describes a *pairwise* promotion rule. Three or more concurrent overlapping inserts into the same voice need a generalization the spec doesn't give.
- **Code choice:** order-independent pre-pass — bucket inserts by voice, walk by `OperationId`, keep a non-overlapping set in the original voice, promote each concurrent overlapping loser; the first lower-id overlapping op retained in the original voice is the winner for each promotion. Also applies the rule to partial interval overlaps, not just identical start positions.
- **Recommendation:** **adopt**, and **lift the generalization into the spec normatively** (it's currently only in code). The "lowest-id retained survivor wins" rule is deterministic and matches the pairwise rule's spirit. Confirm the partial-overlap extension is intended (it's stricter than the spec's identical-start-position language — arguably correct, but it's a widening of the rule).
- **Blast radius:** any score with 3+ concurrent inserts in one voice; the Operation Catalog's promotion payload.

### 2.4 — Open-vocabulary enums: `TransactionCategory`, `ObjectKind` `[P11-C9]`
- **Fork:** the spec calls these open vocabularies. The code gives minimal core sets with a `Registered` escape: `TransactionCategory ∈ {NoteEntry, Structural, Layout, Import, Registered}`; `ObjectKind ∈ {Voice, Pitch, Registered}` (only the kinds actually derived into the system namespace).
- **Recommendation:** **decide the core set, keep `Registered`.** For `TransactionCategory`, confirm the four core categories are the right minimal set (UIs/analytics consume it; under-specifying is cheap to extend, over-specifying is not). For `ObjectKind`, note it's intentionally narrower than `TypedObjectId`'s 27 kinds because only Voice/Pitch are minted into the system namespace today — confirm that's the intended scoping rather than an oversight.
- **Blast radius:** `SystemIdentifierCollision` payloads (`ObjectKind`); UI/analytics (`TransactionCategory`).

### 2.5 — `ResolveConflict` Dismissed selection `[P11-C10]`
- **Fork:** the spec distinguishes `Resolved` from `Dismissed` resolution states but provides one `ResolveConflictPayload { target, action }`. The code maps every applied resolve to `Resolved { action }`; `Dismissed` is a reachable state but no representative op authors it.
- **Recommendation:** **decide:** either add a distinct `action` variant that selects Dismissed, or a separate payload. Recommend the former (a `ResolutionAction::Dismiss` variant) — it's the smaller change and keeps one payload type. This is a small but real gap: without it, half the resolution-state machine is unreachable by authored operations.
- **Blast radius:** conflict-resolution UX; the Operation Catalog's `ResolveConflict` schema.

### 2.6 — Layout-object id derivation `[layout P11-2]`
- **Fork:** Chapter 7 declares `LayoutObjectId(pub u128)` and requires stability across relayouts but specifies neither the derivation, whether it's domain-separated, how a multiply-manifested object (a staff in two regions) is keyed, nor how synthesized objects are keyed.
- **Code choice:** keys multiply-manifested objects on `(source, region)`; synthesized objects on `(source, synthesis_kind, stable_semantic_instance_key)`. No domain tag registered.
- **Recommendation:** **decide + register a tag.** Pin the derivation, the manifestation-context key, and the synthesized-object key. Appendix D's domain-separation discipline suggests registering a dedicated `MUSC*` layout tag — recommend doing so for consistency even though layout ids are non-canonical (they don't enter document state, but stability-across-relayout is easier to reason about with a fixed derivation). This one is lower-stakes than the Bucket 1 ids because layout ids are not canonical document state — flag it for Track A (the visible slice) rather than blocking on it.
- **Blast radius:** incremental relayout correctness; provenance back-references. Consumed by Track A (solver + renderer), not by the interchange track.

---

## Bucket 3 — Fix (spec is contradictory or silent)

These change the spec regardless of the code, because the spec text is wrong or missing.

### 3.1 — Blob hashing shape contradiction `[P11-D3]` ⚠️ spec bug
- **The contradiction:** Chapter 8 §"Blobs" says blobs are "content-addressed identically to chunks (BLAKE3 of uncompressed payload, with the `MUSCBLOB` domain tag)." "Identically to chunks" implies the **structured** preimage (committing to kind, schema version, and length); "BLAKE3 of uncompressed payload with the domain tag" implies a **bare** `MUSCBLOB || payload`. These disagree.
- **Code choice:** follows Agent A's `ContentHash::of_blob` = bare `MUSCBLOB || payload` (the only spec content hash documented as a bare `domain || payload`).
- **Disposition:** **Fix the spec text** to state exactly one. Recommend **bare** (matches the existing `of_blob` and the intuition that a blob is opaque payload with no schema), and **delete the "identically to chunks" phrasing** so the contradiction can't recur. If the structured form is wanted instead, the code and golden change with it.
- **Blast radius:** every `BlobId`. Must be resolved before blobs are stored in any durable bundle.

### 3.2 — Superblock equal-generation tie-break `[P11-D1]`
- **The gap:** Chapter 8 §"Superblock Selection" says "the slot with the higher generation is active" but gives no rule for two valid slots at the *same* generation — which the QUICKSTART nonetheless lists as a harness scenario.
- **Code choice:** equal generation + equivalent load-bearing fields (`manifest_hash`, `manifest_schema_version`, `reduction_algorithm_version`, `profile_id`; advisory `commit_timestamp` and physical offset/length excluded) → equivalent, pick A. Equal generation differing in any load-bearing field → `IntegrityAnomaly::DivergentSameGeneration`, opened read-only.
- **Disposition:** **Fix (add the missing rule).** Adopt the code's rule into Chapter 8 normatively. It's the conservative correct call: two genuinely different committed states cannot share a generation under a conforming writer, so divergence is an integrity anomaly, not a silent pick. State the exact load-bearing field set (the exclusion of advisory fields is the subtle part).
- **Blast radius:** crash-recovery determinism; the manifest-selection harness's correctness depends on this being specified.

### 3.3 — `RetentionPolicy` field placement `[P11-D6]`
- **The gap:** Chapter 8 §"Garbage Collection and Retention" requires "the active conformance profile MUST declare a `RetentionPolicy`," but the `ProfileDeclaration` / `ProfileConstraints` structs shown in §"Format Profiles" don't include the field.
- **Code choice:** `retention_policy` placed inside `ProfileConstraints`; a bundle declaring multiple profiles resolves retention from the first declared profile.
- **Disposition:** **Fix (show the field).** Add `retention_policy` to the `ProfileConstraints` struct in the spec, and **decide** the multi-profile resolution rule (the code uses first-declared; confirm or specify a precedence). This is mostly mechanical but the multi-profile precedence is a small real decision.
- **Blast radius:** GC correctness; any bundle with a non-trivial retention policy.

### 3.4 — DVV zero-based counter floor (make normative) `[P11-C7]`
- **The gap:** the DVV's contiguous `vector[r] = n` asserts predecessors `(r, 0..=n)` exist. The code relies on this zero-based floor; the spec documents it in prose but should retain it explicitly in the *normative* DVV definition (it's load-bearing for the missing-causal-predecessor pending logic).
- **Code choice:** zero-based per-replica counter floor; range check walks known ids rather than expanding `0..=n` (so a sparse high-counter context doesn't cause proportional work).
- **Disposition:** **Fix (promote to normative).** No behavior change — just ensure the normative DVV definition states the zero-based floor so a second implementation can't choose a one-based floor and silently diverge on pending detection.
- **Blast radius:** cross-implementation agreement on which operations are pending vs. ready.

### 3.5 — Invariant count: 18 vs 19 `[P11-2]`
- **The gap:** QUICKSTART says "18 graph invariants"; Chapter 5 body enumerates 19; the code implements 19.
- **Disposition:** **Fix the QUICKSTART** (it's the stale doc; the spec body is authoritative). Trivial, but do it so the count is consistent across all three artifacts. While here, confirm the three Chapter-3 "reject at construction" rules (`TimeSignature` beat-group sum, `EventOrderingDAG` acyclicity, `Tuplet` degenerate ratios) are correctly *outside* the 19-invariant enumeration — the code enforces them at construction, which is faithful, but the spec should be explicit that they're construction-time MUSTs, not runtime invariants.
- **Blast radius:** documentation consistency only; no bytes.

---

## Items that need no spec change (record as resolved)

These appear in the DECISIONS files as P11 candidates but, on review, require no ratification — they resolve when later phases land, and the code already records the approximation honestly. List them so the agent doesn't spend time on them:

- **`P11-C1` (operation payload schemas carry identifiers + fingerprints):** resolves when the Operation Catalog (Track B/Agent K) lands. No Pass 11 action — the payload *schemas* are that companion's deliverable, not a ratification item. The provisional projection is honest.
- **`P11-C5` ("nearest surviving anchor" stand-in):** resolves when the graph-mutation phase tracks resolved positions. The rule *structure* is faithful; only the metric is approximated. No spec change.
- **`P11-C6` (time-model compatibility computed when graph available):** the rich migration payload belongs to the Operation Catalog. No Pass 11 action.
- **`P11-C8` (forward undo via minted-object compensation):** faithful to the spec's content-equivalence definition for insert-shaped transactions; per-primitive inverses are the Operation Catalog's job. No spec change.
- **`P11-5` (Chapter 4 tuning catalog is a separate subsystem):** a scope boundary, not an ambiguity. Becomes Track-C work (the tuning catalog) when scheduled. No Pass 11 action.
- **`P11-D2` (Binary Format companion not yet written):** the companion is Track B/Agent J. Pass 11 ratifies the *convention baseline* (item 1.8) so the crates stay consistent until then, but does not write the companion.
- **`layout P11-1` (Agent E dependency set vs. edit-barrier types):** a build-organization question (does layout depend on ops for `OperationKindTag`, or do the edit-barrier types relocate?). Decide this when re-cutting the agents for G–K — it's a crate-topology call, not a spec-byte call. Recommend blessing the layout→ops dependency (the discriminator type is small and stable) rather than relocating types.

---

## Suggested execution order for the agent

1. **Bucket 1 in id-dependency order:** 1.1 (`TypedObjectId`) first — most things reference object ids — then 1.5 (`ChunkKind`), then the derivations that build on them (1.2, 1.3, 1.4, 1.6), then the encodings (1.7, 1.8). Draft each as normative spec text + annotate the existing golden test to cite the ratified section. This is the bulk of the value and the least deliberation.
2. **Bucket 3 fixes:** 3.1 (blob contradiction) is the only true spec *bug* and gates durable blobs — do it early. 3.2–3.5 are mechanical-with-one-decision-each.
3. **Bucket 2 judgment calls:** these benefit from a short written rationale each. 2.1 (tempo-linear) and 2.2 (effect tag) are the two with lasting semantic weight; give them the most thought. 2.6 (layout ids) can defer to Track A.

**Deliverable:** a spec revision (call it the Pass 11 revision, consistent with the revision-history convention) plus a one-line-per-item ratification log recording the disposition (adopt / decided-X-because-Y / fixed) for each of the 25. The log is what lets the next-phase agents trust that "provisional" is now "ratified."

**Do not:** open any architectural question, rewrite any Pass 1–10 decision, or attempt the Binary Format companion or Operation Catalog here. Those are Track B. Pass 11's boundary is exactly: pin what exists, fix what contradicts, and ratify the convention baseline the companions will inherit.

---

## One-line inventory (for tracking)

| Item | P11 id | Bucket | Disposition | Gates |
|---|---|---|---|---|
| 1.1 TypedObjectId discriminants | P11-1 | 1 | Adopt | all object hashing/ordering |
| 1.2 Promoted-voice id | P11-3 / C4 | 1 | Adopt | Inv 18; promotion payload |
| 1.3 Synthetic-pitch id | P11-6 | 1 | Adopt + 1 decision | Inv 11 |
| 1.4 IntegrityAnomalyId + tag | P11-C2 | 1 | Adopt + tag call | anomaly agreement |
| 1.5 ChunkKind/Profile/Compression discriminants | P11-D4 | 1 | Adopt | all chunk hashing |
| 1.6 ManifestId preimage | P11-D5 | 1 | Adopt | multi-writer interop |
| 1.7 RationalTime + scalars | P11-4 | 1 | Adopt | all composite encoding |
| 1.8 Whole-Score codec conventions | P11-4/D2 | 1 | Adopt as baseline | interchange track seam |
| 2.1 Tempo Linear parameter | P11-7 | 2 | Recommend speed-linear | playback timing |
| 2.2 Field-collision effect tag | P11-C3 | 2 | Decide winner vs loser | conflict UX |
| 2.3 >2-way promotion | P11-C4 | 2 | Adopt + lift to spec | 3+ concurrent inserts |
| 2.4 Open-vocab enums | P11-C9 | 2 | Decide core sets | collision payloads, UI |
| 2.5 ResolveConflict Dismissed | P11-C10 | 2 | Add Dismiss action | conflict state machine |
| 2.6 Layout-object id | layout P11-2 | 2 | Decide + tag (defer to Track A) | relayout stability |
| 3.1 Blob hashing shape | P11-D3 | 3 | **Fix** (recommend bare) | every BlobId |
| 3.2 Equal-gen tie-break | P11-D1 | 3 | Fix (adopt code rule) | crash recovery |
| 3.3 RetentionPolicy placement | P11-D6 | 3 | Fix + multi-profile call | GC |
| 3.4 DVV zero-based floor | P11-C7 | 3 | Fix (make normative) | pending detection |
| 3.5 Invariant count 18→19 | P11-2 | 3 | Fix QUICKSTART | docs |
| — payload schemas | P11-C1 | none | defer to Track B | — |
| — nearest-anchor stand-in | P11-C5 | none | resolves with graph phase | — |
| — time-model compat | P11-C6 | none | defer to Track B | — |
| — forward undo | P11-C8 | none | defer to Track B | — |
| — tuning catalog scope | P11-5 | none | defer to Track C | — |
| — Binary Format companion | P11-D2 | none | Track B (1.8 baselines it) | — |
| — layout/ops dependency | layout P11-1 | none | crate-topology call for G–K | — |

25 candidate items: **8 adopt-and-pin, 6 decide-then-pin, 5 fix, 6 no-spec-change-defer.** (1.2/C4 and the layout dependency note are counted once each at their primary home.)
