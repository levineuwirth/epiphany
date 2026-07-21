//! Serialization of parsed Text Projection documents into canonical bundles.
//!
//! [`serialize_document`] stages the payloads a [`TextDocument`] carries inline
//! — the canonical base's root chunk, every extension's preserved chunks, and
//! one operation-envelope block — and lets [`Bundle::create`] /
//! [`Bundle::commit`] do everything `req:textproj:derive-or-carry` says a
//! serializer must not: assign offsets, content-address every chunk, and
//! de-duplicate identical content. **Nothing here computes a `ChunkId`, a
//! `ContentHash`, or an offset** — every physical field written into the
//! manifest comes straight from the [`ChunkRef`](epiphany_bundle::ChunkRef)s
//! `commit` hands back.
//!
//! # Block splitting is a free physical choice
//!
//! `req:textproj:roundtrip` only requires the *bundle's* physical layout to
//! round-trip in the second, byte-checkable equation (`project(serialize(parse(T)))
//! == T`, quantified over texts, not bundles); how a serializer packs envelopes
//! into blocks is unconstrained. This module always emits **exactly one**
//! operation-envelope block, containing every envelope the document carries (in
//! the order the document carries them) — the simplest possible choice, and
//! sufficient because block boundaries are storage artifacts, never semantic
//! structure (Chapter 8: *"the set of envelopes is the union of all envelopes
//! across all referenced blocks"*).
//!
//! # No accelerator is (re)written
//!
//! A [`TextDocument`] carries no `operation_index_root`, `acceleration_snapshots`,
//! `text_projection_root`, `integrity_root`, or `operation_block_summaries` —
//! they are non-canonical and the companion text does not carry them
//! (`req:textproj:derive-or-carry`). The manifest this module builds leaves every
//! one of those fields at its empty/`None` default. A bundle serialized from a
//! `TextDocument` therefore comes back from this module *without* any
//! accelerator a previous generation might have had. That reads as data loss; it
//! is not — none of those fields contributes to canonical document semantics,
//! and a consumer that wants them back rebuilds them the same way any bundle
//! writer does (e.g. `epiphany_bundle::fuzz` and the testkit's operation-index
//! harness both scan-and-rebuild rather than trust a carried-forward index).
//!
//! # No blob is staged
//!
//! [`TextDocument::blobs`] is never staged into `manifest.blob_roots` here. Today
//! that vector is always empty: no operation payload or Chapter-5 value in
//! `epiphany-core`/`epiphany-ops` carries a `BlobId`, so no blob is reachable from
//! canonical state (`req:textproj:canonical-blobs`), and this companion's own
//! parser rejects every `(blob ...)` line rather than populate the vector (see
//! `parse`). [`serialize_document`] still checks: a `TextDocument` assembled
//! directly against the type (bypassing `parse`) with a populated `blobs` vector
//! is refused with [`SerializeError::NonEmptyBlobs`] rather than silently
//! dropped — wiring a blob into the manifest here would require a canonical
//! `declared_max_uncompressed_length`/compression policy this companion version
//! does not specify, and silently discarding the caller's bytes is worse.

use std::fmt;

use epiphany_bundle::{
    encode_block, BlockStore, Bundle, BundleError, ChunkKind, CommitContext, ExtensionDeclaration,
    FileUuid, Manifest, SchemaVersion, SnapshotRef, StagedChunk,
};
use epiphany_determinism::CanonicalEncode;
use epiphany_ops::OperationEnvelope;

use crate::TextDocument;

/// An error [`serialize_document`] cannot recover from.
#[derive(Debug)]
pub enum SerializeError {
    /// The document's `blobs` vector is non-empty. See the module documentation:
    /// no blob is canonical today, so a document built by this companion's
    /// `parse` never carries one, and staging one here would either wire a
    /// non-canonical root into the manifest or silently drop the caller's
    /// payload bytes. Neither is acceptable, so serialization refuses instead.
    NonEmptyBlobs,
    /// The bundle itself rejected the manifest or a staged chunk — e.g. the
    /// document declares no profile this implementation understands, or a
    /// staged root fails `Bundle::commit`'s structural validation.
    Bundle(BundleError),
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::NonEmptyBlobs => f.write_str(
                "the document carries a populated blobs vector, but no blob can be canonical today",
            ),
            SerializeError::Bundle(error) => {
                write!(f, "the bundle rejected the serialized document: {error}")
            }
        }
    }
}

