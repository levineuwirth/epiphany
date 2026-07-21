# Text Projection — the operation layer: scope and plan

Status: **scoping complete, one ruling needed before dispatch.**
Prepared against `master` @ `cf81074`. Every claim below was checked against the
code; where I ran a probe I say so.

---

## 1. The one decision that must be made first

**Is the operation vocabulary projected by the grammar's productions, or by
`req:textproj:value-projection`?**

They disagree, systematically. The grammar spells out `envelope`, `stamp`,
`causal`, `payload`, `kind`, `action`, `policy`, `tuplet-comp`, `cross-cutting`,
`remapping`, `reassign-entry`. What the mechanical value rule would produce for
the same types is different in three ways:

| grammar says | value rule would say |
|---|---|
| `(envelope …)` | `(operation-envelope …)` |
| `(stamp 1700 0 #x…)` — HLC flattened, 3 args | `(operation-stamp (hybrid-logical-clock 1700 0) #x…)` — 2 args |
| `(causal (…) (…))` | `(causal-context (…) (…))` |
| `(undo #x… best-effort)` | `(undo-transaction (undo-transaction-payload #x… best-effort))` |
| `(insert-event #x… <event>)` | `(insert-event (insert-event-op #x… <event>))` |

The last row is the one that matters most: **every one of the 31 kind
productions inlines its `*Op` payload record.** Under a literal reading of
clause 1 (`*Op` is a struct with named fields, so not a clause-2 newtype), the
doubled head would be required.

### Recommendation: grammar-directed, and say so

The companion already implies it. `req:textproj:value-projection`'s preamble
scopes itself: *"An operation payload **embeds** canonical values from the core
specification's Chapter 5 … It states one rule for turning any of them into
text."* The rule is for the **embedded Chapter-5 values**, and the `value`
nonterminal marks exactly where it applies. If the rule governed the operation
vocabulary too, the grammar's own productions would be redundant and partly
wrong.

What is missing is one normative sentence, because clause 1 is general enough to
be misread. Suggested shape — a new requirement, or an extension of
`req:textproj:schema-directed`:

> Chapter 6's operation vocabulary — the envelope, its stamp and causal context,
> the payload, the operation kinds and their sub-vocabularies — is projected by
> the productions of Chapter~\ref{ch:grammar}, not by
> `req:textproj:value-projection`. That rule governs exactly the `value`
> positions those productions contain.
>
> An operation kind's payload record is **inlined** into its production. The
> record exists so each variant can name a type; it is not a modelling
> distinction, and the binary form agrees — `OperationKind`'s encoding writes the
> kind tag and then delegates to the record, adding no bytes for the wrapper. It
> adds no text here either, for the same reason clause 2 makes a newtype
> transparent.

**Consequence for the implementation:** the ops projector is written *against the
grammar*, explicitly, and calls `TextValue` only at `value` positions. That is
less work than a mechanical derivation and produces far more readable text.

### One inconsistency to fix while we are here

`transpose-interval` inlines its interval as `(interval <d> <c>)`. But
`TranspositionInterval` is a core type with a `TextValue` (from `struct_codec!`)
that projects as `(transposition-interval <d> <c>)`. **Two names for one type**,
and the other one is what appears at any `value` position.

Options: (a) change the production to `"(transpose-interval (" bytes* ") " value
")"` and let the value rule name it — my recommendation, it removes the special
case; or (b) rename the inline head to `transposition-interval`.

---

## 2. What I verified (so nobody re-derives it)

**The grammar is accurate.** I mechanically compared each of the 31 `kind`
productions' argument list against its payload struct's field list: **31/31
match**, in both arity and shape (`bytes` ↔ id, `value` ↔ Chapter-5 type,
`option` ↔ `Option<…>`, `bytes*` ↔ `Vec<Id>`). No production needs changing
apart from the interval naming above.

**Core is ready — with two gaps.** A compile probe confirmed every Chapter-5 type
embedded in an operation payload already has a `TextValue`: `Event`, `Pitch`,
`IdentifiedPitch`, `PitchSpelling`, `Region`, `RegionTimeModel`,
`RepeatStructure`, `ScoreMetadata`, `Staff`, `StaffInstance`, `Voice`,
`TimeAnchor`, `TimeSignature`, `MetricGrid`, `TempoSegment`,
`StaffLineConfiguration`, `TranspositionInterval`, `Tie`, `Slur`, `Beam`,
`Spanner`, plus every embedded id.

