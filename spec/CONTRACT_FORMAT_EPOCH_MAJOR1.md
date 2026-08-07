# Contract — Format epoch: container major 1

**Status:** **RATIFIED** after four adversarial review rounds; **AMENDED
2026-08-07** in a ratified forward amendment of its own — pin 3c, touch rows 10
and 11, gate 8 — closing a gap found by reconnaissance before dispatch: pin 3a's
refusals reach a conformance-suite criterion through a file the touch table did
not carry. Not yet implemented. **The pins are otherwise frozen** — they may be
executed, not edited. A defect found during execution is **reported, not patched
in place**; that is how pin 3c came to exist rather than being absorbed silently
into the work.

**Track:** format epoch. **Not a Pass 13 rung.** `P13-S28` is its dependency
record in `spec/PASS13_CANDIDATES.md` and points here; this contract is where
the work lives. It is sequenced ahead of **P13-S27**, which is ahead of
**P13-S16**.

**Rung type:** **container format-major boundary.** A new reader must
deliberately decode major 0 as legacy; old readers reject major 1 by the
mechanism that already exists.

**Rulings taken 2026-07-31, not re-opened here:**

1. **The carrier is the format major.** No generation-scoped attestation in this
   epoch — a superblock attestation cannot make pre-boundary readers fail
   closed, and once a major-1 container *is* the boundary, every writer able to
   open one is necessarily epoch-aware, so P13-S27's capabilities already
   validate every base replacement. The extra field would add wire and
   re-derivation complexity while solving no additional case.
2. **Legacy resolves to hard rejection, not read-only.** A pre-authority base is
   not safe materialized state.
3. **Its own track**, per above.

---

## §0. What was verified before drafting

Read out of the working tree at `818a16f`.

### 0.1 The boundary has the right polarity already, in one direction only

`FixedHeader::decode` (`header.rs:119`) rejects on `format_major != FORMAT_MAJOR`
with `BundleError::UnsupportedFormatVersion`. `FORMAT_MAJOR = 0`,
`FORMAT_MINOR = 1` (`:39`, `:42`).

So **old readers already fail closed on a major they do not know** — that half
needs no new mechanism and is why the major is the correct carrier. The half
that must be built is the other one: today's decoder is *exact-major-only*, so a
new reader would reject major 0 outright. It must instead decode major 0
**deliberately, as legacy**.

### 0.2 The surface is unusually small

`FORMAT_MAJOR` appears in exactly four places workspace-wide: `header.rs:39`
(definition), `:68` (stamped into every new header), `:119` (the accept check),
and `lib.rs:84` (re-export). There is no second parallel table.

### 0.3 No committed byte artifact pins a header — verified, not assumed

`spec/vectors/decode_vectors.txt` covers `bundle.block`, `bundle.manifest` and
`bundle.operation_index`; it has **no header or superblock surface**. The only
other committed byte artifact is `spec/vectors/textproj_document_vectors.txt`,
which is text-projection documents. A repository-wide search for `*.bin`,
`*.bundle`, `*.epi*` outside `target/` and the out-of-bounds trees finds
**nothing**.

**Consequence, and its limit.** No *binary* frozen file pins a header, so the
major bump itself regenerates nothing there, and "repack" describes what a
*user's* bundle needs rather than an in-tree artifact.

**That conclusion does NOT extend to the text companion, and an earlier draft of
this contract wrongly generalized it into a blanket "no vector file may
change" gate.** `spec/vectors/textproj_document_vectors.txt` carries
base-bearing documents (`textproj/src/vectors.rs:353`, `:363`), and pin 3b makes
projecting, parsing and serializing such a document an error.

**That file WILL change, and how it changes is specified — not left as an
unknown deliverable.** A second earlier draft downgraded the question to
"report whether it moved"; that is not ratifiable either. The corpus was
decoded (it is hex-encoded; see pin 3b) and the exact required edits are pinned
in pin 3b and touch row 5c.

### 0.4 The header cannot carry per-generation state, which is why this works

`core_spec.tex:10799`–`:10800`: *"The header never changes after the file is
created."* `commit_versioned` (`bundle.rs:791`) publishes only a superblock.
That immutability is exactly what disqualified `FORMAT_MINOR` as a *provenance*
carrier (P13-S27 pin 2a, rejected option iv) and is exactly what makes the major
sound as an *epoch* carrier: an epoch is a property of the file's creation, and
must not drift with later commits.

---

## §1. Pins

**Pin 1 — `FORMAT_MAJOR` becomes 1; `FORMAT_MINOR` resets to 0.**
A new major restarts minor numbering. Both constants get doc comments naming
this contract and stating what the epoch means: *a major-1 container is one
whose every base-bearing commit was validated against a supplied reduction
authority.*

**Pin 2 — the decoder becomes deliberately legacy-aware.**
`header.rs:119`'s exact-major test is replaced by an explicit three-way
classification, and the retained major must be carried on `FixedHeader` so
callers can act on it:

- `0` → **legacy**, decoded and marked as such;
- `1` → **current**;
- anything else → `UnsupportedFormatVersion`, unchanged.

**A boolean is not sufficient** — introduce a named enum (e.g. `FormatEpoch`)
so the legacy case is a value the type system carries, not a comparison
re-derived at each use site. Every consumer of the epoch reads that value.

