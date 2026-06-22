# Pass 11 — Ratification Log

One line per worklist item recording the disposition. `adopt` = blessed the
implementation's golden-locked choice as normative spec text. `decided-X` = a
real fork resolved to X with rationale in the spec. `fixed` = the spec text was
contradictory or missing and changed. Spec edits are in `core_spec.tex`; every
byte-layout golden test now cites its ratified requirement.

Date: 2026-06-21. Spec revision: **Pass 11 (spec ratification)**. Architecture
unchanged. The full worklist is `PASS11_WORKLIST.md`.

## Bucket 1 — Adopt-and-pin (bytes)

| Item | P11 id | Disposition | Spec locus | Test/code anchor |
|---|---|---|---|---|
| 1.1 `TypedObjectId` discriminants | P11-1 | **adopt** — pinned the 16-bit BE discriminant table 0..=27; added the 5 missing variants (Tuplet/RepeatStructure/LyricLine/ChordSymbol/View) the code carried; `Registered`=27 = disc(2)‖reg(16)‖raw_be(16); `ObjectKindRegistryId` is 128-bit | §"Identifiers", `req:graph:typed-object-id-discriminants` | `typed_object_id_byte_form_is_locked` |
| 1.2 Promoted-voice id | P11-3 / C4 | **adopt** — `MUSCSVCE` 64-byte preimage (staff_instance‖original_voice‖winning_op‖losing_op, each 16 BE); staff-instance recovered from containment; one derivation feeds both the reducer and Invariant 18 | §"System-Promoted Voices" | `promoted_voice_id_byte_form_is_locked` |
| 1.3 System-pitch id | P11-6 | **adopt + decided** — `MUSCSPCH` over (scale position, acoustic realization), strings length-prefixed + NFC at the boundary; **decided** the tuning reference is *always* part of intrinsic identity, including the `Inherit` presence marker | §"System-Derived Pitch Identity", `req:graph:system-derived-pitch-id` | `system_pitch_id_byte_form_is_locked` |
| 1.4 `IntegrityAnomalyId` + tag | P11-C2 | **adopt + decided** — `derive_system_id(MUSCSANM, kind.canonical_bytes())`; **decided** `MUSCSANM` is a reserved *built-in* tag (anomalies are core), moved from `new_extension` to `DomainTag::SYSTEM_ANOMALY`. No byte change | §"System-Derived Counter Collisions", `req:graph:integrity-anomaly-id`; tag added to §"System-Derived Identifier Namespace" | `domain.rs` builtins; `anomaly.rs::anomaly_domain_tag` |
| 1.5 ChunkKind/Profile/Compression | P11-D4 | **adopt** — ChunkKind 0..=8, ProfileId 0..=3, CompressionAlgorithm 0..=2, all single declaration-order discriminant bytes; ChunkKind byte is in the chunk hash preimage | §"Chunks" `req:format:chunkkind-discriminants`; §"Format Profiles" `req:format:profileid-discriminants` | `chunk.rs::discriminant`, `chunk_kind_discriminants_round_trip` |
| 1.6 `ManifestId` preimage | P11-D5 | **adopt** — `trunc128(BLAKE3("MUSCMNIF" ‖ document_id ‖ generation_le ‖ manifest_body))`, body excludes `manifest_id` (self-reference exclusion is normative) | §"Manifest Encoding", `req:format:manifest-id`; Appendix D entry updated | `bundle/ids.rs::derive` |
| 1.7 `RationalTime` + scalars | P11-4 | **adopt** — sign byte + u32-LE-length-prefixed BE numerator + u32-LE-length-prefixed BE denominator, always reduced; wall-clock integers little-endian | §"Binary Format Companion", `req:format:rationaltime-encoding` | `time.rs::CanonicalEncode for RationalTime` |
| 1.8 Codec conventions | P11-4 / D2 | **adopt as companion baseline** — LE ints; single discriminant byte per tagged union; u32 counts/length-prefixes on every variable-width leaf; raw UTF-8 free-text, NFC only for catalog ids at construction. Companion *inherits* this | §"Binary Format Companion", `req:format:codec-conventions` | `core/codec.rs` module doc |

## Bucket 2 — Decide-and-pin