impl std::error::Error for SerializeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SerializeError::Bundle(error) => Some(error),
            SerializeError::NonEmptyBlobs => None,
        }
    }
}

impl From<BundleError> for SerializeError {
    fn from(error: BundleError) -> Self {
        SerializeError::Bundle(error)
    }
}

/// Serializes a [`TextDocument`] into a freshly created bundle over `store`.
///
/// Two-phase, because [`Bundle::create`] requires a manifest with no canonical
/// roots or blobs (there is nothing to reference before any chunk is written):
/// this creates an empty generation-0 bundle carrying only the document's
/// identity and profile declarations, then stages every payload the document
/// carries inline and commits once, building the real manifest from the
/// [`ChunkRef`](epiphany_bundle::ChunkRef)s that commit assigns.
///
/// Returns [`SerializeError::NonEmptyBlobs`] if `document.blobs` is non-empty
/// (see the module documentation), or [`SerializeError::Bundle`] if the bundle
/// itself refuses the manifest or a staged root.
pub fn serialize_document<S: BlockStore>(
    document: &TextDocument,
    store: S,
    file_uuid: FileUuid,
) -> Result<Bundle<S>, SerializeError> {
    if !document.blobs.is_empty() {
        return Err(SerializeError::NonEmptyBlobs);
    }

    let mut bundle = Bundle::create(store, file_uuid, empty_manifest(document))?;

    let mut staged = Vec::new();
    if let Some(base) = &document.canonical_base {
        // The canonical base's root chunk: kind and schema version are carried
        // verbatim from the document (they are not derivable from anything),
        // and its payload is staged exactly as the document carries it. `commit`
        // computes the chunk's id, hash, and offset.
        staged.push(StagedChunk {
            kind: ChunkKind::Snapshot,
            schema_version: base.root_schema_version,
            payload: base.root_payload.clone(),
        });
    }
    for extension in &document.extensions {
        for chunk in &extension.chunks {
            staged.push(StagedChunk {
                kind: chunk.kind,
                schema_version: chunk.schema_version,
                payload: chunk.payload.clone(),
            });
        }
    }
    staged.push(stage_operation_envelope_block(document));

    bundle.commit(&staged, |ctx| build_manifest(document, ctx))?;
    Ok(bundle)
}

/// The manifest [`Bundle::create`] is given: the document's identity and
/// declared profiles, and nothing else. `create` itself rejects a manifest
/// carrying canonical roots or blobs, so every root is added by the subsequent
/// commit (see [`build_manifest`]). If `document.profiles` is empty or
/// otherwise unemittable, `Bundle::create` reports that itself — this function
/// does not invent a fallback profile the document did not declare.
fn empty_manifest(document: &TextDocument) -> Manifest {
    let mut manifest = Manifest::empty(document.document_id);
    manifest.lineage_id = document.lineage_id;
    manifest.profile_declarations = document.profiles.clone();
    manifest
}

/// Encodes every envelope the document carries, in the order it carries them,
/// into a single operation-envelope block payload (see the module
/// documentation on why one block is the right choice here). The block's
/// schema version is the max over its envelopes' `schema_major` — never a fixed
/// baseline — so a block that carries a higher-major payload (e.g. a v1
/// `CreateRegion` or a v2 cross-cutting value) is never mis-stamped major 0,
/// mirroring `StagedChunk::operation_block_versioned`'s own contract.
fn stage_operation_envelope_block(document: &TextDocument) -> StagedChunk {
    let payloads: Vec<Vec<u8>> = document
        .envelopes
        .iter()
        .map(CanonicalEncode::to_canonical_bytes)
        .collect();
    let major = document
        .envelopes
        .iter()
        .map(OperationEnvelope::schema_major)
        .max()
        .unwrap_or(0);
    StagedChunk::operation_block_versioned(encode_block(&payloads), SchemaVersion::for_major(major))
}