**Pin 3 — the epoch matrix is normative and is implemented exactly.**

| # | Container | Canonical base | Behaviour |
|---|---|---|---|
| 1 | major 0 | none | **may open** — nothing unverifiable is exposed |
| 2 | major 0 | present | **hard reject** at `open` |
| 3 | major 0 | commit attempts to add or replace one | **reject**; require repack into a fresh major-1 bundle |
| 4 | major 1 | none | **may open**; may commit non-base-bearing history |
| 5i | major 1 | present at `open` | **INTERIM (this rung → P13-S27): refuse**, third error |
| 5 | major 1 | present at `open` | *post-S27:* opens; base validated against the supplied current authority |
| 6i | major 1 | commit attempts to add or replace one | **INTERIM (this rung → P13-S27): refuse**, third error |
| 6 | major 1 | commit attempts to add or replace one | *post-S27:* validates against the supplied current authority |

**Rows 5i and 6i are what this rung actually implements**; rows 5 and 6 are what
P13-S27 replaces them with. An earlier draft's matrix stated only the post-S27
behaviour while pin 3a implemented the interim one — the contract described a
container this rung does not build. Both states are now written, and the
interim rows are the normative ones for this rung's tests.

Row 3 is the one that would be missed: a legacy bundle that opens cleanly under
row 1 must not become base-bearing in place, because its header can never say it
was validated. **This is the rule that makes the epoch non-inheritable**, and it
is why the boundary works where `FORMAT_MINOR` did not.

**Corruption precedence is preserved, and it is not automatic. It binds in BOTH
epochs.** A container whose base's version disagrees with its superblock's is
**corrupt**, and MUST still fail with the existing malformed-bundle `DecodeError`
(`bundle.rs:396`–`:401`) **before any of pin 4's three epoch errors is
considered** — row 2's legacy error in a major-0 container, and equally row 5i's
`ReductionAuthorityUnavailable` in a major-1 one.

**The major-1 half is the one an earlier draft missed.** It pinned precedence
only for legacy containers, so a corrupt major-1 base could be reported as
"authority unavailable" — a *temporary* condition a user would reasonably retry
after P13-S27 — if the new epoch check were placed ahead of the existing
malformed check at `bundle.rs:396`. Test 11 cannot catch this: it uses a
**self-consistent** base by construction, so the malformed branch never runs in
it. Ordering the new checks first would collapse tampering into staleness and
**silently erase the very distinction P13-S27 rests on** (its §0.1 and pin 6).
Test 7 pins the order in both epochs.

**Pin 3a — the epoch may not assert what is not yet enforced. SEQUENCING.**
This contract's first draft stamped major 1 while §6 forbade implementing
P13-S27's capability — so between this rung and S27, `Bundle::create` would mint
major-1 containers and `commit_versioned` would still copy any carried base
version unchecked. **That is precisely the false provenance the epoch exists to
exclude**, minted by the mechanism meant to prevent it, and test 4 could not
truthfully claim a major-1 bundle "validates its base."

**A write-side refusal alone is insufficient, and an earlier draft of this pin
stopped there.** `open` (`bundle.rs:393`ff) accepts a matching base/superblock
pair, and S28 adds no authority capability — so a major-1 bundle that *already*
carries a self-consistent base would still open during the interval, its epoch
asserting a validation that never ran.

**Resolution: until P13-S27 lands, BOTH boundaries are temporarily closed:**

- **open** a major-1 container that already carries a canonical base → refuse;
- **commit** a base into a major-1 container → refuse.

**This needs a THIRD error, distinct from both legacy errors** — carried as a
first-class member of pin 4's inventory, not as a footnote to this pin. A
major-1 container does not need repacking — it is the right epoch; what is
missing is S27's authority check. Name the condition for what it is
(`ReductionAuthorityUnavailable`), and assert in tests that it is **neither**
legacy error. Reusing a repack error would tell a user to repack a container
that is already correct.

Both branches are **temporary and must be marked as such in code**, naming
P13-S27 as what replaces them. S27 converts both into capability validation —
not one of them.

*(The alternative — co-landing this rung with S27's capability and every
write-path validation — was considered and not taken: it merges two large rungs
and loses the separate ratification each has already had. If you prefer it, this
pin is where it changes.)*

**Pin 3b — text projection cannot mint a canonical base. TEXT RULE.**
`TextDocument` (`textproj/src/lib.rs:74`ff) has **no container-major or epoch
field**, and the projector deliberately drops physical layout
(`project.rs:29`ff, `req:textproj:derive-or-carry`). Meanwhile `parse.rs:591`
accepts an unbounded `u32` reduction version and `serialize_document`
(`serialize.rs:119`) creates a **fresh** bundle while `build_manifest` (`:216`)
copies the carried base verbatim. So an old or hand-authored text document can
be serialized into a brand-new major-1 container, and once its raw version
matches the current authority nothing downstream can tell it from a validated
base. **Text import is a laundering path straight through the boundary.**

**Ruled: symmetric document-level refusal.** A one-sided serialize refusal would
leave the companion incoherent — `project_bundle` emits canonical-base text
(`project.rs:479`, `:537`) and `parse` accepts it, so text carrying a base could
be produced and read but never serialized, while `req:textproj:roundtrip`
(`text_projection.tex:903`ff) quantifies over **every** bundle and every valid
text. All three sides move together:

