# CONTRACT — Push 4b tranche 3b-i: the score wire (schema major 3)

**Status:** dispatch-ready. Ratified by the user 2026-07-23: (1) **split** 3b into
3b-i (this — the core score wire) then 3b-ii (the layout-ir `SmuflVersion`
unification + `GlyphCatalogIdentity` move); (2) **stage** — freeze `smufl` and
`overrides` on the major-3 wire now, **hold `accidental_extensions` in memory**
until its engrave consumer exists (a later major bump appends it).

This tranche opens **schema major 3** and is **irreversible**: every byte layout
it defines is frozen forever under `req:binfmt:frozen-layout`. 3b-ii is a
separate dispatch and MUST NOT be started here.

---

## What this tranche does, in one sentence

`ScoreTuningContext` grows two wire fields — `smufl` and `overrides` — moving them
from in-memory-only (tranches 2/3a) onto the canonical score wire; the reader
gains a v2→v3 migration; nothing else on the wire changes.

## The scope is much smaller than the original 3b sketch — two findings

1. **No operation embeds the tuning context.** A full-workspace search
   (`ScoreTuningContext`/`TuningOverride`/`tuning_context`/`SmuflVersionRequirement`
   across `epiphany-ops`) finds nothing. The tuning context reaches the wire
   **only** through the full-`Score` acceleration snapshot. Therefore:
   - The **operation-block minimal-stamping machinery is untouched.** No op
     payload is "born at v3"; no frozen v2 op-payload decoder is needed.
   - `max_supported_major(ChunkKind::OperationEnvelopeBlock)` **stays 2.** Only
     `ChunkKind::Snapshot` rises to 3.
   - The **canonical base** (`MaterializedState`) embeds no tuning context, so it
     stays major 0, byte-identical — the same keystone as majors 1 and 2.
2. **Staging removed the accidental subtree.** Only `smufl` +
   `overrides` are frozen. `accidental_extensions` and its whole subtree
   (`ScoreAccidentalExtensions`, `PitchSpaceModification`, `AccidentalEngraving`,
   `AnchorPoint`, …) stay in-memory-only, exactly as they are today. **Do not
   write a `Codec` for any of them.**

## The permanent decision: the frozen major-3 wire layout

`ScoreTuningContext` on the wire, **in this exact order** (append-after-existing
per the frozen-layout rule; the Rust struct's *declaration* order is decoupled
and stays as-is — the hand codec fixes wire order independently, per the note at
`graph.rs:1655`):

```
ScoreTuningContext(v3) =
    default_pitch_space  ⌢ default_tuning_system ⌢ reference   (the frozen v0..v2 prefix)
  ⌢ smufl                                                       (NEW — SmuflVersionRequirement)
  ⌢ overrides                                                   (NEW — Vec<TuningOverride>, u32 count)
```

`accidental_extensions` is **NOT** on the wire. When it lands at a future major it
appends **after** `overrides`. Update the doc comment at `graph.rs:1655-1661` to
say this (currently it claims the "eventual major-3 field order" is
`(accidental_extensions, smufl, overrides)` all together — now false: major 3 is
`smufl ⌢ overrides`, accidental_extensions is staged to a later major and appends
last).

### New leaf wire layouts (four `Codec` impls, all over types whose members
already encode — no transitive new codecs):

- `SmuflVersion` = `major` (u16 LE) ⌢ `minor_centi` (u16 LE).
- `SmuflVersionRequirement` = `minimum` (SmuflVersion) ⌢ `authored_against` (SmuflVersion).
- `TuningScope` = one discriminant byte ⌢ body:
  - `0` `Voice(VoiceId)`; `1` `Staff(StaffId)`; `2` `Region(RegionId)`;
  - `3` `Range { start: TimeAnchor, end: TimeAnchor, voices: VoiceSelector }`
    (fields in declaration order; `TimeAnchor` @ codec.rs:748, `VoiceSelector`
    @ codec.rs:1152 already encode).
- `TuningOverride` = `scope` (TuningScope) ⌢ `pitch_space` (Option<PitchSpaceId>)
  ⌢ `tuning_system` (Option<TuningSystemId>) ⌢ `reference` (Option<ReferencePitch>).
  (One presence byte per Option; the inner id/pitch types already encode.)

Match the codebase's conventions: LE integers, one discriminant byte per union,
`u32` counts/length prefixes, one presence byte per `Option`. Prefer
`struct_codec!` / the enum-codec macro where the shape allows; hand-write only
`TuningScope` if the macro can't express the `Range` struct variant. Every one of
these is **permanent** — get the field order right.

