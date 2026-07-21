//! Projection of canonical bundle documents to their normative text form.
//!
//! This module implements the `Bundle<S> -> TextDocument -> text` half of the
//! companion: [`document_from_bundle`] reads a bundle's manifest and its
//! reachable chunk/blob payloads into a [`TextDocument`], and the `project_*`
//! functions turn each grammar production of `spec/text_projection.tex`'s
//! Grammar chapter into its canonical [`Sexp`]. [`project_text_document`]
//! assembles a whole document's lines in the normative `projection` sequence,
//! and [`project_bundle`] composes both stages.
//!
//! # Grammar-directed, not value-directed
//!
//! `req:textproj:operation-vocabulary` established, for the operation layer,
//! that the grammar's productions govern rather than the mechanical
//! `TextValue` rule; the document layer is the same shape
//! (`CONTRACT_TEXTPROJ_DOCUMENT.md`). No document-line production contains a
//! `value` position, so every function here is a **free function**, never a
//! `TextValue` impl: `epiphany-bundle` does not depend on `epiphany-core`, and
//! implementing a foreign trait for a foreign type would be the orphan rule
//! violation the contract calls out.
//!
//! Envelope lines are the one exception: they are already a solved problem one
//! layer down. [`epiphany_ops::project_envelope`] projects an
//! [`OperationEnvelope`] line, and [`epiphany_ops::canonical_reduction_order`]
//! is the single function that orders the whole operation set. Both are used
//! here, not reimplemented.
//!
//! # What is deliberately absent
//!
//! Per `req:textproj:derive-or-carry`, nothing here carries a chunk's or
//! blob's `offset`, `compressed_length`, `compression`, or
//! `uncompressed_length`, nor a `ChunkId`, `ContentHash`, or `BlobId`: every
//! one is either a serializer's free physical choice or a value re-derived
//! from content. The non-canonical accelerators
//! (`operation_index_root`, `acceleration_snapshots`, `text_projection_root`,
//! `integrity_root`, `operation_block_summaries`) are never read here either.
//! A bundle that round-trips through text comes back without them; that is
//! correct; it is not data loss, because none of the five contributes to
//! canonical document semantics.

use std::collections::BTreeSet;

use epiphany_bundle::{
    BlobId, BlockStore, Bundle, BundleError, ChunkKind, DocumentId, LineageId, ProfileConstraints,
    ProfileDeclaration, ProfileId, RetentionPolicy, SchemaVersion, SemVer,
};
use epiphany_core::textvalue::Sexp;
use epiphany_ops::{
    canonical_reduction_order, decode_envelope, project_envelope, EnvelopeDecodeError,
    OperationEnvelope,
};

use crate::{
    TextBlob, TextCanonicalBase, TextChunk, TextDocument, TextExtension, COMPANION_VERSION,
};

// ===========================================================================
// Errors.
// ===========================================================================

/// A failure encountered projecting a bundle to its Text Projection text.
#[derive(Debug)]
pub enum ProjectError {
    /// Reading a chunk, an operation-envelope block, or a blob through the
    /// bundle failed.
    Bundle(BundleError),
    /// A stored operation-envelope block held bytes that do not decode to a
    /// canonical [`OperationEnvelope`].
    Envelope(EnvelopeDecodeError),
}

impl core::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProjectError::Bundle(e) => write!(f, "bundle read failed: {e}"),
            ProjectError::Envelope(e) => write!(f, "operation envelope failed to decode: {e}"),
        }
    }
}

impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProjectError::Bundle(e) => Some(e),
            ProjectError::Envelope(e) => Some(e),
        }
    }
}

impl From<BundleError> for ProjectError {
    fn from(e: BundleError) -> Self {
        ProjectError::Bundle(e)
    }
}

impl From<EnvelopeDecodeError> for ProjectError {
    fn from(e: EnvelopeDecodeError) -> Self {
        ProjectError::Envelope(e)
    }
}

// ===========================================================================
// Small shared helpers.
// ===========================================================================

/// The grammar's `bool` terminal: `true` or `false`, spelled as a symbol.
fn project_bool(value: bool) -> Sexp {
    Sexp::sym(if value { "true" } else { "false" })
}

/// The grammar's `option` terminal: `()` when absent, `(some <value>)` when
/// present, projecting the payload with `project`.
fn project_option<T>(value: Option<T>, project: impl FnOnce(T) -> Sexp) -> Sexp {
    match value {
        None => Sexp::none(),
        Some(v) => Sexp::some(project(v)),
    }
}

/// The grammar's shared `version` shape, `(<major> <minor> <patch>)`: used
/// verbatim by the header (the companion version) and, restated as a
/// [`SemVer`], by profile/extension declarations and the canonical base.
fn project_version_triple(major: u32, minor: u32, patch: u32) -> Sexp {
    Sexp::List(vec![Sexp::int(major), Sexp::int(minor), Sexp::int(patch)])
}

fn project_semver(version: &SemVer) -> Sexp {
    project_version_triple(version.major, version.minor, version.patch)
}