- **projection** of a base-bearing bundle → error;
- **parsing** of base-bearing text → error;
- **serialization** → a **new, dedicated `SerializeError` variant**, as defence
  for a directly constructed `TextDocument`.

**All three are additions. Serialization has no refusal to "retain" today** —
an earlier draft said it did, and that was simply false: `serialize_document`
(`serialize.rs:119`) stages the carried base as a `Snapshot` chunk
(`serialize.rs:131`–`:141`) and `build_manifest` (`:216`) writes it into the new
manifest. Its documented error set is `NonEmptyBlobs` and `Bundle`
(`serialize.rs:116`–`:118`); neither covers this. The refusal must be **built**,
and it must be its own variant rather than a `SerializeError::Bundle`
passthrough — see M8 for why that distinction is load-bearing.

**Scope of that rule — text only.** *Text projection* may introduce a canonical
base only through an explicit rebuild/repack flow (pin 5). It does **not** say a
canonical base may only ever be created that way: matrix rows 5/6 admit
validated base introduction in major-1 containers once P13-S27 lands, and
ordinary snapshot producers remain governed by S27, not by this pin. An earlier
draft stated the rule unscoped, which contradicted both.

`req:textproj:roundtrip` MUST be amended to state the exclusion in its own terms
— **"round trips excepted" is not sufficient**: the requirement quantifies
universally and must say what is now outside its domain and why.

**Why not the alternative** — carrying provenance through the text companion,
with absent/old classified legacy: `manifest_schema_version` is the existing
precedent for a carried-verbatim field, and its own doc concedes *"the document
author is responsible for updating it."* A text format cannot carry unforgeable
provenance; any field it defines can be typed by hand. Carrying a provenance
marker would therefore reduce to trusting the author, which is exactly what the
epoch was built not to do — it would relocate the laundering one level up rather
than close it. Refusal is the only rule the medium can actually enforce.

**This is a real capability loss and must be stated, not softened:** base-bearing
documents stop round-tripping through text until a repack flow exists.

**`COMPANION_VERSION` MUST bump 0.13.0 → 0.14.0.** Not "determine whether it
moves" — an earlier draft left this open and it is not a ratifiable instruction.
Refusing a document the companion previously serialized is a semantic change,
and `parse_header` (`parse.rs:397`) rejects every version but the exact
`COMPANION_VERSION` (`lib.rs:59`), so the bump is load-bearing rather than
cosmetic.

**Consequence, verified by decoding the corpus** (it is hex-encoded, so a
plaintext grep proves nothing — an earlier draft's grep returned zero and proved
nothing at all): `spec/vectors/textproj_document_vectors.txt` holds **19 vectors
— 10 `accept` and 9 `reject`.** Eighteen carry header `(0 13 0)`; one carries
`(0 12 0)`. Six carry `canonical-base`. So `lib.rs`, `parse.rs`, and the corpus
file are **mandatory** touch rows.

**The header bump reaches 18 rows, not 10.** All 10 accepts move `(0 13 0)` →
`(0 14 0)`. So do the **8 rejection vectors that also carry `(0 13 0)`**
(`unreferenced_blob`, `canonical_base_before_extension`, `lineage_repeated`,
`envelopes_reversed`, `profiles_reversed`, `extensions_reversed`,
`extension_chunks_reversed`, `final_lf_missing`). If they are left at `(0 13 0)`
they still reject — **at the header, not at the predicate each was written to
exercise.** They would pass their declared verdict while testing nothing, which
is precisely the silently-degrading corpus this rung must not create.

**`superseded_companion_version` moves `(0 12 0)` → `(0 13 0)`.** Its purpose
(`vectors.rs:559`–`:565`) is to reject *the immediately superseded companion*;
after the bump that is 0.13.0. Leaving it at 0.12.0 would make it assert the
rejection of a two-generation-old version and stop exercising the deferred
migrate-on-read posture it was written for.

**The corpus takes ONE complete shape, specified here in full.** An earlier
draft left `rich_document` as "reject *or* re-derived" and the new class count
as *n*; neither is executable, and the choice is not free — **the only two
accepted documents carrying extensions are the two base-bearing ones**
(`extension_base_multi` at `vectors.rs:344`, `rich` at `:357`). Converting one to
`reject` and freeing the other drops `extensions` and `multi_envelope` reach
from 2 to 1; converting both drops them to 0. The disposition therefore decides
coverage, not just row count.

**Ruled shape: 20 vectors — 10 accepts, 10 rejects, ten rejection classes.**

*Accepts (10, all base-free):*

| Vector | Change |
|---|---|
| `extension_base_two_envelopes` — the `extension_base_multi` document (`:344`, exported `:461`) | **base removed**; extensions, two envelopes, non-baseline schema version all retained. **Its exported name must stop claiming a base it no longer carries** — and the rename reaches `by_name` at `:551` |
| `rich_document` — the `rich` document (`:357`, exported `:464`) | **base removed**; two extensions, lineage, custom profiles, envelopes all retained |
| the other 8 | header only |

*Rejects (10):*

