//! Deterministic fuzz harnesses for the reduction (the Agent C hand-off gates).
//!
//! The QUICKSTART gates this crate on two properties:
//!
//! 1. **Reduction determinism** — "the determinism property holds across 10,000
//!    randomized envelope sets": any permutation of an operation set reduces to
//!    *byte-identical* materialized state ([`run_reduction_determinism_fuzz`]).
//!    This is v0 acceptance criteria 1 (convergence) and 5 (reduction
//!    determinism), exercised at scale.
//! 2. **Equivocation order-independence** — "the equivocation harness produces
//!    `OperationSlot::Equivocated` for any duplicate-id-with-different-bytes
//!    scenario regardless of arrival order" ([`run_equivocation_fuzz`]). This is
//!    v0 acceptance criterion 3 (Pass 10's order-independence fix).
//!
//! Both harnesses are themselves deterministic: they draw from a seeded
//! SplitMix64 (the determinism crate's, reused — no `rand`, no platform
//! entropy), so a failing iteration reproduces exactly from its seed. The
//! generated sets deliberately reuse a small object-id space so deletes,
//! respellings, and inserts interact (tombstones, already-applied, conflicts),
//! and they occasionally inject equivocation and HLC-monotonicity anomalies so
//! those paths are exercised for permutation-invariance too.

use epiphany_core::{
    EventId, MusicalDuration, MusicalPosition, OperationId, PitchId, RationalTime, RegionId,
    RepeatStructureId, ReplicaId, SlurId, StaffId, StaffInstanceId, TranspositionInterval,
    TypedObjectId, VoiceId,
};
use epiphany_determinism::fuzz::SplitMix64;

use crate::causal::CausalContext;
use crate::envelope::OperationEnvelope;
use crate::opset::OperationSet;
use crate::payload::OperationKindTag;
use crate::payload::{
    CreateCrossCuttingOp, CreateRegionOp, CreateRepeatStructureOp, CreateStaffInstanceOp,
    CreateStaffOp, CreateVoiceOp, CrossCuttingValue, DeleteCrossCuttingOp, DeleteEventOp,
    DeleteIdentifiedPitchOp, DeleteRegionOp, DeleteRepeatStructureOp, DeleteStaffInstanceOp,
    DeleteVoiceOp, InsertEventOp, InsertIdentifiedPitchOp, ModifyCrossCuttingOp, ModifyEventOp,
    ModifyIdentifiedPitchOp, OperationKind, OperationPayload, RespellPitchOp, SetMetadataOp,
    SetMetricGridOp, SetStaffLayoutOp, SetTempoSegmentOp, SetTimeSignatureOp, SetUserPageBreakOp,
    SetUserSystemBreakOp, TransposeIntervalOp, TransposeOp, TupletCompensation,
};
use crate::stamp::{HybridLogicalClock, OperationStamp};
use crate::support::AuthorId;
use crate::valuegen;
use crate::{EnvelopeHash, IntegrityAnomalyKind, OperationEffect};
use epiphany_determinism::{CanonicalDecode, CanonicalEncode};

/// Number of replicas the generator draws authors from.
const REPLICAS: u64 = 3;
/// Size of the shared object-id space (events, pitches), so operations
/// genuinely interact during reduction.
const ID_SPACE: u64 = 6;

/// A tiny extension trait for readable bounded draws.
trait Draw {
    fn below(&mut self, n: u64) -> u64;
    fn chance(&mut self, one_in: u64) -> bool;
}
impl Draw for SplitMix64 {
    #[inline]
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
    #[inline]
    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in.max(1)) == 0
    }
}

/// In-place Fisher–Yates shuffle driven by the seeded generator.
fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

fn event(n: u64) -> EventId {
    EventId::new(ReplicaId(7), n)
}
fn pitch(n: u64) -> PitchId {
    PitchId::new(ReplicaId(7), n)
}

