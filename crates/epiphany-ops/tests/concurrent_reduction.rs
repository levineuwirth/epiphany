//! Integration tests for the Chapter 6 concurrent-reduction contract.
//!
//! These exercise, through the public API only, the v0 acceptance criteria that
//! route through `epiphany-ops`:
//!
//! * **Convergence** (criterion 1): overlapping edits from two replicas
//!   converge to byte-identical materialized state regardless of delivery
//!   order.
//! * **Equivocation** (criterion 3): an injected duplicate `OperationId` with
//!   different canonical bytes equivocates at both replicas, regardless of which
//!   arrived first.
//! * **Reduction determinism** (criterion 5): a 1,000-envelope set reduced in
//!   ten different orders produces byte-identical materialized state.
//!
//! plus the chapter's transaction-atomicity, descriptor-precedence, conflict,
//! anomaly-exclusion, and forward-undo rules.

use epiphany_core::{
    EventId, MusicalDuration, MusicalPosition, OperationId, PitchId, RationalTime, ReplicaId,
    StaffInstanceId, TransactionId, TypedObjectId, VoiceId, WallClockTime,
};
use epiphany_determinism::{fuzz::SplitMix64, ContentHash};
use epiphany_ops::{
    well_formed, AuthorId, CausalContext, ConflictKind, HybridLogicalClock, InsertEventOp,
    IntegrityAnomalyKind, NoOpReason, OperationEffect, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet, OperationStamp, RespellPitchOp, TransactionDescriptor,
    TupletCompensation, UndoPolicy, UndoTransactionPayload,
};

// --- Builders. --------------------------------------------------------------

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

fn insert(voice: u64, event: u64, pos: i64) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
        voice: VoiceId::new(ReplicaId(9), voice),
        staff_instance: StaffInstanceId::new(ReplicaId(9), 0),
        event: EventId::new(ReplicaId(9), event),
        position: MusicalPosition(RationalTime::from_int(pos as i32)),
        duration: MusicalDuration::whole(),
        pitches: vec![PitchId::new(ReplicaId(9), event)],
    }))
}

fn respell(pitch: u64, spelling: u8) -> OperationPayload {
    OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
        pitch: PitchId::new(ReplicaId(9), pitch),
        spelling: ContentHash([spelling; 32]),
    }))
}

fn reduce_in_order(envs: &[OperationEnvelope]) -> Vec<u8> {
    let mut set = OperationSet::new();
    set.accept_all(envs.iter().cloned());
    set.reduce().canonical_bytes()
}

fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

// --- Convergence (v0 criterion 1). ------------------------------------------

#[test]
fn overlapping_edits_from_two_replicas_converge() {
    // Replica 1 builds a small passage; replica 2 concurrently edits the same
    // objects (a concurrent same-position insert and a respelling).
    let envs = vec![
        envelope(1, 0, 10, CausalContext::new(), None, insert(0, 100, 0)),
        envelope(1, 1, 11, CausalContext::new(), None, insert(0, 101, 1)),
        envelope(1, 2, 12, CausalContext::new(), None, respell(100, 3)),
        // Replica 2, concurrent (no causal link to replica 1's ops).
        envelope(2, 0, 10, CausalContext::new(), None, insert(0, 200, 1)), // collides with 101 @pos 1
        envelope(2, 1, 11, CausalContext::new(), None, respell(100, 7)),   // conflicts with op(1,2)
    ];

    let reference = reduce_in_order(&envs);
    let mut rng = SplitMix64::new(0xA11CE);
    for _ in 0..8 {
        let mut perm = envs.clone();
        shuffle(&mut perm, &mut rng);
        assert_eq!(
            reduce_in_order(&perm),
            reference,
            "materialized state must be delivery-order independent"
        );
    }

    // The concurrent respelling produced a structural-field-collision conflict.
    let mut set = OperationSet::new();
    set.accept_all(envs);
    let state = set.reduce();
    assert!(
        state
            .conflicts
            .records()
            .iter()
            .any(|r| matches!(r.kind, ConflictKind::StructuralFieldCollision { .. })),
        "concurrent differing respellings must record a conflict"
    );
}

