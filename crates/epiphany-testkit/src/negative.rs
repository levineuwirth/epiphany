//! Negative / regression guards for the defects found in the Agent C audit
//! (the M1 framework fixes). Item 4 of the v0 follow-up plan charters Agent F's
//! suite to independently guard *every* defect the audit surfaced, so a
//! regression in `epiphany-ops` trips this tripwire rather than slipping past a
//! generic convergence gate.
//!
//! Each guard drives the **real** [`epiphany_ops`] public API and asserts the
//! post-fix behavior. Where the pre-fix bug had a concrete, observable
//! signature (an inverted order, an under-cut quarantine, an un-promoted
//! collision), the guard also pins the *negative* outcome — the thing the buggy
//! reducer would have produced — so the test is provably non-vacuous.
//!
//! The six audited defects:
//!
//! 1. A causal predecessor authored with a larger HLC than its successor sorted
//!    *after* it ([`assert_causal_order_dominates_inverted_hlc`]).
//! 2. A missing predecessor expressed through the DVV *vector* (not a dot) was
//!    not detected ([`assert_missing_vector_predecessor_pends`]).
//! 3. An HLC sequence `100, 200, 50` quarantined from counter 1, not the
//!    counter 0 that also forms a violating pair
//!    ([`assert_hlc_100_200_50_quarantines_from_zero`]).
//! 4. A failed transaction left its members' generated conflicts behind
//!    ([`assert_failed_transaction_rolls_back_member_conflicts`]).
//! 5. A *causally ordered* same-position insert was promoted as if concurrent
//!    ([`assert_causally_ordered_same_position_not_promoted`]).
//! 6. Partial-duration interval overlaps were mishandled
//!    ([`assert_partial_overlap_promotes_but_adjacent_does_not`]).

use epiphany_core::{
    EventId, MusicalDuration, MusicalPosition, OperationId, PitchId, RationalTime, ReplicaId,
    StaffInstanceId, TransactionId, VoiceId, WallClockTime,
};
use epiphany_determinism::ContentHash;
use epiphany_ops::{
    canonical_reduction_order, AuthorId, CausalContext, ConflictKind, DeleteEventOp,
    HybridLogicalClock, InsertEventOp, IntegrityAnomalyKind, NoOpReason, OperationEffect,
    OperationEnvelope, OperationKind, OperationPayload, OperationSet, OperationStamp,
    PendingReason, PreconditionFailureReason, RepairKind, RespellPitchOp, TransactionDescriptor,
    TupletCompensation,
};

/// The replica that owns the synthetic object id space these scenarios edit.
const OBJ: ReplicaId = ReplicaId(9);

fn op(replica: u64, counter: u64) -> OperationId {
    OperationId::new(ReplicaId(replica), counter)
}

fn envelope(
    replica: u64,
    counter: u64,
    physical: i64,
    ctx: CausalContext,
    transaction: Option<TransactionId>,
    payload: OperationPayload,
) -> OperationEnvelope {
    let id = op(replica, counter);
    OperationEnvelope {
        id,
        author: AuthorId(replica as u128),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(physical), 0), id),
        causal_context: ctx,
        transaction,
        payload,
    }
}

fn insert_span(
    voice: u64,
    event: u64,
    position: RationalTime,
    duration: RationalTime,
) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
        voice: VoiceId::new(OBJ, voice),
        staff_instance: StaffInstanceId::new(OBJ, 0),
        event: EventId::new(OBJ, event),
        position: MusicalPosition(position),
        duration: MusicalDuration(duration),
        pitches: vec![PitchId::new(OBJ, event)],
    }))
}

fn insert(voice: u64, event: u64, pos: i64) -> OperationPayload {
    insert_span(
        voice,
        event,
        RationalTime::from_int(pos as i32),
        RationalTime::one(),
    )
}

fn respell(pitch: u64, spelling: u8) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
        pitch: PitchId::new(OBJ, pitch),
        spelling: ContentHash([spelling; 32]),
    }))
}

fn delete_event(event: u64) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::DeleteEvent(DeleteEventOp {
        event: EventId::new(OBJ, event),
        tuplet_compensation: TupletCompensation::NotInTuplet,
    }))
}

fn declare_tx(replica: u64, counter: u64, physical: i64, tx: TransactionId) -> OperationEnvelope {
    envelope(
        replica,
        counter,
        physical,
        CausalContext::new(),
        None,
        OperationPayload::Primitive(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: "edit".to_string(),
            category: None,
        })),
    )
}

fn promotes(effect: Option<&OperationEffect>) -> bool {
    matches!(
        effect,
        Some(OperationEffect::AppliedWithRepair { repairs })
            if repairs.iter().any(|r| matches!(r.kind, RepairKind::VoicePromoted { .. }))
    )
}

// --- Defect 1: causal order vs. an inverted HLC. ----------------------------