/// Generates a random payload over the shared id space.
fn gen_payload(rng: &mut SplitMix64) -> OperationPayload {
    let kind = match rng.below(28) {
        0 => {
            let voice = VoiceId::new(ReplicaId(7), rng.below(3));
            let position = MusicalPosition(RationalTime::from_int(rng.below(4) as i32));
            let pitches = if rng.chance(2) {
                vec![pitch(rng.below(ID_SPACE))]
            } else {
                vec![]
            };
            OperationKind::InsertEvent(InsertEventOp {
                staff_instance: StaffInstanceId::new(ReplicaId(7), 0),
                event: valuegen::insert_event_value(
                    event(rng.below(ID_SPACE)),
                    voice,
                    position,
                    MusicalDuration::whole(),
                    &pitches,
                ),
            })
        }
        1 => OperationKind::DeleteEvent(DeleteEventOp {
            event: event(rng.below(ID_SPACE)),
            tuplet_compensation: TupletCompensation::NotInTuplet,
        }),
        2 => OperationKind::RespellPitch(RespellPitchOp {
            pitch: pitch(rng.below(ID_SPACE)),
            spelling: valuegen::spelling(rng.below(4) as u8 + 1),
        }),
        3 => OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: CrossCuttingValue::Slur(valuegen::slur(
                SlurId::new(ReplicaId(7), rng.below(ID_SPACE)),
                event(rng.below(ID_SPACE)),
                event(rng.below(ID_SPACE)),
            )),
        }),
        4 => OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
            region: RegionId::new(ReplicaId(7), 0),
            anchor: valuegen::region_start_anchor(
                RegionId::new(ReplicaId(7), 0),
                MusicalPosition(RationalTime::from_int(rng.below(4) as i32)),
            ),
            present: rng.chance(2),
        }),
        // Group 1 (M2): event & pitch leaf-field ops over the shared id space.
        5 => OperationKind::ModifyEvent(ModifyEventOp {
            event: valuegen::insert_event_value(
                event(rng.below(ID_SPACE)),
                VoiceId::new(ReplicaId(7), rng.below(3)),
                MusicalPosition(RationalTime::from_int(rng.below(4) as i32)),
                MusicalDuration::whole(),
                &[pitch(rng.below(ID_SPACE))],
            ),
        }),
        6 => OperationKind::Transpose(TransposeOp {
            targets: vec![pitch(rng.below(ID_SPACE))],
            chromatic_steps: rng.below(5) as i32 - 2,
        }),
        7 => OperationKind::InsertIdentifiedPitch(InsertIdentifiedPitchOp {
            event: event(rng.below(ID_SPACE)),
            pitch: valuegen::identified_pitch(pitch(rng.below(ID_SPACE))),
        }),
        8 => OperationKind::DeleteIdentifiedPitch(DeleteIdentifiedPitchOp {
            pitch: pitch(rng.below(ID_SPACE)),
        }),
        9 => OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
            pitch: pitch(rng.below(ID_SPACE)),
            value: valuegen::pitch_value_nth(rng.below(4) as u8 + 1),
        }),
        // Group 2 (M2): cross-cutting CRUD over the shared id space.
        10 => OperationKind::DeleteCrossCutting(DeleteCrossCuttingOp {
            structure: TypedObjectId::Slur(SlurId::new(ReplicaId(7), rng.below(ID_SPACE))),
        }),
        11 => OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
            structure: CrossCuttingValue::Slur(valuegen::slur(
                SlurId::new(ReplicaId(7), rng.below(ID_SPACE)),
                event(rng.below(ID_SPACE)),
                event(rng.below(ID_SPACE)),
            )),
        }),
        // Group 3 (M2c): structural container CRUD over the shared id space.
        12 => OperationKind::CreateRegion(CreateRegionOp {
            region: valuegen::region(RegionId::new(ReplicaId(7), rng.below(3))),
        }),
        13 => OperationKind::DeleteRegion(DeleteRegionOp {
            region: RegionId::new(ReplicaId(7), rng.below(3)),
        }),
        14 => OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: RegionId::new(ReplicaId(7), rng.below(3)),
            instance: valuegen::staff_instance(
                StaffInstanceId::new(ReplicaId(7), rng.below(3)),
                StaffId::new(ReplicaId(7), 0),
            ),
        }),
        15 => OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
            staff_instance: StaffInstanceId::new(ReplicaId(7), rng.below(3)),
        }),
        16 => OperationKind::CreateVoice(CreateVoiceOp {
            staff_instance: StaffInstanceId::new(ReplicaId(7), rng.below(3)),
            voice: valuegen::voice(VoiceId::new(ReplicaId(7), rng.below(3))),
        }),
        17 => OperationKind::DeleteVoice(DeleteVoiceOp {
            voice: VoiceId::new(ReplicaId(7), rng.below(3)),
        }),
        // Group 4 (M2d): score settings over the shared id space.
        18 => OperationKind::SetMetadata(SetMetadataOp {
            metadata: valuegen::score_metadata(rng.below(3) as u8),
        }),
        19 => OperationKind::SetMetricGrid(SetMetricGridOp {
            region: RegionId::new(ReplicaId(7), rng.below(3)),
            grid: rng.chance(2).then(valuegen::metric_grid),
        }),
        20 => OperationKind::SetUserPageBreak(SetUserPageBreakOp {
            region: RegionId::new(ReplicaId(7), 0),
            anchor: valuegen::region_start_anchor(
                RegionId::new(ReplicaId(7), 0),
                MusicalPosition(RationalTime::from_int(rng.below(4) as i32)),
            ),
            present: rng.chance(2),
        }),
        // Phase-3 first tranche: staff mint, meter/tempo overwrites, layout
        // advisory — over the same shared id space so mints, re-carries,
        // overwrites, and removals genuinely interact.
        21 => OperationKind::CreateStaff(CreateStaffOp {
            staff: valuegen::staff(
                StaffId::new(ReplicaId(7), rng.below(3)),
                epiphany_core::InstrumentId::new(ReplicaId(7), rng.below(2)),
            ),
        }),
        22 => {
            let region = RegionId::new(ReplicaId(7), rng.below(3));
            OperationKind::SetTimeSignature(SetTimeSignatureOp {
                region,
                anchor: valuegen::region_start_anchor(
                    region,
                    MusicalPosition(RationalTime::from_int(rng.below(3) as i32 * 4)),
                ),
                time_signature: (!rng.chance(3)).then(|| {
                    valuegen::time_signature(
                        epiphany_core::TimeSignatureId::new(ReplicaId(7), rng.below(3)),
                        rng.below(3) as u16 + 2,
                    )
                }),
            })
        }
        23 => {
            let region = RegionId::new(ReplicaId(7), rng.below(3));
            let at = rng.below(3) as i32 * 4;
            OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: rng.chance(2).then_some(region),
                start: valuegen::region_start_anchor(
                    region,
                    MusicalPosition(RationalTime::from_int(at)),
                ),
                segment: (!rng.chance(3)).then(|| {
                    valuegen::tempo_segment(
                        region,
                        MusicalPosition(RationalTime::from_int(at)),
                        60.0 + rng.below(4) as f64 * 30.0,
                    )
                }),
            })
        }
        // Repeat authoring (schema-major-2 revision) over the shared
        // event-id space.
        25 => OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
            repeat: if rng.chance(2) {
                valuegen::repeat_structure(
                    RepeatStructureId::new(ReplicaId(7), rng.below(3)),
                    event(rng.below(ID_SPACE)),
                    event(rng.below(ID_SPACE)),
                )
            } else {
                valuegen::volta_repeat(
                    RepeatStructureId::new(ReplicaId(7), rng.below(3)),
                    event(rng.below(ID_SPACE)),
                    event(rng.below(ID_SPACE)),
                )
            },
        }),
        26 => OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp {
            repeat: RepeatStructureId::new(ReplicaId(7), rng.below(3)),
        }),
        // Push 4a. The frozen `Transpose` (arm 6) keeps its own coverage: it
        // must reduce correctly forever, and only a generator will ever author
        // one again.
        27 => OperationKind::TransposeInterval(TransposeIntervalOp {
            targets: (0..1 + rng.below(2))
                .map(|_| pitch(rng.below(ID_SPACE)))
                .collect(),
            interval: TranspositionInterval {
                diatonic_steps: rng.below(5) as i32 - 2,
                chromatic_steps: rng.below(9) as i32 - 4,
            },
        }),
        _ => OperationKind::SetStaffLayout(SetStaffLayoutOp {
            staff_instance: StaffInstanceId::new(ReplicaId(7), rng.below(3)),
            instrument_override: None,
            staff_lines_override: rng
                .chance(2)
                .then(epiphany_core::StaffLineConfiguration::default),
            visible: rng.chance(2),
        }),
    };
    OperationPayload::Primitive(kind)
}

