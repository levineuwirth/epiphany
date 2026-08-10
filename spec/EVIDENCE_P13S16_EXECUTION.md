# P13-S16 — execution evidence annex

Companion to the §6 report for `spec/CONTRACT_P13S16_PROJECTION.md`.

**Status: review artifact. Tracked under `CONTRACT_P13S16_PROJECTION.md` §7's touch
row 12, and NOT part of the rung's candidate.** It documents `aee4ff9`; it is not in it.

This file was held **untracked** throughout execution and review, because it did not
appear in the contract's §2 touch table and gate 4 requires every staged path to be a §2
row — so committing it needed an amendment adding one. Amendment 1 added that row, and
the file landed with it. **The distinction survives the change:** the fourteen paths in
`aee4ff9` are the rung; this is the evidence the rung was reviewed against.

**Whitespace.** Trailing whitespace has been stripped throughout, so that
`git diff --cached --check` passes. It carried no information: it appears only where a
quoted source line or panic-message line is empty and this file's `<lineno>: <content>`
prefix left a space behind. No other character of any quotation is altered.

**Provenance.** Part 1 is generated from the retained
`cargo test --workspace --no-fail-fast` logs of each mutation run; Part 2 is
extracted from the staged tree and from `git show HEAD`. Neither part is retyped
from memory: failing output is reproduced verbatim, survivors are matched to
their `... ok` verdict lines from the same run, and source slices are located by
brace matching rather than by line number.

**Candidate this evidence describes:** 14 staged paths on `HEAD` = `34232dc`,
working tree byte-identical to the index, `git diff --cached --check` clean,
1583 passed / 0 failed / 0 ignored across 42 suites.

---

# P13-S16 — EVIDENCE ANNEX, PART 1: MUTATIONS

Generated from the retained `cargo test --workspace --no-fail-fast` logs.
Every failing test's stdout is reproduced verbatim; every required survivor
is listed by its full name with the verdict line matched from the same run.


==============================================================================
## M1 — remove pin 1's refusal
==============================================================================

Aggregate: passed 1580  failed 3

### Complete observed failure set (3)

  reduce::tests::t6_g3a_referential_loops_refuse_a_dangling_target_under_a_graph
  reduce::tests::t7_g3a_referential_preconditions_are_not_enforced_base_free
  reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused

### Verbatim failing output

---- reduce::tests::t6_g3a_referential_loops_refuse_a_dangling_target_under_a_graph stdout ----
thread 'reduce::tests::t6_g3a_referential_loops_refuse_a_dangling_target_under_a_graph' (3108457) panicked at crates/epiphany-ops/src/reduce.rs:16276:9:
assertion `left == right` failed: CreateStaffGroup carrying a non-empty members must refuse ContainerNotEmpty (P13-S16 pin 1), not TargetMissing
  left: Some(Applied)
 right: Some(NoOp { reason: PreconditionFailedUnderReduction { reason: ContainerNotEmpty } })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- reduce::tests::t7_g3a_referential_preconditions_are_not_enforced_base_free stdout ----
thread 'reduce::tests::t7_g3a_referential_preconditions_are_not_enforced_base_free' (3108459) panicked at crates/epiphany-ops/src/reduce.rs:16361:9:
assertion `left == right` failed: an empty-container precondition asks only about the carried value, so it refuses base-free too (P13-S16 pin 1a); a graph gate here would wrongly accept
  left: Some(Applied)
 right: Some(NoOp { reason: PreconditionFailedUnderReduction { reason: ContainerNotEmpty } })

---- reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused stdout ----
thread 'reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused' (3108463) panicked at crates/epiphany-ops/src/reduce.rs:16625:9:
assertion `left == right` failed: spurious order: CreateStaffGroup carrying [s] must refuse ContainerNotEmpty (P13-S16 pin 1)
  [1] spurious-order CreateStaffGroup effect: Some(Applied)
  [2] spurious-order g.members:               Some([StaffId(0000000000000002:0000000000000005)])
  [3] missing-order  g.members:               Some([StaffId(0000000000000001:0000000000000005)])
  [4] missing-order  invariant-21 violations: []
  left: Some(Applied)
 right: Some(NoOp { reason: PreconditionFailedUnderReduction { reason: ContainerNotEmpty } })

#### Structural gate — Gate 8 under M1 (refusal removed)

```
$ read the brace-matched create_staff_group slice (4551..4598), production source
  empty-members refusal present : False
  TypedObjectId::Staff( present : False
  TargetMissing present         : False
  => GATE 8 FAILS
```

### Required survivors, each by name (16)

  PASS  t8c_recarry_compares_against_the_carried_members_not_the_derived
        test reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived ... ok
  PASS  t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent
        test reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent ... ok
  PASS  u5_undoing_a_staff_strips_it_from_the_live_groups_members
        test reduce::tests::u5_undoing_a_staff_strips_it_from_the_live_groups_members ... ok
  PASS  u2a_a_live_staff_naming_the_group_blocks_its_undo
        test reduce::tests::u2a_a_live_staff_naming_the_group_blocks_its_undo ... ok
  PASS  u2bf_a_the_staff_group_guard_holds_base_free
        test reduce::tests::u2bf_a_the_staff_group_guard_holds_base_free ... ok
  PASS  u2tomb_a_a_tombstoned_referencing_staff_does_not_block_the_groups_undo
        test reduce::tests::u2tomb_a_a_tombstoned_referencing_staff_does_not_block_the_groups_undo ... ok
  PASS  u3a_minting_the_group_and_its_referencing_staff_in_one_transaction_undoes_whole
        test reduce::tests::u3a_minting_the_group_and_its_referencing_staff_in_one_transaction_undoes_whole ... ok
  PASS  m41_check_invariants_dispatches_invariant_21_staff_names_absent_group
        test invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group ... ok
  PASS  m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff
        test invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff ... ok
  PASS  invariant_21_negative_generator_breaks_staff_to_group_only
        test generators::tests::invariant_21_negative_generator_breaks_staff_to_group_only ... ok
  PASS  every_invariant_has_a_negative_generator
        test generators::tests::every_invariant_has_a_negative_generator ... ok
  PASS  every_invariant_shrinks_to_a_small_witness
        test generators::tests::every_invariant_shrinks_to_a_small_witness ... ok
  PASS  shrink_is_idempotent
        test generators::tests::shrink_is_idempotent ... ok
  PASS  negative_generators_are_reasonably_targeted
        test generators::tests::negative_generators_are_reasonably_targeted ... ok
  PASS  full_invariant_sweep_via_public_api
        test full_invariant_sweep_via_public_api ... ok
  PASS  t9_from_empty_through_all_four_g3a_ops_passes_check_invariants
        test reduce::tests::t9_from_empty_through_all_four_g3a_ops_passes_check_invariants ... ok

==============================================================================
## M2 — remove pin 2's append
==============================================================================

Aggregate: passed 1580  failed 3

### Complete observed failure set (3)

  reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused
  reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived
  reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent

### Verbatim failing output

---- reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused stdout ----
thread 'reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused' (3116580) panicked at crates/epiphany-ops/src/reduce.rs:16612:9:
assertion `left == right` failed: missing order: g.members is MAINTAINED to [s] (P13-S16 pin 2), not left empty as disposition B permitted
  [1] spurious-order CreateStaffGroup effect: Some(NoOp { reason: PreconditionFailedUnderReduction { reason: ContainerNotEmpty } })
  [2] spurious-order g.members:               None
  [3] missing-order  g.members:               Some([])
  [4] missing-order  invariant-21 violations: [InvariantViolation { invariant: StaffGroupMembershipAgreement, witness: "S->G: staff StaffId(0000000000000001:0000000000000005) names group StaffGroupId(0000000000000001:0000000000000003), but that group's members omit it" }]
  left: Some([])
 right: Some([StaffId(0000000000000001:0000000000000005)])

---- reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived stdout ----
thread 'reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived' (3116581) panicked at crates/epiphany-ops/src/reduce.rs:16742:9:
assertion `left == right` failed: and the graph's derived members must genuinely hold [s] at that moment — otherwise AlreadyApplied proves nothing about separation
  left: Some([])
 right: Some([StaffId(0000000000000001:0000000000000005)])
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent stdout ----
thread 'reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent' (3116582) panicked at crates/epiphany-ops/src/reduce.rs:16834:9:
assertion `left == right` failed: precondition: the materialized base must carry the maintained members, or this test is not exercising the reload hazard at all
  left: Some([])
 right: Some([StaffId(0000000000000001:0000000000000005)])

#### Structural gate — Gate 8 under M2 (append removed; pin 1 untouched)

```
$ read the brace-matched create_staff_group slice (4552..4598), production source
  empty-members refusal present : True
  TypedObjectId::Staff( present : False
  TargetMissing present         : False
  => GATE 8 PASSES
```

### Required survivors, each by name (9)

  PASS  u5_undoing_a_staff_strips_it_from_the_live_groups_members
        test reduce::tests::u5_undoing_a_staff_strips_it_from_the_live_groups_members ... ok
  PASS  m41_check_invariants_dispatches_invariant_21_staff_names_absent_group
        test invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group ... ok
  PASS  m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff
        test invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff ... ok
  PASS  invariant_21_negative_generator_breaks_staff_to_group_only
        test generators::tests::invariant_21_negative_generator_breaks_staff_to_group_only ... ok
  PASS  every_invariant_has_a_negative_generator
        test generators::tests::every_invariant_has_a_negative_generator ... ok
  PASS  every_invariant_shrinks_to_a_small_witness
        test generators::tests::every_invariant_shrinks_to_a_small_witness ... ok
  PASS  shrink_is_idempotent
        test generators::tests::shrink_is_idempotent ... ok
  PASS  negative_generators_are_reasonably_targeted
        test generators::tests::negative_generators_are_reasonably_targeted ... ok
  PASS  full_invariant_sweep_via_public_api
        test full_invariant_sweep_via_public_api ... ok