## The migration (v2 → v3)

Only `ScoreTuningContext` changes v2→v3; every other `Score` field is byte-identical.

1. **Update the live `impl Codec for ScoreTuningContext`** (codec.rs:1854): `enc`
   writes the 5 wire fields (3 existing ⌢ `smufl` ⌢ `overrides`); `dec` reads all
   5 and default-fills **only** `accidental_extensions: Vec::new()`. This is now
   the **v3** form. Rewrite the block comment above it (codec.rs:1838-1853) to
   describe the staged wire (smufl+overrides on; accidental_extensions off).

2. **Freeze the current 3-field form** as a named sub-codec:
   - `fn dec_tuning_context_v2(r) -> Result<ScoreTuningContext>` = today's exact
     3-field read, default-filling all three in-memory fields
     (`accidental_extensions`, `smufl`, `overrides`).
   - `fn enc_tuning_context_v2(ctx, out)` = writes exactly the 3 fields.

3. **Reroute the frozen v0/v1 score decoders** — the must-not-miss edit.
   `decode_v0_score:2544` and `decode_v1_score` currently read `tuning_context`
   via `Codec::dec` (the *live* codec). Now that the live codec is v3 (5 fields),
   both MUST read it via **`dec_tuning_context_v2`** instead — v0/v1/v2 bytes all
   carry the 3-field form. The existing `v0/v1_score_migrates_*` goldens
   (codec.rs:3583, 3645) will fail loudly if this is missed.

4. **Add the v2 frozen score pair**, mirroring `encode_v1_score`/`decode_v1_score`
   (codec.rs:3077/3106):
   - `pub(crate) fn encode_v2_score(s) -> Vec<u8>`: the live walk for all 18
     other fields; `enc_tuning_context_v2` for `tuning_context`.
   - `fn decode_v2_score(bytes) -> Result<Score>`: the live walk for all 18 other
     fields; `dec_tuning_context_v2` for `tuning_context`. Strict-canonical:
     re-encode via `encode_v2_score` and reject on mismatch (exactly as
     `decode_v1_score` does at :3149).

5. **`decode_canonical_versioned`** (codec.rs:2492): `3 => Score::decode_canonical`
   (the live v3), `2 => decode_v2_score`, `1 => decode_v1_score`,
   `0 => decode_v0_score`. Update its doc to name major 3 as current.

6. **New migration golden** `v2_score_migrates_default_filling_smufl_and_overrides`
   (mirror :3583/:3645): synthesize v2 bytes via `encode_v2_score`, decode via
   `decode_v2_score`, assert the score reconstructs with `smufl` at default and
   `overrides` empty. (accidental_extensions is unaffected — it was never on the
   wire.)

## Version + bundle + accept-set

- `SchemaVersion::V3 = { major: 3, minor: 0 }` in `epiphany-bundle/src/ids.rs`
  (mirror V2 @ :189, with a doc line).
- `max_supported_major` (bundle.rs:65): **`Snapshot => 3`**;
  **`OperationEnvelopeBlock` stays `2`** (no op embeds a v3 value).
- `assert_score_serialization_stable` (testkit roundtrip.rs:352, :391, :403): the
  acceleration snapshot stamps the **current** major — flip `for_major(2)` →
  `for_major(3)` at both the stamp and the assert; the read-back seam keys off the
  stamped major and needs no change beyond that.
- bundle.rs tests (~1314–1374): add `assert_eq!(SchemaVersion::V3.major, 3)`;
  `max_supported_major(Snapshot) == 3` (was 2), `OperationEnvelopeBlock == 2`
  (unchanged). The `UnsupportedCanonicalChunkMajor { schema_major: 3 }` case
  (bundle.rs:1356) tests a **canonical base** stamped at an unsupported major —
  the base is always major 0, so major 3 is still unsupported *for the base role*.
  **Read what that test constructs** before touching it: if it asserts "a
  canonical base at major 3 is rejected", it stays valid as-is (base ≠ snapshot);
  if it was standing in for "beyond the accept-set", bump its value to 4. Do not
  guess — inspect and preserve its intent.

## Test inversions — partial, and that is the point

The off-the-wire test `score_tuning_context_accidental_extensions_smufl_and_overrides_do_not_reach_the_wire`
(codec.rs:3457) must become a **staging-boundary** test:
- `smufl` and `overrides` now **round-trip** through `enc`→`dec` (set them
  non-default, encode, decode, assert they survive equal).
