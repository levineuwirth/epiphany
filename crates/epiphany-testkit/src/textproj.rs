//! End-to-end semantic preservation for the Text Projection document layer.
//!
//! This is the weaker `req:textproj:roundtrip` equation over generated real
//! operations:
//!
//! `semantics(parse(project(B))) == semantics(B)`.
//!
//! It compares canonical reducer bytes, never bundle bytes. A text round trip
//! intentionally regenerates layout, collapses duplicate blobs, and omits
//! non-canonical accelerators; none of those changes operation semantics.

use epiphany_bundle::{DocumentId, FileUuid, MemStore, ProfileDeclaration};
use epiphany_ops::{OperationEnvelope, OperationSet};
use epiphany_textproj::parse::parse_document;
use epiphany_textproj::project::{document_from_bundle, project_bundle};
use epiphany_textproj::serialize::serialize_document;
use epiphany_textproj::TextDocument;

use crate::{generators, Rng};

fn reduced_bytes(envelopes: &[OperationEnvelope]) -> Vec<u8> {
    let mut operations = OperationSet::new();
    operations.accept_all(envelopes.iter().cloned());
    operations.reduce().canonical_bytes()
}

/// Generates a bundle carrying real operations and checks the semantic
/// projection equation by reducing both envelope sets to canonical bytes.
pub fn assert_semantics_preserved(seed: u64) {
    let mut rng = Rng::new(seed);
    let envelopes = generators::operation_envelopes(&mut rng, 24, 3, 8, 8);
    assert!(
        !envelopes.is_empty(),
        "the generated bundle carries operations"
    );

    let source = TextDocument {
        document_id: DocumentId(seed.to_le_bytes().repeat(2).try_into().expect("16 bytes")),
        lineage_id: None,
        profiles: vec![ProfileDeclaration::full()],
        extensions: Vec::new(),
        canonical_base: None,
        blobs: Vec::new(),
        envelopes,
    };
    let bundle = serialize_document(&source, MemStore::new(), FileUuid([0x7E; 16]))
        .unwrap_or_else(|error| panic!("seed {seed}: serializing generated operations: {error}"));

    let bundle_document = document_from_bundle(&bundle)
        .unwrap_or_else(|error| panic!("seed {seed}: reading source bundle: {error}"));
    assert!(
        !bundle_document.envelopes.is_empty(),
        "seed {seed}: source bundle lost every operation"
    );
    let text = project_bundle(&bundle)
        .unwrap_or_else(|error| panic!("seed {seed}: projecting source bundle: {error}"));
    let parsed = parse_document(&text)
        .unwrap_or_else(|error| panic!("seed {seed}: parsing its projection: {error}"));

    // `semantics(B)` must be computed WITHOUT going through the projection.
    // Comparing against `bundle_document` alone would not: `project_bundle` is
    // `project_text_document(document_from_bundle(..))`, so both sides would flow
    // through `document_from_bundle` and any bug in it — dropping an envelope,
    // reordering, corrupting one — would cancel out. `source.envelopes` is the
    // independent reference: this test built it and serialized it in, and
    // `reduced_bytes` reduces through an `OperationSet`, which imposes canonical
    // order itself, so the input order does not matter.
    assert_eq!(
        reduced_bytes(&parsed.envelopes),
        reduced_bytes(&source.envelopes),
        "seed {seed}: semantics(parse(project(B))) != semantics(B)"
    );
    // And the read-back path agrees with the same independent reference.
    assert_eq!(
        reduced_bytes(&bundle_document.envelopes),
        reduced_bytes(&source.envelopes),
        "seed {seed}: reading the bundle back changed its semantics"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_operation_bundles_preserve_reduced_semantics_not_bundle_identity() {
        for seed in 0..16 {
            assert_semantics_preserved(seed);
        }
    }
}