// --- Equivocation (v0 criterion 3). -----------------------------------------

#[test]
fn duplicate_id_with_different_bytes_equivocates_at_both_replicas() {
    let id_replica = 1;
    let id_counter = 4;
    let a = envelope(
        id_replica,
        id_counter,
        5,
        CausalContext::new(),
        None,
        respell(0, 1),
    );
    let b = envelope(
        id_replica,
        id_counter,
        9,
        CausalContext::new(),
        None,
        respell(0, 2),
    );
    assert_ne!(a.envelope_hash(), b.envelope_hash());

    // Replica X sees A then B; replica Y sees B then A.
    let mut x = OperationSet::new();
    x.accept(a.clone());
    x.accept(b.clone());
    let mut y = OperationSet::new();
    y.accept(b);
    y.accept(a);

    let id = op(id_replica, id_counter);
    assert!(x.slot(id).unwrap().is_equivocated());
    assert!(y.slot(id).unwrap().is_equivocated());

    // Both reduce identically, with no effect for the equivocated id and an
    // OperationSlotEquivocated anomaly.
    let sx = x.reduce();
    let sy = y.reduce();
    assert_eq!(sx.canonical_bytes(), sy.canonical_bytes());
    assert!(sx.effects.iter().all(|(e, _)| *e != id));
    assert!(sx.anomalies.iter().any(|an| matches!(
        an.kind,
        IntegrityAnomalyKind::OperationSlotEquivocated { operation_id } if operation_id == id
    )));
}

// --- Reduction determinism (v0 criterion 5). --------------------------------

#[test]
fn thousand_envelope_set_reduces_identically_in_ten_orders() {
    let mut rng = SplitMix64::new(0x5EED);
    let base = epiphany_ops::fuzz::gen_envelope_set(&mut rng, 1000);
    let reference = reduce_in_order(&base);
    for _ in 0..10 {
        let mut perm = base.clone();
        shuffle(&mut perm, &mut rng);
        assert_eq!(reduce_in_order(&perm), reference);
    }
}

// --- Transactions (Chapter 6 §6.6). -----------------------------------------

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

#[test]
fn clean_transaction_applies_atomically() {
    let tx = TransactionId::from_raw(77);
    let d = declare_tx(1, 0, 10, tx);
    // Members causally depend on the descriptor (with_seen covers counter 0).
    let ctx = CausalContext::new().with_seen(ReplicaId(1), 0);
    let m1 = envelope(1, 1, 11, ctx.clone(), Some(tx), insert(0, 100, 0));
    let m2 = envelope(1, 2, 12, ctx, Some(tx), insert(0, 101, 1));

    let mut set = OperationSet::new();
    set.accept_all(vec![d, m1.clone(), m2.clone()]);
    let state = set.reduce();

    // Both members applied; no transaction conflict.
    for id in [m1.id, m2.id] {
        let eff = state.effects.iter().find(|(e, _)| *e == id).map(|(_, e)| e);
        assert_eq!(eff, Some(&OperationEffect::Applied), "member {id:?}");
    }
    assert!(state.conflicts.is_empty());
}

#[test]
fn transaction_with_a_failing_member_conflicts_wholesale() {
    let tx = TransactionId::from_raw(88);
    let d = declare_tx(1, 0, 10, tx);
    let ctx = CausalContext::new().with_seen(ReplicaId(1), 0);
    let m1 = envelope(1, 1, 11, ctx.clone(), Some(tx), insert(0, 100, 0));
    // m2 deletes an event that does not exist → invariant precondition fails.
    let m2 = envelope(
        1,
        2,
        12,
        ctx,
        Some(tx),
        OperationPayload::Primitive(OperationKind::DeleteEvent(epiphany_ops::DeleteEventOp {
            event: EventId::new(ReplicaId(9), 999),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        })),
    );

    let mut set = OperationSet::new();
    set.accept_all(vec![d, m1.clone(), m2.clone()]);
    let state = set.reduce();

    // No member applied; both reduce to TransactionConflict; a conflict record exists.
    for id in [m1.id, m2.id] {
        let eff = state.effects.iter().find(|(e, _)| *e == id).map(|(_, e)| e);
        assert_eq!(
            eff,
            Some(&OperationEffect::NoOp {
                reason: NoOpReason::TransactionConflict
            }),
            "member {id:?} must not be independently visible"
        );
    }
    // The would-be-inserted event of m1 must NOT be live (atomic rollback).
    assert!(!state
        .objects
        .contains_key(&TypedObjectId::Event(EventId::new(ReplicaId(9), 100))));
    assert!(state
        .conflicts
        .records()
        .iter()
        .any(|r| matches!(r.kind, ConflictKind::TransactionConflict { .. })));
}