| Vector | Change |
|---|---|
| `superseded_companion_version` | `(0 12 0)` → `(0 13 0)`, per above |
| `canonical_base_before_extension` | **re-expressed with a non-base section pair.** The order is `header document lineage? profile* extension* canonical-base? blob* envelope*` (`parse.rs:45`), so a lineage/profile or profile/extension inversion reaches `out-of-order-sections` without a base |
| `envelopes_reversed`, `extensions_reversed`, `extension_chunks_reversed` | **nothing beyond the header** — they are *derived* from the two accepts above (`:598`, `:614`, `:630`, `:633`), so freeing those accepts frees these automatically. Confirm it rather than assume it |
| `unreferenced_blob`, `lineage_repeated`, `profiles_reversed`, `final_lf_missing` | header only |
| **NEW: `canonical_base_present`** | class **`canonical-base-unsupported`** — a base-bearing text, refused by pin 3b's parse side. Build it from the *pre-change* base-bearing spelling, so the corpus keeps a base-bearing text as a **negative** rather than losing the spelling entirely |

*Why this shape rather than converting the two accepts to rejects:* it preserves
`extensions: 2` and `multi_envelope: 2` exactly, confines the reach loss to the
one capability pin 3b actually removes, and makes the new class carry a purpose-
built vector instead of a demoted accept that also happened to test three other
things.

**`expected_reach()` — every count, stated:**

| Field | Before | After |
|---|---|---|
| `extensions` | 2 | **2** |
| `canonical_bases` | 2 | **0** |
| `custom_profiles` | 2 | **2** |
| `lineages` | 2 | **2** |
| `multi_envelope` | 2 | **2** |
| `reject_classes` | nine classes × 1 | **ten** classes × 1 — the nine existing plus `canonical-base-unsupported` |

`canonical_bases: 0` is a **real reach loss** and its doc comment
(`vectors.rs:124`–`:126`) must record the cause — canonical bases are no longer
reachable through text at all — and the "nine distinct rejection classes"
wording moves to ten. Silently lowering a non-vacuity count without recording
why converts a stated capability loss into an unexplained weakened assertion.

**Three further count sites move with it, each verified present:**

- `vectors.rs:889` asserts the corpus has exactly **19** rows (*"the corpus has
  unexpectedly thinned"*) → **20**;
- `t12_g3b_kinds_round_trip_and_companion_is_0_13_0_rejecting_0_12_0`
  (`vectors.rs:969`, asserting `COMPANION_VERSION == (0, 13, 0)` at `:971`) —
  both its body and **its name** move to 0.14.0/0.13.0;
- `parse.rs:658`'s test `HEADER` constant, and `text_projection.tex:486` and
  `:1146`, which spell `(0 13 0)` literally.

The four base-bearing reject rows were the dangerous ones: they would **still
reject** after pin 3b while their declared class is *informative only*
(`vectors.rs:67`), so `reject_classes` would keep counting them long after the
predicate each names went untested.

**Pin 3c — the interval's conformance cost is bounded, marked, and owed back.
AMENDED 2026-08-07, after ratification, on a finding from execution
reconnaissance.**

Pin 3a's refusals close **every** path to a base-bearing container, which the
pins did not say out loud: `create` already rejects a base-bearing manifest
(`bundle.rs:233`–`:240`), row 3 refuses committing one into major 0 and row 6i
into major 1, and rows 2 and 5i refuse *opening* one in either epoch. So for the
S28 → P13-S27 interval **no bundle anywhere may carry a canonical base** — in
production, in tests, or in the conformance suite.

That reaches one file outside the original touch table and one criterion:
`roundtrip::assert_reduction_serialization_stable` (`roundtrip.rs:241`, base
declared at `:270`) is criterion 4's bookkeeping-projection counterpart
(`testkit/src/lib.rs:86`), driven from `tests/acceptance.rs:135` and
`examples/conformance_suite.rs:60`. It commits the canonical state **as the
canonical base** and reopens the image; pin 3a refuses both halves. Gate 1 could
not have passed, and §6 would have forbidden staging the fix.

**Only the canonical-base wiring is suspended. Criterion 4's cycle keeps
running.** The finding was first reported as "the assertion can only be
suspended" — **too strong, and verified false before this pin was written.** The
serialize → load → decode → reserialize cycle does not depend on the snapshot
being the *canonical base*: `read_chunk` (`bundle.rs:509`–`:511`) hash-verifies
any `ChunkRef` through `read_and_verify_chunk` (`:1002`). The harness therefore
keeps committing the snapshot chunk and reads it back by its ref; the criterion
survives intact.

**Exactly two assertions lapse, both canonical-base-specific:**

1. `verify_canonical_chunks`'s base branch (`bundle.rs:613`–`:621`), including
   the `base.hash != base.root.hash` cross-check;
2. the reopened manifest actually carrying the base (`roundtrip.rs:293`–`:297`).

**Do NOT re-home the snapshot to `acceleration_snapshots`.** That field appears
**nowhere** in `bundle.rs` — not in `open`, not in `verify_canonical_chunks` —
so the reference would verify nothing while looking like preserved coverage.
This is the single most tempting wrong repair here, and it is forbidden.

**The lapse is marked, never absorbed:**

- at the point the base declaration is removed, a comment naming **P13-S27** and
  this pin;
- the harness's doc paragraph (`roundtrip.rs:228`–`:240`) describing the
  canonical base as the snapshot's "correct semantic home" is **amended to state
  the suspension, not deleted** — a deleted paragraph leaves S27 nothing to
  restore against;
- both lapsed assertions are recorded as **owed** in S27's contract (touch
  row 9), beside pin 8 and M8's deferred demonstration.

