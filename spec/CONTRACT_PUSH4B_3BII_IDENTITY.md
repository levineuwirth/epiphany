# CONTRACT — Push 4b tranche 3b-ii: the SmuflVersion unification and the GlyphCatalogIdentity move

**Status:** dispatch-ready. Closes P13-S12. Follows tranche 3b-i (68b08ad), which
opened schema major 3.

**Ratified by the user 2026-07-23:**
1. The unified type **stays in `epiphany_core::accidental`**; `epiphany-layout-ir`
   imports `epiphany_core::SmuflVersion` (already re-exported at core's root,
   `lib.rs:128`). Do **not** move the type between core modules.
2. The core-spec definition is a **type listing with normative prose, no new
   `\label`**. Requirement counts MUST stay **212 / 282 / 282**.

---

## What this tranche does

`epiphany-layout-ir` has its own `SmuflVersion { major: u16, minor: u16 }`
(`glyph.rs:29`) storing the **literal** minor digit — `{1, 4}` for SMuFL 1.4.
Derived `Ord` on a literal minor orders 1.3 and 1.4 **before** 1.12, which is
backwards versus SMuFL's real release history (1.12 → 1.18 → 1.20 → 1.3 → 1.4).
**That bug is live in layout-ir today**, and the type is a direct field of
`GlyphCatalogIdentity` — layout-conformance identity.

Tranche 3a defined the correct type in core (`minor_centi`, fraction-normalized
to hundredths) and deliberately left layout-ir's alone as a bounded homonym.
This tranche **unifies them**: layout-ir deletes its own and uses core's.

The crate graph forces the direction — `layout-ir` depends on `core`
(`Cargo.toml:12`); core depends only on `determinism`. Core's type is
necessarily the survivor.

## Blast radius: NO golden and NO vector moves. Do not go looking for any.

The reviewer verified this before dispatch, and it **corrects the P13-S12 ledger
entry**, which says the move lands "with golden/vector regen":

- Every assertion on `ResolvedLayoutIR::canonical_bytes()` is **relative**:
  stability (`resolved.rs:420`), determinism (`reference_suite.rs:228-230`), and
  sensitivity — which mutates `metrics_hash[0] ^= 1` (`resolved.rs:477`) and
  never touches `smufl_version`. **No absolute hash or byte literal is pinned
  anywhere in the workspace.**
- The committed SVG goldens (`epiphany-render-svg/tests/golden/`) and PNG
  baselines (`epiphany-editor-gui/goldens/`) do **not** embed the catalog
  identity. Their only "SMuFL" is a fixed comment string in the SVG preamble.
  **Conformance gate [9] and the editor track are untouched.**
- `ChunkKind::LayoutCache` is a regenerable major-0 role — discard-and-regenerate.
  No migration, no schema major, **no wire change of any kind in this tranche.**

The encoded catalog bytes do change in *value* (`encode_catalog` writes the minor
as `04 00` today and `28 00` after), and that is expected and unpinned. If a
golden or a byte-literal assertion *does* fail, **stop and report it** — it would
mean the reviewer's analysis missed a pin, and it must not be "fixed" by
re-copying bytes.

## The surface

**`crates/epiphany-layout-ir/src/glyph.rs`**
- `:29` — delete `pub struct SmuflVersion { major, minor }`.
- Re-export or `use epiphany_core::SmuflVersion` so the name still resolves
  in-crate. Keep the doc note at `:27` (Chapter 7/9 role) attached to the
  re-export, and drop the now-stale "This is not epiphany_layout_ir::SmuflVersion"
  paragraph in `epiphany-core/src/accidental.rs:251-258` — the homonym is gone.
- `:90` — `GlyphCatalogIdentity.smufl_version` now takes core's type.
- `:120` (`Default`), `:332` (`BravuraCatalog::smufl_version`), `:519` (the test
  catalog impl) — `SmuflVersion { major: 1, minor: 4 }` becomes
  `SmuflVersion::from_decimal(1, "4").expect("1.4 is a valid SMuFL version")`,
  i.e. `minor_centi: 40`. Use `from_decimal`, not a hand-written
  `minor_centi: 40` — the field's doc forbids literal construction precisely
  because `4` and `40` look interchangeable and are not.
- `:310` — the `GlyphCatalog::smufl_version()` trait method return type.

**`crates/epiphany-layout-ir/src/resolved.rs:364`** — `c.smufl_version.minor`
→ `.minor_centi`. This is the one line that changes emitted bytes.

**`crates/epiphany-layout-ir/src/solver.rs:769`** — `smufl_version.minor += 1`
→ `.minor_centi += 1`, inside `forged_catalog_metadata_is_rejected`. Intent is
"a catalog identity that does not match the bundled Bravura is rejected"; `+= 1`
still denotes a different (nonsense) version, which is the point. Preserve it.

**`crates/epiphany-layout-ir/src/lib.rs:113`** — **keep** `SmuflVersion` in the
public re-export list, now aliasing core's type. This keeps
`epiphany_layout_ir::SmuflVersion` compiling for every downstream user, so
downstream churn is zero.

