# CONTRACT — Push 4b: `ji-adaptive-5limit`, the last fail-closed tuning system

**Status:** dispatch-ready. Closes the final deferred entry in the built-in
tuning catalog (`req:tuning:builtin-tuning-catalog`), leaving only the
compatibility-mapping registry open in Push 4b.

**Ratified by the user 2026-07-23:**
1. **A minimal `HarmonicContext`** carrying only the tonal centre — not the full
   four-field spec listing, and not "no struct at all".
2. **One resolver with an optional context parameter** — static systems ignore
   it, adaptive consumes it. This is the spec's own stated API shape.
3. **Constrain and fail closed** on anchor ordering: order what is
   unambiguously determinable, return a distinct reported error otherwise.
   Never guess.

**Nothing in this tranche reaches the wire.** `TuningResolution` appears nowhere
in `codec.rs` or `textvalue_graph.rs` — the score stores only *identifiers*
(`default_tuning_system`, `overrides`). `TuningResolution::Adaptive` and
`HarmonicContext` are in-memory only and fully reversible. Do **not** add a
`Codec` for anything here, and do not touch schema major 3.

---

## Correct the recorded blocker first

`tuning.rs` says in three places that adaptive "needs `HarmonicContext`, which
does not exist in Rust". That framing is wrong and should be fixed as you go:

- `req:tuning:adaptive-default-version` (`core_spec.tex:3438`) makes version 1
  **"a pure function of (position, anchor pitch class)"**. It explicitly
  *ignores* `concurrent`, `recent`, `hints`, `parameters`, and mode.