/// Generates a random, mostly-well-formed operation set. Per-replica stamps are
/// monotonic by construction (so most sets are anomaly-free), but the generator
/// occasionally injects equivocation (a duplicate id with mutated bytes) and an
/// HLC-monotonicity anomaly, so those paths are exercised for permutation
/// invariance too.
pub fn gen_envelope_set(rng: &mut SplitMix64, n: usize) -> Vec<OperationEnvelope> {
    let mut counters = [0u64; (REPLICAS + 1) as usize];
    let mut clocks = [0i64; (REPLICAS + 1) as usize];
    let mut stamps = vec![Vec::<(i64, u32)>::new(); (REPLICAS + 1) as usize];
    let mut envs = Vec::with_capacity(n);

    for _ in 0..n {
        let replica = 1 + rng.below(REPLICAS);
        let r = replica as usize;
        let counter = counters[r];
        let id = OperationId::new(ReplicaId(replica), counter);

        // Causal context contains prior history only. Track the maximum stamp
        // among selected predecessors so the new HLC can strictly outrank it.
        let mut ctx = CausalContext::new();
        let mut pred_max = (0i64, 0u32);
        if counter > 0 {
            ctx = ctx.with_seen(ReplicaId(replica), counter - 1);
            pred_max = pred_max.max(stamps[r][(counter - 1) as usize]);
        }
        for rr in 1..counters.len() {
            if rr == r {
                continue;
            }
            let seen = counters[rr];
            if seen > 0 && rng.chance(2) {
                let high = rng.below(seen);
                ctx = ctx.with_seen(ReplicaId(rr as u64), high);
                pred_max = pred_max.max(stamps[rr][high as usize]);
            }
        }

        clocks[r] += rng.below(3) as i64;
        let previous = stamps[r].last().copied().unwrap_or((0, 0));
        let physical = clocks[r].max(previous.0).max(pred_max.0);
        let logical = if physical == previous.0 && physical == pred_max.0 {
            previous.1.max(pred_max.1) + 1
        } else if physical == previous.0 {
            previous.1 + 1
        } else if physical == pred_max.0 {
            pred_max.1 + 1
        } else {
            0
        };
        stamps[r].push((physical, logical));
        counters[r] += 1;

        envs.push(OperationEnvelope {
            id,
            author: AuthorId(replica as u128),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(epiphany_core::WallClockTime(physical), logical),
                id,
            ),
            causal_context: ctx,
            transaction: None,
            payload: gen_payload(rng),
        });
    }

    // Occasionally inject an HLC-monotonicity anomaly: a fresh op on some
    // replica whose physical time is below an earlier one.
    if !envs.is_empty() && rng.chance(4) {
        let replica = 1 + rng.below(REPLICAS);
        let r = replica as usize;
        let counter = counters[r];
        let id = OperationId::new(ReplicaId(replica), counter);
        envs.push(OperationEnvelope {
            id,
            author: AuthorId(replica as u128),
            // physical -1 is below every generated (non-negative) clock value.
            stamp: OperationStamp::new(
                HybridLogicalClock::new(epiphany_core::WallClockTime(0), 0),
                id,
            ),
            causal_context: CausalContext::new(),
            payload: gen_payload(rng),
            transaction: None,
        });
        // The earlier op must outrank it; bump an existing op's clock high.
        if let Some(first) = envs
            .iter_mut()
            .find(|e| e.id.replica == ReplicaId(replica) && e.id.counter < counter)
        {
            first.stamp.hlc.physical_time = epiphany_core::WallClockTime(1_000_000);
        }
    }

    // Occasionally inject equivocation: a duplicate id with mutated payload.
    if !envs.is_empty() && rng.chance(4) {
        let victim = &envs[rng.below(envs.len() as u64) as usize];
        let mut twin = victim.clone();
        twin.payload = OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
            pitch: pitch(rng.below(ID_SPACE)),
            spelling: valuegen::spelling(6),
        }));
        if twin.envelope_hash() != victim.envelope_hash() {
            envs.push(twin);
        }
    }

    envs
}