==============================================================================
## M3 — pin 3 writes derived members
==============================================================================

Aggregate: passed 1582  failed 1

### Complete observed failure set (1)

  reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived

### Verbatim failing output

---- reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived stdout ----
thread 'reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived' (3213206) panicked at crates/epiphany-ops/src/reduce.rs:16746:9:
assertion `left == right` failed: a byte-identical re-carry must compare against the CARRIED members and read AlreadyApplied; graph members at this moment: Some([StaffId(0000000000000001:0000000000000005)])
  left: Some(NoOp { reason: PreconditionFailedUnderReduction { reason: RecreateContentMismatch } })
 right: Some(NoOp { reason: AlreadyApplied })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

### Required survivors, each by name (4)

  PASS  t8b_the_projection_is_maintained_and_the_spurious_form_is_refused
        test reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused ... ok
  PASS  u5_undoing_a_staff_strips_it_from_the_live_groups_members
        test reduce::tests::u5_undoing_a_staff_strips_it_from_the_live_groups_members ... ok
  PASS  m41_check_invariants_dispatches_invariant_21_staff_names_absent_group
        test invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group ... ok
  PASS  m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff
        test invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff ... ok

==============================================================================
## M4 — restore group.clone() at the base seed
==============================================================================

Aggregate: passed 1582  failed 1

### Complete observed failure set (1)

  reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent

### Verbatim failing output

---- reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent stdout ----
thread 'reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent' (3221152) panicked at crates/epiphany-ops/src/reduce.rs:16838:9:
assertion `left == right` failed: across a reload the re-carry must still compare against the CARRIED members (P13-S16 pin 4); base members were Some([StaffId(0000000000000001:0000000000000005)])
  left: Some(NoOp { reason: PreconditionFailedUnderReduction { reason: RecreateContentMismatch } })
 right: Some(NoOp { reason: AlreadyApplied })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

### Required survivors, each by name (1)

  PASS  t8c_recarry_compares_against_the_carried_members_not_the_derived
        test reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived ... ok

==============================================================================
## M5 — remove pin 5's strip
==============================================================================

Aggregate: passed 1582  failed 1

### Complete observed failure set (1)

  reduce::tests::u5_undoing_a_staff_strips_it_from_the_live_groups_members

### Verbatim failing output

---- reduce::tests::u5_undoing_a_staff_strips_it_from_the_live_groups_members stdout ----
thread 'reduce::tests::u5_undoing_a_staff_strips_it_from_the_live_groups_members' (3229174) panicked at crates/epiphany-ops/src/reduce.rs:18265:9:
P13-S16 pin 5: undoing the staff must strip StaffId(0000000000000001:0000000000000005) from the live group's members
  post-undo g.members:            Some([StaffId(0000000000000001:0000000000000005)])
  invariant-21 violations:        []
  staff StaffId(0000000000000001:0000000000000005) still in graph: false
  ALL violations:                 [InvariantViolation { invariant: CrossCuttingRefsResolve, witness: "staff group StaffGroupId(0000000000000001:0000000000000001) member staff StaffId(0000000000000001:0000000000000005) is not declared" }]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

### Required survivors, each by name (9)

  PASS  t8b_the_projection_is_maintained_and_the_spurious_form_is_refused
        test reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused ... ok
  PASS  t8c_recarry_compares_against_the_carried_members_not_the_derived
        test reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived ... ok
  PASS  t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent
        test reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent ... ok
  PASS  u2a_a_live_staff_naming_the_group_blocks_its_undo
        test reduce::tests::u2a_a_live_staff_naming_the_group_blocks_its_undo ... ok
  PASS  u2bf_a_the_staff_group_guard_holds_base_free
        test reduce::tests::u2bf_a_the_staff_group_guard_holds_base_free ... ok
  PASS  u2tomb_a_a_tombstoned_referencing_staff_does_not_block_the_groups_undo
        test reduce::tests::u2tomb_a_a_tombstoned_referencing_staff_does_not_block_the_groups_undo ... ok
  PASS  u3a_minting_the_group_and_its_referencing_staff_in_one_transaction_undoes_whole
        test reduce::tests::u3a_minting_the_group_and_its_referencing_staff_in_one_transaction_undoes_whole ... ok
  PASS  m41_check_invariants_dispatches_invariant_21_staff_names_absent_group
        test invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group ... ok
  PASS  m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff
        test invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff ... ok

==============================================================================
## M6a — delete the S->G dispatch call
==============================================================================

Aggregate: passed 1576  failed 7

### Complete observed failure set (7)

  full_invariant_sweep_via_public_api
  generators::tests::every_invariant_has_a_negative_generator
  generators::tests::every_invariant_shrinks_to_a_small_witness
  generators::tests::invariant_21_negative_generator_breaks_staff_to_group_only
  generators::tests::negative_generators_are_reasonably_targeted
  generators::tests::shrink_is_idempotent
  invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group

### Verbatim failing output

---- full_invariant_sweep_via_public_api stdout ----
thread 'full_invariant_sweep_via_public_api' (3704413) panicked at crates/epiphany-core/tests/score_graph.rs:148:9:
StaffGroupMembershipAgreement not reported on its negative graph
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- generators::tests::every_invariant_has_a_negative_generator stdout ----
thread 'generators::tests::every_invariant_has_a_negative_generator' (3704120) panicked at crates/epiphany-core/src/generators.rs:1015:13:
negative generator for StaffGroupMembershipAgreement did not violate it; full report: []

---- generators::tests::every_invariant_shrinks_to_a_small_witness stdout ----
thread 'generators::tests::every_invariant_shrinks_to_a_small_witness' (3704121) panicked at crates/epiphany-core/src/generators.rs:954:5:
shrink starting point must violate the target invariant

---- generators::tests::invariant_21_negative_generator_breaks_staff_to_group_only stdout ----
thread 'generators::tests::invariant_21_negative_generator_breaks_staff_to_group_only' (3704122) panicked at crates/epiphany-core/src/generators.rs:1088:9:
assertion `left == right` failed: raw: expected exactly the invariant-21 S->G violation and nothing else, got []
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- generators::tests::negative_generators_are_reasonably_targeted stdout ----
thread 'generators::tests::negative_generators_are_reasonably_targeted' (3704123) panicked at crates/epiphany-core/src/generators.rs:1175:13:
StaffGroupMembershipAgreement not among {}

---- generators::tests::shrink_is_idempotent stdout ----
thread 'generators::tests::shrink_is_idempotent' (3704126) panicked at crates/epiphany-core/src/generators.rs:954:5:
shrink starting point must violate the target invariant

---- invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group stdout ----
thread 'invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group' (3704211) panicked at crates/epiphany-core/src/invariants.rs:6288:9:
assertion `left == right` failed: expected exactly the invariant-21 S->G violation and nothing else, got []
  left: 0
 right: 1

#### Structural gate — Gate 12 under M6a (S->G dispatch call deleted)

```
$ grep -c "fn check_staff_names_absent_group" crates/epiphany-core/src/invariants.rs   # a
1
$ grep -c "fn check_group_lists_unowned_staff" crates/epiphany-core/src/invariants.rs   # b
1
$ grep -c "idx.check_staff_names_absent_group(&mut v)" crates/epiphany-core/src/invariants.rs   # c
0
$ grep -c "idx.check_group_lists_unowned_staff(&mut v)" crates/epiphany-core/src/invariants.rs   # d
1
  => GATE 12 FAILS  (a/b/c/d = 1/1/0/1)
```

### Required survivors, each by name (2)

  PASS  m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff
        test invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff ... ok
  PASS  u5_undoing_a_staff_strips_it_from_the_live_groups_members
        test reduce::tests::u5_undoing_a_staff_strips_it_from_the_live_groups_members ... ok

==============================================================================
## M6b — delete the G->S dispatch call
==============================================================================

Aggregate: passed 1582  failed 1

### Complete observed failure set (1)

  invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff

### Verbatim failing output

---- invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff stdout ----
thread 'invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff' (3710920) panicked at crates/epiphany-core/src/invariants.rs:6361:9:
assertion `left == right` failed: expected exactly the invariant-21 G->S violation and nothing else, got []
  left: 0
 right: 1

#### Structural gate — Gate 12 under M6b (G->S dispatch call deleted)

```
$ grep -c "fn check_staff_names_absent_group" crates/epiphany-core/src/invariants.rs   # a
1
$ grep -c "fn check_group_lists_unowned_staff" crates/epiphany-core/src/invariants.rs   # b
1
$ grep -c "idx.check_staff_names_absent_group(&mut v)" crates/epiphany-core/src/invariants.rs   # c
1
$ grep -c "idx.check_group_lists_unowned_staff(&mut v)" crates/epiphany-core/src/invariants.rs   # d
0
  => GATE 12 FAILS  (a/b/c/d = 1/1/1/0)
```