#[test]
fn member_without_its_descriptor_is_a_transaction_conflict() {
    // A member that declares membership in a transaction whose descriptor is
    // absent from the set is malformed against the transaction model.
    let tx = TransactionId::from_raw(99);
    let ctx = CausalContext::new().with_seen(ReplicaId(1), 0);
    let orphan = envelope(1, 1, 11, ctx, Some(tx), insert(0, 100, 0));

    let mut set = OperationSet::new();
    set.accept(orphan.clone());
    let state = set.reduce();
    let eff = state
        .effects
        .iter()
        .find(|(e, _)| *e == orphan.id)
        .map(|(_, e)| e);
    assert_eq!(
        eff,
        Some(&OperationEffect::NoOp {
            reason: NoOpReason::TransactionConflict
        })
    );
}

// --- Anomaly exclusion (Chapter 6 §6.6). ------------------------------------

#[test]
fn hlc_monotonicity_violation_excludes_the_segment() {
    // Two ops from replica 1: counter 0 with a high stamp, counter 1 with a low
    // stamp — a monotonicity violation. Both are excluded from reduction.
    let a = envelope(1, 0, 1000, CausalContext::new(), None, insert(0, 100, 0));
    let b = envelope(1, 1, 1, CausalContext::new(), None, insert(0, 101, 1));
    let mut set = OperationSet::new();
    set.accept_all(vec![a.clone(), b.clone()]);
    let state = set.reduce();

    assert!(
        state.effects.is_empty(),
        "every envelope from the offending replica at/after first_bad_counter is excluded"
    );
    assert!(state.anomalies.iter().any(|an| matches!(
        an.kind,
        IntegrityAnomalyKind::ReplicaStreamQuarantined { replica, .. } if replica == ReplicaId(1)
    )));
}

// --- Forward undo (Chapter 6 §6.8). -----------------------------------------

#[test]
fn undo_transaction_tombstones_minted_objects() {
    let tx = TransactionId::from_raw(5);
    let d = declare_tx(1, 0, 10, tx);
    let ctx = CausalContext::new().with_seen(ReplicaId(1), 0);
    let m1 = envelope(1, 1, 11, ctx, Some(tx), insert(0, 100, 0));
    // Undo op causally after the transaction.
    let undo_ctx = CausalContext::new().with_seen(ReplicaId(1), 1);
    let undo = envelope(
        1,
        2,
        20,
        undo_ctx,
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::StrictInverse,
        }),
    );

    let mut set = OperationSet::new();
    set.accept_all(vec![d, m1, undo]);
    let state = set.reduce();

    // The event minted by the transaction is tombstoned by the forward undo.
    let ev = TypedObjectId::Event(EventId::new(ReplicaId(9), 100));
    assert!(matches!(
        state.objects.get(&ev),
        Some(epiphany_ops::ObjectState::Tombstoned { .. })
    ));
}

// --- Well-formedness is a precondition for everything above. ----------------

#[test]
fn malformed_envelopes_never_enter_the_set() {
    let mut set = OperationSet::new();
    // stamp.id != id.
    let mut e = envelope(1, 1, 5, CausalContext::new(), None, respell(0, 1));
    e.stamp.id = op(1, 2);
    assert!(well_formed(&e).is_err());
    set.accept(e);
    assert!(set.is_empty());
}