Two do **not**: `TransactionId` and `TypedObjectId`. Both live in
`epiphany-core`, so `epiphany-ops` cannot implement the trait for them — the
orphan rule makes this a **core prerequisite**, not an ops task.

**`CanonicalSet<T>` is a type alias for `BTreeSet<T>`.** So
`TransposeIntervalOp.targets` is already covered by the existing `BTreeSet`
impl, per-site strict-increase check included. No new impl, no orphan problem.

**The three order-constrained sequences**, and exactly what each requires:

| field | type | encoder | text rule |
|---|---|---|---|
| `TransposeOp.targets` | `Vec<PitchId>` | `sorted_canonical` (**no dedup**) | non-decreasing **multiset** — duplicates legal, frozen wire form |
| `TransposeIntervalOp.targets` | `CanonicalSet<PitchId>` | set iteration | strictly increasing |
| `ChangeRegionTimeModelOp.declared_incompatible` | `Vec<EventId>` | `sorted_canonical` | non-decreasing |

Note the third: it is **not** in `envdecode.rs`'s per-site checks, because the
binary whole-envelope guard catches it (the encoder sorts, so a mis-ordered input
re-encodes differently). The text projector must likewise emit
`sorted_canonical(...)` for it, or text and binary will disagree.

---

## 3. Prerequisite (blocks everything; ~20 lines)

**P0 — `TextValue` for `TransactionId` and `TypedObjectId`, in `epiphany-core`.**

`TypedObjectId` already implements `CanonicalEncode + CanonicalDecode`, so it
drops straight into the existing `bytes_text_value!` list. `TransactionId` is
generated by `graph_id!`, like `EventId` and `PitchId`, and gets the same
traits.

**But do not just add two lines.** `bytes_text_value!` is a hand-maintained list
of 30 ids sitting parallel to the `graph_id!` invocations — the exact shape that
has cost this project four bugs, and `TransactionId`'s absence is that latent bug
*already biting*. The fix is to generate the `TextValue` impl **from `graph_id!`
itself**: every graph id is a byte-string leaf by definition, so the list that
declares them should be the list that projects them. `bytes_text_value!` then
keeps only the genuine non-`graph_id!` leaves (`ContentHash`, `TypedObjectId`).

Gate: the compile probe in §2 must pass for all four types.

---

## 4. Work breakdown

All of it lands in `epiphany-ops`, in a new `textproj` module group. The envelope
belongs with the crate that owns the envelope; the eventual `epiphany-textproj`
crate handles *document* lines (header, profile, extension, blob, canonical-base)
and needs `epiphany-bundle`, which is a later phase.

File boundaries are one-per-agent, and the type lists below are the **transitive
closure**, precomputed. Do not let an agent discover its dependencies from
compiler errors: `cargo check` reports only the frontier, which is how
`AnchorOffset`, `VoiceSelector`, `PowerOfTwo`, `OctaveOffset` and `NonZeroU16`
were each missed in the core fan-out.

### A — `textproj_leaf.rs` (one agent)

Ops-local leaves and the grammar's sub-vocabularies.

* **Leaves → byte strings.** `AuthorId` (`u128`), `ConflictId`, `EnvelopeHash`,
  and the nine `registry_id!` types. As in P0, generate these **from the
  `registry_id!` macro**, not from a parallel list.
* **Sub-vocabulary productions**, written to the grammar:
  * `action` — `ResolutionAction`, 6 variants: `accept-loser`, `keep-winner`,
    `dismiss`, `(override <bytes>)`, `(reanchor <bytes>)`,
    `(registered <bytes>)`. Verified: names are the kebabs of the variants.
  * `policy` — `UndoPolicy`: `strict-inverse`, `best-effort`, `cascade`.
  * `tuplet-comp` — `TupletCompensation`, 4 variants.
  * `cross-cutting` — `CrossCuttingValue`: `(tie <value>)` etc.
  * `remapping` — `PositionRemapping`, with
    `reassign-entry ::= "(" bytes " " ratio ")"` (a `(EventId, MusicalPosition)`
    pair — the generic tuple impl in core already covers it).
  * `TransactionCategory` — 5 variants including `Registered`.

