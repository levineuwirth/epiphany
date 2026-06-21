//! The canonical round-trip harness (QUICKSTART, Agent F):
//!
//! > the canonical round-trip harness (serialize → bytes → deserialize → assert
//! > byte-identical re-serialization)
//!
//! This is v0 acceptance criterion 4 (canonical serialization stability), which
//! tests Appendix D's canonical-serialization layer. All tiers are real (A, B,
//! C, and D have shipped):
//!
//! 1. [`assert_roundtrip`] — the generic property over any
//!    [`CanonicalEncode`] + [`CanonicalDecode`] value:
//!    `decode(encode(x)) == x` and `encode(decode(encode(x))) == encode(x)`.
//!    [`run_roundtrip_corpus`] sweeps it across **every** canonical-serialized
//!    public type in A and B (all typed identifiers, both `RationalTime` arms,
//!    every `TypedObjectId` discriminant, the time types).
//! 2. [`assert_manifest_roundtrip`] — the real bundle [`Manifest`], plus the
//!    [`FixedHeader`] and [`Superblock`] slot encodings, round-tripped. The
//!    manifest is exercised with [`crate::generators::rich_manifest`], so
//!    snapshots, blobs, extensions, profiles, retention, and the optional
//!    roots — not just `operation_roots` — are covered.
//! 3. [`assert_reduction_serialization_stable`] — a score's canonical state: an
//!    [`epiphany_ops::OperationSet`] is reduced to its canonical
//!    [`epiphany_ops::MaterializedState`]'s `canonical_bytes` (the canonical
//!    serialized score state), which survives content-addressed storage in a real
//!    bundle, decodes back into the same materialized state, and re-serializes
//!    byte-identically. Musical sensitivity is
//!    proven by [`assert_content_mutation_changes_serialization`] (same
//!    identities, changed content → different bytes) and
//!    [`assert_distinct_scores_serialize_differently`].

use std::fmt::Debug;

use epiphany_bundle::{
    Bundle, ChunkKind, CommitContext, DocumentId, FileUuid, FixedHeader, FrontierBytes, Manifest,
    MemStore, ProfileId, ReductionAlgorithmVersion, SchemaVersion, SlotParse, SnapshotId,
    SnapshotRef, StagedChunk, Superblock,
};
use epiphany_core::Score;
use epiphany_determinism::{CanonicalDecode, CanonicalEncode};
use epiphany_ops::{MaterializedState, OperationEnvelope, OperationSet};

use crate::generators;
use crate::rng::Rng;

/// The generic round-trip property. Returns the canonical bytes so callers can
/// sanity-check widths. Panics on any violation.
pub fn assert_roundtrip<T>(value: &T) -> Vec<u8>
where
    T: CanonicalEncode + CanonicalDecode + PartialEq + Debug,
{
    let bytes = value.to_canonical_bytes();
    let decoded =
        T::decode_canonical(&bytes).unwrap_or_else(|e| panic!("decode of {value:?} failed: {e}"));
    assert_eq!(&decoded, value, "round-trip changed the value: {value:?}");
    let re_encoded = decoded.to_canonical_bytes();
    assert_eq!(
        re_encoded, bytes,
        "re-encode not byte-identical for {value:?}"
    );
    bytes
}

