# Invariants

The rules a change to Epiphany must not break. The `.tex` suite in `spec/` is
the normative source for the model; this file names the rules that do the most
damage when broken and where each is held. Several have no mechanical backstop
at all, and those say so: for them the discipline is the guarantee.

## Determinism

Canonical document state is independent of platform, CPU, locale, thread
scheduling, hash-map iteration order, floating-point environment, compression
settings and wall-clock time (core specification, Appendix D). The means live in
`epiphany-determinism`, which depends on nothing but `blake3` and holds no
crate-local semantics.

- Anything encoded, hashed or compared canonically iterates in canonical order:
  `CanonicalMap`, `CanonicalSet`, `sort_canonical`, or a `BTreeMap`/`BTreeSet`.
  A hash map's iteration order never reaches canonical bytes.
- A float in canonical state is a `CanonicalF64`: finite only, `-0.0`
  canonicalized to `+0.0`, little-endian bytes. Layout coordinates are
  `QuantizedCoord`, the 1/1024 staff-space grid.
- Musical time is exact rational time; no float enters it.
- Every normative tolerance is one of the named `ToleranceClass`es. An ad-hoc
  epsilon is forbidden.
- Content addresses are BLAKE3-256 over domain-separated preimages
  (`Preimage` and the `MUSC*` domain tags).
- Every crate except `epiphany-editor-gui` carries `#![forbid(unsafe_code)]`.

The conformance suite's determinism gate and the two-replica convergence tests
are the tripwires. They cannot see a nondeterminism their fixtures do not
exercise.

## Layering

- `epiphany-ops` depends on `epiphany-core`, never the reverse. Where both need
  one normative relation (the measure-anchor comparable order and musical
  delta), each implements it and a cross-crate agreement test in
  `epiphany-testkit` drives the same inputs through both; neither reaches into
  the other.
- `epiphany-ops` must not depend on `epiphany-bundle`.
  `CURRENT_REDUCTION_ALGORITHM_VERSION` is a plain `u32`, wrapped into the
  bundle's `ReductionAlgorithmVersion` at the composition boundary by a crate
  that depends on both.

## Wire discriminants are appended, and locked by hand

- A wire discriminant is appended, never inserted, reordered or reused.
  `OperationKind` holds 0 to 39; the next kind takes 40.
- Each discriminant table has a golden test that states the mapping a second
  time, by hand: `operation_kind_wire_discriminants_are_golden`,
  `tag_wire_discriminants_are_golden`,
  `operation_payload_discriminants_are_golden` and
  `transaction_category_discriminants_are_golden` in
  `crates/epiphany-ops/src/payload.rs`; the identifier-kind table in
  `crates/epiphany-core/src/ids.rs`; and the domain-tagged preimage goldens
  (`MUSCSPCH`, `MUSCSVCE`, `MUSCSANM` and their neighbors). A new kind adds its
  row in the same change; a kind appended without a row leaves its byte
  unlocked.
- A golden row, its length and its order are never derived from the code they
  check (`discriminant()`, `from_discriminant()`, `PAYLOAD_FREE`, the tag
  vocabulary macro). A table so derived is born green and can never disagree.
  A computed block may prove the table is total over the vocabulary; only the
  literal rows say which byte belongs to which kind.
- `OperationKind::introduced_minor` is an exhaustive match with no wildcard arm,
  so a new variant cannot compile without its schema-minor epoch.
- A change to the wire format or to canonical reduction semantics changes the
  specification in the same change, runs alone, and takes two review rounds.

## Decode is injective

Two distinct byte strings never decode to one value, because a content address
is a hash of bytes. Several leaf and collection decoders normalize (a rational
reduces to lowest terms, a set re-sorts and de-duplicates, a `CanonicalF64`
canonicalizes through its constructor), so decoding alone is not injective.

- Every decode that accepts content-addressed bytes closes the gap by
  re-encoding the decoded value and rejecting input that differs:
  `Score::decode_canonical`, and the versioned decode at every schema major.
  An accepted byte string is its own canonical form
  (`req:format:codec-conventions`).
- A sub-codec decodes strictly wherever it can. A structure that embeds a
  lenient codec without a whole-value re-encode guard inherits the leniency:
  `CompressionAlgorithm::None` once accepted a non-zero parameter byte, and a
  structure embedding a `ChunkRef` without such a guard inherited it.
- A conforming writer never emits non-canonical bytes, so these checks reject
  only corrupt or adversarial input. That is not a reason to skip one.

## Minimal stamping and mixed majors