/// Reduces an envelope set accepted in the given order, returning the canonical
/// materialized bytes.
fn reduce_in_order(envs: &[OperationEnvelope]) -> Vec<u8> {
    let mut set = OperationSet::new();
    set.accept_all(envs.iter().cloned());
    set.reduce().canonical_bytes()
}

/// Runs `iters` reduction-determinism iterations from `seed`. Each iteration
/// generates a random operation set and asserts that several random
/// *acceptance orders* reduce to byte-identical materialized state. Panics on
/// the first violation (the hand-off gate's failure condition).
pub fn run_reduction_determinism_fuzz(iters: u64, seed: u64) {
    let mut rng = SplitMix64::new(seed);
    for _ in 0..iters {
        let n = 1 + rng.below(14) as usize;
        let base = gen_envelope_set(&mut rng, n);
        let reference = reduce_in_order(&base);
        for _ in 0..3 {
            let mut perm = base.clone();
            shuffle(&mut perm, &mut rng);
            let got = reduce_in_order(&perm);
            assert_eq!(
                got, reference,
                "reduction is not permutation-invariant (n = {n})"
            );
        }
    }
}

/// Runs `iters` equivocation iterations from `seed`. Each iteration builds two
/// distinct canonical envelopes under one `OperationId`, accepts them (with a
/// few unrelated envelopes) in a random order, and asserts the slot is
/// `Equivocated`, the operation contributes no effect, and an
/// `OperationSlotEquivocated` anomaly is recorded — regardless of arrival order.
pub fn run_equivocation_fuzz(iters: u64, seed: u64) {
    let mut rng = SplitMix64::new(seed);
    for _ in 0..iters {
        let id = OperationId::new(ReplicaId(1 + rng.below(REPLICAS)), rng.below(5));

        let mk = |rng: &mut SplitMix64, spelling: u8| OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(epiphany_core::WallClockTime(rng.below(100) as i64), 0),
                id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                pitch: pitch(0),
                spelling: valuegen::spelling(spelling),
            })),
        };
        let a = mk(&mut rng, 1);
        let b = mk(&mut rng, 2); // distinct canonical bytes (different spelling)
        debug_assert_ne!(a.envelope_hash(), b.envelope_hash());

        // A few unrelated, well-formed envelopes to vary the surrounding set.
        let noise_count = rng.below(4) as usize;
        let noise = gen_envelope_set(&mut rng, noise_count)
            .into_iter()
            .filter(|e| e.id != id)
            .collect::<Vec<_>>();

        let mut items = vec![a.clone(), b.clone()];
        items.extend(noise);
        shuffle(&mut items, &mut rng);

        let mut set = OperationSet::new();
        set.accept_all(items);

        let slot = set.slot(id).expect("slot exists for the equivocating id");
        assert!(
            slot.is_equivocated(),
            "duplicate id with different bytes must equivocate regardless of order"
        );

        let state = set.reduce();
        assert!(
            state.effects.iter().all(|(e, _)| *e != id),
            "an equivocated operation must produce no canonical effect"
        );
        assert!(
            state.anomalies.iter().any(|an| matches!(
                an.kind,
                IntegrityAnomalyKind::OperationSlotEquivocated { operation_id } if operation_id == id
            )),
            "an equivocated slot must record an OperationSlotEquivocated anomaly"
        );
    }
}