/// Sweeps [`assert_roundtrip`] over every canonical-serialized public type in
/// Agents A and B, drawing `iters` random values from `seed`. This is the
/// type-level half of acceptance criterion 4.
pub fn run_roundtrip_corpus(iters: u64, seed: u64) {
    let mut rng = Rng::new(seed);
    for _ in 0..iters {
        match rng.below(43) {
            // --- Agent A: epiphany-determinism ---
            0 => drop(assert_roundtrip(&generators::quantized_coord(&mut rng))),
            1 => drop(assert_roundtrip(&generators::canonical_f64(&mut rng))),
            2 => drop(assert_roundtrip(&generators::content_hash(&mut rng))),
            3 => drop(assert_roundtrip(&generators::chunk_id_gen(&mut rng))),
            4 => drop(assert_roundtrip(&generators::domain_tag(&mut rng))),
            // --- Agent B: the full typed-identifier family ---
            5 => drop(assert_roundtrip(&generators::event_id(&mut rng))),
            6 => drop(assert_roundtrip(&generators::pitch_id(&mut rng))),
            7 => drop(assert_roundtrip(&generators::voice_id(&mut rng))),
            8 => drop(assert_roundtrip(&generators::staff_id(&mut rng))),
            9 => drop(assert_roundtrip(&generators::staff_instance_id(&mut rng))),
            10 => drop(assert_roundtrip(&generators::staff_group_id(&mut rng))),
            11 => drop(assert_roundtrip(&generators::region_id(&mut rng))),
            12 => drop(assert_roundtrip(&generators::instrument_id(&mut rng))),
            13 => drop(assert_roundtrip(&generators::part_definition_id(&mut rng))),
            14 => drop(assert_roundtrip(&generators::measure_id(&mut rng))),
            15 => drop(assert_roundtrip(&generators::barline_alignment_group_id(
                &mut rng,
            ))),
            16 => drop(assert_roundtrip(&generators::tuplet_id(&mut rng))),
            17 => drop(assert_roundtrip(&generators::slur_id(&mut rng))),
            18 => drop(assert_roundtrip(&generators::tie_id(&mut rng))),
            19 => drop(assert_roundtrip(&generators::beam_id(&mut rng))),
            20 => drop(assert_roundtrip(&generators::spanner_id(&mut rng))),
            21 => drop(assert_roundtrip(&generators::marker_id(&mut rng))),
            22 => drop(assert_roundtrip(&generators::analytical_annotation_id(
                &mut rng,
            ))),
            23 => drop(assert_roundtrip(&generators::comment_id(&mut rng))),
            24 => drop(assert_roundtrip(&generators::time_signature_id(&mut rng))),
            25 => drop(assert_roundtrip(&generators::analysis_layer_id(&mut rng))),
            26 => drop(assert_roundtrip(&generators::repeat_structure_id(&mut rng))),
            27 => drop(assert_roundtrip(&generators::lyric_line_id(&mut rng))),
            28 => drop(assert_roundtrip(&generators::chord_symbol_id(&mut rng))),
            29 => drop(assert_roundtrip(&generators::operation_id(&mut rng))),
            // The tagged union over the whole family (every discriminant + Registered).
            30 => drop(assert_roundtrip(&generators::typed_object_id(&mut rng))),
            31 => drop(assert_roundtrip(&generators::graphic_object_id(&mut rng))),
            32 => drop(assert_roundtrip(&generators::graphic_gesture_id(&mut rng))),
            33 => drop(assert_roundtrip(&generators::view_id(&mut rng))),
            34 => drop(assert_roundtrip(&generators::object_kind_registry_id(
                &mut rng,
            ))),
            35 => drop(assert_roundtrip(&generators::replica_id(&mut rng))),
            36 => drop(assert_roundtrip(&generators::transaction_id(&mut rng))),
            37 => drop(assert_roundtrip(&generators::integrity_anomaly_id(
                &mut rng,
            ))),
            // --- Agent B: time (both RationalTime arms via the generator) ---
            38 => drop(assert_roundtrip(&generators::rational_time(&mut rng))),
            39 => drop(assert_roundtrip(&generators::musical_position(&mut rng))),
            40 => drop(assert_roundtrip(&generators::musical_duration(&mut rng))),
            41 => drop(assert_roundtrip(&generators::wallclock_time(&mut rng))),
            _ => drop(assert_roundtrip(&generators::wallclock_duration(&mut rng))),
        }
    }
}

/// Runs Agent A's own 1,000,000-iteration determinism round-trip gate (the
/// QUICKSTART hand-off gate), re-exposed here so the whole conformance suite has
/// a single entry point.
pub fn run_determinism_roundtrip_gate(iters: u64, seed: u64) {
    epiphany_determinism::fuzz::run_round_trip_fuzz(iters, seed);
}