An op-envelope block's schema major is the maximum over its operations, and
each operation stamps the lowest major whose layouts decode its bytes
(`OperationKind::schema_major`). The stamp is a pure function of the value, so
identical content stamps and hashes identically. Kinds whose newer fields sit
behind an `Option` are value-dependent; kinds with mandatory appended fields
always stamp their own major.

The consequence is that the current writer produces files whose op blocks
carry lower majors than the newest, and a file that mixes majors is ordinary.

- Files the current writer produces reopen, mixed-major blocks included.
- Before the product's 1.0 release nothing is migrated, and a layout outside
  the supported boundary is refused by name. "Unsupported" is never "any lower
  major": that would refuse the current writer's own files.
- The container format is major 1 (`FORMAT_MAJOR`). A legacy major-0 container
  still opens, but never with a canonical base (below).

## The reduction version, and canonical bases

`epiphany_ops::CURRENT_REDUCTION_ALGORITHM_VERSION` names the reduction
semantics this build implements.

- Any change to a canonical reduction verdict, or to canonical reduced state,
  bumps it and adds an entry to its `Bumps` list in the same change. Both
  classes are named because a change that leaves every verdict intact while
  altering the reduced graph is the easier one to overlook, and it invalidates a
  base just as completely.
- No mechanism detects a missed bump. The authority check compares declared
  versions; a golden over reduction output can prompt the question but not
  answer it, since a deliberate semantics change and an accidental regression
  look the same from outside. The discipline is the whole guarantee.
- A bump without its `Bumps` entry leaves a number nobody can account for.
- A canonical base is accepted only when its `reduction_algorithm_version`
  equals the running authority. A mismatch is
  `BundleError::CanonicalBaseRequiresRebuild { base, current }`, refused on the
  read side (`Bundle::open`) and the write side (`commit`, `commit_versioned`)
  alike.
- A legacy container refuses a base outright on both sides
  (`LegacyBundleHasCanonicalBase`, `LegacyBaseIntroductionRejected`).
- None of these refusals degrades to a read-only open. A stale or unvalidated
  base is the wrong materialization, not a restricted view of a right one. A
  base materialized under an earlier version is rebuilt, never reused.

## Production capabilities versus fixture capabilities

`BundleCapabilities` states which reduction semantics the caller implements.

- A production composition path builds it from
  `CURRENT_REDUCTION_ALGORITHM_VERSION` through the crate-local
  `production_caps()` (`epiphany-textproj`, `epiphany-testkit`).
- `BundleCapabilities::synthetic_for_fixture(v)` is for format and container
  fixtures that deliberately exercise an arbitrary wire version, so a fixture
  asserting behavior at version 7 keeps asserting it when the authority moves.
- `synthetic_for_fixture` never appears on a production path. Nothing
  mechanical enforces this; it is a review rule.

## The text projection refuses canonical bases

A base-bearing document does not round-trip through text, and every side
refuses: projection (`ProjectError::CanonicalBaseUnsupported` from
`document_from_bundle`, `project_bundle` and `project_text_document`),
serialization (`SerializeError::CanonicalBaseUnsupported`), and parsing (a
`(canonical-base …)` line is `TextError::NotCanonical`). The grammar keeps the
production only as what the refusal is defined against.

There is one intentional hole: the crate-private `render_text_document`, which
renders without the refusal so the `canonical_base_present` reject vector can
contain the spelling it asserts is refused. It stays private, and public
documentation does not link it. Exposing it opens the hole to every caller.

## Guard every reachable path

A guard goes on every entry point a caller can reach into an invariant, not on
the path the specification's sentence names. Enumerate the public entry points
and ask which of them a caller can reach today; the named path may be the
unreachable one, leaving the reachable one open. The same holds for tests: a
regression test that drives a path production never takes guards nothing, and
a test vector built through an unguarded entry point is built through the hole.

## Goldens lock reviewed output

A golden records what the program did, not whether it was right.

- A wire golden changes only with the specification that states it.
- An engraving or editor golden is re-blessed only when a change to what the
  engraver draws has been rendered before and after and accepted by reading the
  renders. A golden is never re-blessed to make a suite green.

## Requirement labels are a public citation surface

Requirements are labeled `req:<area>:<slug>` in the `.tex` suite, and the
labels are cited from Rust as public identifiers (`ViolationKind::Requirement`,
`check_requirement` in `crates/epiphany-core/src/invariants.rs`) and from crate
documentation. A citation of a label the suite does not define is an error
wherever it would be read as current; `requirement_labels.rs` in
`epiphany-testkit` enforces the grammar, uniqueness, chapter areas and
citations.
