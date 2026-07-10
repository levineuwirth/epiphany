//! Decode conformance vectors for the operation layer (P4 of the
//! decode-hardening track).
//!
//! A curated, committed corpus of byte strings with their normative accept /
//! reject verdict. The reference implementation's own fuzzer proves *its*
//! decoders self-consistent; these vectors say what any decoder must do, so a
//! second implementation can be checked against the format rather than against
//! this code.
//!
//! Each rejection class here is one this repository actually shipped a bug in,
//! or one whose check is invisible to an injectivity fuzzer (see
//! `DECISIONS.md` §"Push 5 / P2"): the whole-state re-encode guard catches
//! fields the decoder *normalizes*, and is blind to order-preserving `Vec`
//! fields, which need per-site order checks. A conforming decoder needs both.
//!
//! The `class` string is informative, not normative: implementations need not
//! agree on error taxonomy, only on the accept/reject verdict.

use epiphany_core::{EventId, OperationId, ReplicaId, TypedObjectId};
use epiphany_determinism::CanonicalEncode;

use crate::{
    IntegrityAnomaly, IntegrityAnomalyKind, MaterializedState, ObjectState,
    OperationKindRegistryId, OperationKindTag, PendingReason,
};

/// One vector: `(surface, verdict, class, name, bytes)`.
///
/// `verdict` is `"accept"` or `"reject"`. An `accept` vector additionally
/// asserts **injectivity**: the decoded value must re-encode to exactly these
/// bytes.
pub type DecodeVector = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Vec<u8>,
);

/// Swaps the two equal-length records of `entry` bytes that begin at `first`.
fn swap_records(bytes: &[u8], first: usize, entry: usize) -> Vec<u8> {
    let second = first + entry;
    let mut out = bytes.to_vec();
    out[first..second].copy_from_slice(&bytes[second..second + entry]);
    out[second..second + entry].copy_from_slice(&bytes[first..second]);
    out
}

/// The offset of the count that first differs between an empty encoding and a
/// two-element one, and the per-record width. Both encodings agree up to the
/// count, and differ in total length by exactly the two records.
fn count_and_entry(empty: &[u8], two: &[u8]) -> (usize, usize) {
    let count_at = empty
        .iter()
        .zip(two.iter())
        .position(|(a, b)| a != b)
        .expect("the counts differ");
    (count_at, (two.len() - empty.len()) / 2)
}

fn object(counter: u64) -> TypedObjectId {
    TypedObjectId::Event(EventId::new(ReplicaId(1), counter))
}

fn anomaly(counter: u64) -> IntegrityAnomaly {
    IntegrityAnomaly::new(IntegrityAnomalyKind::OperationSlotEquivocated {
        operation_id: OperationId::new(ReplicaId(1), counter),
    })
}