/// The commit-context closure used to advance a bundle: append the commit's new
/// chunks to the previous manifest's `operation_roots`.
fn append_roots(ctx: &CommitContext) -> Manifest {
    let mut m = ctx.previous_manifest.clone();
    m.operation_roots.extend(ctx.new_chunks.iter().copied());
    m
}

/// Asserts the real bundle manifest serialization round-trips byte-stably:
/// `encode → decode → encode` is byte-identical and `decode` is a fixpoint.
/// (The manifest's reference vectors are put into canonical order at encode time,
/// so this is the bundle layer's statement of criterion 4.)
pub fn assert_manifest_roundtrip(manifest: &Manifest) {
    let bytes = manifest.encode();
    let decoded = Manifest::decode(&bytes).expect("manifest must decode");
    let re_encoded = decoded.encode();
    assert_eq!(
        bytes, re_encoded,
        "manifest re-encode not byte-identical (criterion 4)"
    );
    let decoded2 = Manifest::decode(&re_encoded).expect("re-decode");
    assert_eq!(decoded, decoded2, "manifest decode is not a fixpoint");
}

/// Asserts the [`FixedHeader`] round-trips: `decode(encode(h)) == h` and the
/// re-encode is byte-identical.
pub fn assert_header_roundtrip(header: &FixedHeader) {
    let bytes = header.encode();
    let decoded = FixedHeader::decode(&bytes).expect("header decodes");
    assert_eq!(&decoded, header, "header round-trip changed the value");
    assert_eq!(
        decoded.encode(),
        bytes,
        "header re-encode not byte-identical"
    );
}

/// Asserts a committed [`Superblock`] round-trips through its 256-byte slot
/// encoding via [`Superblock::parse_slot`].
pub fn assert_superblock_roundtrip(sb: &Superblock) {
    let bytes = sb.encode();
    match Superblock::parse_slot(&bytes) {
        SlotParse::Valid(parsed) => {
            assert_eq!(&parsed, sb, "superblock round-trip changed the value");
            assert_eq!(
                parsed.encode(),
                bytes,
                "superblock re-encode not byte-identical"
            );
        }
        SlotParse::Rejected(reject) => {
            panic!("a committed superblock must parse as Valid, got {reject:?}")
        }
    }
}

/// Builds a non-trivial manifest by driving a real bundle through several
/// commits, then returns it.
pub fn committed_manifest(seed: u64) -> Manifest {
    let mut rng = Rng::new(seed);
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    for _ in 0..3 {
        let n = rng.range_usize(1, 3);
        let payloads: Vec<Vec<u8>> = (0..n).map(|_| rng.byte_vec(1, 80)).collect();
        let blocks: Vec<StagedChunk> = epiphany_bundle::pack_operation_blocks(&payloads)
            .into_iter()
            .map(StagedChunk::operation_block)
            .collect();
        bundle.commit(&blocks, append_roots).expect("commit");
    }
    bundle.manifest().clone()
}

/// Reduces `envelopes` to the canonical serialized score state (Chapter 6: the
/// materialized graph is a deterministic reduction of the operation set).
fn canonical_score_state(envelopes: &[OperationEnvelope]) -> MaterializedState {
    let mut set = OperationSet::new();
    set.accept_all(envelopes.iter().cloned());
    set.reduce()
}

fn canonical_score_bytes(envelopes: &[OperationEnvelope]) -> Vec<u8> {
    canonical_score_state(envelopes).canonical_bytes()
}