- `accidental_extensions` is still **dropped** (set it non-empty, encode, decode,
  assert it comes back empty). This proves the staging line exactly.
Rename it to reflect the new meaning (e.g.
`score_tuning_context_smufl_and_overrides_reach_the_wire_accidental_extensions_do_not`).
The sibling `score_tuning_context_overrides_do_not_reach_the_wire`, if it still
exists separately, folds into the above or inverts to prove round-trip.

**Text projection is a separate surface — do NOT change it here.** The
`textvalue_graph.rs` analogues (`..._do_not_project`) stay as-is: all three
fields still do not project to text. (Binary-wire persistence without
text-projection parity is a known asymmetry I am flagging to the user as a
possible follow-up, not fixing in this binary-wire tranche.)

## Spec — binary_format.tex §"Schema Major 3"

Add a `\section{Schema Major 3}` mirroring §Schema Major 2's structure
(spec/binary_format.tex:2574):
- **What it adds:** `ScoreTuningContext` gains `smufl` and `overrides` on the
  wire (Chapter 4 tuning context reaching the canonical form); state plainly that
  `accidental_extensions` is **staged** to a later major.
- **Where the changed fields reach:** the acceleration full-`Score` snapshot
  **only**. No operation payload embeds the tuning context, so the canonical
  operation layer is untouched and the canonical base stays major 0,
  byte-identical (assert-again keystone). The snapshot role's accept-set max
  rises to 3; the op-block role stays 2.
- **Cross-major reader behaviour:** composes — a major-3 reader migrates
  v0→v1→v2→v3 in one read, each step total and default-filling.
- **Changed value layouts:** `ScoreTuningContext = (v0..v2 prefix) ⌢ smufl ⌢
  overrides`, and the four new leaf layouts (`SmuflVersion`,
  `SmuflVersionRequirement`, `TuningScope`, `TuningOverride`) exactly as frozen
  above.
- Update the accept-set section (§2474) if it enumerates per-role maxima.

This is a wire-layout ratification, not new normative behaviour: **do not add
`req:` labels.** The requirement counts (212 / 282 / 282) MUST be unchanged — the
gate asserts this.

## Do NOT touch

- **`accidental_extensions` and the entire accidental subtree** — stays
  in-memory-only. No `Codec`, no wire bytes.
- **`epiphany-layout-ir`** — the `SmuflVersion` unification and
  `GlyphCatalogIdentity` move are **tranche 3b-ii**, a separate dispatch. Leave
  layout-ir's own `SmuflVersion { major, minor }` and `encode_catalog` exactly as
  they are. (The two `SmuflVersion` homonyms remain a deliberate bounded homonym
  until 3b-ii, as `accidental.rs:251` already documents.)
- **Text projection** (`textvalue_graph.rs`, `epiphany-textproj`) — unchanged.
- **The operation layer** (`epiphany-ops`) — unchanged (no op embeds the context).
- **`spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_T1A_GOLDENS.md`** — untracked
  parallel work; MUST NOT be touched or staged. Stage only `epiphany-core/`,
  `epiphany-bundle/`, `epiphany-testkit/`, and `spec/binary_format.tex` +
  `spec/CONTRACT_PUSH4B_3BI_WIRE.md`.

## The gate (all must pass; report exact numbers)

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets` → 0 warnings
- `cargo test --workspace` → 0 failed
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
- `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8
- `cargo run -q -p epiphany-testkit --example requirement_labels` → 6/6, counts
  **212 / 282 / 282** (UNCHANGED — a changed count means a stray `req:` label slipped in)

## What I (the reviewer) will verify independently before committing — build to survive it

- Encode a `ScoreTuningContext` with non-default `smufl` and a non-empty
  `overrides`, round-trip through `Score::canonical_bytes()` / `decode_canonical`,
  and confirm both survive **and** `accidental_extensions` is dropped.
- Synthesize real v2 bytes via `encode_v2_score`, decode via
  `decode_canonical_versioned(.., 2)`, confirm default-fill; do the same for v1/v0
  to prove the reroute of the frozen decoders holds (mutation: break the reroute,
  watch a `*_migrates_*` golden fail).
- Confirm `OperationEnvelopeBlock` max is still 2 and the canonical base is still
  major 0 byte-identical across the bump.
- Mutation-verify the staging-boundary test: weaken it to also accept
  `accidental_extensions` surviving, confirm it then fails.