/// A predecessor carrying a *larger* physical stamp than its dot-linked
/// successor must still reduce first. A reducer that trusted the HLC tuple alone
/// (the pre-fix bug) would invert the pair; the canonical order must not.
pub fn assert_causal_order_dominates_inverted_hlc() {
    let predecessor = envelope(1, 0, 100, CausalContext::new(), None, insert(0, 100, 0));
    let successor = envelope(
        2,
        0,
        1,
        CausalContext::new().with_dot(predecessor.id),
        None,
        insert(0, 101, 1),
    );

    // Negative control: sorting by the HLC tuple alone *does* invert these.
    let mut by_hlc = [&successor, &predecessor];
    by_hlc.sort_by_key(|e| e.stamp.reduction_tuple());
    assert_eq!(
        by_hlc[0].id, successor.id,
        "negative control mis-constructed: HLC-only order should place the successor first"
    );

    let ordered = canonical_reduction_order(&[&successor, &predecessor]);
    assert_eq!(
        ordered.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![predecessor.id, successor.id],
        "canonical reduction order must place the causal predecessor first despite its larger HLC"
    );
}

// --- Defect 2: missing predecessor via the DVV vector. ----------------------

/// A dependent whose causal context covers a predecessor through the contiguous
/// *vector* (`with_seen`), not a dot, must be held pending when that predecessor
/// is absent. The pre-fix detector keyed only on dots and let this through.
pub fn assert_missing_vector_predecessor_pends() {
    let present = envelope(1, 0, 1, CausalContext::new(), None, insert(0, 99, -1));
    let dependent = envelope(
        2,
        0,
        10,
        CausalContext::new().with_seen(ReplicaId(1), 2),
        None,
        insert(0, 100, 0),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![present.clone(), dependent.clone()]);
    let state = set.reduce();

    assert_eq!(state.effects.len(), 1, "only the present operation applies");
    assert_eq!(state.effects[0].0, present.id);
    assert_eq!(
        state.pending,
        vec![(
            dependent.id,
            PendingReason::MissingCausalPredecessor { missing: op(1, 1) }
        )],
        "the operation must pend on the vector-covered but absent predecessor (1,1)"
    );
}

// --- Defect 3: HLC 100, 200, 50 quarantines from counter 0. -----------------

/// The cut must begin at the *earliest* counter participating in any violating
/// pair. With stamps `100, 200, 50` the violating pair is (counter 0 = 100,
/// counter 2 = 50), so even the monotone counter 0 is quarantined. The pre-fix
/// code cut from counter 1 (the first strict decrease) and left counter 0 live.
pub fn assert_hlc_100_200_50_quarantines_from_zero() {
    let a = envelope(1, 0, 100, CausalContext::new(), None, insert(0, 100, 0));
    let b = envelope(1, 1, 200, CausalContext::new(), None, insert(0, 101, 1));
    let c = envelope(1, 2, 50, CausalContext::new(), None, insert(0, 102, 2));
    let mut set = OperationSet::new();
    set.accept_all(vec![a, b, c]);
    let state = set.reduce();

    assert!(
        state.effects.is_empty(),
        "the entire offending replica stream must be excluded, including counter 0"
    );
    assert!(
        state.anomalies.iter().any(|an| matches!(
            an.kind,
            IntegrityAnomalyKind::ReplicaStreamQuarantined {
                replica,
                first_bad_counter,
                ..
            } if replica == ReplicaId(1) && first_bad_counter == 0
        )),
        "the quarantine anomaly must report first_bad_counter == 0"
    );
}

// --- Defect 4: failed transaction rolls back member-generated conflicts. -----

/// A transaction whose members would generate conflicts but whose block fails
/// must leave *no* member-generated conflict behind — only the wholesale
/// transaction conflict. The pre-fix snapshot did not restore the conflict
/// registry, so the member's structural-collision conflict survived.
pub fn assert_failed_transaction_rolls_back_member_conflicts() {
    let seed = envelope(2, 0, 1, CausalContext::new(), None, insert(0, 100, 0));
    let initial_spelling = envelope(
        2,
        1,
        2,
        CausalContext::new().with_seen(ReplicaId(2), 0),
        None,
        respell(100, 1),
    );
    let tx = TransactionId::from_raw(89);
    let descriptor = declare_tx(1, 0, 10, tx);
    let tx_ctx = CausalContext::new().with_seen(ReplicaId(1), 0);
    let conflicting = envelope(1, 1, 11, tx_ctx.clone(), Some(tx), respell(100, 2));
    let failing = envelope(1, 2, 12, tx_ctx, Some(tx), delete_event(999));

    let mut set = OperationSet::new();
    set.accept_all(vec![
        seed,
        initial_spelling,
        descriptor,
        conflicting,
        failing,
    ]);
    let state = set.reduce();

    assert_eq!(
        state.spellings.get(&PitchId::new(OBJ, 100)),
        Some(&ContentHash([1; 32])),
        "the pre-transaction spelling must survive the rollback"
    );
    assert_eq!(
        state.conflicts.records().len(),
        1,
        "only the wholesale transaction conflict should remain"
    );
    assert!(matches!(
        state.conflicts.records()[0].kind,
        ConflictKind::TransactionConflict { .. }
    ));
}