/// Reduction-serialization stability for a **score's canonical state**
/// (acceptance criterion 4): the operation
/// set reduces to canonical bytes; re-reducing the same set yields byte-identical
/// bytes; and those bytes survive content-addressed storage in a real bundle —
/// stored as a `Snapshot` chunk referenced by the manifest's `canonical_base`
/// (its correct semantic home), hash-verified on reopen and read back
/// byte-identically.
///
/// The snapshot's `covers_causal_frontier` is the frontier the snapshot actually
/// materializes ([`crate::generators::frontier_bytes`] over the reduced
/// envelopes), so it is semantically consistent — not a falsely-empty frontier
/// that would invite a replay layer to reapply already-materialized effects.
///
/// After reopen, the snapshot payload is decoded through
/// [`MaterializedState::decode_canonical`], compared structurally with the
/// pre-storage reduction, and re-serialized byte-identically. This is the real
/// serialize → load → deserialize → reserialize cycle required by criterion 4.
pub fn assert_reduction_serialization_stable(envelopes: &[OperationEnvelope], seed: u64) {
    let state = canonical_score_state(envelopes);
    let canonical = state.canonical_bytes();
    // re-reduce the same operation set: byte-identical canonical state.
    assert_eq!(
        canonical,
        canonical_score_bytes(envelopes),
        "re-reduction changed the canonical score bytes"
    );

    // serialize: stage the canonical state as a real **Snapshot** chunk and
    // reference it from the manifest's `canonical_base` — its correct semantic
    // home (a materialized snapshot), with the right chunk kind.
    let mut rng = Rng::new(seed);
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    let snapshot = StagedChunk {
        kind: ChunkKind::Snapshot,
        schema_version: SchemaVersion::V0,
        payload: canonical.clone(),
    };
    bundle
        .commit(&[snapshot], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            let root = ctx.new_chunks[0];
            let mut sid = [0u8; 16];
            sid.copy_from_slice(&root.hash.as_bytes()[..16]);
            m.canonical_base = Some(SnapshotRef {
                snapshot_id: SnapshotId(sid),
                // The frontier the snapshot actually materializes (covering every
                // reduced envelope), not a falsely-empty one.
                covers_causal_frontier: FrontierBytes::from_bytes(generators::frontier_bytes(
                    envelopes,
                )),
                reduction_algorithm_version: ReductionAlgorithmVersion(0),
                profile_id: ProfileId::Full,
                hash: root.hash,
                root,
            });
            m
        })
        .expect("commit snapshot");
    let image = bundle.into_store().into_bytes();

    // load: reopen from exactly those bytes; the snapshot chunk is hash-verified
    // on open and read back byte-identically.
    let reopened = Bundle::open(MemStore::from_bytes(image)).expect("reopen bundle");
    reopened
        .verify_canonical_chunks()
        .expect("canonical chunks intact");
    let base = reopened
        .manifest()
        .canonical_base
        .as_ref()
        .expect("a canonical base");
    let loaded = reopened
        .read_chunk(&base.root)
        .expect("read snapshot chunk back");
    assert_eq!(
        loaded, canonical,
        "canonical state was not preserved through content-addressed storage"
    );
    let decoded = MaterializedState::decode_canonical(&loaded)
        .expect("loaded materialized snapshot must decode");
    assert_eq!(decoded, state, "decoded materialized state changed");
    assert_eq!(
        decoded.canonical_bytes(),
        loaded,
        "decoded snapshot did not reserialize byte-identically"
    );

    // The reopened bundle's manifest is itself a real decode→reencode fixpoint.
    assert_manifest_roundtrip(reopened.manifest());
}

