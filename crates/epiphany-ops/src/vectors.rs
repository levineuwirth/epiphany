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

use epiphany_core::{
    AnalysisLayerId, EventId, InstrumentId, OperationId, PartDefinitionId, ReplicaId, StaffGroupId,
    StaffId, TypedObjectId, ViewId,
};
use epiphany_determinism::CanonicalEncode;

use crate::{
    IntegrityAnomaly, IntegrityAnomalyKind, MaterializedState, ObjectState, OperationEnvelope,
    OperationKindRegistryId, OperationKindTag, PendingReason,
};

/// One vector: `(surface, verdict, class, name, bytes)`.
///
/// `verdict` is `"accept"` or `"reject"`. An `accept` vector additionally
/// asserts **injectivity**: the decoded value must re-encode to exactly these
/// bytes.
pub type DecodeVector = (&'static str, &'static str, &'static str, String, Vec<u8>);

/// One row. `name` is a `String` because the tag vectors derive theirs from the
/// production vocabulary rather than spelling them.
fn row(
    surface: &'static str,
    verdict: &'static str,
    class: &'static str,
    name: impl Into<String>,
    bytes: Vec<u8>,
) -> DecodeVector {
    (surface, verdict, class, name.into(), bytes)
}

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
    v.push(row(MS, "accept", "-", "empty_state", empty.clone()));

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
    v.push(row(MS, "accept", "-", "two_objects", two_objects.clone()));
    v.push(row(
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
    v.push(row(
        MS,
        "accept",
        "-",
        "two_anomalies",
        two_anomalies.clone(),
    ));
    v.push(row(
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
    v.push(row(MS, "accept", "-", "two_pending", two_pending.clone()));
    v.push(row(
        MS,
        "reject",
        "non-canonical-vec-order",
        "pending_out_of_order",
        swap_records(&two_pending, at + 4, entry),
    ));

    let mut trailing = empty.clone();
    trailing.push(0);
    v.push(row(
        MS,
        "reject",
        "trailing-bytes",
        "empty_state_trailing",
        trailing,
    ));

    let mut truncated = empty.clone();
    truncated.pop();
    v.push(row(
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
    v.push(row(
        MS,
        "reject",
        "count-exceeds-remaining",
        "effects_count_u32_max",
        huge_count,
    ));

    // --- OperationKindTag --------------------------------------------------
    //
    // EVERY tag gets an accept vector, generated from the production vocabulary.
    // A hand-picked subset is how `TransposeInterval` shipped encoding to a byte
    // its own decoder rejected: the corpus never named it. A new tag now lands in
    // the committed file as a new line, and the drift lock forces it into the diff.
    const TAG: &str = "ops.operation_kind_tag";
    for tag in OperationKindTag::PAYLOAD_FREE {
        let name = format!("tag_{:02}", tag.discriminant());
        v.push(row(TAG, "accept", "-", name, tag.to_canonical_bytes()));
    }
    v.push(row(
        TAG,
        "accept",
        "-",
        "registered",
        OperationKindTag::Registered(OperationKindRegistryId(0x0123_4567_89AB_CDEF))
            .to_canonical_bytes(),
    ));

    // One past the vocabulary, computed rather than spelled.
    let unknown = OperationKindTag::PAYLOAD_FREE
        .iter()
        .map(OperationKindTag::discriminant)
        .max()
        .expect("a non-empty vocabulary")
        + 1;
    v.push(row(
        TAG,
        "reject",
        "unknown-discriminant",
        format!("tag_{unknown}_one_past_the_vocabulary"),
        vec![unknown],
    ));

    v.push(row(
        TAG,
        "reject",
        "unknown-discriminant",
        "tag_200",
        vec![200],
    ));
    v.push(row(TAG, "reject", "truncated", "tag_empty", Vec::new()));
    v.push(row(
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
    v.push(row(
        TAG,
        "reject",
        "truncated",
        "registered_one_byte_short",
        short_registered,
    ));

    // --- OperationEnvelope carrying CreateInstrument (genesis tranche G1) --
    //
    // `ops.operation_kind_tag` above pins only the bare, payload-free tag
    // byte; nothing in this corpus previously exercised a *value-carrying*
    // `OperationKind` payload's decode path at all. Committed here so a
    // future encoder/decoder change to this payload moves this vector's
    // bytes deliberately, in the diff (the 3b-i lesson the module doc names:
    // round-trip locking alone cannot see a self-consistent reorder of both
    // halves).
    const OE: &str = "ops.operation_envelope";
    let envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 1),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 1),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::CreateInstrument(crate::payload::CreateInstrumentOp {
                instrument: crate::valuegen::instrument(InstrumentId::new(ReplicaId(1), 1)),
            }),
        ),
    };
    let envelope_bytes = envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "create_instrument",
        envelope_bytes.clone(),
    ));
    let mut trailing = envelope_bytes;
    trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "create_instrument_trailing",
        trailing,
    ));

    // --- OperationEnvelope carrying SetCanvasLayoutDefaults / SetSpellingPrecedence
    // (genesis tranche G2a) — same rationale as CreateInstrument above: nothing
    // else in this corpus exercises either payload's decode path, and a
    // round-trip check alone cannot see a self-consistent encoder/decoder
    // reorder (the 3b-i lesson).
    let layout_envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 2),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 2),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::SetCanvasLayoutDefaults(
                crate::payload::SetCanvasLayoutDefaultsOp {
                    layout_defaults: crate::valuegen::canvas_layout_defaults(1),
                },
            ),
        ),
    };
    let layout_envelope_bytes = layout_envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "set_canvas_layout_defaults",
        layout_envelope_bytes.clone(),
    ));
    let mut layout_trailing = layout_envelope_bytes;
    layout_trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "set_canvas_layout_defaults_trailing",
        layout_trailing,
    ));

    let precedence_envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 3),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 3),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::SetSpellingPrecedence(
                crate::payload::SetSpellingPrecedenceOp {
                    precedence: crate::valuegen::spelling_precedence(1),
                },
            ),
        ),
    };
    let precedence_envelope_bytes = precedence_envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "set_spelling_precedence",
        precedence_envelope_bytes.clone(),
    ));
    let mut precedence_trailing = precedence_envelope_bytes;
    precedence_trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "set_spelling_precedence_trailing",
        precedence_trailing,
    ));

    // --- OperationEnvelope carrying SetTuningContext (genesis tranche G2b) —
    // the sole genesis payload born at schema major 3. Same rationale as the
    // siblings above: nothing else in this corpus exercises this payload's
    // decode path, and a round-trip check alone cannot see a self-consistent
    // encoder/decoder reorder (the 3b-i lesson).
    let tuning_envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 4),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 4),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::SetTuningContext(crate::payload::SetTuningContextOp {
                settings: crate::valuegen::tuning_context_settings(1),
            }),
        ),
    };
    let tuning_envelope_bytes = tuning_envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "set_tuning_context",
        tuning_envelope_bytes.clone(),
    ));
    let mut tuning_trailing = tuning_envelope_bytes;
    tuning_trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "set_tuning_context_trailing",
        tuning_trailing,
    ));

    // --- OperationEnvelope carrying the four genesis tranche G3a root-level
    // mints (`spec/CONTRACT_GENESIS_G3A_ENTITIES.md`) — same rationale as the
    // siblings above: nothing else in this corpus exercises any of these four
    // payloads' decode paths, and a round-trip check alone cannot see a
    // self-consistent encoder/decoder reorder (the 3b-i lesson; contract t4).
    let staff_group_envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 5),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 5),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::CreateStaffGroup(crate::payload::CreateStaffGroupOp {
                group: crate::valuegen::staff_group(
                    StaffGroupId::new(ReplicaId(1), 1),
                    vec![StaffId::new(ReplicaId(1), 1)],
                ),
            }),
        ),
    };
    let staff_group_envelope_bytes = staff_group_envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "create_staff_group",
        staff_group_envelope_bytes.clone(),
    ));
    let mut staff_group_trailing = staff_group_envelope_bytes;
    staff_group_trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "create_staff_group_trailing",
        staff_group_trailing,
    ));

    let part_definition_envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 6),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 6),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::CreatePartDefinition(
                crate::payload::CreatePartDefinitionOp {
                    part: crate::valuegen::part_definition(
                        PartDefinitionId::new(ReplicaId(1), 1),
                        vec![StaffId::new(ReplicaId(1), 1)],
                    ),
                },
            ),
        ),
    };
    let part_definition_envelope_bytes = part_definition_envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "create_part_definition",
        part_definition_envelope_bytes.clone(),
    ));
    let mut part_definition_trailing = part_definition_envelope_bytes;
    part_definition_trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "create_part_definition_trailing",
        part_definition_trailing,
    ));

    let analysis_layer_envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 7),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 7),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::CreateAnalysisLayer(
                crate::payload::CreateAnalysisLayerOp {
                    layer: crate::valuegen::analysis_layer(AnalysisLayerId::new(ReplicaId(1), 1)),
                },
            ),
        ),
    };
    let analysis_layer_envelope_bytes = analysis_layer_envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "create_analysis_layer",
        analysis_layer_envelope_bytes.clone(),
    ));
    let mut analysis_layer_trailing = analysis_layer_envelope_bytes;
    analysis_layer_trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "create_analysis_layer_trailing",
        analysis_layer_trailing,
    ));

    let view_envelope = OperationEnvelope {
        id: OperationId::new(ReplicaId(1), 8),
        author: crate::support::AuthorId(0),
        stamp: crate::stamp::OperationStamp::new(
            crate::stamp::HybridLogicalClock::new(epiphany_core::WallClockTime(1), 1),
            OperationId::new(ReplicaId(1), 8),
        ),
        causal_context: crate::causal::CausalContext::new(),
        transaction: None,
        payload: crate::payload::OperationPayload::Primitive(
            crate::payload::OperationKind::CreateView(crate::payload::CreateViewOp {
                view: crate::valuegen::view(
                    ViewId::new(ReplicaId(1), 1),
                    vec![AnalysisLayerId::new(ReplicaId(1), 1)],
                ),
            }),
        ),
    };
    let view_envelope_bytes = view_envelope.to_canonical_bytes();
    v.push(row(
        OE,
        "accept",
        "-",
        "create_view",
        view_envelope_bytes.clone(),
    ));
    let mut view_trailing = view_envelope_bytes;
    view_trailing.push(0);
    v.push(row(
        OE,
        "reject",
        "trailing-bytes",
        "create_view_trailing",
        view_trailing,
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
        "ops.operation_envelope" => Some(match crate::envdecode::decode_envelope(bytes) {
            Ok(env) => Ok(env.to_canonical_bytes() == bytes),
            Err(e) => Err(format!("{e:?}")),
        }),
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
        for surface in [
            "ops.materialized_state",
            "ops.operation_kind_tag",
            "ops.operation_envelope",
        ] {
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

    /// (i8) Genesis tranche G1
    /// (`spec/CONTRACT_GENESIS_G1_INSTRUMENT.md`): the `CreateInstrument`
    /// envelope decode vector, pinned to a **literal byte array copied from
    /// the committed corpus** (`spec/vectors/decode_vectors.txt`,
    /// `ops.operation_envelope`/`create_instrument`) — not derived by calling
    /// `.to_canonical_bytes()` here. `every_vector_gets_its_declared_verdict`
    /// above checks `decode_vectors()`'s *own* output against `check`, which
    /// cannot see a self-consistent encoder/decoder reorder: the 3b-i lesson
    /// (`epiphany-core`'s `schema_major_3_tuning_context_wire_bytes_are_frozen`)
    /// is that a swap applied identically to both halves passed 1283 tests
    /// and 8/8 conformance, because every check in that failure mode compared
    /// the live encoder against itself. Bytes written here by hand — as this
    /// module's own `decode_vectors()` writes them, into the *committed* file
    /// a future encoder change must move deliberately, in the diff — close
    /// that hole for `CreateInstrument` specifically.
    #[test]
    fn create_instrument_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x1f, 0x41, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x0c, 0x00, 0x00,
            0x00, 0x69, 0x6e, 0x73, 0x74, 0x72, 0x75, 0x6d, 0x65, 0x6e, 0x74, 0x2d, 0x31, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x05, 0x08, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }

    /// (s8) Genesis tranche G2a (`spec/CONTRACT_GENESIS_G2A_SETTINGS.md`): the
    /// `SetCanvasLayoutDefaults` and `SetSpellingPrecedence` envelope decode
    /// vectors, pinned to literal byte arrays copied from the committed
    /// corpus — not derived by calling `.to_canonical_bytes()` here, for the
    /// same reason as `create_instrument_envelope_decode_vector_is_pinned_to_
    /// literal_bytes` above (the 3b-i lesson: round-trip locking alone cannot
    /// see a self-consistent encoder/decoder reorder). Each new payload
    /// carries exactly one field, so there are no adjacent fields to swap;
    /// the mutation this guards against is a swap of the two new
    /// **discriminants** (32 ↔ 33) in both the encoder and the decoder —
    /// self-consistent, so every round-trip test stays green, while these
    /// correctly-named literal vectors die.
    #[test]
    fn set_canvas_layout_defaults_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 72, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0,
            0, 128, 90, 64, 8, 0, 0, 0, 0, 0, 0, 0, 0, 144, 98, 64, 8, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 30, 64, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 30, 64, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 30,
            64, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 30, 64,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }

    /// (s8) Same rationale as the sibling test above.
    #[test]
    fn set_spelling_precedence_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 33, 9, 0, 0, 0, 5, 0, 0, 0, 4, 3, 2, 1, 0,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }

    /// (s8 analogue) Genesis tranche G2b (`spec/CONTRACT_GENESIS_G2B_TUNING.md`
    /// touch row 11): the `SetTuningContext` envelope decode vector, pinned to
    /// a literal byte array copied from the committed corpus — not derived by
    /// calling `.to_canonical_bytes()` here, for the same reason as the
    /// sibling tests above (the 3b-i lesson: round-trip locking alone cannot
    /// see a self-consistent encoder/decoder reorder). The mutation this
    /// guards against is a swap of discriminant 34 with any neighboring
    /// discriminant in both the encoder and the decoder — self-consistent, so
    /// every round-trip test stays green, while this correctly-named literal
    /// vector dies.
    #[test]
    fn set_tuning_context_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 48, 0, 0, 0, 6, 0, 0, 0, 99, 109, 110,
            45, 49, 50, 6, 0, 0, 0, 116, 101, 116, 45, 49, 50, 0, 5, 0, 4, 8, 0, 0, 0, 0, 0, 0,
            0, 0, 144, 123, 64, 1, 0, 40, 0, 1, 0, 40, 0, 0, 0, 0, 0,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }

    /// (t4) Genesis tranche G3a (`spec/CONTRACT_GENESIS_G3A_ENTITIES.md`): the
    /// `CreateStaffGroup` envelope decode vector, pinned to a literal byte
    /// array copied from the committed corpus — not derived by calling
    /// `.to_canonical_bytes()` here, for the same reason as the sibling tests
    /// above (the 3b-i lesson: round-trip locking alone cannot see a
    /// self-consistent encoder/decoder reorder). **Mutation:** swap two
    /// fields in `StaffGroup`'s `struct_codec!` declaration
    /// (`core/src/codec.rs:2329`, e.g. `{ id, name, kind, members }` →
    /// `{ id, kind, name, members }`); this literal-byte vector must fail
    /// while every round-trip test stays green.
    #[test]
    fn create_staff_group_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
            0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            35, 63, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 13, 0,
            0, 0, 115, 116, 97, 102, 102, 45, 103, 114, 111, 117, 112, 45,
            49, 0, 1, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }

    /// (t4) Same rationale as `create_staff_group_envelope_decode_vector_is_
    /// pinned_to_literal_bytes` above. **Mutation:** swap two fields in
    /// `PartDefinition`'s `struct_codec!` declaration
    /// (`core/src/codec.rs:1790`, `{ id, name, staves }` →
    /// `{ id, staves, name }`); this literal-byte vector must fail while
    /// every round-trip test stays green.
    #[test]
    fn create_part_definition_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
            0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            36, 54, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 6, 0, 0,
            0, 112, 97, 114, 116, 45, 49, 1, 0, 0, 0, 16, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
            0, 0, 1,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }

    /// (t4) Same rationale as the sibling tests above. **Mutation:** swap two
    /// fields in `AnalysisLayer`'s `struct_codec!` declaration
    /// (`core/src/codec.rs:1791`, `{ id, name }` → `{ name, id }`); this
    /// literal-byte vector must fail while every round-trip test stays
    /// green. `valuegen::analysis_layer`'s name is deliberately not 16 bytes
    /// (the `id` field's width): a same-width swap of two length-prefixed
    /// leaves re-encodes byte-identically regardless of which field is
    /// which, so an accidental width match would make this vector blind to
    /// exactly the reorder it exists to catch.
    #[test]
    fn create_analysis_layer_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
            0, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            37, 31, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 7, 0, 0,
            0, 108, 97, 121, 101, 114, 45, 49,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }

    /// (t4) Same rationale as the sibling tests above. **Mutation:** swap two
    /// fields in `ViewDefinition`'s `struct_codec!` declaration
    /// (`core/src/codec.rs:1792`, `{ id, name, active_layers }` →
    /// `{ id, active_layers, name }`); this literal-byte vector must fail
    /// while every round-trip test stays green.
    #[test]
    fn create_view_envelope_decode_vector_is_pinned_to_literal_bytes() {
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
            0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            38, 54, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 6, 0, 0,
            0, 118, 105, 101, 119, 45, 49, 1, 0, 0, 0, 16, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
            0, 0, 1,
        ];
        let result = check("ops.operation_envelope", &bytes)
            .expect("ops.operation_envelope is owned by this crate");
        assert_eq!(
            result,
            Ok(true),
            "the committed literal bytes must decode and re-encode injectively"
        );
    }
}