/// Orders and de-duplicates already-projected elements by the **UTF-8 bytes of
/// their rendered form**, ascending, keeping at most one per distinct
/// rendering (`req:textproj:derived-ordering`). Used for exactly the two
/// sequences the requirement names: the `(blob ...)` lines, and one
/// extension's preserved chunk lines. Every other projected sequence keeps
/// whatever order its caller already put it in (the manifest's binary order,
/// which for those sequences is itself a function only of preserved data).
///
/// This is deliberately a *projection-time* operation rather than a
/// construction-time one: it must produce the correct text even when handed
/// an out-of-order or duplicated slice, which is exactly what
/// `derived_ordering_sorts_and_dedups_blobs_and_chunks` below constructs and
/// checks.
fn ordered_by_projected_form<T>(items: &[T], project: impl Fn(&T) -> Sexp) -> Vec<Sexp> {
    let mut rendered: Vec<(String, Sexp)> = items
        .iter()
        .map(|item| {
            let sexp = project(item);
            let text = sexp.render();
            (text, sexp)
        })
        .collect();
    rendered.sort_by(|(a, _), (b, _)| a.cmp(b));
    rendered.dedup_by(|(a, _), (b, _)| a == b);
    rendered.into_iter().map(|(_, sexp)| sexp).collect()
}

// ===========================================================================
// header, document, lineage.
// ===========================================================================

/// `header ::= "(text-projection " version ")" LF`.
///
/// Always [`COMPANION_VERSION`]: `req:textproj:header-version` fixes the
/// header to name the one companion version this crate implements.
pub fn project_header() -> Sexp {
    let (major, minor, patch) = COMPANION_VERSION;
    Sexp::List(vec![
        Sexp::sym("text-projection"),
        project_version_triple(major, minor, patch),
    ])
}

/// `document ::= "(document " bytes ")" LF`.
pub fn project_document(document_id: &DocumentId) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("document"),
        Sexp::Bytes(document_id.as_bytes().to_vec()),
    ])
}

/// `lineage ::= "(lineage " bytes ")" LF`.
pub fn project_lineage(lineage_id: &LineageId) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("lineage"),
        Sexp::Bytes(lineage_id.as_bytes().to_vec()),
    ])
}

// ===========================================================================
// profile, profile-id, constraints, retention.
// ===========================================================================

/// `profile-id ::= "full" | "read-only" | "lite" | "(custom " bytes ")"`.
///
/// `Custom` is the one profile identity that is not a bare symbol: it carries
/// a 16-byte registry id (`req:textproj:profile-id`).
pub fn project_profile_id(profile_id: &ProfileId) -> Sexp {
    match profile_id {
        ProfileId::Full => Sexp::sym("full"),
        ProfileId::ReadOnly => Sexp::sym("read-only"),
        ProfileId::Lite => Sexp::sym("lite"),
        ProfileId::Custom(registry_id) => Sexp::List(vec![
            Sexp::sym("custom"),
            Sexp::Bytes(registry_id.as_bytes().to_vec()),
        ]),
    }
}

/// `retention ::= "(retention " integer " " option " " bool ")"`.
///
/// Fields, positionally: `retain_previous_manifests`, `retain_duration`,
/// `retain_named_checkpoints`. `WallClockDuration` is a newtype over `i64`, so
/// it projects transparently as the bare integer inside the option, per the
/// value-projection rule for newtypes.
pub fn project_retention(retention: &RetentionPolicy) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("retention"),
        Sexp::int(retention.retain_previous_manifests),
        project_option(retention.retain_duration, |duration| Sexp::int(duration.0)),
        project_bool(retention.retain_named_checkpoints),
    ])
}

/// `constraints ::= "(constraints " integer " " retention ")"`.
pub fn project_constraints(constraints: &ProfileConstraints) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("constraints"),
        Sexp::int(constraints.max_uncompressed_block_size),
        project_retention(&constraints.retention_policy),
    ])
}

/// `profile ::= "(profile " profile-id " " version " " constraints ")" LF`.
pub fn project_profile(profile: &ProfileDeclaration) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("profile"),
        project_profile_id(&profile.profile_id),
        project_semver(&profile.version),
        project_constraints(&profile.constraints),
    ])
}

// ===========================================================================
// chunk, chunk-kind, schema, extension.
// ===========================================================================

/// `chunk-kind ::= "operation-envelope-block" | "operation-index" | "snapshot"
/// | "blob" | "extension-data" | "text-projection" | "layout-cache"
/// | "integrity-index" | "manifest"`.
///
/// Exhaustive over [`ChunkKind`]'s nine variants: adding a tenth to the bundle
/// crate is a compile error here until this match is extended, rather than a
/// silently-unprojectable kind.
pub fn project_chunk_kind(kind: ChunkKind) -> Sexp {
    Sexp::sym(match kind {
        ChunkKind::OperationEnvelopeBlock => "operation-envelope-block",
        ChunkKind::OperationIndex => "operation-index",
        ChunkKind::Snapshot => "snapshot",
        ChunkKind::Blob => "blob",
        ChunkKind::ExtensionData => "extension-data",
        ChunkKind::TextProjection => "text-projection",
        ChunkKind::LayoutCache => "layout-cache",
        ChunkKind::IntegrityIndex => "integrity-index",
        ChunkKind::Manifest => "manifest",
    })
}

/// `schema ::= "(schema " integer " " integer ")"`.
pub fn project_schema(schema: &SchemaVersion) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("schema"),
        Sexp::int(schema.major),
        Sexp::int(schema.minor),
    ])
}

/// `chunk ::= "(chunk " chunk-kind " " schema " " bytes ")"`.
///
/// The bytes are the chunk's **uncompressed payload**, never a `ChunkRef`:
/// the projection has no file to point into
/// (`req:textproj:extension-declaration`).
pub fn project_chunk(chunk: &TextChunk) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("chunk"),
        project_chunk_kind(chunk.kind),
        project_schema(&chunk.schema_version),
        Sexp::Bytes(chunk.payload.clone()),
    ])
}