/// Runs `iters` equivocation-*resolution* iterations from `seed` (the
/// `ResolveEquivocation` sibling of [`run_equivocation_fuzz`]). Each iteration
/// builds two distinct canonical envelopes under one `OperationId` plus a
/// `ResolveEquivocation`, embeds them in random noise, and asserts across four
/// random acceptance orders:
///
/// * with a **valid** resolve (`chosen` names a real candidate): the resolved
///   slot contributes an effect at its own id, no `OperationSlotEquivocated`
///   anomaly is recorded for it, the resolve itself applies, and every
///   permutation reduces to byte-identical [`crate::MaterializedState`];
/// * with an **invalid** resolve (`chosen` names no candidate): behavior is
///   exactly today's unresolved equivocation — the slot contributes nothing,
///   the anomaly is recorded, the resolve is a precondition no-op — and the
///   permutation invariance still holds.
pub fn run_equivocation_resolution_fuzz(iters: u64, seed: u64) {
    let mut rng = SplitMix64::new(seed);
    for _ in 0..iters {
        let id = OperationId::new(ReplicaId(1 + rng.below(REPLICAS)), rng.below(5));

        let mk = |rng: &mut SplitMix64, spelling: u8| OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(epiphany_core::WallClockTime(rng.below(100) as i64), 0),
                id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
                pitch: pitch(0),
                spelling: valuegen::spelling(spelling),
            })),
        };
        let a = mk(&mut rng, 1);
        let b = mk(&mut rng, 2); // distinct canonical bytes (different spelling)
        debug_assert_ne!(a.envelope_hash(), b.envelope_hash());

        // Valid two-thirds of the time; otherwise a hash naming no candidate.
        let valid = !rng.chance(3);
        let chosen = if !valid {
            EnvelopeHash([0xEE; 32])
        } else if rng.chance(2) {
            a.envelope_hash()
        } else {
            b.envelope_hash()
        };
        // The resolve lives on a replica the noise generator never draws
        // (noise uses 1..=REPLICAS), so its own slot can neither equivocate
        // nor land in a quarantined segment.
        let resolve_id = OperationId::new(ReplicaId(REPLICAS + 2), 0);
        let resolve = OperationEnvelope {
            id: resolve_id,
            author: AuthorId(0),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(epiphany_core::WallClockTime(rng.below(100) as i64), 0),
                resolve_id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::ResolveEquivocation(
                crate::payload::ResolveEquivocationPayload { target: id, chosen },
            ),
        };

        let noise_count = rng.below(4) as usize;
        let noise = gen_envelope_set(&mut rng, noise_count)
            .into_iter()
            .filter(|e| e.id != id)
            .collect::<Vec<_>>();

        let mut items = vec![a.clone(), b.clone(), resolve.clone()];
        items.extend(noise);

        let mut reference: Option<Vec<u8>> = None;
        for _ in 0..4 {
            shuffle(&mut items, &mut rng);
            let mut set = OperationSet::new();
            set.accept_all(items.iter().cloned());
            let state = set.reduce();

            let slot_contributes = state.effects.iter().any(|(e, _)| *e == id);
            let anomalous = state.anomalies.iter().any(|an| matches!(
                an.kind,
                IntegrityAnomalyKind::OperationSlotEquivocated { operation_id } if operation_id == id
            ));
            let resolve_effect = state
                .effects
                .iter()
                .find(|(e, _)| *e == resolve_id)
                .map(|(_, eff)| eff);
            if valid {
                assert!(
                    slot_contributes,
                    "a resolved slot must contribute its chosen candidate's effect"
                );
                assert!(
                    !anomalous,
                    "a resolved slot must record no OperationSlotEquivocated anomaly"
                );
                assert_eq!(
                    resolve_effect,
                    Some(&OperationEffect::Applied),
                    "the governing resolve must apply"
                );
            } else {
                assert!(
                    !slot_contributes,
                    "an unresolved equivocated slot must contribute nothing"
                );
                assert!(
                    anomalous,
                    "an unresolved equivocated slot must record its anomaly"
                );
                assert!(
                    matches!(resolve_effect, Some(OperationEffect::NoOp { .. })),
                    "an invalid resolve must be a precondition no-op, got {resolve_effect:?}"
                );
            }

            let bytes = state.canonical_bytes();
            match &reference {
                None => reference = Some(bytes),
                Some(reference) => assert_eq!(
                    &bytes, reference,
                    "equivocation resolution is not permutation-invariant"
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Decode fuzzing (P2 of the decode-hardening track; P1 covered `epiphany-core`).
//
// The operation layer exposes exactly two byte-decode surfaces:
// `MaterializedState::decode_canonical` and `OperationKindTag::decode_canonical`.
// Operation *payloads* have no decoder — `OperationKind` is encode-only — so
// there is nothing here that could accept a duplicate `TransposeInterval`
// target. When such a decoder lands it inherits the wire table's `seq^⇑` rule:
// reject a duplicate, never normalize it away.
//
// The properties, exactly as in P1: an adversarial byte string must never panic
// a decoder, and an *accepted* string must re-encode to itself. Canonical decode
// is injective, which is what content-addressing rests on.
// ---------------------------------------------------------------------------

/// How much of the decode surface a fuzz run actually reached. A harness that
/// never gets a decoder to say `Ok` proves nothing about injectivity, so the
/// run asserts on these rather than merely on the absence of a panic.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct DecodeFuzzCoverage {
    pub states_accepted: u64,
    pub states_rejected: u64,
    pub tags_accepted: u64,
    pub tags_rejected: u64,
}

struct DecodeCorpus {
    states: Vec<Vec<u8>>,
    tags: Vec<Vec<u8>>,
}

/// Builds the pool of valid canonical byte strings to mutate.
///
/// Corpus depth is load-bearing. A small envelope set reduces to a state with no
/// conflicts, anomalies, pending operations, or spellings — and those branches
/// carry every canonical-order check the decoder makes. A corpus without them
/// leaves the decoder's interesting half unreached, and a green run proves
/// nothing.
///
/// So the corpus is *drawn until covered* rather than drawn once and hoped over:
/// a fixed set of sizes is seed-dependent (measured: 6 of 12 seeds failed to
/// produce all four structures). Each round adds one state per size; the loop
/// stops as soon as every structure has appeared. The final assertion is then a
/// property of the harness, not of the seed.
fn build_decode_corpus(rng: &mut SplitMix64) -> DecodeCorpus {
    const MAX_ROUNDS: usize = 12;
    let mut states = Vec::new();
    let (mut conflicts, mut anomalies, mut pending, mut spellings) = (false, false, false, false);

    for round in 0..MAX_ROUNDS {
        for n in [0usize, 1, 8, 200, 400] {
            // Skip the trivial sizes after the first round: they only ever add
            // shallow states, and one of each is enough to cover the empty and
            // near-empty encodings.
            if round > 0 && n < 200 {
                continue;
            }
            let envelopes = gen_envelope_set(rng, n);
            let mut set = OperationSet::new();
            set.accept_all(envelopes);
            let state = set.reduce();
            conflicts |= !state.conflicts.records().is_empty();
            anomalies |= !state.anomalies.is_empty();
            pending |= !state.pending.is_empty();
            spellings |= !state.spellings.is_empty();
            states.push(state.canonical_bytes());
        }
        if conflicts && anomalies && pending && spellings {
            break;
        }
    }
    assert!(
        conflicts && anomalies && pending && spellings,
        "decode corpus never reached every structure in {MAX_ROUNDS} rounds \
         (conflicts={conflicts}, anomalies={anomalies}, pending={pending}, \
         spellings={spellings}); the decoder's canonical-order checks live in \
         exactly those branches"
    );

    let tags = [
        OperationKindTag::InsertEvent,
        OperationKindTag::Transpose,
        OperationKindTag::TransposeInterval,
        OperationKindTag::DeleteRepeatStructure,
        OperationKindTag::Registered(crate::OperationKindRegistryId(0x0123_4567_89AB_CDEF)),
    ]
    .iter()
    .map(|t| t.to_canonical_bytes())
    .collect();
    DecodeCorpus { states, tags }
}

fn pick<'a>(rng: &mut SplitMix64, pool: &'a [Vec<u8>]) -> &'a [u8] {
    &pool[(rng.next_u64() as usize) % pool.len()]
}