`testkit/benches/bundle.rs:104` (`build_fixture`, base at `:133`) takes the same
treatment. It is the `[[bench]] name = "bundle"` target and **not** the
out-of-bounds `editor_pipeline.rs`.

**One trap, named because it will be met.** `bundle.rs:1487`
(`a_canonical_base_stamped_above_major_0_opens_read_only`) both commits a base —
so it breaks — and asserts **read-only + anomaly** for a base fault. It is on a
different axis (data-model schema major, not container format major) and does
**not** contradict pin 4. Do not "harmonize" pin 4's errors toward it; pin 4
forbids read-only for all three of its errors.

**Pin 4 — THREE distinct errors, and none is read-only.**
An earlier draft specified two, then pin 3a introduced a third without amending
this pin — leaving the error inventory, gate 6 and M11 all describing a
two-error design. The full set:

| Error | Raised by | Lifetime | Message |
|---|---|---|---|
| **legacy-base** | matrix row 2 | permanent | names **repack** |
| **legacy-base-introduction** | matrix row 3 | permanent | names **repack** |
| **authority-unavailable** (`ReductionAuthorityUnavailable`) | matrix rows 5i, 6i | **temporary — P13-S27 removes it** | names **P13-S27**; **MUST NOT mention repack** |

- row 2 → the document already contains unverifiable canonical state;
- row 3 → the document is fine, but the operation requested cannot be performed
  in this container;
- rows 5i/6i → the container is the **right** epoch; the reader has not yet been
  given the authority to validate its base. Repacking would be wrong advice, so
  the message must not offer it.

All three are `BundleError` variants (that type has no discriminant and no
encoder — verified — so this is a pure API change). **None degrades to
read-only:** a pre-authority base is not a restricted-but-correct view, and
exposing it read-only would serve unverifiable canonical state confidently.

The three must be **mutually distinguishable in tests**, not merely distinct in
source: every test that expects one asserts the other two are not produced.

**Pin 5 — repack is named, not built.**
This rung provides no repack implementation. It MUST leave the door open for one
and MUST NOT foreclose it: a higher-level, **explicitly non-materializing**
recovery/repack flow may later offer rebuilding when complete history is
available. **That flow is not a read-only `Bundle::open` mode**, and nothing in
this rung may introduce one.

Record this in the errors' doc comments so a later reader does not "helpfully"
add the read-only path.

**Pin 6 — every writer path stamps and is checked, including text projection.**
`Bundle::create` stamps major 1. **The production writer that reaches a
canonical base is one, not two:** `textproj::serialize_document`
(`serialize.rs:119`, via `build_manifest` `:212`). **`project.rs:936` is inside
`#[cfg(test)]`** (the module opens at `project.rs:560`) and is a fixture writer;
an earlier draft of this pin listed it as production, taken from a census
without checking its enclosing module. It stays in the test surface. The committed `.txt` document vectors can declare any
`reduction_algorithm_version` (`parse.rs:591` parses an unbounded `u32`), so
**text projection is a writer path in the full sense** and is enumerated here
rather than left to be discovered, as P13-S27's first draft did.

**Pin 7 — specification updates.**
`binary_format.tex:1808`'s header table states `format_major` is `0` and must
carry the epoch and its meaning. `core_spec.tex`'s Fixed Header subsection
(`:10796`ff) gains the legacy-decode rule and the epoch matrix (**rows 1–4 and
the post-S27 rows 5/6 only — the interim rows 5i/6i are implementation state,
not normative wire semantics, and must not be written into the spec**); the
major-version semantics near `:12467` gain what a major boundary now *means*
beyond wire layout. Revision History rows and version bumps in both.

**Pin 8 — P13-S27's precondition is stated where S27 can rely on it.**
Reduction-version authority is meaningful **only in major-1 containers**. Record
that here and in S27's contract, so S27's pin 2a resolves to: *legacy bases are
refused by container epoch, never by version arithmetic.*

**`spec/CONTRACT_P13S27_REDUCTION_AUTHORITY.md` is therefore an edited file of
this rung**, and touch row 9 carries it. Two things land there, not one:

1. this pin's major-1 precondition, resolving S27's open pin 2a;
2. **M8's deferred laundering demonstration**, which S27 inherits as owed work.

It is still a **DRAFT** and so may be edited; it is not among the ratified
contracts this session may not touch. An earlier draft left it off the touch
table while two pins required writing to it — and since §6 stages **only** the
touch table by explicit path, that omission would have silently dropped both.

**Pin 9 — the ledger.**
`spec/PASS13_CANDIDATES.md`: P13-S28's row points here and records the ruling
set. **P13-S28 does not execute as a Pass 13 rung.** S27 and S16 stay blocked
until this contract lands.

---

## §2. Touch table