/// `extension ::= "(extension " bytes " " version " " bool " (" chunk* ") "
/// bytes " " bytes ")" LF`, fields in the ratified declaration order: id,
/// version, required, chunks, affected-kinds, barriers.
///
/// `affected_object_kinds` and `edit_barriers` are opaque byte strings, never
/// structured sequences: the bundle preserves them without interpreting them,
/// and the projection interprets nothing the bundle does not
/// (`req:textproj:extension-declaration`). The preserved-chunk list is ordered
/// and de-duplicated by projected form (`req:textproj:derived-ordering`),
/// because `ChunkRef`'s binary order breaks ties on the physical offset.
pub fn project_extension(extension: &TextExtension) -> Sexp {
    let chunks = ordered_by_projected_form(&extension.chunks, project_chunk);
    Sexp::List(vec![
        Sexp::sym("extension"),
        Sexp::Bytes(extension.extension_id.as_bytes().to_vec()),
        project_semver(&extension.version),
        project_bool(extension.required),
        Sexp::List(chunks),
        Sexp::Bytes(extension.affected_object_kinds.clone()),
        Sexp::Bytes(extension.edit_barriers.clone()),
    ])
}

// ===========================================================================
// canonical-base.
// ===========================================================================

/// `canonical-base ::= "(canonical-base " bytes " " bytes " " integer " "
/// profile-id " " schema " " bytes ")" LF`: snapshot id, frontier, reduction
/// version, profile, root schema, root payload.
///
/// The `SnapshotId` is the one identity carried verbatim rather than derived
/// (`req:textproj:derive-or-carry`); the root chunk's kind is `Snapshot` by
/// role and is not written, and its `ChunkId`/hash are re-derived from the
/// schema and payload carried here (`req:textproj:base-snapshot-inline`).
pub fn project_canonical_base(base: &TextCanonicalBase) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("canonical-base"),
        Sexp::Bytes(base.snapshot_id.as_bytes().to_vec()),
        Sexp::Bytes(base.covers_causal_frontier.as_bytes().to_vec()),
        Sexp::int(base.reduction_algorithm_version.0),
        project_profile_id(&base.profile_id),
        project_schema(&base.root_schema_version),
        Sexp::Bytes(base.root_payload.clone()),
    ])
}

// ===========================================================================
// blob.
// ===========================================================================

/// `blob ::= "(blob " string " " option " " bytes ")" LF`: media type,
/// declared maximum uncompressed length, payload.
///
/// The `BlobId`, content hash, offset, lengths, and compression are all
/// re-derived or freely chosen by a serializer, never carried
/// (`req:textproj:derive-or-carry`, `req:textproj:canonical-blobs`).
pub fn project_blob(blob: &TextBlob) -> Sexp {
    Sexp::List(vec![
        Sexp::sym("blob"),
        Sexp::Str(blob.media_type.clone()),
        project_option(blob.declared_max_uncompressed_length, Sexp::int),
        Sexp::Bytes(blob.payload.clone()),
    ])
}

// ===========================================================================
// Canonical-blob reachability (`req:textproj:canonical-blobs`).
// ===========================================================================

/// The `BlobId`s canonically reachable from `envelopes`: referenced by a
/// canonical operation, or by canonical reduced state
/// (`req:textproj:canonical-blobs`; core specification Chapter 8
/// §"Canonical and Non-Canonical Manifest Roots"). A blob referenced only by
/// an acceleration structure is **not** in this set and MUSTNOT be projected.
///
/// This is a real predicate over the decoded canonical operations, not an
/// assertion of emptiness: it is written so that the day an operation payload
/// or a Chapter-5 value gains a field that names a blob, extending the walk
/// below is the one and only change needed to make that blob projectable.
///
/// That day has not arrived. Every [`OperationPayload`](epiphany_ops::OperationPayload)
/// variant and every [`OperationKind`](epiphany_ops::OperationKind) variant is
/// exhaustively enumerated by `epiphany-ops`, and canonical reduced state
/// (`epiphany_ops::MaterializedState`) is built exclusively from those same
/// payloads — and the token `BlobId` does not occur anywhere in
/// `epiphany-core` or `epiphany-ops` (verified by
/// `blobid_is_absent_from_core_and_ops_source`, below, which fails the moment
/// it does). So there is no field, on any canonical value or payload this
/// crate can name, to extract a blob reference from — and this function's
/// result is provably the empty set today.
fn canonically_reachable_blob_ids(_envelopes: &[OperationEnvelope]) -> BTreeSet<BlobId> {
    // Nothing to walk: see the doc comment above and the trip-wire test.
    BTreeSet::new()
}

/// The canonical blobs of a document, read through the bundle: the
/// [`TextBlob`]s of exactly the `BlobRef`s in `bundle.manifest().blob_roots`
/// whose id is canonically reachable from `envelopes`.
///
/// **Never** `bundle.manifest().blob_roots` wholesale — that is the plausible
/// wrong answer named by the contract: one line, looks right, and emits every
/// non-canonical blob the manifest happens to carry, in direct violation of
/// `req:textproj:canonical-blobs`. Filtering through
/// [`canonically_reachable_blob_ids`] is what keeps this correct even though,
/// today, that filter is always the empty set — so this function always
/// returns `Ok(Vec::new())` for a real bundle, and does so *because* nothing
/// resolves as reachable, not because the manifest's field is skipped by
/// construction.
fn canonical_blobs<S: BlockStore>(
    bundle: &Bundle<S>,
    envelopes: &[OperationEnvelope],
) -> Result<Vec<TextBlob>, ProjectError> {
    let reachable = canonically_reachable_blob_ids(envelopes);
    let mut blobs = Vec::new();
    for blob_ref in &bundle.manifest().blob_roots {
        if reachable.contains(&blob_ref.blob_id) {
            let payload = bundle.read_blob(blob_ref)?;
            blobs.push(TextBlob {
                media_type: blob_ref.media_type.clone(),
                declared_max_uncompressed_length: blob_ref.declared_max_uncompressed_length,
                payload,
            });
        }
    }
    Ok(blobs)
}

