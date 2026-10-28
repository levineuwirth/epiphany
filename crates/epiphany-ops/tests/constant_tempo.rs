//! A constant tempo segment records no end tempo (P13-S8, reduction version 5):
//! an operation carrying one fails its value's decoder, and one built in memory
//! is refused `TempoMapMalformed` in both modes, where version 4 applied a
//! segment whose end tempo equalled its start.

use epiphany_core::{
    check_invariants, IdentityContext, MusicalPosition, OperationId, RegionId, ReplicaId, Score,
    Tempo, TempoSegment, TempoShape, WallClockTime,
};
use epiphany_ops::{
    decode_envelope, valuegen, AuthorId, CausalContext, CreateRegionOp, EnvelopeDecodeError,
    HybridLogicalClock, NoOpReason, OperationEffect, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet, OperationStamp, PreconditionFailureReason, SetTempoSegmentOp,
};

const AUTHOR: ReplicaId = ReplicaId(5);

fn envelope(counter: u64, kind: OperationKind) -> OperationEnvelope {
    let id = OperationId::new(AUTHOR, counter);
    OperationEnvelope {
        id,
        author: AuthorId(5),
        stamp: OperationStamp::new(
            HybridLogicalClock::new(WallClockTime(counter as i64 + 1), 0),
            id,
        ),
        causal_context: match counter {
            0 => CausalContext::new(),
            n => CausalContext::new().with_seen(AUTHOR, n - 1),
        },
        transaction: None,
        payload: OperationPayload::Primitive(kind),
    }
}

/// A region and a constant tempo segment at its start, recording `end_tempo`.
fn history(end_tempo: Option<Tempo>) -> Vec<OperationEnvelope> {
    let region = RegionId::new(AUTHOR, 900);
    let start = valuegen::region_start_anchor(region, MusicalPosition::origin());
    let tempo = Tempo::quarter(96.0).expect("a tempo");
    vec![
        envelope(
            0,
            OperationKind::CreateRegion(CreateRegionOp {
                region: valuegen::region(region),
            }),
        ),
        envelope(
            1,
            OperationKind::SetTempoSegment(SetTempoSegmentOp {
                region: Some(region),
                start: start.clone(),
                segment: Some(TempoSegment {
                    start,
                    end: None,
                    start_tempo: tempo,
                    end_tempo,
                    shape: TempoShape::Constant,
                }),
            }),
        ),
    ]
}

/// The segment's effect in both modes, which agree.
fn effect(history: &[OperationEnvelope]) -> OperationEffect {
    let mut set = OperationSet::new();
    set.accept_all(history.iter().cloned());
    let free = set.reduce();
    let aware = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(0))));
    assert_eq!(free.effects, aware.state.effects);
    assert!(check_invariants(&aware.score).is_empty());
    free.effects
        .iter()
        .find(|(id, _)| *id == history[1].id)
        .expect("reduced")
        .1
        .clone()
}

#[test]
fn a_constant_segment_recording_an_end_tempo_is_refused_in_both_modes() {
    let tempo = Tempo::quarter(96.0).expect("a tempo");
    assert_eq!(effect(&history(None)), OperationEffect::Applied);
    for end in [tempo, Tempo::quarter(60.0).expect("a tempo")] {
        assert_eq!(
            effect(&history(Some(end))),
            OperationEffect::NoOp {
                reason: NoOpReason::PreconditionFailedUnderReduction {
                    reason: PreconditionFailureReason::TempoMapMalformed,
                },
            }
        );
    }
}

#[test]
fn an_operation_carrying_one_fails_its_value_s_decoder() {
    use epiphany_determinism::CanonicalEncode;
    let open = history(None);
    assert_eq!(
        decode_envelope(&open[1].to_canonical_bytes()).as_ref(),
        Ok(&open[1])
    );
    let spelled = history(Some(Tempo::quarter(96.0).expect("a tempo")));
    assert_eq!(
        decode_envelope(&spelled[1].to_canonical_bytes()),
        Err(EnvelopeDecodeError::InvalidValue("TempoSegment"))
    );
}