| # | File | Change |
|---|---|---|
| 1 | `crates/epiphany-bundle/src/header.rs` | pins 1, 2 |
| 2 | `crates/epiphany-bundle/src/bundle.rs` | pin 3 (open + commit paths) |
| 3 | `crates/epiphany-bundle/src/error.rs` | pins 4, 5 |
| 4 | `crates/epiphany-bundle/src/lib.rs` | re-exports |
| 5 | `crates/epiphany-textproj/src/serialize.rs` | pins 6, 3b — the refusal, and the round-trip laws that must now except base-bearing documents |
| 5b | `crates/epiphany-textproj/src/project.rs` | pin 6 |
| 5c | `crates/epiphany-textproj/src/vectors.rs` **and** `spec/vectors/textproj_document_vectors.txt` | **mandatory**, pin 3b's ruled corpus shape: 18 rows carrying `(0 13 0)` → `(0 14 0)`; `superseded_companion_version` `(0 12 0)` → `(0 13 0)`; both base-bearing accepts re-derived base-free; `canonical_base_before_extension` re-expressed on a non-base pair; **new** `canonical_base_present` reject; corpus 19 → **20** (`:889`); `expected_reach()` `canonical_bases` 2 → 0 with cause, `reject_classes` nine → ten; `t12_…_0_13_0_rejecting_0_12_0` renamed and rebased (`:969`) |
| 5d | `crates/epiphany-textproj/src/lib.rs` | `COMPANION_VERSION` 0.13.0 → **0.14.0** (`:59`) — **mandatory**, not conditional |
| 5e | `crates/epiphany-textproj/src/parse.rs` | pin 3b's parse-side refusal; `parse_header` (`:397`) accepting only the new version; the test `HEADER` constant at `:658` |
| 5f | *(same file as row 5)* `SerializeError` at `serialize.rs:66` | pin 3b's **new dedicated variant** — an addition, not a retained refusal; the doc comment at `:116`–`:118` enumerates the error set and must gain it |
| 5g | `spec/text_projection.tex` (+ `.pdf`) | `req:textproj:roundtrip` (`:903`ff) amended to state the base-bearing exclusion in its own terms; companion version (`:486`, `:1146` spell `(0 13 0)` literally) and Revision History rows |
| 6 | `spec/binary_format.tex` (+ `.pdf`) | pin 7 |
| 7 | `spec/core_spec.tex` (+ `.pdf`) | pin 7 |
| 8 | `spec/PASS13_CANDIDATES.md` | pin 9 |
| 9 | `spec/CONTRACT_P13S27_REDUCTION_AUTHORITY.md` | **mandatory** — pin 8's major-1 precondition (resolving S27's open pin 2a), M8's deferred laundering demonstration, **and pin 3c's two owed-back conformance assertions**. Still a DRAFT, so editable; **not** one of the ratified contracts that may not be touched |
| 10 | `crates/epiphany-testkit/src/roundtrip.rs` | **pin 3c** — criterion 4's canonical-base wiring suspended and marked; the cycle itself preserved via a direct `ChunkRef` read |
| 11 | `crates/epiphany-testkit/benches/bundle.rs` | **pin 3c** — `build_fixture` (`:104`) stops declaring a canonical base at `:133`. **Not** `benches/editor_pipeline.rs`, which is out of bounds |

**`spec/vectors/decode_vectors.txt` is NOT touched** (§0.3). If a change appears
to require regenerating *that* file, stop and report — it would mean something
reads a header where §0.3 found nothing.
**`spec/vectors/textproj_document_vectors.txt` IS touched**, mandatorily, by
pin 3b's companion bump.

---

## §3. Required tests

1. **`a_legacy_major_0_bundle_without_a_base_opens`**
2. **`a_legacy_major_0_bundle_with_a_base_is_rejected`** — asserting the row-2
   error specifically.
3. **`adding_a_base_to_a_legacy_bundle_is_rejected_and_names_repack`** — the
   row-3 error, distinct from row 2's, asserted by variant **and** message.
4. **`a_major_1_bundle_round_trips_and_refuses_to_introduce_a_base`** — renamed
   from `…_validates_its_base`, which pin 3a forbids claiming until P13-S27
   lands. **The name must not promise validation this rung does not perform.**
   It MUST assert matrix row 6i's **authority-unavailable** error specifically,
   and that **neither legacy error** is produced. An earlier draft left this
   test asserting only "some failure", which is what made M11 unfalsifiable.
5. **`an_unknown_major_is_still_unsupported_format_version`** — the third arm of
   pin 2, which a two-way test would silently drop.
6. **`text_projection_serialize_produces_a_major_1_container`** — pin 6.
7. **`a_corrupt_base_fails_as_malformed_before_any_epoch_error`** — pin 3's
   precedence rule, **covering both epochs in one test or two, but covering
   both**. Construct a container whose base version disagrees with its
   superblock's, once at major 0 and once at major 1, and assert the
   **malformed** error each time — **not** row 2's legacy error, and **not**
   row 5i's `ReductionAuthorityUnavailable`. Renamed from
   `…_corrupt_legacy_base…`: the earlier name scoped the guarantee to legacy
   containers, which is exactly the half that was missing. Without this, S28
   silently erases P13-S27's tamper/staleness distinction and every other test
   still passes — **including test 11**, whose base is self-consistent by
   construction.
8. **`serializing_a_text_document_with_a_canonical_base_is_refused`** — pin 3b,
   asserted against a document built from the existing base-bearing fixture, and
   asserting the **dedicated `SerializeError` variant specifically** — not merely
   "an error", and **not** `SerializeError::Bundle`. M8 depends on that
   distinction being asserted.
