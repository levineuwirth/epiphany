#![forbid(unsafe_code)]
//! The Epiphany Text Projection companion document layer.
//!
//! This crate bridges canonical bundle documents and the normative line-oriented
//! s-expression representation defined by `spec/text_projection.tex`.

pub mod parse;
pub mod project;
pub mod serialize;
pub mod vectors;

use epiphany_bundle::{
    ChunkKind, DocumentId, ExtensionId, FrontierBytes, LineageId, ProfileDeclaration, ProfileId,
    ReductionAlgorithmVersion, SchemaVersion, SemVer, SnapshotId,
};
use epiphany_ops::OperationEnvelope;

/// The one Text Projection companion version implemented by this crate.
///
/// A parser must reject every other version rather than migrating or
/// normalizing it on read.
///
/// Bumped 0.7.0 → 0.8.0 by the genesis tranche G1, which appended
/// `create-instrument` to the `kind` production — the first operation kind
/// added since the header was gated to a single version. Extending the grammar
/// without moving this constant would leave two incompatible grammars both
/// claiming `(0 7 0)`. Cached projections do not migrate: a `TextProjection`
/// chunk is a non-canonical accelerator, so a stale one is regenerated.
///
/// Bumped again 0.8.0 → 0.9.0 by the genesis tranche G2a, which appended
/// `set-canvas-layout-defaults` and `set-spelling-precedence` to the `kind`
/// production — the same reasoning: extending the grammar without moving this
/// constant would leave two incompatible grammars both claiming `(0 8 0)`.
///
/// Bumped again 0.9.0 → 0.10.0 by the G-minor rung
/// (`spec/PLAN_GMINOR_SCHEMA_MINOR.md`), which added the carried manifest
/// [`SchemaVersion`] to the `document` production
/// (`document ::= "(document " bytes " " schema ")"`). **Not** because of
/// operation-block schema-minor stamping — block schemas are discarded during
/// projection and stay projection-invisible, exactly as before — but because
/// the manifest's aggregate `SchemaVersion` becomes a carried `TextDocument`
/// attribute that this companion cannot derive (`epiphany-textproj` has no
/// `epiphany-layout-ir` dependency and structurally cannot decode edit-barrier
/// bytes). Holding the version while changing the grammar would leave two
/// incompatible grammars both claiming `(0 9 0)`.
///
/// Bumped again 0.10.0 → 0.11.0 by the genesis tranche G2b
/// (`spec/CONTRACT_GENESIS_G2B_TUNING.md`), which appended
/// `set-tuning-context` to the `kind` production — the same reasoning as G1
/// and G2a: extending the grammar without moving this constant would leave
/// two incompatible grammars both claiming `(0 10 0)`.
///
/// Bumped again 0.11.0 → 0.12.0 by the genesis tranche G3a
/// (`spec/CONTRACT_GENESIS_G3A_ENTITIES.md`), which appended
/// `create-staff-group`, `create-part-definition`, `create-analysis-layer`,
/// and `create-view` to the `kind` production — the same reasoning as every
/// prior kind append: extending the grammar without moving this constant
/// would leave two incompatible grammars both claiming `(0 11 0)`.
pub const COMPANION_VERSION: (u32, u32, u32) = (0, 12, 0);

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
    /// The manifest's aggregate G-minor `SchemaVersion`
    /// (`spec/PLAN_GMINOR_SCHEMA_MINOR.md` §4, pins 8/11): carried verbatim,
    /// never derived. This companion has no `epiphany-layout-ir` dependency
    /// and cannot decode `ExtensionDeclaration::edit_barriers`, so it cannot
    /// recompute this value from the document's other fields — the document
    /// author is responsible for updating it when hand-editing barrier bytes
    /// that change what tags they name (pin 11's ruled design (a)).
    pub manifest_schema_version: SchemaVersion,
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
