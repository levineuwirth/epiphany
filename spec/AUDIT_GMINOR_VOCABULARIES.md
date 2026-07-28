# Audit: G-minor reachable vocabularies — a ledger and a reachability matrix

Governed by `spec/CONTRACT_GMINOR_VOCABULARY_AUDIT.md`. Read-only packet;
produces this file only. Method: walked each `ChunkKind` role's payload
encoder transitively (`encode_canonical` / `Codec::enc` / `canonical_bytes`),
confirming every discriminant-writing site actually reached from that role,
rather than trusting the type graph. `grep -rn "push_tag\|fn discriminant"`
over `crates/epiphany-core/src` and `crates/epiphany-ops/src` was used as the
starting net (and a comprehensive `grep` for `Additive|appended|append at\b`
comments across `epiphany-core`, `epiphany-ops`, `epiphany-layout-ir`,
`epiphany-bundle` was used as a second, independent net for "did this
vocabulary ever get a documented native append" — see "How completeness was
checked" at the end).

Repo state at audit time: `git status --short` showed only the pre-existing
editor-track dirt (`M Cargo.toml`, untracked `spikes/editor-toolkit/**`); see
the Gate section at the end for the before/after capture.

---

## Part 1 — the vocabulary ledger

For each vocabulary: full variant list, discriminant, classification. Unless
otherwise noted, discriminants are one byte (`u8`) and "escape" means a
`Registered`-shaped variant carrying a registry id (native extension point).
`TypedObjectId` is the one 16-bit (`u16`) exception, noted inline.

### 1.1 `OperationKind` (`ops/src/payload.rs:116`, `discriminant()` at :275)

**Baseline** (golden-locked 0..=23 — the *value-typed* v1 catalog's frozen
range; the identifier-only v0 catalog, `ops/src/v0.rs::V0OperationKind`, only
ever had discriminants 0..=7 and is entirely superseded by migration, not a
second baseline):

| # | Variant | | # | Variant |
|---|---|---|---|---|
| 0 | InsertEvent | | 12 | ModifyIdentifiedPitch |
| 1 | DeleteEvent | | 13 | DeleteCrossCutting |
| 2 | RespellPitch | | 14 | ModifyCrossCutting |
| 3 | CreateCrossCutting | | 15 | CreateRegion |
| 4 | ChangeRegionTimeModel | | 16 | DeleteRegion |
| 5 | SetUserSystemBreak | | 17 | CreateStaffInstance |
| 6 | DeclareTransaction | | 18 | DeleteStaffInstance |
| 7 | Registered (escape) | | 19 | CreateVoice |
| 8 | ModifyEvent | | 20 | DeleteVoice |
| 9 | Transpose | | 21 | SetMetadata |
| 10 | InsertIdentifiedPitch | | 22 | SetMetricGrid |
| 11 | DeleteIdentifiedPitch | | 23 | SetUserPageBreak |

**Post-baseline native** (10 variants, exactly matching P13-S14's list):

| Discriminant | Variant | Introduction event |
|---|---|---|
| 24 | CreateStaff | Phase-3 first tranche |
| 25 | SetTimeSignature | Phase-3 first tranche |
| 26 | SetTempoSegment | Phase-3 first tranche |
| 27 | SetStaffLayout | Phase-3 first tranche |
| 28 | CreateRepeatStructure | schema-major-2 repeat-authoring revision |
| 29 | DeleteRepeatStructure | schema-major-2 repeat-authoring revision |
| 30 | TransposeInterval | Push 4a |
| 31 | CreateInstrument | genesis tranche G1 |
| 32 | SetCanvasLayoutDefaults | genesis tranche G2a |
| 33 | SetSpellingPrecedence | genesis tranche G2a |

**Escape:** 7 (Registered). Out of scope per contract.

### 1.2 `OperationKindTag` (`ops/src/payload.rs:421`, macro-generated at :487)

Independent discriminant space from `OperationKind` (trap 2 confirmed:
`RespellPitch` is kind 2 / tag 3). Source of truth is the
`operation_kind_tag_vocabulary!` macro (exhaustive, no wildcard arm) — the
hand-written `OperationKind::discriminant()` match was **not** consulted for
this vocabulary; the macro table at :532 was.

**Baseline** (0..=15, 17..=23 — `Registered` sits at 16, splitting the
range):

| Disc. | Variant | | Disc. | Variant |
|---|---|---|---|---|
| 0 | InsertEvent | | 12 | DeleteStaffInstance |
| 1 | DeleteEvent | | 13 | SetUserSystemBreak |
| 2 | ModifyEvent | | 14 | SetUserPageBreak |
| 3 | RespellPitch | | 15 | DeclareTransaction |
| 4 | Transpose | | 16 | **Registered (escape)** |
| 5 | CreateCrossCutting | | 17 | InsertIdentifiedPitch |
| 6 | DeleteCrossCutting | | 18 | DeleteIdentifiedPitch |
| 7 | ModifyCrossCutting | | 19 | ModifyIdentifiedPitch |
| 8 | ChangeRegionTimeModel | | 20 | CreateVoice |
| 9 | InsertRegion | | 21 | DeleteVoice |
| 10 | DeleteRegion | | 22 | SetMetadata |
| 11 | InsertStaffInstance | | 23 | SetMetricGrid |

**Post-baseline native** (mirrors `OperationKind` 1:1, same 10 events):

| Disc. | Variant | Introduction event |
|---|---|---|
| 24 | InsertStaff | Phase-3 first tranche |
| 25 | SetTimeSignature | Phase-3 first tranche |
| 26 | SetTempoSegment | Phase-3 first tranche |
| 27 | SetStaffLayout | Phase-3 first tranche |
| 28 | CreateRepeatStructure | schema-major-2 repeat-authoring revision |
| 29 | DeleteRepeatStructure | schema-major-2 repeat-authoring revision |
| 30 | TransposeInterval | Push 4a |
| 31 | CreateInstrument | genesis tranche G1 |
| 32 | SetCanvasLayoutDefaults | genesis tranche G2a |
| 33 | SetSpellingPrecedence | genesis tranche G2a |

**Escape:** 16 (Registered). Out of scope.

### 1.3 `OperationPayload` (`ops/src/payload.rs:59`, `discriminant()` at :75)

**Baseline** (0..=2, no escape variant on this vocabulary):

| Disc. | Variant |
|---|---|
| 0 | Primitive (carries `OperationKind`) |
| 1 | ResolveConflict |
| 2 | UndoTransaction |

**Post-baseline native:**

| Disc. | Variant | Introduction event |
|---|---|---|
| 3 | ResolveEquivocation | Push 3 (2026-07); catalog §ResolveEquivocation ratified, `epiphany-ops` DECISIONS.md:620 "ResolveEquivocation + validation modes (2026-07, Push 3)"; PASS12_RATIFICATION_LOG.md:41 confirms discriminant 3, catalog 0.3.0 → 0.4.0 |

Trap 1 confirmed live here: `binary_format.tex:2384` says "append at ≥4," but
discriminant 3 is itself the one and only append this vocabulary has ever
had — the ≥N phrasing names the next free slot, not the baseline boundary,
exactly as the contract warns.

### 1.4 `ReanchorReason` (`ops/src/effect.rs:317`, `discriminant()` at :331)

**Baseline** (0..=5, escape at 5):

| Disc. | Variant |
|---|---|
| 0 | SameVoiceNearer |
| 1 | SameStaffInstanceNearer |
| 2 | SameStaffNearer |
| 3 | SameRegionNearer |
| 4 | ExplicitFallback |
| 5 | DeclaredByExtension (escape) |

**Post-baseline native:**

| Disc. | Variant | Introduction event |
|---|---|---|
| 6 | SameCanvasNearer | Pass-12 G-pass, item P12-C4 (ratified 2026-07-07; `PASS12_RATIFICATION_LOG.md:147`) |

This is the checklist's "sharpest edge" case realized: the native append (6)
sits *after* the escape variant (5) on the same enum.

### 1.5 `PreconditionFailureReason` (`ops/src/effect.rs:124`, `discriminant()` at :188)

**Baseline** (0..=9, escape at 9):

| Disc. | Variant |
|---|---|
| 0 | TargetMissing |
| 1 | TargetTombstoned |
| 2 | WrongRegionTimeModel |
| 3 | TupletCompensationInvalid |
| 4 | EventDurationInvalid |
| 5 | PositionOutsideRegion |
| 6 | PitchSpaceMismatch |
| 7 | VoiceMissing |
| 8 | ExtensionPrecondition |
| 9 | Registered (escape) |

**Post-baseline native:**

| Disc. | Variant | Introduction event |
|---|---|---|
| 10 | ContainerNotEmpty | Operation Catalog M2c ("Group 3": structural-container CRUD, Agent K Phase-2 — `ops/DECISIONS.md:165` "M2c (Group 3) — structural containers: empty-only delete") |
| 11 | TempoMapMalformed | Phase-3 first tranche (`ops/DECISIONS.md:787-796`) |
| 12 | SystemDerivedContentImmutable | Pass-12 G-pass, item P12-K3 (ratified 2026-07-07; `PASS12_RATIFICATION_LOG.md:135`) |
| 13 | RecreateContentMismatch | Pass-12 G-pass, item P12-K9 (ratified 2026-07-07; `PASS12_RATIFICATION_LOG.md:140`) |
| 14 | AcousticRealizationPinned | Push 4a |
| 15 | TranspositionOutOfRange | Push 4a |

Note the boundary mismatch with §1.1: `OperationKind`'s own baseline absorbed
the whole M2c tranche (kinds 15–20) into its golden-locked 0..=23 range, but
`PreconditionFailureReason`'s baseline (0..=9) had already been fixed
*before* M2c landed, so M2c's own contribution to *this* vocabulary
(`ContainerNotEmpty` = 10) is a genuine post-baseline append even though the
`OperationKind` variants that provoke it (`CreateRegion` et al.) are
baseline. **Each vocabulary's baseline boundary is independent — this is the
concrete case that makes that true, not merely a caution from the contract.**

### 1.6 The fourteen checked-clean escape-carrying enums

Every entry below was located, its full discriminant match read, and no
comment, `DECISIONS.md` entry, or ratification-log entry recorded a native
append after its escape variant. **Negative result, recorded as evidence,**
per the contract's requirement that a clean checklist entry be stated, not
omitted.

| # | Vocabulary | Site | Discriminants | Escape at | Result |
|---|---|---|---|---|---|
| 1 | `RepairKind` | `ops/src/effect.rs:248` | 0..=7 | 7 | clean — no append |
| 4 | `IntegrityAnomalyKind` | `ops/src/anomaly.rs:143` | 0..=3 | 3 | clean — no append |
| 5 | `ReplicaAnomalyReason` | `ops/src/anomaly.rs:70` | 0..=1 | 1 | clean — no append |
| 6 | `TransactionCategory` | `ops/src/payload.rs:1007` | 0..=4 | 4 | clean — no append |
| 7 | `ResolutionAction` | `ops/src/conflict.rs:181` | 0..=5 | 5 | clean — no append |
| 8 | `ConflictKind` (via `ExtensionConflict`) | `ops/src/conflict.rs:74` | 0..=5 | 5 | clean — no append |
| 9 | `BarrierScope` | `layout-ir/src/barrier.rs:60` | 0..=7 | 7 | clean — no append |
| 10 | `BarrierCondition` | `layout-ir/src/barrier.rs:75` | 0..=6 | 6 | clean — no append |
| 11 | `TieClass` | `core/src/graph.rs:1080`, codec at `core/src/codec.rs:2302` | 0..=4 | 4 | clean — no append |
| 12 | `StaffGroupKind` | `core/src/graph.rs:1599`, codec at `core/src/codec.rs:2267` | 0..=4 | 4 | clean — no append |
| 13 | `PitchSpacePosition` | `core/src/pitch.rs:436`, codec at `core/src/codec.rs:938` | 0..=3 | 3 | clean — no append |
| 14 | `SpellingNominal` | `core/src/pitch.rs:896`, codec at `core/src/codec.rs:1066` | 0..=2 | 2 | clean — no append |
| 15 | `TypedObjectId` | `core/src/ids.rs:490`, `discriminant()` at :525 (**16-bit**, the one exception) | 0..=27 | 27 | clean — no append. **All 28 values were assigned in one batch**, `epiphany-core/DECISIONS.md:52-68` (P11-1, Pass 11, 2026-06-21): the spec named kinds only through `AnalysisLayer=21`; this crate extended the table to 27 (`Tuplet`, `RepeatStructure`, `LyricLine`, `ChordSymbol`, `View`, `Registered`) in the *same* ratifying pass, then locked it with a golden-bytes test. There is no second, later append on top of that lock. |
| 16 | barrier `ObjectKind`'s open value space | `layout-ir/src/barrier.rs:30` | n/a — see below | n/a | clean, but structurally different — see note |

**Note on #16.** This is **not** an enum with a `discriminant()` match; it is
`pub struct ObjectKind(pub u16)`, a transparent wrapper that stores whatever
`TypedObjectId::discriminant()` produced for the referenced object
(`barrier.rs:30-35`). Its "open value space" is open only in the sense that
the wire form accepts any `u16` without independently validating it against
a known set on decode (`barrier.rs:476-488`); it carries **no discriminant
vocabulary of its own** and has no append story separate from
`TypedObjectId`'s (checked clean at #15). **Naming collision flagged for the
record:** this is a *different* type from `epiphany-ops::support::ObjectKind`
(`ops/src/support.rs:172`, the `Voice`/`Pitch`/`Registered` enum used inside
`IntegrityAnomalyKind::SystemIdentifierCollision`) — same name, two
unrelated types in two crates. Checked separately below as part of
`IntegrityAnomalyKind`'s reachable set (it has no `Registered`-adjacent
append of its own either: 0..=2, escape at 2, clean).

### 1.7 Vocabularies reached with no `Registered` escape at all, checked and clean

These are the "closed value-layer unions" the contract's IN-scope clause
covers (`binary_format.tex:2386`: closed unions may still gain variants
through a ratified document revision, and such appends are minor-additive
too). Located via the reachability walk below; each was checked against the
"Additive / appended" comment net (see the completeness note) with no hits.

| Vocabulary | Site | Discriminants | Result |
|---|---|---|---|
| `OperationEffect` | `ops/src/effect.rs:29` | 0..=4 | clean — no append |
| `NoOpReason` | `ops/src/effect.rs:76` | 0..=4 | clean — no append |
| `ObjectState` | `ops/src/reduce.rs:416` | 0..=1 | clean — no append |
| `PendingReason` | `ops/src/reduce.rs:455` | 0..=4 | clean — no append |
| `ConflictResolutionState` | `ops/src/conflict.rs:230` | 0..=2 | clean — no append |
| `UndoPolicy` | `ops/src/undo.rs:32` | 0..=2 | clean — no append |
| `epiphany-ops::support::ObjectKind` | `ops/src/support.rs:172` | 0..=2 (escape 2) | clean — no append |

`BracketKind`, `MetadataValue`, `PedalKind` and similar new enums introduced
*at* schema major 2 (`core/src/graph.rs:1038,1479` etc.) were checked and are
**not** in scope: they were minted whole at a major bump, not appended-to
afterward — they have no post-baseline history of their own to record.

### 1.8 `CompressionAlgorithm` — EXCLUDED by ruling (2026-07-28)

`bundle/src/chunk.rs:119`: `None=0`, `Zstd{level}=1`, `Reserved(u8)=2`
("Reserved for future format major versions"). This audit surfaced it as a
boundary case — it is in neither the contract's checklist nor the plan's §3
table — and **it is now ruled out of scope for schema-minor epochs.**

Two independent grounds, both read from the tree:

1. **It is not a discriminant emitted by the payload that `SchemaVersion`
   governs.** It is transport metadata riding `ChunkRef` alongside the chunk's
   offset and length, structurally parallel to `ChunkKind` — which the
   contract already excludes — rather than a value- or operation-layer
   discriminant inside the payload bytes.
2. **It is deliberately outside chunk identity.** `chunk_content_hash`'s
   preimage is `domain || kind || schema || uncompressed_length || payload`
   (`bundle/src/chunk.rs:177`), and both the function's own doc comment and
   the type's (`:110`, "**Not** part of content identity") state the exclusion
   as intentional. A minor is content-addressed *because* it enters that
   preimage (plan §2); compression never does, so it has no minor to raise.

Recorded here explicitly so the question is settled rather than reopened each
time the enum's `Reserved(u8)` slot is noticed. Its `Reserved` variant is a
format-**major** escape, consistent with its own doc comment.

---

## Part 2 — the reachability matrix

| Chunk role | Encoded payload type | Discriminant vocabulary | Post-baseline variants | Introduction event | Derivation site |
|---|---|---|---|---|---|
| `OperationEnvelopeBlock` | `OperationEnvelope` → `OperationPayload` | `OperationPayload` | `ResolveEquivocation` = 3 | Push 3 | `ops/src/payload.rs:100` (`push_tag` in `OperationPayload::encode_canonical`) |
| `OperationEnvelopeBlock` | ↳ `OperationPayload::Primitive` | `OperationKind` | 24–33 (10 variants, §1.1) | Phase-3 / schema-major-2 / Push 4a / G1 / G2a | `ops/src/payload.rs:373` |
| `OperationEnvelopeBlock` | ↳ `OperationKind::DeclareTransaction` → `TransactionDescriptor` | `TransactionCategory` | none | — (baseline only) | `ops/src/payload.rs:1029` |
| `OperationEnvelopeBlock` | ↳ `OperationPayload::ResolveConflict` → `ResolveConflictPayload` | `ResolutionAction` | none | — (baseline only) | `ops/src/conflict.rs:211` |
| `OperationEnvelopeBlock` | ↳ `OperationPayload::UndoTransaction` → `UndoTransactionPayload` | `UndoPolicy` | none | — (baseline only) | `ops/src/undo.rs:57` |
| `OperationEnvelopeBlock` | ↳ every primitive op's embedded graph value (`Event`, `Region`, `Instrument`, `PitchSpelling`, `Tie`, `Slur`, `Beam`, `Spanner`, …) | the four escape vocabularies (`TieClass`, `StaffGroupKind`, `PitchSpacePosition`, `SpellingNominal`) + all closed value-layer unions | none found | — | `core/src/codec.rs` (per-type `Codec::enc`, embedded via `push_lp_bytes(out, &value.canonical_bytes())` at each op site, e.g. `ops/src/payload.rs:1513`, `:1528`) |
| `OperationIndex` | `(ChunkRef, Vec<(id, block, offset)>)` | — | — | — | **(b) emits no additive discriminant.** Payload is `block_count × ChunkRef` + `entry_count × {id bytes, block ordinal, offset}` (`bundle/src/opindex.rs:26-38`); the only discriminant present is each `ChunkRef`'s `ChunkKind` tag, out of scope by the contract's own exclusion. |
| `Snapshot` (canonical base) | `MaterializedState` | `OperationEffect` | none | — | `ops/src/reduce.rs:548` |
| `Snapshot` (canonical base) | ↳ `OperationEffect::AppliedWithRepair` → `RepairRecord` | `RepairKind` | none | — | `ops/src/effect.rs` (`RepairKind::encode_canonical`) |
| `Snapshot` (canonical base) | ↳ `RepairKind::Reanchored` | `ReanchorReason` | `SameCanvasNearer` = 6 | Pass-12 G-pass (P12-C4) | `ops/src/effect.rs:331` |
| `Snapshot` (canonical base) | ↳ `OperationEffect::NoOp` | `NoOpReason` | none | — | `ops/src/effect.rs:96` |
| `Snapshot` (canonical base) | ↳ `NoOpReason::PreconditionFailedUnderReduction` | `PreconditionFailureReason` | 10–15 (6 variants, §1.5) | M2c / Phase-3 / Pass-12 G-pass ×2 / Push 4a | `ops/src/effect.rs:188` |
| `Snapshot` (canonical base) | ↳ `conflicts: ConflictRegistry` → `ConflictRecord` | `ConflictKind` | none | — | `ops/src/conflict.rs` (`ConflictKind::encode_canonical`) |
| `Snapshot` (canonical base) | ↳ `ConflictRecord::resolution_state` | `ConflictResolutionState` | none | — | `ops/src/conflict.rs:230` |
| `Snapshot` (canonical base) | ↳ `anomalies: Vec<IntegrityAnomaly>` | `IntegrityAnomalyKind` | none | — | `ops/src/anomaly.rs:143` |
| `Snapshot` (canonical base) | ↳ `IntegrityAnomalyKind::SystemIdentifierCollision` | `epiphany-ops::support::ObjectKind` | none | — | `ops/src/support.rs:191` |
| `Snapshot` (canonical base) | ↳ `objects: BTreeMap<TypedObjectId, ObjectState>` | `TypedObjectId` (map key) + `ObjectState` | none / none | — | `ops/src/reduce.rs:437`, `core/src/ids.rs` |
| `Snapshot` (canonical base) | ↳ `spellings: BTreeMap<PitchId, PitchSpelling>` | `SpellingNominal` | none | — | `core/src/codec.rs:1066` (via `PitchSpelling::canonical_bytes`) |
| `Snapshot` (canonical base) | ↳ `pending: Vec<(OperationId, PendingReason)>` | `PendingReason` | none | — | `ops/src/reduce.rs:494` |
| `Snapshot` (canonical base) | — | **Notably absent:** no `OperationKind`, `OperationKindTag`, or `OperationPayload` discriminant anywhere. Confirms the plan's §3 claim for this role. | | | |
| `Snapshot` (acceleration cache, full `Score`) | `Score` (whole-graph codec) | the four escape vocabularies + all closed value-layer unions | none found | — | `core/src/codec.rs` (`Score`'s `Codec` impl, transitively over every graph type) |
| `Blob` | caller-supplied bytes | — | — | — | **(d) truly producer-owned opaque bytes.** `ChunkKind::Blob` doc: "audio, image, font, ML model" (`bundle/src/chunk.rs:26`); no core encoder produces the content, only stages/verifies it (`bundle/src/bundle.rs:482,1018`). |
| `ExtensionData` | extension-supplied bytes | — | — | — | **(d) truly producer-owned opaque bytes**, per contract's own framing — no core encoder, no core derivation possible. |
| `TextProjection` | — | — | — | — | **(b) emits no additive discriminant — because no producer exists.** `ChunkKind::TextProjection` is declared (`bundle/src/chunk.rs:31,54`) and named in the text-projection kind↔string table (`textproj/src/project.rs:262`, `parse.rs:545`), but no site in this codebase ever builds a chunk of this kind. The role is reserved, unwired. |
| `LayoutCache` | — | — | — | — | **(b) emits no additive discriminant — because no producer exists.** `epiphany-layout-ir::cache::LayoutCache` (`layout-ir/src/cache.rs:82`) is a pure in-memory incremental-layout index (`DependencyIndex` + `BTreeSet<LayoutObjectId>` stage caches) with **no `CanonicalEncode` impl at all** and no wiring to `ChunkKind::LayoutCache`. Every use of `ChunkKind::LayoutCache` in `bundle/src/bundle.rs` is a test planting literal placeholder bytes (e.g. `b"layout-cache-bytes "`, `bundle.rs:2323`). |
| `IntegrityIndex` | — | — | — | — | **(b) emits no additive discriminant — because no producer exists.** Referenced only in the kind↔string round-trip table and in `testkit/src/generators.rs`'s random `ChunkRef` fuzzing (never given real integrity-index content). |
| `Manifest` | `Manifest` body (`bundle/src/manifest.rs:459`) | — (own fields: ids, `ChunkRef`s, `SemVer`s — no discriminant vocabulary) | — | — | `bundle/src/manifest.rs:459-513` |
| `Manifest` | ↳ `ExtensionDeclaration::edit_barriers: Vec<u8>` | **(c) normatively typed bytes, opaque only to the bundle layer.** Producer: `layout_ir::barrier::EditBarrier::encode_canonical` | | | `layout-ir/src/barrier.rs:387` |
| `Manifest` | ↳ ↳ `EditBarrier::scope` | `BarrierScope` | none | — | `layout-ir/src/barrier.rs:317` |
| `Manifest` | ↳ ↳ `EditBarrier::condition` | `BarrierCondition` | none | — | `layout-ir/src/barrier.rs:352` |
| `Manifest` | ↳ ↳ `EditBarrier::prohibited_operation_kinds: Vec<OperationKindTag>` | `OperationKindTag` | 24–33 (10 variants, §1.2) | Phase-3 / schema-major-2 / Push 4a / G1 / G2a | `ops/src/payload.rs:499` (macro-generated `encode_canonical` at :568) |
| `Manifest` | ↳ `ExtensionDeclaration::affected_object_kinds: Vec<u8>` | **(c)**, producer: `layout_ir::barrier::ObjectKind` (u16 wrapper of `TypedObjectId::discriminant()`) | none (see §1.6 #16) | — | `layout-ir/src/barrier.rs:39` |

**Nine roles, all accounted for:** `OperationEnvelopeBlock` (a), `OperationIndex` (b), `Snapshot`/canonical-base (a), `Snapshot`/acceleration (a — clean), `Blob` (d), `ExtensionData` (d), `TextProjection` (b, unwired), `LayoutCache` (b, unwired), `IntegrityIndex` (b, unwired), `Manifest` (c, crossing into `layout-ir`'s barrier encoder).

That is ten rows of disposition against nine `ChunkKind` variants because
`Snapshot` is payload-polymorphic (trap 3) and gets one disposition per form,
as the contract requires.

---

## Completeness statement

- **Which roles emit no additive discriminant, and why:** `OperationIndex`
  (its payload is ids, ordinals, and offsets — no discriminant besides the
  out-of-scope `ChunkKind` tag inside each `ChunkRef`); `TextProjection`,
  `LayoutCache`, `IntegrityIndex` (all three: no producer exists in this
  codebase at all — the chunk kind is declared and named but unwired, which
  is a different finding from "encodes nothing interesting").
- **Every escape-carrying enum from the sixteen-entry checklist, with its
  result:** recorded in §1.6, all sixteen present, two with native
  post-baseline appends (`ReanchorReason` #2, `PreconditionFailureReason`
  #3 — both outside the sixteen but reached via #1/#4's neighborhood and
  recorded in §1.4/§1.5) and fourteen clean.

  **The tally:** sixteen checklist entries, sixteen checked, sixteen results
  recorded. Fourteen clean (§1.6). Two with native post-baseline appends —
  `ReanchorReason` (position 2, §1.4) and `PreconditionFailureReason`
  (position 3, §1.5), which is why §1.6's table skips those two numbers. No
  entry is unchecked and none is counted twice.
- **Variants whose introduction event could not be established:** **none.**
  Every post-baseline native variant found (`OperationKind`/`OperationKindTag`
  24–33, `OperationPayload` 3, `ReanchorReason` 6, `PreconditionFailureReason`
  10–15) has a sourced introduction event from at least one of: a code
  comment naming the tranche, `crates/epiphany-ops/DECISIONS.md`, or
  `spec/PASS12_RATIFICATION_LOG.md`. Where more than one source existed
  (`OperationPayload` 3: both `payload.rs:80` comment and
  `PASS12_RATIFICATION_LOG.md:41` state Push 3 / discriminant 3), they
  agreed; no dispute was found to adjudicate.
- **Where the spec and the code disagree about what was appended when: one
  place, `binary_format.tex:2373`.** The bullet list at `:2373-2385` says
  `OperationKind` and `OperationKindTag` "append at ${\geq}30$", and its
  parenthetical history names only the Phase-3 tranche (24–27) and the
  schema-major-2 repeat pair (28/29). **Kinds 30–33 — `TransposeInterval`
  (Push 4a), `CreateInstrument` (G1), `SetCanvasLayoutDefaults` and
  `SetSpellingPrecedence` (G2a) — are absent from both the number and the
  narrative**, and 30 is no longer free.

  This audit initially recorded "no disagreement," which was wrong, and wrong
  in a way worth naming: it adopted trap 1's next-free-slot reading of "≥N"
  and *then* declared the ≥30 consistent. **Both cannot hold.** Read as a
  floor the sentence survives (34 ≥ 30); read as next-free-slot — the reading
  this audit uses everywhere else, and the reading that made
  `OperationPayload`'s "≥4 while 3 is itself the append" legible — it is
  stale by four.

  **Disposition: stale narrative, not table drift.** The normative kind and
  tag tables *are* current — `binary_format.tex:1443-1457` carries 30–33 with
  their catalog cross-references and `:1526-1527` repeats them in the tag
  table. **The tables are authoritative; `:2373` is prose that stopped being
  maintained.** So this is not the Push-4a failure (an enumeration the
  decoder contradicted) but its milder cousin: a count-and-history sentence
  drifting from a listing that is correct.

  **The `.tex` correction is deferred to the G-minor implementation**, which
  must touch this paragraph anyway — it is the paragraph defining the
  minor-additive mechanism the rung implements. Recorded here so the fix is
  owed rather than rediscovered.

## Findings the plan's §3 table did not name

1. **The canonical base (`MaterializedState`) reaches more than the plan's
   §3 list states.** Plan §3 lists `OperationEffect, NoOpReason,
   PreconditionFailureReason, ConflictKind, IntegrityAnomalyKind,
   PendingReason, ObjectState, TypedObjectId` for the canonical base. Missing
   from that list, and confirmed reachable by this audit: **`RepairKind`**
   (via `OperationEffect::AppliedWithRepair`), **`ReanchorReason`** (nested
   inside `RepairKind::Reanchored` — and the one with a real post-baseline
   append, `SameCanvasNearer` = 6), and **`SpellingNominal`** (via the
   `spellings: BTreeMap<PitchId, PitchSpelling>` field). This is not a
   cosmetic omission: `ReanchorReason` is one of only two vocabularies this
   audit found with an actual native append, and it is missing from the
   plan's inventory of what the canonical base emits.
2. **The manifest role reaches `OperationKindTag`, `BarrierScope`, and
   `BarrierCondition` — none of which appear in the plan's §3 table at
   all.** The plan's table (§3) enumerates only `OperationKind`,
   `OperationKindTag`, `OperationPayload`, and "value-layer unions closed
   under `req:binfmt:frozen-layout`" as the four floor vocabularies, framed
   entirely around the *operation* layer. But `OperationKindTag` is also
   reachable through the **manifest**, independently of any operation
   envelope, via `ExtensionDeclaration::edit_barriers` →
   `EditBarrier::prohibited_operation_kinds: Vec<OperationKindTag>`
   (`layout-ir/src/barrier.rs:205`). A manifest that declares an edit barrier
   naming `CreateInstrument` (tag 31) needs the same G-minor stamping
   discipline as an operation-envelope block that contains one — and the
   manifest chunk's minor is a wholly separate stamping decision from any op
   block's. This is exactly the disposition-(c) crossing the contract warned
   would be "the single most important vocabulary in the whole matrix" if
   skipped, realized concretely, and it lands in a role the plan's table
   never mentions.
3. **Two roles are declared and named end-to-end but have no producer at
   all: `TextProjection` and `LayoutCache` (chunk-kind), plus
   `IntegrityIndex`.** This isn't in the plan's scope (it predates
   vocabulary concerns entirely), but it matters for whoever drafts the
   G-minor implementation contract next: there is currently no code path
   that needs an `introduced_minor` for these three roles, because there is
   no code path that writes them at all.
4. **`ConflictResolutionState` and `UndoPolicy`** are closed unions (no
   `Registered` escape) reachable from the canonical base and the operation
   block respectively, present in neither the plan's table nor the
   contract's sixteen-entry checklist (the checklist is escape-carriers
   only). Checked and clean, but worth naming since the contract's own
   framing ("every reachable discriminant vocabulary") is broader than its
   worked checklist.
5. **A same-named-type trap**: `layout_ir::barrier::ObjectKind` (a `u16`
   wrapper around `TypedObjectId::discriminant()`, no independent vocabulary)
   and `epiphany_ops::support::ObjectKind` (a three-variant enum,
   `Voice`/`Pitch`/`Registered`, feeding `IntegrityAnomalyId` derivation) are
   two unrelated types sharing one name across two crates. Neither the plan
   nor the contract distinguishes them; this audit treats them as separate
   ledger entries (§1.6 #16 and §1.7 respectively) to avoid the two being
   silently merged by a later reader.

## Where this audit's own working assumptions turned out to need correction

- **A vocabulary's "baseline" is not one format-wide date.** Reading
  `PreconditionFailureReason`'s baseline (locked at 0..=9, evidently before
  the M2c tranche) against `OperationKind`'s baseline (locked at 0..=23,
  evidently *including* M2c's `CreateRegion` et al.) shows two vocabularies
  whose own golden-lock moments are not simultaneous, even though one
  vocabulary's post-baseline variant (`ContainerNotEmpty` = 10) is produced
  by an operation (`CreateRegion` = kind 15) that is itself baseline in the
  *other* vocabulary. The contract's Part 1 definition ("baseline — present
  at the format's initial ratification") reads as if there is one
  initial-ratification instant; in practice each vocabulary's own
  code-documented golden lock is the operative boundary, and they disagree
  with each other by design (the M2c precedent shows *the same tranche* can
  be baseline for one vocabulary and a genuine append for another). This
  audit's ledger records introduction events per variant rather than
  asserting a single global baseline date, which is the only way the two
  observations above are both representable.
- **The sixteen-entry checklist, read carefully, already includes
  `ReanchorReason` and `PreconditionFailureReason` at positions 2 and 3** —
  an early pass through this audit almost treated the checklist as
  "fourteen escape-carriers plus these two extras," which would have been a
  double-count. Rereading the contract's list resolved this without any
  discrepancy in the final tally (still sixteen numbered entries, all
  checked); flagged here only because it was a live risk of self-inflicted
  miscounting during the audit, worth naming so a future reader doesn't
  repeat the confusion.
- **`OperationKindTag`'s reachability is not confined to the op layer.**
  The contract frames the audit around "vocabularies reachable from an
  affected chunk payload," in a document whose motivating example is entirely
  about operation envelopes and `ResolveEquivocation`. It would have been
  easy to conclude `OperationKindTag` is reachable *only* through
  `prohibited_operation_kinds` inside an op-adjacent structure and stop
  there. It is in fact reachable through the **manifest**, a chunk role with
  no operation envelope in it at all. Finding 2 above is the concrete
  correction; the general lesson is that "affected chunk payload" needed
  checking against all nine roles independently, not inferred from the one
  role (`OperationEnvelopeBlock`) the motivating example was about.

## What this audit hands to the epoch ratification (ruled 2026-07-28)

**The epoch space stays global** — one globally meaningful minor per additive
revision, as ratified in `PLAN_GMINOR_SCHEMA_MINOR.md` §4. This audit does not
reopen that.

**But baseline/post-baseline classification and introduction events are
established per variant, per vocabulary.** §1.5's M2c case is the proof rather
than the caution: the same tranche is baseline for `OperationKind` (absorbed
into the golden-locked 0..=23) and a genuine append for
`PreconditionFailureReason` (`ContainerNotEmpty` = 10, whose own lock at
0..=9 predates it). A global epoch *numbering* and a per-vocabulary *baseline
boundary* are not in tension — the epoch answers "which revision introduced
this variant," the boundary answers "was this variant present at that
vocabulary's own lock." Only the second varies by vocabulary.

**The tentative ladder is therefore incomplete.** `PLAN_GMINOR_SCHEMA_MINOR.md`
§4's Phase 3 → 2, repeats → 3, Push 4a → 4, G1 → 5, G2a → 6 was drawn against
`OperationKind` alone. Three families this audit exposed are unplaced:

| Family | Variants | Introduction event | Placement |
|---|---|---|---|
| `OperationPayload` | `ResolveEquivocation` = 3 | Push 3 | **unplaced** — Push 3 predates every rung on the tentative ladder |
| `ReanchorReason` | `SameCanvasNearer` = 6 | Pass-12 G-pass (P12-C4) | **unplaced** — Pass-12 G-pass has no rung |
| `PreconditionFailureReason` | 10–15 | M2c / Phase-3 / Pass-12 G-pass ×2 / Push 4a | **partly placeable** — Phase-3 and Push 4a map to existing rungs; M2c and the Pass-12 G-pass do not |

So ratification owes at least two new epochs (Push 3, Pass-12 G-pass) and a
decision on M2c, and must confirm that Phase-3 and Push 4a rungs cover their
`PreconditionFailureReason` contributions as well as their `OperationKind`
ones. **The ladder must not be ratified as written.**

## How completeness was checked (methodology note)

Two independent nets were run and cross-checked against each other:

1. `grep -rn "push_tag\|fn discriminant"` over `epiphany-core/src` and
   `epiphany-ops/src` (the contract's suggested net, ~65 hits), then every
   `fn discriminant` site outside those two crates (`epiphany-layout-ir`,
   `epiphany-bundle`) located separately, since the contract's net is scoped
   to only two crates but `BarrierScope`/`BarrierCondition` live in
   `epiphany-layout-ir`.
2. `grep -rn "Additive|appended|append at|append past|appended past"` across
   `epiphany-core/src`, `epiphany-ops/src`, `epiphany-layout-ir/src`,
   `epiphany-bundle/src` — every hit was read in context (§1.4, §1.5's
   sourcing, plus confirmation that every schema-major-2 "appended" comment
   found was a **struct field** addition, out of scope by the contract's own
   "field additions are always major" rule, not a discriminant append).

Every discriminant vocabulary named in the contract's checklist, the plan's
§3 table, and everything found via nets 1–2 was traced to a concrete
`encode_canonical` (or `Codec::enc`) call site and confirmed reachable from
at least one `ChunkKind` role's payload before being entered in Part 2. No
vocabulary was entered on the strength of its type declaration alone.

## Gate

`git status --short` **before**:

```
 M Cargo.toml
?? spikes/editor-toolkit/Cargo.lock
?? spikes/editor-toolkit/Cargo.toml
?? spikes/editor-toolkit/a11y-verifier/
?? spikes/editor-toolkit/probe-egui/
?? spikes/editor-toolkit/probe-iced/
?? spikes/editor-toolkit/probe-vello/
?? spikes/editor-toolkit/target/
```

`git status --short` **after** (expected — this file is the only addition):

```
 M Cargo.toml
?? spec/AUDIT_GMINOR_VOCABULARIES.md
?? spikes/editor-toolkit/Cargo.lock
?? spikes/editor-toolkit/Cargo.toml
?? spikes/editor-toolkit/a11y-verifier/
?? spikes/editor-toolkit/probe-egui/
?? spikes/editor-toolkit/probe-iced/
?? spikes/editor-toolkit/probe-vello/
?? spikes/editor-toolkit/target/
```

No Rust, `.tex`, vector, or golden file was modified.

**`cargo` was not invoked at any point during this audit** — no build, no
test run, no `fmt`. Every fact above was established by reading source and
spec/decision-log text.

**No test result is claimed by this packet.** `cargo fmt --check` and
`cargo test --workspace` were not run, so this document asserts nothing about
their state; it neither reports them green nor relies on their being so.

What the gate actually proves is the status delta above: the packet's only
effect on the working tree is the addition of
`?? spec/AUDIT_GMINOR_VOCABULARIES.md`. No source file, build input, or
`Cargo.lock` changed, which is what makes the read-only claim checkable
without running anything.