### Required survivors, each by name (7)

  PASS  m41_check_invariants_dispatches_invariant_21_staff_names_absent_group
        test invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group ... ok
  PASS  invariant_21_negative_generator_breaks_staff_to_group_only
        test generators::tests::invariant_21_negative_generator_breaks_staff_to_group_only ... ok
  PASS  every_invariant_has_a_negative_generator
        test generators::tests::every_invariant_has_a_negative_generator ... ok
  PASS  every_invariant_shrinks_to_a_small_witness
        test generators::tests::every_invariant_shrinks_to_a_small_witness ... ok
  PASS  shrink_is_idempotent
        test generators::tests::shrink_is_idempotent ... ok
  PASS  negative_generators_are_reasonably_targeted
        test generators::tests::negative_generators_are_reasonably_targeted ... ok
  PASS  full_invariant_sweep_via_public_api
        test full_invariant_sweep_via_public_api ... ok

==============================================================================
## M7a — revert Staff.group's doc block to B
==============================================================================

Aggregate: passed 1582  failed 1

### Complete observed failure set (1)

  graph::g3a_tests::t14_staff_group_field_doc_comment_states_sole_authority

### Verbatim failing output

---- graph::g3a_tests::t14_staff_group_field_doc_comment_states_sole_authority stdout ----
thread 'graph::g3a_tests::t14_staff_group_field_doc_comment_states_sole_authority' (3379269) panicked at crates/epiphany-core/src/graph.rs:2191:9:
Staff.group's doc comment must state the projection is maintained from it (P13-S16 disposition A); block was:
    /// M7a ACTIVE — RESTORE THE DISPOSITION-A WORDING
    /// Which staff group (if any) this staff belongs to. **The sole authority
    /// for group membership** (genesis tranche G3a,
    /// `spec/CONTRACT_GENESIS_G3A_ENTITIES.md` §1.1, disposition B, filed as
    /// P13-S16): every consumer MUST read membership from this field, not
    /// from [`StaffGroup::members`], which is a non-authoritative denormalized
    /// projection that may disagree with this field in either direction.

note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

### Required survivors, each by name (1)

  PASS  t14_staff_group_members_field_doc_comment_states_non_authoritative_projection
        test graph::g3a_tests::t14_staff_group_members_field_doc_comment_states_non_authoritative_projection ... ok

==============================================================================
## M7b — revert StaffGroup.members's doc block to B
==============================================================================

Aggregate: passed 1582  failed 1

### Complete observed failure set (1)

  graph::g3a_tests::t14_staff_group_members_field_doc_comment_states_non_authoritative_projection

### Verbatim failing output

---- graph::g3a_tests::t14_staff_group_members_field_doc_comment_states_non_authoritative_projection stdout ----
thread 'graph::g3a_tests::t14_staff_group_members_field_doc_comment_states_non_authoritative_projection' (3418176) panicked at crates/epiphany-core/src/graph.rs:2220:9:
StaffGroup.members's doc comment must state it is maintained from Staff.group; block was:
    /// M7b ACTIVE — RESTORE THE DISPOSITION-A WORDING
    /// A **non-authoritative denormalized projection** of group membership
    /// (genesis tranche G3a, `spec/CONTRACT_GENESIS_G3A_ENTITIES.md` §1.1,
    /// disposition B, filed as P13-S16). [`Staff::group`] is the sole
    /// authority: this field MUST NOT be read to decide whether a staff is in
    /// a group, and MAY be stale in **both** directions — a member missing
    /// here while `Staff.group` names this group, or a staff listed here
    /// while its own `Staff.group` is `None` or names a different group.

note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

### Required survivors, each by name (1)

  PASS  t14_staff_group_field_doc_comment_states_sole_authority
        test graph::g3a_tests::t14_staff_group_field_doc_comment_states_sole_authority ... ok

==============================================================================
## M8 — reinstate the dead liveness loop
==============================================================================

Aggregate: passed 1583  failed 0

### Complete observed failure set (0)

  (empty — no test failed)
#### Structural gate — Gate 8 under M8 (dead liveness loop reinstated)

```
$ read the brace-matched create_staff_group slice (4551..4612), production source
  empty-members refusal present : True
  TypedObjectId::Staff( present : True
  TargetMissing present         : True
  => GATE 8 FAILS
```

### Required survivors, each by name (0)

  §3 names no individual survivors: "every behavioural assertion".
  Discharged by the aggregate above — passed 1583, failed 0.

==============================================================================
## M9 — graph-gate pin 1's refusal
==============================================================================

Aggregate: passed 1582  failed 1

### Complete observed failure set (1)

  reduce::tests::t7_g3a_referential_preconditions_are_not_enforced_base_free

### Verbatim failing output

---- reduce::tests::t7_g3a_referential_preconditions_are_not_enforced_base_free stdout ----
thread 'reduce::tests::t7_g3a_referential_preconditions_are_not_enforced_base_free' (3459823) panicked at crates/epiphany-ops/src/reduce.rs:16361:9:
assertion `left == right` failed: an empty-container precondition asks only about the carried value, so it refuses base-free too (P13-S16 pin 1a); a graph gate here would wrongly accept
  left: Some(Applied)
 right: Some(NoOp { reason: PreconditionFailedUnderReduction { reason: ContainerNotEmpty } })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

### Required survivors, each by name (1)

  PASS  t8b_the_projection_is_maintained_and_the_spurious_form_is_refused
        test reduce::tests::t8b_the_projection_is_maintained_and_the_spurious_form_is_refused ... ok

---

# P13-S16 — EVIDENCE ANNEX, PART 2: GATES AND SOURCE QUOTATIONS

Commands with their outputs, and source slices read from the staged tree.


## Gate 1
```
$ cargo test --workspace --no-fail-fast
suites 42  passed 1583  failed 0  ignored 0
exit 0
```

## Gate 2
```
$ cargo +1.95.0 clippy --workspace --all-targets -- -D warnings
(no error or warning lines)
```

## Gate 3
```
$ cargo +1.95.0 fmt -p epiphany-ops -p epiphany-core -p epiphany-textproj -p epiphany-testkit --check
(no output)
```

## Gate 4
```
$ git diff --cached --check
(no output; exit 0)

$ git diff --cached --name-only
crates/epiphany-core/src/generators.rs
crates/epiphany-core/src/graph.rs
crates/epiphany-core/src/invariants.rs
crates/epiphany-ops/src/lib.rs
crates/epiphany-ops/src/payload.rs
crates/epiphany-ops/src/reduce.rs
crates/epiphany-ops/src/valuegen.rs
crates/epiphany-testkit/src/roundtrip.rs
crates/epiphany-textproj/src/serialize.rs
spec/PASS13_CANDIDATES.md
spec/core_spec.pdf
spec/core_spec.tex
spec/operation_catalog.pdf
spec/operation_catalog.tex
```

Row 11 (`requirement_labels.rs`) is the only §2 row not staged: unused per pin 10a.

## Gate 5
```
$ git diff --stat HEAD -- spec/vectors/decode_vectors.txt
(no output)
```

## Gate 6 — invariant 21 reached through `check_invariants`

```
$ cargo test -p epiphany-core --lib m41
test invariants::s16_agreement_dispatch_tests::m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff ... ok
test invariants::s16_agreement_dispatch_tests::m41_check_invariants_dispatches_invariant_21_staff_names_absent_group ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 322 filtered out; finished in 0.00s

$ cargo test -p epiphany-core --lib invariant_21_negative
test generators::tests::invariant_21_negative_generator_breaks_staff_to_group_only ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 323 filtered out; finished in 0.00s
```

The exact-set and direction assertions these three carry are quoted under items
2d/2e below; a passing exact-set assertion is the observation, and no runtime
witness dump is claimed.

### Checked *in addition*, never instead
```
$ grep -n 'assert_eq!(GraphInvariant::all().len(), 21);' crates/epiphany-core/src/invariants.rs
6163:        assert_eq!(GraphInvariant::all().len(), 21);

$ grep -n 'pub fn all() -> \[GraphInvariant; 21\]' crates/epiphany-core/src/invariants.rs
169:    pub fn all() -> [GraphInvariant; 21] {

$ grep -n 'This enumeration contains exactly' spec/core_spec.tex
6673:This enumeration contains exactly \textbf{21} invariants. (Earlier

$ count \item entries inside the req:graph:score-graph-invariants requirement box
21
```

Invariant 21 is a 21st `\item` inside the pre-existing box; no new label is minted.

## Gate 7 — every test named in pin 8, final candidate

```
$ cargo test -p epiphany-ops --lib u2a_a_live_staff_naming_the_group_blocks_its_undo
test reduce::tests::u2a_a_live_staff_naming_the_group_blocks_its_undo ... ok

$ cargo test -p epiphany-ops --lib u2bf_a_the_staff_group_guard_holds_base_free
test reduce::tests::u2bf_a_the_staff_group_guard_holds_base_free ... ok

$ cargo test -p epiphany-ops --lib u2tomb_a_a_tombstoned_referencing_staff_does_not_block_the_groups_undo
test reduce::tests::u2tomb_a_a_tombstoned_referencing_staff_does_not_block_the_groups_undo ... ok

$ cargo test -p epiphany-ops --lib u3a_minting_the_group_and_its_referencing_staff_in_one_transaction_undoes_whole
test reduce::tests::u3a_minting_the_group_and_its_referencing_staff_in_one_transaction_undoes_whole ... ok

```

None changed; each was expected to survive, and each was run rather than assumed.

