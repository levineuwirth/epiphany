//! The supported-layout boundary (roadmap D4): the stamp a reader holds an
//! operation block to is the stamp every writer derives, and the decode
//! corpus's block vectors are framed as the bundle frames a block.
//!
//! `epiphany-ops` derives a block's stamp itself ([`BlockStamp::derive`]),
//! since it does not depend on `epiphany-bundle`, whose
//! `SchemaVersion::for_major_at_epoch` the writers use. Each implements the
//! rule; this file drives the same envelopes through both.

use std::collections::BTreeSet;

use epiphany_bundle::{decode_block, SchemaVersion};
use epiphany_determinism::fuzz::SplitMix64;
use epiphany_ops::fuzz::gen_envelope_set;
use epiphany_ops::{operation_block_introduced_minor, BlockStamp, OperationEnvelope};
use epiphany_testkit::bundle_harness::stage_operation_block;

#[test]
fn the_reader_s_stamp_is_every_writer_s_stamp() {
    let mut rng = SplitMix64::new(0x5B2A);
    let mut reached = BTreeSet::new();
    for len in [0, 1, 2, 3, 5, 8, 13, 24] {
        for _ in 0..200 {
            let envelopes = gen_envelope_set(&mut rng, len);
            let major = envelopes
                .iter()
                .map(OperationEnvelope::schema_major)
                .max()
                .unwrap_or(0);
            let writer = SchemaVersion::for_major_at_epoch(
                major,
                operation_block_introduced_minor(&envelopes),
            );
            let reader = BlockStamp::derive(&envelopes);
            assert_eq!(
                (writer.major, writer.minor),
                (reader.major, reader.minor),
                "{len} envelopes"
            );
            assert_eq!(stage_operation_block(&envelopes).schema_version, writer);
            reached.insert(reader);
        }
    }
    // The generator reaches the baselines and raised minors of several majors.
    for (major, minor) in [(0, 1), (2, 0)] {
        assert!(
            reached.contains(&BlockStamp { major, minor }),
            "{major}.{minor} not reached: {reached:?}"
        );
    }
    assert!(
        reached.iter().any(|s| s.major == 0 && s.minor > 1)
            && reached.iter().any(|s| s.major > 0 && s.minor > 0),
        "no raised minor reached: {reached:?}"
    );
}

#[test]
fn the_block_vectors_are_framed_as_the_bundle_frames_a_block() {
    let mut seen = 0;
    for (surface, _, class, name, bytes) in epiphany_ops::vectors::decode_vectors() {
        if surface != "ops.operation_block" {
            continue;
        }
        seen += 1;
        let framed = decode_block(&bytes[4..]);
        if class == "trailing-bytes" {
            assert!(framed.is_err(), "{name}: the bundle accepts its framing");
        } else {
            assert!(framed.is_ok(), "{name}: {framed:?}");
        }
    }
    assert!(seen >= 5, "only {seen} block vectors");
}