Every `match` in a `project` must be exhaustive with no `_` arm.

### B — `textproj_kind.rs` (one agent; the largest)

The 31 `kind` productions, each inlining its `*Op` record positionally in
`CanonicalEncode` order.

* Drive the name from `OperationKindTag::catalog_name()` — production code,
  generated by `operation_kind_tag_vocabulary!`. **Never** spell a kind name as a
  literal; that list is already the single source and a parallel copy would
  reintroduce the P4 defect.
* `parse` must dispatch on an exhaustive `match` over `OperationKindTag` so a kind
  added to the vocabulary and not to the projector **fails to compile**.
* The three per-site order checks from §2. `TransposeOp`'s is a **multiset**:
  reject strictly-decreasing, accept equal neighbours. Getting this backwards
  silently breaks a frozen operation's replay.

### C — `textproj_envelope.rs` (one agent)

`envelope`, `stamp`, `causal`, `payload`, and the public entry points
`project_envelope(&OperationEnvelope) -> String` and
`parse_envelope(&str) -> Result<OperationEnvelope, …>`.

* `stamp` flattens `HybridLogicalClock` into three arguments — this is the
  grammar's shape, and the ruling in §1 is what authorises it.
* `causal`'s `vector` is a `BTreeMap<ReplicaId, u64>` → map entries; `dots` is a
  `BTreeSet<OperationId>`. Both already carry per-site strict-increase checks
  from the core impls.
* **The whole-line guard.** `parse_envelope` should end with
  `if parsed.project().render() != line { reject }`, mirroring
  `decode_envelope`'s `to_canonical_bytes() != bytes`. Unlike in core, this one
  may well be **live**, because several fields normalize. Whether it survives is
  a question for mutation testing, not assertion — see §5.

---

## 5. Verification contract (non-negotiable)

Every phase gates on: `cargo fmt --all --check`; `cargo clippy --workspace
--all-targets` → 0; full workspace tests; `RUSTDOCFLAGS="-D warnings" cargo doc
--workspace --no-deps` → 0; `cargo run -q -p epiphany-testkit --example
conformance_suite` → 8/8; zero golden churn; `latexmk -xelatex` clean for any
touched spec document.

Beyond that, three things this track has learned the hard way:

1. **Mutation-verify every check**, asserting the anchor is present before
   substituting — a `str.replace` that matches nothing looks exactly like a
   passing test. Delete each order check and each guard in turn and confirm a
   named test fails. In the core layer this found four guards that could never
   fire; they were removed. Expect the same scrutiny here, and expect a different
   answer for the whole-line guard.
2. **Exhaustive coverage, not sampled.** `gen_envelope_set` reaches only 28 of 31
   kinds and 1 of 4 payload variants — a round-trip test built on it would prove
   almost nothing. **Reuse `envdecode.rs`'s `sample_kind`**, which is an
   exhaustive `match` over `OperationKindTag` and already builds a valid payload
   for every kind, plus the three meta payloads. This is the single biggest
   de-risking asset available.
3. **Two blind spots no round-trip test can see**, both already closed in core
   and both reappearing here:
   * *Field order* — a `project`/`parse` pair that agrees with itself on a wrong
     order round-trips perfectly. Close it by mechanically diffing the identifier
     sequence in each `CanonicalEncode::encode_canonical` against each `project`,
     as was done for the 44 hand-written core impls.
   * *Constructor names* — `(insert-evnt …)` round-trips too. Here it is closed
     for free **if** every name comes from `catalog_name()`; that is the reason
     for the rule in §B.

---

## 6. Not in this phase

The document lines — `header`, `document`, `lineage`, `profile`, `extension`,
`canonical-base`, `blob` — plus `req:textproj:derived-ordering`'s sort-and-dedup
by projected form. Those need `epiphany-bundle`'s `Manifest`, `BlobRef`,
`ChunkRef`, `ProfileDeclaration`, `ExtensionDeclaration`, `SnapshotRef`, and
belong in a new `epiphany-textproj` crate. The text vector corpus and the
conformance step land with them, since a conformance vector is a whole document.
