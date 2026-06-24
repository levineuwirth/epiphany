//! The equivocation harness (QUICKSTART, Agent F; v0 acceptance criterion 3):
//!
//! > An injected duplicate `OperationId` with different canonical bytes produces
//! > an `OperationSlot::Equivocated` at both replicas, regardless of which
//! > envelope arrived first. (Tests Pass 10's order-independence fix.)
//!
//! Driven against the **real** [`epiphany_ops`] crate. The harness asserts the
//! full §6.5 contract:
//!
//! 1. A byte-identical duplicate is dropped: the slot stays [`OperationSlot::Single`].
//! 2. A same-id / different-bytes twin equivocates the slot, and the resulting
//!    [`OperationSlot::Equivocated`] is **identical regardless of arrival order**.
//! 3. An equivocated operation contributes **no canonical effect**: it appears in
//!    no effect entry, and the canonical content (spellings/objects/breaks) is
//!    byte-for-byte what it would be with the operation absent. A positive
//!    control confirms the same operation *would* change that content if it were
//!    `Single`, so the no-effect assertion is not vacuous.

use epiphany_core::{
    EventId, MusicalDuration, MusicalPosition, OperationId, PitchId, RationalTime, ReplicaId,
    StaffInstanceId, VoiceId, WallClockTime,
};
use epiphany_ops::{
    valuegen, AuthorId, CausalContext, HybridLogicalClock, InsertEventOp, MaterializedState,
    OperationEnvelope, OperationKind, OperationPayload, OperationSet, OperationSlot,
    OperationStamp, RespellPitchOp,
};

use crate::rng::Rng;

/// Agent C's authoritative equivocation gate, re-exported as the suite's entry
/// point.
pub use epiphany_ops::fuzz::run_equivocation_fuzz as ops_equivocation_fuzz;

/// The shared object-id namespace (matching [`crate::generators`]).
const OBJ_REPLICA: ReplicaId = ReplicaId(0x0B7E_C700);

fn reduce(envelopes: &[OperationEnvelope]) -> MaterializedState {
    let mut set = OperationSet::new();
    set.accept_all(envelopes.iter().cloned());
    set.reduce()
}

/// Asserts the order-independence of equivocation for one `(base, twin)` pair
/// embedded in a surrounding `context`. `base` and `twin` share an `OperationId`
/// but differ in canonical bytes.
pub fn assert_equivocation_order_independent(
    context: &[OperationEnvelope],
    base: &OperationEnvelope,
    twin: &OperationEnvelope,
) {
    assert_eq!(base.id, twin.id, "base and twin must share an OperationId");
    assert_ne!(
        base.envelope_hash(),
        twin.envelope_hash(),
        "base and twin must differ in canonical bytes to equivocate"
    );

    // Arrival order A: context, then base, then twin.
    let mut a = OperationSet::new();
    a.accept_all(context.iter().cloned());
    a.accept(base.clone());
    a.accept(twin.clone());

    // Arrival order B: twin first, then base, then context.
    let mut b = OperationSet::new();
    b.accept(twin.clone());
    b.accept(base.clone());
    b.accept_all(context.iter().cloned());

    let slot_a = a.slot(base.id).expect("slot must exist");
    let slot_b = b.slot(base.id).expect("slot must exist");

    assert!(
        matches!(slot_a, OperationSlot::Equivocated { .. }),
        "order A did not equivocate: {slot_a:?}"
    );
    assert_eq!(
        slot_a, slot_b,
        "equivocated slot differs by arrival order (Pass 10 violation)"
    );
}

/// Asserts a byte-identical duplicate of `env` leaves the slot [`OperationSlot::Single`].
pub fn assert_duplicate_is_idempotent(env: &OperationEnvelope) {
    let mut s = OperationSet::new();
    s.accept(env.clone());
    s.accept(env.clone());
    s.accept(env.clone());
    assert!(
        matches!(s.slot(env.id), Some(OperationSlot::Single(_))),
        "byte-identical duplicate must not equivocate the slot"
    );
}