fn random_bytes(rng: &mut SplitMix64, n: usize) -> Vec<u8> {
    (0..n).map(|_| rng.next_u64() as u8).collect()
}

fn substitute(rng: &mut SplitMix64, bytes: &mut [u8], k: usize) {
    if bytes.is_empty() {
        return;
    }
    for _ in 0..k {
        let i = (rng.next_u64() as usize) % bytes.len();
        bytes[i] = rng.next_u64() as u8;
    }
}

/// Overwrites a random 4-byte window with a fresh, often-extreme `u32`: the
/// count/length-prefix attack a `Vec` decoder trusts for its element count.
fn corrupt_length_prefix(rng: &mut SplitMix64, bytes: &mut [u8]) {
    if bytes.len() < 4 {
        return;
    }
    let i = (rng.next_u64() as usize) % (bytes.len() - 3);
    let v: u32 = match rng.next_u64() % 3 {
        0 => u32::MAX,
        1 => (rng.next_u64() as u32) | 0x8000_0000,
        _ => rng.next_u64() as u32,
    };
    bytes[i..i + 4].copy_from_slice(&v.to_le_bytes());
}

/// One adversarial `MaterializedState` input. Strategy 1 is *unmutated* valid
/// bytes — a live check that the harness's own corpus round-trips.
fn gen_state_input(rng: &mut SplitMix64, corpus: &DecodeCorpus) -> Vec<u8> {
    match rng.next_u64() % 7 {
        0 => {
            let n = (rng.next_u64() % 512) as usize;
            random_bytes(rng, n)
        }
        1 => pick(rng, &corpus.states).to_vec(),
        2 => {
            let mut b = pick(rng, &corpus.states).to_vec();
            let k = 1 + (rng.next_u64() % 4) as usize;
            substitute(rng, &mut b, k);
            b
        }
        3 => {
            let mut b = pick(rng, &corpus.states).to_vec();
            let t = (rng.next_u64() as usize) % (b.len() + 1);
            b.truncate(t);
            b
        }
        4 => {
            let mut b = pick(rng, &corpus.states).to_vec();
            let n = 1 + (rng.next_u64() % 16) as usize;
            let tail = random_bytes(rng, n);
            b.extend_from_slice(&tail);
            b
        }
        5 => {
            let mut b = pick(rng, &corpus.states).to_vec();
            corrupt_length_prefix(rng, &mut b);
            b
        }
        // A valid `OperationKindTag` standing where a state is expected: a
        // structurally plausible, wrong-type payload.
        _ => pick(rng, &corpus.tags).to_vec(),
    }
}

/// One adversarial `OperationKindTag` input.
fn gen_tag_input(rng: &mut SplitMix64, corpus: &DecodeCorpus) -> Vec<u8> {
    match rng.next_u64() % 5 {
        0 => pick(rng, &corpus.tags).to_vec(),
        1 => {
            let mut b = pick(rng, &corpus.tags).to_vec();
            substitute(rng, &mut b, 1);
            b
        }
        2 => {
            let mut b = pick(rng, &corpus.tags).to_vec();
            let n = 1 + (rng.next_u64() % 4) as usize;
            let tail = random_bytes(rng, n);
            b.extend_from_slice(&tail);
            b
        }
        3 => {
            let mut b = pick(rng, &corpus.tags).to_vec();
            let t = (rng.next_u64() as usize) % (b.len() + 1);
            b.truncate(t);
            b
        }
        _ => {
            let n = (rng.next_u64() % 24) as usize;
            random_bytes(rng, n)
        }
    }
}