## Gate 8 — brace-matched `create_staff_group`, production source
```rust
4551:     fn create_staff_group(
4552:         &mut self,
4553:         env: &OperationEnvelope,
4554:         op: &CreateStaffGroupOp,
4555:     ) -> OperationEffect {
4556:         let gobj = TypedObjectId::StaffGroup(op.staff_group_id());
4557:         match self.objects.get(&gobj) {
4558:             Some(ObjectState::Live) => {
4559:                 let identical = self
4560:                     .staff_group_values
4561:                     .get(&op.staff_group_id())
4562:                     .is_some_and(|known| known == &op.group);
4563:                 return if identical {
4564:                     OperationEffect::NoOp {
4565:                         reason: NoOpReason::AlreadyApplied,
4566:                     }
4567:                 } else {
4568:                     OperationEffect::NoOp {
4569:                         reason: NoOpReason::PreconditionFailedUnderReduction {
4570:                             reason: PreconditionFailureReason::RecreateContentMismatch,
4571:                         },
4572:                     }
4573:                 };
4574:             }
4575:             Some(ObjectState::Tombstoned { .. }) => {
4576:                 return OperationEffect::NoOp {
4577:                     reason: NoOpReason::TargetTombstoned,
4578:                 }
4579:             }
4580:             None => {}
4581:         }
4582:         // Reject a carried non-empty `members`: the mint authors the group, and
4583:         // membership is maintained from `Staff.group` (P13-S16 pin 2). Unlike the
4584:         // sibling mints' referential preconditions this is NOT graph-gated — an
4585:         // empty-container precondition asks only about the carried value, so it
4586:         // holds base-free as well (pin 1; `t7` asserts the inversion).
4587:         if !op.group.members.is_empty() {
4588:             return container_not_empty();
4589:         }
4590:         if let Some(score) = self.graph.as_mut() {
4591:             score.staff_groups.push(op.group.clone());
4592:         }
4593:         self.mint_container(env, gobj);
4594:         self.staff_group_values
4595:             .insert(op.staff_group_id(), op.group.clone());
4596:         OperationEffect::Applied
4597:     }
```

## Gate 9 — `t8c` and `t8d`, by name, final candidate

```
$ cargo test -p epiphany-ops --lib t8c_recarry_compares_against_the_carried_members_not_the_derived
test reduce::tests::t8c_recarry_compares_against_the_carried_members_not_the_derived ... ok

$ cargo test -p epiphany-ops --lib t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent
test reduce::tests::t8d_recarry_after_reduction_onto_a_materialized_base_stays_idempotent ... ok

```

## Gate 10
```
$ grep -n "pub const CURRENT_REDUCTION_ALGORITHM_VERSION" crates/epiphany-ops/src/lib.rs
181:pub const CURRENT_REDUCTION_ALGORITHM_VERSION: u32 = 1;

$ git show HEAD:crates/epiphany-ops/src/lib.rs | grep -n "pub const CURRENT_REDUCTION_ALGORITHM_VERSION"
152:pub const CURRENT_REDUCTION_ALGORITHM_VERSION: u32 = 0;
```

### `Bumps` list verbatim
```rust
137: ///
138: /// This is a fact about the *baseline*, not about the current value: read the
139: /// declaration below for that, and the `Bumps` list for how the series got
140: /// there.
141: ///
142: /// # Bumps
143: ///
144: /// * `0` — the baseline. The semantics `canonical_reduction_order` and
145: ///   `reduce_onto` implement as of P13-S27 (2026-08-08). No earlier version
146: ///   exists; nothing predates this constant.
147: /// * `1` — **P13-S16** (2026-08-09, `spec/CONTRACT_P13S16_PROJECTION.md`), the
148: ///   first real bump. It carries **one change of each kind**, and the two are
149: ///   not interchangeable:
150: ///   - a **reduction verdict** change — `CreateStaffGroup` carrying a non-empty
151: ///     `members` now reduces to a `ContainerNotEmpty` no-op where version `0`
152: ///     applied it. The effect recorded for that operation differs.
153: ///   - a **canonical reduced state** change — `CreateStaff` carrying
154: ///     `group: Some(g)` now appends the staff to `g`'s `members`. Its verdict is
155: ///     unchanged (it still applies); the graph the reduction produces is what
156: ///     differs.
157: ///
158: ///   Either alone would require this bump. A base materialized under `0` holds
159: ///   state this version would not have computed, so it must be rebuilt rather
160: ///   than reused.
161: ///
162: /// A bump without its entry above leaves a number nobody can account for: this
163: /// list is the only record of *why* each version exists.
164: ///
165: /// **No mechanism detects a missed bump.** The authority check compares
166: /// *declared* versions, so it catches a base stamped with a version other than
167: /// this one — it cannot notice that the semantics changed while the constant
168: /// stood still. Every future change to a canonical reduction verdict **or to
169: /// canonical reduced state** must move this constant and add its entry here.
170: /// Both classes are named because a change that leaves every verdict intact
171: /// while altering the reduced graph is the easier one to overlook, and it
172: /// invalidates a base just as completely. That discipline is the whole
173: /// guarantee.
174: ///
175: /// # Layering
176: ///
177: /// This is a plain `u32`, and `epiphany-ops` **MUST NOT** gain a dependency on
178: /// `epiphany-bundle` in order to use that crate's `ReductionAlgorithmVersion`
179: /// wrapper. The wrapper is constructed at the composition boundary by whoever
180: /// depends on both (P13-S27 pin 1, §0.3).
181: pub const CURRENT_REDUCTION_ALGORITHM_VERSION: u32 = 1;
```

## Gate 11 — tripwires, BEFORE (HEAD) vs AFTER (staged), verbatim


### 11a  serialize.rs assertion
```
BEFORE (HEAD):
665:            ReductionAlgorithmVersion(0),

AFTER (staged):
667:            ReductionAlgorithmVersion(1),
```

### 11b  fixture capability and staged base
```
BEFORE (HEAD):
400:                reduction_algorithm_version: ReductionAlgorithmVersion(0),   <-- NOT a gate-11 site:
     an `acceleration_snapshots` entry, UNCHANGED in the staged tree and unrelated
     to test 10b's canonical base. The authority check governs canonical bases only,
     so this literal is unaffected by the bump. Listed here because the grep matched
     it, and labelled rather than silently dropped.
872:    /// The fixture is built with `synthetic_for_fixture(0)` and commits a base
901:            BundleCapabilities::synthetic_for_fixture(0),
918:                    reduction_algorithm_version: ReductionAlgorithmVersion(0),

AFTER (staged):
872:    /// The fixture is built with `synthetic_for_fixture(1)` and commits a base
909:            BundleCapabilities::synthetic_for_fixture(1),
926:                    reduction_algorithm_version: ReductionAlgorithmVersion(1),
```

### 11c  success-arm assertion
```
BEFORE (HEAD):
941:                    ReductionAlgorithmVersion(0)

AFTER (staged):
949:                    ReductionAlgorithmVersion(1)
```

### 11d  mutation-only Err arm
```
BEFORE (HEAD):
947:                assert_eq!(base, ReductionAlgorithmVersion(0));

AFTER (staged):
955:                assert_eq!(base, ReductionAlgorithmVersion(1));
```