// ===========================================================================
// Bundle<S> -> TextDocument.
// ===========================================================================

/// Builds a [`TextDocument`] from a bundle's current manifest, reading every
/// chunk and blob payload the projection carries inline: each extension's
/// preserved chunks, the canonical base's root chunk, and (today, always
/// none) canonically reachable blobs.
///
/// Operation envelopes are decoded from **every** operation-envelope block the
/// manifest references (`manifest.operation_roots`; block boundaries are a
/// storage artifact, not semantic structure) via
/// [`epiphany_ops::decode_envelope`], then put into
/// [`epiphany_ops::canonical_reduction_order`] — computed once over the whole
/// gathered set, because the order is causal and cannot be decided per block.
pub fn document_from_bundle<S: BlockStore>(
    bundle: &Bundle<S>,
) -> Result<TextDocument, ProjectError> {
    let manifest = bundle.manifest();

    let mut decoded = Vec::new();
    for root in &manifest.operation_roots {
        for raw in bundle.read_operation_block(root)? {
            decoded.push(decode_envelope(&raw)?);
        }
    }
    let envelopes: Vec<OperationEnvelope> = {
        let refs: Vec<&OperationEnvelope> = decoded.iter().collect();
        canonical_reduction_order(&refs)
            .into_iter()
            .cloned()
            .collect()
    };

    let mut extensions = Vec::with_capacity(manifest.extension_declarations.len());
    for extension in &manifest.extension_declarations {
        let mut chunks = Vec::with_capacity(extension.preserved_chunk_roots.len());
        for chunk_ref in &extension.preserved_chunk_roots {
            let payload = bundle.read_chunk(chunk_ref)?;
            chunks.push(TextChunk {
                kind: chunk_ref.kind,
                schema_version: chunk_ref.schema_version,
                payload,
            });
        }
        extensions.push(TextExtension {
            extension_id: extension.extension_id,
            version: extension.version,
            required: extension.required,
            chunks,
            affected_object_kinds: extension.affected_object_kinds.clone(),
            edit_barriers: extension.edit_barriers.clone(),
        });
    }

    let canonical_base = match &manifest.canonical_base {
        Some(base) => {
            let root_payload = bundle.read_chunk(&base.root)?;
            Some(TextCanonicalBase {
                snapshot_id: base.snapshot_id,
                covers_causal_frontier: base.covers_causal_frontier.clone(),
                reduction_algorithm_version: base.reduction_algorithm_version,
                profile_id: base.profile_id,
                root_schema_version: base.root.schema_version,
                root_payload,
            })
        }
        None => None,
    };

    let blobs = canonical_blobs(bundle, &decoded)?;

    Ok(TextDocument {
        document_id: manifest.document_id,
        lineage_id: manifest.lineage_id,
        // `Manifest::decode`'s own re-encode check (see `epiphany-bundle`)
        // guarantees a bundle's `profile_declarations` are already the
        // canonical (sorted, deduplicated) sequence; `canonical_profiles`
        // names that guarantee rather than leaning on it silently.
        profiles: manifest.canonical_profiles(),
        extensions,
        canonical_base,
        blobs,
        envelopes,
    })
}

// ===========================================================================
// TextDocument -> text.
// ===========================================================================

/// Projects a whole [`TextDocument`] to its canonical text: the header, then
/// every present section in the normative `projection` sequence (header,
/// document, lineage?, profile*, extension*, canonical-base?, blob*,
/// envelope*), one line per element, each terminated by a single LF
/// (`req:textproj:envelope-per-line`). Section order is normative and is
/// simply the order this function writes in; it introduces no ordering of its
/// own beyond `req:textproj:derived-ordering`'s blob sort.
pub fn project_text_document(document: &TextDocument) -> String {
    let mut lines: Vec<Sexp> = Vec::new();
    lines.push(project_header());
    lines.push(project_document(&document.document_id));
    if let Some(lineage_id) = &document.lineage_id {
        lines.push(project_lineage(lineage_id));
    }
    lines.extend(document.profiles.iter().map(project_profile));
    lines.extend(document.extensions.iter().map(project_extension));
    if let Some(base) = &document.canonical_base {
        lines.push(project_canonical_base(base));
    }
    lines.extend(ordered_by_projected_form(&document.blobs, project_blob));

    let mut out = String::new();
    for line in &lines {
        out.push_str(&line.render());
        out.push('\n');
    }
    for envelope in &document.envelopes {
        out.push_str(&project_envelope(envelope));
        out.push('\n');
    }
    out
}