/// **Full-`Score` canonical serialization stability** (acceptance criterion 4,
/// the whole-graph tier — item 5's whole-score codec). The real
/// [`epiphany_core::Score`] encodes to canonical bytes, survives
/// content-addressed storage as a `Snapshot` chunk in a real bundle
/// (hash-verified on reopen), decodes back to an **equal** `Score`, and
/// re-encodes byte-identically. Unlike [`assert_reduction_serialization_stable`]
/// (which round-trips the Chapter 6 bookkeeping projection), this round-trips the
/// whole musical graph — the arena, voices, regions, cross-cutting, and
/// tombstones — through [`Score::canonical_bytes`] / [`Score::decode_canonical`].
///
/// `frontier` is the causal frontier the snapshot materializes (so the snapshot
/// reference is semantically consistent, not falsely empty).
pub fn assert_score_serialization_stable(score: &Score, frontier: &[u8], seed: u64) {
    let canonical = score.canonical_bytes();
    // Determinism: re-encoding the same score is byte-identical.
    assert_eq!(
        canonical,
        score.canonical_bytes(),
        "re-encoding the same score changed its bytes"
    );

    // serialize: stage the canonical score as a real Snapshot chunk referenced
    // from the manifest's canonical_base.
    let mut rng = Rng::new(seed);
    let uuid = FileUuid(rng.array16());
    let doc = DocumentId(rng.array16());
    let mut bundle =
        Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create bundle");
    let snapshot = StagedChunk {
        kind: ChunkKind::Snapshot,
        schema_version: SchemaVersion::V0,
        payload: canonical.clone(),
    };
    let frontier = frontier.to_vec();
    bundle
        .commit(&[snapshot], |ctx| {
            let mut m = ctx.previous_manifest.clone();
            let root = ctx.new_chunks[0];
            let mut sid = [0u8; 16];
            sid.copy_from_slice(&root.hash.as_bytes()[..16]);
            m.canonical_base = Some(SnapshotRef {
                snapshot_id: SnapshotId(sid),
                covers_causal_frontier: FrontierBytes::from_bytes(frontier.clone()),
                reduction_algorithm_version: ReductionAlgorithmVersion(0),
                profile_id: ProfileId::Full,
                hash: root.hash,
                root,
            });
            m
        })
        .expect("commit snapshot");
    let image = bundle.into_store().into_bytes();

    // load: reopen, hash-verify, read back byte-identically.
    let reopened = Bundle::open(MemStore::from_bytes(image)).expect("reopen bundle");
    reopened
        .verify_canonical_chunks()
        .expect("canonical chunks intact");
    let base = reopened
        .manifest()
        .canonical_base
        .as_ref()
        .expect("a canonical base");
    let loaded = reopened
        .read_chunk(&base.root)
        .expect("read snapshot chunk back");
    assert_eq!(
        loaded, canonical,
        "score bytes were not preserved through content-addressed storage"
    );

    // deserialize → equal → reserialize byte-identically.
    let decoded = Score::decode_canonical(&loaded).expect("loaded score must decode");
    assert_eq!(&decoded, score, "decoded score changed");
    assert_eq!(
        decoded.canonical_bytes(),
        loaded,
        "decoded score did not reserialize byte-identically"
    );
}

/// Confirms criterion 4 is *musically sensitive* in the strongest form: a score
/// whose operations keep **identical identities and ordering metadata** but whose
/// payload *content* changes must reduce to **different** canonical bytes. This
/// is the exact rebuttal to an id-only "serializer" that would collapse distinct
/// scores: the ids/stamps/causal contexts are byte-for-byte the same, so only the
/// content differs.
pub fn assert_content_mutation_changes_serialization() {
    let (base, mutated) = generators::content_mutation_pair();

    // The operation identities and ordering metadata are byte-for-byte identical;
    // only one payload's *content* differs.
    assert_eq!(base.len(), mutated.len());
    for (b, m) in base.iter().zip(&mutated) {
        assert_eq!(b.id, m.id, "operation identity changed");
        assert_eq!(b.stamp, m.stamp, "operation stamp changed");
        assert_eq!(
            b.causal_context, m.causal_context,
            "operation causal context changed"
        );
    }
    let differing = base
        .iter()
        .zip(&mutated)
        .filter(|(b, m)| b.payload != m.payload)
        .count();
    assert_eq!(differing, 1, "exactly one payload's content should differ");

    assert_ne!(
        canonical_score_bytes(&base),
        canonical_score_bytes(&mutated),
        "changing operation content (with identities held fixed) must change the canonical bytes"
    );
}

