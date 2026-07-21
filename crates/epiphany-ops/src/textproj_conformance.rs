//! Cross-layer conformance tests for operation text projection.
//!
//! These tests deliberately join the text projector, strict parser, and binary
//! decoder. Keeping them together avoids duplicating full-envelope fixtures in
//! the leaf and kind projection modules.

use std::collections::BTreeSet;

use epiphany_core::textvalue::{read_sexp, Sexp, TextValue};
use epiphany_core::{
    EventId, MusicalPosition, OperationId, PitchId, RationalTime, ReplicaId, TransactionId,
    TranspositionInterval, TupletId, WallClockTime,
};
use epiphany_determinism::CanonicalEncode;

use crate::causal::CausalContext;
use crate::envdecode::{decode_envelope, tests::sample_kind};
use crate::envelope::OperationEnvelope;
use crate::payload::{
    OperationKind, OperationKindTag, OperationPayload, PositionRemapping, TransposeIntervalOp,
};
use crate::stamp::{HybridLogicalClock, OperationStamp};
use crate::support::AuthorId;
use crate::textproj_envelope::{parse_envelope, project_envelope};
use crate::TupletCompensation;

fn envelope(kind: OperationKind) -> OperationEnvelope {
    let id = OperationId::new(ReplicaId(7), 1);
    OperationEnvelope {
        id,
        author: AuthorId(0x1122_3344),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(42), 7), id),
        causal_context: CausalContext::new()
            .with_seen(ReplicaId(1), 3)
            .with_seen(ReplicaId(2), 5)
            .with_dot(OperationId::new(ReplicaId(3), 9))
            .with_dot(OperationId::new(ReplicaId(4), 11)),
        transaction: Some(TransactionId::new(ReplicaId(7), 5)),
        payload: OperationPayload::Primitive(kind),
    }
}

fn projected_kind(text: &str) -> Sexp {
    let tree = read_sexp(text).expect("the projector emits one valid s-expression");
    let Sexp::List(envelope) = tree else {
        panic!("an envelope projection is a list")
    };
    let Sexp::List(payload) = &envelope[6] else {
        panic!("the envelope payload is a list")
    };
    assert_eq!(payload[0].as_symbol(), Some("primitive"));
    payload[1].clone()
}

fn list(s: &Sexp) -> &[Sexp] {
    let Sexp::List(items) = s else {
        panic!("the selected production field is a list")
    };
    items
}

fn assert_parses_and_matches_binary(env: &OperationEnvelope, text: &str) {
    parse_envelope(text).expect("the projector must emit text its strict parser accepts");
    let via_binary =
        decode_envelope(&env.to_canonical_bytes()).expect("the canonical binary form decodes");
    assert_eq!(
        project_envelope(&via_binary),
        text,
        "text and binary projection must apply identical normalization"
    );
}

fn position(n: i32) -> MusicalPosition {
    MusicalPosition(RationalTime::from_int(n))
}

#[test]
fn transpose_targets_are_normalized_on_projection() {
    let high = PitchId::new(ReplicaId(9), 900);
    let low = PitchId::new(ReplicaId(9), 2);
    assert!(high.to_canonical_bytes() > low.to_canonical_bytes());

    let mut kind = sample_kind(OperationKindTag::Transpose);
    let OperationKind::Transpose(op) = &mut kind else {
        panic!("the exhaustive sample matches its requested tag")
    };
    op.targets = vec![high, low];

    let env = envelope(kind);
    let text = project_envelope(&env);
    let kind = projected_kind(&text);
    assert_eq!(list(&kind)[1], vec![low, high].project());
    assert_parses_and_matches_binary(&env, &text);
}

#[test]
fn declared_incompatible_events_are_normalized_on_projection() {
    let high = EventId::new(ReplicaId(9), 900);
    let low = EventId::new(ReplicaId(9), 2);
    assert!(high.to_canonical_bytes() > low.to_canonical_bytes());

    let mut kind = sample_kind(OperationKindTag::ChangeRegionTimeModel);
    let OperationKind::ChangeRegionTimeModel(op) = &mut kind else {
        panic!("the exhaustive sample matches its requested tag")
    };
    op.declared_incompatible = vec![high, low];

    let env = envelope(kind);
    let text = project_envelope(&env);
    let kind = projected_kind(&text);
    assert_eq!(list(&kind)[3], vec![low, high].project());
    assert_parses_and_matches_binary(&env, &text);
}

#[test]
fn rewrite_tuplets_are_normalized_on_projection() {
    let high = TupletId::new(ReplicaId(9), 900);
    let low = TupletId::new(ReplicaId(9), 2);
    assert!(high.to_canonical_bytes() > low.to_canonical_bytes());

    let mut kind = sample_kind(OperationKindTag::DeleteEvent);
    let OperationKind::DeleteEvent(op) = &mut kind else {
        panic!("the exhaustive sample matches its requested tag")
    };
    op.tuplet_compensation = TupletCompensation::RewriteTuplets {
        tuplets: vec![high, low],
    };

    let env = envelope(kind);
    let text = project_envelope(&env);
    let kind = projected_kind(&text);
    let compensation = list(&kind)[2].clone();
    assert_eq!(list(&compensation)[1], vec![low, high].project());
    assert_parses_and_matches_binary(&env, &text);
}