9. **`projecting_a_base_bearing_bundle_is_refused`** — pin 3b's projection side.
10. **`parsing_base_bearing_text_is_refused`** — pin 3b's parse side. Without 9
   and 10 both, the companion is left able to produce text it cannot consume.
11. **`opening_a_major_1_bundle_that_already_carries_a_base_is_refused`** — pin
   3a's **read-side** branch (matrix row 5i), the one an earlier draft omitted
   entirely. Asserted to return the **authority-unavailable** error and to be
   **neither** legacy error.

Tests 2 and 3 must each assert the **other's** error is not produced; they are
the pair pin 4 exists to separate. Test 7 stands in the same relation to test 2.
Tests 4 and 11 stand in that relation to **both** legacy errors.

---

## §4. Mutation plan

Applied, **run**, output verbatim, restored **by hand-editing back**.

**M1 — the legacy classification is real.** Restore the exact-major test; test 1
must fail (a legacy bundle stops opening at all).

**M2 — row 2 fires.** Remove the base check for legacy containers; test 2 fails.

**M3 — row 3 is not row 2.** Make the commit path return row 2's error; test 3
fails. Signs that the two situations are separately diagnosable.

**M4 — the epoch is non-inheritable.** Permit a legacy container to gain a base;
test 3 fails. **This is the signing mutation of the whole rung** — it
reintroduces exactly the counterexample that killed `FORMAT_MINOR`, and the
contract's central claim is that a major boundary does not admit it.

**M5 — the unknown-major arm survives.** Make the classifier treat any non-1
major as legacy; test 5 fails.

**M6 — the writer stamps the epoch.** Make `create` stamp major 0; test 6 fails.

**M7 — corruption precedence holds in both epochs.** Reorder pin 3's checks so
the **epoch** classification runs before the malformed-base check at
`bundle.rs:396`; test 7 fails **on both its major-0 and major-1 halves**. Run it
as a single reorder — one placement decision governs both — but **report both
failures**, because a reorder that only moves the legacy branch would show one
failure and look like a pass of the other. Signs that the ordering is deliberate
rather than incidental to how the code happens to be written, and that a corrupt
major-1 base is never reported as the *temporary*
`ReductionAuthorityUnavailable` — which a user would reasonably retry after
P13-S27 lands, on a container that is in fact tampered with.

**M8 — the text boundary is closed.** In `serialize_document`, **remove or
bypass the base-bearing early-return branch** that raises pin 3b's dedicated
`SerializeError` variant. **Test 8 must then fail by falling through to pin 3a's
interim bundle error** (`ReductionAuthorityUnavailable`, surfacing as
`SerializeError::Bundle`) instead of the dedicated variant. That fall-through
**is** the signature: it shows the text layer's own refusal is what test 8 was
asserting, not the container guard standing behind it.

**Mutate the branch, NOT the variant declaration.** Deleting the variant makes
test 8 — which pin 3b requires to name it — fail to **compile**, and a mutation
that does not compile observes nothing at all: no fall-through, no error
identity, no evidence. An earlier draft said "remove the variant," which would
have produced a compile error and invited the executing agent to report it as
the mutation's "failure." **A compile error is not a test failure**, and this
contract does not accept one as mutation evidence anywhere.

**Do NOT expect a successful serialization here, and an earlier draft did.**
That draft required observing that a base-bearing document "does serialize into
a major-1 container" with the refusal removed — **impossible under pin 3a**,
which refuses every major-1 base commit outright. Removing the text refusal
reaches that guard; it cannot reach success. The mutation as written could not
have produced its promised observation.

**The laundering demonstration is deferred to P13-S27**, where generic authority
validation exists and a base commit can succeed or fail on its version rather
than being refused categorically. Record that deferral in S27's contract when
this rung lands — the demonstration is still owed, just not performable yet.

**M9 — the interim write refusal is not vacuous.** Remove pin 3a's temporary
base-introduction refusal for major-1 containers; test 4 fails.

**M10 — the interim READ refusal is not vacuous.** Remove pin 3a's open-side
branch; test 11 fails. Run this **separately from M9** — a single mutation
covering both would not show that the two branches are independently present,
which is exactly the gap that made the previous draft unsound.

**M11 — the third error is genuinely distinct.** Make **both** of pin 3a's
major-1 branches return the row-3 repack error instead of
`ReductionAuthorityUnavailable`. **Tests 4 and 11 must both fail; test 3 is a
control and must stay green.**

An earlier draft said "tests 10 and 3 must both fail" — **impossible as
written.** M11 touches only the major-1 branches; test 3 exercises the untouched
legacy commit path and passes regardless. That draft's M11 could not have
produced the failure it predicted, and would have been reported as an anomaly or
quietly re-specified by the executing agent. The corrected form is what makes it
signing: test 3 staying green proves the mutation was **confined** to the
major-1 branches, and tests 4 and 11 failing proves both of them assert the
third error rather than any repack error.

---

## §5. Gate

1. `cargo test --workspace` — full pass; report the new total and its delta.
2. `cargo clippy --workspace --all-targets -- -D warnings` → clean.
3. `cargo fmt -p epiphany-bundle -p epiphany-textproj --check` → clean.
   **`cargo fmt --all` is forbidden.**