/// Asserts the equivocated operation contributes **no canonical effect**, in a
/// self-contained controlled scenario: an `InsertEvent` makes a pitch live, then
/// a same-id / different-bytes pair of `RespellPitch` operations target it.
///
/// * with the pair equivocated, the operation appears in no effect entry and the
///   canonical content (spellings/objects/breaks) is exactly the
///   operation-absent baseline (the recorded anomaly differs, legitimately, so
///   content fields are compared, not the whole materialized state);
/// * a **positive control** confirms the same respelling, accepted alone as
///   `Single`, *does* change the canonical spellings — so the no-effect
///   assertion is not vacuous (the reducer would otherwise no-op a respelling of
///   a pitch that was never inserted).
pub fn assert_equivocated_has_no_effect() {
    // An InsertEvent that mints event 0 carrying pitch 0 (so the pitch is live).
    let insert = insert_pitch_env(OperationId::new(ReplicaId(1), 0), 0, 0, 0);
    // The equivocating pair: same id, different spelling bytes, targeting pitch 0.
    let id = OperationId::new(ReplicaId(9), 0);
    let base = respell_env(id, 10, 0, 0xAA);
    let twin = respell_env(id, 10, 0, 0xBB);
    debug_assert_ne!(base.envelope_hash(), twin.envelope_hash());

    let absent = reduce(std::slice::from_ref(&insert));
    let with_equiv = reduce(&[insert.clone(), base.clone(), twin.clone()]);
    let single = reduce(&[insert.clone(), base.clone()]);

    assert!(
        with_equiv.effects.iter().all(|(e, _)| *e != base.id),
        "an equivocated operation must produce no canonical effect"
    );
    assert_eq!(
        with_equiv.spellings, absent.spellings,
        "equivocated operation must not change canonical spellings"
    );
    assert_eq!(
        with_equiv.objects, absent.objects,
        "equivocated operation must not change canonical object state"
    );
    assert_eq!(
        with_equiv.breaks, absent.breaks,
        "equivocated operation must not change canonical breaks"
    );
    assert_ne!(
        single.spellings, absent.spellings,
        "control: a Single respelling of a live pitch must change canonical spellings"
    );
}

fn respell_env(id: OperationId, phys: i64, pitch: u64, spelling: u8) -> OperationEnvelope {
    OperationEnvelope {
        id,
        author: AuthorId(0),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(phys), 0), id),
        causal_context: CausalContext::new(),
        transaction: None,
        payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
            pitch: PitchId::new(OBJ_REPLICA, pitch),
            spelling: valuegen::spelling(spelling),
        })),
    }
}

/// An `InsertEvent` minting `event` (carrying `pitch`) into voice 0 / instance 0,
/// so the pitch becomes live and a later `RespellPitch` can take effect.
fn insert_pitch_env(id: OperationId, phys: i64, event: u64, pitch: u64) -> OperationEnvelope {
    OperationEnvelope {
        id,
        author: AuthorId(id.replica.0 as u128),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(phys), 0), id),
        causal_context: CausalContext::new(),
        transaction: None,
        payload: OperationPayload::Primitive(OperationKind::InsertEvent(InsertEventOp {
            staff_instance: StaffInstanceId::new(OBJ_REPLICA, 0),
            event: valuegen::insert_event_value(
                EventId::new(OBJ_REPLICA, event),
                VoiceId::new(OBJ_REPLICA, 0),
                MusicalPosition(RationalTime::from_int(0)),
                MusicalDuration::whole(),
                &[PitchId::new(OBJ_REPLICA, pitch)],
            ),
        })),
    }
}

/// A self-contained driver: from `seed`, generate a context, mint a fresh target
/// operation (on a replica the context does not use), build its equivocating
/// twin, and assert the order-independence and idempotent-duplicate properties.
/// The no-effect property is asserted on its own controlled scenario.
pub fn run_equivocation(n_context: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    // Context authored by replicas 1..=3.
    let context = crate::generators::operation_envelopes(&mut rng, n_context, 3, 6, 6);
    // The target lives on replica 9 (unused by the context) so it is a genuinely
    // fresh operation id nothing causally depends on.
    let id = OperationId::new(ReplicaId(9), rng.range(0, 5));
    let base = respell_env(id, rng.range(0, 100) as i64, 0, 0xAA);
    let twin = respell_env(id, rng.range(0, 100) as i64, 0, 0xBB);

    assert_duplicate_is_idempotent(&base);
    assert_equivocation_order_independent(&context, &base, &twin);
    assert_equivocated_has_no_effect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivocation_holds_across_many_seeds() {
        for seed in 0..300u64 {
            run_equivocation(12, seed.wrapping_mul(0x9E37_79B9).wrapping_add(1));
        }
    }

    #[test]
    fn agent_c_equivocation_gate_smoke() {
        ops_equivocation_fuzz(500, 0x1234_5678);
    }
}