// --- Defect 5: causally ordered same-position insert is not promoted. --------

/// A same-position insert that *causally follows* the first (so it is not
/// concurrent) must be a precondition failure, never a voice promotion.
/// Promotion is for concurrent collisions only.
pub fn assert_causally_ordered_same_position_not_promoted() {
    let first = envelope(1, 0, 10, CausalContext::new(), None, insert(0, 100, 0));
    let second = envelope(
        1,
        1,
        11,
        CausalContext::new().with_seen(ReplicaId(1), 0),
        None,
        insert(0, 101, 0),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![first, second.clone()]);
    let state = set.reduce();

    let effect = state
        .effects
        .iter()
        .find(|(id, _)| *id == second.id)
        .map(|(_, e)| e);
    assert_eq!(
        effect,
        Some(&OperationEffect::NoOp {
            reason: NoOpReason::PreconditionFailedUnderReduction {
                reason: PreconditionFailureReason::EventDurationInvalid,
            },
        }),
        "a causally-ordered same-position insert must fail its precondition"
    );
    assert!(
        !promotes(effect),
        "a causally-ordered insert must not be promoted as if concurrent"
    );
}

// --- Defect 6: partial-duration overlaps. -----------------------------------

/// Concurrent inserts whose intervals *partially* overlap collide (and promote
/// the greater id), while concurrent inserts at *adjacent* half-open intervals
/// do not. The pre-fix overlap test only compared start positions.
pub fn assert_partial_overlap_promotes_but_adjacent_does_not() {
    // Partial overlap: [0, 1) and [1/2, 3/2).
    let first = envelope(
        1,
        0,
        10,
        CausalContext::new(),
        None,
        insert_span(0, 100, RationalTime::zero(), RationalTime::one()),
    );
    let second = envelope(
        2,
        0,
        10,
        CausalContext::new(),
        None,
        insert_span(
            0,
            200,
            RationalTime::new(1, 2).unwrap(),
            RationalTime::one(),
        ),
    );
    let mut set = OperationSet::new();
    set.accept_all(vec![first, second.clone()]);
    let state = set.reduce();
    assert!(
        promotes(
            state
                .effects
                .iter()
                .find(|(id, _)| *id == second.id)
                .map(|(_, e)| e)
        ),
        "a concurrent partial-interval overlap must promote the greater id"
    );

    // Adjacent half-open intervals: [0, 1) and [1, 2) — no collision.
    let a = envelope(
        1,
        0,
        10,
        CausalContext::new(),
        None,
        insert_span(0, 100, RationalTime::zero(), RationalTime::one()),
    );
    let b = envelope(
        2,
        0,
        10,
        CausalContext::new(),
        None,
        insert_span(0, 200, RationalTime::one(), RationalTime::one()),
    );
    let mut adjacent = OperationSet::new();
    adjacent.accept_all(vec![a, b]);
    let adjacent_state = adjacent.reduce();
    assert!(
        adjacent_state
            .effects
            .iter()
            .all(|(_, e)| *e == OperationEffect::Applied),
        "adjacent half-open intervals must not collide"
    );
}

/// Runs every audited-defect regression guard. The acceptance suite calls this
/// as a single entry point (criterion-adjacent: the audit tripwire).
pub fn run_all() {
    assert_causal_order_dominates_inverted_hlc();
    assert_missing_vector_predecessor_pends();
    assert_hlc_100_200_50_quarantines_from_zero();
    assert_failed_transaction_rolls_back_member_conflicts();
    assert_causally_ordered_same_position_not_promoted();
    assert_partial_overlap_promotes_but_adjacent_does_not();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causal_order_dominates_inverted_hlc() {
        assert_causal_order_dominates_inverted_hlc();
    }

    #[test]
    fn missing_vector_predecessor_pends() {
        assert_missing_vector_predecessor_pends();
    }

    #[test]
    fn hlc_100_200_50_quarantines_from_zero() {
        assert_hlc_100_200_50_quarantines_from_zero();
    }

    #[test]
    fn failed_transaction_rolls_back_member_conflicts() {
        assert_failed_transaction_rolls_back_member_conflicts();
    }

    #[test]
    fn causally_ordered_same_position_not_promoted() {
        assert_causally_ordered_same_position_not_promoted();
    }

    #[test]
    fn partial_overlap_promotes_but_adjacent_does_not() {
        assert_partial_overlap_promotes_but_adjacent_does_not();
    }
}
