#![forbid(unsafe_code)]
//! The Epiphany Text Projection companion document layer.
//!
//! This crate bridges canonical bundle documents and the normative line-oriented
//! s-expression representation defined by `spec/text_projection.tex`.

pub mod parse;
pub mod project;
pub mod serialize;

use epiphany_bundle::{
    ChunkKind, DocumentId, ExtensionId, FrontierBytes, LineageId, ProfileDeclaration, ProfileId,
    ReductionAlgorithmVersion, SchemaVersion, SemVer, SnapshotId,
};
use epiphany_ops::OperationEnvelope;

/// The one Text Projection companion version implemented by this crate.
///
/// A parser must reject every other version rather than migrating or
/// normalizing it on read.
pub const COMPANION_VERSION: (u32, u32, u32) = (0, 7, 0);

/// A parsed canonical Text Projection document.
///
/// Under `req:textproj:derive-or-carry`, this representation deliberately erases
/// physical offsets, compressed and uncompressed storage lengths, and
/// compression choices because serialization is free to choose a new physical
/// layout. It also omits the derivable `ChunkId`, `ContentHash`, and `BlobId`;
/// those identities are recomputed from kind, schema, and inline payload.
/// Finally, it drops the non-canonical `operation_index_root`,
/// `acceleration_snapshots`, `text_projection_root`, `integrity_root`, and
/// `operation_block_summaries` accelerators. None contributes to canonical
/// document semantics, so a bundle serialized from this form correctly rebuilds
/// or omits them rather than carrying stale physical metadata.
#[derive(Debug, PartialEq)]
pub struct TextDocument {
    /// Logical identity of the projected document.
    pub document_id: DocumentId,
    /// Optional shared-ancestor identity used for document genealogy.
    pub lineage_id: Option<LineageId>,
    /// Profile declarations in canonical manifest order.
    pub profiles: Vec<ProfileDeclaration>,
    /// Extension declarations with every preserved chunk payload inline.
    pub extensions: Vec<TextExtension>,
    /// Optional canonical base with its snapshot root payload inline.
    pub canonical_base: Option<TextCanonicalBase>,
    /// Canonically reachable blobs with their payloads inline.
    pub blobs: Vec<TextBlob>,
    /// Operation envelopes in canonical reduction order.
    pub envelopes: Vec<OperationEnvelope>,
}

/// An extension declaration in its text-document form.
///
/// Unlike the bundle's `ExtensionDeclaration`, this type carries preserved
/// chunks as semantic kind/schema/payload triples, not physical `ChunkRef`s.
#[derive(Debug, PartialEq)]
pub struct TextExtension {
    /// Opaque identity of the extension.
    pub extension_id: ExtensionId,
    /// Semantic version of the extension declaration.
    pub version: SemVer,
    /// Whether an implementation unaware of the extension must refuse editing.
    pub required: bool,
    /// Preserved extension chunks, inline and ordered by projected form.
    pub chunks: Vec<TextChunk>,
    /// Canonical opaque encoding of affected object kinds.
    pub affected_object_kinds: Vec<u8>,
    /// Canonical opaque encoding of the extension's edit barriers.
    pub edit_barriers: Vec<u8>,
}

/// One preserved extension chunk with all physical reference data erased.
#[derive(Debug, PartialEq)]
pub struct TextChunk {
    /// Semantic role of the chunk.
    pub kind: ChunkKind,
    /// Schema version governing the payload bytes.
    pub schema_version: SchemaVersion,
    /// Uncompressed chunk payload carried inline.
    pub payload: Vec<u8>,
}

/// A canonical base snapshot in its text-document form.
///
/// The snapshot identity is carried because it is opaque, while the root chunk
/// identity and content hash are derived from its schema and inline payload.
#[derive(Debug, PartialEq)]
pub struct TextCanonicalBase {
    /// Opaque snapshot identity, carried verbatim.
    pub snapshot_id: SnapshotId,
    /// Opaque causal frontier materialized by the snapshot.
    pub covers_causal_frontier: FrontierBytes,
    /// Reduction algorithm version used to produce the snapshot.
    pub reduction_algorithm_version: ReductionAlgorithmVersion,
    /// Profile under which the snapshot was produced.
    pub profile_id: ProfileId,
    /// Schema version of the snapshot root chunk.
    pub root_schema_version: SchemaVersion,
    /// Uncompressed snapshot root payload carried inline.
    pub root_payload: Vec<u8>,
}

/// A canonical blob in its text-document form.
///
/// The payload is inline; its bundle `BlobId`, content hash, offset, lengths,
/// and compression metadata are deliberately absent and are derived or chosen
/// when serialized.
#[derive(Debug, PartialEq)]
pub struct TextBlob {
    /// RFC 6838 media type.
    pub media_type: String,
    /// Optional declared maximum uncompressed size.
    pub declared_max_uncompressed_length: Option<u64>,
    /// Uncompressed blob payload carried inline.
    pub payload: Vec<u8>,
}