/// Composes both stages: reads `bundle` into a [`TextDocument`], then projects
/// it to its canonical text.
pub fn project_bundle<S: BlockStore>(bundle: &Bundle<S>) -> Result<String, ProjectError> {
    Ok(project_text_document(&document_from_bundle(bundle)?))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use epiphany_bundle::{
        encode_block, BlobRef, CompressionAlgorithm, ExtensionDeclaration, ExtensionId, FileUuid,
        FrontierBytes, Manifest, MemStore, ProfileRegistryId, ReductionAlgorithmVersion,
        SnapshotId, SnapshotRef, StagedChunk,
    };
    use epiphany_core::textvalue::read_sexp;
    use epiphany_core::{OperationId, RegionId, ReplicaId, WallClockTime};
    use epiphany_determinism::CanonicalEncode;
    use epiphany_ops::{
        AuthorId, CausalContext, DeleteRegionOp, HybridLogicalClock, OperationKind,
        OperationPayload, OperationStamp,
    };

    use super::*;

    // -----------------------------------------------------------------
    // Pinned against the companion's worked example (Chapter "A Worked
    // Example"). These are the strongest checks in this file: literal
    // strings copied from the specification itself, not values this test
    // module invented.
    // -----------------------------------------------------------------

    #[test]
    fn header_matches_the_worked_example() {
        assert_eq!(project_header().render(), "(text-projection (0 7 0))");
    }

    #[test]
    fn document_id_matches_the_worked_example() {
        let id = DocumentId([0x05; 16]);
        assert_eq!(
            project_document(&id).render(),
            "(document #x05050505050505050505050505050505)"
        );
    }

    #[test]
    fn profile_matches_the_worked_example() {
        assert_eq!(
            project_profile(&ProfileDeclaration::full()).render(),
            "(profile full (0 1 0) (constraints 67108864 (retention 1 () true)))"
        );
    }

    #[test]
    fn canonical_base_matches_the_worked_example() {
        let mut snapshot_id_bytes = [0u8; 16];
        snapshot_id_bytes[0] = 0x1f;
        snapshot_id_bytes[1] = 0x8b;
        let base = TextCanonicalBase {
            snapshot_id: SnapshotId(snapshot_id_bytes),
            covers_causal_frontier: FrontierBytes::empty(),
            reduction_algorithm_version: ReductionAlgorithmVersion(1),
            profile_id: ProfileId::Full,
            root_schema_version: SchemaVersion::V0,
            root_payload: vec![0x00, 0x00],
        };
        assert_eq!(
            project_canonical_base(&base).render(),
            "(canonical-base #x1f8b0000000000000000000000000000 #x 1 full (schema 0 1) #x0000)"
        );
    }

    // -----------------------------------------------------------------
    // profile-id: the closed vocabulary, plus the one non-symbol case.
    // -----------------------------------------------------------------

    #[test]
    fn profile_id_projects_every_closed_vocabulary_symbol() {
        assert_eq!(project_profile_id(&ProfileId::Full).render(), "full");
        assert_eq!(
            project_profile_id(&ProfileId::ReadOnly).render(),
            "read-only"
        );
        assert_eq!(project_profile_id(&ProfileId::Lite).render(), "lite");
    }

    #[test]
    fn profile_id_custom_carries_its_sixteen_byte_registry_id() {
        let id = ProfileId::Custom(ProfileRegistryId([0xAB; 16]));
        assert_eq!(
            project_profile_id(&id).render(),
            "(custom #xabababababababababababababababab)"
        );
    }

    // -----------------------------------------------------------------
    // chunk-kind: exhaustive over all nine variants.
    // -----------------------------------------------------------------

    #[test]
    fn chunk_kind_projects_every_variant_to_its_grammar_symbol() {
        let expected = [
            (
                ChunkKind::OperationEnvelopeBlock,
                "operation-envelope-block",
            ),
            (ChunkKind::OperationIndex, "operation-index"),
            (ChunkKind::Snapshot, "snapshot"),
            (ChunkKind::Blob, "blob"),
            (ChunkKind::ExtensionData, "extension-data"),
            (ChunkKind::TextProjection, "text-projection"),
            (ChunkKind::LayoutCache, "layout-cache"),
            (ChunkKind::IntegrityIndex, "integrity-index"),
            (ChunkKind::Manifest, "manifest"),
        ];
        for (kind, symbol) in expected {
            assert_eq!(project_chunk_kind(kind).render(), symbol);
        }
    }

    #[test]
    fn schema_projects_major_then_minor() {
        assert_eq!(
            project_schema(&SchemaVersion::new(2, 9)).render(),
            "(schema 2 9)"
        );
    }

    // -----------------------------------------------------------------
    // extension: six fields in the ratified order, opaque byte strings for
    // affected-kinds/barriers (the `Vec<u8>`-is-not-a-sequence trap).
    // -----------------------------------------------------------------

    #[test]
    fn extension_projects_six_fields_in_declaration_order() {
        let extension = TextExtension {
            extension_id: ExtensionId([9; 16]),
            version: SemVer::new(1, 2, 3),
            required: true,
            chunks: vec![TextChunk {
                kind: ChunkKind::ExtensionData,
                schema_version: SchemaVersion::V0,
                payload: vec![0xAA],
            }],
            affected_object_kinds: vec![0x01, 0x02],
            edit_barriers: vec![0x03],
        };
        assert_eq!(
            project_extension(&extension).render(),
            "(extension #x09090909090909090909090909090909 (1 2 3) true \
             ((chunk extension-data (schema 0 1) #xaa)) #x0102 #x03)"
        );
    }

    #[test]
    fn extension_affected_kinds_and_barriers_are_byte_strings_not_sequences() {
        let extension = TextExtension {
            extension_id: ExtensionId([0; 16]),
            version: SemVer::new(0, 0, 0),
            required: false,
            chunks: Vec::new(),
            affected_object_kinds: vec![1, 2, 3],
            edit_barriers: vec![4, 5],
        };
        let Sexp::List(fields) = project_extension(&extension) else {
            panic!("an extension projection is a list");
        };
        // Positions: [0]=`extension`, [1]=id, [2]=version, [3]=required,
        // [4]=chunks, [5]=affected-kinds, [6]=barriers.
        assert_eq!(fields[5], Sexp::Bytes(vec![1, 2, 3]));
        assert_eq!(fields[6], Sexp::Bytes(vec![4, 5]));
        // Never the `Vec<u8>`-is-a-sequence spelling (a list of integers).
        assert_ne!(
            fields[5],
            Sexp::List(vec![Sexp::int(1), Sexp::int(2), Sexp::int(3)])
        );
    }

    // -----------------------------------------------------------------
    // blob: line-level project, tested against synthetic data (per the
    // contract: the emit side must be ready even though no real bundle can
    // populate it today).
    // -----------------------------------------------------------------

    #[test]
    fn blob_projects_media_type_declared_max_and_payload() {
        let blob = TextBlob {
            media_type: "audio/wav".to_owned(),
            declared_max_uncompressed_length: Some(1 << 20),
            payload: vec![0xDE, 0xAD],
        };
        assert_eq!(
            project_blob(&blob).render(),
            "(blob \"audio/wav\" (some 1048576) #xdead)"
        );
    }

    #[test]
    fn blob_with_no_declared_maximum_projects_an_absent_option() {
        let blob = TextBlob {
            media_type: "application/octet-stream".to_owned(),
            declared_max_uncompressed_length: None,
            payload: Vec::new(),
        };
        assert_eq!(
            project_blob(&blob).render(),
            "(blob \"application/octet-stream\" () #x)"
        );
    }

    /// The `(blob ...)` production round-trips through the s-expression
    /// reader (the lexical layer parse.rs will build its semantic parser on).
    /// This is not a substitute for parse.rs's own `parse_blob` and the
    /// parse-side rejection the contract requires of it; it proves the emit
    /// side is well-formed and ready.
    #[test]
    fn blob_projection_reads_back_through_the_sexp_reader() {
        let blob = TextBlob {
            media_type: "text/plain".to_owned(),
            declared_max_uncompressed_length: Some(4),
            payload: vec![1, 2, 3, 4],
        };
        let rendered = project_blob(&blob).render();
        let read_back = read_sexp(&rendered).expect("projected blob line is valid s-expression");
        assert_eq!(read_back, project_blob(&blob), "reading back is idempotent");
    }

    // -----------------------------------------------------------------
    // `req:textproj:derived-ordering`: sort AND de-duplicate by rendered
    // form, exercised against out-of-order, duplicated, in-memory data --
    // never against a fixture that was already sorted.
    // -----------------------------------------------------------------

    #[test]
    fn derived_ordering_sorts_and_dedups_blob_lines() {
        let a = TextBlob {
            media_type: "a/a".to_owned(),
            declared_max_uncompressed_length: None,
            payload: vec![1],
        };
        let b = TextBlob {
            media_type: "b/b".to_owned(),
            declared_max_uncompressed_length: None,
            payload: vec![2],
        };
        let a_duplicate = TextBlob {
            media_type: "a/a".to_owned(),
            declared_max_uncompressed_length: None,
            payload: vec![1],
        };
        // Deliberately out of order (b before a) and with a duplicate of a.
        let blobs = vec![b, a_duplicate, a];
        let ordered = ordered_by_projected_form(&blobs, project_blob);
        let rendered: Vec<String> = ordered.iter().map(Sexp::render).collect();
        assert_eq!(
            rendered,
            vec![
                "(blob \"a/a\" () #x01)".to_owned(),
                "(blob \"b/b\" () #x02)".to_owned(),
            ],
            "duplicate collapsed to one line, and the surviving two lines ordered ascending \
             by rendered form"
        );
    }

    #[test]
    fn derived_ordering_sorts_and_dedups_an_extensions_chunk_lines() {
        let x = TextChunk {
            kind: ChunkKind::ExtensionData,
            schema_version: SchemaVersion::V0,
            payload: vec![0xAA],
        };
        let y = TextChunk {
            kind: ChunkKind::LayoutCache,
            schema_version: SchemaVersion::V0,
            payload: vec![0xBB],
        };
        let x_duplicate = TextChunk {
            kind: ChunkKind::ExtensionData,
            schema_version: SchemaVersion::V0,
            payload: vec![0xAA],
        };
        let extension = TextExtension {
            extension_id: ExtensionId([1; 16]),
            version: SemVer::new(1, 0, 0),
            required: false,
            // Deliberately out of order (y before x) and with a duplicate of x.
            chunks: vec![y, x_duplicate, x],
            affected_object_kinds: Vec::new(),
            edit_barriers: Vec::new(),
        };
        let Sexp::List(fields) = project_extension(&extension) else {
            panic!("an extension projection is a list");
        };
        let Sexp::List(chunks) = &fields[4] else {
            panic!("the chunk field is a list");
        };
        assert_eq!(
            chunks.len(),
            2,
            "the duplicate chunk collapsed to a single line"
        );
        assert_eq!(
            chunks[0].render(),
            "(chunk extension-data (schema 0 1) #xaa)"
        );
        assert_eq!(chunks[1].render(), "(chunk layout-cache (schema 0 1) #xbb)");
    }

    // -----------------------------------------------------------------
    // The blob trap: a bundle whose manifest carries a real, non-empty
    // `blob_roots` must still project zero `(blob ...)` lines, because no
    // canonical operation or reduced state can reach a `BlobId` today.
    // -----------------------------------------------------------------

    /// Builds a bundle exercising every optional section at once: a lineage,
    /// one extension with one preserved chunk, a canonical base, one
    /// (deliberately unreachable) blob root, and two operation-envelope
    /// blocks staged so their envelopes are **not** already in canonical
    /// reduction order (physical time descending), across **two** separate
    /// blocks so `document_from_bundle` is proven to gather across all of
    /// `operation_roots`, not just the first.
    fn build_sample_bundle() -> Bundle<MemStore> {
        let env_late = sample_envelope(1, 200); // higher physical time
        let env_early = sample_envelope(2, 100); // lower physical time

        // Block 0 holds the later envelope, block 1 the earlier one: storage
        // order disagrees with canonical reduction order on purpose.
        let op_block_0 =
            StagedChunk::operation_block(encode_block(&[env_late.to_canonical_bytes()]));
        let op_block_1 =
            StagedChunk::operation_block(encode_block(&[env_early.to_canonical_bytes()]));

        let extension_chunk = StagedChunk {
            kind: ChunkKind::ExtensionData,
            schema_version: SchemaVersion::V0,
            payload: b"extension-chunk-payload".to_vec(),
        };

        let base_chunk = StagedChunk {
            kind: ChunkKind::Snapshot,
            schema_version: SchemaVersion::V0,
            payload: b"canonical-base-payload".to_vec(),
        };

        let blob_chunk = StagedChunk {
            kind: ChunkKind::Blob,
            schema_version: SchemaVersion::V0,
            payload: b"unreachable-blob-payload".to_vec(),
        };

        let mut initial = Manifest::empty(DocumentId([3; 16]));
        initial.lineage_id = Some(LineageId([4; 16]));
        let mut bundle = Bundle::create(MemStore::new(), FileUuid([1; 16]), initial)
            .expect("a freshly created bundle with no canonical roots is valid");

        bundle
            .commit(
                &[
                    op_block_0,
                    op_block_1,
                    extension_chunk,
                    base_chunk,
                    blob_chunk,
                ],
                |ctx| {
                    let mut manifest = ctx.previous_manifest.clone();
                    manifest.operation_roots = vec![ctx.new_chunks[0], ctx.new_chunks[1]];
                    manifest.extension_declarations = vec![ExtensionDeclaration {
                        extension_id: ExtensionId([9; 16]),
                        version: SemVer::new(1, 0, 0),
                        required: false,
                        preserved_chunk_roots: vec![ctx.new_chunks[2]],
                        affected_object_kinds: vec![0xAA],
                        edit_barriers: vec![0xBB, 0xCC],
                    }];
                    manifest.canonical_base = Some(SnapshotRef {
                        snapshot_id: SnapshotId([7; 16]),
                        covers_causal_frontier: FrontierBytes::from_bytes(vec![1, 2, 3]),
                        reduction_algorithm_version: ReductionAlgorithmVersion(1),
                        profile_id: ProfileId::Full,
                        root: ctx.new_chunks[3],
                        hash: ctx.new_chunks[3].hash,
                    });
                    manifest.blob_roots = vec![BlobRef {
                        blob_id: BlobId(ctx.new_chunks[4].hash),
                        media_type: "application/octet-stream".to_owned(),
                        offset: ctx.new_chunks[4].offset,
                        compressed_length: ctx.new_chunks[4].compressed_length,
                        uncompressed_length: ctx.new_chunks[4].uncompressed_length,
                        compression: CompressionAlgorithm::None,
                        hash: ctx.new_chunks[4].hash,
                        declared_max_uncompressed_length: None,
                    }];
                    manifest
                },
            )
            .expect("commit of a well-formed, self-consistent manifest succeeds");

        // Sanity: the trap this test exists to guard against is only live if
        // the manifest really does carry a non-empty, non-canonical blob root.
        assert_eq!(bundle.manifest().blob_roots.len(), 1);
        bundle
    }

    fn sample_envelope(counter: u64, physical_time: i64) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(1), counter);
        OperationEnvelope {
            id,
            author: AuthorId(0xAB),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(physical_time), 0),
                id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(OperationKind::DeleteRegion(DeleteRegionOp {
                region: RegionId::new(ReplicaId(1), counter),
            })),
        }
    }

    #[test]
    fn document_from_bundle_reads_every_section_and_orders_envelopes_canonically() {
        let bundle = build_sample_bundle();
        let document = document_from_bundle(&bundle).expect("bundle reads cleanly");

        assert_eq!(document.document_id, DocumentId([3; 16]));
        assert_eq!(document.lineage_id, Some(LineageId([4; 16])));
        assert_eq!(document.profiles, vec![ProfileDeclaration::full()]);

        assert_eq!(document.extensions.len(), 1);
        let extension = &document.extensions[0];
        assert_eq!(extension.extension_id, ExtensionId([9; 16]));
        assert_eq!(extension.chunks.len(), 1);
        assert_eq!(extension.chunks[0].kind, ChunkKind::ExtensionData);
        assert_eq!(extension.chunks[0].payload, b"extension-chunk-payload");

        let base = document.canonical_base.as_ref().expect("base was staged");
        assert_eq!(base.snapshot_id, SnapshotId([7; 16]));
        assert_eq!(base.root_payload, b"canonical-base-payload");

        // The trap: a non-empty `blob_roots` still yields zero canonical
        // blobs, because nothing can reach one yet.
        assert!(
            document.blobs.is_empty(),
            "no blob is canonically reachable today, regardless of manifest.blob_roots"
        );

        // Canonical reduction order is by ascending physical time; the raw
        // blocks stored the higher-physical-time envelope first.
        assert_eq!(document.envelopes.len(), 2);
        assert_eq!(document.envelopes[0].id, OperationId::new(ReplicaId(1), 2));
        assert_eq!(document.envelopes[1].id, OperationId::new(ReplicaId(1), 1));
    }

    #[test]
    fn projected_text_never_emits_a_blob_line_and_orders_sections_correctly() {
        let bundle = build_sample_bundle();
        let document = document_from_bundle(&bundle).expect("bundle reads cleanly");
        let text = project_text_document(&document);

        assert!(
            !text.lines().any(|line| line.starts_with("(blob ")),
            "projected text must never carry a blob line at this companion version:\n{text}"
        );

        // Section order is normative (`projection ::= header document lineage?
        // profile* extension* canonical-base? blob* envelope*`): assert the
        // literal sequence of line-heads, not just a count, so a section
        // written out of order fails this check rather than only a count.
        let lines: Vec<&str> = text.lines().collect();
        let heads: Vec<&str> = lines
            .iter()
            .map(|line| line.split(' ').next().expect("every line has a head token"))
            .collect();
        assert_eq!(
            heads,
            vec![
                "(text-projection",
                "(document",
                "(lineage",
                "(profile",
                "(extension",
                "(canonical-base",
                "(envelope",
                "(envelope",
            ],
            "section order is normative; got:\n{text}"
        );
    }

    #[test]
    fn project_bundle_composes_both_stages() {
        let bundle = build_sample_bundle();
        let via_two_stages =
            project_text_document(&document_from_bundle(&bundle).expect("bundle reads cleanly"));
        let via_one_call = project_bundle(&bundle).expect("bundle reads cleanly");
        assert_eq!(via_one_call, via_two_stages);
    }

    #[test]
    fn a_corrupt_operation_envelope_is_a_typed_error_not_a_panic() {
        let mut initial = Manifest::empty(DocumentId([5; 16]));
        initial.lineage_id = None;
        let mut bundle = Bundle::create(MemStore::new(), FileUuid([2; 16]), initial).unwrap();
        let garbage_block = StagedChunk::operation_block(encode_block(&[vec![0xFF; 4]]));
        bundle
            .commit(&[garbage_block], |ctx| {
                let mut manifest = ctx.previous_manifest.clone();
                manifest.operation_roots = vec![ctx.new_chunks[0]];
                manifest
            })
            .expect("the block's framing is well-formed even though its one envelope is not");

        match document_from_bundle(&bundle) {
            Err(ProjectError::Envelope(_)) => {}
            other => panic!("expected a decode error for garbage envelope bytes, got {other:?}"),
        }
    }

    #[test]
    fn a_corrupt_chunk_hash_is_a_bundle_error() {
        let bundle = build_sample_bundle();
        let extension_root = bundle.manifest().extension_declarations[0].preserved_chunk_roots[0];
        let mut image = bundle.image().to_vec();
        // Flip one byte inside the extension chunk's stored payload region.
        let corrupt_at = extension_root.offset as usize;
        image[corrupt_at] ^= 0xFF;

        let corrupted = Bundle::open(MemStore::from_bytes(image))
            .expect("corrupting a non-canonical chunk's payload does not stop the bundle opening");
        match document_from_bundle(&corrupted) {
            Err(ProjectError::Bundle(_)) => {}
            other => panic!("expected a bundle error for a hash-mismatched chunk, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // Suite reach: count, don't just claim, coverage of an extension, a
    // canonical base, and a multi-envelope document.
    // -----------------------------------------------------------------

    #[test]
    fn test_suite_reach_covers_extension_base_and_multi_envelope_documents() {
        let rich = document_from_bundle(&build_sample_bundle()).expect("bundle reads cleanly");
        let minimal = TextDocument {
            document_id: DocumentId([6; 16]),
            lineage_id: None,
            profiles: vec![ProfileDeclaration::full()],
            extensions: Vec::new(),
            canonical_base: None,
            blobs: Vec::new(),
            envelopes: vec![sample_envelope(1, 1)],
        };
        let documents = [rich, minimal];

        let with_extension = documents
            .iter()
            .filter(|d| !d.extensions.is_empty())
            .count();
        let with_canonical_base = documents
            .iter()
            .filter(|d| d.canonical_base.is_some())
            .count();
        let with_multi_envelope = documents.iter().filter(|d| d.envelopes.len() > 1).count();

        assert_eq!(
            with_extension, 1,
            "expected exactly one exercised document with an extension"
        );
        assert_eq!(
            with_canonical_base, 1,
            "expected exactly one exercised document with a canonical base"
        );
        assert_eq!(
            with_multi_envelope, 1,
            "expected exactly one exercised document with more than one envelope"
        );
    }

    // -----------------------------------------------------------------
    // The trip-wire: if `BlobId` ever appears in `epiphany-core` or
    // `epiphany-ops`, `canonically_reachable_blob_ids` above is no longer
    // provably empty, and both the emit side (that function) and the
    // parse-side `(blob ...)` rejection (parse.rs) need real
    // implementations.
    // -----------------------------------------------------------------

    fn rs_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("failed to read directory {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("directory entry is readable").path();
            if path.is_dir() {
                rs_files_under(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn blobid_is_absent_from_core_and_ops_source() {
        let dirs = [
            concat!(env!("CARGO_MANIFEST_DIR"), "/../epiphany-core/src"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../epiphany-ops/src"),
        ];
        for dir in dirs {
            let mut files = Vec::new();
            rs_files_under(Path::new(dir), &mut files);
            assert!(
                !files.is_empty(),
                "expected to find .rs sources under {dir}"
            );
            for path in files {
                let source = fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
                assert!(
                    !source.contains("BlobId"),
                    "found `BlobId` in {}: `canonically_reachable_blob_ids` \
                     (crates/epiphany-textproj/src/project.rs) could provably return only the \
                     empty set because neither crate could name a BlobId -- that is no longer \
                     true, so both that predicate's emit side and parse.rs's `(blob ...)` \
                     rejection now need their real implementations",
                    path.display()
                );
            }
        }
    }
}