### 11e  literal-preservation doc comments (staged)
```rust
866:     /// P13-S27 test 10b — the test M5b breaks, and **the only place in the rung
867:     /// where the real authority meets a canonical base**. In `epiphany-testkit`,
868:     /// which may reach the real constant.
869:     ///
870:     /// # Two provably independent operands
871:     ///
872:     /// The fixture is built with `synthetic_for_fixture(1)` and commits a base
873:     /// carrying the **literal** `ReductionAlgorithmVersion(1)`; the reopen then
874:     /// supplies `production_caps()`, which wraps the real constant. One operand
875:     /// is a literal written into a fixture, the other is the authority read at
876:     /// the reopen — neither derived from the other.
877:     ///
878:     /// **The literals track the constant's value by hand.** P13-S16 moved the
879:     /// authority `0` → `1`, so every literal below moved with it — by editing,
880:     /// never by referencing `CURRENT_REDUCTION_ALGORITHM_VERSION`. That this
881:     /// test must be edited whenever the authority moves is the **point**, not
882:     /// friction to be engineered away: it is the tripwire. A future rung that
883:     /// bumps the constant will see this test fail, and updating these literals
884:     /// is how it acknowledges the bump.
885:     ///
886:     /// **Round 3 caught the alternative**: if both the supplied capability and
887:     /// the base version descended from `CURRENT_REDUCTION_ALGORITHM_VERSION`,
888:     /// both would move together under M5b's mutation and the comparison would
889:     /// pass for every value — §0.1's own tautology, reproduced inside the
890:     /// mutation built to detect it. **Do not tidy either literal into the
891:     /// constant** (§7 item 4b).
892:     ///
893:     /// # Both `Result` arms are written deliberately
894:     ///
895:     /// Round 5 pinned this: "assert it opens" was not enough, because under M5b
896:     /// the reopen returns `Err` and **a `#[test]` returning `Err` asserts
897:     /// nothing about that error's fields**. The `Err` arm below runs only under
898:     /// mutation, and it is what makes M5b's required two-field observation a
899:     /// *verified* one rather than a stack trace. The third arm exists so a
900:     /// *different* error under mutation is reported rather than read as success.
901:     #[test]
902:     fn a_base_bearing_bundle_reopened_under_the_real_authority_validates() {
903:         use epiphany_bundle::{BundleCapabilities, BundleError};
904:
905:         let mut bundle = Bundle::create(
906:             MemStore::new(),
907:             FileUuid([0x5E; 16]),
908:             Manifest::empty(DocumentId([0x5E; 16])),
909:             BundleCapabilities::synthetic_for_fixture(1),
910:         )
911:         .expect("fixture bundle creates");
912:
913:         let staged = StagedChunk {
914:             kind: ChunkKind::Snapshot,
915:             schema_version: SchemaVersion::V0,
916:             payload: vec![5u8, 5, 5],
917:         };
918:         bundle
919:             .commit(&[staged], |ctx| {
920:                 let mut m = ctx.previous_manifest.clone();
921:                 let root = ctx.new_chunks[0];
922:                 m.canonical_base = Some(SnapshotRef {
923:                     snapshot_id: SnapshotId([0x5E; 16]),
924:                     covers_causal_frontier: FrontierBytes::empty(),
925:                     // A deliberate LITERAL — not the constant. See above.
926:                     reduction_algorithm_version: ReductionAlgorithmVersion(1),
927:                     profile_id: ProfileId::Full,
928:                     hash: root.hash,
929:                     root,
930:                 });
931:                 m
932:             })
933:             .expect("committing the base under the matching synthetic capability succeeds");
934:         let image = bundle.into_store().into_bytes();
935:
936:         match Bundle::open(MemStore::from_bytes(image), crate::production_caps()) {
937:             Ok(reopened) => {
938:                 assert!(
939:                     reopened.manifest().canonical_base.is_some(),
940:                     "the base must survive the reopen, or this asserts nothing"
941:                 );
942:                 assert_eq!(
943:                     reopened
944:                         .manifest()
945:                         .canonical_base
946:                         .as_ref()
947:                         .unwrap()
948:                         .reduction_algorithm_version,
949:                     ReductionAlgorithmVersion(1)
950:                 );
951:             }
952:             Err(BundleError::CanonicalBaseRequiresRebuild { base, current }) => {
953:                 // Reached only under M5b. Assert both fields, then fail loudly
954:                 // quoting them — that is the mutation's required observation.
955:                 assert_eq!(base, ReductionAlgorithmVersion(1));
956:                 panic!(
957:                     "M5b observation: base={} current={} — the authority is load-bearing here",
958:                     base.0, current.0
959:                 );
960:             }
961:             Err(other) => panic!("unexpected error, not the authority verdict: {other:?}"),
962:         }
963:     }
```
```rust
642:     /// P13-S27 test 10a — the test M5a breaks. **In `epiphany-textproj`**,
643:     /// because `epiphany-bundle` must not depend on `epiphany-ops` (pin 1, §0.3)
644:     /// and so no test there can reach the real authority.
645:     ///
646:     /// # The `1` is a deliberate LITERAL, and that is load-bearing
647:     ///
648:     /// Comparing against `CURRENT_REDUCTION_ALGORITHM_VERSION` would compare the
649:     /// constant with itself laundered through one function call: mutate the
650:     /// constant and **both sides move**, so the assertion would hold for every
651:     /// value and M5a could not break it. **Do not "tidy" this into the
652:     /// constant** — doing so makes M5a vacuous while leaving every test green,
653:     /// a failure invisible to the suite (contract §7 item 4b exists to catch it).
654:     ///
655:     /// **This test failed when P13-S16 bumped the authority `0` → `1`, exactly as
656:     /// S27 predicted it would**, and the literal below was updated by hand. That
657:     /// is the tripwire working, not friction: editing this literal is how a rung
658:     /// *states* that the authority moved. A future bump must break this test
659:     /// again.
660:     #[test]
661:     fn serialize_document_supplies_the_real_reduction_authority() {
662:         let document = minimal_document(42);
663:         let bundle = serialize_document(&document, MemStore::new(), FileUuid([1; 16]))
664:             .expect("a base-free document serializes");
665:         assert_eq!(
666:             bundle.capabilities().current_reduction_version,
667:             ReductionAlgorithmVersion(1),
668:             "the production writer must supply the real authority, not a literal of its own"
669:         );
670:     }
```

## Gate 12 — dispatcher and both definitions


> **Method note.** M6a/M6b are applied by **deleting** the dispatch line, as M6
> specifies, and **both the full-suite run and the structural gate above were taken
> under that deletion.** An earlier pass commented the line out instead. The
> compiled behaviour is identical, so the failure sets are unchanged (seven and
> one) — but the contract's unanchored needle still matches a commented-out call,
> so gate 12 read `1/1/1/1` and appeared to pass under M6a. **The mutation must
> remove the line, not disable it** — a commented-out call is invisible to the
> compiler and visible to grep, which is exactly backwards for this gate. The
> earlier comment-out runs were discarded rather than reused.

### `check_invariants` signature and the invariant-21 call sites

```rust
278: pub fn check_invariants(score: &Score) -> Vec<InvariantViolation> {
304:     // Invariant 21's two directions, dispatched separately (P13-S16 pin 6b).
305:     // Each call site is independently deletable, which is what M6a and M6b
306:     // delete; a single call handling both would leave the mutation with no way
307:     // to fail one direction while the other still reports.
308:     idx.check_staff_names_absent_group(&mut v);
309:     idx.check_group_lists_unowned_staff(&mut v);
```

### Enclosing implementation header for both methods

```rust
385: impl<'a> GraphIndex<'a> {
```

### The four counts, final candidate

```
$ grep -c "fn check_staff_names_absent_group" crates/epiphany-core/src/invariants.rs   # a
1
$ grep -c "fn check_group_lists_unowned_staff" crates/epiphany-core/src/invariants.rs   # b
1
$ grep -c "idx.check_staff_names_absent_group(&mut v)" crates/epiphany-core/src/invariants.rs   # c
1
$ grep -c "idx.check_group_lists_unowned_staff(&mut v)" crates/epiphany-core/src/invariants.rs   # d
1
  => GATE 12 PASSES  (a/b/c/d = 1/1/1/1)
```

### S->G definition
```rust
2795:     /// Direction **S→G**: every live staff naming a group appears in that
2796:     /// group's `members`. A maintenance gap — pin 2 failing to append — shows up
2797:     /// here.
2798:     ///
2799:     /// A staff naming a group that does not exist is **not** flagged here:
2800:     /// dangling reference resolution belongs to the referential invariants, and
2801:     /// abstaining keeps this invariant's witnesses about *agreement* only.
2802:     fn check_staff_names_absent_group(&self, out: &mut Vec<InvariantViolation>) {
2803:         let members: HashMap<StaffGroupId, BTreeSet<StaffId>> = self
2804:             .score
2805:             .staff_groups
2806:             .iter()
2807:             .map(|group| (group.id, group.members.iter().copied().collect()))
2808:             .collect();
2809:         for staff in &self.score.staves {
2810:             let Some(group_id) = staff.group else {
2811:                 continue;
2812:             };
2813:             if let Some(ids) = members.get(&group_id) {
2814:                 if !ids.contains(&staff.id) {
2815:                     out.push(InvariantViolation::new(
2816:                         GraphInvariant::StaffGroupMembershipAgreement,
2817:                         format!(
2818:                             "S->G: staff {:?} names group {:?}, but that group's members omit it",
2819:                             staff.id, group_id
2820:                         ),
2821:                     ));
2822:                 }
2823:             }
2824:         }
2825:     }
```

### G->S definition
```rust
2827:     /// Direction **G→S**: every staff a group lists names that group in its own
2828:     /// `group` field. A stale projection — a member left behind, or one pointing
2829:     /// at a different group — shows up here.
2830:     ///
2831:     /// A member id with no live staff is **not** flagged here, for the same
2832:     /// reason as the S→G direction.
2833:     fn check_group_lists_unowned_staff(&self, out: &mut Vec<InvariantViolation>) {
2834:         let owner: HashMap<StaffId, Option<StaffGroupId>> = self
2835:             .score
2836:             .staves
2837:             .iter()
2838:             .map(|staff| (staff.id, staff.group))
2839:             .collect();
2840:         for group in &self.score.staff_groups {
2841:             for member in &group.members {
2842:                 let Some(named) = owner.get(member) else {
2843:                     continue;
2844:                 };
2845:                 if *named != Some(group.id) {
2846:                     out.push(InvariantViolation::new(
2847:                         GraphInvariant::StaffGroupMembershipAgreement,
2848:                         format!(
2849:                             "G->S: group {:?} lists staff {:?}, but that staff names {:?}",
2850:                             group.id, member, named
2851:                         ),
2852:                     ));
2853:                 }
2854:             }
2855:         }
2856:     }
```

## Gate 13 / item 2f — `u5`
```rust
18157:     /// (u5) **P13-S16 pin 5a.** Undoing a `CreateStaff` strips the staff's id
18158:     /// from every live group's `members`.
18159:     ///
18160:     /// **This is the unguarded direction of projection maintenance** — *not* of
18161:     /// invariant 21; see assertion 4 for why the distinction is load-bearing. The
18162:     /// reverse direction is blocked: `undo_strand_block`'s
18163:     /// `TypedObjectId::StaffGroup` arm refuses to undo a group a live staff still
18164:     /// names (see `u2a`). Nothing blocks undoing the *staff*, so before pin 5 the
18165:     /// undo removed it from `Score.staves` and left its id sitting in
18166:     /// `g.members`: a group naming a staff that no longer exists.
18167:     ///
18168:     /// **Nothing permanent exercised this sequence before, checked rather than
18169:     /// assumed.** Pin 8's four are group-undo guards; `u2tomb_a` undoes a staff
18170:     /// but undoes the group along with it — it asserts *"the group leaves
18171:     /// `Score.staff_groups`"* — so no *live* group's `members` is ever inspected.
18172:     /// `m41`/`m41b` build materialized fixtures and never run the reducer's undo
18173:     /// path at all. A mutation demonstrates the hazard once; only a test keeps it
18174:     /// demonstrated.
18175:     ///
18176:     /// **Mutation (M5):** remove pin 5's strip from the `Staff` arm of
18177:     /// `materialize_graph_tombstones`; assertion 2 fails with `s` still in
18178:     /// `members`.
18179:     ///
18180:     /// **Observation harness (same rule as pin 7a).** Both the post-undo
18181:     /// `members` and the invariant-21 violations are bound **before assertion
18182:     /// 2**, and both appear in every assertion's message. Under M5 it is
18183:     /// assertion 2 that fires, so assertion 3 never executes — computing the
18184:     /// violations only where assertion 3 needs them would put M5's required
18185:     /// witness behind an assertion M5 guarantees is unreachable. **The state a
18186:     /// mutation owes must be bound before the assertion that mutation trips.**
18187:     #[test]
18188:     fn u5_undoing_a_staff_strips_it_from_the_live_groups_members() {
18189:         let identity = IdentityContext::new(ReplicaId(1));
18190:         let instrument_id = InstrumentId::new(ReplicaId(9), 1);
18191:         let mut base = Score::empty(identity);
18192:         base.instruments
18193:             .push(crate::valuegen::instrument(instrument_id));
18194:
18195:         let group_id = StaffGroupId::new(ReplicaId(1), 1);
18196:         let staff_id = StaffId::new(ReplicaId(1), 5);
18197:         let tx = TransactionId::new(ReplicaId(1), 900);
18198:
18199:         let mut staff_value = crate::valuegen::staff(staff_id, instrument_id);
18200:         staff_value.group = Some(group_id);
18201:
18202:         let mut set = OperationSet::new();
18203:         set.accept_all(vec![
18204:             // The group is authored OUTSIDE the transaction, so undoing the
18205:             // transaction takes the staff and leaves the group live — which is
18206:             // what gives assertion 1 something to hold.
18207:             staff_group_env(
18208:                 1,
18209:                 0,
18210:                 10,
18211:                 CausalContext::new(),
18212:                 crate::valuegen::staff_group(group_id, vec![]),
18213:             ),
18214:             declare_transaction(1, 1, 20, seen_r1(0), tx),
18215:             tx_member(
18216:                 1,
18217:                 2,
18218:                 21,
18219:                 seen_r1(1),
18220:                 tx,
18221:                 OperationKind::CreateStaff(CreateStaffOp { staff: staff_value }),
18222:             ),
18223:             undo_env(1, 3, 30, seen_r1(2), tx, UndoPolicy::StrictInverse),
18224:         ]);
18225:         let out = reduce_operation_set_onto(&set, &base);
18226:
18227:         // Bound BEFORE assertion 2 — see the harness note above.
18228:         let members: Option<Vec<StaffId>> = out
18229:             .score
18230:             .staff_groups
18231:             .iter()
18232:             .find(|g| g.id == group_id)
18233:             .map(|g| g.members.clone());
18234:         let agreement = epiphany_core::check_invariant(
18235:             &out.score,
18236:             epiphany_core::GraphInvariant::StaffGroupMembershipAgreement,
18237:         );
18238:         let staff_present = out.score.staves.iter().any(|s| s.id == staff_id);
18239:         let all_violations = epiphany_core::check_invariants(&out.score);
18240:         let harness = format!(
18241:             "\n  post-undo g.members:            {members:?}\
18242:              \n  invariant-21 violations:        {agreement:?}\
18243:              \n  staff {staff_id:?} still in graph: {staff_present}\
18244:              \n  ALL violations:                 {all_violations:?}"
18245:         );
18246:
18247:         // 1. The group must still be live, or there is no projection left to be
18248:         //    wrong and this test asserts nothing.
18249:         assert!(
18250:             out.score.staff_groups.iter().any(|g| g.id == group_id),
18251:             "the group must survive the staff's undo for this test to mean \
18252:              anything{harness}"
18253:         );
18254:         // 1b. Not in pin 5a's list, added so a BLOCKED undo cannot be mistaken
18255:         //     for a failed strip: both leave `s` in `members`, and only this
18256:         //     tells them apart.
18257:         assert!(
18258:             !staff_present,
18259:             "precondition: the staff's undo must actually have removed it from \
18260:              the graph — if it was blocked instead, assertion 2 below would fail \
18261:              for an unrelated reason{harness}"
18262:         );
18263:         // 2. The strip itself.
18264:         assert!(
18265:             !members.as_deref().unwrap_or_default().contains(&staff_id),
18266:             "P13-S16 pin 5: undoing the staff must strip {staff_id:?} from the \
18267:              live group's members{harness}"
18268:         );
18269:         // 3. Pin 5a's third assertion: no invariant-21 residue in either
18270:         //    direction.
18271:         assert!(
18272:             agreement.is_empty(),
18273:             "the post-undo graph must leave invariant 21 clean in both \
18274:              directions{harness}"
18275:         );
18276:         // 4. Added during execution, because assertion 3 CANNOT see the residue
18277:         //    this test exists to catch — observed, not reasoned. Under M5 the
18278:         //    strip is gone and `members` keeps an id whose staff has left the
18279:         //    graph: that is a **dangling** reference, and invariant 21
18280:         //    deliberately abstains on those (agreement is a claim about live
18281:         //    pairs; dangling resolution belongs to the referential invariants).
18282:         //    M5 was run and invariant 21 reported `[]`, while invariant 10
18283:         //    `CrossCuttingRefsResolve` reported "staff group ... member staff
18284:         //    ... is not declared". Pin 5a expects assertion 3 to fail "on a
18285:         //    residue whichever direction it leaves"; on its own it does not, so
18286:         //    the whole set is asserted here.
18287:         assert!(
18288:             all_violations.is_empty(),
18289:             "the post-undo graph must be invariant-clean overall — a stripped \
18290:              member must not be left dangling either{harness}"
18291:         );
18292:     }
```

Assertions 1, 2, 3 are pin 5a's; 1b and 4 were added during execution and are so
labelled in source. All five carry `{harness}`. Verdict: ok.

## Gate 14 / pin 7a — `t8b`
```rust
16440:     /// (t8b) **P13-S16 pin 7 inverted this test.** The projection is
16441:     /// **maintained** in the missing order, and the spurious form is
16442:     /// **refused** — the same two authoring orders as before (§0.5), with the
16443:     /// verdicts the ratified disposition A requires.
16444:     ///
16445:     /// **It previously pinned the exact opposite**, as
16446:     /// `t8b_both_permitted_stale_forms_hold`: under disposition B both stale
16447:     /// forms were *permitted states*, so the missing form left `g.members == []`
16448:     /// and the spurious form let `g.members == [s]` reach the graph alongside
16449:     /// `s.group == None`. **That is not a regression being fixed here** — it was
16450:     /// the correct assertion under the ruling in force at the time. P13-S16
16451:     /// ratified disposition A, under which `Staff.group` is the sole authority
16452:     /// and `StaffGroup.members` is maintained from it (pins 1 and 2), so neither
16453:     /// disagreeing state is reachable any more and the assertions invert with the
16454:     /// ruling.
16455:     ///
16456:     /// **Deleting this test is forbidden** — the *pairing* of the two orders is
16457:     /// the coverage, because each order signs a different production change.
16458:     ///
16459:     /// **Mutation 1 (M1; spurious order, where `create_staff_group` runs
16460:     /// SECOND):** remove pin 1's refusal; the group mints carrying `[s]` while
16461:     /// `s.group` is `None`.
16462:     ///
16463:     /// **Mutation 2 (M2; missing order, where `create_staff` runs SECOND):**
16464:     /// remove pin 2's append; `g.members` stays empty and invariant 21 fires.
16465:     ///
16466:     /// An earlier draft assigned these the other way round, which is impossible
16467:     /// in both directions: `create_staff_group` runs first in the missing order
16468:     /// and cannot append a staff that does not exist yet, and `create_staff`
16469:     /// runs first in the spurious order and has no later group to repair.
16470:     ///
16471:     /// **Observation harness (pin 7a).** The two orders are two reductions over
16472:     /// two *different* groups, so there are **four** observations, not three —
16473:     /// and a `#[test]` emits nothing but its assertion diagnostics, so state not
16474:     /// in those diagnostics is unobtainable. All four are bound **before any
16475:     /// assertion** and formatted into **every** assertion's message: a failing
16476:     /// test stops at its *first* failed assertion, and M1 and M2 trip
16477:     /// *different* assertions, so each one must carry the whole set. Splitting
16478:     /// the set per order would reintroduce the same gap one level down.
16479:     #[test]
16480:     fn t8b_the_projection_is_maintained_and_the_spurious_form_is_refused() {
16481:         let instrument_id = InstrumentId::new(ReplicaId(1), 1);
16482:
16483:         // Missing form: CreateStaffGroup(g, []) -> CreateStaff(s, Some(g)).
16484:         let group_id = StaffGroupId::new(ReplicaId(1), 3);
16485:         let staff_id = StaffId::new(ReplicaId(1), 5);
16486:         let create_instrument = prim_env(
16487:             1,
16488:             0,
16489:             10,
16490:             CausalContext::new(),
16491:             OperationKind::CreateInstrument(CreateInstrumentOp {
16492:                 instrument: crate::valuegen::instrument(instrument_id),
16493:             }),
16494:         );
16495:         let create_group = staff_group_env(
16496:             1,
16497:             2,
16498:             20,
16499:             CausalContext::new(),
16500:             crate::valuegen::staff_group(group_id, vec![]),
16501:         );
16502:         let mut staff = crate::valuegen::staff(staff_id, instrument_id);
16503:         staff.group = Some(group_id);
16504:         let create_staff = prim_env(
16505:             1,
16506:             4,
16507:             30,
16508:             CausalContext::new(),
16509:             OperationKind::CreateStaff(CreateStaffOp { staff }),
16510:         );
16511:         let mut set = OperationSet::new();
16512:         set.accept_all(vec![create_instrument, create_group, create_staff]);
16513:         let out =
16514:             reduce_operation_set_onto(&set, &Score::empty(IdentityContext::new(ReplicaId(1))));
16515:
16516:         // Spurious form: CreateStaff(s, None) -> CreateStaffGroup(g, [s]).
16517:         let group_id2 = StaffGroupId::new(ReplicaId(2), 3);
16518:         let staff_id2 = StaffId::new(ReplicaId(2), 5);
16519:         let create_instrument2 = prim_env(
16520:             2,
16521:             0,
16522:             10,
16523:             CausalContext::new(),
16524:             OperationKind::CreateInstrument(CreateInstrumentOp {
16525:                 instrument: crate::valuegen::instrument(instrument_id),
16526:             }),
16527:         );
16528:         let staff2 = crate::valuegen::staff(staff_id2, instrument_id);
16529:         let create_staff2 = prim_env(
16530:             2,
16531:             2,
16532:             20,
16533:             CausalContext::new(),
16534:             OperationKind::CreateStaff(CreateStaffOp { staff: staff2 }),
16535:         );
16536:         let create_group2 = staff_group_env(
16537:             2,
16538:             4,
16539:             30,
16540:             CausalContext::new(),
16541:             crate::valuegen::staff_group(group_id2, vec![staff_id2]),
16542:         );
16543:         let spurious_group_env_id = create_group2.id;
16544:         let mut set2 = OperationSet::new();
16545:         set2.accept_all(vec![create_instrument2, create_staff2, create_group2]);
16546:         let out2 =
16547:             reduce_operation_set_onto(&set2, &Score::empty(IdentityContext::new(ReplicaId(2))));
16548:
16549:         // ---------------------------------------------------------------------
16550:         // Pin 7a's four bindings. ALL taken BEFORE ANY assertion, and read
16551:         // through `find`/`map` rather than `expect` — an `expect` panic here
16552:         // would preempt the harness and emit nothing.
16553:         // ---------------------------------------------------------------------
16554:
16555:         // [1] Spurious-order effect for the `CreateStaffGroup` op. M1 turns this
16556:         //     into an applied effect.
16557:         let spurious_effect = out2
16558:             .state
16559:             .effects
16560:             .iter()
16561:             .find(|(e, _)| *e == spurious_group_env_id)
16562:             .map(|(_, eff)| eff.clone());
16563:         // [2] Spurious-order `StaffGroup.members` — `None` while pin 1 refuses
16564:         //     the mint outright. M1 makes it `Some([s])`: the spurious
16565:         //     membership that reached the graph.
16566:         let spurious_members: Option<Vec<StaffId>> = out2
16567:             .score
16568:             .staff_groups
16569:             .iter()
16570:             .find(|g| g.id == group_id2)
16571:             .map(|g| g.members.clone());
16572:         // [3] Missing-order `StaffGroup.members` — `[s]` while pin 2 maintains
16573:         //     it. M2 leaves it empty. A DIFFERENT group in a DIFFERENT
16574:         //     reduction from [2]; one shared `members` local would satisfy this
16575:         //     harness while leaving one mutation's observation absent.
16576:         let missing_members: Option<Vec<StaffId>> = out
16577:             .score
16578:             .staff_groups
16579:             .iter()
16580:             .find(|g| g.id == group_id)
16581:             .map(|g| g.members.clone());
16582:         // [4] Missing-order invariant-21 verdict — the disagreement an
16583:         //     unmaintained projection leaves. Filtered from the missing-order
16584:         //     score; from the other reduction it would be the wrong verdict
16585:         //     rather than a missing one.
16586:         let missing_agreement = epiphany_core::check_invariant(
16587:             &out.score,
16588:             epiphany_core::GraphInvariant::StaffGroupMembershipAgreement,
16589:         );
16590:
16591:         let harness = format!(
16592:             "\n  [1] spurious-order CreateStaffGroup effect: {spurious_effect:?}\
16593:              \n  [2] spurious-order g.members:               {spurious_members:?}\
16594:              \n  [3] missing-order  g.members:               {missing_members:?}\
16595:              \n  [4] missing-order  invariant-21 violations: {missing_agreement:?}"
16596:         );
16597:
16598:         // ---- Missing order: the projection is maintained. --------------------
16599:         let missing_staff_group = out
16600:             .score
16601:             .staves
16602:             .iter()
16603:             .find(|s| s.id == staff_id)
16604:             .map(|s| s.group);
16605:         assert_eq!(
16606:             missing_staff_group,
16607:             Some(Some(group_id)),
16608:             "missing order: s.group == Some(g) — the staff carries the sole \
16609:              authority{harness}"
16610:         );
16611:         assert_eq!(
16612:             missing_members.as_deref(),
16613:             Some(&[staff_id][..]),
16614:             "missing order: g.members is MAINTAINED to [s] (P13-S16 pin 2), not \
16615:              left empty as disposition B permitted{harness}"
16616:         );
16617:         assert!(
16618:             missing_agreement.is_empty(),
16619:             "missing order: a maintained projection must leave invariant 21 \
16620:              clean{harness}"
16621:         );
16622:
16623:         // ---- Spurious order: the mint is refused. ---------------------------
16624:         assert_eq!(
16625:             spurious_effect,
16626:             Some(container_not_empty()),
16627:             "spurious order: CreateStaffGroup carrying [s] must refuse \
16628:              ContainerNotEmpty (P13-S16 pin 1){harness}"
16629:         );
16630:         assert_eq!(
16631:             spurious_members, None,
16632:             "spurious order: the refused mint must leave no group in the \
16633:              graph{harness}"
16634:         );
16635:         let spurious_staff_group = out2
16636:             .score
16637:             .staves
16638:             .iter()
16639:             .find(|s| s.id == staff_id2)
16640:             .map(|s| s.group);
16641:         assert_eq!(
16642:             spurious_staff_group,
16643:             Some(None),
16644:             "spurious order: s.group stays None — CreateStaffGroup never writes \
16645:              Staff.group{harness}"
16646:         );
16647:     }
```

[1],[2] from `out2` (spurious order, `group_id2`); [3],[4] from `out` (missing
order, `group_id`). Six assertions, six carry `{harness}`. Verdict: ok.

## Item 2e — `m41`
```rust
6254:     /// (m41) **Breaks S→G**: a staff whose `group` names a group whose `members`
6255:     /// omit it — the shape a pin-2 maintenance gap produces.
6256:     ///
6257:     /// **Holds G→S**: `members` is empty, so that direction has nothing to
6258:     /// disagree about. **Holds every other invariant**, which is what the exact
6259:     /// cardinality assertion proves and what `any()` could never touch.
6260:     ///
6261:     /// **Mutation (M6a):** delete the `check_staff_names_absent_group` call from
6262:     /// `check_invariants`. This fixture then goes **unreported** — the assertion
6263:     /// prints `0` against `1` with an empty vector, which is the observation M6a
6264:     /// owes, quoted rather than inferred. `m41b` must still pass.
6265:     #[test]
6266:     fn m41_check_invariants_dispatches_invariant_21_staff_names_absent_group() {
6267:         let mut s = crate::generators::valid_score(4243);
6268:         let replica = s.identity.replica_id;
6269:         let group_id = StaffGroupId::new(replica, 21_001);
6270:         s.staff_groups.push(StaffGroup {
6271:             id: group_id,
6272:             name: None,
6273:             kind: StaffGroupKind::Bracket,
6274:             members: Vec::new(),
6275:         });
6276:         let staff_id = s.staves[0].id;
6277:         s.staves[0].group = Some(group_id);
6278:
6279:         // Bound before ANY assertion: the opposite-direction fact this fixture
6280:         // depends on, then the violations. Nothing is asserted until the
6281:         // cardinality check, which is the one M6a trips.
6282:         let group_members: Option<Vec<StaffId>> = s
6283:             .staff_groups
6284:             .iter()
6285:             .find(|group| group.id == group_id)
6286:             .map(|group| group.members.clone());
6287:         let violations = check_invariants(&s);
6288:
6289:         assert_eq!(
6290:             violations.len(),
6291:             1,
6292:             "expected exactly the invariant-21 S->G violation and nothing else, \
6293:              got {violations:?}"
6294:         );
6295:         assert_eq!(
6296:             violations[0].invariant,
6297:             GraphInvariant::StaffGroupMembershipAgreement,
6298:             "the single violation must be invariant 21, got {violations:?}"
6299:         );
6300:         assert!(
6301:             violations[0].witness.starts_with("S->G:"),
6302:             "the witness must name the S->G direction so this test cannot pass \
6303:              on m41b's fixture, got {violations:?}"
6304:         );
6305:         assert!(
6306:             violations[0].witness.contains(&format!("{staff_id:?}"))
6307:                 && violations[0].witness.contains(&format!("{group_id:?}")),
6308:             "the witness must name both the staff and the group id, got \
6309:              {violations:?}"
6310:         );
6311:         // The G->S direction holds, asserted directly rather than left to follow
6312:         // from the cardinality above: the group must genuinely list nobody, and
6313:         // no G->S witness may be present.
6314:         assert_eq!(
6315:             group_members.as_deref(),
6316:             Some(&[][..]),
6317:             "fixture: the group must list nobody, so only S->G disagrees"
6318:         );
6319:         assert!(
6320:             !violations.iter().any(|v| v.witness.starts_with("G->S:")),
6321:             "the G->S direction must hold on this fixture, got {violations:?}"
6322:         );
6323:     }
```

## Item 2e — `m41b`
```rust
6325:     /// (m41b) **Breaks G→S**: a group listing a staff whose own `group` is not
6326:     /// that group — the shape a stale projection produces.
6327:     ///
6328:     /// **Holds S→G**: the listed staff's `group` is `None`, so it names no group
6329:     /// and that direction abstains. **Holds every other invariant** — note the
6330:     /// listed staff is genuinely declared, so this is a *disagreement*, not a
6331:     /// dangling reference (invariant 21 abstains on those; invariant 10 owns
6332:     /// them).
6333:     ///
6334:     /// **Mutation (M6b):** delete the `check_group_lists_unowned_staff` call from
6335:     /// `check_invariants`. This fixture then goes unreported, printing `0`
6336:     /// against `1`. **`m41b` is the ONLY test M6b breaks** — `m41`, the generator
6337:     /// direction test and all four `all()` consumers use S→G fixtures, so
6338:     /// without this test the G→S arm could be deleted and the suite would stay
6339:     /// green.
6340:     #[test]
6341:     fn m41b_check_invariants_dispatches_invariant_21_group_lists_unowned_staff() {
6342:         let mut s = crate::generators::valid_score(4244);
6343:         let replica = s.identity.replica_id;
6344:         let group_id = StaffGroupId::new(replica, 21_002);
6345:         let staff_id = s.staves[0].id;
6346:         s.staff_groups.push(StaffGroup {
6347:             id: group_id,
6348:             name: None,
6349:             kind: StaffGroupKind::Bracket,
6350:             members: vec![staff_id],
6351:         });
6352:
6353:         // Bound before ANY assertion: the opposite-direction fact (`valid_score`
6354:         // leaves every staff's `group` as `None`, which is what keeps S->G
6355:         // abstaining), then the violations. This was asserted *before* the
6356:         // binding in the first draft, which put a check ahead of the cardinality
6357:         // assertion M6b trips — if the precondition ever broke, M6b's evidence
6358:         // would be replaced by a fixture complaint.
6359:         let listed_staff_group = s.staves[0].group;
6360:         let violations = check_invariants(&s);
6361:
6362:         assert_eq!(
6363:             violations.len(),
6364:             1,
6365:             "expected exactly the invariant-21 G->S violation and nothing else, \
6366:              got {violations:?}"
6367:         );
6368:         assert_eq!(
6369:             violations[0].invariant,
6370:             GraphInvariant::StaffGroupMembershipAgreement,
6371:             "the single violation must be invariant 21, got {violations:?}"
6372:         );
6373:         assert!(
6374:             violations[0].witness.starts_with("G->S:"),
6375:             "the witness must name the G->S direction so this test cannot pass \
6376:              on m41's fixture, got {violations:?}"
6377:         );
6378:         assert!(
6379:             violations[0].witness.contains(&format!("{staff_id:?}"))
6380:                 && violations[0].witness.contains(&format!("{group_id:?}")),
6381:             "the witness must name both the staff and the group id, got \
6382:              {violations:?}"
6383:         );
6384:         // The S->G direction holds, asserted directly: the listed staff must name
6385:         // no group at all, and no S->G witness may be present. Without the first
6386:         // of these the fixture could silently become a both-directions one, which
6387:         // would be reported after EITHER arm was deleted and so would sign
6388:         // neither.
6389:         assert_eq!(
6390:             listed_staff_group, None,
6391:             "fixture: the listed staff must name no group, so only G->S disagrees"
6392:         );
6393:         assert!(
6394:             !violations.iter().any(|v| v.witness.starts_with("S->G:")),
6395:             "the S->G direction must hold on this fixture, got {violations:?}"
6396:         );
6397:     }
```

## Items 2d/2e — generator test, both legs
```rust
1023:     /// **P13-S16 touch row 8.** `violating_score`'s invariant-21 arm breaks the
1024:     /// **S→G** direction ONLY — in the raw fixture **and after shrinking**.
1025:     ///
1026:     /// Invariant 21's two directions carry the same `GraphInvariant`, so every
1027:     /// `all()`-driven test in this module is satisfied by either one and **none
1028:     /// can observe which**. This is the only guard on the generator's direction,
1029:     /// and `m41b` is the only permanent guard on the other direction being
1030:     /// dispatched at all.
1031:     ///
1032:     /// **The shrunk leg is not redundant.** `shrink` rebuilds the witness, and
1033:     /// nothing in its contract preserves *which way* the pair disagrees: a shrink
1034:     /// that cleared the staff's `group` while leaving it listed in `members`
1035:     /// would flip the direction, still violate invariant 21, and satisfy
1036:     /// `every_invariant_shrinks_to_a_small_witness` and `shrink_is_idempotent`
1037:     /// alike.
1038:     ///
1039:     /// **Mutation (M6a):** delete the `check_staff_names_absent_group` call from
1040:     /// `check_invariants`; both legs report nothing and the cardinality
1041:     /// assertions print `0` against `1`. Under **M6b** — deleting the G→S arm —
1042:     /// this test must **pass**, and that asymmetry is the direction claim.
1043:     #[test]
1044:     fn invariant_21_negative_generator_breaks_staff_to_group_only() {
1045:         let inv = GraphInvariant::StaffGroupMembershipAgreement;
1046:
1047:         // The two legs are built and checked SEQUENTIALLY, and the raw leg is
1048:         // fully validated before `shrink` is ever called.
1049:         //
1050:         // Building both in one array would evaluate `shrink` first — Rust
1051:         // constructs every element before the loop body runs — and `shrink`
1052:         // opens with `assert!(!check_invariant(score, inv).is_empty())`. Under
1053:         // M6a that check returns empty, so shrink PANICS on its own entry
1054:         // assertion before the raw leg's cardinality assertion executes, and M6a
1055:         // would report "shrink starting point must violate the target invariant"
1056:         // instead of the pinned `0` against `1`. **A panic upstream of the
1057:         // pinned assertion destroys the evidence the mutation owes.**
1058:         let raw = violating_score(inv, 0x2121_2121);
1059:         assert_breaks_staff_to_group_only("raw", &raw, inv);
1060:
1061:         let shrunk = shrink(&raw, inv);
1062:         assert_breaks_staff_to_group_only("shrunk", &shrunk, inv);
1063:     }
```

```rust
1065:     /// Pin 6a's three properties for one leg of
1066:     /// `invariant_21_negative_generator_breaks_staff_to_group_only`: exact
1067:     /// cardinality, the S→G witness naming both ids, and the G→S direction
1068:     /// holding — each asserted rather than implied.
1069:     fn assert_breaks_staff_to_group_only(leg: &str, s: &Score, inv: GraphInvariant) {
1070:         // Bound before any assertion, and read from the score so both legs
1071:         // survive shrinking rather than hardcoding the generator's counter. The
1072:         // ids are formatted here so this helper needs no extra id imports; the
1073:         // named group's `members` is carried as a value because the G->S claim
1074:         // must be checked against the FIXTURE, not against the checker's output.
1075:         let named = s.staves.iter().find_map(|staff| {
1076:             staff.group.map(|group| {
1077:                 let members = s
1078:                     .staff_groups
1079:                     .iter()
1080:                     .find(|candidate| candidate.id == group)
1081:                     .map(|candidate| candidate.members.clone());
1082:                 (format!("{:?}", staff.id), format!("{group:?}"), members)
1083:             })
1084:         });
1085:         let violations = check_invariants(s);
1086:
1087:         // First, because it is the assertion M6a trips.
1088:         assert_eq!(
1089:             violations.len(),
1090:             1,
1091:             "{leg}: expected exactly the invariant-21 S->G violation and nothing \
1092:              else, got {violations:?}"
1093:         );
1094:         assert_eq!(
1095:             violations[0].invariant, inv,
1096:             "{leg}: the single violation must be invariant 21, got {violations:?}"
1097:         );
1098:         assert!(
1099:             violations[0].witness.starts_with("S->G:"),
1100:             "{leg}: the generator must break the S->G direction only — a flipped \
1101:              direction still violates invariant 21 and no other test would \
1102:              notice; got {violations:?}"
1103:         );
1104:         let (staff_id, group_id, group_members) = named.unwrap_or_else(|| {
1105:             panic!("{leg}: the fixture must have a staff naming a group; got {violations:?}")
1106:         });
1107:         assert!(
1108:             violations[0].witness.contains(&staff_id) && violations[0].witness.contains(&group_id),
1109:             "{leg}: the witness must name both {staff_id} and {group_id}, got \
1110:              {violations:?}"
1111:         );
1112:         // The opposite direction holds, asserted against the FIXTURE. Checking
1113:         // only that no `G->S:` witness was emitted would pass **vacuously under
1114:         // M6b**: with the G->S arm deleted no such witness can appear whatever the
1115:         // fixture holds, so a flipped or both-directions fixture would slip
1116:         // through the leg that is supposed to guarantee the direction. The empty
1117:         // `members` is the checker-independent fact, and `m41` asserts it the
1118:         // same way.
1119:         assert_eq!(
1120:             group_members.as_deref(),
1121:             Some(&[][..]),
1122:             "{leg}: fixture — group {group_id} must list nobody, so nothing can \
1123:              disagree G->S; got {group_members:?}"
1124:         );
1125:         assert!(
1126:             !violations.iter().any(|v| v.witness.starts_with("G->S:")),
1127:             "{leg}: the G->S direction must hold, got {violations:?}"
1128:         );
1129:     }
```