- **Two of `HarmonicContext`'s four spec'd fields are unimplementable by
  construction**: `key_context: Option<KeyContext>` and `hints: Vec<ContextHint>`
  name types the specification deliberately leaves undefined
  (`core_spec.tex:4111`, Forward References: "defining them now would freeze a
  type surface on a chapter with no consumer").

So the minimal shape is not merely preferable — it is the only implementable
one. What v1 needs is a single chromatic pitch class.

## What already exists — do not rebuild it

`ji_static_5limit_ratios(anchor_chromatic_degree: i32) -> Vec<PositionRatio>`
(`tuning.rs:358`) **already takes a runtime anchor**. The three
`ji-static-5limit-{C,G,D}` entries are that one function at anchors 0, 7, and 2.

`req:tuning:adaptive-default-version`'s first clause — "the position resolves
through the construction of `req:tuning:ji-static-construction`, transposed so
the anchor takes the role of 1/1" — **is exactly that call with a derived
anchor**. Reuse it. Do not write a second lattice construction, and do not
transcribe a cents table.

## The surface

### 1. `AdaptiveTuningFunctionId`

Mint via the existing `catalog_id!` macro in `pitch.rs` (beside
`TuningFunctionId`, `pitch.rs:92`), with a doc comment stating that
`ji-adaptive-5limit` is bound to `"default-v1"`, that the version lives *inside*
the identifier string, and that an unregistered id is a hard error with no
silent fallback (`req:tuning:adaptive-default-version`, final clause).

### 2. `TuningResolution::Adaptive`

Add the variant the specification names:

```rust
Adaptive { function: AdaptiveTuningFunctionId },
```

In-memory only. Follow the `Function` variant's doc style (`tuning.rs:119-134`):
say what resolves it, and that an unregistered id fails closed.

### 3. `HarmonicContext` — minimal, and say why

```rust
pub struct HarmonicContext {
    /// The active tonal centre as a chromatic pitch class (0..=11), if known.
    pub tonal_centre: Option<ChromaticPitchClass>,
}
```

The doc comment MUST record why this is not the four-field listing at
`core_spec.tex:3411`: `key_context` and `hints` are typed on `KeyContext` /
`ContextHint`, which the specification leaves undefined on purpose; `concurrent`
and `recent` are ignored by version 1, so carrying them would mint an unconsumed
type surface (the `NOTEHEAD_ANCHORS` failure the module doc at `tuning.rs:25`
already cites). Each field arrives with the first function that consumes it.

For the pitch class, enforce the `0..=11` invariant in the type rather than
merely documenting it — follow `KeySignature::new`'s checked-constructor style
(`graph.rs:156`, returns `Option`). A raw `u8` field a caller can set to 200 is
not acceptable. (`Pitch::chromatic_pitch_class`, `pitch.rs:585`, already returns
a `0..=11` `u8`; a small checked newtype in `pitch.rs` beside it is the natural
home, and lets that method's contract be stated in the type.)

### 4. The anchor derivation — `req:tuning:adaptive-anchor-derivation`

A public function deriving the tonal centre from the score graph. Per the
requirement (`core_spec.tex:3474-3489`):

- `anchor_pc = (7 × fifths).rem_euclid(12)`, reading `fifths` as the **major
  tonic**. **Mode is not consulted** — a signature of 0 anchors at C whether the
  prevailing key is C major or A minor. Use `rem_euclid`, never `%`: `fifths` is
  negative for flat keys (`KeySignature::MIN_FIFTHS == -7`) and `%` would yield a
  negative pitch class.
- The *prevailing* signature is the one **on the staff containing the pitch**,
  from that staff instance's `key_sequence` (`StaffInstance.key_sequence`,
  `graph.rs:607`), at the latest `KeySignatureChange` whose anchor is **at or
  before the pitch's onset**.
- No applicable change ⇒ no tonal centre (the resolver then defaults to C; see
  §5).

**`locate_voice` (`tuning.rs:1081`) currently returns `(RegionId, StaffId)` and
throws away the `StaffInstance` it already has in hand.** `key_sequence` lives on
the *instance*, not on `Staff`. Widen it (or add a sibling) to return the
instance. This is the single structural blocker that made adaptive resolution
impossible before.

**Ordering — constrain and fail closed.** `KeySignatureChange.anchor` is a
`TimeAnchor` (Event / Measure / Region / WallClock, `time.rs:592`) and an event's
onset is an `EventPosition` (`Musical` | `WallClock`, `time.rs:628`). Order the
cases that are unambiguously determinable, and return a **distinct, reported
error** for the rest (cross-clock comparison, or an indirect anchor you cannot
resolve) rather than guessing an ordering. Precedent for anchor resolution, which
you should follow rather than reinvent: `tempo.rs:331/356/376` inject a
`resolve: impl Fn(&TimeAnchor) -> Option<MusicalPosition>` closure, and
`invariants.rs:407` walks anchors depth-bounded. Reusing `tempo.rs`'s injected
shape is preferred — it keeps this module out of the time-model business.

Name the error for what it is (e.g. `AnchorNotOrderable { .. }`), document that
it is a *deliberate deferral* like `IncompatiblePitchSpace`, and make the message
say which anchor kind defeated it. A caller must be able to tell "this score
uses a form I don't order yet" from "your key signature is wrong".

### 5. The resolver seam

```rust
pub fn resolve_pitch_frequency(
    score: &Score,
    pitch: &Pitch,
    voice: VoiceId,
    context: Option<&HarmonicContext>,
) -> Result<f64, TuningResolutionError>
```

`resolve_pitch_frequency` has **no production callers** — only its own tests
(verified across the workspace), so this signature change is nearly free. Update
the five in-module test call sites and the `lib.rs:170` re-export docs.

Resolution of `TuningResolution::Adaptive`:
1. If `function` is not the registered `"default-v1"` ⇒ **hard error**. No
   fallback (`req:tuning:adaptive-default-version`).
2. Anchor = `context`'s `tonal_centre` when supplied; **otherwise C, chromatic
   position 0**. This is spec-mandated (`core_spec.tex:3452-3453`: "C (chromatic
   position 0) when no tonal centre is supplied") — it is a *defined default*,
   **not** a fail-closed case. Do not turn a missing context into an error.
3. Resolve the position through `ji_static_5limit_ratios(anchor)`.

Static systems ignore `context` entirely — that is the spec's shape
(`core_spec.tex:3426-3428`).

### 6. Catalog entry

`built_in_tuning_system` (`tuning.rs:326`) currently returns
`TuningCatalogEntry::Deferred(DEFERRED_ADAPTIVE)` for `"ji-adaptive-5limit"`.
It becomes `Resolved(..)` with `resolution: Adaptive { function: "default-v1" }`
over pitch space `cmn-12`. Remove `DEFERRED_ADAPTIVE` and retire the stale
"needs HarmonicContext" notes at `tuning.rs:27`, `:43`, `:223`, `:279`, `:1386`.

The test `deferred_ji_adaptive_fails_closed` (`tuning.rs:1382`) **inverts**: it
must now prove the system resolves. Rename it accordingly. Do not delete it.

## Tests that would fail against the bugs this invites

- **Comma-drift-free by shape.** `req:tuning:adaptive-default-version`'s last
  clause: "No adjustment is ever carried forward from a previous resolution."
  Resolve the same position repeatedly, and in varying orders, and assert
  bit-identical results — the property that would catch a stateful implementation.
- **Mode-blindness.** A signature of 0 anchors at C. Assert a relative-minor
  reading is *not* applied.
- **Flat keys.** `fifths = -1` ⇒ `(7 × -1).rem_euclid(12) = 5` (F). A `%`
  implementation yields `-7` — assert the correct value so the sign bug cannot
  survive.
- **The default.** No context, or a context with `tonal_centre: None`, resolves
  at anchor C — equal to `ji-static-5limit-C`'s result for the same position.
  This pins the spec's mandated default against a future "fail closed" refactor.
- **Unregistered function id is a hard error**, with no fallback to C.
- **Prevailing selection.** With two `KeySignatureChange`s on a staff, a pitch
  between them takes the earlier; a pitch after both takes the later.

## Do NOT

- Define `KeyContext`, `ContextHint`, or `AdaptiveTuningParameters` — the
  specification leaves all three undefined deliberately.
- Add `concurrent` / `recent` to `HarmonicContext`.
- Add any `Codec`, touch schema major 3, or alter `spec/vectors/`.
- Build a general `TimeAnchor` resolution layer — constrain and fail closed.
- Touch the editor track (`spec/PLAN_EDITOR_APP.md`,
  `spec/CONTRACT_EDITOR_T1A_GOLDENS.md`, `spec/CONTRACT_EDITOR_T2_SELECTION.md`,
  `crates/epiphany-editor-gui/goldens/`) or `.claude/worktrees/`.

## Spec

The two requirements are already ratified and need **no change**. If a
`core_spec.tex` touch is genuinely needed (e.g. a note that the implementation's
`HarmonicContext` carries only the tonal centre until a consumer needs more), add
it as prose or a `rationale` — **no new `\label{req:...}`**. Requirement counts
MUST stay **212 / 282 / 282**.

## The gate (report exact numbers)

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets` → 0 warnings
- `cargo test --workspace` → 0 failed (3b-ii landed at 1283; report the count)
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
- `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8
- `cargo test -p epiphany-testkit --test requirement_labels` → 6/6, counts
  **212 / 282 / 282**

## What the reviewer will verify independently — build to survive it

- The anchor arithmetic by hand across the full `-7..=7` range, especially flat
  keys, against `(7 × fifths).rem_euclid(12)`.
- That adaptive at anchor 0 equals `ji-static-5limit-C` position-for-position,
  and at anchor 7 equals `ji-static-5limit-G` — the transposition identity.
- That the missing-context default is C and is **not** an error.
- Mutation: break `rem_euclid` to `%` and confirm the flat-key test dies; make
  the unregistered-id path fall back to C and confirm the hard-error test dies.
- That no `Codec` was added and canonical bytes are unmoved.