#[test]
fn cascade_delete_tuplets_are_normalized_on_projection() {
    let high = TupletId::new(ReplicaId(9), 900);
    let low = TupletId::new(ReplicaId(9), 2);
    assert!(high.to_canonical_bytes() > low.to_canonical_bytes());

    let mut kind = sample_kind(OperationKindTag::DeleteEvent);
    let OperationKind::DeleteEvent(op) = &mut kind else {
        panic!("the exhaustive sample matches its requested tag")
    };
    op.tuplet_compensation = TupletCompensation::CascadeDeleteTuplets {
        tuplets: vec![high, low],
    };

    let env = envelope(kind);
    let text = project_envelope(&env);
    let kind = projected_kind(&text);
    let compensation = list(&kind)[2].clone();
    assert_eq!(list(&compensation)[1], vec![low, high].project());
    assert_parses_and_matches_binary(&env, &text);
}

#[test]
fn remapping_entries_are_normalized_on_projection() {
    let high = EventId::new(ReplicaId(9), 900);
    let low = EventId::new(ReplicaId(9), 2);
    assert!(high.to_canonical_bytes() > low.to_canonical_bytes());

    let mut kind = sample_kind(OperationKindTag::ChangeRegionTimeModel);
    let OperationKind::ChangeRegionTimeModel(op) = &mut kind else {
        panic!("the exhaustive sample matches its requested tag")
    };
    op.remapping = PositionRemapping::Reassign(vec![(high, position(9)), (low, position(2))]);

    let env = envelope(kind);
    let text = project_envelope(&env);
    let kind = projected_kind(&text);
    let remapping = list(&kind)[4].clone();
    assert_eq!(
        list(&remapping)[1],
        vec![(low, position(2)), (high, position(9))].project()
    );
    assert_parses_and_matches_binary(&env, &text);
}

#[test]
fn worked_example_envelope_is_byte_exact() {
    let id = OperationId::new(ReplicaId(7), 1);
    let env = OperationEnvelope {
        id,
        author: AuthorId(0x1122_3344),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(42), 7), id),
        causal_context: CausalContext::new()
            .with_seen(ReplicaId(1), 3)
            .with_dot(OperationId::new(ReplicaId(2), 9)),
        transaction: Some(TransactionId::new(ReplicaId(7), 5)),
        payload: OperationPayload::Primitive(OperationKind::TransposeInterval(
            TransposeIntervalOp {
                targets: BTreeSet::from([
                    PitchId::new(ReplicaId(7), 1),
                    PitchId::new(ReplicaId(7), 2),
                ]),
                interval: TranspositionInterval {
                    diatonic_steps: 4,
                    chromatic_steps: 7,
                },
            },
        )),
    };
    let spec = "(envelope #x00000000000000070000000000000001 #x00000000000000000000000011223344 (stamp 42 7 #x00000000000000070000000000000001) (causal ((#x0000000000000001 3)) (#x00000000000000020000000000000009)) (some #x00000000000000070000000000000005) (primitive (transpose-interval (#x00000000000000070000000000000001 #x00000000000000070000000000000002) (transposition-interval 4 7))))";
    assert_eq!(
        project_envelope(&env),
        spec,
        "the companion's worked envelope must remain byte-exact"
    );
}

fn mutants(s: &Sexp, out: &mut Vec<Sexp>) {
    if let Sexp::List(items) = s {
        for index in 0..items.len().saturating_sub(1) {
            let mut mutant = items.clone();
            mutant.swap(index, index + 1);
            out.push(Sexp::List(mutant));
        }
        for index in 0..items.len() {
            let mut mutant = items.clone();
            mutant.insert(index, items[index].clone());
            out.push(Sexp::List(mutant));
        }
        for index in 0..items.len() {
            let mut mutant = items.clone();
            mutant.remove(index);
            out.push(Sexp::List(mutant));
        }
        for (index, child) in items.iter().enumerate() {
            let mut child_mutants = Vec::new();
            mutants(child, &mut child_mutants);
            for child_mutant in child_mutants {
                let mut mutant = items.clone();
                mutant[index] = child_mutant;
                out.push(Sexp::List(mutant));
            }
        }
    }
}

#[test]
fn every_accepted_structural_mutant_reprojects_byte_exactly() {
    let mut checked = 0usize;
    let mut accepted = 0usize;
    let mut violations = Vec::new();

    for tag in OperationKindTag::PAYLOAD_FREE {
        let env = envelope(sample_kind(*tag));
        let text = project_envelope(&env);
        assert_eq!(
            project_envelope(&parse_envelope(&text).expect("canonical text parses")),
            text
        );

        let tree = read_sexp(&text).expect("canonical text is one s-expression");
        let mut structural_mutants = Vec::new();
        mutants(&tree, &mut structural_mutants);
        for mutant in structural_mutants {
            let mutant_text = mutant.render();
            checked += 1;
            if let Ok(parsed) = parse_envelope(&mutant_text) {
                accepted += 1;
                let reprojected = project_envelope(&parsed);
                if reprojected != mutant_text {
                    violations.push((mutant_text, reprojected));
                }
            }
        }
    }

    println!(
        "mutants={checked} accepted={accepted} violations={}",
        violations.len()
    );
    assert!(checked > 3_000, "the sweep must retain structural reach");
    assert!(
        accepted > 100,
        "the sweep must exercise successful parses, not only rejection; accepted {accepted}"
    );
    assert!(
        violations.is_empty(),
        "{} accepted mutants re-projected differently; first violation: {:?}",
        violations.len(),
        violations.first()
    );
}