| Item | P11 id | Disposition | Spec locus | Test/code anchor |
|---|---|---|---|---|
| 2.1 Tempo `Linear` parameter | P11-7 | **decided: speed-linear** — interpolates whole-notes-per-second (beat-unit-agnostic), not bpm/period; `Exponential` interpolates speed geometrically. Rationale: a tempo map may change beat unit across segments, so only speed gives a beat-unit-independent wall-clock schedule | §"Conversion", `req:time:linear-interpolates-speed` + rationale | `tempo.rs::SpeedModel` |
| 2.2 Field-collision effect tag | P11-C3 | **decided: winner-carries-`Conflicted`** — the later op (which materializes and noticed the collision) reads `Conflicted`; the earlier op keeps `Applied`. Chosen for order-independence; a UI reads the record's `loser` field for "your edit was overridden" | `req:semops:field-collision-effect` + rationale; RespellPitch reduction rule | `reduce.rs::respell_pitch` |
| 2.3 `>2`-way promotion | P11-C4 | **adopt + lifted to normative** — order-independent pre-pass: bucket by voice, walk by OperationId, retain a non-overlapping set, promote each overlapping loser (lowest-id retained survivor wins); applies to **partial** interval overlaps, not just identical onsets | §"System-Promoted Voices", `req:graph:promotion-generalization` | `reduce.rs::compute_promotions` |
| 2.4 Open-vocab enums | P11-C9 | **decided: pinned core sets, kept `Registered`** — `TransactionCategory ∈ {NoteEntry, Structural, Layout, Import, Registered}`; `ObjectKind ∈ {Voice, Pitch, Registered}` (narrower than the 28 object kinds: only kinds minted into the system namespace) | `req:semops:transaction-category`, `req:graph:object-kind-vocab` | `payload.rs`, `support.rs` |
| 2.5 `ResolveConflict` Dismissed | P11-C10 | **decided: added `ResolutionAction::Dismiss`** (code + spec) — closes the half-unreachable state machine; the Dismiss action selects the `Dismissed` state, every other action selects `Resolved` | §"Conflict Resolution Operations" | `conflict.rs`, `reduce.rs::resolve_conflict`, `resolve_conflict_with_dismiss_reaches_dismissed_state` |
| 2.6 Layout-object id | layout P11-2 | **decided + registered tag (Track A)** — `MUSCLOID`-tagged derivation; keys multiply-manifested objects on `(source, region)`, synthesized objects on `(source, synthesis_kind, stable_semantic_instance_key)`. Non-canonical (not document state); consumed by the solver/renderer | §"Provenance", `req:layoutir:object-id-derivation` | `layout-ir` provenance |

## Bucket 3 — Fixes (spec was contradictory or silent)

| Item | P11 id | Disposition | Spec locus | Test/code anchor |
|---|---|---|---|---|
| 3.1 Blob hashing shape | P11-D3 | **fixed (spec bug)** — bare `BLAKE3("MUSCBLOB" ‖ payload)`; deleted the contradictory "identically to chunks" phrasing so the structured-vs-bare contradiction cannot recur | §"Blobs", `req:format:blob-hash-shape` | `determinism/hash.rs::of_blob` |
| 3.2 Equal-generation tie-break | P11-D1 | **fixed (gap) — adopted code rule** — equal generation + equal load-bearing fields {manifest_hash, manifest_schema_version, reduction_algorithm_version, profile_id} → equivalent, pick A; any divergence → `DivergentSameGeneration`, read-only. Advisory fields (commit_timestamp, offset/length) excluded | §"Superblock Selection" + rationale | `superblock.rs::selection_equivalent` |
| 3.3 RetentionPolicy placement | P11-D6 | **fixed (silent) — defined `ProfileConstraints`** (was a dangling forward reference) and placed the required `retention_policy` in it; **decided** multi-profile precedence = first-declared profile | §"Format Profiles", `req:format:profile-constraints` | `manifest.rs::ProfileConstraints` |
| 3.4 DVV zero-based floor | P11-C7 | **fixed (promote to normative)** — `vector[r]=n` ⟹ `(r,0..=n)` are predecessors; zero-based floor is normative so a one-based implementation can't diverge on pending detection. No behavior change | §"Causal Context via Dotted Version Vectors" | `causal.rs` |
| 3.5 Invariant count 18→19 | P11-2 | **fixed** — QUICKSTART 18→19; spec body states the count is 19 and names the three *construction-time* MUSTs (time-signature beat-group sum, ordering-DAG acyclicity, non-degenerate TupletRatio). **Tuplet honesty:** added `TupletRatio::new` rejecting degenerate ratios at construction (zero term or `actual==notated`) + codec decode validation; removed the now-redundant runtime sub-check | QUICKSTART; §"Graph Invariants" note; §"Tuplets" `req:time:tuplet-ratio-construction` | `graph.rs::TupletRatio::new`, `degenerate_tuplet_ratio_is_rejected_at_construction` |

## No-spec-change items (recorded resolved, no Pass 11 action)

`P11-C1` operation payload schemas → Track B (Operation Catalog). `P11-C5`
nearest-anchor stand-in → resolves with the graph-mutation phase. `P11-C6`
time-model compatibility → Track B. `P11-C8` forward undo → Track B. `P11-5`
Chapter 4 tuning catalog → Track C. `P11-D2` Binary Format companion → Track B
(item 1.8 baselines it). `layout P11-1` layout→ops dependency → crate-topology
call for the G–K re-cut (recommend blessing the dependency).

**Tally:** 8 adopt-and-pin, 6 decide-then-pin, 5 fix, 6 no-spec-change-defer =
25 candidate items. All 19 ratifiable items are ratified; the 6 deferrals are
recorded with their owning track.