/// Confirms two independently-generated operation sets reduce to different
/// canonical bytes (a coarse sensitivity check; the strong form is
/// [`assert_content_mutation_changes_serialization`]).
pub fn assert_distinct_scores_serialize_differently(
    a: &[OperationEnvelope],
    b: &[OperationEnvelope],
) {
    assert_ne!(
        canonical_score_bytes(a),
        canonical_score_bytes(b),
        "distinct operation sets must reduce to distinct canonical bytes"
    );
}

/// Asserts the real `Manifest` **decoder** rejects corrupted bytes — exercising
/// the decode/canonicalization validation path, not just the happy round-trip.
pub fn assert_manifest_decode_rejects_corruption(manifest: &Manifest) {
    let bytes = manifest.encode();
    assert!(bytes.len() > 4);
    // Flip a byte in the body: the stored manifest id will no longer match the
    // id re-derived from the (corrupted) body, so decode must reject it.
    let mut corrupt = bytes.clone();
    let i = corrupt.len() / 2;
    corrupt[i] ^= 0xFF;
    assert!(
        Manifest::decode(&corrupt).is_err(),
        "a corrupted manifest must be rejected by the decoder"
    );
}

/// Asserts the real `FixedHeader` decoder rejects a corrupted header (CRC).
pub fn assert_header_decode_rejects_corruption(header: &FixedHeader) {
    let mut bytes = header.encode().to_vec();
    bytes[8] ^= 0xFF; // a byte inside the CRC-covered region
    assert!(
        FixedHeader::decode(&bytes).is_err(),
        "a corrupted header must be rejected by the decoder"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_bundle::encode_block;

    #[test]
    fn corpus_round_trips() {
        run_roundtrip_corpus(60_000, 0x00C0_FFEE_1234_5678);
    }

    #[test]
    fn manifest_round_trips_and_rejects_corruption() {
        for seed in 0..64u64 {
            let m = committed_manifest(seed.wrapping_mul(0x9E37_79B9).wrapping_add(3));
            assert_manifest_roundtrip(&m);
            // The rich manifest: every optional field and reference vector.
            let mut rng = Rng::new(seed.wrapping_mul(0x100_0193).wrapping_add(17));
            let rich = generators::rich_manifest(&mut rng);
            assert_manifest_roundtrip(&rich);
            // The decoder rejects corruption (canonicalization/validation path).
            assert_manifest_decode_rejects_corruption(&rich);
        }
    }

    #[test]
    fn header_and_superblock_round_trip() {
        let mut rng = Rng::new(0x4845_4144); // "HEAD"
        for _ in 0..64 {
            let header = FixedHeader::new(FileUuid(rng.array16()));
            assert_header_roundtrip(&header);
            assert_header_decode_rejects_corruption(&header);
        }
        // A real committed superblock from a live bundle.
        let uuid = FileUuid(rng.array16());
        let doc = DocumentId(rng.array16());
        let mut bundle =
            Bundle::create(MemStore::new(), uuid, Manifest::empty(doc)).expect("create");
        bundle
            .commit(
                &[StagedChunk::operation_block(encode_block(&[vec![1u8; 8]]))],
                append_roots,
            )
            .expect("commit");
        assert_header_roundtrip(bundle.header());
        assert_superblock_roundtrip(bundle.superblock());
        // Generated committed superblocks round-trip through the slot encoding.
        for _ in 0..32 {
            assert_superblock_roundtrip(&generators::superblock(&mut rng));
        }
    }

    #[test]
    fn scores_serialize_stably_and_distinctly() {
        for seed in 0..48u64 {
            let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9));
            let score = generators::operation_envelopes(&mut rng, 40, 3, 6, 6);
            assert_reduction_serialization_stable(&score, seed);
            let other = generators::operation_envelopes(&mut rng, 41, 3, 6, 6);
            assert_distinct_scores_serialize_differently(&score, &other);
        }
        // Strong sensitivity: same identities, changed content → different bytes.
        assert_content_mutation_changes_serialization();
    }
}