**`crates/epiphany-testkit/src/layout_stub.rs:798`** — `gen_smufl_version`
currently emits `minor: rng.range(0, 6)`, which as *hundredths* would mean
versions 1.00–1.05 — not real SMuFL versions. Generate meaningful normalized
values instead (e.g. sample from the real release set {12, 18, 20, 30, 40}, or
scale a small digit by 10). Say in a comment why.

**A `const` contingency:** `from_decimal` returns `Option` and is not `const`.
Every current site is a function body, so this does not arise. If you find a site
that genuinely needs a `const`, add a `const fn` constructor to
`epiphany_core::accidental::SmuflVersion` rather than hand-writing
`minor_centi: 40` at the call site — and say so in your report.

## Spec — `spec/core_spec.tex`

`SmuflVersion` is currently **defined nowhere in core_spec.tex**: only
`SmuflVersionRequirement` (`:3269`, whose two fields are of that type) and two
Chapter 9 usages — `GlyphCatalog::smufl_version()` (`:10420`) and
`GlyphCatalogIdentity.smufl_version` (`:10460`). P13-S12's ratified shape reached
Rust and the Binary Format companion but never the core specification. This
tranche closes that — it is the last open half of S12.

Add **one** definition (near `SmuflVersionRequirement` at `:3269`, the Chapter 4
site that first needs it) as a type listing:

```
pub struct SmuflVersion {
    pub major: u16,
    /// The fractional part, normalized to hundredths.
    pub minor_centi: u16,
}
```

with normative prose stating: the minor is stored **fraction-normalized to
hundredths** — 1.12 → 12, 1.18 → 18, 1.20 → 20, 1.3 → 30, 1.4 → 40 — so that the
derived ordering on `(major, minor_centi)` matches SMuFL's real release order;
that a literal-digit encoding would order 1.3 and 1.4 before 1.12 and is
non-conforming; and that the normalization collapses the 1.2/1.20 ambiguity (both
→ 20). Include the release table as an explanatory note, not as normative text.

Make the two Chapter 9 sites reference this single definition (a cross-reference
sentence is enough) so it is unambiguous that the glyph catalog's version and the
tuning context's version are **the same type**.

**No new `\label{req:...}`.** The counts MUST remain 212/282/282 — the gate
asserts it. The wire-level rule is already normative in
`binary_format.tex:2963-2967`; this is the core-side type definition, not a new
requirement.

## Ledger — `spec/PASS13_CANDIDATES.md`

P13-S12's resolution text says the unification lands "with golden/vector regen."
Verified false before dispatch (see *Blast radius*). Correct that clause to state
that no golden or vector is pinned to the catalog identity, so the move changes
emitted bytes with nothing to regenerate. This is the S9 discipline the ledger
already applies to itself — a claim that reads cleanly but is not supported.

## Do NOT touch

- **Any wire format.** 3b-ii changes no schema major, no `Codec`, no
  `decode_vectors.txt`. `epiphany-core`'s freshly frozen major-3 layout is
  off-limits except for deleting the stale homonym paragraph in `accidental.rs`.
- **The editor track**: `crates/epiphany-editor-gui/goldens/*.png`,
  `spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_T1A_GOLDENS.md`,
  `spec/CONTRACT_EDITOR_T2_SELECTION.md`. Parallel work; not yours.
- The committed SVG goldens under `crates/epiphany-render-svg/tests/golden/`.
- `.claude/worktrees/` — agent worktrees, not the repo.

## The gate (report exact numbers)

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets` → 0 warnings
- `cargo test --workspace` → 0 failed (report the pass count; 3b-i landed at 1282)
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` → 0
- `cargo run -q -p epiphany-testkit --example conformance_suite` → 8/8
- `cargo test -p epiphany-testkit --test requirement_labels` → 6/6, counts
  **212 / 282 / 282** unchanged

## Deliver a test that would have caught the live bug

The whole point of this tranche is that layout-ir's ordering was wrong. Add a
test **in layout-ir** proving the catalog's version type now orders the real
release sequence correctly (1.12 < 1.18 < 1.20 < 1.3 < 1.4) — the assertion that
fails against the deleted literal-minor type. A unification that silently fixed a
bug with no test naming it is not finished.

## What the reviewer will verify independently — build to survive it

- The emitted catalog bytes changed at exactly the expected offset
  (`04 00` → `28 00`) and nowhere else, by encoding a default
  `GlyphCatalogIdentity` before and after.
- The ordering bug is genuinely dead: the new test fails if the comparison is
  performed on literal minors.
- No committed golden, baseline, or vector file changed (`git status` over
  `goldens/`, `tests/golden/`, `spec/vectors/`).
- `epiphany_layout_ir::SmuflVersion` still resolves for downstream crates.
- Requirement counts unmoved, and no `\label{req:` added to the spec diff.