/// Builds the committed manifest from the previous (empty) manifest and the
/// [`ChunkRef`](epiphany_bundle::ChunkRef)s [`Bundle::commit`] assigned to the
/// chunks [`serialize_document`] staged, in the same order it staged them: the
/// canonical base's root (if any), then each extension's preserved chunks in
/// turn, then the single operation-envelope block. Every physical field
/// (`ChunkId`, `ContentHash`, offset) comes from `ctx.new_chunks` — nothing here
/// recomputes one (`req:textproj:derive-or-carry`). `operation_index_root`,
/// `acceleration_snapshots`, `text_projection_root`, `integrity_root`,
/// `operation_block_summaries`, and `blob_roots` are left untouched at the
/// previous manifest's empty defaults; see the module documentation for why
/// that is correct rather than lossy.
fn build_manifest(document: &TextDocument, ctx: &CommitContext) -> Manifest {
    let mut manifest = ctx.previous_manifest.clone();
    let mut cursor = 0usize;

    if let Some(base) = &document.canonical_base {
        let root = ctx.new_chunks[cursor];
        cursor += 1;
        manifest.canonical_base = Some(SnapshotRef {
            snapshot_id: base.snapshot_id,
            covers_causal_frontier: base.covers_causal_frontier.clone(),
            reduction_algorithm_version: base.reduction_algorithm_version,
            profile_id: base.profile_id,
            root,
            hash: root.hash,
        });
    }

    let mut extension_declarations = Vec::with_capacity(document.extensions.len());
    for extension in &document.extensions {
        let mut preserved_chunk_roots = Vec::with_capacity(extension.chunks.len());
        for _ in &extension.chunks {
            preserved_chunk_roots.push(ctx.new_chunks[cursor]);
            cursor += 1;
        }
        extension_declarations.push(ExtensionDeclaration {
            extension_id: extension.extension_id,
            version: extension.version,
            required: extension.required,
            preserved_chunk_roots,
            affected_object_kinds: extension.affected_object_kinds.clone(),
            edit_barriers: extension.edit_barriers.clone(),
        });
    }
    manifest.extension_declarations = extension_declarations;

    manifest.operation_roots = vec![ctx.new_chunks[cursor]];
    cursor += 1;
    debug_assert_eq!(
        cursor,
        ctx.new_chunks.len(),
        "every chunk staged by serialize_document must be wired into the manifest exactly once"
    );
    manifest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TextCanonicalBase, TextChunk, TextExtension};
    use epiphany_bundle::{
        DocumentId, ExtensionId, FrontierBytes, LineageId, MemStore, ProfileConstraints,
        ProfileDeclaration, ProfileId, ReductionAlgorithmVersion, SemVer, SnapshotId,
    };
    use epiphany_determinism::fuzz::SplitMix64;
    use epiphany_ops::{decode_envelope, fuzz::gen_envelope_set};

    /// A real, varied envelope set (every operation kind the generator reaches),
    /// so the schema-major derivation and the envelope round trip both have
    /// something to bite on.
    fn envelopes(seed: u64, n: usize) -> Vec<OperationEnvelope> {
        let mut rng = SplitMix64::new(seed);
        gen_envelope_set(&mut rng, n)
    }

    /// A profile declaration distinguishable from `ProfileDeclaration::full()`'s
    /// default version, so a test that forgets to carry `document.profiles`
    /// through cannot pass by accident (`full()` would still be a `Full`
    /// profile, just at a different version).
    fn base_profile() -> ProfileDeclaration {
        ProfileDeclaration {
            profile_id: ProfileId::Full,
            version: SemVer::new(0, 2, 0),
            constraints: ProfileConstraints::DEFAULT_FULL,
        }
    }

    fn minimal_document(seed: u64) -> TextDocument {
        TextDocument {
            document_id: DocumentId([seed as u8; 16]),
            lineage_id: None,
            profiles: vec![base_profile()],
            extensions: Vec::new(),
            canonical_base: None,
            blobs: Vec::new(),
            envelopes: envelopes(seed, 5),
        }
    }

    fn document_with_extension(seed: u64) -> TextDocument {
        let mut document = minimal_document(seed);
        document.extensions.push(TextExtension {
            extension_id: ExtensionId([7; 16]),
            version: SemVer::new(1, 0, 0),
            required: false,
            chunks: vec![
                TextChunk {
                    kind: ChunkKind::ExtensionData,
                    schema_version: SchemaVersion::V0,
                    payload: b"chunk-a".to_vec(),
                },
                TextChunk {
                    kind: ChunkKind::ExtensionData,
                    schema_version: SchemaVersion::V0,
                    payload: b"chunk-b".to_vec(),
                },
            ],
            affected_object_kinds: vec![0xAA],
            edit_barriers: vec![0xBB, 0xCC],
        });
        document
    }

    fn document_with_canonical_base(seed: u64) -> TextDocument {
        let mut document = minimal_document(seed);
        document.canonical_base = Some(TextCanonicalBase {
            snapshot_id: SnapshotId([9; 16]),
            covers_causal_frontier: FrontierBytes(vec![1, 2, 3]),
            reduction_algorithm_version: ReductionAlgorithmVersion(1),
            profile_id: ProfileId::Full,
            root_schema_version: SchemaVersion::V0,
            root_payload: b"snapshot-root".to_vec(),
        });
        document
    }

    /// Carries an extension, a canonical base, and many envelopes at once — the
    /// document every round-trip law needs to be checked against together.
    fn rich_document(seed: u64) -> TextDocument {
        let mut document = document_with_extension(seed);
        document.canonical_base = document_with_canonical_base(seed).canonical_base;
        document.envelopes = envelopes(seed, 60);
        document
    }

    fn serialize_and_reopen(document: &TextDocument) -> Bundle<MemStore> {
        let bundle = serialize_document(document, MemStore::new(), FileUuid([1; 16]))
            .expect("a well-formed document serializes");
        let image = bundle.into_store().into_bytes();
        Bundle::open(MemStore::from_bytes(image)).expect("the serialized bundle reopens")
    }

    #[test]
    fn document_identity_round_trips() {
        let mut document = minimal_document(1);
        document.lineage_id = Some(LineageId([2; 16]));
        let reopened = serialize_and_reopen(&document);
        assert_eq!(reopened.manifest().document_id, document.document_id);
        assert_eq!(reopened.manifest().lineage_id, document.lineage_id);
    }

    #[test]
    fn profile_declarations_round_trip() {
        let document = minimal_document(2);
        let reopened = serialize_and_reopen(&document);
        assert_eq!(reopened.manifest().profile_declarations, document.profiles);
    }

    #[test]
    fn missing_profile_declaration_surfaces_the_bundles_own_rejection() {
        // serialize_document must not invent a fallback profile the document
        // did not declare: an empty `profiles` propagates Bundle::create's own
        // "no declared profile" rejection.
        let mut document = minimal_document(3);
        document.profiles.clear();
        let result = serialize_document(&document, MemStore::new(), FileUuid([1; 16]));
        assert!(matches!(result, Err(SerializeError::Bundle(_))));
    }

    #[test]
    fn canonical_base_round_trips_snapshot_id_and_payload() {
        let document = document_with_canonical_base(4);
        let reopened = serialize_and_reopen(&document);
        let expected = document
            .canonical_base
            .as_ref()
            .expect("fixture carries a canonical base");
        let base = reopened
            .manifest()
            .canonical_base
            .as_ref()
            .expect("canonical base present after reopen");
        assert_eq!(base.snapshot_id, expected.snapshot_id);
        assert_eq!(base.covers_causal_frontier, expected.covers_causal_frontier);
        assert_eq!(
            base.reduction_algorithm_version,
            expected.reduction_algorithm_version
        );
        assert_eq!(base.profile_id, expected.profile_id);
        assert_eq!(base.root.kind, ChunkKind::Snapshot);
        assert_eq!(base.root.schema_version, expected.root_schema_version);
        let payload = reopened
            .read_chunk(&base.root)
            .expect("root chunk reads and verifies");
        assert_eq!(payload, expected.root_payload);
    }

    #[test]
    fn extension_round_trips_fields_and_chunk_payloads() {
        let document = document_with_extension(5);
        let reopened = serialize_and_reopen(&document);
        let expected = &document.extensions[0];
        let declaration = reopened
            .manifest()
            .extension_declarations
            .iter()
            .find(|d| d.extension_id == expected.extension_id)
            .expect("extension declaration present after reopen");
        assert_eq!(declaration.version, expected.version);
        assert_eq!(declaration.required, expected.required);
        assert_eq!(
            declaration.affected_object_kinds,
            expected.affected_object_kinds
        );
        assert_eq!(declaration.edit_barriers, expected.edit_barriers);
        assert_eq!(
            declaration.preserved_chunk_roots.len(),
            expected.chunks.len()
        );
        // The manifest's own canonical encoding sorts preserved_chunk_roots by
        // ChunkRef (kind, hash, offset), not by the document's chunk order, so
        // compare payload sets rather than assume position survives.
        let mut payloads: Vec<Vec<u8>> = declaration
            .preserved_chunk_roots
            .iter()
            .map(|root| reopened.read_chunk(root).expect("preserved chunk reads"))
            .collect();
        let mut expected_payloads: Vec<Vec<u8>> =
            expected.chunks.iter().map(|c| c.payload.clone()).collect();
        payloads.sort();
        expected_payloads.sort();
        assert_eq!(
            payloads, expected_payloads,
            "preserved chunk payloads survive, order aside"
        );
    }

    #[test]
    fn envelopes_round_trip_through_a_single_operation_block() {
        let document = minimal_document(6);
        assert!(document.envelopes.len() > 1, "fixture reach check");
        let reopened = serialize_and_reopen(&document);
        assert_eq!(
            reopened.manifest().operation_roots.len(),
            1,
            "one operation-envelope block, by design"
        );
        let root = reopened.manifest().operation_roots[0];
        let payloads = reopened
            .read_operation_block(&root)
            .expect("operation block reads");
        let recovered: Vec<OperationEnvelope> = payloads
            .iter()
            .map(|bytes| decode_envelope(bytes).expect("canonical envelope decodes"))
            .collect();
        assert_eq!(recovered, document.envelopes);
    }

    #[test]
    fn operation_block_schema_major_is_derived_not_hardcoded() {
        let document = rich_document(7);
        let expected_major = document
            .envelopes
            .iter()
            .map(OperationEnvelope::schema_major)
            .max()
            .unwrap_or(0);
        assert!(
            expected_major > 0,
            "fixture must include a schema-major-bearing operation to exercise the derivation"
        );
        let reopened = serialize_and_reopen(&document);
        let root = reopened.manifest().operation_roots[0];
        assert_eq!(
            root.schema_version,
            SchemaVersion::for_major(expected_major)
        );
    }

    #[test]
    fn a_document_with_no_envelopes_still_stages_one_empty_operation_block() {
        let mut document = minimal_document(8);
        document.envelopes.clear();
        let reopened = serialize_and_reopen(&document);
        assert_eq!(reopened.manifest().operation_roots.len(), 1);
        let root = reopened.manifest().operation_roots[0];
        assert!(reopened
            .read_operation_block(&root)
            .expect("empty operation block still reads")
            .is_empty());
    }

    #[test]
    fn nonempty_blobs_are_rejected_rather_than_dropped() {
        let mut document = minimal_document(9);
        document.blobs.push(crate::TextBlob {
            media_type: "audio/wav".to_string(),
            declared_max_uncompressed_length: None,
            payload: b"nope".to_vec(),
        });
        let result = serialize_document(&document, MemStore::new(), FileUuid([1; 16]));
        assert!(matches!(result, Err(SerializeError::NonEmptyBlobs)));
    }

    #[test]
    fn a_rich_document_round_trips_every_root_at_once() {
        let document = rich_document(10);
        let reopened = serialize_and_reopen(&document);
        assert_eq!(reopened.manifest().document_id, document.document_id);
        assert!(reopened.manifest().canonical_base.is_some());
        assert_eq!(reopened.manifest().extension_declarations.len(), 1);
        assert_eq!(reopened.manifest().operation_roots.len(), 1);
        reopened
            .verify_canonical_chunks()
            .expect("every canonical chunk this module wrote is intact");
        assert!(!reopened.is_read_only());
        assert!(reopened.anomalies().is_empty());
    }

    /// `req:textproj` verification discipline item 3: a round-trip suite that
    /// never exercises an extension, a canonical base, or a multi-envelope
    /// document proves far less than its green tick suggests. Count what this
    /// suite actually covers.
    #[test]
    fn the_suite_exercises_an_extension_a_canonical_base_and_multiple_envelopes() {
        let documents = vec![
            minimal_document(20),
            document_with_extension(21),
            document_with_canonical_base(22),
            rich_document(23),
        ];
        let with_extension = documents
            .iter()
            .filter(|d| !d.extensions.is_empty())
            .count();
        let with_canonical_base = documents
            .iter()
            .filter(|d| d.canonical_base.is_some())
            .count();
        let with_multiple_envelopes = documents.iter().filter(|d| d.envelopes.len() > 1).count();

        assert!(
            with_extension >= 1,
            "reach: no document carries an extension"
        );
        assert!(
            with_canonical_base >= 1,
            "reach: no document carries a canonical base"
        );
        assert!(
            with_multiple_envelopes >= 1,
            "reach: no document carries more than one envelope"
        );

        for document in &documents {
            let reopened = serialize_and_reopen(document);
            assert_eq!(reopened.manifest().document_id, document.document_id);
        }
    }
}