4. `git diff --cached --check` clean; staged list exactly §2.
5. **`spec/vectors/decode_vectors.txt` unmodified** — by `git status`, not
   inspection (§0.3). **`spec/vectors/textproj_document_vectors.txt` MUST have
   changed**, and the diff must show every edit from touch row 5c: 18 rows to
   `(0 14 0)`, `superseded_companion_version` to `(0 13 0)`, both accepts
   re-derived base-free, `canonical_base_before_extension` re-expressed, the new
   `canonical_base_present` reject, **20 rows total**, and `expected_reach()` at
   its pinned counts. An unchanged corpus is a **failure of this gate**, not a
   clean result — it would mean pin 3b's text boundary was never exercised.
   **A corpus that changed to 20 rows while `canonical_bases` stayed at 2 is
   also a failure** — it would mean a base survived on the accept side.
6. No read-only path was added for **any** of pin 4's three errors:
   `grep -rn "read_only" crates/epiphany-bundle/src/bundle.rs` reviewed, and all
   three new errors shown not to appear in any branch that sets it.
7. `FORMAT_MAJOR == 1` and `FORMAT_MINOR == 0`, asserted in a test, not only by
   reading the constants.
8. **No surviving base-declaring site outside a crafted-image fixture or a
   refusal test** (pin 3c): `grep -rn "canonical_base = Some\|canonical_base:
   Some" crates/ --include='*.rs'` reviewed hit by hit, each remaining one
   classified as (a) a hand-built image fixture, (b) a test asserting a refusal,
   or (c) the text corpus under pin 3b. **Any hit that is a live `commit` path
   is a failure** — it means a base-bearing container is still being minted.

---

## §6. Staging and boundary

Stage only §2's files, by explicit path. **Never `git add -A`.**

**A concurrent session commits here.** Re-check `HEAD` before staging and before
commit. **Never** `git reset`, `git restore --staged`, `git checkout`, `git
stash`.

**Out of bounds — MUST NOT be read, written, or staged:** the entire `spikes/`
tree, `spec/PLAN_EDITOR_APP.md`, `spec/CONTRACT_EDITOR_*.md`,
`spec/ANALYSIS_GENESIS_PERSISTENCE.md`, `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`,
`spec/DRAFT_T4_FIXTURE_RECIPE.md`, `crates/epiphany-render-svg/**`,
`crates/epiphany-glyphs/**`, `crates/epiphany-editor-gui/**`,
`crates/epiphany-testkit/benches/editor_pipeline.rs`, the root `Cargo.toml`,
`.claude/worktrees/`.

**Do not implement P13-S27 or P13-S16.** No `BundleCapabilities`, no
`CURRENT_REDUCTION_ALGORITHM_VERSION`, no `create_staff` change. This rung
establishes the container epoch those depend on; it does not begin them.

**Editing S27's *contract* is in scope and is required** (touch row 9) — pins 8
and M8 both write to it. That is a document edit, not an implementation of S27.
S16's contract is **not** touched by this rung.

**Pin 3a's interim refusal is the one exception** and is explicitly in scope: a
major-1 container refuses base introduction outright until S27 replaces that
with validation. It MUST be marked in code as temporary and MUST NOT be built as
a partial capability check.

**Do not build a repack flow** (pin 5) and **do not add a read-only mode** for
any of pin 4's three errors.

**The executing agent MUST NOT commit.** Leave the work staged.

---

## §7. Report requirements

1. The **eleven** mutations, each with verbatim failure output — **M4 identified
   as the signing mutation** of the epoch's non-inheritability; **M11's control
   result** (test 3 green) reported alongside its two failures; **M7's two
   failures** (major-0 and major-1 halves) reported separately; and **M8's
   fall-through error named**, confirming it reached pin 3a's interim guard
   rather than a successful serialization.
2. The **eight** gate results, each with its command — gate 8's hits classified
   one by one, not summarized.
3. The staged file list and the test-count delta with its cause.
4. The **eleven** tests by name, with 2/3, 2/7, and 4/11-vs-both-legacy-errors
   each shown to produce **different** errors.
5. `decode_vectors.txt` unchanged. `textproj_document_vectors.txt` **changed to
   20 rows**, with its diff summarized against touch row 5c. `COMPANION_VERSION`
   **at 0.14.0**. Every `expected_reach()` count against pin 3b's table, with
   `canonical_bases: 0`'s recorded cause. Which round-trip laws now except
   base-bearing documents. These are reported as **performed**, not as open
   questions.
6. Confirmation that **no repack flow and no read-only path** were added, and
   that pin 3a's refusal is marked temporary in code with P13-S27 named.
6b. The **three** additions to `spec/CONTRACT_P13S27_REDUCTION_AUTHORITY.md`
   (touch row 9), quoted: pin 8's major-1 precondition, M8's deferred laundering
   demonstration, and pin 3c's two owed-back conformance assertions.
6c. Pin 3c's suspension, shown in the diff: the marker naming P13-S27, the
   **amended** (not deleted) doc paragraph, and confirmation that criterion 4's
   serialize → load → decode → reserialize cycle still runs via a direct
   `ChunkRef` read — **and that `acceleration_snapshots` was not used**.
7. Anything contradicting this contract.