/// Every operation-layer vector.
pub fn decode_vectors() -> Vec<DecodeVector> {
    let mut v: Vec<DecodeVector> = Vec::new();

    // --- MaterializedState -------------------------------------------------
    const MS: &str = "ops.materialized_state";
    let empty = MaterializedState::default().canonical_bytes();
    v.push((MS, "accept", "-", "empty_state", empty.clone()));

    // Two objects, canonically ordered. Swapping them is caught only by the
    // whole-state re-encode guard: `objects` is a BTreeMap, so the decoder
    // silently re-sorts it and no per-site check exists.
    let two_objects = MaterializedState {
        objects: [
            (object(1), ObjectState::Live),
            (object(2), ObjectState::Live),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    }
    .canonical_bytes();
    let (at, entry) = count_and_entry(&empty, &two_objects);
    v.push((MS, "accept", "-", "two_objects", two_objects.clone()));
    v.push((
        MS,
        "reject",
        "non-canonical-map-order",
        "objects_out_of_order",
        swap_records(&two_objects, at + 4, entry),
    ));

    // Two anomalies, canonically ordered. `anomalies` is a Vec whose order the
    // decoder PRESERVES, so a swap re-encodes to itself and the whole-state
    // guard is blind: only a per-site order check rejects it.
    let (lo, hi) = {
        let (a, b) = (anomaly(1), anomaly(2));
        if a.id < b.id {
            (a, b)
        } else {
            (b, a)
        }
    };
    let two_anomalies = MaterializedState {
        anomalies: vec![lo, hi],
        ..Default::default()
    }
    .canonical_bytes();
    let (at, entry) = count_and_entry(&empty, &two_anomalies);
    v.push((MS, "accept", "-", "two_anomalies", two_anomalies.clone()));
    v.push((
        MS,
        "reject",
        "non-canonical-vec-order",
        "anomalies_out_of_order",
        swap_records(&two_anomalies, at + 4, entry),
    ));

    // Same for `pending`, whose entries are (OperationId, PendingReason) pairs.
    let (p1, p2) = (
        OperationId::new(ReplicaId(1), 1),
        OperationId::new(ReplicaId(1), 2),
    );
    let two_pending = MaterializedState {
        pending: vec![
            (p1, PendingReason::MissingCausalPredecessor { missing: p1 }),
            (p2, PendingReason::MissingCausalPredecessor { missing: p1 }),
        ],
        ..Default::default()
    }
    .canonical_bytes();
    let (at, entry) = count_and_entry(&empty, &two_pending);
    v.push((MS, "accept", "-", "two_pending", two_pending.clone()));
    v.push((
        MS,
        "reject",
        "non-canonical-vec-order",
        "pending_out_of_order",
        swap_records(&two_pending, at + 4, entry),
    ));

    let mut trailing = empty.clone();
    trailing.push(0);
    v.push((
        MS,
        "reject",
        "trailing-bytes",
        "empty_state_trailing",
        trailing,
    ));

    let mut truncated = empty.clone();
    truncated.pop();
    v.push((
        MS,
        "reject",
        "truncated",
        "empty_state_truncated",
        truncated,
    ));

    // A count prefix far past the bytes remaining. The decoder must not
    // pre-allocate on it, and must not loop toward EOF for a measurable time.
    let mut huge_count = empty.clone();
    huge_count[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    v.push((
        MS,
        "reject",
        "count-exceeds-remaining",
        "effects_count_u32_max",
        huge_count,
    ));

    // --- OperationKindTag --------------------------------------------------
    const TAG: &str = "ops.operation_kind_tag";
    for (name, tag) in [
        ("insert_event", OperationKindTag::InsertEvent),
        ("transpose_frozen", OperationKindTag::Transpose),
        ("transpose_interval", OperationKindTag::TransposeInterval),
        (
            "registered",
            OperationKindTag::Registered(OperationKindRegistryId(0x0123_4567_89AB_CDEF)),
        ),
    ] {
        v.push((TAG, "accept", "-", name, tag.to_canonical_bytes()));
    }

    v.push((TAG, "reject", "unknown-discriminant", "tag_200", vec![200]));
    v.push((TAG, "reject", "truncated", "tag_empty", Vec::new()));
    v.push((
        TAG,
        "reject",
        "trailing-bytes",
        "insert_event_trailing",
        vec![0, 0],
    ));
    // `Registered` is 1 + 16 bytes; one short must not read past the end.
    let mut short_registered =
        OperationKindTag::Registered(OperationKindRegistryId(1)).to_canonical_bytes();
    short_registered.pop();
    v.push((
        TAG,
        "reject",
        "truncated",
        "registered_one_byte_short",
        short_registered,
    ));

    v
}

/// Applies `surface`'s decoder to `bytes`.
///
/// `Ok(injective)` means the decoder **accepted**, and `injective` says whether
/// the value re-encodes to exactly these bytes. `Err` means it **rejected**.
///
/// The two are deliberately not collapsed. A decoder that accepts non-canonical
/// bytes and silently normalizes them is *not* rejecting them — that is the
/// whole defect class (`non-canonical-map-order`, `lenient-sub-codec`), and an
/// earlier version of this function reported it as a rejection, so the corpus
/// passed against decoders it was written to catch.
///
/// `None` when the surface is not owned by this crate.
pub fn check(surface: &str, bytes: &[u8]) -> Option<Result<bool, String>> {
    match surface {
        "ops.materialized_state" => Some(match MaterializedState::decode_canonical(bytes) {
            Ok(state) => Ok(state.canonical_bytes() == bytes),
            Err(e) => Err(format!("{e}")),
        }),
        "ops.operation_kind_tag" => Some(decode_tag(bytes)),
        _ => None,
    }
}

fn decode_tag(bytes: &[u8]) -> Result<bool, String> {
    use epiphany_determinism::CanonicalDecode;
    match OperationKindTag::decode_canonical(bytes) {
        Ok(tag) => Ok(tag.to_canonical_bytes() == bytes),
        Err(e) => Err(format!("{e:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each vector must get the verdict it declares. This is the property a
    /// second implementation is being asked to satisfy; if the reference cannot,
    /// the corpus is wrong.
    #[test]
    fn every_vector_gets_its_declared_verdict() {
        for (surface, verdict, class, name, bytes) in decode_vectors() {
            let result = check(surface, &bytes).expect("a surface this crate owns");
            match (verdict, &result) {
                ("accept", Ok(true)) => {}
                ("accept", Ok(false)) => {
                    panic!("{surface}/{name}: accepted but does not re-encode to its bytes")
                }
                ("reject", Err(_)) => {}
                _ => panic!("{surface}/{name} ({class}): declared {verdict}, got {result:?}"),
            }
        }
    }

    /// The corpus must actually contain both verdicts on every surface, or it is
    /// pinning half a contract.
    #[test]
    fn every_surface_carries_both_verdicts() {
        for surface in ["ops.materialized_state", "ops.operation_kind_tag"] {
            let rows: Vec<_> = decode_vectors()
                .into_iter()
                .filter(|(s, ..)| *s == surface)
                .collect();
            assert!(
                rows.iter().any(|(_, v, ..)| *v == "accept"),
                "{surface} has no accept vector"
            );
            assert!(
                rows.iter().any(|(_, v, ..)| *v == "reject"),
                "{surface} has no reject vector"
            );
        }
    }

    /// The two rejection classes that need *different* machinery: a map order a
    /// re-encode guard catches, and a `Vec` order only a per-site check catches.
    /// If either vector went missing the corpus would stop pinning the lesson.
    #[test]
    fn the_corpus_pins_both_non_canonical_classes() {
        let classes: Vec<&str> = decode_vectors().iter().map(|(_, _, c, ..)| *c).collect();
        assert!(classes.contains(&"non-canonical-map-order"));
        assert!(classes.contains(&"non-canonical-vec-order"));
    }
}