/// Runs `iters` adversarial byte-decode iterations from `seed` against the
/// operation layer's two decoders, returning what it reached. A panic, or an
/// accepted byte string that does not re-encode to itself, fails the run;
/// `seed` reproduces it exactly.
pub fn run_decode_fuzz(iters: u64, seed: u64) -> DecodeFuzzCoverage {
    let mut rng = SplitMix64::new(seed);
    let corpus = build_decode_corpus(&mut rng);
    let mut cov = DecodeFuzzCoverage::default();

    for _ in 0..iters {
        let bytes = gen_state_input(&mut rng, &corpus);
        match crate::MaterializedState::decode_canonical(&bytes) {
            Ok(state) => {
                cov.states_accepted += 1;
                assert_eq!(
                    state.canonical_bytes(),
                    bytes,
                    "the materialized-state decoder accepted a non-canonical byte string"
                );
            }
            Err(_) => cov.states_rejected += 1,
        }

        let tag_bytes = gen_tag_input(&mut rng, &corpus);
        match OperationKindTag::decode_canonical(&tag_bytes) {
            Ok(tag) => {
                cov.tags_accepted += 1;
                assert_eq!(
                    tag.to_canonical_bytes(),
                    tag_bytes,
                    "the OperationKindTag decoder accepted a non-canonical byte string"
                );
            }
            Err(_) => cov.tags_rejected += 1,
        }
    }
    cov
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduction_determinism_smoke() {
        run_reduction_determinism_fuzz(500, 0xC0FFEE);
    }

    #[test]
    fn equivocation_smoke() {
        run_equivocation_fuzz(500, 0x1234_5678);
    }

    #[test]
    fn equivocation_resolution_smoke() {
        run_equivocation_resolution_fuzz(500, 0x9E50_1AE5);
    }

    #[test]
    fn generator_is_deterministic() {
        let mut a = SplitMix64::new(99);
        let mut b = SplitMix64::new(99);
        let sa = gen_envelope_set(&mut a, 10);
        let sb = gen_envelope_set(&mut b, 10);
        assert_eq!(sa, sb);
    }

    #[test]
    fn clean_generated_histories_respect_causal_stamps() {
        let mut rng = SplitMix64::new(0xCA05_A117);
        let mut checked = 0;
        for _ in 0..2_000 {
            let set = gen_envelope_set(&mut rng, 24);
            let mut accepted = OperationSet::new();
            accepted.accept_all(set);
            let singles = accepted.single_envelopes();
            if !crate::anomaly::detect_replica_anomalies(&singles).is_empty() {
                continue;
            }
            for successor in &singles {
                for predecessor in &singles {
                    if predecessor.id != successor.id
                        && successor.causal_context.covers(predecessor.id)
                    {
                        assert!(
                            predecessor.stamp.reduction_tuple() < successor.stamp.reduction_tuple(),
                            "causal predecessor did not have a lower stamp"
                        );
                    }
                }
            }
            checked += 1;
        }
        assert!(checked > 1_000, "too few clean generated histories checked");
    }

    /// Two deterministic smoke seeds over the decode surface. A deeper sweep
    /// runs the same entry point with a larger budget; a failure reproduces
    /// exactly from its seed.
    ///
    /// The coverage assertions are the point. A decode fuzzer that never gets a
    /// decoder to say `Ok` proves nothing about injectivity — it only proves the
    /// absence of a panic — so the run fails if it did not reach the accept path
    /// on both surfaces.
    #[test]
    fn decode_fuzz_smoke_seed_a() {
        let cov = run_decode_fuzz(20_000, 0x0DEC_0DE0_F022_1234);
        assert!(cov.states_accepted > 1_000, "{cov:?}");
        assert!(cov.states_rejected > 1_000, "{cov:?}");
        assert!(cov.tags_accepted > 1_000, "{cov:?}");
        assert!(cov.tags_rejected > 1_000, "{cov:?}");
    }

    #[test]
    fn decode_fuzz_smoke_seed_b() {
        let cov = run_decode_fuzz(20_000, 0xF0FA_11BA_C0DE_5EED);
        assert!(cov.states_accepted > 1_000, "{cov:?}");
        assert!(cov.tags_accepted > 1_000, "{cov:?}");
    }

    /// `objects` is a `BTreeMap` with **no per-site order check**: the decoder
    /// re-sorts it silently. Only the whole-state re-encode-and-compare guard
    /// can reject an out-of-order encoding of it, so this is the deterministic
    /// lock on that guard — the fuzzer's 2M-input sweep of the same property
    /// finds it too, but a seeded sweep is not a proof.
    ///
    /// Without the guard the decoder would be non-injective: two distinct byte
    /// strings would decode to one value, which is what content-addressing
    /// forbids.
    #[test]
    fn an_out_of_order_objects_map_is_rejected_by_the_whole_state_guard() {
        use crate::ObjectState;
        use epiphany_core::EventId;

        let lo = TypedObjectId::Event(EventId::new(ReplicaId(1), 1));
        let hi = TypedObjectId::Event(EventId::new(ReplicaId(1), 2));
        assert!(lo < hi);

        let empty = crate::MaterializedState::default().canonical_bytes();
        let mut state = crate::MaterializedState::default();
        state.objects.insert(lo, ObjectState::Live);
        state.objects.insert(hi, ObjectState::Live);
        let bytes = state.canonical_bytes();
        assert!(crate::MaterializedState::decode_canonical(&bytes).is_ok());

        // The two encodings agree until the objects count, and differ in length
        // by exactly the two entries.
        let count_at = empty
            .iter()
            .zip(bytes.iter())
            .position(|(a, b)| a != b)
            .expect("the objects count differs");
        let entry = (bytes.len() - empty.len()) / 2;
        let first = count_at + 4;
        let second = first + entry;

        // Swap the two entries: structurally valid, canonically wrong.
        let mut swapped = bytes.clone();
        swapped[first..second].copy_from_slice(&bytes[second..second + entry]);
        swapped[second..second + entry].copy_from_slice(&bytes[first..second]);
        assert_ne!(swapped, bytes, "the swap changed the bytes");

        assert_eq!(
            crate::MaterializedState::decode_canonical(&swapped),
            Err(crate::MaterializedDecodeError::NonCanonical),
            "an out-of-order objects map must be rejected, never silently re-sorted"
        );
    }

    /// **The guard is not complete for order-preserving fields.**
    ///
    /// `MaterializedState`'s decode has two layers. The whole-state
    /// re-encode-and-compare guard catches every field the decoder *normalizes*
    /// — the `BTreeMap`s re-sort and de-duplicate, so a non-canonical encoding
    /// of them cannot survive a round trip. It is blind to `Vec` fields, whose
    /// order the decoder preserves: a reordered `anomalies` or `pending` list
    /// re-encodes to exactly the bytes it came from.
    ///
    /// Those fields therefore need per-site canonical-order checks, and those
    /// checks are invisible to an injectivity fuzzer — verified by removing them
    /// and watching a 40K-input sweep stay green. The deterministic tests here
    /// are the only thing that locks them.
    ///
    /// (`effects` is a `Vec` with no order check by design: its canonical order
    /// is *reduction* order, which a decoder cannot recompute. Two orderings are
    /// two different states, so injectivity is not at stake.)
    #[test]
    fn an_out_of_order_anomaly_list_is_rejected() {
        use crate::{IntegrityAnomaly, IntegrityAnomalyKind};

        let a = IntegrityAnomaly::new(IntegrityAnomalyKind::OperationSlotEquivocated {
            operation_id: OperationId::new(ReplicaId(1), 1),
        });
        let b = IntegrityAnomaly::new(IntegrityAnomalyKind::OperationSlotEquivocated {
            operation_id: OperationId::new(ReplicaId(1), 2),
        });
        let (lo, hi) = if a.id < b.id { (a, b) } else { (b, a) };

        let empty = crate::MaterializedState::default().canonical_bytes();
        let state = crate::MaterializedState {
            anomalies: vec![lo.clone(), hi.clone()],
            ..Default::default()
        };
        let bytes = state.canonical_bytes();
        assert!(crate::MaterializedState::decode_canonical(&bytes).is_ok());

        // The anomalies are equal-length, length-prefixed entries, so swapping
        // them is a pure permutation of the byte string.
        let count_at = empty
            .iter()
            .zip(bytes.iter())
            .position(|(x, y)| x != y)
            .expect("the anomaly count differs");
        let entry = (bytes.len() - empty.len()) / 2;
        let first = count_at + 4;
        let second = first + entry;

        let mut swapped = bytes.clone();
        swapped[first..second].copy_from_slice(&bytes[second..second + entry]);
        swapped[second..second + entry].copy_from_slice(&bytes[first..second]);
        assert_ne!(swapped, bytes);

        // The decoder preserves `Vec` order, so `swapped` re-encodes to itself:
        // the whole-state guard cannot see this. Only the per-site check can.
        assert_eq!(
            crate::MaterializedState::decode_canonical(&swapped),
            Err(crate::MaterializedDecodeError::NonCanonical),
            "an out-of-order anomaly register must be rejected"
        );
    }

    /// The corpus must reach the decoder's interesting half regardless of seed.
    /// A fixed list of envelope-set sizes does not: measured, 6 of 12 seeds
    /// failed to produce all four structures, so `build_decode_corpus` draws
    /// until covered. This pins that.
    #[test]
    fn the_decode_corpus_is_covered_for_every_seed() {
        for seed in 1u64..=12 {
            let _ = build_decode_corpus(&mut SplitMix64::new(seed));
        }
    }

    /// The `pending` list carries its own per-site canonical-order check, so a
    /// reordered one is rejected even without the whole-state guard. This locks
    /// that check specifically (verified: it still passes when the guard is
    /// removed).
    #[test]
    fn a_reordered_pending_list_is_rejected() {
        use crate::PendingReason;
        let mut state = crate::MaterializedState::default();
        let lo = OperationId::new(ReplicaId(1), 1);
        let hi = OperationId::new(ReplicaId(1), 2);
        state.pending = vec![(lo, PendingReason::MissingCausalPredecessor { missing: lo })];
        let canonical = state.canonical_bytes();
        assert!(crate::MaterializedState::decode_canonical(&canonical).is_ok());

        // Two entries in canonical order decode; the same two reversed do not.
        state.pending = vec![
            (lo, PendingReason::MissingCausalPredecessor { missing: lo }),
            (hi, PendingReason::MissingCausalPredecessor { missing: lo }),
        ];
        let ok_bytes = state.canonical_bytes();
        assert!(crate::MaterializedState::decode_canonical(&ok_bytes).is_ok());

        state.pending = vec![
            (hi, PendingReason::MissingCausalPredecessor { missing: lo }),
            (lo, PendingReason::MissingCausalPredecessor { missing: lo }),
        ];
        let bad_bytes = state.canonical_bytes();
        assert_ne!(ok_bytes, bad_bytes, "the two orders differ on the wire");
        assert_eq!(
            crate::MaterializedState::decode_canonical(&bad_bytes),
            Err(crate::MaterializedDecodeError::NonCanonical),
            "a non-canonical pending order must be rejected, never re-sorted"
        );
    }
}
