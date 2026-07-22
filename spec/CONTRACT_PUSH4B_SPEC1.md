# Contract: Push 4b Wave 1a — three ratified spec corrections

Repo root `/home/jeans/Repos/active/epiphany`. Read this in full before editing
anything. The plan is `spec/PLAN_PUSH4B_TUNING.md`; its four rulings are granted
and this contract states the three that concern you as law.

You edit **`spec/core_spec.tex` only**, plus its rebuilt PDF. You write no Rust
except possibly one constant (§4), and you touch no other document.

## Correction 1 — the JI prime basis (P13-S5, Ruling A)

`req:pitch:ji-vector-basis` (`core_spec.tex:999`) says the built-in JI pitch
spaces order primes ascending **starting with 2**, and that `components.len()`
MUST equal the basis size. The built-in pitch-space table contradicts it, each
row exactly one short:

| space | table says today | must become |
|---|---|---|
| `ji-5limit` | "Two-dimensional (prime axes 3, 5)" | three-dimensional, basis {2,3,5} |
| `ji-7limit` | "Three-dimensional" | four-dimensional, basis {2,3,5,7} |
| `ji-11limit` | "Four-dimensional" | five-dimensional, basis {2,3,5,7,11} |

**Ratified: full register.** The three table rows change. The requirement is
correct as written and **MUST NOT** be edited — it is cited from elsewhere and
its text is the thing the table was failing to match.

State each row's basis explicitly, in ascending prime order including 2, so two
readers cannot derive different bases. The reason, if you want it for the prose:
a `JiVector` is an absolute *position*, and without the prime-2 exponent
`ji-5limit` cannot distinguish C4 from C5.

## Correction 2 — a score selects, it does not define (P13-S7, Ruling C)

`ScalePosition`'s listing comment (`core_spec.tex:934`) says the space
"References an entry in **the score's pitch-space registry**". No such registry
exists: `Score` has thirteen fields and none is one, and `ScoreTuningContext`
carries ids plus accidental extensions, never space or system definitions.

`req:pitch:default-pitch-space` says "Every score MUST **define** at least one
pitch space… Scores MAY **define** additional pitch spaces."

**Ratified: the catalog is closed.** Amend both:

* the requirement's two "define"s become *select* (from the built-in catalog),
  preserving everything else it says — including the non-comparability rule and
  the explicit space-conversion MUST, which are cited elsewhere;
* the `ScalePosition` comment names the **built-in** catalog.

Add a sentence recording that score-local pitch-space definition is deferred to
a later schema major, with the reason: the Chapter 4 type surface has never had
a consumer, and `req:binfmt:frozen-layout` would freeze it permanently.

Do **not** add a registry field to any struct. That is the point of the ruling.

## Correction 3 — `AccidentalEngraving` must be canonical-safe (Ruling D)

`AccidentalEngraving` (`core_spec.tex`, §"Accidental Engraving Metadata")
declares `bounding_box: BoundingBox`. That `BoundingBox` is **Chapter 7's**
(`core_spec.tex:8650`), built on `StaffSpace`, which the implementation defines
as `pub struct StaffSpace(pub f32)` — single precision, deliberately
(`crates/epiphany-layout-ir/src/spatial.rs:3`).

Resolved layout is **non-canonical**, so single precision is correct there. But
Push 4b puts `accidental_extensions` inside `ScoreTuningContext`, which **is**
canonical state, and `req:determinism:canonical-floating-point` requires
canonical stored floats to be "finite IEEE 754 **binary64**".

**Ratified: Chapter 4 gets its own bounding box, over `SpaceUnit`** — the type
`AccidentalEngraving.advance_width` already uses, which is `CanonicalF64`. Give
it a distinct name so it cannot be confused with Chapter 7's, declare its four
edges as `SpaceUnit`, and cite `req:determinism:canonical-floating-point` for
why. Chapter 7's `BoundingBox` is **not** edited.

Add a short rationale block: engraving metadata that lives in canonical state
must carry canonical precision, and Chapter 7's coordinates are a
non-canonical layout cache.

## What you must not do

* **Do not touch the built-in tuning-system table.** The 20 tuning systems are
  another agent's work and are not ratified yet. You change the **pitch-space**
  table (correction 1) and nothing else in that section.
* **Do not edit `req:pitch:ji-vector-basis`**, `binary_format.tex`,
  `operation_catalog.tex`, or any other companion.
* **Do not add or remove a `\begin{requirement}` block.** All three corrections
  amend existing text. See §4.
* **Do not rename or move any existing `\label`.** They are cited by code,
  tests, and decision records.
* Do not run `cargo fmt --all`.

## Verification

1. **The requirement counts must not move.** `requirement_labels.rs` pins
   `CORE_REQUIREMENT_COUNT = 209` and both suite counts at `279`. You add no
   requirement, so all three stay. If a count changes, you did something the
   contract did not ask for — stop and report it rather than updating the
   constant.
2. `cargo test -p epiphany-testkit --test requirement_labels` → 6 passed.
3. Rebuild `core_spec.pdf` with `latexmk -xelatex` **twice**. Then check the log
   for `^! `, `Undefined control sequence`, and `Reference .* undefined` — report
   the actual counts, which must be zero.
4. Full workspace gate, because you are in a repo others depend on:
   `cargo fmt --all --check`; `cargo clippy --workspace --all-targets` → 0;
   `cargo test --workspace`; `cargo run -q -p epiphany-testkit --example
   conformance_suite` → 8/8. Zero golden churn is expected — report it if not.

Report the actual commands and their actual output. On this project an agent
once reported "verification passes" while errors pointed into its own file, and
another silently deleted a struct field while rewriting the comment above it —
so **re-read every listing you edit, in full, after editing it**, and confirm the
declarations are still there.
