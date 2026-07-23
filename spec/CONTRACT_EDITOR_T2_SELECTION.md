# Contract: Editor T2 — selection v2, the golden gate, and copy/paste

Repo root `/home/jeans/Repos/active/epiphany`. The plan is
`spec/PLAN_EDITOR_APP.md` (T2 ladder entry; Ruling E for W4). T1a landed at
`6be14d2`; the Push-4b resolver freeze is over, so `epiphany-core` is no
longer off-limits — but nothing in T2 needs it, and touching it remains
forbidden here (tranche 3b may claim it; coordinate through the plan, not by
collision). Requirement counts stay **212 / 282 / 282**; no `.tex` moves.

Execution model as T1a: Sonnet subagents per work packet, file-scoped briefs,
coordinator line-level review with independent mutation re-runs, user
deep-dives at contract sign-off, any golden/baseline change, and the final
report. **Mutation discipline throughout:** anchor-assert before
substituting, restore by reversing, never `git checkout`.

## Work packets

Dependency order: W1 → W2, W1 + Ruling-E grant → W4. W3 is independent and
runs parallel to W1.

### W1 — selection v2 in `epiphany-editor-core` (dispatchable now)

Blast radius: `crates/epiphany-editor-core/**` only.

* `Option<Selection>` becomes a **selection set with an anchor**: members in
  paint order, one member the anchor (the reference point for single-target
  intents and future paste placement). Public surface (names indicative, the
  crate's idiom wins): `selections() -> &[Selection]`, `anchor() ->
  Option<&Selection>`, `click(point)` = replace-with-single (today's
  behavior), `toggle_at(point)` = add/remove a member (anchor = last added),
  `select_within(rect)` = the hit-test `within(rect)` result as the set
  (anchor = first in paint order), `clear_selection()`.
* **Existing single-target intents retarget to the anchor** (alter, move,
  add-chord-note, insert-after, set-duration): behavior with a single
  selection is byte-identical to today — locked by the existing tests
  continuing to pass unmodified where they can, and updated mechanically
  where the accessor changed.
* **Batch intents, atomic:** `delete_selection` deletes **every** member in
  ONE transaction (all-or-nothing under the commit gate — a member whose
  delete is refused rolls back the entire batch); `alter_selection(±1)`
  likewise over all pitch members. The transaction path is
  `apply_transaction`, already atomic.
* **Selection preservation across relayout** extends to sets: members whose
  ids survive stay selected; vanished members drop; the anchor falls to the
  next surviving member (documented rule, tested).
* Proof-of-life tests (value-asserting) + mutations, minimum: (m1) batch
  delete of N members leaves the score with exactly the expected survivors —
  mutation: make the batch loop skip the last member → test dies on the
  survivor count; (m2) **atomicity**: a range containing one refusal-worthy
  member (e.g., a tuplet member whose `DeleteEvent` refuses `NotInTuplet`)
  rolls back the whole batch, score byte-identical to before — mutation:
  drop the transaction wrapper (apply members individually) → test dies;
  (m3) anchor-fallback rule — mutation: make the anchor fall to a dropped
  member → test dies; (m4) `select_within` paint order — mutation: reverse
  the order → test dies.

### W2 — GUI rubber-band select (after W1 review)

Blast radius: `crates/epiphany-editor-gui/src/main.rs` (+ `goldens.rs` only
if a compile fix is needed — **no golden pixel may change**; the score raster
is untouched by selection, which is egui-overlay-side).

* Drag on the score view → world-space rect via `ViewMap` → W1's
  `select_within`; plain click keeps today's behavior; Ctrl/Cmd-click →
  `toggle_at`.
* Overlay draws every member's highlight, the anchor visually distinct;
  debug panel shows member count + anchor.
* Toolbar/keys unchanged — they now act on anchor (single-target) or the
  whole set (delete/transpose), matching W1's semantics.
* The T1a goldens must pass **unchanged** — that is this packet's regression
  gate, stated in its brief.

### W3 — the `[9/9]` golden conformance gate (dispatchable now, parallel to W1)

Blast radius: `crates/epiphany-testkit/**` (Cargo.toml + the
`conformance_suite` example + CONFORMANCE.md at repo root),
`.github/workflows/ci.yml` (the conformance job's invocation only),
`Cargo.lock`.

* **The MSRV constraint decides the design** (probed): the conformance CI
  job runs on `PINNED_STABLE` (`ci.yml:146-155`), but the MSRV `test` job
  builds testkit's examples with `--all-targets`, so `resvg` must never be
  an unconditional dependency of testkit. Therefore: a **`golden-gate`
  feature** on `epiphany-testkit`; `resvg` as an **optional regular
  dependency** enabled only by that feature (dev-dependencies cannot be
  optional); the gate code inside the `conformance_suite` example behind
  `#[cfg(feature = "golden-gate")]`. Without the feature the suite builds
  and prints **exactly today's 8/8** (byte-identical output — MSRV coverage
  regresses zero); with it, **9/9**.
* **The gate itself:** re-render the three T1a baseline states headlessly
  (`ten_measure_single_staff(0)` as opened; the scripted insert re-derived
  through the session inverses exactly as `goldens.rs` does; 
  `ten_measure_with_slurs(0)`), rasterize via `resvg`, decode the committed
  baselines from `crates/epiphany-editor-gui/goldens/` (path resolved
  relative to `CARGO_MANIFEST_DIR`, the cross-crate coupling documented in
  both DECISIONS files), and compare **decoded RGBA** (the Ruling-C
  contract; compare-only — no bless, no artifacts; on failure the gate
  message points at `cargo test -p epiphany-editor-gui goldens` for the
  diagnostic surface).
* `ci.yml`: the conformance job's suite invocation gains
  `--features golden-gate`. Nothing else in the file moves.
* `CONFORMANCE.md`: documents gate [9], the feature split, and why (the
  MSRV closure cannot carry the raster stack).
* Verification: demonstrate `resvg` is absent from the no-feature dependency
  graph (`cargo tree -p epiphany-testkit` vs `--features golden-gate`);
  mutations: (m5) tamper one decoded pixel before comparison → gate 9 must
  fail; (m6) point the gate at a nonexistent baselines dir → must fail
  loudly, not skip silently.
* **The count moves 8/8 → 9/9 deliberately** (plan T2). All standing
  documents that pin 8/8 are historical contracts of landed tranches; the
  new normative number lives in CONFORMANCE.md, and future contracts pin
  9/9.

### W4 — copy/paste over the fragment projection (**blocked on Ruling E's grant**)

Blast radius: `crates/epiphany-editor-core/**` (fragment encode/decode +
`copy_selection`/`paste_at`/`paste_over_selection`),
`crates/epiphany-editor-gui/src/main.rs` (clipboard wiring via egui's
text-clipboard output — no new dependency).

Contracted in detail only after Ruling E is granted; its brief will carry
the ruling verbatim plus: round-trip value tests (copy→paste reproduces the
selected values at the destination with fresh ids), the closure fail-closed
tests (partial tuplet refuses; boundary tie/slur dropped-and-reported), the
untrusted-input cap tests, and paste atomicity mutations.

## Gate (every packet, actual output)

The six standard commands. Expected numbers: counts 212/282/282; conformance
**8/8 until W3 lands, 9/9 after** (each packet's brief states which applies);
T1a's three baselines byte-identical throughout (no re-bless of anything
existing); `cargo test --workspace` 0 failed; clippy 0 warnings.

## Do not

* Touch `epiphany-core`, any `.tex`, or any crate outside the packet's blast
  radius. Do not re-bless or alter any T1a baseline. Do not add `resvg`
  outside W3's optional feature. Do not start W4 before Ruling E is granted.
* Restore a mutation with `git checkout`.

## Report

Per packet, as T1a: files + diffs summary, value assertions, every mutation
with its kill evidence, actual gate output, deviations. Coordinator re-runs
mutations independently before any landing.
